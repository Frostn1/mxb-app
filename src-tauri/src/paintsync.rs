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
use crate::presets;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Where the control plane lives. A constant rather than a setting: pointing the app at
/// another host would let anything served there write files into the mods folder.
pub const CONTROL_PLANE: &str = "https://mxb-control-plane.aui-svi.workers.dev";

/// Set to a base URL to talk to a control plane other than the real one. **Debug builds
/// only** — see [`control_plane`].
pub const CONTROL_PLANE_ENV: &str = "MXB_CONTROL_PLANE";

/// The control plane to talk to.
///
/// A shipped build always uses [`CONTROL_PLANE`] and there is no way to change it. That is
/// the security property this feature rests on: the responses become files written into the
/// game's mods folder, so anything that could redirect them is a way to put arbitrary
/// content on a player's disk. An environment variable is not a defence against anyone who
/// can already set environment variables, but it *is* one more way to end up pointed
/// somewhere unexpected — a stray value in a shell profile, a launcher, a shortcut — and
/// there is no reason a player would ever need it.
///
/// The override exists so the loop can be exercised against `wrangler dev` without pointing
/// a test run at the live accounts, and `cfg!(debug_assertions)` is what keeps it out of
/// anything anyone is handed.
pub fn control_plane() -> String {
    if cfg!(debug_assertions) {
        if let Ok(base) = std::env::var(CONTROL_PLANE_ENV) {
            let base = base.trim().trim_end_matches('/');
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    CONTROL_PLANE.to_string()
}

/// Only `.pnt` files are shared. Models are directories and often large, and none of the
/// non-paint slots carry a file a receiver could use.
const PAINT_EXT: &str = "pnt";

/// Does this path name a paint? Extension only — the bytes are the decoder's business.
pub fn is_paint(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(PAINT_EXT))
        .unwrap_or(false)
}

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
    /// How many bikes carried something worth publishing.
    pub bikes: usize,
    /// Bikes past [`MAX_BIKES`] that were left out.
    pub skipped_bikes: usize,
    /// Paints too large for the control plane to store, so nobody else will see them.
    pub oversized_paints: usize,
    /// Digest of what was sent, for the caller to remember — see [`LocalLook::digest`].
    pub digest: String,
    /// The look already matched `known_digest`, so nothing was sent.
    pub unchanged: bool,
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
    /// Paints left alone because that file is the player's own, not one we installed.
    pub kept_yours: usize,
    /// Destinations two riders disagreed about, where neither was installed.
    pub conflicted: usize,
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

/// One bike's paints, as the control plane stores them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeLoadout {
    pub bike_id: String,
    pub paints: Vec<PaintEntry>,
}

/// Everything a profile is wearing, across every bike it holds.
///
/// A `profile.ini` carries a column per bike the rider has ever sat on, and which one they
/// take out is decided in the game, not here. Publishing only the bike the app last touched
/// is why a rider could appear correctly on one bike and in default livery on the next.
pub struct LocalLook {
    pub bikes: Vec<BikeLoadout>,
    /// `sha256` → the file it was hashed from. Built during the walk so an upload never has
    /// to re-plan to find the bytes it already read.
    pub sources: std::collections::HashMap<String, PathBuf>,
    /// Bikes left out by [`MAX_BIKES`]. Reported rather than dropped quietly — a cap that
    /// silently covers less than it claims is indistinguishable from a bug.
    pub skipped: usize,
    /// Paints too large to publish. Named for the same reason: a rider whose livery never
    /// reaches anyone deserves to be told, not left wondering why they look default.
    pub oversized: usize,
}

