/**
 * Where MX Bikes died, across everyone it died on.
 *
 * The game closes to desktop by itself, and has done for years: landing an overjump, hitting
 * an object, clicking go to track, and more often on a busy server. FrostMod is inside the
 * process when it happens, so it is the one thing in a position to say where. It writes the
 * fault, the call stack and what the player was doing to a file beside its log; the app posts
 * that file here.
 *
 * One row per crash. The column the whole thing turns on is `site` — `mxbikes.exe+0x11D753`,
 * the module and offset of the faulting instruction — because two players hitting the same
 * bug send the same `site`, and nothing else about their two reports will look alike. Ranked
 * by how many distinct accounts hit it, that answers the question that used to be unanswerable:
 * which crash is worth a week.
 *
 * Everything here came off a client, so everything is bounded before it is stored: the frame
 * list and the trail are capped, every string is cut to a length, and the numbers are clamped.
 * A report is never interpreted — no rules, no verdicts, nothing sent back. `putCrash` answers
 * `{ ok: true }` to a report it stored and to one it already had, because the client's correct
 * behaviour in both cases is identical: rename the file and stop.
 *
 * The minidump is not here and is not sent. It is megabytes, it is a copy of process memory,
 * and it stays on the player's machine until a person asks for it.
 */

import { isAppVersion, isGuid } from "./validate";

export interface Account {
  id: string;
  rider_name: string;
  guid?: string | null;
}

/** Frames past this are noise: the interesting part of a stack is the top of it. */
export const MAX_FRAMES = 40;
/** The trail FrostMod keeps is 32 long, and a longer one did not come from FrostMod. */
export const MAX_TRAIL = 32;
/** One report, bounded. Rider and server names are attacker-controlled text. */
const MAX_TEXT = 160;
/** A run longer than a month did not happen; neither did a negative one. */
const MAX_MS = 40 * 24 * 60 * 60 * 1000;

export interface CrashReport {
  site: string;
  kind: string;
  code: string;
  access: string;
  target: string;
  game: string;
  frostmod: string;
  place: string;
  inSession: boolean;
  track: string;
  server: string;
  riders: number | null;
  reloads: number;
  sinceFrameMs: number | null;
  sinceReloadMs: number | null;
  uptimeMs: number;
  frames: string[];
  trail: { beforeMs: number; text: string }[];
  hasDump: boolean;
  crashedAt: number;
}

function text(value: unknown, max = MAX_TEXT): string {
  return typeof value === "string" ? value.slice(0, max) : "";
}

function count(value: unknown, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return 0;
  return Math.min(Math.floor(value), max);
}

/** Null survives as null: "not in a race" and "a grid of nobody" are different answers. */
function optional(value: unknown, max: number): number | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return null;
  return Math.min(Math.floor(value), max);
}

/**
 * A crash site: `module+0xRVA`, and nothing else.
 *
 * Checked rather than trusted because it is the grouping key. A client that could send any
 * string here could split one crash across a thousand rows, or merge a thousand into one, and
 * the dashboard would be confidently wrong either way. A fault outside every loaded module
 * has no module name, so FrostMod sends the bare address form, which is accepted too.
 */
export function isSite(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 128) return false;
  return /^[\w.+-]{1,96}\+0x[0-9A-Fa-f]{1,16}$/.test(value) || /^0x[0-9A-Fa-f]{1,16}$/.test(value);
}

/**
 * The report as the client sent it, or null if it is not one.
 *
 * Absent optional fields are fine and common: an older FrostMod sends fewer of them. A
 * present field of the wrong type is not a report, because the one field that must be right
 * is the one everything groups by, and a client that got the rest of the shape wrong has not
 * earned the benefit of the doubt on that one.
 */
