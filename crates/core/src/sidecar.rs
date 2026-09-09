//! PROPRIETARY — DO NOT COMMIT. This module is gitignored on purpose.
//!
//! Native decryptor for MX Bikes' encrypted (`KCOL`) `.pkz` archives, so the 3D
//! viewer can pull a bike's `model.edf` + textures straight out of a packaged,
//! encrypted mod — no game, no PaintEd. Reverse-engineered from `mxbikes.exe`
//! and validated end-to-end (every entry's CRC-32 matches) against real
//! OEM/track archives.
//!
//! This file is auto-detected by `build.rs`: when present it sets `cfg(kcol)`
//! and the rest of the app wires the encrypted path in; when absent (the public
//! open-source tree) the app compiles fine and reports encrypted archives as
//! unsupported. Keep it OUT of version control (see `src-tauri/.gitignore`).
//!
//! Scheme: a ZIP whose *metadata* is obfuscated but whose *payloads* are plain
//! deflate. A 62-byte `KCOL` footer at EOF holds the drop base, an obfuscated
//! 16-byte RC4 key, the encrypted-directory size, and a 16-bit directory
//! checksum. The RC4 key is de-obfuscated with a 6-byte XOR pad derived from
//! dirSize + checksum. `drop = footer.drop_base + 0x100`. The central directory,
//! and each entry's 30-byte local header + filename, are RC4'd (same key + drop,
//! a fresh keystream per region). Compressed payloads are NOT encrypted.

use anyhow::{bail, Context, Result};
use flate2::read::DeflateDecoder;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const KCOL_MAGIC: &[u8; 4] = b"KCOL";
pub const KCOL_FOOTER_LEN: usize = 62;

/// Keystream bytes discarded before the first byte of every region, on top of the
/// footer's own drop base.
pub const KCOL_DROP_BIAS: usize = 0x100;

/// The 6-byte XOR pad that obfuscates the footer's key/GUID region, from dirSize (D)
/// and checksum (C) — constants lifted from the binary. Pad index is the byte's own
/// offset within the footer, modulo 6.
pub fn footer_pad(dir_size: u32, checksum: u16) -> [u8; 6] {
    [
        (dir_size as u8) ^ 0xFD,
        ((dir_size >> 8) as u8) ^ 0xC7,
        ((dir_size >> 16) as u8) ^ 0x1C,
        ((dir_size >> 24) as u8) ^ 0xAF,
        (checksum as u8) ^ 0x35,
        ((checksum >> 8) as u8) ^ 0x71,
    ]
}

/// RC4 stream cipher with an initial keystream **drop** (RC4-drop[N]), exactly as
/// MX Bikes keys its encrypted `.pkz` entries. Symmetric: the same call decrypts
/// and encrypts.
pub struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    /// Key-schedule (KSA) from `key`, then discard `drop` keystream bytes.
    pub fn new(key: &[u8], drop: usize) -> Self {
        let mut s = [0u8; 256];
        for (i, b) in s.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut j: u8 = 0;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        let mut rc4 = Rc4 { s, i: 0, j: 0 };
        for _ in 0..drop {
            rc4.next_byte();
        }
        rc4
    }

    /// Advance the PRGA one step and return the keystream byte.
    fn next_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[self.i as usize]);
        self.s.swap(self.i as usize, self.j as usize);
        let idx = self.s[self.i as usize].wrapping_add(self.s[self.j as usize]);
        self.s[idx as usize]
    }

    /// XOR `buf` in place with the keystream (decrypt == encrypt).
    pub fn apply(&mut self, buf: &mut [u8]) {
        for b in buf.iter_mut() {
            *b ^= self.next_byte();
        }
    }

    /// Materialize the next `len` keystream bytes (for XORing at chosen offsets).
    pub fn keystream(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_byte()).collect()
    }
}

/// One decrypted archive member: its path and raw (decompressed) bytes.
pub struct PkzEntry {
    pub name: String,
    pub data: Vec<u8>,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Whether a `.pkz` carries the encrypted `KCOL` footer we can decrypt.
pub fn is_kcol(path: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    if f.seek(SeekFrom::End(-(KCOL_FOOTER_LEN as i64))).is_err() {
        return false;
    }
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).is_ok() && &magic == KCOL_MAGIC
}

/// Whether `bytes` carries the encrypted `KCOL` footer (in-memory twin of
/// [`is_kcol`]). Used to detect a creator-locked `.pnt` we can decrypt.
pub fn handles(bytes: &[u8]) -> bool {
    bytes.len() >= KCOL_FOOTER_LEN && &bytes[bytes.len() - KCOL_FOOTER_LEN..][..4] == KCOL_MAGIC
}

