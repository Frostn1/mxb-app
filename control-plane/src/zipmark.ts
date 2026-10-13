/**
 * Marks a zip with the order it was sold under, on the way out of the bucket.
 *
 * The stored zip is never rewritten. The response is the stored bytes up to the central
 * directory, then one extra file (`ORDER.txt`, stored, not compressed), then the original
 * central directory with an entry for that file, then a new end record whose comment names the
 * order. Every byte of the original entries is streamed as it is, so a 3 GB pack costs no memory.
 *
 * A Zip64 archive (over 4 GB, or over 65535 files) only gets the comment: adding an entry would
 * mean rewriting the Zip64 records too, and nothing we sell is that large.
 */

const EOCD_SIG = 0x06054b50;
const ZIP64_LOCATOR_SIG = 0x07064b50;
const EOCD_LEN = 22;
const MAX_COMMENT = 0xffff;

export interface ZipSource {
  /** Total size of the stored object. */
  size: number;
  /** A byte range of it. */
  range(offset: number, length: number): Promise<ReadableStream<Uint8Array> | Uint8Array>;
}

export interface ZipEnd {
  /** Where the end record starts. */
  eocdAt: number;
  entries: number;
  cdSize: number;
  cdOffset: number;
  zip64: boolean;
}

/** Finds and reads the end record in the tail of the file. */
export function readEnd(tail: Uint8Array, tailStart: number): ZipEnd | null {
  const v = new DataView(tail.buffer, tail.byteOffset, tail.byteLength);
  for (let i = tail.length - EOCD_LEN; i >= 0; i--) {
    if (v.getUint32(i, true) !== EOCD_SIG) continue;
    const commentLen = v.getUint16(i + 20, true);
    if (i + EOCD_LEN + commentLen !== tail.length) continue;
    const entries = v.getUint16(i + 10, true);
    const cdSize = v.getUint32(i + 12, true);
    const cdOffset = v.getUint32(i + 16, true);
    const zip64 =
      (i >= 20 && v.getUint32(i - 20, true) === ZIP64_LOCATOR_SIG) || entries === 0xffff || cdSize === 0xffffffff || cdOffset === 0xffffffff;
    return { eocdAt: tailStart + i, entries, cdSize, cdOffset, zip64 };
  }
  return null;
}

let CRC_TABLE: Uint32Array | null = null;

