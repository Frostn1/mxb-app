// Windows: no console window behind the app in a release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Frost's Studio — where MX Bikes content is made.
//!
//! Deliberately thin for now. Every command it answers comes from `mxb_core`, which is the
//! whole point of standing this up before the studio's own 26k lines move in: it proves two
//! binaries build off one core, and that a command registered by path across a crate
//! boundary actually reaches the webview.

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            mxb_core::viewer::app_platform,
            // Reading a track, and the archive metadata behind it. The studio previews the
            // track it is building with the same code the manager shows one with.
            mxb_core::trackview::read_track_info,
            mxb_core::trackview::diagnose_track,
            mxb_core::trackview::load_track_terrain,
            mxb_core::trackview::load_track_overview,
            mxb_core::trackview::load_track_scenery,
            mxb_core::trackview::load_track_surfaces,
            mxb_core::trackview::load_track_backdrop,
            mxb_core::trackview::load_track_ground,
            mxb_core::trackview::load_track_ground_layers,
            mxb_core::trackview::read_track_placements,
            mxb_core::trackview::get_pkz_meta,
            mxb_core::trackview::get_pkz_meta_cached,
            mxb_core::trackview::get_pkz_preview,
            // Drawing a bike, a rider and their gear — what the Designer's preview needs.
            mxb_core::viewer::load_bike_model,
            mxb_core::viewer::load_rider_model,
            mxb_core::viewer::load_rider_body_model,
            mxb_core::viewer::load_gear_model,
            mxb_core::viewer::load_stock_gear_model,
            mxb_core::viewer::list_gear_paints,
            mxb_core::viewer::list_installed_gear_paints,
            mxb_core::viewer::texture_bytes,
            mxb_core::viewer::unpack_pkz,
            mxb_core::viewer::watch_paint_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frost's Studio");
}
