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

/// A server in the control plane's registry, as `GET /v1/servers` returns it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredServer {
    pub id: String,
    pub name: String,
    pub region: String,
    /// `host:port` — the same form the game's connect flag takes.
    pub address: String,
}

/// The joinable servers.
///
/// `token` is optional because this endpoint is public: the join picker has to work before
/// a player has enrolled, since "which servers can I join" is exactly what someone with no
/// account is asking. It is still sent when we have one, so the control plane can key
/// anything it later wants to personalise off the caller.
pub async fn registry(token: Option<&str>) -> anyhow::Result<Vec<RegisteredServer>> {
    #[derive(Deserialize)]
    struct Resp {
        servers: Vec<RegisteredServer>,
    }
    let mut req = client()?.get(format!("{CONTROL_PLANE}/v1/servers"));
    if let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) {
        req = req.bearer_auth(token);
    }
    let resp: Resp = req.send().await?.error_for_status()?.json().await?;
    Ok(resp.servers)
}

/// The roster key for the server at `address`.
///
/// A registered server is keyed by its registry id. Anything else is keyed by its own
/// normalized `host:port`, which is stable, unforgeable by us, and the obvious thing for
/// the control plane to key presence on later — better than dropping the sync entirely
/// just because a player joined a server we don't run.
///
/// Both sides are normalized before comparing, so a registry row written as `10.0.0.5` and
/// an address pasted as `10.0.0.5:54210` are recognised as the same server.
pub fn server_key_for(servers: &[RegisteredServer], address: &str) -> String {
    let Ok(wanted) = crate::gameproc::parse_server_address(address) else {
        return address.trim().to_string();
    };
    servers
        .iter()
        .find(|s| {
            crate::gameproc::parse_server_address(&s.address)
                .map(|a| a.eq_ignore_ascii_case(&wanted))
                .unwrap_or(false)
        })
        .map(|s| s.id.clone())
        .unwrap_or(wanted)
}

#[derive(Deserialize)]
struct RosterRider {
    #[serde(rename = "riderName")]
    rider_name: String,
    #[serde(default)]
    guid: Option<String>,
    paints: Vec<PaintEntry>,
}

#[derive(Deserialize)]
struct Roster {
    riders: Vec<RosterRider>,
}

/// Identity for de-duplication: the GUID where the rider has claimed one, their name
/// otherwise. The same order of preference the control plane groups by, so the two agree
/// on what counts as one rider.
fn rider_key(rider: &RosterRider) -> String {
    match rider.guid.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
        Some(guid) => guid.to_string(),
        None => format!("name:{}", rider.rider_name.to_lowercase()),
    }
}

/// Fetch everyone's paints across `server_ids` and install the ones this machine is missing.
///
/// Takes a list rather than one id because a player who picks their server from the in-game
/// browser gives us nothing to aim at, so the app syncs every server they could land on.
/// Rosters overlap heavily — the same rider is on more than one, and riders share paints —
/// so everything is de-duplicated *before* any work happens. Without that, two servers means
/// hashing every local file twice and a report that double-counts what it did.
pub async fn pull(cfg: &AppConfig, token: &str, server_ids: &[String]) -> anyhow::Result<PullOutcome> {
    let http = client()?;

    let mut seen_riders: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Keyed by destination: two riders wearing the same paint is one file to write, and the
    // digest is part of the key so a genuine disagreement isn't silently collapsed.
    let mut wanted: Vec<PaintEntry> = Vec::new();
    let mut seen_paints: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut reached = 0usize;

    for server_id in server_ids {
        let roster: Roster = match http
            .get(format!("{CONTROL_PLANE}/v1/roster"))
            .query(&[("server", server_id.as_str())])
            .bearer_auth(token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => resp.json().await?,
            // One unreachable roster shouldn't sink the others — the player still wants the
            // paints for the servers that did answer.
            Err(e) => {
                log::warn!("[sync] roster for {server_id} failed: {e}");
                continue;
            }
        };
        reached += 1;
        for rider in roster.riders {
            seen_riders.insert(rider_key(&rider));
            for paint in rider.paints {
                if seen_paints.insert((paint.rel_dest.clone(), paint.sha256.clone())) {
                    wanted.push(paint);
                }
            }
        }
    }

    if reached == 0 && !server_ids.is_empty() {
        anyhow::bail!("couldn't read a roster from any server");
    }

    let mods_dir = crate::library::mods_root(&cfg.mods_path);
    let mut out = PullOutcome { riders: seen_riders.len(), ..Default::default() };

    for paint in &wanted {
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

    fn registry() -> Vec<RegisteredServer> {
        vec![
            RegisteredServer {
                id: "eu-frankfurt-1".into(),
                name: "EU 1".into(),
                region: "eu".into(),
                // Registered without a port, the way an address is usually published.
                address: "203.0.113.10".into(),
            },
            RegisteredServer {
                id: "us-east-1".into(),
                name: "US 1".into(),
                region: "us".into(),
                address: "198.51.100.7:54999".into(),
            },
        ]
    }

    #[test]
    fn matches_a_registered_server_across_port_notation() {
        // The whole point of normalizing both sides: these three all name EU 1.
        for addr in ["203.0.113.10", "203.0.113.10:54210", " 203.0.113.10:54210 "] {
            assert_eq!(server_key_for(&registry(), addr), "eu-frankfurt-1", "{addr:?}");
        }
        assert_eq!(server_key_for(&registry(), "198.51.100.7:54999"), "us-east-1");
    }

    #[test]
    fn a_non_default_port_is_a_different_server() {
        // Same host, another port is another server — it must not collapse onto EU 1.
        assert_eq!(server_key_for(&registry(), "203.0.113.10:54211"), "203.0.113.10:54211");
    }

    #[test]
    fn falls_back_to_the_normalized_address_when_unregistered() {
        assert_eq!(server_key_for(&registry(), "192.0.2.1"), "192.0.2.1:54210");
        assert_eq!(server_key_for(&[], "192.0.2.1:6000"), "192.0.2.1:6000");
    }

    #[test]
    fn an_unparseable_address_is_passed_through_rather_than_dropped() {
        // Nothing sane to resolve, but the caller still gets a key instead of a panic.
        assert_eq!(server_key_for(&registry(), "not a host"), "not a host");
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
