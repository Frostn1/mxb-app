//! The HUD sheet the recorder draws from in practice: the lap to race against, for the gap and
//! the ghost on the map, and each section's name with its tip. Written beside the cue sheet.
//!
//! FrostMod's `src/coachhud.h`, `MXHD` version 1, little-endian, pinned byte for byte by
//! `tests/coachhud_test.cpp` there and by the test here. The layout, in order: magic, version,
//! track length; the reference lap's points (track position 0..1, seconds since the line, world
//! x and z); the sections (start and end in metres from the line, name, tip); then flags; then
//! an optional channel block, `CHAN`, a count equal to the point count, and per point the speed
//! (m/s), throttle (0..1) and brake (0..1, the larger of front and rear) of the same lap. Plugins
//! that predate it stop reading at the flags, and a sheet without it just has no channels, so the
//! version stays 1.

use crate::analysis::{Review, Trace, STEP_M};

pub const MAGIC: &[u8; 4] = b"MXHD";
pub const VERSION: u32 = 1;
/// Tags the optional per-point channel block after the flags.
pub const CHANNELS: &[u8; 4] = b"CHAN";
/// Bit 0: ask the rider to stop two seconds in neutral, so the coach can measure sag.
pub const SAG_PROMPT: u32 = 1;
/// The plugin reads at most this many points and sections, and 255 bytes of each text.
const MAX_POINTS: usize = 2000;
const MAX_SECTIONS: usize = 64;
const MAX_TEXT: usize = 255;

/// A section of the track as the HUD names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub start: f32,
    pub end: f32,
    pub name: String,
    pub tip: String,
}

/// The review's sections, each with the tip that matters most there, or none.
pub fn parts(review: &Review, track_len: f32) -> Vec<Part> {
    review
        .sections
        .iter()
        .filter_map(|s| {
            let (start, end) = (s.section.start as f32 * STEP_M, (s.section.end as f32 * STEP_M).min(track_len));
            (start < end).then(|| Part {
                start,
                end,
                name: s.section.name.clone(),
                tip: s.findings.iter().find(|f| f.skill != "unclear").map(|f| f.title.clone()).unwrap_or_default(),
            })
        })
        .take(MAX_SECTIONS)
        .collect()
}

/// The text cut to what the plugin reads, on a character boundary.
fn text(s: &str) -> &[u8] {
    let mut end = s.len().min(MAX_TEXT);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s.as_bytes()[..end]
}

