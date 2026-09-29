//! mxb-agent — supervises an MX Bikes dedicated server and exposes it over HTTP.
//!
//! Runs on the game host next to `mxbikes.exe`. The desktop app talks to this rather than
//! to the cloud provider, which is the point: managing a server from the app must never
//! require provider credentials to be shipped inside the app.

mod config;
mod ini;
mod native;
mod native_admin;
mod pairing;
mod roster;
mod supervisor;
mod tracks;

use config::{Config, ServerKind};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use supervisor::Supervisor;
use tiny_http::{Header, Request, Response, Server};

/// How often the crash watcher checks on the game.
const WATCH_INTERVAL: Duration = Duration::from_secs(5);
const MAX_TRACK_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 128 * 1024 * 1024;

type Shared = Arc<Mutex<Supervisor>>;

fn main() {
    let cfg_path = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("agent.json"));

    let cfg = match Config::load(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mxb-agent: {e}");
            std::process::exit(1);
        }
    };
    let listen = cfg.listen.clone();

    // Printed before anything else can fail, and on every start rather than once at
    // install: an operator who lost the line just restarts the agent to get it back.
    let pairing = pairing::Pairing {
        url: pairing::public_url(&cfg.listen, cfg.public_url.as_deref()),
        token: cfg.token.clone(),
    };
    eprintln!("mxb-agent: pair this server by pasting the line below into the app");
    println!("{}", pairing.encode());

    let shared: Shared = Arc::new(Mutex::new(Supervisor::new(cfg)));

    // Start the game up front, so a reboot brings the server back with the agent.
    if let Err(e) = shared.lock().unwrap().start() {
        eprintln!("mxb-agent: couldn't start the game: {e}");
    }

    {
        let watched = Arc::clone(&shared);
        std::thread::spawn(move || loop {
            std::thread::sleep(WATCH_INTERVAL);
            if watched.lock().unwrap().revive_if_crashed() {
                eprintln!("mxb-agent: the game exited on its own — restarted it");
            }
        });
    }

    let server = match Server::http(&listen) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mxb-agent: couldn't listen on {listen}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("mxb-agent: listening on {listen}");

    for request in server.incoming_requests() {
        handle(request, &shared);
    }
}

fn handle(mut request: Request, shared: &Shared) {
    let method = request.method().as_str().to_string();
    // Strip any query string: routes here take no parameters.
    let path = request.url().split('?').next().unwrap_or("/").to_string();

    // Liveness is deliberately unauthenticated — it reveals nothing, and it has to work
    // for whoever is debugging a box they can't get a token onto yet.
    if method == "GET" && path == "/health" {
        let _ = request.respond(json(200, &serde_json::json!({ "ok": true })));
        return;
    }

    if !authorized(&request, shared) {
        let _ = request.respond(json(401, &serde_json::json!({ "error": "unauthorized" })));
        return;
    }

    let response = match (method.as_str(), path.as_str()) {
        ("GET", "/capabilities") => capabilities(shared),
        ("GET", "/status") => status(shared),
        ("GET", "/players") => players(shared),
        ("GET", "/tracks") => installed_tracks(shared),
        ("POST", "/start") => act(shared, |s| s.start()),
        ("POST", "/stop") => act(shared, |s| s.stop()),
        ("POST", "/restart") => act(shared, |s| s.restart()),
        ("POST", "/session") => {
            let mut body = String::new();
            if request
                .as_reader()
                .take(16 * 1024)
                .read_to_string(&mut body)
                .is_err()
            {
                json(
                    400,
                    &serde_json::json!({ "error": "couldn't read the request body" }),
                )
            } else {
                session_action(shared, &body)
            }
        }
        ("PUT", "/tracks") => upload_track(shared, &mut request),
        ("PUT", "/version") => upload_version(shared, &mut request),
        ("GET", "/config") => read_config(shared),
        ("PUT", "/config") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                json(
                    400,
                    &serde_json::json!({ "error": "couldn't read the request body" }),
                )
            } else {
                update_config(shared, &body)
            }
        }
        _ => json(404, &serde_json::json!({ "error": "no such endpoint" })),
    };
    let _ = request.respond(response);
}

