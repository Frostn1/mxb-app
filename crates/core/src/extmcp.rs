//! MCP for AI agents: build, pack, install and test extensions against a running app, and
//! call the tools extensions offer.
//!
//! Two halves, one binary:
//!
//!   * **The server**, inside the running app ([`start`]). It listens on 127.0.0.1 (a random
//!     port) and writes the port and a fresh per-launch token to `mcp-endpoint.json` in the
//!     app's data folder, which only this Windows/macOS/Linux user can read. A connection must
//!     present the token first, and gets nothing while "Let AI agents connect" is off (it is
//!     off until the person turns it on, in Extensions -> Build).
//!   * **The bridge**: the same executable started as `MXB App --mcp` ([`bridge_cli`]). It is
//!     what an MCP client (Claude Desktop, Claude Code, Cursor...) launches: MCP over stdio,
//!     relayed line by line to the server. It never runs the app itself.
//!
//! The server speaks MCP (JSON-RPC 2.0, one message per line). What needs the UI - an
//! install's consent dialog, opening a tab, an extension's own tools, a tab's console - goes
//! to the webview as an `ext-mcp` event and comes back through [`ext_mcp_reply`].
//!
//! What an agent can never do here: grant a permission (installs go through the same dialog,
//! clicked by the person), reach anything the extension API doesn't, or connect from another
//! machine.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};
use tauri::Emitter;

use crate::extensions::{self as ext, Trust};

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const ENDPOINT_FILE: &str = "mcp-endpoint.json";
const MAX_LINE: usize = 4 * 1024 * 1024;

fn data_dir() -> Option<PathBuf> {
    // Same folder as `config::data_dir`, which needs an AppHandle the bridge doesn't have.
    if let Some(root) = crate::config::dev_profile_root() {
        return Some(root.join("data"));
    }
    Some(dirs_next::data_local_dir()?.join(crate::config::DATA_ID))
}

// ---------------------------------------------------------------------------------------
// The bridge: `MXB App --mcp`.
// ---------------------------------------------------------------------------------------

/// If this process was started as the MCP bridge, run it and return the exit code.
pub fn bridge_cli() -> Option<i32> {
    if !std::env::args().skip(1).any(|a| a == "--mcp") {
        return None;
    }
    Some(match connect() {
        Ok(stream) => relay(stream),
        Err(why) => offline(&why),
    })
}

fn connect() -> Result<TcpStream, String> {
    let not_running = "MXB App isn't running. Start it, then open Extensions -> Build and turn on \"Let AI agents connect\".";
    let path = data_dir().ok_or("no app data folder")?.join(ENDPOINT_FILE);
    let body = std::fs::read(&path).map_err(|_| not_running.to_string())?;
    let ep: Value = serde_json::from_slice(&body).map_err(|_| not_running.to_string())?;
    let port = ep.get("port").and_then(|p| p.as_u64()).ok_or(not_running)? as u16;
    let token = ep.get("token").and_then(|t| t.as_str()).ok_or(not_running)?.to_string();
    let mut stream = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(3))
        .map_err(|_| not_running.to_string())?;
    writeln!(stream, "{}", json!({ "token": token })).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut hello = String::new();
    reader.read_line(&mut hello).map_err(|_| not_running.to_string())?;
    let hello: Value = serde_json::from_str(hello.trim()).map_err(|_| not_running.to_string())?;
    if let Some(err) = hello.get("error").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    Ok(stream)
}

/// stdin -> app, app -> stdout, until either side closes.
fn relay(stream: TcpStream) -> i32 {
    let mut up = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return 1,
    };
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if writeln!(up, "{line}").is_err() {
                break;
            }
        }
        let _ = up.shutdown(std::net::Shutdown::Both);
        std::process::exit(0);
    });
    let mut out = std::io::stdout();
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
    0
}

/// No app to talk to: still a valid MCP server, whose one tool says why.
fn offline(why: &str) -> i32 {
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = msg.get("id").cloned() else { continue };
        let result = match msg.get("method").and_then(|m| m.as_str()).unwrap_or("") {
            // Offline there's no app to ask; this is the bridge's own build.
            "initialize" => initialize_result(&msg, "offline", env!("CARGO_PKG_VERSION")),
            "tools/list" => json!({ "tools": [tool("app_status", "Whether MXB App is running and accepting agents.", json!({}))] }),
            "tools/call" => text_result(why, true),
            "ping" => json!({}),
            _ => {
                let _ = writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "method not found" } }));
                continue;
            }
        };
        let _ = writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": id, "result": result }));
        let _ = out.flush();
    }
    0
}

