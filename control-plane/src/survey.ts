/**
 * Asking the player.
 *
 * The counters next door (`usage.ts`) answer what people open. They have never been able to
 * answer whether any of it is any good, and every attempt to read that off a counter is a
 * guess wearing a number's clothes: a feature with high reach is one people *find*. So this is
 * the other half — a question put to the player in the app, and what came back.
 *
 * ## What it is, and what it is not
 *
 * There is no user here either. An answer carries the same install id the counters use — a
 * random UUID the app minted for itself, tied to no account, no rider name and no machine —
 * plus the app, its version, the OS and the title. Everything else is a choice id from a list
 * this deployment wrote.
 *
 * With one exception, and it is worth naming plainly rather than burying: [`scrubNote`]. The
 * follow-up ("what happened?") offers an optional box the player may type into, and that text
 * is the only free-form thing in this whole deployment. It is opt-in per answer, capped at
 * [`MAX_NOTE_CHARS`], scrubbed of the identifiers that most often fall into one by accident,
 * deleted by the sweep [`NOTE_RETENTION_DAYS`] before the row it sits on goes, and deletable
 * one at a time from the dashboard. It is still free text. Anything that would rather not
 * carry it turns the box off for its poll (`note = 0`) and collects chips alone.
 *
 * ## Why the questions live in the database
 *
 * Because a question is worth asking for three weeks. "Have you tried Race mode" baked into a
 * client needs a release to ask and another to stop, and a month in between for either to
 * reach anybody — by which time the answer has stopped being interesting. A row can be written
 * in the morning and retired in the afternoon, and every install picks it up on its next fetch.
 *
 * The client never trusts what it reads back: [`parsePoll`] is what this side refuses a
 * malformed poll with, and the app validates the same shape again before it draws anything.
 */

import { ipDigest } from "./voice";
import {
  APPS,
  isAppId,
  isAppVersion,
  isChoiceId,
  isGameId,
  isInstallId,
  isPlatform,
  isPollId,
  MAX_NOTE_CHARS,
  MAX_REASONS_PER_ANSWER,
} from "./validate";
import { adminAllowed, dayKey, SIGNATURE_HEADER, signatureOk, type AppFilter } from "./usage";

/** An answer is a few hundred bytes; a note is capped well under a kilobyte. */
export const MAX_ANSWER_BYTES = 4 * 1024;

/** The content type an answer must arrive as — see `reportUsage` for why this is insisted on. */
const ANSWER_CONTENT_TYPE = "application/json";

/**
 * Answers accepted from one address per day.
 *
 * Far tighter than the usage cap, because the shape is different: an install posts a counter
 * report every half hour and an answer only when somebody taps one. A household or a LAN
 * shares an address, so this is still generous — it bounds a script, not a family.
 */
export const MAX_ANSWERS_PER_DAY = 50;

/** How long answers are kept. The same window the counters use, so the two can be read together. */
export const RETENTION_DAYS = 400;

/**
 * How long a note is kept.
 *
 * Much shorter than the answer it came with, and separately swept. The countable part of an
 * answer — the choice, the chips — is what a trend is drawn from a year later; the prose is
 * only useful while it is still about something current, and holding somebody's sentence for
 * four hundred days to no purpose is a thing to stop doing rather than to justify.
 */
export const NOTE_RETENTION_DAYS = 120;

/** The longest window a read may ask for, matching the counters'. */
export const MAX_WINDOW_DAYS = 365;

/** Notes shown on one read. A dashboard is for reading, not for exporting. */
export const MAX_NOTES = 200;

type AppId = (typeof APPS)[number];

/** Text in whatever languages it was written in, keyed by locale. `en` is the fallback. */
export type Localized = Record<string, string>;

/** One answer the player can tap, or one chip on the follow-up. */
export interface PollChoice {
  /** What travels back and what the dashboard groups on. Never reused for a different answer. */
  id: string;
  label: Localized;
}

