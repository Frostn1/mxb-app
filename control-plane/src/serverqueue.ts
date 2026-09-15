/**
 * The line for a full server.
 *
 * MX Bikes has no queue: a full server turns you away and the only way in is to keep
 * pressing Join until someone leaves. This keeps a fair order among riders using the app.
 *
 * The control plane can't see the server (Workers have no UDP), so it only answers "how many
 * are ahead of me". Each app launches when `players + ahead < max`. A rider who launched still
 * counts as ahead for `LAUNCH_GRACE_MS`, because the server doesn't count them while the game
 * loads — without that, everyone behind would see the same free slot and rush it.
 *
 * Only the front waiting rider reads the server's rider count and sends it with their
 * heartbeat; everyone behind gets it back in theirs. The game server is the box we don't run,
 * so it sees one probe a beat however long the line is. A count older than `PROBE_FRESH_MS`
 * sends everyone back to probing until someone reports again.
 *
 * Riders spamming Join from inside the game can still beat the line. It is fair among app
 * users, which is the part we control.
 */

import { isServerKey } from "./validate";

/** A rider whose app stops heartbeating for this long has left the line. The app beats every 10 s. */
export const QUEUE_TTL_MS = 45_000;

/** How long a launched rider keeps counting as ahead while their game loads in. */
export const LAUNCH_GRACE_MS = 120_000;

/** How long a reported rider count stands in for everyone's own probe. Two beats. */
export const PROBE_FRESH_MS = 25_000;

/** Rows past this are swept by the cron. Reads already ignore them after the TTL. */
const PRUNE_AFTER_MS = 60 * 60 * 1000;

/** MX Bikes sends both counts as a byte. */
function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 255;
}

/**
 * `PUT /v1/queue {server, launched?, players?, maxPlayers?}`: join the line or heartbeat, and
 * learn your place. The counts are sent by whoever the last reply told to probe.
 */
export async function putQueue(request: Request, accountId: string, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body || typeof body !== "object") return json(400, { error: "expected a JSON body" });
  const { server, launched, players, maxPlayers } = body as {
    server?: unknown;
    launched?: unknown;
    players?: unknown;
    maxPlayers?: unknown;
  };
  if (!isServerKey(server)) return json(400, { error: "that isn't a server" });
  const probed = players !== undefined || maxPlayers !== undefined;
  if (probed && !(isCount(players) && isCount(maxPlayers))) {
    return json(400, { error: "players and maxPlayers go together, as counts" });
  }
  const key = server.trim();
  const now = Date.now();
  const fresh = now - QUEUE_TTL_MS;

  // Same server and still fresh keeps your place; anything else puts you at the back.
  // SQLite reads every right-hand side against the old row, so the CASEs see the old values.
  // A new count replaces the old; a heartbeat without one keeps it, unless the server changed.
  await env.DB.prepare(
    "INSERT INTO server_queue" +
      " (account_id, server_id, joined_at, updated_at, launched_at, players, max_players, probed_at)" +
      " VALUES (?, ?, ?, ?, ?, ?, ?, ?)" +
      " ON CONFLICT(account_id) DO UPDATE SET" +
      "  joined_at = CASE WHEN server_id = excluded.server_id AND updated_at > ?" +
      "   THEN joined_at ELSE excluded.joined_at END," +
      "  launched_at = CASE WHEN server_id = excluded.server_id AND updated_at > ?" +
      "   THEN COALESCE(launched_at, excluded.launched_at) ELSE excluded.launched_at END," +
      "  players = CASE WHEN excluded.probed_at IS NOT NULL OR server_id <> excluded.server_id" +
      "   THEN excluded.players ELSE players END," +
      "  max_players = CASE WHEN excluded.probed_at IS NOT NULL OR server_id <> excluded.server_id" +
      "   THEN excluded.max_players ELSE max_players END," +
      "  probed_at = CASE WHEN excluded.probed_at IS NOT NULL OR server_id <> excluded.server_id" +
      "   THEN excluded.probed_at ELSE probed_at END," +
      "  server_id = excluded.server_id," +
      "  updated_at = excluded.updated_at",
  )
    .bind(
      accountId,
      key,
      now,
      now,
      launched === true ? now : null,
      probed ? players : null,
      probed ? maxPlayers : null,
      probed ? now : null,
      fresh,
      fresh,
    )
    .run();

  return json(200, await place(accountId, key, now, env));
}

