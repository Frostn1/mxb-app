// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MXB Servers: one window for every mxbserver — health, build, session, riders and logs.
//!
//! P1 of the server-manager plan (`handoffs/server-manager.md`). Each server is reached over
//! SSH: its observe port gives `/readyz` and `/status` with no credentials, its admin port
//! (when configured) gives `/v1/riders` with a bearer token from the OS keychain, and the log
//! is read with `tail`. Nothing listens on the internet for this.

mod config;
mod local;
mod ssh;
mod store;

#[cfg(test)]
mod tests {
    #[test]
    fn local_tail_returns_the_last_lines() {
        let path =
            std::env::temp_dir().join(format!("mxb-servers-tail-{}.log", crate::store::new_id()));
        let text: String = (1..=50)
            .map(|i| {
                format!(
                    "line {i}
"
                )
            })
            .collect();
        std::fs::write(&path, text).unwrap();
        let tail = super::local_tail(path.to_str().unwrap(), 3).unwrap();
        assert_eq!(tail, ["line 48", "line 49", "line 50"]);
        assert_eq!(
            super::local_tail(path.to_str().unwrap(), 500)
                .unwrap()
                .len(),
            50
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn upload_names_are_made_safe_without_making_the_user_rename_them() {
        assert_eq!(
            super::safe_remote_filename("My Track (Final).pkz"),
            "My_Track_Final_.pkz"
        );
        assert_eq!(super::safe_remote_filename("track.pkz"), "track.pkz");
    }

    #[test]
    fn legacy_pairing_code_becomes_connection_fields() {
        use base64::Engine;
        let body = br#"{"url":"https://mx.example.com:9443","token":"0123456789abcdef"}"#;
        let code = format!(
            "mxb-agent:{}",
            base64::engine::general_purpose::STANDARD.encode(body)
        );
        let parsed = super::parse_legacy_pairing(&code).unwrap();
        assert_eq!(parsed.host, "mx.example.com");
        assert_eq!(parsed.port, 9443);
        assert!(parsed.tls);
        assert_eq!(parsed.token, "0123456789abcdef");
    }
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use store::{Server, ServerKind, Store};
use tauri::{Manager, RunEvent, State};

struct App {
    store: Store,
    tunnels: Arc<ssh::Tunnels>,
    http: reqwest::Client,
}

/// A server as the UI sees it: the stored fields and whether a token is in the keychain.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerView {
    #[serde(flatten)]
    server: Server,
    has_token: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyPairing {
    host: String,
    port: u16,
    tls: bool,
    token: String,
}

fn parse_legacy_pairing(blob: &str) -> Result<LegacyPairing, String> {
    use base64::Engine;
    let encoded = blob
        .trim()
        .strip_prefix("mxb-agent:")
        .ok_or("Paste the whole mxb-agent pairing line.")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| "That pairing code is damaged.".to_string())?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "That pairing code is damaged.".to_string())?;
    let raw_url = value["url"]
        .as_str()
        .ok_or("That pairing code has no address.")?;
    let token = value["token"]
        .as_str()
        .ok_or("That pairing code has no token.")?
        .trim();
    if !store::valid_agent_token(token) {
        return Err("That pairing code has an invalid token.".into());
    }
    let url =
        reqwest::Url::parse(raw_url).map_err(|_| "That pairing code has an invalid address.")?;
    let tls = match url.scheme() {
        "http" => false,
        "https" => true,
        _ => return Err("The agent address must use HTTP or HTTPS.".into()),
    };
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(
            "The agent address must not contain credentials, a path, query, or fragment.".into(),
        );
    }
    Ok(LegacyPairing {
        host: url
            .host_str()
            .ok_or("The agent address has no host.")?
            .to_string(),
        port: url
            .port_or_known_default()
            .ok_or("The agent address has no port.")?,
        tls,
        token: token.to_string(),
    })
}

#[tauri::command]
fn legacy_pairing(blob: String) -> Result<LegacyPairing, String> {
    parse_legacy_pairing(&blob)
}

fn view(server: Server) -> ServerView {
    let has_token = store::token(&server.id).is_some();
    ServerView { server, has_token }
}

