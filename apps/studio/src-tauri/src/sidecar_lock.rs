//! PROPRIETARY — DO NOT COMMIT. This module is gitignored on purpose, and lives in the
//! private repo beside `sidecar.rs`.
//!
//! The writer half of the protected-content format: takes a creator's plaintext files and
//! produces copies bound to a buyer's GUID, which is what "locking" a mod means. The format
//! is the one `sidecar.rs` reads, so the two are inverses and share its footer helpers
//! rather than re-deriving them.
//!
//! Two output shapes, picked by what the input is — the game distinguishes them the same way:
//!
//! * **Single blob** — a loose file (`model.edf`, `helmet.edf`, `gfx.cfg`, every `.pnt`).
//!   The whole file is one encrypted region and the footer's `dirSize` spans all of it.
//! * **Packaged archive** — a `.pkz`, which is a zip. Payloads stay byte-for-byte where they
//!   are; only each entry's 30-byte local header + filename and the trailing central
//!   directory are encrypted. A blob-sealed `.pkz` would decrypt to a zip whose "directory"
//!   started at a local header, so the shape is not interchangeable.
//!
//! Both were validated byte-for-byte against real protected mods: reproducing a shipped
//! locked file exactly from its plaintext plus its own `(key, drop, guid)`, and confirming
//! every payload in a locked `.pkz` inflates to a CRC that matches its directory entry.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Runtime};

use crate::sidecar::{
    dir_checksum, footer_pad, Rc4, KCOL_DROP_BIAS, KCOL_FOOTER_LEN, KCOL_MAGIC,
};

/// A GUID as the game prints it: 18 hex characters.
const GUID_LEN: usize = 18;

/// The reader caps entry names here, so refuse to write one it could never read back.
const MAX_NAME: usize = 4096;

/// Progress channel for a locking run.
pub const LOCK_EVENT: &str = "content-lock://progress";

// ---------------------------------------------------------------------------------------
// GUIDs
// ---------------------------------------------------------------------------------------

/// Validate and normalise a GUID.
///
/// Accepts what the game's own copy button puts on the clipboard, plus the slack people
/// introduce carrying it around: surrounding whitespace, a `0x` prefix, lower case. Anything
/// else is rejected rather than silently locked to a GUID nobody owns — a mis-typed digit
/// produces a file its buyer can't open, and there is no way to tell from the file itself.
pub fn normalize_guid(raw: &str) -> Result<String> {
    let g = raw
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .to_ascii_uppercase();
    if g.len() != GUID_LEN || !g.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("'{}' is not a GUID ({GUID_LEN} hex characters)", raw.trim());
    }
    Ok(g)
}

/// The GUID as the footer stores it: the 18-hex string as 9 bytes, least significant first.
/// All zero means "sealed but bound to nobody", which is how public protected content ships.
fn guid_field(guid: &str) -> [u8; 9] {
    let mut out = [0u8; 9];
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = GUID_LEN - 2 - i * 2;
        *slot = u8::from_str_radix(&guid[hi..hi + 2], 16).unwrap_or(0);
    }
    out
}

// ---------------------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------------------

/// Build the 62-byte trailer: magic, drop base, the obfuscated key/GUID region, and the
/// size and checksum of the encrypted region that precedes it.
fn footer(key: &[u8; 16], drop_base: u32, guid: &str, region_len: u32, checksum: u16) -> Vec<u8> {
    let mut region = [0u8; 48];
    region[..16].copy_from_slice(key);
    region[16..25].copy_from_slice(&guid_field(guid));

    let pad = footer_pad(region_len, checksum);
    let mut out = vec![0u8; KCOL_FOOTER_LEN];
    out[..4].copy_from_slice(KCOL_MAGIC);
    out[4..8].copy_from_slice(&drop_base.to_le_bytes());
    for (i, b) in region.iter().enumerate() {
        let off = 8 + i;
        out[off] = b ^ pad[off % 6];
    }
    out[56..60].copy_from_slice(&region_len.to_le_bytes());
    out[60..62].copy_from_slice(&checksum.to_le_bytes());
    out
}

/// A fresh key and drop for one output file.
///
/// The key ends up inside the file, so its secrecy buys nothing and it does not need a CSPRNG
/// — what it has to be is *different per copy*, so that two buyers diffing their files see
/// noise rather than the shape of the plaintext. Varying the drop alone would do that; a
/// fresh key per copy is simply the stronger version of the same property.
fn fresh_key(seed: &str, nonce: u64) -> ([u8; 16], u32) {
    let mut h = Sha256::new();
    h.update(seed.as_bytes());
    h.update(nonce.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    h.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            .to_le_bytes(),
    );
    let d = h.finalize();
    let mut key = [0u8; 16];
    key.copy_from_slice(&d[..16]);
    // Same order of magnitude as the drops real protected files carry.
    let drop_base = u32::from_le_bytes([d[16], d[17], 0, 0]) & 0x3FF;
    (key, drop_base)
}

// ---------------------------------------------------------------------------------------
// Sealing
// ---------------------------------------------------------------------------------------

