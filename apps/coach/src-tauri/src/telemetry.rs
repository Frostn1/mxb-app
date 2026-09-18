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
    /// How the rider's Sit control is bound and how sure the recorder is of it; then each
    /// change of sitting or standing (FrostMod's `src/stance.h`).
    pub const STANCE_BIND: u8 = 10;
    pub const STANCE: u8 = 11;
    /// The other riders (FrostMod's `src/others.h`): who's in the event, where everyone is
    /// about ten times a second, and each rider's laps and splits.
    pub const ENTRY: u8 = 12;
    pub const POSITIONS: u8 = 13;
    pub const RACE_LAP: u8 = 14;
    /// How the rider's two body-lean controls are bound and how sure the recorder is of each;
    /// then where the rider is asking to be, with every sample (FrostMod's `src/lean.h`).
    pub const LEAN_BIND: u8 = 16;
    pub const LEAN: u8 = 17;
}

/// A rider in the event, as the game lists them.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rider {
    pub num: i32,
    pub name: String,
    pub bike: String,
    pub riding: bool,
}

/// One bike in a POSITIONS record.
#[derive(Clone, Copy, Debug, Default)]
pub struct Place {
    pub num: i32,
    /// The recorder's guess that this is the rider recording: the bike nearest their own.
    pub local: bool,
    pub crashed: bool,
    /// Along the centreline, 0..1.
    pub pos: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Every bike on track at one moment.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub t: f32,
    pub bikes: Vec<Place>,
}

/// A lap the game timed for any rider, stamped with the track time it came in.
#[derive(Clone, Copy, Debug, Default)]
pub struct RaceLap {
    pub t: f32,
    pub num: i32,
    pub lap: i32,
    pub invalid: bool,
    pub time_ms: i32,
}

/// Sitting or standing, as each sample carries it. Unknown in recordings from before the
/// recorder read the Sit control, or when it couldn't.
pub mod stance {
    pub const UNKNOWN: u8 = 0;
    pub const STAND: u8 = 1;
    pub const SIT: u8 = 2;
}

/// Where the rider is asking to put their body, as the recorder gives it: -1 to +1 on each of
/// two axes, or NaN where it could not be read.
///
/// This is the rider's *input*, not a body angle. The body has its own rates and springs in
/// the bike's own config and the riding aids can move it instead, so nothing built on these
/// should be worded as though the rider's body were visible.
pub mod lean {
    /// Left (-1) to right (+1): counter-lean.
    pub const LR: usize = 0;
    /// Back (-1) to forward (+1).
    pub const FB: usize = 1;

    pub fn known(v: f32) -> bool {
        v.is_finite() && (-1.5..=1.5).contains(&v)
    }
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
    /// See [`stance`]: the last change the recorder wrote before this sample.
    pub stance: u8,
    /// See [`lean`]: where the rider was asking to be, indexed by `lean::LR` and `lean::FB`.
    /// NaN on either axis the recorder could not read.
    pub lean: [f32; 2],
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
    /// How sure the recorder is of sitting and standing: 0 not read, 1 a guess (a toggle bind,
    /// or auto-sit it couldn't rule out), 2 sure.
    pub stance_confidence: u8,
    /// How sure the recorder is of each lean axis, by `lean::LR` and `lean::FB`: 0 not read
    /// (the aid moves the rider, nothing is bound, or it is on a device that can't be polled),
    /// 1 a guess, 2 sure.
    pub lean_confidence: [u8; 2],
    /// The other riders, from recorders that write them: the latest listing of each.
    pub riders: Vec<Rider>,
    pub frames: Vec<Frame>,
    pub race_laps: Vec<RaceLap>,
}

/// A NUL-padded string in a record.
fn text(p: &[u8], at: usize, len: usize) -> String {
    let b = p.get(at..(at + len).min(p.len())).unwrap_or(&[]);
    String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).trim().to_string()
}

fn rider(p: &[u8]) -> Rider {
    let b = Bytes(p);
    Rider { num: b.i(0), riding: p.get(4) == Some(&1), name: text(p, 8, 64), bike: text(p, 72, 40) }
}

/// `f32 t, u16 count, u16 stride`, then per bike `u16 num, u8 flags, u8, f32 pos, x, y, z`.
fn frame(p: &[u8]) -> Frame {
    let b = Bytes(p);
    let u16_at = |i: usize| p.get(i..i + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]) as usize);
    let (count, stride) = (u16_at(4), u16_at(6));
    let mut bikes = Vec::with_capacity(count);
    if stride >= 20 {
        for k in 0..count {
            let at = 8 + k * stride;
            if at + 20 > p.len() {
                break;
            }
            let flags = p[at + 2];
            bikes.push(Place {
                num: u16_at(at) as i32,
                crashed: flags & 1 != 0,
                local: flags & 2 != 0,
                pos: b.f(at + 4),
                x: b.f(at + 8),
                y: b.f(at + 12),
                z: b.f(at + 16),
            });
        }
    }
    Frame { t: b.f(0), bikes }
}

