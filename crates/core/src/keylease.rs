//! Key leases: the signed permission the secure-content DLL needs beside a `.mxbkey` before it
//! will unseal it — the app half of the control plane's `POST /v1/keys/lease` (`lease.ts`).
//!
//! A `.mxbkey` is sealed to the buyer's Steam ID and PC and opens with no server. That is what
//! lets a buyer play offline, and it used to be what let a banned install keep playing everything
//! it had unlocked for as long as it stayed offline. So the DLL now also wants a lease: a small
//! Ed25519-signed statement naming the Steam account and an expiry 30 days out. This module keeps
//! one on disk and fresh:
//!
//!  - renewed silently whenever the app is online and the one kept has fewer than
//!    [`RENEW_BELOW`] left, is for another Steam account, or is missing;
//!  - kept as it is when the control plane cannot be reached, answers 409 (no Steam link yet) or
//!    503 (the deployment has no signing key) — not knowing is never a reason to delete one;
//!  - deleted when the answer is a 403 with `code: "blocked"`, which is how a ban reaches keys
//!    already on disk even while the app stays closed: nothing renews the lease, and it runs out.
//!
//! For the buyer: **play offline for up to 30 days between check-ins; the app renews silently
//! whenever it is online.**
//!
//! Nothing here verifies the signature — the DLL does, against the public key it was built with,
//! and a lease it cannot verify simply unseals nothing. The app only reads the expiry and the
//! Steam ID to decide when to ask again, so a hand-edited file costs its editor a renewal, not a
//! bypass.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::appgate;
use crate::names::control_plane;

/// The file the DLL reads, beside its `manifest.tsv`.
pub const LEASE_FILE: &str = "lease.json";

/// The domain tag every lease carries. A signed gate verdict has none, so neither is mistaken for
/// the other even though one key signs both.
pub const LEASE_PURPOSE: &str = "mxbsecure-lease";

/// Renew once fewer than this many days are left on the kept lease. Leases run 30 days, so an app
/// that is online at least every few days never lets one get near the end.
pub const RENEW_BELOW: Duration = Duration::from_secs(25 * 24 * 60 * 60);

/// How often a running app looks at its lease. Asking is free when there is nothing to renew.
pub const CHECK_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// A lease as the control plane hands it out and the DLL reads it: the exact string signed, and
/// an Ed25519 signature over its UTF-8 bytes, base64url without padding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedLease {
    pub payload: String,
    pub sig: String,
}

/// What a lease says, read without checking the signature — for deciding when to renew only.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LeaseClaims {
    pub v: u32,
    pub purpose: String,
    #[serde(rename = "steamId")]
    pub steam_id: String,
    #[serde(rename = "issuedAt")]
    pub issued_at: i64,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
}

/// What [`renew`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renewal {
    /// A fresh lease was written.
    Renewed,
    /// The kept lease stands: still fresh, or the control plane could not issue one right now.
    Kept,
    /// The account is blocked. The lease was deleted; the caller should take its keys back too.
    Blocked,
}

/// Where the lease lives in a DLL run directory.
pub fn lease_path(dir: &Path) -> PathBuf {
    dir.join(LEASE_FILE)
}

/// The kept lease, or `None` when there is none or it cannot be read.
pub fn read(path: &Path) -> Option<SignedLease> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// What a lease claims, if it reads as one of ours.
pub fn claims(lease: &SignedLease) -> Option<LeaseClaims> {
    let c: LeaseClaims = serde_json::from_str(&lease.payload).ok()?;
    (c.v == 1 && c.purpose == LEASE_PURPOSE).then_some(c)
}

/// Milliseconds since the epoch, by this machine's clock.
fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Whether the kept lease should be replaced: none, not for the Steam account running now, or
/// fewer than [`RENEW_BELOW`] left. With no live Steam ID there is nothing to compare against,
/// so only the expiry decides.
pub fn needs_renewal(kept: Option<&LeaseClaims>, live_steam_id: Option<&str>, now_ms: i64) -> bool {
    let Some(kept) = kept else { return true };
    if live_steam_id.is_some_and(|id| id.trim() != kept.steam_id) {
        return true;
    }
    kept.expires_at.saturating_sub(now_ms) < RENEW_BELOW.as_millis() as i64
}