/// The `.hud` file for this track, from the fast lap and the sections.
pub fn write(track_len: f32, fast: &Trace, parts: &[Part], flags: u32) -> Vec<u8> {
    let mut b = MAGIC.to_vec();
    b.extend_from_slice(&VERSION.to_le_bytes());
    b.extend_from_slice(&track_len.to_le_bytes());
    // The lap on its metre grid, thinned to what the plugin reads. Position and time only ever
    // rise, which the plugin checks.
    let n = fast.pts.len();
    let every = n.div_ceil(MAX_POINTS).max(1);
    let t0 = fast.pts.first().map_or(0.0, |p| p.t);
    let mut points: Vec<[f32; 4]> = Vec::new();
    let mut chans: Vec<[f32; 3]> = Vec::new();
    for i in (0..n).step_by(every).chain((n > 0 && (n - 1) % every != 0).then_some(n - 1)) {
        let p = &fast.pts[i];
        let pos = if track_len > 0.0 { (i as f32 * STEP_M / track_len).min(1.0) } else { 0.0 };
        let elapsed = (p.t - t0).max(points.last().map_or(0.0, |q| q[1]));
        points.push([pos, elapsed, p.x, p.z]);
        let unit = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        chans.push([if p.v.is_finite() { p.v.max(0.0) } else { 0.0 }, unit(p.throttle), unit(p.front.max(p.rear))]);
    }
    b.extend_from_slice(&(points.len() as u32).to_le_bytes());
    for p in &points {
        for v in p {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b.extend_from_slice(&(parts.len().min(MAX_SECTIONS) as u32).to_le_bytes());
    for s in parts.iter().take(MAX_SECTIONS) {
        b.extend_from_slice(&s.start.to_le_bytes());
        b.extend_from_slice(&s.end.to_le_bytes());
        for t in [text(&s.name), text(&s.tip)] {
            b.push(t.len() as u8);
            b.extend_from_slice(t);
        }
    }
    b.extend_from_slice(&flags.to_le_bytes());
    if !chans.is_empty() {
        b.extend_from_slice(CHANNELS);
        b.extend_from_slice(&(chans.len() as u32).to_le_bytes());
        for c in &chans {
            for v in c {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    b
}

/// The file the plugin looks for first: this bike on this track, beside the `.cue`.
pub fn file_name(track: &str, bike: &str) -> String {
    format!("{}.{}.hud", crate::cues::safe_name(track), crate::cues::safe_name(bike))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Point;

    fn lap(n: usize) -> Trace {
        Trace { pts: (0..n).map(|i| Point { t: 10.0 + i as f32 * 0.1, x: i as f32, z: -(i as f32), ..Point::default() }).collect() }
    }

    #[test]
    fn writes_the_file_the_plugin_reads() {
        let parts = [Part { start: 0.0, end: 2.0, name: "Turn 1".into(), tip: "Brake later".into() }];
        let b = write(4.0, &lap(3), &parts, SAG_PROMPT);
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let f32_at = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!(&b[..4], b"MXHD");
        assert_eq!((u32_at(4), f32_at(8), u32_at(12)), (1, 4.0, 3));
        // Three points, 16 bytes each: position, seconds since the line, x, z.
        assert_eq!((f32_at(16), f32_at(20), f32_at(24), f32_at(28)), (0.0, 0.0, 0.0, 0.0));
        assert_eq!((f32_at(48), f32_at(56), f32_at(60)), (0.5, 2.0, -2.0));
        assert!((f32_at(52) - 0.2).abs() < 1e-5);
        assert_eq!(u32_at(64), 1);
        assert_eq!((f32_at(68), f32_at(72)), (0.0, 2.0));
        assert_eq!((b[76], &b[77..83]), (6, &b"Turn 1"[..]));
        assert_eq!((b[83], &b[84..95]), (11, &b"Brake later"[..]));
        assert_eq!(u32_at(95), SAG_PROMPT);
        // Then the channels: tag, count, and speed, throttle, brake per point.
        assert_eq!((&b[99..103], u32_at(103)), (&b"CHAN"[..], 3));
        assert_eq!(b.len(), 99 + 8 + 3 * 12);
    }

    #[test]
    fn channels_follow_the_lap_and_stay_in_range() {
        let mut l = lap(2);
        l.pts[0] = Point { v: 20.0, throttle: 0.5, front: 0.25, rear: 0.75, ..l.pts[0] };
        l.pts[1] = Point { v: f32::NAN, throttle: 3.0, front: -1.0, ..l.pts[1] };
        let b = write(2.0, &l, &[], 0);
        let at = b.len() - 24;
        let f: Vec<f32> = (0..6).map(|i| f32::from_le_bytes(b[at + i * 4..at + i * 4 + 4].try_into().unwrap())).collect();
        assert_eq!(f, [20.0, 0.5, 0.75, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_long_lap_is_thinned_to_what_the_plugin_reads_and_keeps_its_end() {
        let b = write(5000.0, &lap(5001), &[], 0);
        let n = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        assert!(n <= MAX_POINTS, "{n}");
        let last = 16 + (n - 1) * 16;
        assert_eq!(f32::from_le_bytes(b[last..last + 4].try_into().unwrap()), 1.0);
    }

    #[test]
    fn file_names_match_the_plugin() {
        assert_eq!(file_name("indiana nationals", "MX2OEM_2023_KTM_250_SX-F"), "indiana_nationals.MX2OEM_2023_KTM_250_SX-F.hud");
    }

    /// Offline regeneration of one sheet from recordings on disk, for checking the channels
    /// without the app. Run by hand:
    ///   HUD_SESSIONS=<mxbcoach\sessions> HUD_SHEET=<existing .hud> HUD_TRACK=755_Compound     ///   HUD_BIKE=MX2OEM_2023_KTM_250_SX-F HUD_OUT=<new .hud> cargo test regenerate_sheet -- --ignored
    /// It races the fastest whole lap for that track and bike, and keeps the sections and flags
    /// of the existing sheet. The app does the same, with the review's sections, when it writes cues.
    #[test]
    #[ignore]
    fn regenerate_sheet() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
        let (track, bike) = (var("HUD_TRACK"), var("HUD_BIKE"));
        let mut best: Option<(i32, crate::telemetry::Recording, i32)> = None;
        for e in std::fs::read_dir(var("HUD_SESSIONS")).unwrap().flatten() {
            let Ok(rec) = crate::coach::load(&e.path().to_string_lossy()) else { continue };
            if crate::cues::safe_name(&rec.event.track_id) != track || crate::cues::safe_name(&rec.event.bike_id) != bike {
                continue;
            }
            for l in rec.laps().iter().filter(|l| l.whole && !l.invalid) {
                if best.as_ref().map_or(true, |b| l.time_ms < b.0) {
                    best = Some((l.time_ms, rec.clone(), l.num));
                }
            }
        }
        let (ms, rec, num) = best.expect("no whole lap for that track and bike");
        let lap = rec.laps().into_iter().find(|l| l.num == num).unwrap();
        let fast = Trace::new(&lap, rec.event.track_length).unwrap();
        // The sections and flags of the sheet already in the game.
        let old = std::fs::read(var("HUD_SHEET")).unwrap();
        let u32_at = |i: usize| u32::from_le_bytes(old[i..i + 4].try_into().unwrap());
        let mut at = 16 + u32_at(12) as usize * 16;
        let n = u32_at(at);
        at += 4;
        let mut parts = Vec::new();
        for _ in 0..n {
            let f = |i: usize| f32::from_le_bytes(old[i..i + 4].try_into().unwrap());
            let (start, end) = (f(at), f(at + 4));
            at += 8;
            let mut t = [String::new(), String::new()];
            for s in &mut t {
                let l = old[at] as usize;
                *s = String::from_utf8_lossy(&old[at + 1..at + 1 + l]).into_owned();
                at += 1 + l;
            }
            let [name, tip] = t;
            parts.push(Part { start, end, name, tip });
        }
        let flags = u32_at(at);
        std::fs::write(var("HUD_OUT"), write(rec.event.track_length, &fast, &parts, flags)).unwrap();
        println!("lap {num} ({ms} ms), {} sections, flags {flags}", parts.len());
    }
}
