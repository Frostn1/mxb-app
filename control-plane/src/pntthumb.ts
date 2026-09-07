/**
 * A picture of a synced paint, rendered from the `.pnt` itself.
 *
 * The control plane stores paints as opaque content-addressed blobs, which is the right
 * thing for syncing them and useless for looking at them: a page listing what we hold can
 * only show a digest and a file name, and neither says whether the object is the paint
 * somebody meant to publish. So the format is read here — the same walk `paint.rs` does on
 * the client, minus everything that isn't needed to draw one small square.
 *
 * Three things shape the implementation, all of them the same fact: a paint is up to 192 MB
 * and a single sheet inflates to 67 MB at 4096².
 *
 *   * The image table is walked with ranged reads. Every image header states its payload's
 *     length, so the table is reachable in a few hundred bytes rather than by downloading
 *     the file.
 *   * Only one texture is fetched, and it is inflated as a stream: rows are sampled as they
 *     arrive and the rest are dropped on the floor, so the isolate holds one chunk and a
 *     thumbnail, never a sheet.
 *   * The result is written back to R2 under its own key. The input is content-addressed, so
 *     the output is too, and a paint is decoded exactly once however many riders wear it.
 *
 * There is no image library in a Worker and none is added: PNG is a length-prefixed chunk
 * format over a zlib stream, and `CompressionStream("deflate")` is exactly that stream.
 */

const MAGIC = [0x50, 0x4e, 0x54, 0x00]; // "PNT\0"
const HEADER_SIZE = 108;
const NAME_SIZE = 100;
/** name + width + height + md5 + payload length. */
const IMAGE_HEADER_SIZE = NAME_SIZE + 4 + 4 + 16 + 4;
const IMAGE_PADDING = 8;

/** Ceilings on numbers read straight out of the file, before anything has checked them. */
const MAX_TEXTURES = 256;
/** Bigger than any sheet a paint carries, and small enough that one is quick to walk. */
const MAX_PIXELS = 8192 * 8192;

/** The edge of the square that comes out. Big enough to recognise a livery in a table row. */
export const THUMB_EDGE = 256;

/** Bumped when the rendering changes, so old squares are re-cut rather than served. */
const CACHE_VERSION = "v1";

export function thumbKey(sha256: string): string {
  return `thumb/${CACHE_VERSION}/${sha256}.png`;
}

/** Why there is no picture. Each reads as a word on the page, so they are the vocabulary. */
export type ThumbFailure = "missing" | "sealed" | "unreadable" | "empty";

class Unreadable extends Error {
  constructor(readonly reason: ThumbFailure) {
    super(reason);
  }
}

/**
 * Random access to a stored paint.
 *
 * An interface rather than an R2 bucket so the decoder can be driven from a byte array in a
 * test: everything below reads through this, and none of it knows what is on the other side.
 */
export interface PaintSource {
  size: number;
  read(offset: number, length: number): Promise<Uint8Array>;
  stream(offset: number, length: number): Promise<ReadableStream<Uint8Array>>;
}

export interface PntImage {
  name: string;
  width: number;
  height: number;
  /** The deflate payload's byte range within the whole `.pnt`. */
  start: number;
  end: number;
}

function u32(buf: Uint8Array, off: number): number {
  return (buf[off] | (buf[off + 1] << 8) | (buf[off + 2] << 16) | (buf[off + 3] << 24)) >>> 0;
}

function name(buf: Uint8Array, off: number): string {
  const raw = buf.subarray(off, off + NAME_SIZE);
  const end = raw.indexOf(0);
  return new TextDecoder().decode(end < 0 ? raw : raw.subarray(0, end));
}

/**
 * The images a paint carries, without inflating a pixel.
 *
 * A file that does not start with the magic is not corrupt: a paint from locked content is
 * a sealed container whose header only exists once it has been decrypted, which happens on
 * the client and never here. It reads as `sealed` rather than as an error.
 */
export async function imageTable(src: PaintSource): Promise<PntImage[]> {
  const head = await src.read(0, HEADER_SIZE);
  if (head.length < HEADER_SIZE) throw new Unreadable("unreadable");
  if (MAGIC.some((b, i) => head[i] !== b)) throw new Unreadable("sealed");

  const count = u32(head, HEADER_SIZE - 4);
  if (count === 0) throw new Unreadable("empty");

  const out: PntImage[] = [];
  let off = HEADER_SIZE;
  for (let i = 0; i < Math.min(count, MAX_TEXTURES); i++) {
    const h = await src.read(off, IMAGE_HEADER_SIZE);
    if (h.length < IMAGE_HEADER_SIZE) break;
    const dataSize = u32(h, NAME_SIZE + 8 + 16);
    if (dataSize < IMAGE_PADDING) break;

    const width = u32(h, NAME_SIZE);
    const height = u32(h, NAME_SIZE + 4);
    const start = off + IMAGE_HEADER_SIZE + IMAGE_PADDING;
    const end = off + IMAGE_HEADER_SIZE + dataSize;
    if (end > src.size) break;
    // Skipped rather than fatal: one header describing no real texture should cost that
    // texture, not the paint's picture.
    if (width > 0 && height > 0 && width * height <= MAX_PIXELS) {
      out.push({ name: name(h, 0), width, height, start, end });
    }
    off = end;
  }
  if (out.length === 0) throw new Unreadable("empty");
  return out;
}

