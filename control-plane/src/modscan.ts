/**
 * The checks an uploaded mod passes before anyone can download it.
 *
 * MX Bikes content is data: textures, models, sounds, configs — packed as `.pkz` (a zip),
 * shipped as `.zip`, or a bare `.pnt` paint. Nothing a mod needs is executable, so the rule is
 * simple: no program, script or library anywhere inside, by name or by content, and nothing
 * whose inside can't be seen.
 *
 *  - The archive's central directory is read first (ranged reads): every entry name is checked
 *    for traversal (`../`, absolute, drive letters), forbidden extensions, encryption, and
 *    archive formats that can't be looked into (rar, 7z, …).
 *  - Then one streaming pass over the bytes, which is also the pass that hashes the file: the
 *    first bytes of every entry are inflated and checked for executable signatures (PE, ELF,
 *    Mach-O, `#!`), so a renamed `.exe` is caught too. A `.zip`/`.pkz` inside the archive is
 *    inflated as it streams and scanned the same way, one level deep; deeper nesting is refused.
 *
 * Every refusal names the entry, so the uploader knows what to take out.
 */

const FORBIDDEN_EXT = new Set([
  "exe", "dll", "sys", "scr", "com", "bat", "cmd", "ps1", "psm1", "psd1", "vbs", "vbe", "js", "jse",
  "wsf", "wsh", "msi", "msp", "msix", "appx", "jar", "lnk", "hta", "cpl", "ocx", "asi", "so",
  "dylib", "sh", "app", "reg", "inf", "scf", "pif", "drv", "efi", "dmg", "pkg", "deb", "rpm",
]);
/** Archive formats we can't look inside. */
const OPAQUE_EXT = new Set(["rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "cab", "iso", "img", "zst", "lzh", "arj"]);
const NESTED_EXT = new Set(["zip", "pkz"]);
const HEAD_BYTES = 4096;
const MAX_ENTRIES = 50_000;
const MAX_CD_BYTES = 32 * 1024 * 1024;
export const MAX_PNT_BYTES = 64 * 1024 * 1024;

export class ScanError extends Error {}

export interface CdEntry {
  name: string;
  method: number;
  flags: number;
  compSize: number;
  size: number;
  localOffset: number;
}

const u16 = (b: Uint8Array, o: number) => b[o] | (b[o + 1] << 8);
const u32 = (b: Uint8Array, o: number) => (b[o] | (b[o + 1] << 8) | (b[o + 2] << 16) | (b[o + 3] << 24)) >>> 0;
const u64 = (b: Uint8Array, o: number) => u32(b, o) + u32(b, o + 4) * 2 ** 32;

export function extOf(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/** A name an extractor could be tricked by, or that we refuse to carry. `null` when fine. */
export function badName(name: string): string | null {
  const n = name.replace(/\\/g, "/");
  if (n.startsWith("/") || /^[a-z]:/i.test(n)) return `absolute path: ${name}`;
  if (n.split("/").some((seg) => seg === "..")) return `path escapes the archive: ${name}`;
  if (/[\u0000-\u001f]/.test(n)) return `control character in a name: ${JSON.stringify(name)}`;
  const ext = extOf(n);
  if (FORBIDDEN_EXT.has(ext)) return `executable or script inside: ${name}`;
  if (OPAQUE_EXT.has(ext)) return `an archive that can't be checked (.${ext}) inside: ${name}`;
  return null;
}

/** Bytes that start a program. `null` when they don't. */
export function executableMagic(head: Uint8Array): string | null {
  if (head.length >= 2 && head[0] === 0x4d && head[1] === 0x5a) {
    // MZ alone is two common letters; a real PE also has "PE\0\0" where e_lfanew points.
    if (head.length >= 0x40) {
      const pe = u32(head, 0x3c);
      if (pe + 4 <= head.length && head[pe] === 0x50 && head[pe + 1] === 0x45 && head[pe + 2] === 0 && head[pe + 3] === 0)
        return "a Windows program (PE)";
      if (pe + 4 > head.length && pe < 0x10000) return "a Windows program (MZ)";
    }
  }
  if (head.length >= 4 && head[0] === 0x7f && head[1] === 0x45 && head[2] === 0x4c && head[3] === 0x46) return "an ELF program";
  if (head.length >= 4) {
    const m = u32(head, 0);
    if ([0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe, 0xbebafeca].includes(m)) return "a macOS program";
  }
  if (head.length >= 2 && head[0] === 0x23 && head[1] === 0x21) return "a script (#!)";
  return null;
}

/** Read the central directory through ranged reads. Throws `ScanError` on anything malformed. */
export async function readCentralDirectory(
  read: (offset: number, length: number) => Promise<Uint8Array>,
  size: number,
): Promise<CdEntry[]> {
  if (size < 22) throw new ScanError("not a zip archive (too small)");
  const tailLen = Math.min(size, 22 + 65535);
  const tail = await read(size - tailLen, tailLen);
  let eocd = -1;
  for (let i = tail.length - 22; i >= 0; i--) {
    if (u32(tail, i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new ScanError("not a zip archive (no end of central directory)");
  let entries = u16(tail, eocd + 10);
  let cdSize = u32(tail, eocd + 12);
  let cdOffset = u32(tail, eocd + 16);
  if (entries === 0xffff || cdSize === 0xffffffff || cdOffset === 0xffffffff) {
    // Zip64: the locator sits just before the EOCD and points at the real record.
    const loc = eocd - 20;
    if (loc < 0 || u32(tail, loc) !== 0x07064b50) throw new ScanError("broken zip64 archive");
    const recOff = u64(tail, loc + 8);
    if (recOff + 56 > size) throw new ScanError("broken zip64 archive");
    const rec = await read(recOff, 56);
    if (u32(rec, 0) !== 0x06064b50) throw new ScanError("broken zip64 archive");
    entries = u64(rec, 32);
    cdSize = u64(rec, 40);
    cdOffset = u64(rec, 48);
  }
  if (entries === 0) throw new ScanError("the archive is empty");
  if (entries > MAX_ENTRIES) throw new ScanError(`too many files in the archive (${entries})`);
  if (cdSize > MAX_CD_BYTES || cdOffset + cdSize > size) throw new ScanError("the archive's directory is malformed");
  const cd = await read(cdOffset, cdSize);
  const out: CdEntry[] = [];
  let p = 0;
  const dec = new TextDecoder();
  for (let i = 0; i < entries; i++) {
    if (p + 46 > cd.length || u32(cd, p) !== 0x02014b50) throw new ScanError("the archive's directory is malformed");
    const flags = u16(cd, p + 8);
    const method = u16(cd, p + 10);
    let compSize = u32(cd, p + 20);
    let usize = u32(cd, p + 24);
    const nameLen = u16(cd, p + 28);
    const extraLen = u16(cd, p + 30);
    const commentLen = u16(cd, p + 32);
    let localOffset = u32(cd, p + 42);
    const name = dec.decode(cd.subarray(p + 46, p + 46 + nameLen));
    // Zip64 extended information: the fields that overflowed, in order.
    let x = p + 46 + nameLen;
    const xEnd = x + extraLen;
    while (x + 4 <= xEnd) {
      const id = u16(cd, x);
      const len = u16(cd, x + 2);
      if (id === 0x0001) {
        let q = x + 4;
        if (usize === 0xffffffff) (usize = u64(cd, q)), (q += 8);
        if (compSize === 0xffffffff) (compSize = u64(cd, q)), (q += 8);
        if (localOffset === 0xffffffff) localOffset = u64(cd, q);
      }
      x += 4 + len;
    }
    p += 46 + nameLen + extraLen + commentLen;
    if (localOffset >= size) throw new ScanError(`an entry points outside the archive: ${name}`);
    out.push({ name, method, flags, compSize, size: usize, localOffset });
  }
  return out;
}

/** Name-level checks over a central directory. */
export function checkEntries(entries: CdEntry[]): void {
  for (const e of entries) {
    const bad = badName(e.name);
    if (bad) throw new ScanError(bad);
    if (e.flags & 0x1) throw new ScanError(`encrypted entry, can't be checked: ${e.name}`);
    if (e.method !== 0 && e.method !== 8 && !e.name.endsWith("/"))
      throw new ScanError(`unsupported compression (${e.method}): ${e.name}`);
  }
}

/** The first bytes an entry's data inflates to. */
export async function inflateHead(data: Uint8Array, method: number, want = HEAD_BYTES): Promise<Uint8Array> {
  if (method === 0) return data.subarray(0, want);
  const ds = new DecompressionStream("deflate-raw");
  const w = ds.writable.getWriter();
  const r = ds.readable.getReader();
  void w.write(data).catch(() => {});
  void w.close().catch(() => {});
  const out = new Uint8Array(want);
  let n = 0;
  try {
    while (n < want) {
      const { done, value } = await r.read();
      if (done || !value) break;
      const take = Math.min(value.length, want - n);
      out.set(value.subarray(0, take), n);
      n += take;
    }
  } catch {
    // A truncated stream (we only fed the head) ends in an error once it has given what it can.
  }
  await r.cancel().catch(() => {});
  return out.subarray(0, n);
}

interface Head {
  start: number;
  end: number;
  name: string;
  method: number;
  buf: Uint8Array;
  fill: number;
}

interface Nested {
  start: number;
  end: number;
  /** How far the nested stream has been fed, so bytes seen twice are written once. */
  written: number;
  writer: WritableStreamDefaultWriter<Uint8Array>;
  done: Promise<void>;
}

/**
 * Scans zip bytes as they stream past, for executables by content, and descends into nested
 * `.zip`/`.pkz` entries one level. Feed it every chunk in order, then `finish()`.
 *
 * At the top level the central directory says where each local header is, so a stray header
 * signature inside compressed data is never mistaken for one. Inside a nested archive there
 * is no directory at hand, so local headers are found by their signature.
 */
export class ZipStreamScanner {
  private pos = 0;
  /** Bytes kept from the end of the last chunk, so a header split across chunks is still read. */
  private carry = new Uint8Array(0);
  private readonly offsets: number[] | null;
  private readonly byOffset: Map<number, CdEntry> | null;
  private next = 0;
  private heads: Head[] = [];
  private nested: Nested | null = null;
  private error: ScanError | null = null;

  constructor(
    entries: CdEntry[] | null,
    private readonly depth = 0,
  ) {
    this.byOffset = entries ? new Map(entries.map((e) => [e.localOffset, e])) : null;
    this.offsets = entries ? [...new Set(entries.map((e) => e.localOffset))].sort((x, y) => x - y) : null;
  }

  get failed(): ScanError | null {
    return this.error;
  }

  async push(chunk: Uint8Array): Promise<void> {
    if (this.error || chunk.length === 0) return;
    const chunkStart = this.pos;
    this.pos += chunk.length;
    await this.feed(chunk, chunkStart);
    if (this.error) return;

    const buf = this.carry.length ? concat(this.carry, chunk) : chunk;
    const base = chunkStart - this.carry.length;
    let resume = buf.length;
    if (this.offsets) {
      while (this.next < this.offsets.length) {
        const at = this.offsets[this.next];
        if (at < base) {
          this.next++;
          continue;
        }
        const i = at - base;
        if (i + 30 > buf.length || i + 30 + u16(buf, i + 26) > buf.length) {
          resume = i;
          break;
        }
        this.next++;
        if (u32(buf, i) !== 0x04034b50) {
          this.error = new ScanError("the archive's entries don't match its directory");
          return;
        }
        await this.header(at, buf, i, base);
        if (this.error) return;
      }
    } else {
      let i = 0;
      for (;;) {
        i = buf.indexOf(0x50, i);
        if (i < 0) break;
        if (i + 4 > buf.length) {
          resume = i;
          break;
        }
        if (buf[i + 1] !== 0x4b || buf[i + 2] !== 0x03 || buf[i + 3] !== 0x04) {
          i++;
          continue;
        }
        if (i + 30 > buf.length || i + 30 + u16(buf, i + 26) > buf.length) {
          resume = i;
          break;
        }
        await this.header(base + i, buf, i, base);
        if (this.error) return;
        i += 4;
      }
    }
    // Keep from where a header was cut off; otherwise only what a split signature could need.
    const keepFrom = Math.min(resume, Math.max(0, buf.length - 3));
    this.carry = buf.slice(keepFrom);
  }

  private async header(at: number, buf: Uint8Array, i: number, base: number): Promise<void> {
    const nameLen = u16(buf, i + 26);
    const extraLen = u16(buf, i + 28);
    let name: string;
    let method: number;
    let compSize: number;
    const cd = this.byOffset?.get(at);
    if (cd) {
      ({ name, method, compSize } = cd);
    } else {
      name = new TextDecoder().decode(buf.subarray(i + 30, i + 30 + nameLen));
      const bad = badName(name);
      if (bad) {
        this.error = new ScanError(`${bad} (in a nested archive)`);
        return;
      }
      if (u16(buf, i + 6) & 0x1) {
        this.error = new ScanError(`encrypted entry in a nested archive: ${name}`);
        return;
      }
      method = u16(buf, i + 8);
      compSize = u32(buf, i + 18);
    }
    if (name.endsWith("/")) return;
    const dataStart = at + 30 + nameLen + extraLen;
    if (NESTED_EXT.has(extOf(name))) {
      if (this.depth >= 1) {
        this.error = new ScanError(`an archive inside an archive inside an archive: ${name}`);
        return;
      }
      // Only the top level knows a nested archive's exact length (from the directory).
      if (cd && compSize > 0 && (method === 0 || method === 8)) {
        await this.closeNested();
        this.startNested(dataStart, dataStart + compSize, method, name);
      }
    }
    const capture = compSize > 0 ? Math.min(HEAD_BYTES, compSize) : HEAD_BYTES;
    this.heads.push({ start: dataStart, end: dataStart + capture, name, method, buf: new Uint8Array(capture), fill: 0 });
    // Whatever of its data is already in hand.
    await this.feed(buf, base);
  }

  private startNested(start: number, end: number, method: number, name: string): void {
    const inner = new ZipStreamScanner(null, this.depth + 1);
    const raw = new TransformStream<Uint8Array, Uint8Array>();
    const stream = method === 8 ? raw.readable.pipeThrough(new DecompressionStream("deflate-raw")) : raw.readable;
    const done = (async () => {
      const r = stream.getReader();
      try {
        for (;;) {
          const { done: over, value } = await r.read();
          if (over) break;
          if (value) await inner.push(value);
          if (inner.failed) break;
        }
        if (!inner.failed) await inner.finish();
      } catch {
        this.error ??= new ScanError(`a nested archive is corrupt: ${name}`);
      }
      await r.cancel().catch(() => {});
      if (inner.failed) this.error ??= inner.failed;
    })();
    this.nested = { start, end, written: start, writer: raw.writable.getWriter(), done };
  }

  private async closeNested(): Promise<void> {
    if (!this.nested) return;
    const n = this.nested;
    this.nested = null;
    await n.writer.close().catch(() => {});
    await n.done;
  }

  /** Hand the bytes of [chunkStart, chunkStart + chunk.length) to whatever is waiting on them. */
  private async feed(chunk: Uint8Array, chunkStart: number): Promise<void> {
    const chunkEnd = chunkStart + chunk.length;
    const n = this.nested;
    if (n) {
      const from = Math.max(n.written, chunkStart);
      const to = Math.min(n.end, chunkEnd);
      if (to > from) {
        // Not awaited past the write itself: the reader side runs alongside.
        await n.writer.write(chunk.slice(from - chunkStart, to - chunkStart)).catch(() => {});
        n.written = to;
      }
      if (n.written >= n.end) await this.closeNested();
    }
    const still: Head[] = [];
    for (const h of this.heads) {
      const from = Math.max(h.start + h.fill, chunkStart);
      const to = Math.min(h.end, chunkEnd);
      if (to > from) {
        h.buf.set(chunk.subarray(from - chunkStart, to - chunkStart), from - h.start);
        h.fill = to - h.start;
      }
      if (h.fill >= h.end - h.start) {
        await this.sniff(h.name, h.method, h.buf.subarray(0, h.fill));
        if (this.error) return;
      } else still.push(h);
    }
    this.heads = still;
  }

  private async sniff(name: string, method: number, data: Uint8Array): Promise<void> {
    const what = executableMagic(await inflateHead(data, method));
    if (what) this.error = new ScanError(`${name} is ${what}`);
  }

  async finish(): Promise<void> {
    for (const h of this.heads) {
      if (this.error) break;
      if (h.fill > 0) await this.sniff(h.name, h.method, h.buf.subarray(0, h.fill));
    }
    this.heads = [];
    await this.closeNested();
  }
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
}

/** A bare `.pnt`: small, and not a program or an archive dressed as a paint. */
export function checkPntHead(head: Uint8Array, size: number): void {
  if (size > MAX_PNT_BYTES) throw new ScanError(`a paint can be at most ${MAX_PNT_BYTES} bytes`);
  const what = executableMagic(head);
  if (what) throw new ScanError(`the paint is ${what}`);
  if (head.length >= 4 && u32(head, 0) === 0x04034b50) throw new ScanError("the paint is a zip archive; upload it as .zip");
}
