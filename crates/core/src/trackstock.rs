//! The tracks that ship with the game, and how to recognise one.
//!
//! Nothing of a stock track is in the mods tree. Every one of them lives inside a single
//! archive in the *install* dir — `tracks.pkz`, laid out as `tracks/<category>/<id>/` — so a
//! scan of `mods/tracks` reports a server running `forest` as a track the player doesn't
//! have. The server browser then went looking for somewhere to buy it and offered the first
//! catalogue hit whose title contained the word: the stock Forest Raceway advertised as a
//! shop product.
//!
//! Read from the archive rather than from a list, because the archive is the truth and a list
//! goes stale the next time PiBoSo adds a track. [`BAKED`] is the fallback for the case that
//! isn't a mistake — the install dir isn't configured and Steam detection found nothing — and
//! is a snapshot, not the authority.

use crate::pkz;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The install-dir archive every stock track is packed into. The same name under GP Bikes;
/// this is PiBoSo's layout, not one title's.
pub const ARCHIVE: &str = "tracks.pkz";

/// The prefix each stock track sits under inside [`ARCHIVE`].
const ROOT: &str = "tracks";

/// The stock tracks of MX Bikes beta21d, as `(category, id)`.
///
/// Only reached when [`ARCHIVE`] can't be read at all. A stock track named here that the
/// installed build doesn't have costs nothing — the id has to come off a live server before
/// anything looks it up, and a server can only run a track it has.
const BAKED: &[(&str, &str)] = &[
    ("enduro", "enduro"),
    ("motocross", "assen"),
    ("motocross", "club"),
    ("motocross", "forest"),
    ("motocross", "mantua"),
    ("motocross", "maryland"),
    ("motocross", "mxb_test_track"),
    ("motocross", "practice"),
    ("motocross", "washington"),
    ("motocross", "winchester"),
    ("motocross", "winchestermxon"),
    ("straight rhythm", "straight_rhythm"),
    ("supercross", "nevada_13"),
    ("supercross", "nevada_18"),
    ("supermoto", "holjes"),
];

/// One track that came with the game.
#[derive(Debug, Clone, PartialEq)]
pub struct StockTrack {
    /// The folder name inside the archive — what a server publishes as its track.
    pub id: String,
    /// `motocross`, `supercross`, `enduro`, `supermoto`, `straight rhythm`.
    pub category: String,
    /// The name the game shows, from the track's own `.ini`. Falls back to [`Self::id`]
    /// when the archive couldn't be opened to read it.
    pub name: String,
}

impl StockTrack {
    /// Where this track's files sit inside [`ARCHIVE`].
    fn prefix(&self) -> String {
        format!("{ROOT}/{}/{}", self.category, self.id)
    }
}

/// The stock track a server means, or `None` when the id names a mod.
///
/// `install_dir` may be empty — that is the "we don't know where the game is" case, not an
/// error, and it falls back to [`BAKED`].
pub fn find(install_dir: &str, track_id: &str) -> Option<StockTrack> {
    let want = fold(track_id);
    if want.is_empty() {
        return None;
    }
    let archive = archive_path(install_dir);
    let (category, id) = index(archive.as_deref())
        .into_iter()
        .find(|(_, id)| fold(id) == want)?;

    let mut track = StockTrack { name: id.clone(), id, category };
    // The display name is worth one entry out of the archive — "Forest Raceway" is what the
    // player sees in the game's own track list, and `forest` is not.
    if let Some(path) = archive.as_deref() {
        if let Ok((meta, _)) = pkz::read_meta_and_preview_under(path, &track.prefix()) {
            if let Some(name) = meta.name.filter(|n| !n.trim().is_empty()) {
                track.name = name;
            }
        }
    }
    Some(track)
}

/// The track's own preview art, as a data URL.
///
/// Separate from [`find`] because it costs an image decode: a stock preview is an
/// uncompressed TGA, and the panel wants the name long before it wants the picture.
pub fn preview(install_dir: &str, track: &StockTrack) -> Option<String> {
    let path = archive_path(install_dir)?;
    pkz::read_meta_and_preview_under(&path, &track.prefix()).ok()?.1
}

fn archive_path(install_dir: &str) -> Option<PathBuf> {
    let dir = install_dir.trim();
    if dir.is_empty() {
        return None;
    }
    let path = Path::new(dir).join(ARCHIVE);
    path.is_file().then_some(path)
}

/// Every `(category, id)` in the archive, or [`BAKED`] when there is no archive to read.
///
/// Cached against the archive's size and mtime: the list only changes when the game updates,
/// and the walk is over a 1.7 GB file's directory.
fn index(archive: Option<&Path>) -> Vec<(String, String)> {
    let Some(path) = archive else {
        return baked();
    };
    let stamp = std::fs::metadata(path)
        .ok()
        .map(|m| {
            (
                m.len(),
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            )
        })
        .unwrap_or_default();
    let key = (path.to_path_buf(), stamp);

    static CACHE: OnceLock<Mutex<Option<(CacheKey, Vec<(String, String)>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    // A poisoned lock is recovered rather than propagated: this is a cache, and a panic in
    // one lookup must not turn every later one into an error.
    let mut held = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, list)) = held.as_ref() {
        if *cached == key {
            return list.clone();
        }
    }

    let list = read_index(path).unwrap_or_else(baked);
    *held = Some((key, list.clone()));
    list
}

