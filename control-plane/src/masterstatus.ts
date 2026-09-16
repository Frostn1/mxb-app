/**
 * Is MX Bikes' master server answering — and if not, is it everyone or is it you?
 *
 * ## The problem this is for
 *
 * When PiBoSo's master server goes quiet, the game says `connection timeout` and stops. That
 * string is all a player gets, and it is indistinguishable from a broken router, a firewall
 * rule, a bad DNS server and a dozen other things that genuinely are theirs to fix. So every
 * outage arrives in the Discord as a handful of people each debugging their own machine, and
 * the standing answer — a troubleshooting list — sends them to reinstall a game that is
 * working perfectly. Ten minutes later the master comes back and nobody ever learns that was
 * the whole story.
 *
 * ## Why the answer is crowd-sourced
 *
 * The obvious design is for this Worker to check the master itself, on a timer, and publish
 * what it found. It cannot. The master speaks its own protocol over UDP; Workers have no
 * datagram socket, and the protocol is not in the public tree to ship into one anyway. A TCP
 * probe of the port would answer a different question and answer it wrong.
 *
 * So the check is the apps. Every install already talks to the master each time somebody
 * opens the Servers tab, and each one reports whether its own fetch worked. That turns out to
 * be the *better* signal rather than a substitute for the real one, because a single
 * observer — us, from one Cloudflare colo — could never have answered the question that is
 * actually being asked. "Is it up for a server in Frankfurt" is not what anyone wants to
 * know. "Are the other twenty people who tried in the last ten minutes also failing" is.
 *
 * ## What leaves a machine, and what doesn't
 *
 * An install id, a minute, worked-or-didn't, and one word from a closed list saying why not.
 * That is all of it. No address, no rider name, no server, no game path. The install id is
 * the same anonymous UUID the usage counters use, and it is here only so that one person
 * hammering Refresh counts once rather than twenty times — the whole measurement is a count
 * of *people*, so the alternative is a single frustrated player being able to declare an
 * outage on their own.
 *
 * Reporting rides on the app's existing anonymous-stats setting: an install with it switched
 * off contributes nothing. Reading does not — `GET /v1/status` is public and unauthenticated,
 * so somebody who shares none of their own data still gets told it isn't their firewall.
 * That asymmetry is deliberate: the reason to withhold a report is privacy, and the reason to
 * read the answer is that your game is broken.
 */

import { ipDigest } from "./voice";
import { isInstallId, isProbeReason, type ProbeReason } from "./validate";

/** A probe is under 200 bytes. Anything near this is not one. */
export const MAX_PROBE_BYTES = 2 * 1024;

/**
 * Probes accepted from one address a day.
 *
 * The app reports at most once a minute and only when somebody is actually looking at the
 * Servers tab, so a heavy day for one machine is a few dozen. A household or a LAN shares an
 * address. Set far above both: turning away a real player's observation during an outage is
 * the one failure mode that matters here, because an outage is exactly when everybody reports
 * at once.
 */
export const MAX_PROBES_PER_DAY = 1000;

/** The window the answer is computed over. */
export const WINDOW_MS = 10 * 60 * 1000;

/**
 * Rows older than this are swept.
 *
 * Far longer than `WINDOW_MS` on purpose: the window answers "right now", and the hour behind
 * it is what lets the page say an outage has been going on for a while rather than re-reporting
 * the same instant. Nothing here is worth keeping past that — it is a liveness signal, not a
 * history, and there is no question anyone wants to ask of yesterday's copy of it.
 */
export const RETENTION_MS = 60 * 60 * 1000;

/**
 * Distinct installs the window needs before it will call anything.
 *
 * Below this the honest answer is `unknown`, and saying so is the entire value of the number:
 * at 3am there may be two people awake, and "1 of 2 apps failed" is not an outage, it is one
 * person with a bad wifi connection. A status page that guesses in the quiet hours is a status
 * page nobody believes in the loud ones.
 */
export const MIN_INSTALLS = 4;

/** At or above this share of installs failing, it is not you. */
export const DOWN_SHARE = 0.8;

/** At or above this, something is wrong for enough people to say so without claiming an outage. */
export const DEGRADED_SHARE = 0.35;

export type State = "up" | "degraded" | "down" | "unknown";

