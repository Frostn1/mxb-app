/**
 * Signed tokens for mxbsecure.com's Steam sign-in: the login state and the session cookie.
 *
 * Stateless on purpose. A token is `base64url(json).base64url(hmac)`, keyed on
 * `MXB_WEB_SESSION_KEY`; rotating that secret signs everyone out. Each payload carries a type
 * tag, so a login state can never be presented as a session.
 *
 * Both cookies are `__Host-`: the browser takes them only from this host, over HTTPS, at `/`,
 * with no `Domain`, so a sibling subdomain can't plant one for us.
 */

import { tokenMatches } from "./auth";
import { isSteamId64 } from "./steam";

export const SESSION_COOKIE = "__Host-mxb_session";

/** The session's name before `__Host-`. Read only when the new one is absent, and cleared on sign-in. */
export const LEGACY_SESSION_COOKIE = "mxb_session";

/** Ties a sign-in to the browser that started it: holds the state's nonce until Steam comes back. */
export const LOGIN_COOKIE = "__Host-mxb_login";

/** How long a sign-in lasts. */
export const SESSION_TTL_MS = 30 * 24 * 60 * 60 * 1000;

/** A sign-in that hasn't come back from Steam in this long has to start again. */
export const LOGIN_TTL_MS = 10 * 60 * 1000;

export interface WebSession {
  t: "session";
  steamId: string;
  name: string;
  exp: number;
}

export interface LoginState {
  t: "state";
  /** The site origin the sign-in started on, so it lands back there. */
  site?: string;
  next: string;
  n: string;
  exp: number;
}

const enc = new TextEncoder();

function b64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function unb64url(text: string): string {
  const s = text.replace(/-/g, "+").replace(/_/g, "/");
  return new TextDecoder().decode(Uint8Array.from(atob(s + "=".repeat((4 - (s.length % 4)) % 4)), (c) => c.charCodeAt(0)));
}

async function mac(key: string, data: string): Promise<string> {
  const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return b64url(new Uint8Array(await crypto.subtle.sign("HMAC", k, enc.encode(data))));
}

export async function sealToken(payload: WebSession | LoginState, key: string): Promise<string> {
  const body = b64url(enc.encode(JSON.stringify(payload)));
  return `${body}.${await mac(key, body)}`;
}

/** The payload if the signature holds, the type matches and it hasn't expired. */
export async function openToken<T extends WebSession | LoginState>(
  token: string | null | undefined,
  key: string,
  type: T["t"],
  now = Date.now(),
): Promise<T | null> {
  if (!token) return null;
  const [body, sig, extra] = token.split(".");
  if (!body || !sig || extra !== undefined) return null;
  if (!tokenMatches(await mac(key, body), sig)) return null;
  try {
    const p = JSON.parse(unb64url(body)) as T;
    if (p.t !== type || typeof p.exp !== "number" || p.exp <= now) return null;
    return p;
  } catch {
    return null;
  }
}

/** A cookie's value by name, or null. `names` in order of preference: the first one present wins. */
export function readCookie(request: Request, ...names: string[]): string | null {
  const found = new Map<string, string>();
  for (const part of (request.headers.get("Cookie") ?? "").split(";")) {
    const i = part.indexOf("=");
    const name = i > 0 ? part.slice(0, i).trim() : "";
    if (name && !found.has(name)) found.set(name, part.slice(i + 1).trim());
  }
  for (const name of names) {
    const value = found.get(name);
    if (value !== undefined) return value;
  }
  return null;
}

/** The signed-in Steam account, or null. */
export async function webSession(request: Request, env: Env, now = Date.now()): Promise<WebSession | null> {
  const key = env.MXB_WEB_SESSION_KEY;
  if (!key) return null;
  const token = readCookie(request, SESSION_COOKIE, LEGACY_SESSION_COOKIE);
  const s = await openToken<WebSession>(token, key, "session", now);
  return s && isSteamId64(s.steamId) ? s : null;
}

function cookie(name: string, value: string, maxAgeSeconds: number): string {
  return `${name}=${value}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=${maxAgeSeconds}`;
}

export function sessionCookie(token: string): string {
  return cookie(SESSION_COOKIE, token, SESSION_TTL_MS / 1000);
}

export function loginCookie(nonce: string): string {
  return cookie(LOGIN_COOKIE, nonce, LOGIN_TTL_MS / 1000);
}

/** A `Set-Cookie` that deletes `name` (set with the same attributes, so it matches). */
export function clearedCookie(name: string): string {
  return cookie(name, "", 0);
}
