// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bikefiles;
mod bikeswap;
mod bundle;
mod cfg;
mod config;
mod cookie_session;
mod edf;
mod frostmod;
mod frostmod_manage;
mod gameproc;
mod install;
mod library;
mod modelswap;
mod mods;
mod modwatch;
mod mxb_session;
mod overlay;
mod paint;
mod pkz;
#[cfg(sidecar)]
mod sidecar;
mod presets;
mod shop_session;
mod soundmods;
mod upload;

use config::AppConfig;
use frostmod::ReloadOutcome;
use frostmod_manage::{FrostmodProcess, FrostmodStatus};
use library::InstalledMod;
use modwatch::ModWatcher;
use mods::mxb::MxbModsSource;
use mods::{ModDetail, ModRating, ModSort, ModSource, ModSummary};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Whether the app is ready to use. Falls back to auto-detection when the config file
/// is missing, so the setup screen only appears when the MX Bikes folder genuinely
/// can't be found — not every time the saved config goes astray.
#[tauri::command]
fn is_configured(app: tauri::AppHandle) -> bool {
    config::load_or_detect(&app).is_some()
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> AppConfig {
    config::load(&app).unwrap_or_default()
}

#[tauri::command]
fn create_config(
    app: tauri::AppHandle,
    watcher: State<ModWatcher>,
    config: AppConfig,
) -> Result<bool, String> {
    let mut cfg = config::finalize(config);
    // Setup only sends the folders, so carry over first-run state from any config
    // that's already there — rewriting it would replay the intro and the tour.
    if let Ok(prev) = config::load(&app) {
        cfg.welcome_seen |= prev.welcome_seen;
        cfg.tour_done |= prev.tour_done;
    }
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Begin watching straight away so a fresh setup doesn't need a restart before
    // manual downloads reload the game.
    if cfg.watch_mods_reload {
        modwatch::start(&app, &watcher, &cfg.mods_path);
    }
    Ok(true)
}

/// Run an mxb-mods.com call; if Cloudflare refuses it, earn a `cf_clearance` in a real
/// browser and try exactly once more.
///
/// Once, not a loop: the handshake either produced a cookie the client didn't have or it
/// didn't, and repeating it would just reopen the window at someone who is already stuck.
/// Only refusals we could plausibly clear get this treatment — a 429 wants patience, not a
/// browser window.
async fn with_clearance<T, F, Fut>(app: &tauri::AppHandle, op: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let err = match op().await {
        Ok(value) => return Ok(value),
        Err(err) => err,
    };
    match err.downcast_ref::<mods::mxb::Blocked>() {
        Some(blocked) if blocked.clearable() => {}
        _ => return Err(format!("{err:#}")),
    }
    if !mxb_session::handshake(app).await {
        return Err(format!("{err:#}"));
    }
    op().await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn search_mods(
    app: tauri::AppHandle,
    query: String,
    category_id: u32,
    page: u32,
    sort: ModSort,
) -> Result<Vec<ModSummary>, String> {
    with_clearance(&app, || {
        MxbModsSource.search(&query, category_id, page, sort)
    })
    .await
}

/// Community scores for the mods currently on screen, keyed by post id. Ids the site
/// wouldn't answer for are left out rather than erroring — the cards just show no stars.
#[tauri::command]
async fn get_mod_ratings(ids: Vec<u64>) -> std::collections::HashMap<u64, ModRating> {
    mods::mxb::ratings(&ids).await
}

#[tauri::command]
async fn get_mod_detail(app: tauri::AppHandle, slug: String) -> Result<ModDetail, String> {
    with_clearance(&app, || MxbModsSource.detail(&slug)).await
}

#[tauri::command]
fn get_installed_mods(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<InstalledMod>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    library::scan_mods(&cfg.mods_path, &subpath).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn scan_library(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<library::LibraryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || scan_library_blocking(app, subpath))
        .await
        .map_err(|e| format!("scan_library task failed: {e}"))?
}

fn scan_library_blocking(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<library::LibraryEntry>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let sound_bikes = app
        .path()
        .app_local_data_dir()
        .map(|d| soundmods::known_bikes(&d))
        .unwrap_or_default();
    library::scan_library(&cfg.mods_path, &subpath, &sound_bikes).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn scan_rider_targets(app: tauri::AppHandle) -> Result<library::RiderTargets, String> {
    tauri::async_runtime::spawn_blocking(move || scan_rider_targets_blocking(app))
        .await
        .map_err(|e| format!("scan_rider_targets task failed: {e}"))?
}

fn scan_rider_targets_blocking(app: tauri::AppHandle) -> Result<library::RiderTargets, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(library::scan_rider_targets(&cfg.mods_path))
}

#[tauri::command]
async fn scan_model_swaps(app: tauri::AppHandle) -> Result<Vec<modelswap::BikeModels>, String> {
    tauri::async_runtime::spawn_blocking(move || scan_model_swaps_blocking(app))
        .await
        .map_err(|e| format!("scan_model_swaps task failed: {e}"))?
}

fn scan_model_swaps_blocking(app: tauri::AppHandle) -> Result<Vec<modelswap::BikeModels>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(modelswap::scan_model_swaps(&cfg.mods_path))
}

/// Outcome of a Locker model/sound swap — mirrors `PresetApplyOutcome` so the UI can
/// report the same "refreshed live in-game" feedback the presets flow gives.
#[derive(serde::Serialize)]
struct SwapApplyOutcome {
    content_reload: ReloadOutcome,
    game_running: bool,
    live_refresh: gameproc::LiveRefresh,
    /// Model swaps only (`None` for sound). `live_refresh` re-runs the *customization*
    /// loader, which reloads paints/gear but never the mesh — the model needs FrostMod
    /// to re-apply the bike. See `frostmod::signal_refresh_model`.
    model_refresh: Option<frostmod::CommandOutcome>,
}

/// Re-run the game's look loader live if instant refresh is enabled, else report it off.
fn live_refresh(enabled: bool) -> gameproc::LiveRefresh {
    if enabled {
        gameproc::refresh_look()
    } else {
        gameproc::LiveRefresh::Disabled
    }
}

/// Ask FrostMod to re-apply `bike` so a just-swapped model shows live. `None` when
/// instant refresh is off — the same switch that gates `live_refresh`, since both
/// reach into the running game.
fn model_refresh_cmd(enabled: bool, bike: &str) -> Option<frostmod::CommandOutcome> {
    enabled.then(|| frostmod::signal_refresh_model(bike))
}

#[tauri::command]
async fn apply_model_swap(
    app: tauri::AppHandle,
    bike: String,
    target: String,
) -> Result<SwapApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || apply_model_swap_blocking(app, bike, target))
        .await
        .map_err(|e| format!("apply_model_swap task failed: {e}"))?
}

fn apply_model_swap_blocking(
    app: tauri::AppHandle,
    bike: String,
    target: String,
) -> Result<SwapApplyOutcome, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let prev = modelswap::current_active(&cfg.mods_path, &bike);
    modelswap::apply_model_swap(&cfg.mods_path, &bike, &target).map_err(|e| format!("{e:#}"))?;
    // Make a bound sound travel with the model (case 2); independent sounds are left
    // untouched (case 1). Best-effort — the model swap itself already succeeded.
    if let Err(e) = soundmods::reconcile_after_model_swap(&cfg.mods_path, &bike, &prev, &target) {
        eprintln!("sound reconcile after model swap failed: {e:#}");
    }
    let content_reload = frostmod::signal_reload();
    // Ask FrostMod to re-apply the bike so the new model shows in the garage without a
    // class switch away-and-back. Only acts if `bike` is the selected one (decided
    // inside FrostMod, which is the only side that knows). Gated on the same
    // instant-refresh setting as the look refresh — both poke the live game.
    let model_refresh = model_refresh_cmd(cfg.instant_refresh, &bike);
    Ok(SwapApplyOutcome {
        content_reload,
        game_running: gameproc::is_game_running(),
        live_refresh: live_refresh(cfg.instant_refresh),
        model_refresh,
    })
}

#[tauri::command]
async fn scan_sound_swaps(app: tauri::AppHandle) -> Result<Vec<soundmods::BikeSounds>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(soundmods::scan_sound_swaps(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("scan_sound_swaps task failed: {e}"))?
}

#[tauri::command]
async fn apply_sound_swap(
    app: tauri::AppHandle,
    bike: String,
    target: String,
) -> Result<SwapApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        soundmods::apply_sound_swap(&cfg.mods_path, &bike, &target).map_err(|e| format!("{e:#}"))?;
        let content_reload = frostmod::signal_reload();
        Ok(SwapApplyOutcome {
            content_reload,
            game_running: gameproc::is_game_running(),
            live_refresh: live_refresh(cfg.instant_refresh),
            model_refresh: None, // a sound swap doesn't touch the model
        })
    })
    .await
    .map_err(|e| format!("apply_sound_swap task failed: {e}"))?
}

#[tauri::command]
async fn bind_sound(app: tauri::AppHandle, bike: String, model: String, sound: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        soundmods::bind_sound(&cfg.mods_path, &bike, &model, &sound).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("bind_sound task failed: {e}"))?
}

#[tauri::command]
async fn unbind_sound(app: tauri::AppHandle, bike: String, model: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        soundmods::unbind_sound(&cfg.mods_path, &bike, &model).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("unbind_sound task failed: {e}"))?
}

#[tauri::command]
async fn detect_loose_swaps(app: tauri::AppHandle) -> Result<Vec<modelswap::LooseSwapBike>, String> {
    tauri::async_runtime::spawn_blocking(move || detect_loose_swaps_blocking(app))
        .await
        .map_err(|e| format!("detect_loose_swaps task failed: {e}"))?
}

