//! What the game's own memory says about itself — the client half.
//!
//! The rest of [`crate::procmods`] identifies foreign code by what it *is*: a file hash, a
//! region fingerprint, a thread that started nowhere. This is the one part that describes what
//! that code *changed*. A trainer that writes one physics coefficient and unloads is otherwise
//! a clean report — nothing was loaded that a rule can name — and this is the only signal in
//! the pipeline that fires on it, and on a build nobody has ever hashed.
//!
//! It is the mirror of `control-plane/src/stateinvariants.ts`, and the split is the same:
//!
//!   * **The client is told where to read, never what to expect.** It asks the control plane
//!     for the manifest that matches the build it is running (`GET /v1/diagnostics/state-regions`),
//!     hashes exactly the bytes it names, and reports the digests. The baseline a clean install
//!     answers lives only in the control plane's table, so the shipped binary still looks for
//!     nothing — a `strings` of it names no region and holds no answer.
//!   * **A digest equal to its baseline produces nothing.** The control plane keeps only the
//!     ones that differ, so the overwhelmingly common case — every clean machine — writes no
//!     row. The client cannot tell which is which; it hashes and reports the lot.
//!   * **A deviation is `warn`, not `alert`.** MX Bikes is a modding game, and a mod that
//!     legitimately rewrites a physics table looks exactly like a trainer that does. Only a
//!     rule, informed by prevalence, promotes one — which is the control plane's job, not this.
//!
//! Cheap by construction: the manifest is small and changes only when the game is patched or an
//! admin edits it, so it is fetched once per build and cached; each region is a handful of bytes
//! read from the game the app is already probing.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One region's digest, as it goes on the wire: `{ "name": ..., "digest": ... }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Digest {
    /// The region's name, e.g. `physics-coefficients`. Named by the manifest, echoed back so
    /// the control plane can line a digest up with the baseline it holds.
    pub name: String,
    /// SHA-256 of the region's bytes, lowercase hex. The baseline is captured the same way, so
    /// the two are compared as plain strings.
    pub digest: String,
}

/// A region the manifest asks the client to hash: `base + rva` for `length` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRegion {
    pub name: String,
    /// Offset from the game module's base. A static global — the manifest addresses the
    /// image, not the heap, so it is the same address in every copy of a build.
    pub rva: u64,
    pub length: usize,
}

/// The most regions one manifest may carry. The control plane caps the same number.
const MAX_STATE_REGIONS: usize = 32;

/// The most bytes one region may be. Matches the game-probe read cap; a baseline over more
/// than this could never be reproduced by a single read anyway.
const MAX_REGION_LEN: usize = 64 * 1024;

/// A region name as the control plane writes it: lowercase, and short enough to be a column.
const NAME_MAX: usize = 48;

/// How long a fetched manifest is trusted before it is refreshed. It changes only on a game
/// patch or an admin edit, so this is about not re-fetching every session, not freshness.
const MANIFEST_TTL: Duration = Duration::from_secs(3600);

/// How long to wait before trying again after a failed or in-flight fetch, so a control plane
/// that is down is asked once in a while rather than on every pass.
const REFRESH_BACKOFF: Duration = Duration::from_secs(60);

/// The manifest currently held, for the build it was fetched for.
struct Cached {
    build_fp: String,
    regions: Vec<StateRegion>,
    at: Instant,
}

fn cache() -> &'static Mutex<Option<Cached>> {
    static CACHE: std::sync::OnceLock<Mutex<Option<Cached>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// One fetch at a time. A due tick that finds the manifest stale kicks a background refresh;
/// this keeps the next few ticks from kicking their own before the first has answered.
fn refreshing() -> &'static AtomicBool {
    static FLAG: AtomicBool = AtomicBool::new(false);
    &FLAG
}

/// When the last refresh was kicked, so a control plane that is unreachable is retried on a
/// backoff rather than on every pass.
fn last_attempt() -> &'static Mutex<Option<Instant>> {
    static AT: Mutex<Option<Instant>> = Mutex::new(None);
    &AT
}

/// Forget the cached manifest and the last-sent state. Called when the game goes away so the
/// next session fetches afresh rather than hashing against a manifest for a build that may no
/// longer be the one running.
pub fn reset() {
    if let Ok(mut slot) = cache().lock() {
        *slot = None;
    }
    if let Ok(mut slot) = last_attempt().lock() {
        *slot = None;
    }
    refreshing().store(false, Ordering::SeqCst);
}

