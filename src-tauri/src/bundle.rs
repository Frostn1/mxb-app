//! Preset **full-share** bundles — package every asset a loadout references into a
//! `.zip`, upload it to an anonymous host, and (on the other end) download +
//! install it so a recipient who owns *none* of the mods gets the complete look.
//!
//! ## Layout
//! The bundle is a `.zip` whose payload is a **`mods/` tree mirroring the real
//! install layout** plus a `preset.json` at the root. That's deliberate:
//! [`crate::install::place_mod`] already merges an inner `mods/` tree into
//! `<MX Bikes>/mods` preserving structure — so **import = extract + place_mod**,
//! with no new placement logic. Building the tree is the reverse of
//! [`crate::library::scan_library`]: for each non-empty slot we locate the backing
//! file/folder and copy it to its correct relative path under `mods/`.
//!
//! ## Scope
//! Presets are **bike-agnostic** (there's no "bike" slot), so a bundle carries the
//! *cosmetic layers* a loadout references — liveries, gear models + paints, gloves,
//! outfit, tyres, and model-swap variants — not the base bikes (those are the
//! recipient's). Free-text fonts, stock/builtin values, and slots whose mod isn't
//! installed can't travel; they're reported as `unresolved` for the UI to list.

use crate::config::AppConfig;
use crate::install;
use crate::library::{self, LibraryEntry};
use crate::presets::{self, BundleRef, Loadout, Preset};
use crate::upload;
use anyhow::Context;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// One asset a preset references, resolved to an on-disk source and its target
/// path under `mods/`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    /// The loadout slot this came from (e.g. `helmet`, `paint`).
    pub slot: String,
    /// The slot value (mod/paint name).
    pub value: String,
    /// File or folder name on disk.
    pub name: String,
    /// Destination path relative to `<MX Bikes>/mods` (forward slashes).
    pub rel_dest: String,
    /// Absolute source path.
    pub abs_path: String,
    /// Size in bytes (folders: sum of immediate files — an estimate).
    pub size: u64,
    /// Whether the source is a directory (copied as a tree).
    pub is_dir: bool,
}

/// A slot whose value can't be bundled, with why (shown in the share preview).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedSlot {
    pub slot: String,
    pub value: String,
    pub reason: String,
}

/// The full plan for a preset's bundle: what will travel, what won't, and the
/// (estimated) total size.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePlan {
    pub assets: Vec<AssetRef>,
    pub unresolved: Vec<UnresolvedSlot>,
    pub total_size: u64,
}

// --- slot → asset resolution ------------------------------------------------

/// Which library scan a slot's options come from.
#[derive(Clone, Copy)]
enum Scan {
    Bikes,
    Rider,
    Tyres,
}

/// A resolvable slot: where to look, which categories count, and the dependency
/// slot value that narrows the match (e.g. a helmet paint belongs to a helmet).
struct Spec {
    slot: &'static str,
    value: String,
    scan: Scan,
    cats: &'static [&'static str],
    parent: Option<String>,
}