/// Seal a whole file as one encrypted region — the form every loose protected file takes.
pub fn seal_blob(plain: &[u8], guid: &str, key: &[u8; 16], drop_base: u32) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(plain.len() + KCOL_FOOTER_LEN);
    buf.extend_from_slice(plain);
    seal_blob_in_place(&mut buf, guid, key, drop_base)?;
    Ok(buf)
}

/// [`seal_blob`] over the buffer the file was read into: ciphered where it lies, trailer
/// pushed into the spare capacity. One allocation and no copy, so a 500 MB file costs its
/// own size in memory rather than three times it. Leave `KCOL_FOOTER_LEN` bytes of headroom
/// (see [`read_with_headroom`]) or the push reallocates and copies the lot.
pub fn seal_blob_in_place(
    buf: &mut Vec<u8>,
    guid: &str,
    key: &[u8; 16],
    drop_base: u32,
) -> Result<()> {
    if buf.len() > u32::MAX as usize {
        bail!("file is larger than the format's 4 GB region limit");
    }
    let region_len = buf.len() as u32;
    let checksum = dir_checksum(&buf[..]);
    Rc4::new(key, drop_base as usize + KCOL_DROP_BIAS).apply(&mut buf[..]);
    buf.extend_from_slice(&footer(key, drop_base, guid, region_len, checksum));
    Ok(())
}

/// Seal a plain `.pkz`.
///
/// The archive is left in place and edited in the two regions the game encrypts, so the
/// payloads never move and never get recompressed — a 500 MB track is copied and XORed, not
/// rebuilt.
pub fn seal_pkz(plain: &[u8], guid: &str, key: &[u8; 16], drop_base: u32) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(plain.len() + KCOL_FOOTER_LEN);
    buf.extend_from_slice(plain);
    seal_pkz_in_place(&mut buf, guid, key, drop_base)?;
    Ok(buf)
}

/// [`seal_pkz`] over the buffer the archive was read into. The directory is copied out
/// first — it is tiny next to the payloads — which is what lets everything else be ciphered
/// where it lies.
pub fn seal_pkz_in_place(
    buf: &mut Vec<u8>,
    guid: &str,
    key: &[u8; 16],
    drop_base: u32,
) -> Result<()> {
    if buf.len() > u32::MAX as usize {
        bail!("archive is larger than the format's 4 GB limit");
    }
    let (cd_off, cd_size, count) = find_eocd(&buf[..])?;
    let (cd_off, cd_size) = (cd_off as usize, cd_size as usize);
    if cd_off + cd_size > buf.len() {
        bail!("central directory runs past the end of the archive");
    }

    let drop = drop_base as usize + KCOL_DROP_BIAS;

    // One keystream prefix, reused from the top for every entry — the reader does the same,
    // and an entry's header is only ever decrypted with its first bytes.
    let ks = Rc4::new(key, drop).keystream(30 + MAX_NAME);

    let cd = buf[cd_off..cd_off + cd_size].to_vec();
    let mut off = 0usize;
    let mut seen = 0u32;
    while off + 46 <= cd.len() && &cd[off..off + 4] == b"PK\x01\x02" {
        let nl = u16le(&cd, off + 28) as usize;
        let el = u16le(&cd, off + 30) as usize;
        let cl = u16le(&cd, off + 32) as usize;
        let lho = u32le(&cd, off + 42) as usize;
        if lho + 30 > buf.len() || &buf[lho..lho + 4] != b"PK\x03\x04" {
            bail!("entry {seen} has no local header at {lho}");
        }
        let lnl = u16le(&buf[..], lho + 26) as usize;
        if lnl > MAX_NAME {
            bail!("entry name length {lnl} exceeds what the format can carry");
        }
        // Sealing in place means a header that overlapped the directory would corrupt what
        // we are still reading. No real archive does; refuse rather than write nonsense.
        if lho + 30 + lnl > cd_off {
            bail!("entry {seen} header runs into the central directory");
        }
        for i in 0..30 + lnl {
            buf[lho + i] ^= ks[i];
        }
        off += 46 + nl + el + cl;
        seen += 1;
    }
    if seen != count {
        bail!("central directory lists {count} entries but {seen} parsed");
    }

    // Everything from the directory to EOF is the second region: directory, end record and
    // any trailing comment.
    let region_len = buf.len() - cd_off;
    let checksum = dir_checksum(&buf[cd_off..]);
    Rc4::new(key, drop).apply(&mut buf[cd_off..]);
    buf.extend_from_slice(&footer(
        key,
        drop_base,
        guid,
        region_len as u32,
        checksum,
    ));
    Ok(())
}

