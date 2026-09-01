//! The control-plane end of voice: get an account, get into the room, relay ICE.
//!
//! Everything here exists to keep the player from having to do anything. They do not sign up,
//! they are not given a code, and they never see this. The app claims an account for itself
//! the first time voice is switched on, and from then on it is a token in the config file
//! like any other setting.
//!
//! The room itself is a WebSocket to a Durable Object named after the server. Joining one
//! that nobody has used before and joining one with ten riders already in it are the same
//! request — there is nothing to create and nothing to look up first.
//!
//! Blocking, on its own thread, talking to the engine over channels. Signalling is a few
//! kilobytes at join and near-silence afterwards, so an async runtime would be all cost.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

use crate::config::AppConfig;
use crate::paintsync::control_plane;

/// How long to wait on the network before deciding the control plane isn't answering.
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// A peer in the room, as the control plane describes them.
///
/// Both fields are that peer's own claim about themselves. Nothing here is proof: the app
/// decides whether to *play* someone by looking them up in the game's own list of who is on
/// the grid, which no stranger can write to.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Peer {
    #[serde(rename = "peerId")]
    pub peer_id: String,
    #[serde(rename = "riderName", default)]
    pub rider_name: String,
    #[serde(rename = "raceNum", default)]
    pub race_num: u16,
}

/// What the room tells us.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomEvent {
    /// We are in. `peers` are the riders already here — we offer to each of them.
    Welcome { peer_id: String, peers: Vec<Peer> },
    Joined(Peer),
    Left { peer_id: String },
    /// A peer's rider name or race number changed — they joined a session, or left one.
    Rider(Peer),
    Signal { from: String, kind: String, data: String },
    /// The socket is gone. The engine decides whether to reconnect.
    Closed(String),
}

/// What we tell the room.
#[derive(Debug, Clone)]
pub enum RoomCommand {
    Rider { rider_name: String, race_num: u16 },
    Signal { to: String, kind: String, data: String },
    Leave,
}

// ---------------------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ClaimedAccount {
    token: String,
    #[serde(rename = "riderName")]
    rider_name: String,
}

