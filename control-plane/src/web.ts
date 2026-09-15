/**
 * Steam sign-in for mxbsecure.com, and the session it leaves behind.
 *
 * The same OpenID round trip the app uses (`steam.ts`), but it ends in a signed cookie on this
 * host rather than a link on an account. `/admin/assets*` reads that cookie, so a creator can
 * use the site without a key. The login state travels signed in the return URL, so there is no
 * table for pending sign-ins either.
 *
 * The state is bound to the browser that started the sign-in: its nonce also goes into a
 * short-lived cookie, and the return only counts if the two match. Without that, someone could
 * finish a sign-in as themselves, stop short of the redirect, and send the link to someone else
 * — who would then be signed in as them.
 */

import { allowedOrigin, assetOrigins, cors, refuseCrossSiteWrite } from "./assets";
import { tokenMatches } from "./auth";
import { repairBySteamId } from "./steamlink";
import { steamResult } from "./page";
import { isWebAdmin, isWebAdminPath, webAdminRoutes } from "./webadmin";
import { isVerified, loginUrl, steamPersonaName, verifyAssertion } from "./steam";
import {
  clearedCookie,
  LEGACY_SESSION_COOKIE,
  LOGIN_COOKIE,
  LOGIN_TTL_MS,
  loginCookie,
  openToken,
  readCookie,
  sealToken,
  SESSION_COOKIE,
  SESSION_TTL_MS,
  sessionCookie,
  webSession,
  type LoginState,
} from "./websession";

export function isWebPath(path: string): boolean {
  return path.startsWith("/v1/web/");
}

/** Where to land after sign-in: a path on the site, never another host. */
export function safeNext(raw: string | null): string {
  return raw && /^\/(?![/\\])[\w\-./?=&%#]*$/.test(raw) ? raw : "/dashboard";
}

function siteOrigin(env: Env): string {
  return (env.MXB_SITE_ORIGIN || "https://mxbsecure.com").replace(/\/+$/, "");
}

/** The site to land back on: the one the sign-in started from if it's ours, else the default. */
export function landingSite(raw: string | null, env: Env): string {
  return raw && assetOrigins(env).includes(raw) ? raw : siteOrigin(env);
}

export async function webRoutes(
  request: Request,
  url: URL,
  env: Env,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  const path = url.pathname;
  const method = request.method;
  const origin = allowedOrigin(request, env);
  const key = env.MXB_WEB_SESSION_KEY;

  if (method === "OPTIONS" && (path === "/v1/web/me" || path === "/v1/web/logout" || isWebAdminPath(path))) {
    if (request.headers.get("Origin") && !origin) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, OPTIONS");
  }

  // Each return asks Steam, and a login state costs nothing to mint: a ceiling per address keeps
  // this host from being a free way to hammer Steam, or to spend our request budget.
  if (method === "GET" && (path === "/v1/web/steam/login" || path === "/v1/web/steam/return") && env.SIGNIN_LIMITER) {
    const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
    if (!(await env.SIGNIN_LIMITER.limit({ key: ip })).success) {
      return steamResult(landingSite(null, env), "busy");
    }
  }

  if (method === "GET" && path === "/v1/web/steam/login") {
    if (!key) return steamResult(landingSite(null, env), "unavailable");
    const n = crypto.randomUUID();
    const state = await sealToken(
      {
        t: "state",
        site: landingSite(url.searchParams.get("site"), env),
        next: safeNext(url.searchParams.get("next")),
        n,
        exp: Date.now() + LOGIN_TTL_MS,
      },
      key,
    );
    const returnTo = `${url.origin}/v1/web/steam/return?state=${encodeURIComponent(state)}`;
    return new Response(null, {
      status: 302,
      headers: {
        Location: loginUrl(returnTo, `${url.origin}/`),
        "Set-Cookie": loginCookie(n),
        "Cache-Control": "no-store",
      },
    });
  }

  if (method === "GET" && path === "/v1/web/steam/return") {
    if (!key) return steamResult(landingSite(null, env), "unavailable");
    const state = await openToken<LoginState>(url.searchParams.get("state"), key, "state");
    if (!state) return steamResult(landingSite(null, env), "expired");
    const site = landingSite(state.site ?? null, env);
    // Checked before Steam is asked anything. A mismatch leaves the cookie alone, so a sign-in
    // this browser really has in flight still completes.
    const started = readCookie(request, LOGIN_COOKIE);
    if (!started || !tokenMatches(state.n, started)) {
      return steamResult(site, "other-browser");
    }
    const result = await verifyAssertion(url.searchParams, `${url.origin}${url.pathname}`, fetchImpl);
    if (!isVerified(result)) {
      const refused = steamResult(site, "unconfirmed");
      refused.headers.append("Set-Cookie", clearedCookie(LOGIN_COOKIE));
      return refused;
    }
    const name = await steamPersonaName(result.steamId, fetchImpl);
    const token = await sealToken({ t: "session", steamId: result.steamId, name, exp: Date.now() + SESSION_TTL_MS }, key);
    const headers = new Headers({
      Location: `${site}${state.next}`,
      "Cache-Control": "no-store",
    });
    headers.append("Set-Cookie", sessionCookie(token));
    headers.append("Set-Cookie", clearedCookie(LOGIN_COOKIE));
    headers.append("Set-Cookie", clearedCookie(LEGACY_SESSION_COOKIE));
    return new Response(null, { status: 302, headers });
  }

  if (method === "GET" && path === "/v1/web/me") {
    const session = await webSession(request, env);
    if (!session) return cors(json(401, { error: "not signed in" }), origin);
    const find = () =>
      env.DB.prepare("SELECT rider_name, kind, creator_at FROM accounts WHERE steam_id = ?")
        .bind(session.steamId)
        .first<{ rider_name: string; kind: string; creator_at: number | null }>();
    // Same retry as `/admin/assets`: a lost `steam_id` would otherwise report a linked creator
    // as neither linked nor a creator, which is the confusing half of the failure.
    const account = (await find()) ?? ((await repairBySteamId(env, session.steamId)) ? await find() : null);
    // A web-only profile (made for a creator on the site) isn't an MXB App profile.
    const app = account && account.kind !== "web" ? account : null;
    const me = cors(
      json(200, {
        steamId: session.steamId,
        name: session.name || app?.rider_name || "",
        creator: !!account?.creator_at,
        linked: !!app,
        // So the site knows whether to offer the dashboards at all. Never the gate itself —
        // every admin route checks the session again, and a client flag decides nothing.
        admin: isWebAdmin(session.steamId, env),
      }),
      origin,
    );
    // Never cached, anywhere. This is the answer to "who am I and may I sell", and it changes
    // the moment an account is made a creator — a reused copy tells someone they are not one
    // long after they are, with nothing on the page to suggest the answer is old.
    me.headers.set("Cache-Control", "no-store");
    return me;
  }

  if (method === "POST" && path === "/v1/web/logout") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    const headers = new Headers();
    headers.append("Set-Cookie", clearedCookie(SESSION_COOKIE));
    headers.append("Set-Cookie", clearedCookie(LEGACY_SESSION_COOKIE));
    return cors(new Response(null, { status: 204, headers }), origin);
  }

  if (method === "GET" && path.startsWith("/v1/web/lockweb/")) {
    return lockweb(request, url, env, origin);
  }

  // The dashboards the site draws. Gated on the Steam account rather than a key — see
  // `webadmin.ts` for why a person's admin credential should not travel in a URL.
  if (isWebAdminPath(path)) return webAdminRoutes(request, url, env, origin, fetchImpl);

  return json(404, { error: "no such endpoint" });
}

