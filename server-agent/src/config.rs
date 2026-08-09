//! The agent's own settings, read from `agent.json` beside the binary.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Bearer token the app must present. No default — an agent that listens without one
    /// would hand process control to anyone who portscans the box.
    pub token: String,
    /// Where the HTTP API listens.
    #[serde(default = "default_listen")]
    pub listen: String,
    /// The MX Bikes install directory holding `mxbikes.exe` and the server `.ini`.
    pub game_dir: PathBuf,
    /// Server config filename, relative to `game_dir`.
    #[serde(default = "default_ini")]
    pub ini: String,
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
        Ok(cfg)
    }

    pub fn ini_path(&self) -> PathBuf {
        self.game_dir.join(&self.ini)
    }

    pub fn exe_path(&self) -> PathBuf {
        self.game_dir.join("mxbikes.exe")
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
    let rest = header_value.strip_prefix("Bearer ").or_else(|| header_value.strip_prefix("bearer "))?;
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
}
