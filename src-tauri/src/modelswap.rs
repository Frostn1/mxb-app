use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const LIB_DIR: &str = "FrostMod Models";
const MARKER: &str = "_active.txt";
const ORIGINAL: &str = "Original";
/// The game's own model, inside the bike's `.pkz`. Never a folder — it's reached by
/// clearing the loose set so the packed one takes over again.
const STOCK: &str = "Stock";
/// Per-variant record of the filenames that variant owns, written whenever we park a
/// set. Lets the reverse swap move back exactly what it moved out instead of guessing.
const MANIFEST: &str = "_files.txt";
/// Which liveries each model variant owns: variant name -> livery base names (no `.pnt`).
/// The game has no notion of a model swap — every livery must sit in the one flat
/// `<Bike>/paints/` folder — so ownership can only live beside the swaps themselves.
const PAINT_ASSIGN: &str = "_paints.json";
/// Where a livery waits while the model it belongs to is *not* active. Out of
/// `<Bike>/paints/` means out of the game's paint list too, which is the point. One shelf
/// per bike rather than one per variant, so a livery owned by two models has one home.
pub const PAINT_SHELF: &str = "_paints";
const PNT_EXT: &str = ".pnt";

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
    /// Liveries assigned to this variant, by base name. Empty means "no opinion" — the
    /// bike's unassigned liveries are offered under every model.
    pub paints: Vec<String>,
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
fn paints_dir(mods_path: &str, bike: &str) -> PathBuf {
    bike_dir(mods_path, bike).join("paints")
}
fn shelf_dir(mods_path: &str, bike: &str) -> PathBuf {
    lib_dir(mods_path, bike).join(PAINT_SHELF)
}

/// The shelf lives inside `FrostMod Models/` but is not a model set. Every walk over that
/// folder's children has to say so, or it reads as a variant named `_paints`.
fn is_shelf(name: &str) -> bool {
    name.eq_ignore_ascii_case(PAINT_SHELF)
}

use crate::library::is_simple_name;

fn read_active(mods_path: &str, bike: &str) -> String {
    fs::read_to_string(lib_dir(mods_path, bike).join(MARKER))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub const ORIGINAL_LABEL: &str = ORIGINAL;
pub const STOCK_LABEL: &str = STOCK;

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

/// True if the bike ships a `.pkz` — a packed model the loose files layer over. It's what
/// makes "no loose model" still mean *a* model, and so the only case where reverting to
/// the game's own model is possible at all.
fn has_packed_fallback(bike_dir: &Path) -> bool {
    list_files(bike_dir).iter().any(|f| f.to_ascii_lowercase().ends_with(".pkz"))
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
            if is_shelf(&name) || except.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
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

// ---------------------------------------------------------------------------
// Livery ownership
//
// A bike's liveries all have to live in one flat `<Bike>/paints/` folder — that is the
// game's rule, and it knows nothing about model swaps. So a Yami model swapped onto a KTM
// shows the Yami liveries and the KTM ones side by side, most of them wrong for whatever
// mesh is currently on the bike.
//
// `_paints.json` records which liveries each variant owns. A livery no variant claims is
// unassigned and stays on offer under every model — so a tree with no assignments behaves
// exactly as it did before. `reconcile_paints` then makes the folder agree with the
// record: liveries owned by some *other* model move to the shelf, the active model's move
// back. The game lists what it finds, so shelving is what filters it in-game too.
//
// Ownership is a record rather than a location on purpose: it lets two models claim the
// same livery without a second copy of it on disk, and it lets `Stock` — which never has
// a folder at all (see `STOCK`) — own liveries like any other variant.
// ---------------------------------------------------------------------------

/// variant name -> the liveries it owns, by base name (no `.pnt`).
pub type PaintAssignments = BTreeMap<String, Vec<String>>;

fn assign_path(mods_path: &str, bike: &str) -> PathBuf {
    lib_dir(mods_path, bike).join(PAINT_ASSIGN)
}

pub fn load_paint_assignments(mods_path: &str, bike: &str) -> PaintAssignments {
    match fs::read_to_string(assign_path(mods_path, bike)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => PaintAssignments::new(),
    }
}

fn save_paint_assignments(
    mods_path: &str,
    bike: &str,
    assignments: &PaintAssignments,
) -> anyhow::Result<()> {
    if assignments.is_empty() {
        let _ = fs::remove_file(assign_path(mods_path, bike));
        return Ok(());
    }
    let lib = lib_dir(mods_path, bike);
    fs::create_dir_all(&lib)?;
    fs::write(assign_path(mods_path, bike), serde_json::to_string_pretty(assignments)?)?;
    Ok(())
}

fn strip_pnt(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("pnt") => stem,
        _ => name,
    }
}

/// The `.pnt` files directly in `dir`, as base names.
fn liveries_in(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = list_files(dir)
        .into_iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(PNT_EXT))
        .map(|f| strip_pnt(&f).to_string())
        .collect();
    out.sort_by_key(|s| s.to_lowercase());
    out
}

