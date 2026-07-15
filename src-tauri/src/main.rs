// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod frostmod;
mod frostmod_manage;
mod install;
mod library;
mod mods;
mod pkz;
mod shop_session;

use config::AppConfig;
use frostmod::ReloadOutcome;
use frostmod_manage::{FrostmodProcess, FrostmodStatus};
use library::InstalledMod;
use mods::mxb::MxbModsSource;
use mods::{ModDetail, ModSource, ModSummary};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

#[tauri::command]
fn is_configured(app: tauri::AppHandle) -> bool {
    config::exists(&app)
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> AppConfig {
    config::load(&app).unwrap_or_default()
}

#[tauri::command]
fn create_config(app: tauri::AppHandle, config: AppConfig) -> Result<bool, String> {
    let cfg = config::finalize(config);
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    Ok(true)
}

#[tauri::command]
async fn search_mods(
    query: String,
    category_id: u32,
    page: u32,
) -> Result<Vec<ModSummary>, String> {
    MxbModsSource
        .search(&query, category_id, page)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn get_mod_detail(slug: String) -> Result<ModDetail, String> {
    MxbModsSource.detail(&slug).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn get_installed_mods(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<InstalledMod>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    library::scan_mods(&cfg.mods_path, &subpath).map_err(|e| format!("{e:#}"))
}

/// Installed rider models (helmet/boot/protection folders) + rider profiles, used
/// to build install destinations for rider paints and per-profile kit/gloves.
#[tauri::command]
fn scan_rider_targets(app: tauri::AppHandle) -> Result<library::RiderTargets, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(library::scan_rider_targets(&cfg.mods_path))
}

/// Read the structure of one installed `.pkz` (name/author/length/preview) for
/// its library card. Plain-zip archives are parsed; GUID-locked ones report
/// `locked`. Called lazily per card and cached on disk.
#[tauri::command]
fn get_pkz_meta(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    pkz::read_meta_cached(&app, &path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn add_to_library(
    app: tauri::AppHandle,
    slug: String,
    url: String,
    host: String,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    install::add_to_library(&app, &cfg, &slug, &url, &host, &subpath, &dest_folder)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn import_file(
    app: tauri::AppHandle,
    path: String,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    install::import_file(&app, &cfg, &path, &subpath, &dest_folder).map_err(|e| format!("{e:#}"))
}

/// Move an installed mod file into a different folder under its type dir.
#[tauri::command]
fn move_mod(
    app: tauri::AppHandle,
    from_path: String,
    to_folder: String,
    subpath: String,
) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    library::move_mod(&cfg.mods_path, &from_path, &to_folder, &subpath)
        .map_err(|e| format!("{e:#}"))
}

/// Move an installed mod file to the OS Recycle Bin / Trash.
#[tauri::command]
fn uninstall_mod(app: tauri::AppHandle, from_path: String, subpath: String) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    library::uninstall_mod(&cfg.mods_path, &from_path, &subpath).map_err(|e| format!("{e:#}"))
}

/// Reveal an installed mod file in the OS file manager.
#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

/// Toggle "keep running in the background" (close hides to the tray).
#[tauri::command]
fn set_run_in_background(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.run_in_background = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Toggle launch-at-login and persist the preference.
#[tauri::command]
fn set_launch_at_startup(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.launch_at_startup = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())
}

/// Focus (and un-hide) the main window — used by the tray.
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Ask a running FrostMod to live-reload the mods folder (manual button).
#[tauri::command]
fn frostmod_reload() -> ReloadOutcome {
    frostmod::signal_reload()
}

/// Is FrostMod currently running? (drives the status indicator)
#[tauri::command]
fn frostmod_running() -> bool {
    frostmod::is_running()
}

/// Install/version/running snapshot for the FrostMod settings panel.
#[tauri::command]
async fn frostmod_status(app: tauri::AppHandle) -> FrostmodStatus {
    frostmod_manage::status(&app).await
}

/// Download (or update to) the latest FrostMod release. Returns the version tag.
#[tauri::command]
async fn frostmod_install(app: tauri::AppHandle) -> Result<String, String> {
    frostmod_manage::install(&app).await.map_err(|e| format!("{e:#}"))
}

/// Launch the managed FrostMod process if it isn't already running.
#[tauri::command]
fn frostmod_start(app: tauri::AppHandle, state: State<FrostmodProcess>) -> Result<bool, String> {
    frostmod_manage::start(&app, &state).map_err(|e| format!("{e:#}"))
}

/// Stop the managed FrostMod process.
#[tauri::command]
fn frostmod_stop(state: State<FrostmodProcess>) {
    frostmod_manage::stop(&state);
}

/// Toggle auto-running FrostMod when the app opens.
#[tauri::command]
fn set_auto_run_frostmod(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.auto_run_frostmod = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

// --- MX Bikes Shop (paid, authenticated downloads) -------------------------

/// Open the shop sign-in page in a real WebView window and, once the user is
/// logged in, capture the session cookies and emit `shop-auth`. We never see the
/// password — the login happens on the actual site.
#[tauri::command]
async fn shop_login(app: tauri::AppHandle) -> Result<(), String> {
    // Re-focus an existing login window if the user clicks again.
    if let Some(w) = app.get_webview_window("shop-login") {
        let _ = w.set_focus();
        return Ok(());
    }

    let url = tauri::WebviewUrl::External(
        format!("{}/all-my-downloads/", shop_session::SHOP_BASE)
            .parse()
            .map_err(|e| format!("{e}"))?,
    );
    let window = tauri::WebviewWindowBuilder::new(&app, "shop-login", url)
        .title("Sign in to MX Bikes Shop")
        .user_agent(shop_session::UA)
        .inner_size(520.0, 760.0)
        .build()
        .map_err(|e| format!("{e:#}"))?;
    let _ = window;

    // Poll for the WordPress session cookie, then capture + persist and close.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // ~5 minutes at 500ms intervals, then give up (user can retry).
        for _ in 0..600u32 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(win) = app.get_webview_window("shop-login") else {
                break; // user closed the window before finishing
            };
            let cookies = shop_session::cookies_from_window(&win);
            if shop_session::is_authenticated(&cookies) {
                let ok = match shop_session::set_session(&app, cookies) {
                    Ok(()) => {
                        log::info!("captured MX Bikes Shop session");
                        true
                    }
                    Err(e) => {
                        log::error!("failed to save shop session: {e:#}");
                        false
                    }
                };
                let _ = app.emit("shop-auth", ok);
                let _ = win.close();
                break;
            }
        }
    });
    Ok(())
}

/// Whether we currently hold a shop session.
#[tauri::command]
fn shop_status(state: State<shop_session::ShopSession>) -> bool {
    state.logged_in()
}

/// Sign out of the shop (drop + delete the stored session).
#[tauri::command]
fn shop_logout(app: tauri::AppHandle) {
    shop_session::clear_session(&app);
}

/// List the signed-in user's purchased downloads ("All My Downloads").
#[tauri::command]
async fn shop_my_downloads(
    app: tauri::AppHandle,
    state: State<'_, shop_session::ShopSession>,
) -> Result<Vec<mods::mxbshop::ShopItem>, String> {
    let client = state
        .client()
        .ok_or_else(|| "Not signed in to MX Bikes Shop.".to_string())?;
    mods::mxbshop::fetch_my_downloads(&app, &client)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Download + install a purchased shop item through the shared install pipeline.
#[tauri::command]
async fn shop_install(
    app: tauri::AppHandle,
    state: State<'_, shop_session::ShopSession>,
    item: mods::mxbshop::ShopItem,
    dest_folder: String,
) -> Result<(), String> {
    let client = state
        .client()
        .ok_or_else(|| "Not signed in to MX Bikes Shop.".to_string())?;
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    install::download_and_place(
        &app,
        &cfg,
        &client,
        &item.slug,
        &item.download_url,
        "mods/tracks",
        &dest_folder,
    )
    .await
    .map_err(|e| format!("{e:#}"))
}

fn main() {
    tauri::Builder::default()
        .plugin(
            // File log in the app log dir + stdout in dev. Rotates when large,
            // keeping the newest file (see the log dir printed at startup).
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: None,
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(FrostmodProcess::default())
        .manage(shop_session::ShopSession::default())
        .setup(|app| {
            // Record where app state lands so the log itself answers "where?".
            log::info!("MXB App {} starting", env!("CARGO_PKG_VERSION"));
            if let Ok(dir) = app.path().app_local_data_dir() {
                log::info!("data dir (config/session/frostmod): {}", dir.display());
            }
            if let Ok(dir) = app.path().app_log_dir() {
                log::info!("log dir: {}", dir.display());
            }

            // System-tray icon so the app can keep running when the window closes.
            let show = MenuItem::with_id(app, "show", "Show MXB App", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("MXB App")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => {
                        // Stop the FrostMod process we started before exiting.
                        frostmod_manage::stop(&app.state::<FrostmodProcess>());
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            // Apply saved preferences on boot (both default ON).
            let handle = app.handle();
            if config::exists(handle) {
                if let Ok(cfg) = config::load(handle) {
                    // Launch-at-login: sync the OS entry to the pref.
                    let manager = handle.autolaunch();
                    let enabled = manager.is_enabled().unwrap_or(false);
                    if cfg.launch_at_startup && !enabled {
                        let _ = manager.enable();
                    } else if !cfg.launch_at_startup && enabled {
                        let _ = manager.disable();
                    }
                    // Auto-run FrostMod so it's connected as soon as the app opens.
                    if cfg.auto_run_frostmod && frostmod_manage::is_installed(handle) {
                        let state = handle.state::<FrostmodProcess>();
                        let _ = frostmod_manage::start(handle, &state);
                    }
                }
            }
            // Restore a saved MX Bikes Shop session, if any.
            shop_session::load_session(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close hides to the tray (keeps FrostMod connected) unless the user
            // turned background mode off. A real quit goes through the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let cfg = config::load(window.app_handle()).unwrap_or_default();
                if cfg.run_in_background {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            is_configured,
            get_config,
            create_config,
            search_mods,
            get_mod_detail,
            get_installed_mods,
            get_pkz_meta,
            scan_rider_targets,
            add_to_library,
            import_file,
            move_mod,
            uninstall_mod,
            reveal_in_explorer,
            set_run_in_background,
            set_launch_at_startup,
            set_auto_run_frostmod,
            frostmod_reload,
            frostmod_running,
            frostmod_status,
            frostmod_install,
            frostmod_start,
            frostmod_stop,
            shop_login,
            shop_status,
            shop_logout,
            shop_my_downloads,
            shop_install
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
