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
import { isPaintFileName, isPaintSize, isRiderName, isSha256, isSlot } from "./validate";

interface Account {
  id: string;
  rider_name: string;
  steam_id: string | null;
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    try {
      return await route(request, env, ctx);
    } catch (err) {
      // Explicit handling rather than passThroughOnException, which hides the bug and
      // leaves the caller with an opaque failure.
      console.error(JSON.stringify({ msg: "unhandled", error: String(err) }));
      return json(500, { error: "internal error" });
    }
  },
} satisfies ExportedHandler<Env>;

async function route(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  const method = request.method;

  if (method === "GET" && path === "/health") return json(200, { ok: true });

  // Enrolment is the one unauthenticated write: it trades an invite code for a token.
  // Steam sign-in will replace the invite code without changing anything downstream.
  if (method === "POST" && path === "/v1/enroll") return enroll(request, env);

  const account = await authenticate(request, env);
  if (!account) return json(401, { error: "unauthorized" });

  if (method === "GET" && path === "/v1/me") return me(account, env);
  if (method === "PUT" && path === "/v1/loadout") return putLoadout(request, account, env);
  if (method === "GET" && path === "/v1/servers") return listServers(env);
  if (method === "GET" && path === "/v1/roster") return roster(url, env);

  return json(404, { error: "no such endpoint" });
}

async function authenticate(request: Request, env: Env): Promise<Account | null> {
  const token = bearer(request.headers.get("Authorization"));
  if (!token) return null;
  // Look up by digest: the comparison happens in the index, so there is no string compare
  // of a secret in our code to leak timing.
  const hash = await hashToken(token);
  return await env.DB.prepare(
    "SELECT id, rider_name, steam_id FROM accounts WHERE token_hash = ?",
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
    paints: paints.results.map((p) => ({
      slot: p.slot,
      fileName: p.file_name,
      sha256: p.sha256,
      size: p.size,
    })),
  });
}

async function putLoadout(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });

  const { bikeId, paints } = body as { bikeId?: unknown; paints?: unknown };
  if (!Array.isArray(paints)) return json(400, { error: "paints must be an array" });
  if (paints.length > 16) return json(400, { error: "too many paints" });

  const rows: { slot: string; fileName: string; sha256: string; size: number }[] = [];
  for (const entry of paints) {
    const p = entry as Record<string, unknown>;
    if (!isSlot(p.slot)) return json(400, { error: `unknown slot: ${String(p.slot)}` });
    if (!isPaintFileName(p.fileName)) {
      return json(400, { error: `invalid paint filename for ${p.slot}` });
    }
    if (!isSha256(p.sha256)) return json(400, { error: `invalid sha256 for ${p.slot}` });
    if (!isPaintSize(p.size)) return json(400, { error: `invalid size for ${p.slot}` });
    rows.push({
      slot: p.slot,
      fileName: (p.fileName as string).trim(),
      sha256: p.sha256,
      size: p.size,
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
        "INSERT INTO loadout_paints (account_id, slot, file_name, sha256, size) VALUES (?, ?, ?, ?, ?)",
      ).bind(account.id, r.slot, r.fileName, r.sha256, r.size),
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
    "SELECT a.rider_name, p.slot, p.file_name, p.sha256, p.size" +
      " FROM accounts a JOIN loadout_paints p ON p.account_id = a.id",
  ).all<{ rider_name: string; slot: string; file_name: string; sha256: string; size: number }>();

  const riders = new Map<string, { riderName: string; paints: unknown[] }>();
  for (const r of rows.results) {
    let rider = riders.get(r.rider_name);
    if (!rider) {
      rider = { riderName: r.rider_name, paints: [] };
      riders.set(r.rider_name, rider);
    }
    rider.paints.push({ slot: r.slot, fileName: r.file_name, sha256: r.sha256, size: r.size });
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
