//! Race mode: before an app-launched join, set aside every mod the server doesn't need.
//!
//! MX Bikes mounts every `.pkz` under `mods` at startup, and it does it again on the way into
//! a server. A rider with three hundred tracks pays for all three hundred to race on one of
//! them. The server list already says which track a server runs and which bike classes it
//! lets in, so the app knows before the game starts what the session can possibly use.
//! Everything else can step aside for the length of the session — the same move Manage makes
//! (see [`crate::modstate`]), done automatically and undone when the game exits.
//!
//! [`set_aside`] is the *decision*, and nothing else: which of the installed mods to move. It
//! reads no disk and moves no file, so every rule is a unit test rather than a folder of
//! fixtures. The second half of this file builds its inventory (the library scan, each bike's
//! `[data] cat`, each track's inner folders) and does the moving — see [`before_join`].
//!
//! The rules lean hard towards keeping. A mod set aside that the session needed is a rider
//! who can't see the track, or a grid of riders on bikes the game can't draw; a mod kept that
//! the session didn't need costs a few hundred milliseconds of mounting. So:
//!
//! * Only **whole tracks** and **whole packed bikes** are candidates. Rider gear, paints,
//!   tyres, sounds and support packs are never moved, whatever the server runs.
//! * Anything the app couldn't read is kept — an archive it can't open, a bike with no
//!   category, a folder it can't place.
//! * An archive holding one needed thing and one unneeded thing is kept whole. Nothing here
//!   unpacks an archive to take half of it away.
//! * With no known server track, or a track this install doesn't have, it does nothing at
//!   all: the one mod that has to be there is the one it can't point at.

//!
//! Moving is [`crate::modstate`]'s own mechanism — into `mxbapp_disabled`, mirroring the path
//! — with one difference that matters: Manage's restore walks the whole shadow tree, and Race
//! mode must not. A player who parked forty tracks by hand before joining expects those forty
//! to still be parked afterwards. So every Race mode session writes a journal of exactly what
//! *it* moved, before the first move, and puts back only that.

use crate::bikeswap;
use crate::config::AppConfig;
use crate::modstate::{self, StateOutcome};
use mxb_core::tracksource;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// What a server is running, as the server list reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerNeeds {
    /// The track id the server publishes — the folder name inside the track's archive, so
    /// `Farm14` has to find `Farm 14.pkz`. Compared through [`tracksource::key`].
    pub track: String,
    /// The layout of that track. Carried for the record and the log: a layout lives inside
    /// the track's own archive, so it never changes what has to stay.
    pub track_layout: String,
    /// The bike categories the server lets in (`MX1 OEM`, `MX2 OEM`, …). Empty means Open —
    /// any bike — which is also what the in-game browser shows as "Any".
    pub categories: Vec<String>,
    /// The track ships with the game, so there's no mod of it to find. A stock track is the
    /// one case where not finding the server's track installed is not a reason to stop.
    pub track_is_stock: bool,
}

/// One bike an archive carries, as its own config names it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BikeContent {
    /// `<name>.cfg` `ID` — what the profile's `bikeid` names.
    pub id: String,
    /// `<name>.ini` `[data] cat`. Blank when the bike declares none.
    pub class: String,
}

/// One installed mod, as the inventory pass saw it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// Path relative to the MX Bikes root, `mods/`-prefixed — [`crate::modstate::ModEntry`]'s
    /// `rel`, and the key the moving half addresses the mod by.
    pub rel: String,
    /// Library category: `track`, `bike`, `helmet`, `bikePaint`, `sound`, …
    pub category: String,
    /// An extracted folder rather than a single archive.
    pub is_dir: bool,
    /// The game can see it. A mod the player already parked isn't Race mode's to move, and
    /// it can't be what the session is waiting on either.
    pub enabled: bool,
    /// The contents were actually read — a track's marker files found, a bike's identity
    /// parsed. `false` is "couldn't tell", and couldn't tell is kept.
    pub known: bool,
    /// For a track: every track id it carries — its own stem and the folder of each track
    /// inside it. More than one for a pack.
    pub tracks: Vec<String>,
    /// For a bike: every bike it carries. More than one for a pack.
    pub bikes: Vec<BikeContent>,
    /// Holds a `.mxbsecure` or `.mxbkey` somewhere inside. Secured content has a key and a
    /// lease tied to where it sits, and is never moved.
    pub protected: bool,
}

/// What the player is riding, from the active profile and preset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    /// The profile's `bikeid`. Blank when there's no profile to read.
    pub bike_id: String,
}

/// Why Race mode left everything where it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// The server list didn't say which track this server runs.
    NoTrack,
    /// It did, but no enabled mod here carries it and it isn't a stock track — either the
    /// player doesn't have it (the game will say so) or it's named in a way we can't match.
    /// Either way, moving tracks now could only move the one that matters.
    TrackNotInstalled(String),
}

/// Archives by these names are shared pieces other mods lean on, not mods of their own.
/// Matched against every path segment, so a `mods/tracks/common/…` folder is covered too.
const SUPPORT_WORDS: [&str; 4] = ["common", "misc", "support", "shared"];

/// Extensions of secured content. The blob and the key beside it move together or not at
/// all, and "not at all" is the only one of those that can't strand a key.
const PROTECTED_EXTS: [&str; 2] = [".mxbsecure", ".mxbkey"];

/// Normalize a rel for comparison, the way [`crate::modstate`] does: case and slash
/// direction both vary by where the path came from.
fn key(rel: &str) -> String {
    rel.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

/// `pinned` is at `rel` or somewhere inside it.
fn holds(rel: &str, pinned: &BTreeSet<String>) -> bool {
    let k = key(rel);
    pinned.contains(&k) || pinned.iter().any(|p| p.starts_with(&format!("{k}/")))
}

fn is_protected(item: &Item) -> bool {
    let lower = item.rel.to_lowercase();
    item.protected || PROTECTED_EXTS.iter().any(|x| lower.ends_with(x))
}

fn is_support_pack(rel: &str) -> bool {
    key(rel)
        .split('/')
        .any(|seg| SUPPORT_WORDS.iter().any(|w| seg.contains(w)))
}

/// What kind of candidate an item is, or `None` when it isn't one at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Track,
    Bike,
}

/// Whether Race mode is willing to move this mod, before asking whether the server needs it.
///
/// Deliberately a short allow-list rather than a list of exceptions: a new kind of content
/// the library learns to categorize next month stays put until someone decides otherwise.
fn candidate(item: &Item) -> Option<Candidate> {
    if !item.enabled || !item.known || is_protected(item) || is_support_pack(&item.rel) {
        return None;
    }
    match item.category.as_str() {
        "track" if !item.tracks.is_empty() => Some(Candidate::Track),
        // Packed only. A bike installed as a folder carries its liveries and model-swap sets
        // inside it, which other parts of the app track by path — and Manage's restore walk
        // can't tell a parked bike folder from a grouping folder.
        "bike" if !item.is_dir && !item.bikes.is_empty() => Some(Candidate::Bike),
        _ => None,
    }
}

