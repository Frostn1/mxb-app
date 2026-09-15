//! The other riders: where each one gains on this lap, section by section.
//!
//! Recorders from FrostMod 0.21 write where every bike is about ten times a second and each
//! rider's timed laps. A rider's lap is their positions between the line and the line, as
//! distance along the track against time, so any section of this lap can be timed for them.
//! Three riders are worth comparing with: the one just faster than you (a target you can
//! reach), the fastest, and in a race the one ahead of you on track as your lap ends.

use crate::telemetry::Recording;
use serde::Serialize;

/// A section of the reviewed lap: its name, metres along the track, and the rider's time.
pub struct Part {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub time: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gain {
    pub section: String,
    /// Seconds the other rider is quicker here.
    pub gain: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rival {
    pub num: i32,
    pub name: String,
    pub bike: String,
    /// `closest` (just faster than you), `fastest`, or `ahead` (in front on track, in a race).
    pub why: &'static str,
    pub lap: i32,
    pub time_ms: i32,
    /// Where they're quicker than this lap, most first, at most three.
    pub gains: Vec<Gain>,
}

/// A rider's lap as metres along the track against track time, rising.
struct TheirLap {
    num: i32,
    lap: i32,
    time_ms: i32,
    t_end: f32,
    pts: Vec<(f32, f32)>,
}

impl TheirLap {
    /// Track time at `m` metres, between the two points either side.
    fn at(&self, m: f32) -> Option<f32> {
        let i = self.pts.partition_point(|p| p.0 < m);
        let (a, b) = (self.pts.get(i.checked_sub(1)?)?, self.pts.get(i)?);
        Some(a.1 + (b.1 - a.1) * (m - a.0) / (b.0 - a.0).max(1e-3))
    }

