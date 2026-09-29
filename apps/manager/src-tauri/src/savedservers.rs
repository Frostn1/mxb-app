//! Servers the player saved by address, for the Online tab's Saved row.
//!
//! The master list is the only way to *discover* a server, and some servers are never in it: a
//! private league box, one that came up an hour ago, one the master has dropped for the evening.
//! This is the player's own short list of those — or of any server they would rather not go
//! looking for — kept as `host:port` and a name in `config.json`, in the order they arranged it.
//!
//! Nothing a server says about itself is stored. A saved server that is in the sweep takes its
//! row from there; one that isn't is asked directly with the same `GETINFO` the detail pane uses
//! ([`crate::serverwatch::probe`]), and the answers are held in memory so Race mode can read a
//! saved server's track the way it reads a listed one's (see [`probed`]).
//!
//! Validation and normalisation are [`crate::gameproc::parse_server_address`]'s, the same rule
//! Join applies, so a saved address and a joined one cannot end up as two different servers.

use std::sync::Mutex;

use crate::config::{self, SavedServer};
use crate::WorldServer;

/// The most servers the Saved row holds. Every one of them costs a datagram each time the tab
/// refreshes, so this is a budget, not tidiness — and a row of a hundred cards is no longer a
/// short list anyone scans.
pub const MAX_SAVED: usize = 100;

/// The longest name kept. A label, not a description.
const MAX_NAME: usize = 64;

/// Error codes the tab translates rather than showing as text.
pub const DUPLICATE: &str = "saved_server_duplicate";
pub const NOT_FOUND: &str = "saved_server_missing";
pub const FULL: &str = "saved_server_full";

/// Two addresses name the same server. Hostnames are case-insensitive and the normalised form
/// keeps whatever case was typed, so `Frost.example:54210` and `frost.example:54210` are one.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// `input` as Join would take it, or the parser's own message about why it can't be.
pub fn normalise(input: &str) -> Result<String, String> {
    crate::gameproc::parse_server_address(input).map_err(|e| format!("{e:#}"))
}

fn clean_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME).collect()
}

/// Append a server. Rejects what the parser rejects, and an address already on the list once
/// both are normalised — the default port filled in and case ignored.
pub fn add(mut list: Vec<SavedServer>, address: &str, name: &str) -> Result<Vec<SavedServer>, String> {
    let address = normalise(address)?;
    if list.iter().any(|s| same(&s.address, &address)) {
        return Err(DUPLICATE.into());
    }
    if list.len() >= MAX_SAVED {
        return Err(FULL.into());
    }
    list.push(SavedServer { address, name: clean_name(name) });
    Ok(list)
}

/// Change a saved server's address and name in place, keeping its position.
///
/// The new address may be the old one (a rename), but not another saved server's.
pub fn edit(
    mut list: Vec<SavedServer>,
    address: &str,
    new_address: &str,
    name: &str,
) -> Result<Vec<SavedServer>, String> {
    let new_address = normalise(new_address)?;
    let at = list.iter().position(|s| same(&s.address, address)).ok_or(NOT_FOUND)?;
    if list.iter().enumerate().any(|(i, s)| i != at && same(&s.address, &new_address)) {
        return Err(DUPLICATE.into());
    }
    list[at] = SavedServer { address: new_address, name: clean_name(name) };
    Ok(list)
}

/// Drop a saved server. Removing one that isn't there is not an error: the row is gone either
/// way, which is what the click asked for.
pub fn remove(mut list: Vec<SavedServer>, address: &str) -> Vec<SavedServer> {
    list.retain(|s| !same(&s.address, address));
    list
}

/// Put the list in the order given.
///
/// The tab sends the whole order after a move, so this has to survive an order drawn from a list
/// that changed underneath it: addresses it doesn't know are ignored, and saved servers the
/// order leaves out keep their relative place at the end rather than being lost.
pub fn reorder(list: Vec<SavedServer>, order: &[String]) -> Vec<SavedServer> {
    let mut rest = list;
    let mut out = Vec::with_capacity(rest.len());
    for want in order {
        if let Some(at) = rest.iter().position(|s| same(&s.address, want)) {
            out.push(rest.remove(at));
        }
    }
    out.extend(rest);
    out
}

fn load(app: &tauri::AppHandle) -> Vec<SavedServer> {
    config::load_or_detect(app).unwrap_or_default().saved_servers
}

