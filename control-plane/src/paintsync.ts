/**
 * Paint sync v2: automatic, per server, delta only.
 *
 * The app says "I'm on this server wearing these paints" (`POST /v1/paintsync/join`) and gets
 * back the hashes of its own paints nobody has stored yet, and everyone else on that server
 * with theirs. It uploads only the first list and downloads only what its own disk lacks from
 * the second. A room per server (`paintroom.ts`) pushes later arrivals, so nothing polls.
 *
 * Paints uploaded this way live under `live/` in R2 for a day and are then swept. The older
 * flow's objects at the bucket root are left alone: shipped apps still read them.
 */

/** A paint uploaded through this flow is served for this long after its last upload. */
export const LIVE_TTL_MS = 24 * 60 * 60 * 1000;

/**
 * Past this age a stored paint is reported as missing again, so the rider wearing it
 * re-uploads it and the clock restarts, well before the sweep would take it from under the
 * people still riding with them.
 */
export const LIVE_REFRESH_MS = 20 * 60 * 60 * 1000;

/** Where this flow's paints live in R2. */
export const LIVE_PREFIX = "live/";

export const liveKey = (sha256: string): string => `${LIVE_PREFIX}${sha256}`;

/** A server as the app knows it: an address when it launched the join, a name when FrostMod saw one. */
export interface ServerHint {
  address: string | null;
  name: string | null;
}

/** Fold a server name the way the app's `room_key` does: trimmed, lowercased, spaces collapsed. */
export function foldName(name: string): string {
  return name.trim().toLowerCase().replace(/\s+/g, " ");
}

/**
 * `host:port`, lowercased, or null. A host is a dotted IPv4 address or a DNS name; the port
 * is what the game connects to, so it is required.
 */
export function normalizeAddress(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const m = /^([A-Za-z0-9.-]{1,253}):(\d{1,5})$/.exec(value.trim());
  if (!m) return null;
  const port = Number(m[2]);
  if (port < 1 || port > 65535) return null;
  return `${m[1]!.toLowerCase()}:${port}`;
}

/** Validate `{ address?, name? }`; at least one of them must be usable. */
export function parseServerHint(value: unknown): ServerHint | string {
  const v = (value ?? {}) as { address?: unknown; name?: unknown };
  const address = v.address == null || v.address === "" ? null : normalizeAddress(v.address);
  if (v.address != null && v.address !== "" && !address) return "that isn't a server address";
  let name: string | null = null;
  if (typeof v.name === "string" && v.name.trim()) {
    name = v.name.trim();
    // eslint-disable-next-line no-control-regex
    if (name.length > 64 || /[\u0000-\u001f\u007f]/.test(name)) return "that isn't a server name";
  }
  if (!address && !name) return "a server address or name is required";
  return { address, name };
}

/**
 * The room key for a server.
 *
 * `addr:<host:port>` whenever an address is known, which is every join the app launched.
 * A name alone — the rider picked the server in the game's own browser — is mapped to the
 * address most recently seen with that name, and only when nothing has been seen does it get a
 * room of its own. Both halves are remembered, which is what traces a server that moved.
 */
export async function resolveServer(hint: ServerHint, env: Env, now = Date.now()): Promise<string> {
  if (hint.address) {
    if (hint.name) {
      await env.DB.prepare(
        "INSERT INTO paint_servers (name_key, address, name, last_seen) VALUES (?, ?, ?, ?)" +
          " ON CONFLICT(name_key, address) DO UPDATE SET name = excluded.name, last_seen = excluded.last_seen",
      )
        .bind(foldName(hint.name), hint.address, hint.name, now)
        .run();
    }
    return `addr:${hint.address}`;
  }
  const nameKey = foldName(hint.name!);
  const seen = await env.DB.prepare(
    "SELECT address FROM paint_servers WHERE name_key = ? AND last_seen > ? ORDER BY last_seen DESC LIMIT 1",
  )
    .bind(nameKey, now - LIVE_TTL_MS)
    .first<{ address: string }>();
  return seen ? `addr:${seen.address}` : `name:${nameKey}`;
}

export interface RiderPaint {
  slot: string;
  fileName: string;
  sha256: string;
  size: number;
  relDest: string;
}

export interface RiderView {
  accountId: string;
  riderName: string;
  guid: string | null;
  joinedAt: number;
  paints: RiderPaint[];
}

/**
 * Everyone present on `serverKey`, with what they wear, oldest arrival first.
 *
 * `isRelDest` is passed in rather than imported so this stays a query and a fold; the caller
 * owns validation, the same way `roster` re-checks destinations on the way out.
 */
