//! A lap from a seed, with the arithmetic to prove it before it is built.
//!
//! There are two halves to making a track and they fail differently. The *shape* — where the
//! lap goes — is geometry, and geometry is checkable: a lap either closes or it does not,
//! either crosses itself or does not, either carries the corners and jumps a published track
//! carries or does not. The *character* — sand or loam, tight or flowing — is taste, and taste
//! is what a model is for. This module does the first half, on its own, from a number.
//!
//! The shape is a closed loop whose radius wobbles with the angle round its centre. That gives
//! two properties for free, and they are the two that every attempt at this failed on:
//!
//! - it **closes**, because it goes all the way round;
//! - it **cannot cross itself**, because it is star-shaped — one radius per angle.
//!
//! Its corners are then filleted into arcs, which is what a `.tcl` is made of. What is left of
//! each edge is a straight.
//!
//! Everything after that is measurement. [`draw`] makes a lap; [`search`] makes laps until one
//! passes [`crate::trackllm::review`] *and* measures up as ground once synthesised — the
//! grooves it wears, the berms it grows, the faces its jumps stand at. A lap that reads well
//! on paper and rides like a ploughed field is one nobody wants, and the only way to know
//! which one this is, is to build it and measure.

use crate::trackprog::{Feature, Relief, Segment, Start, Surface, Terrain, TrackProgram};

/// The plot every generated track is laid out on.
///
/// Smaller than the 620 m it was, on purpose. A 1700 m lap inside a 620 m plot is shorter than
/// the plot's own perimeter — 2160 m round the inside of the margin — so a ring fits
/// comfortably and the walk has no reason to fold inwards. Indiana is 525 m across with a
/// 2138 m lap: its lap is *longer* than its perimeter, so it has to double back through its
/// own middle, and that is what a track looks like.
const PLOT_M: f32 = 470.0;

/// Under this radius a corner is no place for a takeoff: a rider cannot leave the ground
/// square out of a turn that tight.
const TIGHT_M: f32 = 22.0;

/// How far round the lap is left bare for the gate row and the run to the first jump.
const START_CLEAR_M: f32 = 130.0;

/// Names, so two tracks from two seeds are never the same track twice. Each takes its own
/// folder inside its own `.pkz`, which is what stops one installing over another.
const NAMES: [&str; 15] = [
    "Ashgrove National",
    "Blackpine Park",
    "Cold Harbour MX",
    "Draycott Valley",
    "Eastmoor Raceway",
    "Fallow Hill MX",
    "Greystone National",
    "Hollowbrook Park",
    "Ironbark Raceway",
    "Juniper Flats MX",
    "Kestrel Ridge National",
    "Long Marsh Park",
    "Millbrook MX",
    "Northgate Raceway",
    "Oakhanger National",
];

const PLACES: [&str; 12] = [
    "Somerset", "Wexford", "Alberta", "Nord-Pas", "Lombardy", "Otago", "Jutland", "Kalmar",
    "Wallonia", "Aragon", "Limburg", "Waikato",
];

/// A small deterministic generator. Not for anything that has to be unguessable — this only
/// has to give the same track for the same number, on every machine.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// A float in `[lo, hi)`.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() >> 11) as f32 / (1u64 << 53) as f32 * (hi - lo)
    }
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1).max(1) as u64) as i32
    }
    fn chance(&mut self, p: f32) -> bool {
        self.range(0.0, 1.0) < p
    }
}

/// What a lap is made of. `scripts/track-survey.py` prints this off the tracks' own `.trh`
/// centrelines; a corner is same-handed turning under a 300 m radius with under ten metres of
/// run let into it — Motorcycling Australia's own definition of a curve.
///
/// ```text
///                     Indiana   Southwick   all 18
///   lap                 2170 m     2217 m   1065-3055
///   corners           16 (7.4)   18 (8.1)   7.4-10.3 per km
///   arcs in a corner         4          5   1-6
///   a corner's angle      159°       166°   90-166 (p50 136)
///   apex radius         10.4 m     11.9 m   7.1-18.5
///   ground per corner     68 m       74 m   14-82
///   run between them      27 m       30 m   20-72
/// ```
const CORNERS_PER_KM: (f32, f32) = (7.4, 10.3);

/// `crate::trackllm::corpus::LAP_M` refuses anything over 2600.
const LAP_MAX_M: f32 = 2550.0;

/// How much of a lap is tight enough to wear a rut. Indiana measures 8.4% under a 14 m
/// radius and Southwick 7.3%.
const TIGHT_SHARE: f32 = 0.10;

/// The longest straight a rider may meet — the FFM's, and the only one any federation writes.
const STRAIGHT_CAP_M: f32 = 125.0;

/// The ground between the corners, as share of lap length by radius. A published lap is almost
/// never straight — 2.9% of Indiana and 0.8% of Southwick — and a third of it drifts through a
/// radius over 300 m, which rides as a straight and measures as an arc. Draw that band too
/// tight and it reads as part of the corner beside it, which moves two statistics at once.
///
/// ```text
///                    Indiana   Southwick
///   a true straight     2.9%        0.8%
///   R 40-80 m          16.7%       14.7%
///   R 80-160           14.1%       13.6%
///   R 160-300          11.2%       13.4%
///   R 300-1000         18.1%       19.0%
///   R over 1000        15.9%        8.2%
/// ```
const WANDER: [(f32, f32, f32); 5] = [
    (40.0, 80.0, 0.21),
    (80.0, 160.0, 0.19),
    (160.0, 300.0, 0.17),
    (300.0, 1000.0, 0.26),
    (1000.0, 3000.0, 0.17),
];

/// How far the walk may run before it must be closing, and how near it may come to itself.
const MARGIN_M: f32 = 26.0;
/// The start spur stands beside the lap and needs ground of its own.
const GATE_ROOM_M: f32 = 78.0;

fn seg_length(s: &Segment) -> f32 {
    match s {
        Segment::Straight { length, .. } => *length,
        Segment::Arc { radius, angle, .. } => radius.abs() * angle.to_radians(),
    }
}

fn chain_length(segs: &[Segment]) -> f32 {
    segs.iter().map(seg_length).sum()
}

/// Where a segment leaves you, in the app's own frame.
fn advance(pose: (f32, f32, f32), s: &Segment) -> (f32, f32, f32) {
    let (x, z, h) = pose;
    match s {
        Segment::Straight { length, .. } => {
            let (hx, hz) = crate::trackprog::heading_vector(h);
            (x + hx * length, z + hz * length, h)
        }
        Segment::Arc { radius, angle, .. } => {
            let a = angle.to_radians() * if *radius > 0.0 { 1.0 } else { -1.0 };
            let nh = h + a;
            (
                x + radius * (h.cos() - nh.cos()),
                z + radius * (nh.sin() - h.sin()),
                nh,
            )
        }
    }
}