/// Locate the end-of-central-directory record: `(offset, size, entry count)` of the directory.
fn find_eocd(bytes: &[u8]) -> Result<(u32, u32, u32)> {
    if bytes.len() < 22 {
        bail!("too small to be an archive");
    }
    // The record is 22 bytes plus a comment of at most 64 KB.
    let floor = bytes.len().saturating_sub(22 + u16::MAX as usize);
    let mut at = None;
    for i in (floor..=bytes.len() - 22).rev() {
        if &bytes[i..i + 4] == b"PK\x05\x06" {
            at = Some(i);
            break;
        }
    }
    let Some(at) = at else {
        bail!("not a zip archive (no end-of-directory record)");
    };
    let count = u16le(bytes, at + 10) as u32;
    let size = u32le(bytes, at + 12);
    let off = u32le(bytes, at + 16);
    if off == u32::MAX || size == u32::MAX || count == u16::MAX as u32 {
        bail!("zip64 archives are not a shape the game reads");
    }
    Ok((off, size, count))
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Whether these bytes already carry the format's trailer.
fn already_sealed(bytes: &[u8]) -> bool {
    bytes.len() >= KCOL_FOOTER_LEN && &bytes[bytes.len() - KCOL_FOOTER_LEN..][..4] == KCOL_MAGIC
}

// ---------------------------------------------------------------------------------------
// Planning and running
// ---------------------------------------------------------------------------------------

/// One file a run would produce, or a reason it wouldn't.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LockItem {
    /// Where the file lands under each GUID folder — its path relative to the parent of the
    /// selection it came from, so picking a folder keeps the folder.
    pub rel: String,
    pub abs: String,
    pub bytes: u64,
    /// `"archive"` for a `.pkz`, `"file"` for everything else.
    pub kind: &'static str,
    /// Set when the file will be left alone.
    pub skip: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockOutcome {
    pub guids: usize,
    pub files: usize,
    pub written: usize,
    pub skipped: usize,
    pub bytes: u64,
    pub out_dir: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LockProgress {
    done: usize,
    total: usize,
    guid: String,
    file: String,
}

/// Files no creator means to ship, and that would look like a mistake in a buyer's folder.
const JUNK: [&str; 4] = [".DS_Store", "Thumbs.db", "desktop.ini", ".gitignore"];

/// Expand a selection into the files a run would touch, in the order it would touch them.
///
/// Reads the head and tail of each file rather than all of it: the only questions here are
/// "is this already protected" and "how big is it", and the answers must not cost a pass over
/// half a gigabyte per candidate.
pub fn plan(roots: &[PathBuf]) -> Result<Vec<LockItem>> {
    let mut out = Vec::new();
    for root in roots {
        let base = root.parent().unwrap_or(Path::new(""));
        if root.is_file() {
            out.push(item(root, base)?);
            continue;
        }
        for e in walkdir::WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            out.push(item(e.path(), base)?);
        }
    }
    Ok(out)
}

fn item(path: &Path, base: &Path) -> Result<LockItem> {
    let rel = path
        .strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let kind = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pkz"))
    {
        "archive"
    } else {
        "file"
    };

    let skip = if name.as_deref().is_some_and(|n| JUNK.contains(&n)) {
        Some("junk")
    } else if bytes == 0 {
        Some("empty")
    } else if is_sealed_on_disk(path) {
        Some("protected")
    } else {
        None
    };
    Ok(LockItem {
        rel,
        abs: path.to_string_lossy().into_owned(),
        bytes,
        kind,
        skip,
    })
}

/// Trailer check that reads 62 bytes, not the file.
fn is_sealed_on_disk(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    if f.seek(SeekFrom::End(-(KCOL_FOOTER_LEN as i64))).is_err() {
        return false;
    }
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).is_ok() && &magic == KCOL_MAGIC
}

/// One file in flight per worker, so the fleet is sized to keep peak memory here rather
/// than at cores × the biggest file. A 500 MB track therefore runs 4 wide, not 10.
const IN_FLIGHT_BUDGET: u64 = 2 * 1024 * 1024 * 1024;

