/**
 * Reading the diagnostics log rather than only the top of it.
 *
 * `diagnostics.ts` takes reports in and decides what they mean. This takes the questions out:
 * find a rider and everything their game has ever loaded, find a file by anything it says
 * about itself, and take one file and list who has it. Same tables, no new facts.
 *
 * Every list is a page — an offset, a fixed size and a total — because the answer to "who
 * has this" is one row or nine hundred and the page has to read the same either way.
 */

import { isState, isTrust, parseMatched, type Matched, type State, type Trust } from "./diagnostics";
import { PRESENCE_TTL_MS } from "./validate";

// Paging and the search-box escaping live in `adminui.ts` — the paint views page and filter
// the same way, and two copies of "what is page 2" is how they stop agreeing. Re-exported
// rather than moved out of sight: every caller here has always read them from this module.
export { likeTerm, MAX_COUNT, PAGE_SIZE, parsePage, type Paged } from "./adminui";
import { MAX_COUNT, PAGE_SIZE, likeTerm, parsePage, type Paged } from "./adminui";

const DAY_MS = 86_400_000;

/** A window in days, clamped to something a page can draw. */
export function clampDays(value: string | null, fallback = 30): number {
  const asked = Number(value ?? String(fallback));
  if (!Number.isFinite(asked)) return fallback;
  return Math.min(365, Math.max(1, Math.trunc(asked)));
}

/** One of a fixed set, or the first of them. Keeps a query string out of the SQL. */
export function oneOf<T extends string>(value: string | null, allowed: readonly T[]): T {
  return allowed.find((a) => a === value) ?? allowed[0];
}

/** Worst-first ordering for a state column, as SQL. */
function stateRankSql(column: string): string {
  return `CASE ${column} WHEN 'alert' THEN 3 WHEN 'warn' THEN 2 WHEN 'ok' THEN 1 ELSE 0 END`;
}

/** The same for trust: an unsigned build under an otherwise signed name is the interesting one. */
function trustRankSql(column: string): string {
  return `CASE ${column} WHEN 'untrusted' THEN 3 WHEN 'unsigned' THEN 2 WHEN 'unchecked' THEN 1 ELSE 0 END`;
}

const STATE_BY_RANK: State[] = ["unknown", "ok", "warn", "alert"];
const TRUST_BY_RANK: Trust[] = ["signed", "unchecked", "unsigned", "untrusted"];

// ---------------------------------------------------------------------------
// Riders
// ---------------------------------------------------------------------------

export const RIDER_STATES = ["any", "alert", "warn", "ok", "unknown", "quiet"] as const;
export const RIDER_SORTS = ["seen", "state", "name", "unaccounted"] as const;

export interface RiderQuery {
  q: string;
  state: (typeof RIDER_STATES)[number];
  sort: (typeof RIDER_SORTS)[number];
  days: number;
  page: number;
}

export function parseRiderQuery(url: URL): RiderQuery {
  return {
    q: (url.searchParams.get("q") ?? "").trim().slice(0, 96),
    state: oneOf(url.searchParams.get("state"), RIDER_STATES),
    sort: oneOf(url.searchParams.get("sort"), RIDER_SORTS),
    days: clampDays(url.searchParams.get("days")),
    page: parsePage(url.searchParams.get("page")),
  };
}

/** A rider as the search lists them: who they are, and what their game last looked like. */
export interface RiderRow {
  accountId: string;
  riderName: string;
  guid: string;
  steamId: string;
  createdAt: number;
  /** Empty when the account has never reported — which is not the same as "ok". */
  state: State | "";
  worstState: State | "";
  worstAt: number;
  rulesVersion: number;
  moduleCount: number;
  unknownCount: number;
  appVersion: string;
  updatedAt: number;
  /** Where they are now, while their app is still reporting presence. */
  serverId: string;
  /** The last server their files were recorded on, which outlives presence. */
  lastServerId: string;
  /** Distinct files ever recorded for this account, and how many are unaccounted for. */
  files: number;
  flagged: number;
  /** The paint sync half of the same account: equipped slots, and when they last published. */
  paints: number;
  paintedAt: number;
}

