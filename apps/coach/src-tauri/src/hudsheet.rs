//! The HUD sheet the recorder draws from in practice: the lap to race against, for the gap and
//! the ghost on the map, and each section's name with its tip. Written beside the cue sheet.
//!
//! FrostMod's `src/coachhud.h`, `MXHD` version 1, little-endian, pinned byte for byte by
//! `tests/coachhud_test.cpp` there and by the test here. The layout, in order: magic, version,
//! track length; the reference lap's points (track position 0..1, seconds since the line, world
//! x and z); the sections (start and end in metres from the line, name, tip); then flags.
//!
//! Then chunks, each a four-byte tag, a u32 length and that many bytes, which a recorder that
//! predates them skips (one that predates chunks reads nothing past the flags):
//! - `DRIV`: per point, speed m/s, throttle 0..1 and brake 0..1 (the harder of the two), so the
//!   recorder colours its line on the track by where the lap braked (FrostMod v0.43.2).
//! - `REFY`: per point, the reference lap's own height (the bike's y).
//! - `GEAR`: per point, the gear the reference lap was in, one byte each (0 = neutral or unknown), so
//!   the recorder can hint a shift where the rider is in another gear (FrostMod `coachgear.h`).
//! - `TRRN`: per point, the track's own ground height at five offsets across the line, half a
//!   metre apart, right to left, NaN off the grid; only when the laps sit steadily on this
//!   terrain (`ground::measure`). The recorder lays its line on it instead of the centreline.
//! - `AIRH`: per point, the bike's height (world y, m), its height above the track's own ground
//!   at the line's centre (NaN without the terrain), and the share of the lap's samples from that
//!   point to the next that were airborne, 0..1, so the recorder can call the jumps: where to
//!   take off, where the lap landed, and how many faces it cleared (FrostMod v0.44.1).

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

/// The track's terrain, as `ground::height_at` reads it.
pub struct Terrain<'a> {
    pub width: usize,
    pub height: usize,
    pub metres_per_sample: f32,
    pub heights: &'a [f32],
}

/// Ground samples across the line at each point, and how far apart.
pub const TERRAIN_ACROSS: usize = 5;
pub const TERRAIN_STEP_M: f32 = 0.5;

/// The `.hud` file for this track, from the fast lap and the sections.
pub fn write(track_len: f32, fast: &Trace, parts: &[Part], flags: u32) -> Vec<u8> {
    write_with(track_len, fast, parts, flags, None)
}

