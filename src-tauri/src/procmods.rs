//! What is loaded inside the running game.
//!
//! The app already reports presence and anonymous usage. This is the same idea pointed at
//! the game process: while MX Bikes is up, walk its module list and report what is in it —
//! each file's name, where it was loaded from, a hash for anything that is not a Windows
//! system library, and what the file says about itself: its size, when it was last written,
//! whether Windows trusts its signature and who signed it, and the company and product it
//! claims in its version resource (see [`crate::fileinfo`]).
//!
//! The extra detail is there because a name and a hash only identify what is already known.
//! Every first sighting is a name nobody recognises, and "unsigned, no company, written last
//! Tuesday" is what makes one of those readable without recognising it.
//!
//! **This module makes no judgements and holds no lists of anything.** It records where a
//! file came from and hands that over; what any of it means is decided by the control plane
//! against rules it holds, which is what lets those rules change in a minute rather than in
//! a release, and what keeps them out of a binary anyone can read. Nothing comes back down:
//! the endpoint answers `{ ok: true }` and the app is never told what was made of a report.
//!
//! Cheap by construction, because it runs beside a game:
//!
//!   * A module list walk is a Toolhelp snapshot, which is what [`crate::gameproc`] already
//!     does to find FrostMod.
//!   * Hashes are cached by path, size and mtime, so a file is read once per session rather
//!     than once per pass.
//!   * A report is only sent when the module set has actually changed, with a heartbeat so a
//!     settled session still says it is there.

use crate::fileinfo::FileFacts;
use crate::gameproc::ExecRegion;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Refuse to hash anything larger than this. A library is a few megabytes; a hundred-megabyte
/// one would only be a way to make every pass stall on disk I/O.
const MAX_HASH_BYTES: u64 = 96 * 1024 * 1024;

/// The most modules one report carries. The control plane caps the same number.
const MAX_MODULES: usize = 400;

/// How often an unchanged session says it is still there.
const HEARTBEAT: Duration = Duration::from_secs(300);

/// Where a module was loaded from. A statement about location, and only that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Inside the game's own install folder.
    Game,
    /// One of Windows' own library folders — or, under Wine, the prefix's equivalent.
    System,
    /// Inside something this app installed.
    App,
    /// Anywhere else.
    Other,
}

impl Origin {
    fn is_system(self) -> bool {
        matches!(self, Origin::System)
    }
}

/// Everything one pass reads off a file on disk.
///
/// Taken in one go and cached together, because the expensive half is the same for all of
/// it: the file has to be opened and read either way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileRead {
    pub sha256: String,
    pub size: u64,
    /// Last written, as seconds since the epoch. Zero when it could not be read.
    pub mtime: i64,
    pub facts: FileFacts,
}

/// One module, as reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    /// File name, lowercased. Never a path: a path carries the player's user folder, and
    /// nothing on the other end needs one.
    pub name: String,
    pub origin: Origin,
    /// Empty for system libraries, which are not hashed, and for a file we could not read.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    /// Bytes on disk. Zero for anything not read, which is how it is left out on the wire.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub size: u64,
    /// Last written, seconds since the epoch. Two builds of a file that both refuse to hash
    /// still tell themselves apart by this.
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub mtime: i64,
    /// Signature and version resource. Flattened, so a module is one flat object on the wire
    /// rather than an object with a nested one nobody reading the payload would expect.
    #[serde(flatten)]
    pub facts: FileFacts,
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

/// The most unaccounted regions one report carries.
///
/// Small on purpose. Every one of these costs a read into the game's address space and a row
/// on the other end, and a machine with two dozen of them has already said everything it is
/// going to say — the twenty-fifth adds nothing the first did not.
const MAX_REGIONS: usize = 24;

/// The most files from the game's `plugins` folder one report carries.
const MAX_FILES: usize = 32;

/// The most regions one pass will read the head of.
///
/// A separate bound from [`MAX_REGIONS`], which counts what is kept. A process whose graphics
/// driver builds a great deal of code at runtime can have hundreds of unaccounted regions and
/// keep none of them, and without this the pass would read every one of them to find that out.
/// The ranking puts everything worth reading at the front, so a cut here costs the least
/// interesting reads.
const MAX_REGION_READS: usize = 200;

/// How much of a region is read looking for a PE header. One page is enough for the DOS
/// stub, the PE header and the section table; the rest is reached by RVA and only if the
/// headers point somewhere.
const REGION_HEAD: usize = 4096;

