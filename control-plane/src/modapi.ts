/**
 * The mod catalogue's public read side, for mxbsecure.com/mods and the MXB App. Both sources —
 * the mirror and riders' own uploads — come out of the same routes in the same shape.
 *
 *   GET  /v1/assets/search?q=&type=&bike=&source=&page=   FTS5, bm25-ranked, 24 a page
 *   GET  /v1/assets/stats                                  how many mods are listed, in all
 *                                                          and by type (one GROUP BY)
 *   GET  /v1/assets/<uuid>[?version=<seq>]                 one mod, its versions, its files
 *   GET  /v1/assets/<uuid>/download/<idx>[/<part>][?version=<seq>]
 *                                                          302 to cdn.mxbsecure.com, or to a
 *                                                          signed link for locked content
 *   POST /v1/assets/<uuid>/prepare/<idx>[/<part>][?version=<seq>]
 *                                                          queue the copy if it isn't stored, and
 *                                                          say where it stands; polled until ready
 *   GET  /v1/assets/mirror/<mxb-mods slug>                 one mirrored mod by its source's slug
 *   GET  /v1/assets/locked/<sha>?exp=&sig=                 the signed link itself
 *   POST /v1/assets/<uuid>/report                          `modreports.ts`
 *
 * Only active, public mods whose current version is live and has a stored file are listed.
 * A mod is named by its public UUID (`modids.ts`); the integer id never leaves D1. An unlisted
 * mod answers only to its UUID. Everything here is catalogue data, open to any origin and
 * cacheable for a minute; the routes are rate-limited per address.
 */

import { ASSET_TYPES, hex, LEASE_MS, type AssetType, type MirrorJob } from "./mirror";
import { safeBlocks } from "./modbody";
import { activeBikes, supported, touchBlob } from "./mirrorpolicy";
import { reportAsset } from "./modreports";
import { internalId, modSlug, UUID_RE } from "./modids";

export const PER_PAGE = 24;
const MAX_PAGE = 200;
const SIGNED_TTL_S = 300;
const DEFAULT_CDN = "https://cdn.mxbsecure.com";

/** bm25 column weights: title, author, bike, categories, description. */
const WEIGHTS = "10.0, 4.0, 3.0, 1.5, 0.5";

/**
 * A user's words as an FTS5 query: each word a quoted prefix term, all of them required.
 * Quoting keeps FTS5's own syntax (`NEAR`, `-`, `:`, `*`) from being typed at us.
 */
export function ftsQuery(q: string): string | null {
  const words = q
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .slice(0, 8);
  if (words.length === 0) return null;
  return words.map((w) => `"${w}"*`).join(" ");
}

interface AssetRow {
  id: number;
  public_id: string;
  thumb_key: string | null;
  source: string;
  title: string;
  author: string | null;
  type: string;
  bike: string;
  visibility: string;
  thumb_sha: string | null;
  source_url: string | null;
  modified: string;
  first_seen: number;
  last_seen: number;
  label: string | null;
  seq: number | null;
  files: number;
  bytes: number;
}

export function cdnBase(env: Env): string {
  return (env.MXB_ASSETS_CDN || DEFAULT_CDN).replace(/\/+$/, "");
}

function summary(env: Env, r: AssetRow) {
  return {
    id: r.public_id,
    slug: modSlug(r.title),
    source: r.source,
    title: r.title,
    author: r.author,
    type: r.type,
    bike: r.bike ? r.bike.split("; ") : [],
    version: r.label,
    thumb: r.thumb_key ? `${cdnBase(env)}/${r.thumb_key}` : null,
    source_url: r.source_url,
    updated: r.modified,
    first_seen: new Date(r.first_seen).toISOString(),
    last_seen: new Date(r.last_seen).toISOString(),
    stored_files: r.files,
    bytes: r.bytes,
  };
}

/** The current version's label and seq, and what of it is stored. */
const COLUMNS = `a.id, a.public_id, a.thumb_key, a.source, a.title, a.author, a.type, a.bike, a.visibility, a.thumb_sha, a.source_url, a.modified,
  a.first_seen, a.last_seen, v.label, v.seq,
  (SELECT COUNT(*) FROM mod_files f WHERE f.version_id = v.id AND f.status = 'done') AS files,
  (SELECT COALESCE(SUM(b.size), 0) FROM mod_files f JOIN mod_blobs b ON b.sha256 = f.sha256
     WHERE f.version_id = v.id AND f.status = 'done') AS bytes`;
