//! What the library *used* to hold, so deleting a mod doesn't erase that it existed.
//!
//! The library is a scan ([`crate::library::scan_library`]): it says what is on disk right
//! now and remembers nothing between passes. Delete a track and the app has never heard of
//! it. [`crate::downloads`] persists, but only for what the app itself fetched — a track
//! built in the editor and copied into `mods/tracks` by hand was never recorded anywhere.
//!
//! This is the other half: a presence ledger, reconciled against the tree, keeping a row per
//! mod from the first time it was seen until the player says to forget it. The row outlives
//! the files, which is the whole point — months later, "what was that track called" has an
//! answer, and a thumbnail to recognise it by.
//!
//! Same shape as [`crate::downloads`] and [`crate::shop_installed`]: a small JSON file,
//! best-effort, a corrupt one reads as empty. A scan is never worth failing over a ledger.
//!
//! Two things it must not get wrong:
//!
//! * **Parked is not gone.** Manage disables a mod by moving it into `mxbapp_disabled`
//!   ([`crate::modstate`]), which to a naive scan looks exactly like a deletion. Reconciling
//!   against [`crate::modstate::scan_with`] rather than a bare library scan is what keeps
//!   Manage from making the ledger cry wolf.
//! * **A tree that isn't there is not an empty tree.** An unplugged drive would otherwise
//!   bury the entire library in one pass. See the guards on [`reconcile_store`].

use crate::modstate::ModEntry;
use crate::pkz::PkzMeta;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PRESENT: &str = "present";
pub const PARKED: &str = "parked";
pub const GONE: &str = "gone";

/// How long a mod stays remembered after it goes. Long on purpose: the feature is for the
/// track you deleted and half-forgot, so anything short defeats it. Rows are only pruned
/// past this, or when the player forgets them by hand.
const KEEP_GONE_MS: u64 = 2 * 365 * 24 * 60 * 60 * 1000;

/// One mod, from the first time it was seen to now.
///
/// The snapshot fields are captured while the files still exist and are never cleared — they
/// are the only thing left once a mod is gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    /// Lowercased `rel` — the identity [`crate::modstate`] already keys mods by, so a mod
    /// keeps its row across being enabled, disabled and re-enabled.
    pub key: String,
    /// Last known path relative to the MX Bikes root, e.g. `mods/tracks/EU/RedBud.pkz`.
    pub rel: String,
    pub name: String,
    pub category: String,
    pub folder: String,
    pub size: u64,
    #[serde(default)]
    pub is_dir: bool,
    /// Unix milliseconds.
    pub first_seen: u64,
    pub last_seen: u64,
    /// [`PRESENT`], [`PARKED`] or [`GONE`].
    pub state: String,
    /// When it went. Stamped once per disappearance, not refreshed by later passes, so the
    /// date shown is when the mod actually left rather than when the app last looked.
    #[serde(default)]
    pub gone_at: Option<u64>,

    // --- snapshot, taken while the mod was still installed ---
    /// The mod's own declared name, which is often nothing like its filename.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub length: Option<u32>,
    /// File name under the thumbs folder — never the image itself. Several hundred inline
    /// base64 thumbnails would make every read of the store pay for pixels it isn't showing.
    #[serde(default)]
    pub thumb: Option<String>,
    /// Where the Trash put the files, when the app was the one that deleted them and could
    /// tell. What makes Restore possible rather than only a name to go hunting with; `None`
    /// means the mod went some other way, or the platform doesn't say.
    #[serde(default)]
    pub trashed_at: Option<String>,

    /// When the snapshot was taken, whether or not it found anything.
    ///
    /// Recorded rather than inferred from the fields above, because plenty of archives
    /// genuinely carry no metadata — an OEM `model.pkz`, a tyre pack, a gear archive. Judging
    /// by emptiness would leave those looking un-snapshotted forever, re-read on every pass
    /// and holding backfill slots that a mod with something to show should have had.
    #[serde(default)]
    pub snapshot_at: Option<u64>,
}

