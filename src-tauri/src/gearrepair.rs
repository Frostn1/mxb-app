//! Gathering a gear model that was installed loose into the folder it needs.
//!
//! The game reads a model as a folder — `mods/rider/helmets/<Model>/helmet.edf` — and every
//! picker in this app lists the folders an area holds. A model whose files sit directly in
//! the area root is therefore invisible twice over: the game can't load it, and the app
//! can't offer it. Worse, the `paints/` and `goggles/` folders that came with it are read as
//! models of their own, so the helmet picker fills up with entries called "paints".
//!
//! Installs before the placement fix in [`crate::install::gear_model_folder`] could leave
//! exactly that, so finding it and undoing it is this module's whole job. It only ever moves
//! things the game cannot read where they are:
//!
//! * loose files in an area root — nothing there is loadable, whatever it is;
//! * a `paints/` or `goggles/` folder in an area root — those belong to a model.
//!
//! A `.pkz` is left alone: a packaged model *is* a valid entry in an area root. So is any
//! other sub-folder, because that is a model already.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The sub-folders a model carries. Found in an *area* root they are strays that belong to
/// whichever model was scattered there — the game reads neither.
const MODEL_SUBDIRS: [&str; 2] = ["paints", "goggles"];

/// One area root that needs gathering, and what gathering it would move.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GearRepair {
    /// The area folder under `mods/rider`, e.g. `helmets`.
    pub area: String,
    /// The folder the loose content will be gathered into.
    pub model: String,
    /// Absolute path of that folder, so the frontend can name it exactly.
    pub dest: String,
    /// What moves, by name, in the order it will move. Preview and apply read the same list.
    pub items: Vec<String>,
}

/// Whether a directory entry is a model sub-folder that shouldn't be at an area root.
fn is_model_subdir(name: &str) -> bool {
    MODEL_SUBDIRS.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// The name to gather under.
///
/// A gear mod's descriptor `.ini` is named for the mod (`Astars_SM10_EKS.ini`), which is the
/// name its author chose and the one the picker should show. Nothing else in the folder
/// carries it — the mesh, the `.hrc` and `cameras.cfg` are all named for the slot, so
/// `helmet.edf` would name every helmet ever recovered the same thing. With no `.ini` to
/// read, say plainly that this was recovered rather than inventing a brand name.
fn recovered_name(files: &[PathBuf], area: &str) -> String {
    let ini = files
        .iter()
        .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ini")))
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match ini {
        Some(name) => crate::install::sanitize(name),
        // `helmets` → `helmet`; `protections` → `protection`. Crude, and only ever seen by
        // someone whose mod shipped no descriptor at all.
        None => format!("Recovered {}", area.strip_suffix('s').unwrap_or(area)),
    }
}

/// What gathering `dir` would move, or `None` when it is already tidy.
fn plan_area(dir: &Path, area: &str) -> Option<GearRepair> {
    let rd = std::fs::read_dir(dir).ok()?;
    let (mut files, mut subdirs) = (Vec::new(), Vec::new());
    for e in rd.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        // `fs::metadata` on the path, not `DirEntry::file_type`/`DirEntry::metadata`: both of
        // those describe the link itself, so a symlinked `paints/` reads as neither dir nor
        // file and would be skipped without a word. A mods tree assembled out of symlinks is
        // normal here — that's what `linkwalk` exists for.
        match std::fs::metadata(&p) {
            Ok(t) if t.is_dir() => {
                if is_model_subdir(&name) {
                    subdirs.push((name, p));
                }
            }
            Ok(t) if t.is_file() => {
                // A packaged model belongs here; junk is not worth moving and not worth
                // reporting as if it were someone's helmet.
                let packaged = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkz"));
                if !packaged && !crate::install::is_junk(&name) {
                    files.push(p);
                }
            }
            _ => {}
        }
    }
    if files.is_empty() && subdirs.is_empty() {
        return None;
    }
    // Deterministic, so the preview a user reads is the order the apply walks.
    files.sort();
    subdirs.sort();
    let model = recovered_name(&files, area);
    let items = files
        .iter()
        .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
        .chain(subdirs.iter().map(|(n, _)| n.clone()))
        .collect();
    Some(GearRepair {
        area: area.to_string(),
        model: model.clone(),
        dest: dir.join(&model).to_string_lossy().into_owned(),
        items,
    })
}

/// Every gear area under `mods/rider` holding content the game can't reach where it is.
pub fn plan(mods_path: &str) -> Vec<GearRepair> {
    let base = crate::library::mods_subdir(mods_path, "mods/rider");
    crate::game::RIDER_MODEL_AREAS
        .iter()
        // A profile is picked by name from `riders/` and its own loose files are read there,
        // so that area is not scattered in the sense this repairs.
        .filter(|a| !a.eq_ignore_ascii_case("riders"))
        .filter_map(|area| plan_area(&base.join(area), area))
        .collect()
}

