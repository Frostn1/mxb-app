/**
 * Friends: a relation between two MXB App accounts, and where an accepted friend is riding.
 *
 * ## Privacy
 *
 * Presence is only ever read for the caller's *accepted* friends. There is no endpoint that
 * lists who is where in general, and a pending request reveals nothing beyond a display name.
 * "Hide my presence" is enforced at write time as well as read time: while it is on, a report
 * is acknowledged and dropped, and any row already stored is deleted, so there is nothing to leak.
 *
 * ## Abuse
 *
 * Every route is behind the account's bearer token and `FRIENDS_LIMITER`, keyed per account and
 * split into a read bucket and a write bucket so a heartbeat never starves a request. Searching
 * needs three characters and answers ten names. Outgoing requests and friend lists are capped,
 * and a declined request stays declined: the requester is not told.
 *
 * The free text that reaches another person (a server name) is length-bounded and stripped of
 * control characters before it is stored, and only accepted friends can ever read it.
 */

import { isPublicGameAddress } from "./validate";

interface Account {
  id: string;
  rider_name: string;
}

/** A heartbeat older than this means the rider is gone. The app beats every 30 s. */
export const FRIEND_PRESENCE_TTL_MS = 2 * 60 * 1000;
export const MAX_FRIENDS = 200;
export const MAX_OUTGOING = 30;
export const SEARCH_MIN = 3;
export const SEARCH_LIMIT = 10;
const MAX_BODY_BYTES = 2048;
/** No I, O, 0 or 1: a code read aloud or off a screenshot must survive being misread. */
const CODE_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH = 8;

/** `ABCD-EFGH` for display; stored as `ABCDEFGH`. */
export function formatCode(code: string): string {
  return `${code.slice(0, 4)}-${code.slice(4)}`;
}

/** The stored form of what a person typed, or null if it cannot be a code. */
export function normalizeCode(input: unknown): string | null {
  if (typeof input !== "string") return null;
  const code = input.replace(/[\s-]/g, "").toUpperCase();
  if (code.length !== CODE_LENGTH) return null;
  for (const ch of code) if (!CODE_ALPHABET.includes(ch)) return null;
  return code;
}

function newCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(CODE_LENGTH));
  let code = "";
  // The alphabet is 32 long and 256 is a multiple of 32, so the modulo is unbiased.
  for (const b of bytes) code += CODE_ALPHABET[b % CODE_ALPHABET.length];
  return code;
}

/** Trimmed, control and bidi characters removed, bounded. Empty string if nothing is left. */
export function cleanText(value: unknown, max: number): string {
  if (typeof value !== "string") return "";
  return value
    .replace(/[\u0000-\u001f\u007f​-‏‪-‮⁦-⁩]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, max);
}

function json(status: number, body: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

async function readBody(request: Request): Promise<Record<string, unknown> | null> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (declared > MAX_BODY_BYTES) return null;
  try {
    const text = await request.text();
    if (text.length > MAX_BODY_BYTES) return null;
    const parsed: unknown = JSON.parse(text);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

async function limited(account: Account, bucket: "read" | "write", env: Env): Promise<Response | null> {
  if (!env.FRIENDS_LIMITER) return null;
  if ((await env.FRIENDS_LIMITER.limit({ key: `${account.id}:${bucket}` })).success) return null;
  return json(429, { error: "too many friend requests, wait a minute" }, { "Retry-After": "60" });
}

/** This account's friend profile, created on first use. */
async function profileFor(account: Account, env: Env): Promise<{ code: string; hide: boolean }> {
  const read = () =>
    env.DB.prepare("SELECT friend_code, hide_presence FROM friend_profiles WHERE account_id = ?")
      .bind(account.id)
      .first<{ friend_code: string; hide_presence: number }>();
  const row = await read();
  if (row) return { code: row.friend_code, hide: row.hide_presence === 1 };

  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      await env.DB.prepare(
        "INSERT INTO friend_profiles (account_id, friend_code, created_at) VALUES (?, ?, ?)" +
          " ON CONFLICT(account_id) DO NOTHING",
      )
        .bind(account.id, newCode(), Date.now())
        .run();
    } catch {
      continue; // the code collided with someone else's: draw again
    }
    const made = await read();
    if (made) return { code: made.friend_code, hide: made.hide_presence === 1 };
  }
  throw new Error("could not allocate a friend code");
}

interface PairRow {
  requester_id: string;
  addressee_id: string;
  status: "pending" | "accepted" | "declined";
}