/// Whether a fresh lease should replace the kept one. It always does, unless the kept one is for
/// the Steam login running now and the fresh one is not — the app signed in as one Steam account
/// while Steam runs as another. The DLL checks against the running login, so overwriting would
/// lock content that still had days to run.
pub fn replaces(kept: Option<&LeaseClaims>, fresh: Option<&LeaseClaims>, live_steam_id: Option<&str>) -> bool {
    let Some(live) = live_steam_id.map(str::trim) else { return true };
    let kept_is_live = kept.is_some_and(|k| k.steam_id == live);
    let fresh_is_live = fresh.is_some_and(|f| f.steam_id == live);
    fresh_is_live || !kept_is_live
}

/// Write a lease where the DLL reads it: a temporary name moved into place, so the DLL never sees
/// half a file.
pub fn store(path: &Path, lease: &SignedLease) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string(lease).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Take the lease away. `true` when there was one.
pub fn remove(path: &Path) -> bool {
    std::fs::remove_file(path).is_ok()
}

/// What a control-plane answer to `POST /v1/keys/lease` asks of the kept lease.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// A fresh lease: store it.
    Store(SignedLease),
    /// Nothing to act on — keep what is there.
    Keep,
    /// A block: delete it.
    Block,
}

/// Read a control-plane answer, split out from the network so it can be tested.
pub fn decide(status: u16, body: &str) -> Answer {
    if appgate::is_block_refusal(status, body) {
        return Answer::Block;
    }
    if !(200..300).contains(&status) {
        return Answer::Keep;
    }
    #[derive(Deserialize)]
    struct Wire {
        lease: SignedLease,
    }
    serde_json::from_str::<Wire>(body)
        .ok()
        .map(|w| w.lease)
        .filter(|l| claims(l).is_some())
        .map_or(Answer::Keep, Answer::Store)
}

