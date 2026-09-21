//! The line for a full server.
//!
//! The control plane keeps the order (`control-plane/src/serverqueue.ts`); this side does
//! what it can't: read the server's own rider count with `GETINFO` and launch the game when
//! a slot is ours. One line at a time, beating every [`BEAT`].
//!
//! The game only reads `-directconnect` at startup, so a game that's already open can't be
//! steered. Then the turn is announced instead and the rider joins from the in-game browser,
//! unless they turned on `queue_restart_game`: then the game is closed and launched again.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::gameproc::{self, LaunchOutcome};
use crate::paintsync::control_plane;

/// How often the app checks the server and heartbeats. Well inside the control plane's 45 s TTL.
const BEAT: Duration = Duration::from_secs(10);

/// How long a launched game gets to reach the server before we stop waiting on it.
const LAUNCH_WAIT: Duration = Duration::from_secs(180);

/// How long an announced turn is held for a rider whose game was already open.
const TURN_HOLD: Duration = Duration::from_secs(120);

/// How long an open game gets to close before we fall back to announcing the turn.
const CLOSE_WAIT: Duration = Duration::from_secs(15);

/// A pause between the game closing and launching it again, so Steam sees it gone.
const SETTLE: Duration = Duration::from_secs(3);

/// The event the frontend listens on.
const EVENT: &str = "server-queue";

/// Error codes the frontend translates. Anything else is a launch error, shown as is.
const MISSED: &str = "missed";
const OFFLINE: &str = "offline";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// In line.
    Waiting,
    /// A slot is ours but the game is already open: join from the in-game browser.
    Turn,
    /// We launched the game into the slot.
    Launched,
    /// On the server. The line is done.
    Joined,
    /// Left, missed the turn, or the game never got there.
    Ended,
}

/// What the banner shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueState {
    pub address: String,
    pub name: String,
    pub phase: Phase,
    pub position: u32,
    pub waiting: u32,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Place {
    ahead: u32,
    waiting: u32,
    /// Ask the server ourselves next beat. Only the front of the line does, unless the shared
    /// count went stale. Missing from an older control plane, which means everyone probes.
    #[serde(default = "yes")]
    probe: bool,
    /// The line's latest rider count, from whoever probed.
    #[serde(default)]
    players: Option<u32>,
    #[serde(default)]
    max_players: Option<u32>,
}

fn yes() -> bool {
    true
}

/// The line we're in, and a generation that ends any older loop.
static ACTIVE: Mutex<(u64, Option<QueueState>)> = Mutex::new((0, None));

/// Our turn when the free slots cover everyone ahead of us.
pub fn is_my_turn(players: u32, max: u32, ahead: u32) -> bool {
    players.saturating_add(ahead) < max
}

/// Get in line for `address`, leaving any other line first.
pub async fn join(
    app: AppHandle,
    address: String,
    name: String,
    categories: Vec<String>,
    bikes: Vec<String>,
) -> Result<QueueState, String> {
    let key = gameproc::parse_server_address(&address).map_err(|e| format!("{e:#}"))?;
    let cfg = crate::config::load_or_detect(&app).unwrap_or_default();
    let token = crate::voice::signal::account(&app, &cfg).await?;

    let place = beat(&token, &key, false, None)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let state = QueueState {
        address: key.clone(),
        name,
        phase: Phase::Waiting,
        position: place.ahead + 1,
        waiting: place.waiting,
        players: None,
        max_players: None,
        error: None,
    };
    let generation = {
        let mut active = ACTIVE.lock().unwrap();
        active.0 += 1;
        active.1 = Some(state.clone());
        active.0
    };
    let _ = app.emit(EVENT, &state);
    tauri::async_runtime::spawn(run(
        app,
        token,
        key,
        generation,
        place.probe,
        categories,
        bikes,
    ));
    Ok(state)
}

