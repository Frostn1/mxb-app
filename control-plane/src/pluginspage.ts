/**
 * The query side of the site's plugin keys dashboard (`webadmin.ts`): what a listing URL
 * asks for, and the codes from one mint read back.
 */

import { MAX_MINT, type KeyQuery, type LicenseQuery } from "./plugins";
import { parsePage } from "./adminui";

const KEY_STATES = ["any", "unused", "redeemed", "revoked"] as const;
const LICENSE_STATES = ["any", "live", "expired", "revoked"] as const;

function oneOf<T extends string>(value: string | null, allowed: readonly T[], fallback: T): T {
  return (allowed as readonly string[]).includes(value ?? "") ? (value as T) : fallback;
}

export function keyQuery(url: URL): KeyQuery {
  return {
    q: url.searchParams.get("q") ?? "",
    plugin: url.searchParams.get("plugin") ?? "",
    state: oneOf(url.searchParams.get("state"), KEY_STATES, "any") as KeyQuery["state"],
    page: parsePage(url.searchParams.get("page")),
  };
}

export function licenseQuery(url: URL): LicenseQuery {
  return {
    q: url.searchParams.get("q") ?? "",
    plugin: url.searchParams.get("plugin") ?? "",
    state: oneOf(url.searchParams.get("state"), LICENSE_STATES, "any") as LicenseQuery["state"],
    page: parsePage(url.searchParams.get("page")),
  };
}

/** The codes from one mint, read back by the second they were minted in. */
export async function batchCodes(env: Env, createdAt: number): Promise<string[]> {
  const { results } = await env.DB.prepare(
    `SELECT code FROM plugin_keys WHERE created_at = ? ORDER BY code LIMIT ${MAX_MINT}`,
  )
    .bind(createdAt)
    .all<{ code: string }>();
  return (results ?? []).map((r) => r.code);
}
