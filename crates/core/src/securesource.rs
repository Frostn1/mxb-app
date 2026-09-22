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

/// Whether this path is an `.mxbsecure` blob.
pub fn is_secured(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mxbsecure"))
}

/// The key file the app writes beside a new blob: `X.mxbsecure` → `X.mxbsecurekey`. Older blobs
/// were named `X.pkz.mxbsecure` with a `X.pkz.mxbsecure.mxbkey` sibling; that legacy form is still
/// read (see [`existing_key_path`]), but every fresh provision writes the short name.
pub fn key_path_for(blob_path: &str) -> String {
    match blob_path.strip_suffix(".mxbsecure") {
        Some(stem) => format!("{stem}.mxbsecurekey"),
        None => format!("{blob_path}.mxbkey"), // not a .mxbsecure name; keep the old shape
    }
}

/// The key file that actually exists beside `blob_path`, preferring the new `.mxbsecurekey` name
/// and falling back to the legacy `.mxbkey` sibling, or `None` if neither is present.
pub fn existing_key_path(blob_path: &str) -> Option<String> {
    let new = key_path_for(blob_path);
    if Path::new(&new).exists() {
        return Some(new);
    }
    let legacy = format!("{blob_path}.mxbkey");
    Path::new(&legacy).exists().then_some(legacy)
}
