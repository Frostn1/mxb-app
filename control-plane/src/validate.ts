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

/**
 * A server's display name in the public list.
 *
 * Shown to every player in the join picker, so control characters — which would break the
 * line — and absurd lengths are rejected. Otherwise it is the operator's to choose.
 */
export function isServerName(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const name = value.trim();
  if (name.length < 2 || name.length > 48) return false;
  // eslint-disable-next-line no-control-regex
  return !/[\x00-\x1f\x7f]/.test(name);
}

/** Regions are ours to define, so this is a closed set rather than free text. */
export const REGIONS = [
  "eu-central-1",
  "eu-west-1",
  "us-east-1",
  "us-west-2",
  "ap-southeast-2",
] as const;

export function isRegion(value: unknown): value is (typeof REGIONS)[number] {
  return typeof value === "string" && (REGIONS as readonly string[]).includes(value);
}

/**
 * Hosts that must never be reached from a URL a user supplied.
 *
 * The control plane fetches an operator-supplied agent URL to check the server is real, and
 * that makes it a request-forgery surface. Without this, anyone with an account could aim
 * our egress at `127.0.0.1`, at cloud metadata on `169.254.169.254`, or at private space
 * inside whatever network the request originates from, and read back the result.
 *
 * Literal IPv4 is checked numerically. A *hostname* that resolves into private space cannot
 * be caught here — DNS is not available to this check — which is one more reason the agent
 * authenticates every call it serves rather than trusting who reached it.
 */
function isPrivateHost(host: string): boolean {
  const h = host.toLowerCase().trim();
  if (h.length === 0) return true;
  if (h === "localhost" || h.endsWith(".localhost") || h.endsWith(".local")) return true;
  // IPv6 in any form. Loopback, link-local and unique-local are all unroutable, and there
  // is no v6 agent address in use yet, so the whole family is refused rather than parsed.
  if (h.includes(":") || h.startsWith("[")) return true;

  const parts = h.split(".");
  if (parts.length !== 4 || !parts.every((p) => /^\d{1,3}$/.test(p))) {
    // Not a literal IPv4, so it is a hostname — nothing further to check numerically.
    return false;
  }
  const octets = parts.map(Number);
  if (octets.some((n) => n > 255)) return true;
  const [a, b] = octets;
  if (a === 0 || a === 10 || a === 127) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 192 && b === 168) return true;
  // Carrier-grade NAT, link-local (which is where cloud metadata lives), multicast upward.
  if (a === 100 && b >= 64 && b <= 127) return true;
  if (a === 169 && b === 254) return true;
  if (a >= 224) return true;
  return false;
}

/**
 * A `host:port` a player's game can actually be pointed at.
 *
 * This goes into the public list and from there onto a command line, so it is held to the
 * same shape the app's own parser enforces: IPv4 or a hostname, an explicit port, and
 * nothing that could be read as a second argument.
 */
export function isPublicGameAddress(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const addr = value.trim();
  if (addr.length === 0 || addr.length > 128) return false;
  const at = addr.lastIndexOf(":");
  if (at <= 0) return false;
  const host = addr.slice(0, at);
  const port = Number(addr.slice(at + 1));
  if (!Number.isInteger(port) || port < 1 || port > 65535) return false;
  if (!/^[A-Za-z0-9.-]+$/.test(host)) return false;
  if (host.startsWith("-") || host.startsWith(".") || host.endsWith(".")) return false;
  return !isPrivateHost(host);
}

/**
 * An agent base URL the control plane is willing to call.
 *
 * Plain http/https, no embedded credentials, no query or fragment, no path — and never a
 * private or loopback host, per [`isPrivateHost`].
 */
export function isPublicAgentUrl(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const raw = value.trim();
  if (raw.length === 0 || raw.length > 256) return false;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return false;
  if (url.username || url.password) return false;
  if (url.search || url.hash) return false;
  if (url.pathname !== "/" && url.pathname !== "") return false;
  return !isPrivateHost(url.hostname);
}

/**
 * A bike id, as it appears as a key in the game's `profile.ini`.
 *
 * Part of a primary key now that loadouts are per bike, and the value the app matches on when
 * deciding which paints belong to the bike a rider is on — so it has to be a plain name.
 * Accepted unvalidated until this point, which meant a control character or a 4 KB string
 * could go straight into the table and come back out in every roster.
 */
export function isBikeId(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const id = value.trim();
  if (id.length === 0 || id.length > 128) return false;
  if (/[\u0000-\u001f\u007f]/.test(id)) return false;
  // Not a path and not a separator: this is a key, and it is echoed into JSON that other
  // players' apps read.
  return !/[/\\]/.test(id);
}

/** How much of a failed bootstrap's transcript is worth keeping. */
export const MAX_BOOTSTRAP_LOG = 16 * 1024;

/**
 * A bootstrap stage name.
 *
 * Written by a script on a machine we launched, and read back into the app's UI, so it is
 * held to being a short plain label rather than trusted because of where it came from.
 */
export function isBootstrapStage(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const stage = value.trim();
  if (stage.length === 0 || stage.length > 64) return false;
  return /^[a-z0-9 :\-]+$/i.test(stage);
}

