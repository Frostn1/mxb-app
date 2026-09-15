//! The HUD sheet the recorder draws from in practice: the lap to race against, for the gap and
//! the ghost on the map, and each section's name with its tip. Written beside the cue sheet.
//!
//! FrostMod's `src/coachhud.h`, `MXHD` version 1, little-endian, pinned byte for byte by
//! `tests/coachhud_test.cpp` there and by the test here. The layout, in order: magic, version,
//! track length; the reference lap's points (track position 0..1, seconds since the line, world
//! x and z); the sections (start and end in metres from the line, name, tip); then flags.

use crate::analysis::{Review, Trace, STEP_M};

pub const MAGIC: &[u8; 4] = b"MXHD";
pub const VERSION: u32 = 1;
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
    for i in (0..n).step_by(every).chain((n > 0 && (n - 1) % every != 0).then_some(n - 1)) {
        let p = &fast.pts[i];
        let pos = if track_len > 0.0 { (i as f32 * STEP_M / track_len).min(1.0) } else { 0.0 };
        let elapsed = (p.t - t0).max(points.last().map_or(0.0, |q| q[1]));
        points.push([pos, elapsed, p.x, p.z]);
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
        assert_eq!(b.len(), 99);
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
}
