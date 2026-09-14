/**
 * Secured assets, managed from the website.
 *
 * mxbsecure.com is where assets are created and where the list of who may open each one is
 * kept. Those are the two admin writes here: minting an asset (a fresh content key, stored
 * wrapped, handed back once) and changing its grants. Everything downstream — the check and
 * the key release in `index.ts` — already reads these same rows, so a grant made here is
 * honoured by `/v1/keys/grant` with nothing else to wire.
 *
 * Behind `ADMIN_KEY` like the rest of `/admin`, and the only admin routes with CORS: the site
 * calls them from a browser, so the browser has to be told it may. Nothing else gets the
 * headers — the other admin pages are opened by typing a URL, not called cross-origin.
 */

import { currentMasterVersion, wrapContentKey } from "./assetkey";
import { tokenMatches } from "./auth";
import { isSteamId64, steamPersonaName } from "./steam";
import { adminAllowed } from "./usage";
import { webSession } from "./websession";

/** The site's origins, allowed to call these routes from a browser. */
export const SITE_ORIGINS = ["https://mxbsecure.com", "https://www.mxbsecure.com"];

/** A local build of the site. Only with `MXB_ALLOW_DEV_ORIGINS=1`, never in production. */
const DEV_ORIGINS = ["http://localhost:5173", "http://127.0.0.1:5173"];

export function assetOrigins(env: Env): string[] {
  return env.MXB_ALLOW_DEV_ORIGINS === "1" ? [...SITE_ORIGINS, ...DEV_ORIGINS] : SITE_ORIGINS;
}

/** The request's Origin if it's one of ours, else null. */
export function allowedOrigin(request: Request, env: Env): string | null {
  const origin = request.headers.get("Origin");
  return origin && assetOrigins(env).includes(origin) ? origin : null;
}

/**
 * A 403 unless this write came from our own site, as JSON; null when it may go ahead.
 *
 * For requests the sign-in cookie authorizes. The cookie is SameSite=Lax, which keeps other
 * sites out but not a sibling subdomain: a page there could send a plain-text POST, with no
 * preflight, and the cookie would ride along. It can't send our Origin, and a JSON content type
 * would need a preflight we refuse.
 */
export function refuseCrossSiteWrite(request: Request, env: Env): Response | null {
  if (request.method === "GET" || request.method === "HEAD") return null;
  const type = (request.headers.get("Content-Type") ?? "").split(";")[0].trim().toLowerCase();
  if (allowedOrigin(request, env) && type === "application/json") return null;
  return json(403, { error: "that request didn't come from mxbsecure.com" });
}

/** A key (every asset) or a signed-in creator (only their own). */
type Scope = { kind: "admin" } | { kind: "creator"; accountId: string };

/** Most inputs one grants change may carry, adds and removes together. */
export const MAX_GRANT_CHANGES = 100;

/** Longest single grants input. A profile URL is well under this. */
export const MAX_GRANT_INPUT = 256;

/** Largest JSON body these routes read. */
export const MAX_BODY_BYTES = 64 * 1024;

/** Most accounts `?names=1` looks up; the rest come back with a blank name. */
export const MAX_NAMED = 50;

/** Which master-key version wraps a new asset's content key. */
const KEY_ID = "k1";

/** How long to wait on Steam when resolving a custom URL name. */
const VANITY_TIMEOUT_MS = 8_000;

/** Lookups in flight at once. Polite to Steam, and under the Worker's connection limit. */
const VANITY_CONCURRENCY = 6;

/** A Steam custom URL name. Steam allows letters, digits, `_` and `-`. */
const VANITY = /^[A-Za-z0-9_-]{2,32}$/;

/** Is this one of the routes below? */
export function isAssetsPath(path: string): boolean {
  return path === "/admin/assets" || path.startsWith("/admin/assets/");
}

/**
 * Every `/admin/assets*` request, CORS included.
 *
 * The preflight is answered before auth: the browser sends it without the `Authorization`
 * header, so gating it would block the real request it is asking about. A preflight from an
 * origin not on the list gets a 403 and no CORS headers.
 */
