/**
 * Steam sign-in, for the one thing only Valve can tell us: who this person is.
 *
 * Entitlement is ours — it is a row in `entitlements` and nothing else. What we cannot
 * decide for ourselves is whether the person claiming to be a given player really is one,
 * and that is all this file does: turn a browser round trip through Steam into a SteamID64
 * we are willing to trust.
 *
 * ## Why OpenID and not an auth-session ticket
 *
 * The in-process route (`GetAuthSessionTicket`, verified with `AuthenticateUserTicket`) is
 * bound to a Steam AppID. MX Bikes' AppID is PiBoSo's, and `ISteamUser/CheckAppOwnership`
 * "requires the publisher API key that owns the specified App ID" — a publisher key works
 * only on its own apps. So we could never have asked Valve about MX Bikes anyway, with any
 * key obtainable by us.
 *
 * OpenID needs no AppID, no publisher key and no code inside the game. What it proves is
 * narrower and the difference matters: **that the person who authorized this sign-in
 * controls that Steam account**, not that the account is the one running the game right
 * now. The gap that leaves is an entitled user handing access to a friend — bounded by
 * short-lived sessions and attribution, and small next to the fact that anyone entitled can
 * read the decrypted asset out of their own memory regardless.
 *
 * ## Verifying, rather than believing
 *
 * Steam sends the assertion back as query parameters on a URL *the user's browser is
 * holding*, so every one of them is attacker-controlled until proven otherwise. Exactly one
 * thing makes them trustworthy: sending them back to Steam with `check_authentication` and
 * getting `is_valid:true`. Everything else here exists to stop an assertion that is
 * genuinely valid — for some other site, or replayed a second time — from counting as one
 * for us.
 */

/** Steam's OpenID 2.0 endpoint. */
const OPENID_ENDPOINT = "https://steamcommunity.com/openid/login";

/** The only issuer whose claimed identifiers we accept. */
const CLAIMED_ID_PREFIX = "https://steamcommunity.com/openid/id/";

/** A sign-in that hasn't completed in this long is abandoned, not pending. */
export const LOGIN_TTL_MS = 10 * 60 * 1000;

/** How long to wait on Steam before giving up on a verification. */
const VERIFY_TIMEOUT_MS = 10_000;

/**
 * Where to send the browser to sign in.
 *
 * `returnTo` carries the login id, so the assertion comes back attached to the row that
 * says who was signing in. It is also checked on the way back — see [`verifyAssertion`] —
 * which is what stops an assertion minted for another site being replayed at ours.
 */
export function loginUrl(returnTo: string, realm: string): string {
  const params = new URLSearchParams({
    "openid.ns": "http://specs.openid.net/auth/2.0",
    "openid.mode": "checkid_setup",
    "openid.return_to": returnTo,
    "openid.realm": realm,
    // Steam ignores per-user identifiers and always answers with the signed-in account,
    // so both of these are the "not yet known" sentinel the spec defines.
    "openid.identity": "http://specs.openid.net/auth/2.0/identifier_select",
    "openid.claimed_id": "http://specs.openid.net/auth/2.0/identifier_select",
  });
  return `${OPENID_ENDPOINT}?${params.toString()}`;
}

/** What a verified sign-in yields. */
export interface VerifiedSteamId {
  steamId: string;
}

/** Why one didn't. Returned rather than thrown: every branch here is a normal outcome. */
export type VerifyFailure = { error: string };

export type VerifyResult = VerifiedSteamId | VerifyFailure;

export function isVerified(r: VerifyResult): r is VerifiedSteamId {
  return (r as VerifiedSteamId).steamId !== undefined;
}

/**
 * A SteamID64, as Valve writes them: 17 digits, and nothing else.
 *
 * Checked because this value becomes a database key and is compared against entitlements.
 * A claimed identifier that is a valid URL but not a Steam profile must not become an
 * identity.
 */
export function steamIdFromClaimedId(claimedId: unknown): string | null {
  if (typeof claimedId !== "string") return null;
  if (!claimedId.startsWith(CLAIMED_ID_PREFIX)) return null;
  const id = claimedId.slice(CLAIMED_ID_PREFIX.length);
  return /^\d{17}$/.test(id) ? id : null;
}

/**
 * Is this assertion addressed to us?
 *
 * An OpenID assertion is only meaningful for the `return_to` it was minted for. Without
 * this check, an assertion a user legitimately obtained at any other Steam-login site could
 * be pasted at ours and would verify perfectly — `check_authentication` would say `true`,
 * because it *is* a true assertion. It just isn't one about us.
 *
 * Compared on origin and path, ignoring query: the return URL carries the login id, and
 * that is checked separately by looking the row up.
 */
export function returnToMatches(asserted: unknown, expected: string): boolean {
  if (typeof asserted !== "string") return false;
  try {
    const a = new URL(asserted);
    const e = new URL(expected);
    return a.origin === e.origin && a.pathname === e.pathname;
  } catch {
    return false;
  }
}

/**
 * Ask Steam whether it really said this.
 *
 * The whole assertion is echoed back with `openid.mode` swapped to `check_authentication`,
 * which is the spec's "did you sign this" question. Only `is_valid:true` counts; anything
 * else — a false, a malformed body, a network failure — is a refusal. There is deliberately
 * no path here that lets a verification failure resolve as success.
 */
export async function verifyAssertion(
  query: URLSearchParams,
  expectedReturnTo: string,
  fetchImpl: typeof fetch = fetch,
): Promise<VerifyResult> {
  if (query.get("openid.mode") !== "id_res") {
    // `cancel` is the user declining at Steam, which is not an error worth alarming about.
    return { error: query.get("openid.mode") === "cancel" ? "cancelled" : "not an assertion" };
  }
  if (!returnToMatches(query.get("openid.return_to"), expectedReturnTo)) {
    return { error: "that sign-in was not for this site" };
  }
  const steamId = steamIdFromClaimedId(query.get("openid.claimed_id"));
  if (!steamId) return { error: "no Steam account in that sign-in" };

  const body = new URLSearchParams(query);
  body.set("openid.mode", "check_authentication");

  let text: string;
  try {
    const resp = await fetchImpl(OPENID_ENDPOINT, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: body.toString(),
      signal: AbortSignal.timeout(VERIFY_TIMEOUT_MS),
    });
    if (!resp.ok) return { error: `Steam refused the check (${resp.status})` };
    text = await resp.text();
  } catch {
    return { error: "couldn't reach Steam to check that sign-in" };
  }

  // Key-value form, one pair per line. Matched on a whole line so that a value appearing
  // inside some other field cannot be mistaken for the verdict.
  const valid = text
    .split("\n")
    .map((line) => line.trim())
    .includes("is_valid:true");
  if (!valid) return { error: "Steam did not confirm that sign-in" };

  return { steamId };
}