/// Every livery the bike owns, wherever it currently sits — the loose `paints/` folder and
/// the shelf both count, so assigning one doesn't make it disappear from the picker that
/// assigns it.
pub fn bike_liveries(mods_path: &str, bike: &str) -> Vec<String> {
    let mut out = liveries_in(&paints_dir(mods_path, bike));
    for name in liveries_in(&shelf_dir(mods_path, bike)) {
        if !contains_ci(&out, &name) {
            out.push(name);
        }
    }
    out.sort_by_key(|s| s.to_lowercase());
    out
}

/// The real filename of livery `base` inside `dir`, matched case-insensitively — the
/// record stores what the user sees, the disk stores whatever the mod author typed.
fn livery_file(dir: &Path, base: &str) -> Option<String> {
    list_files(dir).into_iter().find(|f| {
        f.to_ascii_lowercase().ends_with(PNT_EXT) && strip_pnt(f).eq_ignore_ascii_case(base)
    })
}

/// Move a livery between the loose folder and the shelf, refusing to write over one that
/// is already there. Two different `.pnt`s can share a base name — one drawn for the Yami
/// and one for the KTM, both called `Redbud` — and `move_one` renames, which would destroy
/// the one at the destination. Leaving it put keeps both files.
fn move_livery(from: &Path, to: &Path, file: &str) -> bool {
    if to.join(file).exists() {
        return false;
    }
    fs::create_dir_all(to).is_ok() && move_one(&from.join(file), &to.join(file))
}