export interface Poll {
  id: string;
  /**
   * `mood` is the standing "how is it going" question: the app draws its own three answers
   * (`bad`, `fine`, `good`) in the player's own language, so it reads properly in six of them
   * without a word being written here. `choice` is everything else, and speaks whatever
   * languages its `ask` and `choices` were written in.
   */
  kind: "mood" | "choice";
  apps: AppId[];
  ask: Localized;
  choices: PollChoice[];
  /** The follow-up's chips. Empty means the app's own built-in list, which ships translated. */
  reasons: PollChoice[];
  /** Choice ids that always get the follow-up. */
  followUp: string[];
  /** How often the follow-up comes after any other answer, 0–1. */
  followUpChance: number;
  /** Whether the optional free-text box is offered at all. */
  note: boolean;
  /** Days before the same install is asked again. 0 means once, ever. */
  againDays: number;
  live: boolean;
  startsAt: number | null;
  endsAt: number | null;
  /** Only ask builds at or past this. Empty asks everybody. */
  minVersion: string;
}

/** The mood poll's own answers. The app draws and translates these; it never invents others. */
export const MOOD_CHOICES = ["bad", "fine", "good"] as const;

/** The follow-up chips the app ships translated, for a poll that names none of its own. */
export const BUILT_IN_REASONS = ["crash", "slow", "confusing", "broken", "missing", "other"] as const;

interface PollRow {
  id: string;
  kind: string;
  apps: string;
  ask: string;
  choices: string;
  reasons: string;
  follow_up: string;
  follow_up_chance: number;
  note: number;
  again_days: number;
  live: number;
  starts_at: number | null;
  ends_at: number | null;
  min_version: string;
}

/** JSON that came out of a column, or the fallback when it is not what it should be. */
function fromJson<T>(raw: string, fallback: T): T {
  try {
    const parsed = JSON.parse(raw);
    return parsed === null || parsed === undefined ? fallback : (parsed as T);
  } catch {
    return fallback;
  }
}

/** A stored row as the wire shape. Tolerant: a column that has gone bad reads as its empty value. */
export function pollFromRow(row: PollRow): Poll {
  return {
    id: row.id,
    kind: row.kind === "mood" ? "mood" : "choice",
    apps: row.apps.split(",").map((a) => a.trim()).filter(isAppId),
    ask: fromJson<Localized>(row.ask, {}),
    choices: fromJson<PollChoice[]>(row.choices, []),
    reasons: fromJson<PollChoice[]>(row.reasons, []),
    followUp: fromJson<string[]>(row.follow_up, []),
    followUpChance: row.follow_up_chance,
    note: row.note === 1,
    againDays: row.again_days,
    live: row.live === 1,
    startsAt: row.starts_at,
    endsAt: row.ends_at,
    minVersion: row.min_version ?? "",
  };
}

/**
 * `GET /v1/survey/polls?app=manager` — the questions this app should be asking.
 *
 * Unauthenticated and carries no install id, deliberately: the app fetches the whole live set
 * and decides for itself which of them it has already answered. That keeps the decision — and
 * the record of what one install has been asked — on the machine it belongs to, and it makes
 * the response the same for everybody, so it can be cached rather than computed per install.
 *
 * `minVersion` is returned rather than applied. Filtering on it here would put the client's
 * version in the cache key for the sake of a comparison the client can do itself.
 */
export async function listPolls(url: URL, env: Env, now = Date.now()): Promise<Response> {
  const app = url.searchParams.get("app");
  const rows = await env.DB.prepare(
    "SELECT id, kind, apps, ask, choices, reasons, follow_up, follow_up_chance, note," +
      " again_days, live, starts_at, ends_at, min_version" +
      " FROM survey_polls WHERE live = 1 ORDER BY updated_at DESC",
  ).all<PollRow>();

  const polls = (rows.results ?? [])
    .map(pollFromRow)
    .filter((p) => (p.startsAt === null || p.startsAt <= now) && (p.endsAt === null || p.endsAt > now))
    .filter((p) => !isAppId(app) || p.apps.includes(app));

  const res = json(200, { polls });
  // Half an hour, matching the counters' flush: a question written now reaches a running app
  // within one interval, and an app that has just opened asks once rather than on every view.
  res.headers.set("Cache-Control", "public, max-age=1800");
  return res;
}