/// Does any server category let this class in? Each entry may itself be a `/`-separated
/// list, the dedicated server's own `[event] category` shape.
fn class_allowed(class: &str, categories: &[String]) -> bool {
    categories.is_empty()
        || categories
            .iter()
            .any(|c| !c.trim().is_empty() && bikeswap::class_matches(class, c))
}

fn track_needed(item: &Item, want: &str) -> bool {
    item.tracks.iter().any(|t| tracksource::key(t) == want)
}

/// A bike archive stays when *anything* in it might be ridden: a bike in one of the server's
/// classes, a bike with no class to judge by, or the one the player has selected.
fn bike_needed(item: &Item, server: &ServerNeeds, player: &Player) -> bool {
    let mine = player.bike_id.trim();
    item.bikes.iter().any(|b| {
        b.class.trim().is_empty()
            || class_allowed(&b.class, &server.categories)
            || (!mine.is_empty() && b.id.trim().eq_ignore_ascii_case(mine))
    })
}

/// The mods to set aside for this session, as `rel` paths, sorted.
///
/// `pinned` is every file that must stay visible whatever else happens: the player's own
/// paints and gear, and everything paint sync installed. A candidate holding one of them is
/// kept whole — the game reads a livery from inside its bike's folder, and moving the folder
/// would take the livery with it.
pub fn set_aside(
    server: &ServerNeeds,
    items: &[Item],
    player: &Player,
    pinned: &[String],
) -> Result<Vec<String>, Skip> {
    let want = tracksource::key(&server.track);
    if want.is_empty() {
        return Err(Skip::NoTrack);
    }
    // The server's track has to be *here* before anything else moves. Matched against every
    // enabled track, candidate or not — a secured or unreadable archive by the right name is
    // still the track, and it is kept either way.
    let installed = items.iter().any(|i| {
        i.enabled
            && i.category == "track"
            && (track_needed(i, &want)
                || tracksource::key(&stem(&i.rel)) == want)
    });
    if !installed && !server.track_is_stock {
        return Err(Skip::TrackNotInstalled(server.track.clone()));
    }

    let pinned: BTreeSet<String> = pinned.iter().map(|p| key(p)).filter(|p| !p.is_empty()).collect();
    let mut out: BTreeSet<String> = BTreeSet::new();
    for item in items {
        let needed = match candidate(item) {
            None => continue,
            Some(Candidate::Track) => track_needed(item, &want),
            Some(Candidate::Bike) => bike_needed(item, server, player),
        };
        if !needed && !holds(&item.rel, &pinned) {
            out.insert(item.rel.clone());
        }
    }
    Ok(out.into_iter().collect())
}

/// `mods/tracks/EU/Farm 14.pkz` → `Farm 14`.
fn stem(rel: &str) -> String {
    let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
    mxb_core::library::strip_ext(name)
}

// ─── Doing it ───────────────────────────────────────────────────────────────────────────────

/// The journal's file name under the app's data folder.
const JOURNAL_FILE: &str = "race_mode.json";

/// Event the frontend listens on for the "N mods set aside" line. Carries a [`Status`].
pub const EVENT: &str = "race-mode";

/// How long after a launch the game has to show up before the moves are undone. A Steam
/// launch the player cancelled, or one Steam never acted on, has no session to end — without
/// this the library would stay narrowed until the next app start.
const LAUNCH_GRACE: Duration = Duration::from_secs(180);

/// How old a server-book row may be and still be trusted with the track. Servers rotate
/// tracks between events, and a stale row is the one way Race mode could set aside the very
/// track the session loads — so an old row is treated as no row at all.
const BOOK_FRESH_MS: u64 = 10 * 60 * 1000;

/// Everything Race mode moved for one session, written before the first move.
///
/// This file *is* the record. Restore reads it and nothing else — not the shadow tree, which
/// also holds whatever the player parked themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Journal {
    pub session_id: String,
    /// The server's `host:port`.
    pub server: String,
    pub server_name: String,
    pub track: String,
    /// The MX Bikes folder the moves were made in. Restore goes back to *this* tree, even
    /// if the player pointed the app at another folder while the game was up.
    pub mods_path: String,
    /// `rel` paths, as [`modstate`] addresses them. Before the moves: everything about to
    /// move. After: exactly what did.
    pub moved: Vec<String>,
    /// Unix milliseconds.
    pub started_at: u64,
    /// Nothing was moved: FrostMod was handed [`FILTER_FILE`] instead, and `moved` is what it
    /// hides. Restoring is deleting that file.
    pub filtered: bool,
}

// ─── The FrostMod filter ────────────────────────────────────────────────────────────────────
//
// FrostMod v0.40.0 filters the game's own content scan by an allow-list: a track or bike that
// isn't on it is never listed and its archive never opened, which is everything moving it bought
// with none of the moving. Nothing to put back after a crash either — delete the list and the
// next start sees the whole library. Moving stays for a FrostMod without the filter.

/// The allow-list FrostMod reads, beside `frostmod_mods.txt` in its folder.
const FILTER_FILE: &str = "frostmod_racemode.txt";

/// The roots FrostMod filters. Everything else under `mods/` is left alone by it, and so here.
const FILTER_ROOTS: [&str; 2] = ["tracks", "bikes"];

/// Every entry under `mods/tracks` and `mods/bikes` that stays visible when `hidden` (rels,
/// `mods/`-prefixed) is set aside, as paths relative to `mods/` with forward slashes.
///
/// The complement is taken on disk rather than from the inventory, because the inventory only
/// holds what Race mode knows how to judge: a stock bike's `paints` folder, a loose readme or a
/// grouping folder is none of those, and FrostMod hides whatever the list leaves off. A folder
/// that holds something hidden is walked into; anything else is listed whole.
fn allow_list(mods_root: &Path, hidden: &[String]) -> Vec<String> {
    let hidden: BTreeSet<String> = hidden
        .iter()
        .map(|r| key(r))
        .map(|k| k.strip_prefix("mods/").map(str::to_owned).unwrap_or(k))
        .collect();
    let mut out = Vec::new();
    fn visit(dir: &Path, rel: &str, hidden: &BTreeSet<String>, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let child = format!("{rel}/{name}");
            let k = key(&child);
            if hidden.contains(&k) {
                continue;
            }
            let holds_hidden = hidden.iter().any(|h| h.starts_with(&format!("{k}/")));
            if holds_hidden && e.path().is_dir() {
                visit(&e.path(), &child, hidden, out);
            } else {
                out.push(child);
            }
        }
    }
    for root in FILTER_ROOTS {
        visit(&mods_root.join(root), root, &hidden, &mut out);
    }
    out
}

