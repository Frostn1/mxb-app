//! Reads the `.mxbc` files the MXB Coach recorder (`mxbcoach.dlo`, built in the frostmod repo
//! from `src/coachrec.h`) writes: the game's own plugin payloads, byte for byte, in tagged
//! records. The field offsets are PiBoSo's published MX Bikes structs (`mxb_api.h`).
//!
//! Every record carries its length, so a game build that appends fields reads fine, and a
//! field past the end of a short record reads as zero rather than failing the file.

use anyhow::{bail, Result};
use serde::Serialize;

const MAGIC: &[u8; 4] = b"MXBC";
const FORMAT_VERSION: u32 = 1;

mod tag {
    pub const EVENT: u8 = 1;
    pub const SESSION: u8 = 2;
    pub const CENTRELINE: u8 = 3;
    pub const SAMPLE: u8 = 4;
    pub const LAP: u8 = 5;
    pub const END: u8 = 9;
}

/// `SPluginsBikeEvent_t`: who, on what, where.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInfo {
    pub rider: String,
    pub bike_id: String,
    pub bike_name: String,
    pub category: String,
    pub track_id: String,
    pub track_name: String,
    /// Centreline length, metres.
    pub track_length: f32,
    /// 1 = testing, 2 = race, 4 = straight rhythm.
    pub event_type: i32,
    pub gears: i32,
    pub max_rpm: i32,
    pub limiter: i32,
    pub shift_rpm: i32,
    /// Metres, front then rear.
    pub susp_max_travel: [f32; 2],
    pub steer_lock: f32,
}

/// `SPluginsBikeSession_t`.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session: i32,
    /// 0 = sunny, 1 = cloudy, 2 = rainy.
    pub conditions: i32,
    pub air_temp: f32,
    pub setup: String,
}

/// `SPluginsTrackSegment_t`.
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub curve: bool,
    pub length: f32,
    /// Metres; negative for a left-hander, 0 on a straight.
    pub radius: f32,
    /// Start heading, degrees from north.
    pub angle: f32,
    pub start: [f32; 2],
    pub height: f32,
}

/// One `RunTelemetry` call: `SPluginsBikeData_t` plus the time and lap position the game
/// passes beside it.
///
/// Decoded in full although the rules read only part of it: the next rule shouldn't need a
/// parser change, and a field here is cheap.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    /// On-track time, seconds.
    pub t: f32,
    /// Along the centreline, 0..1 (can run slightly outside it once a lap is unwrapped).
    pub pos: f32,
    pub rpm: f32,
    pub gear: i32,
    /// m/s.
    pub speed: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// World m/s.
    pub vel: [f32; 3],
    /// G, in the chassis frame.
    pub acc: [f32; 3],
    /// Degrees.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Degrees per second.
    pub yaw_rate: f32,
    pub pitch_rate: f32,
    pub roll_rate: f32,
    /// Metres, front then rear.
    pub susp: [f32; 2],
    pub susp_vel: [f32; 2],
    pub crashed: bool,
    /// Degrees, negative = right.
    pub steer: f32,
    /// 0..1.
    pub throttle: f32,
    pub front_brake: f32,
    pub rear_brake: f32,
    /// 0 = fully engaged.
    pub clutch: f32,
    /// m/s, front then rear.
    pub wheel_speed: [f32; 2],
    /// 0 = not touching the ground.
    pub wheel_material: [i32; 2],
    /// kPa.
    pub brake_pressure: [f32; 2],
    pub steer_torque: f32,
}

