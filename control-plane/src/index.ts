/**
 * MXB control plane.
 *
 * Holds the accounts, the server registry and — the point of the whole thing — what each
 * rider is wearing. MX Bikes transmits no custom content, so a rider only renders correctly
 * for you if you already hold their paint file under the name they picked. The game cannot
 * tell us which paint that is (its plugin API exposes rider names and bikes and nothing
 * else), so the app reports it here and every other app on the server reads it back.
 */

import {
  awsEnv,
  fleet,
  latestWindowsAmi,
  REGION,
  runInstance,
  terminateInstance,
} from "./aws";
import { bootstrapScript } from "./bootstrap";
import { bearer, hashToken, newToken } from "./auth";
import {
  isGuid,
  isPaintFileName,
  isPaintSize,
  isPublicAgentUrl,
  isPublicGameAddress,
  isRegion,
  isRelDest,
  isRiderName,
  isServerName,
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

  /**
   * The idle sweep, on a cron trigger.
   *
   * Servers bill by the hour whether or not anyone is on them, and nobody is watching at
   * 3am. This is the only thing standing between "we should turn those off" and a month of
   * charges for an empty grid.
   */
  async scheduled(_event: ScheduledController, env: Env, ctx: ExecutionContext): Promise<void> {
    ctx.waitUntil(reapIdleServers(env));
  },
} satisfies ExportedHandler<Env>;

async function route(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  const method = request.method;

  if (method === "GET" && path === "/health") return json(200, { ok: true });

  // Unauthenticated on purpose: a freshly launched instance fetches this during boot, when
  // it holds no credentials and has no way to be given any. The binary is not a secret —
  // it is the same agent anyone can build from the public repository.
  if (method === "GET" && path === "/v1/agent.exe") return artifact(env, "artifacts/mxb-agent.exe");

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
  if (method === "POST" && path === "/v1/servers") return registerServer(request, account, env);
  if (method === "GET" && path === "/v1/fleet") return fleetState(env);
  if (method === "POST" && path === "/v1/provision") return provision(request, account, env);

  const owned = /^\/v1\/servers\/([A-Za-z0-9._-]{1,64})$/.exec(path);
  if (owned && method === "DELETE") return deleteServer(owned[1], account, env);

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

/**
 * Serve a build artifact a booting instance needs.
 *
 * Streamed from R2 through the Worker rather than from a public bucket: the bucket stays
 * private, so the only thing reachable from outside is the exact key named here, and the
 * URL doesn't change if the storage behind it ever does.
 */
async function artifact(env: Env, key: string): Promise<Response> {
  const object = await env.PAINTS.get(key);
  if (!object) return json(404, { error: "no such artifact" });
  return new Response(object.body, {
    headers: {
      "content-type": "application/octet-stream",
      // Short rather than immutable: unlike a paint, this key is *not* content-addressed,
      // so a rebuilt agent has to be able to replace it.
      "cache-control": "public, max-age=300",
    },
  });
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
    "SELECT id, name, region, address FROM servers WHERE published = 1 ORDER BY region, name",
  ).all<{ id: string; name: string; region: string; address: string }>();
  return json(200, { servers: servers.results });
}

/** How many servers one account may register. Enough for a small league, not a botnet. */
const MAX_SERVERS_PER_ACCOUNT = 5;

/** Long enough for a cold agent to answer, short enough that registering never hangs. */
const REACHABILITY_TIMEOUT_MS = 5000;

/**
 * Register a server the player runs, so other people can find it.
 *
 * The list was hand-seeded SQL until now, which meant running a server and *having anyone
 * able to join it* were separate problems, the second one solved by passing an address
 * around privately.
 *
 * Publication is conditional on the agent answering. A home server behind NAT resolves and
 * accepts nothing from outside, and a join picker full of servers that cannot be joined is
 * worse than a short one — so an unreachable server is still recorded, still manageable by
 * its owner, and simply not advertised.
 */
