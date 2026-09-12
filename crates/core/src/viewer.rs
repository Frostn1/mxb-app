//! Assembling a bike, a rider or a set of gear into something drawable.
//!
//! Both binaries need this: the manager draws an installed bike in the Locker, and the
//! studio draws the one being painted. What is here reads files and builds models; deciding
//! *which* files — resolving a model swap, planning an install — stays with the app that
//! owns that decision, and arrives as a `PreviewSet`.

use std::path::PathBuf;

/// The OS we're running on — `"windows"`, `"macos"`, `"linux"`.
///
/// The frontend used to infer this from `navigator.userAgent`, which can tell a Mac from
/// everything else and nothing more. Features that only exist on Windows (FrostMod, the
/// live in-game refresh) need to know the difference between Windows and Linux, so it
/// comes from the backend rather than adding `plugin-os` and a capability for one string.
///
/// Here rather than in either app because `@frost/shared` calls it, so both must answer it.
#[tauri::command]
pub fn app_platform() -> &'static str {
    std::env::consts::OS
}

use rayon::prelude::*;
use tauri::State;

use crate::paintwatch::PaintWatcher;
use crate::{
    bikefiles, cfg, cloudfiles, config, edf, game, gate, library, lru, paint, paintwatch,
    pkz, presets, texstore,
};

/// What the bike's files would look like with `variant` active — filenames only, nothing
/// read and nothing moved. Lets the viewer show a swap before it's applied.
#[derive(Debug, Clone)]
pub struct PreviewSet {
    pub bike_dir: PathBuf,
    /// Loose root files that stay put, i.e. the root minus the set the swap would park.
    pub root_keep: Vec<String>,
    /// The variant folder and the files it would bring in — empty for Stock, which brings
    /// in nothing and lets the packed model show through.
    pub variant_dir: PathBuf,
    pub variant_files: Vec<String>,
    /// The liveries this model would offer, as full paths — the ones it claims plus every
    /// unclaimed one. Resolved here rather than read back off `paints/`, which still holds
    /// the *active* model's set until the swap actually happens.
    pub paints: Vec<PathBuf>,
}


/// Raw RGBA for a texture the viewer was handed a token for.
///
/// Returns an `ipc::Response`, which travels as `application/octet-stream` and lands in the
/// webview as an `ArrayBuffer` — the pixels are never encoded, base64'd, or parsed as JSON
/// on the way. The frontend feeds the buffer straight to a `THREE.DataTexture`. `async`
/// keeps the copy off the main thread, as with every other command here.
#[tauri::command]
pub async fn texture_bytes(token: String) -> tauri::ipc::Response {
    tauri::ipc::Response::new(texstore::bytes_or_missing(&token))
}

/// Watch the paint files the 3D viewer is currently showing, replacing whatever it was
/// watching before. An empty list stops.
///
/// There is one viewer showing one paint, so this is a set-the-whole-thing call rather than
/// an add/remove pair: nothing can then leak a watch by forgetting to take one back.
#[tauri::command]
pub fn watch_paint_files(app: tauri::AppHandle, watcher: State<PaintWatcher>, paths: Vec<String>) {
    paintwatch::start(&app, &watcher, &paths);
}

#[tauri::command]
pub async fn unpack_pkz(path: String, out_dir: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_pkz_blocking(path, out_dir))
        .await
        .map_err(|e| format!("unpack_pkz task failed: {e}"))?
}

pub fn unpack_pkz_blocking(path: String, out_dir: String) -> Result<Vec<String>, String> {
    pkz::extract(std::path::Path::new(&path), std::path::Path::new(&out_dir))
        .map_err(|e| format!("{e:#}"))
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BikePaint {
    pub name: String,
    /// Where the `.pnt` sits on disk, for a paint installed loose in the bike's `paints`
    /// folder — the file the viewer watches so an edit re-dresses the model. `None` for a
    /// paint packed inside the archive: nothing rewrites one of those in place.
    pub path: Option<String>,
    pub textures: Vec<paint::PaintTexture>,
    pub changes_preview: bool,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BikeModel {
    pub nodes: Vec<edf::EdfNode>,
    pub paints: Vec<BikePaint>,
    /// The model's own textures — the look it ships with, before any paint replaces one.
    ///
    /// The same pixels the paints below already carry as fillers, said once on their own.
    /// Folded into a paint they're indistinguishable from that paint's own sheets, and the
    /// Designer's reference underlay needs the distinction: an OEM bike's stock `.pnt`
    /// replaces the wheels and the chain, so `plastics` is only ever in here.
    pub base: Vec<paint::PaintTexture>,
    /// The tyres mod the wheels came out of, or `None` when the bike drew none.
    ///
    /// What was *actually* fitted, not what was asked for: a pick that names nothing
    /// installed falls back to the bike's own, and the picker has to show that rather than
    /// claim a pack that isn't on screen.
    pub tyres: Option<String>,
    /// Whether the parts were placed into one frame by the bike's `.geom`.
    ///
    /// False means every node still sits in its own local frame, so a vertex's position says
    /// nothing about where it is on the bike. The Designer names the flank a sheet region
    /// paints from the sign of x, and that answer is only worth giving once this is true.
    pub assembled: bool,
    /// The joints this bike can be posed about, in the frame `nodes` came back in.
    ///
    /// `None` for a bike that wasn't assembled — there is nothing to pose a pile of parts
    /// that are each still in their own frame. See [`edf::BikeRig`] for why the viewer poses
    /// at all rather than drawing one settled stance.
    pub rig: Option<edf::BikeRig>,
}

impl BikeModel {
    /// Every texture token the model holds, so evicting it can free the pixels too.
    ///
    /// `base` as well as the paints, and not only because it is a field now: a base texture
    /// that *every* paint overrides is folded into none of them, and before this was dropped
    /// without ever being released. Duplicates are free — `texstore::release` removes by key.
    fn tokens(&self) -> Vec<String> {
        self.paints
            .iter()
            .flat_map(|p| p.textures.iter())
            .chain(self.base.iter())
            .map(|t| t.token.clone())
            .collect()
    }
}

/// Bikes are big (geometry plus every paint's pixels), so hold only the few most recent.
const BIKE_CACHE_CAP: usize = 3;

pub fn bike_cache() -> &'static std::sync::Mutex<lru::Lru<BikeModel>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<lru::Lru<BikeModel>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(BIKE_CACHE_CAP)))
}

/// The cached bike under `key`, if it's still resident. Taken and released in one step so
/// no caller holds the cache lock while it waits on [`gate::enter`]. A hit whose pixels the
/// texture store has since reaped is a miss: served, it would draw the bike grey.
pub fn cached_bike(key: &str) -> Option<BikeModel> {
    bike_cache()
        .lock()
        .ok()
        .and_then(|mut c| c.get(key).cloned())
        .filter(|m| texstore::all_resident(&m.tokens()))
}

pub fn mtime_nanos(path: &std::path::Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Cache key for whatever lives at `source`: its path, when it was last written, and how
/// big it is.
///
/// Size is in there for the viewer's live reload. A paint being re-saved every few seconds
/// is the one caller that rewrites a file under the cache, and mtime alone would serve it
/// stale pixels on any filesystem whose timestamps are coarser than the gap between two
/// saves — FAT32 rounds to two seconds. A recompressed `.pnt` almost never comes back the
/// same length.
pub fn bike_cache_key(source: &str) -> String {
    let path = std::path::Path::new(source);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    format!("{source}:{}:{size}{}", mtime_nanos(path), packed_stamp(path))
}

/// The bike's packed archive, as a cache key fragment. It's a real input to every bike now
/// — the loose folder layers over it — and updating a bike replaces the `.pkz` without
/// necessarily touching the folder above it.
pub fn packed_stamp(bike_dir: &std::path::Path) -> String {
    if !bike_dir.is_dir() {
        return String::new(); // the source *is* the archive; its own mtime is already in the key
    }
    match packed_bike(bike_dir) {
        Some(pkz) => {
            let size = std::fs::metadata(&pkz).map(|m| m.len()).unwrap_or(0);
            format!("#z{}:{size}", mtime_nanos(&pkz))
        }
        None => String::new(),
    }
}

/// A swap preview is keyed by both folders it's built from — the same bike renders
/// differently per variant, and either side can change under us.
pub fn swap_cache_key(set: &PreviewSet) -> String {
    // The livery list is part of what a preview shows, and an assignment edit changes it
    // without touching either folder — so key on the resolved paths, not just the mtimes.
    let paints: Vec<String> = set.paints.iter().map(|p| p.display().to_string()).collect();
    format!(
        "{}#{}:{}:{}:{}{}",
        set.bike_dir.display(),
        set.variant_dir.display(),
        mtime_nanos(&set.bike_dir),
        mtime_nanos(&set.variant_dir),
        paints.join(","),
        packed_stamp(&set.bike_dir),
    )
}

#[tauri::command]
pub async fn load_bike_model(source: String, tyres: Option<String>) -> Result<BikeModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_bike_model_blocking(source, tyres))
        .await
        .map_err(|e| format!("load_bike_model task failed: {e}"))?
}


/// Draw the bike a [`PreviewSet`] describes.
///
/// Split from the command above so the viewer can be handed a set by anything that can
/// build one — a model swap today, and whatever the studio previews later.
pub fn load_preview_blocking(
    set: &PreviewSet,
    label: &str,
    tyres: std::path::PathBuf,
    pick: Option<String>,
) -> Result<BikeModel, String> {
    let t0 = std::time::Instant::now();
    let key = format!(
        "{}#p{:x}#t{:x}#w{}",
        swap_cache_key(set),
        paints_stamp(&set.bike_dir),
        tyres_stamp(&tyres),
        pick.as_deref().unwrap_or(""),
    );
    if let Some(m) = cached_bike(&key) {
        log::info!("preview_model_swap {label}: cache hit ({:?})", t0.elapsed());
        return Ok(m);
    }
    // Somebody may already be building this exact preview — the panel and a dialog can both
    // be drawing it. Wait for them and take their answer instead of paying a second time.
    let _gate = gate::enter(&key);
    if let Some(m) = cached_bike(&key) {
        log::info!("preview_model_swap {label}: cache hit, waited ({:?})", t0.elapsed());
        return Ok(m);
    }

    let files = gather_preview_files(set).map_err(|e| format!("{e:#}"))?;
    let installed = paints_at(&set.paints);
    build_bike_model(label, key, files, installed, Some(tyres), pick, t0)
}

/// A stamp over the loose paints beside a bike, for the cache key to carry.
///
/// The bike's own file can't see them. A `.pnt` is written into `<bike>/paints/`, which
/// leaves the `.pkz`'s mtime and size exactly as they were — and on a bike loaded from its
/// folder, writing a file inside `paints/` doesn't touch the folder above it either. So the
/// key matched, the cache answered, and the model handed back was the one read before the
/// paint existed: you saved, the bike didn't change, and nothing in the log looked wrong.
///
/// Name, length and mtime per `.pnt`, sorted so `read_dir` order can't shuffle the answer.
/// Covers all three ways the set can move — a paint added, removed, or re-saved in place.
pub fn paints_stamp(source: &std::path::Path) -> u64 {
    let folder = if source.is_dir() {
        source.to_path_buf()
    } else {
        source.with_extension("")
    };
    let mut rows: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(folder.join("paints")) {
        for e in entries.flatten() {
            let path = e.path();
            if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("pnt")) {
                continue;
            }
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            rows.push(format!(
                "{}:{len}:{}",
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                mtime_nanos(&path),
            ));
        }
    }
    rows.sort_unstable();
    fnv1a(&rows)
}

/// A stamp over the installed tyre mods, for the cache key to carry.
///
/// The hole [`paints_stamp`] fills, one folder out. A bike's wheels come from
/// `mods/tyres/<name>`, which nothing on the bike's own path can see: swapping that mod
/// changes what the viewer should draw while the bike's mtime, size and paints all stay
/// exactly as they were.
pub fn tyres_stamp(dir: &std::path::Path) -> u64 {
    let mut rows: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            rows.push(format!(
                "{}:{len}:{}",
                e.file_name().to_string_lossy(),
                mtime_nanos(&e.path()),
            ));
        }
    }
    rows.sort_unstable();
    fnv1a(&rows)
}

/// FNV-1a over the rows a stamp is built from. Not a security question — this only has to
/// change when the folder does.
pub fn fnv1a(rows: &[String]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for row in rows {
        for b in row.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    }
    h
}

pub fn load_bike_model_blocking(
    source: String,
    pick: Option<String>,
) -> Result<BikeModel, String> {
    let t0 = std::time::Instant::now();
    let tyres = tyres_dir_for(std::path::Path::new(&source));
    let key = format!(
        "{}#p{:x}#t{:x}#w{}",
        bike_cache_key(&source),
        paints_stamp(std::path::Path::new(&source)),
        tyres.as_deref().map(tyres_stamp).unwrap_or(0),
        pick.as_deref().unwrap_or(""),
    );
    if let Some(m) = cached_bike(&key) {
        log::info!("load_bike_model {source}: cache hit ({:?})", t0.elapsed());
        return Ok(m);
    }
    let _gate = gate::enter(&key);
    if let Some(m) = cached_bike(&key) {
        log::info!("load_bike_model {source}: cache hit, waited ({:?})", t0.elapsed());
        return Ok(m);
    }

    let files = gather_bike_files(std::path::Path::new(&source)).map_err(|e| format!("{e:#}"))?;
    let installed = installed_paints(std::path::Path::new(&source));
    build_bike_model(&source, key, files, installed, tyres, pick, t0)
}

/// Why a bike came back with nothing to draw, in words the player can act on.
///
/// Three unrelated faults land here and they want three different answers: a mesh that never
/// arrived, a mesh whose bytes aren't a mesh, and a mesh that read but wouldn't come apart.
/// Blaming cloud sync for all three sent a player hunting through their OneDrive settings for
/// what turned out to be a protected model the viewer wasn't unwrapping.
pub fn no_mesh_reason(label: &str, meshes: &[(&str, &[u8])]) -> String {
    if meshes.iter().all(|(_, b)| b.is_empty()) {
        return format!(
            "{label} holds no readable mesh — if the file is cloud-synced, it may not be fully downloaded yet"
        );
    }
    if !meshes.iter().any(|(_, b)| edf::is_edf(b)) {
        return format!(
            "{label}'s mesh didn't decode — the file may be damaged, or protected in a way this version can't open"
        );
    }
    format!("{label}'s mesh read but no parts came out of it — the model may be built in a way the viewer doesn't handle yet")
}

/// Turn a bike's files into the viewer's model: resolve each part's mesh through the
/// `.hrc`s, bind its textures, decode the paints. Shared by a bike loaded from disk and a
/// model-swap preview assembled in memory — `label` only names it in the log.
pub fn build_bike_model(
    label: &str,
    key: String,
    files: Vec<(String, Vec<u8>)>,
    // The loose paints beside the bike, as `installed_paints` answers them.
    installed: Vec<(String, String, Vec<u8>)>,
    // Where the tyre mods live, for the wheels this bike wears. `None` skips them.
    tyres_dir: Option<std::path::PathBuf>,
    // The tyre pack the player picked, if any. Blank/absent → the one the bike names.
    tyres_pick: Option<String>,
    t0: std::time::Instant,
) -> Result<BikeModel, String> {
    let t_read = t0.elapsed();

    let mut nodes = Vec::new();
    // Every mesh the bike ships, by file name — usually just `model.edf`, but a bike can
    // carry one per part. Which are actually used is decided by the `.hrc`s below.
    let mut edfs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut geom: Option<&Vec<u8>> = None;
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut tga_jobs: Vec<(String, &[u8])> = Vec::new();
    // (display name, bytes, shipped-in-the-archive, path on disk if it has one)
    let mut pnt_jobs: Vec<(String, &[u8], bool, Option<&str>)> = Vec::new();
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
            pnt_jobs.push((paint_display_name(&bn), data.as_slice(), true, None));
        }
    }

    let gfx = gfx_bytes.map(|b| cfg::parse_gfx(b)).unwrap_or_default();
    // Read before `used` borrows anything, so the wheel meshes outlive the borrows taken of
    // them below. `edfs` is only consulted so an unreadable bike doesn't pay for a tyre
    // archive it will never draw.
    let tyre_set = match (tyres_dir, gfx_bytes, geom) {
        (Some(dir), Some(bytes), Some(g)) if !edfs.is_empty() => {
            if edf::wheel_axles(g).is_some() {
                gather_tyre_files(&dir, bytes, tyres_pick.as_deref())
            } else {
                log::warn!("[viewer] {label}: the .geom names no axles — no wheels");
                None
            }
        }
        _ => None,
    };
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
    // Wheels last, and only onto a bike that arrived: a mesh that didn't read has to go on
    // reading as "none of this bike arrived", not as a pair of wheels hanging in the air.
    let mut tyres = None;
    if let Some(set) = tyre_set.as_ref().filter(|_| !nodes.is_empty()) {
        let (mut wheels, meshes) = wheel_nodes(&set.files);
        if wheels.is_empty() {
            log::warn!("[viewer] {label}: tyres '{}' hold no readable wheel mesh", set.name);
        } else {
            tyres = Some(set.name.clone());
        }
        nodes.append(&mut wheels);
        used.extend(meshes);
    }
    for (fname, path, data) in &installed {
        pnt_jobs.push((paint_display_name(fname), data.as_slice(), false, Some(path)));
    }
    // Whether the parts ended up in one frame. Logged rather than printed: it decides what the
    // Designer may say about a sheet's flanks, so "was this bike assembled?" has to be
    // answerable from the log file after the fact, not only from a terminal nobody kept.
    let mut rig = match geom {
        Some(g) => {
            let rig = edf::assemble_bike(&mut nodes, g);
            if rig.is_none() {
                log::warn!("[viewer] {label}: .geom present but missing mount points — parts unassembled");
            }
            rig
        }
        None => {
            if !nodes.is_empty() {
                log::warn!("[viewer] {label}: no .geom alongside the mesh — parts unassembled");
            }
            None
        }
    };
    let assembled = rig.is_some();
    edf::to_right_handed(&mut nodes);
    // The rig names points on the mesh, so it goes through the same mirror the mesh does.
    if let Some(r) = rig.as_mut() {
        r.to_right_handed();
    }
    // Nothing to draw. Returning a model with no nodes is worse than failing: the viewer reads
    // it as a successful load and puts its stand-in bike on screen, which reads as "this is your
    // bike" rather than "none of this bike arrived".
    if nodes.is_empty() {
        let mut meshes: Vec<(&str, &[u8])> =
            edfs.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        meshes.sort_unstable_by_key(|(n, _)| *n);
        // What the bytes were is the whole question, and until now this path said nothing at
        // all — a report of it could only be guessed at.
        for (name, bytes) in &meshes {
            log::warn!(
                "[viewer] {label}: {name} read as {} byte(s), header {}",
                bytes.len(),
                if edf::is_edf(bytes) { "ok — but nothing parsed out of it" } else { "not a mesh" }
            );
        }
        return Err(no_mesh_reason(label, &meshes));
    }
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
    // ...and the normal map belonging to each of them, which is what gives a shroud its
    // curve and a seat its grip in the preview. Only for sheets something actually draws:
    // a bike embeds normals for parts it no longer uses, and each one is a megabyte of the
    // texture store spent on pixels nothing hangs off.
    let drawn: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| n.texture.iter().chain(n.submeshes.iter().filter_map(|s| s.texture.as_ref())))
        .map(|t| t.to_ascii_lowercase())
        .collect();
    for data in &used {
        for tex in paint::extract_edf_normal_maps(data, |base_name| drawn.contains(base_name)) {
            if seen.insert(tex.name.to_ascii_lowercase()) {
                base.push(tex);
            }
        }
    }
    let mut paints: Vec<(BikePaint, bool)> = pnt_jobs
        .par_iter()
        .filter_map(|(name, data, shipped, path)| {
            // Uninflated: the viewer fetches the sheets of the paint it shows, not all of them.
            paint::store_lazy_any(data).ok().map(|textures| {
                (
                    BikePaint {
                        name: name.clone(),
                        path: path.map(str::to_string),
                        textures,
                        changes_preview: false, // resolved below, once bindings are known
                    },
                    *shipped,
                )
            })
        })
        .collect();
    let base_count = base.len();
    let t_textures = t0.elapsed();

    let bound = &drawn;
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

    // Kept before the folding below, which is where the model's own look stops being
    // telling apart from a paint's. Names and tokens only — no pixels are copied.
    let model_base = base.clone();

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
            // The mesh's own textures, which live inside it rather than in a `.pnt`.
            path: None,
            textures: base,
            changes_preview: true, // the model's own textures, by definition
        });
    }

    let distinct_tex: std::collections::HashSet<&str> = paints
        .iter()
        .flat_map(|p| p.textures.iter().map(|t| t.token.as_str()))
        .collect();
    // The phase split, on stdout, for the `bike_load_timing` diagnostic — `log` has no
    // subscriber under `cargo test`, and this is the breakdown that says where to optimise.
    if std::env::var_os("MXB_PHASE_TIMES").is_some() {
        println!(
            "  parse mesh           {:>9.2?}\n  decode paints        {:>9.2?}  ({} paint(s), {base_count} base tex)",
            t_parse - t_read,
            t_textures - t_parse,
            paints.len(),
        );
    }
    log::info!(
        "load_bike_model {label}: {} paint(s) + {base_count} base tex | read {t_read:?}, parse {:?}, decode {:?}, total {:?} | {} distinct texture(s), {:.1} MB resident in the texture store",
        paints.len(),
        t_parse - t_read,
        t_textures - t_parse,
        t0.elapsed(),
        distinct_tex.len(),
        texstore::resident_bytes() as f64 / (1024.0 * 1024.0),
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

    let model = BikeModel { nodes, paints, base: model_base, tyres, assembled, rig };
    if let Ok(mut c) = bike_cache().lock() {
        // The pixels of whatever this displaced go with it — evicted or replaced in place,
        // nothing else references them; tokens are minted per build.
        if let Some(dropped) = c.insert(key, model.clone()) {
            texstore::release(&dropped.tokens());
        }
    }
    Ok(model)
}

pub fn bind_textures(
    nodes: &mut [edf::EdfNode],
    edf_bytes: &[u8],
    gfx: &std::collections::HashMap<String, cfg::GfxPart>,
    node_part: &std::collections::HashMap<String, String>,
) {
    // Which list this mesh's material indices count. A mesh whose materials never use the
    // second texture slot is read exactly as it always was; only one that does — a mod
    // shipping companion maps, unreadable until now — gets the companion-aware list.
    let colors = if edf::uses_companion_slots(edf_bytes) {
        edf::bike_material_slots(edf_bytes)
    } else {
        edf::declared_colors(edf_bytes, &[])
    };

    for n in nodes.iter_mut() {
        let part = node_part.get(&n.name.to_ascii_lowercase());
        let overrides = part.and_then(|p| gfx.get(p)).map(|p| &p.textures);
        if n.materials.is_empty() {
            log::warn!("[viewer] node '{}' has no material table — falling back", n.name);
        }
        // A material id is local to its node, so ask the node's own table.
        let material_texture = |mat: Option<u32>| -> Option<&String> {
            let slot = n.materials.get(mat? as usize).copied().flatten()?;
            colors.get(slot)
        };
        // A node with no submesh table draws on its first material.
        n.texture = material_texture(Some(0)).or_else(|| colors.first()).cloned();
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
            // 2. The node's material table picks the colour texture this range was drawn on.
            if let Some(t) = material_texture(sm.mat) {
                sm.texture = Some(t.clone());
                continue;
            }
            // 3. No material recorded → leave unbound so it renders neutral grey, never smeared.
            sm.texture = None;
        }
    }
}

