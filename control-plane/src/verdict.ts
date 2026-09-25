/**
 * Signed gate verdicts: the startup gate's answer, in a form the app can keep and trust offline.
 *
 * `GET /v1/app/gate` has always been a plain JSON answer, which is enough to act on in the moment
 * and worth nothing afterwards: an app that wrote "you are blocked" to disk on the strength of an
 * unsigned reply has a file anybody can delete, and an app that trusted a stored "ok" has a file
 * anybody can write. So the gate also returns the same verdict as a small Ed25519-signed
 * statement. The apps hold only the public half (`crates/core/src/appgate.rs`), keep the last one
 * they were given in the shared data folder, and on a start with no network:
 *
 *  - an install never told it is banned keeps working offline, exactly as before;
 *  - an install given a signed block stays blocked offline, in every app of the lineup, until a
 *    newer signed verdict for the same account says otherwise.
 *
 * Signed with Ed25519 for the reason the plugin licenses are (`plugins.ts`): the app ships to the
 * people the verdict is about, so a MAC key compiled into it is one `strings` away from minting a
 * clean bill of health. With a signature it only ever holds the half that checks.
 *
 * Optional, like every secret here. Without `MXB_VERDICT_SIGNING_KEY` the gate answers exactly as
 * it always did and simply carries no `signed` field — a missing key must never be the thing that
 * stops the gate answering at all.
 */

/** Wire version of the signed payload. The app refuses one it does not know. */
export const VERDICT_VERSION = 1;

/** What the signature covers. Field order is the wire format: the app verifies these bytes. */
export interface VerdictPayload {
  v: number;
  status: "ok" | "signin" | "unsupported";
  /** The account the verdict is about, so a verdict cannot be carried to another install. */
  account: string;
  /**
   * SHA-256 of the bearer token it was fetched with (`hashToken`), lowercase hex. The app never
   * learns its account id except from a verdict, so this — signed — is how a launch knows a kept
   * verdict is about the account it is signed in as: it hashes the token in its config and
   * compares. Harmless to hand back: the caller holds the token itself.
   */
  token: string | null;
  steamId: string | null;
  guid: string | null;
  /** Milliseconds since epoch. A newer verdict for the same account replaces an older one. */
  issuedAt: number;
}

/** The `signed` field on a gate answer: the payload as the exact string signed, and its signature. */
export interface SignedVerdict {
  payload: string;
  /** Ed25519 over the UTF-8 bytes of `payload`, base64url without padding. */
  sig: string;
}

export function b64url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function unb64url(s: string): Uint8Array {
  const padded = s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4);
  const binary = atob(padded);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

/**
 * Import the signing key from its PKCS#8 DER, base64 or base64url, in `MXB_VERDICT_SIGNING_KEY`.
 * Null when unset or unreadable — either way the gate answers unsigned rather than not at all.
 *
 * Also the key `lease.ts` signs key leases with: one pair, one public half in the builds, and the
 * two payloads kept apart by their shape (see `verifyVerdict`).
 */
export async function signingKey(env: Env): Promise<CryptoKey | null> {
  const raw = env.MXB_VERDICT_SIGNING_KEY?.replace(/\s+/g, "");
  if (!raw) return null;
  try {
    return await crypto.subtle.importKey("pkcs8", unb64url(raw), { name: "Ed25519" }, false, ["sign"]);
  } catch (err) {
    console.error(JSON.stringify({ msg: "verdict signing key unreadable", error: String(err) }));
    return null;
  }
}

/** Sign a payload with a key already in hand. Split out so tests can use a pair of their own. */
export async function signPayload(payload: VerdictPayload, key: CryptoKey): Promise<SignedVerdict> {
  // Built field by field rather than spread, so the order on the wire is the order declared above
  // whatever the caller's object happened to hold.
  const text = JSON.stringify({
    v: payload.v,
    status: payload.status,
    account: payload.account,
    token: payload.token,
    steamId: payload.steamId,
    guid: payload.guid,
    issuedAt: payload.issuedAt,
  });
  const sig = await crypto.subtle.sign({ name: "Ed25519" }, key, new TextEncoder().encode(text));
  return { payload: text, sig: b64url(new Uint8Array(sig)) };
}

/**
 * The signed form of a gate verdict, or null when this deployment has no signing key.
 *
 * Never throws: the gate is the one endpoint every app waits on at startup, and a signing failure
 * is logged and answered unsigned — which the app reads exactly as it read every verdict before
 * this existed.
 */
export async function signVerdict(
  env: Env,
  fields: {
    status: VerdictPayload["status"];
    account: string;
    token?: string | null;
    steamId?: string | null;
    guid?: string | null;
  },
  now: number = Date.now(),
): Promise<SignedVerdict | null> {
  const key = await signingKey(env);
  if (!key) return null;
  try {
    return await signPayload(
      {
        v: VERDICT_VERSION,
        status: fields.status,
        account: fields.account,
        token: fields.token?.trim().toLowerCase() || null,
        steamId: fields.steamId?.trim() || null,
        guid: fields.guid?.trim().toUpperCase() || null,
        issuedAt: now,
      },
      key,
    );
  } catch (err) {
    console.error(JSON.stringify({ msg: "verdict not signed", error: String(err) }));
    return null;
  }
}

/** The app's side of the same check, kept here so the two can be tested against each other. */
export async function verifyVerdict(signed: SignedVerdict, publicKey: CryptoKey): Promise<VerdictPayload | null> {
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
    const parsed = JSON.parse(signed.payload) as VerdictPayload & { purpose?: unknown };
    // A key lease (`lease.ts`) is signed by the same key, so it is refused here by its shape: it
    // carries a `purpose` and no `status` or `account`. Neither can be replayed as the other.
    if (parsed.v !== VERDICT_VERSION || "purpose" in parsed) return null;
    if (typeof parsed.status !== "string" || typeof parsed.account !== "string") return null;
    return parsed;
  } catch {
    return null;
  }
}
