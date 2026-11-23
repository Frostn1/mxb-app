//! Studio's commands over `trackgen::trackplace`: the same functions, with the app's data
//! folder and webview events as their host.

use tauri::{AppHandle, Emitter};
use trackgen::trackplace::{self, *};

/// The app, as a place fetch's host.
struct Host(AppHandle);

impl PlaceHost for Host {
    fn data_dir(&self) -> Option<std::path::PathBuf> {
        mxb_core::config::data_dir(&self.0)
    }
    fn fetch_progress(&self, progress: FetchProgress) {
        let _ = self.0.emit(FETCH_EVENT, progress);
    }
}

#[tauri::command]
pub async fn place_find(query: String) -> Result<Vec<PlaceHit>, String> {
    trackplace::place_find(query).await
}

#[tauri::command]
pub async fn place_tracks_near(lat: f64, lon: f64) -> Result<Vec<PlaceHit>, String> {
    trackplace::place_tracks_near(lat, lon).await
}

#[tauri::command]
pub async fn place_coverage(lat: f64, lon: f64) -> Result<CoverageReport, String> {
    trackplace::place_coverage(lat, lon).await
}

#[tauri::command]
pub async fn place_fetch(app: AppHandle, name: String, lat: f64, lon: f64, plot_m: f64, source_id: String) -> Result<Place, String> {
    trackplace::place_fetch(&Host(app), name, lat, lon, plot_m, source_id).await
}

#[tauri::command]
pub async fn place_map(lat: f64, lon: f64, span_m: f64) -> Result<PlaceMap, String> {
    trackplace::place_map(lat, lon, span_m).await
}

#[tauri::command]
pub async fn place_list(app: AppHandle) -> Result<Vec<Place>, String> {
    trackplace::place_list(&Host(app)).await
}

#[tauri::command]
pub async fn place_forget(app: AppHandle, slug: String) -> Result<(), String> {
    trackplace::place_forget(&Host(app), slug).await
}

#[tauri::command]
pub async fn place_paths(app: AppHandle, slug: String) -> Result<PlacePaths, String> {
    trackplace::place_paths(&Host(app), slug).await
}

#[tauri::command]
pub async fn place_layers(app: AppHandle, slug: String) -> Result<PlaceLayers, String> {
    trackplace::place_layers(&Host(app), slug).await
}

#[tauri::command]
pub async fn place_save_trace(app: AppHandle, slug: String, points: Vec<Vec<f64>>, closed: bool, default_width_m: f64, start_index: usize) -> Result<String, String> {
    trackplace::place_save_trace(&Host(app), slug, points, closed, default_width_m, start_index).await
}

#[tauri::command]
pub async fn place_import_dem(app: AppHandle, name: String, path: String, licence: String, attribution: String) -> Result<Place, String> {
    trackplace::place_import_dem(&Host(app), name, path, licence, attribution).await
}

#[tauri::command]
pub async fn place_program(app: AppHandle, slug: String, recut: bool) -> Result<serde_json::Value, String> {
    trackplace::place_program(&Host(app), slug, recut).await
}
