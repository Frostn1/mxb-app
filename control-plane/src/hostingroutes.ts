/**
 * The HTTP side of user server deploy (`hosting.ts`):
 *
 * - `/v1/web/hosting/*`        servers.mxbsecure.com, the signed-in Steam account's own servers
 * - `/v1/web/admin/hosting*`   servers.mxbsecure.com, operators (the admin Steam list)
 * - `/v1/hosted/*`             MSM, with a bearer claimed through a one-time link, or the
 *                              per-user token from signing in with Steam (`hostedauth.ts`)
 * - `/v1/hosting/*`            the box install runner, with `MXB_BOX_ENROLL_KEY`
 */

import { cors, refuseCrossSiteWrite } from "./assets";
import { billingWebRoute, withOperatorBilling } from "./billing";
import {
  addTrack,
  boxStage,
  claimInvite,
  defaultDeps,
  deploy,
  deleteServer,
  enrollBox,
  getServer,
  hostedOwner,
  mintInvite,
  msmClaim,
  msmLink,
  myHosting,
  ownedServers,
  ownsServer,
  operatorBox,
  operatorView,
  rememberHostName,
  ownerDelete,
  restartServer,
  runnerAuthorized,
  runnerTracks,
  updateSettings,
  type Deps,
  type Result,
} from "./hosting";
import { msmLogin, msmReturn, msmRevoke, msmStart, msmToken, touchUserToken, userTokenOwner } from "./hostedauth";
import { isWebAdmin } from "./webadmin";
import { webSession } from "./websession";

const ID = "([0-9a-f-]{36})";

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", "Cache-Control": "no-store" },
  });
}

