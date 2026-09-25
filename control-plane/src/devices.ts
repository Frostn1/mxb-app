/**
 * Which machine an account is being used on, as a keyed one-way hash — so a ban follows the PC.
 *
 * `bans.ts` already follows a ban through the GUID, the Steam login and every account either one
 * touches. What it could not follow is the cheapest evasion left: a fresh token, or a fresh Steam
 * account, on the same banned PC. That is what this adds, and all it adds.
 *
 * ## What is sent, and what is kept
 *
 * The app never sends the machine identifier. It sends `SHA-256("mxb-device-link/v1\0" ‖ id)`
 * in the `X-MXB-Device` header (`crates/core/src/device.rs`), on the startup gate and when it
 * mints a device account. That is already not the identifier, but a bare hash of a GUID-shaped
 * value is guessable by anyone holding the id, so it is not what is stored either: the worker
 * keys it again with its own secret, `MXB_DEVICE_SALT`, and stores
 * `HMAC-SHA256(MXB_DEVICE_SALT, client hash)`. A copy of `device_links` without the secret
 * matches nothing and reverses to nothing.
 *
 * ## Off unless configured
 *
 * No `MXB_DEVICE_SALT`, no feature: nothing is recorded and the ban resolution does not read the
 * table. Never an error either way — a sighting that fails to write must not fail the request
 * carrying it, exactly like `rememberGuid`.
 *
 * ## Not kept past the account
 *
 * `erasure.ts` deletes an account's rows on request, banned or not. A ban survives erasure through
 * the GUID claims; the device link is the one piece of a banned rider's record we chose not to
 * keep, because it describes their hardware rather than what they did.
 */

/** The header the apps report the client-side hash in. */
export const DEVICE_HEADER = "X-MXB-Device";

/** A client hash is SHA-256, lowercase hex. Anything else is ignored, never stored. */
const CLIENT_HASH = /^[0-9a-f]{64}$/;

/** Whether this deployment links devices at all. */
export function deviceLinking(env: Env): boolean {
  return Boolean(env.MXB_DEVICE_SALT);
}

/**
 * The stored form of a reported device: the client's hash keyed with the deployment's secret.
 * `null` when the feature is off or the report isn't a client hash.
 */
export async function deviceHash(env: Env, reported: string | null | undefined): Promise<string | null> {
  const salt = env.MXB_DEVICE_SALT;
  if (!salt) return null;
  const clientHash = reported?.trim().toLowerCase() ?? "";
  if (!CLIENT_HASH.test(clientHash)) return null;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(salt),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(clientHash));
  return [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The stored device hash a request reports, or null. */
export async function deviceFromRequest(env: Env, request: Request): Promise<string | null> {
  try {
    return await deviceHash(env, request.headers.get(DEVICE_HEADER));
  } catch (err) {
    // A device we could not key is a device we do not know about, never a failed request.
    console.error(JSON.stringify({ msg: "device report not keyed", error: String(err) }));
    return null;
  }
}

/**
 * Remember that an account was used on a device (already keyed by `deviceHash`).
 *
 * Never throws: a missing sighting must not fail the request that carried it. The account row
 * has to exist, which is true of every caller — they authenticated as it or just minted it.
 */
export async function rememberDevice(env: Env, accountId: string, device: string | null): Promise<void> {
  if (!device) return;
  const now = Date.now();
  try {
    await env.DB.prepare(
      "INSERT INTO device_links (account_id, device_hash, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?)" +
        " ON CONFLICT (account_id, device_hash) DO UPDATE SET last_seen_at = excluded.last_seen_at",
    )
      .bind(accountId, device, now, now)
      .run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "device sighting not recorded", error: String(err) }));
  }
}
