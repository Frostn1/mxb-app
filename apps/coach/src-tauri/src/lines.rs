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
/// Laps through a corner needed before one line can be told from another.
const MIN_LAPS_LINE: usize = 4;
/// Other riders' positions through a corner needed to say where it'll wear.
const BUSY_PASSES: usize = 30;
/// The crowd this far from the fast line rides a line of its own.
const BUSY_M: f32 = 1.0;
/// A position this far from the fast line isn't on it at all: off track, or another part of it.
const BESIDE_M: f32 = 8.0;

/// Which lap of the session this is. The game starts counting at lap 1 again in every stint,
/// and a session is every stint of one event, so the number alone doesn't name a lap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapId {
    pub lap: i32,
    pub stint: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapLine {
    #[serde(flatten)]
    pub id: LapId,
    pub time: f32,
    /// World x/z every metre.
    pub path: Vec<[f32; 2]>,
    /// The bike's height at each point, for drawing the line in 3D.
    pub heights: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionLine {
    #[serde(flatten)]
    pub id: LapId,
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
    /// Nobody else was on track. Where the track will rut is read off the other riders' lines,
    /// so riding alone there is nothing to read it from — the page says so rather than leaving
    /// the rider to wonder why that kind of note never appears.
    pub alone: bool,
    /// Enough whole laps in the session for one line to be told from another. Under this, no
    /// line note is possible however the rider rode.
    pub enough_laps: bool,
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
    id: LapId,
    offset: f32,
    time: f32,
    ground: Option<f32>,
}

fn mean(rows: &[Row], f: impl Fn(&Row) -> f32) -> f32 {
    rows.iter().map(f).sum::<f32>() / rows.len().max(1) as f32
}

fn mean_of(xs: &[f32]) -> f32 {
    xs.iter().sum::<f32>() / xs.len().max(1) as f32
}

/// The laps a note points at, named the way the session names them. Every stint starts
/// counting at lap 1 again, so once a note reaches across stints the number alone would send
/// the rider to the wrong lap; `stints` says whether the session has more than one.
fn which(ids: &[LapId], stints: bool) -> String {
    let mut ids = ids.to_vec();
    ids.sort_unstable_by_key(|i| (i.stint, i.lap));
    let join = |xs: &[String]| xs.join(", ");
    if !stints {
        return join(&ids.iter().map(|i| (i.lap + 1).to_string()).collect::<Vec<_>>());
    }
    let mut groups: Vec<(i32, Vec<String>)> = Vec::new();
    for i in &ids {
        let num = (i.lap + 1).to_string();
        match groups.iter_mut().find(|(st, _)| *st == i.stint) {
            Some((_, g)) => g.push(num),
            None => groups.push((i.stint, vec![num])),
        }
    }
    groups.iter().map(|(st, g)| format!("{} of stint {}", join(g), st + 1)).collect::<Vec<_>>().join(" and ")
}

/// `laps` in the order they were ridden, across every stint of the session; `reference` decides
/// the sections and the fast line. `others` is every other rider's world x/z the recorder saw,
/// for where the track will wear.
pub fn lines(laps: &[(LapId, Trace)], reference: &Trace, others: &[[f32; 2]]) -> Lines {
    let secs = sections(reference);
    let mut offsets = Vec::with_capacity(secs.len());
    let mut notes = Vec::new();
    // A session ridden in several stints numbers its laps from 1 in each, so a note that names
    // one has to say which stint it was in.
    let stints = laps.first().is_some_and(|(f, _)| laps.iter().any(|(id, _)| id.stint != f.stint));

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
                id: *n,
                off: [offset(t, reference, a.core.0, a.core.1), offset(t, reference, b.core.0, b.core.1)],
                first: t.span(a.start, a.end),
                both: t.span(a.start, b.end),
            })
            .collect();
        if let Some(mut note) = combo((&a.name, a.dir as i32), (&b.name, b.dir as i32), &recs, stints) {
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
            .map(|(n, t)| Row { id: *n, offset: offset(t, reference, a, b), time: t.span(s.start, s.end), ground: ground(t, a, b) })
            .collect();
        offsets.push(rows.iter().map(|r| SectionLine { id: r.id, offset: r.offset, time: r.time }).collect());

        if s.kind == Kind::Corner && rows.len() >= MIN_LAPS_LINE && !paired[si] {
            if let Some(note) = line_that_pays(si, s, &rows, stints) {
                notes.push(note);
            }
        }
        if rows.len() >= MIN_LAPS_CUT {
            if let Some(note) = cutting_up(si, s, &rows, stints) {
                notes.push(note);
            }
        }
        if s.kind == Kind::Corner {
            if let Some(note) = busy(si, s, reference, others, &rows) {
                notes.push(note);
            }
        }
    }

    Lines {
        laps: laps
            .iter()
            .map(|(n, t)| LapLine {
                id: *n,
                time: t.time(),
                path: t.pts.iter().step_by(STEP).map(|q| [q.x, q.z]).collect(),
                heights: t.pts.iter().step_by(STEP).map(|q| q.y).collect(),
            })
            .collect(),
        sections: secs,
        offsets,
        notes,
        alone: others.is_empty(),
        enough_laps: laps.len() >= MIN_LAPS_LINE,
    }
}