impl Sample {
    pub fn airborne(&self) -> bool {
        self.wheel_material[0] == 0 && self.wheel_material[1] == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LapMark {
    pub num: i32,
    pub invalid: bool,
    pub time_ms: i32,
    /// How many samples came before it: the lap ends at `samples[at]`.
    pub at: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Recording {
    pub event: EventInfo,
    pub session: SessionInfo,
    pub centreline: Vec<Segment>,
    pub samples: Vec<Sample>,
    pub laps: Vec<LapMark>,
    /// The stint ended cleanly. False when the game quit or crashed mid-stint.
    pub complete: bool,
}

/// One lap's samples, with `pos` unwrapped to run 0..1 across it.
#[derive(Clone, Debug)]
pub struct Lap {
    pub num: i32,
    pub time_ms: i32,
    pub invalid: bool,
    /// Starts at the line and ends at it, with the time it reports. An out lap from the pits,
    /// or a lap that lost samples, is not whole and can't be compared.
    pub whole: bool,
    pub samples: Vec<Sample>,
}

/// Little-endian reads that give zero past the end.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn word(&self, at: usize) -> [u8; 4] {
        self.0.get(at..at + 4).and_then(|s| s.try_into().ok()).unwrap_or([0; 4])
    }
    fn f(&self, at: usize) -> f32 {
        f32::from_le_bytes(self.word(at))
    }
    fn i(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.word(at))
    }
    fn s(&self, at: usize, len: usize) -> String {
        let end = (at + len).min(self.0.len());
        let raw = self.0.get(at..end).unwrap_or(&[]);
        let raw = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];
        String::from_utf8_lossy(raw).trim().to_string()
    }
}

fn event(p: &[u8]) -> EventInfo {
    let b = Bytes(p);
    EventInfo {
        rider: b.s(0, 100),
        bike_id: b.s(100, 100),
        bike_name: b.s(200, 100),
        gears: b.i(300),
        max_rpm: b.i(304),
        limiter: b.i(308),
        shift_rpm: b.i(312),
        susp_max_travel: [b.f(332), b.f(336)],
        steer_lock: b.f(340),
        category: b.s(344, 100),
        track_id: b.s(444, 100),
        track_name: b.s(544, 100),
        track_length: b.f(644),
        event_type: b.i(648),
    }
}

fn session(p: &[u8]) -> SessionInfo {
    let b = Bytes(p);
    SessionInfo { session: b.i(0), conditions: b.i(4), air_temp: b.f(8), setup: b.s(12, 100) }
}

fn centreline(p: &[u8]) -> Vec<Segment> {
    let b = Bytes(p);
    let (count, stride) = (b.i(0).max(0) as usize, b.i(4).max(0) as usize);
    if stride < 28 {
        return Vec::new();
    }
    (0..count)
        .map(|k| 8 + k * stride)
        .take_while(|&at| at + 28 <= p.len())
        .map(|at| Segment {
            curve: b.i(at) == 1,
            length: b.f(at + 4),
            radius: b.f(at + 8),
            angle: b.f(at + 12),
            start: [b.f(at + 16), b.f(at + 20)],
            height: b.f(at + 24),
        })
        .collect()
}

fn sample(p: &[u8]) -> Sample {
    let h = Bytes(p);
    let b = Bytes(p.get(8..).unwrap_or(&[]));
    Sample {
        t: h.f(0),
        pos: h.f(4),
        rpm: b.i(0) as f32,
        gear: b.i(12),
        speed: b.f(20),
        x: b.f(24),
        y: b.f(28),
        z: b.f(32),
        vel: [b.f(36), b.f(40), b.f(44)],
        acc: [b.f(48), b.f(52), b.f(56)],
        yaw: b.f(96),
        pitch: b.f(100),
        roll: b.f(104),
        yaw_rate: b.f(108),
        pitch_rate: b.f(112),
        roll_rate: b.f(116),
        susp: [b.f(120), b.f(124)],
        susp_vel: [b.f(128), b.f(132)],
        crashed: b.i(136) != 0,
        steer: b.f(140),
        throttle: b.f(144),
        front_brake: b.f(148),
        rear_brake: b.f(152),
        clutch: b.f(156),
        wheel_speed: [b.f(160), b.f(164)],
        wheel_material: [b.i(168), b.i(172)],
        brake_pressure: [b.f(176), b.f(180)],
        steer_torque: b.f(184),
    }
}