async function pairBetween(a: string, b: string, env: Env): Promise<PairRow | null> {
  return env.DB.prepare(
    "SELECT requester_id, addressee_id, status FROM friendships" +
      " WHERE (requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?)",
  )
    .bind(a, b, b, a)
    .first<PairRow>();
}

/** The one entry point: `/v1/friends...`, already authenticated and ban-checked. */
export async function handleFriends(
  request: Request,
  url: URL,
  account: Account,
  env: Env,
): Promise<Response> {
  const method = request.method;
  const path = url.pathname;

  if (method === "GET" && path === "/v1/friends") {
    return (await limited(account, "read", env)) ?? listFriends(account, env);
  }
  if (method === "PUT" && path === "/v1/friends/presence") {
    return (await limited(account, "read", env)) ?? putFriendPresence(request, account, env);
  }
  if (method === "DELETE" && path === "/v1/friends/presence") {
    return (await limited(account, "read", env)) ?? clearFriendPresence(account, env);
  }

  // Everything below changes or searches the social graph: the write bucket.
  const blocked = await limited(account, "write", env);
  if (blocked) return blocked;

  if (method === "GET" && path === "/v1/friends/search") return searchAccounts(url, account, env);
  if (method === "POST" && path === "/v1/friends/request") return requestFriend(request, account, env);
  if (method === "POST" && path === "/v1/friends/respond") return respond(request, account, env);
  if (method === "PUT" && path === "/v1/friends/settings") return putSettings(request, account, env);
  const one = /^\/v1\/friends\/([A-Za-z0-9_-]{1,64})$/.exec(path);
  if (one && method === "DELETE") return removeFriend(one[1]!, account, env);

  return json(404, { error: "no such endpoint" });
}

export interface FriendPresence {
  serverName: string;
  address: string | null;
  track: string | null;
  riders: number | null;
  updatedAt: number;
}

async function listFriends(account: Account, env: Env): Promise<Response> {
  const profile = await profileFor(account, env);
  const fresh = Date.now() - FRIEND_PRESENCE_TTL_MS;

  const accepted = await env.DB.prepare(
    "SELECT a.id, a.rider_name, fp.server_name, fp.address, fp.track, fp.riders, fp.updated_at" +
      " FROM friendships f" +
      " JOIN accounts a ON a.id = CASE WHEN f.requester_id = ? THEN f.addressee_id ELSE f.requester_id END" +
      " LEFT JOIN friend_profiles pr ON pr.account_id = a.id" +
      " LEFT JOIN friend_presence fp ON fp.account_id = a.id AND fp.updated_at > ?" +
      "   AND COALESCE(pr.hide_presence, 0) = 0" +
      " WHERE f.status = 'accepted' AND (f.requester_id = ? OR f.addressee_id = ?)" +
      " AND a.kind <> 'erased'" +
      " ORDER BY a.rider_name COLLATE NOCASE LIMIT ?",
  )
    .bind(account.id, fresh, account.id, account.id, MAX_FRIENDS)
    .all<{
      id: string;
      rider_name: string;
      server_name: string | null;
      address: string | null;
      track: string | null;
      riders: number | null;
      updated_at: number | null;
    }>();

  const pending = await env.DB.prepare(
    "SELECT f.requester_id, f.addressee_id, f.status, a.id, a.rider_name FROM friendships f" +
      " JOIN accounts a ON a.id = CASE WHEN f.requester_id = ? THEN f.addressee_id ELSE f.requester_id END" +
      " WHERE f.status IN ('pending', 'declined') AND (f.requester_id = ? OR f.addressee_id = ?)" +
      " AND a.kind <> 'erased' ORDER BY f.created_at DESC LIMIT 100",
  )
    .bind(account.id, account.id, account.id)
    .all<{ requester_id: string; addressee_id: string; status: string; id: string; rider_name: string }>();

  const incoming: { accountId: string; riderName: string }[] = [];
  const outgoing: { accountId: string; riderName: string }[] = [];
  for (const r of pending.results) {
    const entry = { accountId: r.id, riderName: r.rider_name };
    if (r.addressee_id === account.id) {
      if (r.status === "pending") incoming.push(entry);
    } else {
      // A declined request reads as still pending to the person who sent it.
      outgoing.push(entry);
    }
  }

  return json(200, {
    friendCode: formatCode(profile.code),
    hidePresence: profile.hide,
    friends: accepted.results.map((r) => ({
      accountId: r.id,
      riderName: r.rider_name,
      presence:
        r.server_name !== null && r.updated_at !== null
          ? ({
              serverName: r.server_name,
              address: r.address,
              track: r.track,
              riders: r.riders,
              updatedAt: r.updated_at,
            } satisfies FriendPresence)
          : null,
    })),
    incoming,
    outgoing,
  });
}

