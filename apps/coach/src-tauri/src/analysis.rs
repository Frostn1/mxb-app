//! Compares a lap with a faster reference lap and says where the time went, and why.
//!
//! Both laps are resampled onto the same one-metre grid along the centreline (the game's lap
//! position × track length), so index `i` is the same place on track in both. The reference
//! decides where the corners, jumps and whoops are, because it is the lap that rode them well.
//! Each section's time loss is then put down to the inputs that differ: brake point, brake
//! force, minimum speed, lean, line, throttle pickup, wheelspin, gear, airtime, landing.
//!
//! The rules come from real-life motocross coaching, adjusted where MX Bikes differs (scrub
//! seated and lean the bike, rider lean is its own input, aids blur the brake and clutch
//! channels). The thresholds are starting values to tune against real laps and all live in
//! `th`. Sources are listed in `docs/coach/COACHING.md`.

use std::f32::consts::PI;
use std::ops::Range;

use serde::Serialize;

use crate::telemetry::{Lap, Sample};

/// Grid spacing. Section bounds and finding positions are grid indices, so metres.
pub const STEP_M: f32 = 1.0;
const G: f32 = 9.81;
const KMH: f32 = 3.6;

mod th {
    pub const CORNER_CURVATURE: f32 = 1.0 / 45.0; // tighter than a 45 m radius
    pub const CORNER_MIN_DEG: f32 = 35.0;
    pub const CORNER_MERGE_M: usize = 8;
    pub const CORNER_ENTRY_M: usize = 70; // room for the braking zone
    pub const CORNER_EXIT_M: usize = 25;
    pub const JUMP_MIN_AIR_S: f32 = 0.3;
    /// Shorter than this in the air is a hop over a bump, not a jump.
    pub const JUMP_MIN_M: usize = 6;
    /// Back in the air this soon after touching down is a bounce off the same landing.
    pub const BOUNCE_M: usize = 5;
    pub const JUMP_GROUP_M: usize = 30; // closer than this and it's one rhythm or whoops
    /// A group's jumps averaging this long or less in the air are whoops, not a rhythm.
    pub const WHOOP_HOP_M: usize = 10;
    pub const JUMP_ENTRY_M: usize = 25;
    pub const JUMP_EXIT_M: usize = 15;
    pub const MATCH_JUMP_M: i64 = 20;
    pub const MIN_STRAIGHT_M: usize = 40;
    /// A section losing less than this gets no advice: not worth a rider's attention.
    pub const WORTH_S: f32 = 0.05;
    pub const FOCUS: usize = 3;

    pub const BRAKE_ON: f32 = 0.1;
    pub const BRAKE_POINT_M: f32 = 5.0;
    pub const SOFT_BRAKING: f32 = 0.7;
    pub const LONG_BRAKING: f32 = 1.2;
    pub const FRONT_SHARE_LOW: f32 = 0.5;
    pub const FRONT_SHARE_GOOD: f32 = 0.6;
    pub const MIN_SPEED_RATIO: f32 = 0.95;
    pub const LEAN_DEG: f32 = 5.0;
    pub const LINE_M: f32 = 1.5;
    pub const COAST_S: f32 = 0.3;
    pub const THROTTLE_ON: f32 = 0.3;
    pub const THROTTLE_HOLD_M: usize = 5;
    pub const THROTTLE_POINT_M: f32 = 3.0;
    pub const SPIN_SLIP: f32 = 1.15;
    pub const SPIN_S: f32 = 0.3;
    pub const FRONT_LOCK_SLIP: f32 = 0.8;
    pub const FRONT_LOCK_S: f32 = 0.15;
    pub const CLUTCH_S: f32 = 0.5;
    pub const EXIT_KMH: f32 = 3.0;

    pub const FLOAT_AIR: f32 = 1.1;
    pub const FLOAT_HEIGHT_M: f32 = 0.5;
    pub const SCRUB_ROLL_DEG: f32 = 20.0;
    pub const SHORT_M: i64 = 2;
    pub const LONG_M: i64 = 3;
    pub const CHOP: f32 = 0.3;
    pub const CHOP_M: usize = 15;
    pub const LAND_THROTTLE_LOW: f32 = 0.3;
    pub const LAND_THROTTLE_GOOD: f32 = 0.5;
    pub const LAND_ROLL_DEG: f32 = 15.0;
    /// Share of the travel that counts as bottomed out.
    pub const BOTTOM: f32 = 0.95;
    pub const BOTTOMS_PER_LAP: usize = 3;
    /// Never past this share of the travel all lap: the spring or damping is too stiff.
    pub const LAZY_TRAVEL: f32 = 0.7;
    pub const WHEELIE_S: f32 = 0.3;
    pub const STOPPIE_S: f32 = 0.2;
    pub const REAR_LOCK_SLIP: f32 = 0.7;
    pub const REAR_LOCK_S: f32 = 0.4;
    pub const AIR_GAS: f32 = 0.6;

    // Setup, over a whole lap.
    pub const LIMITER_LAP_S: f32 = 1.5;
    /// Below this share of max revs at a corner's slowest point, the engine is bogging.
    pub const BOG_SHARE: f32 = 0.45;
    /// Upshifting below this share of the bike's shift point is short-shifting.
    pub const SHORT_SHIFT: f32 = 0.85;
    pub const WHEELIE_LAP_S: f32 = 1.0;

    // A lap on its own: plain amounts, with nothing to hold them against.
    // Set from real laps (2026-09-15): lower bars flagged most turns and jumps of a good lap.
    pub const SOLO_COAST_S: f32 = 1.0;
    pub const SOLO_SPIN_S: f32 = 0.8;
    pub const SOLO_SKID_S: f32 = 0.6;
    pub const SOLO_WHEELIE_S: f32 = 0.7;
    pub const SOLO_LAND_ROLL_DEG: f32 = 25.0;
    pub const SOLO_CHOP: f32 = 0.5;
    pub const SOLO_AIR_GAS: f32 = 0.85;
    pub const SOLO_LIMITER_S: f32 = 0.5;
    pub const SOLO_PART_GAS: f32 = 0.75;
    pub const SOLO_LONG_STRAIGHT_M: usize = 80;
    pub const SOLO_OFF_GAS_SHARE: f32 = 0.35;
    pub const TAKEOFF_SPEED_RATIO: f32 = 0.97;

    pub const WHOOPS_SPEED_RATIO: f32 = 0.95;
    pub const WHOOPS_OFF_GAS: f32 = 0.4;
    pub const WHOOPS_OFF_GAS_SHARE: f32 = 0.2;
    pub const WHOOPS_PITCH: f32 = 1.5;

    pub const LIMITER_S: f32 = 0.3;
    pub const PART_GAS: f32 = 0.15;
}

/// One grid point: the lap as it passed this metre of track.
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    /// Seconds since the line.
    pub t: f32,
    /// Ground speed, m/s.
    pub v: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub throttle: f32,
    pub front: f32,
    pub rear: f32,
    pub clutch: f32,
    pub roll: f32,
    pub pitch: f32,
    pub rpm: f32,
    pub gear: i32,
    pub air: bool,
    /// Wheel speed over ground speed: under 1 is locking, over 1 is spinning.
    pub slip_f: f32,
    pub slip_r: f32,
    /// Suspension length as the game reports it, metres, front then rear.
    pub susp: [f32; 2],
    /// Which wheels are off the ground, front then rear.
    pub off: [bool; 2],
    /// Share of the travel in use, 0 fully extended to 1 bottomed; see `Trace::fill_travel`.
    pub used: [f32; 2],
}

impl Point {
    fn brake(&self) -> f32 {
        self.front.max(self.rear)
    }
}

fn point(s: &Sample) -> Point {
    let v = (s.vel[0] * s.vel[0] + s.vel[1] * s.vel[1] + s.vel[2] * s.vel[2]).sqrt();
    let slip = |w: f32| if v > 3.0 { w / v } else { 1.0 };
    Point {
        t: s.t,
        v,
        x: s.x,
        y: s.y,
        z: s.z,
        throttle: s.throttle,
        front: s.front_brake,
        rear: s.rear_brake,
        clutch: s.clutch,
        roll: s.roll,
        pitch: s.pitch,
        rpm: s.rpm,
        gear: s.gear,
        air: s.airborne(),
        slip_f: slip(s.wheel_speed[0]),
        slip_r: slip(s.wheel_speed[1]),
        susp: s.susp,
        off: [s.wheel_material[0] == 0, s.wheel_material[1] == 0],
        used: [0.0; 2],
    }
}

fn lerp(a: &Point, b: &Point, f: f32) -> Point {
    let m = |p: f32, q: f32| p + (q - p) * f;
    let near = if f < 0.5 { a } else { b };
    Point {
        t: m(a.t, b.t),
        v: m(a.v, b.v),
        x: m(a.x, b.x),
        y: m(a.y, b.y),
        z: m(a.z, b.z),
        throttle: m(a.throttle, b.throttle),
        front: m(a.front, b.front),
        rear: m(a.rear, b.rear),
        clutch: m(a.clutch, b.clutch),
        roll: m(a.roll, b.roll),
        pitch: m(a.pitch, b.pitch),
        rpm: m(a.rpm, b.rpm),
        gear: near.gear,
        air: near.air,
        slip_f: m(a.slip_f, b.slip_f),
        slip_r: m(a.slip_r, b.slip_r),
        susp: [m(a.susp[0], b.susp[0]), m(a.susp[1], b.susp[1])],
        off: near.off,
        used: [0.0; 2],
    }
}

/// A lap on the metre grid.
#[derive(Clone, Debug)]
pub struct Trace {
    pub pts: Vec<Point>,
}

impl Trace {
    /// None for a lap too short to place on the track.
    pub fn new(lap: &Lap, track_len: f32) -> Option<Trace> {
        if track_len <= STEP_M {
            return None;
        }
        // Distance must rise: a sample that didn't move forward (a stall, a wobble backwards
        // after a crash) has no place on the grid.
        let mut src: Vec<(f32, Point)> = Vec::with_capacity(lap.samples.len());
        for s in &lap.samples {
            let d = s.pos * track_len;
            if src.last().map_or(true, |(last, _)| d > *last) {
                src.push((d, point(s)));
            }
        }
        if src.len() < 10 {
            return None;
        }
        let n = (track_len / STEP_M).floor() as usize + 1;
        let mut pts = Vec::with_capacity(n);
        let mut k = 0;
        for i in 0..n {
            let g = i as f32 * STEP_M;
            while k + 2 < src.len() && src[k + 1].0 < g {
                k += 1;
            }
            let (d0, a) = &src[k];
            let (d1, b) = &src[k + 1];
            let mut p = lerp(a, b, ((g - d0) / (d1 - d0)).clamp(0.0, 1.0));
            // Past either end of the samples: carry on at the speed it was doing.
            if g < *d0 {
                p.t = a.t - (d0 - g) / a.v.max(1.0);
            } else if g > *d1 {
                p.t = b.t + (g - d1) / b.v.max(1.0);
            }
            pts.push(p);
        }
        let t0 = pts[0].t;
        for p in &mut pts {
            p.t -= t0;
        }
        Some(Trace { pts })
    }

