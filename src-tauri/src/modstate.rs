//! Which installed mods the game is allowed to see.
//!
//! MX Bikes mounts every `.pkz` under `<MX Bikes>/mods` at startup, so a big library is
//! paid for on every load — even though a race needs one track, one bike, one gear set and
//! a couple of support packs. Disabling a mod here means *moving it out of the way*: into
//! `<MX Bikes>/mxbapp_disabled`, mirroring its path under `mods` so putting it back is
//! exact.
//!
//! Rider gear paints ride along for a different reason. A loose `.pnt` costs nothing to
//! mount, but it is a row in the game's own gear pickers — and a preset that names one
//! helmet paint means the other four hundred should be out of sight, not merely cheap.
//!
//! The mirrored path is the whole record. There's no manifest to fall out of step with the
//! disk when app data is wiped, a folder is moved by hand, or two copies of the app run at
//! once — restoring is just "walk the shadow tree and move everything back".
//!
//! Same volume, so each move is a rename: taking 400 mods out of the way costs milliseconds.

use crate::bundle;
use crate::config::AppConfig;
use crate::library::{self, LibraryEntry};
use crate::presets::Preset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Folder that holds disabled mods, as a child of the MX Bikes root. Deliberately outside
/// `mods/`: the game never looks here, and neither does the mods watcher.
pub const SHADOW_DIR: &str = "mxbapp_disabled";

/// The content folders Manage governs for `cfg`'s active game, as `mods/…` subpaths.
/// Narrower than everything under `mods/` on purpose — see
/// [`crate::game::GameProfile::manage_dirs`].
fn subpaths(cfg: &AppConfig) -> Vec<String> {
    cfg.game().manage_dirs.iter().map(|d| format!("mods/{d}")).collect()
}

/// One mod as Manage sees it: where it lives relative to the MX Bikes root, and whether
/// the game can currently see it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    /// Path relative to the MX Bikes root, forward-slashed, *as if enabled* —
    /// `mods/tracks/EU/RedBud.pkz`. The stable identity of a mod across enable/disable.
    pub rel: String,
    pub name: String,
    /// Library category (`track`, `bike`, `bikePaint`, `helmet`, …).
    pub category: String,
    /// Folder inside its content type, e.g. `EU`. Empty at the top level.
    pub folder: String,
    pub size: u64,
    pub enabled: bool,
    /// An extracted track folder rather than a single archive.
    pub is_dir: bool,
}

/// What applying a preset's content would do, before anything moves.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatePlan {
    /// Mods that must be enabled for this preset, whether or not they already are.
    pub keep: Vec<ModEntry>,
    /// Currently enabled, and about to be moved out of the way.
    pub disable: Vec<ModEntry>,
    /// Currently disabled, and about to be brought back because the preset needs them.
    pub enable: Vec<ModEntry>,
    /// Slots the preset asks for that aren't installed — a helmet paint that was deleted,
    /// say. Straight from [`bundle::plan`], so the wording matches the share flow.
    pub unresolved: Vec<bundle::UnresolvedSlot>,
    /// MX Bikes is open. Moving files it holds open will fail, so the UI warns first.
    pub game_running: bool,
}

/// What actually happened. A single stubborn file doesn't abort the run — it's reported.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateOutcome {
    pub disabled: usize,
    pub enabled: usize,
    pub deleted: usize,
    /// `(rel, reason)` for every move that didn't happen.
    pub failed: Vec<(String, String)>,
}

impl StateOutcome {
    pub fn touched(&self) -> usize {
        self.disabled + self.enabled + self.deleted
    }
}

fn shadow_root(mods_path: &str) -> PathBuf {
    library::resolve_child(Path::new(mods_path.trim()), SHADOW_DIR)
}

/// `mods/tracks/EU/RedBud.pkz` → the path it occupies when enabled.
fn enabled_path(mods_path: &str, rel: &str) -> PathBuf {
    library::mods_subdir(mods_path, rel)
}

