import type { Friend } from "../api/friends";

/** The part of a server row that finding a friend on it needs. */
export interface ServerRef {
  name: string;
  address: string;
  players?: number;
}

/**
 * A server name the way FrostMod's session block and the control plane compare it: whitespace
 * collapsed, case folded. Mirrors `voice::session::room_key` in the Rust side, so one server
 * is one name however it is punctuated on the day.
 */
export function foldName(name: string): string {
  return name.split(/\s+/).filter(Boolean).join(" ").toLowerCase();
}

/**
 * Which listed server a friend is on.
 *
 * The address wins when the friend's app sent one, because it is exact and the app only sends
 * it while the session is still on the server it launched into. Otherwise the folded name, and
 * where two listed servers share a name the busier one: a friend is far likelier to be on the
 * server with riders than on its empty twin.
 */
export function serverForFriend<S extends ServerRef>(friend: Friend, servers: readonly S[]): S | null {
  const presence = friend.presence;
  if (!presence) return null;
  if (presence.address) {
    const address = presence.address.toLowerCase();
    const exact = servers.find((s) => s.address.toLowerCase() === address);
    if (exact) return exact;
  }
  const name = foldName(presence.serverName);
  if (!name) return null;
  let best: S | null = null;
  for (const s of servers) {
    if (foldName(s.name) !== name) continue;
    if (!best || (s.players ?? 0) > (best.players ?? 0)) best = s;
  }
  return best;
}

/** Friends standing on each listed server, keyed by the server's address as listed. */
export function friendsByAddress<S extends ServerRef>(
  friends: readonly Friend[],
  servers: readonly S[],
): Record<string, Friend[]> {
  const out: Record<string, Friend[]> = {};
  for (const f of friends) {
    const server = serverForFriend(f, servers);
    if (!server) continue;
    (out[server.address] ??= []).push(f);
  }
  return out;
}

/** Online friends first, then by name. Offline friends sort last. */
export function sortFriends(friends: readonly Friend[]): Friend[] {
  return [...friends].sort((a, b) => {
    const online = Number(b.presence !== null) - Number(a.presence !== null);
    return online || a.riderName.localeCompare(b.riderName);
  });
}

/**
 * Whether what was typed reads as a friend code rather than the start of a name.
 *
 * `ABCD-EFGH` always does. Eight bare characters do only when one is a digit, because a plain
 * eight-letter name such as "Motorist" is far likelier to be a person than a code, and the
 * search box finds it either way.
 */
export function looksLikeCode(input: string): boolean {
  const trimmed = input.trim();
  const alphabet = "[ABCDEFGHJKLMNPQRSTUVWXYZ2-9]";
  if (new RegExp(`^${alphabet}{4}[- ]${alphabet}{4}$`, "i").test(trimmed)) return true;
  return new RegExp(`^${alphabet}{8}$`, "i").test(trimmed) && /[2-9]/.test(trimmed);
}