#[tauri::command]
fn servers_list(app: State<'_, App>) -> Vec<ServerView> {
    app.store.load().into_iter().map(view).collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveRequest {
    server: Server,
    /// `None` keeps the stored token, `Some("")` removes it, anything else replaces it.
    token: Option<String>,
}

#[tauri::command]
fn servers_save(app: State<'_, App>, request: SaveRequest) -> Result<ServerView, String> {
    let mut server = request.server;
    let previous_kind = (!server.id.is_empty())
        .then(|| app.store.get(&server.id).ok().map(|saved| saved.kind))
        .flatten();
    server.name = server.name.trim().to_string();
    server.host = server.host.trim().to_string();
    server.key_path = server
        .key_path
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if server.id.is_empty() {
        server.id = store::new_id();
    }
    store::validate(&server)?;
    match request.token.as_deref().map(str::trim) {
        None if previous_kind.is_some_and(|kind| kind != server.kind) => {
            store::clear_token(&server.id)
        }
        None => {}
        Some("") => store::clear_token(&server.id),
        Some(token)
            if (server.kind == ServerKind::Native && store::valid_token(token))
                || (server.kind == ServerKind::Legacy && store::valid_agent_token(token)) =>
        {
            store::set_token(&server.id, token)?
        }
        Some(_) if server.kind == ServerKind::Legacy => {
            return Err("That doesn't look like an mxb-agent token.".into())
        }
        Some(_) => return Err("That doesn't look like an admin token (id.secret).".into()),
    }
    app.store.upsert(server.clone())?;
    app.tunnels.close_server(&server.id);
    Ok(view(server))
}

#[tauri::command]
fn servers_remove(app: State<'_, App>, id: String) -> Result<(), String> {
    app.tunnels.close_server(&id);
    store::clear_token(&id);
    app.store.remove(&id)
}

async fn local_port(app: &App, server: &Server, remote: u16) -> Result<u16, String> {
    let server = server.clone();
    // Opening a tunnel waits on ssh, so it runs on the blocking pool.
    let tunnels = Arc::clone(&app.tunnels);
    tauri::async_runtime::spawn_blocking(move || tunnels.port(&server, remote))
        .await
        .map_err(|e| e.to_string())?
}

/// GET `path` on one of the server's forwarded ports. A failed request drops the tunnel, so a
/// server that went away is reconnected on the next poll.
/// Why a request got no HTTP answer.
enum Miss {
    /// The SSH connection itself failed: the host is down, the key is wrong, ...
    Ssh(String),
    /// SSH is fine (or the server is on this PC) but nothing answered on the port.
    NoAnswer(String),
}

impl Miss {
    fn text(self) -> String {
        match self {
            Miss::Ssh(e) => format!("can't reach the server over SSH: {e}"),
            Miss::NoAnswer(e) => format!("no answer from the server: {e}"),
        }
    }
}

async fn get(
    app: &App,
    server: &Server,
    remote: u16,
    path: &str,
    token: Option<&str>,
) -> Result<(u16, String), String> {
    fetch(app, server, remote, path, token)
        .await
        .map_err(Miss::text)
}

/// `fetch_once`, and for a server over SSH one more try on a fresh tunnel: a request that fails
/// on an old tunnel may just mean the SSH connection dropped, and only a new one can tell
/// "SSH is down" from "the server isn't running".
async fn fetch(
    app: &App,
    server: &Server,
    remote: u16,
    path: &str,
    token: Option<&str>,
) -> Result<(u16, String), Miss> {
    match fetch_once(app, server, remote, path, token).await {
        Err(Miss::NoAnswer(_)) if !server.local => {
            fetch_once(app, server, remote, path, token).await
        }
        other => other,
    }
}

async fn fetch_once(
    app: &App,
    server: &Server,
    remote: u16,
    path: &str,
    token: Option<&str>,
) -> Result<(u16, String), Miss> {
    // A server on this PC is reached directly; any other through its SSH tunnel.
    let port = if server.local {
        remote
    } else {
        local_port(app, server, remote).await.map_err(Miss::Ssh)?
    };
    let mut request = app.http.get(format!("http://127.0.0.1:{port}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response
                .text()
                .await
                .map_err(|e| Miss::NoAnswer(e.to_string()))?;
            Ok((status, body))
        }
        Err(error) => {
            app.tunnels.reset(&server.id, remote);
            Err(Miss::NoAnswer(error.to_string()))
        }
    }
}

/// Call the authenticated mxb-agent used by Legacy connecting. Unlike native SSH servers,
/// this is a direct agent connection and therefore works with the official Windows host.
async fn legacy_request(
    app: &App,
    server: &Server,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<(u16, String), String> {
    let token = store::token(&server.id).ok_or("No mxb-agent token is saved for this server.")?;
    let host = if server.local {
        "127.0.0.1"
    } else {
        server.host.as_str()
    };
    let scheme = if server.agent_tls { "https" } else { "http" };
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let url = format!("{scheme}://{host}:{}{}", server.observe_port, path);
    let mut request = app.http.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("can't reach mxb-agent: {e}"))?;
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|e| e.to_string())?;
    Ok((status, text))
}