/** The whole mirrored catalogue is listed (metadata only); an upload once its version is live. */
const LISTABLE = `a.state = 'active' AND a.visibility = 'public' AND a.page_status <> 'gone' AND (a.source = 'mirror'
  OR (v.state = 'live' AND EXISTS (SELECT 1 FROM mod_files f WHERE f.version_id = v.id AND f.status = 'done')))`;

export async function searchAssets(url: URL, env: Env): Promise<{ status: number; body: unknown }> {
  const q = (url.searchParams.get("q") ?? "").slice(0, 200);
  const type = url.searchParams.get("type") ?? "";
  const source = url.searchParams.get("source") ?? "";
  const bike = (url.searchParams.get("bike") ?? "").trim().slice(0, 60);
  const page = Math.min(MAX_PAGE, Math.max(1, Math.floor(Number(url.searchParams.get("page") ?? "1")) || 1));
  if (type && !ASSET_TYPES.includes(type as never)) return { status: 400, body: { error: "unknown type" } };
  if (source && source !== "mirror" && source !== "upload") return { status: 400, body: { error: "unknown source" } };

  const where: string[] = [LISTABLE];
  const args: unknown[] = [];
  if (type) where.push("a.type = ?"), args.push(type);
  if (source) where.push("a.source = ?"), args.push(source);
  if (bike) {
    where.push("a.bike LIKE ? ESCAPE '\\'");
    args.push(`%${bike.replace(/[\\%_]/g, (c) => "\\" + c)}%`);
  }
  const match = ftsQuery(q);
  const offset = (page - 1) * PER_PAGE;
  const join = "LEFT JOIN mod_versions v ON v.id = a.current_version";

  let from: string;
  let order: string;
  let lead: unknown[];
  if (match) {
    from = `FROM mod_fts JOIN mod_assets a ON a.id = mod_fts.rowid ${join} WHERE mod_fts MATCH ? AND ${where.join(" AND ")}`;
    order = `bm25(mod_fts, ${WEIGHTS}), a.modified DESC`;
    lead = [match];
  } else {
    from = `FROM mod_assets a ${join} WHERE ${where.join(" AND ")}`;
    order = "a.modified DESC, a.id DESC";
    lead = [];
  }
  const counted = await env.DB.prepare(`SELECT COUNT(*) AS n ${from}`).bind(...lead, ...args).first<{ n: number }>();
  const total = counted?.n ?? 0;
  const { results } = await env.DB.prepare(`SELECT ${COLUMNS} ${from} ORDER BY ${order} LIMIT ? OFFSET ?`)
    .bind(...lead, ...args, PER_PAGE, offset)
    .all<AssetRow>();
  return {
    status: 200,
    body: { page, per_page: PER_PAGE, total, pages: Math.ceil(total / PER_PAGE), results: results.map((r) => summary(env, r)) },
  };
}

interface FileRow {
  idx: number;
  part: number;
  rel: string | null;
  url: string | null;
  host: string;
  label: string | null;
  is_server: number;
  is_default: number;
  status: string;
  sha256: string | null;
  filename: string | null;
  size: number | null;
  bucket: string | null;
  r2_key: string | null;
}

/** The version a request names (`?version=<seq>`), else the current one. */
async function versionOf(env: Env, assetId: number, url: URL): Promise<{ id: number; seq: number } | null> {
  const seq = url.searchParams.get("version");
  if (seq !== null) {
    if (!/^\d{1,6}$/.test(seq)) return null;
    return env.DB.prepare("SELECT id, seq FROM mod_versions WHERE asset_id = ? AND seq = ? AND state = 'live'")
      .bind(assetId, Number(seq))
      .first<{ id: number; seq: number }>();
  }
  return env.DB.prepare(
    "SELECT v.id, v.seq FROM mod_assets a JOIN mod_versions v ON v.id = a.current_version WHERE a.id = ? AND v.state = 'live'",
  )
    .bind(assetId)
    .first<{ id: number; seq: number }>();
}

/** A mirrored post whose page hasn't been read yet has no version: shown with no files. */
const NO_VERSION = { id: -1, seq: 0 };

