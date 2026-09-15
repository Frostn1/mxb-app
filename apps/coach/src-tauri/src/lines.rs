//! How the rider's line changes over a session, and how the track does.
//!
//! Every whole lap is laid on the metre grid (`analysis::Trace`) and compared, section by
//! section, with the fast lap's line: how far to one side each lap ran, and what that did to
//! the time. The ground under the line early in the session against late shows where the
//! track has cut up, since the game deforms it as it is ridden.

use serde::Serialize;

use crate::analysis::{sections, Kind, Section, Trace};

/// Points along the lap paths sent to the map: every metre, the grid's own spacing.
const STEP: usize = 1;
/// Laps' lines this far apart in a corner are two different lines.
const SPLIT_M: f32 = 1.0;
/// One line has to be at least this much quicker to be worth saying.
const FASTER_S: f32 = 0.08;
/// Ground this much lower late in the session than early has cut up.
const CUT_M: f32 = 0.06;
/// Early and late laps compared only when they rode this close to the same line.
const SAME_LINE_M: f32 = 1.0;
const MIN_LAPS_CUT: usize = 5;
/// A second line within this of the fast one is worth keeping: for passing, or once the fast
/// one ruts up.
const ALT_S: f32 = 0.25;
/// Corners this close, metres from one's end to the next one's start, set each other up: the
/// line out of the first is the line into the second, so they're judged together.
const LINK_M: usize = 30;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapLine {
    pub lap: i32,
    pub time: f32,
    /// World x/z every metre.
    pub path: Vec<[f32; 2]>,
    /// The bike's height at each point, for drawing the line in 3D.
    pub heights: Vec<f32>,
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

    // Corners close enough to set each other up are judged as a pair first. Where the pair
    // decides, the first corner's own note would point the wrong way, so it's left out.
    let mut paired = vec![false; secs.len()];
    let corners: Vec<usize> = (0..secs.len()).filter(|&i| secs[i].kind == Kind::Corner).collect();
    for w in corners.windows(2) {
        let (i, j) = (w[0], w[1]);
        let (a, b) = (&secs[i], &secs[j]);
        if b.start.saturating_sub(a.end) > LINK_M {
            continue;
        }
        let recs: Vec<ComboRow> = laps
            .iter()
            .filter(|(_, t)| b.end < t.len())
            .map(|(n, t)| ComboRow {
                lap: *n,
                off: [offset(t, reference, a.core.0, a.core.1), offset(t, reference, b.core.0, b.core.1)],
                first: t.span(a.start, a.end),
                both: t.span(a.start, b.end),
            })
            .collect();
        if let Some(mut note) = combo((&a.name, a.dir as i32), (&b.name, b.dir as i32), &recs) {
            note.section = i;
            paired[i] = true;
            notes.push(note);
        }
    }

    for (si, s) in secs.iter().enumerate() {
        let (a, b) = s.core;
        let rows: Vec<Row> = laps
            .iter()
            .filter(|(_, t)| s.end < t.len())
            .map(|(n, t)| Row { lap: *n, offset: offset(t, reference, a, b), time: t.span(s.start, s.end), ground: ground(t, a, b) })
            .collect();
        offsets.push(rows.iter().map(|r| SectionLine { lap: r.lap, offset: r.offset, time: r.time }).collect());

        if s.kind == Kind::Corner && rows.len() >= 4 && !paired[si] {
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
            .map(|(n, t)| LapLine {
                lap: *n,
                time: t.time(),
                path: t.pts.iter().step_by(STEP).map(|q| [q.x, q.z]).collect(),
                heights: t.pts.iter().step_by(STEP).map(|q| q.y).collect(),
            })
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
    let gap = (lt - rt).abs();
    let second = if gap <= ALT_S {
        let other = if tighter { "wider" } else { "tighter" };
        format!(" The {other} line is only {gap:.2} s slower, so keep it for passing, or for when yours cuts up.")
    } else {
        String::new()
    };
    let mut which: Vec<i32> = fast.iter().map(|r| r.lap + 1).collect();
    which.sort_unstable();
    let which: Vec<String> = which.iter().map(|n| n.to_string()).collect();
    Some(Note {
        section: si,
        name: s.name.clone(),
        kind: "line",
        title: format!("The {way} line is faster in {}", s.name),
        detail: format!(
            "On laps {} you took a line about {:.1} m {way} and were {:.2} s quicker through {}. Keep that line.{second}",
            which.join(", "),
            apart.abs(),
            gap,
            s.name
        ),
    })
}

/// One lap through two linked corners: its line in each (metres right of the fast line), its
/// time through the first, and through both.
struct ComboRow {
    lap: i32,
    off: [f32; 2],
    first: f32,
    both: f32,
}

/// Where laps split into two lines: the middle of the widest gap between their offsets, when
/// it's wider than `SPLIT_M` with at least two laps on each side.
fn split(mut offs: Vec<f32>) -> Option<f32> {
    offs.sort_by(f32::total_cmp);
    let (at, width) = (1..offs.len()).map(|i| (i, offs[i] - offs[i - 1])).max_by(|x, y| x.1.total_cmp(&y.1))?;
    (width > SPLIT_M && at >= 2 && offs.len() - at >= 2).then(|| (offs[at] + offs[at - 1]) / 2.0)
}

/// Two linked corners: the line that's quicker through the first on its own can leave you
/// badly placed for the second. Says so when the pair of lines that's quickest over both
/// corners isn't the one the first corner alone would pick. Corners are (name, direction).
fn combo(a: (&str, i32), b: (&str, i32), recs: &[ComboRow]) -> Option<Note> {
    if recs.len() < 4 {
        return None;
    }
    let cut = [split(recs.iter().map(|r| r.off[0]).collect())?, split(recs.iter().map(|r| r.off[1]).collect())?];
    let side = |r: &ComboRow| (r.off[0] >= cut[0], r.off[1] >= cut[1]);
    let mean = |g: &[&ComboRow], f: fn(&ComboRow) -> f32| g.iter().map(|r| f(r)).sum::<f32>() / g.len() as f32;
    // Each pair of lines ridden on at least two laps, with its time over both corners.
    let mut groups: Vec<((bool, bool), Vec<&ComboRow>)> = Vec::new();
    for r in recs {
        match groups.iter_mut().find(|(k, _)| *k == side(r)) {
            Some((_, g)) => g.push(r),
            None => groups.push((side(r), vec![r])),
        }
    }
    groups.retain(|(_, g)| g.len() >= 2);
    let best = groups.iter().min_by(|x, y| mean(&x.1, |r| r.both).total_cmp(&mean(&y.1, |r| r.both)))?;
    // The first corner on its own: which of its two lines is quicker through it.
    let (right, left): (Vec<&ComboRow>, Vec<&ComboRow>) = recs.iter().partition(|r| r.off[0] >= cut[0]);
    let alone_right = mean(&right, |r| r.first) < mean(&left, |r| r.first);
    if best.0 .0 == alone_right {
        return None;
    }
    // The quickest pair that keeps the first corner's own quicker line.
    let other = groups
        .iter()
        .filter(|(k, _)| k.0 == alone_right)
        .map(|(_, g)| mean(g, |r| r.both))
        .fold(f32::INFINITY, f32::min);
    let gain = other - mean(&best.1, |r| r.both);
    if !other.is_finite() || gain <= FASTER_S {
        return None;
    }
    // Right of the fast line is the tight side of a right-hander.
    let way = |right_side: bool, dir: i32| if (right_side as i32 * 2 - 1) * dir > 0 { "tight" } else { "wide" };
    let mut which: Vec<i32> = best.1.iter().map(|r| r.lap + 1).collect();
    which.sort_unstable();
    let which: Vec<String> = which.iter().map(|n| n.to_string()).collect();
    let first = way(best.0 .0, a.1);
    let first = first[..1].to_uppercase() + &first[1..];
    Some(Note {
        section: 0,
        name: a.0.to_string(),
        kind: "line",
        title: format!("{} and {} go together", a.0, b.0),
        detail: format!(
            "Through {} alone the {} line is quicker, but it leaves you badly placed for {}. {first} into {}, \
             then {} through {}, is {gain:.2} s quicker over both, as on laps {}.",
            a.0,
            way(alone_right, a.1),
            b.0,
            a.0,
            way(best.0 .1, b.1),
            b.0,
            which.join(", ")
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
    // Another line ridden through here is the way round the ruts.
    let other = split(rows.iter().map(|r| r.offset).collect())
        .and_then(|cut| {
            let late_right = mean(late, |r| r.offset) >= cut;
            let alt: Vec<&Row> = rows.iter().filter(|r| (r.offset >= cut) != late_right).collect();
            (alt.len() >= 2).then(|| {
                let d = alt.iter().map(|r| r.offset).sum::<f32>() / alt.len() as f32 - mean(late, |r| r.offset);
                let way = if d * s.dir as f32 > 0.0 { "tighter" } else { "wider" };
                let mut laps: Vec<i32> = alt.iter().map(|r| r.lap + 1).collect();
                laps.sort_unstable();
                let laps: Vec<String> = laps.iter().map(|n| n.to_string()).collect();
                format!(" On laps {} you took a line about {:.1} m {way}: try it as the ruts deepen.", laps.join(", "), d.abs())
            })
        })
        .unwrap_or_default();
    Some(Note {
        section: si,
        name: s.name.clone(),
        kind: "cut",
        title: format!("{} is cutting up", s.name),
        detail: format!(
            "By lap {} the ground on your line is about {:.0} cm lower than on lap {}. Ruts are forming: \
             ride in the rut rather than across it, or find fresh ground beside it.{other}",
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

    fn row(lap: i32, off: [f32; 2], first: f32, both: f32) -> ComboRow {
        ComboRow { lap, off, first, both }
    }

    #[test]
    fn a_second_line_nearly_as_quick_is_kept_for_passing() {
        let wide = Style { wide: 2.5, corner_v: 10.2, ..FAST };
        let laps = vec![(0, lap(&FAST)), (1, lap(&FAST)), (2, lap(&wide)), (3, lap(&wide))];
        let out = lines(&laps, &lap(&FAST));
        let t1 = out.notes.iter().find(|n| n.kind == "line" && n.name == "Turn 1").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert!(t1.detail.contains("tighter line is only") && t1.detail.contains("for passing"), "{}", t1.detail);
        // A line that's much slower isn't offered as a second one.
        let far = Style { wide: 2.5, corner_v: 11.0, ..FAST };
        let laps = vec![(0, lap(&FAST)), (1, lap(&FAST)), (2, lap(&far)), (3, lap(&far))];
        let out = lines(&laps, &lap(&FAST));
        assert!(!out.notes.iter().any(|n| n.detail.contains("for passing")), "{:?}", titles(&out));
    }

    #[test]
    fn linked_corners_are_judged_over_both() {
        // Tight into Turn 3 is quicker there, but leaves Turn 4 wide and slow; wide then tight
        // wins over the two.
        let recs = [
            row(0, [2.0, -2.0], 5.0, 12.0),
            row(1, [2.1, -2.2], 5.0, 12.0),
            row(2, [-2.0, 2.0], 5.2, 11.6),
            row(3, [-2.1, 2.1], 5.2, 11.6),
        ];
        let note = combo(("Turn 3", 1), ("Turn 4", 1), &recs).unwrap();
        assert_eq!(note.title, "Turn 3 and Turn 4 go together");
        assert!(note.detail.contains("alone the tight line is quicker"), "{}", note.detail);
        assert!(note.detail.contains("Wide into Turn 3, then tight through Turn 4, is 0.40 s"), "{}", note.detail);
        assert!(note.detail.contains("laps 3, 4"), "{}", note.detail);
    }

    #[test]
    fn linked_corners_that_agree_say_nothing_extra() {
        // The line that's quicker through Turn 3 is also part of the quickest pair.
        let recs = [
            row(0, [2.0, 2.0], 5.0, 11.5),
            row(1, [2.1, 2.2], 5.0, 11.5),
            row(2, [-2.0, -2.0], 5.2, 11.9),
            row(3, [-2.1, -2.1], 5.2, 11.9),
        ];
        assert!(combo(("Turn 3", 1), ("Turn 4", 1), &recs).is_none());
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
