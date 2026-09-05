/**
 * What is loaded inside a player's running game.
 *
 * The app walks the module list of the running game and posts it here — file name, where it
 * came from, a hash for anything that is not a Windows system library, and what the file says
 * about itself: size, last written, whether Windows trusts its signature and who signed it,
 * and the company and product it claims. That is the whole of the client's job: it observes
 * and reports, and it holds no opinion about any file.
 *
 * The self-description is worth having because a name and a hash only identify what is
 * already known, and every first sighting is a name nobody recognises. `signed by NVIDIA
 * Corporation` and `unsigned, claims nothing` are the same length on the page and mean
 * opposite things. None of it is trusted: a version resource is text the file supplies about
 * itself and is treated exactly like any other string off a client. Only `trust` is checked,
 * because only Windows checked it.
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

import { isAppVersion, isGuid, isSha256, PRESENCE_TTL_MS } from "./validate";

export interface Account {
  id: string;
  rider_name: string;
  /** The player's MX Bikes GUID, once known. Filled from a report when we don't have it. */
  guid?: string | null;
}

/**
 * Where the client found this. Its judgement of location, not of character.
 *
 * The first four are where a *file* was loaded from. The last two are not files at all:
 * `memory` is executable memory in the game that no loaded module covers, and `disk` is a
 * file sitting in the game's `plugins` folder that the game has not loaded. Both are things
 * the module list is structurally unable to mention, which is why they are here.
 */
export type Origin = "game" | "system" | "app" | "other" | "memory" | "disk";

export type State = "unknown" | "ok" | "warn" | "alert";

/** Worst-last, so a report's state is the maximum over its modules. */
const STATE_RANK: Record<State, number> = { unknown: 0, ok: 1, warn: 2, alert: 3 };

export function stateRank(value: string): number {
  return STATE_RANK[value as State] ?? 0;
}

export function isState(value: unknown): value is State {
  return typeof value === "string" && value in STATE_RANK;
}

/**
 * What Windows made of a file's signature, as the client read it.
 *
 * `unchecked` is not `unsigned`: system libraries are never looked at, and neither is
 * anything under Wine, where there is no trust store to ask. Folding the two together would
 * report every Linux player's whole game as unsigned.
 */
export type Trust = "unchecked" | "unsigned" | "signed" | "untrusted";

const TRUSTS: readonly string[] = ["unchecked", "unsigned", "signed", "untrusted"];

export function isTrust(value: unknown): value is Trust {
  return typeof value === "string" && TRUSTS.includes(value);
}

export interface ReportedModule {
  name: string;
  origin: Origin;
  /** Empty for system libraries, which the client does not hash, and for unreadable files. */
  sha256: string;
  /** Bytes on disk; 0 when the client could not read the file. */
  size: number;
  /** Last written, seconds since the epoch; 0 when unknown. */
  mtime: number;
  trust: Trust;
  /** The signing certificate's display name. Only ever set when `trust` is `signed`. */
  publisher: string;
  /** `CompanyName` off the version resource — a claim, not a checked fact. */
  company: string;
  /** `ProductName`. */
  product: string;
  /** `FileDescription`, the field Explorer shows. */
  description: string;
  /**
   * One line about a row that is not a file: a region's protection and shape, or why a
   * plugin file is worth a row of its own. Empty for everything that came off the module
   * list, which the seven fields above already describe.
   */
  detail: string;
}

export interface ModuleRule {
  id: number;
  kind: "deny" | "allow";
  /** Lowercase substring of a file name. Empty when the rule matches by hash. */
  pattern: string;
  sha256: string;
  label: string;
  /** Why the rule exists. Written on the add form, and only ever read on the rules page. */
  note?: string;
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

/** The same bound the client already applies to everything it reads off a file. */
const MAX_FIELD = 96;

/** Bigger than any library, and the point past which a number is a client bug or a lie. */
const MAX_SIZE = 8 * 1024 * 1024 * 1024;

/** Seconds. Anything outside this is not a timestamp — 2000-01-01 to roughly 2100. */
const MIN_MTIME = 946_684_800;
const MAX_MTIME = 4_102_444_800;

const ORIGINS: readonly string[] = ["game", "system", "app", "other", "memory", "disk"];

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
    const e = entry as Record<string, unknown>;
    out.push({
      name: clean,
      origin: origin as Origin,
      sha256: hash,
      size: bounded(e.size, MAX_SIZE),
      mtime: stamp(e.mtime),
      // An absent or unrecognised trust reads as `unchecked` rather than rejecting the
      // report: an older client sends no such field at all, and a report that is thrown away
      // is worse than one that says less than it could.
      trust: isTrust(e.trust) ? e.trust : "unchecked",
      publisher: field(e.publisher),
      company: field(e.company),
      product: field(e.product),
      description: field(e.description),
      detail: "",
    });
  }
  return out;
}

