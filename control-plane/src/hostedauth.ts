/**
 * MSM (MXB Servers) signs in with Steam, the way MXB App does, and gets a per-user token that
 * drives that Steam account's own hosted servers. No claim link per server.
 *
 * The round trip, as MXB App's `/v1/steam/login` -> `/start` -> `/return`:
 *
 * 1. MSM `POST /v1/hosted/auth/login {challenge, state}`: a pending sign-in row; the answer is
 *    the URL MSM opens in the browser.
 * 2. `GET /v1/hosted/auth/start?login=ID`: the branded hop into Steam OpenID.
 * 3. `GET /v1/hosted/auth/return?login=ID`: Valve confirms the Steam ID; the row gets a one-time
 *    code (stored as a digest, two minutes) and the browser hands `mxbservers://auth?code&state`
 *    back to MSM.
 * 4. MSM `POST /v1/hosted/auth/token {code, verifier}`: single use; the verifier must hash to the
 *    challenge from step 1 (PKCE S256), so a code lifted from the browser or a link is worthless
 *    to anyone but the MSM that started the sign-in.
 *
 * The token is hosting-only (it is looked up only by `/v1/hosted/*`), stored as a digest, and
 * revoked by `POST /v1/hosted/auth/revoke` (sign-out). Every server call still checks that the
 * server belongs to the token's Steam account.
 */

import { bearer, hashToken, newToken } from "./auth";
import { redirectPage, steamResult } from "./page";
import { isSteamId64, isVerified, loginUrl, steamPersonaName, verifyAssertion } from "./steam";

export interface AuthResult {
  status: number;
  body: unknown;
}

/** From the row being minted to Valve's answer (MXB App's sign-in allows the same). */
export const MSM_LOGIN_TTL_MS = 30 * 60 * 1000;
/** From Valve's answer to MSM swapping the code. MSM does it the moment the link opens. */
export const MSM_CODE_TTL_MS = 2 * 60 * 1000;

const STATE = /^[A-Za-z0-9_-]{16,128}$/;
/** base64url(SHA-256(verifier)), unpadded: always 43 characters. */
const CHALLENGE = /^[A-Za-z0-9_-]{43}$/;
/** RFC 7636 section 4.1. */
const VERIFIER = /^[A-Za-z0-9._~-]{43,128}$/;
const CODE = /^[A-Za-z0-9_-]{32,128}$/;
const LOGIN_ID = /^[0-9a-f-]{36}$/i;

function b64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** The S256 challenge for a verifier. */
export async function pkceChallenge(verifier: string): Promise<string> {
  return b64url(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))));
}

function siteOrigin(env: Env): string {
  return (env.MXB_SITE_ORIGIN || "https://mxbsecure.com").replace(/\/+$/, "");
}

/** Step 1: MSM asks for a sign-in URL. */
export async function msmLogin(env: Env, origin: string, input: Record<string, unknown>, now: number): Promise<AuthResult> {
  const challenge = typeof input.challenge === "string" ? input.challenge : "";
  const state = typeof input.state === "string" ? input.state : "";
  if (!CHALLENGE.test(challenge) || !STATE.test(state)) return { status: 400, body: { error: "That sign-in request isn't valid." } };
  const id = crypto.randomUUID();
  await env.DB.prepare("INSERT INTO host_msm_logins (id, challenge, state, created_at) VALUES (?, ?, ?, ?)")
    .bind(id, challenge, state, now)
    .run();
  return { status: 200, body: { url: `${origin}/v1/hosted/auth/start?login=${id}` } };
}

interface LoginRow {
  state: string;
  created_at: number;
  steam_id: string | null;
}

async function loginRow(env: Env, id: string | null): Promise<LoginRow | null> {
  if (!id || !LOGIN_ID.test(id)) return null;
  return env.DB.prepare("SELECT state, created_at, steam_id FROM host_msm_logins WHERE id = ?").bind(id).first<LoginRow>();
}

const live = (row: LoginRow | null, now: number): row is LoginRow =>
  !!row && row.steam_id === null && now - row.created_at <= MSM_LOGIN_TTL_MS;

/** Step 2: the browser arrives; on to Steam. */
export async function msmStart(env: Env, url: URL, now: number): Promise<Response> {
  const id = url.searchParams.get("login");
  if (!live(await loginRow(env, id), now)) return steamResult(siteOrigin(env), "expired");
  const returnTo = `${url.origin}/v1/hosted/auth/return?login=${id}`;
  return redirectPage(loginUrl(returnTo, `${url.origin}/`), "Signing you in…", "Taking you to Steam.");
}

