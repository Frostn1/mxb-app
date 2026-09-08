/**
 * Share codes that keep pointing at the current version.
 *
 * An `MXBS1-` code is the whole share: the item list and the catbox URLs are baked into the
 * string. That is what makes it work with no server at all, and it is also why a track
 * author who recompiles has to send a new code round every time.
 *
 * A live share splits the two. catbox still holds the bytes — it is good at that and it
 * costs nothing — and this holds the *pointer*: a short permanent code, a version number,
 * and the manifest that version resolves to. Publish an update and every subscriber's next
 * check finds a higher version.
 *
 * **There is no account behind any of this.** Publishing does not require enrollment, a
 * token or a sign-in, because most people who run the app have none of those and a share
 * nobody can make is not a feature. What stands in for ownership is an update key minted at
 * first publish: the app stores it and never shows it, so publishing an update is one
 * button, while a stranger holding the public code cannot overwrite a track under it for
 * everyone who subscribed.
 *
 * That trade puts the whole weight of safety on validation, since the write path is open.
 * `validManifest` is therefore not a formality — it is the only thing between an arbitrary
 * POST and files landing in someone's mods folder.
 */

import { hashToken, newToken } from "./auth";
import { isShareRel } from "./validate";

/**
 * Crockford base32: no I, L, O or U. The first three because a code gets read down a
 * Discord voice channel, the last because excluding it is what stops the generator spelling
 * something unfortunate.
 */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** Characters per code. 32^8 is 1.1e12 — far past guessing, still one glance to type. */
const CODE_LEN = 8;

/** What the app prefixes a live code with, the way `MXBS1-` marks a static one. */
export const CODE_PREFIX = "MXBL1-";

/** Manifest ceiling. A 512-item share serialises well under this; a megabyte of JSON is
 *  someone using the table as storage. */
const MAX_MANIFEST_BYTES = 64 * 1024;

const MAX_NAME = 64;
const MAX_ITEMS = 512;

/**
 * Hosts a manifest may point at.
 *
 * Fail closed: a bundle URL somewhere else is refused rather than stored, so this table can
 * never be used to hand out a link to anything we did not upload. Adding an upload host to
 * the app means adding it here too, and a publish that fails until you do is the correct
 * order of those two changes.
 */
const HOSTS = ["files.catbox.moe", "litter.catbox.moe"];

/** A fresh code. `crypto.getRandomValues`, never `Math.random`. */
export function newCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(CODE_LEN));
  let out = "";
  for (const b of bytes) out += ALPHABET[b % ALPHABET.length];
  return out;
}

/**
 * Read a code the way it might be typed rather than the way it was printed: with or without
 * the prefix, in any case, with the dashes and spaces someone added to make it readable, and
 * with the letters Crockford treats as digits folded in (`O` is a zero, `I` and `L` are
 * ones). Returns null if what is left is not a code.
 */
export function normaliseCode(text: unknown): string | null {
  if (typeof text !== "string") return null;
  let s = text.trim().toUpperCase();
  if (s.startsWith(CODE_PREFIX)) s = s.slice(CODE_PREFIX.length);
  s = s.replace(/[\s-]/g, "").replace(/O/g, "0").replace(/[IL]/g, "1");
  if (s.length !== CODE_LEN) return null;
  for (const c of s) if (!ALPHABET.includes(c)) return null;
  return s;
}

/** One entry of a manifest, as the app writes it. */
interface ShareItem {
  name: string;
  rel: string;
  size: number;
  isDir: boolean;
}

/** The app's `FileShare`, which is what a manifest is. */
export interface Manifest {
  items: ShareItem[];
  totalSize: number;
  bundle: { url: string; host: string; size: number; parts?: string[]; partSizes?: number[] };
}

function isAllowedUrl(value: unknown): boolean {
  if (typeof value !== "string") return false;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return false;
  }
  return url.protocol === "https:" && HOSTS.includes(url.hostname.toLowerCase());
}

/**
 * Everything the write path is allowed to assume about a manifest.
 *
 * The `rel` check is the important one and it is deliberately the same rule the app applies
 * to a pasted code (`library::is_safe_rel`): every rel is joined onto the receiver's mods
 * root at install time, so one containing `..` would write outside it. The app checks it,
 * and it is checked again here, because a publish is a plain HTTP POST that nothing says
 * came from the app.
 */
