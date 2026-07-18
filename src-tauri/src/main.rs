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

/// Rich Library scan: packaged `.pkz`, extracted mod folders, and loose paint
/// files, each tagged with kind/category/parent for grouping + detail in the UI.
/// (Install pickers keep using the leaner `get_installed_mods`.)
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
    // Provenance for sound mods (empty if the store/dir isn't there yet).
    let sound_bikes = app
        .path()
        .app_local_data_dir()
        .map(|d| soundmods::known_bikes(&d))
        .unwrap_or_default();
    library::scan_library(&cfg.mods_path, &subpath, &sound_bikes).map_err(|e| format!("{e:#}"))
}

/// Installed rider models (helmet/boot/protection folders) + rider profiles, used
/// to build install destinations for rider paints and per-profile kit/gloves.
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

/// Per-bike model-swap view: each extracted bike, its active model, and the
/// variants it can switch between (the app-side twin of FrostMod's F8 swapper).
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

/// Switch a bike to a different model set (backs up the current one, moves the
/// chosen one in). Signals a running FrostMod to live-reload afterward.
#[tauri::command]
async fn apply_model_swap(app: tauri::AppHandle, bike: String, target: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || apply_model_swap_blocking(app, bike, target))
        .await
        .map_err(|e| format!("apply_model_swap task failed: {e}"))?
}

fn apply_model_swap_blocking(app: tauri::AppHandle, bike: String, target: String) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    modelswap::apply_model_swap(&cfg.mods_path, &bike, &target).map_err(|e| format!("{e:#}"))?;
    // Best-effort: nudge the game to reload so the swap shows without a restart.
    frostmod::signal_reload();
    Ok(())
}

/// Read the structure of one installed `.pkz` (name/author/length/preview) for
/// its library card. Plain-zip archives are parsed; non-plain ones report
/// `locked`. Called lazily per card and cached on disk.
#[tauri::command]
async fn get_pkz_meta(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_meta_blocking(app, path))
        .await
        .map_err(|e| format!("get_pkz_meta task failed: {e}"))?
}

fn get_pkz_meta_blocking(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    pkz::read_meta_cached(&app, &path).map_err(|e| format!("{e:#}"))
}

/// Full-resolution preview image for the library detail lightbox (a `data:`
/// URI), or `None` when the archive is locked / has no image. Loaded on demand
/// (one item at a time), not per card.
#[tauri::command]
async fn get_pkz_preview(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_preview_blocking(path))
        .await
        .map_err(|e| format!("get_pkz_preview task failed: {e}"))?
}

