/**
 * Public series for mxbsecure.com/series.
 *
 * A series is scored in MSM (the MXB Servers desktop app) from mxbserver weekends and official
 * servers' live timing. MSM publishes the result here and the site reads it back. Three surfaces:
 *
 *  - publish / unpublish (`PUT`/`DELETE /v1/series/{slug}`) and the registration list, behind a
 *    per-series publish token. An admin reserves the slug on mxbsecure.com/admin, which mints the
 *    token and shows it once; only the SHA-256 digest is stored, like account and rating tokens.
 *    A token is scoped to its one series, so a leaked token can rewrite one standings page and
 *    nothing else (not a managed server's rating token, which can feed the global ratings).
 *  - public reads (`GET /v1/series`, `GET /v1/series/{slug}`), CORS-open and cacheable: they carry
 *    display names, points and places only.
 *  - registration (`POST /v1/series/{slug}/register`), the one anonymous write. There is no
 *    Turnstile on the site, so it is held by a per-address limiter, a per-address daily cap kept in
 *    D1 as a keyed hash, and a honeypot field. Entries start pending; only approved ones are public.
 *
 * Nothing that identifies a player beyond the name they race under is accepted: a publish carrying
 * anything shaped like a GUID or Steam ID, or a field named like one, is refused whole rather than
 * scrubbed, so a misbehaving publisher finds out instead of leaking quietly.
 */

import { bearer, hashToken, newToken } from "./auth";
import { isBanned } from "./bans";

export interface Result {
  status: number;
  body: unknown;
}

const SITE = "https://mxbsecure.com";
export const SLUG_RE = /^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$/;
const MAX_BODY_BYTES = 1_000_000;
const MAX_ROUNDS = 100;
const MAX_RESULTS = 200;
const MAX_STANDINGS = 500;
const MAX_CLASSES = 20;
const KEEP_SNAPSHOTS = 10;
/** Registrations one address may send per day, across every series. */
const REGISTRATIONS_PER_DAY = 10;
const DAY_MS = 24 * 60 * 60 * 1000;

export const MMR_NOTE =
  "MMR is matched to riders by display name, so it can be wrong when names are shared or changed.";

const STATUSES = new Set(["finished", "dnf", "dsq", "dns"]);
const REG_STATUSES = new Set(["pending", "approved", "rejected"]);

// ---------------------------------------------------------------------------------------------
// Identifier screening.
// ---------------------------------------------------------------------------------------------

const FORBIDDEN_KEYS = new Set(["guid", "steam_id", "steamid", "key", "identity", "server_id", "serverid"]);
const STEAM_ID = /7656119\d{10}/;
const HEX_RUN = /[0-9a-f]{16,}/i;
const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;

/** True for a string that looks like a Steam ID, a GUID or a UUID anywhere inside it. */
export function looksLikeIdentifier(value: string): boolean {
  return STEAM_ID.test(value) || HEX_RUN.test(value) || UUID.test(value);
}

/** The first identifier-shaped thing in a JSON value, as a path for the error, or null. */
export function findIdentifier(value: unknown, path = "body", depth = 0): string | null {
  if (depth > 8) return `${path} (nested too deeply)`;
  if (typeof value === "string") return looksLikeIdentifier(value) ? path : null;
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i++) {
      const hit = findIdentifier(value[i], `${path}[${i}]`, depth + 1);
      if (hit) return hit;
    }
    return null;
  }
  if (value && typeof value === "object") {
    for (const [k, v] of Object.entries(value)) {
      if (FORBIDDEN_KEYS.has(k.toLowerCase())) return `${path}.${k}`;
      const hit = findIdentifier(v, `${path}.${k}`, depth + 1);
      if (hit) return hit;
    }
  }
  return null;
}

// ---------------------------------------------------------------------------------------------
// Parsing the publish body.
// ---------------------------------------------------------------------------------------------

type Raw = Record<string, unknown>;
const isObj = (v: unknown): v is Raw => typeof v === "object" && v !== null && !Array.isArray(v);
const text = (v: unknown, max: number): string | null => {
  if (typeof v !== "string") return null;
  const t = v.trim();
  return t.length <= max ? t : null;
};
const int = (v: unknown, min: number, max: number): number | null =>
  typeof v === "number" && Number.isInteger(v) && v >= min && v <= max ? v : null;
