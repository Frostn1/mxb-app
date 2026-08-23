use crate::config::AppConfig;
use crate::install;
use crate::library::{self, LibraryEntry};
use crate::presets::{self, BundleRef, Loadout, Preset};
use crate::upload;
use anyhow::Context;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    pub slot: String,
    pub value: String,
    pub name: String,
    /// Destination path relative to `<MX Bikes>/mods` (forward slashes).
    pub rel_dest: String,
    pub abs_path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedSlot {
    pub slot: String,
    pub value: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePlan {
    pub assets: Vec<AssetRef>,
    pub unresolved: Vec<UnresolvedSlot>,
    pub total_size: u64,
}

#[derive(Clone, Copy)]
enum Scan {
    Bikes,
    Rider,
    Tyres,
}

struct Spec {
    slot: &'static str,
    value: String,
    scan: Scan,
    cats: &'static [&'static str],
    owner: Owner,
}

/// Which installed thing a slot's file has to sit under.
///
/// A bike livery and a gear paint both name a file that lives inside something else, but they
/// differ in how much that containment can be trusted, so they get different rules rather than
/// one flag that has to be remembered at every call site.
enum Owner {
    /// Nothing to check — the value names the thing itself.
    Any,
    /// Prefer this owner, fall back to a match elsewhere. A rider model's folder name and the
    /// profile's value for it do not always agree, and refusing the paint over that would lose
    /// a livery the game itself finds.
    Prefer(String),
    /// Must be this owner. A bike livery belongs to one bike: `Race.pnt` under another bike is
    /// a different file with a different destination, so a near miss is worse than a miss.
    Require(String),
}

impl Owner {
    fn name(&self) -> Option<&str> {
        match self {
            Owner::Any => None,
            Owner::Prefer(p) | Owner::Require(p) => Some(p.trim()).filter(|p| !p.is_empty()),
        }
    }
}

