/**
 * Steam sign-in for mxbsecure.com, and the session it leaves behind.
 *
 * The same OpenID round trip the app uses (`steam.ts`), but it ends in a signed cookie on this
 * host rather than a link on an account. `/admin/assets*` reads that cookie, so a creator can
 * use the site without a key. The login state travels signed in the return URL, so there is no
 * table for pending sign-ins either.
 */

import { allowedOrigin, ASSET_ORIGINS, cors } from "./assets";
import { isVerified, loginUrl, steamPersonaName, verifyAssertion } from "./steam";
import {
  clearedSessionCookie,
  openToken,
  sealToken,
  SESSION_TTL_MS,
  sessionCookie,
  webSession,
  type LoginState,
} from "./websession";

/** A sign-in that hasn't come back from Steam in this long has to start again. */
const STATE_TTL_MS = 10 * 60 * 1000;

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
  return raw && ASSET_ORIGINS.includes(raw) ? raw : siteOrigin(env);
}

export async function webRoutes(
  request: Request,
  url: URL,
  env: Env,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  const path = url.pathname;
  const method = request.method;
  const origin = allowedOrigin(request);
  const key = env.MXB_WEB_SESSION_KEY;

  if (method === "OPTIONS" && (path === "/v1/web/me" || path === "/v1/web/logout")) {
    if (request.headers.get("Origin") && !origin) return cors(json(403, { error: "origin not allowed" }), null);
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, OPTIONS");
  }

  if (method === "GET" && path === "/v1/web/steam/login") {
    if (!key) return page(503, "Sign-in isn't set up on this server yet.");
    const state = await sealToken(
      {
        t: "state",
        site: landingSite(url.searchParams.get("site"), env),
        next: safeNext(url.searchParams.get("next")),
        n: crypto.randomUUID(),
        exp: Date.now() + STATE_TTL_MS,
      },
      key,
    );
    const returnTo = `${url.origin}/v1/web/steam/return?state=${encodeURIComponent(state)}`;
    return new Response(null, { status: 302, headers: { Location: loginUrl(returnTo, `${url.origin}/`) } });
  }

  if (method === "GET" && path === "/v1/web/steam/return") {
    if (!key) return page(503, "Sign-in isn't set up on this server yet.");
    const state = await openToken<LoginState>(url.searchParams.get("state"), key, "state");
    if (!state) return page(400, "That sign-in took too long or the link is broken. Go back and sign in again.");
    const result = await verifyAssertion(url.searchParams, `${url.origin}${url.pathname}`, fetchImpl);
    if (!isVerified(result)) return page(403, `Steam couldn't confirm that sign-in: ${result.error}.`);
    const name = await steamPersonaName(result.steamId, fetchImpl);
    const token = await sealToken({ t: "session", steamId: result.steamId, name, exp: Date.now() + SESSION_TTL_MS }, key);
    return new Response(null, {
      status: 302,
      headers: {
        Location: `${landingSite(state.site ?? null, env)}${state.next}`,
        "Set-Cookie": sessionCookie(token),
        "Cache-Control": "no-store",
      },
    });
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
    return cors(new Response(null, { status: 204, headers: { "Set-Cookie": clearedSessionCookie() } }), origin);
  }

  return json(404, { error: "no such endpoint" });
}

function page(status: number, message: string): Response {
  const body =
    `<!doctype html><meta charset="utf-8"><title>mxbsecure</title>` +
    `<body style="font:16px/1.5 system-ui;margin:4rem auto;max-width:30rem;padding:0 1rem">` +
    `<p>${message.replace(/[<&]/g, (c) => (c === "<" ? "&lt;" : "&amp;"))}</p>`;
  return new Response(body, { status, headers: { "content-type": "text/html; charset=utf-8" } });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