fn json_answer(code: u16, body: &str, label: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(body).map_err(|e| format!("{label}: {e}"))?;
    if (200..300).contains(&code) {
        Ok(value)
    } else {
        Err(value["error"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("{label} answered HTTP {code}")))
    }
}

/// One word for how a server is doing, and the detail behind it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusReport {
    /// "online" (ready for riders), "starting" (running, not ready yet), "offline" (reached,
    /// but the server isn't running) or "unreachable" (SSH itself failed).
    state: &'static str,
    detail: String,
    status: Option<Value>,
}

#[tauri::command]
async fn server_status(app: State<'_, App>, id: String) -> Result<StatusReport, String> {
    let server = app.store.get(&id)?;
    let report = |state, detail: String| StatusReport {
        state,
        detail,
        status: None,
    };
    if server.kind == ServerKind::Legacy {
        let (code, body) =
            match legacy_request(&app, &server, reqwest::Method::GET, "/status", None).await {
                Ok(answer) => answer,
                Err(error) => return Ok(report("unreachable", error)),
            };
        if code != 200 {
            return Ok(report(
                "unreachable",
                format!("mxb-agent answered HTTP {code}"),
            ));
        }
        let status: Value =
            serde_json::from_str(&body).map_err(|e| format!("legacy status: {e}"))?;
        let running = status["game"]["running"].as_bool().unwrap_or(false);
        return Ok(StatusReport {
            state: if running { "online" } else { "offline" },
            detail: if running {
                "Official dedicated server is running.".into()
            } else {
                "mxb-agent is connected; the dedicated server is stopped.".into()
            },
            status: Some(status),
        });
    }
    if !server.local {
        let tunnels = Arc::clone(&app.tunnels);
        let port = server.observe_port.to_string();
        let out = blocking(move || tunnels.run_script(&server, REMOTE_SH, &["observe", &port], 20))
            .await?;
        if !out.success {
            return Ok(report("offline", out.text()));
        }
        let encoded = out
            .field("status_b64")
            .ok_or("the server returned no status")?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| e.to_string())?;
        let status: Value = serde_json::from_slice(&bytes).map_err(|e| format!("/status: {e}"))?;
        let ready = out.field("ready") == Some("1");
        let build = status["build_id"].as_str().unwrap_or("?");
        return Ok(StatusReport {
            state: if ready { "online" } else { "starting" },
            detail: if ready {
                format!("ready for riders · build {build}")
            } else {
                format!("running but not ready for riders yet · build {build}")
            },
            status: Some(status),
        });
    }
    let (code, body) = match fetch(&app, &server, server.observe_port, "/status", None).await {
        Ok(answer) => answer,
        Err(Miss::Ssh(e)) => {
            return Ok(report(
                "unreachable",
                format!("SSH to {} failed: {e}", server.host),
            ))
        }
        Err(Miss::NoAnswer(e)) => {
            return Ok(report(
                "offline",
                format!(
                    "nothing answers on port {} ({e}); the server isn't running",
                    server.observe_port
                ),
            ))
        }
    };
    if code != 200 {
        return Ok(report("offline", format!("/status answered HTTP {code}")));
    }
    let status: Value = serde_json::from_str(&body).map_err(|e| format!("/status: {e}"))?;
    let ready = match fetch(&app, &server, server.observe_port, "/readyz", None).await {
        Ok((200, _)) => true,
        Ok(_) => false,
        Err(Miss::Ssh(e)) => {
            return Ok(report(
                "unreachable",
                format!("SSH to {} failed: {e}", server.host),
            ))
        }
        Err(Miss::NoAnswer(e)) => {
            return Ok(report(
                "offline",
                format!("the server stopped answering ({e})"),
            ))
        }
    };
    let build = status["build_id"].as_str().unwrap_or("?").to_string();
    Ok(StatusReport {
        state: if ready { "online" } else { "starting" },
        detail: if ready {
            format!("ready for riders · build {build}")
        } else {
            format!("running but not ready for riders yet · build {build}")
        },
        status: Some(status),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenCheck {
    ok: bool,
    message: String,
}

/// Try the saved admin token against `/v1/server` and say plainly what happened.
#[tauri::command]
async fn server_test_token(app: State<'_, App>, id: String) -> Result<TokenCheck, String> {
    let server = app.store.get(&id)?;
    let fail = |message: String| Ok(TokenCheck { ok: false, message });
    if server.kind == ServerKind::Legacy {
        return match legacy_request(&app, &server, reqwest::Method::GET, "/capabilities", None)
            .await
        {
            Ok((200, _)) => Ok(TokenCheck {
                ok: true,
                message: "mxb-agent connected.".into(),
            }),
            Ok((401, _)) => fail("mxb-agent refused this token.".into()),
            Ok((code, _)) => fail(format!("mxb-agent answered HTTP {code}.")),
            Err(error) => fail(error),
        };
    }
    let Some(admin) = server.admin_port else {
        return fail(
            "Set the admin port first (the [admin] listen port in the server's config).".into(),
        );
    };
    let Some(token) = store::token(&id) else {
        return fail("No admin token is saved for this server.".into());
    };
    match fetch(&app, &server, admin, "/v1/server", Some(&token)).await {
        Ok((200, _)) => Ok(TokenCheck {
            ok: true,
            message: "Token works.".into(),
        }),
        Ok((401, _)) => fail("The server refused this token: it's mistyped, or was revoked.".into()),
        Ok((403, _)) => fail("The token is valid but not allowed to read the server.".into()),
        Ok((429, _)) => fail("The server is rate-limiting; try again in a few seconds.".into()),
        Ok((code, _)) => fail(format!("The admin API answered HTTP {code}.")),
        Err(Miss::Ssh(e)) => fail(format!("Can't reach the server over SSH: {e}")),
        Err(Miss::NoAnswer(_)) => fail(format!(
            "Nothing answers on admin port {admin}. The server has no [admin] section, or it listens on another port."
        )),
    }
}

#[tauri::command]
async fn server_riders(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        let (code, body) =
            legacy_request(&app, &server, reqwest::Method::GET, "/players", None).await?;
        let value = json_answer(code, &body, "legacy riders")?;
        let riders = value["players"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|player| {
                serde_json::json!({
                    "connection_id": player["id"], "entity_id": null,
                    "name": player["name"], "bike": null, "state": "connected",
                    "connected_seconds": 0, "laps": 0, "best_lap_seconds": null,
                    "ping_ms": null, "guid": player["guid"]
                })
            })
            .collect::<Vec<_>>();
        return Ok(serde_json::json!({ "riders": riders }));
    }
    if !server.local {
        let tunnels = Arc::clone(&app.tunnels);
        let port = server.observe_port.to_string();
        let out = blocking(move || tunnels.run_script(&server, REMOTE_SH, &["riders", &port], 30))
            .await?;
        if !out.success {
            return Err(out.text());
        }
        return serde_json::from_str(&out.stdout).map_err(|e| format!("rider list: {e}"));
    }
    let admin = server
        .admin_port
        .ok_or("No admin port set for this server.")?;
    let token = store::token(&id).ok_or("No admin token saved for this server.")?;
    let (code, body) = get(&app, &server, admin, "/v1/riders", Some(&token)).await?;
    match code {
        200 => serde_json::from_str(&body).map_err(|e| format!("/v1/riders: {e}")),
        401 => Err("The server refused the admin token.".into()),
        429 => Err("Rate limited by the server; try again in a moment.".into()),
        other => Err(format!("/v1/riders answered {other}")),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackState {
    installed: Vec<String>,
    library: Vec<String>,
    current: Option<String>,
    rotation: Vec<String>,
}

#[tauri::command]
async fn server_tracks(app: State<'_, App>, id: String) -> Result<TrackState, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        let (tracks_code, tracks_body) =
            legacy_request(&app, &server, reqwest::Method::GET, "/tracks", None).await?;
        let tracks = json_answer(tracks_code, &tracks_body, "legacy tracks")?;
        let (status_code, status_body) =
            legacy_request(&app, &server, reqwest::Method::GET, "/status", None).await?;
        let status = json_answer(status_code, &status_body, "legacy status")?;
        let installed = tracks["tracks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        return Ok(TrackState {
            library: installed.clone(),
            installed,
            current: status["server"]["track"].as_str().map(str::to_string),
            rotation: Vec::new(),
        });
    }
    if server.local {
        return Err("Track management for a server on this PC is not wired yet.".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let state_tunnels = Arc::clone(&app.tunnels);
    let state_server = server.clone();
    let port = server.observe_port.to_string();
    let state_port = port.clone();
    let (out, state) = blocking(move || {
        let out = tunnels.run_script(&server, REMOTE_SH, &["tracks", &port], 30)?;
        let state = state_tunnels.run_script(
            &state_server,
            REMOTE_SH,
            &["track-state", &state_port],
            30,
        )?;
        Ok((out, state))
    })
    .await?;
    if !out.success || !state.success {
        return Err(format!("{}{}", out.text(), state.text()));
    }
    let body: Value = serde_json::from_str(&out.stdout).map_err(|e| format!("track list: {e}"))?;
    let status: Value =
        serde_json::from_str(&state.stdout).map_err(|e| format!("track state: {e}"))?;
    let installed = body["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let library = body["library"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let server = &status["server"];
    Ok(TrackState {
        installed,
        library,
        current: server["track"].as_str().map(str::to_string),
        rotation: server["rotation"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    })
}

#[tauri::command]
async fn server_track_membership(
    app: State<'_, App>,
    id: String,
    track: String,
    attached: bool,
) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err(
            "Legacy connecting uses the tracks installed in the official server's mods folder."
                .into(),
        );
    }
    if server.local {
        return Err("Shared track management requires a server connected over SSH.".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let encoded = b64(track.trim());
    let command = if attached {
        "attach-track"
    } else {
        "detach-track"
    };
    let out =
        blocking(move || tunnels.run_script(&server, REMOTE_SH, &[command, &port, &encoded], 30))
            .await?;
    if !out.success {
        return Err(out.text());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("track library: {e}"))
}

#[tauri::command]
async fn server_set_track(app: State<'_, App>, id: String, track: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        let payload = serde_json::json!({ "track": track.trim() });
        let (code, body) = legacy_request(
            &app,
            &server,
            reqwest::Method::PUT,
            "/config",
            Some(&payload),
        )
        .await?;
        return json_answer(code, &body, "select track");
    }
    if server.local {
        return Err("Track selection for a server on this PC is not wired yet.".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let encoded = b64(track.trim());
    let out = blocking(move || {
        tunnels.run_script(&server, REMOTE_SH, &["set-track", &port, &encoded], 180)
    })
    .await?;
    if !out.success {
        return Err(out.text());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("set track: {e}"))
}

#[tauri::command]
async fn server_set_rotation(
    app: State<'_, App>,
    id: String,
    tracks: Vec<String>,
) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err(
            "The official dedicated server has no live track rotation. Select one track at a time."
                .into(),
        );
    }
    if server.local {
        return Err("Track rotation for a server on this PC is not wired yet.".into());
    }
    let mut tracks = tracks
        .into_iter()
        .map(|track| track.trim().to_string())
        .filter(|track| !track.is_empty())
        .collect::<Vec<_>>();
    if tracks.is_empty() {
        return Err("Add at least one track.".into());
    }
    let current = tracks.remove(0);
    let payload = serde_json::json!({ "track": current, "rotation": tracks });
    let encoded = b64(&payload.to_string());
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(&server, REMOTE_SH, &["set-rotation", &port, &encoded], 180)
    })
    .await?;
    if !out.success {
        return Err(out.text());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("set rotation: {e}"))
}

#[tauri::command]
async fn server_update_github(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err("GitHub updates are only for mxbserver.".into());
    }
    if server.local {
        return Err("GitHub updates require a server connected over SSH.".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        let gh = |args: &[&str]| -> Result<std::process::Output, String> {
            let mut command = std::process::Command::new("gh");
            command.args(args);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            command.output().map_err(|_| "GitHub CLI is not installed. Install gh and sign in once to update from the private repository.".to_string())
        };
        let listed = gh(&["run", "list", "--repo", "Frostn1/mxbserver", "--workflow", "mxbserver Linux", "--branch", "main", "--status", "success", "--limit", "1", "--json", "databaseId,headSha"])?;
        if !listed.status.success() { return Err(String::from_utf8_lossy(&listed.stderr).trim().to_string()); }
        let runs: Vec<Value> = serde_json::from_slice(&listed.stdout).map_err(|e| format!("couldn't read GitHub build metadata: {e}"))?;
        let run = runs.first().ok_or("GitHub has no successful main build to install.")?;
        let run_id = run["databaseId"].as_u64().ok_or("GitHub build has no run id")?.to_string();
        let revision = run["headSha"].as_str().ok_or("GitHub build has no revision")?;
        let version = format!("main@{}", &revision[..revision.len().min(12)]);
        let dir = std::env::temp_dir().join(format!("mxb-servers-github-{}", store::new_id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create update folder: {e}"))?;
        let dir_text = dir.to_string_lossy().to_string();
        let downloaded = gh(&["run", "download", &run_id, "--repo", "Frostn1/mxbserver", "--name", "mxbserver-x86_64-unknown-linux-gnu-elf", "--dir", &dir_text])?;
        if !downloaded.status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(String::from_utf8_lossy(&downloaded.stderr).trim().to_string());
        }
        let binary = dir.join("mxbserver-x86_64-unknown-linux-gnu.elf");
        let mut file = std::fs::File::open(&binary).map_err(|e| format!("GitHub artifact has no server binary: {e}"))?;
        use sha2::Digest;
        use std::io::Read;
        let mut hash = sha2::Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop { let read = file.read(&mut buffer).map_err(|e| format!("couldn't read GitHub artifact: {e}"))?; if read == 0 { break; } hash.update(&buffer[..read]); }
        let digest = format!("{:x}", hash.finalize());
        let temporary = format!("mxb-servers-{}", store::new_id());
        tunnels.upload(&server, &binary.to_string_lossy(), &temporary, 300)?;
        let out = tunnels.run_script(&server, REMOTE_SH, &["agent-upload", &port, "version", &temporary, "mxbserver", &digest, &version], 360);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }).await?;
    if !out.success {
        return Err(out.text());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("GitHub update: {e}"))
}

#[tauri::command]
async fn server_session(
    app: State<'_, App>,
    id: String,
    action: String,
    to: Option<String>,
) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err("The official dedicated server does not expose live session controls through mxb-agent.".into());
    }
    if server.local {
        return Err("Live session control for a server on this PC is not wired yet.".into());
    }
    if !matches!(action.as_str(), "jump" | "advance" | "restart") {
        return Err("unknown session action".into());
    }
    let destination = to.unwrap_or_default();
    if action == "jump"
        && !matches!(
            destination.as_str(),
            "practice" | "qualifying" | "warmup" | "race"
        )
    {
        return Err("unknown session".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        let args = if action == "jump" {
            vec![
                "session",
                port.as_str(),
                action.as_str(),
                destination.as_str(),
            ]
        } else {
            vec!["session", port.as_str(), action.as_str()]
        };
        tunnels.run_script(&server, REMOTE_SH, &args, 30)
    })
    .await?;
    if !out.success {
        return Err(out.text());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("session control: {e}"))
}

