//! Lock a creator's file into a `.mxbsecure` blob, and open one back — the app's side of the
//! secure-content feature.
//!
//! This is a self-contained copy of the `mxbsecure` crate's format and seal, condensed to the
//! two operations the app needs: **pack** (the Lock tab) and **open** (verify a lock, and the
//! future in-app preview). It is gitignored in `mxb-app` and synced from `mxbapp-private`,
//! the same way `sidecar.rs` is — the format and cipher stay out of the public tree.
//!
//! **The blob bytes must stay identical to the crate's**, because the injected DLL opens what
//! this writes. The construction here is copied verbatim: chunked XChaCha20-Poly1305, a nonce
//! of `base ‖ chunk-index`, the header authenticated as associated data, and the content key
//! kept out of the blob. See the crate's `format.rs` for the full rationale; if you change one
//! byte of the layout here, change it there too.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

const MAGIC: &[u8; 6] = b"MXBSEC";
const VERSION: u8 = 1;
const ALG_XCHACHA20POLY1305: u8 = 1;
const TAG_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const BASE_NONCE_LEN: usize = 16;
const DEFAULT_CHUNK_SIZE: u32 = 64 * 1024;

#[derive(Debug)]
pub enum Error {
    Header(&'static str),
    Truncated,
    Unauthentic,
    Range,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Header(w) => write!(f, "{w}"),
            Error::Truncated => write!(f, "blob is truncated"),
            Error::Unauthentic => write!(f, "a chunk did not authenticate"),
            Error::Range => write!(f, "requested range is outside the asset"),
        }
    }
}

impl std::error::Error for Error {}

struct Header {
    chunk_size: u32,
    plain_len: u64,
    chunk_count: u64,
    base_nonce: [u8; BASE_NONCE_LEN],
    asset_id: String,
    key_id: String,
}

impl Header {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(46 + self.asset_id.len() + self.key_id.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(ALG_XCHACHA20POLY1305);
        out.extend_from_slice(&self.chunk_size.to_le_bytes());
        out.extend_from_slice(&self.plain_len.to_le_bytes());
        out.extend_from_slice(&self.chunk_count.to_le_bytes());
        out.extend_from_slice(&self.base_nonce);
        out.extend_from_slice(&(self.asset_id.len() as u16).to_le_bytes());
        out.extend_from_slice(self.asset_id.as_bytes());
        out.extend_from_slice(&(self.key_id.len() as u16).to_le_bytes());
        out.extend_from_slice(self.key_id.as_bytes());
        out
    }

    fn parse(blob: &[u8]) -> Result<(Header, usize), Error> {
        let mut at = 0usize;
        let take = |at: &mut usize, n: usize| -> Result<&[u8], Error> {
            let end = at.checked_add(n).ok_or(Error::Truncated)?;
            let slice = blob.get(*at..end).ok_or(Error::Truncated)?;
            *at = end;
            Ok(slice)
        };
        if take(&mut at, 6)? != MAGIC {
            return Err(Error::Header("not a .mxbsecure blob"));
        }
        if take(&mut at, 1)?[0] != VERSION {
            return Err(Error::Header("unknown format version"));
        }
        if take(&mut at, 1)?[0] != ALG_XCHACHA20POLY1305 {
            return Err(Error::Header("unknown algorithm"));
        }
        let chunk_size = u32::from_le_bytes(take(&mut at, 4)?.try_into().unwrap());
        let plain_len = u64::from_le_bytes(take(&mut at, 8)?.try_into().unwrap());
        let chunk_count = u64::from_le_bytes(take(&mut at, 8)?.try_into().unwrap());
        let mut base_nonce = [0u8; BASE_NONCE_LEN];
        base_nonce.copy_from_slice(take(&mut at, BASE_NONCE_LEN)?);
        let alen = u16::from_le_bytes(take(&mut at, 2)?.try_into().unwrap()) as usize;
        let asset_id = std::str::from_utf8(take(&mut at, alen)?)
            .map_err(|_| Error::Header("bad asset id"))?
            .to_string();
        let klen = u16::from_le_bytes(take(&mut at, 2)?.try_into().unwrap()) as usize;
        let key_id = std::str::from_utf8(take(&mut at, klen)?)
            .map_err(|_| Error::Header("bad key id"))?
            .to_string();
        Ok((Header { chunk_size, plain_len, chunk_count, base_nonce, asset_id, key_id }, at))
    }