/** An asset anyone may see: active (public or unlisted), not a page the source dropped. */
const VISIBLE = "a.state = 'active' AND a.page_status <> 'gone'";

export async function getAsset(id: number, url: URL, env: Env): Promise<{ status: number; body: unknown }> {
  const named = await versionOf(env, id, url);
  const isMirror = !named && !url.searchParams.has("version")
    ? await env.DB.prepare("SELECT 1 FROM mod_assets WHERE id = ? AND source = 'mirror'").bind(id).first()
    : null;
  const version = named ?? (isMirror ? NO_VERSION : null);
  if (!version) return { status: 404, body: { error: "no such mod" } };
  const asset = await env.DB.prepare(
    `SELECT ${COLUMNS}, a.description, a.categories, a.body FROM mod_assets a LEFT JOIN mod_versions v ON v.id = ?
     WHERE a.id = ? AND ${VISIBLE}`,
  )
    .bind(version.id, id)
    .first<AssetRow & { description: string; categories: string; body: string | null }>();
  if (!asset) return { status: 404, body: { error: "no such mod" } };
  const { results } = await env.DB.prepare(
    `SELECT f.idx, f.part, f.rel, f.url, f.host, f.label, f.is_server, f.is_default, f.status, f.sha256, f.filename,
       b.size, b.bucket, b.r2_key
     FROM mod_files f LEFT JOIN mod_blobs b ON b.sha256 = f.sha256
     WHERE f.version_id = ? ORDER BY f.idx, f.part`,
  )
    .bind(version.id)
    .all<FileRow>();
  const versions = await env.DB.prepare(
    "SELECT seq, label, notes, created_at FROM mod_versions WHERE asset_id = ? AND state = 'live' ORDER BY seq DESC LIMIT 50",
  )
    .bind(id)
    .all<{ seq: number; label: string | null; notes: string | null; created_at: number }>();
  // The post's own pictures, copied to the CDN by the sync (`mirror.ts` copyImages).
  const images = await env.DB.prepare(
    `SELECT b.r2_key, i.width, i.height FROM mod_asset_images i JOIN mod_blobs b ON b.sha256 = i.sha256
     WHERE i.asset_id = ? AND b.bucket = 'public' ORDER BY i.idx`,
  )
    .bind(id)
    .all<{ r2_key: string; width: number | null; height: number | null }>();
  const files = results.map((f) => {
    const stored = f.status === "done" && !!f.r2_key;
    const path = f.part > 0 ? `${f.idx}/${f.part}` : `${f.idx}`;
    return {
      idx: f.idx,
      part: f.part,
      path: f.rel,
      label: f.label,
      host: f.host,
      server: f.is_server === 1,
      recommended: f.is_default === 1,
      // stored: on our CDN. original: not mirrored (yet); the download goes to the source and
      // asks for a copy. folder: a share whose files are the parts listed after it.
      state: stored
        ? "stored"
        : f.status === "folder"
          ? "folder"
          : f.status === "failed" || f.status === "runner"
            ? "original"
            : f.status === "idle"
              ? "original"
              : "mirroring",
      filename: f.filename,
      size: f.size,
      sha256: stored ? f.sha256 : null,
      locked: f.bucket === "private",
      cdn: stored && f.bucket === "public" ? `${cdnBase(env)}/${f.r2_key}` : null,
      // Always a download: ours when stored, otherwise a redirect to the original link.
      download: f.url || stored ? `${url.origin}/v1/assets/${asset.public_id}/download/${path}?version=${version.seq}` : null,
      source: f.url,
    };
  });
  return {
    status: 200,
    body: {
      ...summary(env, asset),
      visibility: asset.visibility,
      version_seq: version.seq,
      description: asset.description,
      // The description as blocks (`modbody.ts`): text, marks, links, YouTube ids. Never HTML.
      body: asset.body ? safeBlocks(asset.body) : null,
      images: images.results.map((r) => ({ url: `${cdnBase(env)}/${r.r2_key}`, width: r.width, height: r.height })),
      categories: asset.categories ? asset.categories.split("; ") : [],
      versions: versions.results.map((v) => ({ ...v, created_at: new Date(v.created_at).toISOString() })),
      files,
    },
  };
}