/// Can this install hand FrostMod the filter instead of moving files?
fn filter_usable(app: &tauri::AppHandle, cfg: &AppConfig) -> bool {
    cfg.auto_run_frostmod
        && crate::frostmod::race_filter_supported(crate::frostmod_manage::installed_version(app).as_deref())
}

fn filter_path(app: &tauri::AppHandle) -> PathBuf {
    crate::frostmod_manage::frostmod_dir(app).join(FILTER_FILE)
}

/// Write the allow-list, whole or not at all.
fn write_filter(path: &Path, lines: &[String]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("txt.tmp");
    let mut text = String::from("# Written by MXB App's Auto race mode for this session. Deleted when it ends.\n");
    for l in lines {
        text.push_str(l);
        text.push('\n');
    }
    fs::write(&tmp, text).map_err(|e| format!("couldn't write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

fn clear_filter(path: &Path) {
    if let Err(e) = fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("[race] couldn't remove {}: {e}", path.display());
        }
    }
}

/// What the "Race mode: N mods set aside" line shows.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub active: bool,
    pub count: usize,
    pub server_name: String,
}

/// One Race mode operation at a time. The game exiting, paint sync ending its session and
/// the launch watchdog can all ask for a restore within the same second.
static LOCK: Mutex<()> = Mutex::new(());

fn read_journal(path: &Path) -> Option<Journal> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(j) => Some(j),
        Err(e) => {
            // Left in place rather than deleted: it's the only list of what moved. Manage's
            // Restore all still brings every parked mod back, journal or not.
            log::warn!(
                "[race] journal at {} is unreadable ({e}); leaving it. Manage → Restore all \
                 puts everything back.",
                path.display()
            );
            None
        }
    }
}

/// Write-then-rename, so a crash mid-write leaves the old journal or the new one — never
/// half of one, which would be a list of moves nobody can read back.
fn write_journal(path: &Path, journal: &Journal) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(journal).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("couldn't write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

fn clear_journal(path: &Path) {
    if let Err(e) = fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("[race] couldn't remove {}: {e}", path.display());
        }
    }
}

/// Move `rels` out of the way, journal first.
///
/// The journal is written with the whole plan before anything moves, so a crash at any point
/// leaves a record that covers every file that could have moved (restoring one that didn't is
/// a no-op). Once the moves are done it's rewritten with exactly what did.
///
/// One failure stops the run and undoes it: half a Race mode is a session missing mods for
/// no reason the player can see, while none at all is just an ordinary join.
fn park(journal_path: &Path, mut journal: Journal, rels: &[String]) -> Result<Journal, String> {
    journal.moved = rels.to_vec();
    write_journal(journal_path, &journal)?;

    let mut moved: Vec<String> = Vec::new();
    for rel in rels {
        match modstate::set_one(&journal.mods_path, rel, false) {
            Ok(true) => moved.push(rel.clone()),
            // Gone since the scan. Nothing to move, nothing to put back.
            Ok(false) => {}
            Err(e) => {
                let mut stuck = Vec::new();
                for back in moved.iter().rev() {
                    if let Err(e) = modstate::set_one(&journal.mods_path, back, true) {
                        log::warn!("[race] rollback couldn't put back {back}: {e:#}");
                        stuck.push(back.clone());
                    }
                }
                if stuck.is_empty() {
                    clear_journal(journal_path);
                } else {
                    // Still recorded, so the next restore pass tries again.
                    journal.moved = stuck;
                    let _ = write_journal(journal_path, &journal);
                }
                return Err(format!(
                    "couldn't set aside {rel}: {e:#} — put back the {} already moved",
                    moved.len()
                ));
            }
        }
    }

    journal.moved = moved;
    if journal.moved.is_empty() {
        clear_journal(journal_path);
    } else {
        write_journal(journal_path, &journal)?;
    }
    Ok(journal)
}

/// Put back what the journal says Race mode moved, and only that. `None` with no journal.
///
/// A path that won't go back stays in the journal for the next pass; the rest are done.
fn unpark(journal_path: &Path) -> Option<StateOutcome> {
    let mut journal = read_journal(journal_path)?;
    let mut out = StateOutcome::default();
    let mut left = Vec::new();
    for rel in &journal.moved {
        match modstate::set_one(&journal.mods_path, rel, true) {
            Ok(true) => out.enabled += 1,
            // Already back — Manage's Restore all got there first — or it never moved.
            Ok(false) => {}
            Err(e) => {
                out.failed.push((rel.clone(), format!("{e:#}")));
                left.push(rel.clone());
            }
        }
    }
    if left.is_empty() {
        clear_journal(journal_path);
    } else {
        journal.moved = left;
        let _ = write_journal(journal_path, &journal);
    }
    Some(out)
}

/// The volume a path lives on, following links: a `mods\tracks` junction onto another drive
/// is on that drive, whatever its path says.
#[cfg(windows)]
fn volume_of(p: &Path) -> Option<String> {
    let real = fs::canonicalize(p).ok()?;
    match real.components().next()? {
        std::path::Component::Prefix(pre) => {
            Some(pre.as_os_str().to_string_lossy().to_lowercase())
        }
        _ => None,
    }
}

#[cfg(unix)]
fn volume_of(p: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    Some(fs::metadata(p).ok()?.dev().to_string())
}

fn nearest_existing(p: &Path) -> Option<PathBuf> {
    p.ancestors().find(|a| a.exists()).map(Path::to_path_buf)
}

/// Every folder `rels` leave from has to share a volume with the shadow tree.
///
/// [`modstate`]'s moves fall back to copy-and-delete across volumes, which is fine for a
/// dozen mods from Manage and not fine here: gigabytes copied on the way into a server,
/// and again on the way out, is the opposite of joining faster. Refused rather than
/// attempted, and "can't tell" is refused too.
fn same_volume(mods_path: &str, rels: &[String]) -> Result<(), String> {
    same_volume_by(mods_path, rels, volume_of)
}

/// [`same_volume`], with the drive lookup as a parameter so a second drive can be faked.
fn same_volume_by(
    mods_path: &str,
    rels: &[String],
    volume_of: impl Fn(&Path) -> Option<String>,
) -> Result<(), String> {
    let shadow = modstate::shadow_root(mods_path);
    let at = nearest_existing(&shadow).ok_or("the MX Bikes folder doesn't exist")?;
    let home = volume_of(&at).ok_or_else(|| format!("couldn't tell which drive {} is on", at.display()))?;
    let mut seen = BTreeSet::new();
    for rel in rels {
        let Some(dir) = modstate::enabled_path(mods_path, rel).parent().map(Path::to_path_buf) else {
            continue;
        };
        if !seen.insert(dir.clone()) {
            continue;
        }
        let vol = volume_of(&dir).ok_or_else(|| format!("couldn't tell which drive {} is on", dir.display()))?;
        if vol != home {
            return Err(format!(
                "{} is on a different drive from {} — moving between drives is a copy, not a rename",
                dir.display(),
                shadow.display()
            ));
        }
    }
    Ok(())
}

