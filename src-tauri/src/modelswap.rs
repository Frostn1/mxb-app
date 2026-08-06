use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const LIB_DIR: &str = "FrostMod Models";
const MARKER: &str = "_active.txt";
const ORIGINAL: &str = "Original";
/// Per-variant record of the filenames that variant owns, written whenever we park a
/// set. Lets the reverse swap move back exactly what it moved out instead of guessing.
const MANIFEST: &str = "_files.txt";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelVariant {
    pub name: String,
    pub active: bool,
    pub valid: bool,
    /// No files at all — an intentional "no model" swap (removes the current model),
    /// distinct from an incomplete set that has files but is missing `model.edf`.
    pub empty: bool,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeModels {
    pub bike: String,
    pub active: String,
    pub variants: Vec<ModelVariant>,
}

/// A model-set folder found loose inside a bike dir (dropped at the bike root or in an
/// ad-hoc container folder) that isn't yet registered under `FrostMod Models/`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LooseSwapCandidate {
    /// The variant name (the folder's own name) it would be registered under.
    pub name: String,
    /// Path relative to the bike dir, used to locate the folder for the move
    /// (`"Factory OEM"` or `"models/Factory OEM"`).
    pub source: String,
    /// `"model"` (a `model.edf` set → `FrostMod Models/`) or `"sound"` (an
    /// `engine.scl` + `sfx.cfg` set → `FrostMod Sounds/`).
    pub kind: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LooseSwapBike {
    pub bike: String,
    pub candidates: Vec<LooseSwapCandidate>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterReport {
    /// Bikes that had at least one candidate.
    pub bikes: usize,
    /// Candidate folders successfully moved into `FrostMod Models/`.
    pub registered: usize,
    /// Candidates skipped (name already taken, or the move failed).
    pub skipped: usize,
    /// `FrostMod Models/` folders newly created on disk.
    pub folders_created: usize,
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
fn is_bookkeeping(name: &str) -> bool {
    name.eq_ignore_ascii_case(MANIFEST) || name.eq_ignore_ascii_case(MARKER)
}

/// The files a parked variant actually consists of — its own bookkeeping doesn't count.
fn set_files(dir: &Path) -> Vec<String> {
    list_files(dir)
        .into_iter()
        .filter(|f| !is_bookkeeping(f))
        .collect()
}

fn read_manifest(dir: &Path) -> Option<Vec<String>> {
    let text = fs::read_to_string(dir.join(MANIFEST)).ok()?;
    let files: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if files.is_empty() { None } else { Some(files) }
}

fn write_manifest(dir: &Path, files: &[String]) {
    if files.is_empty() {
        let _ = fs::remove_file(dir.join(MANIFEST));
        return;
    }
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(dir.join(MANIFEST), format!("{}\n", files.join("\n")));
}

fn contains_ci(haystack: &[String], needle: &str) -> bool {
    haystack.iter().any(|h| h.eq_ignore_ascii_case(needle))
}

/// Every filename mentioned by a *parked* variant other than `except`. Used to scope the
/// very first swap of a bike, before any manifest exists: the files this bike's swaps
/// deal with are exactly the files its swaps contain.
fn files_known_to_other_variants(mods_path: &str, bike: &str, except: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(lib_dir(mods_path, bike)) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if except.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
                continue;
            }
            for f in read_manifest(&p).unwrap_or_else(|| set_files(&p)) {
                if !contains_ci(&out, &f) {
                    out.push(f);
                }
            }
        }
    }
    out
}