/// Parse the `KCOL` footer, de-obfuscate the RC4 key, decrypt the directory
/// region, and verify its 16-bit checksum. Returns the decrypted region plus the
/// `(key, drop)` for decrypting entry headers.
///
/// For a packaged `.pkz` the region is the ZIP central directory (`dir_off` sits
/// near EOF). For a **single-blob** container — a creator-locked `.pnt` — the
/// footer's `dir_size` spans the whole file, so `dir_off` is 0 and the decrypted
/// region IS the plaintext payload (a plain `PNT\0` container).
fn decrypt_dir(bytes: &[u8]) -> Result<(Vec<u8>, [u8; 16], usize)> {
    let n = bytes.len();
    if n < KCOL_FOOTER_LEN {
        bail!("file too small for a KCOL footer");
    }
    let f = &bytes[n - KCOL_FOOTER_LEN..];
    if &f[0..4] != KCOL_MAGIC {
        bail!("not a KCOL-encrypted file");
    }
    let drop_base = u32le(f, 4) as usize;
    let obf_key = &f[8..24];
    let dir_size = u32le(f, 56) as usize;
    let checksum = u16le(f, 60);

    let pad = footer_pad(dir_size as u32, checksum);
    // The RC4 key sits at buffer position 32 (after the two 16-byte sig fields),
    // so key[i] = obfKey[i] ^ pad[(32 + i) % 6] — which is the key's own footer
    // offset (8 + i) modulo 6, since 32 ≡ 8 (mod 6).
    let mut key = [0u8; 16];
    for i in 0..16 {
        key[i] = obf_key[i] ^ pad[(32 + i) % 6];
    }
    let drop = drop_base + KCOL_DROP_BIAS;

    if dir_size + KCOL_FOOTER_LEN > n {
        bail!("KCOL dirSize {dir_size} overruns file");
    }
    let dir_off = n - dir_size - KCOL_FOOTER_LEN;
    let mut dir = bytes[dir_off..dir_off + dir_size].to_vec();
    Rc4::new(&key, drop).apply(&mut dir);
    if dir_checksum(&dir) != checksum {
        bail!("KCOL directory checksum mismatch (wrong key?)");
    }
    Ok((dir, key, drop))
}

/// The footer's 16-bit check over a decrypted region: the byte sum, low 16 bits.
///
/// Folded into a `u16` rather than summed into a `u32` and masked. For a packaged `.pkz`
/// the region is a central directory and either spelling agrees, but a single-blob
/// container *is* the whole file — and a locked helmet paint runs to 30-odd MB, whose byte
/// sum passes `u32::MAX` around 16 MB. A release build wrapped (and so read the right 16
/// bits); a debug build panicked on the overflow, which is why every locked paint over
/// that size failed to decode under `tauri dev` and nowhere else.
pub fn dir_checksum(dir: &[u8]) -> u16 {
    dir.iter().fold(0u16, |acc, &b| acc.wrapping_add(b as u16))
}

/// Decrypt a single-blob `KCOL` container (e.g. a creator-locked `.pnt`) to its
/// plaintext bytes — the whole file is one RC4-encrypted region.
pub fn read_blob(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(decrypt_dir(bytes)?.0)
}

/// Decrypt an encrypted (`KCOL`) `.pkz` into all its members (payloads inflated).
pub fn decrypt_kcol(bytes: &[u8]) -> Result<Vec<PkzEntry>> {
    decrypt_kcol_selected(bytes, |_| true)
}