export function validManifest(value: unknown): value is Manifest {
  if (!value || typeof value !== "object") return false;
  const m = value as Record<string, unknown>;

  if (!Array.isArray(m.items) || m.items.length === 0 || m.items.length > MAX_ITEMS) return false;
  for (const raw of m.items) {
    if (!raw || typeof raw !== "object") return false;
    const item = raw as Record<string, unknown>;
    if (typeof item.name !== "string" || item.name.length === 0 || item.name.length > 256) {
      return false;
    }
    if (!isShareRel(item.rel)) return false;
    if (typeof item.size !== "number" || !Number.isFinite(item.size) || item.size < 0) return false;
    if (typeof item.isDir !== "boolean") return false;
  }

  if (typeof m.totalSize !== "number" || !Number.isFinite(m.totalSize) || m.totalSize < 0) {
    return false;
  }

  const bundle = m.bundle as Record<string, unknown> | undefined;
  if (!bundle || typeof bundle !== "object") return false;
  if (!isAllowedUrl(bundle.url)) return false;
  if (typeof bundle.host !== "string" || bundle.host.length > 32) return false;
  if (typeof bundle.size !== "number" || !Number.isFinite(bundle.size) || bundle.size < 0) {
    return false;
  }
  if (bundle.parts !== undefined) {
    if (!Array.isArray(bundle.parts) || bundle.parts.length > MAX_ITEMS) return false;
    if (!bundle.parts.every(isAllowedUrl)) return false;
  }
  if (bundle.partSizes !== undefined) {
    if (!Array.isArray(bundle.partSizes)) return false;
    if (!bundle.partSizes.every((n) => typeof n === "number" && Number.isFinite(n) && n >= 0)) {
      return false;
    }
  }
  return true;
}

/** Spaces and punctuation are fine — a track is called "RedBud 2026". Control characters
 *  are not: this string is rendered in a list and echoed back in JSON. */
function validName(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const name = value.trim();
  // eslint-disable-next-line no-control-regex
  return name.length > 0 && name.length <= MAX_NAME && !/[\u0000-\u001f\u007f]/.test(name);
}

/** The row, as every route here reads it. */
interface ShareRow {
  code: string;
  update_hash: string;
  name: string;
  version: number;
  manifest: string;
  size: number;
  created_at: number;
  updated_at: number;
}

/** What a subscriber is handed. The update key is never in here. */
function body(row: ShareRow): Record<string, unknown> {
  return {
    code: CODE_PREFIX + row.code,
    name: row.name,
    version: row.version,
    size: row.size,
    updatedAt: row.updated_at,
    manifest: JSON.parse(row.manifest) as Manifest,
  };
}

/**
 * Mint a code. `POST /v1/share`, unauthenticated.
 *
 * Answers with the update key exactly once — it is stored only as a digest, so this response
 * is the only time it exists in a form anyone can use. The app writes it to its config
 * before showing the code.
 */
export async function publishShare(request: Request, env: Env): Promise<Response> {
  const payload = await readJson(request);
  if (!payload || typeof payload !== "object") return json(400, { error: "expected an object" });
  const { name, manifest } = payload as Record<string, unknown>;
  if (!validName(name)) return json(400, { error: "bad name" });
  if (!validManifest(manifest)) return json(400, { error: "bad manifest" });

  const text = JSON.stringify(manifest);
  if (text.length > MAX_MANIFEST_BYTES) return json(413, { error: "manifest too large" });

  const updateKey = newToken();
  const now = Math.floor(Date.now() / 1000);

  // Retried rather than assumed unique: 32^8 makes a clash vanishingly rare, and
  // `INSERT ... ON CONFLICT DO NOTHING` turns the rare one into another draw instead of a
  // 500 and a lost upload.
  for (let attempt = 0; attempt < 5; attempt++) {
    const code = newCode();
    const done = await env.DB.prepare(
      `INSERT INTO live_shares
         (code, update_hash, name, version, manifest, size, created_at, updated_at)
       VALUES (?, ?, ?, 1, ?, ?, ?, ?)
       ON CONFLICT (code) DO NOTHING
       RETURNING code`,
    )
      .bind(
        code,
        await hashToken(updateKey),
        (name as string).trim(),
        text,
        manifest.totalSize,
        now,
        now,
      )
      .first<{ code: string }>();
    if (done) {
      return json(201, {
        code: CODE_PREFIX + code,
        updateKey,
        name: (name as string).trim(),
        version: 1,
        size: manifest.totalSize,
        updatedAt: now,
      });
    }
  }
  return json(503, { error: "couldn't mint a code" });
}

