/**
 * MXB control plane.
 *
 * Holds the accounts, the server registry and — the point of the whole thing — what each
 * rider is wearing. MX Bikes transmits no custom content, so a rider only renders correctly
 * for you if you already hold their paint file under the name they picked. The game cannot
 * tell us which paint that is (its plugin API exposes rider names and bikes and nothing
 * else), so the app reports it here and every other app on the server reads it back.
 */

import { unwrapContentKey } from "./assetkey";
import {
  isVerified,
  loginUrl,
  verifyAssertion,
  LOGIN_TTL_MS,
} from "./steam";
import {
  awsEnv,
  createImage,
  fleet,
  imageState,
  latestWindowsAmi,
  REGION,
  runInstance,
  terminateInstance,
} from "./aws";
import { bmacWebhook } from "./bmac";
import { pruneReports, putReport } from "./diagnostics";
import {
  diagnosticsDashboard,
  diagnosticsFile,
  diagnosticsFiles,
  diagnosticsRider,
  diagnosticsRiders,
  diagnosticsRules,
  diagnosticsRulesPage,
} from "./diagnosticspage";
import {
  paintFiles,
  paintOne,
  paintRider,
  paintRiders,
  paintThumbnail,
} from "./paintspage";
import { listPlugins, myPlugins, pluginBundle, redeemKey } from "./plugins";
import { generateTrack } from "./trackgen";
import { bootstrapScript, imageBootstrapScript } from "./bootstrap";
import { bearer, hashToken, newToken, tokenMatches } from "./auth";
import {
  isBikeId,
  isBootstrapStage,
  isContentName,
  isGuid,
  isPaintFileName,
  isPaintSize,
  isPublicAgentUrl,
  isPublicGameAddress,
  isRegion,
  isRelDest,
  isServerKey,
  isRiderName,
  isServerName,
  isSha256,
  isSlot,
  MAX_BOOTSTRAP_LOG,
  MAX_PAINT_BYTES,
  PRESENCE_TTL_MS,
} from "./validate";
import { claimDeviceAccount, iceServers, voiceRoom } from "./voice";
import { pruneUsage, reportUsage, usageStats } from "./usage";
import { usageDashboard } from "./usagepage";
import { VoiceRoom } from "./voiceroom";

interface Account {
  id: string;
  rider_name: string;
  steam_id: string | null;
  guid: string | null;
  /** `invited` (an invite code was claimed) or `device` (self-serve, voice only). */
  kind: string;
}

