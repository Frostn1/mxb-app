/**
 * Rider rating, Phase 1.
 *
 * Every one of Sean's managed servers (`managed_servers`, 0046/0047) is rated — that's the
 * decision, not a flag to check — so the only trust boundary here is the per-server rating
 * bearer token (0048), separate from the admin token that drives mxbserver's own admin API.
 *
 * Ingest -> filter -> Glicko-2, in that order, and every step is deliberately strict:
 *  - unknown/unrecognised token: the push is rejected outright, nothing is stored.
 *  - malformed or unversioned payload: rejected, nothing is stored (`v` must be `1`).
 *  - a race with fewer than 4 human (non-bot) finishers: stored raw, never rated — kills 1v1
 *    farming per the design without losing the audit trail.
 *  - a rider who completed under 50% of the race's laps: excluded from that race's rating
 *    pairwise comparisons, but the raw row is kept.
 *  - bots/spectators: excluded everywhere, never raters and never ratees.
 *  - idempotent by (server, event, race) id: a re-push of the same race is a no-op, not a
 *    double count.
 */

import { hashToken, newToken, bearer } from "./auth";
import { isBanned } from "./bans";
import { applyRace, DEFAULT_GLICKO, type Glicko } from "./glicko2";

/** A class name as the game reports it. Free-ish text, but bounded and never empty. */
function normalizeClass(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const cleaned = value.trim().toUpperCase();
  if (!cleaned || cleaned.length > 32) return null;
  return cleaned;
}

function isGuidLike(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9._:-]{4,100}$/.test(value.trim());
}

const MIN_HUMAN_RIDERS = 4;
const MIN_LAP_FRACTION = 0.5;
const GLICKO_TAU = 0.5;

// ---------------------------------------------------------------------------------------------
// Admin: issue/rotate a managed server's rating token.
// ---------------------------------------------------------------------------------------------

export interface RatingTokenResult {
  status: number;
  body: unknown;
}

/**
 * Mints a fresh rating token for a managed server, admin-only, shown once — same rule as
 * account tokens (`auth.ts`): only the SHA-256 digest is ever stored. Rotating simply
 * overwrites the digest, so the previous token stops working the instant a new one is
 * issued; there is deliberately no way to read a token back.
 */
export async function issueRatingToken(env: Env, serverId: string): Promise<RatingTokenResult> {
  const exists = await env.DB.prepare("SELECT id FROM managed_servers WHERE id = ?")
    .bind(serverId)
    .first<{ id: string }>();
  if (!exists) return { status: 404, body: { error: "no managed server with that id" } };

  const token = newToken();
  const hash = await hashToken(token);
  const now = Date.now();
  await env.DB.prepare(
    "UPDATE managed_servers SET rating_token_hash = ?, rating_token_issued_at = ? WHERE id = ?",
  )
    .bind(hash, now, serverId)
    .run();
  return { status: 200, body: { ok: true, serverId, token, issuedAt: now } };
}

async function serverForRatingToken(env: Env, token: string): Promise<{ id: string } | null> {
  const hash = await hashToken(token);
  return env.DB.prepare("SELECT id FROM managed_servers WHERE rating_token_hash = ?")
    .bind(hash)
    .first<{ id: string }>();
}

// ---------------------------------------------------------------------------------------------
// Ingest.
// ---------------------------------------------------------------------------------------------

interface PushedRider {
  guid: string;
  name: string;
  raceNum: number | null;
  class: string;
  position: number | null;
  classified: boolean;
  dnf: boolean;
  dsq: boolean;
  lapsCompleted: number;
  raceLaps: number;
  isBot: boolean;
}

interface PushedRace {
  serverId: string;
  eventId: string;
  raceId: string;
  track: string | null;
  class: string;
  session: string;
  timestamp: number;
  riders: PushedRider[];
}