export interface Answer {
  installId: string;
  app: AppId;
  version: string;
  os: string;
  game: string;
  pollId: string;
  choice: string;
  reasons: string[];
  note: string | null;
}

/**
 * The identifiers that most often fall into a free-text box by accident.
 *
 * Not a promise that nothing identifying survives — no regular expression can make that
 * promise about a sentence somebody typed, and pretending otherwise is worse than being plain.
 * What it removes is the handful of things that turn up in a bug report *without the person
 * meaning to share them*: their address, a link they pasted, and the path to their own user
 * folder, which on Windows is their name.
 */
const SCRUBS: [RegExp, string][] = [
  [/[\w.+-]+@[\w-]+\.[\w.-]+/g, "[email]"],
  [/\b(?:https?:\/\/|www\.)\S+/gi, "[link]"],
  [/\b[A-Za-z]:\\[^\s"']*/g, "[path]"],
  [/\\\\[^\s"']+/g, "[path]"],
  [/\/(?:Users|home)\/[^\s"']*/gi, "[path]"],
];

/**
 * A note as it is stored: one short line, scrubbed, capped.
 *
 * Returns null for anything that is not worth a column — an empty box, or one holding nothing
 * but whitespace after the scrub.
 */
export function scrubNote(value: unknown): string | null {
  if (typeof value !== "string") return null;
  // Control characters and every kind of line break become spaces: a note is one sentence, and
  // a newline in a stored string is only ever a problem for whatever displays it next.
  // eslint-disable-next-line no-control-regex
  let text = value.replace(/[\x00-\x1f\x7f]+/g, " ");
  for (const [pattern, with_] of SCRUBS) text = text.replace(pattern, with_);
  text = text.replace(/\s+/g, " ").trim();
  if (text.length > MAX_NOTE_CHARS) text = `${text.slice(0, MAX_NOTE_CHARS - 1).trimEnd()}…`;
  return text.length > 0 ? text : null;
}

/**
 * Check an answer, returning the reason it was refused rather than a bare false.
 *
 * The choice and the chips are held to [`isChoiceId`] rather than to the poll they name. That
 * is deliberate: the poll can be edited or retired between an app fetching it and somebody
 * tapping it, and an answer that arrives a minute after its question was rewritten is a real
 * answer to the question that was on screen. The dashboard groups on whatever came back, and
 * an id nobody recognises is a far smaller problem than an answer silently dropped.
 */
export function parseAnswer(raw: string): Answer | string {
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "expected a JSON body";
  }
  if (!body || typeof body !== "object") return "expected a JSON body";
  const { installId, app, version, os, game, pollId, choice, reasons, note } = body as Record<
    string,
    unknown
  >;

  if (!isInstallId(installId)) return "installId must be a UUID";
  if (!isAppId(app)) return `app must be one of ${APPS.join(", ")}`;
  if (!isAppVersion(version)) return "version must be a semver string";
  if (!isPlatform(os)) return "os must be windows, macos or linux";
  if (!isGameId(game)) return "game must be mxb or gpb";
  if (!isPollId(pollId)) return "pollId must be a slug";
  if (!isChoiceId(choice)) return "choice must be a slug";

  const chips: string[] = [];
  if (reasons !== undefined) {
    if (!Array.isArray(reasons)) return "reasons must be an array";
    if (reasons.length > MAX_REASONS_PER_ANSWER) return "too many reasons in one answer";
    for (const reason of reasons) {
      if (!isChoiceId(reason)) return `not a reason id: ${String(reason).slice(0, 32)}`;
      // The same chip twice is a client bug and not worth refusing an answer over.
      if (!chips.includes(reason)) chips.push(reason);
    }
  }

  return {
    installId: installId as string,
    app: app as AppId,
    version: version as string,
    os: os as string,
    game: game as string,
    pollId: pollId as string,
    choice: choice as string,
    reasons: chips,
    note: scrubNote(note),
  };
}

/**
 * `POST /v1/survey` — one install's answer to one question.
 *
 * Unauthenticated, bounded and content-type-checked for exactly the reasons `POST /v1/usage`
 * is; see that endpoint for why a `text/plain` body would turn every visitor to a web page
 * into a reporter. An answer is an upsert on `(poll, install, day)`, so a client that retries
 * after a dropped connection replaces its own row rather than voting twice.
 */
export async function reportAnswer(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_ANSWER_BYTES) return json(413, { error: "answer too large" });

  const type = (request.headers.get("content-type") ?? "").split(";")[0].trim().toLowerCase();
  if (type !== ANSWER_CONTENT_TYPE) return json(415, { error: `expected ${ANSWER_CONTENT_TYPE}` });

  const raw = await readText(request);
  if (raw === null || raw.length > MAX_ANSWER_BYTES) return json(413, { error: "answer too large" });

  // The same build signature the counters carry, under the same deployment switch, checked by
  // the same function — see `signatureOk` for what it is and is not worth. One construction
  // rather than two: an answer and a report are both "this came from a build we shipped", and
  // two spellings of that would be one of them quietly not being checked.
  if (
    env.MXB_USAGE_REQUIRE_SIGNATURE === "1" &&
    !(await signatureOk(request.headers.get(SIGNATURE_HEADER), raw, env))
  ) {
    return json(401, { error: "unsigned answer" });
  }

  const answer = parseAnswer(raw);
  if (typeof answer === "string") return json(400, { error: answer });

  const now = Date.now();
  const day = dayKey(now);
  const digest = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);
  const seen = await env.DB.prepare(
    "SELECT claims FROM device_claims WHERE ip_digest = ? AND day = ? AND kind = 'survey'",
  )
    .bind(digest, day)
    .first<{ claims: number }>();
  if (seen && seen.claims >= MAX_ANSWERS_PER_DAY) {
    return json(429, { error: "too many answers from here today" });
  }

  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO survey_answers" +
        " (poll_id, install_id, app, day, version, os, game, choice, reasons, note, answered_at)" +
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)" +
        " ON CONFLICT(poll_id, install_id, day) DO UPDATE SET" +
        "  version = excluded.version, os = excluded.os, game = excluded.game," +
        "  choice = excluded.choice, reasons = excluded.reasons," +
        // A later send may only add a note, never blank one that is already there: the second
        // half of the follow-up arrives as its own request, and the first half carries no note.
        "  note = COALESCE(excluded.note, note)," +
        "  answered_at = excluded.answered_at",
    ).bind(
      answer.pollId,
      answer.installId,
      answer.app,
      day,
      answer.version,
      answer.os,
      answer.game,
      answer.choice,
      answer.reasons.join(","),
      answer.note,
      now,
    ),
    env.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
        " VALUES (?, ?, 'survey', 1, ?)" +
        " ON CONFLICT(ip_digest, day, kind) DO UPDATE SET" +
        "  claims = claims + 1, updated_at = excluded.updated_at",
    ).bind(digest, day, now),
  ]);

  return json(202, { ok: true });
}

