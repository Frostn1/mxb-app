// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MXB Servers: one window for every mxbserver — health, build, session, riders and logs.
//!
//! Each server is reached over SSH: its observe port gives `/readyz` and `/status` with no credentials, and its admin API
//! (the `[admin] listen` of its config, default 127.0.0.1:9810) takes riders and session
//! controls with a bearer token from the OS keychain. Config, `systemctl` and `journalctl -u
//! mxbserver` go through `remote.sh` over SSH. There is no mxb-agent. Nothing listens on the
//! internet for this.

mod config;
mod local;
mod ssh;
mod store;
mod uploads;

#[cfg(test)]
mod tests {
    #[test]
    fn the_admin_port_comes_from_the_listen_address() {
        assert_eq!(super::parse_admin_listen("127.0.0.1:9810"), Some(9810));
        assert_eq!(super::parse_admin_listen("[::1]:9811\n"), Some(9811));
        assert_eq!(super::parse_admin_listen("127.0.0.1"), None);
        assert_eq!(super::parse_admin_listen("127.0.0.1:0"), None);
    }

    #[test]
    fn admin_errors_name_the_token_problem() {
        assert!(super::admin_error(401, "", true).contains("refused the admin token"));
        assert!(super::admin_error(403, "", true).contains("read scope"));
        assert!(super::admin_error(403, "", false).contains("isn't allowed"));
        let refusal = r#"{"error":"refused","message":"the race runs until it is over"}"#;
        assert_eq!(
            super::admin_error(409, refusal, true),
            "the race runs until it is over"
        );
        assert_eq!(
            super::admin_error(500, "", true),
            "The admin API answered HTTP 500."
        );
    }

    #[test]
    fn a_query_value_is_percent_encoded() {
        assert_eq!(super::query_value("755 Compound"), "755%20Compound");
        assert_eq!(super::query_value("A&B=c+d/é"), "A%26B%3Dc%2Bd%2F%C3%A9");
        assert_eq!(super::query_value("Plain-Track_1.x~"), "Plain-Track_1.x~");
    }