/// What a track archive carries, keyed by path, size and mtime — reading an archive's file
/// list for every track on every join would be the join getting *slower*.
type TrackRead = (bool, Vec<String>);
static TRACK_READS: Mutex<Option<HashMap<(String, u64, u64), TrackRead>>> = Mutex::new(None);

/// The marker files the game's track loader looks for — the same set Manage's shadow walk
/// uses to recognise a parked track.
const TRACK_MARKERS: [&str; 5] = ["map", "trh", "tsc", "rdf", "ssc"];

/// Whether a packed track really holds a track, and the id of each one it holds.
fn read_track(path: &Path) -> TrackRead {
    let meta = fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cache_key = (path.to_string_lossy().into_owned(), size, mtime);
    if let Some(hit) = TRACK_READS
        .lock()
        .ok()
        .and_then(|c| c.as_ref().and_then(|m| m.get(&cache_key).cloned()))
    {
        return hit;
    }

    let read = match crate::pkz::entry_names(path) {
        Ok(names) => {
            let mut ids: Vec<String> = Vec::new();
            for n in &names {
                let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                if !TRACK_MARKERS.contains(&ext.as_str()) {
                    continue;
                }
                if let Some((top, _)) = n.split_once('/').filter(|(t, _)| !t.is_empty()) {
                    if !ids.iter().any(|i| i.eq_ignore_ascii_case(top)) {
                        ids.push(top.to_string());
                    }
                }
            }
            // Markers at the archive root still make it a track, just one named by its file.
            let known = names.iter().any(|n| {
                TRACK_MARKERS.contains(&n.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str())
            });
            (known, ids)
        }
        Err(_) => (false, Vec::new()),
    };
    if let Ok(mut c) = TRACK_READS.lock() {
        c.get_or_insert_with(HashMap::new).insert(cache_key, read.clone());
    }
    read
}

/// A `.mxbsecure` or `.mxbkey` anywhere under `dir`. Bounded, because an extracted track can
/// hold thousands of files and none of them is where a key would be put.
fn holds_secured(dir: &Path, depth: usize) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| {
        let p = e.path();
        if p.is_dir() {
            return depth > 0 && holds_secured(&p, depth - 1);
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        PROTECTED_EXTS.iter().any(|x| name.ends_with(x))
    })
}

/// The installed library as [`set_aside`] reads it.
fn inventory(cfg: &AppConfig, sound_bikes: &[String]) -> Vec<Item> {
    modstate::scan(cfg, sound_bikes)
        .into_iter()
        .map(|m| {
            let mut item = Item {
                rel: m.rel.clone(),
                category: m.category.clone(),
                is_dir: m.is_dir,
                enabled: m.enabled,
                known: true,
                ..Default::default()
            };
            // A parked mod is never a candidate, so there's nothing worth reading in it.
            if !m.enabled {
                return item;
            }
            let path = modstate::enabled_path(&cfg.mods_path, &m.rel);
            match m.category.as_str() {
                "track" if m.is_dir => {
                    // The library only calls a folder a track once it has found the markers.
                    item.tracks = vec![m.name.clone()];
                    item.protected = holds_secured(&path, 4);
                }
                "track" => {
                    let (known, mut ids) = read_track(&path);
                    ids.insert(0, stem(&m.rel));
                    item.known = known;
                    item.tracks = ids;
                }
                "bike" if !m.is_dir => match bikeswap::read_identity(&path) {
                    Some(b) => item.bikes = vec![BikeContent { id: b.id, class: b.class }],
                    None => item.known = false,
                },
                _ => {}
            }
            item
        })
        .collect()
}

/// The profile's selected bike, the way the server join reads it.
fn player(cfg: &AppConfig) -> Player {
    let dir = cfg.profiles_dir();
    let scan = crate::presets::scan_profiles(&dir);
    let bike_id = scan
        .active
        .or_else(|| scan.profiles.first().cloned())
        .and_then(|p| crate::presets::active_bike(&dir, &p))
        .unwrap_or_default();
    Player { bike_id }
}

/// Everything paint sync put in the mods folder, as `mods/…` rels.
fn synced_paints(cfg: &AppConfig) -> Vec<String> {
    crate::paintsync::Manifest::read(&crate::library::mods_root(&cfg.mods_path))
        .installed
        .into_keys()
        .map(|rel| format!("mods/{rel}"))
        .collect()
}

/// The server-list row for `address`, when one is fresh enough to trust with the track.
fn server_row(app: &tauri::AppHandle, address: &str) -> Option<crate::WorldServer> {
    let want = crate::gameproc::parse_server_address(address).ok()?;
    let same = |a: &str| {
        crate::gameproc::parse_server_address(a)
            .is_ok_and(|a| a.eq_ignore_ascii_case(&want))
    };
    if let Some(row) = crate::serverwatch::warm_list()
        .and_then(|l| l.servers.into_iter().find(|s| same(&s.address)))
    {
        return Some(row);
    }
    let now = crate::ledger::now_ms();
    crate::serverbook::load(app)
        .into_iter()
        .find(|e| same(&e.row.address) && now.saturating_sub(e.last_seen) <= BOOK_FRESH_MS)
        .map(|e| e.row)
}

fn journal_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    Some(app.path().app_data_dir().ok()?.join(JOURNAL_FILE))
}

/// What Race mode is holding right now.
pub fn status(app: &tauri::AppHandle) -> Status {
    match journal_path(app).and_then(|p| read_journal(&p)) {
        Some(j) => Status {
            active: !j.moved.is_empty(),
            count: j.moved.len(),
            server_name: j.server_name,
        },
        None => Status::default(),
    }
}

fn announce(app: &tauri::AppHandle) {
    use tauri::Emitter;
    let _ = app.emit(EVENT, status(app));
}

