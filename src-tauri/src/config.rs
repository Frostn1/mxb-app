use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub mods_path: String,
    /// MX Bikes install dir (`mxbikes.exe` + core `rider.pkz`); distinct from `mods_path`.
    pub game_path: String,
    /// Override for the PiBoSo `profiles` folder. Empty (the normal case) means it
    /// sits inside `mods_path` at `<mods_path>/profiles`. Set only for the edge case
    /// where a player's profiles folder lives outside their MX Bikes folder.
    pub profiles_path: String,
    /// Hide to the tray on window close and keep running.
    pub run_in_background: bool,
    /// Start MXB App automatically on login.
    pub launch_at_startup: bool,
    /// Launch FrostMod automatically when the app opens.
    pub auto_run_frostmod: bool,
    /// Re-run the game's profile loader in place after applying a preset (Windows-only).
    pub instant_refresh: bool,
    /// Watch `<mods_path>/mods` and signal FrostMod to reload when tracks/bikes are
    /// added outside the app (e.g. a manual download dropped into the folder).
    pub watch_mods_reload: bool,
    /// The intro slideshow has been dismissed. Kept here rather than in the webview's
    /// `localStorage` so it survives that storage being cleared (WebView2 resets it on
    /// an app-data wipe, and an OS shutdown can kill the tray-resident process before
    /// it flushes) — losing it made the app re-run its first-run flow on every launch.
    pub welcome_seen: bool,
    /// The first-run guided tour has been finished or skipped. Persisted alongside
    /// `welcome_seen`, for the same reason.
    pub tour_done: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mods_path: String::new(),
            game_path: String::new(),
            profiles_path: String::new(),
            run_in_background: true,
            launch_at_startup: true,
            auto_run_frostmod: true,
            instant_refresh: true,
            watch_mods_reload: true,
            welcome_seen: false,
            tour_done: false,
        }
    }
}

impl AppConfig {
    /// Folder that holds the per-player PiBoSo profiles (each a subdir with a
    /// `profile.ini`). Defaults to `<mods_path>/profiles` — the normal, combined
    /// layout — unless `profiles_path` overrides it for the split-folder edge case.
    pub fn profiles_dir(&self) -> PathBuf {
        let custom = self.profiles_path.trim();
        if custom.is_empty() {
            PathBuf::from(&self.mods_path).join("profiles")
        } else {
            PathBuf::from(custom)
        }
    }
}

pub fn config_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("config.json")
}

pub fn exists(app: &AppHandle) -> bool {
    config_path(app).exists()
}

pub fn load(app: &AppHandle) -> anyhow::Result<AppConfig> {
    let path = config_path(app);
    let text = std::fs::read_to_string(path)?;
    // A truncated/corrupt file is an error, not a silent empty config: callers that
    // can rebuild one (see `load_or_detect`) get the chance to, instead of the app
    // coming up pointed at nothing.
    Ok(serde_json::from_str(&text)?)
}

/// Whether `path` is really a player's MX Bikes folder — the game keeps `profiles/`
/// there, and `mods/` shows up as soon as anything is installed. Checking for those
/// rather than just "the folder exists" keeps auto-detection from quietly adopting an
/// empty `Documents\PiBoSo\MX Bikes` for someone whose setup lives elsewhere; they
/// still get the setup screen to point us at it.
fn looks_like_mods_dir(path: &str) -> bool {
    let dir = Path::new(path.trim());
    if path.trim().is_empty() || !dir.is_dir() {
        return false;
    }
    dir.join("profiles").is_dir() || dir.join("mods").is_dir()
}

/// The saved config, or one built on the spot when the MX Bikes folder sits where it
/// normally does. Returns `None` only when there's genuinely nothing to go on — the
/// one case where the setup screen has something to ask.
///
/// The setup screen never gathered more than this: its default action just runs the
/// same detection. So when the config file goes missing — an app-data wipe, a config
/// written under a different Windows account, a failed write — the app re-detects and
/// carries on rather than walking the user through setup again on every launch.
pub fn load_or_detect(app: &AppHandle) -> Option<AppConfig> {
    if exists(app) {
        match load(app) {
            Ok(cfg) => return Some(cfg),
            Err(e) => log::warn!("config.json unreadable ({e:#}) — re-detecting"),
        }
    }

    let cfg = finalize(AppConfig::default());
    if !looks_like_mods_dir(&cfg.mods_path) {
        return None;
    }
    log::info!("no usable config — auto-detected MX Bikes folder: {}", cfg.mods_path);
    if let Err(e) = save(app, &cfg) {
        log::warn!("couldn't save the auto-detected config: {e:#}");
    }
    Some(cfg)
}

pub fn save(app: &AppHandle, cfg: &AppConfig) -> anyhow::Result<()> {
    let path = config_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg)?)?;
    Ok(())
}

pub fn finalize(mut cfg: AppConfig) -> AppConfig {
    if cfg.mods_path.trim().is_empty() {
        if let Some(docs) = dirs_next::document_dir() {
            cfg.mods_path = docs
                .join("PiBoSo")
                .join("MX Bikes")
                .to_string_lossy()
                .into_owned();
        }
    }
    // Auto-detect the Steam game install (holds `rider.pkz`) so the 3D rider preview
    // works out of the box. Only fills a blank — never overrides a manual pick.
    if cfg.game_path.trim().is_empty() {
        if let Some(gp) = detect_game_path() {
            cfg.game_path = gp;
        }
    }
    cfg
}