impl LedgerEntry {
    fn new(key: &str, m: &ModEntry, now: u64) -> Self {
        LedgerEntry {
            key: key.to_string(),
            rel: m.rel.clone(),
            name: m.name.clone(),
            category: m.category.clone(),
            folder: m.folder.clone(),
            size: m.size,
            is_dir: m.is_dir,
            first_seen: now,
            last_seen: now,
            state: PRESENT.to_string(),
            gone_at: None,
            title: None,
            author: None,
            location: None,
            length: None,
            thumb: None,
            trashed_at: None,
            snapshot_at: None,
        }
    }

    /// Whether the snapshot still needs taking. Checked before doing any work per mod, so a
    /// warm ledger costs one comparison a row.
    pub fn needs_snapshot(&self) -> bool {
        self.snapshot_at.is_none()
    }
}

/// A row on its way to the UI: the entry, plus its thumbnail inflated to a data URI.
///
/// The image lives on disk and is read only for the rows actually being shown, which is why
/// this is a separate shape from what gets persisted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRow {
    #[serde(flatten)]
    pub entry: LedgerEntry,
    /// `data:image/jpeg;base64,…`, or absent when no snapshot was ever taken.
    pub thumb_data: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    /// The tree these rows were last reconciled against. A change means the player repointed
    /// the app, which must not be read as "everything was deleted" — see [`reconcile_store`].
    #[serde(default)]
    pub mods_path: String,
    #[serde(default)]
    pub entries: BTreeMap<String, LedgerEntry>,
}

/// One ledger per title. MX Bikes and GP Bikes share an app data folder but not a library,
/// and merging them would report every one of the other title's mods as missing.
fn store_path(dir: &Path, game: &str) -> PathBuf {
    dir.join(format!("library-ledger-{game}.json"))
}

fn thumbs_dir(dir: &Path, game: &str) -> PathBuf {
    dir.join("ledger-thumbs").join(game)
}

pub fn load(dir: &Path, game: &str) -> Store {
    match fs::read_to_string(store_path(dir, game)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

pub fn save(dir: &Path, game: &str, store: &Store) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir, game), serde_json::to_string_pretty(store)?)?;
    Ok(())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The ledger's identity for a mod. [`ModEntry::rel`] is already normalised — forward-slashed
/// and rooted at `mods/` — so case is the only thing left to fold.
fn key_of(rel: &str) -> String {
    rel.to_lowercase()
}

/// Fold one pass of the tree into the store.
///
/// `scanned` is what [`crate::modstate::scan_with`] found, enabled and parked together.
/// `tree_ok` is whether the mods folder was actually readable.
///
/// Nothing is marked gone when the pass can't be trusted, because a wrong sweep is the one
/// failure that would make the whole feature not worth having:
///
/// * **An empty pass against a non-empty ledger.** A drive that isn't mounted, a mods path
///   pointing at nothing, a scan that errored — all arrive here as "no mods found", and
///   acting on that would bury an entire library at once.
/// * **A tree that changed underfoot.** Repointing the app at another install must not
///   declare the first one's contents deleted. The new path is adopted, and the pass after
///   this one — against a tree the store now agrees with — is free to mark things gone.
pub fn reconcile_store(store: &mut Store, mods_path: &str, scanned: &[ModEntry], tree_ok: bool, now: u64) {
    let repointed = !store.mods_path.is_empty() && store.mods_path != mods_path;
    store.mods_path = mods_path.to_string();

    let mut seen: HashSet<String> = HashSet::new();
    for m in scanned {
        let key = key_of(&m.rel);
        let e = store
            .entries
            .entry(key.clone())
            .or_insert_with(|| LedgerEntry::new(&key, m, now));
        e.rel = m.rel.clone();
        e.name = m.name.clone();
        e.category = m.category.clone();
        e.folder = m.folder.clone();
        e.size = m.size;
        e.is_dir = m.is_dir;
        e.last_seen = now;
        e.state = if m.enabled { PRESENT } else { PARKED }.to_string();
        e.gone_at = None;
        // Back on disk, so whatever was in the Trash is not what you would restore.
        e.trashed_at = None;
        seen.insert(key);
    }

    // A pass that found nothing, against a ledger that holds something, is far likelier to be
    // a tree we couldn't read than a library somebody emptied.
    let blind = scanned.is_empty() && !store.entries.is_empty();
    if !tree_ok || repointed || blind {
        return;
    }

    for e in store.entries.values_mut() {
        // Already gone: leave `gone_at` at the date it actually went.
        if e.state == GONE || seen.contains(&e.key) {
            continue;
        }
        e.state = GONE.to_string();
        e.gone_at = Some(now);
    }
}