export async function adminAssets(
  request: Request,
  url: URL,
  env: Env,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  const origin = request.headers.get("Origin");
  const allowed = allowedOrigin(request, env);

  if (request.method === "OPTIONS") {
    if (origin && !allowed) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), allowed, true);
  }

  let response: Response;
  try {
    response = await handle(request, url, env, fetchImpl);
  } catch (err) {
    if (err instanceof BodyTooLarge) {
      response = json(413, { error: `the request body is over ${MAX_BODY_BYTES / 1024} KB` });
    } else {
      // Caught here rather than by the router so the site still gets a readable 500.
      console.error(JSON.stringify({ msg: "admin assets failed", error: String(err) }));
      response = json(500, { error: "internal error" });
    }
  }
  return cors(response, allowed);
}

/**
 * The site's own key. It opens these routes and nothing else in `/admin`, so the key a browser
 * keeps can't read the dashboards. Bearer only — it is never typed into a URL.
 */
function assetsKeyMatches(request: Request, env: Env): boolean {
  const expected = env.MXB_ASSETS_KEY;
  const presented = /^Bearer\s+(.+)$/i.exec(request.headers.get("Authorization")?.trim() ?? "")?.[1];
  return !!expected && !!presented && tokenMatches(expected, presented);
}

/** Who's asking. A key wins over a cookie; a signed-in Steam account must be a creator. */
async function authorize(request: Request, url: URL, env: Env): Promise<Scope | Response> {
  if (assetsKeyMatches(request, env)) return { kind: "admin" };
  const admin = adminAllowed(request, url, env);
  if (admin === "ok") return { kind: "admin" };
  const session = await webSession(request, env);
  if (session) {
    const account = await env.DB.prepare("SELECT id FROM accounts WHERE steam_id = ? AND creator_at IS NOT NULL")
      .bind(session.steamId)
      .first<{ id: string }>();
    return account ? { kind: "creator", accountId: account.id } : json(403, { error: "this Steam account isn't a creator" });
  }
  if (admin === "unset" && !env.MXB_ASSETS_KEY && !env.MXB_WEB_SESSION_KEY) {
    return json(503, { error: "no admin key is configured" });
  }
  return json(401, { error: "unauthorized" });
}

async function handle(
  request: Request,
  url: URL,
  env: Env,
  fetchImpl: typeof fetch,
): Promise<Response> {
  const scope = await authorize(request, url, env);
  if (scope instanceof Response) return scope;
  // A key is presented on purpose, from curl or the site; only the cookie arrives on its own.
  if (scope.kind === "creator") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return refused;
  }
  if (!currentMasterVersion(env) || (scope.kind === "admin" && !env.MXB_OWNER_ACCOUNT_ID)) {
    return json(503, { error: "secured assets are not configured" });
  }

  const match = /^\/admin\/assets(?:\/([A-Za-z0-9_-]{1,64})(?:\/(grants|usage))?)?\/?$/.exec(url.pathname);
  if (!match) return json(404, { error: "no such endpoint" });
  const [, assetId, sub] = match;
  const method = request.method;
  // Steam display names for the lists, only when the page asks: they cost a lookup per buyer.
  const names = url.searchParams.get("names") === "1" ? fetchImpl : null;

  if (!assetId) {
    if (method === "GET") return listAssets(env, scope);
    if (method === "POST") {
      return createAsset(request, env, scope.kind === "creator" ? scope.accountId : env.MXB_OWNER_ACCOUNT_ID!);
    }
    return json(405, { error: "method not allowed" });
  }
  // A creator's view stops at their own assets: anyone else's looks like it doesn't exist.
  if (scope.kind === "creator" && !(await ownedBy(assetId, scope.accountId, env))) {
    return json(404, { error: "no such asset" });
  }
  if (sub === "grants") {
    if (method === "GET") return listGrants(assetId, env, names);
    if (method === "POST") return changeGrants(request, assetId, env, fetchImpl);
  } else if (sub === "usage") {
    if (method === "GET") return assetUsage(assetId, env, names);
  } else if (method === "PATCH") {
    return updateAsset(request, assetId, env, scope);
  }
  return json(405, { error: "method not allowed" });
}