const optInt = (v: unknown, min: number, max: number): number | null | undefined =>
  v === null || v === undefined ? null : int(v, min, max) ?? undefined;
const className = (v: unknown): string | null => {
  const t = text(v ?? "", 32);
  return t === null ? null : t.toUpperCase();
};

interface ResultRow {
  place: number | null;
  name: string;
  class: string;
  points: number;
  status: string;
  laps: number;
  bestLapMs: number | null;
}

interface RoundRow {
  round: number;
  label: string;
  track: string;
  startedAt: number | null;
  results: ResultRow[];
}

interface StandingRow {
  position: number;
  name: string;
  class: string;
  points: number;
  grossPoints: number;
  wins: number;
  roundsRidden: number;
  rounds: { round: number; place: number | null; points: number; dropped: boolean }[];
}

interface NextRound {
  label: string | null;
  track: string | null;
  startsAt: number | null;
}

export interface Publish {
  name: string;
  classes: string[];
  pointsTable: number[];
  dropWorst: number;
  registrationOpen: boolean;
  nextRound: NextRound | null;
  rounds: RoundRow[];
  standings: StandingRow[];
}

const POINTS_MAX = 100_000;

function parseResult(v: unknown): ResultRow | string {
  if (!isObj(v)) return "each result must be an object";
  const name = text(v.name, 64);
  if (!name) return "each result needs a name of at most 64 characters";
  const place = optInt(v.place, 1, 1000);
  if (place === undefined) return "result place must be a whole number or null";
  const cls = className(v.class);
  if (cls === null) return "result class is too long";
  const points = int(v.points ?? 0, 0, POINTS_MAX);
  if (points === null) return "result points must be a whole number";
  const status = typeof v.status === "string" ? v.status : "finished";
  if (!STATUSES.has(status)) return "result status must be finished, dnf, dsq or dns";
  const laps = int(v.laps ?? 0, 0, 10_000);
  if (laps === null) return "result laps must be a whole number";
  const best = optInt(v.best_lap_ms, 1, 3_600_000);
  if (best === undefined) return "best_lap_ms must be milliseconds or null";
  return { place, name, class: cls, points, status, laps, bestLapMs: best };
}

function parseStanding(v: unknown): StandingRow | string {
  if (!isObj(v)) return "each standing must be an object";
  const name = text(v.name, 64);
  if (!name) return "each standing needs a name of at most 64 characters";
  const position = int(v.position, 1, 10_000);
  const points = int(v.points, 0, POINTS_MAX * MAX_ROUNDS);
  const gross = int(v.gross_points ?? v.points, 0, POINTS_MAX * MAX_ROUNDS);
  const wins = int(v.wins ?? 0, 0, MAX_ROUNDS);
  const ridden = int(v.rounds_ridden ?? 0, 0, MAX_ROUNDS);
  const cls = className(v.class);
  if (position === null || points === null || gross === null || wins === null || ridden === null || cls === null) {
    return "standing position, points, gross_points, wins and rounds_ridden must be whole numbers";
  }
  const cellsRaw = v.rounds ?? [];
  if (!Array.isArray(cellsRaw) || cellsRaw.length > MAX_ROUNDS) return "standing rounds must be a list";
  const rounds: StandingRow["rounds"] = [];
  for (const c of cellsRaw) {
    if (!isObj(c)) return "each standing round must be an object";
    const round = int(c.round, 1, MAX_ROUNDS);
    const place = optInt(c.place, 1, 1000);
    const cellPoints = int(c.points ?? 0, 0, POINTS_MAX);
    if (round === null || place === undefined || cellPoints === null) return "standing round cells need round, place and points";
    rounds.push({ round, place, points: cellPoints, dropped: c.dropped === true });
  }
  return { position, name, class: cls, points, grossPoints: gross, wins, roundsRidden: ridden, rounds };
}

