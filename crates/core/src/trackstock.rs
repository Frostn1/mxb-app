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
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
    pub fn prefix(&self) -> String {
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

/// Every stock track the install carries, in archive order.
///
/// Deliberately cheap: the ids and categories come from the cached [`index`] and **no name is
/// lifted**. Reading one name means walking a 1.7 GB central directory and parsing an `.ini`,
/// and doing that fifteen times before a list can paint is the difference between a tab that
/// opens and a tab that hangs. [`StockTrack::name`] is therefore the id here, and the Library
/// fills the real one in per card the same way it does for an installed mod — lazily, through
/// [`crate::pkz::read_meta_cached_under`], which caches per track.
///
/// Falls back to [`BAKED`] when the install dir isn't configured, same as everything else
/// here, so a list is never empty just because the game hasn't been found yet.
pub fn list(install_dir: &str) -> Vec<StockTrack> {
    index(archive_path(install_dir).as_deref())
        .into_iter()
        .map(|(category, id)| StockTrack { name: id.clone(), id, category })
        .collect()
}

/// Lift one stock track out of the shared archive into a `.pkz` of its own at `to`.
///
/// The point is a file that stands on its own: something to open in the Studio, or to read
/// with any tool that expects one track per archive. Entries are re-rooted from
/// `tracks/<category>/<id>/…` to `<id>/…`, because a track's files must sit inside a folder
/// named after the track or the game lists nothing, and written through
/// [`pkz::pack_entries`], which is the only writer whose output the game's own reader can
/// follow.
///
/// This reads from the player's own installed copy and writes where they asked. It stages
/// nothing in the mods tree: a mod track sharing a stock track's id would give the game two
/// sources for one name.
pub fn extract(install_dir: &str, track: &StockTrack, to: &Path) -> anyhow::Result<u64> {
    use anyhow::{bail, Context};
    let Some(archive) = archive_path(install_dir) else {
        bail!("no {ARCHIVE} to read — the game install folder isn't set");
    };
    let prefix = track.prefix();
    let names = crate::track::entry_names_under(&archive, Some(&prefix))
        .with_context(|| format!("read {}", archive.display()))?;
    if names.is_empty() {
        bail!("{ARCHIVE} holds no files under {prefix}");
    }

    // Matched the way the prefix was matched, so a `Tracks/Motocross/Forest/` archive re-roots
    // as readily as a lowercase one.
    let cut = prefix.len();
    let want: std::collections::HashSet<String> = names.iter().cloned().collect();
    let entries: Vec<(String, Vec<u8>)> = pkz::read_selected(&archive, |n| want.contains(n))
        .with_context(|| format!("read {prefix} out of {}", archive.display()))?
        .into_iter()
        .filter_map(|(name, bytes)| {
            let rest = name.get(cut..)?.trim_start_matches('/');
            (!rest.is_empty()).then(|| (format!("{}/{rest}", track.id), bytes))
        })
        .collect();
    if entries.is_empty() {
        bail!("nothing under {prefix} survived re-rooting");
    }
    pkz::pack_entries(entries, to)
}

/// The track's own preview art, as a data URL.
///
/// Separate from [`find`] because it costs an image decode: a stock preview is an
/// uncompressed TGA, and the panel wants the name long before it wants the picture.
pub fn preview(install_dir: &str, track: &StockTrack) -> Option<String> {
    let path = archive_path(install_dir)?;
    pkz::read_meta_and_preview_under(&path, &track.prefix()).ok()?.1
}

/// The install's [`ARCHIVE`], when there is one on disk.
pub fn archive_path(install_dir: &str) -> Option<PathBuf> {
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
pub fn fold(raw: &str) -> String {
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

    /// The extract that matters: a real stock track out of the real 1.7 GB archive.
    ///
    /// The fixture above proves the re-rooting. This proves the thing a fixture cannot — that
    /// the result is an archive the *game's own reader* can follow, walked raw the way that
    /// reader walks it. An Info-ZIP `zip` of the same files passes every other check and
    /// lists nothing in the game.
    #[test]
    #[ignore = "needs a game install"]
    fn the_real_forest_lifts_out_whole() {
        let Ok(install) = std::env::var("MXB_INSTALL_DIR") else {
            eprintln!("set MXB_INSTALL_DIR to a folder holding tracks.pkz");
            return;
        };
        let track = find(&install, "forest").expect("forest is stock");
        let dir = std::env::temp_dir().join(format!("mxb-forest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("forest.pkz");

        let bytes = extract(&install, &track, &out).expect("forest lifts out");
        let names = pkz::entry_names(&out).unwrap();
        assert!(!names.is_empty(), "nothing came out");
        assert!(
            names.iter().all(|n| n.starts_with("forest/")),
            "something escaped the track's own folder: {:?}",
            names.iter().find(|n| !n.starts_with("forest/")),
        );
        assert!(
            names.iter().any(|n| n.ends_with(".trh")) && names.iter().any(|n| n.ends_with(".map")),
            "a track without its terrain or its graphics is not a track: {names:?}",
        );

        let b = std::fs::read(&out).unwrap();
        let mut at = 0usize;
        let mut seen = 0usize;
        while at + 4 <= b.len() && b[at..at + 4] == ZIP_MAGIC_LOCAL {
            let name_len = u16::from_le_bytes(b[at + 26..at + 28].try_into().unwrap()) as usize;
            let extra = u16::from_le_bytes(b[at + 28..at + 30].try_into().unwrap()) as usize;
            let name = String::from_utf8_lossy(&b[at + 30..at + 30 + name_len]).into_owned();
            assert_eq!(extra, 0, "{name} carries a local extra field");
            assert_eq!(u16::from_le_bytes(b[at + 4..at + 6].try_into().unwrap()), 20);
            let comp = u32::from_le_bytes(b[at + 18..at + 22].try_into().unwrap()) as usize;
            at += 30 + name_len + extra + comp;
            seen += 1;
        }
        assert_eq!(seen, names.len(), "lost the thread after {seen} of {}", names.len());

        println!("forest.pkz: {} entries, {bytes} bytes — {names:?}", names.len());
        let _ = std::fs::remove_dir_all(&dir);
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

    /// A lifted track has to stand on its own: its files nested under a folder named after
    /// it, the neighbouring tracks left behind, and nothing of the shared archive's own
    /// `tracks/<category>/` scaffolding carried across.
    ///
    /// The nesting is the part that bites. Flat at the archive root the game finds nothing,
    /// so an extract that forgot to re-root would produce a file that opens perfectly in
    /// every tool and lists nowhere in the game.
    #[test]
    fn a_lifted_track_stands_on_its_own() {
        let dir = std::env::temp_dir().join(format!("mxb-stock-extract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let src = dir.join(ARCHIVE);
        pkz::pack_entries(
            vec![
                ("tracks/motocross/forest/forest.ini".into(), b"[info]\r\nname = Forest Raceway\r\n".to_vec()),
                ("tracks/motocross/forest/forest.trh".into(), vec![1u8; 128]),
                ("tracks/motocross/forest/short/short.ini".into(), b"[info]\r\n".to_vec()),
                // The neighbours, which must not come along.
                ("tracks/motocross/mantua/mantua.ini".into(), b"[info]\r\n".to_vec()),
                ("tracks/supercross/nevada_13/nevada_13.trh".into(), vec![2u8; 64]),
            ],
            &src,
        )
        .unwrap();

        let track = StockTrack {
            id: "forest".into(),
            category: "motocross".into(),
            name: "Forest Raceway".into(),
        };
        let out = dir.join("forest.pkz");
        let written = extract(dir.to_str().unwrap(), &track, &out).unwrap();
        assert!(written > 0, "wrote an empty archive");

        let mut got = pkz::entry_names(&out).unwrap();
        got.sort();
        assert_eq!(
            got,
            ["forest/forest.ini", "forest/forest.trh", "forest/short/short.ini"],
            "re-rooted under the track's own folder, and only this track",
        );
        // `read_entry` matches on the base name, which is all a track reader ever knows.
        assert_eq!(
            pkz::read_entry(&out, "forest.trh").unwrap(),
            Some(vec![1u8; 128]),
            "the bytes came across unchanged",
        );

        // And it is still an archive the game's own reader can follow — no extra fields — so
        // a lifted track lists where a shell `zip` of the same files would not.
        let b = std::fs::read(&out).unwrap();
        let mut at = 0usize;
        let mut seen = 0;
        while at + 4 <= b.len() && b[at..at + 4] == ZIP_MAGIC_LOCAL {
            let name_len = u16::from_le_bytes(b[at + 26..at + 28].try_into().unwrap()) as usize;
            let extra_len = u16::from_le_bytes(b[at + 28..at + 30].try_into().unwrap()) as usize;
            assert_eq!(extra_len, 0, "entry {seen} carries a local extra field");
            let comp = u32::from_le_bytes(b[at + 18..at + 22].try_into().unwrap()) as usize;
            at += 30 + name_len + extra_len + comp;
            seen += 1;
        }
        assert_eq!(seen, 3, "lost the thread after {seen} entries");

        assert!(
            extract(dir.to_str().unwrap(), &StockTrack {
                id: "nothing".into(),
                category: "motocross".into(),
                name: "nothing".into(),
            }, &dir.join("nothing.pkz"))
            .is_err(),
            "a track the archive doesn't hold is an error, not an empty archive",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ZIP local-file-header magic, for the raw walk above.
    const ZIP_MAGIC_LOCAL: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
}
