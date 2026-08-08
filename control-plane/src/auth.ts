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

function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
