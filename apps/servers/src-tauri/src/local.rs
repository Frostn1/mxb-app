//! Config editing for a server on this PC: the same steps `remote.sh` takes on a Linux host
//! (check the candidate with the server's own binary, back up, replace, restart, wait for
//! `/readyz`, put the backup back if it never comes), done directly.

use crate::config;
use crate::store::{LocalCommand, Server};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Applied {
    pub result: &'static str,
    pub backup: String,
    pub output: String,
}

fn command(server: &Server) -> Result<&LocalCommand, String> {
    server.local_command.as_ref().ok_or_else(|| {
        "This server has no start command saved, so the app can't check or restart it. \
         Start it with server-manager-local.ps1, which records one."
            .to_string()
    })
}

/// The `--config` path in the start command, made absolute against its directory.
pub fn config_path(cmd: &LocalCommand) -> Result<PathBuf, String> {
    let at = cmd
        .args
        .iter()
        .position(|a| a == "--config")
        .ok_or("the start command has no --config")?;
    let path = PathBuf::from(cmd.args.get(at + 1).ok_or("--config has no value")?);
    Ok(if path.is_absolute() {
        path
    } else {
        Path::new(&cmd.cwd).join(path)
    })
}

pub fn read(server: &Server) -> Result<(String, PathBuf), String> {
    let path = config_path(command(server)?)?;
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((text, path))
}

fn has_admin(text: &str) -> bool {
    text.parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|d| d.get("admin").and_then(|a| a.get("listen")).map(|_| ()))
        .is_some()
}

/// Run the server once for a second on side ports with `candidate` as its config.
fn check(cmd: &LocalCommand, candidate: &Path, text: &str) -> Result<(bool, String), String> {
    // Distinct free ports: the server refuses observe and admin on one port, and
    // `127.0.0.1:0` twice counts as the same.
    let admin_port = format!("127.0.0.1:{}", free_port()?);
    let mut admin = false;
    let mut args = Vec::new();
    let mut it = cmd.args.iter();
    while let Some(a) = it.next() {
        if a == "--config" {
            it.next();
            args.push("--config".to_string());
            args.push(candidate.display().to_string());
        } else if a == "--admin" {
            // An admin listener from the command line moves off the live port too.
            it.next();
            args.push("--admin".to_string());
            args.push(admin_port.clone());
            admin = true;
        } else {
            args.push(a.clone());
        }
    }
    args.extend(["--listen".to_string(), "127.0.0.1:0".to_string()]);
    args.extend([
        "--observe".to_string(),
        format!("127.0.0.1:{}", free_port()?),
    ]);
    if !admin && has_admin(text) {
        args.extend(["--admin".to_string(), admin_port]);
    }
    args.extend(["--duration".to_string(), "1".to_string()]);
    let mut child = hidden(Command::new(&cmd.exe))
        .args(&args)
        .current_dir(&cmd.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", cmd.exe))?;
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let ok = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status.success();
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break false;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let text = format!(
        "{}{}",
        err.join().unwrap_or_default(),
        out.join().unwrap_or_default()
    );
    Ok((ok, head_and_tail(&text)))
}

/// A refusal is usually the first line, a crash the last: keep both ends of long output.
pub fn head_and_tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 40 {
        return lines.join("\n");
    }
    format!(
        "{}\n...\n{}",
        lines[..6].join("\n"),
        lines[lines.len() - 30..].join("\n")
    )
}

fn free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

fn candidate_path(config: &Path) -> PathBuf {
    config.with_file_name(format!(".candidate-{}.toml", crate::store::new_id()))
}

pub fn validate(server: &Server, text: &str) -> Result<(bool, String), String> {
    let cmd = command(server)?;
    let config = config_path(cmd)?;
    let candidate = candidate_path(&config);
    fs::write(&candidate, text).map_err(|e| format!("{}: {e}", candidate.display()))?;
    let result = check(cmd, &candidate, text);
    let _ = fs::remove_file(&candidate);
    result
}

