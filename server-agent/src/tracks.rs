//! What tracks this host actually has.
//!
//! The `.ini` takes a bare track name (`track = Victoria`), and setting one the box hasn't
//! got produces a server that restarts into nothing. The app can't answer this from the
//! player's own library either — the operator's PC and the server host are different
//! machines with different mods — so the only honest source is the host's own folder.

use std::collections::BTreeSet;
use std::path::Path;

/// What marks a directory as an extracted track rather than a folder someone filed tracks
/// into. Tracks are commonly organised as `mods/tracks/EU/RedBud.pkz`, so "is a directory"
/// is nowhere near enough — `EU` is not a track, and neither is anything *inside* an
/// extracted one.
///
/// These are the same extensions the app's own library scanner keys on. Deliberately the
/// same list: a track the app calls a track and the agent doesn't would show up as a name
/// the operator can't select, or a selection the server can't load.
const TRACK_MARKERS: [&str; 5] = ["map", "trh", "tsc", "rdf", "ssc"];

/// Tracks are usually filed in subfolders, so this recurses — but not far. A deep walk of a
/// large tracks folder costs real time on a request made just to fill a dropdown, and an
/// extracted track's interior is pruned anyway.
const MAX_DEPTH: usize = 3;

pub fn installed(game_dir: &Path) -> Vec<String> {
    let root = game_dir.join("mods").join("tracks");
    // A `BTreeSet` sorts, and dedupes a host holding one track both packaged and extracted.
    let mut out = BTreeSet::new();
    walk(&root, 0, &mut out);
    out.into_iter().collect()
}

fn walk(dir: &Path, depth: usize, out: &mut BTreeSet<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if has_track_markers(&path) {
                // The folder *is* the track. Its interior holds the track's own data —
                // including, sometimes, `.pkz` files that belong to it — so descending
                // further would invent tracks out of a track's contents.
                if let Some(name) = clean_name(&path) {
                    out.insert(name);
                }
            } else {
                // Not a track: an organisational folder. The tracks are further down.
                walk(&path, depth + 1, out);
            }
        } else if has_ext(&path, "pkz") {
            if let Some(name) = clean_name(&path) {
                out.insert(name);
            }
        }
    }
}

fn has_ext(path: &Path, want: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(want))
        .unwrap_or(false)
}

fn has_track_markers(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let p = e.path();
        p.is_file() && TRACK_MARKERS.iter().any(|m| has_ext(&p, m))
    })
}

/// The name the `.ini` would use: the file stem, with nothing that could smuggle a second
/// key into the config the app writes back.
fn clean_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.trim();
    if stem.is_empty() || stem.starts_with('.') {
        return None;
    }
    if stem.contains(['\n', '\r', '=']) {
        return None;
    }
    Some(stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("mxb-agent-tracks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn touch(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn finds_packaged_tracks_including_ones_filed_in_subfolders() {
        let root = tmp("packaged");
        touch(&root.join("mods/tracks/Victoria.pkz"));
        touch(&root.join("mods/tracks/EU/RedBud.pkz"));

        let got = installed(&root);
        assert_eq!(got, vec!["RedBud".to_string(), "Victoria".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_tracks_are_filed_into_is_not_itself_a_track() {
        // The bug this rule exists for: `EU` is a category, not something you can race on.
        let root = tmp("category");
        touch(&root.join("mods/tracks/EU/RedBud.pkz"));
        touch(&root.join("mods/tracks/EU/Southwick.pkz"));

        let got = installed(&root);
        assert!(!got.contains(&"EU".to_string()), "got {got:?}");
        assert_eq!(got.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_extracted_track_is_found_by_its_markers() {
        let root = tmp("extracted");
        touch(&root.join("mods/tracks/Milestone/track.map"));

        assert_eq!(installed(&root), vec!["Milestone".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_inside_of_an_extracted_track_is_not_more_tracks() {
        // The other half of the bug: without pruning, a track's own subfolders and any
        // `.pkz` it ships turn into track names nobody can select.
        let root = tmp("interior");
        let track = root.join("mods/tracks/Milestone");
        touch(&track.join("track.map"));
        touch(&track.join("data/mesh.pkz"));
        std::fs::create_dir_all(track.join("data/textures")).unwrap();

        assert_eq!(installed(&root), vec!["Milestone".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_track_present_both_packaged_and_extracted_is_listed_once() {
        let root = tmp("dupe");
        touch(&root.join("mods/tracks/Victoria.pkz"));
        touch(&root.join("mods/tracks/Victoria/track.trh"));

        assert_eq!(installed(&root), vec!["Victoria".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_files_that_are_not_tracks() {
        let root = tmp("noise");
        touch(&root.join("mods/tracks/readme.txt"));
        touch(&root.join("mods/tracks/notes.md"));

        assert!(installed(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_tracks_folder_is_an_empty_list_not_an_error() {
        let root = tmp("absent");
        assert!(installed(&root).is_empty());
    }

    #[test]
    fn refuses_names_that_could_rewrite_the_ini() {
        // The value is written straight into the server's config, so a name carrying a
        // newline or an `=` would let a crafted filename on the host add a second key.
        let root = tmp("evil");
        touch(&root.join("mods/tracks/ok.pkz"));
        touch(&root.join("mods/tracks/bad=key.pkz"));

        assert_eq!(installed(&root), vec!["ok".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