/// `mods/tracks/EU/RedBud.pkz` → `<root>/mxbapp_disabled/tracks/EU/RedBud.pkz`.
///
/// The leading `mods/` is dropped so the shadow tree reads like the content folder itself,
/// which matters the one time someone opens it in Explorer.
fn disabled_path(mods_path: &str, rel: &str) -> PathBuf {
    let mut p = shadow_root(mods_path);
    for seg in rel_segments(rel).skip(1) {
        p = library::resolve_child(&p, seg);
    }
    p
}

fn rel_segments(rel: &str) -> impl Iterator<Item = &str> {
    rel.split(['/', '\\']).filter(|s| !s.is_empty())
}

/// Normalize a rel path for comparison — case and slash direction both vary by where the
/// path came from (a scan, a saved preset, a share code written on another machine).
fn key(rel: &str) -> String {
    rel_segments(rel)
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

/// The path of `abs` relative to the MX Bikes root, forward-slashed. `None` when it sits
/// outside the root, which is the guard against a move escaping the content folders.
fn rel_of(mods_path: &str, abs: &Path) -> Option<String> {
    let root = Path::new(mods_path.trim());
    let rel = abs.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Rider gear a player picks a *model* of in-game: helmets, boots, protection. Installed
/// either as a folder or as a bare `.pkz`, and either way one row in the game's picker.
pub const GEAR_MODEL_CATEGORIES: [&str; 3] = ["helmet", "boots", "protection"];

/// Loose `.pnt` paints under `mods/rider` — the gear liveries the game lists once each.
///
/// Bike liveries are deliberately absent. They sit beside the model-swap and sound
/// bookkeeping that tracks them by path, and narrowing which bikes exist is already the bike
/// archive's job.
const RIDER_PAINT_CATEGORIES: [&str; 6] = [
    "helmetPaint",
    "goggles",
    "bootPaint",
    "protectionPaint",
    "gloves",
    "outfit",
];

/// Whether a library entry is something we're willing to move.
///
/// Three kinds of thing: archives the game mounts at boot, extracted tracks, and the rider
/// gear (models and their paints) the game offers in its own pickers. The first two are what
/// a load is waiting on; the third is what a race preset means when it says *this* helmet in
/// *this* paint — everything else should be out of sight, not merely cheap.
///
/// Still excluded: a `FrostMod Models` variant, a sound folder, a bike livery and a rider
/// profile, all of which are tracked by path elsewhere in the app and would desync if moved.
fn is_candidate(e: &LibraryEntry) -> bool {
    match e.kind.as_str() {
        "pkz" => true,
        "folder" => {
            e.category == "track" || GEAR_MODEL_CATEGORIES.contains(&e.category.as_str())
        }
        _ => RIDER_PAINT_CATEGORIES.contains(&e.category.as_str()),
    }
}

fn entry_from_library(mods_path: &str, e: &LibraryEntry, enabled: bool) -> Option<ModEntry> {
    Some(ModEntry {
        rel: rel_of(mods_path, Path::new(&e.path))?,
        name: e.name.clone(),
        category: e.category.clone(),
        folder: e.folder.clone(),
        size: e.size,
        enabled,
        is_dir: e.kind == "folder",
    })
}

/// Every mod Manage can act on, enabled and disabled alike.
///
/// The disabled half is found by scanning the shadow tree *as if it were the content
/// folder* — the same [`library::scan_library`] pass, so a disabled track is categorized
/// exactly like an enabled one and the two lists can sit next to each other.
pub fn scan(cfg: &AppConfig, sound_bikes: &[String]) -> Vec<ModEntry> {
    let mut out = Vec::new();

    for subpath in subpaths(cfg) {
        for e in library::scan_library(&cfg.mods_path, &subpath, sound_bikes, cfg.game())
            .unwrap_or_default()
            .iter()
            .filter(|e| is_candidate(e))
        {
            if let Some(m) = entry_from_library(&cfg.mods_path, e, true) {
                out.push(m);
            }
        }
    }

    let shadow = shadow_root(&cfg.mods_path);
    if shadow.is_dir() {
        let shadow_str = shadow.to_string_lossy().into_owned();
        for subpath in subpaths(cfg) {
            // `mods/tracks` under the real root is `tracks` under the shadow root.
            let leaf = subpath.trim_start_matches("mods/");
            for e in library::scan_library(&shadow_str, leaf, sound_bikes, cfg.game())
                .unwrap_or_default()
                .iter()
                .filter(|e| is_candidate(e))
            {
                // Report it under the path it will occupy once enabled, not where it's parked.
                let Some(parked) = rel_of(&shadow_str, Path::new(&e.path)) else {
                    continue;
                };
                out.push(ModEntry {
                    rel: format!("mods/{parked}"),
                    name: e.name.clone(),
                    category: e.category.clone(),
                    folder: e.folder.clone(),
                    size: e.size,
                    enabled: false,
                    is_dir: e.kind == "folder",
                });
            }
        }
    }

    out.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out.dedup_by(|a, b| key(&a.rel) == key(&b.rel));
    out
}

/// The mods a preset needs on disk.
///
/// Most of it comes free from [`bundle::plan`], which already resolves a loadout's paint,
/// helmet, goggles, suit, gloves, boots, protection, tyres and model swap to installed
/// files for the share-a-preset flow. The preset's own `content` adds what a loadout can't
/// express: the track, and the support packs the player pinned.
fn keep_keys(cfg: &AppConfig, preset: &Preset) -> (BTreeSet<String>, Vec<bundle::UnresolvedSlot>) {
    let mut keys = BTreeSet::new();
    let mut unresolved = Vec::new();

    if let Ok(plan) = bundle::plan_detailed(cfg, &preset.loadout) {
        for a in &plan.assets {
            keys.insert(key(&format!("mods/{}", a.rel_dest)));
        }
        unresolved = plan.unresolved;
    }

    if let Some(content) = preset.content.as_ref() {
        for rel in content.tracks.iter().chain(content.keep.iter()) {
            if !rel.trim().is_empty() {
                keys.insert(key(rel));
            }
        }
    }

    (keys, unresolved)
}

/// What [`apply`] would do, without doing it.
pub fn plan(cfg: &AppConfig, preset: &Preset, sound_bikes: &[String]) -> StatePlan {
    let (keys, unresolved) = keep_keys(cfg, preset);
    let mods = scan(cfg, sound_bikes);

    let mut out = StatePlan {
        unresolved,
        game_running: crate::gameproc::is_game_running(),
        ..Default::default()
    };

    for m in mods {
        let kept = is_kept(&keys, &m.rel);
        if kept {
            if !m.enabled {
                out.enable.push(m.clone());
            }
            out.keep.push(m);
        } else if m.enabled {
            out.disable.push(m);
        }
    }
    out
}

/// Is this mod in the keep set? Either named outright, or living inside a kept folder — a
/// model-swap variant is kept as `mods/bikes/KTM/FrostMod Models/2024`, and the archives
/// beneath it come along.
fn is_kept(keys: &BTreeSet<String>, rel: &str) -> bool {
    let k = key(rel);
    if keys.contains(&k) {
        return true;
    }
    // A paint is kept only when it is named outright. Falling through to the folder rule
    // would let one kept helmet drag in every livery installed under it — the opposite of
    // what choosing a paint means.
    if k.ends_with(".pnt") {
        return false;
    }
    keys.iter().any(|kept| k.starts_with(&format!("{kept}/")))
}

/// Move one mod, in whichever direction. Returns whether anything actually moved.
fn set_one(mods_path: &str, rel: &str, enabled: bool) -> anyhow::Result<bool> {
    let from = if enabled {
        disabled_path(mods_path, rel)
    } else {
        enabled_path(mods_path, rel)
    };
    let to = if enabled {
        enabled_path(mods_path, rel)
    } else {
        disabled_path(mods_path, rel)
    };

    // Already where it's wanted, or never there to begin with. Not an error: a plan can be
    // a few seconds stale, and a no-op is the right answer either way.
    if !from.exists() {
        return Ok(false);
    }
    if to.exists() {
        anyhow::bail!(
            "something is already at {} — left this one alone",
            to.display()
        );
    }

    guard_inside(mods_path, &from)?;
    guard_inside(mods_path, &to)?;

    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    // Rename within the volume; the copy fallback is for the exotic case of a mods folder
    // spanning a mount point.
    if fs::rename(&from, &to).is_err() {
        copy_tree(&from, &to)?;
        if from.is_dir() {
            fs::remove_dir_all(&from)?;
        } else {
            fs::remove_file(&from)?;
        }
    }
    prune_empty(mods_path, from.parent());
    Ok(true)
}

/// Refuse to touch anything outside the MX Bikes root — the same guard `library::move_mod`
/// and `library::uninstall_mod` apply, and the reason a hand-edited preset can't be used to
/// move files around the disk.
fn guard_inside(mods_path: &str, p: &Path) -> anyhow::Result<()> {
    let root = Path::new(mods_path.trim());
    if root.as_os_str().is_empty() {
        anyhow::bail!("no MX Bikes folder configured");
    }
    if !p.starts_with(root) {
        anyhow::bail!("refusing to touch a path outside the MX Bikes folder");
    }
    if rel_segments(&p.to_string_lossy()).any(|s| s == "..") {
        anyhow::bail!("refusing a path that climbs out of the MX Bikes folder");
    }
    Ok(())
}

/// Drop the folders a move emptied, so disabling a whole tracks subfolder doesn't leave a
/// husk behind. Stops at the content roots and never touches anything with files in it.
fn prune_empty(mods_path: &str, dir: Option<&Path>) {
    let root = Path::new(mods_path.trim());
    let mut cur = dir.map(Path::to_path_buf);
    while let Some(d) = cur {
        if !d.starts_with(root) || d == root {
            return;
        }
        // `mods`, `mods/tracks`, `mxbapp_disabled/tracks` and friends stay put.
        if d.strip_prefix(root).map(|r| r.components().count() < 3).unwrap_or(true) {
            return;
        }
        if fs::read_dir(&d).map(|mut rd| rd.next().is_some()).unwrap_or(true) {
            return;
        }
        if fs::remove_dir(&d).is_err() {
            return;
        }
        cur = d.parent().map(Path::to_path_buf);
    }
}

fn copy_tree(from: &Path, to: &Path) -> anyhow::Result<()> {
    if from.is_file() {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        copy_tree(&e.path(), &to.join(e.file_name()))?;
    }
    Ok(())
}

/// Enable or disable a specific set of mods.
pub fn set_many(cfg: &AppConfig, rels: &[String], enabled: bool) -> StateOutcome {
    let mut out = StateOutcome::default();
    for rel in rels {
        match set_one(&cfg.mods_path, rel, enabled) {
            Ok(true) if enabled => out.enabled += 1,
            Ok(true) => out.disabled += 1,
            Ok(false) => {}
            Err(e) => out.failed.push((rel.clone(), format!("{e:#}"))),
        }
    }
    out
}

/// Send mods to the recycle bin, wherever they currently sit.
///
/// `library::uninstall_mod` already does this for an enabled mod, but it takes an absolute
/// path inside a content folder — a disabled mod is parked outside one, and Manage lists
/// both side by side. Same `trash::delete`, addressed by `rel` instead.
pub fn delete_many(cfg: &AppConfig, rels: &[String]) -> StateOutcome {
    let mut out = StateOutcome::default();
    for rel in rels {
        let mut target = enabled_path(&cfg.mods_path, rel);
        if !target.exists() {
            target = disabled_path(&cfg.mods_path, rel);
        }
        let result = guard_inside(&cfg.mods_path, &target).and_then(|()| {
            if !target.exists() {
                anyhow::bail!("not there any more");
            }
            trash::delete(&target)?;
            Ok(())
        });
        match result {
            Ok(()) => {
                out.deleted += 1;
                prune_empty(&cfg.mods_path, target.parent());
            }
            Err(e) => out.failed.push((rel.clone(), format!("{e:#}"))),
        }
    }
    out
}

/// Put the game in the state a preset's content describes: bring back what it needs, take
/// everything else out of the way.
pub fn apply(cfg: &AppConfig, plan: &StatePlan) -> StateOutcome {
    let enable: Vec<String> = plan.enable.iter().map(|m| m.rel.clone()).collect();
    let disable: Vec<String> = plan.disable.iter().map(|m| m.rel.clone()).collect();

    // Enable first: if a disable somehow fails partway, the player is at least left with
    // the content they asked for rather than a race missing its track.
    let mut out = set_many(cfg, &enable, true);
    let off = set_many(cfg, &disable, false);
    out.disabled += off.disabled;
    out.enabled += off.enabled;
    out.failed.extend(off.failed);
    out
}

#[cfg(test)]
mod delete_tests {
    use super::*;

    /// Delete has to reach a mod wherever Manage is showing it — parked mods are outside
    /// the content folders the Library's uninstall can address.
    #[test]
    fn deletes_a_parked_mod_too() {
        let root = std::env::temp_dir().join(format!("frost-modstate-del-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let parked = root.join("mxbapp_disabled/tracks/Old.pkz");
        fs::create_dir_all(parked.parent().unwrap()).unwrap();
        fs::write(&parked, b"x").unwrap();

        let cfg = AppConfig {
            mods_path: root.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let out = delete_many(&cfg, &["mods/tracks/Old.pkz".into()]);
        assert_eq!(out.deleted, 1, "{:?}", out.failed);
        assert!(!parked.exists());

        let _ = fs::remove_dir_all(&root);
    }
}

/// Everything back where it came from. Walks the shadow tree rather than a saved list, so
/// it works on mods disabled by a previous install of the app.
pub fn restore_all(cfg: &AppConfig) -> StateOutcome {
    let shadow = shadow_root(&cfg.mods_path);
    let mut out = StateOutcome::default();
    if !shadow.is_dir() {
        return out;
    }

    let mut rels: Vec<String> = Vec::new();
    for subpath in subpaths(cfg) {
        let leaf = subpath.trim_start_matches("mods/");
        collect_parked(
            &shadow,
            &cfg.mods_path,
            &library::resolve_child(&shadow, leaf),
            &mut rels,
        );
    }

    for rel in &rels {
        match set_one(&cfg.mods_path, rel, true) {
            Ok(true) => out.enabled += 1,
            Ok(false) => {}
            Err(e) => out.failed.push((rel.clone(), format!("{e:#}"))),
        }
    }

    // The shadow tree is scaffolding, not content — once it's empty it shouldn't linger.
    if out.failed.is_empty() {
        let _ = fs::remove_dir_all(&shadow);
    }
    out
}

/// Collect what's parked under `dir` as `mods/…` rel paths. A folder parked whole — an
/// extracted track, a gear model — is one item and isn't descended into; everything else
/// yields its archives and paints.
fn collect_parked(shadow: &Path, mods_path: &str, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Some(rel) = rel_of(&shadow.to_string_lossy(), &p) else {
            continue;
        };
        if p.is_file() {
            if is_parked_file(&p) {
                out.push(format!("mods/{rel}"));
            }
            continue;
        }
        // A directory we parked whole is indistinguishable from a grouping folder by name
        // alone — so descend, and let the leaf test below decide. That test is what keeps a
        // track's interior from being enumerated file by file.
        //
        // A gear model is the one folder that can be *half* parked: leave the helmet in
        // place, park the liveries under it. When the enabled folder is still there, the
        // shadow copy is only holding paints, so descend for those rather than queueing a
        // whole-folder move that would collide with it.
        let whole = if is_gear_model_dir(shadow, &p) {
            !enabled_path(mods_path, &format!("mods/{rel}")).exists()
        } else {
            dir_is_parked_mod(&p)
        };
        if whole {
            out.push(format!("mods/{rel}"));
        } else {
            collect_parked(shadow, mods_path, &p, out);
        }
    }
}

/// A file that was parked in its own right: an archive or a gear paint.
fn is_parked_file(p: &Path) -> bool {
    p.extension()
        .and_then(|x| x.to_str())
        .map(|x| x.eq_ignore_ascii_case("pkz") || x.eq_ignore_ascii_case("pnt"))
        .unwrap_or(false)
}

/// An extracted track parked in the shadow tree: a folder carrying the marker files the
/// game's track loader looks for.
fn dir_is_parked_mod(dir: &Path) -> bool {
    const MARKERS: [&str; 5] = ["map", "trh", "tsc", "rdf", "ssc"];
    let Ok(rd) = fs::read_dir(dir) else {
        return false;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                if MARKERS.contains(&ext.to_ascii_lowercase().as_str()) {
                    return true;
                }
            }
        }
    }
    false
}

/// `<shadow>/rider/helmets/AGV` — a gear model, one level under its area folder.
fn is_gear_model_dir(shadow: &Path, dir: &Path) -> bool {
    let Ok(rel) = dir.strip_prefix(shadow) else {
        return false;
    };
    let segs: Vec<String> = rel_segments(&rel.to_string_lossy())
        .map(|s| s.to_lowercase())
        .collect();
    segs.len() == 3
        && segs[0] == "rider"
        && matches!(segs[1].as_str(), "helmets" | "boots" | "protection")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{Loadout, PresetContent};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-modstate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn cfg_at(root: &Path) -> AppConfig {
        AppConfig {
            mods_path: root.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    fn preset(name: &str, content: PresetContent) -> Preset {
        Preset {
            name: name.into(),
            loadout: Loadout::default(),
            bundle: None,
            content: Some(content),
        }
    }

    /// The property the whole feature rests on: a mod comes back to the *exact* path it
    /// left, subfolder and all.
    #[test]
    fn disable_and_restore_round_trips_the_exact_path() {
        let root = tmp("round");
        let track = root.join("mods/tracks/EU/RedBud.pkz");
        touch(&track);
        let cfg = cfg_at(&root);

        let off = set_many(&cfg, &["mods/tracks/EU/RedBud.pkz".into()], false);
        assert_eq!(off.disabled, 1);
        assert!(!track.exists());
        assert!(root.join("mxbapp_disabled/tracks/EU/RedBud.pkz").is_file());
        // The emptied grouping folder goes with it.
        assert!(!root.join("mods/tracks/EU").exists());

        let on = restore_all(&cfg);
        assert_eq!(on.enabled, 1, "{:?}", on.failed);
        assert!(track.is_file());
        assert!(!root.join("mxbapp_disabled").exists());

        let _ = fs::remove_dir_all(&root);
    }

    /// A preset keeps its track and its pinned packs; everything else steps aside.
    #[test]
    fn a_preset_keeps_its_track_and_pinned_mods() {
        let root = tmp("plan");
        touch(&root.join("mods/tracks/RedBud.pkz"));
        touch(&root.join("mods/tracks/Practice.pkz"));
        touch(&root.join("mods/bikes/OEM Pack.pkz"));
        touch(&root.join("mods/bikes/Some Other Bike.pkz"));
        let cfg = cfg_at(&root);

        let p = preset(
            "Race",
            PresetContent {
                tracks: vec!["mods/tracks/RedBud.pkz".into()],
                keep: vec!["mods/bikes/OEM Pack.pkz".into()],
            },
        );

        let first = plan(&cfg, &p, &[]);
        assert_eq!(first.keep.len(), 2);
        let disabled: Vec<&str> = first.disable.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(disabled.len(), 2, "{disabled:?}");
        assert!(disabled.contains(&"Practice.pkz"));
        assert!(disabled.contains(&"Some Other Bike.pkz"));

        let out = apply(&cfg, &first);
        assert_eq!(out.disabled, 2, "{:?}", out.failed);
        assert!(root.join("mods/tracks/RedBud.pkz").is_file());
        assert!(root.join("mods/bikes/OEM Pack.pkz").is_file());
        assert!(!root.join("mods/tracks/Practice.pkz").exists());
        assert!(root.join("mxbapp_disabled/bikes/Some Other Bike.pkz").is_file());

        // Applying it again is a no-op, not an error.
        let again = apply(&cfg, &plan(&cfg, &p, &[]));
        assert_eq!(again.touched(), 0, "{:?}", again.failed);

        let _ = fs::remove_dir_all(&root);
    }

    /// A preset whose track is currently disabled brings it back rather than racing
    /// without it.
    #[test]
    fn applying_a_preset_re_enables_the_content_it_needs() {
        let root = tmp("re-enable");
        touch(&root.join("mxbapp_disabled/tracks/RedBud.pkz"));
        let cfg = cfg_at(&root);

        let p = preset(
            "Race",
            PresetContent {
                tracks: vec!["mods/tracks/RedBud.pkz".into()],
                keep: vec![],
            },
        );
        let plan = plan(&cfg, &p, &[]);
        assert_eq!(plan.enable.len(), 1);

        let out = apply(&cfg, &plan);
        assert_eq!(out.enabled, 1, "{:?}", out.failed);
        assert!(root.join("mods/tracks/RedBud.pkz").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    /// An extracted track is one mod, not a thousand files.
    #[test]
    fn an_extracted_track_moves_whole() {
        let root = tmp("extracted");
        let dir = root.join("mods/tracks/Milestone");
        touch(&dir.join("track.trh"));
        touch(&dir.join("terrain/heightmap.map"));
        let cfg = cfg_at(&root);

        let scanned = scan(&cfg, &[]);
        assert_eq!(scanned.len(), 1, "{scanned:?}");
        assert!(scanned[0].is_dir);

        set_many(&cfg, &[scanned[0].rel.clone()], false);
        assert!(root.join("mxbapp_disabled/tracks/Milestone/track.trh").is_file());
        assert!(root
            .join("mxbapp_disabled/tracks/Milestone/terrain/heightmap.map")
            .is_file());

        let listed = scan(&cfg, &[]);
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].enabled);
        assert_eq!(listed[0].rel, "mods/tracks/Milestone");

        let out = restore_all(&cfg);
        assert_eq!(out.enabled, 1, "{:?}", out.failed);
        assert!(dir.join("terrain/heightmap.map").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    /// A name collision is reported, never resolved by clobbering someone's mod.
    #[test]
    fn a_collision_is_reported_not_overwritten() {
        let root = tmp("collide");
        touch(&root.join("mods/tracks/RedBud.pkz"));
        touch(&root.join("mxbapp_disabled/tracks/RedBud.pkz"));
        let cfg = cfg_at(&root);

        let out = set_many(&cfg, &["mods/tracks/RedBud.pkz".into()], false);
        assert_eq!(out.disabled, 0);
        assert_eq!(out.failed.len(), 1);
        assert!(root.join("mods/tracks/RedBud.pkz").is_file(), "left alone");

        let _ = fs::remove_dir_all(&root);
    }

    /// A hand-edited preset can't be used to walk files out of the MX Bikes folder.
    #[test]
    fn refuses_to_move_anything_outside_the_game_folder() {
        let root = tmp("escape");
        let outside = root.join("elsewhere/secret.pkz");
        touch(&outside);
        let cfg = cfg_at(&root.join("game"));
        fs::create_dir_all(root.join("game/mods/tracks")).unwrap();

        let out = set_many(&cfg, &["../elsewhere/secret.pkz".into()], false);
        assert_eq!(out.disabled, 0);
        assert!(outside.is_file());

        let _ = fs::remove_dir_all(&root);
    }

    /// Bike liveries and model-swap sets are tracked by path elsewhere in the app — Manage
    /// leaves them where they are.
    #[test]
    fn leaves_bike_liveries_and_swap_sets_alone() {
        let root = tmp("loose");
        touch(&root.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&root.join("mods/bikes/KTM450/FrostMod Models/2024/bike.edf"));
        touch(&root.join("mods/bikes/OEM Pack.pkz"));
        let cfg = cfg_at(&root);

        let names: Vec<String> = scan(&cfg, &[]).iter().map(|m| m.name.clone()).collect();
        assert_eq!(names, vec!["OEM Pack.pkz"]);

        let _ = fs::remove_dir_all(&root);
    }

    fn gear_root(name: &str) -> PathBuf {
        let root = tmp(name);
        touch(&root.join("mods/rider/helmets/AGV/model.edf"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Wanted.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Other.pnt"));
        touch(&root.join("mods/rider/helmets/AGV/goggles/Tinted.pnt"));
        touch(&root.join("mods/rider/helmets/Fox/model.edf"));
        touch(&root.join("mods/rider/boots/Alpinestars/model.edf"));
        touch(&root.join("mods/rider/riders/default_mx/paints/Kit.pnt"));
        root
    }

    fn helmet_preset(helmet: &str, paint: &str, keep: Vec<String>) -> Preset {
        let loadout = Loadout {
            helmet: helmet.into(),
            helmet_paint: paint.into(),
            ..Default::default()
        };
        preset_with("Race", loadout, PresetContent { tracks: vec![], keep })
    }

    fn preset_with(name: &str, loadout: Loadout, content: PresetContent) -> Preset {
        Preset {
            name: name.into(),
            loadout,
            bundle: None,
            content: Some(content),
        }
    }

    /// The gear a preset names is the gear the game should offer. Every other helmet, boot,
    /// protection set and livery steps aside — including the other liveries of the very
    /// helmet the preset keeps, which is what a chosen paint means.
    #[test]
    fn a_race_preset_leaves_only_the_gear_it_names() {
        let root = gear_root("gear");
        let cfg = cfg_at(&root);
        let p = helmet_preset("AGV", "Wanted", vec![]);

        let plan = plan(&cfg, &p, &[]);
        let disabled: Vec<&str> = plan.disable.iter().map(|m| m.name.as_str()).collect();
        for gone in ["Other.pnt", "Tinted.pnt", "Kit.pnt", "Fox", "Alpinestars"] {
            assert!(disabled.contains(&gone), "{gone} should step aside: {disabled:?}");
        }
        assert!(!disabled.contains(&"Wanted.pnt"), "{disabled:?}");
        assert!(!disabled.contains(&"AGV"), "{disabled:?}");

        let out = apply(&cfg, &plan);
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        assert!(root.join("mods/rider/helmets/AGV/paints/Wanted.pnt").is_file());
        assert!(root.join("mods/rider/helmets/AGV/model.edf").is_file());
        assert!(!root.join("mods/rider/helmets/AGV/paints/Other.pnt").exists());
        assert!(!root.join("mods/rider/helmets/Fox").exists());
        assert!(root
            .join("mxbapp_disabled/rider/helmets/AGV/paints/Other.pnt")
            .is_file());
        assert!(root.join("mxbapp_disabled/rider/helmets/Fox/model.edf").is_file());

        // A half-parked helmet — folder in place, liveries parked — restores its paints
        // rather than colliding with the folder that never moved.
        let back = restore_all(&cfg);
        assert!(back.failed.is_empty(), "{:?}", back.failed);
        assert!(root.join("mods/rider/helmets/AGV/paints/Other.pnt").is_file());
        assert!(root.join("mods/rider/helmets/AGV/goggles/Tinted.pnt").is_file());
        assert!(root.join("mods/rider/helmets/Fox/model.edf").is_file());
        assert!(root.join("mods/rider/riders/default_mx/paints/Kit.pnt").is_file());
        assert!(!root.join("mxbapp_disabled").exists());

        let _ = fs::remove_dir_all(&root);
    }

    /// Extra models pinned in the content dialog are kept alongside the preset's own, so a
    /// player can still swap helmets mid-session without leaving race mode.
    #[test]
    fn pinned_gear_models_are_kept_too() {
        let root = gear_root("pinned");
        let cfg = cfg_at(&root);
        let p = helmet_preset("AGV", "Wanted", vec!["mods/rider/helmets/Fox".into()]);

        let plan = plan(&cfg, &p, &[]);
        let disabled: Vec<&str> = plan.disable.iter().map(|m| m.name.as_str()).collect();
        assert!(!disabled.contains(&"Fox"), "{disabled:?}");
        assert!(disabled.contains(&"Alpinestars"), "{disabled:?}");

        let _ = fs::remove_dir_all(&root);
    }
}