async function registerServer(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { name, region, address, agentUrl } = body as Record<string, unknown>;

  if (!isServerName(name)) return json(400, { error: "that server name won't fit the list" });
  if (!isRegion(region)) return json(400, { error: "unknown region" });
  if (!isPublicGameAddress(address)) {
    return json(400, {
      error:
        "that address isn't one other players could connect to — it needs a public host and a port",
    });
  }
  // Optional: a server can be listed for joining without handing us its admin API.
  if (agentUrl !== undefined && agentUrl !== null && !isPublicAgentUrl(agentUrl)) {
    return json(400, { error: "that agent URL isn't one we can call" });
  }

  const owned = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM servers WHERE owner_account_id = ?",
  )
    .bind(account.id)
    .first<{ n: number }>();
  if ((owned?.n ?? 0) >= MAX_SERVERS_PER_ACCOUNT) {
    return json(409, { error: `you can register up to ${MAX_SERVERS_PER_ACCOUNT} servers` });
  }

  // Addresses are unique across the list: two rows for one server would show up twice in
  // everyone's picker, and would let a second account shadow the first one's entry.
  const clash = await env.DB.prepare(
    "SELECT owner_account_id FROM servers WHERE lower(address) = lower(?)",
  )
    .bind((address as string).trim())
    .first<{ owner_account_id: string | null }>();
  if (clash) {
    return json(409, { error: "a server at that address is already registered" });
  }

  const reachable = agentUrl ? await agentAnswers(agentUrl as string) : false;
  const id = crypto.randomUUID();
  const now = Date.now();
  await env.DB.prepare(
    "INSERT INTO servers (id, name, region, address, agent_url, created_at, owner_account_id," +
      " published, checked_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
  )
    .bind(
      id,
      (name as string).trim(),
      region,
      (address as string).trim(),
      agentUrl ? (agentUrl as string).trim() : null,
      now,
      account.id,
      reachable ? 1 : 0,
      agentUrl ? now : null,
    )
    .run();

  return json(201, { id, published: reachable });
}

/**
 * Does the agent at this URL answer?
 *
 * `/health` is unauthenticated by design on the agent, which makes it exactly the right
 * probe: it proves the host is up and reachable from outside without us holding a token.
 *
 * What this cannot prove is that the *game* port is open — that is UDP, and a Worker has no
 * way to send one. So "published" means the box answers and the operator says a server is
 * on it, not that anyone has demonstrably joined.
 */
async function agentAnswers(agentUrl: string): Promise<boolean> {
  const base = agentUrl.trim().replace(/\/+$/, "");
  try {
    const resp = await fetch(`${base}/health`, {
      method: "GET",
      signal: AbortSignal.timeout(REACHABILITY_TIMEOUT_MS),
    });
    return resp.ok;
  } catch {
    // Unreachable, refused, too slow, or DNS that goes nowhere — all the same answer here.
    return false;
  }
}

/**
 * What AWS is currently charging us for.
 *
 * Deliberately reads from EC2 rather than from our own table: the database records what we
 * believe we created, and this is what will actually appear on the bill. When a launch
 * half-fails those two disagree, and only one of them is expensive to be wrong about.
 *
 * Authenticated but not owner-scoped — every enrolled account sees the same fleet, because
 * the number of servers running is what the spending cap is measured against and hiding it
 * from the people it constrains would be perverse.
 */
async function fleetState(env: Env): Promise<Response> {
  const aws = awsEnv(env);
  if (!aws) return json(503, { error: "provisioning isn't configured on this deployment" });
  try {
    const instances = await fleet(aws);
    return json(200, { region: REGION, instances });
  } catch (err) {
    console.error(JSON.stringify({ msg: "fleet", error: String(err) }));
    return json(502, { error: String(err) });
  }
}

/**
 * Create a server: launch the machine, and record it.
 *
 * The order matters. The row is written *before* the launch, so an instance can never exist
 * without something in our database pointing at it — the reverse leaves a billing resource
 * nobody knows to turn off, which is the failure that costs money rather than time. If the
 * launch then fails, the row is removed again.
 */