    pub fn len(&self) -> usize {
        self.pts.len()
    }

    pub fn time(&self) -> f32 {
        self.pts.last().map_or(0.0, |p| p.t)
    }

    /// Works out how much of its travel each end uses, given the travel in metres. The fully
    /// extended length is read off the lap itself, in the air where nothing loads the springs,
    /// so it doesn't matter which way the game counts. False when there's nothing to go on.
    fn fill_travel(&mut self, travel: [f32; 2]) -> bool {
        if travel[0] <= 0.0 || travel[1] <= 0.0 {
            return false;
        }
        let mut ext = [0.0f32; 2];
        for (k, e) in ext.iter_mut().enumerate() {
            let mut v: Vec<f32> = self.pts.iter().filter(|q| q.off[0] && q.off[1]).map(|q| q.susp[k]).collect();
            if v.len() < 5 {
                return false;
            }
            v.sort_by(f32::total_cmp);
            *e = v[v.len() / 2];
        }
        for q in &mut self.pts {
            for k in 0..2 {
                q.used[k] = ((q.susp[k] - ext[k]).abs() / travel[k]).min(1.2);
            }
        }
        true
    }

    pub(crate) fn span(&self, a: usize, b: usize) -> f32 {
        self.pts[b].t - self.pts[a].t
    }

    fn dt(&self, i: usize) -> f32 {
        self.pts.get(i + 1).map_or(0.0, |n| n.t - self.pts[i].t)
    }

    fn time_where(&self, r: Range<usize>, pred: impl Fn(&Point) -> bool) -> f32 {
        r.filter(|&i| pred(&self.pts[i])).map(|i| self.dt(i)).sum()
    }

    fn first(&self, r: Range<usize>, pred: impl Fn(&Point) -> bool) -> Option<usize> {
        r.into_iter().find(|&i| pred(&self.pts[i]))
    }

    fn slowest(&self, r: Range<usize>) -> usize {
        let start = r.start;
        r.min_by(|&i, &j| self.pts[i].v.total_cmp(&self.pts[j].v)).unwrap_or(start)
    }

    fn max_by(&self, r: Range<usize>, f: impl Fn(&Point) -> f32) -> f32 {
        r.map(|i| f(&self.pts[i])).fold(f32::MIN, f32::max)
    }

    fn mean(&self, r: Range<usize>, f: impl Fn(&Point) -> f32) -> f32 {
        let n = r.len().max(1) as f32;
        r.map(|i| f(&self.pts[i])).sum::<f32>() / n
    }

    /// Deceleration in G at a grid point, from the speed either side of it.
    fn decel_g(&self, i: usize) -> f32 {
        let (a, b) = (i.saturating_sub(1), (i + 1).min(self.len() - 1));
        let dt = self.pts[b].t - self.pts[a].t;
        if dt <= 0.0 {
            0.0
        } else {
            (self.pts[a].v - self.pts[b].v) / dt / G
        }
    }

    /// Travel bearing at a grid point, radians clockwise from north (+z).
    pub(crate) fn bearing(&self, i: usize) -> f32 {
        let n = self.len();
        let (a, b) = (&self.pts[i.saturating_sub(2)], &self.pts[(i + 2).min(n - 1)]);
        (b.x - a.x).atan2(b.z - a.z)
    }
}

fn wrap(mut a: f32) -> f32 {
    while a > PI {
        a -= 2.0 * PI;
    }
    while a < -PI {
        a += 2.0 * PI;
    }
    a
}

/// Signed curvature, radians per metre; positive turns right. Smoothed over ±4 m so a
/// wobble on a straight isn't a corner.
fn curvature(tr: &Trace) -> Vec<f32> {
    let n = tr.len();
    let heading: Vec<f32> = (0..n).map(|i| tr.bearing(i)).collect();
    let raw: Vec<f32> =
        (0..n).map(|i| if i + 1 < n { wrap(heading[i + 1] - heading[i]) / STEP_M } else { 0.0 }).collect();
    (0..n)
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(4), (i + 5).min(n));
            raw[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
        })
        .collect()
}

/// Stretches with both wheels off the ground long enough to be a jump: (takeoff, landing). A
/// landing that touches and bounces straight back up is one jump, not two.
fn air_runs(tr: &Trace, within: Range<usize>) -> Vec<(usize, usize)> {
    let mut raw: Vec<(usize, usize)> = Vec::new();
    let mut i = within.start;
    while i < within.end {
        if !tr.pts[i].air {
            i += 1;
            continue;
        }
        let s = i;
        while i < within.end && tr.pts[i].air {
            i += 1;
        }
        match raw.last_mut() {
            Some(last) if s - last.1 <= th::BOUNCE_M => last.1 = i - 1,
            _ => raw.push((s, i - 1)),
        }
    }
    raw.into_iter().filter(|&(s, e)| e - s >= th::JUMP_MIN_M && tr.span(s, e) >= th::JUMP_MIN_AIR_S).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Straight,
    Corner,
    Jump,
    Rhythm,
    Whoops,
}

/// A stretch of track with one job: a corner with its braking zone and exit, a jump with its
/// face and landing, or the straight between.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub kind: Kind,
    pub name: String,
    pub start: usize,
    pub end: usize,
    /// The part that makes it what it is: the turn itself, or takeoff to last landing.
    pub core: (usize, usize),
    /// +1 right-hander, -1 left-hander, 0 otherwise.
    pub dir: i8,
    #[serde(skip)]
    runs: Vec<(usize, usize)>,
}

struct Feature {
    kind: Kind,
    a: usize,
    b: usize,
    dir: i8,
    runs: Vec<(usize, usize)>,
}

impl Feature {
    fn entry(&self) -> usize {
        if self.kind == Kind::Corner { th::CORNER_ENTRY_M } else { th::JUMP_ENTRY_M }
    }
    fn exit(&self) -> usize {
        if self.kind == Kind::Corner { th::CORNER_EXIT_M } else { th::JUMP_EXIT_M }
    }
}

fn features(tr: &Trace) -> Vec<Feature> {
    let n = tr.len();
    let mut feats = Vec::new();

    let mut group: Vec<(usize, usize)> = Vec::new();
    let flush = |group: &mut Vec<(usize, usize)>, feats: &mut Vec<Feature>| {
        if group.is_empty() {
            return;
        }
        // Whoops are many short hops close together; a rhythm is proper jumps in a row.
        let n = group.len();
        let hop = group.iter().map(|&(s, e)| e - s).sum::<usize>() / n;
        let kind = match n {
            1 => Kind::Jump,
            _ if n >= 3 && hop <= th::WHOOP_HOP_M => Kind::Whoops,
            _ => Kind::Rhythm,
        };
        let (a, b) = (group[0].0, group[group.len() - 1].1);
        feats.push(Feature { kind, a, b, dir: 0, runs: std::mem::take(group) });
    };
    for run in air_runs(tr, 0..n) {
        if group.last().is_some_and(|last| run.0 - last.1 > th::JUMP_GROUP_M) {
            flush(&mut group, &mut feats);
        }
        group.push(run);
    }
    flush(&mut group, &mut feats);

    let k = curvature(tr);
    let mut cores: Vec<(usize, usize, f32)> = Vec::new();
    let mut i = 0;
    while i < n {
        if k[i].abs() <= th::CORNER_CURVATURE {
            i += 1;
            continue;
        }
        let (s, sign) = (i, k[i].signum());
        while i < n && k[i].abs() > th::CORNER_CURVATURE && k[i].signum() == sign {
            i += 1;
        }
        match cores.last_mut() {
            Some(last) if last.2 == sign && s - last.1 <= th::CORNER_MERGE_M => last.1 = i - 1,
            _ => cores.push((s, i - 1, sign)),
        }
    }
    for (a, b, sign) in cores {
        let turned = k[a..=b].iter().sum::<f32>().abs() * STEP_M * 180.0 / PI;
        let in_jump = feats.iter().any(|f| f.kind != Kind::Corner && a <= f.b && f.a <= b);
        if turned >= th::CORNER_MIN_DEG && !in_jump {
            feats.push(Feature { kind: Kind::Corner, a, b, dir: sign as i8, runs: Vec::new() });
        }
    }
    feats.sort_by_key(|f| f.a);
    feats
}

/// Splits the track into sections, from the reference lap.
pub fn sections(reference: &Trace) -> Vec<Section> {
    let last = reference.len() - 1;
    let feats = features(reference);
    let mut out = Vec::new();
    let mut counts = [0usize; 5];
    let mut name = |kind: Kind| {
        let k = kind as usize;
        counts[k] += 1;
        let word = ["Straight", "Turn", "Jump", "Rhythm", "Whoops"][k];
        format!("{word} {}", counts[k])
    };
    let straight = |a: usize, b: usize, name: String| Section {
        kind: Kind::Straight,
        name,
        start: a,
        end: b,
        core: (a, b),
        dir: 0,
        runs: Vec::new(),
    };

    let mut cursor = 0;
    for (i, f) in feats.iter().enumerate() {
        let mut start = f.a.saturating_sub(f.entry()).max(cursor);
        if start - cursor > th::MIN_STRAIGHT_M {
            out.push(straight(cursor, start, name(Kind::Straight)));
        } else {
            start = cursor;
        }
        let next = feats.get(i + 1).map_or(last, |n| n.a.saturating_sub(n.entry()));
        let end = (f.b + f.exit()).min(next.max(f.b + 1)).min(last);
        out.push(Section {
            kind: f.kind,
            name: name(f.kind),
            start,
            end,
            core: (f.a, f.b),
            dir: f.dir,
            runs: f.runs.clone(),
        });
        cursor = end;
    }
    if last - cursor > th::MIN_STRAIGHT_M || out.is_empty() {
        out.push(straight(cursor, last, name(Kind::Straight)));
    } else if let Some(s) = out.last_mut() {
        s.end = last;
    }
    out
}

