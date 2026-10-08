//! The server list (`servers.json` in the app data directory) and each server's admin token
//! (the OS keychain). The file never holds a credential.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const KEYCHAIN_SERVICE: &str = "com.frost.mxbservers";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerKind {
    Native,
    Legacy,
    /// A server hosted by mxbsecure: driven through the control plane's `/v1/hosted/*` routes
    /// with a token claimed from servers.mxbsecure.com. No SSH and no agent.
    Hosted,
}

fn default_server_kind() -> ServerKind {
    // Existing saved servers predate this field and are all native mxbserver hosts.
    ServerKind::Native
}

/// Where a server is and how to reach it: over SSH, forwarding its loopback-only ports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    #[serde(default = "default_server_kind")]
    pub kind: ServerKind,
    pub host: String,
    /// Legacy mxb-agent uses HTTPS when it sits behind a TLS reverse proxy.
    #[serde(default)]
    pub agent_tls: bool,
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
    /// A server on this PC: its ports are used directly on 127.0.0.1 and the log is a local
    /// file, with no SSH. Host, user and key are ignored.
    #[serde(default)]
    pub local: bool,
    /// For a server on this PC: how it is started, so the app can check a config with the same
    /// binary and restart it after a change. Written by `server-manager-local.ps1`.
    #[serde(default)]
    pub local_command: Option<LocalCommand>,
    /// For a hosted server: its id in the control plane.
    #[serde(default)]
    pub hosted_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalCommand {
    pub exe: String,
    pub args: Vec<String>,
    pub cwd: String,
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
    if server.kind == ServerKind::Hosted {
        return match &server.hosted_id {
            Some(id) if valid_hosted_id(id) => Ok(()),
            _ => Err("This hosted server has no valid id. Add it again from servers.mxbsecure.com.".into()),
        };
    }
    if server.observe_port == 0 || server.admin_port == Some(0) {
        return Err("Ports must be 1 to 65535.".into());
    }
    if server.local && server.kind == ServerKind::Native {
        if !Path::new(&server.log_path).is_absolute() {
            return Err(
                r"Log file must be a full path, like C:\mxbserver\logs\mxbserver.log".into(),
            );
        }
        return Ok(());
    }
    let host_ok = server.local
        || (!server.host.is_empty()
            && server.host.len() <= 253
            && !server.host.starts_with('-')
            && server
                .host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':')));
    if !host_ok {
        return Err("Host must be a hostname or an IP address.".into());
    }
    let user_ok = server.kind == ServerKind::Legacy
        || (!server.user.is_empty()
            && server.user.len() <= 32
            && server
                .user
                .starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && server
                .user
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-')));
    if !user_ok {
        return Err("User must be a Unix user name, like ubuntu.".into());
    }
    if server.kind == ServerKind::Native {
        if let Some(key) = &server.key_path {
            if key.starts_with('-') || !Path::new(key).is_file() {
                return Err(format!("Key file not found: {key}"));
            }
        }
    }
    if server.ssh_port == 0 || server.observe_port == 0 || server.admin_port == Some(0) {
        return Err("Ports must be 1 to 65535.".into());
    }
    if server.kind == ServerKind::Native && !safe_remote_path(&server.log_path) {
        return Err("Log path must be an absolute path of letters, digits, / . _ -".into());
    }
    Ok(())
}

