use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub size: u64,
}

pub fn mods_subdir(mods_path: &str, subpath: &str) -> PathBuf {
    let mut p = PathBuf::from(mods_path);
    for seg in subpath.split(['/', '\\']).filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
}

fn sanitize_seg(seg: &str) -> String {
    seg.chars()
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

pub fn move_mod(
    mods_path: &str,
    from_path: &str,
    to_folder: &str,
    subpath: &str,
) -> anyhow::Result<()> {
    let from = PathBuf::from(from_path);
    if !from.is_file() {
        anyhow::bail!("file not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to move a file outside the {subpath} folder");
    }

    let mut dest_dir = type_dir;
    for seg in to_folder.split(['/', '\\']).filter(|s| !s.is_empty()) {
        dest_dir.push(sanitize_seg(seg));
    }
    fs::create_dir_all(&dest_dir)?;

    let name = from
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("bad file name"))?;
    let dest = dest_dir.join(name);
    if dest == from {
        return Ok(());
    }
    if dest.exists() {
        anyhow::bail!("a mod named '{}' is already in that folder", name.to_string_lossy());
    }

    // Rename when possible; fall back to copy+remove across volumes.
    if fs::rename(&from, &dest).is_err() {
        fs::copy(&from, &dest)?;
        fs::remove_file(&from)?;
    }
    Ok(())
}

pub fn uninstall_mod(mods_path: &str, from_path: &str, subpath: &str) -> anyhow::Result<()> {
    let from = PathBuf::from(from_path);
    if !from.exists() {
        anyhow::bail!("path not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to uninstall a file outside the {subpath} folder");
    }
    trash::delete(&from)?;
    Ok(())
}

pub fn reveal_in_explorer(path: &str) -> anyhow::Result<()> {
    let p = PathBuf::from(path);
    if !p.exists() {
        anyhow::bail!("path not found: {path}");
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&p)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-R").arg(&p).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // No portable "select the file" on Linux — open its parent folder.
        let target = p.parent().unwrap_or(&p);
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

pub fn scan_mods(mods_path: &str, subpath: &str) -> anyhow::Result<Vec<InstalledMod>> {
    let dir = mods_subdir(mods_path, subpath);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut items = Vec::new();
    for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_pkz = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pkz"))
            .unwrap_or(false);
        if !is_pkz {
            continue;
        }

        let folder = path
            .parent()
            .and_then(|p| p.strip_prefix(&dir).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        items.push(InstalledMod {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            folder,
            size,
        });
    }

    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderTargets {
    pub helmets: Vec<String>,
    pub boots: Vec<String>,
    pub protection: Vec<String>,
    pub profiles: Vec<String>,
}

pub fn scan_rider_targets(mods_path: &str) -> RiderTargets {
    let base = mods_subdir(mods_path, "mods/rider");
    let dirs_in = |sub: &str| -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(base.join(sub)) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    if let Some(n) = e.file_name().to_str() {
                        out.push(n.to_string());
                    }
                }
            }
        }
        out.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        out
    };
    let models_in = |sub: &str| -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(base.join(sub)) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    if let Some(n) = e.file_name().to_str() {
                        out.push(n.to_string());
                    }
                } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkz")) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        out.push(stem.to_string());
                    }
                }
            }
        }
        out.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        out.dedup();
        out
    };
    RiderTargets {
        helmets: models_in("helmets"),
        boots: models_in("boots"),
        protection: models_in("protection"),
        profiles: dirs_in("riders"),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub size: u64,
    pub kind: String,
    pub category: String,
    pub parent: Option<String>,
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn strip_ext(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [".pkz", ".pnt", ".zip"] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    name.to_string()
}

fn rel_folder(base: &Path, path: &Path) -> String {
    path.parent()
        .and_then(|p| p.strip_prefix(base).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                if m.is_file() {
                    total += m.len();
                }
            }
        }
    }
    total
}

fn immediate_dirs(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(base) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    out
}

