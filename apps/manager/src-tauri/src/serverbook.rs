//! Every server we have ever been told about, so the browser doesn't need the master to work.
//!
//! The master server is the only thing that can *discover* a server, and asking it costs the
//! player's Steam account — a `LOGIN` carrying an encrypted app ticket. The game spends that
//! same account whenever it is running, which is why the tab used to go dark mid-session.
//!
//! But discovery is the only part that needs the master. A server answers `GETINFO` to anyone
//! who asks, with no challenge, no password and no ticket, and the reply carries everything the
//! list shows: name, riders, seats, password, the whole event blob. So the addresses are worth
//! keeping: with a book of them the app can rebuild the entire list by asking the servers
//! themselves, and never touch the account the game is using.
//!
//! The book is written after every successful master sweep and is otherwise append-and-age:
//! a server that stops answering keeps its row for a while, because "the host rebooted" and
//! "the host is gone" look identical for the first few minutes and only one of them is worth
//! forgetting.

use crate::WorldServer;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One remembered server. Only the fields the master is the sole source of — everything else
/// comes back live from the server itself, so storing it would just be a staler copy.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    /// `ip:port` — the key, and the only field that is truly required.
    pub address: String,
    /// Last name the master gave it. A `GETINFO` reply carries a name too, so this is really
    /// just what the row says before the probe comes back.
    pub name: String,
    /// The address the server reports for itself; kept because the master is the only place it
    /// appears and the detail panel shows it.
    pub lan_address: String,
    /// Operator free text, `[connection] location`. Never sent by `GETINFO`.
    pub location: String,
    /// Licence class the server requires. Also master-only.
    pub rating: String,
    /// Whether the game could reach the address at all.
    pub joinable: bool,
    /// Milliseconds since the epoch, from the last sweep that saw it.
    pub last_seen: u64,
}

/// How long a server that has stopped appearing is kept.
///
/// Long enough that a host taken down for a weekend comes back to its own row, short enough
/// that the book doesn't become a list of everything that ever existed — every entry costs a
/// datagram on every probe-only refresh, so this is a real budget and not just tidiness.
pub const KEEP: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 60 * 60);

/// The most addresses the book will hold. A cap the master can't push past, because the
/// refresh that reads this book sends one datagram per row and a runaway file would turn a
/// tab refresh into a port scan.
pub const MAX_ENTRIES: usize = 2000;

fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))?
        .join("servers.json"))
}

/// Read the book. A missing or damaged file is an empty book, never an error: the browser's
/// answer to "I have no addresses" is to ask the master, which is what it would do anyway.
pub fn load(app: &tauri::AppHandle) -> Vec<Entry> {
    let Ok(p) = path(app) else { return Vec::new() };
    read(&p)
}

fn read(p: &Path) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(p) else { return Vec::new() };
    match serde_json::from_str::<Vec<Entry>>(&text) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("[serverbook] {} isn't readable ({e}); starting a fresh book", p.display());
            Vec::new()
        }
    }
}

/// Fold a fresh master sweep into the book and write it back.
///
/// Best-effort: a book we couldn't write is a slower refresh next time, not a failed one, so
/// nothing here is allowed to fail a list the player is already looking at.
pub fn remember(app: &tauri::AppHandle, servers: &[WorldServer], now: u64) {
    let Ok(p) = path(app) else { return };
    let merged = merge(read(&p), servers, now);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(&merged) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
                log::warn!("[serverbook] couldn't write {}: {e}", p.display());
            }
        }
        Err(e) => log::warn!("[serverbook] couldn't serialize the book: {e}"),
    }
}

/// The merge itself, with no I/O in it so the ageing and the cap can be tested directly.
///
/// A server that appears in the sweep is refreshed and re-stamped; one that doesn't keeps the
/// stamp it had, and ages out on its own. Newest-seen first, so the cap drops the coldest
/// addresses rather than an arbitrary slice.
pub fn merge(existing: Vec<Entry>, servers: &[WorldServer], now: u64) -> Vec<Entry> {
    let cutoff = now.saturating_sub(KEEP.as_millis() as u64);
    let mut book: Vec<Entry> = existing.into_iter().filter(|e| e.last_seen >= cutoff).collect();

    for s in servers {
        if s.address.trim().is_empty() {
            continue;
        }
        let fresh = Entry {
            address: s.address.clone(),
            name: s.name.clone(),
            lan_address: s.lan_address.clone(),
            location: s.location.clone(),
            rating: s.rating.clone(),
            joinable: s.joinable,
            last_seen: now,
        };
        match book.iter_mut().find(|e| e.address == s.address) {
            Some(e) => *e = fresh,
            None => book.push(fresh),
        }
    }

    book.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then_with(|| a.address.cmp(&b.address)));
    book.truncate(MAX_ENTRIES);
    book
}

