/**
 * MXB control plane.
 *
 * Holds the accounts, the server registry and — the point of the whole thing — what each
 * rider is wearing. MX Bikes transmits no custom content, so a rider only renders correctly
 * for you if you already hold their paint file under the name they picked. The game cannot
 * tell us which paint that is (its plugin API exposes rider names and bikes and nothing
 * else), so the app reports it here and every other app on the server reads it back.
 */

import { bearer, hashToken, newToken } from "./auth";
import {
  isGuid,
  isPaintFileName,
  isPaintSize,
  isRelDest,
  isRiderName,
  isSha256,
  isSlot,
  MAX_PAINT_BYTES,
} from "./validate";

interface Account {
  id: string;
  rider_name: string;
  steam_id: string | null;
  guid: string | null;
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    // `ctx` is intentionally unused for now: every endpoint completes its work before
    // responding, so there is nothing to hand to `ctx.waitUntil`.
    void ctx;
    try {
      return await route(request, env);
    } catch (err) {
      // Explicit handling rather than passThroughOnException, which hides the bug and
      // leaves the caller with an opaque failure.
      console.error(JSON.stringify({ msg: "unhandled", error: String(err) }));
      return json(500, { error: "internal error" });
    }
  },
} satisfies ExportedHandler<Env>;

async function route(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  const method = request.method;

  if (method === "GET" && path === "/health") return json(200, { ok: true });

  // Enrollment is the one unauthenticated write: it trades an invite code for a token.
  // Steam sign-in will replace the invite code without changing anything downstream.
  if (method === "POST" && path === "/v1/enroll") return enroll(request, env);

  // The server list is public, and has to be: it is what the app's join picker offers, and
  // requiring a token meant a player who hadn't enrolled was shown an empty box asking for
  // an IP address — the exact question the registry exists to answer. Nothing here is
  // secret; it is the same name/region/address a server browser shows, and `agent_url` is
  // deliberately not selected, so the admin API's location stays private.
  if (method === "GET" && path === "/v1/servers") return listServers(env);

  const account = await authenticate(request, env);
  if (!account) return json(401, { error: "unauthorized" });

  if (method === "GET" && path === "/v1/me") return me(account, env);
  if (method === "PUT" && path === "/v1/me/guid") return putGuid(request, account, env);
  if (method === "PUT" && path === "/v1/loadout") return putLoadout(request, account, env);
  if (method === "GET" && path === "/v1/roster") return roster(url, env);

  const paint = /^\/v1\/paints\/([0-9a-f]{64})$/.exec(path);
  if (paint) {
    if (method === "PUT") return putPaint(request, paint[1], env);
    if (method === "GET") return getPaint(paint[1], env);
  }

  return json(404, { error: "no such endpoint" });
}

async function authenticate(request: Request, env: Env): Promise<Account | null> {
  const token = bearer(request.headers.get("Authorization"));
  if (!token) return null;
  // Look up by digest: the comparison happens in the index, so there is no string compare
  // of a secret in our code to leak timing.
  const hash = await hashToken(token);
  return await env.DB.prepare(
    "SELECT id, rider_name, steam_id, guid FROM accounts WHERE token_hash = ?",
  )
    .bind(hash)
    .first<Account>();
}

async function enroll(request: Request, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });

  const { code, riderName } = body as { code?: unknown; riderName?: unknown };
  if (typeof code !== "string" || code.trim().length === 0) {
    return json(400, { error: "an invite code is required" });
  }
  if (!isRiderName(riderName)) {
    return json(400, { error: "riderName must match your in-game rider name" });
  }

  const invite = await env.DB.prepare("SELECT code, claimed_by FROM invites WHERE code = ?")
    .bind(code.trim())
    .first<{ code: string; claimed_by: string | null }>();
  // One message for both cases: telling an unknown code apart from a used one turns this
  // into an oracle for enumerating valid codes.
  if (!invite || invite.claimed_by) return json(403, { error: "that invite code isn't usable" });

  const id = crypto.randomUUID();
  const token = newToken();
  const hash = await hashToken(token);
  const now = Date.now();

  try {
    // Batched so a claimed invite can never exist without the account it created.
    await env.DB.batch([
      env.DB.prepare(
        "INSERT INTO accounts (id, rider_name, token_hash, created_at) VALUES (?, ?, ?, ?)",
      ).bind(id, (riderName as string).trim(), hash, now),
      env.DB.prepare(
        "UPDATE invites SET claimed_by = ?, claimed_at = ? WHERE code = ? AND claimed_by IS NULL",
      ).bind(id, now, invite.code),
    ]);
  } catch (err) {
    // The unique index on lower(rider_name) is what rejects a duplicate, so this is the
    // expected path for a name someone already has — not an internal error.
    if (String(err).includes("UNIQUE")) {
      return json(409, { error: "that rider name is already enrolled" });
    }
    throw err;
  }

  // The only time the token is ever visible. It is stored as a digest, so it cannot be
  // shown again and a database leak yields nothing presentable.
  return json(201, { accountId: id, token, riderName: (riderName as string).trim() });
}