export function parsePublish(body: unknown): Publish | { error: string } {
  if (!isObj(body)) return { error: "expected a JSON object" };
  if (body.v !== 1) return { error: "unsupported or missing payload version (expected v: 1)" };
  const hit = findIdentifier(body);
  if (hit) return { error: `${hit} looks like a GUID, Steam ID or rider key; publish display names only` };

  const name = text(body.name, 64);
  if (!name) return { error: "name is required (at most 64 characters)" };

  const classesRaw = body.classes ?? [];
  if (!Array.isArray(classesRaw) || classesRaw.length > MAX_CLASSES) return { error: `classes must be a list of at most ${MAX_CLASSES}` };
  const classes: string[] = [];
  for (const c of classesRaw) {
    const cls = className(c);
    if (!cls) return { error: "each class must be a non-empty name of at most 32 characters" };
    if (!classes.includes(cls)) classes.push(cls);
  }

  const tableRaw = body.points_table ?? [];
  if (!Array.isArray(tableRaw) || tableRaw.length > 200) return { error: "points_table must be a list" };
  const pointsTable: number[] = [];
  for (const p of tableRaw) {
    const n = int(p, 0, POINTS_MAX);
    if (n === null) return { error: "points_table entries must be whole numbers" };
    pointsTable.push(n);
  }
  const dropWorst = int(body.drop_worst ?? 0, 0, MAX_ROUNDS);
  if (dropWorst === null) return { error: "drop_worst must be a whole number" };

  let nextRound: NextRound | null = null;
  if (body.next_round !== undefined && body.next_round !== null) {
    if (!isObj(body.next_round)) return { error: "next_round must be an object or null" };
    const n = body.next_round;
    const label = n.label === undefined || n.label === null ? null : text(n.label, 64);
    const track = n.track === undefined || n.track === null ? null : text(n.track, 120);
    const startsAt = optInt(n.starts_at, 0, 32_503_680_000);
    if (label === undefined || track === undefined || startsAt === undefined) return { error: "next_round fields are invalid" };
    if (n.label != null && label === null) return { error: "next_round label is too long" };
    if (n.track != null && track === null) return { error: "next_round track is too long" };
    nextRound = label || track || startsAt ? { label: label || null, track: track || null, startsAt } : null;
  }

  const roundsRaw = body.rounds ?? [];
  if (!Array.isArray(roundsRaw) || roundsRaw.length > MAX_ROUNDS) return { error: `rounds must be a list of at most ${MAX_ROUNDS}` };
  const rounds: RoundRow[] = [];
  for (const r of roundsRaw) {
    if (!isObj(r)) return { error: "each round must be an object" };
    const round = int(r.round, 1, MAX_ROUNDS);
    if (round === null) return { error: "each round needs a round number" };
    if (rounds.some((x) => x.round === round)) return { error: `round ${round} appears twice` };
    const label = text(r.label ?? "", 64);
    const track = text(r.track ?? "", 120);
    if (label === null || track === null) return { error: "round label or track is too long" };
    const startedAt = optInt(r.started_unix, 0, 32_503_680_000);
    if (startedAt === undefined) return { error: "started_unix must be unix seconds or null" };
    if (!Array.isArray(r.results) || r.results.length > MAX_RESULTS) return { error: `round results must be a list of at most ${MAX_RESULTS}` };
    const results: ResultRow[] = [];
    for (const x of r.results) {
      const parsed = parseResult(x);
      if (typeof parsed === "string") return { error: `round ${round}: ${parsed}` };
      results.push(parsed);
    }
    rounds.push({ round, label, track, startedAt, results });
  }
  rounds.sort((a, b) => a.round - b.round);

  const standingsRaw = body.standings ?? [];
  if (!Array.isArray(standingsRaw) || standingsRaw.length > MAX_STANDINGS) return { error: `standings must be a list of at most ${MAX_STANDINGS}` };
  const standings: StandingRow[] = [];
  for (const s of standingsRaw) {
    const parsed = parseStanding(s);
    if (typeof parsed === "string") return { error: parsed };
    standings.push(parsed);
  }
  standings.sort((a, b) => a.position - b.position);

  return {
    name,
    classes,
    pointsTable,
    dropWorst,
    registrationOpen: body.registration_open !== false,
    nextRound,
    rounds,
    standings,
  };
}

// ---------------------------------------------------------------------------------------------
// Tokens.
// ---------------------------------------------------------------------------------------------

/** The series a bearer token publishes, if it is that slug's current token. */
async function authorised(request: Request, env: Env, slug: string): Promise<Result | null> {
  const token = bearer(request.headers.get("Authorization"));
  if (!token) return { status: 401, body: { error: "a series publish token is required" } };
  const row = await env.DB.prepare("SELECT slug, token_hash FROM series WHERE slug = ?")
    .bind(slug)
    .first<{ slug: string; token_hash: string | null }>();
  if (!row) return { status: 404, body: { error: "no series with that slug; reserve it on mxbsecure.com/admin first" } };
  if (!row.token_hash || row.token_hash !== (await hashToken(token))) {
    return { status: 401, body: { error: "that token does not publish this series" } };
  }
  return null;
}

