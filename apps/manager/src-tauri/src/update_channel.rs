//! The beta update channel: the newest release, pre-releases included.
//!
//! The stable channel is the updater plugin's own check against `releases/latest`, which
//! GitHub never points at a pre-release. Every release — beta or not — carries its own
//! signed `latest.json`, so the beta channel finds the newest one and hands the plugin that
//! manifest instead. Download, signature check and install stay the plugin's.

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

const REPO: &str = "Frostn1/mxb-app";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// What the plugin's own `check` returns, so the webview can wrap it in its `Update` class
/// and install it with the plugin's commands.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: tauri::ResourceId,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

/// The `latest.json` of the highest-versioned published release.
fn newest_manifest(releases: &[Release]) -> Option<&str> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = semver::Version::parse(r.tag_name.trim_start_matches('v')).ok()?;
            let manifest = r.assets.iter().find(|a| a.name == "latest.json")?;
            Some((version, manifest.browser_download_url.as_str()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, url)| url)
}

async fn check(webview: &tauri::Webview) -> anyhow::Result<Option<UpdateMetadata>> {
    let releases: Vec<Release> = reqwest::Client::builder()
        .user_agent(crate::frostmod_manage::UA)
        .build()?
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=20"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let Some(manifest) = newest_manifest(&releases) else {
        return Ok(None);
    };
    let updater = webview
        .updater_builder()
        .endpoints(vec![manifest.parse()?])?
        .build()?;
    let Some(update) = updater.check().await? else {
        return Ok(None);
    };
    Ok(Some(UpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        // Already RFC 3339 in the manifest, which is the form the plugin sends.
        date: update.raw_json["pub_date"].as_str().map(str::to_owned),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    }))
}

/// Check the beta channel. `None` when nothing newer than this build is out.
#[tauri::command]
pub async fn check_beta_update(webview: tauri::Webview) -> Result<Option<UpdateMetadata>, String> {
    check(&webview).await.map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool, manifest: bool) -> Release {
        Release {
            tag_name: tag.into(),
            draft,
            assets: manifest
                .then(|| Asset {
                    name: "latest.json".into(),
                    browser_download_url: format!("https://x/{tag}/latest.json"),
                })
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn picks_the_highest_version_not_the_first_listed() {
        let list = [
            release("v0.14.0-beta.4", false, true),
            release("v0.14.0", false, true),
            release("v0.13.7", false, true),
        ];
        assert_eq!(newest_manifest(&list), Some("https://x/v0.14.0/latest.json"));
    }

    #[test]
    fn a_beta_newer_than_the_release_wins() {
        let list = [release("v0.14.0", false, true), release("v0.14.1-beta.1", false, true)];
        assert_eq!(newest_manifest(&list), Some("https://x/v0.14.1-beta.1/latest.json"));
    }

    #[test]
    fn skips_drafts_odd_tags_and_releases_without_a_manifest() {
        let list = [
            release("v0.15.0", true, true),
            release("v0.14.2", false, false),
            release("nightly", false, true),
            release("v0.14.1", false, true),
        ];
        assert_eq!(newest_manifest(&list), Some("https://x/v0.14.1/latest.json"));
    }
}