interface RiderRecord {
  id: string;
  rider_name: string;
  guid: string | null;
  steam_id: string | null;
  created_at: number;
  state: string | null;
  worst_state: string | null;
  worst_at: number | null;
  rules_version: number | null;
  module_count: number | null;
  unknown_count: number | null;
  app_version: string | null;
  updated_at: number | null;
  server_id: string | null;
}

/** The columns every rider read shares. The presence subquery binds first — it is first in the text. */
const RIDER_COLUMNS =
  "a.id, a.rider_name, a.guid, a.steam_id, a.created_at, c.state, c.worst_state," +
  " c.worst_at, c.rules_version, c.module_count, c.unknown_count, c.app_version," +
  " c.updated_at," +
  " (SELECT p.server_id FROM presence p WHERE p.account_id = a.id AND p.updated_at > ?)" +
  "  AS server_id";

/**
 * The rider search.
 *
 * Driven from `accounts` rather than from the reports, so somebody who has never run the
 * game still answers a search for their name. "No report" is an answer; a page that omitted
 * them would read as "not enrolled".
 */
export async function searchRiders(env: Env, query: RiderQuery): Promise<Paged<RiderRow>> {
  const fresh = Date.now() - PRESENCE_TTL_MS;
  const since = Date.now() - query.days * DAY_MS;
  const where: string[] = [];
  const args: unknown[] = [];

  const term = likeTerm(query.q);
  if (term) {
    // Everything that identifies a person: the name they ride under, the GUID the game
    // publishes to every server, the Steam id, and the account id itself.
    where.push(
      "(a.rider_name LIKE ? ESCAPE '\\' OR a.guid LIKE ? ESCAPE '\\'" +
        " OR a.steam_id LIKE ? ESCAPE '\\' OR a.id LIKE ? ESCAPE '\\')",
    );
    args.push(term, term, term, term);
  }

  if (query.state === "quiet") {
    where.push("(c.account_id IS NULL OR c.updated_at <= ?)");
    args.push(since);
  } else if (query.state !== "any") {
    // The worse of what they say now and the peak inside the live window — the same reading
    // the live table uses, so something unloaded mid-session still answers a search.
    where.push("(c.state = ? OR (c.worst_at > ? AND c.worst_state = ?)) AND c.updated_at > ?");
    args.push(query.state, fresh, query.state, since);
  }

  const from = " FROM accounts a LEFT JOIN client_modules c ON c.account_id = a.id" +
    (where.length ? ` WHERE ${where.join(" AND ")}` : "");

  const order =
    query.sort === "name"
      ? " ORDER BY a.rider_name COLLATE NOCASE ASC"
      : query.sort === "state"
        ? ` ORDER BY ${stateRankSql("COALESCE(c.worst_state, c.state, '')")} DESC,` +
          " COALESCE(c.updated_at, 0) DESC"
        : query.sort === "unaccounted"
          ? " ORDER BY COALESCE(c.unknown_count, 0) DESC, COALESCE(c.updated_at, 0) DESC"
          : " ORDER BY COALESCE(c.updated_at, 0) DESC, a.created_at DESC";

  const total = await env.DB.prepare(
    `SELECT COUNT(*) AS n FROM (SELECT a.id${from} LIMIT ${MAX_COUNT})`,
  )
    .bind(...args)
    .first<{ n: number }>();

  const found = await env.DB.prepare(
    `SELECT ${RIDER_COLUMNS}${from}${order} LIMIT ? OFFSET ?`,
  )
    // The presence subquery is the first `?` in the text, so it binds first.
    .bind(fresh, ...args, PAGE_SIZE, (query.page - 1) * PAGE_SIZE)
    .all<RiderRecord>();

  const rows = (found.results ?? []).map(riderRow);
  await Promise.all([attachFileCounts(env, rows), attachPaintCounts(env, rows)]);
  return { rows, total: total?.n ?? rows.length, page: query.page, size: PAGE_SIZE };
}

