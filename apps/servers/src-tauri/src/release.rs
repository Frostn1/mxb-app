//! Public, signed mxbserver release discovery and verification.
//!
//! The repository is deliberately binary-only and public.  GitHub is transport, not trust:
//! every bundle is verified against the public key compiled into this app before its ELF is
//! allowed anywhere near a server.

use minisign_verify::{PublicKey, Signature};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const REPOSITORY: &str = "Frostn1/mxbserver-releases";
const API: &str = "https://api.github.com/repos/Frostn1/mxbserver-releases";
pub const ELF_ASSET: &str = "mxbserver-x86_64-unknown-linux-gnu.elf";
const VERSION_ASSET: &str = "VERSION";
const SUMS_ASSET: &str = "SHA256SUMS";
const SIGNATURE_ASSET: &str = "SHA256SUMS.minisig";
const KEY_ASSET: &str = "release.pub";
const REQUIRED: [&str; 5] = [
    ELF_ASSET,
    VERSION_ASSET,
    SUMS_ASSET,
    SIGNATURE_ASSET,
    KEY_ASSET,
];
const PINNED_KEY: &str = include_str!("../release.pub");
const MAX_ELF: usize = 128 * 1024 * 1024;
const MAX_TEXT: usize = 128 * 1024;

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    id: u64,
    name: String,
    size: u64,
    updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    draft: bool,
    prerelease: bool,
    target_commitish: String,
    published_at: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePreview {
    pub tag: String,
    pub name: String,
    pub prerelease: bool,
    pub version: String,
    pub revision: String,
    pub build_id: String,
    pub source_revision: String,
    pub published_at: String,
    /// Binds confirmation to this exact release record and exact asset objects.
    pub fingerprint: String,
}

pub struct VerifiedRelease {
    pub preview: ReleasePreview,
    pub binary: Vec<u8>,
    pub binary_sha256: String,
}

fn valid_tag(tag: &str) -> bool {
    if let Some(hex) = tag.strip_prefix("sha-") {
        return hex.len() == 8
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    }
    tag.strip_prefix('v').is_some_and(|rest| {
        !rest.is_empty()
            && rest.len() <= 64
            && rest.starts_with(|c: char| c.is_ascii_digit())
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
    })
}

fn parse_sums(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (hash, name) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("Bad SHA256SUMS line: {line}"))?;
        let name = name.trim_start().trim_start_matches('*');
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("Bad hash in SHA256SUMS line: {line}"));
        }
        if sums
            .insert(name.to_string(), hash.to_ascii_lowercase())
            .is_some()
        {
            return Err(format!("{name} is listed twice in SHA256SUMS."));
        }
    }
    Ok(sums)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verified_signature(key: &str, sums: &[u8], encoded: &str) -> Result<Signature, String> {
    let key = PublicKey::decode(key)
        .or_else(|_| PublicKey::from_base64(key.trim()))
        .map_err(|e| format!("The pinned release key is invalid: {e}"))?;
    let signature =
        Signature::decode(encoded).map_err(|e| format!("SHA256SUMS.minisig is invalid: {e}"))?;
    key.verify(sums, &signature, false)
        .map_err(|e| format!("The release signature does not verify: {e}"))?;
    Ok(signature)
}

fn verify_signature(key: &str, sums: &[u8], encoded: &str, tag: &str) -> Result<String, String> {
    let signature = verified_signature(key, sums, encoded)?;
    let words = signature
        .trusted_comment()
        .split_whitespace()
        .collect::<Vec<_>>();
    if words.len() != 3 || words[0] != "mxbserver" || words[1] != tag {
        return Err(format!(
            "The signature is for '{}', not mxbserver {tag}.",
            signature.trusted_comment()
        ));
    }
    let revision = words[2];
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("The release signature has no full source revision.".into());
    }
    Ok(revision.to_ascii_lowercase())
}

fn verify_hashes(sums: &BTreeMap<String, String>, files: &[(&str, &[u8])]) -> Result<(), String> {
    for (name, bytes) in files {
        let want = sums
            .get(*name)
            .ok_or_else(|| format!("The signed sums do not contain {name}."))?;
        if &sha256(bytes) != want {
            return Err(format!("{name} does not match the signed SHA-256."));
        }
    }
    Ok(())
}

fn parse_version(line: &str) -> Result<(String, String, String), String> {
    let line = line.trim();
    let rest = line
        .strip_prefix("mxbserver v")
        .ok_or("VERSION does not identify mxbserver.")?;
    let (version, metadata) = rest
        .split_once(" (")
        .ok_or("VERSION has no revision/build metadata.")?;
    let metadata = metadata
        .strip_suffix(')')
        .ok_or("VERSION metadata is incomplete.")?;
    let (revision, build) = metadata
        .split_once(", build ")
        .ok_or("VERSION has no build id.")?;
    if version.is_empty()
        || revision.len() < 7
        || !revision.bytes().all(|b| b.is_ascii_hexdigit())
        || build.is_empty()
        || !build
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err("VERSION metadata is invalid.".into());
    }
    Ok((
        version.to_string(),
        revision.to_ascii_lowercase(),
        build.to_string(),
    ))
}

