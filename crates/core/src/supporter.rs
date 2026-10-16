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

// ── The commands the webview calls ──────────────────────────────────────────────────────

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
