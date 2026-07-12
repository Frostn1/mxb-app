use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Persisted app configuration. `mods_path` is the MX Bikes root folder,
/// e.g. `…/Documents/PiBoSo/MX Bikes`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub mods_path: String,
}

/// Location of the config file inside the OS app-config dir (survives bundling,
/// unlike the previous cwd-relative `.config.json`).
pub fn config_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("could not resolve app config dir")
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
