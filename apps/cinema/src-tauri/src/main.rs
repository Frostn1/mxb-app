// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! MXB Cinema: ride, find the moment, make it a shot in Blender.
//!
//! Pick a ride (an MXB Coach recording today, a game replay once that format reads), scrub to
//! the moment on a light 3D preview built from the same viewer Coach replays corners in, mark
//! a frame or a range, and export it: the slice as replay JSON, then one headless Blender run
//! that builds a ready-to-open `.blend` with the track, the bike, the rider in their own kit,
//! cameras, effects and light, and renders a still or a clip when asked.

mod blender;
#[cfg(feature = "bridge")]
mod bridge;
mod clip;
mod commands;
mod export;
mod mxbc;
mod prep;
mod sessions;
mod settings;

use std::sync::Arc;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn main() {
    // Refuse to run under a debugger in release builds — the lineup's shared hardening.
    mxb_core::antidebug::guard();
    #[cfg(feature = "bridge")]
    let bridge_port: Option<u16> = std::env::var("CINEMA_BRIDGE_PORT").ok().and_then(|p| p.parse().ok());
    #[cfg(not(feature = "bridge"))]
    let bridge_port: Option<u16> = None;

    let builder = tauri::Builder::default();
    // A second launch shows the window already running. Release only, so a dev run starts
    // beside an installed one.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage::<blender::Shared>(Arc::new(blender::Job::default()))
        .setup(move |app| {
            // The estate gate, the same lock as every other app in the lineup.
            mxb_core::appgate::enforce_marker(app.handle());
            tauri::async_runtime::spawn(mxb_core::appgate::check(app.handle().clone()));
            mxb_core::appgate::watch(app.handle());
            match bridge_port {
                #[cfg(feature = "bridge")]
                Some(port) => bridge::start(app.handle().clone(), port),
                _ => {
                    WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                        .title("MXB Cinema")
                        .inner_size(1360.0, 860.0)
                        .min_inner_size(1060.0, 700.0)
                        .decorations(false)
                        .background_color(tauri::window::Color(0x12, 0x14, 0x17, 0xff))
                        .build()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::Destroyed = event {
                    // Closing the window is closing Cinema, and a running Blender with it.
                    if let Some(job) = window.app_handle().try_state::<blender::Shared>() {
                        job.cancel();
                    }
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // What the shared shell and the 3D viewer call.
            mxb_core::viewer::app_platform,
            mxb_core::viewer::texture_bytes,
            commands::get_config,
            commands::list_games,
            commands::log_client,
            commands::reveal_in_explorer,
            commands::open_folder,
            commands::steam_link_start,
            commands::steam_link_status,
            commands::recheck_gate,
            commands::gate_verdict,
            commands::preview_model_swap,
            commands::coach_rider_body,
            mxb_core::trackview::read_track_info,
            mxb_core::trackview::load_track_terrain,
            mxb_core::trackview::load_track_overview,
            mxb_core::trackview::load_track_scenery,
            mxb_core::trackview::load_track_surfaces,
            mxb_core::trackview::load_track_backdrop,
            mxb_core::trackview::load_track_ground,
            mxb_core::trackview::load_track_ground_layers,
            mxb_core::trackview::read_track_placements,
            // The theme reads the supporter pack's cached answer (the manager keeps it fresh).
            mxb_core::supporter::supporter_status,
            mxb_core::supporter::supporter_refresh,
            // ── Cinema ─────────────────────────────────────────────────────────────
            commands::cinema_sessions,
            commands::cinema_session,
            commands::cinema_ground,
            commands::cinema_settings,
            commands::cinema_set_settings,
            commands::cinema_check_blender,
            commands::cinema_export,
            commands::cinema_export_status,
            commands::cinema_export_cancel,
            commands::cinema_open_blend,
            commands::cinema_reveal,
            commands::cinema_read_image,
        ])
        .build(tauri::generate_context!())
        .expect("error while running MXB Cinema")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(job) = app.try_state::<blender::Shared>() {
                    job.cancel();
                }
            }
        });
}
