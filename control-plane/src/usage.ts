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
 * ## What these numbers can and cannot promise
 *
 * Worth being plain about, because decisions get made from them. Every field in a report is
 * chosen by the caller, so `version`, `os` and `game` are only as true as whoever sent them —
 * and those three are precisely what "can I stop shipping 0.8.x" and "is the GP Bikes side worth
 * carrying" are read off. What holds them up is not authentication, which this endpoint cannot
 * have, but a stack of bounds: the content type (which is what keeps a web page from conscripting
 * its visitors), the per-address daily cap, the ceilings each row can hold, and — where a
 * deployment turns it on — a build signature. None of that makes a figure unforgeable; together
 * they make forging one cost more than the decision it would move is worth.
 *
 * An install id is a UUID in a file, so the counting is honest rather than exact in the other
 * direction too: clearing a config mints a new install, and a cloned or imaged machine reports
 * as the one it was cloned from.
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
  APPS,
  isAppId,
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
 * An install flushes every half hour while it is open, so a machine left running all day is
 * around 48; a household or a LAN shares an address. Set well above that, because the cost of
 * turning away a real player's numbers is worse than the cost of a few junk rows.
 *
 * What this is *not* is the thing that stops a determined caller: it is keyed on an address, and
 * anyone who minds can use more than one. It bounds an accident — a client in a retry loop, a
 * script someone left running. The deliberate case is bounded by the content type this endpoint
 * insists on and, where it is configured, by [`REQUIRE_SIGNATURE`].
 */
export const MAX_REPORTS_PER_DAY = 2000;

/**
 * The most one `(install, app, day)` row may hold.
 *
 * Reports accumulate onto the row — that is what makes it a rollup — and nothing used to bound
 * the total. Each report is capped at [`MAX_REPORT_MINUTES`], whose comment says "a day is the
 * most that can honestly be reported at once", but 2,000 honest-looking reports put 2,000 days
 * of wall clock inside one 24-hour day, and "Minutes per session" believed every one of them.
 *
 * A day holds 1,440 minutes; a launch a minute all day is already an absurd number of sessions.
 * Past either, the row stops climbing rather than the report being refused: a clamp is
 * idempotent and self-healing, and a real install can never reach it.
 */
export const MAX_DAY_MINUTES = 1440;
export const MAX_DAY_SESSIONS = 1440;

/** The content type a report must arrive as. See [`reportUsage`]. */
const REPORT_CONTENT_TYPE = "application/json";

/**
 * Whether an unsigned report is refused.
 *
 * Off unless the deployment says otherwise, and it must stay off until signed builds are the
 * ones in the field — turning it on early throws away everybody's numbers silently, which is
 * worse than the spoofing it prevents. See [`signatureOk`] for what a signature is worth.
 */
function requireSignature(env: Env): boolean {
  return env.MXB_USAGE_REQUIRE_SIGNATURE === "1";
}

/** How long counters are kept. Long enough to compare a season against the last one. */
export const RETENTION_DAYS = 400;

/**
 * The longest window that can be asked for. It has to stay under `RETENTION_DAYS`.
 *
 * `newInstalls` counts installs whose *first day ever* falls in the window, and the sweep has
 * been deleting first days. Once a window reaches back as far as the prune, every surviving
 * install's oldest row is inside it and they all look new — the figure does not fail, it
 * quietly lies. The two constants have always been related; nothing said so, so
 * `usage.test.ts` now asserts it.
 *
 * Retention is the same trap one window further out, and cannot be fixed by a ceiling: it
 * compares this window with the one *before* it, so it needs twice the reach. It is left out
 * rather than approximated when that lands past the prune.
 */
export const MAX_WINDOW_DAYS = 365;

/** Which app may report a name. */
type AppId = (typeof APPS)[number];

const EVERY: readonly AppId[] = APPS;
const MANAGER: readonly AppId[] = ["manager"];
const STUDIO: readonly AppId[] = ["studio"];
const COACH: readonly AppId[] = ["coach"];

/**
 * Everything the apps are expected to report, and which of them may report it.
 *
 * A display aid, not a filter: a name absent from this list is still stored, because a shipped
 * build that starts sending something new must not have its data dropped by a worker that hasn't
 * been redeployed. What the list buys is the other half of the question — a feature nobody has
 * touched has no row at all, and only a list of what *should* be there can show it.
 *
 * Which app owns a name matters for exactly that. The list used to be flat, so narrowing the
 * dashboard to one app reported the *other* app's whole vocabulary as never touched — which
 * buried the handful of names that were genuinely untouched in a wall of names that were never
 * going to be there.
 *
 * The client keeps the same list, as a closed one it will not report outside of
 * (`crates/core/src/usage.rs`). `usage.test.ts` reads that file and proves the two have not
 * drifted: a name the apps send but this list has never heard of can never turn up in "Never
 * touched", which is the one panel that exists to name an absence.
 */
