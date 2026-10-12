//! Where Coach's logs are, and how to get them off the machine they're on.
//!
//! The same Settings → Logs MXB App has, built on the same `mxb_core::logshare` pieces
//! (scan, zip, upload). Three sets matter when something goes wrong here: Coach's own file
//! log (written by `tauri_plugin_log`, slow-command warnings included), whatever the
//! recorder and FrostMod left beside the plugin, and the game's `log.txt`.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use mxb_core::config::AppConfig;
use mxb_core::logshare::{self, is_game_log, is_plugin_log, scan, scan_all, ExportResult, LogGroup, ShareResult};

/// The recorder's own folder, `<game user folder>\mxbcoach`. Its sessions and sheets are not
/// logs and stay out; only `*.log` and `*.txt` files beside them are.
fn is_recorder_log(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".log") || lower.ends_with(".txt")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsInfo {
    /// Coach's own log dir.
    pub app: LogGroup,
    /// The recorder's folder under the game's user folder.
    pub recorder: LogGroup,
    /// The game's `plugins` folder: logs FrostMod and the recorder write beside the plugin.
    pub plugins: LogGroup,
    /// The game's own logs.
    pub game: LogGroup,
}

fn app_log_dir(app: &AppHandle) -> PathBuf {
    app.path().app_log_dir().unwrap_or_default()
}

fn info_of(app_log_dir: &Path, cfg: &AppConfig, install: Option<&Path>) -> LogsInfo {
    let dirs = crate::coach::session_dirs(cfg);
    let recorder = match crate::hud::coach_dir_of(&dirs) {
        Some(dir) => scan(&dir, is_recorder_log),
        None => LogGroup::missing(PathBuf::new()),
    };
    let (plugins, game) = match install {
        Some(dir) => (
            scan(&mxb_core::library::resolve_child(dir, "plugins"), is_plugin_log),
            scan(dir, is_game_log),
        ),
        None => (LogGroup::missing(PathBuf::new()), LogGroup::missing(PathBuf::new())),
    };
    LogsInfo { app: scan_all(app_log_dir), recorder, plugins, game }
}

fn info(app: &AppHandle, cfg: &AppConfig) -> LogsInfo {
    let install = mxb_core::gamefolders::real_install(cfg);
    info_of(&app_log_dir(app), cfg, install.as_deref())
}

/// The folders in the order they're written, paired with the folder each takes in the zip.
fn groups(info: &LogsInfo) -> [(&'static str, &LogGroup); 4] {
    [("app", &info.app), ("recorder", &info.recorder), ("plugins", &info.plugins), ("game", &info.game)]
}

fn show(path: &str) -> &str {
    if path.trim().is_empty() {
        "(not set)"
    } else {
        path.trim()
    }
}

/// The plain-text header that goes in beside the logs: what was included, and the facts
/// every report starts by asking for.
fn summary(version: &str, recorder_version: Option<&str>, cfg: &AppConfig, info: &LogsInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("MXB Coach {version}\n"));
    out.push_str(&format!("os: {} ({})\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("recorder: {}\n", recorder_version.unwrap_or("(not run yet)")));
    out.push_str(&format!("game: {}\n", cfg.game().display));
    out.push_str(&format!("install folder: {}\n", show(&cfg.install_dir())));
    for (label, group) in groups(info) {
        out.push_str(&format!(
            "\n{label} logs: {}{}\n",
            if group.dir.is_empty() { "(unknown)" } else { &group.dir },
            if group.exists { "" } else { " (folder not found)" },
        ));
        if group.files.is_empty() {
            out.push_str("  (none)\n");
        } else {
            out.push_str(&format!("  {} file(s), {} bytes\n", group.files.len(), group.total_bytes()));
        }
        for f in &group.files {
            out.push_str(&format!("  {} ({} bytes)\n", f.name, f.bytes));
        }
    }
    out
}

fn summary_now(app: &AppHandle, cfg: &AppConfig, info: &LogsInfo) -> String {
    let dirs = crate::coach::session_dirs(cfg);
    let recorder = crate::hud::coach_dir_of(&dirs).as_deref().and_then(crate::hud::recorder_version);
    summary(&app.package_info().version.to_string(), recorder.as_deref(), cfg, info)
}

/// Where the logs are and what's in each. Read fresh on every call: the reason anyone opens
/// this is that something just went wrong, and a stale "no logs" would send them looking in
/// the wrong place.
#[tauri::command]
pub fn logs_info(app: AppHandle) -> LogsInfo {
    let cfg = crate::coach::load_config(&app);
    info(&app, &cfg)
}

/// Open the folder one of the log sets lives in. `which` is `"app"`, `"recorder"`,
/// `"plugins"` or `"game"`.
#[tauri::command]
pub fn open_logs_folder(app: AppHandle, which: String) -> Result<(), String> {
    let info = logs_info(app);
    let group = match which.as_str() {
        "recorder" => &info.recorder,
        "plugins" => &info.plugins,
        "game" => &info.game,
        _ => &info.app,
    };
    logshare::open_location(group).map_err(|e| format!("{e:#}"))
}

/// Zip every set of logs to `dest`, a path the user just picked in a save dialog. Blocking
/// work, so off the UI thread.
#[tauri::command]
pub async fn export_logs(app: AppHandle, dest: String) -> Result<ExportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = crate::coach::load_config(&app);
        let info = info(&app, &cfg);
        let text = summary_now(&app, &cfg, &info);
        logshare::export(Path::new(&dest), &groups(&info), &text).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("export_logs task failed: {e}"))?
}

