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

/// The plot every generated track is laid out on, and its middle.
const PLOT_M: f32 = 620.0;
const CENTRE: f32 = PLOT_M / 2.0;

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

/// One turn of the outline, once it has been rounded off.
struct Corner {
    /// The arcs the turn is made of, widest at each end and tightest in the middle.
    /// `(signed radius, degrees)`, positive turning right, matching [`Segment::Arc`].
    arcs: Vec<(f32, f32)>,
    /// How much of each edge beside it the rounding eats.
    tangent: f32,
}

/// The outline: `n` points round the centre, at a radius that wobbles with the angle.
///
/// Wobbled hard on purpose. A gentle loop turns 360° in total and a published lap turns 1400
/// to 3600; the difference is corners that go the other way, and those only exist where the
/// radius swings far enough in and out to bend the loop back on itself.
fn outline(rng: &mut Rng, n: usize) -> Vec<(f32, f32)> {
    let base = rng.range(185.0, 225.0);
    let waves: [(f32, f32, f32); 3] = [
        (rng.int(3, 6) as f32, rng.range(0.26, 0.40), rng.range(0.0, std::f32::consts::TAU)),
        (rng.int(6, 10) as f32, rng.range(0.16, 0.28), rng.range(0.0, std::f32::consts::TAU)),
        (rng.int(9, 13) as f32, rng.range(0.06, 0.14), rng.range(0.0, std::f32::consts::TAU)),
    ];
    let step = std::f32::consts::TAU / n as f32;
    let mut radii: Vec<f32> = (0..n)
        .map(|i| {
            let th = step * i as f32;
            let r: f32 = base
                * (1.0 + waves.iter().map(|(k, a, p)| a * (k * th + p).sin()).sum::<f32>());
            r.max(60.0)
        })
        .collect();
    // How fast the loop may pull in and out.
    //
    // Star-shaped keeps the *centreline* from crossing, and that is not the same as a track
    // not touching itself: a lap ten metres wide whose radius drops forty metres between two
    // vertices folds back over its own ground, and the reviewer says so. The radius may not
    // move by more than about the distance the loop travels in the same step, which is what
    // holds the turn at each vertex under a fold.
    for _ in 0..4 {
        for i in 0..n {
            let j = (i + 1) % n;
            let allow = SLEW * radii[i] * step;
            let d = radii[j] - radii[i];
            if d.abs() > allow {
                let half = (d.abs() - allow) * 0.5 * d.signum();
                radii[i] += half;
                radii[j] -= half;
            }
        }
    }
    (0..n)
        .map(|i| {
            let th = step * i as f32;
            (CENTRE + radii[i] * th.cos(), CENTRE + radii[i] * th.sin())
        })
        .collect()
}

/// How far the loop's radius may move from one vertex to the next, as a multiple of how far
/// it travels round in the same step. Above about 1.2 the lap starts folding onto itself.
const SLEW: f32 = 0.85;

/// How much of an edge the two corners on it may take between them. The remainder is the
/// straight, and a published lap has almost none: Indiana's whole lap carries one, of 62 m.
const EDGE_FILL: f32 = 0.985;

/// The most a merged run of same-way vertices may turn before it is left as two corners.
/// Past about this the two edges either side run nearly parallel and their crossing point —
/// the virtual apex the run is filleted about — shoots off to infinity.
const MERGE_LIMIT_DEG: f32 = 172.0;

