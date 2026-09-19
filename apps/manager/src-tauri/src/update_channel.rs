//! The beta update channel: the newest release, pre-releases included.
//!
//! The stable channel is the updater plugin's own check against `releases/latest`, which
//! GitHub never points at a pre-release. The beta channel finds the newest `v` release with a
//! signed `latest.json` and hands the plugin that manifest; the lookup is core's, shared with
//! MXB Coach and Frost's Studio, each against its own repo.

use mxb_core::update_channel::{self, UpdateMetadata};

const REPO: &str = "Frostn1/mxb-app";

/// Check the beta channel. `None` when nothing newer than this build is out.
#[tauri::command]
pub async fn check_beta_update(webview: tauri::Webview) -> Result<Option<UpdateMetadata>, String> {
    update_channel::check(&webview, REPO, "v", true, crate::frostmod_manage::UA)
        .await
        .map_err(|e| format!("{e:#}"))
}
