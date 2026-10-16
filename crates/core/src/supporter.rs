//! Whether this player has the supporter pack.
//!
//! The pack is cosmetic only — themes, an accent colour, a badge. Nothing the apps do today is
//! behind it. The control plane says so with `/v1/entitlements` (`supporter`), which this asks
//! while signed in and caches in the shared config, so the cosmetics keep working offline and
//! the "support" card ([`crate::support`]) stops showing.
//!
//! A failed check keeps the last answer. Signing out clears it, because the pack belongs to the
//! account and not to the machine.

use crate::config::{self, AppConfig, SupporterCache};
use crate::names::control_plane;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// What the webview sees.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supporter: bool,
    /// Unix milliseconds of the last answer from the control plane; `0` means never.
    pub checked_at: u64,
}

/// The cached answer, as it stands for this config.
pub fn status(cfg: &AppConfig) -> Status {
    Status {
        supporter: is_supporter(cfg),
        checked_at: cfg.supporter.checked_at,
    }
}

/// Supporter by the cache, and only while signed in to the account it was read for.
pub fn is_supporter(cfg: &AppConfig) -> bool {
    cfg.supporter.supporter && !cfg.cp_token.trim().is_empty()
}

#[derive(Deserialize)]
struct Entitlements {
    #[serde(default)]
    supporter: bool,
}

/// Ask the control plane, cache the answer, return the status. Without a sign-in this clears the
/// cache; a network failure leaves it as it was.
pub async fn refresh(app: &AppHandle) -> Status {
    let cfg = config::load(app).unwrap_or_default();
    let token = cfg.cp_token.trim().to_string();
    if token.is_empty() {
        if cfg.supporter != SupporterCache::default() {
            let mut fresh = cfg.clone();
            fresh.supporter = SupporterCache::default();
            save(app, &fresh);
        }
        return status(&AppConfig { supporter: SupporterCache::default(), ..cfg });
    }

    let Some(supporter) = fetch(&token).await else {
        return status(&cfg);
    };
    // Re-read before writing: the request took a while, and anything else may have saved since.
    let mut fresh = config::load(app).unwrap_or(cfg);
    fresh.supporter = SupporterCache { supporter, checked_at: now_ms() };
    save(app, &fresh);
    status(&fresh)
}

async fn fetch(token: &str) -> Option<bool> {
    let client = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build().ok()?;
    let resp = client
        .get(format!("{}/v1/entitlements", control_plane()))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| log::debug!("[supporter] couldn't reach the service ({e})"))
        .ok()?;
    if !resp.status().is_success() {
        log::debug!("[supporter] the service answered {}", resp.status());
        return None;
    }
    resp.json::<Entitlements>().await.ok().map(|e| e.supporter)
}

fn save(app: &AppHandle, cfg: &AppConfig) {
    if let Err(e) = config::save(app, cfg) {
        log::warn!("[supporter] couldn't save the supporter status ({e:#})");
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Check once at startup, in the background. Called from each app's `setup`.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh(&handle).await;
    });
}

/// Why linking a Buy Me a Coffee email didn't work, as a word the webview has a string for.
/// The control plane's own messages are English; these are what the player reads.
fn email_error(status: u16) -> &'static str {
    match status {
        503 => "notSetUp",
        409 => "steam",
        410 => "expired",
        429 => "tooMany",
        400 => "bad",
        _ => "failed",
    }
}

/// POST one of the email routes with the account's bearer. `Err` is a word from [`email_error`].
async fn email_post(app: &AppHandle, path: &str, body: serde_json::Value) -> Result<(), String> {
    let cfg = config::load(app).unwrap_or_default();
    let token = cfg.cp_token.trim().to_string();
    if token.is_empty() {
        return Err("steam".into());
    }
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|_| "failed".to_string())?;
    let resp = client
        .post(format!("{}{path}", control_plane()))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .map_err(|_| "failed".to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(email_error(resp.status().as_u16()).into())
    }
}

/// Why starting a checkout didn't work, as a word the webview has a string for.
fn checkout_error(status: u16, code: Option<&str>) -> &'static str {
    match (status, code) {
        (_, Some("owned")) => "owned",
        (409, _) => "steam",
        (503, _) => "notSetUp",
        _ => "failed",
    }
}

// ── The commands the webview calls ──────────────────────────────────────────────────────

/// Start buying the pack: a Stripe Checkout URL for the browser. The player has agreed to the
/// Terms and asked for immediate access in the app first; both are sent as such. `Err` is a word
/// from [`checkout_error`].
#[tauri::command]
pub async fn supporter_checkout(app: tauri::AppHandle) -> Result<String, String> {
    let cfg = config::load(&app).unwrap_or_default();
    let token = cfg.cp_token.trim().to_string();
    if token.is_empty() {
        return Err("steam".into());
    }
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|_| "failed".to_string())?;
    let resp = client
        .post(format!("{}/v1/supporter/checkout", control_plane()))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "agree": true, "waiver": true }))
        .send()
        .await
        .map_err(|_| "failed".to_string())?;
    let status = resp.status();
    let answer: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let code = answer.get("code").and_then(|c| c.as_str());
        return Err(checkout_error(status.as_u16(), code).into());
    }
    answer
        .get("url")
        .and_then(|u| u.as_str())
        .filter(|u| u.starts_with("https://checkout.stripe.com/"))
        .map(str::to_string)
        .ok_or_else(|| "failed".to_string())
}

/// Mail a code to the address somebody paid with on Buy Me a Coffee.
#[tauri::command]
pub async fn supporter_email_send(app: tauri::AppHandle, email: String) -> Result<(), String> {
    email_post(&app, "/v1/supporter/email", serde_json::json!({ "email": email.trim() })).await
}

/// Type the code back. Answers with the fresh status: the pack, if that address has paid.
#[tauri::command]
pub async fn supporter_email_confirm(app: tauri::AppHandle, code: String) -> Result<Status, String> {
    email_post(&app, "/v1/supporter/email/confirm", serde_json::json!({ "code": code.trim() })).await?;
    Ok(refresh(&app).await)
}

/// The cached answer. Instant, works offline.
#[tauri::command]
pub fn supporter_status(app: tauri::AppHandle) -> Status {
    status(&config::load(&app).unwrap_or_default())
}

/// Ask again now — after signing in, or after buying the pack.
#[tauri::command]
pub async fn supporter_refresh(app: tauri::AppHandle) -> Status {
    refresh(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(token: &str, supporter: bool) -> AppConfig {
        AppConfig {
            cp_token: token.into(),
            supporter: SupporterCache { supporter, checked_at: 5 },
            ..AppConfig::default()
        }
    }

    #[test]
    fn a_cached_supporter_counts_while_signed_in() {
        assert!(is_supporter(&cfg("tok", true)));
        assert_eq!(status(&cfg("tok", true)), Status { supporter: true, checked_at: 5 });
    }

    #[test]
    fn signed_out_is_never_a_supporter() {
        assert!(!is_supporter(&cfg("", true)));
        assert!(!is_supporter(&cfg("  ", true)));
    }

    #[test]
    fn the_answer_reads_with_or_without_the_flag() {
        let e: Entitlements = serde_json::from_str(r#"{"steamId":null,"assets":[]}"#).unwrap();
        assert!(!e.supporter);
        let e: Entitlements = serde_json::from_str(r#"{"supporter":true,"assets":[]}"#).unwrap();
        assert!(e.supporter);
    }

    #[test]
    fn an_old_config_has_no_supporter() {
        let c: AppConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.supporter, SupporterCache::default());
    }
}
