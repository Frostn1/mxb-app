/**
 * Telling a live share code from a static one, in the one place the UI asks.
 *
 * `MXBS1-` carries the whole share as base64; `MXBL1-` carries eight Crockford characters
 * that the control plane resolves. They arrive through the same paste, the same import box
 * and the same drop, so every entrance needs the same answer — see
 * `src-tauri/src/liveshare.rs`, which folds codes the same way.
 */

export const LIVE_PREFIX = "MXBL1-";

/** Crockford base32 minus the letters that get misheard: no I, L, O or U. */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * A code as it might be typed rather than as it was printed: any case, with or without the
 * prefix, with the spaces and dashes someone added, and with `O`/`I`/`L` read as the digits
 * Crockford means them to be. Null when what is left isn't a code.
 */
export function normaliseLiveCode(text: string): string | null {
  let s = text.trim().toUpperCase();
  if (s.startsWith(LIVE_PREFIX)) s = s.slice(LIVE_PREFIX.length);
  s = s.replace(/[\s-]/g, "").replace(/O/g, "0").replace(/[IL]/g, "1");
  if (s.length !== 8) return null;
  return [...s].every((c) => ALPHABET.includes(c)) ? s : null;
}

/** True when this text is a live code, so the import path can route it. */
export function isLiveCode(text: string): boolean {
  const t = text.trim();
  return t.toUpperCase().startsWith(LIVE_PREFIX) && normaliseLiveCode(t) !== null;
}
