/**
 * The mirror fetcher's side of the control plane: work for the box that fetches what answers
 * Cloudflare Workers 403 (mxb-mods.com's pages, MediaFire, and any host that has refused the
 * Worker a file: `fetcherroute.ts`). The box (`tools/mxb-fetcher`) opens no ports; it pulls.
 *
 *   POST /v1/mirror/fetcher/lease       up to `max` jobs: post pages to read, files to fetch
 *   POST /v1/mirror/fetcher/result      a page's HTML, a folder's listing, or why a job failed
 *   POST /v1/mirror/fetcher/upload-url  a presigned R2 PUT for one file or picture, by SHA-256
 *   POST /v1/mirror/fetcher/done        the upload is in R2: record it
 *
 * Every call carries `Authorization: Bearer <MIRROR_FETCHER_TOKEN>`.
 *
 * A job is a row in D1, as it is for the queue: a page is `mod_assets.page_status = 'fetcher'`
 * with `page_due_at` as the lease's end; a file is `mod_files.status = 'fetcher'` with
 * `leased_until`. A lease that runs out (the box died) is handed out again; nothing here waits
 * on the box.
 *
 * A page's HTML is parsed exactly as the Worker's own read parses it (`mirror.ts` `parsePage`,
 * `writeMirrorVersion`, `finishPage`). Its links are written at once. If it has pictures we
 * don't hold yet, the answer lists them; the box uploads each (`img/<sha256>.<ext>`, the
 * thumbnail as `thumbs/<sha256>.<ext>`) and `done` finishes the row. What the parse needs
 * meanwhile waits in `mirror_state` as `fetcher-page:<id>`.
 *
 * A file goes to `<prefix>/<sha256>` in mxb-assets, the key the Worker would have given it
 * (`mirrorfetch.ts` `placement`). The box hashes it before asking for the URL, so a file we
 * already hold is never uploaded twice. Locked (`.mxbsecure`) content never goes this way.
 */

import { AwsClient } from "aws4fetch";
import {
  backoff,
  DUE_COLUMNS,
  finishPage,
  heldImages,
  heldThumb,
  LEASE_MS,
  MAX_THUMB_BYTES,
  PAGE_LEASE_MS,
  PAGE_REV,
  pageFailed,
  parsePage,
  sniffImage,
  thumbExt,
  writeMirrorVersion,
  type DueAsset,
  type ImageSource,
  type ParsedPage,
  type Thumb,
} from "./mirror";
import { MAX_IMAGE_BYTES, MAX_IMAGES } from "./modbody";
import { expandFolder, placement } from "./mirrorfetch";
import { pagesViaFetcher } from "./fetcherroute";
import { FOLDER_MAX_FILES } from "./mirrorhosts";

export const FETCHER_PREFIX = "/v1/mirror/fetcher/";
/** Jobs one lease may hand out. */
const MAX_LEASE = 10;
const DEFAULT_LEASE = 2;
/** How long a presigned URL stays good: long enough for a 4 GB PUT on a slow line. */
const URL_TTL_S = 6 * 3600;
/** R2 takes at most 5 GiB in one PUT; the box sends one PUT a file. Larger is the runner's. */
export const FETCHER_MAX_BYTES = 4.5 * 1024 ** 3;
const FILE_MAX_ATTEMPTS = 8;
/** A post page is ~150 KB; anything far past that is not one. */
const MAX_HTML_BYTES = 4 * 1024 * 1024;
/** How long a host that refused the box keeps a page away, at least. */
const REFUSED_WAIT_MS = 30 * 60_000;
const MIN_TOKEN = 32;

type Json = Record<string, unknown>;

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

/** Constant-time over SHA-256 digests, so neither the content nor the length leaks. */
async function sameSecret(a: string, b: string): Promise<boolean> {
  const enc = new TextEncoder();
  const [x, y] = await Promise.all([crypto.subtle.digest("SHA-256", enc.encode(a)), crypto.subtle.digest("SHA-256", enc.encode(b))]);
  const p = new Uint8Array(x);
  const q = new Uint8Array(y);
  let d = 0;
  for (let i = 0; i < p.length; i++) d |= p[i] ^ q[i];
  return d === 0;
}

