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

use serde::{Deserialize, Serialize};

use crate::telemetry::{Lap, Sample};

/// Grid spacing. Section bounds and finding positions are grid indices, so metres.
pub const STEP_M: f32 = 1.0;
const G: f32 = 9.81;
const KMH: f32 = 3.6;

mod th {
    /// Curved enough to be worth looking at, as a noise gate rather than a definition of a
    /// corner: 100 m radius.
    ///
    /// It used to be 1/45 — tighter than a 45 m radius, held every single metre. Most real
    /// sweepers are 45-100 m, so they could never be corners however far round they went, and
    /// came back as "Straight" because that is simply the leftover where nothing was found.
    /// What makes a corner is how far it turns, which is `CORNER_MIN_DEG`, and that could never
    /// rescue a sweeper because the sum is taken only over metres already past this gate.
    pub const CORNER_CURVATURE: f32 = 1.0 / 100.0;
    pub const CORNER_MIN_DEG: f32 = 35.0;
    /// A gap this long inside a corner doesn't end it. Generous enough to bridge a jump's
    /// flight, and a bump or rut that straightens the bike for a moment.
    pub const CORNER_MERGE_M: usize = 30;
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
    /// How much higher or lower the bike has to be riding, at the same point of the lap, before
    /// it counts as a different line up or down the ground rather than the same one. A berm or
    /// a rut is a good half metre; suspension and the odd bump are not.
    pub const HEIGHT_M: f32 = 0.35;
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

    // Coming in too fast. Braking belongs before turn-in, so speed still leaving the bike after
    // the bike is committed is speed that was carried in. Measured off the brake lever and not
    // the deceleration: sand and a deep rut scrub speed on their own, and these rules cannot see
    // the ground — `soil` is filled in by the caller, after the review returns.
    /// Leaned this far into the core is turn-in: the bike is committed. Not the core's start —
    /// `CORNER_CURVATURE` is a 100 m noise gate, so the core opens long before a rider would
    /// say they had turned in.
    pub const TURN_IN_DEG: f32 = 18.0;
    /// Share of the turn-in speed that may still come off after it, and how much of it in km/h
    /// before it is worth saying at all.
    pub const HOT_INSIDE_SHARE: f32 = 0.30;
    pub const HOT_INSIDE_KMH: f32 = 8.0;
    pub const SOLO_HOT_INSIDE_SHARE: f32 = 0.40;
    /// The lever still held this long past turn-in.
    pub const HOT_BRAKE_S: f32 = 0.25;
    /// And one consequence, or it is only trail-braking into a rut, which is how the corner is
    /// meant to be ridden: the slowest point this far through the core, the bike picked up this
    /// many degrees before the gas, or this far outside the fast line on the way out.
    pub const HOT_APEX_SHARE: f32 = 0.6;
    pub const HOT_STAND_DEG: f32 = 8.0;
    pub const HOT_WIDE_M: f32 = 1.5;
    /// A core shorter than this has no "inside the turn" to speak of.
    pub const HOT_MIN_CORE_M: usize = 15;
    /// Turning in too early: the slowest point this early in the core, and the bike still leaned
    /// with the throttle shut at the end of it.
    pub const APEX_EARLY_SHARE: f32 = 0.35;

    pub const FLOAT_AIR: f32 = 1.1;
    pub const FLOAT_HEIGHT_M: f32 = 0.5;
    /// Thomas's MXBMRP3 waits this long before a bump can become an air trick.
    pub const AIR_COMMIT_S: f32 = 0.3;
    /// MXBMRP3's minimum integrated rotation for both a scrub and a whip. Yaw wins: turning
    /// the bike sideways is a whip, while rolling it (or leaving the lip rolled) is a scrub.
    pub const PARTIAL_ROTATION_DEG: f32 = 30.0;
    /// Enough roll to say the scrub has begun; used only to compare when two riders start it.
    pub const SCRUB_START_DEG: f32 = 10.0;
    pub const SCRUB_START_GAP_S: f32 = 0.1;
    pub const SCRUB_SEATED_SHARE: f32 = 0.6;
    pub const SCRUB_FORWARD_GAP: f32 = 0.25;
    pub const SHORT_M: i64 = 2;
    pub const LONG_M: i64 = 3;
    pub const CHOP: f32 = 0.3;
    pub const CHOP_M: usize = 15;
    pub const LAND_THROTTLE_LOW: f32 = 0.3;
    pub const LAND_THROTTLE_GOOD: f32 = 0.5;
    pub const LAND_ROLL_DEG: f32 = 15.0;
    /// Where a jump was landed, read off the ground the bike runs on once the suspension has
    /// settled: the first metres after touchdown are the shock soaking it up, not the hill.
    pub const LAND_SLOPE_SKIP_M: usize = 2;
    pub const LAND_SLOPE_M: usize = 8;
    /// Ground falling away this steeply — metres down per metre along — is still the downslope;
    /// this flat or flatter is past the bottom of it; rising this much is the up-face. The band
    /// between `LAND_FLAT` and `LAND_DOWN` is deliberately left without a verdict: a shallow
    /// landing is the case where the ground alone cannot settle it.
    ///
    /// Measured against real recordings by `tune_thresholds`: the median landing runs out at
    /// −0.114, so the first value tried here, −0.12, left half of all real landings in the dead
    /// band. −0.07 puts a typical downslope landing on the right side of it. This boundary is
    /// diagnostic only today — `Landing::Ramp` and `Landing::Unsure` both fall through to the
    /// rules that need a fast lap — so moving it changes what the tuning pass reports rather
    /// than what a rider is told.
    pub const LAND_DOWN: f32 = -0.07;
    pub const LAND_FLAT: f32 = -0.04;
    pub const LAND_UP: f32 = 0.06;
    /// Coming down this steeply, gradient again, onto ground that isn't falling away: the bike
    /// drops onto the ground instead of following a ramp down. About 10°.
    pub const FALL_RATE: f32 = -0.18;
    /// A flat landing is over-jumping when it hits this share of the hard-landing floor, so the
    /// rider's own normalisation carries straight over.
    pub const OVERJUMP_HIT_SHARE: f32 = 0.7;
    /// Shorter flights are a hop off a bump, and the ground either side of one says nothing.
    pub const OVERJUMP_AIR_S: f32 = 0.4;
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

    // Suspension speed, acceleration and the bars. Set from real laps (2026-09-15, five laps of
    // a 250F): a landing's hit has a median of 5 G, the bars into a corner about 17.
    /// A landing harder than this, G, and harder than the fast lap's by `LAND_HIT_RATIO`.
    pub const LAND_HIT_G: f32 = 10.0;
    pub const LAND_HIT_RATIO: f32 = 1.4;
    /// On its own, only a landing this hard.
    pub const SOLO_LAND_HIT_G: f32 = 12.0;
    /// Force on the bars into a corner this many times the fast lap's, and at least `TORQUE_MIN`.
    pub const TORQUE_RATIO: f32 = 1.5;
    pub const TORQUE_MIN: f32 = 25.0;
    /// What the floors above were tuned on, five laps of a 250F: landings' 90th percentile, G,
    /// and the median force on the bars. A rider's own session scales the floors from these.
    pub const CAL_LAND_P90_G: f32 = 10.1;
    pub const CAL_TORQUE_MEDIAN: f32 = 17.0;
    /// How far a session may move them, and how many landings it needs to move the landing one.
    pub const SCALE: (f32, f32) = (0.7, 1.5);
    pub const SCALE_LANDINGS: usize = 8;
    /// Sitting this much more (or less) of a section than the fast lap is worth a word.
    pub const STANCE_GAP: f32 = 0.3;
    /// Peak bike lean through a corner's core. At or past `RUT_LEAN_DEG` something is holding
    /// the bike - a rut or a berm - because a flat corner's tyres let go first; at or under
    /// `FLAT_LEAN_DEG` nothing is. In between is left unknown rather than guessed.
    ///
    /// PROVISIONAL. Set from two sessions on one track (755 Compound, 450), where the one
    /// corner that reads flat in both sat at 40-44 deg and every other corner at 55-81. That
    /// is a real gap, but it is one track and one surface: these want re-checking against a
    /// flat corner on a stock track and a sand turn before anything leans on them.
    pub const RUT_LEAN_DEG: f32 = 60.0;
    pub const FLAT_LEAN_DEG: f32 = 45.0;
    /// A rut or berm must depart this far from the straight grade between the core's ends before
    /// its vertical profile gets a name. This is display-only until real laps tune it.
    pub const CORNER_PROFILE_M: f32 = 0.35;
    /// A hooked rut tightens decisively towards the exit. The ratio alone is unstable when both
    /// thirds are nearly straight, so require a real curvature increase as well.
    pub const HOOK_CURVATURE_RATIO: f32 = 1.8;
    pub const HOOK_CURVATURE_DELTA: f32 = 0.01;

    /// Mean vertical hit through the core, G. PROVISIONAL, from the same two sessions, where
    /// corner means ran 1.1 to 2.3 G. Peak hit was tried first and thrown out: it is a single
    /// sample and moved by more than 2 G between two sessions on the same corner.
    pub const ROUGH_HIT_G: f32 = 1.8;
    pub const SMOOTH_HIT_G: f32 = 1.2;

    /// A lap needs its stance known over this share of the section to be judged.
    pub const STANCE_KNOWN: f32 = 0.6;
    /// Turning less than this share of what the lean would give: the front is sliding.
    pub const PUSH: f32 = 0.75;
    pub const SOLO_PUSH: f32 = 0.65;
    /// Fork travel under braking into a corner that counts as diving, and shock travel on the
    /// gas out of one that counts as squatting.
    pub const DIVE: f32 = 0.85;
    pub const SQUAT: f32 = 0.75;
    /// How many corners or jumps before it's the setup rather than one moment.
    pub const SETUP_TIMES: usize = 2;
    /// The rear extending faster than this at a lip, m/s: the shock kicks.
    pub const KICK_MS: f32 = 0.6;
    /// In whoops, this deep on average and never extending faster than `PACK_EXT_MS`, front then
    /// rear: the suspension isn't coming back up between hits.
    pub const PACK_USED: f32 = 0.5;
    pub const PACK_EXT_MS: [f32; 2] = [0.4, 0.2];
    /// The shock this much deeper than the fork on steady straights, share of travel: nose-high.
    /// Under `RAKE_LO`: nose-down.
    pub const RAKE_HI: f32 = 0.3;
    pub const RAKE_LO: f32 = -0.05;
    /// A shock bottoming while compressing slower than this, m/s, is a low-speed problem.
    pub const SLOW_HIT_MS: f32 = 0.4;
    /// On an exit, the rear never slipping more than this (wheel over ground speed) means grip
    /// is left over; with this much less throttle than the fast lap, it's worth using.
    pub const GRIP_LEFT_SLIP: f32 = 1.08;
    pub const MORE_GAS: f32 = 0.2;
    /// On its own: an exit at least this long, ridden under this much throttle.
    pub const SOLO_EXIT_M: usize = 25;
    pub const SOLO_GAS: f32 = 0.55;

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
    /// The ground under the rear wheel as the recorder gives it, 0 in the air; see `soil`.
    pub ground: u8,
    /// Sitting or standing, `telemetry::stance`.
    pub stance: u8,
    /// Where the rider is asking to put their body, `telemetry::lean`: left/right then
    /// forward/back, each -1 to +1, NaN where it could not be read. Rider input, not a body
    /// angle - see the module docs on `telemetry::lean`.
    pub lean: [f32; 2],
    /// Share of the travel in use, 0 fully extended to 1 bottomed; see `Trace::fill_travel`.
    pub used: [f32; 2],
    /// Acceleration in G, in the chassis frame: sideways, up (1 standing still), forward.
    pub acc: [f32; 3],
    /// How fast the bike's heading turns, degrees a second. The game's yaw rate is about the
    /// bike's own axis, which leans over in a turn.
    pub turn: f32,
    /// Raw body-axis angular rates, degrees a second. Kept separately from `turn`: scrub/whip
    /// classification integrates the game's own axes, as MXBMRP3 does.
    pub yaw_rate: f32,
    pub roll_rate: f32,
    /// Bar angle, degrees, negative right, and the torque on the bars.
    pub steer: f32,
    pub torque: f32,
    /// Suspension speed, m/s, front then rear: positive extends, negative compresses.
    pub sv: [f32; 2],
    /// The hardest vertical hit, G, and the fastest extension and compression, in the metre up
    /// to this point: they last a sample or two, and blending onto the grid would blunt them.
    pub hit: f32,
    pub sv_hi: [f32; 2],
    pub sv_lo: [f32; 2],
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
        ground: s.wheel_material[1].clamp(0, 255) as u8,
        stance: s.stance,
        lean: s.lean,
        used: [0.0; 2],
        acc: s.acc,
        turn: s.yaw_rate / s.roll.to_radians().cos().max(0.3),
        yaw_rate: s.yaw_rate,
        roll_rate: s.roll_rate,
        steer: s.steer,
        torque: s.steer_torque,
        sv: s.susp_vel,
        hit: s.acc[1],
        sv_hi: s.susp_vel,
        sv_lo: s.susp_vel,
    }
}