/** Step 3: Steam sends the browser back; hand MSM a one-time code. */
export async function msmReturn(env: Env, url: URL, now: number, fetchImpl: typeof fetch): Promise<Response> {
  const site = siteOrigin(env);
  const id = url.searchParams.get("login");
  if (!live(await loginRow(env, id), now)) return steamResult(site, "expired");
  const result = await verifyAssertion(url.searchParams, `${url.origin}${url.pathname}`, fetchImpl);
  if (!isVerified(result) || !isSteamId64(result.steamId)) return steamResult(site, "unconfirmed");
  const code = newToken();
  // Conditional, so a reloaded return (or two racing) can only ever mint one code.
  const done = await env.DB.prepare(
    "UPDATE host_msm_logins SET steam_id = ?, code_hash = ?, code_expires_at = ?" +
      " WHERE id = ? AND steam_id IS NULL AND created_at >= ? RETURNING state",
  )
    .bind(result.steamId, await hashToken(code), now + MSM_CODE_TTL_MS, id, now - MSM_LOGIN_TTL_MS)
    .first<{ state: string }>();
  if (!done) return steamResult(site, "expired");
  const back = `mxbservers://auth?code=${code}&state=${encodeURIComponent(done.state)}`;
  return redirectPage(back, "Signed in", "Return to MXB Servers.", "Open MXB Servers");
}

/** Step 4: MSM swaps the code (and its PKCE verifier) for a token. */
export async function msmToken(env: Env, input: Record<string, unknown>, now: number, fetchImpl: typeof fetch): Promise<AuthResult> {
  const code = typeof input.code === "string" ? input.code : "";
  const verifier = typeof input.verifier === "string" ? input.verifier : "";
  if (!CODE.test(code) || !VERIFIER.test(verifier)) return { status: 400, body: { error: "That sign-in isn't valid." } };
  // Spent first, checked after: a code presented with the wrong verifier is burned too.
  const used = await env.DB.prepare(
    "UPDATE host_msm_logins SET used_at = ? WHERE code_hash = ? AND used_at IS NULL AND code_expires_at > ?" +
      " RETURNING steam_id, challenge",
  )
    .bind(now, await hashToken(code), now)
    .first<{ steam_id: string; challenge: string }>();
  if (!used) return { status: 404, body: { error: "That sign-in expired. Sign in again." } };
  if ((await pkceChallenge(verifier)) !== used.challenge) return { status: 403, body: { error: "That sign-in isn't valid." } };
  const token = newToken();
  await env.DB.prepare("INSERT INTO host_user_tokens (token_hash, steam_id, created_at) VALUES (?, ?, ?)")
    .bind(await hashToken(token), used.steam_id, now)
    .run();
  const name = await steamPersonaName(used.steam_id, fetchImpl);
  return { status: 200, body: { token, name } };
}

/** The Steam account a per-user MSM token speaks for, or null. */
export async function userTokenOwner(env: Env, request: Request): Promise<string | null> {
  const presented = bearer(request.headers.get("Authorization"));
  if (!presented) return null;
  const row = await env.DB.prepare("SELECT steam_id FROM host_user_tokens WHERE token_hash = ? AND revoked_at IS NULL")
    .bind(await hashToken(presented))
    .first<{ steam_id: string }>();
  return row?.steam_id ?? null;
}

/** Sign-out: the presented token stops working. Idempotent. */
export async function msmRevoke(env: Env, request: Request, now: number): Promise<AuthResult> {
  const presented = bearer(request.headers.get("Authorization"));
  if (!presented) return { status: 401, body: { error: "Not signed in." } };
  await env.DB.prepare("UPDATE host_user_tokens SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
    .bind(now, await hashToken(presented))
    .run();
  return { status: 200, body: { ok: true } };
}

/** Note that a token was used (the listing call, once per MSM refresh). */
export async function touchUserToken(env: Env, request: Request, now: number): Promise<void> {
  const presented = bearer(request.headers.get("Authorization"));
  if (!presented) return;
  await env.DB.prepare("UPDATE host_user_tokens SET last_used_at = ? WHERE token_hash = ?").bind(now, await hashToken(presented)).run();
}

/** Sign-in rows are done with after a day: dropped by the hosting tick. */
export async function pruneMsmLogins(env: Env, now: number): Promise<void> {
  await env.DB.prepare("DELETE FROM host_msm_logins WHERE created_at < ?").bind(now - 24 * 60 * 60 * 1000).run();
}
