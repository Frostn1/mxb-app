//! Where the logs are, and how to get them off the machine they're on.
//!
//! Three sets matter when something has gone wrong, and they live in different places:
//! MXB App's own file log — written by `tauri_plugin_log` into the OS log dir
//! (`%LOCALAPPDATA%\com.frost.mxbikes\logs` on Windows) — the game's `log.txt`, which
//! PiBoSo titles write beside the executable or into the user folder depending on how the
//! game was started, and whatever FrostMod leaves in the folder we install and run it
//! from. Which of the three matters depends entirely on what broke, and the player
//! reporting it is the last person who can tell.
//!
//! None of them is somewhere a player finds on their own, and "send me your logs" is the
//! first thing every bug report asks for. So this module answers three questions for
//! Settings: where they are, what's actually in there, and how to hand the lot over as
//! one file.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::config::AppConfig;

/// Skip anything absurd rather than build a 400 MB attachment nobody can upload. Log
/// files are text and rotate; one this big is a runaway, and the tail of it would be no
/// more diagnostic than the tail of a smaller one.
const MAX_EXPORT_FILE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    /// Last-modified in Unix ms, or `None` when the platform wouldn't say. The UI leads
    /// with this — "how old is the newest one" is what says whether a log covers the run
    /// the player is complaining about.
    pub modified: Option<u64>,
}

/// One place logs come from, whether or not it currently holds any.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogGroup {
    /// The folder the files below came from — or, when there are none, the folder they'd
    /// be in. Always populated: a path the player can go and look at beats "not found".
    pub dir: String,
    /// Whether `dir` is actually there. A missing folder and an empty one are different
    /// problems (nothing has run yet vs. nothing was written), and only one is worrying.
    pub exists: bool,
    /// Newest first.
    pub files: Vec<LogFile>,
}

impl LogGroup {
    fn missing(dir: PathBuf) -> Self {
        LogGroup { dir: dir.to_string_lossy().into_owned(), exists: false, files: Vec::new() }
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsInfo {
    /// MXB App's own log dir.
    pub app: LogGroup,
    /// The managed FrostMod folder — where we install it and where it runs from.
    pub frostmod: LogGroup,
    /// The active game's logs.
    pub game: LogGroup,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    /// How many log files made it in. Zero is possible and still worth writing — the
    /// summary alone says which folders were looked in and found empty.
    pub files: usize,
    pub bytes: u64,
}

/// Whether a file in a *game* folder is a log.
///
/// The game folder is the player's, full of things that are none of our business, so this
/// matches by name rather than sweeping the folder up. `log.txt` is what PiBoSo titles
/// write; the rest of the patterns cost nothing and cover a rotated or crash-time sibling
/// without needing to know its exact name in advance.
fn is_game_log(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".log")
        || (lower.starts_with("log") && lower.ends_with(".txt"))
        || (lower.starts_with("crash") && (lower.ends_with(".txt") || lower.ends_with(".log")))
}

/// Whether a file in the *FrostMod* folder is a log.
///
/// Inverted against [`is_game_log`], and deliberately: this folder is the app's own —
/// we create it, install into it, and run FrostMod with it as the working directory — so
/// its non-log contents are a known, short list, while what FrostMod names its log is
/// decided in another repo and could change with any release. Excluding what we put there
/// therefore catches a log we've never seen the name of; matching on `log*.txt` would
/// quietly miss it.
///
/// The server filter is excluded with the binaries: it's a settings file, and this button
/// promises not to collect those.
fn is_frostmod_log(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let ours = lower.ends_with(".exe")
        || lower.ends_with(".dll")
        || lower == "version.txt"
        || lower.ends_with(".yaml")
        // The last command we left for FrostMod (Linux, where a file is the only way to
        // reach it). Ours by the same rule as the binaries — and what FrostMod *did* with
        // it is in its log, which is the half worth collecting.
        || lower == "frostmod_cmd.json"
        // A binary moved aside mid-update because the game still had it mapped —
        // `frostmod.dll.in-use-1723…`, swept on the next start.
        || lower.contains(".in-use-");
    !ours
}

fn describe(path: &Path) -> Option<LogFile> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    Some(LogFile {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        bytes: meta.len(),
        modified,
    })
}

