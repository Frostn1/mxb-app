//! How fast a rider is going, round the lap.
//!
//! Nothing in the pipeline knew. Features were placed by how far round the lap they were and
//! checked on height, spacing and density; where one sat relative to a corner exit was never
//! asked. So a 2.5 m double with an eight-metre gap — twenty-one metres of air, about
//! 48 km/h off the lip — could legally sit fifteen metres after a ten-metre hairpin, and
//! every check passed. That is the thing a builder decides first and the one thing we could
//! not say anything about.
//!
//! It is also what put the braking bumps in the wrong places. They were laid a flat
//! [`crate::tracksynth::BRAKING_M`] before anything under a forty-metre radius, so a 90 km/h
//! approach to a hairpin and a 40 km/h approach to a flat left got identical washboard. Real
//! braking bumps are as long as the braking is.
//!
//! The model is the standard one for a racing line and needs no data we do not already have:
//! a corner has a speed its radius allows, and between corners the rider takes what the drive
//! and the brakes give. Two sweeps — forwards under power, backwards under braking — settle
//! it. Both run twice round, because a lap has no beginning and the speed at the finish line
//! is whatever the rider carried across it.

#![allow(dead_code)]

use crate::trackprog::TrackProgram;

/// Metres between samples of the speed profile.
const STEP: f32 = 1.0;

/// How hard a bike can hold a corner, m/s².
///
/// Not a grip figure. A motocross corner is not limited by how much lateral load the tyres
/// will take — it is limited by how fast a rider can change direction in a rut, get the bike
/// turned and be pointing at the exit, and that is a lot slower than the tyres allow. At 8.5
/// this put a ten-metre hairpin at 33 km/h and the whole lap averaged 66; a national lap
/// averages nearer 55, and its hairpins are taken in first gear.
///
/// 4.6 puts a 10 m hairpin at 24 km/h, a 20 m turn at 35 and a 40 m sweeper at 49.
const A_LAT: f32 = 4.6;

/// The most a 450 can put down, m/s², before power runs out of it.
///
/// Traction and wheelie, not engine. It only applies at the bottom of the rev range and out
/// of a corner, which is exactly where a rider is not using all of it anyway.
const A_TRACTION: f32 = 3.4;

/// Specific power, watts per kilogram of bike and rider, as it reaches the ground.
///
/// This is the correction that mattered, and its absence is what made every jump too big.
/// Acceleration was a flat `A_DRIVE` all the way to the top, so a bike left a hairpin at
/// 33 km/h and was doing 100 sixty metres later — and the gate then sized a gap for 100 km/h
/// that a rider arrives at doing seventy. Reported from the seat as "we barely made it on a
/// 450", against a model that said the jump carried nearly twice what it needed.
///
/// A 450 makes about 40 kW at the crank. Nothing like all of it reaches the ground on dirt:
/// what is left after wheelspin, the rider's own limits and a surface that moves is a small
/// fraction.
///
/// And a track is not built for the fastest bike on it. Reported from the seat, on a 250:
/// "on a two fifty I could maybe clear one or two of them, the other one I couldn't jump, I
/// just singled everything" — against gaps this had already been slowed once to size. A 250
/// carries perhaps three quarters of a 450's drive out of a turn, and a track only the
/// fastest bike can ride is a track most people push round.
///
/// So the figure is a 250's, not a 450's. It gives 2.2 m/s² at 10 m/s and 1.1 at 20 — a bike
/// that gets going and then stops gaining, which is what a rider actually has under them
/// halfway down a straight.
const P_SPEC: f32 = 22.0;

/// Brakes, m/s². Limited by the same ground that limits everything else.
const A_BRAKE: f32 = 6.0;

/// The fastest anything goes, m/s — 77 km/h.
///
/// This used to be reasoned from "a 2 km national lap runs about two minutes, which is a
/// 60 km/h average". That average was wrong, and it was the load-bearing number: across 386
/// official MXGP race classifications, 2014–2026, the median winner's race average is
/// **50.6 km/h** and the dry band is 45–54. Sixty is the fastest single lap in twelve seasons.
///
/// The top itself is now measured rather than inferred. Nobody publishes a top speed for a
/// motocross bike — not the manufacturers, not Cycle World, not MXA — but Dirt Rider had LAPD
/// officers radar a supercross test track, and the fastest thing on it was the start at
/// 48 mph. That is 21.4 m/s, and it is the only instrumented number of its kind.
///
/// Note it went *up* while the lap got slower: a lap's average is held down by its corners,
/// which [`A_LAT`] already had right — the same radar put a bowl turn's apex at 27.4 km/h and
/// this model puts a 13 m corner at 27.5. See `docs/tracks/real-track-corpus.md` §2.3.
const V_MAX: f32 = 21.4;