// ---------------------------------------------------------------------------------------
// The server, inside the app.
// ---------------------------------------------------------------------------------------

struct Server {
    app: tauri::AppHandle,
    token: String,
    port: u16,
}

static SERVER: OnceLock<Server> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);

fn pending() -> &'static Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>> {
    static P: OnceLock<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start listening. Called once from the app's setup; does nothing on a second call.
pub fn start(app: tauri::AppHandle) {
    if SERVER.get().is_some() {
        return;
    }
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("[mcp] no loopback port: {e}");
            return;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    if let Some(dir) = data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let body = json!({ "port": port, "token": token, "pid": std::process::id() });
        if let Err(e) = std::fs::write(dir.join(ENDPOINT_FILE), body.to_string()) {
            log::warn!("[mcp] can't write the endpoint file: {e}");
        }
    }
    if SERVER.set(Server { app, token, port }).is_err() {
        return;
    }
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else { continue };
            std::thread::spawn(move || {
                let _ = serve(conn);
            });
        }
    });
    log::info!("[mcp] listening on 127.0.0.1:{port}");
}

fn same(a: &str, b: &str) -> bool {
    // Constant time over the presented token's length.
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn serve(conn: TcpStream) -> std::io::Result<()> {
    let server = SERVER.get().expect("started");
    conn.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut out = conn.try_clone()?;
    let mut reader = BufReader::new(conn);
    let mut first = String::new();
    reader.by_ref().take(4096).read_line(&mut first)?;
    let presented = serde_json::from_str::<Value>(first.trim())
        .ok()
        .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_default();
    if !same(&presented, &server.token) {
        writeln!(out, "{}", json!({ "error": "wrong token" }))?;
        return Ok(());
    }
    if !ext::load_state(&server.app).mcp_enabled {
        writeln!(out, "{}", json!({ "error": "AI agents are turned off in MXB App. Turn on \"Let AI agents connect\" in Extensions -> Build." }))?;
        return Ok(());
    }
    writeln!(out, "{}", json!({ "ok": true }))?;
    reader.get_ref().set_read_timeout(None)?;
    log::info!("[mcp] an agent connected");

    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.by_ref().take(MAX_LINE as u64).read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
            writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } }))?;
            continue;
        };
        // Turned off mid-session: stop answering.
        if !ext::load_state(&server.app).mcp_enabled {
            break;
        }
        let Some(id) = msg.get("id").cloned() else { continue }; // a notification
        let reply = match handle(&server.app, &msg) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        };
        writeln!(out, "{reply}")?;
        out.flush()?;
    }
    Ok(())
}

fn initialize_result(msg: &Value, state: &str, version: &str) -> Value {
    let asked = msg.pointer("/params/protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": asked,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "mxb-app", "version": version },
        "instructions": format!(
            "MXB App ({state}). Build MXB App extensions: read docs/extensions.md, write a folder with extension.json, then extension_validate, extension_install (the person approves it in the app), extension_open and extension_logs to test, extension_pack to ship. Extension tools appear as ext_<id>__<tool>."
        ),
    })
}

