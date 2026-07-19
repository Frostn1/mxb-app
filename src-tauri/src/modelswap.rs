use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const LIB_DIR: &str = "FrostMod Models";
const MARKER: &str = "_active.txt";
const ORIGINAL: &str = "Original";
const MODEL_EDF: &str = "model.edf";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelVariant {
    pub name: String,
    pub active: bool,
    pub valid: bool,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeModels {
    pub bike: String,
    pub active: String,
    pub variants: Vec<ModelVariant>,
}

fn bikes_root(mods_path: &str) -> PathBuf {
    crate::library::mods_subdir(mods_path, "mods/bikes")
}
fn bike_dir(mods_path: &str, bike: &str) -> PathBuf {
    bikes_root(mods_path).join(bike)
}
fn lib_dir(mods_path: &str, bike: &str) -> PathBuf {
    bike_dir(mods_path, bike).join(LIB_DIR)
}
fn variant_dir(mods_path: &str, bike: &str, name: &str) -> PathBuf {
    lib_dir(mods_path, bike).join(name)
}

fn is_simple_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains(':')
}

fn read_active(mods_path: &str, bike: &str) -> String {
    fs::read_to_string(lib_dir(mods_path, bike).join(MARKER))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub const ORIGINAL_LABEL: &str = ORIGINAL;

pub fn current_active(mods_path: &str, bike: &str) -> String {
    let a = read_active(mods_path, bike);
    if a.is_empty() {
        ORIGINAL.to_string()
    } else {
        a
    }
}

fn write_active(mods_path: &str, bike: &str, name: &str) -> anyhow::Result<()> {
    let lib = lib_dir(mods_path, bike);
    fs::create_dir_all(&lib)?;
    fs::write(lib.join(MARKER), name)?;
    Ok(())
}

fn list_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_file() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
    }
    out
}

fn dir_exists(p: &Path) -> bool {
    p.is_dir()
}
fn file_exists(p: &Path) -> bool {
    p.is_file()
}

fn move_set(src: &Path, dst: &Path, files: &[String]) -> bool {
    if fs::create_dir_all(dst).is_err() {
        return false;
    }
    let mut done: Vec<&String> = Vec::new();
    for f in files {
        let s = src.join(f);
        let d = dst.join(f);
        if move_one(&s, &d) {
            done.push(f);
        } else {
            for g in &done {
                let _ = move_one(&dst.join(g), &src.join(g));
            }
            return false;
        }
    }
    true
}

fn move_one(src: &Path, dst: &Path) -> bool {
    if fs::rename(src, dst).is_ok() {
        return true;
    }
    if fs::copy(src, dst).is_ok() && fs::remove_file(src).is_ok() {
        return true;
    }
    false
}

fn scan_variants(mods_path: &str, bike: &str) -> Vec<ModelVariant> {
    let active_label = {
        let a = read_active(mods_path, bike);
        if a.is_empty() { ORIGINAL.to_string() } else { a }
    };

    let mut variants = vec![ModelVariant {
        name: active_label.clone(),
        active: true,
        // The active set is the bike's loose files — valid iff model.edf is there.
        valid: file_exists(&bike_dir(mods_path, bike).join(MODEL_EDF)),
        file_count: list_files(&bike_dir(mods_path, bike)).len(),
    }];

    let mut others: Vec<ModelVariant> = Vec::new();
    if let Ok(rd) = fs::read_dir(lib_dir(mods_path, bike)) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = match e.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.eq_ignore_ascii_case(&active_label) {
                continue; // active is already row 0
            }
            others.push(ModelVariant {
                valid: file_exists(&p.join(MODEL_EDF)),
                file_count: list_files(&p).len(),
                name,
                active: false,
            });
        }
    }
    others.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    variants.extend(others);
    variants
}

pub fn scan_model_swaps(mods_path: &str) -> Vec<BikeModels> {
    let root = bikes_root(mods_path);
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&root) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let bike = match e.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            let qualifies =
                file_exists(&p.join(MODEL_EDF)) || dir_exists(&p.join(LIB_DIR));
            if bike.starts_with('.') || !qualifies {
                continue;
            }
            let variants = scan_variants(mods_path, &bike);
            let active = variants
                .iter()
                .find(|v| v.active)
                .map(|v| v.name.clone())
                .unwrap_or_else(|| ORIGINAL.to_string());
            out.push(BikeModels { bike, active, variants });
        }
    }
    out.sort_by(|a, b| a.bike.to_lowercase().cmp(&b.bike.to_lowercase()));
    out
}