/// A control-plane server id: a UUID, which is all that ever goes into a hosted route's path.
pub fn valid_hosted_id(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// A one-time claim code from servers.mxbsecure.com, as the control plane mints it.
pub fn valid_claim_code(code: &str) -> bool {
    (16..=128).contains(&code.len()) && code.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
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

    /// A server this app reaches itself (SSH, this PC or an agent). Hosted servers are only
    /// driven through the control plane, so every direct command refuses them here.
    pub fn managed(&self, id: &str) -> Result<Server, String> {
        let server = self.get(id)?;
        if server.kind == ServerKind::Hosted {
            return Err("Hosted servers are managed through mxbsecure.".into());
        }
        Ok(server)
    }

    /// The saved hosted server already linked to this control-plane id, if any.
    pub fn by_hosted_id(&self, hosted_id: &str) -> Option<Server> {
        self.load()
            .into_iter()
            .find(|s| s.kind == ServerKind::Hosted && s.hosted_id.as_deref() == Some(hosted_id))
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

pub fn valid_agent_token(token: &str) -> bool {
    let token = token.trim();
    !token.is_empty() && token.len() <= 512 && token.chars().all(|c| c.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            id: "a".into(),
            name: "Lightsail".into(),
            kind: ServerKind::Native,
            host: "16.146.6.22".into(),
            agent_tls: false,
            ssh_port: 22,
            user: "ubuntu".into(),
            key_path: None,
            observe_port: 9809,
            admin_port: Some(9810),
            log_path: default_log_path(),
            local: false,
            local_command: None,
            hosted_id: None,
        }
    }

    #[test]
    fn a_hosted_server_needs_only_a_name_and_a_uuid() {
        let hosted = Server {
            kind: ServerKind::Hosted,
            host: String::new(),
            user: String::new(),
            admin_port: None,
            hosted_id: Some("0f8fad5b-d9cb-469f-a165-70867728950e".into()),
            ..server()
        };
        assert_eq!(validate(&hosted), Ok(()));
        for bad in [None, Some("../v1/web/admin"), Some("0f8fad5b-d9cb-469f-a165-70867728950"), Some("0f8fad5b/d9cb-469f-a165-70867728950e")] {
            let s = Server { hosted_id: bad.map(String::from), ..hosted.clone() };
            assert!(validate(&s).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn claim_codes_are_plain_tokens() {
        assert!(valid_claim_code("abcdefghijklmnop_-0123"));
        assert!(!valid_claim_code("short"));
        assert!(!valid_claim_code("abcdefghijklmnop&x=1"));
        assert!(!valid_claim_code(&"a".repeat(129)));
    }

    #[test]
    fn direct_commands_refuse_hosted_servers() {
        let dir = std::env::temp_dir().join(format!("mxb-servers-{}", new_id()));
        let store = Store::new(dir.clone());
        store.upsert(server()).unwrap();
        let hosted = Server {
            id: "h".into(),
            kind: ServerKind::Hosted,
            hosted_id: Some("0f8fad5b-d9cb-469f-a165-70867728950e".into()),
            ..server()
        };
        store.upsert(hosted).unwrap();
        assert!(store.managed("a").is_ok());
        assert!(store.managed("h").is_err());
        assert_eq!(store.by_hosted_id("0f8fad5b-d9cb-469f-a165-70867728950e").unwrap().id, "h");
        assert!(store.by_hosted_id("x").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_normal_server_is_valid() {
        assert_eq!(validate(&server()), Ok(()));
    }

    #[test]
    fn nothing_can_become_an_ssh_option_or_shell_syntax() {
        for host in ["-oProxyCommand=x", "a b", "a;b", "a$(x)", ""] {
            let s = Server {
                host: host.into(),
                ..server()
            };
            assert!(validate(&s).is_err(), "{host}");
        }
        for user in ["-l", "Root", "a b", "a;b", ""] {
            let s = Server {
                user: user.into(),
                ..server()
            };
            assert!(validate(&s).is_err(), "{user}");
        }
        for path in [
            "logs/x",
            "/a b",
            "/a;rm",
            "/a/../etc/shadow",
            "/$(x)",
            "/a'b",
        ] {
            assert!(!safe_remote_path(path), "{path}");
        }
        let s = Server {
            key_path: Some("-oProxyCommand=x".into()),
            ..server()
        };
        assert!(validate(&s).is_err());
    }

    #[test]
    fn a_local_server_needs_no_ssh_fields_but_a_full_log_path() {
        let local = Server {
            local: true,
            host: String::new(),
            user: String::new(),
            log_path: std::env::temp_dir().join("x.log").display().to_string(),
            ..server()
        };
        assert_eq!(validate(&local), Ok(()));
        let relative = Server {
            log_path: "logs/x.log".into(),
            ..local
        };
        assert!(validate(&relative).is_err());
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
    fn legacy_agent_tokens_are_not_forced_into_admin_token_shape() {
        assert!(valid_agent_token("0123456789abcdef0123456789abcdef"));
        assert!(valid_agent_token("short-but-valid"));
        assert!(!valid_agent_token(""));
        assert!(!valid_agent_token("0123456789abcdef\nheader"));
    }

    #[test]
    fn the_store_round_trips_without_tokens() {
        let dir = std::env::temp_dir().join(format!("mxb-servers-{}", new_id()));
        let store = Store::new(dir.clone());
        store.upsert(server()).unwrap();
        store
            .upsert(Server {
                name: "Renamed".into(),
                ..server()
            })
            .unwrap();
        assert_eq!(store.load().len(), 1);
        assert_eq!(store.get("a").unwrap().name, "Renamed");
        let text = fs::read_to_string(dir.join("servers.json")).unwrap();
        assert!(!text.to_lowercase().contains("token"));
        store.remove("a").unwrap();
        assert!(store.load().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
