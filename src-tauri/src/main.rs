// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bundle;
mod cfg;
mod config;
mod edf;
mod frostmod;
mod frostmod_manage;
mod gameproc;
mod install;
mod library;
mod modelswap;
mod mods;
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

#[tauri::command]
async fn apply_model_swap(app: tauri::AppHandle, bike: String, target: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || apply_model_swap_blocking(app, bike, target))
        .await
        .map_err(|e| format!("apply_model_swap task failed: {e}"))?
}

fn apply_model_swap_blocking(app: tauri::AppHandle, bike: String, target: String) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    modelswap::apply_model_swap(&cfg.mods_path, &bike, &target).map_err(|e| format!("{e:#}"))?;
    frostmod::signal_reload();
    Ok(())
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
    let mut model: Option<&Vec<u8>> = None;
    let mut geom: Option<&Vec<u8>> = None;
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut tga_jobs: Vec<(String, &[u8])> = Vec::new();
    let mut pnt_jobs: Vec<(String, &[u8], bool)> = Vec::new();
    for (name, data) in &files {
        let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        if bn == "model.edf" {
            model = Some(data);
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
    let mut level0: Vec<String> = Vec::new();
    let mut node_part: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (part, gp) in &gfx {
        let Some(hrc_file) = gp.hrc.as_deref() else { continue };
        let stem = hrc_file.trim_end_matches(".hrc").trim_end_matches(".HRC");
        let Some(bytes) = hrcs.get(&stem.to_ascii_lowercase()) else {
            log::warn!("[viewer] gfx.cfg part '{part}' wants {hrc_file}, which the bike doesn't ship");
            continue;
        };
        if let Some(node) = cfg::hrc_level0(&cfg::parse(bytes), stem) {
            node_part.insert(node.to_ascii_lowercase(), part.clone());
            level0.push(node);
        }
    }
    if let Some(data) = model {
        nodes = edf::parse_with_levels(data, &level0);
        bind_textures(&mut nodes, data, &gfx, &node_part);
    }
    for (fname, data) in &installed {
        pnt_jobs.push((paint_display_name(fname), data.as_slice(), false));
    }
    if let Some(g) = geom {
        if !edf::assemble_bike(&mut nodes, g) {
            eprintln!("[viewer] .geom present but missing mount points — parts unassembled");
        }
    } else if !nodes.is_empty() {
        eprintln!("[viewer] no .geom alongside model.edf — parts unassembled");
    }
    edf::to_right_handed(&mut nodes);
    let t_parse = t0.elapsed();

    let mut base: Vec<paint::PaintTexture> = tga_jobs
        .par_iter()
        .filter_map(|(stem, data)| paint::decode_image(stem, data))
        .collect();
    if let Some(data) = model {
        base.extend(paint::extract_edf_textures(data));
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
    let claimed: std::collections::HashSet<String> = gfx
        .values()
        .flat_map(|p| p.textures.values())
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let mut diffuse: Vec<&edf::EmbeddedTexture> = embedded
        .iter()
        .filter(|t| {
            let n = t.name.to_ascii_lowercase();
            !n.ends_with("_n") && !n.ends_with("_r") && !claimed.contains(&n)
        })
        .collect();
    diffuse.sort_by_key(|t| std::cmp::Reverse(t.width as u64 * t.height as u64));
    let diffuse_ord: Vec<&edf::EmbeddedTexture> = embedded
        .iter()
        .filter(|t| {
            let n = t.name.to_ascii_lowercase();
            !n.ends_with("_n") && !n.ends_with("_r") && !claimed.contains(&n)
        })
        .collect();

    for n in nodes.iter_mut() {
        let part = node_part.get(&n.name.to_ascii_lowercase());
        let overrides = part.and_then(|p| gfx.get(p)).map(|p| &p.textures);
        n.texture = diffuse.first().map(|t| t.name.clone());
        for sm in n.submeshes.iter_mut() {
            let group = sm.name.to_ascii_lowercase();
            if let Some(tex) = overrides.and_then(|o| {
                o.get(&group)
                    .or_else(|| o.iter().find(|(g, _)| group.ends_with(&format!("_{g}"))).map(|(_, t)| t))
            }) {
                sm.texture = Some(tex.clone());
                continue;
            }
            if let Some(t) = sm.mat.and_then(|i| diffuse_ord.get(i as usize)) {
                sm.texture = Some(t.name.clone());
                continue;
            }
            if let Some(t) = sm.uv_tile.filter(|&t| t > 0).and_then(|t| diffuse.get(t as usize)) {
                sm.texture = Some(t.name.clone());
                continue;
            }
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

fn wanted_bike_file(name: &str) -> bool {
    let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    bn == "model.edf"
        || bn.ends_with(".tga")
        || bn.ends_with(".pnt")
        || bn.ends_with(".geom")
        || bn.ends_with(".cfg")
        || bn.ends_with(".hrc")
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
        if p.join("model.edf").exists() {
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
            return Ok(out);
        }
        let sibling = p.with_extension("pkz");
        if sibling.exists() {
            return pkz::read_selected(&sibling, wanted_bike_file);
        }
        bail!("no model.edf for bike folder {p:?}");
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
) -> Result<RiderPart, String> {
    tauri::async_runtime::spawn_blocking(move || {
        load_gear_model_blocking(path, part, paint, goggles)
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
    Ok(GearPaints {
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
        let empty = GearPaints { paints: Vec::new(), goggles: Vec::new() };
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
) -> Result<RiderPart, String> {
    let p = std::path::Path::new(&path);
    let files = read_gear_files(p).map_err(|e| format!("{e:#}"))?;
    let want = paint.filter(|s| !s.is_empty());
    let want_goggles = goggles.filter(|s| !s.is_empty());
    let mut nodes = Vec::new();
    let mut textures: Vec<paint::PntTexture> = Vec::new();
    let mut main_tex: Option<String> = None;
    let mut goggle_tex: Option<String> = None;
    let mut main_seen = false;
    let mut goggle_seen = false;
    for (name, data) in &files {
        let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        if base.ends_with(".edf") {
            // Take the visible mesh: skip the `_s` shadow and `c_` cockpit variants.
            if nodes.is_empty() && !base.ends_with("_s.edf") && !base.starts_with("c_") {
                nodes = edf::parse(data);
                edf::to_right_handed(&mut nodes);
                keep_lod0(&mut nodes);
            }
        } else if let Some(pname) = gear_folder_paint_name(name, "paints") {
            let chosen = match &want {
                Some(w) => pname.eq_ignore_ascii_case(w),
                None => !main_seen,
            };
            if chosen {
                if let Ok(pnt) = paint::decode_any(data) {
                    main_seen = true;
                    main_tex = primary_tex_name(&pnt);
                    textures.extend(pnt);
                }
            }
        } else if let Some(gname) = gear_folder_paint_name(name, "goggles") {
            let chosen = match &want_goggles {
                Some(w) => gname.eq_ignore_ascii_case(w),
                None => !goggle_seen,
            };
            if chosen {
                if let Ok(pnt) = paint::decode_any(data) {
                    goggle_seen = true;
                    goggle_tex = primary_tex_name(&pnt);
                    textures.extend(pnt);
                }
            }
        }
    }
    if nodes.is_empty() {
        return Err(format!("no gear mesh found in {path}"));
    }
    for node in &mut nodes {
        for sm in &mut node.submeshes {
            let n = sm.name.to_ascii_lowercase();
            let is_goggle = n.contains("goggle") || n.contains("lens");
            sm.texture = if is_goggle {
                goggle_tex.clone().or_else(|| main_tex.clone())
            } else {
                main_tex.clone()
            };
        }
    }
    let textures = textures.iter().map(paint::to_texture).collect();
    Ok(RiderPart { part, nodes, textures })
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
            if let Ok(part) = load_gear_model_blocking(
                src.to_string_lossy().into_owned(),
                spec.part.to_string(),
                Some(paint.to_string()),
                Some(goggles.to_string()),
            ) {
                return Some(part);
            }
        }
    }
    let name = if model.is_empty() { spec.default_name } else { model };
    let pkz = resolve_game_pkz(cfg, "rider.pkz")?;
    let folder = format!("rider/{}/{}", spec.pkz_kind, name);
    let nodes = load_pkz_mesh(&pkz, &format!("{folder}/{}", spec.mesh))?;
    Some(RiderPart {
        part: spec.part.into(),
        nodes,
        textures: load_pkz_paint(&pkz, &folder, paint),
    })
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
    if profile.is_empty() || paint.is_empty() {
        return None;
    }
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

#[tauri::command]
fn presets_list_profiles(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(presets::list_profiles(&cfg.mods_path))
}

#[tauri::command]
fn presets_list_bikes(app: tauri::AppHandle, profile: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    presets::list_bikes(&cfg.mods_path, &profile).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_read_loadout(
    app: tauri::AppHandle,
    profile: String,
    bikeid: String,
) -> Result<presets::Loadout, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let mut loadout =
        presets::read_loadout(&cfg.mods_path, &profile, &bikeid).map_err(|e| format!("{e:#}"))?;
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
    presets::apply_loadout(&cfg.mods_path, &profile, &bikeid, &loadout, make_active)
        .map_err(|e| format!("{e:#}"))?;
    let want = loadout.model_swap.trim();
    if !want.is_empty() && !want.eq_ignore_ascii_case(&modelswap::current_active(&cfg.mods_path, &bikeid))
    {
        modelswap::apply_model_swap(&cfg.mods_path, &bikeid, want)
            .map_err(|e| format!("Cosmetics applied, but the model swap failed: {e:#}"))?;
    }
    let content_reload = frostmod::signal_reload();
    let live = if cfg.instant_refresh {
        gameproc::refresh_look()
    } else {
        gameproc::LiveRefresh::Disabled
    };
    Ok(PresetApplyOutcome {
        content_reload,
        game_running: gameproc::is_game_running(),
        live_refresh: live,
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
        .manage(FrostmodProcess::default())
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
            if config::exists(handle) {
                if let Ok(cfg) = config::load(handle) {
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
                }
            }
            shop_session::load_session(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let cfg = config::load(window.app_handle()).unwrap_or_default();
                if cfg.run_in_background && !cfg!(debug_assertions) {
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
            scan_library,
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
            add_to_library,
            import_file,
            move_mod,
            uninstall_mod,
            reveal_in_explorer,
            set_game_path,
            set_run_in_background,
            set_launch_at_startup,
            set_auto_run_frostmod,
            set_instant_refresh,
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

        let part = super::load_gear_model_blocking(path, "helmet".into(), None, None)
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

