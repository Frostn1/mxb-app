//! Friends: who is on your list, and telling them where you are.
//!
//! The control plane keeps the list and the presence (`control-plane/src/friends.ts`); this
//! side does two things it cannot. It reads the server name out of FrostMod's session block
//! and heartbeats it, and it proxies the panel's calls so the session token never reaches the
//! webview.
//!
//! ## Presence is a promise to the rider
//!
//! It is on by default for anyone who has an account, and `friends_presence` turns it off in
//! one switch. Off, nothing is sent, and a row already stored is deleted at once. The control
//! plane also has its own "hide my presence" for the same purpose; this is the local half, so
//! the app can stop reporting without a round trip.
//!
//! Only a server *name* is certain: FrostMod publishes it for every online session. An address
//! is known only when this app launched the game itself, and is attached only while the session
//! is still on the server that launch went to.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::paintsync::{client, control_plane};
use crate::voice::gamesession::Reader;
use crate::voice::session::room_key;

/// How often presence is reported. The control plane lets a report go stale after two minutes.
const BEAT: Duration = Duration::from_secs(30);

/// The address of the last server this app launched into, and the server name it turned out to
/// be once FrostMod reported one.
struct JoinNote {
    address: String,
    name: Option<String>,
}

static LAST_JOIN: Mutex<Option<JoinNote>> = Mutex::new(None);

/// Remember where this app just sent the game, so a heartbeat can carry the address too.
pub fn note_join(address: &str) {
    if let Ok(mut note) = LAST_JOIN.lock() {
        *note = Some(JoinNote { address: address.trim().to_string(), name: None });
    }
}

/// The remembered address, if the session is still on the server the launch reached.
///
/// The first session seen after a launch binds the address to that server's name. A later
/// session under another name means the rider moved on inside the game, and the address no
/// longer describes where they are, so it is dropped rather than guessed at.
fn address_for(server_name: &str) -> Option<String> {
    let mut guard = LAST_JOIN.lock().ok()?;
    let note = guard.as_mut()?;
    let key = room_key(server_name);
    match &note.name {
        None => note.name = Some(key),
        Some(bound) if *bound != key => {
            *guard = None;
            return None;
        }
        Some(_) => {}
    }
    guard.as_ref().map(|n| n.address.clone())
}

/// Forget the launch once there is no session, so the next one starts clean.
fn forget_join() {
    if let Ok(mut note) = LAST_JOIN.lock() {
        *note = None;
    }
}

/// The standing reporter. Call once, from `setup`.
pub fn start(app: &AppHandle) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let reader = Reader::default();
        // Whether the control plane currently holds a row for us, so a quiet app sends nothing.
        let mut reporting = false;
        loop {
            tokio::time::sleep(BEAT).await;
            let cfg = crate::config::load_or_detect(&app).unwrap_or_default();
            let token = cfg.cp_token.trim().to_string();
            let session = if cfg.friends_presence && !token.is_empty() {
                reader.read().filter(|s| s.on_a_server())
            } else {
                None
            };

            let Some(session) = session else {
                forget_join();
                if reporting && !token.is_empty() {
                    match send(&token, "DELETE", "/v1/friends/presence", None).await {
                        Ok(_) => reporting = false,
                        Err(e) => log::debug!("[friends] couldn't clear presence: {e}"),
                    }
                }
                continue;
            };

            let body = json!({
                "serverName": session.server_name.trim(),
                "address": address_for(&session.server_name),
                "track": Some(session.track_id.trim()).filter(|t| !t.is_empty()),
                "riders": u8::try_from(session.riders.len()).ok(),
            });
            match send(&token, "PUT", "/v1/friends/presence", Some(body)).await {
                Ok(_) => reporting = true,
                Err(e) => log::debug!("[friends] presence report failed: {e}"),
            }
        }
    });
}

