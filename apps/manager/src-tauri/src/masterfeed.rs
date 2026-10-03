//! The server list the game itself received from the master, as FrostMod publishes it.
//!
//! The app can't ask the master while MX Bikes runs — the login spends the Steam account the
//! game is holding — so during a session it only asks the servers it already remembers. A
//! server that appeared after that was invisible here while the game's own browser listed it
//! and the player could join it.
//!
//! FrostMod sits in the game process and sees every row the browser builds. It writes the
//! address and name of each one to `frostmod_masterlist.txt` in its folder, read-only on the
//! game side, and this module is the other end: it reads that file and folds the servers into
//! the remembered book, which is exactly the list the probe-only refresh asks. See
//! `frostmod/src/masterlist.h` for the format:
//!
//! ```text
//! frostmod-masterlist 1 <unix milliseconds>
//! <ipv4>:<port>\t<name>
//! ```
//!
//! The file is local and untrusted like any other: every address is checked to be a public
//! IPv4 `host:port` before it can join the book (the book is probed with a datagram per row, and
//! a hand-edited file must not turn that into a way to knock on private addresses), and the
//! size and row count are capped.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::AppHandle;

use crate::{serverbook, WorldServer};

/// FrostMod's file name, beside its log.
pub const FILE: &str = "frostmod_masterlist.txt";

const MAGIC: &str = "frostmod-masterlist";
const MAX_BYTES: u64 = 256 * 1024;
const MAX_ROWS: usize = 1000;
const MAX_NAME: usize = 64;

/// How old a feed may be and still be offered to the shared book. The addresses were on the
/// game's master list when it was written, so a day-old file is still good evidence; much older
/// is a game nobody has opened lately.
const FRESH_MS: u64 = 24 * 60 * 60 * 1000;

/// What the game's browser held at the moment it was written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Feed {
    /// Milliseconds since the epoch, from FrostMod's clock.
    pub stamp_ms: u64,
    /// `(address, name)`, public IPv4 only, no duplicates.
    pub rows: Vec<(String, String)>,
}

impl Feed {
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.stamp_ms) <= FRESH_MS
    }
}

/// An address the app may probe: a public IPv4 `host:port`. Anything else — IPv6, a hostname, a
/// LAN or loopback address, port 0 — is not a server the master told anyone about.
pub fn public_v4(address: &str) -> bool {
    let Ok(a) = address.parse::<SocketAddrV4>() else { return false };
    let ip: Ipv4Addr = *a.ip();
    a.port() != 0
        && !(ip.is_unspecified()
            || ip.is_loopback()
            || ip.is_private()
            || ip.is_link_local()
            || ip.is_broadcast()
            || ip.is_multicast())
}

/// Parse the file's text. `None` for anything that isn't this format; individual bad rows are
/// dropped rather than failing the file.
pub fn parse(text: &str) -> Option<Feed> {
    let mut lines = text.lines();
    let mut head = lines.next()?.split_whitespace();
    if head.next()? != MAGIC || head.next()? != "1" {
        return None;
    }
    let stamp_ms: u64 = head.next()?.parse().ok()?;

    let mut rows: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in lines {
        let Some((address, name)) = line.split_once('\t') else { continue };
        let address = address.trim();
        let name: String =
            name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect::<String>();
        let name = name.trim();
        if name.is_empty() || !public_v4(address) || !seen.insert(address.to_string()) {
            continue;
        }
        rows.push((address.to_string(), name.to_string()));
        if rows.len() >= MAX_ROWS {
            break;
        }
    }
    Some(Feed { stamp_ms, rows })
}

/// Read FrostMod's file from its folder. `None` when it isn't there, is too large, or isn't ours.
pub fn read(dir: &Path) -> Option<Feed> {
    let path = dir.join(FILE);
    if std::fs::metadata(&path).ok()?.len() > MAX_BYTES {
        return None;
    }
    parse(&std::fs::read_to_string(path).ok()?)
}

/// The stamp of the last feed folded into the book, so a beat that finds the same file doesn't
/// rewrite the book for nothing.
static ABSORBED: AtomicU64 = AtomicU64::new(0);

/// Fold what the game's browser saw into the remembered book, and hand the feed back.
///
/// Called before every list: whatever the book holds is what a session's probe-only refresh asks,
/// so a server the game just saw is asked about on this very beat. Best-effort — no file, no
/// FrostMod, or a book that won't write all leave things as they were.
pub fn absorb(app: &AppHandle) -> Option<Feed> {
    let feed = read(&crate::frostmod_manage::frostmod_dir(app))?;
    if feed.rows.is_empty() {
        return Some(feed);
    }
    if ABSORBED.swap(feed.stamp_ms, Ordering::Relaxed) != feed.stamp_ms {
        let before = serverbook::load(app);
        let known = before.len();
        let book = serverbook::learn(before, &feed.rows, serverbook::now_millis());
        log::info!(
            "[masterfeed] the game saw {} server(s); {} new to the book",
            feed.rows.len(),
            book.len().saturating_sub(known)
        );
        serverbook::write(app, &book);
    }
    Some(feed)
}