/// Everything in `dir`, newest first. For folders that are ours alone — the app log dir,
/// where every file is a log whatever it's called or however the plugin rotated it.
fn scan_all(dir: &Path) -> LogGroup {
    scan(dir, |_| true)
}

fn scan(dir: &Path, keep: impl Fn(&str) -> bool) -> LogGroup {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return LogGroup::missing(dir.to_path_buf());
    };
    let mut files: Vec<LogFile> = rd
        .flatten()
        .filter(|e| keep(&e.file_name().to_string_lossy()))
        .filter_map(|e| describe(&e.path()))
        .collect();
    // Newest first: the run that just went wrong is the one anyone opens.
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    LogGroup { dir: dir.to_string_lossy().into_owned(), exists: true, files }
}

/// The folders the active game could be logging into, best guess first.
///
/// The install dir leads because that is where the executable writes: `log.txt` lands
/// beside `mxbikes.exe`. The user folder follows because a Steam install can be
/// read-only-ish in practice (and a relocated mods tree moves what we know about), and
/// looking in both costs one `read_dir`.
pub fn game_dirs(cfg: &AppConfig) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let install = cfg.install_dir();
    if !install.trim().is_empty() {
        dirs.push(PathBuf::from(install.trim()));
    }
    let mods_path = cfg.mods_path.trim();
    if !mods_path.is_empty() {
        let base = Path::new(mods_path);
        dirs.push(base.to_path_buf());
        // `mods_path` may be the mods tree itself rather than the folder above it, in
        // which case the user folder — where the game writes — is its parent.
        if crate::library::mods_root(mods_path) == base {
            if let Some(parent) = base.parent().filter(|p| p.parent().is_some()) {
                dirs.push(parent.to_path_buf());
            }
        }
    }
    dirs.dedup();
    dirs
}

/// Pick the game folder to report on: the first candidate that actually holds logs,
/// falling back to the first that exists and finally to the first guess, so the UI always
/// has a path to show and a folder to open.
fn game_group(cfg: &AppConfig) -> LogGroup {
    let dirs = game_dirs(cfg);
    let Some(first) = dirs.first().cloned() else {
        // No folders configured at all — first run, or a game that was never set up.
        return LogGroup::missing(PathBuf::new());
    };
    let mut fallback: Option<LogGroup> = None;
    for dir in dirs {
        let group = scan(&dir, is_game_log);
        if !group.files.is_empty() {
            return group;
        }
        if group.exists && fallback.is_none() {
            fallback = Some(group);
        }
    }
    fallback.unwrap_or_else(|| LogGroup::missing(first))
}

pub fn info(app_log_dir: &Path, frostmod_dir: &Path, cfg: &AppConfig) -> LogsInfo {
    LogsInfo {
        app: scan_all(app_log_dir),
        frostmod: scan(frostmod_dir, is_frostmod_log),
        game: game_group(cfg),
    }
}

/// Open the folder a group lives in, with its newest file selected where the platform can
/// do that. Selecting matters: an app log dir holds several files and the one worth
/// opening is not the one sorted first alphabetically.
pub fn open_location(group: &LogGroup) -> anyhow::Result<()> {
    match group.files.first() {
        Some(newest) => crate::library::reveal_in_explorer(&newest.path),
        None => crate::library::open_folder(&group.dir),
    }
}

