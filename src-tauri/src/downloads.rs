//! What the app has downloaded, kept so it can be looked back at.
//!
//! Nothing else remembers. The library is a scan of the mods folder, which says what is there but
//! not when it arrived or what it was called on the site — and a *failed* download leaves no trace
//! at all once its toast is gone, so a mod can simply be missing with no record it was ever tried.
//!
//! Same shape as [`crate::shop_installed`]: a small JSON file, best-effort, a corrupt one reads as
//! empty. History is never worth failing an install over.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Newest first; appending past this drops the oldest. A few hundred covers "what did I grab this
/// week" without the file growing forever.
const MAX_RECORDS: usize = 300;

/// One download, as recorded. `status` is `installed` or `failed`; `source` is `site`, `shop` or
/// `file` (imported or dragged in).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub id: String,
    /// Unix milliseconds, stamped here rather than by the caller so ordering is one clock's.
    pub at: u64,
    pub title: String,
    /// Empty for a dragged-in file: it has no page to go back to.
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub subpath: String,
    #[serde(default)]
    pub dest_folder: String,
    #[serde(default)]
    pub category_id: Option<u32>,
    pub source: String,
    /// Which mirror served it — MediaFire, Google Drive, MEGA…
    #[serde(default)]
    pub host: Option<String>,
    /// The link it came from, kept so a failed download can be retried straight from the row
    /// rather than sending the user back to find the mod again. Absent for shops and files.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub bytes: Option<u64>,
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// What a caller supplies. `id` and `at` are ours to assign.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDownload {
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub subpath: String,
    #[serde(default)]
    pub dest_folder: String,
    #[serde(default)]
    pub category_id: Option<u32>,
    pub source: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub bytes: Option<u64>,
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    /// Bumped per record, so two downloads inside the same millisecond still get distinct ids.
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    records: Vec<DownloadRecord>,
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("download-history.json")
}

fn load(dir: &Path) -> Store {
    match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

fn save(dir: &Path, store: &Store) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(store)?)?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Note one download. Returns the stored record so the caller can show it without re-reading.
///
/// A record with nothing to name is dropped rather than stored: a row with a blank title tells
/// the user less than no row at all.
pub fn record(dir: &Path, new: NewDownload) -> anyhow::Result<Option<DownloadRecord>> {
    if new.title.trim().is_empty() && new.slug.trim().is_empty() {
        return Ok(None);
    }
    let mut store = load(dir);
    store.seq = store.seq.wrapping_add(1);
    let at = now_ms();
    let entry = DownloadRecord {
        id: format!("{at:x}-{}", store.seq),
        at,
        title: new.title,
        slug: new.slug,
        subpath: new.subpath,
        dest_folder: new.dest_folder,
        category_id: new.category_id,
        source: new.source,
        host: new.host,
        url: new.url,
        bytes: new.bytes,
        status: new.status,
        error: new.error,
    };
    store.records.insert(0, entry.clone());
    store.records.truncate(MAX_RECORDS);
    save(dir, &store)?;
    Ok(Some(entry))
}

/// Everything recorded, newest first.
pub fn history(dir: &Path) -> Vec<DownloadRecord> {
    let mut records = load(dir).records;
    // Sorted on read as well as written: a store hand-edited or merged from another machine
    // should still come back in order.
    records.sort_by(|a, b| b.at.cmp(&a.at));
    records
}

/// Drop one row. Unknown ids are not an error — the row is gone either way.
pub fn forget(dir: &Path, id: &str) -> anyhow::Result<()> {
    let mut store = load(dir);
    let before = store.records.len();
    store.records.retain(|r| r.id != id);
    if store.records.len() != before {
        save(dir, &store)?;
    }
    Ok(())
}

