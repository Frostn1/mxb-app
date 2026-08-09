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
    parent: Option<String>,
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
    let mut p = resolve(cfg, loadout)?;
    dedup_assets(&mut p.assets);
    p.total_size = p.assets.iter().map(|a| a.size).sum();
    Ok(p)
}

/// [`plan`] for several loadouts, paying for the library walk once.
///
/// Resolving a slot means matching it against every installed file, and gathering those files
/// means a full recursive walk of `mods/bikes` — which on a real install is every livery the
/// player owns. One walk per loadout is fine for the one-at-a-time callers; it is not fine for
/// "publish this rider's whole profile", where the loadout count is the number of bikes they
/// have ever sat on. Same resolution, same order, one scan.
pub fn plan_many(cfg: &AppConfig, loadouts: &[Loadout]) -> Vec<BundlePlan> {
    if loadouts.is_empty() {
        return Vec::new();
    }
    let libs = Libraries::scan(cfg);
    loadouts
        .iter()
        .map(|loadout| {
            let mut p = resolve_with(cfg, &libs, loadout);
            dedup_assets(&mut p.assets);
            p.total_size = p.assets.iter().map(|a| a.size).sum();
            p
        })
        .collect()
}

/// The same resolution as [`plan`], with every asset still addressed in its own right.
///
/// [`plan`] collapses an asset into the folder that already contains it, because a zip that
/// carries `rider/helmets/AGV` carries the liveries under it for free. Manage needs the
/// opposite: it keeps that helmet by moving nothing at all, and decides livery by livery
/// which ones the game still gets to offer — so the paint has to be named, not implied.
pub fn plan_detailed(cfg: &AppConfig, loadout: &Loadout) -> anyhow::Result<BundlePlan> {
    resolve(cfg, loadout)
}

/// The three scans a resolution reads from, gathered once.
///
/// Exists so [`plan_many`] can hand the same walk to every loadout. A scan is infallible from
/// the caller's point of view — an unreadable folder resolves to nothing, exactly as it did
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

fn resolve(cfg: &AppConfig, loadout: &Loadout) -> anyhow::Result<BundlePlan> {
    Ok(resolve_with(cfg, &Libraries::scan(cfg), loadout))
}

fn resolve_with(cfg: &AppConfig, libs: &Libraries, loadout: &Loadout) -> BundlePlan {
    let Libraries { bikes, rider, tyres } = libs;

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
        // A custom riding style is a mod like any other. The two stock ones live in
        // `rider.pkz` and leave nothing on disk, which `is_builtin` skips rather than
        // reporting unresolved.
        Spec { slot: "riding_style", value: loadout.riding_style.clone(), scan: Scan::Rider, cats: &["animation"], parent: None },
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

fn dir_size_deep(dir: &Path) -> u64 {
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

fn phase(app: &AppHandle, phase: &'static str, message: Option<String>) {
    let _ = app.emit("preset-bundle-progress", BundleProgress { phase, message });
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

    phase(app, "uploading", Some(format!("Uploading {}…", human_size(file_size(&zip_path)))));
    let client = install::build_client()?;
    let up = upload::upload_file(&client, &zip_path).await?;

    let _ = std::fs::remove_dir_all(&work);

    preset.bundle = Some(BundleRef { url: up.url, host: up.host, size: up.size });
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

    // MEGA links decrypt in-app; everything else streams via the shared downloader.
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

    phase(app, "installing", None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    install::extract_archive(&archive, &extracted)?;
    let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
    install::place_mod(&extracted, &mods_dir, "bikes", "", BUNDLE_SLUG)?;

    presets::save_preset(presets_dir, preset.clone())?;

    let _ = std::fs::remove_dir_all(&work);
    install::notify_frostmod(app, BUNDLE_SLUG);
    phase(app, "done", None);

    Ok(preset)
}

fn rel_to_native(rel: &str) -> PathBuf {
    let mut p = PathBuf::new();
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
}

/// Copy an asset folder into the bundle, resolving any links inside it — a bundle is for
/// someone else's machine, where the far end of the sender's junction doesn't exist. See
/// [`crate::linkwalk::copy_tree`].
fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    Ok(crate::linkwalk::copy_tree(src, dst)?)
}

fn zip_dir(root: &Path, zip_path: &Path) -> anyhow::Result<()> {
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

    // Publishing a rider's whole profile plans every bike they own. `plan_many` exists to
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

        let loadouts = vec![ktm.clone(), yam.clone()];
        let many = plan_many(&cfg, &loadouts);
        assert_eq!(many.len(), 2);
        for (batched, one) in many.iter().zip([plan(&cfg, &ktm).unwrap(), plan(&cfg, &yam).unwrap()])
        {
            let dests = |p: &BundlePlan| {
                p.assets.iter().map(|a| a.rel_dest.clone()).collect::<Vec<_>>()
            };
            assert_eq!(dests(batched), dests(&one));
            assert_eq!(batched.total_size, one.total_size);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn planning_nothing_walks_nothing() {
        // Guards the early return: a profile with no bikes must not pay for a library scan.
        let cfg = AppConfig { mods_path: "/nowhere".into(), ..Default::default() };
        assert!(plan_many(&cfg, &[]).is_empty());
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
}