/// One authenticated call to the control plane. A refusal comes back as the server's own
/// `error` sentence, which the panel shows as it is.
async fn send(token: &str, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    let client = client().map_err(|e| format!("{e:#}"))?;
    let url = format!("{}{}", control_plane(), path);
    let request = match method {
        "GET" => client.get(url),
        "PUT" => client.put(url),
        "POST" => client.post(url),
        "DELETE" => client.delete(url),
        other => return Err(format!("unsupported method {other}")),
    }
    .bearer_auth(token);
    let request = match body {
        Some(body) => request.json(&body),
        None => request,
    };
    let response = request.send().await.map_err(|e| format!("{e:#}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(parsed);
    }
    crate::gate::note_refusal(status.as_u16(), None);
    Err(parsed
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("friends request failed ({status})")))
}

/// The panel's calls. The account is claimed here if the app does not have one yet, which is
/// the same silent claim voice and the server queue make.
async fn call(app: &AppHandle, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    let token = crate::voice::signal::account(app, &cfg).await?;
    send(&token, method, path, body).await
}

#[tauri::command]
pub async fn friends_list(app: AppHandle) -> Result<Value, String> {
    call(&app, "GET", "/v1/friends", None).await
}

#[tauri::command]
pub async fn friends_search(app: AppHandle, query: String) -> Result<Value, String> {
    let mut url = reqwest::Url::parse("https://x.invalid/v1/friends/search").map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("q", query.trim());
    let path = format!("{}?{}", url.path(), url.query().unwrap_or_default());
    call(&app, "GET", &path, None).await
}

/// Ask by account id (from a search) or by friend code. Exactly one is given.
#[tauri::command]
pub async fn friends_request(
    app: AppHandle,
    account_id: Option<String>,
    friend_code: Option<String>,
) -> Result<Value, String> {
    let body = match (account_id, friend_code) {
        (Some(id), None) => json!({ "accountId": id }),
        (None, Some(code)) => json!({ "friendCode": code }),
        _ => return Err("give an account id or a friend code".into()),
    };
    call(&app, "POST", "/v1/friends/request", Some(body)).await
}

#[tauri::command]
pub async fn friends_respond(app: AppHandle, account_id: String, accept: bool) -> Result<Value, String> {
    call(&app, "POST", "/v1/friends/respond", Some(json!({ "accountId": account_id, "accept": accept }))).await
}

#[tauri::command]
pub async fn friends_remove(app: AppHandle, account_id: String) -> Result<Value, String> {
    if !account_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') || account_id.is_empty() {
        return Err("that isn't an account".into());
    }
    call(&app, "DELETE", &format!("/v1/friends/{account_id}"), None).await
}

/// The control plane's "hide my presence", which stops the row being stored at all.
#[tauri::command]
pub async fn friends_hide_presence(app: AppHandle, hide: bool) -> Result<Value, String> {
    call(&app, "PUT", "/v1/friends/settings", Some(json!({ "hidePresence": hide }))).await
}

/// The local switch: whether this app reports where the rider is. Off clears what was sent.
#[tauri::command]
pub async fn set_friends_presence(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = crate::config::load(&app).unwrap_or_default();
    cfg.friends_presence = enabled;
    crate::config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    if !enabled {
        forget_join();
        let token = cfg.cp_token.trim();
        if !token.is_empty() {
            let _ = send(token, "DELETE", "/v1/friends/presence", None).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, because the note is process-wide state and parallel tests would race on it.
    #[test]
    fn an_address_follows_its_server_and_not_the_next_one() {
        forget_join();
        assert_eq!(address_for("Fake Server"), None, "no launch, no address");

        note_join("203.0.113.7:54210");
        assert_eq!(address_for("  Fake   Server "), Some("203.0.113.7:54210".into()));
        assert_eq!(address_for("fake server"), Some("203.0.113.7:54210".into()), "same server, any spelling");

        assert_eq!(address_for("Another Fake Server"), None, "the rider moved on");
        assert_eq!(address_for("Fake Server"), None, "and the old address does not come back");

        note_join("198.51.100.2:54210");
        forget_join();
        assert_eq!(address_for("Fake Server"), None, "a session that ended forgets its launch");
    }
}
