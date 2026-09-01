/**
 * What is loaded inside a player's running game.
 *
 * The app walks the module list of the running game and posts it here — file name, where it
 * came from, and a hash for anything that is not a Windows system library. That is the whole
 * of the client's job: it observes and reports, and it holds no opinion about any file.
 *
 * Every judgement lives here instead, against `module_rules`, and that split is the design
 * rather than an accident of layering:
 *
 *   * **The client cannot be read.** A shipped binary is one `strings` away from telling
 *     anyone what it looks for. Nothing in it looks for anything.
 *   * **A rule takes effect immediately**, on the next report from every install, without a
 *     release or a deploy — which matters because the thing being described changes weekly.
 *   * **The client is never told what we made of it.** `putReport` answers `{ ok: true }`
 *     whatever it decided. A client that could read its own state could be made to stop
 *     reporting exactly when the answer got interesting.
 *
 * Three states above "we could not look", and the middle one is doing the real work:
 *
 *   * `ok`      — everything in the list is accounted for.
 *   * `warn`    — something is loaded that nothing accounts for. Not a conclusion. An
 *                 overlay nobody has listed yet lands here, and so does the first sighting
 *                 of something that matters.
 *   * `alert`   — a rule named it.
 *   * `unknown` — the list could not be read. Never folded into `ok`: a report that says a
 *                 machine is fine when it was never looked at is worse than no report.
 *
 * The honest limit, stated once: this describes clients that run the app and report. Someone
 * who does not is not described, and someone who patches the app reports what they like. It
 * raises the cost of the ordinary case; it is not a wall.
 */

import { isAppVersion, isSha256, PRESENCE_TTL_MS } from "./validate";

export interface Account {
  id: string;
  rider_name: string;
}

/** Where the client loaded a module from. Its judgement of location, not of character. */
export type Origin = "game" | "system" | "app" | "other";

export type State = "unknown" | "ok" | "warn" | "alert";

/** Worst-last, so a report's state is the maximum over its modules. */
const STATE_RANK: Record<State, number> = { unknown: 0, ok: 1, warn: 2, alert: 3 };

export function stateRank(value: string): number {
  return STATE_RANK[value as State] ?? 0;
}

export function isState(value: unknown): value is State {
  return typeof value === "string" && value in STATE_RANK;
}

export interface ReportedModule {
  name: string;
  origin: Origin;
  /** Empty for system libraries, which the client does not hash, and for unreadable files. */
  sha256: string;
}

export interface ModuleRule {
  id: number;
  kind: "deny" | "allow";
  /** Lowercase substring of a file name. Empty when the rule matches by hash. */
  pattern: string;
  sha256: string;
  label: string;
}

/** One rule-matched module, as it is stored and shown. */
export interface Matched {
  name: string;
  sha256: string;
  label: string;
}

export interface Classification {
  state: State;
  matched: Matched[];
  /** Loaded from somewhere nothing accounts for. */
  unknown: ReportedModule[];
}

/** A module list longer than this is not a game, it is a client bug or an attempt at one. */
export const MAX_MODULES = 400;

/** A file name. Long enough for anything real, short enough to be a column. */
const MAX_NAME = 96;

const ORIGINS: readonly string[] = ["game", "system", "app", "other"];

/** Anything but a plain file name: a path would carry the player's user folder with it. */
const NAME_SHAPE = /^[a-z0-9._+()-]+$/;

/**
 * Names the Windows loader will pull out of the executable's own folder before it looks
 * anywhere else.
 *
 * A file with one of these names beside the game's exe is loaded into it without anything
 * having to inject it, which makes it the cheapest way into a process — and the one case
 * where sitting in the game's own folder is the observation rather than the defence. Kept
 * here rather than in the client for the reason everything else is: it is a thing we look
 * for, and the client looks for nothing.
 *
 * `opengl32.dll` is on the list and is also how ReShade legitimately installs into an OpenGL
 * title, which MX Bikes is. That is what `allow` rules are for: allow the hash once, and
 * every install carrying that exact file reads as accounted for.
 */
const LOADER_NAMES: readonly string[] = [
  "opengl32.dll",
  "dxgi.dll",
  "d3d9.dll",
  "d3d10.dll",
  "d3d11.dll",
  "d3d12.dll",
  "dinput8.dll",
  "dsound.dll",
  "winmm.dll",
  "version.dll",
  "wininet.dll",
  "xinput1_3.dll",
];