fn authorized(request: &Request, shared: &Shared) -> bool {
    let expected = shared.lock().unwrap().config().token.clone();
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .and_then(|h| config::bearer(h.value.as_str()))
        .map(|presented| config::token_matches(&expected, presented))
        .unwrap_or(false)
}

fn status(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut guard = shared.lock().unwrap();
    let game = guard.status();
    let cfg = guard.config().clone();
    let port = cfg.game_port;
    if cfg.kind == ServerKind::Native {
        let settings = guard
            .read_server_config()
            .and_then(|text| native::view(&text));
        let version = fs::read_to_string(cfg.version_path())
            .ok()
            .map(|v| v.trim().to_string());
        let session = match (
            cfg.native_admin.as_deref(),
            cfg.native_admin_token.as_deref(),
        ) {
            (Some(addr), Some(token)) if game.running => {
                native_admin::request(addr, token, "GET", "/v1/session", "").ok()
            }
            _ => None,
        };
        return match settings {
            Ok(server) => json(
                200,
                &serde_json::json!({
                    "kind": "native", "game": game, "port": port, "server": server,
                    "version": version, "session": session
                }),
            ),
            Err(error) => json(
                200,
                &serde_json::json!({
                    "kind": "native", "game": game, "port": port, "configError": error,
                    "version": version, "session": session
                }),
            ),
        };
    }
    // Config is best-effort: a server whose .ini we can't read is still worth reporting on.
    let text = guard.read_ini().unwrap_or_default();
    json(
        200,
        &serde_json::json!({
            "kind": "stock",
            "game": game,
            "port": port,
            "server": {
                "name": ini::get(&text, ini::NAME),
                "track": ini::get(&text, ini::TRACK),
                "maxClients": ini::get(&text, ini::MAX_CLIENT),
            }
        }),
    )
}

/// Who is connected right now, with the GUID that identifies them stably.
fn players(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let guard = shared.lock().unwrap();
    let log = guard.config().game_dir.join("log.txt");
    match std::fs::read_to_string(&log) {
        Ok(text) => json(
            200,
            &serde_json::json!({ "players": roster::connected(&text) }),
        ),
        // No log yet is a server that has not been connected to, not a failure.
        Err(_) => json(200, &serde_json::json!({ "players": [] })),
    }
}

/// The tracks this host can actually run, so the app offers a list instead of a text box
/// the operator has to spell a track name into from memory.
fn installed_tracks(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let guard = shared.lock().unwrap();
    let cfg = guard.config().clone();
    drop(guard);
    let found = match cfg.kind {
        ServerKind::Stock => tracks::installed(&cfg.game_dir),
        ServerKind::Native => tracks::native_packages(&cfg.tracks_dir()),
    };
    json(200, &serde_json::json!({ "tracks": found }))
}

fn capabilities(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let cfg = shared.lock().unwrap().config().clone();
    json(
        200,
        &serde_json::json!({
            "apiVersion": 1,
            "kind": if cfg.kind == ServerKind::Native { "native" } else { "stock" },
            "actions": {
                "start": true, "stop": true, "restart": true, "trackSelect": true,
                "trackUpload": cfg.kind == ServerKind::Native,
                "versionUpload": cfg.kind == ServerKind::Native,
                "bots": cfg.kind == ServerKind::Native,
                "sessions": cfg.kind == ServerKind::Native && cfg.native_admin.is_some()
            },
            "limits": { "trackBytes": MAX_TRACK_BYTES, "versionBytes": MAX_BINARY_BYTES }
        }),
    )
}

fn read_config(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let guard = shared.lock().unwrap();
    if guard.config().kind == ServerKind::Native {
        return match guard
            .read_server_config()
            .and_then(|text| native::view(&text))
        {
            Ok(settings) => json(200, &serde_json::json!({ "server": settings })),
            Err(error) => json(500, &serde_json::json!({ "error": error })),
        };
    }
    match guard.read_ini() {
        Ok(text) => json(200, &serde_json::json!({ "ini": text })),
        Err(error) => json(500, &serde_json::json!({ "error": error })),
    }
}