#[tauri::command]
async fn server_upload(
    app: State<'_, App>,
    id: String,
    kind: String,
    path: String,
    version: Option<String>,
) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err("Legacy connecting uses tracks and game versions already installed on the official server host.".into());
    }
    if server.local {
        return Err("Uploads for a server on this PC are not wired yet.".into());
    }
    if !matches!(kind.as_str(), "track" | "version") {
        return Err("unknown upload type".into());
    }
    let file = std::path::Path::new(&path);
    let original_name = file
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("the file has no usable name")?
        .to_string();
    let name = if kind == "track" {
        safe_remote_filename(&original_name)
    } else {
        original_name
    };
    if kind == "version"
        && !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err("The binary file name contains unsupported characters.".into());
    }
    if kind == "track" && !name.to_ascii_lowercase().ends_with(".pkz") {
        return Err("Tracks must be .pkz packages.".into());
    }
    let version = version.unwrap_or_default();
    if kind == "version"
        && (version.trim().is_empty()
            || version.len() > 80
            || !version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')))
    {
        return Err(
            "Use a short version label with letters, numbers, dots, dashes, underscores, or +."
                .into(),
        );
    }
    let maximum = if kind == "track" {
        512 * 1024 * 1024
    } else {
        128 * 1024 * 1024
    };
    let digest = blocking({
        let path = path.clone();
        move || {
            use sha2::Digest;
            use std::io::Read;
            let mut file =
                std::fs::File::open(&path).map_err(|e| format!("could not read {path}: {e}"))?;
            let size = file.metadata().map_err(|e| e.to_string())?.len();
            if size == 0 || size > maximum {
                return Err(format!(
                    "the file must be between 1 byte and {} MiB",
                    maximum / 1024 / 1024
                ));
            }
            let mut hash = sha2::Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|e| format!("could not read {path}: {e}"))?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
            }
            Ok(format!("{:x}", hash.finalize()))
        }
    })
    .await?;
    let temporary = format!("mxb-servers-{}", store::new_id());
    let tunnels = Arc::clone(&app.tunnels);
    let upload_server = server.clone();
    let upload_path = path.clone();
    let upload_name = temporary.clone();
    blocking(move || tunnels.upload(&upload_server, &upload_path, &upload_name, 300)).await?;
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        let mut args = vec![
            "agent-upload",
            port.as_str(),
            kind.as_str(),
            temporary.as_str(),
            name.as_str(),
            digest.as_str(),
        ];
        if kind == "version" {
            args.push(version.as_str());
        }
        tunnels.run_script(&server, REMOTE_SH, &args, 360)
    })
    .await?;
    if !out.success {
        return Err(out.text());
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("upload: {e}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackUploadCheck {
    bytes: u64,
    server_track: bool,
    detail: String,
    upload_name: String,
}

fn safe_remote_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let safe = if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            c
        } else {
            '_'
        };
        if safe != '_' || !out.ends_with('_') {
            out.push(safe);
        }
    }
    let cleaned = out.trim_matches(['.', '_', '-']).to_string();
    if cleaned.is_empty() {
        "track.pkz".into()
    } else {
        cleaned
    }
}

