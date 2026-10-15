//! The Convert screen's Tauri commands, registered as they are by Frost's Studio and Frost
//! Converter: `fbx2edf::commands::fbx_convert` and the rest. Each is a thin call into
//! [`crate::job`]; the progress events are `fbx-convert-progress` and `fbx-convert-done`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::host::{self, Options};
use crate::job::{self, Found, HrcMode, Maps, Model, Outcome, Progress, Shd, ShdValues};

#[derive(Debug, Deserialize)]
pub struct Pair {
    pub input: String,
    pub output: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    index: usize,
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent<'a> {
    index: usize,
    outcome: &'a Outcome,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn fbx_convert_available() -> bool {
    host::available()
}

#[tauri::command]
pub fn fbx_scan(paths: Vec<String>) -> Vec<Found> {
    job::scan(&paths)
}

/// The objects an FBX makes as bike parts, for telling a bike from a single model.
#[tauri::command]
pub async fn fbx_part_names(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || host::part_names(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// Whether an FBX is a bike, by its parts.
#[tauri::command]
pub async fn fbx_is_bike(path: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || job::is_bike(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// Where each model is written (see [`job::output_paths`]).
#[tauri::command]
pub fn fbx_output_paths(models: Vec<Model>, folder: Option<String>, bike: bool) -> Vec<String> {
    let folder = folder.filter(|f| !f.is_empty()).map(PathBuf::from);
    job::output_paths(&models, folder.as_deref(), bike)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// A parameter file or export script, as text. Small files only: these are a few lines.
#[tauri::command]
pub fn fbx_read_text(path: String) -> Result<String, String> {
    let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if len > 1 << 20 {
        return Err(format!("{path} is too large to be a parameter file"));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
}

/// Write one material's `.shd` with these maps and values: the Convert screen's sliders.
#[tauri::command]
pub fn fbx_write_shd(path: String, maps: Maps, values: ShdValues) -> Result<(), String> {
    job::save_shd(Path::new(&path), &maps, &values)
}

/// Convert a batch. Each pair's progress and outcome is sent as it happens, and every outcome
/// is returned at the end, in order. Each model gets a `.shd` per material beside its `.edf`
/// (`overwrite_shd` writes over the ones there), with `shd` the values they start from.
#[tauri::command]
pub async fn fbx_convert(
    app: tauri::AppHandle,
    pairs: Vec<Pair>,
    options: Options,
    hrc: HrcMode,
    overwrite_shd: Option<bool>,
    shd: Option<ShdValues>,
) -> Result<Vec<Outcome>, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A batch is already converting.".into());
    }
    let shd = Shd {
        make: true,
        overwrite: overwrite_shd.unwrap_or(false),
        values: shd.unwrap_or_default(),
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|p| (PathBuf::from(p.input), PathBuf::from(p.output))).collect();
        let started = std::time::Instant::now();
        let outcomes = job::run(&pairs, &options, hrc, shd, &mut |event| match event {
            Progress::Pictures { index, done, total } => {
                let _ = app.emit("fbx-convert-progress", ProgressEvent { index, done, total });
            }
            Progress::Done { index, outcome } => {
                let _ = app.emit("fbx-convert-done", DoneEvent { index, outcome });
            }
        });
        log::info!("[convert] {} files in {} ms", pairs.len(), started.elapsed().as_millis());
        outcomes
    })
    .await
    .map_err(|e| e.to_string());
    RUNNING.store(false, Ordering::SeqCst);
    result?
}
