/**
 * Voice rooms — one per game server, created by whoever arrives first.
 *
 * A room is a Durable Object addressed by the server key the app already computes for paint
 * sync (`server_key_for`). That is the whole of "is there a room for this server?": naming a
 * Durable Object *is* creating it if it doesn't exist, and it evicts itself once the last
 * rider disconnects. No table of rooms, no reaper, no lifecycle to get wrong.
 *
 * The room never carries audio. It carries the few kilobytes of ICE negotiation two apps
 * need to find each other, and then gets out of the way — the voice itself goes peer to
 * peer, which is what makes this free to run on servers we don't own and can't bill for.
 *
 * ## What the room does and does not vouch for
 *
 * The Worker admits a socket only if the bearer token resolves to an account *and* that
 * account's presence row says it is on this server. Presence is self-reported, so this
 * proves an account, not a rider: on a community server nothing can prove someone is really
 * on that grid.
 *
 * The claim that matters is therefore checked by the receiving app, not here — it plays a
 * peer only if that peer's race number appears in its own copy of the game's race-entry
 * list. So `riderName` and `raceNum` below are the peer's own claims, relayed verbatim and
 * believed by nobody. Anything that must be trusted is derived from the token instead.
 */

import { hashToken, newToken } from "./auth";
import { isRiderName, isServerKey, PRESENCE_TTL_MS } from "./validate";

/** A full grid is around forty. The cap is a backstop against a room being used as a chat. */
export const MAX_PEERS = 64;

/** An SDP offer runs to a couple of KB; a candidate is a line. Anything larger is not ICE. */
export const MAX_MESSAGE_BYTES = 16 * 1024;

/** Signalling is bursty at join and near-silent after. Enough for a renegotiation storm. */
export const SIGNAL_BURST = 240;
export const SIGNAL_WINDOW_MS = 10_000;

/** Who a socket belongs to. Stored on the socket so it survives hibernation. */
export interface PeerIdentity {
  peerId: string;
  /** From the bearer token. The only field here the room itself vouches for. */
  accountId: string;
  /** The rider name the client claims in-game. Checked by receivers, not by us. */
  riderName: string;
  /** The game's race number for that rider, or 0 before they have one. */
  raceNum: number;
  /** False until `hello` lands — a socket that hasn't introduced itself isn't a peer yet. */
  ready: boolean;
  /** Signal-rate window, kept per socket so one noisy peer can't drown the room. */
  signalCount: number;
  signalSince: number;
}

/** A peer as everyone else sees it. */
export interface PeerView {
  peerId: string;
  riderName: string;
  raceNum: number;
}

export type ClientMessage =
  | { t: "hello"; riderName: string; raceNum: number }
  | { t: "rider"; riderName: string; raceNum: number }
  | { t: "signal"; to: string; kind: "offer" | "answer" | "candidate"; data: string }
  | { t: "bye" };

const SIGNAL_KINDS = new Set(["offer", "answer", "candidate"]);

/**
 * Parse and validate one client frame.
 *
 * Returns a string on rejection rather than throwing: every failure here is someone else's
 * bad input, and the room's answer to bad input is an error frame, never an exception.
 */
export function parseClientMessage(raw: unknown): ClientMessage | string {
  if (typeof raw !== "string") return "expected text frames";
  if (raw.length > MAX_MESSAGE_BYTES) return "message too large";

  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return "not JSON";
  }
  if (!body || typeof body !== "object") return "not an object";
  const msg = body as Record<string, unknown>;

  switch (msg.t) {
    case "hello":
    case "rider": {
      const riderName = typeof msg.riderName === "string" ? msg.riderName.trim() : "";
      // A blank name is allowed: the app connects as soon as it knows the server, which can
      // be before the game has told it who is on the grid.
      if (riderName.length > 64) return "riderName too long";
      if (/[\u0000-\u001f\u007f]/.test(riderName)) return "riderName has control characters";
      const raceNum = typeof msg.raceNum === "number" ? Math.trunc(msg.raceNum) : 0;
      if (!Number.isFinite(raceNum) || raceNum < 0 || raceNum > 9999) return "raceNum out of range";
      return { t: msg.t, riderName, raceNum };
    }
    case "signal": {
      if (typeof msg.to !== "string" || msg.to.length === 0 || msg.to.length > 64) {
        return "signal needs a peer to send to";
      }
      if (typeof msg.kind !== "string" || !SIGNAL_KINDS.has(msg.kind)) return "unknown signal kind";
      if (typeof msg.data !== "string" || msg.data.length === 0) return "signal needs data";
      return { t: "signal", to: msg.to, kind: msg.kind as "offer" | "answer" | "candidate", data: msg.data };
    }
    case "bye":
      return { t: "bye" };
    default:
      return "unknown message type";
  }
}

/** The peer list a newcomer is handed. It offers to each of these; they wait for the offer. */
export function peerViews(identities: PeerIdentity[], exclude: string): PeerView[] {
  return identities
    .filter((p) => p.ready && p.peerId !== exclude)
    .map(({ peerId, riderName, raceNum }) => ({ peerId, riderName, raceNum }));
}

/**
 * Has this socket exceeded its signalling budget?
 *
 * Mutates the window in place — the caller writes the identity back to the socket, and
 * doing it here keeps the "count, then decide" in one piece.
 */
export function overSignalBudget(identity: PeerIdentity, now: number): boolean {
  if (now - identity.signalSince > SIGNAL_WINDOW_MS) {
    identity.signalSince = now;
    identity.signalCount = 0;
  }
  identity.signalCount += 1;
  return identity.signalCount > SIGNAL_BURST;
}

