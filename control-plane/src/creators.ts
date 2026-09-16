/**
 * Who may lock and sell through mxbsecure.com: the people who sign themselves up on the site,
 * and the ones the admin page adds by hand.
 *
 * Being a creator is `accounts.creator_at`. Someone with no MXB App account gets a `kind='web'`
 * row keyed on their Steam ID; when they later link Steam in the app, `steamReturn` hands that
 * row's creator standing and assets to the app account. Removing clears `creator_at` only: the
 * assets stay theirs, and their API keys stop working because key auth checks it too.
 *
 * `creator_source` says where the standing came from — 'self' for a signup on the site, 'admin'
 * for the creators page — so the list can be read at a glance now that anyone may join. It
 * decides nothing: every gate reads `creator_at` and only that.
 */

import { resolveSteamAccount } from "./assets";
import { hashToken, newToken } from "./auth";
import { steamPersonaName } from "./steam";
import { repairBySteamId } from "./steamlink";

/** How an account came by its creator standing. Null on the creators from before signup opened. */
export type CreatorSource = "self" | "admin";

export interface CreatorRow {
  accountId: string;
  riderName: string;
  steamId: string | null;
  steamName: string;
  /** False for a web-only row that no app account has claimed yet. */
  linked: boolean;
  creatorAt: number;
  /** Null for a creator invited before mxbsecure opened to signups. */
  source: CreatorSource | null;
  assets: number;
}

export async function listCreators(env: Env, fetchImpl: typeof fetch = fetch): Promise<CreatorRow[]> {
  const { results } = await env.DB.prepare(
    "SELECT a.id, a.rider_name, a.steam_id, a.kind, a.creator_at, a.creator_source," +
      " (SELECT COUNT(*) FROM assets s WHERE s.creator_id = a.id) AS assets" +
      " FROM accounts a WHERE a.creator_at IS NOT NULL ORDER BY a.creator_at DESC",
  ).all<{
    id: string;
    rider_name: string;
    steam_id: string | null;
    kind: string;
    creator_at: number;
    creator_source: string | null;
    assets: number;
  }>();
  // A handful of rows, and each name is edge-cached for a day.
  return Promise.all(
    results.map(async (r) => ({
      accountId: r.id,
      riderName: r.rider_name,
      steamId: r.steam_id,
      steamName: r.steam_id ? await steamPersonaName(r.steam_id, fetchImpl) : "",
      linked: r.kind !== "web",
      creatorAt: r.creator_at,
      source: r.creator_source === "self" || r.creator_source === "admin" ? r.creator_source : null,
      assets: r.assets,
    })),
  );
}

/**
 * Give a Steam account creator standing, making the web-only row for it if there isn't one.
 *
 * Idempotent: an account that is already a creator comes back `already`, with the source it
 * was first given left alone — how somebody joined is a fact about the past, and a second
 * signup does not change it.
 */
export async function makeCreator(
  env: Env,
  steamId: string,
  source: CreatorSource,
): Promise<{ accountId: string; already: boolean }> {
  const now = Date.now();
  const find = () =>
    env.DB.prepare("SELECT id, creator_at FROM accounts WHERE steam_id = ?")
      .bind(steamId)
      .first<{ id: string; creator_at: number | null }>();
  // A creator whose `steam_id` was lost looks like a stranger here and would get a second
  // account; the link log puts the column back first.
  const account = (await find()) ?? ((await repairBySteamId(env, steamId)) ? await find() : null);
  if (account) {
    if (account.creator_at) return { accountId: account.id, already: true };
    await env.DB.prepare("UPDATE accounts SET creator_at = ?, creator_source = ? WHERE id = ?")
      .bind(now, source, account.id)
      .run();
    return { accountId: account.id, already: false };
  }

  const id = crypto.randomUUID();
  try {
    await env.DB.prepare(
      "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at, kind, creator_at, creator_source)" +
        " VALUES (?, ?, ?, ?, ?, 'web', ?, ?)",
    )
      .bind(id, `web:${steamId}`, steamId, await hashToken(newToken()), now, now, source)
      .run();
  } catch (err) {
    // Two signups at once: the other one made the row.
    const again = await find();
    if (!again) throw err;
    return { accountId: again.id, already: true };
  }
  return { accountId: id, already: false };
}

export async function addCreator(
  env: Env,
  who: string,
  fetchImpl: typeof fetch = fetch,
): Promise<{ ok: true; accountId: string; steamId: string; already: boolean } | { ok: false; error: string }> {
  const resolved = await resolveSteamAccount(who, fetchImpl);
  if ("error" in resolved) return { ok: false, error: resolved.error };
  const { steamId } = resolved;
  const made = await makeCreator(env, steamId, "admin");
  return { ok: true, accountId: made.accountId, steamId, already: made.already };
}

/** False when the account wasn't a creator to begin with. */
export async function removeCreator(env: Env, accountId: string): Promise<boolean> {
  const res = await env.DB.prepare("UPDATE accounts SET creator_at = NULL WHERE id = ? AND creator_at IS NOT NULL")
    .bind(accountId)
    .run();
  return (res.meta?.changes ?? 0) > 0;
}