export interface ChoiceCount {
  id: string;
  /** Rows. A poll that comes round again counts an install once per time it answered. */
  answers: number;
  /** Distinct installs. The "how many people" number. */
  installs: number;
}

export interface ReasonCount {
  id: string;
  answers: number;
}

export interface NoteRow {
  /** `poll|install|day` — what the dashboard's delete button sends back. */
  handle: string;
  day: string;
  app: string;
  version: string;
  os: string;
  game: string;
  choice: string;
  note: string;
  at: number;
}

export interface SurveyStats {
  generatedAt: number;
  days: number;
  app: AppFilter;
  retentionDays: number;
  noteRetentionDays: number;
  /** Which poll the figures below are about. */
  pollId: string;
  /** Every poll worth naming: the live ones, and any with answers inside the window. */
  polls: Poll[];
  answers: number;
  installs: number;
  choices: ChoiceCount[];
  /** One row per day per choice, for the trend. Zero days are absent rather than zero. */
  daily: { day: string; choice: string; answers: number }[];
  reasons: ReasonCount[];
  notes: NoteRow[];
  /**
   * −1 to 1 for the mood poll: how far the good answers outweigh the bad, ignoring "fine".
   * Null for any other poll, whose answers have no order to average.
   */
  score: number | null;
}

/** Which poll a read is about. */
export function windowPoll(url: URL): string {
  const asked = url.searchParams.get("poll");
  return isPollId(asked) ? asked : "mood";
}

