//! Hiding the spam and cheat servers, by the same rules FrostMod already hides them in-game.
//!
//! The master lists whatever registers with it, and a good part of that is advertising: names
//! shouting about cheats that can't be joined, and clusters of them from one host. FrostMod
//! solved this on the game's own browser (`serverfilter.cpp`) by splicing its list-populate
//! loop; the app's browser needs the same answer, and a player who has tuned the rules in one
//! should not have to tune them again in the other.
//!
//! So this is a port, not a second design: the same minimal YAML, the same keys, the same
//! order of tests, and the same defaults. It reads FrostMod's own
//! `frostmod_serverfilter.yaml` when there is one, which is what makes the two agree.
//!
//! It does have to be its own parser rather than a `serde_yaml` dependency. FrostMod hand-rolls
//! the reader in C++, and the point here is to read *that* file the way *it* does — an
//! off-the-shelf YAML parser would be stricter about some lines and looser about others, and
//! the two browsers would quietly disagree about a rule the player wrote once.
//!
//! **It is also more capable than the original, because the app has more to work with.**
//! `SB_SuppressRow` builds its candidate from the game's in-memory row, which carries no
//! address and no password flag, so `maxPerIP` and `hideLocked` are unreachable in the DLL
//! however the file is written. Here both fields come off the master record, so both rules run.

use crate::WorldServer;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

/// FrostMod's shipped defaults, verbatim from `kDefaultConfig` in `serverfilter.cpp`.
///
/// Carried as the file's own text rather than as constructed rules so that "what the app
/// filters with" and "what FrostMod writes on first run" are the same bytes, and a drift
/// between them is a failing test rather than a difference nobody notices.
const DEFAULT_CONFIG: &str = "\
# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
  - kaizo
  - kalz0
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|k[a4][il1]z[o0]|\\.pr0\\b)'
";

/// The file FrostMod writes beside its DLL, and the one we read if it's there.
pub const CONFIG_NAME: &str = "frostmod_serverfilter.yaml";

/// What the rules can be asked about one server.
#[derive(Debug, Default, Clone)]
pub struct Candidate {
    pub name: String,
    /// Host part of the address, for `maxPerIP`. Empty disables that rule for this row.
    pub ip: String,
    pub players: u32,
    pub passworded: bool,
    /// The server didn't answer its own `GETINFO`. Unlike in the game — where every row reads
    /// `---` until the pings resolve, which is why FrostMod keeps this off — the probe here has
    /// already run by the time a rule sees the row, so the answer means something.
    pub unjoinable: bool,
}

#[derive(Debug, Default)]
pub struct Rules {
    name_contains: Vec<String>,
    /// Source text beside the compiled form: a reason a player can match against their own file
    /// is worth more than "a regex matched".
    regexes: Vec<(String, Regex)>,
    max_per_ip: u32,
    hide_locked: bool,
    hide_empty: bool,
    hide_unjoinable: bool,
    /// Lines the parser couldn't use, surfaced so a typo in the file is visible in the log
    /// rather than silently costing a rule.
    pub complaints: Vec<String>,
}

impl Rules {
    /// The shipped defaults, for when there is no file to read.
    pub fn defaults() -> Rules {
        Rules::parse(DEFAULT_CONFIG)
    }

    /// FrostMod's file if it has one, otherwise the defaults.
    ///
    /// Deliberately never writes the file. FrostMod owns it — it versions it, backs up an
    /// out-of-date one and rewrites it — and two programs writing one config is how a player's
    /// edits get lost.
    pub fn load(dir: &Path) -> Rules {
        match std::fs::read_to_string(dir.join(CONFIG_NAME)) {
            Ok(text) => {
                let rules = Rules::parse(&text);
                log::info!(
                    "[serverfilter] {} name rule(s), {} regex(es) from FrostMod's own {CONFIG_NAME}",
                    rules.name_contains.len(),
                    rules.regexes.len()
                );
                for c in &rules.complaints {
                    log::warn!("[serverfilter] {c}");
                }
                rules
            }
            Err(_) => Rules::defaults(),
        }
    }

