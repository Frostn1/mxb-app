/**
 * Riders uploading their own mods, from the MXB App, straight into R2.
 *
 *   POST   /v1/uploads                 open a session: metadata + size + SHA-256 in, presigned
 *                                      part URLs out. Bytes never pass through the Worker.
 *   GET    /v1/uploads/<id>            where it stands; for an open one, the parts R2 already
 *                                      holds and fresh URLs for the rest (resume after a crash)
 *   POST   /v1/uploads/<id>/complete   the part ETags in; the upload is assembled and queued
 *                                      for checking (`verifying`)
 *   DELETE /v1/uploads/<id>            abandon it
 *   GET    /v1/me/mods                 the caller's own mods, every state
 *   PATCH  /v1/assets/<uuid>           the owner edits title/description/bike/visibility
 *   PUT    /v1/assets/<uuid>/thumb     the owner sets the mod's picture (the image as the body)
 *   DELETE /v1/assets/<uuid>           the owner deletes their mod
 *
 * A mod is named by its public UUID here as everywhere outside D1 (`modids.ts`).
 *
 * The bytes land in the private bucket under `quarantine/`, which nothing serves. The mirror
 * worker (`uploadcheck.ts` `verifyUpload`, run by `mirror/index.ts`) then checks the size and SHA-256 the app declared, the archive's
 * structure and contents (`modscan.ts`), and only then copies the file to the public bucket
 * under `<type>/<sha256>` and makes the version live. A failed check rejects the version and
 * deletes the bytes.
 *
 * Who may upload: a Steam-confirmed account the ban gate let through (every route here sits
 * below it in `index.ts`). Quotas are per account: open sessions, uploads and bytes per day,
 * and total storage.
 */

import { AwsClient } from "aws4fetch";
import { ASSET_TYPES, hex, type AssetType, type MirrorJob } from "./mirror";
import { ScanError, MAX_PNT_BYTES } from "./modscan";
import { reject } from "./uploadcheck";
import { isPublicId, newPublicId } from "./modids";
import { MAX_THUMB_BYTES, putThumb, sniffImage, thumbExt } from "./mirror";
import { cdnBase } from "./modapi";

export const MAX_UPLOAD_BYTES = 2 * 1024 ** 3;
export const PART_BYTES = 32 * 1024 * 1024;
const SESSION_TTL_MS = 24 * 3600_000;
const URL_TTL_S = 6 * 3600;
export const QUOTA = {
  openSessions: 3,
  uploadsPerDay: 20,
  bytesPerDay: 10 * 1024 ** 3,
  storageBytes: 25 * 1024 ** 3,
};
const KINDS = { pkz: "pkz", zip: "zip", pnt: "pnt" } as const;
export type Kind = keyof typeof KINDS;

export interface Uploader {
  id: string;
  rider_name: string;
  steam_id: string | null;
}

type Result = { status: number; body: unknown };

export interface Meta {
  title: string;
  type: AssetType;
  description: string;
  bike: string;
  visibility: "public" | "unlisted";
  version: string | null;
  notes: string | null;
}

export interface UploadRow {
  id: string;
  account_id: string;
  asset_id: number | null;
  r2_key: string;
  r2_upload_id: string;
  filename: string;
  kind: Kind;
  size: number;
  sha256: string;
  part_size: number;
  meta: string;
  state: string;
  error: string | null;
  version_id: number | null;
  created_at: number;
  expires_at: number;
}

// ───────────────────────────── validation ─────────────────────────────

function str(v: unknown, max: number): string | null {
  return typeof v === "string" && v.trim().length > 0 && v.length <= max ? v.trim() : null;
}

function cleanName(raw: string): string {
  return raw.replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").trim().slice(0, 150);
}

export function kindOf(filename: string): Kind | null {
  const ext = filename.toLowerCase().split(".").pop() ?? "";
  return ext in KINDS ? (ext as Kind) : null;
}

