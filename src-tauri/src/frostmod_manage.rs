//! In-app FrostMod management: download FrostMod from its GitHub releases into an
//! app-owned folder, run `frostmod.exe` as a managed background process (injector
//! mode), and update it. The injector is Windows-only, so process control is
//! `#[cfg(windows)]` with graceful stubs elsewhere (mirrors `frostmod.rs`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
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

/// FrostMod reads its server-browser filter from this file in its own folder,
/// auto-generating a default on first run. That stock default *hides Kaizo*
/// community servers — it carries a `kaizo` name rule plus a `k[a4][il1]z[o0]`
/// regex meant to catch cheat-shop spam, which also matches the legit Kaizo
/// servers. We drop in a curated copy that keeps the spam rules but no longer
/// matches Kaizo, so those servers stay visible in the browser.
const SERVERFILTER_FILE: &str = "frostmod_serverfilter.yaml";

/// Our curated filter. Same `# frostmod-filter v4` sentinel FrostMod expects on
/// line 1 (so it treats the config as current and won't rewrite ours), spam
/// regex kept, all Kaizo rules removed.
const CURATED_SERVERFILTER: &str = "# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|\\.pr0\\b)'
";

/// FrostMod's stock v4 default — the one that hides Kaizo. We only replace an
/// on-disk filter that still matches this exactly (ignoring line endings), so a
/// user's own edits to the filter are never clobbered.
const STOCK_SERVERFILTER: &str = "# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
  - kaizo
  - kalz0
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|k[a4][il1]z[o0]|\\.pr0\\b)'
";

fn serverfilter_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join(SERVERFILTER_FILE)
}

/// Compare filter text ignoring line endings (FrostMod writes CRLF on Windows)
/// and trailing blank space, so those alone don't read as a user edit.
fn filter_eq(a: &str, b: &str) -> bool {
    a.replace('\r', "").trim_end() == b.replace('\r', "").trim_end()
}

/// Drop our curated server filter into FrostMod's folder so Kaizo servers stay
/// visible. Writes only when the file is missing or still holds FrostMod's stock
/// Kaizo-blocking default — a filter the user has edited is left untouched.
/// Best-effort: failures are logged, never fatal to install/launch.
pub fn ensure_serverfilter(app: &AppHandle) {
    let path = serverfilter_path(app);
    let should_write = match std::fs::read_to_string(&path) {
        Ok(cur) => filter_eq(&cur, STOCK_SERVERFILTER),
        Err(_) => true, // missing / unreadable -> lay down our copy
    };
    if !should_write {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, CURATED_SERVERFILTER) {
        Ok(()) => log::info!("wrote curated FrostMod server filter (Kaizo unhidden): {}", path.display()),
        Err(e) => log::warn!("could not write FrostMod server filter {}: {e}", path.display()),
    }
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

/// Overwrite a file, retrying briefly if it's still locked. Right after we stop
/// FrostMod, Windows can take a moment to release the handles on the running
/// `frostmod.exe`/`.dll`, so a plain write would fail with a sharing violation.
fn write_with_retry(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..15 {
        match std::fs::write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Err(last.expect("loop runs at least once"))
}

/// Download `frostmod.exe` + `frostmod.dll` from the latest release into our
/// managed folder and record the version. Also used for updates. Returns the tag.
/// Callers must stop a running FrostMod first (see `frostmod_install`), or the
/// overwrite hits a locked file.
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
            write_with_retry(&dir.join(want), &bytes)?;
            got += 1;
        }
    }
    if got == 0 {
        anyhow::bail!("the latest FrostMod release has no frostmod.exe/.dll");
    }
    std::fs::write(version_path(app), &rel.tag_name)?;
    // Ship our curated server filter so Kaizo servers aren't hidden by FrostMod's
    // stock default. Best-effort; never fails the install.
    ensure_serverfilter(app);
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
    // Refresh the curated filter before FrostMod loads it, so existing installs
    // (whose stock default hides Kaizo) get corrected on the next launch too.
    ensure_serverfilter(app);
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

/// Force-terminate any running `frostmod.exe` — including an instance this app
/// didn't spawn (e.g. one left from a previous session) — so its files can be
/// overwritten during an update. Best-effort; a no-op when none is running.
#[cfg(windows)]
pub fn force_stop_exe() {
    use std::os::windows::process::CommandExt;
    /// Don't flash a console window for the kill.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "frostmod.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(windows))]
pub fn force_stop_exe() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_filter_unhides_kaizo_but_keeps_sentinel() {
        // FrostMod only respects a config whose first line is the v4 sentinel.
        assert!(CURATED_SERVERFILTER.starts_with("# frostmod-filter v4"));
        // Kaizo must no longer be matched, by name or the spam regex.
        let lc = CURATED_SERVERFILTER.to_lowercase();
        assert!(!lc.contains("kaizo"));
        assert!(!lc.contains("kalz0"));
        assert!(!CURATED_SERVERFILTER.contains("k[a4][il1]z[o0]"));
        // Spam rules we keep.
        assert!(CURATED_SERVERFILTER.contains("che4ts"));
        assert!(CURATED_SERVERFILTER.contains(r"\.pr0\b"));
    }

    #[test]
    fn stock_default_is_the_kaizo_blocking_one() {
        // Guards our overwrite trigger: the stock text must actually block Kaizo.
        assert!(STOCK_SERVERFILTER.contains("- kaizo"));
        assert!(STOCK_SERVERFILTER.contains("k[a4][il1]z[o0]"));
    }

    #[test]
    fn filter_eq_ignores_line_endings_and_trailing_space() {
        let crlf = STOCK_SERVERFILTER.replace('\n', "\r\n");
        assert!(filter_eq(&crlf, STOCK_SERVERFILTER));
        assert!(filter_eq(&format!("{STOCK_SERVERFILTER}\n\n"), STOCK_SERVERFILTER));
        // A real edit (curated vs stock) must NOT compare equal.
        assert!(!filter_eq(CURATED_SERVERFILTER, STOCK_SERVERFILTER));
    }
}