fn get_pkz_preview_blocking(path: String) -> Result<Option<String>, String> {
    pkz::read_preview(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

/// Decode a `.pnt` paint file at `path` into its textures (PNG `data:` URIs),
/// for the 3D viewer to map onto a model. Native — no PaintEd needed.
#[tauri::command]
async fn unpack_paint(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_paint_blocking(path))
        .await
        .map_err(|e| format!("unpack_paint task failed: {e}"))?
}

fn unpack_paint_blocking(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    paint::unpack_file(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

/// Extract a `.pkz` to `out_dir`, returning the written relative paths. Lets the
/// 3D viewer pull a bike's `model.edf` + textures out of a packaged bike.
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

/// One selectable paint (livery) for a bike: a display name and its textures
/// (the paint's own packed textures plus the bike's shared base textures).
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikePaint {
    name: String,
    textures: Vec<paint::PaintTexture>,
    /// Whether selecting this paint changes anything **in the preview**.
    ///
    /// A `.pnt` replaces a model texture **by name** — that name match is the whole
    /// mechanism. So a paint moves the preview only if it ships a texture that is
    /// bound to one of the parts we render.
    ///
    /// Crucially, that is NOT the same as "this paint is for this bike". The viewer
    /// renders the chassis/steer/fork/swingarm and nothing else: the wheels and
    /// chain are separate models the bike `.pkz` doesn't even contain. The Honda's
    /// own `stock.pnt` carries *only* `chain`/`wheel`/`wheels` textures, and its
    /// `model.edf` packs only `2021crf`/`w_plate`/`exhaust_22` — so the bike's own
    /// stock paint legitimately changes nothing here. Reporting that as "not for
    /// this model" is simply false, and it is what the user saw.
    ///
    /// So this flag says one honest thing — the preview won't move — and paints
    /// shipped inside the bike are exempt from it entirely (see below): they are
    /// for this bike by definition, whatever they happen to paint.
    changes_preview: bool,
}

/// A bike's real 3D model for the viewer: decoded mesh nodes plus the selectable
/// paints (stock + any installed liveries) the user can preview.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikeModel {
    nodes: Vec<edf::EdfNode>,
    paints: Vec<BikePaint>,
}

/// In-memory cache of loaded bike models, keyed by `source + mtime`, so reopening
/// the same bike skips the (costly) decrypt + `.edf` parse + texture decode.
fn bike_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, BikeModel>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, BikeModel>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Cache key for a bike source: path + last-modified (so edits invalidate it).
fn bike_cache_key(source: &str) -> String {
    let mtime = std::fs::metadata(source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{source}:{mtime}")
}

/// Load a bike's real 3D geometry + paints for the viewer. `source` may be the
/// bike's extracted folder, its packaged `.pkz`, or a loose `.edf`. Packaged bikes
/// are read (and decrypted if needed) in a single pass; results are cached.
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

    // Split the archive into the mesh, the shared base textures (`.tga`), and the
    // paint files (`.pnt`) — plus any installed liveries. Only the mesh parse is
    // sequential; the (dominant) texture decode+PNG-encode is fanned out below.
    let mut nodes = Vec::new();
    let mut model: Option<&Vec<u8>> = None;
    let mut geom: Option<&Vec<u8>> = None;
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut tga_jobs: Vec<(String, &[u8])> = Vec::new();
    // `(display name, bytes, shipped-inside-the-bike)`. The third field is what
    // separates the bike's own stock paint from a livery the user dropped into
    // `paints/` — the former is for this bike by definition.
    let mut pnt_jobs: Vec<(String, &[u8], bool)> = Vec::new();
    // The `.pkz` flattens paths (`MX1OEM_…__gfx.cfg`), so every lookup is by
    // BASENAME — matching on the full name finds nothing in a packaged bike.
    for (name, data) in &files {
        let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        if bn == "model.edf" {
            model = Some(data);
        } else if bn.ends_with(".geom") {
            geom = Some(data);
        } else if bn.ends_with("gfx.cfg") {
            gfx_bytes = Some(data);
        } else if let Some(stem) = bn.strip_suffix(".hrc") {
            // `chassis.hrc` may arrive as `MX1OEM_2023_Honda_CRF450R__chassis.hrc`;
            // gfx.cfg refers to it as plain `chassis.hrc`.
            let stem = stem.rsplit("__").next().unwrap_or(stem);
            hrcs.insert(stem.to_string(), data);
        } else if let Some(stem) = bn.strip_suffix(".tga") {
            // Lowercased stem — the frontend matches textures case-insensitively.
            tga_jobs.push((stem.to_string(), data.as_slice()));
        } else if bn.ends_with(".pnt") {
            pnt_jobs.push((paint_display_name(&bn), data.as_slice(), true));
        }
    }

    // `gfx.cfg` → each part's `.hrc` → that part's level0 node. This *states* the
    // LOD lineup, replacing the old name heuristic (which strips a `b`/`c` before
    // the first digit and tiebreaks on triangle count — and once silently flipped
    // the KTM 450 onto its un-placeable LOD-B chassis).
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
    // Hang the fork/steering/swingarm off the chassis using the bike's physics
    // mounts. Without the `.geom` the parts stay in their own local frames (each
    // correct in isolation, but stacked at the origin), so log when it's missing.
    if let Some(g) = geom {
        if !edf::assemble_bike(&mut nodes, g) {
            eprintln!("[viewer] .geom present but missing mount points — parts unassembled");
        }
    } else if !nodes.is_empty() {
        eprintln!("[viewer] no .geom alongside model.edf — parts unassembled");
    }
    // The `.edf` is authored left-handed (DirectX); three.js is right-handed. Convert
    // once, here — after assembly, which has to run in the game's own frame because
    // mirroring X would invert its rake rotations. Without this the bike renders
    // mirrored (backwards "HONDA") AND lit inside-out (the black facets).
    edf::to_right_handed(&mut nodes);
    let t_parse = t0.elapsed();

    // Encode textures in parallel: PNG-encoding several 2048² paints one-by-one was
    // the viewer's dominant cost, and every texture is independent.
    let mut base: Vec<paint::PaintTexture> = tga_jobs
        .par_iter()
        .filter_map(|(stem, data)| paint::decode_image(stem, data))
        .collect();
    // The model's own packed textures are shared base textures too — under their
    // real names, so a paint that ships a same-named texture replaces one below.
    if let Some(data) = model {
        base.extend(paint::extract_edf_textures(data));
    }
    // `(paint, shipped-inside-the-bike)` — decoding can drop entries, so the flag
    // has to travel WITH its paint rather than by index.
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

    // Will each paint move the preview? Answer BEFORE the base textures are appended
    // below — afterwards every paint carries the model's own textures too and they'd
    // all look like they match.
    //
    // The test is the game's own rule: a paint texture replaces a model texture of
    // the SAME name, so compare against the names actually bound to the mesh (what
    // `bind_textures` resolved), not against a guessed convention.
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
        // A paint packaged inside the bike is the bike's own — never call it out,
        // whatever it paints. The Honda's `stock.pnt` touches only the chain and
        // wheels (parts the viewer doesn't render), so the binding test below would
        // flag the bike's OWN stock livery. That's the bug the user reported.
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

    // Make each paint self-contained by appending the shared base textures — but
    // the PAINT wins on a name collision. That collision *is* the paint: a `.pnt`
    // carrying `plastics` replaces the model's own `plastics`. (The frontend keys
    // textures by name, last-write-wins, so appending base blindly would let the
    // model's texture shadow the paint and every livery would render identically.)
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
    // Which textures each paint actually carries. A paint that ships no `livery`
    // can't change the bodywork — the body then falls back to the `albedo` baked
    // into model.edf, so switching to it looks like nothing happened. (The Honda's
    // stock paint carries only chain/wheel textures.)
    for p in &paints {
        let mut names: Vec<&str> = p.textures.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        log::info!("  paint '{}' textures: {}", p.name, names.join(", "));
    }
    // The resolved bindings — `group -> texture (tile)`. A group bound to a texture
    // that NO paint carries (directly or via the bodywork alias above) can't change
    // with the paint dropdown — e.g. the exhaust/plate when a paint only ships the
    // bodywork map; this log is how you tell that apart from a real binding bug.
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
        // Bound memory: each cached bike holds its full geometry + textures.
        if c.len() >= 6 {
            c.clear();
        }
        c.insert(key, model.clone());
    }
    Ok(model)
}

/// Resolve which texture every mesh group binds to, from the bike's own files —
/// never from the group's name.
///
/// The rules, all read off real bikes rather than inferred:
///
/// * **`gfx.cfg` overrides win.** A part section may redirect a named group:
///   `plate { texture = w_plate }`, `chain { name = chain  texture = chain }`. It
///   overrides only those few; everything else takes the default.
/// * **The default is the model's own primary diffuse** — the largest texture
///   packed into `model.edf` that isn't a normal/roughness map and isn't already
///   claimed by a `gfx.cfg` override. On the KTM 450 that texture is *named*
///   `plastics`; on the Honda CRF450R it's named `2021crf`.
/// * **A paint replaces a model texture of the same name.** That's the whole
///   mechanism: a `.pnt` supplies textures by name (`plastics`, `plastics_n`), and
///   the game binds by name.
/// * **UV tile ≥ 1 selects a further texture.** The Honda's exhaust is authored on
///   tile 1 and takes the next diffuse in the model (`exhaust_22`). Confirmed by
///   rendering; only the Honda-style exhaust exercises it (the KTM/TM/GasGas models
///   are entirely tile 0).
///
/// `part` is the `gfx.cfg` section this node belongs to (`chassis`, `steer`, …).
fn bind_textures(
    nodes: &mut [edf::EdfNode],
    edf_bytes: &[u8],
    gfx: &std::collections::HashMap<String, cfg::GfxPart>,
    node_part: &std::collections::HashMap<String, String>,
) {
    // Every texture name the model packs, in file order.
    let embedded = edf::embedded_textures(edf_bytes);
    // Names any gfx.cfg override claims — these are bound explicitly, so they must
    // not also be handed out as a default (the Honda's `w_plate` sits between
    // `2021crf` and `exhaust_22` in the file and would otherwise steal tile 1).
    let claimed: std::collections::HashSet<String> = gfx
        .values()
        .flat_map(|p| p.textures.values())
        .map(|t| t.to_ascii_lowercase())
        .collect();
    // Candidate default textures: colour maps only (`_n` normal / `_r` roughness
    // are not diffuse), unclaimed, biggest first — the body map is always the
    // largest thing in the model (4096² on every bike checked).
    let mut diffuse: Vec<&edf::EmbeddedTexture> = embedded
        .iter()
        .filter(|t| {
            let n = t.name.to_ascii_lowercase();
            !n.ends_with("_n") && !n.ends_with("_r") && !claimed.contains(&n)
        })
        .collect();
    diffuse.sort_by_key(|t| std::cmp::Reverse(t.width as u64 * t.height as u64));

    for n in nodes.iter_mut() {
        let part = node_part.get(&n.name.to_ascii_lowercase());
        let overrides = part.and_then(|p| gfx.get(p)).map(|p| &p.textures);
        // Whole-node fallback for a node whose submesh table didn't resolve (the
        // TM/Yamaha/Triumph forks and swingarms): there are no groups to bind, but
        // its UVs still address the body map, so bind the primary diffuse rather
        // than leave the part flat grey.
        n.texture = diffuse.first().map(|t| t.name.clone());
        for sm in n.submeshes.iter_mut() {
            let group = sm.name.to_ascii_lowercase();
            if let Some(tex) = overrides.and_then(|o| {
                // The override names a group, but the mesh may qualify it with the
                // part: `gfx.cfg`'s `steer { plate { texture = w_plate } }` targets
                // the group the Honda calls `steer_plate` (its chassis section's
                // twin targets a group named plainly `plate`). An override the mesh
                // has no group for would just be dead config, so accept a
                // `<part>_<group>` spelling too — but only that, nothing looser.
                o.get(&group)
                    .or_else(|| o.iter().find(|(g, _)| group.ends_with(&format!("_{g}"))).map(|(_, t)| t))
            }) {
                sm.texture = Some(tex.clone());
                continue;
            }
            // Tile 0 (or an unknown/straddling tile) → the primary diffuse.
            let slot = sm.uv_tile.filter(|&t| t > 0).unwrap_or(0) as usize;
            sm.texture = diffuse.get(slot).map(|t| t.name.clone());
        }
    }
}

/// A readable paint name from a `.pnt` file name (`stock.pnt` → `Stock`).
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

/// `.pnt` liveries installed under `<bike folder>/paints/` (name, bytes). The bike
/// folder is `source` itself (a dir) or `source` minus its `.pkz` extension.
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

/// Only the files the 3D viewer needs — the mesh, its textures, and paints —
/// skipping a bike's many megabytes of sound `.wav`s (which are decrypted +
/// inflated for nothing, and can OOM on a big bike).
/// Only the files the 3D viewer needs — the mesh, its textures, its paints, and
/// the bike's **plain-text configs** (`gfx.cfg` names each part's `.hrc` and its
/// texture overrides; each `.hrc` names that part's LOD lineup). Both ship
/// unencrypted inside the `.pkz`.
fn wanted_bike_file(name: &str) -> bool {
    let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    bn == "model.edf"
        || bn.ends_with(".tga")
        || bn.ends_with(".pnt")
        || bn.ends_with(".geom")
        || bn.ends_with(".cfg")
        || bn.ends_with(".hrc")
}

/// Gather a bike's on-disk files as `(name, bytes)` from a folder / `.pkz` / `.edf`.
fn gather_bike_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::{bail, Context};
    // A direct .edf file — geometry only.
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("edf")) {
        let bytes = std::fs::read(p).with_context(|| format!("read {p:?}"))?;
        return Ok(vec![("model.edf".to_string(), bytes)]);
    }
    // A packaged bike — decrypt/inflate only the mesh + textures, not the sounds.
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz")) {
        return pkz::read_selected(p, wanted_bike_file);
    }
    // An extracted bike folder: loose files if a model.edf is present, else the
    // sibling `<name>.pkz`.
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

