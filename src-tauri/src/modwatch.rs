//! Watch the mods **content** folder and ask FrostMod to reload when it changes.
//!
//! The in-app installer already signals a reload after it places a mod (see
//! `install::notify_frostmod`). This module covers the other path: a track or bike
//! the user downloads and drops into the folder themselves. A recursive watcher on
//! `<mods_path>/mods` pulses a reload once the folder has settled.
//!
//! We deliberately watch only `<mods_path>/mods` — where tracks/bikes live — and NOT
//! the sibling `profiles/` folder, which churns constantly during gameplay (replays,
//! telemetry, settings) and would otherwise fire reloads mid-race.
//!
//! Two things keep the reload from being a blunt instrument:
//!
//! * **Settling.** Copying a folder of tracks writes files for as long as the copy
//!   takes, and the debouncer keeps handing us batches throughout. Reloading on each
//!   one asks the game to re-scan its content over and over while it's still being
//!   written. Instead we accumulate and wait for the folder to go quiet.
//! * **Naming what changed.** Each changed path is reduced to the mod it belongs to,
//!   and that set is handed to FrostMod through the command channel so a reload can be
//!   scoped to the new mods rather than the whole collection. The plain reload event is
//!   still pulsed afterwards, so a FrostMod that doesn't know the verb behaves exactly
//!   as it does today.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Debounce window inside the watcher. Extracting a track writes many files in a
/// burst; this collapses them into batches rather than one event per file.
const DEBOUNCE: Duration = Duration::from_millis(1500);

/// How long the folder must stay quiet before we call a change finished. Sized to
/// outlast the gaps between files in a slow copy without making a single dropped-in
/// `.pkz` feel sluggish.
const SETTLE: Duration = Duration::from_secs(3);

/// Upper bound on settling. A download that trickles in for minutes shouldn't hold
/// the reload forever — past this we act on what has landed so far.
const MAX_SETTLE: Duration = Duration::from_secs(45);

/// Slug the folder watcher tags its `frostmod-reload` events with. In-app install
/// handlers filter on their own slug, so this sentinel never collides with them.
pub const WATCH_SLUG: &str = "__mods_watch__";

/// Partial-write suffixes browsers and archivers leave behind. A reload triggered by
/// one of these would fire against a file that isn't finished being written.
const PARTIAL_SUFFIXES: [&str; 6] = [".tmp", ".part", ".partial", ".crdownload", ".download", ".!ut"];

/// A watcher and the flag that retires it.
pub struct Running {
    /// Dropping the debouncer stops its background thread.
    _debouncer: Debouncer<RecommendedWatcher>,
    /// Cleared on stop. A settle thread can be mid-wait when the user switches folders
    /// or turns auto-reload off, and it must not fire a reload for a watcher that's
    /// already been retired.
    live: Arc<AtomicBool>,
}