/**
 * Replace what a code points at. `PUT /v1/share/:code`, `X-Update-Key`.
 *
 * The version is bumped in the same statement that writes the manifest, so two publishes
 * racing each other produce two versions rather than one overwriting the other's number.
 */
export async function updateShare(request: Request, rawCode: string, env: Env): Promise<Response> {
  const code = normaliseCode(rawCode);
  if (!code) return json(404, { error: "no such share code" });

  const key = request.headers.get("x-update-key")?.trim();
  if (!key) return json(401, { error: "this share needs its update key" });

  const payload = await readJson(request);
  if (!payload || typeof payload !== "object") return json(400, { error: "expected an object" });
  const { name, manifest } = payload as Record<string, unknown>;
  if (!validName(name)) return json(400, { error: "bad name" });
  if (!validManifest(manifest)) return json(400, { error: "bad manifest" });

  const text = JSON.stringify(manifest);
  if (text.length > MAX_MANIFEST_BYTES) return json(413, { error: "manifest too large" });

  // Matched inside the statement, by digest. Same reasoning as account tokens: the compare
  // happens in the index, so there is no string comparison of ours to leak timing, and a
  // wrong key is indistinguishable from a code that does not exist.
  const now = Math.floor(Date.now() / 1000);
  const row = await env.DB.prepare(
    `UPDATE live_shares
        SET name = ?, manifest = ?, size = ?, version = version + 1, updated_at = ?
      WHERE code = ? AND update_hash = ?
      RETURNING version`,
  )
    .bind((name as string).trim(), text, manifest.totalSize, now, code, await hashToken(key))
    .first<{ version: number }>();

  if (!row) return json(403, { error: "that update key doesn't own this code" });
  return json(200, {
    code: CODE_PREFIX + code,
    name: (name as string).trim(),
    version: row.version,
    size: manifest.totalSize,
    updatedAt: now,
  });
}

/**
 * What a code points at now. `GET /v1/share/:code`, unauthenticated.
 *
 * The version is the ETag, so a subscriber that already has it sends `If-None-Match` and
 * gets a 304 with no body. That is the shape of nearly every check — the manifest only
 * crosses the wire when there is genuinely something new — which is what keeps a few
 * thousand followers inside the request budget.
 */
export async function readShare(request: Request, rawCode: string, env: Env): Promise<Response> {
  const code = normaliseCode(rawCode);
  if (!code) return json(404, { error: "no such share code" });

  const row = await env.DB.prepare(
    `SELECT code, update_hash, name, version, manifest, size, created_at, updated_at
       FROM live_shares WHERE code = ?`,
  )
    .bind(code)
    .first<ShareRow>();
  if (!row) return json(404, { error: "no such share code" });

  const etag = `"${row.version}"`;
  const headers = { "content-type": "application/json", etag, "cache-control": "no-cache" };
  // Split on commas rather than compared whole: a conditional request may carry a list, and
  // a proxy is entitled to weaken the tag it echoes back.
  const seen = (request.headers.get("if-none-match") ?? "")
    .split(",")
    .map((t) => t.trim().replace(/^W\//, ""));
  if (seen.includes(etag)) return new Response(null, { status: 304, headers });

  return new Response(JSON.stringify(body(row)), { status: 200, headers });
}

/** Stop serving a code. `DELETE /v1/share/:code`, `X-Update-Key`. */
export async function deleteShare(request: Request, rawCode: string, env: Env): Promise<Response> {
  const code = normaliseCode(rawCode);
  if (!code) return json(404, { error: "no such share code" });
  const key = request.headers.get("x-update-key")?.trim();
  if (!key) return json(401, { error: "this share needs its update key" });

  const gone = await env.DB.prepare(
    `DELETE FROM live_shares WHERE code = ? AND update_hash = ? RETURNING code`,
  )
    .bind(code, await hashToken(key))
    .first<{ code: string }>();
  if (!gone) return json(403, { error: "that update key doesn't own this code" });
  return json(200, { ok: true });
}

async function readJson(request: Request): Promise<unknown | null> {
  try {
    return await request.json();
  } catch {
    return null;
  }
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}