/// Zip every set of logs and upload it, handing back the direct link.
#[tauri::command]
pub async fn share_logs(app: AppHandle) -> Result<ShareResult, String> {
    let cfg = crate::coach::load_config(&app);
    let info = info(&app, &cfg);
    let text = summary_now(&app, &cfg, &info);
    let owned = groups(&info).iter().map(|(n, g)| (*n, (*g).clone())).collect();
    logshare::share(&app, owned, &text, "mxb-coach-logs.zip")
        .await
        .map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("coach-logs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_bundle_holds_coachs_log_the_recorders_and_the_games() {
        let root = tmpdir("bundle");
        let app_dir = root.join("applogs");
        let install = root.join("MX Bikes");
        let user = root.join("user");
        let coach_dir = user.join("mxbcoach");
        for d in [&app_dir, &install.join("plugins"), &coach_dir.join("sessions")] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(app_dir.join("mxb-coach.log"), "slow command coach_status: 700 ms").unwrap();
        std::fs::write(coach_dir.join("recorder.log"), "recorder side").unwrap();
        std::fs::write(coach_dir.join("hud.ini"), "not a log").unwrap();
        std::fs::write(coach_dir.join("sessions").join("a.mxbc"), "not a log").unwrap();
        std::fs::write(install.join("log.txt"), "game side").unwrap();
        std::fs::write(install.join("mxbikes.ini"), "not a log").unwrap();
        std::fs::write(install.join("plugins").join("mxbcoach.dlo"), "plugin").unwrap();
        std::fs::write(install.join("plugins").join("frostmod_session.log"), "loader side").unwrap();

        let cfg = AppConfig { mods_path: user.to_string_lossy().into_owned(), ..Default::default() };
        let info = info_of(&app_dir, &cfg, Some(&install));
        assert_eq!(info.app.files.len(), 1);
        assert_eq!(info.plugins.files.len(), 1);
        assert_eq!(info.game.files.len(), 1);

        let dest = root.join("out.zip");
        let text = summary("9.9.9", Some("0.30.0"), &cfg, &info);
        assert!(text.contains("MXB Coach 9.9.9") && text.contains("recorder: 0.30.0"), "{text}");
        let result = logshare::export(&dest, &groups(&info), &text).unwrap();
        assert!(result.files >= 3);
        let zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        for want in ["summary.txt", "app/mxb-coach.log", "plugins/frostmod_session.log", "game/log.txt"] {
            assert!(names.contains(&want.to_string()), "{want} missing from {names:?}");
        }
        assert!(!names.iter().any(|n| n.ends_with("hud.ini") || n.ends_with(".mxbc") || n.ends_with(".dlo")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
