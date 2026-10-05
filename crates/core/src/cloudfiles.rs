//! Notice when the mods folder isn't really on disk.
//!
//! OneDrive, Dropbox and iCloud all offer the same trick: a file that looks completely
//! ordinary in Explorer — right name, right size — while its bytes still live on a server.
//! Windows calls these placeholders, and reading one is supposed to be transparent: the
//! filter driver fetches the content and the read succeeds, a little late.
//!
//! "A little late" is the problem. MX Bikes reads the mods tree during the load screen,
//! memory-mapped and on the critical path, and a placeholder whose fetch is slow, offline
//! or refused surfaces there as a failed read of a mapped page — `STATUS_IN_PAGE_ERROR`,
//! which is a crash, not an error message. From the player's side the game "just crashes on
//! the loading screen", with nothing in any log to say why, and it recurs for as long as
//! the file stays evicted.
//!
//! The app is already watching this folder ([`crate::modwatch`]) and already knows when a
//! session starts ([`crate::sessionwatch`]), so it is in a position to answer the question
//! before the crash rather than after: are these files actually here?
//!
//! Note what this deliberately does **not** do. It never opens a placeholder, because
//! opening one is what triggers the download — a scan that hydrated the folder would turn a
//! diagnostic into a multi-gigabyte surprise. It only ever asks for attributes, which the
//! filter driver answers from metadata it already has.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Event name the UI listens on.
pub const EVENT: &str = "mods-dehydrated";

/// Only content files matter. A placeholder `readme.txt` is nobody's crash.
const CONTENT_EXTENSIONS: [&str; 4] = ["pkz", "pnt", "edf", "sav"];

/// Stop walking after this many files. A mods tree in the thousands is normal and the
/// answer does not get truer past this point — one placeholder is already the whole story.
const MAX_FILES: usize = 20_000;

/// How deep to walk. Deep enough for `mods/tracks/<name>/<files>` and a level of slack.
const MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dehydrated {
    /// How many content files are placeholders rather than real bytes.
    pub count: usize,
    /// How many content files were looked at, so `count` has a denominator.
    pub scanned: usize,
    /// A few names, to make the warning concrete rather than a number.
    pub examples: Vec<String>,
    /// Whether the tree sits under a recognisable sync root, which changes the advice.
    pub provider: Option<String>,
}

/// Check the mods tree in the background and warn if any of it isn't really there.
///
/// Fire-and-forget by design: this runs as the game is starting, and must not add a step
/// to the Play button under any circumstance.
pub fn warn_if_dehydrated(app: &AppHandle, cfg: &crate::config::AppConfig) {
    let root = crate::library::mods_root(&cfg.mods_path);
    let app = app.clone();
    std::thread::spawn(move || {
        let mut found = scan(&root);
        // Two separate problems, and the second used to go unmentioned.
        //
        // Evicted bytes are the crash. But a tree that merely *sits* on a sync provider is
        // slow to read even fully hydrated — every read goes through the filter driver — and
        // the game reads the whole tree during the load screen. One player's took 5–6 seconds
        // per track folder, which is a game that never reaches the loading screen at all. That
        // is worth saying before it happens, not only when bytes have actually gone.
        if found.count == 0 && found.provider.is_none() {
            return;
        }
        // The gate above is the *detected* provider; the name below is what the player is
        // told. When the path doesn't say, don't hedge — on Windows this is OneDrive far
        // more often than not, and "a cloud sync tool" only makes players insist they don't
        // have one. It ships on, and it syncs Documents by default.
        found.provider = found.provider.or_else(fallback_provider);
        let provider = found.provider.clone().unwrap_or_else(|| "a cloud sync tool".into());
        if found.count > 0 {
            log::warn!(
                "[cloud] {} of {} mod file(s) under {} are placeholders, not real files — \
                 {provider} has evicted them. The game reads these during the load screen and \
                 can crash there (in-page error). Fix: right-click the folder in Explorer and \
                 choose \"Always keep on this device\", or move the folder out of {provider}. \
                 Examples: {}",
                found.count,
                found.scanned,
                root.display(),
                found.examples.join(", ")
            );
        } else {
            log::warn!(
                "[cloud] the mods folder is inside {provider} ({}). Every read there goes \
                 through {provider}'s filter driver, and the game reads the whole tree during \
                 the load screen — slow enough, on a big collection, to look like the game has \
                 hung. Nothing is evicted right now, so this is a warning, not a fault. Fix: \
                 move the folder out of {provider}.",
                root.display()
            );
        }
        let _ = app.emit(EVENT, &found);
    });
}