/// Set aside what the server at `address` can't use, before the game is started into it.
///
/// Best-effort from end to end: every way this can decline or fail leaves an ordinary join,
/// and says why in the log. Never touches a running game's files — a join made from the
/// in-game browser never gets here, and one made while the game is up is refused below.
pub fn before_join(app: &tauri::AppHandle, cfg: &AppConfig, address: &str) {
    if !cfg.race_mode {
        return;
    }
    if crate::gameproc::is_game_running() {
        log::info!("[race] the game is already running; leaving the mods folder alone");
        return;
    }
    let Some(path) = journal_path(app) else { return };
    let _held = LOCK.lock().unwrap_or_else(|p| p.into_inner());

    // An earlier session that never got its restore — the app was closed mid-game, say.
    // Back first, so this session's plan starts from the whole library.
    if path.exists() {
        restore_locked(app, cfg, &path, "a new join");
    }

    let Some(row) = server_row(app, address) else {
        log::info!("[race] {address} isn't in a recent server list, so its track is unknown; nothing set aside");
        return;
    };
    let needs = ServerNeeds {
        track: row.track.clone(),
        track_layout: row.track_layout.clone(),
        categories: row.categories.clone(),
        track_is_stock: !row.track.trim().is_empty()
            && crate::trackstock::find(&cfg.install_dir(), &row.track).is_some(),
    };
    let items = inventory(cfg, &crate::sound_bikes_of(app));
    let rels = match set_aside(&needs, &items, &player(cfg), &synced_paints(cfg)) {
        Ok(rels) if rels.is_empty() => {
            log::info!("[race] {}: everything installed is needed; nothing set aside", row.name);
            return;
        }
        Ok(rels) => rels,
        Err(Skip::NoTrack) => {
            log::info!("[race] {} doesn't say which track it runs; nothing set aside", row.name);
            return;
        }
        Err(Skip::TrackNotInstalled(t)) => {
            log::info!("[race] {} runs {t:?}, which isn't installed here; nothing set aside", row.name);
            return;
        }
    };
    // FrostMod's filter first: nothing moves, and the journal only says the list is out there.
    if filter_usable(app, cfg) {
        let lines = allow_list(&crate::library::mods_root(&cfg.mods_path), &rels);
        let journal = Journal {
            session_id: uuid::Uuid::new_v4().to_string(),
            server: address.to_string(),
            server_name: row.name.clone(),
            track: row.track.clone(),
            mods_path: cfg.mods_path.clone(),
            moved: rels.clone(),
            started_at: crate::ledger::now_ms(),
            filtered: true,
        };
        // Journal before the list, so the list is never out there without a record to clear it.
        let written = write_journal(&path, &journal).and_then(|()| write_filter(&filter_path(app), &lines));
        match written {
            Ok(()) => log::info!(
                "[race] {} ({} {}): FrostMod hides {} of {} mods ({} entries allowed)",
                row.name,
                needs.track,
                needs.track_layout,
                rels.len(),
                items.len(),
                lines.len()
            ),
            Err(e) => {
                log::warn!("[race] couldn't hand FrostMod the filter: {e}");
                clear_filter(&filter_path(app));
                clear_journal(&path);
            }
        }
        announce(app);
        return;
    }

    if let Err(why) = same_volume(&cfg.mods_path, &rels) {
        log::warn!("[race] not setting anything aside: {why}");
        return;
    }

    let journal = Journal {
        session_id: uuid::Uuid::new_v4().to_string(),
        server: address.to_string(),
        server_name: row.name.clone(),
        track: row.track.clone(),
        mods_path: cfg.mods_path.clone(),
        moved: Vec::new(),
        started_at: crate::ledger::now_ms(),
        filtered: false,
    };
    // With the folder watcher parked: these moves are ours, and a reload pulsed for them
    // would land on the game's load screen.
    match crate::with_watcher_parked(app, cfg, || park(&path, journal, &rels)) {
        Ok(j) => log::info!(
            "[race] {} ({} {}): set aside {} of {} mods",
            row.name,
            needs.track,
            needs.track_layout,
            j.moved.len(),
            items.len()
        ),
        Err(e) => log::warn!("[race] {e}"),
    }
    announce(app);
}

fn restore_locked(app: &tauri::AppHandle, cfg: &AppConfig, path: &Path, why: &str) {
    // A filtered session moved nothing: taking the list away is the whole restore.
    if read_journal(path).is_some_and(|j| j.filtered) {
        clear_filter(&filter_path(app));
        clear_journal(path);
        log::info!("[race] {why}: FrostMod's filter removed");
        announce(app);
        return;
    }
    let Some(out) = crate::with_watcher_parked(app, cfg, || unpark(path)) else {
        return;
    };
    if out.failed.is_empty() {
        log::info!("[race] {why}: put back {} mods", out.enabled);
    } else {
        log::warn!(
            "[race] {why}: put back {}, {} still set aside: {:?}",
            out.enabled,
            out.failed.len(),
            out.failed
        );
    }
    announce(app);
}

/// Put back what Race mode moved, unless the game is running — then it waits for the exit.
///
/// Safe to call from anywhere and as often as anyone likes: with no journal it's a no-op.
pub fn restore_if_idle(app: &tauri::AppHandle, why: &str) {
    let Some(path) = journal_path(app) else { return };
    if !path.exists() {
        return;
    }
    if crate::gameproc::is_game_running() {
        log::info!("[race] {why}, but the game is running; the mods come back when it exits");
        return;
    }
    let _held = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    restore_locked(app, &cfg, &path, why);
}