/** The most regions and plugin files one report may carry. The client caps the same numbers. */
export const MAX_REGIONS = 24;
export const MAX_FILES = 32;

/** What the client says backs a region. Anything else is not a report we wrote. */
const REGION_KINDS: readonly string[] = ["image", "mapped", "private", "other"];

/** A page protection as letters — `rx`, `rwx`, `rwxc`. */
const PROTECT_SHAPE = /^[-rwxc?]{1,4}$/;

/**
 * A region with nothing to call itself.
 *
 * Shaped like a file name because everything downstream of here — the name column, the
 * search, the rule matcher — takes file names and refuses anything else. An unnamed region
 * is still worth a row: it is told apart from the others by its fingerprint, and a rule can
 * name that.
 */
const UNNAMED_REGION = "unnamed.region";

/** Three numbers about the game's threads. */
export interface ReportedThreads {
  total: number;
  foreign: number;
  breakpoints: number;
}

/**
 * Read the regions off a report, as rows.
 *
 * Parsed straight into the shape everything else is stored in, which is the point: a region
 * has a name, a hash and a place it was seen, so it is a row like any other and it goes
 * through the same rules, the same prevalence read and the same search. A rule naming a
 * fingerprint reads exactly like a rule naming a file hash, and nothing had to learn a new
 * kind of thing to make that work.
 *
 * `null` if this is not a region list. An absent one is not that — an app too old to look
 * sends no field at all — and the caller passes `[]` for it.
 */
export function parseRegions(value: unknown): ReportedModule[] | null {
  if (!Array.isArray(value)) return null;
  if (value.length > MAX_REGIONS) return null;
  const out: ReportedModule[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    if (!entry || typeof entry !== "object") return null;
    const e = entry as Record<string, unknown>;
    const kind = typeof e.kind === "string" ? e.kind : "";
    if (!REGION_KINDS.includes(kind)) return null;
    const hash = typeof e.sha256 === "string" ? e.sha256.trim().toLowerCase() : "";
    if (hash && !isSha256(hash)) return null;
    const protect = typeof e.protect === "string" && PROTECT_SHAPE.test(e.protect) ? e.protect : "";
    const image = e.image === true;
    const thread = e.thread === true;
    const name = shapedName(e.name);
    const pdb = shapedName(e.pdb);
    const label = name || pdb || UNNAMED_REGION;
    const key = `${label} ${hash}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({
      name: label,
      origin: "memory",
      sha256: hash,
      size: bounded(e.size, MAX_SIZE),
      // A run of bytes in memory was never written to a disk. Left at zero rather than
      // filled with the time it was noticed, which would read as a fact about the thing.
      mtime: 0,
      trust: "unchecked",
      publisher: "",
      company: "",
      product: "",
      description: "",
      detail: regionDetail(protect, kind, image, thread, label, pdb),
    });
  }
  return out;
}

/** The one line a region gets on the page. */
function regionDetail(
  protect: string,
  kind: string,
  image: boolean,
  thread: boolean,
  name: string,
  pdb: string,
): string {
  const parts = [[protect, kind, image ? "image" : ""].filter(Boolean).join(" ")];
  if (thread) parts.push("a thread starts here");
  if (pdb && pdb !== name) parts.push(pdb);
  return parts.filter(Boolean).join(" · ").slice(0, MAX_FIELD);
}

/**
 * Read the game's `plugins` folder off a report, as rows — keeping only what is *not*
 * loaded.
 *
 * The client sends the whole folder because it is the client's job to describe what it saw,
 * not to decide what matters. Storing the loaded ones again would be storing them twice:
 * they are already rows, from the module list, under the same name and the same hash, and
 * the two would fight over one primary key and flip its origin on every report.
 *
 * What is left is the reason the folder is read at all. MX Bikes loads every `.dlo` in there
 * at startup, so a file sitting in it that is *not* loaded is one the game refused, one that
 * crashed on load, or one waiting for the next launch — and none of those appear anywhere
 * else in a report.
 */
export function parseFiles(value: unknown): ReportedModule[] | null {
  if (!Array.isArray(value)) return null;
  if (value.length > MAX_FILES) return null;
  const out: ReportedModule[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    if (!entry || typeof entry !== "object") return null;
    const e = entry as Record<string, unknown>;
    const name = shapedName(e.name);
    if (!name) return null;
    const hash = typeof e.sha256 === "string" ? e.sha256.trim().toLowerCase() : "";
    if (hash && !isSha256(hash)) return null;
    if (e.loaded === true) continue;
    const key = `${name} ${hash}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({
      name,
      origin: "disk",
      sha256: hash,
      size: bounded(e.size, MAX_SIZE),
      mtime: stamp(e.mtime),
      trust: isTrust(e.trust) ? e.trust : "unchecked",
      publisher: field(e.publisher),
      company: field(e.company),
      product: field(e.product),
      description: field(e.description),
      detail: "in the plugins folder, not loaded",
    });
  }
  return out;
}