/// One part of the rider preview. A **gear** part (`helmet`/`boots`/`protection`)
/// carries real `.edf` geometry + its paint; a **paint-only** part (`suit`/`gloves`)
/// has no mesh — its texture just tints the stand-in body. `part` is the slot the
/// viewer maps it onto.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderPart {
    part: String,
    nodes: Vec<edf::EdfNode>,
    textures: Vec<paint::PaintTexture>,
}

/// The rider's real 3D preview, assembled from a loadout: whichever gear the user
/// has installed (rendered from `.edf`) plus the suit/gloves paints.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderModel {
    parts: Vec<RiderPart>,
}

/// Load the rider preview for a loadout: resolve each rider slot against the
/// installed `mods/rider` content, decode the real gear meshes + their paints, and
/// return the suit/gloves paints for the stand-in body. Reuses the same
/// `.edf`/`.pnt` pipeline as the bike side — gear is authored in the identical
/// format. Missing/unset slots are simply omitted (never an error), so the viewer
/// falls back to its stand-in for those.
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

    // Gear: the installed model the loadout names, else the game's stock set, each
    // with its selected (or stock) paint.
    for spec in &GEAR {
        let (model, paint) = match spec.part {
            "helmet" => (&loadout.helmet, &loadout.helmet_paint),
            "boots" => (&loadout.boots, &loadout.boots_paint),
            _ => (&loadout.protection, &loadout.protection_paint),
        };
        if let Some(p) = load_gear(&cfg, &base, spec, model, paint) {
            parts.push(p);
        }
    }

    // The outfit (suit) paint — its textures (`rider`/`rider_n`/`rider_r`).
    let suit = load_rider_paint(&base, "suit", &loadout.rider, "paints", &loadout.suit_paint);
    // Prefer the real rider BODY mesh from the game's `rider.pkz`, textured with
    // the outfit; if it can't be loaded (no game path / not found), fall back to
    // the outfit as a paint-only slot that tints the stand-in body.
    let suit_texs = suit.as_ref().map(|s| s.textures.clone()).unwrap_or_default();
    match load_rider_body(&cfg, &loadout.rider, suit_texs) {
        Some(body) => parts.push(body),
        None => {
            if let Some(s) = suit {
                parts.push(s);
            }
        }
    }

    if let Some(p) = load_rider_paint(&base, "gloves", &loadout.rider, "gloves", &loadout.gloves_paint)
    {
        parts.push(p);
    }

    Ok(RiderModel { parts })
}

