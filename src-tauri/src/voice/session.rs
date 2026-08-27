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
//! Today the answer comes from us: the app launched the game with `-directconnect`, so it
//! knows. A rider who picks a server from the game's own browser is invisible to us until
//! FrostMod reports it — the RE is settled (an import hook on `recvfrom`; the connect-string
//! buffer only ever holds what the command line put there) and it is the next phase. Until
//! then [`set_current_server`] is the one place that knowledge enters, and there will be
//! exactly one more caller of it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::engine::{self, Status};
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

/// The server the game is on, as best we know. Empty when it isn't on one.
static CURRENT_SERVER: Mutex<String> = Mutex::new(String::new());

/// Whether the supervisor has been started, so a second call is a no-op rather than a second
/// loop fighting the first over the same engine.
static SUPERVISING: AtomicBool = AtomicBool::new(false);

/// The running engine, if any. Tauri state.
#[derive(Default)]
pub struct Session {
    running: Mutex<Option<Running>>,
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

/// Tell voice which server the game is on. Empty means "not on one".
pub fn set_current_server(address: &str) {
    if let Ok(mut current) = CURRENT_SERVER.lock() {
        *current = address.trim().to_string();
    }
}

fn current_server() -> String {
    CURRENT_SERVER.lock().map(|s| s.clone()).unwrap_or_default()
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

    // Off, or the game is gone: whatever is running should not be.
    if !cfg.voice_enabled || !crate::gameproc::is_game_running() {
        if session.server().is_some() {
            session.stop();
            set_current_server("");
        }
        return Ok(());
    }

    let address = current_server();
    if address.is_empty() {
        // On a server we can't name yet. Nothing to join — and nothing to apologise for
        // until FrostMod can tell us (see the module docs).
        if session.server().is_some() {
            session.stop();
        }
        return Ok(());
    }

    // The same key paint sync uses, so both features agree on what "this server" means and a
    // registered server is recognised however its address was written.
    let registry = crate::paintsync::registry(None).await.unwrap_or_default();
    let key = crate::paintsync::server_key_for(&registry, &address);

    if session.server().as_deref() == Some(key.as_str()) {
        // Already connected here. Keep presence fresh — it is what the room checks on the
        // way in, and a reconnect after a blip must not be turned away.
        if let Ok((token, _)) = signal::ensure_account(&cfg).await {
            let _ = crate::paintsync::report_presence(&token, &key).await;
        }
        return Ok(());
    }

    // Somewhere new.
    session.stop();

    let (token, is_new) = signal::ensure_account(&cfg).await?;
    if is_new {
        // Re-read before writing: `save` rewrites the whole file and this ran across an
        // await, so the config on disk may have moved on.
        let mut cfg = crate::config::load_or_detect(app).unwrap_or_default();
        cfg.cp_token = token.clone();
        if cfg.cp_rider_name.trim().is_empty() {
            cfg.cp_rider_name = signal::rider_name(&cfg);
        }
        if let Err(e) = crate::config::save(app, &cfg) {
            log::warn!("[voice] couldn't save the voice account: {e:#}");
        }
    }

    // Presence before joining: the room admits a rider because the control plane already
    // believes they are on this server, so saying so has to come first.
    crate::paintsync::report_presence(&token, &key)
        .await
        .map_err(|e| format!("couldn't report presence for voice: {e:#}"))?;

    let handle = engine::start(engine::Config {
        token,
        server_key: key.clone(),
        rider_name: signal::rider_name(&cfg),
        // Zero until FrostMod says otherwise: a rider in the room but not yet on the grid.
        race_num: 0,
        input_device: cfg.voice_input_device.clone(),
        output_device: cfg.voice_output_device.clone(),
        input_gain: cfg.voice_input_gain,
        output_volume: cfg.voice_output_volume,
        stun_servers: stun_servers(),
    });
    log::info!("[voice] joining the room for {key}");
    session.set(key, handle);
    Ok(())
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
    fn remembers_the_server_it_was_told_about() {
        set_current_server("  203.0.113.10:54210 ");
        assert_eq!(current_server(), "203.0.113.10:54210");
        set_current_server("");
        assert_eq!(current_server(), "");
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
