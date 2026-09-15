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
import { filesData, onePaintData, oneRiderData, ridersData } from "./paintspage";
import {
  adminPlugins,
  grantLicense,
  mintKeys,
  searchKeys,
  searchLicenses,
  setKeyRevoked,
  setLicenseRevoked,
} from "./plugins";
import { batchCodes, keyQuery, licenseQuery } from "./pluginspage";
import { paintThumb } from "./pntthumb";
import { isSteamId64 } from "./steam";
import { collectStats, windowApp, windowDays } from "./usage";
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
 * Every `/v1/web/admin/*` route the site's dashboards read and write.
 *
 * Usage is the same `collectStats` that `/v1/usage/stats` returns, so a script and the page
 * can never disagree about what a figure means.
 */
export async function webAdminRoutes(request: Request, url: URL, env: Env, origin: string | null): Promise<Response> {
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
    switch (path) {
      case "/v1/web/admin/usage":
        return said(200, await collectStats(env, windowDays(url), Date.now(), windowApp(url)));

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

      case "/v1/web/admin/plugins/keys": {
        const query = keyQuery(url);
        // A batch is identified by the second it was minted in, so the codes survive a
        // reload and a link without ever being in the address themselves.
        const minted = Number(url.searchParams.get("minted") ?? "");
        const [plugins, found, batch] = await Promise.all([
          adminPlugins(env),
          searchKeys(env, query),
          Number.isInteger(minted) && minted > 0 ? batchCodes(env, minted) : Promise.resolve([]),
        ]);
        return said(200, { plugins, found, query, batch });
      }
      case "/v1/web/admin/plugins/licenses": {
        const query = licenseQuery(url);
        const [plugins, found] = await Promise.all([adminPlugins(env), searchLicenses(env, query)]);
        return said(200, { plugins, found, query });
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

  // Everything a button on the plugin pages does. One endpoint rather than six, because they
  // are all the same shape: a change, and what to say about it afterwards.
  if (request.method === "POST" && path === "/v1/web/admin/plugins") {
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
      case "mint": {
        const result = await mintKeys(env, field("plugin"), Number(body.months), Number(body.count), field("note"));
        return result.ok ? said(200, { ok: true, at: result.at }) : said(400, { error: result.error ?? "those keys were not minted" });
      }
      case "key-revoke":
        await setKeyRevoked(env, field("code"), true);
        return said(200, { ok: true });
      case "key-restore":
        await setKeyRevoked(env, field("code"), false);
        return said(200, { ok: true });
      case "license-revoke":
        await setLicenseRevoked(env, field("account"), field("plugin"), true);
        return said(200, { ok: true });
      case "license-restore":
        await setLicenseRevoked(env, field("account"), field("plugin"), false);
        return said(200, { ok: true });
      case "grant": {
        const result = await grantLicense(env, field("who"), field("plugin"), Number(body.months));
        return result.ok
          ? said(200, { ok: true, account: result.account, expires: result.expires })
          : said(400, { error: result.error ?? "nothing was granted" });
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