/// Where the other riders run through a corner. The game deforms the ground where it's
/// ridden, so the line most of them share is the one that ruts first.
fn busy(si: usize, s: &Section, r: &Trace, others: &[[f32; 2]], rows: &[Row]) -> Option<Note> {
    let (a, b) = (s.core.0, s.core.1.min(r.len().checked_sub(1)?));
    if b <= a + 1 {
        return None;
    }
    let near = |i: usize, p: &[f32; 2]| (p[0] - r.pts[i].x).powi(2) + (p[1] - r.pts[i].z).powi(2);
    let (lo_x, hi_x, lo_z, hi_z) = r.pts[a..=b].iter().fold((f32::MAX, f32::MIN, f32::MAX, f32::MIN), |m, q| {
        (m.0.min(q.x), m.1.max(q.x), m.2.min(q.z), m.3.max(q.z))
    });
    // Each position beside the corner's core, as metres to the right of the fast line there.
    let mut side: Vec<f32> = others
        .iter()
        .filter(|p| p[0] > lo_x - BESIDE_M && p[0] < hi_x + BESIDE_M && p[1] > lo_z - BESIDE_M && p[1] < hi_z + BESIDE_M)
        .filter_map(|p| {
            let i = (a..=b).min_by(|&i, &j| near(i, p).total_cmp(&near(j, p)))?;
            // Past either end of the core it's before or after the corner, not beside it.
            if i == a || i == b || near(i, p) > BESIDE_M * BESIDE_M {
                return None;
            }
            let h = r.bearing(i);
            Some((p[0] - r.pts[i].x) * h.cos() - (p[1] - r.pts[i].z) * h.sin())
        })
        .collect();
    if side.len() < BUSY_PASSES {
        return None;
    }
    side.sort_by(f32::total_cmp);
    let crowd = side[side.len() / 2];
    let name = &s.name;
    let yours = mean(rows, |x| x.offset);
    // Where the others' lines fall into two groups far enough apart to be two ruts rather than
    // one spread, each one is a line with a name: the inside rut and the outside rut. Riders
    // talk about them that way, and "which rut" is the thing they want told.
    let (title, detail) = if let Some(between) = split(side.clone()) {
        let (a, b): (Vec<f32>, Vec<f32>) = side.iter().partition(|x| **x < between);
        let (in_off, out_off) = (mean_of(&a), mean_of(&b));
        let (first, second) = (side_word(in_off, s.dir), side_word(out_off, s.dir));
        let (busier, busier_n) = if a.len() >= b.len() { (first, a.len()) } else { (second, b.len()) };
        let ridden = side_word(yours, s.dir);
        (
            format!("Two ruts in {name}: the {first} and the {second}"),
            format!(
                "The other riders split into two lines through {name}, about {:.1} m apart — {busier_n} of them on \
                 the {busier}. You ride the {ridden}. The busier one ruts first, so when it does, take the other.",
                (out_off - in_off).abs()
            ),
        )
    } else if crowd.abs() <= BUSY_M {
        let away = other_side(side_word(yours, s.dir));
        (
            format!("Everyone rides the same line in {name}"),
            format!(
                "The other riders all run within a metre of the fast lap through {name}, so that is where it will rut \
                 first. When it does, move to the {away} — a metre or two off it stays smoother."
            ),
        )
    } else {
        let theirs = side_word(crowd, s.dir);
        let same = !rows.is_empty() && (yours - crowd).abs() <= BUSY_M;
        (
            format!("The others ride the {theirs} in {name}"),
            format!(
                "The other riders run about {:.1} m to the {theirs} of the fast lap through {name}, so that is where \
                 it will rut first, and the fast lap's line stays smoother.{}",
                crowd.abs(),
                if same {
                    format!(" You ride the {theirs} too: move onto the fast lap's line.")
                } else {
                    String::new()
                }
            ),
        )
    };
    Some(Note { section: si, name: name.clone(), kind: "wear", title, detail })
}

