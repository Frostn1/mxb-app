//! Notice when a paint the 3D viewer is showing gets rewritten on disk.
//!
//! A painter's loop is draw, look at it on the model, fix, look again. The "look" step used
//! to mean closing the viewer and opening it again, because the app read the `.pnt` once and
//! never asked after it. This watches the file the viewer is currently dressed in and says
//! when it changes; the frontend re-decodes it and re-dresses the model.
//!
//! Two decisions worth keeping:
//!
//! * **The folder is watched, not the file.** Saving a paint usually means writing a temp
//!   file and renaming it over the target, which replaces the inode. A watch registered on
//!   the file itself follows the old one and goes silent after the very first save — the
//!   exact save this exists to catch. A non-recursive watch on the parent folder survives
//!   that, and also catches a paint that doesn't exist yet.
//! * **Events are matched by file name alone.** Only the folders holding watched files are
//!   watched, so anything arriving here is already from a folder we asked about, and the
//!   name is enough. Comparing whole paths would mean out-guessing whatever the OS
//!   canonicalises them to (`/private/var` on macOS, 8.3 names on Windows) for nothing.
//!
//! Scope is deliberately narrow — [`start`] replaces the whole watch set each time, because
//! there is one viewer and it shows one paint. Unlike `modwatch` there is no settling: a
//! paint is a single file, the debounce below outlasts writing one, and the caller re-tries
//! a decode that lands mid-write anyway.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::{AppHandle, Emitter, Runtime};

/// Debounce window. Short, because this is the whole latency a painter feels between
/// hitting save and seeing the bike change — and long enough to collapse the burst of write
/// events a multi-megabyte `.pnt` produces into one answer.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// Emitted with the caller's own spelling of the path that changed, so the frontend can
/// compare it against the one it asked to watch.
pub const EVENT: &str = "paint-changed";

/// Upper bound on watched files. The viewer shows one paint; this only exists so a caller
/// that goes wrong can't turn into a fistful of OS watch handles.
const MAX_WATCHED: usize = 16;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Changed {
    path: String,
}

pub struct Running {
    /// Dropping the debouncer stops its background thread.
    _debouncer: Debouncer<RecommendedWatcher>,
    /// Cleared on stop, so a callback already in flight doesn't announce a change for a
    /// watcher that has been retired.
    live: Arc<AtomicBool>,
}

/// Tauri-managed handle. `None` when nothing is being previewed.
#[derive(Default)]
pub struct PaintWatcher(pub Mutex<Option<Running>>);

/// The file name a path ends in, lowercased — what an event is matched on.
fn file_key(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
}

/// Group the requested paths by the folder that has to be watched to see them.
///
/// A `BTreeMap` rather than a list: two paints in one folder cost one watch, and the order
/// is fixed so the log reads the same twice running.
fn by_folder(paths: &[String]) -> BTreeMap<PathBuf, Vec<String>> {
    let mut out: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for p in paths.iter().take(MAX_WATCHED) {
        let path = Path::new(p);
        // A bare file name has no folder to watch, and neither has a path that is one.
        let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) else {
            continue;
        };
        if file_key(path).is_none() {
            continue;
        }
        out.entry(dir.to_path_buf()).or_default().push(p.clone());
    }
    out
}

/// Which of the watched paths a changed path names, if any — answered as the caller's own
/// spelling, never the event's.
fn matches<'a>(watched: &'a [String], changed: &Path) -> Option<&'a str> {
    let key = file_key(changed)?;
    watched
        .iter()
        .find(|w| file_key(Path::new(w)).as_deref() == Some(key.as_str()))
        .map(String::as_str)
}

/// Every watched path a batch of changed paths names, each at most once.
fn changed_paints(watched: &[String], events: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for path in events {
        if let Some(hit) = matches(watched, path) {
            if !out.iter().any(|p| p == hit) {
                out.push(hit.to_string());
            }
        }
    }
    out
}

/// Watch `paths`, replacing whatever was being watched before. An empty list just stops.
///
/// Best-effort throughout, as the other watchers are: a folder that isn't there or refuses
/// a watch costs the live reload for that paint, not the viewer.
/// Generic over the runtime so the tests below can drive it with Tauri's mock one — the
/// thing worth proving here is that a real save on a real folder comes back out as an
/// event, and that needs a genuine watcher and a handle to emit through.
pub fn start<R: Runtime>(app: &AppHandle<R>, state: &PaintWatcher, paths: &[String]) {
    stop(state);
    let folders = by_folder(paths);
    if folders.is_empty() {
        return;
    }

    // Flattened once here so the callback matches against a plain list rather than walking
    // the map — which folder an event came from tells us nothing the name doesn't.
    let watched: Vec<String> = folders.values().flatten().cloned().collect();
    let live = Arc::new(AtomicBool::new(true));
    let alive = live.clone();
    let handle = app.clone();

    let mut debouncer = match new_debouncer(DEBOUNCE, move |res: DebounceEventResult| {
        if !alive.load(Ordering::SeqCst) {
            return;
        }
        let Ok(events) = res else { return };
        let seen: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
        for path in changed_paints(&watched, &seen) {
            log::info!("paint watcher: {path} changed on disk");
            let _ = handle.emit(EVENT, Changed { path });
        }
    }) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("paint watcher: couldn't start: {e}");
            return;
        }
    };

    let mut any = false;
    for dir in folders.keys() {
        // Non-recursive: a paints folder holds paints, and descending into whatever else
        // someone has parked beside them buys nothing.
        match debouncer.watcher().watch(dir, RecursiveMode::NonRecursive) {
            Ok(()) => {
                any = true;
                log::info!("paint watcher: watching {}", dir.display());
            }
            Err(e) => log::warn!("paint watcher: couldn't watch {}: {e}", dir.display()),
        }
    }
    if !any {
        return;
    }
    *state.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
        _debouncer: debouncer,
        live,
    });
}

