//! Dev only (`--features bridge`): the app with no window, answering the frontend's commands
//! over HTTP on 127.0.0.1, so the real UI can run in a headless browser against the real
//! backend for screenshots. A page loads `test/bridge.js`, which stands in for Tauri's IPC.
//!
//! `POST /invoke/<command>` with the arguments as JSON; JSON back, or the raw bytes for a
//! command that answers with them.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::Value;
use tauri::ipc::IpcResponse;
use tauri::{AppHandle, Manager};

use crate::blender::Shared;
use crate::{commands, export, sessions, settings};

enum Out {
    Json(Value),
    Bytes(Vec<u8>),
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn opt(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn json<T: serde::Serialize>(t: T) -> Result<Out, String> {
    serde_json::to_value(t).map(Out::Json).map_err(|e| e.to_string())
}

fn bytes(r: Result<tauri::ipc::Response, String>) -> Result<Out, String> {
    match r?.body().map_err(|e| e.to_string())? {
        tauri::ipc::InvokeResponseBody::Raw(b) => Ok(Out::Bytes(b)),
        tauri::ipc::InvokeResponseBody::Json(j) => Ok(Out::Json(serde_json::from_str(&j).unwrap_or(Value::Null))),
    }
}

fn call(app: &AppHandle, cmd: &str, a: &Value) -> Result<Out, String> {
    use mxb_core::trackview as tv;
    let job = app.state::<Shared>().inner().clone();
    match cmd {
        "app_platform" => json(std::env::consts::OS),
        "get_config" => json(mxb_core::config::load(app).unwrap_or_default()),
        "list_games" => json(mxb_core::game::all_info()),
        "log_client" => json(Value::Null),
        "texture_bytes" => bytes(Ok(tauri::async_runtime::block_on(mxb_core::viewer::texture_bytes(s(a, "token"))))),
        "gate_verdict" | "recheck_gate" => json(Value::Null),
        "preview_model_swap" => json(commands::bike_model(app, &s(a, "bike"), opt(a, "tyres"))?),
        "coach_rider_body" => json(commands::rider_body(app, &s(a, "profile"))?),
        "read_track_info" => json(tauri::async_runtime::block_on(tv::read_track_info(app.clone(), s(a, "path"), opt(a, "prefix")))?),
        "load_track_terrain" => bytes(tauri::async_runtime::block_on(tv::load_track_terrain(
            app.clone(),
            s(a, "path"),
            a.get("maxDim").and_then(Value::as_u64).unwrap_or(1024) as u32,
            opt(a, "prefix"),
        ))),
        "load_track_overview" => bytes(tauri::async_runtime::block_on(tv::load_track_overview(
            app.clone(),
            s(a, "path"),
            a.get("maxDim").and_then(Value::as_u64).unwrap_or(1024) as u32,
            opt(a, "prefix"),
        ))),
        "load_track_scenery" => bytes(tauri::async_runtime::block_on(tv::load_track_scenery(app.clone(), s(a, "path"), opt(a, "prefix")))),
        "load_track_surfaces" => bytes(tauri::async_runtime::block_on(tv::load_track_surfaces(app.clone(), s(a, "path"), opt(a, "prefix")))),
        "load_track_backdrop" => bytes(tauri::async_runtime::block_on(tv::load_track_backdrop(s(a, "path"), opt(a, "prefix")))),
        "load_track_ground" => bytes(tauri::async_runtime::block_on(tv::load_track_ground(app.clone(), s(a, "path"), opt(a, "prefix")))),
        "load_track_ground_layers" => bytes(tauri::async_runtime::block_on(tv::load_track_ground_layers(app.clone(), s(a, "path"), opt(a, "prefix")))),
        "read_track_placements" => json(tauri::async_runtime::block_on(tv::read_track_placements(s(a, "path"), opt(a, "prefix")))?),
        "cinema_sessions" => json(sessions::list(app)),
        "cinema_session" => json(sessions::data(&s(a, "path"))?),
        "cinema_ground" => json(sessions::ground(app, &s(a, "path"))?),
        "cinema_settings" => json(settings::view(app)),
        "cinema_set_settings" => {
            let set: settings::Settings = serde_json::from_value(a.get("settings").cloned().unwrap_or_default()).map_err(|e| e.to_string())?;
            settings::save(app, &set)?;
            json(settings::view(app))
        }
        "cinema_check_blender" => json(crate::blender::install(std::path::Path::new(&s(a, "path")))),
        "cinema_export" => {
            let req: export::ExportRequest = serde_json::from_value(a.get("request").cloned().unwrap_or_default()).map_err(|e| e.to_string())?;
            export::start(app, job, req)?;
            json(Value::Null)
        }
        "cinema_export_status" => json(job.snapshot()),
        "cinema_export_cancel" => {
            job.cancel();
            json(Value::Null)
        }
        "cinema_read_image" => json(commands::read_image(&s(a, "path"))?),
        // Never from a headless page: nothing may open on the desktop.
        "cinema_open_blend" | "cinema_reveal" | "reveal_in_explorer" | "open_folder" => {
            log::info!("bridge: {cmd} {a}");
            json(Value::Null)
        }
        _ => Err(format!("bridge: no command {cmd}")),
    }
}

fn reply(mut st: TcpStream, code: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = st.write_all(head.as_bytes());
    let _ = st.write_all(body);
}

fn serve(app: AppHandle, st: TcpStream) {
    let mut r = BufReader::new(match st.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").to_string());
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if method == "OPTIONS" {
        return reply(st, "204 No Content", "text/plain", b"");
    }
    let mut body = vec![0u8; len];
    if r.read_exact(&mut body).is_err() {
        return;
    }
    let Some(cmd) = path.strip_prefix("/invoke/") else {
        return reply(st, "200 OK", "text/plain", b"cinema bridge");
    };
    let args: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match call(&app, cmd, &args) {
        Ok(Out::Json(v)) => reply(st, "200 OK", "application/json", &serde_json::to_vec(&v).unwrap_or_default()),
        Ok(Out::Bytes(b)) => reply(st, "200 OK", "application/octet-stream", &b),
        Err(e) => reply(st, "500 Error", "application/json", &serde_json::to_vec(&Value::String(e)).unwrap_or_default()),
    }
}

pub fn start(app: AppHandle, port: u16) {
    std::thread::spawn(move || {
        let Ok(l) = TcpListener::bind(("127.0.0.1", port)) else {
            log::error!("bridge: port {port} is taken");
            return;
        };
        log::info!("bridge: listening on 127.0.0.1:{port}");
        for st in l.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || serve(app, st));
        }
    });
}
