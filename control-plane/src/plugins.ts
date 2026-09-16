/**
 * Paid plugins: licenses, redeemable keys, and the signed license the app runs on.
 *
 * The shape of the problem is that the app has to keep working on a plane. So the plane is
 * not asked "may this person run the plugin" at the moment they run it — it is asked
 * periodically, and answers with a short-lived **license**: a small signed statement the
 * app can check by itself, with no network and no shared secret.
 *
 * Signed with Ed25519, not an HMAC. An HMAC would need the same key at both ends, and the
 * app's end ships to the people paying for the plugin — one `strings` away from being able
 * to mint their own. With a signature the app only ever holds the public half, and forging
 * an license means breaking the curve rather than reading a binary.
 *
 * Two clocks matter and they are deliberately different:
 *   * `expires`      — when the subscription runs out. Months away.
 *   * `refreshAfter` — when the app must have talked to us again. Days away.
 * A cancelled subscription therefore stops working within the grace window rather than at
 * the end of the month, without the app needing to be online to find out.
 */

import { likeTerm, MAX_COUNT, PAGE_SIZE, type Paged } from "./adminui";

/** How long an license is honoured with no contact. */
export const GRACE_DAYS = 7;

/** Format version, so a future field can be added without old apps mis-reading a token. */
export const LICENSE_VERSION = 1;

