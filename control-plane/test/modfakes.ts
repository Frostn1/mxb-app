/**
 * Stand-ins for the parts of the runtime the mod catalogue's tests need: an R2 bucket that
 * keeps multipart uploads and ranges honest, a queue that records what it was sent, a SHA-256
 * that works outside a Worker, and a zip writer for building test archives.
 */

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import type { Hasher } from "../src/mirrorfetch";
import { readPageJobs, type MirrorJob, type PageBatchResult } from "../src/mirror";

const FIXTURES = join(dirname(fileURLToPath(import.meta.url)), "fixtures", "hosts");

export function fixture(name: string): string {
  return readFileSync(join(FIXTURES, name), "utf8");
}

/** Node's SHA-256, wearing the `Hasher` the Worker gets from `crypto.DigestStream`. */
export function nodeHasher(): Hasher {
  const h = createHash("sha256");
  return {
    update: (c) => void h.update(c),
    digest: async () => h.digest("hex"),
  };
}

export async function sha256(bytes: Uint8Array): Promise<string> {
  return [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map((b) => b.toString(16).padStart(2, "0")).join("");
}

interface Stored {
  bytes: Uint8Array;
  httpMetadata?: R2HTTPMetadata;
}

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) out.set(p, (o += p.length) - p.length);
  return out;
}

function body(bytes: Uint8Array, meta?: R2HTTPMetadata) {
  return {
    size: bytes.length,
    httpMetadata: meta,
    httpEtag: '"etag"',
    body: new Blob([bytes]).stream(),
    arrayBuffer: async () => bytes.slice().buffer,
  };
}

/** An in-memory R2 bucket: put/get(range)/head/delete and multipart uploads. */
export function fakeBucket() {
  const objects = new Map<string, Stored>();
  const uploads = new Map<string, { key: string; parts: Map<number, Uint8Array>; meta?: R2HTTPMetadata }>();
  let seq = 0;
  const bucket = {
    objects,
    uploads,
    partSizes: [] as number[],
    async put(key: string, value: ArrayBuffer | Uint8Array | string, opts?: { httpMetadata?: R2HTTPMetadata }) {
      const bytes = typeof value === "string" ? new TextEncoder().encode(value) : new Uint8Array(value as ArrayBuffer);
      objects.set(key, { bytes, httpMetadata: opts?.httpMetadata });
      return { key, size: bytes.length };
    },
    async get(key: string, opts?: { range?: { offset: number; length: number } }) {
      const o = objects.get(key);
      if (!o) return null;
      const bytes = opts?.range ? o.bytes.subarray(opts.range.offset, opts.range.offset + opts.range.length) : o.bytes;
      return body(bytes, o.httpMetadata);
    },
    async head(key: string) {
      const o = objects.get(key);
      return o ? { size: o.bytes.length, httpMetadata: o.httpMetadata } : null;
    },
    async delete(keys: string | string[]) {
      for (const k of Array.isArray(keys) ? keys : [keys]) objects.delete(k);
    },
    async createMultipartUpload(key: string, opts?: { httpMetadata?: R2HTTPMetadata }) {
      const uploadId = `up${++seq}`;
      uploads.set(uploadId, { key, parts: new Map(), meta: opts?.httpMetadata });
      return bucket.resumeMultipartUpload(key, uploadId);
    },
    resumeMultipartUpload(key: string, uploadId: string) {
      return {
        key,
        uploadId,
        async uploadPart(n: number, value: Uint8Array | ArrayBuffer) {
          const u = uploads.get(uploadId);
          if (!u) throw new Error("no such upload");
          const bytes = new Uint8Array(value as ArrayBuffer).slice();
          u.parts.set(n, bytes);
          bucket.partSizes.push(bytes.length);
          return { partNumber: n, etag: `etag-${n}` };
        },
        async complete(parts: { partNumber: number; etag: string }[]) {
          const u = uploads.get(uploadId);
          if (!u) throw new Error("no such upload");
          const bytes = concat(parts.map((p) => u.parts.get(p.partNumber)!));
          objects.set(u.key, { bytes, httpMetadata: u.meta });
          uploads.delete(uploadId);
          return { key: u.key, size: bytes.length };
        },
        async abort() {
          uploads.delete(uploadId);
        },
      };
    },
  };
  return bucket;
}

export function fakeQueue<T>() {
  const sent: T[] = [];
  return {
    sent,
    async send(body: T) {
      sent.push(body);
    },
    async sendBatch(msgs: { body: T }[]) {
      for (const m of msgs) sent.push(m.body);
    },
  };
}