/// Executable memory in the game that no loaded module accounts for.
///
/// This is the half of the report the module list cannot produce. A DLL is only *in* the
/// module list because it asked the loader to put it there; code written straight into the
/// process and started with a thread never asks, and so has no name, no path and no file to
/// hash. What it cannot avoid is being executable memory the loader does not cover, and that
/// is what one of these is.
///
/// It carries no verdict, in the same way [`Module`] carries none. `rwx`, `private`,
/// `thread: true` and a PDB called `kaizo.pdb` are four observations; what they add up to is
/// decided against rules the client does not hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    /// `image`, `mapped` or `private` — what the kernel says backs it.
    pub kind: &'static str,
    pub size: u64,
    /// The page protection as letters: `rx`, `rwx`, and so on.
    pub protect: String,
    /// A thread in the game was created to start inside this region.
    #[serde(skip_serializing_if = "is_false")]
    pub thread: bool,
    /// A PE header was found at the base. Manually mapped code is still a PE image, because
    /// that is what it was compiled as and what its own loader stub needs it to be.
    #[serde(skip_serializing_if = "is_false")]
    pub image: bool,
    /// What the image calls itself in its export directory, when it has one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The file name — never the folder — of the PDB the linker recorded.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pdb: String,
    /// The linker timestamp, part of what makes the fingerprint a build and not a machine.
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub timestamp: u32,
    /// A fingerprint of the build, from [`crate::peident::PeIdent::fingerprint`], or of the
    /// first bytes when this is not an image at all.
    ///
    /// Deliberately not a hash of the region: a mapped image has had relocations applied
    /// against wherever it happened to land, so the same payload on two machines hashes
    /// differently and the hash identifies nothing. Header fields do not move.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha256: String,
}

/// What the game's threads look like, as three numbers.
///
/// Counts rather than a list, because the individual threads of a game are not interesting
/// and the two numbers that are do not need one. A thread whose start address is in no
/// loaded module was created to run code that came from nowhere the loader knows; a thread
/// carrying an armed hardware breakpoint is being used to hook a function without altering
/// a byte of it, which is a thing ordinary game code never does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Threads {
    pub total: u32,
    pub foreign: u32,
    pub breakpoints: u32,
}

/// A file sitting in the game's `plugins` folder.
///
/// MX Bikes loads every `.dlo` in there at startup, which makes it a way into the process
/// that needs no injector at all — and one the module list only describes while the game is
/// up and the plugin has actually loaded. A plugin that crashed, that the game refused, or
/// that is waiting for the next launch is invisible to everything else in this report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskFile {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub size: u64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub mtime: i64,
    /// Whether this file is also in the game's module list right now.
    #[serde(skip_serializing_if = "is_false")]
    pub loaded: bool,
    #[serde(flatten)]
    pub facts: FileFacts,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// Order the unaccounted regions by how much they are worth reading, and mark the ones a
/// foreign thread starts in.
///
/// Ranking rather than filtering, because what makes a region worth carrying is only known
/// after its head has been read, and reading is the expensive part. The order is the policy:
///
///   1. **A `MEM_IMAGE` region nothing covers** — something mapped as an image that the
///      loader's own list does not mention, which is what unlinking a module looks like.
///   2. **A region holding a foreign thread's start address** — code that came from nowhere
///      and is running.
///   3. **The largest of what is left**, because a payload is bigger than a stub.
pub fn rank_regions(regions: &[ExecRegion], starts: &[u64]) -> Vec<ExecRegion> {
    let mut ranked: Vec<ExecRegion> = regions
        .iter()
        .map(|region| {
            let end = region.base.saturating_add(region.size);
            let thread = starts.iter().any(|&s| s >= region.base && s < end);
            ExecRegion { thread, ..*region }
        })
        .collect();
    ranked.sort_by(|a, b| {
        let rank = |r: &ExecRegion| {
            (
                u8::from(r.kind == crate::gameproc::MEM_IMAGE),
                u8::from(r.thread),
            )
        };
        rank(b).cmp(&rank(a)).then_with(|| b.size.cmp(&a.size)).then_with(|| a.base.cmp(&b.base))
    });
    ranked
}

/// Is a ranked region worth carrying once its head has been read?
///
/// Three shapes, and the reason for each is that ordinary processes are full of executable
/// memory that no module covers — a graphics driver building shader code, a runtime with a
/// JIT — and reporting all of it would bury the answer in noise it can never be separated
/// from. An unlisted image, a PE header somewhere it should not be, and a thread running
/// code from nowhere are not that.
pub fn worth_reporting(region: &ExecRegion, image: bool) -> bool {
    region.kind == crate::gameproc::MEM_IMAGE || image || region.thread
}

/// What one report says.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload<'a> {
    app_version: &'a str,
    /// False when the module list could not be read — the game is above us, or the platform
    /// has nothing to read. Sent rather than skipped: "we could not look" is a real answer,
    /// and the alternative is silence that reads exactly like a clean machine.
    available: bool,
    /// The player's MX Bikes GUID, so a report is tied to a player and not only to an install.
    /// Empty until the game has signed in to Steam; skipped on the wire when unknown, and the
    /// next report carries it once it is.
    #[serde(skip_serializing_if = "str::is_empty")]
    guid: &'a str,
    modules: &'a [Module],
    /// Executable memory in the game that no loaded module accounts for.
    ///
    /// Sent even when empty, and so are the thread counts: an absent field means an app too
    /// old to have looked, and an empty one means it looked and found nothing. Collapsing
    /// those two into the same silence is the mistake `available` exists to avoid.
    regions: &'a [Region],
    threads: Threads,
    /// What is sitting in the game's `plugins` folder.
    files: &'a [DiskFile],
}

/// What the last report said, so an unchanged session stays quiet.
struct Sent {
    digest: u64,
    at: Instant,
}