export interface ReasonCount {
  reason: ProbeReason;
  /** Distinct installs whose last failure was this. */
  installs: number;
}

export interface MasterStatus {
  state: State;
  /** Distinct installs that reported at all in the window. */
  installs: number;
  /** Distinct installs that tried and never once succeeded in the window. */
  failing: number;
  /** `failing / installs`, rounded to two places, or null when there is nothing to divide. */
  share: number | null;
  /** Why they failed, commonest first. Empty when nothing is failing. */
  reasons: ReasonCount[];
  /**
   * How long it has been failing, in minutes — measured from the first minute we actually saw
   * fail, never from the last one that succeeded, because a gap where nobody reported is a gap
   * where nobody looked and inventing minutes into it would overstate every outage. Reaches
   * back as far as `RETENTION_MS` and no further, so it is a floor rather than a duration.
   * Null unless the state is `down`; see where it is filled in.
   */
  failingForMinutes: number | null;
}

export interface StatusBody {
  state: State;
  /**
   * One sentence, already written, for whoever is reading. The page renders it and a Discord
   * bot can post it verbatim — the point of composing it here rather than in each client is
   * that there is then exactly one wording of the answer, and it can be fixed in one place.
   */
  summary: string;
  master: MasterStatus;
  windowMinutes: number;
  checkedAt: number;
  /** How this is known. On the page, because a status page that won't say is worth nothing. */
  method: string;
}

interface Probe {
  installId: string;
  ok: boolean;
  reason: ProbeReason | null;
}

/**
 * `POST /v1/master-status` — one install saying whether it could reach the master.
 *
 * Unauthenticated, like the usage counters and for the same reason: most people who run the
 * app have never claimed an invite, and an outage signal that only enrolled accounts could
 * contribute to would be an outage signal from almost nobody. Everything that could be abused
 * is bounded instead — the body, the shape, and the probes one address may send in a day —
 * and the upsert means the worst a single determined client can do to the ratio is count once.
 */
export async function reportMasterProbe(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_PROBE_BYTES) return json(413, { error: "probe too large" });

  const raw = await readText(request);
  if (raw === null || raw.length > MAX_PROBE_BYTES) return json(413, { error: "probe too large" });

  const probe = parseProbe(raw);
  if (typeof probe === "string") return json(400, { error: probe });

  const now = Date.now();
  const day = new Date(now).toISOString().slice(0, 10);
  const digest = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);
  const seen = await env.DB.prepare(
    "SELECT claims FROM device_claims WHERE ip_digest = ? AND day = ? AND kind = 'master'",
  )
    .bind(digest, day)
    .first<{ claims: number }>();
  if (seen && seen.claims >= MAX_PROBES_PER_DAY) {
    return json(429, { error: "too many probes from here today" });
  }

  const minute = Math.floor(now / 60_000) * 60_000;
  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO master_probes (install_id, minute, ok, failed, reason, updated_at)" +
        " VALUES (?, ?, ?, ?, ?, ?)" +
        " ON CONFLICT(install_id, minute) DO UPDATE SET" +
        "  ok = ok + excluded.ok, failed = failed + excluded.failed," +
        // The last failure wins, and a success never clears it: a minute that failed and then
        // recovered still has something to say about why it failed.
        "  reason = CASE WHEN excluded.reason = '' THEN reason ELSE excluded.reason END," +
        "  updated_at = excluded.updated_at",
    ).bind(
      probe.installId,
      minute,
      probe.ok ? 1 : 0,
      probe.ok ? 0 : 1,
      probe.ok ? "" : (probe.reason ?? "error"),
      now,
    ),
    env.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
        " VALUES (?, ?, 'master', 1, ?)" +
        " ON CONFLICT(ip_digest, day, kind) DO UPDATE SET" +
        "  claims = claims + 1, updated_at = excluded.updated_at",
    ).bind(digest, day, now),
  ]);

  return json(202, { ok: true });
}

