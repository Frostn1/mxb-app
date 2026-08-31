/**
 * Paid plugins: licences, redeemable keys, and the signed entitlement the app runs on.
 *
 * The shape of the problem is that the app has to keep working on a plane. So the plane is
 * not asked "may this person run the plugin" at the moment they run it — it is asked
 * periodically, and answers with a short-lived **entitlement**: a small signed statement the
 * app can check by itself, with no network and no shared secret.
 *
 * Signed with Ed25519, not an HMAC. An HMAC would need the same key at both ends, and the
 * app's end ships to the people paying for the plugin — one `strings` away from being able
 * to mint their own. With a signature the app only ever holds the public half, and forging
 * an entitlement means breaking the curve rather than reading a binary.
 *
 * Two clocks matter and they are deliberately different:
 *   * `expires`      — when the subscription runs out. Months away.
 *   * `refreshAfter` — when the app must have talked to us again. Days away.
 * A cancelled subscription therefore stops working within the grace window rather than at
 * the end of the month, without the app needing to be online to find out.
 */

/** How long an entitlement is honoured with no contact. */
export const GRACE_DAYS = 7;

/** Format version, so a future field can be added without old apps mis-reading a token. */
export const ENTITLEMENT_VERSION = 1;

export interface Entitlement {
  v: number;
  /** Account the entitlement was issued to. Bound so a token cannot be passed around. */
  account: string;
  plugin: string;
  /** Seconds since epoch. When the subscription ends. */
  expires: number;
  /** Seconds since epoch. When the app must re-check. Always <= expires. */
  refreshAfter: number;
  /** The bundle this entitlement is good for, so a swapped bundle fails to verify. */
  bundleSha256: string | null;
  issued: number;
}

// ---------------------------------------------------------------------------
// signing
// ---------------------------------------------------------------------------

function b64url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function unb64url(s: string): Uint8Array {
  const padded = s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4);
  const binary = atob(padded);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

/**
 * Import the signing key from its PKCS#8 DER, base64'd, in `PLUGIN_SIGNING_KEY`.
 *
 * Generate one with `scripts/plugin-keypair.ts`. Absent, every endpoint here answers 503
 * rather than issuing something unsigned — an unsigned entitlement is not a degraded
 * entitlement, it is a forged one that happens to be ours.
 */
async function signingKey(env: Env): Promise<CryptoKey | null> {
  if (!env.PLUGIN_SIGNING_KEY) return null;
  try {
    return await crypto.subtle.importKey(
      "pkcs8",
      unb64url(env.PLUGIN_SIGNING_KEY.replace(/\s+/g, "")),
      { name: "Ed25519" },
      false,
      ["sign"],
    );
  } catch {
    return null;
  }
}

/** `<b64url(json)>.<b64url(sig)>` — a JWS in spirit, without the header nobody reads. */
export async function signEntitlement(e: Entitlement, key: CryptoKey): Promise<string> {
  const payload = new TextEncoder().encode(JSON.stringify(e));
  const sig = await crypto.subtle.sign({ name: "Ed25519" }, key, payload);
  return `${b64url(payload)}.${b64url(new Uint8Array(sig))}`;
}

