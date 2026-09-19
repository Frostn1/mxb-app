//! Which file holds the track the game names.
//!
//! A session knows its track only by the id the game reports: the folder inside a mod's
//! `.pkz`, which is often not the file's name, or a stock track's folder inside the install's
//! `tracks.pkz`. This turns that id into something the track readers can open — a path, and
//! for a stock track the folder inside the archive that is its own.

use crate::{config::AppConfig, library, track, trackstock};
use std::path::Path;

/// Where a track's files are.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackSource {
    /// The track's `.pkz`, folder or `.mxbsecure` — or the install's `tracks.pkz` when stock.
    pub path: String,
    /// The track's folder inside `path` when that archive holds more than one track.
    pub prefix: Option<String>,
    /// The installed file's name, or the name the game shows for a stock track.
    pub name: String,
    pub stock: bool,
    /// Its contents can't be read here, so there is no terrain to draw.
    pub locked: bool,
}

/// Where installed tracks live, under the user folder. The `mods` segment is what routes the
/// lookup through the mods root; without it the scan lands somewhere that isn't there.
const TRACKS: &str = "mods/tracks";

/// The track an id names: an installed mod first, then the game's own.
pub fn resolve(cfg: &AppConfig, track_id: &str) -> Option<TrackSource> {
    let id = track_id.trim();
    if id.is_empty() {
        return None;
    }
    // "mods/tracks", not "tracks". `mods_path` is the user folder, and only a leading `mods`
    // segment is routed through the mods root — so "tracks" asked for `<user>/tracks`, which
    // does not exist on a normal install, while every track sits in `<user>/mods/tracks`.
    // `scan_library` answers a missing folder with an empty list and `unwrap_or_default`
    // swallowed the rest, so every mod track came back "isn't in your mods" and always had.
    let dir = library::mods_subdir(&cfg.mods_path, TRACKS);
    let entries = library::scan_library(&cfg.mods_path, TRACKS, &[], cfg.game()).unwrap_or_default();
    log::debug!("track \"{id}\": {} installed in {}", entries.len(), dir.display());
    if let Some(hit) = find_installed(entries, id) {
        return Some(installed_source(hit));
    }
    stock_source(&cfg.install_dir(), id)
}

