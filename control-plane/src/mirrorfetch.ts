/**
 * The mirror's queue consumer: one message, one file, streamed into R2 by SHA-256.
 *
 * A file is resolved (`mirrorhosts.ts`) right before it is fetched, because the links hosts
 * hand out expire. A folder share is not fetched at all: its files are listed as parts of the
 * same download option and each comes back through the queue on its own.
 *
 * Bytes are never held whole. The body is read in chunks, hashed as it passes, and cut into
 * 16 MiB parts of an R2 multipart upload to a temporary key. Only once the hash is known is
 * the object's real key known, so the upload is then copied (in the same streaming way) to
 * `<type>/<sha256>` — or simply deleted, when that blob is already held. Memory stays at one
 * part, whatever the file's size.
 */

import { backoff, hex, type MirrorJob } from "./mirror";
import { verifyUpload } from "./uploads";
import { HostError, RunnerNeeded, filenameFrom, megaDecryptStream, openBody, resolveShare, hostKind, type Resolved } from "./mirrorhosts";

/** What one Worker invocation will stream. Larger files are left for the runner. */
export const MAX_WORKER_BYTES = 2 * 1024 ** 3;
export const PART_BYTES = 16 * 1024 * 1024;
const MAX_ATTEMPTS = 8;

/** An incremental SHA-256. */
export interface Hasher {
  update(chunk: Uint8Array): void | Promise<void>;
  digest(): Promise<string>;
}

/** Workers' native streaming digest. Tests hand in node's instead. */
export function digestStreamHasher(): Hasher {
  const ds = new crypto.DigestStream("SHA-256");
  const w = ds.getWriter();
  return {
    update: (c) => w.write(c),
    async digest() {
      await w.close();
      return hex(await ds.digest);
    },
  };
}

export interface FetchDeps {
  now?: number;
  fetch?: typeof fetch;
  hasher?: () => Hasher;
}

interface FileRow {
  version_id: number;
  idx: number;
  part: number;
  rel: string | null;
  url: string;
  is_server: number;
  status: string;
  attempts: number;
  type: string;
}

/** The queue handler. Every message is acked: D1, not the queue, holds the retry schedule. */
export async function consumeMirror(batch: MessageBatch<MirrorJob>, env: Env, deps: FetchDeps = {}): Promise<void> {
  for (const msg of batch.messages) {
    try {
      if (msg.body.kind === "upload") await verifyUpload(env, msg.body.id, deps);
      else await mirrorFile(env, msg.body, deps);
    } catch (err) {
      console.error(JSON.stringify({ msg: "mirror job crashed", job: msg.body, error: String(err) }));
    }
    msg.ack();
  }
}

export async function mirrorFile(
  env: Env,
  job: Extract<MirrorJob, { kind: "file" }>,
  deps: FetchDeps = {},
): Promise<void> {
  const now = deps.now ?? Date.now();
  const f = deps.fetch ?? fetch;
  const row = await env.DB.prepare(
    `SELECT f.version_id, f.idx, f.part, f.rel, f.url, f.is_server, f.status, f.attempts, a.type
     FROM mod_files f JOIN mod_versions v ON v.id = f.version_id JOIN mod_assets a ON a.id = v.asset_id
     WHERE f.version_id = ? AND f.idx = ? AND f.part = ?`,
  )
    .bind(job.version, job.idx, job.part)
    .first<FileRow>();
  // Idempotent: a duplicate delivery, or a row the page re-read has since replaced, is skipped.
  if (!row || row.status !== "queued" || !row.url) return;

  try {
    const resolved = await resolveShare(row.url, f, row.part === 0);
    if (resolved.kind === "folder") return await expandFolder(env, row, resolved, now);
    const stored = await fetchAndStore(env, row, resolved, f, deps.hasher ?? digestStreamHasher, now);
    if (stored === "too-big") {
      await setStatus(env, row, "runner", `larger than ${MAX_WORKER_BYTES} bytes`, now);
      return;
    }
    await env.DB.prepare(
      `UPDATE mod_files SET status = 'done', sha256 = ?, filename = ?, error = NULL, attempts = 0,
         fetched_at = ?, leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?`,
    )
      .bind(stored.sha, stored.filename, now, row.version_id, row.idx, row.part)
      .run();
  } catch (err) {
    if (err instanceof RunnerNeeded) {
      await setStatus(env, row, "runner", err.message, now);
      return;
    }
    const e = err instanceof HostError ? err : new HostError(String(err));
    const attempts = row.attempts + 1;
    const failed = e.permanent || attempts >= MAX_ATTEMPTS;
    await env.DB.prepare(
      `UPDATE mod_files SET status = ?, attempts = ?, due_at = ?, error = ?, leased_until = 0
       WHERE version_id = ? AND idx = ? AND part = ?`,
    )
      .bind(
        failed ? "failed" : "retry",
        attempts,
        now + Math.max(backoff(attempts), e.retryAfterMs ?? 0),
        e.message.slice(0, 300),
        row.version_id,
        row.idx,
        row.part,
      )
      .run();
  }
}