/// The loose root files that belong to the **active model set** — never the whole folder.
///
/// A bike root holds far more than its model: the `.hrc`s naming each part's scene, plus
/// `.cfg`/`.geom` and physics data. Parking all of it (what versions up to 0.6.1 did) is
/// what made the bike itself vanish from the game. The set is resolved as:
///
/// 1. the active variant's manifest, when we wrote one on the way in;
/// 2. otherwise every mesh at the root (a model swap always replaces the mesh) plus any
///    file this bike's other parked variants contain — self-scoping, and it leaves setup
///    the swaps never mention exactly where the game expects it.
///
/// `incoming` adds the files the arriving set would overwrite, which must be displaced
/// whatever else is true. Sound files are excluded throughout: they swap independently
/// (see `soundmods`), so a model swap must leave them at the root untouched.
fn active_set_files(mods_path: &str, bike: &str, active: &str, incoming: &[String]) -> Vec<String> {
    let root = bike_dir(mods_path, bike);
    let root_files = list_files(&root);

    let owned = match read_manifest(&variant_dir(mods_path, bike, active)) {
        Some(m) => m,
        None => {
            let mut m: Vec<String> = root_files
                .iter()
                .filter(|f| crate::bikefiles::is_mesh(f))
                .cloned()
                .collect();
            for f in files_known_to_other_variants(mods_path, bike, &[active]) {
                if !contains_ci(&m, &f) {
                    m.push(f);
                }
            }
            m
        }
    };

    root_files
        .into_iter()
        .filter(|f| !crate::soundmods::is_sound_file(f) && !is_bookkeeping(f))
        .filter(|f| contains_ci(&owned, f) || contains_ci(incoming, f))
        .collect()
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

    // The active model set is the subset of the bike's loose files that belongs to the
    // model — not the whole folder (see `active_set_files`).
    let active_files = active_set_files(mods_path, bike, &active_label, &[]).len();
    let mut variants = vec![ModelVariant {
        name: active_label.clone(),
        active: true,
        // The active set is loose at the root — valid iff a mesh is there.
        valid: crate::bikefiles::dir_has_mesh(&bike_dir(mods_path, bike)),
        empty: active_files == 0,
        file_count: active_files,
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
            let files = set_files(&p).len();
            others.push(ModelVariant {
                valid: crate::bikefiles::dir_has_mesh(&p),
                empty: files == 0,
                file_count: files,
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
            // Any mesh qualifies, not just `model.edf` — a bike may ship one EDF per
            // part (`96cr250.edf`, `96cr250_st.edf`, …), and those were invisible here.
            let qualifies =
                crate::bikefiles::dir_has_mesh(&p) || dir_exists(&p.join(LIB_DIR));
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
    if !dir_exists(&target_dir) {
        anyhow::bail!("model '{target}' not found");
    }

    let target_files = set_files(&target_dir); // variant files to bring in

    // An empty variant (no files) is an intentional "no model" swap: back up the live
    // set and bring in nothing, leaving the bike without a model. A variant that *has*
    // files but no mesh at all is an incomplete set and is rejected.
    if !target_files.is_empty() && !crate::bikefiles::dir_has_mesh(&target_dir) {
        anyhow::bail!("model '{target}' has no mesh (.edf) — it looks like an incomplete set");
    }

    // Only the files that belong to the model move. The bike's own setup stays put.
    let root_files = active_set_files(mods_path, bike, &active_label, &target_files);

    // 1) Back up the current set into the library (all-or-nothing).
    if !root_files.is_empty() && !move_set(&root, &backup_dir, &root_files) {
        anyhow::bail!("couldn't back up the current model — is the bike loaded in-game? Exit the bike first.");
    }
    // 2) Move the target's set into the bike root; roll the backup back on failure.
    if !move_set(&target_dir, &root, &target_files) {
        move_set(&backup_dir, &root, &root_files); // restore
        anyhow::bail!("swap failed and was rolled back (see the model files)");
    }

    // Record what each side owns, so the next swap moves exactly this back and never has
    // to guess again.
    write_manifest(&backup_dir, &root_files);
    write_manifest(&target_dir, &target_files);
    write_active(mods_path, bike, target)?;
    Ok(())
}

/// A bike whose setup files (`.hrc`/`.cfg`/`.geom`) were carried off into a swap folder
/// by a version that treated the whole folder as the model set. The game can't see such
/// a bike at all until they're back at the root.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanedSetup {
    pub bike: String,
    /// Filenames missing from the bike root that a parked variant still holds.
    pub files: Vec<String>,
}

fn root_setup_files(mods_path: &str, bike: &str) -> Vec<String> {
    list_files(&bike_dir(mods_path, bike))
        .into_iter()
        .filter(|f| crate::bikefiles::is_bike_setup(f) && !crate::soundmods::is_sound_file(f))
        .collect()
}

/// Setup files a parked variant holds that the bike root is missing, paired with where to
/// find them. Empty when nothing is wrong.
///
/// Gated on the unambiguous signature of the damage: not one `.hrc` at the bike root, so
/// nothing tells the game which mesh each part uses. Without that gate a swap set that
/// legitimately ships its own `.cfg` would look broken. Note this must *not* also require
/// a mesh at the root — swapping to an empty "no model" variant under the old rule left
/// the root with nothing at all, which is the worst case and the one worth catching.
fn orphaned_setup_for(mods_path: &str, bike: &str) -> Vec<(String, PathBuf)> {
    let root = bike_dir(mods_path, bike);
    // A `.pkz` sitting in the bike folder is a packed fallback the loose files layer over,
    // so having no `.hrc` of its own is normal there, not damage.
    if list_files(&root).iter().any(|f| f.to_ascii_lowercase().ends_with(".pkz")) {
        return Vec::new();
    }
    let at_root = root_setup_files(mods_path, bike);
    if at_root.iter().any(|f| f.to_ascii_lowercase().ends_with(".hrc")) {
        return Vec::new();
    }
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(rd) = fs::read_dir(lib_dir(mods_path, bike)) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            for f in set_files(&p) {
                if !crate::bikefiles::is_bike_setup(&f) || crate::soundmods::is_sound_file(&f) {
                    continue;
                }
                if contains_ci(&at_root, &f) || out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&f)) {
                    continue;
                }
                out.push((f.clone(), p.join(&f)));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

pub fn detect_orphaned_setup(mods_path: &str) -> Vec<OrphanedSetup> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(bikes_root(mods_path)) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let bike = match e.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            if bike.starts_with('.') || !is_simple_name(&bike) {
                continue;
            }
            let files: Vec<String> =
                orphaned_setup_for(mods_path, &bike).into_iter().map(|(n, _)| n).collect();
            if !files.is_empty() {
                out.push(OrphanedSetup { bike, files });
            }
        }
    }
    out.sort_by(|a, b| a.bike.to_lowercase().cmp(&b.bike.to_lowercase()));
    out
}