// ---------------------------------------------------------------------------------------------
// Admin (mxbsecure.com/admin, Steam-session gated in `webadmin.ts`).
// ---------------------------------------------------------------------------------------------

export async function adminListSeries(env: Env): Promise<Result> {
  const rows = await env.DB.prepare(
    `SELECT s.slug, s.name, s.published, s.token_issued_at, s.updated_at,
            (SELECT COUNT(*) FROM series_registrations r WHERE r.series_slug = s.slug AND r.status = 'pending') AS pending,
            (SELECT COUNT(*) FROM series_registrations r WHERE r.series_slug = s.slug AND r.status = 'approved') AS approved
       FROM series s ORDER BY s.created_at DESC`,
  ).all<{ slug: string; name: string; published: number; token_issued_at: number | null; updated_at: number | null; pending: number; approved: number }>();
  return {
    status: 200,
    body: {
      series: rows.results.map((r) => ({
        slug: r.slug,
        name: r.name,
        published: r.published === 1,
        tokenIssuedAt: r.token_issued_at,
        updatedAt: r.updated_at,
        pending: r.pending,
        approved: r.approved,
      })),
    },
  };
}

async function mint(env: Env, slug: string): Promise<{ token: string; at: number }> {
  const token = newToken();
  const at = Date.now();
  await env.DB.prepare("UPDATE series SET token_hash = ?, token_issued_at = ? WHERE slug = ?")
    .bind(await hashToken(token), at, slug)
    .run();
  return { token, at };
}

/** One admin action: create (reserve + first token), rotate, unpublish or delete. */
export async function adminSeriesAction(env: Env, body: Raw): Promise<Result> {
  const action = String(body.action ?? "");
  const slug = String(body.slug ?? "").trim().toLowerCase();
  if (!SLUG_RE.test(slug)) return { status: 400, body: { error: "a slug is 1-48 lowercase letters, digits and dashes" } };
  const exists = await env.DB.prepare("SELECT slug FROM series WHERE slug = ?").bind(slug).first();

  switch (action) {
    case "create": {
      const name = text(body.name, 64);
      if (!name) return { status: 400, body: { error: "a series needs a name of at most 64 characters" } };
      if (exists) return { status: 409, body: { error: "that slug is already taken" } };
      await env.DB.prepare("INSERT INTO series (slug, name, created_at) VALUES (?, ?, ?)").bind(slug, name, Date.now()).run();
      const { token, at } = await mint(env, slug);
      return { status: 200, body: { ok: true, slug, token, issuedAt: at } };
    }
    case "rotate": {
      if (!exists) return { status: 404, body: { error: "no series with that slug" } };
      const { token, at } = await mint(env, slug);
      return { status: 200, body: { ok: true, slug, token, issuedAt: at } };
    }
    case "unpublish": {
      if (!exists) return { status: 404, body: { error: "no series with that slug" } };
      await unpublishRows(env, slug);
      return { status: 200, body: { ok: true } };
    }
    case "delete": {
      if (!exists) return { status: 404, body: { error: "no series with that slug" } };
      await env.DB.batch([
        env.DB.prepare("DELETE FROM series_registrations WHERE series_slug = ?").bind(slug),
        env.DB.prepare("DELETE FROM series_rounds WHERE series_slug = ?").bind(slug),
        env.DB.prepare("DELETE FROM series_standings WHERE series_slug = ?").bind(slug),
        env.DB.prepare("DELETE FROM series WHERE slug = ?").bind(slug),
      ]);
      return { status: 200, body: { ok: true } };
    }
    default:
      return { status: 400, body: { error: "action must be create, rotate, unpublish or delete" } };
  }
}

async function unpublishRows(env: Env, slug: string): Promise<void> {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM series_rounds WHERE series_slug = ?").bind(slug),
    env.DB.prepare("DELETE FROM series_standings WHERE series_slug = ?").bind(slug),
    env.DB.prepare("UPDATE series SET published = 0, updated_at = ? WHERE slug = ?").bind(Date.now(), slug),
  ]);
}