fn act<F>(shared: &Shared, f: F) -> Response<std::io::Cursor<Vec<u8>>>
where
    F: FnOnce(&mut Supervisor) -> Result<(), String>,
{
    let mut guard = shared.lock().unwrap();
    match f(&mut guard) {
        Ok(()) => {
            let game = guard.status();
            json(200, &serde_json::json!({ "ok": true, "game": game }))
        }
        Err(e) => json(500, &serde_json::json!({ "error": e })),
    }
}

/// The subset of server settings the app may change. Anything else is a hand edit on the
/// box — this is an API for running a server, not for rewriting arbitrary game config.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConfigPatch {
    track: Option<String>,
    name: Option<String>,
    max_clients: Option<u32>,
}

fn update_config(shared: &Shared, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    if shared.lock().unwrap().config().kind == ServerKind::Native {
        let patch: native::Patch = match serde_json::from_str(body) {
            Ok(p) => p,
            Err(e) => {
                return json(
                    400,
                    &serde_json::json!({ "error": format!("bad JSON: {e}") }),
                )
            }
        };
        let mut guard = shared.lock().unwrap();
        let tracks = tracks::native_packages(&guard.config().tracks_dir());
        let text = match guard.read_server_config() {
            Ok(text) => text,
            Err(error) => return json(500, &serde_json::json!({ "error": error })),
        };
        let updated = match native::patch(&text, &patch, &tracks) {
            Ok(text) => text,
            Err(error) => return json(400, &serde_json::json!({ "error": error })),
        };
        if let Err(error) = guard.write_server_config(&updated) {
            return json(500, &serde_json::json!({ "error": error }));
        }
        if let Err(error) = guard.restart() {
            return json(
                500,
                &serde_json::json!({ "error": format!("config saved, restart failed: {error}") }),
            );
        }
        return json(
            200,
            &serde_json::json!({
                "ok": true, "restarted": true, "game": guard.status(),
                "server": native::view(&updated).ok()
            }),
        );
    }
    let patch: ConfigPatch = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(e) => {
            return json(
                400,
                &serde_json::json!({ "error": format!("bad JSON: {e}") }),
            )
        }
    };

    let mut guard = shared.lock().unwrap();
    let mut text = match guard.read_ini() {
        Ok(t) => t,
        Err(e) => return json(500, &serde_json::json!({ "error": e })),
    };

    let mut changed = false;
    if let Some(track) = patch.track.as_deref() {
        // Values go onto the game's command-free config, but a newline would still let a
        // caller inject an unrelated key — so reject anything that isn't one line.
        if track.contains(['\n', '\r']) {
            return json(
                400,
                &serde_json::json!({ "error": "track must be a single line" }),
            );
        }
        text = ini::set(&text, ini::TRACK, track);
        changed = true;
    }
    if let Some(name) = patch.name.as_deref() {
        if name.contains(['\n', '\r']) {
            return json(
                400,
                &serde_json::json!({ "error": "name must be a single line" }),
            );
        }
        text = ini::set(&text, ini::NAME, name);
        changed = true;
    }
    if let Some(max) = patch.max_clients {
        text = ini::set(&text, ini::MAX_CLIENT, &max.to_string());
        changed = true;
    }

    if !changed {
        return json(400, &serde_json::json!({ "error": "nothing to change" }));
    }
    if let Err(e) = guard.write_ini(&text) {
        return json(500, &serde_json::json!({ "error": e }));
    }
    // The game reads its .ini once at startup, so a config change only means anything
    // after a restart. Doing it here keeps the API honest about what it just did.
    if let Err(e) = guard.restart() {
        return json(
            500,
            &serde_json::json!({ "error": format!("config saved, restart failed: {e}") }),
        );
    }
    let game = guard.status();
    json(
        200,
        &serde_json::json!({ "ok": true, "restarted": true, "game": game }),
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionAction {
    action: String,
    #[serde(default)]
    to: Option<String>,
}

fn session_action(shared: &Shared, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let action: SessionAction = match serde_json::from_str(body) {
        Ok(action) => action,
        Err(error) => {
            return json(
                400,
                &serde_json::json!({ "error": format!("bad JSON: {error}") }),
            )
        }
    };
    let cfg = shared.lock().unwrap().config().clone();
    if cfg.kind != ServerKind::Native {
        return json(
            409,
            &serde_json::json!({ "error": "session control requires native mxbserver" }),
        );
    }
    let (Some(addr), Some(token)) = (
        cfg.native_admin.as_deref(),
        cfg.native_admin_token.as_deref(),
    ) else {
        return json(
            409,
            &serde_json::json!({ "error": "native admin is not configured" }),
        );
    };
    let (path, payload) = match action.action.as_str() {
        "jump" => {
            let Some(to) = action.to.as_deref() else {
                return json(
                    400,
                    &serde_json::json!({ "error": "jump needs a destination" }),
                );
            };
            if !matches!(to, "practice" | "qualifying" | "warmup" | "race") {
                return json(
                    400,
                    &serde_json::json!({ "error": "session must be practice, qualifying, warmup or race" }),
                );
            }
            (
                "/v1/session/jump",
                serde_json::json!({ "to": to }).to_string(),
            )
        }
        "advance" => ("/v1/session/advance", "{}".into()),
        "restart" => ("/v1/session/restart", "{}".into()),
        _ => {
            return json(
                400,
                &serde_json::json!({ "error": "unknown session action" }),
            )
        }
    };
    match native_admin::request(addr, token, "POST", path, &payload) {
        Ok(value) => json(200, &value),
        Err(error) => json(502, &serde_json::json!({ "error": error })),
    }
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().trim().to_string())
}