fn assets(release: &GithubRelease) -> Result<BTreeMap<String, Asset>, String> {
    let required = REQUIRED.into_iter().collect::<BTreeSet<_>>();
    let mut found = BTreeMap::new();
    for asset in &release.assets {
        if required.contains(asset.name.as_str())
            && found.insert(asset.name.clone(), asset.clone()).is_some()
        {
            return Err(format!(
                "Release {} contains {} more than once.",
                release.tag_name, asset.name
            ));
        }
    }
    for name in REQUIRED {
        if !found.contains_key(name) {
            return Err(format!("Release {} is missing {name}.", release.tag_name));
        }
    }
    Ok(found)
}

fn fingerprint(release: &GithubRelease, assets: &BTreeMap<String, Asset>) -> String {
    let mut input = format!(
        "{}\n{}\n{:?}\n",
        release.tag_name,
        release.published_at.as_deref().unwrap_or(""),
        release.prerelease
    );
    for (name, asset) in assets {
        input.push_str(&format!(
            "{name}\t{}\t{}\t{}\n",
            asset.id, asset.size, asset.updated_at
        ));
    }
    sha256(input.as_bytes())
}

async fn github_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    path: &str,
) -> Result<T, String> {
    let response = client
        .get(format!("{API}{path}"))
        .header(USER_AGENT, "mxb-servers")
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the public release repository: {e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "The public release repository {REPOSITORY} or that release does not exist yet."
        ));
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned HTTP {} while reading public releases.",
            response.status()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Couldn't read GitHub's response: {e}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("GitHub returned unreadable release metadata: {e}"))
}

async fn find_release(
    client: &reqwest::Client,
    channel: &str,
    exact: Option<&str>,
) -> Result<GithubRelease, String> {
    let release = match channel {
        "stable" => github_json(client, "/releases/latest").await?,
        "prerelease" => {
            let releases: Vec<GithubRelease> = github_json(client, "/releases?per_page=30").await?;
            choose_prerelease(releases).ok_or("There is no public prerelease to install.")?
        }
        "tag" => {
            let tag = exact
                .filter(|tag| valid_tag(tag))
                .ok_or("Enter a valid release tag such as v0.2.0 or sha-1234abcd.")?;
            github_json(client, &format!("/releases/tags/{tag}")).await?
        }
        _ => return Err("Unknown release channel.".into()),
    };
    if release.draft {
        return Err("Draft releases cannot be installed.".into());
    }
    if !valid_tag(&release.tag_name) {
        return Err(format!(
            "GitHub returned an unsupported release tag: {}.",
            release.tag_name
        ));
    }
    if channel == "stable" && release.prerelease {
        return Err("GitHub marked the latest stable release as a prerelease.".into());
    }
    Ok(release)
}

fn choose_prerelease(releases: Vec<GithubRelease>) -> Option<GithubRelease> {
    releases
        .into_iter()
        .find(|release| !release.draft && release.prerelease && valid_tag(&release.tag_name))
}

async fn asset_bytes(
    client: &reqwest::Client,
    asset: &Asset,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    if asset.size > maximum as u64 {
        return Err(format!(
            "{} is larger than the {}-byte limit.",
            asset.name, maximum
        ));
    }
    let response = client
        .get(format!("{API}/releases/assets/{}", asset.id))
        .header(USER_AGENT, "mxb-servers")
        .header(ACCEPT, "application/octet-stream")
        .send()
        .await
        .map_err(|e| format!("Couldn't download {}: {e}", asset.name))?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned HTTP {} for {}.",
            response.status(),
            asset.name
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Couldn't read {}: {e}", asset.name))?;
    if bytes.len() > maximum || bytes.len() as u64 != asset.size {
        return Err(format!(
            "{} changed size while it was downloaded.",
            asset.name
        ));
    }
    Ok(bytes.to_vec())
}

