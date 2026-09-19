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

import { lastSweepAt } from "./roster";
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

/**
 * Failures that are facts about the reporting machine, not observations of the master.
 *
 * These are dropped from the window entirely — not counted as failing, not counted as an
 * install — because an app that never got as far as asking has made no observation to fold in,
 * and leaving it in means our own bugs are published as somebody else's outage.
 *
 * That is not hypothetical. On 2026-09-18 this page said `down` with 162 of 164 apps failing,
 * and 152 of those were `auth` — a regression in *our* Steam ticket path, on a page whose
 * headline reads "MX Bikes' servers aren't answering". The master was serving 69 servers at
 * the time. `unsupported` was already excluded for exactly this reason; the mistake was
 * treating it as one odd case rather than as the rule it is.
 *
 * `auth` is here **provisionally**, and should come out. Its own definition is the problem —
 * the ticket "was refused, or couldn't be minted" — so one code covers both a master saying no
 * and our own Steam path failing, and a reason that ambiguous cannot be allowed to convict
 * anybody. Once shipped clients send `ticket` for the half they know is theirs, `auth` means
 * only the master saying no and belongs back in the count. Until then, treating it as local
 * costs us the ability to see a login-refusal outage, which is the rarer and less damaging
 * mistake of the two.
 */
export const LOCAL_REASONS: ReadonlySet<string> = new Set([
  "unsupported",
  "offline",
  "ticket",
  "auth",
]);

/**
 * The share of a window that can be dropped as local before the rest stops meaning anything.
 *
 * This is the guard that actually matters, and it is about sampling rather than about any one
 * reason. Dropping local failures is right, but what is left afterwards is not a random sample
 * of the population — it is the survivors of whatever took the others out. On 2026-09-18 a
 * regression in our own ticket path removed 153 of 164 reports, and the 11 that could still
 * ask were a self-selected remnant: 9 of them failed, which reads as 82% and would have been
 * published as "MX Bikes' servers aren't answering" on the strength of nine machines.
 *
 * So when most of a window is our own faults, the answer is `unknown` — which the page already
 * knows how to say, and which sends the reader to check their own machine instead of telling
 * them a falsehood about somebody else's. In a real outage local reasons are a small minority
 * and this never fires.
 */
export const UNSOUND_LOCAL_SHARE = 0.5;

/**
 * How recently a real master sweep has to have landed to contradict a `down` verdict.
 *
 * Every install that reads the master successfully offers the control plane its list, and the
 * store keeps the latest (`roster.lastSweepAt`). So a sweep inside this window is proof the
 * master answered somebody inside this window, and no count of failing apps can honestly be
 * called an outage over the top of it — the failures are then something the failing machines
 * have in common, which is the opposite of what `down` tells the reader.
 *
 * Five minutes. A healthy population rewrites that row every 60-90 s, so this is several missed
 * rounds rather than a tight bar, and the cost is the honest one: a genuine outage beginning
 * seconds after a good sweep reads `degraded` for its first few minutes before the row goes
 * stale and `down` is allowed. Note what this is *not* good for — when our own client is the
 * thing broken, sweeps are starved too (measured at one per 5-6 minutes on 2026-09-18, against
 * 60-90 s healthy), so this floor thins out in exactly the case it would be most wanted.
 * `UNSOUND_LOCAL_SHARE` is what covers that case; this covers a window that looks bad for
 * reasons we cannot name at all.
 */
export const SWEEP_FRESH_MS = 5 * 60 * 1000;

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
  /**
   * When an app last read the master successfully, or null if not inside `RETENTION_MS`.
   *
   * On the page because it is the one number here that is evidence rather than inference: a
   * reader who has been told for ten minutes that their internet is broken deserves to see
   * that somebody else's sweep came back two minutes ago.
   */
  lastSweep: number | null;
  /**
   * Reports dropped as facts about the reporting machine rather than about the master.
   *
   * Published because it is the number that says how much to trust the rest. A window where
   * this dwarfs `installs` is a window describing our own bug, and a reader — or whoever is
   * looking at this during the next incident — should be able to see that at a glance.
   */
  local: number;
}

