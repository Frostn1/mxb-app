//! Turning up on a server is the only thing a rider has to do to be in voice.
//!
//! This is the supervisor: a standing loop that watches which server the game is on and
//! keeps exactly one engine running for it. Joining, leaving and re-joining are consequences
//! of racing, not buttons — nobody picks a room, types an address, shares a code, or waits
//! for anyone to host anything.
//!
//! It also does the one bit of paperwork the player would otherwise have to: an app with no
//! control-plane account claims one, silently, the first time voice is switched on.
//!
//! ## Knowing which server
//!
//! FrostMod tells us. `EventInit` hands the plugin `m_szServerName`, and that name is the
//! room key — the only identifier every rider on a server has. An address reaches only the
//! riders whose app launched the game; anyone who picked the server from the game's own
//! browser never sees one, and a key half the grid cannot compute is a room that splits in
//! two without anybody noticing.
//!
//! So voice needs FrostMod running, which the app already installs and arms for every
//! session. Without it there is no server name, and without a server name there is nothing
//! to join — reported as such rather than guessed at.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::engine::{self, Status};
use super::gamesession::{GameSession, Reader};
use super::signal;

/// How often to reconcile "where the rider is" with "where voice is connected".
///
/// Voice starting a few seconds after the load screen is unnoticeable — the rider is looking
/// at a track, not at us — and polling is what keeps this robust to every way a session can
/// end, including the ones that don't tell anybody.
const POLL: Duration = Duration::from_secs(5);

/// How often to look for a change worth telling the UI about. Fast enough that a talking
/// indicator tracks speech rather than trailing it.
const STATUS_POLL: Duration = Duration::from_millis(150);

/// Whether the supervisor has been started, so a second call is a no-op rather than a second
/// loop fighting the first over the same engine.
static SUPERVISING: AtomicBool = AtomicBool::new(false);

/// The running engine, if any. Tauri state.
#[derive(Default)]
pub struct Session {
    running: Mutex<Option<Running>>,
    /// Holds FrostMod's shared block open across polls.
    game: Reader,
}

struct Running {
    server: String,
    handle: engine::Handle,
}

impl Session {
    pub fn status(&self) -> Status {
        match self.running.lock() {
            Ok(running) => running.as_ref().map(|r| r.handle.status()).unwrap_or_default(),
            Err(_) => Status::default(),
        }
    }

    pub fn send(&self, command: engine::Command) {
        if let Ok(running) = self.running.lock() {
            if let Some(running) = running.as_ref() {
                running.handle.send(command);
            }
        }
    }

    fn server(&self) -> Option<String> {
        self.running.lock().ok()?.as_ref().map(|r| r.server.clone())
    }

    /// Leave the room now, rather than at the next reconcile. The privacy control uses this:
    /// "off" has to mean the microphone is closed by the time the switch stops moving.
    pub fn leave(&self) {
        self.stop();
    }

    fn stop(&self) {
        if let Ok(mut running) = self.running.lock() {
            // Dropping the handle stops the thread, which closes the microphone. That the
            // mic closes when voice stops is not a detail — it is the promise the indicator
            // in the UI is making.
            if running.take().is_some() {
                log::info!("[voice] left the room");
            }
        }
    }

    fn set(&self, server: String, handle: engine::Handle) {
        if let Ok(mut running) = self.running.lock() {
            *running = Some(Running { server, handle });
        }
    }
}

/// Start the standing watcher. Call once, from `setup`.
pub fn start(app: &AppHandle) {
    if SUPERVISING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    let watcher = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(e) = reconcile(&app).await {
                log::debug!("[voice] {e}");
            }
            tokio::time::sleep(POLL).await;
        }
    });

    // The room changes far faster than it is reconciled — someone starts talking, stops,
    // connects — and a talking indicator that lagged five seconds behind would be worse than
    // none. Emitted only when something actually changed, so a quiet room costs a comparison.
    tauri::async_runtime::spawn(async move {
        let mut last: Option<Status> = None;
        loop {
            tokio::time::sleep(STATUS_POLL).await;
            let status = {
                use tauri::Manager;
                watcher.state::<Session>().status()
            };
            if last.as_ref() != Some(&status) {
                let _ = watcher.emit("voice-status", &status);
                last = Some(status);
            }
        }
    });
}