export interface License {
  v: number;
  /** Account the license was issued to. Bound so a token cannot be passed around. */
  account: string;
  plugin: string;
  /** Seconds since epoch. When the subscription ends. */
  expires: number;
  /** Seconds since epoch. When the app must re-check. Always <= expires. */
  refreshAfter: number;
  /** The bundle this license is good for, so a swapped bundle fails to verify. */
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
 * rather than issuing something unsigned — an unsigned license is not a degraded
 * license, it is a forged one that happens to be ours.
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
export async function signLicense(e: License, key: CryptoKey): Promise<string> {
  const payload = new TextEncoder().encode(JSON.stringify(e));
  const sig = await crypto.subtle.sign({ name: "Ed25519" }, key, payload);
  return `${b64url(payload)}.${b64url(new Uint8Array(sig))}`;
}

/** The app's side of the same check, kept here so the two can be tested against each other. */
export async function verifyLicense(
  token: string,
  publicKey: CryptoKey,
): Promise<License | null> {
  const dot = token.indexOf(".");
  if (dot < 0) return null;
  const payload = unb64url(token.slice(0, dot));
  const sig = unb64url(token.slice(dot + 1));
  const ok = await crypto.subtle.verify({ name: "Ed25519" }, publicKey, sig, payload);
  if (!ok) return null;
  try {
    return JSON.parse(new TextDecoder().decode(payload)) as License;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// licenses
// ---------------------------------------------------------------------------

export interface Account {
  id: string;
}

interface LicenseRow {
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
  return signLicense(
    {
      v: LICENSE_VERSION,
      account: accountId,
      plugin: row.plugin_id,
      expires: row.expires_at,
      // Never past the subscription itself: a license with three days left must not hand out
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

/** What this account holds, each with a freshly signed license. */
export async function myPlugins(account: Account, env: Env): Promise<Response> {
  const key = await signingKey(env);
  if (!key) return json(503, { error: "plugin licensing is not configured" });

  const { results } = await env.DB.prepare(
    // A revoked license is not a lapsed one, and is not reported as one: the row is left
    // out entirely, so the app shows the plugin exactly as it does to someone who never
    // held it. Saying "you have this, but no" would only invite a support conversation.
    `SELECT l.plugin_id, l.expires_at, p.bundle_sha256, p.version, p.name
       FROM plugin_licenses l JOIN plugins p ON p.id = l.plugin_id
      WHERE l.account_id = ? AND l.revoked_at IS NULL`,
  )
    .bind(account.id)
    .all<LicenseRow>();

  const now = nowSec();
  const licenses = [];
  for (const row of results ?? []) {
    licenses.push({
      plugin: row.plugin_id,
      name: row.name,
      version: row.version,
      expires: row.expires_at,
      active: row.expires_at > now,
      // An expired license gets no license at all. Handing one out and letting the app
      // notice it is stale would make every consumer responsible for a check that belongs
      // in exactly one place.
      license: row.expires_at > now ? await issue(env, key, account.id, row) : null,
    });
  }
  return json(200, { licenses });
}

/**
 * Why a key cannot be spent, or null when it can.
 *
 * Three different problems wear the same "it didn't work" in the app, and the person
 * holding the code needs to know which: a typo, a code already spent, and a code we
 * withdrew are three different next steps.
 */
function refusal(row: { redeemed_by: string | null; revoked_at: number | null }): Response | null {
  if (row.revoked_at) return json(403, { error: "that key has been revoked" });
  if (row.redeemed_by) return json(409, { error: "that key has already been used" });
  return null;
}

/** Trade a key for months on a license. */
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

  // Read the key before claiming it, for one reason: months must never land on a revoked
  // license, and which license that is only becomes knowable here. Claiming first and
  // discovering the revocation after would burn the code for nothing.
  const keyRow = await env.DB.prepare(
    `SELECT plugin_id, redeemed_by, revoked_at FROM plugin_keys WHERE code = ?`,
  )
    .bind(code)
    .first<{ plugin_id: string; redeemed_by: string | null; revoked_at: number | null }>();
  if (!keyRow) return json(404, { error: "that key isn't one of ours" });
  const refused = refusal(keyRow);
  if (refused) return refused;

  const license = (accountId: string, pluginId: string) =>
    env.DB.prepare(
      `SELECT expires_at, revoked_at FROM plugin_licenses WHERE account_id = ? AND plugin_id = ?`,
    )
      .bind(accountId, pluginId)
      .first<{ expires_at: number; revoked_at: number | null }>();

  if ((await license(account.id, keyRow.plugin_id))?.revoked_at) {
    return json(403, { error: "this account's license for that plugin has been revoked" });
  }

  // Now claim it, and only if it is still unclaimed. Doing this as the WHERE of the UPDATE
  // is what makes two simultaneous redemptions of one code resolve to one winner: the
  // second matches no row and gets the "already used" answer, rather than both reading
  // "unredeemed" and both extending the license.
  const claimed = await env.DB.prepare(
    `UPDATE plugin_keys SET redeemed_by = ?, redeemed_at = ?
      WHERE code = ? AND redeemed_by IS NULL AND revoked_at IS NULL
      RETURNING plugin_id, months`,
  )
    .bind(account.id, now, code)
    .first<{ plugin_id: string; months: number }>();

  if (!claimed) {
    // It was taken, or revoked, between the read above and this line. Re-read rather than
    // guess, so the answer names what actually happened.
    const raced = await env.DB.prepare(
      `SELECT plugin_id, redeemed_by, revoked_at FROM plugin_keys WHERE code = ?`,
    )
      .bind(code)
      .first<{ plugin_id: string; redeemed_by: string | null; revoked_at: number | null }>();
    if (!raced) return json(404, { error: "that key isn't one of ours" });
    return refusal(raced) ?? json(409, { error: "that key has already been used" });
  }

  // Read again rather than reuse the row from the guard: the months have to be added to
  // whatever the expiry is once this key is definitely ours, not to what it was before.
  const current = await license(account.id, claimed.plugin_id);
  const expires = extendBy(current?.expires_at ?? null, claimed.months, now);
  await env.DB.prepare(
    `INSERT INTO plugin_licenses (account_id, plugin_id, expires_at, granted_at)
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
    license: await issue(env, key, account.id, {
      plugin_id: claimed.plugin_id,
      expires_at: expires,
      bundle_sha256: meta?.bundle_sha256 ?? null,
    }),
  });
}

/**
 * The bundle itself, for an account whose license is live right now.
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
       LEFT JOIN plugin_licenses l ON l.plugin_id = p.id AND l.account_id = ?
                                   AND l.revoked_at IS NULL
      WHERE p.id = ?`,
  )
    .bind(account.id, pluginId)
    .first<{ bundle_key: string | null; bundle_sha256: string | null; expires_at: number | null }>();

  if (!row) return json(404, { error: "no such plugin" });
  if (!row.expires_at || row.expires_at <= nowSec()) {
    return json(403, { error: "no live license for that plugin" });
  }
  if (!row.bundle_key) return json(404, { error: "that plugin has no build published yet" });

  const object = await env.PAINTS.get(row.bundle_key);
  if (!object) return json(404, { error: "the published build is missing" });
  return new Response(object.body, {
    headers: {
      "content-type": "application/octet-stream",
      // Private and uncached: this response is only correct for the license that fetched it.
      "cache-control": "private, no-store",
      ...(row.bundle_sha256 ? { etag: row.bundle_sha256 } : {}),
    },
  });
}

// ---------------------------------------------------------------------------
// admin
// ---------------------------------------------------------------------------
//
// Minting and revoking, as data operations. The HTML that drives them is in
// `pluginspage.ts`; everything that decides anything is here, where it can be tested
// without a page around it.

/**
 * Crockford base32 minus the letters that get misread aloud or in a screenshot: no I, L, O,
 * U. These get typed by hand off a Discord message, and a code that reads `1` as `I` half
 * the time turns every sale into a support conversation.
 *
 * 256 is a whole multiple of 32, so a byte modulo the alphabet is unbiased.
 */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** `FRST-XXXX-XXXX-XXXX`. 60 bits of it, which is more than the sales channel will ever need. */
export function newCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(12));
  const group = (at: number) =>
    [...bytes.slice(at, at + 4)].map((b) => ALPHABET[b % ALPHABET.length]).join("");
  return `FRST-${group(0)}-${group(4)}-${group(8)}`;
}

/** Most keys one press of the button may make. A batch, not a mailing list. */
export const MAX_MINT = 100;

/** Longest a subscription may be sold in one go. */
export const MAX_MONTHS = 24;

export interface Minted {
  ok: boolean;
  codes: string[];
  /** The second the batch was written in, which is how the page finds it again. */
  at?: number;
  error?: string;
}

/**
 * Mint redeemable keys straight into the database.
 *
 * The alternative — and what this replaces — was a script that printed SQL for someone to
 * paste at `wrangler d1 execute --remote`. That works, and it is still there for a
 * disconnected machine, but it makes minting a key for a tester a five-minute errand, which
 * is the reason testing the plugin end to end kept not happening.
 */
export async function mintKeys(
  env: Env,
  pluginId: string,
  months: number,
  count: number,
  note: string,
): Promise<Minted> {
  if (!Number.isInteger(months) || months < 1 || months > MAX_MONTHS) {
    return { ok: false, codes: [], error: `Months must be a whole number, 1 to ${MAX_MONTHS}.` };
  }
  if (!Number.isInteger(count) || count < 1 || count > MAX_MINT) {
    return { ok: false, codes: [], error: `Count must be a whole number, 1 to ${MAX_MINT}.` };
  }
  const plugin = await env.DB.prepare(`SELECT id FROM plugins WHERE id = ?`)
    .bind(pluginId)
    .first<{ id: string }>();
  if (!plugin) return { ok: false, codes: [], error: "No such plugin." };

  const trimmed = note.trim().slice(0, 200);
  const now = nowSec();
  const codes: string[] = [];
  while (codes.length < count) {
    const code = newCode();
    if (!codes.includes(code)) codes.push(code);
  }

  // One batch, so a half-minted run cannot leave codes that were shown to nobody. A
  // collision with an existing code fails the whole batch on the primary key, which at
  // 2^60 per code is a thing to notice rather than a thing to handle.
  await env.DB.batch(
    codes.map((code) =>
      env.DB.prepare(
        `INSERT INTO plugin_keys (code, plugin_id, months, note, created_at)
         VALUES (?, ?, ?, ?, ?)`,
      ).bind(code, pluginId, months, trimmed || null, now),
    ),
  );
  return { ok: true, codes, at: now };
}

/**
 * Withdraw an unredeemed key, or put one back.
 *
 * Only meaningful before redemption — once a key has been spent the thing worth revoking is
 * the license it created, not the code. Revoking a spent key is still allowed, and still
 * says something true about the batch it came from, but it does not take anything away.
 */
export async function setKeyRevoked(env: Env, code: string, revoked: boolean): Promise<void> {
  await env.DB.prepare(`UPDATE plugin_keys SET revoked_at = ? WHERE code = ?`)
    .bind(revoked ? nowSec() : null, normaliseCode(code))
    .run();
}

/**
 * Kill a live license, or bring it back.
 *
 * `expires_at` is left alone. What was paid for stays on the row, so lifting a revocation
 * returns the months that were left rather than an empty license — which matters when the
 * revocation was a mistake, and matters more when it was a chargeback that got reversed.
 *
 * This does not reach into the app. The license token already issued keeps verifying until
 * it expires, which is at most `GRACE_DAYS` away and is the whole point of the grace: the
 * app keeps working on a plane, and stops working within the week when we say so.
 */
export async function setLicenseRevoked(
  env: Env,
  accountId: string,
  pluginId: string,
  revoked: boolean,
): Promise<void> {
  await env.DB.prepare(
    `UPDATE plugin_licenses SET revoked_at = ? WHERE account_id = ? AND plugin_id = ?`,
  )
    .bind(revoked ? nowSec() : null, accountId, pluginId)
    .run();
}

/**
 * Give an account months directly, with no key in between.
 *
 * For testing, and for the handful of times a person has to be made whole without a sale:
 * a tester needs the plugin running on their machine, and minting a key so they can type it
 * back in adds a step that proves nothing. The months are added on exactly the terms a key
 * would add them — see `extendBy`.
 */
export async function grantLicense(
  env: Env,
  who: string,
  pluginId: string,
  months: number,
): Promise<{ ok: boolean; account?: string; expires?: number; error?: string }> {
  if (!Number.isInteger(months) || months < 1 || months > MAX_MONTHS) {
    return { ok: false, error: `Months must be a whole number, 1 to ${MAX_MONTHS}.` };
  }
  // However the person is to hand: an account id copied off another dashboard, the rider
  // name they ride under, or the Steam id they signed in with.
  const account = await env.DB.prepare(
    `SELECT id FROM accounts
      WHERE id = ? OR lower(rider_name) = lower(?) OR steam_id = ? LIMIT 1`,
  )
    .bind(who.trim(), who.trim(), who.trim())
    .first<{ id: string }>();
  if (!account) return { ok: false, error: "No account with that id, rider name or Steam id." };
  const accountId = account.id;
  const plugin = await env.DB.prepare(`SELECT id FROM plugins WHERE id = ?`)
    .bind(pluginId)
    .first<{ id: string }>();
  if (!plugin) return { ok: false, error: "No such plugin." };

  const now = nowSec();
  const current = await env.DB.prepare(
    `SELECT expires_at, revoked_at FROM plugin_licenses WHERE account_id = ? AND plugin_id = ?`,
  )
    .bind(accountId, pluginId)
    .first<{ expires_at: number; revoked_at: number | null }>();
  // Same rule as redemption: months never land on a revoked license. Adding them silently
  // would look like it worked and change nothing the app can see.
  if (current?.revoked_at) {
    return { ok: false, error: "That license is revoked. Restore it before granting months." };
  }

  const expires = extendBy(current?.expires_at ?? null, months, now);
  await env.DB.prepare(
    `INSERT INTO plugin_licenses (account_id, plugin_id, expires_at, granted_at)
     VALUES (?, ?, ?, ?)
     ON CONFLICT (account_id, plugin_id) DO UPDATE SET expires_at = excluded.expires_at`,
  )
    .bind(accountId, pluginId, expires, now)
    .run();
  return { ok: true, account: accountId, expires };
}

/** Where a key is in its life. Revoked wins: it is the only one that is a decision. */
export type KeyState = "unused" | "redeemed" | "revoked";

export function keyState(row: { redeemed_by: string | null; revoked_at: number | null }): KeyState {
  if (row.revoked_at) return "revoked";
  return row.redeemed_by ? "redeemed" : "unused";
}

/** Where a license is. Revoked wins over expired for the same reason. */
export type LicenseState = "live" | "expired" | "revoked";

export function licenseState(
  row: { expires_at: number; revoked_at: number | null },
  now = nowSec(),
): LicenseState {
  if (row.revoked_at) return "revoked";
  return row.expires_at > now ? "live" : "expired";
}

// ---------------------------------------------------------------------------
// admin: the two lists
// ---------------------------------------------------------------------------

/** One key, with whoever spent it resolved to a name. */
export interface KeyRow {
  code: string;
  plugin_id: string;
  months: number;
  note: string | null;
  created_at: number;
  redeemed_by: string | null;
  redeemed_at: number | null;
  revoked_at: number | null;
  rider_name: string | null;
}

/** One license, with the account it belongs to resolved. */
export interface LicenseAdminRow {
  account_id: string;
  plugin_id: string;
  expires_at: number;
  granted_at: number;
  revoked_at: number | null;
  rider_name: string;
  steam_id: string | null;
  kind: string;
}

export interface KeyQuery {
  q: string;
  plugin: string;
  state: KeyState | "any";
  page: number;
}

export interface LicenseQuery {
  q: string;
  plugin: string;
  state: LicenseState | "any";
  page: number;
}

/** The SQL for each state, written once so the filter and the badge cannot disagree. */
const KEY_STATE_SQL: Record<KeyState, string> = {
  unused: "k.revoked_at IS NULL AND k.redeemed_by IS NULL",
  redeemed: "k.revoked_at IS NULL AND k.redeemed_by IS NOT NULL",
  revoked: "k.revoked_at IS NOT NULL",
};

const LICENSE_STATE_SQL: Record<LicenseState, string> = {
  live: "l.revoked_at IS NULL AND l.expires_at > ?",
  expired: "l.revoked_at IS NULL AND l.expires_at <= ?",
  revoked: "l.revoked_at IS NOT NULL",
};

export async function searchKeys(env: Env, query: KeyQuery): Promise<Paged<KeyRow>> {
  const where: string[] = [];
  const args: unknown[] = [];

  const term = likeTerm(query.q);
  if (term) {
    // The code as printed, and the note it was minted under — which is how a batch is
    // found again ("August giveaway") when nobody wrote the codes down.
    where.push("(k.code LIKE ? ESCAPE '\\' OR k.note LIKE ? ESCAPE '\\' OR a.rider_name LIKE ? ESCAPE '\\')");
    args.push(term, term, term);
  }
  if (query.plugin) {
    where.push("k.plugin_id = ?");
    args.push(query.plugin);
  }
  if (query.state !== "any") where.push(KEY_STATE_SQL[query.state]);

  const from =
    " FROM plugin_keys k LEFT JOIN accounts a ON a.id = k.redeemed_by" +
    (where.length ? ` WHERE ${where.join(" AND ")}` : "");

  const total = await env.DB.prepare(
    `SELECT COUNT(*) AS n FROM (SELECT k.code${from} LIMIT ${MAX_COUNT})`,
  )
    .bind(...args)
    .first<{ n: number }>();

  const found = await env.DB.prepare(
    `SELECT k.code, k.plugin_id, k.months, k.note, k.created_at, k.redeemed_by, k.redeemed_at,
            k.revoked_at, a.rider_name${from}
      ORDER BY k.created_at DESC, k.code ASC LIMIT ? OFFSET ?`,
  )
    .bind(...args, PAGE_SIZE, (query.page - 1) * PAGE_SIZE)
    .all<KeyRow>();

  return {
    rows: found.results ?? [],
    total: total?.n ?? (found.results ?? []).length,
    page: query.page,
    size: PAGE_SIZE,
  };
}

export async function searchLicenses(env: Env, query: LicenseQuery): Promise<Paged<LicenseAdminRow>> {
  const now = nowSec();
  const where: string[] = [];
  const args: unknown[] = [];

  const term = likeTerm(query.q);
  if (term) {
    where.push(
      "(a.rider_name LIKE ? ESCAPE '\\' OR a.id LIKE ? ESCAPE '\\' OR a.steam_id LIKE ? ESCAPE '\\')",
    );
    args.push(term, term, term);
  }
  if (query.plugin) {
    where.push("l.plugin_id = ?");
    args.push(query.plugin);
  }
  if (query.state !== "any") {
    where.push(LICENSE_STATE_SQL[query.state]);
    // `revoked` takes no bound value; the other two compare against now.
    if (query.state !== "revoked") args.push(now);
  }

  const from =
    " FROM plugin_licenses l JOIN accounts a ON a.id = l.account_id" +
    (where.length ? ` WHERE ${where.join(" AND ")}` : "");

  const total = await env.DB.prepare(
    `SELECT COUNT(*) AS n FROM (SELECT l.account_id${from} LIMIT ${MAX_COUNT})`,
  )
    .bind(...args)
    .first<{ n: number }>();

  const found = await env.DB.prepare(
    `SELECT l.account_id, l.plugin_id, l.expires_at, l.granted_at, l.revoked_at,
            a.rider_name, a.steam_id, a.kind${from}
      ORDER BY l.revoked_at IS NOT NULL ASC, l.expires_at DESC LIMIT ? OFFSET ?`,
  )
    .bind(...args, PAGE_SIZE, (query.page - 1) * PAGE_SIZE)
    .all<LicenseAdminRow>();

  return {
    rows: found.results ?? [],
    total: total?.n ?? (found.results ?? []).length,
    page: query.page,
    size: PAGE_SIZE,
  };
}

/** The catalogue as the admin page needs it: everything, including what has no build yet. */
export async function adminPlugins(env: Env): Promise<PluginRow[]> {
  const { results } = await env.DB.prepare(
    `SELECT p.id, p.name, p.version, p.bundle_sha256,
            (SELECT COUNT(*) FROM plugin_keys k WHERE k.plugin_id = p.id) AS keys,
            (SELECT COUNT(*) FROM plugin_licenses l
              WHERE l.plugin_id = p.id AND l.revoked_at IS NULL AND l.expires_at > ?) AS live
       FROM plugins p ORDER BY p.name`,
  )
    .bind(nowSec())
    .all<PluginRow>();
  return results ?? [];
}

export interface PluginRow {
  id: string;
  name: string;
  version: string | null;
  bundle_sha256: string | null;
  keys: number;
  live: number;
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