impl LocalLook {
    /// A digest of exactly what would be sent.
    ///
    /// Publishing is hung off every path that could change a look, including a filesystem
    /// watcher that cannot tell our own writes from the game's. This is what makes that safe:
    /// an unchanged look hashes the same and the request is never made.
    pub fn digest(&self) -> String {
        let mut buf = String::new();
        for bike in &self.bikes {
            buf.push_str(&bike.bike_id);
            buf.push('\u{1}');
            for paint in &bike.paints {
                buf.push_str(&paint.slot);
                buf.push('\u{2}');
                buf.push_str(&paint.rel_dest);
                buf.push('\u{2}');
                buf.push_str(&paint.sha256);
                buf.push('\u{3}');
            }
        }
        sha256_bytes(buf.as_bytes())
    }

    pub fn total_paints(&self) -> usize {
        self.bikes.iter().map(|b| b.paints.len()).sum()
    }
}

/// How many bikes one account may publish.
///
/// Matches the control plane's cap. A profile with more than this is vanishingly rare; the
/// bikes kept are the active one and then the rest in file order, so the bike the rider is
/// actually on is never the one dropped.
pub const MAX_BIKES: usize = 32;

/// Read every bike in a profile and hash what each one is wearing.
///
/// One `profile.ini` parse ([`presets::read_all_loadouts`]) and one library walk
/// ([`bundle::plan_profile`]) for the whole profile — doing either per bike turns publishing
/// into dozens of recursive scans of a folder holding every livery the player owns.
pub fn local_look(cfg: &AppConfig, profile: &str) -> anyhow::Result<LocalLook> {
    let profiles_dir = cfg.profiles_dir();
    let mut loadouts = presets::read_all_loadouts(&profiles_dir, profile)?;

    // The active bike leads, so a profile over the cap keeps the one being ridden.
    if let Some(active) = presets::active_bike(&profiles_dir, profile) {
        if let Some(at) = loadouts.iter().position(|(b, _)| b.eq_ignore_ascii_case(&active)) {
            loadouts.swap(0, at);
        }
    }
    let skipped = loadouts.len().saturating_sub(MAX_BIKES);
    loadouts.truncate(MAX_BIKES);

    let plans = bundle::plan_profile(cfg, &loadouts);

    let mut sources = std::collections::HashMap::new();
    let mut oversized = 0usize;
    let mut bikes = Vec::new();
    for ((bike_id, _), plan) in loadouts.into_iter().zip(plans) {
        let paints = paints_of(&plan, &mut sources, &mut oversized);
        // A bike with nothing custom on it is not worth a row; the receiver would install
        // nothing and the roster would carry an empty entry for every bike ever ridden.
        if paints.is_empty() {
            continue;
        }
        bikes.push(BikeLoadout { bike_id, paints });
    }
    Ok(LocalLook { bikes, sources, skipped, oversized })
}

/// The slots the control plane stores.
///
/// A wire contract, duplicated here rather than shared: the two ship separately and update
/// independently, exactly like the agent's pairing code. `bundle::plan` also resolves slots
/// that are models rather than paints — a helmet, boots, a tyre set — and while those are
/// usually folders or archives and get filtered out by extension, a `.pnt` sitting in one of
/// those categories would be sent and refused, taking the whole publish with it.
/// The largest paint the control plane will store, mirrored here for the same reason as
/// [`PUBLISHABLE_SLOTS`]: a loadout is validated whole, so one file over the limit is a rider
/// publishing nothing at all. Skipping it costs that one paint; sending it costs the lot.
const MAX_PAINT_BYTES: u64 = 192 * 1024 * 1024;

const PUBLISHABLE_SLOTS: [&str; 9] = [
    "paint",
    "bike_font",
    "helmet_paint",
    "goggles_paint",
    "suit_paint",
    "suit_font",
    "boots_paint",
    "gloves_paint",
    "protection_paint",
];

