//! Sag: how far the bike sits into its travel with the rider on.
//!
//! Setup guides mean sag standing still, so that's what's measured when the recording has a
//! second or more of the bike stopped on its wheels with the rider on (the coach asks for one).
//! Without it, sag while riding on steady straights is measured instead: it's deeper than the
//! standing number, so it isn't held to the guide's target.

use crate::analysis::Finding;
use crate::telemetry::{Recording, Sample};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sag {
    /// Measured standing still, the number setup guides use, rather than while riding.
    pub still: bool,
    /// Metres into the travel, front then rear (the shock's own stroke).
    pub metres: [f32; 2],
    /// Share of each end's travel.
    pub share: [f32; 2],
}

/// Rear sag standing still, as a share of the travel: about a third.
pub const REAR_TARGET: (f32, f32) = (0.30, 0.36);
/// The middle of it, what a preload change aims for.
pub const REAR_AIM: f32 = 0.33;
/// Standing still for at least this long, seconds.
const STILL_S: f32 = 1.0;

fn median(v: impl Iterator<Item = f32>) -> Option<f32> {
    let mut v: Vec<f32> = v.filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

fn on_ground(x: &Sample) -> bool {
    x.wheel_material[0] != 0 && x.wheel_material[1] != 0
}

/// The longest stretch of the bike standing still on its wheels, rider on, of a second or more.
fn still_run(s: &[Sample]) -> Option<&[Sample]> {
    let stopped = |x: &Sample| x.speed.abs() < 0.3 && on_ground(x) && x.throttle < 0.05 && !x.crashed;
    let mut best: Option<&[Sample]> = None;
    let mut i = 0;
    while i < s.len() {
        let mut j = i;
        while j < s.len() && stopped(&s[j]) {
            j += 1;
        }
        if j > i + 1 && s[j - 1].t - s[i].t >= STILL_S && best.map_or(true, |b| j - i > b.len()) {
            best = Some(&s[i..j]);
        }
        i = j.max(i + 1);
    }
    best
}

pub fn measure(rec: &Recording) -> Option<Sag> {
    let travel = rec.event.susp_max_travel;
    if travel[0] <= 0.0 || travel[1] <= 0.0 {
        return None;
    }
    let s = &rec.samples;
    // Fully extended: in the air, where nothing loads the springs.
    let air: Vec<&Sample> = s.iter().filter(|x| x.airborne()).collect();
    if air.len() < 10 {
        return None;
    }
    let ext = [median(air.iter().map(|x| x.susp[0]))?, median(air.iter().map(|x| x.susp[1]))?];
    let (still, pick): (bool, Vec<&Sample>) = match still_run(s) {
        // The middle half: settled, clear of stopping and setting off.
        Some(run) => (true, run[run.len() / 4..run.len() - run.len() / 4].iter().collect()),
        None => (
            false,
            s.iter()
                .filter(|x| {
                    on_ground(x) && x.speed > 12.0 && x.yaw_rate.abs() < 10.0
                        && (0.3..0.95).contains(&x.throttle) && x.acc[2].abs() < 0.3
                })
                .collect(),
        ),
    };
    if pick.len() < 20 {
        return None;
    }
    let metres = [
        median(pick.iter().map(|x| (x.susp[0] - ext[0]).abs()))?,
        median(pick.iter().map(|x| (x.susp[1] - ext[1]).abs()))?,
    ];
    Some(Sag { still, metres, share: [metres[0] / travel[0], metres[1] / travel[1]] })
}

/// The most of each end's travel the session used, as a share, to check "it bottoms" or "it's
/// harsh" against. The deepest 1 in 200 samples on the ground, so one glitch doesn't count.
pub fn travel_used(rec: &Recording) -> Option<[f32; 2]> {
    let travel = rec.event.susp_max_travel;
    if travel[0] <= 0.0 || travel[1] <= 0.0 {
        return None;
    }
    let air: Vec<&Sample> = rec.samples.iter().filter(|x| x.airborne()).collect();
    let ground: Vec<&Sample> = rec.samples.iter().filter(|x| on_ground(x) && !x.crashed).collect();
    if air.len() < 10 || ground.len() < 50 {
        return None;
    }
    let mut out = [0.0; 2];
    for k in 0..2 {
        let ext = median(air.iter().map(|x| x.susp[k]))?;
        let mut d: Vec<f32> = ground.iter().map(|x| (x.susp[k] - ext).abs()).filter(|v| v.is_finite()).collect();
        d.sort_by(f32::total_cmp);
        out[k] = (d.get(d.len() * 199 / 200)? / travel[k]).min(1.0);
    }
    Some(out)
}

/// How far the rear sits from the target, as metres of the shock's stroke: positive is too much
/// sag (more preload), negative too little. None when it's within the target or not measured
/// standing still.
pub fn rear_off(sag: &Sag, travel: f32) -> Option<f32> {
    if !sag.still || (REAR_TARGET.0..=REAR_TARGET.1).contains(&sag.share[1]) {
        return None;
    }
    Some(sag.metres[1] - REAR_AIM * travel)
}

/// The tip, when the rear's standing sag is off target.
pub fn finding(sag: &Sag, travel: [f32; 2]) -> Option<Finding> {
    let off = rear_off(sag, travel[1])?;
    let deep = off > 0.0;
    Some(Finding {
        skill: if deep { "setup_sag_rear_deep" } else { "setup_sag_rear_high" },
        title: if deep { "The rear sags too much" } else { "The rear sags too little" }.into(),
        detail: format!(
            "Standing still with you on, the shock sits {:.0} mm into its stroke, {:.0}% of its travel, where \
             about a third is usual. {}",
            sag.metres[1] * 1000.0,
            sag.share[1] * 100.0,
            if deep { "More shock preload lifts it." } else { "Less shock preload lets it settle." }
        ),
        at: 0,
        weight: 0.9,
        safety: false,
        absolute: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{EventInfo, Recording, SessionInfo};

    fn sample(t: f32, speed: f32, susp: [f32; 2], ground: bool) -> Sample {
        Sample {
            t,
            speed,
            susp,
            wheel_material: if ground { [1, 1] } else { [0, 0] },
            ..Sample::default()
        }
    }

    fn rec(samples: Vec<Sample>) -> Recording {
        Recording {
            event: EventInfo { susp_max_travel: [0.31, 0.13], ..EventInfo::default() },
            session: SessionInfo::default(),
            centreline: Vec::new(),
            samples,
            laps: Vec::new(),
            complete: true,
            ..Recording::default()
        }
    }

    #[test]
    fn standing_still_gives_the_real_sag() {
        let mut s: Vec<Sample> = (0..20).map(|i| sample(i as f32 * 0.02, 15.0, [0.31, 0.13], false)).collect();
        // Two seconds stopped: 60 mm into the fork, 52 mm into the shock (40% of 130).
        s.extend((0..100).map(|i| sample(1.0 + i as f32 * 0.02, 0.0, [0.25, 0.078], true)));
        let sag = measure(&rec(s)).unwrap();
        assert!(sag.still);
        assert!((sag.metres[0] - 0.06).abs() < 1e-4 && (sag.metres[1] - 0.052).abs() < 1e-4, "{sag:?}");
        // 40% is past the 36% target: about 9 mm of preload to take out of the sag.
        let off = rear_off(&sag, 0.13).unwrap();
        assert!((off - (0.052 - 0.33 * 0.13)).abs() < 1e-4, "{off}");
    }

    #[test]
    fn without_a_stop_it_is_riding_sag_and_not_held_to_the_target() {
        let mut s: Vec<Sample> = (0..20).map(|i| sample(i as f32 * 0.02, 15.0, [0.31, 0.13], false)).collect();
        s.extend((0..100).map(|i| {
            let mut x = sample(1.0 + i as f32 * 0.02, 15.0, [0.22, 0.07], true);
            x.throttle = 0.6;
            x
        }));
        let sag = measure(&rec(s)).unwrap();
        assert!(!sag.still);
        assert_eq!(rear_off(&sag, 0.13), None);
    }

    #[test]
    fn travel_used_counts_real_landings_not_one_glitch() {
        let mut s: Vec<Sample> = (0..20).map(|i| sample(i as f32 * 0.02, 15.0, [0.31, 0.13], false)).collect();
        s.extend((0..400).map(|i| sample(1.0 + i as f32 * 0.02, 15.0, [0.26, 0.10], true)));
        // One sample deep in the fork: a glitch, not a landing.
        s.push(sample(9.0, 15.0, [0.0, 0.10], true));
        let used = travel_used(&rec(s.clone())).unwrap();
        assert!((used[0] - 0.05 / 0.31).abs() < 1e-3, "{used:?}");
        // A real landing lasts: ten samples 290 mm into the fork.
        s.extend((0..10).map(|i| sample(9.1 + i as f32 * 0.02, 15.0, [0.02, 0.10], true)));
        assert!(travel_used(&rec(s)).unwrap()[0] > 0.9);
    }
}