#[tauri::command]
async fn inspect_track_upload(path: String) -> Result<TrackUploadCheck, String> {
    blocking(move || {
        let file = std::path::Path::new(&path);
        if !file
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("pkz"))
        {
            return Err("Choose a .pkz track package.".into());
        }
        let bytes = std::fs::metadata(file)
            .map_err(|e| format!("could not read the selected file: {e}"))?
            .len();
        let upload_name = safe_remote_filename(
            file.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("track.pkz"),
        );
        if bytes == 0 || bytes > 512 * 1024 * 1024 {
            return Err("Track packages must be between 1 byte and 512 MiB.".into());
        }
        match mxb_content::TrackPackage::open(file) {
            Ok(track) => Ok(TrackUploadCheck {
                bytes,
                server_track: true,
                detail: format!("Server track package · {}", track.id),
                upload_name,
            }),
            Err(error) => Ok(TrackUploadCheck {
                bytes,
                server_track: false,
                detail: format!("This does not look like a server track package: {error:#}"),
                upload_name,
            }),
        }
    })
    .await
}

#[tauri::command]
async fn server_logs(app: State<'_, App>, id: String, lines: u32) -> Result<Vec<String>, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        let (code, body) =
            legacy_request(&app, &server, reqwest::Method::GET, "/logs", None).await?;
        let value = json_answer(code, &body, "legacy logs")?;
        return Ok(value["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|line| line.as_str().map(str::to_string))
            .rev()
            .take(lines.min(500) as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect());
    }
    if server.local {
        return local_tail(&server.log_path, lines);
    }
    let tunnels = Arc::clone(&app.tunnels);
    tauri::async_runtime::spawn_blocking(move || tunnels.tail(&server, lines))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn legacy_config(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind != ServerKind::Legacy {
        return Err("This server does not use Legacy connecting.".into());
    }
    let (code, body) = legacy_request(&app, &server, reqwest::Method::GET, "/status", None).await?;
    json_answer(code, &body, "legacy settings")
}

#[tauri::command]
async fn legacy_config_save(
    app: State<'_, App>,
    id: String,
    name: String,
    track: String,
    max_clients: u32,
) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind != ServerKind::Legacy {
        return Err("This server does not use Legacy connecting.".into());
    }
    let name = name.trim();
    let track = track.trim();
    if name.is_empty()
        || name.len() > 100
        || track.is_empty()
        || max_clients == 0
        || max_clients > 50
    {
        return Err("Enter a server name, an installed track, and 1–50 riders.".into());
    }
    let payload = serde_json::json!({ "name": name, "track": track, "maxClients": max_clients });
    let (code, body) = legacy_request(
        &app,
        &server,
        reqwest::Method::PUT,
        "/config",
        Some(&payload),
    )
    .await?;
    json_answer(code, &body, "save legacy settings")
}

#[tauri::command]
async fn legacy_process(app: State<'_, App>, id: String, action: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind != ServerKind::Legacy || !matches!(action.as_str(), "start" | "stop" | "restart")
    {
        return Err("Unknown Legacy connecting action.".into());
    }
    let path = format!("/{action}");
    let (code, body) = legacy_request(&app, &server, reqwest::Method::POST, &path, None).await?;
    json_answer(code, &body, "legacy process control")
}

/// The last `lines` lines of a log file on this PC, reading at most its last 1 MiB.
fn local_tail(path: &str, lines: u32) -> Result<Vec<String>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let start = len.saturating_sub(1 << 20);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    // A read that starts mid-file drops its first, partial line.
    let skip = usize::from(start > 0 && !all.is_empty());
    let from = all.len().saturating_sub(lines as usize).max(skip);
    Ok(all[from..].iter().map(|l| l.to_string()).collect())
}