/// Put a gutted bike back together at its root. Deliberately a **copy**: the variant that
/// holds the files keeps its own, so repairing can't break a swap set even if a file
/// turned out to legitimately belong to it. Returns how many files were restored.
///
/// When the root was stripped bare — no mesh either, which is what swapping to an empty
/// variant used to do — the donor variant's whole set comes back and it becomes the active
/// model, so the bike is coherent again rather than setup-without-a-model.
pub fn repair_orphaned_setup(mods_path: &str, bike: &str) -> anyhow::Result<usize> {
    if !is_simple_name(bike) {
        anyhow::bail!("invalid bike name");
    }
    let root = bike_dir(mods_path, bike);
    if !dir_exists(&root) {
        anyhow::bail!("bike '{bike}' not found");
    }
    let missing = orphaned_setup_for(mods_path, bike);
    if missing.is_empty() {
        anyhow::bail!("nothing to restore for '{bike}'");
    }

    // The variant holding most of the bike's setup is the one it came from.
    let donor: Option<PathBuf> = {
        let mut counts: Vec<(PathBuf, usize)> = Vec::new();
        for (_, path) in &missing {
            let dir = match path.parent() {
                Some(d) => d.to_path_buf(),
                None => continue,
            };
            match counts.iter_mut().find(|(p, _)| *p == dir) {
                Some((_, n)) => *n += 1,
                None => counts.push((dir, 1)),
            }
        }
        counts.into_iter().max_by_key(|(_, n)| *n).map(|(p, _)| p)
    };

    let mut restore: Vec<(String, PathBuf)> = missing;
    let stripped = !crate::bikefiles::dir_has_mesh(&root);
    if stripped {
        if let Some(d) = &donor {
            for f in set_files(d) {
                if !restore.iter().any(|(n, _)| n.eq_ignore_ascii_case(&f)) {
                    restore.push((f.clone(), d.join(&f)));
                }
            }
        }
    }

    let mut restored = 0usize;
    for (name, src) in &restore {
        let dst = root.join(name);
        if dst.exists() {
            continue;
        }
        if fs::copy(src, &dst).is_ok() {
            restored += 1;
        }
    }
    if restored == 0 {
        anyhow::bail!("nothing to restore for '{bike}'");
    }
    // The root now mirrors the donor, so say so — otherwise the next swap would park the
    // restored files under whatever name the stale marker happens to hold.
    if stripped {
        if let Some(name) = donor
            .as_ref()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
        {
            write_active(mods_path, bike, name)?;
        }
    }
    Ok(restored)
}

const KIND_MODEL: &str = "model";
const KIND_SOUND: &str = "sound";

/// Classify a loose folder as a swappable set: a mesh set — any `.edf`, since a bike may
/// ship one per part (models win when a folder somehow has both) — or a complete
/// `engine.scl` + `sfx.cfg` sound set. Anything else (liveries, screenshots, junk) is
/// `None` and ignored.
fn classify_set(p: &Path) -> Option<&'static str> {
    if crate::bikefiles::dir_has_mesh(p) {
        Some(KIND_MODEL)
    } else if crate::soundmods::is_sound_set(p) {
        Some(KIND_SOUND)
    } else {
        None
    }
}

/// The library folder a candidate of this kind registers into.
fn kind_lib_dir(mods_path: &str, bike: &str, kind: &str) -> PathBuf {
    if kind == KIND_SOUND {
        bike_dir(mods_path, bike).join(crate::soundmods::SOUND_LIB_DIR)
    } else {
        lib_dir(mods_path, bike)
    }
}

/// True for a bike-dir child we must never treat as a loose set: either swap library
/// (`FrostMod Models` / `FrostMod Sounds`), the paints (livery) folder, or a hidden
/// dotfolder.
fn is_reserved_child(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case(LIB_DIR)
        || name.eq_ignore_ascii_case(crate::soundmods::SOUND_LIB_DIR)
        || name.eq_ignore_ascii_case("paints")
}

fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if let Some(n) = e.file_name().to_str() {
                out.push((n.to_string(), p));
            }
        }
    }
    out
}

