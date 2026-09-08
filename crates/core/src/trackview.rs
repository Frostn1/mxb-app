//! Reading a track for the viewer.
//!
//! Everything here answers "what does this track look like" — its info block, its terrain,
//! its scenery, the sheets its ground is painted with — and nothing here writes anything.
//! It is in the shared crate because inspecting an installed track is something every
//! binary does: the manager shows a track in the Library, and the studio previews one it is
//! building.
//!
//! Also the `.pkz` metadata readers, for the same reason: the Library lists archives and the
//! studio reads its own output back.

use crate::{config, map, pkz, scenery, track};

#[tauri::command]
pub async fn get_pkz_meta(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_meta_blocking(app, path))
        .await
        .map_err(|e| format!("get_pkz_meta task failed: {e}"))?
}

fn get_pkz_meta_blocking(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    pkz::read_meta_cached(&app, &path).map_err(|e| format!("{e:#}"))
}

/// Metadata for many mods at once, but only for the ones already cached — `None` marks
/// an entry the caller still has to request individually.
///
/// The Library asks for this first so a known collection paints in a single round trip
/// instead of one request (and one archive read) per card.
#[tauri::command]
pub async fn get_pkz_meta_cached(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<Option<pkz::PkzMeta>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| pkz::read_meta_if_cached(&app, p))
            .collect()
    })
    .await
    .map_err(|e| format!("get_pkz_meta_cached task failed: {e}"))
}

#[tauri::command]
pub async fn get_pkz_preview(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_preview_blocking(path))
        .await
        .map_err(|e| format!("get_pkz_preview task failed: {e}"))?
}

fn get_pkz_preview_blocking(path: String) -> Result<Option<String>, String> {
    pkz::read_preview(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

/// A track's metadata and contents. Cheap by construction — nothing is inflated — so the
/// track view can paint everything except the terrain immediately.
#[tauri::command]
pub async fn read_track_info(app: tauri::AppHandle, path: String) -> Result<track::TrackInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        track::read_info(&app, &path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("read_track_info task failed: {e}"))?
}

/// A plain-text account of what a track's terrain looks like to the reader.
///
/// Shown in the viewer when a track's terrain won't load. The height format is undocumented,
/// so a track that fails is evidence we don't otherwise have — and the player holding it is
/// rarely the person who can rebuild the app to investigate.
#[tauri::command]
pub async fn diagnose_track(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || track::diagnose(std::path::Path::new(&path)))
        .await
        .map_err(|e| format!("diagnose_track task failed: {e}"))
}

/// A track's terrain grid, at no more than `max_dim` samples on its longest edge.
///
/// Returned as raw bytes rather than JSON: a grid is a few hundred thousand floats, and
/// serialising that as a JSON array costs more than reading it out of the archive did. The
/// app reads the header described in [`track::BLOB_HEADER`] and takes the rest in place.
#[tauri::command]
pub async fn load_track_terrain(
    app: tauri::AppHandle,
    path: String,
    max_dim: u32,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let master = track::load_master(&app, &path).map_err(|e| format!("{e:#}"))?;
        Ok(tauri::ipc::Response::new(track::terrain_blob(
            &master, max_dim,
        )))
    })
    .await
    .map_err(|e| format!("load_track_terrain task failed: {e}"))?
}

/// A picture of a track's surfaces, to lay over its terrain.
///
/// Empty — not an error — when the track's height file carries no coverage masks. That track
/// draws on its relief alone, which is what every track did before this existed, so there is
/// nothing here worth failing a view over.
#[tauri::command]
pub async fn load_track_overview(
    app: tauri::AppHandle,
    path: String,
    max_dim: u32,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let blob = track::overview_blob(&app, std::path::Path::new(&path), max_dim)
            .unwrap_or_default();
        tauri::ipc::Response::new(blob)
    })
    .await
    .map_err(|e| format!("load_track_overview task failed: {e}"))
}

