// Windows: no console window behind the app in a release build. The command line attaches to
// the console it was started from instead (see `cli::console`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Frost Converter: FBX to EDF for MX Bikes, on its own.
//!
//! The window is Frost's Studio's Convert screen and the command line runs the same batch, both
//! from `crates/fbx2edf`, so a file made here is byte for byte the one the Studio makes.

mod cli;

use std::path::PathBuf;

/// Paths the window was opened with: FBX files dropped on the exe's icon.
struct Opened(Vec<String>);

#[tauri::command]
fn converter_opened(opened: tauri::State<Opened>) -> Vec<String> {
    opened.0.clone()
}

/// Frontend log lines, into the same file the Rust side writes. The shared 3D viewer reports
/// its renderer through this.
#[tauri::command]
fn log_client(level: String, message: String) {
    mxb_core::clientlog::record(&level, &message);
}

#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path not found: {path}"));
    }
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("explorer").arg("/select,").arg(&p).spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg("-R").arg(&p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(p.parent().unwrap_or(&p)).spawn();
    spawned.map(|_| ()).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // From a terminal or a script, any argument means the command line. From Explorer (files
    // dropped on the icon) there is no console to write to, so the window opens with them.
    if !args.is_empty() && cli::console() {
        std::process::exit(cli::run(&args));
    }
    let opened = args.into_iter().filter(|a| !a.starts_with('-')).collect();

    tauri::Builder::default()
        .manage(Opened(opened))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            fbx2edf::commands::fbx_convert_available,
            fbx2edf::commands::fbx_scan,
            fbx2edf::commands::fbx_part_names,
            fbx2edf::commands::fbx_is_bike,
            fbx2edf::commands::fbx_output_paths,
            fbx2edf::commands::fbx_read_text,
            fbx2edf::commands::fbx_convert,
            fbx2edf::commands::fbx_write_shd,
            fbx2edf::commands::fbx_scripts,
            fbx2edf::commands::fbx_scripts_last,
            fbx2edf::commands::fbx_scripts_save,
            fbx2edf::commands::fbx_scripts_remove,
            fbx2edf::commands::fbx_scripts_rename,
            fbx2edf::commands::fbx_watch,
            fbx2edf::commands::fbx_unwatch,
            mxb_core::viewer::load_model_file,
            mxb_core::viewer::texture_bytes,
            // The theme reads the supporter pack's cached answer (the manager keeps it fresh).
            mxb_core::supporter::supporter_status,
            mxb_core::supporter::supporter_refresh,
            converter_opened,
            reveal_in_explorer,
            log_client,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frost Converter");
}
