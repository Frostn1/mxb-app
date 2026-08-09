//! Sharing paints with everyone else on a server.
//!
//! MX Bikes transmits no custom content: a remote rider renders using whatever local file
//! matches the name they picked, so a grid of strangers is a grid of default liveries. The
//! game can't tell us what they picked either — its plugin API carries rider names and
//! bikes and no paint field at all — so the loop is closed outside the game. Each app
//! publishes what its own rider is wearing, and pulls back what everyone else published.
//!
//! Everything is content-addressed by SHA-256, so twenty riders sharing a paint is one
//! stored object and nineteen uploads that never happen.

use crate::bundle;
use crate::config::AppConfig;
use crate::presets::{self, Loadout};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Where the control plane lives. A constant rather than a setting: pointing the app at
/// another host would let anything served there write files into the mods folder.
pub const CONTROL_PLANE: &str = "https://mxb-control-plane.aui-svi.workers.dev";

/// Only `.pnt` files are shared. Models are directories and often large, and none of the
/// non-paint slots carry a file a receiver could use.
const PAINT_EXT: &str = "pnt";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintEntry {
    pub slot: String,
    pub file_name: String,
    pub sha256: String,
    pub size: u64,
    /// Destination relative to `<MX Bikes>/mods`, forward slashes.
    pub rel_dest: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishOutcome {
    pub published: usize,
    pub uploaded: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcome {
    pub riders: usize,
    pub installed: usize,
    /// Already present with matching content — the common case after the first sync.
    pub already_had: usize,
    /// Entries refused because their destination wasn't safe to write.
    pub rejected: usize,
}

/// Resolve a control-plane-supplied destination against the local mods folder.
///
/// **This is the security boundary of the whole feature.** `rel_dest` is written by another
/// player, and this is where it becomes a real path. A value like `../../mxbikes.ini`, an
/// absolute path or a Windows drive letter would each escape the mods folder and let one
/// player overwrite arbitrary files on everyone else's machine. The control plane rejects
/// those too, but only this check actually protects a disk, so it does not trust it.
pub fn safe_dest(mods_dir: &Path, rel_dest: &str) -> Option<PathBuf> {
    let rel = rel_dest.trim();
    if rel.is_empty() || rel.len() > 256 {
        return None;
    }
    // One separator form to reason about; a backslash would be a path separator on Windows
    // while looking like an ordinary character to a naive check.
    if rel.contains('\\') {
        return None;
    }
    if rel.starts_with('/') {
        return None;
    }
    // `C:` and friends.
    if rel.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    if rel.chars().any(|c| c.is_control()) {
        return None;
    }

    let mut out = mods_dir.to_path_buf();
    let mut segments = 0usize;
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return None;
        }
        // Reject anything the OS would interpret as more than a plain name.
        let as_path = Path::new(segment);
        if as_path.components().count() != 1
            || !matches!(as_path.components().next(), Some(Component::Normal(_)))
        {
            return None;
        }
        out.push(segment);
        segments += 1;
    }
    if segments < 2 {
        // A bare filename would drop a paint at the root of the mods folder, which is never
        // where one belongs.
        return None;
    }
    if out.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
        != Some(PAINT_EXT.to_string())
    {
        return None;
    }
    // Belt and braces: whatever the segment walk produced must still sit under the root.
    if !out.starts_with(mods_dir) {
        return None;
    }
    Some(out)
}

pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_bytes(&bytes))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The paints in a rider's current look, ready to publish.
pub fn local_paints(cfg: &AppConfig, profile: &str, bike: &str) -> anyhow::Result<Vec<PaintEntry>> {
    let loadout: Loadout = presets::read_loadout(&cfg.profiles_dir(), profile, bike)?;
    let plan = bundle::plan(cfg, &loadout)?;

    let mut out = Vec::new();
    for asset in plan.assets {
        if asset.is_dir {
            continue;
        }
        let path = Path::new(&asset.abs_path);
        let is_paint = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(PAINT_EXT))
            .unwrap_or(false);
        if !is_paint {
            continue;
        }
        let Ok(sha) = sha256_file(path) else { continue };
        out.push(PaintEntry {
            slot: asset.slot,
            file_name: asset.name,
            sha256: sha,
            size: asset.size,
            rel_dest: asset.rel_dest,
        });
    }
    Ok(out)
}

// ── Control-plane calls ──────────────────────────────────────────────────────

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

/// Publish this rider's paints, then upload only the blobs nobody has shared yet.
pub async fn publish(
    cfg: &AppConfig,
    token: &str,
    profile: &str,
    bike: &str,
) -> anyhow::Result<PublishOutcome> {
    let entries = local_paints(cfg, profile, bike)?;
    let http = client()?;

    #[derive(Deserialize)]
    struct Resp {
        missing: Vec<String>,
    }
    let resp = http
        .put(format!("{CONTROL_PLANE}/v1/loadout"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "bikeId": bike, "paints": entries }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("the control plane refused the loadout ({})", resp.status());
    }
    let missing: Resp = resp.json().await?;

    let mut uploaded = 0usize;
    for sha in missing.missing {
        let Some(entry) = entries.iter().find(|e| e.sha256 == sha) else { continue };
        // Re-resolve from the plan rather than trusting a path round-tripped through the
        // server; the bytes we upload must be the bytes we hashed.
        let Some(path) = local_paths(cfg, profile, bike, &entry.sha256)? else { continue };
        let bytes = std::fs::read(&path)?;
        let up = http
            .put(format!("{CONTROL_PLANE}/v1/paints/{sha}"))
            .bearer_auth(token)
            .body(bytes)
            .send()
            .await?;
        if up.status().is_success() {
            uploaded += 1;
        } else {
            log::warn!("uploading {sha} failed: {}", up.status());
        }
    }

    Ok(PublishOutcome { published: entries.len(), uploaded })
}