async function ownedBy(assetId: string, accountId: string, env: Env): Promise<boolean> {
  const row = await env.DB.prepare("SELECT id FROM assets WHERE id = ? AND creator_id = ?").bind(assetId, accountId).first();
  return row !== null;
}

/**
 * `PATCH /admin/assets/:id` — `{ blobSha256?, withdrawn?, takenDown? }`.
 *
 * `blobSha256` is the packed file's hash. The site packs in the browser, so it's the only thing
 * that sees the finished file; once stored, `/v1/keys/grant` releases the key only for it.
 * `withdrawn: true` stops every new key release for the asset, `false` undoes that.
 * `takenDown` is the operator's own switch, apart from `withdrawn` so a creator can't undo it:
 * a key only, never the sign-in cookie.
 */
async function updateAsset(request: Request, assetId: string, env: Env, scope: Scope): Promise<Response> {
  const body = (await readJson(request)) as { blobSha256?: unknown; withdrawn?: unknown; takenDown?: unknown } | null;
  if (!body || typeof body !== "object") return json(400, { error: "expected a JSON body" });
  const { blobSha256, withdrawn, takenDown } = body;
  if (blobSha256 === undefined && withdrawn === undefined && takenDown === undefined) {
    return json(400, { error: "nothing to change" });
  }
  if (blobSha256 !== undefined && (typeof blobSha256 !== "string" || !/^[0-9a-f]{64}$/i.test(blobSha256.trim()))) {
    return json(400, { error: "blobSha256 must be 64 hex characters" });
  }
  if (withdrawn !== undefined && typeof withdrawn !== "boolean") {
    return json(400, { error: "withdrawn must be true or false" });
  }
  if (takenDown !== undefined && scope.kind !== "admin") {
    return json(403, { error: "only mxbsecure can take an asset down or restore it" });
  }
  if (takenDown !== undefined && typeof takenDown !== "boolean") {
    return json(400, { error: "takenDown must be true or false" });
  }
  if (!(await assetExists(assetId, env))) return json(404, { error: "no such asset" });

  const statements = [];
  if (typeof blobSha256 === "string") {
    statements.push(
      env.DB.prepare("UPDATE assets SET blob_sha256 = ? WHERE id = ?").bind(blobSha256.trim().toLowerCase(), assetId),
    );
  }
  if (withdrawn === true) {
    statements.push(env.DB.prepare("UPDATE assets SET withdrawn_at = COALESCE(withdrawn_at, ?) WHERE id = ?").bind(Date.now(), assetId));
  } else if (withdrawn === false) {
    statements.push(env.DB.prepare("UPDATE assets SET withdrawn_at = NULL WHERE id = ?").bind(assetId));
  }
  if (takenDown === true) {
    statements.push(
      env.DB.prepare("UPDATE assets SET taken_down_at = COALESCE(taken_down_at, ?) WHERE id = ?").bind(Date.now(), assetId),
    );
  } else if (takenDown === false) {
    statements.push(env.DB.prepare("UPDATE assets SET taken_down_at = NULL WHERE id = ?").bind(assetId));
  }
  await env.DB.batch(statements);
  const row = await env.DB.prepare("SELECT blob_sha256, withdrawn_at, taken_down_at FROM assets WHERE id = ?")
    .bind(assetId)
    .first<{ blob_sha256: string | null; withdrawn_at: number | null; taken_down_at: number | null }>();
  return json(200, {
    assetId,
    blobSha256: row?.blob_sha256 ?? null,
    withdrawnAt: row?.withdrawn_at ?? null,
    takenDownAt: row?.taken_down_at ?? null,
  });
}

/**
 * `POST /admin/assets` — a new asset and its content key.
 *
 * The key is 32 random bytes, stored only wrapped under the master key. The raw bytes are in
 * this response and nowhere else, ever: the packer needs them once, to seal the blob.
 */
