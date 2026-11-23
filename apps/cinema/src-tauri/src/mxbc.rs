//! MXB Coach recordings (`.mxbc`), read for a shot rather than for a lap.
//!
//! The recorder (`mxbcoach.dlo`, FrostMod's `src/coachrec.h`) writes the game's own plugin
//! payloads in tagged records: `u8 tag, 3 pad, u32 len, payload`. Coach's reader
//! (`apps/coach/src-tauri/src/telemetry.rs`) is the reference for every offset here; Cinema
//! needs the chassis matrix as well, which Coach never reads, and nothing about laps.
//!
//! The rider's skeleton as the game drew it (`RIDER_SKELETON` 18, `RIDER_BONES` 19, FrostMod
//! `docs/RIDER_BONES.md`) is read the way the prototype's `replay/mxbc2json.py` reads it.

use std::collections::HashMap;

use anyhow::{bail, Result};
use serde::Serialize;

const MAGIC: &[u8; 4] = b"MXBC";

mod tag {
    pub const EVENT: u8 = 1;
    pub const SAMPLE: u8 = 4;
    pub const END: u8 = 9;
    pub const STANCE_BIND: u8 = 10;
    pub const STANCE: u8 = 11;
    pub const LEAN_BIND: u8 = 16;
    pub const LEAN: u8 = 17;
    pub const RIDER_SKELETON: u8 = 18;
    pub const RIDER_BONES: u8 = 19;
}

/// Sitting or standing, as the recorder last said.
pub mod stance {
    pub const UNKNOWN: u8 = 0;
    pub const STAND: u8 = 1;
    pub const SIT: u8 = 2;
}

/// `SPluginsBikeEvent_t`: who, on what, where.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub rider: String,
    pub bike_id: String,
    pub bike_name: String,
    pub category: String,
    pub track_id: String,
    pub track_name: String,
    pub track_length: f32,
    /// Metres, front then rear.
    pub susp_max_travel: [f32; 2],
}

/// One `RunTelemetry` call, the parts a shot needs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    /// On-track time, seconds.
    pub t: f32,
    pub speed: f32,
    pub gear: i32,
    pub pos: [f32; 3],
    /// Chassis axes in the world, row-major, game axes (left-handed, Y up, Z forward).
    pub rot: [f32; 9],
    /// Degrees.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Current suspension length, metres, front then rear.
    pub susp: [f32; 2],
    pub crashed: bool,
    /// Bar angle, degrees, + = left.
    pub steer: f32,
    pub throttle: f32,
    pub brake: [f32; 2],
    /// m/s, front then rear.
    pub wheel_speed: [f32; 2],
    /// Neither wheel touching anything.
    pub air: bool,
    pub stance: u8,
    /// -1..1 per axis (left/right, back/forward), NaN where the recorder couldn't read it.
    pub lean: [f32; 2],
}

/// One node of the rider's skeleton, in the recorder's order (a `RIDER_BONES` node index is
/// a position in this list).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    /// `None` for a root.
    pub parent: Option<u8>,
    pub recorded: bool,
}

/// `RIDER_SKELETON`: the stint's node names and where the bike body sits on the chassis.
#[derive(Clone, Debug, Default)]
pub struct Skeleton {
    /// 0 ok, 1 unknown game build, 2 not found.
    pub status: u8,
    pub exe_stamp: u32,
    /// The bike body's offset from the physics chassis, game axes, metres.
    pub bike_offset: [f32; 3],
    /// Empty unless the status is ok.
    pub nodes: Vec<Node>,
}

/// One bone: `[qw, qx, qy, qz, x, y, z]`, rotation and position in the bike frame (game axes,
/// position including the record's origin and the bike offset), not parent-relative.
pub type Bone = [f64; 7];

/// One `RIDER_BONES` record, decoded.
#[derive(Clone, Debug, Default)]
pub struct Bones {
    pub bones: Vec<(String, Bone)>,
    /// No new frame was drawn since the last record.
    pub repeat: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Recording {
    pub event: Event,
    pub samples: Vec<Sample>,
    pub complete: bool,
    /// Whether the recorder read each lean axis, and the Sit control, at all.
    pub lean_known: [bool; 2],
    pub stance_known: bool,
    /// The last `RIDER_SKELETON` read.
    pub skeleton: Option<Skeleton>,
    /// `RIDER_BONES` records by time (`key`), still as written: a long stint holds tens of
    /// thousands, and a shot decodes only its own few hundred.
    bones: HashMap<i64, (std::sync::Arc<Skeleton>, Vec<u8>)>,
}

/// A time in seconds, rounded to 4 places as a whole number of 0.1 ms: how a sample finds the
/// bones recorded with it (`mxbc2json.py`: `BONES.get(round(t, 4))`).
pub fn key(t: f32) -> i64 {
    (t as f64 * 1e4).round() as i64
}

fn round(v: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (v * k).round() / k
}

impl Recording {
    /// The bones recorded with the sample at `t` (seconds, the sample's own on-track time).
    pub fn bones_at(&self, t: f32) -> Option<Bones> {
        let (skel, p) = self.bones.get(&key(t))?;
        Some(decode_bones(skel, p))
    }