// The runtime finds a Durable Object class by its export from the entry module.
export { VoiceRoom };

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
    ctx.waitUntil(
      Promise.all([
        reapIdleServers(env),
        advanceImageBuild(env),
        pruneDeviceClaims(env),
        pruneUsage(env),
        pruneReports(env),
      ]).then(
        () => undefined,
      ),
    );
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

  // The bikes a provisioned server needs in order to accept the ones players ride.
  //
  // Unauthenticated for the same reason as the agent binary: a booting instance holds no
  // credential and has no way to be given one. Nothing here is secret — it is the same
  // content every client already has installed — and the listing is what lets the bootstrap
  // stay ignorant of which bikes exist.
  if (method === "GET" && path === "/v1/content/bikes") return listContent(env);
  const content = /^\/v1\/content\/bikes\/(.+)$/.exec(path);
  if (content && method === "GET") {
    const name = decodeURIComponent(content[1]);
    // Validated rather than trusted: this segment becomes an R2 key and then a filename on
    // someone's disk.
    if (!isContentName(name)) return json(400, { error: "not a content file" });
    return artifact(env, `content/bikes/${name}`);
  }

  // Enrollment is the one unauthenticated write: it trades an invite code for a token.
  // Steam sign-in will replace the invite code without changing anything downstream.
  // Above the account gate on purpose. The endpoint spends our Anthropic budget, so it is
  // capped hard by its own shape — one call, 16k output tokens, a schema that can only be a
  // motocross track — rather than by who is asking. That also makes it usable against a local
  // `wrangler dev`, which has no accounts to enroll with.
  if (method === "POST" && path === "/v1/track/generate") return generateTrack(request, env);
  if (method === "POST" && path === "/v1/enroll") return enroll(request, env);

  // Steam comes back to the browser, which carries no bearer token — the login id in the
  // return URL is what identifies the sign-in, and it is single-use. Above the account
  // gate for that reason, not because it is unprotected.
  if (method === "GET" && path === "/v1/steam/return") return steamReturn(request, url, env);

  // Self-serve signup, no invite. Voice is the reason this exists: a rider on a community
  // server has nobody to talk to unless the people beside them can sign up too. The account
  // it mints can report presence and join a voice room and nothing else — see `invitedOnly`.
  if (method === "POST" && path === "/v1/account") return claimDeviceAccount(request, env);

  // The server list is public, and has to be: it is what the app's join picker offers, and
  // requiring a token meant a player who hadn't enrolled was shown an empty box asking for
  // an IP address — the exact question the registry exists to answer. Nothing here is
  // secret; it is the same name/region/address a server browser shows, and `agent_url` is
  // deliberately not selected, so the admin API's location stays private.
  if (method === "GET" && path === "/v1/servers") return listServers(env);

  // A provisioned box announcing itself. Authenticated by the agent token in its own row,
  // not by an account bearer — the machine holds no account and has no way to be given one.
  const hello = /^\/v1\/servers\/([A-Za-z0-9._-]{1,64})\/hello$/.exec(path);
  if (hello && method === "POST") return serverHello(request, hello[1], env);

  const boot = /^\/v1\/servers\/([A-Za-z0-9._-]{1,64})\/bootstrap$/.exec(path);
  if (boot && method === "POST") return serverBootstrap(request, boot[1], env);

  // Buy Me a Coffee announcing a supporter, for the Discord channel. Unauthenticated in the
  // same sense as the two above: the caller is not a player and holds no bearer token. Its
  // credential is the HMAC signature over the body, checked before the body is parsed.
  if (method === "POST" && path === "/v1/bmac/webhook") return bmacWebhook(request, env);

  // The plugin catalogue, before there is anyone to authenticate. What is on offer is not a
  // secret and the app lists it on a first run, with no account and nothing enrolled.
  if (method === "GET" && path === "/v1/plugins") return listPlugins(env);
  // Anonymous usage counters, from every install rather than every account. Unauthenticated
  // for the reason the feature exists: most people who run the app never claim an invite, so
  // a report that required a token would only ever describe the few who did. Nothing here
  // identifies anyone — see `usage.ts` — and everything about it is bounded by size, by
  // count and by a per-address daily cap.
  if (method === "POST" && path === "/v1/usage") return reportUsage(request, env);

  // Reading the numbers back. Behind `ADMIN_KEY`, above the account gate because it is not a
  // player's endpoint at all: the key belongs to whoever runs the deployment, and an account
  // token must never be enough to read what everybody else is doing.
  if (method === "GET" && path === "/v1/usage/stats") return usageStats(request, url, env);
  if (method === "GET" && path === "/admin/usage") return usageDashboard(request, url, env);

  // What the app sees loaded inside people's running games, and the rules that say how to
  // read it. Behind `ADMIN_KEY` on the same terms as the usage page, and above the account
  // gate for the same reason: the key belongs to whoever runs the deployment, and no
  // player's token should ever be enough to read this about anybody else.
  if (method === "GET" && path === "/admin/diagnostics") {
    return diagnosticsDashboard(request, url, env);
  }
  if (method === "GET" && path === "/admin/diagnostics/riders") {
    return diagnosticsRiders(request, url, env);
  }
  if (method === "GET" && path === "/admin/diagnostics/rider") {
    return diagnosticsRider(request, url, env);
  }
  if (method === "GET" && path === "/admin/diagnostics/files") {
    return diagnosticsFiles(request, url, env);
  }
  if (method === "GET" && path === "/admin/diagnostics/file") {
    return diagnosticsFile(request, url, env);
  }
  if (method === "GET" && path === "/admin/diagnostics/rules") {
    return diagnosticsRulesPage(request, url, env);
  }
  if (method === "POST" && path === "/admin/diagnostics/rules") {
    return diagnosticsRules(request, url, env);
  }

  // Who has published a look, and what we are shipping to a grid. Behind `ADMIN_KEY` and
  // above the account gate on exactly the same terms as the two pages before it: it names
  // riders, their GUIDs and their Steam ids, and no player's token should be enough to read
  // any of that about anybody else.
  if (method === "GET" && path === "/admin/paints") return paintRiders(request, url, env);
  if (method === "GET" && path === "/admin/paints/rider") return paintRider(request, url, env);
  if (method === "GET" && path === "/admin/paints/files") return paintFiles(request, url, env);
  if (method === "GET" && path === "/admin/paints/paint") return paintOne(request, url, env);
  if (method === "GET" && path === "/admin/paints/thumb") return paintThumbnail(request, url, env);

  const account = await authenticate(request, env);
  if (!account) return json(401, { error: "unauthorized" });

  // Open to every account, self-serve ones included: who you are, where you are, and the
  // voice room for the server you said you are on.
  if (method === "GET" && path === "/v1/me") return me(account, env);
  if (method === "PUT" && path === "/v1/me/guid") return putGuid(request, account, env);
  if (method === "PUT" && path === "/v1/me/name") return putName(request, account, env);
  if (method === "PUT" && path === "/v1/presence") return putPresence(request, account, env);
  if (method === "PUT" && path === "/v1/diagnostics") return putReport(request, account, env);
  if (method === "GET" && path === "/v1/voice/ice") return iceServers();

  // Paid plugins. Open to every account on the same terms as voice and paint sync: holding
  // a license is what gates the bundle, not holding an invite.
  if (method === "GET" && path === "/v1/me/plugins") return myPlugins(account, env);
  if (method === "POST" && path === "/v1/plugins/redeem") return redeemKey(request, account, env);
  const bundle = /^\/v1\/plugins\/([a-z0-9-]{1,32})\/bundle$/.exec(path);
  if (bundle && method === "GET") return pluginBundle(bundle[1], account, env);
  // Linking a Steam account, and asking what it may use. Open to every account: identity
  // is the point of the flow, and entitlement is checked per asset when it is asked for.
  if (method === "POST" && path === "/v1/steam/login") return steamLogin(request, account, env);
  if (method === "GET" && path === "/v1/entitlements") return listEntitlements(account, env);
  if (method === "POST" && path === "/v1/entitlements/check") {
    return checkEntitlement(request, account, env);
  }
  if (method === "POST" && path === "/v1/keys/grant") return grantKey(request, account, env);
  if (method === "GET" && path === "/v1/voice/room") return voiceRoom(request, url, account, env);

  // Paint sync, open on the same terms as voice, and for the same reason: a rider only sees
  // the grid correctly if the riders beside them are publishing too, and an invite code is
  // exactly the thing the people beside them do not have. Publishing is bounded by the
  // account itself — a loadout is a fixed set of slots, a paint is size-capped and
  // content-addressed — so an uninvited publisher costs one more row and no more objects
  // than the paints they actually wear.
  if (method === "PUT" && path === "/v1/loadout") return putLoadout(request, account, env);
  if (method === "PUT" && path === "/v1/loadouts") return putLoadouts(request, account, env);
  if (method === "GET" && path === "/v1/roster") return roster(url, account, env);

  const openPaint = /^\/v1\/paints\/([0-9a-f]{64})$/.exec(path);
  if (openPaint) {
    if (method === "PUT") return putPaint(request, openPaint[1], env);
    if (method === "GET") return getPaint(openPaint[1], env);
  }

  // Everything past here needs an invite. The gate is the *position* rather than a check
  // repeated on each route, so a route added below inherits it and one added above is a
  // deliberate decision to open it up. What is left below is the estate: registering a
  // server, provisioning one, and the fleet it bills for.
  const gate = invitedOnly(account);
  if (gate) return gate;

  if (method === "POST" && path === "/v1/servers") return registerServer(request, account, env);
  if (method === "GET" && path === "/v1/servers/mine") return myServers(account, env);
  if (method === "GET" && path === "/v1/fleet") return fleetState(account, env);
  if (method === "POST" && path === "/v1/provision") return provision(request, account, env);
  if (method === "POST" && path === "/v1/images/build") return buildImage(request, account, env);
  if (method === "GET" && path === "/v1/images") return imageStatus(env);

  const owned = /^\/v1\/servers\/([A-Za-z0-9._-]{1,64})$/.exec(path);
  if (owned && method === "DELETE") return deleteServer(owned[1], account, env);

  return json(404, { error: "no such endpoint" });
}

/**
 * Forget yesterday's signup counters.
 *
 * The counter only ever answers "how many today", so a row from last week is a record of an
 * address we said we had no use for. Swept on the same cron as the idle servers.
 */