/** Strict, versioned parsing. Any shape that isn't exactly `v: 1` and this field set is rejected. */
function parsePush(body: unknown): PushedRace | { error: string } {
  if (typeof body !== "object" || body === null) return { error: "expected a JSON object" };
  const b = body as Record<string, unknown>;
  if (b.v !== 1) return { error: "unsupported or missing payload version (expected v: 1)" };

  const serverId = typeof b.serverId === "string" ? b.serverId.trim() : "";
  const eventId = typeof b.eventId === "string" ? b.eventId.trim() : "";
  const raceId = typeof b.raceId === "string" ? b.raceId.trim() : "";
  const cls = normalizeClass(b.class);
  const session = typeof b.session === "string" ? b.session.trim() : "";
  const timestamp = Number(b.timestamp);
  const track = b.track === undefined || b.track === null ? null : typeof b.track === "string" ? b.track.trim().slice(0, 200) : null;

  if (!serverId || !eventId || !raceId || !cls || !session || !Number.isFinite(timestamp)) {
    return { error: "missing or invalid race fields" };
  }
  if (!Array.isArray(b.riders) || b.riders.length === 0) {
    return { error: "riders must be a non-empty array" };
  }
  if (b.riders.length > 64) return { error: "too many riders in one push" };

  const riders: PushedRider[] = [];
  for (const raw of b.riders) {
    if (typeof raw !== "object" || raw === null) return { error: "each rider must be an object" };
    const r = raw as Record<string, unknown>;
    if (!isGuidLike(r.guid)) return { error: "every rider needs a valid guid" };
    const name = typeof r.name === "string" ? r.name.trim().slice(0, 64) : "";
    const riderClass = normalizeClass(r.class) ?? cls;
    const lapsCompleted = Number(r.lapsCompleted);
    const raceLaps = Number(r.raceLaps);
    if (!Number.isFinite(lapsCompleted) || lapsCompleted < 0) return { error: "lapsCompleted must be a non-negative number" };
    if (!Number.isFinite(raceLaps) || raceLaps <= 0) return { error: "raceLaps must be a positive number" };
    riders.push({
      guid: (r.guid as string).trim(),
      name: name || "unknown",
      raceNum: Number.isFinite(Number(r.raceNum)) ? Number(r.raceNum) : null,
      class: riderClass,
      position: Number.isFinite(Number(r.position)) ? Number(r.position) : null,
      classified: r.classified === true,
      dnf: r.dnf === true,
      dsq: r.dsq === true,
      lapsCompleted,
      raceLaps,
      isBot: r.isBot === true,
    });
  }

  return { serverId, eventId, raceId, track, class: cls, session, timestamp, riders };
}

export interface IngestResult {
  status: number;
  body: unknown;
}

/**
 * The push endpoint's whole job: authenticate the server, parse strictly, decide who counts,
 * store the raw race, and — only if the race clears the 4-human threshold — run Glicko-2 for
 * every counted rider and persist the new ratings plus one history row each.
 */