/// Carry out one plan, returning how many entries moved.
///
/// Re-planned rather than trusting the caller's copy: a preview can be minutes old, and the
/// only thing worth acting on is what is in the folder now.
pub fn apply_one(mods_path: &str, area: &str) -> anyhow::Result<usize> {
    use anyhow::Context;
    let base = crate::library::mods_subdir(mods_path, "mods/rider");
    let dir = base.join(area);
    let Some(plan) = plan_area(&dir, area) else {
        return Ok(0);
    };
    let dest = dir.join(&plan.model);
    // A second run after a half-finished first one must merge into the folder it made, not
    // fail on it — so `create_dir_all`, and a per-entry existence check below.
    std::fs::create_dir_all(&dest)
        .with_context(|| format!("create {dest:?}"))?;
    let mut moved = 0;
    for item in &plan.items {
        let from = dir.join(item);
        let to = dest.join(item);
        if to.exists() {
            log::warn!("[gearrepair] {area}: '{item}' already exists in {dest:?} — left alone");
            continue;
        }
        std::fs::rename(&from, &to).with_context(|| format!("move {from:?} -> {to:?}"))?;
        moved += 1;
    }
    log::info!("[gearrepair] gathered {moved} entries into {dest:?}");
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("frost-gearrepair-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// The shape a pre-fix install left behind: the mesh loose in the area root with the
    /// model's `paints/` and `goggles/` beside it, named from the descriptor `.ini`.
    #[test]
    fn gathers_a_scattered_helmet_under_its_own_name() {
        let root = tmp("scattered");
        let helmets = root.join("mods/rider/helmets");
        for f in ["gfx.cfg", "helmet.edf", "helmet.hrc", "Astars_SM10_EKS.ini"] {
            touch(&helmets.join(f));
        }
        touch(&helmets.join("paints/Red.pnt"));
        touch(&helmets.join("goggles/Smoke.pnt"));

        let plans = plan(root.to_str().unwrap());
        assert_eq!(plans.len(), 1, "one area needs gathering");
        assert_eq!(plans[0].model, "Astars_SM10_EKS", "named from the descriptor");
        assert_eq!(plans[0].area, "helmets");

        let moved = apply_one(root.to_str().unwrap(), "helmets").unwrap();
        assert_eq!(moved, 6);
        let model = helmets.join("Astars_SM10_EKS");
        assert!(model.join("helmet.edf").exists());
        assert!(model.join("paints/Red.pnt").exists());
        assert!(model.join("goggles/Smoke.pnt").exists());
        assert!(!helmets.join("helmet.edf").exists(), "nothing left loose");
        assert!(!helmets.join("paints").exists(), "no stray `paints` sibling");
        assert!(plan(root.to_str().unwrap()).is_empty(), "and it stays tidy");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A packaged model and a properly-installed one are what a healthy area looks like.
    /// Neither is loose content, so neither is touched — and nothing is reported.
    #[test]
    fn a_tidy_area_is_left_alone() {
        let root = tmp("tidy");
        let helmets = root.join("mods/rider/helmets");
        touch(&helmets.join("Fox V3/helmet.edf"));
        touch(&helmets.join("Fox V3/paints/Blue.pnt"));
        touch(&helmets.join("Airoh.pkz"));
        touch(&helmets.join("readme.txt"));
        assert!(plan(root.to_str().unwrap()).is_empty());
        assert_eq!(apply_one(root.to_str().unwrap(), "helmets").unwrap(), 0);
        assert!(helmets.join("Airoh.pkz").exists(), "a packaged model stays put");
        assert!(helmets.join("Fox V3/helmet.edf").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// No descriptor to read. Still gathered — loose files are unloadable either way — but
    /// named for what happened rather than after `helmet.edf`, which would name them all
    /// alike.
    #[test]
    fn names_the_folder_plainly_when_no_descriptor_shipped() {
        let root = tmp("noini");
        let boots = root.join("mods/rider/boots");
        touch(&boots.join("gfx.cfg"));
        touch(&boots.join("boots.edf"));
        let plans = plan(root.to_str().unwrap());
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].model, "Recovered boot");
        apply_one(root.to_str().unwrap(), "boots").unwrap();
        assert!(boots.join("Recovered boot/boots.edf").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Applying twice must not throw away the second copy of a name — leave it and say so,
    /// rather than overwriting what is already filed.
    #[test]
    fn a_name_that_already_exists_is_never_overwritten() {
        let root = tmp("collide");
        let helmets = root.join("mods/rider/helmets");
        touch(&helmets.join("Fox.ini"));
        touch(&helmets.join("helmet.edf"));
        touch(&helmets.join("Fox/helmet.edf"));
        std::fs::write(helmets.join("Fox/helmet.edf"), b"the one already filed").unwrap();
        apply_one(root.to_str().unwrap(), "helmets").unwrap();
        assert_eq!(
            std::fs::read(helmets.join("Fox/helmet.edf")).unwrap(),
            b"the one already filed",
            "the filed model wins"
        );
        assert!(helmets.join("helmet.edf").exists(), "and the loose one is still there");
        let _ = std::fs::remove_dir_all(&root);
    }
}