/// Move a whole directory: try a fast rename, then fall back to a recursive copy +
/// remove (handles cross-volume). Refuses to overwrite an existing destination.
fn move_dir(src: &Path, dst: &Path) -> bool {
    if dst.exists() {
        return false;
    }
    if fs::rename(src, dst).is_ok() {
        return true;
    }
    if copy_tree(src, dst).is_ok() && fs::remove_dir_all(src).is_ok() {
        return true;
    }
    let _ = fs::remove_dir_all(dst); // don't leave a half-copied dir behind
    false
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)?.flatten() {
        let from = e.path();
        let to = dst.join(e.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn scan_loose_candidates(mods_path: &str, bike: &str) -> Vec<LooseSwapCandidate> {
    let root = bike_dir(mods_path, bike);
    let mut out: Vec<LooseSwapCandidate> = Vec::new();
    for (name, path) in subdirs(&root) {
        if is_reserved_child(&name) || !is_simple_name(&name) {
            continue;
        }
        if let Some(kind) = classify_set(&path) {
            // A model or sound set dropped straight into the bike dir.
            out.push(LooseSwapCandidate {
                file_count: list_files(&path).len(),
                source: name.clone(),
                kind: kind.to_string(),
                name,
            });
        } else {
            // Not a set itself — treat it as a container (e.g. `models/`, `sounds/`) and
            // look one level down for variant folders.
            for (child, child_path) in subdirs(&path) {
                if child.starts_with('.') || !is_simple_name(&child) {
                    continue;
                }
                if let Some(kind) = classify_set(&child_path) {
                    out.push(LooseSwapCandidate {
                        file_count: list_files(&child_path).len(),
                        source: format!("{name}/{child}"),
                        kind: kind.to_string(),
                        name: child,
                    });
                }
            }
        }
    }
    // Group by kind, then by name, so the dialog lists models and sounds tidily.
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// Scan every bike for model- and sound-set folders sitting outside their library
/// (`FrostMod Models/` / `FrostMod Sounds/`), so we can offer to register them. Only
/// bikes with at least one candidate are returned.
pub fn detect_loose_swaps(mods_path: &str) -> Vec<LooseSwapBike> {
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
            if bike.starts_with('.') || !is_simple_name(&bike) {
                continue;
            }
            let candidates = scan_loose_candidates(mods_path, &bike);
            if !candidates.is_empty() {
                out.push(LooseSwapBike { bike, candidates });
            }
        }
    }
    out.sort_by(|a, b| a.bike.to_lowercase().cmp(&b.bike.to_lowercase()));
    out
}

/// Act on the loose swaps found by [`detect_loose_swaps`]. With `move_files`, each
/// candidate folder is moved into its kind's library — a model set into
/// `FrostMod Models/<name>/`, a sound set into `FrostMod Sounds/<name>/` — skipping any
/// whose name is already taken there. Without it, we only create the relevant library
/// folder(s) for each affected bike and leave the files in place.
pub fn register_loose_swaps(mods_path: &str, move_files: bool) -> anyhow::Result<RegisterReport> {
    let mut report = RegisterReport::default();
    for bike_info in detect_loose_swaps(mods_path) {
        let bike = &bike_info.bike;
        report.bikes += 1;

        // Create the library folder for each kind of set this bike has loose.
        for kind in [KIND_MODEL, KIND_SOUND] {
            if bike_info.candidates.iter().any(|c| c.kind == kind) {
                let lib = kind_lib_dir(mods_path, bike, kind);
                let existed = lib.is_dir();
                fs::create_dir_all(&lib)?;
                if !existed {
                    report.folders_created += 1;
                }
            }
        }

        if !move_files {
            continue;
        }

        for c in bike_info.candidates {
            if !is_simple_name(&c.name) {
                report.skipped += 1;
                continue;
            }
            let dst = kind_lib_dir(mods_path, bike, &c.kind).join(&c.name);
            if dst.exists() {
                report.skipped += 1; // name already registered — don't clobber
                continue;
            }
            let src = bike_dir(mods_path, bike).join(&c.source);
            if move_dir(&src, &dst) {
                report.registered += 1;
                // If the candidate lived in a container folder that's now empty, tidy it.
                if let Some(parent) = src.parent() {
                    if parent != bike_dir(mods_path, bike).as_path()
                        && subdirs(parent).is_empty()
                        && list_files(parent).is_empty()
                    {
                        let _ = fs::remove_dir(parent);
                    }
                }
            } else {
                report.skipped += 1;
            }
        }
    }
    Ok(report)
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
    fn file_exists(p: &Path) -> bool {
        p.is_file()
    }

    /// A realistic extracted bike: mesh + the setup files the game needs to see it.
    fn make_bike(mp: &str, bike: &str, mesh: &str) {
        touch(&bike_dir(mp, bike).join(mesh));
        touch(&bike_dir(mp, bike).join("chassis.hrc"));
        touch(&bike_dir(mp, bike).join("bike.cfg"));
        touch(&bike_dir(mp, bike).join("wheel.geom"));
    }
    fn names_at(p: &Path) -> Vec<String> {
        let mut v = list_files(p);
        v.sort();
        v
    }

    #[test]
    fn swap_leaves_the_bikes_setup_files_at_the_root() {
        // The 0.6.1 bug: every loose root file was parked, so the game lost the bike.
        let root = tmp("keeps-setup");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        touch(&bike_dir(mp, "KTM450").join("body.tga"));
        touch(&variant_dir(mp, "KTM450", "Factory").join("model.edf"));

        apply_model_swap(mp, "KTM450", "Factory").unwrap();

        let at_root = names_at(&bike_dir(mp, "KTM450"));
        for keep in ["chassis.hrc", "bike.cfg", "wheel.geom"] {
            assert!(at_root.contains(&keep.to_string()), "{keep} must stay: {at_root:?}");
        }
        assert!(at_root.contains(&"model.edf".to_string()), "the new mesh arrived");
        let parked = names_at(&variant_dir(mp, "KTM450", ORIGINAL));
        assert!(parked.contains(&"model.edf".to_string()), "old mesh parked");
        assert!(!parked.contains(&"chassis.hrc".to_string()), "setup never parked");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn swap_round_trip_restores_the_original_set() {
        let root = tmp("round-trip");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        fs::write(bike_dir(mp, "KTM450").join("model.edf"), b"stock").unwrap();
        touch(&variant_dir(mp, "KTM450", "Factory").join("model.edf"));
        touch(&variant_dir(mp, "KTM450", "Factory").join("factory.tga"));

        let before = names_at(&bike_dir(mp, "KTM450"));
        apply_model_swap(mp, "KTM450", "Factory").unwrap();
        apply_model_swap(mp, "KTM450", ORIGINAL).unwrap();

        assert_eq!(names_at(&bike_dir(mp, "KTM450")), before, "root is back to stock");
        assert_eq!(
            fs::read(bike_dir(mp, "KTM450").join("model.edf")).unwrap(),
            b"stock",
            "the original mesh came back, not the swap's"
        );
        assert_eq!(
            names_at(&variant_dir(mp, "KTM450", "Factory")),
            vec!["_files.txt", "factory.tga", "model.edf"],
            "the swap is parked whole again"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn per_part_edf_bike_scans_and_swaps() {
        // A bike whose mesh is split per part has no `model.edf` at all — it used to be
        // invisible to the Locker and rejected on apply.
        let root = tmp("per-part");
        let mp = root.to_str().unwrap();
        make_bike(mp, "CR250", "96cr250.edf");
        touch(&bike_dir(mp, "CR250").join("96cr250_st.edf"));
        touch(&variant_dir(mp, "CR250", "OEM").join("96cr250.edf"));

        let bikes = scan_model_swaps(mp);
        assert_eq!(bikes.len(), 1, "per-part bike lists");
        assert!(bikes[0].variants.iter().all(|v| v.valid), "both sets are valid");

        apply_model_swap(mp, "CR250", "OEM").unwrap();
        let at_root = names_at(&bike_dir(mp, "CR250"));
        assert!(at_root.contains(&"chassis.hrc".to_string()));
        // Every root mesh is part of the model set, including the part the swap omits.
        assert!(!at_root.contains(&"96cr250_st.edf".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn loose_per_part_set_is_detected_for_registration() {
        let root = tmp("loose-per-part");
        let mp = root.to_str().unwrap();
        make_bike(mp, "CR250", "96cr250.edf");
        touch(&bike_dir(mp, "CR250").join("OEM Replica").join("96cr250.edf"));

        let found = detect_loose_swaps(mp);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].candidates[0].name, "OEM Replica");
        assert_eq!(found[0].candidates[0].kind, KIND_MODEL);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn set_with_files_but_no_mesh_is_rejected() {
        let root = tmp("no-mesh");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        touch(&variant_dir(mp, "KTM450", "Broken").join("readme.txt"));

        let err = apply_model_swap(mp, "KTM450", "Broken").unwrap_err().to_string();
        assert!(err.contains("no mesh"), "got: {err}");
        assert!(
            bike_dir(mp, "KTM450").join("chassis.hrc").is_file(),
            "a rejected swap touches nothing"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_and_repairs_a_bike_gutted_by_an_older_version() {
        let root = tmp("repair");
        let mp = root.to_str().unwrap();
        // Reproduce the damage: setup files sitting in the swap folder, missing at root.
        touch(&bike_dir(mp, "KTM450").join("model.edf"));
        for f in ["chassis.hrc", "bike.cfg", "wheel.geom"] {
            touch(&variant_dir(mp, "KTM450", ORIGINAL).join(f));
        }
        touch(&variant_dir(mp, "KTM450", ORIGINAL).join("model.edf"));

        let found = detect_orphaned_setup(mp);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bike, "KTM450");
        assert_eq!(found[0].files, vec!["bike.cfg", "chassis.hrc", "wheel.geom"]);

        assert_eq!(repair_orphaned_setup(mp, "KTM450").unwrap(), 3);
        for f in ["chassis.hrc", "bike.cfg", "wheel.geom"] {
            assert!(bike_dir(mp, "KTM450").join(f).is_file(), "{f} restored");
            assert!(
                variant_dir(mp, "KTM450", ORIGINAL).join(f).is_file(),
                "{f} left in the variant too — repair copies, it can't break a set"
            );
        }
        assert!(detect_orphaned_setup(mp).is_empty(), "nothing left to repair");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_healthy_bike_is_not_flagged_for_repair() {
        let root = tmp("healthy");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        // A swap that legitimately ships its own hrc, while the root has one too.
        touch(&variant_dir(mp, "KTM450", "Factory").join("model.edf"));
        touch(&variant_dir(mp, "KTM450", "Factory").join("chassis.hrc"));
        // And one that ships a cfg the base bike never had — not damage, just a set.
        touch(&variant_dir(mp, "KTM450", "Loud").join("model.edf"));
        touch(&variant_dir(mp, "KTM450", "Loud").join("extra.cfg"));

        assert!(detect_orphaned_setup(mp).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repairs_a_bike_stripped_bare_by_a_no_model_swap() {
        // The state found in a real install: swapping to an empty variant under the old
        // rule left the bike root with nothing but `paints/`, the whole bike parked under
        // `Original`, and the marker pointing at the empty variant.
        let root = tmp("stripped");
        let mp = root.to_str().unwrap();
        fs::create_dir_all(bike_dir(mp, "KTM450").join("paints")).unwrap();
        for f in ["model.edf", "gfx.cfg", "chassis.hrc", "fsusp.hrc", "rsusp.hrc", "steer.hrc"] {
            touch(&variant_dir(mp, "KTM450", ORIGINAL).join(f));
        }
        fs::create_dir_all(variant_dir(mp, "KTM450", "new model")).unwrap();
        write_active(mp, "KTM450", "new model").unwrap();

        let found = detect_orphaned_setup(mp);
        assert_eq!(found.len(), 1, "a bike with nothing at its root is the worst case");
        assert_eq!(found[0].bike, "KTM450");

        assert_eq!(repair_orphaned_setup(mp, "KTM450").unwrap(), 6, "whole set restored");
        let at_root = names_at(&bike_dir(mp, "KTM450"));
        assert!(crate::bikefiles::dir_has_mesh(&bike_dir(mp, "KTM450")), "mesh is back");
        for f in ["chassis.hrc", "fsusp.hrc", "rsusp.hrc", "steer.hrc", "gfx.cfg"] {
            assert!(at_root.contains(&f.to_string()), "{f} restored: {at_root:?}");
        }
        assert_eq!(read_active(mp, "KTM450"), ORIGINAL, "the marker matches what's at root");
        assert!(detect_orphaned_setup(mp).is_empty(), "nothing left to repair");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_packed_bike_with_swaps_is_not_flagged_for_repair() {
        // No loose model at the root: the pkz provides it, so a missing hrc is normal.
        let root = tmp("packed-not-flagged");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "RM").join("RM.pkz"));
        touch(&variant_dir(mp, "RM", "Factory").join("model.edf"));
        touch(&variant_dir(mp, "RM", "Factory").join("chassis.hrc"));

        assert!(detect_orphaned_setup(mp).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_swaps_own_setup_file_is_displaced_and_restored() {
        let root = tmp("swap-owns-hrc");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        fs::write(bike_dir(mp, "KTM450").join("chassis.hrc"), b"stock-hrc").unwrap();
        touch(&variant_dir(mp, "KTM450", "Factory").join("model.edf"));
        fs::write(variant_dir(mp, "KTM450", "Factory").join("chassis.hrc"), b"factory-hrc")
            .unwrap();

        apply_model_swap(mp, "KTM450", "Factory").unwrap();
        assert_eq!(
            fs::read(bike_dir(mp, "KTM450").join("chassis.hrc")).unwrap(),
            b"factory-hrc",
            "the swap's own hrc wins while it is active"
        );
        apply_model_swap(mp, "KTM450", ORIGINAL).unwrap();
        assert_eq!(
            fs::read(bike_dir(mp, "KTM450").join("chassis.hrc")).unwrap(),
            b"stock-hrc",
            "and the bike's own comes back"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Read-only sweep of a real MX Bikes folder: what the Locker would list, what it
    /// would offer to register, and whether anything looks gutted. Touches nothing.
    ///
    /// MXB_REAL_BIKES=~/Projects/PiBoSo/"MX Bikes" \
    ///   cargo test real_mods_tree_discovery -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_mods_tree_discovery() {
        let Ok(mp) = std::env::var("MXB_REAL_BIKES") else {
            eprintln!("set MXB_REAL_BIKES to the MX Bikes folder to run");
            return;
        };
        let bikes = scan_model_swaps(&mp);
        eprintln!("-- swappable bikes: {}", bikes.len());
        for b in &bikes {
            let vs: Vec<String> = b
                .variants
                .iter()
                .map(|v| {
                    format!(
                        "{}{}{} ({} files)",
                        v.name,
                        if v.active { "*" } else { "" },
                        if v.valid { "" } else { " !no-mesh" },
                        v.file_count
                    )
                })
                .collect();
            eprintln!("   {} -> {}", b.bike, vs.join(", "));
        }
        eprintln!("-- loose sets offered for registration:");
        for b in detect_loose_swaps(&mp) {
            for c in &b.candidates {
                eprintln!("   {}/{} [{}] {} files", b.bike, c.source, c.kind, c.file_count);
            }
        }
        eprintln!("-- bikes flagged as gutted: {:?}", detect_orphaned_setup(&mp));
    }

    /// Repairs a real damaged bike, on a copy. Skips cleanly when the tree is healthy.
    ///
    /// MXB_REAL_BIKES=~/Documents/PiBoSo/"MX Bikes" \
    ///   cargo test real_gutted_bike_is_repaired -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_gutted_bike_is_repaired() {
        let Ok(src_root) = std::env::var("MXB_REAL_BIKES") else { return };
        let Some(flagged) = detect_orphaned_setup(&src_root).into_iter().next() else {
            eprintln!("no damaged bike in this tree — nothing to repair");
            return;
        };
        eprintln!("repairing real bike: {} (missing {:?})", flagged.bike, flagged.files);

        let root = tmp("real-repair");
        let mp = root.to_str().unwrap();
        copy_tree(&bike_dir(&src_root, &flagged.bike), &bike_dir(mp, &flagged.bike))
            .expect("copy the damaged bike");
        eprintln!("root before: {:?}", names_at(&bike_dir(mp, &flagged.bike)));

        let n = repair_orphaned_setup(mp, &flagged.bike).expect("repair runs");
        eprintln!("restored {n} files");
        eprintln!("root after:  {:?}", names_at(&bike_dir(mp, &flagged.bike)));
        eprintln!("active is now: {:?}", read_active(mp, &flagged.bike));

        assert!(
            detect_orphaned_setup(mp).is_empty(),
            "the bike is no longer flagged as damaged"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// End-to-end against a **real** extracted bike, because the whole defect was a wrong
    /// assumption about what a real bike folder contains. Copies the bike into a scratch
    /// mods tree, swaps it, and asserts the bike still resolves through the app's own
    /// loader — `.hrc` → scene → `.edf`, the same chain the game walks. Under the old
    /// "park every loose file" rule this fails: the `.hrc`s end up in the swap folder and
    /// `gather_bike_files` can't find a bike at all.
    ///
    /// MXB_REAL_BIKES=~/Projects/PiBoSo/"MX Bikes" \
    ///   cargo test real_bike_survives_a_model_swap -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_bike_survives_a_model_swap() {
        let Ok(src_root) = std::env::var("MXB_REAL_BIKES") else {
            eprintln!("set MXB_REAL_BIKES to the MX Bikes folder to run");
            return;
        };
        let src_bikes = Path::new(&src_root).join("mods").join("bikes");
        // Any extracted bike with a mesh and at least one .hrc will do.
        let (bike, src_dir) = subdirs(&src_bikes)
            .into_iter()
            .find(|(_, p)| {
                crate::bikefiles::dir_has_mesh(p)
                    && list_files(p).iter().any(|f| f.to_ascii_lowercase().ends_with(".hrc"))
            })
            .expect("no extracted bike with a mesh + .hrc found");
        eprintln!("using real bike: {bike}");

        let root = tmp("real-bike");
        let mp = root.to_str().unwrap();
        let dst = bike_dir(mp, &bike);
        copy_tree(&src_dir, &dst).expect("copy bike");
        // A realistic swap set: an alternate mesh, nothing else.
        let mesh = list_files(&dst)
            .into_iter()
            .find(|f| crate::bikefiles::is_mesh(f))
            .expect("mesh");
        let variant = variant_dir(mp, &bike, "Factory");
        fs::create_dir_all(&variant).unwrap();
        fs::copy(dst.join(&mesh), variant.join(&mesh)).expect("copy mesh into the variant");

        let before = names_at(&dst);
        eprintln!("root before: {before:?}");
        let nodes_before = crate::load_bike_model_blocking(dst.to_string_lossy().to_string())
            .expect("the bike loads before the swap")
            .nodes
            .len();
        assert!(nodes_before > 0, "loader produced no nodes before the swap");

        apply_model_swap(mp, &bike, "Factory").expect("swap applies");

        let after = names_at(&dst);
        eprintln!("root after:  {after:?}");
        for f in before.iter().filter(|f| crate::bikefiles::is_bike_setup(f)) {
            assert!(after.contains(f), "{f} must still be at the bike root after a swap");
        }
        let model =
            crate::load_bike_model_blocking(dst.to_string_lossy().to_string())
                .expect("the bike still loads after the swap");
        assert_eq!(model.nodes.len(), nodes_before, "same parts resolve after the swap");

        apply_model_swap(mp, &bike, ORIGINAL).expect("swap back");
        assert_eq!(names_at(&dst), before, "swapping back restores the bike folder exactly");

        let _ = fs::remove_dir_all(&root);
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
    fn model_swap_leaves_the_sound_set_at_the_root() {
        // The engine sound is swapped independently — a model swap must NOT drag the
        // loose sound files into the model backup.
        let root = tmp("keep-sound");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("engine.scl"));
        touch(&bike_dir(mp, "KTM").join("sfx.cfg"));
        touch(&bike_dir(mp, "KTM").join("idle.wav"));
        touch(&variant_dir(mp, "KTM", "Factory").join("model.edf"));

        apply_model_swap(mp, "KTM", "Factory").unwrap();

        // Sound files stay loose at the bike root...
        assert!(file_exists(&bike_dir(mp, "KTM").join("engine.scl")));
        assert!(file_exists(&bike_dir(mp, "KTM").join("sfx.cfg")));
        assert!(file_exists(&bike_dir(mp, "KTM").join("idle.wav")));
        // ...and never land in the model's Original backup.
        assert!(!file_exists(&variant_dir(mp, "KTM", "Original").join("engine.scl")));
        assert!(!file_exists(&variant_dir(mp, "KTM", "Original").join("idle.wav")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_empty_variant_removes_the_model() {
        let root = tmp("empty-swap");
        let mp = root.to_str().unwrap();
        // Original loose set with a model.
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("KTM.cfg"));
        // An intentional empty "No model" variant folder (no files).
        fs::create_dir_all(&variant_dir(mp, "KTM", "No model")).unwrap();

        // The empty variant is applicable, unlike a files-but-no-edf set.
        apply_model_swap(mp, "KTM", "No model").unwrap();

        // Marker names the empty variant; the bike root now has no model files.
        assert_eq!(read_active(mp, "KTM"), "No model");
        assert!(!file_exists(&bike_dir(mp, "KTM").join("model.edf")));
        // The Original set was parked in the library.
        assert!(file_exists(&variant_dir(mp, "KTM", "Original").join("model.edf")));

        // The scan flags it empty (and therefore selectable) while it's active.
        let bikes = scan_model_swaps(mp);
        let active = bikes[0].variants.iter().find(|v| v.active).unwrap();
        assert!(active.empty && !active.valid);

        // And it swaps back cleanly.
        apply_model_swap(mp, "KTM", "Original").unwrap();
        assert!(file_exists(&bike_dir(mp, "KTM").join("model.edf")));
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

    #[test]
    fn detects_loose_variants_at_root_and_in_a_container() {
        let root = tmp("detect");
        let mp = root.to_str().unwrap();
        // Active loose set (root-level model.edf) — never a candidate.
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("KTM.cfg"));
        // paints/ is reserved and must be ignored.
        touch(&bike_dir(mp, "KTM").join("paints").join("Red.pnt"));
        // A variant dropped straight into the bike dir.
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("KTM.cfg"));
        // A container folder holding another variant one level down.
        touch(&bike_dir(mp, "KTM").join("models").join("Race Kit").join("model.edf"));
        // A folder without a model.edf is not a model set — ignored.
        touch(&bike_dir(mp, "KTM").join("screenshots").join("shot.png"));

        let found = detect_loose_swaps(mp);
        assert_eq!(found.len(), 1);
        let names: Vec<_> = found[0].candidates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Factory OEM", "Race Kit"]); // sorted, no screenshots
        let race = found[0].candidates.iter().find(|c| c.name == "Race Kit").unwrap();
        assert_eq!(race.source, "models/Race Kit");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn already_registered_bikes_report_nothing() {
        let root = tmp("detect-clean");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        // Variants already under the library — nothing loose.
        touch(&variant_dir(mp, "KTM", "Factory").join("model.edf"));
        assert!(detect_loose_swaps(mp).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn register_moves_loose_sets_into_the_library() {
        let root = tmp("register-move");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("models").join("Race Kit").join("model.edf"));

        let rep = register_loose_swaps(mp, true).unwrap();
        assert_eq!(rep.bikes, 1);
        assert_eq!(rep.registered, 2);
        assert_eq!(rep.skipped, 0);
        assert_eq!(rep.folders_created, 1);

        // Both sets now live under FrostMod Models/ and the loose copies are gone.
        assert!(file_exists(&variant_dir(mp, "KTM", "Factory OEM").join("model.edf")));
        assert!(file_exists(&variant_dir(mp, "KTM", "Race Kit").join("model.edf")));
        assert!(!dir_exists(&bike_dir(mp, "KTM").join("Factory OEM")));
        // The now-empty container folder was tidied away.
        assert!(!dir_exists(&bike_dir(mp, "KTM").join("models")));

        // The Locker scan now sees them, and nothing loose remains.
        let names: Vec<_> = scan_model_swaps(mp)[0]
            .variants
            .iter()
            .map(|v| v.name.clone())
            .collect();
        assert!(names.contains(&"Factory OEM".to_string()));
        assert!(names.contains(&"Race Kit".to_string()));
        assert!(detect_loose_swaps(mp).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn register_without_move_only_creates_the_folder() {
        let root = tmp("register-nomove");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf"));

        let rep = register_loose_swaps(mp, false).unwrap();
        assert_eq!(rep.bikes, 1);
        assert_eq!(rep.registered, 0);
        assert_eq!(rep.folders_created, 1);

        // The library folder now exists, but the loose set is untouched (still detected).
        assert!(dir_exists(&lib_dir(mp, "KTM")));
        assert!(file_exists(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf")));
        assert_eq!(detect_loose_swaps(mp).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn register_skips_names_already_in_the_library() {
        let root = tmp("register-collide");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        // A loose "Factory" set collides with an existing library variant of the same name.
        touch(&bike_dir(mp, "KTM").join("Factory").join("model.edf"));
        touch(&variant_dir(mp, "KTM", "Factory").join("model.edf"));

        let rep = register_loose_swaps(mp, true).unwrap();
        assert_eq!(rep.registered, 0);
        assert_eq!(rep.skipped, 1);
        // The existing library variant is left intact and the loose one stays put.
        assert!(file_exists(&variant_dir(mp, "KTM", "Factory").join("model.edf")));
        assert!(file_exists(&bike_dir(mp, "KTM").join("Factory").join("model.edf")));
        let _ = fs::remove_dir_all(&root);
    }

    // A complete loose sound set (both must-files) at `dir`.
    fn touch_sound(dir: &Path) {
        touch(&dir.join("engine.scl"));
        touch(&dir.join("sfx.cfg"));
    }
    fn sound_dir(mods_path: &str, bike: &str, name: &str) -> PathBuf {
        bike_dir(mods_path, bike).join("FrostMod Sounds").join(name)
    }

    #[test]
    fn detects_loose_sound_sets_alongside_models() {
        let root = tmp("detect-sound");
        let mp = root.to_str().unwrap();
        // Active loose model + sound at the bike root — never candidates.
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch_sound(&bike_dir(mp, "KTM"));
        // A loose model set and a loose sound set dropped at the bike root.
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf"));
        touch_sound(&bike_dir(mp, "KTM").join("Braaap"));
        // A sound set inside a `sounds/` container, one level down.
        touch_sound(&bike_dir(mp, "KTM").join("sounds").join("FourStroke"));
        // A folder with a lone sfx.cfg (missing engine.scl) is incomplete — ignored.
        touch(&bike_dir(mp, "KTM").join("Half").join("sfx.cfg"));

        let found = detect_loose_swaps(mp);
        assert_eq!(found.len(), 1);
        let cands = &found[0].candidates;
        // Grouped model-first, then sounds — each tagged with its kind + source.
        let model: Vec<_> = cands.iter().filter(|c| c.kind == "model").map(|c| c.name.as_str()).collect();
        let sound: Vec<_> = cands.iter().filter(|c| c.kind == "sound").map(|c| c.name.as_str()).collect();
        assert_eq!(model, vec!["Factory OEM"]);
        assert_eq!(sound, vec!["Braaap", "FourStroke"]);
        let four = cands.iter().find(|c| c.name == "FourStroke").unwrap();
        assert_eq!(four.source, "sounds/FourStroke");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn register_moves_sound_sets_into_frostmod_sounds() {
        let root = tmp("register-sound");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch_sound(&bike_dir(mp, "KTM").join("Braaap"));
        touch(&bike_dir(mp, "KTM").join("Braaap").join("idle.wav"));
        touch_sound(&bike_dir(mp, "KTM").join("sounds").join("FourStroke"));
        // Plus a loose model set, to prove both kinds route to the right library.
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf"));

        let rep = register_loose_swaps(mp, true).unwrap();
        assert_eq!(rep.registered, 3); // 2 sounds + 1 model
        assert_eq!(rep.skipped, 0);
        assert_eq!(rep.folders_created, 2); // FrostMod Models + FrostMod Sounds

        // Sounds landed under FrostMod Sounds/, the model under FrostMod Models/.
        assert!(file_exists(&sound_dir(mp, "KTM", "Braaap").join("engine.scl")));
        assert!(file_exists(&sound_dir(mp, "KTM", "Braaap").join("idle.wav")));
        assert!(file_exists(&sound_dir(mp, "KTM", "FourStroke").join("sfx.cfg")));
        assert!(file_exists(&variant_dir(mp, "KTM", "Factory OEM").join("model.edf")));
        // The `sounds/` container was tidied once emptied.
        assert!(!dir_exists(&bike_dir(mp, "KTM").join("sounds")));

        // The sound scanner now sees them, and nothing loose remains.
        let names: Vec<_> = crate::soundmods::scan_sound_swaps(mp)[0]
            .variants
            .iter()
            .map(|v| v.name.clone())
            .collect();
        assert!(names.contains(&"Braaap".to_string()));
        assert!(names.contains(&"FourStroke".to_string()));
        assert!(detect_loose_swaps(mp).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn register_folders_only_creates_both_libraries() {
        let root = tmp("register-sound-nomove");
        let mp = root.to_str().unwrap();
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        touch(&bike_dir(mp, "KTM").join("Factory OEM").join("model.edf"));
        touch_sound(&bike_dir(mp, "KTM").join("Braaap"));

        let rep = register_loose_swaps(mp, false).unwrap();
        assert_eq!(rep.registered, 0);
        assert_eq!(rep.folders_created, 2); // both libraries created
        assert!(dir_exists(&lib_dir(mp, "KTM")));
        assert!(dir_exists(&bike_dir(mp, "KTM").join("FrostMod Sounds")));
        // Files untouched — still detected.
        assert!(file_exists(&bike_dir(mp, "KTM").join("Braaap").join("engine.scl")));
        assert_eq!(detect_loose_swaps(mp).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }
}