async function setStatus(env: Env, row: FileRow, status: string, error: string | null, now: number): Promise<void> {
  await env.DB.prepare(
    "UPDATE mod_files SET status = ?, error = ?, fetched_at = ?, leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?",
  )
    .bind(status, error, now, row.version_id, row.idx, row.part)
    .run();
}

/** List a folder's files as parts of this download option; each is then fetched on its own. */
async function expandFolder(
  env: Env,
  row: FileRow,
  folder: Extract<Resolved, { kind: "folder" }>,
  now: number,
): Promise<void> {
  const stmts = [
    env.DB.prepare("DELETE FROM mod_files WHERE version_id = ? AND idx = ? AND part > 0").bind(row.version_id, row.idx),
    ...folder.files.map((file, i) =>
      env.DB.prepare(
        `INSERT INTO mod_files (version_id, idx, part, rel, url, host, label, is_server, is_default, status)
         SELECT version_id, idx, ?, ?, ?, ?, label, is_server, is_default, 'pending'
         FROM mod_files WHERE version_id = ? AND idx = ? AND part = 0`,
      ).bind(i + 1, `${folder.name}/${file.rel}`.slice(0, 400), file.url, hostKind(file.url), row.version_id, row.idx),
    ),
    env.DB.prepare(
      `UPDATE mod_files SET status = 'folder', error = NULL, attempts = 0, fetched_at = ?, leased_until = 0
       WHERE version_id = ? AND idx = ? AND part = 0`,
    ).bind(now, row.version_id, row.idx),
  ];
  await env.DB.batch(stmts);
}

/** Which bucket and prefix a file goes under. */
export function placement(type: string, isServer: boolean, filename: string): { bucket: "public" | "private"; prefix: string } {
  if (/\.mxbsecure$/i.test(filename)) return { bucket: "private", prefix: "locked" };
  if (isServer) return { bucket: "public", prefix: "server" };
  const known = ["paints", "bikes", "liveries", "kits", "tracks", "other"];
  return { bucket: "public", prefix: known.includes(type) ? type : "other" };
}

async function fetchAndStore(
  env: Env,
  row: FileRow,
  resolved: Exclude<Resolved, { kind: "folder" }>,
  f: typeof fetch,
  hasher: () => Hasher,
  now: number,
): Promise<{ sha: string; filename: string } | "too-big"> {
  let body: ReadableStream<Uint8Array>;
  let declared: number | null;
  let filename: string;
  let contentType: string;
  if (resolved.kind === "mega") {
    if (resolved.size > MAX_WORKER_BYTES) return "too-big";
    const res = await f(resolved.url, { headers: { "user-agent": "mxbsecure-mirror/1" } });
    if (!res.ok || !res.body) throw new HostError(`MEGA: storage answered ${res.status}`);
    body = res.body.pipeThrough(megaDecryptStream(resolved.key, resolved.nonce));
    declared = resolved.size;
    filename = resolved.name;
    contentType = "application/octet-stream";
  } else {
    const res = await openBody(f, resolved.url);
    declared = Number(res.headers.get("content-length")) || null;
    if (declared !== null && declared > MAX_WORKER_BYTES) {
      await res.body?.cancel();
      return "too-big";
    }
    if (!res.body) throw new HostError("empty body");
    body = res.body;
    filename = row.rel ? row.rel.split("/").pop()! : filenameFrom(res, resolved.url);
    contentType = res.headers.get("content-type")?.split(";")[0] || "application/octet-stream";
  }

  const where = placement(row.type, row.is_server === 1, filename);
  const bucket = where.bucket === "private" ? env.ASSET_LOCKED : env.ASSET_MIRROR;
  if (!bucket) throw new HostError("the asset bucket is not bound");
  const tmp = `incoming/${row.version_id}-${row.idx}-${row.part}-${now}`;
  const h = hasher();
  const size = await multipartFrom(bucket, tmp, body, { hash: h, max: MAX_WORKER_BYTES, contentType });
  if (size === "too-big") return "too-big";
  if (declared !== null && size !== declared) {
    await bucket.delete(tmp);
    throw new HostError(`short body: ${size} of ${declared} bytes`);
  }
  if (size === 0) {
    await bucket.delete(tmp);
    throw new HostError("empty file", true);
  }
  const sha = await h.digest();
  await promoteBlob(env, { from: bucket, key: tmp }, { bucket: where.bucket, prefix: where.prefix }, {
    sha,
    size,
    contentType,
    filename,
    now,
  });
  return { sha, filename };
}