/** 302 to the file: the CDN for public files, a short-lived signed link for locked ones. */
export async function downloadAsset(
  id: number,
  idx: number,
  part: number,
  url: URL,
  env: Env,
  now = Date.now(),
): Promise<Response> {
  const visible = await env.DB.prepare(`SELECT 1 FROM mod_assets a WHERE a.id = ? AND ${VISIBLE}`).bind(id).first();
  const version = visible ? await versionOf(env, id, url) : null;
  if (!version) return publicJson(404, { error: "no such mod" });
  const file = await env.DB.prepare(
    `SELECT f.url, f.status, a.source, a.type, a.bike, b.bucket, b.r2_key, b.sha256
     FROM mod_files f JOIN mod_versions v ON v.id = f.version_id JOIN mod_assets a ON a.id = v.asset_id
     LEFT JOIN mod_blobs b ON b.sha256 = f.sha256
     WHERE f.version_id = ? AND f.idx = ? AND f.part = ?`,
  )
    .bind(version.id, idx, part)
    .first<{ url: string | null; status: string; source: string; type: string; bike: string; bucket: string | null; r2_key: string | null; sha256: string | null }>();
  if (!file) return publicJson(404, { error: "no such file" });
  if (file.status !== "done" || !file.r2_key || !file.sha256) {
    // Not mirrored: this download goes to the original, and asks for a copy for the next one.
    if (!file.url) return publicJson(404, { error: "not stored" });
    await requestMirror(env, version.id, idx, part, file, now);
    return redirect(file.url, "no-store");
  }
  const row = { bucket: file.bucket, r2_key: file.r2_key, sha256: file.sha256 };
  await touchBlob(env, row.sha256, now);
  if (row.bucket === "private") {
    const exp = Math.floor(now / 1000) + SIGNED_TTL_S;
    const sig = await signLocked(env, row.sha256, exp);
    if (!sig) return publicJson(503, { error: "locked downloads are not configured" });
    return redirect(`${url.origin}/v1/assets/locked/${row.sha256}?exp=${exp}&sig=${sig}`);
  }
  return redirect(`${cdnBase(env)}/${row.r2_key}`);
}

/**
 * Someone wants a mirrored post's file that isn't in R2: queue it now, unless it is content the
 * policy doesn't keep (`mirrorpolicy.ts`), already on its way, or known not to be fetchable.
 */
async function requestMirror(
  env: Env,
  version: number,
  idx: number,
  part: number,
  file: { status: string; source: string; type: string; bike: string },
  now: number,
): Promise<void> {
  if (file.source !== "mirror" || file.status !== "idle") return;
  if (!supported(file, await activeBikes(env, now)).ok) return;
  const claimed = await env.DB.prepare(
    "UPDATE mod_files SET status = 'queued', leased_until = ? WHERE version_id = ? AND idx = ? AND part = ? AND status = 'idle'",
  )
    .bind(now + LEASE_MS, version, idx, part)
    .run();
  if (claimed.meta.changes && env.MIRROR_QUEUE) {
    await env.MIRROR_QUEUE.send({ kind: "file", version, idx, part } satisfies MirrorJob);
  }
}

/** Where a file stands for a client waiting on it (`POST …/prepare/<idx>[/<part>]`). */
export type PrepareState =
  /** On our CDN: `download` serves it. */
  | { state: "stored"; download: string }
  /** A copy is queued or on its way: ask again shortly. */
  | { state: "mirroring" }
  /** A share whose files are listed as parts: read the mod again. */
  | { state: "folder" }
  /** Not ours and not coming (a host refusal, a quota, too big, policy): use `source`. */
  | { state: "original"; source: string | null };

/**
 * Queue the copy of a file that isn't stored, and say where it stands.
 *
 * What mxbsecure.com's Download button and the MXB App poll instead of being sent to the
 * original host: idempotent, so asking again only reports. A file the mirror gave up on, or is
 * backing off from (a Drive quota answers like that), is `original` at once rather than a wait.
 */
