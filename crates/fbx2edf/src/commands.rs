//! The Convert screen's Tauri commands, registered as they are by Frost's Studio and Frost
//! Converter: `fbx2edf::commands::fbx_convert` and the rest. Each is a thin call into
//! [`crate::job`]; the progress events are `fbx-convert-progress` and `fbx-convert-done`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::host::{self, Options};
use crate::scripts::{self, Scripts};
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
pub fn fbx_output_paths(models: Vec<Model>, folder: Option<String>, bike: bool, name: Option<String>) -> Vec<String> {
    let folder = folder.filter(|f| !f.is_empty()).map(PathBuf::from);
    job::output_paths(&models, folder.as_deref(), bike, name.as_deref())
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

fn scripts_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path().app_config_dir().map_err(|e| format!("no config directory: {e}"))
}

/// The export scripts kept between runs: the last one used and the saved list.
#[tauri::command]
pub fn fbx_scripts(app: tauri::AppHandle) -> Scripts {
    scripts_dir(&app).map(|d| scripts::load(&d)).unwrap_or_default()
}

/// Remember the script just used (empty `text` forgets it).
#[tauri::command]
pub fn fbx_scripts_last(app: tauri::AppHandle, name: String, text: String) -> Result<Scripts, String> {
    scripts::set_last(&scripts_dir(&app)?, &name, &text)
}

/// Save a script under a name.
#[tauri::command]
pub fn fbx_scripts_save(app: tauri::AppHandle, name: String, text: String) -> Result<Scripts, String> {
    scripts::save(&scripts_dir(&app)?, &name, &text)
}

/// Rename a saved script.
#[tauri::command]
pub fn fbx_scripts_rename(app: tauri::AppHandle, from: String, to: String) -> Result<Scripts, String> {
    scripts::rename(&scripts_dir(&app)?, &from, &to)
}

/// Remove a saved script.
#[tauri::command]
pub fn fbx_scripts_remove(app: tauri::AppHandle, name: String) -> Result<Scripts, String> {
    scripts::remove(&scripts_dir(&app)?, &name)
}

/// Write one material's `.shd` with these maps and values: the Convert screen's sliders.
#[tauri::command]
pub fn fbx_write_shd(path: String, maps: Maps, values: ShdValues) -> Result<(), String> {
    job::save_shd(Path::new(&path), &maps, &values)
}

/// Convert a batch. Each pair's progress and outcome is sent as it happens, and every outcome
/// is returned at the end, in order. Each model gets a `.shd` per material beside its `.edf`
/// (only where there is none, in the model's folder; `write_shd` false writes none), with `shd` the values they start from.
#[tauri::command]
pub async fn fbx_convert(
    app: tauri::AppHandle,
    pairs: Vec<Pair>,
    options: Options,
    hrc: HrcMode,
    write_shd: Option<bool>,
    shd: Option<ShdValues>,
) -> Result<Vec<Outcome>, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A batch is already converting.".into());
    }
    let shd = Shd {
        make: write_shd.unwrap_or(true),
        overwrite: false,
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

/// The debouncer watching the Preview's sources: dropping it stops the watch.
static WATCH: std::sync::Mutex<Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>> = std::sync::Mutex::new(None);

#[derive(Clone, Serialize)]
struct WatchEvent {
    path: String,
}

/// Watch these source files; when one changes (an export saved over it), `fbx-source-changed`
/// is emitted with its path once the writes settle. Replaces any earlier watch.
#[tauri::command]
pub fn fbx_watch(app: tauri::AppHandle, paths: Vec<String>) -> Result<(), String> {
    use notify::RecursiveMode;
    let watched: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let mut dirs: Vec<PathBuf> = watched.iter().filter_map(|p| p.parent().map(Path::to_path_buf)).collect();
    dirs.sort();
    dirs.dedup();
    let list = watched.clone();
    let mut debouncer = notify_debouncer_mini::new_debouncer(job::WATCH_SETTLE, move |res: notify_debouncer_mini::DebounceEventResult| {
        let Ok(events) = res else { return };
        for e in events {
            if let Some(hit) = job::watched_match(&list, &e.path) {
                let _ = app.emit("fbx-source-changed", WatchEvent { path: hit.to_string_lossy().into_owned() });
            }
        }
    })
    .map_err(|e| e.to_string())?;
    for d in &dirs {
        debouncer.watcher().watch(d, RecursiveMode::NonRecursive).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    *WATCH.lock().map_err(|e| e.to_string())? = Some(debouncer);
    Ok(())
}

/// Stop watching.
#[tauri::command]
pub fn fbx_unwatch() {
    if let Ok(mut w) = WATCH.lock() {
        *w = None;
    }
}