fn tool(name: &str, description: &str, properties: Value) -> Value {
    let required: Vec<String> = properties
        .as_object()
        .map(|o| o.iter().filter(|(_, v)| v.get("optional").is_none()).map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    let props: BTreeMap<String, Value> = properties
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| {
                    let mut v = v.clone();
                    if let Some(m) = v.as_object_mut() {
                        m.remove("optional");
                    }
                    (k.clone(), v)
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": props, "required": required } })
}

fn text_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn json_result(v: &Value) -> Value {
    text_result(&serde_json::to_string_pretty(v).unwrap_or_default(), false)
}

fn dev_tools() -> Vec<Value> {
    let s = |d: &str| json!({ "type": "string", "description": d });
    let opt = |d: &str| json!({ "type": "string", "description": d, "optional": true });
    vec![
        tool("app_status", "MXB App's version, developer mode, and how many extensions are installed.", json!({})),
        tool("extensions_list", "Installed extensions: id, version, enabled, trust, permissions asked and granted, tabs, commands and AI tools.", json!({})),
        tool("extension_validate", "Check an extension folder or .mxbext the way the app does before installing: manifest, permissions, file types, every file the manifest names. Returns the problems, or a summary.", json!({ "path": s("Absolute path to the extension folder or .mxbext") })),
        tool("extension_pack", "Pack a folder into a .mxbext, signed if a key is given (an Ed25519 PKCS#8 .pem, e.g. from `node scripts/extensions/pack.mjs --keygen`).", json!({ "path": s("Absolute path to the extension folder"), "key": opt("Absolute path to the signing key .pem"), "out": opt("Where to write the .mxbext (default: beside the folder)") })),
        tool("extension_install", "Install a folder (needs developer mode) or .mxbext into the running app. The app shows its consent dialog and the person decides; this waits for them (up to 5 minutes) and says what happened.", json!({ "path": s("Absolute path to the extension folder or .mxbext") })),
        tool("extension_open", "Show one of an extension's tabs in the app, to see it run.", json!({ "id": s("Extension id"), "view": opt("View id (default: the first)") })),
        tool("extension_logs", "The last console messages and errors from an extension's tabs and background page.", json!({ "id": s("Extension id") })),
        tool("extension_set_enabled", "Turn an installed extension off. Turning one on needs developer mode.", json!({ "id": s("Extension id"), "enabled": { "type": "boolean" } })),
        tool("extension_tool_call", "Call an AI tool an installed extension offers.", json!({ "extension": s("Extension id"), "tool": s("Tool name"), "arguments": { "type": "object", "optional": true } })),
    ]
}

/// `ext_<id with dots as _>__<tool>`, inside MCP's 64-character name limit.
pub fn ext_tool_name(id: &str, tool: &str) -> String {
    let mut n = format!("ext_{}__{}", id.replace(['.', '-'], "_"), tool);
    n.truncate(64);
    n
}

/// Ask the webview to do something only it can, and wait for its answer.
fn ui(app: &tauri::AppHandle, op: &str, params: Value, wait: Duration) -> Result<Value, String> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    pending().lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
    app.emit("ext-mcp", json!({ "id": id, "op": op, "params": params })).map_err(|e| e.to_string())?;
    let out = rx.recv_timeout(wait).map_err(|_| "the app didn't answer in time".to_string());
    pending().lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    out?
}

/// The webview's answer to an `ext-mcp` request.
#[tauri::command]
pub fn ext_mcp_reply(id: u64, ok: bool, result: Value) {
    if let Some(tx) = pending().lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
        let _ = tx.send(if ok { Ok(result) } else { Err(result.as_str().unwrap_or("failed").to_string()) });
    }
}

fn arg<'a>(msg: &'a Value, key: &str) -> Option<&'a Value> {
    msg.pointer("/params/arguments").and_then(|a| a.get(key))
}

fn arg_str(msg: &Value, key: &str) -> Result<String, String> {
    arg(msg, key).and_then(|v| v.as_str()).map(String::from).ok_or_else(|| format!("missing '{key}'"))
}

type RpcError = (i64, String);

fn handle(app: &tauri::AppHandle, msg: &Value) -> Result<Value, RpcError> {
    match msg.get("method").and_then(|m| m.as_str()).unwrap_or("") {
        "initialize" => Ok(initialize_result(msg, "connected", &app.package_info().version.to_string())),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut tools = dev_tools();
            if let Ok(Value::Array(list)) = ui(app, "tools", json!({}), Duration::from_secs(5)) {
                for t in list {
                    let (Some(e), Some(n)) = (t.get("ext").and_then(|v| v.as_str()), t.get("name").and_then(|v| v.as_str())) else { continue };
                    let schema = t.get("inputSchema").cloned().filter(|s| s.is_object()).unwrap_or(json!({ "type": "object" }));
                    let desc = t.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    tools.push(json!({ "name": ext_tool_name(e, n), "description": format!("{desc} (from the extension {e})"), "inputSchema": schema }));
                }
            }
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => Ok(match call(app, msg) {
            Ok(v) => json_result(&v),
            Err(e) => text_result(&e, true),
        }),
        _ => Err((-32601, "method not found".into())),
    }
}