fn safe_upload_name(name: &str, extension: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('.')
        || name.contains(['/', '\\', '\n', '\r'])
    {
        return None;
    }
    let path = Path::new(name);
    (path.components().count() == 1
        && path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case(extension)))
    .then(|| name.to_string())
}

fn receive(
    request: &mut Request,
    target: &Path,
    maximum: u64,
    expected: &str,
) -> Result<u64, String> {
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("X-Content-SHA256 must be a hexadecimal SHA-256 digest".into());
    }
    if let Some(length) = request.body_length() {
        if length as u64 > maximum {
            return Err(format!("upload exceeds the {maximum}-byte limit"));
        }
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("couldn't create upload directory: {e}"))?;
    }
    let temporary = target.with_extension("uploading");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| format!("couldn't stage upload: {e}"))?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = request
            .as_reader()
            .read(&mut buffer)
            .map_err(|e| format!("upload failed: {e}"))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > maximum {
            let _ = fs::remove_file(&temporary);
            return Err(format!("upload exceeds the {maximum}-byte limit"));
        }
        digest.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|e| format!("couldn't stage upload: {e}"))?;
    }
    file.sync_all()
        .map_err(|e| format!("couldn't flush upload: {e}"))?;
    let actual = format!("{:x}", digest.finalize());
    if !actual.eq_ignore_ascii_case(expected) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("SHA-256 mismatch: received {actual}"));
    }
    Ok(total)
}

