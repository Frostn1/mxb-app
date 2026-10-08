/**
 * The mirror fetcher's side of the control plane: work for the box that fetches what answers
 * Cloudflare Workers 403 (mxb-mods.com's pages, MediaFire, and any host that has refused the
 * Worker a file: `fetcherroute.ts`). The box (`tools/mxb-fetcher`) opens no ports; it pulls.
 *
 *   POST /v1/mirror/fetcher/lease       up to `max` jobs: post pages to read, files to fetch
 *   POST /v1/mirror/fetcher/result      a page's HTML, a folder's listing, or why a job failed
 *   POST /v1/mirror/fetcher/upload/start     a file or picture by SHA-256: `have`, or an upload id
 *   PUT  /v1/mirror/fetcher/upload/part      ?upload=&n= one part's raw bytes (32 MiB, in order)
 *   POST /v1/mirror/fetcher/upload/complete  assembled, then checked: size and SHA-256
 *   POST /v1/mirror/fetcher/upload/abort     the box gave up on it
 *   POST /v1/mirror/fetcher/done             the job's upload is in R2: finish the row
 *
 * Every call carries `Authorization: Bearer <MIRROR_FETCHER_TOKEN>`.
 *
 * Uploads go through this Worker's own `ASSET_MIRROR` binding as R2 multipart uploads, so no S3
 * credentials are involved. The whole file's SHA-256 is computed here as the parts pass
 * (`sha256state.ts`, state kept in `mirror_state` as `fetcher-upload:<id>`), so an object only
 * lands under `<prefix>/<sha256>` when its bytes really hash to that.
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
 * (`mirrorfetch.ts` `placement`). The box hashes it before it starts the upload, so a file we
 * already hold is never uploaded twice. Locked (`.mxbsecure`) content never goes this way.
 */

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
  discoveryResult,
  nextDiscoveryJob,
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
import { fetcherTakes, hostname, pagesViaFetcher } from "./fetcherroute";
import { FOLDER_MAX_FILES } from "./mirrorhosts";
import { sha256Digest, sha256Init, sha256Update, type Sha256State } from "./sha256state";

export const FETCHER_PREFIX = "/v1/mirror/fetcher/";
/** Jobs one lease may hand out. */
const MAX_LEASE = 10;
const DEFAULT_LEASE = 2;
/** The largest file the fetcher mirrors; larger is the runner's. */
export const FETCHER_MAX_BYTES = 4.5 * 1024 ** 3;
/**
 * One multipart part. A Workers request body may be 100 MB; a part is buffered once here and
 * hashed, so it stays well inside both that and the Worker's 128 MB of memory.
 */
export const UPLOAD_PART_BYTES = 32 * 1024 * 1024;
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
  // Trimmed: a secret pasted from a Windows terminal can carry a trailing CR/LF.
  const want = (env.MIRROR_FETCHER_TOKEN ?? "").trim();
  if (want.length < MIN_TOKEN) return json(503, { error: "the fetcher is not configured" });
  const got = /^Bearer\s+(\S+)\s*$/i.exec(request.headers.get("authorization") ?? "")?.[1] ?? "";
  if (!got || !(await sameSecret(got, want))) return json(401, { error: "unauthorised" });
  return null;
}

// ───────────────────────────── jobs ─────────────────────────────

type JobRef =
  | { kind: "page"; id: number }
  | { kind: "file"; version: number; idx: number; part: number }
  /** A discovery request (category tree, listing or id sweep): `seq` in the chain. */
  | { kind: "list"; seq: number };

export function jobId(j: JobRef): string {
  if (j.kind === "list") return `list:${j.seq}`;
  return j.kind === "page" ? `page:${j.id}` : `file:${j.version}:${j.idx}:${j.part}`;
}

export function parseJobId(raw: unknown): JobRef | null {
  if (typeof raw !== "string") return null;
  const page = /^page:(\d{1,12})$/.exec(raw);
  if (page) return { kind: "page", id: Number(page[1]) };
  const list = /^list:(\d{1,12})$/.exec(raw);
  if (list) return { kind: "list", seq: Number(list[1]) };
  const file = /^file:(\d{1,12}):(\d{1,6}):(\d{1,6})$/.exec(raw);
  if (file) return { kind: "file", version: Number(file[1]), idx: Number(file[2]), part: Number(file[3]) };
  return null;
}