/**
 * Move a hashed temporary object to its content-addressed home, `<prefix>/<sha256>`, and record
 * the blob — or drop it, when that blob is already held. Streaming, so any size is fine.
 */
export async function promoteBlob(
  env: Env,
  src: { from: R2Bucket; key: string },
  dst: { bucket: "public" | "private"; prefix: string },
  blob: { sha: string; size: number; contentType: string; filename: string; now: number },
): Promise<void> {
  const known = await env.DB.prepare("SELECT 1 FROM mod_blobs WHERE sha256 = ?").bind(blob.sha).first();
  if (!known) {
    const to = dst.bucket === "private" ? env.ASSET_LOCKED : env.ASSET_MIRROR;
    const obj = await src.from.get(src.key);
    if (!obj) throw new HostError("the temporary object vanished");
    const key = `${dst.prefix}/${blob.sha}`;
    await multipartFrom(to, key, obj.body, {
      contentType: blob.contentType,
      disposition: `attachment; filename*=UTF-8''${encodeURIComponent(blob.filename)}`,
    });
    await env.DB.prepare(
      `INSERT OR IGNORE INTO mod_blobs (sha256, bucket, r2_key, size, content_type, filename, first_seen)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    )
      .bind(blob.sha, dst.bucket, key, blob.size, blob.contentType, blob.filename, blob.now)
      .run();
  }
  await src.from.delete(src.key);
}

/**
 * Stream a body into `key` as a multipart upload, `PART_BYTES` at a time. Returns the size,
 * or "too-big" (with the upload aborted) once more than `max` bytes have arrived.
 */
export async function multipartFrom(
  bucket: R2Bucket,
  key: string,
  body: ReadableStream<Uint8Array>,
  opts: { hash?: Hasher; max?: number; contentType?: string; disposition?: string } = {},
): Promise<number | "too-big"> {
  const upload = await bucket.createMultipartUpload(key, {
    httpMetadata: {
      contentType: opts.contentType ?? "application/octet-stream",
      contentDisposition: opts.disposition,
      cacheControl: "public, max-age=31536000, immutable",
    },
  });
  const parts: R2UploadedPart[] = [];
  let buf = new Uint8Array(PART_BYTES);
  let fill = 0;
  let total = 0;
  const flush = async () => {
    if (fill === 0) return;
    parts.push(await upload.uploadPart(parts.length + 1, buf.subarray(0, fill)));
    buf = new Uint8Array(PART_BYTES);
    fill = 0;
  };
  const reader = body.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value || value.length === 0) continue;
      total += value.length;
      if (opts.max !== undefined && total > opts.max) {
        await reader.cancel();
        await upload.abort();
        return "too-big";
      }
      if (opts.hash) await opts.hash.update(value);
      let off = 0;
      while (off < value.length) {
        const n = Math.min(PART_BYTES - fill, value.length - off);
        buf.set(value.subarray(off, off + n), fill);
        fill += n;
        off += n;
        if (fill === PART_BYTES) await flush();
      }
    }
    await flush();
    if (parts.length === 0) parts.push(await upload.uploadPart(1, new Uint8Array(0)));
    await upload.complete(parts);
    return total;
  } catch (err) {
    await upload.abort().catch(() => {});
    throw err instanceof HostError ? err : new HostError(`stream failed: ${String(err)}`);
  }
}