async function me(account: Account, env: Env): Promise<Response> {
  const paints = await env.DB.prepare(
    "SELECT slot, file_name, sha256, size FROM loadout_paints WHERE account_id = ?",
  )
    .bind(account.id)
    .all<{ slot: string; file_name: string; sha256: string; size: number }>();

  return json(200, {
    accountId: account.id,
    riderName: account.rider_name,
    steamId: account.steam_id,
    guid: account.guid,
    paints: paints.results.map((p) => ({
      slot: p.slot,
      fileName: p.file_name,
      sha256: p.sha256,
      size: p.size,
    })),
  });
}

/**
 * Claim a GUID for this account.
 *
 * The GUID is what makes a rider identifiable across name changes, and it's what the server
 * log reports on every connection. Claiming is first-come: the unique index rejects a second
 * account trying to take one already held, which is the whole point — otherwise anyone could
 * assert someone else's identity and have their paints served under it.
 */
async function putGuid(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { guid } = body as { guid?: unknown };
  if (!isGuid(guid)) return json(400, { error: "that doesn't look like an MX Bikes GUID" });

  try {
    await env.DB.prepare("UPDATE accounts SET guid = ? WHERE id = ?")
      .bind((guid as string).trim(), account.id)
      .run();
  } catch (err) {
    if (String(err).includes("UNIQUE")) {
      return json(409, { error: "that GUID is already claimed by another account" });
    }
    throw err;
  }
  return json(200, { ok: true, guid: (guid as string).trim() });
}

async function putLoadout(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });

  const { bikeId, paints } = body as { bikeId?: unknown; paints?: unknown };
  if (!Array.isArray(paints)) return json(400, { error: "paints must be an array" });
  if (paints.length > 16) return json(400, { error: "too many paints" });

  const rows: { slot: string; fileName: string; sha256: string; size: number; relDest: string }[] =
    [];
  for (const entry of paints) {
    const p = entry as Record<string, unknown>;
    if (!isSlot(p.slot)) return json(400, { error: `unknown slot: ${String(p.slot)}` });
    if (!isPaintFileName(p.fileName)) {
      return json(400, { error: `invalid paint filename for ${p.slot}` });
    }
    if (!isRelDest(p.relDest)) return json(400, { error: `invalid destination for ${p.slot}` });
    if (!isSha256(p.sha256)) return json(400, { error: `invalid sha256 for ${p.slot}` });
    if (!isPaintSize(p.size)) return json(400, { error: `invalid size for ${p.slot}` });
    rows.push({
      slot: p.slot,
      fileName: (p.fileName as string).trim(),
      sha256: p.sha256,
      size: p.size,
      relDest: (p.relDest as string).trim(),
    });
  }

  const now = Date.now();
  const statements = [
    env.DB.prepare(
      "INSERT INTO loadouts (account_id, bike_id, updated_at) VALUES (?, ?, ?)" +
        " ON CONFLICT(account_id) DO UPDATE SET bike_id = excluded.bike_id, updated_at = excluded.updated_at",
    ).bind(account.id, typeof bikeId === "string" ? bikeId : null, now),
    // Replace wholesale: a slot the player cleared has to disappear, and merging would
    // leave them wearing something they took off.
    env.DB.prepare("DELETE FROM loadout_paints WHERE account_id = ?").bind(account.id),
    ...rows.map((r) =>
      env.DB.prepare(
        "INSERT INTO loadout_paints (account_id, slot, file_name, sha256, size, rel_dest)" +
          " VALUES (?, ?, ?, ?, ?, ?)",
      ).bind(account.id, r.slot, r.fileName, r.sha256, r.size, r.relDest),
    ),
  ];
  await env.DB.batch(statements);

  // Tell the client which blobs we still need, so it uploads only what nobody has shared
  // yet. Content addressing makes this cheap: the same paint from twenty riders is one
  // object and nineteen skipped uploads.
  const missing: string[] = [];
  for (const r of rows) {
    if (!(await env.PAINTS.head(r.sha256))) missing.push(r.sha256);
  }

  return json(200, { ok: true, missing });
}