/// Leave the line.
pub async fn leave(app: AppHandle) {
    let ended = {
        let mut active = ACTIVE.lock().unwrap();
        active.0 += 1;
        active.1.take()
    };
    let cfg = crate::config::load_or_detect(&app).unwrap_or_default();
    if !cfg.cp_token.trim().is_empty() {
        if let Err(e) = remove(&cfg.cp_token).await {
            log::debug!("[queue] leave failed: {e:#}");
        }
    }
    if let Some(mut state) = ended {
        state.phase = Phase::Ended;
        let _ = app.emit(EVENT, &state);
    }
}

/// The line we're in, if any.
pub fn status() -> Option<QueueState> {
    ACTIVE.lock().unwrap().1.clone()
}

/// How many are waiting on each server, for the detail panel.
pub async fn counts(app: &AppHandle, addresses: Vec<String>) -> Result<Vec<(String, u32)>, String> {
    #[derive(Deserialize)]
    struct Resp {
        servers: std::collections::HashMap<String, u32>,
    }
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    let token = cfg.cp_token.trim();
    let keys: Vec<String> = addresses
        .iter()
        .filter_map(|a| gameproc::parse_server_address(a).ok())
        .take(8)
        .collect();
    if token.is_empty() || keys.is_empty() {
        return Ok(Vec::new());
    }
    let resp: Resp = client()
        .map_err(|e| format!("{e:#}"))?
        .get(format!("{}/v1/queue/counts", control_plane()))
        .query(&[("server", keys.join(","))])
        .bearer_auth(token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("{e:#}"))?
        .json()
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(resp.servers.into_iter().collect())
}

/// The loop behind one line. Ends when a newer [`join`] or a [`leave`] bumps the generation.
async fn run(
    app: AppHandle,
    token: String,
    key: String,
    generation: u64,
    mut probe_next: bool,
    categories: Vec<String>,
    bikes: Vec<String>,
) {
    let mut launched_at: Option<Instant> = None;
    let mut turn_at: Option<Instant> = None;
    // The server an already-open game was on when the turn came. Still being there isn't a join.
    let mut turn_from: Option<String> = None;

    loop {
        tokio::time::sleep(BEAT).await;
        let Some(mut state) = current(generation) else {
            return;
        };

        if launched_at.is_some() || turn_at.is_some() {
            let on = on_server();
            if on.is_some() && (launched_at.is_some() || on != turn_from) {
                return finish(&app, &token, generation, state, Phase::Joined, None).await;
            }
            if launched_at.is_some_and(|t| t.elapsed() > LAUNCH_WAIT) {
                // No FrostMod session to confirm it (or the join failed); stop holding the slot.
                return finish(&app, &token, generation, state, Phase::Ended, None).await;
            }
            if turn_at.is_some_and(|t| t.elapsed() > TURN_HOLD) {
                let missed = Some(MISSED.to_string());
                return finish(&app, &token, generation, state, Phase::Ended, missed).await;
            }
        }

        // Only the rider the control plane picked asks the server; the rest read its answer.
        let own = if probe_next {
            crate::probe_server(key.clone())
                .await
                .ok()
                .map(|s| (s.players, s.max_players))
        } else {
            None
        };
        let claimed = launched_at.is_some() || turn_at.is_some();
        let place = match beat(&token, &key, claimed, own).await {
            Ok(p) => p,
            Err(e) => {
                log::debug!("[queue] heartbeat failed: {e:#}");
                state.error = Some(OFFLINE.to_string());
                publish(&app, generation, state);
                continue;
            }
        };
        state.error = None;
        state.position = place.ahead + 1;
        state.waiting = place.waiting;
        probe_next = place.probe;
        let count = own.or(place.players.zip(place.max_players));
        if let Some((players, max)) = count {
            state.players = Some(players);
            state.max_players = Some(max);
        }

        if !claimed {
            if let Some((players, max)) = count {
                if is_my_turn(players, max, place.ahead) {
                    let mut outcome = take_turn(&app, &key, &categories, &bikes);
                    if matches!(outcome, Ok(LaunchOutcome::AlreadyRunning))
                        && close_open_game(&app).await
                    {
                        outcome = take_turn(&app, &key, &categories, &bikes);
                    }
                    match outcome {
                        Ok(LaunchOutcome::Launched) => {
                            launched_at = Some(Instant::now());
                            state.phase = Phase::Launched;
                        }
                        Ok(LaunchOutcome::AlreadyRunning) => {
                            turn_at = Some(Instant::now());
                            turn_from = on_server();
                            state.phase = Phase::Turn;
                            attention(&app);
                        }
                        Err(e) => {
                            return finish(&app, &token, generation, state, Phase::Ended, Some(e))
                                .await;
                        }
                    }
                    // Marks the slot as ours so the riders behind don't rush it too.
                    if let Err(e) = beat(&token, &key, true, None).await {
                        log::debug!("[queue] claiming the slot failed: {e:#}");
                    }
                }
            }
        }
        publish(&app, generation, state);
    }
}