/** Check a probe, returning the reason it was refused rather than a bare false. */
export function parseProbe(raw: string): Probe | string {
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "expected a JSON body";
  }
  if (!body || typeof body !== "object") return "expected a JSON body";
  const { installId, ok, reason } = body as Record<string, unknown>;

  if (!isInstallId(installId)) return "installId must be a UUID";
  if (typeof ok !== "boolean") return "ok must be a boolean";
  if (ok) return { installId, ok: true, reason: null };
  if (reason !== undefined && !isProbeReason(reason)) {
    return `not a probe reason: ${String(reason).slice(0, 32)}`;
  }
  return { installId, ok: false, reason: (reason as ProbeReason | undefined) ?? "error" };
}

/**
 * `GET /v1/status` — what everyone's apps are seeing, right now.
 *
 * Public, CORS-open and cacheable by anything. It carries no per-caller data at all, it is the
 * endpoint a Discord bot answering `!timeout` reads, and the moment it is worth reading is the
 * moment a few hundred people all want it at once.
 */
export async function masterStatus(env: Env): Promise<Response> {
  const now = Date.now();
  let status: MasterStatus;
  try {
    status = await readWindow(now, env);
  } catch (err) {
    console.error(JSON.stringify({ msg: "status read failed", error: String(err) }));
    status = { state: "unknown", installs: 0, failing: 0, share: null, reasons: [], failingForMinutes: null };
  }

  const body: StatusBody = {
    state: status.state,
    summary: summarize(status),
    master: status,
    windowMinutes: WINDOW_MS / 60_000,
    checkedAt: now,
    method:
      "Not a probe of our own: MX Bikes' master server speaks UDP, which this service cannot. " +
      "Every MXB App reports whether its own server-list fetch worked, and this is what they agree on.",
  };

  return new Response(JSON.stringify(body), {
    status: 200,
    headers: {
      "content-type": "application/json",
      // Long enough to absorb everyone refreshing at once, short enough that a recovery shows
      // up while people are still looking. The window is ten minutes; half a minute of cache
      // is invisible against it.
      "cache-control": "public, max-age=30",
      // Deliberately open. There is nothing here belonging to anyone, and the whole point is
      // that a Discord bot, a server owner's own page or a community site can read it.
      "access-control-allow-origin": "*",
    },
  });
}

/**
 * Fold the window into one answer.
 *
 * Each install counts once, by its **most recent** minute in the window and nothing else. The
 * first draft of this asked whether an install had ever succeeded in the window, and that was
 * wrong in the one case the whole feature exists for: for the first ten minutes of an outage
 * every affected install has a success behind it — it was working right up until the master
 * stopped — so the status would have read "up" through exactly the window everybody is in the
 * channel asking about. Latest-wins gets both directions right. Failed at 15:02 and fetched
 * fine at 15:04 is a machine watching the master come back, and is healthy. Fetched fine at
 * 15:00 and failing since 15:05 is a machine watching it go down, and is not.
 *
 * `unsupported` installs — builds with no server browser in them — are dropped rather than
 * counted either way. They report a fact about the build, not an observation of the master,
 * and leaving them in would peg the ratio high for ever on every install that cannot ask.
 */
async function readWindow(now: number, env: Env): Promise<MasterStatus> {
  const since = now - WINDOW_MS;
  const rows = await env.DB.prepare(
    "SELECT p.install_id, p.ok AS ok, p.failed AS failed, p.reason AS reason" +
      " FROM master_probes p" +
      " JOIN (SELECT install_id, MAX(minute) AS minute FROM master_probes" +
      "       WHERE minute > ? GROUP BY install_id) latest" +
      "  ON latest.install_id = p.install_id AND latest.minute = p.minute",
  )
    .bind(since)
    .all<{ install_id: string; ok: number; failed: number; reason: string | null }>();

  let installs = 0;
  let failing = 0;
  const reasons = new Map<ProbeReason, number>();
  for (const row of rows.results ?? []) {
    const ok = Number(row.ok);
    if (row.reason === "unsupported" && ok === 0) continue;
    installs += 1;
    // A success anywhere in the install's latest minute is a working connection, whatever else
    // that minute holds: one fetch out of three getting through still proves the path is open.
    if (ok > 0) continue;
    failing += 1;
    const reason = isProbeReason(row.reason) ? row.reason : "error";
    reasons.set(reason, (reasons.get(reason) ?? 0) + 1);
  }

  const state = stateFor(installs, failing);
  return {
    state,
    installs,
    failing,
    share: installs === 0 ? null : Math.round((failing / installs) * 100) / 100,
    reasons: [...reasons]
      .map(([reason, count]): ReasonCount => ({ reason, installs: count }))
      .sort((a, b) => b.installs - a.installs || a.reason.localeCompare(b.reason)),
    // Only for a full outage. "How long has it been partly broken" has no honest answer from
    // this data — a degraded window has successes in every minute by definition, which is the
    // exact thing `failingSince` stops at — so it is left null rather than answered with a 1.
    failingForMinutes: state === "down" ? await failingSince(now, env) : null,
  };
}