/**
 * The files the locker is. A closed list, so a name can never wander out of the bucket.
 */
const LOCKWEB_FILES: Record<string, string> = {
  "mxb_lockweb.js": "text/javascript; charset=utf-8",
  "mxb_lockweb_bg.wasm": "application/wasm",
};

/**
 * The in-browser locker, handed to affiliated creators and to nobody else.
 *
 * It cannot live on the site. mxbsecure.com is static assets, so everything it serves is
 * public — committing the locker there would publish the packer to anyone who guessed the
 * URL, gate or no gate, because the gate only decides what the page draws. It is served from
 * here because this is the host the `__Host-` session cookie is bound to: mxbsecure.com never
 * receives that cookie and so could not check a creator even if it wanted to.
 *
 * Being a creator is `creator_at`, set by hand for an affiliated creator. A Steam sign-in
 * alone gets a 403 here, exactly as it does on `/admin/assets`.
 */
async function lockweb(request: Request, url: URL, env: Env, origin: string | null): Promise<Response> {
  const name = url.pathname.slice("/v1/web/lockweb/".length);
  const type = LOCKWEB_FILES[name];
  if (!type) return cors(json(404, { error: "no such file" }), origin);

  const session = await webSession(request, env);
  if (!session) return cors(json(401, { error: "not signed in" }), origin);

  const find = () =>
    env.DB.prepare("SELECT creator_at FROM accounts WHERE steam_id = ?")
      .bind(session.steamId)
      .first<{ creator_at: number | null }>();
  const account = (await find()) ?? ((await repairBySteamId(env, session.steamId)) ? await find() : null);
  if (!account?.creator_at) {
    return cors(json(403, { error: "mxbsecure is invite only, for affiliated creators" }), origin);
  }

  const object = await env.LOCKWEB.get(name);
  // Nothing uploaded yet is a configuration problem, not a missing page: say so as 503 so it
  // reads differently from a name that was never servable.
  if (!object) return cors(json(503, { error: "the locker isn't available right now" }), origin);

  return cors(
    new Response(object.body, {
      headers: {
        "Content-Type": type,
        // The creator's own browser may keep it; no shared cache may, because this response
        // is the one thing on this host that is large, static and not public.
        "Cache-Control": "private, max-age=3600",
        "X-Content-Type-Options": "nosniff",
      },
    }),
    origin,
  );
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
