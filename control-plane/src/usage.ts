/**
 * Anonymous usage counters.
 *
 * The question this answers is the one nothing else could: how many people actually run the
 * app, and which parts of it do they open. Release downloads count downloads, and the
 * accounts table counts the few who claimed an invite — neither is the number that decides
 * whether a feature is worth carrying.
 *
 * ## What it is not
 *
 * There is no user here. The key is an install id the app generated for itself, a random
 * UUID kept in its own config; it is tied to no account, no rider name and no machine. IP
 * addresses are hashed for the day and used only to rate limit, exactly as open signup
 * already does. What arrives is a day, a version, an OS, a title, a session count and a
 * handful of counters — and `isEventName` is what makes it impossible for a careless call
 * site to smuggle anything else in.
 *
 * ## Why rollups
 *
 * Every write is an upsert onto a row that already exists for that install and day, so the
 * table grows with the number of people running the app rather than with how long they
 * leave it open. `install_id` stays in the events key because reach
 * (COUNT(DISTINCT install_id)) and volume (SUM(count)) answer different questions, and
 * "nobody uses this" is only ever the first one.
 */

import { tokenMatches } from "./auth";
import { ipDigest } from "./voice";
import {
  isAppVersion,
  isCount,
  isEventName,
  isGameId,
  isInstallId,
  isPlatform,
  MAX_EVENTS_PER_REPORT,
  MAX_EVENT_COUNT,
  MAX_REPORT_MINUTES,
} from "./validate";

/** A report is a few hundred bytes. Anything approaching this is not one. */
export const MAX_REPORT_BYTES = 16 * 1024;

/**
 * Reports accepted from one address per day.
 *
 * An install flushes every few minutes while it is open, so a heavy day is around 300 for a
 * single machine; a household or a LAN shares an address. Set well above both, because the
 * cost of turning away a real player's numbers is worse than the cost of a few junk rows.
 */
export const MAX_REPORTS_PER_DAY = 2000;

/** How long counters are kept. Long enough to compare a season against the last one. */
export const RETENTION_DAYS = 400;

/**
 * Everything the app is expected to report.
 *
 * A display aid, not a filter: a name absent from this list is still stored, because a
 * shipped build that starts sending something new must not have its data dropped by a
 * worker that hasn't been redeployed. What the list buys is the other half of the question —
 * a feature nobody has touched has no row at all, and only a list of what *should* be there
 * can show it.
 */
export const KNOWN_EVENTS = [
  "app.start",
  "app.update",
  "view.browse",
  "view.library",
  "view.downloads",
  "view.locker",
  "view.presets",
  "view.studio.designer",
  "view.studio.paints",
  "view.studio.rider",
  "view.studio.pose",
  "view.studio.track",
  "view.studio.protect",
  "view.manage",
  "view.shop",
  "view.hub",
  "view.settings",
  "game.launch",
  "mod.install",
  "mod.download",
  "mod.detail",
  "preset.apply",
  "preset.save",
  "paint.publish",
  "paint.save",
  "track.generate",
  "track.install",
  "content.protect",
  "voice.join",
  "server.join",
  "overlay.open",
  "frostmod.install",
  "drop.import",
] as const;

interface Report {
  installId: string;
  version: string;
  os: string;
  game: string;
  sessions: number;
  minutes: number;
  events: { name: string; count: number }[];
}

/**
 * `POST /v1/usage` — one install's counters since its last report.
 *
 * Unauthenticated, like open signup: the caller holds no token and there is no token we
 * could give it that wouldn't itself be an identifier. Everything that could be abused is
 * bounded instead — the body size, the number of events, each count, and the reports one
 * address may send in a day.
 */