fn call(app: &tauri::AppHandle, msg: &Value) -> Result<Value, String> {
    let name = msg.pointer("/params/name").and_then(|v| v.as_str()).unwrap_or("");
    match name {
        "app_status" => {
            let st = ext::load_state(app);
            Ok(json!({
                "app": "MXB App",
                "version": app.package_info().version.to_string(),
                "developerMode": st.developer_mode,
                "installed": st.extensions.len(),
            }))
        }
        "extensions_list" => Ok(serde_json::to_value(ext::ext_list(app.clone())).map_err(|e| e.to_string())?),
        "extension_validate" => {
            let path = PathBuf::from(arg_str(msg, "path")?);
            let pkg = open_path(app, &path)?;
            Ok(summary(&pkg))
        }
        "extension_pack" => {
            let folder = PathBuf::from(arg_str(msg, "path")?);
            let key = arg(msg, "key").and_then(|v| v.as_str()).map(PathBuf::from);
            let out = arg(msg, "out").and_then(|v| v.as_str()).map(PathBuf::from);
            pack(app, &folder, key.as_deref(), out.as_deref())
        }
        "extension_install" => {
            let path = arg_str(msg, "path")?;
            ui(app, "install", json!({ "path": path }), Duration::from_secs(300))
        }
        "extension_open" => {
            let id = arg_str(msg, "id")?;
            let view = arg(msg, "view").and_then(|v| v.as_str()).map(String::from);
            ui(app, "open", json!({ "id": id, "view": view }), Duration::from_secs(10))
        }
        "extension_logs" => ui(app, "logs", json!({ "id": arg_str(msg, "id")? }), Duration::from_secs(5)),
        "extension_set_enabled" => {
            let id = arg_str(msg, "id")?;
            let enabled = arg(msg, "enabled").and_then(|v| v.as_bool()).ok_or("missing 'enabled'")?;
            agent_may_set_enabled(enabled, ext::load_state(app).developer_mode)?;
            ext::ext_set_enabled(app.clone(), id.clone(), enabled)?;
            let _ = ui(app, "refresh", json!({}), Duration::from_secs(5));
            Ok(json!({ "id": id, "enabled": enabled }))
        }
        "extension_tool_call" => ui(
            app,
            "call",
            json!({ "ext": arg_str(msg, "extension")?, "tool": arg_str(msg, "tool")?, "args": arg(msg, "arguments").cloned().unwrap_or(json!({})) }),
            Duration::from_secs(15),
        ),
        other if other.starts_with("ext_") => {
            // Map the flattened name back through the live tool list.
            let list = ui(app, "tools", json!({}), Duration::from_secs(5))?;
            let found = list.as_array().and_then(|l| {
                l.iter().find(|t| {
                    let (Some(e), Some(n)) = (t.get("ext").and_then(|v| v.as_str()), t.get("name").and_then(|v| v.as_str())) else { return false };
                    ext_tool_name(e, n) == other
                })
            });
            let t = found.ok_or_else(|| format!("no extension tool {other}"))?;
            let args = msg.pointer("/params/arguments").cloned().unwrap_or(json!({}));
            ui(app, "call", json!({ "ext": t["ext"], "tool": t["name"], "args": args }), Duration::from_secs(15))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

fn agent_may_set_enabled(enabled: bool, developer_mode: bool) -> Result<(), String> {
    if enabled && !developer_mode {
        return Err("Turning an extension on from an agent needs developer mode. Ask the person to do it in MXB App.".into());
    }
    Ok(())
}

fn open_path(app: &tauri::AppHandle, path: &Path) -> Result<ext::Package, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let files = if meta.is_dir() {
        ext::read_folder(path)
    } else {
        std::fs::read(path).map_err(|e| e.into()).and_then(|b| ext::read_zip(&b))
    }
    .map_err(|e| e.to_string())?;
    ext::open_package(files, &app.package_info().version.to_string()).map_err(|e| e.to_string())
}

fn summary(pkg: &ext::Package) -> Value {
    let c = &pkg.manifest.contributes;
    let count = |k: &str| c.get(k).and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    json!({
        "ok": true,
        "id": pkg.manifest.id,
        "version": pkg.manifest.version,
        "trust": pkg.trust,
        "signedBy": pkg.key,
        "permissions": pkg.manifest.permissions,
        "files": pkg.files.len(),
        "themes": count("themes"),
        "views": count("views"),
        "commands": count("commands"),
        "tools": c.pointer("/mcp/tools").and_then(|t| t.as_array()).map(|a| a.len()).unwrap_or(0),
        "note": if pkg.trust == Trust::Unsigned { "unsigned: installs only with developer mode on" } else { "" },
    })
}

/// The 32-byte seed out of an Ed25519 PKCS#8 PEM (what `pack.mjs --keygen` writes).
pub fn signing_key_from_pem(pem: &str) -> Result<ed25519_dalek::SigningKey, String> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let der = B64.decode(body.trim()).map_err(|_| "that key file isn't a PEM".to_string())?;
    const PREFIX: [u8; 16] = [0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
    if der.len() != 48 || der[..16] != PREFIX {
        return Err("that key isn't an Ed25519 PKCS#8 private key".into());
    }
    let seed: [u8; 32] = der[16..].try_into().map_err(|_| "bad key length".to_string())?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed))
}

