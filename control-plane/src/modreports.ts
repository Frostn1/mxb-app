/**
 * Reports and moderation for the mod catalogue.
 *
 *   POST /v1/assets/<id>/report          anyone: {reason, details}. Capped per address a day.
 *   GET  /v1/web/mods/queue              admins: reported mods, most-reported first, plus the
 *                                        newest uploads (the queue a moderator reads)
 *   POST /v1/web/mods/<id>/moderate      admins: {action: hide | unhide | remove | dismiss, note}
 *
 * The admin half is gated on the Steam sign-in of the site (`ADMIN_STEAM_IDS`), like the
 * racing and servers consoles, and lives under its own `/v1/web/mods/` prefix rather than
 * `/v1/web/admin/` so the moderation page can live on its own host (mods.mxbsecure.com)
 * instead of the dashboard that is being retired.
 *
 * `hide` takes a mod out of search and downloads and can be undone. `remove` also deletes the
 * files nothing else uses and cannot. Either resolves the mod's open reports.
 */

import { cors, refuseCrossSiteWrite } from "./assets";
import { freeBlobs } from "./uploads";
import { ipDigest } from "./voice";
import { webSession } from "./websession";
import { isWebAdmin } from "./webadmin";

export const REPORT_REASONS = ["broken", "stolen", "malware", "offensive", "other"] as const;
const REPORTS_PER_ADDRESS_PER_DAY = 20;

type Result = { status: number; body: unknown };

export async function reportAsset(request: Request, assetId: number, env: Env, now = Date.now()): Promise<Result> {
  const b = ((await request.json().catch(() => null)) ?? {}) as Record<string, unknown>;
  const reason = typeof b.reason === "string" && (REPORT_REASONS as readonly string[]).includes(b.reason) ? b.reason : null;
  if (!reason) return { status: 400, body: { error: `reason must be one of ${REPORT_REASONS.join(", ")}` } };
  const details = typeof b.details === "string" ? b.details.slice(0, 1000) : null;
  const asset = await env.DB.prepare("SELECT id FROM mod_assets WHERE id = ? AND state IN ('active', 'hidden')")
    .bind(assetId)
    .first();
  if (!asset) return { status: 404, body: { error: "no such mod" } };
  const day = new Date(now).toISOString().slice(0, 10);
  const who = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);
  const today = await env.DB.prepare("SELECT COUNT(*) AS n FROM mod_reports WHERE reporter_ip = ? AND created_at > ?")
    .bind(who, now - 24 * 3600_000)
    .first<{ n: number }>();
  if ((today?.n ?? 0) >= REPORTS_PER_ADDRESS_PER_DAY) return { status: 429, body: { error: "too many reports today" } };
  await env.DB.batch([
    env.DB.prepare("INSERT INTO mod_reports (asset_id, reporter_ip, reason, details, created_at) VALUES (?, ?, ?, ?, ?)").bind(
      assetId,
      who,
      reason,
      details,
      now,
    ),
    env.DB.prepare("UPDATE mod_assets SET reports_open = reports_open + 1 WHERE id = ?").bind(assetId),
  ]);
  return { status: 201, body: { ok: true } };
}

export function isWebModsPath(path: string): boolean {
  return path.startsWith("/v1/web/mods/");
}