/// Walk `root` and count content files that are placeholders.
fn scan(root: &std::path::Path) -> Dehydrated {
    let mut out = Dehydrated { provider: provider_of(root), ..Default::default() };
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH || out.scanned >= MAX_FILES {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if out.scanned >= MAX_FILES {
                break;
            }
            let path = entry.path();
            // `file_type` reads the directory entry we already have — it does not open
            // the file, so it cannot trigger a download.
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            if !is_content(&path) {
                continue;
            }
            out.scanned += 1;
            if !is_placeholder(&path) {
                continue;
            }
            out.count += 1;
            if out.examples.len() < 5 {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    out.examples.push(name.to_string());
                }
            }
        }
    }
    out
}

fn is_content(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| CONTENT_EXTENSIONS.contains(&e.as_str()))
}

/// Name the sync tool from the path, so the advice can name it too. `None` when the folder
/// is somewhere unrecognised — a placeholder there is still a placeholder.
/// The sync provider the configured mods tree sits under, if any.
///
/// A path question, not a disk one: it answers whether reads there go through a sync
/// tool's filter driver at all, which is true even when every file is currently hydrated.
/// That is the thing worth knowing before asking the game to re-walk the tree.
pub fn mods_provider(cfg: &crate::config::AppConfig) -> Option<String> {
    provider_of(&crate::library::mods_root(&cfg.mods_path))
}

/// What to call the sync tool when the path doesn't name one. `None` where the platform
/// has no obvious default, and the UI supplies a generic phrase instead.
fn fallback_provider() -> Option<String> {
    if cfg!(windows) {
        Some("OneDrive".into())
    } else if cfg!(target_os = "macos") {
        Some("iCloud Drive".into())
    } else {
        None
    }
}

fn provider_of(root: &std::path::Path) -> Option<String> {
    let lower = root.to_string_lossy().to_ascii_lowercase();
    for (needle, name) in [
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("google drive", "Google Drive"),
        ("icloud", "iCloud Drive"),
        ("creative cloud", "Creative Cloud"),
    ] {
        if lower.contains(needle) {
            return Some(name.to_string());
        }
    }
    None
}

/// Is this file a placeholder rather than real bytes?
///
/// Three attributes, because the providers do not agree on one:
///
///   * `RECALL_ON_DATA_ACCESS` — the modern per-file placeholder (OneDrive Files On-Demand).
///   * `RECALL_ON_OPEN` — the older whole-file variant.
///   * `OFFLINE` — set by classic HSM tools, and still what some providers use.
#[cfg(windows)]
pub fn is_placeholder(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

    extern "system" {
        fn GetFileAttributesW(name: *const u16) -> u32;
    }

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call. This reads
    // metadata only — it never opens the file, so it cannot trigger a hydration.
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return false;
    }
    attrs
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
        != 0
}