export function parseCrash(body: unknown): CrashReport | null {
  if (!body || typeof body !== "object") return null;
  const b = body as Record<string, unknown>;
  const fault = (b.fault && typeof b.fault === "object" ? b.fault : {}) as Record<string, unknown>;
  const session = (b.session && typeof b.session === "object" ? b.session : {}) as Record<string, unknown>;

  if (!isSite(fault.site)) return null;

  // Not the client's clock. A report stamped in 2031 would sort above every real crash
  // forever, and one stamped at 0 would never be seen again.
  const when = Date.parse(text(b.when, 40));
  const now = Date.now();
  const crashedAt = Number.isFinite(when) && when > 0 && when < now + 60_000 ? when : now;

  const frames: string[] = [];
  if (Array.isArray(b.frames)) {
    for (const frame of b.frames.slice(0, MAX_FRAMES)) {
      if (typeof frame === "string" && frame) frames.push(frame.slice(0, 128));
    }
  }

  const trail: { beforeMs: number; text: string }[] = [];
  if (Array.isArray(b.trail)) {
    for (const note of b.trail.slice(0, MAX_TRAIL)) {
      if (!note || typeof note !== "object") continue;
      const n = note as Record<string, unknown>;
      if (typeof n.text !== "string" || !n.text) continue;
      trail.push({ beforeMs: count(n.beforeMs, MAX_MS), text: n.text.slice(0, MAX_TEXT) });
    }
  }

  return {
    site: fault.site as string,
    kind: text(fault.kind, 64),
    code: text(fault.code, 16),
    access: text(fault.access, 16),
    target: text(fault.target, 32),
    game: text(b.game, 64),
    frostmod: text(b.frostmod, 32),
    place: text(session.where, 64),
    inSession: session.inSession === true,
    track: text(session.track),
    server: text(session.server),
    riders: optional(session.riders, 256),
    reloads: count(session.reloads, 10_000),
    sinceFrameMs: optional(session.sinceFrameMs, MAX_MS),
    sinceReloadMs: optional(session.sinceReloadMs, MAX_MS),
    uptimeMs: count(b.uptimeMs, MAX_MS),
    frames,
    trail,
    hasDump: typeof b.dump === "string" && b.dump.length > 0,
    crashedAt,
  };
}