export async function moderationQueue(env: Env): Promise<Result> {
  const reported = await env.DB.prepare(
    `SELECT a.id, a.source, a.title, a.type, a.state, a.visibility, a.owner_account, a.author, a.reports_open,
       (SELECT json_group_array(json_object('reason', r.reason, 'details', r.details, 'at', r.created_at))
          FROM (SELECT * FROM mod_reports r WHERE r.asset_id = a.id AND r.resolved_at IS NULL
                ORDER BY r.created_at DESC LIMIT 20) r) AS reports
     FROM mod_assets a WHERE a.reports_open > 0 AND a.state IN ('active', 'hidden')
     ORDER BY a.reports_open DESC, a.id DESC LIMIT 100`,
  ).all<Record<string, unknown> & { reports: string }>();
  const fresh = await env.DB.prepare(
    `SELECT a.id, a.title, a.type, a.state, a.visibility, a.author, a.first_seen
     FROM mod_assets a WHERE a.source = 'upload' AND a.state IN ('active', 'hidden')
     ORDER BY a.first_seen DESC LIMIT 50`,
  ).all();
  const rejected = await env.DB.prepare(
    `SELECT u.id, u.asset_id, u.filename, u.size, u.error, u.created_at FROM mod_uploads u
     WHERE u.state = 'rejected' ORDER BY u.created_at DESC LIMIT 50`,
  ).all();
  return {
    status: 200,
    body: {
      reported: reported.results.map((r) => ({ ...r, reports: JSON.parse(r.reports || "[]") })),
      recent_uploads: fresh.results,
      rejected_uploads: rejected.results,
    },
  };
}

export async function moderate(
  assetId: number,
  action: string,
  note: string | null,
  by: string,
  env: Env,
  now = Date.now(),
): Promise<Result> {
  const asset = await env.DB.prepare("SELECT state FROM mod_assets WHERE id = ?").bind(assetId).first<{ state: string }>();
  if (!asset || asset.state === "deleted" || asset.state === "removed") return { status: 404, body: { error: "no such mod" } };
  const resolve = (resolution: string) =>
    env.DB.prepare(
      "UPDATE mod_reports SET resolved_at = ?, resolution = ?, resolved_by = ? WHERE asset_id = ? AND resolved_at IS NULL",
    ).bind(now, resolution, by, assetId);
  const mark = (state: string) =>
    env.DB.prepare(
      "UPDATE mod_assets SET state = ?, reports_open = 0, moderated_at = ?, moderated_by = ?, moderation_note = ? WHERE id = ?",
    ).bind(state, now, by, note, assetId);
  switch (action) {
    case "hide":
      await env.DB.batch([mark("hidden"), resolve("hidden")]);
      break;
    case "unhide":
      await env.DB.batch([mark("active")]);
      break;
    case "remove":
      await env.DB.batch([mark("removed"), resolve("removed")]);
      await freeBlobs(env, assetId);
      break;
    case "dismiss":
      await env.DB.batch([
        resolve("dismissed"),
        env.DB.prepare("UPDATE mod_assets SET reports_open = 0 WHERE id = ?").bind(assetId),
      ]);
      break;
    default:
      return { status: 400, body: { error: "action must be hide, unhide, remove or dismiss" } };
  }
  console.log(JSON.stringify({ msg: "mod moderated", asset: assetId, action, by }));
  return { status: 200, body: { ok: true } };
}

export async function webModRoutes(request: Request, url: URL, env: Env, origin: string | null): Promise<Response> {
  const say = (status: number, body: unknown) => {
    const res = cors(
      new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } }),
      origin,
    );
    return res;
  };
  if (request.method === "OPTIONS") {
    return cors(new Response(null, { status: 204 }), origin, true, "GET, POST, OPTIONS", "Content-Type");
  }
  const session = await webSession(request, env);
  if (!session) return say(401, { error: "not signed in" });
  if (!isWebAdmin(session.steamId, env)) return say(403, { error: "not an admin" });
  const offSite = refuseCrossSiteWrite(request, env);
  if (offSite) return cors(offSite, origin);

  if (request.method === "GET" && url.pathname === "/v1/web/mods/queue") {
    const r = await moderationQueue(env);
    return say(r.status, r.body);
  }
  const m = /^\/v1\/web\/mods\/(\d{1,12})\/moderate$/.exec(url.pathname);
  if (m && request.method === "POST") {
    const b = ((await request.json().catch(() => null)) ?? {}) as Record<string, unknown>;
    const r = await moderate(
      Number(m[1]),
      String(b.action ?? ""),
      typeof b.note === "string" ? b.note.slice(0, 500) : null,
      `steam:${session.steamId}`,
      env,
    );
    return say(r.status, r.body);
  }
  return say(404, { error: "no such endpoint" });
}