async function createAsset(request: Request, env: Env, owner: string): Promise<Response> {
  const body = await readJson(request);
  const title = (body as { title?: unknown } | null)?.title;
  if (typeof title !== "string" || !title.trim() || title.trim().length > 200) {
    return json(400, { error: "title must be 1 to 200 characters" });
  }

  const account = await env.DB.prepare("SELECT id FROM accounts WHERE id = ?")
    .bind(owner)
    .first<{ id: string }>();
  if (!account) return json(503, { error: "the owner account does not exist" });

  const key = crypto.getRandomValues(new Uint8Array(32));
  const wrapped = await wrapContentKey(key, env);
  // Set but malformed: refuse rather than store the key some other way.
  if (!wrapped) return json(503, { error: "content keys are unavailable" });

  const assetId = `ast_${base64url(crypto.getRandomValues(new Uint8Array(16)))}`;
  await env.DB.prepare(
    "INSERT INTO assets (id, creator_id, title, blob_key, key_id, created_at, wrapped_key)" +
      " VALUES (?, ?, ?, NULL, ?, ?, ?)",
  )
    .bind(assetId, owner, title.trim(), KEY_ID, Date.now(), wrapped)
    .run();

  return json(201, { assetId, keyId: KEY_ID, key: base64(key), title: title.trim() });
}

/**
 * `GET /admin/assets` — newest first, with how many hold each one now, whether its file hash is
 * registered, and when a buyer's app last asked for its key. A creator sees only their own.
 */
async function listAssets(env: Env, scope: Scope): Promise<Response> {
  const mine = scope.kind === "creator";
  const statement = env.DB.prepare(
    "SELECT a.id, a.title, a.created_at, a.withdrawn_at, a.taken_down_at, a.blob_sha256 IS NOT NULL AS hashed," +
      " (SELECT COUNT(*) FROM entitlements e WHERE e.asset_id = a.id AND e.revoked_at IS NULL) AS buyers," +
      ` (SELECT MAX(g.issued_at) FROM entitlement_grants g WHERE g.asset_id = a.id AND ${isSteamIdSql("g.steam_id")})` +
      " AS last_request_at" +
      ` FROM assets a${mine ? " WHERE a.creator_id = ?" : ""} ORDER BY a.created_at DESC, a.id DESC`,
  );
  const rows = await (mine ? statement.bind(scope.accountId) : statement).all<{
    id: string;
    title: string;
    created_at: number;
    withdrawn_at: number | null;
    taken_down_at: number | null;
    hashed: number;
    buyers: number;
    last_request_at: number | null;
  }>();
  return json(200, {
    assets: (rows.results ?? []).map((r) => ({
      assetId: r.id,
      title: r.title,
      createdAt: r.created_at,
      withdrawnAt: r.withdrawn_at,
      takenDownAt: r.taken_down_at,
      buyers: r.buyers,
      hashed: !!r.hashed,
      lastRequestAt: r.last_request_at,
    })),
  });
}

/** SQL that holds when `column` is a SteamID64: 17 digits and nothing else. */
function isSteamIdSql(column: string): string {
  return `length(${column}) = 17 AND ${column} NOT GLOB '*[^0-9]*'`;
}

/** Steam display names for the first `MAX_NAMED` accounts, a few lookups at a time. */
async function namesFor(ids: string[], fetchImpl: typeof fetch): Promise<Map<string, string>> {
  const todo = [...new Set(ids)].slice(0, MAX_NAMED);
  const out = new Map<string, string>();
  let next = 0;
  const worker = async () => {
    while (next < todo.length) {
      const id = todo[next++];
      out.set(id, await steamPersonaName(id, fetchImpl));
    }
  };
  await Promise.all(Array.from({ length: VANITY_CONCURRENCY }, worker));
  return out;
}