export async function reportUsage(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_REPORT_BYTES) return json(413, { error: "report too large" });

  const raw = await readText(request);
  if (raw === null || raw.length > MAX_REPORT_BYTES) {
    return json(413, { error: "report too large" });
  }
  const report = parseReport(raw);
  if (typeof report === "string") return json(400, { error: report });

  const now = Date.now();
  const day = new Date(now).toISOString().slice(0, 10);
  const digest = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);
  const seen = await env.DB.prepare(
    "SELECT claims FROM device_claims WHERE ip_digest = ? AND day = ? AND kind = 'usage'",
  )
    .bind(digest, day)
    .first<{ claims: number }>();
  if (seen && seen.claims >= MAX_REPORTS_PER_DAY) {
    // A real 429, not the 202-and-ignore this first shipped with. The client reads it as
    // "stop reporting for this run" rather than as something to retry, which is the only
    // answer that actually reduces load — and load is the entire reason the cap exists.
    return json(429, { error: "too many reports from here today" });
  }

  const statements = [
    env.DB.prepare(
      "INSERT INTO usage_daily" +
        " (install_id, day, version, os, game, sessions, minutes, first_seen, updated_at)" +
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)" +
        " ON CONFLICT(install_id, day) DO UPDATE SET" +
        "  version = excluded.version, os = excluded.os, game = excluded.game," +
        "  sessions = sessions + excluded.sessions, minutes = minutes + excluded.minutes," +
        "  updated_at = excluded.updated_at",
    ).bind(
      report.installId,
      day,
      report.version,
      report.os,
      report.game,
      report.sessions,
      report.minutes,
      now,
      now,
    ),
    env.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
        " VALUES (?, ?, 'usage', 1, ?)" +
        " ON CONFLICT(ip_digest, day, kind) DO UPDATE SET" +
        "  claims = claims + 1, updated_at = excluded.updated_at",
    ).bind(digest, day, now),
  ];
  for (const event of report.events) {
    statements.push(
      env.DB.prepare(
        "INSERT INTO usage_events (day, name, install_id, count, updated_at)" +
          " VALUES (?, ?, ?, ?, ?)" +
          " ON CONFLICT(day, name, install_id) DO UPDATE SET" +
          "  count = count + excluded.count, updated_at = excluded.updated_at",
      ).bind(day, event.name, report.installId, event.count, now),
    );
  }
  await env.DB.batch(statements);

  return json(202, { ok: true });
}

/**
 * Check a report, returning the reason it was refused rather than a bare false.
 *
 * The messages are for whoever is writing a client, which for now is us — an app in the
 * field never reads them, because a rejected report is dropped and forgotten.
 */
export function parseReport(raw: string): Report | string {
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "expected a JSON body";
  }
  if (!body || typeof body !== "object") return "expected a JSON body";
  const { installId, version, os, game, sessions, minutes, events } = body as Record<
    string,
    unknown
  >;

  if (!isInstallId(installId)) return "installId must be a UUID";
  if (!isAppVersion(version)) return "version must be a semver string";
  if (!isPlatform(os)) return "os must be windows, macos or linux";
  if (!isGameId(game)) return "game must be mxb or gpb";
  if (!isCount(sessions, 1000)) return "sessions out of range";
  if (!isCount(minutes, MAX_REPORT_MINUTES)) return "minutes out of range";
  if (!Array.isArray(events)) return "events must be an array";
  if (events.length > MAX_EVENTS_PER_REPORT) return "too many events in one report";

  const seen = new Set<string>();
  const clean: { name: string; count: number }[] = [];
  for (const entry of events) {
    if (!entry || typeof entry !== "object") return "each event must be an object";
    const { name, count } = entry as Record<string, unknown>;
    if (!isEventName(name)) return `not an event name: ${String(name).slice(0, 32)}`;
    if (!isCount(count, MAX_EVENT_COUNT)) return "event count out of range";
    // A name twice in one report is a client bug; folding them beats letting the later one
    // decide, and beats rejecting a report over something we can simply add up.
    if (seen.has(name)) {
      const existing = clean.find((e) => e.name === name);
      if (existing) existing.count += count as number;
      continue;
    }
    seen.add(name);
    clean.push({ name: name as string, count: count as number });
  }

  return {
    installId: installId as string,
    version: version as string,
    os: os as string,
    game: game as string,
    sessions: sessions as number,
    minutes: minutes as number,
    events: clean,
  };
}

export interface Bucket {
  label: string;
  installs: number;
}

