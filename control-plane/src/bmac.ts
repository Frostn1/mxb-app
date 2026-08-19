/**
 * Buy Me a Coffee → Discord.
 *
 * BMAC posts an event here when somebody supports the project; we turn it into an embed in
 * the announcements channel. The route is unauthenticated because BMAC holds no account —
 * the HMAC signature over the raw body is the whole of its credential.
 */

import { tokenMatches } from "./auth";

/** Bigger than any real event, small enough that a junk body can't be used to burn CPU. */
const MAX_BODY_BYTES = 64 * 1024;

/** How much of a supporter's note is forwarded. The rest is on the BMAC dashboard. */
const MAX_NOTE_CHARS = 400;

/** BMAC's yellow, so the embed reads as coming from them. */
const EMBED_COLOUR = 0xffdd00;

/**
 * The events worth announcing, and what each one says.
 *
 * Money-in only. Refunds, cancellations and pauses are accepted and dropped — a public
 * channel is the wrong place to narrate somebody's withdrawal — and anything not listed
 * here is dropped too, so a new BMAC event type can never surprise the channel.
 */
const ANNOUNCED: Record<string, string> = {
  "donation.created": "bought you a coffee",
  "membership.started": "started a membership",
  "recurring_donation.started": "started a recurring donation",
  "extra_purchase.created": "bought an extra",
  "commission_order.created": "ordered a commission",
  "wishlist_payment.created": "granted a wishlist item",
};

// BMAC's per-event payloads aren't fully published, and the fields differ between a coffee
// and a membership. Every read is a list of candidates with a fallback: a missing field must
// degrade to a vaguer sentence, never to a 500 — that would only earn four retries of the
// same failure.
const NAME_KEYS = ["supporter_name", "payer_name", "buyer_name", "name"];
const NOTE_KEYS = ["support_note", "supporter_message", "message", "note"];
const TIER_KEYS = ["membership_level_name", "tier_name", "level_name", "plan_name"];

interface BmacEvent {
  event_id?: unknown;
  type?: unknown;
  live_mode?: unknown;
  created?: unknown;
  data?: unknown;
}

export async function bmacWebhook(request: Request, env: Env): Promise<Response> {
  const secret = env.BMAC_WEBHOOK_SECRET;
  const channel = env.DISCORD_DONATION_WEBHOOK_URL;
  // 503 rather than 404, matching how provisioning answers without its AWS key: the route
  // exists, this deployment just isn't wired for it.
  if (!secret || !channel) {
    return json(503, { error: "donation announcements aren't configured on this deployment" });
  }

  // The signature covers the raw bytes, so nothing may parse the body before it is checked.
  const raw = await request.text();
  if (raw.length > MAX_BODY_BYTES) return json(413, { error: "that body is too large" });
  if (!(await signatureMatches(raw, secret, request.headers.get("x-signature-sha256")))) {
    return json(401, { error: "bad signature" });
  }

  let event: BmacEvent;
  try {
    event = JSON.parse(raw) as BmacEvent;
  } catch {
    return json(400, { error: "expected a JSON body" });
  }

  const payload = announcement(event);
  if (!payload) return json(200, { ok: true, announced: false });

  // BMAC retries a delivery up to four more times, and a response we sent but it never
  // received looks exactly like a failure from its side. Claiming the id first is what keeps
  // one coffee from being announced five times.
  const id = eventKey(event);
  if (id && !(await claim(env, id, String(event.type)))) {
    return json(200, { ok: true, announced: false, duplicate: true });
  }

  const posted = await fetch(channel, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!posted.ok) {
    // Hand the id back so the retry can have another go, rather than swallowing the
    // announcement because of a blip at Discord's end.
    if (id) await release(env, id);
    console.error(JSON.stringify({ msg: "bmac announce failed", status: posted.status }));
    return json(502, { error: "the announcement was not accepted" });
  }
  return json(200, { ok: true, announced: true });
}

/** HMAC-SHA256 of the raw body, hex, against `x-signature-sha256`. */
export async function signatureMatches(
  raw: string,
  secret: string,
  header: string | null,
): Promise<boolean> {
  if (!header) return false;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(raw));
  const expected = [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
  // Some senders prefix the algorithm; the digest is the part that matters.
  const presented = header.trim().toLowerCase().replace(/^sha256=/, "");
  return tokenMatches(expected, presented);
}

/**
 * The Discord payload for an event, or null if it isn't one we announce.
 *
 * Name and note only. No amount, no coffee count, and `supporter_email` is never so much as
 * read — this goes to a public channel, and what somebody paid is between them and BMAC.
 */
export function announcement(event: BmacEvent): unknown | null {
  const type = typeof event.type === "string" ? event.type : "";
  const verb = ANNOUNCED[type];
  if (!verb) return null;

  const data = (event.data && typeof event.data === "object" ? event.data : {}) as Record<
    string,
    unknown
  >;
  const name = escapeMarkdown(pick(data, NAME_KEYS) ?? "Someone");
  const tier = type.startsWith("membership.") ? pick(data, TIER_KEYS) : undefined;
  const note = pick(data, NOTE_KEYS);

  const lines = [`**${name}** ${verb}${tier ? ` — *${escapeMarkdown(tier)}*` : ""}.`];
  // A test delivery from the BMAC dashboard must never be mistaken for a real supporter.
  if (event.live_mode === false) lines.unshift("*(test event)*");

  const created = typeof event.created === "number" ? event.created : null;
  return {
    // Belt to the escaping's braces: a note is somebody else's text, and it is not going to
    // ping the server.
    allowed_mentions: { parse: [] },
    embeds: [
      {
        title: "☕ New supporter",
        description: lines.join("\n"),
        color: EMBED_COLOUR,
        fields: note ? [{ name: "Their message", value: escapeMarkdown(clip(note)) }] : undefined,
        timestamp: created ? new Date(created * 1000).toISOString() : undefined,
      },
    ],
  };
}

/** First non-empty string among `keys`. */
function pick(data: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = data[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return undefined;
}

function clip(text: string): string {
  return text.length > MAX_NOTE_CHARS ? `${text.slice(0, MAX_NOTE_CHARS - 1)}…` : text;
}

/** Someone else's words shouldn't be able to restyle the embed. */
function escapeMarkdown(text: string): string {
  return text.replace(/([\\*_~`|>#-])/g, "\\$1");
}

/** The event's own id, as a string. Absent or unusable means no dedupe rather than no post. */
function eventKey(event: BmacEvent): string | null {
  const id = event.event_id;
  if (typeof id === "number" && Number.isFinite(id)) return String(id);
  if (typeof id === "string" && id.trim()) return id.trim().slice(0, 64);
  return null;
}

/** True if this id is ours to announce; false if it has already been seen. */
async function claim(env: Env, id: string, type: string): Promise<boolean> {
  const row = await env.DB.prepare(
    "INSERT INTO bmac_events (event_id, type, seen_at) VALUES (?, ?, ?)" +
      " ON CONFLICT(event_id) DO NOTHING RETURNING event_id",
  )
    .bind(id, type, Date.now())
    .first<{ event_id: string }>();
  return row !== null;
}

async function release(env: Env, id: string): Promise<void> {
  await env.DB.prepare("DELETE FROM bmac_events WHERE event_id = ?").bind(id).run();
}

// index.ts has its own copy; duplicating four lines beats importing the entry point back
// into a module it imports.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
