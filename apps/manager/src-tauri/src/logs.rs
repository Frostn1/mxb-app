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
//! first thing every bug report asks for. So this module answers four questions for
//! Settings: where they are, what's actually in there, how to hand the lot over as one
//! file, and — since the file still has to get to whoever asked — how to turn that file
//! into a link, through the same upload a shared track goes out on.

use serde::Serialize;
use std::path::{Path, PathBuf};

pub use mxb_core::logshare::{
    is_game_log, is_plugin_log, open_location, scan, scan_all, ExportResult, LogGroup, ShareResult,
};

use crate::config::AppConfig;

/// MXBMRP3's crash folder can collect a log per crash for months; the newest are what matter.
const MAX_MXBMRP3_FILES: usize = 10;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsInfo {
    /// MXB App's own log dir.
    pub app: LogGroup,
    /// The managed FrostMod folder — where we install it and where it runs from.
    pub frostmod: LogGroup,
    /// The active game's logs.
    pub game: LogGroup,
    /// The locked-content DLL's run folder: its log and the manifest it read.
    pub secure: LogGroup,
    /// The game's user folder (`Documents\PiBoSo\<game>`), when the game's logs above came
    /// from the install folder — both are looked in.
    pub game_user: LogGroup,
    /// MXBMRP3's crash logs (`<user folder>\mxbmrp3\crashes`).
    pub mxbmrp3: LogGroup,
    /// The game's `plugins` folder. FrostMod and the locked-content DLL write their logs
    /// beside the plugin when it runs from there rather than from the managed folder.
    pub plugins: LogGroup,
    /// Where the game really is, and whether the game will load plugins from there.
    pub install: InstallFacts,
}

/// The install folder as resolved on disk, for the summary: what a report most needs to
/// confirm is that the game is where the app thinks it is, and that FrostMod is in the
/// `plugins` folder the game actually loads.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallFacts {
    /// The install folder the app resolved (saved, or found by Steam detection). Empty when
    /// neither holds the executable.
    pub dir: String,
    /// What the config says, which differs from `dir` when it is wrong.
    pub saved: String,
    /// The executable's name, and whether it is in `dir`.
    pub exe: String,
    pub exe_present: bool,
    /// `<dir>\plugins`, where the game loads `.dlo` plugins from.
    pub plugins_dir: String,
    pub frostmod_plugin: bool,
    pub frostmod_pointer: bool,
}

impl InstallFacts {
    fn of(cfg: &AppConfig, install: Option<&Path>) -> Self {
        let game = cfg.game();
        let Some(dir) = install else {
            return InstallFacts {
                saved: cfg.game_path.trim().to_string(),
                exe: game.exe.to_string(),
                ..Default::default()
            };
        };
        let plugins = crate::library::resolve_child(dir, "plugins");
        InstallFacts {
            dir: dir.to_string_lossy().into_owned(),
            saved: cfg.game_path.trim().to_string(),
            exe: game.exe.to_string(),
            exe_present: crate::gamefolders::has_exe(dir, game),
            frostmod_plugin: plugins.join("frostmod.dlo").is_file(),
            frostmod_pointer: plugins.join("frostmod.dir").is_file(),
            plugins_dir: plugins.to_string_lossy().into_owned(),
        }
    }
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
        // The last command we left for FrostMod (Linux and macOS, where a file is the only
        // way to reach it). Ours by the same rule as the binaries — and what FrostMod *did*
        // with it is in its log, which is the half worth collecting.
        || lower == "frostmod_cmd.json"
        // The app's record of where it put the plugin.
        || lower == "plugin_places.txt"
        // A binary moved aside mid-update because the game still had it mapped —
        // `frostmod.dll.in-use-1723…`, swept on the next start.
        || lower.contains(".in-use-");
    !ours
}

/// Whether a file in the locked-content folder goes in the bundle: the DLL's log, and the
/// manifest (paths only — keys stay in their `.mxbkey`), never the DLL itself.
fn is_secure_log(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "mxbsecure.log" || lower == "manifest.tsv"
}