async function provision(request: Request, account: Account, env: Env): Promise<Response> {
  const aws = awsEnv(env);
  if (!aws) return json(503, { error: "provisioning isn't configured on this deployment" });
  const securityGroupId = env.MXB_SECURITY_GROUP_ID?.trim();
  const agentDownload = env.MXB_AGENT_DOWNLOAD_URL?.trim();
  const gameDownload = env.MXB_GAME_DOWNLOAD_URL?.trim();
  if (!securityGroupId || !agentDownload || !gameDownload) {
    return json(503, { error: "provisioning isn't finished being set up yet" });
  }

  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { name } = body as Record<string, unknown>;
  if (!isServerName(name)) return json(400, { error: "that server name won't fit the list" });

  // Counted from EC2, not from our table. This is the number that turns into a bill, and
  // the two disagree exactly when something has already gone wrong.
  const running = await fleet(aws);
  const cap = Number(env.MXB_MAX_INSTANCES ?? "2");
  if (running.length >= cap) {
    return json(409, {
      error: `there are already ${running.length} servers running, and the limit is ${cap}`,
    });
  }

  const id = crypto.randomUUID();
  const agentToken = newToken();
  const now = Date.now();

  await env.DB.prepare(
    "INSERT INTO servers (id, name, region, address, created_at, owner_account_id, published," +
      " agent_token) VALUES (?, ?, ?, '', ?, ?, 0, ?)",
  )
    .bind(id, (name as string).trim(), REGION, now, account.id, agentToken)
    .run();

  try {
    const amiId = await latestWindowsAmi(aws);
    const instanceId = await runInstance(
      aws,
      {
        name: `mxb ${(name as string).trim()}`,
        instanceType: env.MXB_INSTANCE_TYPE?.trim() || "t3.small",
        securityGroupId,
        userData: bootstrapScript({
          agentToken,
          agentUrl: agentDownload,
          gameUrl: gameDownload,
          serverName: (name as string).trim(),
          gamePort: 54210,
          agentPort: 8787,
        }),
      },
      amiId,
    );
    await env.DB.prepare("UPDATE servers SET instance_id = ? WHERE id = ?")
      .bind(instanceId, id)
      .run();
    // No address yet: EC2 assigns the public IP as the instance comes up, and the app polls
    // for it. Publishing waits until there is something to publish.
    return json(201, { id, instanceId, state: "pending" });
  } catch (err) {
    // The row would otherwise claim a server that does not exist, and the cap counts rows
    // nobody can ever use.
    await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(id).run();
    console.error(JSON.stringify({ msg: "provision", error: String(err) }));
    return json(502, { error: String(err) });
  }
}

/**
 * Remove a server from the list — and destroy the machine, if we made one.
 *
 * Only its owner may, and the hand-seeded rows have no owner so the API cannot touch them.
 * Termination happens before the row is dropped: losing the row while the instance lives is
 * how an orphan starts billing forever.
 */
async function deleteServer(id: string, account: Account, env: Env): Promise<Response> {
  const row = await env.DB.prepare(
    "SELECT owner_account_id, instance_id FROM servers WHERE id = ?",
  )
    .bind(id)
    .first<{ owner_account_id: string | null; instance_id: string | null }>();
  if (!row) return json(404, { error: "no such server" });
  // One message whether it is someone else's or unowned: which of the two it is isn't the
  // caller's business.
  if (row.owner_account_id !== account.id) {
    return json(403, { error: "that isn't your server" });
  }

  if (row.instance_id) {
    const aws = awsEnv(env);
    if (!aws) return json(503, { error: "can't reach AWS to shut that server down" });
    try {
      await terminateInstance(aws, row.instance_id);
    } catch (err) {
      // Deliberately fatal. Dropping the row here would leave an instance running with
      // nothing left pointing at it, and the reaper works from these rows.
      console.error(JSON.stringify({ msg: "terminate", error: String(err) }));
      return json(502, { error: `couldn't shut the server down: ${String(err)}` });
    }
  }

  await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(id).run();
  return json(200, { ok: true, terminated: Boolean(row.instance_id) });
}

/**
 * Destroy servers that nobody is riding on.
 *
 * This is what makes "no unattended servers" true rather than merely intended. An idle
 * server is indistinguishable from a busy one on the bill, and the only person who would
 * notice is the one paying — long after the fact.
 *
 * A server is given a grace period rather than being killed on the first empty poll,
 * because empty is normal: between races, and while the first rider is still loading in.
 */
