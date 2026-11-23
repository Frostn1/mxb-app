//! The commands the frontend calls. Each is a thin wrapper over a plain function, so the
//! dev bridge (`bridge.rs`) answers exactly what the window would.

use std::path::Path;

use base64::Engine;
use mxb_core::{config, game};
use tauri::{AppHandle, State};

use crate::blender::{self, Shared};
use crate::export::{self, ExportRequest};
use crate::{sessions, settings};

fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = Result<T, String>> {
    async move { tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string()) }
}

#[tauri::command]
pub fn get_config(app: AppHandle) -> config::AppConfig {
    config::load(&app).unwrap_or_default()
}

#[tauri::command]
pub fn list_games() -> Vec<game::GameInfo> {
    game::all_info()
}

#[tauri::command]
pub fn log_client(level: String, message: String) {
    mxb_core::clientlog::record(&level, &message);
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    mxb_core::library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    let _ = std::fs::create_dir_all(&path);
    mxb_core::library::open_folder(&path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn steam_link_start(app: AppHandle) -> Result<String, String> {
    mxb_core::appgate::steam_link_start(&app).await
}

#[tauri::command]
pub async fn steam_link_status(app: AppHandle) -> Result<Option<String>, String> {
    mxb_core::appgate::steam_link_status(&app).await
}

#[tauri::command]
pub async fn recheck_gate(app: AppHandle) {
    mxb_core::appgate::check(app).await;
}

#[tauri::command]
pub async fn gate_verdict(app: AppHandle) {
    mxb_core::appgate::replay_verdict(app).await;
}

/// The bike a ride was on, as the game draws it: the archive with the bike folder's loose
/// files over it — a model swap included, since that is the bike in the shot.
pub fn bike_model(app: &AppHandle, bike: &str, tyres: Option<String>) -> Result<mxb_core::viewer::BikeModel, String> {
    let cfg = config::load(app).map_err(|e| format!("{e:#}"))?;
    let root = mxb_core::library::mods_subdir(cfg.mods_path.trim(), "mods/bikes").join(bike);
    mxb_core::viewer::load_bike_model_blocking(root.to_string_lossy().into_owned(), tyres)
}

#[tauri::command]
pub async fn preview_model_swap(
    app: AppHandle,
    bike: String,
    _variant: String,
    tyres: Option<String>,
) -> Result<mxb_core::viewer::BikeModel, String> {
    blocking(move || bike_model(&app, &bike, tyres)).await?
}

/// The rider's body for the preview: the mesh and its rig, wearing what the mesh carries.
pub fn rider_body(app: &AppHandle, profile: &str) -> Result<Vec<mxb_core::viewer::RiderPart>, String> {
    use mxb_core::viewer;
    let cfg = config::load(app).map_err(|e| format!("{e:#}"))?;
    let name = viewer::rider_profile_or_stock(profile).to_string();
    Ok(viewer::load_rider_body(&cfg, &name, Vec::new()).map(|b| vec![b]).unwrap_or_default())
}

/// Named as the shared API calls it; Coach registers the same command.
#[tauri::command]
pub async fn coach_rider_body(app: AppHandle, profile: String) -> Result<Vec<mxb_core::viewer::RiderPart>, String> {
    blocking(move || rider_body(&app, &profile)).await?
}

#[tauri::command]
pub async fn cinema_sessions(app: AppHandle) -> Result<Vec<sessions::SessionSummary>, String> {
    blocking(move || sessions::list(&app)).await
}

#[tauri::command]
pub async fn cinema_session(path: String) -> Result<sessions::SessionData, String> {
    blocking(move || sessions::data(&path)).await?
}

#[tauri::command]
pub async fn cinema_ground(app: AppHandle, path: String) -> Result<sessions::GroundAnswer, String> {
    blocking(move || sessions::ground(&app, &path)).await?
}

#[tauri::command]
pub async fn cinema_settings(app: AppHandle) -> Result<settings::View, String> {
    blocking(move || settings::view(&app)).await
}

#[tauri::command]
pub async fn cinema_set_settings(app: AppHandle, settings: settings::Settings) -> Result<settings::View, String> {
    blocking(move || {
        settings::save(&app, &settings)?;
        Ok(settings::view(&app))
    })
    .await?
}

/// Whether a picked file is a Blender this app can use.
#[tauri::command]
pub fn cinema_check_blender(path: String) -> Option<blender::Install> {
    blender::install(Path::new(&path))
}

#[tauri::command]
pub async fn cinema_export(app: AppHandle, job: State<'_, Shared>, request: ExportRequest) -> Result<(), String> {
    let job = job.inner().clone();
    blocking(move || export::start(&app, job, request)).await?
}

#[tauri::command]
pub fn cinema_export_status(job: State<'_, Shared>) -> blender::Status {
    job.snapshot()
}

#[tauri::command]
pub fn cinema_export_cancel(job: State<'_, Shared>) {
    job.cancel();
}

/// Opens a shot in Blender's own window: the found blender.exe with the .blend, detached, two
/// threads, the GPU set up first. Not while MX Bikes runs. Never waits on the child.
pub fn open_blend(app: &AppHandle, path: &str) -> Result<(), String> {
    if !Path::new(path).is_file() {
        return Err("noBlend".into());
    }
    let s = settings::load(app);
    let exe = settings::blender_exe(app, &s).ok_or("noBlender")?;
    let gpu = export::scripts_dir(app).join("gpu.py");
    let mut cmd = std::process::Command::new(&exe);
    // Each argument passed on its own: paths with spaces arrive whole.
    cmd.arg(path).args(["-t", "2", "-P"]).arg(&gpu).env("CINEMA_NEED_GPU", "0");
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // below normal, its own process group, detached from this app
        cmd.creation_flags(0x0000_4000 | 0x0000_0200 | 0x0000_0008);
    }
    log::info!("[cinema] open in Blender: {exe:?} {path:?} -t 2 -P {gpu:?}");
    cmd.spawn().map(drop).map_err(|e| format!("blenderStart: {e}"))
}

#[tauri::command]
pub async fn cinema_open_blend(app: AppHandle, path: String) -> Result<(), String> {
    blocking(move || open_blend(&app, &path)).await?
}

/// Explorer with the shot's .blend selected.
#[tauri::command]
pub async fn cinema_reveal(path: String) -> Result<(), String> {
    log::info!("[cinema] show in folder: {path:?}");
    blocking(move || mxb_core::library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))).await?
}

/// A rendered still, as a data URL for the result card.
pub fn read_image(path: &str) -> Result<String, String> {
    if !path.to_ascii_lowercase().ends_with(".png") {
        return Err("notPng".into());
    }
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)))
}

#[tauri::command]
pub async fn cinema_read_image(path: String) -> Result<String, String> {
    blocking(move || read_image(&path)).await?
}