/** `GET /admin/assets/:id/grants` — who holds it, revoked rows included. `?names=1` adds names. */
async function listGrants(assetId: string, env: Env, names: typeof fetch | null): Promise<Response> {
  if (!(await assetExists(assetId, env))) return json(404, { error: "no such asset" });
  const rows = await env.DB.prepare(
    "SELECT steam_id, source, granted_at, revoked_at FROM entitlements" +
      " WHERE asset_id = ? ORDER BY granted_at DESC, steam_id",
  )
    .bind(assetId)
    .all<{ steam_id: string; source: string; granted_at: number; revoked_at: number | null }>();
  const list = rows.results ?? [];
  const named = names ? await namesFor(list.map((r) => r.steam_id), names) : null;
  return json(200, {
    grants: list.map((r) => ({
      steamId: r.steam_id,
      ...(named ? { name: named.get(r.steam_id) ?? "" } : {}),
      source: r.source,
      grantedAt: r.granted_at,
      revokedAt: r.revoked_at,
    })),
  });
}

/**
 * `GET /admin/assets/:id/usage` — whose app asked for the key, and when.
 *
 * Every request from a linked Steam account is logged, refusals included. A buyer's app asks
 * once per PC; after that the file opens offline, so later plays don't show up here. Rows from
 * before accounts had to be linked (`steam_id` "unlinked") are left out.
 */
async function assetUsage(assetId: string, env: Env, names: typeof fetch | null): Promise<Response> {
  const asset = await env.DB.prepare("SELECT taken_down_at FROM assets WHERE id = ?")
    .bind(assetId)
    .first<{ taken_down_at: number | null }>();
  if (!asset) return json(404, { error: "no such asset" });
  const rows = await env.DB.prepare(
    "SELECT steam_id, decision, reason, issued_at FROM entitlement_grants" +
      ` WHERE asset_id = ? AND ${isSteamIdSql("steam_id")} ORDER BY issued_at DESC LIMIT 500`,
  )
    .bind(assetId)
    .all<{ steam_id: string; decision: string; reason: string | null; issued_at: number }>();
  const events = (rows.results ?? []).map((r) => ({
    steamId: r.steam_id,
    allowed: r.decision === "allow",
    reason: r.reason,
    at: r.issued_at,
  }));
  const byBuyer = new Map<string, { steamId: string; unlocks: number; refused: number; lastAt: number }>();
  for (const e of events) {
    const b = byBuyer.get(e.steamId) ?? { steamId: e.steamId, unlocks: 0, refused: 0, lastAt: 0 };
    if (e.allowed) b.unlocks++;
    else b.refused++;
    b.lastAt = Math.max(b.lastAt, e.at);
    byBuyer.set(e.steamId, b);
  }
  const buyers = [...byBuyer.values()].sort((a, b) => b.lastAt - a.lastAt);
  const named = names ? await namesFor(buyers.map((b) => b.steamId), names) : null;
  return json(200, {
    takenDownAt: asset.taken_down_at,
    buyers: named ? buyers.map((b) => ({ ...b, name: named.get(b.steamId) ?? "" })) : buyers,
    events: events.slice(0, 100),
  });
}

interface Resolved {
  input: string;
  steamId: string;
}

interface Failed {
  input: string;
  error: string;
}

/**
 * `POST /admin/assets/:id/grants` — `{ add?, remove? }`, each a list of Steam accounts.
 *
 * Add inserts a `grant` entitlement, or lifts a revocation (the row, and its provision
 * secret, are kept — so a returning buyer's existing key files still open). An already
 * active entitlement is left exactly as it was. Remove revokes: a timestamp, not a delete,
 * like every other revocation here. Removing an account that holds nothing is a success —
 * the end state asked for is the end state it is in.
 *
 * Adds run before removes, so an account in both lists ends up revoked.
 */
