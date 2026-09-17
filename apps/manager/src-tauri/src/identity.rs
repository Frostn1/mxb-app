//! Who this player is, taken from the game rather than guessed from the disk.
//!
//! Two facts the control plane needs, and neither was reliably arriving.
//!
//! **The GUID** is the only stable identity a rider has: a name is free text they can change
//! between sessions, and one they share with anybody who picked the same one. For a Steam copy
//! of the game it is not observed at all any more — it is *derived* from the signed-in Steam
//! account (`steamid::local_guid`: `FF` + the SteamID64 in hex), the same value the control
//! plane pins from the identity Valve confirms, so it is known instantly and cannot be a
//! different rider's. Only a non-Steam (Piboso) copy still has to be watched for: there the GUID
//! is opaque, and it is read from the running game (below) or a dedicated-server log.
//!
//! **The rider name** was read off the game's profile *folder*, which is not the name the
//! server shows. A player who never renamed their profile is `unnamedProfile`, and so is
//! everyone else who never renamed theirs.
//!
//! Both are in the `EventInit` payload the plugin API hands FrostMod on entering a session:
//! `m_szRiderName` is the name every other rider on the grid sees, and `m_szGUID` is this
//! player's own GUID — the API exposes nobody else's. FrostMod already publishes both into
//! the shared session block for voice chat. This is the same block, read for identity.
//!
//! Nothing here needs the player to own a server, to type anything, or to be online: entering
//! a session at all is enough, testing sessions included.

use crate::config::{self, AppConfig};
use crate::paintsync;
use tauri::AppHandle;

/// What the running game says this player is called and what their GUID is.
///
/// Split out from the claiming so the decisions can be tested off Windows, where the shared
/// block does not exist.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeenIdentity {
    pub guid: String,
    pub rider_name: String,
}

/// The GUID worth sending, or nothing.
///
/// Nothing to send when the game hasn't reported one — an offline session before `EventInit`,
/// or a FrostMod too old to publish it — and nothing to send when it agrees with what we
/// already hold, which is the case on every pass after the first.
pub fn guid_to_claim<'a>(held: &str, seen: &'a str) -> Option<&'a str> {
    let seen = seen.trim();
    if seen.is_empty() || seen == held.trim() {
        return None;
    }
    Some(seen)
}

/// The GUID to auto-find, preferring the one derived from the signed-in Steam account.
///
/// For a Steam copy of the game the GUID is a pure function of the Steam login
/// (`steamid::local_guid`), which is deterministic and known without watching a server or
/// reading game memory — and it is exactly the value the control plane will pin from the
/// identity Valve confirms, so claiming anything else is pointless. `steam_derived` is `None`
/// only on a non-Steam (Piboso) copy or when Steam can't be read; there we fall back to what the
/// running game reported. Either way the value still passes through [`guid_to_claim`], so an
/// unchanged GUID is not re-sent.
pub fn preferred_guid<'a>(steam_derived: Option<&'a str>, seen: &'a str) -> &'a str {
    match steam_derived {
        Some(g) if !g.trim().is_empty() => g,
        _ => seen,
    }
}

/// The rider name worth sending, or nothing.
///
/// The game's name wins over the one enrolment guessed, because it is the name the server
/// shows and paint sync is matched on it. Compared case-sensitively: a player who corrected
/// their own capitalisation meant it, and the control plane's uniqueness is case-insensitive
/// either way.
pub fn name_to_claim<'a>(enrolled: &str, seen: &'a str) -> Option<&'a str> {
    let seen = seen.trim();
    if seen.is_empty() || seen == enrolled.trim() {
        return None;
    }
    Some(seen)
}

/// Take whatever the running game is willing to say about this player.
///
/// Called on every pass of the session watch while a game is up. Costs a shared-memory read
/// and two string compares; it only reaches the network when something has actually changed,
/// which is once per player per install for the GUID and once per rename for the name.
pub async fn claim_from_game(app: &AppHandle, seen: &SeenIdentity) {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return;
    }

    // Prefer the Steam-derived GUID over whatever the game happened to report: it is the same
    // value for a Steam player and it is the one the control plane treats as authoritative.
    let steam_derived = crate::steamid::local_guid();
    let want = preferred_guid(steam_derived.as_deref(), &seen.guid);
    if let Some(guid) = guid_to_claim(&cfg.cp_guid, want) {
        match claim_guid(app, guid).await {
            Ok(()) => log::info!("[identity] claimed GUID {guid} from the running game"),
            // First-come on the server side, so a rejection is a real answer — another
            // account holds it — not something to retry into a loop. It is not retried
            // because the config now holds nothing new to compare against, and the next
            // pass makes the same request only if the game reports a different GUID.
            Err(e) => log::warn!("[identity] couldn't claim GUID {guid}: {e}"),
        }
    }

    if let Some(name) = name_to_claim(&cfg.cp_rider_name, &seen.rider_name) {
        match claim_rider_name(app, name).await {
            Ok(()) => log::info!("[identity] took the rider name '{name}' from the running game"),
            Err(e) => log::warn!("[identity] couldn't take the rider name '{name}': {e}"),
        }
    }
}

