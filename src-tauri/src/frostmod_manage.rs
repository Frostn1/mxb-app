//! In-app FrostMod management: download FrostMod from its GitHub releases into an
//! app-owned folder, run `frostmod.exe` as a managed background process (injector
//! mode), and update it. The injector is Windows-only, so process control is
//! `#[cfg(windows)]` with graceful stubs elsewhere (mirrors `frostmod.rs`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// FrostMod's GitHub repo — releases carry `frostmod.exe` + `frostmod.dll`.
const REPO: &str = "Frostn1/frostmod";
const UA: &str = "mxb-app";

/// Managed FrostMod child process (only ever `Some` on Windows while running).
#[derive(Default)]
pub struct FrostmodProcess(pub Mutex<Option<std::process::Child>>);

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrostmodStatus {
    /// Whether `frostmod.exe` is present in our managed folder.
    pub installed: bool,
    /// Installed release tag, if known.
    pub version: Option<String>,
    /// Latest release tag on GitHub (None if the check failed / offline).
    pub latest: Option<String>,
    /// Whether FrostMod is currently running (its reload event exists).
    pub running: bool,
}

fn frostmod_dir(app: &AppHandle) -> PathBuf {
    // Local app-data dir (Windows: `%LOCALAPPDATA%\com.frost.mxbikes\frostmod`),
    // alongside config/cache/logs — see `config::config_path`.
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("frostmod")
}

fn exe_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join("frostmod.exe")
}

fn version_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join("version.txt")
}

fn installed_version(app: &AppHandle) -> Option<String> {
    std::fs::read_to_string(version_path(app))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn is_installed(app: &AppHandle) -> bool {
    exe_path(app).exists()
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

async fn latest_release() -> anyhow::Result<Release> {
    let client = reqwest::Client::builder().user_agent(UA).build()?;
    let rel = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json::<Release>()
        .await?;
    Ok(rel)
}

/// Current install + latest-available snapshot. `latest` is best-effort (network).
pub async fn status(app: &AppHandle) -> FrostmodStatus {
    let latest = latest_release().await.ok().map(|r| r.tag_name);
    FrostmodStatus {
        installed: is_installed(app),
        version: installed_version(app),
        latest,
        running: crate::frostmod::is_running(),
    }
}

/// Download `frostmod.exe` + `frostmod.dll` from the latest release into our
/// managed folder and record the version. Also used for updates. Returns the tag.
pub async fn install(app: &AppHandle) -> anyhow::Result<String> {
    let rel = latest_release().await?;
    let dir = frostmod_dir(app);
    std::fs::create_dir_all(&dir)?;

    let client = reqwest::Client::builder().user_agent(UA).build()?;
    let mut got = 0;
    for want in ["frostmod.exe", "frostmod.dll"] {
        if let Some(asset) = rel.assets.iter().find(|a| a.name.eq_ignore_ascii_case(want)) {
            let bytes = client
                .get(&asset.browser_download_url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            std::fs::write(dir.join(want), &bytes)?;
            got += 1;
        }
    }
    if got == 0 {
        anyhow::bail!("the latest FrostMod release has no frostmod.exe/.dll");
    }
    std::fs::write(version_path(app), &rel.tag_name)?;
    Ok(rel.tag_name)
}

/// Launch `frostmod.exe` hidden as a managed child, unless it's already running.
/// Returns whether we started it. Windows-only (the injector is a Windows binary).
#[cfg(windows)]
pub fn start(app: &AppHandle, state: &FrostmodProcess) -> anyhow::Result<bool> {
    use std::os::windows::process::CommandExt;
    /// Don't pop a console window for the headless reloader.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if crate::frostmod::is_running() {
        return Ok(false);
    }
    let exe = exe_path(app);
    if !exe.exists() {
        anyhow::bail!("FrostMod isn't installed yet");
    }
    let child = std::process::Command::new(&exe)
        .current_dir(frostmod_dir(app))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    *state.0.lock().unwrap() = Some(child);
    Ok(true)
}

#[cfg(not(windows))]
pub fn start(_app: &AppHandle, _state: &FrostmodProcess) -> anyhow::Result<bool> {
    anyhow::bail!("FrostMod runs on Windows only")
}

/// Kill the managed FrostMod child, if we started one.
pub fn stop(state: &FrostmodProcess) {
    if let Some(mut child) = state.0.lock().unwrap().take() {
        let _ = child.kill();
    }
}
