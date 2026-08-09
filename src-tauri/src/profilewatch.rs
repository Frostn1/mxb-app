//! Notice when the *game* changes what the rider is wearing.
//!
//! Paint sync publishes whenever the app writes a look. That covers the Locker and the
//! Manage tab, and misses the way most riders actually change kit: in the game's own garage.
//! MX Bikes writes the same `profile.ini` the app does, so the file itself is the signal —
//! there just wasn't anything listening to it.
//!
//! `modwatch` deliberately stays out of `profiles/`, and its reasoning still holds: that
//! folder churns all through a session with replays, telemetry and settings, and a watcher
//! reacting to all of it would fire constantly mid-race. This one is not that watcher. It
//! filters to exactly `profile.ini` — never the `profile.ini.bak` that `presets::apply_loadout`
//! writes alongside it — which is a file that changes only when a look does.
//!
//! It also makes no attempt to tell the game's writes from the app's. It doesn't need to:
//! `paintsync::publish_all` hashes the look and sends nothing when the digest is unchanged,
//! so a redundant fire costs one local hash and no request. That is the whole reason this
//! can be a dumb watcher rather than a careful one.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::AppHandle;

/// Debounce window. Writing a look touches the file once, but an editor or the game may
/// rewrite it in two steps; this collapses that into a single answer.
const DEBOUNCE: Duration = Duration::from_millis(1500);

/// The one file worth reacting to.
const PROFILE_INI: &str = "profile.ini";

/// Whether a changed path is a look changing.
///
/// Everything else under `profiles/` — replays, telemetry, settings, and the `.bak` the app
/// writes on every apply — is noise. Matched case-insensitively because the folder is
/// Windows' and a Proton prefix reproduces whatever case the game used.
pub fn is_profile_ini(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case(PROFILE_INI))
        .unwrap_or(false)
}

pub struct Running {
    _debouncer: Debouncer<RecommendedWatcher>,
    live: Arc<AtomicBool>,
}

/// Tauri-managed handle, so the watcher can be replaced when the profiles folder moves.
#[derive(Default)]
pub struct ProfileWatcher(pub Mutex<Option<Running>>);

/// Start watching `profiles_dir` for look changes, replacing any watcher already running.
///
/// Best-effort throughout: a missing folder or a refused watch logs and returns. Losing this
/// costs the immediacy of an in-game change being published, not correctness — the publish at
/// launch catches the same change a moment later.
pub fn start(app: &AppHandle, state: &ProfileWatcher, profiles_dir: &Path) {
    stop(state);
    if !profiles_dir.is_dir() {
        log::info!("profile watcher: {} isn't there, not watching", profiles_dir.display());
        return;
    }

    let live = Arc::new(AtomicBool::new(true));
    let app_handle = app.clone();
    let alive = live.clone();

    let mut debouncer = match new_debouncer(DEBOUNCE, move |res: DebounceEventResult| {
        if !alive.load(Ordering::SeqCst) {
            return;
        }
        let Ok(events) = res else { return };
        if !events.iter().any(|e| is_profile_ini(&e.path)) {
            return;
        }
        // Which profile changed is not passed on: a rider has one identity, and
        // `publish_paints_soon` resolves it the same way every other caller does.
        log::info!("profile watcher: a look changed on disk");
        crate::publish_look_now(&app_handle);
    }) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("profile watcher: couldn't start: {e}");
            return;
        }
    };

    // Recursive because a profile is a folder under this one. The filename filter is what
    // keeps that from turning into the mid-race noise `modwatch` warns about.
    if let Err(e) = debouncer.watcher().watch(profiles_dir, RecursiveMode::Recursive) {
        log::warn!("profile watcher: couldn't watch {}: {e}", profiles_dir.display());
        return;
    }
    log::info!("profile watcher: watching {}", profiles_dir.display());
    *state.0.lock().unwrap() = Some(Running { _debouncer: debouncer, live });
}

pub fn stop(state: &ProfileWatcher) {
    if let Some(running) = state.0.lock().unwrap().take() {
        // Retires any callback already in flight; the debouncer's thread may outlive the
        // handle by a moment.
        running.live.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reacts_to_a_profile_but_not_to_its_backup() {
        // `presets::apply_loadout` writes the `.bak` on every apply, so treating it as a
        // change would make the app's own writes look like the game's.
        assert!(is_profile_ini(&PathBuf::from("/mx/profiles/Frost/profile.ini")));
        assert!(!is_profile_ini(&PathBuf::from("/mx/profiles/Frost/profile.ini.bak")));
    }

    #[test]
    fn ignores_everything_else_the_folder_churns_out() {
        // The reason `modwatch` stays out of `profiles/` entirely: this is what a session
        // writes there while it runs.
        for noise in [
            "/mx/profiles/Frost/replays/moto1.rpy",
            "/mx/profiles/Frost/telemetry/lap.bin",
            "/mx/profiles/Frost/settings.ini",
            "/mx/profiles/Frost/profile.tmp",
            "/mx/profiles",
        ] {
            assert!(!is_profile_ini(&PathBuf::from(noise)), "{noise} must be ignored");
        }
    }

    #[test]
    fn matches_whatever_case_the_game_wrote() {
        // The folder is Windows', and a Proton prefix keeps the case the game chose.
        for ok in ["/mx/profiles/Frost/Profile.INI", "/mx/profiles/Frost/PROFILE.ini"] {
            assert!(is_profile_ini(&PathBuf::from(ok)), "{ok} must be recognised");
        }
    }
}