async function refusal(request: Request, env: Env): Promise<Response | null> {
  const want = env.MIRROR_FETCHER_TOKEN ?? "";
  if (want.length < MIN_TOKEN) return json(503, { error: "the fetcher is not configured" });
  const got = /^Bearer\s+(\S+)\s*$/i.exec(request.headers.get("authorization") ?? "")?.[1] ?? "";
  if (!got || !(await sameSecret(got, want))) return json(401, { error: "unauthorised" });
  return null;
}

// ───────────────────────────── jobs ─────────────────────────────

type JobRef = { kind: "page"; id: number } | { kind: "file"; version: number; idx: number; part: number };

export function jobId(j: JobRef): string {
  return j.kind === "page" ? `page:${j.id}` : `file:${j.version}:${j.idx}:${j.part}`;
}

export function parseJobId(raw: unknown): JobRef | null {
  if (typeof raw !== "string") return null;
  const page = /^page:(\d{1,12})$/.exec(raw);
  if (page) return { kind: "page", id: Number(page[1]) };
  const file = /^file:(\d{1,12}):(\d{1,6}):(\d{1,6})$/.exec(raw);
  if (file) return { kind: "file", version: Number(file[1]), idx: Number(file[2]), part: Number(file[3]) };
  return null;
}

export interface LeasedJob {
  id: string;
  kind: "page" | "file";
  url: string;
  /** Files: the name the folder listing gave it, if any. */
  filename?: string | null;
  /** Files: whether a folder share may be listed (not for a file already listed out of one). */
  folder_allowed?: boolean;
  max_bytes?: number;
}

interface FileRow {
  version_id: number;
  idx: number;
  part: number;
  rel: string | null;
  url: string;
  is_server: number;
  attempts: number;
  type: string;
}

async function leasedFile(env: Env, j: Extract<JobRef, { kind: "file" }>, now: number): Promise<FileRow | null> {
  return await env.DB.prepare(
    `SELECT f.version_id, f.idx, f.part, f.rel, f.url, f.is_server, f.attempts, a.type
     FROM mod_files f JOIN mod_versions v ON v.id = f.version_id JOIN mod_assets a ON a.id = v.asset_id
     WHERE f.version_id = ? AND f.idx = ? AND f.part = ? AND f.status = 'fetcher' AND f.leased_until > ? AND f.url IS NOT NULL`,
  )
    .bind(j.version, j.idx, j.part, now)
    .first<FileRow>();
}

async function leasedPage(env: Env, id: number, now: number): Promise<DueAsset | null> {
  return await env.DB.prepare(
    `SELECT ${DUE_COLUMNS} FROM mod_assets WHERE id = ? AND source = 'mirror' AND page_status = 'fetcher' AND page_due_at > ?`,
  )
    .bind(id, now)
    .first<DueAsset>();
}