/// Which side of the fast lap a line sits on, in the words a rider actually uses.
///
/// Through a corner that is the inside or the outside — what the line is called when anyone
/// talks about it. Off a corner there is no inside, so it is left or right of the fast lap
/// instead. `offset` is metres to the right of the fast lap, `dir` +1 for a right-hander and
/// -1 for a left: a line to the right of a right-hander is the inside of it.
fn side_word(offset: f32, dir: i8) -> &'static str {
    if dir == 0 {
        if offset > 0.0 { "right" } else { "left" }
    } else if offset * dir as f32 > 0.0 {
        "inside"
    } else {
        "outside"
    }
}

/// The other side. "Take the inside" is only half a tip without somewhere to go from.
fn other_side(side: &str) -> &'static str {
    match side {
        "inside" => "outside",
        "outside" => "inside",
        "right" => "left",
        _ => "right",
    }
}

/// The laps split into two lines through a corner, and one of them is clearly quicker.
fn line_that_pays(si: usize, s: &Section, rows: &[Row], stints: bool) -> Option<Note> {
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
    let way = side_word(apart, s.dir);
    let gap = (lt - rt).abs();
    let second = if gap <= ALT_S {
        let other = other_side(way);
        format!(" The {other} line is only {gap:.2} s slower, so keep it for passing, or for when yours cuts up.")
    } else {
        String::new()
    };
    let on = which(&fast.iter().map(|r| r.id).collect::<Vec<_>>(), stints);
    Some(Note {
        section: si,
        name: s.name.clone(),
        kind: "line",
        title: format!("The {way} line is faster in {}", s.name),
        detail: format!(
            "On laps {} you took the {way} line, about {:.1} m off the other one, and were {:.2} s quicker through \
             {}. Keep that line.{second}",
            on,
            apart.abs(),
            gap,
            s.name
        ),
    })
}

/// One lap through two linked corners: its line in each (metres right of the fast line), its
/// time through the first, and through both.
struct ComboRow {
    id: LapId,
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
fn combo(a: (&str, i32), b: (&str, i32), recs: &[ComboRow], stints: bool) -> Option<Note> {
    if recs.len() < MIN_LAPS_LINE {
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
    // Right of the fast lap is the inside of a right-hander.
    let way = |right_side: bool, dir: i32| side_word((right_side as i32 * 2 - 1) as f32, dir as i8);
    let on = which(&best.1.iter().map(|r| r.id).collect::<Vec<_>>(), stints);
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
            on
        ),
    })
}