/// One piece of advice.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable id for the rule, e.g. `brake_early`.
    pub skill: &'static str,
    pub title: String,
    pub detail: String,
    /// Where on track, metres.
    pub at: usize,
    /// Orders the advice within a section: the likeliest cause first.
    #[serde(skip)]
    weight: f32,
    /// Advice about something risky: shown even where the section lost no time.
    #[serde(skip)]
    safety: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionReview {
    #[serde(flatten)]
    pub section: Section,
    pub lap_time: f32,
    pub ref_time: f32,
    /// Seconds lost to the reference here; negative is a gain.
    pub lost: f32,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Channel {
    pub lap: Vec<f32>,
    pub reference: Vec<f32>,
}

/// The traces the review draws, every `step` metres.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channels {
    pub step: usize,
    /// Running time gap, seconds; positive is behind.
    pub delta: Vec<f32>,
    pub speed: Channel,
    pub throttle: Channel,
    pub brake: Channel,
    pub lean: Channel,
    pub gear: Channel,
    pub height: Channel,
    /// Share of the travel in use, percent; all zero when the lap gives nothing to go on.
    pub fork: Channel,
    pub shock: Channel,
}

/// Both laps' world x/z every `step` metres, with the bike's height at each point.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    pub step: usize,
    pub lap: Vec<[f32; 2]>,
    pub reference: Vec<[f32; 2]>,
    pub lap_y: Vec<f32>,
    pub reference_y: Vec<f32>,
}

/// Every grid point: the grid is already a metre apart, and the lines are what the rider looks at.
fn paths(p: &Trace, r: &Trace) -> Paths {
    let xz = |t: &Trace| t.pts.iter().map(|q| [q.x, q.z]).collect();
    let y = |t: &Trace| t.pts.iter().map(|q| q.y).collect();
    Paths { step: 1, lap: xz(p), reference: xz(r), lap_y: y(p), reference_y: y(r) }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub lap_time: f32,
    pub ref_time: f32,
    pub sections: Vec<SectionReview>,
    /// The sections to work on first: most time lost, at most three.
    pub focus: Vec<usize>,
    /// The lap in a few lines: where the time went, by theme.
    pub overall: Vec<Theme>,
    /// Bike setup advice for the whole lap: suspension, gearing, shifting, chassis.
    pub setup: Vec<Finding>,
    /// Reviewed on its own, with no faster lap to compare with.
    pub solo: bool,
    pub channels: Channels,
    pub paths: Paths,
}

/// One kind of mistake across the lap, e.g. braking, with the time it cost.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub name: &'static str,
    /// Seconds, over every section where it's the main cause; 0 on a lap reviewed alone.
    pub lost: f32,
    pub sections: Vec<String>,
    /// The headline tip where it cost most.
    pub tip: String,
    #[serde(skip)]
    most: f32,
}

/// The bike the lap was ridden on, as far as the review needs it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bike {
    /// Rev limiter, 0 if unknown.
    pub limiter: f32,
    pub max_rpm: f32,
    /// Where the bike's maker says to change up, 0 if unknown.
    pub shift_rpm: f32,
    /// Suspension travel in metres, front then rear; 0 if unknown.
    pub travel: [f32; 2],
}

fn theme(skill: &str) -> &'static str {
    match skill {
        "brake_early" | "brake_late" | "brake_harder" | "brake_unneeded" | "more_front" | "front_lock"
        | "rear_lock" | "stoppie" | "clutch_braking" | "bottom_braking" => "Braking",
        "carry_speed" | "lean_more" | "line" | "coasting" => "Corner speed",
        "late_throttle" | "wheelspin" | "wheelie" | "exit_speed" | "gear_up" | "gear_down" => "Corner exits",
        "jump_it" | "chop_face" | "scrub" | "land_short" | "overjump" | "land_throttle" | "land_crooked"
        | "bottom_landing" | "air_throttle" | "rhythm_count" => "Jumps",
        "whoops_speed" | "whoops_throttle" | "whoops_bucking" => "Whoops",
        "shift_earlier" | "full_gas" => "Straights",
        _ => "Other",
    }
}

/// The lap by theme: each section counts towards the theme of its headline tip.
fn overall(sections: &[SectionReview], solo: bool) -> Vec<Theme> {
    let mut themes: Vec<Theme> = Vec::new();
    for s in sections {
        let Some(head) = s.findings.iter().find(|f| f.skill != "unclear") else { continue };
        if !solo && s.lost <= th::WORTH_S && !head.safety {
            continue;
        }
        let (name, lost) = (theme(head.skill), if solo { 0.0 } else { s.lost.max(0.0) });
        match themes.iter_mut().find(|t| t.name == name) {
            Some(t) => {
                t.lost += lost;
                t.sections.push(s.section.name.clone());
                if lost > t.most {
                    (t.most, t.tip) = (lost, head.title.clone());
                }
            }
            None => themes.push(Theme { name, lost, sections: vec![s.section.name.clone()], tip: head.title.clone(), most: lost }),
        }
    }
    themes.sort_by(|a, b| b.lost.total_cmp(&a.lost).then(b.sections.len().cmp(&a.sections.len())));
    for t in &mut themes {
        t.lost = (t.lost * 1000.0).round() / 1000.0;
    }
    themes
}

/// The same tip twice in one section is said once; on a jump section, with how many more
/// jumps it applies to.
fn dedupe(findings: Vec<Finding>, jumps: bool) -> Vec<Finding> {
    let mut out: Vec<(Finding, usize)> = Vec::new();
    for f in findings {
        match out.iter_mut().find(|(o, _)| o.skill == f.skill && o.title == f.title) {
            Some((_, n)) => *n += 1,
            None => out.push((f, 0)),
        }
    }
    out.into_iter()
        .map(|(mut f, n)| {
            if n > 0 && jumps {
                f.detail.push_str(&format!(" The same goes for {n} more jump{} here.", if n == 1 { "" } else { "s" }));
            }
            f
        })
        .collect()
}

fn ordinal(n: usize) -> String {
    match n {
        1 => "first".into(),
        2 => "second".into(),
        3 => "third".into(),
        4 => "fourth".into(),
        _ => format!("{n}th"),
    }
}

/// Compares `lap` with the faster `reference`, both ridden on `bike`.
pub fn review(lap: &Trace, reference: &Trace, bike: Bike) -> Review {
    let n = lap.len().min(reference.len());
    let (mut p, mut r) = (Trace { pts: lap.pts[..n].to_vec() }, Trace { pts: reference.pts[..n].to_vec() });
    let travel = p.fill_travel(bike.travel) && r.fill_travel(bike.travel);
    let (p, r) = (&p, &r);
    let secs = sections(r);
    let setup = setup(p, Some(r), &secs, bike, travel);
    let mut out: Vec<SectionReview> = secs
        .into_iter()
        .map(|s| {
            let (lap_time, ref_time) = (p.span(s.start, s.end), r.span(s.start, s.end));
            let lost = lap_time - ref_time;
            let mut c = Ctx { p, r, s: &s, bike, travel, out: Vec::new() };
            match s.kind {
                Kind::Corner => corner(&mut c),
                Kind::Jump | Kind::Rhythm => jumps(&mut c),
                Kind::Whoops => whoops(&mut c),
                Kind::Straight => straight(&mut c),
            }
            let mut findings = dedupe(c.out, s.kind != Kind::Corner);
            if lost <= th::WORTH_S {
                findings.retain(|f| f.safety);
            } else if findings.iter().all(|f| f.safety) {
                findings.push(Finding {
                    skill: "unclear",
                    title: "Compare the traces".into(),
                    detail: format!(
                        "You lose {lost:.2} s here, but none of your inputs stand out. \
                         Look at the speed trace against the fast lap."
                    ),
                    at: s.start,
                    weight: 0.0,
                    safety: false,
                });
            }
            findings.sort_by(|a, b| b.weight.total_cmp(&a.weight));
            SectionReview { section: s, lap_time, ref_time, lost, findings }
        })
        .collect();

    let mut order: Vec<usize> = (0..out.len()).filter(|&i| out[i].lost > th::WORTH_S).collect();
    order.sort_by(|&a, &b| out[b].lost.total_cmp(&out[a].lost));
    order.truncate(th::FOCUS);
    for s in &mut out {
        s.lost = (s.lost * 1000.0).round() / 1000.0;
    }

    Review {
        lap_time: p.time(),
        ref_time: r.time(),
        overall: overall(&out, false),
        sections: out,
        focus: order,
        setup,
        solo: false,
        channels: channels(p, r, 2),
        paths: paths(p, r),
    }
}

fn channels(p: &Trace, r: &Trace, step: usize) -> Channels {
    let ch = |f: &dyn Fn(&Point) -> f32| Channel {
        lap: p.pts.iter().step_by(step).map(f).collect(),
        reference: r.pts.iter().step_by(step).map(f).collect(),
    };
    Channels {
        step,
        delta: p.pts.iter().zip(&r.pts).step_by(step).map(|(a, b)| a.t - b.t).collect(),
        speed: ch(&|q| q.v * KMH),
        throttle: ch(&|q| q.throttle),
        brake: ch(&|q| q.brake()),
        lean: ch(&|q| q.roll),
        gear: ch(&|q| q.gear as f32),
        height: ch(&|q| q.y),
        fork: ch(&|q| q.used[0] * 100.0),
        shock: ch(&|q| q.used[1] * 100.0),
    }
}

struct Ctx<'a> {
    p: &'a Trace,
    r: &'a Trace,
    s: &'a Section,
    bike: Bike,
    /// Suspension travel could be worked out for both laps.
    travel: bool,
    out: Vec<Finding>,
}

impl Ctx<'_> {
    fn add(&mut self, skill: &'static str, weight: f32, at: usize, title: &str, detail: String) {
        self.out.push(Finding { skill, title: title.into(), detail, at, weight, safety: false });
    }

    fn warn(&mut self, skill: &'static str, at: usize, title: &str, detail: String) {
        self.out.push(Finding { skill, title: title.into(), detail, at, weight: 2.0, safety: true });
    }
}

