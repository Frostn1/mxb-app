/**
 * Key leases: the signed permission a `.mxbkey` needs beside it before the DLL will unseal it.
 *
 * A key the app has sealed to a buyer's PC opens with no server, which is what lets a buyer play
 * offline — and, before this, what let a banned install keep playing everything it had already
 * unlocked for as long as it stayed offline. The status poll (`POST /v1/assets/status`) reaches
 * those keys only when the app is online and left alone. So the key is now half of what the DLL
 * asks for; the other half is a lease:
 *
 *  - `POST /v1/keys/lease` answers a Valve-confirmed Steam account with a small Ed25519-signed
 *    statement naming that Steam ID and an expiry 30 days out;
 *  - the app keeps the latest one beside the manifest, renewing it silently whenever it is online
 *    and fewer than 25 days are left;
 *  - the DLL unseals nothing without a lease that verifies, names the live Steam ID, and has not
 *    run out. One lease covers every key that Steam account holds on this install.
 *
 * So a buyer plays offline for up to 30 days between check-ins, and a ban stops the renewal: the
 * route sits behind the ban gate, so a banned account is refused with `code: "blocked"`, and the
 * app deletes its lease when it hears that.
 *
 * Signed with the gate-verdict key (`MXB_VERDICT_SIGNING_KEY`, see `verdict.ts`), so the builds
 * carry one public half. The two payloads are kept apart by shape: a lease carries
 * `purpose: "mxbsecure-lease"` and no `status` or `account`, and each verifier refuses the other.
 *
 * Without the key the route answers 503 "leases not configured". The DLL is built to match: with
 * no public key compiled in it does not ask for a lease at all, so a deployment that has not set
 * the secret and a DLL shipped before the keys exist both keep working exactly as before.
 */

import { appBlocked, banFor } from "./bans";
import { steamIdFor } from "./steamlink";
import { b64url, signingKey, unb64url } from "./verdict";

/** Wire version of the lease payload. The DLL refuses one it does not know. */
export const LEASE_VERSION = 1;

/** The domain tag, so a lease is never mistaken for a gate verdict signed by the same key. */
export const LEASE_PURPOSE = "mxbsecure-lease";

/** How long a lease holds: the longest a buyer can play without the app checking in. */
export const LEASE_DAYS = 30;
export const LEASE_TTL_MS = LEASE_DAYS * 24 * 60 * 60 * 1000;

/** What the signature covers. Field order is the wire format: the DLL verifies these bytes. */
export interface LeasePayload {
  v: number;
  purpose: typeof LEASE_PURPOSE;
  /** The Steam account the lease is for. The DLL compares it with the Steam ID it reads live. */
  steamId: string;
  /** Milliseconds since epoch, by the server's clock. */
  issuedAt: number;
  expiresAt: number;
}

/** A lease as it goes on the wire and on disk: the exact string signed, and its signature. */
export interface SignedLease {
  payload: string;
  /** Ed25519 over the UTF-8 bytes of `payload`, base64url without padding. */
  sig: string;
}

/** Sign a lease with a key already in hand. Split out so tests can use a pair of their own. */
export async function signLeaseWith(steamId: string, key: CryptoKey, now: number = Date.now()): Promise<SignedLease> {
  // Built field by field, so the order on the wire is the order declared above.
  const text = JSON.stringify({
    v: LEASE_VERSION,
    purpose: LEASE_PURPOSE,
    steamId,
    issuedAt: now,
    expiresAt: now + LEASE_TTL_MS,
  });
  const sig = await crypto.subtle.sign({ name: "Ed25519" }, key, new TextEncoder().encode(text));
  return { payload: text, sig: b64url(new Uint8Array(sig)) };
}

/** The DLL's side of the check, kept here so the two can be tested against each other. */
export async function verifyLease(signed: SignedLease, publicKey: CryptoKey): Promise<LeasePayload | null> {
  let ok = false;
  try {
    ok = await crypto.subtle.verify(
      { name: "Ed25519" },
      publicKey,
      unb64url(signed.sig),
      new TextEncoder().encode(signed.payload),
    );
  } catch {
    return null;
  }
  if (!ok) return null;
  try {
    const parsed = JSON.parse(signed.payload) as LeasePayload;
    return parsed.v === LEASE_VERSION && parsed.purpose === LEASE_PURPOSE ? parsed : null;
  } catch {
    return null;
  }
}

/**
 * `POST /v1/keys/lease`: a fresh lease for the caller's Valve-confirmed Steam account.
 *
 * Reached only past the ban gate in `route`, which already refuses a banned account. The ban is
 * asked again here through the Steam ID `steamIdFor` resolves, because that can be a confirmed
 * link the account row had not caught up with — the same reason `decideEntitlement` asks twice.
 */
export async function issueLease(
  account: { id: string; steam_id: string | null; guid: string | null },
  env: Env,
  now: number = Date.now(),
): Promise<Response> {
  const steamId = await steamIdFor(env, account);
  if (await banFor(env, { accountId: account.id, steamId, guid: account.guid })) {
    return json(403, appBlocked());
  }
  // The same words the key grant refuses with, so the app treats both the same way.
  if (!steamId) return json(409, { error: "no Steam account linked" });
  const key = await signingKey(env);
  // No key, no leases — and nothing breaks: a DLL only asks for one when it was built with the
  // public half, which is only done once this secret is set.
  if (!key) return json(503, { error: "leases not configured" });
  try {
    const lease = await signLeaseWith(steamId, key, now);
    return json(200, { lease, expiresAt: now + LEASE_TTL_MS });
  } catch (err) {
    console.error(JSON.stringify({ msg: "lease not signed", error: String(err) }));
    return json(503, { error: "leases not configured" });
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
