/**
 * Buy Me a Coffee supporters get the supporter pack, matched by email.
 *
 * BMAC knows somebody by the address they paid with; the control plane knows them by Steam id.
 * The bridge is an address the player confirms once in the app: we mail a six-digit code to
 * it, they type it back, and from then on that address belongs to their Steam id. A coffee from
 * that address, before or after, grants the pack.
 *
 * No address is stored. Both sides keep a SHA-256 of the normalised address with a fixed
 * prefix, which is enough to match and not enough to mail anybody. The code is a digest too,
 * lasts 15 minutes and allows five tries.
 *
 * Sending needs a mail provider: `RESEND_API_KEY` and `SUPPORTER_MAIL_FROM`. Without them the
 * routes answer 503, as the BMAC webhook does without its secret, and the pack is granted by
 * hand from the admin page instead.
 */

import { hashToken } from "./auth";
import { steamIdFor } from "./steamlink";
import { grantSupporter, isSupporter } from "./supporters";

const CODE_TTL_MS = 15 * 60 * 1000;
const MAX_ATTEMPTS = 5;
const MAX_SENDS_PER_HOUR = 3;
const HOUR_MS = 60 * 60 * 1000;
/** RFC 5321's limit. */
const MAX_EMAIL = 254;

interface Account {
  id: string;
  steam_id: string | null;
}

/** Lower-cased and trimmed, or null if it doesn't look like an address. */
export function normaliseEmail(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const email = raw.trim().toLowerCase();
  if (email.length > MAX_EMAIL) return null;
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) ? email : null;
}

/** What both sides store instead of the address. */
export async function emailHash(email: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`frost-supporter-email:${email}`),
  );
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Six digits, from the CSPRNG. */
function newCode(): string {
  const n = crypto.getRandomValues(new Uint32Array(1))[0] % 1_000_000;
  return String(n).padStart(6, "0");
}

/** Sends one code and says whether it went. Swapped out in tests. */
export type Mailer = (to: string, code: string) => Promise<boolean>;

/** Resend's API, or null when this deployment has no mail provider. */
export function resendMailer(env: Env, fetchImpl: typeof fetch = fetch): Mailer | null {
  const key = env.RESEND_API_KEY;
  const from = env.SUPPORTER_MAIL_FROM;
  if (!key || !from) return null;
  return async (to, code) => {
    const res = await fetchImpl("https://api.resend.com/emails", {
      method: "POST",
      headers: { authorization: `Bearer ${key}`, "content-type": "application/json" },
      body: JSON.stringify({
        from,
        to: [to],
        subject: `Your Frost code: ${code}`,
        text: `Your code is ${code}. It works for 15 minutes.\n\nIf you didn't ask for this, ignore it.`,
      }),
    });
    if (!res.ok) console.error(JSON.stringify({ msg: "supporter mail failed", status: res.status }));
    return res.ok;
  };
}

/** Give the pack to every confirmed account behind this address that has paid on BMAC. */
async function grantMatches(env: Env, hash: string, now: number): Promise<void> {
  const paid = await env.DB.prepare("SELECT 1 AS yes FROM bmac_supporters WHERE email_hash = ?")
    .bind(hash)
    .first<{ yes: number }>();
  if (!paid) return;
  const rows = await env.DB.prepare("SELECT steam_id FROM supporter_emails WHERE email_hash = ?")
    .bind(hash)
    .all<{ steam_id: string }>();
  for (const row of rows.results ?? []) {
    await grantSupporter(env, row.steam_id, "bmac", hash, null, now);
  }
}

/** From the BMAC webhook: this address has paid. Grants any account that already confirmed it. */
export async function recordBmacPayer(env: Env, rawEmail: unknown, now = Date.now()): Promise<void> {
  const email = normaliseEmail(rawEmail);
  if (!email) return;
  const hash = await emailHash(email);
  await env.DB.prepare(
    "INSERT INTO bmac_supporters (email_hash, first_seen, last_seen) VALUES (?, ?, ?)" +
      " ON CONFLICT (email_hash) DO UPDATE SET last_seen = excluded.last_seen",
  )
    .bind(hash, now, now)
    .run();
  await grantMatches(env, hash, now);
}

export interface Answer {
  status: number;
  body: Record<string, unknown>;
}