fn detect_loose_swaps_blocking(app: tauri::AppHandle) -> Result<Vec<modelswap::LooseSwapBike>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(modelswap::detect_loose_swaps(&cfg.mods_path))
}

#[tauri::command]
async fn register_loose_swaps(
    app: tauri::AppHandle,
    move_files: bool,
) -> Result<modelswap::RegisterReport, String> {
    tauri::async_runtime::spawn_blocking(move || register_loose_swaps_blocking(app, move_files))
        .await
        .map_err(|e| format!("register_loose_swaps task failed: {e}"))?
}

fn register_loose_swaps_blocking(
    app: tauri::AppHandle,
    move_files: bool,
) -> Result<modelswap::RegisterReport, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    modelswap::register_loose_swaps(&cfg.mods_path, move_files).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn detect_orphaned_setup(
    app: tauri::AppHandle,
) -> Result<Vec<modelswap::OrphanedSetup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(modelswap::detect_orphaned_setup(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("detect_orphaned_setup task failed: {e}"))?
}

#[tauri::command]
async fn repair_orphaned_setup(app: tauri::AppHandle, bike: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        modelswap::repair_orphaned_setup(&cfg.mods_path, &bike).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("repair_orphaned_setup task failed: {e}"))?
}

#[tauri::command]
async fn get_pkz_meta(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
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
async fn get_pkz_meta_cached(
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
async fn get_pkz_preview(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_preview_blocking(path))
        .await
        .map_err(|e| format!("get_pkz_preview task failed: {e}"))?
}

fn get_pkz_preview_blocking(path: String) -> Result<Option<String>, String> {
    pkz::read_preview(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn unpack_paint(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_paint_blocking(path))
        .await
        .map_err(|e| format!("unpack_paint task failed: {e}"))?
}

fn unpack_paint_blocking(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    paint::unpack_file(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn unpack_pkz(path: String, out_dir: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_pkz_blocking(path, out_dir))
        .await
        .map_err(|e| format!("unpack_pkz task failed: {e}"))?
}

fn unpack_pkz_blocking(path: String, out_dir: String) -> Result<Vec<String>, String> {
    pkz::extract(std::path::Path::new(&path), std::path::Path::new(&out_dir))
        .map_err(|e| format!("{e:#}"))
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikePaint {
    name: String,
    textures: Vec<paint::PaintTexture>,
    changes_preview: bool,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikeModel {
    nodes: Vec<edf::EdfNode>,
    paints: Vec<BikePaint>,
}

fn bike_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, BikeModel>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, BikeModel>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn bike_cache_key(source: &str) -> String {
    let mtime = std::fs::metadata(source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{source}:{mtime}")
}

#[tauri::command]
async fn load_bike_model(source: String) -> Result<BikeModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_bike_model_blocking(source))
        .await
        .map_err(|e| format!("load_bike_model task failed: {e}"))?
}

fn load_bike_model_blocking(source: String) -> Result<BikeModel, String> {
    use rayon::prelude::*;
    let t0 = std::time::Instant::now();
    let key = bike_cache_key(&source);
    if let Some(m) = bike_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        log::info!("load_bike_model {source}: cache hit ({:?})", t0.elapsed());
        return Ok(m);
    }

    let files = gather_bike_files(std::path::Path::new(&source)).map_err(|e| format!("{e:#}"))?;
    let installed = installed_paints(std::path::Path::new(&source));
    let t_read = t0.elapsed();

    let mut nodes = Vec::new();
    // Every mesh the bike ships, by file name — usually just `model.edf`, but a bike can
    // carry one per part. Which are actually used is decided by the `.hrc`s below.
    let mut edfs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut geom: Option<&Vec<u8>> = None;
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut tga_jobs: Vec<(String, &[u8])> = Vec::new();
    let mut pnt_jobs: Vec<(String, &[u8], bool)> = Vec::new();
    for (name, data) in &files {
        let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        if bn.ends_with(".edf") {
            edfs.insert(bn.clone(), data);
        } else if bn.ends_with(".geom") {
            geom = Some(data);
        } else if bn.ends_with("gfx.cfg") {
            gfx_bytes = Some(data);
        } else if let Some(stem) = bn.strip_suffix(".hrc") {
            let stem = stem.rsplit("__").next().unwrap_or(stem);
            hrcs.insert(stem.to_string(), data);
        } else if let Some(stem) = bn.strip_suffix(".tga") {
            // Lowercased stem — the frontend matches textures case-insensitively.
            tga_jobs.push((stem.to_string(), data.as_slice()));
        } else if bn.ends_with(".pnt") {
            pnt_jobs.push((paint_display_name(&bn), data.as_slice(), true));
        }
    }

    let gfx = gfx_bytes.map(|b| cfg::parse_gfx(b)).unwrap_or_default();
    // Group each part's level0 node under the mesh its `.hrc` names. Bikes that point
    // every part at one `model.edf` collapse to a single group — the original path.
    let mut scenes: Vec<(String, Vec<String>)> = Vec::new();
    let mut node_part: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Fixed part order — `gfx` is a map, and node order must not shuffle between runs.
    for part in cfg::GFX_PARTS {
        let Some(gp) = gfx.get(part) else { continue };
        let Some(hrc_file) = gp.hrc.as_deref() else { continue };
        let stem = hrc_file.trim_end_matches(".hrc").trim_end_matches(".HRC");
        let Some(bytes) = hrcs.get(&stem.to_ascii_lowercase()) else {
            log::warn!("[viewer] gfx.cfg part '{part}' wants {hrc_file}, which the bike doesn't ship");
            continue;
        };
        let hrc = cfg::parse(bytes);
        let Some(node) = cfg::hrc_level0(&hrc, stem) else { continue };
        let scene = cfg::hrc_level0_scene(&hrc)
            .map(|s| s.replace('\\', "/"))
            .and_then(|s| s.rsplit('/').next().map(str::to_ascii_lowercase))
            .unwrap_or_else(|| "model.edf".to_string());
        node_part.insert(node.to_ascii_lowercase(), part.to_string());
        match scenes.iter_mut().find(|(f, _)| *f == scene) {
            Some((_, level0)) => level0.push(node),
            None => scenes.push((scene, vec![node])),
        }
    }

    // Parse each referenced mesh and bind its textures against *its own* bytes: a
    // submesh's material index selects from that file's texture pool, so a part must
    // never be bound through another file's pool.
    let mut used: Vec<&Vec<u8>> = Vec::new();
    for (file, level0) in &scenes {
        let Some(data) = edfs.get(file) else {
            log::warn!("[viewer] an .hrc wants {file}, which the bike doesn't ship");
            continue;
        };
        let mut part_nodes = edf::parse_with_levels(data, level0);
        bind_textures(&mut part_nodes, data, &gfx, &node_part);
        nodes.append(&mut part_nodes);
        used.push(data);
    }
    // No gfx.cfg/.hrc to go on (or none of it resolved) — fall back to the bike's base
    // mesh and let the parser's own level0 heuristic pick the parts.
    if nodes.is_empty() {
        if let Some(data) = base_edf(&edfs) {
            nodes = edf::parse_with_levels(data, &[]);
            bind_textures(&mut nodes, data, &gfx, &node_part);
            used.push(data);
        }
    }
    for (fname, data) in &installed {
        pnt_jobs.push((paint_display_name(fname), data.as_slice(), false));
    }
    if let Some(g) = geom {
        if !edf::assemble_bike(&mut nodes, g) {
            eprintln!("[viewer] .geom present but missing mount points — parts unassembled");
        }
    } else if !nodes.is_empty() {
        eprintln!("[viewer] no .geom alongside the mesh — parts unassembled");
    }
    edf::to_right_handed(&mut nodes);
    let t_parse = t0.elapsed();

    let mut base: Vec<paint::PaintTexture> = tga_jobs
        .par_iter()
        .filter_map(|(stem, data)| paint::decode_image(stem, data))
        .collect();
    // Textures embedded in the meshes actually shown. Parts often share a name (each
    // file embeds the plastics it needs), so keep the first of each.
    let mut seen: std::collections::HashSet<String> =
        base.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    for data in &used {
        for tex in paint::extract_edf_textures(data) {
            if seen.insert(tex.name.to_ascii_lowercase()) {
                base.push(tex);
            }
        }
    }
    let mut paints: Vec<(BikePaint, bool)> = pnt_jobs
        .par_iter()
        .filter_map(|(name, data, shipped)| {
            paint::decode_any(data).ok().map(|pnt| {
                (
                    BikePaint {
                        name: name.clone(),
                        textures: pnt.par_iter().map(paint::to_texture).collect(),
                        changes_preview: false, // resolved below, once bindings are known
                    },
                    *shipped,
                )
            })
        })
        .collect();
    let base_count = base.len();
    let t_encode = t0.elapsed();

    let bound: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| {
            n.texture
                .iter()
                .chain(n.submeshes.iter().filter_map(|s| s.texture.as_ref()))
        })
        .map(|t| t.to_ascii_lowercase())
        .collect();
    for (p, shipped) in &mut paints {
        p.changes_preview = *shipped
            || (!bound.is_empty()
                && p.textures
                    .iter()
                    .any(|t| bound.contains(&t.name.to_ascii_lowercase())));
        if !p.changes_preview {
            log::info!(
                "[viewer] paint '{}' won't move the preview: it ships {:?}, and the parts shown bind {:?}",
                p.name,
                p.textures.iter().map(|t| &t.name).collect::<Vec<_>>(),
                bound,
            );
        }
    }
    let mut paints: Vec<BikePaint> = paints.into_iter().map(|(p, _)| p).collect();

    for p in &mut paints {
        let own: std::collections::HashSet<String> =
            p.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        p.textures.extend(
            base.iter()
                .filter(|t| !own.contains(&t.name.to_ascii_lowercase()))
                .cloned(),
        );
    }
    if paints.is_empty() {
        paints.push(BikePaint {
            name: "Stock".into(),
            textures: base,
            changes_preview: true, // the model's own textures, by definition
        });
    }

    log::info!(
        "load_bike_model {source}: {} paint(s) + {base_count} base tex | read {t_read:?}, parse {:?}, encode {:?}, total {:?}",
        paints.len(),
        t_parse - t_read,
        t_encode - t_parse,
        t0.elapsed(),
    );
    for p in &paints {
        let mut names: Vec<&str> = p.textures.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        log::info!("  paint '{}' textures: {}", p.name, names.join(", "));
    }
    for n in &nodes {
        let subs: Vec<String> = n
            .submeshes
            .iter()
            .map(|s| {
                format!(
                    "{}->{}{}",
                    s.name,
                    s.texture.as_deref().unwrap_or("(none)"),
                    match s.uv_tile {
                        Some(0) | None => String::new(),
                        Some(t) => format!("@tile{t}"),
                    }
                )
            })
            .collect();
        log::info!("  node '{}' placed={} {}", n.name, n.placed, subs.join(", "));
    }

    let model = BikeModel { nodes, paints };
    if let Ok(mut c) = bike_cache().lock() {
        if c.len() >= 6 {
            c.clear();
        }
        c.insert(key, model.clone());
    }
    Ok(model)
}

fn bind_textures(
    nodes: &mut [edf::EdfNode],
    edf_bytes: &[u8],
    gfx: &std::collections::HashMap<String, cfg::GfxPart>,
    node_part: &std::collections::HashMap<String, String>,
) {
    let embedded = edf::embedded_textures(edf_bytes);
    // The model's COLOUR textures, in file order — every embedded texture that isn't a
    // companion map (MX Bikes names those `_n` normal, `_s` specular, `_r` reflection).
    // The list keeps gfx-referenced textures (chain, w_plate): material indices count
    // them, so dropping any shifts every later material onto the wrong texture.
    let color: Vec<&edf::EmbeddedTexture> = embedded
        .iter()
        .filter(|t| {
            let n = t.name.to_ascii_lowercase();
            !n.ends_with("_n") && !n.ends_with("_s") && !n.ends_with("_r")
        })
        .collect();
    // Two readings of a material index compete: its position in the blob list above, and
    // whatever the header's material table maps it to. Neither is right everywhere — blob
    // order puts the number plate over the KX250's bodywork, the table puts the KTM 125
    // SX's cables on it — so the mesh itself breaks the tie below, per part.
    let slots = match std::env::var("MXB_MAT_TABLE").as_deref() {
        Ok("0") => Vec::new(),
        _ => edf::material_slots(edf_bytes, color.len()),
    };
    let disputed: Vec<usize> = (0..color.len())
        .filter(|i| matches!(slots.get(*i), Some(Some(j)) if j != i))
        .collect();
    // Only the disputed textures get inflated, and only on the bikes that disagree.
    let ink: std::collections::HashMap<usize, Vec<bool>> = disputed
        .iter()
        .flat_map(|i| [*i, slots[*i].unwrap_or(*i)])
        .collect::<std::collections::BTreeSet<usize>>()
        .into_iter()
        .filter_map(|slot| {
            let mask = color.get(slot).and_then(|t| edf::content_mask(edf_bytes, t))?;
            Some((slot, mask))
        })
        .collect();

    for n in nodes.iter_mut() {
        let part = node_part.get(&n.name.to_ascii_lowercase());
        let overrides = part.and_then(|p| gfx.get(p)).map(|p| &p.textures);
        // A part that draws on ONE material numbers it 0 whichever material that is, so
        // the index says nothing to look up — the YZ125 numbers its chassis' plastics 0
        // and, in the rear suspension, its metals 0 too. Only a part that distinguishes
        // materials at all is worth resolving.
        let spread: std::collections::HashSet<u32> =
            n.submeshes.iter().filter_map(|s| s.mat).collect();
        // Ask the geometry which reading it was drawn for: for every disputed material,
        // how much of what this part samples lands on texels the artist actually inked.
        // The parts of one bike need not agree — the YZ125's chassis reads through the
        // table while its steering reads straight off the blob list.
        let mut table_fit = 0f64;
        let mut blob_fit = 0f64;
        if spread.len() > 1 {
            for mat in spread.iter().map(|m| *m as usize).filter(|m| disputed.contains(m)) {
                let mut seen = vec![false; edf::FIT_RES * edf::FIT_RES];
                for sm in n.submeshes.iter().filter(|s| s.mat == Some(mat as u32)) {
                    for (dst, src) in seen
                        .iter_mut()
                        .zip(edf::uv_coverage(n, sm.tri_start, sm.tri_count))
                    {
                        *dst |= src;
                    }
                }
                let area = seen.iter().filter(|s| **s).count();
                if area == 0 {
                    continue;
                }
                // A blank overlay (`w_plate`, which the game composites numbers onto) has
                // no islands to land on and so scores nothing — it can only ever lose a
                // comparison, never win one.
                let landed = |slot: usize| {
                    ink.get(&slot).map_or(0.0, |m| {
                        seen.iter().zip(m).filter(|(s, i)| **s && **i).count() as f64
                    })
                };
                let Some(Some(via_table)) = slots.get(mat).copied() else { continue };
                table_fit += landed(via_table);
                blob_fit += landed(mat);
            }
        }
        let by_table = spread.len() > 1 && table_fit > blob_fit;
        if table_fit != blob_fit {
            log::info!(
                "[viewer] node '{}' reads its materials through the {} (fit {:.0} vs {:.0})",
                n.name,
                if by_table { "material table" } else { "texture order" },
                table_fit.max(blob_fit),
                table_fit.min(blob_fit),
            );
        }
        let texture_for = |mat: usize| -> Option<&edf::EmbeddedTexture> {
            match slots.get(mat).filter(|_| by_table) {
                Some(slot) => slot.and_then(|s| color.get(s)).copied(),
                None => color.get(mat).copied(),
            }
        };
        // A node with no submesh table is a single material — the first colour texture.
        n.texture = color.first().map(|t| t.name.clone());
        for sm in n.submeshes.iter_mut() {
            let group = sm.name.to_ascii_lowercase();
            // 1. An explicit gfx texture (animated chain, number plate) is authoritative.
            if let Some(tex) = overrides.and_then(|o| {
                o.get(&group)
                    .or_else(|| o.iter().find(|(g, _)| group.ends_with(&format!("_{g}"))).map(|(_, t)| t))
            }) {
                sm.texture = Some(tex.clone());
                continue;
            }
            // 2. The material index picks its colour texture, via the table where usable.
            if let Some(t) = sm.mat.and_then(|i| texture_for(i as usize)) {
                sm.texture = Some(t.name.clone());
                continue;
            }
            // 3. No material recorded → leave unbound so it renders neutral grey, never smeared.
            sm.texture = None;
        }
    }
}

fn paint_display_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit('/')
        .next()
        .unwrap_or(file_name)
        .trim_end_matches(".pnt")
        .trim_end_matches(".PNT");
    let mut chars = stem.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Stock".into(),
    }
}

