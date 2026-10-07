/**
 * Plugins: the catalogue, the bundle, and the signed license the app runs on.
 *
 * Every plugin is free. There are no keys and no license rows: any signed-in account gets a
 * license for every plugin on each check-in. The license stays because the app's install and
 * run checks (account, bundle hash, refresh) are built on it.
 *
 * The shape of the problem is that the app has to keep working on a plane. So the plane is
 * not asked "may this person run the plugin" at the moment they run it — it is asked
 * periodically, and answers with a short-lived **license**: a small signed statement the
 * app can check by itself, with no network and no shared secret.
 *
 * Signed with Ed25519, not an HMAC. An HMAC would need the same key at both ends, and the
 * app's end ships to everyone who runs the plugin — one `strings` away from being able
 * to mint their own. With a signature the app only ever holds the public half, and forging
 * an license means breaking the curve rather than reading a binary.
 *
 * Two clocks, deliberately different:
 *   * `expires`      — when the license runs out. A year away, rolled forward on each check-in.
 *   * `refreshAfter` — when the app must have talked to us again. Days away.
 * A banned account therefore stops working within the grace window, without the app needing
 * to be online to find out.
 */

/** How long an license is honoured with no contact. */
export const GRACE_DAYS = 7;

/** Format version, so a future field can be added without old apps mis-reading a token. */
export const LICENSE_VERSION = 1;

export interface License {
  v: number;
  /** Account the license was issued to. Bound so a token cannot be passed around. */
  account: string;
  plugin: string;
  /** Seconds since epoch. When the license ends. */
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

interface PluginRow {
  plugin_id: string;
  bundle_sha256: string | null;
  version: string | null;
  name: string;
}

const DAY = 86400;

/** How far a license reaches. It is re-issued on every check-in. */
export const FREE_TERM_DAYS = 365;

function nowSec(): number {
  return Math.floor(Date.now() / 1000);
}

async function issue(
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
      refreshAfter: Math.min(now + GRACE_DAYS * DAY, row.expires_at),
      bundleSha256: row.bundle_sha256,
      issued: now,
    },
    key,
  );
}

/** The catalogue. Public: what is on offer is not a secret, and the app shows it to everyone. */
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
      // Published means "there is a build to install".
      published: Boolean(p.bundle_sha256),
      free: true,
    })),
  });
}

/** A freshly signed license for every plugin, for this account. */
export async function myPlugins(account: Account, env: Env): Promise<Response> {
  const key = await signingKey(env);
  if (!key) return json(503, { error: "plugin licensing is not configured" });

  const { results } = await env.DB.prepare(
    `SELECT id AS plugin_id, bundle_sha256, version, name FROM plugins ORDER BY name`,
  ).all<PluginRow>();

  const expires = nowSec() + FREE_TERM_DAYS * DAY;
  const licenses = [];
  for (const row of results ?? []) {
    licenses.push({
      plugin: row.plugin_id,
      name: row.name,
      version: row.version,
      expires,
      active: true,
      license: await issue(key, account.id, { ...row, expires_at: expires }),
      free: true,
    });
  }
  return json(200, { licenses });
}

/**
 * The bundle itself, for any signed-in account (banned ones are refused before this).
 *
 * Streamed from R2 rather than redirected to it: a redirect would be a URL that works for
 * whoever holds it, signed in or not.
 */
export async function pluginBundle(
  pluginId: string,
  _account: Account,
  env: Env,
): Promise<Response> {
  const row = await env.DB.prepare(`SELECT bundle_key, bundle_sha256 FROM plugins WHERE id = ?`)
    .bind(pluginId)
    .first<{ bundle_key: string | null; bundle_sha256: string | null }>();

  if (!row) return json(404, { error: "no such plugin" });
  if (!row.bundle_key) return json(404, { error: "that plugin has no build published yet" });

  const object = await env.PAINTS.get(row.bundle_key);
  if (!object) return json(404, { error: "the published build is missing" });
  return new Response(object.body, {
    headers: {
      "content-type": "application/octet-stream",
      // Private and uncached: only signed-in accounts get it.
      "cache-control": "private, no-store",
      ...(row.bundle_sha256 ? { etag: row.bundle_sha256 } : {}),
    },
  });
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