/// Collapse runs of consecutive same-way vertices into one.
///
/// The outline turns a little at every vertex and leaves a straight on every edge, so a lap
/// approaches a corner *polygonally* — `arc R44, straight 31 m, arc R44, straight 29 m, arc
/// R22` was one real approach, all of it turning the same way. Curvature snapping between
/// zero and 1/44 every twenty metres is what made the racing line visibly weave, and capping
/// each corner at one vertex is why nothing we drew turned more than 89° when Indiana's
/// corners run to 313°.
///
/// A run of same-way vertices is one corner. Its two outer edges, extended, cross at a
/// virtual apex; putting that point in place of the whole run gives a polygon that is still
/// closed — the edges either side are the ones it already had — and filleting *it* gives one
/// long chain where there were three corners and two straights.
fn merge_runs(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 6 {
        return pts.to_vec();
    }
    let turn = |i: usize| -> f32 {
        let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
        let v1 = (b.0 - a.0, b.1 - a.1);
        let v2 = (c.0 - b.0, c.1 - b.1);
        (v1.0 * v2.1 - v1.1 * v2.0).atan2(v1.0 * v2.0 + v1.1 * v2.1)
    };
    // Start the walk at a vertex that turns the other way, so no run is split across the seam.
    let start = (0..n)
        .find(|i| turn(*i).signum() != turn((*i + 1) % n).signum())
        .map_or(0, |i| (i + 1) % n);

    let mut out: Vec<(f32, f32)> = Vec::with_capacity(n);
    let mut i = 0usize;
    while i < n {
        let a = (start + i) % n;
        let sign = turn(a).signum();
        let mut deg = turn(a).abs().to_degrees();
        let mut len = 1usize;
        while i + len < n {
            let b = (start + i + len) % n;
            let t = turn(b);
            if t.signum() != sign || deg + t.abs().to_degrees() > MERGE_LIMIT_DEG {
                break;
            }
            deg += t.abs().to_degrees();
            len += 1;
        }
        if len == 1 {
            out.push(pts[a]);
            i += 1;
            continue;
        }
        let last = (start + i + len - 1) % n;
        let a0 = pts[(a + n - 1) % n];
        let d1 = (pts[a].0 - a0.0, pts[a].1 - a0.1);
        let b0 = pts[last];
        let d2 = (pts[(last + 1) % n].0 - b0.0, pts[(last + 1) % n].1 - b0.1);
        let cross = d1.0 * d2.1 - d1.1 * d2.0;
        let span = (b0.0 - pts[a].0).hypot(b0.1 - pts[a].1).max(1.0);
        let ok = if cross.abs() < 1e-4 {
            None
        } else {
            let w = (b0.0 - a0.0, b0.1 - a0.1);
            let t = (w.0 * d2.1 - w.1 * d2.0) / cross;
            let v = (a0.0 + t * d1.0, a0.1 + t * d1.1);
            // The apex has to be near the run it stands for, or the merge bends the lap into
            // somewhere the outline never went.
            let mid = ((pts[a].0 + b0.0) / 2.0, (pts[a].1 + b0.1) / 2.0);
            if t > 1.0 && (v.0 - mid.0).hypot(v.1 - mid.1) < 1.6 * span {
                Some(v)
            } else {
                None
            }
        };
        match ok {
            Some(v) => {
                out.push(v);
                i += len;
            }
            None => {
                out.push(pts[a]);
                i += 1;
            }
        }
    }
    out
}

/// How many arcs a corner of `deg` is made of, and how much wider than the apex each one runs.
///
/// A published corner is not one arc. Indiana's are three to nineteen, and the radius *inside
/// a single corner* spans an order of magnitude — 6.4 m at the apex of its first hairpin and
/// 72 m on the way in. That is what a rider feels as a corner that tightens and releases, and
/// one arc cannot have it: filleting each vertex with a single radius is what left our lap
/// reading as straights joined by corners, at 0.49 arcs to Indiana's 0.99.
///
/// Every arc is the same *length*, not the same turn. That is the part of the signature that
/// matters: Indiana's arcs run 9.5 to 27 m almost regardless of radius (p50 15.8 m), so the
/// wide ones on the way in are long sweeps that turn only a degree or two, and nearly all of
/// the corner's heading change happens on the short tight arc at the apex. Turning each arc
/// equally instead makes the entry arcs enormous and the apex no tighter than the rest.
///
/// Equal length means turn share goes as `1/radius`, which is all this has to say.
fn chain_shape(deg: f32, rng: &mut Rng) -> Vec<(f32, f32)> {
    let k = ((deg / rng.range(13.0, 19.0)).round() as i32).clamp(1, 9) as usize;
    if k <= 1 {
        return vec![(1.0, 1.0)];
    }
    // Indiana's radius spans 56x inside the median corner — apex 11 m, entry 70 to 200.
    let spread = rng.range(8.0, 28.0);
    let bias = rng.range(1.6, 2.6);
    let mid = (k - 1) as f32 / 2.0;
    let mults: Vec<f32> = (0..k)
        .map(|i| {
            let u = (i as f32 - mid).abs() / mid.max(1e-3);
            1.0 + spread * u.powf(bias)
        })
        .collect();
    let total: f32 = mults.iter().map(|m| 1.0 / m).sum();
    mults.iter().map(|m| (*m, (1.0 / m) / total)).collect()
}

