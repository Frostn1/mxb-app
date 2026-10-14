// Windows: no console window behind the app in a release build. The command line attaches to
// the console it was started from instead (see `cli::console`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Frost Importer: a mod's .pkz, .edf or .fbx to a .blend, ready to edit.
//!
//! The reverse of Frost Converter, in the same window. A creator-locked `.pkz` is read in memory
//! the way MXB App reads it; `.mxbsecure` and other sealed files are left alone and listed.

mod cli;
mod lineup;
mod plan;
mod run;

use std::path::PathBuf;

use tauri::Emitter;

/// Paths the window was opened with: files dropped on the exe's icon.
struct Opened(Vec<String>);

#[tauri::command]
fn importer_opened(opened: tauri::State<Opened>) -> Vec<String> {
    opened.0.clone()
}

#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path not found: {path}"));
    }
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("explorer").arg("/select,").arg(&p).spawn();
    #[cfg(not(target_os = "windows"))]
    let spawned = std::process::Command::new("open").arg("-R").arg(&p).spawn();
    spawned.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn import_scan(paths: Vec<String>) -> Vec<plan::Found> {
    plan::scan(&paths)
}

#[tauri::command]
fn import_find_blender() -> Option<String> {
    run::find_blender().map(|p| p.to_string_lossy().to_string())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Done {
    index: usize,
    outcome: run::Outcome,
}

#[tauri::command]
async fn import_run(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: plan::Options,
    folder: Option<String>,
    blender: Option<String>,
) -> Result<Vec<run::Outcome>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let folder = folder.filter(|f| !f.trim().is_empty()).map(PathBuf::from);
        let blender = blender.filter(|b| !b.trim().is_empty()).map(PathBuf::from);
        let mut all = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            let a = app.clone();
            let mut tick = move |done: u32, total: u32, stage: &str| {
                let _ = a.emit("import-progress", run::Progress { index, done, total, stage: stage.to_string() });
            };
            let outcome = run::import_item(path, options, folder.as_deref(), blender.as_deref(), &mut tick);
            let _ = app.emit("import-done", Done { index, outcome: outcome.clone() });
            all.push(outcome);
        }
        all
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LineupDone {
    lineup: lineup::Outcome,
    preview: Option<run::Outcome>,
}

/// Where the app puts a line-up's preview `.blend`.
fn lineup_blends() -> PathBuf {
    let docs = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Documents");
    docs.join("MXB").join("Line up")
}

#[tauri::command]
async fn lineup_run(
    swap: String,
    target: String,
    name: Option<String>,
    anchor: lineup::Anchor,
    dry: bool,
    preview: bool,
    blender: Option<String>,
) -> Result<LineupDone, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = PathBuf::from(&target);
        let o = lineup::line_up(std::path::Path::new(&swap), &target, name.as_deref(), anchor, dry).map_err(|e| format!("{e:#}"))?;
        let preview = match (&o.out, preview) {
            (Some(out), true) => {
                let geom = lineup::bike_geom(&target).map_err(|e| format!("{e:#}"))?;
                let blender = blender.filter(|b| !b.trim().is_empty()).map(PathBuf::from);
                Some(run::preview_lineup(std::path::Path::new(out), geom, &lineup_blends(), blender.as_deref(), &mut |_, _, _| {}))
            }
            _ => None,
        };
        Ok(LineupDone { lineup: o, preview })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() && cli::console() {
        std::process::exit(cli::run(&args));
    }
    let opened = args.into_iter().filter(|a| !a.starts_with('-')).collect();

    tauri::Builder::default()
        .manage(Opened(opened))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            importer_opened,
            reveal_in_explorer,
            import_scan,
            import_find_blender,
            import_run,
            lineup_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frost Importer");
}
