//! The ideal lap as a line to ride.
//!
//! The ideal lap is the rider's best time for each section, from whichever lap set it. This builds
//! the matching path: for each section the path, speed, brake, gas, gear and air of the lap that
//! set that section's best, joined at the section boundaries so the line has no kink.
//!
//! The sheet format is unchanged: the result is an ordinary [`Trace`] the HUD sheet writer takes
//! in place of the fastest lap, so DRIV, REFY, GEAR and the air share (the recorder's jump calls)
//! all come from the lap used in each stretch.
//!
//! Two laps never ride the same line, so a join is made where they are closest, not where the
//! section happens to end: the boundary moves to the nearest point within [`WINDOW_M`] where the
//! two laps are within [`CONVERGE_M`] of each other, and the two are blended across
//! [`BLEND_M`] either side of it (position, height, speed, brake, gas, time). Where they never
//! converge, the earlier lap carries on through the next section and the note says so.

use crate::analysis::{Point, Trace};

/// How far either side of a section boundary to look for the two laps to meet.
pub const WINDOW_M: usize = 16;
/// Half the width of the blend at a join.
pub const BLEND_M: usize = 8;
/// Laps further apart than this at a join don't meet there.
pub const CONVERGE_M: f32 = 2.0;
/// A step of the finished line, in the plane, that is not one metre of riding.
const MAX_STEP_M: f32 = 3.0;
/// And how far it may climb or drop in a metre.
const MAX_RISE_M: f32 = 2.5;

/// A stretch of the finished line and the lap it was ridden on (an index into the laps given).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub start: usize,
    pub end: usize,
    pub lap: usize,
}

pub struct Stitched {
    pub trace: Trace,
    pub pieces: Vec<Piece>,
    pub notes: Vec<String>,
}