/// The tangent length a chain of unit apex radius needs, for a corner turning `deg`.
///
/// The chain is symmetric, so it leaves the corner tangent to both edges the same distance
/// from the vertex and drops into the single fillet's place exactly — the polygon still
/// closes, and so does the lap. Walk it once and measure: for one arc this is `tan(deg/2)`,
/// which is what the fillet used before.
fn chain_tangent(shape: &[(f32, f32)], deg: f32) -> f32 {
    let total = deg.to_radians();
    let (mut x, mut z, mut th) = (0.0f32, 0.0f32, 0.0f32);
    for (m, frac) in shape {
        let d = frac * total;
        x += m * ((th + d).sin() - th.sin());
        z += m * (th.cos() - (th + d).cos());
        th += d;
    }
    x.hypot(z) / (2.0 * (total / 2.0).cos()).abs().max(1e-3)
}

/// Round every corner of the outline, and keep what is left of each edge as a straight.
///
/// The radius is chosen by how hard the corner is — a hairpin is 9 to 19 m and a sweeper is a
/// hundred — and then taken as large as the edges allow. Both matter. Using the hairpin's
/// radius for every bend leaves a lap reading as straights joined by corners: an arc of radius
/// 25 through 15° is seven metres long and the forty metres either side of it are straight.
/// Taking a random radius inside the band rather than the largest one available is worth
/// twenty points of arc fraction on its own.
fn fillet(pts: &[(f32, f32)], rng: &mut Rng) -> (Vec<Segment>, Start) {
    let n = pts.len();
    let corners: Vec<Option<Corner>> = (0..n)
        .map(|i| {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let v1 = (b.0 - a.0, b.1 - a.1);
            let v2 = (c.0 - b.0, c.1 - b.1);
            let (l1, l2) = (v1.0.hypot(v1.1).max(1e-6), v2.0.hypot(v2.1).max(1e-6));
            let cross = v1.0 * v2.1 - v1.1 * v2.0;
            let dot = v1.0 * v2.0 + v1.1 * v2.1;
            let delta = cross.atan2(dot);
            if delta.abs() < 6f32.to_radians() {
                return None;
            }
            let half = delta.abs() / 2.0;
            let deg = delta.abs().to_degrees();
            // Tight. A corner only wears grooves if riders lean on it, and nothing leans on
            // a hundred-metre sweeper: a lap of them rode, in one word, with "no ruts". So
            // the bands stop at 45 m, and most of the lap sits well under that.
            let (lo, hi) = if deg >= 80.0 {
                (7.0, 12.0)
            } else if deg >= 45.0 {
                (12.0, 24.0)
            } else if deg >= 22.0 {
                (18.0, 34.0)
            } else {
                (26.0, 45.0)
            };
            // The band is the *apex* radius now; the arcs either side of it open out from
            // there, so a corner is tight where it is ridden and wide on the way in.
            let _ = (l1, l2, half);
            let shape = chain_shape(deg, rng);
            let unit = chain_tangent(&shape, deg);
            let mut r = hi * rng.range(0.88, 1.0);
            if r < lo {
                r = lo;
            }
            // Screen-space positive cross is a left turn, and a left turn is a negative
            // radius.
            let sign = if delta > 0.0 { -1.0 } else { 1.0 };
            Some(Corner {
                arcs: shape.iter().map(|(m, f)| (r * m * sign, deg * f)).collect(),
                tangent: r * unit,
            })
        })
        .collect();

    // Fit the corners to the edges they actually share.
    //
    // Capping each corner at `0.49 * min(both its edges)` is what left twenty-metre straights
    // all round the lap: it is the *same* budget whatever shape the corner is, and where one
    // edge is twice its neighbour the short one's allowance is spent on the long one too. The
    // real constraint is per edge — the two corners on it may not want more of it than it has
    // — so ask for what the band wants and relax until that holds. Whatever is left over is
    // the straight, and there is very little of it.
    let mut want: Vec<f32> = corners.iter().map(|c| c.as_ref().map_or(0.0, |c| c.tangent)).collect();
    for _ in 0..400 {
        let mut moved = false;
        for i in 0..n {
            let edge = (pts[(i + 1) % n].0 - pts[i].0).hypot(pts[(i + 1) % n].1 - pts[i].1);
            let j = (i + 1) % n;
            let sum = want[i] + want[j];
            if sum > EDGE_FILL * edge {
                let f = EDGE_FILL * edge / sum.max(1e-6);
                want[i] *= f;
                want[j] *= f;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    let corners: Vec<Option<Corner>> = corners
        .into_iter()
        .zip(&want)
        .map(|(c, t)| {
            c.map(|c| {
                let f = t / c.tangent.max(1e-6);
                Corner {
                    arcs: c.arcs.iter().map(|(r, a)| (r * f, *a)).collect(),
                    tangent: *t,
                }
            })
        })
        .collect();

    let mut segs = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let edge = (b.0 - a.0).hypot(b.1 - a.1);
        let here = corners[i].as_ref().map_or(0.0, |c| c.tangent);
        let next = corners[(i + 1) % n].as_ref().map_or(0.0, |c| c.tangent);
        let run = edge - here - next;
        if run > 0.001 {
            segs.push(Segment::Straight { length: run, rise: 0.0 });
        }
        if let Some(c) = &corners[(i + 1) % n] {
            // Not rounded. Fifty corners rounded to a tenth of a degree is a couple of
            // degrees of heading by the end of the lap, and a couple of degrees over two
            // kilometres is a lap that misses itself by forty metres.
            for (radius, angle) in &c.arcs {
                segs.push(Segment::Arc { radius: *radius, angle: *angle, rise: 0.0 });
            }
        }
    }

    // Where the lap begins: the first tangent point, running along the first edge.
    let (a, b) = (pts[0], pts[1]);
    let t0 = corners[0].as_ref().map_or(0.0, |c| c.tangent);
    let l = (b.0 - a.0).hypot(b.1 - a.1).max(1e-6);
    let start = Start {
        x: a.0 + (b.0 - a.0) / l * t0,
        z: a.1 + (b.1 - a.1) / l * t0,
        angle: (b.0 - a.0).atan2(b.1 - a.1).to_degrees().rem_euclid(360.0),
    };
    (segs, start)
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
    let mut pos = START_CLEAR_M;
    while pos < total - 40.0 {
        let room = total - 20.0 - pos;
        if tight(pos, 34.0) {
            pos += 12.0;
            continue;
        }
        // No whoops. Asked for outright, after riding a lap with them: they are the one
        // thing on a track that has to be built right or not at all, and ours are not.
        let pick = rng.range(0.0, 1.0);
        let length;
        if pick < 0.55 && room > 30.0 {
            // A table a rider can actually jump. The first ones out of here were 18 m long
            // and a metre high, which from the seat is a speed bump.
            length = rng.range(19.0, 27.0).min(room);
            out.push(Feature::Tabletop {
                at: pos,
                length,
                height: rng.range(2.4, 3.4),
            });
        } else if pick < 0.68 && room > 26.0 {
            let gap = rng.range(3.5, 7.5);
            length = (gap + 14.0).min(room);
            out.push(Feature::Double { at: pos, height: rng.range(0.8, 1.3), gap, lip: 4.0 });
        } else if pick < 0.82 && room > 24.0 {
            length = rng.range(24.0, 34.0).min(room);
            out.push(Feature::StepUp { at: pos, length, height: rng.range(1.0, 1.7) });
        } else {
            length = rng.range(10.0, 16.0).min(room);
            if length < 8.0 {
                break;
            }
            out.push(Feature::Roller { at: pos, length, height: rng.range(0.55, 0.95) });
        }
        // Close together. A lap that rode as "a lot of flat long sections, almost zero
        // features" was leaving up to 34 m of nothing between one jump and the next, on top
        // of whatever the corners took.
        pos += length + rng.range(8.0, 20.0);
    }
    out
}

/// One lap, from one number. Not checked — see [`search`] for that.
pub fn draw(seed: u64) -> TrackProgram {
    let mut rng = Rng::new(seed);
    let vertices = rng.int(64, 84) as usize;
    let points = merge_runs(&outline(&mut rng, vertices));
    let (segments, start) = fillet(&points, &mut rng);
    let features = features(&mut rng, &segments);
    let surface = match rng.int(0, 9) {
        0..=6 => Surface::Soil,
        7..=8 => Surface::Sand,
        _ => Surface::Grass,
    };
    TrackProgram {
        name: NAMES[(seed % NAMES.len() as u64) as usize].to_string(),
        author: "MXB App".into(),
        location: PLACES[((seed / 7) % PLACES.len() as u64) as usize].to_string(),
        width: rng.range(10.0, 13.5),
        blend: crate::trackprog::default_blend(),
        terrain: Terrain {
            size_x: PLOT_M,
            size_z: PLOT_M,
            samples: 2049,
            scale: if rng.chance(0.5) { 63.0 } else { 70.0 },
            surface,
            wear: crate::trackprog::default_wear(),
            relief: Relief {
                amplitude: rng.range(5.0, 11.0),
                wavelength: rng.range(320.0, 480.0),
                seed: (seed % 9973) as u32,
                texture: 0.085,
                tilt: rng.range(10.0, 30.0),
                tilt_angle: rng.range(0.0, 359.0),
                landforms: rng.int(4, 8) as u32,
                landform_height: rng.range(10.0, 22.0),
            },
        },
        start,
        segments,
        features,
        elevation: Vec::new(),
    }
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
        let mut program = draw(seed);
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

    /// The two properties the shape exists to guarantee, over a spread of seeds.
    #[test]
    fn every_lap_closes_and_none_crosses_itself() {
        for seed in [1u64, 7, 42, 777, 1234, 31337] {
            let mut p = draw(seed);
            crate::trackllm::repair_for_tests(&mut p);
            assert!(
                p.closure_error() < 20.0,
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

    /// And the shape of it: corners, turning, jumps, all against the corpus.
    #[test]
    fn a_drawn_lap_is_shaped_like_a_published_one() {
        let mut p = draw(1234);
        crate::trackllm::repair_for_tests(&mut p);
        let lap = p.lap_length();
        let arcs: f32 = p
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Arc { radius, angle, .. } => Some(radius.abs() * angle.to_radians()),
                _ => None,
            })
            .sum();
        let turning: f32 = p
            .segments
            .iter()
            .filter_map(|s| match s {
                Segment::Arc { angle, .. } => Some(angle.abs()),
                _ => None,
            })
            .sum();
        assert!((500.0..2600.0).contains(&lap), "the lap is {lap:.0} m");
        assert!(arcs / lap > 0.5, "only {:.0}% of it is arcs", arcs / lap * 100.0);
        assert!(turning > 1400.0, "it turns {turning:.0}° in total");
        let per_km = p.features.len() as f32 / lap * 1000.0;
        assert!((12.0..45.0).contains(&per_km), "{per_km:.0} jumps a km");
    }

    /// The same number gives the same track, on any machine. Seeds are how a track is named
    /// and found again, so this is not a detail.
    #[test]
    fn a_seed_draws_the_same_lap_every_time() {
        let a = draw(99);
        let b = draw(99);
        assert_eq!(a.name, b.name);
        assert_eq!(a.segments.len(), b.segments.len());
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
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

    /// What a drawn lap measures against Indiana, seed by seed. Printing, not asserting —
    /// `cargo test -- --ignored --nocapture drawn_layout_numbers`.
    #[test]
    #[ignore]
    fn drawn_layout_numbers() {
        println!("[chain v2 equal-length]");
        println!("seed  segs  arc%  corners  arcs/corner  turning  straights p50  closure");
        for seed in [1u64, 16, 31, 7, 42, 1234] {
            let mut p = draw(seed);
            crate::trackllm::repair_for_tests(&mut p);
            let n = p.segments.len();
            let straights: Vec<f32> = p
                .segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Straight { length, .. } => Some(*length),
                    _ => None,
                })
                .collect();
            let turning: f32 = p
                .segments
                .iter()
                .map(|s| match s {
                    Segment::Arc { angle, .. } => *angle,
                    _ => 0.0,
                })
                .sum();
            // Corners: runs of consecutive same-sign arcs turning 25 deg or more.
            let mut runs: Vec<(f32, usize)> = Vec::new();
            let (mut deg, mut cnt, mut sign) = (0.0f32, 0usize, 0.0f32);
            for s in &p.segments {
                match s {
                    Segment::Arc { radius, angle, .. } if radius.signum() == sign || cnt == 0 => {
                        sign = radius.signum();
                        deg += angle;
                        cnt += 1;
                    }
                    Segment::Arc { radius, angle, .. } => {
                        if deg >= 25.0 {
                            runs.push((deg, cnt));
                        }
                        sign = radius.signum();
                        deg = *angle;
                        cnt = 1;
                    }
                    _ => {
                        if deg >= 25.0 {
                            runs.push((deg, cnt));
                        }
                        deg = 0.0;
                        cnt = 0;
                        sign = 0.0;
                    }
                }
            }
            if deg >= 25.0 {
                runs.push((deg, cnt));
            }
            let mut per: Vec<usize> = runs.iter().map(|r| r.1).collect();
            per.sort_unstable();
            let mut st = straights.clone();
            st.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!(
                "{seed:5} {n:5} {:5.2} {:8} {:12} {:8.0} {:14.0} {:8.1}",
                1.0 - straights.len() as f32 / n as f32,
                runs.len(),
                per.get(per.len() / 2).copied().unwrap_or(0),
                turning,
                st.get(st.len() / 2).copied().unwrap_or(0.0),
                p.closure_error(),
            );
            let arc_len: f32 = p.segments.iter().filter_map(|s| match s {
                Segment::Arc { radius, angle, .. } => Some(radius.abs() * angle.to_radians()),
                _ => None,
            }).sum();
            println!("        arc share by length {:.2}", arc_len / p.lap_length());
        }
        println!("Indiana: 120 segs, arc% 0.99, 14 corners, 6 arcs/corner, 2407 deg, 1 straight");
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
        let mut p = draw(seed);
        crate::trackllm::repair_for_tests(&mut p);
        let syn = crate::tracksynth::synthesise(&p).expect("synthesise");
        let bytes = crate::tracksynth::trh(&p, &syn, true);
        let out = std::path::Path::new(&dir).join(format!("seed{seed}.trh"));
        std::fs::write(&out, &bytes).expect("write");
        println!("wrote {} ({} bytes) name={} lap={:.0} m", out.display(), bytes.len(), p.name, p.lap_length());
    }
}