/**
 * Three numbers about the game's threads, or zeroes.
 *
 * Never refuses a report: an app too old to count threads sends no field, and a report
 * thrown away over a number is worse than one that says less than it could. The two that
 * matter are `foreign` — threads that started somewhere no module covers — and
 * `breakpoints`, threads carrying an armed hardware breakpoint, which is how a function is
 * hooked without altering a byte of it.
 */
export function parseThreads(value: unknown): ReportedThreads {
  if (!value || typeof value !== "object") return { total: 0, foreign: 0, breakpoints: 0 };
  const e = value as Record<string, unknown>;
  const cap = 4096;
  return {
    total: bounded(e.total, cap),
    foreign: bounded(e.foreign, cap),
    breakpoints: bounded(e.breakpoints, cap),
  };
}

/** A file name off a client, or empty. The same shape the module list is held to. */
function shapedName(value: unknown): string {
  if (typeof value !== "string") return "";
  const clean = value.trim().toLowerCase();
  if (!clean || clean.length > MAX_NAME) return "";
  return NAME_SHAPE.test(clean) ? clean : "";
}

/** One string a file said about itself: trimmed, bounded, and stripped of control characters. */
function field(value: unknown): string {
  if (typeof value !== "string") return "";
  // eslint-disable-next-line no-control-regex
  return value.replace(/[\u0000-\u001f\u007f]/g, "").trim().slice(0, MAX_FIELD);
}

/** A non-negative integer inside a bound, or 0. */
function bounded(value: unknown, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  const n = Math.trunc(value);
  return n > 0 && n <= max ? n : 0;
}

/** A plausible epoch-seconds timestamp, or 0. */
function stamp(value: unknown): number {
  const n = bounded(value, MAX_MTIME);
  return n >= MIN_MTIME ? n : 0;
}

