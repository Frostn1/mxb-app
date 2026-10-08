//! Settings: the environment first, then an optional `KEY=VALUE` file. The file is for Windows,
//! where a scheduled task has no handy place for environment variables: its path is the first
//! argument, or `MXB_FETCHER_CONFIG`. Every value is trimmed, so a file saved on Windows (CRLF,
//! a BOM) or a token pasted with a trailing newline reads the same.

use anyhow::{Context, Result};
use std::collections::HashMap;

pub struct Config {
    file: HashMap<String, String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = std::env::args()
            .nth(1)
            .or_else(|| std::env::var("MXB_FETCHER_CONFIG").ok())
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        let file = match path {
            Some(p) => parse(&std::fs::read_to_string(&p).with_context(|| format!("reading {p}"))?),
            None => HashMap::new(),
        };
        Ok(Self { file })
    }

    /// A setting, trimmed; None when unset or blank.
    pub fn get(&self, name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .or_else(|| self.file.get(name).cloned())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }
}

/// `KEY=VALUE` lines; blank lines and `#` comments skipped, quotes around a value dropped.
pub fn parse(text: &str) -> HashMap<String, String> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(v);
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// `MXB_FETCHER_HOSTS`: which hosts this fetcher serves, as the control plane reads them
/// (names, `*` for any, `-name` to leave one out). Empty: everything.
pub fn hosts(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_windows_saved_file() {
        let text = "\u{feff}# the fetcher\r\nMXB_FETCHER_API = https://api.mxbsecure.com\r\nMXB_FETCHER_TOKEN=\"abc123\" \r\n\r\nMXB_FETCHER_HOSTS='mxb-mods.com'\r\nnot a setting\r\n";
        let m = parse(text);
        assert_eq!(
            m.get("MXB_FETCHER_API").map(String::as_str),
            Some("https://api.mxbsecure.com")
        );
        assert_eq!(
            m.get("MXB_FETCHER_TOKEN").map(String::as_str),
            Some("abc123")
        );
        assert_eq!(
            m.get("MXB_FETCHER_HOSTS").map(String::as_str),
            Some("mxb-mods.com")
        );
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn reads_a_host_filter() {
        assert_eq!(hosts(Some("* -mxb-mods.com")), vec!["*", "-mxb-mods.com"]);
        assert_eq!(hosts(Some(" MXB-Mods.com,\r\n")), vec!["mxb-mods.com"]);
        assert!(hosts(None).is_empty());
    }
}
