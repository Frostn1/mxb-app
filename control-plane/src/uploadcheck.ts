/**
 * The slow half of an upload, run by the mirror worker (`mirror/index.ts`), never by the
 * control plane: hash it, look inside it (`modscan.ts`), and publish or reject it. Plus the
 * housekeeping that keeps sessions from leaking R2 parts.
 *
 * Shared, not copied: the control plane imports `reject` for a completion that doesn't add up.
 */

import { type MirrorJob } from "./mirror";
import { digestStreamHasher, type FetchDeps, promoteBlob } from "./mirrorfetch";
import { ScanError, ZipStreamScanner, checkEntries, checkPntHead, readCentralDirectory } from "./modscan";
import type { Meta, UploadRow } from "./uploads";

export async function reject(env: Env, row: UploadRow, reason: string, now: number): Promise<void> {
  await env.ASSET_LOCKED.delete(row.r2_key).catch(() => {});
  await env.DB.batch([
    env.DB.prepare("UPDATE mod_uploads SET state = 'rejected', error = ?, finished_at = ? WHERE id = ?").bind(reason.slice(0, 400), now, row.id),
    env.DB.prepare("UPDATE mod_versions SET state = 'rejected', notes = COALESCE(notes, '') WHERE id = ?").bind(row.version_id ?? -1),
  ]);
}

// ───────────────────────────── the check ─────────────────────────────

/**
 * The queue's half of an upload: hash it, look inside it, and either publish it or reject it.
 * Idempotent: anything not `verifying` is left alone.
 */
export async function verifyUpload(env: Env, id: string, deps: FetchDeps = {}): Promise<void> {
  const now = deps.now ?? Date.now();
  const row = await env.DB.prepare("SELECT * FROM mod_uploads WHERE id = ?").bind(id).first<UploadRow>();
  if (!row || row.state !== "verifying" || row.version_id === null) return;
  // Claimed, so a duplicate delivery can't check (and publish, or reject) the same upload twice.
  const claimed = await env.DB.prepare(
    "UPDATE mod_uploads SET state = 'checking', finished_at = ? WHERE id = ? AND state = 'verifying'",
  )
    .bind(now, id)
    .run();
  if (!claimed.meta.changes) return;
  const bucket = env.ASSET_LOCKED;
  try {
    const head = await bucket.head(row.r2_key);
    if (!head) throw new ScanError("the upload is missing");
    if (head.size !== row.size) throw new ScanError(`the file is ${head.size} bytes, not the ${row.size} declared`);

    const read = async (offset: number, length: number) => {
      const o = await bucket.get(row.r2_key, { range: { offset, length } });
      if (!o) throw new ScanError("the upload is missing");
      return new Uint8Array(await o.arrayBuffer());
    };
    let scanner: ZipStreamScanner | null = null;
    if (row.kind === "pnt") {
      checkPntHead(await read(0, Math.min(4096, row.size)), row.size);
    } else {
      const first = await read(0, Math.min(4, row.size));
      if (first.length < 4 || first[0] !== 0x50 || first[1] !== 0x4b || first[2] !== 0x03 || first[3] !== 0x04)
        throw new ScanError(`not a ${row.kind} archive`);
      const entries = await readCentralDirectory(read, row.size);
      checkEntries(entries);
      scanner = new ZipStreamScanner(entries);
    }

    // One pass: the hash, and the content scan alongside it.
    const hash = (deps.hasher ?? digestStreamHasher)();
    const obj = await bucket.get(row.r2_key);
    if (!obj) throw new ScanError("the upload is missing");
    const reader = obj.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value) continue;
      await hash.update(value);
      if (scanner) {
        await scanner.push(value);
        if (scanner.failed) {
          await reader.cancel().catch(() => {});
          throw scanner.failed;
        }
      }
    }
    if (scanner) {
      await scanner.finish();
      if (scanner.failed) throw scanner.failed;
    }
    const sha = await hash.digest();
    if (sha !== row.sha256) throw new ScanError("the file's SHA-256 doesn't match the one declared");

    const asset = await env.DB.prepare("SELECT type, state FROM mod_assets WHERE id = ?")
      .bind(row.asset_id)
      .first<{ type: string; state: string }>();
    await promoteBlob(env, { from: bucket, key: row.r2_key }, { bucket: "public", prefix: asset?.type ?? "other" }, {
      sha,
      size: row.size,
      contentType: row.kind === "pnt" ? "application/octet-stream" : "application/zip",
      filename: row.filename,
      now,
    });
    const meta = JSON.parse(row.meta) as Meta;
    await env.DB.batch([
      env.DB.prepare(
        `INSERT INTO mod_files (version_id, idx, part, host, label, status, sha256, filename, fetched_at)
         VALUES (?, 0, 0, 'mxbsecure', ?, 'done', ?, ?, ?)`,
      ).bind(row.version_id, row.filename, sha, row.filename, now),
      env.DB.prepare("UPDATE mod_versions SET state = 'live' WHERE id = ?").bind(row.version_id),
      // A new version updates the asset's own fields only where the uploader gave new ones.
      env.DB.prepare(
        `UPDATE mod_assets SET current_version = ?, modified = ?, last_seen = ?,
           title = COALESCE(NULLIF(?, ''), title), bike = COALESCE(NULLIF(?, ''), bike),
           description = COALESCE(NULLIF(?, ''), description)
         WHERE id = ?`,
      ).bind(row.version_id, new Date(now).toISOString(), now, meta.title, meta.bike, meta.description, row.asset_id),
      env.DB.prepare("UPDATE mod_uploads SET state = 'live', finished_at = ? WHERE id = ?").bind(now, row.id),
    ]);
  } catch (err) {
    const reason = err instanceof ScanError ? err.message : `the check failed: ${String(err)}`;
    if (!(err instanceof ScanError)) console.error(JSON.stringify({ msg: "upload check crashed", id, error: String(err) }));
    await reject(env, row, reason, now);
  }
}

/**
 * Housekeeping, on the catalogue's cron. Sessions nobody finished are aborted so R2 frees the
 * parts; a completed upload whose queue message never arrived is sent again; a check that died
 * mid-way (the invocation was evicted) is handed back to be run again.
 */
export async function expireUploads(env: Env, now = Date.now()): Promise<void> {
  await env.DB.prepare("UPDATE mod_uploads SET state = 'verifying' WHERE state = 'checking' AND finished_at < ?")
    .bind(now - 3600_000)
    .run();
  const stuck = await env.DB.prepare(
    "SELECT id FROM mod_uploads WHERE state = 'verifying' AND created_at < ? ORDER BY created_at LIMIT 20",
  )
    .bind(now - 30 * 60_000)
    .all<{ id: string }>();
  for (const u of stuck.results) await env.MIRROR_QUEUE.send({ kind: "upload", id: u.id } satisfies MirrorJob);
  const { results } = await env.DB.prepare(
    "SELECT * FROM mod_uploads WHERE state = 'open' AND expires_at < ? LIMIT 50",
  )
    .bind(now)
    .all<UploadRow>();
  for (const row of results) {
    await env.ASSET_LOCKED?.resumeMultipartUpload(row.r2_key, row.r2_upload_id).abort().catch(() => {});
    await env.DB.prepare("UPDATE mod_uploads SET state = 'expired', finished_at = ? WHERE id = ?").bind(now, row.id).run();
  }
}
