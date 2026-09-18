//! A frozen roster of a track's corners and jumps, so "Turn 5" means the same corner next week.
//!
//! A grid index already is a stable track coordinate: [`crate::analysis::Trace`] builds it from
//! `pos * track_length`, so metre `i` is the same metre of track on every lap, and the review
//! already refuses a reference whose centreline differs by more than a metre. What distance
//! alone cannot give is a stable *number*. "Turn 5" is the fifth corner in the detected list,
//! and that list moves: on a lap where the rider rolls a jump there is one fewer feature, and
//! everything after it shifts up. Set a personal best with a slightly different line and the
//! whole track renumbers.
//!
//! So the numbers are assigned once, from the union of features over every lap the rider has on
//! the track, and never reassigned. A feature discovered later takes the next free number for
//! its kind, which means it can sit out of numeric order — a "Turn 9" between 4 and 5. That is
//! the price of never renumbering, and it is the right price: renumbering silently
//! re-attributes the live-cue history and any per-corner progress.
//!
//! Not built on the game's own centreline (`SPluginsTrackSegment_t`, which the recorder does
//! capture): it is the track builder's coarse driving line rather than the rutted berm people
//! ride, its corner count doesn't match what a rider feels, it says nothing about jumps or
//! whoops, and its distances have no origin in the recording — the start line's offset lives in
//! race data the recorder never writes.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::analysis::{Kind, Section};

/// Bumped when the shape of a stored map changes: an older file is discarded and rebuilt.
pub const VERSION: u32 = 1;

/// Cores whose middles sit this close, metre for metre, are the same feature seen from two laps.
pub const MATCH_M: f32 = 25.0;

/// One named thing on a track, as the rider is told it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landmark {
    pub kind: Kind,
    /// The stable key: `t5`, `j2`, `w1`.
    pub id: String,
    /// What the rider sees: "Turn 5".
    pub name: String,
    /// Metres past the start line: the middle of the core, averaged over the laps that have
    /// seen it, and the core itself.
    pub at: f32,
    pub core: (f32, f32),
    /// How many laps have contributed. Also the signal for a jump the rider used to take and
    /// now rolls.
    pub laps: u32,
}

/// A track's roster, as stored.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMap {
    pub version: u32,
    pub track_id: String,
    /// The centreline length the map was built against. A track rebuilt to a different length
    /// is a different track as far as the numbers are concerned.
    pub length: f32,
    pub marks: Vec<Landmark>,
}

/// The word and the slug for each kind, in `Kind`'s own order.
const WORDS: [(&str, &str); 5] =
    [("Straight", "s"), ("Turn", "t"), ("Jump", "j"), ("Rhythm", "r"), ("Whoops", "w")];

fn word_of(kind: Kind) -> (&'static str, &'static str) {
    WORDS[kind as usize]
}

impl TrackMap {
    pub fn empty(track_id: &str, length: f32) -> Self {
        TrackMap { version: VERSION, track_id: track_id.to_string(), length, marks: Vec::new() }
    }

    /// True when this map was built for a different build of the track, or by older code, and
    /// should be thrown away rather than trusted. `slack` is the same centreline tolerance the
    /// review uses when it picks a reference.
    pub fn stale(&self, length: f32, slack: f32) -> bool {
        self.version != VERSION || (self.length - length).abs() > slack
    }

    /// The next free number for a kind: one past the highest already handed out, so a number is
    /// never reused even after the feature that held it stops being detected.
    fn next(&self, kind: Kind) -> usize {
        let (_, tag) = word_of(kind);
        self.marks
            .iter()
            .filter(|m| m.kind == kind)
            .filter_map(|m| m.id.strip_prefix(tag)?.parse::<usize>().ok())
            .max()
            .unwrap_or(0)
            + 1
    }

    fn insert(&mut self, kind: Kind, core: (usize, usize)) -> usize {
        let n = self.next(kind);
        let (word, tag) = word_of(kind);
        let core = (core.0 as f32, core.1 as f32);
        self.marks.push(Landmark {
            kind,
            id: format!("{tag}{n}"),
            name: format!("{word} {n}"),
            at: (core.0 + core.1) / 2.0,
            core,
            laps: 1,
        });
        self.marks.len() - 1
    }