export interface LeasedJob {
  id: string;
  /** `list`: GET the URL as JSON and hand back its status and body as they came. */
  kind: "page" | "file" | "list";
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

/** Due files looked at per lease, to find `max` a fetcher's host filter takes. */
const FILE_SCAN = 200;

/**
 * Files first (someone is usually waiting on one), then pages: new and changed, then re-reads.
 * `takes` is the asking fetcher's own host filter (`fetcherroute.ts` `fetcherTakes`): a box in a
 * datacenter takes MediaFire and leaves mxb-mods.com, which blocks datacenter addresses, to a
 * fetcher on a home connection. A job no running fetcher takes simply waits in D1.
 */
export async function lease(env: Env, max: number, now: number, takes: (host: string) => boolean = () => true): Promise<LeasedJob[]> {
  const jobs: LeasedJob[] = [];
  const { results: dueFiles } = await env.DB.prepare(
    `SELECT version_id, idx, part, rel, url FROM mod_files
     WHERE status = 'fetcher' AND due_at <= ?1 AND leased_until < ?1 AND url IS NOT NULL
     ORDER BY due_at, version_id DESC, idx, part LIMIT ?2`,
  )
    .bind(now, FILE_SCAN)
    .all<{ version_id: number; idx: number; part: number; rel: string | null; url: string }>();
  const files = dueFiles.filter((f) => takes(hostname(f.url))).slice(0, max);
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

  if (jobs.length >= max || !pagesViaFetcher(env) || !takes("mxb-mods.com")) return jobs;
  // Discovery: at most one request in flight, a round every ten minutes (`mirror.ts`).
  const list = await nextDiscoveryJob(env, now);
  if (list) jobs.push({ id: jobId({ kind: "list", seq: list.seq }), kind: "list", url: list.url });

  const room = max - jobs.length;
  if (room <= 0) return jobs;
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

/** A listing body is 50 posts with their content; a few MB at most. */
const MAX_LIST_BYTES = 8 * 1024 * 1024;

/** A discovery request's answer: the status and body as the site gave them, or a failure. */
async function listResult(env: Env, seq: number, body: Json, now: number): Promise<Response> {
  let taken: boolean;
  if (typeof body.error === "string") {
    taken = await discoveryResult(env, seq, { error: body.error, retryAfterMs: Number(body.retry_after_ms) || 0 }, now);
  } else {
    const status = Number(body.status);
    if (!Number.isInteger(status) || typeof body.body !== "string") return json(400, { error: "status and body, or error, required" });
    if (body.body.length > MAX_LIST_BYTES) return json(413, { error: "too large for a listing" });
    taken = await discoveryResult(env, seq, { status, body: body.body }, now);
  }
  return taken ? json(200, { ok: true }) : json(409, { error: "not leased" });
}

// ───────────────────────────── uploads ─────────────────────────────

const IMMUTABLE = "public, max-age=31536000, immutable";

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

/**
 * Where an upload goes, and how it is stored, once the job and the bytes it describes check
 * out; or the answer to give instead.
 */
async function uploadTarget(
  env: Env,
  j: Exclude<JobRef, { kind: "list" }>,
  body: Json,
  up: Upload,
  now: number,
): Promise<{ key: string; meta: R2HTTPMetadata; picture: boolean } | Response> {
  const meta: R2HTTPMetadata = { contentType: up.type, cacheControl: IMMUTABLE };
  if (j.kind === "page") {
    if (!(await leasedPage(env, j.id, now))) return json(409, { error: "not leased" });
    const pending = await getPending(env, j.id);
    if (!pending || !pageWants(pending, body.purpose, body.src)) return json(409, { error: "the page didn't ask for that picture" });
    const where = imageKey(body.purpose, up);
    if (typeof where === "string") return json(400, { error: where });
    return { key: where.key, meta, picture: true };
  }
  const row = await leasedFile(env, j, now);
  if (!row) return json(409, { error: "not leased" });
  if (up.size > FETCHER_MAX_BYTES) return json(413, { error: "larger than the fetcher takes" });
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
  meta.contentDisposition = `attachment; filename*=UTF-8''${encodeURIComponent(nameOf(row, up))}`;
  return { key: `${where.prefix}/${up.sha}`, meta, picture: false };
}

/** An upload in flight: an R2 multipart upload on mxb-assets, and the hash of what it holds. */
interface UploadSession {
  job: string;
  key: string;
  r2: string;
  up: Upload;
  picture: boolean;
  parts: R2UploadedPart[];
  hash: Sha256State;
  started: number;
}

const sessionKey = (id: string) => `fetcher-upload:${id}`;

async function getSession(env: Env, id: string): Promise<UploadSession | null> {
  if (!/^[0-9a-f]{32}$/.test(id)) return null;
  const row = await env.DB.prepare("SELECT value FROM mirror_state WHERE key = ?").bind(sessionKey(id)).first<{ value: string }>();
  return row ? (JSON.parse(row.value) as UploadSession) : null;
}

async function putSession(env: Env, id: string, s: UploadSession): Promise<void> {
  await env.DB.prepare(
    "INSERT INTO mirror_state (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
  )
    .bind(sessionKey(id), JSON.stringify(s))
    .run();
}

/** Give an upload up: R2's parts dropped, the session forgotten. */
async function abortSession(env: Env, id: string, s: UploadSession): Promise<void> {
  await env.ASSET_MIRROR.resumeMultipartUpload(s.key, s.r2).abort().catch(() => {});
  await env.DB.prepare("DELETE FROM mirror_state WHERE key = ?").bind(sessionKey(id)).run();
}

/** `POST upload/start`: a multipart upload opened, or `have` when the blob is already held. */
async function uploadStart(env: Env, j: Exclude<JobRef, { kind: "list" }>, body: Json, now: number): Promise<Response> {
  const up = readUpload(body);
  if (typeof up === "string") return json(400, { error: up });
  const target = await uploadTarget(env, j, body, up, now);
  if (target instanceof Response) return target;
  const have = await knownBlob(env, up.sha);
  if (have) return json(200, { have: true });
  if (!env.ASSET_MIRROR) return json(503, { error: "the asset bucket is not bound" });
  const r2 = await env.ASSET_MIRROR.createMultipartUpload(target.key, { httpMetadata: target.meta });
  const id = [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, "0")).join("");
  await putSession(env, id, {
    job: jobId(j),
    key: target.key,
    r2: r2.uploadId,
    up,
    picture: target.picture,
    parts: [],
    hash: sha256Init(),
    started: now,
  });
  return json(200, { have: false, upload: id, part_bytes: UPLOAD_PART_BYTES });
}

/**
 * `PUT upload/part?upload=<id>&n=<n>`: the next part, in order. Every part but the last is
 * exactly `part_bytes` (R2 wants equal parts). The bytes are hashed on their way to R2.
 */
async function uploadPart(request: Request, url: URL, env: Env): Promise<Response> {
  const id = url.searchParams.get("upload") ?? "";
  const n = Number(url.searchParams.get("n"));
  const s = await getSession(env, id);
  if (!s) return json(404, { error: "no such upload" });
  if (n !== s.parts.length + 1) return json(409, { error: `expected part ${s.parts.length + 1}` });
  const left = s.up.size - s.hash.length;
  const want = Math.min(UPLOAD_PART_BYTES, left);
  const declared = Number(request.headers.get("content-length"));
  if (want <= 0 || (Number.isFinite(declared) && declared > 0 && declared !== want)) {
    await abortSession(env, id, s);
    return json(400, { error: `part ${n} must be ${want} bytes` });
  }
  const bytes = await readExactly(request, want);
  if (!bytes) {
    await abortSession(env, id, s);
    return json(400, { error: `part ${n} must be ${want} bytes` });
  }
  try {
    const part = await env.ASSET_MIRROR.resumeMultipartUpload(s.key, s.r2).uploadPart(n, bytes);
    s.parts.push({ partNumber: part.partNumber, etag: part.etag });
  } catch (err) {
    await abortSession(env, id, s);
    return json(502, { error: `R2 refused the part: ${String(err)}` });
  }
  s.hash = sha256Update(s.hash, bytes);
  await putSession(env, id, s);
  return json(200, { ok: true, received: s.hash.length });
}

/** The request body, if it is exactly `size` bytes; read into one buffer of that size. */
async function readExactly(request: Request, size: number): Promise<Uint8Array | null> {
  if (!request.body) return null;
  const out = new Uint8Array(size);
  let at = 0;
  const reader = request.body.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (at + value.length > size) {
      await reader.cancel().catch(() => {});
      return null;
    }
    out.set(value, at);
    at += value.length;
  }
  return at === size ? out : null;
}