fn make_entry(base: &Path, p: &Path, category: &str, parent: Option<String>) -> LibraryEntry {
    let is_dir = p.is_dir();
    let kind = if is_dir {
        "folder"
    } else if has_ext(p, "pkz") {
        "pkz"
    } else {
        "loose"
    };
    let size = if is_dir {
        dir_size(p)
    } else {
        fs::metadata(p).map(|m| m.len()).unwrap_or(0)
    };
    LibraryEntry {
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: p.to_string_lossy().into_owned(),
        folder: rel_folder(base, p),
        size,
        kind: kind.to_string(),
        category: category.to_string(),
        parent,
    }
}

const TRACK_MARKERS: [&str; 5] = ["map", "trh", "tsc", "rdf", "ssc"];

fn dir_has_track_markers(dir: &Path) -> bool {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                    if TRACK_MARKERS.contains(&ext.to_ascii_lowercase().as_str()) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn collect_loose(
    base: &Path,
    dir: &Path,
    category: &str,
    parent: Option<&str>,
    out: &mut Vec<LibraryEntry>,
) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && (has_ext(&p, "pnt") || has_ext(&p, "pkz")) {
                out.push(make_entry(base, &p, category, parent.map(str::to_string)));
            }
        }
    }
}

fn collect_pkz_shallow(base: &Path, dir: &Path, category: &str, out: &mut Vec<LibraryEntry>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && has_ext(&p, "pkz") {
                out.push(make_entry(base, &p, category, None));
            }
        }
    }
}

fn sort_entries(v: &mut [LibraryEntry]) {
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
}

fn scan_tracks(dir: &Path) -> Vec<LibraryEntry> {
    let mut out = Vec::new();
    let mut track_dirs: Vec<PathBuf> = Vec::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let p = entry.path();
        if p == dir || track_dirs.iter().any(|t| p.starts_with(t)) {
            continue;
        }
        if dir_has_track_markers(p) {
            track_dirs.push(p.to_path_buf());
            out.push(make_entry(dir, p, "track", None));
        }
    }

    // Packaged `.pkz`, skipping any that live inside an extracted-track folder.
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if has_ext(p, "pkz") && !track_dirs.iter().any(|t| p.starts_with(t)) {
            out.push(make_entry(dir, p, "track", None));
        }
    }

    sort_entries(&mut out);
    out
}

const SOUND_MARKERS: [&str; 2] = ["engine.scl", "sfx.cfg"];

fn dir_has_sound_markers(dir: &Path) -> bool {
    let mut found = [false; SOUND_MARKERS.len()];
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_file() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                for (i, m) in SOUND_MARKERS.iter().enumerate() {
                    if name.eq_ignore_ascii_case(m) {
                        found[i] = true;
                    }
                }
            }
        }
    }
    found.iter().all(|&f| f)
}

fn scan_bikes(dir: &Path, sound_bikes: &[String]) -> Vec<LibraryEntry> {
    let mut out = Vec::new();

    for name in sound_bikes {
        let folder = dir.join(name);
        if folder.is_dir() && dir_has_sound_markers(&folder) {
            out.push(make_entry(dir, &folder, "sound", None));
        }
    }

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let is_pnt = has_ext(p, "pnt");
        let is_pkz = has_ext(p, "pkz");
        if !is_pnt && !is_pkz {
            continue;
        }
        let folder = rel_folder(dir, p);
        let segs: Vec<&str> = folder.split('/').filter(|s| !s.is_empty()).collect();
        let paints_pos = segs.iter().position(|s| s.eq_ignore_ascii_case("paints"));

        if let Some(pos) = paints_pos {
            // `<Bike>/paints/…` livery — owner is the segment before `paints`.
            let parent = if pos > 0 { Some(segs[pos - 1].to_string()) } else { None };
            out.push(make_entry(dir, p, "bikePaint", parent));
        } else if is_pkz {
            out.push(make_entry(dir, p, "bike", None));
        }
        // A loose `.pnt` outside any `paints` folder is a stray — ignore it.
    }

    let bike_names: HashSet<String> = out
        .iter()
        .filter(|e| e.category == "bike" && e.folder.is_empty())
        .map(|e| strip_ext(&e.name).to_lowercase())
        .collect();
    for e in out.iter_mut() {
        if e.category != "bike" || e.folder.is_empty() {
            continue;
        }
        if let Some(last) = e.folder.rsplit('/').next() {
            if bike_names.contains(&last.to_lowercase()) {
                e.category = "bikeModelSwap".to_string();
                e.parent = Some(last.to_string());
            }
        }
    }

    sort_entries(&mut out);
    out
}

