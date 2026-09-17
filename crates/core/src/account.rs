//! A control-plane token for any app in the lineup, claimed once and shared.
//!
//! MXB App claims a self-serve account the first time voice or paint sync needs one; the token
//! lands in the shared config and every app in the lineup reads the same file. But Frost's Studio
//! and MXB Coach can be opened on their own, before MXB App ever has, so the startup gate needs a
//! way to get a token from any of them. That is all this is: return the token already in the
//! config, or claim a device account (`POST /v1/account`) and save it, the same self-serve
//! account MXB App would have made.
//!
//! It never enrolls with an invite and never links Steam — a device account is anonymous until
//! the player signs in with Steam, which is exactly what the gate then asks them to do.

use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

use crate::config;
use crate::names::control_plane;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
struct Claimed {
    token: String,
}

/// The token this app talks to the control plane with, claiming a device account if it has none.
///
/// Idempotent and shared: a token already in the config (claimed here or by MXB App) is returned
/// untouched. An error means the control plane could not be reached — the caller treats that as
/// "don't know", never as a reason to lock a paying player out.
pub async fn ensure_token(app: &AppHandle) -> Result<String, String> {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    let existing = cfg.cp_token.trim();
    if !existing.is_empty() {
        return Ok(existing.to_string());
    }

    // A label only — what other riders would see beside a talking indicator. The account is the
    // identity, not this. Uses the enrolled name if there is one, else a placeholder; nothing
    // here reaches for the game's profile list, which is MXB App's richer path.
    let rider_name = {
        let n = cfg.cp_rider_name.trim();
        if n.is_empty() { "Rider".to_string() } else { n.to_string() }
    };

    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("couldn't reach the service: {e}"))?;
    let resp = client
        .post(format!("{}/v1/account", control_plane()))
        .json(&serde_json::json!({ "riderName": rider_name }))
        .send()
        .await
        .map_err(|e| format!("couldn't reach the service: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("the service turned down the sign-up ({})", resp.status()));
    }
    let claimed: Claimed = resp
        .json()
        .await
        .map_err(|e| format!("the service sent something unexpected: {e}"))?;

    // Re-read before writing: `save` rewrites the whole file and this ran across an await, so the
    // config on disk may have moved on (MXB App claiming in parallel, say).
    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        cfg.cp_token = claimed.token.clone();
        let _ = config::save(app, &cfg);
        return Ok(claimed.token);
    }
    // Somebody else won the race; use theirs so both apps share one account.
    Ok(cfg.cp_token.trim().to_string())
}