/** How many days a read asked for, clamped to a window the figures stay honest over. */
export function windowDays(url: URL): number {
  const asked = Number(url.searchParams.get("days") ?? "30");
  if (!Number.isFinite(asked)) return 30;
  return Math.min(MAX_WINDOW_DAYS, Math.max(1, Math.trunc(asked)));
}

/**
 * Everything the dashboard shows about one question.
 *
 * Read-only, and every query is an aggregate over an indexed `(poll, day)` range, so the cost
 * follows the window rather than the history.
 */
export async function collectSurvey(
  env: Env,
  days: number,
  now = Date.now(),
  app: AppFilter = "all",
  pollId = "mood",
): Promise<SurveyStats> {
  const from = dayKey(now, days - 1);

  // Spliced rather than bound, exactly as `collectStats` does and for the same reason: `app`
  // has been through `isAppId`, so it is one word from a closed list, and threading a
  // parameter through queries whose other placeholders are positional is how a plausible
  // wrong number gets returned.
  const only = app === "all" ? "" : ` AND app = '${app}'`;
  const q = <T>(sql: string, ...binds: unknown[]) =>
    env.DB.prepare(sql.replaceAll("/*app*/", only))
      .bind(...binds)
      .all<T>();

  const [polls, counts, choices, daily, chips, notes] = await Promise.all([
    // The live set, plus anything answered in the window — a retired poll still has to be
    // nameable on the page showing what it collected.
    env.DB.prepare(
      "SELECT id, kind, apps, ask, choices, reasons, follow_up, follow_up_chance, note," +
        " again_days, live, starts_at, ends_at, min_version FROM survey_polls" +
        " WHERE live = 1 OR id IN (SELECT DISTINCT poll_id FROM survey_answers WHERE day >= ?)" +
        " ORDER BY live DESC, updated_at DESC",
    )
      .bind(from)
      .all<PollRow>(),
    // Distinct across every choice, which is not the sum of the per-choice figures: one
    // install answering "fine" in March and "good" in May is one person, twice.
    q<{ answers: number; installs: number }>(
      "SELECT COUNT(*) AS answers, COUNT(DISTINCT install_id) AS installs" +
        " FROM survey_answers WHERE poll_id = ?1 AND day >= ?2/*app*/",
      pollId,
      from,
    ),
    q<ChoiceCount>(
      "SELECT choice AS id, COUNT(*) AS answers, COUNT(DISTINCT install_id) AS installs" +
        " FROM survey_answers WHERE poll_id = ?1 AND day >= ?2/*app*/" +
        " GROUP BY choice ORDER BY answers DESC",
      pollId,
      from,
    ),
    q<{ day: string; choice: string; answers: number }>(
      "SELECT day, choice, COUNT(*) AS answers FROM survey_answers" +
        " WHERE poll_id = ?1 AND day >= ?2/*app*/ GROUP BY day, choice ORDER BY day",
      pollId,
      from,
    ),
    // Tallied here rather than in SQL: the chips are a joined string, and splitting a few
    // thousand of them in the worker beats a fourth table that is only ever counted.
    q<{ reasons: string }>(
      "SELECT reasons FROM survey_answers" +
        " WHERE poll_id = ?1 AND day >= ?2 AND reasons <> ''/*app*/",
      pollId,
      from,
    ),
    q<Omit<NoteRow, "handle"> & { install_id: string }>(
      "SELECT install_id, day, app, version, os, game, choice, note, answered_at AS at" +
        " FROM survey_answers WHERE poll_id = ?1 AND day >= ?2 AND note IS NOT NULL/*app*/" +
        " ORDER BY answered_at DESC LIMIT ?3",
      pollId,
      from,
      MAX_NOTES,
    ),
  ]);

  const counted = choices.results ?? [];
  const totals = counts.results?.[0] ?? { answers: 0, installs: 0 };
  const answers = totals.answers;
  const tally = new Map<string, number>();
  for (const row of chips.results ?? []) {
    for (const id of row.reasons.split(",")) {
      if (id) tally.set(id, (tally.get(id) ?? 0) + 1);
    }
  }

  const definitions = (polls.results ?? []).map(pollFromRow);
  const mood = definitions.find((p) => p.id === pollId)?.kind === "mood";
  const of = (id: string) => counted.find((c) => c.id === id)?.answers ?? 0;

  return {
    generatedAt: now,
    days,
    app,
    retentionDays: RETENTION_DAYS,
    noteRetentionDays: NOTE_RETENTION_DAYS,
    pollId,
    polls: definitions,
    answers,
    installs: totals.installs,
    choices: counted,
    daily: daily.results ?? [],
    reasons: [...tally.entries()]
      .map(([id, n]) => ({ id, answers: n }))
      .sort((a, b) => b.answers - a.answers || a.id.localeCompare(b.id)),
    notes: (notes.results ?? []).map((row) => ({
      handle: `${pollId}|${row.install_id}|${row.day}`,
      day: row.day,
      app: row.app,
      version: row.version,
      os: row.os,
      game: row.game,
      choice: row.choice,
      note: row.note,
      at: row.at,
    })),
    score: mood && answers > 0 ? (of("good") - of("bad")) / answers : null,
  };
}