/**
 * How long it has looked like this, in whole minutes.
 *
 * Walks back a minute at a time from now and stops at the first minute that had a success in
 * it, so an outage that started twenty minutes ago reads as twenty rather than as the ten the
 * window can see. Capped by `RETENTION_MS`, and the caller says "at least" for that reason.
 * A minute nobody reported in is not a minute of recovery — it is a minute nobody looked —
 * and has no row here at all, so it never breaks the run.
 */
async function failingSince(now: number, env: Env): Promise<number | null> {
  const rows = await env.DB.prepare(
    "SELECT minute, SUM(ok) AS ok FROM master_probes WHERE minute > ?" +
      " GROUP BY minute ORDER BY minute DESC",
  )
    .bind(now - RETENTION_MS)
    .all<{ minute: number; ok: number }>();

  const minutes = rows.results ?? [];
  if (minutes.length === 0) return null;
  let oldest: number | null = null;
  for (const row of minutes) {
    if (Number(row.ok) > 0) break;
    oldest = Number(row.minute);
  }
  if (oldest === null) return null;
  return Math.max(1, Math.round((now - oldest) / 60_000));
}

/** Where a count of failing installs out of a count of installs lands. */
export function stateFor(installs: number, failing: number): State {
  if (installs < MIN_INSTALLS) return "unknown";
  const share = failing / installs;
  if (share >= DOWN_SHARE) return "down";
  if (share >= DEGRADED_SHARE) return "degraded";
  return "up";
}

/**
 * The sentence.
 *
 * Written here, once, rather than in the page and the app and whatever reads the JSON next,
 * because the wording is the feature. Somebody arrives at this having been told for ten
 * minutes that their internet is broken, so it says what is happening, how many people it is
 * happening to, and — the part the gist never had — whether it is theirs to fix.
 */
export function summarize(status: MasterStatus): string {
  const { state, installs, failing } = status;
  const others = installs - failing;
  switch (state) {
    case "down":
      return (
        `MX Bikes' own master server isn't answering. ${failing} of the last ${installs} apps that ` +
        `tried failed to load the server list — this isn't your connection, and there's nothing to ` +
        `fix at your end. It usually comes back within a few minutes.`
      );
    case "degraded":
      return (
        `MX Bikes' master server is answering some people and not others: ${failing} of the last ` +
        `${installs} apps that tried failed, ${others} got through. Worth trying again in a minute ` +
        `before you go looking at your own setup.`
      );
    case "up":
      return (
        `MX Bikes' master server is answering. ${others} of the last ${installs} apps that tried ` +
        `loaded the server list fine, so a timeout right now is likely to be something at your end.`
      );
    case "unknown":
      return (
        `Not enough apps have checked in the last ${WINDOW_MS / 60_000} minutes to say. It takes ` +
        `${MIN_INSTALLS} before this is worth trusting, and in the quiet hours there may not be ` +
        `that many people online.`
      );
  }
}

/**
 * Drop what the window has finished with.
 *
 * Runs on the same five-minute cron as every other sweep. Failing quietly on purpose: a sweep
 * that didn't happen is the next sweep's problem, and it must never be the reason a cron run
 * that also reaps idle servers gives up.
 */
export async function pruneMasterProbes(env: Env): Promise<void> {
  try {
    await env.DB.prepare("DELETE FROM master_probes WHERE minute < ?")
      .bind(Date.now() - RETENTION_MS)
      .run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "master probe sweep failed", error: String(err) }));
  }
}

async function readText(request: Request): Promise<string | null> {
  try {
    return await request.text();
  } catch {
    return null;
  }
}

// index.ts has its own copy; duplicating four lines beats importing the entry point back into
// a module it imports.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