/// One lap's samples, with `pos` unwrapped to run 0..1 across it.
#[derive(Clone, Debug)]
pub struct Lap {
    pub num: i32,
    pub time_ms: i32,
    pub invalid: bool,
    /// Starts at the line and ends at it, with the time it reports: it can be compared.
    pub whole: bool,
    /// Why it isn't whole: `out lap`, `unfinished`, `untimed`, `gap in the recording`.
    pub issue: Option<&'static str>,
    /// The rider came off during it. Still whole: the game timed it, and the review says where.
    pub crashed: bool,
    /// How long it took by the recording, for a lap the game left untimed.
    pub ridden_ms: i32,
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
        stance: stance::UNKNOWN,
        lean: [f32::NAN; 2],
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
    let mut now = stance::UNKNOWN;
    let mut now_lean = [f32::NAN; 2];
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
            tag::SAMPLE => rec.samples.push(Sample { stance: now, lean: now_lean, ..sample(p) }),
            tag::STANCE_BIND => rec.stance_confidence = p.get(4).copied().unwrap_or(0),
            // Layout byte, three zeros, then one 96-byte block per axis; each block leads with
            // input, aid, confidence, source.
            tag::LEAN_BIND => {
                const AXIS: usize = 96;
                rec.lean_confidence = [
                    p.get(4 + 2).copied().unwrap_or(0),
                    p.get(4 + AXIS + 2).copied().unwrap_or(0),
                ];
            }
            // `f32 t`, `f32 lap position`, then the two axes. Anything that isn't a reading
            // between -1 and +1 — NaN from the recorder included — stays unknown.
            tag::LEAN => {
                let read = |at: usize| {
                    let v = b.f(at);
                    if lean::known(v) { v.clamp(-1.0, 1.0) } else { f32::NAN }
                };
                now_lean = [read(8), read(12)];
            }
            tag::ENTRY => {
                let r = rider(p);
                match rec.riders.iter_mut().find(|x| x.num == r.num) {
                    Some(x) => *x = r,
                    None => rec.riders.push(r),
                }
            }
            tag::POSITIONS => rec.frames.push(frame(p)),
            // `f32 t`, then `SPluginsRaceLap_t`: session, race number, lap, invalid, lap time.
            tag::RACE_LAP => rec.race_laps.push(RaceLap {
                t: b.f(0),
                num: b.i(8),
                lap: b.i(12),
                invalid: b.i(16) != 0,
                time_ms: b.i(20),
            }),
            // The recorder writes 0 standing, 1 sitting, 2 unknown.
            tag::STANCE => {
                now = match p.get(8) {
                    Some(0) => stance::STAND,
                    Some(1) => stance::SIT,
                    _ => stance::UNKNOWN,
                }
            }
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

/// Lap timing is off by at most this much from the samples and still counts as whole: a sample
/// either side of the line, plus slack for a stutter or a pause.
const WHOLE_SLACK_S: f32 = 0.5;
/// How far from the line a lap's first and last samples may sit, as a share of the lap.
const LINE_SLACK: f32 = 0.05;

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
            let issue = issue(&samples, m.time_ms);
            let crashed = samples.iter().any(|s| s.crashed);
            let ridden_ms = match (samples.first(), samples.last()) {
                (Some(a), Some(b)) => ((b.t - a.t) * 1000.0).round() as i32,
                _ => 0,
            };
            out.push(Lap { num: m.num, time_ms: m.time_ms, invalid: m.invalid, whole: issue.is_none(), issue, crashed, ridden_ms, samples });
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

/// Why a lap can't be compared, or None when it can.
fn issue(samples: &[Sample], time_ms: i32) -> Option<&'static str> {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else { return Some("gap in the recording") };
    let reported = time_ms as f32 / 1000.0;
    if time_ms <= 0 {
        Some("untimed")
    } else if first.pos.abs() >= LINE_SLACK {
        Some("out lap")
    } else if (last.pos - 1.0).abs() >= LINE_SLACK {
        Some("unfinished")
    } else if ((last.t - first.t) - reported).abs() > WHOLE_SLACK_S + reported * 0.02 {
        Some("gap in the recording")
    } else {
        None
    }
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
        /// A hold bind on key 18 (E), with the confidence given.
        pub fn stance_bind(&mut self, confidence: u8) -> &mut Self {
            let mut p = vec![0u8; 52];
            p[..6].copy_from_slice(&[1, 1, 0, 0, confidence, 1]);
            p[8..12].copy_from_slice(&18i32.to_le_bytes());
            self.record(tag::STANCE_BIND, &p)
        }
        /// `state` as the recorder writes it: 0 standing, 1 sitting, 2 unknown.
        pub fn stance(&mut self, t: f32, pos: f32, state: u8) -> &mut Self {
            let mut p = Vec::with_capacity(12);
            p.extend_from_slice(&t.to_le_bytes());
            p.extend_from_slice(&pos.to_le_bytes());
            p.extend_from_slice(&[state, 0, 0, 0]);
            self.record(tag::STANCE, &p)
        }
        /// Both lean axes bound to a pad, with a confidence each.
        pub fn lean_bind(&mut self, lr: u8, fb: u8) -> &mut Self {
            const AXIS: usize = 96;
            let mut p = vec![0u8; 4 + AXIS * 2];
            p[0] = 1;
            // input = axis (3), aid off, confidence, read through DirectInput (2).
            p[4..4 + 4].copy_from_slice(&[3, 0, lr, 2]);
            p[4 + AXIS..4 + AXIS + 4].copy_from_slice(&[3, 0, fb, 2]);
            self.record(tag::LEAN_BIND, &p)
        }
        pub fn lean(&mut self, t: f32, pos: f32, lr: f32, fb: f32) -> &mut Self {
            let mut p = Vec::with_capacity(16);
            for v in [t, pos, lr, fb] {
                p.extend_from_slice(&v.to_le_bytes());
            }
            self.record(tag::LEAN, &p)
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

    /// The recorder writes all the way through a stint, so the coach re-reads a file that is
    /// still growing. A record cut in half at the end is where the writer had got to, not a
    /// broken file, and the laps already in it have to come back either way — the session list
    /// and the open session's laps refresh off exactly this.
    #[test]
    fn a_recording_still_being_written_reads_up_to_where_it_stopped() {
        let mut f = File::new();
        f.event("indiana", 1650.0);
        f.sample(0.0, 0.0, |_| {}).sample(30.0, 0.5, |_| {}).lap(0, 60_000);
        f.sample(60.0, 0.0, |_| {}).sample(90.0, 0.5, |_| {});
        let mid_stint = f.0.clone();
        let finished = {
            let mut g = File(mid_stint.clone());
            g.end();
            g.0
        };

        let done = parse(&finished).unwrap();
        assert!(done.complete);
        assert_eq!((done.laps.len(), done.samples.len()), (1, 4));

        // Still out on track: no end record yet, and the lap already timed is there.
        let mid = parse(&mid_stint).unwrap();
        assert!(!mid.complete, "the stint is still running");
        assert_eq!((mid.laps.len(), mid.samples.len()), (1, 4));

        // Cut anywhere inside the last record — a half-written header, a half-written payload
        // — and it still reads up to the last whole record rather than failing.
        for cut in [1, 5, 9, 40, 100, 203] {
            let short = &mid_stint[..mid_stint.len() - cut];
            let rec = parse(short).expect("a half-written tail is not a broken file");
            assert_eq!(rec.laps.len(), 1, "the finished lap survives a cut of {cut}");
            assert_eq!(rec.samples.len(), 3, "and the sample being written is simply not there yet");
            assert!(!rec.complete);
        }
    }

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
        assert_eq!((s.stance, rec.stance_confidence), (stance::UNKNOWN, 0));
    }

