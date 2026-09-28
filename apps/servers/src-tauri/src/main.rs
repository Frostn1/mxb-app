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
        let path = std::env::temp_dir().join(format!("mxb-servers-tail-{}.log", crate::store::new_id()));
        let text: String = (1..=50).map(|i| format!("line {i}
")).collect();
        std::fs::write(&path, text).unwrap();
        let tail = super::local_tail(path.to_str().unwrap(), 3).unwrap();
        assert_eq!(tail, ["line 48", "line 49", "line 50"]);
        assert_eq!(super::local_tail(path.to_str().unwrap(), 500).unwrap().len(), 50);
        std::fs::remove_file(path).unwrap();
    }
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use store::{Server, Store};
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
        None => {}
        Some("") => store::clear_token(&server.id),
        Some(token) if store::valid_token(token) => store::set_token(&server.id, token)?,
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
    fetch(app, server, remote, path, token).await.map_err(Miss::text)
}

async fn fetch(
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
    let (code, body) = match fetch(&app, &server, server.observe_port, "/status", None).await {
        Ok(answer) => answer,
        Err(Miss::Ssh(e)) => return Ok(report("unreachable", format!("SSH to {} failed: {e}", server.host))),
        Err(Miss::NoAnswer(e)) => {
            return Ok(report(
                "offline",
                format!("nothing answers on port {} ({e}); the server isn't running", server.observe_port),
            ))
        }
    };
    if code != 200 {
        return Ok(report("offline", format!("/status answered HTTP {code}")));
    }
    let status: Value = serde_json::from_str(&body).map_err(|e| format!("/status: {e}"))?;
    let ready = matches!(
        fetch(&app, &server, server.observe_port, "/readyz", None).await,
        Ok((200, _))
    );
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
    let Some(admin) = server.admin_port else {
        return fail("Set the admin port first (the [admin] listen port in the server's config).".into());
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

#[tauri::command]
async fn server_logs(app: State<'_, App>, id: String, lines: u32) -> Result<Vec<String>, String> {
    let server = app.store.get(&id)?;
    if server.local {
        return local_tail(&server.log_path, lines);
    }
    let tunnels = Arc::clone(&app.tunnels);
    tauri::async_runtime::spawn_blocking(move || tunnels.tail(&server, lines))
        .await
        .map_err(|e| e.to_string())?
}

/// The last `lines` lines of a log file on this PC, reading at most its last 1 MiB.
fn local_tail(path: &str, lines: u32) -> Result<Vec<String>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let start = len.saturating_sub(1 << 20);
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
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
        (text, out.field("config").unwrap_or("").to_string(), detect_mode)
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
fn config_preview(base: String, changes: serde_json::Map<String, Value>) -> Result<Preview, String> {
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
        tunnels.run_script(&server, REMOTE_SH, &["apply", &port, &encoded, &base_sha], 300)
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
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                // Only ever our own tunnels on 127.0.0.1; never an OS proxy.
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
            server_status,
            server_riders,
            server_logs,
            server_test_token,
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