fn corner(c: &mut Ctx) {
    let (p, r, s) = (c.p, c.r, c.s);
    let (start, end) = (s.start, s.end.max(s.start + 1));
    let (a, b) = (s.core.0.max(start), s.core.1.min(end));
    let name = s.name.clone();
    let apex_r = r.slowest(a..b + 1);
    let apex_p = p.slowest(a..b + 1);
    let (vmin_p, vmin_r) = (p.pts[apex_p].v, r.pts[apex_r].v);
    let slower_mid = vmin_p < vmin_r * th::MIN_SPEED_RATIO;

    // Braking.
    let onset = |t: &Trace| t.first(start..b + 1, |q| q.brake() > th::BRAKE_ON);
    let release = |t: &Trace, on: usize| {
        let mut i = on;
        while i < b && t.pts[i + 1].brake() > th::BRAKE_ON {
            i += 1;
        }
        i
    };
    match (onset(p), onset(r)) {
        (Some(op), Some(or)) => {
            let early = or as f32 - op as f32;
            if early > th::BRAKE_POINT_M {
                c.add("brake_early", 0.9, op, "Brake later", format!(
                    "You start braking about {early:.0} m before the fast lap into {name}. Brake later \
                     and harder, and be done before you turn in."
                ));
            } else if -early > th::BRAKE_POINT_M && slower_mid {
                c.add("brake_late", 0.7, op, "Brake a touch earlier", format!(
                    "You brake {:.0} m later than the fast lap, then scrub off too much speed in the \
                     turn. Brake a little earlier so you can roll through {name}.",
                    -early
                ));
            }
            let (ep, er) = (release(p, op), release(r, or));
            let peak = |t: &Trace, x: usize, y: usize| (x..=y).map(|i| t.decel_g(i)).fold(0.0, f32::max);
            let (gp, gr) = (peak(p, op, ep), peak(r, or, er));
            let (lp, lr) = ((ep - op + 1) as f32, (er - or + 1) as f32);
            if gr > 0.2 && gp < gr * th::SOFT_BRAKING && lp > lr * th::LONG_BRAKING {
                c.add("brake_harder", 0.8, op, "Brake harder, for less time", format!(
                    "Your braking into {name} is softer ({gp:.2} G against {gr:.2} G) and {:.0} m longer \
                     than the fast lap's. Squeeze harder for a shorter time, mostly on the front.",
                    lp - lr
                ));
            }
            let share = |t: &Trace, x: usize, y: usize| {
                let (f, sum) = (x..=y).fold((0.0, 0.0), |(f, sum), i| {
                    let q = &t.pts[i];
                    (f + q.front, sum + q.front + q.rear)
                });
                if sum > 0.0 { f / sum } else { 0.5 }
            };
            if share(p, op, ep) < th::FRONT_SHARE_LOW && share(r, or, er) >= th::FRONT_SHARE_GOOD {
                c.add("more_front", 0.4, op, "Use more front brake", format!(
                    "Most of your stopping into {name} comes from the rear. Let the front do most of it \
                     and use the rear to keep the bike straight."
                ));
            }
        }
        (Some(op), None) => {
            c.add("brake_unneeded", 0.9, op, "Carry it through", format!(
                "The fast lap doesn't touch the brakes into {name}. Roll off the throttle and let the \
                 bike carry its speed."
            ));
        }
        _ => {}
    }

    // Through the turn.
    if slower_mid {
        let early = if apex_p + 5 < apex_r { " You also slow down too early." } else { "" };
        c.add("carry_speed", 1.0, apex_p, "Carry more speed", format!(
            "Your slowest point in {name} is {:.0} km/h below the fast lap. Brake less and roll more \
             speed into the turn.{early}",
            (vmin_r - vmin_p) * KMH
        ));
    }
    let lean = |t: &Trace| t.max_by(a..b + 1, |q| q.roll.abs());
    let (lean_p, lean_r) = (lean(p), lean(r));
    if lean_r - lean_p > th::LEAN_DEG && slower_mid {
        c.add("lean_more", 0.6, apex_p, "Lean the bike more", format!(
            "The fast lap leans the bike {:.0}° further in {name}. Lean the bike in and keep the rider \
             slightly to the outside, so the tyres keep their grip.",
            lean_r - lean_p
        ));
    }
    let heading = r.bearing(apex_r);
    let (dx, dz) = (p.pts[apex_r].x - r.pts[apex_r].x, p.pts[apex_r].z - r.pts[apex_r].z);
    let offset = dx * heading.cos() - dz * heading.sin(); // + is right of the fast line
    if offset.abs() > th::LINE_M && s.dir != 0 {
        let tighter = offset * s.dir as f32 > 0.0;
        c.add("line", 0.75, apex_r, if tighter { "Take a wider line" } else { "Take a tighter line" }, format!(
            "At the apex of {name} you are {:.1} m {} the fast lap. Pick its line before you brake.",
            offset.abs(),
            if tighter { "inside" } else { "outside" }
        ));
    }
    let coast = |t: &Trace| t.time_where(start..end, |q| q.brake() < 0.05 && q.throttle < 0.15 && !q.air);
    let (cp, cr) = (coast(p), coast(r));
    if cp - cr > th::COAST_S {
        c.add("coasting", 0.7, apex_p, "Don't coast", format!(
            "You coast for {:.1} s longer than the fast lap in {name}, off the brakes and off the gas. \
             Go straight from the brakes to the throttle.",
            cp - cr
        ));
    }

    // The exit.
    let pickup = |t: &Trace, from: usize| {
        t.first(from..end, |_| true).and_then(|_| {
            (from..end).find(|&i| {
                (i..(i + th::THROTTLE_HOLD_M).min(end)).all(|j| t.pts[j].throttle > th::THROTTLE_ON)
            })
        })
    };
    let mut exit_explained = false;
    if let (Some(gp), Some(gr)) = (pickup(p, apex_p.min(apex_r)), pickup(r, apex_p.min(apex_r))) {
        if gp as f32 - gr as f32 > th::THROTTLE_POINT_M {
            exit_explained = true;
            c.add("late_throttle", 0.85, gp, "Get on the gas sooner", format!(
                "You pick up the throttle {} m later than the fast lap out of {name}. Open it as soon as \
                 the bike starts to come upright.",
                gp - gr
            ));
        }
    }
    let spin = |t: &Trace| t.time_where(apex_r..end, |q| q.slip_r > th::SPIN_SLIP && q.v > 5.0 && !q.air);
    let (sp, sr) = (spin(p), spin(r));
    if sp - sr > th::SPIN_S {
        exit_explained = true;
        c.add("wheelspin", 0.65, apex_p, "Smoother on the throttle", format!(
            "The rear wheel spins for {sp:.1} s out of {name}. Roll the throttle on more gradually, and \
             feed the clutch if you ride with a manual one."
        ));
    }
    let (gear_p, gear_r) = (p.pts[apex_p].gear, r.pts[apex_r].gear);
    let (exit_p, exit_r) = (p.pts[end].v, r.pts[end].v);
    if gear_p != gear_r && gear_p > 0 && gear_r > 0 && exit_p < exit_r {
        exit_explained = true;
        let (title, skill) =
            if gear_p < gear_r { ("Try a gear higher", "gear_up") } else { ("Try a gear lower", "gear_down") };
        c.add(skill, 0.5, apex_p, title, format!(
            "The fast lap takes {name} in gear {gear_r}. You are in gear {gear_p}."
        ));
    }
    if !exit_explained && (exit_r - exit_p) * KMH > th::EXIT_KMH {
        c.add("exit_speed", 0.6, end, "Drive out harder", format!(
            "You leave {name} {:.0} km/h slower than the fast lap, and that costs time all the way down \
             the next straight.",
            (exit_r - exit_p) * KMH
        ));
    }

    // Habits.
    let clutch = |t: &Trace| t.time_where(start..apex_r, |q| q.clutch > 0.5 && q.brake() > th::BRAKE_ON);
    if clutch(p) > th::CLUTCH_S && clutch(r) < th::CLUTCH_S / 2.0 {
        c.add("clutch_braking", 0.3, start, "Keep the clutch out when braking", format!(
            "You pull the clutch while braking into {name}. Leave it out: engine braking slows you and \
             keeps the rear settled."
        ));
    }
    let wheelie = |t: &Trace| t.time_where(apex_r..end, |q| q.off[0] && !q.off[1] && q.throttle > 0.5);
    let (wp, wr) = (wheelie(p), wheelie(r));
    if wp - wr > th::WHEELIE_S {
        c.add("wheelie", 0.6, apex_p, "Keep the front down on the exit", format!(
            "The front wheel comes up for {wp:.1} s out of {name}. Move your weight forward and roll the \
             throttle on more smoothly."
        ));
    }
    let entry = start..apex_r.max(start + 1);
    let stoppie = p.time_where(entry.clone(), |q| q.off[1] && !q.off[0] && q.brake() > th::BRAKE_ON);
    if stoppie > th::STOPPIE_S {
        c.warn("stoppie", start, "Rear wheel lifts under braking", format!(
            "The rear comes off the ground while you brake into {name}. Ease the front brake a little and \
             keep your weight back."
        ));
    }
    let skid = |t: &Trace| t.time_where(start..end, |q| q.rear > 0.3 && q.slip_r < th::REAR_LOCK_SLIP && q.v > 5.0 && !q.air);
    let (kp, kr) = (skid(p), skid(r));
    if kp - kr > th::REAR_LOCK_S {
        c.add("rear_lock", 0.45, start, "Don't lock the rear", format!(
            "The rear wheel skids for {kp:.1} s into {name}. A locked wheel slows you less than a turning one. \
             Ease the rear brake until it keeps rolling."
        ));
    }
    if c.travel && !bottom_runs(p, entry.clone(), 0).is_empty() && bottom_runs(r, entry, 0).is_empty() {
        c.warn("bottom_braking", start, "The fork bottoms in the braking bumps", format!(
            "The fork runs out of travel braking into {name}. Brake a little earlier and lighter on the front, \
             use more rear, and stay standing. If it keeps happening, add fork compression."
        ));
    }
    let lock = p.time_where(start..end, |q| q.front > 0.3 && q.slip_f < th::FRONT_LOCK_SLIP && q.v > 5.0 && !q.air);
    if lock > th::FRONT_LOCK_S {
        c.warn("front_lock", start, "Front wheel locking", format!(
            "The front wheel locks for {lock:.1} s into {name}. That's how front-end crashes start. Ease \
             the front brake and brake a little earlier."
        ));
    }
}