/// The ground a segment covers: `(x, z, how far along the segment)`.
///
/// Each point carries its own distance, not the segment's. Giving a whole segment one age is
/// what made every walk paint itself in on the first move: the piece just laid ends where the
/// next one starts, so if its far end is dated from its own beginning it is still "old" track
/// sitting right under the new piece, and nothing is ever legal.
fn samples(pose: (f32, f32, f32), s: &Segment, step: f32, out: &mut Vec<(f32, f32, f32)>) {
    match s {
        Segment::Straight { length, .. } => {
            let n = ((length / step) as usize).max(1);
            let (hx, hz) = crate::trackprog::heading_vector(pose.2);
            for k in 1..=n {
                let t = length * k as f32 / n as f32;
                out.push((pose.0 + hx * t, pose.1 + hz * t, t));
            }
        }
        Segment::Arc { radius, angle, .. } => {
            let ang = angle.to_radians();
            let run = radius.abs() * ang;
            let n = ((run / step) as usize).max(1);
            for k in 1..=n {
                let a = ang * k as f32 / n as f32 * if *radius > 0.0 { 1.0 } else { -1.0 };
                let nh = pose.2 + a;
                out.push((
                    pose.0 + radius * (pose.2.cos() - nh.cos()),
                    pose.1 + radius * (nh.sin() - pose.2.sin()),
                    run * k as f32 / n as f32,
                ));
            }
        }
    }
}