export async function prepareFile(
  id: number,
  idx: number,
  part: number,
  url: URL,
  env: Env,
  now = Date.now(),
): Promise<{ status: number; body: PrepareState | { error: string } }> {
  const visible = await env.DB.prepare(`SELECT 1 FROM mod_assets a WHERE a.id = ? AND ${VISIBLE}`).bind(id).first();
  const version = visible ? await versionOf(env, id, url) : null;
  if (!version) return { status: 404, body: { error: "no such mod" } };
  const file = await env.DB.prepare(
    `SELECT f.url, f.status, a.public_id, a.source, a.type, a.bike, b.r2_key
     FROM mod_files f JOIN mod_versions v ON v.id = f.version_id JOIN mod_assets a ON a.id = v.asset_id
     LEFT JOIN mod_blobs b ON b.sha256 = f.sha256
     WHERE f.version_id = ? AND f.idx = ? AND f.part = ?`,
  )
    .bind(version.id, idx, part)
    .first<{ url: string | null; status: string; public_id: string; source: string; type: string; bike: string; r2_key: string | null }>();
  if (!file) return { status: 404, body: { error: "no such file" } };
  if (file.status === "done" && file.r2_key) {
    const path = part > 0 ? `${idx}/${part}` : `${idx}`;
    return { status: 200, body: { state: "stored", download: `${url.origin}/v1/assets/${file.public_id}/download/${path}?version=${version.seq}` } };
  }
  if (file.status === "folder") return { status: 200, body: { state: "folder" } };
  if (file.status === "idle") {
    await requestMirror(env, version.id, idx, part, file, now);
    const after = await env.DB.prepare("SELECT status FROM mod_files WHERE version_id = ? AND idx = ? AND part = ?")
      .bind(version.id, idx, part)
      .first<{ status: string }>();
    file.status = after?.status ?? file.status;
  }
  if (file.status === "queued" || file.status === "pending") return { status: 200, body: { state: "mirroring" } };
  return { status: 200, body: { state: "original", source: file.url } };
}

/** A mirrored mod by its source post's slug: how the MXB App, which browses mxb-mods, finds our copy. */
async function mirroredBySlug(env: Env, slug: string): Promise<number | null> {
  const row = await env.DB.prepare(
    "SELECT id FROM mod_assets WHERE source = 'mirror' AND slug = ? AND state = 'active' ORDER BY id DESC LIMIT 1",
  )
    .bind(slug)
    .first<{ id: number }>();
  return row?.id ?? null;
}

/** Serve a locked blob to whoever holds a valid, unexpired signature for it. */
export async function lockedAsset(sha: string, url: URL, env: Env, now = Date.now()): Promise<Response> {
  const exp = Number(url.searchParams.get("exp"));
  const sig = url.searchParams.get("sig") ?? "";
  if (!Number.isFinite(exp) || exp < Math.floor(now / 1000)) return publicJson(403, { error: "expired" });
  const want = await signLocked(env, sha, exp);
  if (!want || !timingSafeEqual(want, sig)) return publicJson(403, { error: "bad signature" });
  const blob = await env.DB.prepare("SELECT r2_key, filename FROM mod_blobs WHERE sha256 = ? AND bucket = 'private'")
    .bind(sha)
    .first<{ r2_key: string; filename: string | null }>();
  const obj = blob ? await env.ASSET_LOCKED.get(blob.r2_key) : null;
  if (!blob || !obj) return publicJson(404, { error: "gone" });
  return new Response(obj.body, {
    headers: {
      "content-type": "application/octet-stream",
      "content-length": String(obj.size),
      "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(blob.filename ?? `${sha}.mxbsecure`)}`,
      "cache-control": "private, no-store",
    },
  });
}

export async function signLocked(env: Env, sha: string, exp: number): Promise<string | null> {
  const secret = env.MXB_ASSET_URL_KEY;
  if (!secret) return null;
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, [
    "sign",
  ]);
  return hex(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(`${sha}.${exp}`)));
}

function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}

function redirect(location: string, cache = "public, max-age=60"): Response {
  return new Response(null, {
    status: 302,
    headers: { location, "access-control-allow-origin": "*", "cache-control": cache },
  });
}

/** A state that changes from one poll to the next: never cached. */
function noStoreJson(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "access-control-allow-origin": "*", "cache-control": "no-store" },
  });
}

function publicJson(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
      "access-control-allow-origin": "*",
      "cache-control": status === 200 ? "public, max-age=60" : "no-store",
    },
  });
}

