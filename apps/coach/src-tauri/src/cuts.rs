//! Track cuts, as the recorder plugin writes them: `<game user folder>/mxbcoach/cuts/cuts.jsonl`,
//! one JSON line per cut, and `cuts/<track>.markers.json`, every centreline marker of a track
//! (FrostMod 0.49.13, `src/coachcuts.h`). Marker N is the start of the track's centreline
//! segment N, which is how a track maker can find it. Read-only: nothing here is written.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::coach::{load_config, session_dirs};
use crate::hud::coach_dir_of;

/// Most rides listed: the newest first.
const MAX_SESSIONS: usize = 20;

/// One line of `cuts.jsonl`. Fields an older or newer plugin leaves out take their default.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CutRow {
    pub at: String,
    pub session: String,
    pub track: String,
    pub rider: String,
    #[serde(rename(deserialize = "self"))]
    pub own: bool,
    pub source: String,
    pub confirmed: bool,
    pub lap: i32,
    pub marker: i32,
    pub to: i32,
    pub x: f32,
    pub z: f32,
    #[serde(rename(deserialize = "gain_m"))]
    pub gain_m: f32,
    #[serde(rename(deserialize = "penalty_ms"))]
    pub penalty_ms: i32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Marker {
    pub id: i32,
    pub x: f32,
    pub z: f32,
    #[serde(rename(deserialize = "at_m"))]
    pub at_m: f32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MarkersFile {
    markers: Vec<Marker>,
}

/// A marker and how many times the rider was cut there.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarkerCount {
    pub marker: i32,
    pub confirmed: u32,
    pub possible: u32,
}

/// One ride: an event in the game, on one track.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutSession {
    pub session: String,
    pub track: String,
    pub cuts: Vec<CutRow>,
    /// The rider's own cuts per marker, most confirmed first.
    pub by_marker: Vec<MarkerCount>,
    /// Every marker of the track, for the map. Empty when the plugin hasn't written them.
    pub markers: Vec<Marker>,
}

/// The rides in `jsonl`, newest first, `markers` giving each track's markers.
pub(crate) fn sessions_of(jsonl: &str, markers: impl Fn(&str) -> Vec<Marker>) -> Vec<CutSession> {
    let mut by: BTreeMap<(String, String), Vec<CutRow>> = BTreeMap::new();
    for line in jsonl.lines() {
        let Ok(row) = serde_json::from_str::<CutRow>(line.trim()) else { continue };
        if row.track.is_empty() {
            continue;
        }
        let key = (if row.session.is_empty() { row.at.get(..8).unwrap_or("").to_string() } else { row.session.clone() }, row.track.clone());
        by.entry(key).or_default().push(row);
    }
    let mut out: Vec<CutSession> = by
        .into_iter()
        .map(|((session, track), cuts)| {
            let mut counts: BTreeMap<i32, MarkerCount> = BTreeMap::new();
            for c in cuts.iter().filter(|c| c.own && c.marker >= 0) {
                let e = counts.entry(c.marker).or_insert(MarkerCount { marker: c.marker, confirmed: 0, possible: 0 });
                if c.confirmed {
                    e.confirmed += 1;
                } else {
                    e.possible += 1;
                }
            }
            let mut by_marker: Vec<MarkerCount> = counts.into_values().collect();
            by_marker.sort_by(|a, b| b.confirmed.cmp(&a.confirmed).then(b.possible.cmp(&a.possible)).then(a.marker.cmp(&b.marker)));
            let markers = markers(&track);
            CutSession { session, track, cuts, by_marker, markers }
        })
        .collect();
    out.sort_by(|a, b| b.session.cmp(&a.session));
    out.truncate(MAX_SESSIONS);
    out
}

/// A track name as the plugin makes it a file name (coachcuts::FileSafe).
fn file_safe(s: &str) -> String {
    let o: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' })
        .collect();
    if o.is_empty() { "track".into() } else { o }
}

fn markers_in(dir: &Path, track: &str) -> Vec<Marker> {
    fs::read_to_string(dir.join(format!("{}.markers.json", file_safe(track))))
        .ok()
        .and_then(|s| serde_json::from_str::<MarkersFile>(&s).ok())
        .map(|f| f.markers)
        .unwrap_or_default()
}

/// The rides with cuts, newest first. Empty when nothing has been written yet.
#[tauri::command]
pub fn coach_cuts(app: AppHandle) -> Result<Vec<CutSession>, String> {
    let cfg = load_config(&app);
    let dir = coach_dir_of(&session_dirs(&cfg)).ok_or("The game's user folder wasn't found.")?.join("cuts");
    let text = fs::read_to_string(dir.join("cuts.jsonl")).unwrap_or_default();
    Ok(sessions_of(&text, |t| markers_in(&dir, t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: &str = r#"{"at":"20261013-100100-000","session":"20261013-100000-000","track":"Benny MX","rider":"Benny","self":true,"source":"lap","confirmed":true,"lap":1,"marker":12,"to":13,"x":1.0,"z":2.0,"pos":0.4,"gain_m":30.0,"penalty_ms":0}
{"at":"20261013-100200-000","session":"20261013-100000-000","track":"Benny MX","rider":"Benny","self":true,"source":"watch","confirmed":false,"lap":2,"marker":27,"to":28,"x":1.0,"z":2.0,"pos":0.7,"gain_m":26.0,"penalty_ms":0}
{"at":"20261013-100300-000","session":"20261013-100000-000","track":"Benny MX","rider":"Benny","self":true,"source":"game","confirmed":true,"lap":3,"marker":12,"to":12,"x":1.0,"z":2.0,"pos":0.4,"gain_m":0.0,"penalty_ms":10000}
{"at":"20261013-100300-000","session":"20261013-100000-000","track":"Benny MX","rider":"Other","self":false,"source":"game","confirmed":true,"lap":3,"marker":5,"to":5,"x":1.0,"z":2.0,"pos":0.1,"gain_m":0.0,"penalty_ms":10000}
not json
{"at":"20261012-090000-000","session":"20261012-085900-000","track":"Old","rider":"","self":true,"source":"lap","confirmed":true,"lap":1,"marker":3,"to":4}"#;

    #[test]
    fn rides_newest_first_with_the_riders_own_counts() {
        let s = sessions_of(LINES, |t| if t == "Benny MX" { vec![Marker { id: 12, ..Default::default() }] } else { vec![] });
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].track, "Benny MX");
        assert_eq!(s[0].cuts.len(), 4);
        assert_eq!(
            s[0].by_marker,
            vec![MarkerCount { marker: 12, confirmed: 2, possible: 0 }, MarkerCount { marker: 27, confirmed: 0, possible: 1 }]
        );
        assert_eq!(s[0].markers.len(), 1);
        assert!(s[0].cuts.iter().any(|c| c.own && c.penalty_ms == 10000 && c.gain_m == 0.0));
        assert_eq!(s[1].track, "Old");
    }

    #[test]
    fn file_names_match_the_plugin() {
        assert_eq!(file_safe("a/b:c"), "a_b_c");
        assert_eq!(file_safe("Benny MX"), "Benny MX");
    }
}