function riderRow(r: RiderRecord): RiderRow {
  const fresh = Date.now() - PRESENCE_TTL_MS;
  return {
    accountId: r.id,
    riderName: r.rider_name,
    guid: r.guid ?? "",
    steamId: r.steam_id ?? "",
    createdAt: r.created_at,
    state: isState(r.state) ? r.state : "",
    // Only while it is still inside the live window: past that the live row has forgotten
    // what the peak was about.
    worstState: (r.worst_at ?? 0) > fresh && isState(r.worst_state) ? r.worst_state : "",
    worstAt: r.worst_at ?? 0,
    rulesVersion: r.rules_version ?? 0,
    moduleCount: r.module_count ?? 0,
    unknownCount: r.unknown_count ?? 0,
    appVersion: r.app_version ?? "",
    updatedAt: r.updated_at ?? 0,
    serverId: r.server_id ?? "",
    lastServerId: "",
    files: 0,
    flagged: 0,
    paints: 0,
    paintedAt: 0,
  };
}

/**
 * File counts for a page of riders, in one read.
 *
 * A correlated subquery per row would be fifty scans of the sightings table to draw one
 * page. This is one grouped read over the accounts actually on screen.
 */
async function attachFileCounts(env: Env, rows: RiderRow[]): Promise<void> {
  if (!rows.length) return;
  const ids = rows.map((r) => r.accountId);
  const counts = await env.DB.prepare(
    "SELECT account_id, COUNT(*) AS files," +
      " SUM(CASE WHEN state IN ('warn','alert') THEN 1 ELSE 0 END) AS flagged" +
      ` FROM client_module_seen WHERE account_id IN (${ids.map(() => "?").join(",")})` +
      " GROUP BY account_id",
  )
    .bind(...ids)
    .all<{ account_id: string; files: number; flagged: number }>();

  const byId = new Map((counts.results ?? []).map((c) => [c.account_id, c]));
  for (const row of rows) {
    row.files = byId.get(row.accountId)?.files ?? 0;
    row.flagged = byId.get(row.accountId)?.flagged ?? 0;
  }
}

/**
 * What paint sync holds for the same page of riders.
 *
 * The other half of the account, read the same way and for the same reason as the file
 * counts above: one grouped read over the rows on screen rather than a subquery per row.
 * Nothing joined the two dashboards before this, and they have always keyed on the same id.
 */
async function attachPaintCounts(env: Env, rows: RiderRow[]): Promise<void> {
  if (!rows.length) return;
  const ids = rows.map((r) => r.accountId);
  const counts = await env.DB.prepare(
    "SELECT p.account_id, COUNT(*) AS paints," +
      " (SELECT MAX(l.updated_at) FROM loadouts l WHERE l.account_id = p.account_id) AS painted_at" +
      ` FROM loadout_paints p WHERE p.account_id IN (${ids.map(() => "?").join(",")})` +
      " GROUP BY p.account_id",
  )
    .bind(...ids)
    .all<{ account_id: string; paints: number; painted_at: number | null }>();

  const byId = new Map((counts.results ?? []).map((c) => [c.account_id, c]));
  for (const row of rows) {
    row.paints = byId.get(row.accountId)?.paints ?? 0;
    row.paintedAt = byId.get(row.accountId)?.painted_at ?? 0;
  }
}

// ---------------------------------------------------------------------------
// One rider
// ---------------------------------------------------------------------------

/** One file as it was seen on one machine — what the rider page and the reverse lookup share. */
export interface Sighting {
  accountId: string;
  riderName: string;
  guid: string;
  name: string;
  sha256: string;
  origin: string;
  state: State;
  label: string;
  trust: Trust;
  publisher: string;
  company: string;
  product: string;
  description: string;
  size: number;
  mtime: number;
  serverId: string;
  firstAt: number;
  lastAt: number;
  hits: number;
}

interface SightingRecord {
  account_id: string;
  rider_name: string;
  guid?: string | null;
  name: string;
  sha256: string;
  origin: string;
  state: string;
  label: string;
  trust: string;
  publisher: string;
  company: string;
  product: string;
  description: string;
  size: number;
  mtime: number;
  server_id: string;
  first_at: number;
  last_at: number;
  hits: number;
}