pub fn apply_model_swap(mods_path: &str, bike: &str, target: &str) -> anyhow::Result<()> {
    if !is_simple_name(bike) || !is_simple_name(target) {
        anyhow::bail!("invalid bike or model name");
    }
    let root = bike_dir(mods_path, bike);
    if !dir_exists(&root) {
        anyhow::bail!("bike '{bike}' not found");
    }

    let active = read_active(mods_path, bike);
    let active_label = if active.is_empty() { ORIGINAL.to_string() } else { active };
    if target.eq_ignore_ascii_case(&active_label) {
        anyhow::bail!("'{target}' is already the active model");
    }

    let backup_dir = variant_dir(mods_path, bike, &active_label); // park the live set here
    let target_dir = variant_dir(mods_path, bike, target); // bring this set in
    if !dir_exists(&target_dir) || !file_exists(&target_dir.join(MODEL_EDF)) {
        anyhow::bail!("model '{target}' is missing its {MODEL_EDF}");
    }

    let root_files = list_files(&root); // current model files to back up
    let target_files = list_files(&target_dir); // variant files to bring in

    // 1) Back up the current set into the library (all-or-nothing).
    if !root_files.is_empty() && !move_set(&root, &backup_dir, &root_files) {
        anyhow::bail!("couldn't back up the current model — is the bike loaded in-game? Exit the bike first.");
    }
    // 2) Move the target's set into the bike root; roll the backup back on failure.
    if !move_set(&target_dir, &root, &target_files) {
        move_set(&backup_dir, &root, &root_files); // restore
        anyhow::bail!("swap failed and was rolled back (see the model files)");
    }

    write_active(mods_path, bike, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-ms-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }
    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn scans_bikes_with_variants_active_first() {
        let root = tmp("scan");
        let mp = root.to_str().unwrap();
        // Extracted bike with a loose model set.
        touch(&bike_dir(mp, "KTM450").join("model.edf"));
        touch(&bike_dir(mp, "KTM450").join("KTM450.cfg"));
        // A packed bike (no model.edf) must be ignored.
        touch(&bikes_root(mp).join("Packed").join("Packed.pkz"));
        // Two library variants + a marker naming the active one.
        touch(&variant_dir(mp, "KTM450", "OEM2024").join("model.edf"));
        touch(&variant_dir(mp, "KTM450", "Factory").join("model.edf"));
        write_active(mp, "KTM450", "Factory").unwrap();

        let bikes = scan_model_swaps(mp);
        assert_eq!(bikes.len(), 1, "only the extracted bike shows");
        let b = &bikes[0];
        assert_eq!(b.bike, "KTM450");
        assert_eq!(b.active, "Factory");
        assert!(b.variants[0].active, "active variant is row 0");
        assert_eq!(b.variants[0].name, "Factory");
        let names: Vec<_> = b.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"OEM2024"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bike_with_only_a_library_still_lists() {
        let root = tmp("lib-only");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "RM").join("RM.pkz")); // packed mesh, no loose model.edf
        touch(&variant_dir(mp, "RM", "Factory").join("model.edf"));
        write_active(mp, "RM", "Original").unwrap();

        let bikes = scan_model_swaps(mp);
        assert_eq!(bikes.len(), 1, "bike with a library folder still lists");
        assert_eq!(bikes[0].active, "Original");
        let names: Vec<_> = bikes[0].variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"Factory"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn original_is_active_when_never_swapped() {
        let root = tmp("orig");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "YZ").join("model.edf"));
        let bikes = scan_model_swaps(mp);
        assert_eq!(bikes[0].active, "Original");
        assert!(bikes[0].variants[0].active);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_swaps_sets_and_backs_up_original() {
        let root = tmp("apply");
        let mp = root.to_str().unwrap();
        // Original loose set.
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("KTM.cfg"));
        // paints/ must survive untouched.
        touch(&bike_dir(mp, "KTM").join("paints").join("Red.pnt"));
        // A variant to bring in.
        touch(&variant_dir(mp, "KTM", "Factory").join("model.edf"));
        touch(&variant_dir(mp, "KTM", "Factory").join("KTM.cfg"));

        apply_model_swap(mp, "KTM", "Factory").unwrap();

        // Marker now names Factory; the Original set is parked in the library.
        assert_eq!(read_active(mp, "KTM"), "Factory");
        assert!(file_exists(&variant_dir(mp, "KTM", "Original").join("model.edf")));
        // Bike root still has a model.edf (the Factory one) and its paints.
        assert!(file_exists(&bike_dir(mp, "KTM").join("model.edf")));
        assert!(file_exists(&bike_dir(mp, "KTM").join("paints").join("Red.pnt")));
        // Factory's own library folder is now emptied of its set.
        assert!(!file_exists(&variant_dir(mp, "KTM", "Factory").join("model.edf")));

        // Swap back to Original restores it.
        apply_model_swap(mp, "KTM", "Original").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Original");
        assert!(file_exists(&variant_dir(mp, "KTM", "Factory").join("model.edf")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_rejects_active_and_invalid_targets() {
        let root = tmp("reject");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        // Already active.
        assert!(apply_model_swap(mp, "KTM", "Original").is_err());
        // Missing variant.
        assert!(apply_model_swap(mp, "KTM", "Nope").is_err());
        // Variant folder without a model.edf is invalid.
        touch(&variant_dir(mp, "KTM", "Bad").join("readme.txt"));
        assert!(apply_model_swap(mp, "KTM", "Bad").is_err());
        // Path-traversal names are refused.
        assert!(apply_model_swap(mp, "KTM", "../../evil").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