pub fn paint_display_name(file_name: &str) -> String {
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

/// The loose `.pnt`s installed beside a bike, as (file name, full path, bytes).
///
/// The path rides along because these are the only paints that can change under the viewer:
/// they are ordinary files a painter re-saves, where the ones inside the archive are not.
/// Read an already-resolved livery list. A preview's liveries come from
/// `modelswap::PreviewSet`, which knows the shelf — reading `paints/` directly would show
/// whatever the model *currently* on the bike offers, not the one being previewed.
pub fn paints_at(paths: &[std::path::PathBuf]) -> Vec<(String, String, Vec<u8>)> {
    paths
        .iter()
        .filter_map(|p| {
            let name = p.file_name().and_then(|n| n.to_str())?.to_string();
            let full = p.to_str()?.to_string();
            Some((name, full, std::fs::read(p).ok()?))
        })
        .collect()
}

pub fn installed_paints(source: &std::path::Path) -> Vec<(String, String, Vec<u8>)> {
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
                if let (Some(name), Some(full), Ok(bytes)) = (
                    p.file_name().and_then(|n| n.to_str()),
                    p.to_str(),
                    std::fs::read(&p),
                ) {
                    out.push((name.to_string(), full.to_string(), bytes));
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
pub fn base_edf<'a>(
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

/// The `tyres` folder beside a bike, where the wheels it wears come from.
///
/// A bike source is `<mods>/bikes/<Bike>` or `<mods>/bikes/<Bike>.pkz`, so the sibling
/// folder is two levels up. Derived rather than configured: `load_bike_model` is handed a
/// path and nothing else, and that path already says where the mods tree is.
pub fn tyres_dir_for(source: &std::path::Path) -> Option<std::path::PathBuf> {
    // Resolved, not joined: under Proton the tree is case-sensitive and a `Tyres` folder is
    // a different path from `tyres`.
    Some(library::resolve_child(source.parent()?.parent()?, "tyres"))
}

/// Whether `mods/tyres/<name>` is installed, as a folder or as the `.pkz` beside it.
pub fn tyres_mod_exists(tyres_dir: &std::path::Path, name: &str) -> bool {
    library::resolve_child(tyres_dir, name).is_dir()
        || library::resolve_child(tyres_dir, &format!("{name}.pkz")).is_file()
}

/// A tyres mod, opened: the name it goes by and the files a wheel resolves through.
pub(crate) struct TyreSet {
    name: String,
    files: Vec<(String, Vec<u8>)>,
}

/// Open the tyres mod a bike will wear — its own `gfx.cfg`, an `.hrc` per wheel, and the
/// meshes those name.
///
/// A bike ships no wheel of its own. Its `gfx.cfg` ends with one line — `tyres = oem_mx` —
/// and `mods/tyres/oem_mx`, a folder or the `.pkz` beside it, is where the mesh actually
/// lives. `pick` substitutes that name so a bike can be *seen* on another pack; nothing on
/// disk moves and the bike's own `gfx.cfg` still reads as the game will read it.
///
/// `None` when there is no line, no mod, or nothing readable in one — the bike the viewer
/// drew before wheels, not a failure.
pub(crate) fn gather_tyre_files(
    tyres_dir: &std::path::Path,
    gfx_bytes: &[u8],
    // The pack the player picked, if they picked one. Blank or absent → the bike's own.
    pick: Option<&str>,
) -> Option<TyreSet> {
    let root = cfg::parse(gfx_bytes);
    let own = root.get("tyres").map(str::trim).filter(|n| library::is_simple_name(n));
    // A pick that names nothing installed falls back to the bike's own rather than taking
    // the wheels away: the picker is a way to look at a bike, not a way to break it.
    let name = match pick.map(str::trim).filter(|n| !n.is_empty()) {
        Some(p) if library::is_simple_name(p) && tyres_mod_exists(tyres_dir, p) => p,
        Some(p) => {
            log::warn!("[viewer] tyres '{p}' isn't installed — falling back to the bike's own");
            own?
        }
        None => own?,
    }
    .to_string();

    // The `.tyre` parameter files, the previews and the shadow meshes all sit beside these
    // and none of them are drawn — read only what a wheel is resolved through.
    let want = |n: &str| {
        let n = n.rsplit(['/', '\\']).next().unwrap_or(n).to_ascii_lowercase();
        n.ends_with(".edf") || n.ends_with(".hrc") || n.ends_with(".cfg")
    };

    let dir = library::resolve_child(tyres_dir, &name);
    let mut loose = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let path = e.path();
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if path.is_file() && want(fname) {
                if let Ok(bytes) = std::fs::read(&path) {
                    loose.push((fname.to_string(), bytes));
                }
            }
        }
    }
    if !loose.is_empty() {
        return Some(TyreSet { name, files: loose });
    }

    let pkz = library::resolve_child(tyres_dir, &format!("{name}.pkz"));
    if !pkz.is_file() {
        log::warn!("[viewer] tyres '{name}' isn't installed — no wheels");
        return None;
    }
    match pkz::read_selected(&pkz, want) {
        Ok(files) => Some(TyreSet { name, files }),
        Err(e) => {
            log::warn!("[viewer] tyres '{name}' wouldn't read: {e:#} — no wheels");
            None
        }
    }
}

/// The wheel nodes out of a tyres mod, textured and ready for the `.geom` to mount.
///
/// Same shape as a bike's own parts — `gfx.cfg` names an `.hrc` per wheel, and the `.hrc`'s
/// level0 names both the node and the mesh it lives in — so the bike's own resolution reads
/// it unchanged. Returns the nodes and the meshes they came out of, which the caller needs
/// in order to lift the wheel textures out of them.
pub fn wheel_nodes(files: &[(String, Vec<u8>)]) -> (Vec<edf::EdfNode>, Vec<&Vec<u8>>) {
    let mut edfs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    for (name, data) in files {
        let bn = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase();
        if bn.ends_with(".edf") {
            edfs.insert(bn, data);
        } else if let Some(stem) = bn.strip_suffix(".hrc") {
            hrcs.insert(stem.to_string(), data);
        } else if bn.ends_with("gfx.cfg") {
            gfx_bytes = Some(data);
        }
    }
    let Some(gfx_bytes) = gfx_bytes else {
        log::warn!("[viewer] the tyres mod ships no gfx.cfg — no wheels");
        return (Vec::new(), Vec::new());
    };
    let gfx = cfg::parse(gfx_bytes);

    let mut nodes = Vec::new();
    let mut used: Vec<&Vec<u8>> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    // Front then rear, fixed: node order must not shuffle between runs.
    for part in ["front_wheel", "rear_wheel"] {
        let Some(hrc_file) = gfx
            .block(part)
            .and_then(|p| p.block("model"))
            .and_then(|m| m.get("file"))
        else {
            continue;
        };
        let stem = hrc_file
            .trim_end_matches(".hrc")
            .trim_end_matches(".HRC")
            .to_ascii_lowercase();
        let Some(bytes) = hrcs.get(&stem) else {
            log::warn!("[viewer] tyres '{part}' wants {hrc_file}, which the mod doesn't ship");
            continue;
        };
        let hrc = cfg::parse(bytes);
        let Some(node) = cfg::hrc_level0(&hrc, &stem) else { continue };
        let scene = cfg::hrc_level0_scene(&hrc)
            .map(|s| s.replace('\\', "/"))
            .and_then(|s| s.rsplit('/').next().map(str::to_ascii_lowercase))
            .unwrap_or_else(|| "model.edf".to_string());
        let Some(data) = edfs.get(&scene) else {
            log::warn!("[viewer] a tyres .hrc wants {scene}, which the mod doesn't ship");
            continue;
        };
        let mut part_nodes = edf::parse_with_levels(data, &[node]);
        // No gfx overrides: a wheel binds straight off its own material table.
        bind_textures(
            &mut part_nodes,
            data,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        drop_chain(&mut part_nodes);
        nodes.append(&mut part_nodes);
        if !seen.contains(&scene) {
            seen.push(scene);
            used.push(data);
        }
    }
    (nodes, used)
}

pub fn is_chain(sm: &edf::Submesh) -> bool {
    sm.texture.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("chain"))
}

/// Take the chain off the wheels — the one thing the wheel mesh carries that the viewer
/// can't draw.
///
/// It ships as a straight template strip that the game bends onto the sprockets from the
/// `pos`/`engine`/`ratio` the *bike's* `gfx.cfg` gives, geometry we don't build. Drawn where
/// it sits it is a bar standing 0.7 m out of the rear wheel.
///
/// A node that was nothing but chain goes entirely, since one left with no groups at all is
/// drawn whole on a single texture rather than not at all.
pub fn drop_chain(nodes: &mut Vec<edf::EdfNode>) {
    nodes.retain_mut(|n| {
        // No submesh table: a whole-node binding, and not ours to judge.
        if n.submeshes.is_empty() || !n.submeshes.iter().any(is_chain) {
            return true;
        }
        n.submeshes.retain(|sm| !is_chain(sm));
        if n.submeshes.is_empty() {
            return false;
        }
        compact_to_submeshes(n);
        true
    });
}

/// Rebuild a node around the submeshes it has left, so what it no longer draws stops
/// counting for anything else either.
///
/// Dropping a submesh on its own leaves its triangles — and their vertices — in the buffers.
/// Nothing draws them, but everything that *measures* the model still sees them, and the
/// chain's 0.7 m of template was enough to move where the viewer centres the bike and how
/// far `SideBySide` drops it onto the ground.
pub fn compact_to_submeshes(n: &mut edf::EdfNode) {
    let old_idx = std::mem::take(&mut n.indices);
    let old_pos = std::mem::take(&mut n.positions);
    let old_uv = std::mem::take(&mut n.uvs);
    let old_nrm = std::mem::take(&mut n.normals);
    let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut indices: Vec<u32> = Vec::with_capacity(old_idx.len());
    let mut tri_start = 0u32;

    for sm in n.submeshes.iter_mut() {
        let from = (sm.tri_start as usize).saturating_mul(3);
        let to = from.saturating_add((sm.tri_count as usize).saturating_mul(3));
        let range = old_idx.get(from..to).unwrap_or(&[]);
        for &v in range {
            let slot = match remap.get(&v) {
                Some(&slot) => slot,
                None => {
                    let slot = (n.positions.len() / 3) as u32;
                    let o = v as usize;
                    n.positions
                        .extend_from_slice(old_pos.get(o * 3..o * 3 + 3).unwrap_or(&[0.0; 3]));
                    if !old_uv.is_empty() {
                        n.uvs.extend_from_slice(old_uv.get(o * 2..o * 2 + 2).unwrap_or(&[0.0; 2]));
                    }
                    if !old_nrm.is_empty() {
                        n.normals
                            .extend_from_slice(old_nrm.get(o * 3..o * 3 + 3).unwrap_or(&[0.0; 3]));
                    }
                    remap.insert(v, slot);
                    slot
                }
            };
            indices.push(slot);
        }
        sm.tri_start = tri_start;
        sm.tri_count = (range.len() / 3) as u32;
        tri_start += sm.tri_count;
    }
    n.indices = indices;
}

/// One of a bike's loose files, unwrapped if it arrived sealed.
///
/// A protected model installed loose ships its `.edf` sealed, the same way a locked archive
/// is. Read plainly the bytes reach the parser as an opaque blob, fail its header check, and
/// a bike that runs perfectly in game reads here as having no mesh at all. Gear and paints
/// have always been read this way; bikes hadn't been.
pub fn read_bike_file(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    Some(pkz::read_sidecar_blob(&bytes).unwrap_or(bytes))
}

pub fn gather_bike_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::{bail, Context};
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("edf")) {
        let bytes = std::fs::read(p).with_context(|| format!("read {p:?}"))?;
        let bytes = pkz::read_sidecar_blob(&bytes).unwrap_or(bytes);
        return Ok(vec![("model.edf".to_string(), bytes)]);
    }
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz")) {
        return pkz::read_selected(p, wanted_bike_file);
    }
    if p.is_dir() {
        let mut loose = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).map(str::to_string);
            if path.is_file() && name.as_deref().is_some_and(wanted_bike_file) {
                if let (Some(name), Some(bytes)) = (name, read_bike_file(&path)) {
                    loose.push((name, bytes));
                }
            }
        }
        // Packed first, loose over it. A folder holding only a swapped-in mesh still draws
        // with the `.geom`, `gfx.cfg` and stock paint that never left the archive; taking
        // the loose files alone left every part stacked at the origin and untextured.
        let mut out = packed_layer(p);
        overlay_files(&mut out, loose);
        // A mesh of any name will do — `model.edf` is the convention, not a rule.
        if !out.iter().any(|(n, _)| bikefiles::is_mesh(n)) {
            if awaiting_download(&[p]) {
                bail!("this bike's files are still in the cloud — download them and try again");
            }
            bail!("no .edf mesh for bike folder {p:?}");
        }
        return Ok(out);
    }
    bail!("can't load a bike model from {p:?}")
}

/// Add `incoming` to `files`, replacing any entry of the same name — later wins, which is
/// how the game reads a bike too: loose files layer over the packed archive, and a swap's
/// files layer over the loose ones.
pub fn overlay_files(files: &mut Vec<(String, Vec<u8>)>, incoming: Vec<(String, Vec<u8>)>) {
    for (name, data) in incoming {
        let bn = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_ascii_lowercase();
        match files
            .iter_mut()
            .find(|(n, _)| n.rsplit(['/', '\\']).next().unwrap_or(n).eq_ignore_ascii_case(&bn))
        {
            Some(slot) => *slot = (name, data),
            None => files.push((name, data)),
        }
    }
}

/// Read the named files out of `dir`, keeping only what the viewer draws with.
pub fn read_named(dir: &std::path::Path, names: &[String]) -> Vec<(String, Vec<u8>)> {
    names
        .iter()
        .filter(|n| wanted_bike_file(n))
        .filter_map(|n| read_bike_file(&dir.join(n)).map(|b| (n.clone(), b)))
        .collect()
}

/// The bike's packed model, either inside the folder or as its `<Bike>.pkz` sibling —
/// both layouts exist, and it's the fallback a Stock preview shows.
pub fn packed_bike(bike_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if let Ok(rd) = std::fs::read_dir(bike_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkz")) {
                return Some(p);
            }
        }
    }
    let sibling = library::sibling_pkz(bike_dir);
    sibling.exists().then_some(sibling)
}

/// The bike's packed layer, or nothing at all.
///
/// An archive that won't read — a locked one, or a stub iCloud has evicted — must not take
/// down a bike whose loose folder can still be drawn. Callers bail later if what's left
/// holds no mesh.
pub fn packed_layer(bike_dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let Some(pkz) = packed_bike(bike_dir) else { return Vec::new() };
    match pkz::read_selected(&pkz, wanted_bike_file) {
        // An archive can hold sealed entries of its own — unwrap them the same way a loose
        // file is unwrapped, so where a mod ships its mesh can't decide whether it draws.
        Ok(files) => files
            .into_iter()
            .map(|(n, d)| {
                let d = pkz::read_sidecar_blob(&d).unwrap_or(d);
                (n, d)
            })
            .collect(),
        Err(e) => {
            log::warn!("[viewer] couldn't read {pkz:?} ({e:#}) — drawing the loose files alone");
            Vec::new()
        }
    }
}

/// Whether a bike's files are still waiting on the cloud to hand them over.
///
/// A placeholder OneDrive or iCloud hasn't fetched is indistinguishable from a mod with
/// nothing in it, so "there's no mesh here" is the wrong thing to tell someone whose mesh is
/// simply still in the cloud. Asked of the metadata only — `stat` never triggers a download.
pub fn awaiting_download(dirs: &[&std::path::Path]) -> bool {
    dirs.iter().any(|dir| {
        if packed_bike(dir).is_some_and(|p| cloudfiles::is_placeholder(&p)) {
            return true;
        }
        std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
            let p = e.path();
            p.is_file()
                && e.file_name().to_str().is_some_and(wanted_bike_file)
                && cloudfiles::is_placeholder(&p)
        })
    })
}

/// The bytes behind a `PreviewSet`: the packed bike, with the loose files that stay laid
/// over it and the variant's over those. Stock parks every loose mesh and so draws the
/// packed model itself; every other variant draws its own mesh on the same foundation.
pub fn gather_preview_files(set: &PreviewSet) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::bail;
    // Packed, then the loose root, then the variant — the game's own order. The archive is
    // never skipped: a swap ships a mesh and little else, so the bike's `.geom`, `gfx.cfg`
    // and `.hrc`s have nowhere else to come from.
    let mut out = packed_layer(&set.bike_dir);
    overlay_files(&mut out, read_named(&set.bike_dir, &set.root_keep));
    overlay_files(&mut out, read_named(&set.variant_dir, &set.variant_files));
    if !out.iter().any(|(n, _)| bikefiles::is_mesh(n)) {
        if awaiting_download(&[&set.bike_dir, &set.variant_dir]) {
            bail!("this model's files are still in the cloud — download them and try again");
        }
        bail!("this model has no mesh to show — the bike would have no model at all");
    }
    Ok(out)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderPart {
    pub part: String,
    pub nodes: Vec<edf::EdfNode>,
    pub textures: Vec<paint::PaintTexture>,
    /// The body's rig, in the frame `nodes` came back in. Only the body has one — gear is
    /// rigid and hangs off a bone rather than carrying any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skeleton: Vec<edf::Bone>,
    /// Which bones move which vertices. Empty unless `skeleton` is filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin: Option<edf::Skin>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderModel {
    pub parts: Vec<RiderPart>,
}

#[tauri::command]
pub async fn load_rider_model(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<RiderModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_rider_model_blocking(app, loadout))
        .await
        .map_err(|e| format!("load_rider_model task failed: {e}"))?
}

pub fn load_rider_model_blocking(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<RiderModel, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let base = library::mods_subdir(&cfg.mods_path, "mods/rider");
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
        if let Some(p) = load_gear(&cfg, &base, spec, model, paint, goggles, &loadout.rider) {
            parts.push(p);
        }
    }

    let suit = load_rider_paint(&cfg, &base, "suit", &loadout.rider, "paints", &loadout.suit_paint);
    let gloves =
        load_rider_paint(&cfg, &base, "gloves", &loadout.rider, "gloves", &loadout.gloves_paint);
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
pub async fn load_rider_body_model(
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

/// The textures a body mesh carries itself, memoised alongside the mesh.
///
/// These depend on the model and not on the loadout, but reaching them means reading the
/// whole `rider.edf` back — 67 MB for Rider+. The viewer reloads on every loadout change, so
/// without this a rider wearing a kit that leaves one slot bare re-reads the model each time
/// you touch a dropdown.
///
/// Only textures the viewer could actually draw are decoded. Skin renders as flat colour and
/// the `w_` planes render as nothing, so inflating and re-encoding them is time spent on
/// pixels no one will ever see — and on a rider body that decode costs more than parsing the
/// mesh does.
pub(crate) fn body_textures(src: &BodySource, profile: &str) -> Option<Vec<paint::PaintTexture>> {
    static C: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Vec<paint::PaintTexture>>>,
    > = std::sync::OnceLock::new();
    let cache = C.get_or_init(Default::default);
    let key = src.cache_key(profile);
    if let Some(t) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(t);
    }
    let drawn = |name: &str| !matches!(body_slot(Some(name)).as_str(), "hide" | "face");
    let texs = paint::extract_edf_textures_where(&src.read(profile)?, drawn);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, texs.clone());
    }
    Some(texs)
}

pub fn load_rider_body(
    cfg: &config::AppConfig,
    profile: &str,
    mut textures: Vec<paint::PaintTexture>,
) -> Option<RiderPart> {
    let profile = rider_profile_or_stock(profile);
    let src = rider_body_source(cfg, profile)?;
    let nodes = rider_body_nodes(&src, profile)?;

    // Whatever the mesh asks for that no paint supplies, the model itself supplies: the
    // supermoto rider ships no `.pnt` at all and wears its baked textures, and a custom
    // model paints its own extra pieces into the mesh. Reading the file back costs real
    // time on a 60 MB body, so only a name actually missing pays for it.
    let supplied: std::collections::HashSet<String> =
        textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    let wanted: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| n.submeshes.iter().filter_map(|s| s.texture.as_deref()))
        .map(|t| t.to_ascii_lowercase())
        // `hide` draws nothing and `face` is bare skin — neither wants a texture.
        .filter(|t| t != "hide" && t != "face" && !supplied.contains(t))
        .collect();
    if !wanted.is_empty() {
        match body_textures(&src, profile) {
            Some(own) => textures.extend(
                own.into_iter().filter(|t| wanted.contains(&t.name.to_ascii_lowercase())),
            ),
            None => log::warn!("[rider] body '{profile}' could not be re-read for {wanted:?}"),
        }
    }

    log::info!(
        "[rider] body '{profile}' loaded from {src:?}: {} nodes, tex={:?}",
        nodes.len(),
        textures.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
    );
    let skeleton = body_rig(&src, profile);
    let skin = (!skeleton.is_empty()).then(|| body_skin(&src, profile, &nodes, &skeleton));
    Some(RiderPart {
        part: "body".into(),
        nodes,
        textures,
        skeleton,
        skin,
    })
}

/// Stand a rider body up.
///
/// Rider meshes don't agree on which axis is up. The stock motocross rider is authored Y-up;
/// the supermoto rider and Rider+ are Z-up and arrive lying on their back. The viewer anchors
/// every piece of gear to a fraction of the body's height, so a body on its side doesn't just
/// look wrong — it measures a quarter of a metre tall instead of a metre and a bit, and the
/// helmet and boots scale down to specks and sink into the torso.
///
/// A rider is a standing figure: its longest axis is its height. Where that's Z, the mesh is
/// authored in the other convention and takes that convention's one fixed rotation. Where
/// it's already Y, leave the mesh alone — guessing at a body that's already upright is how
/// the stock rider would get broken to fix a custom one.
///
/// The rotation is a half turn about Y on top of the quarter turn about X. Standing the body
/// up alone leaves it facing backwards, which the name and number planes give away: they sit
/// on a rider's back, and on the stock motocross rider — authored upright, so correct by
/// construction — they sit behind its centre. On the Z-up meshes a bare quarter turn puts
/// them in front. Both halves are needed together: `y = -z, z = -y` on its own mirrors the
/// mesh rather than turning it, which would swap the rider's left and right hands.
/// The turn a Z-up body takes: a half turn about Y on top of a quarter turn about X.
/// Named here because the rig has to take exactly the same one — see [`body_rig`].
const BODY_STAND_UP: [[f32; 3]; 3] =
    [[-1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]];

/// Is this body authored Z-up — lying on its back, longest axis in Z?
pub fn body_is_z_up(ext: [f32; 3]) -> bool {
    ext[2] > ext[1] && ext[2] > ext[0]
}

/// A half turn about X. Up and front both invert; left and right are kept, so it turns the
/// body rather than mirroring it.
const BODY_FLIP_UPRIGHT: [[f32; 3]; 3] =
    [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

/// A half turn about Y. Front and left invert, up is kept — the turn that faces a body the
/// other way without disturbing which end is the head.
const BODY_TURN_AROUND: [[f32; 3]; 3] =
    [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]];

/// The extent of a body, or of one slot of it.
pub fn body_bounds(nodes: &[edf::EdfNode], slot: Option<&str>) -> ([f32; 3], [f32; 3]) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for n in nodes {
        for sm in &n.submeshes {
            if slot.is_some() && sm.texture.as_deref() != slot {
                continue;
            }
            let range =
                sm.tri_start as usize * 3..(sm.tri_start + sm.tri_count) as usize * 3;
            for i in n.indices.get(range).unwrap_or(&[]) {
                let Some(v) = n.positions.get(*i as usize * 3..*i as usize * 3 + 3) else {
                    continue;
                };
                for a in 0..3 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
        }
    }
    (lo, hi)
}

/// Where one slot sits front-to-back, relative to the body's own centre. `None` when the
/// model has no such slot.
pub fn slot_depth(nodes: &[edf::EdfNode], slot: &str, centre_z: f32) -> Option<f64> {
    let (mut sum, mut count) = (0f64, 0usize);
    for n in nodes {
        for sm in &n.submeshes {
            if sm.texture.as_deref() != Some(slot) {
                continue;
            }
            let range =
                sm.tri_start as usize * 3..(sm.tri_start + sm.tri_count) as usize * 3;
            for i in n.indices.get(range).unwrap_or(&[]) {
                if let Some(v) = n.positions.get(*i as usize * 3..*i as usize * 3 + 3) {
                    sum += (v[2] - centre_z) as f64;
                    count += 1;
                }
            }
        }
    }
    (count > 0).then(|| sum / count as f64)
}

pub fn turn_body(nodes: &mut [edf::EdfNode], r: [[f32; 3]; 3]) {
    for n in nodes.iter_mut() {
        for v in n.positions.chunks_exact_mut(3).chain(n.normals.chunks_exact_mut(3)) {
            let (x, y, z) = (v[0], v[1], v[2]);
            v[0] = r[0][0] * x + r[0][1] * y + r[0][2] * z;
            v[1] = r[1][0] * x + r[1][1] * y + r[1][2] * z;
            v[2] = r[2][0] * x + r[2][1] * y + r[2][2] * z;
        }
    }
}

/// `b` applied after `a`, as one matrix. The rig has to take exactly what the mesh took,
/// and it takes it in one go rather than replaying the steps.
pub fn compose(b: [[f32; 3]; 3], a: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut m = [[0.0f32; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            m[r][c] = (0..3).map(|k| b[r][k] * a[k][c]).sum();
        }
    }
    m
}