/// Register `guid` against this account and remember it locally.
///
/// Shared by the manual field, the automatic claim off a server roster, and the claim off
/// the running game, so all three validate the same way and land in the same place.
pub async fn claim_guid(app: &AppHandle, guid: &str) -> Result<(), String> {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    put(&cfg, "guid", serde_json::json!({ "guid": guid.trim() })).await?;

    // Re-read rather than reusing the config above: the round trip is long enough for
    // something else to have written it.
    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    cfg.cp_guid = guid.trim().to_string();
    config::save(app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Tell the control plane the name the game knows this player by.
pub async fn claim_rider_name(app: &AppHandle, name: &str) -> Result<(), String> {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    put(&cfg, "name", serde_json::json!({ "riderName": name.trim() })).await?;

    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    cfg.cp_rider_name = name.trim().to_string();
    config::save(app, &cfg).map_err(|e| format!("{e:#}"))
}

/// `PUT /v1/me/<what>`, with the control plane's own error text on the way out.
async fn put(cfg: &AppConfig, what: &str, body: serde_json::Value) -> Result<(), String> {
    let resp = reqwest::Client::new()
        .put(format!("{}/v1/me/{what}", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if resp.status().is_success() {
        return Ok(());
    }
    let detail = resp.text().await.unwrap_or_default();
    Err(serde_json::from_str::<serde_json::Value>(&detail)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or(detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guid_we_do_not_hold_is_claimed() {
        assert_eq!(guid_to_claim("", "abc-123"), Some("abc-123"));
    }

    #[test]
    fn the_guid_we_already_hold_is_not_sent_again() {
        assert_eq!(guid_to_claim("abc-123", "abc-123"), None);
        assert_eq!(guid_to_claim("abc-123", " abc-123 "), None);
    }

    #[test]
    fn a_session_reporting_no_guid_asks_for_nothing() {
        // Every pass before EventInit, and every pass under a FrostMod too old to publish it.
        assert_eq!(guid_to_claim("", ""), None);
        assert_eq!(guid_to_claim("abc-123", "   "), None);
    }

    #[test]
    fn a_guid_that_changed_is_claimed_again() {
        // A second Steam account on one machine. The control plane decides who keeps it.
        assert_eq!(guid_to_claim("abc-123", "def-456"), Some("def-456"));
    }

    #[test]
    fn the_games_name_replaces_the_one_enrolment_guessed() {
        assert_eq!(name_to_claim("unnamedProfile", "Frost"), Some("Frost"));
    }

    #[test]
    fn a_name_that_already_agrees_is_not_sent() {
        assert_eq!(name_to_claim("Frost", "Frost"), None);
        assert_eq!(name_to_claim("Frost", "  Frost  "), None);
    }

    #[test]
    fn a_session_reporting_no_name_leaves_the_enrolled_one_alone() {
        assert_eq!(name_to_claim("Frost", ""), None);
    }

    #[test]
    fn the_steam_derived_guid_is_preferred_when_present() {
        assert_eq!(preferred_guid(Some("FF011000010178A758"), "FF0000000000000001"), "FF011000010178A758");
    }

    #[test]
    fn a_non_steam_copy_falls_back_to_what_the_game_reported() {
        assert_eq!(preferred_guid(None, "AA0110000100000010"), "AA0110000100000010");
        // An empty derived value (Steam unreadable) is not preferred over a real one.
        assert_eq!(preferred_guid(Some("  "), "AA0110000100000010"), "AA0110000100000010");
    }

    #[test]
    fn capitalisation_the_player_corrected_is_taken() {
        assert_eq!(name_to_claim("frost", "Frost"), Some("Frost"));
    }
}
