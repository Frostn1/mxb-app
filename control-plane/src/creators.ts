/**
 * Who may lock and sell through mxbsecure.com, as the admin page manages it.
 *
 * Being a creator is `accounts.creator_at`. Someone with no MXB App account gets a `kind='web'`
 * row keyed on their Steam ID; when they later link Steam in the app, `steamReturn` hands that
 * row's creator standing and assets to the app account. Removing clears `creator_at` only: the
 * assets stay theirs, and their API keys stop working because key auth checks it too.
 */

import { resolveSteamAccount } from "./assets";
import { hashToken, newToken } from "./auth";
import { steamPersonaName } from "./steam";
import { repairBySteamId } from "./steamlink";

export interface CreatorRow {
  accountId: string;
  riderName: string;
  steamId: string | null;
  steamName: string;
  /** False for a web-only row that no app account has claimed yet. */
  linked: boolean;
  creatorAt: number;
  assets: number;
}

export async function listCreators(env: Env, fetchImpl: typeof fetch = fetch): Promise<CreatorRow[]> {
  const { results } = await env.DB.prepare(
    "SELECT a.id, a.rider_name, a.steam_id, a.kind, a.creator_at," +
      " (SELECT COUNT(*) FROM assets s WHERE s.creator_id = a.id) AS assets" +
      " FROM accounts a WHERE a.creator_at IS NOT NULL ORDER BY a.creator_at DESC",
  ).all<{ id: string; rider_name: string; steam_id: string | null; kind: string; creator_at: number; assets: number }>();
  // A handful of rows, and each name is edge-cached for a day.
  return Promise.all(
    results.map(async (r) => ({
      accountId: r.id,
      riderName: r.rider_name,
      steamId: r.steam_id,
      steamName: r.steam_id ? await steamPersonaName(r.steam_id, fetchImpl) : "",
      linked: r.kind !== "web",
      creatorAt: r.creator_at,
      assets: r.assets,
    })),
  );
}

export async function addCreator(
  env: Env,
  who: string,
  fetchImpl: typeof fetch = fetch,
): Promise<{ ok: true; accountId: string; steamId: string; already: boolean } | { ok: false; error: string }> {
  const resolved = await resolveSteamAccount(who, fetchImpl);
  if ("error" in resolved) return { ok: false, error: resolved.error };
  const { steamId } = resolved;
  const now = Date.now();

  const find = () =>
    env.DB.prepare("SELECT id, creator_at FROM accounts WHERE steam_id = ?")
      .bind(steamId)
      .first<{ id: string; creator_at: number | null }>();
  const account = (await find()) ?? ((await repairBySteamId(env, steamId)) ? await find() : null);
  if (account) {
    if (account.creator_at) return { ok: true, accountId: account.id, steamId, already: true };
    await env.DB.prepare("UPDATE accounts SET creator_at = ? WHERE id = ?").bind(now, account.id).run();
    return { ok: true, accountId: account.id, steamId, already: false };
  }

  const id = crypto.randomUUID();
  try {
    await env.DB.prepare(
      "INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at, kind, creator_at) VALUES (?, ?, ?, ?, ?, 'web', ?)",
    )
      .bind(id, `web:${steamId}`, steamId, await hashToken(newToken()), now, now)
      .run();
  } catch (err) {
    // Two adds at once: the other one made the row.
    const again = await find();
    if (!again) throw err;
    return { ok: true, accountId: again.id, steamId, already: true };
  }
  return { ok: true, accountId: id, steamId, already: false };
}

/** False when the account wasn't a creator to begin with. */
export async function removeCreator(env: Env, accountId: string): Promise<boolean> {
  const res = await env.DB.prepare("UPDATE accounts SET creator_at = NULL WHERE id = ? AND creator_at IS NOT NULL")
    .bind(accountId)
    .run();
  return (res.meta?.changes ?? 0) > 0;
}
