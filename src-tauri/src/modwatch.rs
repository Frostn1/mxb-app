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

/// How deep to look for folders that are really links to somewhere else.
///
/// A recursive watch does not cross a junction — `ReadDirectoryChangesW` stops at a
/// reparse point — so a player who shares one paints folder between rider models gets no
/// events for it unless the target is watched in its own right. Four levels reaches every
/// place such a link is put in practice (`bikes/<bike>/paints`,
/// `rider/riders/<model>/paints`, `rider/helmets/<helmet>/goggles`) without descending
/// into a track's interior looking for one.
const LINK_DEPTH: usize = 4;

/// Upper bound on the extra watches. Each is a real OS handle; a tree with more links
/// than this is doing something we shouldn't try to keep up with.
const LINK_CAP: usize = 64;

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

/// The content root we watch — the folder holding `tracks/` and `bikes/`. Usually
/// `<mods_path>/mods`, but `mods_path` may already be the mods tree, so it goes through
/// the resolver rather than joining `mods` blind.
fn watch_root(mods_path: &str) -> PathBuf {
    crate::library::mods_root(mods_path)
}

/// Changes seen since the last reload, plus when the most recent one landed.
///
/// All the coalescing lives here rather than in the settle thread, so "a copy in
/// progress yields exactly one reload, once it's finished" is something tests can
/// assert against a clock they control.
#[derive(Default)]
struct Pending {
    mods: BTreeSet<String>,
    first_seen: Option<Instant>,
    last_seen: Option<Instant>,
    /// A settle thread is already waiting on this batch.
    settling: bool,
}

impl Pending {
    /// Fold a batch of changed mods in. Returns whether this batch is the one that
    /// needs a settle thread started behind it — every later batch joins that one.
    fn absorb(&mut self, keys: Vec<String>, now: Instant) -> bool {
        self.mods.extend(keys);
        self.first_seen.get_or_insert(now);
        self.last_seen = Some(now);
        let start = !self.settling;
        self.settling = true;
        start
    }

    /// The batch, once the folder has gone quiet — or once we've waited long enough
    /// that a still-trickling download shouldn't hold the reload any longer. Taking it
    /// resets the state, so the next change starts a fresh batch.
    fn take_if_settled(&mut self, now: Instant) -> Option<Vec<String>> {
        let since = |t: Option<Instant>| t.map(|t| now.saturating_duration_since(t));
        let quiet = since(self.last_seen).is_none_or(|d| d >= SETTLE);
        let overdue = since(self.first_seen).is_some_and(|d| d >= MAX_SETTLE);
        if !quiet && !overdue {
            return None;
        }
        if overdue && !quiet {
            log::info!("mods watcher: still changing after {MAX_SETTLE:?} — reloading what's landed");
        }
        Some(std::mem::take(self).mods.into_iter().collect())
    }
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
    // Folders inside the tree that really live somewhere else. Resolved before the
    // debouncer is built because its callback needs them to name what changed.
    let links = crate::linkwalk::dir_links(&root, LINK_DEPTH, LINK_CAP);
    let event_links = links.clone();

