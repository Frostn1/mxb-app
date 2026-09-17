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
mod cues;
mod fixes;
mod ground;
mod hud;
mod hudsheet;
mod imports;
mod ini;
mod lines;
mod others;
mod overlay;
mod sag;
mod soil;
mod stp;
mod surface;
mod telemetry;
mod tyres;

use mxb_core::{config, game, usage};
use tauri::{Manager, WindowEvent};

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
    mxb_core::clientlog::record(&level, &message);
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

/// Opens a secured (`.mxbsecure`) track the rider has unlocked, in memory, the way MXB App does:
/// the key beside it is unsealed for the Steam account signed in now. Anyone else's stays locked.
#[cfg(mxbsecure)]
fn register_secure_opener() {
    use mxb_core::{mxbsecure, securesource, steamid};
    securesource::set_opener(Box::new(|blob_path: &std::path::Path| {
        let steam_id = steamid::current_steam_id64()?;
        let sealed = std::fs::read(securesource::existing_key_path(blob_path.to_str()?)?).ok()?;
        let key = mxbsecure::unseal_key(&sealed, &steam_id, "")?;
        mxbsecure::open(&std::fs::read(blob_path).ok()?, &key).ok()
    }));
    securesource::set_unlocked_check(Box::new(|blob_path: &std::path::Path| {
        (|| {
            let steam_id = steamid::current_steam_id64()?;
            let sealed = std::fs::read(securesource::existing_key_path(blob_path.to_str()?)?).ok()?;
            mxbsecure::unseal_key(&sealed, &steam_id, "")
        })()
        .is_some()
    }));
}

/// The Steam-link round trip and the gate re-check the sign-in wall drives. Thin wrappers over
/// the shared `mxb_core::appgate`, so Coach's wall behaves exactly like MXB App's and Studio's.
#[tauri::command]
async fn steam_link_start(app: tauri::AppHandle) -> Result<String, String> {
    mxb_core::appgate::steam_link_start(&app).await
}

#[tauri::command]
async fn steam_link_status(app: tauri::AppHandle) -> Result<Option<String>, String> {
    mxb_core::appgate::steam_link_status(&app).await
}

#[tauri::command]
async fn recheck_gate(app: tauri::AppHandle) {
    mxb_core::appgate::check(app).await;
}

/// Re-send the startup gate's verdict, for a frontend that mounted after it was emitted.
///
/// The gate runs from `setup`, before this webview exists, and a Tauri event reaches only the
/// listeners attached when it fires. The sign-in wall asks for the verdict on mount so a slow
/// cold start cannot leave it never knowing one was reached.
#[tauri::command]
async fn gate_verdict(app: tauri::AppHandle) {
    mxb_core::appgate::replay_verdict(app).await;
}

fn main() {
    // Refuse to run under a debugger in release builds — the runtime half of the binary
    // hardening, shared by the whole lineup from `mxb_core`.
    mxb_core::antidebug::guard();
    #[cfg(mxbsecure)]
    register_secure_opener();
    let builder = tauri::Builder::default();
    // A second launch shows the window already running rather than starting another Coach
    // (and another overlay key). Release only, so a dev run starts beside the installed one.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        overlay::show_main(app);
    }));
    builder
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The overlay's hotkey has to fire while MX Bikes holds keyboard focus.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // The sessions folders, watched while the app is open: the recorder writes all through
        // a stint, and the list used to need the rider to leave the page and come back.
        .manage(coach::SessionWatch::default())
        .setup(|app| {
            // The estate gate, first: refuse a blocked install now (offline-proof), and ask the
            // server afresh in the background — the same lock as MXB App and Studio, one core.
            mxb_core::appgate::enforce_marker(app.handle());
            tauri::async_runtime::spawn(mxb_core::appgate::check(app.handle().clone()));
            overlay::start(app.handle());
            coach::watch_sessions(app.handle());
            // Anonymous counters, under the same switch and the same config file as the manager's
            // — which is also where the install id comes from. Coach does not mint one (no
            // `mint-install-id` feature, exactly as the studio), so a machine with only Coach on
            // it reports nothing rather than inventing a second identity for one computer.
            //
            // Until this existed Coach was a shipped app the numbers could not see at all: every
            // decision about whether to keep building it was being made from the one source the
            // rollups were meant to replace.
            usage::start(app.handle(), usage::COACH);
            // The survey prompt. Started beside the counters and gated on the same consent:
            // it answers the question counters cannot, which is whether any of this is any
            // good. Nothing is fetched or asked when either switch is off.
            mxb_core::survey::start(app.handle(), usage::COACH);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != overlay::LABEL {
                return;
            }
            match event {
                // Clicking back into the game puts the overlay away.
                WindowEvent::Focused(false) => overlay::on_focus_lost(window.app_handle()),
                // Closing it parks it, so the next press doesn't rebuild the webview.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = mxb_core::overlay::hide(window.app_handle());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // What the shared shell calls: the platform, the config, the titles.
            mxb_core::viewer::app_platform,
            get_config,
            list_games,
            steam_link_start,
            steam_link_status,
            recheck_gate,
            gate_verdict,
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
            coach::coach_select_setup,
            coach::coach_write_cues,
            // The trainer laps: another rider's recordings, kept apart from the rider's own.
            imports::coach_imports,
            imports::coach_import_laps,
            imports::coach_remove_import,
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
            coach::coach_refresh_plugin,
            coach::coach_set_game_dir,
            coach::coach_uninstall_plugin,
            hud::coach_hud,
            hud::coach_set_hud,
            hud::coach_set_cue_pos,
            hud::coach_voice,
            hud::coach_set_voice,
            overlay::overlay_toggle,
            overlay::overlay_hide,
            overlay::overlay_state,
            overlay::set_overlay_enabled,
            overlay::set_overlay_hotkey,
            overlay::overlay_open_main,
            overlay::overlay_handoff,
            overlay::overlay_peer,
            track_event,
            mxb_core::survey::survey_due,
            mxb_core::survey::survey_shown,
            mxb_core::survey::survey_answer,
            mxb_core::survey::survey_dismiss,
            mxb_core::survey::set_survey_enabled,
        ])
        .build(tauri::generate_context!())
        .expect("error while running MXB Coach")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                overlay::stop(app);
                // The last counters, on the way out. This is the right exit point rather than a
                // `Destroyed` window event: the window handler above is the overlay's, and the
                // overlay parking itself is not this app finishing.
                usage::flush_on_exit(app);
            }
        });
}

/// Count something the rider did.
///
/// A name and nothing else, exactly as in the manager and the studio: the backend holds the
/// switch, the buffer and the vocabulary (`usage::KNOWN_EVENTS`), so there is no payload here to
/// accidentally put a session path or a rider name into.
#[tauri::command]
fn track_event(name: String) {
    usage::track(&name);
}