/// The slowest a corner is ever taken, m/s. A first-gear pivot turn is still moving.
const V_MIN: f32 = 3.0;

/// Gravity, m/s².
const G: f32 = 9.81;

/// How much of a face's own angle a bike actually leaves at.
///
/// Not the face angle. A rider compresses the suspension into a lip and the bike leaves
/// flatter than the ramp it left — how much flatter depends on speed, on the lip's shape, and
/// on whether the rider is trying to jump it or scrub it, none of which this knows. Four
/// fifths is the conservative reading: it *under*-states the carry, so a gap this says is too
/// long really is too long, which is the direction a check should err in.
/// Lowered with the rest. A gap the arithmetic says is *exactly* reachable is one a rider
/// cases, and twice now a built track has come back with jumps that were only just clearable.
const LAUNCH_SHARE: f32 = 0.72;

/// What a rider carries round the lap.
pub struct Speed {
    /// Metres per second, every [`STEP`] metres from the start.
    pub v: Vec<f32>,
    pub lap: f32,
}

impl Speed {
    /// Speed at a distance round the lap, m/s. Wraps, because a lap does.
    pub fn at(&self, s: f32) -> f32 {
        if self.v.is_empty() {
            return 0.0;
        }
        let n = self.v.len();
        let i = (s / STEP).rem_euclid(n as f32);
        let (a, f) = (i.floor() as usize % n, i.fract());
        self.v[a] + (self.v[(a + 1) % n] - self.v[a]) * f
    }

    /// How much the speed is changing at a point, m/s per metre. Negative under braking.
    ///
    /// This is what says where a track gets chopped up, and it says it in the one way that is
    /// actually true: braking bumps form where a rider is braking, for as long as they are
    /// braking, and hard in proportion to how hard.
    pub fn slope(&self, s: f32) -> f32 {
        (self.at(s + STEP) - self.at(s - STEP)) / (2.0 * STEP)
    }

    /// The acceleration along the direction of travel at a point, m/s². Negative under
    /// braking.
    ///
    /// This and not [`Speed::slope`] is what a bump forms in proportion to. A rider hauling a
    /// bike down from 25 m/s and one scrubbing the last of it off at 8 both change speed per
    /// metre, and the second changes it faster — but it is the first that is standing on the
    /// brake, and it is the first that builds the washboard.
    pub fn along(&self, s: f32) -> f32 {
        self.at(s) * self.slope(s)
    }

    /// How hard the rider is braking here, 0 to 1 of everything the brakes have.
    pub fn braking(&self, s: f32) -> f32 {
        (-self.along(s) / A_BRAKE).clamp(0.0, 1.0)
    }

    /// And how hard they are driving.
    pub fn driving(&self, s: f32) -> f32 {
        (self.along(s) / A_TRACTION).clamp(0.0, 1.0)
    }

    /// How far a bike thrown off a lip of this height reaches before it is back to the height
    /// it left, in metres.
    ///
    /// Flat-ground projectile with the launch angle taken off the face. It ignores drag, the
    /// suspension, and every choice a rider makes in the air, so it is not a simulation of a
    /// jump — it is the arithmetic a builder does before shaping one, which is all a check
    /// needs.
    pub fn carry(&self, s: f32, face_deg: f32) -> f32 {
        let v = self.at(s);
        let theta = (face_deg * LAUNCH_SHARE).to_radians();
        v * v * (2.0 * theta).sin() / G
    }
}

/// What the bike has left at this speed, m/s².
fn drive(v: f32) -> f32 {
    A_TRACTION.min(P_SPEC / v.max(2.0))
}