/**
 * Read a reported module list, or `null` if it is not one.
 *
 * Bounded on every axis before anything is stored: this is a list of strings from a client,
 * and it becomes rows in a table and text on an admin's page.
 */
export function parseModules(value: unknown): ReportedModule[] | null {
  if (!Array.isArray(value)) return null;
  if (value.length > MAX_MODULES) return null;
  const out: ReportedModule[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    if (!entry || typeof entry !== "object") return null;
    const { name, origin, sha256 } = entry as Record<string, unknown>;
    if (typeof name !== "string" || typeof origin !== "string") return null;
    const clean = name.trim().toLowerCase();
    if (!clean || clean.length > MAX_NAME) return null;
    if (!NAME_SHAPE.test(clean)) return null;
    if (!ORIGINS.includes(origin)) return null;
    const hash = typeof sha256 === "string" ? sha256.trim().toLowerCase() : "";
    if (hash && !isSha256(hash)) return null;
    const key = `${clean} ${hash}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ name: clean, origin: origin as Origin, sha256: hash });
  }
  return out;
}

/**
 * What the rules make of one module list.
 *
 * The order is the policy, and it is deliberate:
 *
 *   1. **Deny first.** A named file is named wherever it loaded from — something that copies
 *      itself into the game's folder must not be trusted for being there.
 *   2. **Allow next**, so a false positive is silenced by adding one row, not by a release.
 *   3. **Loader names in the game's folder**, the one case where the location is the point.
 *   4. **Anything from outside** the game, the system, and what the app installed.
 *   5. Everything else is accounted for.
 */
export function classify(modules: ReportedModule[], rules: ModuleRule[]): Classification {
  const deny = rules.filter((r) => r.kind === "deny");
  const allow = rules.filter((r) => r.kind === "allow");
  const matched: Matched[] = [];
  const unknown: ReportedModule[] = [];

  for (const mod of modules) {
    const hit = matchRule(deny, mod);
    if (hit) {
      matched.push({ name: mod.name, sha256: mod.sha256, label: hit.label });
      continue;
    }
    if (matchRule(allow, mod)) continue;
    if (mod.origin === "game" && LOADER_NAMES.includes(mod.name)) {
      unknown.push(mod);
      continue;
    }
    if (mod.origin === "other") unknown.push(mod);
  }

  const state: State = matched.length > 0 ? "alert" : unknown.length > 0 ? "warn" : "ok";
  return { state, matched, unknown };
}

/** The first rule that names this module, by hash or by name. */
function matchRule(rules: ModuleRule[], mod: ReportedModule): ModuleRule | null {
  for (const rule of rules) {
    if (rule.sha256 && mod.sha256 && rule.sha256 === mod.sha256) return rule;
    if (rule.pattern && mod.name.includes(rule.pattern)) return rule;
  }
  return null;
}

/** The rules in force, oldest first. Version is `max(id)` — see the migration. */
export async function loadRules(env: Env): Promise<{ rules: ModuleRule[]; version: number }> {
  const rows = await env.DB.prepare(
    "SELECT id, kind, pattern, sha256, label FROM module_rules ORDER BY id",
  ).all<{ id: number; kind: string; pattern: string; sha256: string; label: string }>();
  const rules = (rows.results ?? [])
    .filter((r) => r.kind === "deny" || r.kind === "allow")
    .map((r) => ({
      id: r.id,
      kind: r.kind as "deny" | "allow",
      pattern: (r.pattern ?? "").toLowerCase(),
      sha256: (r.sha256 ?? "").toLowerCase(),
      label: r.label ?? "",
    }));
  const version = rules.reduce((max, r) => Math.max(max, r.id), 0);
  return { rules, version };
}

/**
 * A client posting what its game has loaded.
 *
 * Answers `{ ok: true }` for everything it accepts, including the reports it finds most
 * interesting. Nothing about the classification travels back down the wire.
 */
export async function putReport(request: Request, account: Account, env: Env): Promise<Response> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return json(400, { error: "expected a JSON body" });
  }
  if (!body || typeof body !== "object") return json(400, { error: "expected a JSON body" });
  const { modules, appVersion, available } = body as Record<string, unknown>;

  const now = Date.now();
  const version = isAppVersion(appVersion) ? appVersion : "";

  // The client saying it could not read the list. A real answer, stored as one: it is the
  // difference between "nothing was loaded" and "we were not allowed to look", and an app
  // running below the game's integrity level reports it every pass.
  if (available === false) {
    await store(env, account, "unknown", 0, 0, 0, [], [], version, now);
    return json(200, { ok: true });
  }

  const list = parseModules(modules);
  if (!list) return json(400, { error: "that is not a module list" });

  const { rules, version: rulesVersion } = await loadRules(env);
  const verdict = classify(list, rules);
  await store(
    env,
    account,
    verdict.state,
    rulesVersion,
    list.length,
    verdict.unknown.length,
    verdict.matched,
    // System libraries are not recorded per-file: hundreds of them, identical everywhere,
    // and they would bury the rows worth reading.
    list.filter((m) => m.origin !== "system"),
    version,
    now,
  );
  return json(200, { ok: true });
}

/**
 * Write one report: the live row, and a row per non-system module seen.
 *
 * The live row keeps the worst state of the recent past alongside the current one, for the
 * reason the migration gives — otherwise the feature is defeated by unloading. The `seen`
 * rows are what survive the session.
 */
async function store(
  env: Env,
  account: Account,
  state: State,
  rulesVersion: number,
  moduleCount: number,
  unknownCount: number,
  matched: Matched[],
  seen: ReportedModule[],
  appVersion: string,
  now: number,
): Promise<void> {
  const prev = await env.DB.prepare(
    "SELECT worst_state, worst_at FROM client_modules WHERE account_id = ?",
  )
    .bind(account.id)
    .first<{ worst_state: string; worst_at: number }>();
  const stillCurrent = prev !== null && prev.worst_at > now - PRESENCE_TTL_MS;
  const keepWorst = stillCurrent && stateRank(prev!.worst_state) > stateRank(state);

  const where = await env.DB.prepare(
    "SELECT server_id FROM presence WHERE account_id = ? AND updated_at > ?",
  )
    .bind(account.id, now - PRESENCE_TTL_MS)
    .first<{ server_id: string }>();
  const serverId = where?.server_id ?? "";

  const byName = new Map<string, Matched>();
  for (const hit of matched) byName.set(`${hit.name} ${hit.sha256}`, hit);

  const statements = [
    env.DB.prepare(
      "INSERT INTO client_modules (account_id, state, rules_version, module_count," +
        " unknown_count, matched, worst_state, worst_at, app_version, updated_at)" +
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)" +
        " ON CONFLICT(account_id) DO UPDATE SET state = excluded.state," +
        " rules_version = excluded.rules_version, module_count = excluded.module_count," +
        " unknown_count = excluded.unknown_count, matched = excluded.matched," +
        " worst_state = excluded.worst_state, worst_at = excluded.worst_at," +
        " app_version = excluded.app_version, updated_at = excluded.updated_at",
    ).bind(
      account.id,
      state,
      rulesVersion,
      moduleCount,
      unknownCount,
      JSON.stringify(matched),
      keepWorst ? prev!.worst_state : state,
      keepWorst ? prev!.worst_at : now,
      appVersion,
      now,
    ),
  ];

  for (const mod of seen) {
    const hit = byName.get(`${mod.name} ${mod.sha256}`);
    // Per module rather than per report: this row is about the file, so it carries what the
    // rules made of that file and not the state of the machine it was on.
    const modState: State = hit ? "alert" : unknownState(mod, state);
    statements.push(
      env.DB.prepare(
        "INSERT INTO client_module_seen (account_id, name, sha256, origin, state, label," +
          " rider_name, server_id, first_at, last_at, hits)" +
          " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)" +
          " ON CONFLICT(account_id, name, sha256) DO UPDATE SET origin = excluded.origin," +
          " state = excluded.state, label = excluded.label, rider_name = excluded.rider_name," +
          " server_id = excluded.server_id, last_at = excluded.last_at, hits = hits + 1",
      ).bind(
        account.id,
        mod.name,
        mod.sha256,
        mod.origin,
        modState,
        hit?.label ?? "",
        account.rider_name,
        serverId,
        now,
        now,
      ),
    );
  }

  await env.DB.batch(statements);
}

/** Was this module one of the ones nothing accounted for in the report it arrived in? */
function unknownState(mod: ReportedModule, reportState: State): State {
  if (mod.origin === "other") return "warn";
  if (mod.origin === "game" && LOADER_NAMES.includes(mod.name)) return "warn";
  return reportState === "unknown" ? "unknown" : "ok";
}

/** One account's current answer, for the admin page. */
export interface LiveRow {
  accountId: string;
  riderName: string;
  guid: string;
  serverId: string;
  state: State;
  worstState: State;
  worstAt: number;
  rulesVersion: number;
  moduleCount: number;
  unknownCount: number;
  matched: Matched[];
  appVersion: string;
  updatedAt: number;
}

/** One file, as it has been seen across everyone. */
export interface SeenRow {
  accountId: string;
  riderName: string;
  serverId: string;
  name: string;
  sha256: string;
  origin: string;
  state: State;
  label: string;
  firstAt: number;
  lastAt: number;
  hits: number;
  /** How many distinct accounts have ever loaded this exact file. */
  accounts: number;
}

export interface AdminView {
  live: LiveRow[];
  seen: SeenRow[];
  rules: ModuleRule[];
  rulesVersion: number;
  reporting: number;
}

/**
 * Everything the admin page draws.
 *
 * `live` is who is reporting right now, worst first. `seen` is the evidence log — every
 * non-system file anyone's game has loaded that the rules do not account for, most recent
 * first, with the number of accounts that have ever loaded it. That last number is the one
 * that reads best: a file on one machine out of hundreds is interesting whatever any rule
 * says, and a file on all of them is a driver.
 */
export async function collectAdminView(env: Env, days: number): Promise<AdminView> {
  const fresh = Date.now() - PRESENCE_TTL_MS;
  const since = Date.now() - days * 24 * 60 * 60 * 1000;

  const live = await env.DB.prepare(
    "SELECT c.account_id, a.rider_name, a.guid, c.state, c.rules_version, c.module_count," +
      " c.unknown_count, c.matched, c.worst_state, c.worst_at, c.app_version, c.updated_at," +
      " (SELECT p.server_id FROM presence p WHERE p.account_id = c.account_id" +
      "  AND p.updated_at > ?) AS server_id" +
      " FROM client_modules c JOIN accounts a ON a.id = c.account_id" +
      " WHERE c.updated_at > ?",
  )
    .bind(fresh, fresh)
    .all<{
      account_id: string;
      rider_name: string;
      guid: string | null;
      state: string;
      rules_version: number;
      module_count: number;
      unknown_count: number;
      matched: string;
      worst_state: string;
      worst_at: number;
      app_version: string;
      updated_at: number;
      server_id: string | null;
    }>();

  const seen = await env.DB.prepare(
    "SELECT s.account_id, s.rider_name, s.server_id, s.name, s.sha256, s.origin, s.state," +
      " s.label, s.first_at, s.last_at, s.hits," +
      " (SELECT COUNT(DISTINCT t.account_id) FROM client_module_seen t" +
      "  WHERE t.name = s.name AND t.sha256 = s.sha256) AS accounts" +
      " FROM client_module_seen s" +
      " WHERE s.state IN ('warn', 'alert') AND s.last_at > ?" +
      " ORDER BY s.last_at DESC LIMIT 500",
  )
    .bind(since)
    .all<{
      account_id: string;
      rider_name: string;
      server_id: string;
      name: string;
      sha256: string;
      origin: string;
      state: string;
      label: string;
      first_at: number;
      last_at: number;
      hits: number;
      accounts: number;
    }>();

  const { rules, version } = await loadRules(env);

  return {
    live: (live.results ?? [])
      .map((r) => ({
        accountId: r.account_id,
        riderName: r.rider_name,
        guid: r.guid ?? "",
        serverId: r.server_id ?? "",
        state: isState(r.state) ? r.state : "unknown",
        worstState:
          r.worst_at > fresh && isState(r.worst_state) ? r.worst_state : "unknown",
        worstAt: r.worst_at,
        rulesVersion: r.rules_version,
        moduleCount: r.module_count,
        unknownCount: r.unknown_count,
        matched: parseMatched(r.matched),
        appVersion: r.app_version ?? "",
        updatedAt: r.updated_at,
      }))
      .sort(
        (a, b) =>
          Math.max(stateRank(b.worstState), stateRank(b.state)) -
            Math.max(stateRank(a.worstState), stateRank(a.state)) ||
          b.updatedAt - a.updatedAt,
      ),
    seen: (seen.results ?? []).map((r) => ({
      accountId: r.account_id,
      riderName: r.rider_name ?? "",
      serverId: r.server_id ?? "",
      name: r.name,
      sha256: r.sha256,
      origin: r.origin,
      state: isState(r.state) ? r.state : "unknown",
      label: r.label ?? "",
      firstAt: r.first_at,
      lastAt: r.last_at,
      hits: r.hits,
      accounts: r.accounts,
    })),
    rules,
    rulesVersion: version,
    reporting: (live.results ?? []).length,
  };
}

/** A stored JSON column read back. A row that will not parse is empty, never a 500. */
function parseMatched(text: string): Matched[] {
  try {
    const value = JSON.parse(text);
    if (!Array.isArray(value)) return [];
    return value
      .filter((v) => v && typeof v === "object")
      .map((v) => ({
        name: String((v as Matched).name ?? ""),
        sha256: String((v as Matched).sha256 ?? ""),
        label: String((v as Matched).label ?? ""),
      }))
      .filter((v) => v.name);
  } catch {
    return [];
  }
}

/**
 * Add a rule.
 *
 * Takes effect on the next report from every install. Nothing re-reads the past: a rule
 * added now says what the next sighting means, and the row that prompted it keeps whatever
 * it was recorded as — which is the honest record of what was known at the time.
 */
export async function addRule(
  env: Env,
  kind: string,
  pattern: string,
  sha256: string,
  label: string,
  note: string,
): Promise<{ ok: boolean; error?: string }> {
  if (kind !== "deny" && kind !== "allow") return { ok: false, error: "kind must be deny or allow" };
  const cleanPattern = pattern.trim().toLowerCase().slice(0, 96);
  const cleanHash = sha256.trim().toLowerCase();
  if (cleanHash && !isSha256(cleanHash)) return { ok: false, error: "that is not a sha256" };
  if (!cleanPattern && !cleanHash) return { ok: false, error: "a name or a hash is required" };
  // One or the other. A rule carrying both reads as "this name with this hash" to whoever
  // writes it and as "this name, or this hash" to the matcher, and the gap between those is
  // where a wrong accusation comes from.
  if (cleanPattern && cleanHash) return { ok: false, error: "a name or a hash, not both" };
  await env.DB.prepare(
    "INSERT INTO module_rules (kind, pattern, sha256, label, note, created_at)" +
      " VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(kind, pattern, sha256) DO UPDATE SET" +
      " label = excluded.label, note = excluded.note",
  )
    .bind(kind, cleanPattern, cleanHash, label.trim().slice(0, 96), note.trim().slice(0, 256), Date.now())
    .run();
  return { ok: true };
}

/** Drop a rule by id. The version does not go backwards: `max(id)` is over what remains. */
export async function deleteRule(env: Env, id: number): Promise<void> {
  await env.DB.prepare("DELETE FROM module_rules WHERE id = ?").bind(id).run();
}

/** How long a sighting is kept. Long enough to describe a season, not a career. */
const RETENTION_DAYS = 90;

/**
 * Drop sightings nobody will read again.
 *
 * Runs on the same cron sweep as the other prunes. A failed sweep is the next sweep's
 * problem: it must never be able to take out a scheduled run that also does real work.
 */
export async function pruneReports(env: Env): Promise<void> {
  try {
    await env.DB.batch([
      env.DB.prepare("DELETE FROM client_module_seen WHERE last_at < ?").bind(
        Date.now() - RETENTION_DAYS * 24 * 60 * 60 * 1000,
      ),
      env.DB.prepare("DELETE FROM client_modules WHERE updated_at < ?").bind(
        Date.now() - RETENTION_DAYS * 24 * 60 * 60 * 1000,
      ),
    ]);
  } catch (err) {
    console.error(JSON.stringify({ msg: "report sweep failed", error: String(err) }));
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8" },
  });
}
