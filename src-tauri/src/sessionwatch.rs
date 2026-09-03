//! Notice when the game starts, and drive the things that need to happen when it does.
//!
//! A standing poll started at app launch. It notices when MX Bikes comes up — whether the
//! app launched it or Steam did — and on each new session re-arms FrostMod for it and checks
//! the mods folder is really on disk before the load screen reads it. It also holds a handle
//! on the running session, so how it ended is still readable once the process is gone (see
//! [`crate::gameproc::GameSession`]).
//!
//! While a session is up it is also where [`crate::procmods`] reports what the game has
//! loaded. That is a second job for one poll rather than a second poll: this loop is already
//! the one thing that knows a game is running, and it knows it whether the game came from
//! the Play button or from Steam.

use crate::gameproc;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// How often to ask whether the game has started. A process-table walk, and nothing else.
const POLL: Duration = Duration::from_secs(15);

/// How often to look at what the running game has loaded. Slower than the poll above: the
/// answer barely moves within a session, and the report is only sent when it does. Matched
/// to the live paint sync, which is the other thing running through a race.
const REPORT_EVERY: Duration = Duration::from_secs(45);

/// Start the standing watcher. Call once, from `setup`.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut was_running = false;
        // A handle on the current session, held so how it ended is still readable once the
        // process is gone.
        let mut session: Option<gameproc::GameSession> = None;
        // When the game's module list was last looked at. `None` until a session starts, so
        // the first pass of every session reports rather than waiting out the interval.
        let mut reported: Option<Instant> = None;
        loop {
            let cfg = crate::config::load_or_detect(&app).unwrap_or_default();

            let running = gameproc::is_game_running();
            let started = running && !was_running;
            if running != was_running {
                // Publish the transition: the mods watcher holds its reloads while a session
                // is young, because that is when the game is walking the whole content tree.
                gameproc::note_session(running);
            }
            was_running = running;

            // Checked every pass, not only when the poll says the game is gone: the handle
            // is what knows the process ended, and it knows it exactly.
            if let Some(open) = session.take() {
                session = open.report_if_ended();
            }

            if started {
                // Re-arm FrostMod for the new session — whether it was launched from Steam,
                // the desktop, or the Play button.
                crate::frostmod_manage::on_game_started(&app, &cfg);
                // Paint sync, for the sessions the app didn't start. Most players open the
                // game from Steam or a shortcut, and until this was here those sessions
                // synced with nobody — the Play button was the only way in.
                crate::sync_on_game_started(&app, &cfg);
                session = gameproc::GameSession::open();
                // The mods folder is read during the load screen, so a placeholder that
                // isn't really on disk becomes a crash there. Ask now, while there is still
                // a log line to attach the answer to.
                crate::cloudfiles::warn_if_dehydrated(&app, &cfg);
                // A new session is a new answer, whatever the last one said.
                crate::procmods::reset();
                reported = None;
            }

            if running {
                // Who this player is, from the game rather than from the disk. Every pass
                // rather than once a session: `EventInit` is what carries the GUID and the
                // rider name, and it fires when they enter a session, which can be long
                // after the process started. A pass with nothing new costs a shared-memory
                // read and two string compares.
                if let Some(seen) = crate::seen_identity() {
                    crate::identity::claim_from_game(&app, &seen).await;
                }

                if reported.is_none_or(|at| at.elapsed() >= REPORT_EVERY) {
                    reported = Some(Instant::now());
                    // Off the runtime, not on it. The first pass of a session reads every
                    // non-system module the game has loaded — hash, signature and version
                    // resource — and that is disk I/O and a trust check per file, seconds of
                    // it on a cold cache. Held here it would stall every other async task in
                    // the app, the updater and paint sync among them, for the length of it.
                    let handle = app.clone();
                    tauri::async_runtime::spawn_blocking(move || crate::procmods::tick(&handle));
                }
            } else if reported.take().is_some() {
                // The session is over, so nothing that was true of it is true now.
                crate::procmods::reset();
            }

            tokio::time::sleep(POLL).await;
        }
    });
}
