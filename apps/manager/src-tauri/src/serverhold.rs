//! Holds disruptive FrostMod reloads until an online session is safely over.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::frostmod::ReloadKind;

/// Event transitions clear the server name for a median 35 seconds and p90 about 200 seconds.
pub(crate) const LINGER: Duration = Duration::from_secs(300);

static LAST_ON_SERVER: Mutex<Option<Instant>> = Mutex::new(None);
static PENDING: Mutex<Option<ReloadKind>> = Mutex::new(None);

pub(crate) fn holding(last_on_server: Option<Instant>, now: Instant, game_running: bool) -> bool {
    game_running
        && last_on_server.is_some_and(|at| now.saturating_duration_since(at) < LINGER)
}

fn next_last_on_server(
    prev: Option<Instant>,
    on_server_read: bool,
    game_running: bool,
    now: Instant,
) -> Option<Instant> {
    if !game_running {
        None
    } else if on_server_read {
        Some(now)
    } else {
        prev
    }
}

fn merge(a: Option<ReloadKind>, b: ReloadKind) -> ReloadKind {
    match (a, b) {
        (None, kind) => kind,
        (Some(a), b) if a == b => a,
        (Some(ReloadKind::Full), _) | (Some(_), ReloadKind::Full) => ReloadKind::Full,
        (Some(_), _) => ReloadKind::Full,
    }
}

pub(crate) fn on_server_now() -> bool {
    let now = Instant::now();
    let game_running = crate::gameproc::is_game_running();
    let on_server_read = crate::game_on_a_server();
    let mut last = LAST_ON_SERVER.lock().unwrap_or_else(|e| e.into_inner());
    *last = next_last_on_server(*last, on_server_read, game_running, now);
    let is_holding = holding(*last, now, game_running);
    drop(last);

    if !game_running {
        let dropped = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take().is_some();
        if dropped {
            log::info!("[frostmod] held reload dropped: the game exited");
        }
    }
    is_holding
}

pub(crate) fn hold(kind: ReloadKind, why: &str) {
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    let first = pending.is_none();
    *pending = Some(merge(*pending, kind));
    if first {
        log::info!(
            "[frostmod] {kind:?} reload held: the game is on a server (or left one under 5 min ago); \
             a reload mid-session drops every rider. It runs once the game has been off a server \
             for 5 minutes. ({why})"
        );
    }
}

pub(crate) fn start() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(2));
        if on_server_now() {
            continue;
        }
        let Some(kind) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            continue;
        };
        if on_server_now() {
            let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
            *pending = Some(merge(*pending, kind));
            continue;
        }
        if !crate::gameproc::is_game_running() {
            log::info!("[frostmod] held reload dropped: the game exited");
            continue;
        }
        let _ = crate::frostmod::signal_held_kind_now(kind);
        log::info!("[frostmod] held {kind:?} reload sent: off a server for 5 minutes");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_tracks_linger_and_process() {
        let now = Instant::now();
        assert!(holding(Some(now), now, true));
        assert!(holding(Some(now), now + Duration::from_secs(240), true));
        assert!(!holding(Some(now), now + LINGER, true));
        assert!(!holding(Some(now), now, false));
        assert!(!holding(None, now, true));
    }

    #[test]
    fn server_reads_only_advance_or_clear_state() {
        let before = Instant::now();
        let now = before + Duration::from_secs(1);
        assert_eq!(next_last_on_server(Some(before), false, true, now), Some(before));
        assert_eq!(next_last_on_server(Some(before), true, true, now), Some(now));
        assert_eq!(next_last_on_server(Some(before), true, false, now), None);
    }

    #[test]
    fn pending_kinds_merge_safely() {
        assert_eq!(merge(Some(ReloadKind::Full), ReloadKind::Gear), ReloadKind::Full);
        assert_eq!(merge(Some(ReloadKind::Gear), ReloadKind::Tracks), ReloadKind::Full);
        assert_eq!(merge(Some(ReloadKind::Gear), ReloadKind::Gear), ReloadKind::Gear);
    }
}