pub async fn download(
    client: &reqwest::Client,
    channel: &str,
    exact: Option<&str>,
) -> Result<VerifiedRelease, String> {
    let release = find_release(client, channel, exact).await?;
    let found = assets(&release)?;
    let get = |name: &str| found.get(name).expect("required above");
    let key = asset_bytes(client, get(KEY_ASSET), MAX_TEXT).await?;
    let key = String::from_utf8(key).map_err(|_| "release.pub is not text.".to_string())?;
    if key.trim() != PINNED_KEY.trim() {
        return Err("The release public key does not match the key pinned in this app.".into());
    }
    let sums = asset_bytes(client, get(SUMS_ASSET), MAX_TEXT).await?;
    let signature = String::from_utf8(asset_bytes(client, get(SIGNATURE_ASSET), MAX_TEXT).await?)
        .map_err(|_| "SHA256SUMS.minisig is not text.".to_string())?;
    let source_revision = verify_signature(PINNED_KEY, &sums, &signature, &release.tag_name)?;
    let parsed_sums =
        parse_sums(std::str::from_utf8(&sums).map_err(|_| "SHA256SUMS is not text.".to_string())?)?;
    let version_bytes = asset_bytes(client, get(VERSION_ASSET), MAX_TEXT).await?;
    let binary = asset_bytes(client, get(ELF_ASSET), MAX_ELF).await?;
    verify_hashes(
        &parsed_sums,
        &[
            (VERSION_ASSET, version_bytes.as_slice()),
            (ELF_ASSET, binary.as_slice()),
        ],
    )?;
    let version_line =
        std::str::from_utf8(&version_bytes).map_err(|_| "VERSION is not text.".to_string())?;
    let (version, revision, build_id) = parse_version(version_line)?;
    if !source_revision.starts_with(&revision) {
        return Err(format!(
            "VERSION revision {revision} does not match signed source {source_revision}."
        ));
    }
    if release.target_commitish.len() == 40
        && release
            .target_commitish
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        && !release
            .target_commitish
            .eq_ignore_ascii_case(&source_revision)
    {
        return Err("The GitHub release target does not match the signed source revision.".into());
    }
    let binary_sha256 = sha256(&binary);
    Ok(VerifiedRelease {
        preview: ReleasePreview {
            tag: release.tag_name.clone(),
            name: release
                .name
                .clone()
                .unwrap_or_else(|| release.tag_name.clone()),
            prerelease: release.prerelease,
            version,
            revision,
            build_id,
            source_revision,
            published_at: release.published_at.clone().unwrap_or_default(),
            fingerprint: fingerprint(&release, &found),
        },
        binary,
        binary_sha256,
    })
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .no_proxy()
        .build()
        .map_err(|e| format!("Couldn't prepare the public release connection: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_reject_duplicate_assets() {
        assert!(parse_sums(&format!(
            "{}  VERSION\n{}  VERSION\n",
            "0".repeat(64),
            "1".repeat(64)
        ))
        .unwrap_err()
        .contains("twice"));
    }

    #[test]
    fn version_carries_every_runtime_identity() {
        assert_eq!(
            parse_version("mxbserver v0.1.1 (53b6edbd, build 6abb1a05)").unwrap(),
            ("0.1.1".into(), "53b6edbd".into(), "6abb1a05".into())
        );
        assert!(parse_version("mxbserver v0.1.1").is_err());
    }

    #[test]
    fn only_release_tags_are_accepted() {
        assert!(valid_tag("v0.2.0"));
        assert!(valid_tag("sha-53b6edbd"));
        assert!(!valid_tag("feature/bots"));
        assert!(!valid_tag("sha-53b6edb"));
        assert!(!valid_tag("sha-53B6EDBD"));
    }

    #[test]
    fn duplicate_named_release_assets_are_refused() {
        let mut all = REQUIRED
            .iter()
            .enumerate()
            .map(|(i, name)| Asset {
                id: i as u64,
                name: (*name).into(),
                size: 1,
                updated_at: "now".into(),
            })
            .collect::<Vec<_>>();
        all.push(all[0].clone());
        let release = GithubRelease {
            tag_name: "v1.0.0".into(),
            name: None,
            draft: false,
            prerelease: false,
            target_commitish: "main".into(),
            published_at: None,
            assets: all,
        };
        assert!(assets(&release).unwrap_err().contains("more than once"));
    }

    #[test]
    fn prerelease_discovery_skips_drafts_and_stable_releases() {
        let release = |tag: &str, draft: bool, prerelease: bool| GithubRelease {
            tag_name: tag.into(),
            name: None,
            draft,
            prerelease,
            target_commitish: "main".into(),
            published_at: None,
            assets: Vec::new(),
        };
        let chosen = choose_prerelease(vec![
            release("sha-11111111", true, true),
            release("v1.0.0", false, false),
            release("sha-22222222", false, true),
        ])
        .unwrap();
        assert_eq!(chosen.tag_name, "sha-22222222");
    }

    #[test]
    fn invalid_signature_is_rejected_before_hashes_are_trusted() {
        assert!(
            verify_signature("not a key", b"contents", "not a signature", "v1.0.0")
                .unwrap_err()
                .contains("pinned release key")
        );
    }

    #[test]
    fn a_real_minisign_signature_verifies_and_tampering_does_not() {
        const KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";
        assert!(verified_signature(KEY, b"test", SIGNATURE).is_ok());
        assert!(verified_signature(KEY, b"tesT", SIGNATURE).is_err());
    }

    #[test]
    fn signed_hashes_detect_missing_and_changed_files() {
        let good = b"server".as_slice();
        let mut sums = BTreeMap::from([(ELF_ASSET.to_string(), sha256(good))]);
        assert!(verify_hashes(&sums, &[(ELF_ASSET, good)]).is_ok());
        assert!(verify_hashes(&sums, &[(ELF_ASSET, b"changed")])
            .unwrap_err()
            .contains("does not match"));
        sums.clear();
        assert!(verify_hashes(&sums, &[(ELF_ASSET, good)])
            .unwrap_err()
            .contains("do not contain"));
    }
}