/// The rows of a probed list that are on the game's own master list.
///
/// What the shared book may be told when the app didn't ask the master itself. The shared book
/// holds an address back until independent installs have seen it on the master, so it must only
/// be fed addresses that really came from there — not the ones the app rebuilt from its own book
/// (which includes whatever the shared book handed out). The game's list is the master's; the
/// probe is what made each row live.
pub fn from_game(list: &[WorldServer], feed: &Feed) -> Vec<WorldServer> {
    let seen: std::collections::HashSet<&str> = feed.rows.iter().map(|(a, _)| a.as_str()).collect();
    list.iter().filter(|s| seen.contains(s.address.trim())).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "frostmod-masterlist 1 1700000000000\n";

    #[test]
    fn a_published_list_reads_back() {
        let feed = parse(&format!("{HEAD}203.0.113.7:54245\tCHEAP GAMER GUY - LIVE\n198.51.100.9:54210\tSecond\n"))
            .unwrap();
        assert_eq!(feed.stamp_ms, 1_700_000_000_000);
        assert_eq!(feed.rows.len(), 2);
        assert_eq!(feed.rows[0], ("203.0.113.7:54245".to_string(), "CHEAP GAMER GUY - LIVE".to_string()));
    }

    #[test]
    fn only_public_ipv4_addresses_get_in() {
        let text = format!(
            "{HEAD}192.168.1.5:54210\tLan\n10.0.0.2:54210\tTen\n127.0.0.1:54210\tLoop\n\
             203.0.113.7:0\tNoPort\n[::1]:54210\tV6\nexample.com:54210\tHost\n\
             172.16.0.1:54210\tPriv\n203.0.113.7:54210\tFine\n"
        );
        let feed = parse(&text).unwrap();
        assert_eq!(feed.rows, vec![("203.0.113.7:54210".to_string(), "Fine".to_string())]);
    }

    #[test]
    fn duplicates_blank_names_and_stray_lines_are_dropped() {
        let text = format!("{HEAD}203.0.113.7:54210\tOne\n203.0.113.7:54210\tAgain\n198.51.100.1:54210\t  \nno tab here\n");
        assert_eq!(parse(&text).unwrap().rows.len(), 1);
    }

    #[test]
    fn something_that_is_not_the_file_is_not_a_feed() {
        assert!(parse("").is_none());
        assert!(parse("hello\n203.0.113.7:54210\tX\n").is_none());
        assert!(parse("frostmod-masterlist 2 5\n").is_none(), "an unknown version is refused");
        assert!(parse("frostmod-masterlist 1 soon\n").is_none());
    }

    #[test]
    fn the_row_count_is_capped() {
        let mut text = HEAD.to_string();
        for i in 0..(MAX_ROWS + 50) {
            text.push_str(&format!("203.0.{}.{}:54210\tS{i}\n", 1 + i / 250, 1 + i % 250));
        }
        assert_eq!(parse(&text).unwrap().rows.len(), MAX_ROWS);
    }

    #[test]
    fn a_feed_goes_stale_after_a_day() {
        let f = Feed { stamp_ms: 1_000, rows: vec![] };
        assert!(f.is_fresh(1_000 + FRESH_MS));
        assert!(!f.is_fresh(1_001 + FRESH_MS));
    }

    #[test]
    fn only_the_games_own_servers_are_offered_to_the_shared_book() {
        let feed = Feed { stamp_ms: 1, rows: vec![("203.0.113.7:54210".into(), "Seen".into())] };
        let row = |a: &str| WorldServer { address: a.into(), name: "x".into(), ..Default::default() };
        let list = vec![row("203.0.113.7:54210"), row("198.51.100.1:54210")];
        let out = from_game(&list, &feed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].address, "203.0.113.7:54210");
    }

    #[test]
    fn a_missing_or_oversized_file_reads_as_nothing() {
        let dir = std::env::temp_dir().join(format!("masterfeed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read(&dir).is_none());
        std::fs::write(dir.join(FILE), vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        assert!(read(&dir).is_none());
        std::fs::write(dir.join(FILE), format!("{HEAD}203.0.113.7:54210\tOk\n")).unwrap();
        assert_eq!(read(&dir).unwrap().rows.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
