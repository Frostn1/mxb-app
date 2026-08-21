//! Notice when the game starts, and drive the things that need to happen when it does.
//!
//! A standing poll started at app launch. It notices when MX Bikes comes up — whether the
//! app launched it or Steam did — and on each new session re-arms FrostMod for it and checks
//! the mods folder is really on disk before the load screen reads it. It also holds a handle
//! on the running session, so how it ended is still readable once the process is gone (see
//! [`crate::gameproc::GameSession`]).

use crate::gameproc;
use std::time::Duration;
use tauri::AppHandle;

/// How often to ask whether the game has started. A process-table walk, and nothing else.
const POLL: Duration = Duration::from_secs(15);

/// Start the standing watcher. Call once, from `setup`.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut was_running = false;
        // A handle on the current session, held so how it ended is still readable once the
        // process is gone.
        let mut session: Option<gameproc::GameSession> = None;
        loop {
            let cfg = crate::config::load_or_detect(&app).unwrap_or_default();

            let running = gameproc::is_game_running();
            let started = running && !was_running;
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
                session = gameproc::GameSession::open();
                // The mods folder is read during the load screen, so a placeholder that
                // isn't really on disk becomes a crash there. Ask now, while there is still
                // a log line to attach the answer to.
                crate::cloudfiles::warn_if_dehydrated(&app, &cfg);
            }

            tokio::time::sleep(POLL).await;
        }
    });
}
