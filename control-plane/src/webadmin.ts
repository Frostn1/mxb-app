/**
 * The admin dashboards, read from mxbsecure.com by the person who runs the deployment.
 *
 * `ADMIN_KEY` is the credential for a script, and the wrong one for a person: a key in a
 * dashboard URL ends up in a browser history, a screenshot and a support thread. So the
 * dashboards live only on the site, and gate on the Steam sign-in instead. `MXB_ADMIN_STEAM_IDS` names the
 * accounts; the session cookie proves one. A list in config rather than a column on `accounts`
 * is deliberate — this is the credential that reads everybody's numbers, so granting it should
 * be a deploy that leaves a diff, not an UPDATE that leaves none.
 *
 * Unset means nobody is an admin. A deployment that was never given the list has no admin
 * surface here, which is the same default `ADMIN_KEY` takes.
 */

import { cors, refuseCrossSiteWrite } from "./assets";
import { addBan, liftBan, listBans } from "./bans";
import { addCreator, listCreators, removeCreator } from "./creators";
import { addRule, collectAdminView, deleteRule } from "./diagnostics";
import { crashDetail, crashSites, recentCrashes } from "./crashes";
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
import { filesData, onePaintData, oneRiderData, ridersData } from "./paintspage";
import { paintThumb } from "./pntthumb";
import { adminRiderLookup, issueRatingToken, leaderboard as ratingLeaderboard } from "./rating";
import { adminListSeries, adminSeriesAction } from "./series";
import { isSteamId64 } from "./steam";
import { steamAdoption } from "./steamstats";
import {
  clearNote,
  collectSurvey,
  deletePoll,
  parsePoll,
  savePoll,
  setPollLive,
  windowDays as surveyDays,
  windowPoll,
} from "./survey";
import { collectStats, windowApp, windowDays } from "./usage";
import { collectMisses, missWindow } from "./searchmisses";
import { webSession } from "./websession";
import {
  action as serverAction,
  connect as connectServer,
  detail as serverDetail,
  inventory as serverInventory,
  logs as serverLogs,
} from "./servermanager";

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
 * Every `/v1/web/admin/*` route the site's dashboards read and write.
 *
 * Usage is the same `collectStats` that `/v1/usage/stats` returns, so a script and the page
 * can never disagree about what a figure means. The page additionally gets `steam`, which is a
 * whole extra block rather than a changed figure — nothing shared has two definitions.
 */