/** `GET /v1/survey/stats` — the same figures the dashboard draws, for anything that scripts them. */
export async function surveyStats(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return json(503, { error: "no admin key is configured" });
  if (allowed === "denied") return json(401, { error: "unauthorized" });
  const app = url.searchParams.get("app");
  return json(
    200,
    await collectSurvey(env, windowDays(url), Date.now(), isAppId(app) ? app : "all", windowPoll(url)),
  );
}

/**
 * Check a poll as the dashboard submits it.
 *
 * Strict where the client is not: this is the one place a question is written, and a poll with
 * a choice id nobody can group on, or an `ask` nobody can read, is a poll that wastes every
 * answer it collects. Better to refuse it at the form than to find out three weeks later.
 */
export function parsePoll(body: unknown, now = Date.now()): Poll | string {
  if (!body || typeof body !== "object") return "expected a poll";
  const raw = body as Record<string, unknown>;

  if (!isPollId(raw.id)) return "id must be a slug: lower-case letters, digits and hyphens";
  const kind = raw.kind === "mood" ? "mood" : "choice";

  const apps = Array.isArray(raw.apps) ? raw.apps.filter(isAppId) : [];
  if (apps.length === 0) return `apps must name at least one of ${APPS.join(", ")}`;

  const ask = localized(raw.ask);
  if (typeof ask === "string") return `ask: ${ask}`;
  // The mood poll's wording ships in the app, translated; every other question has to say
  // something, and it has to say it in at least English.
  if (kind === "choice" && !ask.en) return "ask needs at least an English question";

  const choices = choiceList(raw.choices);
  if (typeof choices === "string") return `choices: ${choices}`;
  if (kind === "choice" && choices.length < 2) return "a question needs at least two answers";
  if (kind === "mood" && choices.length > 0) return "the mood poll draws its own answers";

  const reasons = choiceList(raw.reasons);
  if (typeof reasons === "string") return `reasons: ${reasons}`;

  const followUp = Array.isArray(raw.followUp) ? raw.followUp.filter(isChoiceId) : [];
  const known = kind === "mood" ? [...MOOD_CHOICES] : choices.map((c) => c.id);
  const stray = followUp.find((id) => !known.includes(id));
  if (stray) return `followUp names ${stray}, which is not one of the answers`;

  const chance = Number(raw.followUpChance ?? 0.25);
  if (!Number.isFinite(chance) || chance < 0 || chance > 1) return "followUpChance must be 0 to 1";

  const againDays = Math.trunc(Number(raw.againDays ?? 0));
  if (!Number.isFinite(againDays) || againDays < 0 || againDays > RETENTION_DAYS) {
    return `againDays must be 0 to ${RETENTION_DAYS}`;
  }

  const startsAt = when(raw.startsAt);
  const endsAt = when(raw.endsAt);
  if (typeof startsAt === "string") return `startsAt: ${startsAt}`;
  if (typeof endsAt === "string") return `endsAt: ${endsAt}`;
  if (startsAt !== null && endsAt !== null && endsAt <= startsAt) return "endsAt is before startsAt";
  if (endsAt !== null && endsAt <= now && raw.live !== false) {
    return "that window has already closed — retire the poll instead";
  }

  const minVersion = String(raw.minVersion ?? "").trim();
  if (minVersion && !isAppVersion(minVersion)) return "minVersion must be a semver string";

  return {
    id: raw.id,
    kind,
    apps,
    ask,
    choices,
    reasons,
    followUp,
    followUpChance: chance,
    note: raw.note !== false,
    againDays,
    live: raw.live !== false,
    startsAt,
    endsAt,
    minVersion,
  };
}