/**
 * Which sheet stands for the paint.
 *
 * The largest one, which on every real paint is the body: a bike's plastics sheet is 2048²
 * or 4096² where its number plate and fonts are a quarter of that, and a helmet's shell
 * dwarfs its trim. `w_plate` is excluded outright — it is the game's own number plate, the
 * one surface an author never paints, so a paint whose largest sheet is that one would be
 * represented by the only picture that says nothing about it.
 */
export function pickImage(images: PntImage[]): PntImage {
  const area = (i: PntImage) => i.width * i.height;
  const painted = images.filter((i) => !/plate/i.test(i.name));
  const from = painted.length > 0 ? painted : images;
  return from.reduce((best, i) => (area(i) > area(best) ? i : best));
}

/**
 * Sample one texture down to a thumbnail, inflating it as a stream.
 *
 * Nearest-neighbour, because the alternative is holding rows either side of the sample and
 * this is a 64-pixel square in a table. Rows come out of the inflate in file order and are
 * written to their flipped destination as they pass: `.pnt` stores sheets the way the mesh
 * samples them, which is upside-down as a flat image, uniformly for bike and rider alike.
 */
export async function renderThumb(
  src: PaintSource,
  image: PntImage,
  edge = THUMB_EDGE,
): Promise<{ rgba: Uint8Array; width: number; height: number }> {
  const scale = Math.min(edge / image.width, edge / image.height, 1);
  const tw = Math.max(1, Math.round(image.width * scale));
  const th = Math.max(1, Math.round(image.height * scale));

  // Source row -> the output rows it fills, already flipped. Built up front so the streaming
  // pass only has to ask whether the row going past is one of them.
  const wanted = new Map<number, number[]>();
  for (let y = 0; y < th; y++) {
    const sy = Math.min(image.height - 1, Math.floor(((y + 0.5) * image.height) / th));
    const at = wanted.get(sy);
    if (at) at.push(th - 1 - y);
    else wanted.set(sy, [th - 1 - y]);
  }
  const cols = new Int32Array(tw);
  for (let x = 0; x < tw; x++) {
    cols[x] = Math.min(image.width - 1, Math.floor(((x + 0.5) * image.width) / tw)) * 4;
  }

  const out = new Uint8Array(tw * th * 4);
  const rowBytes = image.width * 4;
  const row = new Uint8Array(rowBytes);
  const body = await src.stream(image.start, image.end - image.start);
  // Raw deflate, no zlib wrapper — the client writes these with `flate2`'s `DeflateEncoder`.
  const reader = body.pipeThrough(new DecompressionStream("deflate-raw")).getReader();

  let sy = 0;
  let filled = 0;
  let left = wanted.size;
  try {
    while (left > 0) {
      const { value, done } = await reader.read();
      if (done) break;
      const chunk = value as Uint8Array;
      let off = 0;
      while (off < chunk.length && sy < image.height) {
        const targets = wanted.get(sy);
        const take = Math.min(rowBytes - filled, chunk.length - off);
        // Rows nobody sampled are skipped, not copied: at 4096² that is 4000 rows of 16 KB
        // the isolate never touches.
        if (targets) row.set(chunk.subarray(off, off + take), filled);
        filled += take;
        off += take;
        if (filled === rowBytes) {
          if (targets) {
            for (const y of targets) {
              const dst = y * tw * 4;
              for (let x = 0; x < tw; x++) {
                const s = cols[x];
                const d = dst + x * 4;
                out[d] = row[s];
                out[d + 1] = row[s + 1];
                out[d + 2] = row[s + 2];
                out[d + 3] = row[s + 3];
              }
            }
            left--;
          }
          filled = 0;
          sy++;
        }
      }
    }
  } catch (err) {
    // A payload that is not deflate, or that stops mid-stream, fails inside the inflate with
    // whatever the runtime calls that. It is the same fact as a short one — the file does not
    // hold the texture its header describes — so it leaves here saying so.
    if (err instanceof Unreadable) throw err;
    throw new Unreadable("unreadable");
  } finally {
    // The last sampled row is usually well before the end of a 67 MB plane; nothing is
    // gained by inflating the rest.
    await reader.cancel().catch(() => {});
  }
  if (left > 0) throw new Unreadable("unreadable");
  return { rgba: out, width: tw, height: th };
}

