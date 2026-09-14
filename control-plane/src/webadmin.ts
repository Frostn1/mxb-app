/**
 * The admin dashboards, read from mxbsecure.com by the person who runs the deployment.
 *
 * `ADMIN_KEY` opens every `/admin` page on this host and will keep doing so — it is the right
 * credential for a script. It is the wrong one for a person: `href` in `adminui.ts` puts it in
 * every link on the page, which means it ends up in a browser history, a screenshot and a
 * support thread, and nothing about a URL says which of those it has already reached.
 *
 * The site's dashboards gate on the Steam sign-in instead. `MXB_ADMIN_STEAM_IDS` names the
 * accounts; the session cookie proves one. A list in config rather than a column on `accounts`
 * is deliberate — this is the credential that reads everybody's numbers, so granting it should
 * be a deploy that leaves a diff, not an UPDATE that leaves none.
 *
 * Unset means nobody is an admin. A deployment that was never given the list has no admin
 * surface here, which is the same default `ADMIN_KEY` takes.
 */

import { cors } from "./assets";
import { isSteamId64 } from "./steam";
import { collectStats, windowDays } from "./usage";
import { webSession } from "./websession";

export function isWebAdminPath(path: string): boolean {
  return path.startsWith("/v1/web/admin/");
}

/** The Steam accounts that may read the dashboards. Anything that isn't a SteamID64 is dropped. */
export function adminSteamIds(env: Env): string[] {
  return (env.MXB_ADMIN_STEAM_IDS ?? "").split(/[,\s]+/).filter(isSteamId64);
}

export function isWebAdmin(steamId: string, env: Env): boolean {
  return adminSteamIds(env).includes(steamId);
}

/**
 * `GET /v1/web/admin/usage?days=N` — the numbers the `/admin/usage` page draws, as JSON.
 *
 * The same `collectStats` the server-rendered page calls, so the two can never disagree about
 * what a figure means: this route is a second door onto one query, not a second query.
 */
export async function webAdminRoutes(request: Request, url: URL, env: Env, origin: string | null): Promise<Response> {
  const session = await webSession(request, env);
  if (!session) return cors(json(401, { error: "not signed in" }), origin);
  if (!isWebAdmin(session.steamId, env)) return cors(json(403, { error: "not an admin" }), origin);

  if (request.method === "GET" && url.pathname === "/v1/web/admin/usage") {
    const res = cors(json(200, await collectStats(env, windowDays(url))), origin);
    // Numbers about people: never held by anything in between, as on the rendered page.
    res.headers.set("Cache-Control", "no-store");
    return res;
  }
  return cors(json(404, { error: "no such endpoint" }), origin);
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