/// Managed handle to the running watcher. `None` when disabled, when no mods path is
/// configured, or when the content folder doesn't exist yet.
#[derive(Default)]
pub struct ModWatcher(pub Mutex<Option<Running>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchReload {
    slug: &'static str,
    outcome: crate::frostmod::ReloadOutcome,
    /// The mods that changed, as `<type>/<name>` (e.g. `tracks/Red Bud`). Lets the UI
    /// say what was picked up instead of announcing a bare reload.
    mods: Vec<String>,
}

/// The content root we watch: `<mods_path>/mods` holds `tracks/` and `bikes/`.
fn watch_root(mods_path: &str) -> PathBuf {
    Path::new(mods_path).join("mods")
}

/// Changes seen since the last reload, plus when the most recent one landed.
#[derive(Default)]
struct Pending {
    mods: BTreeSet<String>,
    first_seen: Option<Instant>,
    last_seen: Option<Instant>,
    /// A settle thread is already waiting on this batch.
    settling: bool,
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
    let pending: Arc<Mutex<Pending>> = Arc::default();
    let live = Arc::new(AtomicBool::new(true));
    let watched = root.clone();

    let batch_live = Arc::clone(&live);
    let mut debouncer = match new_debouncer(DEBOUNCE, move |res: DebounceEventResult| match res {
        Ok(events) if !events.is_empty() => {
            let changed: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
            on_batch(&handle, &pending, &batch_live, &watched, changed);
        }
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
    *state.0.lock().unwrap() = Some(Running {
        _debouncer: debouncer,
        live,
    });
}

/// Tear down the watcher, if any, and retire anything still settling behind it.
pub fn stop(state: &ModWatcher) {
    if let Some(running) = state.0.lock().unwrap().take() {
        running.live.store(false, Ordering::SeqCst);
    }
}

/// Is this a half-written file we should ignore until the writer is done with it?
fn is_partial(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    // Office/editor lock files and the archivers' scratch names.
    lower.starts_with('~') || PARTIAL_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// Reduce a changed path to the mod it belongs to: `<type>/<name>` relative to the
/// watched root, e.g. `.../mods/tracks/Red Bud/Red Bud.pkz` -> `tracks/Red Bud`.
///
/// A change anywhere inside a mod names that mod, so a track being extracted file by
/// file collapses to a single entry rather than hundreds.
fn mod_key(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut segs = rel.components().filter_map(|c| match c {
        Component::Normal(s) => s.to_str(),
        _ => None,
    });
    let kind = segs.next()?;
    let name = segs.next()?;
    Some(format!("{kind}/{name}"))
}

/// A debounced batch landed. Fold it into the pending set and make sure something is
/// waiting for the folder to go quiet.
fn on_batch(
    app: &AppHandle,
    pending: &Arc<Mutex<Pending>>,
    live: &Arc<AtomicBool>,
    root: &Path,
    changed: Vec<PathBuf>,
) {
    if !live.load(Ordering::SeqCst) {
        return;
    }
    let keys: Vec<String> = changed
        .iter()
        .filter(|p| !is_partial(p))
        .filter_map(|p| mod_key(root, p))
        .collect();
    if keys.is_empty() {
        // Everything in the batch was scratch files, or churn directly in the root.
        return;
    }

    let now = Instant::now();
    let start_settling = {
        let mut p = pending.lock().unwrap_or_else(|e| e.into_inner());
        p.mods.extend(keys);
        p.first_seen.get_or_insert(now);
        p.last_seen = Some(now);
        let start = !p.settling;
        p.settling = true;
        start
    };

    if start_settling {
        let app = app.clone();
        let pending = Arc::clone(pending);
        let live = Arc::clone(live);
        std::thread::spawn(move || settle_then_reload(app, pending, live));
    }
}

/// Wait for the mods folder to stop changing, then fire one reload for the whole batch.
fn settle_then_reload(app: AppHandle, pending: Arc<Mutex<Pending>>, live: Arc<AtomicBool>) {
    let mods = loop {
        std::thread::sleep(SETTLE / 3);
        if !live.load(Ordering::SeqCst) {
            return;
        }

        let mut p = pending.lock().unwrap_or_else(|e| e.into_inner());
        let quiet = p.last_seen.map(|t| t.elapsed() >= SETTLE).unwrap_or(true);
        let overdue = p.first_seen.map(|t| t.elapsed() >= MAX_SETTLE).unwrap_or(false);
        if quiet || overdue {
            if overdue && !quiet {
                log::info!("mods watcher: still changing after {MAX_SETTLE:?} — reloading what's landed");
            }
            let taken = std::mem::take(&mut *p);
            break taken.mods;
        }
    };

    if mods.is_empty() {
        return;
    }
    reload(&app, mods.into_iter().collect());
}

/// Tell FrostMod which mods changed, then pulse the reload it already understands.
///
/// Order matters: the command file is in place before the reload event fires, so a
/// FrostMod that reads it can scope the rescan to those mods. One that doesn't simply
/// ignores an unknown verb and does its usual full reload — no behaviour change.
fn reload(app: &AppHandle, mods: Vec<String>) {
    crate::frostmod::signal_reload_paths(&mods);
    let outcome = crate::frostmod::signal_reload();
    log::info!("mods watcher: {} mod(s) changed -> reload {outcome:?}: {mods:?}", mods.len());
    let _ = app.emit(
        "frostmod-reload",
        WatchReload {
            slug: WATCH_SLUG,
            outcome,
            mods,
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

    #[test]
    fn a_change_anywhere_inside_a_mod_names_that_mod() {
        let root = Path::new("/games/mxb/mods");
        // Every file of an extracting track collapses onto one key.
        assert_eq!(
            mod_key(root, &root.join("tracks/Red Bud/Red Bud.map")).as_deref(),
            Some("tracks/Red Bud"),
        );
        assert_eq!(
            mod_key(root, &root.join("tracks/Red Bud/textures/dirt.tga")).as_deref(),
            Some("tracks/Red Bud"),
        );
        // A packaged mod dropped straight into its type folder is its own key.
        assert_eq!(
            mod_key(root, &root.join("tracks/Red Bud.pkz")).as_deref(),
            Some("tracks/Red Bud.pkz"),
        );
    }

    #[test]
    fn churn_outside_a_mod_yields_no_key() {
        let root = Path::new("/games/mxb/mods");
        // The type folder itself changing tells us nothing about which mod moved.
        assert_eq!(mod_key(root, &root.join("tracks")), None);
        assert_eq!(mod_key(root, root), None);
        // Somewhere else entirely (a watcher restart racing a path change).
        assert_eq!(mod_key(root, Path::new("/elsewhere/x.pkz")), None);
    }

    #[test]
    fn half_written_downloads_are_ignored() {
        assert!(is_partial(Path::new("/m/tracks/Red Bud.pkz.crdownload")));
        assert!(is_partial(Path::new("/m/tracks/Red Bud.pkz.part")));
        assert!(is_partial(Path::new("/m/tracks/RedBud.TMP")));
        assert!(is_partial(Path::new("/m/tracks/~$scratch")));
        // The finished article must still get through.
        assert!(!is_partial(Path::new("/m/tracks/Red Bud.pkz")));
    }
}