function sighting(r: SightingRecord): Sighting {
  return {
    accountId: r.account_id,
    riderName: r.rider_name ?? "",
    guid: r.guid ?? "",
    name: r.name,
    sha256: r.sha256 ?? "",
    origin: r.origin ?? "",
    state: isState(r.state) ? r.state : "unknown",
    label: r.label ?? "",
    trust: isTrust(r.trust) ? r.trust : "unchecked",
    publisher: r.publisher ?? "",
    company: r.company ?? "",
    product: r.product ?? "",
    description: r.description ?? "",
    size: r.size ?? 0,
    mtime: r.mtime ?? 0,
    serverId: r.server_id ?? "",
    firstAt: r.first_at ?? 0,
    lastAt: r.last_at ?? 0,
    hits: r.hits ?? 0,
  };
}

export interface RiderDetail {
  rider: RiderRow;
  /** What the rules named in their last report. */
  matched: Matched[];
  files: Paged<Sighting>;
  /** Distinct servers they have been recorded on, most recent first. */
  servers: { serverId: string; lastAt: number }[];
}

export const SIGHTING_STATES = ["any", "alert", "warn", "ok", "unknown"] as const;

export interface SightingQuery {
  q: string;
  state: (typeof SIGHTING_STATES)[number];
  page: number;
}

export function parseSightingQuery(url: URL): SightingQuery {
  return {
    q: (url.searchParams.get("f") ?? "").trim().slice(0, 96),
    state: oneOf(url.searchParams.get("fstate"), SIGHTING_STATES),
    page: parsePage(url.searchParams.get("page")),
  };
}