/// macOS has the same idea: iCloud Drive evicts a file's bytes and leaves the entry behind,
/// flagged `SF_DATALESS`. Reading one is *meant* to fetch it back transparently, and often
/// does — but not always, and a `.pkz` that reads as empty is indistinguishable from a mod
/// with nothing in it. That is why this is worth knowing before reading rather than after.
///
/// Like the Windows half, this asks for attributes only. `stat` does not hydrate.
#[cfg(target_os = "macos")]
pub fn is_placeholder(path: &std::path::Path) -> bool {
    use std::os::macos::fs::MetadataExt;
    /// `SF_DATALESS` from `sys/stat.h` — the bytes live in iCloud, not here.
    const SF_DATALESS: u32 = 0x4000_0000;
    std::fs::metadata(path).is_ok_and(|m| m.st_flags() & SF_DATALESS != 0)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_placeholder(_path: &std::path::Path) -> bool {
    false
}

// ---------------------------------------------------------------------------
// The health check: is the PiBoSo folder in OneDrive, and is any of it online-only?
// ---------------------------------------------------------------------------
//
// The session-start warning above only fires when the app sees a game session begin, and it
// only lands in the log. A player who launches from Steam never sees it. This half answers
// the same question on demand, for the Home screen and the log export, and adds the one fix
// the app can make for them: mark the folder "Always keep on this device".
//
// Joining a busy server is where an online-only folder hurts most: the game loads every
// other rider's bike, paints and textures at once, on its main thread, and each placeholder
// it touches is a download it has to wait for.

/// `FILE_ATTRIBUTE_*` values the check reads. Spelled out here rather than pulled from a
/// bindings crate: the core crate binds the handful of Win32 calls it needs by hand.
pub mod attr {
    pub const READONLY: u32 = 0x0000_0001;
    pub const HIDDEN: u32 = 0x0000_0002;
    pub const SYSTEM: u32 = 0x0000_0004;
    pub const DIRECTORY: u32 = 0x0000_0010;
    pub const ARCHIVE: u32 = 0x0000_0020;
    pub const NORMAL: u32 = 0x0000_0080;
    pub const TEMPORARY: u32 = 0x0000_0100;
    pub const OFFLINE: u32 = 0x0000_1000;
    pub const NOT_CONTENT_INDEXED: u32 = 0x0000_2000;
    pub const RECALL_ON_OPEN: u32 = 0x0004_0000;
    /// "Always keep on this device" — what Explorer's menu item and `attrib +P` set.
    pub const PINNED: u32 = 0x0008_0000;
    /// "Free up space" — the opposite of `PINNED`; the two must never be set together.
    pub const UNPINNED: u32 = 0x0010_0000;
    pub const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

    /// The attributes `SetFileAttributesW` accepts. Anything else in a value read back from
    /// `GetFileAttributesW` (directory, reparse point, the recall bits) has to be masked off
    /// before writing it back.
    pub const SETTABLE: u32 = READONLY
        | HIDDEN
        | SYSTEM
        | ARCHIVE
        | TEMPORARY
        | OFFLINE
        | NOT_CONTENT_INDEXED
        | PINNED
        | UNPINNED;
}

/// The bytes of a file with these attributes are not on this PC.
pub fn attrs_online_only(attrs: u32) -> bool {
    attrs & (attr::OFFLINE | attr::RECALL_ON_OPEN | attr::RECALL_ON_DATA_ACCESS) != 0
}

/// Marked "Always keep on this device".
pub fn attrs_pinned(attrs: u32) -> bool {
    attrs & attr::PINNED != 0
}

/// The attribute set to write to pin an item: whatever it had that can be written back, minus
/// "free up space", plus "always keep". Never zero, so it can't be read as "no change".
pub fn attrs_to_pin(attrs: u32) -> u32 {
    let next = (attrs & attr::SETTABLE & !attr::UNPINNED) | attr::PINNED;
    if next == 0 {
        attr::NORMAL
    } else {
        next
    }
}

/// OneDrive's own roots, from the variables its client sets for every signed-in account.
/// They catch a OneDrive folder that has been moved or renamed — a path check alone would
/// miss `D:\Cloud\Documents`.
pub fn onedrive_roots() -> Vec<std::path::PathBuf> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(|v| std::env::var_os(v))
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .collect()
}

