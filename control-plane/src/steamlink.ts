/**
 * The record that a Steam link happened, kept apart from `accounts.steam_id`.
 *
 * `accounts.steam_id` is a single mutable cell. Anything that clears it — a stray statement,
 * a restore from an older copy — silently unlinks a player, and nothing afterwards can tell
 * "never linked" from "was linked until something dropped it": both are NULL, and every
 * entitlement call is refused with "no Steam account linked" as if the player had never
 * signed in. That is not hypothetical; it has already happened to a link Valve confirmed.
 *
 * So every confirmed link is also written to `steam_links`, in the same batch as the link
 * itself, and nothing in this worker deletes a row there. The paths that need an identity ask
 * through here, and a link Valve has already confirmed is put back rather than refused.
 */

import { rememberGuid } from "./bans";
import { guidFromSteamId, isSteamId64 } from "./steam";

/**
 * Remember a link Valve confirmed.
 *
 * Returned unrun so the caller can put it in the same `batch` as the account update: the log
 * and the column must not be able to disagree because one of two writes failed.
 */
export function rememberLink(env: Env, accountId: string, steamId: string): D1PreparedStatement {
  return env.DB.prepare(
    "INSERT INTO steam_links (account_id, steam_id, linked_at) VALUES (?, ?, ?)" +
      " ON CONFLICT (account_id, steam_id) DO UPDATE SET linked_at = excluded.linked_at",
  ).bind(accountId, steamId, Date.now());
}

/**
 * Put back a link the log knows about but `accounts` has lost.
 *
 * Deliberately conservative, because `accounts.steam_id` is UNIQUE and this runs without a
 * human watching. It writes only when the row it would fill is still NULL *and* no other
 * account currently holds that identity — if one does, that row is the present owner and
 * deciding between them is not something to do automatically. Both conditions are in the
 * statement rather than checked first, so two requests racing cannot both think they won.
 *
 * Returns whether a link was restored.
 */
async function restore(env: Env, where: string, binds: string[]): Promise<boolean> {
  const result = await env.DB.prepare(
    "UPDATE accounts SET steam_id = (SELECT steam_id FROM steam_links WHERE " +
      where +
      " ORDER BY linked_at DESC LIMIT 1)" +
      " WHERE steam_id IS NULL AND id = (SELECT account_id FROM steam_links WHERE " +
      where +
      " ORDER BY linked_at DESC LIMIT 1)" +
      " AND NOT EXISTS (SELECT 1 FROM accounts WHERE steam_id = (SELECT steam_id FROM steam_links WHERE " +
      where +
      " ORDER BY linked_at DESC LIMIT 1))",
  )
    .bind(...binds, ...binds, ...binds)
    .run();
  return (result.meta.changes ?? 0) > 0;
}

/**
 * This account's Steam identity, restored from the log if the column has been lost.
 *
 * Two things that are not the same are kept apart here. Writing the column back is
 * conservative — [`restore`] refuses while anybody else holds the value, because picking a
 * winner for a unique cell is not a decision to make unattended. *Answering* is not: the log
 * holds links Valve confirmed for this very account, and one of them being written in another
 * account's column does not make it less true. Two installs of one person — a second PC, a
 * reinstall that claimed a fresh device account — are exactly that case, and refusing to name
 * their identity is what walls them out of every app in the lineup under `MXB_REQUIRE_STEAM`.
 * Nothing downstream is keyed on which row holds the cell: entitlements are keyed on the
 * SteamID64 itself, and the ban resolution widens through `steam_links` already.
 *
 * Null only when the account has genuinely never linked — which stays a normal answer, not an
 * error: an unlinked account simply owns nothing yet.
 */
export async function steamIdFor(
  env: Env,
  account: { id: string; steam_id: string | null },
): Promise<string | null> {
  if (account.steam_id) return account.steam_id;
  if (await restore(env, "account_id = ?", [account.id])) {
    const row = await env.DB.prepare("SELECT steam_id FROM accounts WHERE id = ?")
      .bind(account.id)
      .first<{ steam_id: string | null }>();
    const restored = row?.steam_id ?? null;
    if (restored && isSteamId64(restored)) return restored;
  }
  // The column could not be filled — most often because another install of this same person is
  // holding the value. The confirmed link is still the answer.
  const linked = await env.DB.prepare(
    "SELECT steam_id FROM steam_links WHERE account_id = ? ORDER BY linked_at DESC LIMIT 1",
  )
    .bind(account.id)
    .first<{ steam_id: string | null }>();
  const steamId = linked?.steam_id ?? null;
  return steamId && isSteamId64(steamId) ? steamId : null;
}

/**
 * Put back the account behind a Steam identity, for the site — which signs people in with
 * Steam and so only ever knows this side of the link.
 *
 * Returns whether it restored one, so a caller that found nothing can retry its own lookup.
 */
export function repairBySteamId(env: Env, steamId: string): Promise<boolean> {
  if (!isSteamId64(steamId)) return Promise.resolve(false);
  return restore(env, "steam_id = ?", [steamId]);
}

/**
 * Pin an account's GUID to the one its Valve-confirmed Steam identity derives to.
 *
 * This is the auto-find the whole anti-spoof rests on: for a Steam copy of the game the GUID is
 * a pure function of the SteamID64 (`guidFromSteamId`), and the SteamID64 is the value Valve
 * just vouched for. So we do not ask the client what its GUID is — we compute it and store it.
 *
 * A derived GUID is unique per Steam account, but another row may already be holding this one:
 * a first-come claim from before the link, or a spoofer who claimed the victim's GUID. Valve's
 * word beats both, so the holder is cleared and this account takes it — in one batch, so the
 * unique index never sees the GUID on two rows at once. Recorded in `guid_claims` too, so the
 * ban resolution can follow it.
 *
 * Returns the derived GUID, or `null` for a non-Steam (Piboso) identity, whose GUID is its own
 * opaque value and cannot be derived — those stay first-come, corroborated by server sightings.
 */
export async function pinGuidFromSteam(
  env: Env,
  accountId: string,
  steamId: string,
): Promise<string | null> {
  const guid = guidFromSteamId(steamId);
  if (!guid) return null;
  await env.DB.batch([
    env.DB.prepare("UPDATE accounts SET guid = NULL WHERE guid = ? AND id <> ?").bind(guid, accountId),
    env.DB.prepare("UPDATE accounts SET guid = ? WHERE id = ?").bind(guid, accountId),
  ]);
  await rememberGuid(env, accountId, guid);
  return guid;
}
