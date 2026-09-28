//! The server list (`servers.json` in the app data directory) and each server's admin token
//! (the OS keychain). The file never holds a credential.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const KEYCHAIN_SERVICE: &str = "com.frost.mxbservers";

/// Where a server is and how to reach it: over SSH, forwarding its loopback-only ports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub user: String,
    /// Private key file; `None` lets ssh use its agent and defaults.
    #[serde(default)]
    pub key_path: Option<String>,
    /// The observe listener (`/readyz`, `/status`), unauthenticated, loopback on the server.
    #[serde(default = "default_observe_port")]
    pub observe_port: u16,
    /// The admin listener (`/v1/...`, bearer token), when the server runs one.
    #[serde(default)]
    pub admin_port: Option<u16>,
    #[serde(default = "default_log_path")]
    pub log_path: String,
}

fn default_ssh_port() -> u16 {
    22
}
fn default_observe_port() -> u16 {
    9809
}
fn default_log_path() -> String {
    "/opt/mxbserver/logs/mxbserver.log".to_string()
}

/// Every field is checked before it reaches an `ssh` argument list, so nothing typed here can
/// become an option (a leading `-`) or, for the remote command, shell syntax.
pub fn validate(server: &Server) -> Result<(), String> {
    let name = server.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err("Name must be 1 to 64 characters.".into());
    }
    let host_ok = !server.host.is_empty()
        && server.host.len() <= 253
        && !server.host.starts_with('-')
        && server
            .host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if !host_ok {
        return Err("Host must be a hostname or an IP address.".into());
    }
    let user_ok = !server.user.is_empty()
        && server.user.len() <= 32
        && server.user.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && server
            .user
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));
    if !user_ok {
        return Err("User must be a Unix user name, like ubuntu.".into());
    }
    if let Some(key) = &server.key_path {
        if key.starts_with('-') || !Path::new(key).is_file() {
            return Err(format!("Key file not found: {key}"));
        }
    }
    if server.ssh_port == 0 || server.observe_port == 0 || server.admin_port == Some(0) {
        return Err("Ports must be 1 to 65535.".into());
    }
    if !safe_remote_path(&server.log_path) {
        return Err("Log path must be an absolute path of letters, digits, / . _ -".into());
    }
    Ok(())
}

/// Absolute, and only characters that need no quoting in a remote shell command.
pub fn safe_remote_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 255
        && !path.contains("..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
}

pub fn new_id() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join("servers.json"),
        }
    }

    pub fn load(&self) -> Vec<Server> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_all(&self, servers: &[Server]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(servers).map_err(|e| e.to_string())?;
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Insert or replace by id.
    pub fn upsert(&self, server: Server) -> Result<(), String> {
        let mut all = self.load();
        match all.iter_mut().find(|s| s.id == server.id) {
            Some(existing) => *existing = server,
            None => all.push(server),
        }
        self.save_all(&all)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut all = self.load();
        all.retain(|s| s.id != id);
        self.save_all(&all)
    }

    pub fn get(&self, id: &str) -> Result<Server, String> {
        self.load()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| "No such server.".to_string())
    }
}

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|e| format!("keychain: {e}"))
}

pub fn token(id: &str) -> Option<String> {
    entry(id).ok()?.get_password().ok()
}

pub fn set_token(id: &str, token: &str) -> Result<(), String> {
    entry(id)?
        .set_password(token)
        .map_err(|e| format!("keychain: {e}"))
}

pub fn clear_token(id: &str) {
    if let Ok(entry) = entry(id) {
        let _ = entry.delete_credential();
    }
}

/// An admin token as `mxbserver admin token new` prints it: `<id>.<secret>`.
pub fn valid_token(token: &str) -> bool {
    let Some((id, secret)) = token.split_once('.') else {
        return false;
    };
    !id.is_empty()
        && id.len() <= 32
        && secret.len() >= 16
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            id: "a".into(),
            name: "Lightsail".into(),
            host: "16.146.6.22".into(),
            ssh_port: 22,
            user: "ubuntu".into(),
            key_path: None,
            observe_port: 9809,
            admin_port: Some(9810),
            log_path: default_log_path(),
        }
    }

    #[test]
    fn a_normal_server_is_valid() {
        assert_eq!(validate(&server()), Ok(()));
    }

    #[test]
    fn nothing_can_become_an_ssh_option_or_shell_syntax() {
        for host in ["-oProxyCommand=x", "a b", "a;b", "a$(x)", ""] {
            let s = Server { host: host.into(), ..server() };
            assert!(validate(&s).is_err(), "{host}");
        }
        for user in ["-l", "Root", "a b", "a;b", ""] {
            let s = Server { user: user.into(), ..server() };
            assert!(validate(&s).is_err(), "{user}");
        }
        for path in ["logs/x", "/a b", "/a;rm", "/a/../etc/shadow", "/$(x)", "/a'b"] {
            assert!(!safe_remote_path(path), "{path}");
        }
        let s = Server { key_path: Some("-oProxyCommand=x".into()), ..server() };
        assert!(validate(&s).is_err());
    }

    #[test]
    fn tokens_look_like_the_server_prints_them() {
        assert!(valid_token("sean.0123456789abcdef0123"));
        assert!(!valid_token("nodot"));
        assert!(!valid_token(".secretsecretsecret"));
        assert!(!valid_token("sean.short"));
        assert!(!valid_token("sean.0123456789abcdef\nx"));
    }

    #[test]
    fn the_store_round_trips_without_tokens() {
        let dir = std::env::temp_dir().join(format!("mxb-servers-{}", new_id()));
        let store = Store::new(dir.clone());
        store.upsert(server()).unwrap();
        store.upsert(Server { name: "Renamed".into(), ..server() }).unwrap();
        assert_eq!(store.load().len(), 1);
        assert_eq!(store.get("a").unwrap().name, "Renamed");
        let text = fs::read_to_string(dir.join("servers.json")).unwrap();
        assert!(!text.to_lowercase().contains("token"));
        store.remove("a").unwrap();
        assert!(store.load().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