fn installed_paints(source: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let folder = if source.is_dir() {
        source.to_path_buf()
    } else {
        source.with_extension("")
    };
    let paints_dir = folder.join("paints");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&paints_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pnt")) {
                if let (Some(name), Ok(bytes)) =
                    (p.file_name().and_then(|n| n.to_str()), std::fs::read(&p))
                {
                    out.push((name.to_string(), bytes));
                }
            }
        }
    }
    out
}

// Any `.edf`, not just `model.edf`: a bike may ship one mesh per part, named by its
// `.hrc` (see `scene_files_for_parts`). Shadow meshes ride along unused. Shared with
// `modelswap` so the swapper and the viewer classify the same files the same way.
use bikefiles::is_viewer_file as wanted_bike_file;

/// The bike's main mesh when the `.hrc`s can't say which it is: `model.edf` by
/// convention, else the shortest non-shadow name — a per-part set like `96cr250.edf` /
/// `96cr250_fs.edf` / `96cr250_s.edf` (shadow) reduces to the chassis.
fn base_edf<'a>(
    edfs: &std::collections::HashMap<String, &'a Vec<u8>>,
) -> Option<&'a Vec<u8>> {
    if let Some(data) = edfs.get("model.edf") {
        return Some(data);
    }
    edfs.iter()
        .filter(|(name, _)| !name.ends_with("_s.edf"))
        .min_by_key(|(name, _)| (name.len(), name.to_string()))
        .or_else(|| edfs.iter().min_by_key(|(name, _)| (name.len(), name.to_string())))
        .map(|(_, data)| *data)
}

