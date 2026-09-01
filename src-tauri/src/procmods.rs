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

    let digest = digest(available, &modules);
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
        if let Err(e) = send(&token, &version, available, &guid, &modules).await {
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

/// A stable fingerprint of one answer, so an unchanged session sends nothing.
///
/// FNV-1a over the sorted list. Not a cryptographic question: the only thing asked of it is
/// whether this pass differs from the last one.
fn digest(available: bool, modules: &[Module]) -> u64 {
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
    hash
}

async fn send(
    token: &str,
    app_version: &str,
    available: bool,
    guid: &str,
    modules: &[Module],
) -> anyhow::Result<()> {
    let res = reqwest::Client::new()
        .put(format!("{}/v1/diagnostics", crate::paintsync::control_plane()))
        .bearer_auth(token)
        .json(&Payload { app_version, available, guid, modules })
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
        assert_eq!(digest(true, &a), digest(true, &b), "order must not matter");
        let c = collected(&["C:\\x\\a.dll"]);
        assert_ne!(digest(true, &a), digest(true, &c));
        // "Could not look" is a different answer from "looked and found nothing".
        assert_ne!(digest(true, &[]), digest(false, &[]));
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
}