/// Does `path` sit under OneDrive? Either a folder in it is named like OneDrive's own
/// (`OneDrive`, `OneDrive - Contoso`), or it is under one of `roots`.
pub fn in_onedrive(path: &std::path::Path, roots: &[std::path::PathBuf]) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    // Split on both separators rather than `components()`: a Windows path read on the Linux
    // side of Proton is one long component with backslashes in it.
    let named = path
        .to_string_lossy()
        .split(['/', '\\'])
        .any(|seg| seg.to_ascii_lowercase().starts_with("onedrive"));
    if named {
        return true;
    }
    let norm = |p: &std::path::Path| {
        p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase()
    };
    let here = norm(path);
    roots.iter().map(|r| norm(r)).any(|r| {
        !r.is_empty() && (here == r || here.starts_with(&format!("{r}\\")))
    })
}

/// The PiBoSo folder a game folder sits in (`Documents\PiBoSo` for `…\PiBoSo\MX Bikes\mods`),
/// or `dir` itself when there is no such ancestor. This is the folder worth pinning: it holds
/// the mods and the profiles both, for every PiBoSo game the player has.
pub fn piboso_root(dir: &std::path::Path) -> std::path::PathBuf {
    dir.ancestors()
        .find(|a| {
            a.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("piboso"))
        })
        .unwrap_or(dir)
        .to_path_buf()
}

/// Online-only files, by what the game would be loading when it reached them.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AreaCounts {
    pub bikes: usize,
    pub tracks: usize,
    pub paints: usize,
    pub plugins: usize,
}