/// Read a file with room for the trailer, so sealing it never has to reallocate and copy.
fn read_with_headroom(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0) as usize;
    // The spare byte is what lets `read_to_end` see EOF without growing the buffer.
    let mut buf = Vec::with_capacity(len + KCOL_FOOTER_LEN + 1);
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Lock every file in `roots` to every GUID in `guids`, under `out_dir/<GUID>/…`.
///
/// Never writes over the source: a creator's plaintext is the thing they can't get back, and
/// a locker that consumed it would be a one-way door. Each GUID gets its own copy with its
/// own key, so the copies differ.
///
/// Every (buyer, file) pair is independent — own key, own stream, own output — so the job is
/// one flat work list run in parallel. RC4 is a serial byte-at-a-time cipher at about
/// 1 GB/s per core and there is no faster way through a file, so the only lever is doing
/// several at once.
pub fn run<R: Runtime>(
    app: &AppHandle<R>,
    roots: &[PathBuf],
    guids: &[String],
    out_dir: &Path,
) -> Result<LockOutcome> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    let guids: Vec<String> = guids
        .iter()
        .map(|g| normalize_guid(g))
        .collect::<Result<_>>()?;
    if guids.is_empty() {
        bail!("no GUID to lock to");
    }
    let items = plan(roots)?;
    let todo: Vec<&LockItem> = items.iter().filter(|i| i.skip.is_none()).collect();
    if todo.is_empty() {
        bail!("nothing to lock — every file was already protected, empty or skipped");
    }

    let work: Vec<(usize, &String, &LockItem)> = guids
        .iter()
        .flat_map(|g| todo.iter().map(move |it| (g, *it)))
        .enumerate()
        .map(|(n, (g, it))| (n, g, it))
        .collect();

    let total = work.len();
    let done = AtomicUsize::new(0);
    let bytes = AtomicU64::new(0);

    let biggest = todo.iter().map(|i| i.bytes).max().unwrap_or(1).max(1);
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let workers = cores
        .min((IN_FLIGHT_BUDGET / biggest).max(1) as usize)
        .min(total);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .context("start the locking workers")?;

    pool.install(|| {
        work.par_iter().try_for_each(|(n, guid, it)| -> Result<()> {
            let mut buf =
                read_with_headroom(Path::new(&it.abs)).with_context(|| format!("read {}", it.rel))?;
            // A file can be sealed between planning and here; the plan's answer is advisory.
            if already_sealed(&buf) {
                done.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
            // Keyed off the work index, not a running counter: the same job must produce the
            // same set of keys whichever order the workers happen to finish in.
            let (key, drop_base) = fresh_key(&format!("{guid}/{}", it.rel), *n as u64);
            if it.kind == "archive" {
                seal_pkz_in_place(&mut buf, guid, &key, drop_base)
                    .with_context(|| format!("lock archive {}", it.rel))?;
            } else {
                seal_blob_in_place(&mut buf, guid, &key, drop_base)
                    .with_context(|| format!("lock {}", it.rel))?;
            }

            let dest = out_dir.join(guid.as_str()).join(&it.rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).with_context(|| format!("mkdir {parent:?}"))?;
            }
            std::fs::write(&dest, &buf).with_context(|| format!("write {dest:?}"))?;

            bytes.fetch_add(buf.len() as u64, Ordering::Relaxed);
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(
                LOCK_EVENT,
                LockProgress {
                    done: d,
                    total,
                    guid: (*guid).clone(),
                    file: it.rel.clone(),
                },
            );
            Ok(())
        })
    })?;

    Ok(LockOutcome {
        guids: guids.len(),
        files: todo.len(),
        written: done.load(Ordering::Relaxed),
        skipped: items.len() - todo.len(),
        bytes: bytes.load(Ordering::Relaxed),
        out_dir: out_dir.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Recover a sealed file's `(key, drop base, GUID)` from its trailer — the inverse of
    /// [`footer`], and only needed to prove the two are inverses.
    pub(super) fn parse_footer(sealed: &[u8]) -> ([u8; 16], u32, String) {
        let f = &sealed[sealed.len() - KCOL_FOOTER_LEN..];
        assert_eq!(&f[..4], KCOL_MAGIC, "sealed file carries the trailer");
        let drop_base = u32::from_le_bytes([f[4], f[5], f[6], f[7]]);
        let region_len = u32::from_le_bytes([f[56], f[57], f[58], f[59]]);
        let checksum = u16::from_le_bytes([f[60], f[61]]);
        let pad = footer_pad(region_len, checksum);
        let plain: Vec<u8> = (8..56).map(|o| f[o] ^ pad[o % 6]).collect();
        let mut key = [0u8; 16];
        key.copy_from_slice(&plain[..16]);
        let guid: String = plain[16..25]
            .iter()
            .rev()
            .map(|b| format!("{b:02X}"))
            .collect();
        (key, drop_base, guid)
    }

    const GUID: &str = "FF0110000108D7CFE3";

    #[test]
    fn guid_is_stored_least_significant_byte_first() {
        // The ordering a real protected file uses; getting it backwards produces a file
        // bound to a GUID that doesn't exist, which nothing downstream could catch.
        assert_eq!(
            guid_field(GUID),
            [0xE3, 0xCF, 0xD7, 0x08, 0x01, 0x00, 0x10, 0x01, 0xFF]
        );
    }

    #[test]
    fn normalizes_the_shapes_people_paste() {
        assert_eq!(normalize_guid("  ff0110000108d7cfe3 ").unwrap(), GUID);
        assert_eq!(normalize_guid("0xFF0110000108D7CFE3").unwrap(), GUID);
        assert!(normalize_guid("FF0110000108D7CFE").is_err(), "too short");
        assert!(normalize_guid("FF0110000108D7CFE33").is_err(), "too long");
        assert!(normalize_guid("FF0110000108D7CFEZ").is_err(), "not hex");
        assert!(normalize_guid("").is_err());
    }

    /// A loose file seals as one region, and the reader gets the bytes back untouched.
    #[test]
    fn sealed_blob_round_trips_through_the_reader() {
        let plain: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let (key, drop) = fresh_key("model.edf", 0);
        let sealed = seal_blob(&plain, GUID, &key, drop).unwrap();

        assert!(already_sealed(&sealed));
        assert_eq!(sealed.len(), plain.len() + KCOL_FOOTER_LEN);
        assert_ne!(&sealed[..plain.len()], &plain[..], "payload is encrypted");
        assert_eq!(
            crate::sidecar::read_blob(&sealed).unwrap(),
            plain,
            "reader recovers the plaintext exactly"
        );
        let (rk, rd, rg) = parse_footer(&sealed);
        assert_eq!((rk, rd, rg.as_str()), (key, drop, GUID));
    }

    /// Two copies of the same file for two buyers must not be the same bytes — otherwise a
    /// pair of buyers can tell they hold identical files, and a leak names nobody.
    #[test]
    fn each_copy_gets_its_own_key() {
        let plain = b"same source file".to_vec();
        let a = seal_blob(&plain, GUID, &fresh_key("x", 1).0, 1).unwrap();
        let b = seal_blob(&plain, GUID, &fresh_key("x", 2).0, 2).unwrap();
        assert_ne!(a, b);
        assert_eq!(crate::sidecar::read_blob(&a).unwrap(), plain);
        assert_eq!(crate::sidecar::read_blob(&b).unwrap(), plain);
    }

    pub(super) fn plain_pkz() -> Vec<u8> {
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut cur);
            let deflated = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file::<_, ()>("TestBike/model.edf", deflated).unwrap();
            let edf: Vec<u8> = b"EDF\x00"
                .iter()
                .copied()
                .chain((0..720u32).map(|i| (i % 240) as u8))
                .collect();
            w.write_all(&edf).unwrap();
            w.start_file::<_, ()>("TestBike/hud.cfg", stored).unwrap();
            w.write_all(b"id = testbike\nredline = 13000\n").unwrap();
            w.finish().unwrap();
        }
        cur.into_inner()
    }

    /// A `.pkz` seals as an archive: payloads stay put, headers and directory are encrypted,
    /// and the reader walks it like any protected archive.
    #[test]
    fn sealed_pkz_round_trips_through_the_reader() {
        let plain = plain_pkz();
        let (key, drop) = fresh_key("bike.pkz", 0);
        let sealed = seal_pkz(&plain, GUID, &key, drop).unwrap();

        assert_eq!(sealed.len(), plain.len() + KCOL_FOOTER_LEN);
        assert_ne!(
            &sealed[..plain.len()],
            &plain[..],
            "headers and directory are encrypted"
        );
        let entries = crate::sidecar::decrypt_kcol(&sealed).expect("reader opens it");
        let by_name: std::collections::HashMap<_, _> =
            entries.iter().map(|e| (e.name.as_str(), &e.data)).collect();
        assert_eq!(
            **by_name.get("TestBike/hud.cfg").expect("hud.cfg"),
            b"id = testbike\nredline = 13000\n".to_vec()
        );
        assert_eq!(
            &by_name.get("TestBike/model.edf").expect("model.edf")[..4],
            b"EDF\x00"
        );
        assert_eq!(parse_footer(&sealed).2, GUID);
    }

    /// The payload bytes are the creator's, compressed exactly once. A locked archive that
    /// re-deflated them would be a different file to the game's checksums.
    #[test]
    fn sealing_an_archive_leaves_the_payloads_alone() {
        let plain = plain_pkz();
        let sealed = seal_pkz(&plain, GUID, &fresh_key("p", 0).0, 7).unwrap();
        // The stored entry's bytes appear verbatim in both, at the same offset.
        let needle = b"id = testbike";
        let at = plain
            .windows(needle.len())
            .position(|w| w == needle)
            .expect("plaintext holds the stored payload");
        assert_eq!(&sealed[at..at + needle.len()], needle);
    }

    #[test]
    fn refuses_what_it_cannot_read_back() {
        assert!(seal_pkz(b"not a zip at all, no directory here", GUID, &[0; 16], 0).is_err());
    }

    #[test]
    fn planning_flags_what_it_would_skip() {
        let dir = std::env::temp_dir().join(format!("mxb-lock-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("MyBike")).unwrap();
        std::fs::write(dir.join("MyBike/model.edf"), b"plaintext model").unwrap();
        std::fs::write(dir.join("MyBike/empty.pnt"), b"").unwrap();
        std::fs::write(dir.join("MyBike/.DS_Store"), b"junk").unwrap();
        let sealed = seal_blob(b"already done", GUID, &[3; 16], 4).unwrap();
        std::fs::write(dir.join("MyBike/locked.pnt"), &sealed).unwrap();

        let items = plan(&[dir.join("MyBike")]).unwrap();
        let by_rel: std::collections::HashMap<_, _> =
            items.iter().map(|i| (i.rel.as_str(), i)).collect();
        // Relative to the parent of the selection, so picking a folder keeps the folder.
        assert_eq!(by_rel["MyBike/model.edf"].skip, None);
        assert_eq!(by_rel["MyBike/empty.pnt"].skip, Some("empty"));
        assert_eq!(by_rel["MyBike/.DS_Store"].skip, Some("junk"));
        assert_eq!(by_rel["MyBike/locked.pnt"].skip, Some("protected"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The proof that matters, against a mod somebody actually shipped: re-seal the creator's
    /// plaintext with the very key, drop and GUID their locked copy carries, and the bytes
    /// must come out identical. Point `MXB_SEALED`/`MXB_PLAIN` at such a pair and run
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reseals_a_real_file_byte_for_byte() {
        let (Ok(sealed_p), Ok(plain_p)) = (std::env::var("MXB_SEALED"), std::env::var("MXB_PLAIN"))
        else {
            eprintln!("set MXB_SEALED and MXB_PLAIN to run");
            return;
        };
        let sealed = std::fs::read(&sealed_p).expect("read sealed");
        let plain = std::fs::read(&plain_p).expect("read plaintext");
        let (key, drop, guid) = parse_footer(&sealed);
        eprintln!("{sealed_p}: guid {guid}, drop base {drop:#x}");
        let mine = if plain.starts_with(b"PK\x03\x04") {
            seal_pkz(&plain, &guid, &key, drop).expect("seal archive")
        } else {
            seal_blob(&plain, &guid, &key, drop).expect("seal blob")
        };
        assert_eq!(mine.len(), sealed.len(), "same length");
        assert!(mine == sealed, "reproduced the shipped file exactly");
    }
}

#[cfg(test)]
mod run_tests {
    use super::tests::*;
    use super::*;

    /// Tauri's mock runtime — enough of an app to emit progress into, no display.
    pub(super) fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app builds")
    }

    const A: &str = "FF0110000108D7CFE3";
    const B: &str = "FF01100001730AF2F1";

    /// The whole job, end to end: a mod folder in, a folder per buyer out, every file
    /// readable again and the creator's originals untouched.
    #[test]
    fn locks_a_mod_folder_for_two_buyers() {
        let root = std::env::temp_dir().join(format!("mxb-lock-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("MyBike");
        std::fs::create_dir_all(src.join("paints")).unwrap();

        let edf: Vec<u8> = (0..40_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(src.join("model.edf"), &edf).unwrap();
        std::fs::write(src.join("paints/red.pnt"), b"PNT\x00red").unwrap();
        std::fs::write(src.join(".DS_Store"), b"junk").unwrap();
        let pkz = plain_pkz();
        std::fs::write(src.join("bike.pkz"), &pkz).unwrap();

        let out = root.join("out");
        let app = mock_app();
        let outcome = run(
            app.handle(),
            &[src.clone()],
            &[A.to_string(), b_lower()],
            &out,
        )
        .expect("run");

        assert_eq!(outcome.guids, 2);
        assert_eq!(outcome.files, 3, "three files, the junk one skipped");
        assert_eq!(outcome.skipped, 1);
        assert_eq!(outcome.written, 6, "every file for every buyer");

        for guid in [A, B] {
            let dir = out.join(guid);
            assert!(!dir.join("MyBike/.DS_Store").exists(), "junk stays behind");

            let sealed = std::fs::read(dir.join("MyBike/model.edf")).unwrap();
            assert_eq!(
                crate::sidecar::read_blob(&sealed).unwrap(),
                edf,
                "the buyer's copy opens to the creator's bytes"
            );
            assert_eq!(parse_footer(&sealed).2, guid, "bound to this buyer");

            let paint = std::fs::read(dir.join("MyBike/paints/red.pnt")).unwrap();
            assert_eq!(crate::sidecar::read_blob(&paint).unwrap(), b"PNT\x00red");

            let arc = std::fs::read(dir.join("MyBike/bike.pkz")).unwrap();
            let names: Vec<String> = crate::sidecar::decrypt_kcol(&arc)
                .expect("archive opens")
                .into_iter()
                .map(|e| e.name)
                .collect();
            assert!(names.contains(&"TestBike/hud.cfg".to_string()), "{names:?}");
        }

        // Two buyers holding the same file must not hold the same bytes.
        assert_ne!(
            std::fs::read(out.join(A).join("MyBike/model.edf")).unwrap(),
            std::fs::read(out.join(B).join("MyBike/model.edf")).unwrap(),
        );
        // And the creator still has their plaintext.
        assert_eq!(std::fs::read(src.join("model.edf")).unwrap(), edf);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The GUIDs are normalised on the way in, so a pasted lower-case one still names the
    /// same folder as the same GUID typed in upper case.
    fn b_lower() -> String {
        B.to_ascii_lowercase()
    }

    #[test]
    fn refuses_a_run_with_nothing_to_do() {
        let root = std::env::temp_dir().join(format!("mxb-lock-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".DS_Store"), b"junk").unwrap();
        let app = mock_app();
        assert!(run(app.handle(), &[root.clone()], &[A.to_string()], &root).is_err());
        // And a GUID nobody could own is refused before anything is written.
        std::fs::write(root.join("model.edf"), b"real content").unwrap();
        assert!(run(app.handle(), &[root.clone()], &["nope".into()], &root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    // -----------------------------------------------------------------------------------
    // Stopwatch, not a correctness test. What the locker costs per byte, and where the time
    // goes. Run with: cargo test --release -- --ignored --nocapture locking_speed
    // -----------------------------------------------------------------------------------

    const MB: usize = 1 << 20;

    /// Cheap LCG fill — real bytes, so nothing folds away.
    fn filler(n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n);
        let mut s: u32 = 0x1234_5678;
        for _ in 0..n {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            v.push((s >> 24) as u8);
        }
        v
    }

    fn mbps(bytes: usize, secs: f64) -> f64 {
        (bytes as f64 / MB as f64) / secs
    }

    fn time<T>(reps: u32, mut f: impl FnMut() -> T) -> f64 {
        let start = std::time::Instant::now();
        for _ in 0..reps {
            std::hint::black_box(f());
        }
        start.elapsed().as_secs_f64() / reps as f64
    }

    /// A stored (uncompressed) archive of roughly `n` bytes across `entries` files — the
    /// shape a real `.pkz` has, where the payloads dominate the size.
    fn big_pkz(n: usize, entries: usize) -> Vec<u8> {
        use std::io::Write as _;
        let each = n / entries;
        let chunk = filler(each);
        let mut cur = std::io::Cursor::new(Vec::with_capacity(n + 64 * entries));
        {
            let mut w = zip::ZipWriter::new(&mut cur);
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for i in 0..entries {
                w.start_file::<_, ()>(format!("Bike/part{i:04}.edf"), stored)
                    .unwrap();
                w.write_all(&chunk).unwrap();
            }
            w.finish().unwrap();
        }
        cur.into_inner()
    }

    #[test]
    #[ignore]
    fn locking_speed_by_file_size() {
        let sizes: Vec<usize> = vec![MB, 4 * MB, 16 * MB, 64 * MB, 256 * MB, 512 * MB];
        let key = [7u8; 16];

        eprintln!("\n== loose file: seal_blob (checksum + copy + RC4) ==");
        eprintln!(
            "{:>8} {:>10} {:>9} | {:>9} {:>9} {:>9}",
            "size", "total", "MB/s", "checksum", "copy", "rc4"
        );
        for &n in &sizes {
            let plain = filler(n);
            let reps = if n <= 16 * MB { 5 } else { 2 };
            let total = time(reps, || seal_blob(&plain, A, &key, 3).unwrap());
            let ck = time(reps, || dir_checksum(&plain));
            let cp = time(reps, || plain.to_vec());
            let cp_rc4 = time(reps, || {
                let mut b = plain.to_vec();
                Rc4::new(&key, 3 + KCOL_DROP_BIAS).apply(&mut b);
                b
            });
            eprintln!(
                "{:>7}M {:>9.1}ms {:>9.0} | {:>8.1}ms {:>8.1}ms {:>8.1}ms",
                n / MB,
                total * 1e3,
                mbps(n, total),
                ck * 1e3,
                cp * 1e3,
                (cp_rc4 - cp) * 1e3
            );
        }

        eprintln!("\n== archive: seal_pkz (headers + directory only) ==");
        eprintln!("{:>8} {:>8} {:>10} {:>9}", "size", "entries", "total", "MB/s");
        for &n in &[16 * MB, 64 * MB, 256 * MB, 512 * MB] {
            for &entries in &[16usize, 512] {
                let plain = big_pkz(n, entries);
                let len = plain.len();
                let reps = if n <= 64 * MB { 5 } else { 2 };
                let t = time(reps, || seal_pkz(&plain, A, &key, 3).unwrap());
                eprintln!(
                    "{:>7}M {:>8} {:>9.1}ms {:>9.0}",
                    len / MB,
                    entries,
                    t * 1e3,
                    mbps(len, t)
                );
            }
        }
    }

    /// The number a creator actually waits on: read from disk, seal, write the copy.
    #[test]
    #[ignore]
    fn locking_speed_end_to_end() {
        let root = std::env::temp_dir().join(format!("mxb-lock-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = mock_app();

        eprintln!("\n== end to end: run() — read + seal + write, one GUID ==");
        eprintln!("{:>8} {:>7} {:>10} {:>9}", "size", "files", "total", "MB/s");
        // One big file, then the same bytes split many ways, then a whole-mod shape.
        let cases: Vec<(usize, usize)> = vec![
            (4 * MB, 1),
            (64 * MB, 1),
            (256 * MB, 1),
            (512 * MB, 1),
            (64 * MB, 256),
            (16 * MB, 1024),
        ];
        for (total_bytes, files) in cases {
            let src = root.join(format!("src-{total_bytes}-{files}"));
            let out = root.join(format!("out-{total_bytes}-{files}"));
            std::fs::create_dir_all(&src).unwrap();
            let chunk = filler(total_bytes / files);
            for i in 0..files {
                std::fs::write(src.join(format!("part{i:05}.edf")), &chunk).unwrap();
            }
            let t = std::time::Instant::now();
            let o = run(app.handle(), &[src.clone()], &[A.to_string()], &out).unwrap();
            let secs = t.elapsed().as_secs_f64();
            eprintln!(
                "{:>7}M {:>7} {:>9.1}ms {:>9.0}",
                total_bytes / MB,
                files,
                secs * 1e3,
                mbps(o.bytes as usize, secs)
            );
            let _ = std::fs::remove_dir_all(&src);
            let _ = std::fs::remove_dir_all(&out);
        }

        // A real mod ships as a .pkz, and that path only rewrites headers and directory.
        eprintln!("\n== end to end: a .pkz (payloads stay put) ==");
        for &n in &[64 * MB, 512 * MB] {
            let src = root.join(format!("pkz-{n}"));
            std::fs::create_dir_all(&src).unwrap();
            let bytes = big_pkz(n, 512);
            let len = bytes.len();
            std::fs::write(src.join("bike.pkz"), &bytes).unwrap();
            drop(bytes);
            let out = root.join(format!("pkzout-{n}"));
            let t = std::time::Instant::now();
            let o = run(app.handle(), &[src.clone()], &[A.to_string()], &out).unwrap();
            let secs = t.elapsed().as_secs_f64();
            eprintln!(
                "{:>7}M {:>9.1}ms {:>9.0} MB/s",
                len / MB,
                secs * 1e3,
                mbps(o.bytes as usize, secs)
            );
            let _ = std::fs::remove_dir_all(&src);
            let _ = std::fs::remove_dir_all(&out);
        }

        // Every extra buyer is another full pass: its own key, its own copy.
        let src = root.join("src-guids");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("model.edf"), filler(64 * MB)).unwrap();
        eprintln!("\n== the per-GUID multiplier (64M, one file) ==");
        for guids in [1usize, 2, 4] {
            let ids: Vec<String> = [A, B, "FF0110000108D7CF01", "FF0110000108D7CF02"]
                .iter()
                .take(guids)
                .map(|s| s.to_string())
                .collect();
            let out = root.join(format!("out-g{guids}"));
            let t = std::time::Instant::now();
            let o = run(app.handle(), &[src.clone()], &ids, &out).unwrap();
            let secs = t.elapsed().as_secs_f64();
            eprintln!(
                "{guids} GUID(s): {:>7.1}ms  {:>6.0} MB/s over {} MB written",
                secs * 1e3,
                mbps(o.bytes as usize, secs),
                o.bytes / MB as u64
            );
            let _ = std::fs::remove_dir_all(&out);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Where the wall-clock actually goes for one 200 MB loose file: every phase of
    /// `run()` timed on its own, plus the copy the footer's `extend` forces.
    #[test]
    #[ignore]
    fn locking_speed_where_the_time_goes() {
        let n = 200 * MB;
        let plain = filler(n);
        let root = std::env::temp_dir().join(format!("mxb-lock-phase-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join("model.edf");
        std::fs::write(&file, &plain).unwrap();
        let dest = root.join("out.edf");

        let key = [7u8; 16];
        let sealed = seal_blob(&plain, A, &key, 3).unwrap();
        let reps = 3;

        let read = time(reps, || std::fs::read(&file).unwrap());
        let ck = time(reps, || dir_checksum(&plain));
        let cp = time(reps, || plain.to_vec());
        let cp_grow = time(reps, || {
            let mut v = plain.to_vec();
            v.extend_from_slice(&[0u8; KCOL_FOOTER_LEN]);
            v
        });
        let cp_cap = time(reps, || {
            let mut v = Vec::with_capacity(n + KCOL_FOOTER_LEN);
            v.extend_from_slice(&plain);
            v.extend_from_slice(&[0u8; KCOL_FOOTER_LEN]);
            v
        });
        let rc4 = time(reps, || {
            let mut b = plain.clone();
            Rc4::new(&key, 3 + KCOL_DROP_BIAS).apply(&mut b);
            b
        }) - cp;
        let seal = time(reps, || seal_blob(&plain, A, &key, 3).unwrap());
        let write = time(reps, || std::fs::write(&dest, &sealed).unwrap());
        let write_sync = time(reps, || {
            use std::io::Write as _;
            let mut f = std::fs::File::create(&dest).unwrap();
            f.write_all(&sealed).unwrap();
            f.sync_all().unwrap();
        });

        eprintln!("\n== one 200 MB loose file, phase by phase ==");
        for (what, secs) in [
            ("fs::read (warm cache)", read),
            ("dir_checksum", ck),
            ("to_vec (the copy)", cp),
            ("  + extend footer (realloc)", cp_grow),
            ("  with_capacity instead", cp_cap),
            ("RC4 over the copy", rc4),
            ("seal_blob (all of the above)", seal),
            ("fs::write", write),
            ("fs::write + fsync", write_sync),
        ] {
            eprintln!("{what:>30}  {:>8.1}ms  {:>7.0} MB/s", secs * 1e3, mbps(n, secs));
        }

        let app = mock_app();
        let out = root.join("out");
        let t = std::time::Instant::now();
        let o = run(app.handle(), &[src.clone()], &[A.to_string()], &out).unwrap();
        let secs = t.elapsed().as_secs_f64();
        eprintln!(
            "{:>30}  {:>8.1}ms  {:>7.0} MB/s",
            "run() end to end",
            secs * 1e3,
            mbps(o.bytes as usize, secs)
        );
        eprintln!(
            "{:>30}  {:>8.1}ms",
            "read + seal + write",
            (read + seal + write) * 1e3
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
