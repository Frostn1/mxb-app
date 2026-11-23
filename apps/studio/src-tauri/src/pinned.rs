//! Third-party tools the Studio downloads and then runs, checked against digests pinned here.
//!
//! A digest fetched from the same host as the file only catches a broken download. These are
//! compiled in, so a file that changed upstream (or was swapped) is refused rather than run,
//! and the fix is a Studio update that pins the new one.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// What to say when a download doesn't match: nothing about attacks, just what to do.
pub const CHANGED: &str = "doesn't match the version this Studio was built for, so nothing was installed. \
Update the Studio to get the current one.";

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `Ok` only if `bytes` hash to `want` (lowercase hex).
pub fn check(bytes: &[u8], want: &str, what: &str) -> Result<(), String> {
    if !want.is_empty() && sha256_hex(bytes) == want {
        Ok(())
    } else {
        Err(format!("the {what} download {CHANGED}"))
    }
}

/// The largest single file taken out of a pinned zip. The track tools are around a megabyte.
const MAX_ENTRY: u64 = 64 * 1024 * 1024;

/// Take exactly the `pins` (path inside the zip, SHA-256) out of `zip_bytes` into `dir`.
///
/// Every pinned file has to be there and match before anything is written. Entries that
/// aren't pinned are left in the archive, so nothing unexpected ever lands on disk, and paths
/// come from `pins` rather than the archive.
pub fn extract_pinned(zip_bytes: &[u8], dir: &Path, pins: &[(&str, &str)], what: &str) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| format!("that {what} download isn't a zip: {e}"))?;
    let mut found: Vec<Option<Vec<u8>>> = vec![None; pins.len()];
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("{what}: {e}"))?;
        if !entry.is_file() {
            continue;
        }
        let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else { continue };
        let Some(slot) = pins.iter().position(|(p, _)| p.eq_ignore_ascii_case(&name)) else {
            continue;
        };
        if entry.size() > MAX_ENTRY {
            return Err(format!("the {what} download {CHANGED}"));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        (&mut entry)
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{what}: {e}"))?;
        found[slot] = Some(bytes);
    }
    for ((_, want), got) in pins.iter().zip(&found) {
        match got {
            Some(bytes) if sha256_hex(bytes) == *want => {}
            _ => return Err(format!("the {what} download {CHANGED}")),
        }
    }
    for ((path, _), bytes) in pins.iter().zip(found) {
        let dest = dir.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&dest, bytes.unwrap_or_default()).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for (name, body) in files {
                w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(body).unwrap();
            }
            w.finish().unwrap();
        }
        out.into_inner()
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("studio-pinned-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn every_pin_is_a_sha256() {
        let hex64 = |h: &str| h.len() == 64 && h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'));
        for (path, sha) in crate::TRACK_TOOLS {
            assert!(hex64(sha), "{path}");
        }
        #[cfg(windows)]
        assert!(hex64(crate::replayrec::FFMPEG_SHA256));
        assert!(hex64(crate::winefetch::SHA256));
    }

    #[test]
    fn a_matching_digest_passes_and_anything_else_fails() {
        let want = sha256_hex(b"tool");
        assert!(check(b"tool", &want, "x").is_ok());
        assert!(check(b"tool!", &want, "x").is_err());
        assert!(check(b"tool", "", "x").is_err(), "a missing digest is a failure, not a pass");
    }

    #[test]
    fn only_pinned_files_that_match_are_written() {
        let a = sha256_hex(b"AAA");
        let b = sha256_hex(b"BBB");
        let pins = [("one/a.exe", a.as_str()), ("two/b.fnt", b.as_str())];

        let dir = scratch("ok");
        let zip = zip_of(&[("one/a.exe", b"AAA"), ("two/b.fnt", b"BBB"), ("extra.bat", b"echo")]);
        extract_pinned(&zip, &dir, &pins, "tools").unwrap();
        assert_eq!(std::fs::read(dir.join("one/a.exe")).unwrap(), b"AAA");
        assert_eq!(std::fs::read(dir.join("two/b.fnt")).unwrap(), b"BBB");
        assert!(!dir.join("extra.bat").exists(), "an entry that isn't pinned stays in the archive");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_or_missing_file_writes_nothing() {
        let a = sha256_hex(b"AAA");
        let b = sha256_hex(b"BBB");
        let pins = [("one/a.exe", a.as_str()), ("two/b.fnt", b.as_str())];
        for (label, zip) in [
            ("changed", zip_of(&[("one/a.exe", b"AAA"), ("two/b.fnt", b"EVIL")])),
            ("missing", zip_of(&[("one/a.exe", b"AAA")])),
        ] {
            let dir = scratch(label);
            assert!(extract_pinned(&zip, &dir, &pins, "tools").is_err(), "{label}");
            assert!(!dir.join("one/a.exe").exists(), "{label}: nothing is written on a mismatch");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