const BODY_KEEP: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Stand the body up, then check the result and fix it if the guess was wrong.
///
/// Returns the whole turn, so [`body_rig`] can put the skeleton through the same one instead
/// of deciding for itself. It used to decide from the rig's own extents, which agreed with
/// the mesh only for as long as the mesh's answer was a fixed rotation — the moment the mesh
/// can be corrected and the rig can't, a corrected body gets a skeleton lying across it.
pub fn stand_body_upright(nodes: &mut [edf::EdfNode]) -> [[f32; 3]; 3] {
    let (lo, hi) = body_bounds(nodes, None);
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let mut applied = BODY_KEEP;
    if body_is_z_up(ext) {
        // The quarter turn assumes the head lies at the most negative Z, which is where the
        // stock Z-up bodies put it. `check_body_orientation` is what catches a model that
        // doesn't — the assumption is now a starting guess rather than the answer.
        turn_body(nodes, BODY_STAND_UP);
        applied = BODY_STAND_UP;
        log::info!("[rider] body was authored Z-up ({ext:?}); stood it upright");
    }
    compose(check_body_orientation(nodes), applied)
}

/// Confirm the body ended up the right way up and the right way round, and turn it if not.
///
/// The turn above is one fixed rotation for one authoring convention, and a custom model is
/// under no obligation to share it: a Z-up body with its head at *positive* Z takes that
/// rotation and lands upside down and facing backwards, which is exactly what a hoodie-and-
/// baggies rider model was reported doing while the stock ones were fine.
///
/// So measure the result instead of trusting the guess. Both signals are the ones the
/// viewer's own real-model test has always asserted — they just used to be checked in a test
/// nobody runs on a player's machine, against models that happened to pass:
///
///   * **Which end is the head.** Bare skin. The head is the highest thing on a rider, so
///     skin sitting in the bottom half means the body is upside down.
///   * **Which way it faces.** The name and number planes go on a rider's back. Where a model
///     has none, the head leans forward over the bars — a weaker signal, so it only decides
///     when the strong one is absent.
///
/// A model showing no skin at all — every inch covered by kit, helmet and gloves — leaves the
/// first question unanswerable, and it keeps whatever the guess gave it. Better an unturned
/// body than one turned on no evidence.
pub fn check_body_orientation(nodes: &mut [edf::EdfNode]) -> [[f32; 3]; 3] {
    let mut applied = BODY_KEEP;
    let (lo, hi) = body_bounds(nodes, None);
    let height = hi[1] - lo[1];
    if height <= 0.0 {
        return applied;
    }
    let (skin_lo, skin_hi) = body_bounds(nodes, Some("face"));
    // `>=`, not `>`: the question is whether the model shows any skin at all, and an
    // unfound slot leaves the sentinels crossed (`hi` below `lo`). Asking for vertical
    // extent instead would call a model with skin no taller than a point "no skin".
    let has_skin = skin_hi[1] >= skin_lo[1];
    if has_skin && skin_hi[1] < lo[1] + 0.5 * height {
        log::info!(
            "[rider] body is upside down (skin tops out at {:.3} of {:.3}..{:.3}); turning it              the right way up",
            skin_hi[1],
            lo[1],
            hi[1],
        );
        turn_body(nodes, BODY_FLIP_UPRIGHT);
        applied = BODY_FLIP_UPRIGHT;
    }

    // Re-measured: the flip above moves everything it is about to judge.
    let (lo, hi) = body_bounds(nodes, None);
    let centre_z = (lo[2] + hi[2]) / 2.0;
    let backwards = match slot_depth(nodes, "hide", centre_z) {
        // The planes are on the back, so they belong behind the centre.
        Some(back) => back > 0.0,
        None => {
            // No planes. The head leans forward over the bars — but only the head, since on
            // the rolled-sleeve models the skin texture also covers bare wrists that reach
            // well down the body and would drag the answer with them.
            let head_floor = hi[1] - 0.125 * (hi[1] - lo[1]);
            let (mut sum, mut count) = (0f64, 0usize);
            for n in nodes.iter() {
                for sm in &n.submeshes {
                    if sm.texture.as_deref() != Some("face") {
                        continue;
                    }
                    let range =
                        sm.tri_start as usize * 3..(sm.tri_start + sm.tri_count) as usize * 3;
                    for i in n.indices.get(range).unwrap_or(&[]) {
                        if let Some(v) = n.positions.get(*i as usize * 3..*i as usize * 3 + 3) {
                            if v[1] > head_floor {
                                sum += (v[2] - centre_z) as f64;
                                count += 1;
                            }
                        }
                    }
                }
            }
            count > 0 && (sum / count as f64) < 0.0
        }
    };
    if backwards {
        log::info!("[rider] body was facing backwards; turned it around");
        turn_body(nodes, BODY_TURN_AROUND);
        applied = compose(BODY_TURN_AROUND, applied);
    }
    applied
}

/// Bind each body submesh to the texture the mesh itself says it wears.
///
/// A material index is not a slot: it counts into the model's own texture list, and that
/// list is written in the exporter's order. `default_mx` happens to put the suit first and
/// the gloves second; `default_sm` puts the face second and its gloves third; Rider+ puts
/// the gloves first and the suit last. So a fixed index→slot map is one model memorised —
/// it already swaps face and gloves on the supermoto rider, and on a custom model it smears
/// the glove texture across the whole body. Read the name the model was drawn against
/// instead, the same reading the bike and gear viewers take — through each node's own
/// material table, since an id counts into the table of the part that owns it and means
/// nothing outside it (see `bind_textures`).
pub fn bind_body_submeshes(nodes: &mut [edf::EdfNode], mesh: &[u8]) {
    let colors = edf::color_textures(mesh);
    if colors.is_empty() {
        // A mesh whose texture table doesn't parse tells us nothing; the stock layout is
        // still right for the model the app has always shown.
        return tag_body_materials(nodes);
    }
    bind_body_to_colors(nodes, &colors);
}

/// The binding itself, split out so a test can drive it without a mesh blob: reading a
/// material id through the wrong node's table is the failure worth pinning down, and it
/// needs two nodes whose tables disagree, not a parseable `.edf`.
pub fn bind_body_to_colors(nodes: &mut [edf::EdfNode], colors: &[edf::EmbeddedTexture]) {
    for node in nodes.iter_mut() {
        // Disjoint field borrows: the node's table is read while its submeshes are written.
        let materials = &node.materials;
        for sm in node.submeshes.iter_mut() {
            let emb = sm
                .mat
                .and_then(|m| materials.get(m as usize).copied().flatten())
                .and_then(|slot| colors.get(slot))
                .map(|t| t.name.as_str());
            sm.texture = Some(body_slot(emb));
        }
    }
}

/// The viewer slot an embedded texture name belongs to. The `w_` planes are decals the game
/// composites a rider's name and number onto and carry no look of their own, and skin must
/// never wear the kit. Everything else keeps its own name, so a paint replaces it by name
/// and a piece the paint doesn't cover falls back to the model's own texture.
pub fn body_slot(name: Option<&str>) -> String {
    let Some(n) = name else { return "rider".into() };
    let l = n.to_ascii_lowercase();
    if l.starts_with("w_") {
        return "hide".into();
    }
    if l.contains("face") {
        return "face".into();
    }
    l
}

pub fn tag_body_materials(nodes: &mut [edf::EdfNode]) {
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

/// Rider bodies and helmets are small next to a bike, and a session cycles through a
/// handful of them, so this can hold more entries than the bike cache does.
const MESH_CACHE_CAP: usize = 12;

pub fn pkz_mesh_cache() -> &'static std::sync::Mutex<lru::Lru<Vec<edf::EdfNode>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<Vec<edf::EdfNode>>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP)))
}

pub fn keep_lod0(nodes: &mut Vec<edf::EdfNode>) {
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| n.name.is_empty() || seen.insert(n.name.clone()));
}

/// Rigs are tiny — 65 bones of two matrices each — so this holds more than the mesh cache.
pub fn rig_cache() -> &'static std::sync::Mutex<lru::Lru<Vec<edf::Bone>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<Vec<edf::Bone>>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP * 2)))
}

/// The turn each body mesh took, so its rig can take the same one. Keyed exactly as the
/// mesh cache is, and written on the parse that fills it.
pub fn body_turn_cache() -> &'static std::sync::Mutex<lru::Lru<[[f32; 3]; 3]>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<[[f32; 3]; 3]>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP * 2)))
}

/// A rider body's rig, in the same frame the viewer gets its mesh in, memoised.
///
/// The mesh takes two turns on the way out of the file — [`edf::to_right_handed`] mirrors X,
/// then [`stand_body_upright`] stands a Z-up body up — and the rig has to take both, or the
/// skeleton ends up mirrored or lying beside a standing body. Whether the second one applies
/// is decided from the rig's own extents rather than the mesh's: both are authored in the
/// same frame, so they agree, and asking the rig costs nothing where asking the mesh would
/// mean parsing 67 MB a second time.
pub(crate) fn body_rig(src: &BodySource, profile: &str) -> Vec<edf::Bone> {
    let key = format!("rig:{}", src.cache_key(profile));
    if let Some(r) = rig_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        return r;
    }
    let mut rig = src.read(profile).map(|b| edf::parse_skeleton(&b)).unwrap_or_default();
    if !rig.is_empty() {
        edf::transform_skeleton(&mut rig, [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        // What the mesh actually took, where the mesh has been read — which is every path
        // that reaches here, since a rig is only wanted alongside the body it bends.
        let mesh_turn = body_turn_cache()
            .lock()
            .ok()
            .and_then(|mut c| c.get(&src.cache_key(profile)).copied());
        match mesh_turn {
            Some(turn) => edf::transform_skeleton(&mut rig, turn),
            None => {
                // No mesh read this session. Fall back to the rig's own extents, which is
                // the old answer and right for every body whose head is where the stock ones
                // put it — the models this ever differed for are the ones the mesh corrects.
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for b in rig.iter() {
                    let o = b.origin();
                    for a in 0..3 {
                        lo[a] = lo[a].min(o[a]);
                        hi[a] = hi[a].max(o[a]);
                    }
                }
                if body_is_z_up([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) {
                    edf::transform_skeleton(&mut rig, BODY_STAND_UP);
                }
            }
        }
        log::info!("[rider] body '{profile}' rig: {} bones", rig.len());
    }
    if let Ok(mut c) = rig_cache().lock() {
        c.insert(key, rig.clone());
    }
    rig
}

/// The body's binding to its rig, memoised. Working it out is quick — a third of a million
/// point-to-segment distances — but it depends only on the model, and a loadout change must
/// not pay for it again.
pub(crate) fn body_skin(
    src: &BodySource,
    profile: &str,
    nodes: &[edf::EdfNode],
    rig: &[edf::Bone],
) -> edf::Skin {
    let key = format!("skin:{}", src.cache_key(profile));
    if let Some(s) = skin_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        return s;
    }
    let skin = edf::skin_mesh(nodes, rig);
    if let Ok(mut c) = skin_cache().lock() {
        c.insert(key, skin.clone());
    }
    skin
}

pub fn skin_cache() -> &'static std::sync::Mutex<lru::Lru<edf::Skin>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<edf::Skin>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP)))
}

pub fn cached_mesh(key: &str) -> Option<Vec<edf::EdfNode>> {
    // `mut` because a hit marks the entry as the warm one — see `lru::Lru`.
    pkz_mesh_cache().lock().ok().and_then(|mut c| c.get(key).cloned())
}

/// Parse a mesh into viewer space and memoise it. `prepare` runs once, on the parse that
/// populates the cache, for work that depends on the file rather than on the loadout.
pub fn mesh_from_bytes(
    key: String,
    data: &[u8],
    // Gear is read with [`edf::parse_gear`] — see there for why the two readings differ.
    gear: bool,
    prepare: impl FnOnce(&mut Vec<edf::EdfNode>, &[u8]),
) -> Option<Vec<edf::EdfNode>> {
    let mut nodes = if gear { edf::parse_gear(data) } else { edf::parse(data) };
    edf::to_right_handed(&mut nodes);
    keep_lod0(&mut nodes);
    if nodes.is_empty() {
        return None;
    }
    prepare(&mut nodes, data);
    if let Ok(mut c) = pkz_mesh_cache().lock() {
        c.insert(key, nodes.clone());
    }
    Some(nodes)
}

/// A gear mesh out of a game archive, memoised. Gear-only: the two readings would otherwise
/// share a cache entry, and whichever asked first would settle how the other saw the file.
pub fn load_pkz_mesh(pkz: &std::path::Path, entry: &str) -> Option<Vec<edf::EdfNode>> {
    let key = format!("{}:{}#gear", bike_cache_key(&pkz.to_string_lossy()), entry);
    if let Some(n) = cached_mesh(&key) {
        return Some(n);
    }
    mesh_from_bytes(key, &read_pkz_entry(pkz, entry)?, true, |_, _| {})
}

/// The stock rider profiles the game itself ships. They're the fallback for a custom model
/// that brings a mesh but none of the kits meant to be worn on it.
const STOCK_RIDER_PROFILES: [&str; 2] = ["default_mx", "default_sm"];

pub fn rider_profile_or_stock(profile: &str) -> &str {
    if profile.is_empty() { STOCK_RIDER_PROFILES[0] } else { profile }
}

/// Where a rider profile's body mesh lives.
///
/// A rider model is a whole new `rider.edf`, not a texture — Rider+ and its variants install
/// as folders under `mods/rider/riders`. The game's own archive is the last place to look,
/// not the only one: reading only `rider.pkz` left a picked custom profile rendering no body
/// at all, just gear floating where the rider should be.
#[derive(Debug, Clone)]
pub(crate) enum BodySource {
    /// `mods/rider/riders/<profile>/rider.edf`, installed loose — the shape every rider
    /// model on mxb-mods ships.
    Loose(std::path::PathBuf),
    /// A profile packed as `<profile>.pkz`, or the game's own `rider.pkz`.
    Packed(std::path::PathBuf),
}

impl BodySource {
    fn cache_key(&self, profile: &str) -> String {
        match self {
            Self::Loose(p) => bike_cache_key(&p.to_string_lossy()),
            Self::Packed(p) => format!("{}:{profile}", bike_cache_key(&p.to_string_lossy())),
        }
    }

    /// The mesh bytes. A packed profile is read at the entry the game uses, then — for a
    /// repack that flattened the folder tree — by any `rider.edf` in the archive.
    fn read(&self, profile: &str) -> Option<Vec<u8>> {
        match self {
            Self::Loose(p) => std::fs::read(p).ok(),
            Self::Packed(p) => read_pkz_entry(p, &format!("rider/riders/{profile}/rider.edf"))
                .or_else(|| read_pkz_basename(p, "rider.edf")),
        }
    }
}

/// One named file out of an archive wherever it sits in the tree, for repacks that don't
/// keep the game's layout. Only the wanted entry is inflated.
pub fn read_pkz_basename(pkz: &std::path::Path, base: &str) -> Option<Vec<u8>> {
    let want = |n: &str| {
        n.replace('\\', "/")
            .rsplit('/')
            .next()
            .is_some_and(|b| b.eq_ignore_ascii_case(base))
    };
    pkz::read_selected(pkz, want).ok()?.into_iter().next().map(|(_, d)| d)
}

pub(crate) fn rider_body_source(cfg: &config::AppConfig, profile: &str) -> Option<BodySource> {
    let riders = library::mods_subdir(&cfg.mods_path, "mods/rider/riders");
    let loose = riders.join(profile).join("rider.edf");
    if loose.is_file() {
        return Some(BodySource::Loose(loose));
    }
    let packed = riders.join(format!("{profile}.pkz"));
    if packed.is_file() {
        return Some(BodySource::Packed(packed));
    }
    resolve_game_pkz(cfg, "rider.pkz").map(BodySource::Packed)
}

/// The body mesh, submeshes already bound to their textures. Binding depends on the model
/// and not on the loadout, so it happens on the parse that fills the cache — a paint change
/// must not re-read a 60 MB body.
pub(crate) fn rider_body_nodes(src: &BodySource, profile: &str) -> Option<Vec<edf::EdfNode>> {
    let key = src.cache_key(profile);
    if let Some(n) = cached_mesh(&key) {
        return Some(n);
    }
    let turn_key = key.clone();
    mesh_from_bytes(key, &src.read(profile)?, false, move |nodes, data| {
        bind_body_submeshes(nodes, data);
        let turn = stand_body_upright(nodes);
        if let Ok(mut c) = body_turn_cache().lock() {
            c.insert(turn_key, turn);
        }
    })
}

pub fn load_rider_body_nodes(cfg: &config::AppConfig, profile: &str) -> Option<Vec<edf::EdfNode>> {
    let profile = rider_profile_or_stock(profile);
    rider_body_nodes(&rider_body_source(cfg, profile)?, profile)
}

pub fn resolve_game_pkz(cfg: &config::AppConfig, name: &str) -> Option<std::path::PathBuf> {
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
    let detected = config::detect_game_path(cfg.game())?;
    let p = std::path::Path::new(&detected).join(name);
    p.exists().then_some(p)
}

pub fn read_pkz_entry(pkz: &std::path::Path, entry: &str) -> Option<Vec<u8>> {
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
pub async fn load_gear_model(
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
            // A library preview shows one mod on its own — nothing outside it to gather.
            Vec::new(),
        )
    })
    .await
    .map_err(|e| format!("load_gear_model task failed: {e}"))?
}

#[tauri::command]
pub async fn load_stock_gear_model(
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
        // The entry is kept, not just the nodes: with no paint to show, the mesh's own
        // textures are the look, and they're read back out of the file it came from.
        let named = format!("{folder}/{}", spec.mesh);
        let (entry, nodes) = load_pkz_mesh(&pkz, &named)
            .map(|n| (named, n))
            .or_else(|| {
                let alt = stock_gear_entry(&pkz, &folder)?;
                let n = load_pkz_mesh(&pkz, &alt)?;
                Some((alt, n))
            })
            .ok_or_else(|| format!("stock {part} mesh not found in rider.pkz"))?;
        let textures = match paint_path.filter(|s| !s.is_empty()) {
            Some(p) => std::fs::read(&p)
                .ok()
                .and_then(|d| paint::decode_any(&d).ok())
                .map(|pnt| pnt.into_par_iter().map(paint::into_texture).collect())
                .unwrap_or_default(),
            // Nothing to preview → the stock look, which is what the mesh carries. Not the
            // first `.pnt` in the folder: that's a paint like any other, and picking it here
            // is what showed the stock helmet bronze everywhere else. Companion maps are
            // skipped — the viewer never draws a normal or roughness sheet, so inflating
            // one is time spent on pixels no one sees.
            None => read_pkz_entry(&pkz, &entry)
                .map(|d| paint::extract_edf_textures_where(&d, |n| !is_companion_map(n)))
                .unwrap_or_default(),
        };
        Ok(RiderPart {
            part: spec.part.into(),
            nodes,
            textures,
            skeleton: Vec::new(),
        skin: None,
        })
    })
    .await
    .map_err(|e| format!("load_stock_gear_model task failed: {e}"))?
}

#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GearPaints {
    pub paints: Vec<String>,
    pub goggles: Vec<String>,
    /// The mesh carries its own shell / goggle texture, so a "Stock" entry is worth
    /// offering next to the packed paints. Preview-only — never a loadout value, since
    /// the game names a `.pnt` there and has no word for "the model's own look".
    pub has_stock: bool,
    pub has_stock_goggles: bool,
}

impl GearPaints {
    /// Fold another source's paints into this one.
    ///
    /// A name that turns up twice is the same look twice — a paint pack installed loose
    /// beside the `.pkz` it was made for ships the same file names — so repeats are dropped
    /// rather than offered as two choices.
    fn absorb(&mut self, other: GearPaints) {
        let merge = |into: &mut Vec<String>, more: Vec<String>| {
            for n in more {
                if !into.iter().any(|have| have.eq_ignore_ascii_case(&n)) {
                    into.push(n);
                }
            }
        };
        merge(&mut self.paints, other.paints);
        merge(&mut self.goggles, other.goggles);
        self.has_stock |= other.has_stock;
        self.has_stock_goggles |= other.has_stock_goggles;
    }

    /// Alphabetical across the merged set — sources arrive sorted individually, which on its
    /// own would list one source after the other rather than one list of paints.
    fn sort(&mut self) {
        self.paints.sort_by_key(|s| s.to_lowercase());
        self.goggles.sort_by_key(|s| s.to_lowercase());
    }
}

pub fn gear_paints_at(path: &std::path::Path) -> Result<GearPaints, String> {
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
    // Resolved the same way the loader resolves it, so the picker can't offer a stock look
    // that comes off a different mesh than the one drawn — every mesh it draws, since a
    // two-piece set carries a texture per piece.
    let scenes = gear_scenes(&files);
    let mut meshes: Vec<&Vec<u8>> = scenes.iter().filter_map(|s| gear_file(&files, s)).collect();
    if meshes.is_empty() {
        meshes.extend(files.iter().find(|(n, _)| is_visible_gear_mesh(n)).map(|(_, d)| d));
    }
    let mut embedded: Vec<String> = Vec::new();
    for d in &meshes {
        for t in edf::embedded_textures(d) {
            if !embedded.iter().any(|h| h.eq_ignore_ascii_case(&t.name)) {
                embedded.push(t.name);
            }
        }
    }
    let supplied = |folder: &str| {
        paint_texture_names(
            files
                .iter()
                .filter(|(n, _)| gear_folder_paint_name(n, folder).is_some())
                .map(|(_, d)| d.as_slice()),
        )
    };
    Ok(GearPaints {
        has_stock: mesh_supplies_side(&embedded, &supplied("paints"), false),
        has_stock_goggles: mesh_supplies_side(&embedded, &supplied("goggles"), true),
        paints: names("paints"),
        goggles: names("goggles"),
    })
}

/// Whether a side has a stock look to offer: the mesh carries its own copy of the sheet that
/// side's paints replace.
///
/// Asking the paints, not the texture names, is what keeps a "Stock" entry off the Bell Moto 10 —
/// it embeds a tear-off film and a goggle, but not the shell sheet its paints supply, so picking
/// "Stock" there drew the helmet in a near-blank film. With nothing painted on a side at all, the
/// mesh's own look is the only one there is, so anything it carries for that side counts.
///
/// One function rather than two readings of the same question: it decides both whether the
/// library's picker offers "Stock" and whether an empty paint slot resolves to it
/// ([`load_gear_model_blocking`]). Those two have to agree — the rider tab rendering a helmet
/// the library's "Stock" entry says doesn't exist is the drift worth designing out.
pub fn mesh_supplies_side(embedded: &[String], side_paints: &[String], goggle_side: bool) -> bool {
    let mut usable = embedded.iter().filter(|n| !is_companion_map(n));
    if side_paints.is_empty() {
        return usable.any(|n| is_goggle_name(n) == goggle_side);
    }
    usable.any(|e| side_paints.iter().any(|p| p.eq_ignore_ascii_case(e)))
}

#[tauri::command]
pub async fn list_gear_paints(path: String) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || gear_paints_at(std::path::Path::new(&path)))
        .await
        .map_err(|e| format!("list_gear_paints task failed: {e}"))?
}

#[tauri::command]
pub async fn list_installed_gear_paints(
    app: tauri::AppHandle,
    part: String,
    model: String,
) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if model.trim().is_empty() {
            return Ok(GearPaints::default());
        }
        let Some(spec) = GEAR.iter().find(|g| g.part == part) else {
            return Ok(GearPaints::default());
        };
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(gear_paints_for(&cfg, spec, &model))
    })
    .await
    .map_err(|e| format!("list_installed_gear_paints task failed: {e}"))?
}

/// Every paint a gear model can be worn in, from every place one can live.
///
/// A model reaches the game three ways, and more than one is true at once more often than
/// not: an unpacked `<model>/` folder, a `<model>.pkz`, and — for the pieces the game itself
/// ships — a folder inside `rider.pkz`. A paint pack for a packaged mod installs loose in a
/// folder beside it, because nothing can write into the `.pkz`. So a picker that stopped at
/// the first source it found showed one of those sets and never the other.
pub(crate) fn gear_paints_for(cfg: &config::AppConfig, spec: &GearSpec, model: &str) -> GearPaints {
    let stem = model.trim_end_matches(".pkz");
    let rider = library::mods_subdir(&cfg.mods_path, "mods/rider");
    let mut out = GearPaints::default();
    for src in gear_sources(&rider, spec, stem) {
        if !src.exists() {
            continue;
        }
        match gear_paints_at(&src) {
            Ok(found) => out.absorb(found),
            // One unreadable source must not cost the others their paints.
            Err(e) => log::warn!("[rider] {} paints from {src:?}: {e}", spec.part),
        }
    }
    // The game's own copy of the piece. This is all a stock name — `default`, `full`, `neck`
    // — has instead of a folder, and a mod installed under a stock name gets both.
    if let Some(pkz) = resolve_game_pkz(cfg, "rider.pkz") {
        let folder = format!("rider/{}/{}", spec.pkz_kind, stem);
        out.absorb(GearPaints {
            paints: pkz_paint_names(&pkz, &folder, "paints"),
            goggles: pkz_paint_names(&pkz, &folder, "goggles"),
            // Whether the stock mesh has a look of its own is a question about the mesh, and
            // answering it would mean pulling that mesh out of a 100 MB archive every time a
            // picker opens. The installed sources above answer it where a "Stock" entry is
            // actually offered.
            ..GearPaints::default()
        });
    }
    out.sort();
    out
}

