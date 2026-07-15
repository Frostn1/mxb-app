use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Persisted app configuration. `mods_path` is the MX Bikes root folder,
/// e.g. `…/Documents/PiBoSo/MX Bikes`. The behaviour flags default ON
/// (Discord-style always-on companion) and `#[serde(default)]` keeps older
/// config files (which only had `mods_path`) loading without losing it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub mods_path: String,
    /// Hide to the tray on window close and keep running.
    pub run_in_background: bool,
    /// Start MXB App automatically on login.
    pub launch_at_startup: bool,
    /// Launch FrostMod automatically when the app opens.
    pub auto_run_frostmod: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mods_path: String::new(),
            run_in_background: true,
            launch_at_startup: true,
            auto_run_frostmod: true,
        }
    }
}

/// Location of the config file inside the OS local app-data dir — on Windows
/// `%LOCALAPPDATA%\com.frost.mxbikes\config.json`. Local (not Roaming) keeps all
/// app state — config, shop session, FrostMod, cache, logs — in one per-machine
/// folder rather than syncing settings across domain machines.
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

/// Fill in the default MX Bikes path when the user chose "Recommended".
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
    cfg
}