async function searchAccounts(url: URL, account: Account, env: Env): Promise<Response> {
  const q = cleanText(url.searchParams.get("q"), 32);
  if (q.length < SEARCH_MIN) return json(400, { error: `type at least ${SEARCH_MIN} characters` });
  const pattern = `${q.replace(/[\\%_]/g, "\\$&")}%`;
  const rows = await env.DB.prepare(
    "SELECT id, rider_name FROM accounts WHERE rider_name LIKE ? ESCAPE '\\'" +
      " AND id <> ? AND kind <> 'erased' ORDER BY rider_name COLLATE NOCASE LIMIT ?",
  )
    .bind(pattern, account.id, SEARCH_LIMIT)
    .all<{ id: string; rider_name: string }>();

  const results = [];
  for (const r of rows.results) {
    const pair = await pairBetween(account.id, r.id, env);
    let relation = "none";
    if (pair?.status === "accepted") relation = "friends";
    else if (pair?.requester_id === account.id) relation = "pending_out";
    else if (pair?.status === "pending") relation = "pending_in";
    results.push({ accountId: r.id, riderName: r.rider_name, relation });
  }
  return json(200, { results });
}

async function requestFriend(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readBody(request);
  if (!body) return json(400, { error: "expected a JSON body" });

  let target: { id: string; rider_name: string } | null = null;
  if (typeof body.friendCode === "string") {
    const code = normalizeCode(body.friendCode);
    if (!code) return json(400, { error: "that isn't a friend code" });
    target = await env.DB.prepare(
      "SELECT a.id, a.rider_name FROM friend_profiles p JOIN accounts a ON a.id = p.account_id" +
        " WHERE p.friend_code = ? AND a.kind <> 'erased'",
    )
      .bind(code)
      .first<{ id: string; rider_name: string }>();
  } else if (typeof body.accountId === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(body.accountId)) {
    target = await env.DB.prepare("SELECT id, rider_name FROM accounts WHERE id = ? AND kind <> 'erased'")
      .bind(body.accountId)
      .first<{ id: string; rider_name: string }>();
  } else {
    return json(400, { error: "send an accountId or a friendCode" });
  }
  if (!target) return json(404, { error: "no rider found" });
  if (target.id === account.id) return json(400, { error: "that's you" });

  const pair = await pairBetween(account.id, target.id, env);
  if (pair?.status === "accepted") return json(200, { status: "friends", riderName: target.rider_name });
  if (pair && pair.requester_id === account.id) {
    // Already asked, or asked and was declined, which the asker is not told.
    return json(200, { status: "pending", riderName: target.rider_name });
  }

  const now = Date.now();
  if (pair?.status === "pending") {
    // They already asked us: asking back is accepting.
    if ((await friendCount(account.id, env)) >= MAX_FRIENDS) return json(409, { error: "your friend list is full" });
    await env.DB.prepare(
      "UPDATE friendships SET status = 'accepted', responded_at = ? WHERE requester_id = ? AND addressee_id = ?",
    )
      .bind(now, target.id, account.id)
      .run();
    return json(200, { status: "friends", riderName: target.rider_name });
  }
  if (pair) {
    // We declined them earlier and now ask them: a fresh request in our direction.
    await env.DB.prepare("DELETE FROM friendships WHERE requester_id = ? AND addressee_id = ?")
      .bind(target.id, account.id)
      .run();
  }

  const out = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM friendships WHERE requester_id = ? AND status = 'pending'",
  )
    .bind(account.id)
    .first<{ n: number }>();
  if ((out?.n ?? 0) >= MAX_OUTGOING) return json(429, { error: "too many pending requests" });
  if ((await friendCount(account.id, env)) >= MAX_FRIENDS) return json(409, { error: "your friend list is full" });

  await env.DB.prepare(
    "INSERT INTO friendships (requester_id, addressee_id, status, created_at) VALUES (?, ?, 'pending', ?)" +
      " ON CONFLICT DO NOTHING",
  )
    .bind(account.id, target.id, now)
    .run();
  return json(200, { status: "pending", riderName: target.rider_name });
}

async function friendCount(id: string, env: Env): Promise<number> {
  const row = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM friendships WHERE status = 'accepted' AND (requester_id = ? OR addressee_id = ?)",
  )
    .bind(id, id)
    .first<{ n: number }>();
  return row?.n ?? 0;
}

