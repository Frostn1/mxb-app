/**
 * Searches that found nothing.
 *
 * The apps' Browse tab reports the final, settled text of a search that came back empty, so
 * the catalog gaps people actually hit are visible. This is deliberately the thinnest thing in
 * the deployment: a query string, the title it was typed under, and a count per day.
 *
 * ## What is not kept
 *
 * No install id, no account, no Steam id, no device, no IP, no per-request time. The rate limit
 * below uses the same per-address daily digest as the survey (`device_claims`), which is a
 * salted one-way hash that is never stored beside a query and is swept with the rest of that
 * table. Cloudflare's request headers are not read or logged here.
 *
 * Queries that look like they carry an identifier (an address, a link, a path, a long number,
 * a GUID) are dropped rather than scrubbed: a search for a mod name never needs one.
 */

import { ipDigest } from "./voice";
import { adminAllowed, dayKey, SIGNATURE_HEADER, signatureOk } from "./usage";

export const MAX_QUERY_CHARS = 80;
export const MIN_QUERY_CHARS = 2;
export const MAX_MISS_BYTES = 1024;
/** Per address per day. A person searching for things that don't exist hits this slowly. */
export const MAX_MISSES_PER_DAY = 100;
export const RETENTION_DAYS = 90;
export const MAX_WINDOW_DAYS = 90;
export const MAX_ROWS = 100;
const CONTENT_TYPE = "application/json";

export interface Miss {
  query: string;
  game: string;
}

/**
 * The query as it is stored: trimmed, lowercased, whitespace collapsed, capped. Null when it is
 * empty, too short to mean anything, or looks like it carries an identifier.
 */
export function normaliseQuery(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const q = value
    .replace(/[\p{Cc}\p{Cf}]/gu, " ")
    .trim()
    .toLowerCase()
    .replace(/\s+/g, " ")
    .slice(0, MAX_QUERY_CHARS)
    .trim();
  if (q.length < MIN_QUERY_CHARS) return null;
  if (/@|:\/\/|www\.|[\\/]/.test(q)) return null;
  if (/\d{7,}/.test(q.replace(/[\s-]/g, ""))) return null;
  if (/[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12}/.test(q)) return null;
  return q;
}

/** Check a body. A string is the reason it was refused. */
export function parseMiss(raw: string): Miss | string {
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "expected JSON";
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) return "expected an object";
  const { query, game } = body as Record<string, unknown>;
  const q = normaliseQuery(query);
  if (q === null) return "query is not usable";
  const g = game === undefined || game === null || game === "" ? "" : game;
  if (typeof g !== "string" || (g !== "" && !/^[a-z0-9-]{1,16}$/.test(g))) return "game must be a slug";
  return { query: q, game: g };
}

/** `POST /v1/search-misses` — one empty search. Unauthenticated, bounded, rate limited. */
export async function reportMiss(request: Request, env: Env): Promise<Response> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_MISS_BYTES) return json(413, { error: "too large" });

  const type = (request.headers.get("content-type") ?? "").split(";")[0].trim().toLowerCase();
  if (type !== CONTENT_TYPE) return json(415, { error: `expected ${CONTENT_TYPE}` });

  let raw: string;
  try {
    raw = await request.text();
  } catch {
    return json(413, { error: "too large" });
  }
  if (raw.length > MAX_MISS_BYTES) return json(413, { error: "too large" });

  if (
    env.MXB_USAGE_REQUIRE_SIGNATURE === "1" &&
    !(await signatureOk(request.headers.get(SIGNATURE_HEADER), raw, env))
  ) {
    return json(401, { error: "unsigned report" });
  }

  const miss = parseMiss(raw);
  if (typeof miss === "string") return json(400, { error: miss });

  const now = Date.now();
  const day = dayKey(now);
  const digest = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);
  const seen = await env.DB.prepare(
    "SELECT claims FROM device_claims WHERE ip_digest = ? AND day = ? AND kind = 'searchmiss'",
  )
    .bind(digest, day)
    .first<{ claims: number }>();
  if (seen && seen.claims >= MAX_MISSES_PER_DAY) return json(429, { error: "too many reports today" });

  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO search_misses (day, query, game, misses) VALUES (?, ?, ?, 1)" +
        " ON CONFLICT(day, query, game) DO UPDATE SET misses = misses + 1",
    ).bind(day, miss.query, miss.game),
    env.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at)" +
        " VALUES (?, ?, 'searchmiss', 1, ?)" +
        " ON CONFLICT(ip_digest, day, kind) DO UPDATE SET" +
        "  claims = claims + 1, updated_at = excluded.updated_at",
    ).bind(digest, day, now),
  ]);
  return json(202, { ok: true });
}

export interface MissRow {
  query: string;
  game: string;
  misses: number;
  days: number;
  /** ISO date (day) of the most recent miss in the window. */
  lastSeen: string;
}

export interface MissStats {
  generatedAt: number;
  days: number;
  retentionDays: number;
  top: MissRow[];
}

/** Clamp a requested window to 1..MAX_WINDOW_DAYS, default 30. */
export function missWindow(url: URL): number {
  const asked = Number(url.searchParams.get("days") ?? "30");
  if (!Number.isFinite(asked)) return 30;
  return Math.min(MAX_WINDOW_DAYS, Math.max(1, Math.trunc(asked)));
}

/** The most-missed queries in the window, by total count. */
export async function collectMisses(env: Env, days: number, now = Date.now()): Promise<MissStats> {
  const rows = await env.DB.prepare(
    "SELECT query, game, SUM(misses) AS misses, COUNT(*) AS days, MAX(day) AS lastSeen FROM search_misses" +
      " WHERE day >= ? GROUP BY query, game ORDER BY misses DESC, query ASC LIMIT ?",
  )
    .bind(dayKey(now, days - 1), MAX_ROWS)
    .all<MissRow>();
  return {
    generatedAt: now,
    days,
    retentionDays: RETENTION_DAYS,
    top: (rows.results ?? []).map((r) => ({
      query: r.query,
      game: r.game,
      misses: Number(r.misses),
      days: Number(r.days),
      lastSeen: String(r.lastSeen),
    })),
  };
}

/** `GET /v1/search-misses/stats` — gated exactly like `/v1/survey/stats`. */
export async function missStats(request: Request, url: URL, env: Env): Promise<Response> {
  const allowed = adminAllowed(request, url, env);
  if (allowed === "unset") return json(503, { error: "no admin key is configured" });
  if (allowed === "denied") return json(401, { error: "unauthorized" });
  return json(200, await collectMisses(env, missWindow(url)));
}

export async function pruneSearchMisses(env: Env): Promise<void> {
  try {
    await env.DB.prepare("DELETE FROM search_misses WHERE day < ?")
      .bind(dayKey(Date.now(), RETENTION_DAYS))
      .run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "search miss sweep failed", error: String(err) }));
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
