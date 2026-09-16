/**
 * Who is banned from mxbsecure, and every identity that resolves to them.
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
 * At the gates of mxbsecure, and at as few of them as will cover it, so a product added behind
 * one inherits the ban rather than having to remember it: the entitlement decision
 * (`decideEntitlement`, which every key grant and check goes through), the status poll that
 * tells an app to delete a key it already holds, the creator authorization in `assets.ts`, the
 * site's creator signup and locker download, and the paid plugins — which are sold here too.
 *
 * It is deliberately *not* asked by voice, paint sync, presence or the server book. Those are
 * the MXB App's, not mxbsecure's, and they are worthless unless the riders beside you can use
 * them too — banning somebody from the grid everyone else is on punishes the grid. The scope of
 * a ban is what mxbsecure sells and protects: the locking system, the content behind it, and
 * the paid plugins.
 */

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

/** The message a banned caller is given, wherever it is refused. */
export const BANNED = "this install is banned from mxbsecure";

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
 * handed. The hop matters for the most common caller of all — a bearer token, so only an account
 * id — because without it a second account on a banned Steam login would resolve to nothing but
 * its own fresh GUID. The final select is a primary-key hit per candidate against a table with a
 * handful of rows in it.
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

  const row = await env.DB.prepare(RESOLVE)
    .bind(accountId, steamId, guid)
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