fn last_sent() -> &'static Mutex<Option<Sent>> {
    static LAST: Mutex<Option<Sent>> = Mutex::new(None);
    &LAST
}

/// What has already been read off each file, keyed by path and invalidated by size or
/// mtime. What keeps a pass from re-reading the same forty files every forty-five seconds —
/// and it matters more now than it did: a signature check reads the whole file too.
fn hash_cache() -> &'static Mutex<HashMap<String, FileRead>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, FileRead>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Forget what was sent and what was hashed. Called when the game goes away, so the next
/// session reports in full rather than inheriting the last one's answer.
pub fn reset() {
    if let Ok(mut slot) = last_sent().lock() {
        *slot = None;
    }
    if let Ok(mut cache) = hash_cache().lock() {
        cache.clear();
    }
}

/// Look at the running game and report, if there is anything new to say.
///
/// Called from [`crate::sessionwatch`] on its poll rather than from a watcher of its own:
/// the question only has an answer while the game is up, and that loop already knows.
pub fn tick(app: &tauri::AppHandle) {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    let token = cfg.cp_token.trim().to_string();
    // Nothing to report to. Enrolment is what gives a report somewhere to go.
    if token.is_empty() {
        return;
    }

    let (available, modules) = match crate::gameproc::game_modules() {
        crate::gameproc::GameModules::Loaded(paths) => {
            (true, collect(&paths, &roots(app, &cfg)))
        }
        // Refused, or a platform that cannot read a mapping list. Both are "we could not
        // look", which is deliberately not the same as an empty list.
        crate::gameproc::GameModules::Denied | crate::gameproc::GameModules::Unavailable => {
            (false, Vec::new())
        }
        crate::gameproc::GameModules::NotRunning => return,
    };

    // Everything the module list cannot describe: executable memory nothing accounts for,
    // what the threads are doing, and what is sitting in the folder the game loads plugins
    // from. Only asked when the module list was readable — without it there is nothing to
    // measure a region against, and every mapping in the process would read as unaccounted.
    let (regions, threads) = if available { look_inside() } else { (Vec::new(), Threads::default()) };
    let files = if available {
        let loaded: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
        plugin_files(&std::path::PathBuf::from(cfg.install_dir()).join("plugins"), &loaded)
    } else {
        Vec::new()
    };

    let digest = digest(available, &modules, &regions, threads, &files);
    let due = match last_sent().lock() {
        Ok(slot) => match slot.as_ref() {
            Some(prev) => prev.digest != digest || prev.at.elapsed() >= HEARTBEAT,
            None => true,
        },
        Err(_) => true,
    };
    if !due {
        return;
    }
    if let Ok(mut slot) = last_sent().lock() {
        *slot = Some(Sent { digest, at: Instant::now() });
    }

    // The identity the game knows the player by, read only when a report is actually going
    // out — never on a tick that finds nothing new. Prefer the value already claimed and
    // persisted; otherwise read it out of the running game, which is up whenever this runs.
    // Empty before Steam sign-in, and a later heartbeat carries it once it is known.
    let guid = {
        let claimed = cfg.cp_guid.trim();
        if !claimed.is_empty() {
            claimed.to_string()
        } else {
            crate::gameproc::local_guid().unwrap_or_default()
        }
    };

    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn(async move {
        if let Err(e) =
            send(&token, &version, available, &guid, &modules, &regions, threads, &files).await
        {
            log::debug!("[diag] report not sent: {e:#}");
        }
    });
}

/// The folders whose contents are this app's own doing.
fn roots(app: &tauri::AppHandle, cfg: &crate::config::AppConfig) -> Roots {
    Roots {
        game: norm_str(&cfg.install_dir()),
        app: vec![norm(&crate::frostmod_manage::frostmod_dir(app))],
    }
}

/// The folders one pass is compared against.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// The game's install folder.
    pub game: String,
    /// Folders this app installed into.
    pub app: Vec<String>,
}

/// Turn a list of module paths into what gets reported.
///
/// Pure, so the whole of it is testable with made-up paths on a machine with no game on it.
/// `read` is passed in for the same reason.
pub fn collect(paths: &[String], roots: &Roots) -> Vec<Module> {
    collect_with(paths, roots, &read_file)
}