/**
 * `POST upload/complete`: every byte there and hashing to what the fetcher said, then the
 * object assembled, checked again in R2 (size; a picture by its first bytes) and recorded.
 * Anything wrong aborts the upload.
 */
async function uploadComplete(env: Env, body: Json, now: number): Promise<Response> {
  const id = typeof body.upload === "string" ? body.upload : "";
  const s = await getSession(env, id);
  if (!s) return json(404, { error: "no such upload" });
  if (s.hash.length !== s.up.size) {
    await abortSession(env, id, s);
    return json(409, { error: `received ${s.hash.length} of ${s.up.size} bytes` });
  }
  const sha = sha256Digest(s.hash);
  if (sha !== s.up.sha) {
    await abortSession(env, id, s);
    return json(409, { error: "the bytes don't hash to the sha256 given" });
  }
  try {
    await env.ASSET_MIRROR.resumeMultipartUpload(s.key, s.r2).complete(s.parts);
  } catch (err) {
    await abortSession(env, id, s);
    return json(502, { error: `R2 refused to complete: ${String(err)}` });
  }
  await env.DB.prepare("DELETE FROM mirror_state WHERE key = ?").bind(sessionKey(id)).run();
  const bad = await registerUpload(env, s.key, s.up, s.picture, now);
  if (bad) return json(409, { error: bad });
  return json(200, { ok: true, key: s.key });
}

