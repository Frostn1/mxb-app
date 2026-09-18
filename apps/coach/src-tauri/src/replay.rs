//! One section of one lap, frame by frame: the rider's own inputs and the bike's attitude, so
//! a corner can be watched back on a bike instead of read off a chart.
//!
//! The frames are the recorder's own samples, 50 Hz as it wrote them. The review works on a
//! one-metre grid and sections are cut on that grid, so the grid says where a section starts
//! and ends — but a rutted corner at 5 m/s is five grid points a second, far too coarse to
//! watch anybody move, and it is slowest exactly where body position matters most. So the
//! frames come from the samples and only the extent comes from the grid.

use std::path::Path;

use serde::Serialize;
use tauri::AppHandle;

use crate::analysis::{self, Kind, Section, Trace, STEP_M};
use crate::telemetry::{Lap, Recording, Sample};

/// Rolling radius of the rear wheel, metres. Nothing in a recording says how big the wheel is
/// — the game never reports it — so this is the usual 19" MX rear on a 110/90: a 0.241 m rim
/// plus a 0.099 m section is 0.34 m unloaded, and it rolls at about 0.31 under the bike. It
/// only sets how fast the wheels look going round, so a few percent out is invisible.
const ROLL_RADIUS_M: f32 = 0.31;

/// Degrees a wheel turns for every metre rolled.
const DEG_PER_M: f32 = 360.0 / (2.0 * std::f32::consts::PI * ROLL_RADIUS_M);

/// One sample of the section: enough to put a bike on screen in the pose the rider had it in.
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// Seconds since the section's first frame.
    pub t: f32,
    /// Metres along the centreline since the section's first frame.
    pub dist: f32,
    /// Ground speed, m/s.
    pub v: f32,
    /// Bar angle, degrees, negative right, as the recording gives it.
    pub steer: f32,
    /// Share of the travel in use, 0 fully extended to 1 bottomed, front then rear. Zero
    /// throughout where the lap gave nothing to measure the extended length against.
    pub used: [f32; 2],
    /// The bike's own attitude, degrees.
    pub roll: f32,
    pub pitch: f32,
    /// How far the wheels have turned about their axles since the section started, degrees.
    /// Only ever climbs.
    pub spin: f32,
    pub throttle: f32,
    pub front: f32,
    pub rear: f32,
    pub gear: i32,
    pub air: bool,
    /// Where the rider was asking to put their body, `telemetry::lean`. An axis the recorder
    /// could not read arrives as null, not as a centred rider: the two are different facts.
    pub lean: [f32; 2],
    /// Sitting or standing, `telemetry::stance`; 0 where it could not be read.
    pub stance: u8,
}

/// A section of a lap to play back, with the same section of the rider's best lap beside it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replay {
    pub section_id: String,
    pub name: String,
    pub kind: Kind,
    /// Section length, metres.
    pub length: f32,
    pub frames: Vec<Frame>,
    /// The ghost: empty when this lap is the best one, or when the best lap hasn't got this
    /// section.
    pub best: Vec<Frame>,
    /// Which lap the ghost is.
    pub best_lap: Option<i32>,
    /// Whether the recorder read each lean axis at all, and the rider's stance. False means the
    /// frames say nothing about the body, and it has to be left still rather than invented.
    pub lean_known: [bool; 2],
    pub stance_known: bool,
}

/// Plays section `section_id` of lap `lap` back frame by frame, with the same section of the
/// rider's fastest lap of the session beside it.
#[tauri::command]
pub async fn coach_replay(app: AppHandle, path: String, lap: i32, section_id: String) -> Result<Replay, String> {
    tauri::async_runtime::spawn_blocking(move || replay_for(&app, &path, lap, &section_id))
        .await
        .map_err(|e| e.to_string())?
}

fn replay_for(app: &AppHandle, path: &str, num: i32, section_id: &str) -> Result<Replay, String> {
    let rec = crate::coach::load(path)?;
    let (lap, tr) = lap_and_trace(&rec, num)?;
    let sec = section_of(app, &rec, &tr, section_id)?;
    let (best, best_lap) = ghost(app, path, num, &rec, section_id)?;
    Ok(Replay {
        section_id: sec.id.clone(),
        name: sec.name.clone(),
        kind: sec.kind,
        length: (sec.end - sec.start) as f32 * STEP_M,
        frames: frames(&lap, &rec, span(&sec)),
        best,
        best_lap,
        lean_known: [rec.lean_confidence[0] > 0, rec.lean_confidence[1] > 0],
        stance_known: rec.stance_confidence > 0,
    })
}

fn lap_and_trace(rec: &Recording, num: i32) -> Result<(Lap, Trace), String> {
    let lap = rec.laps().into_iter().find(|l| l.num == num).ok_or_else(|| format!("Lap {num} isn't in that session."))?;
    let tr = Trace::new(&lap, rec.event.track_length).ok_or_else(|| format!("Lap {num} is too short to replay."))?;
    Ok((lap, tr))
}