fn gather_bike_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::{bail, Context};
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("edf")) {
        let bytes = std::fs::read(p).with_context(|| format!("read {p:?}"))?;
        return Ok(vec![("model.edf".to_string(), bytes)]);
    }
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz")) {
        return pkz::read_selected(p, wanted_bike_file);
    }
    if p.is_dir() {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).map(str::to_string);
            if path.is_file() && name.as_deref().is_some_and(wanted_bike_file) {
                if let (Some(name), Ok(bytes)) = (name, std::fs::read(&path)) {
                    out.push((name, bytes));
                }
            }
        }
        // A mesh of any name will do — `model.edf` is the convention, not a rule.
        if out.iter().any(|(n, _)| n.to_ascii_lowercase().ends_with(".edf")) {
            return Ok(out);
        }
        let sibling = p.with_extension("pkz");
        if sibling.exists() {
            return pkz::read_selected(&sibling, wanted_bike_file);
        }
        bail!("no .edf mesh for bike folder {p:?}");
    }
    bail!("can't load a bike model from {p:?}")
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderPart {
    part: String,
    nodes: Vec<edf::EdfNode>,
    textures: Vec<paint::PaintTexture>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderModel {
    parts: Vec<RiderPart>,
}

#[tauri::command]
async fn load_rider_model(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<RiderModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_rider_model_blocking(app, loadout))
        .await
        .map_err(|e| format!("load_rider_model task failed: {e}"))?
}

fn load_rider_model_blocking(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<RiderModel, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let base = std::path::Path::new(&cfg.mods_path)
        .join("mods")
        .join("rider");
    let mut parts = Vec::new();

    for spec in &GEAR {
        let (model, paint, goggles) = match spec.part {
            "helmet" => (
                loadout.helmet.as_str(),
                loadout.helmet_paint.as_str(),
                loadout.goggles_paint.as_str(),
            ),
            "boots" => (loadout.boots.as_str(), loadout.boots_paint.as_str(), ""),
            _ => (loadout.protection.as_str(), loadout.protection_paint.as_str(), ""),
        };
        if let Some(p) = load_gear(&cfg, &base, spec, model, paint, goggles) {
            parts.push(p);
        }
    }

    let suit = load_rider_paint(&base, "suit", &loadout.rider, "paints", &loadout.suit_paint);
    let gloves = load_rider_paint(&base, "gloves", &loadout.rider, "gloves", &loadout.gloves_paint);
    if !loadout.suit_paint.is_empty() && suit.is_none() {
        log::warn!("[rider] suit paint '{}' did not load for profile '{}'", loadout.suit_paint, loadout.rider);
    }
    if !loadout.gloves_paint.is_empty() && gloves.is_none() {
        log::warn!("[rider] glove paint '{}' did not load for profile '{}'", loadout.gloves_paint, loadout.rider);
    }
    let suit_texs = suit.as_ref().map(|s| s.textures.clone()).unwrap_or_default();
    let glove_texs = gloves.as_ref().map(|g| g.textures.clone()).unwrap_or_default();
    let mut body_texs = suit_texs;
    body_texs.extend(glove_texs);
    match load_rider_body(&cfg, &loadout.rider, body_texs) {
        Some(body) => parts.push(body),
        None => {
            if let Some(s) = suit {
                parts.push(s);
            }
            if let Some(g) = gloves {
                parts.push(g);
            }
        }
    }

    Ok(RiderModel { parts })
}

#[tauri::command]
async fn load_rider_body_model(
    app: tauri::AppHandle,
    profile: String,
) -> Result<Vec<edf::EdfNode>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(load_rider_body_nodes(&cfg, &profile).unwrap_or_default())
    })
    .await
    .map_err(|e| format!("load_rider_body_model task failed: {e}"))?
}

fn load_rider_body(
    cfg: &config::AppConfig,
    profile: &str,
    textures: Vec<paint::PaintTexture>,
) -> Option<RiderPart> {
    let mut nodes = load_rider_body_nodes(cfg, profile)?;
    tag_body_materials(&mut nodes);
    Some(RiderPart {
        part: "body".into(),
        nodes,
        textures,
    })
}

fn tag_body_materials(nodes: &mut [edf::EdfNode]) {
    for n in nodes.iter_mut() {
        for sm in n.submeshes.iter_mut() {
            sm.texture = Some(
                match sm.mat {
                    Some(1) => "gloves",
                    Some(2) => "face",
                    Some(3) | Some(4) => "hide",
                    _ => "rider",
                }
                .into(),
            );
        }
    }
}

fn pkz_mesh_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, Vec<edf::EdfNode>>>
{
    static C: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Vec<edf::EdfNode>>>,
    > = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn keep_lod0(nodes: &mut Vec<edf::EdfNode>) {
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| n.name.is_empty() || seen.insert(n.name.clone()));
}

fn load_pkz_mesh(pkz: &std::path::Path, entry: &str) -> Option<Vec<edf::EdfNode>> {
    let key = format!("{}:{}", bike_cache_key(&pkz.to_string_lossy()), entry);
    if let Some(n) = pkz_mesh_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(n);
    }
    let data = read_pkz_entry(pkz, entry)?;
    let mut nodes = edf::parse(&data);
    edf::to_right_handed(&mut nodes);
    keep_lod0(&mut nodes);
    if nodes.is_empty() {
        return None;
    }
    if let Ok(mut c) = pkz_mesh_cache().lock() {
        c.insert(key, nodes.clone());
    }
    Some(nodes)
}

fn load_rider_body_nodes(cfg: &config::AppConfig, profile: &str) -> Option<Vec<edf::EdfNode>> {
    let profile = if profile.is_empty() { "default_mx" } else { profile };
    let pkz = resolve_game_pkz(cfg, "rider.pkz")?;
    load_pkz_mesh(&pkz, &format!("rider/riders/{profile}/rider.edf"))
}

fn resolve_game_pkz(cfg: &config::AppConfig, name: &str) -> Option<std::path::PathBuf> {
    let gp = cfg.game_path.trim();
    if !gp.is_empty() {
        let p = std::path::Path::new(gp).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    let p = std::path::Path::new(&cfg.mods_path).join(name);
    if p.exists() {
        return Some(p);
    }
    // Last resort for configs that predate game-path auto-detection: scan Steam now.
    let detected = config::detect_game_path()?;
    let p = std::path::Path::new(&detected).join(name);
    p.exists().then_some(p)
}

fn read_pkz_entry(pkz: &std::path::Path, entry: &str) -> Option<Vec<u8>> {
    let matches = |name: &str| name.replace('\\', "/").eq_ignore_ascii_case(entry);
    if pkz::is_plain_zip(pkz) {
        let file = std::fs::File::open(pkz).ok()?;
        let mut zip = zip::ZipArchive::new(file).ok()?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).ok()?;
            if matches(f.name()) {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut buf).ok()?;
                return Some(buf);
            }
        }
        return None;
    }
    pkz::read_all(pkz)
        .ok()?
        .into_iter()
        .find(|(n, _)| matches(n))
        .map(|(_, d)| d)
}

#[tauri::command]
async fn load_gear_model(
    path: String,
    part: String,
    paint: Option<String>,
    goggles: Option<String>,
    // Show the mesh's own textures instead of a `.pnt` — the stock look. Separate flags
    // because a helmet's goggles are picked independently of its shell.
    stock: Option<bool>,
    stock_goggles: Option<bool>,
) -> Result<RiderPart, String> {
    tauri::async_runtime::spawn_blocking(move || {
        load_gear_model_blocking(
            path,
            part,
            paint,
            goggles,
            stock.unwrap_or(false),
            stock_goggles.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| format!("load_gear_model task failed: {e}"))?
}

#[tauri::command]
async fn load_stock_gear_model(
    app: tauri::AppHandle,
    part: String,
    paint_path: Option<String>,
) -> Result<RiderPart, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let spec = GEAR
            .iter()
            .find(|g| g.part == part)
            .ok_or_else(|| format!("no stock model for gear slot '{part}'"))?;
        let pkz = resolve_game_pkz(&cfg, "rider.pkz")
            .ok_or_else(|| "game path not set or rider.pkz not found".to_string())?;
        let folder = format!("rider/{}/{}", spec.pkz_kind, spec.default_name);
        let nodes = load_pkz_mesh(&pkz, &format!("{folder}/{}", spec.mesh))
            .ok_or_else(|| format!("stock {part} mesh not found in rider.pkz"))?;
        let textures = match paint_path.filter(|s| !s.is_empty()) {
            Some(p) => std::fs::read(&p)
                .ok()
                .and_then(|d| paint::decode_any(&d).ok())
                .map(|pnt| pnt.iter().map(paint::to_texture).collect())
                .unwrap_or_default(),
            None => load_pkz_paint(&pkz, &folder, ""),
        };
        Ok(RiderPart {
            part: spec.part.into(),
            nodes,
            textures,
        })
    })
    .await
    .map_err(|e| format!("load_stock_gear_model task failed: {e}"))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GearPaints {
    paints: Vec<String>,
    goggles: Vec<String>,
    /// The mesh carries its own shell / goggle texture, so a "Stock" entry is worth
    /// offering next to the packed paints. Preview-only — never a loadout value, since
    /// the game names a `.pnt` there and has no word for "the model's own look".
    has_stock: bool,
    has_stock_goggles: bool,
}

fn gear_paints_at(path: &std::path::Path) -> Result<GearPaints, String> {
    let files = read_gear_files(path).map_err(|e| format!("{e:#}"))?;
    let names = |folder: &str| {
        let mut out: Vec<String> = files
            .iter()
            .filter_map(|(n, _)| gear_folder_paint_name(n, folder))
            .collect();
        out.sort_by_key(|s| s.to_lowercase());
        out.dedup();
        out
    };
    // Names only — decoding the pixels is the load path's job, and this runs per picker.
    let embedded: Vec<String> = files
        .iter()
        .find(|(n, _)| is_visible_gear_mesh(n))
        .map(|(_, d)| edf::embedded_textures(d).iter().map(|t| t.name.clone()).collect())
        .unwrap_or_default();
    Ok(GearPaints {
        has_stock: embedded.iter().any(|n| !is_goggle_name(n) && !is_companion_map(n)),
        has_stock_goggles: embedded.iter().any(|n| is_goggle_name(n) && !is_companion_map(n)),
        paints: names("paints"),
        goggles: names("goggles"),
    })
}