    #[test]
    fn a_track_name_is_the_last_path_part() {
        assert_eq!(super::track_name("tracks/smokey.pkz"), "smokey.pkz");
        assert_eq!(super::track_name(r"C:\tracks\a.pkz"), "a.pkz");
        assert_eq!(super::track_name("a.pkz"), "a.pkz");
    }

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
    fn tracks_outside_the_package_folder_are_named_by_absolute_path() {
        // prod: package = "../tracks/755-Compound.pkz", files only in content/ and content/tracks/.
        let home = "/home/mxb/mxbserver";
        let paths: Vec<String> = [
            "content/tracks/755-Compound.pkz",
            "content/tracks/WDR.MX.26.R01.pkz",
            "content/zd-blackwood-server.pkz",
        ]
        .iter()
        .map(|p| format!("{home}/{p}"))
        .collect();
        let package_dir = format!("{home}/tracks");
        assert_eq!(
            super::track_reference("755-Compound.pkz", &paths, &package_dir, "../tracks/"),
            format!("{home}/content/tracks/755-Compound.pkz")
        );
        assert_eq!(
            super::track_reference("zd-blackwood-server.pkz", &paths, &package_dir, "../tracks/"),
            format!("{home}/content/zd-blackwood-server.pkz")
        );
        // In the package's own folder, or unknown: written beside the package as before.
        let beside = vec![format!("{package_dir}/a.pkz")];
        assert_eq!(super::track_reference("a.pkz", &beside, &package_dir, "../tracks/"), "../tracks/a.pkz");
        assert_eq!(super::track_reference("new.pkz", &[], &package_dir, "../tracks/"), "../tracks/new.pkz");
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
use tauri::{Emitter, Manager, RunEvent, State, WindowEvent};

struct App {
    store: Store,
    tunnels: Arc<ssh::Tunnels>,
    /// Track uploads, owned here so they outlive every screen.
    uploads: Arc<uploads::Uploads>,
    http: reqwest::Client,
    /// Admin API ports found in each SSH server's config, by server id.
    admin_ports: std::sync::Mutex<std::collections::HashMap<String, u16>>,
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
    app.admin_ports
        .lock()
        .ok()
        .map(|mut p| p.remove(&server.id));
    Ok(view(server))
}

#[tauri::command]
fn servers_remove(app: State<'_, App>, id: String) -> Result<(), String> {
    app.tunnels.close_server(&id);
    app.admin_ports.lock().ok().map(|mut p| p.remove(&id));
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

async fn fetch(
    app: &App,
    server: &Server,
    remote: u16,
    path: &str,
    token: Option<&str>,
) -> Result<(u16, String), Miss> {
    send(app, server, remote, reqwest::Method::GET, path, token, None).await
}

/// `send_once`, and for a server over SSH one more try on a fresh tunnel: a request that fails
/// on an old tunnel may just mean the SSH connection dropped, and only a new one can tell
/// "SSH is down" from "the server isn't running". A request that changes something is retried
/// only when it never connected, so it can't be done twice.
async fn send(
    app: &App,
    server: &Server,
    remote: u16,
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> Result<(u16, String), Miss> {
    match send_once(app, server, remote, &method, path, token, body).await {
        Err((Miss::NoAnswer(_), false)) if !server.local => {
            send_once(app, server, remote, &method, path, token, body)
                .await
                .map_err(|(miss, _)| miss)
        }
        other => other.map_err(|(miss, _)| miss),
    }
}

/// One request. The error says whether the request may have reached the server (so a request
/// that changes something must not be repeated).
async fn send_once(
    app: &App,
    server: &Server,
    remote: u16,
    method: &reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> Result<(u16, String), (Miss, bool)> {
    // A server on this PC is reached directly; any other through its SSH tunnel.
    let port = if server.local {
        remote
    } else {
        local_port(app, server, remote)
            .await
            .map_err(|e| (Miss::Ssh(e), false))?
    };
    let mut request = app
        .http
        .request(method.clone(), format!("http://127.0.0.1:{port}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response
                .text()
                .await
                .map_err(|e| (Miss::NoAnswer(e.to_string()), true))?;
            Ok((status, body))
        }
        Err(error) => {
            app.tunnels.reset(&server.id, remote);
            // Past the connect step a change may already have been made: don't repeat it.
            let sent = *method != reqwest::Method::GET && !error.is_connect();
            Err((Miss::NoAnswer(error.to_string()), sent))
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

/// The admin API's loopback port for `server`: the one saved with it, else the port of the
/// `[admin] listen` in the server's config (read over SSH once, then remembered), else mxbserver's
/// default, 9810.
async fn admin_port(app: &App, server: &Server) -> Result<u16, String> {
    if let Some(port) = server.admin_port {
        return Ok(port);
    }
    if server.local {
        return Err(
            "Set the admin port first (the [admin] listen port in the server's config).".into(),
        );
    }
    if let Some(port) = app
        .admin_ports
        .lock()
        .ok()
        .and_then(|ports| ports.get(&server.id).copied())
    {
        return Ok(port);
    }
    let tunnels = Arc::clone(&app.tunnels);
    let remote = server.clone();
    let observe = server.observe_port.to_string();
    let out =
        blocking(move || tunnels.run_script(&remote, REMOTE_SH, &["admin-addr", &observe], 30))
            .await?;
    if !out.success {
        return Err(out.text());
    }
    let port = parse_admin_listen(out.field("admin").unwrap_or(""))
        .ok_or("The server's [admin] listen address is not host:port.")?;
    if let Ok(mut ports) = app.admin_ports.lock() {
        ports.insert(server.id.clone(), port);
    }
    Ok(port)
}

/// The port of an `[admin] listen` value such as `127.0.0.1:9810` or `[::1]:9810`.
fn parse_admin_listen(listen: &str) -> Option<u16> {
    let port = listen.trim().rsplit_once(':')?.1.parse::<u16>().ok()?;
    (port != 0).then_some(port)
}

const NO_TOKEN: &str = "No admin token is saved for this server. On the server run `mxbserver admin token new --id app --scope control`, add the printed entry to its tokens file, and save the token in this server's settings.";

/// One sentence for a non-2xx answer of the admin API. `control` says the call needs the
/// `control` scope, which is what a 403 then means.
fn admin_error(code: u16, body: &str, control: bool) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string))
        .filter(|m| !m.is_empty());
    match code {
        401 => "The server refused the admin token: it's mistyped, or was revoked. Save a fresh one in this server's settings.".into(),
        403 if control => "This admin token only has read scope, so it can't control the server. Create a control token (`mxbserver admin token new --id app --scope control`), add it to the server's tokens file, and save it in this server's settings.".into(),
        403 => "The admin token is valid but isn't allowed to do that.".into(),
        429 => "The server is rate-limiting the admin API; try again in a few seconds.".into(),
        _ => detail.unwrap_or_else(|| format!("The admin API answered HTTP {code}.")),
    }
}

/// Call mxbserver's own admin API with the token saved for this server, through the SSH tunnel
/// (or straight to loopback for a server on this PC). `control` marks a call that needs the
/// `control` scope. Returns the JSON answer.
async fn admin_call(
    app: &App,
    server: &Server,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
    control: bool,
) -> Result<Value, String> {
    let token = store::token(&server.id).ok_or(NO_TOKEN)?;
    let port = admin_port(app, server).await?;
    let (code, text) = send(app, server, port, method, path, Some(&token), body)
        .await
        .map_err(|miss| match miss {
            Miss::NoAnswer(_) => format!(
                "Nothing answers on admin port {port}. Is mxbserver running, with an [admin] section in its config (default 127.0.0.1:9810)?"
            ),
            other => other.text(),
        })?;
    if !(200..300).contains(&code) {
        return Err(admin_error(code, &text, control));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

/// A GET of the admin API that treats 404 (a server older than the endpoint) as "not
/// supported": `{"supported": false}` instead of an error.
async fn admin_get_optional(app: &App, server: &Server, path: &str) -> Result<Value, String> {
    let token = store::token(&server.id).ok_or(NO_TOKEN)?;
    let port = admin_port(app, server).await?;
    let (code, text) = send(app, server, port, reqwest::Method::GET, path, Some(&token), None)
        .await
        .map_err(|miss| match miss {
            Miss::NoAnswer(_) => format!(
                "Nothing answers on admin port {port}. Is mxbserver running, with an [admin] section in its config (default 127.0.0.1:9810)?"
            ),
            other => other.text(),
        })?;
    if code == 404 {
        return Ok(serde_json::json!({ "supported": false }));
    }
    if !(200..300).contains(&code) {
        return Err(admin_error(code, &text, false));
    }
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

/// The admin `/v1/cuts`: cut detection settings, the penalties, and per track the outline and zones.
#[tauri::command]
async fn server_cuts(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Ok(serde_json::json!({ "supported": false }));
    }
    admin_get_optional(&app, &server, "/v1/cuts").await
}

/// Percent-encode a query value (everything but the RFC 3986 unreserved characters).
fn query_value(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The admin `/v1/cuts/outline?track=`: one track's outline from its TRH, whether or not cut
/// detection is on. `{"supported": false}` for a server older than the route; `{"status":
/// "unknown_track"}` when the server has no package for that track.
#[tauri::command]
async fn server_cut_outline(app: State<'_, App>, id: String, track: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Ok(serde_json::json!({ "supported": false }));
    }
    let token = store::token(&server.id).ok_or(NO_TOKEN)?;
    let port = admin_port(&app, &server).await?;
    let path = format!("/v1/cuts/outline?track={}", query_value(&track));
    let (code, text) = send(&app, &server, port, reqwest::Method::GET, &path, Some(&token), None)
        .await
        .map_err(|miss| match miss {
            Miss::NoAnswer(_) => format!(
                "Nothing answers on admin port {port}. Is mxbserver running, with an [admin] section in its config (default 127.0.0.1:9810)?"
            ),
            other => other.text(),
        })?;
    if code == 404 {
        let known = serde_json::from_str::<Value>(&text).ok().is_some_and(|v| v["error"] == "unknown_track");
        return Ok(if known {
            serde_json::json!({ "status": "unknown_track", "track": track })
        } else {
            serde_json::json!({ "supported": false })
        });
    }
    if !(200..300).contains(&code) {
        return Err(admin_error(code, &text, false));
    }
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

/// The admin `/v1/events` (its `cuts.recent` feeds the cut map).
#[tauri::command]
async fn server_cut_events(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Ok(serde_json::json!({ "supported": false }));
    }
    admin_get_optional(&app, &server, "/v1/events").await
}
/// Try the saved admin token against `/v1/server` and say plainly what happened. A dry-run
/// config reload (changes nothing) tells whether it also has the `control` scope.
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
    if let Err(error) = admin_call(
        &app,
        &server,
        reqwest::Method::GET,
        "/v1/server",
        None,
        false,
    )
    .await
    {
        return fail(error);
    }
    let dry_run = serde_json::json!({ "dry_run": true });
    match admin_call(
        &app,
        &server,
        reqwest::Method::POST,
        "/v1/config/reload",
        Some(&dry_run),
        true,
    )
    .await
    {
        Err(error) if error.contains("read scope") => Ok(TokenCheck {
            ok: true,
            message: "Token works, but it is read-only: race and session controls need a control-scope token.".into(),
        }),
        _ => Ok(TokenCheck {
            ok: true,
            message: "Token works, with control scope.".into(),
        }),
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
    admin_call(
        &app,
        &server,
        reqwest::Method::GET,
        "/v1/riders",
        None,
        false,
    )
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackState {
    installed: Vec<String>,
    library: Vec<String>,
    current: Option<String>,
    rotation: Vec<String>,
}

/// The file name of a track path from the config (`tracks/smokey.pkz` -> `smokey.pkz`).
fn track_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// The config text out of a `read` run of remote.sh, with its hash.
fn read_config(out: &ssh::ScriptOutput) -> Result<(String, String), String> {
    use base64::Engine;
    let encoded = out
        .field("config_b64")
        .filter(|_| out.success)
        .ok_or_else(|| format!("could not read the config: {}", out.text()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "the config is not UTF-8".to_string())?;
    let sha = out.field("sha").unwrap_or("").to_string();
    Ok((text, sha))
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
    let port = server.observe_port.to_string();
    let (config, listing) = blocking(move || {
        let config = tunnels.run_script(&server, REMOTE_SH, &["read", &port], 30)?;
        let listing = tunnels.run_script(&server, REMOTE_SH, &["tracks", &port], 30)?;
        Ok((config, listing))
    })
    .await?;
    if !listing.success {
        return Err(listing.text());
    }
    let (text, _) = read_config(&config)?;
    let (package, rotation) = config::tracks(&text)?;
    let installed = listed_names(&listing);
    Ok(TrackState {
        library: installed.clone(),
        installed,
        current: package.as_deref().map(track_name),
        rotation: rotation.iter().map(|t| track_name(t)).collect(),
    })
}


fn decode_field(out: &ssh::ScriptOutput, field: &str) -> String {
    use base64::Engine;
    out.field(field)
        .map(|b| {
            base64::engine::general_purpose::STANDARD
                .decode(b)
                .unwrap_or_default()
        })
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// The distinct track file names of a `tracks` run of remote.sh.
fn listed_names(listing: &ssh::ScriptOutput) -> Vec<String> {
    let mut names: Vec<String> = decode_field(listing, "tracks_b64")
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// How the config should name `name` in `[track] package` / `[rotation] tracks`: written
/// beside the current package the way that path is written, or as the absolute path when the
/// file lives in another folder (content/, content/tracks/).
fn track_reference(name: &str, paths: &[String], package_dir: &str, prefix: &str) -> String {
    let found = paths
        .iter()
        .find(|p| p.rsplit('/').next() == Some(name))
        .map(String::as_str);
    match found {
        Some(path) if path.rsplit_once('/').map(|(d, _)| d) != Some(package_dir) => path.to_string(),
        _ => format!("{prefix}{name}"),
    }
}

/// Rewrite the server's `[track] package` (and the `[rotation] tracks` when given) in its
/// config over SSH, and apply it: the host validates it, restarts the service with systemctl,
/// and puts the old file back if the server doesn't come up.
async fn apply_tracks(
    app: &App,
    server: Server,
    current: &str,
    rotation: Option<Vec<String>>,
) -> Result<Value, String> {
    let bad =
        |name: &str| name.is_empty() || name == ".." || name.contains(['/', '\\', '"', '\n', '\r']);
    if bad(current) || rotation.iter().flatten().any(|name| bad(name)) {
        return Err("Track names must be plain file names.".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let current = current.to_string();
    let out = blocking(move || {
        let config = tunnels.run_script(&server, REMOTE_SH, &["read", &port], 30)?;
        let (text, sha) = read_config(&config)?;
        let (package, old_rotation) = config::tracks(&text)?;
        let listing = tunnels.run_script(&server, REMOTE_SH, &["tracks", &port], 30)?;
        let package_dir = listing.field("dir").unwrap_or_default().to_string();
        let paths: Vec<String> = decode_field(&listing, "paths_b64")
            .lines()
            .map(str::to_string)
            .collect();
        // New tracks sit beside the current package, written the way the config writes it.
        let prefix = package
            .as_deref()
            .and_then(|p| p.rsplit_once('/'))
            .map(|(dir, _)| format!("{dir}/"))
            .unwrap_or_default();
        let rotation = match rotation {
            Some(names) => names
                .iter()
                .map(|n| track_reference(n, &paths, &package_dir, &prefix))
                .collect(),
            None => old_rotation,
        };
        let edited = config::set_tracks(&text, &track_reference(&current, &paths, &package_dir, &prefix), &rotation)?;
        let encoded = b64(&edited);
        tunnels.run_script(&server, REMOTE_SH, &["apply", &port, &encoded, &sha], 300)
    })
    .await?;
    match out.field("result") {
        Some("applied") => Ok(serde_json::json!({ "result": "applied" })),
        Some("rolled-back") => Err(format!(
            "The server did not come back with that track, so the old config is live again. {}",
            out.text()
        )),
        _ => Err(out.text()),
    }
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
    apply_tracks(&app, server, track.trim(), None).await
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
    apply_tracks(&app, server, &current, Some(tracks)).await
}

/// Replace the server binary on the host (sha-checked upload, then `systemctl restart`; the old
/// binary is put back if the new one doesn't come up).
async fn install_version(
    app: &App,
    server: Server,
    temporary: String,
    name: String,
    digest: String,
    version: String,
) -> Result<Value, String> {
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(
            &server,
            REMOTE_SH,
            &[
                "install-version",
                &port,
                &temporary,
                &name,
                &digest,
                &version,
            ],
            360,
        )
    })
    .await?;
    match out.field("result") {
        Some("installed") => Ok(serde_json::json!({
            "result": "installed",
            "version": out.field("version").unwrap_or("")
        })),
        Some("rolled-back") => Err(format!(
            "The new server build did not start, so the old one is running again. {}",
            out.text()
        )),
        _ => Err(out.text()),
    }
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
    let upload_server = server.clone();
    let (temporary, digest, version) = blocking(move || {
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
        let uploaded = tunnels.upload(&upload_server, &binary.to_string_lossy(), &temporary, 300);
        let _ = std::fs::remove_dir_all(&dir);
        uploaded?;
        Ok((temporary, digest, version))
    }).await?;
    install_version(&app, server, temporary, "mxbserver".into(), digest, version).await
}

/// Live session control through mxbserver's admin API: jump to practice, qualifying, warm-up or
/// the race, end the current stage (`advance`), restart it, or `rotate` to the next track.
/// Needs a control-scope token.
#[tauri::command]
async fn server_session(
    app: State<'_, App>,
    id: String,
    action: String,
    to: Option<String>,
) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err("The official dedicated server does not expose live session controls.".into());
    }
    let (path, body) = match action.as_str() {
        "jump" => {
            let destination = to.unwrap_or_default();
            if !matches!(
                destination.as_str(),
                "practice" | "qualifying" | "warmup" | "race"
            ) {
                return Err("unknown session".into());
            }
            ("/v1/session/jump", serde_json::json!({ "to": destination }))
        }
        "advance" => ("/v1/session/advance", serde_json::json!({})),
        "restart" => ("/v1/session/restart", serde_json::json!({})),
        "rotate" => ("/v1/session/rotate", serde_json::json!({})),
        _ => return Err("unknown session action".into()),
    };
    admin_call(
        &app,
        &server,
        reqwest::Method::POST,
        path,
        Some(&body),
        true,
    )
    .await
}

/// Restart the mxbserver service on its host with systemctl (SIGINT + relaunch for a hand-started
/// server) and wait until it answers again.
#[tauri::command]
async fn server_restart_service(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err("Use the Restart button of the official server's controls.".into());
    }
    if server.local {
        return Err("Restart a server on this PC from its own window.".into());
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(&server, REMOTE_SH, &["service", &port, "restart"], 120)
    })
    .await?;
    if out.field("result") == Some("restarted") {
        Ok(serde_json::json!({ "result": "restarted" }))
    } else {
        Err(out.text())
    }
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
    if kind == "version" {
        return install_version(&app, server, temporary, name, digest, version).await;
    }
    let tunnels = Arc::clone(&app.tunnels);
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(
            &server,
            REMOTE_SH,
            &["install-track", &port, &temporary, &name, &digest],
            120,
        )
    })
    .await?;
    if out.field("installed").is_some() {
        Ok(serde_json::json!({ "installed": out.field("installed") }))
    } else {
        Err(out.text())
    }
}

const TRACK_UPLOAD_MAX: u64 = 512 * 1024 * 1024;
const UPLOAD_ATTEMPTS: u32 = 3;

fn publish_upload(
    handle: &tauri::AppHandle,
    uploads: &uploads::Uploads,
    id: &str,
    change: impl FnOnce(&mut uploads::UploadInfo),
) {
    if let Some(info) = uploads.update(id, change) {
        let _ = handle.emit("upload-update", &info);
    }
}

/// Start uploading a .pkz to a server. Returns at once; the transfer runs in the backend and
/// reports through `upload-update` events and `upload_list`, whatever screen is showing.
#[tauri::command]
async fn upload_start(
    handle: tauri::AppHandle,
    app: State<'_, App>,
    id: String,
    path: String,
) -> Result<uploads::UploadInfo, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Err("Legacy connecting uses tracks and game versions already installed on the official server host.".into());
    }
    if server.local {
        return Err("Uploads for a server on this PC are not wired yet.".into());
    }
    if let Some(existing) = app.uploads.running(&id, &path) {
        return Ok(existing);
    }
    let original = std::path::Path::new(&path)
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("the file has no usable name")?
        .to_string();
    let name = safe_remote_filename(&original);
    if !name.to_ascii_lowercase().ends_with(".pkz") {
        return Err("Tracks must be .pkz packages.".into());
    }
    let bytes = std::fs::metadata(&path)
        .map_err(|e| format!("could not read {path}: {e}"))?
        .len();
    if bytes == 0 || bytes > TRACK_UPLOAD_MAX {
        return Err(format!(
            "the file must be between 1 byte and {} MiB",
            TRACK_UPLOAD_MAX / 1024 / 1024
        ));
    }
    let info = uploads::UploadInfo {
        id: store::new_id(),
        server_id: server.id.clone(),
        server_name: server.name.clone(),
        path: path.clone(),
        file_name: name.clone(),
        bytes,
        sent: 0,
        speed: 0.0,
        attempt: 1,
        status: uploads::UploadStatus::Checking,
        error: None,
    };
    let cancel = app.uploads.add(info.clone());
    let _ = handle.emit("upload-update", &info);
    let tunnels = Arc::clone(&app.tunnels);
    let registry = Arc::clone(&app.uploads);
    let upload_id = info.id.clone();
    tauri::async_runtime::spawn(async move {
        run_track_upload(handle, tunnels, registry, server, upload_id, path, name, cancel).await;
    });
    Ok(info)
}

#[allow(clippy::too_many_arguments)]
async fn run_track_upload(
    handle: tauri::AppHandle,
    tunnels: Arc<ssh::Tunnels>,
    registry: Arc<uploads::Uploads>,
    server: Server,
    id: String,
    path: String,
    name: String,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let fail = |message: String| {
        publish_upload(&handle, &registry, &id, |i| {
            i.status = uploads::UploadStatus::Error;
            i.speed = 0.0;
            i.error = Some(message);
        });
    };
    let stopped = || {
        publish_upload(&handle, &registry, &id, |i| {
            i.status = uploads::UploadStatus::Cancelled;
            i.speed = 0.0;
        });
    };
    // The hash the host checks the upload against.
    let digest = blocking({
        let (path, cancel) = (path.clone(), Arc::clone(&cancel));
        move || {
            use sha2::Digest;
            use std::io::Read;
            let mut file =
                std::fs::File::open(&path).map_err(|e| format!("could not read {path}: {e}"))?;
            let mut hash = sha2::Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                if cancel.load(Ordering::SeqCst) {
                    return Ok(String::new());
                }
                let read = file
                    .read(&mut buffer)
                    .map_err(|e| format!("could not read {path}: {e}"))?;
                if read == 0 {
                    return Ok(format!("{:x}", hash.finalize()));
                }
                hash.update(&buffer[..read]);
            }
        }
    })
    .await;
    let digest = match digest {
        Ok(_) if cancel.load(Ordering::SeqCst) => return stopped(),
        Ok(d) => d,
        Err(e) => return fail(e),
    };
    let temporary = format!("mxb-servers-{}", store::new_id());
    for attempt in 1..=UPLOAD_ATTEMPTS {
        publish_upload(&handle, &registry, &id, |i| {
            i.status = uploads::UploadStatus::Uploading;
            i.attempt = attempt;
            i.sent = 0;
            i.error = None;
        });
        let result = blocking({
            let (tunnels, server, path, temporary) = (
                Arc::clone(&tunnels),
                server.clone(),
                path.clone(),
                temporary.clone(),
            );
            let (handle, registry, id, cancel) = (
                handle.clone(),
                Arc::clone(&registry),
                id.clone(),
                Arc::clone(&cancel),
            );
            move || {
                let mut meter = uploads::SpeedMeter::new();
                let mut last_emit = std::time::Instant::now();
                Ok(
                    tunnels.upload_with_progress(&server, &path, &temporary, &cancel, |sent| {
                        let now = std::time::Instant::now();
                        let speed = meter.record(sent, now);
                        if now.duration_since(last_emit) >= Duration::from_millis(200) {
                            last_emit = now;
                            publish_upload(&handle, &registry, &id, |i| {
                                i.sent = sent;
                                i.speed = speed;
                            });
                        }
                    }),
                )
            }
        })
        .await;
        match result {
            Ok(Ok(())) => break,
            Ok(Err(ssh::UploadError::Cancelled)) => return stopped(),
            Ok(Err(ssh::UploadError::Failed(message))) => {
                if attempt < UPLOAD_ATTEMPTS && uploads::transient(&message) {
                    publish_upload(&handle, &registry, &id, |i| {
                        i.status = uploads::UploadStatus::Retrying;
                        i.speed = 0.0;
                        i.error = Some(message);
                    });
                    for _ in 0..30 {
                        if cancel.load(Ordering::SeqCst) {
                            return stopped();
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                    continue;
                }
                return fail(message);
            }
            Err(e) => return fail(e),
        }
    }
    publish_upload(&handle, &registry, &id, |i| {
        i.status = uploads::UploadStatus::Installing;
        i.sent = i.bytes;
        i.speed = 0.0;
        i.error = None;
    });
    let port = server.observe_port.to_string();
    let out = blocking(move || {
        tunnels.run_script(
            &server,
            REMOTE_SH,
            &["install-track", &port, &temporary, &name, &digest],
            120,
        )
    })
    .await;
    match out {
        Ok(out) if out.field("installed").is_some() => {
            publish_upload(&handle, &registry, &id, |i| {
                i.status = uploads::UploadStatus::Done;
                i.speed = 0.0;
            });
        }
        Ok(out) => fail(out.text()),
        Err(e) => fail(e),
    }
}

async fn sleep(duration: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration)).await;
}

#[tauri::command]
fn upload_list(app: State<'_, App>) -> Vec<uploads::UploadInfo> {
    app.uploads.list()
}

/// Stop a running upload. Nothing else does: leaving a screen never cancels one.
#[tauri::command]
fn upload_cancel(app: State<'_, App>, id: String) -> bool {
    app.uploads.cancel(&id)
}

/// Forget a finished (done, failed or cancelled) upload.
#[tauri::command]
fn upload_dismiss(app: State<'_, App>, id: String) {
    app.uploads.dismiss(&id);
}

/// Quit now, after the screen confirmed losing any running uploads.
#[tauri::command]
fn quit_app(handle: tauri::AppHandle) {
    handle.exit(0);
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
    let port = server.observe_port.to_string();
    let count = lines.clamp(1, 2000).to_string();
    // `journalctl -u mxbserver`, run on the host by the helper script.
    let out =
        blocking(move || tunnels.run_script(&server, REMOTE_SH, &["logs", &port, &count], 30))
            .await?;
    if !out.success {
        return Err(out.text());
    }
    Ok(out.stdout.lines().map(str::to_string).collect())
}

/// The observe `/timing` feed: race numbers, names, laps and split times (no credentials, no
/// GUIDs). The Events tab diffs it into lap finishes and time checks.
#[tauri::command]
async fn server_timing(app: State<'_, App>, id: String) -> Result<Value, String> {
    let server = app.store.get(&id)?;
    if server.kind == ServerKind::Legacy {
        return Ok(serde_json::json!({ "available": false }));
    }
    let (code, body) = fetch(&app, &server, server.observe_port, "/timing", None)
        .await
        .map_err(Miss::text)?;
    json_answer(code, &body, "/timing")
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
                uploads: Arc::default(),
                http,
                admin_ports: Default::default(),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Quitting mid-upload loses it: ask the screen to confirm first.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Some(app) = window.try_state::<App>() {
                    if app.uploads.active() > 0 {
                        api.prevent_close();
                        let _ = window.emit("quit-requested", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            servers_list,
            servers_save,
            servers_remove,
            legacy_pairing,
            server_status,
            server_riders,
            server_cuts,
            server_cut_outline,
            server_cut_events,
            server_tracks,
            server_set_track,
            server_set_rotation,
            server_update_github,
            server_session,
            server_restart_service,
            server_upload,
            upload_start,
            upload_list,
            upload_cancel,
            upload_dismiss,
            quit_app,
            inspect_track_upload,
            server_logs,
            server_timing,
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
