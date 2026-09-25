//! Whether MXBMRP3 is installed, so the app can suggest it where it isn't.
//!
//! MXBMRP3 is thomas4f's open-source HUD plugin for PiBoSo's sims: standings, timing, a gap to
//! your best, a track map, radar. It's installed and updated by its own installer from
//! <https://github.com/thomas4f/mxbmrp3/releases>, and per its README it puts one `.dlo` in the
//! game's `plugins` folder (`mxbmrp3.dlo` for MX Bikes, `mxbmrp3_gpb.dlo` for GP Bikes) next to
//! an `mxbmrp3_data` folder. That `.dlo` is what the game loads, so it's what's looked for.
//!
//! Read-only. The app never downloads MXBMRP3 or writes anything into the game folder for it.

use mxb_core::config::{self, AppConfig};
use mxb_core::game::Game;
use serde::Serialize;
use std::path::Path;

/// The official place to get it: the author's releases, which carry the installer.
pub const DOWNLOAD_URL: &str = "https://github.com/thomas4f/mxbmrp3/releases";

/// The plugin file the game loads for `game`.
pub fn plugin_file(game: Game) -> &'static str {
    match game {
        Game::Mxb => "mxbmrp3.dlo",
        Game::Gpb => "mxbmrp3_gpb.dlo",
    }
}

/// `Some(true)` when the plugin is in `<install_dir>\plugins`, `Some(false)` when that folder
/// can be looked in and it isn't there, `None` when the game folder isn't known. "Don't know"
/// is kept apart from "not installed" so a rider whose game can't be found isn't told to
/// install a plugin they may well have.
pub fn installed_in(install_dir: &str, game: Game) -> Option<bool> {
    let dir = install_dir.trim();
    if dir.is_empty() || !Path::new(dir).is_dir() {
        return None;
    }
    Some(Path::new(dir).join("plugins").join(plugin_file(game)).is_file())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// See [`installed_in`]: `None` is "the game folder isn't known".
    pub installed: Option<bool>,
    /// "Don't ask again" was chosen.
    pub dismissed: bool,
    pub download_url: &'static str,
}

pub fn status_of(cfg: &AppConfig) -> Status {
    Status {
        installed: installed_in(&cfg.install_dir(), cfg.active_game),
        dismissed: cfg.mxbmrp3_dismissed,
        download_url: DOWNLOAD_URL,
    }
}

#[tauri::command]
pub fn mxbmrp3_status(app: tauri::AppHandle) -> Status {
    status_of(&config::load(&app).unwrap_or_default())
}

/// Set or clear "don't ask again". No-op before the config exists, like `set_intro_seen`.
#[tauri::command]
pub fn set_mxbmrp3_dismissed(app: tauri::AppHandle, dismissed: bool) -> Result<(), String> {
    if !config::exists(&app) {
        return Ok(());
    }
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.mxbmrp3_dismissed = dismissed;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A fake game folder of our own under the temp dir, gone when dropped.
    struct GameDir(PathBuf);
    impl GameDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("mxbmrp3-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(p.join("plugins")).unwrap();
            GameDir(p)
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
        fn put(&self, name: &str) {
            std::fs::write(self.0.join("plugins").join(name), b"").unwrap();
        }
    }
    impl Drop for GameDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn found_where_the_game_loads_it() {
        let g = GameDir::new("found");
        assert_eq!(installed_in(g.path(), Game::Mxb), Some(false), "an empty plugins folder");
        g.put("mxbmrp3.dlo");
        assert_eq!(installed_in(g.path(), Game::Mxb), Some(true));
    }

    /// Each game has its own build; GP Bikes' being there says nothing about MX Bikes'.
    #[test]
    fn each_game_looks_for_its_own_build() {
        let g = GameDir::new("per-game");
        g.put("mxbmrp3_gpb.dlo");
        assert_eq!(installed_in(g.path(), Game::Mxb), Some(false));
        assert_eq!(installed_in(g.path(), Game::Gpb), Some(true));
    }

    /// Its data folder, or a stray file with a similar name, isn't the plugin the game loads.
    #[test]
    fn only_the_plugin_file_counts() {
        let g = GameDir::new("only-dlo");
        std::fs::create_dir_all(g.0.join("plugins").join("mxbmrp3_data")).unwrap();
        g.put("mxbmrp3.dlo.bak");
        assert_eq!(installed_in(g.path(), Game::Mxb), Some(false));
    }

    /// A game folder that isn't known is "don't know", never "not installed".
    #[test]
    fn an_unknown_game_folder_suggests_nothing() {
        assert_eq!(installed_in("", Game::Mxb), None);
        assert_eq!(installed_in("   ", Game::Mxb), None);
        assert_eq!(installed_in("Z:\\no\\such\\game", Game::Mxb), None);
    }

    #[test]
    fn the_status_carries_the_choice_and_the_official_link() {
        let g = GameDir::new("status");
        let cfg = AppConfig { game_path: g.path().into(), mxbmrp3_dismissed: true, ..Default::default() };
        let s = status_of(&cfg);
        assert_eq!(s.installed, Some(false));
        assert!(s.dismissed);
        assert_eq!(s.download_url, "https://github.com/thomas4f/mxbmrp3/releases");
    }

    /// A config written before this existed still asks.
    #[test]
    fn an_older_config_has_not_said_no() {
        let cfg: AppConfig = serde_json::from_str(r#"{"gamePath":"C:\\MX Bikes"}"#).unwrap();
        assert!(!cfg.mxbmrp3_dismissed);
    }
}