/// Drop everything, keeping `seq` so ids stay unique against anything still on screen.
pub fn clear(dir: &Path) -> anyhow::Result<()> {
    let mut store = load(dir);
    store.records.clear();
    save(dir, &store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "frost-downloads-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(title: &str, status: &str) -> NewDownload {
        NewDownload {
            title: title.to_string(),
            slug: title.to_lowercase().replace(' ', "-"),
            subpath: "tracks".to_string(),
            dest_folder: "tracks".to_string(),
            category_id: Some(7),
            source: "site".to_string(),
            host: Some("MediaFire".to_string()),
            url: Some("https://example.invalid/file.pkz".to_string()),
            bytes: Some(1024),
            status: status.to_string(),
            error: None,
        }
    }

    #[test]
    fn a_record_round_trips() {
        let dir = tmp("roundtrip");
        record(&dir, sample("Cedar Ridge", "installed")).unwrap();

        let back = history(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].title, "Cedar Ridge");
        assert_eq!(back[0].host.as_deref(), Some("MediaFire"));
        assert!(back[0].at > 0, "a row with no time can't be sorted by time");
        let _ = fs::remove_dir_all(&dir);
    }

    /// The whole point of the page: the most recent download is the one at the top.
    #[test]
    fn history_comes_back_newest_first() {
        let dir = tmp("order");
        for n in 0..3 {
            record(&dir, sample(&format!("Track {n}"), "installed")).unwrap();
        }

        let back = history(&dir);
        assert_eq!(
            back.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            ["Track 2", "Track 1", "Track 0"]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Ids have to survive a burst: a bulk install writes several inside one millisecond, and a
    /// duplicate id would make dismissing one row dismiss another.
    #[test]
    fn ids_are_unique_across_a_burst() {
        let dir = tmp("ids");
        for n in 0..25 {
            record(&dir, sample(&format!("Track {n}"), "installed")).unwrap();
        }

        let ids: std::collections::HashSet<String> =
            history(&dir).into_iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 25);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_oldest_rows_fall_off_the_end() {
        let dir = tmp("cap");
        for n in 0..(MAX_RECORDS + 5) {
            record(&dir, sample(&format!("Track {n}"), "installed")).unwrap();
        }

        let back = history(&dir);
        assert_eq!(back.len(), MAX_RECORDS);
        assert_eq!(back[0].title, format!("Track {}", MAX_RECORDS + 4));
        assert!(
            !back.iter().any(|r| r.title == "Track 0"),
            "the cap drops the oldest, not the newest"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_keeps_its_error() {
        let dir = tmp("failure");
        let mut failed = sample("Sandbox Park", "failed");
        failed.error = Some("Google Drive has hit this file's download limit".to_string());
        record(&dir, failed).unwrap();

        let back = history(&dir);
        assert_eq!(back[0].status, "failed");
        assert!(back[0].error.as_deref().unwrap().contains("download limit"));
        assert!(
            back[0].url.is_some(),
            "without the link the row can't offer a retry"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_removes_only_that_row() {
        let dir = tmp("forget");
        record(&dir, sample("Keep", "installed")).unwrap();
        let drop = record(&dir, sample("Drop", "installed")).unwrap().unwrap();

        forget(&dir, &drop.id).unwrap();
        let back = history(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].title, "Keep");

        forget(&dir, "not-an-id").unwrap();
        assert_eq!(history(&dir).len(), 1, "an unknown id is a no-op");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_leaves_later_ids_unique() {
        let dir = tmp("clear");
        let first = record(&dir, sample("Old", "installed")).unwrap().unwrap();
        clear(&dir).unwrap();
        assert!(history(&dir).is_empty());

        let next = record(&dir, sample("New", "installed")).unwrap().unwrap();
        assert_ne!(first.id, next.id, "a cleared store must not reuse ids");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Nothing here is worth failing an install over.
    #[test]
    fn missing_corrupt_and_nameless_are_survivable() {
        let dir = tmp("robust");
        assert!(history(&dir).is_empty(), "no file yet reads as no history");

        let mut nameless = sample("", "installed");
        nameless.slug = String::new();
        assert!(record(&dir, nameless).unwrap().is_none());
        assert!(history(&dir).is_empty(), "a nameless row is not worth a line");

        fs::write(store_path(&dir), "not json").unwrap();
        assert!(history(&dir).is_empty(), "a corrupt store reads as empty");
        // ...and writing over it recovers rather than erroring.
        record(&dir, sample("After", "installed")).unwrap();
        assert_eq!(history(&dir).len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