/** Files first (someone is usually waiting on one), then pages: new and changed, then re-reads. */
export async function lease(env: Env, max: number, now: number): Promise<LeasedJob[]> {
  const jobs: LeasedJob[] = [];
  const { results: files } = await env.DB.prepare(
    `SELECT version_id, idx, part, rel, url FROM mod_files
     WHERE status = 'fetcher' AND due_at <= ?1 AND leased_until < ?1 AND url IS NOT NULL
     ORDER BY due_at, version_id DESC, idx, part LIMIT ?2`,
  )
    .bind(now, max)
    .all<{ version_id: number; idx: number; part: number; rel: string | null; url: string }>();
  for (const f of files) {
    const took = await env.DB.prepare(
      `UPDATE mod_files SET leased_until = ? WHERE version_id = ? AND idx = ? AND part = ? AND status = 'fetcher' AND leased_until < ?`,
    )
      .bind(now + LEASE_MS, f.version_id, f.idx, f.part, now)
      .run();
    if (!took.meta.changes) continue;
    jobs.push({
      id: jobId({ kind: "file", version: f.version_id, idx: f.idx, part: f.part }),
      kind: "file",
      url: f.url,
      filename: f.rel ? f.rel.split("/").pop() ?? null : null,
      folder_allowed: f.part === 0,
      max_bytes: FETCHER_MAX_BYTES,
    });
  }

  const room = max - jobs.length;
  if (room <= 0 || !pagesViaFetcher(env)) return jobs;
  const due = `source = 'mirror' AND (
       (page_status IN ('due', 'retry', 'queued', 'fetcher') AND page_due_at <= ?1)
       OR (page_status = 'ok' AND page_rev < ?2))`;
  const { results: pages } = await env.DB.prepare(
    `SELECT id, source_url FROM mod_assets WHERE ${due}
     ORDER BY CASE WHEN page_status = 'ok' THEN 1 ELSE 0 END, page_due_at, id DESC LIMIT ?3`,
  )
    .bind(now, PAGE_REV, room)
    .all<{ id: number; source_url: string }>();
  for (const p of pages) {
    let onSite = false;
    try {
      onSite = new URL(p.source_url).hostname === "mxb-mods.com";
    } catch {
      onSite = false;
    }
    if (!onSite) {
      await env.DB.prepare("UPDATE mod_assets SET page_status = 'gone', page_error = 'off-site link' WHERE id = ?").bind(p.id).run();
      continue;
    }
    const took = await env.DB.prepare(`UPDATE mod_assets SET page_status = 'fetcher', page_due_at = ?3 WHERE id = ?4 AND ${due}`)
      .bind(now, PAGE_REV, now + PAGE_LEASE_MS, p.id)
      .run();
    if (took.meta.changes) jobs.push({ id: jobId({ kind: "page", id: p.id }), kind: "page", url: p.source_url });
  }
  return jobs;
}

// ───────────────────────────── results ─────────────────────────────

/** A picture the box should fetch and upload before `done`. */
export interface WantedImage {
  src: string;
  purpose: "thumb" | "image";
  max_bytes: number;
}

/** What a page waits on between `result` and `done`. */
type PendingPage = Omit<ParsedPage, "downloads" | "version">;

const pendingKey = (id: number) => `fetcher-page:${id}`;

async function getPending(env: Env, id: number): Promise<PendingPage | null> {
  const row = await env.DB.prepare("SELECT value FROM mirror_state WHERE key = ?").bind(pendingKey(id)).first<{ value: string }>();
  if (!row) return null;
  try {
    return JSON.parse(row.value) as PendingPage;
  } catch {
    return null;
  }
}