// ---------------------------------------------------------------------------------------------
// Publish / unpublish (series token).
// ---------------------------------------------------------------------------------------------

export async function publishSeries(request: Request, env: Env, slug: string): Promise<Result> {
  const denied = await authorised(request, env, slug);
  if (denied) return denied;

  const raw = await request.text();
  if (raw.length > MAX_BODY_BYTES) return { status: 413, body: { error: "that series is too large to publish" } };
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return { status: 400, body: { error: "expected a JSON body" } };
  }
  const parsed = parsePublish(body);
  if ("error" in parsed) return { status: 400, body: { error: parsed.error } };

  const now = Date.now();
  const writes = [
    env.DB.prepare(
      `UPDATE series SET name = ?, published = 1, classes = ?, points_table = ?, drop_worst = ?,
              registration_open = ?, next_round = ?, updated_at = ? WHERE slug = ?`,
    ).bind(
      parsed.name,
      JSON.stringify(parsed.classes),
      JSON.stringify(parsed.pointsTable),
      parsed.dropWorst,
      parsed.registrationOpen ? 1 : 0,
      parsed.nextRound ? JSON.stringify(parsed.nextRound) : null,
      now,
      slug,
    ),
    env.DB.prepare("DELETE FROM series_rounds WHERE series_slug = ?").bind(slug),
    ...parsed.rounds.map((r) =>
      env.DB.prepare(
        "INSERT INTO series_rounds (series_slug, round_no, label, track, started_at, results) VALUES (?, ?, ?, ?, ?, ?)",
      ).bind(slug, r.round, r.label, r.track, r.startedAt, JSON.stringify(r.results)),
    ),
    env.DB.prepare("INSERT INTO series_standings (series_slug, standings, created_at) VALUES (?, ?, ?)").bind(
      slug,
      JSON.stringify(parsed.standings),
      now,
    ),
    env.DB.prepare(
      `DELETE FROM series_standings WHERE series_slug = ? AND id NOT IN
         (SELECT id FROM series_standings WHERE series_slug = ? ORDER BY id DESC LIMIT ${KEEP_SNAPSHOTS})`,
    ).bind(slug, slug),
  ];
  await env.DB.batch(writes);
  return { status: 200, body: { ok: true, slug, url: `${SITE}/series/${slug}`, updatedAt: now } };
}

export async function unpublishSeries(request: Request, env: Env, slug: string): Promise<Result> {
  const denied = await authorised(request, env, slug);
  if (denied) return denied;
  await unpublishRows(env, slug);
  return { status: 200, body: { ok: true } };
}

// ---------------------------------------------------------------------------------------------
// Public reads.
// ---------------------------------------------------------------------------------------------

interface SeriesRow {
  slug: string;
  name: string;
  classes: string;
  points_table: string;
  drop_worst: number;
  registration_open: number;
  next_round: string | null;
  updated_at: number | null;
}

