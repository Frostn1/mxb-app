//! Small shared facts that have no better home.
//!
//! Each of these was a private helper inside a module that serves one app, and each is
//! needed by code that serves both. They are here rather than duplicated so the two apps
//! cannot end up disagreeing about where the control plane is, or about which characters
//! are safe in a filename.

use std::path::{Component, Path, PathBuf};

/// The extension a paint file carries. Only `.pnt` is ever shared or written by name.
pub const PAINT_EXT: &str = "pnt";

/// Where the control plane lives. A constant rather than a setting: pointing the app at
/// another host would let anything served there write files into the mods folder.
pub const CONTROL_PLANE: &str = "https://mxb-control-plane.aui-svi.workers.dev";

/// Set to a base URL to talk to a control plane other than the real one. **Debug builds
/// only** — see [`control_plane`].
pub const CONTROL_PLANE_ENV: &str = "MXB_CONTROL_PLANE";

/// The control plane's base URL, honouring the debug-only override.
///
/// The override exists so the loop can be exercised against `wrangler dev` without pointing
/// a test run at the live accounts, and `cfg!(debug_assertions)` is what keeps it out of
/// anything anyone is handed.
pub fn control_plane() -> String {
    if cfg!(debug_assertions) {
        if let Ok(base) = std::env::var(CONTROL_PLANE_ENV) {
            let base = base.trim().trim_end_matches('/');
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    CONTROL_PLANE.to_string()
}

/// Does `p` end in `ext`, whatever case it was written in?
pub fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

/// A file that came along with a mod but is not part of it — a readme, a link, a notes file.
pub fn is_junk(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("readme")
        || n.ends_with(".txt")
        || n.ends_with(".url")
        || n.ends_with(".nfo")
        || n.ends_with(".md")
}

/// Flatten the characters a filesystem will not take. Not a path sanitiser — separators are
/// replaced rather than rejected, because this names a single file, never a path.
pub fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

/// A staging directory nobody else is using.
///
/// This used to be one path per process, wiped on entry — fine while installs were strictly
/// serial, fatal the moment two can be alive at once: a dropzone plan sits staged while the
/// user reviews it, and a second drop would delete the first one's files out from under it.
///
/// Shared because both binaries stage work: the manager while installing, the studio while
/// packing a paint or building a track.
pub fn staging_dir(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("frost-{tag}-{}-{stamp:x}-{n}", std::process::id()))
}

/// those too, but only this check actually protects a disk, so it does not trust it.
pub fn safe_dest(mods_dir: &Path, rel_dest: &str) -> Option<PathBuf> {
    let rel = rel_dest.trim();
    if rel.is_empty() || rel.len() > 256 {
        return None;
    }
    // One separator form to reason about; a backslash would be a path separator on Windows
    // while looking like an ordinary character to a naive check.
    if rel.contains('\\') {
        return None;
    }
    if rel.starts_with('/') {
        return None;
    }
    // `C:` and friends.
    if rel.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    if rel.chars().any(|c| c.is_control()) {
        return None;
    }

    let mut out = mods_dir.to_path_buf();
    let mut segments = 0usize;
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return None;
        }
        // Reject anything the OS would interpret as more than a plain name.
        let as_path = Path::new(segment);
        if as_path.components().count() != 1
            || !matches!(as_path.components().next(), Some(Component::Normal(_)))
        {
            return None;
        }
        out.push(segment);
        segments += 1;
    }
    if segments < 2 {
        // A bare filename would drop a paint at the root of the mods folder, which is never
        // where one belongs.
        return None;
    }
    if out.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
        != Some(PAINT_EXT.to_string())
    {
        return None;
    }
    // Belt and braces: whatever the segment walk produced must still sit under the root.
    if !out.starts_with(mods_dir) {
        return None;
    }
    Some(out)
}

/// Keep an asset id to the characters a header and a URL are both happy with.
///
/// Both binaries need it: the studio names a sealed asset, the manager opens one.
pub fn sanitize_asset_id(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() { "asset".to_string() } else { trimmed.to_string() }
}