/** The pictures a page offers that we don't hold, as the Worker's read would have copied them. */
export async function missingImages(env: Env, asset: DueAsset, page: PendingPage): Promise<WantedImage[]> {
  const out: WantedImage[] = [];
  if (!asset.thumb_key && page.src && isSiteUrl(page.src) && !(await heldThumb(env, page.src))) {
    out.push({ src: page.src, purpose: "thumb", max_bytes: MAX_THUMB_BYTES });
  }
  const known = await heldImages(env, asset.id);
  for (const img of page.body.images.slice(0, MAX_IMAGES)) {
    if (known.has(img.src) || !/^https:\/\//i.test(img.src)) continue;
    if (out.some((o) => o.purpose === "image" && o.src === img.src)) continue;
    out.push({ src: img.src, purpose: "image", max_bytes: MAX_IMAGE_BYTES });
  }
  return out;
}

/** The Worker copies thumbnails from the site itself only; so does this. */
function isSiteUrl(src: string): boolean {
  try {
    return new URL(src).hostname === "mxb-mods.com";
  } catch {
    return false;
  }
}

/** Only what is already held: for a page whose pictures all are, or that `done` completes. */
function heldSource(env: Env, thumbs = new Map<string, Thumb>(), images = new Map<string, string>()): ImageSource {
  return {
    thumb: async (src) => (src ? thumbs.get(src) ?? (await heldThumb(env, src)) : null),
    image: async (src) => images.get(src) ?? null,
  };
}

async function pageResult(env: Env, id: number, body: Json, now: number): Promise<Response> {
  const asset = await leasedPage(env, id, now);
  if (!asset) return json(409, { error: "not leased" });
  if (typeof body.error === "string") {
    const status = Number(body.status) || 0;
    if (status === 404 || status === 410) {
      await env.DB.prepare("UPDATE mod_assets SET page_status = 'gone', page_error = ? WHERE id = ?")
        .bind(`page answered ${status}`, id)
        .run();
    } else if (status === 403 || status === 429 || status === 503 || body.deferred === true) {
      // The site turned the box away too, or the box is backing it off. Not the page's fault:
      // no attempt is counted.
      const wait = Math.max(REFUSED_WAIT_MS, Number(body.retry_after_ms) || 0);
      await env.DB.prepare("UPDATE mod_assets SET page_status = 'due', page_due_at = ?, page_error = ? WHERE id = ?")
        .bind(now + wait, `fetcher: ${String(body.error).slice(0, 200)}`, id)
        .run();
    } else {
      await pageFailed(env, now, asset, `fetcher: ${body.error}`);
    }
    return json(200, { ok: true });
  }
  if (typeof body.html !== "string") return json(400, { error: "html or error required" });
  if (body.html.length > MAX_HTML_BYTES) return json(413, { error: "too large for a post page" });

  const page = parsePage(body.html, asset);
  if (!page) {
    await pageFailed(env, now, asset, "challenge page");
    return json(200, { ok: true, images: [] });
  }
  // The links first, as the Worker's read does: they are what the page is for.
  await writeMirrorVersion(env, asset.id, page.version, page.downloads, now);
  const pending: PendingPage = { author: page.author, src: page.src, body: page.body };
  const images = await missingImages(env, asset, pending);
  if (images.length === 0) {
    await finishPage(env, asset, pending, heldSource(env), now);
    return json(200, { ok: true, images: [] });
  }
  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO mirror_state (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    ).bind(pendingKey(id), JSON.stringify(pending)),
    env.DB.prepare("UPDATE mod_assets SET page_due_at = ? WHERE id = ?").bind(now + PAGE_LEASE_MS, id),
  ]);
  return json(200, { ok: true, images });
}

async function fileResult(env: Env, j: Extract<JobRef, { kind: "file" }>, body: Json, now: number): Promise<Response> {
  const row = await leasedFile(env, j, now);
  if (!row) return json(409, { error: "not leased" });

  if (body.folder && typeof body.folder === "object") {
    const folder = body.folder as { name?: unknown; files?: unknown };
    const files = Array.isArray(folder.files) ? folder.files : [];
    if (row.part > 0) return fail(env, row, "a folder inside a listed folder", true, now);
    const list = files
      .filter((f): f is { rel: string; url: string; size?: number } =>
        !!f && typeof (f as Json).rel === "string" && typeof (f as Json).url === "string" && /^https?:\/\//i.test((f as Json).url as string),
      )
      .slice(0, FOLDER_MAX_FILES)
      .map((f) => ({ rel: f.rel.slice(0, 300), url: f.url.slice(0, 2000), size: Number(f.size) || null }));
    if (list.length === 0) return fail(env, row, "the folder is empty", true, now);
    const name = typeof folder.name === "string" && folder.name.trim() ? folder.name.trim().slice(0, 100) : "mod";
    await expandFolder(env, row, { kind: "folder", name, files: list }, now, "fetcher");
    return json(200, { ok: true });
  }
  if (body.too_big === true) {
    await env.DB.prepare(
      "UPDATE mod_files SET status = 'runner', error = ?, fetched_at = ?, leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?",
    )
      .bind(`larger than ${FETCHER_MAX_BYTES} bytes`, now, row.version_id, row.idx, row.part)
      .run();
    return json(200, { ok: true });
  }
  if (typeof body.error === "string" && body.deferred === true) {
    // The box is backing the host off (it said 403/429): later, and no attempt counted.
    const wait = Math.max(REFUSED_WAIT_MS, Number(body.retry_after_ms) || 0);
    await env.DB.prepare(
      "UPDATE mod_files SET due_at = ?, error = ?, leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?",
    )
      .bind(now + wait, `fetcher: ${body.error}`.slice(0, 300), row.version_id, row.idx, row.part)
      .run();
    return json(200, { ok: true });
  }
  if (typeof body.error === "string") {
    return fail(env, row, `fetcher: ${body.error}`, body.permanent === true, now, Number(body.retry_after_ms) || 0);
  }
  return json(400, { error: "folder, too_big or error required" });
}