/**
 * Store a paint blob under its own digest.
 *
 * The digest is recomputed from the body rather than trusted: the key is what every other
 * client will fetch by, so letting an uploader name a key it does not match would let one
 * player replace the bytes every other player receives under a hash they already verified.
 */
async function putPaint(request: Request, sha256: string, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_PAINT_BYTES) return json(413, { error: "that paint is too large" });

  // Buffered deliberately: the digest has to be checked before the object is stored, and a
  // paint is bounded to a few megabytes by the check above.
  const body = await request.arrayBuffer();
  if (body.byteLength === 0) return json(400, { error: "empty body" });
  if (body.byteLength > MAX_PAINT_BYTES) return json(413, { error: "that paint is too large" });

  const digest = await crypto.subtle.digest("SHA-256", body);
  const actual = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
  if (actual !== sha256) {
    return json(400, { error: "the body does not match the digest in the URL" });
  }

  // Content-addressed, so an upload of something already stored is a no-op rather than a
  // conflict — twenty riders sharing a paint means one object.
  if (!(await env.PAINTS.head(sha256))) {
    await env.PAINTS.put(sha256, body);
  }
  return json(201, { ok: true, sha256, size: body.byteLength });
}

async function getPaint(sha256: string, env: Env): Promise<Response> {
  const object = await env.PAINTS.get(sha256);
  if (!object) return json(404, { error: "no such paint" });
  // Streamed rather than buffered: no reason to hold it in the isolate on the way out.
  return new Response(object.body, {
    headers: {
      "content-type": "application/octet-stream",
      // Immutable by construction — the name is the hash of the content.
      "cache-control": "public, max-age=31536000, immutable",
      etag: sha256,
    },
  });
}

/**
 * The joinable servers. Unauthenticated — see the routing table.
 *
 * `agent_url` is not in the select list and must not be added to it: that column is the
 * base URL of a server's admin API, and while it is still bearer-protected on the agent
 * side, publishing where every one of them lives hands an attacker the target list for
 * free. Everything else here is what a player needs in order to connect.
 */
async function listServers(env: Env): Promise<Response> {
  const servers = await env.DB.prepare(
    "SELECT id, name, region, address FROM servers ORDER BY region, name",
  ).all<{ id: string; name: string; region: string; address: string }>();
  return json(200, { servers: servers.results });
}

/**
 * Every rider currently registered against a server, with what they are wearing.
 *
 * This is what the app polls to know which paints to fetch. Until the agent reports live
 * rosters it returns the whole enrolled set, which on an invite-only server is close enough
 * to be useful and is a strict superset of who is actually on.
 */
async function roster(url: URL, env: Env): Promise<Response> {
  const serverId = url.searchParams.get("server");
  if (!serverId) return json(400, { error: "a server id is required" });

  const rows = await env.DB.prepare(
    "SELECT a.rider_name, a.guid, p.slot, p.file_name, p.sha256, p.size, p.rel_dest" +
      " FROM accounts a JOIN loadout_paints p ON p.account_id = a.id",
  ).all<{
    rider_name: string;
    guid: string | null;
    slot: string;
    file_name: string;
    sha256: string;
    size: number;
    rel_dest: string;
  }>();

  const riders = new Map<string, { riderName: string; guid: string | null; paints: unknown[] }>();
  for (const r of rows.results) {
    // Re-checked on the way out as well as in. A row predating the validation, or one
    // written by some future path that forgot it, must not reach a client that is about to
    // turn it into a filesystem path.
    if (!isRelDest(r.rel_dest)) continue;
    // Group by GUID where the account has claimed one — it is stable across name changes
    // and cannot be taken by someone else. Name is the fallback for accounts that have not
    // supplied a GUID yet, which is every account until the player has connected once.
    const key = r.guid ?? `name:${r.rider_name.toLowerCase()}`;
    let rider = riders.get(key);
    if (!rider) {
      rider = { riderName: r.rider_name, guid: r.guid, paints: [] };
      riders.set(key, rider);
    }
    rider.paints.push({
      slot: r.slot,
      fileName: r.file_name,
      sha256: r.sha256,
      size: r.size,
      relDest: r.rel_dest,
    });
  }
  return json(200, { server: serverId, riders: [...riders.values()] });
}

async function readJson(request: Request): Promise<unknown | null> {
  try {
    return await request.json();
  } catch {
    return null;
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