    let batch_live = Arc::clone(&live);
    let mut debouncer = match new_debouncer(DEBOUNCE, move |res: DebounceEventResult| match res {
        Ok(events) if !events.is_empty() => {
            let changed: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
            on_batch(&handle, &pending, &batch_live, &watched, &event_links, changed);
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

    // One watch per distinct target: six rider models sharing a paints folder is six
    // links and one place to watch.
    let mut targets: Vec<&PathBuf> = links.iter().map(|(_, t)| t).collect();
    targets.sort();
    targets.dedup();
    for target in targets {
        match debouncer.watcher().watch(target, RecursiveMode::Recursive) {
            Ok(()) => log::info!("mods watcher: also watching linked folder {}", target.display()),
            // Best-effort, exactly like the root: a target we can't watch costs the
            // auto-reload for that folder, not the watcher.
            Err(e) => log::warn!("mods watcher: could not watch {}: {e}", target.display()),
        }
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
    // Manage's parking lot. It normally sits beside the mods tree and never shows up
    // here, but a player who relocated their tree with `mxbikes.ini` has `mods_path`
    // *as* the tree, which puts the shadow folder inside the watched root. Parking a
    // track would then announce "mxbapp_disabled/Red Bud" and pulse a reload for files
    // the game is no longer meant to see.
    if kind.eq_ignore_ascii_case(crate::modstate::SHADOW_DIR) {
        return None;
    }
    let name = segs.next()?;
    Some(format!("{kind}/{name}"))
}

/// Every mod a changed path belongs to.
///
/// Usually exactly one. More than one when the change landed in a folder several mods
/// link to — dropping a livery into a shared paints folder changes it for all six rider
/// models pointing at it, and each of them needs naming. The path arrives as the watcher
/// saw it (under the link's *target*), so it's put back where it appears in the tree
/// before being reduced.
fn mod_keys(root: &Path, links: &[(PathBuf, PathBuf)], path: &Path) -> Vec<String> {
    if let Some(key) = mod_key(root, path) {
        return vec![key];
    }
    let mut out = Vec::new();
    for (link, target) in links {
        let Ok(rest) = path.strip_prefix(target) else {
            continue;
        };
        if let Some(key) = mod_key(root, &link.join(rest)) {
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

/// A debounced batch landed. Fold it into the pending set and make sure something is
/// waiting for the folder to go quiet.
fn on_batch(
    app: &AppHandle,
    pending: &Arc<Mutex<Pending>>,
    live: &Arc<AtomicBool>,
    root: &Path,
    links: &[(PathBuf, PathBuf)],
    changed: Vec<PathBuf>,
) {
    if !live.load(Ordering::SeqCst) {
        return;
    }
    let keys: Vec<String> = changed
        .iter()
        .filter(|p| !is_partial(p))
        .flat_map(|p| mod_keys(root, links, p))
        .collect();
    if keys.is_empty() {
        // Everything in the batch was scratch files, or churn directly in the root.
        return;
    }

    let start_settling = pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .absorb(keys, Instant::now());

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
        let settled = pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take_if_settled(Instant::now());
        if let Some(mods) = settled {
            break mods;
        }
    };

    if mods.is_empty() {
        return;
    }
    reload(&app, mods);
}

/// Pulse FrostMod's reload once for the settled batch.
///
/// Scoping the reload to just these mods is FrostMod's call, not ours — its reload
/// already rebuilds the content lists surgically, stepped a list per frame. All this
/// side owes it is one pulse, after the writes are done. The mod names ride along to
/// the UI only.
fn reload(app: &AppHandle, mods: Vec<String>) {
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

    /// Manage's parking lot normally sits beside the mods tree, out of the watcher's way.
    /// A player who relocated their tree has it *inside* the watched root — parking a
    /// track would otherwise announce a mod called "mxbapp_disabled/Red Bud" and pulse a
    /// reload for files the game is no longer meant to see.
    #[test]
    fn the_parking_lot_is_not_watched_as_content() {
        let root = Path::new("/games/mxb/mods");
        assert_eq!(
            mod_key(root, &root.join("tracks").join("Red Bud").join("a.pkz")),
            Some("tracks/Red Bud".to_string()),
        );
        assert_eq!(mod_key(root, &root.join("mxbapp_disabled/tracks/Red Bud.pkz")), None);
        assert_eq!(mod_key(root, &root.join("MXBApp_Disabled/tracks/Red Bud.pkz")), None);
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

    /// A livery dropped into a folder that several rider models link to changes all of
    /// them, and the watcher sees it at the target's own path — outside the tree.
    #[test]
    fn a_change_behind_a_link_names_every_mod_pointing_at_it() {
        let root = Path::new("/games/mxb/mods");
        let shared = PathBuf::from("/paints/shared");
        let links = vec![
            (root.join("rider/riders/Male/paints"), shared.clone()),
            (root.join("rider/riders/Female/paints"), shared.clone()),
            (root.join("bikes/KTM 450/paints"), PathBuf::from("/paints/bikes")),
        ];

        assert_eq!(
            mod_keys(root, &links, &shared.join("Frost.pnt")),
            vec!["rider/riders"],
            "the change is placed back in the tree — both models reduce to the same key",
        );
        assert_eq!(
            mod_keys(root, &links, Path::new("/paints/bikes/Frost.pnt")),
            vec!["bikes/KTM 450"],
        );
        // A path inside the tree still resolves directly, without consulting the links.
        assert_eq!(
            mod_keys(root, &links, &root.join("tracks/Red Bud/x.map")),
            vec!["tracks/Red Bud"],
        );
        // And something that is neither still yields nothing.
        assert!(mod_keys(root, &links, Path::new("/elsewhere/x.pkz")).is_empty());
    }

    /// A batch of changed mod keys, as `on_batch` would hand them over.
    fn batch(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn a_copy_in_progress_yields_one_reload_once_it_finishes() {
        // Dropping a folder of tracks in writes files for as long as the copy takes,
        // and the debouncer hands us a batch throughout. Reloading on each one asks
        // the game to rescan while the files are still being written — which is how a
        // track ends up missing until the game is restarted.
        let t0 = Instant::now();
        let mut p = Pending::default();

        assert!(p.absorb(batch(&["tracks/Red Bud"]), t0), "first batch starts settling");
        assert!(
            !p.absorb(batch(&["tracks/Iron Man"]), t0 + Duration::from_secs(1)),
            "later batches join the one already settling — no second thread, no second reload",
        );
        // Still being written: nothing fires.
        assert_eq!(p.take_if_settled(t0 + Duration::from_secs(2)), None);
        assert_eq!(p.take_if_settled(t0 + Duration::from_secs(3)), None);

        // Quiet for SETTLE after the *last* write — now, and only now, one reload
        // carrying both tracks.
        let settled = p
            .take_if_settled(t0 + Duration::from_secs(1) + SETTLE)
            .expect("fires once the folder goes quiet");
        assert_eq!(settled, vec!["tracks/Iron Man", "tracks/Red Bud"]);
    }

    #[test]
    fn every_change_inside_one_mod_collapses_to_a_single_entry() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        // An extracting track touches hundreds of files; the game only needs telling once.
        p.absorb(batch(&["tracks/Red Bud"]), t0);
        p.absorb(batch(&["tracks/Red Bud", "tracks/Red Bud"]), t0);
        assert_eq!(p.take_if_settled(t0 + SETTLE), Some(vec!["tracks/Red Bud".into()]));
    }

    #[test]
    fn a_download_that_never_settles_still_reloads_eventually() {
        // A slow trickle would otherwise hold the reload forever.
        let t0 = Instant::now();
        let mut p = Pending::default();
        p.absorb(batch(&["tracks/Slow"]), t0);

        let mut t = t0;
        for _ in 0..10 {
            t += Duration::from_secs(1);
            p.absorb(batch(&["tracks/Slow"]), t);
            assert_eq!(p.take_if_settled(t), None, "never quiet, and not yet overdue");
        }
        assert!(
            p.take_if_settled(t0 + MAX_SETTLE).is_some(),
            "past the cap we act on what's landed",
        );
    }

    #[test]
    fn taking_a_batch_leaves_the_next_change_to_start_a_fresh_one() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        p.absorb(batch(&["tracks/First"]), t0);
        assert!(p.take_if_settled(t0 + SETTLE).is_some());

        // The settle thread has retired; the next drop must be able to start another.
        let later = t0 + SETTLE + Duration::from_secs(60);
        assert!(p.absorb(batch(&["tracks/Second"]), later), "a new batch settles on its own");
        assert_eq!(
            p.take_if_settled(later + SETTLE),
            Some(vec!["tracks/Second".into()]),
            "and carries only the new mod",
        );
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
