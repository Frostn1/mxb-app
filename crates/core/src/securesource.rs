//! Opening a `.mxbsecure` file to its inner `.pkz` bytes **in memory**, for the app's own
//! viewer.
//!
//! The decrypt itself lives in the app: it holds the Steam identity and the sealed key, which
//! core does not. So the app registers an opener here at startup, and the archive readers in
//! [`crate::pkz`] call through it whenever they are handed a `.mxbsecure` path — the plaintext
//! `.pkz` comes back as a `Vec<u8>` that is parsed and rendered from RAM and **never written to
//! disk**. That is the whole point: the source files a secured mod is protecting are visible on
//! screen but never left anywhere they could be copied.

use std::path::Path;
use std::sync::OnceLock;

type Opener = Box<dyn Fn(&Path) -> Option<Vec<u8>> + Send + Sync>;
type UnlockedCheck = Box<dyn Fn(&Path) -> bool + Send + Sync>;

static OPENER: OnceLock<Opener> = OnceLock::new();
static UNLOCKED: OnceLock<UnlockedCheck> = OnceLock::new();

/// Register the app's opener. Called once, early in startup. A second call is ignored.
pub fn set_opener(f: Opener) {
    let _ = OPENER.set(f);
}

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

/// Whether the viewer should route this path through the secure opener (a `.mxbsecure` blob).
pub fn is_secured(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mxbsecure"))
}

/// Decrypt a `.mxbsecure` file to its inner `.pkz` bytes, or `None` when no opener is registered
/// (a build without the secure module) or the file can't be opened for the live account.
pub fn open(path: &Path) -> Option<Vec<u8>> {
    OPENER.get().and_then(|f| f(path))
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
