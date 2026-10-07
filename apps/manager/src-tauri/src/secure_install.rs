#![cfg_attr(not(mxbsecure), allow(dead_code))]
//! Putting an unlocked `.mxbsecure` file where MX Bikes reads it.
//!
//! Unlocking provisions a key beside the blob, wherever the blob is. The game only ever asks
//! for content inside its mods tree, and the secure DLL serves a file only from the same folder
//! the game asks in, so a paint unlocked in Downloads never shows. When the picked blob is
//! outside the mods tree, Settings asks where it belongs and this moves the blob and its key
//! there.
//!
//! What the file is comes from the original game name in its header: a `.pnt` is a paint (for a
//! bike or a piece of rider gear, which the player picks), anything else is a package that goes
//! in one of the content folders (which the player picks too, since a `.pkz` does not say
//! whether it is a track, a bike or gear).

use std::path::{Path, PathBuf};

use mxb_core::library::is_simple_name;
use mxb_core::securesource::{existing_key_path, key_path_for, strip_secured_ext};
use serde::{Deserialize, Serialize};

/// What a secured file is, by the original name in its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A `.pnt`: a bike paint or a rider gear paint.
    Paint,
    /// Anything else (`.pkz` and the rest): a track, bike or gear package.
    Package,
}

pub fn kind_of(game_name: &str) -> Kind {
    let is_pnt = Path::new(game_name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pnt"));
    if is_pnt { Kind::Paint } else { Kind::Package }
}

/// Where the player said the file goes.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// For a paint: `bike`, `helmet`, `goggles`, `boots`, `suit` or `gloves`. For a package:
    /// `tracks`, `bikes`, `helmets`, `boots` or `riders`.
    pub area: String,
    /// The bike or gear model a paint is for. Unused for a package.
    #[serde(default)]
    pub name: Option<String>,
}

/// The folder under the mods root a target names, as path segments.
///
/// Paint folders follow the game's own lookups (FrostMod's `offsets.h` paint formats):
/// `bikes\<bike>\paints`, `rider\helmets\<model>\paints` and `\goggles`,
/// `rider\boots\<model>\paints`, `rider\riders\<model>\paints` (suit) and `\gloves`.
pub fn target_segments(kind: Kind, target: &Target) -> Result<Vec<String>, String> {
    let named = |pre: &[&str], post: &str| -> Result<Vec<String>, String> {
        let name = target.name.as_deref().map(str::trim).unwrap_or_default();
        if !is_simple_name(name) {
            return Err("Choose the bike or gear this paint is for.".into());
        }
        let mut v: Vec<String> = pre.iter().map(|s| s.to_string()).collect();
        v.push(name.to_string());
        v.push(post.to_string());
        Ok(v)
    };
    let fixed = |segs: &[&str]| Ok(segs.iter().map(|s| s.to_string()).collect());
    match (kind, target.area.as_str()) {
        (Kind::Paint, "bike") => named(&["bikes"], "paints"),
        (Kind::Paint, "helmet") => named(&["rider", "helmets"], "paints"),
        (Kind::Paint, "goggles") => named(&["rider", "helmets"], "goggles"),
        (Kind::Paint, "boots") => named(&["rider", "boots"], "paints"),
        (Kind::Paint, "suit") => named(&["rider", "riders"], "paints"),
        (Kind::Paint, "gloves") => named(&["rider", "riders"], "gloves"),
        (Kind::Package, "tracks") => fixed(&["tracks"]),
        (Kind::Package, "bikes") => fixed(&["bikes"]),
        (Kind::Package, "helmets") => fixed(&["rider", "helmets"]),
        (Kind::Package, "boots") => fixed(&["rider", "boots"]),
        (Kind::Package, "riders") => fixed(&["rider", "riders"]),
        _ => Err("That isn't a place this kind of file can go.".into()),
    }
}

/// Join segments under `root`, reusing an existing folder of any letter case under its own
/// name on disk, so the paths written to the manifest match the folder the game reads.
pub fn target_dir(root: &Path, segments: &[String]) -> PathBuf {
    segments.iter().fold(root.to_path_buf(), |p, seg| {
        std::fs::read_dir(&p)
            .ok()
            .and_then(|rd| {
                rd.flatten()
                    .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(seg) && e.path().is_dir())
                    .map(|e| e.path())
            })
            .unwrap_or_else(|| p.join(seg))
    })
}