export function crc32(bytes: Uint8Array): number {
  if (!CRC_TABLE) {
    CRC_TABLE = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      CRC_TABLE[n] = c >>> 0;
    }
  }
  let crc = 0xffffffff;
  for (const b of bytes) crc = CRC_TABLE[(crc ^ b) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function dosTime(d: Date): { time: number; date: number } {
  return {
    time: (d.getUTCHours() << 11) | (d.getUTCMinutes() << 5) | Math.floor(d.getUTCSeconds() / 2),
    date: ((d.getUTCFullYear() - 1980) << 9) | ((d.getUTCMonth() + 1) << 5) | d.getUTCDate(),
  };
}

/** The extra entry: its local header with data, and its central directory record. */
export function orderEntry(name: string, text: string, offset: number, at: Date): { local: Uint8Array; central: Uint8Array } {
  const nameBytes = new TextEncoder().encode(name);
  const data = new TextEncoder().encode(text);
  const crc = crc32(data);
  const { time, date } = dosTime(at);

  const local = new Uint8Array(30 + nameBytes.length + data.length);
  const l = new DataView(local.buffer);
  l.setUint32(0, 0x04034b50, true);
  l.setUint16(4, 20, true); // version needed
  l.setUint16(6, 0x0800, true); // UTF-8 names
  l.setUint16(8, 0, true); // stored
  l.setUint16(10, time, true);
  l.setUint16(12, date, true);
  l.setUint32(14, crc, true);
  l.setUint32(18, data.length, true);
  l.setUint32(22, data.length, true);
  l.setUint16(26, nameBytes.length, true);
  l.setUint16(28, 0, true);
  local.set(nameBytes, 30);
  local.set(data, 30 + nameBytes.length);

  const central = new Uint8Array(46 + nameBytes.length);
  const c = new DataView(central.buffer);
  c.setUint32(0, 0x02014b50, true);
  c.setUint16(4, 20, true); // made by
  c.setUint16(6, 20, true); // needed
  c.setUint16(8, 0x0800, true);
  c.setUint16(10, 0, true);
  c.setUint16(12, time, true);
  c.setUint16(14, date, true);
  c.setUint32(16, crc, true);
  c.setUint32(20, data.length, true);
  c.setUint32(24, data.length, true);
  c.setUint16(28, nameBytes.length, true);
  c.setUint16(30, 0, true); // extra
  c.setUint16(32, 0, true); // comment
  c.setUint16(34, 0, true); // disk
  c.setUint16(36, 0, true); // internal attrs
  c.setUint32(38, 0, true); // external attrs
  c.setUint32(42, offset, true);
  central.set(nameBytes, 46);
  return { local, central };
}

/** An end record with the given counts and comment. */
export function endRecord(entries: number, cdSize: number, cdOffset: number, comment: string): Uint8Array {
  const text = new TextEncoder().encode(comment).subarray(0, MAX_COMMENT);
  const out = new Uint8Array(EOCD_LEN + text.length);
  const v = new DataView(out.buffer);
  v.setUint32(0, EOCD_SIG, true);
  v.setUint16(4, 0, true);
  v.setUint16(6, 0, true);
  v.setUint16(8, entries, true);
  v.setUint16(10, entries, true);
  v.setUint32(12, cdSize, true);
  v.setUint32(16, cdOffset, true);
  v.setUint16(20, text.length, true);
  out.set(text, EOCD_LEN);
  return out;
}

async function toBytes(part: ReadableStream<Uint8Array> | Uint8Array): Promise<Uint8Array> {
  if (part instanceof Uint8Array) return part;
  return new Uint8Array(await new Response(part).arrayBuffer());
}

type Piece = Uint8Array | (() => Promise<ReadableStream<Uint8Array> | Uint8Array>);

function concat(pieces: Piece[]): ReadableStream<Uint8Array> {
  let i = 0;
  let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  return new ReadableStream<Uint8Array>({
    async pull(controller) {
      for (;;) {
        if (reader) {
          const { done, value } = await reader.read();
          if (!done) {
            controller.enqueue(value);
            return;
          }
          reader = null;
        }
        if (i >= pieces.length) {
          controller.close();
          return;
        }
        const p = pieces[i++];
        if (p instanceof Uint8Array) {
          if (p.length) {
            controller.enqueue(p);
            return;
          }
          continue;
        }
        const got = await p();
        if (got instanceof Uint8Array) {
          if (got.length) {
            controller.enqueue(got);
            return;
          }
          continue;
        }
        reader = got.getReader();
      }
    },
    cancel() {
      void reader?.cancel();
    },
  });
}

export interface Marked {
  body: ReadableStream<Uint8Array>;
  length: number;
}

/**
 * The marked zip as a stream, and its exact length. Null when the object isn't a zip this can
 * read (no end record), so the caller can refuse rather than send something broken.
 */
export async function markZip(src: ZipSource, mark: { comment: string; name: string; text: string; at: Date }): Promise<Marked | null> {
  if (src.size < EOCD_LEN) return null;
  const tailLen = Math.min(src.size, EOCD_LEN + MAX_COMMENT);
  const tailStart = src.size - tailLen;
  const end = readEnd(await toBytes(await src.range(tailStart, tailLen)), tailStart);
  if (!end) return null;
  const comment = endRecord(0, 0, 0, mark.comment).subarray(EOCD_LEN);

  if (end.zip64) {
    // Everything up to the old end record, then the same record with the new comment.
    const head = await toBytes(await src.range(end.eocdAt, EOCD_LEN));
    const eocd = new Uint8Array(EOCD_LEN + comment.length);
    eocd.set(head);
    new DataView(eocd.buffer).setUint16(20, comment.length, true);
    eocd.set(comment, EOCD_LEN);
    return {
      body: concat([() => src.range(0, end.eocdAt), eocd]),
      length: end.eocdAt + eocd.length,
    };
  }

  if (end.cdOffset + end.cdSize > end.eocdAt) return null;
  const entry = orderEntry(mark.name, mark.text, end.cdOffset, mark.at);
  const cdOffset = end.cdOffset + entry.local.length;
  const cdSize = end.cdSize + entry.central.length;
  const eocd = endRecord(end.entries + 1, cdSize, cdOffset, mark.comment);
  return {
    body: concat([
      () => src.range(0, end.cdOffset),
      entry.local,
      () => src.range(end.cdOffset, end.cdSize),
      entry.central,
      eocd,
    ]),
    length: cdOffset + cdSize + eocd.length,
  };
}