/** The app's side of the same check, kept here so the two can be tested against each other. */
export async function verifyEntitlement(
  token: string,
  publicKey: CryptoKey,
): Promise<Entitlement | null> {
  const dot = token.indexOf(".");
  if (dot < 0) return null;
  const payload = unb64url(token.slice(0, dot));
  const sig = unb64url(token.slice(dot + 1));
  const ok = await crypto.subtle.verify({ name: "Ed25519" }, publicKey, sig, payload);
  if (!ok) return null;
  try {
    return JSON.parse(new TextDecoder().decode(payload)) as Entitlement;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// licences
// ---------------------------------------------------------------------------

export interface Account {
  id: string;
}

interface LicenceRow {
  plugin_id: string;
  expires_at: number;
  bundle_sha256: string | null;
  version: string | null;
  name: string;
}

const DAY = 86400;

function nowSec(): number {
  return Math.floor(Date.now() / 1000);
}

/**
 * Add whole months to a timestamp, from whichever is later — now, or the current expiry.
 *
 * Renewing early has to add to what is left rather than restart from today, or the honest
 * thing (renewing before you lapse) costs you the remainder. Renewing *late* starts from
 * today, because the lapsed days were not paid for.
 */
export function extendBy(currentExpiry: number | null, months: number, now: number): number {
  const from = currentExpiry && currentExpiry > now ? currentExpiry : now;
  const d = new Date(from * 1000);
  d.setUTCMonth(d.getUTCMonth() + months);
  return Math.floor(d.getTime() / 1000);
}

async function issue(
  env: Env,
  key: CryptoKey,
  accountId: string,
  row: { plugin_id: string; expires_at: number; bundle_sha256: string | null },
): Promise<string> {
  const now = nowSec();
  return signEntitlement(
    {
      v: ENTITLEMENT_VERSION,
      account: accountId,
      plugin: row.plugin_id,
      expires: row.expires_at,
      // Never past the subscription itself: a licence with three days left must not hand out
      // a seven-day grace.
      refreshAfter: Math.min(now + GRACE_DAYS * DAY, row.expires_at),
      bundleSha256: row.bundle_sha256,
      issued: now,
    },
    key,
  );
}

/** The catalogue. Public: what is for sale is not a secret, and the app shows it to everyone. */
export async function listPlugins(env: Env): Promise<Response> {
  const { results } = await env.DB.prepare(
    `SELECT id, name, summary, version, bundle_sha256 FROM plugins ORDER BY name`,
  ).all<{
    id: string;
    name: string;
    summary: string | null;
    version: string | null;
    bundle_sha256: string | null;
  }>();
  return json(200, {
    plugins: (results ?? []).map((p) => ({
      id: p.id,
      name: p.name,
      summary: p.summary,
      version: p.version,
      // Published means "there is a build to install", which is not the same as "for sale".
      published: Boolean(p.bundle_sha256),
    })),
  });
}

/** What this account holds, each with a freshly signed entitlement. */
export async function myPlugins(account: Account, env: Env): Promise<Response> {
  const key = await signingKey(env);
  if (!key) return json(503, { error: "plugin licensing is not configured" });

  const { results } = await env.DB.prepare(
    `SELECT l.plugin_id, l.expires_at, p.bundle_sha256, p.version, p.name
       FROM plugin_licences l JOIN plugins p ON p.id = l.plugin_id
      WHERE l.account_id = ?`,
  )
    .bind(account.id)
    .all<LicenceRow>();

  const now = nowSec();
  const licences = [];
  for (const row of results ?? []) {
    licences.push({
      plugin: row.plugin_id,
      name: row.name,
      version: row.version,
      expires: row.expires_at,
      active: row.expires_at > now,
      // An expired licence gets no entitlement at all. Handing one out and letting the app
      // notice it is stale would make every consumer responsible for a check that belongs
      // in exactly one place.
      entitlement: row.expires_at > now ? await issue(env, key, account.id, row) : null,
    });
  }
  return json(200, { licences });
}

/** Trade a key for months on a licence. */
export async function redeemKey(
  request: Request,
  account: Account,
  env: Env,
): Promise<Response> {
  const key = await signingKey(env);
  if (!key) return json(503, { error: "plugin licensing is not configured" });

  let body: { code?: unknown };
  try {
    body = (await request.json()) as { code?: unknown };
  } catch {
    return json(400, { error: "expected a JSON body" });
  }
  const code = typeof body.code === "string" ? normaliseCode(body.code) : "";
  if (!code) return json(400, { error: "no key given" });

  const now = nowSec();
  // Claim the key first, and only if it is still unclaimed. Doing this as the WHERE of the
  // UPDATE is what makes two simultaneous redemptions of one code resolve to one winner:
  // the second matches no row and gets the "already used" answer, rather than both reading
  // "unredeemed" and both extending the licence.
  const claimed = await env.DB.prepare(
    `UPDATE plugin_keys SET redeemed_by = ?, redeemed_at = ?
      WHERE code = ? AND redeemed_by IS NULL
      RETURNING plugin_id, months`,
  )
    .bind(account.id, now, code)
    .first<{ plugin_id: string; months: number }>();

  if (!claimed) {
    // Tell the two failures apart: "already used" is a different problem from "not a key",
    // and someone renewing needs to know which one they are looking at.
    const exists = await env.DB.prepare(`SELECT redeemed_by FROM plugin_keys WHERE code = ?`)
      .bind(code)
      .first<{ redeemed_by: string | null }>();
    if (exists) return json(409, { error: "that key has already been used" });
    return json(404, { error: "that key isn't one of ours" });
  }

  const current = await env.DB.prepare(
    `SELECT expires_at FROM plugin_licences WHERE account_id = ? AND plugin_id = ?`,
  )
    .bind(account.id, claimed.plugin_id)
    .first<{ expires_at: number }>();

  const expires = extendBy(current?.expires_at ?? null, claimed.months, now);
  await env.DB.prepare(
    `INSERT INTO plugin_licences (account_id, plugin_id, expires_at, granted_at)
     VALUES (?, ?, ?, ?)
     ON CONFLICT (account_id, plugin_id) DO UPDATE SET expires_at = excluded.expires_at`,
  )
    .bind(account.id, claimed.plugin_id, expires, now)
    .run();

  const meta = await env.DB.prepare(
    `SELECT bundle_sha256, version, name FROM plugins WHERE id = ?`,
  )
    .bind(claimed.plugin_id)
    .first<{ bundle_sha256: string | null; version: string | null; name: string }>();

  return json(200, {
    plugin: claimed.plugin_id,
    name: meta?.name ?? claimed.plugin_id,
    version: meta?.version ?? null,
    expires,
    entitlement: await issue(env, key, account.id, {
      plugin_id: claimed.plugin_id,
      expires_at: expires,
      bundle_sha256: meta?.bundle_sha256 ?? null,
    }),
  });
}

/**
 * The bundle itself, for an account whose licence is live right now.
 *
 * Streamed from R2 rather than redirected to it: a redirect would be a URL that works for
 * whoever holds it, and the whole point of this route is that it does not.
 */
export async function pluginBundle(
  pluginId: string,
  account: Account,
  env: Env,
): Promise<Response> {
  const row = await env.DB.prepare(
    `SELECT p.bundle_key, p.bundle_sha256, l.expires_at
       FROM plugins p
       LEFT JOIN plugin_licences l ON l.plugin_id = p.id AND l.account_id = ?
      WHERE p.id = ?`,
  )
    .bind(account.id, pluginId)
    .first<{ bundle_key: string | null; bundle_sha256: string | null; expires_at: number | null }>();

  if (!row) return json(404, { error: "no such plugin" });
  if (!row.expires_at || row.expires_at <= nowSec()) {
    return json(403, { error: "no live licence for that plugin" });
  }
  if (!row.bundle_key) return json(404, { error: "that plugin has no build published yet" });

  const object = await env.PAINTS.get(row.bundle_key);
  if (!object) return json(404, { error: "the published build is missing" });
  return new Response(object.body, {
    headers: {
      "content-type": "application/octet-stream",
      // Private and uncached: this response is only correct for the licence that fetched it.
      "cache-control": "private, no-store",
      ...(row.bundle_sha256 ? { etag: row.bundle_sha256 } : {}),
    },
  });
}

/** Keys are shown to humans, so accept the shapes a human will type. */
export function normaliseCode(raw: string): string {
  return raw.trim().toUpperCase().replace(/\s+/g, "").replace(/[^A-Z0-9-]/g, "");
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