/// The local file backing a published entry, matched by digest so a renamed or swapped
/// file can't be uploaded under someone else's hash.
fn local_paths(
    cfg: &AppConfig,
    profile: &str,
    bike: &str,
    sha: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let loadout = presets::read_loadout(&cfg.profiles_dir(), profile, bike)?;
    for asset in bundle::plan(cfg, &loadout)?.assets {
        if asset.is_dir {
            continue;
        }
        let path = PathBuf::from(&asset.abs_path);
        if sha256_file(&path).map(|h| h == sha).unwrap_or(false) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Fetch everyone's paints and install the ones this machine is missing.
pub async fn pull(cfg: &AppConfig, token: &str, server_id: &str) -> anyhow::Result<PullOutcome> {
    #[derive(Deserialize)]
    struct Rider {
        #[allow(dead_code)]
        #[serde(rename = "riderName")]
        rider_name: String,
        paints: Vec<PaintEntry>,
    }
    #[derive(Deserialize)]
    struct Roster {
        riders: Vec<Rider>,
    }

    let http = client()?;
    let roster: Roster = http
        .get(format!("{CONTROL_PLANE}/v1/roster"))
        .query(&[("server", server_id)])
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mods_dir = crate::library::mods_root(&cfg.mods_path);
    let mut out = PullOutcome { riders: roster.riders.len(), ..Default::default() };

    for rider in &roster.riders {
        for paint in &rider.paints {
            let Some(dest) = safe_dest(&mods_dir, &paint.rel_dest) else {
                // Refused rather than sanitised: a destination we had to rewrite is one we
                // don't understand, and guessing writes someone's file somewhere odd.
                log::warn!("refusing paint destination {:?}", paint.rel_dest);
                out.rejected += 1;
                continue;
            };
            if dest.is_file() && sha256_file(&dest).map(|h| h == paint.sha256).unwrap_or(false) {
                out.already_had += 1;
                continue;
            }

            let bytes = http
                .get(format!("{CONTROL_PLANE}/v1/paints/{}", paint.sha256))
                .bearer_auth(token)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;

            // Verify before writing. The digest is the only thing making these bytes
            // trustworthy, and an unverified write would put unchecked content into the
            // game's folder under a name the game will load.
            if sha256_bytes(&bytes) != paint.sha256 {
                log::warn!("digest mismatch for {}, skipping", paint.sha256);
                out.rejected += 1;
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, &bytes)?;
            out.installed += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods() -> PathBuf {
        PathBuf::from("/games/MX Bikes/mods")
    }

    #[test]
    fn accepts_the_layout_the_game_uses() {
        let dest = safe_dest(&mods(), "bikes/2026 KTM 450/paints/Frost.pnt").unwrap();
        assert!(dest.ends_with("bikes/2026 KTM 450/paints/Frost.pnt"));
        assert!(dest.starts_with(mods()));
    }

    #[test]
    fn refuses_to_escape_the_mods_folder() {
        // Each of these is written by another player and would otherwise land outside the
        // mods folder — the whole reason this function exists.
        for bad in [
            "../mxbikes.ini",
            "bikes/../../../mxbikes.ini",
            "/etc/passwd",
            "C:/Windows/system32/evil.pnt",
            "c:\\windows\\evil.pnt",
            "bikes\\ktm\\paints\\x.pnt",
            "bikes//paints/x.pnt",
            "./bikes/x.pnt",
            "bikes/./x.pnt",
        ] {
            assert!(safe_dest(&mods(), bad).is_none(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn requires_a_paint_at_the_end() {
        assert!(safe_dest(&mods(), "bikes/ktm/paints/readme.txt").is_none());
        assert!(safe_dest(&mods(), "bikes/ktm/paints/mxbikes.exe").is_none());
        assert!(safe_dest(&mods(), "bikes/ktm/paints").is_none());
    }

    #[test]
    fn refuses_a_bare_filename_at_the_mods_root() {
        assert!(safe_dest(&mods(), "Frost.pnt").is_none());
    }

    #[test]
    fn refuses_control_characters_and_absurd_lengths() {
        assert!(safe_dest(&mods(), "bikes/\u{0}/x.pnt").is_none());
        assert!(safe_dest(&mods(), &format!("{}x.pnt", "a/".repeat(200))).is_none());
        assert!(safe_dest(&mods(), "   ").is_none());
    }

    #[test]
    fn hashes_match_the_reference_digest() {
        // Anchored to a known vector so a future change to the hashing can't silently
        // repartition everyone's content-addressed storage.
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