/// [`write`], with the ground under the line when the track's terrain is known.
pub fn write_with(track_len: f32, fast: &Trace, parts: &[Part], flags: u32, terrain: Option<&Terrain>) -> Vec<u8> {
    let mut b = MAGIC.to_vec();
    b.extend_from_slice(&VERSION.to_le_bytes());
    b.extend_from_slice(&track_len.to_le_bytes());
    // The lap on its metre grid, thinned to what the plugin reads. Position and time only ever
    // rise, which the plugin checks.
    let n = fast.pts.len();
    let every = n.div_ceil(MAX_POINTS).max(1);
    let t0 = fast.pts.first().map_or(0.0, |p| p.t);
    let mut points: Vec<[f32; 4]> = Vec::new();
    let mut taken: Vec<usize> = Vec::new();
    for i in (0..n).step_by(every).chain((n > 0 && (n - 1) % every != 0).then_some(n - 1)) {
        taken.push(i);
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

    // DRIV: how the lap was ridden at each point.
    let mut driv = (points.len() as u32).to_le_bytes().to_vec();
    for &i in &taken {
        let p = &fast.pts[i];
        let clean = |v: f32, hi: f32| if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 };
        for v in [clean(p.v, 200.0), clean(p.throttle, 1.0), clean(p.front.max(p.rear), 1.0)] {
            driv.extend_from_slice(&v.to_le_bytes());
        }
    }
    chunk(&mut b, b"DRIV", &driv);

    // REFY: the reference lap's own height at each point (the bike's y as recorded), so the
    // recorder's line follows the ground's rise along the line on tracks whose terrain it can't
    // read (FrostMod v0.43.5).
    let mut refy = (points.len() as u32).to_le_bytes().to_vec();
    for &i in &taken {
        let y = fast.pts[i].y;
        refy.extend_from_slice(&(if y.is_finite() { y } else { 0.0 }).to_le_bytes());
    }
    chunk(&mut b, b"REFY", &refy);

    // AIRH: where the lap was in the air. Airborne is a share over the samples a point stands
    // for, so a thinned sheet still sees a short hop between two of its points.
    let mut airh = (points.len() as u32).to_le_bytes().to_vec();
    for (j, &i) in taken.iter().enumerate() {
        let p = &fast.pts[i];
        let end = taken.get(j + 1).copied().unwrap_or(i + 1).max(i + 1).min(n);
        let share = fast.pts[i..end].iter().filter(|q| q.air).count() as f32 / (end - i) as f32;
        let above = terrain
            .and_then(|t| crate::ground::height_at(t.width, t.height, t.metres_per_sample, t.heights, p.x, p.z))
            .map_or(f32::NAN, |g| p.y - g);
        let y = if p.y.is_finite() { p.y } else { f32::NAN };
        for v in [y, above, share] {
            airh.extend_from_slice(&v.to_le_bytes());
        }
    }
    chunk(&mut b, b"AIRH", &airh);

    // GEAR: the gear the reference lap was in at each point, a byte each; 0 where the gear is
    // neutral, unknown or out of range. A plugin that predates the tag skips it by its length.
    let mut gear = (points.len() as u32).to_le_bytes().to_vec();
    gear.extend(taken.iter().map(|&i| u8::try_from(fast.pts[i].gear).ok().filter(|g| *g <= 9).unwrap_or(0)));
    chunk(&mut b, b"GEAR", &gear);

    // TRRN: the ground across the line, left being the direction of travel turned a quarter
    // anticlockwise seen from above (x east, z north).
    if let Some(t) = terrain {
        let mut trrn = (points.len() as u32).to_le_bytes().to_vec();
        trrn.extend_from_slice(&(TERRAIN_ACROSS as u32).to_le_bytes());
        trrn.extend_from_slice(&TERRAIN_STEP_M.to_le_bytes());
        for j in 0..points.len() {
            let (a, c) = (&points[j.saturating_sub(1)], &points[(j + 1).min(points.len() - 1)]);
            let (dx, dz) = (c[2] - a[2], c[3] - a[3]);
            let len = (dx * dx + dz * dz).sqrt();
            let (nx, nz) = if len > 1e-3 { (-dz / len, dx / len) } else { (0.0, 0.0) };
            for k in 0..TERRAIN_ACROSS {
                let off = (k as f32 - (TERRAIN_ACROSS as f32 - 1.0) / 2.0) * TERRAIN_STEP_M;
                let (x, z) = (points[j][2] + nx * off, points[j][3] + nz * off);
                let h = crate::ground::height_at(t.width, t.height, t.metres_per_sample, t.heights, x, z)
                    .unwrap_or(f32::NAN);
                trrn.extend_from_slice(&h.to_le_bytes());
            }
        }
        chunk(&mut b, b"TRRN", &trrn);
    }
    b
}

