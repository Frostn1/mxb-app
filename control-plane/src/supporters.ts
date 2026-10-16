/**
 * Supporters: who has the supporter pack.
 *
 * The pack is cosmetic only — app themes, an accent colour, a badge. Nothing the apps do today
 * depends on it. Somebody is a supporter while any row in `supporters` for their Steam id is
 * unrevoked; rows come from a Stripe pack, a Buy Me a Coffee match, or an admin's hand.
 *
 * Returned to the app with `/v1/entitlements`, which it already asks while signed in, and
 * cached there with the time it was read so the cosmetics keep working offline.
 */

import { resolveSteamAccount } from "./assets";
import { bearer, hashToken } from "./auth";
import { steamIdFor } from "./steamlink";

export type SupporterSource = "manual" | "bmac" | "stripe";

const SOURCES: readonly SupporterSource[] = ["manual", "bmac", "stripe"];

export function isSupporterSource(value: string): value is SupporterSource {
  return (SOURCES as readonly string[]).includes(value);
}

/** Whether this Steam id has any live grant. */
export async function isSupporter(env: Env, steamId: string | null): Promise<boolean> {
  if (!steamId) return false;
  const row = await env.DB.prepare(
    "SELECT 1 AS yes FROM supporters WHERE steam_id = ? AND revoked_at IS NULL LIMIT 1",
  )
    .bind(steamId)
    .first<{ yes: number }>();
  return Boolean(row);
}

/**
 * Whether an optional bearer on an otherwise open request belongs to a supporter. No token, an
 * unknown token or an unlinked account are all simply "no".
 */
export async function requestIsSupporter(request: Request, env: Env): Promise<boolean> {
  const token = bearer(request.headers.get("Authorization"));
  if (!token) return false;
  const account = await env.DB.prepare("SELECT id, steam_id FROM accounts WHERE token_hash = ?")
    .bind(await hashToken(token))
    .first<{ id: string; steam_id: string | null }>();
  if (!account) return false;
  return isSupporter(env, await steamIdFor(env, account));
}

/**
 * SQL for "the account in `column` has the pack": its own Steam id, or the one it linked.
 * For joining onto rows that carry an account id, like `servers.owner_account_id`.
 */
export function supporterOwnerSql(column: string): string {
  return (
    "EXISTS (SELECT 1 FROM supporters sp WHERE sp.revoked_at IS NULL AND sp.steam_id IN (" +
    `SELECT a.steam_id FROM accounts a WHERE a.id = ${column} AND a.steam_id IS NOT NULL` +
    ` UNION SELECT l.steam_id FROM steam_links l WHERE l.account_id = ${column}))`
  );
}

/**
 * Grant, or re-grant a revoked row. Idempotent per (steam id, source, ref): a webhook delivered
 * twice is one grant. Returns whether anything changed.
 */
export async function grantSupporter(
  env: Env,
  steamId: string,
  source: SupporterSource,
  ref = "",
  grantedBy: string | null = null,
  now = Date.now(),
): Promise<boolean> {
  const res = await env.DB.prepare(
    "INSERT INTO supporters (steam_id, source, ref, granted_at, granted_by) VALUES (?, ?, ?, ?, ?)" +
      " ON CONFLICT (steam_id, source, ref) DO UPDATE SET revoked_at = NULL, granted_at = excluded.granted_at," +
      " granted_by = excluded.granted_by WHERE supporters.revoked_at IS NOT NULL",
  )
    .bind(steamId, source, ref, now, grantedBy)
    .run();
  return (res.meta?.changes ?? 0) > 0;
}

/** Revoke one grant (a refunded pack) or, with no `ref`, every grant from that source. */
export async function revokeSupporter(
  env: Env,
  steamId: string,
  source: SupporterSource,
  ref?: string,
  now = Date.now(),
): Promise<boolean> {
  const res =
    ref === undefined
      ? await env.DB.prepare(
          "UPDATE supporters SET revoked_at = ? WHERE steam_id = ? AND source = ? AND revoked_at IS NULL",
        )
          .bind(now, steamId, source)
          .run()
      : await env.DB.prepare(
          "UPDATE supporters SET revoked_at = ? WHERE steam_id = ? AND source = ? AND ref = ? AND revoked_at IS NULL",
        )
          .bind(now, steamId, source, ref)
          .run();
  return (res.meta?.changes ?? 0) > 0;
}

export interface SupporterRow {
  steamId: string;
  source: SupporterSource;
  ref: string;
  grantedAt: number;
  grantedBy: string | null;
  revokedAt: number | null;
}

/** Every grant, newest first, for the admin page. */
export async function listSupporters(env: Env, limit = 500): Promise<SupporterRow[]> {
  const rows = await env.DB.prepare(
    "SELECT steam_id, source, ref, granted_at, granted_by, revoked_at FROM supporters" +
      " ORDER BY granted_at DESC LIMIT ?",
  )
    .bind(limit)
    .all<{
      steam_id: string;
      source: SupporterSource;
      ref: string;
      granted_at: number;
      granted_by: string | null;
      revoked_at: number | null;
    }>();
  return (rows.results ?? []).map((r) => ({
    steamId: r.steam_id,
    source: r.source,
    ref: r.ref,
    grantedAt: r.granted_at,
    grantedBy: r.granted_by,
    revokedAt: r.revoked_at,
  }));
}

/**
 * The admin's buttons: grant or revoke by hand. `who` is anything `resolveSteamAccount` reads —
 * a SteamID64, a profile link, a vanity name.
 */
export async function adminSupporterAction(
  env: Env,
  action: string,
  who: string,
  adminSteamId: string,
  fetchImpl: typeof fetch = fetch,
): Promise<{ ok: true; steamId: string; changed: boolean } | { ok: false; status: number; error: string }> {
  if (action !== "grant" && action !== "revoke") return { ok: false, status: 400, error: "no such action" };
  const resolved = await resolveSteamAccount(who, fetchImpl);
  if ("error" in resolved) return { ok: false, status: 400, error: resolved.error };
  const { steamId } = resolved;
  const changed =
    action === "grant"
      ? await grantSupporter(env, steamId, "manual", "", adminSteamId)
      : await revokeSupporter(env, steamId, "manual");
  return { ok: true, steamId, changed };
}
