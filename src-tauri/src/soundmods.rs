use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Library labeling store (best-effort record of which bikes received a sound).
// Kept as-is; used by the Library to tell modded bikes from stock. Distinct from
// the sound-swap engine below.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    bikes: BTreeMap<String, String>,
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("sound-mods.json")
}

fn load(dir: &Path) -> Store {
    match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

pub fn record(dir: &Path, bikes: &[String], mod_name: &str) -> anyhow::Result<()> {
    if bikes.is_empty() {
        return Ok(());
    }
    let mut store = load(dir);
    for b in bikes {
        store.bikes.insert(b.clone(), mod_name.to_string());
    }
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(&store)?)?;
    Ok(())
}

pub fn known_bikes(dir: &Path) -> Vec<String> {
    load(dir).bikes.into_keys().collect()
}

// ---------------------------------------------------------------------------
// Sound-swap engine — the twin of `modelswap`, for a bike's engine sound.
//
// A **sound set** is the loose files at a bike's root that make up its sound:
// the two must-files `engine.scl` + `sfx.cfg`, plus any optional `.wav`/`.mp3`.
// The active set lives loose at `mods/bikes/<Bike>/` (alongside the model files);
// alternative sets are parked in `mods/bikes/<Bike>/FrostMod Sounds/<Variant>/`,
// and `_active.txt` names the active one. **Stock** = no loose sound files (the
// bike's packed default plays), the sound analogue of the model "No model".
//
// Bindings (`_bindings.json`) map a model-swap variant to a sound variant so a
// model swap can pull its sound along; see `reconcile_after_model_swap`.
// ---------------------------------------------------------------------------

const SOUND_LIB_DIR: &str = "FrostMod Sounds";
const MARKER: &str = "_active.txt";
const BINDINGS: &str = "_bindings.json";
pub const STOCK_LABEL: &str = "Stock";
const MUST_FILES: [&str; 2] = ["engine.scl", "sfx.cfg"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundVariant {
    pub name: String,
    pub active: bool,
    /// Has both must-files (`engine.scl` + `sfx.cfg`) — a complete, applicable set.
    pub valid: bool,
    /// No sound files at all — the intentional "Stock" (no sound mod) set, distinct
    /// from an incomplete set that has files but is missing a must-file.
    pub empty: bool,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeSounds {
    pub bike: String,
    pub active: String,
    /// The bike's currently-active model swap, so the UI can render bindings
    /// relative to it ("bind this sound to <active model>").
    pub active_model: String,
    pub variants: Vec<SoundVariant>,
    /// model-swap variant name -> bound sound variant name.
    pub bindings: BTreeMap<String, String>,
}

fn bikes_root(mods_path: &str) -> PathBuf {
    crate::library::mods_subdir(mods_path, "mods/bikes")
}
fn bike_dir(mods_path: &str, bike: &str) -> PathBuf {
    bikes_root(mods_path).join(bike)
}
fn lib_dir(mods_path: &str, bike: &str) -> PathBuf {
    bike_dir(mods_path, bike).join(SOUND_LIB_DIR)
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

/// A file that belongs to a sound set: the two must-files or any `.wav`/`.mp3`.
/// Shared with `modelswap`, which excludes these so a model swap never drags
/// audio along.
pub fn is_sound_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    MUST_FILES.iter().any(|m| n == *m)
        || n.ends_with(".wav")
        || n.ends_with(".mp3")
}

/// Root-level sound files in `dir` (non-recursive, mirrors the model swapper).
fn sound_files_in(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_file() {
                if let Some(n) = e.file_name().to_str() {
                    if is_sound_file(n) {
                        out.push(n.to_string());
                    }
                }
            }
        }
    }
    out
}

fn has_both_must(files: &[String]) -> bool {
    MUST_FILES
        .iter()
        .all(|m| files.iter().any(|f| f.eq_ignore_ascii_case(m)))
}

fn dir_exists(p: &Path) -> bool {
    p.is_dir()
}

