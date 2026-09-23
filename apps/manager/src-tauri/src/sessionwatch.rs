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
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// How often to ask whether the game has started. A process-table walk, and nothing else.
const POLL: Duration = Duration::from_secs(15);

/// How often to look at what the running game has loaded. Slower than the poll above: the
/// answer barely moves within a session, and the report is only sent when it does. Matched
/// to the live paint sync, which is the other thing running through a race.
const REPORT_EVERY: Duration = Duration::from_secs(45);

/// A slow module/signature pass must never pile up behind the next heartbeat.
struct Exclusive(AtomicBool);

impl Exclusive {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    fn try_enter(&self) -> Option<ExclusiveGuard<'_>> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| ExclusiveGuard(self))
    }
}

struct ExclusiveGuard<'a>(&'a Exclusive);

impl Drop for ExclusiveGuard<'_> {
    fn drop(&mut self) {
        self.0.0.store(false, Ordering::Release);
    }
}

static PROCMODS_PASS: Exclusive = Exclusive::new();

/// Start the standing watcher. Call once, from `setup`.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Whatever the last session left behind, before this one starts. A crash with the
        // app closed is the ordinary case, and it would otherwise sit there until the player
        // happened to play again and quit again.
        {
            let cfg = crate::config::load_or_detect(&app).unwrap_or_default();
            send_crash_reports(&app, &cfg).await;
            // And the crash the game has not had yet: a trainer file carrying leftover
            // memory takes it down at track load. Before the first launch of this run is
            // the moment to clear it.
            //
            // Only while the game is down. A running game holds its own idea of these
            // files and rewrites them when it exits, so repairing underneath it would be
            // both undone and, for the file it has open, a rename it could fight. The app
            // is normally started before the game; when it is not, the session-end pass
            // below catches everything anyway.
            if !gameproc::is_game_running() {
                crate::trainerfix::repair_and_report(&app, &cfg);
            }
        }

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
                    if let Some(pass) = PROCMODS_PASS.try_enter() {
                        let handle = app.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            let _pass = pass;
                            crate::procmods::tick(&handle)
                        });
                    } else {
                        log::debug!("[diag] previous module inspection is still running; skipping this beat");
                    }
                }
            } else if reported.take().is_some() {
                // The session is over, so nothing that was true of it is true now.
                crate::procmods::reset();
                // And if it ended by crashing, FrostMod left a report behind. This is the
                // first moment it can be sent: the game is gone, so the file is finished and
                // nothing is competing for the disk.
                send_crash_reports(&app, &cfg).await;
                // The session just wrote its trainers, and that write is where the damage
                // gets in. Clear it now rather than on the load screen that would crash.
                crate::trainerfix::repair_and_report(&app, &cfg);
            }

            tokio::time::sleep(POLL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_diagnostic_pass_cannot_overlap_the_next_beat() {
        let pass = Exclusive::new();
        let first = pass.try_enter().expect("first pass starts");
        assert!(pass.try_enter().is_none(), "a second pass must be skipped");
        drop(first);
        assert!(pass.try_enter().is_some(), "the next beat starts after completion");
    }
}

/// Hand anything FrostMod left after a crash to [`crate::crashreports`].
///
/// Here rather than inline because both callers want it and neither wants to know where
/// FrostMod's folder is or which build the game on disk is.
async fn send_crash_reports(app: &AppHandle, cfg: &crate::config::AppConfig) {
    let dir = crate::frostmod_manage::frostmod_dir(app);
    if !crate::crashreports::any_pending(&dir) {
        return;
    }
    let version = app.package_info().version.to_string();
    // Which build the crash's offsets are offsets into. The usual build fingerprint is read
    // out of the running process, and by now the process is the thing that died — so this is
    // the game executable's own digest, off the disk. Computed only when there is a report
    // waiting, which is the rare case. Empty if it cannot be read, which is honest: a site
    // with no build against it is still a site, it just cannot be compared across an update.
    let exe = std::path::PathBuf::from(cfg.install_dir()).join(cfg.game().exe);
    let build = crate::paintsync::sha256_file(&exe).unwrap_or_default();
    crate::crashreports::flush(&version, cfg, &dir, &build).await;
}
