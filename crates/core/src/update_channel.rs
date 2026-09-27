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
    newest_release(releases, prefix, beta).map(|(_, url)| url)
}

/// [`newest_manifest`], with the tag's version it was picked by.
fn newest_release<'a>(releases: &'a [Release], prefix: &str, beta: bool) -> Option<(semver::Version, &'a str)> {
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
}

/// Is the release tagged `newest` an update for the running build?
///
/// Betas of one version share its bundle version: every `v0.19.0-beta.N` is built from a
/// `tauri.conf.json` that says `0.19.0`, so the manifests' `version` fields are all equal and
/// the plugin's own comparison (manifest against bundle) never offers beta.2 to beta.1. The
/// tag is where they differ, so when the running build knows its own tag (`current_tag`,
/// baked in as `MXB_RELEASE_TAG`), tags are compared. A build without one — a local build, or
/// an app that doesn't bake it — falls back to the plugin's comparison of bundle versions.
fn is_update(
    current_tag: Option<&semver::Version>,
    newest: &semver::Version,
    current_bundle: &semver::Version,
    remote_bundle: &semver::Version,
) -> bool {
    match current_tag {
        Some(tag) => newest > tag,
        None => remote_bundle > current_bundle,
    }
}

/// A build of this app in `repo` newer than the running one, or `None`.
///
/// `current_tag` is the release tag this build came from (`v0.19.0-beta.2`), when it has one;
/// see [`is_update`] for why it matters.
pub async fn check(
    webview: &tauri::Webview,
    repo: &str,
    prefix: &str,
    beta: bool,
    user_agent: &str,
    current_tag: Option<&str>,
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
    let Some((newest, manifest)) = newest_release(&releases, prefix, beta) else {
        return Ok(None);
    };
    let current_tag =
        current_tag.and_then(|t| semver::Version::parse(t.trim().strip_prefix(prefix).unwrap_or(t.trim())).ok());
    let updater = webview
        .updater_builder()
        .version_comparator(move |current, remote| is_update(current_tag.as_ref(), &newest, &current, &remote.version))
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

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    /// Every beta of 0.19.0 ships bundle version 0.19.0, so only the tag can tell beta.1 that
    /// beta.4 is newer. It must, and a build must never be offered itself or an older tag.
    #[test]
    fn betas_of_one_version_are_ordered_by_their_tags() {
        let bundle = v("0.19.0");
        assert!(is_update(Some(&v("0.19.0-beta.1")), &v("0.19.0-beta.4"), &bundle, &bundle));
        assert!(is_update(Some(&v("0.19.0-beta.4")), &v("0.19.0"), &bundle, &bundle), "the release outranks its betas");
        assert!(!is_update(Some(&v("0.19.0-beta.4")), &v("0.19.0-beta.4"), &bundle, &bundle));
        assert!(!is_update(Some(&v("0.19.0")), &v("0.19.0-beta.4"), &bundle, &bundle));
    }

    /// Without a baked tag (a local build), the bundle versions decide, as the plugin would.
    #[test]
    fn a_build_without_a_tag_compares_bundle_versions() {
        assert!(!is_update(None, &v("0.19.0-beta.4"), &v("0.19.0"), &v("0.19.0")));
        assert!(is_update(None, &v("0.19.1-beta.1"), &v("0.19.0"), &v("0.19.1")));
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

    /// A pre-move coach install (0.1.3 to 0.1.17) scans the manager's releases for `coach-v`,
    /// where the pointer releases live. It must pick the newest of those and nothing else —
    /// a manager release would be the wrong app entirely.
    #[test]
    fn a_pre_move_coach_follows_the_pointer_releases() {
        let list = [
            release("v0.17.3", false, true),
            release("coach-v0.1.17-beta.17", false, true),
            release("coach-v0.1.18-beta.18", false, true),
        ];
        assert_eq!(
            newest_manifest(&list, "coach-v", true),
            Some("https://x/coach-v0.1.18-beta.18/latest.json")
        );
    }

    /// In the coach's own repo the tags carry no product prefix, so it reads them the way the
    /// manager reads its own.
    #[test]
    fn the_coach_reads_plain_tags_in_its_own_repo() {
        let list = [release("v0.1.18-beta.18", false, true), release("v0.1.18", false, true)];
        assert_eq!(newest_manifest(&list, "v", false), Some("https://x/v0.1.18/latest.json"));
        assert_eq!(
            newest_manifest(&list, "v", true),
            Some("https://x/v0.1.18/latest.json"),
            "a release outranks its own beta"
        );
    }
}