impl AreaCounts {
    pub fn total(&self) -> usize {
        self.bikes + self.tracks + self.paints + self.plugins
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudHealth {
    /// The folder "Keep on this device" pins — the PiBoSo folder. Empty when unknown.
    pub piboso_dir: String,
    /// It sits under OneDrive.
    pub piboso_in_onedrive: bool,
    /// The game's install folder, and whether that is under OneDrive too.
    pub game_dir: String,
    pub game_in_onedrive: bool,
    /// The PiBoSo folder already carries "Always keep on this device".
    pub pinned: bool,
    /// Files whose bytes aren't on this PC.
    pub online_only: AreaCounts,
    /// Files looked at, so `online_only` has a denominator.
    pub scanned: usize,
    /// The walk stopped at its cap; the counts are a floor.
    pub truncated: bool,
}

impl CloudHealth {
    pub fn in_onedrive(&self) -> bool {
        self.piboso_in_onedrive || self.game_in_onedrive
    }

    /// Worth a notice: anything in OneDrive, or anything online-only wherever it is.
    pub fn needs_attention(&self) -> bool {
        self.in_onedrive() || self.online_only.total() > 0
    }

    /// One line for the log export's `summary.txt`.
    pub fn summary_line(&self) -> String {
        let place = match (self.piboso_in_onedrive, self.game_in_onedrive) {
            (true, true) => "PiBoSo and game folders in OneDrive",
            (true, false) => "PiBoSo folder in OneDrive",
            (false, true) => "game folder in OneDrive",
            (false, false) => "not in OneDrive",
        };
        let c = &self.online_only;
        format!(
            "onedrive: {place}; {} online-only of {}{} scanned (bikes {}, tracks {}, paints {}, \
             plugins {}); pinned: {}",
            c.total(),
            self.scanned,
            if self.truncated { "+" } else { "" },
            c.bikes,
            c.tracks,
            c.paints,
            c.plugins,
            if self.pinned { "yes" } else { "no" },
        )
    }
}

/// Cap on the walk. A big mods tree is tens of thousands of files; the answer is already
/// clear long before this.
const HEALTH_MAX_FILES: usize = 250_000;
const HEALTH_MAX_DEPTH: usize = 12;

/// Which area a file under the mods tree belongs to. Paints live inside bike folders
/// (`bikes/<Bike>/paints/…`), so they are told apart by the `paints` segment or the extension.
fn mods_area(rel: &std::path::Path) -> Option<Area> {
    let mut segs = rel.components().map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase());
    let top = segs.next()?;
    let rest: Vec<String> = segs.collect();
    let is_paint = rest.iter().any(|s| s == "paints")
        || rest.last().is_some_and(|f| f.ends_with(".pnt"));
    match top.as_str() {
        "bikes" if is_paint => Some(Area::Paints),
        "bikes" => Some(Area::Bikes),
        "tracks" => Some(Area::Tracks),
        _ if is_paint => Some(Area::Paints),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Area {
    Bikes,
    Tracks,
    Paints,
}

impl AreaCounts {
    fn bump(&mut self, area: Area) {
        match area {
            Area::Bikes => self.bikes += 1,
            Area::Tracks => self.tracks += 1,
            Area::Paints => self.paints += 1,
        }
    }
}

/// Is this directory entry online-only? Reads the attributes the directory listing already
/// carries — on Windows `DirEntry::metadata` costs no extra call and never opens the file.
#[cfg(windows)]
fn entry_online_only(entry: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    entry.metadata().is_ok_and(|m| attrs_online_only(m.file_attributes()))
}

#[cfg(not(windows))]
fn entry_online_only(entry: &std::fs::DirEntry) -> bool {
    is_placeholder(&entry.path())
}

/// Walk `root`, calling `on_file(relative path, online_only)` per file. Never opens a file.
fn walk_files(
    root: &std::path::Path,
    budget: &mut usize,
    truncated: &mut bool,
    mut on_file: impl FnMut(&std::path::Path, bool),
) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > HEALTH_MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if *budget == 0 {
                *truncated = true;
                return;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            *budget -= 1;
            let rel = path.strip_prefix(root).unwrap_or(&path);
            on_file(rel, entry_online_only(&entry));
        }
    }
}

/// Count online-only files in the places the game loads from when joining a server: the
/// mods tree's bikes, tracks and paints, rider paints under the profiles folder, and the
/// game's `plugins` folder. Attributes only — nothing is hydrated by looking.
pub fn check(
    mods_root: &std::path::Path,
    profiles_dir: &std::path::Path,
    game_dir: &std::path::Path,
) -> CloudHealth {
    check_with_roots(mods_root, profiles_dir, game_dir, &onedrive_roots())
}

/// The folder to pin: the PiBoSo folder above the mods tree, or above the profiles when the
/// mods tree has been moved somewhere with no PiBoSo parent, or the mods tree itself.
pub fn pin_dir(
    mods_root: &std::path::Path,
    profiles_dir: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let has = |p: &&std::path::Path| !p.as_os_str().is_empty();
    [mods_root, profiles_dir]
        .into_iter()
        .filter(has)
        .map(piboso_root)
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("piboso")))
        .or_else(|| has(&mods_root).then(|| mods_root.to_path_buf()))
}

