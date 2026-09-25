/**
 * `POST /v1/web/lock/permit` — may this creator GUID-lock a file to these GUIDs?
 *
 * The GUID lock runs in the creator's browser (`/v1/web/lockweb/*` hands them the locker), so
 * this host never sees the file or the lock. What it can see is the question the site asks just
 * before it locks: here are the GUIDs. Two answers come back as refusals that did not used to:
 *
 * - **a banned GUID.** A creator locking a file to a rider banned for unlocking other people's
 *   content is handing that rider content, and the rider is banned from all of it.
 * - **too many GUIDs this hour** (`LOCK_GUIDS_PER_HOUR`). Otherwise the first refusal is an
 *   oracle: a list of GUIDs in, "which of these are banned" out, as fast as it can be asked.
 *
 * Both are refused with the same sentence, with the same status, as every other refusal here —
 * `LOCK_REFUSED`, the words the lock page already shows when a lock fails. Never "banned":
 * the creator is not the banned party, the rider is, and telling a creator which GUIDs are
 * banned is telling whoever they pass it on to. The real reason goes to the Worker's log, one
 * JSON line per refusal (`msg: "lock permit refused"`), with the creator's account and Steam
 * id and the GUIDs asked for, so probing shows up as a run of them.
 *
 * ## What this does not do
 *
 * The locker is WebAssembly running on the creator's own machine. A creator who edits the page
 * can skip this call and lock to anything; nothing here can stop that, and a signed permit the
 * locker checked would be checked by the same code they can edit. This is the gate for everybody
 * who uses the site as it is, and a log line for the ones who go around it on the way. Nor can it
 * reach a file already locked: a GUID lock is a property of the bytes, and those are out there.
 */

import { refuseCrossSiteWrite } from "./assets";
import { banFor, normalizeGuid } from "./bans";
import { repairBySteamId } from "./steamlink";
import { webSession } from "./websession";

/** Every refusal, whatever the reason: the lock page's own words for a lock that failed. */
export const LOCK_REFUSED = "couldn't lock this file";

/**
 * GUIDs a creator may ask to lock to in a rolling hour. One file for every rider on a full
 * grid twice over, which is more than any real batch; a scan of the ban list it is not.
 */
export const LOCK_GUIDS_PER_HOUR = 100;

/** Most GUIDs one permit may name — the lock page makes one file per GUID. */
export const MAX_PERMIT_GUIDS = 50;

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

/** Why a permit was refused, as the log line says it. Never sent to the caller. */
type Reason = "caller_banned" | "not_creator" | "bad_request" | "rate_limited" | "banned_target";

export async function lockPermit(request: Request, env: Env): Promise<Response> {
  const offSite = refuseCrossSiteWrite(request, env);
  if (offSite) return offSite;

  const session = await webSession(request, env);
  if (!session) return refused(401);

  const find = () =>
    env.DB.prepare("SELECT id, creator_at FROM accounts WHERE steam_id = ?")
      .bind(session.steamId)
      .first<{ id: string; creator_at: number | null }>();
  const account = (await find()) ?? ((await repairBySteamId(env, session.steamId)) ? await find() : null);
  const who = { account: account?.id ?? null, steamId: session.steamId };

  // The same standing the locker itself is handed out on (`web.ts`'s `lockweb`).
  if (await banFor(env, { accountId: account?.id, steamId: session.steamId })) {
    return refuse(403, "caller_banned", who);
  }
  if (!account?.creator_at) return refuse(403, "not_creator", who);

  const guids = await readGuids(request);
  if (!guids) return refuse(400, "bad_request", who);

  // Counted before the ban list is asked, and whatever it answers, so a refusal costs as much
  // of the hour as a lock does. Check and record are one statement, so two tabs asking at once
  // cannot both squeeze under the line. The owner account has no ceiling, as with assets.
  if (account.id !== env.MXB_OWNER_ACCOUNT_ID) {
    const now = Date.now();
    const counted = await env.DB.prepare(
      "INSERT INTO lock_attempts (account_id, attempted_at, guids)" +
        " SELECT ?1, ?2, ?3 WHERE" +
        " (SELECT COALESCE(SUM(guids), 0) FROM lock_attempts WHERE account_id = ?1 AND attempted_at > ?4) + ?3 <= ?5" +
        " RETURNING guids",
    )
      .bind(account.id, now, guids.length, now - HOUR_MS, LOCK_GUIDS_PER_HOUR)
      .first<{ guids: number }>();
    if (!counted) return refuse(403, "rate_limited", { ...who, guids });
  }

  // The target alone, resolved the normal way: the question is whether that rider is banned,
  // not whether anybody the creator knows is.
  const banned: string[] = [];
  for (const guid of guids) {
    if (await banFor(env, { guid })) banned.push(guid);
  }
  if (banned.length) return refuse(403, "banned_target", { ...who, guids, banned });

  return noStore(json(200, { ok: true }));
}

/** `{guids: string[]}`, normalised and de-duplicated, or null if it isn't a usable list. */
async function readGuids(request: Request): Promise<string[] | null> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return null;
  }
  const raw = (body as { guids?: unknown } | null)?.guids;
  if (!Array.isArray(raw) || raw.length === 0 || raw.length > MAX_PERMIT_GUIDS) return null;
  const out = new Set<string>();
  for (const item of raw) {
    // The shape the locker itself insists on: an MX Bikes GUID is 18 hex digits.
    const guid = normalizeGuid(item);
    if (!guid || !/^[0-9A-F]{18}$/.test(guid)) return null;
    out.add(guid);
  }
  return [...out];
}

function refuse(
  status: number,
  reason: Reason,
  detail: { account: string | null; steamId: string; guids?: string[]; banned?: string[] },
): Response {
  console.log(JSON.stringify({ msg: "lock permit refused", reason, ...detail }));
  return refused(status);
}

function refused(status: number): Response {
  return noStore(json(status, { error: LOCK_REFUSED }));
}

/** Forget attempts nothing reads any more. On the cron sweep; swallows its own failure. */
export async function pruneLockAttempts(env: Env, now = Date.now()): Promise<void> {
  try {
    await env.DB.prepare("DELETE FROM lock_attempts WHERE attempted_at < ?").bind(now - DAY_MS).run();
  } catch (err) {
    console.error(JSON.stringify({ msg: "lock attempt sweep failed", error: String(err) }));
  }
}

function noStore(response: Response): Response {
  response.headers.set("Cache-Control", "no-store");
  return response;
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}
