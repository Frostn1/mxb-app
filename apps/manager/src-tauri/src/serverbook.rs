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

/// One remembered server: the whole row as the last sweep had it, and when that was.
///
/// It used to be the master-only fields alone, on the grounds that anything a `GETINFO` reply
/// carries would only ever be stored stale. That is still true of the rebuild path — see
/// [`rows`], which drops the live half before handing the book to the prober — but it is not
/// true of opening the tab. A list from four minutes ago, drawn at once and labelled with its
/// own age, is what the tab has instead of an empty screen while the sweep runs. So the row is
/// kept whole, and the two readers take what each of them can honestly use.
///
/// The JSON shape is unchanged: `WorldServer` is flattened into the same flat object the book
/// has always been, so a book written by an older build loads with the new fields defaulted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Entry {
    /// The row itself — `address` is the key, and the only field truly required.
    #[serde(flatten)]
    pub row: WorldServer,
    /// Milliseconds since the epoch, from the last sweep that saw it.
    pub last_seen: u64,
}

impl Entry {
    /// The key, which reads better than reaching through the row every time.
    pub fn address(&self) -> &str {
        &self.row.address
    }
}

/// How far back from the newest stamp still counts as the same sweep.
///
/// A sweep writes every row it saw with one `now`, so this only has to cover the sweep itself
/// rather than any real interval — and being generous here costs nothing, since the rows it
/// would let in are the ones that same sweep wrote.
const SWEEP_WINDOW_MS: u64 = 60 * 1000;

/// The last sweep's rows exactly as they were shown, and the moment they were true.
///
/// What the tab paints while the real one runs. Empty when there is nothing worth painting,
/// which is what a fresh install has and is the shared book's cue (see [`crate::roster`]).
///
/// Rows with no name are skipped: an address seeded from the shared book carries nothing but
/// the address and is stamped like anything else, and painting a screen of blank tiles would
/// be a worse answer than the spinner it replaced.
pub fn last_sweep(book: &[Entry]) -> (Vec<WorldServer>, u64) {
    let newest = book.iter().map(|e| e.last_seen).max().unwrap_or(0);
    if newest == 0 {
        return (Vec::new(), 0);
    }
    let cutoff = newest.saturating_sub(SWEEP_WINDOW_MS);
    let rows: Vec<WorldServer> = book
        .iter()
        .filter(|e| e.last_seen >= cutoff && !e.row.name.trim().is_empty())
        .map(|e| e.row.clone())
        .collect();
    if rows.is_empty() {
        return (Vec::new(), 0);
    }
    (rows, newest)
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
    write(app, &merge(read(&p), servers, now));
}

/// Put a book on disk. Best-effort in the same sense as [`remember`], and split out from it
/// because [`seed`] builds its book a different way and has the same nothing-may-fail rule.
pub fn write(app: &tauri::AppHandle, book: &[Entry]) {
    let Ok(p) = path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(book) {
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
        let fresh = Entry { row: s.clone(), last_seen: now };
        match book.iter_mut().find(|e| e.address() == s.address) {
            Some(e) => *e = fresh,
            None => book.push(fresh),
        }
    }

    book.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then_with(|| a.address().cmp(b.address())));
    book.truncate(MAX_ENTRIES);
    book
}

/// The most addresses one seed from the shared book will add.
///
/// A real budget, not tidiness. A probe-only refresh sends one datagram per row, so seeding is
/// the one path that could turn a tab refresh into a port scan — everything else in this file
/// only ever learns addresses the player's own client was already talking to. A busy evening's
/// master list is a few hundred servers, so this clears the real population comfortably and
/// still bounds what a runaway roster could do.
pub const MAX_SEEDED: usize = 600;

