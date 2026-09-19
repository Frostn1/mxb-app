//! Update channels for the apps in this workspace: the newest signed build among a repo's
//! releases, picked by tag prefix and channel.
//!
//! Every release carries its own signed `latest.json`, so a channel only has to find the newest
//! one and hand the updater plugin that manifest. Download, signature check and install stay
//! the plugin's. Each app has its own release repo now — the manager `Frostn1/mxb-app`, MXB
//! Coach `Frostn1/mxb-coach` — but the prefix stays: coach installs from before that move look
//! for `coach-v` tags in the manager's repo, where a pointer release carries the manifest
//! (scripts/coach-update-bridge.sh), and the prefix is what keeps them off the manager's own
//! `v` releases.

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

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

/// The `latest.json` of the highest-versioned published release tagged `prefix` + a version.
/// Off the beta channel, a version with a pre-release part (`-beta.1`) doesn't count.
fn newest_manifest<'a>(releases: &'a [Release], prefix: &str, beta: bool) -> Option<&'a str> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = semver::Version::parse(r.tag_name.strip_prefix(prefix)?).ok()?;
            if !beta && !version.pre.is_empty() {
                return None;
            }
            let manifest = r.assets.iter().find(|a| a.name == "latest.json")?;
            Some((version, manifest.browser_download_url.as_str()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, url)| url)
}

/// A build of this app in `repo` newer than the running one, or `None`.
pub async fn check(
    webview: &tauri::Webview,
    repo: &str,
    prefix: &str,
    beta: bool,
    user_agent: &str,
) -> anyhow::Result<Option<UpdateMetadata>> {
    let text = reqwest::Client::builder()
        .user_agent(user_agent)
        .build()?
        .get(format!("https://api.github.com/repos/{repo}/releases?per_page=50"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let releases: Vec<Release> = serde_json::from_str(&text)?;
    let Some(manifest) = newest_manifest(&releases, prefix, beta) else {
        return Ok(None);
    };
    let updater = webview.updater_builder().endpoints(vec![manifest.parse()?])?.build()?;
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
        assert_eq!(newest_manifest(&list, "v", true), Some("https://x/v0.14.0/latest.json"));
    }

    #[test]
    fn a_beta_newer_than_the_release_wins_only_on_the_beta_channel() {
        let list = [release("v0.14.0", false, true), release("v0.14.1-beta.1", false, true)];
        assert_eq!(newest_manifest(&list, "v", true), Some("https://x/v0.14.1-beta.1/latest.json"));
        assert_eq!(newest_manifest(&list, "v", false), Some("https://x/v0.14.0/latest.json"));
    }

    #[test]
    fn skips_drafts_odd_tags_and_releases_without_a_manifest() {
        let list = [
            release("v0.15.0", true, true),
            release("v0.14.2", false, false),
            release("nightly", false, true),
            release("v0.14.1", false, true),
        ];
        assert_eq!(newest_manifest(&list, "v", true), Some("https://x/v0.14.1/latest.json"));
    }

    #[test]
    fn each_app_only_sees_its_own_tags() {
        let list = [
            release("v0.14.4", false, true),
            release("coach-v0.1.3-beta.3", false, true),
            release("studio-v0.1.6", false, true),
        ];
        assert_eq!(newest_manifest(&list, "v", true), Some("https://x/v0.14.4/latest.json"));
        assert_eq!(newest_manifest(&list, "coach-v", true), Some("https://x/coach-v0.1.3-beta.3/latest.json"));
        assert_eq!(newest_manifest(&list, "coach-v", false), None, "no stable coach yet");
    }

    /// A pre-move coach install (0.1.3 to 0.1.16) scans the manager's releases for `coach-v`,
    /// where the pointer releases live. It must pick the newest of those and nothing else —
    /// a manager release would be the wrong app entirely.
    #[test]
    fn a_pre_move_coach_follows_the_pointer_releases() {
        let list = [
            release("v0.17.3", false, true),
            release("coach-v0.1.16-beta.16", false, true),
            release("coach-v0.1.17-beta.17", false, true),
        ];
        assert_eq!(
            newest_manifest(&list, "coach-v", true),
            Some("https://x/coach-v0.1.17-beta.17/latest.json")
        );
    }

    /// In the coach's own repo the tags carry no product prefix, so it reads them the way the
    /// manager reads its own.
    #[test]
    fn the_coach_reads_plain_tags_in_its_own_repo() {
        let list = [release("v0.1.17-beta.17", false, true), release("v0.1.17", false, true)];
        assert_eq!(newest_manifest(&list, "v", false), Some("https://x/v0.1.17/latest.json"));
        assert_eq!(
            newest_manifest(&list, "v", true),
            Some("https://x/v0.1.17/latest.json"),
            "a release outranks its own beta"
        );
    }
}