async function changeGrants(
  request: Request,
  assetId: string,
  env: Env,
  fetchImpl: typeof fetch,
): Promise<Response> {
  const body = await readJson(request);
  if (!body || typeof body !== "object") return json(400, { error: "expected a JSON body" });
  const { add = [], remove = [] } = body as { add?: unknown; remove?: unknown };
  if (!Array.isArray(add) || !Array.isArray(remove)) {
    return json(400, { error: "add and remove must be arrays" });
  }
  if (add.length + remove.length === 0) return json(400, { error: "nothing to add or remove" });
  if (add.length + remove.length > MAX_GRANT_CHANGES) {
    return json(400, { error: `at most ${MAX_GRANT_CHANGES} changes at once` });
  }
  if (!(await assetExists(assetId, env))) return json(404, { error: "no such asset" });

  const lookup = await resolveAll([...add, ...remove], fetchImpl);
  const failed: Failed[] = [];
  const split = (list: unknown[]): Resolved[] => {
    const ok: Resolved[] = [];
    for (const entry of list) {
      if (typeof entry === "string" && entry.length > MAX_GRANT_INPUT) {
        failed.push({ input: `${entry.slice(0, MAX_GRANT_INPUT)}…`, error: `longer than ${MAX_GRANT_INPUT} characters` });
        continue;
      }
      const input = String(entry);
      const result = typeof entry === "string" ? lookup.get(entry)! : { error: "not a string" };
      if ("steamId" in result) ok.push({ input, steamId: result.steamId });
      else failed.push({ input, error: result.error });
    }
    return ok;
  };
  const added = split(add);
  const removed = split(remove);

  const now = Date.now();
  const statements = [
    ...added.map((a) =>
      env.DB.prepare(
        "INSERT INTO entitlements (steam_id, asset_id, source, granted_at)" +
          " VALUES (?, ?, 'grant', ?)" +
          " ON CONFLICT (steam_id, asset_id) DO UPDATE SET" +
          "  source = 'grant', granted_at = excluded.granted_at, revoked_at = NULL" +
          " WHERE entitlements.revoked_at IS NOT NULL",
      ).bind(a.steamId, assetId, now),
    ),
    ...removed.map((r) =>
      env.DB.prepare(
        "UPDATE entitlements SET revoked_at = ?" +
          " WHERE steam_id = ? AND asset_id = ? AND revoked_at IS NULL",
      ).bind(now, r.steamId, assetId),
    ),
  ];
  if (statements.length > 0) await env.DB.batch(statements);

  return json(200, { added, removed, failed });
}

/**
 * Every string input to a result. Each distinct custom URL name is asked of Steam once —
 * names are case-insensitive, so `Frost` and `frost` are one lookup — a few at a time.
 */
async function resolveAll(
  inputs: unknown[],
  fetchImpl: typeof fetch,
): Promise<Map<string, { steamId: string } | { error: string }>> {
  const parsed = new Map<string, ReturnType<typeof parseSteamInput>>();
  for (const input of inputs) {
    if (typeof input === "string" && input.length <= MAX_GRANT_INPUT && !parsed.has(input)) {
      parsed.set(input, parseSteamInput(input));
    }
  }
  const names = [
    ...new Set([...parsed.values()].flatMap((p) => ("vanity" in p ? [p.vanity.toLowerCase()] : []))),
  ];
  const looked = new Map<string, { steamId: string } | { error: string }>();
  let next = 0;
  const worker = async () => {
    while (next < names.length) {
      const name = names[next++];
      looked.set(name, await lookUpVanity(name, fetchImpl));
    }
  };
  await Promise.all(Array.from({ length: VANITY_CONCURRENCY }, worker));

  const out = new Map<string, { steamId: string } | { error: string }>();
  for (const [input, p] of parsed) {
    out.set(input, "vanity" in p ? looked.get(p.vanity.toLowerCase())! : p);
  }
  return out;
}

/**
 * What an admin typed, as far as it can be read without asking Steam.
 *
 * A SteamID64, a `steamcommunity.com/profiles/<id>` URL, a `steamcommunity.com/id/<name>`
 * URL, or a bare custom URL name. The last two still need a lookup.
 */