/// Launch into the slot. Reports `AlreadyRunning` rather than touching an open game.
fn take_turn(
    app: &AppHandle,
    key: &str,
    categories: &[String],
    bikes: &[String],
) -> Result<LaunchOutcome, String> {
    crate::join_listed_server(
        app.clone(),
        key.to_string(),
        categories.to_vec(),
        bikes.to_vec(),
    )
}

/// Close an open game so the turn can launch into the slot, if the rider turned that on.
async fn close_open_game(app: &AppHandle) -> bool {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    if !cfg.queue_restart_game {
        return false;
    }
    close_and_settle().await
}

/// Close the game and give Steam a moment to notice, then say whether it went.
///
/// Shared with the Servers tab's own Close & join, which is the same two problems in a row:
/// the game reads the connect flag only at startup, and a launch that follows the close too
/// closely meets a Steam that still has the old session down as running.
pub(crate) async fn close_and_settle() -> bool {
    let closed = tauri::async_runtime::spawn_blocking(|| gameproc::close_game(CLOSE_WAIT))
        .await
        .unwrap_or(false);
    if closed {
        tokio::time::sleep(SETTLE).await;
    }
    closed
}

/// The server FrostMod says the game is on, if any.
fn on_server() -> Option<String> {
    crate::live_session()
        .filter(|s| s.on_a_server())
        .map(|s| s.server_name)
}

/// Flash the window so a rider in another app sees their turn.
fn attention(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.request_user_attention(Some(tauri::UserAttentionType::Critical));
    }
}

/// The state for `generation`, or `None` once the line has moved on.
fn current(generation: u64) -> Option<QueueState> {
    let active = ACTIVE.lock().unwrap();
    (active.0 == generation).then(|| active.1.clone()).flatten()
}

fn publish(app: &AppHandle, generation: u64, state: QueueState) {
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.0 != generation {
            return;
        }
        active.1 = Some(state.clone());
    }
    let _ = app.emit(EVENT, &state);
}

async fn finish(
    app: &AppHandle,
    token: &str,
    generation: u64,
    mut state: QueueState,
    phase: Phase,
    error: Option<String>,
) {
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.0 != generation {
            return;
        }
        active.1 = None;
    }
    if let Err(e) = remove(token).await {
        log::debug!("[queue] leaving after the turn failed: {e:#}");
    }
    state.phase = phase;
    state.error = error;
    let _ = app.emit(EVENT, &state);
}

/// Heartbeat, carrying the server's `(players, max)` when we were the one who probed.
async fn beat(
    token: &str,
    key: &str,
    launched: bool,
    count: Option<(u32, u32)>,
) -> anyhow::Result<Place> {
    let mut body = serde_json::json!({ "server": key, "launched": launched });
    if let Some((players, max)) = count {
        body["players"] = players.into();
        body["maxPlayers"] = max.into();
    }
    Ok(client()?
        .put(format!("{}/v1/queue", control_plane()))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

async fn remove(token: &str) -> anyhow::Result<()> {
    client()?
        .delete(format!("{}/v1/queue", control_plane()))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::is_my_turn;

    #[test]
    fn turn_needs_a_slot_for_everyone_ahead() {
        assert!(!is_my_turn(20, 20, 0));
        assert!(is_my_turn(19, 20, 0));
        assert!(!is_my_turn(19, 20, 1));
        assert!(is_my_turn(18, 20, 1));
        assert!(!is_my_turn(0, 0, 0));
        assert!(!is_my_turn(u32::MAX, 20, 5));
    }
}