/** The session request, checked. A string is the reason it was refused. */
export function parseOpen(body: unknown): { meta: Meta; filename: string; size: number; sha256: string; kind: Kind; assetId: string | null } | string {
  const b = (body ?? {}) as Record<string, unknown>;
  const filename = str(b.filename, 200);
  if (!filename) return "filename is required";
  const kind = kindOf(filename);
  if (!kind) return "only .pkz, .zip and .pnt files can be uploaded";
  const size = Number(b.size);
  if (!Number.isInteger(size) || size <= 0) return "size must be a positive whole number of bytes";
  const max = kind === "pnt" ? MAX_PNT_BYTES : MAX_UPLOAD_BYTES;
  if (size > max) return `the file is larger than the ${max} byte limit`;
  const sha256 = typeof b.sha256 === "string" ? b.sha256.toLowerCase() : "";
  if (!/^[0-9a-f]{64}$/.test(sha256)) return "sha256 must be 64 hex characters";
  const assetId = b.asset_id === undefined || b.asset_id === null ? null : String(b.asset_id).toLowerCase();
  if (assetId !== null && !isPublicId(assetId)) return "asset_id must be a mod's id";
  const title = str(b.title, 120);
  const type = typeof b.type === "string" && ASSET_TYPES.includes(b.type as AssetType) ? (b.type as AssetType) : null;
  // A new version may leave the asset's own fields alone.
  if (assetId === null && !title) return "title is required";
  if (assetId === null && !type) return `type must be one of ${ASSET_TYPES.join(", ")}`;
  const visibility = b.visibility === undefined ? "public" : b.visibility;
  if (visibility !== "public" && visibility !== "unlisted") return "visibility must be public or unlisted";
  const description = typeof b.description === "string" ? b.description.slice(0, 5000) : "";
  const bike = typeof b.bike === "string" ? b.bike.slice(0, 120) : "";
  return {
    filename: cleanName(filename),
    size,
    sha256,
    kind,
    assetId,
    meta: {
      title: title ?? "",
      type: type ?? "other",
      description,
      bike,
      visibility,
      version: str(b.version, 40),
      notes: typeof b.notes === "string" ? b.notes.slice(0, 2000) : null,
    },
  };
}

// ───────────────────────────── presigning ─────────────────────────────

/** R2's S3 endpoint and a key scoped to the private bucket. Absent: uploads answer 503. */
function s3(env: Env): { aws: AwsClient; base: string } | null {
  if (!env.R2_ACCESS_KEY_ID || !env.R2_SECRET_ACCESS_KEY || !env.R2_S3_ENDPOINT) return null;
  const bucket = env.MXB_PRIVATE_BUCKET || "mxb-private";
  return {
    aws: new AwsClient({
      accessKeyId: env.R2_ACCESS_KEY_ID,
      secretAccessKey: env.R2_SECRET_ACCESS_KEY,
      service: "s3",
      region: "auto",
    }),
    base: `${env.R2_S3_ENDPOINT.replace(/\/+$/, "")}/${bucket}`,
  };
}

function objectUrl(base: string, key: string): string {
  return `${base}/${key.split("/").map(encodeURIComponent).join("/")}`;
}

async function presignParts(
  env: Env,
  row: Pick<UploadRow, "r2_key" | "r2_upload_id">,
  numbers: number[],
): Promise<{ part: number; url: string }[]> {
  const c = s3(env)!;
  return Promise.all(
    numbers.map(async (n) => {
      const u = new URL(objectUrl(c.base, row.r2_key));
      u.searchParams.set("partNumber", String(n));
      u.searchParams.set("uploadId", row.r2_upload_id);
      u.searchParams.set("X-Amz-Expires", String(URL_TTL_S));
      const signed = await c.aws.sign(new Request(u, { method: "PUT" }), { aws: { signQuery: true } });
      return { part: n, url: signed.url };
    }),
  );
}

/** The parts R2 already holds for an upload (S3 ListParts), for a resume. */
async function listParts(env: Env, row: UploadRow, f: typeof fetch = fetch): Promise<{ part: number; etag: string; size: number }[]> {
  const c = s3(env)!;
  const u = new URL(objectUrl(c.base, row.r2_key));
  u.searchParams.set("uploadId", row.r2_upload_id);
  u.searchParams.set("max-parts", "1000");
  const res = await f(await c.aws.sign(u.toString(), { method: "GET" }));
  if (!res.ok) return [];
  return parseListParts(await res.text());
}