/// Turn the book back into rows to probe.
///
/// Every field the master owns is restored and everything else left at its default, because
/// the probe is about to overwrite it. A row that never gets an answer is dropped by the
/// caller — showing a remembered name beside a rider count from last week would be worse than
/// showing nothing.
pub fn rows(book: &[Entry]) -> Vec<WorldServer> {
    book.iter()
        .map(|e| WorldServer {
            name: e.name.clone(),
            address: e.address.clone(),
            joinable: e.joinable,
            lan_address: e.lan_address.clone(),
            location: e.location.clone(),
            rating: e.rating.clone(),
            ..Default::default()
        })
        .collect()
}

/// Milliseconds since the epoch; 0 from a clock we can't read, which ages everything out
/// rather than keeping it forever.
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(address: &str, name: &str) -> WorldServer {
        WorldServer {
            name: name.into(),
            address: address.into(),
            joinable: true,
            location: "EU".into(),
            rating: "B".into(),
            ..Default::default()
        }
    }

    const DAY: u64 = 24 * 60 * 60 * 1000;

    #[test]
    fn a_sweep_becomes_a_book() {
        let book = merge(vec![], &[server("198.51.100.1:54210", "One")], 1_000);
        assert_eq!(book.len(), 1);
        assert_eq!(book[0].address, "198.51.100.1:54210");
        assert_eq!(book[0].location, "EU");
        assert_eq!(book[0].last_seen, 1_000);
    }

    /// The book is the union, not the last sweep: a server the master happened not to list this
    /// time is exactly the one a probe-only refresh still wants to ask.
    #[test]
    fn a_server_missing_from_this_sweep_is_kept() {
        let first = merge(vec![], &[server("198.51.100.1:54210", "One")], DAY);
        let second = merge(first, &[server("198.51.100.2:54210", "Two")], 2 * DAY);
        assert_eq!(second.len(), 2);
        assert!(second.iter().any(|e| e.address == "198.51.100.1:54210"));
    }

    #[test]
    fn a_returning_server_is_refreshed_not_duplicated() {
        let first = merge(vec![], &[server("198.51.100.1:54210", "Old name")], DAY);
        let second = merge(first, &[server("198.51.100.1:54210", "New name")], 2 * DAY);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].name, "New name");
        assert_eq!(second[0].last_seen, 2 * DAY);
    }

    /// Every row costs a datagram on every probe-only refresh, so a host that disappeared
    /// months ago has to stop being asked.
    #[test]
    fn a_long_gone_server_ages_out() {
        let old = merge(vec![], &[server("198.51.100.1:54210", "Gone")], DAY);
        let now = DAY + KEEP.as_millis() as u64 + 1;
        let book = merge(old, &[], now);
        assert!(book.is_empty());
    }

    #[test]
    fn a_server_seen_yesterday_survives() {
        let old = merge(vec![], &[server("198.51.100.1:54210", "Here")], DAY);
        let book = merge(old, &[], 2 * DAY);
        assert_eq!(book.len(), 1);
    }

    /// The cap has to drop the coldest addresses; dropping the freshest would mean the servers
    /// people are actually on are the ones that stop being remembered.
    #[test]
    fn the_cap_drops_the_coldest_first() {
        let mut book: Vec<Entry> = (0..MAX_ENTRIES)
            .map(|i| Entry {
                address: format!("198.51.100.1:{}", 10_000 + i),
                last_seen: 10 * DAY,
                ..Default::default()
            })
            .collect();
        book.push(Entry { address: "cold:1".into(), last_seen: 2 * DAY, ..Default::default() });

        let merged = merge(book, &[server("hot:1", "Hot")], 11 * DAY);
        assert_eq!(merged.len(), MAX_ENTRIES);
        assert_eq!(merged[0].address, "hot:1", "the newest sighting leads");
        assert!(!merged.iter().any(|e| e.address == "cold:1"), "the coldest row is what goes");
    }

    /// A record with no address can't be probed and can't be joined, so it has no business in
    /// a book whose whole purpose is addresses.
    #[test]
    fn a_record_with_no_address_is_not_remembered() {
        let book = merge(vec![], &[server("", "Nameless"), server("  ", "Blank")], 1_000);
        assert!(book.is_empty());
    }

    /// What the probe-only refresh starts from: the master's fields restored, everything the
    /// server itself will answer left empty.
    #[test]
    fn rows_restore_what_only_the_master_knows() {
        let book = merge(vec![], &[server("198.51.100.1:54210", "One")], 1_000);
        let rows = rows(&book);
        assert_eq!(rows[0].location, "EU");
        assert_eq!(rows[0].rating, "B");
        assert!(rows[0].joinable);
        assert_eq!(rows[0].players, 0, "the probe fills this in, the book must not pretend to");
        assert_eq!(rows[0].ping_ms, None);
    }

    /// A book that has been hand-edited into nonsense must not take the tab down with it.
    #[test]
    fn a_damaged_book_reads_as_an_empty_one() {
        let dir = std::env::temp_dir().join(format!("serverbook-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("servers.json");
        std::fs::write(&p, "{ not json at all").unwrap();
        assert!(read(&p).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