fn scan_rider(dir: &Path) -> Vec<LibraryEntry> {
    let mut out = Vec::new();

    for (area, model_cat, paint_cat) in [
        ("helmets", "helmet", "helmetPaint"),
        ("boots", "boots", "bootPaint"),
        ("protection", "protection", "protectionPaint"),
    ] {
        let abase = dir.join(area);
        for model in immediate_dirs(&abase) {
            let mpath = abase.join(&model);
            out.push(make_entry(dir, &mpath, model_cat, None));
            collect_loose(dir, &mpath.join("paints"), paint_cat, Some(&model), &mut out);
            if area == "helmets" {
                collect_loose(dir, &mpath.join("goggles"), "goggles", Some(&model), &mut out);
            }
        }
        // A model packaged as a bare `.pkz` directly under the area folder.
        collect_pkz_shallow(dir, &abase, model_cat, &mut out);
        if let Ok(rd) = fs::read_dir(&abase) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && has_ext(&p, "pnt") {
                    out.push(make_entry(dir, &p, paint_cat, None));
                }
            }
        }
    }

    // Gloves installed directly under rider/gloves.
    collect_loose(dir, &dir.join("gloves"), "gloves", None, &mut out);
    collect_pkz_shallow(dir, &dir.join("gloves"), "gloves", &mut out);

    // Rider profiles: outfit/kit paints, gloves, and goggles live per profile.
    for profile in immediate_dirs(&dir.join("riders")) {
        let pbase = dir.join("riders").join(&profile);
        collect_loose(dir, &pbase.join("paints"), "outfit", Some(&profile), &mut out);
        collect_loose(dir, &pbase.join("gloves"), "gloves", Some(&profile), &mut out);
        collect_loose(dir, &pbase.join("goggles"), "goggles", Some(&profile), &mut out);
    }

    sort_entries(&mut out);
    out
}

fn scan_generic(dir: &Path) -> Vec<LibraryEntry> {
    let mut out = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && has_ext(entry.path(), "pkz") {
            out.push(make_entry(dir, entry.path(), "misc", None));
        }
    }
    sort_entries(&mut out);
    out
}