/// Reads a whole file. A record cut short by a crash ends the file rather than failing it.
pub fn parse(bytes: &[u8]) -> Result<Recording> {
    if bytes.len() < 8 || &bytes[..4] != MAGIC {
        bail!("not an MXB Coach recording");
    }
    let version = Bytes(bytes).i(4) as u32;
    if version != FORMAT_VERSION {
        bail!("recording format {version} is newer than this app understands");
    }
    let mut rec = Recording::default();
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let t = bytes[at];
        let len = Bytes(bytes).i(at + 4) as u32 as usize;
        let Some(p) = bytes.get(at + 8..at + 8 + len) else { break };
        at += 8 + len;
        let b = Bytes(p);
        match t {
            tag::EVENT => rec.event = event(p),
            tag::SESSION => rec.session = session(p),
            tag::CENTRELINE => rec.centreline = centreline(p),
            tag::SAMPLE => rec.samples.push(sample(p)),
            tag::LAP => rec.laps.push(LapMark {
                num: b.i(0),
                invalid: b.i(4) != 0,
                time_ms: b.i(8),
                at: rec.samples.len(),
            }),
            tag::END => rec.complete = true,
            // SPLIT, START/STOP and anything newer: recorded, but nothing reads them yet.
            _ => {}
        }
    }
    Ok(rec)
}

/// Lap timing is off by at most this much from the samples and still counts as whole: one
/// sample either side of the line, plus slack for a stutter.
const WHOLE_SLACK_S: f32 = 0.25;

impl Recording {
    /// Every lap the game timed, in order. The samples before the first timed lap belong to
    /// no lap and are dropped.
    pub fn laps(&self) -> Vec<Lap> {
        let mut out = Vec::with_capacity(self.laps.len());
        let mut start = 0;
        for m in &self.laps {
            let end = m.at.min(self.samples.len());
            let mut samples = self.samples[start.min(end)..end].to_vec();
            start = end;
            unwrap(&mut samples);
            let whole = is_whole(&samples, m.time_ms);
            out.push(Lap { num: m.num, time_ms: m.time_ms, invalid: m.invalid, whole, samples });
        }
        out
    }
}

/// The lap crosses the line at both ends, so the first few samples can still read ~1 and the
/// last few already ~0. Folds them back so position rises through the lap.
fn unwrap(samples: &mut [Sample]) {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else { return };
    let (t0, span) = (first.t, (last.t - first.t).max(1e-3));
    for s in samples.iter_mut() {
        let f = (s.t - t0) / span;
        if f < 0.2 && s.pos > 0.5 {
            s.pos -= 1.0;
        } else if f > 0.8 && s.pos < 0.5 {
            s.pos += 1.0;
        }
    }
}

fn is_whole(samples: &[Sample], time_ms: i32) -> bool {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else { return false };
    let duration = last.t - first.t;
    let reported = time_ms as f32 / 1000.0;
    time_ms > 0
        && first.pos.abs() < 0.03
        && (last.pos - 1.0).abs() < 0.03
        && (duration - reported).abs() <= WHOLE_SLACK_S + reported * 0.01
        && !samples.iter().any(|s| s.crashed)
}

#[cfg(test)]
pub(crate) mod testfile {
    //! Builds `.mxbc` bytes the way `coachrec.h` writes them.
    use super::*;

    pub struct File(pub Vec<u8>);

    impl File {
        pub fn new() -> Self {
            let mut v = MAGIC.to_vec();
            v.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
            File(v)
        }
        pub fn record(&mut self, t: u8, payload: &[u8]) -> &mut Self {
            self.0.extend_from_slice(&[t, 0, 0, 0]);
            self.0.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            self.0.extend_from_slice(payload);
            self
        }
        pub fn event(&mut self, track: &str, length: f32) -> &mut Self {
            let mut p = vec![0u8; 820];
            p[..5].copy_from_slice(b"Rider");
            p[444..444 + track.len()].copy_from_slice(track.as_bytes());
            p[644..648].copy_from_slice(&length.to_le_bytes());
            p[308..312].copy_from_slice(&13000i32.to_le_bytes());
            p[332..336].copy_from_slice(&0.3f32.to_le_bytes());
            p[336..340].copy_from_slice(&0.3f32.to_le_bytes());
            self.record(tag::EVENT, &p)
        }
        pub fn sample(&mut self, t: f32, pos: f32, fill: impl Fn(&mut Bike)) -> &mut Self {
            let mut bike = Bike([0u8; 188]);
            fill(&mut bike);
            let mut p = Vec::with_capacity(196);
            p.extend_from_slice(&t.to_le_bytes());
            p.extend_from_slice(&pos.to_le_bytes());
            p.extend_from_slice(&bike.0);
            self.record(tag::SAMPLE, &p)
        }
        pub fn lap(&mut self, num: i32, time_ms: i32) -> &mut Self {
            let mut p = Vec::new();
            for v in [num, 0, time_ms, 0] {
                p.extend_from_slice(&v.to_le_bytes());
            }
            self.record(tag::LAP, &p)
        }
        pub fn end(&mut self) -> &mut Self {
            self.record(tag::END, &[])
        }
    }