/// Just the core rider **body** mesh nodes for a profile (from the game's
/// `rider.pkz`), for the Library outfit viewer — which already has the paint and
/// only needs geometry to skin. Empty vec when the game path isn't set / not found.
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

/// Load the core rider **body** mesh for a profile from the game's `rider.pkz`
/// (`rider/riders/<profile>/rider.edf`), carrying the outfit `textures` so the
/// viewer maps the suit onto it. `None` when the game path isn't set, the archive
/// isn't found, or the mesh doesn't decode — the viewer then keeps its stand-in.
fn load_rider_body(
    cfg: &config::AppConfig,
    profile: &str,
    textures: Vec<paint::PaintTexture>,
) -> Option<RiderPart> {
    let nodes = load_rider_body_nodes(cfg, profile)?;
    Some(RiderPart {
        part: "body".into(),
        nodes,
        textures,
    })
}

/// Cache of rider-side meshes decoded out of the game's `rider.pkz`
/// (`key = pkz path + mtime + entry`). The archive is 105 MB and the meshes are
/// megabytes each, so re-reading them on every preview open (per slot change) is
/// what made the viewer feel slow.
fn pkz_mesh_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, Vec<edf::EdfNode>>>
{
    static C: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Vec<edf::EdfNode>>>,
    > = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Keep only the first (highest-detail) node of each name. Rider gear packs its
/// LODs as repeated node names in one `.edf` — the stock boots ship `boot_l`/`boot_r`
/// three times (1950 → 1141 → 131 triangles) — and without this they all render
/// stacked on top of each other. Nodes appear highest-detail first, so first-wins is
/// LOD0. Empty-named nodes are always kept (they're not an LOD series).
fn keep_lod0(nodes: &mut Vec<edf::EdfNode>) {
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| n.name.is_empty() || seen.insert(n.name.clone()));
}