/// After a launch: if the game never turns up, don't leave the library narrowed.
pub fn watch_launch(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        while started.elapsed() < LAUNCH_GRACE {
            if crate::gameproc::is_game_running() {
                // The session watcher owns it from here: its exit is the restore.
                return;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        let _ = tauri::async_runtime::spawn_blocking(move || {
            restore_if_idle(&app, "the game never started")
        })
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(rel: &str, ids: &[&str]) -> Item {
        Item {
            rel: rel.into(),
            category: "track".into(),
            enabled: true,
            known: true,
            tracks: ids.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn bike(rel: &str, contents: &[(&str, &str)]) -> Item {
        Item {
            rel: rel.into(),
            category: "bike".into(),
            enabled: true,
            known: true,
            bikes: contents
                .iter()
                .map(|(id, class)| BikeContent { id: id.to_string(), class: class.to_string() })
                .collect(),
            ..Default::default()
        }
    }

    fn other(rel: &str, category: &str) -> Item {
        Item {
            rel: rel.into(),
            category: category.into(),
            enabled: true,
            known: true,
            ..Default::default()
        }
    }

    /// The list is the complement on disk: a hidden archive is left off, a folder holding one is
    /// walked into, and everything the inventory never judged — a stock bike's paints folder, a
    /// grouping folder with nothing hidden — is listed whole, so FrostMod doesn't hide it.
    #[test]
    fn the_allow_list_is_everything_not_hidden() {
        let root = std::env::temp_dir().join(format!("race-filter-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in ["tracks/EU", "bikes/MX1OEM_2023_Yamaha_YZ450F/paints"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["tracks/EU/Farm 14.pkz", "tracks/EU/Red Bud.pkz", "tracks/Loretta.pkz", "bikes/KTM 450.pkz", "bikes/Yamaha.pkz"] {
            fs::write(root.join(file), b"").unwrap();
        }
        let hidden = vec!["mods/tracks/EU/Red Bud.pkz".to_string(), "mods/Tracks/Loretta.pkz".to_string(), "mods/bikes/KTM 450.pkz".to_string()];
        assert_eq!(
            allow_list(&root, &hidden),
            vec![
                "tracks/EU/Farm 14.pkz".to_string(),
                "bikes/MX1OEM_2023_Yamaha_YZ450F".to_string(),
                "bikes/Yamaha.pkz".to_string(),
            ]
        );
        let _ = fs::remove_dir_all(root);
    }

    fn server(track: &str, categories: &[&str]) -> ServerNeeds {
        ServerNeeds {
            track: track.into(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn run(s: &ServerNeeds, items: &[Item]) -> Vec<String> {
        set_aside(s, items, &Player::default(), &[]).expect("race mode runs")
    }

    fn library() -> Vec<Item> {
        vec![
            track("mods/tracks/Farm 14.pkz", &["Farm 14", "Farm14"]),
            track("mods/tracks/EU/RedBud.pkz", &["RedBud", "redbud_2024"]),
            track("mods/tracks/Milestone", &["Milestone"]),
            bike("mods/bikes/KTM 450.pkz", &[("ktm450", "MX1 OEM")]),
            bike("mods/bikes/YZ250F.pkz", &[("yz250f", "MX2 OEM")]),
            bike("mods/bikes/CR500.pkz", &[("cr500", "Classic MX1 OEM")]),
        ]
    }

    #[test]
    fn with_no_server_track_nothing_moves() {
        let err = set_aside(&server("  ", &["MX1 OEM"]), &library(), &Player::default(), &[]);
        assert_eq!(err, Err(Skip::NoTrack));
    }

    /// The one mod that has to be there is the one we can't point at, so nothing moves —
    /// not even the bikes, which we *could* judge.
    #[test]
    fn a_track_this_install_does_not_have_stops_everything() {
        let err = set_aside(&server("Nowhere", &["MX1 OEM"]), &library(), &Player::default(), &[]);
        assert_eq!(err, Err(Skip::TrackNotInstalled("Nowhere".into())));
    }

    /// A parked copy of the server's track isn't one the game can load, so it doesn't count.
    #[test]
    fn a_parked_copy_of_the_track_does_not_count_as_installed() {
        let mut items = library();
        items[0].enabled = false;
        let err = set_aside(&server("Farm14", &[]), &items, &Player::default(), &[]);
        assert!(matches!(err, Err(Skip::TrackNotInstalled(_))), "{err:?}");
    }

    /// A stock track has no mod of its own, and every modded track can step aside for it.
    #[test]
    fn a_stock_track_sets_every_modded_track_aside() {
        let mut s = server("forest", &[]);
        s.track_is_stock = true;
        let out = run(&s, &library());
        assert!(out.contains(&"mods/tracks/Farm 14.pkz".to_string()));
        assert!(out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()));
        assert!(out.contains(&"mods/tracks/Milestone".to_string()));
    }

    #[test]
    fn keeps_the_server_track_and_sets_the_others_aside() {
        let out = run(&server("Farm14", &[]), &library());
        assert!(!out.contains(&"mods/tracks/Farm 14.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/tracks/Milestone".to_string()), "extracted tracks too");
    }

    /// The server reports the folder inside the archive, which needn't be the file name.
    #[test]
    fn the_server_track_is_found_by_its_inner_folder() {
        let out = run(&server("redbud_2024", &[]), &library());
        assert!(!out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/tracks/Farm 14.pkz".to_string()));
    }

    /// A layout lives inside its track's archive, so it never changes what stays.
    #[test]
    fn the_layout_does_not_change_what_stays() {
        let plain = run(&server("Farm14", &["MX1 OEM"]), &library());
        let mut s = server("Farm14", &["MX1 OEM"]);
        s.track_layout = "Short".into();
        assert_eq!(run(&s, &library()), plain);
    }

    #[test]
    fn keeps_bikes_in_the_server_classes_and_sets_the_rest_aside() {
        let out = run(&server("Farm14", &["mx1 oem"]), &library());
        assert!(!out.contains(&"mods/bikes/KTM 450.pkz".to_string()), "case-insensitive: {out:?}");
        assert!(out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
        assert!(out.contains(&"mods/bikes/CR500.pkz".to_string()));
    }

    /// The dedicated server's own shape: several classes in one `/`-separated entry.
    #[test]
    fn a_slash_separated_category_lets_each_class_in() {
        let out = run(&server("Farm14", &["MX1 OEM/MX2 OEM"]), &library());
        assert!(!out.contains(&"mods/bikes/KTM 450.pkz".to_string()));
        assert!(!out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
        assert!(out.contains(&"mods/bikes/CR500.pkz".to_string()));
    }

    /// Open class — no categories at all — lets every bike in, so every bike stays.
    #[test]
    fn an_open_server_keeps_every_bike() {
        let out = run(&server("Farm14", &[]), &library());
        assert!(out.iter().all(|r| !r.starts_with("mods/bikes/")), "{out:?}");
    }

    /// A bike with no `[data] cat` can't be judged, and couldn't-tell is kept.
    #[test]
    fn a_bike_with_no_category_is_kept() {
        let mut items = library();
        items.push(bike("mods/bikes/Mystery.pkz", &[("mystery", "  ")]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        assert!(!out.contains(&"mods/bikes/Mystery.pkz".to_string()), "{out:?}");
    }

    /// An archive whose identity couldn't be read — a pack, a protected zip — is kept, as is
    /// a track archive whose markers weren't found.
    #[test]
    fn an_unreadable_archive_is_kept() {
        let mut items = library();
        items.push(Item { known: false, ..bike("mods/bikes/OEM Bikes.pkz", &[]) });
        items.push(Item { known: false, ..track("mods/tracks/Odd.pkz", &["Odd"]) });
        items.push(bike("mods/bikes/NoIdentity.pkz", &[]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for kept in ["mods/bikes/OEM Bikes.pkz", "mods/tracks/Odd.pkz", "mods/bikes/NoIdentity.pkz"] {
            assert!(!out.contains(&kept.to_string()), "{kept}: {out:?}");
        }
    }

    /// The player's own bike stays even when the server's class wouldn't keep it: the join
    /// selects a compatible bike first, but a join the app couldn't steer mustn't leave the
    /// profile naming a bike that isn't there.
    #[test]
    fn the_players_selected_bike_is_kept() {
        let player = Player { bike_id: "CR500".into() };
        let out = set_aside(&server("Farm14", &["MX1 OEM"]), &library(), &player, &[]).unwrap();
        assert!(!out.contains(&"mods/bikes/CR500.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
    }

    /// The player's paints and gear, and anything paint sync installed, are pinned: a
    /// candidate that holds one is kept whole rather than taking it along.
    #[test]
    fn a_candidate_holding_a_pinned_file_is_kept() {
        let mut items = library();
        items.push(track("mods/tracks/Paintable", &["Paintable"]));
        let pinned = vec![
            "mods/tracks/Paintable/paints/mine.pnt".to_string(),
            // A different case and slash direction still names the same file.
            "MODS\\bikes\\YZ250F.pkz".to_string(),
        ];
        let out = set_aside(&server("Farm14", &["MX1 OEM"]), &items, &Player::default(), &pinned).unwrap();
        assert!(!out.contains(&"mods/tracks/Paintable".to_string()), "{out:?}");
        assert!(!out.contains(&"mods/bikes/YZ250F.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/CR500.pkz".to_string()));
    }

    /// Rider models, gear, paints, tyres and sounds are never candidates, whatever the server
    /// runs.
    #[test]
    fn rider_gear_paints_tyres_and_sounds_always_stay() {
        let mut items = library();
        let always = [
            ("mods/rider/riders/default_mx", "rider"),
            ("mods/rider/helmets/AGV", "helmet"),
            ("mods/rider/helmets/AGV/paints/Red.pnt", "helmetPaint"),
            ("mods/rider/helmets/AGV/goggles/Tint.pnt", "goggles"),
            ("mods/rider/boots/Alpinestars", "boots"),
            ("mods/rider/boots/Alpinestars/paints/White.pnt", "bootPaint"),
            ("mods/rider/gloves/Fox.pnt", "gloves"),
            ("mods/rider/riders/default_mx/paints/Kit.pnt", "outfit"),
            ("mods/rider/protections/Leatt", "protection"),
            ("mods/tyres/Dunlop.pkz", "tyre"),
            ("mods/bikes/KTM450/sounds", "sound"),
            ("mods/bikes/CR500/paints/Red.pnt", "bikePaint"),
            ("mods/bikes/CR500/FrostMod Models/2024.pkz", "bikeModelSwap"),
        ];
        for (rel, cat) in always {
            items.push(other(rel, cat));
        }
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for (rel, _) in always {
            assert!(!out.contains(&rel.to_string()), "{rel} moved: {out:?}");
        }
    }

    /// Shared packs other mods lean on stay, whichever content folder they sit in.
    #[test]
    fn support_packs_always_stay() {
        let mut items = library();
        items.push(track("mods/tracks/Common Objects.pkz", &["common objects"]));
        items.push(track("mods/tracks/misc/Banners.pkz", &["banners"]));
        items.push(bike("mods/bikes/Support Parts.pkz", &[("parts", "Parts")]));
        items.push(bike("mods/bikes/Shared Rims.pkz", &[("rims", "Parts")]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for kept in [
            "mods/tracks/Common Objects.pkz",
            "mods/tracks/misc/Banners.pkz",
            "mods/bikes/Support Parts.pkz",
            "mods/bikes/Shared Rims.pkz",
        ] {
            assert!(!out.contains(&kept.to_string()), "{kept}: {out:?}");
        }
    }

    /// Secured content — the blob, its key, or a folder holding either — never moves.
    #[test]
    fn secured_content_always_stays() {
        let mut items = library();
        items.push(track("mods/tracks/Locked.pkz.mxbsecure", &["Locked"]));
        items.push(bike("mods/bikes/Locked Bike.mxbsecure", &[("lb", "MX2 OEM")]));
        items.push(track("mods/tracks/Keyed.mxbkey", &["Keyed"]));
        items.push(Item { protected: true, ..track("mods/tracks/Inside", &["Inside"]) });
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        for kept in [
            "mods/tracks/Locked.pkz.mxbsecure",
            "mods/bikes/Locked Bike.mxbsecure",
            "mods/tracks/Keyed.mxbkey",
            "mods/tracks/Inside",
        ] {
            assert!(!out.contains(&kept.to_string()), "{kept}: {out:?}");
        }
    }

    /// A pack with one needed track or bike in it is kept whole.
    #[test]
    fn a_pack_with_anything_needed_is_kept_whole() {
        let mut items = library();
        items.push(track("mods/tracks/Pack.pkz", &["Pack", "Farm14", "Other"]));
        items.push(bike("mods/bikes/Mixed.pkz", &[("a", "MX2 OEM"), ("b", "MX1 OEM")]));
        items.push(bike("mods/bikes/AllMX2.pkz", &[("c", "MX2 OEM"), ("d", "MX2 OEM")]));
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        assert!(!out.contains(&"mods/tracks/Pack.pkz".to_string()), "{out:?}");
        assert!(!out.contains(&"mods/bikes/Mixed.pkz".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/AllMX2.pkz".to_string()), "nothing needed in it");
    }

    /// Whole tracks and whole *packed* bikes only. A bike installed as a folder carries its
    /// liveries and swap sets, which the app tracks by path elsewhere.
    #[test]
    fn only_whole_tracks_and_packed_bikes_are_candidates() {
        let mut items = library();
        items.push(Item { is_dir: true, ..bike("mods/bikes/RM250", &[("rm250", "MX2 OEM")]) });
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        assert!(!out.contains(&"mods/bikes/RM250".to_string()), "{out:?}");
        assert!(out.contains(&"mods/bikes/YZ250F.pkz".to_string()));
    }

    /// A mod the player parked themselves isn't Race mode's to list, or it would end up in
    /// the journal and come back at session end.
    #[test]
    fn a_mod_already_parked_is_not_listed() {
        let mut items = library();
        items[1].enabled = false;
        let out = run(&server("Farm14", &[]), &items);
        assert!(!out.contains(&"mods/tracks/EU/RedBud.pkz".to_string()), "{out:?}");
    }

    #[test]
    fn the_list_is_sorted_and_free_of_duplicates() {
        let mut items = library();
        items.push(items[1].clone());
        let out = run(&server("Farm14", &["MX1 OEM"]), &items);
        let mut sorted = out.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(out, sorted);
    }
}

#[cfg(test)]
mod journal_tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-racemode-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn journal_for(root: &Path) -> Journal {
        Journal {
            session_id: "test-session".into(),
            server: "203.0.113.10:54210".into(),
            server_name: "Test MX".into(),
            track: "Farm14".into(),
            mods_path: root.to_string_lossy().into_owned(),
            moved: Vec::new(),
            started_at: 1_700_000_000_000,
            filtered: false,
        }
    }

    #[test]
    fn the_journal_round_trips() {
        let root = tmp("journal");
        let path = root.join("data").join(JOURNAL_FILE);
        let mut j = journal_for(&root);
        j.moved = vec!["mods/tracks/RedBud.pkz".into(), "mods/bikes/CR500.pkz".into()];
        write_journal(&path, &j).unwrap();
        assert_eq!(read_journal(&path), Some(j));
        assert!(!path.with_extension("json.tmp").exists(), "no scratch file left behind");

        // The field names are the file format, and a later build has to read this one.
        let text = fs::read_to_string(&path).unwrap();
        for field in ["sessionId", "server", "moved", "startedAt"] {
            assert!(text.contains(&format!("\"{field}\"")), "{field} missing: {text}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// Parking and unparking through the journal, end to end — and only the journal's paths
    /// come back: a track the player parked by hand stays parked.
    #[test]
    fn restore_puts_back_only_what_race_mode_moved() {
        let root = tmp("only-ours");
        touch(&root.join("mods/tracks/RedBud.pkz"));
        touch(&root.join("mods/tracks/EU/Milestone.pkz"));
        touch(&root.join("mods/bikes/CR500.pkz"));
        // The player's own, parked before Race mode ever ran.
        touch(&root.join("mxbapp_disabled/tracks/Mine.pkz"));
        let path = root.join(JOURNAL_FILE);
        let rels: Vec<String> = vec![
            "mods/tracks/RedBud.pkz".into(),
            "mods/tracks/EU/Milestone.pkz".into(),
            "mods/bikes/CR500.pkz".into(),
        ];

        let j = park(&path, journal_for(&root), &rels).unwrap();
        assert_eq!(j.moved, rels);
        assert_eq!(read_journal(&path).unwrap().moved, rels, "journal holds what moved");
        assert!(!root.join("mods/tracks/RedBud.pkz").exists());
        assert!(root.join("mxbapp_disabled/tracks/EU/Milestone.pkz").is_file());

        let out = unpark(&path).expect("a journal to restore from");
        assert_eq!(out.enabled, 3, "{:?}", out.failed);
        assert!(root.join("mods/tracks/RedBud.pkz").is_file());
        assert!(root.join("mods/tracks/EU/Milestone.pkz").is_file());
        assert!(root.join("mods/bikes/CR500.pkz").is_file());
        assert!(root.join("mxbapp_disabled/tracks/Mine.pkz").is_file(), "the player's own stays parked");
        assert!(!root.join("mods/tracks/Mine.pkz").exists());
        assert!(!path.exists(), "a finished restore clears the journal");

        // And with no journal, restore is a no-op rather than a walk of the shadow tree.
        assert!(unpark(&path).is_none());
        assert!(root.join("mxbapp_disabled/tracks/Mine.pkz").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    /// The app died mid-session: the journal is on disk, the files are parked, and nothing
    /// else is known. The next start restores from the journal alone — including a path the
    /// journal names that never actually moved (the crash came between the journal and the
    /// move), which is a no-op rather than an error.
    #[test]
    fn crash_repair_restores_from_the_journal_alone() {
        let root = tmp("crash");
        touch(&root.join("mxbapp_disabled/tracks/RedBud.pkz"));
        touch(&root.join("mxbapp_disabled/bikes/CR500.pkz"));
        touch(&root.join("mods/tracks/NeverMoved.pkz"));
        touch(&root.join("mxbapp_disabled/tracks/Mine.pkz"));
        let path = root.join(JOURNAL_FILE);
        let mut j = journal_for(&root);
        j.moved = vec![
            "mods/tracks/RedBud.pkz".into(),
            "mods/bikes/CR500.pkz".into(),
            "mods/tracks/NeverMoved.pkz".into(),
        ];
        write_journal(&path, &j).unwrap();

        let out = unpark(&path).unwrap();
        assert_eq!(out.enabled, 2, "{:?}", out.failed);
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        assert!(root.join("mods/tracks/RedBud.pkz").is_file());
        assert!(root.join("mods/bikes/CR500.pkz").is_file());
        assert!(root.join("mods/tracks/NeverMoved.pkz").is_file(), "untouched");
        assert!(root.join("mxbapp_disabled/tracks/Mine.pkz").is_file());
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A path that won't go back stays in the journal for the next pass; the others are done.
    #[test]
    fn a_restore_that_cannot_finish_keeps_the_rest_recorded() {
        let root = tmp("stuck");
        touch(&root.join("mxbapp_disabled/tracks/RedBud.pkz"));
        touch(&root.join("mxbapp_disabled/tracks/Farm.pkz"));
        // Reinstalled during the session: putting the parked copy back would clobber it.
        touch(&root.join("mods/tracks/Farm.pkz"));
        let path = root.join(JOURNAL_FILE);
        let mut j = journal_for(&root);
        j.moved = vec!["mods/tracks/RedBud.pkz".into(), "mods/tracks/Farm.pkz".into()];
        write_journal(&path, &j).unwrap();

        let out = unpark(&path).unwrap();
        assert_eq!(out.enabled, 1);
        assert_eq!(out.failed.len(), 1);
        assert_eq!(read_journal(&path).unwrap().moved, vec!["mods/tracks/Farm.pkz".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    /// One move fails part-way: everything already moved goes back, and no journal is left
    /// claiming a Race mode that isn't happening.
    #[test]
    fn a_failed_move_rolls_back_what_already_moved() {
        let root = tmp("rollback");
        touch(&root.join("mods/tracks/A.pkz"));
        touch(&root.join("mods/tracks/B.pkz"));
        touch(&root.join("mods/tracks/C.pkz"));
        // Something is already parked under B's name, so B's move is refused.
        touch(&root.join("mxbapp_disabled/tracks/B.pkz"));
        let path = root.join(JOURNAL_FILE);
        let rels: Vec<String> =
            vec!["mods/tracks/A.pkz".into(), "mods/tracks/B.pkz".into(), "mods/tracks/C.pkz".into()];

        let err = park(&path, journal_for(&root), &rels).unwrap_err();
        assert!(err.contains("B.pkz"), "{err}");
        for name in ["A", "B", "C"] {
            assert!(root.join(format!("mods/tracks/{name}.pkz")).is_file(), "{name} is where it was");
        }
        assert!(!root.join("mxbapp_disabled/tracks/A.pkz").exists(), "A came back");
        assert!(root.join("mxbapp_disabled/tracks/B.pkz").is_file(), "the blocker is untouched");
        assert!(!path.exists(), "no journal for a Race mode that didn't happen");
        let _ = fs::remove_dir_all(&root);
    }

    /// Everything inside one folder is one volume; the check has to pass for the ordinary
    /// install, or Race mode would never run.
    #[test]
    fn one_folder_is_one_volume() {
        let root = tmp("volume");
        touch(&root.join("mods/tracks/A.pkz"));
        touch(&root.join("mods/bikes/B.pkz"));
        let mods_path = root.to_string_lossy().into_owned();
        let rels = vec!["mods/tracks/A.pkz".to_string(), "mods/bikes/B.pkz".to_string()];
        assert_eq!(same_volume(&mods_path, &rels), Ok(()));
        let _ = fs::remove_dir_all(&root);
    }

    /// `mods\tracks` junctioned onto another drive: refused, naming the folder, and nothing
    /// is checked as "probably fine" when the drive can't be read at all.
    #[test]
    fn a_content_folder_on_another_drive_is_refused() {
        let root = tmp("volume-other");
        touch(&root.join("mods/tracks/A.pkz"));
        touch(&root.join("mods/bikes/B.pkz"));
        let mods_path = root.to_string_lossy().into_owned();
        let rels = vec!["mods/bikes/B.pkz".to_string(), "mods/tracks/A.pkz".to_string()];

        let tracks_elsewhere = |p: &Path| {
            Some(if p.ends_with("tracks") { "d:" } else { "c:" }.to_string())
        };
        let err = same_volume_by(&mods_path, &rels, tracks_elsewhere).unwrap_err();
        assert!(err.contains("different drive"), "{err}");
        assert!(err.contains("tracks"), "names the folder: {err}");

        let unreadable = |_: &Path| None;
        assert!(same_volume_by(&mods_path, &rels, unreadable).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