/// What two spellings of one track share: its letters and digits, lowercased. A server's
/// `Farm14` has to find the player's `Farm 14.pkz`, and a fold that keeps word breaks can't.
pub fn key(raw: &str) -> String {
    raw.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The installed track an id names. By file name first, since that costs nothing; then by
/// the folder inside each archive, which is what the game actually reports.
pub fn find_installed(
    entries: Vec<library::LibraryEntry>,
    id: &str,
) -> Option<library::LibraryEntry> {
    let want = key(id);
    if want.is_empty() {
        return None;
    }
    if let Some(i) = entries.iter().position(|e| key(&library::strip_ext(&e.name)) == want) {
        return entries.into_iter().nth(i);
    }
    entries
        .into_iter()
        .find(|e| track::folder_name(Path::new(&e.path)).is_some_and(|f| key(&f) == want))
}

fn installed_source(hit: library::LibraryEntry) -> TrackSource {
    TrackSource {
        locked: hit.locked || track::is_locked(Path::new(&hit.path)),
        name: library::strip_ext(&hit.name),
        path: hit.path,
        prefix: None,
        stock: false,
    }
}

/// A stock track's folder in the install's archive. `None` without the archive: a stock
/// track known only from the baked list has no terrain to read.
fn stock_source(install_dir: &str, id: &str) -> Option<TrackSource> {
    let archive = trackstock::archive_path(install_dir)?;
    let hit = trackstock::find(install_dir, id)?;
    Some(TrackSource {
        path: archive.to_string_lossy().into_owned(),
        prefix: Some(hit.prefix()),
        name: hit.name,
        stock: true,
        locked: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, path: &Path) -> library::LibraryEntry {
        library::LibraryEntry {
            name: name.into(),
            path: path.to_string_lossy().into_owned(),
            folder: String::new(),
            size: 0,
            modified: 0,
            kind: "pkz".into(),
            category: "track".into(),
            parent: None,
            secured: false,
            locked: false,
            prefix: None,
            stock: false,
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, text) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tracksource-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_mod_is_found_by_its_file_name_folded() {
        let list = || {
            vec![
                entry("Hangtown.pkz", Path::new("/x/Hangtown.pkz")),
                entry("Briarcliff MX.pkz", Path::new("/x/Briarcliff MX.pkz")),
            ]
        };
        let name = |id: &str| find_installed(list(), id).map(|e| e.name);
        assert_eq!(name("briarcliff_mx").as_deref(), Some("Briarcliff MX.pkz"));
        assert_eq!(name("HANGTOWN").as_deref(), Some("Hangtown.pkz"));
        assert_eq!(name(""), None);
        assert_eq!(name("Farm14"), None);
    }

    /// Servers and downloads disagree about spaces: `Farm14` on the server, `Farm 14.pkz` on
    /// disk.
    #[test]
    fn a_mod_is_found_whatever_its_spacing() {
        let list = || vec![entry("Farm 14.pkz", Path::new("/x/Farm 14.pkz"))];
        let name = |id: &str| find_installed(list(), id).map(|e| e.name);
        assert_eq!(name("Farm14").as_deref(), Some("Farm 14.pkz"));
        assert_eq!(name("farm_14").as_deref(), Some("Farm 14.pkz"));
        assert_eq!(name("Farm 15"), None);
    }

    /// What the game reports is the folder inside the archive, which a download often
    /// names nothing like its file.
    #[test]
    fn a_mod_is_found_by_the_folder_inside_it() {
        let dir = scratch("inner");
        let pkz = dir.join("Hangtown Classic v2 (fixed).pkz");
        write_zip(&pkz, &[("Hangtown_Classic/hangtown.ini", ""), ("Hangtown_Classic/hangtown.trh", "")]);
        let hit = find_installed(
            vec![entry("Other.pkz", &dir.join("missing.pkz")), entry("Hangtown Classic v2 (fixed).pkz", &pkz)],
            "hangtown classic",
        );
        assert_eq!(hit.map(|e| e.path), Some(pkz.to_string_lossy().into_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_name_match_beats_an_inner_folder_one() {
        let dir = scratch("stem-first");
        let pack = dir.join("pack.pkz");
        write_zip(&pack, &[("Forest/forest.trh", "")]);
        let hit = find_installed(
            vec![entry("pack.pkz", &pack), entry("Forest.pkz", &dir.join("Forest.pkz"))],
            "forest",
        );
        assert_eq!(hit.map(|e| e.name).as_deref(), Some("Forest.pkz"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stock_track_resolves_to_its_folder_in_the_install_archive() {
        let dir = scratch("stock");
        let archive = dir.join(trackstock::ARCHIVE);
        write_zip(
            &archive,
            &[
                ("tracks/motocross/club/club.ini", "[info]\nname = Club MX\n"),
                ("tracks/motocross/forest/forest.ini", "[info]\nname = Forest Raceway\n"),
                ("tracks/motocross/forest/forest.trh", ""),
            ],
        );
        let install = dir.to_string_lossy().into_owned();
        let src = stock_source(&install, "Forest").expect("forest is stock");
        assert_eq!(src.path, archive.to_string_lossy());
        assert_eq!(src.prefix.as_deref(), Some("tracks/motocross/forest"));
        assert_eq!(src.name, "Forest Raceway");
        assert!(src.stock && !src.locked);
        assert!(stock_source(&install, "Briarcliff MX").is_none());
        assert!(stock_source("", "forest").is_none(), "no archive, no terrain to read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_track_that_cannot_be_opened_resolves_as_locked() {
        let dir = scratch("locked");
        let pkz = dir.join("Farm14.pkz");
        std::fs::write(&pkz, b"no reader for this here").unwrap();
        let src = installed_source(entry("Farm14.pkz", &pkz));
        assert!(src.locked);
        assert_eq!(src.name, "Farm14");

        let open = dir.join("Open.pkz");
        write_zip(&open, &[("Open/open.trh", "")]);
        assert!(!installed_source(entry("Open.pkz", &open)).locked);

        let mut keyless = entry("Sealed.mxbsecure", &dir.join("Sealed.mxbsecure"));
        keyless.secured = true;
        keyless.locked = true;
        assert!(installed_source(keyless).locked);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `resolve` scans a real folder, and every other test here hands `find_installed` a list
    /// it built itself — so nothing ever checked *where* the scan looks. It looked under
    /// `<user>/tracks`, which does not exist on a normal install, while every track lives in
    /// `<user>/mods/tracks`. The result was that no installed mod track ever resolved, from
    /// the day the module was written, with `unwrap_or_default` swallowing the empty scan.
    #[test]
    fn resolve_looks_where_tracks_are_actually_installed() {
        let user = scratch("scan-root");
        let tracks = user.join("mods").join("tracks");
        std::fs::create_dir_all(&tracks).unwrap();
        write_zip(&tracks.join("Farm 14.pkz"), &[("Farm14/farm.trh", "")]);
        // The folder the broken version scanned, with a decoy in it: if the scan ever goes
        // back there, this test finds the wrong track rather than nothing, and says so.
        let wrong = user.join("tracks");
        std::fs::create_dir_all(&wrong).unwrap();
        write_zip(&wrong.join("Decoy.pkz"), &[("Decoy/decoy.trh", "")]);

        let cfg = AppConfig { mods_path: user.to_string_lossy().into_owned(), ..AppConfig::default() };
        let found = resolve(&cfg, "Farm14").expect("a track in mods/tracks must resolve");
        assert_eq!(found.name, "Farm 14", "found the installed track, not the decoy");
        assert!(resolve(&cfg, "Decoy").is_none(), "nothing outside mods/tracks is a track");
        let _ = std::fs::remove_dir_all(&user);
    }
}
