/**
 * Bearer tokens for player accounts.
 *
 * Tokens are shown once at enrollment and stored only as a SHA-256 digest, so a dump of the
 * database yields nothing that can be presented as a credential. Lookup is by digest, which
 * also means the comparison happens inside the index rather than in our code — there is no
 * string compare to leak timing.
 */

/** Bytes of entropy per token. 32 is well past guessing range and still a short string. */
const TOKEN_BYTES = 32;

/** A fresh token. `crypto.getRandomValues` — never `Math.random`, which is predictable. */
export function newToken(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(TOKEN_BYTES));
  return base64url(bytes);
}

export async function hashToken(token: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The token out of an `Authorization: Bearer …` header, or null. */
export function bearer(header: string | null): string | null {
  if (!header) return null;
  const match = /^Bearer\s+(.+)$/i.exec(header.trim());
  const token = match?.[1]?.trim();
  return token ? token : null;
}

/**
 * Compare two tokens without leaking where they differ.
 *
 * Account tokens never need this — they are looked up by digest, so the comparison happens
 * inside an index. An agent token does: it is stored in plaintext (the box it belongs to has
 * to present it verbatim, and we have to hand it back to the owner), so this is a real
 * string compare and a naive one would answer faster the sooner it found a mismatch.
 */
export function tokenMatches(expected: string, presented: string): boolean {
  const a = new TextEncoder().encode(expected);
  const b = new TextEncoder().encode(presented);
  // Fold the length difference in rather than returning early on it: the length is not the
  // secret, but a short-circuit here would be one more thing to get wrong later.
  let diff = a.length ^ b.length;
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  }
  return diff === 0;
}

function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/**
 * Is this request allowed to read the numbers?
 *
 * `ADMIN_KEY` is a secret like the rest (see `env.d.ts`); a deployment without one has no
 * admin surface at all rather than an open one. The key may arrive as a bearer token or as
 * `?key=`, because the dashboard is opened by typing a URL into a browser and a browser
 * cannot send a header.
 */
export function adminAllowed(request: Request, url: URL, env: Env): "ok" | "unset" | "denied" {
  const expected = env.ADMIN_KEY;
  if (!expected) return "unset";
  const header = request.headers.get("Authorization");
  const presented = /^Bearer\s+(.+)$/i.exec(header?.trim() ?? "")?.[1] ?? url.searchParams.get("key");
  if (!presented) return "denied";
  return tokenMatches(expected, presented) ? "ok" : "denied";
}

/** How many days a request asked for, clamped to something a dashboard can draw. */
export function windowDays(url: URL): number {
  const asked = Number(url.searchParams.get("days") ?? "30");
  if (!Number.isFinite(asked)) return 30;
  return Math.min(365, Math.max(1, Math.trunc(asked)));
}