/// Drop a trailing `.pnt`/`.pkz`/`.zip` (paint/model file extensions) for matching.
fn strip_ext(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [".pnt", ".pkz", ".zip"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

/// Builtin/stock values the game accepts with no installed mod (never bundled).
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

/// `rel_dest` under `mods/` for a library entry of a given type folder:
/// `<type>/<entry.folder>/<entry.name>` (the folder already captures nesting).
fn rel_dest(type_folder: &str, e: &LibraryEntry) -> String {
    let folder = e.folder.trim_matches('/');
    if folder.is_empty() {
        format!("{type_folder}/{}", e.name)
    } else {
        format!("{type_folder}/{folder}/{}", e.name)
    }
}

/// Resolve every bundle-able slot of a loadout against what's installed, returning
/// the plan (resolved assets + unresolved slots + total size).
pub fn plan(cfg: &AppConfig, loadout: &Loadout) -> anyhow::Result<BundlePlan> {
    let bikes = library::scan_library(&cfg.mods_path, "mods/bikes", &[]).unwrap_or_default();
    let rider = library::scan_library(&cfg.mods_path, "mods/rider", &[]).unwrap_or_default();
    let tyres = library::scan_library(&cfg.mods_path, "mods/tyres", &[]).unwrap_or_default();

    let specs = vec![
        Spec { slot: "paint", value: loadout.paint.clone(), scan: Scan::Bikes, cats: &["bikePaint"], parent: None },
        Spec { slot: "helmet", value: loadout.helmet.clone(), scan: Scan::Rider, cats: &["helmet"], parent: None },
        Spec { slot: "helmet_paint", value: loadout.helmet_paint.clone(), scan: Scan::Rider, cats: &["helmetPaint"], parent: Some(loadout.helmet.clone()) },
        Spec { slot: "goggles_paint", value: loadout.goggles_paint.clone(), scan: Scan::Rider, cats: &["goggles"], parent: Some(loadout.helmet.clone()) },
        Spec { slot: "suit_paint", value: loadout.suit_paint.clone(), scan: Scan::Rider, cats: &["outfit"], parent: Some(loadout.rider.clone()) },
        Spec { slot: "gloves_paint", value: loadout.gloves_paint.clone(), scan: Scan::Rider, cats: &["gloves"], parent: None },
        Spec { slot: "boots", value: loadout.boots.clone(), scan: Scan::Rider, cats: &["boots"], parent: None },
        Spec { slot: "boots_paint", value: loadout.boots_paint.clone(), scan: Scan::Rider, cats: &["bootPaint"], parent: Some(loadout.boots.clone()) },
        Spec { slot: "protection", value: loadout.protection.clone(), scan: Scan::Rider, cats: &["protection"], parent: None },
        Spec { slot: "protection_paint", value: loadout.protection_paint.clone(), scan: Scan::Rider, cats: &["protectionPaint"], parent: Some(loadout.protection.clone()) },
        Spec { slot: "tyres", value: loadout.tyres.clone(), scan: Scan::Tyres, cats: &["misc"], parent: None },
    ];

    let mut assets: Vec<AssetRef> = Vec::new();
    let mut unresolved: Vec<UnresolvedSlot> = Vec::new();

    for spec in &specs {
        let value = spec.value.trim();
        if value.is_empty() || is_builtin(spec.slot, value) {
            continue;
        }
        let (entries, type_folder) = match spec.scan {
            Scan::Bikes => (&bikes, "bikes"),
            Scan::Rider => (&rider, "rider"),
            Scan::Tyres => (&tyres, "tyres"),
        };

        // Entries whose category + name match this slot value.
        let mut matches: Vec<&LibraryEntry> = entries
            .iter()
            .filter(|e| {
                spec.cats.contains(&e.category.as_str())
                    && strip_ext(&e.name).eq_ignore_ascii_case(value)
            })
            .collect();

        // If a dependency parent is set and any match has it, keep only those (a
        // paint belongs to its specific model / profile).
        if let Some(parent) = spec.parent.as_ref().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            if matches.iter().any(|e| {
                e.parent.as_deref().map(|p| p.eq_ignore_ascii_case(parent)).unwrap_or(false)
            }) {
                matches.retain(|e| {
                    e.parent.as_deref().map(|p| p.eq_ignore_ascii_case(parent)).unwrap_or(false)
                });
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

    // Model swap: an alternate model parked at `<Bike>/FrostMod Models/<variant>`.
    resolve_model_swap(cfg, loadout, &mut assets, &mut unresolved);

    // Free-text fonts can't be tied to an installed file — note them.
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

    // Drop assets nested inside another bundled folder (already carried by it), and
    // exact-path duplicates.
    dedup_assets(&mut assets);

    let total_size = assets.iter().map(|a| a.size).sum();
    Ok(BundlePlan { assets, unresolved, total_size })
}

/// Find a model-swap variant folder (`mods/bikes/<Bike>/FrostMod Models/<value>`)
/// across every bike and bundle it. If none exists on disk (e.g. the variant is the
/// bike's *active* loose model), report it as unresolved.
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

/// Remove assets whose `rel_dest` is nested inside another asset's directory
/// `rel_dest` (already carried by it), plus exact-path duplicates.
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
        // Drop if it lives inside one of the bundled directories (and isn't that dir).
        !dirs.iter().any(|d| {
            a.rel_dest != *d && a.rel_dest.starts_with(&format!("{d}/"))
        })
    });
}

fn dir_size_deep(dir: &Path) -> u64 {
    let mut total = 0;
    for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if e.file_type().is_file() {
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

// --- progress ---------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleProgress {
    phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Progress `slug` the bundle flow passes to the shared downloader, so the UI can
/// pick byte-level `install-progress` events out for the download phase.
pub const BUNDLE_SLUG: &str = "__preset_bundle__";

fn phase(app: &AppHandle, phase: &'static str, message: Option<String>) {
    let _ = app.emit("preset-bundle-progress", BundleProgress { phase, message });
}

// --- build (share side) -----------------------------------------------------

/// Create a preset's full-share code: resolve its assets, zip them, upload the
/// bundle, and return the share code with the bundle link embedded.
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

    // Copy each asset into `bundle/mods/<rel_dest>`.
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

    // `preset.json` (without the transient bundle link) travels for portability.
    let mut meta = preset.clone();
    meta.bundle = None;
    std::fs::write(root.join("preset.json"), serde_json::to_vec_pretty(&meta)?)?;

    // Zip it.
    let zip_path = work.join(format!("{}.zip", sanitize_file(name)));
    zip_dir(&root, &zip_path)?;

    // Upload and build the share code.
    phase(app, "uploading", Some(format!("Uploading {}…", human_size(file_size(&zip_path)))));
    let client = install::build_client()?;
    let up = upload::upload_file(&client, &zip_path).await?;

    let _ = std::fs::remove_dir_all(&work);

    preset.bundle = Some(BundleRef { url: up.url, host: up.host, size: up.size });
    let code = presets::encode_code_public(&preset);
    phase(app, "done", None);
    Ok(code)
}

// --- import (recipient side) ------------------------------------------------

/// Import a full-share code: decode it, download the bundle from its link, extract
/// + place every asset into the game's `mods/`, and save the preset.
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

    // Download the bundle (MEGA links decrypt in-app; everything else is a direct /
    // resolvable URL streamed by the shared downloader).
    phase(app, "downloading", None);
    let client = install::build_client()?;
    let h = bundle.host.to_lowercase();
    let u = bundle.url.to_lowercase();
    let archive = if h.contains("mega") || u.contains("mega.nz") || u.contains("mega.co") {
        install::download_mega(app, &client, BUNDLE_SLUG, &bundle.url, &work).await?
    } else {
        let direct = install::resolve_direct_url(&client, &bundle.url, &bundle.host).await?;
        install::download(app, &client, BUNDLE_SLUG, &direct, &work).await?
    };

    // Extract + place the inner `mods/` tree (place_mod merges it preserving
    // structure — the same path a normal install takes).
    phase(app, "installing", None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    install::extract_archive(&archive, &extracted)?;
    let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
    install::place_mod(&extracted, &mods_dir, "bikes", "", BUNDLE_SLUG)?;

    // Save the preset (bundle link stripped inside save_preset).
    presets::save_preset(presets_dir, preset.clone())?;

    let _ = std::fs::remove_dir_all(&work);
    install::notify_frostmod(app, BUNDLE_SLUG);
    phase(app, "done", None);

    Ok(preset)
}

// --- helpers ----------------------------------------------------------------

/// Turn a forward-slash `rel_dest` into a native relative path.
fn rel_to_native(rel: &str) -> PathBuf {
    let mut p = PathBuf::new();
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
}

/// Recursively copy `src` into `dst`, creating folders as needed.
fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Zip everything under `root` into `zip_path`, storing (no re-compression — the
/// payload is mostly already-compressed `.pkz`/`.pnt`).
fn zip_dir(root: &Path, zip_path: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
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

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn human_size(bytes: u64) -> String {
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

/// Strip separators/unsafe chars from a preset name used as a zip filename.
fn sanitize_file(name: &str) -> String {
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

    /// A loadout resolves each slot to the right on-disk file at the right
    /// `mods/`-relative destination.
    #[test]
    fn plan_resolves_slots_to_rel_dests() {
        let root = tmp("plan");
        // Bike livery, helmet model + paint, tyres.
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
        // The helmet paint is inside the bundled helmet folder → deduped away.
        assert!(dest("helmet_paint").is_none());
        assert!(plan.unresolved.iter().any(|u| u.slot == "suit_font"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Builtin/stock values (default helmet, p_mx tyres) never resolve to assets.
    #[test]
    fn plan_skips_builtins() {
        let root = tmp("builtins");
        touch(&root.join("mods/bikes/x.txt"));
        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut lo = Loadout::default();
        lo.helmet = "default".into();
        lo.tyres = "p_mx".into();
        let plan = plan(&cfg, &lo).unwrap();
        assert!(plan.assets.is_empty());
        assert!(plan.unresolved.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Round-trip: build a bundle tree → zip → extract → place reproduces every
    /// file at its correct spot under a fresh `mods/` (no host/network).
    #[test]
    fn bundle_zip_place_round_trips() {
        let root = tmp("roundtrip");
        // Fake a resolved bundle tree.
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
}