/// The `.pnt` names under `<folder>/<sub>/` inside a pkz, without reading a byte of pixels.
///
/// Names come out of the archive's own directory, which keeps this cheap enough to run every
/// time a picker opens — the game's `rider.pkz` is around 100 MB, and reading it to list a
/// dozen names would be felt. That shortcut is the same assumption [`read_pkz_first`] makes,
/// that the game's own archives are plain zips, with the general reader behind it for
/// anything else.
pub fn pkz_paint_names(pkz: &std::path::Path, folder: &str, sub: &str) -> Vec<String> {
    let prefix = format!("{folder}/{sub}/").to_ascii_lowercase();
    let stem = |name: &str| -> Option<String> {
        let n = name.replace('\\', "/");
        let lower = n.to_ascii_lowercase();
        if !lower.starts_with(&prefix) || !lower.ends_with(".pnt") {
            return None;
        }
        let base = n.rsplit('/').next()?;
        Some(base[..base.len() - ".pnt".len()].to_string())
    };
    if pkz::is_plain_zip(pkz) {
        let Ok(file) = std::fs::File::open(pkz) else {
            return Vec::new();
        };
        let Ok(zip) = zip::ZipArchive::new(file) else {
            return Vec::new();
        };
        return zip.file_names().filter_map(stem).collect();
    }
    pkz::read_selected(pkz, |n| stem(n).is_some())
        .map(|hits| hits.iter().filter_map(|(n, _)| stem(n)).collect())
        .unwrap_or_default()
}

pub fn gear_folder_paint_name(entry: &str, folder: &str) -> Option<String> {
    let n = entry.replace('\\', "/").to_ascii_lowercase();
    if !n.contains(&format!("/{folder}/")) && !n.starts_with(&format!("{folder}/")) {
        return None;
    }
    let base = entry.replace('\\', "/");
    let base = base.rsplit('/').next()?;
    let stem = base.strip_suffix(".pnt").or_else(|| base.strip_suffix(".PNT"))?;
    (!stem.is_empty()).then(|| stem.to_string())
}

/// One painted side of a gear item — the shell, or the goggles. A `.pnt` replaces the
/// mesh's textures by name, so a side is the names it supplies plus the colour texture a
/// piece falls back on when the mesh asks for one this side doesn't carry.
#[derive(Default)]
pub(crate) struct GearSide {
    names: Vec<String>,
    primary: Option<String>,
}

impl GearSide {
    fn new(names: Vec<String>) -> Self {
        let primary = names
            .iter()
            .find(|n| !is_companion_map(n))
            .or_else(|| names.first())
            .cloned();
        Self { names, primary }
    }

    /// This side's own name for a texture the mesh asks for, if it supplies it.
    fn supplies(&self, want: &str) -> Option<String> {
        self.names.iter().find(|n| n.eq_ignore_ascii_case(want)).cloned()
    }
}

pub fn load_gear_model_blocking(
    path: String,
    part: String,
    paint: Option<String>,
    goggles: Option<String>,
    stock: bool,
    stock_goggles: bool,
    // Paints that live outside the model — a rider profile's goggles, or `.pnt` files
    // dropped beside a `.pkz`. Named as a gear archive would carry them (`goggles/x.pnt`)
    // so they read exactly like the packed ones.
    extra: Vec<(String, Vec<u8>)>,
) -> Result<RiderPart, String> {
    let p = std::path::Path::new(&path);
    let mut files = read_gear_files(p).map_err(|e| format!("{e:#}"))?;
    // Appended, so a name the model itself packs still wins.
    files.extend(extra);
    let want = paint.filter(|s| !s.is_empty());
    let want_goggles = goggles.filter(|s| !s.is_empty());
    // Collect paint/goggle entries up front so we can prefer the requested one but always
    // fall back to the first available: a stale or unknown paint name must still show the
    // gear textured, never bare grey.
    let mut paints: Vec<(String, &Vec<u8>)> = Vec::new();
    let mut goggle_paints: Vec<(String, &Vec<u8>)> = Vec::new();
    for (name, data) in &files {
        if let Some(pname) = gear_folder_paint_name(name, "paints") {
            paints.push((pname, data));
        } else if let Some(gname) = gear_folder_paint_name(name, "goggles") {
            goggle_paints.push((gname, data));
        }
    }
    // Every `.edf` the item draws, in the mod's own words where it says so — kept as bytes
    // too, so a stock side can read each mesh's own textures back out of it.
    let mut meshes: Vec<&Vec<u8>> =
        gear_scenes(&files).iter().filter_map(|s| gear_file(&files, s)).collect();
    if meshes.is_empty() {
        // A mod that names a scene it doesn't ship still has a mesh in the folder; take it.
        meshes.extend(files.iter().find(|(n, _)| is_visible_gear_mesh(n)).map(|(_, d)| d));
    }
    // Parsed per mesh and kept that way until binding: a submesh's material id counts its own
    // mesh's texture list, so one mesh's materials must never be read against another's.
    let mut drawn: Vec<(&Vec<u8>, Vec<edf::EdfNode>)> = Vec::new();
    for d in &meshes {
        let mut nodes = edf::parse_gear(d);
        edf::to_right_handed(&mut nodes);
        keep_lod0(&mut nodes);
        if !nodes.is_empty() {
            drawn.push((d, nodes));
        }
    }
    if drawn.is_empty() {
        return Err(format!("no gear mesh found in {path}"));
    }
    // Which textures each side's paints supply — the names the mesh declares and leaves to a
    // `.pnt`, and so part of the slot order its materials count. Headers only, no pixels, and
    // wanted twice over: to decide whether a side has a stock look at all, and (rejoined
    // below) to bind the submeshes.
    let shell_declared = paint_texture_names(paints.iter().map(|(_, d)| d.as_slice()));
    let goggle_declared = paint_texture_names(goggle_paints.iter().map(|(_, d)| d.as_slice()));
    // Whether the meshes carry the shell sheet themselves. Reads headers, never pixels — but
    // it still walks each mesh's bytes looking for them, so it's a closure rather than a
    // value: the rider viewer reloads on every slot edit, and a load that names its paint has
    // already answered the question without asking.
    let mesh_carries_shell = || {
        let mut names: Vec<String> = Vec::new();
        for (d, _) in &drawn {
            for t in edf::embedded_textures(d) {
                if !names.iter().any(|h| h.eq_ignore_ascii_case(&t.name)) {
                    names.push(t.name);
                }
            }
        }
        mesh_supplies_side(&names, &shell_declared, false)
    };
    // With no `.pnt` to offer, the shell wears what the mesh already carries. Helmets and
    // boots nearly always ship a paint, so this reads as an edge case there — on the
    // protection slot it's the norm: a chain, a bib or a chest protector bakes its look into
    // the `.edf` and ships an empty `paints/` folder. Asked for a paint that doesn't exist,
    // the binder had nothing to hand each piece and the whole item came out bare grey.
    //
    // Naming no paint at all is that same answer arrived at from the other side: an empty
    // slot is the loadout saying "the model's own look", not "any paint will do", and letting
    // it fall through to `pick_gear_paint` dressed the rider in whichever paint the mod
    // happened to list first. Only where the mesh actually carries the shell sheet, though —
    // the Bell Moto 10 ships paints and embeds only a tear-off film, and forcing stock there
    // would draw the helmet near-blank instead. Same question the library's picker asks
    // before it offers "Stock", asked through the same function, so the two can't disagree.
    //
    // Only the shell. Unpainted goggles already have somewhere to go — they fall back to the
    // shell's texture, which is where a helmet that doesn't paint them apart drew them — and
    // a helmet whose shell paint repaints the goggles too would lose that to the mesh's own.
    let stock = stock || paints.is_empty() || (want.is_none() && mesh_carries_shell());
    // A stock side decodes nothing from `paints/` — the mesh already carries that texture.
    let main_pnt = (!stock)
        .then(|| pick_gear_paint(&paints, want.as_deref(), &part))
        .flatten()
        .unwrap_or_default();
    let goggle_pnt = (!stock_goggles)
        .then(|| pick_gear_paint(&goggle_paints, want_goggles.as_deref(), &format!("{part} goggle")))
        .flatten()
        .unwrap_or_default();
    let names_of = |texs: &[paint::PntTexture]| texs.iter().map(|t| t.name.clone()).collect();
    let mut main_side = GearSide::new(names_of(&main_pnt));
    let mut goggle_side = GearSide::new(names_of(&goggle_pnt));
    let mut out: Vec<paint::PaintTexture> =
        main_pnt.into_par_iter().chain(goggle_pnt).map(paint::into_texture).collect();
    // The look the model ships with, before any paint: the textures embedded in the meshes.
    if stock || stock_goggles {
        let mut embedded: Vec<paint::PaintTexture> = Vec::new();
        for (d, _) in &drawn {
            for t in paint::extract_edf_textures(d) {
                if !embedded.iter().any(|h| h.name.eq_ignore_ascii_case(&t.name)) {
                    embedded.push(t);
                }
            }
        }
        let (emb_goggle, emb_main): (Vec<String>, Vec<String>) =
            embedded.iter().map(|t| t.name.clone()).partition(|n| is_goggle_name(n));
        if stock {
            main_side = GearSide::new(emb_main);
            if main_side.primary.is_none() {
                log::warn!("[rider] {part} has no stock texture in its mesh — showing it bare");
            }
        }
        if stock_goggles {
            goggle_side = GearSide::new(emb_goggle);
        }
        // A paint reuses the mesh's texture names (that's how it replaces them), so with
        // one side stock and the other painted the two sets collide. Resolve it here: the
        // stock side's embedded texture wins, the painted side keeps its `.pnt`. The
        // frontend maps textures by name and would otherwise show whichever image
        // happened to finish loading last.
        let claimed: Vec<String> = [stock.then_some(&main_side), stock_goggles.then_some(&goggle_side)]
            .into_iter()
            .flatten()
            .flat_map(|s| s.names.iter())
            .map(|n| n.to_ascii_lowercase())
            .collect();
        out.retain(|t| !claimed.contains(&t.name.to_ascii_lowercase()));
        let taken: std::collections::HashSet<String> =
            out.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        out.extend(
            embedded
                .into_iter()
                .filter(|t| !taken.contains(&t.name.to_ascii_lowercase())),
        );
    }
    // Both sides' names as one list, for the binder — counted even when nothing is painted,
    // since a stock preview counts the same slots.
    let mut declared = shell_declared;
    for n in goggle_declared {
        if !declared.iter().any(|h| h.eq_ignore_ascii_case(&n)) {
            declared.push(n);
        }
    }
    for (d, nodes) in &mut drawn {
        bind_gear_submeshes(nodes, Some(d.as_slice()), &main_side, &goggle_side, &declared);
    }
    let nodes: Vec<edf::EdfNode> = drawn.into_iter().flat_map(|(_, n)| n).collect();
    // Pieces the paints don't cover keep the mesh's own texture — a tear-off film, a visor,
    // anything the author baked in and left out of the `.pnt`. The game draws those from the
    // mesh, so they have to travel with it; without them the Bell Moto 10's tear-off had
    // nothing to wear and took the helmet's paint across it. Decoded by name, so a mesh
    // holding several 4K sheets only pays for the ones actually on screen.
    let shipped: std::collections::HashSet<String> =
        out.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    let worn: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| n.texture.iter().chain(n.submeshes.iter().filter_map(|s| s.texture.as_ref())))
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !shipped.contains(t))
        .collect();
    for d in meshes.iter().filter(|_| !worn.is_empty()) {
        for t in paint::extract_edf_textures_where(d, |n| worn.contains(&n.to_ascii_lowercase())) {
            if !out.iter().any(|h| h.name.eq_ignore_ascii_case(&t.name)) {
                out.push(t);
            }
        }
    }
    log::info!(
        "[viewer] {part}: paint={want:?} goggles={want_goggles:?} stock={stock}/{stock_goggles} \
         -> shell {:?}, goggles {:?} ({} textures)",
        main_side.primary,
        goggle_side.primary,
        out.len(),
    );
    Ok(RiderPart { part, nodes, textures: out, skeleton: Vec::new(), skin: None })
}

/// One file out of a gear folder or archive, by base name.
pub fn gear_file<'a>(files: &'a [(String, Vec<u8>)], want: &str) -> Option<&'a Vec<u8>> {
    files
        .iter()
        .find(|(n, _)| n.rsplit('/').next().unwrap_or(n).eq_ignore_ascii_case(want))
        .map(|(_, d)| d)
}

/// Every `.edf` a gear item draws, in the order it declares them.
///
/// Follow `gfx.cfg` → `<piece>.hrc` → `level0 { scene }`, the same chain the game walks and
/// the bike loader already uses. Worth following on gear because a gear mesh is named for the
/// piece rather than the slot — `neckbrace.edf`, `pickaxe.edf`, `protection.edf` all turn up
/// in the protection folder — so which `.edf` is *the* one is the mod's answer to give, not
/// ours to guess from filenames.
///
/// A gear item is not always one mesh: the stock `full` protection declares an `armour` and a
/// `neckbrace`, each its own `.edf`, and drawing whichever block came first left the rider
/// wearing half the item — a different half from one run to the next, since the blocks are
/// keyed by name rather than kept in file order.
pub fn gear_scenes(files: &[(String, Vec<u8>)]) -> Vec<String> {
    let Some(gfx) = gear_file(files, "gfx.cfg").map(|d| cfg::parse(d)) else {
        return Vec::new();
    };
    // Three spellings are in use and all three are the item: `model = x.hrc` at the top level
    // (helmets), `<piece> { model = x.hrc }` (protection), `<piece> { model { file = x.hrc } }`
    // (boots). `cockpit` is the first-person mesh and `shadow` the blob cast under the rider —
    // neither is ever drawn on the model.
    let mut hrcs: Vec<String> = Vec::new();
    let mut push = |name: Option<&str>| {
        if let Some(n) = name.filter(|n| !n.is_empty()) {
            if !hrcs.iter().any(|h| h.eq_ignore_ascii_case(n)) {
                hrcs.push(n.to_string());
            }
        }
    };
    push(gfx.get("model"));
    let mut blocks: Vec<&String> = gfx.blocks.keys().collect();
    blocks.sort();
    for name in blocks {
        if matches!(name.as_str(), "cockpit" | "shadow") {
            continue;
        }
        let b = &gfx.blocks[name];
        push(b.get("model").or_else(|| b.block("model").and_then(|m| m.get("file"))));
    }
    let mut scenes: Vec<String> = Vec::new();
    for hrc in &hrcs {
        let Some(scene) = gear_file(files, hrc)
            .map(|d| cfg::parse(d))
            .and_then(|c| cfg::hrc_level0_scene(&c))
        else {
            continue;
        };
        // Two `.hrc`s can name one mesh — the boots' left and right both point at `boots.edf`,
        // which already holds both feet — so a repeat is one piece, not two.
        if !scenes.iter().any(|s| s.eq_ignore_ascii_case(&scene)) {
            scenes.push(scene);
        }
    }
    scenes
}

/// The gear file carrying the visible mesh — not the `_s` shadow or the `c_` cockpit variant.
pub fn is_visible_gear_mesh(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    base.ends_with(".edf") && !base.ends_with("_s.edf") && !base.starts_with("c_")
}

/// Normal (`_n`) and reflection (`_r`) maps ride alongside a colour texture and are never
/// the look itself. Mirrors the filter in `paint::extract_edf_textures`, and shares the
/// exporter-spelled names with [`edf::is_companion_texture`] so the two can't drift.
/// `_s` is left out on purpose: the mesh-side filter reads it as MX Bikes' specular map,
/// but a `.pnt` may legitimately name a texture that way.
pub fn is_companion_map(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with("_n") || n.ends_with("_r") || edf::is_exporter_companion(&n)
}

/// Goggles (and their lens) are the one gear part painted separately from the shell —
/// the same test decides which submesh wears which texture, and which embedded texture
/// is the stock goggle.
pub fn is_goggle_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("goggle") || n.contains("lens")
}

/// Pick a gear paint by name, else the first available so the piece is always textured —
/// a stale or unknown name must never leave the gear bare grey. `what` labels the side in
/// the log, since a miss is exactly how a picked paint ends up looking like no change.
pub fn pick_gear_paint(
    paints: &[(String, &Vec<u8>)],
    want: Option<&str>,
    what: &str,
) -> Option<Vec<paint::PntTexture>> {
    let hit = want.and_then(|w| paints.iter().find(|(n, _)| n.eq_ignore_ascii_case(w)));
    if let (Some(w), None, false) = (want, hit, paints.is_empty()) {
        log::warn!("[rider] {what} paint '{w}' not found; used first of {} available", paints.len());
    }
    paint::decode_any(hit.or_else(|| paints.first())?.1).ok()
}

/// Every texture name a set of `.pnt` files supplies, deduplicated. Headers only — no pixels
/// are inflated, so this stays cheap enough to run on every paint an item ships.
pub fn paint_texture_names<'a>(paints: impl Iterator<Item = &'a [u8]>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for data in paints {
        for name in paint::texture_names_any(data).unwrap_or_default() {
            if !out.iter().any(|n: &String| n.eq_ignore_ascii_case(&name)) {
                out.push(name);
            }
        }
    }
    out
}

/// Which side of the item a piece belongs to, given the texture the mesh draws it from.
///
/// The paints decide it. A `.pnt` replaces textures *by name*, so the side that supplies
/// the name a piece is drawn from is the side that piece is on — the mod's own answer,
/// stated in its own files. Only when neither side claims the name (or the mesh names none)
/// does the piece's spelling get a say.
///
/// It has to be this way round, because a helmet names its goggle group after the goggle it
/// ships: the Bell Moto 10's goggle submeshes are called `Armega.001` and drawn from
/// `Racecraft`, neither of which reads as "goggles" — so on names alone the whole goggle
/// went out wearing the helmet's paint.
pub(crate) fn on_goggle_side(emb: Option<&str>, spelled_goggle: bool, main: &GearSide, goggle: &GearSide) -> bool {
    match emb {
        Some(e) if goggle.supplies(e).is_some() => true,
        Some(e) if main.supplies(e).is_some() => false,
        _ => spelled_goggle,
    }
}

/// Bind each submesh (or single-material node) to the texture it should wear: goggles take
/// the goggle paint, everything else the shell paint.
///
/// The mesh settles which texture a piece was drawn against — a submesh's material names it,
/// the same reading the bike viewer uses — and [`on_goggle_side`] settles whose texture that
/// is. A paint replaces textures by name, so a side wearing one it doesn't supply falls back
/// to its primary; unmatched → `None`, so the frontend renders neutral grey rather than
/// smearing another part's texture over it.
pub(crate) fn bind_gear_submeshes(
    nodes: &mut [edf::EdfNode],
    mesh: Option<&[u8]>,
    main: &GearSide,
    goggle: &GearSide,
    // Every texture name the item's own paints supply — see `paint_texture_names`.
    declared: &[String],
) {
    let colors = mesh.map(|d| edf::declared_colors(d, declared)).unwrap_or_default();
    // Names the mesh carries pixels for, so a piece no paint covers can keep its own look.
    let embedded: Vec<String> = mesh
        .map(|d| edf::embedded_textures(d).into_iter().map(|t| t.name).collect())
        .unwrap_or_default();
    let carried = |want: &str| embedded.iter().any(|n| n.eq_ignore_ascii_case(want));
    // What this side puts on a piece the mesh draws from `emb`.
    let wear = |emb: Option<&str>, goggles_here: bool| -> Option<String> {
        let (side, other) = if goggles_here { (goggle, main) } else { (main, goggle) };
        emb.and_then(|e| side.supplies(e))
            // No paint replaces this one, but the mesh has it: that IS the piece's look, and
            // it beats stretching the side's paint over something it was never drawn for.
            .or_else(|| emb.filter(|e| carried(e)).map(str::to_string))
            .or_else(|| side.primary.clone())
            // A helmet whose goggles aren't painted apart still shows them — in the shell's
            // texture, which is where they were drawn.
            .or_else(|| goggles_here.then(|| other.primary.clone()).flatten())
    };
    for node in nodes.iter_mut() {
        // A material id is local to its node — resolve it through that node's own table.
        let material_texture = |mat: Option<u32>| -> Option<&str> {
            let slot = node.materials.get(mat? as usize).copied().flatten()?;
            colors.get(slot).map(String::as_str)
        };
        let node_goggle = is_goggle_name(&node.name);
        if node.submeshes.is_empty() {
            // No submesh table means no material index to look up. Take a texture from this
            // side of the mesh, so a goggle node isn't handed the shell's.
            let emb = colors
                .iter()
                .map(String::as_str)
                .find(|n| on_goggle_side(Some(n), is_goggle_name(n), main, goggle) == node_goggle);
            node.texture = wear(emb, node_goggle);
            continue;
        }
        for sm in &mut node.submeshes {
            let emb = material_texture(sm.mat);
            let spelled = node_goggle || is_goggle_name(&sm.name) || emb.is_some_and(is_goggle_name);
            sm.texture = wear(emb, on_goggle_side(emb, spelled, main, goggle));
        }
    }
}

/// One loose file out of an unpacked gear folder, in the form the rest of the loader reads.
///
/// A mod that ships as a plain folder may still seal its individual files the way a `.pkz`
/// seals its entries — the Tactical Vest on mxb-mods does, and read raw its `.edf` isn't a
/// mesh at all, so the whole item failed with "no gear mesh found". Anything already plain
/// passes straight through.
pub fn read_gear_file(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    Some(pkz::read_sidecar_blob(&bytes).unwrap_or(bytes))
}

/// Every file a gear item is made of — from its folder *and* from the `.pkz` beside it.
///
/// A packed helmet is one file, but a paint installed for it later is a loose `.pnt` in a
/// folder of the same name next to it: that's where the game looks, and it's where this
/// app's paint studio writes one. Reading only whichever of the two the caller resolved
/// meant a folder holding nothing but paints hid the archive entirely — the picker listed
/// the new paint alone and the preview lost the mesh it belongs to. So both are read, the
/// folder's copy winning a name clash because it's the one installed last.
pub fn read_gear_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let stem = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = stem.strip_suffix(".pkz").unwrap_or(&stem);
    let (dir, pkz) = match p.parent() {
        Some(parent) => (parent.join(stem), parent.join(format!("{stem}.pkz"))),
        None => return read_gear_source(p),
    };
    if !dir.is_dir() || !pkz.is_file() {
        return read_gear_source(p);
    }
    // Neither side is allowed to take the other down with it: a `.pkz` that won't open is a
    // reason to show the folder's paints on their own, not to fail the whole item.
    let mut out = read_gear_source(&dir).unwrap_or_default();
    let mut have: Vec<String> = out.iter().map(|(n, _)| gear_entry_key(n)).collect();
    for (name, bytes) in read_gear_source(&pkz).unwrap_or_default() {
        let key = gear_entry_key(&name);
        if !have.contains(&key) {
            have.push(key);
            out.push((name, bytes));
        }
    }
    // Both empty says nothing about *why*; let the caller's own path report it.
    if out.is_empty() {
        return read_gear_source(p);
    }
    Ok(out)
}

/// What two spellings of the same gear file have in common: `helmets/Foo/paints/red.pnt`
/// from an archive and `paints/red.pnt` from the folder beside it are one entry, while
/// `paints/red.pnt` and `goggles/red.pnt` stay two.
pub fn gear_entry_key(name: &str) -> String {
    let n = name.replace('\\', "/").to_ascii_lowercase();
    let base = n.rsplit('/').next().unwrap_or(&n).to_string();
    match n.rsplit('/').nth(1) {
        Some(folder @ ("paints" | "goggles")) => format!("{folder}/{base}"),
        _ => base,
    }
}

pub fn read_gear_source(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::Context;
    if p.is_dir() {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            if path.is_file() {
                if let (Some(name), Some(bytes)) =
                    (path.file_name().and_then(|n| n.to_str()), read_gear_file(&path))
                {
                    out.push((name.to_string(), bytes));
                }
            }
        }
        for sub in ["paints", "goggles"] {
            if let Ok(rd) = std::fs::read_dir(p.join(sub)) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if let (Some(name), Some(bytes)) =
                        (path.file_name().and_then(|n| n.to_str()), read_gear_file(&path))
                    {
                        out.push((format!("{sub}/{name}"), bytes));
                    }
                }
            }
        }
        return Ok(out);
    }
    // A sealed file stays sealed when it's zipped up: pack the Tactical Vest's folder into a
    // `.pkz` — which is what anyone tidying a mods folder does — and every entry comes back
    // as a blob rather than the mesh and paints it holds. Unwrap them the same way the loose
    // files above are unwrapped; anything already plain passes straight through.
    Ok(pkz::read_all(p)?
        .into_iter()
        .map(|(n, d)| {
            let d = pkz::read_sidecar_blob(&d).unwrap_or(d);
            (n, d)
        })
        .collect())
}