export interface EventRow {
  name: string;
  /** How many distinct installs did it at all. The "is anyone using this" number. */
  reach: number;
  /** How many times in total. The "how hard do they lean on it" number. */
  volume: number;
}

export interface DayRow {
  day: string;
  installs: number;
  sessions: number;
  minutes: number;
}

export interface Stats {
  generatedAt: number;
  days: number;
  active: { day: number; week: number; month: number };
  installsEver: number;
  newInstalls: number;
  sessions: number;
  minutes: number;
  daily: DayRow[];
  /** Installs that ran each version at any point in the window. Overlaps — see `currentVersions`. */
  versions: Bucket[];
  /** The version each install last reported. One bucket each, so these sum to the window's actives. */
  currentVersions: Bucket[];
  platforms: Bucket[];
  games: Bucket[];
  events: EventRow[];
  /** Names from `KNOWN_EVENTS` with no rows in the window at all. */
  unused: string[];
}

/** UTC day, `n` days back from `now`. */
export function dayKey(now: number, back = 0): string {
  return new Date(now - back * 86_400_000).toISOString().slice(0, 10);
}

/**
 * Everything the dashboard shows, in one pass over the two tables.
 *
 * Read-only and cheap: every query is an aggregate over an indexed day range, so the cost is
 * the size of the window rather than the size of the history.
 */
export async function collectStats(env: Env, days: number, now = Date.now()): Promise<Stats> {
  const today = dayKey(now);
  const from = dayKey(now, days - 1);
  const week = dayKey(now, 6);
  const month = dayKey(now, 29);

  const q = <T>(sql: string, ...binds: unknown[]) =>
    env.DB.prepare(sql)
      .bind(...binds)
      .all<T>();

  const [active, ever, fresh, totals, daily, versions, current, platforms, games, events] =
    await Promise.all([
      q<{ day: number; week: number; month: number }>(
        "SELECT" +
          "  COUNT(DISTINCT CASE WHEN day = ?1 THEN install_id END) AS day," +
          "  COUNT(DISTINCT CASE WHEN day >= ?2 THEN install_id END) AS week," +
          "  COUNT(DISTINCT CASE WHEN day >= ?3 THEN install_id END) AS month" +
          " FROM usage_daily WHERE day >= ?3",
        today,
        week,
        month,
      ),
      q<{ n: number }>("SELECT COUNT(DISTINCT install_id) AS n FROM usage_daily"),
      // An install is new in the window if the first day we ever saw it falls inside it.
      q<{ n: number }>(
        "SELECT COUNT(*) AS n FROM (" +
          " SELECT install_id, MIN(day) AS firstDay FROM usage_daily GROUP BY install_id" +
          ") WHERE firstDay >= ?",
        from,
      ),
      q<{ sessions: number; minutes: number }>(
        "SELECT COALESCE(SUM(sessions), 0) AS sessions, COALESCE(SUM(minutes), 0) AS minutes" +
          " FROM usage_daily WHERE day >= ?",
        from,
      ),
      q<DayRow>(
        "SELECT day, COUNT(*) AS installs, COALESCE(SUM(sessions), 0) AS sessions," +
          " COALESCE(SUM(minutes), 0) AS minutes" +
          " FROM usage_daily WHERE day >= ? GROUP BY day ORDER BY day",
        from,
      ),
      q<Bucket>(
        "SELECT version AS label, COUNT(DISTINCT install_id) AS installs FROM usage_daily" +
          " WHERE day >= ? GROUP BY version ORDER BY installs DESC, label DESC",
        from,
      ),
      // What everyone is on now. `versions` counts an install under every version it ran,
      // so it overcounts; here each install contributes once, from its most recent day.
      q<Bucket>(
        "SELECT label, COUNT(*) AS installs FROM (" +
          " SELECT version AS label," +
          " ROW_NUMBER() OVER (PARTITION BY install_id ORDER BY day DESC) AS rn" +
          " FROM usage_daily WHERE day >= ?" +
          ") WHERE rn = 1 GROUP BY label ORDER BY installs DESC, label DESC",
        from,
      ),
      q<Bucket>(
        "SELECT os AS label, COUNT(DISTINCT install_id) AS installs FROM usage_daily" +
          " WHERE day >= ? GROUP BY os ORDER BY installs DESC",
        from,
      ),
      q<Bucket>(
        "SELECT game AS label, COUNT(DISTINCT install_id) AS installs FROM usage_daily" +
          " WHERE day >= ? GROUP BY game ORDER BY installs DESC",
        from,
      ),
      q<EventRow>(
        "SELECT name, COUNT(DISTINCT install_id) AS reach, COALESCE(SUM(count), 0) AS volume" +
          " FROM usage_events WHERE day >= ? GROUP BY name ORDER BY reach DESC, volume DESC",
        from,
      ),
    ]);

  const rows = events.results ?? [];
  const seen = new Set(rows.map((r) => r.name));
  const counts = active.results?.[0] ?? { day: 0, week: 0, month: 0 };

  return {
    generatedAt: now,
    days,
    active: { day: counts.day ?? 0, week: counts.week ?? 0, month: counts.month ?? 0 },
    installsEver: ever.results?.[0]?.n ?? 0,
    newInstalls: fresh.results?.[0]?.n ?? 0,
    sessions: totals.results?.[0]?.sessions ?? 0,
    minutes: totals.results?.[0]?.minutes ?? 0,
    daily: daily.results ?? [],
    versions: versions.results ?? [],
    currentVersions: current.results ?? [],
    platforms: platforms.results ?? [],
    games: games.results ?? [],
    events: rows,
    unused: KNOWN_EVENTS.filter((name) => !seen.has(name)),
  };
}