pub fn check_with_roots(
    mods_root: &std::path::Path,
    profiles_dir: &std::path::Path,
    game_dir: &std::path::Path,
    roots: &[std::path::PathBuf],
) -> CloudHealth {
    let mut out = CloudHealth::default();
    let has = |p: &std::path::Path| !p.as_os_str().is_empty();

    if let Some(p) = &pin_dir(mods_root, profiles_dir) {
        out.piboso_dir = p.to_string_lossy().into_owned();
        out.piboso_in_onedrive = in_onedrive(p, roots) || in_onedrive(mods_root, roots);
        out.pinned = read_attrs(p).is_some_and(attrs_pinned);
    }
    if has(game_dir) {
        out.game_dir = game_dir.to_string_lossy().into_owned();
        out.game_in_onedrive = in_onedrive(game_dir, roots);
    }

    let mut budget = HEALTH_MAX_FILES;
    let mut truncated = false;
    let mut counts = AreaCounts::default();
    let mut scanned = 0usize;
    if has(mods_root) {
        for sub in ["bikes", "tracks"] {
            let dir = crate::library::resolve_child(mods_root, sub);
            walk_files(&dir, &mut budget, &mut truncated, |rel, online| {
                scanned += 1;
                if online {
                    if let Some(area) = mods_area(&std::path::Path::new(sub).join(rel)) {
                        counts.bump(area);
                    }
                }
            });
        }
    }
    if has(profiles_dir) {
        walk_files(profiles_dir, &mut budget, &mut truncated, |rel, online| {
            let paint = rel.components().any(|c| c.as_os_str().eq_ignore_ascii_case("paints"));
            if paint {
                scanned += 1;
                if online {
                    counts.paints += 1;
                }
            }
        });
    }
    if has(game_dir) {
        let dir = crate::library::resolve_child(game_dir, "plugins");
        walk_files(&dir, &mut budget, &mut truncated, |_, online| {
            scanned += 1;
            if online {
                counts.plugins += 1;
            }
        });
    }
    out.online_only = counts;
    out.scanned = scanned;
    out.truncated = truncated;
    out
}

#[cfg(windows)]
fn wide(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
mod win {
    extern "system" {
        pub fn GetFileAttributesW(name: *const u16) -> u32;
        pub fn SetFileAttributesW(name: *const u16, attrs: u32) -> i32;
    }
}

/// The item's attributes, without opening it.
#[cfg(windows)]
pub fn read_attrs(path: &std::path::Path) -> Option<u32> {
    let w = wide(path);
    // SAFETY: NUL-terminated UTF-16 that outlives the call; metadata only.
    let a = unsafe { win::GetFileAttributesW(w.as_ptr()) };
    (a != u32::MAX).then_some(a)
}

#[cfg(not(windows))]
pub fn read_attrs(_path: &std::path::Path) -> Option<u32> {
    None
}

/// What pinning did, for the player to read as it is.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinResult {
    /// False off Windows, where there is no such attribute to set.
    pub supported: bool,
    /// Files and folders found under the PiBoSo folder, the folder itself included.
    pub total: usize,
    /// Of those, how many now carry "Always keep on this device".
    pub pinned: usize,
    /// How many refused the change.
    pub failed: usize,
    /// The first refusal, so a failure has a reason attached.
    pub first_error: Option<String>,
    /// Files still online-only right after pinning. OneDrive downloads them in the
    /// background; until it finishes, they are not on this PC yet.
    pub still_online_only: usize,
}

/// Mark `root` and everything under it "Always keep on this device" — the same attribute
/// Explorer's menu item and `attrib +P -U /s /d` set. OneDrive then downloads whatever is
/// online-only and stops evicting it. Nothing is moved, copied or read.
///
/// `progress(done, total)` is called as it goes; `total` is known after a metadata-only walk.
#[cfg(windows)]
pub fn pin_tree(root: &std::path::Path, mut progress: impl FnMut(usize, usize)) -> PinResult {
    let mut out = PinResult { supported: true, ..Default::default() };
    let mut items = vec![root.to_path_buf()];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|k| k.is_dir()) {
                stack.push(path.clone());
            }
            items.push(path);
        }
    }
    out.total = items.len();
    progress(0, out.total);
    for (i, path) in items.iter().enumerate() {
        match read_attrs(path) {
            Some(a) if attrs_pinned(a) && a & attr::UNPINNED == 0 => out.pinned += 1,
            Some(a) => {
                let w = wide(path);
                // SAFETY: NUL-terminated UTF-16 that outlives the call. Changes attributes
                // only; the file is not opened, so this cannot hydrate it by itself.
                let ok = unsafe { win::SetFileAttributesW(w.as_ptr(), attrs_to_pin(a)) } != 0;
                if ok {
                    out.pinned += 1;
                } else {
                    out.failed += 1;
                    if out.first_error.is_none() {
                        out.first_error = Some(format!(
                            "{}: {}",
                            path.display(),
                            std::io::Error::last_os_error()
                        ));
                    }
                }
            }
            None => {
                out.failed += 1;
                if out.first_error.is_none() {
                    out.first_error = Some(format!("{}: can't read its attributes", path.display()));
                }
            }
        }
        if i % 200 == 0 || i + 1 == out.total {
            progress(i + 1, out.total);
        }
    }
    out.still_online_only = items
        .iter()
        .filter(|p| read_attrs(p).is_some_and(|a| a & attr::DIRECTORY == 0 && attrs_online_only(a)))
        .count();
    out
}