/// Drop rows that have been gone longer than [`KEEP_GONE_MS`], and their thumbnails with
/// them. Returns how many went.
pub fn prune(dir: &Path, game: &str, store: &mut Store, now: u64) -> usize {
    let stale: Vec<String> = store
        .entries
        .values()
        .filter(|e| e.state == GONE && e.gone_at.is_some_and(|t| now.saturating_sub(t) > KEEP_GONE_MS))
        .map(|e| e.key.clone())
        .collect();
    for key in &stale {
        if let Some(e) = store.entries.remove(key) {
            drop_thumb(dir, game, &e);
        }
    }
    stale.len()
}

fn drop_thumb(dir: &Path, game: &str, e: &LedgerEntry) {
    if let Some(name) = &e.thumb {
        let _ = fs::remove_file(thumbs_dir(dir, game).join(name));
    }
}

/// Stable thumbnail filename for a row. Hashed rather than derived from the name so a mod
/// called `../../evil` can't place a file outside the thumbs folder.
fn thumb_name(key: &str) -> String {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}.jpg", hasher.finish())
}

/// Split a `data:image/jpeg;base64,…` URI into its bytes. `None` for anything else — the
/// snapshot is a nicety, and a shape we don't recognise just means no picture.
fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
    let b64 = uri.strip_prefix("data:image/jpeg;base64,")?;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// Fold a mod's metadata into its row, writing the thumbnail out beside the store.
///
/// Called while the mod is still installed — this is the last chance to capture any of it.
pub fn apply_snapshot(dir: &Path, game: &str, e: &mut LedgerEntry, meta: &PkzMeta, now: u64) {
    e.snapshot_at = Some(now);
    e.title = meta.name.clone();
    e.author = meta.author.clone();
    e.location = meta.location.clone();
    e.length = meta.length;

    let Some(bytes) = meta.thumbnail.as_deref().and_then(decode_data_uri) else {
        return;
    };
    let tdir = thumbs_dir(dir, game);
    if fs::create_dir_all(&tdir).is_err() {
        return;
    }
    let name = thumb_name(&e.key);
    if fs::write(tdir.join(&name), bytes).is_ok() {
        e.thumb = Some(name);
    }
}

/// Rows for the UI, newest-departed first, with thumbnails inflated.
///
/// Takes the entries rather than the whole store so the caller can narrow first: reading and
/// base64-ing a picture per row is only worth doing for rows about to be shown.
pub fn rows(dir: &Path, game: &str, entries: impl IntoIterator<Item = LedgerEntry>) -> Vec<LedgerRow> {
    let tdir = thumbs_dir(dir, game);
    let mut out: Vec<LedgerRow> = entries
        .into_iter()
        .map(|e| LedgerRow {
            thumb_data: e
                .thumb
                .as_deref()
                .and_then(|n| fs::read(tdir.join(n)).ok())
                .map(|b| {
                    format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&b)
                    )
                }),
            entry: e,
        })
        .collect();
    out.sort_by(|a, b| {
        b.entry
            .gone_at
            .unwrap_or(0)
            .cmp(&a.entry.gone_at.unwrap_or(0))
            .then_with(|| a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()))
    });
    out
}

