//! The agent's own settings, read from `agent.json` beside the binary.

use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerKind {
    Stock,
    Native,
}

fn default_kind() -> ServerKind {
    ServerKind::Stock
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Bearer token the app must present. No default — an agent that listens without one
    /// would hand process control to anyone who portscans the box.
    pub token: String,
    /// Which server is supervised. Omitted keeps the original stock-Windows behaviour.
    #[serde(default = "default_kind")]
    pub kind: ServerKind,
    /// Where the HTTP API listens.
    #[serde(default = "default_listen")]
    pub listen: String,
    /// The MX Bikes install directory holding `mxbikes.exe` and the server `.ini`.
    pub game_dir: PathBuf,
    /// Machine-wide native track store. Servers on the same host link packages from here.
    /// Omitted keeps it beside the individual server directories.
    #[serde(default)]
    pub track_library: Option<PathBuf>,
    /// Server config filename, relative to `game_dir`.
    #[serde(default = "default_ini")]
    pub ini: String,
    /// Native server config, relative to `game_dir`.
    #[serde(default = "default_native_config")]
    pub native_config: String,
    /// Native server binary, relative to `game_dir`.
    #[serde(default = "default_native_binary")]
    pub native_binary: String,
    /// Loopback admin listener and its control token. Required for native session actions.
    #[serde(default)]
    pub native_admin: Option<String>,
    #[serde(default)]
    pub native_admin_token: Option<String>,
    /// UDP port passed to `-dedicated`.
    #[serde(default = "default_game_port")]
    pub game_port: u16,
    /// Base URL the app should use to reach this agent, when it isn't simply the address
    /// the agent binds. Behind NAT or a reverse proxy the two differ and only the operator
    /// knows the outside one, so this is the override the pairing blob uses.
    #[serde(default)]
    pub public_url: Option<String>,
}

fn default_listen() -> String {
    "0.0.0.0:8787".to_string()
}
fn default_ini() -> String {
    "dedicated.ini".to_string()
}
fn default_native_config() -> String {
    "server.toml".to_string()
}
fn default_native_binary() -> String {
    if cfg!(windows) {
        "mxbserver.exe".to_string()
    } else {
        "mxbserver".to_string()
    }
}
fn default_game_port() -> u16 {
    54210
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("couldn't read {}: {e}", path.display()))?;
        let cfg: Config = serde_json::from_str(&text)
            .map_err(|e| format!("couldn't parse {}: {e}", path.display()))?;
        // A blank token is the same hole as a missing one, and serde can't express that.
        if cfg.token.trim().is_empty() {
            return Err(format!("{}: \"token\" must not be empty", path.display()));
        }
        if cfg.kind == ServerKind::Native {
            let safe_relative = |value: &str| {
                !value.trim().is_empty()
                    && !Path::new(value).is_absolute()
                    && Path::new(value)
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
            };
            if !safe_relative(&cfg.native_config) {
                return Err(format!(
                    "{}: native_config must be a relative file name",
                    path.display()
                ));
            }
            if !safe_relative(&cfg.native_binary) {
                return Err(format!(
                    "{}: native_binary must be a relative file name",
                    path.display()
                ));
            }
            if cfg.native_admin.is_some() != cfg.native_admin_token.is_some() {
                return Err(format!(
                    "{}: native_admin and native_admin_token must be set together",
                    path.display()
                ));
            }
        }
        Ok(cfg)
    }

    pub fn ini_path(&self) -> PathBuf {
        self.game_dir.join(&self.ini)
    }

    pub fn exe_path(&self) -> PathBuf {
        match self.kind {
            ServerKind::Stock => self.game_dir.join("mxbikes.exe"),
            ServerKind::Native => self.game_dir.join(&self.native_binary),
        }
    }

    pub fn server_config_path(&self) -> PathBuf {
        match self.kind {
            ServerKind::Stock => self.ini_path(),
            ServerKind::Native => self.game_dir.join(&self.native_config),
        }
    }

    pub fn tracks_dir(&self) -> PathBuf {
        match self.kind {
            ServerKind::Stock => self.game_dir.join("mods").join("tracks"),
            ServerKind::Native => self.game_dir.join("tracks"),
        }
    }

    pub fn track_library_dir(&self) -> PathBuf {
        self.track_library.clone().unwrap_or_else(|| {
            self.game_dir
                .parent()
                .unwrap_or(&self.game_dir)
                .join("mxbserver-track-library")
        })
    }

    pub fn version_path(&self) -> PathBuf {
        self.game_dir
            .join(format!("{}.version", self.native_binary))
    }
}

/// Whether `presented` matches `expected`, compared in constant time.
///
/// A short-circuiting `==` leaks the length of the matching prefix through timing, which
/// over enough attempts recovers the token a byte at a time. The comparison is cheap and
/// the attack is not theoretical for a network-reachable admin API.
pub fn token_matches(expected: &str, presented: &str) -> bool {
    let (a, b) = (expected.as_bytes(), presented.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// The token out of an `Authorization: Bearer …` header value.
pub fn bearer(header_value: &str) -> Option<&str> {
    let rest = header_value
        .strip_prefix("Bearer ")
        .or_else(|| header_value.strip_prefix("bearer "))?;
    let rest = rest.trim();
    (!rest.is_empty()).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_tokens_match_and_different_ones_do_not() {
        assert!(token_matches("s3cret", "s3cret"));
        assert!(!token_matches("s3cret", "s3creT"));
        assert!(!token_matches("s3cret", "s3cre"));
        assert!(!token_matches("s3cret", ""));
    }

    #[test]
    fn parses_a_bearer_header() {
        assert_eq!(bearer("Bearer abc123"), Some("abc123"));
        assert_eq!(bearer("bearer abc123"), Some("abc123"));
    }

    #[test]
    fn rejects_header_shapes_that_are_not_a_bearer_token() {
        assert_eq!(bearer("Basic abc123"), None);
        assert_eq!(bearer("Bearer "), None);
        assert_eq!(bearer("abc123"), None);
    }

    #[test]
    fn native_paths_cannot_escape_the_game_directory() {
        let dir = std::env::temp_dir().join(format!("mxb-agent-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.json");
        std::fs::write(&path, r#"{"token":"abcdefghijklmnopqrstuvwxyz123456","kind":"native","game_dir":".","native_binary":"../mxbserver"}"#).unwrap();
        assert!(Config::load(&path)
            .unwrap_err()
            .contains("relative file name"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