    #[test]
    fn reads_the_other_riders() {
        let mut f = File::new();
        f.event("indiana", 1650.0);
        let mut entry = vec![0u8; 152];
        entry[..4].copy_from_slice(&7i32.to_le_bytes());
        entry[4] = 1;
        entry[8..16].copy_from_slice(b"Fast Guy");
        entry[72..74].copy_from_slice(b"YZ");
        f.record(tag::ENTRY, &entry);
        // Two bikes: #7 crashed, #3 the rider recording.
        let mut pos = Vec::new();
        pos.extend_from_slice(&12.5f32.to_le_bytes());
        pos.extend_from_slice(&2u16.to_le_bytes());
        pos.extend_from_slice(&20u16.to_le_bytes());
        for (num, flags, p) in [(7u16, 1u8, 0.5f32), (3, 2, 0.25)] {
            pos.extend_from_slice(&num.to_le_bytes());
            pos.extend_from_slice(&[flags, 0]);
            for v in [p, 10.0, 2.0, -4.0] {
                pos.extend_from_slice(&v.to_le_bytes());
            }
        }
        f.record(tag::POSITIONS, &pos);
        let mut lap = Vec::new();
        for v in [13.0f32.to_bits() as i32, 2, 7, 3, 0, 61_234, 0, 0, 0] {
            lap.extend_from_slice(&v.to_le_bytes());
        }
        f.record(tag::RACE_LAP, &lap);
        let rec = parse(&f.0).unwrap();
        assert_eq!((rec.riders[0].num, rec.riders[0].name.as_str(), rec.riders[0].bike.as_str()), (7, "Fast Guy", "YZ"));
        let b = &rec.frames[0].bikes;
        assert_eq!(rec.frames[0].t, 12.5);
        assert_eq!((b[0].num, b[0].crashed, b[0].local, b[0].pos), (7, true, false, 0.5));
        assert_eq!((b[1].num, b[1].local, b[1].x, b[1].z), (3, true, 10.0, -4.0));
        let l = rec.race_laps[0];
        assert_eq!((l.t, l.num, l.lap, l.invalid, l.time_ms), (13.0, 7, 3, false, 61_234));
    }