    #[cfg(test)]
    pub fn has_bones(&self) -> bool {
        !self.bones.is_empty()
    }
}

fn skeleton(p: &[u8]) -> Skeleton {
    let b = B(p);
    let mut nodes = Vec::new();
    let mut k = 20;
    for _ in 0..p[2] {
        let Some(&[par, recd, len]) = p.get(k..k + 3).and_then(|s| <&[u8; 3]>::try_from(s).ok()) else { break };
        let Some(name) = p.get(k + 3..k + 3 + len as usize) else { break };
        // latin1: every byte is the code point of the same value.
        nodes.push(Node { name: name.iter().map(|&c| c as char).collect(), parent: (par != 255).then_some(par), recorded: recd != 0 });
        k += 3 + len as usize;
    }
    Skeleton {
        status: p[1],
        exe_stamp: u32::from_le_bytes([p[4], p[5], p[6], p[7]]),
        bike_offset: [b.f(8), b.f(12), b.f(16)],
        nodes: if p[1] == 0 { nodes } else { Vec::new() },
    }
}

fn decode_bones(skel: &Skeleton, p: &[u8]) -> Bones {
    let b = B(p);
    let o = [b.f(12), b.f(16), b.f(20)];
    let off = skel.bike_offset;
    let mut bones = Vec::new();
    for i in 0..p[9] as usize {
        let at = 24 + 15 * i;
        let Some(r) = p.get(at..at + 15) else { break };
        let node = r[0] as usize;
        let h = |k: usize| i16::from_le_bytes([r[1 + 2 * k], r[2 + 2 * k]]) as f64;
        let Some(n) = skel.nodes.get(node) else { continue };
        let (qx, qy, qz, qw) = (h(0), h(1), h(2), h(3));
        let pos = |a: usize, mm: f64| round(off[a] as f64 + o[a] as f64 + mm / 1000.0, 4);
        bones.push((
            n.name.clone(),
            [round(qw / 32767.0, 5), round(qx / 32767.0, 5), round(qy / 32767.0, 5), round(qz / 32767.0, 5), pos(0, h(4)), pos(1, h(5)), pos(2, h(6))],
        ));
    }
    Bones { bones, repeat: p[10] & 1 != 0 }
}

struct B<'a>(&'a [u8]);
impl B<'_> {
    fn f(&self, at: usize) -> f32 {
        self.0.get(at..at + 4).map_or(0.0, |s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i(&self, at: usize) -> i32 {
        self.0.get(at..at + 4).map_or(0, |s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn s(&self, at: usize, len: usize) -> String {
        let b = self.0.get(at..(at + len).min(self.0.len())).unwrap_or(&[]);
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).trim().to_string()
    }
}

fn event(p: &[u8]) -> Event {
    let b = B(p);
    Event {
        rider: b.s(0, 100),
        bike_id: b.s(100, 100),
        bike_name: b.s(200, 100),
        susp_max_travel: [b.f(332), b.f(336)],
        category: b.s(344, 100),
        track_id: b.s(444, 100),
        track_name: b.s(544, 100),
        track_length: b.f(644),
    }
}

fn sample(p: &[u8]) -> Sample {
    let h = B(p);
    let b = B(p.get(8..).unwrap_or(&[]));
    let mut rot = [0.0f32; 9];
    for (k, r) in rot.iter_mut().enumerate() {
        *r = b.f(60 + 4 * k);
    }
    Sample {
        t: h.f(0),
        gear: b.i(12),
        speed: b.f(20),
        pos: [b.f(24), b.f(28), b.f(32)],
        rot,
        yaw: b.f(96),
        pitch: b.f(100),
        roll: b.f(104),
        susp: [b.f(120), b.f(124)],
        crashed: b.i(136) != 0,
        steer: b.f(140),
        throttle: b.f(144),
        brake: [b.f(148), b.f(152)],
        wheel_speed: [b.f(160), b.f(164)],
        // Neither wheel on anything, and moving: a bike parked before the start or sitting in the
        // menu reports no ground under either wheel too.
        air: b.i(168) == 0 && b.i(172) == 0 && b.f(20) > 2.0,
        stance: stance::UNKNOWN,
        lean: [f32::NAN; 2],
    }
}

fn known(v: f32) -> bool {
    v.is_finite() && (-1.5..=1.5).contains(&v)
}

/// Every record of a recording, walked once. A record cut short by a crash ends the file
/// rather than failing it.
pub fn parse(bytes: &[u8]) -> Result<Recording> {
    if bytes.len() < 8 || &bytes[..4] != MAGIC {
        bail!("not an MXB Coach recording");
    }
    let mut rec = Recording::default();
    let mut now = stance::UNKNOWN;
    let mut lean = [f32::NAN; 2];
    let mut skel: Option<std::sync::Arc<Skeleton>> = None;
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let t = bytes[at];
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let Some(p) = bytes.get(at + 8..at + 8 + len) else { break };
        at += 8 + len;
        match t {
            tag::EVENT => rec.event = event(p),
            tag::SAMPLE => rec.samples.push(Sample { stance: now, lean, ..sample(p) }),
            tag::STANCE_BIND => rec.stance_known = p.get(4).copied().unwrap_or(0) > 0,
            tag::LEAN_BIND => {
                const AXIS: usize = 96;
                rec.lean_known = [
                    p.get(4 + 2).copied().unwrap_or(0) > 0,
                    p.get(4 + AXIS + 2).copied().unwrap_or(0) > 0,
                ];
            }
            tag::LEAN => {
                let b = B(p);
                let read = |at: usize| {
                    let v = b.f(at);
                    if known(v) { v.clamp(-1.0, 1.0) } else { f32::NAN }
                };
                lean = [read(8), read(12)];
            }
            // The recorder writes 0 standing, 1 sitting, 2 unknown.
            tag::STANCE => {
                now = match p.get(8) {
                    Some(0) => stance::STAND,
                    Some(1) => stance::SIT,
                    _ => stance::UNKNOWN,
                }
            }
            tag::RIDER_SKELETON if p.len() >= 20 && p[0] == 1 => {
                let k = skeleton(p);
                skel = Some(std::sync::Arc::new(k.clone()));
                rec.skeleton = Some(k);
            }
            tag::RIDER_BONES if p.len() >= 24 && p[8] == 1 => {
                if let Some(k) = skel.as_ref().filter(|k| !k.nodes.is_empty()) {
                    rec.bones.insert(key(B(p).f(0)), (k.clone(), p.to_vec()));
                }
            }
            tag::END => rec.complete = true,
            _ => {}
        }
    }
    Ok(rec)
}

/// A run of samples with both wheels off the ground, long enough to be a jump rather than a
/// skip over a braking bump.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Jump {
    /// Seconds from the recording's first sample.
    pub start: f32,
    pub end: f32,
    /// Where it was highest.
    pub apex: f32,
    /// Metres the bike rose above where it left the ground.
    pub height: f32,
    /// Metres covered in the air.
    pub length: f32,
}