pub fn scan_library(
    mods_path: &str,
    subpath: &str,
    sound_bikes: &[String],
) -> anyhow::Result<Vec<LibraryEntry>> {
    let dir = mods_subdir(mods_path, subpath);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let kind = subpath.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or("");
    Ok(match kind {
        "tracks" => scan_tracks(&dir),
        "bikes" => scan_bikes(&dir, sound_bikes),
        "rider" => scan_rider(&dir),
        _ => scan_generic(&dir),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-lib-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn moves_mod_between_folders() {
        let root = tmp("move");
        let old = root.join("mods").join("tracks").join("Old");
        fs::create_dir_all(&old).unwrap();
        let file = old.join("t.pkz");
        fs::write(&file, b"x").unwrap();

        move_mod(
            root.to_str().unwrap(),
            file.to_str().unwrap(),
            "New Folder",
            "mods/tracks",
        )
        .unwrap();

        assert!(!file.exists());
        assert!(root.join("mods/tracks/New Folder/t.pkz").exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn cat<'a>(v: &'a [LibraryEntry], name: &str) -> Option<&'a LibraryEntry> {
        v.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    #[test]
    fn scans_extracted_tracks_and_pkz() {
        let root = tmp("lib-tracks");
        let base = root.join("mods/tracks");
        touch(&base.join("Packed.pkz"));
        touch(&base.join("Loose Track/Loose.map"));
        touch(&base.join("Loose Track/Loose.cfg"));
        touch(&base.join("Loose Track/Loose.pkz")); // inside a track folder → skipped

        let v = scan_library(root.to_str().unwrap(), "mods/tracks", &[]).unwrap();
        assert!(cat(&v, "Packed.pkz").is_some());
        let lt = cat(&v, "Loose Track").expect("extracted track surfaced");
        assert_eq!(lt.kind, "folder");
        assert_eq!(lt.category, "track");
        // The .pkz inside the extracted track must not double-count.
        assert!(cat(&v, "Loose.pkz").is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn classifies_bike_paints_and_model_swaps() {
        let root = tmp("lib-bikes");
        let base = root.join("mods/bikes");
        touch(&base.join("KTM450.pkz")); // top-level bike
        touch(&base.join("KTM450/paints/Red.pnt")); // livery for it
        touch(&base.join("KTM450/OEM2024.pkz")); // model swap for it

        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &[]).unwrap();
        assert_eq!(cat(&v, "KTM450.pkz").unwrap().category, "bike");
        let paint = cat(&v, "Red.pnt").unwrap();
        assert_eq!(paint.category, "bikePaint");
        assert_eq!(paint.parent.as_deref(), Some("KTM450"));
        let swap = cat(&v, "OEM2024.pkz").unwrap();
        assert_eq!(swap.category, "bikeModelSwap");
        assert_eq!(swap.parent.as_deref(), Some("KTM450"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn surfaces_recorded_sound_mods() {
        let root = tmp("lib-sound");
        let base = root.join("mods/bikes");
        // A sound-modded OEM bike folder (loose configs, no .pkz).
        touch(&base.join("MX2OEM_2023_KTM_250_SX-F/engine.scl"));
        touch(&base.join("MX2OEM_2023_KTM_250_SX-F/sfx.cfg"));
        touch(&base.join("Stock/model.edf"));

        let recorded = vec![
            "MX2OEM_2023_KTM_250_SX-F".to_string(),
            "Gone".to_string(),
            "Stock".to_string(),
        ];
        let v = scan_library(root.to_str().unwrap(), "mods/bikes", &recorded).unwrap();
        let s = cat(&v, "MX2OEM_2023_KTM_250_SX-F").expect("sound bike surfaced");
        assert_eq!(s.category, "sound");
        assert_eq!(s.kind, "folder");
        assert!(cat(&v, "Gone").is_none(), "removed bike pruned");
        assert!(
            v.iter().all(|e| e.name != "Stock" || e.category != "sound"),
            "a recorded folder without sound markers isn't a sound entry",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn surfaces_all_rider_categories() {
        let root = tmp("lib-rider");
        let base = root.join("mods/rider");
        touch(&base.join("helmets/AGV/AGV.pkz"));
        touch(&base.join("helmets/AGV/paints/Blue.pnt"));
        touch(&base.join("helmets/AGV/goggles/Smoke.pnt"));
        touch(&base.join("boots/Tech10/paints/Wht.pnt"));
        touch(&base.join("boots/Purple White Alpinestar Boots.pnt"));
        touch(&base.join("gloves/Flexair.pnt"));
        touch(&base.join("riders/default_mx/paints/Kit.pnt"));
        touch(&base.join("riders/default_mx/gloves/G.pnt"));

        let v = scan_library(root.to_str().unwrap(), "mods/rider", &[]).unwrap();
        let has = |c: &str| v.iter().any(|e| e.category == c);
        assert!(has("helmet"), "helmet model");
        assert!(has("helmetPaint"), "helmet paint");
        assert!(has("goggles"), "goggles");
        assert!(has("bootPaint"), "boot paint");
        assert!(
            cat(&v, "Purple White Alpinestar Boots.pnt")
                .is_some_and(|e| e.category == "bootPaint" && e.parent.is_none()),
            "loose boot paint under boots/ surfaces as a parentless bootPaint",
        );
        assert!(has("gloves"), "gloves");
        assert!(has("outfit"), "outfit/kit");
        assert_eq!(cat(&v, "Kit.pnt").unwrap().parent.as_deref(), Some("default_mx"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn move_rejects_file_outside_type_dir() {
        let root = tmp("move-guard");
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside.pkz");
        fs::write(&outside, b"x").unwrap();

        let res = move_mod(
            root.to_str().unwrap(),
            outside.to_str().unwrap(),
            "X",
            "mods/tracks",
        );
        assert!(res.is_err());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
