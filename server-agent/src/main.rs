//! mxb-agent — supervises an MX Bikes dedicated server and exposes it over HTTP.
//!
//! Runs on the game host next to `mxbikes.exe`. The desktop app talks to this rather than
//! to the cloud provider, which is the point: managing a server from the app must never
//! require provider credentials to be shipped inside the app.

mod bikes;
mod config;
mod ini;
mod pairing;
mod roster;
mod supervisor;
mod tracks;

use config::Config;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use supervisor::Supervisor;
use tiny_http::{Header, Request, Response, Server};

/// How often the crash watcher checks on the game.
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

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

    // The server-browser view, and the other endpoint that cannot require a token.
    //
    // What it answers — who is on, what is being ridden, what the host will accept — is the
    // question a player asks *before* they have anything to do with this box, so a
    // credential they could only get from its operator is the wrong gate. It is also
    // already public in every meaningful sense: the game advertises the same server in its
    // own in-game list, and anyone can join and read the grid.
    //
    // What is *not* here is the point. GUIDs are a stable identity and stay behind the token
    // on `/players`; so does everything that changes the box. This is a read of facts a
    // player standing on the server would see anyway.
    if method == "GET" && path == "/info" {
        let _ = request.respond(info(shared));
        return;
    }

    if !authorized(&request, shared) {
        let _ = request.respond(json(401, &serde_json::json!({ "error": "unauthorized" })));
        return;
    }

    let response = match (method.as_str(), path.as_str()) {
        ("GET", "/status") => status(shared),
        ("GET", "/players") => players(shared),
        ("GET", "/tracks") => installed_tracks(shared),
        ("POST", "/start") => act(shared, |s| s.start()),
        ("POST", "/stop") => act(shared, |s| s.stop()),
        ("POST", "/restart") => act(shared, |s| s.restart()),
        ("GET", "/config") => match shared.lock().unwrap().read_ini() {
            Ok(text) => json(200, &serde_json::json!({ "ini": text })),
            Err(e) => json(500, &serde_json::json!({ "error": e })),
        },
        ("PUT", "/config") => {
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                json(400, &serde_json::json!({ "error": "couldn't read the request body" }))
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
    let port = guard.config().game_port;
    // Config is best-effort: a server whose .ini we can't read is still worth reporting on.
    let text = guard.read_ini().unwrap_or_default();
    json(
        200,
        &serde_json::json!({
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

/// Everything a server browser shows, in one unauthenticated read.
///
/// One call rather than four (`/status` + `/players` + `/tracks` + a bike list) because the
/// caller is a list of servers being refreshed, not an operator inspecting one: four
/// round-trips per row over a link that may be transatlantic is what makes a browser feel
/// broken.
///
/// Players are reported by name only. The roster this reads carries GUIDs — that is the
/// whole reason it parses the log — and a GUID is a stable per-install identity, so it stays
/// behind the token on `/players`. A name is what the grid already shows to everyone on it.
fn info(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut guard = shared.lock().unwrap();
    let game = guard.status();
    let port = guard.config().game_port;
    let game_dir = guard.config().game_dir.clone();
    // Best-effort, exactly as in `status`: a server whose `.ini` we cannot read is still
    // worth listing, and answering with an error would drop it out of the browser entirely.
    let text = guard.read_ini().unwrap_or_default();
    let name = ini::get(&text, ini::NAME);
    let track = ini::get(&text, ini::TRACK);
    let track_layout = ini::get(&text, ini::TRACK_LAYOUT);
    let max_clients = ini::get(&text, ini::MAX_CLIENT);
    // Whether joining needs a password. The password itself is never reported — only that
    // there is one, which is what a browser has to show and a player has to know.
    let locked = ini::get(&text, ini::PASSWORD).is_some_and(|p| !p.trim().is_empty());
    drop(guard);

    let players: Vec<String> = std::fs::read_to_string(game_dir.join("log.txt"))
        .map(|log| roster::connected(&log).into_iter().map(|p| p.name).collect())
        // No log yet is a server nobody has connected to, not a failure.
        .unwrap_or_default();

    json(
        200,
        &serde_json::json!({
            "name": name,
            "track": track,
            "trackLayout": track_layout,
            "maxClients": max_clients,
            "locked": locked,
            "port": port,
            "running": game.running,
            "uptimeSecs": game.uptime_secs,
            "playerCount": players.len(),
            "players": players,
            "tracks": tracks::installed(&game_dir),
            // The list that decides whether a player is turned away at the gate.
            "bikes": bikes::installed(&game_dir),
        }),
    )
}

/// Who is connected right now, with the GUID that identifies them stably.
fn players(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let guard = shared.lock().unwrap();
    let log = guard.config().game_dir.join("log.txt");
    match std::fs::read_to_string(&log) {
        Ok(text) => json(200, &serde_json::json!({ "players": roster::connected(&text) })),
        // No log yet is a server that has not been connected to, not a failure.
        Err(_) => json(200, &serde_json::json!({ "players": [] })),
    }
}

/// The tracks this host can actually run, so the app offers a list instead of a text box
/// the operator has to spell a track name into from memory.
fn installed_tracks(shared: &Shared) -> Response<std::io::Cursor<Vec<u8>>> {
    let guard = shared.lock().unwrap();
    let dir = guard.config().game_dir.clone();
    drop(guard);
    json(200, &serde_json::json!({ "tracks": tracks::installed(&dir) }))
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
    let patch: ConfigPatch = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(e) => return json(400, &serde_json::json!({ "error": format!("bad JSON: {e}") })),
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
            return json(400, &serde_json::json!({ "error": "track must be a single line" }));
        }
        text = ini::set(&text, ini::TRACK, track);
        changed = true;
    }
    if let Some(name) = patch.name.as_deref() {
        if name.contains(['\n', '\r']) {
            return json(400, &serde_json::json!({ "error": "name must be a single line" }));
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
        return json(500, &serde_json::json!({ "error": format!("config saved, restart failed: {e}") }));
    }
    let game = guard.status();
    json(200, &serde_json::json!({ "ok": true, "restarted": true, "game": game }))
}

fn json(code: u16, value: &serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("a literal header is always valid");
    Response::from_data(body).with_status_code(code).with_header(header)
}
