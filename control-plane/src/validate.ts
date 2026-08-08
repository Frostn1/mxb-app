/**
 * Input validation for the public API.
 *
 * Kept as pure functions so the rules are testable without a Worker, a database or a
 * request — most of the bugs worth catching here are about which strings are acceptable,
 * not about plumbing.
 */

/**
 * Slot names, which are the `profile.ini` section names the game itself uses — not a
 * vocabulary of our own. The app reads and writes these sections directly, so inventing
 * parallel names would mean a translation layer on both sides and a mismatch the first time
 * one of them gained a slot.
 */
export const SLOTS = [
  "paint",
  "bike_font",
  "helmet_paint",
  "goggles_paint",
  "suit_paint",
  "suit_font",
  "boots_paint",
  "gloves_paint",
  "protection_paint",
] as const;
export type Slot = (typeof SLOTS)[number];

export function isSlot(value: unknown): value is Slot {
  return typeof value === "string" && (SLOTS as readonly string[]).includes(value);
}

/**
 * A rider name that can survive the round trip through the game's roster.
 *
 * The name is the join key between "who is on this server" and "whose paints do I fetch",
 * so it has to match exactly what MX Bikes reports. Control characters would never come
 * back the same, and an empty or absurdly long name is not a real rider.
 */
export function isRiderName(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const name = value.trim();
  if (name.length < 2 || name.length > 64) return false;
  // eslint-disable-next-line no-control-regex
  return !/[\x00-\x1f\x7f]/.test(name);
}

/**
 * An MX Bikes player GUID.
 *
 * Deliberately loose on format — the exact shape PiBoSo emits hasn't been confirmed against
 * a real connection, so this checks only that it is a plausible opaque identifier rather
 * than inventing a pattern that might reject valid ones. Tightened once a real GUID is seen.
 */
export function isGuid(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const g = value.trim();
  if (g.length < 4 || g.length > 100) return false;
  // No whitespace: the server log delimits the GUID by whitespace, so one containing any
  // could never have come from there.
  return /^[A-Za-z0-9._:-]+$/.test(g);
}

/** Lowercase 64-char hex — the shape of a SHA-256 digest, which is also the R2 object key. */
export function isSha256(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

/**
 * A paint filename safe to write into another player's mods folder.
 *
 * This is the one input that becomes a *path on someone else's disk*, so it is the one
 * place a traversal would matter: a name of `../../mxbikes.ini` would otherwise be written
 * wherever the client joins it. Reject separators and traversal outright rather than trying
 * to sanitise, and require the extension the game actually loads.
 */
export function isPaintFileName(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const name = value.trim();
  if (name.length === 0 || name.length > 128) return false;
  if (name.includes("/") || name.includes("\\") || name.includes("..")) return false;
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f\x7f:*?"<>|]/.test(name)) return false;
  return name.toLowerCase().endsWith(".pnt");
}

/**
 * A destination path, relative to the receiver's `mods` folder, that is safe to write.
 *
 * This is the most dangerous value in the whole API: one player uploads it and another
 * player's app joins it onto a real directory. A value of `../../../mxbikes.ini`, an
 * absolute path, or a Windows drive letter would each escape the mods folder entirely. It
 * is rejected here and again on the client, because only the client's check actually
 * protects a disk — this one just stops the bad value being stored and served.
 */
export function isRelDest(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const p = value.trim();
  if (p.length === 0 || p.length > 256) return false;
  // Forward slashes only, so there is a single form to reason about.
  if (p.includes("\\")) return false;
  // No absolute paths, no drive letters, no UNC.
  if (p.startsWith("/") || /^[a-z]:/i.test(p)) return false;
  // No traversal, in any segment.
  const segments = p.split("/");
  if (segments.some((s) => s === "" || s === "." || s === "..")) return false;
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f\x7f:*?"<>|]/.test(p)) return false;
  // The last segment has to be the paint itself, under the same rules as any filename.
  return isPaintFileName(segments[segments.length - 1]);
}

/** Paints are single-digit megabytes; anything far past that is not a paint. */
export const MAX_PAINT_BYTES = 32 * 1024 * 1024;

export function isPaintSize(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0 && value <= MAX_PAINT_BYTES;
}