/// The token this app talks to the control plane with, claiming one if it has none.
///
/// An enrolled player already has a token and keeps it — same account, same paints. Everyone
/// else gets a self-serve one, silently, the first time voice or paint sync needs one. That
/// is the difference between "install the app and ride" and "install the app, then go and
/// ask someone for an invite code", and it is the whole reason the endpoint exists.
///
/// Returns the token and whether it is new, so the caller can save the config exactly once.
pub async fn ensure_account(cfg: &AppConfig) -> Result<(String, bool), String> {
    let existing = cfg.cp_token.trim();
    if !existing.is_empty() {
        return Ok((existing.to_string(), false));
    }

    let rider_name = rider_name(cfg);
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("Couldn't reach MXB App's service: {e}"))?;
    let resp = client
        .post(format!("{}/v1/account", control_plane()))
        .json(&serde_json::json!({ "riderName": rider_name }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach MXB App's service: {e}"))?;

    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("Too many new accounts from this connection today. Try again tomorrow.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("MXB App's service turned down the sign-up ({}).", resp.status()));
    }
    let claimed: ClaimedAccount = resp
        .json()
        .await
        .map_err(|e| format!("MXB App's service sent something unexpected: {e}"))?;
    log::info!("[cp] claimed an account as {}", claimed.rider_name);
    Ok((claimed.token, true))
}

/// The token, claimed and saved, ready to use.
///
/// [`ensure_account`] hands back whether the account is new because only the caller can
/// write the config; every caller then does the same thing with the answer, so it lives
/// here once. Voice and paint sync share one account — they are the same rider on the same
/// server, and a second identity would put their name on the grid twice.
pub async fn account(app: &tauri::AppHandle, cfg: &AppConfig) -> Result<String, String> {
    let (token, is_new) = ensure_account(cfg).await?;
    if is_new {
        // Re-read before writing: `save` rewrites the whole file and this ran across an
        // await, so the config on disk may have moved on.
        let mut cfg = crate::config::load_or_detect(app).unwrap_or_default();
        cfg.cp_token = token.clone();
        if cfg.cp_rider_name.trim().is_empty() {
            cfg.cp_rider_name = rider_name(&cfg);
        }
        if let Err(e) = crate::config::save(app, &cfg) {
            log::warn!("[cp] couldn't save the claimed account: {e:#}");
        }
    }
    Ok(token)
}

/// The best guess at what this player is called in game.
///
/// A label, not an identity — it is what other riders see beside a talking indicator. The
/// enrolled name first, then the profile folder the game keeps (which *is* the rider name),
/// then a placeholder, because not knowing must never be a reason voice won't start.
pub fn rider_name(cfg: &AppConfig) -> String {
    let enrolled = cfg.cp_rider_name.trim();
    if !enrolled.is_empty() {
        return enrolled.to_string();
    }
    if let Some(profile) = crate::presets::list_profiles(&cfg.profiles_dir()).into_iter().next() {
        if !profile.trim().is_empty() {
            return profile;
        }
    }
    "Rider".to_string()
}

// ---------------------------------------------------------------------------------------
// The room socket
// ---------------------------------------------------------------------------------------

/// A live connection to one server's voice room.
pub struct Room {
    commands: Sender<RoomCommand>,
    events: Receiver<RoomEvent>,
}

impl Room {
    /// Open the room for `server_key`, introducing ourselves as `rider_name`.
    ///
    /// Returns as soon as the socket is up; the `Welcome` arrives as the first event.
    pub fn join(
        token: &str,
        server_key: &str,
        rider_name: &str,
        race_num: u16,
    ) -> Result<Room, String> {
        let url = room_url(&control_plane(), server_key);
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| format!("Couldn't build the voice room address: {e}"))?;
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse().map_err(|_| "bad token")?);

        let (mut socket, _) = tungstenite::connect(request).map_err(describe_connect_failure)?;

        let hello = serde_json::json!({ "t": "hello", "riderName": rider_name, "raceNum": race_num });
        socket
            .send(Message::Text(hello.to_string()))
            .map_err(|e| format!("Couldn't say hello to the voice room: {e}"))?;

        let (command_tx, command_rx) = std::sync::mpsc::channel::<RoomCommand>();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<RoomEvent>();

        std::thread::Builder::new()
            .name("voice-room".into())
            .spawn(move || pump(socket, command_rx, event_tx))
            .map_err(|e| format!("Couldn't start the voice room thread: {e}"))?;

        Ok(Room { commands: command_tx, events: event_rx })
    }

    /// Everything the room has said since the last call. Never blocks.
    pub fn drain(&self) -> Vec<RoomEvent> {
        self.events.try_iter().collect()
    }

    pub fn send(&self, command: RoomCommand) {
        // A closed channel means the pump thread is gone, which the engine learns from the
        // `Closed` event. Nothing useful to do with the error here.
        let _ = self.commands.send(command);
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.send(RoomCommand::Leave);
    }
}

/// `https://host` → `wss://host/v1/voice/room?server=…`, and `http` → `ws` for local runs.
fn room_url(base: &str, server_key: &str) -> String {
    let scheme = if base.starts_with("http://") { "ws://" } else { "wss://" };
    let host = base.trim_end_matches('/').trim_start_matches("http://").trim_start_matches("https://");
    let key = urlencoding_encode(server_key);
    format!("{scheme}{host}/v1/voice/room?server={key}")
}

/// Percent-encode a server key for a query string. Keys are host:port or a registry id, so
/// this only ever has a colon or a dot to deal with — but it is a URL, so it is encoded.
fn urlencoding_encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn describe_connect_failure(err: tungstenite::Error) -> String {
    match &err {
        tungstenite::Error::Http(resp) if resp.status() == 403 => {
            "The voice service doesn't think you're on this server yet.".to_string()
        }
        tungstenite::Error::Http(resp) if resp.status() == 401 => {
            "This app's voice account was rejected. Try turning voice off and on again.".to_string()
        }
        _ => format!("Couldn't join the voice room: {err}"),
    }
}

/// The socket thread: commands out, events in, until either end goes away.
fn pump(
    mut socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    commands: Receiver<RoomCommand>,
    events: Sender<RoomEvent>,
) {
    // Non-blocking reads, so one thread serves both directions without a second socket.
    set_nonblocking(socket.get_ref());

    let mut reason = String::from("the voice room closed");
    loop {
        // Anything the engine wants said.
        let mut leaving = false;
        for command in commands.try_iter() {
            let text = match command {
                RoomCommand::Rider { rider_name, race_num } => {
                    serde_json::json!({ "t": "rider", "riderName": rider_name, "raceNum": race_num })
                }
                RoomCommand::Signal { to, kind, data } => {
                    serde_json::json!({ "t": "signal", "to": to, "kind": kind, "data": data })
                }
                RoomCommand::Leave => {
                    leaving = true;
                    serde_json::json!({ "t": "bye" })
                }
            };
            if socket.send(Message::Text(text.to_string())).is_err() {
                reason = "lost the connection to the voice room".into();
                leaving = true;
                break;
            }
        }
        if leaving {
            break;
        }

        // Anything the room has said.
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Some(event) = parse_event(&text) {
                    if events.send(event).is_err() {
                        // The engine dropped the room; nobody is listening any more.
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Nothing to read. Signalling is quiet by nature, so sleeping here is right;
                // the engine's own loop is the one that must not.
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                reason = format!("the voice room connection failed: {e}");
                break;
            }
        }
    }

    let _ = socket.close(None);
    let _ = events.send(RoomEvent::Closed(reason));
}