function json<T>(raw: string | null, fallback: T): T {
  if (!raw) return fallback;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

async function latestStandings(env: Env, slug: string): Promise<StandingRow[]> {
  const row = await env.DB.prepare(
    "SELECT standings FROM series_standings WHERE series_slug = ? ORDER BY id DESC LIMIT 1",
  )
    .bind(slug)
    .first<{ standings: string }>();
  return json<StandingRow[]>(row?.standings ?? null, []);
}

export async function listPublished(env: Env): Promise<Result> {
  const rows = await env.DB.prepare(
    `SELECT s.slug, s.name, s.classes, s.points_table, s.drop_worst, s.registration_open, s.next_round, s.updated_at,
            (SELECT COUNT(*) FROM series_rounds r WHERE r.series_slug = s.slug) AS rounds,
            (SELECT standings FROM series_standings st WHERE st.series_slug = s.slug ORDER BY st.id DESC LIMIT 1) AS standings
       FROM series s WHERE s.published = 1 ORDER BY s.updated_at DESC`,
  ).all<SeriesRow & { rounds: number; standings: string | null }>();
  return {
    status: 200,
    body: {
      series: rows.results.map((r) => {
        const standings = json<StandingRow[]>(r.standings, []);
        const leader = standings[0];
        return {
          slug: r.slug,
          name: r.name,
          classes: json<string[]>(r.classes, []),
          rounds: r.rounds,
          riders: standings.length,
          updatedAt: r.updated_at,
          nextRound: json<NextRound | null>(r.next_round, null),
          registrationOpen: r.registration_open === 1,
          leader: leader ? { name: leader.name, points: leader.points } : null,
        };
      }),
    },
  };
}

interface Mmr {
  rating: number;
  races: number;
  class: string;
}

/**
 * Each standing rider's rating in their class, matched by display name. Ratings are keyed on the
 * GUID and the series has only names, so this is a best guess: a name shared by two rated GUIDs
 * in a class matches nobody, and a banned rider never matches (the public leaderboard hides them,
 * so this does too).
 */
export async function mmrByName(
  env: Env,
  riders: { name: string; class: string }[],
  seriesClasses: string[],
): Promise<Map<string, Mmr | null>> {
  const out = new Map<string, Mmr | null>();
  const classFor = (cls: string) => cls || (seriesClasses.length === 1 ? seriesClasses[0] : "");
  const wanted = [...new Set(riders.map((r) => classFor(r.class)).filter(Boolean))];
  if (wanted.length === 0) return out;

  const rows = await env.DB.prepare(
    `SELECT r.guid AS guid, r.class AS class, r.rating AS rating, r.races AS races,
            (SELECT name FROM race_results WHERE guid = r.guid AND counted = 1 ORDER BY id DESC LIMIT 1) AS name
       FROM rider_ratings r
      WHERE r.class IN (${wanted.map(() => "?").join(", ")})`,
  )
    .bind(...wanted)
    .all<{ guid: string; class: string; rating: number; races: number; name: string | null }>();

  // class -> lowercased name -> candidates
  const index = new Map<string, Map<string, { guid: string; rating: number; races: number }[]>>();
  for (const row of rows.results) {
    if (!row.name) continue;
    const byName = index.get(row.class) ?? new Map();
    index.set(row.class, byName);
    const k = row.name.trim().toLowerCase();
    byName.set(k, [...(byName.get(k) ?? []), { guid: row.guid, rating: row.rating, races: row.races }]);
  }

  for (const rider of riders) {
    const cls = classFor(rider.class);
    const key = `${rider.class}\u0000${rider.name.trim().toLowerCase()}`;
    if (out.has(key)) continue;
    const candidates = cls ? index.get(cls)?.get(rider.name.trim().toLowerCase()) ?? [] : [];
    const only = candidates.length === 1 ? candidates[0] : null;
    out.set(
      key,
      only && !(await isBanned(env, { guid: only.guid }))
        ? { rating: Math.round(only.rating), races: only.races, class: cls }
        : null,
    );
  }
  return out;
}

export async function readSeries(env: Env, slug: string): Promise<Result> {
  if (!SLUG_RE.test(slug)) return { status: 404, body: { error: "no such series" } };
  const s = await env.DB.prepare(
    `SELECT slug, name, classes, points_table, drop_worst, registration_open, next_round, updated_at
       FROM series WHERE slug = ? AND published = 1`,
  )
    .bind(slug)
    .first<SeriesRow>();
  if (!s) return { status: 404, body: { error: "no such series" } };

  const classes = json<string[]>(s.classes, []);
  const [roundRows, standings, entries] = await Promise.all([
    env.DB.prepare(
      "SELECT round_no, label, track, started_at, results FROM series_rounds WHERE series_slug = ? ORDER BY round_no",
    )
      .bind(slug)
      .all<{ round_no: number; label: string; track: string; started_at: number | null; results: string }>(),
    latestStandings(env, slug),
    env.DB.prepare(
      `SELECT rider_name, race_number, class, team FROM series_registrations
        WHERE series_slug = ? AND status = 'approved' ORDER BY race_number, name_key`,
    )
      .bind(slug)
      .all<{ rider_name: string; race_number: number; class: string; team: string | null }>(),
  ]);

  const mmr = await mmrByName(env, standings, classes);
  return {
    status: 200,
    body: {
      slug: s.slug,
      name: s.name,
      classes,
      pointsTable: json<number[]>(s.points_table, []),
      dropWorst: s.drop_worst,
      updatedAt: s.updated_at,
      registrationOpen: s.registration_open === 1,
      nextRound: json<NextRound | null>(s.next_round, null),
      rounds: roundRows.results.map((r) => ({
        round: r.round_no,
        label: r.label,
        track: r.track,
        startedAt: r.started_at,
        results: json<ResultRow[]>(r.results, []),
      })),
      standings: standings.map((row) => ({
        ...row,
        mmr: mmr.get(`${row.class}\u0000${row.name.trim().toLowerCase()}`) ?? null,
      })),
      entries: entries.results.map((e) => ({ name: e.rider_name, number: e.race_number, class: e.class, team: e.team })),
      mmrNote: MMR_NOTE,
    },
  };
}

// ---------------------------------------------------------------------------------------------
// Registration.
// ---------------------------------------------------------------------------------------------

async function addressHash(env: Env, ip: string): Promise<string> {
  // Keyed when the deployment has a secret to key it with, so a copy of the table cannot be
  // walked back to addresses by hashing the IPv4 space.
  const secret = env.MXB_DEVICE_SALT || env.MXB_WEB_SESSION_KEY || "";
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(`series-register:${secret}`),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(ip));
  return [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("").slice(0, 32);
}

export async function register(request: Request, env: Env, slug: string): Promise<Result> {
  const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
  if (env.REGISTER_LIMITER && !(await env.REGISTER_LIMITER.limit({ key: ip })).success) {
    return { status: 429, body: { error: "Too many tries from your connection. Wait a minute and try again." } };
  }
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return { status: 400, body: { error: "expected a JSON body" } };
  }
  if (!isObj(body)) return { status: 400, body: { error: "expected a JSON object" } };

  const s = await env.DB.prepare("SELECT classes, registration_open FROM series WHERE slug = ? AND published = 1")
    .bind(slug)
    .first<{ classes: string; registration_open: number }>();
  if (!s) return { status: 404, body: { error: "no such series" } };

  // The honeypot: a field people never see. Answered as a success so a bot learns nothing.
  if (typeof body.website === "string" && body.website.trim() !== "") {
    return { status: 201, body: { ok: true, status: "pending" } };
  }
  if (s.registration_open !== 1) return { status: 403, body: { error: "Registration for this series is closed." } };

  const name = text(body.name, 32);
  if (!name) return { status: 400, body: { error: "Enter your in-game name (32 characters at most)." } };
  if (looksLikeIdentifier(name)) return { status: 400, body: { error: "Enter the name you race under, not an ID." } };
  const number = int(typeof body.number === "string" ? Number(body.number) : body.number, 0, 999);
  if (number === null) return { status: 400, body: { error: "Race number must be a whole number from 0 to 999." } };
  const classes = json<string[]>(s.classes, []);
  const cls = className(body.class) ?? "";
  if (classes.length > 0 && !classes.includes(cls)) return { status: 400, body: { error: "Pick one of the series' classes." } };
  const team = body.team == null || body.team === "" ? null : text(body.team, 40);
  if (team === null && body.team != null && body.team !== "") return { status: 400, body: { error: "Team name is too long (40 characters at most)." } };
  const discord = body.discord == null || body.discord === "" ? null : text(body.discord, 40);
  if (discord === null && body.discord != null && body.discord !== "") return { status: 400, body: { error: "Discord name is too long (40 characters at most)." } };
  if ((team && looksLikeIdentifier(team)) || (discord && looksLikeIdentifier(discord))) {
    return { status: 400, body: { error: "Team and Discord should be names, not IDs." } };
  }

  const ipHash = await addressHash(env, ip);
  const recent = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM series_registrations WHERE ip_hash = ? AND created_at > ?",
  )
    .bind(ipHash, Date.now() - DAY_MS)
    .first<{ n: number }>();
  if ((recent?.n ?? 0) >= REGISTRATIONS_PER_DAY) {
    return { status: 429, body: { error: "Too many registrations from your connection today." } };
  }

  try {
    await env.DB.prepare(
      `INSERT INTO series_registrations
         (id, series_slug, rider_name, name_key, race_number, class, team, discord, status, ip_hash, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)`,
    )
      .bind(crypto.randomUUID(), slug, name, name.toLowerCase(), number, cls, team || null, discord || null, ipHash, Date.now())
      .run();
  } catch (err) {
    if (String(err).includes("UNIQUE")) return { status: 409, body: { error: "That name is already registered for this series." } };
    throw err;
  }
  return { status: 201, body: { ok: true, status: "pending" } };
}