async function reapIdleServers(env: Env): Promise<void> {
  const aws = awsEnv(env);
  if (!aws) return;
  const idleMs = Number(env.MXB_IDLE_MINUTES ?? "20") * 60_000;
  const maxLifeMs = Number(env.MXB_MAX_LIFETIME_MINUTES ?? "240") * 60_000;
  const now = Date.now();

  // Driven from EC2, not from our table. The instances AWS is billing for are the ones that
  // need turning off, and a row that went missing is precisely the case where our records
  // cannot be trusted to find them.
  const instances = await fleet(aws);
  const rows = await env.DB.prepare(
    "SELECT id, instance_id, agent_token, idle_since FROM servers WHERE instance_id IS NOT NULL",
  ).all<{
    id: string;
    instance_id: string;
    agent_token: string | null;
    idle_since: number | null;
  }>();
  const byInstance = new Map(rows.results.map((r) => [r.instance_id, r]));

  for (const instance of instances) {
    if (instance.state !== "running" && instance.state !== "pending") continue;
    const row = byInstance.get(instance.instanceId);

    const kill = async (why: string) => {
      try {
        await terminateInstance(aws, instance.instanceId);
        if (row) await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(row.id).run();
        console.log(JSON.stringify({ msg: "reaped", instance: instance.instanceId, why }));
      } catch (err) {
        // Left on the list rather than forgotten — an instance we failed to kill is exactly
        // the one that must be tried again next sweep.
        console.error(
          JSON.stringify({ msg: "reap", instance: instance.instanceId, why, error: String(err) }),
        );
      }
    };

    // An instance with no row is one whose record we already deleted, or one whose launch
    // half-failed. Either way nothing is tracking it any more, so nothing will ever turn it
    // off — which makes destroying it the safe direction, not the risky one.
    if (!row) {
      await kill("orphan: no database row points at this instance");
      continue;
    }

    // The backstop that catches everything else: a bootstrap that hung instead of trapping,
    // an agent that never started, a failure mode nobody has thought of yet. Without this,
    // any instance we cannot talk to bills until a human notices.
    const age = instance.launchedAt ? now - Date.parse(instance.launchedAt) : 0;
    if (age > maxLifeMs) {
      await kill(`older than the ${maxLifeMs / 60_000} minute limit`);
      continue;
    }

    // Built from EC2's view rather than a stored column: the public IP is assigned while the
    // instance boots, long after the row was written, so the row can never have it at
    // creation time and a server whose address we forgot to record would never be checked.
    const agentUrl = instance.publicIp ? `http://${instance.publicIp}:8787` : null;
    const players = await connectedCount(agentUrl, row.agent_token);

    if (players === null) {
      // Still booting, or briefly unreachable. Not fatal on its own — the age limit above
      // is what stops "unreachable" from meaning "runs forever".
      continue;
    }
    if (players > 0) {
      if (row.idle_since !== null) {
        await env.DB.prepare("UPDATE servers SET idle_since = NULL WHERE id = ?")
          .bind(row.id)
          .run();
      }
      continue;
    }
    if (row.idle_since === null) {
      await env.DB.prepare("UPDATE servers SET idle_since = ? WHERE id = ?")
        .bind(now, row.id)
        .run();
      continue;
    }
    if (now - row.idle_since >= idleMs) {
      await kill(`empty for ${Math.round((now - row.idle_since) / 60_000)} minutes`);
    }
  }
}

/** How many riders are on a server, or null if the agent couldn't be asked. */
async function connectedCount(
  agentUrl: string | null,
  agentToken: string | null,
): Promise<number | null> {
  if (!agentUrl || !agentToken) return null;
  try {
    const resp = await fetch(`${agentUrl.replace(/\/+$/, "")}/players`, {
      headers: { authorization: `Bearer ${agentToken}` },
      signal: AbortSignal.timeout(5000),
    });
    if (!resp.ok) return null;
    const body = (await resp.json()) as { players?: unknown[] };
    return Array.isArray(body.players) ? body.players.length : null;
  } catch {
    return null;
  }
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