    fn span(&self, start: usize, end: usize) -> Option<f32> {
        Some(self.at(end as f32)? - self.at(start as f32)?)
    }
}

/// The bike the recorder took for the rider's own: the one it flagged most.
pub(crate) fn local_num(rec: &Recording) -> Option<i32> {
    let mut count: Vec<(i32, usize)> = Vec::new();
    for b in rec.frames.iter().flat_map(|f| &f.bikes).filter(|b| b.local) {
        match count.iter_mut().find(|c| c.0 == b.num) {
            Some(c) => c.1 += 1,
            None => count.push((b.num, 1)),
        }
    }
    count.into_iter().max_by_key(|c| c.1).map(|c| c.0)
}

/// Every other rider's timed, valid lap the positions cover from line to line.
fn their_laps(rec: &Recording, local: Option<i32>) -> Vec<TheirLap> {
    let len = rec.event.track_length;
    if len <= 0.0 {
        return Vec::new();
    }
    rec.race_laps
        .iter()
        .filter(|l| !l.invalid && l.time_ms > 0 && Some(l.num) != local)
        .filter_map(|l| {
            let t0 = l.t - l.time_ms as f32 / 1000.0;
            let span = (l.t - t0).max(1e-3);
            let mut pts: Vec<(f32, f32)> = Vec::new();
            for f in rec.frames.iter().filter(|f| f.t >= t0 - 0.3 && f.t <= l.t + 0.3) {
                let Some(b) = f.bikes.iter().find(|b| b.num == l.num && !b.crashed) else { continue };
                // Either end of the lap sits across the line: fold it back so distance rises.
                let share = (f.t - t0) / span;
                let pos = if share < 0.2 && b.pos > 0.5 {
                    b.pos - 1.0
                } else if share > 0.8 && b.pos < 0.5 {
                    b.pos + 1.0
                } else {
                    b.pos
                };
                let m = pos * len;
                if pts.last().map_or(true, |p| m > p.0) {
                    pts.push((m, f.t));
                }
            }
            let covers = pts.first()?.0 <= 0.05 * len && pts.last()?.0 >= 0.95 * len;
            covers.then_some(TheirLap { num: l.num, lap: l.lap, time_ms: l.time_ms, t_end: l.t, pts })
        })
        .collect()
}

/// How far round a rider is at track time `t`: laps the game timed for them, plus where they are.
fn progress(rec: &Recording, num: i32, t: f32) -> Option<f32> {
    let done = rec.race_laps.iter().filter(|l| l.num == num && l.t <= t).count() as f32;
    let f = rec.frames.iter().min_by(|a, b| (a.t - t).abs().total_cmp(&(b.t - t).abs()))?;
    Some(done + f.bikes.iter().find(|b| b.num == num)?.pos)
}

/// The riders worth comparing this lap with. `ended` is the track time the lap ended;
/// `race` whether it was a race, where the rider ahead on track counts.
pub fn rivals(rec: &Recording, parts: &[Part], my_ms: i32, ended: f32, race: bool) -> Vec<Rival> {
    let local = local_num(rec);
    let laps = their_laps(rec, local);
    let faster = || laps.iter().filter(|l| my_ms <= 0 || l.time_ms < my_ms);
    let mut picked: Vec<(&'static str, &TheirLap)> = Vec::new();
    if let Some(l) = faster().max_by_key(|l| l.time_ms) {
        picked.push(("closest", l));
    }
    if let Some(l) = faster().min_by_key(|l| l.time_ms) {
        picked.push(("fastest", l));
    }
    if race {
        // Ahead on track when this lap ended: the least progress that's still more than ours.
        let me = local.and_then(|n| progress(rec, n, ended));
        let ahead = me.and_then(|me| {
            rec.riders
                .iter()
                .filter(|r| Some(r.num) != local)
                .filter_map(|r| Some((r.num, progress(rec, r.num, ended)?)))
                .filter(|&(_, p)| p > me)
                .min_by(|a, b| a.1.total_cmp(&b.1))
        });
        // Their lap nearest to ours in time.
        if let Some((num, _)) = ahead {
            if let Some(l) = laps.iter().filter(|l| l.num == num).min_by(|a, b| (a.t_end - ended).abs().total_cmp(&(b.t_end - ended).abs())) {
                picked.push(("ahead", l));
            }
        }
    }
    let mut out: Vec<Rival> = Vec::new();
    for (why, l) in picked {
        if out.iter().any(|r| r.num == l.num && r.lap == l.lap) {
            continue;
        }
        let mut gains: Vec<Gain> = parts
            .iter()
            .filter_map(|p| Some(Gain { section: p.name.clone(), gain: p.time - l.span(p.start, p.end)? }))
            .filter(|g| g.gain > 0.05)
            .collect();
        gains.sort_by(|a, b| b.gain.total_cmp(&a.gain));
        gains.truncate(3);
        for g in &mut gains {
            g.gain = (g.gain * 100.0).round() / 100.0;
        }
        let who = rec.riders.iter().find(|r| r.num == l.num);
        out.push(Rival {
            num: l.num,
            name: who.map_or_else(|| format!("#{}", l.num), |r| r.name.clone()),
            bike: who.map_or_else(String::new, |r| r.bike.clone()),
            why,
            lap: l.lap,
            time_ms: l.time_ms,
            gains,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{EventInfo, Frame, Place, RaceLap, Rider};

    /// Two riders on a 1000 m track from t = 0: #3 is the one recording (a 60 s lap), #7 does
    /// the first half as fast and the second half 5 s quicker (55 s).
    fn session() -> Recording {
        let mut rec = Recording { event: EventInfo { track_length: 1000.0, ..EventInfo::default() }, ..Recording::default() };
        rec.riders = vec![
            Rider { num: 3, name: "Me".into(), bike: "KTM".into(), riding: true },
            Rider { num: 7, name: "Fast Guy".into(), bike: "YZ".into(), riding: true },
        ];
        let seven = |t: f32| if t <= 30.0 { t / 60.0 } else { 0.5 + (t - 30.0) / 50.0 };
        for i in 0..=600 {
            let t = i as f32 * 0.1;
            let bike = |num: i32, pos: f32, local: bool| Place { num, local, pos: pos.rem_euclid(1.0), ..Place::default() };
            rec.frames.push(Frame { t, bikes: vec![bike(3, t / 60.0, true), bike(7, seven(t), false)] });
        }
        rec.race_laps = vec![
            RaceLap { t: 55.0, num: 7, lap: 1, invalid: false, time_ms: 55_000 },
            RaceLap { t: 60.0, num: 3, lap: 1, invalid: false, time_ms: 60_000 },
        ];
        rec
    }

    fn parts() -> Vec<Part> {
        vec![
            Part { name: "Turn 1".into(), start: 0, end: 500, time: 30.0 },
            Part { name: "Back straight".into(), start: 500, end: 990, time: 29.4 },
        ]
    }

    #[test]
    fn finds_where_the_faster_rider_gains() {
        let r = rivals(&session(), &parts(), 60_000, 60.0, false);
        assert_eq!(r.len(), 1, "one faster lap is both the closest and the fastest: {r:?}");
        assert_eq!((r[0].name.as_str(), r[0].why, r[0].time_ms), ("Fast Guy", "closest", 55_000));
        // 490 m at 20 m/s is 24.5 s against the rider's 29.4 s; Turn 1 is level.
        assert_eq!(r[0].gains, vec![Gain { section: "Back straight".into(), gain: 4.9 }]);
    }

    #[test]
    fn the_rider_recording_is_never_their_own_rival() {
        let r = rivals(&session(), &parts(), 50_000, 60.0, false);
        assert!(r.is_empty(), "nobody is faster than 50 s: {r:?}");
    }

    #[test]
    fn in_a_race_the_rider_ahead_on_track_counts() {
        let r = rivals(&session(), &parts(), 0, 58.0, true);
        assert!(r.iter().any(|r| r.num == 7), "{r:?}");
        assert!(r.iter().all(|r| r.num != 3));
    }
}
