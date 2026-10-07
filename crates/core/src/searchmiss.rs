//! Anonymous "nothing found" reports from Browse.
//!
//! When a Browse search settles on a query and finds nothing, the webview hands the text here.
//! This is what lets the catalog's gaps be seen: the searches people actually run that no
//! source answers.
//!
//! ## What leaves the machine
//!
//! The query (trimmed, lowercased, whitespace collapsed, at most [`MAX_QUERY_CHARS`]) and the
//! id of the game it was searched under. Nothing else: no install id, no account, no Steam id,
//! no GUID. The request is signed with the same build signature the counters carry, which proves
//! the build and carries no identity.
//!
//! ## What stops it
//!
//! The counters' switch (`analytics_enabled`), [`usage::DISABLE_ENV`] for one run, and the
//! counters' debug-build rule. The same query is reported once per run, and a run reports at most
//! [`MAX_PER_RUN`] queries.

use crate::config::{self, AppConfig};
use crate::names::control_plane;
use crate::usage;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

/// The longest query that travels. The endpoint caps it too.
pub const MAX_QUERY_CHARS: usize = 80;
/// Shorter than this is a keystroke, not a search.
pub const MIN_QUERY_CHARS: usize = 2;
/// A run reports at most this many distinct queries.
pub const MAX_PER_RUN: usize = 40;

static SENT: Mutex<Option<HashSet<(String, String)>>> = Mutex::new(None);

/// Whether this run may report at all: exactly the counters' rule.
pub fn allowed(cfg: &AppConfig) -> bool {
    usage::allowed(cfg)
}

/// The query as it travels: trimmed, lowercased, one line, capped. `None` if too short.
pub fn normalise(query: &str) -> Option<String> {
    let line: String = query
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let capped: String = line.chars().take(MAX_QUERY_CHARS).collect();
    let capped = capped.trim().to_string();
    if capped.chars().count() < MIN_QUERY_CHARS {
        return None;
    }
    Some(capped)
}

/// Record `(query, game)` as sent. False if it was already sent this run, or the run is full.
fn first_time(set: &mut HashSet<(String, String)>, query: &str, game: &str) -> bool {
    if set.len() >= MAX_PER_RUN {
        return false;
    }
    set.insert((query.to_string(), game.to_string()))
}

#[derive(Debug, Serialize, PartialEq)]
struct Report<'a> {
    query: &'a str,
    game: &'a str,
}

/// A Browse search settled on `query` and found nothing.
///
/// Infallible and silent from the caller's side: a report that does not land is a data point
/// lost, which is not worth an error anywhere near a search box.
#[tauri::command]
pub async fn report_search_miss(app: tauri::AppHandle, query: String) {
    let Ok(cfg) = config::load(&app) else { return };
    if !allowed(&cfg) {
        return;
    }
    let Some(query) = normalise(&query) else { return };
    let game = cfg.active_game.id().to_string();
    {
        let mut guard = SENT.lock().unwrap();
        if !first_time(guard.get_or_insert_with(HashSet::new), &query, &game) {
            return;
        }
    }
    send(&query, &game).await;
}

async fn send(query: &str, game: &str) {
    let raw = match serde_json::to_string(&Report { query, game }) {
        Ok(raw) => raw,
        Err(_) => return,
    };
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(15)).build() else {
        return;
    };
    let mut request = client
        .post(format!("{}/v1/search-misses", control_plane()))
        .header("content-type", "application/json")
        .body(raw.clone());
    if let Some(header) = usage::signature(&raw) {
        request = request.header(usage::SIGNATURE_HEADER, header);
    }
    match request.send().await {
        Ok(res) if res.status().is_success() => {}
        Ok(res) => log::debug!("[searchmiss] came back {}", res.status()),
        Err(e) => log::debug!("[searchmiss] didn't send ({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_trimmed_lowercased_and_one_line() {
        assert_eq!(normalise("  Honda\tCR250 \n 2022 "), Some("honda cr250 2022".to_string()));
    }

    #[test]
    fn a_query_is_capped() {
        let long = normalise(&"a".repeat(MAX_QUERY_CHARS * 3)).unwrap();
        assert_eq!(long.chars().count(), MAX_QUERY_CHARS);
    }

    #[test]
    fn nothing_or_a_single_character_is_not_a_search() {
        assert_eq!(normalise("   "), None);
        assert_eq!(normalise("a"), None);
        assert_eq!(normalise(" \n "), None);
    }

    #[test]
    fn the_same_query_is_reported_once() {
        let mut set = HashSet::new();
        assert!(first_time(&mut set, "foo", "mxb"));
        assert!(!first_time(&mut set, "foo", "mxb"));
        // Another game's catalog is a different gap.
        assert!(first_time(&mut set, "foo", "gp"));
    }

    #[test]
    fn a_run_stops_reporting_at_the_cap() {
        let mut set = HashSet::new();
        for i in 0..MAX_PER_RUN {
            assert!(first_time(&mut set, &format!("q{i}"), "mxb"));
        }
        assert!(!first_time(&mut set, "one more", "mxb"));
    }

    #[test]
    fn the_payload_is_the_query_and_the_game_and_nothing_else() {
        let raw = serde_json::to_string(&Report { query: "foo", game: "mxb" }).unwrap();
        assert_eq!(raw, r#"{"query":"foo","game":"mxb"}"#);
    }
}