/** `POST upload/abort`: the fetcher gave up on it. */
async function uploadAbort(env: Env, body: Json): Promise<Response> {
  const id = typeof body.upload === "string" ? body.upload : "";
  const s = await getSession(env, id);
  if (s) await abortSession(env, id, s);
  return json(200, { ok: true });
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
  const action = url.pathname.slice(FETCHER_PREFIX.length);
  const isPart = action === "upload/part";
  if (request.method !== (isPart ? "PUT" : "POST")) return json(405, { error: isPart ? "PUT only" : "POST only" });
  const denied = await refusal(request, env);
  if (denied) return denied;
  // A part is raw bytes, up to 32 MiB; everything else is a small JSON object.
  if (isPart) return uploadPart(request, url, env);
  const body = (await request.json().catch(() => null)) as Json | null;
  if (!body || typeof body !== "object" || Array.isArray(body)) return json(400, { error: "a JSON object is required" });
  if (action === "upload/complete") return uploadComplete(env, body, now);
  if (action === "upload/abort") return uploadAbort(env, body);

  if (action === "lease") {
    const max = Math.min(MAX_LEASE, Math.max(1, Math.floor(Number(body.max) || DEFAULT_LEASE)));
    // The fetcher's own host filter, e.g. ["*", "-mxb-mods.com"]; absent means everything.
    const hosts = Array.isArray(body.hosts) ? body.hosts.filter((h): h is string => typeof h === "string").slice(0, 50) : null;
    const jobs = await lease(env, max, now, hosts ? (h) => fetcherTakes(hosts, h) : undefined);
    return json(200, { jobs, lease_seconds: Math.floor(Math.min(LEASE_MS, PAGE_LEASE_MS) / 1000) });
  }
  const job = parseJobId(body.job);
  if (!job) return json(400, { error: "job must be page:<id> or file:<version>:<idx>:<part>" });
  if (job.kind === "list") {
    // A discovery request has a result and nothing to upload.
    if (action !== "result") return json(400, { error: "a list job only has a result" });
    return listResult(env, job.seq, body, now);
  }
  switch (action) {
    case "result":
      return job.kind === "page" ? pageResult(env, job.id, body, now) : fileResult(env, job, body, now);
    case "upload/start":
      return uploadStart(env, job, body, now);
    case "done":
      return job.kind === "page" ? pageDone(env, job.id, body, now) : fileDone(env, job, body, now);
    default:
      return json(404, { error: "no such fetcher route" });
  }
}