/// The ground under the same line sits lower late in the session than early: ruts forming.
fn cutting_up(si: usize, s: &Section, rows: &[Row], stints: bool) -> Option<Note> {
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
                let way = side_word(d, s.dir);
                format!(
                    " On laps {} you took the {way} line, about {:.1} m off it: try that as the ruts deepen.",
                    which(&alt.iter().map(|r| r.id).collect::<Vec<_>>(), stints),
                    d.abs()
                )
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
            which(&[late[late.len() - 1].id], stints),
            drop * 100.0,
            which(&[early[0].id], stints)
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

    /// Lap `n` of the session's first stint.
    fn id(n: i32) -> LapId {
        LapId { lap: n, stint: 0 }
    }

    #[test]
    fn the_same_lap_over_and_over_says_nothing() {
        let laps: Vec<(LapId, Trace)> = (0..6).map(|n| (id(n), lap(&FAST))).collect();
        let out = lines(&laps, &lap(&FAST), &[]);
        assert!(out.notes.is_empty(), "{:?}", titles(&out));
        assert_eq!(out.laps.len(), 6);
        assert!(out.offsets.iter().flatten().all(|r| r.offset.abs() < 0.05));
    }

    #[test]
    fn a_line_that_pays_is_named() {
        let wide = Style { wide: 2.5, corner_v: 11.0, ..FAST };
        let laps = vec![(id(0), lap(&FAST)), (id(1), lap(&FAST)), (id(2), lap(&wide)), (id(3), lap(&wide))];
        let out = lines(&laps, &lap(&FAST), &[]);
        let t1 = out.notes.iter().find(|n| n.kind == "line" && n.name == "Turn 1").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert!(t1.title.contains("outside"), "{}", t1.title);
        assert!(t1.detail.contains("laps 3, 4"), "{}", t1.detail);
        assert!(!out.notes.iter().any(|n| n.kind == "line" && n.name == "Turn 2"), "same line in Turn 2");
    }

    fn row(lap: i32, off: [f32; 2], first: f32, both: f32) -> ComboRow {
        ComboRow { id: id(lap), off, first, both }
    }

    #[test]
    fn a_second_line_nearly_as_quick_is_kept_for_passing() {
        let wide = Style { wide: 2.5, corner_v: 10.2, ..FAST };
        let laps = vec![(id(0), lap(&FAST)), (id(1), lap(&FAST)), (id(2), lap(&wide)), (id(3), lap(&wide))];
        let out = lines(&laps, &lap(&FAST), &[]);
        let t1 = out.notes.iter().find(|n| n.kind == "line" && n.name == "Turn 1").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert!(t1.detail.contains("inside line is only") && t1.detail.contains("for passing"), "{}", t1.detail);
        // A line that's much slower isn't offered as a second one.
        let far = Style { wide: 2.5, corner_v: 11.0, ..FAST };
        let laps = vec![(id(0), lap(&FAST)), (id(1), lap(&FAST)), (id(2), lap(&far)), (id(3), lap(&far))];
        let out = lines(&laps, &lap(&FAST), &[]);
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
        let note = combo(("Turn 3", 1), ("Turn 4", 1), &recs, false).unwrap();
        assert_eq!(note.title, "Turn 3 and Turn 4 go together");
        assert!(note.detail.contains("alone the inside line is quicker"), "{}", note.detail);
        assert!(note.detail.contains("Outside into Turn 3, then inside through Turn 4, is 0.40 s"), "{}", note.detail);
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
        assert!(combo(("Turn 3", 1), ("Turn 4", 1), &recs, false).is_none());
    }

    #[test]
    fn a_corner_that_sinks_over_the_session_is_cutting_up() {
        let laps: Vec<(LapId, Trace)> = [0.0, 0.0, 0.02, 0.05, 0.1, 0.12]
            .iter()
            .enumerate()
            .map(|(n, &sink)| (id(n as i32), lap(&Style { sink, ..FAST })))
            .collect();
        let out = lines(&laps, &lap(&FAST), &[]);
        let cut = out.notes.iter().find(|n| n.kind == "cut").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert_eq!(cut.name, "Turn 1");
        assert!(cut.detail.contains("about 11 cm"), "{}", cut.detail);
    }

    #[test]
    fn where_the_other_riders_crowd_is_where_it_will_wear() {
        let fast = lap(&FAST);
        let secs = sections(&fast);
        let (si, s) = secs.iter().enumerate().find(|(_, s)| s.kind == Kind::Corner && s.core.1 > s.core.0 + 4).unwrap();
        // Forty passes 2 m to the right of the fast line through the corner's core.
        let crowd: Vec<[f32; 2]> = (0..40)
            .map(|k| {
                let i = s.core.0 + 1 + k % (s.core.1 - s.core.0 - 1);
                let h = fast.bearing(i);
                [fast.pts[i].x + 2.0 * h.cos(), fast.pts[i].z - 2.0 * h.sin()]
            })
            .collect();
        let out = lines(&[], &fast, &crowd);
        let n = out.notes.iter().find(|n| n.kind == "wear" && n.section == si).expect("a wear note");
        assert!(n.detail.contains("about 2.0 m"), "{}", n.detail);
        assert!(lines(&[], &fast, &crowd[..10]).notes.iter().all(|n| n.kind != "wear"), "too few passes to say");
    }

    /// Every line note names a side a rider can act on. "Take the fast line" was the whole
    /// complaint: it is a summary, not somewhere to put the bike. The cue text has had this
    /// guard since the cues were written; the notes never did, which is how the words here
    /// survived a round of fixing the ones over in `analysis`.
    #[test]
    fn no_note_tells_the_rider_to_take_the_fast_line() {
        let wide = Style { wide: 2.5, corner_v: 11.0, ..FAST };
        let sunk = Style { wide: 2.5, corner_v: 10.2, ..FAST };
        let laps = vec![
            (id(0), lap(&FAST)),
            (id(1), lap(&FAST)),
            (id(2), lap(&wide)),
            (id(3), lap(&wide)),
            (id(4), lap(&sunk)),
            (id(5), lap(&sunk)),
        ];
        let out = lines(&laps, &lap(&FAST), &[]);
        assert!(!out.notes.is_empty(), "nothing to check");
        for n in &out.notes {
            for text in [&n.title, &n.detail] {
                assert!(!text.contains("fast line"), "\"{text}\" tells the rider nothing they can do");
                for vague in ["tighter", "wider", "tight line", "wide line"] {
                    assert!(!text.contains(vague), "\"{text}\" says {vague} where it could say inside or outside");
                }
            }
        }
    }

    /// A session is every stint of one event and the game starts counting at lap 1 again in
    /// each, so a note that named a lap by its number alone would send the rider to the wrong
    /// one. Reading the whole session is what makes the notes reachable at all — one stint of
    /// three laps never has enough of them.
    #[test]
    fn a_note_names_the_stint_a_lap_was_ridden_in() {
        let wide = Style { wide: 2.5, corner_v: 11.0, ..FAST };
        let laps = vec![
            (LapId { lap: 0, stint: 0 }, lap(&FAST)),
            (LapId { lap: 1, stint: 0 }, lap(&FAST)),
            (LapId { lap: 0, stint: 1 }, lap(&wide)),
            (LapId { lap: 1, stint: 1 }, lap(&wide)),
        ];
        let out = lines(&laps, &lap(&FAST), &[]);
        let t1 = out.notes.iter().find(|n| n.kind == "line" && n.name == "Turn 1").unwrap_or_else(|| panic!("{:?}", titles(&out)));
        assert!(t1.detail.contains("laps 1, 2 of stint 2"), "{}", t1.detail);
    }

    #[test]
    fn laps_are_named_by_their_stint_only_when_there_is_more_than_one() {
        let ids = [LapId { lap: 2, stint: 0 }, LapId { lap: 0, stint: 1 }];
        assert_eq!(which(&ids, false), "3, 1");
        assert_eq!(which(&ids, true), "3 of stint 1 and 1 of stint 2");
    }

    /// No note at all is the usual answer for a short session ridden alone, and the page says
    /// which of the two it was rather than showing an empty panel.
    #[test]
    fn a_session_with_no_notes_says_what_it_was_missing() {
        let few: Vec<(LapId, Trace)> = (0..2).map(|n| (id(n), lap(&FAST))).collect();
        let out = lines(&few, &lap(&FAST), &[]);
        assert!(out.notes.is_empty(), "{:?}", titles(&out));
        assert!(!out.enough_laps, "two laps can't tell one line from another");
        assert!(out.alone, "nobody else was on track");
        let many: Vec<(LapId, Trace)> = (0..MIN_LAPS_LINE as i32).map(|n| (id(n), lap(&FAST))).collect();
        let out = lines(&many, &lap(&FAST), &[[0.0, 0.0]]);
        assert!(out.enough_laps);
        assert!(!out.alone);
    }

    /// Inside and outside are the corner's own sides, not the rider's left and right: the
    /// inside of a left-hander is to the left of the fast lap, and of a right-hander, right.
    #[test]
    fn a_side_is_named_by_the_corner_it_is_in() {
        assert_eq!(side_word(1.0, 1), "inside", "right of a right-hander");
        assert_eq!(side_word(-1.0, 1), "outside");
        assert_eq!(side_word(-1.0, -1), "inside", "left of a left-hander");
        assert_eq!(side_word(1.0, -1), "outside");
        // Off a corner there is no inside, so it is simply which way off the fast lap.
        assert_eq!(side_word(1.0, 0), "right");
        assert_eq!(side_word(-1.0, 0), "left");
        assert_eq!(other_side("inside"), "outside");
        assert_eq!(other_side("outside"), "inside");
    }
}