fn chunk(b: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    b.extend_from_slice(tag);
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    b.extend_from_slice(payload);
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
        // Then DRIV: three points, speed, throttle, brake.
        assert_eq!(&b[99..103], b"DRIV");
        assert_eq!((u32_at(103), u32_at(107)), (4 + 3 * 12, 3));
        // Then REFY: three heights.
        assert_eq!(&b[147..151], b"REFY");
        assert_eq!((u32_at(151), u32_at(155)), (4 + 3 * 4, 3));
        // Then AIRH: three points, height, height above the ground (unknown here), airborne.
        assert_eq!(&b[171..175], b"AIRH");
        assert_eq!((u32_at(175), u32_at(179)), (4 + 3 * 12, 3));
        assert!(f32_at(187).is_nan(), "no terrain, no height above it");
        // Then GEAR: three bytes, 0 where the lap had no gear.
        assert_eq!(&b[219..223], b"GEAR");
        assert_eq!((u32_at(223), u32_at(227)), (4 + 3, 3));
        assert_eq!(&b[231..234], &[0, 0, 0]);
        assert_eq!(b.len(), 234, "no TRRN without terrain");
    }

    #[test]
    fn the_gear_of_each_point_rides_along() {
        let fast = Trace {
            pts: [2, 3, 0, -1, 40].iter().enumerate().map(|(i, &g)| Point { t: i as f32, x: i as f32, gear: g, ..Point::default() }).collect(),
        };
        let b = write(5.0, &fast, &[], 0);
        let at = b.windows(4).position(|w| w == b"GEAR").expect("a GEAR chunk");
        let n = u32::from_le_bytes(b[at + 8..at + 12].try_into().unwrap()) as usize;
        assert_eq!(n, 5);
        assert_eq!(&b[at + 12..at + 12 + n], &[2, 3, 0, 0, 0], "neutral, negative and absurd gears read as unknown");
    }

    #[test]
    fn the_ground_across_the_line_rides_along_when_the_terrain_is_known() {
        // A 10 x 10 grid a metre apart, rising a metre per metre north (z).
        let heights: Vec<f32> = (0..100).map(|i| (i / 10) as f32).collect();
        let t = Terrain { width: 10, height: 10, metres_per_sample: 1.0, heights: &heights };
        // Riding east along z = 5: left is north, so the five samples run 4, 4.5, 5, 5.5, 6.
        let fast = Trace { pts: (0..4).map(|i| Point { t: i as f32, x: 2.0 + i as f32, z: 5.0, v: 9.0, front: 0.5, ..Point::default() }).collect() };
        let b = write_with(4.0, &fast, &[], 0, Some(&t));
        let at = b.windows(4).position(|w| w == b"TRRN").expect("a TRRN chunk");
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let f32_at = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!((u32_at(at + 8), u32_at(at + 12), f32_at(at + 16)), (4, 5, 0.5));
        let row: Vec<f32> = (0..5).map(|k| f32_at(at + 20 + k * 4)).collect();
        assert_eq!(row, vec![4.0, 4.5, 5.0, 5.5, 6.0]);
        assert_eq!(u32_at(at + 4) as usize, 12 + 4 * 5 * 4);
        // And DRIV carries the speed and the brake.
        let d = b.windows(4).position(|w| w == b"DRIV").expect("a DRIV chunk");
        assert_eq!((f32_at(d + 12), f32_at(d + 20)), (9.0, 0.5));
        // Off the grid is NaN, which the recorder reads as "not known here".
        let off = Trace { pts: (0..2).map(|i| Point { t: i as f32, x: 50.0 + i as f32, z: 50.0, ..Point::default() }).collect() };
        let b2 = write_with(2.0, &off, &[], 0, Some(&t));
        let at2 = b2.windows(4).position(|w| w == b"TRRN").unwrap();
        assert!(f32::from_le_bytes(b2[at2 + 20..at2 + 24].try_into().unwrap()).is_nan());
    }

    #[test]
    fn the_air_rides_along_so_the_recorder_can_call_the_jumps() {
        // Flat ground at 2 m; the bike rides 0.6 m up, then four points in the air.
        let heights = vec![2.0f32; 100];
        let t = Terrain { width: 10, height: 10, metres_per_sample: 1.0, heights: &heights };
        let fast = Trace {
            pts: (0..8)
                .map(|i| {
                    let air = (2..6).contains(&i);
                    Point { t: i as f32 * 0.1, x: 1.0 + i as f32, z: 5.0, y: if air { 4.0 } else { 2.6 }, air, ..Point::default() }
                })
                .collect(),
        };
        let b = write_with(8.0, &fast, &[], 0, Some(&t));
        let at = b.windows(4).position(|w| w == b"AIRH").expect("an AIRH chunk");
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let f32_at = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!((u32_at(at + 4), u32_at(at + 8)), (4 + 8 * 12, 8));
        let row = |j: usize| (0..3).map(|k| f32_at(at + 12 + j * 12 + k * 4)).collect::<Vec<_>>();
        let near = |a: Vec<f32>, b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(near(row(0), [2.6, 0.6, 0.0]), "{:?}", row(0));
        assert!(near(row(3), [4.0, 2.0, 1.0]), "{:?}", row(3));
        // A long lap thinned to the plugin's points keeps a hop between two of them.
        let long = Trace {
            pts: (0..4001).map(|i| Point { t: i as f32 * 0.05, x: i as f32, air: i == 1, ..Point::default() }).collect(),
        };
        let b = write(4001.0, &long, &[], 0);
        let at = b.windows(4).position(|w| w == b"AIRH").unwrap();
        let first = f32::from_le_bytes(b[at + 20..at + 24].try_into().unwrap());
        assert!(first > 0.0 && first < 1.0, "{first}");
    }

    #[test]
    fn a_long_lap_is_thinned_to_what_the_plugin_reads_and_keeps_its_end() {
        let b = write(5000.0, &lap(5001), &[], 0);
        let n = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        assert!(n <= MAX_POINTS, "{n}");
        let last = 16 + (n - 1) * 16;
        assert_eq!(f32::from_le_bytes(b[last..last + 4].try_into().unwrap()), 1.0);
    }

    /// Not a check: writes the sheet Coach would write for the fastest whole lap of a track in a
    /// folder of recordings, with the track's terrain, for FrostMod's offline previews.
    /// `MXBC_DIR` (the sessions folder), `MXBC_TRACK` (part of the track id), `MXB_TRACK_ARCHIVE`
    /// (its .pkz) and `HUD_OUT`; `GRID_OUT` also dumps the terrain grid. `cargo test -p mxb-coach -- --ignored write_a_sheet`.
    #[test]
    #[ignore]
    fn write_a_sheet_from_recorded_sessions() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("set {k}"));
        let (dir, needle, archive, out) = (var("MXBC_DIR"), var("MXBC_TRACK"), var("MXB_TRACK_ARCHIVE"), var("HUD_OUT"));
        let mut best: Option<(i32, Trace, f32)> = None;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let Ok(rec) = crate::coach::load(&e.path().to_string_lossy()) else { continue };
            if !rec.event.track_id.contains(&needle) {
                continue;
            }
            for lap in rec.laps().iter().filter(|l| l.whole && l.time_ms > 0) {
                if best.as_ref().is_some_and(|b| b.0 <= lap.time_ms) {
                    continue;
                }
                if let Some(tr) = Trace::new(lap, rec.event.track_length) {
                    best = Some((lap.time_ms, tr, rec.event.track_length));
                }
            }
        }
        let (ms, fast, len) = best.expect("no whole lap of that track");
        let m = mxb_core::track::decode_master(std::path::Path::new(&archive)).unwrap();
        let t = Terrain { width: m.info.width as usize, height: m.info.height as usize, metres_per_sample: m.info.metres_per_sample, heights: &m.heights };
        // Coach only writes the terrain when the lap sits steadily on it; say whether this one does.
        let grounded: Vec<[f32; 3]> = fast.pts.iter().filter(|p| !p.air).map(|p| [p.x, p.y, p.z]).collect();
        let fit = crate::ground::measure(t.width, t.height, t.metres_per_sample, t.heights, &grounded);
        std::fs::write(&out, write_with(len, &fast, &[], 0, Some(&t))).unwrap();
        println!("{out}: lap {ms} ms, {} points, lift {:?} spread {:.2}", fast.pts.len(), fit.lift, fit.spread);
        // And the grid itself, for a 3D preview: u32 width, u32 height, f32 metres a sample, heights.
        if let Ok(grid) = std::env::var("GRID_OUT") {
            let mut g = (t.width as u32).to_le_bytes().to_vec();
            g.extend_from_slice(&(t.height as u32).to_le_bytes());
            g.extend_from_slice(&t.metres_per_sample.to_le_bytes());
            for h in t.heights {
                g.extend_from_slice(&h.to_le_bytes());
            }
            std::fs::write(grid, g).unwrap();
        }
    }

    #[test]
    fn file_names_match_the_plugin() {
        assert_eq!(file_name("indiana nationals", "MX2OEM_2023_KTM_250_SX-F"), "indiana_nationals.MX2OEM_2023_KTM_250_SX-F.hud");
    }
}