pub(crate) struct GearSpec {
    part: &'static str,
    /// Folders under `mods/rider` this slot's models live in. The first is the game's own —
    /// where a new install goes — and any after it are read for what earlier versions of this
    /// app wrote somewhere else. See [`game::PROTECTION_AREAS`].
    mods_kind: &'static [&'static str],
    pkz_kind: &'static str,
    mesh: &'static str,
    default_name: &'static str,
}

const GEAR: [GearSpec; 3] = [
    GearSpec { part: "helmet", mods_kind: &["helmets"], pkz_kind: "helmets", mesh: "helmet.edf", default_name: "default" },
    GearSpec { part: "boots", mods_kind: &["boots"], pkz_kind: "boots", mesh: "boots.edf", default_name: "default" },
    GearSpec { part: "protection", mods_kind: game::PROTECTION_AREAS, pkz_kind: "protections", mesh: "armour.edf", default_name: "full" },
];

/// Everywhere a gear model of this slot could be installed, in the order they're preferred:
/// each of the slot's folders as an unpacked `<model>/` and as a packed `<model>.pkz`.
pub(crate) fn gear_sources(rider: &std::path::Path, spec: &GearSpec, stem: &str) -> Vec<std::path::PathBuf> {
    spec.mods_kind
        .iter()
        .flat_map(|kind| {
            let dir = rider.join(kind);
            [dir.join(stem), dir.join(format!("{stem}.pkz"))]
        })
        .collect()
}

pub(crate) fn load_gear(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    spec: &GearSpec,
    model: &str,
    paint: &str,
    goggles: &str,
    profile: &str,
) -> Option<RiderPart> {
    let stem = model.trim_end_matches(".pkz");
    let sources = gear_sources(base, spec, stem);
    // A goggle paint routinely ships apart from the helmet it's worn with — under the
    // rider profile, or loose beside a `.pkz` the loader can't write into. Gather those so
    // a name the picker offered always resolves to a paint instead of falling back to
    // whichever goggle happened to be packed first.
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    if !goggles.is_empty() {
        if !model.is_empty() {
            for src in &sources {
                extra.extend(loose_paints(&src.join("goggles"), "goggles"));
            }
        }
        if !profile.is_empty() {
            extra.extend(loose_paints(&base.join("riders").join(profile).join("goggles"), "goggles"));
        }
    }

    if !model.is_empty() {
        for (i, src) in sources.iter().enumerate() {
            if !src.exists() {
                continue;
            }
            // The same model can be installed more than one way at once — a paint pack for a
            // packaged mod has nowhere to go but a folder beside it — and only one of them is
            // opened here. Carry the named paint over from the others, so a name the picker
            // offered can't quietly render as whichever paint happened to be first.
            let mut extra = extra.clone();
            for other in sources.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| p) {
                if other.exists() {
                    extra.extend(gear_paint_from(other, "paints", paint));
                    extra.extend(gear_paint_from(other, "goggles", goggles));
                }
            }
            match load_gear_model_blocking(
                src.to_string_lossy().into_owned(),
                spec.part.to_string(),
                Some(paint.to_string()),
                Some(goggles.to_string()),
                // The rider wears what the loadout names; "stock" is a preview-only choice.
                false,
                false,
                extra,
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
    // Stock / "free" gear: mesh and paints ship separately in the game pkz, so bind
    // submeshes here too (installed gear is bound in load_gear_model_blocking).
    let name = if model.is_empty() { spec.default_name } else { model };
    let pkz = resolve_game_pkz(cfg, "rider.pkz")?;
    let folder = format!("rider/{}/{}", spec.pkz_kind, name);
    // What the folder says it draws, asked the same way an installed mod is asked. `full`
    // declares two pieces — the chest protector and the neck brace worn with it — so taking
    // only the slot's usual mesh name dressed the rider in half the item.
    let mut drawn: Vec<(String, Vec<edf::EdfNode>)> = gear_scenes(&pkz_gear_cfgs(&pkz, &folder))
        .iter()
        .map(|s| format!("{folder}/{s}"))
        .filter_map(|e| load_pkz_mesh(&pkz, &e).map(|n| (e, n)))
        .collect();
    if drawn.is_empty() {
        let named = format!("{folder}/{}", spec.mesh);
        drawn.push(match load_pkz_mesh(&pkz, &named) {
            Some(n) => (named, n),
            None => {
                let alt = stock_gear_entry(&pkz, &folder)?;
                let n = load_pkz_mesh(&pkz, &alt)?;
                (alt, n)
            }
        });
    }
    let shell_texs = load_pkz_paint(&pkz, &folder, "paints", paint);
    // A stock folder can ship no paint at all — `rider/protections/{full,neck}` carry none —
    // and then the mesh's own textures are the look, exactly as they are for an installed mod
    // that bakes it in. Without this the whole piece came out bare grey.
    let stock_shell = shell_texs.is_empty();
    let mut main_side = GearSide::new(shell_texs.iter().map(|t| t.name.clone()).collect());
    // Stock gear paints its goggles apart from the shell just as an installed helmet does:
    // from the rider profile's own folder where it has one, else from `rider.pkz`.
    let goggle_texs: Vec<paint::PaintTexture> = if goggles.is_empty() {
        Vec::new()
    } else {
        loose_paint_named(&extra, "goggles", goggles)
            .unwrap_or_else(|| load_pkz_paint(&pkz, &folder, "goggles", goggles))
    };
    let goggle_side = GearSide::new(goggle_texs.iter().map(|t| t.name.clone()).collect());
    if !goggles.is_empty() && goggle_side.primary.is_none() {
        log::warn!("[rider] goggle paint '{goggles}' not found for stock {}", spec.part);
    }
    let mut textures: Vec<paint::PaintTexture> =
        shell_texs.into_iter().chain(goggle_texs).collect();
    // The mesh's own materials are what the binder reads to tell one piece from the next, so
    // every source that needs them pays for the archive read — the nodes themselves stay
    // cached. Skipped only when a paint covers the shell and nothing is goggled.
    let need_mesh = stock_shell || !goggles.is_empty();
    let bytes: Vec<Option<Vec<u8>>> = drawn
        .iter()
        .map(|(e, _)| need_mesh.then(|| read_pkz_entry(&pkz, e)).flatten())
        .collect();
    if stock_shell {
        let mut embedded: Vec<paint::PaintTexture> = Vec::new();
        for d in bytes.iter().flatten() {
            for t in paint::extract_edf_textures(d) {
                if !embedded.iter().any(|h| h.name.eq_ignore_ascii_case(&t.name)) {
                    embedded.push(t);
                }
            }
        }
        main_side = GearSide::new(
            embedded.iter().map(|t| t.name.clone()).filter(|n| !is_goggle_name(n)).collect(),
        );
        if main_side.primary.is_none() {
            log::warn!("[rider] stock {} '{name}' has no texture of its own", spec.part);
        }
        // The goggle side keeps its own `.pnt`; a paint reuses the mesh's names, so only what
        // it doesn't supply comes off the mesh.
        let taken: std::collections::HashSet<String> =
            textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        textures
            .extend(embedded.into_iter().filter(|t| !taken.contains(&t.name.to_ascii_lowercase())));
    }
    // The stock model's own paints are what it declares — both sides' names are already in
    // hand here, decoded from `rider.pkz` above.
    let declared: Vec<String> =
        main_side.names.iter().chain(goggle_side.names.iter()).cloned().collect();
    let mut nodes = Vec::new();
    for ((_, mut n), d) in drawn.into_iter().zip(&bytes) {
        bind_gear_submeshes(&mut n, d.as_deref(), &main_side, &goggle_side, &declared);
        nodes.extend(n);
    }
    log::info!(
        "[rider] {} stock '{name}' loaded: {} nodes, tex={:?} goggles={:?}",
        spec.part,
        nodes.len(),
        main_side.primary,
        goggle_side.primary,
    );
    Some(RiderPart {
        part: spec.part.into(),
        nodes,
        textures,
        skeleton: Vec::new(),
        skin: None,
    })
}

/// The small text files a stock gear folder ships — `gfx.cfg` and the `.hrc`s it names —
/// read out of the game archive in the shape [`gear_scenes`] reads an installed mod's folder.
/// A few hundred bytes each, against a 100 MB archive, so ask before guessing at mesh names.
pub fn pkz_gear_cfgs(pkz: &std::path::Path, folder: &str) -> Vec<(String, Vec<u8>)> {
    let prefix = format!("{}/", folder.replace('\\', "/").to_ascii_lowercase());
    pkz::read_selected(pkz, |n| {
        let n = n.replace('\\', "/").to_ascii_lowercase();
        n.starts_with(&prefix) && (n.ends_with(".cfg") || n.ends_with(".hrc"))
    })
    .unwrap_or_default()
}

/// The `.edf` a stock gear folder in `rider.pkz` actually carries, for the folders that
/// don't answer to the slot's usual name.
///
/// Protection is where this bites: the slot expects `armour.edf`, which is the chest
/// protector's name — the neck brace beside it is its own mesh, and asking for a name that
/// isn't there left the slot silently empty rather than wrong, which is harder to notice.
/// Only reached once the expected name has already missed.
pub fn stock_gear_entry(pkz: &std::path::Path, folder: &str) -> Option<String> {
    let prefix = format!("{}/", folder.replace('\\', "/").to_ascii_lowercase());
    let mut found: Vec<String> = pkz::read_selected(pkz, |n| {
        let n = n.replace('\\', "/").to_ascii_lowercase();
        n.starts_with(&prefix) && is_visible_gear_mesh(&n)
    })
    .ok()?
    .into_iter()
    .map(|(n, _)| n.replace('\\', "/"))
    .collect();
    // Deterministic rather than archive-order, so the same folder always resolves the same.
    found.sort_by_key(|n| n.to_ascii_lowercase());
    let hit = found.into_iter().next()?;
    log::info!("[rider] stock '{folder}' doesn't carry the slot's mesh; using '{hit}'");
    Some(hit)
}

/// One named `.pnt` out of a gear source — an unpacked folder or a `.pkz`, whichever it is —
/// named the way a gear archive carries it so the loader reads it like any packed paint.
///
/// Just the one paint: a helmet folder holds dozens, and this runs to cover the *other*
/// install of a model the loader already has open, so reading them all would be paying a
/// hundred megabytes for a file it needs one of. No name wanted, or a source that doesn't
/// carry it → nothing, and the loader falls back exactly as it did before.
pub fn gear_paint_from(src: &std::path::Path, sub: &str, want: &str) -> Vec<(String, Vec<u8>)> {
    if want.is_empty() {
        return Vec::new();
    }
    let entry = format!("{sub}/{want}.pnt");
    if src.is_dir() {
        return std::fs::read(src.join(sub).join(format!("{want}.pnt")))
            .map(|d| vec![(entry, d)])
            .unwrap_or_default();
    }
    pkz::read_selected(src, |n| {
        gear_folder_paint_name(n, sub).is_some_and(|p| p.eq_ignore_ascii_case(want))
    })
    .map(|hits| hits.into_iter().map(|(_, d)| (entry.clone(), d)).collect())
    .unwrap_or_default()
}

/// Loose `.pnt` files in a folder, named as a gear archive would carry them so the loader
/// reads them exactly like packed ones. Missing folder → nothing, which is the norm.
pub fn loose_paints(dir: &std::path::Path, folder: &str) -> Vec<(String, Vec<u8>)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pnt")))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            Some((format!("{folder}/{name}"), std::fs::read(&p).ok()?))
        })
        .collect()
}

/// Decode one named paint out of a gathered set — no fallback: a miss means the caller
/// should look elsewhere, not that some other paint will do.
pub fn loose_paint_named(
    files: &[(String, Vec<u8>)],
    folder: &str,
    want: &str,
) -> Option<Vec<paint::PaintTexture>> {
    let hit = files.iter().find(|(n, _)| {
        gear_folder_paint_name(n, folder).is_some_and(|p| p.eq_ignore_ascii_case(want))
    })?;
    Some(paint::decode_any(&hit.1).ok()?.into_par_iter().map(paint::into_texture).collect())
}

/// A paint the game itself ships, from `<folder>/<sub>/<paint>.pnt` — `sub` being `paints`
/// for the piece's own look or `goggles` for the goggles worn with it.
///
/// A name that misses falls back to the first paint in the folder, so a stale name still shows
/// the piece textured rather than bare grey. *No name* is a different answer and must not reach
/// that fallback: an empty slot is the loadout saying "the model's own look", and dressing it in
/// whichever `.pnt` sorts first is how the stock helmet came out bronze — `black_yellow` leads
/// `rider/helmets/default/paints/`, while the mesh's own sheet is white. Nothing here, and the
/// caller falls through to the mesh's embedded textures, which is what stock means.
pub fn load_pkz_paint(
    pkz: &std::path::Path,
    folder: &str,
    sub: &str,
    paint: &str,
) -> Vec<paint::PaintTexture> {
    if paint.is_empty() {
        return Vec::new();
    }
    read_pkz_entry(pkz, &format!("{folder}/{sub}/{paint}.pnt"))
        .or_else(|| read_pkz_first(pkz, &format!("{folder}/{sub}/"), ".pnt"))
        .and_then(|d| paint::decode_any(&d).ok())
        .map(|p| p.into_par_iter().map(paint::into_texture).collect())
        .unwrap_or_default()
}