/** `DELETE /v1/queue`: leave the line, whichever server it was for. */
export async function leaveQueue(accountId: string, env: Env): Promise<Response> {
  await env.DB.prepare("DELETE FROM server_queue WHERE account_id = ?").bind(accountId).run();
  return json(200, { ok: true });
}

/** `GET /v1/queue/counts?server=a,b`: how many are waiting, for the browser's badges. */
export async function queueCounts(url: URL, env: Env): Promise<Response> {
  const keys = url.searchParams
    .getAll("server")
    .flatMap((v) => v.split(","))
    .map((v) => v.trim())
    .filter((v) => isServerKey(v));
  // Bounded because it becomes an IN list.
  const wanted = [...new Set(keys)].slice(0, 8);
  if (wanted.length === 0) return json(400, { error: "a server id is required" });

  const rows = await env.DB.prepare(
    "SELECT server_id, COUNT(*) AS waiting FROM server_queue" +
      ` WHERE server_id IN (${wanted.map(() => "?").join(", ")})` +
      " AND updated_at > ? AND launched_at IS NULL GROUP BY server_id",
  )
    .bind(...wanted, Date.now() - QUEUE_TTL_MS)
    .all<{ server_id: string; waiting: number }>();

  const servers: Record<string, number> = {};
  for (const r of rows.results) servers[r.server_id] = r.waiting;
  return json(200, { servers });
}

/** Sweep rows long past mattering. On the 5-minute cron. */
export async function pruneQueue(env: Env): Promise<void> {
  try {
    await env.DB.prepare("DELETE FROM server_queue WHERE updated_at < ?")
      .bind(Date.now() - PRUNE_AFTER_MS)
      .run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "queue sweep failed", error: String(err) }));
  }
}

/** A rider's answer: their place, whether to probe the server, and the line's latest count. */
export interface Place {
  position: number;
  ahead: number;
  waiting: number;
  probe: boolean;
  players: number | null;
  maxPlayers: number | null;
}

/**
 * Where a rider stands. `ahead` counts fresh riders who joined earlier, waiting or launched
 * within the grace; ties on `joined_at` break by account id so two apps never both see 0.
 * The rider with no one *waiting* ahead is the one who probes.
 */
async function place(accountId: string, key: string, now: number, env: Env): Promise<Place> {
  const earlier =
    "(q.joined_at < me.joined_at OR (q.joined_at = me.joined_at AND q.account_id < me.account_id))";
  const row = await env.DB.prepare(
    "WITH me AS (SELECT account_id, joined_at, launched_at FROM server_queue WHERE account_id = ?)," +
      " probe AS (SELECT players, max_players FROM server_queue" +
      "  WHERE server_id = ? AND updated_at > ? AND probed_at > ?" +
      "  ORDER BY probed_at DESC LIMIT 1)" +
      " SELECT" +
      `  SUM(CASE WHEN ${earlier} THEN 1 ELSE 0 END) AS ahead,` +
      `  SUM(CASE WHEN ${earlier} AND q.launched_at IS NULL THEN 1 ELSE 0 END) AS waiting_ahead,` +
      "  SUM(CASE WHEN q.launched_at IS NULL THEN 1 ELSE 0 END) AS waiting," +
      "  (SELECT launched_at FROM me) AS launched_at," +
      "  (SELECT players FROM probe) AS players," +
      "  (SELECT max_players FROM probe) AS max_players" +
      " FROM server_queue q, me" +
      " WHERE q.server_id = ? AND q.updated_at > ?" +
      "  AND (q.launched_at IS NULL OR q.launched_at > ?)",
  )
    .bind(
      accountId,
      key,
      now - QUEUE_TTL_MS,
      now - PROBE_FRESH_MS,
      key,
      now - QUEUE_TTL_MS,
      now - LAUNCH_GRACE_MS,
    )
    .first<{
      ahead: number | null;
      waiting_ahead: number | null;
      waiting: number | null;
      launched_at: number | null;
      players: number | null;
      max_players: number | null;
    }>();

  const ahead = row?.ahead ?? 0;
  const players = row?.players ?? null;
  const maxPlayers = row?.max_players ?? null;
  const front = row?.launched_at == null && (row?.waiting_ahead ?? 0) === 0;
  return {
    position: ahead + 1,
    ahead,
    waiting: row?.waiting ?? 0,
    probe: front || players === null || maxPlayers === null,
    players,
    maxPlayers,
  };
}

async function readJson(request: Request): Promise<unknown | null> {
  try {
    return await request.json();
  } catch {
    return null;
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
