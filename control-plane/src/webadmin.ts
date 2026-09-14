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

import { cors, refuseCrossSiteWrite } from "./assets";
import { addRule, collectAdminView, deleteRule } from "./diagnostics";
import {
  clampDays,
  fileDetail,
  parseFileQuery,
  parsePage,
  parseRiderQuery,
  parseSightingQuery,
  riderDetail,
  searchFiles,
  searchRiders,
  totals,
} from "./diagnosticssearch";
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

  const said = (status: number, body: unknown) => {
    const res = cors(json(status, body), origin);
    // Numbers about people: never held by anything in between, as on the rendered pages.
    res.headers.set("Cache-Control", "no-store");
    return res;
  };

  const path = url.pathname;
  if (request.method === "GET") {
    switch (path) {
      case "/v1/web/admin/usage":
        return said(200, await collectStats(env, windowDays(url)));

      // The overview carries the rules as well: they are four rows in the same read, and a
      // second endpoint for them would be a second round trip for a tab switch.
      case "/v1/web/admin/diagnostics": {
        const days = clampDays(url.searchParams.get("days"));
        const [view, counts] = await Promise.all([collectAdminView(env), totals(env, days)]);
        return said(200, { days, totals: counts, ...view });
      }
      case "/v1/web/admin/diagnostics/riders": {
        const query = parseRiderQuery(url);
        return said(200, { query, ...(await searchRiders(env, query)) });
      }
      case "/v1/web/admin/diagnostics/rider": {
        const who = url.searchParams.get("who") ?? "";
        const query = parseSightingQuery(url);
        const detail = who ? await riderDetail(env, who, query) : null;
        return detail ? said(200, { query, ...detail }) : said(404, { error: "no such rider" });
      }
      case "/v1/web/admin/diagnostics/files": {
        const query = parseFileQuery(url);
        return said(200, { query, ...(await searchFiles(env, query)) });
      }
      case "/v1/web/admin/diagnostics/file": {
        const name = url.searchParams.get("name") ?? "";
        const sha256 = url.searchParams.get("sha256") ?? "";
        const detail = name ? await fileDetail(env, name, sha256, parsePage(url.searchParams.get("page"))) : null;
        return detail ? said(200, detail) : said(404, { error: "no such file" });
      }
    }
  }

  // The one write here. A rule takes effect on the next report from every install, so it is
  // held to the same cross-site check as the rest of the site's writes.
  if (request.method === "POST" && path === "/v1/web/admin/diagnostics/rules") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    let body: Record<string, unknown>;
    try {
      body = (await request.json()) as Record<string, unknown>;
    } catch {
      return said(400, { error: "that was not JSON" });
    }
    const field = (name: string) => String(body[name] ?? "");
    if (field("action") === "delete") {
      const id = Number(body.id);
      if (!Number.isInteger(id) || id <= 0) return said(400, { error: "that is not a rule id" });
      await deleteRule(env, id);
      return said(200, { ok: true });
    }
    const result = await addRule(env, field("kind"), field("pattern"), field("sha256"), field("label"), field("note"));
    return result.ok ? said(200, { ok: true }) : said(400, { error: result.error ?? "that rule was not usable" });
  }

  return said(404, { error: "no such endpoint" });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