// ---------------------------------------------------------------------------------------
// The HTTP half: signing up for voice, and getting into a room.
// ---------------------------------------------------------------------------------------

/** All the room's endpoints need of an account. Kept narrow so this module owns no schema. */
export interface VoiceAccount {
  id: string;
}

/** How many accounts one address may mint in a day before it has to slow down. */
export const MAX_DEVICE_CLAIMS_PER_DAY = 5;

/**
 * Sign up with no invite code, for voice.
 *
 * The account this mints is deliberately weak: it proves the same caller across sessions and
 * nothing else. It cannot publish, cannot register a server, and is not in anybody's paint
 * roster. What it can do is report presence and join the voice room for the server it is on
 * — which is the whole of what "anyone with the app can talk" requires.
 *
 * There is no proof of personhood here and no attempt at one. The defence that matters is
 * downstream: a peer is only audible to riders whose own game says that race number is on
 * the grid, so an account with nothing behind it is an account nobody can hear.
 */
export async function claimDeviceAccount(request: Request, env: Env): Promise<Response> {
  const body = await readJson(request);
  if (!body) return json(400, { error: "expected a JSON body" });
  const { riderName } = body as { riderName?: unknown };
  if (!isRiderName(riderName)) {
    return json(400, { error: "riderName must match your in-game rider name" });
  }

  const now = Date.now();
  const day = new Date(now).toISOString().slice(0, 10);
  const digest = await ipDigest(request.headers.get("CF-Connecting-IP"), day, env);

  const seen = await env.DB.prepare(
    "SELECT claims FROM device_claims WHERE ip_digest = ? AND day = ? AND kind = 'signup'",
  )
    .bind(digest, day)
    .first<{ claims: number }>();
  if (seen && seen.claims >= MAX_DEVICE_CLAIMS_PER_DAY) {
    return json(429, { error: "too many accounts from here today" });
  }

  const id = crypto.randomUUID();
  const token = newToken();
  const hash = await hashToken(token);

  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO accounts (id, rider_name, token_hash, created_at, kind)" +
        " VALUES (?, ?, ?, ?, 'device')",
    ).bind(id, (riderName as string).trim(), hash, now),
    env.DB.prepare(
      "INSERT INTO device_claims (ip_digest, day, kind, claims, updated_at) VALUES (?, ?, 'signup', 1, ?)" +
        " ON CONFLICT(ip_digest, day, kind) DO UPDATE SET claims = claims + 1, updated_at = excluded.updated_at",
    ).bind(digest, day, now),
  ]);

  return json(201, { accountId: id, token, riderName: (riderName as string).trim(), kind: "device" });
}

/**
 * A one-day identifier for a caller's address.
 *
 * The address itself is never stored: it is personal data we have no use for beyond counting
 * today's signups. Keyed with a secret where one is configured, because a bare hash of an
 * IPv4 address is reversible by anyone willing to hash four billion strings.
 */
export async function ipDigest(ip: string | null, day: string, env: Env): Promise<string> {
  const material = `${day}:${ip ?? "unknown"}`;
  const secret = env.IP_HASH_SECRET;
  if (secret) {
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(secret),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(material));
    return hex(new Uint8Array(mac));
  }
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(material))));
}

function hex(bytes: Uint8Array): string {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Where to look for a route to another rider.
 *
 * STUN only, for now. It tells each app what its own address looks like from outside, which
 * is what lets two of them meet directly — and costs nothing, because no media ever passes
 * through it. The pairs STUN cannot join (symmetric NAT at both ends) need a TURN relay,
 * which is a per-gigabyte bill; the shape of this response is what will carry those
 * credentials when the measured failure rate says it's worth paying for.
 */
export function iceServers(): Response {
  return json(200, {
    iceServers: [
      { urls: ["stun:stun.cloudflare.com:3478"] },
      { urls: ["stun:stun.l.google.com:19302"] },
    ],
  });
}

/**
 * Join the voice room for a server, creating it if this rider is the first one there.
 *
 * Naming the Durable Object is what creates it, so "does a room exist for this server?" is
 * never asked — the answer is always the same object, and it disappears on its own when the
 * last rider leaves.
 *
 * Admission is the presence row the app already writes for paint sync. That proves an
 * account said it was on this server, which is as much as anyone can prove about a server we
 * do not run; what stops an impostor being *heard* is the receiving app checking the race
 * number against its own copy of the grid.
 */
export async function voiceRoom(request: Request, url: URL, account: VoiceAccount, env: Env): Promise<Response> {
  if (request.headers.get("Upgrade") !== "websocket") {
    return json(426, { error: "expected a websocket" });
  }
  const server = url.searchParams.get("server");
  if (!isServerKey(server)) return json(400, { error: "that isn't a server" });
  const key = (server as string).trim();

  const here = await env.DB.prepare(
    "SELECT updated_at FROM presence WHERE account_id = ? AND server_id = ?",
  )
    .bind(account.id, key)
    .first<{ updated_at: number }>();
  if (!here || Date.now() - here.updated_at > PRESENCE_TTL_MS) {
    return json(403, { error: "report presence on that server first" });
  }

  const room = env.VOICE_ROOMS.get(env.VOICE_ROOMS.idFromName(key));
  // A fresh header set rather than the caller's: `X-Account-Id` is the room's only proof of
  // who this is, so it must not be something the caller could have sent themselves.
  return room.fetch("https://voice.room/", {
    headers: { Upgrade: "websocket", "X-Account-Id": account.id },
  });
}

// index.ts has its own copy; duplicating four lines beats importing the entry point back
// into a module it imports.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

async function readJson(request: Request): Promise<unknown | null> {
  try {
    return await request.json();
  } catch {
    return null;
  }
}