async function pruneDeviceClaims(env: Env): Promise<void> {
  const cutoff = new Date(Date.now() - 3 * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
  try {
    await env.DB.prepare("DELETE FROM device_claims WHERE day < ?").bind(cutoff).run();
  } catch (err) {
    // A sweep that fails is tomorrow's sweep's problem, not this invocation's.
    console.error(JSON.stringify({ msg: "device claim sweep failed", error: String(err) }));
  }
}

async function authenticate(request: Request, env: Env): Promise<Account | null> {
  const token = bearer(request.headers.get("Authorization"));
  if (!token) return null;
  // Look up by digest: the comparison happens in the index, so there is no string compare
  // of a secret in our code to leak timing.
  const hash = await hashToken(token);
  return await env.DB.prepare(
    "SELECT id, rider_name, steam_id, guid, kind FROM accounts WHERE token_hash = ?",
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

/**
 * Begin a Steam sign-in.
 *
 * Returns a URL rather than redirecting: the caller is the app, which opens it in the
 * player's browser and waits. The login row is what ties the browser's eventual return to
 * the account that started it — the browser itself carries no credential of ours.
 */
async function steamLogin(request: Request, account: Account, env: Env): Promise<Response> {
  const id = crypto.randomUUID();
  await env.DB.prepare(
    "INSERT INTO steam_logins (id, account_id, created_at) VALUES (?, ?, ?)",
  )
    .bind(id, account.id, Date.now())
    .run();

  const origin = new URL(request.url).origin;
  const returnTo = `${origin}/v1/steam/return?login=${id}`;
  return json(200, { url: loginUrl(returnTo, `${origin}/`), loginId: id });
}

/**
 * Steam sending the player back.
 *
 * Everything in the query string is attacker-controlled until Valve confirms it, so the
 * order here is deliberate: find the pending login, check it is still open, then ask Steam
 * whether it really signed this. The row is consumed before anything is written, so a
 * replayed return finds nothing to complete.
 *
 * The response is a page, not JSON — a person is looking at it.
 */
async function steamReturn(request: Request, url: URL, env: Env): Promise<Response> {
  const loginId = url.searchParams.get("login");
  if (!loginId) return page(400, "That sign-in link is incomplete.");

  const login = await env.DB.prepare(
    "SELECT account_id, created_at, consumed_at FROM steam_logins WHERE id = ?",
  )
    .bind(loginId)
    .first<{ account_id: string; created_at: number; consumed_at: number | null }>();

  if (!login) return page(404, "That sign-in has expired or already been used.");
  if (login.consumed_at !== null) return page(409, "That sign-in has already been used.");
  if (Date.now() - login.created_at > LOGIN_TTL_MS) {
    return page(410, "That sign-in took too long. Start it again from the app.");
  }

  const expectedReturnTo = `${url.origin}${url.pathname}`;
  const result = await verifyAssertion(url.searchParams, expectedReturnTo);
  if (!isVerified(result)) {
    return page(403, `Steam couldn't confirm that sign-in: ${result.error}.`);
  }

  // Consumed whatever happens next, so a failed link cannot be retried against a row that
  // has already been through Valve.
  await env.DB.prepare("UPDATE steam_logins SET consumed_at = ? WHERE id = ?")
    .bind(Date.now(), loginId)
    .run();

  // `accounts.steam_id` is UNIQUE: one Steam account is one identity here, and a second
  // community account cannot quietly claim an identity that is already spoken for.
  try {
    await env.DB.prepare("UPDATE accounts SET steam_id = ? WHERE id = ?")
      .bind(result.steamId, login.account_id)
      .run();
  } catch (err) {
    if (String(err).includes("UNIQUE")) {
      return page(409, "That Steam account is already linked to another profile.");
    }
    throw err;
  }

  return page(200, "Steam account linked. You can close this tab and go back to the app.");
}

/** A one-line page for the browser half of the sign-in. */
function page(status: number, message: string): Response {
  const body = `<!doctype html><meta charset="utf-8"><title>MXB App</title>` +
    `<body style="font:16px/1.5 system-ui;margin:4rem auto;max-width:30rem;padding:0 1rem">` +
    `<p>${message.replace(/[<&]/g, (c) => (c === "<" ? "&lt;" : "&amp;"))}</p>`;
  return new Response(body, { status, headers: { "content-type": "text/html; charset=utf-8" } });
}

/**
 * What this player may use.
 *
 * Empty for an account with no Steam link — not an error. Entitlement is keyed on the
 * Steam identity, so an unlinked account simply owns nothing yet, and saying so plainly is
 * more useful than a failure the app has to interpret.
 */
async function listEntitlements(account: Account, env: Env): Promise<Response> {
  if (!account.steam_id) return json(200, { steamId: null, assets: [] });

  const rows = await env.DB.prepare(
    "SELECT e.asset_id, a.title, e.source, e.granted_at" +
      " FROM entitlements e JOIN assets a ON a.id = e.asset_id" +
      " WHERE e.steam_id = ? AND e.revoked_at IS NULL AND a.withdrawn_at IS NULL" +
      " ORDER BY e.granted_at DESC",
  )
    .bind(account.steam_id)
    .all<{ asset_id: string; title: string; source: string; granted_at: number }>();

  return json(200, {
    steamId: account.steam_id,
    assets: (rows.results ?? []).map((r) => ({
      assetId: r.asset_id,
      title: r.title,
      source: r.source,
      grantedAt: r.granted_at,
    })),
  });
}

/**
 * May this player use this asset, right now?
 *
 * The question the whole system exists to answer. No key is minted here and no crypto
 * happens — this is the decision, proved end to end before there is anything to decrypt.
 *
 * Every call is written to the audit log, refusals included: one identity walking the
 * catalogue is only visible if the "no"s are recorded too.
 */
/**
 * The single entitlement decision, and the single audit write.
 *
 * Extracted so the check and the key grant cannot drift: a key is released on exactly the
 * condition the check reports, because they are the same function. Both the withdrawn-asset
 * and the revoked-entitlement branches matter to the grant especially — a key must stop
 * being issued the instant either flips, which is what makes revocation real.
 *
 * Every call is logged, refusals included: one identity walking the catalogue is only
 * visible if the "no"s are written down too.
 */
async function decideEntitlement(
  account: Account,
  assetId: string,
  session: string,
  env: Env,
): Promise<{ allowed: boolean; reason: string }> {
  const decide = async (): Promise<{ allowed: boolean; reason: string }> => {
    if (!account.steam_id) return { allowed: false, reason: "no Steam account linked" };
    const asset = await env.DB.prepare("SELECT withdrawn_at FROM assets WHERE id = ?")
      .bind(assetId)
      .first<{ withdrawn_at: number | null }>();
    if (!asset) return { allowed: false, reason: "no such asset" };
    if (asset.withdrawn_at !== null) return { allowed: false, reason: "withdrawn" };

    const row = await env.DB.prepare(
      "SELECT revoked_at FROM entitlements WHERE steam_id = ? AND asset_id = ?",
    )
      .bind(account.steam_id, assetId)
      .first<{ revoked_at: number | null }>();
    if (!row) return { allowed: false, reason: "not entitled" };
    if (row.revoked_at !== null) return { allowed: false, reason: "revoked" };
    return { allowed: true, reason: "entitled" };
  };

  const result = await decide();
  await env.DB.prepare(
    "INSERT INTO entitlement_grants (steam_id, asset_id, session_id, decision, reason, issued_at)" +
      " VALUES (?, ?, ?, ?, ?, ?)",
  )
    .bind(
      account.steam_id ?? "unlinked",
      assetId,
      session,
      result.allowed ? "allow" : "deny",
      result.reason,
      Date.now(),
    )
    .run();
  return result;
}

/** Pull and validate `{ assetId, sessionId }` from a request body. */
async function assetRequest(
  request: Request,
): Promise<{ assetId: string; session: string } | Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { assetId, sessionId } = body as { assetId?: unknown; sessionId?: unknown };
  if (typeof assetId !== "string" || !assetId.trim()) {
    return json(400, { error: "an asset id is required" });
  }
  const session = typeof sessionId === "string" && sessionId.trim() ? sessionId.trim() : "none";
  return { assetId: assetId.trim(), session };
}

async function checkEntitlement(request: Request, account: Account, env: Env): Promise<Response> {
  const parsed = await assetRequest(request);
  if (parsed instanceof Response) return parsed;
  const { allowed, reason } = await decideEntitlement(account, parsed.assetId, parsed.session, env);
  return json(allowed ? 200 : 403, { allowed, reason });
}

/**
 * Release the content key for a secured asset to an entitled session.
 *
 * The step the DLL cannot proceed without: it has the ciphertext blob and needs the key to
 * open it. The key is released on exactly the entitlement check above — same function, so a
 * withdrawal or revocation stops key issuance immediately — and only after the master-key
 * secret unwraps the stored key. The raw content key leaves the server only here, only to a
 * caller Valve's identity and our entitlement both vouch for, and the grant is audited like
 * any other.
 *
 * `ttlSeconds` tells the DLL how briefly to hold it before asking again, which is what keeps
 * revocation to one TTL rather than one session.
 */
async function grantKey(request: Request, account: Account, env: Env): Promise<Response> {
  const parsed = await assetRequest(request);
  if (parsed instanceof Response) return parsed;
  const { assetId, session } = parsed;

  const { allowed, reason } = await decideEntitlement(account, assetId, session, env);
  if (!allowed) return json(403, { error: reason });

  const asset = await env.DB.prepare("SELECT wrapped_key, key_id FROM assets WHERE id = ?")
    .bind(assetId)
    .first<{ wrapped_key: string | null; key_id: string | null }>();
  if (!asset?.wrapped_key) {
    // Entitled, but the asset has no stored key — it was never packed, or was registered
    // before key custody existed. Not the caller's fault and not a 403: there is simply
    // nothing to hand back.
    return json(409, { error: "asset has no content key" });
  }

  const key = await unwrapContentKey(asset.wrapped_key, env.MXB_ASSET_MASTER_KEY);
  if (!key) {
    // No master key configured, or the stored key doesn't unwrap. Either way this
    // deployment cannot serve secured content right now; say so rather than 200 with
    // nothing usable.
    return json(503, { error: "content keys are unavailable" });
  }

  return json(200, {
    assetId,
    keyId: asset.key_id ?? null,
    contentKey: b64(key),
    ttlSeconds: KEY_GRANT_TTL_SECONDS,
  });
}

/** How briefly the DLL should cache a released key before re-checking entitlement. */
const KEY_GRANT_TTL_SECONDS = 300;

/** Base64 of raw bytes, for handing the key back over JSON. */
function b64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

/**
 * Refuse anything a self-serve account has no business doing.
 *
 * Voice and paint sync are open to everyone with the app — both are worthless unless the
 * riders beside you can use them too. Registering a server and provisioning one are not:
 * they spend real money and are tied to a person we have vouched for. Called once, at the
 * point in the route table where the open endpoints end.
 */
function invitedOnly(account: Account): Response | null {
  if (account.kind === "invited") return null;
  return json(403, { error: "that needs an invite" });
}

/**
 * The account, and what is actually stored for it.
 *
 * The per-bike summary is what lets the app say "published — 6 bikes, 14 paints" from the
 * server's own record rather than from its optimism about a request it made. Those are not
 * the same thing the moment a publish half-fails, which is exactly when a player looks.
 */
async function me(account: Account, env: Env): Promise<Response> {
  const paints = await env.DB.prepare(
    "SELECT bike_id, slot, file_name, sha256, size FROM loadout_paints WHERE account_id = ?" +
      " ORDER BY bike_id, slot",
  )
    .bind(account.id)
    .all<{ bike_id: string; slot: string; file_name: string; sha256: string; size: number }>();

  const updated = await env.DB.prepare(
    "SELECT bike_id, updated_at FROM loadouts WHERE account_id = ?",
  )
    .bind(account.id)
    .all<{ bike_id: string; updated_at: number }>();
  const updatedAt = new Map(updated.results.map((r) => [r.bike_id, r.updated_at]));

  const bikes = new Map<string, { bikeId: string; paints: number; updatedAt: number | null }>();
  for (const p of paints.results) {
    const bike = bikes.get(p.bike_id) ?? {
      bikeId: p.bike_id,
      paints: 0,
      updatedAt: updatedAt.get(p.bike_id) ?? null,
    };
    bike.paints += 1;
    bikes.set(p.bike_id, bike);
  }

  return json(200, {
    accountId: account.id,
    riderName: account.rider_name,
    steamId: account.steam_id,
    guid: account.guid,
    bikes: [...bikes.values()],
    totalPaints: paints.results.length,
    paints: paints.results.map((p) => ({
      bikeId: p.bike_id,
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
/**
 * Take the rider name the game itself uses.
 *
 * The name an account enrolled under was whatever the app could find on disk, which is the
 * *profile folder*'s name — and a player who never renamed their profile is called
 * `unnamedProfile`, along with two hundred others. That name is not what the server shows:
 * `EventInit` hands the plugin `m_szRiderName`, the name every other rider on the grid sees,
 * and that is what arrives here.
 *
 * Idempotent, because it is sent from a poll: the same name twice is a no-op rather than an
 * error. Uniqueness is only enforced for invited accounts (see 0012), so a clash is a real
 * answer for those and impossible for the rest.
 */
async function putName(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { riderName } = body as { riderName?: unknown };
  if (!isRiderName(riderName)) return json(400, { error: "that isn't a usable rider name" });

  const name = (riderName as string).trim();
  if (name === account.rider_name) return json(200, { ok: true, riderName: name });

  try {
    await env.DB.prepare("UPDATE accounts SET rider_name = ? WHERE id = ?")
      .bind(name, account.id)
      .run();
  } catch (err) {
    if (String(err).includes("UNIQUE")) {
      return json(409, { error: "another account already uses that rider name" });
    }
    throw err;
  }
  return json(200, { ok: true, riderName: name });
}

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

/** A publish carries at most this many bikes, matching the app's own cap. */
const MAX_BIKES = 32;

/** And at most this many paints per bike — a loadout has one slot each. */
const MAX_PAINTS_PER_BIKE = 16;

/** One bike's worth of validated rows, ready to insert. */
interface BikeRows {
  bikeId: string;
  rows: { slot: string; fileName: string; sha256: string; size: number; relDest: string }[];
}

/**
 * Validate `{ bikeId, paints }` into rows, or return the message explaining why not.
 *
 * `relDest` is the value that becomes a path on another player's disk, so it is checked here
 * as well as by the app that receives it. Neither check is redundant: this one keeps bad rows
 * out of the database, and the app's keeps a bad row already in it from reaching a filesystem.
 */
function bikeRows(value: unknown): BikeRows | string {
  const { bikeId, paints } = (value ?? {}) as { bikeId?: unknown; paints?: unknown };
  if (!isBikeId(bikeId)) return `not a usable bike id: ${String(bikeId)}`;
  if (!Array.isArray(paints)) return `paints must be an array for ${String(bikeId)}`;
  if (paints.length > MAX_PAINTS_PER_BIKE) return `too many paints for ${String(bikeId)}`;

  const rows: BikeRows["rows"] = [];
  const seen = new Set<string>();
  for (const entry of paints) {
    const p = entry as Record<string, unknown>;
    if (!isSlot(p.slot)) return `unknown slot: ${String(p.slot)}`;
    // A slot twice in one bike would violate the primary key and fail the whole batch, so
    // it is named here rather than surfacing as an opaque constraint error.
    if (seen.has(p.slot)) return `${p.slot} given twice for ${bikeId}`;
    seen.add(p.slot);
    if (!isPaintFileName(p.fileName)) return `invalid paint filename for ${p.slot}`;
    if (!isRelDest(p.relDest)) return `invalid destination for ${p.slot}`;
    if (!isSha256(p.sha256)) return `invalid sha256 for ${p.slot}`;
    if (!isPaintSize(p.size)) return `invalid size for ${p.slot}`;
    rows.push({
      slot: p.slot,
      fileName: (p.fileName as string).trim(),
      sha256: p.sha256,
      size: p.size,
      relDest: (p.relDest as string).trim(),
    });
  }
  return { bikeId: (bikeId as string).trim(), rows };
}

/** Write a set of bikes, replacing `scope` wholesale, and report the blobs still missing. */
async function storeLoadouts(
  account: Account,
  bikes: BikeRows[],
  scope: "all" | "these",
  env: Env,
): Promise<Response> {
  const now = Date.now();
  // Replace rather than merge: a slot the player cleared has to disappear, and merging would
  // leave them wearing something they took off. `scope: "all"` is a whole-profile publish, so
  // a bike they no longer have any custom paint on goes too.
  const clear =
    scope === "all"
      ? [
          env.DB.prepare("DELETE FROM loadout_paints WHERE account_id = ?").bind(account.id),
          env.DB.prepare("DELETE FROM loadouts WHERE account_id = ?").bind(account.id),
        ]
      : bikes.flatMap((b) => [
          env.DB.prepare("DELETE FROM loadout_paints WHERE account_id = ? AND bike_id = ?").bind(
            account.id,
            b.bikeId,
          ),
        ]);

  const statements = [
    ...clear,
    ...bikes.map((b) =>
      env.DB.prepare(
        "INSERT INTO loadouts (account_id, bike_id, updated_at) VALUES (?, ?, ?)" +
          " ON CONFLICT(account_id, bike_id) DO UPDATE SET updated_at = excluded.updated_at",
      ).bind(account.id, b.bikeId, now),
    ),
    ...bikes.flatMap((b) =>
      b.rows.map((r) =>
        env.DB.prepare(
          "INSERT INTO loadout_paints (account_id, bike_id, slot, file_name, sha256, size, rel_dest)" +
            " VALUES (?, ?, ?, ?, ?, ?, ?)",
        ).bind(account.id, b.bikeId, r.slot, r.fileName, r.sha256, r.size, r.relDest),
      ),
    ),
  ];
  await env.DB.batch(statements);

  // Tell the client which blobs we still need, so it uploads only what nobody has shared
  // yet. Content addressing makes this cheap: the same paint from twenty riders is one
  // object and nineteen skipped uploads. De-duplicated first — gear repeats across bikes,
  // so a whole-profile publish asks about the same digest many times over.
  const wanted = [...new Set(bikes.flatMap((b) => b.rows.map((r) => r.sha256)))];
  const missing: string[] = [];
  for (const sha of wanted) {
    if (!(await env.PAINTS.head(sha))) missing.push(sha);
  }
  return json(200, { ok: true, missing });
}

/**
 * Replace one bike's loadout.
 *
 * Kept for clients older than per-bike storage, which send one bike at a time and would
 * otherwise have every other bike deleted out from under them on each publish.
 */
async function putLoadout(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const bike = bikeRows(body);
  if (typeof bike === "string") return json(400, { error: bike });
  return storeLoadouts(account, [bike], "these", env);
}

/**
 * Replace this rider's whole look, every bike at once.
 *
 * Which bike a rider takes out is decided in the game, and nothing tells us which one that
 * will be — so the only answer that always renders correctly is to hold all of them. Sending
 * them together also makes the replacement atomic: there is no moment where half a profile
 * is published.
 */
async function putLoadouts(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { bikes } = body as { bikes?: unknown };
  if (!Array.isArray(bikes)) return json(400, { error: "bikes must be an array" });
  if (bikes.length > MAX_BIKES) return json(400, { error: `at most ${MAX_BIKES} bikes` });

  const parsed: BikeRows[] = [];
  const seen = new Set<string>();
  for (const entry of bikes) {
    const bike = bikeRows(entry);
    if (typeof bike === "string") return json(400, { error: bike });
    const key = bike.bikeId.toLowerCase();
    if (seen.has(key)) return json(400, { error: `${bike.bikeId} given twice` });
    seen.add(key);
    parsed.push(bike);
  }
  return storeLoadouts(account, parsed, "all", env);
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
 * What bikes are mirrored, so a server can install them without being told the list.
 *
 * A dedicated server rejects any bike it does not itself have installed — which is why a
 * freshly provisioned one refuses every rider on a mod bike. Mirroring the pack means a new
 * server can be rideable the moment it boots rather than only for whoever owns stock content.
 */
async function listContent(env: Env): Promise<Response> {
  const listed = await env.PAINTS.list({ prefix: "content/bikes/" });
  const bikes = listed.objects
    .map((o) => ({ name: o.key.slice("content/bikes/".length), size: o.size }))
    .filter((b) => b.name.length > 0);
  return json(200, { bikes, totalBytes: bikes.reduce((n, b) => n + b.size, 0) });
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
 * The ports a provisioned box uses.
 *
 * Fixed rather than configurable: the bootstrap writes them into `agent.json` and opens them
 * in the firewall, so anything reading them back has to agree. They were repeated as literals
 * in three places, one of which — the reaper — silently decides whether a server is reachable.
 */
const AGENT_PORT = 8787;
const GAME_PORT = 54210;

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
 * The *count* is everyone's business — it is what the concurrency cap is measured against, and
 * hiding it from the people it constrains would be perverse. The instance list is not: an
 * instance id and public IP belong to whoever is paying for that box, so only its owner sees
 * the row. Splitting the two is what lets the panel say "1 of 2 running" without handing every
 * enrolled account the address of every server.
 */
async function fleetState(account: Account, env: Env): Promise<Response> {
  const aws = awsEnv(env);
  if (!aws) return json(503, { error: "provisioning isn't configured on this deployment" });
  try {
    const instances = await fleet(aws);
    const mine = await env.DB.prepare(
      "SELECT instance_id FROM servers WHERE owner_account_id = ? AND instance_id IS NOT NULL",
    )
      .bind(account.id)
      .all<{ instance_id: string }>();
    const owned = new Set(mine.results.map((r) => r.instance_id));
    return json(200, {
      region: REGION,
      running: instances.length,
      cap: Number(env.MXB_MAX_INSTANCES ?? "2"),
      instances: instances.filter((i) => owned.has(i.instanceId)),
    });
  } catch (err) {
    console.error(JSON.stringify({ msg: "fleet", error: String(err) }));
    return json(502, { error: String(err) });
  }
}

/** Where the built image's id lives, and where a build in progress keeps its place. */
const SETTING_AMI = "server_ami_id";
const SETTING_PENDING_AMI = "pending_ami_id";

async function getSetting(env: Env, key: string): Promise<string | null> {
  const row = await env.DB.prepare("SELECT value FROM settings WHERE key = ?")
    .bind(key)
    .first<{ value: string }>();
  return row?.value ?? null;
}

async function setSetting(env: Env, key: string, value: string | null): Promise<void> {
  if (value === null) {
    await env.DB.prepare("DELETE FROM settings WHERE key = ?").bind(key).run();
    return;
  }
  await env.DB.prepare(
    "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)" +
      " ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
  )
    .bind(key, value, Date.now())
    .run();
}

/**
 * Build the image every later server launches from.
 *
 * Installing the game and the bike pack takes a quarter of an hour and produces the same disk
 * every time. Doing it once and snapshotting the result is the difference between a server
 * arriving in fifteen minutes and arriving in two — and it stops getting slower as the pack
 * grows, which it just did, from 1.9 GB to 3.8.
 *
 * The builder is an ordinary provisioned instance running the ordinary bootstrap, so what gets
 * captured is exactly what a working server looks like. It is marked with a role so the idle
 * reaper leaves it alone: an instance with nobody connected is precisely what a builder is.
 */
async function buildImage(request: Request, account: Account, env: Env): Promise<Response> {
  void request;
  const aws = awsEnv(env);
  if (!aws) return json(503, { error: "provisioning isn't configured on this deployment" });
  const securityGroupId = env.MXB_SECURITY_GROUP_ID?.trim();
  const agentDownload = env.MXB_AGENT_DOWNLOAD_URL?.trim();
  const gameDownload = env.MXB_GAME_DOWNLOAD_URL?.trim();
  if (!securityGroupId || !agentDownload || !gameDownload) {
    return json(503, { error: "provisioning isn't finished being set up yet" });
  }
  if (await getSetting(env, SETTING_PENDING_AMI)) {
    return json(409, { error: "an image is already being built" });
  }
  const existingBuilder = await env.DB.prepare(
    "SELECT id FROM servers WHERE role = 'builder'",
  ).first<{ id: string }>();
  if (existingBuilder) return json(409, { error: "a builder is already running" });

  const id = crypto.randomUUID();
  const agentToken = newToken();
  await env.DB.prepare(
    "INSERT INTO servers (id, name, region, address, created_at, owner_account_id, published," +
      " agent_token, role) VALUES (?, ?, ?, '', ?, ?, 0, ?, 'builder')",
  )
    .bind(id, "image builder", REGION, Date.now(), account.id, agentToken)
    .run();

  try {
    const amiId = await latestWindowsAmi(aws);
    const instanceId = await runInstance(
      aws,
      {
        name: "mxb image builder",
        instanceType: env.MXB_INSTANCE_TYPE?.trim() || "t3.small",
        securityGroupId,
        userData: bootstrapScript({
          agentToken,
          agentUrl: agentDownload,
          gameUrl: gameDownload,
          serverName: "image builder",
          gamePort: GAME_PORT,
          agentPort: AGENT_PORT,
          serverId: id,
          controlPlaneUrl: new URL(request.url).origin,
        }),
      },
      amiId,
    );
    await env.DB.prepare("UPDATE servers SET instance_id = ? WHERE id = ?")
      .bind(instanceId, id)
      .run();
    return json(201, { id, instanceId, state: "building" });
  } catch (err) {
    await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(id).run();
    console.error(JSON.stringify({ msg: "buildImage", error: String(err) }));
    return json(502, { error: String(err) });
  }
}

/** What the image situation is: none, building, baking, or ready. */
async function imageStatus(env: Env): Promise<Response> {
  const ami = await getSetting(env, SETTING_AMI);
  const pending = await getSetting(env, SETTING_PENDING_AMI);
  const builder = await env.DB.prepare(
    "SELECT bootstrap_stage FROM servers WHERE role = 'builder'",
  ).first<{ bootstrap_stage: string | null }>();
  return json(200, {
    amiId: ami,
    pendingAmiId: pending,
    builderStage: builder?.bootstrap_stage ?? null,
    state: ami ? "ready" : pending ? "baking" : builder ? "building" : "none",
  });
}

/**
 * Move a finished build along, one cron tick at a time.
 *
 * Three states, none of which can be waited on inline: the builder installing (~15 minutes),
 * EC2 baking the image (~10), and then the swap. Each run does whichever step is due.
 */
async function advanceImageBuild(env: Env): Promise<void> {
  const aws = awsEnv(env);
  if (!aws) return;

  const pending = await getSetting(env, SETTING_PENDING_AMI);
  if (pending) {
    const state = await imageState(aws, pending);
    if (state === "available") {
      const previous = await getSetting(env, SETTING_AMI);
      await setSetting(env, SETTING_AMI, pending);
      await setSetting(env, SETTING_PENDING_AMI, null);
      console.log(JSON.stringify({ msg: "image ready", ami: pending, replaced: previous }));
    } else if (state === "failed" || state === null) {
      // Nothing to salvage, and leaving it pending would block every future build.
      await setSetting(env, SETTING_PENDING_AMI, null);
      console.error(JSON.stringify({ msg: "image failed", ami: pending, state }));
    }
    return;
  }

  const builder = await env.DB.prepare(
    "SELECT id, instance_id, bootstrap_stage FROM servers WHERE role = 'builder'",
  ).first<{ id: string; instance_id: string | null; bootstrap_stage: string | null }>();
  if (!builder?.instance_id) return;

  // An instance that is no longer there cannot finish. Without this the row sits at
  // "building" forever and every later build is refused as one already in progress — which is
  // exactly what happened when the reaper was killing builders as orphans.
  const live = await fleet(aws).catch(() => []);
  if (!live.some((i) => i.instanceId === builder.instance_id)) {
    await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(builder.id).run();
    console.error(
      JSON.stringify({ msg: "image builder vanished", id: builder.id, instance: builder.instance_id }),
    );
    return;
  }

  if (builder.bootstrap_stage === "ready") {
    const imageId = await createImage(aws, builder.instance_id, `mxb-server-${Date.now()}`);
    await setSetting(env, SETTING_PENDING_AMI, imageId);
    // CreateImage stops the instance to take a consistent disk; it is of no further use.
    await terminateInstance(aws, builder.instance_id);
    await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(builder.id).run();
    console.log(JSON.stringify({ msg: "image baking", ami: imageId }));
  } else if (builder.bootstrap_stage === "failed") {
    await terminateInstance(aws, builder.instance_id).catch(() => {});
    await env.DB.prepare("DELETE FROM servers WHERE id = ?").bind(builder.id).run();
    console.error(JSON.stringify({ msg: "image build failed", id: builder.id }));
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
    // Launch from the prebuilt image when there is one: the game and the bike pack are
    // already on its disk, so the server only has to write its own config and start. That is
    // two minutes instead of fifteen, and it does not grow with the pack.
    const built = await getSetting(env, SETTING_AMI);
    const amiId = built ?? (await latestWindowsAmi(aws));
    const script = built ? imageBootstrapScript : bootstrapScript;
    const instanceId = await runInstance(
      aws,
      {
        name: `mxb ${(name as string).trim()}`,
        instanceType: env.MXB_INSTANCE_TYPE?.trim() || "t3.small",
        securityGroupId,
        userData: script({
          agentToken,
          agentUrl: agentDownload,
          gameUrl: gameDownload,
          serverName: (name as string).trim(),
          gamePort: GAME_PORT,
          agentPort: AGENT_PORT,
          serverId: id,
          // Taken from the request rather than configured, so a preview deployment's boxes
          // announce themselves to that preview rather than to production.
          controlPlaneUrl: new URL(request.url).origin,
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
 * A provisioned box announcing that it is up, and where it is.
 *
 * This is the one thing that made cloud servers real. A launched instance had no way to be
 * reached: its public IP exists only in EC2's view, its agent token only in this table and on
 * the box, and nothing ever wrote `address` or flipped `published` — so a server the app
 * created could never be joined, managed or even found. Now the box says so itself.
 *
 * **The address is observed, not supplied.** The IP comes from `cf-connecting-ip` rather than
 * from the request body, so a box that is compromised — or an operator poking at this by hand
 * — cannot register somebody else's address into everyone's join picker. Only the ports are
 * taken from the body, and only after the token proves the caller is that server.
 *
 * Idempotent: a bootstrap that retries, or an instance that reboots and announces itself
 * again, simply rewrites the same row.
 */
/**
 * Prove a request came from the box a row was provisioned for.
 *
 * The agent token is the only credential that machine has — it holds no account and there is
 * no way to give it one. Shared by every route the box itself calls, so the answer to a wrong
 * token cannot drift between them: an unknown id and a bad token return the same 403, which
 * is what stops this being a way to discover which server ids exist.
 */
async function asProvisionedServer(
  request: Request,
  id: string,
  env: Env,
): Promise<{ instance_id: string | null } | null> {
  const row = await env.DB.prepare("SELECT agent_token, instance_id FROM servers WHERE id = ?")
    .bind(id)
    .first<{ agent_token: string | null; instance_id: string | null }>();
  const presented = bearer(request.headers.get("Authorization"));
  if (!row?.agent_token || !presented || !tokenMatches(row.agent_token, presented)) {
    return null;
  }
  return { instance_id: row.instance_id };
}

/**
 * A booting instance reporting what it is doing, and why it died if it did.
 *
 * Nothing else can tell you. The box has no console and no key pair, so `C:\mxb-bootstrap.log`
 * is unreadable from outside — and the bootstrap's own failure trap shuts the machine down,
 * which terminates it and takes the log with it. Before this, a bootstrap that failed at
 * minute twelve and one still downloading looked exactly the same from here: nothing.
 *
 * Also what lets "Create a server" say `extracting the game` instead of spinning for a
 * quarter of an hour with nothing to show.
 */
async function serverBootstrap(request: Request, id: string, env: Env): Promise<Response> {
  if (!(await asProvisionedServer(request, id, env))) {
    return json(403, { error: "not this server" });
  }
  const body = (await readJson(request)) as
    | { stage?: unknown; ok?: unknown; log?: unknown }
    | null;
  if (!isBootstrapStage(body?.stage)) return json(400, { error: "that isn't a stage" });

  // Kept only when something went wrong, and only the tail: this is a transcript written by a
  // script, and there is no version of "store all of it" that ends well.
  const log =
    body?.ok === false && typeof body.log === "string"
      ? body.log.slice(-MAX_BOOTSTRAP_LOG)
      : null;

  await env.DB.prepare(
    "UPDATE servers SET bootstrap_stage = ?, bootstrap_at = ?, bootstrap_log = COALESCE(?, bootstrap_log)" +
      " WHERE id = ?",
  )
    .bind((body!.stage as string).trim(), Date.now(), log, id)
    .run();

  console.log(JSON.stringify({ msg: "bootstrap", id, stage: body!.stage, failed: log !== null }));
  return json(200, { ok: true });
}

async function serverHello(request: Request, id: string, env: Env): Promise<Response> {
  const row = await asProvisionedServer(request, id, env);
  if (!row) {
    return json(403, { error: "not this server" });
  }

  const body = (await readJson(request)) as { agentPort?: unknown; gamePort?: unknown } | null;
  const agentPort = isPort(body?.agentPort) ? body!.agentPort : AGENT_PORT;
  const gamePort = isPort(body?.gamePort) ? body!.gamePort : GAME_PORT;

  const ip = (request.headers.get("cf-connecting-ip") ?? "").trim();
  // Behind `wrangler dev` there is no edge header and the caller is loopback; refusing that
  // is right for production and is why the local exercise of this uses a public test address.
  if (!isPublicGameAddress(`${ip}:${gamePort}`)) {
    return json(400, { error: "that address isn't reachable from the internet" });
  }

  const now = Date.now();
  await env.DB.prepare(
    "UPDATE servers SET address = ?, agent_url = ?, checked_at = ?, published = 1 WHERE id = ?",
  )
    .bind(`${ip}:${gamePort}`, `http://${ip}:${agentPort}`, now, id)
    .run();

  console.log(JSON.stringify({ msg: "hello", id, instance: row.instance_id, address: ip }));
  return json(200, { ok: true, address: `${ip}:${gamePort}` });
}

function isPort(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0 && value <= 65535;
}

/**
 * The servers this account runs, with what it takes to drive them.
 *
 * Includes `agent_token`, which the public list deliberately never carries. That is the
 * point: a box the control plane launched has no console and prints its pairing code to
 * nobody, so the owner has no other way to obtain the credential for a server they are
 * paying for. Scoped strictly to `owner_account_id`, so it is only ever the caller's own.
 *
 * `state` and `publicIp` come from EC2 rather than from this table, which is what lets the app
 * distinguish "still booting" from "up but not answering".
 */
async function myServers(account: Account, env: Env): Promise<Response> {
  const rows = await env.DB.prepare(
    "SELECT id, name, region, address, agent_url, agent_token, instance_id, published," +
      " created_at, idle_since, bootstrap_stage, bootstrap_at, bootstrap_log" +
      " FROM servers WHERE owner_account_id = ? ORDER BY created_at",
  )
    .bind(account.id)
    .all<{
      id: string;
      name: string;
      region: string;
      address: string;
      agent_url: string | null;
      agent_token: string | null;
      instance_id: string | null;
      published: number;
      created_at: number;
      idle_since: number | null;
      bootstrap_stage: string | null;
      bootstrap_at: number | null;
      bootstrap_log: string | null;
    }>();

  // Only ask EC2 when at least one row is a machine we launched; a player who only runs
  // servers on their own hardware shouldn't make this endpoint depend on AWS at all.
  const aws = rows.results.some((r) => r.instance_id) ? awsEnv(env) : null;
  let live = new Map<string, { state: string; publicIp: string | null }>();
  if (aws) {
    try {
      live = new Map((await fleet(aws)).map((i) => [i.instanceId, i]));
    } catch (err) {
      // The rows are still useful without it — the panel just can't say "booting".
      console.error(JSON.stringify({ msg: "servers/mine fleet", error: String(err) }));
    }
  }

  return json(200, {
    servers: rows.results.map((r) => {
      const instance = r.instance_id ? live.get(r.instance_id) : undefined;
      return {
        id: r.id,
        name: r.name,
        region: r.region,
        address: r.address,
        // Falls back to EC2's view for the window between the instance getting an IP and its
        // bootstrap finishing — the agent is not up yet, but the app can already show where.
        agentUrl:
          r.agent_url ?? (instance?.publicIp ? `http://${instance.publicIp}:${AGENT_PORT}` : null),
        agentToken: r.agent_token,
        instanceId: r.instance_id,
        published: r.published === 1,
        createdAt: r.created_at,
        idleSince: r.idle_since,
        idleMinutes: Number(env.MXB_IDLE_MINUTES ?? "20"),
        state: r.instance_id ? (instance?.state ?? "gone") : "self-hosted",
        publicIp: instance?.publicIp ?? null,
        // What the box last said it was doing, and why it gave up if it did. The only
        // window into a machine with no console.
        bootstrapStage: r.bootstrap_stage,
        bootstrapAt: r.bootstrap_at,
        bootstrapLog: r.bootstrap_log,
      };
    }),
  });
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
    // Every row with an instance, builders included. Filtering them out here is what killed
    // the first image build: absent from this map, the builder matched the orphan check below
    // — "no database row points at this instance" — and was terminated within five minutes of
    // launching. Builders are skipped at the idle check instead, which is the only part that
    // should ignore them.
    "SELECT id, instance_id, agent_token, idle_since, role FROM servers" +
      " WHERE instance_id IS NOT NULL",
  ).all<{
    id: string;
    instance_id: string;
    agent_token: string | null;
    idle_since: number | null;
    role: string | null;
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

    // A builder has nobody connected because nobody rides on it. Reaping one throws away a
    // quarter of an hour of install; `advanceImageBuild` owns its lifetime instead.
    if (row.role === "builder") continue;

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
    const agentUrl = instance.publicIp ? `http://${instance.publicIp}:${AGENT_PORT}` : null;
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
 * "I am on this server."
 *
 * Reported by the player's own app, which knows the server it launched into. That is what
 * lets a roster mean one grid instead of every enrolled account — see `0008_presence.sql`.
 *
 * One row per account: a rider is in one place at a time, and the last thing they said wins.
 */
async function putPresence(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { serverId } = body as { serverId?: unknown };
  if (!isServerKey(serverId)) return json(400, { error: "that isn't a server" });

  await markPresent(account.id, (serverId as string).trim(), env);
  return json(200, { ok: true });
}

/** Record that an account is on a server. One writer, so the two callers cannot drift. */
async function markPresent(accountId: string, serverId: string, env: Env): Promise<void> {
  await env.DB.prepare(
    "INSERT INTO presence (account_id, server_id, updated_at) VALUES (?, ?, ?)" +
      " ON CONFLICT(account_id) DO UPDATE SET server_id = excluded.server_id," +
      " updated_at = excluded.updated_at",
  )
    .bind(accountId, serverId, Date.now())
    .run();
}

/**
 * The riders on one server, and what they are wearing.
 *
 * This is what the app polls to know which paints to fetch. It is scoped to the riders whose
 * apps have said they are on this server recently — previously it returned every enrolled
 * account, because nothing recorded where anyone was, so every rider downloaded the paints of
 * every other rider on the platform.
 *
 * A rider whose app has gone quiet for `PRESENCE_TTL_MS` drops out. Better to miss someone
 * who is genuinely there — the next heartbeat brings them back within a minute — than to
 * accumulate a grid of people who left hours ago.
 */
/**
 * Who is on a server, and what they are wearing.
 *
 * `?here=1` also records that the caller is on that server, which is what lets a client in a
 * session do the whole loop in one request instead of a presence write followed by this
 * read. That halving is not a micro-optimisation: every app in a session ran both every 45
 * seconds, and on 2026-08-31 the pair took the worker past its daily request ceiling within
 * hours of paint sync being turned on for everyone.
 *
 * Absent, nothing is written — a client sweeping the registry is asking about servers it is
 * *not* on, and claiming presence on all of them would be both untrue and the thing that
 * made every rider download every other rider's paints.
 */
async function roster(url: URL, account: Account, env: Env): Promise<Response> {
  const serverId = url.searchParams.get("server");
  if (!serverId) return json(400, { error: "a server id is required" });

  if (url.searchParams.get("here") === "1") {
    // Held to the same rule as the standalone report: this one writes a row other clients
    // read, so it cannot be looser about what a server key is just because it arrived in a
    // query string.
    if (!isServerKey(serverId)) return json(400, { error: "that isn't a server" });
    // Before the read, not after: the roster is scoped by presence, and a rider should
    // appear in the same answer they are asking for.
    await markPresent(account.id, serverId.trim(), env);
  }

  // DISTINCT on the destination: loadouts are per bike, and gear repeats across every bike a
  // rider owns, so the raw join returns the same helmet paint many times over. The receiver
  // installs by destination and de-duplicates anyway.
  //
  // MIN(slot) only picks a stable representative for a destination two slots agree on; the
  // client keys on rel_dest, not on slot.
  const rows = await env.DB.prepare(
    "SELECT a.rider_name, a.guid, MIN(p.slot) AS slot, p.file_name, p.sha256, p.size, p.rel_dest" +
      " FROM accounts a" +
      " JOIN presence pr ON pr.account_id = a.id" +
      " JOIN loadout_paints p ON p.account_id = a.id" +
      " WHERE pr.server_id = ? AND pr.updated_at > ?" +
      " GROUP BY a.id, p.rel_dest, p.sha256",
  )
    .bind(serverId, Date.now() - PRESENCE_TTL_MS)
    .all<{
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
    // Re-checked on the way out as well as in. A row predating the validation, or one written
    // by some future path that forgot it, must not reach a client that is about to turn it
    // into a filesystem path.
    if (!isRelDest(r.rel_dest)) continue;
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

/** Read a column we wrote as JSON. A row that somehow isn't parseable is an empty list, not
 *  a 500 — one bad row must not take out an admin's whole view. */
function safeParse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return [];
  }
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