#[tauri::command]
async fn list_gear_paints(path: String) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || gear_paints_at(std::path::Path::new(&path)))
        .await
        .map_err(|e| format!("list_gear_paints task failed: {e}"))?
}

#[tauri::command]
async fn list_installed_gear_paints(
    app: tauri::AppHandle,
    part: String,
    model: String,
) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let empty = GearPaints {
            paints: Vec::new(),
            goggles: Vec::new(),
            has_stock: false,
            has_stock_goggles: false,
        };
        if model.trim().is_empty() {
            return Ok(empty);
        }
        let Some(spec) = GEAR.iter().find(|g| g.part == part) else {
            return Ok(empty);
        };
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let kind_dir = std::path::Path::new(&cfg.mods_path)
            .join("mods")
            .join("rider")
            .join(spec.mods_kind);
        let stem = model.trim_end_matches(".pkz");
        for src in [kind_dir.join(stem), kind_dir.join(format!("{stem}.pkz"))] {
            if src.exists() {
                return gear_paints_at(&src);
            }
        }
        Ok(empty)
    })
    .await
    .map_err(|e| format!("list_installed_gear_paints task failed: {e}"))?
}

fn gear_folder_paint_name(entry: &str, folder: &str) -> Option<String> {
    let n = entry.replace('\\', "/").to_ascii_lowercase();
    if !n.contains(&format!("/{folder}/")) && !n.starts_with(&format!("{folder}/")) {
        return None;
    }
    let base = entry.replace('\\', "/");
    let base = base.rsplit('/').next()?;
    let stem = base.strip_suffix(".pnt").or_else(|| base.strip_suffix(".PNT"))?;
    (!stem.is_empty()).then(|| stem.to_string())
}

fn primary_tex_name(texs: &[paint::PntTexture]) -> Option<String> {
    texs.iter()
        .map(|t| t.name.as_str())
        .find(|n| {
            let l = n.to_ascii_lowercase();
            !l.ends_with("_n") && !l.ends_with("_r")
        })
        .or_else(|| texs.first().map(|t| t.name.as_str()))
        .map(|s| s.to_string())
}

fn load_gear_model_blocking(
    path: String,
    part: String,
    paint: Option<String>,
    goggles: Option<String>,
    stock: bool,
    stock_goggles: bool,
) -> Result<RiderPart, String> {
    let p = std::path::Path::new(&path);
    let files = read_gear_files(p).map_err(|e| format!("{e:#}"))?;
    let want = paint.filter(|s| !s.is_empty());
    let want_goggles = goggles.filter(|s| !s.is_empty());
    let mut nodes = Vec::new();
    // Kept so a stock request can read the mesh's own textures back out of it.
    let mut mesh: Option<&Vec<u8>> = None;
    // Collect paint/goggle entries up front so we can prefer the requested one but always
    // fall back to the first available: a stale or unknown paint name must still show the
    // gear textured, never bare grey.
    let mut paints: Vec<(String, &Vec<u8>)> = Vec::new();
    let mut goggle_paints: Vec<(String, &Vec<u8>)> = Vec::new();
    for (name, data) in &files {
        let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        if base.ends_with(".edf") {
            if nodes.is_empty() && is_visible_gear_mesh(name) {
                mesh = Some(data);
                nodes = edf::parse(data);
                edf::to_right_handed(&mut nodes);
                keep_lod0(&mut nodes);
            }
        } else if let Some(pname) = gear_folder_paint_name(name, "paints") {
            paints.push((pname, data));
        } else if let Some(gname) = gear_folder_paint_name(name, "goggles") {
            goggle_paints.push((gname, data));
        }
    }
    if nodes.is_empty() {
        return Err(format!("no gear mesh found in {path}"));
    }
    let mut textures: Vec<paint::PntTexture> = Vec::new();
    // A stock side decodes nothing from `paints/` — the mesh already carries that texture.
    let main_tex = (!stock)
        .then(|| pick_gear_paint(&paints, want.as_deref(), &mut textures))
        .flatten();
    let goggle_tex = (!stock_goggles)
        .then(|| pick_gear_paint(&goggle_paints, want_goggles.as_deref(), &mut textures))
        .flatten();
    if want.is_some() && main_tex.is_none() && !stock && !paints.is_empty() {
        log::warn!("[rider] {part} paint {want:?} not found; used first of {} packed", paints.len());
    }
    let mut out: Vec<paint::PaintTexture> = textures.iter().map(paint::to_texture).collect();
    // The look the model ships with, before any paint: the textures embedded in the mesh.
    let (mut stock_main, mut stock_goggle) = (None, None);
    if stock || stock_goggles {
        let embedded = mesh.map(|d| paint::extract_edf_textures(d)).unwrap_or_default();
        for t in &embedded {
            let slot = if is_goggle_name(&t.name) { &mut stock_goggle } else { &mut stock_main };
            slot.get_or_insert_with(|| t.name.clone());
        }
        if stock && stock_main.is_none() {
            log::warn!("[rider] {part} has no stock texture in its mesh — showing it bare");
        }
        // A paint reuses the mesh's texture names (that's how it replaces them), so with
        // one side stock and the other painted the two sets collide. Resolve it here: the
        // stock side's embedded texture wins, the painted side keeps its `.pnt`. The
        // frontend maps textures by name and would otherwise show whichever image
        // happened to finish loading last.
        let mut claimed: Vec<String> = Vec::new();
        if stock {
            claimed.extend(stock_main.clone());
        }
        if stock_goggles {
            claimed.extend(stock_goggle.clone());
        }
        let claimed: Vec<String> = claimed.iter().map(|s| s.to_ascii_lowercase()).collect();
        out.retain(|t| !claimed.contains(&t.name.to_ascii_lowercase()));
        let taken: std::collections::HashSet<String> =
            out.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        out.extend(
            embedded
                .into_iter()
                .filter(|t| !taken.contains(&t.name.to_ascii_lowercase())),
        );
    }
    let main_tex = if stock { stock_main } else { main_tex };
    let goggle_tex = if stock_goggles { stock_goggle } else { goggle_tex };
    log::info!(
        "[viewer] {part}: paint={want:?} goggles={want_goggles:?} stock={stock}/{stock_goggles} \
         -> shell {main_tex:?}, goggles {goggle_tex:?} ({} textures)",
        out.len(),
    );
    bind_gear_submeshes(&mut nodes, main_tex.as_deref(), goggle_tex.as_deref());
    Ok(RiderPart { part, nodes, textures: out })
}

/// The gear file carrying the visible mesh — not the `_s` shadow or the `c_` cockpit variant.
fn is_visible_gear_mesh(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    base.ends_with(".edf") && !base.ends_with("_s.edf") && !base.starts_with("c_")
}

/// Normal (`_n`) and reflection (`_r`) maps ride alongside a colour texture and are never
/// the look itself. Mirrors the filter in `paint::extract_edf_textures`.
fn is_companion_map(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with("_n") || n.ends_with("_r")
}

/// Goggles (and their lens) are the one gear part painted separately from the shell —
/// the same test decides which submesh wears which texture, and which embedded texture
/// is the stock goggle.
fn is_goggle_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("goggle") || n.contains("lens")
}

/// Pick a gear paint by name, else the first available so the piece is always textured.
/// Decodes the winner, appends its textures, and returns its primary (colour) texture name.
fn pick_gear_paint(
    paints: &[(String, &Vec<u8>)],
    want: Option<&str>,
    textures: &mut Vec<paint::PntTexture>,
) -> Option<String> {
    let chosen = want
        .and_then(|w| paints.iter().find(|(n, _)| n.eq_ignore_ascii_case(w)))
        .or_else(|| paints.first())?;
    let pnt = paint::decode_any(chosen.1).ok()?;
    let tex = primary_tex_name(&pnt);
    textures.extend(pnt);
    tex
}

/// Bind each submesh (or single-material node) to its colour texture: goggles/lenses take
/// the goggle paint, everything else the shell paint. Unmatched → `None`, so the frontend
/// renders neutral grey rather than smearing another part's texture over it.
fn bind_gear_submeshes(nodes: &mut [edf::EdfNode], main_tex: Option<&str>, goggle_tex: Option<&str>) {
    for node in nodes.iter_mut() {
        if node.submeshes.is_empty() {
            node.texture = main_tex.map(str::to_string);
            continue;
        }
        for sm in &mut node.submeshes {
            sm.texture = if is_goggle_name(&sm.name) {
                goggle_tex.or(main_tex).map(str::to_string)
            } else {
                main_tex.map(str::to_string)
            };
        }
    }
}

fn read_gear_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::Context;
    if p.is_dir() {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            if path.is_file() {
                if let (Some(name), Ok(bytes)) =
                    (path.file_name().and_then(|n| n.to_str()), std::fs::read(&path))
                {
                    out.push((name.to_string(), bytes));
                }
            }
        }
        for sub in ["paints", "goggles"] {
            if let Ok(rd) = std::fs::read_dir(p.join(sub)) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if let (Some(name), Ok(bytes)) =
                        (path.file_name().and_then(|n| n.to_str()), std::fs::read(&path))
                    {
                        out.push((format!("{sub}/{name}"), bytes));
                    }
                }
            }
        }
        return Ok(out);
    }
    pkz::read_all(p)
}

