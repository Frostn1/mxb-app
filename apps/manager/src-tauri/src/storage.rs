//! The Storage page: where the space under the mods folder goes, and what can safely go.
//!
//! Three questions, all answered from the library scan the Library already runs
//! ([`crate::scan_library_blocking`]) so the two screens can never disagree about what is
//! installed:
//!
//! * **Leftover downloads** — archives in the user's Downloads folder, or in the app's own
//!   staging folders, whose contents are already in the mods folder. Matched by name and by
//!   content: a `.pkz` by hash against an installed file of the same size, a `.zip`/`.rar`/`.7z`
//!   by its file list (name and size of every entry) against the files on disk.
//! * **Duplicates** — the same mod installed twice under different names or folders, matched
//!   by a hash of its contents.
//! * **Big and unused** — every mod by size, with the last time it was ridden when MXB Coach
//!   has recorded it.
//!
//! Hashing is the only expensive step, so it is done only where sizes already collide and
//! is remembered across runs by path, size and modified time ([`HashCache`]).
//!
//! Nothing in here deletes a mod by itself. Removal goes through the Library's own uninstall
//! ([`crate::trashbin::uninstall_mod`]) and an archive goes to the Recycle Bin only when the
//! last leftover scan reported it — see [`check_archive_removal`].

use mxb_core::library::{self, LibraryEntry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Archive extensions a leftover download can have.
pub const ARCHIVE_EXTS: [&str; 4] = ["zip", "rar", "7z", "pkz"];

/// How much of an archive's bytes must already be on disk for its contents to count as
/// installed. Not 1.0: a pack nearly always carries a preview image or a readme the
/// installer never copies.
pub const CONTENT_MATCH_RATIO: f64 = 0.9;

/// A name shorter than this is never evidence on its own — `mx` sits inside half the
/// catalog. Same bar as the catalog matcher's `MIN_CONTAINED_LEN`.
const MIN_NAME_LEN: usize = 6;

/// A staging archive younger than this may belong to a download still in flight.
const CACHE_MIN_AGE_MS: u64 = 30 * 60 * 1000;

/// Where the size of the mods folder goes, by what the player would call it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Bucket {
    Tracks,
    Bikes,
    Paints,
    Gear,
    Other,
}