fn strip_ext(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [".pnt", ".pkz", ".zip"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

fn is_builtin(slot: &str, value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    match slot {
        "helmet" | "boots" => v == "default",
        "protection" => v == "full" || v == "neck",
        "riding_style" => v == "mx" || v == "sm",
        "tyres" => v == "p_mx",
        _ => false,
    }
}

fn rel_dest(type_folder: &str, e: &LibraryEntry) -> String {
    let folder = e.folder.trim_matches('/');
    if folder.is_empty() {
        format!("{type_folder}/{}", e.name)
    } else {
        format!("{type_folder}/{folder}/{}", e.name)
    }
}

pub fn plan(cfg: &AppConfig, loadout: &Loadout) -> anyhow::Result<BundlePlan> {
    let mut p = resolve(cfg, loadout, None)?;
    dedup_assets(&mut p.assets);
    p.total_size = p.assets.iter().map(|a| a.size).sum();
    Ok(p)
}

/// Every bike in a profile, resolved for publishing, paying for the library walk once.
///
/// Resolving a slot means matching it against every installed file, and gathering those files
/// means a full recursive walk of `mods/bikes` — which on a real install is every livery the
/// player owns. One walk per loadout is fine for the one-at-a-time callers; it is not fine for
/// "publish this rider's whole profile", where the loadout count is the number of bikes they
/// have ever sat on. Same resolution, same order, one scan.
///
/// Two things differ from [`plan`], both because the publisher uploads file by file rather
/// than zipping a folder: each loadout's livery is pinned to the bike it was read under, and
/// assets stay individually addressed as they do in [`plan_detailed`] — collapsing a gear
/// paint into the model folder containing it would drop it from a publish entirely.
pub fn plan_profile(cfg: &AppConfig, loadouts: &[(String, Loadout)]) -> Vec<BundlePlan> {
    if loadouts.is_empty() {
        return Vec::new();
    }
    let libs = Libraries::scan(cfg);
    loadouts
        .iter()
        .map(|(bike, loadout)| resolve_with(cfg, &libs, loadout, Some(bike)))
        .collect()
}

/// The same resolution as [`plan`], with every asset still addressed in its own right.
///
/// [`plan`] collapses an asset into the folder that already contains it, because a zip that
/// carries `rider/helmets/AGV` carries the liveries under it for free. Manage needs the
/// opposite: it keeps that helmet by moving nothing at all, and decides livery by livery
/// which ones the game still gets to offer — so the paint has to be named, not implied.
pub fn plan_detailed(
    cfg: &AppConfig,
    loadout: &Loadout,
    bike: Option<&str>,
) -> anyhow::Result<BundlePlan> {
    resolve(cfg, loadout, bike)
}

/// The three scans a resolution reads from, gathered once.
///
/// Exists so [`plan_profile`] can hand the same walk to every loadout. A scan is infallible
/// from the caller's point of view — an unreadable folder resolves to nothing, exactly as it did
/// when each `resolve` did its own `unwrap_or_default`.
struct Libraries {
    bikes: Vec<LibraryEntry>,
    rider: Vec<LibraryEntry>,
    tyres: Vec<LibraryEntry>,
}

impl Libraries {
    fn scan(cfg: &AppConfig) -> Self {
        Libraries {
            bikes: library::scan_library(&cfg.mods_path, "mods/bikes", &[], cfg.game())
                .unwrap_or_default(),
            rider: library::scan_library(&cfg.mods_path, "mods/rider", &[], cfg.game())
                .unwrap_or_default(),
            tyres: library::scan_library(&cfg.mods_path, "mods/tyres", &[], cfg.game())
                .unwrap_or_default(),
        }
    }
}

fn resolve(cfg: &AppConfig, loadout: &Loadout, bike: Option<&str>) -> anyhow::Result<BundlePlan> {
    Ok(resolve_with(cfg, &Libraries::scan(cfg), loadout, bike))
}

/// `bike` is the bike id the loadout was read under, where the caller knows it. A preset does
/// not have one — it dresses whichever bike it is applied to — so the livery stays loose there.
fn resolve_with(
    cfg: &AppConfig,
    libs: &Libraries,
    loadout: &Loadout,
    bike: Option<&str>,
) -> BundlePlan {
    let Libraries { bikes, rider, tyres } = libs;

    let specs = vec![
        Spec { slot: "paint", value: loadout.paint.clone(), scan: Scan::Bikes, cats: &["bikePaint"], owner: bike.map_or(Owner::Any, |b| Owner::Require(b.to_string())) },
        Spec { slot: "helmet", value: loadout.helmet.clone(), scan: Scan::Rider, cats: &["helmet"], owner: Owner::Any },
        Spec { slot: "helmet_paint", value: loadout.helmet_paint.clone(), scan: Scan::Rider, cats: &["helmetPaint"], owner: Owner::Prefer(loadout.helmet.clone()) },
        Spec { slot: "goggles_paint", value: loadout.goggles_paint.clone(), scan: Scan::Rider, cats: &["goggles"], owner: Owner::Prefer(loadout.helmet.clone()) },
        Spec { slot: "suit_paint", value: loadout.suit_paint.clone(), scan: Scan::Rider, cats: &["outfit"], owner: Owner::Prefer(loadout.rider.clone()) },
        Spec { slot: "gloves_paint", value: loadout.gloves_paint.clone(), scan: Scan::Rider, cats: &["gloves"], owner: Owner::Any },
        Spec { slot: "boots", value: loadout.boots.clone(), scan: Scan::Rider, cats: &["boots"], owner: Owner::Any },
        Spec { slot: "boots_paint", value: loadout.boots_paint.clone(), scan: Scan::Rider, cats: &["bootPaint"], owner: Owner::Prefer(loadout.boots.clone()) },
        Spec { slot: "protection", value: loadout.protection.clone(), scan: Scan::Rider, cats: &["protection"], owner: Owner::Any },
        Spec { slot: "protection_paint", value: loadout.protection_paint.clone(), scan: Scan::Rider, cats: &["protectionPaint"], owner: Owner::Prefer(loadout.protection.clone()) },
        // A custom riding style is a mod like any other. The two stock ones live in
        // `rider.pkz` and leave nothing on disk, which `is_builtin` skips rather than
        // reporting unresolved.
        Spec { slot: "riding_style", value: loadout.riding_style.clone(), scan: Scan::Rider, cats: &["animation"], owner: Owner::Any },
        Spec { slot: "tyres", value: loadout.tyres.clone(), scan: Scan::Tyres, cats: &["misc"], owner: Owner::Any },
    ];

    let mut assets: Vec<AssetRef> = Vec::new();
    let mut unresolved: Vec<UnresolvedSlot> = Vec::new();

    for spec in &specs {
        let value = spec.value.trim();
        if value.is_empty() || is_builtin(spec.slot, value) {
            continue;
        }
        let (entries, type_folder) = match spec.scan {
            Scan::Bikes => (bikes, "bikes"),
            Scan::Rider => (rider, "rider"),
            Scan::Tyres => (tyres, "tyres"),
        };

        let mut matches: Vec<&LibraryEntry> = entries
            .iter()
            .filter(|e| {
                spec.cats.contains(&e.category.as_str())
                    && strip_ext(&e.name).eq_ignore_ascii_case(value)
            })
            .collect();

        if let Some(owner) = spec.owner.name() {
            let under_owner = |e: &LibraryEntry| {
                e.parent.as_deref().map(|p| p.eq_ignore_ascii_case(owner)).unwrap_or(false)
            };
            // `Require` keeps the filter even when it empties the list. Reporting the slot
            // unresolved is the honest answer there; falling back would hand back another
            // bike's livery, which also carries that bike's destination path.
            if matches!(spec.owner, Owner::Require(_)) || matches.iter().any(|e| under_owner(e)) {
                matches.retain(|e| under_owner(e));
            }
        }

        if matches.is_empty() {
            unresolved.push(UnresolvedSlot {
                slot: spec.slot.to_string(),
                value: value.to_string(),
                reason: "not installed — can't be bundled".to_string(),
            });
            continue;
        }
        for e in matches {
            assets.push(AssetRef {
                slot: spec.slot.to_string(),
                value: value.to_string(),
                name: e.name.clone(),
                rel_dest: rel_dest(type_folder, e),
                abs_path: e.path.clone(),
                size: e.size,
                is_dir: e.kind == "folder",
            });
        }
    }

    resolve_model_swap(cfg, loadout, &mut assets, &mut unresolved);

    for (slot, value) in [("bike_font", &loadout.bike_font), ("suit_font", &loadout.suit_font)] {
        let v = value.trim();
        if !v.is_empty() && !v.eq_ignore_ascii_case("default_black") && !v.eq_ignore_ascii_case("default_white") {
            unresolved.push(UnresolvedSlot {
                slot: slot.to_string(),
                value: v.to_string(),
                reason: "custom font — bundle it manually if needed".to_string(),
            });
        }
    }

    let total_size = assets.iter().map(|a| a.size).sum();
    BundlePlan { assets, unresolved, total_size }
}

fn resolve_model_swap(
    cfg: &AppConfig,
    loadout: &Loadout,
    assets: &mut Vec<AssetRef>,
    unresolved: &mut Vec<UnresolvedSlot>,
) {
    let value = loadout.model_swap.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("Original") {
        return;
    }
    let bikes_root = library::mods_subdir(&cfg.mods_path, "mods/bikes");
    let mut found = false;
    if let Ok(rd) = std::fs::read_dir(&bikes_root) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let bike = e.file_name().to_string_lossy().into_owned();
            let variant = e.path().join("FrostMod Models").join(value);
            if variant.is_dir() {
                assets.push(AssetRef {
                    slot: "model_swap".to_string(),
                    value: value.to_string(),
                    name: value.to_string(),
                    rel_dest: format!("bikes/{bike}/FrostMod Models/{value}"),
                    abs_path: variant.to_string_lossy().into_owned(),
                    size: dir_size_deep(&variant),
                    is_dir: true,
                });
                found = true;
            }
        }
    }
    if !found {
        unresolved.push(UnresolvedSlot {
            slot: "model_swap".to_string(),
            value: value.to_string(),
            reason: "model variant not parked in the library (it may be the active model)".to_string(),
        });
    }
}