pub fn read_pkz_first(pkz: &std::path::Path, prefix: &str, ext: &str) -> Option<Vec<u8>> {
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

pub fn load_rider_paint(
    cfg: &config::AppConfig,
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
    let profile = rider_profile_or_stock(profile);
    let data = read_rider_paint_file(cfg, base, profile, sub, paint)?;
    let textures: Vec<_> =
        paint::decode_any(&data).ok()?.into_par_iter().map(paint::into_texture).collect();
    if textures.is_empty() {
        return None;
    }
    Some(RiderPart {
        part: part.into(),
        nodes: Vec::new(),
        textures,
        skeleton: Vec::new(),
        skin: None,
    })
}

/// A kit or glove paint for `profile`, by exact name.
///
/// A rider model isn't a wardrobe. Rider+ ships its `paints` and `gloves` folders empty on
/// purpose — the kits already installed under the stock profile are meant to work on it —
/// so looking only inside the chosen profile drops every paint the picker offered. Look in
/// the profile's own folder, then inside its archive where it's packed, then under the
/// stock profiles.
///
/// Exact name at every step, and never the first paint in a folder: reaching past the
/// chosen profile is only safe while the name still means the same paint.
pub fn read_rider_paint_file(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    profile: &str,
    sub: &str,
    paint: &str,
) -> Option<Vec<u8>> {
    let riders = base.join("riders");
    let game = resolve_game_pkz(cfg, "rider.pkz");
    let candidates =
        std::iter::once(profile).chain(STOCK_RIDER_PROFILES.into_iter().filter(|s| *s != profile));

    for from in candidates {
        // Installed loose, then packed as `<profile>.pkz`, then — for a stock profile, whose
        // kits ship inside the game archive and never touch the disk — the game's own copy.
        let hit = read_paint_file(&riders.join(from).join(sub), paint)
            .or_else(|| {
                let packed = riders.join(format!("{from}.pkz"));
                packed.is_file().then(|| read_pkz_paint_named(&packed, sub, paint)).flatten()
            })
            .or_else(|| {
                let pkz = game.as_ref()?;
                read_pkz_entry(pkz, &format!("rider/riders/{from}/{sub}/{paint}.pnt"))
            });
        if let Some(d) = hit {
            if from != profile {
                log::info!("[rider] {sub} '{paint}' for '{profile}' came from '{from}'");
            }
            return Some(d);
        }
    }
    None
}

/// A named paint out of an archive, matched on its folder as well as its name — `red.pnt`
/// under `gloves` is not the `red.pnt` under `paints`.
pub fn read_pkz_paint_named(pkz: &std::path::Path, sub: &str, paint: &str) -> Option<Vec<u8>> {
    let tail = format!("/{}/{}.pnt", sub.to_ascii_lowercase(), paint.to_ascii_lowercase());
    let want = |n: &str| n.replace('\\', "/").to_ascii_lowercase().ends_with(&tail);
    pkz::read_selected(pkz, want).ok()?.into_iter().next().map(|(_, d)| d)
}

pub fn read_paint_file(dir: &std::path::Path, paint: &str) -> Option<Vec<u8>> {
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

#[cfg(test)]
mod gear_bind_tests {
    use super::{bind_gear_submeshes, edf, GearSide};

    fn submesh(name: &str) -> edf::Submesh {
        edf::Submesh {
            name: name.into(),
            tri_start: 0,
            tri_count: 1,
            texture: None,
            uv_tile: None,
            mat: None,
        }
    }

    fn node(name: &str, subs: &[&str]) -> edf::EdfNode {
        edf::EdfNode {
            name: name.into(),
            positions: Vec::new(),
            uvs: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            submeshes: subs.iter().map(|s| submesh(s)).collect(),
            texture: None,
            placed: false,
            materials: Vec::new(),
        }
    }

    fn side(names: &[&str]) -> GearSide {
        GearSide::new(names.iter().map(|s| s.to_string()).collect())
    }

    fn bound(nodes: &[edf::EdfNode]) -> Vec<Option<String>> {
        nodes
            .iter()
            .flat_map(|n| {
                if n.submeshes.is_empty() {
                    vec![n.texture.clone()]
                } else {
                    n.submeshes.iter().map(|s| s.texture.clone()).collect()
                }
            })
            .collect()
    }

    // Without the mesh's materials to read, the names are all there is to go on.
    #[test]
    fn a_named_goggle_submesh_wears_the_goggle_paint() {
        let mut nodes = vec![node("helmet", &["shell", "goggle", "lens"])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &side(&["smoke"]), &[]);
        assert_eq!(
            bound(&nodes),
            [Some("hjc".into()), Some("smoke".into()), Some("smoke".into())],
        );
    }

    // The goggles of many helmets are a node of their own, with no submesh table at all —
    // the case that had every goggle paint land on the shell instead.
    #[test]
    fn a_goggle_node_without_submeshes_wears_the_goggle_paint() {
        let mut nodes = vec![node("helmet", &[]), node("goggles", &[])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &side(&["smoke"]), &[]);
        assert_eq!(bound(&nodes), [Some("hjc".into()), Some("smoke".into())]);
    }

    // A goggle group inside a goggle node needn't repeat the word.
    #[test]
    fn a_submesh_inherits_its_node() {
        let mut nodes = vec![node("goggles", &["strap", "glass"])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &side(&["smoke"]), &[]);
        assert_eq!(bound(&nodes), [Some("smoke".into()), Some("smoke".into())]);
    }

    // A helmet that draws its goggles into the shell atlas still shows them.
    #[test]
    fn unpainted_goggles_fall_back_to_the_shell() {
        let mut nodes = vec![node("helmet", &["shell", "goggle"])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &GearSide::default(), &[]);
        assert_eq!(bound(&nodes), [Some("hjc".into()), Some("hjc".into())]);
    }

    // The shell never borrows the goggle paint the other way round — that's the smear.
    #[test]
    fn an_unpainted_shell_stays_bare() {
        let mut nodes = vec![node("helmet", &["shell", "goggle"])];
        bind_gear_submeshes(&mut nodes, None, &GearSide::default(), &side(&["smoke"]), &[]);
        assert_eq!(bound(&nodes), [None, Some("smoke".into())]);
    }

    // A paint replaces the mesh's textures by name, so where it supplies the exact name a
    // piece asks for, that beats the side's primary.
    #[test]
    fn a_side_supplies_the_name_the_mesh_asks_for() {
        let s = side(&["shell_n", "shell", "strap"]);
        assert_eq!(s.primary.as_deref(), Some("shell")); // never the companion map
        assert_eq!(s.supplies("STRAP").as_deref(), Some("strap")); // case-insensitive
        assert_eq!(s.supplies("visor"), None);
    }

    // A paint baked out of Substance names its maps the exporter's way, and one of those
    // taken for the look leaves the piece wearing a normal map.
    #[test]
    fn an_exporter_named_map_is_never_the_primary() {
        let s = side(&["Vest_Normal", "Vest_BaseColor"]);
        assert_eq!(s.primary.as_deref(), Some("Vest_BaseColor"));
    }
}

/// A packed gear item and a folder of the same name are one item, not two — the case the
/// paint studio creates every time it installs a paint for a `.pkz` helmet.
#[cfg(test)]
mod gear_source_tests {
    use super::{gear_entry_key, read_gear_files};

    #[test]
    fn one_entry_however_it_is_spelled() {
        assert_eq!(gear_entry_key("helmets/Foo/paints/red.pnt"), gear_entry_key("paints/red.pnt"));
        assert_eq!(gear_entry_key("helmets/Foo/helmet.edf"), gear_entry_key("helmet.edf"));
        assert_ne!(gear_entry_key("paints/red.pnt"), gear_entry_key("goggles/red.pnt"));
    }

    #[test]
    fn a_folder_of_paints_beside_a_pkz_reads_as_both() {
        let root = std::env::temp_dir().join(format!("frost-gear-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Helmet");
        std::fs::create_dir_all(dir.join("paints")).unwrap();
        std::fs::write(dir.join("paints").join("mine.pnt"), b"PNT\0mine").unwrap();
        // Not a real archive — `pkz::read_all` returning nothing is enough to prove the
        // folder's paint isn't lost, and a genuine `.pkz` is exercised by the pkz tests.
        std::fs::write(root.join("Helmet.pkz"), b"not really an archive").unwrap();

        for from in [dir.clone(), root.join("Helmet.pkz")] {
            let files = read_gear_files(&from).unwrap_or_default();
            assert!(
                files.iter().any(|(n, _)| n.ends_with("mine.pnt")),
                "the loose paint has to be visible whichever side the caller resolved ({from:?})"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod gear_scene_tests {
    use super::gear_scenes;

    fn files(entries: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
        entries.iter().map(|(n, d)| (n.to_string(), d.as_bytes().to_vec())).collect()
    }

    // A protection folder names its mesh for the piece, not the slot, so the loader has to
    // ask rather than guess: `neckbrace.edf` sits beside the shadow mesh it must not pick.
    #[test]
    fn a_mod_names_its_own_mesh() {
        let f = files(&[
            ("gfx.cfg", "neckbrace\n{\n\tmodel = neckbrace.hrc\n}\n"),
            ("neckbrace.hrc", "level0\n{\n\tscene = neckbrace.edf\n\tswitch = 0\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["neckbrace.edf"]);
    }

    // The block in `gfx.cfg` is named for the piece, and authors don't agree on the word —
    // `armour` and `neckbrace` both turn up on protection mods that ship `protection.edf`.
    #[test]
    fn the_block_name_doesnt_matter() {
        let f = files(&[
            ("gfx.cfg", "armour\n{\n\tmodel = protection.hrc\n}\n"),
            ("protection.hrc", "level0\n{\n\tscene = protection.edf\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["protection.edf"]);
    }

    // The stock `full` protection: two pieces worn together, each its own mesh. Taking one
    // block's mesh dressed the rider in half the item.
    #[test]
    fn a_two_piece_set_draws_both_pieces() {
        let f = files(&[
            (
                "gfx.cfg",
                "neckbrace\n{\n\tmodel = neckbrace.hrc\n}\n\narmour\n{\n\tmodel = armour.hrc\n}\n",
            ),
            ("armour.hrc", "level0\n{\n\tscene = armour.edf\n}\n"),
            ("neckbrace.hrc", "level0\n{\n\tscene = neckbrace.edf\n}\n"),
        ]);
        // Sorted by block name, so the same folder always draws in the same order.
        assert_eq!(gear_scenes(&f), ["armour.edf", "neckbrace.edf"]);
    }

    // Boots declare a left and a right, both pointing at the one mesh that holds both feet.
    // A repeat is one piece, not two — and the nested `model { file = … }` is the boots'
    // own spelling, which nothing else uses.
    #[test]
    fn two_blocks_naming_one_mesh_are_one_piece() {
        let f = files(&[
            (
                "gfx.cfg",
                "left\n{\n\tmodel\n\t{\n\t\tfile = left_boot.hrc\n\t}\n}\nright\n{\n\tmodel\n\t{\n\t\tfile = right_boot.hrc\n\t}\n}\n",
            ),
            ("left_boot.hrc", "level0\n{\n\tscene = boots.edf\n}\n"),
            ("right_boot.hrc", "level0\n{\n\tscene = boots.edf\n\tname = righta\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["boots.edf"]);
    }

    // A helmet names its mesh at the top level and its first-person mesh in `cockpit`. The
    // cockpit one is never drawn on the model, and picking it up put a headless shell on the
    // rider.
    #[test]
    fn the_cockpit_mesh_is_not_the_item() {
        let f = files(&[
            ("gfx.cfg", "model = helmet.hrc\nshadow = helmet_s.edf\n\ncockpit\n{\n\tmodel = c_helmet.edf\n}\n"),
            ("helmet.hrc", "level0\n{\n\tscene = helmet.edf\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["helmet.edf"]);
    }

    // Most gear ships nothing to read, and that's not an error — the loader falls back to
    // scanning the folder for a mesh.
    #[test]
    fn a_mod_that_says_nothing_answers_nothing() {
        assert!(gear_scenes(&files(&[("protection.edf", "EDF\0")])).is_empty());
    }
}

#[cfg(test)]
mod gear_paint_merge_tests {
    use super::GearPaints;

    fn paints(names: &[&str]) -> GearPaints {
        GearPaints {
            paints: names.iter().map(|s| s.to_string()).collect(),
            ..GearPaints::default()
        }
    }

    // The whole point: a model installed as a `.pkz` *and* as a folder of extra paints
    // offers both sets. Taking the first source is what showed one and hid the other.
    #[test]
    fn sources_add_up() {
        let mut all = paints(&["Black", "Red"]);
        all.absorb(paints(&["Purple White", "RDS Leopard"]));
        all.sort();
        assert_eq!(all.paints, ["Black", "Purple White", "RDS Leopard", "Red"]);
    }

    // A paint pack ships the same file names as the mod it was made for, so the same look
    // arrives twice. Twice in a dropdown is a bug, not a choice.
    #[test]
    fn a_repeat_is_the_same_look_twice() {
        let mut all = paints(&["Black", "Red"]);
        all.absorb(paints(&["black", "Flo"]));
        all.sort();
        assert_eq!(all.paints, ["Black", "Flo", "Red"]);
    }

    // Each source sorts its own names; merged, they have to sort as one list rather than
    // as one source appended to the next.
    #[test]
    fn the_merged_list_sorts_as_one() {
        let mut all = paints(&["Alpha", "Zulu"]);
        all.absorb(paints(&["Bravo"]));
        all.sort();
        assert_eq!(all.paints, ["Alpha", "Bravo", "Zulu"]);
    }

    // A "Stock" entry is worth offering as soon as any source's mesh carries its own look.
    #[test]
    fn stock_carries_across() {
        let mut all = GearPaints::default();
        all.absorb(GearPaints { has_stock: true, ..GearPaints::default() });
        assert!(all.has_stock);
        all.absorb(GearPaints::default());
        assert!(all.has_stock, "a later source without one doesn't take it away");
    }
}

/// Whether a side has a stock look at all — the question behind both the library's "Stock"
/// entry and what an empty paint slot resolves to on the rider. They read the same function,
/// so these cases pin down both at once.
#[cfg(test)]
mod stock_side_tests {
    use super::mesh_supplies_side;

    fn v(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // The ordinary helmet: the mesh carries the sheet its paints replace, so there is a stock
    // look to show and an empty slot means it.
    #[test]
    fn the_mesh_carrying_the_painted_sheet_is_a_stock_look() {
        assert!(mesh_supplies_side(&v(&["helmet", "visor"]), &v(&["helmet"]), false));
    }

    // Case is the mod author's business, not ours — a `.pnt` replaces by name regardless.
    #[test]
    fn the_match_ignores_case() {
        assert!(mesh_supplies_side(&v(&["Helmet"]), &v(&["helmet"]), false));
    }

    // The Bell Moto 10: it embeds a tear-off film and a goggle, but the shell sheet comes from
    // its paints. Calling that a stock look drew the helmet near-blank — and now it would also
    // make an empty slot render bare, which is worse than the first-paint fallback it keeps.
    #[test]
    fn a_mesh_that_leaves_the_shell_to_its_paints_has_no_stock_look() {
        assert!(!mesh_supplies_side(&v(&["tearoff", "Racecraft"]), &v(&["airoh_shell"]), false));
    }

    // Normal and roughness maps are never the look. A mesh carrying only companions carries
    // nothing to show.
    #[test]
    fn companion_maps_dont_count_as_a_look() {
        assert!(!mesh_supplies_side(&v(&["boots_n", "boots_r"]), &v(&[]), false));
        assert!(!mesh_supplies_side(&v(&["helmet_n"]), &v(&["helmet_n"]), false));
    }

    // Nothing painted on a side at all — the stock protection slot — so whatever the mesh
    // carries for that side is the only look there is.
    #[test]
    fn with_no_paints_the_mesh_is_the_only_look() {
        assert!(mesh_supplies_side(&v(&["armor"]), &v(&[]), false));
    }

    // ...and the sides don't borrow from each other: an embedded goggle is not a shell look.
    #[test]
    fn an_unpainted_side_only_counts_its_own_sheets() {
        let embedded = v(&["goggles"]);
        assert!(mesh_supplies_side(&embedded, &v(&[]), true), "the goggle side has one");
        assert!(!mesh_supplies_side(&embedded, &v(&[]), false), "the shell does not");
    }
}

#[cfg(test)]
mod body_orientation_tests {
    use super::*;

    /// A body as three marked points: the head's skin, the name planes on its back, and the
    /// boots. Enough to answer both questions the orientation asks and nothing more.
    fn body(skin: [f32; 3], back: [f32; 3], feet: [f32; 3]) -> Vec<edf::EdfNode> {
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        let mut submeshes = Vec::new();
        for (i, (p, slot)) in
            [(skin, "face"), (back, "hide"), (feet, "rider")].into_iter().enumerate()
        {
            // A triangle per part, all three corners on the same point: the orientation only
            // ever reads positions, so a degenerate triangle carries everything it needs.
            for _ in 0..3 {
                positions.extend_from_slice(&p);
                indices.push((i * 3 + indices.len() % 3) as u32);
            }
            let base = i * 3;
            indices.truncate(base);
            indices.extend_from_slice(&[base as u32, base as u32 + 1, base as u32 + 2]);
            submeshes.push(edf::Submesh {
                name: slot.into(),
                tri_start: i as u32,
                tri_count: 1,
                texture: Some(slot.into()),
                uv_tile: None,
                mat: None,
            });
        }
        vec![edf::EdfNode {
            name: "body".into(),
            positions,
            uvs: Vec::new(),
            normals: Vec::new(),
            indices,
            submeshes,
            texture: None,
            placed: true,
            materials: Vec::new(),
        }]
    }

    fn skin_top(nodes: &[edf::EdfNode]) -> f32 {
        body_bounds(nodes, Some("face")).1[1]
    }

    fn back_depth(nodes: &[edf::EdfNode]) -> f64 {
        let (lo, hi) = body_bounds(nodes, None);
        slot_depth(nodes, "hide", (lo[2] + hi[2]) / 2.0).expect("the planes are there")
    }

    /// The report: a custom body came out upside down and facing backwards while the stock
    /// ones were fine. A Z-up mesh with its head at *positive* Z takes the one fixed quarter
    /// turn and lands exactly like that, because that turn is written for the other end.
    #[test]
    fn a_body_that_lands_upside_down_is_turned_back() {
        // Already stood up, and wrong: skin at the bottom, name planes in front.
        let mut nodes = body([0.0, 0.05, 0.10], [0.0, 0.9, 0.10], [0.0, 1.7, 0.0]);
        check_body_orientation(&mut nodes);

        let (lo, hi) = body_bounds(&nodes, None);
        assert!(
            skin_top(&nodes) > lo[1] + 0.5 * (hi[1] - lo[1]),
            "the head ends up at the top ({:.3} of {:.3}..{:.3})",
            skin_top(&nodes),
            lo[1],
            hi[1],
        );
        assert!(back_depth(&nodes) < 0.0, "and the name planes end up on the back");
    }

    /// The half turn that rights a body also swings it front-to-back, so a body that is only
    /// upside down must not be left facing the wrong way by the fix for the first fault.
    #[test]
    fn righting_a_body_leaves_it_facing_forward() {
        // Upside down, but its planes are already behind it.
        let mut nodes = body([0.0, 0.05, -0.10], [0.0, 0.9, -0.10], [0.0, 1.7, 0.0]);
        check_body_orientation(&mut nodes);
        assert!(back_depth(&nodes) < 0.0, "the planes are still on the back");
    }

    /// A body that only faces the wrong way is turned about its height, which must not put
    /// its head back at the bottom.
    #[test]
    fn a_backwards_body_is_turned_without_upending_it() {
        let mut nodes = body([0.0, 1.7, 0.10], [0.0, 0.9, 0.10], [0.0, 0.05, 0.0]);
        check_body_orientation(&mut nodes);

        let (lo, hi) = body_bounds(&nodes, None);
        assert!(skin_top(&nodes) > lo[1] + 0.5 * (hi[1] - lo[1]), "the head stays at the top");
        assert!(back_depth(&nodes) < 0.0, "and it now faces forward");
    }

    /// The one that matters most: the stock bodies already arrive correct, and a check that
    /// "corrects" them is worse than no check at all.
    #[test]
    fn a_body_that_is_already_right_is_left_alone() {
        let before = body([0.0, 1.7, -0.10], [0.0, 0.9, -0.10], [0.0, 0.05, 0.0]);
        let mut after = before.clone();
        check_body_orientation(&mut after);
        assert_eq!(after[0].positions, before[0].positions, "not a vertex moves");
    }

    /// Every inch covered — kit, helmet, gloves, no skin anywhere — leaves the question
    /// unanswerable. An unturned body beats one turned on no evidence.
    #[test]
    fn a_body_showing_no_skin_is_not_guessed_at() {
        let mut nodes = body([0.0, 1.7, -0.10], [0.0, 0.9, -0.10], [0.0, 0.05, 0.0]);
        for n in nodes.iter_mut() {
            for sm in n.submeshes.iter_mut() {
                if sm.texture.as_deref() == Some("face") {
                    sm.texture = Some("rider".into());
                }
            }
        }
        let before = nodes.clone();
        check_body_orientation(&mut nodes);
        assert_eq!(nodes[0].positions, before[0].positions, "nothing is assumed");
    }
}

#[cfg(test)]
mod viewer_tests {
    use std::path::{Path, PathBuf};


    fn copy_tree(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap().flatten() {
            let (from, to) = (e.path(), dst.join(e.file_name()));
            if from.is_dir() {
                copy_tree(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }

    /// How the viewer sees a bike, as a comparable shape: which parts resolved and what
    /// each submesh is bound to. Node order is fixed by `GFX_PARTS`, so this is stable.
    fn shape(m: &super::BikeModel) -> Vec<(String, Vec<(String, Option<String>)>)> {
        m.nodes
            .iter()
            .map(|n| {
                (
                    n.name.clone(),
                    n.submeshes.iter().map(|s| (s.name.clone(), s.texture.clone())).collect(),
                )
            })
            .collect()
    }

    /// Write a plain-zip `.pkz` — `pkz::read_selected` reads those natively, so the packed
    /// half of a bike can be fixtured without the sidecar.
    fn write_pkz(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    fn named<'a>(files: &'a [(String, Vec<u8>)], base: &str) -> Option<&'a [u8]> {
        files
            .iter()
            .find(|(n, _)| {
                n.rsplit('/').next().unwrap_or(n).eq_ignore_ascii_case(base)
            })
            .map(|(_, d)| d.as_slice())
    }

    /// The fault behind a swap that renders white with every part stacked at the origin: a
    /// model set is a mesh and little else, so the bike's `.geom`, `gfx.cfg`, `.hrc`s and
    /// stock paint have nowhere to come from but the archive — which the preview used to
    /// skip the moment the variant brought a mesh.

    /// The same law on the ordinary load path: an extracted bike whose folder holds only a
    /// mesh still draws with the setup and paint left behind in its archive.
    #[test]
    fn a_loose_bike_still_reads_its_packed_setup() {
        let root: PathBuf =
            std::env::temp_dir().join(format!("frost-loose-packed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bikes = root.join("bikes");
        std::fs::create_dir_all(bikes.join("KTM450")).unwrap();
        write_pkz(
            &bikes.join("KTM450.pkz"),
            &[
                ("KTM450/model.edf", b"packed mesh"),
                ("KTM450/gfx.cfg", b"packed gfx"),
                ("KTM450/wheel.geom", b"packed geom"),
            ],
        );
        std::fs::write(bikes.join("KTM450").join("model.edf"), b"loose mesh").unwrap();

        let files = super::gather_bike_files(&bikes.join("KTM450")).expect("gather");
        assert_eq!(named(&files, "model.edf"), Some(&b"loose mesh"[..]), "loose wins");
        assert!(named(&files, "gfx.cfg").is_some(), "packed gfx.cfg comes through");
        assert!(named(&files, "wheel.geom").is_some(), "packed .geom comes through");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A mod mesh that ships companion maps used to render entirely grey: its material
    /// tables were thrown out because `w13` was assumed to be padding, and even read back they
    /// index a list `declared_colors` doesn't build — one that counts the sheets the mesh
    /// declares but never embeds. `polarm` is the proof: nothing embeds it, and it is the only
    /// candidate for the `Polar + Mount` submesh.
    ///
    /// Pinned against parts whose names name their own sheet, so a wrong index space can't
    /// pass. Needs the real tree — no synthetic `.edf` exercises this.
    ///
    /// MXB_REAL_BIKES=~/Documents/PiBoSo/"MX Bikes" \
    ///   cargo test a_companion_shipping_mesh_binds_every_part -- --ignored --nocapture
    #[test]
    #[ignore]
    fn a_companion_shipping_mesh_binds_every_part() {
        let Ok(src_root) = std::env::var("MXB_REAL_BIKES") else {
            eprintln!("set MXB_REAL_BIKES to run");
            return;
        };
        let dir = Path::new(&src_root)
            .join("mods")
            .join("bikes")
            .join("MX1OEM_2023_KTM_450_SX-F");
        if !crate::bikefiles::dir_has_mesh(&dir) {
            eprintln!("no extracted mesh at {dir:?} — skipping");
            return;
        }
        let m = super::load_bike_model_blocking(dir.to_string_lossy().to_string(), None)
            .expect("the bike loads");
        let bound: Vec<(String, String)> = m
            .nodes
            .iter()
            .flat_map(|n| n.submeshes.iter().map(|s| {
                (s.name.clone(), s.texture.clone().unwrap_or_default())
            }))
            .collect();
        for (part, sheet) in [
            ("LUXON LMM.001", "luxlmm"),
            ("pedale_low", "HHpedal"),
            ("pedale_low.002", "HHshifter"),
            ("tank_low", "rmxtank"),
            ("L master cyl.002", "asv"),
            ("ODI Grips+bar end", "ODIGRIPBAREND"),
            ("Polar + Mount", "polarm"),
            ("levers", "arclever"),
        ] {
            let got = bound.iter().find(|(n, _)| n == part).map(|(_, t)| t.as_str());
            assert_eq!(got, Some(sheet), "{part} should wear {sheet}");
        }
        assert!(
            bound.iter().all(|(_, t)| !t.is_empty()),
            "every submesh should be bound: {:?}",
            bound.iter().filter(|(_, t)| t.is_empty()).collect::<Vec<_>>(),
        );
    }



    fn tex(name: &str, token: &str) -> crate::paint::PaintTexture {
        crate::paint::PaintTexture {
            name: name.into(),
            width: 4,
            height: 4,
            token: token.into(),
        }
    }

    /// Evicting a bike has to free every blob it put in the store.
    ///
    /// The one that gets away is a model texture every paint overrides: it is folded into
    /// none of them, so walking the paints alone never names it and its pixels outlive the
    /// bike by the whole session. Cheap to leak, since it's the biggest sheets — `plastics`
    /// on a bike whose paints all replace it — that leak.
    #[test]
    fn eviction_frees_the_models_own_textures_too() {
        let model = super::BikeModel {
            nodes: Vec::new(),
            paints: vec![super::BikePaint {
                name: "Red".into(),
                path: None,
                // Its own `plastics`, so the model's never reaches it.
                textures: vec![tex("plastics", "t-paint"), tex("wheel", "t-shared")],
                changes_preview: true,
            }],
            base: vec![tex("plastics", "t-own"), tex("wheel", "t-shared")],
            tyres: None,
            assembled: true,
            rig: None,
        };
        let tokens = model.tokens();
        assert!(tokens.contains(&"t-own".to_string()), "the overridden one is still released");
        assert!(tokens.contains(&"t-paint".to_string()));
        // Named twice over — the paints borrowed it — and that's fine: `release` removes by
        // key, so the second pass over a token is a no-op rather than a double free.
        assert_eq!(tokens.iter().filter(|t| *t == "t-shared").count(), 2);
    }

    /// A paint installed loose beside a bike has to come back with the file it lives in —
    /// that path is the only thing the viewer can watch, and without it a re-saved livery
    /// goes unnoticed until the dialog is closed and re-opened.
    #[test]
    fn an_installed_paint_names_the_file_it_came_from() {
        let root = std::env::temp_dir().join(format!("frost-installed-paints-{}", std::process::id()));
        let paints = root.join("KTM 450").join("paints");
        std::fs::create_dir_all(&paints).expect("make the bike's paints folder");
        std::fs::write(paints.join("Frost.pnt"), b"not really a paint").expect("write a paint");
        // Whatever else is parked in there is not a paint and must not be offered as one.
        std::fs::write(paints.join("notes.txt"), b"x").expect("write the noise");

        let found = super::installed_paints(&root.join("KTM 450"));
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(found.len(), 1, "only the .pnt counts");
        let (name, path, bytes) = &found[0];
        assert_eq!(name, "Frost.pnt");
        assert_eq!(std::path::Path::new(path), paints.join("Frost.pnt"));
        assert_eq!(bytes, b"not really a paint");
    }

    /// A paint re-saved inside one timestamp tick still misses the cache.
    ///
    /// The viewer's live reload re-decodes a file the moment it changes, which can be twice
    /// in the same second — and a filesystem with coarse timestamps (FAT32 rounds to two)
    /// would hand back the previous decode, i.e. the painter's *last* attempt. The mtime is
    /// pinned here so the size is the only thing left to tell the two apart.
    #[test]
    fn a_paint_resaved_within_a_timestamp_tick_is_not_served_stale() {
        let dir = std::env::temp_dir().join(format!("frost-cache-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("make the folder");
        let paint = dir.join("Frost.pnt");
        let pinned = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let stamp = |bytes: &[u8]| {
            std::fs::write(&paint, bytes).expect("save the paint");
            let f = std::fs::File::options().write(true).open(&paint).expect("reopen");
            f.set_times(std::fs::FileTimes::new().set_modified(pinned))
                .expect("pin the mtime");
            super::bike_cache_key(&paint.to_string_lossy())
        };

        let first = stamp(b"the first attempt");
        let second = stamp(b"the second attempt, recompressed");
        let identical = stamp(b"the second attempt, recompressed");
        let _ = std::fs::remove_dir_all(&dir);

        assert_ne!(first, second, "a re-saved paint must miss its cached decode");
        assert_eq!(identical, second, "and an unchanged one must still hit it");
    }

    /// The same, reached through the `.pkz` beside the folder — how a packaged bike is
    /// installed, and the path a painter's own liveries actually sit next to.
    #[test]
    fn paints_are_found_beside_a_packaged_bike_too() {
        let root = std::env::temp_dir().join(format!("frost-packaged-paints-{}", std::process::id()));
        let paints = root.join("KTM 450").join("paints");
        std::fs::create_dir_all(&paints).expect("make the bike's paints folder");
        std::fs::write(paints.join("Frost.pnt"), b"paint").expect("write a paint");

        // The source the viewer is given is the archive, not the folder next to it.
        let found = super::installed_paints(&root.join("KTM 450.pkz"));
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(found.len(), 1);
        assert_eq!(std::path::Path::new(&found[0].1), paints.join("Frost.pnt"));
    }

    fn sub(name: &str, texture: Option<&str>) -> crate::edf::Submesh {
        crate::edf::Submesh {
            name: name.into(),
            tri_start: 0,
            tri_count: 1,
            texture: texture.map(str::to_string),
            uv_tile: None,
            mat: None,
        }
    }

    fn node(name: &str, subs: Vec<crate::edf::Submesh>) -> crate::edf::EdfNode {
        crate::edf::EdfNode {
            name: name.into(),
            positions: vec![0.0; 3],
            uvs: Vec::new(),
            normals: Vec::new(),
            indices: vec![0, 0, 0],
            submeshes: subs,
            texture: None,
            placed: true,
            materials: Vec::new(),
        }
    }

    /// The rear wheel arrives with the chain in it, and the chain is a template strip the
    /// game bends — a metre-long bar if it's drawn where it sits. Everything else on the
    /// wheel has to survive.
    #[test]
    fn the_chain_comes_off_the_rear_wheel() {
        let mut nodes = vec![
            node("fwheel", vec![sub("thefwheel", Some("wheel")), sub("thefwheel", Some("fgeomax"))]),
            node(
                "rwheela",
                vec![
                    sub("thechain", Some("chain")),
                    sub("therwheela", Some("wheel")),
                    sub("therwheela", Some("sprocket")),
                    sub("therwheela", Some("rgeomax")),
                ],
            ),
        ];
        super::drop_chain(&mut nodes);
        assert_eq!(nodes.len(), 2, "both wheels stay");
        assert_eq!(nodes[0].submeshes.len(), 2, "the front wheel is untouched");
        let rear: Vec<&str> =
            nodes[1].submeshes.iter().filter_map(|s| s.texture.as_deref()).collect();
        assert_eq!(rear, ["wheel", "sprocket", "rgeomax"], "rim, sprocket and tyre stay");
    }

    /// The bug the first cut of this had: the submesh went, its vertices stayed, and the
    /// chain's 0.7 m of template still counted towards the bounds everything else is
    /// measured against — where the viewer centres the bike, where `SideBySide` stands it.
    #[test]
    fn the_chains_vertices_go_with_it() {
        let mut n = node(
            "rwheela",
            vec![sub("therwheela", Some("wheel")), sub("thechain", Some("chain"))],
        );
        // Two triangles: the wheel's on the axle, the chain's a long way above it.
        n.positions = vec![0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.7, 0.0];
        n.uvs = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.5, 0.5];
        n.indices = vec![0, 1, 2, 1, 2, 3];
        n.submeshes[0].tri_start = 0;
        n.submeshes[1].tri_start = 1;
        let mut nodes = vec![n];

        super::drop_chain(&mut nodes);
        let n = &nodes[0];
        assert_eq!(n.submeshes.len(), 1);
        assert_eq!(n.submeshes[0].texture.as_deref(), Some("wheel"));
        assert_eq!(n.submeshes[0].tri_start, 0, "the survivor is renumbered from zero");
        assert_eq!(n.submeshes[0].tri_count, 1);
        assert_eq!(n.indices, vec![0, 1, 2], "vertices remapped onto what's left");
        assert_eq!(n.positions.len(), 9, "the chain's lone vertex is gone");
        assert_eq!(n.uvs.len(), 6, "uvs are compacted alongside");
        let top = n.positions.chunks_exact(3).map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!(top < 0.2, "nothing left standing 0.7 m up: {top}");
    }

    /// A node with nothing but chain in it has to go entirely: left with no groups, the
    /// frontend draws the whole node on one texture rather than nothing at all.
    #[test]
    fn a_node_that_is_only_chain_is_dropped() {
        let mut nodes = vec![
            node("chain", vec![sub("thechain", Some("CHAIN"))]),
            // No submesh table at all — a whole-node binding, and not ours to judge.
            node("rwheela", Vec::new()),
        ];
        super::drop_chain(&mut nodes);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "rwheela");
    }

    /// The bike source is `<mods>/bikes/<Bike>`; the tyres sit beside `bikes`, not inside it.
    #[test]
    fn tyres_sit_beside_the_bikes_folder() {
        let dir = super::tyres_dir_for(Path::new("/games/mods/bikes/MX1OEM_2023_Honda_CRF450R"));
        assert_eq!(dir.as_deref(), Some(Path::new("/games/mods/tyres")));
        let packed = super::tyres_dir_for(Path::new("/games/mods/bikes/Some_Bike.pkz"));
        assert_eq!(packed.as_deref(), Some(Path::new("/games/mods/tyres")));
    }

    fn tyres_tmp(tag: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("frost-tyres-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Lay down a minimal but real tyres mod under `<root>/<name>`.
    fn write_tyres_mod(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, body) in [
            ("gfx.cfg", "front_wheel\n{\n\tmodel\n\t{\n\t\tfile = fwheel.hrc\n\t}\n}\n"),
            ("fwheel.hrc", "level0\n{\n\tscene = model.edf\n}\n"),
            ("model.edf", "EDF\0"),
            // Beside them and never drawn — must not be read.
            ("OEM_MXf_is80100-21.tyre", "params"),
            ("preview.tga", "pixels"),
        ] {
            std::fs::write(dir.join(file), body).unwrap();
        }
    }

    fn tyre_file_names(set: &super::TyreSet) -> Vec<&str> {
        let mut names: Vec<&str> = set.files.iter().map(|(n, _)| n.as_str()).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn tyre_files_come_from_the_mod_the_bike_names() {
        let root = tyres_tmp("loose");
        write_tyres_mod(&root, "oem_mx");

        let set = super::gather_tyre_files(&root, b"tyres = oem_mx\n", None).expect("found");
        assert_eq!(set.name, "oem_mx");
        assert_eq!(tyre_file_names(&set), ["fwheel.hrc", "gfx.cfg", "model.edf"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole point of the picker: a bike names one pack, and picking another has to
    /// beat it — without anything on disk being renamed.
    #[test]
    fn a_picked_pack_beats_the_one_the_bike_names() {
        let root = tyres_tmp("pick");
        write_tyres_mod(&root, "oem_mx");
        write_tyres_mod(&root, "p_mx");

        let own = super::gather_tyre_files(&root, b"tyres = oem_mx\n", None).unwrap();
        assert_eq!(own.name, "oem_mx");
        let picked = super::gather_tyre_files(&root, b"tyres = oem_mx\n", Some("p_mx")).unwrap();
        assert_eq!(picked.name, "p_mx", "the pick wins");
        // Blank is "no pick", not "a pack called nothing".
        let blank = super::gather_tyre_files(&root, b"tyres = oem_mx\n", Some("  ")).unwrap();
        assert_eq!(blank.name, "oem_mx");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A pick that names nothing installed must not cost the bike its wheels — it falls back
    /// to the pack the bike itself names.
    #[test]
    fn a_pick_that_isnt_installed_falls_back_to_the_bikes_own() {
        let root = tyres_tmp("fallback");
        write_tyres_mod(&root, "oem_mx");

        for pick in ["uninstalled_pack", "../bikes", "sub/dir"] {
            let set = super::gather_tyre_files(&root, b"tyres = oem_mx\n", Some(pick))
                .unwrap_or_else(|| panic!("still wheels for pick {pick:?}"));
            assert_eq!(set.name, "oem_mx", "pick {pick:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Every way a bike can end up with no wheels. None of them is an error: that is the
    /// bike the viewer drew before wheels existed.
    #[test]
    fn no_tyres_mod_means_no_wheels_and_no_fuss() {
        let root = tyres_tmp("empty");
        let none = |gfx: &[u8], pick: Option<&str>| {
            super::gather_tyre_files(&root, gfx, pick).is_none()
        };
        assert!(none(b"tyres = oem_mx\n", None), "not installed");
        assert!(none(b"chassis\n{\n}\n", None), "no tyres line");
        // The name is read out of a mod's own file, so it never gets to walk out of `tyres/`.
        assert!(none(b"tyres = ../bikes\n", None), "traversal");
        // A pick can't rescue a bike that names nothing installed either.
        assert!(none(b"chassis\n{\n}\n", Some("p_mx")), "pick, but nothing installed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Every base texture a bike decodes, with its source size and what it cost.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> cargo test bike_texture_costs -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_texture_costs() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");

        println!("\n  source            decode    src px      -> stored");
        let mut total = std::time::Duration::ZERO;
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if let Some(stem) = bn.strip_suffix(".tga") {
                let t = std::time::Instant::now();
                let tex = super::paint::decode_image(stem, data);
                let d = t.elapsed();
                total += d;
                if let Some(tex) = tex {
                    println!("  {stem:<16}{d:>9.2?}  {:>6}KB  -> {}x{}",
                             data.len() / 1024, tex.width, tex.height);
                }
            } else if bn.ends_with(".edf") {
                let t = std::time::Instant::now();
                let texs = super::paint::extract_edf_textures(data);
                let d = t.elapsed();
                total += d;
                println!("  {bn:<16}{d:>9.2?}  {:>6}KB  -> {} embedded texture(s)",
                         data.len() / 1024, texs.len());
                for tex in &texs {
                    println!("      {:<12}            -> {}x{}", tex.name, tex.width, tex.height);
                }
            }
        }
        println!("\n  base textures total {total:.2?}");

        // The other half of the parse phase: the geometry in the same files.
        let mut mesh = std::time::Duration::ZERO;
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if !bn.ends_with(".edf") {
                continue;
            }
            let t = std::time::Instant::now();
            let nodes = super::edf::parse(data);
            let d = t.elapsed();
            mesh += d;
            println!("  parse {bn:<20}{d:>9.2?}  -> {} node(s)", nodes.len());
        }
        println!("  mesh parse total    {mesh:.2?}\n");
    }

    /// Where a bike view's time goes, uncached.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> cargo test bike_load_timing -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_load_timing() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("\n  {path}  ({:.1} MB)", size as f64 / 1e6);

        // The archive read, split out from everything downstream of it.
        let t = std::time::Instant::now();
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");
        let read = t.elapsed();
        let bytes: usize = files.iter().map(|(_, d)| d.len()).sum();
        drop(files);

        // Cold: the cache is what a second open gets, and it is not what anyone complains about.
        let t = std::time::Instant::now();
        let m = super::load_bike_model_blocking(path.clone(), None).expect("load bike");
        let cold = t.elapsed();
        println!("  read archive         {read:>9.2?}  ({:.1} MB inflated)", bytes as f64 / 1e6);

        let t = std::time::Instant::now();
        let _ = super::load_bike_model_blocking(path, None).expect("load bike");
        let warm = t.elapsed();

        let t = std::time::Instant::now();
        let json = serde_json::to_string(&m.nodes).unwrap();
        let encode = t.elapsed();

        let sheets: usize = m.paints.iter().map(|p| p.textures.len()).sum();
        println!("  load, cold           {cold:>9.2?}");
        println!("  load, cached         {warm:>9.2?}");
        println!("  mesh -> JSON         {encode:>9.2?}  ({:.1} MB of text to the webview)",
                 json.len() as f64 / 1e6);
        println!("  {} paint(s), {sheets} sheet(s) — pixels stay in the texture store\n",
                 m.paints.len());
    }

    /// What the mesh costs to hand the webview.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> cargo test bike_mesh_payload -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_mesh_payload() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let m = super::load_bike_model_blocking(path, None).expect("load bike");

        let verts: usize = m.nodes.iter().map(|n| n.positions.len() / 3).sum();
        let tris: usize = m.nodes.iter().map(|n| n.indices.len() / 3).sum();
        let floats: usize = m
            .nodes
            .iter()
            .map(|n| n.positions.len() + n.uvs.len() + n.normals.len())
            .sum();
        let ints: usize = m.nodes.iter().map(|n| n.indices.len()).sum();

        let t = std::time::Instant::now();
        let json = serde_json::to_string(&m.nodes).unwrap();
        let encode = t.elapsed();

        // What the same numbers weigh as raw little-endian, which is what a binary channel
        // would carry and what the webview can adopt without parsing.
        let binary = floats * 4 + ints * 4;

        println!("\n  {} nodes, {verts} vertices, {tris} triangles", m.nodes.len());
        println!("  JSON   {:>9.1} MB  encoded in {encode:.2?}", json.len() as f64 / 1e6);
        println!("  binary {:>9.1} MB", binary as f64 / 1e6);
        println!("  ratio  {:>9.1}x\n", json.len() as f64 / binary as f64);
    }

    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_model_from_pkz() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let m = super::load_bike_model_blocking(path, std::env::var("MXB_TYRES").ok()).expect("load bike");
        for n in &m.nodes {
            // Where the part ended up. Printed because placement is the half of this that a
            // texture listing can't show: a part bound to the right sheet and hung in the
            // wrong place reads as correct here otherwise.
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for v in n.positions.chunks_exact(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            eprintln!(
                "node '{}' placed={} x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                n.name, n.placed, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2],
            );
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
        let mut own: Vec<&str> = m.base.iter().map(|t| t.name.as_str()).collect();
        own.sort_unstable();
        eprintln!("the model's own textures: {}", own.join(", "));
        // Every paint is offered, one is shown: the store must hold the list cheaply, and a
        // bike's own sheets must still be there once all of its paints are in.
        let base_bytes: u64 = m.base.iter().map(|t| t.width as u64 * t.height as u64 * 4).sum();
        eprintln!(
            "texture store: {:.1} MB resident, {:.1} MB of it the model's own sheets",
            crate::texstore::resident_bytes() as f64 / (1024.0 * 1024.0),
            base_bytes as f64 / (1024.0 * 1024.0)
        );
        for t in &m.base {
            eprintln!("  base '{}' {}x{}", t.name, t.width, t.height);
        }
        for t in m.base.iter().chain(&m.paints[0].textures) {
            let px = crate::texstore::get(&t.token).expect("token resolves");
            assert_eq!(px.len() as u32, t.width * t.height * 4, "'{}' at its stated size", t.name);
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
        // The Designer's stock underlay reads exactly this list, and it is wanted most where no
        // `.pnt` can answer: an OEM bike's stock paint replaces the wheels and the chain, so its
        // `plastics` is only ever embedded in the mesh. Not asserted by that name — a mod bike
        // calls its sheets whatever it likes, and a bike whose paints supply everything has a
        // shorter list than this one — only that the list survived being folded into the paints
        // above, which is the way it would be lost.
        assert!(!m.base.is_empty(), "the model's own textures outlive the fold into the paints");
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

        // What the mesh itself draws each piece from — the reading the binder goes by, and
        // the one worth eyeballing when a helmet's goggles come out wearing the shell.
        if let Some((_, d)) = files.iter().find(|(n, _)| super::is_visible_gear_mesh(n)) {
            // The slots as the loader counts them: what the mesh embeds, plus the sheets it
            // leaves to a `.pnt`, in the order the model names them.
            let declared = super::paint_texture_names(
                files
                    .iter()
                    .filter(|(n, _)| {
                        super::gear_folder_paint_name(n, "paints").is_some()
                            || super::gear_folder_paint_name(n, "goggles").is_some()
                    })
                    .map(|(_, d)| d.as_slice()),
            );
            let colors = super::edf::declared_colors(d, &declared);
            eprintln!("mesh colour textures: {colors:?}");
            // Per piece, the texture the model was drawn against. This is the evidence the
            // binder decides on, so when a piece ends up wearing the wrong side it says
            // whether the reading was wrong or the choice made from it was.
            for n in &super::edf::parse(d) {
                eprintln!("mats    {:<28} {:?}", n.name, n.materials);
                for sm in &n.submeshes {
                    let emb = sm
                        .mat
                        .and_then(|m| n.materials.get(m as usize).copied().flatten())
                        .and_then(|slot| colors.get(slot))
                        .map(String::as_str);
                    // The triangle count tells the pieces apart when their names don't: a
                    // helmet shell dwarfs its lens, and both dwarf a tear-off film.
                    eprintln!(
                        "drawn   {:<28} mat={:?} tris={:<6} -> {}",
                        format!("{}/{}", n.name, sm.name),
                        sm.mat,
                        sm.tri_count,
                        emb.unwrap_or("(none)"),
                    );
                }
            }
        }
        // Where each mesh sits in its own frame, against the bounds the file states for
        // itself. Protection is authored in the rider's own space, so these say whether a
        // piece is a chest-wide vest or a thin chain — the difference a one-size fit erases —
        // and disagreeing with the header is how a placement that ran twice shows up.
        let mut scenes: Vec<&Vec<u8>> =
            super::gear_scenes(&files).iter().filter_map(|s| super::gear_file(&files, s)).collect();
        if scenes.is_empty() {
            scenes.extend(files.iter().find(|(n, _)| super::is_visible_gear_mesh(n)).map(|(_, d)| d));
        }
        for d in scenes {
            let mut nodes = super::edf::parse_gear(d);
            super::edf::to_right_handed(&mut nodes);
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for n in &nodes {
                let (mut nlo, mut nhi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
                for v in n.positions.chunks_exact(3) {
                    for k in 0..3 {
                        nlo[k] = nlo[k].min(v[k]);
                        nhi[k] = nhi[k].max(v[k]);
                    }
                }
                eprintln!(
                    "bounds  {:<16} x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                    n.name, nlo[0], nhi[0], nlo[1], nhi[1], nlo[2], nhi[2],
                );
                for k in 0..3 {
                    lo[k] = lo[k].min(nlo[k]);
                    hi[k] = hi[k].max(nhi[k]);
                }
            }
            let Some((hlo, hhi)) = super::edf::header_aabb(d) else { continue };
            // The header is written in authored space, so it takes the same X flip the
            // vertices got — and the flip swaps which end is the minimum.
            let (hlo, hhi) = ([-hhi[0], hlo[1], hlo[2]], [-hlo[0], hhi[1], hhi[2]]);
            eprintln!(
                "header                   x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                hlo[0], hhi[0], hlo[1], hhi[1], hlo[2], hhi[2],
            );
            // Only the highest LOD is kept and a dropped one can reach a millimetre further,
            // so this is a "same place, same size" check, not an equality.
            for k in 0..3 {
                let slack = 0.02 + 0.1 * (hhi[k] - hlo[k]);
                assert!(
                    (lo[k] - hlo[k]).abs() <= slack && (hi[k] - hhi[k]).abs() <= slack,
                    "axis {k}: mesh [{:.3},{:.3}] is not where the file says it is \
                     ([{:.3},{:.3}]) — placement",
                    lo[k], hi[k], hlo[k], hhi[k],
                );
            }
        }
        // The names each side's first paint supplies — where each can actually land.
        let supplied = |folder: &str| -> std::collections::HashSet<String> {
            files
                .iter()
                .find(|(n, _)| super::gear_folder_paint_name(n, folder).is_some())
                .and_then(|(_, d)| super::paint::decode_any(d).ok())
                .map(|p| p.iter().map(|t| t.name.to_ascii_lowercase()).collect())
                .unwrap_or_default()
        };

        // The slot this item is worn in. Gear behaves differently per slot — protection has
        // no goggle side, and is the slot where a paintless mod is the norm rather than the
        // exception — so the run is worth naming honestly.
        let slot = std::env::var("MXB_REAL_GEAR_PART").unwrap_or_else(|_| "helmet".into());
        let part = super::load_gear_model_blocking(path.clone(), slot.clone(), None, None, false, false, Vec::new())
            .expect("load gear");
        // MXB_DUMP_OBJ=<file> writes the loaded geometry in viewer-input space, so a
        // silhouette can be drawn outside the test — the only way to settle which way a
        // gear frame points without the game in front of you.
        if let Ok(out) = std::env::var("MXB_DUMP_OBJ") {
            let mut s = String::new();
            for n in &part.nodes {
                s.push_str(&format!("o {}\n", n.name));
                for v in n.positions.chunks_exact(3) {
                    s.push_str(&format!("v {} {} {}\n", v[0], v[1], v[2]));
                }
            }
            std::fs::write(&out, s).expect("write obj");
            eprintln!("wrote {out}");
        }
        let have: std::collections::HashSet<String> =
            part.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        // An item with no paint and nothing baked into its mesh has no look to wear — the
        // Minecraft pickaxe on mxb-mods ships exactly that. Bare grey is the honest answer
        // there, and the only case where an unbound piece isn't a bug.
        let has_look = !paints.is_empty() || !have.is_empty();
        // Which texture each piece ended up wearing, by node so a goggle node that carries
        // no submeshes of its own shows up too.
        let mut worn: Vec<(String, String)> = Vec::new();
        let mut bare = 0usize;
        for n in &part.nodes {
            let pieces: Vec<(String, &Option<String>)> = if n.submeshes.is_empty() {
                vec![(n.name.clone(), &n.texture)]
            } else {
                n.submeshes
                    .iter()
                    .map(|s| (format!("{}/{}", n.name, s.name), &s.texture))
                    .collect()
            };
            for (label, tex) in pieces {
                match tex {
                    Some(t) => {
                        eprintln!("worn    {label:<28} -> {t}");
                        worn.push((label, t.clone()));
                    }
                    None => {
                        eprintln!("worn    {label:<28} -> (bare)");
                        bare += 1;
                    }
                }
            }
        }
        assert!(
            !has_look || bare == 0,
            "{bare} piece(s) left bare though the item ships a look",
        );
        for (_, t) in &worn {
            assert!(have.contains(&t.to_ascii_lowercase()), "'{t}' is shipped");
        }
        let (shell_names, goggle_names) = (supplied("paints"), supplied("goggles"));
        eprintln!("shell paint supplies {shell_names:?}, goggle paint {goggle_names:?}");
        // A helmet that ships goggle paints has somewhere for them to land — asked by name,
        // not by what a piece is called, since that's exactly what a mesh needn't spell out.
        // Skipped where the two sides share an atlas: then "whose texture is this" has no
        // answer to check.
        if !goggle_names.is_empty() && goggle_names.is_disjoint(&shell_names) {
            assert!(
                worn.iter().any(|(_, t)| goggle_names.contains(&t.to_ascii_lowercase())),
                "a piece wears the goggle paint: {worn:?}",
            );
            assert!(
                worn.iter().any(|(_, t)| !goggle_names.contains(&t.to_ascii_lowercase())),
                "the shell keeps its own texture: {worn:?}",
            );
        }

        // Stock: the same mesh wearing the textures embedded in it, not a packed `.pnt`.
        let listed = super::gear_paints_at(std::path::Path::new(&path)).expect("list paints");
        eprintln!("has_stock={} goggles={}", listed.has_stock, listed.has_stock_goggles);
        if !listed.has_stock {
            eprintln!("this piece embeds no textures — no stock entry to check");
            return;
        }
        let stock =
            super::load_gear_model_blocking(
                path,
                slot,
                None,
                None,
                true,
                listed.has_stock_goggles,
                Vec::new(),
            )
            .expect("load stock gear");
        let embedded: std::collections::HashSet<String> =
            stock.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        assert!(
            !paints.iter().any(|p| embedded.contains(&p.to_ascii_lowercase())),
            "a stock preview decodes no packed paint",
        );
        let mut bound = 0;
        for n in &stock.nodes {
            // A one-piece item has no submesh table at all and wears its texture on the node
            // — `bind_gear_submeshes` says so explicitly. Counting only submeshes read that
            // as "nothing was bound" on exactly the meshes worth checking.
            let pieces: Vec<(&str, Option<&String>)> = if n.submeshes.is_empty() {
                vec![(n.name.as_str(), n.texture.as_ref())]
            } else {
                n.submeshes.iter().map(|s| (s.name.as_str(), s.texture.as_ref())).collect()
            };
            for (name, tex) in pieces {
                let t = tex.expect("stock piece bound to a texture");
                eprintln!("stock piece {name:<10} -> {t}");
                assert!(embedded.contains(&t.to_ascii_lowercase()), "'{t}' is embedded in the mesh");
                bound += 1;
            }
        }
        assert!(bound > 0, "stock bound at least one piece");

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
                Vec::new(),
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

    /// The picker's paint list against a real install: whatever any single source can
    /// supply for a model, the merged list offers.
    ///
    /// `MXB_MODS=~/Documents/PiBoSo/MX\ Bikes MXB_GEAR_PART=boots \
    ///   MXB_GEAR_MODEL='Fox Instinct 2.0 by Aeffertz' \
    ///   cargo test gear_paints_merge_every_source -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gear_paints_merge_every_source() {
        let Ok(mods) = std::env::var("MXB_MODS") else {
            eprintln!("set MXB_MODS to run");
            return;
        };
        let part = std::env::var("MXB_GEAR_PART").unwrap_or_else(|_| "boots".into());
        let Ok(model) = std::env::var("MXB_GEAR_MODEL") else {
            eprintln!("set MXB_GEAR_MODEL to run");
            return;
        };
        let cfg = crate::config::AppConfig { mods_path: mods.clone(), ..Default::default() };
        let spec = super::GEAR.iter().find(|g| g.part == part).expect("a gear slot");

        let merged = super::gear_paints_for(&cfg, spec, &model);
        eprintln!("merged paints ({}): {:?}", merged.paints.len(), merged.paints);
        eprintln!("merged goggles ({}): {:?}", merged.goggles.len(), merged.goggles);

        let has = |set: &[String], want: &str| set.iter().any(|n| n.eq_ignore_ascii_case(want));

        // Every source, asked on its own. Each one's paints have to survive the merge —
        // taking the first source is exactly what dropped the others.
        let rider = std::path::Path::new(&mods).join("mods").join("rider");
        let stem = model.trim_end_matches(".pkz");
        let mut sources = 0;
        for src in super::gear_sources(&rider, spec, stem) {
            if !src.exists() {
                continue;
            }
            sources += 1;
            let alone = super::gear_paints_at(&src).expect("list one source");
            eprintln!("  {src:?}: {:?}", alone.paints);
            for p in &alone.paints {
                assert!(has(&merged.paints, p), "'{p}' from {src:?} survives the merge");
            }
            for g in &alone.goggles {
                assert!(has(&merged.goggles, g), "goggle '{g}' from {src:?} survives the merge");
            }
        }

        // The game's own copy, which is all a stock name has and which nothing listed before.
        if let Some(pkz) = super::resolve_game_pkz(&cfg, "rider.pkz") {
            let folder = format!("rider/{}/{}", spec.pkz_kind, stem);
            let stock = super::pkz_paint_names(&pkz, &folder, "paints");
            eprintln!("  rider.pkz {folder}: {stock:?}");
            if !stock.is_empty() {
                sources += 1;
            }
            for p in &stock {
                assert!(has(&merged.paints, p), "stock '{p}' survives the merge");
            }
        }
        assert!(sources > 0, "'{model}' resolved to at least one source");
        // Nothing is offered twice — a paint pack installed beside the mod it was made for
        // ships the same names, and the same look twice is not two choices.
        let mut seen: Vec<String> = merged.paints.iter().map(|s| s.to_lowercase()).collect();
        let total = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), total, "no paint is offered twice");
    }

    #[test]
    fn body_slot_reads_the_name_not_the_index() {
        // The `w_` planes are decals the game composites a name and number onto.
        for decal in ["w_name", "w_number", "w_plate", "W_Name"] {
            assert_eq!(super::body_slot(Some(decal)), "hide");
        }
        // Skin, whatever the model calls it, must not wear the kit.
        for skin in ["face_parts", "face", "Face_Parts", "rider_face"] {
            assert_eq!(super::body_slot(Some(skin)), "face");
        }
        // Everything else keeps its own name, so a paint replaces it by name and a piece no
        // paint covers falls back to the model's own texture.
        for (name, slot) in [
            ("rider", "rider"),
            ("gloves", "gloves"),
            ("rider_sm", "rider_sm"),
            ("glovessm", "glovessm"),
            ("Braces", "braces"),
        ] {
            assert_eq!(super::body_slot(Some(name)), slot);
        }
        // A material that names no texture still has to render as something.
        assert_eq!(super::body_slot(None), "rider");
    }

    /// A material id is LOCAL to the node that owns it, so the same id must resolve to
    /// different textures in different parts of one mesh.
    ///
    /// This is the invariant the per-part material-table fix established, and the one the
    /// rider binder lost: it was still resolving ids through a whole-model reading, which
    /// is what put one part's texture on another's geometry.
    #[test]
    fn a_body_part_resolves_its_material_through_its_own_table() {
        fn tex(name: &str) -> super::edf::EmbeddedTexture {
            super::edf::EmbeddedTexture {
                name: name.into(),
                width: 4,
                height: 4,
                data_off: 0,
                data_len: 0,
            }
        }
        fn sm(mat: u32) -> super::edf::Submesh {
            super::edf::Submesh {
                name: "range".into(),
                tri_start: 0,
                tri_count: 1,
                texture: None,
                uv_tile: None,
                mat: Some(mat),
            }
        }
        fn node(materials: Vec<Option<usize>>, mats: &[u32]) -> super::edf::EdfNode {
            super::edf::EdfNode {
                name: "part".into(),
                positions: Vec::new(),
                uvs: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                submeshes: mats.iter().map(|m| sm(*m)).collect(),
                texture: None,
                placed: false,
                materials,
            }
        }

        let colors = [tex("rider"), tex("gloves"), tex("w_number")];
        // Both parts draw on local id 0 — and mean different textures by it.
        let mut nodes = vec![
            node(vec![Some(0)], &[0]),           // body  -> rider
            node(vec![Some(1)], &[0]),           // hands -> gloves
            node(vec![Some(2), None], &[0, 1]),  // plate -> hidden decal, then untextured
        ];
        super::bind_body_to_colors(&mut nodes, &colors);

        assert_eq!(nodes[0].submeshes[0].texture.as_deref(), Some("rider"));
        assert_eq!(
            nodes[1].submeshes[0].texture.as_deref(),
            Some("gloves"),
            "id 0 read through the first node's table would smear the suit onto the hands"
        );
        assert_eq!(nodes[2].submeshes[0].texture.as_deref(), Some("hide"));
        // An untextured material, and an id past the end of the table, both still render.
        assert_eq!(nodes[2].submeshes[1].texture.as_deref(), Some("rider"));
    }

    /// Investigation aid: a rider's rig as JSON, in the frame the viewer draws it in.
    ///
    /// The two turns `body_rig` puts a rig through are what make the difference between this
    /// and `edf::tests::rig_dump`, and they are what the front end's ready-made moves are
    /// stated against — so this is the shape to check those against.
    ///
    /// `MXB_EDF_FILE=…/rider.edf cargo test rig_json -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rig_json() {
        let Ok(path) = std::env::var("MXB_EDF_FILE") else {
            eprintln!("set MXB_EDF_FILE to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read edf");
        let mut rig = super::edf::parse_skeleton(&bytes);
        super::edf::transform_skeleton(&mut rig, [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for b in rig.iter() {
            let o = b.origin();
            for a in 0..3 {
                lo[a] = lo[a].min(o[a]);
                hi[a] = hi[a].max(o[a]);
            }
        }
        if super::body_is_z_up([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) {
            super::edf::transform_skeleton(&mut rig, super::BODY_STAND_UP);
        }
        println!("{}", serde_json::to_string(&rig).expect("serialise"));
    }

    /// The bug this replaced: material indices count into the model's own texture list, and
    /// no two rider models write that list in the same order.
    ///
    /// `MXB_REAL_BODY=<rider.edf>[,<rider.edf>…] cargo test rider_body_binding -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rider_body_binding_from_env() {
        let Ok(paths) = std::env::var("MXB_REAL_BODY") else {
            eprintln!("set MXB_REAL_BODY to run");
            return;
        };
        for path in paths.split(',').filter(|p| !p.is_empty()) {
            let bytes = std::fs::read(path).expect("read rider.edf");
            let order: Vec<String> =
                crate::edf::color_textures(&bytes).iter().map(|t| t.name.clone()).collect();
            let mut nodes = crate::edf::parse(&bytes);
            crate::edf::to_right_handed(&mut nodes);
            super::keep_lod0(&mut nodes);
            super::bind_body_submeshes(&mut nodes, &bytes);

            super::stand_body_upright(&mut nodes);

            let bounds = |ns: &[crate::edf::EdfNode], only_face: bool| {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for n in ns {
                    for sm in &n.submeshes {
                        if only_face && sm.texture.as_deref() != Some("face") {
                            continue;
                        }
                        for i in &n.indices[sm.tri_start as usize * 3
                            ..(sm.tri_start + sm.tri_count) as usize * 3]
                        {
                            let v = &n.positions[*i as usize * 3..*i as usize * 3 + 3];
                            for a in 0..3 {
                                lo[a] = lo[a].min(v[a]);
                                hi[a] = hi[a].max(v[a]);
                            }
                        }
                    }
                }
                (lo, hi)
            };
            let (lo, hi) = bounds(&nodes, false);
            let (h, d) = (hi[1] - lo[1], hi[2] - lo[2]);
            eprintln!("  upright bounds x={:.3} y={h:.3} z={d:.3}", hi[0] - lo[0]);

            // A rider is a standing figure, so height is its longest axis. The viewer scales
            // and anchors every piece of gear off this — a body on its side buries the
            // helmet and boots in the torso at a fifth of their size.
            assert!(h > hi[0] - lo[0] && h > d, "the body stands up");
            // And it stands the right way up. Height alone can't tell a rider from one
            // hanging upside down, so check the skin: the head is the highest thing on a
            // rider. Its top, not its bottom — Rider+'s skin texture also covers the bare
            // wrists of its rolled-sleeve variants, which reach well down the body.
            let (_, fhi) = bounds(&nodes, true);
            eprintln!("  skin tops out at {:.3} of {:.3}", fhi[1], hi[1]);

            // Which way does it face? Report the Z centroid of each slot relative to the
            // body's own centre, plus the head alone (the top eighth of the skin, so
            // Rider+'s bare wrists don't drag the number toward the bars).
            let cz = (lo[2] + hi[2]) / 2.0;
            let mut per: std::collections::BTreeMap<String, (f64, f64, usize)> = Default::default();
            for n in &nodes {
                for sm in &n.submeshes {
                    let slot = sm.texture.clone().unwrap_or_default();
                    for i in &n.indices
                        [sm.tri_start as usize * 3..(sm.tri_start + sm.tri_count) as usize * 3]
                    {
                        let v = &n.positions[*i as usize * 3..*i as usize * 3 + 3];
                        let e = per.entry(slot.clone()).or_default();
                        e.0 += (v[2] - cz) as f64;
                        e.2 += 1;
                        if slot == "face" && v[1] > hi[1] - 0.125 * h {
                            let hd = per.entry("face(head)".into()).or_default();
                            hd.0 += (v[2] - cz) as f64;
                            hd.2 += 1;
                        }
                    }
                }
            }
            for (slot, (sum, _, n)) in &per {
                eprintln!("  {slot:>12} z-centroid {:+.4} ({n} verts)", sum / *n as f64);
            }
            let centroid = |slot: &str| per.get(slot).map(|(s, _, n)| s / *n as f64);

            // And it faces the right way. The viewer nudges the helmet and boots forward in
            // +Z, so a rider turned around wears its gear through its own back.
            //
            // The name and number planes are the tell: they go on a rider's back. Where the
            // model has none, fall back to the head, which leans forward over the bars —
            // a weaker signal, so it only decides when the strong one is absent.
            match centroid("hide") {
                Some(back) => assert!(back < 0.0, "the name and number sit on the back ({back:+.4})"),
                None => {
                    let head = centroid("face(head)").expect("a rider has a head");
                    assert!(head > 0.0, "the head leans forward ({head:+.4})");
                }
            }
            assert!(
                fhi[1] > lo[1] + 0.9 * h,
                "the head is at the top (skin tops at {:.3}, body {:.3}..{:.3})",
                fhi[1],
                lo[1],
                hi[1],
            );

            let mut slots: Vec<String> = nodes
                .iter()
                .flat_map(|n| n.submeshes.iter().filter_map(|s| s.texture.clone()))
                .collect();
            slots.sort_unstable();
            slots.dedup();
            eprintln!("{path}\n  blob order: {order:?}\n  bound slots: {slots:?}");

            assert!(!slots.is_empty(), "every body binds its submeshes to something");
            // Whatever the model calls its suit and gloves, the binding must name the
            // textures the model actually carries — never a slot borrowed from another model.
            for s in &slots {
                assert!(
                    s == "hide"
                        || s == "face"
                        || order.iter().any(|t| t.eq_ignore_ascii_case(s)),
                    "'{s}' is a texture this model carries",
                );
            }
            // Skin is its own slot on every rider model shipped so far; catching its loss
            // is what tells us a mesh stopped being read and an index map crept back in.
            assert!(slots.iter().any(|s| s == "face"), "the face binds to bare skin");
        }
    }

    /// The whole rider-body path against a real install: a custom model resolves out of
    /// `mods/rider/riders`, binds, and wears a kit that only the stock profile owns.
    ///
    /// `MXB_MODS=~/Documents/PiBoSo/MX\ Bikes MXB_PROFILE=Rider+ cargo test rider_body_end_to_end -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rider_body_end_to_end() {
        let Ok(mods) = std::env::var("MXB_MODS") else {
            eprintln!("set MXB_MODS to run");
            return;
        };
        let profile = std::env::var("MXB_PROFILE").unwrap_or_else(|_| "Rider+".into());
        let cfg = crate::config::AppConfig { mods_path: mods.clone(), ..Default::default() };
        let base = std::path::Path::new(&mods).join("mods").join("rider");

        // A model nobody can pick is a model nobody can wear. The scan reports what's on
        // disk; the two stock riders live in `rider.pkz` and the picker adds them itself.
        let targets = crate::library::scan_rider_targets(&mods);
        eprintln!("profiles: {:?}", targets.profiles);
        assert!(
            targets.profiles.iter().any(|p| *p == profile)
                || super::STOCK_RIDER_PROFILES.contains(&profile.as_str()),
            "'{profile}' is offered",
        );

        let src = super::rider_body_source(&cfg, &profile).expect("a body source");
        eprintln!("{profile}: {src:?}");

        {
            let t = std::time::Instant::now();
            let data = src.read(&profile).expect("read mesh");
            eprintln!("  read {} MB in {:?}", data.len() / 1_000_000, t.elapsed());
            let t = std::time::Instant::now();
            let mut n = crate::edf::parse(&data);
            eprintln!("  parse {} nodes in {:?}", n.len(), t.elapsed());
            let t = std::time::Instant::now();
            crate::edf::to_right_handed(&mut n);
            super::keep_lod0(&mut n);
            eprintln!("  handedness + lod0 in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            super::bind_body_submeshes(&mut n, &data);
            eprintln!("  bind in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            super::stand_body_upright(&mut n);
            eprintln!("  stand in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            let texs = super::body_textures(&src, &profile).expect("textures");
            eprintln!(
                "  extract {:?} in {:?}",
                texs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
                t.elapsed(),
            );
        }
        // A profile folder that carries a mesh is a model, and beats the game archive. One
        // that only carries paints — which is what installing a kit under `default_mx`
        // leaves behind — is not, and must still fall through to the stock body.
        let installed = base.join("riders").join(&profile).join("rider.edf").is_file();
        assert_eq!(
            matches!(src, super::BodySource::Loose(_)),
            installed,
            "an installed mesh wins, a paints-only folder doesn't",
        );

        let part = super::load_rider_body(&cfg, &profile, Vec::new()).expect("a body part");
        let slots: std::collections::BTreeSet<&str> = part
            .nodes
            .iter()
            .flat_map(|n| n.submeshes.iter().filter_map(|s| s.texture.as_deref()))
            .collect();
        let texs: std::collections::BTreeSet<String> =
            part.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        eprintln!("  slots={slots:?}\n  textures={texs:?}");
        // With no paint chosen, every slot that draws something is dressed by the model.
        for s in slots.iter().filter(|s| **s != "hide" && **s != "face") {
            assert!(texs.contains(*s), "'{s}' is supplied by the mesh when no paint is");
        }

        // Rider+ ships `paints` empty on purpose — the kit still has to resolve.
        if let Some(kit) = std::env::var("MXB_KIT").ok().filter(|k| !k.is_empty()) {
            let found = super::read_rider_paint_file(&cfg, &base, &profile, "paints", &kit);
            assert!(found.is_some(), "kit '{kit}' resolves for '{profile}'");
            eprintln!("  kit '{kit}' resolved ({} bytes)", found.unwrap().len());
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

    /// Write a bike's raw `.edf` meshes out so the binary layout can be studied directly.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> MXB_EDF_OUT=<dir> \
    ///   cargo test extract_bike_edfs -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn extract_bike_edfs() {
        let (Ok(path), Ok(out)) = (std::env::var("MXB_REAL_PKZ"), std::env::var("MXB_EDF_OUT"))
        else {
            eprintln!("set MXB_REAL_PKZ and MXB_EDF_OUT to run");
            return;
        };
        let stem = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::create_dir_all(&out).expect("create out dir");
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if !bn.ends_with(".edf") {
                continue;
            }
            let dst = std::path::Path::new(&out).join(format!("{stem}__{bn}"));
            std::fs::write(&dst, data).expect("write edf");
            println!("wrote {} ({} bytes)", dst.display(), data.len());
        }
    }

    /// Dump, as one JSON object, everything that decides which texture each part of a
    /// bike wears: the mesh's colour list, what each reading of a material index claims,
    /// which reading the geometry settled on, and what the viewer finally bound.
    ///
    /// `MXB_REAL_PKZ='…/mods/bikes/MX2OEM_2023_Kawasaki_KX250.pkz' \
    ///   cargo test audit_bike_bindings -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn audit_bike_bindings() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let bike = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        // What the viewer renders today.
        let model = super::load_bike_model_blocking(path.clone(), None).expect("load bike");
        // Keyed on the triangle range too: one group name can appear twice in a node,
        // once per material, and those two are exactly the interesting case.
        let bound: std::collections::HashMap<(String, String, u32), Option<String>> = model
            .nodes
            .iter()
            .flat_map(|n| {
                n.submeshes.iter().map(move |s| {
                    (
                        (n.name.to_ascii_lowercase(), s.name.to_ascii_lowercase(), s.tri_start),
                        s.texture.clone(),
                    )
                })
            })
            .collect();

        // The same meshes again, raw, so both readings of every material can be shown
        // side by side with the fit that chose between them.
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");
        let mut out = String::new();
        out.push_str(&format!("{{\"bike\":\"{}\",\"meshes\":[", esc(&bike)));
        let mut first_mesh = true;
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if !bn.ends_with(".edf") {
                continue;
            }
            let nodes = crate::edf::parse_with_levels(data, &[]);
            if nodes.is_empty() {
                continue;
            }
            let color = crate::edf::color_textures(data);
            if color.is_empty() {
                continue; // a shadow mesh carries no colour textures and is never rendered
            }
            let colors: Vec<String> =
                color.iter().map(|t| format!("\"{}\"", esc(&t.name))).collect();
            if !first_mesh {
                out.push(',');
            }
            first_mesh = false;
            out.push_str(&format!(
                "{{\"file\":\"{}\",\"colors\":[{}],\"nodes\":[",
                esc(&bn),
                colors.join(",")
            ));
            for (i, n) in nodes.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // The node's OWN material table: local id -> colour texture.
                let table: Vec<String> = n
                    .materials
                    .iter()
                    .map(|slot| match slot.and_then(|s| color.get(s)) {
                        Some(t) => format!("\"{}\"", esc(&t.name)),
                        None => "null".into(),
                    })
                    .collect();
                out.push_str(&format!(
                    "{{\"node\":\"{}\",\"materials\":[{}],\"submeshes\":[",
                    esc(&n.name),
                    table.join(",")
                ));
                for (j, s) in n.submeshes.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    let key =
                        (n.name.to_ascii_lowercase(), s.name.to_ascii_lowercase(), s.tri_start);
                    let rendered = bound
                        .get(&key)
                        .cloned()
                        .flatten()
                        .map(|t| format!("\"{}\"", esc(&t)))
                        .unwrap_or_else(|| "null".into());
                    out.push_str(&format!(
                        "{{\"group\":\"{}\",\"mat\":{},\"tris\":{},\"rendered\":{}}}",
                        esc(&s.name),
                        s.mat.map(|m| m.to_string()).unwrap_or_else(|| "null".into()),
                        s.tri_count,
                        rendered,
                    ));
                }
                out.push_str("]}");
            }
            out.push_str("]}");
        }
        let paints: Vec<String> = model
            .paints
            .iter()
            .map(|p| {
                let mut t: Vec<String> =
                    p.textures.iter().map(|t| format!("\"{}\"", esc(&t.name))).collect();
                t.sort_unstable();
                format!("{{\"paint\":\"{}\",\"textures\":[{}]}}", esc(&p.name), t.join(","))
            })
            .collect();
        out.push_str(&format!("],\"paints\":[{}]}}", paints.join(",")));
        println!("AUDIT {out}");
    }

    /// The average colour of a stored texture, and how far off neutral it is.
    ///
    /// Neutrality rather than "is it white" on purpose: it holds whichever way round the
    /// channels are stored, so this can't pass or fail for the wrong reason if that ever
    /// moves. A white or grey sheet has its three channels together; a painted one doesn't.
    fn avg_and_spread(tex: &crate::paint::PaintTexture) -> ([u32; 3], u32) {
        let px = crate::texstore::get(&tex.token).expect("token resolves");
        let mut sum = [0u64; 3];
        let mut n = 0u64;
        for p in px.chunks_exact(4) {
            if p[3] < 128 {
                continue; // fully transparent regions are not the look
            }
            for k in 0..3 {
                sum[k] += p[k] as u64;
            }
            n += 1;
        }
        assert!(n > 0, "'{}' has no opaque pixels", tex.name);
        let avg = [(sum[0] / n) as u32, (sum[1] / n) as u32, (sum[2] / n) as u32];
        (avg, avg.iter().max().unwrap() - avg.iter().min().unwrap())
    }

    /// The reported bug, pinned to the archive it came from: the rider tab drew the stock
    /// helmet bronze while the library's "Stock" entry drew it white.
    ///
    /// Both halves matter and they pull in opposite directions — an empty name must reach no
    /// paint at all, while a *stale* name must still reach one, or gear whose paint was
    /// renamed comes out bare grey. A single assertion would let the fix overshoot.
    #[test]
    #[ignore]
    fn an_unnamed_stock_paint_is_the_mesh_not_the_first_pnt() {
        let Ok(pkz) = std::env::var("MXB_RIDER_PKZ") else {
            eprintln!("set MXB_RIDER_PKZ to the game's rider.pkz to run");
            return;
        };
        let pkz = std::path::Path::new(&pkz);
        let folder = "rider/helmets/default";

        // No name → no paint, so the caller falls through to the mesh's own textures.
        assert!(
            super::load_pkz_paint(pkz, folder, "paints", "").is_empty(),
            "an empty slot must not resolve to a paint",
        );
        // A name that misses → still a paint, so a renamed livery shows textured.
        assert!(
            !super::load_pkz_paint(pkz, folder, "paints", "no_such_paint_ea7b").is_empty(),
            "a stale name must still fall back to a paint",
        );

        // And what the two answers actually look like. The mesh's own sheet is the white one
        // the library shows; the paint that used to stand in for it is not.
        let mesh = super::read_pkz_entry(pkz, &format!("{folder}/helmet.edf")).expect("helmet.edf");
        let stock = crate::paint::extract_edf_textures_where(&mesh, |n| n.eq_ignore_ascii_case("helmet"));
        let stock = stock.first().expect("the mesh carries its own 'helmet' sheet");
        let (stock_avg, stock_spread) = avg_and_spread(stock);

        let first = super::load_pkz_paint(pkz, folder, "paints", "black_yellow");
        let first = first.iter().find(|t| t.name.eq_ignore_ascii_case("helmet"));
        let first = first.expect("black_yellow paints 'helmet'");
        let (first_avg, first_spread) = avg_and_spread(first);

        eprintln!("stock mesh sheet  avg={stock_avg:?} spread={stock_spread}");
        eprintln!("black_yellow.pnt  avg={first_avg:?} spread={first_spread}");
        assert!(stock_spread < 12, "the stock helmet is neutral (white/grey), got {stock_avg:?}");
        assert!(
            first_spread > 40,
            "black_yellow is a colour, not a neutral — if this fails the fixture archive \
             changed and the test above proves less than it looks like ({first_avg:?})",
        );
    }

    /// The invariant behind the fix, for whatever gear is on this machine: an empty paint
    /// slot renders exactly what the library's "Stock" entry renders — and where there is no
    /// stock look to render, it still renders *something* rather than going bare.
    ///
    /// Stated as an equality between the two code paths rather than as an expected texture
    /// name, because the point is that they agree, not what they agree on.
    #[test]
    #[ignore]
    fn an_empty_slot_renders_what_the_library_calls_stock() {
        let Ok(path) = std::env::var("MXB_REAL_GEAR") else {
            eprintln!("set MXB_REAL_GEAR to an installed gear folder/.pkz to run");
            return;
        };
        let slot = std::env::var("MXB_REAL_GEAR_PART").unwrap_or_else(|_| "helmet".into());
        let names = |p: super::RiderPart| {
            let mut v: Vec<String> = p.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
            v.sort_unstable();
            v
        };
        let load = |paint: Option<&str>, stock: bool| {
            names(
                super::load_gear_model_blocking(
                    path.clone(),
                    slot.clone(),
                    paint.map(str::to_string),
                    None,
                    stock,
                    false,
                    Vec::new(),
                )
                .expect("load gear"),
            )
        };

        let offers_stock = super::gear_paints_at(std::path::Path::new(&path))
            .expect("read gear paints")
            .has_stock;
        // The rider tab's empty slot, and the library's picker on "Stock".
        let empty = load(Some(""), false);
        eprintln!("has_stock={offers_stock} empty slot -> {empty:?}");
        assert!(!empty.is_empty(), "an empty slot must never render untextured");
        if offers_stock {
            assert_eq!(empty, load(None, true), "empty slot != the library's Stock");
        } else {
            // No stock look to fall back on, so the first-paint fallback still stands —
            // forcing stock here is what would draw the Bell Moto 10 in a near-blank film.
            assert_eq!(empty, load(None, false), "kept the first-paint fallback");
        }
    }
}

#[cfg(test)]
mod no_mesh_tests {
    use super::no_mesh_reason;

    /// An `.edf` long enough to clear the header check — the shape of a mesh that read fine.
    fn a_mesh() -> Vec<u8> {
        let mut b = b"EDF\0".to_vec();
        b.resize(128, 0);
        b
    }

    // A cloud placeholder that was never fetched: the entry is there and empty. This is the
    // only case the old wording was right about, and it keeps it.
    #[test]
    fn a_mesh_that_never_arrived_points_at_cloud_sync() {
        let msg = no_mesh_reason("Bike · Stock", &[("model.edf", b"")]);
        assert!(msg.contains("cloud-synced"), "{msg}");
    }

    // The report this came from: a protected model that runs perfectly in game, on a machine
    // with no cloud sync anywhere near it. Bytes arrived, they just weren't a mesh — sending
    // that player to their OneDrive settings is the one thing the message must not do.
    #[test]
    fn a_mesh_that_isnt_a_mesh_is_not_blamed_on_cloud_sync() {
        let msg = no_mesh_reason("Bike · MySwap", &[("model.edf", &[0xfe, 0x9c, 0xa5, 0x6a])]);
        assert!(!msg.contains("cloud"), "{msg}");
        assert!(msg.contains("didn't decode"), "{msg}");
    }

    // A real `.edf` the parser walked and found nothing in. Nothing the player can fix, so
    // the message says where the fault is rather than sending them looking.
    #[test]
    fn a_real_mesh_that_parsed_to_nothing_says_so() {
        let mesh = a_mesh();
        let msg = no_mesh_reason("Bike · MySwap", &[("model.edf", &mesh)]);
        assert!(msg.contains("no parts came out of it"), "{msg}");
    }

    // One good mesh among several is still a bike that should have drawn — the parser gap is
    // the fault worth naming, not the empty sibling beside it.
    #[test]
    fn one_readable_mesh_decides_the_answer() {
        let mesh = a_mesh();
        let msg = no_mesh_reason("Bike · MySwap", &[("fwheel.edf", b""), ("model.edf", &mesh)]);
        assert!(msg.contains("no parts came out of it"), "{msg}");
    }

    // The header check itself: sealed bytes and a truncated file both fail it, a mesh doesn't.
    #[test]
    fn only_a_real_header_reads_as_a_mesh() {
        assert!(crate::edf::is_edf(&a_mesh()));
        assert!(!crate::edf::is_edf(b"EDF\0"), "long enough to match, too short to parse");
        assert!(!crate::edf::is_edf(&[0xfe, 0x9c, 0xa5, 0x6a, 0, 0, 0, 0]));
    }
}


#[tauri::command]
pub async fn unpack_paint(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_paint_blocking(path))
        .await
        .map_err(|e| format!("unpack_paint task failed: {e}"))?
}

/// Paints decoded for the viewer, so re-opening one doesn't inflate it a second time.
///
/// The picker re-runs this on every selection change and on every re-open, and a gear paint is
/// tens of megabytes of DEFLATE — the pixels behind an entry, on the other hand, are far
/// fewer, because only the sheets the viewer binds are kept.
const PAINT_CACHE_CAP: usize = 4;

fn paint_cache() -> &'static std::sync::Mutex<lru::Lru<Vec<paint::PaintTexture>>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<lru::Lru<Vec<paint::PaintTexture>>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(PAINT_CACHE_CAP)))
}

/// As [`cached_bike`], for the paints: looked up and released without holding the lock.
pub fn cached_paint(key: &str) -> Option<Vec<paint::PaintTexture>> {
    paint_cache()
        .lock()
        .ok()
        .and_then(|mut c| c.get(key).cloned())
        .filter(|t| texstore::all_resident(&t.iter().map(|x| x.token.clone()).collect::<Vec<_>>()))
}

pub fn unpack_paint_blocking(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    let t0 = std::time::Instant::now();
    // Path *and* mtime, as the bike cache does, so a paint re-saved under the same name misses.
    let key = bike_cache_key(&path);
    if let Some(t) = cached_paint(&key) {
        log::info!("unpack_paint {path}: cache hit ({:?})", t0.elapsed());
        return Ok(t);
    }
    let _gate = gate::enter(&key);
    if let Some(t) = cached_paint(&key) {
        log::info!("unpack_paint {path}: cache hit, waited ({:?})", t0.elapsed());
        return Ok(t);
    }

    let textures = paint::unpack_file(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))?;
    log::info!(
        "unpack_paint {path}: {} texture(s) in {:?} | {:.1} MB resident in the texture store",
        textures.len(),
        t0.elapsed(),
        texstore::resident_bytes() as f64 / (1024.0 * 1024.0),
    );
    if let Ok(mut c) = paint_cache().lock() {
        // Cloning an entry copies names, sizes and tokens — never pixels, which stay in the
        // texture store. The displaced paint's go with it; nothing else holds those tokens.
        if let Some(dropped) = c.insert(key, textures.clone()) {
            let tokens: Vec<String> = dropped.iter().map(|t| t.token.clone()).collect();
            texstore::release(&tokens);
        }
    }
    Ok(textures)
}