/** A failed fetch: retried with backoff (still the fetcher's), or given up on. */
async function fail(env: Env, row: FileRow, error: string, permanent: boolean, now: number, retryAfter = 0): Promise<Response> {
  const attempts = row.attempts + 1;
  const failed = permanent || attempts >= FILE_MAX_ATTEMPTS;
  await env.DB.prepare(
    `UPDATE mod_files SET status = ?, attempts = ?, due_at = ?, error = ?, leased_until = 0
     WHERE version_id = ? AND idx = ? AND part = ?`,
  )
    .bind(failed ? "failed" : "fetcher", attempts, now + Math.max(backoff(attempts), retryAfter), error.slice(0, 300), row.version_id, row.idx, row.part)
    .run();
  return json(200, { ok: true });
}

// ───────────────────────────── uploads ─────────────────────────────

/** R2's S3 endpoint and the R2_* key, against the public bucket. Absent: 503. */
function s3(env: Env): { aws: AwsClient; base: string } | null {
  if (!env.R2_ACCESS_KEY_ID || !env.R2_SECRET_ACCESS_KEY || !env.R2_S3_ENDPOINT) return null;
  const bucket = env.MXB_ASSETS_BUCKET || "mxb-assets";
  return {
    aws: new AwsClient({ accessKeyId: env.R2_ACCESS_KEY_ID, secretAccessKey: env.R2_SECRET_ACCESS_KEY, service: "s3", region: "auto" }),
    base: `${env.R2_S3_ENDPOINT.replace(/\/+$/, "")}/${bucket}`,
  };
}

const IMMUTABLE = "public, max-age=31536000, immutable";

/**
 * A presigned PUT for `key`. The headers it returns are part of the signature (bar
 * content-type), so the box must send exactly them.
 */
export async function presignPut(
  env: Env,
  key: string,
  headers: Record<string, string>,
): Promise<{ url: string; headers: Record<string, string> } | null> {
  const c = s3(env);
  if (!c) return null;
  const u = new URL(`${c.base}/${key.split("/").map(encodeURIComponent).join("/")}`);
  u.searchParams.set("X-Amz-Expires", String(URL_TTL_S));
  const signed = await c.aws.sign(new Request(u, { method: "PUT", headers }), { aws: { signQuery: true } });
  return { url: signed.url, headers };
}

interface Upload {
  sha: string;
  size: number;
  type: string;
  filename: string;
}