    /// A `SPluginsBikeData_t` being filled in by offset.
    pub struct Bike(pub [u8; 188]);

    impl Bike {
        pub fn f(&mut self, at: usize, v: f32) -> &mut Self {
            self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
            self
        }
        pub fn i(&mut self, at: usize, v: i32) -> &mut Self {
            self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testfile::File;
    use super::*;

    #[test]
    fn reads_the_fields_at_the_published_offsets() {
        let mut f = File::new();
        f.event("indiana", 1650.0).sample(1.5, 0.25, |b| {
            b.f(20, 17.0).f(24, 10.0).f(32, -4.0).f(104, -32.0).f(144, 0.8).f(148, 0.3);
            b.i(12, 3).i(168, 2).i(172, 0);
        });
        let rec = parse(&f.0).unwrap();
        assert_eq!(rec.event.rider, "Rider");
        assert_eq!(rec.event.track_id, "indiana");
        assert_eq!(rec.event.track_length, 1650.0);
        assert_eq!(rec.event.limiter, 13000);
        let s = rec.samples[0];
        assert_eq!((s.t, s.pos, s.speed, s.x, s.z), (1.5, 0.25, 17.0, 10.0, -4.0));
        assert_eq!((s.roll, s.throttle, s.front_brake, s.gear), (-32.0, 0.8, 0.3, 3));
        assert_eq!(s.wheel_material, [2, 0]);
        assert!(!s.airborne());
        assert!(!rec.complete);
    }

    #[test]
    fn a_crash_mid_record_keeps_everything_before_it() {
        let mut f = File::new();
        f.event("t", 100.0).sample(0.0, 0.0, |_| {}).sample(0.02, 0.01, |_| {});
        let cut = f.0.len() - 30;
        let rec = parse(&f.0[..cut]).unwrap();
        assert_eq!(rec.samples.len(), 1);
    }

    #[test]
    fn refuses_what_is_not_a_recording() {
        assert!(parse(b"PK\x03\x04rest").is_err());
        let mut v = MAGIC.to_vec();
        v.extend_from_slice(&2u32.to_le_bytes());
        assert!(parse(&v).is_err());
    }

    #[test]
    fn laps_split_at_the_line_and_unwrap_across_it() {
        let mut f = File::new();
        f.event("t", 100.0);
        // Out lap from the pits: starts mid-track.
        for k in 0..20 {
            f.sample(k as f32 * 0.1, 0.5 + k as f32 * 0.02, |_| {});
        }
        f.lap(0, 2100);
        // A clean lap of 10 s at 10 Hz. Its first sample still reads just under 1.
        f.sample(2.0, 0.995, |_| {});
        for k in 1..=100 {
            let pos = (k as f32 / 100.0) % 1.0;
            f.sample(2.0 + k as f32 * 0.1, pos.min(0.999), |_| {});
        }
        f.lap(1, 10_050).end();
        let rec = parse(&f.0).unwrap();
        assert!(rec.complete);
        let laps = rec.laps();
        assert_eq!(laps.len(), 2);
        assert!(!laps[0].whole, "an out lap is not comparable");
        let lap = &laps[1];
        assert!(lap.samples[0].pos < 0.0, "early sample folded before the line");
        assert!(lap.samples.last().unwrap().pos > 0.99);
        assert!(lap.whole, "a clean lap is whole");
    }
}