/// Read, change and write the list in one go, handing the tab back what was stored.
fn update(
    app: &tauri::AppHandle,
    change: impl FnOnce(Vec<SavedServer>) -> Result<Vec<SavedServer>, String>,
) -> Result<Vec<SavedServer>, String> {
    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    let next = change(std::mem::take(&mut cfg.saved_servers))?;
    cfg.saved_servers = next.clone();
    config::save(app, &cfg).map_err(|e| format!("{e:#}"))?;
    Ok(next)
}

/// The saved servers, in the player's order.
#[tauri::command]
pub fn saved_servers(app: tauri::AppHandle) -> Vec<SavedServer> {
    load(&app)
}

/// Save a server by address. Returns the whole list as stored, the new one normalised.
#[tauri::command]
pub fn add_saved_server(
    app: tauri::AppHandle,
    address: String,
    name: String,
) -> Result<Vec<SavedServer>, String> {
    update(&app, |list| add(list, &address, &name))
}

/// Change one saved server's address or name.
#[tauri::command]
pub fn edit_saved_server(
    app: tauri::AppHandle,
    address: String,
    new_address: String,
    name: String,
) -> Result<Vec<SavedServer>, String> {
    update(&app, |list| edit(list, &address, &new_address, &name))
}

#[tauri::command]
pub fn remove_saved_server(app: tauri::AppHandle, address: String) -> Result<Vec<SavedServer>, String> {
    update(&app, |list| Ok(remove(list, &address)))
}

#[tauri::command]
pub fn reorder_saved_servers(
    app: tauri::AppHandle,
    order: Vec<String>,
) -> Result<Vec<SavedServer>, String> {
    update(&app, |list| Ok(reorder(list, &order)))
}

/// A saved server's last direct answer, and when it came.
struct Probed {
    at_ms: u64,
    row: WorldServer,
}

static PROBED: Mutex<Vec<Probed>> = Mutex::new(Vec::new());

/// Ask each of `addresses` about itself, at once, and answer with the ones that replied.
///
/// Only saved addresses are asked: the tab names which of them it needs (the ones the sweep
/// didn't carry), and this won't send a datagram anywhere the player didn't save. A server that
/// stays quiet is simply missing from the answer, which the tab draws as offline.
#[tauri::command]
pub async fn probe_saved_servers(
    app: tauri::AppHandle,
    addresses: Vec<String>,
) -> Vec<WorldServer> {
    let saved = load(&app);
    let asked: Vec<String> = saved
        .into_iter()
        .map(|s| s.address)
        .filter(|a| addresses.iter().any(|want| same(a, want)))
        .collect();
    let answers = futures_util::future::join_all(
        asked.into_iter().map(crate::serverwatch::probe),
    )
    .await;
    let rows: Vec<WorldServer> = answers.into_iter().filter_map(Result::ok).collect();

    let now = crate::serverbook::now_millis();
    let mut held = PROBED.lock().unwrap_or_else(|p| p.into_inner());
    for row in &rows {
        held.retain(|p| !same(&p.row.address, &row.address));
        held.push(Probed { at_ms: now, row: row.clone() });
    }
    // Newest last, so a list that outgrew the cap loses its oldest answers.
    let over = held.len().saturating_sub(MAX_SAVED);
    held.drain(..over);
    rows
}