export async function listRegistrations(request: Request, env: Env, slug: string): Promise<Result> {
  const denied = await authorised(request, env, slug);
  if (denied) return denied;
  const rows = await env.DB.prepare(
    `SELECT id, rider_name, race_number, class, team, discord, status, created_at, decided_at
       FROM series_registrations WHERE series_slug = ? ORDER BY created_at`,
  )
    .bind(slug)
    .all<{ id: string; rider_name: string; race_number: number; class: string; team: string | null; discord: string | null; status: string; created_at: number; decided_at: number | null }>();
  return {
    status: 200,
    body: {
      registrations: rows.results.map((r) => ({
        id: r.id,
        name: r.rider_name,
        number: r.race_number,
        class: r.class,
        team: r.team,
        discord: r.discord,
        status: r.status,
        createdAt: r.created_at,
        decidedAt: r.decided_at,
      })),
    },
  };
}

export async function decideRegistration(request: Request, env: Env, slug: string, id: string): Promise<Result> {
  const denied = await authorised(request, env, slug);
  if (denied) return denied;
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return { status: 400, body: { error: "expected a JSON body" } };
  }
  const status = isObj(body) ? String(body.status ?? "") : "";
  if (!REG_STATUSES.has(status)) return { status: 400, body: { error: "status must be approved, rejected or pending" } };
  const res = await env.DB.prepare(
    "UPDATE series_registrations SET status = ?, decided_at = ? WHERE id = ? AND series_slug = ?",
  )
    .bind(status, status === "pending" ? null : Date.now(), id, slug)
    .run();
  if (!res.meta.changes) return { status: 404, body: { error: "no such registration" } };
  return { status: 200, body: { ok: true, id, status } };
}

