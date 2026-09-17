/**
 * Who is banned, and every identity that resolves to them.
 *
 * "Banned" means banned from all of it. MXB App, Studio, Coach, FrostMod and mxbsecure are one
 * brand and one company, so there is no version of this that bans somebody from the locking
 * system and leaves them the rest of the estate.
 *
 * A ban is recorded against an MX Bikes GUID (`0038_guid_bans.sql` says why that key and not
 * another), but a ban that only matched the GUID column on one account would be worth about a
 * reinstall. So nothing here asks "is this GUID banned". It asks the question a banned rider
 * cannot get a different answer to by making something new:
 *
 *   is any identity this caller can be tied to a banned one?
 *
 * The tie runs in both directions, through the two append-only logs beside the mutable
 * columns — `steam_links` for the Steam identity, `guid_claims` for the GUID:
 *
 * - the GUID in front of us, if we were handed one;
 * - the GUID a Steam identity *is*, derived from the SteamID64 (`guidFromSteamId`), because for
 *   a Steam copy the two are one value and the derivation needs no row to exist;
 * - every GUID the calling account holds or has ever claimed;
 * - every account that shares the caller's Steam identity, now or in the link log, and every
 *   GUID *those* accounts hold or have ever claimed.
 *
 * So: a second account on the same Steam login resolves through the Steam side. A new GUID
 * claimed by a banned account resolves through the claim log, which the rename cannot erase. A
 * new Steam account on the banned install resolves through the GUID it reports. What is left
 * is a genuinely fresh install on a fresh Steam account, which is a new identity by every
 * measure we have, and is the point at which detection is the anti-cheat's job rather than
 * this table's.
 *
 * ## Where this is asked
 *
 * At the three doors the estate comes through, never per feature. MXB App, Studio, Coach,
 * FrostMod and mxbsecure are one brand, and a ban is a ban from all of it:
 *
 *  1. **`route` in `index.ts`, straight after `authenticate`.** Every bearer-token endpoint —
 *     voice, paint sync, presence, the queue, the server registry, provisioning, the plugins,
 *     the key grants — is below that line, so all of them are refused by position and anything
 *     added later inherits the refusal. A short closed list (`bannedMayUse`) names the few that
 *     stay open, and the gate itself says why each one does.
 *  2. **`assets.ts`'s `authorize`,** for the creator surface, which arrives on a sign-in cookie
 *     or a creator API key rather than an account token.
 *  3. **`web.ts`,** for the site: the signed-in identity on mxbsecure.com, its creator signup,
 *     and the locker download.
 *
 * `decideEntitlement` also asks, though the gate already covers its routes, because the answer
 * there is not a refusal but a *reason* — written to the audit ledger, and returned to the app
 * so a buyer is told why a file they paid for stopped opening.
 *
 * What a ban cannot reach is the handful of endpoints that carry no identity at all: anonymous
 * usage counters, the master-server probe, the shared server book, a live share code, an
 * unenrolled track generation. There is nothing there to match a ban against, and inventing
 * something to match would mean identifying everybody else too.
 */

import { guidFromSteamId } from "./steam";
import { isGuid } from "./validate";

/** A live ban, as the gates and the admin page read it. */
export interface Ban {
  guid: string;
  reason: string;
  evidence: string | null;
  altOf: string | null;
  bannedAt: number;
  bannedBy: string;
}

/** A ban row as stored, live or lifted — what the admin list shows. */
export interface BanRow extends Ban {
  liftedAt: number | null;
  liftedBy: string | null;
  liftedNote: string | null;
  /** Accounts we can currently tie to this GUID, so a ban isn't a bare hex string on a page. */
  accounts: { accountId: string; riderName: string; steamId: string | null; current: boolean }[];
}

/** What a caller can be identified by. Any subset; nulls and blanks are simply ignored. */
export interface Who {
  accountId?: string | null;
  steamId?: string | null;
  guid?: string | null;
}

/**
 * The honest refusal, used on the **website** only.
 *
 * mxbsecure.com is where a creator signs in, and where an appeal starts, so there the door is
 * named: a person who has to argue a ban should be told it is one. The site's own card
 * (`/v1/web/me` → `banned`/`banReason`) is the fuller version; this is the one-line 403.
 */
export const BANNED = "this install is banned from mxbsecure";

/**
 * What the **app** is told instead — a plausible, mundane failure, never the word "ban".
 *
 * The app is not a place to argue; it is a place a pirate is trying to keep using. Telling it
 * "banned" only says "make another account", and the honest message we show a creator is
 * exactly the coaching a content thief would act on. So the desktop apps are handed a
 * verification/integrity failure: it reads as an ordinary broken install, and the fix it names
 * — reinstall — cannot work, because a ban follows the GUID, the Steam login and the install,
 * not the files on disk. We know it is a ban (the ledger, the admin page and the internal
 * `reason` all say so); the machine in front of the person does not.
 *
 * One sentence, identical across MXB App, Studio, Coach and FrostMod, so it never reads as a
 * bug specific to one of them.
 */