/// The last direct answer from the saved server at `address`, if it is no older than `max_age_ms`.
///
/// What Race mode falls back to for a server the sweep doesn't carry: without it, a saved
/// server's track would be unknown to the filter it hands FrostMod.
pub fn probed(address: &str, max_age_ms: u64) -> Option<WorldServer> {
    let now = crate::serverbook::now_millis();
    PROBED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .find(|p| same(&p.row.address, address) && now.saturating_sub(p.at_ms) <= max_age_ms)
        .map(|p| p.row.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(address: &str, name: &str) -> SavedServer {
        SavedServer { address: address.into(), name: name.into() }
    }

    fn addresses(list: &[SavedServer]) -> Vec<&str> {
        list.iter().map(|s| s.address.as_str()).collect()
    }

    #[test]
    fn adding_normalises_with_the_join_parser() {
        let list = add(vec![], "  203.0.113.10 ", " Frost EU ").unwrap();
        assert_eq!(list, vec![saved("203.0.113.10:54210", "Frost EU")]);
    }

    #[test]
    fn a_name_is_optional() {
        let list = add(vec![], "203.0.113.10:54300", "").unwrap();
        assert_eq!(list[0].name, "");
    }

    #[test]
    fn what_join_rejects_is_rejected() {
        for bad in ["", "   ", "203.0.113.10:0", "203.0.113.10:99999", "[::1]:54210", "-directconnect"] {
            assert!(add(vec![], bad, "x").is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn duplicates_are_caught_after_normalising() {
        let list = add(vec![], "frost.example", "").unwrap();
        // The default port filled in, and case ignored, both land on the same server.
        assert_eq!(add(list.clone(), "frost.example:54210", "").unwrap_err(), DUPLICATE);
        assert_eq!(add(list.clone(), "FROST.example", "").unwrap_err(), DUPLICATE);
        // Another port on the same host is another server.
        assert_eq!(add(list, "frost.example:54211", "").unwrap().len(), 2);
    }

    #[test]
    fn the_list_is_capped() {
        let mut list = Vec::new();
        for i in 0..MAX_SAVED {
            list = add(list, &format!("198.51.100.1:{}", 10_000 + i), "").unwrap();
        }
        assert_eq!(add(list, "198.51.100.2", "").unwrap_err(), FULL);
    }

    #[test]
    fn long_names_are_trimmed() {
        let list = add(vec![], "198.51.100.1", &"x".repeat(200)).unwrap();
        assert_eq!(list[0].name.chars().count(), MAX_NAME);
    }

    #[test]
    fn editing_keeps_the_position() {
        let list = vec![saved("a:1", "A"), saved("b:1", "B"), saved("c:1", "C")];
        let list = edit(list, "b:1", "b2", "Bee").unwrap();
        assert_eq!(addresses(&list), ["a:1", "b2:54210", "c:1"]);
        assert_eq!(list[1].name, "Bee");
    }

    #[test]
    fn editing_can_rename_without_moving() {
        let list = vec![saved("a:1", "A")];
        assert_eq!(edit(list, "A:1", "a:1", "New").unwrap(), vec![saved("a:1", "New")]);
    }

    #[test]
    fn editing_onto_another_saved_server_is_a_duplicate() {
        let list = vec![saved("a:1", "A"), saved("b:1", "B")];
        assert_eq!(edit(list.clone(), "b:1", "A:1", "").unwrap_err(), DUPLICATE);
        assert_eq!(edit(list, "zz:1", "c:1", "").unwrap_err(), NOT_FOUND);
    }

    #[test]
    fn removing_ignores_case_and_absence() {
        let list = vec![saved("frost.example:54210", "A"), saved("b:1", "B")];
        let list = remove(list, "FROST.example:54210");
        assert_eq!(addresses(&list), ["b:1"]);
        assert_eq!(remove(list, "gone:1").len(), 1);
    }

    #[test]
    fn reordering_follows_the_order_given() {
        let list = vec![saved("a:1", "A"), saved("b:1", "B"), saved("c:1", "C")];
        let list = reorder(list, &["c:1".into(), "a:1".into(), "b:1".into()]);
        assert_eq!(addresses(&list), ["c:1", "a:1", "b:1"]);
    }

    #[test]
    fn reordering_from_a_stale_list_loses_nothing() {
        let list = vec![saved("a:1", "A"), saved("b:1", "B"), saved("c:1", "C")];
        // Drawn before `c` was added, and naming one since removed.
        let list = reorder(list, &["b:1".into(), "gone:1".into(), "a:1".into(), "b:1".into()]);
        assert_eq!(addresses(&list), ["b:1", "a:1", "c:1"]);
        assert_eq!(list[0].name, "B", "entries move whole");
    }

    /// Stored as address, name and order and nothing else, and read back the same.
    #[test]
    fn the_list_persists_in_the_config() {
        let cfg = config::AppConfig {
            saved_servers: add(add(vec![], "b.example", "B").unwrap(), "a.example:1", "").unwrap(),
            ..Default::default()
        };
        let text = serde_json::to_string(&cfg).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json["savedServers"],
            serde_json::json!([
                { "address": "b.example:54210", "name": "B" },
                { "address": "a.example:1", "name": "" },
            ])
        );
        let back: config::AppConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(back.saved_servers, cfg.saved_servers);
    }

    /// A config written before this existed loads with an empty list, not as a damaged file.
    #[test]
    fn an_older_config_has_no_saved_servers() {
        let cfg: config::AppConfig = serde_json::from_str(r#"{"modsPath":"x"}"#).unwrap();
        assert!(cfg.saved_servers.is_empty());
    }
}