/** Forget the address hashes once the daily cap no longer needs them. */
export async function pruneSeriesRegistrations(env: Env, now = Date.now()): Promise<void> {
  await env.DB.prepare("UPDATE series_registrations SET ip_hash = NULL WHERE ip_hash IS NOT NULL AND created_at < ?")
    .bind(now - 2 * DAY_MS)
    .run();
}

// ---------------------------------------------------------------------------------------------
// Routing.
// ---------------------------------------------------------------------------------------------

export function isSeriesPath(path: string): boolean {
  return path === "/v1/series" || path.startsWith("/v1/series/");
}

function respond(status: number, body: unknown, cache = false): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
      // Bearer tokens only, never cookies, so any origin may read and write.
      "access-control-allow-origin": "*",
      "cache-control": cache && status === 200 ? "public, max-age=60" : "no-store",
    },
  });
}

export async function seriesRoutes(request: Request, url: URL, env: Env): Promise<Response> {
  const method = request.method;
  const path = url.pathname;

  if (method === "OPTIONS") {
    return new Response(null, {
      status: 204,
      headers: {
        "access-control-allow-origin": "*",
        "access-control-allow-methods": "GET, PUT, POST, DELETE, OPTIONS",
        "access-control-allow-headers": "Authorization, Content-Type",
        "access-control-max-age": "600",
      },
    });
  }

  if (path === "/v1/series") {
    if (method === "GET") {
      const r = await listPublished(env);
      return respond(r.status, r.body, true);
    }
    return respond(405, { error: "method not allowed" });
  }

  const m = /^\/v1\/series\/([^/]+)(?:\/(register|registrations)(?:\/([0-9a-f-]{36}))?)?$/.exec(path);
  if (!m) return respond(404, { error: "not found" });
  const slug = decodeURIComponent(m[1]);
  if (!SLUG_RE.test(slug)) return respond(404, { error: "no such series" });
  const [, , sub, id] = m;

  let r: Result;
  if (!sub && method === "GET") {
    r = await readSeries(env, slug);
    return respond(r.status, r.body, true);
  } else if (!sub && method === "PUT") r = await publishSeries(request, env, slug);
  else if (!sub && method === "DELETE") r = await unpublishSeries(request, env, slug);
  else if (sub === "register" && !id && method === "POST") r = await register(request, env, slug);
  else if (sub === "registrations" && !id && method === "GET") r = await listRegistrations(request, env, slug);
  else if (sub === "registrations" && id && method === "POST") r = await decideRegistration(request, env, slug, id);
  else r = { status: 405, body: { error: "method not allowed" } };
  return respond(r.status, r.body);
}