/// The game's user folder: `mods_path`, or the folder above it when that is the mods tree.
fn user_folder(cfg: &AppConfig) -> Option<PathBuf> {
    let mods_path = cfg.mods_path.trim();
    if mods_path.is_empty() {
        return None;
    }
    let base = Path::new(mods_path);
    if crate::library::mods_root(mods_path) == base {
        if let Some(parent) = base.parent().filter(|p| p.parent().is_some()) {
            return Some(parent.to_path_buf());
        }
    }
    Some(base.to_path_buf())
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
///
/// With the real install known (`install`, the folder holding the executable), that folder is
/// the answer: it is where `log.txt`, `e3dlog.txt`, `sllog.txt` and the crash dumps are, and
/// the user folder gets a group of its own.
///
/// Without it, the folders FrostMod was installed into (`places`) are tried after the
/// configured ones: on a PC where the saved path is wrong and Steam detection misses the
/// library (`G:\SteamLibrary`), they are the record of where the game really is.
fn game_group(cfg: &AppConfig, install: Option<&Path>, places: &[PathBuf]) -> LogGroup {
    if let Some(dir) = install {
        return scan(dir, is_game_log);
    }
    let mut dirs = game_dirs(cfg);
    dirs.extend(places.iter().cloned());
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

/// The user folder's own logs, unless the game group above already is that folder.
fn game_user_group(cfg: &AppConfig, game: &LogGroup) -> LogGroup {
    let Some(user) = user_folder(cfg) else {
        return LogGroup::missing(PathBuf::new());
    };
    if crate::docsdir::same_folder(&user, Path::new(&game.dir)) {
        return LogGroup { dir: user.to_string_lossy().into_owned(), exists: game.exists, files: Vec::new() };
    }
    scan(&user, is_game_log)
}

/// MXBMRP3's crash logs, newest first and only the newest few.
fn mxbmrp3_group(cfg: &AppConfig) -> LogGroup {
    let Some(user) = user_folder(cfg) else {
        return LogGroup::missing(PathBuf::new());
    };
    let dir = crate::library::resolve_child(&crate::library::resolve_child(&user, "mxbmrp3"), "crashes");
    let mut group = scan_all(&dir);
    group.files.truncate(MAX_MXBMRP3_FILES);
    group
}

/// The `plugins` folder of the install, or else of the first game folder FrostMod was put
/// into that holds logs.
fn plugins_group(install: Option<&Path>, places: &[PathBuf]) -> LogGroup {
    let dirs: Vec<PathBuf> = install
        .map(Path::to_path_buf)
        .into_iter()
        .chain(places.iter().cloned())
        .map(|d| crate::library::resolve_child(&d, "plugins"))
        .collect();
    let Some(first) = dirs.first().cloned() else {
        return LogGroup::missing(PathBuf::new());
    };
    let mut fallback: Option<LogGroup> = None;
    for dir in dirs {
        let group = scan(&dir, is_plugin_log);
        if !group.files.is_empty() {
            return group;
        }
        if group.exists && fallback.is_none() {
            fallback = Some(group);
        }
    }
    fallback.unwrap_or_else(|| LogGroup::missing(first))
}

pub fn info(app_log_dir: &Path, frostmod_dir: &Path, secure_dir: &Path, cfg: &AppConfig) -> LogsInfo {
    let install = crate::gamefolders::real_install(cfg);
    info_with(app_log_dir, frostmod_dir, secure_dir, cfg, install.as_deref())
}

/// [`info`] with the install folder already resolved, so tests don't depend on what Steam
/// has installed on the machine running them.
fn info_with(
    app_log_dir: &Path,
    frostmod_dir: &Path,
    secure_dir: &Path,
    cfg: &AppConfig,
    install: Option<&Path>,
) -> LogsInfo {
    // Every game folder FrostMod was put into, from the app's own record of them.
    let places = crate::frostmod_manage::read_places(frostmod_dir);
    let game = game_group(cfg, install, &places);
    LogsInfo {
        plugins: plugins_group(install, &places),
        app: scan_all(app_log_dir),
        frostmod: scan(frostmod_dir, is_frostmod_log),
        game_user: game_user_group(cfg, &game),
        game,
        secure: scan(secure_dir, is_secure_log),
        mxbmrp3: mxbmrp3_group(cfg),
        install: InstallFacts::of(cfg, install),
    }
}

/// Write every group into one zip at `dest`, a folder each.
///
/// Deliberately just the logs and a summary of where they came from, not the config
/// file, which holds session cookies and shop credentials. The whole point of this button
/// is that the archive is safe to drop into a public Discord thread.
pub fn export(dest: &Path, info: &LogsInfo, summary: &str) -> anyhow::Result<ExportResult> {
    mxb_core::logshare::export(dest, &groups(info), summary)
}

/// The groups in the order they're written, paired with the folder each takes inside an
/// exported zip. Named once so the archive's layout and the summary listing it can't drift
/// apart.
fn groups(info: &LogsInfo) -> [(&'static str, &LogGroup); 7] {
    [
        ("app", &info.app),
        ("frostmod", &info.frostmod),
        ("game", &info.game),
        ("game-user", &info.game_user),
        ("mxbmrp3", &info.mxbmrp3),
        ("plugins", &info.plugins),
        ("secure", &info.secure),
    ]
}


/// Pack every log set exactly as [`export`] does, upload the zip, and hand back the link.
///
/// The save button asks for a folder and leaves the player to attach the file somewhere.
/// This is the same archive with the last step done for them: one link to paste into a
/// bug report, uploaded through the same host and the same slicing as a shared track
/// (`mxb_core::upload`). What goes in is unchanged, logs and the summary and never the
/// config file, because the link is public to anyone who has it.
pub async fn share(
    app: &tauri::AppHandle,
    info: &LogsInfo,
    summary: &str,
) -> anyhow::Result<ShareResult> {
    let owned = groups(info).iter().map(|(n, g)| (*n, (*g).clone())).collect();
    mxb_core::logshare::share(app, owned, summary, "mxb-app-logs.zip").await
}

/// The plain-text header that goes in beside the logs: what was included, and the handful
/// of facts every report starts by asking for.
pub fn summary(
    version: &str,
    frostmod_version: Option<&str>,
    cfg: &AppConfig,
    info: &LogsInfo,
    health: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("MXB App {version}\n"));
    out.push_str(&format!("os: {} ({})\n", std::env::consts::OS, std::env::consts::ARCH));
    // Which FrostMod is installed, if any. The loader's own log is in the zip beside this,
    // and "which build wrote it" is the first thing anyone reading that log has to ask —
    // `version.txt` itself stays out of the archive as one of the files we put there.
    out.push_str(&format!("frostmod: {}\n", frostmod_version.unwrap_or("(not installed)")));
    out.push_str(&format!("game: {}\n", cfg.game().display));
    out.push_str(&format!("mods folder: {}\n", show(&cfg.mods_path)));
    out.push_str(&format!(
        "mods root: {}\n",
        if cfg.mods_path.trim().is_empty() {
            "(not set)".to_string()
        } else {
            crate::library::mods_root(&cfg.mods_path).display().to_string()
        }
    ));
    out.push_str(&format!("profiles folder: {}\n", cfg.profiles_dir().display()));
    // The `reshade: …` and `tyres: …` lines from [`crate::health`] — setups the game crashes on,
    // so support sees them without having to ask.
    out.push_str(health);
    // Resolved on disk, not just read from the config: a saved folder without the exe in it is
    // exactly the mistake this line exists to show.
    let facts = &info.install;
    out.push_str(&format!(
        "install folder: {}\n",
        if facts.dir.is_empty() { "(not found)" } else { &facts.dir }
    ));
    if !crate::docsdir::same_folder(Path::new(&facts.saved), Path::new(&facts.dir)) {
        out.push_str(&format!("  saved in settings: {}\n", show(&facts.saved)));
    }
    out.push_str(&format!(
        "  {}: {}\n",
        facts.exe,
        if facts.exe_present { "present" } else { "MISSING" }
    ));
    if !facts.plugins_dir.is_empty() {
        out.push_str(&format!(
            "  plugins load from: {} (frostmod.dlo {}, frostmod.dir {})\n",
            facts.plugins_dir,
            if facts.frostmod_plugin { "present" } else { "absent" },
            if facts.frostmod_pointer { "present" } else { "absent" },
        ));
    }
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
        let group = game_group(&cfg, None, &[]);
        assert_eq!(group.dir, install.to_string_lossy());
        assert!(group.files.is_empty());

        // A log in the user folder wins over an install folder that has none.
        write(&user, "log.txt", "hello");
        let group = game_group(&cfg, None, &[]);
        assert_eq!(group.dir, user.to_string_lossy());
        assert_eq!(group.files.len(), 1);

        // …and the install folder wins as soon as it has one of its own.
        write(&install, "log.txt", "beside the exe");
        let group = game_group(&cfg, None, &[]);
        assert_eq!(group.dir, install.to_string_lossy());
    }

    /// A rider whose saved path is wrong and whose library Steam detection missed (another
    /// drive, `G:\SteamLibrary`): the folders FrostMod was installed into still lead to the
    /// game's `log.txt` and to the logs FrostMod wrote beside its plugin.
    #[test]
    fn logs_are_found_through_the_places_frostmod_was_installed() {
        let root = tmpdir("places");
        let frostmod_dir = root.join("frostmod");
        let library = root.join("G").join("SteamLibrary").join("steamapps").join("common").join("MX Bikes");
        let plugins = library.join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::create_dir_all(&frostmod_dir).unwrap();
        write(&library, "log.txt", "game");
        write(&plugins, "frostmod.dlo", "plugin");
        write(&plugins, "frostmod.dir", "C:\\pointer");
        write(&plugins, "frostmod_session.log", "plugin side");
        write(&plugins, "mxbsecure.log", "dll side");
        write(&plugins, "frostmod-crash-20261011-101010.json", "{}");
        write(&plugins, "someone_elses.dlo", "not ours");
        write(&plugins, "someone_elses.cfg", "settings");
        write(&frostmod_dir, "plugin_places.txt", &library.to_string_lossy());

        let cfg = AppConfig {
            game_path: root.join("wrong").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let info = info_with(&root.join("app"), &frostmod_dir, &root.join("secure"), &cfg, None);
        assert_eq!(info.game.dir, library.to_string_lossy());
        assert_eq!(info.game.files.len(), 1);
        let mut names: Vec<&str> = info.plugins.files.iter().map(|f| f.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["frostmod-crash-20261011-101010.json", "frostmod_session.log", "mxbsecure.log"]);

        let dest = root.join("out.zip");
        export(&dest, &info, "").unwrap();
        let zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let files: Vec<String> = zip.file_names().map(str::to_string).collect();
        assert!(files.contains(&"plugins/frostmod_session.log".to_string()), "{files:?}");
        assert!(files.contains(&"game/log.txt".to_string()), "{files:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_logs_and_frostmod_crash_reports_come_out_of_plugins() {
        assert!(is_plugin_log("frostmod_session.log"));
        assert!(is_plugin_log("mxbsecure.log"));
        assert!(is_plugin_log("FrostMod-Crash-1.json"));
        assert!(is_plugin_log("frostmod-crash-1.dmp"));
        assert!(!is_plugin_log("frostmod.dlo"));
        assert!(!is_plugin_log("frostmod.dir"));
        assert!(!is_plugin_log("frostmod_cmd.json"));
        assert!(!is_plugin_log("other.json"));
        assert!(!is_plugin_log("mxbsecure.dll"));
    }

    #[test]
    fn matches_the_engines_own_logs_and_dumps() {
        assert!(is_game_log("e3dlog.txt"));
        assert!(is_game_log("sllog.txt"));
        assert!(is_game_log("mxbikes_20261004.dmp"));
        assert!(!is_game_log("catalog.dat"));
    }

    /// The reported case: the saved install was `Documents\PiBoSo`. The export reads the game's
    /// logs and newest dumps out of the real install instead, names where plugins load from,
    /// and brings FrostMod's crash files and MXBMRP3's crash logs along.
    #[test]
    fn export_collects_the_real_installs_logs_and_dumps() {
        let root = tmpdir("real-install");
        let app_dir = root.join("applogs");
        let frostmod_dir = root.join("frostmod");
        let install = root.join("steamapps").join("common").join("MX Bikes");
        let docs_piboso = root.join("Documents").join("PiBoSo");
        let user = docs_piboso.join("MX Bikes");
        let crashes = user.join("mxbmrp3").join("crashes");
        for d in [&app_dir, &frostmod_dir, &install.join("plugins"), &user.join("profiles"), &crashes] {
            std::fs::create_dir_all(d).unwrap();
        }
        write(&install, "mxbikes.exe", "MZ");
        write(&install, "log.txt", "game");
        write(&install, "e3dlog.txt", "engine");
        write(&install, "sllog.txt", "sound");
        write(&install, "mxbikes.ini", "not a log");
        write(&install.join("plugins"), "frostmod.dlo", "plugin");
        // Five dumps: only the newest three go, and none over the size cap.
        let now = std::time::SystemTime::now();
        for i in 0..5u64 {
            let name = format!("crash{i}.dmp");
            write(&install, &name, "MDMP");
            let at = now - std::time::Duration::from_secs(600 * (i + 1));
            std::fs::File::options().write(true).open(install.join(&name)).unwrap().set_modified(at).unwrap();
        }
        write(&frostmod_dir, "frostmod-crash-20261004-101010.json", "{}");
        write(&frostmod_dir, "frostmod-crash-20261004-101010.dmp", "MDMP");
        write(&frostmod_dir, "plugin_places.txt", "C:\\somewhere");
        write(&crashes, "crash_2026-10-04.log", "mxbmrp3");

        let cfg = AppConfig {
            mods_path: user.to_string_lossy().into_owned(),
            game_path: docs_piboso.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let info = info_with(&app_dir, &frostmod_dir, &root.join("secure"), &cfg, Some(&install));
        let text = summary("9.9.9", Some("v0.41.0"), &cfg, &info, "");
        assert!(text.contains(&format!("install folder: {}", install.display())), "{text}");
        assert!(text.contains(&format!("saved in settings: {}", docs_piboso.display())), "{text}");
        assert!(text.contains("mxbikes.exe: present"), "{text}");
        assert!(text.contains("frostmod.dlo present"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("token"), "{text}");

        let dest = root.join("out.zip");
        export(&dest, &info, &text).unwrap();
        let zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        for want in [
            "game/log.txt",
            "game/e3dlog.txt",
            "game/sllog.txt",
            "game/crash0.dmp",
            "game/crash1.dmp",
            "game/crash2.dmp",
            "frostmod/frostmod-crash-20261004-101010.json",
            "frostmod/frostmod-crash-20261004-101010.dmp",
            "mxbmrp3/crash_2026-10-04.log",
        ] {
            assert!(names.contains(&want.to_string()), "{want} missing from {names:?}");
        }
        assert!(!names.iter().any(|n| n.ends_with("crash3.dmp") || n.ends_with("crash4.dmp")), "{names:?}");
        assert!(!names.iter().any(|n| n.ends_with("mxbikes.ini") || n.ends_with("plugin_places.txt")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The header is the first thing anyone reading the archive sees, and the FrostMod
    /// build it names is the one that wrote the loader log sitting beside it — `version.txt`
    /// itself is filtered out of the zip, so this line is the only place it appears.
    #[test]
    fn the_summary_names_the_loader_build() {
        let root = tmpdir("summary");
        let app_dir = root.join("applogs");
        let frostmod_dir = root.join("frostmod");
        for d in [&app_dir, &frostmod_dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        write(&frostmod_dir, "frostmod.log", "loader side");

        let cfg = AppConfig::default();
        let info = info_with(&app_dir, &frostmod_dir, &root.join("secure"), &cfg, None);

        let text = summary("9.9.9", Some("v0.13.0"), &cfg, &info, "reshade: x\ntyres: y\n");
        assert!(text.contains("MXB App 9.9.9"), "{text}");
        assert!(text.contains("frostmod: v0.13.0"), "{text}");
        assert!(text.contains("frostmod.log"), "{text}");
        assert!(text.contains("reshade: x\ntyres: y\n"), "{text}");

        // Nothing installed is a fact worth stating, not a line to leave out.
        assert!(summary("9.9.9", None, &cfg, &info, "").contains("frostmod: (not installed)"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn export_holds_every_group_and_a_summary() {
        let root = tmpdir("export");
        let app_dir = root.join("applogs");
        let frostmod_dir = root.join("frostmod");
        let game_dir = root.join("game");
        let secure_dir = root.join("secure");
        for d in [&app_dir, &frostmod_dir, &game_dir, &secure_dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        write(&app_dir, "frost.log", "app side");
        write(&frostmod_dir, "frostmod.log", "loader side");
        write(&frostmod_dir, "frostmod.exe", "not a log");
        write(&game_dir, "log.txt", "game side");
        write(&game_dir, "mxbikes.ini", "not a log");
        write(&secure_dir, "mxbsecure.log", "dll side");
        write(&secure_dir, "mxbsecure.dll", "not a log");

        let cfg = AppConfig {
            game_path: game_dir.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let info = info_with(&app_dir, &frostmod_dir, &secure_dir, &cfg, None);
        assert_eq!(info.app.files.len(), 1);
        assert_eq!(info.frostmod.files.len(), 1);
        assert_eq!(info.game.files.len(), 1);
        assert_eq!(info.secure.files.len(), 1);

        let dest = root.join("out").join("logs.zip");
        let result = export(&dest, &info, &summary("9.9.9", Some("v0.13.0"), &cfg, &info, "")).unwrap();
        assert_eq!(result.files, 4);

        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        assert!(names.contains(&"summary.txt".to_string()));
        assert!(names.contains(&"app/frost.log".to_string()));
        assert!(names.contains(&"frostmod/frostmod.log".to_string()));
        assert!(names.contains(&"game/log.txt".to_string()));
        assert!(names.contains(&"secure/mxbsecure.log".to_string()));
        // No folder's non-log contents came along.
        assert!(!names.iter().any(|n| n.ends_with("mxbikes.ini")));
        assert!(!names.iter().any(|n| n.ends_with("frostmod.exe")));
        assert!(!names.iter().any(|n| n.ends_with("mxbsecure.dll")));

        let mut body = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("game/log.txt").unwrap(), &mut body).unwrap();
        assert_eq!(body, "game side");
    }
}