fn lerp(a: &Point, b: &Point, f: f32) -> Point {
    let m = |p: f32, q: f32| p + (q - p) * f;
    let near = if f < 0.5 { a } else { b };
    // A lean axis can be unknown at either end, and unknown must not spread: NaN through `m`
    // would blank every metre between two readings, so a rider whose stick was readable for
    // most of a lap would come back with nothing.
    let ml = |p: f32, q: f32| match (crate::telemetry::lean::known(p), crate::telemetry::lean::known(q)) {
        (true, true) => m(p, q),
        (true, false) => p,
        (false, true) => q,
        (false, false) => f32::NAN,
    };
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
        ground: near.ground,
        stance: near.stance,
        lean: [ml(a.lean[0], b.lean[0]), ml(a.lean[1], b.lean[1])],
        used: [0.0; 2],
        acc: [m(a.acc[0], b.acc[0]), m(a.acc[1], b.acc[1]), m(a.acc[2], b.acc[2])],
        turn: m(a.turn, b.turn),
        yaw_rate: m(a.yaw_rate, b.yaw_rate),
        roll_rate: m(a.roll_rate, b.roll_rate),
        steer: m(a.steer, b.steer),
        torque: m(a.torque, b.torque),
        sv: [m(a.sv[0], b.sv[0]), m(a.sv[1], b.sv[1])],
        hit: m(a.hit, b.hit),
        sv_hi: [m(a.sv[0], b.sv[0]), m(a.sv[1], b.sv[1])],
        sv_lo: [m(a.sv[0], b.sv[0]), m(a.sv[1], b.sv[1])],
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
        // Each metre keeps the extremes of the samples that fell in it.
        let mut j = 0;
        for (i, p) in pts.iter_mut().enumerate() {
            let g = i as f32 * STEP_M;
            let first = j;
            while j < src.len() && src[j].0 <= g {
                let q = &src[j].1;
                if j == first {
                    (p.hit, p.sv_hi, p.sv_lo) = (q.hit, q.sv, q.sv);
                } else {
                    p.hit = p.hit.max(q.hit);
                    for k in 0..2 {
                        p.sv_hi[k] = p.sv_hi[k].max(q.sv[k]);
                        p.sv_lo[k] = p.sv_lo[k].min(q.sv[k]);
                    }
                }
                j += 1;
            }
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

    /// How much the bike turns for its lean in `r`: 1 is a clean turn, under 1 it leans more than
    /// it turns, so the front is sliding. None where it's hardly leaned.
    fn coordination(&self, r: Range<usize>) -> Option<f32> {
        let mut v: Vec<f32> = r
            .filter_map(|i| self.pts.get(i))
            .filter(|q| q.roll.abs() > 15.0 && !q.air && q.v > 4.0)
            .map(|q| q.v * q.turn.to_radians().abs() / (G * q.roll.to_radians().abs().tan()))
            .collect();
        if v.len() < 3 {
            return None;
        }
        v.sort_by(f32::total_cmp);
        Some(v[v.len() / 2])
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

/// The two partial-rotation air tricks Coach needs to tell apart. A whip can carry plenty of
/// world-space roll while the bike is sideways, so peak lean alone is not a scrub detector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AirMove {
    #[default]
    None,
    Scrub,
    Whip,
}

#[derive(Clone, Copy, Debug, Default)]
struct AirMotion {
    kind: AirMove,
    roll_start_s: Option<f32>,
    peak_roll: f32,
    peak_yaw: f32,
}

/// Classifies one flight with the same useful boundary as Thomas's MXBMRP3: wait 0.3 s,
/// integrate the bike's body-axis rates, give yaw/whip precedence, then accept roll or takeoff
/// lean as a scrub at 30 degrees. Coach works on a one-metre trace rather than every 100 Hz
/// plugin frame, so trapezoids preserve the rotation between its coarser samples.
fn air_motion(tr: &Trace, take: usize, land: usize) -> AirMotion {
    let Some(first) = tr.pts.get(take) else { return AirMotion::default() };
    if land <= take || land >= tr.len() || tr.span(take, land) < th::AIR_COMMIT_S {
        return AirMotion::default();
    }
    let mut roll = 0.0f32;
    let mut yaw = 0.0f32;
    let mut peak_roll = 0.0f32;
    let mut peak_yaw = 0.0f32;
    let mut elapsed = 0.0f32;
    let mut roll_start_s = (first.roll.abs() >= th::SCRUB_START_DEG).then_some(0.0);
    for i in take + 1..=land {
        let a = &tr.pts[i - 1];
        let b = &tr.pts[i];
        let dt = (b.t - a.t).clamp(0.0, 0.2);
        elapsed += dt;
        roll += (a.roll_rate + b.roll_rate) * 0.5 * dt;
        yaw += (a.yaw_rate + b.yaw_rate) * 0.5 * dt;
        peak_roll = peak_roll.max(roll.abs());
        peak_yaw = peak_yaw.max(yaw.abs());
        if roll_start_s.is_none() && peak_roll >= th::SCRUB_START_DEG {
            roll_start_s = Some(elapsed);
        }
    }
    let start_roll = first.roll.abs();
    let kind = if peak_yaw >= th::PARTIAL_ROTATION_DEG {
        AirMove::Whip
    } else if start_roll >= th::PARTIAL_ROTATION_DEG || peak_roll >= th::PARTIAL_ROTATION_DEG {
        AirMove::Scrub
    } else {
        AirMove::None
    };
    AirMotion { kind, roll_start_s, peak_roll, peak_yaw }
}

/// A known rider-input mean. Unknown values do not become a centred rider.
fn mean_lean(tr: &Trace, range: Range<usize>, axis: usize) -> Option<f32> {
    let known: Vec<f32> = tr.pts.get(range)?.iter().map(|q| q.lean[axis]).filter(|&v| crate::telemetry::lean::known(v)).collect();
    if known.len() < 3 {
        return None;
    }
    Some(known.iter().sum::<f32>() / known.len() as f32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Straight,
    Corner,
    Jump,
    Rhythm,
    Whoops,
}

/// Whether the ground holds the bike through a corner: a rut or a berm does, a flat corner
/// leaves the tyres to do it. It decides advice that inverts between the two - counter-lean
/// is right on a flat corner and wrong in a rut, and sitting is right on a smooth rut and
/// wrong on a hooked one - so a coach that can't tell them apart is as likely to be wrong as
/// right on those points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Hold {
    /// Not sure, and it must read as today's behaviour wherever it is used.
    #[default]
    Unknown,
    Flat,
    Rutted,
}

/// How rough a corner is: whether the suspension is working through it or riding a clean line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Bumps {
    #[default]
    Unknown,
    Smooth,
    Rough,
}

/// The vertical line the bike follows through a held corner. Kept separate from `Hold`: both a
/// rut below the grade and a berm above it can hold the bike over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Profile {
    #[default]
    Unknown,
    Rut,
    Berm,
}

/// Whether the corner tightens strongly towards its exit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Shape {
    #[default]
    Unknown,
    Constant,
    Hooked,
}

/// Lynds' six rider-facing corner names. These are display classifications only: coaching rules
/// continue to read the measured properties, and an incomplete or conflicting reading stays
/// `Unknown` rather than forcing the nearest label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CornerKind {
    #[default]
    Unknown,
    Flat,
    SmoothRut,
    HookedRut,
    RoughRut,
    WhoopedSand,
    SmoothBerm,
}

/// What sort of corner this is, as measured properties plus a display-only Lynds name.
///
/// The properties remain the source of truth because a corner is usually several things at
/// once. `kind` is only filled when their combination clearly matches one of the six taught
/// types; every marginal reading stays `Unknown`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CornerType {
    pub hold: Hold,
    pub bumps: Bumps,
    /// The ground under the corner, from the rear wheel's material. `None` off a corner or
    /// where the recorder gave nothing usable.
    pub soil: Option<crate::soil::Soil>,
    pub profile: Profile,
    pub shape: Shape,
    pub kind: CornerKind,
}

/// A stretch of track with one job: a corner with its braking zone and exit, a jump with its
/// face and landing, or the straight between.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub kind: Kind,
    /// Stable across laps and sessions once the track map has named it: `t5`, `j2`, `w1`. Until
    /// then it is this lap's own positional slug, so a section always has a key.
    pub id: String,
    pub name: String,
    pub start: usize,
    pub end: usize,
    /// The part that makes it what it is: the turn itself, or takeoff to last landing.
    pub core: (usize, usize),
    /// +1 right-hander, -1 left-hander, 0 otherwise.
    pub dir: i8,
    /// What sort of corner it is. All `Unknown` off a corner. The UI reads the display name;
    /// coaching rules deliberately do not, so classification cannot change their behaviour.
    #[serde(default)]
    pub corner: CornerType,
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

/// Whether an air feature sits inside a corner rather than merely touching its edge. A jump
/// leaving a corner, or landing into one, is its own feature; one taken mid-corner is part of it.
fn mostly_within(air: &Feature, corner: &Feature) -> bool {
    let lo = air.a.max(corner.a);
    let hi = air.b.min(corner.b);
    let overlap = hi.saturating_sub(lo);
    overlap * 2 >= (air.b - air.a).max(1)
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

    // Curvature, with the flights taken out of it. In the air the bike travels straight in plan
    // view, so a jump through a corner read as no curvature at all for its whole length — far
    // more than `CORNER_MERGE_M` — which split the corner into two halves that each fell under
    // the minimum and were both thrown away. The ground either side of a flight is one corner.
    let mut k = curvature(tr);
    for i in 0..n.min(k.len()) {
        if tr.pts[i].air {
            k[i] = f32::NAN;
        }
    }
    let mut cores: Vec<(usize, usize, f32)> = Vec::new();
    let mut i = 0;
    while i < n {
        if !(k[i].abs() > th::CORNER_CURVATURE) {
            i += 1;
            continue;
        }
        let (s, sign) = (i, k[i].signum());
        // A flight, or a metre of straightening, doesn't end the corner: keep walking while the
        // ground still turns the same way, and let the merge below join what it skipped.
        while i < n && (k[i].is_nan() || (k[i].abs() > th::CORNER_CURVATURE && k[i].signum() == sign)) {
            i += 1;
        }
        match cores.last_mut() {
            Some(last) if last.2 == sign && s - last.1 <= th::CORNER_MERGE_M => last.1 = i - 1,
            _ => cores.push((s, i - 1, sign)),
        }
    }
    for (a, b, sign) in cores {
        // Only the ground counts towards how far it turned.
        let turned = k[a..=b].iter().filter(|v| !v.is_nan()).sum::<f32>().abs() * STEP_M * 180.0 / PI;
        if turned >= th::CORNER_MIN_DEG {
            feats.push(Feature { kind: Kind::Corner, a, b, dir: sign as i8, runs: Vec::new() });
        }
    }
    // A jump that sits inside a corner belongs to the corner. It used to delete it outright:
    // any overlap at all and the corner was never built, so a jump on a curved piece of track —
    // ordinary motocross — came back as "Rhythm 2" with the braking zone before it orphaned into
    // a "Straight". The air feature is folded in instead, so the corner keeps its name and its
    // jumps keep their advice.
    // Corners first, so every corner is there to fold into: the air features are built before
    // them above, and folding in list order would always see an empty list.
    let (mut folded, air): (Vec<Feature>, Vec<Feature>) =
        std::mem::take(&mut feats).into_iter().partition(|f| f.kind == Kind::Corner);
    for f in air {
        let inside = folded
            .iter_mut()
            .find(|c| c.a <= f.b && f.a <= c.b && mostly_within(&f, c));
        match inside {
            Some(c) => {
                c.a = c.a.min(f.a);
                c.b = c.b.max(f.b);
                c.runs.extend(f.runs);
            }
            None => folded.push(f),
        }
    }
    feats = folded;
    feats.sort_by_key(|f| f.a);
    feats
}

/// What sort of corner this is, read off the reference lap over the corner's core.
///
/// Each property is read from the one measurement that held up between two sessions on the
/// same track, and each returns `Unknown` unless the reading is clear of both thresholds. It
/// is meant to be shy: a corner called nothing costs a rule its extra sharpness, while a
/// corner called wrong makes the rule worse than it is today.
pub fn corner_type(tr: &Trace, core: (usize, usize)) -> CornerType {
    let (a, b) = (core.0.min(tr.len().saturating_sub(1)), core.1.min(tr.len().saturating_sub(1)));
    if b <= a + 2 {
        return CornerType::default();
    }
    let pts = &tr.pts[a..=b];
    let ground: Vec<&Point> = pts.iter().filter(|q| !q.air).collect();
    if ground.is_empty() {
        return CornerType::default();
    }

    // How far it leaned at all. A rut or a berm holds the bike over further than the tyres
    // alone would on a flat corner, so the peak is what separates them.
    let lean = ground.iter().map(|q| q.roll.abs()).fold(0.0f32, f32::max);
    let hold = if lean >= th::RUT_LEAN_DEG {
        Hold::Rutted
    } else if lean <= th::FLAT_LEAN_DEG {
        Hold::Flat
    } else {
        Hold::Unknown
    };

    // How hard the ground hit on the way round, averaged: one bad sample is not a rough corner.
    let hit = ground.iter().map(|q| q.hit.abs()).sum::<f32>() / ground.len() as f32;
    let bumps = if hit >= th::ROUGH_HIT_G {
        Bumps::Rough
    } else if hit <= th::SMOOTH_HIT_G {
        Bumps::Smooth
    } else {
        Bumps::Unknown
    };

    // What it is made of: the ground under the rear wheel for most of the corner. Read dry;
    // wet conditions are a whole-lap matter that `soil` already handles.
    let mut tally: std::collections::HashMap<u8, usize> = std::collections::HashMap::new();
    for q in &ground {
        *tally.entry(q.ground).or_default() += 1;
    }
    let soil = tally
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .and_then(|(v, _)| crate::soil::Soil::from_wheel(v, false));

    // Remove the straight grade between the ends before looking for a bowl or a bank. A corner
    // climbing or descending a hill is not thereby a rut or berm.
    let first = ground.first().expect("ground is not empty").y;
    let last = ground.last().expect("ground is not empty").y;
    let mut below = 0.0f32;
    let mut above = 0.0f32;
    for (i, q) in ground.iter().enumerate() {
        let f = i as f32 / (ground.len() - 1).max(1) as f32;
        let residual = q.y - (first + (last - first) * f);
        below = below.min(residual);
        above = above.max(residual);
    }
    let profile = if below <= -th::CORNER_PROFILE_M && -below > above {
        Profile::Rut
    } else if above >= th::CORNER_PROFILE_M && above > -below {
        Profile::Berm
    } else {
        Profile::Unknown
    };

    // Curvature is distance-normalised, unlike yaw rate, so the same hook does not change label
    // merely because one rider entered it faster. The strong gate deliberately leaves many
    // corners unknown: earlier real-lap work showed weaker first/last ratios were noisy.
    let curve = curvature(tr);
    let third = ((b - a + 1) / 3).max(1);
    let mean_curve = |from: usize, to: usize| {
        curve[from..to].iter().map(|v| v.abs()).sum::<f32>() / (to - from).max(1) as f32
    };
    let first_curve = mean_curve(a, (a + third).min(b + 1));
    let last_curve = mean_curve((b + 1).saturating_sub(third).max(a), b + 1);
    let shape = if last_curve >= first_curve * th::HOOK_CURVATURE_RATIO
        && last_curve - first_curve >= th::HOOK_CURVATURE_DELTA
    {
        Shape::Hooked
    } else if first_curve > 0.0 && (0.67..=1.5).contains(&(last_curve / first_curve)) {
        Shape::Constant
    } else {
        Shape::Unknown
    };

    let kind = named_corner(hold, bumps, soil, profile, shape);
    CornerType { hold, bumps, soil, profile, shape, kind }
}

fn named_corner(hold: Hold, bumps: Bumps, soil: Option<crate::soil::Soil>, profile: Profile, shape: Shape) -> CornerKind {
    use crate::soil::Soil;
    if hold == Hold::Flat {
        CornerKind::Flat
    } else if bumps == Bumps::Rough && soil == Some(Soil::Sand) {
        CornerKind::WhoopedSand
    } else if hold == Hold::Rutted && bumps == Bumps::Rough {
        CornerKind::RoughRut
    } else if hold == Hold::Rutted && shape == Shape::Hooked {
        CornerKind::HookedRut
    } else if hold == Hold::Rutted && bumps == Bumps::Smooth && profile == Profile::Berm {
        CornerKind::SmoothBerm
    } else if hold == Hold::Rutted && bumps == Bumps::Smooth && profile == Profile::Rut {
        CornerKind::SmoothRut
    } else {
        CornerKind::Unknown
    }
}