struct GearSpec {
    part: &'static str,
    mods_kind: &'static str,
    pkz_kind: &'static str,
    mesh: &'static str,
    default_name: &'static str,
}

const GEAR: [GearSpec; 3] = [
    GearSpec { part: "helmet", mods_kind: "helmets", pkz_kind: "helmets", mesh: "helmet.edf", default_name: "default" },
    GearSpec { part: "boots", mods_kind: "boots", pkz_kind: "boots", mesh: "boots.edf", default_name: "default" },
    GearSpec { part: "protection", mods_kind: "protection", pkz_kind: "protections", mesh: "armour.edf", default_name: "full" },
];

fn load_gear(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    spec: &GearSpec,
    model: &str,
    paint: &str,
    goggles: &str,
) -> Option<RiderPart> {
    if !model.is_empty() {
        let kind_dir = base.join(spec.mods_kind);
        let stem = model.trim_end_matches(".pkz");
        for src in [kind_dir.join(stem), kind_dir.join(format!("{stem}.pkz"))] {
            if !src.exists() {
                continue;
            }
            match load_gear_model_blocking(
                src.to_string_lossy().into_owned(),
                spec.part.to_string(),
                Some(paint.to_string()),
                Some(goggles.to_string()),
                // The rider wears what the loadout names; "stock" is a preview-only choice.
                false,
                false,
            ) {
                Ok(part) => {
                    log::info!("[rider] {} '{model}' loaded: {} nodes", spec.part, part.nodes.len());
                    return Some(part);
                }
                // Don't silently fall through to stock: a chosen model that fails to parse is a
                // real problem the client's log should show, not a bare head with no trace.
                Err(e) => log::warn!("[rider] {} '{model}' from {src:?} failed: {e}", spec.part),
            }
        }
    }
    // Stock / "free" gear: mesh and paint ship separately in the game pkz, so bind submeshes
    // to the paint's primary texture here too (installed gear is bound in load_gear_model_blocking).
    let name = if model.is_empty() { spec.default_name } else { model };
    let pkz = resolve_game_pkz(cfg, "rider.pkz")?;
    let folder = format!("rider/{}/{}", spec.pkz_kind, name);
    let mut nodes = load_pkz_mesh(&pkz, &format!("{folder}/{}", spec.mesh))?;
    let textures = load_pkz_paint(&pkz, &folder, paint);
    let main_tex = primary_paint_texture_name(&textures);
    bind_gear_submeshes(&mut nodes, main_tex.as_deref(), None);
    log::info!("[rider] {} stock '{name}' loaded: {} nodes, tex={main_tex:?}", spec.part, nodes.len());
    Some(RiderPart {
        part: spec.part.into(),
        nodes,
        textures,
    })
}

/// Primary (colour) texture name of an already-decoded paint — first that isn't a
/// companion `_n`/`_r` map, else the first texture.
fn primary_paint_texture_name(texs: &[paint::PaintTexture]) -> Option<String> {
    texs.iter()
        .map(|t| t.name.as_str())
        .find(|n| {
            let l = n.to_ascii_lowercase();
            !l.ends_with("_n") && !l.ends_with("_r")
        })
        .or_else(|| texs.first().map(|t| t.name.as_str()))
        .map(str::to_string)
}

fn load_pkz_paint(
    pkz: &std::path::Path,
    folder: &str,
    paint: &str,
) -> Vec<paint::PaintTexture> {
    let named = (!paint.is_empty())
        .then(|| read_pkz_entry(pkz, &format!("{folder}/paints/{paint}.pnt")))
        .flatten();
    named
        .or_else(|| read_pkz_first(pkz, &format!("{folder}/paints/"), ".pnt"))
        .and_then(|d| paint::decode_any(&d).ok())
        .map(|p| p.iter().map(paint::to_texture).collect())
        .unwrap_or_default()
}

fn read_pkz_first(pkz: &std::path::Path, prefix: &str, ext: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(pkz).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut hit = None;
    for i in 0..zip.len() {
        let f = zip.by_index(i).ok()?;
        let n = f.name().replace('\\', "/");
        if n.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())
            && n.to_ascii_lowercase().ends_with(ext)
        {
            hit = Some(i);
            break;
        }
    }
    let mut f = zip.by_index(hit?).ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut f, &mut buf).ok()?;
    Some(buf)
}

fn load_rider_paint(
    base: &std::path::Path,
    part: &str,
    profile: &str,
    sub: &str,
    paint: &str,
) -> Option<RiderPart> {
    if paint.is_empty() {
        return None;
    }
    // With no profile picked the body mesh already falls back to the stock rider
    // (`load_rider_body_nodes`); do the same here so a chosen suit/glove paint still
    // resolves instead of silently dropping off the preview.
    let profile = if profile.is_empty() { "default_mx" } else { profile };
    let data = read_paint_file(&base.join("riders").join(profile).join(sub), paint)?;
    let textures: Vec<_> = paint::decode_any(&data).ok()?.iter().map(paint::to_texture).collect();
    if textures.is_empty() {
        return None;
    }
    Some(RiderPart {
        part: part.into(),
        nodes: Vec::new(),
        textures,
    })
}

fn read_paint_file(dir: &std::path::Path, paint: &str) -> Option<Vec<u8>> {
    if !paint.is_empty() {
        return std::fs::read(dir.join(format!("{paint}.pnt"))).ok();
    }
    let first = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pnt")))?;
    std::fs::read(first).ok()
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
async fn import_file(
    app: tauri::AppHandle,
    path: String,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        install::import_file(&app, &cfg, &path, &subpath, &dest_folder).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("import_file task failed: {e}"))?
}

#[tauri::command]
async fn move_mod(
    app: tauri::AppHandle,
    from_path: String,
    to_folder: String,
    subpath: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        library::move_mod(&cfg.mods_path, &from_path, &to_folder, &subpath)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("move_mod task failed: {e}"))?
}

#[tauri::command]
async fn uninstall_mod(app: tauri::AppHandle, from_path: String, subpath: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        library::uninstall_mod(&cfg.mods_path, &from_path, &subpath).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("uninstall_mod task failed: {e}"))?
}

#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_game_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.game_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Point the app at a different MX Bikes folder; an empty string re-runs detection.
/// Only the folder changes — unlike a full `create_config`, the rest of the settings
/// (startup, tray, FrostMod, first-run state) are left alone.
///
/// Async so the switch runs off the UI thread: `finalize` can scan Steam libraries and
/// restarting the watcher tears down its background thread, neither of which should be
/// able to lock up the window.
#[tauri::command]
async fn set_mods_path(
    app: tauri::AppHandle,
    watcher: State<'_, ModWatcher>,
    path: String,
) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.mods_path = path;
    let cfg = config::finalize(cfg);
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    if cfg.watch_mods_reload {
        modwatch::start(&app, &watcher, &cfg.mods_path);
    }
    Ok(())
}