export function parseListParts(xml: string): { part: number; etag: string; size: number }[] {
  return [...xml.matchAll(/<Part>([\s\S]*?)<\/Part>/g)].map((m) => ({
    part: Number(/<PartNumber>(\d+)<\/PartNumber>/.exec(m[1])?.[1]),
    etag: (/<ETag>([^<]*)<\/ETag>/.exec(m[1])?.[1] ?? "").replace(/&quot;/g, '"'),
    size: Number(/<Size>(\d+)<\/Size>/.exec(m[1])?.[1]),
  }));
}

function partCount(size: number, partSize: number): number {
  return Math.max(1, Math.ceil(size / partSize));
}

// ───────────────────────────── the routes ─────────────────────────────

async function quotaRefusal(env: Env, accountId: string, size: number, now: number): Promise<string | null> {
  const q = await env.DB.prepare(
    `SELECT
       (SELECT COUNT(*) FROM mod_uploads WHERE account_id = ?1 AND state = 'open' AND expires_at > ?2) AS open,
       (SELECT COUNT(*) FROM mod_uploads WHERE account_id = ?1 AND created_at > ?3) AS today,
       (SELECT COALESCE(SUM(size), 0) FROM mod_uploads WHERE account_id = ?1 AND created_at > ?3
          AND state NOT IN ('aborted', 'expired')) AS bytes_today,
       (SELECT COALESCE(SUM(size), 0) FROM mod_uploads u WHERE u.account_id = ?1 AND u.state = 'live'
          AND EXISTS (SELECT 1 FROM mod_assets a WHERE a.id = u.asset_id AND a.state IN ('active', 'hidden'))) AS stored`,
  )
    .bind(accountId, now, now - 24 * 3600_000)
    .first<{ open: number; today: number; bytes_today: number; stored: number }>();
  if (!q) return null;
  if (q.open >= QUOTA.openSessions) return `at most ${QUOTA.openSessions} uploads at once — finish or cancel one first`;
  if (q.today >= QUOTA.uploadsPerDay) return `at most ${QUOTA.uploadsPerDay} uploads a day`;
  if (q.bytes_today + size > QUOTA.bytesPerDay) return "that would pass your daily upload allowance";
  if (q.stored + size > QUOTA.storageBytes) return "that would pass your storage allowance — delete an old mod first";
  return null;
}

export async function openUpload(request: Request, who: Uploader, env: Env, now = Date.now()): Promise<Result> {
  if (!who.steam_id) return { status: 403, body: { error: "sign in with Steam to upload mods" } };
  if (!s3(env) || !env.ASSET_LOCKED) return { status: 503, body: { error: "uploads are not configured" } };
  if (env.UPLOAD_LIMITER && !(await env.UPLOAD_LIMITER.limit({ key: who.id })).success)
    return { status: 429, body: { error: "slow down" } };
  const parsed = parseOpen(await request.json().catch(() => null));
  if (typeof parsed === "string") return { status: 400, body: { error: parsed } };

  let assetId: number | null = null;
  if (parsed.assetId !== null) {
    const asset = await env.DB.prepare("SELECT id, owner_account, state, source FROM mod_assets WHERE public_id = ?")
      .bind(parsed.assetId)
      .first<{ id: number; owner_account: string | null; state: string; source: string }>();
    if (!asset || asset.source !== "upload" || asset.owner_account !== who.id || asset.state === "deleted" || asset.state === "removed")
      return { status: 404, body: { error: "no such mod of yours" } };
    assetId = asset.id;
  }
  const refusal = await quotaRefusal(env, who.id, parsed.size, now);
  if (refusal) return { status: 429, body: { error: refusal } };

  const id = hex(crypto.getRandomValues(new Uint8Array(16)).buffer as ArrayBuffer);
  const key = `quarantine/${id}`;
  const mp = await env.ASSET_LOCKED.createMultipartUpload(key, {
    httpMetadata: { contentType: "application/octet-stream" },
  });
  const row = {
    id,
    r2_key: key,
    r2_upload_id: mp.uploadId,
  };
  await env.DB.prepare(
    `INSERT INTO mod_uploads (id, account_id, asset_id, r2_key, r2_upload_id, filename, kind, size, sha256,
       part_size, meta, state, created_at, expires_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?, ?)`,
  )
    .bind(
      id,
      who.id,
      assetId,
      key,
      mp.uploadId,
      parsed.filename,
      parsed.kind,
      parsed.size,
      parsed.sha256,
      PART_BYTES,
      JSON.stringify(parsed.meta),
      now,
      now + SESSION_TTL_MS,
    )
    .run();
  const n = partCount(parsed.size, PART_BYTES);
  return {
    status: 201,
    body: {
      id,
      state: "open",
      part_size: PART_BYTES,
      parts: await presignParts(env, row, Array.from({ length: n }, (_, i) => i + 1)),
      expires_at: new Date(now + SESSION_TTL_MS).toISOString(),
    },
  };
}