export const APP_BLOCK_MESSAGE =
  "This copy couldn't be verified. It may be out of date or damaged — reinstall the latest version from mxbsecure.com.";

/**
 * What the app is told when a deployment requires a Steam sign-in and this install has none.
 *
 * Honest, unlike the ban message: this is a requirement to meet, not a refusal to hide. The app
 * shows it above a "Sign in with Steam" button and unlocks the moment Valve confirms the account.
 */
export const APP_SIGNIN_MESSAGE = "Sign in with Steam to use MXB App.";

/**
 * The gate verdict the desktop apps read:
 *  - `ok` — run;
 *  - `signin` — a Steam sign-in is required first (honest; the app prompts and retries);
 *  - `unsupported` — refuse (the disguised ban).
 */
export type AppGate =
  | { status: "ok" }
  | { status: "signin"; message: string }
  | { status: "unsupported"; message: string };

/** How a GUID is written down: trimmed and upper-cased, or null if it isn't one. */
export function normalizeGuid(raw: unknown): string | null {
  if (!isGuid(raw)) return null;
  return raw.trim().toUpperCase();
}

const MAX_REASON = 200;
const MAX_EVIDENCE = 2000;

/**
 * Remember that an account used a GUID.
 *
 * Called from both places a GUID arrives — a claim and a diagnostics report — because either
 * is a sighting, and the point of the log is that a later rename cannot take it back. Never
 * throws: a missing sighting must not fail the request that carried it. The account row has to
 * exist (the foreign key says so), which is true of every caller: they authenticated as it.
 */
export async function rememberGuid(env: Env, accountId: string, raw: unknown): Promise<void> {
  const guid = normalizeGuid(raw);
  if (!guid) return;
  const now = Date.now();
  try {
    await env.DB.prepare(
      "INSERT INTO guid_claims (account_id, guid, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?)" +
        " ON CONFLICT (account_id, guid) DO UPDATE SET last_seen_at = excluded.last_seen_at",
    )
      .bind(accountId, guid, now, now)
      .run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "guid sighting not recorded", error: String(err) }));
  }
}

/**
 * The resolution, as one statement: `?1` the account id, `?2` the Steam id, `?3` a normalised
 * GUID, any of them null.
 *
 * One round trip rather than three, because `POST /v1/assets/status` asks this on every poll
 * from every install that holds a secured file — the answer is cheap, and it should stay cheap
 * for the overwhelming majority of callers who are not banned.
 *
 * It widens in one hop and then stops: `seed` is the accounts named directly, `steams` the Steam
 * identities those accounts hold or have held, `ids` every account reachable through them, and
 * `guids` every GUID any of those accounts hold or have ever claimed, plus the one we were
 * handed, plus `?4`, the GUID the Steam identity itself derives to. The hop matters for the most
 * common caller of all — a bearer token, so only an account id — because without it a second
 * account on a banned Steam login would resolve to nothing but its own fresh GUID. `?4` matters
 * for the caller with no row at all: somebody signing in to the website on a banned Steam
 * account that never had an MXB App profile, or no longer has one, resolves through nothing in
 * the database — and for a Steam copy their GUID is a pure function of the SteamID Valve just
 * confirmed, so it is known without being stored. The final select is a primary-key hit per
 * candidate against a table with a handful of rows in it.
 */
const RESOLVE =
  "WITH seed AS (" +
  "  SELECT id AS account_id FROM accounts WHERE (?1 IS NOT NULL AND id = ?1) OR (?2 IS NOT NULL AND steam_id = ?2)" +
  "  UNION SELECT account_id FROM steam_links WHERE ?2 IS NOT NULL AND steam_id = ?2" +
  "), steams AS (" +
  "  SELECT ?2 AS steam_id WHERE ?2 IS NOT NULL" +
  "  UNION SELECT steam_id FROM accounts WHERE steam_id IS NOT NULL AND id IN (SELECT account_id FROM seed)" +
  "  UNION SELECT steam_id FROM steam_links WHERE account_id IN (SELECT account_id FROM seed)" +
  "), ids AS (" +
  "  SELECT account_id FROM seed" +
  "  UNION SELECT id FROM accounts WHERE steam_id IN (SELECT steam_id FROM steams)" +
  "  UNION SELECT account_id FROM steam_links WHERE steam_id IN (SELECT steam_id FROM steams)" +
  "), guids AS (" +
  "  SELECT ?3 AS guid WHERE ?3 IS NOT NULL" +
  "  UNION SELECT ?4 WHERE ?4 IS NOT NULL" +
  "  UNION SELECT UPPER(TRIM(guid)) FROM accounts WHERE guid IS NOT NULL AND id IN (SELECT account_id FROM ids)" +
  "  UNION SELECT guid FROM guid_claims WHERE account_id IN (SELECT account_id FROM ids)" +
  ")" +
  " SELECT guid, reason, evidence, alt_of, banned_at, banned_by FROM guid_bans" +
  " WHERE lifted_at IS NULL AND guid IN (SELECT guid FROM guids)" +
  " ORDER BY banned_at, guid LIMIT 1";