// ---- Config editing -------------------------------------------------------------------------

/// The server-side helper for config editing, sent over SSH on stdin each time.
const REMOTE_SH: &str = include_str!("remote.sh");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigState {
    text: String,
    sha: String,
    path: String,
    /// "systemd", "bare" (a hand-started process) or "local" (this PC).
    mode: String,
    values: serde_json::Map<String, Value>,
    fields: &'static [config::Field],
}

fn b64(text: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(text)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn config_load(app: State<'_, App>, id: String) -> Result<ConfigState, String> {
    let server = app.store.get(&id)?;
    let (text, path, mode) = if server.local {
        let (text, path) = blocking(move || local::read(&server)).await?;
        (text, path.display().to_string(), "local".to_string())
    } else {
        let tunnels = Arc::clone(&app.tunnels);
        let port = server.observe_port.to_string();
        let out =
            blocking(move || tunnels.run_script(&server, REMOTE_SH, &["read", &port], 30)).await?;
        let detect_mode = out.field("mode").unwrap_or("").to_string();
        let encoded = out
            .field("config_b64")
            .filter(|_| out.success)
            .ok_or_else(|| format!("could not read the config: {}", out.text()))?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8(bytes).map_err(|_| "the config is not UTF-8".to_string())?;
        (
            text,
            out.field("config").unwrap_or("").to_string(),
            detect_mode,
        )
    };
    let values = config::read(&text)?;
    Ok(ConfigState {
        sha: config::sha256(&text),
        text,
        path,
        mode,
        values,
        fields: config::FIELDS,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    text: String,
    diff: String,
}

/// The file with `changes` applied, and the diff. Pure: nothing leaves this PC.
#[tauri::command]
fn config_preview(
    base: String,
    changes: serde_json::Map<String, Value>,
) -> Result<Preview, String> {
    let text = config::apply(&base, &changes)?;
    let diff = config::diff(&base, &text);
    Ok(Preview { text, diff })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Checked {
    ok: bool,
    output: String,
}

/// Run the candidate through the server's own parser: the server binary, once, for a second,
/// on side ports, where the live config is.
#[tauri::command]
async fn config_validate(app: State<'_, App>, id: String, text: String) -> Result<Checked, String> {
    let server = app.store.get(&id)?;
    if server.local {
        let (ok, output) = blocking(move || local::validate(&server, &text)).await?;
        return Ok(Checked { ok, output });
    }
    let tunnels = Arc::clone(&app.tunnels);
    let encoded = b64(&text);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(&server, REMOTE_SH, &["validate", &port, &encoded], 120)
    })
    .await?;
    Ok(Checked {
        ok: out.field("valid") == Some("1"),
        output: out.text(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyResult {
    /// "applied", "rolled-back" (not ready after the change; the backup is live again) or
    /// "failed".
    result: String,
    backup: String,
    output: String,
}

/// Back up the live config, replace it with `text` (only if the live one still hashes to
/// `base_sha`), restart, and wait for `/readyz`; the backup goes back if it never comes.
#[tauri::command]
async fn config_apply(
    app: State<'_, App>,
    id: String,
    base_sha: String,
    text: String,
) -> Result<ApplyResult, String> {
    let server = app.store.get(&id)?;
    if !base_sha.chars().all(|c| c.is_ascii_hexdigit()) || base_sha.len() != 64 {
        return Err("bad config hash".into());
    }
    if server.local {
        let done = blocking(move || local::apply(&server, &base_sha, &text)).await?;
        return Ok(ApplyResult {
            result: done.result.to_string(),
            backup: done.backup,
            output: done.output,
        });
    }
    let tunnels = Arc::clone(&app.tunnels);
    let encoded = b64(&text);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(
            &server,
            REMOTE_SH,
            &["apply", &port, &encoded, &base_sha],
            300,
        )
    })
    .await?;
    match out.field("result") {
        Some(result) => Ok(ApplyResult {
            result: result.to_string(),
            backup: out.field("backup").unwrap_or("").to_string(),
            output: out.text(),
        }),
        None => Err(out.text()),
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                // Server connections are explicit; never leak their credentials to an OS proxy.
                .no_proxy()
                .build()?;
            app.manage(App {
                store: Store::new(dir),
                tunnels: Arc::default(),
                http,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            servers_list,
            servers_save,
            servers_remove,
            legacy_pairing,
            server_status,
            server_riders,
            server_tracks,
            server_track_membership,
            server_set_track,
            server_set_rotation,
            server_update_github,
            server_session,
            server_upload,
            inspect_track_upload,
            server_logs,
            server_test_token,
            legacy_config,
            legacy_config_save,
            legacy_process,
            config_load,
            config_preview,
            config_validate,
            config_apply,
        ])
        .build(tauri::generate_context!())
        .expect("error while building MXB Servers")
        .run(|handle, event| {
            // Close every ssh tunnel with the app.
            if let RunEvent::Exit = event {
                if let Some(app) = handle.try_state::<App>() {
                    app.tunnels.close_all();
                }
            }
        });
}