fn ready(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    if stream
        .write_all(b"GET /readyz HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf).unwrap_or(0);
    buf[..n].starts_with(b"HTTP/1.1 200") || buf[..n].starts_with(b"HTTP/1.0 200")
}

fn wait_ready(port: u16, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if ready(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn hidden(mut cmd: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// The process listening on 127.0.0.1:`port`, from `netstat -ano`.
#[cfg(windows)]
fn listener_pid(port: u16) -> Option<u32> {
    let out = hidden(Command::new("netstat"))
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()?;
    let want = format!("127.0.0.1:{port}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            (cols.len() == 5 && cols[1] == want && cols[3] == "LISTENING")
                .then(|| cols[4].parse().ok())
                .flatten()
        })
}

#[cfg(not(windows))]
fn listener_pid(_port: u16) -> Option<u32> {
    None
}

/// Stop whatever serves the observe port, start the saved command, and wait for `/readyz`.
fn restart(server: &Server, cmd: &LocalCommand) -> Result<(), String> {
    if cfg!(not(windows)) {
        return Err("restarting a server on this PC is Windows-only for now".into());
    }
    if let Some(pid) = listener_pid(server.observe_port) {
        let _ = hidden(Command::new("taskkill"))
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
        let deadline = Instant::now() + Duration::from_secs(10);
        while listener_pid(server.observe_port).is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        // Starting beside a survivor would fail on its ports while the survivor's /readyz
        // made it look fine.
        if let Some(still) = listener_pid(server.observe_port) {
            return Err(format!("the old server (pid {still}) did not stop"));
        }
    }
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&server.log_path)
        .map_err(|e| format!("{}: {e}", server.log_path))?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut start = hidden(Command::new(&cmd.exe));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Its own process group, not a child the app's exit takes down.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        start.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let child = start
        .args(&cmd.args)
        .current_dir(&cmd.cwd)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("{}: {e}", cmd.exe))?;
    if !wait_ready(server.observe_port, 30) {
        return Err("not ready within 30 s".into());
    }
    // Ready, and it is the process just started that answers.
    match listener_pid(server.observe_port) {
        Some(pid) if pid == child.id() => Ok(()),
        other => Err(format!(
            "the observe port is served by pid {other:?}, not the new server ({})",
            child.id()
        )),
    }
}

/// Check `text` with the server's own binary, back up the live config and replace it. Nothing
/// is restarted. Returns the config, the backup and the check's output.
fn replace(
    server: &Server,
    cmd: &LocalCommand,
    base_sha: &str,
    text: &str,
) -> Result<(PathBuf, PathBuf, String), String> {
    let config = config_path(cmd)?;
    let unchanged = || -> Result<bool, String> {
        let now = fs::read_to_string(&config).map_err(|e| format!("{}: {e}", config.display()))?;
        Ok(config::sha256(&now) == base_sha)
    };
    if !unchanged()? {
        return Err("The config changed since it was loaded; reload and try again.".into());
    }
    let (ok, output) = validate(server, text)?;
    if !ok {
        return Err(format!("The server's own check refused it:\n{output}"));
    }
    // Again, after the check: nobody edited it meanwhile.
    if !unchanged()? {
        return Err("The config changed during the check; reload and try again.".into());
    }
    let dir = config.parent().ok_or("config has no directory")?;
    let backups = dir.join("backups");
    fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
    let name = config
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "server.toml".into());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup = backups.join(format!("{name}.{stamp}"));
    fs::copy(&config, &backup).map_err(|e| format!("backup failed, nothing changed: {e}"))?;
    let tmp = candidate_path(&config);
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &config)
        .map_err(|e| format!("replace failed, the old config is still live: {e}"))?;
    Ok((config, backup, output))
}

/// Apply without a restart: check, back up and replace only. The running server then reloads
/// the file itself (admin API). Returns the backup's path and the check's output.
pub fn write(server: &Server, base_sha: &str, text: &str) -> Result<(String, String), String> {
    let _one = APPLYING
        .try_lock()
        .map_err(|_| "Another config change is being applied.".to_string())?;
    let cmd = command(server)?;
    let (_, backup, output) = replace(server, cmd, base_sha, text)?;
    Ok((backup.display().to_string(), output))
}