/// Pack (and sign) a folder into a `.mxbext`, then check the result like an install would.
pub fn pack_files(mut files: BTreeMap<String, Vec<u8>>, key: Option<&ed25519_dalek::SigningKey>) -> Result<Vec<u8>, String> {
    use ed25519_dalek::Signer;
    files.remove("signature.json");
    if let Some(k) = key {
        let sig = k.sign(&ext::digest_input(&files));
        let body = json!({ "publicKey": B64.encode(k.verifying_key().to_bytes()), "signature": B64.encode(sig.to_bytes()) });
        files.insert("signature.json".into(), serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in &files {
            z.start_file(path.as_str(), opts).map_err(|e| e.to_string())?;
            z.write_all(bytes).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

fn pack(app: &tauri::AppHandle, folder: &Path, key: Option<&Path>, out: Option<&Path>) -> Result<Value, String> {
    let files = ext::read_folder(folder).map_err(|e| e.to_string())?;
    let signing = match key {
        Some(k) => Some(signing_key_from_pem(&std::fs::read_to_string(k).map_err(|e| format!("{}: {e}", k.display()))?)?),
        None => None,
    };
    let bytes = pack_files(files, signing.as_ref())?;
    let pkg = ext::open_package(ext::read_zip(&bytes).map_err(|e| e.to_string())?, &app.package_info().version.to_string())
        .map_err(|e| e.to_string())?;
    let dest = match out {
        Some(o) => o.to_path_buf(),
        None => folder
            .parent()
            .unwrap_or(folder)
            .join(format!("{}-{}.mxbext", pkg.manifest.id, pkg.manifest.version)),
    };
    std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut s = summary(&pkg);
    s["path"] = json!(dest.to_string_lossy());
    s["bytes"] = json!(bytes.len());
    Ok(s)
}

// ---------------------------------------------------------------------------------------
// Settings for the Build page.
// ---------------------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub listening: bool,
    /// This executable, for the client config: `{ "command": exe, "args": ["--mcp"] }`.
    pub command: String,
}

#[tauri::command]
pub fn ext_mcp_status(app: tauri::AppHandle) -> McpStatus {
    McpStatus {
        enabled: ext::load_state(&app).mcp_enabled,
        listening: SERVER.get().map(|s| s.port != 0).unwrap_or(false),
        command: std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
    }
}

#[tauri::command]
pub fn ext_set_mcp_enabled(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    ext::set_mcp_enabled(&app, on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_fit_mcp() {
        assert_eq!(ext_tool_name("mxbsecure.coach-laps", "best_laps"), "ext_mxbsecure_coach_laps__best_laps");
        let long = ext_tool_name(&"a.b".repeat(30), "tool");
        assert!(long.len() <= 64);
        assert!(long.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    }

    #[test]
    fn a_pack_script_key_signs_a_package_the_app_accepts() {
        // The same PKCS#8 layout `pack.mjs --keygen` and node's crypto write.
        let seed = [9u8; 32];
        let mut der = vec![0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
        der.extend_from_slice(&seed);
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", B64.encode(&der));
        let key = signing_key_from_pem(&pem).unwrap();
        let mut files = BTreeMap::new();
        files.insert(
            "extension.json".to_string(),
            br#"{"manifestVersion":1,"id":"someone.packed","name":"P","version":"1.0.0","author":"A","permissions":[],"contributes":{}}"#.to_vec(),
        );
        let bytes = pack_files(files, Some(&key)).unwrap();
        let pkg = ext::open_package(ext::read_zip(&bytes).unwrap(), "1.0.0").unwrap();
        assert_eq!(pkg.trust, Trust::Signed);
        assert!(signing_key_from_pem("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----").is_err());
    }

    #[test]
    fn tokens_compare_whole() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
        assert!(!same("", "a"));
    }

    #[test]
    fn agents_only_turn_extensions_on_in_developer_mode() {
        assert!(agent_may_set_enabled(false, false).is_ok());
        assert!(agent_may_set_enabled(true, true).is_ok());
        assert_eq!(
            agent_may_set_enabled(true, false).unwrap_err(),
            "Turning an extension on from an agent needs developer mode. Ask the person to do it in MXB App."
        );
    }
}