/**
 * The live ban that applies to this caller, or null.
 *
 * One row even when several match — it is the answer to "may this person use mxbsecure", and
 * the first ban is as final as the fifth. The oldest is returned so the reason a rider is
 * shown is the one they were first banned for rather than whichever alt was noticed last.
 */
export async function banFor(env: Env, who: Who): Promise<Ban | null> {
  const accountId = who.accountId?.trim() || null;
  const steamId = who.steamId?.trim() || null;
  const guid = normalizeGuid(who.guid);
  // Nothing to go on is not a ban. Said here so the statement below never runs with three
  // nulls, which would scan for a GUID that is nobody's.
  if (!accountId && !steamId && !guid) return null;

  // For a Steam copy the GUID is the SteamID written in hex (`0039_derive_guids.sql`), so a
  // banned Steam login is a banned GUID with or without a row to join through.
  const derived = steamId ? guidFromSteamId(steamId) : null;
  const row = await env.DB.prepare(RESOLVE)
    .bind(accountId, steamId, guid, derived)
    .first<{
      guid: string;
      reason: string;
      evidence: string | null;
      alt_of: string | null;
      banned_at: number;
      banned_by: string;
    }>();
  return row
    ? {
        guid: row.guid,
        reason: row.reason,
        evidence: row.evidence,
        altOf: row.alt_of,
        bannedAt: row.banned_at,
        bannedBy: row.banned_by,
      }
    : null;
}

/** Whether a ban applies, for the callers that only need the yes or no. */
export async function isBanned(env: Env, who: Who): Promise<boolean> {
  return (await banFor(env, who)) !== null;
}

/**
 * The verdict the desktop apps' startup gate reads — disguised on purpose.
 *
 * A banned install is told `unsupported` with the mundane message, never that it is banned; a
 * clean one is told `ok`. This is the whole of what makes the app refuse to run for a banned
 * rider without handing them the reason. The ban is still logged and still visible to us — see
 * `APP_BLOCK_MESSAGE` for why the person is not.
 */
export async function appGate(env: Env, who: Who): Promise<AppGate> {
  const ban = await banFor(env, who);
  if (!ban) return { status: "ok" };
  console.log(JSON.stringify({ msg: "app blocked", ban: ban.guid, reason: ban.reason }));
  return { status: "unsupported", message: APP_BLOCK_MESSAGE };
}

/**
 * Every ban, live ones first, each with the accounts we can currently tie to it.
 *
 * The accounts are what makes the page reviewable: a list of bare GUIDs says nothing about who
 * was refused, and the admin reading it after an appeal needs to see the rider name and the
 * Steam account the ban is actually stopping.
 */
export async function listBans(env: Env): Promise<BanRow[]> {
  const bans = await env.DB.prepare(
    "SELECT guid, reason, evidence, alt_of, banned_at, banned_by, lifted_at, lifted_by, lifted_note" +
      " FROM guid_bans ORDER BY lifted_at IS NOT NULL, banned_at DESC",
  ).all<{
    guid: string;
    reason: string;
    evidence: string | null;
    alt_of: string | null;
    banned_at: number;
    banned_by: string;
    lifted_at: number | null;
    lifted_by: string | null;
    lifted_note: string | null;
  }>();
  const rows = bans.results ?? [];
  if (rows.length === 0) return [];

  // One read for the whole page: the accounts holding any banned GUID now, and the accounts
  // that ever claimed one.
  const placeholders = rows.map(() => "?").join(",");
  const guids = rows.map((r) => r.guid);
  const tied = await env.DB.prepare(
    `SELECT UPPER(TRIM(a.guid)) AS guid, a.id, a.rider_name, a.steam_id, 1 AS current FROM accounts a` +
      ` WHERE UPPER(TRIM(a.guid)) IN (${placeholders})` +
      ` UNION SELECT c.guid, a.id, a.rider_name, a.steam_id, 0 FROM guid_claims c` +
      ` JOIN accounts a ON a.id = c.account_id WHERE c.guid IN (${placeholders})`,
  )
    .bind(...guids, ...guids)
    .all<{ guid: string; id: string; rider_name: string; steam_id: string | null; current: number }>();

  const byGuid = new Map<string, BanRow["accounts"]>();
  for (const r of tied.results ?? []) {
    const list = byGuid.get(r.guid) ?? [];
    // The same account can arrive twice — holding the GUID and having claimed it. Keep one,
    // and let "holds it now" win.
    const seen = list.find((a) => a.accountId === r.id);
    if (seen) seen.current = seen.current || r.current === 1;
    else list.push({ accountId: r.id, riderName: r.rider_name, steamId: r.steam_id, current: r.current === 1 });
    byGuid.set(r.guid, list);
  }

  return rows.map((r) => ({
    guid: r.guid,
    reason: r.reason,
    evidence: r.evidence,
    altOf: r.alt_of,
    bannedAt: r.banned_at,
    bannedBy: r.banned_by,
    liftedAt: r.lifted_at,
    liftedBy: r.lifted_by,
    liftedNote: r.lifted_note,
    accounts: byGuid.get(r.guid) ?? [],
  }));
}