export async function ridersOn(
  serverKey: string,
  env: Env,
  presenceTtlMs: number,
  isRelDest: (v: unknown) => boolean,
  now = Date.now(),
): Promise<RiderView[]> {
  const rows = await env.DB.prepare(
    "SELECT a.id AS account_id, a.rider_name, a.guid, COALESCE(pr.joined_at, pr.updated_at) AS joined_at," +
      " MIN(p.slot) AS slot, p.file_name, p.sha256, p.size, p.rel_dest" +
      " FROM presence pr" +
      " JOIN accounts a ON a.id = pr.account_id" +
      " LEFT JOIN loadout_paints p ON p.account_id = a.id" +
      " WHERE pr.server_id = ? AND pr.updated_at > ?" +
      " GROUP BY a.id, p.rel_dest, p.sha256",
  )
    .bind(serverKey, now - presenceTtlMs)
    .all<{
      account_id: string;
      rider_name: string;
      guid: string | null;
      joined_at: number;
      slot: string | null;
      file_name: string | null;
      sha256: string | null;
      size: number | null;
      rel_dest: string | null;
    }>();

  const riders = new Map<string, RiderView>();
  for (const r of rows.results ?? []) {
    let rider = riders.get(r.account_id);
    if (!rider) {
      rider = { accountId: r.account_id, riderName: r.rider_name, guid: r.guid, joinedAt: r.joined_at, paints: [] };
      riders.set(r.account_id, rider);
    }
    if (r.sha256 && r.rel_dest && r.file_name && r.slot && isRelDest(r.rel_dest)) {
      rider.paints.push({ slot: r.slot, fileName: r.file_name, sha256: r.sha256, size: r.size ?? 0, relDest: r.rel_dest });
    }
  }
  return [...riders.values()].sort((a, b) => a.joinedAt - b.joinedAt);
}

/** What a client is shown of a rider: never the account id. */
export function publicRider(r: RiderView): Omit<RiderView, "accountId"> {
  return { riderName: r.riderName, guid: r.guid, joinedAt: r.joinedAt, paints: r.paints };
}

/**
 * Which of `shas` nobody has stored, or stored so long ago it is about to be swept.
 *
 * The bucket root counts: those are the older flow's objects, kept indefinitely, and a paint
 * already there is a paint nobody needs to upload again.
 */
export async function missingPaints(shas: string[], env: Env, now = Date.now()): Promise<string[]> {
  const missing: string[] = [];
  for (const sha of new Set(shas)) {
    if (await env.PAINTS.head(sha)) continue;
    const live = await env.PAINTS.head(liveKey(sha));
    if (live && now - live.uploaded.getTime() < LIVE_REFRESH_MS) continue;
    missing.push(sha);
  }
  return missing;
}

/** How many `live/` objects one sweep looks at. The cron runs every five minutes. */
const SWEEP_PAGE = 1000;
const SWEEP_PAGES = 5;

/**
 * Drop this flow's paints a day after their last upload, the loadout rows that point at them,
 * and server names nobody has reported for a day.
 *
 * The rows go with the objects so an older app reading `/v1/roster` is never handed a hash
 * that 404s — its pull stops at the first failed download. A rider still on the server sends
 * their look again on the next heartbeat, and the upload that follows restores both.
 */
export async function pruneLivePaints(env: Env, now = Date.now()): Promise<void> {
  try {
    const expired: string[] = [];
    let cursor: string | undefined;
    for (let page = 0; page < SWEEP_PAGES; page += 1) {
      const listed = await env.PAINTS.list({ prefix: LIVE_PREFIX, limit: SWEEP_PAGE, cursor });
      for (const o of listed.objects) {
        if (now - o.uploaded.getTime() > LIVE_TTL_MS) expired.push(o.key);
      }
      if (!listed.truncated) break;
      cursor = listed.cursor;
    }
    for (let i = 0; i < expired.length; i += 1000) {
      await env.PAINTS.delete(expired.slice(i, i + 1000));
    }
    for (const key of expired) {
      const sha = key.slice(LIVE_PREFIX.length);
      if (await env.PAINTS.head(sha)) continue;
      await env.DB.prepare("DELETE FROM loadout_paints WHERE sha256 = ?").bind(sha).run();
    }
    await env.DB.prepare("DELETE FROM paint_servers WHERE last_seen < ?").bind(now - LIVE_TTL_MS).run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "paint sweep failed", error: String(err) }));
  }
}