fn install_staged(target: &Path) -> Result<Option<PathBuf>, String> {
    let staged = target.with_extension("uploading");
    let backup = target.with_extension("previous");
    let had_previous = target.exists();
    if had_previous {
        let _ = fs::remove_file(&backup);
        fs::rename(target, &backup)
            .map_err(|e| format!("couldn't back up {}: {e}", target.display()))?;
    }
    if let Err(error) = fs::rename(&staged, target) {
        if had_previous {
            let _ = fs::rename(&backup, target);
        }
        return Err(format!("couldn't install {}: {error}", target.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if target.extension().is_none() {
            let _ = fs::set_permissions(target, fs::Permissions::from_mode(0o755));
        }
    }
    Ok(had_previous.then_some(backup))
}

fn upload_track(shared: &Shared, request: &mut Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let cfg = shared.lock().unwrap().config().clone();
    if cfg.kind != ServerKind::Native {
        return json(
            409,
            &serde_json::json!({ "error": "track upload requires native mxbserver" }),
        );
    }
    let Some(filename) = header(request, "X-Filename").and_then(|v| safe_upload_name(&v, "pkz"))
    else {
        return json(
            400,
            &serde_json::json!({ "error": "X-Filename must be a plain .pkz file name" }),
        );
    };
    let Some(digest) = header(request, "X-Content-SHA256") else {
        return json(
            400,
            &serde_json::json!({ "error": "X-Content-SHA256 is required" }),
        );
    };
    let target = cfg.tracks_dir().join(&filename);
    let bytes = match receive(request, &target, MAX_TRACK_BYTES, &digest) {
        Ok(bytes) => bytes,
        Err(error) => return json(400, &serde_json::json!({ "error": error })),
    };
    match install_staged(&target) {
        Ok(backup) => {
            if let Some(path) = backup {
                let _ = fs::remove_file(path);
            }
            json(
                200,
                &serde_json::json!({ "ok": true, "track": Path::new(&filename).file_stem().and_then(|v| v.to_str()), "bytes": bytes, "sha256": digest.to_ascii_lowercase() }),
            )
        }
        Err(error) => json(500, &serde_json::json!({ "error": error })),
    }
}

fn upload_version(shared: &Shared, request: &mut Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let cfg = shared.lock().unwrap().config().clone();
    if cfg.kind != ServerKind::Native {
        return json(
            409,
            &serde_json::json!({ "error": "version upload requires native mxbserver" }),
        );
    }
    let Some(digest) = header(request, "X-Content-SHA256") else {
        return json(
            400,
            &serde_json::json!({ "error": "X-Content-SHA256 is required" }),
        );
    };
    let version =
        header(request, "X-Version").unwrap_or_else(|| digest[..digest.len().min(12)].to_string());
    if version.is_empty() || version.len() > 80 || version.contains(['\n', '\r']) {
        return json(400, &serde_json::json!({ "error": "X-Version is invalid" }));
    }
    let target = cfg.exe_path();
    let bytes = match receive(request, &target, MAX_BINARY_BYTES, &digest) {
        Ok(bytes) => bytes,
        Err(error) => return json(400, &serde_json::json!({ "error": error })),
    };
    let mut guard = shared.lock().unwrap();
    if let Err(error) = guard.stop() {
        return json(500, &serde_json::json!({ "error": error }));
    }
    let backup = match install_staged(&target) {
        Ok(backup) => backup,
        Err(error) => {
            let _ = guard.start();
            return json(500, &serde_json::json!({ "error": error }));
        }
    };
    let start_error = match guard.start() {
        Err(error) => Some(error),
        Ok(()) => {
            // `spawn` alone accepts a binary that exits immediately on this host. Give the
            // process one short grace window so a bad-format/crash-on-start upload rolls back.
            std::thread::sleep(Duration::from_millis(750));
            (!guard.is_alive()).then(|| "the new process exited during startup".to_string())
        }
    };
    if let Some(error) = start_error {
        let _ = fs::remove_file(&target);
        if let Some(ref old) = backup {
            let _ = fs::rename(old, &target);
        }
        let rollback = guard.start().is_ok();
        return json(
            500,
            &serde_json::json!({ "error": format!("new version failed to start: {error}"), "rolledBack": rollback }),
        );
    }
    if let Some(old) = backup {
        let _ = fs::remove_file(old);
    }
    if let Err(error) = fs::write(cfg.version_path(), format!("{version}\n")) {
        return json(
            500,
            &serde_json::json!({ "error": format!("server updated, but version marker failed: {error}") }),
        );
    }
    json(
        200,
        &serde_json::json!({ "ok": true, "version": version, "bytes": bytes, "sha256": digest.to_ascii_lowercase(), "game": guard.status() }),
    )
}

fn json(code: u16, value: &serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("a literal header is always valid");
    Response::from_data(body)
        .with_status_code(code)
        .with_header(header)
}