export const KNOWN_EVENTS: Readonly<Record<string, readonly AppId[]>> = {
  // Lifecycle, from all three.
  "app.start": EVERY,
  "app.update": EVERY,

  // The manager's pages. `view.plugin` is every plugin panel in one bucket, because naming
  // each one would be unbounded cardinality.
  "view.browse": MANAGER,
  "view.library": MANAGER,
  "view.downloads": MANAGER,
  "view.locker": MANAGER,
  "view.presets": MANAGER,
  "view.manage": MANAGER,
  "view.shop": MANAGER,
  "view.hub": MANAGER,
  "view.servers": MANAGER,
  "view.ranked": MANAGER,
  "view.studio": MANAGER,
  "view.plugin": MANAGER,
  // The studio counts its own settings tool as `view.studio.settings`, so this one is the
  // manager's page and Coach's.
  "view.settings": ["manager", "coach"],

  // What the manager is for.
  "mod.detail": MANAGER,
  "mod.install": MANAGER,
  "mod.download": MANAGER,
  "game.launch": MANAGER,
  "preset.apply": MANAGER,
  "preset.save": ["manager", "studio"],
  "paint.publish": MANAGER,
  "voice.join": MANAGER,
  "server.join": MANAGER,
  "overlay.open": MANAGER,
  "frostmod.install": MANAGER,
  "drop.import": MANAGER,

  // Frost's Studio: its tools, and what they make.
  "view.studio.designer": STUDIO,
  "view.studio.paints": STUDIO,
  "view.studio.rider": STUDIO,
  "view.studio.pose": STUDIO,
  "view.studio.track": STUDIO,
  "view.studio.diagnose": STUDIO,
  "view.studio.settings": STUDIO,
  "track.generate": STUDIO,
  "track.settings": STUDIO,
  "track.build.install": STUDIO,
  "paint.save": STUDIO,

  // MXB Coach.
  "view.sessions": COACH,
  "coach.session.open": COACH,
  "coach.review": COACH,
};

/** The names one app — or all of them together — is expected to report. */
export function knownFor(app: AppFilter): string[] {
  return Object.keys(KNOWN_EVENTS).filter(
    (name) => app === "all" || KNOWN_EVENTS[name].includes(app),
  );
}

/** The header a signed report carries. `v1 <unix seconds> <hex hmac-sha256>`. */
export const SIGNATURE_HEADER = "X-MXB-Usage";

/** How far a signed report's clock may be out. Wide enough for a machine nobody syncs. */
export const MAX_SIGNATURE_SKEW_SECONDS = 15 * 60;

/**
 * Is this report signed by something that holds the build key?
 *
 * ## What this buys, and what it does not
 *
 * The key is compiled into a client that anyone can download, so it is extractable and this is
 * not authentication — a determined person will pull it out of the binary. What it stops is
 * everything cheaper than that: a `curl` loop, a script pointed at the endpoint, and the case
 * that actually worries this deployment — a web page quietly making its visitors post reports
 * from their own addresses, where the per-address cap buys nothing because every visitor brings
 * a fresh one. Raising the floor from "anyone with a terminal" to "someone willing to reverse a
 * binary" is the whole of the ambition.
 *
 * The timestamp bounds replay to [`MAX_SIGNATURE_SKEW_SECONDS`] rather than preventing it: a
 * captured report can be sent again inside that window. What it is worth there is bounded by the
 * row ceilings above, which is why those came first.
 */
export async function signatureOk(
  presented: string | null,
  body: string,
  env: Env,
  now = Date.now(),
): Promise<boolean> {
  const key = env.USAGE_SIGNING_KEY;
  if (!key || !presented) return false;
  const [version, seconds, mac] = presented.trim().split(/\s+/);
  if (version !== "v1" || !seconds || !mac) return false;

  const at = Number(seconds);
  if (!Number.isInteger(at)) return false;
  if (Math.abs(now / 1000 - at) > MAX_SIGNATURE_SKEW_SECONDS) return false;

  const imported = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(key),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signed = await crypto.subtle.sign(
    "HMAC",
    imported,
    new TextEncoder().encode(`v1.${seconds}.${body}`),
  );
  const expected = [...new Uint8Array(signed)].map((b) => b.toString(16).padStart(2, "0")).join("");
  return tokenMatches(expected, mac.toLowerCase());
}