export function parseSteamInput(raw: string): { steamId: string } | { vanity: string } | { error: string } {
  const s = raw.trim();
  if (isSteamId64(s)) return { steamId: s };
  const url = /^(?:https?:\/\/)?(?:www\.)?steamcommunity\.com\/(profiles|id)\/([^/?#\s]+)(?:[/?#].*)?$/i.exec(s);
  if (url) {
    if (url[1].toLowerCase() === "profiles") {
      return isSteamId64(url[2]) ? { steamId: url[2] } : { error: "that profile URL has no SteamID64" };
    }
    return VANITY.test(url[2]) ? { vanity: url[2] } : { error: "that is not a Steam custom URL" };
  }
  if (VANITY.test(s)) return { vanity: s };
  return { error: "not a SteamID64, a Steam profile URL or a custom URL name" };
}

/** One input to a SteamID64, asking Steam for custom URL names. */
export async function resolveSteamAccount(
  raw: string,
  fetchImpl: typeof fetch = fetch,
): Promise<{ steamId: string } | { error: string }> {
  const parsed = parseSteamInput(raw);
  return "vanity" in parsed ? lookUpVanity(parsed.vanity, fetchImpl) : parsed;
}

/** A custom URL name to its SteamID64, from the profile's XML. */
async function lookUpVanity(
  vanity: string,
  fetchImpl: typeof fetch,
): Promise<{ steamId: string } | { error: string }> {
  let text: string;
  try {
    const resp = await fetchImpl(
      `https://steamcommunity.com/id/${encodeURIComponent(vanity)}/?xml=1`,
      { signal: AbortSignal.timeout(VANITY_TIMEOUT_MS) },
    );
    if (!resp.ok) return { error: `Steam answered ${resp.status}` };
    text = await resp.text();
  } catch {
    return { error: "couldn't reach Steam to look that name up" };
  }
  const id = /<steamID64>\s*(\d+)\s*<\/steamID64>/.exec(text)?.[1];
  if (!id || !isSteamId64(id)) return { error: `no Steam profile at /id/${vanity}` };
  return { steamId: id };
}

async function assetExists(assetId: string, env: Env): Promise<boolean> {
  const row = await env.DB.prepare("SELECT id FROM assets WHERE id = ?").bind(assetId).first();
  return row !== null;
}

/**
 * CORS headers for an allowed origin; always `Vary: Origin`, since the answer depends on it.
 * Credentials are allowed so the sign-in cookie rides along from the site.
 */
export function cors(response: Response, origin: string | null, preflight = false, methods = "GET, POST, PATCH, OPTIONS"): Response {
  const out = new Response(response.body, response);
  out.headers.append("Vary", "Origin");
  if (origin) {
    out.headers.set("Access-Control-Allow-Origin", origin);
    out.headers.set("Access-Control-Allow-Credentials", "true");
    if (preflight) {
      out.headers.set("Access-Control-Allow-Methods", methods);
      out.headers.set("Access-Control-Allow-Headers", "Authorization, Content-Type");
      out.headers.set("Access-Control-Max-Age", "600");
    }
  }
  return out;
}

function base64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

function base64url(bytes: Uint8Array): string {
  return base64(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

class BodyTooLarge extends Error {}

/**
 * The JSON body, or null if it isn't JSON. Throws `BodyTooLarge` past `MAX_BODY_BYTES`, read
 * a chunk at a time so an oversized body is never held whole — a declared length isn't trusted.
 */
async function readJson(request: Request): Promise<unknown | null> {
  if (Number(request.headers.get("Content-Length") ?? 0) > MAX_BODY_BYTES) throw new BodyTooLarge();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    const reader = request.body?.getReader();
    while (reader) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > MAX_BODY_BYTES) {
        await reader.cancel();
        throw new BodyTooLarge();
      }
      chunks.push(value);
    }
  } catch (err) {
    if (err instanceof BodyTooLarge) throw err;
    return null;
  }
  const bytes = new Uint8Array(size);
  let at = 0;
  for (const c of chunks) {
    bytes.set(c, at);
    at += c.byteLength;
  }
  try {
    return JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    return null;
  }
}

// index.ts has its own copy; duplicating four lines beats importing the entry point back
// into a module it imports.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
