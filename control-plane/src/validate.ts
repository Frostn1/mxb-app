/**
 * Input validation for the public API.
 *
 * Kept as pure functions so the rules are testable without a Worker, a database or a
 * request — most of the bugs worth catching here are about which strings are acceptable,
 * not about plumbing.
 */

/** Paint slots the game actually has. Anything else is a client bug or a probe. */
export const SLOTS = ["bike", "helmet", "boots", "goggles", "gear", "protections"] as const;
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

/** Paints are single-digit megabytes; anything far past that is not a paint. */
export const MAX_PAINT_BYTES = 32 * 1024 * 1024;

export function isPaintSize(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0 && value <= MAX_PAINT_BYTES;
}
