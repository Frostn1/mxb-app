//! Identifying `.mxbsecure` files and their local key state without opening their contents.
//!
//! The desktop apps must never decrypt these blobs. Only `mxbsecure.dll`, inside the game
//! process, may turn one into its inner PKZ. Core therefore exposes filename/key-state helpers
//! only; archive readers deliberately reject this format.

use std::path::Path;
use std::sync::OnceLock;

type UnlockedCheck = Box<dyn Fn(&Path) -> bool + Send + Sync>;

static UNLOCKED: OnceLock<UnlockedCheck> = OnceLock::new();

/// Register the cheap "is this unlocked for the live account?" check — unseals the key only, never
/// the content, so the library scan can flag every secured file without decrypting it.
pub fn set_unlocked_check(f: UnlockedCheck) {
    let _ = UNLOCKED.set(f);
}

/// Whether a secured file is unlocked for the live account (a key beside it opens). `false` when
/// there's no check registered.
pub fn is_unlocked(path: &Path) -> bool {
    UNLOCKED.get().map(|f| f(path)).unwrap_or(false)
}

/// Whether this path is an `.mxbsecure` blob. Any case: a player who renames `X.MXBSECURE` still
/// has a secured file, and every scan (library, auto-unlock, game manifest) must agree on that.
pub fn is_secured(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mxbsecure"))
}

/// Whether a bare file name is an `.mxbsecure` blob, in any case. `X.mxbsecurekey` is not.
pub fn is_secured_name(name: &str) -> bool {
    is_secured(Path::new(name))
}

/// `name` without its `.mxbsecure` suffix, matched in any case. `None` when it has none.
pub fn strip_secured_ext(name: &str) -> Option<&str> {
    const EXT: &str = ".mxbsecure";
    let cut = name.len().checked_sub(EXT.len())?;
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(EXT)).then(|| &name[..cut])
}

/// The key file the app writes beside a new blob: `X.mxbsecure` → `X.mxbsecurekey`, whatever the
/// case of the blob's extension. Older blobs were named `X.pkz.mxbsecure` with a
/// `X.pkz.mxbsecure.mxbkey` sibling; that legacy form is still read (see [`existing_key_path`]),
/// but every fresh provision writes the short name.
pub fn key_path_for(blob_path: &str) -> String {
    match strip_secured_ext(blob_path) {
        Some(stem) => format!("{stem}.mxbsecurekey"),
        None => format!("{blob_path}.mxbkey"), // not a .mxbsecure name; keep the old shape
    }
}

/// The key file that actually exists beside `blob_path`, preferring the new `.mxbsecurekey` name
/// and falling back to the legacy `.mxbkey` sibling, or `None` if neither is present.
///
/// Matched in any case, so `X.MXBSECUREKEY` beside `X.MXBSECURE` is found on a case-sensitive
/// file system (Linux) as well as on Windows.
pub fn existing_key_path(blob_path: &str) -> Option<String> {
    let new = key_path_for(blob_path);
    let legacy = format!("{blob_path}.mxbkey");
    for want in [&new, &legacy] {
        if Path::new(want).exists() {
            return Some(want.clone());
        }
    }
    for want in [&new, &legacy] {
        if let Some(found) = sibling_any_case(Path::new(want)) {
            return Some(found);
        }
    }
    None
}

/// A file in `want`'s folder whose name equals `want`'s ignoring ASCII case.
fn sibling_any_case(want: &Path) -> Option<String> {
    let name = want.file_name()?.to_str()?;
    let dir = match want.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let n = e.file_name();
        let n = n.to_str()?;
        (n.eq_ignore_ascii_case(name) && e.path().is_file())
            .then(|| e.path().to_string_lossy().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_is_recognised_in_any_case() {
        assert!(is_secured_name("Paint.mxbsecure"));
        assert!(is_secured_name("Paint.MXBSECURE"));
        assert!(is_secured_name("Paint.MxbSecure"));
        assert!(!is_secured_name("Paint.mxbsecurekey"));
        assert!(!is_secured_name("Paint.pnt"));
        assert_eq!(strip_secured_ext("Paint.MXBSECURE"), Some("Paint"));
        assert_eq!(strip_secured_ext("Paint.pnt"), None);
    }

    #[test]
    fn an_upper_case_blob_gets_the_short_key_name() {
        assert_eq!(key_path_for("C:/m/Paint.MXBSECURE"), "C:/m/Paint.mxbsecurekey");
        assert_eq!(key_path_for("C:/m/Paint.mxbsecure"), "C:/m/Paint.mxbsecurekey");
    }

    #[test]
    fn a_key_sibling_is_found_whatever_its_case() {
        let dir = std::env::temp_dir().join(format!("mxb-keycase-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let blob = dir.join("Paint.MXBSECURE");
        std::fs::write(&blob, b"x").unwrap();
        std::fs::write(dir.join("PAINT.MXBSECUREKEY"), b"k").unwrap();
        let found = existing_key_path(&blob.to_string_lossy()).expect("key found");
        assert!(found.to_ascii_lowercase().ends_with("paint.mxbsecurekey"));

        // The legacy sibling too.
        let old = dir.join("Old.pkz.MXBSECURE");
        std::fs::write(&old, b"x").unwrap();
        std::fs::write(dir.join("Old.pkz.MXBSECURE.MXBKEY"), b"k").unwrap();
        assert!(existing_key_path(&old.to_string_lossy()).is_some());

        let none = dir.join("Nothing.mxbsecure");
        std::fs::write(&none, b"x").unwrap();
        assert!(existing_key_path(&none.to_string_lossy()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