/**
 * Ban a GUID, or re-ban one whose ban was lifted.
 *
 * `by` is the admin's Steam ID, recorded on the row: a ban is a decision somebody made, and
 * "who" is the first thing asked about it afterwards. A reason is required for the same reason
 * — the rider is shown it, and an appeal is judged against it.
 *
 * Re-banning a live GUID is not an error and does not move the original date: the ban already
 * says what it says, and a second click on a page somebody reloaded must not quietly rewrite
 * the reason it was first applied under.
 */
export async function addBan(
  env: Env,
  input: { guid: unknown; reason: unknown; evidence?: unknown; altOf?: unknown },
  by: string,
): Promise<{ ok: true; guid: string; already: boolean } | { ok: false; error: string }> {
  const guid = normalizeGuid(input.guid);
  if (!guid) return { ok: false, error: "that doesn't look like an MX Bikes GUID" };
  const altOf = input.altOf === undefined || input.altOf === null || input.altOf === "" ? null : normalizeGuid(input.altOf);
  if (input.altOf && !altOf) return { ok: false, error: "the GUID it is an alt of doesn't look like one" };
  if (altOf === guid) return { ok: false, error: "a GUID cannot be an alt of itself" };
  const reason = typeof input.reason === "string" ? input.reason.trim().slice(0, MAX_REASON) : "";
  if (!reason) return { ok: false, error: "say why, in a few words" };
  const evidence =
    typeof input.evidence === "string" && input.evidence.trim()
      ? input.evidence.trim().slice(0, MAX_EVIDENCE)
      : null;

  const live = await env.DB.prepare("SELECT lifted_at FROM guid_bans WHERE guid = ?")
    .bind(guid)
    .first<{ lifted_at: number | null }>();
  if (live && live.lifted_at === null) return { ok: true, guid, already: true };

  // A lifted ban is re-applied in place rather than as a second row: the GUID is the key, and
  // the lift columns are cleared so the row reads as what it now is. The evidence for the
  // earlier one lives in the new `evidence` if it still matters.
  await env.DB.prepare(
    "INSERT INTO guid_bans (guid, reason, evidence, alt_of, banned_at, banned_by) VALUES (?, ?, ?, ?, ?, ?)" +
      " ON CONFLICT (guid) DO UPDATE SET reason = excluded.reason, evidence = excluded.evidence," +
      " alt_of = excluded.alt_of, banned_at = excluded.banned_at, banned_by = excluded.banned_by," +
      " lifted_at = NULL, lifted_by = NULL, lifted_note = NULL",
  )
    .bind(guid, reason, evidence, altOf, Date.now(), by)
    .run();
  return { ok: true, guid, already: false };
}

/**
 * Lift a ban: the row stays, marked lifted, and stops being matched.
 *
 * Kept rather than deleted because a lifted ban is the only record that an appeal was upheld
 * — and the only thing that tells the next admin reading the same old report that it has
 * already been dealt with.
 */
export async function liftBan(
  env: Env,
  rawGuid: unknown,
  by: string,
  note?: unknown,
): Promise<{ ok: true; guid: string } | { ok: false; error: string }> {
  const guid = normalizeGuid(rawGuid);
  if (!guid) return { ok: false, error: "that doesn't look like an MX Bikes GUID" };
  const result = await env.DB.prepare(
    "UPDATE guid_bans SET lifted_at = ?, lifted_by = ?, lifted_note = ? WHERE guid = ? AND lifted_at IS NULL",
  )
    .bind(
      Date.now(),
      by,
      typeof note === "string" && note.trim() ? note.trim().slice(0, MAX_EVIDENCE) : null,
      guid,
    )
    .run();
  if ((result.meta.changes ?? 0) === 0) return { ok: false, error: "that GUID isn't banned" };
  return { ok: true, guid };
}