/// The shortest time in the air that counts as a jump.
const MIN_AIR_S: f32 = 0.3;

pub fn jumps(samples: &[Sample]) -> Vec<Jump> {
    let Some(first) = samples.first() else { return Vec::new() };
    let t0 = first.t;
    let mut out = Vec::new();
    let mut i = 0;
    while i < samples.len() {
        if !samples[i].air || samples[i].crashed {
            i += 1;
            continue;
        }
        let a = i;
        while i < samples.len() && samples[i].air {
            i += 1;
        }
        let b = i - 1;
        let (sa, sb) = (&samples[a], &samples[b]);
        if sb.t - sa.t < MIN_AIR_S {
            continue;
        }
        let top = (a..=b).max_by(|&x, &y| samples[x].pos[1].total_cmp(&samples[y].pos[1])).unwrap_or(a);
        let dx = sb.pos[0] - sa.pos[0];
        let dz = sb.pos[2] - sa.pos[2];
        out.push(Jump {
            start: sa.t - t0,
            end: sb.t - t0,
            apex: samples[top].t - t0,
            height: samples[top].pos[1] - sa.pos[1],
            length: (dx * dx + dz * dz).sqrt(),
        });
    }
    out
}

/// A small recording for tests: an event, a three-node skeleton with `status`, then three
/// samples 20 ms apart, the first two with a bones record (the second a repeat).
#[cfg(test)]
pub fn test_file(status: u8, with_bones: bool) -> Vec<u8> {
    fn rec(out: &mut Vec<u8>, tag: u8, p: &[u8]) {
        out.extend_from_slice(&[tag, 0, 0, 0]);
        out.extend_from_slice(&(p.len() as u32).to_le_bytes());
        out.extend_from_slice(p);
    }
    let f = |v: f32| v.to_le_bytes();
    let mut out = b"MXBC\0\0\0\0".to_vec();
    let mut ev = vec![0u8; 648];
    ev[332..336].copy_from_slice(&f(0.3));
    ev[336..340].copy_from_slice(&f(0.3));
    rec(&mut out, tag::EVENT, &ev);
    if with_bones {
        let mut sk = vec![1, status, 3, 2];
        sk.extend_from_slice(&0x6A21_833Du32.to_le_bytes());
        for v in [0.1f32, 0.2, 0.3] {
            sk.extend_from_slice(&f(v));
        }
        for (par, recd, name) in [(255u8, 0u8, "riderRIG_Root"), (0, 1, "riderRIG_Spine1"), (1, 1, "riderRIG_Head")] {
            sk.extend_from_slice(&[par, recd, name.len() as u8]);
            sk.extend_from_slice(name.as_bytes());
        }
        rec(&mut out, tag::RIDER_SKELETON, &sk);
    }
    for k in 0..3 {
        let t = 10.0 + 0.02 * k as f32;
        let mut s = vec![0u8; 8 + 180];
        s[0..4].copy_from_slice(&f(t));
        rec(&mut out, tag::SAMPLE, &s);
        if with_bones && k < 2 {
            let mut b = Vec::new();
            b.extend_from_slice(&f(t));
            b.extend_from_slice(&f(0.5));
            b.extend_from_slice(&[1, 2, k as u8, 0]);
            for v in [0.5f32, 0.5, 0.5] {
                b.extend_from_slice(&f(v));
            }
            // Head: identity rotation, 250 mm along each axis; Spine1: a quarter turn about y.
            for (node, q, p) in [(2u8, [0i16, 0, 0, 32767], [250i16, 250, 250]), (1, [0, 23170, 0, 23170], [-10, 0, 1000])] {
                b.push(node);
                for v in q.iter().chain(p.iter()) {
                    b.extend_from_slice(&v.to_le_bytes());
                }
            }
            rec(&mut out, tag::RIDER_BONES, &b);
        }
    }
    rec(&mut out, tag::END, &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_skeleton_and_bones() {
        let r = parse(&test_file(0, true)).unwrap();
        assert!(r.complete);
        assert_eq!(r.samples.len(), 3);
        let k = r.skeleton.as_ref().unwrap();
        assert_eq!((k.status, k.exe_stamp), (0, 0x6A21_833D));
        assert_eq!(k.bike_offset, [0.1, 0.2, 0.3]);
        assert_eq!(k.nodes.len(), 3);
        assert_eq!(k.nodes[0], Node { name: "riderRIG_Root".into(), parent: None, recorded: false });
        assert_eq!(k.nodes[2].parent, Some(1));
        let b = r.bones_at(r.samples[0].t).unwrap();
        assert!(!b.repeat);
        assert_eq!(b.bones.len(), 2);
        assert_eq!(b.bones[0].0, "riderRIG_Head");
        let want = [1.0, 0.0, 0.0, 0.0, 0.1 + 0.5 + 0.25, 0.2 + 0.5 + 0.25, 0.3 + 0.5 + 0.25];
        for (a, w) in b.bones[0].1.iter().zip(want) {
            assert!((a - w).abs() < 1e-4, "{:?}", b.bones[0].1);
        }
        let spine = b.bones[1].1;
        assert_eq!(spine[0], 0.70711);
        assert_eq!(spine[2], 0.70711);
        assert!((spine[4] - (0.1 + 0.5 - 0.01)).abs() < 1e-4);
        assert!((spine[6] - (0.3 + 0.5 + 1.0)).abs() < 1e-4);
        assert!(r.bones_at(r.samples[1].t).unwrap().repeat);
        assert!(r.bones_at(r.samples[2].t).is_none());
    }

    #[test]
    fn unknown_build_has_no_nodes_and_no_bones() {
        let r = parse(&test_file(1, true)).unwrap();
        let k = r.skeleton.clone().unwrap();
        assert_eq!(k.status, 1);
        assert!(k.nodes.is_empty());
        assert!(!r.has_bones());
    }

    #[test]
    fn older_recordings_have_neither() {
        let r = parse(&test_file(0, false)).unwrap();
        assert!(r.skeleton.is_none());
        assert!(!r.has_bones());
    }

    /// Sean's recording of 2026-10-29 (the 007 moment), where it is on this PC.
    #[test]
    #[ignore]
    fn real_session_has_the_head() {
        let p = r"C:\Users\Sean Dahan\Documents\PiBoSo\MX Bikes\mxbcoach\sessions\20261029-221339-533.mxbc";
        let Ok(bytes) = std::fs::read(p) else { return };
        let r = parse(&bytes).unwrap();
        assert_eq!(r.skeleton.as_ref().map(|k| k.status), Some(0));
        let with = r.samples.iter().filter(|s| r.bones_at(s.t).is_some_and(|b| b.bones.iter().any(|(n, _)| n == "riderRIG_Head"))).count();
        assert!(with * 10 > r.samples.len() * 9, "{with} of {}", r.samples.len());
    }
}