/// The bucket a library category belongs to.
pub fn bucket_of(category: &str) -> Bucket {
    match category {
        "track" => Bucket::Tracks,
        "bike" | "bikeModelSwap" | "sound" => Bucket::Bikes,
        c if c.ends_with("Paint") || c == "goggles" => Bucket::Paints,
        "helmet" | "boots" | "protection" | "animation" | "gloves" | "rider" => Bucket::Gear,
        _ => Bucket::Other,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// Everything under the mods folder, measured by walking it.
    pub total: u64,
    pub tracks: u64,
    pub bikes: u64,
    pub paints: u64,
    pub gear: u64,
    /// Everything else: tyres, misc, and files no Library tab lists.
    pub other: u64,
}

/// One mod, as the Storage page lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMod {
    pub name: String,
    pub path: String,
    /// The Library tab's `installSubpath` — what the uninstall path is scoped to.
    pub subpath: String,
    pub category: String,
    pub kind: String,
    /// Real size on disk; for a folder, every file under it that is not its own entry.
    pub size: u64,
    pub modified: u64,
    /// Protected content. Listed, never removable from here.
    pub secured: bool,
    /// Unix milliseconds of the last recorded ride, when a source has one.
    pub last_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageScan {
    pub overview: Overview,
    pub mods: Vec<StorageMod>,
    /// How many MXB Coach recordings "last used" was read from. Zero means there is no
    /// source and the page shows size only.
    pub coach_sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// Short content fingerprint, for a stable list key.
    pub id: String,
    /// Size of one copy.
    pub size: u64,
    pub items: Vec<StorageMod>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftover {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: u64,
    /// `downloads` or `cache`.
    pub location: String,
    /// `hash` (identical `.pkz`), `files` (file list on disk) or `name` (name only).
    pub matched_by: String,
    /// The installed mods it matched.
    pub matches: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveReport {
    pub removed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub bytes: u64,
}

// ── names ────────────────────────────────────────────────────────────────────────────────

/// Lowercase alphanumerics with any packaging extension dropped: `Red_Bud-2024.zip` →
/// `redbud2024`.
pub fn norm_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut stem = lower.as_str();
    for ext in [".mxbsecure", ".pkz", ".zip", ".rar", ".7z", ".pnt"] {
        if let Some(s) = stem.strip_suffix(ext) {
            stem = s;
            break;
        }
    }
    stem.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// Whether an archive name and an installed mod name plausibly name the same thing: equal
/// once normalised, or one wholly inside the other when that one is long enough to mean
/// something (`RedBud_2024_v2.zip` ↔ `RedBud_2024.pkz`).
pub fn names_match(archive: &str, installed: &str) -> bool {
    let a = norm_name(archive);
    let b = norm_name(installed);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    short.len() >= MIN_NAME_LEN && long.contains(short.as_str())
}

// ── hashing ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHash {
    size: u64,
    modified: u64,
    hash: String,
}

/// File hashes remembered by path, size and modified time, so a second scan reads nothing it
/// has read before.
#[derive(Debug, Default)]
pub struct HashCache {
    file: Option<PathBuf>,
    map: HashMap<String, CachedHash>,
    dirty: bool,
}

impl HashCache {
    pub fn load(file: Option<PathBuf>) -> Self {
        let map = file
            .as_ref()
            .and_then(|f| fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { file, map, dirty: false }
    }

    /// SHA-256 of a file, from the cache when its size and time still agree.
    pub fn hash(&mut self, path: &Path) -> Option<String> {
        let meta = fs::metadata(path).ok()?;
        let size = meta.len();
        let modified = library::mtime_ms(&meta);
        let key = path.to_string_lossy().into_owned();
        if let Some(c) = self.map.get(&key) {
            if c.size == size && c.modified == modified {
                return Some(c.hash.clone());
            }
        }
        let hash = sha256_file(path)?;
        self.map.insert(key, CachedHash { size, modified, hash: hash.clone() });
        self.dirty = true;
        Some(hash)
    }

    /// Write it back, dropping rows for files that are gone. Best-effort.
    pub fn save(&mut self) {
        let Some(file) = self.file.clone() else { return };
        let before = self.map.len();
        self.map.retain(|k, _| Path::new(k).exists());
        if !self.dirty && self.map.len() == before {
            return;
        }
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(&self.map) {
            let _ = fs::write(&file, json);
        }
        self.dirty = false;
    }
}

pub fn sha256_file(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── what a mod is made of ────────────────────────────────────────────────────────────────

/// One file belonging to a mod: its path relative to the mod (empty for a single-file mod),
/// lowercased, and its size.
#[derive(Debug, Clone)]
pub struct ModFile {
    pub rel: String,
    pub path: PathBuf,
    pub size: u64,
}

/// The files that make up an entry. A folder's own nested entries (a bike's liveries, its
/// model swaps) are theirs, not its, so `others` is skipped — that is what stops a bike and
/// its liveries counting the same bytes twice.
pub fn mod_files(entry: &Path, others: &HashSet<PathBuf>) -> Vec<ModFile> {
    if entry.is_file() {
        let size = fs::metadata(entry).map(|m| m.len()).unwrap_or(0);
        return vec![ModFile { rel: String::new(), path: entry.to_path_buf(), size }];
    }
    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(entry).follow_links(false).into_iter();
    for e in walker.filter_entry(|e| e.depth() == 0 || !others.contains(e.path())).flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        let rel = e
            .path()
            .strip_prefix(entry)
            .map(|r| r.to_string_lossy().replace('\\', "/").to_lowercase())
            .unwrap_or_default();
        out.push(ModFile { rel, path: e.path().to_path_buf(), size });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// Whether a mod is, or holds, protected content. Such a mod is never offered for removal.
pub fn touches_secure(files: &[ModFile]) -> bool {
    files.iter().any(|f| {
        let n = f.path.to_string_lossy().to_lowercase();
        n.ends_with(".mxbsecure") || n.ends_with(".mxbsecurekey")
    })
}

/// A copy the duplicate finder must leave alone: a livery parked on FrostMod's shelf, or a
/// model swap's own files. Those are deliberate second copies the model swapper manages.
fn managed_copy(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s.eq_ignore_ascii_case(library::LIB_DIR) || s.eq_ignore_ascii_case(library::PAINT_SHELF)
    })
}

// ── duplicates ───────────────────────────────────────────────────────────────────────────

/// Group mods whose contents are byte-identical.
///
/// Sizes first: only mods with the same total size, file count and shape can be the same,
/// and only those are hashed. A folder's fingerprint is every file's path inside it and its
/// hash, so two folders match whatever they are called, and a single file matches another
/// whatever its name.
pub fn find_duplicates(
    mods: &[StorageMod],
    files: &HashMap<String, Vec<ModFile>>,
    cache: &mut HashCache,
) -> Vec<DuplicateGroup> {
    let mut by_shape: HashMap<(bool, u64, usize), Vec<&StorageMod>> = HashMap::new();
    for m in mods {
        let Some(fs_) = files.get(&m.path) else { continue };
        if m.secured
            || m.size == 0
            || m.category == "bikeModelSwap"
            || managed_copy(Path::new(&m.path))
            || touches_secure(fs_)
        {
            continue;
        }
        by_shape
            .entry((m.kind == "folder", m.size, fs_.len()))
            .or_default()
            .push(m);
    }

    let mut groups: HashMap<String, Vec<StorageMod>> = HashMap::new();
    for (_, same) in by_shape.into_iter().filter(|(_, v)| v.len() > 1) {
        for m in same {
            let Some(print) = fingerprint(&files[&m.path], cache) else { continue };
            groups.entry(print).or_default().push(m.clone());
        }
    }

    let mut out: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(id, mut items)| {
            items.sort_by(|a, b| keep_order(a).cmp(&keep_order(b)));
            DuplicateGroup { id: id[..16].to_string(), size: items[0].size, items }
        })
        .collect();
    out.sort_by(|a, b| {
        let wa = a.size * (a.items.len() as u64 - 1);
        let wb = b.size * (b.items.len() as u64 - 1);
        wb.cmp(&wa).then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// Which copy to suggest keeping first: the shallowest, then the oldest, then by path. The
/// page lets the player pick another.
fn keep_order(m: &StorageMod) -> (usize, u64, String) {
    let depth = Path::new(&m.path).components().count();
    (depth, m.modified, m.path.to_lowercase())
}

fn fingerprint(files: &[ModFile], cache: &mut HashCache) -> Option<String> {
    let mut h = Sha256::new();
    for f in files {
        let fh = cache.hash(&f.path)?;
        h.update(f.rel.as_bytes());
        h.update([0]);
        h.update(fh.as_bytes());
        h.update([0]);
    }
    Some(hex(&h.finalize()))
}

// ── leftover archives ────────────────────────────────────────────────────────────────────

/// What is on disk, indexed for matching an archive against it.
#[derive(Default)]
pub struct Installed {
    /// (file name lowercased, size) → the mod it belongs to.
    by_name_size: HashMap<(String, u64), String>,
    /// size → installed single-file mods of that size, for a hash comparison.
    by_size: HashMap<u64, Vec<(PathBuf, String)>>,
    /// Every mod's display name.
    names: Vec<String>,
}

impl Installed {
    pub fn build(mods: &[StorageMod], files: &HashMap<String, Vec<ModFile>>) -> Self {
        let mut out = Self::default();
        for m in mods {
            out.names.push(m.name.clone());
            let Some(fs_) = files.get(&m.path) else { continue };
            for f in fs_ {
                let base = f
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                out.by_name_size.entry((base, f.size)).or_insert_with(|| m.name.clone());
            }
            if m.kind != "folder" {
                out.by_size
                    .entry(m.size)
                    .or_default()
                    .push((PathBuf::from(&m.path), m.name.clone()));
            }
        }
        out
    }
}

/// An archive's entries as (file name lowercased, uncompressed size), directories left out.
/// `None` when it can't be read — encrypted, damaged, or not an archive at all.
pub fn list_archive(path: &Path) -> Option<Vec<(String, u64)>> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let base = |name: &str| {
        name.rsplit(['/', '\\']).next().unwrap_or(name).to_lowercase()
    };
    match ext.as_str() {
        "zip" => {
            let mut z = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
            let mut out = Vec::new();
            for i in 0..z.len() {
                let e = z.by_index_raw(i).ok()?;
                if !e.is_dir() {
                    out.push((base(e.name()), e.size()));
                }
            }
            Some(out)
        }
        "7z" => {
            let f = fs::File::open(path).ok()?;
            let len = f.metadata().ok()?.len();
            let r = sevenz_rust::SevenZReader::new(f, len, sevenz_rust::Password::empty()).ok()?;
            Some(
                r.archive()
                    .files
                    .iter()
                    .filter(|e| !e.is_directory())
                    .map(|e| (base(e.name()), e.size()))
                    .collect(),
            )
        }
        "rar" => {
            let listing = unrar::Archive::new(path).open_for_listing().ok()?;
            let mut out = Vec::new();
            for h in listing {
                let h = h.ok()?;
                if !h.is_directory() {
                    out.push((base(&h.filename.to_string_lossy()), h.unpacked_size));
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// How much of an archive's listing is already on disk: the mods it matched, and whether the
/// matched share of its bytes reaches [`CONTENT_MATCH_RATIO`]. Readmes don't count either way.
pub fn content_match(listing: &[(String, u64)], installed: &Installed) -> Option<Vec<String>> {
    let mut total = 0u64;
    let mut matched = 0u64;
    let mut owners: Vec<String> = Vec::new();
    for (name, size) in listing {
        if *size == 0 || mxb_core::names::is_junk(name) {
            continue;
        }
        total += size;
        if let Some(owner) = installed.by_name_size.get(&(name.clone(), *size)) {
            matched += size;
            if !owners.contains(owner) {
                owners.push(owner.clone());
            }
        }
    }
    (total > 0 && matched as f64 >= total as f64 * CONTENT_MATCH_RATIO).then_some(owners)
}

/// Decide whether one archive is a leftover, and why. `None` when nothing installed matches.
pub fn match_archive(path: &Path, installed: &Installed, cache: &mut HashCache) -> Option<(String, Vec<String>)> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let size = fs::metadata(path).ok()?.len();
    let is_pkz = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz"));

    if is_pkz {
        if let Some(cands) = installed.by_size.get(&size) {
            let mine = cache.hash(path);
            for (p, owner) in cands {
                if mine.is_some() && cache.hash(p) == mine {
                    return Some(("hash".into(), vec![owner.clone()]));
                }
            }
        }
    } else if let Some(owners) = list_archive(path).and_then(|l| content_match(&l, installed)) {
        return Some(("files".into(), owners));
    }

    let by_name: Vec<String> = installed
        .names
        .iter()
        .filter(|n| names_match(&name, n))
        .take(5)
        .cloned()
        .collect();
    (!by_name.is_empty()).then(|| ("name".into(), by_name))
}

pub fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ARCHIVE_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Archives under `dir`, at most `depth` folders down, never following links.
pub fn archives_in(dir: &Path, depth: usize) -> Vec<PathBuf> {
    walkdir::WalkDir::new(dir)
        .max_depth(depth + 1)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && is_archive(e.path()))
        .map(|e| e.into_path())
        .collect()
}

/// The app's own folders a download can be left behind in: the staging folder beside the
/// mods folder, and staging folders in the system temp dir.
pub fn cache_dirs(mods_path: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let near = Path::new(mods_path.trim()).join(".frost-staging");
    if !mods_path.trim().is_empty() && near.is_dir() {
        out.push(near);
    }
    if let Ok(rd) = fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if (n.starts_with("frost-dl-") || n.starts_with("frost-import-")) && e.path().is_dir() {
                out.push(e.path());
            }
        }
    }
    out
}

/// Whether a cache archive is old enough that no download can still be writing it.
pub fn settled(path: &Path, now_ms: u64) -> bool {
    fs::metadata(path)
        .map(|m| now_ms.saturating_sub(library::mtime_ms(&m)) >= CACHE_MIN_AGE_MS)
        .unwrap_or(false)
}

/// The one gate an archive passes before it goes to the Recycle Bin. It must be an archive,
/// not protected content, not inside a profile or the mods folder (an installed `.pkz` is a
/// mod, and mods only leave through the uninstall path), and one the last scan reported.
pub fn check_archive_removal(
    path: &Path,
    reported: &HashSet<PathBuf>,
    profiles_dir: &Path,
    mods_root: &Path,
) -> Result<(), String> {
    let lower = path.to_string_lossy().to_lowercase();
    if lower.ends_with(".mxbsecure") || lower.ends_with(".mxbsecurekey") {
        return Err("protected content is never removed".into());
    }
    if !is_archive(path) {
        return Err("not an archive".into());
    }
    if !profiles_dir.as_os_str().is_empty() && path.starts_with(profiles_dir) {
        return Err("profiles are never touched".into());
    }
    if !mods_root.as_os_str().is_empty() && path.starts_with(mods_root) {
        return Err("installed mods are removed through uninstall".into());
    }
    if !reported.contains(path) {
        return Err("not in the last scan; scan again first".into());
    }
    if !path.is_file() {
        return Err("no longer there".into());
    }
    Ok(())
}

// ── last used ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CoachIndexed {
    #[serde(default)]
    modified: u64,
    summary: CoachSummary,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoachSummary {
    #[serde(default)]
    track_id: String,
    #[serde(default)]
    bike_id: String,
}

/// The last time each track and bike was ridden, from MXB Coach's session index.
///
/// The index is Coach's own cache of every recording it has read: keyed by recording path,
/// each row carries the recording's modified time (seconds) and the event's track and bike
/// ids, which are the names of their folders or archives in the mods tree. Returns the map
/// (lowercased id → unix ms) and how many sessions it came from.
pub fn coach_last_used(index_json: &[u8]) -> (HashMap<String, u64>, usize) {
    let rows: HashMap<String, CoachIndexed> = serde_json::from_slice(index_json).unwrap_or_default();
    let mut out: HashMap<String, u64> = HashMap::new();
    for row in rows.values() {
        let ms = row.modified.saturating_mul(1000);
        for id in [&row.summary.track_id, &row.summary.bike_id] {
            let id = id.trim().to_lowercase();
            if id.is_empty() {
                continue;
            }
            let slot = out.entry(id).or_insert(0);
            *slot = (*slot).max(ms);
        }
    }
    (out, rows.len())
}

/// The last-used time for one mod, matched by its file or folder name.
pub fn last_used_for(m: &StorageMod, used: &HashMap<String, u64>) -> Option<u64> {
    if !matches!(m.category.as_str(), "track" | "bike") {
        return None;
    }
    used.get(&library::strip_ext(&m.name).to_lowercase()).copied()
}

// ── assembling a scan ────────────────────────────────────────────────────────────────────

/// Turn the library's entries into the Storage page's mods, measuring each one, and remember
/// what each is made of for the duplicate and leftover passes.
pub fn measure(
    entries: Vec<(String, LibraryEntry)>,
) -> (Vec<StorageMod>, HashMap<String, Vec<ModFile>>) {
    let paths: HashSet<PathBuf> = entries.iter().map(|(_, e)| PathBuf::from(&e.path)).collect();
    let mut mods = Vec::new();
    let mut files = HashMap::new();
    for (subpath, e) in entries {
        if e.stock {
            continue;
        }
        let p = PathBuf::from(&e.path);
        let mine = mod_files(&p, &paths);
        let size = mine.iter().map(|f| f.size).sum();
        mods.push(StorageMod {
            name: e.name,
            path: e.path.clone(),
            subpath,
            category: e.category,
            kind: e.kind,
            size,
            modified: e.modified,
            secured: e.secured || touches_secure(&mine),
            last_used: None,
        });
        files.insert(e.path, mine);
    }
    mods.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    (mods, files)
}

/// Total bytes under the mods folder, leaving out the app's staging folder.
pub fn tree_size(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".frost-staging")
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn overview(mods: &[StorageMod], total: u64) -> Overview {
    let mut o = Overview::default();
    for m in mods {
        let slot = match bucket_of(&m.category) {
            Bucket::Tracks => &mut o.tracks,
            Bucket::Bikes => &mut o.bikes,
            Bucket::Paints => &mut o.paints,
            Bucket::Gear => &mut o.gear,
            Bucket::Other => &mut o.other,
        };
        *slot += m.size;
    }
    let listed = o.tracks + o.bikes + o.paints + o.gear + o.other;
    o.total = total.max(listed);
    o.other += o.total - listed;
    o
}

// ── commands ─────────────────────────────────────────────────────────────────────────────

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

/// How long one measurement serves the next tab. The three passes all start from it, and the
/// page opens them one after another; a removal drops it outright.
const MEASURE_TTL: Duration = Duration::from_secs(30);

type Measured = (Vec<StorageMod>, HashMap<String, Vec<ModFile>>);

struct LastMeasure {
    at: Instant,
    mods_path: String,
    data: std::sync::Arc<Measured>,
}

static LAST: Mutex<Option<LastMeasure>> = Mutex::new(None);
/// Every archive the last leftover scan reported. Nothing else can be sent to the bin.
static REPORTED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

fn forget_measure() {
    if let Ok(mut g) = LAST.lock() {
        *g = None;
    }
}

fn hash_cache(app: &tauri::AppHandle) -> HashCache {
    HashCache::load(app.path().app_local_data_dir().ok().map(|d| d.join("storage-hashes.json")))
}

/// Every installed mod, measured. One library scan per mods folder — the same scan the
/// Library runs — then a walk of each folder mod for its real size.
fn measured(app: &tauri::AppHandle, fresh: bool) -> Result<(crate::config::AppConfig, std::sync::Arc<Measured>), String> {
    let cfg = crate::config::load(app).map_err(|e| format!("{e:#}"))?;
    if cfg.mods_path.trim().is_empty() {
        return Err("the mods folder hasn't been set yet".into());
    }
    if !fresh {
        if let Ok(g) = LAST.lock() {
            if let Some(l) = g.as_ref() {
                if l.mods_path == cfg.mods_path && l.at.elapsed() < MEASURE_TTL {
                    return Ok((cfg, l.data.clone()));
                }
            }
        }
    }
    let mut entries = Vec::new();
    for dir in cfg.game().mods_dirs {
        let subpath = format!("mods/{dir}");
        for e in crate::scan_library_blocking(app.clone(), subpath.clone()).unwrap_or_default() {
            entries.push((subpath.clone(), e));
        }
    }
    let data = std::sync::Arc::new(measure(entries));
    if let Ok(mut g) = LAST.lock() {
        *g = Some(LastMeasure { at: Instant::now(), mods_path: cfg.mods_path.clone(), data: data.clone() });
    }
    Ok((cfg, data))
}

fn coach_index(app: &tauri::AppHandle) -> Option<Vec<u8>> {
    // MXB Coach shares this data folder; its session index is `coach/index-v4.json`.
    fs::read(crate::config::data_dir(app)?.join("coach").join("index-v4.json")).ok()
}

fn scan_blocking(app: tauri::AppHandle) -> Result<StorageScan, String> {
    let (cfg, data) = measured(&app, true)?;
    let (used, sessions) = coach_index(&app).map(|b| coach_last_used(&b)).unwrap_or_default();
    let mut mods = data.0.clone();
    for m in &mut mods {
        m.last_used = last_used_for(m, &used);
    }
    let total = tree_size(&library::mods_root(&cfg.mods_path));
    Ok(StorageScan { overview: overview(&mods, total), mods, coach_sessions: sessions })
}

#[tauri::command]
pub async fn storage_scan(app: tauri::AppHandle) -> Result<StorageScan, String> {
    tauri::async_runtime::spawn_blocking(move || scan_blocking(app))
        .await
        .map_err(|e| format!("storage_scan task failed: {e}"))?
}

#[tauri::command]
pub async fn storage_duplicates(app: tauri::AppHandle) -> Result<Vec<DuplicateGroup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, data) = measured(&app, false)?;
        let mut cache = hash_cache(&app);
        let out = find_duplicates(&data.0, &data.1, &mut cache);
        cache.save();
        Ok(out)
    })
    .await
    .map_err(|e| format!("storage_duplicates task failed: {e}"))?
}

#[tauri::command]
pub async fn storage_leftovers(app: tauri::AppHandle) -> Result<Vec<Leftover>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (cfg, data) = measured(&app, false)?;
        let installed = Installed::build(&data.0, &data.1);
        let mut cache = hash_cache(&app);
        let now = crate::ledger::now_ms();
        let mut found: Vec<(PathBuf, &str)> = Vec::new();
        if let Some(dl) = dirs_next::download_dir() {
            found.extend(archives_in(&dl, 1).into_iter().map(|p| (p, "downloads")));
        }
        for dir in cache_dirs(&cfg.mods_path) {
            found.extend(
                archives_in(&dir, 3)
                    .into_iter()
                    .filter(|p| settled(p, now))
                    .map(|p| (p, "cache")),
            );
        }
        let profiles = cfg.profiles_dir();
        let mods_root = library::mods_root(&cfg.mods_path);
        let mut out = Vec::new();
        for (path, location) in found {
            // A Downloads folder that holds the mods folder must not offer the mods themselves.
            if path.starts_with(&profiles) || path.starts_with(&mods_root) {
                continue;
            }
            let Some((matched_by, matches)) = match_archive(&path, &installed, &mut cache) else {
                continue;
            };
            let meta = fs::metadata(&path).ok();
            out.push(Leftover {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: path.to_string_lossy().into_owned(),
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta.as_ref().map(library::mtime_ms).unwrap_or(0),
                location: location.into(),
                matched_by,
                matches,
            });
        }
        cache.save();
        out.sort_by(|a, b| b.size.cmp(&a.size));
        if let Ok(mut r) = REPORTED.lock() {
            *r = Some(out.iter().map(|l| PathBuf::from(&l.path)).collect());
        }
        Ok(out)
    })
    .await
    .map_err(|e| format!("storage_leftovers task failed: {e}"))?
}