export async function ingestResults(request: Request, env: Env): Promise<IngestResult> {
  const token = bearer(request.headers.get("Authorization"));
  if (!token) return { status: 401, body: { error: "a per-server rating bearer token is required" } };
  const server = await serverForRatingToken(env, token);
  if (!server) return { status: 401, body: { error: "unknown or revoked rating token" } };

  let raw: unknown;
  try {
    raw = await request.json();
  } catch {
    return { status: 400, body: { error: "expected a JSON body" } };
  }
  const parsed = parsePush(raw);
  if ("error" in parsed) return { status: 400, body: { error: parsed.error } };

  // The token identifies the server; a mismatched serverId in the body is a sign of a
  // misconfigured pusher, not something to silently paper over.
  if (parsed.serverId !== server.id) {
    return { status: 400, body: { error: "serverId does not match the server this token belongs to" } };
  }

  const rowId = `${parsed.serverId}:${parsed.eventId}:${parsed.raceId}`;
  const already = await env.DB.prepare("SELECT id FROM ingested_races WHERE id = ?").bind(rowId).first();
  if (already) return { status: 200, body: { ok: true, id: rowId, alreadyIngested: true } };

  // Who's human (never a bot/spectator) — bots never count as raters or ratees, full stop.
  const humans = parsed.riders.filter((r) => !r.isBot);
  const humanCount = humans.length;
  const raceRated = humanCount >= MIN_HUMAN_RIDERS;

  // Of the humans, who cleared the 50%-laps floor: that's who's eligible to be ranked and
  // rated for this race. A DNF/DSQ still counts if they made it past the floor — a retirement
  // after real mileage is real signal (design §1) — but a lap-1 crash or disconnect isn't.
  const eligible = humans.filter((r) => r.raceLaps > 0 && r.lapsCompleted / r.raceLaps >= MIN_LAP_FRACTION);

  // Finish order, best to worst: classified riders by position, then DNF/DSQ riders by laps
  // completed (more laps = finished more of the race = ranked better among non-finishers).
  const classified = eligible
    .filter((r) => r.classified && !r.dnf && !r.dsq)
    .sort((a, b) => (a.position ?? Number.MAX_SAFE_INTEGER) - (b.position ?? Number.MAX_SAFE_INTEGER));
  const retired = eligible
    .filter((r) => !(r.classified && !r.dnf && !r.dsq))
    .sort((a, b) => b.lapsCompleted - a.lapsCompleted);
  const order = [...classified, ...retired];

  const now = Date.now();

  // Store the raw race + every raw rider row first, whether or not the race ends up rated.
  const insertRace = env.DB.prepare(
    `INSERT INTO ingested_races
       (id, server_id, event_id, race_id, track, class, session, occurred_at, human_count, rated, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  ).bind(
    rowId,
    parsed.serverId,
    parsed.eventId,
    parsed.raceId,
    parsed.track,
    parsed.class,
    parsed.session,
    parsed.timestamp,
    humanCount,
    raceRated ? 1 : 0,
    now,
  );

  const rankOf = new Map(order.map((r, i) => [r.guid, i]));
  const resultInserts = parsed.riders.map((r) => {
    const counted = raceRated && rankOf.has(r.guid);
    return env.DB.prepare(
      `INSERT INTO race_results
         (race_row_id, guid, name, race_num, class, position, classified, dnf, dsq,
          laps_completed, race_laps, is_bot, counted, rank_order)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    ).bind(
      rowId,
      r.guid,
      r.name,
      r.raceNum,
      r.class,
      r.position,
      r.classified ? 1 : 0,
      r.dnf ? 1 : 0,
      r.dsq ? 1 : 0,
      r.lapsCompleted,
      r.raceLaps,
      r.isBot ? 1 : 0,
      counted ? 1 : 0,
      counted ? rankOf.get(r.guid)! : null,
    );
  });

  await env.DB.batch([insertRace, ...resultInserts]);

  if (!raceRated || order.length === 0) {
    return { status: 201, body: { ok: true, id: rowId, rated: false, humanCount } };
  }

  // Load each counted rider's current per-class rating (or the new-rider default), run one
  // Glicko-2 period for the race, and persist the after-state plus a history row per rider —
  // in the ingestion order the request gave us, deterministically.
  const before = new Map<string, Glicko>();
  for (const r of order) {
    const existing = await env.DB.prepare(
      "SELECT rating, rd, volatility FROM rider_ratings WHERE guid = ? AND class = ?",
    )
      .bind(r.guid, parsed.class)
      .first<{ rating: number; rd: number; volatility: number }>();
    before.set(r.guid, existing ? { rating: existing.rating, rd: existing.rd, volatility: existing.volatility } : DEFAULT_GLICKO);
  }

  const after = applyRace(
    order.map((r) => ({ guid: r.guid, before: before.get(r.guid)! })),
    GLICKO_TAU,
  );

  const writes = [];
  for (const r of order) {
    const preState = before.get(r.guid)!;
    const postState = after.get(r.guid)!;
    writes.push(
      env.DB.prepare(
        `INSERT INTO rider_ratings (guid, class, rating, rd, volatility, races, last_race_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 1, ?, ?)
         ON CONFLICT (guid, class) DO UPDATE SET
           rating = excluded.rating, rd = excluded.rd, volatility = excluded.volatility,
           races = races + 1, last_race_at = excluded.last_race_at, updated_at = excluded.updated_at`,
      ).bind(r.guid, parsed.class, postState.rating, postState.rd, postState.volatility, parsed.timestamp, now),
    );
    writes.push(
      env.DB.prepare(
        `INSERT INTO rating_history (guid, class, race_row_id, rating_before, rd_before, rating_after, rd_after, delta, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ).bind(
        r.guid,
        parsed.class,
        rowId,
        preState.rating,
        preState.rd,
        postState.rating,
        postState.rd,
        postState.rating - preState.rating,
        now,
      ),
    );
  }
  await env.DB.batch(writes);

  return { status: 201, body: { ok: true, id: rowId, rated: true, humanCount, ratedRiders: order.length } };
}

// ---------------------------------------------------------------------------------------------
// Public reads.
// ---------------------------------------------------------------------------------------------

/** An opaque id in place of the raw GUID, for anything the public can see. Stable, one-way. */
export async function opaqueRiderId(guid: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`rating:${guid}`));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("").slice(0, 20);
}

interface LeaderboardRow {
  guid: string;
  name: string;
  rating: number;
  races: number;
}

export interface LeaderboardResult {
  status: number;
  body: unknown;
}

/**
 * Top N for a class: rank, display name, rounded rating, race count. No RD/volatility, no
 * GUID — an opaque id instead — and no banned rider ever appears (design §2/§4: "a banned
 * rider simply disappears... rather than being shown as (banned)"). `includeBanned` is the
 * admin escape hatch (`ratingroute.ts`'s admin surface), never reachable from the public path.
 */
export async function leaderboard(
  env: Env,
  klass: string,
  limit = 50,
  includeBanned = false,
): Promise<LeaderboardResult> {
  const cls = normalizeClass(klass);
  if (!cls) return { status: 400, body: { error: "class is required" } };
  const cap = Math.min(Math.max(Number.isFinite(limit) ? Math.trunc(limit) : 50, 1), 200);

  const rows = await env.DB.prepare(
    `SELECT rr.guid AS guid, rr.rating AS rating, rr.races AS races,
            COALESCE(
              (SELECT name FROM race_results WHERE guid = rr.guid AND counted = 1
                 ORDER BY id DESC LIMIT 1),
              'unknown'
            ) AS name
       FROM rider_ratings rr
       WHERE rr.class = ?
       ORDER BY rr.rating DESC
       LIMIT ?`,
  )
    .bind(cls, includeBanned ? cap : cap * 3) // over-fetch: banned rows get filtered below
    .all<LeaderboardRow>();

  const out: { rank: number; id: string; name: string; rating: number; races: number }[] = [];
  for (const row of rows.results) {
    if (!includeBanned && (await isBanned(env, { guid: row.guid }))) continue;
    out.push({
      rank: out.length + 1,
      id: await opaqueRiderId(row.guid),
      name: row.name,
      rating: Math.round(row.rating),
      races: row.races,
    });
    if (out.length >= cap) break;
  }
  return { status: 200, body: { class: cls, riders: out } };
}

export interface ProfileResult {
  status: number;
  body: unknown;
}

/**
 * The signed-in rider's own GUID-linked ratings, across every class — never another rider's,
 * and never reachable without an account bearer token (`ratingroute.ts` gates this behind
 * `authenticate`, same as every other `/v1/me/*` route).
 */
export async function myRatings(env: Env, guid: string | null): Promise<ProfileResult> {
  if (!guid) return { status: 200, body: { linked: false, classes: [] } };

  const ratings = await env.DB.prepare(
    "SELECT class, rating, rd, volatility, races, last_race_at FROM rider_ratings WHERE guid = ? ORDER BY class",
  )
    .bind(guid)
    .all<{ class: string; rating: number; rd: number; volatility: number; races: number; last_race_at: number | null }>();

  const history = await env.DB.prepare(
    `SELECT class, rating_after AS rating, created_at FROM rating_history
       WHERE guid = ? ORDER BY created_at DESC LIMIT 100`,
  )
    .bind(guid)
    .all<{ class: string; rating: number; created_at: number }>();

  return {
    status: 200,
    body: {
      linked: true,
      classes: ratings.results.map((r) => ({
        class: r.class,
        rating: Math.round(r.rating),
        rd: Math.round(r.rd),
        volatility: r.volatility,
        races: r.races,
        lastRaceAt: r.last_race_at,
      })),
      history: history.results,
    },
  };
}

/** Admin-only: the raw row for a GUID, every class, RD/volatility included, ban status too. */
export async function adminRiderLookup(env: Env, guid: string): Promise<ProfileResult> {
  if (!isGuidLike(guid)) return { status: 400, body: { error: "not a guid" } };
  const ratings = await env.DB.prepare(
    "SELECT class, rating, rd, volatility, races, last_race_at, updated_at FROM rider_ratings WHERE guid = ? ORDER BY class",
  )
    .bind(guid)
    .all();
  const banned = await isBanned(env, { guid });
  return { status: 200, body: { guid, banned, classes: ratings.results } };
}