/// The running build's fingerprint and the digests of the regions its manifest names.
///
/// Returns `("", [])` when there is nothing to look at — no game, or a platform that cannot
/// read the game's memory. The fingerprint is returned as soon as it is known even when there
/// is nothing to hash yet (a manifest not fetched, no baselined regions), because it costs
/// nothing and the control plane ignores a `build` that arrives with no digests.
///
/// One process handle does all of it: the PE header for the fingerprint, and the named regions
/// for the digests. The fingerprint is the same one [`crate::peident`] produces for any mapped
/// image, so it is exactly what the control plane's `state_regions.build_fp` is keyed on.
pub fn look(token: &str) -> (String, Vec<Digest>) {
    let Some(probe) = crate::gameproc::GameProbe::open() else {
        return (String::new(), Vec::new());
    };
    // The first module a snapshot lists is the process's own executable — the game itself.
    let Some(base) = probe.module_ranges().first().map(|m| m.base) else {
        return (String::new(), Vec::new());
    };
    let build_fp =
        match crate::peident::identify(&|rva, len| probe.read(base.saturating_add(rva), len)) {
            Some(ident) if ident.is_useful() => ident.fingerprint(),
            _ => return (String::new(), Vec::new()),
        };

    let regions = match manifest(token, &build_fp) {
        Some(regions) => regions,
        None => return (build_fp, Vec::new()),
    };

    let mut out = Vec::with_capacity(regions.len());
    for region in &regions {
        let Some(bytes) = probe.read(base.saturating_add(region.rva), region.length) else {
            continue;
        };
        // A short read is a region that is not what the manifest thought it was — unmapped
        // past some point, or smaller than the baseline was taken over. A digest over fewer
        // bytes would mismatch every time and read as a deviation on every clean machine, so
        // it is dropped rather than sent.
        if bytes.len() != region.length {
            continue;
        }
        out.push(Digest { name: region.name.clone(), digest: digest_bytes(&bytes) });
    }
    (build_fp, out)
}

/// The manifest for `build_fp`, from cache — or `None` while a background fetch runs.
///
/// Never blocks the caller on the network: a miss kicks a refresh and answers `None`, so the
/// pass reports no digests and the next one, once the fetch has landed, reports them.
fn manifest(token: &str, build_fp: &str) -> Option<Vec<StateRegion>> {
    if build_fp.is_empty() || token.is_empty() {
        return None;
    }
    if let Ok(guard) = cache().lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.build_fp == build_fp && cached.at.elapsed() < MANIFEST_TTL {
                return Some(cached.regions.clone());
            }
        }
    }
    kick_refresh(token, build_fp);
    None
}

/// Start a background fetch of the manifest, at most one at a time and no more often than the
/// backoff. Cross-platform: `tauri::async_runtime::spawn` is what [`crate::procmods`] already
/// uses to send a report without blocking its tick.
fn kick_refresh(token: &str, build_fp: &str) {
    if let Ok(guard) = last_attempt().lock() {
        if let Some(at) = guard.as_ref() {
            if at.elapsed() < REFRESH_BACKOFF {
                return;
            }
        }
    }
    if refreshing().swap(true, Ordering::SeqCst) {
        return;
    }
    if let Ok(mut guard) = last_attempt().lock() {
        *guard = Some(Instant::now());
    }
    let token = token.to_string();
    let build_fp = build_fp.to_string();
    tauri::async_runtime::spawn(async move {
        match fetch(&token, &build_fp).await {
            Ok(regions) => {
                if let Ok(mut guard) = cache().lock() {
                    *guard = Some(Cached { build_fp, regions, at: Instant::now() });
                }
            }
            Err(e) => log::debug!("[state] manifest not fetched: {e:#}"),
        }
        refreshing().store(false, Ordering::SeqCst);
    });
}

/// Ask the control plane which bytes this build should hash.
async fn fetch(token: &str, build_fp: &str) -> anyhow::Result<Vec<StateRegion>> {
    let url = format!(
        "{}/v1/diagnostics/state-regions?build={}",
        crate::paintsync::control_plane(),
        build_fp
    );
    let res = reqwest::Client::new()
        .get(url)
        .bearer_auth(token)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("control plane said {}", res.status());
    }
    let body: serde_json::Value = res.json().await?;
    Ok(parse_manifest(&body))
}