/// The `.pnt` files in a plan, hashed, recording where each digest came from.
fn paints_of(
    plan: &bundle::BundlePlan,
    sources: &mut std::collections::HashMap<String, PathBuf>,
    oversized: &mut usize,
) -> Vec<PaintEntry> {
    let mut out = Vec::new();
    for asset in &plan.assets {
        if asset.is_dir {
            continue;
        }
        let path = Path::new(&asset.abs_path);
        if !is_paint(path) {
            continue;
        }
        // A slot the control plane does not store is not worth failing a publish over.
        if !PUBLISHABLE_SLOTS.contains(&asset.slot.as_str()) {
            continue;
        }
        // One paint per slot. A loadout names one file per slot, but resolution can match
        // the same name in two places — a livery installed both loose and in a pack — and
        // the control plane keys on (account, bike, slot), so a second one is rejected and
        // the rider publishes nothing at all. First match wins, as it does in the game.
        if out.iter().any(|e: &PaintEntry| e.slot == asset.slot) {
            log::debug!("[sync] {} matched twice; keeping the first", asset.slot);
            continue;
        }
        if asset.size > MAX_PAINT_BYTES {
            log::warn!(
                "[sync] {} is {} MB, past the {} MB limit — not published",
                asset.name,
                asset.size / 1_048_576,
                MAX_PAINT_BYTES / 1_048_576
            );
            *oversized += 1;
            continue;
        }
        let Ok(sha) = sha256_file(path) else { continue };
        sources.entry(sha.clone()).or_insert_with(|| path.to_path_buf());
        out.push(PaintEntry {
            slot: asset.slot.clone(),
            file_name: asset.name.clone(),
            sha256: sha,
            size: asset.size,
            rel_dest: asset.rel_dest.clone(),
        });
    }
    out
}

// ── Control-plane calls ──────────────────────────────────────────────────────

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

