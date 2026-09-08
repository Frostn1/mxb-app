//! Moving a mod to the Trash, and putting it back.
//!
//! Manager-only, and that is the point: on macOS the `trash` crate drives Finder over
//! AppleScript, which is why this calls NSFileManager directly and drags in `objc2` and
//! `objc2-foundation` to do it. None of that has any business in a binary that only paints,
//! and uninstalling is the manager's job anyway.

use mxb_core::library::mods_subdir;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the Trash put something, when we can tell — so it can be put back.
///
/// macOS can answer with a path, because the folders are ordinary directories we can look
/// in. Windows renames everything it recycles to `$R…`, so there is no path worth keeping and
/// the answer is `None`; [`restore_from_trash`] asks the OS instead. Either way the ledger
/// only has to record what it was given.
pub type TrashedAt = Option<String>;

/// Move `path` to the Trash, reporting where it went.
///
/// Not plain `trash::delete`: on macOS that drives Finder over AppleScript, and Finder refuses
/// a file iCloud has evicted — *"the item needs to be downloaded"*, error -8013. A mods folder
/// under `Documents` is exactly where eviction happens, and with most of a library dataless
/// this made uninstalling fail on nearly every mod. `NSFileManager` has no such objection, and
/// wants no automation permission either.
///
/// Calling it directly rather than through the crate, which passes `None` for the resulting
/// URL and throws the answer away. That answer is the only reliable way to learn where the
/// file went: the Trash cannot simply be listed afterwards, because macOS refuses `~/.Trash`
/// to an app without Full Disk Access, and a mods folder in iCloud and one outside it land in
/// different Trash folders anyway.
#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &Path) -> anyhow::Result<TrashedAt> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let path_str = path.to_string_lossy();
    // The same call the `trash` crate makes, asking for the one extra value it declines to
    // request. `objc2` wraps it safely, so there is no unsafe block to justify.
    let mgr = NSFileManager::defaultManager();
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path_str));
    let mut out = None;
    mgr.trashItemAtURL_resultingItemURL_error(&url, Some(&mut out))
        .map_err(|e| anyhow::anyhow!("could not move to Trash: {e}"))?;
    Ok(out.and_then(|u| u.path()).map(|p| p.to_string()))
}

/// Windows and Linux have no Finder problem, and their own Trash reports nothing useful
/// about where an item landed — see [`TrashedAt`].
#[cfg(not(target_os = "macos"))]
pub fn move_to_trash(path: &Path) -> anyhow::Result<TrashedAt> {
    trash::delete(path)?;
    Ok(None)
}

/// Put a mod back where it came from.
///
/// `trashed_at` is whatever [`move_to_trash`] reported. When it names a path — macOS — the
/// restore is a plain move. When it doesn't, the OS is asked to undo its own recycle, matching
/// on the path the mod used to occupy.
///
/// Refuses to overwrite: if something is already sitting at `original`, the mod on disk wins
/// and the caller is told, rather than a restore quietly replacing a newer copy.
pub fn restore_from_trash(original: &Path, trashed_at: Option<&str>) -> anyhow::Result<()> {
    if original.exists() {
        anyhow::bail!("something is already installed there");
    }
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Some(from) = trashed_at {
        let from = Path::new(from);
        if !from.exists() {
            anyhow::bail!("it is no longer in the Trash");
        }
        fs::rename(from, original)?;
        return Ok(());
    }

    restore_via_os(original)
}

/// Windows and Linux keep an index of what they recycled and where it came from, so the item
/// can be found by the path it used to have.
#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn restore_via_os(original: &Path) -> anyhow::Result<()> {
    use trash::os_limited;

    let name = original
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = original.parent().unwrap_or(Path::new(""));

    let item = os_limited::list()?
        .into_iter()
        .filter(|i| i.name.to_string_lossy() == name && Path::new(&i.original_parent) == parent)
        .max_by_key(|i| i.time_deleted)
        .ok_or_else(|| anyhow::anyhow!("it is no longer in the Recycle Bin"))?;

    os_limited::restore_all([item])?;
    Ok(())
}

#[cfg(not(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
)))]
fn restore_via_os(_original: &Path) -> anyhow::Result<()> {
    anyhow::bail!("this system can't put files back from the Trash")
}

pub fn uninstall_mod(mods_path: &str, from_path: &str, subpath: &str) -> anyhow::Result<TrashedAt> {
    let from = PathBuf::from(from_path);
    if !from.exists() {
        anyhow::bail!("path not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to uninstall a file outside the {subpath} folder");
    }
    move_to_trash(&from)
}
