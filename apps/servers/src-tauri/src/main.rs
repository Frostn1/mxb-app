// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MXB Servers: one window for every mxbserver — health, build, session, riders and logs.
//!
//! P1 of the server-manager plan (`handoffs/server-manager.md`). Each server is reached over
//! SSH: its observe port gives `/readyz` and `/status` with no credentials, its admin port
//! (when configured) gives `/v1/riders` with a bearer token from the OS keychain, and the log
//! is read with `tail`. Nothing listens on the internet for this.

mod ssh;
mod store;

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
async fn get(
    app: &App,
    server: &Server,
    remote: u16,
    path: &str,
    token: Option<&str>,
) -> Result<(u16, String), String> {
    let port = local_port(app, server, remote).await?;
    let mut request = app.http.get(format!("http://127.0.0.1:{port}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.map_err(|e| e.to_string())?;
            Ok((status, body))
        }
        Err(error) => {
            app.tunnels.reset(&server.id, remote);
            Err(format!("no answer from the server: {error}"))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusReport {
    ready: bool,
    status: Option<Value>,
}

#[tauri::command]
async fn server_status(app: State<'_, App>, id: String) -> Result<StatusReport, String> {
    let server = app.store.get(&id)?;
    let (code, body) = get(&app, &server, server.observe_port, "/status", None).await?;
    if code != 200 {
        return Err(format!("/status answered {code}"));
    }
    let status: Value = serde_json::from_str(&body).map_err(|e| format!("/status: {e}"))?;
    let ready = matches!(
        get(&app, &server, server.observe_port, "/readyz", None).await,
        Ok((200, _))
    );
    Ok(StatusReport {
        ready,
        status: Some(status),
    })
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
    let tunnels = Arc::clone(&app.tunnels);
    tauri::async_runtime::spawn_blocking(move || tunnels.tail(&server, lines))
        .await
        .map_err(|e| e.to_string())?
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