/// Like [`decrypt_kcol`] but only inflates the entries whose name passes `keep`.
/// Names are recovered from the (cheap) local headers, so we skip decompressing
/// everything else — e.g. a bike's dozens of MB of sound `.wav`s when the viewer
/// only wants `model.edf` + textures.
pub fn decrypt_kcol_selected(
    bytes: &[u8],
    keep: impl Fn(&str) -> bool,
) -> Result<Vec<PkzEntry>> {
    let n = bytes.len();
    let (dir, key, drop) = decrypt_dir(bytes)?;

    // One fresh keystream, reused for every entry's local header (30 B) + name.
    const MAX_NAME: usize = 4096;
    let ks = Rc4::new(&key, drop).keystream(30 + MAX_NAME);

    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 46 <= dir.len() && &dir[off..off + 4] == b"PK\x01\x02" {
        let nl = u16le(&dir, off + 28) as usize;
        let el = u16le(&dir, off + 30) as usize;
        let cl = u16le(&dir, off + 32) as usize;
        let lho = u32le(&dir, off + 42) as usize;

        if lho + 30 > n {
            bail!("entry local header past end of file");
        }
        // Decrypt the 30-byte local header, then read its own name/extra/size.
        let mut lh = [0u8; 30];
        for i in 0..30 {
            lh[i] = bytes[lho + i] ^ ks[i];
        }
        if u32le(&lh, 0) != 0x0403_4b50 {
            bail!("bad local header signature (decrypt failed)");
        }
        // Prefer the CENTRAL DIRECTORY's method/size. A `.pkz` written streaming
        // sets the data-descriptor flag and leaves the local header's sizes ZERO
        // (the real ones live here) — reading them from the local header yields a
        // 0-byte payload for every entry, which is what made packaged gear mods
        // decrypt to empty files.
        let dir_method = u16le(&dir, off + 10);
        let dir_comp_size = u32le(&dir, off + 20) as usize;
        let lh_comp_size = u32le(&lh, 18) as usize;
        let method = if dir_comp_size > 0 { dir_method } else { u16le(&lh, 8) };
        let comp_size = if dir_comp_size > 0 { dir_comp_size } else { lh_comp_size };
        let lnl = u16le(&lh, 26) as usize;
        let lel = u16le(&lh, 28) as usize;
        if lnl > MAX_NAME {
            bail!("entry name length {lnl} exceeds cap");
        }
        let mut name_bytes = vec![0u8; lnl];
        for i in 0..lnl {
            name_bytes[i] = bytes[lho + 30 + i] ^ ks[30 + i];
        }
        let name = String::from_utf8_lossy(&name_bytes).into_owned();

        // Only inflate what the caller wants — skip the rest for free.
        if keep(&name) {
            let data_off = lho + 30 + lnl + lel;
            if data_off + comp_size > n {
                bail!("entry '{name}' payload past end of file");
            }
            let payload = &bytes[data_off..data_off + comp_size];
            // Payload is plaintext: stored (0) or raw-deflate (8), never RC4'd.
            let data = if method == 8 {
                let mut raw = Vec::new();
                DeflateDecoder::new(payload)
                    .read_to_end(&mut raw)
                    .with_context(|| format!("inflate entry '{name}'"))?;
                raw
            } else {
                payload.to_vec()
            };
            out.push(PkzEntry { name, data });
        }
        off += 46 + nl + el + cl;
    }
    Ok(out)
}