const ONE = new RegExp(`^/v1/assets/(${UUID_RE.source})$`, "i");
const DOWNLOAD = new RegExp(`^/v1/assets/(${UUID_RE.source})/download/([0-9]{1,4})(?:/([0-9]{1,4}))?$`, "i");
const REPORT = new RegExp(`^/v1/assets/(${UUID_RE.source})/report$`, "i");
const PREPARE = new RegExp(`^/v1/assets/(${UUID_RE.source})/prepare/([0-9]{1,4})(?:/([0-9]{1,4}))?$`, "i");
/** mxb-mods slugs: lowercase words and dashes (WordPress also percent-encodes non-ASCII ones). */
const BY_SLUG = /^\/v1\/assets\/mirror\/([a-z0-9%_-]{1,200})$/i;
const LEGACY = /^\/v1\/assets\/[0-9]{1,12}(?:\/(?:download\/.*|report))?$/;

/** How many mods search lists with no filter: the site's headline number. */
/** How many mods are listed, in all and per type. One query; the route caches it a minute. */
export async function assetStats(env: Env): Promise<{ total: number; by_type: Record<AssetType, number> }> {
  const { results } = await env.DB.prepare(
    `SELECT a.type, COUNT(*) AS n FROM mod_assets a LEFT JOIN mod_versions v ON v.id = a.current_version
     WHERE ${LISTABLE} GROUP BY a.type`,
  ).all<{ type: string; n: number }>();
  const by_type = Object.fromEntries(ASSET_TYPES.map((t) => [t, 0])) as Record<AssetType, number>;
  let total = 0;
  for (const r of results) {
    total += r.n;
    if ((ASSET_TYPES as string[]).includes(r.type)) by_type[r.type as AssetType] = r.n;
  }
  return { total, by_type };
}

/** The public `/v1/assets/*` routes. `null` for anything else (the owner's writes, below the gate). */
export async function publicModRoutes(request: Request, url: URL, env: Env): Promise<Response | null> {
  const path = url.pathname;
  const m = request.method;
  const isSearch = path === "/v1/assets/search";
  const isStats = path === "/v1/assets/stats";
  const one = ONE.exec(path);
  const dl = DOWNLOAD.exec(path);
  const locked = /^\/v1\/assets\/locked\/([0-9a-f]{64})$/.exec(path);
  const report = REPORT.exec(path);
  const prepare = PREPARE.exec(path);
  const bySlug = BY_SLUG.exec(path);
  const reading = m === "GET" || m === "HEAD";
  if (m === "OPTIONS" && (report || prepare)) {
    return new Response(null, {
      status: 204,
      headers: {
        "access-control-allow-origin": "*",
        "access-control-allow-methods": "POST, OPTIONS",
        "access-control-allow-headers": "Content-Type",
      },
    });
  }
  // The old integer addresses name nothing now: a plain 404, not the account gate's 401.
  if ((reading || m === "POST") && LEGACY.test(path)) return publicJson(404, { error: "no such mod" });
  if (!(reading && (isSearch || isStats || one || dl || locked || bySlug)) && !(m === "POST" && (report || prepare))) return null;

  const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
  if (env.ASSETS_LIMITER && !(await env.ASSETS_LIMITER.limit({ key: ip })).success) {
    return publicJson(429, { error: "slow down" });
  }
  if (isSearch) {
    const r = await searchAssets(url, env);
    return publicJson(r.status, r.body);
  }
  if (isStats) return publicJson(200, await assetStats(env));
  if (bySlug) {
    const id = await mirroredBySlug(env, bySlug[1].toLowerCase());
    if (id === null) return publicJson(404, { error: "no such mod" });
    const r = await getAsset(id, url, env);
    return publicJson(r.status, r.body);
  }
  const named = one?.[1] ?? dl?.[1] ?? report?.[1] ?? prepare?.[1];
  if (named) {
    const id = await internalId(env, named);
    if (id === null) return publicJson(404, { error: "no such mod" });
    if (report) {
      const r = await reportAsset(request, id, env);
      return publicJson(r.status, r.body);
    }
    if (prepare) {
      const r = await prepareFile(id, Number(prepare[2]), Number(prepare[3] ?? "0"), url, env);
      return noStoreJson(r.status, r.body);
    }
    if (one) {
      const r = await getAsset(id, url, env);
      return publicJson(r.status, r.body);
    }
    return downloadAsset(id, Number(dl![2]), Number(dl![3] ?? "0"), url, env);
  }
  return lockedAsset(locked![1], url, env);
}