    /// The minimal YAML FrostMod writes: `key: value` scalars, and `key:` followed by `  - item`
    /// block lists for `names` / `regex`. `#` starts a comment, matching is case-insensitive,
    /// and an unreadable line is a complaint rather than a failure — half a ruleset still hides
    /// half the spam, where refusing the file hides none of it.
    pub fn parse(text: &str) -> Rules {
        #[derive(PartialEq, Clone, Copy)]
        enum List {
            None,
            Names,
            Regex,
        }
        let mut r = Rules::default();
        let mut cur = List::None;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(item) = line.strip_prefix('-') {
                let v = unquote(item.trim());
                if v.is_empty() {
                    continue;
                }
                match cur {
                    List::Names => r.name_contains.push(v.to_lowercase()),
                    List::Regex => r.push_regex(&v),
                    List::None => r.complaints.push(format!("'- {v}' has no list key above it; ignored")),
                }
                continue;
            }

            let Some((key, value)) = line.split_once(':') else {
                r.complaints.push(format!("ignoring line: {line}"));
                continue;
            };
            let value = value.trim();
            cur = List::None;
            match key.trim().to_lowercase().as_str() {
                "names" => cur = List::Names,
                "regex" | "regexes" => cur = List::Regex,
                "hideunjoinable" => r.hide_unjoinable = truthy(value),
                "hideempty" => r.hide_empty = truthy(value),
                "hidelocked" => r.hide_locked = truthy(value),
                "maxperip" => r.max_per_ip = strip_comment(value).parse().unwrap_or(0),
                other => r.complaints.push(format!("ignoring unknown key: {other}")),
            }

            // An inline scalar beside the list key (`names: foo`) counts too, the way it does
            // in the C++ — a convenience that exists in the wild because the file invites it.
            if cur != List::None && !value.is_empty() && !value.starts_with('#') {
                let v = unquote(value);
                match cur {
                    List::Names => r.name_contains.push(v.to_lowercase()),
                    List::Regex => r.push_regex(&v),
                    List::None => unreachable!("guarded above"),
                }
            }
        }
        r
    }

    /// Compile one pattern, case-insensitively, and keep its source beside it.
    ///
    /// FrostMod's are ECMAScript and these are Rust's, which is a real difference: a rule using
    /// a backreference or a lookaround compiles there and not here. That is a complaint and a
    /// skipped rule, never a refused file — the other rules are still worth running.
    fn push_regex(&mut self, src: &str) {
        match Regex::new(&format!("(?i){src}")) {
            Ok(re) => self.regexes.push((src.to_string(), re)),
            Err(e) => self.complaints.push(format!("bad regex '{src}' skipped ({e})")),
        }
    }

    /// Why this server should be hidden, or `None` to show it.
    ///
    /// Test order is `serverfilter.cpp`'s, so that a server hidden in the game is hidden here
    /// for the same stated reason. `seen` carries the per-IP tally across one list; a fresh map
    /// per call is what makes the answer depend on the list rather than on how recently the
    /// last refresh happened, which is the one thing here that is deliberately *not* a port.
    pub fn hide(&self, c: &Candidate, seen: &mut HashMap<String, u32>) -> Option<String> {
        if self.hide_unjoinable && c.unjoinable {
            return Some("didn't answer".into());
        }

        let name = c.name.to_lowercase();
        if let Some(sub) = self.name_contains.iter().find(|s| name.contains(s.as_str())) {
            return Some(format!("name contains '{sub}'"));
        }
        if let Some((src, _)) = self.regexes.iter().find(|(_, re)| re.is_match(&c.name)) {
            return Some(format!("name matches /{src}/"));
        }
        if self.hide_locked && c.passworded {
            return Some("password-locked".into());
        }
        if self.hide_empty && c.players == 0 {
            return Some("empty".into());
        }
        if self.max_per_ip > 0 && !c.ip.is_empty() {
            let n = seen.entry(c.ip.clone()).or_insert(0);
            *n += 1;
            if *n > self.max_per_ip {
                return Some(format!("more than {} from {}", self.max_per_ip, c.ip));
            }
        }
        None
    }
}

/// Mark every server the rules would hide, leaving the list otherwise intact.
///
/// Nothing is dropped: the tab shows a count of what was hidden and can reveal it, and a
/// player who thinks a rule is wrong needs to be able to see the row it caught. Dropping here
/// would mean a refetch to change your mind.
///
/// Sorted busiest-first before the tally so `maxPerIP` keeps a host's *busiest* servers rather
/// than whichever ones the master happened to send first. The tab sorts the same way, so this
/// costs nothing and makes the rule mean something stable across refreshes.
pub fn mark(rules: &Rules, servers: &mut [WorldServer]) -> usize {
    servers.sort_by(|a, b| b.players.cmp(&a.players));
    let mut seen = HashMap::new();
    let mut hidden = 0;
    for s in servers.iter_mut() {
        let c = Candidate {
            name: s.name.clone(),
            ip: host_of(&s.address),
            players: s.players,
            passworded: s.passworded,
            unjoinable: s.ping_ms.is_none(),
        };
        s.hidden = rules.hide(&c, &mut seen).unwrap_or_default();
        if !s.hidden.is_empty() {
            hidden += 1;
        }
    }
    hidden
}

