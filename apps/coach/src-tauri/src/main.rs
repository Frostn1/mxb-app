// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MXB Coach — lap-time coaching for MX Bikes.
//!
//! A skeleton: it answers only what the shared shell asks for. The config and the game list
//! are wrapped here, as in the studio, rather than shared as commands — core owns the logic,
//! each app wraps the part of it that app means.

mod analysis;
mod bikecfg;
mod coach;
mod fixes;
mod ground;
mod lines;
mod stp;
mod surface;
mod telemetry;

use mxb_core::{config, game};

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> config::AppConfig {
    config::load(&app).unwrap_or_default()
}

#[tauri::command]
fn list_games() -> Vec<game::GameInfo> {
    game::all_info()
}

/// Frontend log lines, into the same file the Rust side writes. The shared 3D viewer reports
/// its renderer through this.
#[tauri::command]
fn log_client(level: String, message: String) {
    // A log line is not a transport for arbitrary payloads: trim rather than reject.
    let msg: String = message.chars().take(2000).collect();
    match level.as_str() {
        "error" => log::error!("[webview] {msg}"),
        "warn" => log::warn!("[webview] {msg}"),
        _ => log::info!("[webview] {msg}"),
    }
}

/// Show a file in the OS file manager, selected. Core's, as the manager and studio wrap it.
#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    mxb_core::library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

/// Open a folder in the OS file manager, making it first if it isn't there yet: the sessions
/// folder only appears once the recorder has written to it.
#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let _ = std::fs::create_dir_all(&path);
    mxb_core::library::open_folder(&path).map_err(|e| format!("{e:#}"))
}

/// The coach's own builds: `coach-v` releases in the manager's repo, betas when asked for.
#[tauri::command]
async fn check_coach_update(
    webview: tauri::Webview,
    beta: bool,
) -> Result<Option<mxb_core::update_channel::UpdateMetadata>, String> {
    mxb_core::update_channel::check(&webview, "Frostn1/mxb-app", "coach-v", beta, "mxb-coach")
        .await
        .map_err(|e| format!("{e:#}"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            // What the shared shell calls: the platform, the config, the titles.
            mxb_core::viewer::app_platform,
            get_config,
            list_games,
            // ── Coach commands ─────────────────────────────────────────────────────
            // Register the coach's own commands below this line.
            coach::coach_status,
            coach::coach_sessions,
            coach::coach_session,
            coach::coach_review,
            coach::coach_surface,
            coach::coach_lines,
            coach::coach_ground,
            coach::coach_setup_plan,
            coach::coach_save_setup,
            check_coach_update,
            reveal_in_explorer,
            open_folder,
            log_client,
            // The track's own terrain, from core, for the map and the 3D view.
            mxb_core::trackview::load_track_terrain,
            mxb_core::trackview::load_track_overview,
            // The rest of what the app's track viewer draws: scenery, its colours, sky, ground.
            mxb_core::trackview::read_track_info,
            mxb_core::trackview::load_track_scenery,
            mxb_core::trackview::load_track_surfaces,
            mxb_core::trackview::load_track_backdrop,
            mxb_core::trackview::load_track_ground,
            mxb_core::trackview::load_track_ground_layers,
            mxb_core::trackview::read_track_placements,
            coach::coach_install_plugin,
            coach::coach_uninstall_plugin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MXB Coach");
}
