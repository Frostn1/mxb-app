use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    Ok(serde_json::from_str(&text).unwrap_or_default())
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