fn section_of(app: &AppHandle, rec: &Recording, tr: &Trace, id: &str) -> Result<Section, String> {
    let mut secs = analysis::sections(tr);
    // The ids and names every other screen uses, or the corner the rider picked in the review
    // isn't the corner that plays back here.
    crate::coach::label(app, &rec.event.track_id, rec.event.track_length, &[], &mut secs);
    secs.into_iter().find(|s| s.id == id).ok_or_else(|| format!("\"{id}\" isn't a section of that lap."))
}

/// Where a section sits along the centreline, metres: entry through exit, not just its core.
fn span(sec: &Section) -> (f32, f32) {
    (sec.start as f32 * STEP_M, sec.end as f32 * STEP_M)
}

/// The rider's own fastest whole lap of this session over the same section. Nothing when the
/// asked-for lap is already that lap, and nothing when the fastest lap hasn't got the section:
/// sections are found lap by lap, so a jump one lap cleared and the other rolled isn't there
/// on both to compare.
fn ghost(app: &AppHandle, path: &str, num: i32, rec: &Recording, id: &str) -> Result<(Vec<Frame>, Option<i32>), String> {
    let sessions = crate::coach::all_sessions(app);
    // The session this recording belongs to, or a session of its own when it isn't on disk to
    // be grouped — the same reading of it the rest of the coach takes.
    let summary = match sessions.iter().find(|s| s.stints.iter().any(|x| x.path == path)) {
        Some(s) => s.clone(),
        None => crate::coach::summarize(Path::new(path), rec),
    };
    let Some(best) = summary.laps.iter().filter(|l| l.comparable()).min_by_key(|l| l.time_ms) else {
        return Ok((Vec::new(), None));
    };
    if best.path == path && best.num == num {
        return Ok((Vec::new(), None));
    }
    // Another stint of the same session is another file; the same file is already read.
    let other = if best.path == path { None } else { Some(crate::coach::load(&best.path)?) };
    let brec = other.as_ref().unwrap_or(rec);
    let Ok((blap, btr)) = lap_and_trace(brec, best.num) else { return Ok((Vec::new(), None)) };
    let mut bsecs = analysis::sections(&btr);
    crate::coach::label(app, &brec.event.track_id, brec.event.track_length, &[], &mut bsecs);
    let Some(bsec) = bsecs.iter().find(|s| s.id == id) else { return Ok((Vec::new(), None)) };
    Ok((frames(&blap, brec, span(bsec)), Some(best.num)))
}

/// Where each end sits with nothing on it: the median suspension length while both wheels are
/// off the ground, so it doesn't matter which way the game counts. `Trace::fill_travel` reads
/// it the same way, off the review's own grid, but that grid is private to the review and the
/// samples say it just as well. None when the lap never left the ground long enough to ask.
fn extended(samples: &[Sample]) -> Option<[f32; 2]> {
    let mut out = [0.0f32; 2];
    for (k, e) in out.iter_mut().enumerate() {
        let mut v: Vec<f32> = samples.iter().filter(|s| s.airborne()).map(|s| s.susp[k]).collect();
        if v.len() < 5 {
            return None;
        }
        v.sort_by(f32::total_cmp);
        *e = v[v.len() / 2];
    }
    Some(out)
}