/// The four turn-straight-turn ways from one pose to another, shortest first.
///
/// Only the CSC families: with a lap this size the RLR and LRL cases only matter when the two
/// poses are almost on top of each other, and a lap that close to its own start has finished.
fn dubins(start: (f32, f32, f32), goal: (f32, f32, f32), r: f32) -> Vec<(f32, Vec<Segment>)> {
    let mut out: Vec<(f32, Vec<Segment>)> = Vec::new();
    for s1 in [1.0f32, -1.0] {
        for s2 in [1.0f32, -1.0] {
            let rv1 = crate::trackprog::right_vector(start.2);
            let rv2 = crate::trackprog::right_vector(goal.2);
            let c1 = (start.0 + s1 * r * rv1.0, start.1 + s1 * r * rv1.1);
            let c2 = (goal.0 + s2 * r * rv2.0, goal.1 + s2 * r * rv2.1);
            let (dx, dz) = (c2.0 - c1.0, c2.1 - c1.1);
            let d = dx.hypot(dz);
            if d < 1e-6 {
                continue;
            }
            let theta = dx.atan2(dz); // app frame: the heading of the line of centres
            let (tangent_h, run) = if s1 == s2 {
                // Same handedness: the straight is parallel to the line of centres.
                (theta, d)
            } else {
                // Opposite: the straight crosses between them, and only if they are far
                // enough apart to have a common internal tangent.
                if d < 2.0 * r {
                    continue;
                }
                let alpha = (2.0 * r / d).min(1.0).acos();
                (
                    theta + if s1 > 0.0 { alpha } else { -alpha },
                    (d * d - 4.0 * r * r).max(0.0).sqrt(),
                )
            };
            let a1 = ((tangent_h - start.2) * s1).rem_euclid(std::f32::consts::TAU);
            let a2 = ((goal.2 - tangent_h) * s2).rem_euclid(std::f32::consts::TAU);
            let mut segs = Vec::new();
            if a1 > 1e-4 {
                segs.push(Segment::Arc { radius: r * s1, angle: a1.to_degrees(), rise: 0.0 });
            }
            if run > 0.5 {
                segs.push(Segment::Straight { length: run, rise: 0.0 });
            }
            if a2 > 1e-4 {
                segs.push(Segment::Arc { radius: r * s2, angle: a2.to_degrees(), rise: 0.0 });
            }
            out.push((a1 * r + run + a2 * r, segs));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Which parts of the plot the lap has been near, on a coarse grid.
///
/// The walk used to be scored by how much open ground lay ahead of it, and the most open
/// ground is always the perimeter — so it followed the boundary round and left the middle
/// empty, which from above is a giant ring. Rewarding *new ground covered* instead makes it
/// fill the plot.
struct Cover {
    cell: f32,
    n: usize,
    seen: Vec<bool>,
}

impl Cover {
    fn new(plot: f32) -> Self {
        let cell = 26.0;
        let n = (plot / cell) as usize + 1;
        Cover { cell, n, seen: vec![false; n * n] }
    }

    fn index(&self, x: f32, z: f32) -> Option<usize> {
        if x < 0.0 || z < 0.0 {
            return None;
        }
        let (c, r) = ((x / self.cell) as usize, (z / self.cell) as usize);
        (c < self.n && r < self.n).then_some(r * self.n + c)
    }

    /// How many cells this piece would visit that nothing has visited yet.
    fn fresh(&self, pts: &[(f32, f32, f32)]) -> usize {
        let mut hit: Vec<usize> = Vec::new();
        for (x, z, _) in pts {
            if let Some(i) = self.index(*x, *z) {
                if !self.seen[i] && !hit.contains(&i) {
                    hit.push(i);
                }
            }
        }
        hit.len()
    }

    /// Mark the ground, and hand back what to unmark if the move is taken back.
    fn add(&mut self, pts: &[(f32, f32, f32)]) -> Vec<usize> {
        let mut marked = Vec::new();
        for (x, z, _) in pts {
            if let Some(i) = self.index(*x, *z) {
                if !self.seen[i] {
                    self.seen[i] = true;
                    marked.push(i);
                }
            }
        }
        marked
    }

    fn undo(&mut self, marked: &[usize]) {
        for i in marked {
            self.seen[*i] = false;
        }
    }
}

/// What the lap has used, on a coarse grid, so "is this clear" is cheap.
struct Ground {
    cell: f32,
    n: usize,
    plot: f32,
    used: Vec<Vec<(f32, f32, f32)>>,
}

impl Ground {
    fn new(plot: f32) -> Self {
        let cell = 8.0;
        let n = (plot / cell) as usize + 1;
        Ground { cell, n, plot, used: vec![Vec::new(); n * n] }
    }

    fn cells(&self, x: f32, z: f32, reach: f32) -> impl Iterator<Item = usize> + '_ {
        let c0 = ((x - reach) / self.cell).max(0.0) as usize;
        let c1 = (((x + reach) / self.cell) as usize).min(self.n - 1);
        let r0 = ((z - reach) / self.cell).max(0.0) as usize;
        let r1 = (((z + reach) / self.cell) as usize).min(self.n - 1);
        let n = self.n;
        (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| r * n + c))
    }

    /// Lay ground, and hand back how much each cell grew by so it can be taken back.
    fn add(&mut self, pts: &[(f32, f32, f32)], at: f32) -> Vec<(usize, usize)> {
        let mut grew: Vec<(usize, usize)> = Vec::new();
        for (x, z, along) in pts {
            if *x < 0.0 || *z < 0.0 {
                continue;
            }
            let i = (*z / self.cell) as usize * self.n + (*x / self.cell) as usize;
            if i < self.used.len() {
                self.used[i].push((*x, *z, at + along));
                match grew.iter_mut().find(|(c, _)| *c == i) {
                    Some(e) => e.1 += 1,
                    None => grew.push((i, 1)),
                }
            }
        }
        grew
    }

    fn undo(&mut self, grew: &[(usize, usize)]) {
        for (i, k) in grew {
            let keep = self.used[*i].len() - k;
            self.used[*i].truncate(keep);
        }
    }

    /// Is anything already laid within `clear` of here? Stops at the first one.
    ///
    /// `nearest` measures, which costs a sweep of every cell inside sixty metres — 225 of
    /// them — when the walk only ever asks whether one point is too close. This looks at 25.
    ///
    /// Both bounds are ages round the lap. `ignore_after` is the last thirty metres, because a
    /// corner may come close to the run that fed it. `ignore_before` is for the way home: a
    /// lap arriving back at its gate necessarily runs up beside the straight it left on.
    fn blocked(&self, x: f32, z: f32, ignore_after: f32, ignore_before: f32, clear: f32) -> bool {
        for i in self.cells(x, z, clear) {
            for (px, pz, age) in &self.used[i] {
                if *age > ignore_after || *age < ignore_before {
                    continue;
                }
                if (px - x) * (px - x) + (pz - z) * (pz - z) < clear * clear {
                    return true;
                }
            }
        }
        false
    }

    fn nearest(&self, x: f32, z: f32, ignore_after: f32) -> f32 {
        let reach = 60.0;
        let mut best = reach;
        for i in self.cells(x, z, reach) {
            for (px, pz, age) in &self.used[i] {
                if *age > ignore_after {
                    continue;
                }
                let d = (px - x).hypot(pz - z);
                if d < best {
                    best = d;
                }
            }
        }
        best
    }

    /// How open the ground is here — used to steer towards what has not been ridden.
    fn room(&self, x: f32, z: f32, ignore_after: f32) -> f32 {
        let edge = x.min(z).min(self.plot - x).min(self.plot - z);
        self.nearest(x, z, ignore_after).min(edge)
    }
}

/// A piece of the ground between corners: a radius off the corpus, and 15 to 45 m of it.
///
/// Drawn as a length with the angle following, because a fixed angle band makes a 500 m
/// sweeper 200 m long. Indiana's segments average eighteen metres.
fn a_wander(rng: &mut Rng) -> Vec<Segment> {
    let roll = rng.range(0.0, 1.0);
    let mut acc = 0.0;
    let mut band = WANDER[WANDER.len() - 1];
    for w in WANDER {
        acc += w.2;
        if roll <= acc {
            band = w;
            break;
        }
    }
    let r = rng.range(band.0.ln(), band.1.ln()).exp();
    let run = rng.range(15.0, 45.0);
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    vec![Segment::Arc {
        radius: r * side,
        angle: (run / r).to_degrees(),
        rise: 0.0,
    }]
}

/// One corner, built the way a real one is: in on a loosening arc, round, and out.
///
/// A published corner is not one arc — Indiana's median is four and Southwick's five — which
/// is why it covers seventy metres of ground turning through 160 degrees where a lone arc of
/// the same apex radius covers thirty. Offered one arc at a time the walk could only reach the
/// corpus's tight radii with a hairpin, and a same-handed wander landing beside it read as one
/// corner of 197 degrees.
fn a_corner(rng: &mut Rng) -> Vec<Segment> {
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    if rng.chance(0.22) {
        // Not every corner is a hairpin: a fifth of the corpus's are a single sweep.
        return vec![Segment::Arc {
            radius: side * rng.range(15.0, 32.0),
            angle: rng.range(40.0, 95.0),
            rise: 0.0,
        }];
    }
    vec![
        Segment::Arc { radius: side * rng.range(26.0, 60.0), angle: rng.range(22.0, 50.0), rise: 0.0 },
        Segment::Arc { radius: side * rng.range(8.0, 15.0), angle: rng.range(55.0, 105.0), rise: 0.0 },
        Segment::Arc { radius: side * rng.range(18.0, 45.0), angle: rng.range(18.0, 45.0), rise: 0.0 },
    ]
}

/// Is this chain a corner, or the ground between two of them?
fn is_corner(chain: &[Segment]) -> bool {
    chain.len() > 1
        || matches!(chain.first(), Some(Segment::Arc { angle, .. }) if *angle >= 40.0)
}

/// The longest unbroken straight a rider meets, metres — merged the way the app merges.
///
/// Mirrors `Station::straight_runs`: consecutive straights are one straight, and so is the
/// pair either side of the finish line, because a lap that ends on a straight and begins on
/// one runs through the line without a corner in it.
fn longest_straight(segs: &[Segment], opening: f32) -> f32 {
    let mut runs = Vec::new();
    let mut run = 0.0;
    for s in segs {
        match s {
            Segment::Straight { length, .. } => run += length,
            _ => {
                if run > 0.0 {
                    runs.push(run);
                }
                run = 0.0;
            }
        }
    }
    // Through the line: whatever the lap ends on, plus the opening straight it rejoins.
    runs.push(run + opening);
    runs.into_iter().fold(0.0f32, f32::max)
}

/// One move on the lap, and what it would take to undo it.
struct Step {
    pose: (f32, f32, f32),
    laid: f32,
    n: usize,
    cands: Vec<Vec<Segment>>,
    cursor: usize,
    ground: Vec<(usize, usize)>,
    cover: Vec<usize>,
    since: f32,
    corners: u32,
    tight: f32,
}

/// Walk a lap out of the ground and dock it back onto its own start.
///
/// Each step offers the same handful of moves — a run, a gentle wander, or a whole corner
/// either way — and the one taken is whichever leaves the track in the most open ground while
/// clearing everything already laid. When the budget runs down, the shortest Dubins path back
/// to the start pose that is also clear closes the lap exactly, which is why there is no
/// return leg to route round anything.
///
/// And it takes moves back. Greedy, sixty seeds gave one lap and the other fifty-nine ran out
/// of legal moves 150 to 900 m in, because the first dead end was final. The moves live on a
/// stack, and a dead end pops one and takes the next-best instead.
fn walk(rng: &mut Rng, plot: f32, width: f32, want_m: f32) -> Option<(Vec<Segment>, Start)> {
    let clear = width + 5.0;
    // Drawn once: redrawn at every step it averages to the middle and the noise decides.
    let want_rate = rng.range(CORNERS_PER_KM.0, CORNERS_PER_KM.1) / 1000.0;
    let start = (plot * 0.5, MARGIN_M + GATE_ROOM_M, 0.0); // facing +z, up the plot
    let mut ground = Ground::new(plot);
    let mut cover = Cover::new(plot);
    let mut segs: Vec<Segment> = Vec::new();
    let mut pts = Vec::new();

    // The first stretch is the start straight, and nothing may be built on it.
    let opening = Segment::Straight { length: rng.range(90.0, 130.0), rise: 0.0 };
    samples(start, &opening, 3.0, &mut pts);
    ground.add(&pts, 0.0);
    cover.add(&pts);
    let mut pose = advance(start, &opening);
    let mut laid = seg_length(&opening);
    let opening_m = laid;
    segs.push(opening);

    let cap = (want_m * 1.25).min(LAP_MAX_M);
    let mut stack: Vec<Step> = Vec::new();
    let mut budget = 9000u32;
    let (mut since_corner, mut corners_laid, mut tight_m) = (0.0f32, 0u32, 0.0f32);
    let mut node: Option<(Vec<Vec<Segment>>, usize)> = None;

    while budget > 0 {
        if node.is_none() {
            if laid > want_m * 0.86 {
                if let Some(home) = home_from(
                    rng, &ground, plot, clear, start, pose, laid, &segs, opening_m,
                ) {
                    segs.extend(home);
                    return Some((
                        segs,
                        Start {
                            x: start.0,
                            z: start.1,
                            angle: start.2.to_degrees().rem_euclid(360.0),
                        },
                    ));
                }
            }
            let cands = if laid < cap {
                offers(rng, &ground, &cover, &segs, pose, laid, since_corner, corners_laid,
                       tight_m, want_rate)
            } else {
                Vec::new()
            };
            node = Some((cands, 0));
        }
        budget -= 1;

        // Take the best move left here that is actually clear. Ignore the last thirty metres
        // of track when checking: a corner is allowed to come close to the run that fed it.
        let (cands, cursor) = node.as_mut().unwrap();
        let mut chain: Option<Vec<Segment>> = None;
        while *cursor < cands.len() {
            let c = cands[*cursor].clone();
            *cursor += 1;
            if legal(&ground, plot, clear, pose, &c, laid - 34.0, 0.0) {
                chain = Some(c);
                break;
            }
        }

        let Some(chain) = chain else {
            // Painted in. Take the last move back and try this node's next-best instead.
            let back = stack.pop()?;
            ground.undo(&back.ground);
            cover.undo(&back.cover);
            segs.truncate(segs.len() - back.n);
            pose = back.pose;
            laid = back.laid;
            since_corner = back.since;
            corners_laid = back.corners;
            tight_m = back.tight;
            node = Some((back.cands, back.cursor));
            continue;
        };

        let (cands, cursor) = node.take().unwrap();
        let mut grew = Vec::new();
        let mut marked = Vec::new();
        let (mut p, mut at) = (pose, laid);
        for seg in &chain {
            pts.clear();
            samples(p, seg, 3.0, &mut pts);
            for (i, k) in ground.add(&pts, at) {
                match grew.iter_mut().find(|(c, _): &&mut (usize, usize)| *c == i) {
                    Some(e) => e.1 += k,
                    None => grew.push((i, k)),
                }
            }
            marked.extend(cover.add(&pts));
            at += seg_length(seg);
            p = advance(p, seg);
        }
        stack.push(Step {
            pose,
            laid,
            n: chain.len(),
            cands,
            cursor,
            ground: grew,
            cover: marked,
            since: since_corner,
            corners: corners_laid,
            tight: tight_m,
        });
        let run = chain_length(&chain);
        if is_corner(&chain) {
            corners_laid += 1;
            since_corner = 0.0;
            tight_m += chain
                .iter()
                .filter(|s| matches!(s, Segment::Arc { radius, .. } if radius.abs() < 14.0))
                .map(|s| seg_length(s))
                .sum::<f32>();
        } else {
            since_corner += run;
        }
        pose = p;
        laid += run;
        segs.extend(chain);
    }
    None
}

/// Would this chain of segments fit on ground nothing has used?
#[allow(clippy::too_many_arguments)]
fn legal(
    ground: &Ground,
    plot: f32,
    clear: f32,
    from: (f32, f32, f32),
    chain: &[Segment],
    age_cut: f32,
    skip_before: f32,
) -> bool {
    let mut p = from;
    let mut pts = Vec::new();
    for seg in chain {
        pts.clear();
        samples(p, seg, 2.5, &mut pts);
        for (x, z, _) in &pts {
            if !(MARGIN_M..=plot - MARGIN_M).contains(x)
                || !(MARGIN_M..=plot - MARGIN_M).contains(z)
            {
                return false;
            }
            if ground.blocked(*x, *z, age_cut, skip_before, clear) {
                return false;
            }
        }
        p = advance(p, seg);
    }
    true
}

/// Every move worth trying here, best first.
///
/// Scored on ground it would be the first to visit — per metre travelled, not per move,
/// because rewarding raw coverage buys it with long straights, and a lap of long runs with
/// angles between them is not a track.
#[allow(clippy::too_many_arguments)]
fn offers(
    rng: &mut Rng,
    ground: &Ground,
    cover: &Cover,
    segs: &[Segment],
    pose: (f32, f32, f32),
    laid: f32,
    since_corner: f32,
    corners_laid: u32,
    tight_m: f32,
    want_rate: f32,
) -> Vec<Vec<Segment>> {
    let mut running = 0.0;
    for prev in segs.iter().rev() {
        match prev {
            Segment::Straight { length, .. } => running += length,
            _ => break,
        }
    }
    let mut cand: Vec<Vec<Segment>> = Vec::new();
    // A straight, unless the lap is already on one long enough. Consecutive straights are
    // colinear, so the app's `straight_runs` reads a row of them as ONE straight.
    let room = STRAIGHT_CAP_M - running;
    if room > 30.0 {
        cand.push(vec![Segment::Straight { length: rng.range(30.0, room.min(75.0)), rise: 0.0 }]);
    }
    // The lap's own wander, and most of the ground between corners.
    for _ in 0..7 {
        cand.push(a_wander(rng));
    }
    // And corners, whole — but only once the lap has run far enough since the last one for the
    // two to be told apart, or a same-handed pair reads as one corner of twice the angle. The
    // corpus's p50 run between corners is 27 to 30 m.
    if since_corner >= 20.0 {
        for _ in 0..4 {
            cand.push(a_corner(rng));
        }
    }

    let behind = (corners_laid as f32) < laid * want_rate;
    let mut scored: Vec<(f32, Vec<Segment>)> = Vec::with_capacity(cand.len());
    let mut pts = Vec::new();
    for chain in cand {
        let run_m = chain_length(&chain);
        pts.clear();
        let mut p = pose;
        for seg in &chain {
            samples(p, seg, 6.0, &mut pts);
            p = advance(p, seg);
        }
        let mut score = 190.0 * cover.fresh(&pts) as f32 / run_m.max(1.0);
        let (hx, hz) = crate::trackprog::heading_vector(p.2);
        score += 0.35 * ground.room(p.0 + hx * 26.0, p.1 + hz * 26.0, laid - 34.0);
        if is_corner(&chain) {
            // Corners are the point — but eight a kilometre, not as many as will fit.
            score += if behind { 34.0 } else { -22.0 };
            let apex = chain
                .iter()
                .map(|s| match s {
                    Segment::Arc { radius, .. } => radius.abs(),
                    _ => f32::MAX,
                })
                .fold(f32::MAX, f32::min);
            if apex < 14.0 && tight_m < laid * TIGHT_SHARE {
                score += 20.0;
            }
        }
        scored.push((score, chain));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// A way back onto the start pose that lands, is clear, and breaks no rule.
#[allow(clippy::too_many_arguments)]
fn home_from(
    rng: &mut Rng,
    ground: &Ground,
    plot: f32,
    clear: f32,
    start: (f32, f32, f32),
    pose: (f32, f32, f32),
    laid: f32,
    segs: &[Segment],
    opening: f32,
) -> Option<Vec<Segment>> {
    // Several radii, not one. A Dubins path is fixed by the radius you give it, so a single
    // draw is a single shape: with the gate exemption tightened to an age, one shape closed on
    // two seeds in three and the rest walked on to the cap, which showed up as 12.6 corners a
    // kilometre. Five radii is five shapes, and the shortest that clears wins.
    let mut ways: Vec<(f32, Vec<Segment>)> = Vec::new();
    for r in [14.0f32, 18.0, 23.0, 29.0, 36.0] {
        ways.extend(dubins(pose, start, r * rng.range(0.92, 1.08)));
    }
    ways.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, home) in ways {
        // Walked, not trusted: one of the four families has a sign in it that only bites on
        // some geometries, and the symptom is a lap that misses itself by thirty metres.
        let landed = home.iter().fold(pose, |p, s| advance(p, s));
        if (landed.0 - start.0).hypot(landed.1 - start.1) > 0.25 {
            continue;
        }
        // The way home may run up beside the start straight — that is where it is going — so
        // the opening straight is not an obstacle to it, and nothing else is excused.
        //
        // This used to be a disc round the start pose, which excused every *other* piece of
        // lap that happened to pass through the disc too: `review` caught a way home running
        // 12 m from a mid-lap straight on a 15 m track. Ages are exact where a radius is not.
        if !legal(ground, plot, clear, pose, &home, laid - 34.0, opening) {
            continue;
        }
        // The way home is part of the lap and its own length counts: laps came out at 2627 and
        // 2820 m because the walk stopped at the cap and Dubins added on top of it.
        if laid + chain_length(&home) > LAP_MAX_M {
            continue;
        }
        // And no straight on the finished lap may run past the FFM's 125 m. Checked on the
        // whole lap: consecutive straights are colinear so `straight_runs` reads a row of them
        // as one, and a Dubins path's straight sits in its middle.
        let mut all: Vec<Segment> = segs.to_vec();
        all.extend(home.iter().copied());
        if longest_straight(&all, opening) > STRAIGHT_CAP_M {
            continue;
        }
        if home
            .iter()
            .all(|s| !matches!(s, Segment::Arc { angle, .. } if *angle >= 200.0))
        {
            return Some(home);
        }
    }
    None
}

/// Where each segment sits round the lap, and how tight it is there.
fn spans(segs: &[Segment]) -> (f32, Vec<(f32, f32, Option<f32>)>) {
    let mut at = 0.0;
    let mut out = Vec::with_capacity(segs.len());
    for s in segs {
        let run = match s {
            Segment::Straight { length, .. } => *length,
            Segment::Arc { radius, angle, .. } => radius.abs() * angle.to_radians(),
        };
        out.push((
            at,
            at + run,
            match s {
                Segment::Straight { .. } => None,
                Segment::Arc { radius, .. } => Some(radius.abs()),
            },
        ));
        at += run;
    }
    (at, out)
}

/// How much run a jump's own faces take, metres — through the app's own arithmetic.
///
/// `scripts/track-walk.py` mirrors this by hand and nothing enforces the mirror, which is how
/// it came to hold a 27 degree face angle after `JUMP_FACE_DEG` moved to 38. Here it is the
/// same call the builder makes.
fn faces(height: f32) -> f32 {
    lip_run(height) + landing_run(height)
}

fn lip_run(height: f32) -> f32 {
    crate::trackprog::face_run(
        height,
        crate::trackprog::JUMP_FACE_DEG,
        crate::trackprog::JUMP_FACE_MIN_M,
    )
}

fn landing_run(height: f32) -> f32 {
    crate::trackprog::face_run(
        height,
        crate::trackprog::JUMP_LANDING_DEG,
        crate::trackprog::JUMP_LANDING_MIN_M,
    )
}

/// Jumps, placed by distance round the lap rather than by which segment they land on.
///
/// Published tracks put them everywhere: Indiana is 109 arcs to 11 straights and still carries
/// between twelve and forty-five lips per kilometre, so most of its jumps are in or beside
/// corners. Placing them on straights only leaves a lap made of corners — which is what a lap
/// is — with almost nothing built on it: five per kilometre, measured.
fn features(rng: &mut Rng, segs: &[Segment]) -> Vec<Feature> {
    let (total, spans) = spans(segs);
    let tight = |at: f32, length: f32| {
        spans
            .iter()
            .any(|(a, b, r)| r.is_some_and(|r| r < TIGHT_M) && *b > at && *a < at + length)
    };
    let mut out = Vec::new();
    // The first stretch is where the gate row goes, and forty riders arrive at the first jump
    // in a pack. Leave it bare.
    let mut pos = START_CLEAR_M;
    while pos < total - 40.0 {
        let room = total - 20.0 - pos;
        if tight(pos, 34.0) {
            pos += 12.0;
            continue;
        }
        // No whoops. Asked for outright, after riding a lap with them: they are the one thing
        // on a track that has to be built right or not at all, and ours are not.
        let pick = rng.range(0.0, 1.0);
        let length;
        if pick < 0.36 && room > 30.0 {
            // A table's size is its *deck*, with the faces added on. The faces are set by the
            // published lip and landing angles and come to thirty-odd metres on their own, so
            // stating a 40 m table asks for a 6 m top and gets a long rounded hill.
            //
            // Capped at three metres: Motorcycling Australia and Motorcycling New Zealand both
            // write "jumps must not exceed 3m in height", and `corpus::FEATURE_HEIGHT_M` holds
            // a program to it. This used to draw up to 3.4 and every table was outside it.
            let height = rng.range(2.4, 3.0);
            length = (rng.range(16.0, 27.0) + faces(height)).min(room);
            out.push(Feature::Tabletop { at: pos, length, height });
        } else if pick < 0.55 && room > 30.0 {
            // A table is not always flat end to end. A whale tail rises, dips over its middle
            // and rises again before the landing — two crests a rider can either double or
            // roll — which is a shape a tabletop's three numbers cannot describe.
            //
            // Drawn in metres and normalised afterwards, so the take-off gets the same run a
            // tabletop of this height gets. Drawn as fractions it had 3.6 m of lip in 8.5 m of
            // ground, and from the seat that is a wall.
            let h = rng.range(2.4, 3.0);
            let dip = rng.range(0.30, 0.40);
            let (up, down) = (lip_run(h), landing_run(h));
            let near = up + 4.0 + down * 0.55;
            let marks = [
                (0.0, 0.0),
                (up, h),
                (up + 4.0, h),
                (near, h * dip),
                (near + 11.0, h * 0.66),
                (near + 11.0 + down * 0.7, h * 0.16),
                (near + 11.0 + down, 0.0),
            ];
            let span = marks[marks.len() - 1].0;
            length = span.min(room);
            let scale = length / span;
            out.push(Feature::Custom {
                at: pos,
                length,
                shape: marks
                    .iter()
                    .map(|(m, v)| crate::trackprog::ShapePoint { u: m / span, h: v * scale })
                    .collect(),
            });
        } else if pick < 0.82 && room > 24.0 {
            // A climb rather than a wall with a ramp on it.
            length = rng.range(34.0, 48.0).min(room);
            out.push(Feature::StepUp { at: pos, length, height: rng.range(1.2, 2.0) });
        } else {
            length = rng.range(10.0, 16.0).min(room);
            if length < 8.0 {
                break;
            }
            out.push(Feature::Roller { at: pos, length, height: rng.range(0.7, 1.2) });
        }
        pos += length + rng.range(6.0, 15.0);
    }
    out
}

/// One lap, from one number. Not checked — see [`search`] for that.
pub fn draw(seed: u64) -> Option<TrackProgram> {
    let mut rng = Rng::new(seed);
    // Ridden and called "little skinny": ten to thirteen and a half metres is the bottom of
    // what the corpus allows, and a national is wider than that.
    let width = rng.range(14.5, 18.0);
    // And longer: 1451 m rode as "overall small" and 2400 m "a bit too big". Indiana is 2138.
    let mut grown = None;
    for _ in 0..6 {
        let want = rng.range(2000.0, 2350.0);
        grown = walk(&mut rng, PLOT_M, width, want);
        if grown.is_some() {
            break;
        }
    }
    let (segments, start) = grown?;
    let features = features(&mut rng, &segments);
    let surface = match rng.int(0, 9) {
        0..=6 => Surface::Soil,
        7..=8 => Surface::Sand,
        _ => Surface::Grass,
    };
    Some(TrackProgram {
        name: NAMES[(seed % NAMES.len() as u64) as usize].to_string(),
        author: "MXB App".into(),
        location: PLACES[((seed / 7) % PLACES.len() as u64) as usize].to_string(),
        width,
        blend: crate::trackprog::default_blend(),
        terrain: Terrain {
            size_x: PLOT_M,
            size_z: PLOT_M,
            samples: 2049,
            scale: if rng.chance(0.5) { 63.0 } else { 70.0 },
            surface,
            wear: crate::trackprog::default_wear(),
            // Gently rolling, and no more. A lap is benched into whatever it crosses, so
            // ground with twenty metres of landform in it puts the track in a trench with the
            // banners along the rim of the cut. A motocross venue is a field with shape in it.
            relief: Relief {
                amplitude: rng.range(4.2, 7.5),
                wavelength: rng.range(320.0, 480.0),
                seed: (seed % 9973) as u32,
                texture: 0.085,
                tilt: rng.range(6.0, 16.0),
                tilt_angle: rng.range(0.0, 359.0),
                landforms: rng.int(2, 4) as u32,
                landform_height: rng.range(2.0, 5.0),
            },
        },
        start,
        segments,
        features,
        elevation: Vec::new(),
    })
}

/// What a lap came out measuring, once it was built.
#[derive(Debug, Clone)]
pub struct Measured {
    pub seed: u64,
    pub program: TrackProgram,
    /// Anything [`crate::trackllm::review`] had to say. Empty is the point.
    pub review: Vec<String>,
    /// And what the ground itself measures — see [`ground_notes`].
    pub ground: Vec<String>,
}

/// Measure the ground a lap actually makes, against what a published one makes.
///
/// The program can be right and the ground still wrong: a lap whose corners are all sweepers
/// wears no grooves, and one whose jumps sit where the ground already falls away stands them
/// at faces nobody can land. Neither shows up in the program — only in the terrain it
/// synthesises to — which is why this is worth the two seconds it costs.
pub fn ground_notes(prog: &TrackProgram) -> Vec<String> {
    let mut notes = Vec::new();
    let Ok(syn) = crate::tracksynth::synthesise(prog) else {
        notes.push("the lap doesn't synthesise".into());
        return notes;
    };
    let grid = crate::trackstats::Grid {
        w: syn.gw,
        h: syn.gh,
        size_x: prog.terrain.size_x,
        size_z: prog.terrain.size_z,
        v: syn.heights.clone(),
    };
    let stations: Vec<(f32, f32, f32)> =
        syn.stations.iter().map(|st| (st.x, st.z, st.heading)).collect();
    if let Some(r) = crate::trackstats::rut_shape(&stations, crate::tracksynth::STATION_STEP, &grid)
    {
        // Indiana, measured: 2.2 grooves at 2.67 m, floor 0.90 m, chatter 0.007 m.
        if r.grooves < GROOVES.0 || r.grooves > GROOVES.1 {
            notes.push(format!(
                "the ground wears {:.1} grooves a section; a published track wears {}–{}",
                r.grooves, GROOVES.0, GROOVES.1
            ));
        }
        if r.chatter_m > CHATTER_M {
            notes.push(format!(
                "the surface is {:.3} m rough at the scale a wheel feels; a published track \
                 is under {CHATTER_M:.3}",
                r.chatter_m
            ));
        }
        if r.anisotropy < ANISOTROPY {
            notes.push(format!(
                "the ground is {:.2} times rougher across than along; ridden ground is at \
                 least {ANISOTROPY:.2} — under that it is shaken rather than driven on",
                r.anisotropy
            ));
        }
    }
    // Berms: the outside of a corner has to stand higher than its inside, or nothing on the
    // lap is bankable.
    let mut stood = 0usize;
    let mut corners = 0usize;
    for (i, st) in syn.stations.iter().enumerate().step_by(8) {
        let k = st.curvature;
        if k.abs() * FULL_LEAN < 0.6 {
            continue;
        }
        corners += 1;
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let half = prog.width * 0.5;
        let at = |t: f32| {
            let (x, z) = (st.x + rx * t, st.z + rz * t);
            crate::trackstats::Grid::at(&grid, x, z)
        };
        let outside = -k.signum() * (half - 0.5);
        let inside = k.signum() * (half - 0.5);
        if at(outside) > at(inside) + BERM_STAND_M {
            stood += 1;
        }
        let _ = i;
    }
    if corners > 0 && (stood as f32 / corners as f32) < BERM_SHARE {
        notes.push(format!(
            "only {stood} of {corners} corners grew a berm; a lap wants {:.0}% of them",
            BERM_SHARE * 100.0
        ));
    }
    notes
}

/// What the ground has to measure before a lap is worth building. All off published tracks —
/// see `trackstats::rut_shape` and the numbers in `tracksynth`.
const GROOVES: (f32, f32) = (1.4, 3.4);
const CHATTER_M: f32 = 0.012;
const ANISOTROPY: f32 = 1.35;
const BERM_STAND_M: f32 = 0.12;
const BERM_SHARE: f32 = 0.55;
/// The radius at which a corner is leaned on hard enough to bank — see `tracksynth`.
const FULL_LEAN: f32 = 22.0;

/// Draw laps until one passes, and say what the rejected ones got wrong.
///
/// Both halves have to hold: the program has to read like a published track's, and the ground
/// it makes has to measure like one. `ground` costs a synthesis apiece, so it is only asked
/// about laps that already pass review.
pub fn search(from: u64, tries: u32) -> Result<Measured, Vec<Measured>> {
    let mut rejected = Vec::new();
    for i in 0..tries as u64 {
        let seed = from + i;
        // A seed the walk paints itself in on is not a lap; it is the next seed's turn.
        let Some(mut program) = draw(seed) else { continue };
        crate::trackllm::repair_for_tests(&mut program);
        let review = crate::trackllm::review(&program);
        let mut notes = review.problems.clone();
        notes.extend(review.notes.clone());
        if !notes.is_empty() {
            rejected.push(Measured { seed, program, review: notes, ground: Vec::new() });
            continue;
        }
        let ground = ground_notes(&program);
        let out = Measured { seed, program, review: Vec::new(), ground };
        if out.ground.is_empty() {
            return Ok(out);
        }
        rejected.push(out);
    }
    Err(rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seed that draws, for the tests that need one. Roughly nine in ten do.
    fn drawn(seed: u64) -> TrackProgram {
        (0..24)
            .find_map(|i| draw(seed + i))
            .unwrap_or_else(|| panic!("no seed near {seed} drew a lap"))
    }

    /// Corners, grouped the way `scripts/track-survey.py` groups a published one: same-handed
    /// turning under a 300 m radius, with under ten metres of run let into it.
    ///
    /// Both halves of that matter. Over 300 m is a sweeper — Motorcycling Australia's own
    /// definition of a curve is a direction change over 15 degrees with a radius under 300 —
    /// and counting arcs instead of corners counts a published corner four or five times over.
    /// Grouping any same-signed arc, which is what this used to do, reported Indiana as
    /// carrying 615-degree corners.
    fn corners(segs: &[Segment]) -> Vec<(f32, f32, f32, usize)> {
        const SWEEP: f32 = 300.0;
        const JOIN: f32 = 10.0;
        let mut out: Vec<(f32, f32, f32, usize)> = Vec::new(); // angle, ground, apex, arcs
        let (mut sign, mut gap) = (0.0f32, f32::MAX);
        for s in segs {
            let turning = matches!(s, Segment::Arc { radius, .. } if radius.abs() <= SWEEP);
            match s {
                Segment::Arc { radius, angle, .. } if turning => {
                    let run = radius.abs() * angle.to_radians();
                    if radius.signum() == sign && gap <= JOIN {
                        let c = out.last_mut().unwrap();
                        c.0 += angle;
                        c.1 += run + gap;
                        c.2 = c.2.min(radius.abs());
                        c.3 += 1;
                    } else {
                        out.push((*angle, run, radius.abs(), 1));
                    }
                    sign = radius.signum();
                    gap = 0.0;
                }
                _ => gap += seg_length(s),
            }
        }
        out.retain(|c| c.0 >= 25.0);
        out
    }

    fn median(mut v: Vec<f32>) -> f32 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    }

    /// The two properties the shape exists to guarantee, over a spread of seeds.
    #[test]
    fn every_lap_closes_and_none_crosses_itself() {
        for seed in [1u64, 7, 42, 777, 1234, 31337] {
            let mut p = drawn(seed);
            crate::trackllm::repair_for_tests(&mut p);
            // The walk docks onto its own start pose with a Dubins path it has walked, so
            // this is exact rather than nearly: it used to be allowed 20 m.
            assert!(
                p.closure_error() < 2.0,
                "seed {seed}: the lap misses itself by {:.1} m",
                p.closure_error()
            );
            let crossing = crate::trackllm::review(&p)
                .problems
                .into_iter()
                .find(|n| n.contains("crosses itself"));
            assert!(crossing.is_none(), "seed {seed}: {}", crossing.unwrap_or_default());
        }
    }

    /// And the shape of it, against what Indiana and Southwick measure.
    ///
    /// These are the numbers the walk exists to hit, so they are asserted rather than printed.
    /// The bands are the corpus's own, from `scripts/track-survey.py` over the eighteen laps
    /// in `~/Projects/pkz`; the medians of Indiana and Southwick sit in the middle of each.
    #[test]
    fn a_drawn_lap_is_shaped_like_a_published_one() {
        for seed in [1u64, 7, 42, 777, 1234, 31337] {
            let mut p = drawn(seed);
            crate::trackllm::repair_for_tests(&mut p);
            let lap = p.lap_length();
            let turning: f32 = p
                .segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Arc { radius, angle, .. } if radius.abs() <= 300.0 => {
                        Some(radius.abs() * angle.to_radians())
                    }
                    _ => None,
                })
                .sum();
            let gross: f32 = p
                .segments
                .iter()
                .map(|s| match s {
                    Segment::Arc { angle, .. } => angle.abs(),
                    _ => 0.0,
                })
                .sum();
            let cs = corners(&p.segments);
            let per_km = cs.len() as f32 / lap * 1000.0;
            let angle = median(cs.iter().map(|c| c.0).collect());
            let apex = median(cs.iter().map(|c| c.2).collect());
            let ground = median(cs.iter().map(|c| c.1).collect());
            let jumps = p.features.len() as f32 / lap * 1000.0;
            let tallest = p
                .features
                .iter()
                .map(|f| f.height())
                .fold(0.0f32, f32::max);

            let say = format!(
                "seed {seed}: {lap:.0} m, {} corners ({per_km:.1}/km), {angle:.0}° each, \
                 apex {apex:.1} m, {ground:.0} m of ground, {:.2} turning, {gross:.0}° gross, \
                 {jumps:.0} jumps/km, tallest {tallest:.1} m",
                cs.len(),
                turning / lap
            );
            // Indiana 2170, Southwick 2217; the corpus runs 1065-3055.
            assert!((1700.0..2600.0).contains(&lap), "{say}");
            // Indiana 7.4, Southwick 8.1; the corpus 7.4-10.3.
            assert!((6.5..12.0).contains(&per_km), "{say}");
            // Indiana 159, Southwick 166; the corpus p50 90-166.
            assert!((85.0..185.0).contains(&angle), "{say}");
            // Indiana 10.4, Southwick 11.9; the corpus p50 7.1-18.5.
            assert!((7.0..20.0).contains(&apex), "{say}");
            // Indiana 68, Southwick 74; the corpus p50 14-82.
            assert!((40.0..95.0).contains(&ground), "{say}");
            // Indiana 0.63, Southwick 0.72.
            assert!((0.55..0.88).contains(&(turning / lap)), "{say}");
            // `trackllm::corpus::LIPS_PER_KM`, and the height ceiling two federations write.
            assert!((12.0..45.0).contains(&jumps), "{say}");
            assert!(tallest <= 3.0 + 1e-3, "{say}");
        }
    }

    /// What `random_track_program` hands the studio: a value with every defaulted field
    /// filled in, that parses straight back into the type.
    ///
    /// This is the trap [`crate::trackprog::BLANK`] documents from the other side. A track
    /// program has fields with serde defaults — `blend`, `elevation`, the ground's `wear` —
    /// and a value missing any of them puts an `undefined` into a number input the moment the
    /// studio loads it. Serialising the *type* is what fills them; nothing else does.
    #[test]
    fn a_random_track_arrives_whole() {
        let p = drawn(4242);
        let v = serde_json::to_value(&p).expect("serialises");
        for key in ["name", "author", "location", "width", "terrain", "start", "segments",
                    "features", "blend", "elevation"] {
            assert!(v.get(key).is_some(), "the studio is handed no `{key}`");
        }
        for key in ["sizeX", "sizeZ", "samples", "scale", "surface", "wear", "relief"] {
            assert!(v["terrain"].get(key).is_some(), "no `terrain.{key}`");
        }
        let back: TrackProgram = serde_json::from_value(v).expect("and parses back");
        assert_eq!(back.name, p.name);
        assert_eq!(back.segments.len(), p.segments.len());
        back.check().expect("and it is a track");
    }

    /// The same number gives the same track, on any machine. Seeds are how a track is named
    /// and found again, so this is not a detail.
    #[test]
    fn a_seed_draws_the_same_lap_every_time() {
        let a = drawn(99);
        let b = drawn(99);
        assert_eq!(a.name, b.name);
        assert_eq!(a.segments.len(), b.segments.len());
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    /// Most seeds draw. The walk backtracks, and this is what that bought: greedy, one seed in
    /// sixty produced a lap and the rest ran out of legal moves a fifth of the way round.
    #[test]
    fn most_seeds_draw_a_lap() {
        let drew = (200u64..240).filter(|s| draw(*s).is_some()).count();
        // Measured at 27-30 of 40. `random_track_program` searches forward from its seed, so
        // what this guards is a regression to the greedy walk's one in sixty.
        assert!(drew >= 24, "only {drew} of 40 seeds drew a lap");
    }

    /// The whole point: a seed range yields a lap that passes both halves.
    ///
    /// Slow — every candidate that passes review is synthesised and measured — so it runs on
    /// demand rather than in the suite.
    ///
    /// ```text
    /// cargo test --bin mxb-app -- --ignored --nocapture a_search_finds_a_lap_that_measures_up
    /// ```
    #[test]
    #[ignore = "synthesises every candidate — slow"]
    fn a_search_finds_a_lap_that_measures_up() {
        match search(1000, 12) {
            Ok(m) => {
                println!(
                    "seed {}: {} — {:.0} m, {} segments, {} jumps",
                    m.seed,
                    m.program.name,
                    m.program.lap_length(),
                    m.program.segments.len(),
                    m.program.features.len()
                );
            }
            Err(tried) => {
                for m in &tried {
                    println!(
                        "  seed {} rejected: {}",
                        m.seed,
                        m.review.iter().chain(m.ground.iter()).cloned().collect::<Vec<_>>().join("; ")
                    );
                }
                panic!("no lap out of {} passed", tried.len());
            }
        }
    }
}

#[cfg(test)]
mod corner_shape_tests {
    use super::*;

    /// Write drawn programmes where `scripts/track-survey.py` can measure them, which is how
    /// the Rust walk is checked against the Python one it was ported from.
    ///
    /// ```text
    /// LAYOUT_OUT=/tmp/laps LAYOUT_SEEDS=40 \
    ///   cargo test --bin mxb-app -- --ignored --nocapture dump_programs
    /// ```
    #[test]
    #[ignore]
    fn dump_programs() {
        let Ok(dir) = std::env::var("LAYOUT_OUT") else {
            println!("set LAYOUT_OUT");
            return;
        };
        let n: u64 = std::env::var("LAYOUT_SEEDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let from: u64 = std::env::var("LAYOUT_FROM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        std::fs::create_dir_all(&dir).expect("create");
        let mut drew = 0;
        for seed in from..from + n {
            let Some(p) = draw(seed) else {
                println!("seed {seed}: painted in");
                continue;
            };
            drew += 1;
            let out = std::path::Path::new(&dir).join(format!("seed{seed}.json"));
            std::fs::write(&out, serde_json::to_vec_pretty(&p).unwrap()).expect("write");
            println!(
                "seed {seed}: {} — {:.0} m, {} segments, {} features",
                p.name,
                p.lap_length(),
                p.segments.len(),
                p.features.len()
            );
        }
        println!("{drew} of {n} seeds drew a lap; programmes in {dir}");
    }

    /// Print a drawn lap as a `trackprog::EXAMPLE` literal — this is how the base track is
    /// made, so it can be remade when the walk changes rather than being hand-kept.
    ///
    /// ```text
    /// BASE_SEED=24 cargo test --bin mxb-app -- --ignored --nocapture emit_base_track
    /// ```
    #[test]
    #[ignore]
    fn emit_base_track() {
        let seed: u64 = std::env::var("BASE_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);
        let mut p = draw(seed).expect("that seed paints itself in");
        p.name = "Corpus National".into();
        p.location = "Generated".into();
        // Left exactly as the repair pass leaves it, so the base track loads with nothing to
        // say about it: begun on a straight, on ground sized and centred for where the lap
        // ends up rather than where it started.
        crate::trackllm::repair_for_tests(&mut p);
        let n = |v: f32| {
            let t = format!("{v:.4}");
            let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
            if t == "-0" { "0".into() } else { t }
        };
        let mut out = String::from("pub const EXAMPLE: &str = r#\"{\n");
        out += &format!("      \"name\": \"{}\",\n", p.name);
        out += &format!("      \"author\": \"{}\",\n", p.author);
        out += &format!("      \"location\": \"{}\",\n", p.location);
        out += &format!("      \"width\": {},\n", n(p.width));
        out += "      \"terrain\": {\n";
        out += &format!(
            "        \"sizeX\": {}, \"sizeZ\": {}, \"samples\": {}, \"scale\": {}, \"surface\": \"{}\",\n",
            n(p.terrain.size_x), n(p.terrain.size_z), p.terrain.samples, n(p.terrain.scale),
            serde_json::to_value(p.terrain.surface).unwrap().as_str().unwrap()
        );
        let r = &p.terrain.relief;
        out += &format!(
            "        \"relief\": {{ \"amplitude\": {}, \"wavelength\": {}, \"seed\": {}, \"texture\": {}, \"tilt\": {}, \"tiltAngle\": {}, \"landforms\": {}, \"landformHeight\": {} }}\n",
            n(r.amplitude), n(r.wavelength), r.seed, n(r.texture), n(r.tilt),
            n(r.tilt_angle), r.landforms, n(r.landform_height)
        );
        out += "      },\n";
        out += &format!(
            "      \"start\": {{ \"x\": {}, \"z\": {}, \"angle\": {} }},\n",
            n(p.start.x), n(p.start.z), n(p.start.angle)
        );
        out += "      \"segments\": [\n";
        let segs: Vec<String> = p
            .segments
            .iter()
            .map(|s| match s {
                Segment::Straight { length, .. } => {
                    format!("        {{ \"kind\": \"straight\", \"length\": {} }}", n(*length))
                }
                Segment::Arc { radius, angle, .. } => format!(
                    "        {{ \"kind\": \"arc\", \"radius\": {}, \"angle\": {} }}",
                    n(*radius),
                    n(*angle)
                ),
            })
            .collect();
        out += &segs.join(",\n");
        out += "\n      ],\n      \"features\": [\n";
        let feats: Vec<String> = p
            .features
            .iter()
            .map(|f| {
                let v = serde_json::to_value(f).unwrap();
                let o = v.as_object().unwrap();
                let body: Vec<String> = o
                    .iter()
                    .map(|(k, val)| {
                        let val = match (val.as_f64(), val.as_array()) {
                            (Some(x), _) => n(x as f32),
                            (_, Some(pts)) => {
                                let marks: Vec<String> = pts
                                    .iter()
                                    .map(|m| {
                                        let m = m.as_object().unwrap();
                                        format!(
                                            "{{ \"u\": {}, \"h\": {} }}",
                                            n(m["u"].as_f64().unwrap() as f32),
                                            n(m["h"].as_f64().unwrap() as f32)
                                        )
                                    })
                                    .collect();
                                format!("[{}]", marks.join(", "))
                            }
                            _ => serde_json::to_string(val).unwrap(),
                        };
                        format!("\"{k}\": {val}")
                    })
                    .collect();
                format!("        {{ {} }}", body.join(", "))
            })
            .collect();
        out += &feats.join(",\n");
        out += "\n      ]\n    }\"#;\n";
        println!("{out}");

        let review = crate::trackllm::review(&p);
        println!(
            "\n// seed {seed}: {:.0} m, {} segments, {} features, closes to {:.2} m",
            p.lap_length(),
            p.segments.len(),
            p.features.len(),
            p.closure_error()
        );
        for note in review.problems.iter().chain(review.notes.iter()) {
            println!("// REVIEW: {note}");
        }
        if review.problems.is_empty() && review.notes.is_empty() {
            println!("// review: nothing to say");
        }
    }

    /// Write a seed's `.trh` where the comparison scripts can read it.
    /// `TRH_OUT=/path/dir cargo test -- --ignored --nocapture dump_trh`
    #[test]
    #[ignore]
    fn dump_trh() {
        let Ok(dir) = std::env::var("TRH_OUT") else {
            println!("set TRH_OUT");
            return;
        };
        let seed: u64 = std::env::var("TRH_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(16);
        let mut p = draw(seed).expect("that seed paints itself in");
        crate::trackllm::repair_for_tests(&mut p);
        let syn = crate::tracksynth::synthesise(&p).expect("synthesise");
        let bytes = crate::tracksynth::trh(&p, &syn, true);
        let out = std::path::Path::new(&dir).join(format!("seed{seed}.trh"));
        std::fs::write(&out, &bytes).expect("write");
        println!("wrote {} ({} bytes) name={} lap={:.0} m", out.display(), bytes.len(), p.name, p.lap_length());
    }
}