/// The variant folders under `FrostMod Models/`, shelf excluded.
fn variant_names(mods_path: &str, bike: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(lib_dir(mods_path, bike)) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            if let Some(n) = e.file_name().to_str() {
                if !is_shelf(n) {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort_by_key(|s| s.to_lowercase());
    out
}

/// Take ownership of liveries stranded inside a variant folder.
///
/// A model pack that shipped `model.edf` alongside its own `paints/` leaves them at
/// `FrostMod Models/<Variant>/paints/*.pnt` once `register_loose_swaps` moves the folder
/// in. The game only ever reads `<Bike>/paints/`, and no scan of ours looked in there
/// either, so those liveries are dead files — installed, invisible, and unusable.
///
/// They are exactly what an assignment describes, so record the claim and move them onto
/// the shelf; the reconcile pass that follows brings them home if their model is the one
/// on the bike. Idempotent: once moved, the folder is empty and there is nothing to adopt.
fn adopt_stranded_liveries(mods_path: &str, bike: &str, assignments: &mut PaintAssignments) -> bool {
    let shelf = shelf_dir(mods_path, bike);
    let mut changed = false;

    for variant in variant_names(mods_path, bike) {
        let stray = variant_dir(mods_path, bike, &variant).join("paints");
        if !stray.is_dir() {
            continue;
        }
        for file in list_files(&stray) {
            if !file.to_ascii_lowercase().ends_with(PNT_EXT) {
                continue;
            }
            if !move_livery(&stray, &shelf, &file) {
                continue; // a livery of that name is already shelved — leave this one be
            }
            let base = strip_pnt(&file).to_string();
            let owned = assignments.entry(variant.clone()).or_default();
            if !contains_ci(owned, &base) {
                owned.push(base);
            }
            changed = true;
        }
        // Only if we emptied it: a `paints/` folder still holding something isn't ours.
        let _ = fs::remove_dir(&stray);
    }
    changed
}

/// Put every *assigned* livery where the active model says it belongs: owned by the active
/// variant → loose in `paints/`; owned only by others → on the shelf. Liveries no variant
/// claims are never touched.
///
/// Idempotent and order-independent, so it can be re-run after any drift. Returns how many
/// liveries it could **not** move — MX Bikes holds these files open while it runs, so a
/// reconcile mid-session legitimately fails and the caller has to say so rather than
/// report a clean filter.
pub fn reconcile_paints(mods_path: &str, bike: &str) -> usize {
    let mut assignments = load_paint_assignments(mods_path, bike);
    if adopt_stranded_liveries(mods_path, bike, &mut assignments) {
        let _ = save_paint_assignments(mods_path, bike, &assignments);
    }

    let paints = paints_dir(mods_path, bike);
    let shelf = shelf_dir(mods_path, bike);

    // Every livery an assignment has an opinion about: claimed by some variant, or sitting
    // on the shelf — which only ever happens because it *was* claimed. Including the shelf
    // is what brings a livery home once its last claim is dropped.
    let mut subject: Vec<String> = Vec::new();
    for name in assignments.values().flatten().cloned().chain(liveries_in(&shelf)) {
        if !contains_ci(&subject, &name) {
            subject.push(name);
        }
    }
    if subject.is_empty() {
        return 0;
    }

    let active = current_active(mods_path, bike);
    let owned_by_active: Vec<String> = assignments
        .iter()
        .filter(|(v, _)| v.eq_ignore_ascii_case(&active))
        .flat_map(|(_, paints)| paints.iter().cloned())
        .collect();
    let mut stuck = 0usize;

    for base in &subject {
        // Home is the loose folder unless someone else has claimed it and the active model
        // hasn't — an unclaimed livery belongs on offer under every model.
        let claimed = assignments.values().any(|p| contains_ci(p, base));
        let (from, to) = if claimed && !contains_ci(&owned_by_active, base) {
            (&paints, &shelf)
        } else {
            (&shelf, &paints)
        };
        let Some(file) = livery_file(from, base) else {
            continue; // already where it belongs (or gone from the tree entirely)
        };
        if !move_livery(from, to, &file) {
            stuck += 1;
        }
    }

    // Don't leave an empty `_paints/` behind once nothing is shelved.
    if shelf.is_dir() && list_files(&shelf).is_empty() {
        let _ = fs::remove_dir(&shelf);
    }
    stuck
}

/// The liveries that would sit loose in `paints/` with `variant` on the bike: the ones it
/// claims, plus every livery no variant claims at all.
pub fn liveries_under(mods_path: &str, bike: &str, variant: &str) -> Vec<String> {
    let assignments = load_paint_assignments(mods_path, bike);
    let mine: Vec<String> = assignments
        .iter()
        .filter(|(v, _)| v.eq_ignore_ascii_case(variant))
        .flat_map(|(_, p)| p.iter().cloned())
        .collect();
    bike_liveries(mods_path, bike)
        .into_iter()
        .filter(|l| {
            contains_ci(&mine, l) || !assignments.values().any(|p| contains_ci(p, l))
        })
        .collect()
}

/// Where each of `variant`'s liveries actually sits right now — loose in `paints/` if that
/// model is the one on the bike, on the shelf if it isn't. Lets a preview show the liveries
/// a swap would bring without moving anything first.
fn livery_paths(mods_path: &str, bike: &str, variant: &str) -> Vec<PathBuf> {
    let paints = paints_dir(mods_path, bike);
    let shelf = shelf_dir(mods_path, bike);
    liveries_under(mods_path, bike, variant)
        .into_iter()
        .filter_map(|base| {
            livery_file(&paints, &base)
                .map(|f| paints.join(f))
                .or_else(|| livery_file(&shelf, &base).map(|f| shelf.join(f)))
        })
        .collect()
}

/// Replace the set of liveries owned by one variant, then make the folder match. An empty
/// list drops the variant from the record entirely, so unassigning everything leaves the
/// tree exactly as it was found.
pub fn set_model_paints(
    mods_path: &str,
    bike: &str,
    model: &str,
    paints: &[String],
) -> anyhow::Result<usize> {
    if !is_simple_name(bike) || !is_simple_name(model) {
        anyhow::bail!("invalid bike or model name");
    }
    if !dir_exists(&bike_dir(mods_path, bike)) {
        anyhow::bail!("bike '{bike}' not found");
    }
    let mut assignments = load_paint_assignments(mods_path, bike);
    let cleaned: Vec<String> = paints
        .iter()
        .map(|p| strip_pnt(p).to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if cleaned.is_empty() {
        assignments.remove(model);
        // The record keys on the name the caller used; an older one may differ in case.
        assignments.retain(|v, _| !v.eq_ignore_ascii_case(model));
    } else {
        assignments.retain(|v, _| !v.eq_ignore_ascii_case(model));
        assignments.insert(model.to_string(), cleaned);
    }
    save_paint_assignments(mods_path, bike, &assignments)?;
    Ok(reconcile_paints(mods_path, bike))
}

fn scan_variants(mods_path: &str, bike: &str) -> Vec<ModelVariant> {
    let active_label = {
        let a = read_active(mods_path, bike);
        if a.is_empty() { ORIGINAL.to_string() } else { a }
    };

    // The active model set is the subset of the bike's loose files that belongs to the
    // model — not the whole folder (see `active_set_files`).
    let active_files = active_set_files(mods_path, bike, &active_label, &[]).len();
    let assignments = load_paint_assignments(mods_path, bike);
    let paints_of = |name: &str| -> Vec<String> {
        assignments
            .iter()
            .find(|(v, _)| v.eq_ignore_ascii_case(name))
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    };
    let mut variants = vec![ModelVariant {
        // The active set is loose at the root — valid iff a mesh is there.
        valid: crate::bikefiles::dir_has_mesh(&bike_dir(mods_path, bike)),
        empty: active_files == 0,
        file_count: active_files,
        paints: paints_of(&active_label),
        name: active_label.clone(),
        active: true,
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
            if is_shelf(&name) {
                continue; // the livery shelf is not a model set
            }
            if name.eq_ignore_ascii_case(&active_label) {
                continue; // active is already row 0
            }
            let files = set_files(&p).len();
            others.push(ModelVariant {
                valid: crate::bikefiles::dir_has_mesh(&p),
                empty: files == 0,
                file_count: files,
                paints: paints_of(&name),
                name,
                active: false,
            });
        }
    }
    // Guarantee a Stock row so a bike with a single swap can still go back to the game's
    // own model, exactly as `soundmods` does for sounds. Unconditional on purpose: nothing
    // on disk distinguishes an OEM bike carrying a dropped-in swap from an unpacked mod
    // bike — an OEM bike keeps its model in the game's own archive, with nothing of it in
    // `mods/bikes` at all (see `library::scan_bike_targets`) — so gating would hide the row
    // from the very bikes that need it. Reverting only ever *parks* the loose set, so the
    // worst case is a bike left without a model and one click to put it back.
    // Skipped when nothing is loose: the active row is already stock, whatever it's called.
    let has_stock = |v: &ModelVariant| v.name.eq_ignore_ascii_case(STOCK);
    if !variants[0].empty && !variants.iter().chain(others.iter()).any(has_stock) {
        others.push(ModelVariant {
            paints: paints_of(STOCK),
            name: STOCK.to_string(),
            active: false,
            valid: false,
            empty: true,
            file_count: 0,
        });
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
    apply_model_swap_reporting(mods_path, bike, target).map(|_| ())
}

/// [`apply_model_swap`], reporting how many liveries the follow-up reconcile could not
/// move. See [`reconcile_paints`] — MX Bikes holds bike files open while it runs.
pub fn apply_model_swap_reporting(
    mods_path: &str,
    bike: &str,
    target: &str,
) -> anyhow::Result<usize> {
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

    let is_stock = target.eq_ignore_ascii_case(STOCK);
    let backup_dir = variant_dir(mods_path, bike, &active_label); // park the live set here
    let target_dir = variant_dir(mods_path, bike, target); // bring this set in

    // Stock is never a folder — it's whatever model the game itself has for this bike,
    // reached by parking the loose set and bringing in nothing. Every other target has to
    // exist in the library.
    if !is_stock && !dir_exists(&target_dir) {
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
    let mut root_files = active_set_files(mods_path, bike, &active_label, &target_files);

    // Reverting to Stock has to clear every loose override, not just the meshes. A swap
    // dropped straight at the bike root was never registered, so it has no manifest and
    // `active_set_files` finds only its meshes — leaving its `.hrc`/`.cfg` behind, still
    // overriding the `.pkz` but now naming meshes that are gone. Nothing is deleted: it
    // parks with the rest, and the manifest written below makes the way back exact.
    if is_stock && read_manifest(&backup_dir).is_none() {
        for f in root_setup_files(mods_path, bike) {
            if !contains_ci(&root_files, &f) {
                root_files.push(f);
            }
        }
    }

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
    // The model changed, so the liveries on offer changed with it. The swap itself has
    // already happened, so a livery the game is holding open is worth a word to the user
    // rather than a rolled-back swap — hence a count, not an error.
    Ok(reconcile_paints(mods_path, bike))
}

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

/// The file accounting `apply_model_swap` would do, without doing it. Same rules on
/// purpose: what the preview shows has to be what applying the swap gives you, so the two
/// resolve the set through `active_set_files` and the same Stock special-case.
pub fn preview_set(mods_path: &str, bike: &str, variant: &str) -> anyhow::Result<PreviewSet> {
    if !is_simple_name(bike) || !is_simple_name(variant) {
        anyhow::bail!("invalid bike or model name");
    }
    let root = bike_dir(mods_path, bike);
    // A bike installed as a bare `NAME.pkz` has no folder beside it until something writes
    // one — no swap registered, no paint installed. There is still a model in there, and the
    // reader downstream finds it by the sibling name, so a missing folder alone isn't a missing
    // bike: it just means every file comes out of the archive. Only the read path is relaxed —
    // applying a swap still needs somewhere to park the files it displaces.
    if !dir_exists(&root) && !crate::library::sibling_pkz(&root).is_file() {
        anyhow::bail!("bike '{bike}' not found");
    }
    let active = current_active(mods_path, bike);
    let target_dir = variant_dir(mods_path, bike, variant);

    // The active set is already loose at the root — show the bike as it stands.
    if variant.eq_ignore_ascii_case(&active) {
        return Ok(PreviewSet {
            root_keep: list_files(&root),
            paints: livery_paths(mods_path, bike, variant),
            bike_dir: root,
            variant_dir: target_dir,
            variant_files: Vec::new(),
        });
    }

    let is_stock = variant.eq_ignore_ascii_case(STOCK);
    if !is_stock && !dir_exists(&target_dir) {
        anyhow::bail!("model '{variant}' not found");
    }
    let variant_files = set_files(&target_dir);
    if !variant_files.is_empty() && !crate::bikefiles::dir_has_mesh(&target_dir) {
        anyhow::bail!("model '{variant}' has no mesh (.edf) — it looks like an incomplete set");
    }

    let mut parked = active_set_files(mods_path, bike, &active, &variant_files);
    if is_stock && read_manifest(&variant_dir(mods_path, bike, &active)).is_none() {
        for f in root_setup_files(mods_path, bike) {
            if !contains_ci(&parked, &f) {
                parked.push(f);
            }
        }
    }

    let root_keep = list_files(&root)
        .into_iter()
        .filter(|f| !contains_ci(&parked, f))
        .collect();
    let paints = livery_paths(mods_path, bike, variant);
    Ok(PreviewSet { bike_dir: root, root_keep, variant_dir: target_dir, variant_files, paints })
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
    if has_packed_fallback(&root) {
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
            if !p.is_dir() || e.file_name().to_str().is_some_and(is_shelf) {
                continue; // the shelf holds liveries, never a bike's setup
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

    // --- livery ownership -------------------------------------------------------------

    /// A KTM wearing a Yami model swap, with liveries drawn for each — the case the
    /// feature exists for.
    fn make_bike_with_liveries(mp: &str, bike: &str, liveries: &[&str]) {
        make_bike(mp, bike, "model.edf");
        touch(&variant_dir(mp, bike, "Yami").join("model.edf"));
        for l in liveries {
            touch(&paints_dir(mp, bike).join(format!("{l}.pnt")));
        }
    }
    fn assign(mp: &str, bike: &str, model: &str, paints: &[&str]) -> usize {
        let owned: Vec<String> = paints.iter().map(|s| s.to_string()).collect();
        set_model_paints(mp, bike, model, &owned).unwrap()
    }
    fn loose_liveries(mp: &str, bike: &str) -> Vec<String> {
        liveries_in(&paints_dir(mp, bike))
    }
    fn shelved_liveries(mp: &str, bike: &str) -> Vec<String> {
        liveries_in(&shelf_dir(mp, bike))
    }

    #[test]
    fn assigned_liveries_follow_the_active_model() {
        let root = tmp("paint-follows-model");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "KTM Factory", "Plain White"]);

        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);
        assign(mp, "KTM450", ORIGINAL, &["KTM Factory"]);

        // Original is active, so only its livery — plus the unassigned one — is on offer.
        assert_eq!(loose_liveries(mp, "KTM450"), ["KTM Factory", "Plain White"]);
        assert_eq!(shelved_liveries(mp, "KTM450"), ["Yami Redbud"]);

        apply_model_swap(mp, "KTM450", "Yami").unwrap();
        assert_eq!(
            loose_liveries(mp, "KTM450"),
            ["Plain White", "Yami Redbud"],
            "the Yami livery came back and the KTM one went away",
        );
        assert_eq!(shelved_liveries(mp, "KTM450"), ["KTM Factory"]);

        apply_model_swap(mp, "KTM450", ORIGINAL).unwrap();
        assert_eq!(loose_liveries(mp, "KTM450"), ["KTM Factory", "Plain White"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unassigned_livery_is_never_moved() {
        let root = tmp("paint-unassigned");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "Plain White"]);

        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);
        apply_model_swap(mp, "KTM450", "Yami").unwrap();
        apply_model_swap(mp, "KTM450", ORIGINAL).unwrap();

        assert!(
            loose_liveries(mp, "KTM450").contains(&"Plain White".to_string()),
            "a livery no model claims stays on offer under every model",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn swapping_an_unassigned_bike_is_byte_for_byte_what_it_was() {
        // The invariant the whole feature rests on: until someone assigns a livery, every
        // path behaves exactly as it did before this existed.
        let root = tmp("paint-inert");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Red", "Blue", "Green"]);
        let paints_before = names_at(&paints_dir(mp, "KTM450"));
        let root_before = names_at(&bike_dir(mp, "KTM450"));

        apply_model_swap(mp, "KTM450", "Yami").unwrap();
        apply_model_swap(mp, "KTM450", ORIGINAL).unwrap();

        assert_eq!(names_at(&paints_dir(mp, "KTM450")), paints_before);
        assert_eq!(names_at(&bike_dir(mp, "KTM450")), root_before);
        assert!(!shelf_dir(mp, "KTM450").exists());
        assert!(!assign_path(mp, "KTM450").exists());
        assert!(
            scan_model_swaps(mp)[0].variants.iter().all(|v| v.paints.is_empty()),
            "no model claims anything",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unassigning_everything_puts_the_folder_back() {
        let root = tmp("paint-unassign");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "Plain White"]);
        let before = names_at(&paints_dir(mp, "KTM450"));

        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);
        assert_eq!(shelved_liveries(mp, "KTM450"), ["Yami Redbud"]);

        assign(mp, "KTM450", "Yami", &[]);
        assert_eq!(names_at(&paints_dir(mp, "KTM450")), before, "the livery came home");
        assert!(!shelf_dir(mp, "KTM450").exists(), "and the empty shelf is gone");
        assert!(!assign_path(mp, "KTM450").exists(), "and so is the record");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn two_models_can_claim_the_same_livery() {
        // One file, no copies: ownership is a record, so a livery that suits both meshes
        // comes home under either one.
        let root = tmp("paint-shared-claim");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Number 7", "Yami Only"]);

        assign(mp, "KTM450", "Yami", &["Number 7", "Yami Only"]);
        assign(mp, "KTM450", ORIGINAL, &["Number 7"]);

        assert_eq!(loose_liveries(mp, "KTM450"), ["Number 7"]);
        assert_eq!(shelved_liveries(mp, "KTM450"), ["Yami Only"]);

        apply_model_swap(mp, "KTM450", "Yami").unwrap();
        assert_eq!(
            loose_liveries(mp, "KTM450"),
            ["Number 7", "Yami Only"],
            "the shared livery stayed put while the Yami one arrived",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stock_can_own_liveries() {
        // The OP's case: a KTM's own liveries belong to the model in its `.pkz`, which
        // never has a folder — so only a record can express it.
        let root = tmp("paint-stock-owns");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["KTM Factory", "Yami Redbud"]);
        touch(&bike_dir(mp, "KTM450").join("KTM450.pkz"));

        assign(mp, "KTM450", STOCK, &["KTM Factory"]);
        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);

        apply_model_swap(mp, "KTM450", "Yami").unwrap();
        assert_eq!(loose_liveries(mp, "KTM450"), ["Yami Redbud"]);

        apply_model_swap(mp, "KTM450", STOCK).unwrap();
        assert_eq!(loose_liveries(mp, "KTM450"), ["KTM Factory"]);

        let stock = scan_model_swaps(mp)[0]
            .variants
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(STOCK))
            .cloned()
            .expect("a Stock row");
        assert_eq!(stock.paints, ["KTM Factory"], "and the row reports its claim");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_record_naming_a_deleted_livery_is_harmless() {
        // Realistic drift: the user deletes a `.pnt` the record still claims. Reconcile
        // must skip it rather than fail, and must not disturb the liveries that remain.
        let root = tmp("paint-stale-record");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "Plain White"]);
        assign(mp, "KTM450", "Yami", &["Yami Redbud", "Deleted Livery"]);

        assert_eq!(reconcile_paints(mp, "KTM450"), 0, "a name with no file isn't a failure");
        assert_eq!(loose_liveries(mp, "KTM450"), ["Plain White"]);
        assert_eq!(shelved_liveries(mp, "KTM450"), ["Yami Redbud"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reconcile_is_idempotent() {
        let root = tmp("paint-idempotent");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "Plain White"]);
        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);

        let loose = loose_liveries(mp, "KTM450");
        let shelved = shelved_liveries(mp, "KTM450");
        for _ in 0..3 {
            assert_eq!(reconcile_paints(mp, "KTM450"), 0);
            assert_eq!(loose_liveries(mp, "KTM450"), loose);
            assert_eq!(shelved_liveries(mp, "KTM450"), shelved);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_scan_never_moves_a_livery() {
        // Adoption moves files, and `scan_model_swaps` runs over the whole tree on every
        // Locker refresh and every mods-changed event. Only the single-bike, user-initiated
        // calls reconcile; a scan stays read-only.
        let root = tmp("paint-scan-readonly");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Plain White"]);
        let stray = variant_dir(mp, "KTM450", "Yami").join("paints");
        touch(&stray.join("Yami Redbud.pnt"));

        let _ = scan_model_swaps(mp);
        assert!(stray.join("Yami Redbud.pnt").is_file(), "the scan left it where it was");
        assert!(!assign_path(mp, "KTM450").exists(), "and recorded nothing");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bike_liveries_sees_shelved_ones_too() {
        // The picker that assigns liveries has to keep offering the ones it shelved.
        let root = tmp("paint-liveries-list");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "Plain White"]);
        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);

        assert_eq!(bike_liveries(mp, "KTM450"), ["Plain White", "Yami Redbud"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_livery_stranded_in_a_variant_folder_is_adopted() {
        // What a model pack that shipped its own `paints/` leaves behind once
        // `register_loose_swaps` files it away: liveries the game never reads and no scan
        // of ours ever showed. They're an assignment waiting to be recorded.
        let root = tmp("paint-adopt");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Plain White"]);
        touch(&variant_dir(mp, "KTM450", "Yami").join("paints/Yami Redbud.pnt"));

        assert_eq!(reconcile_paints(mp, "KTM450"), 0);
        assert_eq!(
            load_paint_assignments(mp, "KTM450").get("Yami").map(Vec::as_slice),
            Some(["Yami Redbud".to_string()].as_slice()),
            "the variant that carried it is recorded as its owner",
        );
        assert_eq!(shelved_liveries(mp, "KTM450"), ["Yami Redbud"], "shelved: Yami is off");
        assert!(
            !variant_dir(mp, "KTM450", "Yami").join("paints").exists(),
            "and the emptied folder is tidied away",
        );

        apply_model_swap(mp, "KTM450", "Yami").unwrap();
        assert_eq!(
            loose_liveries(mp, "KTM450"),
            ["Plain White", "Yami Redbud"],
            "and it works like any other assignment from then on",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn adopting_never_writes_over_a_livery_of_the_same_name() {
        // Two different `.pnt`s can share a base name — one drawn for each mesh. Adoption
        // renames files into a shared shelf, so it has to refuse rather than clobber.
        let root = tmp("paint-adopt-collision");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Redbud"]);
        assign(mp, "KTM450", "Yami", &["Redbud"]); // shelves the KTM's `Redbud.pnt`
        let stray = variant_dir(mp, "KTM450", "Yami").join("paints");
        fs::create_dir_all(&stray).unwrap();
        fs::write(stray.join("Redbud.pnt"), b"a different Redbud").unwrap();

        reconcile_paints(mp, "KTM450");
        assert_eq!(
            fs::read(stray.join("Redbud.pnt")).unwrap(),
            b"a different Redbud",
            "the stranded livery is left where it is rather than destroying the shelved one",
        );
        assert_eq!(shelved_liveries(mp, "KTM450"), ["Redbud"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_shelf_is_not_a_model_variant() {
        // It lives inside `FrostMod Models/`, so every walk over that folder's children
        // has to skip it or it reads as a swap called `_paints`.
        let root = tmp("paint-shelf-not-variant");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "Plain White"]);
        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);
        assert!(shelf_dir(mp, "KTM450").is_dir(), "something is shelved");

        let names: Vec<String> =
            scan_model_swaps(mp)[0].variants.iter().map(|v| v.name.clone()).collect();
        assert!(!names.iter().any(|n| is_shelf(n)), "not offered as a model: {names:?}");
        assert!(detect_orphaned_setup(mp).is_empty(), "and not read as a gutted bike");
        assert!(
            detect_loose_swaps(mp).is_empty(),
            "and not offered for registration as a loose swap",
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_reports_the_liveries_the_swap_would_bring() {
        // The viewer's paint list has to be the target model's, not whatever is sitting in
        // `paints/` for the model still on the bike.
        let root = tmp("paint-preview");
        let mp = root.to_str().unwrap();
        make_bike_with_liveries(mp, "KTM450", &["Yami Redbud", "KTM Factory", "Plain White"]);
        assign(mp, "KTM450", "Yami", &["Yami Redbud"]);
        assign(mp, "KTM450", ORIGINAL, &["KTM Factory"]);

        let names = |set: &PreviewSet| -> Vec<String> {
            let mut v: Vec<String> = set
                .paints
                .iter()
                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                .collect();
            v.sort();
            v
        };

        let yami = preview_set(mp, "KTM450", "Yami").unwrap();
        assert_eq!(names(&yami), ["Plain White.pnt", "Yami Redbud.pnt"]);
        let now = preview_set(mp, "KTM450", ORIGINAL).unwrap();
        assert_eq!(names(&now), ["KTM Factory.pnt", "Plain White.pnt"]);
        let _ = fs::remove_dir_all(&root);
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

    /// The whole point of a preview: what it shows must be what applying the swap gives
    /// you. Asserted against the real thing — predict the root, then apply and compare.
    #[test]
    fn preview_predicts_the_root_the_swap_would_leave() {
        let root = tmp("preview-matches");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        touch(&bike_dir(mp, "KTM450").join("body.tga"));
        touch(&variant_dir(mp, "KTM450", "Factory").join("model.edf"));
        touch(&variant_dir(mp, "KTM450", "Factory").join("factory.tga"));

        let set = preview_set(mp, "KTM450", "Factory").unwrap();
        let mut predicted: Vec<String> =
            set.root_keep.iter().chain(set.variant_files.iter()).cloned().collect();
        predicted.sort();

        apply_model_swap(mp, "KTM450", "Factory").unwrap();
        assert_eq!(predicted, names_at(&bike_dir(mp, "KTM450")));
        let _ = fs::remove_dir_all(&root);
    }

    /// Stock brings in nothing, so the preview has to clear the loose overrides — meshes
    /// *and* the setup naming them — leaving the packed model to show through.
    #[test]
    fn preview_of_stock_clears_every_loose_override() {
        let root = tmp("preview-stock");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        touch(&bike_dir(mp, "KTM450").join("KTM450.pkz"));

        let set = preview_set(mp, "KTM450", STOCK).unwrap();
        assert_eq!(set.root_keep, vec!["KTM450.pkz"], "only the packed bike is left");
        assert!(set.variant_files.is_empty(), "Stock is never a folder");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_of_the_active_model_is_the_bike_as_it_stands() {
        let root = tmp("preview-active");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");

        let set = preview_set(mp, "KTM450", ORIGINAL).unwrap();
        let mut keep = set.root_keep.clone();
        keep.sort();
        assert_eq!(keep, names_at(&bike_dir(mp, "KTM450")));
        assert!(set.variant_files.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_refuses_what_the_swap_would_refuse() {
        let root = tmp("preview-refuse");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM450", "model.edf");
        // Files but no mesh — an incomplete set, rejected on apply and on preview alike.
        touch(&variant_dir(mp, "KTM450", "Broken").join("body.tga"));

        assert!(preview_set(mp, "KTM450", "Broken").is_err(), "incomplete set");
        assert!(preview_set(mp, "KTM450", "Nope").is_err(), "no such variant");
        assert!(preview_set(mp, "Ghost", "Factory").is_err(), "no such bike");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_reads_a_bike_that_is_only_a_pkz() {
        // A bike installed as a bare `NAME.pkz` grows its folder the first time a swap is
        // registered or a paint installed — until then there is no folder, and the preview
        // used to call that "bike not found". Every file simply comes out of the archive.
        let root = tmp("preview-pkz-only");
        let mp = root.to_str().unwrap();
        touch(&bikes_root(mp).join("Packed.pkz"));
        assert!(!bike_dir(mp, "Packed").exists(), "no folder beside the archive");

        let set = preview_set(mp, "Packed", ORIGINAL).expect("pkz-only bike previews");
        assert!(set.root_keep.is_empty(), "nothing loose to keep");
        assert!(set.variant_files.is_empty());
        // Still not a bike when neither the folder nor the archive is there.
        assert!(preview_set(mp, "Ghost", ORIGINAL).is_err());
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
        // The synthesized Stock row is empty by definition — it's the real sets that must
        // not be written off for lacking a `model.edf`.
        assert!(
            bikes[0].variants.iter().filter(|v| !v.empty).all(|v| v.valid),
            "both sets are valid"
        );

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

    /// An OEM bike as the game ships it: every mesh inside the `.pkz`, only paints loose,
    /// with a model swap dropped over the top (mesh + the setup naming it).
    fn make_packed_bike_with_dropin(mp: &str, bike: &str) {
        touch(&bike_dir(mp, bike).join(format!("{bike}.pkz")));
        touch(&bike_dir(mp, bike).join("paints").join("Red.pnt"));
        make_bike(mp, bike, "model.edf");
    }

    #[test]
    fn stock_row_is_offered_when_the_bike_has_a_packed_fallback() {
        let root = tmp("stock-row");
        let mp = root.to_str().unwrap();
        make_packed_bike_with_dropin(mp, "KTM");

        let bikes = scan_model_swaps(mp);
        let names: Vec<&str> = bikes[0].variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&STOCK), "a Stock row is offered: {names:?}");

        let stock = bikes[0].variants.iter().find(|v| v.name == STOCK).unwrap();
        assert!(stock.empty && !stock.valid && !stock.active);
        assert_eq!(stock.file_count, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stock_row_is_offered_without_a_packed_fallback_too() {
        // Nothing on disk tells an OEM bike carrying a dropped-in swap apart from an
        // unpacked mod bike, so the row is unconditional — gating it would hide it from
        // exactly the bikes that need it.
        let root = tmp("stock-unpacked");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM", "model.edf"); // no `.pkz`
        touch(&variant_dir(mp, "KTM", "Factory").join("model.edf"));

        let bikes = scan_model_swaps(mp);
        let names: Vec<&str> = bikes[0].variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&STOCK), "Stock is always offered: {names:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stock_row_is_never_duplicated() {
        let root = tmp("stock-dup");
        let mp = root.to_str().unwrap();
        make_packed_bike_with_dropin(mp, "KTM");
        apply_model_swap(mp, "KTM", STOCK).unwrap();

        // Active *is* Stock now — the synthesized row must not be added a second time.
        let bikes = scan_model_swaps(mp);
        let stock: Vec<_> = bikes[0].variants.iter().filter(|v| v.name == STOCK).collect();
        assert_eq!(stock.len(), 1, "exactly one Stock row: {:?}", bikes[0].variants);
        assert!(stock[0].active);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_stock_clears_every_loose_override() {
        let root = tmp("stock-apply");
        let mp = root.to_str().unwrap();
        make_packed_bike_with_dropin(mp, "KTM");

        apply_model_swap(mp, "KTM", STOCK).unwrap();

        // The drop-in never had a manifest, so its setup would have been left behind by
        // the mesh-only rule — still overriding the `.pkz`, now naming a mesh that's gone.
        let at_root = names_at(&bike_dir(mp, "KTM"));
        assert_eq!(at_root, vec!["KTM.pkz"], "only the packed bike is left: {at_root:?}");
        assert!(file_exists(&bike_dir(mp, "KTM").join("paints").join("Red.pnt")), "paints untouched");

        let parked = names_at(&variant_dir(mp, "KTM", ORIGINAL));
        for f in ["model.edf", "chassis.hrc", "bike.cfg", "wheel.geom"] {
            assert!(parked.contains(&f.to_string()), "{f} parked, not deleted: {parked:?}");
        }
        assert_eq!(read_active(mp, "KTM"), STOCK);
        // Stock is not a folder — nothing was created for it.
        assert!(!dir_exists(&variant_dir(mp, "KTM", STOCK)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn swapping_back_off_stock_restores_the_whole_set() {
        let root = tmp("stock-back");
        let mp = root.to_str().unwrap();
        make_packed_bike_with_dropin(mp, "KTM");
        let before = names_at(&bike_dir(mp, "KTM"));

        apply_model_swap(mp, "KTM", STOCK).unwrap();
        apply_model_swap(mp, "KTM", ORIGINAL).unwrap();

        assert_eq!(names_at(&bike_dir(mp, "KTM")), before, "the bike folder is exactly as it was");
        assert_eq!(read_active(mp, "KTM"), ORIGINAL);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stock_on_a_bike_with_nothing_behind_it_is_still_reversible() {
        // The cost of offering Stock unconditionally: a bike with no packed model is left
        // without one. That has to stay a parking job, never a delete, and one swap back
        // has to undo it completely — otherwise the row is a trap.
        let root = tmp("stock-no-fallback");
        let mp = root.to_str().unwrap();
        make_bike(mp, "KTM", "model.edf"); // no `.pkz` behind it
        let before = names_at(&bike_dir(mp, "KTM"));

        apply_model_swap(mp, "KTM", STOCK).unwrap();
        assert!(names_at(&bike_dir(mp, "KTM")).is_empty(), "the root is bare");
        assert!(file_exists(&variant_dir(mp, "KTM", ORIGINAL).join("model.edf")), "parked, not deleted");

        apply_model_swap(mp, "KTM", ORIGINAL).unwrap();
        assert_eq!(names_at(&bike_dir(mp, "KTM")), before, "one click puts it all back");
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