async function body(request: Request): Promise<Record<string, unknown>> {
  try {
    const text = await request.text();
    if (text.length > 64 * 1024) return {};
    const parsed = text ? JSON.parse(text) : {};
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

export function isHostingWebPath(path: string): boolean {
  return path.startsWith("/v1/web/hosting/") || path === "/v1/web/admin/hosting" || path.startsWith("/v1/web/admin/hosting/");
}

/** The site's routes, user and operator. */
export async function hostingWebRoutes(
  request: Request,
  url: URL,
  env: Env,
  origin: string | null,
  deps: Deps = defaultDeps(env),
): Promise<Response> {
  const path = url.pathname;
  const method = request.method;
  if (method === "OPTIONS") {
    if (request.headers.get("Origin") && !origin) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, PUT, DELETE, OPTIONS", "Content-Type");
  }
  const said = (r: Result) => cors(json(r.status, r.body), origin);
  const refused = refuseCrossSiteWrite(request, env);
  if (refused) return cors(refused, origin);
  const session = await webSession(request, env);
  if (!session) return said({ status: 401, body: { error: "not signed in" } });
  const steamId = session.steamId;
  const operator = isWebAdmin(steamId, env);

  if (path.startsWith("/v1/web/admin/hosting")) {
    if (!operator) return said({ status: 403, body: { error: "not an admin" } });
    return said(await operatorRoute(request, path, method, env, deps, steamId));
  }

  const billed = await billingWebRoute(env, deps, steamId, method, path);
  if (billed) return said(billed);
  if (method === "GET" && path === "/v1/web/hosting/me") {
    await rememberHostName(env, steamId, session.name);
    return said(await myHosting(env, steamId, operator));
  }
  if (method === "POST" && path === "/v1/web/hosting/claim") {
    const claimed = await claimInvite(env, deps, steamId, await body(request));
    await rememberHostName(env, steamId, session.name);
    return said(claimed);
  }
  if (method === "POST" && path === "/v1/web/hosting/servers") return said(await deploy(env, deps, steamId, await body(request)));

  let m = path.match(new RegExp(`^/v1/web/hosting/servers/${ID}$`, "i"));
  if (m && method === "GET") return said(await getServer(env, steamId, m[1]));
  if (m && method === "DELETE") return said(await ownerDelete(env, deps, steamId, m[1]));
  m = path.match(new RegExp(`^/v1/web/hosting/servers/${ID}/(settings|restart|msm-link)$`, "i"));
  if (m) {
    if (m[2] === "settings" && method === "PUT") return said(await updateSettings(env, deps, steamId, m[1], await body(request)));
    if (m[2] === "restart" && method === "POST") return said(await restartServer(env, deps, steamId, m[1]));
    if (m[2] === "msm-link" && method === "POST") return said(await msmLink(env, deps, steamId, m[1]));
  }
  return said({ status: 404, body: { error: "no such endpoint" } });
}

async function operatorRoute(
  request: Request,
  path: string,
  method: string,
  env: Env,
  deps: Deps,
  steamId: string,
): Promise<Result> {
  if (method === "GET" && path === "/v1/web/admin/hosting") return withOperatorBilling(env, await operatorView(env, deps.fetch));
  if (method === "POST" && path === "/v1/web/admin/hosting/invites") {
    const result = await mintInvite(env, deps, steamId, await body(request));
    console.log(JSON.stringify({ msg: "hosting invite minted", admin: steamId, status: result.status }));
    return result;
  }
  if (method === "POST" && path === "/v1/web/admin/hosting/tracks") return addTrack(env, deps, await body(request));
  let m = path.match(new RegExp(`^/v1/web/admin/hosting/invites/${ID}$`, "i"));
  if (m && method === "DELETE") {
    await env.DB.prepare("UPDATE host_invites SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL").bind(deps.now(), m[1]).run();
    return { status: 200, body: { ok: true } };
  }
  m = path.match(/^\/v1\/web\/admin\/hosting\/tracks\/([a-z0-9-]{1,64})$/);
  if (m && method === "DELETE") {
    await env.DB.prepare("DELETE FROM host_tracks WHERE id = ?").bind(m[1]).run();
    return { status: 200, body: { ok: true } };
  }
  m = path.match(new RegExp(`^/v1/web/admin/hosting/boxes/${ID}/(drain|undrain|cancelled|retry)$`, "i"));
  if (m && method === "POST") return operatorBox(env, deps, m[1], m[2]);
  m = path.match(new RegExp(`^/v1/web/admin/hosting/alerts/${ID}/ack$`, "i"));
  if (m && method === "POST") {
    await env.DB.prepare("UPDATE host_alerts SET acked_at = ? WHERE id = ?").bind(deps.now(), m[1]).run();
    return { status: 200, body: { ok: true } };
  }
  m = path.match(new RegExp(`^/v1/web/admin/hosting/servers/${ID}/delete$`, "i"));
  if (m && method === "POST") return deleteServer(env, deps, m[1], `operator ${steamId}`);
  return { status: 404, body: { error: "no such endpoint" } };
}

export function isHostedPath(path: string): boolean {
  return path.startsWith("/v1/hosted/") || path.startsWith("/v1/hosting/");
}

/** MSM's routes and the install runner's. Neither uses a cookie, so neither needs CORS. */
export async function hostedRoutes(request: Request, url: URL, env: Env, deps: Deps = defaultDeps(env)): Promise<Response> {
  const path = url.pathname;
  const method = request.method;
  const out = (r: Result) => json(r.status, r.body);

  if (path.startsWith("/v1/hosting/")) {
    if (!runnerAuthorized(env, request)) return json(403, { error: "not the install runner" });
    if (method === "GET" && path === "/v1/hosting/tracks") return out(await runnerTracks(env, url));
    const m = path.match(new RegExp(`^/v1/hosting/boxes/${ID}/(stage|enroll)$`, "i"));
    if (m && method === "POST") {
      const input = await body(request);
      return out(m[2] === "stage" ? await boxStage(env, deps, m[1], input) : await enrollBox(env, deps, m[1], input));
    }
    return json(404, { error: "no such endpoint" });
  }

  if (method === "POST" && path === "/v1/hosted/claim") return out(await msmClaim(env, deps, await body(request)));

  // Sign in with Steam (`hostedauth.ts`). Steam is asked from the Worker, so the fetch stays
  // unbound: a stored `fetch` called as a method throws "Illegal invocation".
  const steamFetch: typeof fetch = (input, init) => fetch(input, init);
  if (path.startsWith("/v1/hosted/auth/")) {
    if (path !== "/v1/hosted/auth/revoke" && env.SIGNIN_LIMITER) {
      const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
      if (!(await env.SIGNIN_LIMITER.limit({ key: ip })).success) return json(429, { error: "Too many sign-ins. Try again in a minute." });
    }
    const now = deps.now();
    if (method === "POST" && path === "/v1/hosted/auth/login") return out(await msmLogin(env, url.origin, await body(request), now));
    if (method === "GET" && path === "/v1/hosted/auth/start") return msmStart(env, url, now);
    if (method === "GET" && path === "/v1/hosted/auth/return") return msmReturn(env, url, now, steamFetch);
    if (method === "POST" && path === "/v1/hosted/auth/token") return out(await msmToken(env, await body(request), now, steamFetch));
    if (method === "POST" && path === "/v1/hosted/auth/revoke") return out(await msmRevoke(env, request, now));
    return json(404, { error: "no such endpoint" });
  }
  if (path === "/v1/hosted/me/servers") {
    if (method !== "GET") return json(404, { error: "no such endpoint" });
    const steamId = await userTokenOwner(env, request);
    if (!steamId) return json(401, { error: "Signed out. Sign in again." });
    await touchUserToken(env, request, deps.now());
    return out(await ownedServers(env, steamId));
  }

  const m = path.match(new RegExp(`^/v1/hosted/servers/${ID}(?:/(settings|restart))?$`, "i"));
  if (!m) return json(404, { error: "no such endpoint" });
  // A claimed per-server bearer, else the signed-in account's token, which reaches only the
  // servers that account owns (checked here and again by each owner action).
  let owner = await hostedOwner(env, request, m[1]);
  if (!owner) {
    const user = await userTokenOwner(env, request);
    if (user && !(await ownsServer(env, user, m[1]))) return json(404, { error: "No such server." });
    owner = user;
  }
  if (!owner) return json(401, { error: "This server is no longer linked. Open it again from servers.mxbsecure.com." });
  if (!m[2] && method === "GET") return out(await getServer(env, owner, m[1]));
  if (m[2] === "settings" && method === "PUT") return out(await updateSettings(env, deps, owner, m[1], await body(request)));
  if (m[2] === "restart" && method === "POST") return out(await restartServer(env, deps, owner, m[1]));
  return json(404, { error: "no such endpoint" });
}