/// Write every group into one zip at `dest`, a folder each.
///
/// Deliberately just the logs and a summary of where they came from — not the config
/// file, which holds session cookies and shop credentials. The whole point of this button
/// is that the archive is safe to drop into a public Discord thread.
pub fn export(dest: &Path, info: &LogsInfo, summary: &str) -> anyhow::Result<ExportResult> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    // Deflate rather than the `Stored` a mod bundle uses: this payload is plain text,
    // which is the one thing compression is dramatic on.
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("summary.txt", opts)?;
    std::io::Write::write_all(&mut zip, summary.as_bytes())?;

    let mut written = 0usize;
    let mut bytes = 0u64;
    for (folder, group) in groups(info) {
        for log in &group.files {
            if log.bytes > MAX_EXPORT_FILE {
                continue;
            }
            // A file that vanished or turned unreadable between the scan and now is not a
            // reason to fail the export — the rest of the logs are still worth having.
            let Ok(data) = std::fs::read(&log.path) else { continue };
            zip.start_file(format!("{folder}/{}", log.name), opts)?;
            std::io::Write::write_all(&mut zip, &data)?;
            written += 1;
            bytes += data.len() as u64;
        }
    }
    zip.finish()?;

    Ok(ExportResult { path: dest.to_string_lossy().into_owned(), files: written, bytes })
}

