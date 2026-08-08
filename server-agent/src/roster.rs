//! Who is connected, read out of the dedicated server's log.
//!
//! The plugin API would give us names and bikes but no stable identity — `m_szGUID` exists
//! only on the *local* player's own event struct, not on the entries describing everyone
//! else. The server log does carry it: the game formats connections as
//! `Connection: %d %s GUID: %s - %s`, so the log is the one place a rider's name and their
//! GUID appear together.
//!
//! GUID is the identity worth keying on. A rider name is free text the player can change
//! between sessions and two people can pick the same one; a GUID is stable per install.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    /// Connection slot the server assigned.
    pub id: i64,
    pub name: String,
    pub guid: String,
}

/// Parse a whole log into the set of players currently connected.
///
/// The log is a running history, not a snapshot: a rider who joined and left appears as a
/// connection *and* a disconnection, so the current roster is what remains after replaying
/// both. Keyed by connection id, which is what the disconnection lines refer to.
pub fn connected(log: &str) -> Vec<Player> {
    let mut current: HashMap<i64, Player> = HashMap::new();
    for line in log.lines() {
        if let Some(p) = parse_connection(line) {
            current.insert(p.id, p);
        } else if let Some(id) = parse_disconnection(line) {
            current.remove(&id);
        }
    }
    let mut out: Vec<Player> = current.into_values().collect();
    // Stable order so a poll that changes nothing doesn't reorder the UI.
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// `Connection: 3 Frost GUID: ab12… - something`
///
/// The name sits between the id and the literal `GUID:`, and may contain spaces, so it is
/// taken as everything in between rather than as a single token.
fn parse_connection(line: &str) -> Option<Player> {
    let rest = line.trim().strip_prefix("Connection:")?;
    let (before, after) = rest.split_once("GUID:")?;
    let before = before.trim();
    let (id_str, name) = before.split_once(char::is_whitespace)?;
    let id: i64 = id_str.trim().parse().ok()?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    // The GUID is the first token after the marker; the trailing ` - <address>` is not part
    // of it. Splitting on " - " instead would accept a *missing* GUID, since `GUID: - x`
    // leaves `- x` as the first field.
    let guid = after.split_whitespace().next()?;
    if guid == "-" {
        return None;
    }
    Some(Player { id, name: name.to_string(), guid: guid.to_string() })
}

/// `Disconnection: 3` / `Drop (Connection Timeout): 3` — both free the slot.
fn parse_disconnection(line: &str) -> Option<i64> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("Disconnection:")
        .or_else(|| trimmed.strip_prefix("Drop (Connection Timeout):"))?;
    rest.trim().split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_connection_line() {
        let players = connected("Connection: 3 Frost GUID: abc123 - 1.2.3.4");
        assert_eq!(
            players,
            vec![Player { id: 3, name: "Frost".into(), guid: "abc123".into() }]
        );
    }

    #[test]
    fn keeps_names_that_contain_spaces() {
        // Rider names are free text, so the name cannot be taken as a single token.
        let players = connected("Connection: 7 Jean Luc GUID: def456 - 5.6.7.8");
        assert_eq!(players[0].name, "Jean Luc");
        assert_eq!(players[0].guid, "def456");
    }

    #[test]
    fn a_rider_who_left_is_not_in_the_roster() {
        // The log is a history, not a snapshot — replaying it is the whole point.
        let log = "Connection: 1 A GUID: g1 - x\nConnection: 2 B GUID: g2 - x\nDisconnection: 1";
        let players = connected(log);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].name, "B");
    }

    #[test]
    fn a_timeout_frees_the_slot_too() {
        let log = "Connection: 4 C GUID: g4 - x\nDrop (Connection Timeout): 4";
        assert!(connected(log).is_empty());
    }

    #[test]
    fn a_reused_slot_takes_the_newer_rider() {
        // Connection ids are slots; the server hands them out again after a disconnect.
        let log = "Connection: 1 Old GUID: g1 - x\nDisconnection: 1\nConnection: 1 New GUID: g2 - x";
        let players = connected(log);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].name, "New");
        assert_eq!(players[0].guid, "g2");
    }

    #[test]
    fn ignores_the_noise_the_log_is_mostly_made_of() {
        let log = "input\nend input\nsomething else\nConnection: 2 Frost GUID: g - x\ninput";
        assert_eq!(connected(log).len(), 1);
    }

    #[test]
    fn refuses_malformed_lines_rather_than_inventing_a_player() {
        for bad in [
            "Connection: Frost GUID: g - x", // no id
            "Connection: 3 GUID: g - x",     // no name
            "Connection: 3 Frost GUID: - x", // no guid
            "Connection: 3 Frost",           // no GUID marker
        ] {
            assert!(connected(bad).is_empty(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn orders_by_connection_slot() {
        let log = "Connection: 5 E GUID: g5 - x\nConnection: 1 A GUID: g1 - x";
        let players = connected(log);
        assert_eq!(players.iter().map(|p| p.id).collect::<Vec<_>>(), vec![1, 5]);
    }
}
