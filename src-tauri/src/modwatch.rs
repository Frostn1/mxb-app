//! Watch the mods **content** folder and ask FrostMod to reload when it changes.
//!
//! The in-app installer already signals a reload after it places a mod (see
//! `install::notify_frostmod`). This module covers the other path: a track or bike
//! the user downloads and drops into the folder themselves. A debounced recursive
//! watcher on `<mods_path>/mods` pulses the same reload once the folder settles.
//!
//! We deliberately watch only `<mods_path>/mods` — where tracks/bikes live — and NOT
//! the sibling `profiles/` folder, which churns constantly during gameplay (replays,
//! telemetry, settings) and would otherwise fire reloads mid-race.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Debounce window. Extracting/copying a track writes many files in a burst; we want
/// a single reload once the folder settles, not one per file.
const DEBOUNCE: Duration = Duration::from_millis(1500);

/// Slug the folder watcher tags its `frostmod-reload` events with. In-app install
/// handlers filter on their own slug, so this sentinel never collides with them.
pub const WATCH_SLUG: &str = "__mods_watch__";

/// Managed handle to the running watcher. `None` when disabled, when no mods path is
/// configured, or when the content folder doesn't exist yet. Dropping the inner
/// `Debouncer` stops its background thread.
#[derive(Default)]
pub struct ModWatcher(pub Mutex<Option<Debouncer<RecommendedWatcher>>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchReload {
    slug: &'static str,
    outcome: crate::frostmod::ReloadOutcome,
}

/// The content root we watch: `<mods_path>/mods` holds `tracks/` and `bikes/`.
fn watch_root(mods_path: &str) -> PathBuf {
    Path::new(mods_path).join("mods")
}

/// Start (or restart) the watcher on `<mods_path>/mods`. Replaces any existing
/// watcher. Best-effort: a blank path, a missing folder, or a watch error just
/// leaves the watcher disabled and is logged.
pub fn start(app: &AppHandle, state: &ModWatcher, mods_path: &str) {
    stop(state);

    if mods_path.trim().is_empty() {
        return;
    }
    let root = watch_root(mods_path);
    if !root.is_dir() {
        log::info!("mods watcher: content folder not present yet, not watching: {}", root.display());
        return;
    }

    let handle = app.clone();
    let mut debouncer = match new_debouncer(DEBOUNCE, move |res: DebounceEventResult| match res {
        Ok(events) if !events.is_empty() => on_change(&handle),
        Ok(_) => {}
        Err(e) => log::warn!("mods watcher: event error: {e:?}"),
    }) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("mods watcher: could not create debouncer: {e}");
            return;
        }
    };

    if let Err(e) = debouncer.watcher().watch(&root, RecursiveMode::Recursive) {
        log::warn!("mods watcher: could not watch {}: {e}", root.display());
        return;
    }

    log::info!("mods watcher: watching {} for changes", root.display());
    *state.0.lock().unwrap() = Some(debouncer);
}

/// Tear down the watcher, if any.
pub fn stop(state: &ModWatcher) {
    *state.0.lock().unwrap() = None;
}

/// A settled batch of changes landed in the mods folder — pulse FrostMod's reload and
/// tell the UI so it can surface it. Fire-and-forget; `signal_reload` no-ops when
/// FrostMod isn't running or off-Windows.
fn on_change(app: &AppHandle) {
    let outcome = crate::frostmod::signal_reload();
    log::info!("mods watcher: folder changed -> reload {outcome:?}");
    let _ = app.emit(
        "frostmod-reload",
        WatchReload {
            slug: WATCH_SLUG,
            outcome,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_root_is_the_content_subfolder_not_the_root() {
        // Must be `<mods_path>/mods`, never the root (which also holds churny profiles/).
        assert_eq!(watch_root("/games/mxb"), PathBuf::from("/games/mxb").join("mods"));
        assert!(watch_root("/games/mxb").ends_with("mods"));
    }
}