    /// Folds one more sighting into a landmark, so its position is the average of the laps that
    /// have seen it rather than whichever lap happened to be first.
    fn seen(&mut self, i: usize, core: (usize, usize)) {
        let m = &mut self.marks[i];
        let n = m.laps as f32;
        let (a, b) = (core.0 as f32, core.1 as f32);
        m.core = ((m.core.0 * n + a) / (n + 1.0), (m.core.1 * n + b) / (n + 1.0));
        m.at = (m.core.0 + m.core.1) / 2.0;
        m.laps += 1;
    }

    /// The landmark this section is, if the map already knows it. Same kind, then the largest
    /// overlap of the cores — overlap first, because a corner detected forty metres longer on
    /// one lap is still the same corner — falling back to how close their middles are.
    fn find(&self, kind: Kind, core: (usize, usize)) -> Option<usize> {
        let (a, b) = (core.0 as f32, core.1 as f32);
        let mid = (a + b) / 2.0;
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.marks.iter().enumerate() {
            if m.kind != kind {
                continue;
            }
            let overlap = (b.min(m.core.1) - a.max(m.core.0)).max(0.0);
            let shorter = (b - a).min(m.core.1 - m.core.0).max(1.0);
            let near = (mid - m.at).abs() <= MATCH_M;
            if overlap < shorter / 2.0 && !near {
                continue;
            }
            // Prefer the biggest overlap; with none, the closest middle.
            let score = if overlap > 0.0 { overlap } else { -(mid - m.at).abs() };
            if best.is_none_or(|(_, s)| score > s) {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| i)
    }
}

/// Builds a roster from every lap the rider has on the track. Numbering runs along the lap, so
/// a map built from a session's own laps comes out in order on day one.
pub fn build(track_id: &str, length: f32, laps: &[Vec<Section>]) -> TrackMap {
    let mut map = TrackMap::empty(track_id, length);
    // Every sighting first, so the roster is the union rather than whatever the first lap saw.
    let mut seen: Vec<(Kind, (usize, usize))> = Vec::new();
    for secs in laps {
        for s in secs.iter().filter(|s| s.kind != Kind::Straight) {
            seen.push((s.kind, s.core));
        }
    }
    // Cluster before numbering: the numbers have to run along the lap, and a feature only one
    // lap saw still deserves its place in that order.
    let mut clusters: Vec<(Kind, (f32, f32), u32)> = Vec::new();
    for (kind, core) in seen {
        let (a, b) = (core.0 as f32, core.1 as f32);
        let mid = (a + b) / 2.0;
        let hit = clusters.iter().position(|(k, c, _)| {
            *k == kind && {
                let overlap = (b.min(c.1) - a.max(c.0)).max(0.0);
                let shorter = (b - a).min(c.1 - c.0).max(1.0);
                overlap >= shorter / 2.0 || ((c.0 + c.1) / 2.0 - mid).abs() <= MATCH_M
            }
        });
        match hit {
            Some(i) => {
                let (_, c, n) = &mut clusters[i];
                let f = *n as f32;
                *c = ((c.0 * f + a) / (f + 1.0), (c.1 * f + b) / (f + 1.0));
                *n += 1;
            }
            None => clusters.push((kind, (a, b), 1)),
        }
    }
    clusters.sort_by(|x, y| ((x.1).0 + (x.1).1).total_cmp(&((y.1).0 + (y.1).1)));
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for (kind, core, laps) in clusters {
        let n = counts.entry(kind as usize).and_modify(|c| *c += 1).or_insert(1);
        let (word, tag) = word_of(kind);
        map.marks.push(Landmark {
            kind,
            id: format!("{tag}{n}"),
            name: format!("{word} {n}"),
            at: (core.0 + core.1) / 2.0,
            core,
            laps,
        });
    }
    map
}

/// Stamps the map's ids and names onto a freshly detected list, adding anything it has never
/// seen. Returns true when the map gained or moved something and is worth writing back.
///
/// Straights keep their own positional names: they are the leftovers between features, they
/// move whenever a feature's bounds move, and nobody says "meet me at straight four".
pub fn apply(map: &mut TrackMap, secs: &mut [Section]) -> bool {
    let mut changed = false;
    for s in secs.iter_mut().filter(|s| s.kind != Kind::Straight) {
        let i = match map.find(s.kind, s.core) {
            Some(i) => {
                map.seen(i, s.core);
                i
            }
            None => {
                changed = true;
                map.insert(s.kind, s.core)
            }
        };
        s.id = map.marks[i].id.clone();
        s.name = map.marks[i].name.clone();
    }
    changed
}

/// The file a track's map lives in, under the app's data dir.
pub fn file_name(track: &str) -> String {
    format!("{}.json", crate::cues::safe_name(track))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{lap, Style, FAST};
    use crate::analysis::sections;

    fn ids(secs: &[Section]) -> Vec<(&str, &str)> {
        secs.iter().map(|s| (s.id.as_str(), s.name.as_str())).collect()
    }

    fn id_of(secs: &[Section], name: &str) -> String {
        secs.iter().find(|s| s.name == name).map(|s| s.id.clone()).unwrap_or_default()
    }

    #[test]
    fn a_section_carries_its_positional_slug_until_a_map_names_it() {
        let secs = sections(&lap(&FAST));
        let all = ids(&secs);
        assert!(all.contains(&("t1", "Turn 1")), "{all:?}");
        assert!(all.contains(&("t2", "Turn 2")), "{all:?}");
        assert!(all.contains(&("j1", "Jump 1")), "{all:?}");
    }

    #[test]
    fn a_corner_keeps_its_number_when_the_reference_changes() {
        // One lap rolls the jump, so it has one feature fewer and everything after it would
        // shift on the positional numbering.
        let jumped = sections(&lap(&FAST));
        let rolled = sections(&lap(&Style { jump: (0.0, 0.0, 0.0), ..FAST }));
        let mut map = build("indiana", 600.0, &[jumped.clone(), rolled.clone()]);

        let mut a = jumped;
        let mut b = rolled;
        apply(&mut map, &mut a);
        apply(&mut map, &mut b);
        assert_eq!(id_of(&a, "Turn 2"), id_of(&b, "Turn 2"));
        assert!(!id_of(&a, "Turn 2").is_empty());
    }

    #[test]
    fn the_map_is_built_from_every_lap_so_a_rolled_jump_is_still_a_landmark() {
        let jumped = sections(&lap(&FAST));
        let rolled = sections(&lap(&Style { jump: (0.0, 0.0, 0.0), ..FAST }));
        let map = build("indiana", 600.0, &[rolled, jumped]);
        assert!(map.marks.iter().any(|m| m.kind == Kind::Jump), "{:?}", map.marks);
    }

    #[test]
    fn a_feature_nobody_has_named_gets_the_next_free_number() {
        let plain = sections(&lap(&FAST));
        let mut map = build("indiana", 600.0, &[plain]);
        let before: Vec<String> = map.marks.iter().map(|m| m.id.clone()).collect();

        // A second jump the map has never seen, out on the straight — an air feature inside a
        // corner is folded into that corner and never becomes a jump of its own.
        let mut extra = sections(&lap(&Style { hop: (280.0, 295.0, 2.0), ..FAST }));
        assert!(apply(&mut map, &mut extra), "the map should have gained something");
        let after: Vec<String> = map.marks.iter().map(|m| m.id.clone()).collect();
        // Everything it already knew keeps its id.
        for id in &before {
            assert!(after.contains(id), "{id} went missing from {after:?}");
        }
        assert!(after.len() > before.len(), "{after:?}");
    }

    #[test]
    fn a_rebuilt_track_starts_a_new_map() {
        let map = TrackMap::empty("indiana", 600.0);
        assert!(!map.stale(600.4, 1.0));
        assert!(map.stale(640.0, 1.0));
    }

    #[test]
    fn the_same_corner_seen_forty_metres_longer_is_still_the_same_corner() {
        let mut map = TrackMap::empty("indiana", 600.0);
        map.insert(Kind::Corner, (200.0 as usize, 260));
        let found = map.find(Kind::Corner, (190, 300));
        assert!(found.is_some(), "a longer read of the same corner should match");
        assert_eq!(map.marks[found.unwrap()].id, "t1");
    }

    #[test]
    fn a_number_is_never_reused_after_its_feature_stops_being_detected() {
        let mut map = TrackMap::empty("indiana", 600.0);
        map.insert(Kind::Corner, (100, 150));
        map.insert(Kind::Corner, (300, 350));
        map.marks.remove(0);
        // t1 is gone, but the next corner must not be handed t1 again: the cue history and any
        // per-corner progress are keyed on it.
        map.insert(Kind::Corner, (500, 550));
        assert_eq!(map.marks.last().unwrap().id, "t3");
    }
}