// ---------------------------------------------------------------------------
// PNG
// ---------------------------------------------------------------------------

let crcTable: Uint32Array | null = null;

function crc32(bytes: Uint8Array): number {
  if (!crcTable) {
    crcTable = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[n] = c >>> 0;
    }
  }
  let c = 0xffffffff;
  for (const b of bytes) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

async function zlib(data: Uint8Array): Promise<Uint8Array> {
  // `deflate` rather than `deflate-raw`: PNG's IDAT is a zlib stream, header and Adler-32
  // included, which is precisely what this produces.
  const cs = new CompressionStream("deflate");
  const writer = cs.writable.getWriter();
  void writer.write(data).then(() => writer.close());
  const parts: Uint8Array[] = [];
  const reader = cs.readable.getReader();
  let total = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    parts.push(value as Uint8Array);
    total += (value as Uint8Array).length;
  }
  const out = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** 8-bit RGBA, no interlace, every scanline unfiltered. */
export async function encodePng(
  rgba: Uint8Array,
  width: number,
  height: number,
): Promise<Uint8Array> {
  const stride = width * 4;
  const raw = new Uint8Array(height * (stride + 1));
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    raw.set(rgba.subarray(y * stride, (y + 1) * stride), y * (stride + 1) + 1);
  }

  const ihdr = new Uint8Array(13);
  const view = new DataView(ihdr.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // colour type: truecolour with alpha
  const parts = [
    new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", await zlib(raw)),
    chunk("IEND", new Uint8Array(0)),
  ];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Serving
// ---------------------------------------------------------------------------

function r2Source(env: Env, key: string, size: number): PaintSource {
  return {
    size,
    async read(offset, length) {
      const object = await env.PAINTS.get(key, { range: { offset, length } });
      if (!object) throw new Unreadable("missing");
      return new Uint8Array(await object.arrayBuffer());
    },
    async stream(offset, length) {
      const object = await env.PAINTS.get(key, { range: { offset, length } });
      if (!object?.body) throw new Unreadable("missing");
      return object.body;
    },
  };
}

/**
 * `GET /admin/paints/thumb?sha=…` — the square, cut once and kept.
 *
 * A paint that cannot be drawn answers with a labelled tile rather than a 404, because the
 * caller is an `<img>` in a table: a broken-image icon says the page is wrong, where the word
 * "sealed" says the paint is locked content and everything is working.
 */
export async function paintThumb(sha256: string, env: Env): Promise<Response> {
  if (!/^[0-9a-f]{64}$/.test(sha256)) return placeholder("unreadable");

  const cached = await env.PAINTS.get(thumbKey(sha256));
  if (cached) return png(cached.body);

  let rendered: Uint8Array;
  try {
    const head = await env.PAINTS.head(sha256);
    if (!head) return placeholder("missing");
    const src = r2Source(env, sha256, head.size);
    const image = pickImage(await imageTable(src));
    const thumb = await renderThumb(src, image);
    rendered = await encodePng(thumb.rgba, thumb.width, thumb.height);
  } catch (err) {
    const reason = err instanceof Unreadable ? err.reason : "unreadable";
    if (reason === "unreadable") {
      console.error(JSON.stringify({ msg: "thumbnail failed", sha256, error: String(err) }));
    }
    return placeholder(reason);
  }

  // Stored, not just returned: the decode is the expensive half and the input is immutable,
  // so the next viewer — and every other rider wearing this paint — reads one object.
  await env.PAINTS.put(thumbKey(sha256), rendered).catch(() => {});
  return png(rendered);
}

function png(body: BodyInit): Response {
  return new Response(body, {
    headers: {
      "content-type": "image/png",
      // Content-addressed, so it can never mean anything else — but behind an admin key,
      // so no shared cache is invited to keep a copy.
      "cache-control": "private, max-age=86400, immutable",
    },
  });
}

/** A tile that says why, in the palette of the page it sits on. */
function placeholder(reason: ThumbFailure): Response {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">` +
    `<rect width="64" height="64" fill="#9aa3ae" fill-opacity=".16"/>` +
    `<text x="32" y="35" text-anchor="middle" font-family="ui-sans-serif,system-ui,sans-serif"` +
    ` font-size="9" fill="#9aa3ae">${reason}</text></svg>`;
  return new Response(svg, {
    headers: {
      "content-type": "image/svg+xml; charset=utf-8",
      // Short: a paint that is missing today may have been uploaded by this afternoon.
      "cache-control": "private, max-age=300",
    },
  });
}