export interface StatusBody {
  state: State;
  /**
   * The headline, in the same voice and from the same place as `summary`.
   *
   * Added because the four states are not four *answers*: `unknown` covers both "nobody is
   * awake at 3am" and "our own client is broken and the window means nothing", and a page
   * picking its title from the state alone will say the first while the second is true.
   * Clients that predate this field fall back to their own titles and are none the worse.
   */
  headline: string;
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
    status = {
      state: "unknown",
      installs: 0,
      failing: 0,
      share: null,
      reasons: [],
      failingForMinutes: null,
      lastSweep: null,
      local: 0,
    };
  }

  const body: StatusBody = {
    state: status.state,
    headline: headline(status),
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
  let local = 0;
  const reasons = new Map<ProbeReason, number>();
  for (const row of rows.results ?? []) {
    const ok = Number(row.ok);
    if (ok === 0 && LOCAL_REASONS.has(row.reason ?? "")) {
      local += 1;
      continue;
    }
    installs += 1;
    // A success anywhere in the install's latest minute is a working connection, whatever else
    // that minute holds: one fetch out of three getting through still proves the path is open.
    if (ok > 0) continue;
    failing += 1;
    const reason = isProbeReason(row.reason) ? row.reason : "error";
    reasons.set(reason, (reasons.get(reason) ?? 0) + 1);
  }

  // The sweep floor. Read unconditionally rather than only when the ratio says `down`, because
  // the page shows it either way and one indexed row is not worth branching over.
  const sweep = await lastSweep(now, env);
  const state = verdict({ installs, failing, local, sweptAt: sweep, now });
  return {
    state,
    installs,
    failing,
    share: installs === 0 ? null : Math.round((failing / installs) * 100) / 100,
    reasons: [...reasons]
      .map(([reason, count]): ReasonCount => ({ reason, installs: count }))
      .sort((a, b) => b.installs - a.installs || a.reason.localeCompare(b.reason)),
    lastSweep: sweep,
    local,
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

/**
 * The verdict: what the ratio says, floored by what we can prove.
 *
 * `stateFor` alone can only ever reason about failures, and a population of failures has two
 * explanations — the master is down, or the apps are broken. Everything here is about refusing
 * to publish the first when the evidence only supports the second, because `down` is printed on
 * a public page as an accusation about somebody else's service.
 *
 * Neither guard can talk a window all the way up to `up`: the failures are real and the people
 * hitting them still need the page to say something is wrong. They only stop it naming a party
 * the evidence does not reach.
 */
export function verdict({
  installs,
  failing,
  local,
  sweptAt,
  now,
}: {
  installs: number;
  failing: number;
  local: number;
  sweptAt: number | null;
  now: number;
}): State {
  const state = stateFor(installs, failing);
  if (state !== "down") return state;

  // Most of the window was our own faults, so what is left is a remnant rather than a sample.
  // `unknown` is the honest answer and the page already says it well.
  if (local > 0 && local / (local + installs) >= UNSOUND_LOCAL_SHARE) return "unknown";

  // A corroborated list came back from the master recently, so "isn't answering" is false.
  if (sweptAt !== null && now - sweptAt < SWEEP_FRESH_MS) return "degraded";

  return "down";
}

/**
 * The last confirmed master sweep, or null if there isn't one worth quoting.
 *
 * Bounded by `RETENTION_MS` for the same reason everything else here is: an hour-old sweep is
 * not evidence about now, and showing it beside a live verdict would invite reading it as one.
 * Never throws — the floor is a safety net, and a safety net that can take the endpoint down
 * with it is worse than not having it.
 */
async function lastSweep(now: number, env: Env): Promise<number | null> {
  try {
    const at = await lastSweepAt(env);
    return at !== null && now - at <= RETENTION_MS ? at : null;
  } catch (err) {
    console.error(JSON.stringify({ msg: "sweep read failed", error: String(err) }));
    return null;
  }
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
export function summarize(status: MasterStatus, now = Date.now()): string {
  const { state, installs, failing, local } = status;
  const others = installs - failing;

  // Floored by a recent sweep: the ratio said `down` and the evidence says otherwise. Worth its
  // own sentence rather than the ordinary degraded one, because "lots of apps are failing and
  // the master is demonstrably fine" is a different thing to be told than "it's patchy", and
  // the reader's next move is different too — there is no point waiting this one out.
  if (state === "degraded" && stateFor(installs, failing) === "down" && status.lastSweep !== null) {
    const mins = Math.max(1, Math.round((now - status.lastSweep) / 60_000));
    return (
      `Something is failing for a lot of people — ${failing} of the last ${installs} apps that ` +
      `tried — but MX Bikes' master server itself is answering: another app read the full server ` +
      `list from it ${mins} minute${mins === 1 ? "" : "s"} ago. So this is not an outage at ` +
      `PiBoSo's end, and it is worth checking whether your MXB App is up to date.`
    );
  }

  // Unknown because most of the window was our own faults, rather than because nobody was
  // awake. Both are "we can't say", and they send the reader somewhere completely different.
  if (state === "unknown" && local > 0 && local / (local + installs) >= UNSOUND_LOCAL_SHARE) {
    return (
      `We can't tell you anything useful about MX Bikes' servers right now: ${local} of the last ` +
      `${local + installs} apps that reported never got as far as asking them — they failed on ` +
      `this end first — so the handful that did aren't enough to judge by. That's a fault in the ` +
      `MXB App rather than in MX Bikes, and it's being worked on. Check you're on the latest ` +
      `version, and use the Servers tab's Check my connection for your own machine.`
    );
  }

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

/** The title over the sentence. Same rule as `summarize`, kept beside it so they cannot drift. */
export function headline(status: MasterStatus): string {
  const { state, installs, local } = status;
  if (state === "unknown" && local > 0 && local / (local + installs) >= UNSOUND_LOCAL_SHARE) {
    return "MXB App can't check right now — and that's ours, not MX Bikes'.";
  }
  switch (state) {
    case "down":
      return "MX Bikes' servers aren't answering.";
    case "degraded":
      return "MX Bikes' servers are answering some people and not others.";
    case "up":
      return "MX Bikes' servers are answering.";
    case "unknown":
      return "Not enough people have checked to say.";
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
