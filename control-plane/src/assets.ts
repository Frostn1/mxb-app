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
import { isSteamId64 } from "./steam";
import { adminAllowed } from "./usage";

/** Origins allowed to call these routes from a browser. */
export const ASSET_ORIGINS = [
  "https://mxbsecure.com",
  "https://www.mxbsecure.com",
  "http://localhost:5173",
];

/** Most inputs one grants change may carry, adds and removes together. */
export const MAX_GRANT_CHANGES = 100;

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
  const allowed = origin && ASSET_ORIGINS.includes(origin) ? origin : null;

  if (request.method === "OPTIONS") {
    if (origin && !allowed) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), allowed, true);
  }

  let response: Response;
  try {
    response = await handle(request, url, env, fetchImpl);
  } catch (err) {
    // Caught here rather than by the router so the site still gets a readable 500.
    console.error(JSON.stringify({ msg: "admin assets failed", error: String(err) }));
    response = json(500, { error: "internal error" });
  }
  return cors(response, allowed);
}

async function handle(
  request: Request,
  url: URL,
  env: Env,
  fetchImpl: typeof fetch,
): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return json(503, { error: "no admin key is configured" });
  if (allowed === "denied") return json(401, { error: "unauthorized" });
  if (!currentMasterVersion(env) || !env.MXB_OWNER_ACCOUNT_ID) {
    return json(503, { error: "secured assets are not configured" });
  }

  const match = /^\/admin\/assets(?:\/([A-Za-z0-9_-]{1,64})\/grants)?\/?$/.exec(url.pathname);
  if (!match) return json(404, { error: "no such endpoint" });
  const assetId = match[1];
  const method = request.method;

  if (!assetId) {
    if (method === "GET") return listAssets(env);
    if (method === "POST") return createAsset(request, env);
  } else {
    if (method === "GET") return listGrants(assetId, env);
    if (method === "POST") return changeGrants(request, assetId, env, fetchImpl);
  }
  return json(405, { error: "method not allowed" });
}

/**
 * `POST /admin/assets` — a new asset and its content key.
 *
 * The key is 32 random bytes, stored only wrapped under the master key. The raw bytes are in
 * this response and nowhere else, ever: the packer needs them once, to seal the blob.
 */
async function createAsset(request: Request, env: Env): Promise<Response> {
  const body = await readJson(request);
  const title = (body as { title?: unknown } | null)?.title;
  if (typeof title !== "string" || !title.trim() || title.trim().length > 200) {
    return json(400, { error: "title must be 1 to 200 characters" });
  }

  const owner = env.MXB_OWNER_ACCOUNT_ID;
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

/** `GET /admin/assets` — every asset, newest first, with how many hold it now. */
async function listAssets(env: Env): Promise<Response> {
  const rows = await env.DB.prepare(
    "SELECT a.id, a.title, a.created_at, a.withdrawn_at," +
      " (SELECT COUNT(*) FROM entitlements e WHERE e.asset_id = a.id AND e.revoked_at IS NULL)" +
      " AS buyers" +
      " FROM assets a ORDER BY a.created_at DESC, a.id DESC",
  ).all<{
    id: string;
    title: string;
    created_at: number;
    withdrawn_at: number | null;
    buyers: number;
  }>();
  return json(200, {
    assets: (rows.results ?? []).map((r) => ({
      assetId: r.id,
      title: r.title,
      createdAt: r.created_at,
      withdrawnAt: r.withdrawn_at,
      buyers: r.buyers,
    })),
  });
}

/** `GET /admin/assets/:id/grants` — who holds it, revoked rows included. */
async function listGrants(assetId: string, env: Env): Promise<Response> {
  if (!(await assetExists(assetId, env))) return json(404, { error: "no such asset" });
  const rows = await env.DB.prepare(
    "SELECT steam_id, source, granted_at, revoked_at FROM entitlements" +
      " WHERE asset_id = ? ORDER BY granted_at DESC, steam_id",
  )
    .bind(assetId)
    .all<{ steam_id: string; source: string; granted_at: number; revoked_at: number | null }>();
  return json(200, {
    grants: (rows.results ?? []).map((r) => ({
      steamId: r.steam_id,
      source: r.source,
      grantedAt: r.granted_at,
      revokedAt: r.revoked_at,
    })),
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
    if (typeof input === "string" && !parsed.has(input)) parsed.set(input, parseSteamInput(input));
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

/** CORS headers for an allowed origin; always `Vary: Origin`, since the answer depends on it. */
function cors(response: Response, origin: string | null, preflight = false): Response {
  const out = new Response(response.body, response);
  out.headers.append("Vary", "Origin");
  if (origin) {
    out.headers.set("Access-Control-Allow-Origin", origin);
    if (preflight) {
      out.headers.set("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
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

async function readJson(request: Request): Promise<unknown | null> {
  try {
    return await request.json();
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
