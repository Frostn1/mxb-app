// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MXB Coach — lap-time coaching for MX Bikes.
//!
//! A skeleton: it answers only what the shared shell asks for. The config and the game list
//! are wrapped here, as in the studio, rather than shared as commands — core owns the logic,
//! each app wraps the part of it that app means.

mod analysis;
mod coach;
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
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
            coach::coach_install_plugin,
            coach::coach_uninstall_plugin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MXB Coach");
}
