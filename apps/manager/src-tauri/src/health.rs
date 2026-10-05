//! Two health checks the Home screen shows and the log export records.
//!
//! Both come from the same report: a player who crashed joining busy public servers but never
//! riding alone. Joining loads every other rider's bikes, paints and textures at once on the
//! game's main thread, and two things on his PC made that fragile:
//!
//! * **The PiBoSo folder inside OneDrive.** Every read goes through OneDrive's filter driver,
//!   and an online-only file is a download the game waits on mid-join. See
//!   [`crate::cloudfiles::check`] — attributes only, nothing is hydrated by looking.
//! * **ReShade**, hooked into `opengl32`. The crashes stopped when he removed it. See
//!   [`crate::reshade::health`].
//!
//! Neither is a fault the app can prove from here, so both are notices with a reversible
//! one-click fix, never something done behind the player's back.

use crate::config::AppConfig;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub onedrive: crate::cloudfiles::CloudHealth,
    pub reshade: crate::reshade::Health,
}

impl Report {
    /// The two lines `summary.txt` carries, so support sees both without asking.
    pub fn summary_lines(&self) -> String {
        format!("{}\n{}\n", self.onedrive.summary_line(), self.reshade.summary_line())
    }
}

/// Run both checks. Blocking: walks the mods tree's metadata and reads the candidate DLLs.
pub fn check(cfg: &AppConfig) -> Report {
    let mods_root = if cfg.mods_path.trim().is_empty() {
        std::path::PathBuf::new()
    } else {
        crate::library::mods_root(&cfg.mods_path)
    };
    let profiles = if cfg.mods_path.trim().is_empty() && cfg.profiles_path.trim().is_empty() {
        std::path::PathBuf::new()
    } else {
        cfg.profiles_dir()
    };
    let install = cfg.install_dir();
    Report {
        onedrive: crate::cloudfiles::check(&mods_root, &profiles, std::path::Path::new(install.trim())),
        reshade: crate::reshade::health(&cfg.reshade_dir()),
    }
}

/// The folder "Keep on this device" pins: the PiBoSo folder, re-derived here rather than
/// taken from the UI, so the button can only ever act on the folder the check named.
pub fn pin_target(cfg: &AppConfig) -> Option<std::path::PathBuf> {
    if cfg.mods_path.trim().is_empty() {
        return None;
    }
    crate::cloudfiles::pin_dir(&crate::library::mods_root(&cfg.mods_path), &cfg.profiles_dir())
        .filter(|d| d.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unconfigured_app_reports_unknown_not_a_problem() {
        let r = check(&AppConfig::default());
        assert_eq!(r.onedrive.online_only.total(), 0);
        let lines = r.summary_lines();
        assert!(lines.contains("onedrive: "));
        assert!(lines.contains("reshade: "));
    }
}