fn dedup_assets(assets: &mut Vec<AssetRef>) {
    let dirs: Vec<String> = assets
        .iter()
        .filter(|a| a.is_dir)
        .map(|a| a.rel_dest.trim_end_matches('/').to_string())
        .collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    assets.retain(|a| {
        if !seen.insert(a.rel_dest.clone()) {
            return false;
        }
        !dirs.iter().any(|d| {
            a.rel_dest != *d && a.rel_dest.starts_with(&format!("{d}/"))
        })
    });
}

/// Total bytes under `dir`, following the links a mods tree is full of. Shared with
/// [`crate::fileshare`], which sizes a picked folder the same way this sizes a model variant.
pub(crate) fn dir_size_deep(dir: &Path) -> u64 {
    let mut total = 0;
    for e in crate::linkwalk::walk(dir).into_iter().flatten() {
        if e.file_type().is_file() {
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleProgress {
    phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

pub const BUNDLE_SLUG: &str = "__preset_bundle__";

pub const BUNDLE_EVENT: &str = "preset-bundle-progress";

/// Emit one phase update on `event`. The event name is a parameter because the same
/// create/download machinery serves two flows — the preset bundle here and the file share
/// in [`crate::fileshare`] — and each has its own dialog listening.
pub(crate) fn emit(app: &AppHandle, event: &str, phase: &'static str, message: Option<String>) {
    let _ = app.emit(event, BundleProgress { phase, message });
}

fn phase(app: &AppHandle, phase: &'static str, message: Option<String>) {
    emit(app, BUNDLE_EVENT, phase, message);
}

pub async fn create(
    app: &AppHandle,
    cfg: &AppConfig,
    presets_dir: &Path,
    name: &str,
) -> anyhow::Result<String> {
    let mut preset = presets::find_preset(presets_dir, name)
        .ok_or_else(|| anyhow::anyhow!("no preset named '{name}'"))?;

    phase(app, "bundling", None);
    let plan = plan(cfg, &preset.loadout)?;
    if plan.assets.is_empty() {
        anyhow::bail!(
            "This preset has no installed assets to bundle — share the plain code instead."
        );
    }

    let work = std::env::temp_dir().join(format!("mxb-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let root = work.join("bundle");
    std::fs::create_dir_all(&root)?;

    for a in &plan.assets {
        let dest = root.join("mods").join(rel_to_native(&a.rel_dest));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let src = Path::new(&a.abs_path);
        if a.is_dir {
            copy_tree(src, &dest)?;
        } else {
            std::fs::copy(src, &dest)
                .with_context(|| format!("copying {}", a.abs_path))?;
        }
    }

    let mut meta = preset.clone();
    meta.bundle = None;
    std::fs::write(root.join("preset.json"), serde_json::to_vec_pretty(&meta)?)?;

    let zip_path = work.join(format!("{}.zip", sanitize_file(name)));
    zip_dir(&root, &zip_path)?;

    let total = human_size(file_size(&zip_path));
    phase(app, "uploading", Some(format!("Uploading {total}…")));
    let client = install::build_client()?;
    let up = upload::upload_file(&client, &zip_path, |i, n| {
        let msg = if n > 1 {
            format!("Uploading part {i} of {n} ({total})…")
        } else {
            format!("Uploading {total}…")
        };
        phase(app, "uploading", Some(msg));
    })
    .await?;

    let _ = std::fs::remove_dir_all(&work);

    let first = up
        .parts
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the upload returned no link"))?;
    // `url` stays the first slice so a one-part bundle reads exactly as it always has;
    // `parts` is only carried when there's more than one to stitch.
    let parts = if up.parts.len() > 1 { up.parts } else { Vec::new() };
    preset.bundle = Some(BundleRef { url: first, host: up.host, size: up.size, parts });
    let code = presets::encode_code_public(&preset);
    phase(app, "done", None);
    Ok(code)
}

pub async fn import(
    app: &AppHandle,
    cfg: &AppConfig,
    presets_dir: &Path,
    text: &str,
) -> anyhow::Result<Preset> {
    let preset = presets::decode_code(text)?;
    let bundle = preset
        .bundle
        .clone()
        .ok_or_else(|| anyhow::anyhow!("This code has no asset bundle — use plain Import."))?;

    let work = std::env::temp_dir().join(format!("mxb-bundle-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let archive = fetch(app, BUNDLE_EVENT, BUNDLE_SLUG, &bundle, &work).await?;

    phase(app, "installing", None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    install::extract_archive(&archive, &extracted)?;
    let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
    // Anything the receiver already has wins: a bundle ships whole asset folders, so
    // overwriting would swap their helmet mesh and their liveries for the sender's.
    install::place_mod_with(
        &extracted,
        &mods_dir,
        "bikes",
        "",
        BUNDLE_SLUG,
        install::OnConflict::Keep,
    )?;

    presets::save_preset(presets_dir, preset.clone())?;

    let _ = std::fs::remove_dir_all(&work);
    install::notify_frostmod(app, BUNDLE_SLUG);
    phase(app, "done", None);

    Ok(preset)
}

/// Bring a hosted bundle down to `work` as one archive file, whatever shape it was uploaded
/// in: a MEGA link that decrypts in-app, a sliced upload that has to be stitched, or a plain
/// single file. Shared with [`crate::fileshare`], which hosts its payload the same way.
pub(crate) async fn fetch(
    app: &AppHandle,
    event: &str,
    slug: &str,
    bundle: &BundleRef,
    work: &Path,
) -> anyhow::Result<PathBuf> {
    emit(app, event, "downloading", None);
    let client = install::build_client()?;
    let h = bundle.host.to_lowercase();
    let u = bundle.url.to_lowercase();
    if h.contains("mega") || u.contains("mega.nz") || u.contains("mega.co") {
        install::download_mega(app, &client, slug, &bundle.url, work).await
    } else if bundle.parts.len() > 1 {
        download_parts(app, event, slug, &client, bundle, work).await
    } else {
        let direct = install::resolve_direct_url(&client, &bundle.url, &bundle.host).await?;
        install::download(app, &client, slug, &direct, work).await
    }
}

/// Fetch every slice of a multi-part bundle and stitch them back into one zip. The slices are
/// raw byte ranges, so concatenating them in order reproduces the original file exactly.
async fn download_parts(
    app: &AppHandle,
    event: &str,
    slug: &str,
    client: &reqwest::Client,
    bundle: &BundleRef,
    work: &Path,
) -> anyhow::Result<PathBuf> {
    let n = bundle.parts.len();
    let dir = work.join("parts");

    let mut paths = Vec::with_capacity(n);
    for (i, url) in bundle.parts.iter().enumerate() {
        emit(app, event, "downloading", Some(format!("Downloading part {} of {n}…", i + 1)));
        // Each part lands in its own folder: the host names the file, and two parts of the
        // same bundle can easily come back under the same name.
        let into = dir.join(format!("part{}", i + 1));
        std::fs::create_dir_all(&into)?;
        let direct = install::resolve_direct_url(client, url, &bundle.host).await?;
        paths.push(
            install::download(app, client, slug, &direct, &into)
                .await
                .with_context(|| format!("part {} of {n} couldn't be downloaded", i + 1))?,
        );
    }

    emit(app, event, "downloading", Some(format!("Joining {n} parts…")));
    let zip_path = work.join("bundle.zip");
    let written = concat_files(&paths, &zip_path)?;
    if written != bundle.size {
        anyhow::bail!(
            "This bundle came back as {} instead of {} — one of its {n} parts is incomplete or \
             no longer hosted. Ask whoever shared it for a fresh code.",
            human_size(written),
            human_size(bundle.size)
        );
    }
    Ok(zip_path)
}

/// Concatenate `parts` into `dest` in order, returning the bytes written.
fn concat_files(parts: &[PathBuf], dest: &Path) -> anyhow::Result<u64> {
    let mut out = std::fs::File::create(dest)
        .with_context(|| format!("creating {}", dest.display()))?;
    let mut total = 0u64;
    for p in parts {
        let mut input =
            std::fs::File::open(p).with_context(|| format!("reading {}", p.display()))?;
        total += std::io::copy(&mut input, &mut out)?;
    }
    std::io::Write::flush(&mut out)?;
    Ok(total)
}

pub(crate) fn rel_to_native(rel: &str) -> PathBuf {
    let mut p = PathBuf::new();
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
}

/// Copy an asset folder into the bundle, resolving any links inside it — a bundle is for
/// someone else's machine, where the far end of the sender's junction doesn't exist. See
/// [`crate::linkwalk::copy_tree`].
pub(crate) fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    Ok(crate::linkwalk::copy_tree(src, dst)?)
}

pub(crate) fn zip_dir(root: &Path, zip_path: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    // Stored (no re-compression): payload is mostly already-compressed `.pkz`/`.pnt`.
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(rel, opts)?;
        let bytes = std::fs::read(entry.path())?;
        std::io::Write::write_all(&mut zip, &bytes)?;
    }
    zip.finish()?;
    Ok(())
}

pub(crate) fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

pub(crate) fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn sanitize_file(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let t = s.trim();
    if t.is_empty() { "preset-bundle".to_string() } else { t.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mxb-bundle-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The whole multi-part scheme rests on this: slices are raw byte ranges, so joining
    /// them back in order has to give the original zip byte for byte.
    #[test]
    fn joining_slices_rebuilds_the_zip() {
        let dir = tmp("concat");
        let original: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();

        let mut paths = Vec::new();
        for (i, chunk) in original.chunks(2048).enumerate() {
            let p = dir.join(format!("part{i}"));
            std::fs::write(&p, chunk).unwrap();
            paths.push(p);
        }
        assert_eq!(paths.len(), 5, "expected a short final slice");

        let dest = dir.join("bundle.zip");
        let written = concat_files(&paths, &dest).unwrap();

        assert_eq!(written, original.len() as u64);
        assert_eq!(std::fs::read(&dest).unwrap(), original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn joining_a_missing_slice_fails_instead_of_truncating() {
        let dir = tmp("concat-missing");
        let present = dir.join("part0");
        std::fs::write(&present, b"abc").unwrap();
        let paths = vec![present, dir.join("part1-never-downloaded")];

        assert!(concat_files(&paths, &dir.join("bundle.zip")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_resolves_slots_to_rel_dests() {
        let root = tmp("plan");
        touch(&root.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Blue.pnt"));
        touch(&root.join("mods/tyres/oem_mx.pkz"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.paint = "RedBud".into();
        lo.helmet = "AGV".into();
        lo.helmet_paint = "Blue".into();
        lo.tyres = "oem_mx".into();
        lo.suit_font = "MyFont".into(); // free text → unresolved

        let plan = plan(&cfg, &lo).unwrap();
        let dest = |slot: &str| plan.assets.iter().find(|a| a.slot == slot).map(|a| a.rel_dest.clone());
        assert_eq!(dest("paint").as_deref(), Some("bikes/KTM450/paints/RedBud.pnt"));
        assert_eq!(dest("helmet").as_deref(), Some("rider/helmets/AGV"));
        assert_eq!(dest("tyres").as_deref(), Some("tyres/oem_mx.pkz"));
        assert!(dest("helmet_paint").is_none());
        assert!(plan.unresolved.iter().any(|u| u.slot == "suit_font"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // Publishing a rider's whole profile plans every bike they own. `plan_profile` exists to
    // make that one library walk instead of one per bike, so the thing worth pinning is that
    // it still answers exactly what planning them separately would have.
    #[test]
    fn planning_many_at_once_answers_the_same_as_planning_each() {
        let root = tmp("plan-many");
        touch(&root.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&root.join("mods/bikes/YZ250/paints/Southwick.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut ktm = Loadout::default();
        ktm.paint = "RedBud".into();
        ktm.helmet = "AGV".into();
        let mut yam = Loadout::default();
        yam.paint = "Southwick".into();

        let loadouts =
            vec![("KTM450".to_string(), ktm.clone()), ("YZ250".to_string(), yam.clone())];
        let many = plan_profile(&cfg, &loadouts);
        assert_eq!(many.len(), 2);
        let each = [
            plan_detailed(&cfg, &ktm, Some("KTM450")).unwrap(),
            plan_detailed(&cfg, &yam, Some("YZ250")).unwrap(),
        ];
        for (batched, one) in many.iter().zip(each) {
            let dests = |p: &BundlePlan| {
                p.assets.iter().map(|a| a.rel_dest.clone()).collect::<Vec<_>>()
            };
            assert_eq!(dests(batched), dests(&one));
            assert_eq!(batched.total_size, one.total_size);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    // Livery names repeat across bikes — `default`, `race`, a team name. Matching on the
    // filename alone published whichever one the walk reached first, and `rel_dest` carries
    // that bike's folder, so the receiver installed it onto the wrong bike too.
    #[test]
    fn a_livery_resolves_under_the_bike_that_wears_it() {
        let root = tmp("livery-owner");
        touch(&root.join("mods/bikes/KTM450/paints/Race.pnt"));
        touch(&root.join("mods/bikes/YZ250/paints/Race.pnt"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.paint = "Race".into();

        for bike in ["YZ250", "KTM450"] {
            let plans = plan_profile(&cfg, &[(bike.to_string(), lo.clone())]);
            let paints = plans[0]
                .assets
                .iter()
                .filter(|a| a.slot == "paint")
                .map(|a| a.rel_dest.as_str())
                .collect::<Vec<_>>();
            assert_eq!(paints, vec![format!("bikes/{bike}/paints/Race.pnt")]);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    // The other half of that rule: when the owning bike has no such livery, the answer is
    // "unresolved", not "here is someone else's". Unlike a rider model — whose folder name and
    // profile value genuinely disagree sometimes — a bike livery has one correct home.
    #[test]
    fn a_livery_missing_from_its_own_bike_is_never_borrowed() {
        let root = tmp("livery-no-borrow");
        touch(&root.join("mods/bikes/KTM450/paints/Race.pnt"));
        touch(&root.join("mods/bikes/YZ250/paints/Southwick.pnt"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.paint = "Race".into();

        let plans = plan_profile(&cfg, &[("YZ250".to_string(), lo)]);
        assert!(!plans[0].assets.iter().any(|a| a.slot == "paint"));
        assert!(plans[0].unresolved.iter().any(|u| u.slot == "paint"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // A gear paint usually lives inside the model folder that binds it. `plan` folds it into
    // that folder on purpose — a zip carrying `rider/helmets/AGV` carries the liveries free —
    // but a publish uploads `.pnt` files one at a time and skips folders, so folding them in
    // meant every helmet, boot and protection paint silently went unshared.
    #[test]
    fn gear_paints_nested_in_their_model_stay_addressable() {
        let root = tmp("nested-gear");
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Blue.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/goggles/Smoke.pnt"));
        touch(&root.join("mods/rider/boots/Tech10/model.edf"));
        touch(&root.join("mods/rider/boots/Tech10/paints/White.pnt"));
        touch(&root.join("mods/rider/protections/Leatt/model.edf"));
        touch(&root.join("mods/rider/protections/Leatt/paints/Carbon.pnt"));

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.helmet = "AGV".into();
        lo.helmet_paint = "Blue".into();
        lo.goggles_paint = "Smoke".into();
        lo.boots = "Tech10".into();
        lo.boots_paint = "White".into();
        lo.protection = "Leatt".into();
        lo.protection_paint = "Carbon".into();

        let plans = plan_profile(&cfg, &[("YZ250".to_string(), lo.clone())]);
        let dest = |slot: &str| {
            plans[0]
                .assets
                .iter()
                .find(|a| a.slot == slot)
                .map(|a| a.rel_dest.as_str())
        };
        assert_eq!(dest("helmet_paint"), Some("rider/helmets/AGV/paints/Blue.pnt"));
        assert_eq!(dest("goggles_paint"), Some("rider/helmets/AGV/goggles/Smoke.pnt"));
        assert_eq!(dest("boots_paint"), Some("rider/boots/Tech10/paints/White.pnt"));
        assert_eq!(dest("protection_paint"), Some("rider/protections/Leatt/paints/Carbon.pnt"));

        // The zip path still collapses them, which is what makes it a smaller archive.
        let zipped = plan(&cfg, &lo).unwrap();
        assert!(!zipped.assets.iter().any(|a| a.slot == "helmet_paint"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn planning_nothing_walks_nothing() {
        // Guards the early return: a profile with no bikes must not pay for a library scan.
        let cfg = AppConfig { mods_path: "/nowhere".into(), ..Default::default() };
        assert!(plan_profile(&cfg, &[]).is_empty());
    }

    #[test]
    fn plan_skips_builtins() {
        let root = tmp("builtins");
        touch(&root.join("mods/bikes/x.txt"));
        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.helmet = "default".into();
        lo.tyres = "p_mx".into();
        lo.riding_style = "mx".into();
        let plan = plan(&cfg, &lo).unwrap();
        assert!(plan.assets.is_empty());
        assert!(plan.unresolved.is_empty(), "a stock style ships in rider.pkz, nothing to pack");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A shared preset has to carry the riding style, or it lands on the other player's
    /// machine naming a style they have no way to get.
    #[test]
    fn plan_packs_a_custom_riding_style() {
        let root = tmp("riding-style");
        touch(&root.join("mods/rider/animations/Scrub/Scrub.ini"));
        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.riding_style = "Scrub".into();

        let plan = plan(&cfg, &lo).unwrap();
        let asset = plan.assets.iter().find(|a| a.slot == "riding_style");
        assert_eq!(
            asset.map(|a| a.rel_dest.as_str()),
            Some("rider/animations/Scrub"),
            "assets: {:?}",
            plan.assets.iter().map(|a| &a.rel_dest).collect::<Vec<_>>(),
        );
        assert!(plan.unresolved.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bundle_zip_place_round_trips() {
        let root = tmp("roundtrip");
        let src = root.join("bundle");
        touch(&src.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&src.join("mods/rider/helmets/AGV/model.edf"));
        touch(&src.join("preset.json"));

        let zip_path = root.join("b.zip");
        zip_dir(&src, &zip_path).unwrap();

        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        install::extract_archive(&zip_path, &extracted).unwrap();
        let mods = root.join("game/mods");
        install::place_mod(&extracted, &mods, "bikes", "", "slug").unwrap();

        assert!(mods.join("bikes/KTM450/paints/RedBud.pnt").exists());
        assert!(mods.join("rider/helmets/AGV/model.edf").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An import fills gaps, it doesn't trade. Sharing one helmet paint ships the whole
    /// helmet — `dedup_assets` collapses the paint into its parent folder — so a receiver
    /// who already owns that helmet would otherwise have their mesh and their own liveries
    /// replaced by the sender's copies.
    #[test]
    fn importing_keeps_what_the_receiver_already_has() {
        let root = tmp("keep-existing");
        let src = root.join("bundle");
        std::fs::create_dir_all(src.join("mods/rider/helmets/AGV/paints")).unwrap();
        std::fs::write(src.join("mods/rider/helmets/AGV/model.edf"), b"theirs").unwrap();
        std::fs::write(src.join("mods/rider/helmets/AGV/paints/Theirs.pnt"), b"theirs").unwrap();

        let zip_path = root.join("b.zip");
        zip_dir(&src, &zip_path).unwrap();
        let extracted = root.join("extracted");
        std::fs::create_dir_all(&extracted).unwrap();
        install::extract_archive(&zip_path, &extracted).unwrap();

        let mods = root.join("game/mods");
        std::fs::create_dir_all(mods.join("rider/helmets/AGV/paints")).unwrap();
        std::fs::write(mods.join("rider/helmets/AGV/model.edf"), b"mine").unwrap();
        std::fs::write(mods.join("rider/helmets/AGV/paints/Mine.pnt"), b"mine").unwrap();

        let written = install::place_mod_with(
            &extracted,
            &mods,
            "bikes",
            "",
            "slug",
            install::OnConflict::Keep,
        )
        .unwrap();

        let read = |p: &str| std::fs::read(mods.join(p)).unwrap();
        assert_eq!(read("rider/helmets/AGV/model.edf"), b"mine", "their mesh stays");
        assert_eq!(read("rider/helmets/AGV/paints/Mine.pnt"), b"mine", "their paint stays");
        assert_eq!(read("rider/helmets/AGV/paints/Theirs.pnt"), b"theirs", "the new paint lands");
        assert_eq!(written, 1, "only the file they were missing was written");
        let _ = std::fs::remove_dir_all(&root);
    }
}