async function respond(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readBody(request);
  if (!body || typeof body.accountId !== "string" || typeof body.accept !== "boolean") {
    return json(400, { error: "expected { accountId, accept }" });
  }
  const pending = await env.DB.prepare(
    "SELECT status FROM friendships WHERE requester_id = ? AND addressee_id = ? AND status = 'pending'",
  )
    .bind(body.accountId, account.id)
    .first();
  if (!pending) return json(404, { error: "no such request" });

  if (body.accept) {
    if ((await friendCount(account.id, env)) >= MAX_FRIENDS) return json(409, { error: "your friend list is full" });
    await env.DB.prepare(
      "UPDATE friendships SET status = 'accepted', responded_at = ? WHERE requester_id = ? AND addressee_id = ?",
    )
      .bind(Date.now(), body.accountId, account.id)
      .run();
    return json(200, { status: "friends" });
  }
  await env.DB.prepare(
    "UPDATE friendships SET status = 'declined', responded_at = ? WHERE requester_id = ? AND addressee_id = ?",
  )
    .bind(Date.now(), body.accountId, account.id)
    .run();
  return json(200, { status: "declined" });
}

/** Unfriend, or cancel a request you sent. */
async function removeFriend(otherId: string, account: Account, env: Env): Promise<Response> {
  // A declined request the caller *sent* stays: deleting it would be the way round the decline.
  await env.DB.prepare(
    "DELETE FROM friendships WHERE ((requester_id = ? AND addressee_id = ?) OR (requester_id = ? AND addressee_id = ?))" +
      " AND NOT (status = 'declined' AND requester_id = ?)",
  )
    .bind(account.id, otherId, otherId, account.id, account.id)
    .run();
  return json(200, { ok: true });
}

async function putSettings(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readBody(request);
  if (!body || typeof body.hidePresence !== "boolean") return json(400, { error: "expected { hidePresence }" });
  await profileFor(account, env);
  await env.DB.prepare("UPDATE friend_profiles SET hide_presence = ? WHERE account_id = ?")
    .bind(body.hidePresence ? 1 : 0, account.id)
    .run();
  // Hiding takes effect at once: whatever was stored is gone before this answers.
  if (body.hidePresence) await clearFriendPresence(account, env);
  return json(200, { hidePresence: body.hidePresence });
}

async function putFriendPresence(request: Request, account: Account, env: Env): Promise<Response> {
  const body = await readBody(request);
  if (!body) return json(400, { error: "expected a JSON body" });

  const serverName = cleanText(body.serverName, 64);
  if (!serverName) return json(400, { error: "a server name is required" });
  let address: string | null = null;
  if (body.address !== undefined && body.address !== null && body.address !== "") {
    if (!isPublicGameAddress(body.address)) return json(400, { error: "that isn't a server address" });
    address = body.address.trim();
  }
  const track = cleanText(body.track, 64) || null;
  let riders: number | null = null;
  if (body.riders !== undefined && body.riders !== null) {
    if (typeof body.riders !== "number" || !Number.isInteger(body.riders) || body.riders < 0 || body.riders > 255) {
      return json(400, { error: "riders must be a small count" });
    }
    riders = body.riders;
  }

  const profile = await env.DB.prepare("SELECT hide_presence FROM friend_profiles WHERE account_id = ?")
    .bind(account.id)
    .first<{ hide_presence: number }>();
  if (profile?.hide_presence === 1) return json(200, { ok: true, shared: false });

  await env.DB.prepare(
    "INSERT INTO friend_presence (account_id, server_name, address, track, riders, updated_at)" +
      " VALUES (?, ?, ?, ?, ?, ?)" +
      " ON CONFLICT(account_id) DO UPDATE SET server_name = excluded.server_name," +
      " address = excluded.address, track = excluded.track, riders = excluded.riders," +
      " updated_at = excluded.updated_at",
  )
    .bind(account.id, serverName, address, track, riders, Date.now())
    .run();
  return json(200, { ok: true, shared: true });
}

async function clearFriendPresence(account: Account, env: Env): Promise<Response> {
  await env.DB.prepare("DELETE FROM friend_presence WHERE account_id = ?").bind(account.id).run();
  return json(200, { ok: true });
}

/** Sweep presence rows long past the TTL. On the cron. */
export async function pruneFriendPresence(env: Env): Promise<void> {
  try {
    await env.DB.prepare("DELETE FROM friend_presence WHERE updated_at < ?")
      .bind(Date.now() - 60 * 60 * 1000)
      .run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "friend presence sweep failed", error: String(err) }));
  }
}