pub fn collect_with(
    paths: &[String],
    roots: &Roots,
    read: &dyn Fn(&Path) -> FileRead,
) -> Vec<Module> {
    let mut out: Vec<Module> = Vec::new();
    for path in paths.iter().take(MAX_MODULES) {
        let normalized = norm_str(path);
        let name = file_name_of(&normalized);
        if name.is_empty() {
            continue;
        }
        let origin = origin_of(&normalized, roots);
        // System libraries are not read at all: there are hundreds of them, they are the
        // same on every machine, and hashing and signature-checking them every session would
        // be the expensive half of this for no answer anyone wants. They keep the default
        // `unchecked` trust, which is not the same claim as `unsigned`.
        let file =
            if origin.is_system() { FileRead::default() } else { read(Path::new(path)) };
        out.push(Module {
            name,
            origin,
            sha256: file.sha256,
            size: file.size,
            mtime: file.mtime,
            facts: file.facts,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.sha256.cmp(&b.sha256)));
    out.dedup_by(|a, b| a.name == b.name && a.sha256 == b.sha256);
    out
}

/// Where did this come from?
///
/// The order matters in one place: the system test runs before the game test, so a Wine
/// prefix's `system32` inside the game's own folder still reads as the system folder.
fn origin_of(path: &str, roots: &Roots) -> Origin {
    if is_system_path(path) {
        return Origin::System;
    }
    if !roots.game.is_empty() && is_inside(path, &roots.game) {
        return Origin::Game;
    }
    if roots.app.iter().any(|root| !root.is_empty() && is_inside(path, root)) {
        return Origin::App;
    }
    Origin::Other
}

/// Is this one of the platform's own libraries, by where it lives?
///
/// Matched on the shape of the path rather than on a substring, so nothing buys a free pass
/// by putting itself in `C:\somewhere\windows\system32\`. Three spellings are real: a drive
/// root, the same folder inside a Wine prefix, and the builtin libraries a Wine or Proton
/// runtime maps in place of them.
fn is_system_path(path: &str) -> bool {
    const SYSTEM_DIRS: [&str; 4] = ["system32/", "syswow64/", "winsxs/", "globalization/"];
    const RUNTIME_DIRS: [&str; 4] = ["/wine/", "/proton", "/dist/lib/", "/files/lib/"];
    if RUNTIME_DIRS.iter().any(|d| path.contains(d)) {
        return true;
    }
    let Some((root, rest)) = path.split_once("/windows/") else { return false };
    let rooted = (root.len() == 2 && root.ends_with(':')) || root.ends_with("/drive_c");
    rooted && SYSTEM_DIRS.iter().any(|d| rest.starts_with(d))
}

/// Is `path` inside `root`? Compared on path boundaries, so `c:/games/mx` does not swallow
/// `c:/games/mxsomething`.
fn is_inside(path: &str, root: &str) -> bool {
    path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// A path lowercased with forward slashes, so Windows and Wine spellings compare equal.
fn norm(path: &Path) -> String {
    norm_str(&path.to_string_lossy())
}

fn norm_str(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
}

/// The file name from a path, lowercased.
fn file_name_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    // The control plane takes file names and refuses anything else, so a mapping whose name
    // is not one is dropped here rather than rejected there. An extension is part of that:
    // every mapped module has one, and requiring it drops the directories and the anonymous
    // mappings Linux hands back alongside them.
    let shaped = name
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'+' | b'(' | b')' | b'-'));
    if !shaped || !name.contains('.') || name.starts_with('.') || name.ends_with('.') {
        return String::new();
    }
    name
}

/// Everything read off one file: size, mtime, SHA-256, signature and version resource.
/// Cached by path, and the cache entry is thrown away when either size or mtime moves.
///
/// A file that cannot be read, or is implausibly large, comes back as the default — no hash,
/// no size, and `unchecked` rather than `unsigned`. A missing answer costs a weaker report,
/// never a wrong one: a file with no hash can still be recognised by name.
fn read_file(path: &Path) -> FileRead {
    let Ok(meta) = std::fs::metadata(path) else { return FileRead::default() };
    if !meta.is_file() || meta.len() > MAX_HASH_BYTES {
        return FileRead::default();
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let key = norm(path);
    if let Ok(cache) = hash_cache().lock() {
        if let Some(hit) = cache.get(&key) {
            if hit.size == meta.len() && hit.mtime == mtime {
                return hit.clone();
            }
        }
    }

    let read = FileRead {
        sha256: sha256_of(path),
        size: meta.len(),
        mtime,
        // Asked after the hash, so a file that vanished between the two costs a signature
        // and not the whole entry.
        facts: crate::fileinfo::read(path),
    };
    if let Ok(mut cache) = hash_cache().lock() {
        cache.insert(key, read.clone());
    }
    read
}

/// SHA-256 of a file, lowercase hex. Empty when it cannot be opened or read through.
fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let Ok(mut file) = std::fs::File::open(path) else { return String::new() };
    let mut hasher = Sha256::new();
    if std::io::copy(&mut file, &mut hasher).is_err() {
        return String::new();
    }
    format!("{:x}", hasher.finalize())
}

/// Every file in the game's `plugins` folder, whether the game has loaded it or not.
///
/// The folder is small — a handful of files on a machine that has any — so it is read whole
/// rather than filtered to `.dlo`. A file in there that the game does not load is still
/// something that arrived in the folder the game loads from, and saying so costs one row.
pub fn plugin_files(dir: &Path, loaded: &[String]) -> Vec<DiskFile> {
    plugin_files_with(dir, loaded, &read_file)
}