/**
 * Is this request allowed to read the numbers?
 *
 * `ADMIN_KEY` is a secret like the rest (see `env.d.ts`); a deployment without one has no
 * admin surface at all rather than an open one. The key may arrive as a bearer token or as
 * `?key=`, because the dashboard is opened by typing a URL into a browser and a browser
 * cannot send a header.
 */
export function adminAllowed(request: Request, url: URL, env: Env): "ok" | "unset" | "denied" {
  const expected = env.ADMIN_KEY;
  if (!expected) return "unset";
  const header = request.headers.get("Authorization");
  const presented = /^Bearer\s+(.+)$/i.exec(header?.trim() ?? "")?.[1] ?? url.searchParams.get("key");
  if (!presented) return "denied";
  return tokenMatches(expected, presented) ? "ok" : "denied";
}

/** How many days a request asked for, clamped to something a dashboard can draw. */
export function windowDays(url: URL): number {
  const asked = Number(url.searchParams.get("days") ?? "30");
  if (!Number.isFinite(asked)) return 30;
  return Math.min(365, Math.max(1, Math.trunc(asked)));
}

/** `GET /v1/usage/stats` — the same numbers as the dashboard, for anything that scripts them. */
export async function usageStats(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return json(503, { error: "no admin key is configured" });
  if (allowed === "denied") return json(401, { error: "unauthorized" });
  return json(200, await collectStats(env, windowDays(url)));
}

/**
 * Drop counters past the retention window.
 *
 * Runs on the same cron as the idle-server sweep. Nothing here is worth keeping forever, and
 * a table that only grows is a bill nobody decided to pay.
 */
export async function pruneUsage(env: Env): Promise<void> {
  const cutoff = dayKey(Date.now(), RETENTION_DAYS);
  try {
    await env.DB.batch([
      env.DB.prepare("DELETE FROM usage_events WHERE day < ?").bind(cutoff),
      env.DB.prepare("DELETE FROM usage_daily WHERE day < ?").bind(cutoff),
    ]);
  } catch (err) {
    // A sweep that fails is the next sweep's problem, as with the signup counters.
    console.error(JSON.stringify({ msg: "usage sweep failed", error: String(err) }));
  }
}

async function readText(request: Request): Promise<string | null> {
  try {
    return await request.text();
  } catch {
    return null;
  }
}

// index.ts has its own copy; duplicating four lines beats importing the entry point back
// into a module it imports.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