pub fn stop(state: &PaintWatcher) {
    if let Some(running) = state.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
        running.live.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn paints_sharing_a_folder_cost_one_watch() {
        let folders = by_folder(&owned(&[
            "/mx/mods/bikes/KTM 450/paints/Frost.pnt",
            "/mx/mods/bikes/KTM 450/paints/Frost Blue.pnt",
            "/mx/mods/rider/helmets/Fox/paints/Frost.pnt",
        ]));
        assert_eq!(folders.len(), 2, "one watch per distinct folder");
        assert_eq!(
            folders[Path::new("/mx/mods/bikes/KTM 450/paints")].len(),
            2,
            "both bike paints ride the same watch",
        );
    }

    #[test]
    fn a_path_with_no_folder_is_not_watched() {
        // Nothing to register a watch on — and watching the process's cwd instead would be
        // a watch on somewhere the caller never named.
        assert!(by_folder(&owned(&["Frost.pnt"])).is_empty());
        assert!(by_folder(&owned(&["/"])).is_empty());
        assert!(by_folder(&[]).is_empty());
    }

    #[test]
    fn a_rewritten_paint_is_reported_under_the_name_the_caller_asked_about() {
        // The event carries the OS's spelling of the path — canonicalised, and on Windows
        // in whatever case the filesystem kept. The caller compares against the string it
        // handed over, so that is the string that must come back.
        let watched = owned(&["/mx/mods/bikes/KTM 450/paints/Frost.pnt"]);
        assert_eq!(
            changed_paints(
                &watched,
                &[PathBuf::from("/private/mx/mods/bikes/KTM 450/paints/FROST.PNT")],
            ),
            owned(&["/mx/mods/bikes/KTM 450/paints/Frost.pnt"]),
        );
    }

    #[test]
    fn the_folders_own_churn_is_ignored() {
        let watched = owned(&["/mx/paints/Frost.pnt"]);
        // A packer's scratch file, the folder itself, and a paint nobody asked about.
        let noise = [
            PathBuf::from("/mx/paints/Frost.pnt.tmp"),
            PathBuf::from("/mx/paints"),
            PathBuf::from("/mx/paints/Someone Else.pnt"),
        ];
        assert!(changed_paints(&watched, &noise).is_empty());
    }

    #[test]
    fn a_burst_of_writes_names_the_paint_once() {
        // Writing a multi-megabyte `.pnt` lands as many events inside one debounce window;
        // re-decoding it once per event would be several seconds of work for one save.
        let watched = owned(&["/mx/paints/Frost.pnt"]);
        let burst = vec![PathBuf::from("/mx/paints/Frost.pnt"); 5];
        assert_eq!(changed_paints(&watched, &burst), owned(&["/mx/paints/Frost.pnt"]));
    }

    fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
    }

    /// The claim the unit tests above can't make: a paint saved the way a packer saves one
    /// — write a temp file, rename it over the target — comes back out as an event.
    ///
    /// That rename is the whole reason the *folder* is watched. A watch on the file itself
    /// passes a naive "overwrite in place" test and then goes silent in the field, because
    /// the rename leaves it holding an inode nothing writes to again.
    #[test]
    fn a_paint_saved_by_rename_still_reaches_the_viewer() {
        use tauri::Listener;

        let dir = std::env::temp_dir().join(format!("frost-paintwatch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("make the paints folder");
        let paint = dir.join("Frost.pnt");
        std::fs::write(&paint, b"first").expect("install a paint");
        let watched = paint.to_string_lossy().to_string();

        let app = mock_app();
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let heard = Arc::clone(&seen);
        app.handle().listen(EVENT, move |e| {
            heard.lock().unwrap().push(e.payload().to_string());
        });

        let state = PaintWatcher::default();
        start(app.handle(), &state, std::slice::from_ref(&watched));

        // Let the watch register before touching the folder — a save that lands in the gap
        // is one FSEvents never had a reason to report.
        std::thread::sleep(Duration::from_millis(300));
        let staged = dir.join("Frost.pnt.tmp");
        std::fs::write(&staged, b"second").expect("stage the new paint");
        std::fs::rename(&staged, &paint).expect("rename it over the old one");

        // Generously past DEBOUNCE — this is a real filesystem, not a clock we control.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let hit = loop {
            if seen.lock().unwrap().iter().any(|p| p.contains("Frost.pnt")) {
                break true;
            }
            if std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        stop(&state);
        let payloads = seen.lock().unwrap().clone();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(hit, "a renamed-over paint must be reported; saw {payloads:?}");
        assert!(
            payloads.iter().all(|p| !p.contains(".tmp")),
            "the packer's scratch file is not a paint: {payloads:?}",
        );
    }
}