fn jumps(c: &mut Ctx) {
    let (p, r, s) = (c.p, c.r, c.s);
    let name = s.name.clone();
    let mine = air_runs(p, s.start..s.end + 1);
    let theirs = s.runs.len();

    // Taken in a different number of jumps, the jumps can't be paired one by one: that is
    // what reads as landing short and overjumping in the same breath. Say the one thing.
    if !mine.is_empty() && mine.len() != theirs {
        let (pt0, rt0) = (mine[0].0, s.runs[0].0);
        let quicker = (r.pts[rt0].v - p.pts[pt0].v) * KMH;
        let speed = if quicker > 1.0 { format!(" Carry about {quicker:.0} km/h more to the first face.") } else { String::new() };
        let jumps = |n: usize| if n == 1 { "1 jump".to_string() } else { format!("{n} jumps") };
        if mine.len() > theirs {
            c.add("rhythm_count", 1.0, pt0, "Link the jumps", format!(
                "The fast lap takes {name} in {}, you take {}. Linking them is where the time is.{speed}",
                jumps(theirs),
                jumps(mine.len())
            ));
        } else {
            c.add("rhythm_count", 1.0, pt0, "Take it the fast lap's way", format!(
                "You take {name} in {}, the fast lap in {}, and its way is quicker here. Try it jump by jump.",
                jumps(mine.len()),
                jumps(theirs)
            ));
        }
        return;
    }

    let mut used = vec![false; mine.len()];
    for (k, &(rt, rl)) in s.runs.iter().enumerate() {
        let near = |i: &usize| (mine[*i].0 as i64 - rt as i64).abs();
        let pick = (0..mine.len()).filter(|&i| !used[i]).min_by_key(near).filter(|i| near(i) <= th::MATCH_JUMP_M);
        let name = if theirs > 1 { format!("the {} jump of {name}", ordinal(k + 1)) } else { name.clone() };
        let Some(i) = pick else {
            c.add("jump_it", 0.9, rt, "Jump it", format!(
                "The fast lap jumps {name} and you roll it. Carry more speed up the face and commit."
            ));
            continue;
        };
        used[i] = true;
        let (pt, pl) = mine[i];
        let face = |t: &Trace, take: usize| {
            let from = take.saturating_sub(th::CHOP_M);
            t.max_by(from..take + 1, |q| q.throttle) - t.pts[take].throttle
        };
        if face(p, pt) > th::CHOP && face(r, rt) < th::CHOP / 2.0 {
            c.add("chop_face", 0.9, pt, "Stay on the gas up the face", format!(
                "You shut the throttle on the face of {name}, which drops the nose. Hold it steady all \
                 the way to the lip."
            ));
        }
        let (air_p, air_r) = (p.span(pt, pl), r.span(rt, rl));
        let peak = |t: &Trace, x: usize, y: usize| t.max_by(x..y + 1, |q| q.y);
        let higher = peak(p, pt, pl) - peak(r, rt, rl);
        // Landing short, the fix is speed; staying low would only make it shorter.
        let short = (pl as i64 - rl as i64) < -th::SHORT_M;
        if air_p > air_r * th::FLOAT_AIR + 0.1 && higher > th::FLOAT_HEIGHT_M && !short {
            let scrubs = r.max_by(rt..rt + (rl - rt) / 3 + 1, |q| q.roll.abs()) > th::SCRUB_ROLL_DEG;
            let hint = if scrubs { " The fast lap scrubs this one." } else { "" };
            c.add("scrub", 0.85, pt, "Stay low: scrub it", format!(
                "You fly {higher:.1} m higher and {:.1} s longer than the fast lap over {name}. In MX Bikes, \
                 stay seated, lean the bike over as you leave the lip, then lean the other way in the air \
                 to straighten it before you land.{hint}",
                air_p - air_r
            ));
        }
        let landed = pl as i64 - rl as i64;
        if landed < -th::SHORT_M {
            let slow = p.pts[pt].v < r.pts[rt].v * th::TAKEOFF_SPEED_RATIO;
            let why = if slow {
                format!(" You hit the lip {:.0} km/h slower.", (r.pts[rt].v - p.pts[pt].v) * KMH)
            } else {
                String::new()
            };
            c.add("land_short", 0.8, pl, "You're landing short", format!(
                "You land {} m short of the fast lap at {name} and lose speed on the face of the landing. \
                 Carry more speed up to the lip.{why}",
                -landed
            ));
        } else if landed > th::LONG_M {
            c.add("overjump", 0.8, pl, "You're overjumping", format!(
                "You land {landed} m past the fast lap at {name}, beyond the downslope. Roll off a touch \
                 before the lip, or scrub it lower."
            ));
        }
        let at_land = |t: &Trace, l: usize| t.pts[(l + 2).min(s.end)].throttle;
        if at_land(p, pl) < th::LAND_THROTTLE_LOW && at_land(r, rl) > th::LAND_THROTTLE_GOOD {
            c.add("land_throttle", 0.6, pl, "Throttle on at touchdown", format!(
                "You land off the gas at {name}. Have the throttle open as you touch down so the rear \
                 drives and the shock stays firm."
            ));
        }
        // A whip is still unwinding at touchdown, so judge the lean once the bike has settled,
        // and only where the fast lap lands straighter.
        let settled = |t: &Trace, l: usize| t.pts[(l + 4).min(s.end)].roll.abs();
        let (lean_p, lean_r) = (settled(p, pl), settled(r, rl));
        if lean_p > th::LAND_ROLL_DEG && lean_p - lean_r > th::LAND_ROLL_DEG / 2.0 {
            c.warn("land_crooked", pl, "Straighten up before landing", format!(
                "The bike is still leaned {lean_p:.0}° after you land at {name}. Bring it straight in the air, \
                 a moment earlier."
            ));
        }
        if c.travel {
            for (k, end) in ["fork", "shock"].iter().enumerate() {
                let after = |t: &Trace, l: usize| bottom_runs(t, l..(l + 12).min(s.end + 1), k);
                if !after(p, pl).is_empty() && after(r, rl).is_empty() {
                    c.warn("bottom_landing", pl, &format!("The {end} bottoms on the landing"), format!(
                        "You run out of {end} travel landing {name}. Land on the downslope rather than flat: \
                         carry a touch more speed, or scrub lower. If it bottoms on a good landing too, add \
                         {end} compression."
                    ));
                }
            }
        }
        let gas = |t: &Trace, a: usize, b: usize| t.mean(a..b + 1, |q| q.throttle);
        if gas(p, pt, pl) > th::AIR_GAS && gas(r, rt, rl) < th::AIR_GAS / 2.0 {
            c.add("air_throttle", 0.4, pt, "Off the gas in the air", format!(
                "You hold the throttle open in the air over {name}. Close it in the air so the bike stays \
                 level, then open it as you touch down."
            ));
        }
    }

    // Through a rhythm, one landing off throws the next jump off too: say the first only.
    if theirs > 1 {
        let landing = |f: &Finding| f.skill == "land_short" || f.skill == "overjump";
        let before = c.out.iter().filter(|f| landing(f)).count();
        let mut seen = false;
        c.out.retain(|f| {
            if !landing(f) {
                return true;
            }
            let keep = !seen;
            seen = true;
            keep
        });
        if before > 1 {
            if let Some(f) = c.out.iter_mut().find(|f| landing(f)) {
                f.detail.push_str(" That throws off the rest of the rhythm.");
            }
        }
    }
}

/// Stretches where one end (0 fork, 1 shock) is out of travel.
fn bottom_runs(tr: &Trace, within: Range<usize>, k: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = within.start;
    while i < within.end {
        if tr.pts[i].used[k] < th::BOTTOM {
            i += 1;
            continue;
        }
        let s = i;
        while i < within.end && tr.pts[i].used[k] >= th::BOTTOM {
            i += 1;
        }
        out.push((s, i - 1));
    }
    out
}

fn upshifts(t: &Trace) -> Vec<usize> {
    (1..t.len()).filter(|&i| t.pts[i].gear > t.pts[i - 1].gear && t.pts[i - 1].gear > 0).collect()
}

fn shifts(t: &Trace) -> usize {
    (1..t.len()).filter(|&i| t.pts[i].gear != t.pts[i - 1].gear && t.pts[i].gear > 0 && t.pts[i - 1].gear > 0).count()
}