interface Env {
  DB: D1Database;
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

/**
 * Take a crash report.
 *
 * `INSERT OR IGNORE` against the unique index does the deduplication: the app renames its
 * file only after a success, so a success whose answer was lost on the way back is retried,
 * and a crash counted twice would look twice as common as it is.
 */
export async function putCrash(request: Request, account: Account, env: Env): Promise<Response> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return json(400, { error: "expected a JSON body" });
  }
  if (!body || typeof body !== "object") return json(400, { error: "expected a JSON body" });

  const report = parseCrash(body);
  if (!report) return json(400, { error: "that is not a crash report" });

  const b = body as Record<string, unknown>;
  const appVersion = isAppVersion(b.appVersion) ? (b.appVersion as string) : "";
  const guid = isGuid(b.guid) ? (b.guid as string) : (account.guid ?? "");

  await env.DB.prepare(
    `INSERT OR IGNORE INTO client_crashes (
       account_id, rider_name, guid,
       site, kind, code, access, target,
       game, build, frostmod, app_version,
       place, in_session, track, server, riders, reloads,
       since_frame_ms, since_reload_ms, uptime_ms,
       frames, trail, has_dump, crashed_at, received_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  )
    .bind(
      account.id,
      account.rider_name ?? "",
      guid,
      report.site,
      report.kind,
      report.code,
      report.access,
      report.target,
      report.game,
      typeof b.build === "string" ? b.build.slice(0, 64) : "",
      report.frostmod,
      appVersion,
      report.place,
      report.inSession ? 1 : 0,
      report.track,
      report.server,
      report.riders,
      report.reloads,
      report.sinceFrameMs,
      report.sinceReloadMs,
      report.uptimeMs,
      JSON.stringify(report.frames),
      JSON.stringify(report.trail),
      report.hasDump ? 1 : 0,
      report.crashedAt,
      Date.now(),
    )
    .run();

  // Nothing comes back down. The client's next move is the same whatever happened here.
  return json(200, { ok: true });
}

export interface CrashSite {
  site: string;
  hits: number;
  riders: number;
  lastAt: number;
  firstAt: number;
  kind: string;
  /** How many of those happened in a session rather than in the menus. */
  inSession: number;
  /** How many left a dump on a player's machine, so it can be asked for. */
  withDump: number;
}

export interface CrashRow {
  id: number;
  riderName: string;
  site: string;
  kind: string;
  place: string;
  track: string;
  server: string;
  riders: number | null;
  frostmod: string;
  appVersion: string;
  hasDump: boolean;
  crashedAt: number;
}

/**
 * What the dashboard opens on: the crash sites, worst first.
 *
 * Ranked by distinct riders rather than by hits, because a hundred crashes from one machine
 * is one person having a bad night and three crashes across three accounts is a bug. The
 * ranking decides what gets looked at, so it has to be the honest number.
 */
export async function crashSites(db: D1Database, limit = 50): Promise<CrashSite[]> {
  const { results } = await db
    .prepare(
      `SELECT site,
              COUNT(*)                        AS hits,
              COUNT(DISTINCT account_id)      AS riders,
              MAX(received_at)                AS last_at,
              MIN(received_at)                AS first_at,
              MAX(kind)                       AS kind,
              SUM(in_session)                 AS in_session,
              SUM(has_dump)                   AS with_dump
         FROM client_crashes
        GROUP BY site
        ORDER BY riders DESC, hits DESC, last_at DESC
        LIMIT ?`,
    )
    .bind(Math.min(Math.max(limit, 1), 200))
    .all<Record<string, unknown>>();

  return (results ?? []).map((r) => ({
    site: String(r.site ?? ""),
    hits: Number(r.hits ?? 0),
    riders: Number(r.riders ?? 0),
    lastAt: Number(r.last_at ?? 0),
    firstAt: Number(r.first_at ?? 0),
    kind: String(r.kind ?? ""),
    inSession: Number(r.in_session ?? 0),
    withDump: Number(r.with_dump ?? 0),
  }));
}

/** The most recent crashes, whatever they were. The "what is happening right now" list. */
export async function recentCrashes(db: D1Database, limit = 50): Promise<CrashRow[]> {
  const { results } = await db
    .prepare(
      `SELECT id, rider_name, site, kind, place, track, server, riders,
              frostmod, app_version, has_dump, crashed_at
         FROM client_crashes
        ORDER BY received_at DESC
        LIMIT ?`,
    )
    .bind(Math.min(Math.max(limit, 1), 200))
    .all<Record<string, unknown>>();

  return (results ?? []).map((r) => ({
    id: Number(r.id ?? 0),
    riderName: String(r.rider_name ?? ""),
    site: String(r.site ?? ""),
    kind: String(r.kind ?? ""),
    place: String(r.place ?? ""),
    track: String(r.track ?? ""),
    server: String(r.server ?? ""),
    riders: r.riders === null || r.riders === undefined ? null : Number(r.riders),
    frostmod: String(r.frostmod ?? ""),
    appVersion: String(r.app_version ?? ""),
    hasDump: Number(r.has_dump ?? 0) === 1,
    crashedAt: Number(r.crashed_at ?? 0),
  }));
}

/**
 * One crash site, in full: every report of it, with the stacks and the trails.
 *
 * This is the read that replaces what taking one apart used to mean. Forty stacks of the same
 * fault, side by side, is how the frame they share stops being a guess.
 */
export async function crashDetail(
  db: D1Database,
  site: string,
  limit = 25,
): Promise<{ site: string; reports: (CrashRow & { frames: string[]; trail: unknown[] })[] } | null> {
  if (!isSite(site)) return null;
  const { results } = await db
    .prepare(
      `SELECT id, rider_name, site, kind, place, track, server, riders,
              frostmod, app_version, has_dump, crashed_at, frames, trail
         FROM client_crashes
        WHERE site = ?
        ORDER BY received_at DESC
        LIMIT ?`,
    )
    .bind(site, Math.min(Math.max(limit, 1), 100))
    .all<Record<string, unknown>>();

  const reports = (results ?? []).map((r) => ({
    id: Number(r.id ?? 0),
    riderName: String(r.rider_name ?? ""),
    site: String(r.site ?? ""),
    kind: String(r.kind ?? ""),
    place: String(r.place ?? ""),
    track: String(r.track ?? ""),
    server: String(r.server ?? ""),
    riders: r.riders === null || r.riders === undefined ? null : Number(r.riders),
    frostmod: String(r.frostmod ?? ""),
    appVersion: String(r.app_version ?? ""),
    hasDump: Number(r.has_dump ?? 0) === 1,
    crashedAt: Number(r.crashed_at ?? 0),
    // Stored as the client's own JSON and never interpreted here. A row written before a
    // shape changed still reads, and a malformed one is an empty list rather than a 500.
    frames: safeList(r.frames).filter((f): f is string => typeof f === "string"),
    trail: safeList(r.trail),
  }));
  return { site, reports };
}

function safeList(value: unknown): unknown[] {
  if (typeof value !== "string") return [];
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}
