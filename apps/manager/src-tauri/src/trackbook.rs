//! What each track id turned out to be, kept on disk.
//!
//! Identifying a track is the most expensive thing the server browser does: a slug lookup on
//! mxb-mods, then up to three catalogue searches behind it. The answer is also the most
//! stable thing it holds — a track's page does not move, and `fort_red` means the same track
//! on every server that runs it and on every run of the app. Keeping it only in memory meant
//! a restart, and even a page refresh, paid for all of it again.
//!
//! What is stored is the catalogue half alone: where the track can be had, what it is called
//! there and its picture. Whether the player *has* it is worked out fresh every time — that
//! depends on the library, which changes under us, and costs nothing but a local scan.
//!
//! A miss is stored too, and is the point of the exercise: a track with no page anywhere is
//! exactly the one that would otherwise spend four requests every time it came round. It is
//! held for a shorter while than a hit, because a track absent today can be uploaded
//! tomorrow.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How the answer was arrived at.
///
/// Bumped whenever identification changes in a way that could give a different answer for the
/// same id, which retires every row written by the old way. Slug lookup on mxb-mods made
/// version 2: version 1 searched, and its misses are not this version's misses. Version 3
/// stopped a stock track being matched to a mod that merely shares its address, which retires
/// every row that mistake wrote — `forest` among them. Version 4 supplements an mxb-mods hit
/// with Shop artwork, version 5 uses a server's visible pack title for secured tracks whose
/// internal folder name does not resemble the Shop product, version 6 canonicalizes round ids
/// such as `RD01` to match Shop titles that use `RD1`, and version 7 tries both zero-padded
/// and unpadded round spellings because the Shop uses both forms. Version 8 also understands
/// compact series/round ids such as `ARLSX_RD09` when the Shop spells them `ARL SX ROUND 09`.
/// Version 9 recognizes the Shop's `SPX ARL SX` and spaced `ARL FINALS RD 02` identities.
pub const VERSION: u32 = 9;

/// How long a found track is trusted. A page does not move, and if it ever does the worst
/// case is a dead link on one panel until the row ages out.
pub const KEEP_HIT: u64 = 30 * 24 * 60 * 60 * 1000;

/// How long "nowhere to be found" is trusted. Shorter, because it is the answer most likely
/// to stop being true: mods get uploaded.
pub const KEEP_MISS: u64 = 3 * 24 * 60 * 60 * 1000;

/// The most tracks the book will hold. Oldest-checked rows go first.
pub const MAX_ENTRIES: usize = 4000;

/// One identified track. The id is the key; the rest is what a catalogue said about it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    /// `"mods"`, `"shop"`, `"hub"`, or empty for "looked, found nothing".
    pub source: String,
    pub product_id: u64,
    pub product_name: String,
    pub product_url: String,
    pub product_image: String,
    /// False when the name only resembled the track rather than matching it.
    pub exact: bool,
    /// Milliseconds since the epoch, from when the lookup ran.
    pub checked: u64,
    /// The [`VERSION`] that wrote this row.
    pub version: u32,
}

impl Entry {
    /// Nothing was found for this track.
    pub fn is_miss(&self) -> bool {
        self.source.is_empty()
    }

    /// Still worth believing at `now`.
    pub fn fresh(&self, now: u64) -> bool {
        if self.version != VERSION {
            return false;
        }
        let age = now.saturating_sub(self.checked);
        age < if self.is_miss() { KEEP_MISS } else { KEEP_HIT }
    }
}

/// The whole book, keyed by the id a server publishes.
pub type Book = HashMap<String, Entry>;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))?
        .join("tracks.json"))
}

/// The book, in memory for the run. Read once, written through on every new answer.
static BOOK: std::sync::OnceLock<std::sync::Mutex<Book>> = std::sync::OnceLock::new();

fn cell(app: &tauri::AppHandle) -> &'static std::sync::Mutex<Book> {
    BOOK.get_or_init(|| std::sync::Mutex::new(load(app)))
}

/// Read the book. A missing or damaged file is an empty book, never an error.
fn load(app: &tauri::AppHandle) -> Book {
    let Ok(p) = path(app) else { return Book::new() };
    read(&p)
}

fn read(p: &Path) -> Book {
    let Ok(text) = std::fs::read_to_string(p) else { return Book::new() };
    match serde_json::from_str::<Book>(&text) {
        Ok(book) => book,
        Err(e) => {
            log::warn!("[trackbook] {} isn't readable ({e}); starting fresh", p.display());
            Book::new()
        }
    }
}

/// What the book has for a track, if it is still worth believing.
pub fn get(app: &tauri::AppHandle, track: &str) -> Option<Entry> {
    let book = cell(app).lock().ok()?;
    book.get(track).filter(|e| e.fresh(now_ms())).cloned()
}

/// Record what a track turned out to be, and write the book back.
///
/// Best-effort: a book we couldn't write is a slower panel next time, not a failed one.
pub fn remember(app: &tauri::AppHandle, track: &str, entry: Entry) {
    let snapshot = {
        let Ok(mut book) = cell(app).lock() else { return };
        book.insert(track.to_string(), entry);
        prune(&mut book);
        book.clone()
    };
    write(app, &snapshot);
}

/// Drop what has aged out, then the oldest-checked rows until the book fits.
fn prune(book: &mut Book) {
    let now = now_ms();
    book.retain(|_, e| e.fresh(now));
    if book.len() <= MAX_ENTRIES {
        return;
    }
    let mut ages: Vec<(u64, String)> =
        book.iter().map(|(k, e)| (e.checked, k.clone())).collect();
    ages.sort_unstable();
    for (_, key) in ages.into_iter().take(book.len() - MAX_ENTRIES) {
        book.remove(&key);
    }
}

fn write(app: &tauri::AppHandle, book: &Book) {
    let Ok(p) = path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(book) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                log::warn!("[trackbook] couldn't write {}: {e}", p.display());
            }
        }
        Err(e) => log::warn!("[trackbook] couldn't serialize the book: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str, checked: u64) -> Entry {
        Entry {
            source: source.into(),
            checked,
            version: VERSION,
            ..Default::default()
        }
    }

    #[test]
    fn a_found_track_is_trusted_far_longer_than_a_missing_one() {
        let now = 10 * KEEP_HIT;
        let hit = entry("mods", now - KEEP_MISS - 1);
        let miss = entry("", now - KEEP_MISS - 1);
        assert!(hit.fresh(now), "a page that was found doesn't move");
        assert!(!miss.fresh(now), "nothing found is worth another look sooner");
    }

    #[test]
    fn a_row_written_by_an_older_way_of_looking_is_never_used() {
        let now = 10 * KEEP_HIT;
        let stale = Entry { version: VERSION - 1, ..entry("mods", now) };
        assert!(!stale.fresh(now));
    }

    #[test]
    fn pruning_drops_what_aged_out_then_the_coldest() {
        let now = now_ms();
        let mut book = Book::new();
        book.insert("gone".into(), entry("", now - KEEP_MISS - 1));
        for i in 0..MAX_ENTRIES + 10 {
            book.insert(format!("t{i}"), entry("mods", now - i as u64));
        }
        prune(&mut book);
        assert!(!book.contains_key("gone"), "an aged-out row goes first");
        assert_eq!(book.len(), MAX_ENTRIES);
        assert!(book.contains_key("t0"), "the most recently checked row stays");
        assert!(!book.contains_key(&format!("t{}", MAX_ENTRIES + 9)), "the coldest goes");
    }
}