/// Decode one rider-side mesh out of a `.pkz`, cached.
fn load_pkz_mesh(pkz: &std::path::Path, entry: &str) -> Option<Vec<edf::EdfNode>> {
    let key = format!("{}:{}", bike_cache_key(&pkz.to_string_lossy()), entry);
    if let Some(n) = pkz_mesh_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(n);
    }
    let data = read_pkz_entry(pkz, entry)?;
    let mut nodes = edf::parse(&data);
    // Convert rider-side meshes out of the game's left-handed frame, same as bikes:
    // otherwise the artwork renders mirrored (a helmet's "Red Bull"/"Oakley"/"Troy
    // Lee" text reads backwards). The gear fitting compensates for the flipped up-axis
    // via `GEAR_ROT` (see ModelViewer). This also fixes the inside-out lighting for
    // free — negating X makes the winding agree with the normals.
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

/// The rider body nodes for a profile, decoded once and cached.
fn load_rider_body_nodes(cfg: &config::AppConfig, profile: &str) -> Option<Vec<edf::EdfNode>> {
    let profile = if profile.is_empty() { "default_mx" } else { profile };
    let pkz = resolve_game_pkz(cfg, "rider.pkz")?;
    load_pkz_mesh(&pkz, &format!("rider/riders/{profile}/rider.edf"))
}

/// Resolve a core game file (e.g. `rider.pkz`) inside the configured MX Bikes
/// **install** directory. `None` if `game_path` is unset or the file is missing.
fn resolve_game_pkz(cfg: &config::AppConfig, name: &str) -> Option<std::path::PathBuf> {
    // 1. The configured install dir (Steam `…/common/MX Bikes`), if set.
    let gp = cfg.game_path.trim();
    if !gp.is_empty() {
        let p = std::path::Path::new(gp).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    // 2. Fallback: dropped next to the mods folder (`<mods_path>/rider.pkz`), so a
    //    core `.pkz` copied there is found without configuring the install dir.
    let p = std::path::Path::new(&cfg.mods_path).join(name);
    p.exists().then_some(p)
}

/// Read one entry (by exact forward-slashed path, case-insensitive) out of a
/// `.pkz`, decompressed. Fast path for a plain ZIP (pulls just that entry); falls
/// back to the full decrypt for an encrypted archive. `None` if absent.
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

/// Load an installed gear item straight from its Library path — an extracted
/// folder **or** a packaged `.pkz` (decrypted when needed) — so it can be previewed
/// on its own. This is what makes a helmet/boots mod viewable without a game
/// profile. `part` is the viewer slot to fill (`helmet`/`boots`/`protection`).
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

/// Preview a **loose gear paint** (a `.pnt` for a helmet/boots/protection with no
/// model installed) on the game's **stock** model for that slot. This is the gear
/// analogue of the rider-outfit preview: the boot/helmet mesh comes from `rider.pkz`,
/// the paint from the loose file. When the paint was made for a *different* model its
/// texture name won't match the stock one — the frontend then force-applies it, so
/// the paint is still visible (UVs may not line up perfectly). `paint_path` empty →
/// the stock paint, so the model is never bare.
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
        // The chosen loose paint, else the stock paint so the model isn't bare.
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

/// The paint sets a gear item ships, for the viewer's pickers: its `paints/`
/// entries, plus a helmet's separate `goggles/` entries (a different texture on the
/// goggles submesh). Gear usually installs packaged, so these aren't loose files.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GearPaints {
    paints: Vec<String>,
    goggles: Vec<String>,
}