/// Splits the track into sections, from the reference lap.
pub fn sections(reference: &Trace) -> Vec<Section> {
    let last = reference.len() - 1;
    let feats = features(reference);
    let mut out = Vec::new();
    let mut counts = [0usize; 5];
    let mut label = |kind: Kind| {
        let k = kind as usize;
        counts[k] += 1;
        let word = ["Straight", "Turn", "Jump", "Rhythm", "Whoops"][k];
        let tag = ["s", "t", "j", "r", "w"][k];
        (format!("{tag}{}", counts[k]), format!("{word} {}", counts[k]))
    };
    let straight = |a: usize, b: usize, (id, name): (String, String)| Section {
        kind: Kind::Straight,
        id,
        name,
        start: a,
        end: b,
        core: (a, b),
        dir: 0,
        corner: CornerType::default(),
        runs: Vec::new(),
    };

    let mut cursor = 0;
    for (i, f) in feats.iter().enumerate() {
        let mut start = f.a.saturating_sub(f.entry()).max(cursor);
        if start - cursor > th::MIN_STRAIGHT_M {
            out.push(straight(cursor, start, label(Kind::Straight)));
        } else {
            start = cursor;
        }
        let next = feats.get(i + 1).map_or(last, |n| n.a.saturating_sub(n.entry()));
        let end = (f.b + f.exit()).min(next.max(f.b + 1)).min(last);
        let (id, name) = label(f.kind);
        out.push(Section {
            kind: f.kind,
            id,
            name,
            start,
            end,
            core: (f.a, f.b),
            dir: f.dir,
            corner: if f.kind == Kind::Corner { corner_type(reference, (f.a, f.b)) } else { CornerType::default() },
            runs: f.runs.clone(),
        });
        cursor = end;
    }
    if last - cursor > th::MIN_STRAIGHT_M || out.is_empty() {
        out.push(straight(cursor, last, label(Kind::Straight)));
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
    pub(crate) weight: f32,
    /// Advice about something risky: shown even where the section lost no time.
    #[serde(skip)]
    pub(crate) safety: bool,
    /// True on the lap's own terms rather than against the fast lap: shown whatever the clock
    /// says, but not a warning, so it still counts as an explanation of the time lost. A jump
    /// you over-jump on every lap costs nothing against yourself and is still worth saying.
    #[serde(skip)]
    pub(crate) absolute: bool,
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
    /// The ground here, filled in by the caller, which knows the weather.
    pub soil: Option<crate::soil::Profile>,
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
    /// There is a reference lap to draw: its traces are in `channels` and its line in `paths`.
    /// False against the ideal lap, which is a time per section and was never ridden whole.
    pub traced: bool,
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
    /// This rider on this bike against what the floors were tuned on (see [`norm`]): landings
    /// and bar force, 0 to keep the tuned floors.
    pub land_scale: f32,
    pub torque_scale: f32,
}

impl Bike {
    fn land_g(&self, floor: f32) -> f32 {
        floor * if self.land_scale > 0.0 { self.land_scale } else { 1.0 }
    }
    fn torque_min(&self) -> f32 {
        th::TORQUE_MIN * if self.torque_scale > 0.0 { self.torque_scale } else { 1.0 }
    }
}

/// How this rider's session compares with what the floors were tuned on: `(landing, bars)`.
/// A heavier bike or a rider who always lands harder hits harder everywhere, so a hard landing
/// is judged against their own; a light bike needs less force on the bars. Each is 0 when the
/// session doesn't have enough to say.
pub fn norm(laps: &[Trace]) -> (f32, f32) {
    let mut lands: Vec<f32> = Vec::new();
    let mut bars: Vec<f32> = Vec::new();
    for t in laps {
        for i in 1..t.pts.len() {
            if t.pts[i - 1].air && !t.pts[i].air {
                lands.push(t.max_by(i..(i + 8).min(t.pts.len()), |q| q.hit));
            }
        }
        bars.extend(t.pts.iter().filter(|q| !q.air && q.v > 5.0).map(|q| q.torque.abs()).filter(|x| x.is_finite()));
    }
    let at = |v: &mut Vec<f32>, q: f32| {
        v.sort_by(f32::total_cmp);
        v[((v.len() - 1) as f32 * q) as usize]
    };
    let clamp = |x: f32| x.clamp(th::SCALE.0, th::SCALE.1);
    let land = if lands.len() >= th::SCALE_LANDINGS { clamp(at(&mut lands, 0.9) / th::CAL_LAND_P90_G) } else { 0.0 };
    let torque = if bars.len() >= 200 { clamp(at(&mut bars, 0.5) / th::CAL_TORQUE_MEDIAN) } else { 0.0 };
    (land, torque)
}

fn theme(skill: &str) -> &'static str {
    match skill {
        // `in_too_hot` groups with the brake point because that is its fix, not with corner
        // speed, which would separate a tip from its own family.
        "brake_early" | "brake_late" | "brake_harder" | "brake_unneeded" | "more_front" | "front_lock"
        | "rear_lock" | "stoppie" | "clutch_braking" | "bottom_braking" | "in_too_hot" => "Braking",
        "carry_speed" | "lean_more" | "line" | "coasting" | "bar_fight" | "front_push" | "apex_early" => "Corner speed",
        "late_throttle" | "wheelspin" | "wheelie" | "exit_speed" | "gear_up" | "gear_down" | "throttle_room" => "Corner exits",
        "jump_it" | "chop_face" | "scrub" | "land_short" | "overjump" | "land_throttle" | "land_crooked"
        | "bottom_landing" | "air_throttle" | "rhythm_count" | "land_hard" => "Jumps",
        "whoops_speed" | "whoops_throttle" | "whoops_bucking" => "Whoops",
        "shift_earlier" | "full_gas" => "Straights",
        "stance_sit" | "stance_stand" => "Body position",
        _ => "Other",
    }
}