/// Read the entries of a handled (encrypted) archive whose name passes `keep`, as
/// `(name, bytes)`. Skips decompressing everything else.
pub fn read_selected(path: &Path, keep: impl Fn(&str) -> bool) -> Result<Vec<(String, Vec<u8>)>> {
    if !is_kcol(path) {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    Ok(decrypt_kcol_selected(&bytes, keep)?
        .into_iter()
        .map(|e| (e.name, e.data))
        .collect())
}

/// Entry point used by `pkz::extract`: if this archive is one we handle, decrypt
/// and write it out and return `Some(paths)`; otherwise `None` so the caller can
/// fall through. Keeps all format-specific logic in this (local-only) module.
pub fn try_extract(path: &Path, out_dir: &Path) -> Result<Option<Vec<String>>> {
    if !is_kcol(path) {
        return Ok(None);
    }
    extract_encrypted(path, out_dir).map(Some)
}

/// Read every entry of a handled (encrypted) archive as `(name, bytes)`.
pub fn read_all(path: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    if !is_kcol(path) {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    Ok(decrypt_kcol(&bytes)?
        .into_iter()
        .map(|e| (e.name, e.data))
        .collect())
}

/// Read a single entry (by file-name, case-insensitive) out of a handled archive,
/// decompressed. `None` if this isn't a handled archive or has no such entry.
pub fn read_entry(path: &Path, file_name: &str) -> Result<Option<Vec<u8>>> {
    if !is_kcol(path) {
        return Ok(None);
    }
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    for e in decrypt_kcol(&bytes)? {
        let base = e.name.replace('\\', "/");
        let base = base.rsplit('/').next().unwrap_or(&base);
        if base.eq_ignore_ascii_case(file_name) {
            return Ok(Some(e.data));
        }
    }
    Ok(None)
}

/// Decrypt a `KCOL` `.pkz` at `path` and write its members into `out_dir`,
/// returning the written relative paths. Reuses the parent module's zip-slip
/// guard so extracted paths can't escape `out_dir`.
fn extract_encrypted(path: &Path, out_dir: &Path) -> Result<Vec<String>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    let entries = decrypt_kcol(&bytes)?;
    std::fs::create_dir_all(out_dir).with_context(|| format!("mkdir {out_dir:?}"))?;
    let mut written = Vec::new();
    for e in &entries {
        // Directory members come through as zero-byte names ending in '/'.
        if e.name.ends_with('/') {
            continue;
        }
        if let Some(dest) = crate::pkz::safe_dest(out_dir, &e.name) {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).with_context(|| format!("mkdir {parent:?}"))?;
            }
            std::fs::write(&dest, &e.data).with_context(|| format!("write {dest:?}"))?;
            written.push(e.name.clone());
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_matches_known_vector() {
        // Classic RC4 test vector (no drop): key "Key", plaintext "Plaintext"
        // → ciphertext BBF316E8D940AF0AD3.
        let mut rc4 = Rc4::new(b"Key", 0);
        let mut buf = b"Plaintext".to_vec();
        rc4.apply(&mut buf);
        assert_eq!(
            buf,
            vec![0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]
        );
    }

    #[test]
    fn rc4_is_symmetric_with_drop() {
        let key = b"secretkey1234567";
        let plain = b"model.edf contents \x00\x01\x02 binary".to_vec();
        let mut enc = plain.clone();
        Rc4::new(key, 256).apply(&mut enc);
        assert_ne!(enc, plain, "encrypted differs");
        let mut dec = enc.clone();
        Rc4::new(key, 256).apply(&mut dec);
        assert_eq!(dec, plain, "same key+drop round-trips");
    }

    /// Synthetic `KCOL`-encrypted `.pkz` (no game content) built with the exact
    /// reverse-engineered scheme; the decryptor must recover both members.
    const FIXTURE_KCOL: &[u8] = include_bytes!("fixtures/test_sidecar.pkz");

    #[test]
    fn decrypts_kcol_fixture() {
        let entries = decrypt_kcol(FIXTURE_KCOL).expect("decrypt KCOL fixture");
        let by_name: std::collections::HashMap<_, _> =
            entries.iter().map(|e| (e.name.as_str(), &e.data)).collect();

        let edf = by_name.get("TestBike/model.edf").expect("model.edf present");
        assert_eq!(&edf[..4], b"EDF\x00");
        let expected_edf: Vec<u8> = b"EDF\x00"
            .iter()
            .copied()
            .chain((0u16..240).map(|b| b as u8).cycle().take(240 * 3))
            .collect();
        assert_eq!(**edf, expected_edf, "deflated payload recovered exactly");

        let cfg = by_name.get("TestBike/hud.cfg").expect("hud.cfg present");
        assert_eq!(**cfg, b"id = testbike\nredline = 13000\n".to_vec());
    }

    /// A creator-locked helmet paint is a single-blob container tens of megabytes wide, so
    /// the checksum runs over the whole file — past `u32::MAX` once the region clears ~16 MB.
    /// Summed into a `u32` that panicked in a debug build, taking every such paint (and the
    /// helmet wearing it) out of the viewer under `tauri dev`. 17 MB of `0xFF` is the
    /// smallest region that crosses the line.
    #[test]
    fn checksum_survives_a_region_past_u32_max() {
        let big = vec![0xFFu8; 17_000_000];
        assert!(
            big.len() as u64 * 255 > u32::MAX as u64,
            "fixture has to overflow a u32 to be testing anything"
        );
        // 17_000_000 × 255 = 4_335_000_000; low 16 bits of that are 0xD9C0.
        assert_eq!(dir_checksum(&big), 0xD9C0);
    }

    #[test]
    fn wrong_footer_is_rejected() {
        let mut bad = FIXTURE_KCOL.to_vec();
        let n = bad.len();
        bad[n - 62] ^= 0xFF; // corrupt the KCOL magic
        assert!(decrypt_kcol(&bad).is_err());
    }

    /// Local-only proof against a real encrypted bike: set `MXB_REAL_PKZ` to a
    /// real `.pkz` path and run `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn decrypts_real_pkz_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read real pkz");
        let entries = decrypt_kcol(&bytes).expect("decrypt real pkz");
        assert!(!entries.is_empty(), "recovered at least one entry");
        eprintln!("decrypted {} entries from {path}", entries.len());
        for e in &entries {
            eprintln!("  {} ({} bytes)", e.name, e.data.len());
        }
    }
}