/// Send confirmed leftover archives to the Recycle Bin. Only ones the last scan reported.
#[tauri::command]
pub async fn storage_trash_archives(app: tauri::AppHandle, paths: Vec<String>) -> Result<RemoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::config::load(&app).map_err(|e| format!("{e:#}"))?;
        let reported = REPORTED.lock().ok().and_then(|r| r.clone()).unwrap_or_default();
        let profiles = cfg.profiles_dir();
        let mods_root = library::mods_root(&cfg.mods_path);
        let mut report = RemoveReport::default();
        for p in paths {
            let path = PathBuf::from(&p);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let res = check_archive_removal(&path, &reported, &profiles, &mods_root)
                .and_then(|_| crate::trashbin::move_to_trash(&path).map(|_| ()).map_err(|e| format!("{e:#}")));
            match res {
                Ok(()) => {
                    report.bytes += size;
                    report.removed.push(p);
                }
                Err(e) => report.failed.push((p, e)),
            }
        }
        if let Ok(mut r) = REPORTED.lock() {
            if let Some(set) = r.as_mut() {
                for p in &report.removed {
                    set.remove(Path::new(p));
                }
            }
        }
        Ok(report)
    })
    .await
    .map_err(|e| format!("storage_trash_archives task failed: {e}"))?
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMod {
    pub path: String,
    pub subpath: String,
}