async function ownUpload(env: Env, id: string, who: Uploader): Promise<UploadRow | null> {
  if (!/^[0-9a-f]{32}$/.test(id)) return null;
  return env.DB.prepare("SELECT * FROM mod_uploads WHERE id = ? AND account_id = ?").bind(id, who.id).first<UploadRow>();
}

export async function uploadStatus(id: string, who: Uploader, env: Env, now = Date.now(), f: typeof fetch = fetch): Promise<Result> {
  const row = await ownUpload(env, id, who);
  if (!row) return { status: 404, body: { error: "no such upload" } };
  const body: Record<string, unknown> = {
    id: row.id,
    state: row.state,
    error: row.error,
    asset_id: await publicIdOf(env, row.asset_id),
  };
  if (row.state === "open" && row.expires_at > now && s3(env)) {
    const have = await listParts(env, row, f);
    const got = new Set(have.map((p) => p.part));
    const missing = Array.from({ length: partCount(row.size, row.part_size) }, (_, i) => i + 1).filter((n) => !got.has(n));
    body.part_size = row.part_size;
    body.uploaded = have;
    body.parts = await presignParts(env, row, missing);
  }
  return { status: 200, body };
}

export async function completeUpload(
  request: Request,
  id: string,
  who: Uploader,
  env: Env,
  now = Date.now(),
): Promise<Result> {
  const row = await ownUpload(env, id, who);
  if (!row) return { status: 404, body: { error: "no such upload" } };
  if (row.state !== "open") return { status: 409, body: { error: `the upload is ${row.state}` } };
  if (row.expires_at <= now) return { status: 410, body: { error: "the upload expired" } };
  const b = (await request.json().catch(() => null)) as { parts?: { part?: number; etag?: string }[] } | null;
  const parts = (b?.parts ?? [])
    .filter((p) => Number.isInteger(p?.part) && typeof p?.etag === "string")
    .map((p) => ({ partNumber: p.part!, etag: p.etag! }))
    .sort((x, y) => x.partNumber - y.partNumber);
  if (parts.length !== partCount(row.size, row.part_size)) return { status: 400, body: { error: "every part's etag is needed" } };

  // Claimed first, so a double-tap can't assemble it twice.
  const claimed = await env.DB.prepare("UPDATE mod_uploads SET state = 'verifying' WHERE id = ? AND state = 'open'")
    .bind(id)
    .run();
  if (!claimed.meta.changes) return { status: 409, body: { error: "the upload is already being completed" } };
  try {
    const obj = await env.ASSET_LOCKED.resumeMultipartUpload(row.r2_key, row.r2_upload_id).complete(parts);
    if (obj.size !== row.size) throw new ScanError(`the file is ${obj.size} bytes, not the ${row.size} declared`);
  } catch (err) {
    await reject(env, row, err instanceof ScanError ? err.message : "the parts could not be assembled", now);
    return { status: 400, body: { error: err instanceof ScanError ? err.message : "the parts could not be assembled" } };
  }

  // The version exists from here on, in quarantine, so the owner sees it while it's checked.
  const meta = JSON.parse(row.meta) as Meta;
  const assetId = row.asset_id ?? (await createAsset(env, who, meta, now));
  const v = await env.DB.prepare(
    `INSERT INTO mod_versions (asset_id, seq, label, notes, state, created_at, created_by)
     VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM mod_versions WHERE asset_id = ?1), ?2, ?3, 'quarantine', ?4, ?5)
     RETURNING id`,
  )
    .bind(assetId, meta.version, meta.notes, now, who.id)
    .first<{ id: number }>();
  await env.DB.prepare("UPDATE mod_uploads SET asset_id = ?, version_id = ? WHERE id = ?").bind(assetId, v!.id, id).run();
  await env.MIRROR_QUEUE.send({ kind: "upload", id } satisfies MirrorJob);
  return { status: 202, body: { id, state: "verifying", asset_id: await publicIdOf(env, assetId) } };
}

