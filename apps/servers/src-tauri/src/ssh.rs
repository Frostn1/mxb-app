//! Reaching a server's loopback-only ports: the system `ssh` forwards each one to a free local
//! port (`ssh -N -L`), and runs the few read-only remote commands (`tail` of the log).
//!
//! This is the management plane's own design (D2: the admin and observe listeners never face
//! the internet; remote access is an SSH tunnel), done by the app so nobody has to keep a
//! terminal open. `BatchMode` means a missing key or an unknown passphrase fails fast instead
//! of prompting inside a hidden process.

use crate::store::{safe_remote_path, Server};
use std::collections::HashMap;
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
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
        args.extend(["-i".into(), key.clone(), "-o".into(), "IdentitiesOnly=yes".into()]);
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

/// The arguments of a remote `tail`; `None` when the path or count is not safe to send.
pub fn tail_args(server: &Server, lines: u32) -> Option<Vec<String>> {
    if !safe_remote_path(&server.log_path) || !(1..=5000).contains(&lines) {
        return None;
    }
    let mut args = base_args(server);
    args.push("--".into());
    args.push(destination(server));
    args.push(format!("tail -n {lines} -- {}", server.log_path));
    Some(args)
}

struct Tunnel {
    child: Child,
    local: u16,
    /// The server fields the tunnel was opened with; an edit reopens it.
    opened_for: Server,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Open tunnels, keyed by server id and remote port.
#[derive(Default)]
pub struct Tunnels {
    open: Mutex<HashMap<(String, u16), Tunnel>>,
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

fn stderr_of(child: &mut Child) -> String {
    let mut text = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut text);
    }
    text.trim().to_string()
}

impl Tunnels {
    /// The local port forwarding to `remote` on `server`, opening a tunnel when there is no
    /// live one. Blocking: call from a blocking task.
    pub fn port(&self, server: &Server, remote: u16) -> Result<u16, String> {
        let key = (server.id.clone(), remote);
        {
            let mut open = self.open.lock().map_err(|_| "tunnel lock poisoned")?;
            if let Some(tunnel) = open.get_mut(&key) {
                let alive = matches!(tunnel.child.try_wait(), Ok(None));
                if alive && tunnel.opened_for == *server {
                    return Ok(tunnel.local);
                }
                open.remove(&key);
            }
        }
        let local = free_port()?;
        let mut child = command()
            .args(tunnel_args(server, local, remote))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run ssh ({e}); is OpenSSH installed?"))?;
        let addr = SocketAddr::from(([127, 0, 0, 1], local));
        let deadline = Instant::now() + Duration::from_secs(CONNECT_SECS);
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let why = stderr_of(&mut child);
                return Err(if why.is_empty() {
                    format!("ssh exited ({status})")
                } else {
                    why
                });
            }
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("ssh tunnel to {} timed out", server.host));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        // Nothing reads stderr from here on; a chatty ssh must not block on a full pipe.
        drop(child.stderr.take());
        let mut open = self.open.lock().map_err(|_| "tunnel lock poisoned")?;
        open.insert(
            key,
            Tunnel {
                child,
                local,
                opened_for: server.clone(),
            },
        );
        Ok(local)
    }

    /// Forget (and kill) a tunnel whose requests fail, so the next call reopens it.
    pub fn reset(&self, id: &str, remote: u16) {
        if let Ok(mut open) = self.open.lock() {
            open.remove(&(id.to_string(), remote));
        }
    }

    pub fn close_server(&self, id: &str) {
        if let Ok(mut open) = self.open.lock() {
            open.retain(|(server, _), _| server != id);
        }
    }

    pub fn close_all(&self) {
        if let Ok(mut open) = self.open.lock() {
            open.clear();
        }
    }
}

/// The last `lines` lines of the server's log. Blocking.
pub fn tail(server: &Server, lines: u32) -> Result<Vec<String>, String> {
    let args = tail_args(server, lines).ok_or("unsafe log path or line count")?;
    let output = command()
        .args(args)
        .output()
        .map_err(|e| format!("could not run ssh ({e}); is OpenSSH installed?"))?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if why.is_empty() {
            format!("ssh exited ({})", output.status)
        } else {
            why
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            id: "a".into(),
            name: "x".into(),
            host: "16.146.6.22".into(),
            ssh_port: 22,
            user: "ubuntu".into(),
            key_path: Some("C:/keys/k.pem".into()),
            observe_port: 9809,
            admin_port: None,
            log_path: "/opt/mxbserver/logs/mxbserver.log".into(),
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
    fn tail_sends_only_a_checked_path() {
        let args = tail_args(&server(), 200).unwrap();
        assert_eq!(
            args.last().unwrap(),
            "tail -n 200 -- /opt/mxbserver/logs/mxbserver.log"
        );
        let bad = Server { log_path: "/x;rm -rf /".into(), ..server() };
        assert!(tail_args(&bad, 200).is_none());
        assert!(tail_args(&server(), 0).is_none());
        assert!(tail_args(&server(), 5001).is_none());
    }
}