#[tauri::command]
async fn list_gear_paints(path: String) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = read_gear_files(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))?;
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
    })
    .await
    .map_err(|e| format!("list_gear_paints task failed: {e}"))?
}

/// `…/<folder>/Red White.pnt` → `Red White`, when `entry` is a `.pnt` in that
/// subfolder. Used for both the helmet skin (`paints`) and its goggles (`goggles`).
fn gear_folder_paint_name(entry: &str, folder: &str) -> Option<String> {
    let n = entry.replace('\\', "/").to_ascii_lowercase();
    if !n.contains(&format!("/{folder}/")) {
        return None;
    }
    let base = entry.replace('\\', "/");
    let base = base.rsplit('/').next()?;
    let stem = base.strip_suffix(".pnt").or_else(|| base.strip_suffix(".PNT"))?;
    (!stem.is_empty()).then(|| stem.to_string())
}

/// The paint's primary diffuse texture name — the first that isn't a `_n` normal or
/// `_r` roughness map. This is what a submesh binds to (a `.pnt` replaces a model
/// texture by this name).
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
    // The two diffuse names a submesh can bind to: the helmet/boots skin, and (for a
    // helmet) the separate goggles skin. Resolved from whichever paint we decode.
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
                // Un-mirror out of the game's left-handed frame so the gear's artwork
                // reads correctly (see `to_right_handed`); GEAR_ROT compensates for the
                // flipped up-axis, and the winding then agrees with the normals.
                edf::to_right_handed(&mut nodes);
                keep_lod0(&mut nodes);
            }
        } else if let Some(pname) = gear_folder_paint_name(name, "paints") {
            // Decode only the chosen paint (or the first, when none is named) — gear
            // ships several 2048² sets and decoding them all stalls the viewer.
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
    // Bind each submesh to its texture: the goggles submesh wears the goggle skin,
    // everything else the main skin. Name-based so it holds across mods (the goggles
    // submesh is conventionally named `goggles`).
    for node in &mut nodes {
        for sm in &mut node.submeshes {
            let is_goggle = sm.name.to_ascii_lowercase().contains("goggle");
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

/// A gear item's files, from an extracted folder or a packaged `.pkz`.
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
        // A gear folder keeps its paints in `paints/`.
        if let Ok(rd) = std::fs::read_dir(p.join("paints")) {
            for entry in rd.flatten() {
                let path = entry.path();
                if let (Some(name), Ok(bytes)) =
                    (path.file_name().and_then(|n| n.to_str()), std::fs::read(&path))
                {
                    out.push((format!("paints/{name}"), bytes));
                }
            }
        }
        return Ok(out);
    }
    pkz::read_all(p)
}

/// Where one rider gear slot lives — in the installed-mods tree and in the game's
/// core `rider.pkz`, which supplies the **stock** set when the loadout doesn't name
/// an installed one (so the rider is never missing a helmet/boots).
struct GearSpec {
    /// Viewer slot (see `RiderPart::part`).
    part: &'static str,
    /// `mods/rider/<mods_kind>/<name>` — the installed-mod tree.
    mods_kind: &'static str,
    /// `rider/<pkz_kind>/<name>` inside `rider.pkz` (note: `protections`, plural).
    pkz_kind: &'static str,
    /// Gear meshes are named per kind — they don't use the bikes' `model.edf`.
    mesh: &'static str,
    /// The stock set to fall back on.
    default_name: &'static str,
}

const GEAR: [GearSpec; 3] = [
    GearSpec { part: "helmet", mods_kind: "helmets", pkz_kind: "helmets", mesh: "helmet.edf", default_name: "default" },
    GearSpec { part: "boots", mods_kind: "boots", pkz_kind: "boots", mesh: "boots.edf", default_name: "default" },
    GearSpec { part: "protection", mods_kind: "protection", pkz_kind: "protections", mesh: "armour.edf", default_name: "full" },
];

/// Load one gear slot: the installed model the loadout names, else the game's stock
/// set out of `rider.pkz`. Gear is rider-side content, so it's strip-decoded.
/// `None` only when neither source has it.
fn load_gear(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    spec: &GearSpec,
    model: &str,
    paint: &str,
) -> Option<RiderPart> {
    // 1. An installed gear mod — either an extracted folder or a packaged `.pkz`
    //    sitting in the kind folder (which is how most gear actually installs).
    if !model.is_empty() {
        let kind_dir = base.join(spec.mods_kind);
        let stem = model.trim_end_matches(".pkz");
        for src in [kind_dir.join(stem), kind_dir.join(format!("{stem}.pkz"))] {
            if !src.exists() {
                continue;
            }
            // The loadout's chosen paint is resolved inside (it may live in the
            // packaged `.pkz` rather than as a loose file).
            if let Ok(part) = load_gear_model_blocking(
                src.to_string_lossy().into_owned(),
                spec.part.to_string(),
                Some(paint.to_string()),
                None, // loadout has no goggle choice → default goggle paint
            ) {
                return Some(part);
            }
        }
    }
    // 2. The game's stock gear.
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

/// Decode a gear paint out of `rider.pkz`: the named one, else the folder's first
/// stock `.pnt` so default gear still gets factory colours.
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

/// First entry under `prefix` with `ext`, decompressed (plain-ZIP `.pkz` only).
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

/// Load a paint-only rider slot (suit/gloves) from a profile: decode
/// `<base>/riders/<profile>/<sub>/<paint>.pnt` into textures (no mesh).
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

/// Read a paint `.pnt` from `dir`: the named `<paint>.pnt` if given, else the first
/// `.pnt` in the folder (the stock livery). Returns `None` if nothing matches.
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

/// Move an installed mod file into a different folder under its type dir.
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

/// Move an installed mod file to the OS Recycle Bin / Trash.
#[tauri::command]
async fn uninstall_mod(app: tauri::AppHandle, from_path: String, subpath: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        library::uninstall_mod(&cfg.mods_path, &from_path, &subpath).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("uninstall_mod task failed: {e}"))?
}

/// Reveal an installed mod file in the OS file manager.
#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

/// Set the MX Bikes **install** directory (holds core `rider.pkz`), so the 3D
/// viewer can load the real rider body model. Distinct from `mods_path`.
#[tauri::command]
fn set_game_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.game_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
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
///
/// FrostMod's `.exe`/`.dll` are locked by Windows while it runs, so an in-place
/// update would fail with "file in use". We stop any running FrostMod first
/// (including an instance from a previous session), overwrite, then restart it if
/// it was running so the update is seamless.
#[tauri::command]
async fn frostmod_install(
    app: tauri::AppHandle,
    state: State<'_, FrostmodProcess>,
) -> Result<String, String> {
    let was_running = frostmod::is_running();
    let was_installed = frostmod_manage::is_installed(&app);
    // Release the file locks before overwriting.
    frostmod_manage::stop(&state);
    frostmod_manage::force_stop_exe();

    let tag = frostmod_manage::install(&app).await.map_err(|e| format!("{e:#}"))?;

    // Bring FrostMod back up if we just took it down for an update, or start it
    // for a first-time install. (The frontend no longer starts it, so this is the
    // single place that does — avoids a double-spawn race.)
    if was_running || !was_installed {
        let _ = frostmod_manage::start(&app, &state);
    }
    Ok(tag)
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

/// Toggle instant-refresh: re-run the game's profile loader in place after
/// applying a preset so the look updates live (Windows-only).
#[tauri::command]
fn set_instant_refresh(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.instant_refresh = enabled;
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

    // Open the WordPress login form directly (not the downloads page, which only
    // shows a "log in" prompt). `redirect_to` sends the user to their downloads
    // once authenticated; the cookie poller below captures the session either way.
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
    // Route by mod type: a structured archive self-routes by its folders in
    // `place_mod`; for locked/structure-less content this picks the fallback
    // bucket from the item's title instead of always assuming tracks.
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

// --- Customization presets (per-bike loadouts) -----------------------------

/// App-local dir where `presets.json` lives (next to `config.json`).
fn presets_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("{e:#}"))
}

/// Rider/game profiles that have a `profile.ini` (each keeps its own per-bike look).
#[tauri::command]
fn presets_list_profiles(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(presets::list_profiles(&cfg.mods_path))
}

/// Bike ids present in a profile (the targets a loadout can be applied to).
#[tauri::command]
fn presets_list_bikes(app: tauri::AppHandle, profile: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    presets::list_bikes(&cfg.mods_path, &profile).map_err(|e| format!("{e:#}"))
}

/// Read a bike's current cosmetic column (for "capture current look"), including
/// its active model swap when it's a real captured variant (not the untouched
/// Original) — so a preset can carry the bike's current model too.
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

/// What happened when a preset was applied, so the UI can say precisely how it
/// took effect. `content_reload` is FrostMod re-scanning the mods folder (makes
/// new paint *files* available); it does **not** refresh the *selected* look,
/// which the game holds in memory. `game_running` drives the "reselect your
/// profile to load it" hint, and `live_refresh` reports the experimental attempt
/// to re-run the game's profile loader in place.
#[derive(serde::Serialize)]
struct PresetApplyOutcome {
    content_reload: ReloadOutcome,
    game_running: bool,
    live_refresh: gameproc::LiveRefresh,
}

/// Apply a loadout to a bike (writes its row across all slot sections; optionally
/// makes it the active bike), nudge a running FrostMod to reload the mods folder,
/// and — when the `instant_refresh` setting is on — re-run the game's profile
/// loader in the live process so the new look shows without a restart or manual
/// reselect. The returned outcome tells the UI exactly how it took effect.
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
    // A model swap (Locker) is a filesystem move, not a profile.ini value — apply
    // it when the preset carries one and it isn't already the bike's active model.
    let want = loadout.model_swap.trim();
    if !want.is_empty() && !want.eq_ignore_ascii_case(&modelswap::current_active(&cfg.mods_path, &bikeid))
    {
        modelswap::apply_model_swap(&cfg.mods_path, &bikeid, want)
            .map_err(|e| format!("Cosmetics applied, but the model swap failed: {e:#}"))?;
    }
    // FrostMod reload only refreshes mods *content*, not the in-memory look. The
    // look re-reads from profile.ini only when the game (re)selects a profile —
    // so we detect the game and, if asked, re-run its loader in place.
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

/// All saved presets.
#[tauri::command]
fn presets_list(app: tauri::AppHandle) -> Result<Vec<presets::Preset>, String> {
    Ok(presets::load_presets(&presets_dir(&app)?))
}

/// Save (or overwrite by name) a preset.
#[tauri::command]
fn presets_save(app: tauri::AppHandle, preset: presets::Preset) -> Result<(), String> {
    presets::save_preset(&presets_dir(&app)?, preset).map_err(|e| format!("{e:#}"))
}

/// Delete a preset by name.
#[tauri::command]
fn presets_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    presets::delete_preset(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

/// Export a saved preset as a portable share code (`MXBP1-…`).
#[tauri::command]
fn presets_export(app: tauri::AppHandle, name: String) -> Result<String, String> {
    presets::export_code(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

/// Decode a share code *without* saving it — lets the UI preview a shared preset
/// (name + slots) and check for missing mods before importing.
#[tauri::command]
fn presets_decode(text: String) -> Result<presets::Preset, String> {
    presets::decode_code(&text).map_err(|e| format!("{e:#}"))
}

/// Import a share code: decode + save + return the stored preset.
#[tauri::command]
fn presets_import(app: tauri::AppHandle, text: String) -> Result<presets::Preset, String> {
    presets::import_code(&presets_dir(&app)?, &text).map_err(|e| format!("{e:#}"))
}

// --- Preset full-share bundles (assets + config uploaded/downloaded) --------

/// Preview what a preset's full bundle would carry: the resolved assets, the slots
/// that can't travel, and the (estimated) total size. Read-only — nothing uploads.
#[tauri::command]
fn preset_bundle_stats(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<bundle::BundlePlan, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    bundle::plan(&cfg, &loadout).map_err(|e| format!("{e:#}"))
}

/// Create a full-share code for a saved preset: package every asset it references,
/// upload the bundle to an anonymous host, and return the share code (with the
/// bundle link embedded). Progress arrives via `preset-bundle-progress`.
#[tauri::command]
async fn preset_bundle_create(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let dir = presets_dir(&app)?;
    bundle::create(&app, &cfg, &dir, &name)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Import a full-share code: download its asset bundle, install every file into the
/// game's `mods/`, and save the preset. Progress via `preset-bundle-progress` (+
/// byte-level `install-progress` for the download).
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
                // In dev, closing the window fully quits so `tauri dev` releases the
                // app + vite (otherwise the tray keeps them alive, orphaning the
                // process and squatting port 1420 → the next run can't bind).
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
    /// End-to-end over a REAL packaged bike: `.pkz` → `gfx.cfg` + `.hrc` + mesh →
    /// LOD choice + texture bindings. This is the one test that exercises the parts
    /// unit tests can't — that the configs survive the `.pkz`'s path flattening, and
    /// that basename matching finds them.
    ///
    /// `MXB_REAL_PKZ='…/mods/bikes/MX1OEM_2023_Honda_CRF450R.pkz' \
    ///   cargo test bike_model_from_pkz -- --ignored --nocapture`
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
        // Every group must resolve to a texture the paint actually carries —
        // otherwise the viewer renders it grey.
        let have: std::collections::HashSet<String> = m.paints[0]
            .textures
            .iter()
            .map(|t| t.name.to_ascii_lowercase())
            .collect();
        for n in &m.nodes {
            for s in &n.submeshes {
                let t = s.texture.as_ref().expect("group bound to a texture");
                assert!(have.contains(&t.to_ascii_lowercase()), "'{t}' is available");
            }
        }
    }

    /// End-to-end over a REAL packaged helmet: lists both paint sets, and checks the
    /// `goggles` submesh binds a DIFFERENT texture than the shell — and that both
    /// resolve to a texture actually shipped, so neither renders grey.
    ///
    /// `MXB_REAL_GEAR='…/TLD SE4 - Oakley Airbrake.pkz' \
    ///   cargo test gear_model_from_pkz -- --ignored --nocapture`
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

    /// The stock boots pack three LODs as repeated `boot_l`/`boot_r` nodes in one
    /// `.edf`; `keep_lod0` must collapse them to the two highest-detail boots so the
    /// preview doesn't render them stacked.
    ///
    /// `MXB_REAL_EDF='…/boots.edf' cargo test lod0_dedup_from_env -- --ignored --nocapture`
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

