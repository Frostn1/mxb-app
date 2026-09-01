//! Notice when the mods folder isn't really on disk.
//!
//! OneDrive, Dropbox and iCloud all offer the same trick: a file that looks completely
//! ordinary in Explorer — right name, right size — while its bytes still live on a server.
//! Windows calls these placeholders, and reading one is supposed to be transparent: the
//! filter driver fetches the content and the read succeeds, a little late.
//!
//! "A little late" is the problem. MX Bikes reads the mods tree during the load screen,
//! memory-mapped and on the critical path, and a placeholder whose fetch is slow, offline
//! or refused surfaces there as a failed read of a mapped page — `STATUS_IN_PAGE_ERROR`,
//! which is a crash, not an error message. From the player's side the game "just crashes on
//! the loading screen", with nothing in any log to say why, and it recurs for as long as
//! the file stays evicted.
//!
//! The app is already watching this folder ([`crate::modwatch`]) and already knows when a
//! session starts ([`crate::sessionwatch`]), so it is in a position to answer the question
//! before the crash rather than after: are these files actually here?
//!
//! Note what this deliberately does **not** do. It never opens a placeholder, because
//! opening one is what triggers the download — a scan that hydrated the folder would turn a
//! diagnostic into a multi-gigabyte surprise. It only ever asks for attributes, which the
//! filter driver answers from metadata it already has.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Event name the UI listens on.
pub const EVENT: &str = "mods-dehydrated";

/// Only content files matter. A placeholder `readme.txt` is nobody's crash.
const CONTENT_EXTENSIONS: [&str; 4] = ["pkz", "pnt", "edf", "sav"];

/// Stop walking after this many files. A mods tree in the thousands is normal and the
/// answer does not get truer past this point — one placeholder is already the whole story.
const MAX_FILES: usize = 20_000;

/// How deep to walk. Deep enough for `mods/tracks/<name>/<files>` and a level of slack.
const MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dehydrated {
    /// How many content files are placeholders rather than real bytes.
    pub count: usize,
    /// How many content files were looked at, so `count` has a denominator.
    pub scanned: usize,
    /// A few names, to make the warning concrete rather than a number.
    pub examples: Vec<String>,
    /// Whether the tree sits under a recognisable sync root, which changes the advice.
    pub provider: Option<String>,
}

/// Check the mods tree in the background and warn if any of it isn't really there.
///
/// Fire-and-forget by design: this runs as the game is starting, and must not add a step
/// to the Play button under any circumstance.
pub fn warn_if_dehydrated(app: &AppHandle, cfg: &crate::config::AppConfig) {
    let root = crate::library::mods_root(&cfg.mods_path);
    let app = app.clone();
    std::thread::spawn(move || {
        let found = scan(&root);
        if found.count == 0 {
            return;
        }
        let provider = found.provider.clone().unwrap_or_else(|| "a cloud sync tool".into());
        log::warn!(
            "[cloud] {} of {} mod file(s) under {} are placeholders, not real files — {provider} \
             has evicted them. The game reads these during the load screen and can crash there \
             (in-page error). Fix: right-click the folder in Explorer and choose \
             \"Always keep on this device\", or move the folder out of {provider}. Examples: {}",
            found.count,
            found.scanned,
            root.display(),
            found.examples.join(", ")
        );
        let _ = app.emit(EVENT, &found);
    });
}

/// Walk `root` and count content files that are placeholders.
fn scan(root: &std::path::Path) -> Dehydrated {
    let mut out = Dehydrated { provider: provider_of(root), ..Default::default() };
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH || out.scanned >= MAX_FILES {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if out.scanned >= MAX_FILES {
                break;
            }
            let path = entry.path();
            // `file_type` reads the directory entry we already have — it does not open
            // the file, so it cannot trigger a download.
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            if !is_content(&path) {
                continue;
            }
            out.scanned += 1;
            if !is_placeholder(&path) {
                continue;
            }
            out.count += 1;
            if out.examples.len() < 5 {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    out.examples.push(name.to_string());
                }
            }
        }
    }
    out
}

fn is_content(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| CONTENT_EXTENSIONS.contains(&e.as_str()))
}

/// Name the sync tool from the path, so the advice can name it too. `None` when the folder
/// is somewhere unrecognised — a placeholder there is still a placeholder.
fn provider_of(root: &std::path::Path) -> Option<String> {
    let lower = root.to_string_lossy().to_ascii_lowercase();
    for (needle, name) in [
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("google drive", "Google Drive"),
        ("icloud", "iCloud Drive"),
        ("creative cloud", "Creative Cloud"),
    ] {
        if lower.contains(needle) {
            return Some(name.to_string());
        }
    }
    None
}

/// Is this file a placeholder rather than real bytes?
///
/// Three attributes, because the providers do not agree on one:
///
///   * `RECALL_ON_DATA_ACCESS` — the modern per-file placeholder (OneDrive Files On-Demand).
///   * `RECALL_ON_OPEN` — the older whole-file variant.
///   * `OFFLINE` — set by classic HSM tools, and still what some providers use.
#[cfg(windows)]
pub(crate) fn is_placeholder(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

    extern "system" {
        fn GetFileAttributesW(name: *const u16) -> u32;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call. This reads
    // metadata only — it never opens the file, so it cannot trigger a hydration.
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return false;
    }
    attrs
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
        != 0
}

/// macOS has the same idea: iCloud Drive evicts a file's bytes and leaves the entry behind,
/// flagged `SF_DATALESS`. Reading one is *meant* to fetch it back transparently, and often
/// does — but not always, and a `.pkz` that reads as empty is indistinguishable from a mod
/// with nothing in it. That is why this is worth knowing before reading rather than after.
///
/// Like the Windows half, this asks for attributes only. `stat` does not hydrate.
#[cfg(target_os = "macos")]
pub(crate) fn is_placeholder(path: &std::path::Path) -> bool {
    use std::os::macos::fs::MetadataExt;
    /// `SF_DATALESS` from `sys/stat.h` — the bytes live in iCloud, not here.
    const SF_DATALESS: u32 = 0x4000_0000;
    std::fs::metadata(path).is_ok_and(|m| m.st_flags() & SF_DATALESS != 0)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn is_placeholder(_path: &std::path::Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_content_extensions_are_counted() {
        assert!(is_content(std::path::Path::new("a/b/bike.pkz")));
        assert!(is_content(std::path::Path::new("a/b/PAINT.PNT")));
        assert!(!is_content(std::path::Path::new("a/b/readme.txt")));
        assert!(!is_content(std::path::Path::new("a/b/noext")));
    }

    #[test]
    fn the_provider_is_named_from_the_path() {
        let p = std::path::Path::new("C:/Users/x/OneDrive/Documents/PiBoSo/MX Bikes/mods");
        assert_eq!(provider_of(p).as_deref(), Some("OneDrive"));
        assert_eq!(provider_of(std::path::Path::new("D:/Games/mods")), None);
    }

    #[test]
    fn a_real_folder_reports_nothing_dehydrated() {
        // Whatever else is true of the temp dir, nothing in it is a cloud placeholder.
        let found = scan(&std::env::temp_dir());
        assert_eq!(found.count, 0);
    }
}