/**
 * What the rules make of one module list.
 *
 * The order is the policy, and it is deliberate:
 *
 *   1. **Deny first.** A named file is named wherever it loaded from — something that copies
 *      itself into the game's folder must not be trusted for being there.
 *   2. **Allow next**, so a false positive is silenced by adding one row, not by a release.
 *   3. **Anything in the game's own folder.** Sitting beside the executable is the cheapest
 *      way into a process, not a credential — and this is the hole the first version left:
 *      a file dropped next to `mxbikes.exe` under an ordinary name read as accounted for
 *      because of where it was. The game's own libraries land here too and are cleared once,
 *      by hash, from the admin page.
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
    if (isUnaccounted(mod)) unknown.push(mod);
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
    "SELECT id, kind, pattern, sha256, label, note FROM module_rules ORDER BY id",
  ).all<{
    id: number;
    kind: string;
    pattern: string;
    sha256: string;
    label: string;
    note: string | null;
  }>();
  const rules = (rows.results ?? [])
    .filter((r) => r.kind === "deny" || r.kind === "allow")
    .map((r) => ({
      id: r.id,
      kind: r.kind as "deny" | "allow",
      pattern: (r.pattern ?? "").toLowerCase(),
      sha256: (r.sha256 ?? "").toLowerCase(),
      label: r.label ?? "",
      note: r.note ?? "",
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
  const { modules, appVersion, available, guid, regions, threads, files } = body as Record<
    string,
    unknown
  >;

  // Tie the report to the player, not just the install. The game already publishes this GUID
  // to every server the player joins, so it is the identity the rest of the system keys on.
  // Fill-only: a GUID claimed elsewhere (a server log, the app) is left as it stands.
  if (isGuid(guid) && !(account.guid ?? "").trim()) {
    await env.DB.prepare(
      "UPDATE accounts SET guid = ? WHERE id = ? AND (guid IS NULL OR guid = '')",
    )
      .bind((guid as string).trim(), account.id)
      .run();
  }

  const now = Date.now();
  const version = isAppVersion(appVersion) ? appVersion : "";

  // The client saying it could not read the list. A real answer, stored as one: it is the
  // difference between "nothing was loaded" and "we were not allowed to look", and an app
  // running below the game's integrity level reports it every pass.
  if (available === false) {
    await store(env, account, "unknown", 0, 0, 0, [], [], NO_THREADS, 0, version, now);
    return json(200, { ok: true });
  }

  const list = parseModules(modules);
  if (!list) return json(400, { error: "that is not a module list" });

  // Absent rather than empty is an app too old to have looked, and is not an error. Present
  // and malformed is, and is refused the same way a bad module list is.
  const found = parseRegions(regions ?? []);
  if (!found) return json(400, { error: "that is not a region list" });
  const plugins = parseFiles(files ?? []);
  if (!plugins) return json(400, { error: "that is not a plugins folder" });
  const counted = parseThreads(threads);

  const { rules, version: rulesVersion } = await loadRules(env);
  // One list, because they are one question. A region and a file in the plugins folder are
  // both "something is in the game that we cannot account for", and the rules that read a
  // module read them without knowing there was ever a difference.
  const verdict = classify([...list, ...found, ...plugins], rules);
  await store(
    env,
    account,
    withThreads(verdict.state, counted),
    rulesVersion,
    list.length,
    verdict.unknown.length,
    verdict.matched,
    // System libraries are not recorded per-file: hundreds of them, identical everywhere,
    // and they would bury the rows worth reading.
    [...list.filter((m) => m.origin !== "system"), ...found, ...plugins],
    counted,
    found.length,
    version,
    now,
  );
  return json(200, { ok: true });
}

/** What an app that could not look reports about threads. */
const NO_THREADS: ReportedThreads = { total: 0, foreign: 0, breakpoints: 0 };

/**
 * Raise a report's state for what the thread counts say.
 *
 * Not `alert`, ever, and deliberately: a thread that started outside every loaded module is
 * the strongest thing in a report that still has an innocent explanation — a debugger is
 * attached, an overlay nobody has listed yet did something unusual — and `warn` is what this
 * page means by "something here is not accounted for". Only a rule names a thing.
 */