/// Make the world match: one engine for the server the rider is on, none otherwise.
async fn reconcile(app: &AppHandle) -> Result<(), String> {
    let session = app_session(app)?;
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();

    // Off, or the game is not on a server we can name: whatever is running should not be.
    // A rider in the menus, in a replay, or testing alone has nobody to talk to.
    let game = cfg.voice_enabled.then(|| session.game.read()).flatten();
    let Some(game) = game.filter(GameSession::on_a_server) else {
        if session.server().is_some() {
            session.stop();
        }
        return Ok(());
    };

    // The server name, as every rider on that server sees it. Trimmed and case-folded so
    // one server is one room however its name is punctuated on the day.
    let key = room_key(&game.server_name);

    if session.server().as_deref() == Some(key.as_str()) {
        // Already here. Keep presence fresh — it is what the room checks on the way in, so
        // a reconnect after a blip must not be turned away — and pass on a race number that
        // has arrived since we joined, which is what places this rider on the grid.
        if let Ok(token) = signal::account(app, &cfg).await {
            let _ = crate::paintsync::report_presence(&presence(&game, &key), &token).await;
        }
        session.send(engine::Command::Rider {
            rider_name: game.rider_name.clone(),
            race_num: game.race_num_for_room(),
        });
        return Ok(());
    }

    // Somewhere new.
    session.stop();

    let token = signal::account(app, &cfg).await?;

    // Presence before joining: the room admits a rider because the control plane already
    // believes they are on this server, so saying so has to come first.
    crate::paintsync::report_presence(&presence(&game, &key), &token)
        .await
        .map_err(|e| format!("couldn't report presence for voice: {e:#}"))?;

    let handle = engine::start(engine::Config {
        token,
        server_key: key.clone(),
        // The name the game knows this rider by, which is what other riders will see.
        rider_name: if game.rider_name.trim().is_empty() {
            signal::rider_name(&cfg)
        } else {
            game.rider_name.clone()
        },
        race_num: game.race_num_for_room(),
        input_device: cfg.voice_input_device.clone(),
        output_device: cfg.voice_output_device.clone(),
        input_gain: cfg.voice_input_gain,
        output_volume: cfg.voice_output_volume,
        stun_servers: stun_servers(),
    });
    log::info!("[voice] joining the room for \"{}\"", game.server_name);
    crate::usage::track("voice.join");
    session.set(key, handle);
    Ok(())
}

/// What to report about the server this rider is on.
///
/// The key is what the roster and the room are scoped by; the rest is what the server
/// browser is made of. Voice is where it comes from because voice is already reading the
/// running game every few seconds — the same block that names the server names its track and
/// counts its grid, so the report describes the place at no extra cost.
fn presence(game: &GameSession, key: &str) -> crate::paintsync::Presence {
    let some = |s: &str| {
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_string())
    };
    crate::paintsync::Presence {
        key: key.to_string(),
        server_name: some(&game.server_name),
        track: some(&game.track_id),
        riders: Some(game.riders.len()),
    }
}

/// The room key for a server name.
///
/// Folded so that trailing spaces or a change of capitalisation cannot split one server's
/// riders into two rooms. Not hashed: the key is also what the app shows the player, and a
/// name they recognise is worth more than an opaque id.
pub fn room_key(server_name: &str) -> String {
    server_name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Where to ask what this machine looks like from outside.
///
/// Resolved every time rather than cached: this runs once per join, and a DNS answer held
/// across a laptop moving between networks is worth less than the lookup costs.
fn stun_servers() -> Vec<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    ["stun.cloudflare.com:3478", "stun.l.google.com:19302"]
        .iter()
        .filter_map(|host| host.to_socket_addrs().ok()?.find(|a| a.is_ipv4()))
        .collect()
}

fn app_session(app: &AppHandle) -> Result<tauri::State<'_, Session>, String> {
    use tauri::Manager;
    Ok(app.state::<Session>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_server_is_one_room_however_its_name_is_written() {
        // Every rider on a server computes this from the same `EventInit` string, so the
        // only way they end up apart is if this function disagrees with itself.
        let expected = "frost racing eu";
        for name in ["Frost Racing EU", "frost racing eu", "  Frost Racing EU  ", "Frost  Racing\tEU"] {
            assert_eq!(room_key(name), expected, "{name:?}");
        }
    }

    #[test]
    fn different_servers_stay_different_rooms() {
        assert_ne!(room_key("Frost Racing EU"), room_key("Frost Racing US"));
    }

    #[test]
    fn finds_somewhere_to_ask_about_our_own_address() {
        // Not a network test: it only asserts the hostnames parse and resolve to something
        // usable, which is the failure that would silently leave every rider unreachable.
        let servers = stun_servers();
        assert!(!servers.is_empty(), "no STUN server resolved");
        assert!(servers.iter().all(|s| s.is_ipv4()));
    }
}