/// Reach through whatever TLS wrapper the socket has to the TCP stream underneath.
fn set_nonblocking(stream: &tungstenite::stream::MaybeTlsStream<std::net::TcpStream>) {
    use tungstenite::stream::MaybeTlsStream;
    let tcp = match stream {
        MaybeTlsStream::Plain(tcp) => tcp,
        MaybeTlsStream::Rustls(tls) => tls.get_ref(),
        // Another TLS backend we didn't enable. Blocking reads still work; the command side
        // just waits for the next frame before it is sent, which signalling can afford.
        _ => return,
    };
    let _ = tcp.set_nonblocking(true);
}

/// One frame from the room. Unknown types are ignored rather than fatal, so the control
/// plane can add a message the shipped app doesn't know about.
pub fn parse_event(text: &str) -> Option<RoomEvent> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    match value.get("t")?.as_str()? {
        "welcome" => Some(RoomEvent::Welcome {
            peer_id: value.get("peerId")?.as_str()?.to_string(),
            peers: serde_json::from_value(value.get("peers")?.clone()).ok()?,
        }),
        "joined" => Some(RoomEvent::Joined(
            serde_json::from_value(value.get("peer")?.clone()).ok()?,
        )),
        "left" => Some(RoomEvent::Left { peer_id: value.get("peerId")?.as_str()?.to_string() }),
        "rider" => Some(RoomEvent::Rider(serde_json::from_value(value.clone()).ok()?)),
        "signal" => Some(RoomEvent::Signal {
            from: value.get("from")?.as_str()?.to_string(),
            kind: value.get("kind")?.as_str()?.to_string(),
            data: value.get("data")?.as_str()?.to_string(),
        }),
        "error" => {
            log::warn!("[voice] room error: {}", value.get("error").and_then(|e| e.as_str()).unwrap_or("?"));
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_secure_room_url() {
        assert_eq!(
            room_url("https://cp.example.com", "203.0.113.10:54210"),
            "wss://cp.example.com/v1/voice/room?server=203%2E0%2E113%2E10%3A54210"
        );
    }

    #[test]
    fn a_local_control_plane_stays_unencrypted() {
        // `wrangler dev` serves plain http; forcing wss there would make local testing
        // impossible for the one feature that most needs it.
        assert!(room_url("http://127.0.0.1:8799", "x").starts_with("ws://127.0.0.1:8799/"));
    }

    #[test]
    fn reads_the_room_saying_we_are_in() {
        let event = parse_event(
            r#"{"t":"welcome","peerId":"me","peers":[{"peerId":"p2","riderName":"Frost","raceNum":7}]}"#,
        );
        assert_eq!(
            event,
            Some(RoomEvent::Welcome {
                peer_id: "me".into(),
                peers: vec![Peer { peer_id: "p2".into(), rider_name: "Frost".into(), race_num: 7 }],
            })
        );
    }

    #[test]
    fn reads_a_peer_with_no_race_number_yet() {
        // Someone who is in the app but not yet on track.
        let event = parse_event(r#"{"t":"joined","peer":{"peerId":"p3"}}"#);
        assert_eq!(
            event,
            Some(RoomEvent::Joined(Peer { peer_id: "p3".into(), rider_name: String::new(), race_num: 0 }))
        );
    }

    #[test]
    fn reads_a_relayed_offer() {
        let event = parse_event(r#"{"t":"signal","from":"p2","kind":"offer","data":"v=0"}"#);
        assert_eq!(
            event,
            Some(RoomEvent::Signal { from: "p2".into(), kind: "offer".into(), data: "v=0".into() })
        );
    }

    #[test]
    fn ignores_what_it_doesnt_understand() {
        assert_eq!(parse_event("not json"), None);
        assert_eq!(parse_event(r#"{"t":"something-new","x":1}"#), None);
        assert_eq!(parse_event(r#"{"t":"welcome"}"#), None, "a welcome with no peer id is not one");
    }
}