/// Uninstall confirmed mods through the Library's own path: Recycle Bin, scoped to the tab's
/// folder, and noted in the ledger so the Library's history can offer Restore.
#[tauri::command]
pub async fn storage_remove_mods(app: tauri::AppHandle, items: Vec<RemoveMod>) -> Result<RemoveReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (cfg, data) = measured(&app, true)?;
        let known: HashMap<&str, &StorageMod> = data.0.iter().map(|m| (m.path.as_str(), m)).collect();
        let profiles = cfg.profiles_dir();
        let mut report = RemoveReport::default();
        for item in items {
            let res = match known.get(item.path.as_str()) {
                None => Err("not an installed mod; scan again first".to_string()),
                Some(m) if m.secured => Err("protected content is never removed".into()),
                Some(m) if m.subpath != item.subpath => Err("wrong library folder".into()),
                Some(_) if Path::new(&item.path).starts_with(&profiles) => Err("profiles are never touched".into()),
                Some(m) => crate::trashbin::uninstall_mod(&cfg.mods_path, &item.path, &item.subpath)
                    .map(|landed| {
                        crate::ledger_note_trashed(&app, &cfg, &item.path, landed);
                        m.size
                    })
                    .map_err(|e| format!("{e:#}")),
            };
            match res {
                Ok(size) => {
                    report.bytes += size;
                    report.removed.push(item.path);
                }
                Err(e) => report.failed.push((item.path, e)),
            }
        }
        forget_measure();
        Ok(report)
    })
    .await
    .map_err(|e| format!("storage_remove_mods task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-storage-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(p: &Path, bytes: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn entry(path: &Path, category: &str, kind: &str) -> (String, LibraryEntry) {
        (
            "mods/tracks".into(),
            LibraryEntry {
                name: path.file_name().unwrap().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                folder: String::new(),
                size: 0,
                modified: 0,
                kind: kind.into(),
                category: category.into(),
                parent: None,
                secured: false,
                locked: false,
                prefix: None,
                stock: false,
            },
        )
    }

    #[test]
    fn names_match_through_spelling_and_versions() {
        assert!(names_match("Red_Bud-2024.zip", "RedBud2024.pkz"));
        assert!(names_match("RedBud_2024_v2.zip", "RedBud_2024.pkz"));
        assert!(names_match("KTM450.rar", "ktm450"));
        // Too short to be evidence inside a longer name.
        assert!(!names_match("mx.zip", "mxbikes_pack.pkz"));
        assert!(!names_match("Washougal.zip", "RedBud.pkz"));
        assert!(!names_match(".zip", "a.pkz"));
    }

    #[test]
    fn buckets_follow_library_categories() {
        assert_eq!(bucket_of("track"), Bucket::Tracks);
        assert_eq!(bucket_of("bike"), Bucket::Bikes);
        assert_eq!(bucket_of("bikePaint"), Bucket::Paints);
        assert_eq!(bucket_of("helmetPaint"), Bucket::Paints);
        assert_eq!(bucket_of("helmet"), Bucket::Gear);
        assert_eq!(bucket_of("misc"), Bucket::Other);
    }

    #[test]
    fn identical_mods_under_different_names_are_duplicates() {
        let root = tmp("dup");
        let t = root.join("mods/tracks");
        write(&t.join("RedBud.pkz"), b"same track bytes");
        write(&t.join("EU/RedBud (copy).pkz"), b"same track bytes");
        write(&t.join("Other.pkz"), b"diff track bytes"); // same size, different bytes
        write(&t.join("Folder A/a.map"), b"map");
        write(&t.join("Folder A/sub/b.tsc"), b"tsc!");
        write(&t.join("Folder B/a.map"), b"map");
        write(&t.join("Folder B/sub/b.tsc"), b"tsc!");
        // A livery on FrostMod's shelf is a managed second copy, never a duplicate.
        write(&root.join("mods/bikes/KTM/_paints/x.pnt"), b"same track bytes");

        let entries = vec![
            entry(&t.join("RedBud.pkz"), "track", "pkz"),
            entry(&t.join("EU/RedBud (copy).pkz"), "track", "pkz"),
            entry(&t.join("Other.pkz"), "track", "pkz"),
            entry(&t.join("Folder A"), "track", "folder"),
            entry(&t.join("Folder B"), "track", "folder"),
            entry(&root.join("mods/bikes/KTM/_paints/x.pnt"), "bikePaint", "loose"),
        ];
        let (mods, files) = measure(entries);
        let mut cache = HashCache::default();
        let groups = find_duplicates(&mods, &files, &mut cache);
        assert_eq!(groups.len(), 2, "{groups:#?}");
        let names: Vec<Vec<&str>> = groups
            .iter()
            .map(|g| g.items.iter().map(|m| m.name.as_str()).collect())
            .collect();
        assert!(names.contains(&vec!["RedBud.pkz", "RedBud (copy).pkz"]), "shallowest first: {names:?}");
        assert!(names.contains(&vec!["Folder A", "Folder B"]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn protected_mods_are_never_duplicates() {
        let root = tmp("dup-secure");
        let t = root.join("mods/tracks");
        write(&t.join("A/track.mxbsecurekey"), b"k");
        write(&t.join("B/track.mxbsecurekey"), b"k");
        let (mods, files) = measure(vec![
            entry(&t.join("A"), "track", "folder"),
            entry(&t.join("B"), "track", "folder"),
        ]);
        assert!(mods.iter().all(|m| m.secured));
        assert!(find_duplicates(&mods, &files, &mut HashCache::default()).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_does_not_count_its_own_liveries() {
        let root = tmp("measure");
        let b = root.join("mods/bikes/KTM");
        write(&b.join("KTM.edf"), &[0; 10]);
        write(&b.join("paints/red.pnt"), &[0; 5]);
        let (mods, _) = measure(vec![
            entry(&b, "bike", "folder"),
            entry(&b.join("paints/red.pnt"), "bikePaint", "loose"),
        ]);
        let size = |n: &str| mods.iter().find(|m| m.name == n).unwrap().size;
        assert_eq!(size("KTM"), 10);
        assert_eq!(size("red.pnt"), 5);
        let o = overview(&mods, 20);
        assert_eq!((o.bikes, o.paints, o.other, o.total), (10, 5, 5, 20));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn archives_match_by_file_list_hash_and_name() {
        let root = tmp("leftover");
        let t = root.join("mods/tracks");
        write(&t.join("Washougal/w.map"), &[1; 4000]);
        write(&t.join("Washougal/w.tsc"), &[2; 3000]);
        write(&t.join("RedBud.pkz"), &[3; 500]);
        let (mods, files) = measure(vec![
            entry(&t.join("Washougal"), "track", "folder"),
            entry(&t.join("RedBud.pkz"), "track", "pkz"),
        ]);
        let installed = Installed::build(&mods, &files);
        let mut cache = HashCache::default();
        let dl = root.join("Downloads");

        // A zip whose files are on disk, plus a readme and a small preview: content match.
        let zip_path = dl.join("wash_pack_final.zip");
        fs::create_dir_all(&dl).unwrap();
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("Washougal/w.map", o).unwrap();
            z.write_all(&[1; 4000]).unwrap();
            z.start_file("Washougal/w.tsc", o).unwrap();
            z.write_all(&[2; 3000]).unwrap();
            z.start_file("readme.txt", o).unwrap();
            z.write_all(b"thanks").unwrap();
            z.start_file("preview.jpg", o).unwrap();
            z.write_all(&[9; 100]).unwrap();
            z.finish().unwrap();
        }
        let (by, owners) = match_archive(&zip_path, &installed, &mut cache).unwrap();
        assert_eq!((by.as_str(), owners), ("files", vec!["Washougal".to_string()]));

        // An identical pkz under another name: hash match.
        write(&dl.join("redbud_download.pkz"), &[3; 500]);
        let (by, owners) = match_archive(&dl.join("redbud_download.pkz"), &installed, &mut cache).unwrap();
        assert_eq!((by.as_str(), owners), ("hash", vec!["RedBud.pkz".to_string()]));

        // Same size, different bytes, unrelated name: not a leftover.
        write(&dl.join("something.pkz"), &[4; 500]);
        assert!(match_archive(&dl.join("something.pkz"), &installed, &mut cache).is_none());

        // An unreadable archive named like an installed mod: name only.
        write(&dl.join("RedBud_v2.zip"), b"not a zip");
        let (by, _) = match_archive(&dl.join("RedBud_v2.zip"), &installed, &mut cache).unwrap();
        assert_eq!(by, "name");

        // A zip with only a fraction of its bytes installed is not a content match.
        let partial = vec![("w.map".to_string(), 4000), ("new.trh".to_string(), 9000)];
        assert!(content_match(&partial, &installed).is_none());

        assert_eq!(archives_in(&dl, 0).len(), 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn removal_gate_refuses_anything_unreported_or_protected() {
        let root = tmp("gate");
        let a = root.join("Downloads/a.zip");
        write(&a, b"x");
        let profiles = root.join("profiles");
        let p = profiles.join("me/bak.zip");
        write(&p, b"x");
        let s = root.join("Downloads/t.mxbsecure");
        write(&s, b"x");
        let mods = root.join("mods");
        let installed = mods.join("tracks/RedBud.pkz");
        write(&installed, b"x");
        let reported: HashSet<PathBuf> =
            [a.clone(), p.clone(), s.clone(), installed.clone()].into_iter().collect();
        let gate = |x: &Path, r: &HashSet<PathBuf>| check_archive_removal(x, r, &profiles, &mods);
        assert!(gate(&a, &reported).is_ok());
        assert!(gate(&p, &reported).is_err(), "profiles");
        assert!(gate(&s, &reported).is_err(), "protected");
        assert!(gate(&installed, &reported).is_err(), "an installed mod");
        assert!(gate(&a, &HashSet::new()).is_err(), "not reported");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn last_used_comes_from_the_coach_index() {
        let json = br#"{
            "C:/u/mxbcoach/sessions/a.mxbc": {"size": 1, "modified": 100, "summary": {"trackId": "RedBud", "bikeId": "KTM450"}},
            "C:/u/mxbcoach/sessions/b.mxbc": {"size": 1, "modified": 200, "summary": {"trackId": "redbud", "bikeId": ""}}
        }"#;
        let (used, n) = coach_last_used(json);
        assert_eq!(n, 2);
        assert_eq!(used.get("redbud"), Some(&200_000));
        assert_eq!(used.get("ktm450"), Some(&100_000));
        let m = |name: &str, cat: &str| StorageMod {
            name: name.into(),
            path: String::new(),
            subpath: String::new(),
            category: cat.into(),
            kind: "pkz".into(),
            size: 0,
            modified: 0,
            secured: false,
            last_used: None,
        };
        assert_eq!(last_used_for(&m("RedBud.pkz", "track"), &used), Some(200_000));
        assert_eq!(last_used_for(&m("RedBud.pnt", "bikePaint"), &used), None);
        assert_eq!(coach_last_used(b"not json").1, 0);
    }
}