/// Walk the lap and work out what speed it allows.
pub fn of(prog: &TrackProgram) -> Speed {
    let lap = prog.lap_length();
    let n = ((lap / STEP).ceil() as usize).max(3);
    let st = prog.stations(STEP);
    if st.is_empty() {
        return Speed { v: vec![V_MIN; n], lap };
    }

    // What each point allows on its own: a corner is held by grip and nothing else.
    let mut v: Vec<f32> = (0..n)
        .map(|i| {
            let k = st[(i * st.len() / n).min(st.len() - 1)].curvature.abs();
            if k <= 1e-6 {
                V_MAX
            } else {
                (A_LAT / k).sqrt().clamp(V_MIN, V_MAX)
            }
        })
        .collect();

    // How much the ground helps or hinders, as an acceleration along the direction of travel.
    // A lap that climbs 60 m is a lap where the uphills are slower, and that is a real part of
    // how a hillside national rides.
    let grade: Vec<f32> = {
        let step_ups = &prog.elevation;
        let mut g = vec![0.0f32; n];
        if !step_ups.is_empty() || prog.segments.iter().any(|s| s.rise() != 0.0) {
            let mut at = 0.0f32;
            let mut height = 0.0f32;
            let mut marks: Vec<(f32, f32)> = vec![(0.0, 0.0)];
            for seg in &prog.segments {
                at += seg.length();
                height += seg.rise();
                marks.push((at, height));
            }
            for (i, slot) in g.iter_mut().enumerate() {
                let s = i as f32 * STEP;
                let j = marks.partition_point(|m| m.0 <= s).clamp(1, marks.len() - 1);
                let (a, b) = (marks[j - 1], marks[j]);
                let run = (b.0 - a.0).max(1e-3);
                *slot = -G * ((b.1 - a.1) / run).clamp(-0.5, 0.5);
            }
        }
        g
    };

    // Then the two sweeps. Twice round each, so the first lap only primes the carry and the
    // second is the answer — a lap has no start, and a profile that begins at rest at the
    // finish line is a profile with a corner in it that is not there.
    // Under power, and the power is what runs out. Acceleration is whichever is smaller —
    // what the ground will take, or what is left of the engine at this speed — so a straight
    // gives less and less the further down it you are, which is the shape a real one has.
    for _ in 0..2 {
        for i in 0..n {
            let j = (i + 1) % n;
            let a = (drive(v[i]) + grade[i]).max(0.3);
            let reach = (v[i] * v[i] + 2.0 * a * STEP).max(0.0).sqrt();
            if reach < v[j] {
                v[j] = reach;
            }
        }
    }
    for _ in 0..2 {
        for i in (0..n).rev() {
            let j = (i + n - 1) % n;
            let a = (A_BRAKE - grade[i]).max(0.5);
            let reach = (v[i] * v[i] + 2.0 * a * STEP).max(0.0).sqrt();
            if reach < v[j] {
                v[j] = reach;
            }
        }
    }

    Speed { v, lap }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trackprog::{Feature, Segment};

    fn prog(segments: Vec<Segment>) -> TrackProgram {
        let mut p: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        p.segments = segments;
        p.features = Vec::<Feature>::new();
        p
    }

    /// A kilometre of straight with one hairpin in it.
    fn one_hairpin() -> TrackProgram {
        prog(vec![
            Segment::Straight { length: 300.0, rise: 0.0 },
            Segment::Arc { radius: 10.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 300.0, rise: 0.0 },
            Segment::Arc { radius: 10.0, angle: 180.0, rise: 0.0 },
        ])
    }

    #[test]
    fn a_hairpin_is_slower_than_the_straight_that_feeds_it() {
        let s = of(&one_hairpin());
        let apex = s.at(300.0 + std::f32::consts::PI * 10.0 * 0.5);
        let straight = s.at(250.0);
        assert!(apex < straight * 0.5, "apex {apex:.1} m/s against {straight:.1} on the straight");
        // A ten-metre hairpin comes to sqrt(A_LAT * 10) — 6.8 m/s, which is 24 km/h and first
        // gear. It read 9.2 while `A_LAT` was a grip figure, and a lap built on that put its
        // jumps where a rider arriving thirty per cent slower could not reach them.
        assert!((apex - 6.8).abs() < 0.6, "a 10 m corner should be about 6.8 m/s, not {apex:.1}");
    }

    #[test]
    fn the_rider_brakes_before_the_corner_and_drives_out_of_it() {
        let s = of(&one_hairpin());
        // Braking: the speed is falling on the approach.
        assert!(s.slope(290.0) < -0.02, "not braking into the corner: {:.3}", s.slope(290.0));
        // Driving: and rising out of it.
        let exit = 300.0 + std::f32::consts::PI * 10.0 + 10.0;
        assert!(s.slope(exit) > 0.02, "not driving out of it: {:.3}", s.slope(exit));
    }

    #[test]
    fn the_profile_closes_on_itself() {
        // A lap has no beginning, so the seam at the finish line must not be visible in the
        // profile. Stated as "the speed either side of it is the same" it is not a test of
        // that at all — this lap's finish sits on a corner exit, where the speed is *meant*
        // to be climbing a metre per second every couple of metres. What must not happen is
        // a step there bigger than the bike could make anywhere else.
        let s = of(&one_hairpin());
        let n = s.v.len();
        let inside = (0..n - 1)
            .map(|i| (s.v[i + 1] - s.v[i]).abs())
            .fold(0.0f32, f32::max);
        let seam = (s.v[0] - s.v[n - 1]).abs();
        assert!(
            seam <= inside + 1e-3,
            "the profile steps {seam:.3} m/s across the finish line and never more than              {inside:.3} anywhere else"
        );
    }

    #[test]
    fn a_long_straight_reaches_a_bikes_own_limit() {
        let s = of(&prog(vec![
            Segment::Straight { length: 900.0, rise: 0.0 },
            Segment::Arc { radius: 60.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 900.0, rise: 0.0 },
            Segment::Arc { radius: 60.0, angle: 180.0, rise: 0.0 },
        ]));
        assert!(s.at(450.0) > V_MAX - 0.5, "{:.1} m/s down a 900 m straight", s.at(450.0));
    }

    #[test]
    fn what_a_jump_carries_grows_with_the_speed_into_it() {
        let s = of(&one_hairpin());
        let out_of_the_corner = s.carry(300.0 + std::f32::consts::PI * 10.0 + 15.0, 30.0);
        let down_the_straight = s.carry(250.0, 30.0);
        assert!(
            down_the_straight > out_of_the_corner * 3.0,
            "{down_the_straight:.1} m off the straight against {out_of_the_corner:.1} m out of \
             the hairpin"
        );
        // And the arithmetic itself: 25 m/s off a 30° lip, leaving at `LAUNCH_SHARE` of it,
        // is 625 * sin(2 * 21.6°) / 9.81 — a little under forty-four metres.
        let flat = Speed { v: vec![25.0; 8], lap: 8.0 };
        assert!((flat.carry(0.0, 30.0) - 43.6).abs() < 1.0, "{:.1}", flat.carry(0.0, 30.0));
    }

    #[test]
    #[ignore]
    fn diag_speed() {
        let path = std::env::var("FROST_PROGRAM").expect("set FROST_PROGRAM");
        let p: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let s = of(&p);
        let n = s.v.len() as f32;
        let mean: f32 = s.v.iter().sum::<f32>() / n;
        let mut sorted = s.v.clone();
        sorted.sort_by(f32::total_cmp);
        // A lap's time is the sum of dt = ds/v, which is what a lap time actually is.
        let secs: f32 = s.v.iter().map(|v| STEP / v.max(0.5)).sum();
        println!(
            "mean {:.1} km/h   min {:.1}   p10 {:.1}   median {:.1}   p90 {:.1}   max {:.1}",
            mean * 3.6,
            sorted[0] * 3.6,
            sorted[sorted.len() / 10] * 3.6,
            sorted[sorted.len() / 2] * 3.6,
            sorted[sorted.len() * 9 / 10] * 3.6,
            sorted[sorted.len() - 1] * 3.6,
        );
        println!(
            "lap {:.0} m in {:.0}:{:04.1} — average {:.1} km/h",
            s.lap, (secs / 60.0).floor(), secs % 60.0, s.lap / secs * 3.6
        );
        for f in &p.features {
            if let crate::trackprog::Feature::Double { at, height, gap, lip } = f {
                let fa = crate::trackprog::double_faces(*height, *lip);
                let crest = at + fa.ramp;
                let deg = crate::trackprog::face_sweep(*height, fa.ramp).to_degrees();
                println!(
                    "  double at {at:.0}: {height:.1} m, gap {gap:.0} m — lip at {:.0} km/h, \
                     model says it carries {:.0} m and needs {:.0}",
                    s.at(crest) * 3.6,
                    s.carry(crest, deg),
                    fa.back + gap + fa.face * 0.25
                );
            }
        }
    }
}
