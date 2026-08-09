//! Gathering a gear model that was installed loose into the folder it needs.
//!
//! The game reads a model as a folder — `mods/rider/helmets/<Model>/helmet.edf` — and every
//! picker in this app lists the folders an area holds. A model whose files sit directly in
//! the area root is therefore invisible twice over: the game can't load it, and the app
//! can't offer it. Worse, the `paints/` and `goggles/` folders that came with it are read as
//! models of their own, so the helmet picker fills up with entries called "paints".
//!
//! Installs before the placement fix in [`crate::install::gear_model_folder`] could leave
//! exactly that, so finding it and undoing it is this module's whole job — but only where a
//! model is genuinely what got scattered. That is the entry condition, not a detail of the
//! plan: see [`is_a_model`]. A `paints/`/`goggles/` pair with no mesh beside it is not a
//! scattered helmet at all, it is a *paint pack*, and paints for a packaged helmet have
//! nowhere else to live since nothing can write inside a `.pkz`. Gathering those would invent
//! a helmet out of liveries — a folder with no mesh, offered by every picker as a model, with
//! the real helmet's paints filed underneath where the loader will never look for them.
//!
//! Once a model is established, what moves with it is everything the game cannot read where
//! it sits:
//!
//! * the loose files in the area root — nothing there is loadable, whatever it is;
//! * a `paints/` or `goggles/` folder in the area root — those belong to a model.
//!
//! A `.pkz` is left alone: a packaged model *is* a valid entry in an area root. So is any
//! other sub-folder, because that is a model already.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The sub-folders a model carries. Found in an *area* root they are strays that belong to
/// whichever model was scattered there — the game reads neither.
const MODEL_SUBDIRS: [&str; 2] = ["paints", "goggles"];

/// A gear model *is* its mesh, and a mesh is an `.edf`. The stem varies by mod — `helmet.edf`,
/// `model.edf`, one per part — so the extension is the whole test.
///
/// Holds for sealed mods too: a creator-locked helmet is a folder of KCOL blobs that keep
/// their real names, so its 32 MB `helmet.edf` is still an `.edf` to everything here.
fn is_mesh(p: &Path) -> bool {
    p.extension().is_some_and(|x| x.eq_ignore_ascii_case("edf"))
}

/// Whether these loose files are a model rather than a pile of paints.
///
/// In a paintable area the mesh is the proof, because the thing being ruled out — a paint
/// pack — is made of exactly the files a model's own `paints/` would hold, and only the mesh
/// separates them. Riding-style `animations` carry no mesh and no paints either: the game
/// reads `animations/<name>/<name>.ini`, so the descriptor *is* the model there, and nothing
/// in that area could be a livery to begin with.
fn is_a_model(files: &[PathBuf], area: &str) -> bool {
    if crate::game::rider_area_is_paintable(area) {
        files.iter().any(|p| is_mesh(p))
    } else {
        !descriptors(files).is_empty()
    }
}

/// A mod's descriptor `.ini`, one per mod. Two of them in one area root means two mods were
/// scattered on top of each other, and nothing on disk says which file came from which.
fn descriptors(files: &[PathBuf]) -> Vec<&PathBuf> {
    files
        .iter()
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ini")))
        .collect()
}

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
    let ini = descriptors(files)
        .first()
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

    // No model, nothing to gather. Most often this is a paint pack for a helmet that is
    // already installed — commonly a packaged one, whose paints *have* to live outside the
    // archive. Those belong under the helmet they paint; there is nothing here that says
    // which, and filing them under a folder of their own would only bury them deeper. Left
    // where they are, and said out loud, because it is the shape most likely to be reported.
    if !is_a_model(&files, area) {
        log::info!(
            "[gearrepair] {area}: {} loose file(s) and {} stray folder(s), but nothing that \
             proves a model — not a scattered install, left alone",
            files.len(),
            subdirs.len(),
        );
        return None;
    }

    // One descriptor is a mod naming itself. Several means several mods were emptied into
    // the same folder, and no rule can say which mesh, paint or config came from which —
    // gathering them under one name would fuse them into a model that never existed.
    let inis = descriptors(&files);
    if inis.len() > 1 {
        log::warn!(
            "[gearrepair] {area}: {} descriptors ({:?}) — more than one mod scattered here, \
             so there is no safe way to gather them; left alone",
            inis.len(),
            inis.iter().filter_map(|p| p.file_name()).collect::<Vec<_>>(),
        );
        return None;
    }

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
///
/// All-or-nothing. Stopping at the first failure used to leave the mesh gathered and
/// `paints/`/`goggles/` still in the area root — the half-done state is *worse* than either
/// end of it, because the pickers then show the model and both strays side by side, which
/// reads as the repair having split the helmet in three. Windows makes that a live risk: a
/// file the running game holds open refuses to move. So anything already moved goes back.
///
/// No cross-device fallback, deliberately: `dest` is always a child of `dir`, so a rename
/// between them cannot cross a filesystem and copying would only add a way to half-write a
/// 32 MB sealed mesh.
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
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for item in &plan.items {
        let from = dir.join(item);
        let to = dest.join(item);
        if to.exists() {
            log::warn!("[gearrepair] {area}: '{item}' already exists in {dest:?} — left alone");
            continue;
        }
        if let Err(e) = std::fs::rename(&from, &to) {
            rollback(&done);
            return Err(anyhow::Error::new(e))
                .with_context(|| format!("move {from:?} -> {to:?} (nothing was moved)"));
        }
        done.push((from, to));
    }
    log::info!("[gearrepair] gathered {} entries into {dest:?}", done.len());
    Ok(done.len())
}

