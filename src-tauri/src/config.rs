use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub mods_path: String,
    /// MX Bikes install dir (`mxbikes.exe` + core `rider.pkz`); distinct from `mods_path`.
    pub game_path: String,
    /// Hide to the tray on window close and keep running.
    pub run_in_background: bool,
    /// Start MXB App automatically on login.
    pub launch_at_startup: bool,
    /// Launch FrostMod automatically when the app opens.
    pub auto_run_frostmod: bool,
    /// Re-run the game's profile loader in place after applying a preset (Windows-only).
    pub instant_refresh: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mods_path: String::new(),
            game_path: String::new(),
            run_in_background: true,
            launch_at_startup: true,
            auto_run_frostmod: true,
            instant_refresh: true,
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
    cfg
}