fn read_active(mods_path: &str, bike: &str) -> String {
    fs::read_to_string(lib_dir(mods_path, bike).join(MARKER))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn active_label(mods_path: &str, bike: &str) -> String {
    let a = read_active(mods_path, bike);
    if a.is_empty() {
        STOCK_LABEL.to_string()
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

fn bindings_path(mods_path: &str, bike: &str) -> PathBuf {
    lib_dir(mods_path, bike).join(BINDINGS)
}

fn load_bindings(mods_path: &str, bike: &str) -> BTreeMap<String, String> {
    match fs::read_to_string(bindings_path(mods_path, bike)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    }
}

fn save_bindings(
    mods_path: &str,
    bike: &str,
    bindings: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let lib = lib_dir(mods_path, bike);
    fs::create_dir_all(&lib)?;
    fs::write(
        bindings_path(mods_path, bike),
        serde_json::to_string_pretty(bindings)?,
    )?;
    Ok(())
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

fn move_set(src: &Path, dst: &Path, files: &[String]) -> bool {
    if fs::create_dir_all(dst).is_err() {
        return false;
    }
    let mut done: Vec<&String> = Vec::new();
    for f in files {
        if move_one(&src.join(f), &dst.join(f)) {
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

fn scan_variants(mods_path: &str, bike: &str) -> Vec<SoundVariant> {
    let active = active_label(mods_path, bike);

    let active_files = sound_files_in(&bike_dir(mods_path, bike));
    let mut variants = vec![SoundVariant {
        valid: has_both_must(&active_files),
        empty: active_files.is_empty(),
        file_count: active_files.len(),
        name: active.clone(),
        active: true,
    }];

    let mut others: Vec<SoundVariant> = Vec::new();
    if let Ok(rd) = fs::read_dir(lib_dir(mods_path, bike)) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue; // skip _active.txt / _bindings.json
            }
            let name = match e.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.eq_ignore_ascii_case(&active) {
                continue; // active is already row 0
            }
            let files = sound_files_in(&p);
            others.push(SoundVariant {
                valid: has_both_must(&files),
                empty: files.is_empty(),
                file_count: files.len(),
                name,
                active: false,
            });
        }
    }
    others.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Guarantee a Stock row so you can always revert to the built-in sound.
    if !variants.iter().chain(others.iter()).any(|v| v.name.eq_ignore_ascii_case(STOCK_LABEL)) {
        others.push(SoundVariant {
            name: STOCK_LABEL.to_string(),
            active: false,
            valid: false,
            empty: true,
            file_count: 0,
        });
    }

    variants.extend(others);
    variants
}

pub fn scan_sound_swaps(mods_path: &str) -> Vec<BikeSounds> {
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
            if bike.starts_with('.') {
                continue;
            }
            // A bike is relevant to sounds if it already has a sound library or any
            // loose sound files. Model-only bikes get a synthesized Stock row in the UI.
            let qualifies = dir_exists(&lib_dir(mods_path, &bike))
                || !sound_files_in(&p).is_empty();
            if !qualifies {
                continue;
            }
            let variants = scan_variants(mods_path, &bike);
            let active = variants
                .iter()
                .find(|v| v.active)
                .map(|v| v.name.clone())
                .unwrap_or_else(|| STOCK_LABEL.to_string());
            out.push(BikeSounds {
                active_model: crate::modelswap::current_active(mods_path, &bike),
                bindings: load_bindings(mods_path, &bike),
                bike,
                active,
                variants,
            });
        }
    }
    out.sort_by(|a, b| a.bike.to_lowercase().cmp(&b.bike.to_lowercase()));
    out
}

pub fn apply_sound_swap(mods_path: &str, bike: &str, target: &str) -> anyhow::Result<()> {
    if !is_simple_name(bike) || !is_simple_name(target) {
        anyhow::bail!("invalid bike or sound name");
    }
    let root = bike_dir(mods_path, bike);
    if !dir_exists(&root) {
        anyhow::bail!("bike '{bike}' not found");
    }

    let active = active_label(mods_path, bike);
    if target.eq_ignore_ascii_case(&active) {
        anyhow::bail!("'{target}' is already the active sound");
    }

    let is_stock = target.eq_ignore_ascii_case(STOCK_LABEL);
    let backup_dir = variant_dir(mods_path, bike, &active); // park the live sound here
    let target_dir = variant_dir(mods_path, bike, target); // bring this sound in

    // Stock is always available (revert = remove loose sound). Any other target must
    // exist as a folder in the library.
    if !is_stock && !dir_exists(&target_dir) {
        anyhow::bail!("sound '{target}' not found");
    }

    let root_files = sound_files_in(&root); // current loose sound to back up
    let target_files = sound_files_in(&target_dir); // variant sound to bring in

    // A set with files but missing a must-file is incomplete and rejected. An empty
    // target (Stock, or an intentional empty folder) is a valid "remove the sound" swap.
    if !target_files.is_empty() && !has_both_must(&target_files) {
        anyhow::bail!("sound '{target}' is missing engine.scl or sfx.cfg");
    }

    // 1) Back up the current loose sound into the library (all-or-nothing).
    if !root_files.is_empty() && !move_set(&root, &backup_dir, &root_files) {
        anyhow::bail!("couldn't back up the current sound — is the bike loaded in-game? Exit the bike first.");
    }
    // 2) Move the target's sound into the bike root; roll the backup back on failure.
    if !move_set(&target_dir, &root, &target_files) {
        move_set(&backup_dir, &root, &root_files); // restore
        anyhow::bail!("sound swap failed and was rolled back");
    }

    write_active(mods_path, bike, target)?;
    Ok(())
}

/// Bind a sound variant to a model-swap variant so activating that model applies
/// the sound. Pure metadata — the sound stays a normal variant in the library.
pub fn bind_sound(mods_path: &str, bike: &str, model: &str, sound: &str) -> anyhow::Result<()> {
    if !is_simple_name(bike) || model.is_empty() || sound.is_empty() {
        anyhow::bail!("invalid bike, model, or sound name");
    }
    let mut bindings = load_bindings(mods_path, bike);
    bindings.insert(model.to_string(), sound.to_string());
    save_bindings(mods_path, bike, &bindings)
}

/// Remove any sound binding for a model-swap variant.
pub fn unbind_sound(mods_path: &str, bike: &str, model: &str) -> anyhow::Result<()> {
    if !is_simple_name(bike) {
        anyhow::bail!("invalid bike name");
    }
    let mut bindings = load_bindings(mods_path, bike);
    if bindings.remove(model).is_some() {
        save_bindings(mods_path, bike, &bindings)?;
    }
    Ok(())
}

/// After a model swap `prev -> next`, make the sound travel with the model:
///   1. `next` has a bound sound        -> apply it.
///   2. else the active sound was `prev`'s bound sound (it belongs to the model we
///      just left) -> revert to Stock.
///   3. else                            -> leave the active sound as-is (independent).
pub fn reconcile_after_model_swap(
    mods_path: &str,
    bike: &str,
    prev: &str,
    next: &str,
) -> anyhow::Result<()> {
    let bindings = load_bindings(mods_path, bike);
    let active_sound = active_label(mods_path, bike);

    let desired = if let Some(bound) = bindings.get(next) {
        bound.clone()
    } else if bindings.get(prev).map(|s| s.eq_ignore_ascii_case(&active_sound)) == Some(true) {
        STOCK_LABEL.to_string()
    } else {
        active_sound.clone()
    };

    if desired.eq_ignore_ascii_case(&active_sound) {
        return Ok(()); // nothing to do
    }
    // Only apply if we can: Stock always, or a variant folder that actually exists.
    let can_apply = desired.eq_ignore_ascii_case(STOCK_LABEL)
        || dir_exists(&variant_dir(mods_path, bike, &desired));
    if !can_apply {
        return Ok(());
    }
    apply_sound_swap(mods_path, bike, &desired)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-snd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }
    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }
    // A complete loose sound set at the bike root.
    fn touch_sound(dir: &Path) {
        touch(&dir.join("engine.scl"));
        touch(&dir.join("sfx.cfg"));
    }

    // Print the bike tree (sorted) so the walkthrough is legible with --nocapture.
    fn tree(mods_path: &str, bike: &str, note: &str) {
        eprintln!("\n── {note} ─────────────────────────────────");
        eprintln!("active model: {}", crate::modelswap::current_active(mods_path, bike));
        eprintln!("active sound: {}", active_label(mods_path, bike));
        let mut lines: Vec<String> = walkdir::WalkDir::new(bike_dir(mods_path, bike))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                e.path()
                    .strip_prefix(bikes_root(mods_path))
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .collect();
        lines.sort();
        for l in lines {
            eprintln!("  {l}");
        }
    }

    fn has(mods_path: &str, bike: &str, rel: &str) -> bool {
        bike_dir(mods_path, bike)
            .join(rel.replace('/', &std::path::MAIN_SEPARATOR.to_string()))
            .is_file()
    }

    // A model-swap variant folder (`<Bike>/FrostMod Models/<name>/`).
    fn model_variant(mods_path: &str, bike: &str, name: &str) -> PathBuf {
        bike_dir(mods_path, bike).join("FrostMod Models").join(name)
    }

    /// Full user story on a real on-disk tree, asserting AND dumping the tree at each
    /// step. Run with: `cargo test full_scenario -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn full_scenario_walkthrough() {
        let root = std::env::temp_dir().join("frost-snd-scenario");
        let _ = fs::remove_dir_all(&root);
        let mp = root.to_str().unwrap();
        let bike = "KTM450SXF";

        // A freshly-installed, extracted stock bike: model files + built-in sound loose.
        touch(&bike_dir(mp, bike).join("model.edf"));
        touch(&bike_dir(mp, bike).join("KTM450SXF.cfg"));
        touch(&bike_dir(mp, bike).join("paints").join("Red.pnt"));
        touch_sound(&bike_dir(mp, bike)); // stock engine.scl + sfx.cfg
        write_active(mp, bike, STOCK_LABEL).unwrap();
        // Two installed sound mods parked in the library, and one model variant.
        touch_sound(&variant_dir(mp, bike, "Braaap"));
        touch(&variant_dir(mp, bike, "Braaap").join("idle.wav"));
        touch_sound(&variant_dir(mp, bike, "FourStroke"));
        touch(&model_variant(mp, bike, "Factory").join("model.edf"));
        tree(mp, bike, "start: stock sound, one Factory model in library");

        // Case 3 — switch sounds freely (independent of any model).
        apply_sound_swap(mp, bike, "Braaap").unwrap();
        tree(mp, bike, "case 3: swapped sound → Braaap (model untouched)");
        assert!(has(mp, bike, "idle.wav") && has(mp, bike, "engine.scl"));
        assert!(has(mp, bike, "model.edf"), "model preserved by sound swap");

        // Case 1 — swap the MODEL while an independent sound is active → sound preserved.
        let prev = crate::modelswap::current_active(mp, bike);
        crate::modelswap::apply_model_swap(mp, bike, "Factory").unwrap();
        reconcile_after_model_swap(mp, bike, &prev, "Factory").unwrap();
        tree(mp, bike, "case 1: swapped model → Factory (Braaap sound stays)");
        assert_eq!(active_label(mp, bike), "Braaap", "independent sound preserved");
        assert!(has(mp, bike, "idle.wav"));
        assert!(!variant_dir(mp, bike, "Original").join("engine.scl").is_file(),
            "model backup must NOT contain sound files");

        // Case 4 — tie the active sound to the active model (still standalone in library).
        let model_now = crate::modelswap::current_active(mp, bike);
        bind_sound(mp, bike, &model_now, "Braaap").unwrap();
        tree(mp, bike, "case 4: tied Braaap → Factory (metadata only)");
        assert_eq!(load_bindings(mp, bike).get("Factory").map(String::as_str), Some("Braaap"));

        // Case 2 — leave Factory for a model with no binding → the tied sound travels away.
        let prev = crate::modelswap::current_active(mp, bike);
        crate::modelswap::apply_model_swap(mp, bike, "Original").unwrap();
        reconcile_after_model_swap(mp, bike, &prev, "Original").unwrap();
        tree(mp, bike, "case 2: back to Original → Braaap reverts to Stock");
        assert_eq!(active_label(mp, bike), STOCK_LABEL, "tied sound left with its model");
        assert!(variant_dir(mp, bike, "Braaap").join("idle.wav").is_file(), "Braaap parked");

        // Case 2 (return) — re-enter Factory → its bound sound is pulled back in.
        let prev = crate::modelswap::current_active(mp, bike);
        crate::modelswap::apply_model_swap(mp, bike, "Factory").unwrap();
        reconcile_after_model_swap(mp, bike, &prev, "Factory").unwrap();
        tree(mp, bike, "case 2: re-enter Factory → Braaap pulled back");
        assert_eq!(active_label(mp, bike), "Braaap");
        assert!(has(mp, bike, "idle.wav"));

        eprintln!("\n✔ scenario tree left at: {}", root.display());
    }

    #[test]
    fn is_sound_file_matches_must_and_audio() {
        assert!(is_sound_file("engine.scl"));
        assert!(is_sound_file("SFX.CFG"));
        assert!(is_sound_file("idle.wav"));
        assert!(is_sound_file("Rev.MP3"));
        assert!(!is_sound_file("model.edf"));
        assert!(!is_sound_file("KTM.cfg"));
    }

    #[test]
    fn scan_lists_active_first_and_synthesizes_stock() {
        let root = tmp("scan");
        let mp = root.to_str().unwrap();
        touch_sound(&bike_dir(mp, "KTM450")); // loose active sound
        touch(&bike_dir(mp, "KTM450").join("engine.wav")); // optional audio
        touch_sound(&variant_dir(mp, "KTM450", "Roar"));
        write_active(mp, "KTM450", "Loud").unwrap();
        // active marker names "Loud" but its set is the loose files.

        let bikes = scan_sound_swaps(mp);
        assert_eq!(bikes.len(), 1);
        let b = &bikes[0];
        assert_eq!(b.active, "Loud");
        assert!(b.variants[0].active && b.variants[0].valid);
        assert_eq!(b.variants[0].file_count, 3); // engine.scl + sfx.cfg + engine.wav
        let names: Vec<_> = b.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"Roar"));
        assert!(names.contains(&"Stock"), "a Stock row is always present");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_backs_up_current_and_brings_target_in() {
        let root = tmp("apply");
        let mp = root.to_str().unwrap();
        touch_sound(&bike_dir(mp, "KTM")); // Stock-era loose sound
        touch(&bike_dir(mp, "KTM").join("model.edf")); // model must not move
        touch_sound(&variant_dir(mp, "KTM", "Roar"));
        touch(&variant_dir(mp, "KTM", "Roar").join("rev.wav"));

        apply_sound_swap(mp, "KTM", "Roar").unwrap();

        assert_eq!(read_active(mp, "KTM"), "Roar");
        // Roar's files are now loose at the bike root; the model stayed put.
        assert!(bike_dir(mp, "KTM").join("rev.wav").is_file());
        assert!(bike_dir(mp, "KTM").join("model.edf").is_file());
        // The prior loose sound was parked under "Stock".
        assert!(variant_dir(mp, "KTM", "Stock").join("engine.scl").is_file());
        // Roar's library folder is emptied of its set.
        assert!(!variant_dir(mp, "KTM", "Roar").join("engine.scl").is_file());

        // Revert to Stock removes the loose sound again.
        apply_sound_swap(mp, "KTM", "Stock").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Stock");
        assert!(!bike_dir(mp, "KTM").join("rev.wav").exists());
        assert!(variant_dir(mp, "KTM", "Roar").join("engine.scl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_rejects_active_missing_and_incomplete() {
        let root = tmp("reject");
        let mp = root.to_str().unwrap();
        touch_sound(&bike_dir(mp, "KTM"));
        write_active(mp, "KTM", "Stock").unwrap();
        // Already active.
        assert!(apply_sound_swap(mp, "KTM", "Stock").is_err());
        // Missing variant.
        assert!(apply_sound_swap(mp, "KTM", "Nope").is_err());
        // Files but no must-file = incomplete.
        touch(&variant_dir(mp, "KTM", "Bad").join("idle.wav"));
        assert!(apply_sound_swap(mp, "KTM", "Bad").is_err());
        // Path traversal refused.
        assert!(apply_sound_swap(mp, "KTM", "../evil").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    // ----- Reconciliation (the four cases) -----

    #[test]
    fn case1_independent_sound_survives_model_swap() {
        // No bindings, active sound Roar. Swapping models leaves the sound alone.
        let root = tmp("case1");
        let mp = root.to_str().unwrap();
        touch_sound(&bike_dir(mp, "KTM"));
        write_active(mp, "KTM", "Roar").unwrap();

        reconcile_after_model_swap(mp, "KTM", "Original", "Factory").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Roar", "sound preserved");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case2_bound_sound_travels_with_model() {
        let root = tmp("case2");
        let mp = root.to_str().unwrap();
        // Factory is bound to Roar; Roar is the active loose sound.
        touch_sound(&bike_dir(mp, "KTM"));
        write_active(mp, "KTM", "Roar").unwrap();
        bind_sound(mp, "KTM", "Factory", "Roar").unwrap();
        // A Stock parking spot so Roar can be backed up when we leave Factory.
        // (apply_sound_swap creates the backup dir on demand.)

        // Leaving Factory (whose bound sound Roar is active) for an unbound model
        // reverts the sound to Stock — it travels away with Factory.
        reconcile_after_model_swap(mp, "KTM", "Factory", "Original").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Stock");
        assert!(variant_dir(mp, "KTM", "Roar").join("engine.scl").is_file());

        // Entering Factory again brings Roar back.
        reconcile_after_model_swap(mp, "KTM", "Original", "Factory").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Roar");
        assert!(bike_dir(mp, "KTM").join("engine.scl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case2_entering_bound_model_applies_its_sound() {
        let root = tmp("case2b");
        let mp = root.to_str().unwrap();
        // Active is Stock; Race model is bound to Quiet, parked in the library.
        touch(&bike_dir(mp, "KTM").join("model.edf"));
        write_active(mp, "KTM", "Stock").unwrap();
        touch_sound(&variant_dir(mp, "KTM", "Quiet"));
        bind_sound(mp, "KTM", "Race", "Quiet").unwrap();

        reconcile_after_model_swap(mp, "KTM", "Original", "Race").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Quiet");
        assert!(bike_dir(mp, "KTM").join("engine.scl").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_unbound_active_sound_not_owned_by_prev_is_kept() {
        // Prev model IS bound, but to a different sound than the active one — the
        // active (independent) sound must be preserved, not reverted.
        let root = tmp("keep");
        let mp = root.to_str().unwrap();
        touch_sound(&bike_dir(mp, "KTM"));
        write_active(mp, "KTM", "Roar").unwrap();
        bind_sound(mp, "KTM", "Factory", "Quiet").unwrap(); // Factory->Quiet, not Roar

        reconcile_after_model_swap(mp, "KTM", "Factory", "Original").unwrap();
        assert_eq!(read_active(mp, "KTM"), "Roar", "independent sound kept");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bind_and_unbind_roundtrip() {
        let root = tmp("bind");
        let mp = root.to_str().unwrap();
        touch_sound(&bike_dir(mp, "KTM"));
        bind_sound(mp, "KTM", "Factory", "Roar").unwrap();
        assert_eq!(load_bindings(mp, "KTM").get("Factory").map(String::as_str), Some("Roar"));
        unbind_sound(mp, "KTM", "Factory").unwrap();
        assert!(load_bindings(mp, "KTM").get("Factory").is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
