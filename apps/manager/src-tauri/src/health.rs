//! The health checks the Home screen shows and the log export records. The tyre check has its
//! own story in [`crate::tyres`].
//!
//! ReShade comes from a player who crashed joining busy public servers but never riding
//! alone: joining loads every other rider's bikes, paints and textures at once on the game's
//! main thread, and ReShade, hooked into `opengl32`, made that fragile. The crashes stopped
//! when he removed it. See [`crate::reshade::health`]. It is not a fault the app can prove
//! from here, so it is a notice with a reversible one-click fix, never something done behind
//! the player's back.

use crate::config::AppConfig;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub reshade: crate::reshade::Health,
    /// Empty or stock-replacing folders in `mods/tyres`. See [`crate::tyres`].
    pub tyres: crate::tyres::TyreHealth,
}

impl Report {
    /// The lines `summary.txt` carries, so support sees them without asking.
    pub fn summary_lines(&self) -> String {
        format!("{}\n{}\n", self.reshade.summary_line(), self.tyres.summary_line())
    }
}

/// Run the checks. Blocking: reads the candidate DLLs and lists `mods/tyres`.
pub fn check(cfg: &AppConfig) -> Report {
    Report {
        reshade: crate::reshade::health(&cfg.reshade_dir()),
        tyres: crate::tyres::check(cfg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unconfigured_app_reports_unknown_not_a_problem() {
        let r = check(&AppConfig::default());
        let lines = r.summary_lines();
        assert!(!lines.contains("onedrive"));
        assert!(lines.contains("reshade: "));
        assert!(lines.contains("tyres: ok"));
    }
}