export async function webAdminRoutes(
  request: Request,
  url: URL,
  env: Env,
  origin: string | null,
  fetchImpl: typeof fetch = fetch,
): Promise<Response> {
  const session = await webSession(request, env);
  if (!session) return cors(json(401, { error: "not signed in" }), origin);
  if (!isWebAdmin(session.steamId, env)) return cors(json(403, { error: "not an admin" }), origin);

  const said = (status: number, body: unknown) => {
    const res = cors(json(status, body), origin);
    // Numbers about people: never held by anything in between.
    res.headers.set("Cache-Control", "no-store");
    return res;
  };

  const path = url.pathname;
  if (request.method === "GET") {
    if (path === "/v1/web/admin/servers") {
      const result = await serverInventory(env);
      return said(result.status, result.body);
    }
    const detail = path.match(/^\/v1\/web\/admin\/servers\/([0-9a-f-]{36})$/i);
    if (detail) {
      const result = await serverDetail(env, detail[1], fetchImpl);
      return said(result.status, result.body);
    }
    const logs = path.match(/^\/v1\/web\/admin\/servers\/([0-9a-f-]{36})\/logs$/i);
    if (logs) {
      const result = await serverLogs(env, logs[1], fetchImpl);
      return said(result.status, result.body);
    }
    switch (path) {
      // The Steam split rides along rather than living behind its own route: it is one query,
      // it is read on the same tab, and a second round trip would let the page draw a version
      // panel from one window and a sign-in panel from another.
      case "/v1/web/admin/usage": {
        const days = windowDays(url);
        const now = Date.now();
        const [stats, steam] = await Promise.all([
          collectStats(env, days, now, windowApp(url)),
          steamAdoption(env, days, now),
        ]);
        return said(200, { ...stats, steam });
      }

      // What people said when they were asked. The poll definitions ride along with the
      // figures because the page cannot label a choice id without them, and a second round
      // trip for the labels would mean the two could disagree about which polls exist.
      case "/v1/web/admin/survey":
        return said(
          200,
          await collectSurvey(env, surveyDays(url), Date.now(), windowApp(url), windowPoll(url)),
        );

      // Browse searches in the MXB App that found nothing, for /insights on the site.
      case "/v1/web/admin/search-misses":
        return said(200, await collectMisses(env, missWindow(url), Date.now()));

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
      // Where the game died, for everyone it died on. Three reads because they are three
      // questions: which crash matters (by how many riders hit it), what is happening right
      // now, and what one crash looks like across every report of it.
      case "/v1/web/admin/crashes":
        return said(200, { sites: await crashSites(env.DB), recent: await recentCrashes(env.DB) });
      case "/v1/web/admin/crashes/site": {
        const detail = await crashDetail(env.DB, url.searchParams.get("site") ?? "");
        return detail ? said(200, detail) : said(404, { error: "no such crash site" });
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

      case "/v1/web/admin/paints/riders":
        return said(200, await ridersData(env, url));
      case "/v1/web/admin/paints/files":
        return said(200, await filesData(env, url));
      case "/v1/web/admin/paints/rider": {
        const d = await oneRiderData(env, url.searchParams.get("id") ?? "");
        return d ? said(200, d) : said(404, { error: "no such account" });
      }
      case "/v1/web/admin/paints/paint": {
        const d = await onePaintData(env, (url.searchParams.get("sha") ?? "").toLowerCase());
        // The sheets are pixels; the page asks for those separately, from /thumb.
        return d ? said(200, { ...d, sheets: d.sheets ? { chosen: d.sheets.chosen, count: d.sheets.images.length } : null })
          : said(404, { error: "nothing here has that digest" });
      }
      // The picture itself. `cors` so the page can ask for it with the sign-in cookie —
      // it is somebody's paint, not a public asset.
      case "/v1/web/admin/paints/thumb":
        return cors(await paintThumb((url.searchParams.get("sha") ?? "").toLowerCase(), env), origin);

      case "/v1/web/admin/creators":
        return said(200, { creators: await listCreators(env, fetchImpl) });

      case "/v1/web/admin/bans":
        return said(200, { bans: await listBans(env) });

      // Public series (`series.ts`): which slugs are reserved, published, and waiting on review.
      case "/v1/web/admin/series": {
        const result = await adminListSeries(env);
        return said(result.status, result.body);
      }

      // Admin sees everything the public leaderboard hides: banned riders included, so a
      // flagged/banned rider's numbers stay reviewable instead of just disappearing.
      case "/v1/web/admin/rating/leaderboard": {
        const result = await ratingLeaderboard(
          env,
          url.searchParams.get("class") ?? "",
          Number(url.searchParams.get("limit") ?? "50"),
          true,
        );
        return said(result.status, result.body);
      }
    }

    // The raw GUID row: every class, RD/volatility, ban status. Never exposed off the admin
    // surface.
    const riderLookup = path.match(/^\/v1\/web\/admin\/rating\/riders\/([A-Za-z0-9._:-]{4,100})$/);
    if (riderLookup) {
      const result = await adminRiderLookup(env, decodeURIComponent(riderLookup[1]));
      return said(result.status, result.body);
    }
  }

  if (request.method === "POST" && path.startsWith("/v1/web/admin/servers")) {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    let body: Record<string, unknown>;
    try {
      body = (await request.json()) as Record<string, unknown>;
    } catch {
      return said(400, { error: "that was not JSON" });
    }
    if (path === "/v1/web/admin/servers/connect") {
      const result = await connectServer(env, body, fetchImpl);
      console.log(JSON.stringify({ msg: "managed server connect", admin: session.steamId, status: result.status, server: (result.body as { id?: unknown }).id ?? null }));
      return said(result.status, result.body);
    }
    const action = path.match(/^\/v1\/web\/admin\/servers\/([0-9a-f-]{36})\/action$/i);
    if (action) {
      const result = await serverAction(env, action[1], body, fetchImpl);
      console.log(JSON.stringify({ msg: "managed server action", server: action[1], action: String(body.action ?? ""), admin: session.steamId, status: result.status }));
      return said(result.status, result.body);
    }
    // Issues/rotates the per-server *rating* bearer token — separate from the admin token
    // above, and shown exactly once, same rule as an account token (`auth.ts`).
    const ratingToken = path.match(/^\/v1\/web\/admin\/servers\/([0-9a-f-]{36})\/rating-token$/i);
    if (ratingToken) {
      const result = await issueRatingToken(env, ratingToken[1]);
      console.log(JSON.stringify({ msg: "managed server rating token issued", server: ratingToken[1], admin: session.steamId, status: result.status }));
      return said(result.status, result.body);
    }
    return said(404, { error: "no such server-manager endpoint" });
  }

  // Reserving a series slug mints its publish token, shown once here and pasted into MSM.
  if (request.method === "POST" && path === "/v1/web/admin/series") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    let body: Record<string, unknown>;
    try {
      body = (await request.json()) as Record<string, unknown>;
    } catch {
      return said(400, { error: "that was not JSON" });
    }
    const result = await adminSeriesAction(env, body);
    console.log(JSON.stringify({ msg: "series admin", action: String(body.action ?? ""), slug: String(body.slug ?? ""), admin: session.steamId, status: result.status }));
    return said(result.status, result.body);
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

  // Writing the questions. The one admin write whose effect is felt by every install rather
  // than by one account: a poll saved here is on screen in the field within a flush interval,
  // so it is held to the same cross-site check as the rest and validated as strictly as the
  // form can manage — a badly-written question wastes every answer it collects.
  if (request.method === "POST" && path === "/v1/web/admin/survey") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    let body: Record<string, unknown>;
    try {
      body = (await request.json()) as Record<string, unknown>;
    } catch {
      return said(400, { error: "that was not JSON" });
    }
    const field = (name: string) => String(body[name] ?? "");
    switch (field("action")) {
      case "save": {
        const poll = parsePoll(body.poll);
        if (typeof poll === "string") return said(400, { error: poll });
        await savePoll(env, poll);
        return said(200, { ok: true, id: poll.id });
      }
      case "retire":
      case "revive": {
        const live = field("action") === "revive";
        return (await setPollLive(env, field("id"), live))
          ? said(200, { ok: true })
          : said(404, { error: "no such poll" });
      }
      case "delete": {
        const done = await deletePoll(env, field("id"));
        if (done === "ok") return said(200, { ok: true });
        return done === "answered"
          ? said(409, { error: "people have answered that one — retire it instead" })
          : said(404, { error: "no such poll" });
      }
      // The button that makes offering a free-text box defensible at all.
      case "note-clear":
        return (await clearNote(env, body.handle))
          ? said(200, { ok: true })
          : said(404, { error: "no such note" });
      default:
        return said(400, { error: "no such action" });
    }
  }

  // Who may lock and sell. One endpoint, an action each.
  if (request.method === "POST" && path === "/v1/web/admin/creators") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    let body: Record<string, unknown>;
    try {
      body = (await request.json()) as Record<string, unknown>;
    } catch {
      return said(400, { error: "that was not JSON" });
    }
    const field = (name: string) => String(body[name] ?? "");
    switch (field("action")) {
      case "add": {
        const result = await addCreator(env, field("who"), fetchImpl);
        return result.ok ? said(200, result) : said(400, { error: result.error });
      }
      case "remove":
        return (await removeCreator(env, field("account")))
          ? said(200, { ok: true })
          : said(404, { error: "that account isn't a creator" });
      default:
        return said(400, { error: "no such action" });
    }
  }

  // Who is shut out of mxbsecure. The heaviest button on the site: it refuses content somebody
  // paid for, on every machine they own, so both halves record the admin who pressed it
  // (`session.steamId`) and neither is a DELETE.
  if (request.method === "POST" && path === "/v1/web/admin/bans") {
    const refused = refuseCrossSiteWrite(request, env);
    if (refused) return cors(refused, origin);
    let body: Record<string, unknown>;
    try {
      body = (await request.json()) as Record<string, unknown>;
    } catch {
      return said(400, { error: "that was not JSON" });
    }
    const field = (name: string) => String(body[name] ?? "");
    switch (field("action")) {
      case "ban": {
        const result = await addBan(
          env,
          { guid: body.guid, reason: body.reason, evidence: body.evidence, altOf: body.altOf },
          session.steamId,
        );
        return result.ok ? said(200, result) : said(400, { error: result.error });
      }
      case "lift": {
        const result = await liftBan(env, body.guid, session.steamId, body.note);
        return result.ok ? said(200, result) : said(404, { error: result.error });
      }
      default:
        return said(400, { error: "no such action" });
    }
  }

  return said(404, { error: "no such endpoint" });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