/** A locale map, or why it is not one. */
function localized(value: unknown): Localized | string {
  if (value === undefined || value === null) return {};
  if (typeof value !== "object" || Array.isArray(value)) return "expected a locale map";
  const out: Localized = {};
  for (const [locale, text] of Object.entries(value as Record<string, unknown>)) {
    if (!/^[a-z]{2}(-[A-Za-z]{2,4})?$/.test(locale)) return `not a locale: ${locale.slice(0, 16)}`;
    if (typeof text !== "string") return `${locale} must be a string`;
    const line = text.replace(/\s+/g, " ").trim();
    if (line.length === 0) continue;
    if (line.length > 160) return `${locale} is too long to fit in the prompt`;
    out[locale] = line;
  }
  return out;
}

/** A list of answers or chips, or why it is not one. */
function choiceList(value: unknown): PollChoice[] | string {
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value)) return "expected a list";
  if (value.length > 8) return "more than eight is a form, not a question";
  const out: PollChoice[] = [];
  for (const entry of value) {
    if (!entry || typeof entry !== "object") return "each entry needs an id and a label";
    const { id, label } = entry as Record<string, unknown>;
    if (!isChoiceId(id)) return `not an id: ${String(id).slice(0, 32)}`;
    if (out.some((c) => c.id === id)) return `${id} is in there twice`;
    const text = localized(label);
    if (typeof text === "string") return `${id}: ${text}`;
    if (!text.en) return `${id} needs at least an English label`;
    out.push({ id, label: text });
  }
  return out;
}

/** A unix-ms timestamp, null, or why it is neither. */
function when(value: unknown): number | null | string {
  if (value === undefined || value === null || value === "") return null;
  const at = typeof value === "string" ? Date.parse(value) : Number(value);
  if (!Number.isFinite(at)) return "expected a date";
  return Math.trunc(at);
}