interface Report {
  installId: string;
  /** Which app reported. Absent on a build that predates the field, and that is the manager. */
  app: (typeof APPS)[number];
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
 * bounded instead — the body size, the number of events, each count, the reports one address
 * may send in a day, and the ceilings on the row they land on.
 *
 * ## Why the content type is insisted on
 *
 * The body used to be read without looking at it, which made this a CORS *simple request*: a
 * `text/plain` POST from any web page is delivered and processed, and the page does not care
 * that it cannot read the reply. That turned every visitor to that page into a reporter, from
 * their own address — so the per-address cap, the one bound this endpoint had, was being handed
 * a fresh bucket per visitor. Insisting on `application/json` forces a preflight, and this
 * worker answers no CORS headers here, so the browser never sends the request at all. The app's
 * own client has always sent this type; nothing in the field notices.
 */
export async function reportUsage(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_REPORT_BYTES) return json(413, { error: "report too large" });

  const type = (request.headers.get("content-type") ?? "").split(";")[0].trim().toLowerCase();
  if (type !== REPORT_CONTENT_TYPE) {
    return json(415, { error: `expected ${REPORT_CONTENT_TYPE}` });
  }

  const raw = await readText(request);
  if (raw === null || raw.length > MAX_REPORT_BYTES) {
    return json(413, { error: "report too large" });
  }
  if (requireSignature(env) && !(await signatureOk(request.headers.get(SIGNATURE_HEADER), raw, env))) {
    return json(401, { error: "unsigned report" });
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
        " (install_id, app, day, version, os, game, sessions, minutes, first_seen, updated_at)" +
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)" +
        " ON CONFLICT(install_id, app, day) DO UPDATE SET" +
        "  version = excluded.version, os = excluded.os, game = excluded.game," +
        // Accumulate, but never past what a day can hold. A real install cannot reach either
        // ceiling; anything that does is telling us something other than how long it was open.
        // Spliced, not bound: these are module constants, and mixing numbered parameters into
        // a statement whose other placeholders are positional is exactly the kind of quiet
        // off-by-one that returns a plausible wrong number.
        `  sessions = MIN(${MAX_DAY_SESSIONS}, sessions + excluded.sessions),` +
        `  minutes = MIN(${MAX_DAY_MINUTES}, minutes + excluded.minutes),` +
        "  updated_at = excluded.updated_at",
    ).bind(
      report.installId,
      report.app,
      day,
      report.version,
      report.os,
      report.game,
      Math.min(report.sessions, MAX_DAY_SESSIONS),
      Math.min(report.minutes, MAX_DAY_MINUTES),
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
        "INSERT INTO usage_events (day, app, name, install_id, count, updated_at)" +
          " VALUES (?, ?, ?, ?, ?, ?)" +
          " ON CONFLICT(day, app, name, install_id) DO UPDATE SET" +
          "  count = count + excluded.count, updated_at = excluded.updated_at",
      ).bind(day, report.app, event.name, report.installId, event.count, now),
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
  const { installId, app, version, os, game, sessions, minutes, events } = body as Record<
    string,
    unknown
  >;

  if (!isInstallId(installId)) return "installId must be a UUID";
  // Absent is the manager: every build that shipped before this field existed is one, and a
  // report from one of those must keep landing where it always did.
  if (app !== undefined && !isAppId(app)) return `app must be one of ${APPS.join(", ")}`;
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
    app: (app as (typeof APPS)[number] | undefined) ?? "manager",
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

/**
 * This window against the one before it.
 *
 * The only thing that tells growth from churn: without it, "1,486 active installs" reads the
 * same whether they are the same 1,486 as last month or 1,486 different people who each
 * tried it once.
 *
 * Null when the previous window reaches past `RETENTION_DAYS` and the sweep has eaten it,
 * which is a figure that cannot be computed rather than one that is zero.
 */