/// Where a track pins the things it ships no mesh for — marshal posts, TV cameras, crowd
/// sound — plus the props its `.scr` places.
///
/// Split from the scenery mesh because it costs nothing: these files are kilobytes, so the
/// viewer can mark them while the `.map` is still being read out of the archive.
#[tauri::command]
pub async fn read_track_placements(path: String) -> Result<Vec<scenery::Placement>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scenery::read_placements(&path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("read_track_placements task failed: {e}"))?
}

/// A track's scenery mesh — what stands on the ground the terrain grid describes.
///
/// Raw bytes for the same reason the terrain is: this is a few hundred thousand triangles,
/// and as JSON numbers it would cost more to parse than the archive read that produced it.
/// Empty rather than an error when a track carries no scenery, which is ordinary — the OEM
/// drag strip declares none at all.
#[tauri::command]
pub async fn load_track_scenery(
    app: tauri::AppHandle,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || match scenery::load(&app, &path) {
        Ok(s) => tauri::ipc::Response::new(scenery::blob(&s)),
        Err(e) => {
            log::debug!("[scenery] {path}: {e:#}");
            tauri::ipc::Response::new(Vec::new())
        }
    })
    .await
    .map_err(|e| format!("load_track_scenery task failed: {e}"))
}

/// A track's surfaces, fetched after its mesh is already on screen.
///
/// The second half of a two-stage load: the mesh parses in milliseconds, while inflating a
/// map's sheets is hundreds of megabytes of work. Splitting them is the difference between a
/// track appearing at once and a second of empty canvas.
#[tauri::command]
pub async fn load_track_surfaces(
    app: tauri::AppHandle,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || match scenery::load_surfaces(&app, &path) {
        Ok(t) => tauri::ipc::Response::new(scenery::surfaces_blob(&t)),
        Err(e) => {
            log::debug!("[scenery] surfaces for {path}: {e:#}");
            tauri::ipc::Response::new(Vec::new())
        }
    })
    .await
    .map_err(|e| format!("load_track_surfaces task failed: {e}"))
}

/// What a track wraps itself in — its sky, its backdrop, and the light it sits under.
///
/// A dome is a few hundred triangles carrying one very large picture, so this is cheap next
/// to the scenery and is what stops a track ending at a hard edge with nothing beyond it.
#[tauri::command]
pub async fn load_track_backdrop(
    path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || match scenery::backdrop(&path) {
        Ok((amb, sky, back)) => tauri::ipc::Response::new(scenery::backdrop_blob(&amb, &sky, &back)),
        Err(e) => {
            log::debug!("[scenery] backdrop for {path}: {e:#}");
            tauri::ipc::Response::new(Vec::new())
        }
    })
    .await
    .map_err(|e| format!("load_track_backdrop task failed: {e}"))
}

/// A tiling sheet of a track's own ground, for detail finer than its data carries.
///
/// A track states its surface at about a third of a metre per sample, and a viewer that lets
/// you get close magnifies that into a blur. This puts the grain back.
#[tauri::command]
pub async fn load_track_ground(
    app: tauri::AppHandle,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sheets = scenery::load_ground(&app, &path).unwrap_or_default();
        tauri::ipc::Response::new(map::surfaces_blob(&sheets))
    })
    .await
    .map_err(|e| format!("load_track_ground task failed: {e}"))
}

/// The ground a track is painted with, layer by layer — what the game puts under the bike.
///
/// Separate from `load_track_ground`, which hands back a single sheet to tile everywhere. This
/// is the stack itself: each layer's sheet, how far it tiles, and the mask that cuts it into
/// the one below.
#[tauri::command]
pub async fn load_track_ground_layers(
    app: tauri::AppHandle,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let blob = scenery::load_ground_layers(&app, &path).unwrap_or_else(|e| {
            log::debug!("[scenery] ground layers for {path}: {e:#}");
            Vec::new()
        });
        tauri::ipc::Response::new(blob)
    })
    .await
    .map_err(|e| format!("load_track_ground_layers task failed: {e}"))
}