/** Write a poll, creating it or replacing it. */
export async function savePoll(env: Env, poll: Poll, now = Date.now()): Promise<void> {
  await env.DB.prepare(
    "INSERT INTO survey_polls" +
      " (id, kind, apps, ask, choices, reasons, follow_up, follow_up_chance, note, again_days," +
      "  live, starts_at, ends_at, min_version, created_at, updated_at)" +
      " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)" +
      " ON CONFLICT(id) DO UPDATE SET" +
      "  kind = excluded.kind, apps = excluded.apps, ask = excluded.ask," +
      "  choices = excluded.choices, reasons = excluded.reasons, follow_up = excluded.follow_up," +
      "  follow_up_chance = excluded.follow_up_chance, note = excluded.note," +
      "  again_days = excluded.again_days, live = excluded.live, starts_at = excluded.starts_at," +
      "  ends_at = excluded.ends_at, min_version = excluded.min_version," +
      "  updated_at = excluded.updated_at",
  )
    .bind(
      poll.id,
      poll.kind,
      poll.apps.join(","),
      JSON.stringify(poll.ask),
      JSON.stringify(poll.choices),
      JSON.stringify(poll.reasons),
      JSON.stringify(poll.followUp),
      poll.followUpChance,
      poll.note ? 1 : 0,
      poll.againDays,
      poll.live ? 1 : 0,
      poll.startsAt,
      poll.endsAt,
      poll.minVersion,
      now,
      now,
    )
    .run();
}

/** Take a poll out of the field, or put it back. Answers are untouched either way. */
export async function setPollLive(env: Env, id: string, live: boolean, now = Date.now()): Promise<boolean> {
  const done = await env.DB.prepare(
    "UPDATE survey_polls SET live = ?, updated_at = ? WHERE id = ?",
  )
    .bind(live ? 1 : 0, now, id)
    .run();
  return (done.meta?.changes ?? 0) > 0;
}

/**
 * Delete a poll outright.
 *
 * Refused once anything has answered it: the answers outlive the question, and a dashboard
 * showing rows it can no longer name is worse than a retired poll sitting in a list.
 */
export async function deletePoll(env: Env, id: string): Promise<"ok" | "answered" | "missing"> {
  const answered = await env.DB.prepare("SELECT 1 AS n FROM survey_answers WHERE poll_id = ? LIMIT 1")
    .bind(id)
    .first<{ n: number }>();
  if (answered) return "answered";
  const done = await env.DB.prepare("DELETE FROM survey_polls WHERE id = ?").bind(id).run();
  return (done.meta?.changes ?? 0) > 0 ? "ok" : "missing";
}

/**
 * Drop one note, keeping the answer it came with.
 *
 * The button behind this is the reason the box can be offered at all: somebody who typed
 * something they would rather not have is one click away from it being gone, and the choice
 * they made stays counted.
 */
export async function clearNote(env: Env, handle: unknown): Promise<boolean> {
  if (typeof handle !== "string") return false;
  const [pollId, installId, day] = handle.split("|");
  if (!isPollId(pollId) || !isInstallId(installId) || !/^\d{4}-\d{2}-\d{2}$/.test(day ?? "")) {
    return false;
  }
  const done = await env.DB.prepare(
    "UPDATE survey_answers SET note = NULL WHERE poll_id = ? AND install_id = ? AND day = ?",
  )
    .bind(pollId, installId, day)
    .run();
  return (done.meta?.changes ?? 0) > 0;
}

/**
 * Drop old answers, and older notes.
 *
 * Runs on the same cron as the counters' sweep. The two passes are the point: the countable
 * part of an answer is worth a year, and the sentence somebody typed is not.
 */
export async function pruneSurvey(env: Env): Promise<void> {
  const now = Date.now();
  try {
    await env.DB.batch([
      env.DB.prepare("UPDATE survey_answers SET note = NULL WHERE note IS NOT NULL AND day < ?").bind(
        dayKey(now, NOTE_RETENTION_DAYS),
      ),
      env.DB.prepare("DELETE FROM survey_answers WHERE day < ?").bind(dayKey(now, RETENTION_DAYS)),
    ]);
  } catch (err) {
    console.error(JSON.stringify({ msg: "survey sweep failed", error: String(err) }));
  }
}

async function readText(request: Request): Promise<string | null> {
  try {
    return await request.text();
  } catch {
    return null;
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
