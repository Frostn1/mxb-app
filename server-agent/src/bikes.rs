//! What bikes this host has, and therefore which ones it will accept.
//!
//! A dedicated server refuses any bike it does not itself have installed — a rider on a mod
//! bike the host has never seen is rejected at the gate, with nothing in the game to say
//! why. That single fact is the reason this file exists: the app can offer to check a
//! player's bike against the server *before* they launch, but only if the server can say
//! what it has, and only the host's own folder knows.
//!
//! The player's own library is no substitute. The operator's PC and the server box are
//! different machines with different mods — the same reason [`crate::tracks`] reads the
//! host's tracks rather than trusting the app's list.

use std::collections::BTreeSet;
use std::path::Path;

/// Bikes are packaged as `mods/bikes/<name>.pkz`, or unpacked into `mods/bikes/<name>/`.
///
/// Both forms are the bike's own id — the value that ends up in a rider's `profile.ini` and
/// the value the server matches on — so no deeper walk is needed or wanted: the inside of an
/// unpacked bike is its data, not more bikes.
pub fn installed(game_dir: &Path) -> Vec<String> {
    let root = game_dir.join("mods").join("bikes");
    // A `BTreeSet` sorts, and dedupes a host holding one bike both packaged and unpacked.
    let mut out = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = if path.is_dir() {
            path.file_name().and_then(|n| n.to_str())
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pkz"))
        {
            path.file_stem().and_then(|s| s.to_str())
        } else {
            None
        };
        if let Some(name) = name.map(str::trim).filter(|n| clean(n)) {
            out.insert(name.to_string());
        }
    }
    out.into_iter().collect()
}

/// Names worth reporting. Hidden entries are housekeeping rather than content, and control
/// characters have no business in a value the app renders and compares against.
fn clean(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['\n', '\r'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("mxb-agent-bikes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn touch(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn finds_packaged_and_unpacked_bikes() {
        let root = tmp("both");
        touch(&root.join("mods/bikes/KX450.pkz"));
        touch(&root.join("mods/bikes/YZ250/bike.cfg"));

        assert_eq!(installed(&root), vec!["KX450".to_string(), "YZ250".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bike_present_both_ways_is_listed_once() {
        let root = tmp("dupe");
        touch(&root.join("mods/bikes/KX450.pkz"));
        touch(&root.join("mods/bikes/KX450/bike.cfg"));

        assert_eq!(installed(&root), vec!["KX450".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_inside_of_an_unpacked_bike_is_not_more_bikes() {
        // A bike's own folders would otherwise become bike ids nobody rides.
        let root = tmp("interior");
        touch(&root.join("mods/bikes/YZ250/paints/blue.pnt"));
        touch(&root.join("mods/bikes/YZ250/data/mesh.pkz"));

        assert_eq!(installed(&root), vec!["YZ250".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_files_that_are_not_bikes() {
        let root = tmp("noise");
        touch(&root.join("mods/bikes/readme.txt"));
        touch(&root.join("mods/bikes/.DS_Store"));

        assert!(installed(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_bikes_folder_is_an_empty_list_not_an_error() {
        let root = tmp("absent");
        assert!(installed(&root).is_empty());
    }
}
