//! Reaching a server's loopback-only ports: the system `ssh` forwards each one to a free local
//! port (`ssh -N -L`), and runs the host-side helper script (`remote.sh`) for config, systemctl and journalctl.
//!
//! This is the management plane's own design (D2: the admin and observe listeners never face
//! the internet; remote access is an SSH tunnel), done by the app so nobody has to keep a
//! terminal open. `BatchMode` means a missing key or an unknown passphrase fails fast instead
//! of prompting inside a hidden process.

use crate::store::Server;
use std::collections::HashMap;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CONNECT_SECS: u64 = 12;

fn base_args(server: &Server) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        server.ssh_port.to_string(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-o".into(),
        "ServerAliveInterval=15".into(),
        "-o".into(),
        "ServerAliveCountMax=2".into(),
        // First contact records the host key; a changed key afterwards is refused.
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
    ];
    if let Some(key) = &server.key_path {
        args.extend([
            "-i".into(),
            key.clone(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
        ]);
    }
    args
}

fn destination(server: &Server) -> String {
    format!("{}@{}", server.user, server.host)
}

fn command() -> Command {
    let mut cmd = Command::new("ssh");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
    cmd
}

fn scp_command() -> Command {
    let mut cmd = Command::new("scp");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
    cmd
}

fn scp_args(server: &Server, local: &str, remote_name: &str) -> Option<Vec<String>> {
    if !plain_word(remote_name) || remote_name.contains('.') {
        return None;
    }
    let mut args = vec![
        "-P".into(),
        server.ssh_port.to_string(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
    ];
    if let Some(key) = &server.key_path {
        args.extend([
            "-i".into(),
            key.clone(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
        ]);
    }
    args.extend([
        "--".into(),
        local.into(),
        format!("{}:/tmp/{remote_name}", destination(server)),
    ]);
    Some(args)
}

/// The arguments of a tunnel from `local` to the server's loopback `remote` port.
pub fn tunnel_args(server: &Server, local: u16, remote: u16) -> Vec<String> {
    let mut args = vec![
        "-N".to_string(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-L".into(),
        format!("127.0.0.1:{local}:127.0.0.1:{remote}"),
    ];
    args.extend(base_args(server));
    args.push("--".into());
    args.push(destination(server));
    args
}

/// Every ssh process this app starts, so quitting kills all of them: tunnels, tunnels still
/// connecting, and log reads.
#[derive(Default)]
struct Registry {
    children: Mutex<HashMap<u64, Arc<Mutex<Child>>>>,
    next: AtomicU64,
    closing: AtomicBool,
}

impl Registry {
    fn spawn(&self, mut cmd: Command) -> Result<(u64, Arc<Mutex<Child>>), String> {
        if self.closing.load(Ordering::SeqCst) {
            return Err("closing".into());
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("could not run ssh ({e}); is OpenSSH installed?"))?;
        let child = Arc::new(Mutex::new(child));
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut all) = self.children.lock() {
            all.insert(id, Arc::clone(&child));
        }
        Ok((id, child))
    }

    /// Kill and reap one process, and forget it.
    fn kill(&self, id: u64) {
        let child = self
            .children
            .lock()
            .ok()
            .and_then(|mut all| all.remove(&id));
        if let Some(child) = child {
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn kill_all(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let ids: Vec<u64> = self
            .children
            .lock()
            .map(|all| all.keys().copied().collect())
            .unwrap_or_default();
        for id in ids {
            self.kill(id);
        }
    }
}

/// `Some(reason)` once the process has exited.
fn exited(child: &Mutex<Child>) -> Option<String> {
    let mut child = child.lock().ok()?;
    let status = child.try_wait().ok()??;
    let mut why = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut why);
    }
    let why = why.trim().to_string();
    Some(if why.is_empty() {
        format!("ssh exited ({status})")
    } else {
        why
    })
}

struct Tunnel {
    id: u64,
    child: Arc<Mutex<Child>>,
    registry: Arc<Registry>,
    local: u16,
    /// The server fields the tunnel was opened with; an edit reopens it.
    opened_for: Server,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.registry.kill(self.id);
    }
}

/// Why `upload_with_progress` stopped.
#[derive(Debug)]
pub enum UploadError {
    Cancelled,
    Failed(String),
}

type Key = (String, u16);
type Slot = Arc<Mutex<Option<Tunnel>>>;

/// Open tunnels, keyed by server id and remote port. Each key has its own lock, held while a
/// tunnel connects, so two screens asking at once share one tunnel instead of racing.
#[derive(Default)]
pub struct Tunnels {
    slots: Mutex<HashMap<Key, Slot>>,
    registry: Arc<Registry>,
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

impl Tunnels {
    /// Copy one local file to a random, checked name in the server's /tmp directory, reporting
    /// the bytes sent so far. The file goes through `ssh ... cat` (scp prints no progress when
    /// it is not on a terminal). `cancel` kills the transfer; the partial remote file is left
    /// for the next `/tmp` clean-up.
    pub fn upload_with_progress(
        &self,
        server: &Server,
        local: &str,
        remote_name: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<(), UploadError> {
        use std::io::Write;
        if !plain_word(remote_name) || remote_name.contains('.') || remote_name.contains('/') {
            return Err(UploadError::Failed("unsafe upload destination".into()));
        }
        let mut file = std::fs::File::open(local)
            .map_err(|e| UploadError::Failed(format!("could not read {local}: {e}")))?;
        let mut cmd = command();
        cmd.args(base_args(server))
            .arg("--")
            .arg(destination(server))
            .arg(format!("umask 077 && cat > /tmp/{remote_name}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let (id, child) = self
            .registry
            .spawn(cmd)
            .map_err(UploadError::Failed)?;
        let (mut stdin, stderr) = {
            let mut child = child.lock().map_err(|_| UploadError::Failed("lock poisoned".into()))?;
            (child.stdin.take(), child.stderr.take())
        };
        // ssh's own messages, collected off to the side so a full pipe can never stall it.
        let why = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut err) = stderr {
                let _ = err.read_to_string(&mut text);
            }
            text
        });
        let mut sent = 0u64;
        let mut buffer = vec![0u8; 64 * 1024];
        let mut write_error = None;
        loop {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let read = match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    write_error = Some(format!("could not read {local}: {e}"));
                    break;
                }
            };
            let Some(pipe) = stdin.as_mut() else { break };
            if let Err(e) = pipe.write_all(&buffer[..read]) {
                write_error = Some(e.to_string());
                break;
            }
            sent += read as u64;
            progress(sent);
        }
        // Closing stdin ends `cat`; a cancelled or failed transfer kills ssh instead.
        drop(stdin);
        if cancel.load(Ordering::SeqCst) {
            self.registry.kill(id);
            let _ = why.join();
            return Err(UploadError::Cancelled);
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            let status = child.lock().ok().and_then(|mut c| c.try_wait().ok().flatten());
            if let Some(status) = status {
                break Some(status);
            }
            if cancel.load(Ordering::SeqCst) || Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        self.registry.kill(id);
        let why = why.join().unwrap_or_default();
        if cancel.load(Ordering::SeqCst) {
            return Err(UploadError::Cancelled);
        }
        let why = why.trim().to_string();
        match status {
            Some(status) if status.success() && write_error.is_none() => Ok(()),
            Some(status) => Err(UploadError::Failed(if !why.is_empty() {
                why
            } else if let Some(e) = write_error {
                e
            } else {
                format!("ssh exited ({status})")
            })),
            None => Err(UploadError::Failed(if why.is_empty() {
                format!("upload to {} did not finish", server.host)
            } else {
                why
            })),
        }
    }

    /// Copy one local file to a random, checked name in the server's /tmp directory.
    pub fn upload(
        &self,
        server: &Server,
        local: &str,
        remote_name: &str,
        secs: u64,
    ) -> Result<(), String> {
        let args = scp_args(server, local, remote_name).ok_or("unsafe upload destination")?;
        let mut cmd = scp_command();
        cmd.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
        let (id, child) = self.registry.spawn(cmd)?;
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            let status = child
                .lock()
                .ok()
                .and_then(|mut c| c.try_wait().ok().flatten());
            if let Some(status) = status {
                let mut why = String::new();
                if let Ok(mut child) = child.lock() {
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_string(&mut why);
                    }
                }
                self.registry.kill(id);
                return if status.success() {
                    Ok(())
                } else if why.trim().is_empty() {
                    Err(format!("scp exited ({status})"))
                } else {
                    Err(why.trim().into())
                };
            }
            if Instant::now() > deadline {
                self.registry.kill(id);
                return Err(format!("upload to {} timed out", server.host));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn slot(&self, key: &Key) -> Result<Slot, String> {
        let mut slots = self.slots.lock().map_err(|_| "tunnel lock poisoned")?;
        Ok(Arc::clone(slots.entry(key.clone()).or_default()))
    }

    /// The local port forwarding to `remote` on `server`, opening a tunnel when there is no
    /// live one. Blocking: call from a blocking task.
    pub fn port(&self, server: &Server, remote: u16) -> Result<u16, String> {
        let slot = self.slot(&(server.id.clone(), remote))?;
        let mut slot = slot.lock().map_err(|_| "tunnel lock poisoned")?;
        if let Some(tunnel) = slot.as_ref() {
            if exited(&tunnel.child).is_none() && tunnel.opened_for == *server {
                return Ok(tunnel.local);
            }
            *slot = None;
        }
        let local = free_port()?;
        let mut cmd = command();
        cmd.args(tunnel_args(server, local, remote))
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let (id, child) = self.registry.spawn(cmd)?;
        // From here, dropping `tunnel` on any early return kills the ssh process.
        let tunnel = Tunnel {
            id,
            child,
            registry: Arc::clone(&self.registry),
            local,
            opened_for: server.clone(),
        };
        let addr = SocketAddr::from(([127, 0, 0, 1], local));
        let deadline = Instant::now() + Duration::from_secs(CONNECT_SECS);
        loop {
            if let Some(why) = exited(&tunnel.child) {
                return Err(why);
            }
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                break;
            }
            if Instant::now() > deadline {
                return Err(format!("ssh tunnel to {} timed out", server.host));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        // Nothing reads stderr from here on; a chatty ssh must not block on a full pipe.
        if let Ok(mut child) = tunnel.child.lock() {
            drop(child.stderr.take());
        }
        *slot = Some(tunnel);
        Ok(local)
    }

    /// Drop every tunnel whose key `keep` rejects.
    fn clear(&self, keep: impl Fn(&Key) -> bool) {
        let gone: Vec<Slot> = match self.slots.lock() {
            Ok(mut slots) => {
                let keys: Vec<Key> = slots.keys().filter(|k| !keep(k)).cloned().collect();
                keys.iter().filter_map(|k| slots.remove(k)).collect()
            }
            Err(_) => return,
        };
        for slot in gone {
            if let Ok(mut slot) = slot.lock() {
                *slot = None;
            }
        }
    }

    /// Forget (and kill) a tunnel whose requests fail, so the next call reopens it.
    pub fn reset(&self, id: &str, remote: u16) {
        self.clear(|(server, port)| !(server == id && *port == remote));
    }

    pub fn close_server(&self, id: &str) {
        self.clear(|(server, _)| server != id);
    }

    /// On quit: kill every ssh this app started, including ones still connecting.
    pub fn close_all(&self) {
        self.registry.kill_all();
        self.clear(|_| false);
    }
}

/// What a remote script printed.
pub struct ScriptOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl ScriptOutput {
    /// The value of the first `@@key value` line.
    pub fn field(&self, key: &str) -> Option<&str> {
        let prefix = format!("@@{key} ");
        self.stdout
            .lines()
            .find_map(|l| l.strip_prefix(&prefix))
            .map(str::trim)
    }

    /// Everything that is not an `@@` line: what a person should see.
    pub fn text(&self) -> String {
        let out: Vec<&str> = self
            .stdout
            .lines()
            .filter(|l| !l.starts_with("@@"))
            .collect();
        let mut text = out.join("\n");
        if !self.stderr.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(self.stderr.trim());
        }
        text
    }
}

/// A word that is safe on a remote command line without quoting.
fn plain_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_' | '.'))
}

impl Tunnels {
    /// Run `script` on the server with bash, fed on stdin, as `bash -s -- <args>`. Every
    /// argument must be a plain word (base64, hex, digits, command names). Blocking.
    pub fn run_script(
        &self,
        server: &Server,
        script: &str,
        args: &[&str],
        secs: u64,
    ) -> Result<ScriptOutput, String> {
        if let Some(bad) = args.iter().find(|a| !plain_word(a)) {
            return Err(format!("refusing to send '{bad}' to the server"));
        }
        let mut ssh_args = base_args(server);
        ssh_args.push("--".into());
        ssh_args.push(destination(server));
        ssh_args.push(format!("bash -s -- {}", args.join(" ")));
        let mut cmd = command();
        cmd.args(ssh_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (id, child) = self.registry.spawn(cmd)?;
        let (stdin, out, err) = match child.lock() {
            Ok(mut c) => (c.stdin.take(), c.stdout.take(), c.stderr.take()),
            Err(_) => (None, None, None),
        };
        // bash on the server reads a CR as part of each word; a CRLF checkout must not reach it.
        let script = script.replace('\r', "");
        let feed = std::thread::spawn(move || {
            use std::io::Write;
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(script.as_bytes());
            }
        });
        let out = std::thread::spawn(move || read_all(out));
        let err = std::thread::spawn(move || read_all(err));
        let deadline = Instant::now() + Duration::from_secs(secs);
        let status = loop {
            let done = child
                .lock()
                .ok()
                .and_then(|mut c| c.try_wait().ok().flatten());
            if done.is_some() || Instant::now() > deadline {
                break done;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        self.registry.kill(id);
        let _ = feed.join();
        let stdout = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
        let stderr = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
        match status {
            None => Err(format!("{} did not answer within {secs} s", server.host)),
            Some(status) => Ok(ScriptOutput {
                success: status.success(),
                stdout,
                stderr,
            }),
        }
    }
}

fn read_all(pipe: Option<impl Read>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut buf);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            id: "a".into(),
            name: "x".into(),
            kind: crate::store::ServerKind::Native,
            host: "16.146.6.22".into(),
            agent_tls: false,
            ssh_port: 22,
            user: "ubuntu".into(),
            key_path: Some("C:/keys/k.pem".into()),
            observe_port: 9809,
            admin_port: None,
            log_path: "/opt/mxbserver/logs/mxbserver.log".into(),
            local: false,
            local_command: None,
            hosted_id: None,
        }
    }

    #[test]
    fn a_tunnel_forwards_loopback_to_loopback_and_ends_options_before_the_host() {
        let args = tunnel_args(&server(), 50123, 9809);
        assert!(args.contains(&"127.0.0.1:50123:127.0.0.1:9809".to_string()));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        let dashdash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dashdash + 1], "ubuntu@16.146.6.22");
        assert_eq!(args.len(), dashdash + 2);
    }

    #[test]
    fn nothing_starts_after_close_all() {
        let tunnels = Tunnels::default();
        tunnels.close_all();
        let err = tunnels.port(&server(), 9809).unwrap_err();
        assert_eq!(err, "closing");
    }
}