pub fn plugin_files_with(
    dir: &Path,
    loaded: &[String],
    read: &dyn Fn(&Path) -> FileRead,
) -> Vec<DiskFile> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten().take(MAX_FILES * 4) {
        if out.len() >= MAX_FILES {
            break;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = file_name_of(&norm(&path));
        if name.is_empty() {
            continue;
        }
        let file = read(&path);
        out.push(DiskFile {
            loaded: loaded.iter().any(|m| *m == name),
            name,
            sha256: file.sha256,
            size: file.size,
            mtime: file.mtime,
            facts: file.facts,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Look inside the running game for what its module list cannot describe.
///
/// One open handle does all of it: where the loaded modules are, what executable memory none
/// of them covers, what the threads are doing, and — for anything unaccounted that turns out
/// to have a PE header — who that image says it is.
///
/// Off Windows [`crate::gameproc::GameProbe::open`] answers `None` and this is one branch
/// long. It is still the same function on every platform, because the half that reads a
/// header and ranks a region is ordinary code that deserves to be tested on the machine it
/// is written on.
fn look_inside() -> (Vec<Region>, Threads) {
    let Some(probe) = crate::gameproc::GameProbe::open() else {
        return (Vec::new(), Threads::default());
    };
    let ranges = probe.module_ranges();
    // No ranges means the snapshot failed, not that the game has no modules. Without them
    // every region in the process reads as unaccounted, which would be a report full of the
    // game's own code.
    if ranges.is_empty() {
        return (Vec::new(), Threads::default());
    }

    let survey = probe.threads(&ranges);
    let (regions, _) = probe.exec_regions(&ranges);
    let mut out = Vec::new();
    for region in rank_regions(&regions, &survey.starts).into_iter().take(MAX_REGION_READS) {
        if out.len() >= MAX_REGIONS {
            break;
        }
        let Some(head) = probe.read(region.base, REGION_HEAD) else { continue };
        // Reads inside the head we already have are answered from it; the export name and
        // the PDB record are usually further in and cost a read each.
        let ident = crate::peident::identify(&|rva, len| {
            let at = rva as usize;
            if let Some(end) = at.checked_add(len).filter(|end| *end <= head.len()) {
                return Some(head[at..end].to_vec());
            }
            probe.read(region.base.saturating_add(rva), len)
        });
        let is_image = ident.is_some();
        if !worth_reporting(&region, is_image) {
            continue;
        }
        let (name, pdb, timestamp, sha256) = match ident {
            Some(ident) if ident.is_useful() => (
                ident.name.clone(),
                ident.pdb.clone(),
                ident.timestamp,
                ident.fingerprint(),
            ),
            // Either not an image, or an image whose headers gave up nothing — and the
            // second must not fingerprint, because a fingerprint over empty fields is the
            // same on every machine in the world and a rule naming it would name everyone.
            // The head bytes are all there is to know it by, and headers carry no
            // relocations, so they read the same wherever the region landed.
            _ => (String::new(), String::new(), 0, sha256_of_bytes(&head)),
        };
        out.push(Region {
            kind: crate::gameproc::region_kind(region.kind),
            size: region.size,
            protect: crate::gameproc::protection(region.protect),
            thread: region.thread,
            image: is_image,
            name,
            pdb,
            timestamp,
            sha256,
        });
    }
    (
        out,
        Threads {
            total: survey.total,
            foreign: survey.foreign,
            breakpoints: survey.breakpoints,
        },
    )
}

/// SHA-256 of a run of bytes, lowercase hex.
fn sha256_of_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A stable fingerprint of one answer, so an unchanged session sends nothing.
///
/// FNV-1a over the sorted list. Not a cryptographic question: the only thing asked of it is
/// whether this pass differs from the last one.
fn digest(
    available: bool,
    modules: &[Module],
    regions: &[Region],
    threads: Threads,
    files: &[DiskFile],
) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    };
    eat(if available { b"1" } else { b"0" });
    for module in modules {
        eat(module.name.as_bytes());
        eat(module.sha256.as_bytes());
        eat(match module.origin {
            Origin::Game => b"g",
            Origin::System => b"s",
            Origin::App => b"a",
            Origin::Other => b"o",
        });
    }
    for region in regions {
        eat(region.kind.as_bytes());
        eat(region.name.as_bytes());
        eat(region.pdb.as_bytes());
        eat(region.sha256.as_bytes());
        eat(region.protect.as_bytes());
        eat(&region.size.to_le_bytes());
    }
    // The counts, not the identities: a thread that comes and goes is the same answer, and
    // one that appears where there were none is a different one.
    eat(&threads.foreign.to_le_bytes());
    eat(&threads.breakpoints.to_le_bytes());
    for file in files {
        eat(file.name.as_bytes());
        eat(file.sha256.as_bytes());
        eat(if file.loaded { b"1" } else { b"0" });
    }
    hash
}

#[allow(clippy::too_many_arguments)]
async fn send(
    token: &str,
    app_version: &str,
    available: bool,
    guid: &str,
    modules: &[Module],
    regions: &[Region],
    threads: Threads,
    files: &[DiskFile],
) -> anyhow::Result<()> {
    let res = reqwest::Client::new()
        .put(format!("{}/v1/diagnostics", crate::paintsync::control_plane()))
        .bearer_auth(token)
        .json(&Payload { app_version, available, guid, modules, regions, threads, files })
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("control plane said {}", res.status());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Roots {
        Roots {
            game: "c:/games/mx bikes".into(),
            app: vec!["c:/users/rider/appdata/local/mxbapp/frostmod".into()],
        }
    }

    fn collected(paths: &[&str]) -> Vec<Module> {
        let owned: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        collect_with(&owned, &roots(), &|p| {
            // A stand-in for reading the file, so the classification is testable without one.
            FileRead {
                sha256: format!("{:0>64}", p.to_string_lossy().len()),
                size: p.to_string_lossy().len() as u64,
                mtime: 1_700_000_000,
                facts: FileFacts {
                    trust: crate::fileinfo::Trust::Unsigned,
                    ..Default::default()
                },
            }
        })
    }

    #[test]
    fn a_module_is_placed_by_where_it_loaded_from() {
        let mods = collected(&[
            "C:\\Games\\MX Bikes\\mxbikes.exe",
            "C:\\Windows\\System32\\kernel32.dll",
            "C:\\Users\\rider\\AppData\\Local\\mxbapp\\frostmod\\frostmod.dll",
            "C:\\Users\\rider\\Downloads\\something.dll",
        ]);
        let by_name = |n: &str| mods.iter().find(|m| m.name == n).unwrap().origin;
        assert_eq!(by_name("mxbikes.exe"), Origin::Game);
        assert_eq!(by_name("kernel32.dll"), Origin::System);
        assert_eq!(by_name("frostmod.dll"), Origin::App);
        assert_eq!(by_name("something.dll"), Origin::Other);
    }

    /// The privacy line the whole feature stands on: what leaves is a file name, a location
    /// word and a hash. Anything that changes this test is changing what the app says about
    /// the person running it.
    #[test]
    fn a_report_carries_no_paths() {
        let mods = collected(&["C:\\Users\\rider\\Secret Work Tools\\vpnhook.dll"]);
        let json = serde_json::to_string(&mods).unwrap();
        assert!(!json.to_lowercase().contains("secret"), "{json}");
        assert!(!json.to_lowercase().contains("rider"), "{json}");
        assert!(!json.contains("users"), "{json}");
        assert!(json.contains("vpnhook.dll"), "{json}");
    }

    #[test]
    fn system_libraries_are_not_read_at_all() {
        let mods = collected(&["C:\\Windows\\System32\\kernel32.dll", "C:\\x\\other.dll"]);
        let sys = mods.iter().find(|m| m.name == "kernel32.dll").unwrap();
        let other = mods.iter().find(|m| m.name == "other.dll").unwrap();
        assert_eq!(sys.sha256, "");
        assert_eq!(sys.size, 0);
        assert_eq!(sys.mtime, 0);
        // Not `unsigned`: nothing looked. There are hundreds of these and they are the same
        // on every machine, and calling them unsigned would be a claim we never checked.
        assert_eq!(sys.facts.trust, crate::fileinfo::Trust::Unchecked);
        assert_ne!(other.sha256, "");
        assert_ne!(other.size, 0);
        assert_eq!(other.facts.trust, crate::fileinfo::Trust::Unsigned);
    }

    /// The detail is flattened onto the module, so a report is a list of flat objects rather
    /// than one with a nested `facts` nobody reading the payload would expect.
    #[test]
    fn what_a_file_says_about_itself_rides_alongside_it() {
        let owned = vec!["C:\\x\\overlay.dll".to_string()];
        let mods = collect_with(&owned, &roots(), &|_| FileRead {
            sha256: "a".repeat(64),
            size: 2_400_000,
            mtime: 1_750_000_000,
            facts: FileFacts {
                trust: crate::fileinfo::Trust::Signed,
                details: crate::fileinfo::Details {
                    publisher: "NVIDIA Corporation".into(),
                    company: "NVIDIA Corporation".into(),
                    product: "NVIDIA Share".into(),
                    description: "NVIDIA Share overlay".into(),
                },
            },
        });
        let json = serde_json::to_value(&mods).unwrap();
        let one = &json[0];
        assert_eq!(one["trust"], "signed");
        assert_eq!(one["publisher"], "NVIDIA Corporation");
        assert_eq!(one["product"], "NVIDIA Share");
        assert_eq!(one["size"], 2_400_000);
        assert_eq!(one["mtime"], 1_750_000_000);
        assert!(one.get("facts").is_none(), "flattened, not nested: {json}");
    }

    /// Empty strings and zeros are left off the wire entirely, so a file nothing could be
    /// read off costs three keys rather than nine on every report from every install.
    #[test]
    fn nothing_known_about_a_file_is_nothing_sent() {
        let owned = vec!["C:\\x\\locked.dll".to_string()];
        let mods = collect_with(&owned, &roots(), &|_| FileRead::default());
        let json = serde_json::to_value(&mods).unwrap();
        let one = json[0].as_object().unwrap();
        assert_eq!(one.keys().len(), 3, "{json}");
        assert_eq!(one["trust"], "unchecked");
    }

    #[test]
    fn a_folder_that_merely_starts_the_same_is_not_the_game() {
        let mods = collected(&["C:\\Games\\MX Bikes Cheats\\loader.dll"]);
        assert_eq!(mods[0].origin, Origin::Other);
    }

    #[test]
    fn a_windows_folder_somebody_made_up_is_not_the_system_folder() {
        let mods = collected(&["C:\\loader\\windows\\system32\\hook.dll"]);
        assert_eq!(mods[0].origin, Origin::Other);
        // And a real one still is, including inside a Wine prefix.
        let real = collected(&[
            "C:\\Windows\\SysWOW64\\user32.dll",
            "/home/rider/.steam/pfx/drive_c/windows/system32/ntdll.dll",
        ]);
        assert!(real.iter().all(|m| m.origin == Origin::System), "{real:?}");
    }

    #[test]
    fn a_proton_runtime_library_reads_as_the_system_one() {
        // Under Proton the game's own kernel32 genuinely comes out of the runtime, which is
        // neither the system folder nor the game folder. Without this every Linux session
        // would report a hundred unaccounted-for files.
        let mods = collected(&[
            "/home/rider/.steam/steam/steamapps/common/Proton - Experimental/files/lib/wine/x86_64-windows/kernel32.dll",
        ]);
        assert_eq!(mods[0].origin, Origin::System);
    }

    #[test]
    fn the_same_file_twice_is_reported_once() {
        let mods = collected(&["C:\\x\\a.dll", "C:\\x\\a.dll"]);
        assert_eq!(mods.len(), 1);
    }

    #[test]
    fn the_digest_only_changes_when_the_answer_does() {
        let a = collected(&["C:\\x\\a.dll", "C:\\x\\b.dll"]);
        let b = collected(&["C:\\x\\b.dll", "C:\\x\\a.dll"]);
        let quiet = |mods: &[Module]| digest(true, mods, &[], Threads::default(), &[]);
        assert_eq!(quiet(&a), quiet(&b), "order must not matter");
        let c = collected(&["C:\\x\\a.dll"]);
        assert_ne!(quiet(&a), quiet(&c));
        // "Could not look" is a different answer from "looked and found nothing".
        assert_ne!(
            digest(true, &[], &[], Threads::default(), &[]),
            digest(false, &[], &[], Threads::default(), &[])
        );
    }

    fn a_region() -> Region {
        Region {
            kind: "private",
            size: 0x20_000,
            protect: "rwx".into(),
            thread: true,
            image: true,
            name: "kaizo.dll".into(),
            pdb: "kaizo.pdb".into(),
            timestamp: 0x6512_3456,
            sha256: "b".repeat(64),
        }
    }

    #[test]
    fn a_region_appearing_is_something_new_to_say() {
        let mods = collected(&["C:\\x\\a.dll"]);
        let clean = digest(true, &mods, &[], Threads::default(), &[]);
        assert_ne!(clean, digest(true, &mods, &[a_region()], Threads::default(), &[]));
    }

    #[test]
    fn a_thread_from_nowhere_is_something_new_to_say() {
        let mods = collected(&["C:\\x\\a.dll"]);
        let clean = digest(true, &mods, &[], Threads::default(), &[]);
        let foreign = Threads { total: 40, foreign: 1, breakpoints: 0 };
        assert_ne!(clean, digest(true, &mods, &[], foreign, &[]));
        // The total moves every time the game spawns a worker. On its own it is not news.
        let busier = Threads { total: 41, foreign: 0, breakpoints: 0 };
        assert_eq!(clean, digest(true, &mods, &[], busier, &[]));
    }

    /// The wire shape the control plane parses. Its validator refuses anything else outright,
    /// so a rename on either side is a silent stop rather than an error anybody sees.
    #[test]
    fn the_payload_is_the_shape_the_other_end_reads() {
        let mods = collected(&["C:\\Games\\MX Bikes\\mxbikes.exe", "C:\\Windows\\System32\\a.dll"]);
        let json = serde_json::to_string(&Payload {
            app_version: "0.13.1",
            available: true,
            guid: "FF0110000108D7CFE3",
            modules: &mods,
            regions: &[],
            threads: Threads::default(),
            files: &[],
        })
        .unwrap();
        assert!(json.contains(r#""appVersion":"0.13.1""#), "{json}");
        assert!(json.contains(r#""available":true"#), "{json}");
        assert!(json.contains(r#""guid":"FF0110000108D7CFE3""#), "{json}");
        assert!(json.contains(r#""origin":"game""#), "{json}");
        assert!(json.contains(r#""origin":"system""#), "{json}");
        // Absent rather than empty, which is what the parser expects of an unhashed file.
        // `trust` is the exception and is always sent: "we did not look" is an answer, and
        // leaving it off would have the other end infer it rather than be told.
        assert!(
            json.contains(r#"{"name":"a.dll","origin":"system","trust":"unchecked"}"#),
            "{json}"
        );

        // Unknown until sign-in, and then simply absent rather than an empty string.
        let anon = serde_json::to_string(&Payload {
            app_version: "0.13.1",
            available: true,
            guid: "",
            modules: &mods,
            regions: &[],
            threads: Threads::default(),
            files: &[],
        })
        .unwrap();
        assert!(!anon.contains("guid"), "{anon}");
    }

    #[test]
    fn a_mapping_that_is_not_a_file_name_is_dropped() {
        // Linux hands back mappings that are not always plain files; the control plane takes
        // file names and nothing else, so anything odd is dropped here.
        assert!(collected(&["/memfd:wayland-shm (deleted)"]).is_empty());
        assert!(collected(&["C:\\x\\"]).is_empty());
    }

    use crate::gameproc::{ExecRegion, MEM_IMAGE, MEM_PRIVATE};

    fn region_at(base: u64, size: u64, kind: u32) -> ExecRegion {
        ExecRegion { base, size, protect: 0x40, kind, thread: false }
    }

    #[test]
    fn a_region_is_marked_when_a_thread_from_nowhere_starts_inside_it() {
        let regions = [region_at(0x10_000, 0x1000, MEM_PRIVATE)];
        let ranked = rank_regions(&regions, &[0x10_800]);
        assert!(ranked[0].thread);
        let elsewhere = rank_regions(&regions, &[0x11_000]);
        assert!(!elsewhere[0].thread, "one past the end is not inside");
    }

    #[test]
    fn an_unlisted_image_is_read_before_anything_else() {
        // A big private region and a small one mapped as an image the loader never listed.
        // The second is the loud one however small it is: nothing legitimate is an image
        // that the loader's own list does not mention.
        let regions = [
            region_at(0x10_000, 0x40_000, MEM_PRIVATE),
            region_at(0x90_000, 0x1000, MEM_IMAGE),
        ];
        let ranked = rank_regions(&regions, &[]);
        assert_eq!(ranked[0].base, 0x90_000);
    }

    #[test]
    fn a_region_running_a_thread_is_read_before_a_bigger_quiet_one() {
        let regions = [
            region_at(0x10_000, 0x40_000, MEM_PRIVATE),
            region_at(0x90_000, 0x1000, MEM_PRIVATE),
        ];
        let ranked = rank_regions(&regions, &[0x90_100]);
        assert_eq!(ranked[0].base, 0x90_000);
        assert_eq!(ranked[1].base, 0x10_000, "then the largest of what is left");
    }

    #[test]
    fn ordinary_executable_memory_is_not_carried() {
        // A graphics driver building code at runtime looks exactly like this, and there are
        // several of them in any game process. Counted, never enumerated.
        let jit = region_at(0x10_000, 0x8000, MEM_PRIVATE);
        assert!(!worth_reporting(&jit, false));

        assert!(worth_reporting(&jit, true), "a PE header there is not ordinary");
        assert!(
            worth_reporting(&region_at(0x10_000, 0x8000, MEM_IMAGE), false),
            "an image the loader does not list is not ordinary"
        );
        assert!(
            worth_reporting(&ExecRegion { thread: true, ..jit }, false),
            "code from nowhere that is running is not ordinary"
        );
    }

    #[test]
    fn the_plugins_folder_says_which_of_its_files_the_game_has_loaded() {
        let dir = std::env::temp_dir().join(format!("mxb-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("frostmod.dlo"), b"a").unwrap();
        std::fs::write(dir.join("something.dlo"), b"bb").unwrap();

        let files = plugin_files_with(&dir, &["frostmod.dlo".to_string()], &|path| FileRead {
            sha256: format!("{:0>64}", path.to_string_lossy().len()),
            size: 2,
            mtime: 7,
            facts: FileFacts::default(),
        });

        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["frostmod.dlo", "something.dlo"]);
        assert!(files[0].loaded);
        // The one the game has not loaded is the whole reason this folder is read: it is in
        // no module list, so nothing else in the report mentions it at all.
        assert!(!files[1].loaded);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_with_no_plugins_in_it_is_no_rows() {
        assert!(plugin_files(&std::path::PathBuf::from("/nowhere/at/all"), &[]).is_empty());
    }

    #[test]
    fn the_new_half_of_the_payload_is_the_shape_the_other_end_reads() {
        let json = serde_json::to_string(&Payload {
            app_version: "0.13.1",
            available: true,
            guid: "",
            modules: &[],
            regions: &[a_region()],
            threads: Threads { total: 44, foreign: 1, breakpoints: 2 },
            files: &[DiskFile {
                name: "frostmod.dlo".into(),
                sha256: "c".repeat(64),
                size: 12,
                mtime: 99,
                loaded: true,
                facts: FileFacts::default(),
            }],
        })
        .unwrap();
        assert!(json.contains(r#""kind":"private""#), "{json}");
        assert!(json.contains(r#""protect":"rwx""#), "{json}");
        assert!(json.contains(r#""pdb":"kaizo.pdb""#), "{json}");
        assert!(json.contains(r#""thread":true"#), "{json}");
        assert!(json.contains(r#""threads":{"total":44,"foreign":1,"breakpoints":2}"#), "{json}");
        assert!(json.contains(r#""loaded":true"#), "{json}");
    }

    /// An app that looked and found nothing must not read like one too old to have looked.
    #[test]
    fn a_clean_machine_still_says_so() {
        let json = serde_json::to_string(&Payload {
            app_version: "0.13.1",
            available: true,
            guid: "",
            modules: &[],
            regions: &[],
            threads: Threads::default(),
            files: &[],
        })
        .unwrap();
        assert!(json.contains(r#""regions":[]"#), "{json}");
        assert!(json.contains(r#""threads":{"total":0,"foreign":0,"breakpoints":0}"#), "{json}");
        assert!(json.contains(r#""files":[]"#), "{json}");
    }
}