/** A fetch answering from a table of URL → response makers, and recording every request. */
export function fakeFetch(routes: [RegExp | string, (req: Request) => Response | Promise<Response>][]) {
  const calls: string[] = [];
  const f = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const req = new Request(input instanceof URL ? input.toString() : input, init);
    calls.push(`${req.method} ${req.url}`);
    for (const [pat, make] of routes) {
      if (typeof pat === "string" ? req.url === pat : pat.test(req.url)) return make(req);
    }
    return new Response("not found", { status: 404 });
  }) as typeof fetch;
  return Object.assign(f, { calls });
}

// ───────────────────────────── a zip writer ─────────────────────────────

export interface ZipEntry {
  name: string;
  data: Uint8Array | string;
  /** 0 stored, 8 deflated. */
  method?: 0 | 8;
  encrypted?: boolean;
}

async function deflateRaw(data: Uint8Array): Promise<Uint8Array> {
  const cs = new CompressionStream("deflate-raw");
  const out = new Response(new Blob([data]).stream().pipeThrough(cs));
  return new Uint8Array(await out.arrayBuffer());
}

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(b: Uint8Array): number {
  let c = 0xffffffff;
  for (const x of b) c = CRC_TABLE[(c ^ x) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** A plain zip (no zip64), enough to exercise the scanner. */
export async function makeZip(entries: ZipEntry[]): Promise<Uint8Array> {
  const enc = new TextEncoder();
  const locals: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  for (const e of entries) {
    const raw = typeof e.data === "string" ? enc.encode(e.data) : e.data;
    const method = e.method ?? 8;
    const comp = method === 8 ? await deflateRaw(raw) : raw;
    const name = enc.encode(e.name);
    const crc = crc32(raw);
    const flags = e.encrypted ? 1 : 0;
    const lh = new DataView(new ArrayBuffer(30));
    lh.setUint32(0, 0x04034b50, true);
    lh.setUint16(4, 20, true);
    lh.setUint16(6, flags, true);
    lh.setUint16(8, method, true);
    lh.setUint32(14, crc, true);
    lh.setUint32(18, comp.length, true);
    lh.setUint32(22, raw.length, true);
    lh.setUint16(26, name.length, true);
    const local = concat([new Uint8Array(lh.buffer), name, comp]);
    const ch = new DataView(new ArrayBuffer(46));
    ch.setUint32(0, 0x02014b50, true);
    ch.setUint16(4, 20, true);
    ch.setUint16(6, 20, true);
    ch.setUint16(8, flags, true);
    ch.setUint16(10, method, true);
    ch.setUint32(16, crc, true);
    ch.setUint32(20, comp.length, true);
    ch.setUint32(24, raw.length, true);
    ch.setUint16(28, name.length, true);
    ch.setUint32(42, offset, true);
    central.push(concat([new Uint8Array(ch.buffer), name]));
    locals.push(local);
    offset += local.length;
  }
  const cd = concat(central);
  const eocd = new DataView(new ArrayBuffer(22));
  eocd.setUint32(0, 0x06054b50, true);
  eocd.setUint16(8, entries.length, true);
  eocd.setUint16(10, entries.length, true);
  eocd.setUint32(12, cd.length, true);
  eocd.setUint32(16, offset, true);
  return concat([...locals, cd, new Uint8Array(eocd.buffer)]);
}

/** The first bytes of a Windows program: MZ, e_lfanew at 0x3c, "PE\0\0" where it points. */
export function fakePe(): Uint8Array {
  const b = new Uint8Array(512);
  b[0] = 0x4d;
  b[1] = 0x5a;
  new DataView(b.buffer).setUint32(0x3c, 0x80, true);
  b.set([0x50, 0x45, 0, 0], 0x80);
  return b;
}

/**
 * What the queue consumer does with the page reads the cron sent: take them off the fake
 * queue and read them (`readPageJobs`), as one batch.
 */
export async function drainPages(
  env: Env,
  opts: { now?: number; fetch?: typeof fetch; wait?: (ms: number) => Promise<void>; clock?: () => number } = {},
): Promise<PageBatchResult & { ids: number[] }> {
  const q = (env.MIRROR_QUEUE as unknown as { sent: MirrorJob[] }).sent;
  const ids: number[] = [];
  for (let i = q.length - 1; i >= 0; i--) {
    const j = q[i];
    if (j.kind === "page") {
      ids.unshift(j.id);
      q.splice(i, 1);
    }
  }
  return { ...(await readPageJobs(env, ids, opts)), ids };
}