export interface Retention {
  /** Distinct installs active in the window. */
  recent: number;
  /** Distinct installs active in the window immediately before it. */
  prior: number;
  /** Installs active in both. */
  returning: number;
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
  /** Which app these numbers are about; "all" is both together. */
  app: AppFilter;
  /** How far back counters go. What "all time" actually means, and why retention can be null. */
  retentionDays: number;
  /**
   * Distinct installs active over each period. `day`, `week` and `month` are fixed periods and
   * do not move with the range picker; `window` is the one that does, and is the only honest
   * denominator for anything else here — every other figure on this page is window-scoped.
   */
  active: { day: number; week: number; month: number; window: number };
  installsEver: number;
  newInstalls: number;
  sessions: number;
  minutes: number;
  daily: DayRow[];
  /** Whether the same installs keep coming back, or new ones keep replacing them. */
  retention: Retention | null;
  /** The version each install last reported. One bucket each, so these sum to the window's actives. */
  currentVersions: Bucket[];
  platforms: Bucket[];
  games: Bucket[];
  events: EventRow[];
  /** Names this app is expected to report that have no rows in the window at all. */
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
/** The apps a read may be narrowed to, plus the two of them together. */
export type AppFilter = (typeof APPS)[number] | "all";

export function windowApp(url: URL): AppFilter {
  const asked = url.searchParams.get("app");
  return isAppId(asked) ? asked : "all";
}

export async function collectStats(
  env: Env,
  days: number,
  now = Date.now(),
  app: AppFilter = "all",
): Promise<Stats> {
  const today = dayKey(now);
  const from = dayKey(now, days - 1);
  const week = dayKey(now, 6);
  const month = dayKey(now, 29);

  /**
   * One predicate, spliced into every read: a figure drawn from a different slice than the
   * one beside it is worse than no filter at all.
   *
   * Written into the SQL rather than bound. `app` has already been through `isAppId`, so it
   * is one of two words from a closed list and there is nothing to inject; binding it would
   * mean threading a parameter through nine queries whose `?`s are positional and, in two
   * cases, sit inside a subquery that is read before the outer `WHERE` — which is exactly
   * the kind of silent off-by-one that returns a plausible wrong number.
   */
  const only = app === "all" ? "" : ` AND app = '${app}'`;
  const q = <T>(sql: string, ...binds: unknown[]) =>
    env.DB.prepare(sql.replaceAll("/*app*/", only))
      .bind(...binds)
      .all<T>();

  // The window immediately before this one, for retention. Only asked for when the whole of
  // it is still inside `RETENTION_DAYS` — see `MAX_WINDOW_DAYS`.
  const before = dayKey(now, days * 2 - 1);
  const canRetain = days * 2 <= RETENTION_DAYS;

  const [active, ever, fresh, totals, daily, retention, current, platforms, games, events] =
    await Promise.all([
      // `window` is quoted for the reason `returning` is below: both are SQLite keywords.
      // Scanned from whichever of the month and the window reaches further back, so one pass
      // still answers all four.
      q<{ day: number; week: number; month: number; window: number }>(
        "SELECT" +
          "  COUNT(DISTINCT CASE WHEN day = ?1 THEN install_id END) AS day," +
          "  COUNT(DISTINCT CASE WHEN day >= ?2 THEN install_id END) AS week," +
          "  COUNT(DISTINCT CASE WHEN day >= ?3 THEN install_id END) AS month," +
          "  COUNT(DISTINCT CASE WHEN day >= ?4 THEN install_id END) AS \"window\"" +
          " FROM usage_daily WHERE day >= MIN(?3, ?4)/*app*/",
        today,
        week,
        month,
        from,
      ),
      // Bounded by the retention window rather than left open. The figure was only ever "since
      // the beginning of time" by accident — the sweep happened to have deleted the rest — and
      // the tile that shows it promises a number of days. A sweep that fails for a week should
      // not quietly change what this means.
      q<{ n: number }>(
        "SELECT COUNT(DISTINCT install_id) AS n FROM usage_daily WHERE day >= ?/*app*/",
        dayKey(now, RETENTION_DAYS - 1),
      ),
      // An install is new in the window if the first day we ever saw it falls inside it.
      q<{ n: number }>(
        "SELECT COUNT(*) AS n FROM (" +
          " SELECT install_id, MIN(day) AS firstDay FROM usage_daily WHERE 1 = 1/*app*/ GROUP BY install_id" +
          ") WHERE firstDay >= ?",
        from,
      ),
      q<{ sessions: number; minutes: number }>(
        "SELECT COALESCE(SUM(sessions), 0) AS sessions, COALESCE(SUM(minutes), 0) AS minutes" +
          " FROM usage_daily WHERE day >= ?/*app*/",
        from,
      ),
      // COUNT(DISTINCT install_id), not COUNT(*): the row key is (install_id, app, day), so a
      // machine running two of the apps is two rows and was drawn as two installs — on the one
      // read, "all", whose whole promise is that it counts a machine once however many of them
      // reported. Every figure beside it on the page already counted distinctly.
      q<DayRow>(
        "SELECT day, COUNT(DISTINCT install_id) AS installs, COALESCE(SUM(sessions), 0) AS sessions," +
          " COALESCE(SUM(minutes), 0) AS minutes" +
          " FROM usage_daily WHERE day >= ?/*app*/ GROUP BY day ORDER BY day",
        from,
      ),
      // One pass over both windows: each install is reduced to "was it in this one" and "was
      // it in the one before", and the three counts fall out of the pair.
      canRetain
        ? q<Retention>(
            "SELECT" +
              "  COUNT(CASE WHEN recent THEN 1 END) AS recent," +
              "  COUNT(CASE WHEN prior THEN 1 END) AS prior," +
              "  COUNT(CASE WHEN recent AND prior THEN 1 END) AS \"returning\"" +
              " FROM (" +
              "  SELECT install_id, MAX(day >= ?1) AS recent, MAX(day >= ?2 AND day < ?1) AS prior" +
              "  FROM usage_daily WHERE day >= ?2/*app*/ GROUP BY install_id" +
              " )",
            from,
            before,
          )
        : Promise.resolve({ results: [] as Retention[] }),
      // What everyone is on now: each install contributes once, from its most recent day. A
      // plain GROUP BY version counts an install under every build it ran in the window,
      // which double-counts exactly the installs that updated — the ones you are asking about.
      q<Bucket>(
        "SELECT label, COUNT(*) AS installs FROM (" +
          " SELECT version AS label," +
          " ROW_NUMBER() OVER (PARTITION BY install_id ORDER BY day DESC) AS rn" +
          " FROM usage_daily WHERE day >= ?/*app*/" +
          ") WHERE rn = 1 GROUP BY label ORDER BY installs DESC, label DESC",
        from,
      ),
      q<Bucket>(
        "SELECT os AS label, COUNT(DISTINCT install_id) AS installs FROM usage_daily" +
          " WHERE day >= ?/*app*/ GROUP BY os ORDER BY installs DESC",
        from,
      ),
      q<Bucket>(
        "SELECT game AS label, COUNT(DISTINCT install_id) AS installs FROM usage_daily" +
          " WHERE day >= ?/*app*/ GROUP BY game ORDER BY installs DESC",
        from,
      ),
      q<EventRow>(
        "SELECT name, COUNT(DISTINCT install_id) AS reach, COALESCE(SUM(count), 0) AS volume" +
          " FROM usage_events WHERE day >= ?/*app*/ GROUP BY name ORDER BY reach DESC, volume DESC",
        from,
      ),
    ]);

  const rows = events.results ?? [];
  const seen = new Set(rows.map((r) => r.name));
  const counts = active.results?.[0] ?? { day: 0, week: 0, month: 0, window: 0 };

  return {
    generatedAt: now,
    days,
    app,
    retentionDays: RETENTION_DAYS,
    active: {
      day: counts.day ?? 0,
      week: counts.week ?? 0,
      month: counts.month ?? 0,
      window: counts.window ?? 0,
    },
    installsEver: ever.results?.[0]?.n ?? 0,
    newInstalls: fresh.results?.[0]?.n ?? 0,
    sessions: totals.results?.[0]?.sessions ?? 0,
    minutes: totals.results?.[0]?.minutes ?? 0,
    daily: daily.results ?? [],
    retention: retention.results?.[0] ?? null,
    currentVersions: current.results ?? [],
    platforms: platforms.results ?? [],
    games: games.results ?? [],
    events: rows,
    // Only the names this app could have sent. A studio-only read used to list the manager's
    // entire vocabulary as never touched, which buried the few that genuinely were.
    unused: knownFor(app).filter((name) => !seen.has(name)),
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

/** How many days a request asked for, clamped to a window the figures stay honest over. */
export function windowDays(url: URL): number {
  const asked = Number(url.searchParams.get("days") ?? "30");
  if (!Number.isFinite(asked)) return 30;
  return Math.min(MAX_WINDOW_DAYS, Math.max(1, Math.trunc(asked)));
}

/** `GET /v1/usage/stats` — the same numbers as the dashboard, for anything that scripts them. */
export async function usageStats(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return json(503, { error: "no admin key is configured" });
  if (allowed === "denied") return json(401, { error: "unauthorized" });
  return json(200, await collectStats(env, windowDays(url), Date.now(), windowApp(url)));
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
