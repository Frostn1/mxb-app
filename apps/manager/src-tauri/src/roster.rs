//! The shared server book.
//!
//! ## What this finishes
//!
//! [`crate::serverbook`] already lets the app survive a dead master server, and the mechanism
//! is worth restating because this is only its missing half. The master is the sole source of
//! *discovery* — the only thing that can tell you a server exists — but it is the source of
//! nothing else: a server answers `GETINFO` to whoever asks, with no account, no ticket and no
//! challenge, and that reply carries the name, the riders, the seats and the whole event blob.
//! So the app keeps every address it has been told about and, when the master won't answer,
//! rebuilds the entire list by asking the servers themselves.
//!
//! That works, and it works for the wrong people. The book is per-install and starts empty, so
//! it is worth nothing to a fresh install and nothing to anybody who had not opened the Servers
//! tab before the outage began — which is the whole population that shows up in the Discord
//! asking what happened. Pooling the book is what gets the fallback there before the outage
//! does.
//!
//! ## What is sent, and what isn't
//!
//! Addresses. `host:port` of public game servers, which the app has just read out of the game's
//! own public server browser, and nothing else — no names, no rider, no install id, no account.
//! The control plane keys corroboration on a day-salted digest of the caller's address that it
//! computes for itself (see `control-plane/src/roster.ts`), so this end sends nothing that
//! identifies anybody and nothing that could be traced back between days.
//!
//! That is also why this is not behind the anonymous-stats setting the outage probe rides on.
//! That setting governs telling us about *you* — which features you open, how long the app is
//! up. This tells us about public infrastructure the player just looked at, and gating it would
//! mean the people who most value their privacy are the ones whose Servers tab stays broken.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::paintsync::control_plane;
use crate::serverbook;
use crate::WorldServer;

/// Long enough for a cold Worker, short enough that nothing here delays a tab.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct Report<'a> {
    addresses: Vec<&'a str>,
}

#[derive(Deserialize)]
struct Roster {
    #[serde(default)]
    addresses: Vec<String>,
}

/// Contribute the addresses of a sweep that really came from the master.
///
/// Only ever called with a list the master itself produced, which is the property that makes
/// the shared book safe to serve: the control plane holds an address back until distinct
/// networks have independently seen it there. Feeding it a list rebuilt from our own book would
/// be circular — it would corroborate addresses using the very copies it handed out — so the
/// caller passes the master's answer and nothing else.
///
/// Spawned and forgotten. This hangs off something the player is waiting on, and a contribution
/// that doesn't send is simply one that doesn't send; the next sweep sends another.
pub fn contribute(servers: &[WorldServer]) {
    let addresses: Vec<String> = servers
        .iter()
        .filter(|s| s.joinable && !s.address.trim().is_empty())
        .map(|s| s.address.trim().to_string())
        .collect();
    if addresses.is_empty() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let body = Report { addresses: addresses.iter().map(String::as_str).collect() };
        let sent = match client() {
            Ok(client) => {
                client
                    .post(format!("{}/v1/roster", control_plane()))
                    .json(&body)
                    .send()
                    .await
            }
            Err(e) => {
                log::debug!("[roster] no HTTP client: {e}");
                return;
            }
        };
        match sent {
            Ok(res) if res.status().is_success() => {
                log::debug!("[roster] contributed {} address(es)", body.addresses.len());
            }
            Ok(res) => log::debug!("[roster] contribution refused ({})", res.status()),
            Err(e) => log::debug!("[roster] contribution didn't send: {e}"),
        }
    });
}

/// Fill the local book from the shared one. Returns how many addresses it added.
///
/// Additive by construction — see [`serverbook::seed`] — so this can run whenever without ever
/// costing the book something it already knew. Every failure is a quiet zero: the caller is
/// either starting the app or already handling a failed list, and neither is improved by a
/// second error about the thing that was meant to help with the first.
pub async fn seed(app: &AppHandle) -> usize {
    let Some(addresses) = fetch().await else {
        return 0;
    };
    if addresses.is_empty() {
        return 0;
    }

    let before = serverbook::load(app);
    let known = before.len();
    let seeded = serverbook::seed(before, &addresses, serverbook::now_millis());
    let added = seeded.len().saturating_sub(known);
    if added == 0 {
        return 0;
    }
    serverbook::write(app, &seeded);
    log::info!("[roster] seeded {added} address(es) into the server book");
    added
}

/// Put a server on the shared book deliberately, as its own operator.
///
/// The one path onto that list that needs no corroborating, because the account behind it is
/// the corroboration — see `claimRoster` in `control-plane/src/roster.ts`. It exists for the
/// server that two strangers will never happen to report: a new one nobody has found yet, or a
/// private one that was never in the game's master list to be seen in.
///
/// The address goes through [`gameproc::parse_server_address`] first, which is the same
/// normalisation the Join flow uses. That matters beyond tidiness: the shared book is keyed by
/// the exact string, so registering `203.0.113.10` and joining `203.0.113.10:54210` must not
/// end up as two different servers. It also fills in the default port, which is what most
/// people will leave off.
pub async fn register_mine(app: &AppHandle, address: &str) -> Result<String, String> {
    let address = crate::gameproc::parse_server_address(address).map_err(|e| format!("{e:#}"))?;
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first — Settings, then Account.".into());
    }

    let client = client().map_err(|e| format!("Couldn't build an HTTP client: {e}"))?;
    let resp = client
        .post(format!("{}/v1/roster/mine", control_plane()))
        .bearer_auth(cfg.cp_token.trim())
        .json(&serde_json::json!({ "address": address }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;

    if resp.status().is_success() {
        log::info!("[roster] registered {address} on the shared book");
        return Ok(address);
    }
    // The one refusal worth rewording. The endpoint is invite-gated on purpose — a self-serve
    // account anyone can mint with one request would walk straight past the two-network bar
    // that makes the anonymous path safe — but most people running the app have never claimed
    // an invite, so "that needs an invite" is what the majority of clicks would get, and it
    // tells them nothing they can act on. The useful half is the second sentence: almost
    // everyone who lands here did not need this in the first place.
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("Adding a server by hand needs an invited account. You probably don't need \
                    it: a server the game lists is remembered automatically, and this is only \
                    for one that never appears there."
            .into());
    }
    // Otherwise the control plane's own wording: it knows why it refused and this side would
    // only be guessing.
    let detail = resp.text().await.unwrap_or_default();
    Err(serde_json::from_str::<serde_json::Value>(&detail)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or(detail))
}

/// Ask the control plane for the shared book. `None` for any failure at all.
async fn fetch() -> Option<Vec<String>> {
    let client = client().ok()?;
    let res = client
        .get(format!("{}/v1/roster", control_plane()))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    Some(res.json::<Roster>().await.ok()?.addresses)
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(TIMEOUT).build()?)
}