function readUpload(body: Json): Upload | string {
  const sha = typeof body.sha256 === "string" ? body.sha256.toLowerCase() : "";
  if (!/^[0-9a-f]{64}$/.test(sha)) return "sha256 must be 64 hex digits";
  const size = Number(body.size);
  if (!Number.isSafeInteger(size) || size <= 0) return "size must be a positive integer";
  const type = typeof body.content_type === "string" && /^[\w.+-]+\/[\w.+-]+$/.test(body.content_type) ? body.content_type.toLowerCase() : "application/octet-stream";
  const raw = typeof body.filename === "string" ? body.filename : "";
  const filename = raw.replace(/[\\/:*?"<>|\x00-\x1f]/g, "_").trim().slice(0, 200) || "download.bin";
  return { sha, size, type, filename };
}

/** Where a picture goes, or why it can't. */
function imageKey(purpose: unknown, up: Upload): { key: string; cap: number } | string {
  if (purpose !== "thumb" && purpose !== "image") return "purpose must be thumb or image";
  const ext = thumbExt(up.type);
  if (!ext) return "not a picture type we keep";
  const cap = purpose === "thumb" ? MAX_THUMB_BYTES : MAX_IMAGE_BYTES;
  if (up.size > cap) return "larger than a picture may be";
  return { key: `${purpose === "thumb" ? "thumbs" : "img"}/${up.sha}.${ext}`, cap };
}

/** Is this picture one the page asked for? */
function pageWants(pending: PendingPage, purpose: unknown, src: unknown): boolean {
  if (typeof src !== "string") return false;
  if (purpose === "thumb") return pending.src === src;
  return pending.body.images.some((i) => i.src === src);
}

/** A file's name: the one its folder listing gave it, else what the host said. */
function nameOf(row: FileRow, up: Upload): string {
  return row.rel ? row.rel.split("/").pop()! : up.filename;
}

async function knownBlob(env: Env, sha: string): Promise<{ r2_key: string; bucket: string } | null> {
  return await env.DB.prepare("SELECT r2_key, bucket FROM mod_blobs WHERE sha256 = ?").bind(sha).first<{ r2_key: string; bucket: string }>();
}

async function uploadUrl(env: Env, j: JobRef, body: Json, now: number): Promise<Response> {
  const up = readUpload(body);
  if (typeof up === "string") return json(400, { error: up });
  let key: string;
  const headers: Record<string, string> = { "cache-control": IMMUTABLE, "content-type": up.type };
  if (j.kind === "page") {
    if (!(await leasedPage(env, j.id, now))) return json(409, { error: "not leased" });
    const pending = await getPending(env, j.id);
    if (!pending || !pageWants(pending, body.purpose, body.src)) return json(409, { error: "the page didn't ask for that picture" });
    const where = imageKey(body.purpose, up);
    if (typeof where === "string") return json(400, { error: where });
    key = where.key;
  } else {
    const row = await leasedFile(env, j, now);
    if (!row) return json(409, { error: "not leased" });
    if (up.size > FETCHER_MAX_BYTES) return json(413, { error: "too big for one PUT" });
    const where = placement(row.type, row.is_server === 1, nameOf(row, up));
    if (where.bucket !== "public") {
      // Locked content belongs in the private bucket, which the fetcher never writes to.
      await env.DB.prepare(
        "UPDATE mod_files SET status = 'runner', error = 'locked content: not via the fetcher', leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?",
      )
        .bind(row.version_id, row.idx, row.part)
        .run();
      return json(422, { error: "locked content is not mirrored through the fetcher" });
    }
    key = `${where.prefix}/${up.sha}`;
    headers["content-disposition"] = `attachment; filename*=UTF-8''${encodeURIComponent(nameOf(row, up))}`;
  }
  const have = await knownBlob(env, up.sha);
  if (have) return json(200, { have: true, key: have.r2_key });
  const signed = await presignPut(env, key, headers);
  if (!signed) return json(503, { error: "uploads are not configured" });
  return json(200, { have: false, key, method: "PUT", url: signed.url, headers: signed.headers, expires_in: URL_TTL_S });
}

/**
 * Check what the box says it uploaded: there, the size it said, and for a picture, a picture by
 * its first bytes (never SVG). A bad object is deleted. Recorded in `mod_blobs` when good.
 */
async function registerUpload(env: Env, key: string, up: Upload, picture: boolean, now: number): Promise<string | null> {
  const bucket = env.ASSET_MIRROR;
  if (!bucket) return "the asset bucket is not bound";
  const head = await bucket.head(key);
  if (!head) return "not in R2";
  if (head.size !== up.size) {
    await bucket.delete(key);
    return `R2 holds ${head.size} bytes, not ${up.size}`;
  }
  if (picture) {
    const first = await bucket.get(key, { range: { offset: 0, length: 32 } });
    const bytes = first ? new Uint8Array(await first.arrayBuffer()) : new Uint8Array();
    if (sniffImage(bytes) !== up.type) {
      await bucket.delete(key);
      return "not the picture it claims to be";
    }
  }
  await env.DB.prepare(
    `INSERT OR IGNORE INTO mod_blobs (sha256, bucket, r2_key, size, content_type, filename, first_seen)
     VALUES (?, 'public', ?, ?, ?, ?, ?)`,
  )
    .bind(up.sha, key, up.size, up.type, up.filename, now)
    .run();
  return null;
}

async function fileDone(env: Env, j: Extract<JobRef, { kind: "file" }>, body: Json, now: number): Promise<Response> {
  const row = await leasedFile(env, j, now);
  if (!row) return json(409, { error: "not leased" });
  const up = readUpload(body);
  if (typeof up === "string") return json(400, { error: up });
  const filename = nameOf(row, up);
  if (!(await knownBlob(env, up.sha))) {
    const where = placement(row.type, row.is_server === 1, filename);
    if (where.bucket !== "public") return json(422, { error: "locked content is not mirrored through the fetcher" });
    const bad = await registerUpload(env, `${where.prefix}/${up.sha}`, up, false, now);
    if (bad) return json(409, { error: bad });
  }
  await env.DB.prepare(
    `UPDATE mod_files SET status = 'done', sha256 = ?, filename = ?, error = NULL, attempts = 0,
       fetched_at = ?, leased_until = 0 WHERE version_id = ? AND idx = ? AND part = ?`,
  )
    .bind(up.sha, filename, now, row.version_id, row.idx, row.part)
    .run();
  return json(200, { ok: true });
}

async function pageDone(env: Env, id: number, body: Json, now: number): Promise<Response> {
  const asset = await leasedPage(env, id, now);
  if (!asset) return json(409, { error: "not leased" });
  const pending = await getPending(env, id);
  if (!pending) return json(409, { error: "no page waiting on pictures" });
  const thumbs = new Map<string, Thumb>();
  const images = new Map<string, string>();
  const rejected: { src: string; error: string }[] = [];
  for (const raw of Array.isArray(body.images) ? body.images.slice(0, MAX_IMAGES + 1) : []) {
    const item = (raw ?? {}) as Json;
    const src = typeof item.src === "string" ? item.src : "";
    if (!pageWants(pending, item.purpose, src)) continue;
    const up = readUpload(item);
    const where = typeof up === "string" ? up : imageKey(item.purpose, up);
    if (typeof up === "string" || typeof where === "string") {
      rejected.push({ src, error: typeof up === "string" ? up : (where as string) });
      continue;
    }
    const have = await knownBlob(env, up.sha);
    if (!have) {
      const bad = await registerUpload(env, where.key, up, true, now);
      if (bad) {
        rejected.push({ src, error: bad });
        continue;
      }
    }
    if (item.purpose === "thumb") thumbs.set(src, { sha: up.sha, key: have?.r2_key ?? where.key });
    else images.set(src, up.sha);
  }
  await finishPage(env, asset, pending, heldSource(env, thumbs, images), now);
  await env.DB.prepare("DELETE FROM mirror_state WHERE key = ?").bind(pendingKey(id)).run();
  return json(200, { ok: true, rejected });
}

// ───────────────────────────── the routes ─────────────────────────────

export async function fetcherRoutes(request: Request, url: URL, env: Env, now = Date.now()): Promise<Response> {
  if (request.method !== "POST") return json(405, { error: "POST only" });
  const denied = await refusal(request, env);
  if (denied) return denied;
  const body = (await request.json().catch(() => null)) as Json | null;
  if (!body || typeof body !== "object" || Array.isArray(body)) return json(400, { error: "a JSON object is required" });
  const action = url.pathname.slice(FETCHER_PREFIX.length);

  if (action === "lease") {
    const max = Math.min(MAX_LEASE, Math.max(1, Math.floor(Number(body.max) || DEFAULT_LEASE)));
    const jobs = await lease(env, max, now);
    return json(200, { jobs, lease_seconds: Math.floor(Math.min(LEASE_MS, PAGE_LEASE_MS) / 1000) });
  }
  const job = parseJobId(body.job);
  if (!job) return json(400, { error: "job must be page:<id> or file:<version>:<idx>:<part>" });
  switch (action) {
    case "result":
      return job.kind === "page" ? pageResult(env, job.id, body, now) : fileResult(env, job, body, now);
    case "upload-url":
      return uploadUrl(env, job, body, now);
    case "done":
      return job.kind === "page" ? pageDone(env, job.id, body, now) : fileDone(env, job, body, now);
    default:
      return json(404, { error: "no such fetcher route" });
  }
}