/// Fill an empty book from the shared one, without disturbing what is already in it.
///
/// Deliberately not [`merge`]. That replaces a row wholesale, which is right for a master sweep
/// — every field it carries is fresher than what was there — and wrong for this, where the only
/// thing known about an address is the address. Seeding over a remembered row would trade a
/// name, a location and a licence class for three empty strings.
///
/// A seeded row is marked joinable because that is what it is: the shared book only carries
/// addresses that are a public `host:port` to begin with, which is the same test the master's
/// own records are held to. Unjoinable rows are never probed, so seeding them as anything else
/// would quietly add addresses that could never be asked and could only ever be dropped.
pub fn seed(existing: Vec<Entry>, addresses: &[String], now: u64) -> Vec<Entry> {
    let cutoff = now.saturating_sub(KEEP.as_millis() as u64);
    let mut book: Vec<Entry> = existing.into_iter().filter(|e| e.last_seen >= cutoff).collect();
    let known: std::collections::HashSet<String> =
        book.iter().map(|e| e.address().to_string()).collect();

    for address in addresses.iter().take(MAX_SEEDED) {
        let address = address.trim();
        // An address already in the book keeps everything it knows, and keeps its own stamp:
        // a seed is not a sighting, and must not make a cold row look freshly seen.
        if address.is_empty() || known.contains(address) {
            continue;
        }
        book.push(Entry {
            row: WorldServer { address: address.to_string(), joinable: true, ..Default::default() },
            last_seen: now,
        });
    }

    book.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then_with(|| a.address().cmp(b.address())));
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
            name: e.row.name.clone(),
            address: e.row.address.clone(),
            joinable: e.row.joinable,
            lan_address: e.row.lan_address.clone(),
            location: e.row.location.clone(),
            rating: e.row.rating.clone(),
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
    fn seeding_fills_an_empty_book() {
        // The case the shared book exists for: a fresh install, an outage, and nothing to fall
        // back to until now.
        let book = seed(vec![], &["198.51.100.1:54210".into(), "203.0.113.9:54210".into()], 1_000);
        assert_eq!(book.len(), 2);
        // Seeded rows have to be joinable or the probe skips them and they can only ever be
        // dropped — which would make the whole seed a no-op.
        assert!(book.iter().all(|e| e.row.joinable));
        assert!(book.iter().all(|e| e.last_seen == 1_000));
    }

    #[test]
    fn seeding_never_overwrites_what_the_master_taught_us() {
        // `merge` replaces a row wholesale, which is right for a sweep and catastrophic here:
        // the only thing a seed knows is the address, so seeding over a remembered row would
        // trade its name, location and licence class for three empty strings.
        let known = merge(vec![], &[server("198.51.100.1:54210", "One")], 1_000);
        let book = seed(known, &["198.51.100.1:54210".into()], 9_000);

        assert_eq!(book.len(), 1);
        assert_eq!(book[0].row.name, "One");
        assert_eq!(book[0].row.location, "EU");
        // And its own stamp: a seed is not a sighting, so it must not make a cold row look
        // freshly seen and win it another 30 days.
        assert_eq!(book[0].last_seen, 1_000);
    }

    #[test]
    fn seeding_ages_out_and_caps_like_any_other_write() {
        let stale = merge(vec![], &[server("198.51.100.1:54210", "Old")], 1_000);
        let book = seed(stale, &["203.0.113.9:54210".into()], 40 * DAY);
        assert_eq!(book.len(), 1);
        assert_eq!(book[0].row.address, "203.0.113.9:54210");
    }

    #[test]
    fn a_runaway_shared_book_cannot_turn_a_refresh_into_a_port_scan() {
        // One datagram per row on every probe-only refresh, so this bound is the real one.
        let many: Vec<String> = (0..MAX_SEEDED + 50)
            .map(|i| format!("198.51.100.{}:{}", i % 250, 54000 + i))
            .collect();
        assert_eq!(seed(vec![], &many, 1_000).len(), MAX_SEEDED);
    }

    #[test]
    fn a_sweep_becomes_a_book() {
        let book = merge(vec![], &[server("198.51.100.1:54210", "One")], 1_000);
        assert_eq!(book.len(), 1);
        assert_eq!(book[0].row.address, "198.51.100.1:54210");
        assert_eq!(book[0].row.location, "EU");
        assert_eq!(book[0].last_seen, 1_000);
    }

    /// The book is the union, not the last sweep: a server the master happened not to list this
    /// time is exactly the one a probe-only refresh still wants to ask.
    #[test]
    fn a_server_missing_from_this_sweep_is_kept() {
        let first = merge(vec![], &[server("198.51.100.1:54210", "One")], DAY);
        let second = merge(first, &[server("198.51.100.2:54210", "Two")], 2 * DAY);
        assert_eq!(second.len(), 2);
        assert!(second.iter().any(|e| e.address() == "198.51.100.1:54210"));
    }

    #[test]
    fn a_returning_server_is_refreshed_not_duplicated() {
        let first = merge(vec![], &[server("198.51.100.1:54210", "Old name")], DAY);
        let second = merge(first, &[server("198.51.100.1:54210", "New name")], 2 * DAY);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].row.name, "New name");
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

    /// A book written before the row was stored whole still loads — the flat shape is the same
    /// one it always was, and everything new defaults.
    #[test]
    fn an_older_book_still_reads() {
        let older = r#"[{"address":"203.0.113.9:54210","name":"One","lanAddress":"192.168.0.2",
                         "location":"EU","rating":"B","joinable":true,"lastSeen":1700}]"#;
        let book: Vec<Entry> = serde_json::from_str(older).expect("an older book is readable");
        assert_eq!(book[0].address(), "203.0.113.9:54210");
        assert_eq!(book[0].row.name, "One");
        assert_eq!(book[0].last_seen, 1700);
        assert_eq!(book[0].row.players, 0, "what it never stored defaults");
    }

    /// The tab paints the last sweep, not the whole book: a row nobody has seen since last
    /// week is an address to probe, not something to draw as though it were live.
    #[test]
    fn only_the_last_sweep_is_painted() {
        let mut now = merge(vec![], &[server("hot:1", "Hot"), server("warm:1", "Warm")], 10 * DAY);
        now.push(Entry {
            row: WorldServer { address: "old:1".into(), name: "Old".into(), ..Default::default() },
            last_seen: 3 * DAY,
        });

        let (rows, at) = last_sweep(&now);
        assert_eq!(at, 10 * DAY);
        assert_eq!(rows.len(), 2);
        assert!(!rows.iter().any(|r| r.address == "old:1"), "last week is not the last sweep");
    }

    /// A book of nothing but seeded addresses has nothing to draw, and says so — which is what
    /// sends the tab to the shared snapshot instead of painting blank tiles.
    #[test]
    fn a_seeded_book_paints_nothing() {
        let book = seed(vec![], &["198.51.100.1:54210".into()], 1_000);
        assert_eq!(last_sweep(&book), (vec![], 0));
    }

    /// The cap has to drop the coldest addresses; dropping the freshest would mean the servers
    /// people are actually on are the ones that stop being remembered.
    #[test]
    fn the_cap_drops_the_coldest_first() {
        let mut book: Vec<Entry> = (0..MAX_ENTRIES)
            .map(|i| Entry {
                row: WorldServer {
                    address: format!("198.51.100.1:{}", 10_000 + i),
                    ..Default::default()
                },
                last_seen: 10 * DAY,
                ..Default::default()
            })
            .collect();
        book.push(Entry {
            row: WorldServer { address: "cold:1".into(), ..Default::default() },
            last_seen: 2 * DAY,
        });

        let merged = merge(book, &[server("hot:1", "Hot")], 11 * DAY);
        assert_eq!(merged.len(), MAX_ENTRIES);
        assert_eq!(merged[0].row.address, "hot:1", "the newest sighting leads");
        assert!(!merged.iter().any(|e| e.address() == "cold:1"), "the coldest row is what goes");
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