    #[test]
    /// Lean rides along with each sample the way stance does, and an axis the recorder could
    /// not read stays unknown rather than arriving as a centred rider.
    #[test]
    fn each_sample_carries_where_the_rider_was_asking_to_be() {
        let mut f = File::new();
        f.event("indiana", 1650.0).lean_bind(2, 1);
        f.lean(0.0, 0.0, -0.5, 0.25).sample(0.0, 0.0, |_| {});
        f.lean(0.02, 0.001, 0.75, f32::NAN).sample(0.02, 0.001, |_| {});
        let rec = parse(&f.0).unwrap();

        assert_eq!(rec.lean_confidence, [2, 1]);
        assert!((rec.samples[0].lean[lean::LR] + 0.5).abs() < 1e-6);
        assert!((rec.samples[0].lean[lean::FB] - 0.25).abs() < 1e-6);
        assert!((rec.samples[1].lean[lean::LR] - 0.75).abs() < 1e-6);
        assert!(
            !lean::known(rec.samples[1].lean[lean::FB]),
            "an axis the recorder couldn't read is unknown, not centred"
        );
    }

    /// A recorder that writes no lean at all — every one before this change — still reads, with
    /// both axes unknown rather than zero.
    #[test]
    fn a_recording_without_lean_reads_with_it_unknown() {
        let mut f = File::new();
        f.event("indiana", 1650.0);
        f.sample(0.0, 0.0, |_| {});
        let rec = parse(&f.0).unwrap();
        assert_eq!(rec.lean_confidence, [0, 0]);
        assert!(!lean::known(rec.samples[0].lean[lean::LR]));
        assert!(!lean::known(rec.samples[0].lean[lean::FB]));
    }

    /// Out of range is not a reading. A stick the recorder mapped to the wrong axis can hand
    /// back anything, and believing it would put the rider somewhere they never were.
    #[test]
    fn a_lean_reading_outside_the_axis_is_unknown() {
        let mut f = File::new();
        f.event("indiana", 1650.0).lean_bind(2, 2);
        f.lean(0.0, 0.0, 9999.0, -7.5).sample(0.0, 0.0, |_| {});
        let rec = parse(&f.0).unwrap();
        assert!(!lean::known(rec.samples[0].lean[lean::LR]), "9999 is not a stick position");
        assert!(!lean::known(rec.samples[0].lean[lean::FB]), "-7.5 is not either");
    }

    #[test]
    fn each_sample_carries_the_last_stance_the_recorder_wrote() {
        let mut f = File::new();
        f.event("indiana", 1650.0).stance_bind(2).stance(0.0, 0.0, 0);
        f.sample(0.0, 0.0, |_| {}).sample(0.02, 0.001, |_| {});
        f.stance(0.04, 0.002, 1).sample(0.04, 0.002, |_| {});
        f.stance(0.06, 0.003, 2).sample(0.06, 0.003, |_| {});
        let rec = parse(&f.0).unwrap();
        assert_eq!(rec.stance_confidence, 2);
        let got: Vec<u8> = rec.samples.iter().map(|s| s.stance).collect();
        assert_eq!(got, [stance::STAND, stance::STAND, stance::SIT, stance::UNKNOWN]);
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
        assert_eq!(laps[0].issue, Some("out lap"));
        let lap = &laps[1];
        assert!(lap.samples[0].pos < 0.0, "early sample folded before the line");
        assert!(lap.samples.last().unwrap().pos > 0.99);
        assert!(lap.whole, "a clean lap is whole");
    }

    #[test]
    fn a_crash_leaves_the_lap_timed_and_comparable() {
        let mut f = File::new();
        f.event("t", 100.0);
        f.sample(0.0, 0.0, |_| {});
        for k in 1..=100 {
            let pos = (k as f32 / 100.0).min(0.999);
            f.sample(k as f32 * 0.1, pos, |b| {
                if k == 50 {
                    b.i(136, 1);
                }
            });
        }
        f.lap(0, 10_000).end();
        let lap = &parse(&f.0).unwrap().laps()[0];
        assert!(lap.crashed);
        assert!(lap.whole, "{:?}", lap.issue);
    }
}