async function readBody(request: Request): Promise<Record<string, unknown> | null> {
  try {
    const body = (await request.json()) as unknown;
    return body && typeof body === "object" ? (body as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

/** `POST /v1/supporter/email` with `{ email }`: mail a code to it. */
export async function sendEmailCode(
  request: Request,
  account: Account,
  env: Env,
  mailer: Mailer | null = resendMailer(env),
  now = Date.now(),
): Promise<Answer> {
  if (!mailer) return { status: 503, body: { error: "email confirmation isn't set up yet" } };
  const steamId = await steamIdFor(env, account);
  if (!steamId) return { status: 409, body: { error: "sign in with Steam first" } };

  const body = await readBody(request);
  if (!body) return { status: 400, body: { error: "expected a JSON body" } };
  const email = normaliseEmail(body.email);
  if (!email) return { status: 400, body: { error: "that doesn't look like an email address" } };

  const prev = await env.DB.prepare(
    "SELECT window_start, sends FROM supporter_email_codes WHERE account_id = ?",
  )
    .bind(account.id)
    .first<{ window_start: number; sends: number }>();
  const fresh = !prev || now - prev.window_start >= HOUR_MS;
  if (prev && !fresh && prev.sends >= MAX_SENDS_PER_HOUR) {
    return { status: 429, body: { error: "too many codes, try again in an hour" } };
  }

  const code = newCode();
  await env.DB.prepare(
    "INSERT INTO supporter_email_codes" +
      " (account_id, email_hash, code_hash, expires_at, attempts, window_start, sends)" +
      " VALUES (?, ?, ?, ?, 0, ?, ?)" +
      " ON CONFLICT (account_id) DO UPDATE SET email_hash = excluded.email_hash," +
      " code_hash = excluded.code_hash, expires_at = excluded.expires_at, attempts = 0," +
      " window_start = excluded.window_start, sends = excluded.sends",
  )
    .bind(
      account.id,
      await emailHash(email),
      await hashToken(code),
      now + CODE_TTL_MS,
      prev && !fresh ? prev.window_start : now,
      prev && !fresh ? prev.sends + 1 : 1,
    )
    .run();

  if (!(await mailer(email, code))) return { status: 502, body: { error: "the email didn't send" } };
  return { status: 200, body: { ok: true } };
}

/** `POST /v1/supporter/email/confirm` with `{ code }`: confirm, and grant if the address has paid. */
export async function confirmEmailCode(
  request: Request,
  account: Account,
  env: Env,
  now = Date.now(),
): Promise<Answer> {
  const steamId = await steamIdFor(env, account);
  if (!steamId) return { status: 409, body: { error: "sign in with Steam first" } };

  const body = await readBody(request);
  if (!body) return { status: 400, body: { error: "expected a JSON body" } };
  const code = typeof body.code === "string" ? body.code.trim() : "";

  const row = await env.DB.prepare(
    "SELECT email_hash, code_hash, expires_at, attempts FROM supporter_email_codes WHERE account_id = ?",
  )
    .bind(account.id)
    .first<{ email_hash: string; code_hash: string; expires_at: number; attempts: number }>();
  if (!row || now > row.expires_at || row.attempts >= MAX_ATTEMPTS) {
    return { status: 410, body: { error: "that code has expired, ask for a new one" } };
  }
  if (!/^\d{6}$/.test(code) || (await hashToken(code)) !== row.code_hash) {
    await env.DB.prepare("UPDATE supporter_email_codes SET attempts = attempts + 1 WHERE account_id = ?")
      .bind(account.id)
      .run();
    return { status: 400, body: { error: "wrong code" } };
  }

  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO supporter_emails (account_id, steam_id, email_hash, confirmed_at) VALUES (?, ?, ?, ?)" +
        " ON CONFLICT (account_id) DO UPDATE SET steam_id = excluded.steam_id," +
        " email_hash = excluded.email_hash, confirmed_at = excluded.confirmed_at",
    ).bind(account.id, steamId, row.email_hash, now),
    env.DB.prepare("DELETE FROM supporter_email_codes WHERE account_id = ?").bind(account.id),
  ]);
  await grantMatches(env, row.email_hash, now);
  return { status: 200, body: { ok: true, supporter: await isSupporter(env, steamId) } };
}