/// Put back what a failed gather had already moved, best-effort.
///
/// In reverse, so the area root is rebuilt in the order it came apart. A rollback that itself
/// fails is logged and no more: the original error is the one worth reporting, and there is
/// nothing further this can do about a folder the OS won't let it touch.
fn rollback(done: &[(PathBuf, PathBuf)]) {
    for (from, to) in done.iter().rev() {
        if let Err(e) = std::fs::rename(to, from) {
            log::error!("[gearrepair] rollback failed: {to:?} -> {from:?}: {e}");
        }
    }
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

    /// The shape this repair used to get wrong. A paint pack for a *packaged* helmet has
    /// nowhere to live but outside the archive, so `paints/` and `goggles/` beside a `.pkz`
    /// with no mesh anywhere is a normal thing to find — and gathering it would invent a
    /// mesh-less helmet, offer it in every picker, and bury the real helmet's liveries under
    /// a name the loader never looks for.
    #[test]
    fn a_paint_pack_with_no_mesh_is_never_gathered() {
        let root = tmp("paintsonly");
        let helmets = root.join("mods/rider/helmets");
        touch(&helmets.join("Astars_SM10_EKS.pkz"));
        touch(&helmets.join("paints/Red.pnt"));
        touch(&helmets.join("goggles/Smoke.pnt"));

        assert!(plan(root.to_str().unwrap()).is_empty(), "no mesh, so nothing to gather");
        assert_eq!(apply_one(root.to_str().unwrap(), "helmets").unwrap(), 0);
        assert!(helmets.join("paints/Red.pnt").exists(), "the pack is left exactly as found");
        assert!(helmets.join("goggles/Smoke.pnt").exists());
        assert!(!helmets.join("Recovered helmet").exists(), "and no helmet is invented");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Riding styles are models with no mesh — the game reads `animations/<name>/<name>.ini`
    /// — and no paints either, so the mesh rule must not reach them. One scattered here is
    /// still gathered, on the strength of its descriptor.
    #[test]
    fn a_scattered_riding_style_is_still_gathered() {
        let root = tmp("animations");
        let anims = root.join("mods/rider/animations");
        touch(&anims.join("Scrub.ini"));
        touch(&anims.join("scrub.cfg"));

        let plans = plan(root.to_str().unwrap());
        assert_eq!(plans.len(), 1, "a mesh-less area still repairs");
        assert_eq!(plans[0].model, "Scrub");
        assert_eq!(apply_one(root.to_str().unwrap(), "animations").unwrap(), 2);
        assert!(anims.join("Scrub/Scrub.ini").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A loose `.pnt` or two in the area root is the same story without the folders: paints,
    /// misfiled, belonging to a helmet that already exists.
    #[test]
    fn loose_paints_alone_are_not_a_model() {
        let root = tmp("loosepnt");
        let helmets = root.join("mods/rider/helmets");
        touch(&helmets.join("Red.pnt"));
        touch(&helmets.join("Blue.pnt"));
        assert!(plan(root.to_str().unwrap()).is_empty());
        assert!(helmets.join("Red.pnt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two mods emptied into the same area root. Both are scattered and both need gathering,
    /// but nothing on disk says which mesh went with which descriptor — so gathering them
    /// under one name would fuse two helmets into one that never existed. Refused instead.
    #[test]
    fn two_scattered_mods_are_left_alone_rather_than_fused() {
        let root = tmp("twomods");
        let helmets = root.join("mods/rider/helmets");
        for f in ["Fox_V3.ini", "Airoh_Twist.ini", "helmet.edf", "gfx.cfg"] {
            touch(&helmets.join(f));
        }
        assert!(plan(root.to_str().unwrap()).is_empty(), "two descriptors, no safe gather");
        assert_eq!(apply_one(root.to_str().unwrap(), "helmets").unwrap(), 0);
        assert!(helmets.join("helmet.edf").exists(), "and nothing moved");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A move that fails part-way must not leave the mesh gathered and the strays behind —
    /// that state shows the model *and* `paints` in the picker, which is exactly what a
    /// player reads as "the repair split my helmet up".
    ///
    /// Unix-only because it needs a rename to fail on demand: a directory with no write
    /// permission of its own cannot be moved to a new parent, since the move has to rewrite
    /// its `..`. Windows fails the same way for the real reason — a file the game holds open.
    #[cfg(unix)]
    #[test]
    fn a_move_that_fails_part_way_puts_everything_back() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp("rollback");
        let helmets = root.join("mods/rider/helmets");
        touch(&helmets.join("Fox_V3.ini"));
        touch(&helmets.join("helmet.edf"));
        touch(&helmets.join("paints/Red.pnt"));
        // Files sort before folders in the plan, so both move before this one refuses.
        std::fs::set_permissions(helmets.join("paints"), std::fs::Permissions::from_mode(0o500))
            .unwrap();

        let err = apply_one(root.to_str().unwrap(), "helmets").unwrap_err();
        assert!(format!("{err:#}").contains("nothing was moved"), "and it says so: {err:#}");

        assert!(helmets.join("Fox_V3.ini").exists(), "descriptor back at the area root");
        assert!(helmets.join("helmet.edf").exists(), "mesh back at the area root");
        assert!(helmets.join("paints/Red.pnt").exists(), "strays never left");
        assert!(!helmets.join("Fox_V3/helmet.edf").exists(), "and nothing is half-gathered");

        std::fs::set_permissions(helmets.join("paints"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
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