/// Note where the Trash put a mod the app just deleted, against the row for `rel`.
///
/// Separate from [`reconcile_store`] because it has to happen at the moment of deletion —
/// once the next pass runs, all anyone knows is that the files are missing.
pub fn note_trashed(dir: &Path, game: &str, rel: &str, trashed_at: Option<String>) {
    let mut store = load(dir, game);
    let Some(e) = store.entries.get_mut(&key_of(rel)) else {
        return;
    };
    e.trashed_at = trashed_at;
    e.state = GONE.to_string();
    e.gone_at.get_or_insert(now_ms());
    let _ = save(dir, game, &store);
}

/// Forget one row and its thumbnail.
pub fn forget(dir: &Path, game: &str, key: &str) -> anyhow::Result<()> {
    let mut store = load(dir, game);
    if let Some(e) = store.entries.remove(&key.to_lowercase()) {
        drop_thumb(dir, game, &e);
        save(dir, game, &store)?;
    }
    Ok(())
}

/// Forget everything that is no longer installed. Present and parked rows stay: they aren't
/// history yet, and clearing them would only make the next pass write them straight back.
pub fn clear_gone(dir: &Path, game: &str) -> anyhow::Result<()> {
    let mut store = load(dir, game);
    let gone: Vec<String> = store
        .entries
        .values()
        .filter(|e| e.state == GONE)
        .map(|e| e.key.clone())
        .collect();
    for key in gone {
        if let Some(e) = store.entries.remove(&key) {
            drop_thumb(dir, game, &e);
        }
    }
    save(dir, game, &store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-ledger-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn entry(rel: &str, enabled: bool) -> ModEntry {
        ModEntry {
            rel: rel.to_string(),
            name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
            category: "track".to_string(),
            folder: String::new(),
            size: 10,
            enabled,
            is_dir: false,
        }
    }

    /// The core promise: a mod that leaves the tree is remembered, and dated.
    #[test]
    fn a_mod_that_vanishes_is_marked_gone_and_dated() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].state, PRESENT);

        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], true, 2_000);
        let e = &s.entries["mods/tracks/redbud.pkz"];
        assert_eq!(e.state, GONE);
        assert_eq!(e.gone_at, Some(2_000));
    }

    /// The date shown must be when the mod left, not when the app last looked.
    #[test]
    fn gone_at_is_stamped_once_not_refreshed() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], true, 2_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], true, 9_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].gone_at, Some(2_000));
    }

    /// The regression that matters most: Manage parks a mod, and the ledger must not call
    /// that a deletion. `scan_with` reports a parked mod under the path it would occupy when
    /// enabled, with `enabled: false` — so the row is still *seen*, just not present.
    #[test]
    fn a_mod_parked_by_manage_reads_parked_not_gone() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", false)], true, 2_000);

        let e = &s.entries["mods/tracks/redbud.pkz"];
        assert_eq!(e.state, PARKED, "disabled is not deleted");
        assert_eq!(e.gone_at, None);
    }

    /// An unplugged drive arrives here as "no mods found". Acting on it would bury the lot.
    #[test]
    fn an_empty_pass_against_a_full_ledger_marks_nothing_gone() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[], true, 2_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].state, PRESENT);
    }

    #[test]
    fn an_unreadable_tree_marks_nothing_gone() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], false, 2_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].state, PRESENT);
    }

    /// Repointing at a second install must not declare the first one's contents deleted —
    /// but the pass *after* the move, now against a tree the store agrees with, may.
    #[test]
    fn repointing_the_tree_merges_first_then_reconciles() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/old", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);

        reconcile_store(&mut s, "/new", &[entry("mods/tracks/Other.pkz", true)], true, 2_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].state, PRESENT, "not on the move itself");

        reconcile_store(&mut s, "/new", &[entry("mods/tracks/Other.pkz", true)], true, 3_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].state, GONE, "but on the next pass");
    }

    /// Reinstalling a mod restores it without losing when it first showed up.
    #[test]
    fn a_mod_that_comes_back_keeps_its_first_seen() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], true, 2_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 3_000);

        let e = &s.entries["mods/tracks/redbud.pkz"];
        assert_eq!(e.state, PRESENT);
        assert_eq!(e.first_seen, 1_000);
        assert_eq!(e.gone_at, None, "back means back — no lingering departure date");
    }

    /// And going again after coming back stamps the new date, not the old one.
    #[test]
    fn a_second_departure_is_stamped_afresh() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[], true, 2_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 3_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], true, 4_000);
        assert_eq!(s.entries["mods/tracks/redbud.pkz"].gone_at, Some(4_000));
    }

    /// Case is folded, so a folder renamed `Tracks` doesn't double every row.
    #[test]
    fn identity_folds_case() {
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        reconcile_store(&mut s, "/mods", &[entry("mods/Tracks/RedBud.pkz", true)], true, 2_000);
        assert_eq!(s.entries.len(), 1);
    }

    #[test]
    fn a_corrupt_store_reads_as_empty() {
        let dir = tmp("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(store_path(&dir, "mxb"), b"{ not json").unwrap();
        assert!(load(&dir, "mxb").entries.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_file_yet_reads_as_empty() {
        let dir = tmp("missing");
        assert!(load(&dir, "mxb").entries.is_empty());
    }

    #[test]
    fn ledgers_are_kept_apart_per_title() {
        let dir = tmp("per-game");
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        save(&dir, "mxb", &s).unwrap();

        assert_eq!(load(&dir, "mxb").entries.len(), 1);
        assert!(load(&dir, "gpb").entries.is_empty(), "the other title sees nothing");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_snapshot_survives_the_mod_and_comes_back_as_a_data_uri() {
        let dir = tmp("snapshot");
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);

        // A 1×1 JPEG is enough — this is about the round trip, not the pixels.
        let jpeg = base64::engine::general_purpose::STANDARD.encode([0xFF, 0xD8, 0xFF, 0xD9]);
        let meta = PkzMeta {
            locked: false,
            name: Some("Red Bud National".into()),
            author: Some("Someone".into()),
            location: Some("Michigan".into()),
            length: Some(1600),
            altitude: None,
            thumbnail: Some(format!("data:image/jpeg;base64,{jpeg}")),
        };
        let e = s.entries.get_mut("mods/tracks/redbud.pkz").unwrap();
        assert!(e.needs_snapshot());
        apply_snapshot(&dir, "mxb", e, &meta, 1_500);
        assert!(!e.needs_snapshot());

        // Now delete the mod.
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Other.pkz", true)], true, 2_000);

        let row = rows(&dir, "mxb", s.entries.values().cloned())
            .into_iter()
            .find(|r| r.entry.key == "mods/tracks/redbud.pkz")
            .unwrap();
        assert_eq!(row.entry.state, GONE);
        assert_eq!(row.entry.title.as_deref(), Some("Red Bud National"));
        assert_eq!(row.entry.location.as_deref(), Some("Michigan"));
        assert!(
            row.thumb_data.is_some_and(|d| d.starts_with("data:image/jpeg;base64,")),
            "the picture outlives the file — that's the whole feature"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Plenty of archives carry no metadata at all — an OEM `model.pkz`, a tyre pack, a gear
    /// archive. Those must still count as snapshotted, or every capture pass re-reads them
    /// forever and they hold backfill slots a mod with something to show should have had.
    #[test]
    fn a_mod_with_no_metadata_is_not_asked_again() {
        let dir = tmp("empty-meta");
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/Bare.pkz", true)], true, 1_000);

        let e = s.entries.get_mut("mods/tracks/bare.pkz").unwrap();
        assert!(e.needs_snapshot());
        apply_snapshot(&dir, "mxb", e, &PkzMeta::default(), 1_500);

        assert!(e.title.is_none(), "there was nothing to find");
        assert!(!e.needs_snapshot(), "but it was looked for, and that is what counts");
        assert_eq!(e.snapshot_at, Some(1_500));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_a_row_takes_its_thumbnail_with_it() {
        let dir = tmp("forget");
        let mut s = Store::default();
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/RedBud.pkz", true)], true, 1_000);
        let jpeg = base64::engine::general_purpose::STANDARD.encode([0xFF, 0xD8]);
        let meta = PkzMeta {
            locked: false,
            name: None,
            author: None,
            location: None,
            length: None,
            altitude: None,
            thumbnail: Some(format!("data:image/jpeg;base64,{jpeg}")),
        };
        let e = s.entries.get_mut("mods/tracks/redbud.pkz").unwrap();
        apply_snapshot(&dir, "mxb", e, &meta, 1_500);
        let thumb = thumbs_dir(&dir, "mxb").join(e.thumb.clone().unwrap());
        assert!(thumb.is_file());
        save(&dir, "mxb", &s).unwrap();

        forget(&dir, "mxb", "mods/tracks/redbud.pkz").unwrap();
        assert!(load(&dir, "mxb").entries.is_empty());
        assert!(!thumb.exists(), "no orphan images left behind");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_drops_the_gone_and_keeps_the_installed() {
        let dir = tmp("clear");
        let mut s = Store::default();
        reconcile_store(
            &mut s,
            "/mods",
            &[entry("mods/tracks/A.pkz", true), entry("mods/tracks/B.pkz", true)],
            true,
            1_000,
        );
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/A.pkz", true)], true, 2_000);
        save(&dir, "mxb", &s).unwrap();

        clear_gone(&dir, "mxb").unwrap();
        let back = load(&dir, "mxb");
        assert_eq!(back.entries.len(), 1);
        assert!(back.entries.contains_key("mods/tracks/a.pkz"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pruning_only_touches_rows_gone_a_very_long_time() {
        let dir = tmp("prune");
        let mut s = Store::default();
        reconcile_store(
            &mut s,
            "/mods",
            &[entry("mods/tracks/A.pkz", true), entry("mods/tracks/B.pkz", true)],
            true,
            1_000,
        );
        reconcile_store(&mut s, "/mods", &[entry("mods/tracks/A.pkz", true)], true, 2_000);

        assert_eq!(prune(&dir, "mxb", &mut s, 2_000 + KEEP_GONE_MS), 0, "not yet");
        assert_eq!(prune(&dir, "mxb", &mut s, 2_001 + KEEP_GONE_MS), 1);
        assert!(s.entries.contains_key("mods/tracks/a.pkz"), "the installed one stays");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod restore_tests {
    use super::*;
    use crate::library;

    /// The full round trip on a real filesystem: install, delete to the Trash, put it back.
    #[test]
    #[ignore]
    fn a_trashed_mod_comes_back() {
        let root = std::env::temp_dir().join(format!("frost-restore-{}", std::process::id()));
        let file = root.join("mods/tracks/RestoreMe.pkz");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"a mod").unwrap();

        let landed = library::move_to_trash(&file).unwrap();
        println!("trashed to: {landed:?}");
        assert!(!file.exists(), "gone from the mods folder");
        assert!(landed.is_some(), "and we know where it went");

        library::restore_from_trash(&file, landed.as_deref()).unwrap();
        assert!(file.exists(), "and back again");
        assert_eq!(fs::read(&file).unwrap(), b"a mod", "with its contents intact");

        // Restoring over something already there must refuse rather than clobber.
        let landed2 = library::move_to_trash(&file).unwrap();
        fs::write(&file, b"newer copy").unwrap();
        let err = library::restore_from_trash(&file, landed2.as_deref()).unwrap_err();
        println!("refused as expected: {err}");
        assert_eq!(fs::read(&file).unwrap(), b"newer copy", "the newer copy survived");

        let _ = fs::remove_dir_all(&root);
        if let Some(p) = landed2 {
            let _ = fs::remove_file(p);
        }
    }
}
