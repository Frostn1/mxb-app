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
import { page } from "./page";
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

  if (method === "OPTIONS" && (path === "/v1/web/me" || path === "/v1/web/logout")) {
    if (request.headers.get("Origin") && !origin) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, OPTIONS");
  }

  // Each return asks Steam, and a login state costs nothing to mint: a ceiling per address keeps
  // this host from being a free way to hammer Steam, or to spend our request budget.
  if (method === "GET" && (path === "/v1/web/steam/login" || path === "/v1/web/steam/return") && env.SIGNIN_LIMITER) {
    const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
    if (!(await env.SIGNIN_LIMITER.limit({ key: ip })).success) {
      const slow = page(429, "Too many sign-in attempts from here. Wait a minute and try again.");
      slow.headers.set("Retry-After", "60");
      return slow;
    }
  }

  if (method === "GET" && path === "/v1/web/steam/login") {
    if (!key) return page(503, "Sign-in isn't set up on this server yet.");
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
    if (!key) return page(503, "Sign-in isn't set up on this server yet.");
    const state = await openToken<LoginState>(url.searchParams.get("state"), key, "state");
    if (!state) return page(400, "That sign-in took too long or the link is broken. Go back and sign in again.");
    // Checked before Steam is asked anything. A mismatch leaves the cookie alone, so a sign-in
    // this browser really has in flight still completes.
    const started = readCookie(request, LOGIN_COOKIE);
    if (!started || !tokenMatches(state.n, started)) {
      return page(403, "That sign-in didn't start in this browser. Go back to mxbsecure and sign in again.");
    }
    const result = await verifyAssertion(url.searchParams, `${url.origin}${url.pathname}`, fetchImpl);
    if (!isVerified(result)) {
      const refused = page(403, `Steam couldn't confirm that sign-in: ${result.error}.`);
      refused.headers.append("Set-Cookie", clearedCookie(LOGIN_COOKIE));
      return refused;
    }
    const name = await steamPersonaName(result.steamId, fetchImpl);
    const token = await sealToken({ t: "session", steamId: result.steamId, name, exp: Date.now() + SESSION_TTL_MS }, key);
    const headers = new Headers({
      Location: `${landingSite(state.site ?? null, env)}${state.next}`,
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
    const account = await env.DB.prepare("SELECT rider_name, creator_at FROM accounts WHERE steam_id = ?")
      .bind(session.steamId)
      .first<{ rider_name: string; creator_at: number | null }>();
    return cors(
      json(200, {
        steamId: session.steamId,
        name: session.name || account?.rider_name || "",
        creator: !!account?.creator_at,
        linked: !!account,
      }),
      origin,
    );
  }

  if (method === "POST" && path === "/v1/web/logout") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    const headers = new Headers();
    headers.append("Set-Cookie", clearedCookie(SESSION_COOKIE));
    headers.append("Set-Cookie", clearedCookie(LEGACY_SESSION_COOKIE));
    return cors(new Response(null, { status: 204, headers }), origin);
  }

  return json(404, { error: "no such endpoint" });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
