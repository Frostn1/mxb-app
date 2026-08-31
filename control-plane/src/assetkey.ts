/**
 * Wrapping and unwrapping a content key under the master key.
 *
 * The packer seals each asset under a content key; the DLL needs that exact key to open the
 * blob. We hold it so it can be released to an entitled session and withheld from everyone
 * else — but holding a decryption key in a database is only safe if the database never has
 * the usable form. So the content key is stored **wrapped**: AES-GCM-encrypted under a
 * master key that exists only as a Worker secret. A leaked database is a pile of wrapped
 * keys and no way to unwrap them.
 *
 * AES-GCM via WebCrypto, which is present in the Workers runtime — no dependency, and the
 * primitive rather than a scheme of our own. The wrapped form is `iv(12) ‖ ciphertext+tag`,
 * base64, so it is one self-contained string on the row.
 */

const IV_LEN = 12;

/** Base64 of raw bytes. */
function b64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

/** Raw bytes of base64, or null if it isn't valid base64. */
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
 * Import the master key from the secret.
 *
 * The secret is base64 of 32 raw bytes. Absent or malformed means the feature is off, not
 * that a request should be served with a key derived from nothing — the caller turns `null`
 * into a 503, the same way the rest of the control plane treats a missing credential.
 */
async function masterKey(secret: string | undefined): Promise<CryptoKey | null> {
  if (!secret) return null;
  const raw = unb64(secret.trim());
  if (!raw || raw.length !== 32) return null;
  return crypto.subtle.importKey("raw", raw, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

/**
 * Wrap a content key for storage. Used at pack/register time.
 *
 * `contentKey` is the raw 32-byte CEK. Returns the base64 string to put on the row, or null
 * if there is no usable master key — in which case the asset cannot be registered as
 * secured, which is the correct refusal rather than storing the key in the clear.
 */
export async function wrapContentKey(
  contentKey: Uint8Array,
  secret: string | undefined,
): Promise<string | null> {
  const key = await masterKey(secret);
  if (!key || contentKey.length !== 32) return null;
  const iv = crypto.getRandomValues(new Uint8Array(IV_LEN));
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, contentKey));
  const out = new Uint8Array(IV_LEN + ct.length);
  out.set(iv, 0);
  out.set(ct, IV_LEN);
  return b64(out);
}

/**
 * Unwrap a stored content key for release to an entitled session.
 *
 * Returns the raw 32-byte CEK, or null if the master key is missing or the wrapped value
 * doesn't authenticate — a tampered or wrongly-keyed row must yield nothing, never a
 * plausible-looking key.
 */
export async function unwrapContentKey(
  wrapped: string,
  secret: string | undefined,
): Promise<Uint8Array | null> {
  const key = await masterKey(secret);
  if (!key) return null;
  const raw = unb64(wrapped);
  if (!raw || raw.length <= IV_LEN) return null;
  const iv = raw.slice(0, IV_LEN);
  const ct = raw.slice(IV_LEN);
  try {
    const pt = new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ct));
    return pt.length === 32 ? pt : null;
  } catch {
    return null;
  }
}