fn smooth(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

fn gap(a: &Point, b: &Point) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// The ideal line: section `i` (its first and last metre) from `laps[best[i]]`, the rest (and a lap that doesn't fit the
/// grid) from `base`. None when the result isn't a line worth riding (see the module docs).
pub fn stitch(sections: &[(usize, usize)], best: &[usize], laps: &[&Trace], base: usize) -> Option<Stitched> {
    let n = laps.get(base)?.len();
    if n < 4 * WINDOW_M || best.len() != sections.len() {
        return None;
    }
    let usable = |l: usize| laps.get(l).is_some_and(|t| t.len() == n);
    // Who rides each metre.
    let mut owner = vec![base; n];
    for (&(start, end), &l) in sections.iter().zip(best) {
        if !usable(l) {
            continue;
        }
        for m in start..=end.min(n - 1) {
            owner[m] = l;
        }
    }
    // Runs of one lap.
    let mut runs: Vec<Piece> = Vec::new();
    for (m, &l) in owner.iter().enumerate() {
        match runs.last_mut() {
            Some(r) if r.lap == l => r.end = m,
            _ => runs.push(Piece { start: m, end: m, lap: l }),
        }
    }
    // Joins: where one run hands to the next, the boundary moves to where the two meet.
    let mut notes = Vec::new();
    let mut joined: Vec<Piece> = vec![runs[0].clone()];
    let mut joins: Vec<usize> = Vec::new(); // the join point between joined[k] and joined[k+1]
    for next in runs.into_iter().skip(1) {
        let last = joined.last().unwrap().clone();
        let b = next.start;
        let lo = (last.start + BLEND_M + 1).max(b.saturating_sub(WINDOW_M));
        let hi = (next.end.saturating_sub(BLEND_M + 1)).min(b + WINDOW_M);
        // A join is judged over the whole stretch it blends, not at one metre: two laps that
        // cross each other still sit well apart a few metres either side.
        let (mut at, mut d_min) = (0usize, f32::MAX);
        if lo <= hi {
            for c in lo..=hi {
                let d = (c - BLEND_M..c + BLEND_M)
                    .map(|m| gap(&laps[last.lap].pts[m], &laps[next.lap].pts[m]))
                    .fold(0.0f32, f32::max);
                // The nearest to the boundary among equals.
                if d < d_min - 1e-3 || ((d - d_min).abs() <= 1e-3 && c.abs_diff(b) < at.abs_diff(b)) {
                    d_min = d;
                    at = c;
                }
            }
        }
        if lo <= hi && d_min <= CONVERGE_M {
            if at != b {
                notes.push(format!("join at {b} m moved to {at} m (at most {d_min:.1} m apart across the join)"));
            }
            let l = joined.last_mut().unwrap();
            l.end = at - 1;
            joins.push(at);
            joined.push(Piece { start: at, end: next.end, lap: next.lap });
        } else {
            let apart = if lo <= hi { d_min } else { gap(&laps[last.lap].pts[b], &laps[next.lap].pts[b]) };
            notes.push(format!(
                "at {b} m the laps are {apart:.1} m apart: the earlier lap's line carries on instead"
            ));
            joined.last_mut().unwrap().end = next.end;
        }
    }
    // Build the points.
    let mix = |m: usize, f: &dyn Fn(&Point) -> f32| -> f32 {
        let l0 = owner_at(&joined, m);
        for (k, &c) in joins.iter().enumerate() {
            if m + BLEND_M >= c && m < c + BLEND_M {
                let w = smooth((m + BLEND_M - c) as f32 / (2 * BLEND_M) as f32);
                let (a, b) = (joined[k].lap, joined[k + 1].lap);
                return f(&laps[a].pts[m]) * (1.0 - w) + f(&laps[b].pts[m]) * w;
            }
        }
        f(&laps[l0].pts[m])
    };
    let dt = |p: &Point, q: &Point| (q.t - p.t).max(0.0);
    let mut out: Vec<Point> = Vec::with_capacity(n);
    let mut t = 0.0f32;
    for m in 0..n {
        let mut p = laps[owner_at(&joined, m)].pts[m];
        p.x = mix(m, &|q| q.x);
        p.y = mix(m, &|q| q.y);
        p.z = mix(m, &|q| q.z);
        p.v = mix(m, &|q| q.v);
        p.throttle = mix(m, &|q| q.throttle);
        p.front = mix(m, &|q| q.front);
        p.rear = mix(m, &|q| q.rear);
        p.t = t;
        out.push(p);
        if m + 1 < n {
            t += mix_dt(&joined, &joins, laps, m, &dt);
        }
    }
    // A line worth riding: no jumps in the plane or in height, about the length of the lap it
    // stands in for.
    let mut len = 0.0f32;
    for w in out.windows(2) {
        let d = gap(&w[0], &w[1]);
        if !d.is_finite() || d > MAX_STEP_M || (w[1].y - w[0].y).abs() > MAX_RISE_M || !w[1].t.is_finite() {
            return None;
        }
        len += d;
    }
    let base_len: f32 = laps[base].pts.windows(2).map(|w| gap(&w[0], &w[1])).sum();
    if base_len > 0.0 && (len - base_len).abs() > 0.03 * base_len {
        return None;
    }
    Some(Stitched { trace: Trace { pts: out }, pieces: joined, notes })
}

fn owner_at(joined: &[Piece], m: usize) -> usize {
    joined.iter().find(|p| m >= p.start && m <= p.end).map_or(joined[0].lap, |p| p.lap)
}

/// The time from metre `m` to the next on the finished line: each lap's own, blended at joins.
fn mix_dt(
    joined: &[Piece],
    joins: &[usize],
    laps: &[&Trace],
    m: usize,
    dt: &dyn Fn(&Point, &Point) -> f32,
) -> f32 {
    let d = |l: usize| dt(&laps[l].pts[m], &laps[l].pts[m + 1]);
    for (k, &c) in joins.iter().enumerate() {
        if m + BLEND_M >= c && m < c + BLEND_M {
            let w = smooth((m + BLEND_M - c) as f32 / (2 * BLEND_M) as f32);
            return d(joined[k].lap) * (1.0 - w) + d(joined[k + 1].lap) * w;
        }
    }
    d(owner_at(joined, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight road along x, 1 m a point, ridden `off` metres to the side over the stretch
    /// `bump`, at `speed` m/s, with the gear and the air as given.
    fn lap(n: usize, speed: f32, off: f32, bump: std::ops::Range<usize>, gear: i32, air: bool) -> Trace {
        let pts = (0..n)
            .map(|i| Point {
                t: i as f32 / speed,
                v: speed,
                x: i as f32,
                y: 0.0,
                z: if bump.contains(&i) { off } else { 0.0 },
                gear,
                air: air && i > 100 && i < 110,
                ..Default::default()
            })
            .collect();
        Trace { pts }
    }

    #[test]
    fn two_laps_on_one_line_join_where_the_section_ends_and_the_time_adds_up() {
        let a = lap(400, 20.0, 0.0, 0..0, 3, false);
        let b = lap(400, 25.0, 0.0, 0..0, 4, true);
        let secs = [(0, 199), (200, 399)];
        let s = stitch(&secs, &[0, 1], &[&a, &b], 0).expect("a line");
        assert_eq!(s.pieces.len(), 2);
        assert_eq!(s.pieces[0].lap, 0);
        assert_eq!(s.pieces[1].lap, 1);
        assert_eq!(s.trace.pts[50].gear, 3);
        assert_eq!(s.trace.pts[350].gear, 4);
        // Time: 200 m at 20, then 199 m at 25, near enough (the blend moves a fraction).
        let want = 200.0 / 20.0 + 199.0 / 25.0;
        let got = s.trace.pts[399].t;
        assert!((got - want).abs() < 0.3, "time {got} vs {want}");
        assert!(s.trace.pts.windows(2).all(|w| w[1].t >= w[0].t), "time only rises");
    }

    #[test]
    fn a_join_moves_to_where_the_laps_meet_and_blends_without_a_kink() {
        // Lap B rides 1.5 m wide until metre 205, then joins A's line; the section break is at 200.
        let a = lap(400, 20.0, 0.0, 0..0, 3, false);
        let b = lap(400, 25.0, 1.5, 0..205, 4, false);
        let secs = [(0, 199), (200, 399)];
        let s = stitch(&secs, &[0, 1], &[&a, &b], 0).expect("a line");
        assert!(!s.notes.is_empty());
        let max_step = s.trace.pts.windows(2).map(|w| gap(&w[0], &w[1])).fold(0.0, f32::max);
        assert!(max_step < 1.6, "no kink: biggest step {max_step}");
        // Sideways, it never moves more than a little per metre.
        let max_side = s.trace.pts.windows(2).map(|w| (w[1].z - w[0].z).abs()).fold(0.0, f32::max);
        assert!(max_side < 0.2, "lateral change {max_side} m a metre");
    }

    #[test]
    fn laps_that_never_meet_leave_the_earlier_line_in_place() {
        let a = lap(400, 20.0, 0.0, 0..0, 3, false);
        let b = lap(400, 25.0, 6.0, 0..400, 4, false);
        let secs = [(0, 199), (200, 399)];
        let s = stitch(&secs, &[0, 1], &[&a, &b], 0).expect("a line");
        assert!(s.notes.iter().any(|n| n.contains("apart")), "{:?}", s.notes);
        assert_eq!(s.pieces.len(), 1);
        assert!(s.trace.pts.iter().all(|p| p.z.abs() < 1e-3), "all of it is lap A's line");
    }

    #[test]
    fn the_air_comes_from_the_lap_used_in_each_section() {
        let a = lap(400, 20.0, 0.0, 0..0, 3, true); // airborne at 101..110
        let b = lap(400, 25.0, 0.0, 0..0, 4, false);
        let secs = [(0, 199), (200, 399)];
        let s = stitch(&secs, &[1, 0], &[&a, &b], 0).expect("a line");
        assert!(s.trace.pts[105].air == false, "section 1 is lap B's, which stayed on the ground");
        let s = stitch(&secs, &[0, 1], &[&a, &b], 0).unwrap();
        assert!(s.trace.pts[105].air, "section 1 is lap A's, which was in the air");
    }

    #[test]
    fn a_lap_that_does_not_fit_the_grid_is_not_used() {
        let a = lap(400, 20.0, 0.0, 0..0, 3, false);
        let b = lap(390, 25.0, 0.0, 0..0, 4, false);
        let secs = [(0, 199), (200, 399)];
        let s = stitch(&secs, &[0, 1], &[&a, &b], 0).expect("a line");
        assert_eq!(s.pieces.len(), 1);
    }
}