/// Publish everything this rider is wearing, then upload only the blobs nobody has shared yet.
///
/// `known_digest` is what was last published from this machine. When the look hashes the
/// same, nothing is sent — which is what lets every path that could possibly change a look
/// call this without anyone having to work out whether it really did.
pub async fn publish_all(
    cfg: &AppConfig,
    token: &str,
    profile: &str,
    known_digest: Option<&str>,
) -> anyhow::Result<PublishOutcome> {
    let look = local_look(cfg, profile)?;
    let digest = look.digest();
    let published = look.total_paints();

    if known_digest == Some(digest.as_str()) {
        return Ok(PublishOutcome {
            published,
            uploaded: 0,
            bikes: look.bikes.len(),
            skipped_bikes: look.skipped,
            oversized_paints: look.oversized,
            digest,
            unchanged: true,
        });
    }
    if look.skipped > 0 {
        log::warn!(
            "[sync] {profile} has more than {MAX_BIKES} bikes; {} were not published",
            look.skipped
        );
    }

    let http = client()?;

    #[derive(Deserialize)]
    struct Resp {
        missing: Vec<String>,
    }
    let resp = http
        .put(format!("{}/v1/loadouts", control_plane()))
        .bearer_auth(token)
        .json(&serde_json::json!({ "bikes": look.bikes }))
        .send()
        .await?;
    if !resp.status().is_success() {
        // Carry the reason, not just the number. "400 Bad Request" says a payload was
        // wrong; the body says which slot, which bike, and why — and without it a rejected
        // publish is an investigation rather than a sentence.
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        let reason = serde_json::from_str::<serde_json::Value>(&detail)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(detail);
        anyhow::bail!("the control plane refused the loadout ({status}): {reason}");
    }
    let missing: Resp = resp.json().await?;

    let mut uploaded = 0usize;
    for sha in missing.missing {
        // Taken from the map built while hashing, so the bytes uploaded are the bytes that
        // produced the digest — never a path round-tripped through the server.
        let Some(path) = look.sources.get(&sha) else { continue };
        let bytes = std::fs::read(path)?;
        let up = http
            .put(format!("{}/v1/paints/{sha}", control_plane()))
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

    Ok(PublishOutcome {
        published,
        uploaded,
        bikes: look.bikes.len(),
        skipped_bikes: look.skipped,
        oversized_paints: look.oversized,
        digest,
        unchanged: false,
    })
}

/// Tell the control plane which server this rider is on.
///
/// The roster is scoped by this. Without it the control plane has no idea where anyone is, so
/// a roster can only mean "everyone enrolled" — which is what it used to mean, and why every
/// rider downloaded the paints of every other rider on the platform.
///
/// Best-effort: failing to report presence costs a narrower roster for a minute, and must
/// never be the thing that stops a sync.
pub async fn report_presence(token: &str, server_id: &str) -> anyhow::Result<()> {
    client()?
        .put(format!("{}/v1/presence", control_plane()))
        .bearer_auth(token)
        .json(&serde_json::json!({ "serverId": server_id }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
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
    let mut req = client()?.get(format!("{}/v1/servers", control_plane()));
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
            .get(format!("{}/v1/roster", control_plane()))
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
    let mut manifest = Manifest::read(&mods_dir);
    let mut out = PullOutcome { riders: seen_riders.len(), ..Default::default() };

    // Two riders whose paints have the same file name but different bytes cannot both be
    // installed — the game matches by name, so one destination holds one paint. Settled
    // before anything is written, because installing whichever arrived last is silently
    // choosing for the player.
    let contested = contested_destinations(&wanted);

    for paint in &wanted {
        let Some(dest) = safe_dest(&mods_dir, &paint.rel_dest) else {
            // Refused rather than sanitised: a destination we had to rewrite is one we
            // don't understand, and guessing writes someone's file somewhere odd.
            log::warn!("refusing paint destination {:?}", paint.rel_dest);
            out.rejected += 1;
            continue;
        };
        if contested.contains(&paint.rel_dest.to_ascii_lowercase()) {
            log::warn!("[sync] riders disagree about {:?}; installing neither", paint.rel_dest);
            out.conflicted += 1;
            continue;
        }
        if dest.is_file() {
            let held = sha256_file(&dest).ok();
            if held.as_deref() == Some(paint.sha256.as_str()) {
                out.already_had += 1;
                // Claim it: an identical file is one we'd have written anyway, and recording
                // it means a later change by this player is still recognised as theirs.
                manifest.claim(&paint.rel_dest, &paint.sha256);
                continue;
            }
            // The file is there and is not what the roster says. If we didn't put it there —
            // or the player has since edited what we did — it is theirs, and a sync that
            // overwrites the paints someone made is worse than one that installs nothing.
            if !manifest.owns(&paint.rel_dest, held.as_deref()) {
                log::info!("[sync] keeping your own {:?} rather than replacing it", paint.rel_dest);
                out.kept_yours += 1;
                continue;
            }
        }

        let bytes = http
            .get(format!("{}/v1/paints/{}", control_plane(), paint.sha256))
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
        manifest.claim(&paint.rel_dest, &paint.sha256);
        out.installed += 1;
    }
    manifest.write(&mods_dir);
    Ok(out)
}

/// Destinations more than one digest wants, lowercased for comparison.
///
/// MX Bikes picks a remote rider's paint by the file name they chose, so two riders using
/// the same name for different artwork is a collision the game itself cannot express. What
/// this decides is only that neither is installed — better than a grid that changes
/// depending on which roster came back first.
fn contested_destinations(wanted: &[PaintEntry]) -> std::collections::HashSet<String> {
    let mut first: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut contested = std::collections::HashSet::new();
    for paint in wanted {
        let key = paint.rel_dest.to_ascii_lowercase();
        match first.get(&key) {
            Some(sha) if sha != &paint.sha256 => {
                contested.insert(key);
            }
            Some(_) => {}
            None => {
                first.insert(key, paint.sha256.clone());
            }
        }
    }
    contested
}

/// What the sync has installed, so it knows which files are its to replace.
///
/// The same idea as `modelswap`'s `_files.txt`: an operation that puts files somewhere has
/// to record which ones, or it can never tell its own work from the player's. Without it,
/// pulling a roster overwrote any local paint whose name matched — including liveries the
/// player made themselves, which is the one thing a sync must never do.
#[derive(Debug, Default)]
struct Manifest {
    /// `rel_dest` (lowercased) → the digest this sync last wrote there.
    installed: std::collections::HashMap<String, String>,
    dirty: bool,
}

/// Sits beside the content it describes, so moving or wiping the mods folder takes it too.
const MANIFEST_NAME: &str = "mxbapp_synced.json";

impl Manifest {
    fn read(mods_dir: &Path) -> Self {
        let path = mods_dir.join(MANIFEST_NAME);
        let installed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Manifest { installed, dirty: false }
    }

    /// Whether the file at `rel_dest` is one this sync put there and nobody has touched
    /// since. An edited file stops being ours — the player has made it theirs.
    fn owns(&self, rel_dest: &str, on_disk: Option<&str>) -> bool {
        match (self.installed.get(&rel_dest.to_ascii_lowercase()), on_disk) {
            (Some(recorded), Some(actual)) => recorded == actual,
            _ => false,
        }
    }

    fn claim(&mut self, rel_dest: &str, sha: &str) {
        let key = rel_dest.to_ascii_lowercase();
        if self.installed.get(&key).map(String::as_str) != Some(sha) {
            self.installed.insert(key, sha.to_string());
            self.dirty = true;
        }
    }

    fn write(&self, mods_dir: &Path) {
        if !self.dirty {
            return;
        }
        let path = mods_dir.join(MANIFEST_NAME);
        let Ok(text) = serde_json::to_string_pretty(&self.installed) else { return };
        if let Err(e) = std::fs::write(&path, text) {
            // Not fatal: the paints are on disk and correct. The cost of losing this is
            // that a future sync treats them as the player's and declines to update them.
            log::warn!("[sync] couldn't write {}: {e}", path.display());
        }
    }
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

    fn entry(rel_dest: &str, sha: &str) -> PaintEntry {
        PaintEntry {
            slot: "paint".into(),
            file_name: rel_dest.rsplit('/').next().unwrap_or(rel_dest).into(),
            sha256: sha.into(),
            size: 1,
            rel_dest: rel_dest.into(),
        }
    }

    // MX Bikes picks a remote rider's paint by the name they chose, so one destination can
    // hold one paint. Installing whichever roster answered last would make the grid depend
    // on network ordering.
    #[test]
    fn two_riders_wanting_different_bytes_at_one_name_get_neither() {
        let wanted = vec![
            entry("bikes/KTM450/paints/Red.pnt", "aa"),
            entry("bikes/KTM450/paints/Red.pnt", "bb"),
            entry("bikes/KTM450/paints/Blue.pnt", "cc"),
        ];
        let contested = contested_destinations(&wanted);
        assert!(contested.contains("bikes/ktm450/paints/red.pnt"));
        assert!(!contested.contains("bikes/ktm450/paints/blue.pnt"));
    }

    #[test]
    fn riders_wearing_the_very_same_paint_are_not_a_conflict() {
        // The common case, and the whole reason content addressing is worth having.
        let wanted =
            vec![entry("bikes/KTM450/paints/Red.pnt", "aa"), entry("bikes/KTM450/paints/Red.pnt", "aa")];
        assert!(contested_destinations(&wanted).is_empty());
    }

    // The manifest is what stops a sync overwriting a livery the player made. Anything it
    // did not install — or that has changed since it did — belongs to them.
    #[test]
    fn the_sync_only_owns_files_it_wrote_and_nobody_has_touched() {
        let mut m = Manifest::default();
        let dest = "bikes/KTM450/paints/Red.pnt";

        assert!(!m.owns(dest, Some("aa")), "a file we never wrote is the player's");
        m.claim(dest, "aa");
        assert!(m.owns(dest, Some("aa")), "ours, untouched");
        assert!(!m.owns(dest, Some("zz")), "ours, but they've edited it since");
        assert!(!m.owns(dest, None), "nothing on disk to own");
    }

    #[test]
    fn ownership_survives_the_case_the_destination_was_written_in() {
        // `rel_dest` comes from another player's machine; the same file can arrive spelled
        // either way and must still be recognised.
        let mut m = Manifest::default();
        m.claim("Bikes/KTM450/Paints/Red.pnt", "aa");
        assert!(m.owns("bikes/ktm450/paints/red.pnt", Some("aa")));
    }

    #[test]
    fn a_manifest_nobody_changed_is_not_rewritten() {
        let mut m = Manifest::default();
        m.claim("bikes/KTM450/paints/Red.pnt", "aa");
        assert!(m.dirty);
        let mut same = Manifest { installed: m.installed.clone(), dirty: false };
        same.claim("bikes/KTM450/paints/Red.pnt", "aa");
        assert!(!same.dirty, "re-claiming what we already recorded is not a change");
    }

    fn look(bikes: Vec<BikeLoadout>) -> LocalLook {
        LocalLook { bikes, sources: std::collections::HashMap::new(), skipped: 0, oversized: 0 }
    }

    // The digest is what lets every path that might have changed a look publish without
    // checking first, so it has to be exact in both directions.
    #[test]
    fn the_same_look_hashes_the_same_and_a_changed_one_does_not() {
        let one = || {
            look(vec![BikeLoadout {
                bike_id: "KTM450".into(),
                paints: vec![entry("bikes/KTM450/paints/Red.pnt", "aa")],
            }])
        };
        assert_eq!(one().digest(), one().digest());

        let repainted = look(vec![BikeLoadout {
            bike_id: "KTM450".into(),
            paints: vec![entry("bikes/KTM450/paints/Red.pnt", "bb")],
        }]);
        assert_ne!(one().digest(), repainted.digest(), "different bytes, different look");

        let second_bike = look(vec![
            BikeLoadout {
                bike_id: "KTM450".into(),
                paints: vec![entry("bikes/KTM450/paints/Red.pnt", "aa")],
            },
            BikeLoadout {
                bike_id: "YZ250".into(),
                paints: vec![entry("bikes/YZ250/paints/Blue.pnt", "cc")],
            },
        ]);
        assert_ne!(one().digest(), second_bike.digest(), "a bike gained is a change");
    }

    #[test]
    fn the_same_paint_on_two_bikes_is_not_the_same_as_one_bike_with_two() {
        // The separators exist for this: without them the flattened fields of one shape
        // could read identically to the other, and a real change would publish nothing.
        let split = look(vec![
            BikeLoadout { bike_id: "A".into(), paints: vec![entry("bikes/A/paints/x.pnt", "aa")] },
            BikeLoadout { bike_id: "B".into(), paints: vec![entry("bikes/B/paints/y.pnt", "bb")] },
        ]);
        let together = look(vec![BikeLoadout {
            bike_id: "A".into(),
            paints: vec![entry("bikes/A/paints/x.pnt", "aa"), entry("bikes/B/paints/y.pnt", "bb")],
        }]);
        assert_ne!(split.digest(), together.digest());
    }

    fn asset(slot: &str, name: &str) -> bundle::AssetRef {
        bundle::AssetRef {
            slot: slot.into(),
            value: name.into(),
            name: format!("{name}.pnt"),
            rel_dest: format!("bikes/KTM450/paints/{name}.pnt"),
            abs_path: format!("/nowhere/{name}.pnt"),
            size: 1,
            is_dir: false,
        }
    }

    // The publish is rejected whole if any entry is bad, so a rider with one odd file
    // published nothing at all. Both of these came from a real install.
    #[test]
    fn a_slot_the_control_plane_does_not_store_is_left_out() {
        // `bundle::plan` resolves models as well as paints. A `.pnt` filed under one of
        // those categories would be sent, refused, and take the whole loadout with it.
        let plan = bundle::BundlePlan {
            assets: vec![asset("helmet", "Shell"), asset("tyres", "Mud")],
            unresolved: Vec::new(),
            total_size: 0,
        };
        let mut sources = std::collections::HashMap::new();
        let mut big = 0;
        assert!(paints_of(&plan, &mut sources, &mut big).is_empty());
    }

    #[test]
    fn one_paint_per_slot_even_when_a_name_matches_twice() {
        // The same livery installed loose and inside a pack resolves twice. The control
        // plane keys on (account, bike, slot) and refuses the second, so this must not
        // reach it.
        let plan = bundle::BundlePlan {
            assets: vec![asset("paint", "RedBud"), asset("paint", "RedBud")],
            unresolved: Vec::new(),
            total_size: 0,
        };
        let mut sources = std::collections::HashMap::new();
        let mut big = 0;
        // Hashing a path that does not exist drops the entry, so this asserts the guard
        // rather than the file work: at most one `paint` survives either way.
        assert!(paints_of(&plan, &mut sources, &mut big).len() <= 1);
    }

    // Four paints on a normal install are past the old 32 MiB limit, the largest 121.7 MB.
    // A loadout is validated whole, so one of them meant the rider published nothing at all.
    #[test]
    fn a_paint_too_large_to_store_is_left_behind_rather_than_failing_the_lot() {
        let mut huge = asset("paint", "Enormous");
        huge.size = MAX_PAINT_BYTES + 1;
        let plan = bundle::BundlePlan {
            assets: vec![huge, asset("helmet_paint", "Normal")],
            unresolved: Vec::new(),
            total_size: 0,
        };
        let mut sources = std::collections::HashMap::new();
        let mut oversized = 0;
        paints_of(&plan, &mut sources, &mut oversized);
        assert_eq!(oversized, 1, "the outsized paint must be counted, not silently dropped");
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

/// End-to-end checks against a running control plane.
///
/// `#[ignore]`d because they need one: start `wrangler dev` in `control-plane/`, then
///
/// ```sh
/// MXB_CONTROL_PLANE=http://127.0.0.1:8799 MXB_TEST_TOKEN=<a token> \
///   cargo test --locked live_sync -- --ignored --nocapture
/// ```
///
/// The unit tests above prove the manifest's decisions in isolation; this proves the wiring
/// around them — that a real roster, a real download and a real file on disk reach the same
/// answer.
#[cfg(test)]
mod live_sync {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mxb-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn token() -> String {
        std::env::var("MXB_TEST_TOKEN").expect("MXB_TEST_TOKEN must name an enrolled account")
    }

    /// The failure this whole manifest exists to prevent: a rider publishing a different
    /// paint under a name you already use, and the sync quietly replacing your artwork.
    #[tokio::test]
    #[ignore = "needs a running control plane"]
    async fn a_pull_keeps_your_own_paint_and_installs_the_ones_you_lack() {
        let root = scratch("pull");
        let mods = root.join("mods");
        let mine = mods.join("rider/riders/default_mx/gloves/26 FOX Flexair (Black).pnt");
        std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
        std::fs::write(&mine, b"my own artwork").unwrap();
        let before = std::fs::read(&mine).unwrap();

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let out = pull(&cfg, &token(), &["local".to_string()]).await.unwrap();
        println!("{out:?}");

        assert_eq!(std::fs::read(&mine).unwrap(), before, "our own paint must be untouched");
        assert!(out.kept_yours >= 1, "the clash must be reported, not silent: {out:?}");
        assert!(out.installed >= 1, "a paint we didn't have must still arrive: {out:?}");

        // Everything installed is recorded, so a later sync knows it may replace it — and
        // knows the file above is not its to touch.
        let manifest = Manifest::read(&mods);
        assert!(
            !manifest.owns("rider/riders/default_mx/gloves/26 FOX Flexair (Black).pnt", None),
            "a file we declined to write must never be claimed"
        );

        // Second run: nothing new, and still no damage.
        let again = pull(&cfg, &token(), &["local".to_string()]).await.unwrap();
        assert_eq!(again.installed, 0, "a repeat sync installs nothing: {again:?}");
        assert!(again.already_had >= 1, "what we installed is recognised: {again:?}");
        assert_eq!(std::fs::read(&mine).unwrap(), before);

        let _ = std::fs::remove_dir_all(&root);
    }
}
