/**
 * Wrapping and unwrapping a content key under the master key — with rotation.
 *
 * The packer seals each asset under a content key; the DLL needs that exact key to open the
 * blob. We hold it so it can be released to an entitled session and withheld from everyone
 * else — but holding a decryption key in a database is only safe if the database never has
 * the usable form. So the content key is stored **wrapped**: AES-GCM-encrypted under a
 * master key that exists only as a Worker secret. A leaked database is a pile of wrapped
 * keys and no way to unwrap them.
 *
 * ## Rotation
 *
 * The master key is **not** permanent. A content key lives in the blob and never changes;
 * only its wrapped form on the row is under the master key. So the master key can be rotated
 * with **no re-packing**: unwrap each row under the key it was wrapped with, re-wrap under the
 * new one. To know which key wrapped a row, the wrapped value is version-tagged
 * (`"<version>:<base64(iv‖ct)>"`); a legacy row with no tag is version `"1"`, i.e. the single
 * `MXB_ASSET_MASTER_KEY`. Multiple versions live in `MXB_ASSET_MASTER_KEYS` (a JSON map
 * `{ "1": b64, "2": b64 }`), and `MXB_ASSET_MASTER_KEY_VERSION` names the one to wrap under.
 * A single-key deployment (only `MXB_ASSET_MASTER_KEY` set) keeps working: it is version `"1"`.
 *
 * AES-GCM via WebCrypto, present in the Workers runtime — the primitive, not a scheme of our
 * own.
 */

const IV_LEN = 12;
const DEFAULT_VERSION = "1";

/** The env fields that carry master-key material. */
export interface MasterKeyEnv {
  MXB_ASSET_MASTER_KEY?: string;
  MXB_ASSET_MASTER_KEYS?: string;
  MXB_ASSET_MASTER_KEY_VERSION?: string;
}

/** The available master keys, and which version new wraps use. */
interface MasterKeys {
  current: string;
  raw: Map<string, Uint8Array>;
}

/** Base64 of raw bytes. */
function b64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

/** Raw bytes of base64, or null if it isn't valid base64 of the right length material. */
function unb64(text: string): Uint8Array | null {
  try {
    const s = atob(text);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/**
 * Load every configured master key, keyed by version, and the current version to wrap under.
 *
 * `MXB_ASSET_MASTER_KEYS` (JSON `{version: b64}`) is the general form; a bare
 * `MXB_ASSET_MASTER_KEY` is folded in as version `"1"` so a single-key deployment needs no
 * change. Each key must be base64 of exactly 32 bytes or it is ignored. `null` when nothing
 * usable is configured — the caller turns that into a 503, never a fixed fallback key.
 */
function loadMasterKeys(env: MasterKeyEnv): MasterKeys | null {
  const raw = new Map<string, Uint8Array>();

  if (env.MXB_ASSET_MASTER_KEYS) {
    try {
      const map = JSON.parse(env.MXB_ASSET_MASTER_KEYS) as Record<string, unknown>;
      for (const [version, value] of Object.entries(map)) {
        if (typeof value !== "string") continue;
        const bytes = unb64(value.trim());
        if (bytes && bytes.length === 32) raw.set(version, bytes);
      }
    } catch {
      // A malformed map is treated as absent rather than fatal — the single key below can
      // still carry the deployment.
    }
  }

  if (env.MXB_ASSET_MASTER_KEY && !raw.has(DEFAULT_VERSION)) {
    const bytes = unb64(env.MXB_ASSET_MASTER_KEY.trim());
    if (bytes && bytes.length === 32) raw.set(DEFAULT_VERSION, bytes);
  }

  if (raw.size === 0) return null;

  const current = env.MXB_ASSET_MASTER_KEY_VERSION?.trim() || DEFAULT_VERSION;
  if (!raw.has(current)) return null; // asked to wrap under a version we don't hold — refuse
  return { current, raw };
}

/** Import raw key bytes as an AES-GCM key. */
function importKey(raw: Uint8Array): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", raw, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

/**
 * Split a stored wrapped value into its version and body. A legacy value with no `":"` is
 * version `"1"` — base64 never contains a colon, so the tag is unambiguous.
 */
function splitWrapped(wrapped: string): { version: string; body: string } {
  const i = wrapped.indexOf(":");
  if (i > 0) return { version: wrapped.slice(0, i), body: wrapped.slice(i + 1) };
  return { version: DEFAULT_VERSION, body: wrapped };
}

/** The version a stored wrapped value was wrapped under. */
export function wrappedVersion(wrapped: string): string {
  return splitWrapped(wrapped).version;
}

/** The version new wraps will use, or `null` if no master key is configured. */
export function currentMasterVersion(env: MasterKeyEnv): string | null {
  return loadMasterKeys(env)?.current ?? null;
}

/**
 * Wrap a content key for storage. Used at pack/register time and at rotation.
 *
 * Returns `"<version>:<base64(iv‖ct)>"`, or `null` if there is no usable master key or the
 * content key is not 32 bytes.
 */
export async function wrapContentKey(
  contentKey: Uint8Array,
  env: MasterKeyEnv,
): Promise<string | null> {
  const keys = loadMasterKeys(env);
  if (!keys || contentKey.length !== 32) return null;
  const key = await importKey(keys.raw.get(keys.current)!);
  const iv = crypto.getRandomValues(new Uint8Array(IV_LEN));
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, contentKey));
  const out = new Uint8Array(IV_LEN + ct.length);
  out.set(iv, 0);
  out.set(ct, IV_LEN);
  return `${keys.current}:${b64(out)}`;
}

/**
 * Unwrap a stored content key for release to an entitled session.
 *
 * Reads the version tag, picks that master key, and decrypts. Returns the raw 32-byte content
 * key, or `null` if the master key is missing, the version is unknown, or the value doesn't
 * authenticate — a tampered or wrongly-keyed row must yield nothing.
 */
export async function unwrapContentKey(
  wrapped: string,
  env: MasterKeyEnv,
): Promise<Uint8Array | null> {
  const keys = loadMasterKeys(env);
  if (!keys) return null;
  const { version, body } = splitWrapped(wrapped);
  const raw = keys.raw.get(version);
  if (!raw) return null;
  const bytes = unb64(body);
  if (!bytes || bytes.length <= IV_LEN) return null;
  const key = await importKey(raw);
  const iv = bytes.slice(0, IV_LEN);
  const ct = bytes.slice(IV_LEN);
  try {
    const pt = new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ct));
    return pt.length === 32 ? pt : null;
  } catch {
    return null;
  }
}

/**
 * Re-wrap a stored value to the current master-key version: unwrap under whatever version it
 * carries, wrap under the current one. This is rotation, per row, with no re-packing. Returns
 * the new stored value, or `null` if it could not be unwrapped (a missing old key or a
 * tampered row — those are surfaced, not silently dropped).
 */
export async function rewrapToCurrent(
  wrapped: string,
  env: MasterKeyEnv,
): Promise<string | null> {
  const contentKey = await unwrapContentKey(wrapped, env);
  if (!contentKey) return null;
  return wrapContentKey(contentKey, env);
}