type CacheKey = (PathBuf, (u64, u128));

/// The `(category, id)` pairs named by the archive's own entry list.
///
/// `None` when the archive can't be read — a locked or truncated file, or a build without
/// the reader for it — which is the caller's cue to fall back rather than report nothing.
fn read_index(path: &Path) -> Option<Vec<(String, String)>> {
    let names = pkz::entry_names(path).ok()?;
    let mut out: Vec<(String, String)> = Vec::new();
    for name in names {
        let parts: Vec<&str> = name.split('/').collect();
        // `tracks/<category>/<id>/<something>` — the fourth segment proves the third is a
        // folder, so a stray file directly under a category is never taken for a track.
        if parts.len() < 4 || !parts[0].eq_ignore_ascii_case(ROOT) {
            continue;
        }
        let (category, id) = (parts[1].to_string(), parts[2].to_string());
        if category.is_empty() || id.is_empty() {
            continue;
        }
        if !out.iter().any(|(c, i)| *c == category && *i == id) {
            out.push((category, id));
        }
    }
    (!out.is_empty()).then_some(out)
}

fn baked() -> Vec<(String, String)> {
    BAKED.iter().map(|(c, i)| (c.to_string(), i.to_string())).collect()
}

/// The same fold the rest of the app matches names by: lowercase, and everything that isn't
/// alphanumeric reduced to a single space.
fn fold(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fault this module exists for: a server on the stock `forest` was being offered
    /// as a mod to buy, because nothing of a stock track is in the mods tree.
    #[test]
    fn the_stock_tracks_are_recognised_without_an_install() {
        for id in ["forest", "nevada_13", "holjes", "straight_rhythm", "mxb_test_track"] {
            let hit = find("", id).unwrap_or_else(|| panic!("{id} should be stock"));
            assert_eq!(hit.id, id);
        }
        assert!(find("", "Briarcliff MX").is_none(), "a mod track is not stock");
        assert!(find("", "").is_none());
    }

    /// A server publishes the folder name, but the app folds names before comparing them,
    /// and the two must agree about what counts as the same track.
    #[test]
    fn a_stock_id_matches_however_it_is_cased_or_punctuated() {
        assert_eq!(find("", "Forest").map(|t| t.id), Some("forest".into()));
        assert_eq!(
            find("", "MXB Test Track").map(|t| t.id),
            Some("mxb_test_track".into()),
            "the fold reduces the underscore to a space, so the spaced form matches",
        );
    }

    /// With no install dir there is no artwork to be had, and the name falls back to the id.
    #[test]
    fn without_the_archive_a_stock_track_still_answers() {
        let hit = find("", "forest").unwrap();
        assert_eq!(hit.name, "forest");
        assert_eq!(hit.category, "motocross");
        assert!(preview("", &hit).is_none());
    }

    /// The real archive, when there is one to point at: `MXB_INSTALL_DIR` at a folder
    /// holding the game's `tracks.pkz`. Proves the three things the fixtures can't — that
    /// the archive's own list agrees with [`BAKED`], that the display name comes out of the
    /// track's `.ini`, and that the preview scoped to one folder is that track's picture.
    #[test]
    #[ignore = "needs a game install"]
    fn the_real_archive_names_every_stock_track() {
        let Ok(install) = std::env::var("MXB_INSTALL_DIR") else {
            eprintln!("set MXB_INSTALL_DIR to a folder holding tracks.pkz");
            return;
        };
        let path = archive_path(&install).expect("tracks.pkz beside the exe");
        let mut from_archive = read_index(&path).expect("the archive lists its tracks");
        from_archive.sort();
        let mut from_baked = baked();
        from_baked.sort();
        assert_eq!(from_archive, from_baked, "the baked snapshot has gone stale");

        let forest = find(&install, "forest").expect("forest is stock");
        assert_eq!(forest.name, "Forest Raceway", "the name comes from the track's own ini");
        let art = preview(&install, &forest).expect("forest has preview art");
        assert!(art.starts_with("data:image/"), "got {}", &art[..art.len().min(40)]);
        println!("forest: {} — {} bytes of preview", forest.name, art.len());
    }

    /// An entry list shaped like the game's own, read the way [`read_index`] reads it.
    #[test]
    fn the_index_takes_folders_and_not_stray_files() {
        let dir = std::env::temp_dir().join(format!("mxb-trackstock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(ARCHIVE);

        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for entry in [
            "tracks/motocross/forest/forest.ini",
            "tracks/motocross/forest/track_image.tga",
            "tracks/motocross/forest/short/short.ini",
            "tracks/supercross/nevada_13/nevada_13.ini",
            // Not a track: no folder of its own under the category.
            "tracks/motocross/readme.txt",
        ] {
            zip.start_file(entry, opts).unwrap();
        }
        zip.finish().unwrap();

        let mut got = read_index(&path).unwrap();
        got.sort();
        assert_eq!(
            got,
            vec![
                ("motocross".to_string(), "forest".to_string()),
                ("supercross".to_string(), "nevada_13".to_string()),
            ],
            "a layout folder is part of its track, and a loose file is not a track",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
