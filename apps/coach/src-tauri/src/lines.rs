//! How the rider's line changes over a session, and how the track does.
//!
//! Every whole lap is laid on the metre grid (`analysis::Trace`) and compared, section by
//! section, with the fast lap's line: how far to one side each lap ran, and what that did to
//! the time. The ground under the line early in the session against late shows where the
//! track has cut up, since the game deforms it as it is ridden.

use serde::Serialize;

use crate::analysis::{sections, Kind, Section, Trace};

/// Points along the lap paths sent to the map: one every 2 m.
const STEP: usize = 2;
/// Laps' lines this far apart in a corner are two different lines.
const SPLIT_M: f32 = 1.0;
/// One line has to be at least this much quicker to be worth saying.
const FASTER_S: f32 = 0.08;
/// Ground this much lower late in the session than early has cut up.
const CUT_M: f32 = 0.06;
/// Early and late laps compared only when they rode this close to the same line.
const SAME_LINE_M: f32 = 1.0;
const MIN_LAPS_CUT: usize = 5;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapLine {
    pub lap: i32,
    pub time: f32,
    /// World x/z every 2 m.
    pub path: Vec<[f32; 2]>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionLine {
    pub lap: i32,
    /// Metres to the right of the fast line, averaged over the section's core.
    pub offset: f32,
    pub time: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub section: usize,
    pub name: String,
    /// `line` for a line that pays, `cut` for ground cutting up.
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lines {
    pub laps: Vec<LapLine>,
    pub sections: Vec<Section>,
    /// Per section, one row per lap.
    pub offsets: Vec<Vec<SectionLine>>,
    pub notes: Vec<Note>,
}

/// Mean distance to the right of the reference line over `a..=b`.
fn offset(lap: &Trace, r: &Trace, a: usize, b: usize) -> f32 {
    let end = b.min(lap.len() - 1).min(r.len() - 1);
    let (mut sum, mut n) = (0.0, 0);
    for i in a..=end {
        let h = r.bearing(i);
        let (dx, dz) = (lap.pts[i].x - r.pts[i].x, lap.pts[i].z - r.pts[i].z);
        sum += dx * h.cos() - dz * h.sin();
        n += 1;
    }
    if n == 0 { 0.0 } else { sum / n as f32 }
}

/// Mean height of the ground under the line, from the grounded points only.
fn ground(lap: &Trace, a: usize, b: usize) -> Option<f32> {
    let ys: Vec<f32> = lap.pts[a..=b.min(lap.len() - 1)].iter().filter(|q| !q.air).map(|q| q.y).collect();
    (!ys.is_empty()).then(|| ys.iter().sum::<f32>() / ys.len() as f32)
}

struct Row {
    lap: i32,
    offset: f32,
    time: f32,
    ground: Option<f32>,
}

fn mean(rows: &[Row], f: impl Fn(&Row) -> f32) -> f32 {
    rows.iter().map(f).sum::<f32>() / rows.len().max(1) as f32
}

/// `laps` in the order they were ridden; `reference` decides the sections and the fast line.
pub fn lines(laps: &[(i32, Trace)], reference: &Trace) -> Lines {
    let secs = sections(reference);
    let mut offsets = Vec::with_capacity(secs.len());
    let mut notes = Vec::new();

    for (si, s) in secs.iter().enumerate() {
        let (a, b) = s.core;
        let rows: Vec<Row> = laps
            .iter()
            .filter(|(_, t)| s.end < t.len())
            .map(|(n, t)| Row { lap: *n, offset: offset(t, reference, a, b), time: t.span(s.start, s.end), ground: ground(t, a, b) })
            .collect();
        offsets.push(rows.iter().map(|r| SectionLine { lap: r.lap, offset: r.offset, time: r.time }).collect());

        if s.kind == Kind::Corner && rows.len() >= 4 {
            if let Some(note) = line_that_pays(si, s, &rows) {
                notes.push(note);
            }
        }
        if rows.len() >= MIN_LAPS_CUT {
            if let Some(note) = cutting_up(si, s, &rows) {
                notes.push(note);
            }
        }
    }

    Lines {
        laps: laps
            .iter()
            .map(|(n, t)| LapLine { lap: *n, time: t.time(), path: t.pts.iter().step_by(STEP).map(|q| [q.x, q.z]).collect() })
            .collect(),
        sections: secs,
        offsets,
        notes,
    }
}

/// The laps split into two lines through a corner, and one of them is clearly quicker.
fn line_that_pays(si: usize, s: &Section, rows: &[Row]) -> Option<Note> {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|x, y| x.offset.total_cmp(&y.offset));
    let (at, width) = (1..sorted.len())
        .map(|i| (i, sorted[i].offset - sorted[i - 1].offset))
        .max_by(|x, y| x.1.total_cmp(&y.1))?;
    if width <= SPLIT_M || at < 2 || sorted.len() - at < 2 {
        return None;
    }
    let (left, right) = sorted.split_at(at);
    let avg = |g: &[&Row], f: fn(&Row) -> f32| g.iter().map(|r| f(r)).sum::<f32>() / g.len() as f32;
    let (lt, rt) = (avg(left, |r| r.time), avg(right, |r| r.time));
    if (lt - rt).abs() <= FASTER_S {
        return None;
    }
    let (fast, slow) = if lt < rt { (left, right) } else { (right, left) };
    let apart = avg(fast, |r| r.offset) - avg(slow, |r| r.offset);
    let tighter = apart * s.dir as f32 > 0.0;
    let way = if tighter { "tighter" } else { "wider" };
    let mut which: Vec<i32> = fast.iter().map(|r| r.lap + 1).collect();
    which.sort_unstable();
    let which: Vec<String> = which.iter().map(|n| n.to_string()).collect();
    Some(Note {
        section: si,
        name: s.name.clone(),
        kind: "line",
        title: format!("The {way} line is faster in {}", s.name),
        detail: format!(
            "On laps {} you took a line about {:.1} m {way} and were {:.2} s quicker through {}. Keep that line.",
            which.join(", "),
            apart.abs(),
            (lt - rt).abs(),
            s.name
        ),
    })
}

/// The ground under the same line sits lower late in the session than early: ruts forming.
fn cutting_up(si: usize, s: &Section, rows: &[Row]) -> Option<Note> {
    let third = (rows.len() / 3).max(2);
    let (early, late) = (&rows[..third], &rows[rows.len() - third..]);
    if (mean(early, |r| r.offset) - mean(late, |r| r.offset)).abs() >= SAME_LINE_M {
        return None;
    }
    let height = |g: &[Row]| {
        let ys: Vec<f32> = g.iter().filter_map(|r| r.ground).collect();
        (!ys.is_empty()).then(|| ys.iter().sum::<f32>() / ys.len() as f32)
    };
    let drop = height(early)? - height(late)?;
    if drop <= CUT_M {
        return None;
    }
    Some(Note {
        section: si,
        name: s.name.clone(),
        kind: "cut",
        title: format!("{} is cutting up", s.name),
        detail: format!(
            "By lap {} the ground on your line is about {:.0} cm lower than on lap {}. Ruts are forming: \
             ride in the rut rather than across it, or find fresh ground beside it.",
            late[late.len() - 1].lap + 1,
            drop * 100.0,
            early[0].lap + 1
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{lap, Style, FAST};

    fn titles(l: &Lines) -> Vec<String> {
        l.notes.iter().map(|n| n.title.clone()).collect()
    }

    #[test]
    fn the_same_lap_over_and_over_says_nothing() {
        let laps: Vec<(i32, Trace)> = (0..6).map(|n| (n, lap(&FAST))).collect();
        let out = lines(&laps, &lap(&FAST));
        assert!(out.notes.is_empty(), "{:?}", titles(&out));
        assert_eq!(out.laps.len(), 6);
        assert!(out.offsets.iter().flatten().all(|r| r.offset.abs() < 0.05));
    }

    #[test]
    fn a_line_that_pays_is_named() {
        let wide = Style { wide: 2.5, corner_v: 11.0, ..FAST };
        let laps = vec![(0, lap(&FAST)), (1, lap(&FAST)), (2, lap(&wide)), (3, lap(&wide))];
        let out = lines(&laps, &lap(&FAST));
        let t1 = out.notes.iter().find(|n| n.kind == "line" && n.name == "Turn 1").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert!(t1.title.contains("wider"), "{}", t1.title);
        assert!(t1.detail.contains("laps 3, 4"), "{}", t1.detail);
        assert!(!out.notes.iter().any(|n| n.kind == "line" && n.name == "Turn 2"), "same line in Turn 2");
    }

    #[test]
    fn a_corner_that_sinks_over_the_session_is_cutting_up() {
        let laps: Vec<(i32, Trace)> = [0.0, 0.0, 0.02, 0.05, 0.1, 0.12]
            .iter()
            .enumerate()
            .map(|(n, &sink)| (n as i32, lap(&Style { sink, ..FAST })))
            .collect();
        let out = lines(&laps, &lap(&FAST));
        let cut = out.notes.iter().find(|n| n.kind == "cut").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert_eq!(cut.name, "Turn 1");
        assert!(cut.detail.contains("about 11 cm"), "{}", cut.detail);
    }
}