/** Everything known about one rider, and a page of the files their game has loaded. */
export async function riderDetail(
  env: Env,
  who: string,
  query: SightingQuery,
): Promise<RiderDetail | null> {
  const fresh = Date.now() - PRESENCE_TTL_MS;
  // Account id, GUID or rider name, because all three are things you have in your hand when
  // you come to look someone up.
  const record = await env.DB.prepare(
    `SELECT ${RIDER_COLUMNS}, c.matched` +
      " FROM accounts a LEFT JOIN client_modules c ON c.account_id = a.id" +
      " WHERE a.id = ? OR a.guid = ? OR lower(a.rider_name) = lower(?) LIMIT 1",
  )
    .bind(fresh, who, who, who)
    .first<RiderRecord & { matched: string | null }>();
  if (!record) return null;

  const rider = riderRow(record);
  await Promise.all([attachFileCounts(env, [rider]), attachPaintCounts(env, [rider])]);

  const where = ["account_id = ?"];
  const args: unknown[] = [rider.accountId];
  const term = likeTerm(query.q);
  if (term) {
    where.push(
      "(name LIKE ? ESCAPE '\\' OR sha256 LIKE ? ESCAPE '\\'" +
        " OR publisher LIKE ? ESCAPE '\\' OR company LIKE ? ESCAPE '\\'" +
        " OR product LIKE ? ESCAPE '\\' OR description LIKE ? ESCAPE '\\')",
    );
    args.push(term, term, term, term, term, term);
  }
  if (query.state !== "any") {
    where.push("state = ?");
    args.push(query.state);
  }
  const filter = ` WHERE ${where.join(" AND ")}`;

  const [total, files, servers] = await Promise.all([
    env.DB.prepare(`SELECT COUNT(*) AS n FROM client_module_seen${filter}`)
      .bind(...args)
      .first<{ n: number }>(),
    env.DB.prepare(
      "SELECT account_id, rider_name, name, sha256, origin, state, label, trust, publisher," +
        " company, product, description, size, mtime, server_id, first_at, last_at, hits" +
        ` FROM client_module_seen${filter}` +
        ` ORDER BY ${stateRankSql("state")} DESC, last_at DESC LIMIT ? OFFSET ?`,
    )
      .bind(...args, PAGE_SIZE, (query.page - 1) * PAGE_SIZE)
      .all<SightingRecord>(),
    env.DB.prepare(
      "SELECT server_id, MAX(last_at) AS last_at FROM client_module_seen" +
        " WHERE account_id = ? AND server_id <> '' GROUP BY server_id" +
        " ORDER BY MAX(last_at) DESC LIMIT 12",
    )
      .bind(rider.accountId)
      .all<{ server_id: string; last_at: number }>(),
  ]);

  const seenServers = (servers.results ?? []).map((s) => ({
    serverId: s.server_id,
    lastAt: s.last_at,
  }));
  rider.lastServerId = seenServers[0]?.serverId ?? "";

  return {
    rider,
    matched: parseMatched(record.matched ?? ""),
    files: {
      rows: (files.results ?? []).map(sighting),
      total: total?.n ?? 0,
      page: query.page,
      size: PAGE_SIZE,
    },
    servers: seenServers,
  };
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

export const FILE_STATES = ["flagged", "any", "alert", "warn", "ok"] as const;
export const FILE_TRUSTS = ["any", "signed", "unsigned", "untrusted", "unchecked"] as const;
export const FILE_ORIGINS = ["any", "game", "app", "other"] as const;
export const FILE_SORTS = ["state", "last", "accounts", "hits", "name"] as const;

export interface FileQuery {
  q: string;
  state: (typeof FILE_STATES)[number];
  trust: (typeof FILE_TRUSTS)[number];
  origin: (typeof FILE_ORIGINS)[number];
  sort: (typeof FILE_SORTS)[number];
  days: number;
  page: number;
}

export function parseFileQuery(url: URL): FileQuery {
  return {
    q: (url.searchParams.get("q") ?? "").trim().slice(0, 96),
    state: oneOf(url.searchParams.get("state"), FILE_STATES),
    trust: oneOf(url.searchParams.get("trust"), FILE_TRUSTS),
    origin: oneOf(url.searchParams.get("origin"), FILE_ORIGINS),
    sort: oneOf(url.searchParams.get("sort"), FILE_SORTS),
    days: clampDays(url.searchParams.get("days")),
    page: parsePage(url.searchParams.get("page")),
  };
}

/** One distinct build of a file, folded across everyone who has it. */
export interface FileVariant {
  name: string;
  sha256: string;
  origin: string;
  state: State;
  label: string;
  trust: Trust;
  publisher: string;
  company: string;
  product: string;
  description: string;
  size: number;
  mtime: number;
  /** Only when exactly one account has it: naming one of several would accuse them. */
  riderName: string;
  accounts: number;
  hits: number;
  firstAt: number;
  lastAt: number;
}

/** Every build sharing a file name — the unit a rule is written against. */
export interface FileGroup {
  name: string;
  state: State;
  label: string;
  trust: Trust;
  publisher: string;
  claims: string;
  accounts: number;
  hits: number;
  firstAt: number;
  lastAt: number;
  variantCount: number;
  riderName: string;
}

interface GroupRecord {
  name: string;
  accounts: number;
  variants: number;
  hits: number;
  first_at: number;
  last_at: number;
  state_rank: number;
  trust_rank: number;
  label: string | null;
  publisher: string | null;
  company: string | null;
  product: string | null;
  description: string | null;
  rider_name: string | null;
}

/**
 * The grouped columns, shared by the file search and one file's build list.
 *
 * The worst state and the worst signature come out of a ranked aggregate rather than a
 * correlated subquery: it respects whatever filter the caller applied, and averaging would
 * hide the odd build out, which is the one worth seeing.
 */
const GROUP_COLUMNS =
  "COUNT(DISTINCT account_id) AS accounts, COUNT(DISTINCT sha256) AS variants," +
  " SUM(hits) AS hits, MIN(first_at) AS first_at, MAX(last_at) AS last_at," +
  ` MAX(${stateRankSql("state")}) AS state_rank, MAX(${trustRankSql("trust")}) AS trust_rank,` +
  " MAX(label) AS label, MAX(publisher) AS publisher, MAX(company) AS company," +
  " MAX(product) AS product, MAX(description) AS description, MAX(rider_name) AS rider_name";

function fileFilter(query: FileQuery): { sql: string; args: unknown[] } {
  const where = ["last_at > ?"];
  const args: unknown[] = [Date.now() - query.days * DAY_MS];

  const term = likeTerm(query.q);
  if (term) {
    // Name, hash, who signed it, and everything it claims about itself — one box, because
    // remembering which field a half-remembered string came from is the hard part.
    where.push(
      "(name LIKE ? ESCAPE '\\' OR sha256 LIKE ? ESCAPE '\\'" +
        " OR publisher LIKE ? ESCAPE '\\' OR company LIKE ? ESCAPE '\\'" +
        " OR product LIKE ? ESCAPE '\\' OR description LIKE ? ESCAPE '\\'" +
        " OR label LIKE ? ESCAPE '\\')",
    );
    args.push(term, term, term, term, term, term, term);
  }
  if (query.state === "flagged") where.push("state IN ('warn','alert')");
  else if (query.state !== "any") {
    where.push("state = ?");
    args.push(query.state);
  }
  if (query.trust !== "any") {
    where.push("trust = ?");
    args.push(query.trust);
  }
  if (query.origin !== "any") {
    where.push("origin = ?");
    args.push(query.origin);
  }
  return { sql: ` WHERE ${where.join(" AND ")}`, args };
}

function fileGroup(g: GroupRecord): FileGroup {
  return {
    name: g.name,
    state: STATE_BY_RANK[g.state_rank] ?? "unknown",
    label: g.label ?? "",
    trust: TRUST_BY_RANK[g.trust_rank] ?? "unchecked",
    publisher: g.publisher ?? "",
    claims: claimText(g.description ?? "", g.product ?? "", g.company ?? ""),
    accounts: g.accounts,
    hits: g.hits,
    firstAt: g.first_at,
    lastAt: g.last_at,
    variantCount: g.variants,
    riderName: g.accounts === 1 ? (g.rider_name ?? "") : "",
  };
}

/**
 * The file search: one row per file name, however many people have it.
 *
 * Grouped in SQL because a driver loaded by six hundred people is six hundred rows that say
 * the same thing, and the name is what a rule is written against anyway.
 */
export async function searchFiles(env: Env, query: FileQuery): Promise<Paged<FileGroup>> {
  const { sql, args } = fileFilter(query);

  const order =
    query.sort === "accounts"
      ? " ORDER BY accounts DESC, last_at DESC"
      : query.sort === "name"
        ? " ORDER BY name ASC"
        : query.sort === "hits"
          ? " ORDER BY hits DESC, last_at DESC"
          : query.sort === "last"
            ? " ORDER BY last_at DESC"
            : " ORDER BY state_rank DESC, last_at DESC";

  const [total, found] = await Promise.all([
    env.DB.prepare(
      `SELECT COUNT(*) AS n FROM (SELECT name FROM client_module_seen${sql}` +
        ` GROUP BY name LIMIT ${MAX_COUNT})`,
    )
      .bind(...args)
      .first<{ n: number }>(),
    env.DB.prepare(
      `SELECT name, ${GROUP_COLUMNS} FROM client_module_seen${sql}` +
        ` GROUP BY name${order} LIMIT ? OFFSET ?`,
    )
      .bind(...args, PAGE_SIZE, (query.page - 1) * PAGE_SIZE)
      .all<GroupRecord>(),
  ]);

  const rows = (found.results ?? []).map(fileGroup);
  return { rows, total: total?.n ?? rows.length, page: query.page, size: PAGE_SIZE };
}

/** What a file says it is, in one line. Text off a file, never evidence. */
export function claimText(description: string, product: string, company: string): string {
  const said = description || product || company;
  if (!said) return "";
  return company && company !== said ? `${said} — ${company}` : said;
}

// ---------------------------------------------------------------------------
// One file, backwards
// ---------------------------------------------------------------------------

export interface FileDetail {
  name: string;
  /** Set when the page was narrowed to one build. */
  sha256: string;
  state: State;
  label: string;
  accounts: number;
  hits: number;
  firstAt: number;
  lastAt: number;
  variants: FileVariant[];
  /** Who has loaded it. The reverse lookup this page exists for. */
  holders: Paged<Sighting>;
}

/** The most builds drawn under one name. Past it the holders list still tells the story. */
const MAX_VARIANTS = 100;

/**
 * One file name, and everyone whose game has loaded it.
 *
 * `sha256` narrows both halves to a single build, which is what a rule written by hash
 * covers: the two questions are "what is this file" and "who has this exact file".
 */
export async function fileDetail(
  env: Env,
  name: string,
  sha256: string,
  page: number,
): Promise<FileDetail | null> {
  const narrow = sha256 ? " AND sha256 = ?" : "";
  const key: unknown[] = sha256 ? [name, sha256] : [name];

  const [summary, variants, total, holders] = await Promise.all([
    env.DB.prepare(
      `SELECT name, ${GROUP_COLUMNS} FROM client_module_seen WHERE name = ?${narrow}`,
    )
      .bind(...key)
      .first<GroupRecord>(),
    env.DB.prepare(
      `SELECT name, sha256, MAX(origin) AS origin, MAX(size) AS size, MAX(mtime) AS mtime,` +
        ` ${GROUP_COLUMNS} FROM client_module_seen WHERE name = ?${narrow}` +
        ` GROUP BY sha256 ORDER BY state_rank DESC, last_at DESC LIMIT ${MAX_VARIANTS}`,
    )
      .bind(...key)
      .all<GroupRecord & { sha256: string; origin: string; size: number; mtime: number }>(),
    env.DB.prepare(`SELECT COUNT(*) AS n FROM client_module_seen WHERE name = ?${narrow}`)
      .bind(...key)
      .first<{ n: number }>(),
    env.DB.prepare(
      "SELECT s.account_id, s.rider_name, a.guid, s.name, s.sha256, s.origin, s.state," +
        " s.label, s.trust, s.publisher, s.company, s.product, s.description, s.size," +
        " s.mtime, s.server_id, s.first_at, s.last_at, s.hits" +
        " FROM client_module_seen s LEFT JOIN accounts a ON a.id = s.account_id" +
        ` WHERE s.name = ?${sha256 ? " AND s.sha256 = ?" : ""}` +
        ` ORDER BY ${stateRankSql("s.state")} DESC, s.last_at DESC LIMIT ? OFFSET ?`,
    )
      .bind(...key, PAGE_SIZE, (page - 1) * PAGE_SIZE)
      .all<SightingRecord>(),
  ]);

  // An aggregate over no rows still returns one row, of nulls. That is the "not found" case.
  if (!summary || !summary.accounts) return null;
  const group = fileGroup({ ...summary, name });

  return {
    name,
    sha256,
    state: group.state,
    label: group.label,
    accounts: group.accounts,
    hits: group.hits,
    firstAt: group.firstAt,
    lastAt: group.lastAt,
    variants: (variants.results ?? []).map<FileVariant>((v) => {
      const folded = fileGroup({ ...v, name });
      return {
        name,
        sha256: v.sha256 ?? "",
        origin: v.origin ?? "",
        state: folded.state,
        label: folded.label,
        trust: folded.trust,
        publisher: folded.publisher,
        company: v.company ?? "",
        product: v.product ?? "",
        description: v.description ?? "",
        size: v.size ?? 0,
        mtime: v.mtime ?? 0,
        riderName: folded.riderName,
        accounts: folded.accounts,
        hits: folded.hits,
        firstAt: folded.firstAt,
        lastAt: folded.lastAt,
      };
    }),
    holders: {
      rows: (holders.results ?? []).map(sighting),
      total: total?.n ?? 0,
      page,
      size: PAGE_SIZE,
    },
  };
}

// ---------------------------------------------------------------------------
// Overview counts
// ---------------------------------------------------------------------------

export interface Totals {
  /** Accounts on record, and how many have ever reported. */
  accounts: number;
  reported: number;
  /** Distinct file names and distinct builds recorded inside the window. */
  files: number;
  builds: number;
  flagged: number;
}

/** The numbers the overview tiles read. */
export async function totals(env: Env, days: number): Promise<Totals> {
  const [accounts, files] = await Promise.all([
    env.DB.prepare(
      "SELECT (SELECT COUNT(*) FROM accounts) AS accounts," +
        " (SELECT COUNT(*) FROM client_modules) AS reported",
    ).first<{ accounts: number; reported: number }>(),
    env.DB.prepare(
      "SELECT COUNT(DISTINCT name) AS files, COUNT(DISTINCT sha256) AS builds," +
        " COUNT(DISTINCT CASE WHEN state IN ('warn','alert') THEN name END) AS flagged" +
        " FROM client_module_seen WHERE last_at > ?",
    )
      .bind(Date.now() - days * DAY_MS)
      .first<{ files: number; builds: number; flagged: number }>(),
  ]);
  return {
    accounts: accounts?.accounts ?? 0,
    reported: accounts?.reported ?? 0,
    files: files?.files ?? 0,
    builds: files?.builds ?? 0,
    flagged: files?.flagged ?? 0,
  };
}