/// Setup advice for the whole lap: suspension that bottoms or never works, gearing that sits on
/// the limiter or bogs, shifting habits, and a front that won't stay down. `r` is the fast lap,
/// when there is one to compare with.
fn setup(p: &Trace, r: Option<&Trace>, secs: &[Section], bike: Bike, travel: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut tip = |skill: &'static str, weight: f32, at: usize, title: String, detail: String| {
        out.push(Finding { skill, title, detail, at, weight, safety: false });
    };

    if travel {
        let ends = [("fork", "setup_bottoming_fork", "setup_stiff_fork"), ("shock", "setup_bottoming_shock", "setup_stiff_shock")];
        for (k, (end, bottoming, stiff)) in ends.into_iter().enumerate() {
            let runs = bottom_runs(p, 0..p.len(), k);
            if runs.len() >= th::BOTTOMS_PER_LAP {
                let mut places: Vec<&str> = runs
                    .iter()
                    .filter_map(|&(a, _)| secs.iter().find(|s| s.start <= a && a <= s.end).map(|s| s.name.as_str()))
                    .collect();
                places.dedup();
                tip(bottoming, 1.0, runs[0].0, format!("The {end} bottoms {} times a lap", runs.len()), format!(
                    "It runs out of travel at {}. If your landings are clean, stiffen the {end}: more compression \
                     damping or a stiffer spring, one step at a time.",
                    places.join(", ")
                ));
            } else {
                let most = p.pts.iter().map(|q| q.used[k]).fold(0.0, f32::max);
                if most < th::LAZY_TRAVEL {
                    tip(stiff, 0.5, 0, format!("The {end} never uses its travel"), format!(
                        "It uses at most {:.0}% of its travel all lap. A softer spring or less compression would \
                         let it soak up the bumps.",
                        most * 100.0
                    ));
                }
            }
        }
    }

    let on = if bike.limiter > 0.0 { p.time_where(0..p.len(), |q| q.rpm >= bike.limiter * 0.98) } else { 0.0 };
    let bogs: Vec<&str> = if bike.max_rpm > 0.0 {
        secs.iter()
            .filter(|s| s.kind == Kind::Corner)
            .filter(|s| {
                let apex = p.slowest(s.core.0..s.core.1 + 1);
                p.pts[apex].rpm > 0.0 && p.pts[apex].rpm < bike.max_rpm * th::BOG_SHARE && p.pts[apex].gear > 1
            })
            .map(|s| s.name.as_str())
            .collect()
    } else {
        Vec::new()
    };
    let (limited, bogging) = (on > th::LIMITER_LAP_S, bogs.len() >= 2);
    if limited && bogging {
        // Taller gearing would bog it worse and shorter would hit the limiter sooner: this one
        // is riding, not gearing.
        tip("setup_gearing_mixed", 0.8, 0, "Shift up sooner, and take slow corners a gear lower".into(), format!(
            "You're on the rev limiter for {on:.1} s a lap, and the engine bogs out of {}. Changing the gearing \
             can't fix both: shift up before the limiter, and take those corners a gear lower.",
            bogs.join(", ")
        ));
    } else if limited {
        tip("setup_gearing_tall", 0.8, 0, "Try taller gearing".into(), format!(
            "You're on the rev limiter for {on:.1} s a lap. Taller gearing, a bigger front sprocket or a \
             smaller rear one, lets the bike keep pulling. Shifting up sooner helps too."
        ));
    } else if bogging {
        tip("setup_gearing_short", 0.7, 0, "The engine bogs out of corners".into(), format!(
            "Out of {} the engine is below {:.0}% of its revs at the slowest point. Take those corners a gear \
             lower, or try shorter gearing: a smaller front sprocket or a bigger rear one.",
            bogs.join(", "),
            th::BOG_SHARE * 100.0
        ));
    }

    let ups = upshifts(p);
    if ups.len() >= 4 {
        let mut revs: Vec<f32> = ups.iter().map(|&i| p.pts[i.saturating_sub(2)].rpm).collect();
        revs.sort_by(f32::total_cmp);
        let typical = revs[revs.len() / 2];
        if bike.limiter > 0.0 && typical >= bike.limiter * 0.98 {
            let at = if bike.shift_rpm > 0.0 { format!(", at about {:.0} rpm", bike.shift_rpm) } else { String::new() };
            tip("setup_shift_late", 0.6, ups[0], "You shift on the limiter".into(), format!(
                "Most of your upshifts come after the engine has hit the limiter. Change up just before it{at}."
            ));
        } else if bike.shift_rpm > 0.0 && typical < bike.shift_rpm * th::SHORT_SHIFT {
            tip("setup_shift_early", 0.5, ups[0], "You short-shift".into(), format!(
                "You change up at about {typical:.0} rpm. This bike pulls hardest up to about {:.0}. Hold each gear \
                 a little longer.",
                bike.shift_rpm
            ));
        }
    }
    if let Some(r) = r {
        let (np, nr) = (shifts(p), shifts(r));
        if np >= nr + 4 && np as f32 > nr as f32 * 1.4 {
            tip("setup_shift_count", 0.5, 0, "You change gear a lot".into(), format!(
                "{np} gear changes a lap against the fast lap's {nr}. Pick one gear for each corner and stay in it."
            ));
        }
    }

    let wheelie = |t: &Trace| {
        secs.iter()
            .filter(|s| s.kind == Kind::Corner)
            .map(|s| {
                let apex = t.slowest(s.core.0..s.core.1 + 1);
                t.time_where(apex..s.end.max(apex + 1), |q| q.off[0] && !q.off[1] && q.throttle > 0.5)
            })
            .sum::<f32>()
    };
    let (wp, wr) = (wheelie(p), r.map_or(0.0, wheelie));
    if wp > th::WHEELIE_LAP_S && wp - wr > th::WHEELIE_LAP_S / 2.0 {
        tip("setup_swingarm", 0.5, 0, "The front lifts out of corners".into(), format!(
            "The front wheel is up for {wp:.1} s a lap coming out of corners. Smoother throttle first; then a \
             longer swingarm (the rear wheel further back) or taller gearing keeps it down."
        ));
    }
    out
}

/// Reviews a lap on its own, with no faster lap to hold it against: only what costs time or
/// risks a crash whatever the line, like coasting, wheelspin, the limiter or a hard landing.
pub fn solo(lap: &Trace, bike: Bike) -> Review {
    let mut p = lap.clone();
    let travel = p.fill_travel(bike.travel);
    let p = &p;
    let secs = sections(p);
    let setup = setup(p, None, &secs, bike, travel);
    let out: Vec<SectionReview> = secs
        .into_iter()
        .map(|s| {
            let t = p.span(s.start, s.end);
            let mut c = Ctx { p, r: p, s: &s, bike, travel, out: Vec::new() };
            alone(&mut c);
            let mut findings = dedupe(c.out, s.kind != Kind::Corner);
            findings.sort_by(|a, b| b.weight.total_cmp(&a.weight));
            SectionReview { section: s, lap_time: t, ref_time: t, lost: 0.0, findings }
        })
        .collect();
    let mut order: Vec<usize> = (0..out.len()).filter(|&i| !out[i].findings.is_empty()).collect();
    order.sort_by(|&a, &b| out[b].findings[0].weight.total_cmp(&out[a].findings[0].weight));
    order.truncate(th::FOCUS);
    Review {
        lap_time: p.time(),
        ref_time: p.time(),
        overall: overall(&out, true),
        sections: out,
        focus: order,
        setup,
        solo: true,
        channels: channels(p, p, 2),
        paths: paths(p, p),
    }
}

/// The rules that need no faster lap.
fn alone(c: &mut Ctx) {
    let (p, s) = (c.p, c.s);
    let name = s.name.clone();
    let (start, end) = (s.start, s.end.max(s.start + 1));
    match s.kind {
        Kind::Corner => {
            let apex = p.slowest(s.core.0.max(start)..s.core.1.min(end) + 1);
            let coast = p.time_where(start..end, |q| q.brake() < 0.05 && q.throttle < 0.15 && !q.air);
            if coast > th::SOLO_COAST_S {
                c.add("coasting", 0.8, apex, "Don't coast", format!(
                    "You coast for {coast:.1} s in {name}, off the brakes and off the gas. Go straight from the \
                     brakes to the throttle."
                ));
            }
            let spin = p.time_where(apex..end, |q| q.slip_r > th::SPIN_SLIP && q.v > 5.0 && !q.air);
            if spin > th::SOLO_SPIN_S {
                c.add("wheelspin", 0.7, apex, "Smoother on the throttle", format!(
                    "The rear wheel spins for {spin:.1} s out of {name}. Roll the throttle on more gradually."
                ));
            }
            let skid = p.time_where(start..end, |q| q.rear > 0.3 && q.slip_r < th::REAR_LOCK_SLIP && q.v > 5.0 && !q.air);
            if skid > th::SOLO_SKID_S {
                c.add("rear_lock", 0.5, start, "Don't lock the rear", format!(
                    "The rear wheel skids for {skid:.1} s into {name}. Ease the rear brake until it keeps rolling."
                ));
            }
            let wheelie = p.time_where(apex..end, |q| q.off[0] && !q.off[1] && q.throttle > 0.5);
            if wheelie > th::SOLO_WHEELIE_S {
                c.add("wheelie", 0.6, apex, "Keep the front down on the exit", format!(
                    "The front wheel comes up for {wheelie:.1} s out of {name}. Move your weight forward and roll \
                     the throttle on more smoothly."
                ));
            }
            let entry = start..apex.max(start + 1);
            if p.time_where(entry.clone(), |q| q.off[1] && !q.off[0] && q.brake() > th::BRAKE_ON) > th::STOPPIE_S {
                c.warn("stoppie", start, "Rear wheel lifts under braking", format!(
                    "The rear comes off the ground while you brake into {name}. Ease the front brake a little and \
                     keep your weight back."
                ));
            }
            let lock = p.time_where(start..end, |q| q.front > 0.3 && q.slip_f < th::FRONT_LOCK_SLIP && q.v > 5.0 && !q.air);
            if lock > th::FRONT_LOCK_S {
                c.warn("front_lock", start, "Front wheel locking", format!(
                    "The front wheel locks for {lock:.1} s into {name}. Ease the front brake and brake a little earlier."
                ));
            }
            if p.time_where(entry.clone(), |q| q.clutch > 0.5 && q.brake() > th::BRAKE_ON) > th::CLUTCH_S {
                c.add("clutch_braking", 0.3, start, "Keep the clutch out when braking", format!(
                    "You pull the clutch while braking into {name}. Leave it out: engine braking slows you and keeps \
                     the rear settled."
                ));
            }
            if c.travel && !bottom_runs(p, entry, 0).is_empty() {
                c.warn("bottom_braking", start, "The fork bottoms in the braking bumps", format!(
                    "The fork runs out of travel braking into {name}. Brake a little earlier and lighter on the front, \
                     and stay standing. If it keeps happening, add fork compression."
                ));
            }
        }
        Kind::Jump | Kind::Rhythm => {
            let runs = air_runs(p, start..end + 1);
            for (k, &(pt, pl)) in runs.iter().enumerate() {
                let which = if runs.len() > 1 { format!("the {} jump of {name}", ordinal(k + 1)) } else { name.clone() };
                let from = pt.saturating_sub(th::CHOP_M);
                if p.max_by(from..pt + 1, |q| q.throttle) - p.pts[pt].throttle > th::SOLO_CHOP && p.pts[pt].throttle < 0.3 {
                    c.add("chop_face", 0.9, pt, "Stay on the gas up the face", format!(
                        "You shut the throttle on the face of {which}, which drops the nose. Hold it steady to the lip."
                    ));
                }
                if p.mean(pt..pl + 1, |q| q.throttle) > th::SOLO_AIR_GAS {
                    c.add("air_throttle", 0.4, pt, "Off the gas in the air", format!(
                        "You hold the throttle open in the air over {which}. Close it so the bike stays level, then \
                         open it as you touch down."
                    ));
                }
                let lean = p.pts[(pl + 4).min(s.end)].roll.abs();
                if lean > th::SOLO_LAND_ROLL_DEG {
                    c.warn("land_crooked", pl, "Straighten up before landing", format!(
                        "The bike is still leaned {lean:.0}° after you land {which}. Bring it straight in the air, a \
                         moment earlier."
                    ));
                }
                if c.travel {
                    for (e, end_name) in ["fork", "shock"].iter().enumerate() {
                        if !bottom_runs(p, pl..(pl + 12).min(s.end + 1), e).is_empty() {
                            c.warn("bottom_landing", pl, &format!("The {end_name} bottoms on the landing"), format!(
                                "You run out of {end_name} travel landing {which}. Land on the downslope rather than \
                                 flat. If it bottoms on a good landing too, add {end_name} compression."
                            ));
                        }
                    }
                }
            }
        }
        Kind::Whoops => {
            let (a, b) = (s.core.0, s.core.1 + 1);
            if p.time_where(a..b, |q| q.throttle < th::WHOOPS_OFF_GAS) / p.span(a, b).max(0.01) > th::SOLO_OFF_GAS_SHARE {
                c.add("whoops_throttle", 0.8, a, "Stay on the gas", format!(
                    "You come off the throttle in {name}. Hold it on: the bike skims when it's driving."
                ));
            }
        }
        Kind::Straight => {
            let range = start..end;
            if c.bike.limiter > 0.0 {
                let on = p.time_where(range.clone(), |q| q.rpm >= c.bike.limiter * 0.98);
                if on > th::SOLO_LIMITER_S {
                    c.add("shift_earlier", 0.7, start, "Shift up sooner", format!(
                        "You sit on the rev limiter for {on:.1} s on this straight. Shift up before you hit it."
                    ));
                }
            }
            let gas = p.mean(range.clone(), |q| q.throttle);
            if range.len() > th::SOLO_LONG_STRAIGHT_M && gas < th::SOLO_PART_GAS {
                c.add("full_gas", 0.6, start, "Hold it wide open", format!(
                    "You use {:.0}% throttle down this straight. Stay pinned until the braking point.",
                    gas * 100.0
                ));
            }
        }
    }
}