/// The host out of an `ip:port`, so a filter counts servers per machine. An IPv6 address is
/// bracketed, and a row that isn't an address at all simply opts out of the per-IP rule.
fn host_of(address: &str) -> String {
    let a = address.trim();
    if let Some(rest) = a.strip_prefix('[') {
        return rest.split(']').next().unwrap_or("").to_string();
    }
    match a.rsplit_once(':') {
        Some((host, _)) => host.to_string(),
        None => a.to_string(),
    }
}

/// Strip one pair of surrounding quotes, as a YAML scalar may carry.
fn unquote(v: &str) -> String {
    let v = v.trim();
    let mut c = v.chars();
    match (c.next(), v.chars().last()) {
        (Some(q @ ('\'' | '"')), Some(last)) if last == q && v.len() >= 2 => {
            v[q.len_utf8()..v.len() - last.len_utf8()].to_string()
        }
        _ => v.to_string(),
    }
}

/// Drop a trailing ` # comment` from a scalar. Only after whitespace, so a `#` inside a value
/// survives.
fn strip_comment(v: &str) -> &str {
    let b = v.as_bytes();
    for i in 1..b.len() {
        if b[i] == b'#' && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return v[..i].trim();
        }
    }
    v.trim()
}

fn truthy(v: &str) -> bool {
    matches!(strip_comment(v).to_lowercase().as_str(), "true" | "1" | "yes" | "on")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str) -> Candidate {
        Candidate { name: name.into(), ..Default::default() }
    }

    fn hidden(rules: &Rules, c: &Candidate) -> Option<String> {
        rules.hide(c, &mut HashMap::new())
    }

    /// The whole point of the port: the names FrostMod hides in the game are hidden here, with
    /// no file to read and nothing for the player to set up.
    #[test]
    fn the_shipped_defaults_catch_the_spam_they_were_written_for() {
        let r = Rules::defaults();
        for name in ["CHE4TS.PR0 free mods", "kaizo racing", "Kalz0 Server #3", "cheats zone", "ka1z0 pub"] {
            assert!(hidden(&r, &candidate(name)).is_some(), "{name} should be hidden");
        }
    }

    /// A filter that hid real servers would be worse than no filter — this is the half that
    /// matters when someone reports "my server vanished".
    #[test]
    fn it_leaves_ordinary_servers_alone() {
        let r = Rules::defaults();
        for name in ["MXB-Central EU", "Sunday Night Supercross", "Frost's Server", "A1 Practice"] {
            assert_eq!(hidden(&r, &candidate(name)), None, "{name} must be shown");
        }
    }

    /// Matching is case-insensitive on both paths, which is most of what makes a substring rule
    /// worth writing at all.
    #[test]
    fn matching_ignores_case_on_both_paths() {
        let r = Rules::parse("names:\n  - spam\nregex:\n  - 'wid[g]et'\n");
        assert!(hidden(&r, &candidate("SPAM central")).is_some());
        assert!(hidden(&r, &candidate("WIDGET works")).is_some());
    }

    /// The two rules the DLL cannot run, because its candidate has neither field.
    #[test]
    fn hide_locked_and_hide_empty_are_reachable_here() {
        let r = Rules::parse("hideLocked: true\nhideEmpty: true\n");
        let locked = Candidate { name: "x".into(), passworded: true, players: 4, ..Default::default() };
        let empty = Candidate { name: "y".into(), players: 0, ..Default::default() };
        assert_eq!(hidden(&r, &locked).as_deref(), Some("password-locked"));
        assert_eq!(hidden(&r, &empty).as_deref(), Some("empty"));
    }

    /// `maxPerIP` counts within one list, so the same list always gives the same answer —
    /// unlike the DLL's version, which resets on a gap between rows and so depends on timing.
    #[test]
    fn max_per_ip_keeps_the_first_n_from_a_host() {
        let r = Rules::parse("maxPerIP: 2\n");
        let mut seen = HashMap::new();
        let one = |ip: &str| Candidate { name: "s".into(), ip: ip.into(), ..Default::default() };
        assert_eq!(r.hide(&one("198.51.100.7"), &mut seen), None);
        assert_eq!(r.hide(&one("198.51.100.7"), &mut seen), None);
        assert!(r.hide(&one("198.51.100.7"), &mut seen).is_some(), "the third is past the cap");
        assert_eq!(r.hide(&one("203.0.113.9"), &mut seen), None, "a different host has its own tally");
    }

    /// A row with no usable address opts out rather than sharing an empty-string bucket with
    /// every other unparseable row — which would hide all but the first of them.
    #[test]
    fn a_row_without_an_address_is_not_counted_against_a_host() {
        let r = Rules::parse("maxPerIP: 1\n");
        let mut seen = HashMap::new();
        let no_ip = Candidate { name: "s".into(), ..Default::default() };
        assert_eq!(r.hide(&no_ip, &mut seen), None);
        assert_eq!(r.hide(&no_ip, &mut seen), None);
    }

    #[test]
    fn takes_the_host_out_of_an_address() {
        assert_eq!(host_of("198.51.100.7:54210"), "198.51.100.7");
        assert_eq!(host_of("[2001:db8::1]:54210"), "2001:db8::1");
        assert_eq!(host_of("mx.example.com:54210"), "mx.example.com");
    }

    /// Every scalar in the shipped file carries a trailing comment; reading `false   # ...` as
    /// a value would turn all four switches on.
    #[test]
    fn a_trailing_comment_is_not_part_of_the_value() {
        let r = Rules::parse(DEFAULT_CONFIG);
        assert!(!r.hide_empty && !r.hide_locked && !r.hide_unjoinable);
        assert_eq!(r.max_per_ip, 0);
    }

    /// Single quotes are what keep a regex's backslashes intact in the file, so they must not
    /// survive into the pattern.
    #[test]
    fn quotes_come_off_a_scalar() {
        assert_eq!(unquote("'che4ts'"), "che4ts");
        assert_eq!(unquote("\"kaizo\""), "kaizo");
        assert_eq!(unquote("bare"), "bare");
        assert_eq!(unquote("'unbalanced"), "'unbalanced");
    }

    /// The defaults must actually parse into rules — an empty ruleset would silently filter
    /// nothing and look exactly like a working filter with nothing to catch.
    #[test]
    fn the_defaults_are_not_an_empty_ruleset() {
        let r = Rules::defaults();
        assert_eq!(r.name_contains.len(), 3, "{:?}", r.name_contains);
        assert_eq!(r.regexes.len(), 1);
        assert!(r.complaints.is_empty(), "{:?}", r.complaints);
    }

    /// A rule the Rust engine can't compile costs that rule and nothing else. ECMAScript has
    /// lookarounds and this doesn't, so a player who wrote one in FrostMod's file will hit it.
    #[test]
    fn an_uncompilable_regex_costs_only_itself() {
        let r = Rules::parse("names:\n  - spam\nregex:\n  - '(?<=x)y'\n");
        assert_eq!(r.regexes.len(), 0);
        assert_eq!(r.complaints.len(), 1, "the skipped rule should be visible: {:?}", r.complaints);
        assert!(hidden(&r, &candidate("SPAM")).is_some(), "the other rules still run");
    }

    /// A file written by hand, with the inline form the C++ also accepts.
    #[test]
    fn an_inline_scalar_beside_a_list_key_counts() {
        let r = Rules::parse("names: junk\n");
        assert!(hidden(&r, &candidate("JUNK server")).is_some());
    }

    /// A list item before any list key is a mistake worth reporting, not a rule.
    #[test]
    fn an_orphan_list_item_is_a_complaint() {
        let r = Rules::parse("  - stray\n");
        assert!(r.name_contains.is_empty());
        assert_eq!(r.complaints.len(), 1);
    }

    /// Nothing is dropped and the count is what the tab shows on its chip.
    #[test]
    fn marking_keeps_every_row_and_counts_the_hidden_ones() {
        let mut list = vec![
            world("Kaizo Cheats", "198.51.100.1:54210", 0),
            world("MXB-Central EU", "198.51.100.2:54210", 12),
        ];
        let n = mark(&Rules::defaults(), &mut list);
        assert_eq!(n, 1);
        assert_eq!(list.len(), 2, "a hidden row is still there to be revealed");
        assert_eq!(list[0].name, "MXB-Central EU", "busiest first");
        assert!(list[0].hidden.is_empty());
        assert!(!list[1].hidden.is_empty());
    }

    fn world(name: &str, address: &str, players: u32) -> WorldServer {
        WorldServer {
            name: name.into(),
            address: address.into(),
            joinable: true,
            lan_address: String::new(),
            players,
            max_players: 20,
            ping_ms: Some(30),
            passworded: false,
            location: String::new(),
            rating: String::new(),
            track: String::new(),
            track_layout: String::new(),
            categories: vec![],
            bikes: vec![],
            session: String::new(),
            race_length: String::new(),
            conditions: String::new(),
            realistic_weather: false,
            force_cockpit: false,
            no_aids: false,
            limited_tyre_sets: false,
            hidden: String::new(),
        }
    }
}
