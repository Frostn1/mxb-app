//! Cinema's own settings: which Blender, and where shots go. Kept in Cinema's folder beside
//! the shared config rather than in it: MXB App rewrites `config.json` from its own struct and
//! would drop keys it doesn't know.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::blender;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// The Blender the rider picked. Empty: the newest one found.
    pub blender_path: String,
    /// Where shots are written. Empty: `Documents\MXB Cinema`.
    pub output_dir: String,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    Some(mxb_core::config::data_dir(app)?.join("cinema").join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    file(app).and_then(|f| fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let f = file(app).ok_or("noDataDir")?;
    if let Some(p) = f.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    fs::write(f, serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn default_output() -> PathBuf {
    let docs = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join("Documents");
    docs.join("MXB Cinema")
}

pub fn output_dir(s: &Settings) -> PathBuf {
    if s.output_dir.trim().is_empty() {
        default_output()
    } else {
        PathBuf::from(s.output_dir.trim())
    }
}

/// The Blender a job runs: the picked one when it is still there, else the newest found.
pub fn blender_exe(app: &AppHandle, s: &Settings) -> Option<String> {
    if !s.blender_path.trim().is_empty() {
        return blender::install(std::path::Path::new(s.blender_path.trim())).map(|i| i.path);
    }
    let cfg = crate::sessions::load_config(app);
    blender::detect(&cfg.install_dir()).into_iter().find(|i| !i.too_old).map(|i| i.path)
}

/// Everything the Settings page shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub settings: Settings,
    pub found: Vec<blender::Install>,
    /// The Blender a job would use right now.
    pub using: Option<blender::Install>,
    pub output: String,
}

pub fn view(app: &AppHandle) -> View {
    let s = load(app);
    let cfg = crate::sessions::load_config(app);
    let found = blender::detect(&cfg.install_dir());
    let using = blender_exe(app, &s).and_then(|p| blender::install(std::path::Path::new(&p)));
    View { output: output_dir(&s).to_string_lossy().into_owned(), settings: s, found, using }
}