    fn nonce(&self, index: u64) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[..BASE_NONCE_LEN].copy_from_slice(&self.base_nonce);
        nonce[BASE_NONCE_LEN..].copy_from_slice(&index.to_le_bytes());
        nonce
    }

    fn chunk_aad(&self, header_bytes: &[u8], index: u64) -> Vec<u8> {
        let mut aad = Vec::with_capacity(header_bytes.len() + 9);
        aad.extend_from_slice(header_bytes);
        aad.extend_from_slice(&index.to_le_bytes());
        aad.push(if index + 1 == self.chunk_count { 1 } else { 0 });
        aad
    }
}

/// What a locked asset is called and keyed by, before it is stored.
pub struct Locked {
    pub blob: Vec<u8>,
    pub content_key: [u8; KEY_LEN],
    pub asset_id: String,
    pub key_id: String,
}

/// Lock `plaintext` under a fresh random content key. `asset_id`/`key_id` are recorded in the
/// (authenticated) header; a real registration mints these, but for a local lock the caller
/// can pass any stable strings.
pub fn lock(plaintext: &[u8], asset_id: &str, key_id: &str) -> Locked {
    let mut content_key = [0u8; KEY_LEN];
    getrandom::getrandom(&mut content_key).expect("OS CSPRNG unavailable");
    let mut base_nonce = [0u8; BASE_NONCE_LEN];
    getrandom::getrandom(&mut base_nonce).expect("OS CSPRNG unavailable");

    let chunk_bytes = DEFAULT_CHUNK_SIZE as usize;
    let chunk_count = plaintext.len().div_ceil(chunk_bytes) as u64;
    let header = Header {
        chunk_size: DEFAULT_CHUNK_SIZE,
        plain_len: plaintext.len() as u64,
        chunk_count,
        base_nonce,
        asset_id: asset_id.to_string(),
        key_id: key_id.to_string(),
    };
    let header_bytes = header.encode();
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&content_key));

    let mut blob = header_bytes.clone();
    for (index, chunk) in plaintext.chunks(chunk_bytes).enumerate() {
        let index = index as u64;
        let nonce = header.nonce(index);
        let aad = header.chunk_aad(&header_bytes, index);
        let sealed = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: chunk, aad: &aad })
            .expect("AEAD seal cannot fail on valid inputs");
        blob.extend_from_slice(&sealed);
    }
    Locked { blob, content_key, asset_id: asset_id.to_string(), key_id: key_id.to_string() }
}

/// Open a whole blob back to plaintext given its content key. Used to verify a lock, and by
/// the in-app preview later. Fails closed on any tamper, truncation, or wrong key.
pub fn open(blob: &[u8], content_key: &[u8; KEY_LEN]) -> Result<Vec<u8>, Error> {
    let (header, body_at) = Header::parse(blob)?;
    let header_bytes = header.encode();
    let cipher = XChaCha20Poly1305::new(Key::from_slice(content_key));
    let full_ct = header.chunk_size as usize + TAG_LEN;

    let mut out = Vec::with_capacity(header.plain_len as usize);
    for index in 0..header.chunk_count {
        let is_last = index + 1 == header.chunk_count;
        let ct_len = if is_last {
            let remainder = header.plain_len - index * header.chunk_size as u64;
            remainder as usize + TAG_LEN
        } else {
            full_ct
        };
        let offset = body_at + (index as usize) * full_ct;
        let ct = blob.get(offset..offset + ct_len).ok_or(Error::Truncated)?;
        let nonce = header.nonce(index);
        let aad = header.chunk_aad(&header_bytes, index);
        let plain = cipher
            .decrypt(XNonce::from_slice(&nonce), Payload { msg: ct, aad: &aad })
            .map_err(|_| Error::Unauthentic)?;
        out.extend_from_slice(&plain);
    }
    if out.len() as u64 != header.plain_len {
        return Err(Error::Range);
    }
    Ok(out)
}