/// Locate the MX Bikes install folder (the one containing `rider.pkz`) by scanning
/// Steam libraries. Returns `None` when it can't be found (e.g. non-Steam install).
pub fn detect_game_path() -> Option<String> {
    for lib in steam_libraries() {
        let dir = lib.join("steamapps").join("common").join("MX Bikes");
        if dir.join("rider.pkz").is_file() {
            return Some(dir.to_string_lossy().into_owned());
        }
    }
    None
}

/// Candidate Steam library roots: the default install locations plus any extra
/// libraries registered in `steamapps/libraryfolders.vdf`.
fn steam_libraries() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |roots: &mut Vec<PathBuf>, p: PathBuf| {
        if !roots.contains(&p) {
            roots.push(p);
        }
    };

    #[cfg(windows)]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(pf) = std::env::var(var) {
                push(&mut roots, PathBuf::from(pf).join("Steam"));
            }
        }
        for drive in ['C', 'D', 'E', 'F'] {
            push(&mut roots, PathBuf::from(format!("{drive}:\\Program Files (x86)\\Steam")));
            push(&mut roots, PathBuf::from(format!("{drive}:\\Steam")));
            push(&mut roots, PathBuf::from(format!("{drive}:\\SteamLibrary")));
        }
    }

    #[cfg(not(windows))]
    {
        // Steam on macOS/Linux — lets the detector run (and tests exercise it) off-Windows.
        if let Some(home) = dirs_next::home_dir() {
            push(&mut roots, home.join("Library/Application Support/Steam"));
            push(&mut roots, home.join(".steam/steam"));
            push(&mut roots, home.join(".local/share/Steam"));
        }
    }

    // Extra libraries the user added on other drives, per libraryfolders.vdf.
    for root in roots.clone() {
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for lib in parse_library_paths(&text) {
                push(&mut roots, PathBuf::from(lib));
            }
        }
    }

    roots
}

/// Pull the `"path"  "..."` values out of a Steam `libraryfolders.vdf`.
fn parse_library_paths(vdf: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in vdf.lines() {
        let rest = match line.trim().strip_prefix("\"path\"") {
            Some(r) => r,
            None => continue,
        };
        let start = match rest.find('"') {
            Some(i) => i + 1,
            None => continue,
        };
        if let Some(len) = rest[start..].find('"') {
            // VDF escapes backslashes; normalize `\\` back to `\`.
            out.push(rest[start..start + len].replace("\\\\", "\\"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_dir_defaults_to_mods_subfolder() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/games/mxb".into();
        assert_eq!(cfg.profiles_dir(), PathBuf::from("/games/mxb").join("profiles"));
    }

    #[test]
    fn profiles_dir_uses_override_when_set() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/games/mxb".into();
        cfg.profiles_path = "/other/drive/profiles".into();
        assert_eq!(cfg.profiles_dir(), PathBuf::from("/other/drive/profiles"));
    }

    #[test]
    fn only_a_real_mx_bikes_folder_is_adopted_automatically() {
        let root = std::env::temp_dir()
            .join(format!("frost-config-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(!looks_like_mods_dir(""));
        assert!(!looks_like_mods_dir(&root.join("nope").to_string_lossy()));

        // The folder exists but holds nothing of ours — don't assume it's the one.
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        assert!(!looks_like_mods_dir(&bare.to_string_lossy()));

        // `profiles/` (the game writes it) or `mods/` (we do) makes it recognizable.
        let played = root.join("played");
        std::fs::create_dir_all(played.join("profiles")).unwrap();
        assert!(looks_like_mods_dir(&played.to_string_lossy()));

        let modded = root.join("modded");
        std::fs::create_dir_all(modded.join("mods")).unwrap();
        assert!(looks_like_mods_dir(&modded.to_string_lossy()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn older_configs_keep_their_settings_and_default_the_new_flags() {
        // A config written before the intro flags existed must still load — a parse
        // failure now makes the app re-detect and overwrite it.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"modsPath":"C:\\MXB","gamePath":"C:\\Steam\\MX Bikes","runInBackground":false}"#,
        )
        .expect("older config still parses");
        assert_eq!(cfg.mods_path, "C:\\MXB");
        assert_eq!(cfg.game_path, "C:\\Steam\\MX Bikes");
        assert!(!cfg.run_in_background);
        assert!(cfg.launch_at_startup, "unset fields fall back to the defaults");
        assert!(!cfg.welcome_seen);
        assert!(!cfg.tour_done);
    }

    #[test]
    fn corrupt_config_is_an_error_not_an_empty_config() {
        assert!(serde_json::from_str::<AppConfig>(r#"{"modsPath":"C:\\MX"#).is_err());
    }

    #[test]
    fn parses_library_paths_from_vdf() {
        let vdf = r#"
"libraryfolders"
{
    "0"
    {
        "path"        "C:\\Program Files (x86)\\Steam"
    }
    "1"
    {
        "path"        "D:\\SteamLibrary"
    }
}
"#;
        assert_eq!(
            parse_library_paths(vdf),
            vec![
                "C:\\Program Files (x86)\\Steam".to_string(),
                "D:\\SteamLibrary".to_string(),
            ]
        );
    }
}