export function withThreads(state: State, threads: ReportedThreads): State {
  if (state === "unknown" || state === "alert") return state;
  return threads.foreign > 0 || threads.breakpoints > 0 ? "warn" : state;
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
  threads: ReportedThreads,
  regionCount: number,
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
        " unknown_count, matched, worst_state, worst_at, app_version, updated_at," +
        " region_count, foreign_threads, breakpoints)" +
        " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)" +
        " ON CONFLICT(account_id) DO UPDATE SET state = excluded.state," +
        " rules_version = excluded.rules_version, module_count = excluded.module_count," +
        " unknown_count = excluded.unknown_count, matched = excluded.matched," +
        " worst_state = excluded.worst_state, worst_at = excluded.worst_at," +
        " app_version = excluded.app_version, updated_at = excluded.updated_at," +
        " region_count = excluded.region_count," +
        " foreign_threads = excluded.foreign_threads, breakpoints = excluded.breakpoints",
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
      regionCount,
      threads.foreign,
      threads.breakpoints,
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
          " rider_name, server_id, first_at, last_at, hits," +
          " size, mtime, trust, publisher, company, product, description, detail)" +
          " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?)" +
          " ON CONFLICT(account_id, name, sha256) DO UPDATE SET origin = excluded.origin," +
          " state = excluded.state, label = excluded.label, rider_name = excluded.rider_name," +
          " server_id = excluded.server_id, last_at = excluded.last_at, hits = hits + 1," +
          " size = excluded.size, mtime = excluded.mtime, trust = excluded.trust," +
          " publisher = excluded.publisher, company = excluded.company," +
          " product = excluded.product, description = excluded.description," +
          " detail = excluded.detail",
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
        mod.size,
        mod.mtime,
        mod.trust,
        mod.publisher,
        mod.company,
        mod.product,
        mod.description,
        mod.detail,
      ),
    );
  }

  await env.DB.batch(statements);
}

/**
 * Does this module need a rule to account for it?
 *
 * `system` is the only origin that accounts for itself, and it does so by where it is: a
 * Windows system folder is not writable without administrator rights, so a file there was
 * put there by something that already had the machine. `app` is our own install folder,
 * which we put every file in ourselves.
 *
 * Everything else — the game's folder included — needs a rule. `LOADER_NAMES` no longer
 * changes the answer for the game's folder, because the whole folder is asked about now; it
 * stays because those names matter wherever else they turn up.
 */
export function isUnaccounted(mod: ReportedModule): boolean {
  if (mod.origin === "system") return false;
  if (mod.origin === "app") return LOADER_NAMES.includes(mod.name);
  // `memory` and `disk` fall through to the same answer the game's own folder gets, and for
  // the same reason: being there is not a credential. Executable memory nothing loaded, and
  // a file in the folder the game loads plugins from, both need a rule to be accounted for
  // — and an `allow` rule on the fingerprint is how one stops being asked about.
  return true;
}

/** Was this module one of the ones nothing accounted for in the report it arrived in? */
function unknownState(mod: ReportedModule, reportState: State): State {
  if (isUnaccounted(mod)) return "warn";
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
  /** Executable memory in the game that no loaded module covered, when this was reported. */
  regionCount: number;
  /** Threads that started somewhere no loaded module covers. */
  foreignThreads: number;
  /** Threads carrying an armed hardware breakpoint. */
  breakpoints: number;
  appVersion: string;
  updatedAt: number;
}

export interface AdminView {
  live: LiveRow[];
  rules: ModuleRule[];
  rulesVersion: number;
  reporting: number;
}

/**
 * Who is reporting right now, worst first, and the rules that read them.
 *
 * The file side of the page lives in `diagnosticssearch.ts`: it is a search now rather than
 * a fixed list, and the two have nothing in common but the tables.
 */
export async function collectAdminView(env: Env): Promise<AdminView> {
  const fresh = Date.now() - PRESENCE_TTL_MS;

  const live = await env.DB.prepare(
    "SELECT c.account_id, a.rider_name, a.guid, c.state, c.rules_version, c.module_count," +
      " c.unknown_count, c.matched, c.worst_state, c.worst_at, c.app_version, c.updated_at," +
      " c.region_count, c.foreign_threads, c.breakpoints," +
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
      region_count: number | null;
      foreign_threads: number | null;
      breakpoints: number | null;
      server_id: string | null;
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
        regionCount: r.region_count ?? 0,
        foreignThreads: r.foreign_threads ?? 0,
        breakpoints: r.breakpoints ?? 0,
        appVersion: r.app_version ?? "",
        updatedAt: r.updated_at,
      }))
      .sort(
        (a, b) =>
          Math.max(stateRank(b.worstState), stateRank(b.state)) -
            Math.max(stateRank(a.worstState), stateRank(a.state)) ||
          b.updatedAt - a.updatedAt,
      ),
    rules,
    rulesVersion: version,
    reporting: (live.results ?? []).length,
  };
}

/** A stored JSON column read back. A row that will not parse is empty, never a 500. */
export function parseMatched(text: string): Matched[] {
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