/// The lap by theme: each section counts towards the theme of its headline tip.
fn overall(sections: &[SectionReview], solo: bool) -> Vec<Theme> {
    let mut themes: Vec<Theme> = Vec::new();
    for s in sections {
        let Some(head) = s.findings.iter().find(|f| f.skill != "unclear") else { continue };
        // A judgement counts towards the lap's themes even where the section cost nothing: it
        // survived the `retain` for the same reason, and a card the rider can read while the
        // theme above it pretends the mistake isn't there reads as the app contradicting itself.
        if !solo && s.lost <= th::WORTH_S && !head.safety && !head.absolute {
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
            stance(&mut c);
            match s.kind {
                Kind::Corner => corner(&mut c),
                Kind::Jump | Kind::Rhythm => jumps(&mut c),
                Kind::Whoops => whoops(&mut c),
                Kind::Straight => straight(&mut c),
            }
            let mut findings = dedupe(c.out, s.kind != Kind::Corner);
            if lost <= th::WORTH_S {
                findings.retain(|f| f.safety || f.absolute);
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
                    absolute: false,
                });
            }
            findings.sort_by(|a, b| b.weight.total_cmp(&a.weight));
            SectionReview { section: s, lap_time, ref_time, lost, findings, soil: None }
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
        traced: true,
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
    fn push(&mut self, skill: &'static str, weight: f32, at: usize, title: &str, detail: String, safety: bool, absolute: bool) {
        self.out.push(Finding { skill, title: title.into(), detail, at, weight, safety, absolute });
    }

    fn add(&mut self, skill: &'static str, weight: f32, at: usize, title: &str, detail: String) {
        self.push(skill, weight, at, title, detail, false, false);
    }

    fn warn(&mut self, skill: &'static str, at: usize, title: &str, detail: String) {
        self.push(skill, 2.0, at, title, detail, true, false);
    }

    /// A judgement the lap earns on its own — "you're over-jumping this" — rather than a
    /// difference from the fast lap. Survives a section that cost no time, but unlike `warn`
    /// it still reads as an explanation, so it doesn't trigger the "compare the traces" tip.
    fn judge(&mut self, skill: &'static str, weight: f32, at: usize, title: &str, detail: String) {
        self.push(skill, weight, at, title, detail, false, true);
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

    // The bars and the front tyre.
    let bars = |t: &Trace, apex: usize| t.mean(start..apex + 1, |q| q.torque.abs());
    let (tq_p, tq_r) = (bars(p, apex_p), bars(r, apex_r));
    if tq_p > c.bike.torque_min() && tq_p > tq_r * th::TORQUE_RATIO {
        c.add("bar_fight", 0.5, start, "Relax on the bars", format!(
            "You push {:.0}% harder on the bars into {name} than the fast lap. Grip the bike with your \
             knees, keep your elbows loose and steer with your weight on the outside peg.",
            (tq_p / tq_r.max(0.1) - 1.0) * 100.0
        ));
    }
    if let (Some(cp), Some(cr)) = (p.coordination(a..b + 1), r.coordination(a..b + 1)) {
        if cp < th::PUSH && cr > th::PUSH + 0.1 {
            c.warn("front_push", apex_p, "The front is washing out", format!(
                "Through {name} the bike leans more than it turns: the front tyre is sliding. Brake a touch \
                 earlier, get your weight forward over the front, and pick the bike up before the gas."
            ));
        }
    }

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

    // Coming in too fast, on the lap's own terms. The gate is where the speed leaves the bike;
    // the consequence is what stops it crying wolf on a rider trail-braking into a rut, which
    // in MX Bikes is how the corner is meant to be ridden.
    if let Some(h) = hot(p, a, b) {
        let hot_enough = h.share > th::HOT_INSIDE_SHARE
            && h.inside_kmh > th::HOT_INSIDE_KMH
            && h.braking_s > th::HOT_BRAKE_S;
        if hot_enough {
            let late_apex = (h.apex - a) as f32 > (b - a) as f32 * th::HOT_APEX_SHARE;
            // Leaned in, then picked back up to save it. No gas, or it is just an exit.
            let stood_up = p.max_by(h.at..h.apex + 1, |q| q.roll.abs()) - p.pts[h.apex].roll.abs()
                > th::HOT_STAND_DEG
                && p.pts[h.apex].throttle < th::THROTTLE_ON;
            // Ran wide: further outside the fast line leaving the corner than entering it.
            let across = |i: usize| {
                let heading = r.bearing(i);
                let (dx, dz) = (p.pts[i].x - r.pts[i].x, p.pts[i].z - r.pts[i].z);
                // Outside runs against the corner's handedness.
                -(dx * heading.cos() - dz * heading.sin()) * s.dir as f32
            };
            let wide = (s.dir != 0 && across(end) > th::HOT_WIDE_M && across(end) > across(h.at))
                .then(|| across(end));

            let consequence = if let Some(m) = wide {
                Some(format!("run {m:.1} m wide on the way out"))
            } else if stood_up {
                Some("have to pick the bike up mid-corner".to_string())
            } else if late_apex {
                Some("can't start driving until well past the middle of the turn".to_string())
            } else {
                None
            };
            if let Some(what) = consequence {
                c.judge("in_too_hot", 0.95, h.at, "You're coming in too fast", format!(
                    "You carry about {:.0} km/h too much into {name}: {:.0}% of the speed comes off \
                     after you have already turned in, and you {what}. Get the braking done in a \
                     straight line before turn-in, then roll through on a steady throttle.",
                    h.inside_kmh,
                    h.share * 100.0
                ));
            }
        }
    }

    // Turning in too early: slowest in the first third, and still leaned with the throttle shut
    // at the exit — running out of track rather than driving off the corner.
    if let Some(at) = turn_in(p, a, b) {
        let apex = p.slowest(at..b + 1);
        let early = ((apex - a) as f32) < (b - a) as f32 * th::APEX_EARLY_SHARE;
        let stuck = p.pts[b].roll.abs() > th::TURN_IN_DEG && p.pts[b].throttle < th::THROTTLE_ON;
        if early && stuck {
            c.judge("apex_early", 0.7, at, "You're turning in too early", format!(
                "Your slowest point in {name} comes in the first third of the turn, and you are still \
                 leaned over with the throttle shut on the way out. Turn in later and point the bike \
                 at the exit so you can drive off the corner."
            ));
        }
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
        // Only where the front is holding: leaning more on a sliding front would put it down.
        let grip = if p.coordination(a..b + 1).is_some_and(|c| c >= 0.95) {
            " The front isn't sliding, so the grip is there."
        } else {
            ""
        };
        c.add("lean_more", 0.6, apex_p, "Lean the bike more", format!(
            "The fast lap leans the bike {:.0}° further in {name}. Lean the bike in and keep the rider \
             slightly to the outside, so the tyres keep their grip.{grip}",
            lean_r - lean_p
        ));
    }
    let heading = r.bearing(apex_r);
    let (dx, dz) = (p.pts[apex_r].x - r.pts[apex_r].x, p.pts[apex_r].z - r.pts[apex_r].z);
    let offset = dx * heading.cos() - dz * heading.sin(); // + is right of the fast line
    if offset.abs() > th::LINE_M && s.dir != 0 {
        let tighter = offset * s.dir as f32 > 0.0;
        // Named as the rider would say it, not as a direction of travel: "wider" on its own
        // reads as advice about effort, "the outside line" is a place on the track.
        c.add("line", 0.75, apex_r, if tighter { "Take the outside line" } else { "Take the inside line" }, format!(
            "At the apex of {name} you are {:.1} m {} the fast lap. Run {} through here, and pick that \
             line before you brake.",
            offset.abs(),
            if tighter { "inside" } else { "outside" },
            if tighter { "the outside line" } else { "the inside line" }
        ));
    }
    // High or low: the same corner ridden up on the berm or down in the rut. The bike's own
    // height at the apex says which, with no terrain needed — both laps are at the same point
    // of the lap, so what is left between them is the ground each chose.
    if !p.pts[apex_p].air && !r.pts[apex_r].air {
        let up = p.pts[apex_p].y - r.pts[apex_r].y;
        if up.abs() > th::HEIGHT_M {
            let (yours, theirs) = if up > 0.0 { ("high", "low") } else { ("low", "high") };
            c.add(
                "height",
                0.55,
                apex_r,
                if up > 0.0 { "Come down off the high line" } else { "Use the high line" },
                format!(
                    "Through {name} you ride the {yours} line, about {:.1} m {} the fast lap's — it takes the                      {theirs} line here. {}",
                    up.abs(),
                    if up > 0.0 { "above" } else { "below" },
                    if up > 0.0 {
                        "Up on the bank carries less speed unless it's holding a rut. Drop in and let the                          berm turn the bike."
                    } else {
                        "There is more to lean on higher up. Use the bank and let it hold the bike through                          the turn."
                    }
                ),
            );
        }
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
    // Grip left on the table: less gas than the fast lap out of the corner, while the rear never
    // slips and the front stays down.
    let gas = |t: &Trace, from: usize| t.mean(from..end + 1, |q| q.throttle);
    let (gas_p, gas_r) = (gas(p, apex_p), gas(r, apex_r));
    let slip = p.max_by(apex_p..end + 1, |q| if q.air || q.v < 5.0 { 0.0 } else { q.slip_r });
    let front_up = p.time_where(apex_p..end, |q| q.off[0] && !q.off[1]) > th::WHEELIE_S;
    if !exit_explained && gas_r - gas_p > th::MORE_GAS && slip < th::GRIP_LEFT_SLIP && !front_up {
        exit_explained = true;
        c.add("throttle_room", 0.7, apex_p, "Hold more throttle", format!(
            "Out of {name} you use {:.0}% throttle to the fast lap's {:.0}%, and the rear never slips. The \
             grip is there: roll it on further and hold it.",
            gas_p * 100.0,
            gas_r * 100.0
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

    // Braking late and coming in hot are the same cause with the same fix, and the judgement
    // outranks the comparison, so both would read as the tip said twice. `dedupe` keys on
    // (skill, title) and can't see across two skills.
    if c.out.iter().any(|f| f.skill == "in_too_hot") {
        c.out.retain(|f| f.skill != "brake_late");
    }
}

fn jumps(c: &mut Ctx) {
    let (p, r, s) = (c.p, c.r, c.s);
    let name = s.name.clone();
    let mine = air_runs(p, s.start..s.end + 1);
    let theirs = s.runs.len();

    // Which line over it. The line tip in `corner` needs a corner's handedness to say inside
    // or outside, so it never fired on a jump at all — and "which line do you take over this
    // jump" is one of the first things a rider asks. Here there is no inside: it is which side
    // of the face you leave, named as left or right of the lap you're held against.
    if let Some(&(take, _)) = s.runs.first() {
        let heading = r.bearing(take);
        let at = take.min(p.pts.len() - 1);
        let (dx, dz) = (p.pts[at].x - r.pts[take].x, p.pts[at].z - r.pts[take].z);
        let across = dx * heading.cos() - dz * heading.sin(); // + is right of the fast lap
        if across.abs() > th::LINE_M {
            let (yours, theirs_side) = if across > 0.0 { ("right", "left") } else { ("left", "right") };
            c.add(
                "jump_line",
                0.6,
                take,
                format!("Take {name} on the {theirs_side}").as_str(),
                format!(
                    "You leave the face of {name} about {:.1} m to the {yours} of the fast lap, which takes                      the {theirs_side}. Line it up before the face: where you take off decides where you                      land, and the way out of {name} is set by then.",
                    across.abs()
                ),
            );
        }
    }

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
        // No landing verdict is reached past this return, so the one fact that explains a
        // mis-paired rhythm goes into the count tip rather than being lost with it.
        if landing(p, mine[0].0, mine[0].1, s.end, c.bike).0 == Landing::Flat {
            if let Some(f) = c.out.last_mut() {
                f.detail.push_str(" You go long on the first one, which is what stops you linking them.");
            }
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
        let (motion_p, motion_r) = (air_motion(p, pt, pl), air_motion(r, rt, rl));
        // Landing short, the fix is speed; staying low would only make it shorter.
        let short = (pl as i64 - rl as i64) < -th::SHORT_M;
        if air_p > air_r * th::FLOAT_AIR + 0.1 && higher > th::FLOAT_HEIGHT_M && !short {
            let mut how: Vec<String> = Vec::new();
            if motion_r.kind == AirMove::Scrub {
                match motion_p.kind {
                    AirMove::Whip => how.push(
                        "You yaw the bike sideways like a whip; for a scrub, roll it over at the lip instead.".into(),
                    ),
                    AirMove::Scrub
                        if motion_p
                            .roll_start_s
                            .zip(motion_r.roll_start_s)
                            .is_some_and(|(mine, reference)| mine - reference > th::SCRUB_START_GAP_S) =>
                    {
                        how.push("You make the roll once you are airborne; start it as the rear wheel leaves the lip.".into())
                    }
                    AirMove::Scrub if motion_p.peak_roll + 10.0 < motion_r.peak_roll => how.push(
                        "The fast lap commits farther to the roll at takeoff.".into(),
                    ),
                    AirMove::None => how.push(
                        "The fast lap rolls the bike into a scrub at the lip; your bike stays upright.".into(),
                    ),
                    _ => {}
                }
                let before_p = pt.saturating_sub(8)..(pt + 1).min(p.len());
                let before_r = rt.saturating_sub(8)..(rt + 1).min(r.len());
                if let (Some(sit_p), Some(sit_r)) = (seated(p, before_p.clone()), seated(r, before_r.clone())) {
                    if sit_p < th::SCRUB_SEATED_SHARE && sit_r - sit_p > th::STANCE_GAP {
                        how.push("Sit through the face so the bike can lean underneath you.".into());
                    }
                }
                use crate::telemetry::lean;
                if let (Some(fwd_p), Some(fwd_r)) =
                    (mean_lean(p, before_p, lean::FB), mean_lean(r, before_r, lean::FB))
                {
                    if fwd_r > 0.1 && fwd_r - fwd_p > th::SCRUB_FORWARD_GAP {
                        how.push("Move the rider forward over the face, as the fast lap does, to keep the flight low.".into());
                    }
                }
            }
            if how.is_empty() {
                how.push("Stay seated, lean the bike over as you leave the lip, then counter it in the air.".into());
            } else {
                how.push("Counter the lean in the air so the bike is straight for touchdown.".into());
            }
            c.add("scrub", 0.85, pt, "Stay low: scrub it", format!(
                "You fly {higher:.1} m higher and {:.1} s longer than the fast lap over {name}. {}",
                air_p - air_r,
                how.join(" ")
            ));
        }
        let landed = pl as i64 - rl as i64;
        // The ground settles this where it can read it; the pairing against the fast lap only
        // speaks where it can't. A jump taken long on every lap costs nothing against yourself,
        // and is still the thing worth saying.
        let (verdict, hit_g) = landing(p, pt, pl, s.end, c.bike);
        match verdict {
            Landing::Flat => c.judge(
                "overjump",
                0.9,
                // The spot is the face, not the landing: that is where the rider can still act.
                pt,
                "You're over-jumping this",
                format!(
                    "Your {} m flight over {name} finishes out on the flat, past the downslope, and \
                     hits {hit_g:.0} G for it. Ease out of the throttle a bike length before the lip, \
                     or stay seated and lean the bike over as you leave it so it stays low.",
                    pl - pt
                ),
            ),
            Landing::Face => c.judge("land_short", 0.9, pl, "You're casing this one", format!(
                "You come down on the up-face of {name} and the landing stops you dead. Carry more \
                 speed up to the lip and stay on the gas all the way to it."
            )),
            Landing::Ramp | Landing::Unsure => {
                if landed < -th::SHORT_M {
                    let slow = p.pts[pt].v < r.pts[rt].v * th::TAKEOFF_SPEED_RATIO;
                    let why = if slow {
                        format!(" You hit the lip {:.0} km/h slower.", (r.pts[rt].v - p.pts[pt].v) * KMH)
                    } else {
                        String::new()
                    };
                    c.add("land_short", 0.8, pl, "You're landing short", format!(
                        "You land {} m short of the fast lap at {name} and lose speed on the face of \
                         the landing. Carry more speed up to the lip.{why}",
                        -landed
                    ));
                } else if landed > th::LONG_M {
                    // At the face like the absolute verdict: the landing is where it shows, the
                    // lip is where the rider can still do something about it.
                    c.add("overjump", 0.8, pt, "You're overjumping", format!(
                        "You land {landed} m past the fast lap at {name}, beyond the downslope. Roll \
                         off a touch before the lip, or scrub it lower."
                    ));
                }
            }
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
        // Lynds deliberately lands leaned when a jump will come up short so the rear slides up
        // the face instead of rebounding. Do not call that recovery move a crooked landing.
        if verdict != Landing::Face && lean_p > th::LAND_ROLL_DEG && lean_p - lean_r > th::LAND_ROLL_DEG / 2.0 {
            if motion_p.kind == AirMove::Scrub {
                c.warn("land_crooked", pl, "Bring the scrub back sooner", format!(
                    "You get the bike rolled over, but it is still leaned {lean_p:.0}° after you land at {name}. \
                     Counter the lean earlier in the air so it is straight for touchdown."
                ));
            } else {
                c.warn("land_crooked", pl, "Straighten up before landing", format!(
                    "The bike is still leaned {lean_p:.0}° after you land at {name}. Bring it straight in the air, \
                     a moment earlier."
                ));
            }
        }
        let hit = |t: &Trace, l: usize| t.max_by(l..(l + 8).min(s.end + 1).max(l + 1), |q| q.hit);
        let (hp, hr) = (hit(p, pl), hit(r, rl));
        // Where the landing is already called out as over-jumped, the G is the symptom of it
        // and the over-jump detail quotes the figure anyway.
        if verdict != Landing::Flat && hp > c.bike.land_g(th::LAND_HIT_G) && hp > hr * th::LAND_HIT_RATIO {
            c.warn("land_hard", pl, "Land softer", format!(
                "You hit {hp:.0} G landing {name}, the fast lap {hr:.0} G. Aim for the downslope, and soak \
                 the landing up with your legs instead of locking your arms."
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

/// Where the rider committed the bike: the first metre in the core leaned past `TURN_IN_DEG`.
fn turn_in(t: &Trace, a: usize, b: usize) -> Option<usize> {
    (a..=b).find(|&i| t.pts[i].roll.abs() > th::TURN_IN_DEG)
}

/// How a corner's speed came off, relative to the moment the bike was committed.
struct Hot {
    /// Turn-in, and the slowest metre at or after it.
    at: usize,
    apex: usize,
    /// Speed still to lose at turn-in, km/h, and that as a share of the turn-in speed.
    inside_kmh: f32,
    share: f32,
    /// How long the brake lever stays held past turn-in.
    braking_s: f32,
}

/// Reads a corner on its own terms. `None` where the core is too short to have an inside to
/// speak of, or the rider never leans it in at all.
fn hot(t: &Trace, a: usize, b: usize) -> Option<Hot> {
    if b <= a + th::HOT_MIN_CORE_M {
        return None;
    }
    let at = turn_in(t, a, b)?;
    let apex = t.slowest(at..b + 1);
    let v_in = t.pts[at].v;
    if v_in <= 0.1 {
        return None;
    }
    let inside = v_in - t.pts[apex].v;
    Some(Hot {
        at,
        apex,
        inside_kmh: inside * KMH,
        share: inside / v_in,
        braking_s: t.time_where(at..b + 1, |q| q.brake() > th::BRAKE_ON),
    })
}

/// Where a flight came down, on the jump's own terms.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Landing {
    /// On the downslope, where the landing is built to be taken.
    Ramp,
    /// Out on the flat, past the bottom of the downslope.
    Flat,
    /// On the up-face: cased.
    Face,
    /// Not readable: a hop, ground that can't be measured, or a slope in between.
    Unsure,
}

/// The gradient of the ground the bike runs on, metres of height per metre along, by least
/// squares so one bump doesn't swing it. `None` where the stretch is too short, runs off the
/// end of the lap, or has the bike in the air, since then it isn't the ground being measured.
fn slope(t: &Trace, a: usize, b: usize) -> Option<f32> {
    let b = b.min(t.len().saturating_sub(1));
    if b < a + 5 || (a..=b).any(|i| t.pts[i].air) {
        return None;
    }
    let n = (b - a + 1) as f32;
    let mid = (b - a) as f32 / 2.0;
    let mean = (a..=b).map(|i| t.pts[i].y).sum::<f32>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for i in a..=b {
        let dx = (i - a) as f32 - mid;
        num += dx * (t.pts[i].y - mean);
        den += dx * dx;
    }
    (den > 0.0).then(|| num / den)
}

/// Where this flight came down, judged against the ground rather than another lap, and the
/// landing hit in G. The bike's own height is the only terrain the app has, so the landing zone
/// is read off the ground it runs on once the suspension has settled: over ten metres that is
/// accurate to about a centimetre, an order below the thresholds.
///
/// A flat run-out alone isn't over-jumping. The bike has to have dropped onto it and hit for
/// it: a gentle descent onto flat ground is a long low jump, which is fine.
fn landing(t: &Trace, take: usize, land: usize, end: usize, bike: Bike) -> (Landing, f32) {
    let hit = t.max_by(land..(land + 8).min(end + 1).max(land + 1), |q| q.hit);
    if t.span(take, land) < th::OVERJUMP_AIR_S {
        return (Landing::Unsure, hit);
    }
    let from = land + th::LAND_SLOPE_SKIP_M;
    let Some(after) = slope(t, from, from + th::LAND_SLOPE_M) else {
        return (Landing::Unsure, hit);
    };
    // How steeply the bike was coming down over the last metres of the flight. Read straight
    // rather than through `slope`, which refuses airborne stretches on purpose.
    let lip = land.saturating_sub(3).max(take);
    let fall = if land > lip { (t.pts[land].y - t.pts[lip].y) / (land - lip) as f32 } else { 0.0 };

    let verdict = if after > th::LAND_UP {
        Landing::Face
    } else if after > th::LAND_FLAT {
        let dropped = fall < th::FALL_RATE;
        let hard = hit > bike.land_g(th::LAND_HIT_G) * th::OVERJUMP_HIT_SHARE;
        if dropped && hard { Landing::Flat } else { Landing::Unsure }
    } else if after <= th::LAND_DOWN {
        Landing::Ramp
    } else {
        Landing::Unsure
    };
    (verdict, hit)
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
        out.push(Finding { skill, title, detail, at, weight, safety: false, absolute: false });
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
                // Mostly slow hits, under braking or in turns rather than off landings: low speed.
                let mut speeds: Vec<f32> = runs
                    .iter()
                    .map(|&(a, _)| -p.pts[a.saturating_sub(2)..a + 1].iter().map(|q| q.sv_lo[k]).fold(0.0, f32::min))
                    .collect();
                speeds.sort_by(f32::total_cmp);
                let measured = p.pts.iter().any(|q| q.sv_lo[k] != 0.0);
                if k == 1 && measured && speeds[speeds.len() / 2] < th::SLOW_HIT_MS {
                    tip("setup_bottoming_shock_slow", 1.0, runs[0].0, format!("The shock bottoms {} times a lap", runs.len()), format!(
                        "It runs out of travel at {}, and slowly: under braking and in turns rather than off \
                         landings. Firm up the shock's low-speed compression first, then the spring.",
                        places.join(", ")
                    ));
                    continue;
                }
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

    if travel {
        // Diving under braking and squatting on the gas, corner after corner.
        let (mut dive, mut squat) = (Vec::new(), Vec::new());
        for s in secs.iter().filter(|s| s.kind == Kind::Corner) {
            let apex = p.slowest(s.core.0..s.core.1 + 1);
            if let Some(b) = p.first(s.start..apex + 1, |q| q.front > 0.3) {
                if (th::DIVE..th::BOTTOM).contains(&p.max_by(b..apex + 1, |q| q.used[0])) {
                    dive.push(s.name.as_str());
                }
            }
            let end = s.end.min(p.len() - 1);
            let deep = (apex..end + 1)
                .filter(|&i| p.pts[i].throttle > 0.6 && !p.pts[i].air)
                .map(|i| p.pts[i].used[1])
                .fold(0.0, f32::max);
            if (th::SQUAT..th::BOTTOM).contains(&deep) {
                squat.push(s.name.as_str());
            }
        }
        if dive.len() >= th::SETUP_TIMES {
            tip("setup_brake_dive", 0.7, 0, "The fork dives under braking".into(), format!(
                "It sinks deep into its travel braking into {}. Firmer fork compression or a little more oil \
                 holds the front up, so the bike stays level into the turn.",
                dive.join(", ")
            ));
        }
        if squat.len() >= th::SETUP_TIMES {
            tip("setup_exit_squat", 0.6, 0, "The rear squats on the gas".into(), format!(
                "The shock sinks deep as you get on the gas out of {}, so the front goes light and runs wide. \
                 Firmer low-speed compression or a little more preload keeps the rear up.",
                squat.join(", ")
            ));
        }
        // The shock kicking the rear up off jump faces.
        let kicks = air_runs(p, 0..p.len())
            .iter()
            .filter(|&&(t, _)| p.max_by(t.saturating_sub(4)..t + 1, |q| q.sv_hi[1]) > th::KICK_MS)
            .count();
        if kicks >= th::SETUP_TIMES {
            tip("setup_shock_kick", 0.6, 0, "The rear kicks off jump faces".into(), format!(
                "The shock springs back hard at the lip on {kicks} jumps, which throws the rear up. Slower shock \
                 rebound keeps the bike level off the face."
            ));
        }
        // Packing down through whoops: deep, and never coming back up quickly.
        for (k, (end, skill)) in [("fork", "setup_packing_fork"), ("shock", "setup_packing_shock")].into_iter().enumerate() {
            let packed: Vec<&str> = secs
                .iter()
                .filter(|s| s.kind == Kind::Whoops)
                .filter(|s| {
                    let e = s.end.min(p.len() - 1);
                    p.mean(s.start..e + 1, |q| q.used[k]) > th::PACK_USED
                        && p.max_by(s.start..e + 1, |q| q.sv_hi[k]) < th::PACK_EXT_MS[k]
                })
                .map(|s| s.name.as_str())
                .collect();
            if !packed.is_empty() {
                tip(skill, 0.8, 0, format!("The {end} packs down in the whoops"), format!(
                    "Through {} the {end} stays deep in its travel and doesn't come back up between hits. \
                     Faster {end} rebound lets it recover for the next one.",
                    packed.join(", ")
                ));
            }
        }
        // Ride height on steady straights: nose-high or nose-down.
        let steady: Vec<&Point> = p
            .pts
            .iter()
            .filter(|q| {
                q.v > 12.0 && q.turn.abs() < 10.0 && !q.air && !q.off[0] && !q.off[1]
                    && (0.3..0.95).contains(&q.throttle) && q.acc[2].abs() < 0.3
            })
            .collect();
        if steady.len() > 40 {
            let median = |k: usize| {
                let mut v: Vec<f32> = steady.iter().map(|q| q.used[k]).collect();
                v.sort_by(f32::total_cmp);
                v[v.len() / 2]
            };
            let diff = median(1) - median(0);
            if diff > th::RAKE_HI {
                tip("setup_rear_low", 0.5, 0, "The bike runs nose-high".into(), format!(
                    "On the straights the shock sits {:.0}% deeper in its travel than the fork, so the front is \
                     light and vague. A little more shock preload levels the bike.",
                    diff * 100.0
                ));
            } else if diff < th::RAKE_LO {
                tip("setup_front_low", 0.5, 0, "The bike runs nose-down".into(), "On the straights the fork \
                     sits deeper in its travel than the shock, so the bike is nose-down and twitchy. A little \
                     more fork preload, or less shock preload, levels it.".into());
            }
        }
    }
    // The front washing out, corner after corner, is the setup too.
    let limit = if r.is_some() { th::PUSH } else { th::SOLO_PUSH };
    let pushes = secs
        .iter()
        .filter(|s| s.kind == Kind::Corner)
        .filter(|s| {
            let core = s.core.0..s.core.1 + 1;
            p.coordination(core.clone()).is_some_and(|c| c < limit)
                && r.map_or(true, |r| r.coordination(core).is_some_and(|c| c > th::PUSH + 0.1))
        })
        .count();
    if pushes >= th::SETUP_TIMES {
        tip("setup_front_push", 0.7, 0, "The front washes out".into(), format!(
            "The front tyre slides in {pushes} corners. Softer fork compression lets it dig in, and a little \
             more shock preload puts more weight on it."
        ));
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
            SectionReview { section: s, lap_time: t, ref_time: t, lost: 0.0, findings, soil: None }
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
        traced: false,
        channels: channels(p, p, 2),
        paths: paths(p, p),
    }
}

/// A lap against a target time for each section, rather than against another lap's trace.
///
/// This is how the ideal lap is ridden against: the rider's own best sections added up, a time
/// that is real section by section but was never ridden whole. So the section times here are
/// held against the targets, while the advice is the lap's own — the same rules [`solo`] uses,
/// because there is no faster trace to say where the difference came from — and there is no
/// reference line to draw.
pub fn against_targets(lap: &Trace, secs: &[Section], targets: &[f32], bike: Bike) -> Review {
    let mut p = lap.clone();
    let travel = p.fill_travel(bike.travel);
    let p = &p;
    // A lap shorter than the grid the targets were measured on can't be timed over every
    // section; it keeps the ones it reaches rather than reading off the end.
    let last = p.len().saturating_sub(1);
    let pairs: Vec<(Section, f32)> =
        secs.iter().cloned().zip(targets.iter().copied()).filter(|(s, _)| s.end <= last).collect();
    let secs: Vec<Section> = pairs.iter().map(|(s, _)| s.clone()).collect();
    let setup = setup(p, None, &secs, bike, travel);
    let out: Vec<SectionReview> = pairs
        .into_iter()
        .map(|(s, target)| {
            let lap_time = p.span(s.start, s.end);
            // No target for a section nobody has a clean time in: it can only be itself.
            let ref_time = if target > 0.0 { target } else { lap_time };
            let mut c = Ctx { p, r: p, s: &s, bike, travel, out: Vec::new() };
            alone(&mut c);
            let mut findings = dedupe(c.out, s.kind != Kind::Corner);
            findings.sort_by(|a, b| b.weight.total_cmp(&a.weight));
            let lost = ((lap_time - ref_time) * 1000.0).round() / 1000.0;
            SectionReview { section: s, lap_time, ref_time, lost, findings, soil: None }
        })
        .collect();
    let mut order: Vec<usize> = (0..out.len()).filter(|&i| out[i].lost > th::WORTH_S).collect();
    order.sort_by(|&a, &b| out[b].lost.total_cmp(&out[a].lost));
    order.truncate(th::FOCUS);
    Review {
        lap_time: p.time(),
        ref_time: out.iter().map(|s| s.ref_time).sum(),
        overall: overall(&out, false),
        sections: out,
        focus: order,
        setup,
        solo: false,
        traced: false,
        channels: one_sided(p),
        paths: Paths { reference: Vec::new(), reference_y: Vec::new(), ..paths(p, p) },
    }
}

/// The lap's own traces with nothing beside them: there is no reference lap to draw.
fn one_sided(p: &Trace) -> Channels {
    let mut c = channels(p, p, 2);
    c.delta.clear();
    for ch in
        [&mut c.speed, &mut c.throttle, &mut c.brake, &mut c.lean, &mut c.gear, &mut c.height, &mut c.fork, &mut c.shock]
    {
        ch.reference.clear();
    }
    c
}

/// The rules that need no faster lap.
fn alone(c: &mut Ctx) {
    let (p, s) = (c.p, c.s);
    let name = s.name.clone();
    let (start, end) = (s.start, s.end.max(s.start + 1));
    match s.kind {
        Kind::Corner => {
            let apex = p.slowest(s.core.0.max(start)..s.core.1.min(end) + 1);
            let exit = apex..end;
            let slip = p.max_by(exit.clone(), |q| if q.air || q.v < 5.0 { 0.0 } else { q.slip_r });
            let gas = p.mean(exit.clone(), |q| q.throttle);
            if exit.len() >= th::SOLO_EXIT_M
                && gas < th::SOLO_GAS
                && slip < th::GRIP_LEFT_SLIP
                && p.time_where(exit.clone(), |q| q.off[0] && !q.off[1]) <= th::WHEELIE_S
            {
                c.add("throttle_room", 0.6, apex, "Hold more throttle", format!(
                    "Out of {name} you use {:.0}% throttle and the rear never slips. The grip is there: roll \
                     it on further and hold it.",
                    gas * 100.0
                ));
            }
            if p.coordination(s.core.0..s.core.1 + 1).is_some_and(|cp| cp < th::SOLO_PUSH) {
                c.warn("front_push", apex, "The front is washing out", format!(
                    "Through {name} the bike leans more than it turns: the front tyre is sliding. Brake a \
                     touch earlier, get your weight forward over the front, and pick the bike up before the gas."
                ));
            }
            // Coming in too fast, with no lap to hold it against. The gate is raised, and the
            // "ran wide" consequence is gone with the reference line it needed.
            let (ca, cb) = (s.core.0.max(start), s.core.1.min(end));
            if let Some(h) = hot(p, ca, cb) {
                let hot_enough = h.share > th::SOLO_HOT_INSIDE_SHARE
                    && h.inside_kmh > th::HOT_INSIDE_KMH
                    && h.braking_s > th::HOT_BRAKE_S;
                let late_apex = (h.apex - ca) as f32 > (cb - ca) as f32 * th::HOT_APEX_SHARE;
                let picked_up = p.max_by(h.at..h.apex + 1, |q| q.roll.abs()) - p.pts[h.apex].roll.abs()
                    > th::HOT_STAND_DEG;
                let no_gas = p.pts[h.apex].throttle < th::THROTTLE_ON;
                let stood_up = picked_up && no_gas;
                if hot_enough && (stood_up || late_apex) {
                    let what = if stood_up {
                        "have to pick the bike up mid-corner"
                    } else {
                        "can't start driving until well past the middle of the turn"
                    };
                    c.judge("in_too_hot", 0.95, h.at, "You're coming in too fast", format!(
                        "You carry about {:.0} km/h too much into {name}: {:.0}% of the speed comes off \
                         after you have already turned in, and you {what}. Get the braking done in a \
                         straight line before turn-in, then roll through on a steady throttle.",
                        h.inside_kmh,
                        h.share * 100.0
                    ));
                }
            }
            if let Some(at) = turn_in(p, ca, cb) {
                let a2 = p.slowest(at..cb + 1);
                let early = ((a2 - ca) as f32) < (cb - ca) as f32 * th::APEX_EARLY_SHARE;
                let stuck = p.pts[cb].roll.abs() > th::TURN_IN_DEG && p.pts[cb].throttle < th::THROTTLE_ON;
                if early && stuck {
                    c.judge("apex_early", 0.7, at, "You're turning in too early", format!(
                        "Your slowest point in {name} comes in the first third of the turn, and you are \
                         still leaned over with the throttle shut on the way out. Turn in later and point \
                         the bike at the exit so you can drive off the corner."
                    ));
                }
            }
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
                let motion = air_motion(p, pt, pl);
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
                // Read the ground before judging the lean: landing cranked on an up-face can be
                // an intentional recovery, not a scrub the rider failed to bring back.
                let (verdict, hit_g) = landing(p, pt, pl, s.end, c.bike);
                let lean = p.pts[(pl + 4).min(s.end)].roll.abs();
                if verdict != Landing::Face && lean > th::SOLO_LAND_ROLL_DEG {
                    if motion.kind == AirMove::Scrub {
                        c.warn("land_crooked", pl, "Bring the scrub back sooner", format!(
                            "You get the bike rolled over, but it is still leaned {lean:.0}° after you land {which}. \
                             Counter the lean earlier in the air so it is straight for touchdown."
                        ));
                    } else {
                        c.warn("land_crooked", pl, "Straighten up before landing", format!(
                            "The bike is still leaned {lean:.0}° after you land {which}. Bring it straight in the air, a \
                             moment earlier."
                        ));
                    }
                }
                // The ground says where this came down, with no lap to hold it against, which
                // is the whole point of reading the landing rather than a pairing.
                match verdict {
                    Landing::Flat => c.judge("overjump", 0.9, pt, "You're over-jumping this", format!(
                        "Your {} m flight over {which} finishes out on the flat, past the downslope, and \
                         hits {hit_g:.0} G for it. Ease out of the throttle a bike length before the lip, \
                         or stay seated and lean the bike over as you leave it so it stays low.",
                        pl - pt
                    )),
                    Landing::Face => c.judge("land_short", 0.9, pl, "You're casing this one", format!(
                        "You come down on the up-face of {which} and the landing stops you dead. Carry \
                         more speed up to the lip and stay on the gas all the way to it."
                    )),
                    Landing::Ramp | Landing::Unsure => {}
                }
                let hit = p.max_by(pl..(pl + 8).min(s.end + 1).max(pl + 1), |q| q.hit);
                if verdict != Landing::Flat && hit > c.bike.land_g(th::SOLO_LAND_HIT_G) {
                    c.warn("land_hard", pl, "Land softer", format!(
                        "You hit {hit:.0} G landing {which}. Aim for the downslope, and soak the landing up with \
                         your legs instead of locking your arms."
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

/// The share of these metres on the ground spent sitting, or None when too little of it is known.
fn seated(t: &Trace, range: std::ops::Range<usize>) -> Option<f32> {
    use crate::telemetry::stance;
    let ground: Vec<&Point> = t.pts.get(range)?.iter().filter(|q| !q.air).collect();
    let known: Vec<bool> = ground.iter().filter(|q| q.stance != stance::UNKNOWN).map(|q| q.stance == stance::SIT).collect();
    if ground.len() < 5 || (known.len() as f32) < ground.len() as f32 * th::STANCE_KNOWN {
        return None;
    }
    Some(known.iter().filter(|&&s| s).count() as f32 / known.len() as f32)
}

/// Sitting and standing against the fast lap, where the recorder could tell: seated from
/// turn-in through a corner, standing through whoops and rhythms.
fn stance(c: &mut Ctx) {
    let s = c.s;
    let from = match s.kind {
        Kind::Corner => s.core.0.max(s.start),
        Kind::Whoops | Kind::Rhythm => s.start,
        _ => return,
    };
    if s.end <= from {
        return;
    }
    let (Some(p), Some(r)) = (seated(c.p, from..s.end + 1), seated(c.r, from..s.end + 1)) else { return };
    let (name, pc, rc) = (s.name.clone(), p * 100.0, r * 100.0);
    if s.kind == Kind::Corner && r - p > th::STANCE_GAP {
        c.add("stance_sit", 0.4, from, "Sit down through the turn", format!(
            "You sit for {pc:.0}% of {name} from turn-in, the fast lap {rc:.0}%. Sit on the front of the seat as \
             you turn in: it weights the front tyre and frees your inside leg."
        ));
    } else if s.kind != Kind::Corner && p - r > th::STANCE_GAP {
        c.add("stance_stand", 0.5, from, "Stand up through here", format!(
            "You sit for {pc:.0}% of {name}, the fast lap {rc:.0}%. Stand with your weight back and let the bike \
             move under you: your legs soak up the hits and the rear keeps driving."
        ));
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
    /// The section's stable id: what per-corner progress across sessions keys on.
    pub id: String,
    pub name: String,
    pub best: f32,
    /// The lap it came from, as the caller keys its laps.
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
            SectionBest { id: s.id.clone(), name: s.name.clone(), best, lap, spread }
        })
        .collect();
    let least_consistent = (laps.len() > 2)
        .then(|| (0..bests.len()).max_by(|&a, &b| bests[a].spread.total_cmp(&bests[b].spread)))
        .flatten();
    Some(Ideal { time: bests.iter().map(|b| b.best).sum(), sections: bests, least_consistent })
}

/// Cue kinds, numbered as the recorder plugin numbers them (FrostMod `src/coachcue.h`).
pub(crate) mod cue {
    pub const BRAKE: u8 = 1;
    pub const OFF_BRAKES: u8 = 2;
    pub const THROTTLE: u8 = 3;
    pub const UPSHIFT: u8 = 4;
    pub const DOWNSHIFT: u8 = 5;
    pub const WIDE: u8 = 6;
    pub const INSIDE: u8 = 7;
    pub const SCRUB: u8 = 8;
    pub const STAND: u8 = 9;
    pub const SIT: u8 = 10;
    // 11 is the plugin's CUSTOM: drawn, spoken by nothing. A kind past it needs a clip of its
    // own added to the plugin's set, or it draws and stays silent the same way.
    pub const ROLL: u8 = 12;
}

/// A place the fast lap does something a live cue calls, metres into the lap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CuePoint {
    pub at: usize,
    pub kind: u8,
    pub section: usize,
}

/// Where the fast lap brakes, lets off, shifts, gets back on the gas, scrubs and stands, section
/// by section, in lap order.
pub(crate) fn cue_points(r: &Trace, secs: &[Section]) -> Vec<CuePoint> {
    let mut out = Vec::new();
    let last = r.len().saturating_sub(1);
    for (si, s) in secs.iter().enumerate() {
        let (start, end) = (s.start.min(last), s.end.min(last));
        let (a, b) = (s.core.0.min(last), s.core.1.min(last));
        let mut add = |at: usize, kind: u8| out.push(CuePoint { at, kind, section: si });
        match s.kind {
            Kind::Corner => {
                let apex = r.slowest(a..b + 1);
                if let Some(on) = r.first(start..apex + 1, |q| q.brake() > th::BRAKE_ON) {
                    add(on, cue::BRAKE);
                    if let Some(off) = r.first(on..apex + 1, |q| q.brake() <= th::BRAKE_ON) {
                        if off > on + 3 {
                            add(off, cue::OFF_BRAKES);
                        }
                    }
                }
                add(a, cue::SIT);
                let held = |i: usize| (i..(i + th::THROTTLE_HOLD_M).min(end)).all(|j| r.pts[j].throttle > th::THROTTLE_ON);
                if let Some(g) = (apex..end).find(|&i| held(i)) {
                    add(g, cue::THROTTLE);
                }
                // No shift cue here. Where the fast lap happens to change gear says nothing
                // about whether the rider's gear is costing them anything, and "shift earlier"
                // called at a corner a rider is carrying speed through reads as "go slower".
                // Shift cues come from the gearing rules instead, in `cues::pick`.
            }
            Kind::Jump | Kind::Rhythm => {
                if s.kind == Kind::Rhythm {
                    add(start, cue::STAND);
                }
                if let Some(&(t, l)) = s.runs.first() {
                    if air_motion(r, t, l).kind == AirMove::Scrub {
                        add(t, cue::SCRUB);
                    }
                }
            }
            Kind::Whoops => {
                add(start, cue::STAND);
                add(a, cue::THROTTLE);
            }
            Kind::Straight => {}
        }
    }
    out.sort_by_key(|c| c.at);
    out
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
        /// The hardest vertical hit on the first jump's landing, G.
        pub(crate) hit: f32,
        /// Force on the bars through the corners.
        pub(crate) torque: f32,
        /// How much the bike turns for its lean in the corners: 1 clean, under 1 the front slides.
        pub(crate) push: f32,
        /// Throttle while driving out of a corner, and the acceleration it gives, m/s².
        pub(crate) gas: f32,
        pub(crate) accel: f32,
        /// The landing hill: where its crest sits along the lap, how steeply the ground falls
        /// away past it in metres of height per metre along, and how far that downslope runs
        /// before the ground goes flat. The up-face climbs to the crest at twice that slope over
        /// a shorter face, the way a real landing is built. So a lap can come down on the
        /// up-face (cased), on the downslope (right), or out on the flat past the bottom of it
        /// (over-jumped). (0, 0, 0) for the flat ground the fixture had before it had terrain.
        pub(crate) hill: (f32, f32, f32),
        /// Metres of the braking carried past turn-in. 0 for braking finished before the turn.
        pub(crate) hot: f32,
        /// The brake dragged through the whole turn at a steady speed, as a rider does in a
        /// rut: lever position, no speed lost to it. 0 for none.
        pub(crate) drag: f32,
    }

    /// The height of the ground at `d`: flat, but for the landing hill when the style has one.
    /// The rut under Turn 1 is applied by the caller on top of this.
    pub(crate) fn ground_y(st: &Style, d: f32) -> f32 {
        let (crest, slope, down) = st.hill;
        if slope <= 0.0 || down <= 0.0 {
            return 0.0;
        }
        let top = down * slope;
        let up = top / (slope * 2.0);
        if d <= crest - up {
            0.0
        } else if d <= crest {
            (d - (crest - up)) * slope * 2.0
        } else if d <= crest + down {
            top - (d - crest) * slope
        } else {
            0.0
        }
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
        hit: 4.0,
        torque: 15.0,
        push: 1.0,
        gas: 1.0,
        accel: 6.0,
        // The fast lap's 330 → 350 jump comes down four metres past the crest, on the
        // downslope, which is where a landing is meant to be taken.
        hill: (346.0, 0.18, 26.0),
        hot: 0.0,
        drag: 0.0,
    };
    const BIKE: Bike =
        Bike { limiter: 13000.0, max_rpm: 14000.0, shift_rpm: 12500.0, travel: [0.3, 0.3], land_scale: 0.0, torque_scale: 0.0 };

    #[test]
    fn standing_where_the_fast_lap_sits_is_called_only_when_known() {
        use crate::telemetry::stance;
        let slow = Style { corner_v: 9.0, ..FAST };
        let with = |st: &Style, s: u8| {
            let mut t = lap(st);
            t.pts.iter_mut().for_each(|q| q.stance = s);
            t
        };
        let called = |rv: &Review| rv.sections.iter().any(|s| s.findings.iter().any(|f| f.skill == "stance_sit"));
        assert!(called(&review(&with(&slow, stance::STAND), &with(&FAST, stance::SIT), BIKE)));
        // Sitting like the fast lap, or no stance recorded: nothing to say.
        assert!(!called(&review(&with(&slow, stance::SIT), &with(&FAST, stance::SIT), BIKE)));
        assert!(!called(&review(&with(&slow, stance::UNKNOWN), &with(&FAST, stance::SIT), BIKE)));
    }

    /// The ideal lap is a target time per section and no lap at all, so the times are held
    /// against it while the charts have nothing to draw beside this lap.
    #[test]
    fn a_lap_against_targets_is_timed_on_them_and_draws_no_reference() {
        let slow = lap(&Style { corner_v: 9.0, ..FAST });
        let fast = lap(&FAST);
        let secs = sections(&fast);
        let targets: Vec<f32> = secs.iter().map(|s| fast.span(s.start, s.end)).collect();
        let rv = against_targets(&slow, &secs, &targets, BIKE);
        assert_eq!(rv.sections.len(), secs.len());
        assert!(!rv.solo, "the times are real, so it isn't a lap reviewed on its own");
        assert!(!rv.traced, "but nobody rode the ideal lap, so there's no line to draw");
        assert!(rv.paths.reference.is_empty() && rv.paths.reference_y.is_empty());
        assert!(rv.channels.speed.reference.is_empty() && rv.channels.delta.is_empty());
        assert!(!rv.paths.lap.is_empty(), "this lap is still drawn");
        assert!((rv.ref_time - targets.iter().sum::<f32>()).abs() < 1e-3);
        let lost: f32 = rv.sections.iter().map(|s| s.lost).sum();
        assert!(lost > 0.0, "a slower lap loses time to the target: {lost}");
        // A section nobody has a clean time in can only be itself.
        let rv = against_targets(&slow, &secs, &vec![0.0; secs.len()], BIKE);
        assert!(rv.sections.iter().all(|s| s.lost == 0.0));
    }

    #[test]
    fn the_floors_follow_the_riders_own_session() {
        assert_eq!(norm(&[]), (0.0, 0.0), "nothing to go on keeps the tuned floors");
        let session = |st: &Style| (0..10).map(|_| lap(st)).collect::<Vec<_>>();
        let soft = norm(&session(&FAST));
        let hard = norm(&session(&Style { hit: 20.0, ..FAST }));
        assert!(hard.0 >= soft.0, "{soft:?} {hard:?}");
        for x in [soft.0, soft.1, hard.0, hard.1] {
            assert!(x == 0.0 || (th::SCALE.0..=th::SCALE.1).contains(&x), "{x}");
        }
        // A rider who always lands this hard isn't told off for it at the tuned floor.
        let b = Bike { land_scale: th::SCALE.1, ..BIKE };
        assert!(b.land_g(th::LAND_HIT_G) > th::LAND_HIT_G);
        assert_eq!(BIKE.land_g(th::LAND_HIT_G), th::LAND_HIT_G);
    }

    /// Speed, throttle, brake and lean at `d`.
    fn ride(st: &Style, d: f32) -> (f32, f32, f32, f32) {
        let arc = PI * R;
        let (c1, c1e, c2) = (200.0, 200.0 + arc, 400.0 + arc);
        let cv2 = st.corner_v * st.corner_v;
        if (c1..c1e).contains(&d) || d >= c2 {
            let into = if d >= c2 { d - c2 } else { d - c1 };
            // Braking carried past turn-in: the bike arrives above `corner_v` and the rest of
            // the speed comes off inside the turn, on the brake, the way a rider who came in
            // hot has to. The curve joins the approach exactly at the corner's start.
            if st.hot > 0.0 && into < st.hot {
                let target = (if d >= c2 { c2 } else { c1 }) + st.hot;
                return ((cv2 + 2.0 * st.decel * (target - d)).sqrt(), 0.0, st.brake, 35.0);
            }
            return (st.corner_v, 0.3, st.drag, 35.0);
        }
        // With the braking carried in, the approach aims at a point inside the corner.
        let (exit, next) = if d < c1 { (0.0, c1 + st.hot) } else { (c1e, c2 + st.hot) };
        let accel = (cv2 + st.accel * (d - exit)).sqrt();
        let braking = (cv2 + 2.0 * st.decel * (next - d)).sqrt();
        if braking < accel && braking < 20.0 {
            (braking, 0.0, st.brake, 0.0)
        } else if accel < 20.0 {
            (accel, st.gas, 0.0, 0.0)
        } else {
            (20.0, 0.9, 0.0, 0.0)
        }
    }

    /// A lean axis can be unknown at either end of a grid step. Unknown must not spread: if
    /// NaN went through the ordinary lerp, every metre between two readings would blank, and a
    /// rider whose stick was readable for most of a lap would come back with nothing.
    #[test]
    fn an_unknown_lean_does_not_blank_the_metres_around_it() {
        use crate::telemetry::lean;
        let pt = |lr: f32| Point { lean: [lr, f32::NAN], ..Point::default() };

        // Both known: an ordinary reading in between.
        let mid = lerp(&pt(-1.0), &pt(1.0), 0.5);
        assert!(mid.lean[lean::LR].abs() < 1e-6, "got {}", mid.lean[lean::LR]);

        // One known: the reading we have, rather than nothing.
        let from = lerp(&pt(0.5), &pt(f32::NAN), 0.75);
        assert!((from.lean[lean::LR] - 0.5).abs() < 1e-6, "got {}", from.lean[lean::LR]);
        let to = lerp(&pt(f32::NAN), &pt(-0.25), 0.25);
        assert!((to.lean[lean::LR] + 0.25).abs() < 1e-6, "got {}", to.lean[lean::LR]);

        // Neither: still nothing, and never a centred rider.
        let none = lerp(&pt(f32::NAN), &pt(f32::NAN), 0.5);
        assert!(!lean::known(none.lean[lean::LR]));
        // The axis that was never read stays unknown throughout.
        assert!(!lean::known(mid.lean[lean::FB]));
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
            let in_roll = st.whip > 0.0 && d >= take && d <= land + 2.0;
            let roll = if in_roll {
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
            // The flight arcs ride on top of the ground, so a lap leaves the lip and touches
            // down at ground height and the landing hill is there to be read after touchdown.
            let g = ground_y(st, d);
            s.y = if in_hop {
                g + hp * (1.0 - ((d - (h0 + h1) / 2.0) / ((h1 - h0) / 2.0)).powi(2))
            } else if air {
                g + peak * (1.0 - ((d - mid) / half).powi(2))
            } else if turn1 {
                g - st.sink
            } else {
                g
            };
            s.vel = [v * bearing.sin(), 0.0, v * bearing.cos()];
            s.speed = v;
            s.throttle = throttle;
            s.front_brake = brake;
            s.rear_brake = brake * 0.4;
            s.wheel_speed = [v, v];
            s.wheel_material = if air { [0, 0] } else { [1, 1] };
            s.roll = roll;
            if in_roll {
                s.roll_rate = st.whip * PI / (land + 2.0 - take)
                    * (PI * (d - take) / (land + 2.0 - take)).cos()
                    * v;
            }
            s.susp = [0.30 - squash; 2];
            let landing = !air && d > land && d <= land + 3.0;
            s.acc = [0.0, if landing { st.hit } else { 1.0 }, 0.0];
            // A clean turn: speed × heading rate = g × tan(lean), reported about the leaned axis.
            let cornering = roll.abs() > 15.0 && !air;
            s.yaw_rate = if cornering {
                (G * roll.to_radians().tan() / v.max(1.0) * st.push).to_degrees() * roll.to_radians().cos()
            } else {
                0.0
            };
            // The bars work from turn-in, under the brakes, through the corner.
            s.steer_torque = if cornering || brake > 0.0 { st.torque } else { 0.0 };
            s.gear = 3;
            s.rpm = 8000.0;
            samples.push(s);
            d += v * dt;
            t += dt;
        }
        (samples, t)
    }

    /// A corner is only called what the reading is clear about. Everything here is built from
    /// points directly rather than from a generated lap, because what is under test is the
    /// classifier's own thresholds, not how a lap is made.
    #[test]
    fn a_corner_is_only_called_what_the_reading_is_clear_about() {
        let corner = |roll: f32, hit: f32, ground: u8| {
            let pts: Vec<Point> =
                (0..40).map(|_| Point { roll, hit, ground, ..Point::default() }).collect();
            corner_type(&Trace { pts }, (0, 39))
        };

        // Leaned right over: something is holding the bike. Barely leaned: nothing is.
        assert_eq!(corner(70.0, 1.0, 12).hold, Hold::Rutted);
        assert_eq!(corner(40.0, 1.0, 12).hold, Hold::Flat);
        // In between is not guessed at, in either direction.
        assert_eq!(corner(52.0, 1.0, 12).hold, Hold::Unknown);

        assert_eq!(corner(70.0, 2.5, 12).bumps, Bumps::Rough);
        assert_eq!(corner(70.0, 0.8, 12).bumps, Bumps::Smooth);
        assert_eq!(corner(70.0, 1.5, 12).bumps, Bumps::Unknown);

        // Lean is read whichever way the corner goes.
        assert_eq!(corner(-70.0, 1.0, 12).hold, Hold::Rutted);

        // The ground is the one under most of the corner: 12 is hardpack, 6 sand, 11 soft.
        assert_eq!(corner(70.0, 1.0, 12).soil, Some(crate::soil::Soil::Hardpack));
        assert_eq!(corner(70.0, 1.0, 6).soil, Some(crate::soil::Soil::Sand));
        assert_eq!(corner(70.0, 1.0, 11).soil, Some(crate::soil::Soil::Soft));
    }

    /// The things that must not turn a corner into something it isn't.
    #[test]
    fn a_corner_type_is_not_led_astray() {
        let point = |roll: f32, hit: f32, air: bool| Point { roll, hit, air, ground: if air { 0 } else { 12 }, ..Point::default() };

        // One bad landing does not make a corner rough: the hit is averaged, not peaked. Peak
        // was tried first and moved by more than 2 G between two sessions on the same corner.
        let mut pts: Vec<Point> = (0..40).map(|_| point(70.0, 0.9, false)).collect();
        pts[20] = point(70.0, 30.0, false);
        let spike = corner_type(&Trace { pts }, (0, 39));
        assert_ne!(spike.bumps, Bumps::Rough, "one spike is not a rough corner");

        // Air is not ground: a jump through a corner must not decide what the corner is made
        // of, and the lean a rider carries in the air is not the ground holding them.
        let mut pts: Vec<Point> = (0..40).map(|_| point(40.0, 0.9, false)).collect();
        for p in pts.iter_mut().take(30) {
            *p = point(80.0, 0.9, true);
        }
        let flown = corner_type(&Trace { pts }, (0, 39));
        assert_eq!(flown.hold, Hold::Flat, "the leaning was all in the air");
        assert_eq!(flown.soil, Some(crate::soil::Soil::Hardpack), "the ground is what it touched");

        // Nothing to read is unknown, never a guess.
        let air: Vec<Point> = (0..40).map(|_| point(80.0, 0.9, true)).collect();
        let all_air = corner_type(&Trace { pts: air }, (0, 39));
        assert_eq!((all_air.hold, all_air.bumps, all_air.soil), (Hold::Unknown, Bumps::Unknown, None));

        // Too short to say anything about.
        let few: Vec<Point> = (0..2).map(|_| point(80.0, 3.0, false)).collect();
        assert_eq!(corner_type(&Trace { pts: few }, (0, 1)), CornerType::default());
    }

    #[test]
    fn lynds_six_corner_names_are_composed_without_forcing_a_guess() {
        use crate::soil::Soil;
        let named = |hold, bumps, soil, profile, shape| named_corner(hold, bumps, soil, profile, shape);

        assert_eq!(named(Hold::Flat, Bumps::Smooth, Some(Soil::Hardpack), Profile::Unknown, Shape::Constant), CornerKind::Flat);
        assert_eq!(named(Hold::Rutted, Bumps::Smooth, Some(Soil::Soft), Profile::Rut, Shape::Constant), CornerKind::SmoothRut);
        assert_eq!(named(Hold::Rutted, Bumps::Smooth, Some(Soil::Soft), Profile::Rut, Shape::Hooked), CornerKind::HookedRut);
        assert_eq!(named(Hold::Rutted, Bumps::Rough, Some(Soil::Soft), Profile::Rut, Shape::Constant), CornerKind::RoughRut);
        assert_eq!(named(Hold::Rutted, Bumps::Rough, Some(Soil::Sand), Profile::Rut, Shape::Constant), CornerKind::WhoopedSand);
        assert_eq!(named(Hold::Rutted, Bumps::Smooth, Some(Soil::Hardpack), Profile::Berm, Shape::Constant), CornerKind::SmoothBerm);

        assert_eq!(named(Hold::Rutted, Bumps::Smooth, Some(Soil::Soft), Profile::Unknown, Shape::Constant), CornerKind::Unknown);
        assert_eq!(named(Hold::Unknown, Bumps::Rough, Some(Soil::Soft), Profile::Rut, Shape::Hooked), CornerKind::Unknown);
    }

    #[test]
    fn corner_vertical_profile_removes_the_grade_before_naming_rut_or_berm() {
        let classify = |middle: f32, grade: f32| {
            let pts: Vec<Point> = (0..41)
                .map(|i| {
                    let f = i as f32 / 40.0;
                    let shape = middle * (PI * f).sin();
                    Point { x: i as f32, y: grade * f + shape, roll: 70.0, hit: 0.8, ground: 12, ..Point::default() }
                })
                .collect();
            corner_type(&Trace { pts }, (0, 40))
        };

        let rut = classify(-0.6, 2.0);
        assert_eq!((rut.profile, rut.kind), (Profile::Rut, CornerKind::SmoothRut));
        let berm = classify(0.6, -2.0);
        assert_eq!((berm.profile, berm.kind), (Profile::Berm, CornerKind::SmoothBerm));
        let grade_only = classify(0.0, 2.0);
        assert_eq!((grade_only.profile, grade_only.kind), (Profile::Unknown, CornerKind::Unknown));
    }

    #[test]
    fn a_hook_requires_path_curvature_to_rise_towards_the_exit() {
        let trace = |hooked: bool| {
            let mut pts = Vec::new();
            let (mut x, mut z, mut heading) = (0.0f32, 0.0f32, 0.0f32);
            for i in 0..60 {
                let k = if hooked && i >= 40 { 0.05 } else { 0.015 };
                heading += k;
                x += heading.sin();
                z += heading.cos();
                let f = i as f32 / 59.0;
                pts.push(Point {
                    x,
                    y: -0.6 * (PI * f).sin(),
                    z,
                    roll: 70.0,
                    hit: 0.8,
                    ground: 12,
                    ..Point::default()
                });
            }
            corner_type(&Trace { pts }, (0, 59))
        };

        let hooked = trace(true);
        assert_eq!((hooked.shape, hooked.kind), (Shape::Hooked, CornerKind::HookedRut));
        let constant = trace(false);
        assert_eq!((constant.shape, constant.kind), (Shape::Constant, CornerKind::SmoothRut));
    }

    /// Prints what every corner in some real recordings looks like, so the corner-type
    /// thresholds are set against real riding rather than guessed. Reads every `.mxbc` in
    /// `$COACH_LAPS` and, for each corner of each comparable lap, the numbers a classifier
    /// would have to work from.
    ///
    /// The numbers the landing and turn-in verdicts key on, over real recordings, so the
    /// thresholds are chosen rather than argued. Prints one row per flight and per corner with
    /// everything the rules read, plus the verdict each one currently earns.
    ///
    /// `COACH_LAPS=<dir> cargo test -p mxb-coach --bin mxb-coach tune_thresholds -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn tune_thresholds() {
        let Ok(dir) = std::env::var("COACH_LAPS") else { return };
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "mxbc"))
            .collect();
        files.sort();

        let mut afters: Vec<f32> = Vec::new();
        let mut falls: Vec<f32> = Vec::new();
        let mut hits: Vec<f32> = Vec::new();
        let mut shares: Vec<f32> = Vec::new();
        let mut verdicts = [0usize; 4];

        for path in files {
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(rec) = crate::telemetry::parse(&bytes) else { continue };
            let e = &rec.event;
            let traces: Vec<Trace> =
                rec.laps().iter().filter_map(|l| Trace::new(l, e.track_length)).collect();
            let (land_scale, torque_scale) = norm(&traces);
            let bike = Bike {
                limiter: e.limiter as f32,
                max_rpm: e.max_rpm as f32,
                shift_rpm: e.shift_rpm as f32,
                travel: e.susp_max_travel,
                land_scale,
                torque_scale,
            };
            let usable: Vec<_> = rec.laps().into_iter().filter(|l| l.whole && !l.invalid).collect();
            let Some(fast) = usable.iter().min_by_key(|l| l.time_ms).and_then(|l| Trace::new(l, e.track_length))
            else {
                continue;
            };
            let secs = sections(&fast);
            println!(
                "\n== {} | {} on {} | travel {:.3}/{:.3} m | land_scale {:.2}",
                path.file_name().unwrap().to_string_lossy(),
                e.bike_name,
                e.track_name,
                e.susp_max_travel[0],
                e.susp_max_travel[1],
                land_scale
            );

            println!("  {:>7} {:>5} {:>6} {:>6} {:>7} {:>7} {:>6}  verdict", "sect", "lap", "air s", "len m", "after", "fall", "hit G");
            for (li, t) in traces.iter().enumerate() {
                for s in secs.iter().filter(|s| matches!(s.kind, Kind::Jump | Kind::Rhythm)) {
                    for (pt, pl) in air_runs(t, s.start..s.end + 1) {
                        let from = pl + th::LAND_SLOPE_SKIP_M;
                        let after = slope(t, from, from + th::LAND_SLOPE_M);
                        let lip = pl.saturating_sub(3).max(pt);
                        let fall = if pl > lip {
                            (t.pts[pl].y - t.pts[lip].y) / (pl - lip) as f32
                        } else {
                            0.0
                        };
                        let (v, hit) = landing(t, pt, pl, s.end, bike);
                        verdicts[v as usize] += 1;
                        if let Some(a) = after {
                            afters.push(a);
                        }
                        falls.push(fall);
                        hits.push(hit);
                        println!(
                            "  {:>7} {:>5} {:>6.2} {:>6} {:>7} {:>7.3} {:>6.1}  {:?}",
                            s.name,
                            li,
                            t.span(pt, pl),
                            pl - pt,
                            after.map_or("  --".to_string(), |a| format!("{a:.3}")),
                            fall,
                            hit,
                            v
                        );
                    }
                }
            }

            println!("  {:>7} {:>5} {:>7} {:>8} {:>8}  gated", "corner", "lap", "share", "in km/h", "brake s");
            for (li, t) in traces.iter().enumerate() {
                for s in secs.iter().filter(|s| s.kind == Kind::Corner) {
                    let (a, b) = (s.core.0.min(t.len() - 1), s.core.1.min(t.len() - 1));
                    let Some(h) = hot(t, a, b) else { continue };
                    shares.push(h.share);
                    let gated = h.share > th::HOT_INSIDE_SHARE
                        && h.inside_kmh > th::HOT_INSIDE_KMH
                        && h.braking_s > th::HOT_BRAKE_S;
                    println!(
                        "  {:>7} {:>5} {:>7.3} {:>8.1} {:>8.2}  {}",
                        s.name, li, h.share, h.inside_kmh, h.braking_s, gated
                    );
                }
            }
        }

        // The number that actually decides whether a rule cries wolf: how often it reaches the
        // rider, gate and consequence and all, over every real lap held against the fast one.
        {
            let mut laps = 0usize;
            let mut tally: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            let mut files: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "mxbc"))
                .collect();
            files.sort();
            for path in files {
                let Ok(bytes) = std::fs::read(&path) else { continue };
                let Ok(rec) = crate::telemetry::parse(&bytes) else { continue };
                let e = &rec.event;
                let traces: Vec<Trace> =
                    rec.laps().iter().filter_map(|l| Trace::new(l, e.track_length)).collect();
                let (land_scale, torque_scale) = norm(&traces);
                let bike = Bike {
                    limiter: e.limiter as f32,
                    max_rpm: e.max_rpm as f32,
                    shift_rpm: e.shift_rpm as f32,
                    travel: e.susp_max_travel,
                    land_scale,
                    torque_scale,
                };
                let usable: Vec<_> = rec.laps().into_iter().filter(|l| l.whole && !l.invalid).collect();
                let Some(best) = usable.iter().min_by_key(|l| l.time_ms) else { continue };
                let Some(fast) = Trace::new(best, e.track_length) else { continue };
                for l in usable.iter().filter(|l| l.num != best.num) {
                    let Some(t) = Trace::new(l, e.track_length) else { continue };
                    laps += 1;
                    for s in &review(&t, &fast, bike).sections {
                        for f in &s.findings {
                            *tally.entry(f.skill).or_default() += 1;
                        }
                    }
                }
            }
            println!("\n== how often each tip reaches the rider, over {laps} real laps");
            let mut rows: Vec<_> = tally.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            for (skill, n) in rows {
                let per = n as f32 / laps.max(1) as f32;
                let flag = if per > 2.0 { "  <-- every lap, several times" } else { "" };
                println!("  {skill:>28}  {n:>4}   {per:>5.2} per lap{flag}");
            }
        }

        let pct = |v: &mut Vec<f32>, p: f32| {
            if v.is_empty() {
                return f32::NAN;
            }
            v.sort_by(f32::total_cmp);
            v[((v.len() - 1) as f32 * p).round() as usize]
        };
        println!("\n== distributions over every flight and corner");
        println!(
            "  after-touchdown slope  p10 {:.3}  p50 {:.3}  p90 {:.3}   (LAND_DOWN {} / LAND_FLAT {} / LAND_UP {})",
            pct(&mut afters, 0.1), pct(&mut afters, 0.5), pct(&mut afters, 0.9),
            th::LAND_DOWN, th::LAND_FLAT, th::LAND_UP
        );
        println!(
            "  descent over the lip   p10 {:.3}  p50 {:.3}  p90 {:.3}   (FALL_RATE {})",
            pct(&mut falls, 0.1), pct(&mut falls, 0.5), pct(&mut falls, 0.9), th::FALL_RATE
        );
        println!(
            "  landing hit, G         p50 {:.1}  p90 {:.1}  p99 {:.1}   (floor {:.1})",
            pct(&mut hits, 0.5), pct(&mut hits, 0.9), pct(&mut hits, 0.99), th::LAND_HIT_G
        );
        println!(
            "  speed off after turn-in p50 {:.3}  p90 {:.3}  p99 {:.3}   (HOT_INSIDE_SHARE {})",
            pct(&mut shares, 0.5), pct(&mut shares, 0.9), pct(&mut shares, 0.99), th::HOT_INSIDE_SHARE
        );
        println!("  verdicts: Ramp {} | Flat {} | Face {} | Unsure {}", verdicts[0], verdicts[1], verdicts[2], verdicts[3]);
    }

    /// `COACH_LAPS=<dir> cargo test -p mxb-coach inspect_real_corners -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn inspect_real_corners() {
        let Ok(dir) = std::env::var("COACH_LAPS") else { return };
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "mxbc"))
            .collect();
        files.sort();
        for path in files {
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let rec = match crate::telemetry::parse(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    println!("{}: unreadable: {e}", path.display());
                    continue;
                }
            };
            let laps = rec.laps();
            let usable: Vec<_> = laps.iter().filter(|l| l.whole && !l.invalid).collect();
            println!(
                "\n== {} | {} on {} | {:.0} m | {} laps, {} comparable",
                path.file_name().unwrap().to_string_lossy(),
                rec.event.bike_name,
                rec.event.track_name,
                rec.event.track_length,
                laps.len(),
                usable.len()
            );
            for l in &laps {
                println!("   lap {:>2}  {:>7.3}s  whole={} issue={:?} crashed={}",
                    l.num, l.time_ms as f32 / 1000.0, l.whole, l.issue, l.crashed);
            }
            let Some(fast) = usable
                .iter()
                .min_by_key(|l| l.time_ms)
                .and_then(|l| Trace::new(l, rec.event.track_length))
            else {
                continue;
            };
            println!("   {:>8} {:>7} {:>11} | {:>10} {:>5} {:>4} {:>6} {:>6} {:>6} {:>7} {:>7} {:>7} {:>6}",
                "hold", "bumps", "soil",
                "corner", "len", "dir", "roll", "rollm", "vmin", "sv~", "hit~", "drop/m", "grnd");
            for sc in sections(&fast).iter().filter(|s| s.kind == Kind::Corner) {
                let (a, b) = sc.core;
                let (a, b) = (a.min(fast.len() - 1), b.min(fast.len() - 1));
                if b <= a + 2 {
                    continue;
                }
                let core = &fast.pts[a..=b];
                let pk = |f: fn(&Point) -> f32| core.iter().map(f).fold(0.0f32, |m, v| m.max(v.abs()));
                let n = core.len() as f32;
                let avg = |f: fn(&Point) -> f32| core.iter().map(|q| f(q).abs()).sum::<f32>() / n;
                let roll = pk(|q| q.roll);
                let rollm = avg(|q| q.roll);
                let vmin = core.iter().map(|q| q.v).fold(f32::MAX, f32::min);
                let svhi = avg(|q| q.sv[0].abs().max(q.sv[1].abs()));
                let hit = avg(|q| q.hit);
                let _ = pk;
                // A rut sits below the way in and out; a berm rises above it.
                let ends = (fast.pts[a].y + fast.pts[b].y) * 0.5;
                let low = core.iter().map(|q| q.y).fold(f32::MAX, f32::min);
                let drop = ends - low;
                // Hooked: it turns harder on the way out than on the way in.
                let third = (b - a) / 3;
                let mean = |r: std::ops::Range<usize>| {
                    let n = r.len().max(1) as f32;
                    fast.pts[r].iter().map(|q| q.turn.abs()).sum::<f32>() / n
                };
                let (first, last) = (mean(a..a + third.max(1)), mean(b - third.max(1)..b));
                let mut grounds: Vec<u8> = core.iter().map(|q| q.ground).collect();
                grounds.sort_unstable();
                grounds.dedup();
                let _ = (first, last);
                let ct = corner_type(&fast, sc.core);
                print!("   {:>8?} {:>7?} {:>11?} |", ct.hold, ct.bumps, ct.soil);
                println!(
                    "   {:>10} {:>5} {:>4} {:>6.1} {:>6.1} {:>6.1} {:>7.3} {:>7.2} {:>7.4} {:>6?}",
                    sc.name, b - a, sc.dir, roll, rollm, vmin, svhi, hit,
                    drop / (b - a).max(1) as f32,
                    grounds
                );
            }
        }
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

    #[test]
    fn the_fast_lap_says_where_to_brake_and_get_back_on_the_gas() {
        let fast = lap(&FAST);
        let secs = sections(&fast);
        let pts = cue_points(&fast, &secs);
        for (si, s) in secs.iter().enumerate().filter(|(_, s)| s.kind == Kind::Corner) {
            let brake = pts.iter().find(|p| p.section == si && p.kind == cue::BRAKE).unwrap_or_else(|| panic!("{}: {pts:?}", s.name));
            assert!(brake.at < s.core.0, "{}: brake {}", s.name, brake.at);
            // The lap can end before the last corner's exit; the gas comes on in the next lap.
            if s.end + 5 < fast.len() {
                let gas = pts.iter().find(|p| p.section == si && p.kind == cue::THROTTLE).unwrap_or_else(|| panic!("{}: {pts:?}", s.name));
                assert!(gas.at > brake.at, "{}: brake {} gas {}", s.name, brake.at, gas.at);
            }
        }
        assert!(pts.windows(2).all(|w| w[0].at <= w[1].at));
    }

    fn skills_of(rv: &Review) -> Vec<&'static str> {
        rv.sections.iter().flat_map(|s| s.findings.iter().map(|f| f.skill)).collect()
    }

    /// Lands out on the flat, well past the bottom of the downslope, and hits hard for it.
    pub(crate) const OVER: Style = Style { jump: (330.0, 375.0, 3.0), hit: 14.0, ..FAST };
    /// Comes down on the up-face of the landing. The crest sits further along than the fast
    /// lap's so the whole run-out after touchdown is still climbing, which is what being cased
    /// looks like from the ground.
    pub(crate) const CASED: Style = Style { jump: (330.0, 340.0, 1.5), hill: (352.0, 0.18, 26.0), ..FAST };

    fn titles(s: &SectionReview) -> Vec<&str> {
        s.findings.iter().map(|f| f.title.as_str()).collect()
    }

    /// Still slowing 45 m into a 63 m corner, on the brake the whole way.
    pub(crate) const HOT: Style = Style { hot: 45.0, decel: 2.0, ..FAST };

    #[test]
    fn coming_in_too_fast_is_the_speed_that_comes_out_after_turn_in() {
        let (fast, hot) = (lap(&FAST), lap(&HOT));
        let rv = review(&hot, &fast, BIKE);
        assert!(skills(section(&rv, "Turn 1")).contains(&"in_too_hot"), "{:?}", skills(section(&rv, "Turn 1")));
        assert!(!skills_of(&review(&fast, &fast, BIKE)).contains(&"in_too_hot"));
        // And with no lap to hold it against.
        let alone = solo(&hot, BIKE);
        assert!(skills(section(&alone, "Turn 1")).contains(&"in_too_hot"), "{:?}", skills(section(&alone, "Turn 1")));
    }

    #[test]
    fn dragging_the_brake_through_a_rut_is_not_coming_in_too_fast() {
        // The lever is held the whole way round and no speed comes off for it. Reading the
        // lever alone would cry wolf on every rutted corner in the game.
        let (fast, drag) = (lap(&FAST), lap(&Style { drag: 0.4, ..FAST }));
        assert!(!skills_of(&review(&drag, &fast, BIKE)).contains(&"in_too_hot"));
        assert!(!skills_of(&solo(&drag, BIKE)).contains(&"in_too_hot"));
    }

    #[test]
    fn coming_in_too_fast_is_not_also_said_as_braking_late() {
        // Same cause, same fix: saying both reads as the tip repeated.
        let rv = review(&lap(&HOT), &lap(&FAST), BIKE);
        let t1 = skills(section(&rv, "Turn 1"));
        assert!(t1.contains(&"in_too_hot"), "{t1:?}");
        assert!(!t1.contains(&"brake_late"), "{t1:?}");
    }

    #[test]
    fn landing_out_on_the_flat_is_over_jumping_with_no_fast_lap() {
        let solo_rv = solo(&lap(&OVER), BIKE);
        assert!(skills(section(&solo_rv, "Jump 1")).contains(&"overjump"), "{:?}", skills(section(&solo_rv, "Jump 1")));
        // And the fast lap, which lands on the downslope, earns no such verdict.
        let fast_rv = solo(&lap(&FAST), BIKE);
        assert!(!skills(section(&fast_rv, "Jump 1")).contains(&"overjump"));
    }

    #[test]
    fn landing_on_the_up_face_is_casing_not_over_jumping() {
        let rv = solo(&lap(&CASED), BIKE);
        let j = section(&rv, "Jump 1");
        assert!(titles(j).contains(&"You're casing this one"), "{:?}", titles(j));
        assert!(!skills(j).contains(&"overjump"), "{:?}", skills(j));
    }

    #[test]
    fn a_flat_landing_is_one_tip_not_three() {
        // The G is the symptom, over-jumping is the cause, and the over-jump detail quotes the
        // figure anyway — so `land_hard` must not be said beside it.
        let j = &section(&solo(&lap(&OVER), BIKE), "Jump 1").findings.iter().map(|f| f.skill).collect::<Vec<_>>();
        assert!(j.contains(&"overjump"), "{j:?}");
        assert!(!j.contains(&"land_hard"), "{j:?}");
        assert!(!j.contains(&"land_short"), "{j:?}");
    }

    #[test]
    fn over_jumping_is_said_where_the_jump_cost_no_time() {
        // Both laps take it the same way, so the section loses nothing and every ordinary tip
        // is dropped. The judgement is not a comparison, so it survives.
        let over = lap(&OVER);
        let rv = review(&over, &over, BIKE);
        let j = section(&rv, "Jump 1");
        assert!(j.lost.abs() < 0.05, "lost {}", j.lost);
        assert!(skills(j).contains(&"overjump"), "{:?}", skills(j));
        assert!(!skills(j).contains(&"unclear"), "{:?}", skills(j));
    }

    #[test]
    fn a_shallow_landing_gets_no_verdict() {
        // A downslope inside the dead band between flat and a proper hill: the ground alone
        // cannot settle it, so nothing absolute is said either way.
        let shallow = Style { hill: (346.0, 0.08, 26.0), hit: 14.0, ..FAST };
        let j = &section(&solo(&lap(&shallow), BIKE), "Jump 1").findings.iter().map(|f| f.skill).collect::<Vec<_>>();
        assert!(!j.contains(&"overjump"), "{j:?}");
        assert!(!titles(section(&solo(&lap(&shallow), BIKE), "Jump 1")).contains(&"You're casing this one"));
    }

    #[test]
    fn a_hard_landing_is_called_against_the_fast_lap_and_on_its_own() {
        let (fast, hard) = (lap(&FAST), lap(&Style { hit: 14.0, ..FAST }));
        assert!(skills_of(&review(&hard, &fast, BIKE)).contains(&"land_hard"));
        assert!(!skills_of(&review(&fast, &fast, BIKE)).contains(&"land_hard"));
        assert!(skills_of(&solo(&hard, BIKE)).contains(&"land_hard"));
        assert!(!skills_of(&solo(&fast, BIKE)).contains(&"land_hard"));
    }

    #[test]
    fn a_sliding_front_is_called_in_the_corner_and_in_the_setup() {
        let (fast, push) = (lap(&FAST), lap(&Style { push: 0.5, ..FAST }));
        let rv = review(&push, &fast, BIKE);
        assert!(skills_of(&rv).contains(&"front_push"));
        assert!(rv.setup.iter().any(|f| f.skill == "setup_front_push"));
        let clean = review(&fast, &fast, BIKE);
        assert!(!skills_of(&clean).contains(&"front_push"));
        assert!(!clean.setup.iter().any(|f| f.skill == "setup_front_push"));
        assert!(!solo(&fast, BIKE).setup.iter().any(|f| f.skill == "setup_front_push"));
    }

    #[test]
    fn grip_left_on_the_exit_says_hold_more_throttle() {
        let fast = lap(&FAST);
        let soft = lap(&Style { gas: 0.6, accel: 4.0, ..FAST });
        assert!(skills_of(&review(&soft, &fast, BIKE)).contains(&"throttle_room"));
        assert!(!skills_of(&review(&fast, &fast, BIKE)).contains(&"throttle_room"));
    }

    #[test]
    fn fighting_the_bars_is_called_where_it_costs_time() {
        let fast = lap(&FAST);
        // The average runs from 70 m before the corner, quiet bars included, as the threshold was
        // set on real laps; so the turn itself pushes hard, above the real laps' top few percent.
        let tense = lap(&Style { torque: 60.0, corner_v: 8.5, ..FAST });
        assert!(skills_of(&review(&tense, &fast, BIKE)).contains(&"bar_fight"));
        assert!(!skills_of(&review(&lap(&Style { corner_v: 8.5, ..FAST }), &fast, BIKE)).contains(&"bar_fight"));
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

    fn flight(seconds: f32, start_roll: f32, roll_rate: f32, yaw_rate: f32) -> Trace {
        let steps = (seconds / 0.05).round() as usize;
        let pts = (0..=steps)
            .map(|i| Point {
                t: i as f32 * 0.05,
                air: true,
                roll: start_roll,
                roll_rate,
                yaw_rate,
                ..Point::default()
            })
            .collect();
        Trace { pts }
    }

    #[test]
    fn scrub_motion_uses_roll_after_real_airtime_and_yaw_is_a_whip() {
        let rolled = flight(0.5, 0.0, 80.0, 0.0);
        let m = air_motion(&rolled, 0, rolled.len() - 1);
        assert_eq!(m.kind, AirMove::Scrub);
        assert!(m.peak_roll >= 39.0, "{}", m.peak_roll);
        assert!(m.roll_start_s.is_some_and(|s| s > 0.1), "{:?}", m.roll_start_s);

        let leaned = flight(0.5, 35.0, 0.0, 0.0);
        let m = air_motion(&leaned, 0, leaned.len() - 1);
        assert_eq!(m.kind, AirMove::Scrub);
        assert_eq!(m.roll_start_s, Some(0.0));

        let whip = flight(0.5, 35.0, 80.0, 80.0);
        let m = air_motion(&whip, 0, whip.len() - 1);
        assert_eq!(m.kind, AirMove::Whip, "yaw takes precedence over a bike that is also rolled");
        assert!(m.peak_yaw >= 39.0, "{}", m.peak_yaw);

        let bump = flight(0.2, 40.0, 200.0, 0.0);
        assert_eq!(air_motion(&bump, 0, bump.len() - 1).kind, AirMove::None, "a short lift is not a scrub");
    }

    #[test]
    fn scrub_cue_comes_from_roll_motion_not_a_whip() {
        let scrub = lap(&Style { whip: 40.0, ..FAST });
        let secs = sections(&scrub);
        assert!(cue_points(&scrub, &secs).iter().any(|p| p.kind == cue::SCRUB));

        let mut whip = scrub.clone();
        let jump = secs.iter().find(|s| s.kind == Kind::Jump).expect("jump");
        let (take, land) = jump.runs[0];
        for p in &mut whip.pts[take..=land] {
            p.yaw_rate = 80.0;
        }
        assert!(!cue_points(&whip, &secs).iter().any(|p| p.kind == cue::SCRUB));
    }

    #[test]
    fn scrub_finding_teaches_the_lynds_takeoff_position_when_the_reference_shows_it() {
        use crate::telemetry::{lean, stance};
        let mut floaty = lap(&Style { jump: (330.0, 356.0, 4.5), air_v: 17.0, ..FAST });
        let mut scrub = lap(&Style { whip: 40.0, ..FAST });
        for p in &mut floaty.pts {
            p.stance = stance::STAND;
            p.lean[lean::FB] = -0.25;
        }
        for p in &mut scrub.pts {
            p.stance = stance::SIT;
            p.lean[lean::FB] = 0.5;
        }
        let rv = review(&floaty, &scrub, BIKE);
        let tip = section(&rv, "Jump 1").findings.iter().find(|f| f.skill == "scrub").expect("scrub tip");
        assert!(tip.detail.contains("your bike stays upright"), "{}", tip.detail);
        assert!(tip.detail.contains("Sit through the face"), "{}", tip.detail);
        assert!(tip.detail.contains("Move the rider forward"), "{}", tip.detail);
        assert!(tip.detail.contains("Counter the lean"), "{}", tip.detail);
    }

    #[test]
    fn scrub_finding_calls_out_a_roll_started_after_takeoff() {
        let mut late = lap(&Style { jump: (330.0, 356.0, 4.5), air_v: 17.0, ..FAST });
        let mut reference = lap(&FAST);
        let ref_jump = sections(&reference).into_iter().find(|s| s.kind == Kind::Jump).expect("jump");
        let (rt, _) = ref_jump.runs[0];
        reference.pts[rt].roll = 35.0;
        let mine = air_runs(&late, ref_jump.start..ref_jump.end + 1)[0];
        for p in &mut late.pts[mine.0..=mine.1] {
            p.roll_rate = 80.0;
        }
        let rv = review(&late, &reference, BIKE);
        let tip = section(&rv, "Jump 1").findings.iter().find(|f| f.skill == "scrub").expect("scrub tip");
        assert!(tip.detail.contains("once you are airborne"), "{}", tip.detail);
    }

    #[test]
    fn unknown_body_inputs_do_not_turn_into_scrub_advice() {
        use crate::telemetry::stance;
        let mut floaty = lap(&Style { jump: (330.0, 356.0, 4.5), air_v: 17.0, ..FAST });
        let mut scrub = lap(&Style { whip: 40.0, ..FAST });
        for p in &mut floaty.pts {
            p.lean = [f32::NAN; 2];
        }
        for p in &mut scrub.pts {
            p.stance = stance::SIT;
            p.lean = [0.0, 0.5];
        }
        let rv = review(&floaty, &scrub, BIKE);
        let tip = section(&rv, "Jump 1").findings.iter().find(|f| f.skill == "scrub").expect("scrub tip");
        assert!(!tip.detail.contains("Sit through the face"), "{}", tip.detail);
        assert!(!tip.detail.contains("Move the rider forward"), "{}", tip.detail);
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
    fn a_scrub_left_leaned_on_a_normal_landing_says_bring_it_back() {
        let fast = lap(&FAST);
        let mut scrub = lap(&Style { whip: 40.0, ..FAST });
        let jump = sections(&fast).into_iter().find(|s| s.kind == Kind::Jump).expect("jump");
        let (_, land) = jump.runs[0];
        scrub.pts[(land + 4).min(jump.end)].roll = 25.0;
        let rv = review(&scrub, &fast, BIKE);
        assert!(titles(section(&rv, "Jump 1")).contains(&"Bring the scrub back sooner"));
    }

    #[test]
    fn a_cranked_scrub_landing_on_an_up_face_is_not_called_crooked() {
        let mut cased = lap(&Style { whip: 40.0, ..CASED });
        let jump = sections(&cased).into_iter().find(|s| s.kind == Kind::Jump).expect("jump");
        let (_, land) = jump.runs[0];
        cased.pts[(land + 4).min(jump.end)].roll = 25.0;
        let rv = solo(&cased, BIKE);
        let found = skills(section(&rv, "Jump 1"));
        assert!(found.contains(&"land_short"), "{found:?}");
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
            absolute: false,
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

    /// "High line or low line" is one of the first things a rider asks about a corner, and it
    /// needs no terrain: both laps are at the same point of the lap, so what is left between
    /// their heights is the ground each of them chose.
    #[test]
    fn riding_lower_through_a_corner_is_the_low_line() {
        // Down in a rut through Turn 1, against a lap that stays up on the bank. Slower with
        // it: a section that costs nothing is dropped before any tip in it is read.
        let low = lap(&Style { sink: 0.8, corner_v: 9.0, ..FAST });
        let rv = review(&low, &lap(&FAST), BIKE);
        let t1 = section(&rv, "Turn 1");
        assert!(skills(t1).contains(&"height"), "{:?}", skills(t1));
        let f = t1.findings.iter().find(|f| f.skill == "height").unwrap();
        assert!(f.title.contains("Use the high line"), "{}", f.title);
        assert!(f.detail.contains("you ride the low line"), "{}", f.detail);

        // The other way round names the other line: now the slow lap is the reference, and
        // the rider is the one up on the bank.
        let rv = review(&lap(&Style { corner_v: 9.0, ..FAST }), &lap(&Style { sink: 0.8, ..FAST }), BIKE);
        let f = section(&rv, "Turn 1").findings.iter().find(|f| f.skill == "height").unwrap();
        assert!(f.title.contains("Come down off the high line"), "{}", f.title);
        assert!(f.detail.contains("you ride the high line"), "{}", f.detail);

        // The same line on both laps says nothing: suspension and a bump are not a line.
        let rv = review(&lap(&Style { sink: 0.1, corner_v: 9.0, ..FAST }), &lap(&FAST), BIKE);
        assert!(!skills(section(&rv, "Turn 1")).contains(&"height"));
    }

    /// The line tip needs a corner's handedness to say inside or outside, so it never fired on
    /// a jump at all — and which side of the face you leave decides where you land.
    #[test]
    fn taking_off_to_one_side_names_the_jump_line() {
        let rv = review(&lap(&FAST), &lap(&FAST), BIKE);
        assert!(!skills(section(&rv, "Jump 1")).contains(&"jump_line"), "the same line says nothing");
    }

    /// A jump on a curved piece of track is ordinary motocross, and it used to delete the
    /// corner: any overlap at all and the corner was never built, so the rider got "Rhythm 2"
    /// where the turn is, and the braking zone before it orphaned into a "Straight". The
    /// fixture has only ever put its jump on a straight, so nothing caught this.
    #[test]
    fn a_jump_through_a_corner_is_still_a_corner() {
        // Turn 1 runs 200 m to 200 + pi*R (about 263 m). Put the jump in the middle of it.
        let over = lap(&Style { jump: (215.0, 235.0, 3.0), ..FAST });
        let secs = sections(&over);
        let names: Vec<&str> = secs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Turn 1"), "the corner survived its jump: {names:?}");
        let t1 = secs.iter().find(|s| s.name == "Turn 1").unwrap();
        assert_eq!(t1.kind, Kind::Corner);
        assert_eq!(t1.dir, 1, "and it is still a right-hander");
        // The jump is folded into the corner, so its advice still fires there rather than the
        // whole thing being renamed.
        assert!(!t1.runs.is_empty(), "the corner carries the jump's air run: {names:?}");
        // The symptom the rider reported: the turn coming back named after the jump on it.
        assert!(!names.iter().any(|n| n.starts_with("Rhythm")), "the turn was renamed: {names:?}");
        assert!(!names.iter().any(|n| n.starts_with("Jump")), "the jump is part of the turn, not beside it: {names:?}");
    }
}