/// Renew the lease in `dir` if it needs it, as the account holding `token`.
///
/// `force` asks even when the kept lease is fresh — for a moment the answer may just have
/// changed, like a Steam sign-in. Every failure to reach or read the control plane keeps the
/// lease that is there: offline, the buyer plays on what they have.
pub async fn renew(token: &str, dir: &Path, live_steam_id: Option<&str>, force: bool) -> Renewal {
    let path = lease_path(dir);
    let kept = read(&path).as_ref().and_then(claims);
    if !force && !needs_renewal(kept.as_ref(), live_steam_id, now_ms()) {
        return Renewal::Kept;
    }
    if token.trim().is_empty() {
        return Renewal::Kept;
    }
    let client = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build().unwrap_or_default();
    let resp = match client
        .post(format!("{}/v1/keys/lease", control_plane()))
        .bearer_auth(token.trim())
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::info!("[lease] couldn't reach the control plane, keeping the lease: {e}");
            return Renewal::Kept;
        }
    };
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    match decide(status, &body) {
        Answer::Block => {
            appgate::note_refusal(status, Some(&body));
            let had = remove(&path);
            log::warn!("[lease] refused; lease {}", if had { "removed" } else { "was already gone" });
            Renewal::Blocked
        }
        Answer::Store(lease) if !replaces(kept.as_ref(), claims(&lease).as_ref(), live_steam_id) => {
            log::info!("[lease] the account's lease is for another Steam login than the one running; keeping this one's");
            Renewal::Kept
        }
        Answer::Store(lease) => match store(&path, &lease) {
            Ok(()) => {
                let until = claims(&lease).map(|c| c.expires_at).unwrap_or_default();
                log::info!("[lease] renewed until {until}");
                Renewal::Renewed
            }
            Err(e) => {
                log::warn!("[lease] couldn't write the lease: {e}");
                Renewal::Kept
            }
        },
        Answer::Keep => {
            // 409 (no Steam link yet), 503 (the deployment issues none), anything else: keep.
            log::info!("[lease] not renewed ({status}): {}", body.chars().take(200).collect::<String>());
            Renewal::Kept
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 60 * 60 * 1000;
    const STEAM: &str = "76561198000000077";

    fn lease(steam_id: &str, issued_at: i64) -> SignedLease {
        SignedLease {
            payload: format!(
                r#"{{"v":1,"purpose":"mxbsecure-lease","steamId":"{steam_id}","issuedAt":{issued_at},"expiresAt":{}}}"#,
                issued_at + 30 * DAY
            ),
            sig: "sig".into(),
        }
    }

    #[test]
    fn renews_when_missing_for_another_account_or_short() {
        let now = 1_800_000_000_000;
        let fresh = claims(&lease(STEAM, now)).unwrap();
        assert!(needs_renewal(None, Some(STEAM), now));
        assert!(!needs_renewal(Some(&fresh), Some(STEAM), now));
        assert!(!needs_renewal(Some(&fresh), None, now));
        assert!(needs_renewal(Some(&fresh), Some("76561198000000042"), now));
        // Four days in there are 26 left: keep. Six days in, 24 left: renew.
        assert!(!needs_renewal(Some(&fresh), Some(STEAM), now + 4 * DAY));
        assert!(needs_renewal(Some(&fresh), Some(STEAM), now + 6 * DAY));
        assert!(needs_renewal(Some(&fresh), Some(STEAM), now + 31 * DAY));
    }

    #[test]
    fn never_trades_the_running_logins_lease_for_another_accounts() {
        let mine = claims(&lease(STEAM, 0)).unwrap();
        let theirs = claims(&lease("76561198000000042", 5)).unwrap();
        // Signed in as another account than Steam runs: keep the running login's lease.
        assert!(!replaces(Some(&mine), Some(&theirs), Some(STEAM)));
        // Nothing useful kept, so the fresh one may as well be there.
        assert!(replaces(None, Some(&theirs), Some(STEAM)));
        assert!(replaces(Some(&theirs), Some(&theirs), Some(STEAM)));
        // The ordinary renewal, and no running login to compare against.
        assert!(replaces(Some(&mine), Some(&mine), Some(STEAM)));
        assert!(replaces(Some(&mine), Some(&theirs), None));
    }

    #[test]
    fn reads_only_leases_as_leases() {
        assert!(claims(&lease(STEAM, 0)).is_some());
        // A gate verdict signed by the same key is not a lease.
        let verdict = SignedLease {
            payload: r#"{"v":1,"status":"ok","account":"acc_x","token":null,"steamId":null,"guid":null,"issuedAt":0}"#.into(),
            sig: "sig".into(),
        };
        assert!(claims(&verdict).is_none());
    }

    #[test]
    fn a_block_deletes_and_anything_else_unknown_keeps() {
        let ok = format!(r#"{{"lease":{},"expiresAt":1}}"#, serde_json::to_string(&lease(STEAM, 0)).unwrap());
        assert_eq!(decide(200, &ok), Answer::Store(lease(STEAM, 0)));
        assert_eq!(decide(403, r#"{"error":"x","code":"blocked"}"#), Answer::Block);
        assert_eq!(decide(403, r#"{"error":"not entitled"}"#), Answer::Keep);
        assert_eq!(decide(409, r#"{"error":"no Steam account linked"}"#), Answer::Keep);
        assert_eq!(decide(503, r#"{"error":"leases not configured"}"#), Answer::Keep);
        assert_eq!(decide(200, "not json"), Answer::Keep);
    }

    #[test]
    fn stores_reads_and_removes() {
        let dir = std::env::temp_dir().join(format!("mxb-lease-test-{}", std::process::id()));
        let path = lease_path(&dir);
        store(&path, &lease(STEAM, 5)).unwrap();
        assert_eq!(read(&path), Some(lease(STEAM, 5)));
        assert!(remove(&path));
        assert!(read(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