/// The asset id and key id recorded in a blob's header, read without a key — what the
/// registry needs to store the blob. Not called by the app yet; kept for the register step.
#[allow(dead_code)]
pub fn header_of(blob: &[u8]) -> Result<(String, String, u64), Error> {
    let (h, _) = Header::parse(blob)?;
    Ok((h.asset_id, h.key_id, h.plain_len))
}

/// Domain separator for the identity binding, versioned. Must match the crate's keybind.rs.
const KEYBIND_CONTEXT: &[u8] = b"mxbsecure-keybind-v1";

/// Derive the seal key from the buyer's Steam ID (+ optional GUID).
fn seal_key(steam_id: &str, guid: &str) -> [u8; KEY_LEN] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(KEYBIND_CONTEXT);
    h.update([0x1f]);
    h.update(steam_id.trim().as_bytes());
    h.update([0x1f]);
    h.update(guid.trim().as_bytes());
    h.finalize().into()
}

/// Seal a content key to `(steam_id, guid)` for local, offline storage — the `.mxbkey`.
///
/// Provisioned once (the server released the key); from then on the app reads the live Steam
/// ID and unseals with no network. A copy on another account derives a different key and
/// [`unseal_key`] returns `None`.
pub fn seal_key_to_identity(content_key: &[u8; KEY_LEN], steam_id: &str, guid: &str) -> Vec<u8> {
    let key = seal_key(steam_id, guid);
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).expect("OS CSPRNG unavailable");
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: content_key, aad: KEYBIND_CONTEXT })
        .expect("AEAD seal cannot fail on valid inputs");
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

/// Unseal a `.mxbkey` with the live identity. `None` on a different account or tamper.
pub fn unseal_key(sealed: &[u8], steam_id: &str, guid: &str) -> Option<[u8; KEY_LEN]> {
    if sealed.len() < NONCE_LEN {
        return None;
    }
    let (nonce, ct) = sealed.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&seal_key(steam_id, guid)));
    let pt = cipher
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: KEYBIND_CONTEXT })
        .ok()?;
    pt.try_into().ok()
}

/// 32 bytes as lowercase hex — how the content key is shown once, for the server to hold.
pub fn hex_key(key: &[u8; KEY_LEN]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse a hex content key back to bytes.
pub fn key_from_hex(hex: &str) -> Option<[u8; KEY_LEN]> {
    let hex = hex.trim();
    if hex.len() != KEY_LEN * 2 {
        return None;
    }
    let mut key = [0u8; KEY_LEN];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_then_open_round_trips() {
        let plaintext = b"a creator's track, locked once and only decrypted with the key".to_vec();
        let locked = lock(&plaintext, "trk_pinehill", "k1");
        assert_eq!(open(&locked.blob, &locked.content_key).unwrap(), plaintext);
        assert!(!locked.blob.windows(KEY_LEN).any(|w| w == locked.content_key), "key not in blob");
    }

    #[test]
    fn the_wrong_key_will_not_open_it() {
        let locked = lock(b"secret", "a", "k");
        assert!(open(&locked.blob, &[0u8; KEY_LEN]).is_err());
    }

    #[test]
    fn a_tampered_blob_is_refused() {
        let locked = lock(&vec![7u8; 300], "a", "k");
        let mut blob = locked.blob.clone();
        *blob.last_mut().unwrap() ^= 1;
        assert!(open(&blob, &locked.content_key).is_err());
    }

    #[test]
    fn seal_binds_to_the_steam_id() {
        let locked = lock(b"x", "a", "k");
        let sealed = seal_key_to_identity(&locked.content_key, "76561198000000001", "");
        assert_eq!(unseal_key(&sealed, "76561198000000001", ""), Some(locked.content_key));
        assert_eq!(unseal_key(&sealed, "76561198000000002", ""), None, "another account");
    }

    #[test]
    fn hex_round_trips() {
        let locked = lock(b"x", "a", "k");
        let hex = hex_key(&locked.content_key);
        assert_eq!(key_from_hex(&hex), Some(locked.content_key));
        assert_eq!(header_of(&locked.blob).unwrap().0, "a");
    }
}