async function publicIdOf(env: Env, id: number | null): Promise<string | null> {
  if (id === null) return null;
  const row = await env.DB.prepare("SELECT public_id FROM mod_assets WHERE id = ?").bind(id).first<{ public_id: string }>();
  return row?.public_id ?? null;
}

async function createAsset(env: Env, who: Uploader, meta: Meta, now: number): Promise<number> {
  const row = await env.DB.prepare(
    `INSERT INTO mod_assets (public_id, source, owner_account, visibility, state, title, author, type, bike, description,
       modified, first_seen, last_seen, page_status)
     VALUES (?, 'upload', ?, ?, 'active', ?, ?, ?, ?, ?, ?, ?, ?, 'ok') RETURNING id`,
  )
    .bind(newPublicId(), who.id, meta.visibility, meta.title, who.rider_name, meta.type, meta.bike, meta.description, new Date(now).toISOString(), now, now)
    .first<{ id: number }>();
  return row!.id;
}

export async function abortUpload(id: string, who: Uploader, env: Env, now = Date.now()): Promise<Result> {
  const row = await ownUpload(env, id, who);
  if (!row) return { status: 404, body: { error: "no such upload" } };
  if (row.state !== "open") return { status: 409, body: { error: `the upload is ${row.state}` } };
  await env.ASSET_LOCKED.resumeMultipartUpload(row.r2_key, row.r2_upload_id).abort().catch(() => {});
  await env.DB.prepare("UPDATE mod_uploads SET state = 'aborted', finished_at = ? WHERE id = ?").bind(now, id).run();
  return { status: 200, body: { id, state: "aborted" } };
}

// ───────────────────────────── the owner ─────────────────────────────

export async function myMods(who: Uploader, env: Env): Promise<Result> {
  const { results } = await env.DB.prepare(
    `SELECT a.public_id AS id, a.title, a.type, a.visibility, a.state, a.modified, a.thumb_key, a.current_version,
       (SELECT COUNT(*) FROM mod_reports r WHERE r.asset_id = a.id AND r.resolved_at IS NULL) AS reports
     FROM mod_assets a WHERE a.owner_account = ? AND a.state <> 'deleted' ORDER BY a.modified DESC LIMIT 200`,
  )
    .bind(who.id)
    .all<Record<string, unknown> & { thumb_key: string | null; current_version: number | null }>();
  const uploads = await env.DB.prepare(
    `SELECT u.id, a.public_id AS asset_id, u.filename, u.size, u.state, u.error, u.created_at FROM mod_uploads u
     LEFT JOIN mod_assets a ON a.id = u.asset_id
     WHERE u.account_id = ? AND u.state IN ('open', 'verifying', 'rejected') ORDER BY u.created_at DESC LIMIT 50`,
  )
    .bind(who.id)
    .all();
  // `live`: a version has been published. The version's own id stays inside D1.
  const mods = results.map(({ thumb_key, current_version, ...m }) => ({
    ...m,
    live: current_version !== null,
    thumb: thumb_key ? `${cdnBase(env)}/${thumb_key}` : null,
  }));
  return { status: 200, body: { mods, uploads: uploads.results, quota: QUOTA } };
}

/**
 * The owner sets their mod's picture: the image itself as the request body, JPEG, PNG, WebP,
 * GIF or AVIF, at most `MAX_THUMB_BYTES`. Checked by its first bytes, not by what it claims.
 * Stored like a mirrored post's picture, `thumbs/<sha256>.<ext>` in the public bucket.
 */
export async function setThumb(request: Request, assetId: number, who: Uploader, env: Env, now = Date.now()): Promise<Result> {
  const owned = await env.DB.prepare(
    "SELECT 1 FROM mod_assets WHERE id = ? AND owner_account = ? AND state IN ('active', 'hidden')",
  )
    .bind(assetId, who.id)
    .first();
  if (!owned) return { status: 404, body: { error: "no such mod of yours" } };
  const tooBig = { status: 413, body: { error: `the picture is larger than ${MAX_THUMB_BYTES} bytes` } };
  if (Number(request.headers.get("content-length") ?? "0") > MAX_THUMB_BYTES) return tooBig;
  const bytes = await request.arrayBuffer();
  if (bytes.byteLength === 0) return { status: 400, body: { error: "the picture is empty" } };
  if (bytes.byteLength > MAX_THUMB_BYTES) return tooBig;
  const type = sniffImage(new Uint8Array(bytes));
  const ext = type ? thumbExt(type) : null;
  if (!type || !ext) return { status: 415, body: { error: "the picture must be a JPEG, PNG, WebP, GIF or AVIF" } };
  const thumb = await putThumb(env, bytes, type, `thumb.${ext}`, now);
  await env.DB.prepare("UPDATE mod_assets SET thumb_sha = ?, thumb_key = ?, thumb_src = NULL WHERE id = ?")
    .bind(thumb.sha, thumb.key, assetId)
    .run();
  return { status: 200, body: { thumb: `${cdnBase(env)}/${thumb.key}` } };
}