#[cfg(not(windows))]
pub fn pin_tree(_root: &std::path::Path, _progress: impl FnMut(usize, usize)) -> PinResult {
    PinResult::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_parse_to_online_only_and_pinned() {
        // A hydrated, ordinary file.
        assert!(!attrs_online_only(attr::ARCHIVE));
        // Files On-Demand placeholder, the classic offline bit, and the whole-file variant.
        assert!(attrs_online_only(attr::ARCHIVE | attr::RECALL_ON_DATA_ACCESS));
        assert!(attrs_online_only(attr::OFFLINE));
        assert!(attrs_online_only(attr::RECALL_ON_OPEN | attr::DIRECTORY));
        // Pinned but still downloading: both are true at once, and that's honest.
        let downloading = attr::PINNED | attr::RECALL_ON_DATA_ACCESS;
        assert!(attrs_pinned(downloading) && attrs_online_only(downloading));
        assert!(!attrs_pinned(attr::UNPINNED));
    }

    #[test]
    fn pinning_clears_unpinned_and_drops_unsettable_bits() {
        let a = attr::ARCHIVE | attr::UNPINNED | attr::RECALL_ON_DATA_ACCESS | attr::DIRECTORY;
        let next = attrs_to_pin(a);
        assert_eq!(next, attr::ARCHIVE | attr::PINNED);
        assert_eq!(attrs_to_pin(0), attr::PINNED);
        assert_eq!(attrs_to_pin(attr::READONLY | attr::HIDDEN), attr::READONLY | attr::HIDDEN | attr::PINNED);
    }

    #[test]
    fn onedrive_paths_are_recognised_and_other_drives_are_not() {
        let none: Vec<std::path::PathBuf> = Vec::new();
        let p = std::path::Path::new("C:\\Users\\u\\OneDrive\\Documents\\PiBoSo\\MX Bikes\\mods");
        assert!(in_onedrive(p, &none));
        assert!(in_onedrive(std::path::Path::new("C:/Users/u/OneDrive - Contoso/Documents/PiBoSo"), &none));
        assert!(!in_onedrive(std::path::Path::new("D:\\Games\\PiBoSo\\MX Bikes"), &none));
        assert!(!in_onedrive(std::path::Path::new("C:\\Users\\u\\Documents\\PiBoSo"), &none));
        assert!(!in_onedrive(std::path::Path::new(""), &none));
        // A OneDrive moved somewhere that doesn't say so is caught by its root.
        let roots = vec![std::path::PathBuf::from("D:\\Cloud")];
        assert!(in_onedrive(std::path::Path::new("d:\\cloud\\Documents\\PiBoSo"), &roots));
        assert!(!in_onedrive(std::path::Path::new("D:\\Cloudy\\PiBoSo"), &roots));
        assert!(!in_onedrive(std::path::Path::new("E:\\Cloud\\PiBoSo"), &roots));
    }

    #[test]
    fn the_piboso_folder_is_found_above_the_mods_tree() {
        let p = std::path::Path::new("C:/Users/u/OneDrive/Documents/PiBoSo/MX Bikes/mods");
        assert_eq!(piboso_root(p), std::path::Path::new("C:/Users/u/OneDrive/Documents/PiBoSo"));
        let moved = std::path::Path::new("D:/mods");
        assert_eq!(piboso_root(moved), moved);
    }

    #[test]
    fn files_are_counted_by_area() {
        let count = |rel: &str| {
            let mut c = AreaCounts::default();
            if let Some(area) = mods_area(std::path::Path::new(rel)) {
                c.bump(area);
            }
            c
        };
        assert_eq!(count("bikes/KTM/ktm.pkz").bikes, 1);
        assert_eq!(count("bikes/KTM/paints/red.pnt").paints, 1);
        assert_eq!(count("bikes/KTM/loose.pnt").paints, 1);
        assert_eq!(count("tracks/Club/club.pkz").tracks, 1);
        assert_eq!(count("misc/readme.txt").total(), 0);
    }

    /// NTFS stores the pinned bit on any file, synced or not, so a temp folder is enough to
    /// check the walk marks every item, the folder itself included, and reports it.
    #[cfg(windows)]
    #[test]
    fn pinning_marks_the_folder_and_everything_in_it() {
        let d = std::env::temp_dir().join(format!("frost-pin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("mods/bikes")).unwrap();
        std::fs::write(d.join("mods/bikes/a.pkz"), b"x").unwrap();
        let mut last = (0, 0);
        let r = pin_tree(&d, |done, total| last = (done, total));
        assert!(r.supported);
        assert_eq!((r.total, r.pinned, r.failed, r.still_online_only), (4, 4, 0, 0));
        assert_eq!(last, (4, 4));
        assert!(read_attrs(&d.join("mods/bikes/a.pkz")).is_some_and(attrs_pinned));
        assert!(read_attrs(&d).is_some_and(attrs_pinned));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_local_folder_off_onedrive_reports_nothing() {
        let root = std::env::temp_dir().join(format!("frost-cloudhealth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mods = root.join("Games").join("mods");
        std::fs::create_dir_all(mods.join("bikes/KTM/paints")).unwrap();
        std::fs::create_dir_all(mods.join("tracks/Club")).unwrap();
        std::fs::write(mods.join("bikes/KTM/ktm.pkz"), b"x").unwrap();
        std::fs::write(mods.join("bikes/KTM/paints/red.pnt"), b"x").unwrap();
        std::fs::write(mods.join("tracks/Club/club.pkz"), b"x").unwrap();
        let game = root.join("Game");
        std::fs::create_dir_all(game.join("plugins")).unwrap();
        std::fs::write(game.join("plugins/a.dlo"), b"x").unwrap();

        let h = check_with_roots(&mods, &root.join("nope"), &game, &[]);
        assert_eq!(h.scanned, 4);
        assert_eq!(h.online_only.total(), 0);
        assert!(!h.needs_attention() || root.to_string_lossy().to_ascii_lowercase().contains("onedrive"));
        assert!(h.summary_line().starts_with("onedrive: "));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_content_extensions_are_counted() {
        assert!(is_content(std::path::Path::new("a/b/bike.pkz")));
        assert!(is_content(std::path::Path::new("a/b/PAINT.PNT")));
        assert!(!is_content(std::path::Path::new("a/b/readme.txt")));
        assert!(!is_content(std::path::Path::new("a/b/noext")));
    }

    #[test]
    fn the_provider_is_named_from_the_path() {
        let p = std::path::Path::new("C:/Users/x/OneDrive/Documents/PiBoSo/MX Bikes/mods");
        assert_eq!(provider_of(p).as_deref(), Some("OneDrive"));
        assert_eq!(provider_of(std::path::Path::new("D:/Games/mods")), None);
    }

    #[test]
    fn a_real_folder_reports_nothing_dehydrated() {
        // Whatever else is true of the temp dir, nothing in it is a cloud placeholder.
        let found = scan(&std::env::temp_dir());
        assert_eq!(found.count, 0);
    }
}