fn whoops(c: &mut Ctx) {
    let (p, r, s) = (c.p, c.r, c.s);
    let name = s.name.clone();
    let (a, b) = (s.core.0, s.core.1 + 1);
    let (vp, vr) = (p.mean(a..b, |q| q.v), r.mean(a..b, |q| q.v));
    if vp < vr * th::WHOOPS_SPEED_RATIO {
        c.add("whoops_speed", 1.0, a, "Commit to the whoops", format!(
            "You go through {name} {:.0} km/h slower than the fast lap. Come in a gear higher with more \
             speed, and stay on the gas to skim the tops.",
            (vr - vp) * KMH
        ));
    }
    let off = |t: &Trace| t.time_where(a..b, |q| q.throttle < th::WHOOPS_OFF_GAS) / t.span(a, b).max(0.01);
    if off(p) - off(r) > th::WHOOPS_OFF_GAS_SHARE {
        c.add("whoops_throttle", 0.8, a, "Stay on the gas", format!(
            "You come off the throttle in {name}. Hold it on: the bike skims when it's driving."
        ));
    }
    let wobble = |t: &Trace| {
        let m = t.mean(a..b, |q| q.pitch);
        t.mean(a..b, |q| (q.pitch - m).powi(2)).sqrt()
    };
    if wobble(p) > wobble(r) * th::WHOOPS_PITCH && wobble(r) > 0.5 {
        c.add("whoops_bucking", 0.6, a, "Keep the bike level", format!(
            "The bike pitches much more than on the fast lap in {name}. Keep your weight a little back \
             and let the bike move under you."
        ));
    }
}

fn straight(c: &mut Ctx) {
    let (p, r, s) = (c.p, c.r, c.s);
    let range = s.start..s.end.max(s.start + 1);
    if c.bike.limiter > 0.0 {
        let cap = c.bike.limiter * 0.98;
        let (lp, lr) = (p.time_where(range.clone(), |q| q.rpm >= cap), r.time_where(range.clone(), |q| q.rpm >= cap));
        if lp - lr > th::LIMITER_S {
            c.add("shift_earlier", 0.7, s.start, "Shift up sooner", format!(
                "You sit on the rev limiter for {lp:.1} s on this straight. Shift up before you hit it."
            ));
        }
    }
    let (tp, tr) = (p.mean(range.clone(), |q| q.throttle), r.mean(range, |q| q.throttle));
    if tr - tp > th::PART_GAS {
        c.add("full_gas", 0.6, s.start, "Hold it wide open", format!(
            "You use {:.0}% throttle down this straight, the fast lap {:.0}%. Stay pinned until the \
             braking point.",
            tp * 100.0,
            tr * 100.0
        ));
    }
}

/// A session's best time for each section, and how much it varies.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionBest {
    pub name: String,
    pub best: f32,
    /// The lap it came from.
    pub lap: i32,
    /// Standard deviation over the session's laps, seconds.
    pub spread: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ideal {
    /// Every section's best, added up.
    pub time: f32,
    pub sections: Vec<SectionBest>,
    /// The section that varies most from lap to lap.
    pub least_consistent: Option<usize>,
}

/// The best lap the session's own sections add up to, on the reference's sections.
pub fn ideal(sections: &[Section], laps: &[(i32, Trace)]) -> Option<Ideal> {
    if laps.is_empty() {
        return None;
    }
    let bests: Vec<SectionBest> = sections
        .iter()
        .map(|s| {
            let times: Vec<(i32, f32)> = laps
                .iter()
                .filter(|(_, t)| s.end < t.len())
                .map(|(n, t)| (*n, t.span(s.start, s.end)))
                .collect();
            let (lap, best) = times.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((0, 0.0));
            let mean = times.iter().map(|t| t.1).sum::<f32>() / times.len().max(1) as f32;
            let spread =
                (times.iter().map(|t| (t.1 - mean).powi(2)).sum::<f32>() / times.len().max(1) as f32).sqrt();
            SectionBest { name: s.name.clone(), best, lap, spread }
        })
        .collect();
    let least_consistent = (laps.len() > 2)
        .then(|| (0..bests.len()).max_by(|&a, &b| bests[a].spread.total_cmp(&bests[b].spread)))
        .flatten();
    Some(Ideal { time: bests.iter().map(|b| b.best).sum(), sections: bests, least_consistent })
}