/// Lowercased, normal components, for a prefix test that ignores case and slash direction.
fn norm(p: &Path) -> Vec<String> {
    p.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            std::path::Component::Prefix(s) => Some(s.as_os_str().to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

/// Whether `path` is somewhere under `root`.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    let (r, p) = (norm(root), norm(path));
    !r.is_empty() && p.len() > r.len() && p[..r.len()] == r[..]
}

/// Move a file, across drives too (Downloads on one disk, the game on another).
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    if let Err(e) = std::fs::remove_file(from) {
        // The copy is in place, which is what matters; the original is only left behind.
        log::warn!("[secure] moved {} but couldn't remove the original: {e}", from.display());
    }
    Ok(())
}

/// Where a moved blob and its key ended up.
#[derive(Debug, PartialEq)]
pub struct Moved {
    pub blob: PathBuf,
    pub key: PathBuf,
}

/// Move `blob` and the key beside it into `dest`, as `<name>.mxbsecure` and
/// `<name>.mxbsecurekey`. A blob already there is replaced only when `same_asset` says it is
/// the same content (an updated copy of it); anything else there is refused.
pub fn move_into(
    blob: &Path,
    dest: &Path,
    same_asset: impl Fn(&Path) -> bool,
) -> Result<Moved, String> {
    let key = existing_key_path(&blob.to_string_lossy())
        .map(PathBuf::from)
        .ok_or("The file was unlocked, but its key is missing.")?;
    let file_name = blob.file_name().and_then(|n| n.to_str()).ok_or("The file has no name.")?;
    let stem = strip_secured_ext(file_name).unwrap_or(file_name);
    std::fs::create_dir_all(dest).map_err(|e| format!("Couldn't create {}: {e}", dest.display()))?;
    let new_blob = dest.join(format!("{stem}.mxbsecure"));
    let new_key = PathBuf::from(key_path_for(&new_blob.to_string_lossy()));
    if norm(&new_blob) == norm(blob) {
        return Ok(Moved { blob: new_blob, key });
    }
    if new_blob.exists() && !same_asset(&new_blob) {
        return Err(format!(
            "There is already a different file named {stem}.mxbsecure in {}.",
            dest.display()
        ));
    }
    move_file(blob, &new_blob).map_err(|e| format!("Couldn't move the file to {}: {e}", dest.display()))?;
    if let Err(e) = move_file(&key, &new_key) {
        // Put the blob back so the pair stays together.
        let _ = move_file(&new_blob, blob);
        return Err(format!("Couldn't move the key to {}: {e}", dest.display()));
    }
    Ok(Moved { blob: new_blob, key: new_key })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-secinstall-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn t(area: &str, name: Option<&str>) -> Target {
        Target { area: area.into(), name: name.map(str::to_string) }
    }

    #[test]
    fn the_kind_comes_from_the_original_name() {
        assert_eq!(kind_of("Red.pnt"), Kind::Paint);
        assert_eq!(kind_of("Red.PNT"), Kind::Paint);
        assert_eq!(kind_of("Pinehill.pkz"), Kind::Package);
        assert_eq!(kind_of("whatever"), Kind::Package);
    }

    #[test]
    fn a_paint_goes_in_the_picked_bike_or_gear_folder() {
        let segs = |k, a, n| target_segments(k, &t(a, n)).unwrap().join("/");
        assert_eq!(segs(Kind::Paint, "bike", Some("KTM450")), "bikes/KTM450/paints");
        assert_eq!(segs(Kind::Paint, "helmet", Some("Airoh")), "rider/helmets/Airoh/paints");
        assert_eq!(segs(Kind::Paint, "goggles", Some("Airoh")), "rider/helmets/Airoh/goggles");
        assert_eq!(segs(Kind::Paint, "boots", Some("Alpi")), "rider/boots/Alpi/paints");
        assert_eq!(segs(Kind::Paint, "suit", Some("rider")), "rider/riders/rider/paints");
        assert_eq!(segs(Kind::Paint, "gloves", Some("rider")), "rider/riders/rider/gloves");
        assert_eq!(segs(Kind::Package, "tracks", None), "tracks");
        assert_eq!(segs(Kind::Package, "helmets", None), "rider/helmets");
    }

    #[test]
    fn a_paint_without_a_bike_or_a_crafted_name_is_refused() {
        assert!(target_segments(Kind::Paint, &t("bike", None)).is_err(), "the bike must be picked");
        assert!(target_segments(Kind::Paint, &t("bike", Some(""))).is_err());
        assert!(target_segments(Kind::Paint, &t("bike", Some(".."))).is_err());
        assert!(target_segments(Kind::Paint, &t("bike", Some(r"..\..\x"))).is_err());
        assert!(target_segments(Kind::Paint, &t("tracks", None)).is_err(), "a paint is not a track");
        assert!(target_segments(Kind::Package, &t("bike", Some("KTM450"))).is_err());
    }

    #[test]
    fn inside_ignores_case_and_slashes() {
        let root = Path::new(r"C:\Users\A\Documents\PiBoSo\MX Bikes\mods");
        assert!(is_inside(root, Path::new(r"c:/users/a/documents/piboso/mx bikes/MODS/bikes/K/paints/R.mxbsecure")));
        assert!(!is_inside(root, Path::new(r"C:\Users\A\Downloads\R.mxbsecure")));
        assert!(!is_inside(root, root), "the root itself holds no content");
        assert!(!is_inside(Path::new(""), Path::new(r"C:\x")));
    }

    #[test]
    fn unlocked_paint_and_key_move_into_the_bike_paints_folder() {
        let d = tmp("move");
        let downloads = d.join("Downloads");
        let mods = d.join("mods");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(mods.join("bikes").join("KTM450")).unwrap();
        let blob = downloads.join("Red.MXBSECURE");
        std::fs::write(&blob, b"blob").unwrap();
        std::fs::write(downloads.join("Red.mxbsecurekey"), b"key").unwrap();

        let segs = target_segments(Kind::Paint, &t("bike", Some("ktm450"))).unwrap();
        let dest = target_dir(&mods, &segs);
        let moved = move_into(&blob, &dest, |_| false).unwrap();

        assert!(is_inside(&mods, &moved.blob));
        assert_eq!(moved.blob.file_name().unwrap(), "Red.mxbsecure", "lower-case extension on disk");
        assert_eq!(moved.key.file_name().unwrap(), "Red.mxbsecurekey");
        assert_eq!(std::fs::read(&moved.blob).unwrap(), b"blob");
        assert_eq!(std::fs::read(&moved.key).unwrap(), b"key");
        assert!(moved.blob.parent().unwrap().ends_with(Path::new("KTM450").join("paints")),
            "the existing bike folder is reused whatever case was picked: {}", moved.blob.display());
        assert!(!blob.exists() && !downloads.join("Red.mxbsecurekey").exists(), "nothing left behind");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_different_file_already_there_is_not_overwritten() {
        let d = tmp("clash");
        let dest = d.join("mods").join("tracks");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("Pine.mxbsecure"), b"someone else's").unwrap();
        let blob = d.join("Pine.mxbsecure");
        std::fs::write(&blob, b"mine").unwrap();
        std::fs::write(d.join("Pine.mxbsecurekey"), b"key").unwrap();

        assert!(move_into(&blob, &dest, |_| false).is_err());
        assert_eq!(std::fs::read(dest.join("Pine.mxbsecure")).unwrap(), b"someone else's");
        assert!(blob.exists(), "the picked file stays where it was");

        // An updated copy of the same asset replaces the old one.
        let moved = move_into(&blob, &dest, |_| true).unwrap();
        assert_eq!(std::fs::read(&moved.blob).unwrap(), b"mine");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_key_means_nothing_moves() {
        let d = tmp("nokey");
        let blob = d.join("Red.mxbsecure");
        std::fs::write(&blob, b"blob").unwrap();
        assert!(move_into(&blob, &d.join("mods"), |_| false).is_err());
        assert!(blob.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