export async function editMod(request: Request, assetId: number, who: Uploader, env: Env, now = Date.now()): Promise<Result> {
  const b = ((await request.json().catch(() => null)) ?? {}) as Record<string, unknown>;
  const sets: string[] = [];
  const args: unknown[] = [];
  const title = b.title === undefined ? undefined : str(b.title, 120);
  if (title === null) return { status: 400, body: { error: "title must be 1–120 characters" } };
  if (title !== undefined) sets.push("title = ?"), args.push(title);
  if (typeof b.description === "string") sets.push("description = ?"), args.push(b.description.slice(0, 5000));
  if (typeof b.bike === "string") sets.push("bike = ?"), args.push(b.bike.slice(0, 120));
  if (b.visibility !== undefined) {
    if (b.visibility !== "public" && b.visibility !== "unlisted") return { status: 400, body: { error: "visibility must be public or unlisted" } };
    sets.push("visibility = ?"), args.push(b.visibility);
  }
  if (sets.length === 0) return { status: 400, body: { error: "nothing to change" } };
  const res = await env.DB.prepare(
    `UPDATE mod_assets SET ${sets.join(", ")}, modified = ? WHERE id = ? AND owner_account = ? AND state IN ('active', 'hidden')`,
  )
    .bind(...args, new Date(now).toISOString(), assetId, who.id)
    .run();
  if (!res.meta.changes) return { status: 404, body: { error: "no such mod of yours" } };
  return { status: 200, body: { ok: true } };
}

/** The owner deletes their mod: gone from search and downloads; files nobody else uses are freed. */
export async function deleteMod(assetId: number, who: Uploader, env: Env, now = Date.now()): Promise<Result> {
  const res = await env.DB.prepare(
    "UPDATE mod_assets SET state = 'deleted', moderated_at = ?, moderated_by = ? WHERE id = ? AND owner_account = ? AND state <> 'deleted'",
  )
    .bind(now, who.id, assetId, who.id)
    .run();
  if (!res.meta.changes) return { status: 404, body: { error: "no such mod of yours" } };
  await freeBlobs(env, assetId);
  return { status: 200, body: { ok: true } };
}

/**
 * Delete the R2 objects only this asset's versions use. A blob another asset (or the mirror)
 * also references stays — content addressing means one object can be several mods' file.
 */
export async function freeBlobs(env: Env, assetId: number): Promise<number> {
  const { results } = await env.DB.prepare(
    `SELECT DISTINCT b.sha256, b.bucket, b.r2_key FROM mod_files f
     JOIN mod_versions v ON v.id = f.version_id
     JOIN mod_blobs b ON b.sha256 = f.sha256
     WHERE v.asset_id = ?1 AND NOT EXISTS (
       SELECT 1 FROM mod_files f2 JOIN mod_versions v2 ON v2.id = f2.version_id
       JOIN mod_assets a2 ON a2.id = v2.asset_id
       WHERE f2.sha256 = b.sha256 AND v2.asset_id <> ?1 AND a2.state IN ('active', 'hidden'))`,
  )
    .bind(assetId)
    .all<{ sha256: string; bucket: string; r2_key: string }>();
  for (const b of results) {
    await (b.bucket === "private" ? env.ASSET_LOCKED : env.ASSET_MIRROR).delete(b.r2_key);
    await env.DB.prepare("DELETE FROM mod_blobs WHERE sha256 = ?").bind(b.sha256).run();
    await env.DB.prepare("UPDATE mod_files SET status = 'failed', error = 'removed' WHERE sha256 = ?").bind(b.sha256).run();
  }
  return results.length;
}
