//! FrostServer for a dedicated server the player hosts on this PC: one click to install or
//! update the plugin in that server's `plugins` folder, and the server's own protection report.
//!
//! FrostServer (Frostn1/frostmod, docs/FROSTSERVER.md) keeps distant riders visible on a full
//! gate, removes a leaving rider from everyone's track and stops the host crash at +0x7A860.
//! The plugin is the signed `frostserver.dlo` from FrostMod's newest release, checked against
//! the SHA-256 GitHub publishes for it before it is written.
//!
//! It never goes into the folder the player's own game runs from: the client game loads every
//! plugin in its folder too, and FrostServer is server code.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const REPO: &str = "Frostn1/frostmod";
const ASSET: &str = "frostserver.dlo";
const MARKER: &str = "frostserver.version";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_PORT: u16 = 54210;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Release {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

pub fn parse_latest(json: &str) -> Result<Release, String> {
    let r: GhRelease = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = r.tag_name;
    let asset = r
        .assets
        .into_iter()
        .find(|a| a.name == ASSET)
        .ok_or_else(|| format!("FrostMod {tag} has no {ASSET}"))?;
    let sha = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase)
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| format!("FrostMod {tag} publishes no checksum for {ASSET}; not installing it"))?;
    Ok(Release { tag, url: asset.browser_download_url, sha256: sha })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The folder holds `mxbikes.exe`.
    pub is_server_folder: bool,
    /// It is the folder the player's own game runs from (FrostServer is not installed there).
    pub is_game_folder: bool,
    pub installed: bool,
    /// The release tag the app installed, empty when the plugin was put there by hand.
    pub version: Option<String>,
    pub latest: Option<String>,
    pub latest_error: Option<String>,
    /// `GET /frostserver/protection` from the running server, when it answers.
    pub protection: Option<serde_json::Value>,
}

fn plugins(folder: &Path) -> PathBuf {
    folder.join("plugins")
}

pub fn same_folder(a: &Path, b: &Path) -> bool {
    match (dunce_canon(a), dunce_canon(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

fn dunce_canon(p: &Path) -> Option<String> {
    std::fs::canonicalize(p).ok().map(|c| c.to_string_lossy().trim_start_matches(r"\\?\").to_ascii_lowercase())
}

fn port_of(folder: &Path) -> u16 {
    let Ok(text) = std::fs::read_to_string(plugins(folder).join("frostserver.yaml")) else {
        return DEFAULT_PORT;
    };
    text.lines()
        .filter(|l| !l.starts_with([' ', '\t']))
        .find_map(|l| l.strip_prefix("port:"))
        .and_then(|v| v.split('#').next())
        .and_then(|v| v.trim().trim_matches(['\'', '"']).parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(DEFAULT_PORT)
}

fn web() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("MXB-App/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

async fn latest() -> Result<Release, String> {
    let text = web()?
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't read FrostMod's latest release: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    parse_latest(&text)
}

async fn protection(port: u16) -> Option<serde_json::Value> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?
        .get(format!("http://127.0.0.1:{port}/frostserver/protection"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

pub async fn status(folder: &Path, game_folder: Option<&Path>) -> Status {
    let installed = plugins(folder).join(ASSET).is_file();
    let version = installed.then(|| {
        std::fs::read_to_string(plugins(folder).join(MARKER)).map(|v| v.trim().to_string()).unwrap_or_default()
    });
    let (latest, latest_error) = match latest().await {
        Ok(r) => (Some(r.tag), None),
        Err(e) => (None, Some(e)),
    };
    let protection = if installed { protection(port_of(folder)).await } else { None };
    Status {
        is_server_folder: folder.join("mxbikes.exe").is_file(),
        is_game_folder: game_folder.is_some_and(|g| same_folder(g, folder)),
        installed,
        version,
        latest,
        latest_error,
        protection,
    }
}

/// Download, check and write the plugin; returns the tag installed.
pub async fn install(folder: &Path, game_folder: Option<&Path>) -> Result<String, String> {
    if !folder.join("mxbikes.exe").is_file() {
        return Err("That folder has no mxbikes.exe. Pick the folder your dedicated server runs from.".into());
    }
    if game_folder.is_some_and(|g| same_folder(g, folder)) {
        return Err("That's the folder your own game runs from. FrostServer is for a separate dedicated-server folder: the game would load it too.".into());
    }
    let release = latest().await?;
    let bytes = web()?
        .get(&release.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't download {ASSET}: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES || bytes.len() < 2 || &bytes[..2] != b"MZ" {
        return Err(format!("{ASSET} from {} is not a Windows plugin; not installing it", release.tag));
    }
    let have = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&bytes))
    };
    if have != release.sha256 {
        return Err(format!("{ASSET} doesn't match its published SHA-256 (got {have}); not installing it"));
    }
    let dir = plugins(folder);
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let staged = dir.join("frostserver.dlo.download");
    std::fs::write(&staged, &bytes).map_err(|e| format!("couldn't write the plugin: {e}"))?;
    std::fs::rename(&staged, dir.join(ASSET)).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        format!("couldn't replace {ASSET} (is the server running? stop it, then try again): {e}")
    })?;
    let _ = std::fs::write(dir.join(MARKER), format!("{}\n", release.tag));
    Ok(release.tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_plugin_and_its_checksum() {
        let sha = "4022c42bc25318ac6b3884f3fc2c070f68fcfdc9701511d2ee6c6be0e410b39c";
        let json = format!(r#"{{"tag_name":"v0.51.0","assets":[{{"name":"frostserver.dlo","browser_download_url":"https://x/b","digest":"sha256:{sha}"}}]}}"#);
        let r = parse_latest(&json).unwrap();
        assert_eq!((r.tag.as_str(), r.sha256.as_str()), ("v0.51.0", sha));
        assert!(parse_latest(r#"{"tag_name":"v1","assets":[{"name":"frostserver.dlo","browser_download_url":"u"}]}"#).is_err());
    }

    #[test]
    fn refuses_a_folder_without_the_server_and_the_players_own_game() {
        let d = std::env::temp_dir().join(format!("fs-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert!(rt.block_on(install(&d, None)).unwrap_err().contains("mxbikes.exe"));
        std::fs::write(d.join("mxbikes.exe"), b"MZ").unwrap();
        assert!(rt.block_on(install(&d, Some(&d))).unwrap_err().contains("your own game"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