/// SHA-256 of a run of bytes, lowercase hex. The one place the digest algorithm is defined;
/// whoever captures a baseline must use the same, which is why it is the plainest thing it
/// could be.
fn digest_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Read `{ "regions": [ { name, rva, length }, ... ] }` into the regions to hash.
///
/// This is our own control plane answering, but it is still parsed defensively: bounded on
/// count and length, names held to the shape the control plane writes them in, and anything
/// malformed simply dropped. A bad manifest costs a pass with no digests, never a panic.
pub fn parse_manifest(body: &serde_json::Value) -> Vec<StateRegion> {
    let Some(list) = body.get("regions").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out: Vec<StateRegion> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in list.iter().take(MAX_STATE_REGIONS * 2) {
        if out.len() >= MAX_STATE_REGIONS {
            break;
        }
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let name = name.to_ascii_lowercase();
        if !name_is_shaped(&name) {
            continue;
        }
        let rva = entry.get("rva").and_then(|v| v.as_u64());
        let length = entry.get("length").and_then(|v| v.as_u64());
        let (Some(rva), Some(length)) = (rva, length) else {
            continue;
        };
        let length = length as usize;
        if length == 0 || length > MAX_REGION_LEN {
            continue;
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push(StateRegion { name, rva, length });
    }
    out
}

/// A region name as the control plane writes it: `[a-z0-9._-]`, 1..=48 characters.
fn name_is_shaped(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_well_formed_manifest_is_read() {
        let body = json!({ "regions": [
            { "name": "physics-coefficients", "rva": 0xe5522c, "length": 256 },
            { "name": "unlock-flags", "rva": 0x1000, "length": 4 },
        ]});
        let regions = parse_manifest(&body);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "physics-coefficients");
        assert_eq!(regions[0].rva, 0xe5522c);
        assert_eq!(regions[0].length, 256);
    }

    #[test]
    fn a_missing_or_wrong_shaped_manifest_is_empty_not_a_panic() {
        assert!(parse_manifest(&json!({})).is_empty());
        assert!(parse_manifest(&json!({ "regions": "nope" })).is_empty());
        assert!(parse_manifest(&json!([])).is_empty());
    }

    #[test]
    fn a_region_is_dropped_when_it_is_out_of_shape() {
        let body = json!({ "regions": [
            { "name": "UPPER_and_space bad", "rva": 1, "length": 4 },
            { "name": "no-length", "rva": 1 },
            { "name": "no-rva", "length": 4 },
            { "name": "zero-length", "rva": 1, "length": 0 },
            { "name": "too-long", "rva": 1, "length": 64 * 1024 + 1 },
            { "name": "ok", "rva": 1, "length": 4 },
        ]});
        let regions = parse_manifest(&body);
        assert_eq!(regions.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["ok"]);
    }

    #[test]
    fn the_same_region_named_twice_is_kept_once() {
        let body = json!({ "regions": [
            { "name": "dup", "rva": 1, "length": 4 },
            { "name": "dup", "rva": 2, "length": 8 },
        ]});
        let regions = parse_manifest(&body);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].rva, 1, "the first wins, the second is dropped");
    }

    #[test]
    fn a_manifest_longer_than_the_cap_is_bounded() {
        let many: Vec<_> = (0..100)
            .map(|i| json!({ "name": format!("r{i}"), "rva": i, "length": 4 }))
            .collect();
        let regions = parse_manifest(&json!({ "regions": many }));
        assert_eq!(regions.len(), MAX_STATE_REGIONS);
    }

    #[test]
    fn a_digest_is_stable_and_tells_bytes_apart() {
        let a = digest_bytes(&[1, 2, 3, 4]);
        assert_eq!(a, digest_bytes(&[1, 2, 3, 4]), "same bytes, same digest");
        assert_ne!(a, digest_bytes(&[1, 2, 3, 5]), "one byte moved is a different digest");
        assert_eq!(a.len(), 64, "sha-256 hex");
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn a_digest_matches_the_shape_the_control_plane_accepts() {
        // control-plane DIGEST_SHAPE is /^[a-f0-9]{16,128}$/.
        let d = digest_bytes(b"whatever");
        assert!((16..=128).contains(&d.len()));
        assert!(d.bytes().all(|b| matches!(b, b'a'..=b'f' | b'0'..=b'9')));
    }
}