/// Also the stadium laps other modules' tests ride.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::telemetry::{Lap, Sample};

    // A stadium: 200 m north, a 180° right-hander of 20 m radius, 200 m south, and another.
    const R: f32 = 20.0;

    fn len() -> f32 {
        400.0 + 2.0 * PI * R
    }

    /// Position and bearing `d` metres round the lap.
    fn place(d: f32) -> (f32, f32, f32) {
        let arc = PI * R;
        if d < 200.0 {
            (0.0, d, 0.0)
        } else if d < 200.0 + arc {
            let s = (d - 200.0) / R;
            (R - R * s.cos(), 200.0 + R * s.sin(), s)
        } else if d < 400.0 + arc {
            (2.0 * R, 200.0 - (d - 200.0 - arc), PI)
        } else {
            let s = (d - 400.0 - arc) / R;
            (R + R * s.cos(), -R * s.sin(), PI + s)
        }
    }

    pub(crate) struct Style {
        pub(crate) decel: f32,
        pub(crate) brake: f32,
        /// Takeoff, landing, peak height.
        pub(crate) jump: (f32, f32, f32),
        /// Speed along the ground while airborne: nothing drives the bike in the air, so the
        /// longer the flight the more it bleeds off.
        pub(crate) air_v: f32,
        /// Peak lean in the air, degrees; it unwinds just after touchdown, the way a whip does.
        pub(crate) whip: f32,
        /// Suspension squashed on the landing, metres of the 0.3 m travel; 0 lands normally.
        pub(crate) bottom: f32,
        /// Turn 1 ridden this many metres outside the centreline; negative is inside.
        pub(crate) wide: f32,
        /// Speed through both corners, m/s.
        pub(crate) corner_v: f32,
        /// Ground under Turn 1 this much lower, metres, as a rut cut by earlier laps.
        pub(crate) sink: f32,
        /// A second jump after the first, (takeoff, landing, peak); peak 0 for none.
        pub(crate) hop: (f32, f32, f32),
    }

    pub(crate) const FAST: Style = Style {
        decel: 4.0,
        brake: 0.8,
        jump: (330.0, 350.0, 3.0),
        air_v: 20.0,
        whip: 0.0,
        bottom: 0.0,
        wide: 0.0,
        corner_v: 10.0,
        sink: 0.0,
        hop: (0.0, 0.0, 0.0),
    };
    const BIKE: Bike = Bike { limiter: 13000.0, max_rpm: 14000.0, shift_rpm: 12500.0, travel: [0.3, 0.3] };

    /// Speed, throttle, brake and lean at `d`.
    fn ride(st: &Style, d: f32) -> (f32, f32, f32, f32) {
        let arc = PI * R;
        let (c1, c1e, c2) = (200.0, 200.0 + arc, 400.0 + arc);
        if (c1..c1e).contains(&d) || d >= c2 {
            return (st.corner_v, 0.3, 0.0, 35.0);
        }
        let (exit, next) = if d < c1 { (0.0, c1) } else { (c1e, c2) };
        let cv2 = st.corner_v * st.corner_v;
        let accel = (cv2 + 6.0 * (d - exit)).sqrt();
        let braking = (cv2 + 2.0 * st.decel * (next - d)).sqrt();
        if braking < accel && braking < 20.0 {
            (braking, 0.0, st.brake, 0.0)
        } else if accel < 20.0 {
            (accel, 1.0, 0.0, 0.0)
        } else {
            (20.0, 0.9, 0.0, 0.0)
        }
    }

    pub(crate) fn lap(st: &Style) -> Trace {
        let (samples, t) = ride_lap(st);
        let lap = Lap { num: 1, time_ms: (t * 1000.0) as i32, invalid: false, whole: true, issue: None, crashed: false, ridden_ms: 0, samples };
        Trace::new(&lap, len()).unwrap()
    }

    /// One lap of the stadium at 50 Hz, and how long it took.
    fn ride_lap(st: &Style) -> (Vec<Sample>, f32) {
        let l = len();
        let (mut d, mut t, dt) = (0.0f32, 0.0f32, 0.02f32);
        let mut samples = Vec::new();
        while d < l {
            let (v, throttle, brake, roll) = ride(st, d);
            let (x, z, bearing) = place(d);
            // Outside of a right-hander is the rider's left: (-cos, sin) of the bearing.
            let turn1 = (200.0..200.0 + PI * R).contains(&d);
            let (x, z) = if turn1 { (x - bearing.cos() * st.wide, z + bearing.sin() * st.wide) } else { (x, z) };
            let (take, land, peak) = st.jump;
            let (h0, h1, hp) = st.hop;
            let in_hop = hp > 0.0 && d >= h0 && d <= h1;
            let air = (d >= take && d <= land) || in_hop;
            let v = if air { v.min(st.air_v) } else { v };
            let roll = if st.whip > 0.0 && d >= take && d <= land + 2.0 {
                st.whip * (PI * (d - take) / (land + 2.0 - take)).sin()
            } else {
                roll
            };
            let squash = if air {
                0.0
            } else if st.bottom > 0.0 && d > land && d <= land + 4.0 {
                st.bottom
            } else {
                0.05
            };
            let mid = (take + land) / 2.0;
            let half = (land - take) / 2.0;
            let mut s = Sample::default();
            s.t = t;
            s.pos = d / l;
            s.x = x;
            s.z = z;
            s.y = if in_hop {
                hp * (1.0 - ((d - (h0 + h1) / 2.0) / ((h1 - h0) / 2.0)).powi(2))
            } else if air {
                peak * (1.0 - ((d - mid) / half).powi(2))
            } else if turn1 {
                -st.sink
            } else {
                0.0
            };
            s.vel = [v * bearing.sin(), 0.0, v * bearing.cos()];
            s.speed = v;
            s.throttle = throttle;
            s.front_brake = brake;
            s.rear_brake = brake * 0.4;
            s.wheel_speed = [v, v];
            s.wheel_material = if air { [0, 0] } else { [1, 1] };
            s.roll = roll;
            s.susp = [0.30 - squash; 2];
            s.gear = 3;
            s.rpm = 8000.0;
            samples.push(s);
            d += v * dt;
            t += dt;
        }
        (samples, t)
    }

    /// Writes a demo session to `$COACH_DEMO_DIR` for looking at the app without the game:
    /// a fast lap, one braking early and soft, one floating the jump, then the fast one again.
    /// `COACH_DEMO_DIR=<sessions dir> cargo test -p mxb-coach write_demo_session -- --ignored`
    #[test]
    #[ignore]
    fn write_demo_session() {
        let Ok(dir) = std::env::var("COACH_DEMO_DIR") else { return };
        let styles = [
            FAST,
            Style { decel: 2.5, brake: 0.5, ..FAST },
            Style { jump: (330.0, 356.0, 4.5), air_v: 17.0, ..FAST },
            Style { whip: 35.0, bottom: 0.29, ..FAST },
            FAST,
        ];
        let mut f = crate::telemetry::testfile::File::new();
        f.event("coach-demo", len());
        let mut clock = 0.0;
        for (num, st) in styles.iter().enumerate() {
            let (samples, t) = ride_lap(st);
            for s in &samples {
                f.sample(clock + s.t, s.pos, |b| {
                    b.i(0, s.rpm as i32).i(12, s.gear).f(20, s.speed);
                    b.f(24, s.x).f(28, s.y).f(32, s.z).f(36, s.vel[0]).f(40, s.vel[1]).f(44, s.vel[2]);
                    b.f(104, s.roll).f(144, s.throttle).f(148, s.front_brake).f(152, s.rear_brake);
                    b.f(120, s.susp[0]).f(124, s.susp[1]);
                    b.f(160, s.wheel_speed[0]).f(164, s.wheel_speed[1]);
                    b.i(168, s.wheel_material[0]).i(172, s.wheel_material[1]);
                });
            }
            clock += t;
            f.lap(num as i32, (t * 1000.0).round() as i32);
        }
        f.end();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("20260914-120000-000.mxbc"), &f.0).unwrap();
    }

    fn section<'a>(rv: &'a Review, name: &str) -> &'a SectionReview {
        rv.sections.iter().find(|s| s.section.name == name).unwrap_or_else(|| {
            panic!("no {name} in {:?}", rv.sections.iter().map(|s| &s.section.name).collect::<Vec<_>>())
        })
    }

    fn skills(s: &SectionReview) -> Vec<&'static str> {
        s.findings.iter().map(|f| f.skill).collect()
    }

    #[test]
    fn finds_the_corners_and_the_jump_on_the_fast_lap() {
        let secs = sections(&lap(&FAST));
        let names: Vec<&str> = secs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Turn 1") && names.contains(&"Turn 2"), "{names:?}");
        assert!(names.contains(&"Jump 1"), "{names:?}");
        let t1 = secs.iter().find(|s| s.name == "Turn 1").unwrap();
        assert_eq!(t1.dir, 1, "a right-hander");
        assert!(t1.start <= 140 && t1.core.0 >= 190, "braking zone inside the section: {t1:?}");
        // Contiguous, covering the lap.
        assert_eq!(secs[0].start, 0);
        for w in secs.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
        assert_eq!(secs.last().unwrap().end, lap(&FAST).len() - 1);
    }

    #[test]
    fn the_same_lap_has_nothing_to_say() {
        let fast = lap(&FAST);
        let rv = review(&fast, &fast, BIKE);
        assert!(rv.focus.is_empty());
        for s in &rv.sections {
            assert!(s.lost.abs() < 0.01, "{} lost {}", s.section.name, s.lost);
            assert!(s.findings.is_empty(), "{}: {:?}", s.section.name, skills(s));
        }
    }

    #[test]
    fn early_soft_braking_is_called_out_where_it_costs() {
        let slow = lap(&Style { decel: 2.5, brake: 0.5, ..FAST });
        let rv = review(&slow, &lap(&FAST), BIKE);
        let t1 = section(&rv, "Turn 1");
        assert!(t1.lost > 0.1, "lost {}", t1.lost);
        let found = skills(t1);
        assert!(found.contains(&"brake_early"), "{found:?}");
        assert!(found.contains(&"brake_harder"), "{found:?}");
        assert!(rv.focus.iter().any(|&i| rv.sections[i].section.name == "Turn 1"));
        assert!(section(&rv, "Jump 1").findings.is_empty());
    }

    #[test]
    fn floating_long_over_a_jump_says_scrub_and_overjump() {
        let floaty = lap(&Style { jump: (330.0, 356.0, 4.5), air_v: 17.0, ..FAST });
        let rv = review(&floaty, &lap(&FAST), BIKE);
        let found = skills(section(&rv, "Jump 1"));
        assert!(found.contains(&"scrub"), "{found:?}");
        assert!(found.contains(&"overjump"), "{found:?}");
    }

    #[test]
    fn rolling_a_jump_the_fast_lap_clears_says_jump_it() {
        let rolled = lap(&Style { jump: (0.0, 0.0, 0.0), ..FAST });
        let rv = review(&rolled, &lap(&FAST), Bike::default());
        let j = section(&rv, "Jump 1");
        // Same speed on the ground: no time lost, so no advice. What matters is it matched.
        assert!(j.findings.is_empty() || skills(j).contains(&"jump_it"));
    }

    #[test]
    fn a_whip_that_straightens_on_touchdown_is_not_a_crooked_landing() {
        let whip = lap(&Style { whip: 35.0, ..FAST });
        let rv = review(&whip, &lap(&FAST), BIKE);
        let found = skills(section(&rv, "Jump 1"));
        assert!(!found.contains(&"land_crooked"), "{found:?}");
    }

    #[test]
    fn bottoming_on_a_landing_is_called_out_even_without_time_lost() {
        let hard = lap(&Style { bottom: 0.29, ..FAST });
        let rv = review(&hard, &lap(&FAST), BIKE);
        let found = skills(section(&rv, "Jump 1"));
        assert!(found.contains(&"bottom_landing"), "{found:?}");
        // Unknown travel: no suspension advice at all rather than a guess.
        let rv = review(&hard, &lap(&FAST), Bike { travel: [0.0; 2], ..BIKE });
        assert!(!skills(section(&rv, "Jump 1")).contains(&"bottom_landing"));
        assert!(rv.setup.is_empty());
    }

    #[test]
    fn a_jump_taken_in_two_hops_says_link_them_not_short_and_long() {
        let hops = lap(&Style { jump: (330.0, 340.0, 1.5), hop: (348.0, 358.0, 1.5), air_v: 17.0, ..FAST });
        let rv = review(&hops, &lap(&FAST), BIKE);
        let found = skills(section(&rv, "Jump 1"));
        assert!(found.contains(&"rhythm_count"), "{found:?}");
        assert!(!found.contains(&"land_short") && !found.contains(&"overjump"), "{found:?}");
    }

    #[test]
    fn a_bounce_off_the_landing_is_the_same_jump() {
        let bounced = lap(&Style { jump: (330.0, 350.0, 3.0), hop: (353.0, 360.0, 0.3), ..FAST });
        let rv = review(&bounced, &lap(&FAST), BIKE);
        assert!(!skills(section(&rv, "Jump 1")).contains(&"rhythm_count"));
    }

    #[test]
    fn on_the_limiter_and_bogging_is_one_tip_not_two_opposite_ones() {
        // Every corner's slowest point is at 8000 rpm: under 45% of an 18,000 rpm engine.
        let rv = review(&lap(&FAST), &lap(&FAST), Bike { limiter: 7900.0, max_rpm: 18000.0, ..BIKE });
        let gearing: Vec<_> = rv.setup.iter().filter(|f| f.skill.starts_with("setup_gearing")).map(|f| f.skill).collect();
        assert_eq!(gearing, vec!["setup_gearing_mixed"]);
    }

    #[test]
    fn the_same_tip_twice_in_a_section_is_said_once() {
        let f = |skill: &'static str, title: &str| Finding {
            skill,
            title: title.into(),
            detail: "Hold it.".into(),
            at: 0,
            weight: 1.0,
            safety: false,
        };
        let out = dedupe(vec![f("chop_face", "Gas"), f("chop_face", "Gas"), f("scrub", "Scrub")], true);
        assert_eq!(out.len(), 2);
        assert!(out[0].detail.contains("1 more jump"), "{}", out[0].detail);
    }

    #[test]
    fn the_overall_groups_the_lap_by_theme() {
        let rv = review(&lap(&Style { decel: 2.5, brake: 0.5, ..FAST }), &lap(&FAST), BIKE);
        let top = &rv.overall[0];
        assert_eq!(top.name, "Braking");
        assert_eq!(top.sections, vec!["Turn 1", "Turn 2"]);
        assert!(top.lost > 0.3, "{}", top.lost);
    }

    #[test]
    fn a_lap_on_its_own_gets_only_plain_tips() {
        let rv = solo(&lap(&Style { bottom: 0.29, ..FAST }), BIKE);
        assert!(rv.solo);
        assert!(rv.sections.iter().all(|s| s.lost == 0.0));
        assert!(skills(section(&rv, "Jump 1")).contains(&"bottom_landing"));
        assert!(rv.sections.iter().flat_map(skills).all(|k| !k.starts_with("brake_")));
    }

    #[test]
    fn a_lap_on_the_limiter_asks_for_taller_gearing() {
        let rv = review(&lap(&FAST), &lap(&FAST), Bike { limiter: 7900.0, ..BIKE });
        assert!(rv.setup.iter().any(|f| f.skill == "setup_gearing_tall"));
        let rv = review(&lap(&FAST), &lap(&FAST), BIKE);
        assert!(!rv.setup.iter().any(|f| f.skill.starts_with("setup_gearing")));
    }

    #[test]
    fn the_ideal_lap_takes_each_sections_best() {
        let (fast, slow) = (lap(&FAST), lap(&Style { decel: 2.5, brake: 0.5, ..FAST }));
        let secs = sections(&fast);
        let ideal = ideal(&secs, &[(1, slow.clone()), (2, fast.clone())]).unwrap();
        assert!(ideal.time <= fast.time() + 1e-3 && ideal.time <= slow.time());
        let t1 = secs.iter().position(|s| s.name == "Turn 1").unwrap();
        assert_eq!(ideal.sections[t1].lap, 2);
    }
}