/// Remember that the intro slideshow / guided tour is done. No-ops before the config
/// exists — writing one there would leave the app "configured" with no folder set;
/// the webview flag covers that short window instead.
#[tauri::command]
fn set_intro_seen(app: tauri::AppHandle, welcome: bool, tour: bool) -> Result<(), String> {
    if !config::exists(&app) {
        return Ok(());
    }
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.welcome_seen |= welcome;
    cfg.tour_done |= tour;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Override the PiBoSo `profiles` folder for the split-folder edge case. An empty
/// string clears the override, falling back to `<mods_path>/profiles`.
#[tauri::command]
fn set_profiles_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.profiles_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Scan Steam for the MX Bikes install (holds `rider.pkz`). `None` if not found.
#[tauri::command]
fn detect_game_path() -> Option<String> {
    config::detect_game_path()
}

/// How many profiles (subdirs with a `profile.ini`) live under `path` — lets the
/// UI warn when a picked profiles folder has none.
#[tauri::command]
fn count_profiles_in(path: String) -> usize {
    presets::list_profiles(std::path::Path::new(&path)).len()
}

/// Whether this build can decode real bike geometry (the optional local module is
/// compiled in). Public builds without it return `false`, so the UI hides the bike
/// 3D preview instead of showing a broken/empty one.
#[tauri::command]
fn bike_preview_available() -> bool {
    cfg!(sidecar)
}

/// The OS we're running on — `"windows"`, `"macos"`, `"linux"`.
///
/// The frontend used to infer this from `navigator.userAgent`, which can tell a Mac from
/// everything else and nothing more. Features that only exist on Windows (FrostMod, the
/// live in-game refresh) need to know the difference between Windows and Linux, so it
/// comes from the backend rather than adding `plugin-os` and a capability for one string.
#[tauri::command]
fn app_platform() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
fn set_run_in_background(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.run_in_background = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

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

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn frostmod_reload() -> ReloadOutcome {
    frostmod::signal_reload()
}

#[tauri::command]
fn frostmod_running() -> bool {
    frostmod::is_running()
}

/// Start MX Bikes from the Play button in the sidebar.
#[tauri::command]
fn launch_game(app: tauri::AppHandle) -> Result<gameproc::LaunchOutcome, String> {
    // `load_or_detect`, not `load`: a missing config file shouldn't turn Play into an
    // error when the install is sitting exactly where the detector looks.
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    gameproc::launch(&cfg).map_err(|e| format!("{e:#}"))
}

/// Is MX Bikes running? Polled by the sidebar so Play can show the live state.
#[tauri::command]
fn game_running() -> bool {
    gameproc::is_game_running()
}

/// Installed bikes with their class, for the garage bike-switch UI. The frontend
/// filters this to the current race's class before offering a swap.
#[tauri::command]
async fn garage_scan_bikes(app: tauri::AppHandle) -> Result<Vec<bikeswap::BikeIdentity>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(bikeswap::scan_installed_bikes(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("garage_scan_bikes task failed: {e}"))?
}

/// Ask FrostMod to swap the active bike (offline, in-garage). FrostMod enforces the
/// offline/in-garage guard; this only sends the request.
#[tauri::command]
fn garage_swap_bike(bike_id: String) -> frostmod::CommandOutcome {
    frostmod::signal_swap_bike(&bike_id)
}

#[tauri::command]
async fn frostmod_status(app: tauri::AppHandle) -> FrostmodStatus {
    frostmod_manage::status(&app).await
}

#[tauri::command]
async fn frostmod_install(
    app: tauri::AppHandle,
    state: State<'_, FrostmodProcess>,
) -> Result<String, String> {
    let was_running = frostmod::is_running();
    let was_installed = frostmod_manage::is_installed(&app);
    frostmod_manage::stop(&state);
    frostmod_manage::force_stop_exe();

    let tag = frostmod_manage::install(&app).await.map_err(|e| format!("{e:#}"))?;

    if was_running || !was_installed {
        let _ = frostmod_manage::start(&app, &state);
    }
    Ok(tag)
}

#[tauri::command]
fn frostmod_start(app: tauri::AppHandle, state: State<FrostmodProcess>) -> Result<bool, String> {
    frostmod_manage::start(&app, &state).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn frostmod_stop(state: State<FrostmodProcess>) {
    frostmod_manage::stop(&state);
}

#[tauri::command]
fn set_auto_run_frostmod(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.auto_run_frostmod = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_instant_refresh(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.instant_refresh = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Show or hide the in-game overlay. Also reachable from its global hotkey.
#[tauri::command]
fn overlay_toggle(app: tauri::AppHandle) -> Result<(), String> {
    overlay::toggle(&app)
}

/// Dismiss the overlay (its close button and Esc) and hand focus back to the game.
#[tauri::command]
fn overlay_hide(app: tauri::AppHandle) -> Result<(), String> {
    overlay::hide(&app)
}

#[tauri::command]
fn overlay_state(app: tauri::AppHandle) -> overlay::OverlayState {
    overlay::state(&config::load(&app).unwrap_or_default())
}

#[tauri::command]
fn set_overlay_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.overlay_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Turning it off should also take the overlay off the screen, not just stop the
    // hotkey from re-summoning it.
    if !enabled {
        let _ = overlay::hide(&app);
    }
    overlay::register(&app, &cfg)
}

/// Rebind the overlay hotkey. Validates and registers before saving, so a combo that
/// another app already owns leaves the working one in place.
#[tauri::command]
fn set_overlay_hotkey(app: tauri::AppHandle, hotkey: String) -> Result<(), String> {
    let previous = config::load(&app).unwrap_or_default();
    let mut cfg = previous.clone();
    cfg.overlay_hotkey = hotkey;
    if let Err(e) = overlay::register(&app, &cfg) {
        // Put the old binding back — a rejected combo must not leave the player with
        // no way to open the overlay.
        let _ = overlay::register(&app, &previous);
        return Err(e);
    }
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_watch_mods_reload(
    app: tauri::AppHandle,
    state: State<ModWatcher>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.watch_mods_reload = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Start/stop the watcher live so the toggle takes effect without a restart.
    if enabled {
        modwatch::start(&app, &state, &cfg.mods_path);
    } else {
        modwatch::stop(&state);
    }
    Ok(())
}

#[tauri::command]
async fn shop_login(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("shop-login") {
        let _ = w.set_focus();
        return Ok(());
    }

    let url = tauri::WebviewUrl::External(
        format!(
            "{base}/wp-login.php?redirect_to={base}%2Fall-my-downloads%2F",
            base = shop_session::SHOP_BASE
        )
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

#[tauri::command]
fn shop_status(state: State<shop_session::ShopSession>) -> bool {
    state.logged_in()
}

#[tauri::command]
fn shop_logout(app: tauri::AppHandle) {
    shop_session::clear_session(&app);
}

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
    let subpath = format!("mods/{}", mods::mxbshop::guess_mod_type(&item.title));
    install::download_and_place(
        &app,
        &cfg,
        &client,
        &item.slug,
        &item.download_url,
        &subpath,
        &dest_folder,
    )
    .await
    .map_err(|e| format!("{e:#}"))
}

fn presets_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("{e:#}"))
}

/// The player's profiles, plus the folder they were read from and whether it exists —
/// so an empty Presets tab can say *which* folder came up empty instead of leaving the
/// player to guess that a path is involved at all.
#[tauri::command]
fn presets_list_profiles(app: tauri::AppHandle) -> Result<presets::ProfilesScan, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(presets::scan_profiles(&cfg.profiles_dir()))
}

#[tauri::command]
fn presets_list_bikes(app: tauri::AppHandle, profile: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    presets::list_bikes(&cfg.profiles_dir(), &profile).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_read_loadout(
    app: tauri::AppHandle,
    profile: String,
    bikeid: String,
) -> Result<presets::Loadout, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let mut loadout =
        presets::read_loadout(&cfg.profiles_dir(), &profile, &bikeid).map_err(|e| format!("{e:#}"))?;
    let active = modelswap::current_active(&cfg.mods_path, &bikeid);
    if !active.eq_ignore_ascii_case(modelswap::ORIGINAL_LABEL) {
        loadout.model_swap = active;
    }
    Ok(loadout)
}

#[derive(serde::Serialize)]
struct PresetApplyOutcome {
    content_reload: ReloadOutcome,
    game_running: bool,
    live_refresh: gameproc::LiveRefresh,
    /// Set only when the preset actually performed a model swap — see the note on
    /// `SwapApplyOutcome::model_refresh`.
    model_refresh: Option<frostmod::CommandOutcome>,
}

#[tauri::command]
fn presets_apply(
    app: tauri::AppHandle,
    profile: String,
    bikeid: String,
    loadout: presets::Loadout,
    make_active: bool,
) -> Result<PresetApplyOutcome, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    presets::apply_loadout(&cfg.profiles_dir(), &profile, &bikeid, &loadout, make_active)
        .map_err(|e| format!("{e:#}"))?;
    let want = loadout.model_swap.trim();
    let mut model_refresh = None;
    if !want.is_empty() && !want.eq_ignore_ascii_case(&modelswap::current_active(&cfg.mods_path, &bikeid))
    {
        modelswap::apply_model_swap(&cfg.mods_path, &bikeid, want)
            .map_err(|e| format!("Cosmetics applied, but the model swap failed: {e:#}"))?;
        // Same reason as the Locker path: the look loader won't reload the mesh.
        model_refresh = model_refresh_cmd(cfg.instant_refresh, &bikeid);
    }
    let content_reload = frostmod::signal_reload();
    Ok(PresetApplyOutcome {
        content_reload,
        game_running: gameproc::is_game_running(),
        live_refresh: live_refresh(cfg.instant_refresh),
        model_refresh,
    })
}

#[tauri::command]
fn presets_list(app: tauri::AppHandle) -> Result<Vec<presets::Preset>, String> {
    Ok(presets::load_presets(&presets_dir(&app)?))
}

#[tauri::command]
fn presets_save(app: tauri::AppHandle, preset: presets::Preset) -> Result<(), String> {
    presets::save_preset(&presets_dir(&app)?, preset).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    presets::delete_preset(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_export(app: tauri::AppHandle, name: String) -> Result<String, String> {
    presets::export_code(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_decode(text: String) -> Result<presets::Preset, String> {
    presets::decode_code(&text).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_import(app: tauri::AppHandle, text: String) -> Result<presets::Preset, String> {
    presets::import_code(&presets_dir(&app)?, &text).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn preset_bundle_stats(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<bundle::BundlePlan, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    bundle::plan(&cfg, &loadout).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn preset_bundle_create(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let dir = presets_dir(&app)?;
    bundle::create(&app, &cfg, &dir, &name)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn preset_bundle_import(
    app: tauri::AppHandle,
    text: String,
) -> Result<presets::Preset, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let dir = presets_dir(&app)?;
    bundle::import(&app, &cfg, &dir, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

fn main() {
    tauri::Builder::default()
        .plugin(
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
        // The overlay's hotkey has to fire while MX Bikes holds keyboard focus.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(FrostmodProcess::default())
        .manage(ModWatcher::default())
        .manage(shop_session::ShopSession::default())
        .setup(|app| {
            log::info!("MXB App {} starting", env!("CARGO_PKG_VERSION"));
            if let Ok(dir) = app.path().app_local_data_dir() {
                log::info!("data dir (config/session/frostmod): {}", dir.display());
            }
            if let Ok(dir) = app.path().app_log_dir() {
                log::info!("log dir: {}", dir.display());
            }

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

            let handle = app.handle();
            log::info!(
                "config: {} ({})",
                config::config_path(handle).display(),
                if config::exists(handle) { "found" } else { "missing" },
            );
            // `load_or_detect` rebuilds a missing/unreadable config from the standard
            // MX Bikes folder, so a lost config no longer means a trip through setup.
            if let Some(mut cfg) = config::load_or_detect(handle) {
                // Auto-detect the MX Bikes install on launch for configs that
                // never got one (created before detection existed, or when the
                // game wasn't installed yet). Only fills a blank — never overrides
                // a manual pick — and persists it so the 3D rider preview works.
                if cfg.game_path.trim().is_empty() {
                    if let Some(gp) = config::detect_game_path() {
                        log::info!("auto-detected MX Bikes install: {gp}");
                        cfg.game_path = gp;
                        let _ = config::save(handle, &cfg);
                    }
                }
                let manager = handle.autolaunch();
                let enabled = manager.is_enabled().unwrap_or(false);
                if cfg.launch_at_startup && !enabled {
                    let _ = manager.enable();
                } else if !cfg.launch_at_startup && enabled {
                    let _ = manager.disable();
                }
                if cfg.auto_run_frostmod && frostmod_manage::is_installed(handle) {
                    let state = handle.state::<FrostmodProcess>();
                    let _ = frostmod_manage::start(handle, &state);
                }
                if cfg.watch_mods_reload {
                    let watcher = handle.state::<ModWatcher>();
                    modwatch::start(handle, &watcher, &cfg.mods_path);
                }
                // A combo another app already owns shouldn't stop the app from starting
                // — Settings reports the state and lets the player pick another.
                if let Err(e) = overlay::register(handle, &cfg) {
                    log::warn!("overlay hotkey not registered: {e}");
                }
            } else {
                log::info!("no MX Bikes folder found — showing first-run setup");
            }
            shop_session::load_session(handle);
            mxb_session::load(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing the overlay (Alt+F4, its own button) parks it rather than
                // destroying it, so the next hotkey press doesn't rebuild the webview.
                if window.label() == overlay::LABEL {
                    api.prevent_close();
                    let _ = overlay::hide(window.app_handle());
                    return;
                }
                let cfg = config::load(window.app_handle()).unwrap_or_default();
                // Never on Linux: the tray runs through libayatana-appindicator, which
                // doesn't deliver click events to Tauri and isn't present at all on a
                // stock GNOME desktop. Hiding there can strand the window with no way
                // back, so closing closes.
                let tray_can_restore = cfg!(not(target_os = "linux"));
                if cfg.run_in_background && tray_can_restore && !cfg!(debug_assertions) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            is_configured,
            get_config,
            create_config,
            bike_preview_available,
            app_platform,
            search_mods,
            get_mod_detail,
            get_mod_ratings,
            get_installed_mods,
            scan_library,
            get_pkz_meta_cached,
            get_pkz_meta,
            get_pkz_preview,
            unpack_paint,
            unpack_pkz,
            load_bike_model,
            load_rider_model,
            load_rider_body_model,
            load_gear_model,
            load_stock_gear_model,
            list_gear_paints,
            list_installed_gear_paints,
            scan_rider_targets,
            scan_model_swaps,
            apply_model_swap,
            scan_sound_swaps,
            apply_sound_swap,
            bind_sound,
            unbind_sound,
            detect_loose_swaps,
            register_loose_swaps,
            detect_orphaned_setup,
            repair_orphaned_setup,
            add_to_library,
            import_file,
            move_mod,
            uninstall_mod,
            reveal_in_explorer,
            set_game_path,
            set_mods_path,
            set_intro_seen,
            set_profiles_path,
            detect_game_path,
            count_profiles_in,
            set_run_in_background,
            set_launch_at_startup,
            set_auto_run_frostmod,
            set_instant_refresh,
            overlay_toggle,
            overlay_hide,
            overlay_state,
            set_overlay_enabled,
            set_overlay_hotkey,
            set_watch_mods_reload,
            frostmod_reload,
            frostmod_running,
            garage_scan_bikes,
            garage_swap_bike,
            frostmod_status,
            frostmod_install,
            frostmod_start,
            frostmod_stop,
            launch_game,
            game_running,
            shop_login,
            shop_status,
            shop_logout,
            shop_my_downloads,
            shop_install,
            presets_list_profiles,
            presets_list_bikes,
            presets_read_loadout,
            presets_apply,
            presets_list,
            presets_save,
            presets_delete,
            presets_export,
            presets_decode,
            presets_import,
            preset_bundle_stats,
            preset_bundle_create,
            preset_bundle_import
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod viewer_tests {
    #[test]
    #[ignore]
    fn bike_model_from_pkz() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let m = super::load_bike_model_blocking(path).expect("load bike");
        for n in &m.nodes {
            eprintln!("node '{}' placed={}", n.name, n.placed);
            for s in &n.submeshes {
                eprintln!(
                    "   {:<16} -> {:<12} tile={:?}",
                    s.name,
                    s.texture.as_deref().unwrap_or("(none)"),
                    s.uv_tile
                );
            }
        }
        for p in &m.paints {
            let mut names: Vec<&str> = p.textures.iter().map(|t| t.name.as_str()).collect();
            names.sort_unstable();
            eprintln!(
                "paint '{}' changes_preview={}: {}",
                p.name,
                p.changes_preview,
                names.join(", ")
            );
        }
        assert!(!m.nodes.is_empty(), "decoded the mesh");
        let have: std::collections::HashSet<String> = m.paints[0]
            .textures
            .iter()
            .map(|t| t.name.to_ascii_lowercase())
            .collect();
        for n in &m.nodes {
            for s in &n.submeshes {
                if let Some(t) = &s.texture {
                    assert!(have.contains(&t.to_ascii_lowercase()), "'{t}' is available");
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn gear_model_from_pkz() {
        let Ok(path) = std::env::var("MXB_REAL_GEAR") else {
            eprintln!("set MXB_REAL_GEAR to run");
            return;
        };
        let files = super::read_gear_files(std::path::Path::new(&path)).expect("read gear");
        let paints: Vec<String> = files
            .iter()
            .filter_map(|(n, _)| super::gear_folder_paint_name(n, "paints"))
            .collect();
        let goggles: Vec<String> = files
            .iter()
            .filter_map(|(n, _)| super::gear_folder_paint_name(n, "goggles"))
            .collect();
        eprintln!("paints ({}): {:?}", paints.len(), &paints[..paints.len().min(4)]);
        eprintln!("goggles ({}): {:?}", goggles.len(), &goggles[..goggles.len().min(4)]);

        let part = super::load_gear_model_blocking(path.clone(), "helmet".into(), None, None, false, false)
            .expect("load gear");
        let have: std::collections::HashSet<String> =
            part.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        let mut shell = None;
        let mut goggle = None;
        for n in &part.nodes {
            for s in &n.submeshes {
                let t = s.texture.as_ref().expect("submesh bound to a texture");
                eprintln!("submesh {:<10} -> {t}", s.name);
                assert!(have.contains(&t.to_ascii_lowercase()), "'{t}' is shipped");
                if s.name.to_ascii_lowercase().contains("goggle") {
                    goggle = Some(t.clone());
                } else {
                    shell = Some(t.clone());
                }
            }
        }
        if !goggles.is_empty() {
            let (shell, goggle) = (shell.expect("a shell submesh"), goggle.expect("a goggle submesh"));
            assert_ne!(shell, goggle, "goggles bind their own texture, not the shell's");
        }

        // Stock: the same mesh wearing the textures embedded in it, not a packed `.pnt`.
        let listed = super::gear_paints_at(std::path::Path::new(&path)).expect("list paints");
        eprintln!("has_stock={} goggles={}", listed.has_stock, listed.has_stock_goggles);
        if !listed.has_stock {
            eprintln!("this piece embeds no textures — no stock entry to check");
            return;
        }
        let stock =
            super::load_gear_model_blocking(path, "helmet".into(), None, None, true, listed.has_stock_goggles)
                .expect("load stock gear");
        let embedded: std::collections::HashSet<String> =
            stock.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        assert!(
            !paints.iter().any(|p| embedded.contains(&p.to_ascii_lowercase())),
            "a stock preview decodes no packed paint",
        );
        let mut bound = 0;
        for n in &stock.nodes {
            for s in &n.submeshes {
                let t = s.texture.as_ref().expect("stock submesh bound to a texture");
                eprintln!("stock submesh {:<10} -> {t}", s.name);
                assert!(embedded.contains(&t.to_ascii_lowercase()), "'{t}' is embedded in the mesh");
                bound += 1;
            }
        }
        assert!(bound > 0, "stock bound at least one submesh");

        // Mixed: stock shell, painted goggles. A paint reuses the mesh's texture names, so
        // this is where the two sets would collide and the viewer would pick at random.
        if let Some(g) = goggles.first() {
            let mixed = super::load_gear_model_blocking(
                std::env::var("MXB_REAL_GEAR").unwrap(),
                "helmet".into(),
                None,
                Some(g.clone()),
                true,
                false,
            )
            .expect("load mixed gear");
            let mut names: Vec<String> =
                mixed.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
            names.sort_unstable();
            let total = names.len();
            names.dedup();
            assert_eq!(names.len(), total, "one texture per name, so binding is unambiguous");
            for n in &mixed.nodes {
                for s in &n.submeshes {
                    let t = s.texture.as_ref().expect("mixed submesh bound to a texture");
                    assert!(names.contains(&t.to_ascii_lowercase()), "'{t}' is available");
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn lod0_dedup_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            eprintln!("set MXB_REAL_EDF to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read edf");
        let mut nodes = crate::edf::parse(&bytes);
        let before = nodes.len();
        super::keep_lod0(&mut nodes);
        for n in &nodes {
            eprintln!("kept node '{}' tris={}", n.name, n.indices.len() / 3);
        }
        let mut names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "no duplicate node names survive");
        eprintln!("{before} nodes -> {} after LOD dedup", nodes.len());
    }
}