/// The lap's samples between `from` and `to` metres along the centreline.
fn frames(lap: &Lap, rec: &Recording, (from, to): (f32, f32)) -> Vec<Frame> {
    let (track_len, travel) = (rec.event.track_length, rec.event.susp_max_travel);
    let ext = extended(&lap.samples).filter(|_| travel[0] > 0.0 && travel[1] > 0.0);
    let mut out: Vec<Frame> = Vec::new();
    let (mut start, mut last_d, mut spin) = (None, f32::MIN, 0.0f32);
    // The clock and the speed of the frame before, to integrate the roll between them.
    let mut prev: Option<(f32, f32)> = None;
    for s in &lap.samples {
        let d = s.pos * track_len;
        // A sample that didn't move forward — a stall, a wobble backwards after a crash — has
        // no place in a replay, the same reason the review's grid drops it.
        if d < from || d > to || d <= last_d {
            continue;
        }
        last_d = d;
        let v = (s.vel[0] * s.vel[0] + s.vel[1] * s.vel[1] + s.vel[2] * s.vel[2]).sqrt();
        if let Some((pt, pv)) = prev {
            // Ground speed over time is the distance rolled, and a wheel turns so many degrees
            // for every metre of it. Both halves are positive, so this can only climb.
            spin += 0.5 * (pv + v) * (s.t - pt).max(0.0) * DEG_PER_M;
        }
        prev = Some((s.t, v));
        let mut used = [0.0f32; 2];
        if let Some(e) = ext {
            for k in 0..2 {
                used[k] = ((s.susp[k] - e[k]).abs() / travel[k]).min(1.2);
            }
        }
        let t0 = *start.get_or_insert(s.t);
        out.push(Frame {
            t: s.t - t0,
            dist: d - from,
            v,
            steer: s.steer,
            used,
            roll: s.roll,
            pitch: s.pitch,
            spin,
            throttle: s.throttle,
            front: s.front_brake,
            rear: s.rear_brake,
            gear: s.gear,
            air: s.airborne(),
            lean: s.lean,
            stance: s.stance,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{self, lean, testfile::File};

    const LEN: f32 = 400.0;
    /// Metres of the lap spent in the air, so there is an extended length to measure travel
    /// against.
    const FLIGHT: (f32, f32) = (150.0, 165.0);

    /// A straight lap at 50 Hz, ridden at `v` metres a second and leaning `lean` all the way
    /// round. `bind` is what the recorder managed to read of each lean axis.
    fn ride(v: impl Fn(f32) -> f32, lean: [f32; 2], bind: [u8; 2]) -> Recording {
        let mut f = File::new();
        f.event("stadium", LEN).lean_bind(bind[0], bind[1]).stance_bind(2);
        let (mut d, mut t) = (0.0f32, 0.0f32);
        while d <= LEN {
            let speed = v(d).max(0.5);
            let air = (FLIGHT.0..FLIGHT.1).contains(&d);
            f.lean(t, d / LEN, lean[0], lean[1]);
            f.stance(t, d / LEN, 0);
            f.sample(t, d / LEN, |b| {
                b.f(20, speed).f(44, speed).f(32, d);
                b.f(140, 3.0).f(144, 0.6).f(148, 0.1);
                b.f(120, if air { 0.30 } else { 0.24 }).f(124, if air { 0.30 } else { 0.24 });
                b.i(12, 3).i(168, if air { 0 } else { 2 }).i(172, if air { 0 } else { 2 });
            });
            d += speed * 0.02;
            t += 0.02;
        }
        f.lap(1, (t * 1000.0) as i32).end();
        telemetry::parse(&f.0).unwrap()
    }

    /// The wheels turn one way. A replay that ran the integration off the clock, or off a
    /// position that stepped backwards, would hand the bike a wheel that flicked back and forth.
    #[test]
    fn the_wheels_only_ever_turn_forwards() {
        // Braking to a crawl and driving out again: the speed the spin is integrated from
        // falls, and the spin still must not.
        let rec = ride(|d| if (100.0..200.0).contains(&d) { 3.0 } else { 18.0 }, [0.4, -0.2], [2, 2]);
        let (lap, tr) = lap_and_trace(&rec, 1).unwrap();
        let secs = analysis::sections(&tr);
        let sec = &secs[0];
        let fr = frames(&lap, &rec, span(sec));

        assert!(fr.len() > 100, "a section of a 50 Hz lap is hundreds of frames, not {}", fr.len());
        assert_eq!(fr[0].spin, 0.0, "the section starts with the wheels where they were");
        for w in fr.windows(2) {
            assert!(w[1].spin >= w[0].spin, "spin fell from {} to {}", w[0].spin, w[1].spin);
            assert!(w[1].dist > w[0].dist && w[1].t > w[0].t);
        }
        // On a straight, what the wheels rolled is what the lap covered.
        let last = fr.last().unwrap();
        let turns = last.spin / 360.0;
        assert!((turns - last.dist / (2.0 * std::f32::consts::PI * ROLL_RADIUS_M)).abs() < turns * 0.02);
        // Travel is measured against the flight, so the frames on the ground use some of it.
        assert!((fr[0].used[0] - 0.2).abs() < 0.01, "used {:?}", fr[0].used);
    }

    /// A lean axis the recorder could not read has to arrive unread. Zero is the rider asking
    /// to be centred, which is a different thing, and a replay that filled it in would sit the
    /// rider bolt upright through a corner they were hanging off.
    #[test]
    fn an_axis_the_recorder_could_not_read_stays_unread() {
        let rec = ride(|_| 14.0, [0.6, f32::NAN], [2, 0]);
        let (lap, tr) = lap_and_trace(&rec, 1).unwrap();
        let secs = analysis::sections(&tr);
        let fr = frames(&lap, &rec, span(&secs[0]));

        assert!(!fr.is_empty());
        for f in &fr {
            assert!((f.lean[lean::LR] - 0.6).abs() < 1e-6);
            assert!(!lean::known(f.lean[lean::FB]), "it came back as {}", f.lean[lean::FB]);
            assert_ne!(f.lean[lean::FB], 0.0);
        }
        assert_eq!([rec.lean_confidence[0] > 0, rec.lean_confidence[1] > 0], [true, false]);
    }
}