/// Put back a backup that [`write`] made (the reload refused the new file).
pub fn restore(server: &Server, backup: &str) -> Result<(), String> {
    let config = config_path(command(server)?)?;
    let backup = PathBuf::from(backup);
    if backup.parent() != config.parent().map(|dir| dir.join("backups")).as_deref() {
        return Err("not a backup of this config".into());
    }
    fs::copy(&backup, &config)
        .map(|_| ())
        .map_err(|e| format!("restore failed: {e}"))
}

/// Restart the server with its saved command, waiting until it is ready.
pub fn restart_now(server: &Server) -> Result<(), String> {
    restart(server, command(server)?)
}

/// One apply at a time from this app.
static APPLYING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn apply(server: &Server, base_sha: &str, text: &str) -> Result<Applied, String> {
    let _one = APPLYING
        .try_lock()
        .map_err(|_| "Another config change is being applied.".to_string())?;
    let cmd = command(server)?;
    let (config, backup, output) = replace(server, cmd, base_sha, text)?;
    let backup_text = backup.display().to_string();
    match restart(server, cmd) {
        Ok(()) => Ok(Applied {
            result: "applied",
            backup: backup_text,
            output,
        }),
        Err(why) => {
            fs::copy(&backup, &config)
                .map_err(|e| format!("{why}; restoring the backup failed too: {e}"))?;
            let result = if restart(server, cmd).is_ok() {
                "rolled-back"
            } else {
                "failed"
            };
            Ok(Applied {
                result,
                backup: backup_text,
                output: format!("{why}\n{output}"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_path_comes_from_the_start_command() {
        let cmd = LocalCommand {
            exe: "mxbserver.exe".into(),
            args: vec![
                "--config".into(),
                "server.toml".into(),
                "--report".into(),
                "10".into(),
            ],
            cwd: std::env::temp_dir().display().to_string(),
        };
        assert_eq!(
            config_path(&cmd).unwrap(),
            std::env::temp_dir().join("server.toml")
        );
        let none = LocalCommand {
            args: vec![],
            ..cmd
        };
        assert!(config_path(&none).is_err());
    }

    /// End to end on a real local server started by server-manager-local.ps1:
    /// `cargo test -p mxb-servers -- --ignored local_apply_end_to_end`.
    #[test]
    #[ignore]
    fn local_apply_end_to_end() {
        let list = std::env::var("APPDATA").unwrap() + "\\com.frost.mxbservers\\servers.json";
        let servers: Vec<Server> =
            serde_json::from_str(&fs::read_to_string(list).unwrap()).unwrap();
        let server = servers
            .into_iter()
            .find(|s| s.local)
            .expect("a local server");
        let (text, _) = read(&server).unwrap();
        let sha = config::sha256(&text);
        let mut changes = serde_json::Map::new();
        changes.insert("events.collisions".into(), serde_json::json!(true));
        let new = config::apply(&text, &changes).unwrap();

        // A config the server refuses never reaches the file.
        let broken = format!("{new}\n[native]\nbot_status = \"not a number\"\n");
        assert!(apply(&server, &sha, &broken).is_err());
        assert_eq!(
            fs::read_to_string(config_path(command(&server).unwrap()).unwrap()).unwrap(),
            text
        );

        let done = apply(&server, &sha, &new).unwrap();
        assert_eq!(done.result, "applied", "{}", done.output);
        assert!(ready(server.observe_port));
        let (live, _) = read(&server).unwrap();
        assert!(live.contains("collisions = true"));
        assert_eq!(fs::read_to_string(&done.backup).unwrap(), text);

        // A stale hash is refused; then put the original back the same way.
        assert!(apply(&server, &sha, &text).is_err());
        let back = apply(&server, &config::sha256(&live), &text).unwrap();
        assert_eq!(back.result, "applied");
    }

    #[test]
    fn admin_is_detected_from_the_file() {
        assert!(has_admin("[admin]\nlisten = \"127.0.0.1:9810\"\n"));
        assert!(!has_admin("[server]\nlisten = \"0.0.0.0:1\"\n"));
    }
}