/// The three groups in the order they're written, paired with the folder each takes
/// inside an exported zip. Named once so the archive's layout and the summary listing it
/// can't drift apart.
fn groups(info: &LogsInfo) -> [(&'static str, &LogGroup); 3] {
    [("app", &info.app), ("frostmod", &info.frostmod), ("game", &info.game)]
}

/// The plain-text header that goes in beside the logs: what was included, and the handful
/// of facts every report starts by asking for.
pub fn summary(version: &str, cfg: &AppConfig, info: &LogsInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("MXB App {version}\n"));
    out.push_str(&format!("os: {} ({})\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("game: {}\n", cfg.game().display));
    out.push_str(&format!("mods folder: {}\n", show(&cfg.mods_path)));
    out.push_str(&format!("install folder: {}\n", show(&cfg.install_dir())));
    out.push_str(&format!("profiles folder: {}\n", cfg.profiles_dir().display()));
    for (label, group) in groups(info) {
        out.push_str(&format!(
            "\n{label} logs: {}{}\n",
            if group.dir.is_empty() { "(unknown)" } else { &group.dir },
            if group.exists { "" } else { " (folder not found)" },
        ));
        if group.files.is_empty() {
            out.push_str("  (none)\n");
        } else {
            out.push_str(&format!(
                "  {} file(s), {} bytes\n",
                group.files.len(),
                group.total_bytes()
            ));
        }
        for f in &group.files {
            out.push_str(&format!("  {} ({} bytes)\n", f.name, f.bytes));
        }
    }
    out
}

fn show(path: &str) -> &str {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        "(not set)"
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("frost-logs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn matches_the_game_log_and_leaves_the_rest_alone() {
        assert!(is_game_log("log.txt"));
        assert!(is_game_log("LOG.TXT"));
        assert!(is_game_log("log_2026-08-11.txt"));
        assert!(is_game_log("mxbikes.log"));
        assert!(is_game_log("crash.txt"));
        // The game folder is the player's; nothing else in it is ours to collect.
        assert!(!is_game_log("mxbikes.ini"));
        assert!(!is_game_log("readme.txt"));
        assert!(!is_game_log("rider.pkz"));
    }

    #[test]
    fn keeps_frostmods_log_and_drops_what_we_put_beside_it() {
        // Whatever FrostMod writes, under any name — the filter can't be told the name in
        // advance, so it works by knowing what *isn't* a log.
        assert!(is_frostmod_log("frostmod.log"));
        assert!(is_frostmod_log("log.txt"));
        assert!(is_frostmod_log("frostmod-2026-08-11.txt"));
        // The install: our binaries, our version marker, our server filter, and a binary
        // parked mid-update because the game still had it mapped.
        assert!(!is_frostmod_log("frostmod.exe"));
        assert!(!is_frostmod_log("frostmod.dll"));
        assert!(!is_frostmod_log("version.txt"));
        assert!(!is_frostmod_log("frostmod_serverfilter.yaml"));
        assert!(!is_frostmod_log("frostmod.dll.in-use-1723500000"));
        // The command we last left for it. Ours, not a log — FrostMod's own log says what
        // it made of it.
        assert!(!is_frostmod_log("frostmod_cmd.json"));
    }

    #[test]
    fn scans_newest_first() {
        let dir = tmpdir("order");
        write(&dir, "old.log", "one");
        write(&dir, "new.log", "two");
        // mtime granularity is coarse enough that two writes in a row can tie, so set the
        // order explicitly rather than racing the filesystem clock.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        std::fs::File::options().write(true).open(dir.join("old.log")).unwrap().set_modified(old).unwrap();

        let group = scan_all(&dir);
        assert!(group.exists);
        let names: Vec<&str> = group.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["new.log", "old.log"]);
        assert_eq!(group.total_bytes(), 6);
    }

    #[test]
    fn a_missing_folder_still_reports_where_it_looked() {
        let group = scan_all(Path::new("/definitely/not/here/logs"));
        assert!(!group.exists);
        assert!(group.files.is_empty());
        assert!(group.dir.ends_with("logs"));
    }

    #[test]
    fn game_folders_are_tried_install_first_then_the_user_folder() {
        let root = tmpdir("dirs");
        let install = root.join("steamapps");
        let user = root.join("user");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&user).unwrap();

        let cfg = AppConfig {
            game_path: install.to_string_lossy().into_owned(),
            mods_path: user.to_string_lossy().into_owned(),
            ..Default::default()
        };

        // Nothing anywhere: we still name the install folder rather than going blank.
        let group = game_group(&cfg);
        assert_eq!(group.dir, install.to_string_lossy());
        assert!(group.files.is_empty());

        // A log in the user folder wins over an install folder that has none.
        write(&user, "log.txt", "hello");
        let group = game_group(&cfg);
        assert_eq!(group.dir, user.to_string_lossy());
        assert_eq!(group.files.len(), 1);

        // …and the install folder wins as soon as it has one of its own.
        write(&install, "log.txt", "beside the exe");
        let group = game_group(&cfg);
        assert_eq!(group.dir, install.to_string_lossy());
    }

    #[test]
    fn export_holds_every_group_and_a_summary() {
        let root = tmpdir("export");
        let app_dir = root.join("applogs");
        let frostmod_dir = root.join("frostmod");
        let game_dir = root.join("game");
        for d in [&app_dir, &frostmod_dir, &game_dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        write(&app_dir, "frost.log", "app side");
        write(&frostmod_dir, "frostmod.log", "loader side");
        write(&frostmod_dir, "frostmod.exe", "not a log");
        write(&game_dir, "log.txt", "game side");
        write(&game_dir, "mxbikes.ini", "not a log");

        let cfg = AppConfig {
            game_path: game_dir.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let info = info(&app_dir, &frostmod_dir, &cfg);
        assert_eq!(info.app.files.len(), 1);
        assert_eq!(info.frostmod.files.len(), 1);
        assert_eq!(info.game.files.len(), 1);

        let dest = root.join("out").join("logs.zip");
        let result = export(&dest, &info, &summary("9.9.9", &cfg, &info)).unwrap();
        assert_eq!(result.files, 3);

        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        assert!(names.contains(&"summary.txt".to_string()));
        assert!(names.contains(&"app/frost.log".to_string()));
        assert!(names.contains(&"frostmod/frostmod.log".to_string()));
        assert!(names.contains(&"game/log.txt".to_string()));
        // Neither folder's non-log contents came along.
        assert!(!names.iter().any(|n| n.ends_with("mxbikes.ini")));
        assert!(!names.iter().any(|n| n.ends_with("frostmod.exe")));

        let mut body = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("game/log.txt").unwrap(), &mut body).unwrap();
        assert_eq!(body, "game side");
    }
}
