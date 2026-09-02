/**
 * The server browser: where people are actually riding, right now.
 *
 * MX Bikes has its own list, served by PiBoSo's master server over a protocol we cannot
 * speak, so the app has never been able to show one. What it *can* see is where its own
 * users are: every app in a session already reports its presence here, keyed by the server's
 * folded name, because that is the one identifier every rider on a grid computes the same
 * way. Counting those rows by key turns paint sync's bookkeeping into a live list — a
 * server with people on it appears the moment one of them is running the app.
 *
 * Two kinds of row come out of it:
 *
 * - **Registered.** A server somebody put in the registry: it has a name an operator chose,
 *   a region and — the part that matters — an address, so the app can launch the game
 *   straight into it. Listed whether or not anyone is on it, because an empty server you can
 *   join is still somewhere to go.
 * - **Live.** A server nobody registered, seen because riders running the app are on it. It
 *   has a name, a track and a count, and no address: the app never learns one for a server
 *   its user picked out of the game's own browser. Those rows say where the people are; the
 *   game's list is still how you get there.
 *
 * What is deliberately *not* here: presence keyed by `host:port`. That is somebody's server
 * address, and this list is public — see `isAddressKey`.
 */

import { isAddressKey, PRESENCE_TTL_MS } from "./validate";

/** A registry row, as `servers` holds it. `agent_url` is not selected — see `listServers`. */
export interface RegistryRow {
  id: string;
  name: string;
  region: string;
  address: string;
}

/** One server key's worth of presence, already grouped. */
export interface LiveRow {
  server_id: string;
  riders: number;
  /** The most recent reporter's name for the server, where one was reported. */
  server_name: string | null;
  /** And the track they were on. */
  track: string | null;
}

/** A row of the browser. */
export interface BrowseServer {
  /** The registry id, or the presence key for a server nobody registered. */
  id: string;
  name: string;
  /** Only a registered server has one — it is a field of the registry, not of the game. */
  region: string | null;
  /** `host:port`, and `null` for a server we only know about because people are on it. */
  address: string | null;
  /** What the riders there are on, when one of them told us. */
  track: string | null;
  /** Riders on it whose app is reporting. A floor, never a total — see the app's own note. */
  riders: number;
  registered: boolean;
}

/**
 * The key a rider's app reports while they are in a session.
 *
 * Must fold exactly the way `voice::session::room_key` does, because that is what is in the
 * table: whitespace collapsed, then lowercased. A registered server whose riders are keyed
 * by its name would otherwise show 0 people on it while a second, nameless row beside it
 * showed all of them.
 */
export function nameKey(name: string): string {
  return name.trim().split(/\s+/).filter(Boolean).join(" ").toLowerCase();
}

/**
 * Merge the registry with what presence has seen.
 *
 * Pure, so the merge — which is where the two halves can disagree — is testable without a
 * database.
 */
export function compose(registry: RegistryRow[], live: LiveRow[]): BrowseServer[] {
  const byKey = new Map<string, LiveRow>();
  for (const row of live) byKey.set(row.server_id.trim().toLowerCase(), row);

  const claimed = new Set<string>();
  const take = (key: string | undefined): LiveRow | undefined => {
    if (!key) return undefined;
    const row = byKey.get(key);
    if (row) claimed.add(key);
    return row;
  };

  const servers: BrowseServer[] = registry.map((s) => {
    // A registered server can be keyed either way: by its id when the app resolved an
    // address against the registry before the game was up, and by its folded name once the
    // rider is in the session. Both are the same grid, so both are counted.
    const byId = take(s.id.trim().toLowerCase());
    const byName = take(nameKey(s.name));
    const seen = [byId, byName].filter((r): r is LiveRow => r !== undefined);
    return {
      id: s.id,
      name: s.name,
      region: s.region,
      address: s.address,
      track: seen.find((r) => r.track)?.track ?? null,
      riders: seen.reduce((n, r) => n + r.riders, 0),
      registered: true,
    };
  });

  for (const [key, row] of byKey) {
    if (claimed.has(key)) continue;
    // Somebody's address, not a server's name. Not ours to publish.
    if (isAddressKey(key)) continue;
    servers.push({
      id: key,
      // The key is the name, folded. The reported one is the same name with its own
      // capitals, so prefer it and fall back to the fold rather than showing nothing.
      name: row.server_name ?? key,
      region: null,
      address: null,
      track: row.track,
      riders: row.riders,
      registered: false,
    });
  }

  // Busiest first — the whole question a browser answers is "where is anyone" — then by
  // name, so a list of empty servers doesn't reshuffle itself between refreshes.
  return servers.sort((a, b) => b.riders - a.riders || a.name.localeCompare(b.name));
}

/**
 * `GET /v1/browse`.
 *
 * Unauthenticated, on the same terms as the registry it extends: a player who has never
 * enrolled is exactly the one asking where to ride, and nothing here is a secret — a server
 * name, a track and a head count are what the game's own browser shows everybody.
 */
export async function browseServers(env: Env): Promise<Response> {
  const registry = await env.DB.prepare(
    "SELECT id, name, region, address FROM servers WHERE published = 1",
  ).all<RegistryRow>();

  // `server_name` and `track` are bare columns beside a `MAX()`, which SQLite answers from
  // the row the maximum came from: the most recent report wins, so a server that changed
  // track is described by whoever is on it now rather than by whoever arrived first.
  const live = await env.DB.prepare(
    "SELECT server_id, COUNT(*) AS riders, MAX(updated_at) AS seen_at, server_name, track" +
      " FROM presence WHERE updated_at > ? GROUP BY server_id",
  )
    .bind(Date.now() - PRESENCE_TTL_MS)
    .all<LiveRow & { seen_at: number }>();

  return json(200, {
    servers: compose(registry.results ?? [], live.results ?? []),
    // What "riders" is a count over, so the app can say how stale the number can be.
    ttlMs: PRESENCE_TTL_MS,
  });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
