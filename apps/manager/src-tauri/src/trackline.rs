//! The centreline a published track was built from, read back out of its `.trh`.
//!
//! `tracked -merge` takes a `.tcl` — a start pose and a list of straights and arcs — and
//! writes it into the height file. It is still in there, and it is the same vocabulary
//! [`crate::trackprog`] is written in, so a published lap decodes straight into a track
//! program: every corner's radius and angle, every straight's length, exactly as its builder
//! typed them.
//!
//! That is the only way to see a real track's *layout* as numbers. Everything else we measure
//! comes off the terrain and describes what the ground looks like; this describes what
//! somebody decided.

#![allow(dead_code)]

use crate::trackprog::Segment;

/// Bytes per segment record.
const RECORD: usize = 60;

/// One entry of the centreline, as the file states it.
#[derive(Clone, Copy, Debug)]
pub struct LineSegment {
    /// Metres along the ground.
    pub length: f32,
    /// Signed radius, metres; zero on a straight. Positive turns right, as everywhere else
    /// in the pipeline.
    pub radius: f32,
    /// Degrees turned through, unsigned — the sign is the radius's.
    pub angle: f32,
    /// Ground height at the segment's start, metres.
    pub elevation: f32,
    /// Metres round the lap the segment starts at.
    pub at: f32,
    pub x: f32,
    pub z: f32,
    /// Radians, per [`crate::trackprog::heading_vector`].
    pub heading: f32,
}

impl LineSegment {
    pub fn is_corner(&self) -> bool {
        self.radius != 0.0 && self.angle >= crate::trackprog::CORNER_DEG
    }
}

/// A track's lap, as its builder drew it.
#[derive(Clone, Debug)]
pub struct Lap {
    pub start: (f32, f32),
    /// Degrees.
    pub heading: f32,
    /// The lap's own length, as the file states it.
    pub length: f32,
    pub segments: Vec<LineSegment>,
}

impl Lap {
    /// The same lap as a track program's segment list.
    pub fn program_segments(&self) -> Vec<Segment> {
        self.segments
            .iter()
            .map(|s| {
                if s.radius == 0.0 || s.angle == 0.0 {
                    Segment::Straight { length: s.length, rise: 0.0 }
                } else {
                    // The angle is a magnitude: which way a corner goes is the radius's
                    // sign, here and everywhere else in the pipeline.
                    Segment::Arc { radius: s.radius, angle: s.angle, rise: 0.0 }
                }
            })
            .collect()
    }

    /// The lap's corners, by the one definition of what a corner is.
    pub fn turns(&self) -> Vec<(f32, f32)> {
        crate::trackprog::turns(&self.program_segments())
    }
}

/// A point on the lap: where it is, which way it faces, and how far round it sits.
#[derive(Clone, Copy, Debug)]
pub struct Station {
    pub x: f32,
    pub z: f32,
    /// Radians, per [`crate::trackprog::heading_vector`].
    pub heading: f32,
    /// Metres round the lap.
    pub at: f32,
}

impl Lap {
    /// Walk the lap at roughly `step` metres.
    ///
    /// Each station is placed from its own segment's stated pose rather than integrated from
    /// the one before, which is what keeps a two-kilometre walk from drifting.
    pub fn stations(&self, step: f32) -> Vec<Station> {
        let step = step.max(0.05);
        let mut out = Vec::new();
        for seg in &self.segments {
            let steps = ((seg.length / step) as usize).max(1);
            for k in 0..steps {
                let d = k as f32 * seg.length / steps as f32;
                let (x, z, heading) = if seg.radius == 0.0 {
                    let (hx, hz) = crate::trackprog::heading_vector(seg.heading);
                    (seg.x + d * hx, seg.z + d * hz, seg.heading)
                } else {
                    let h = seg.heading + d / seg.radius;
                    (
                        seg.x + seg.radius * (seg.heading.cos() - h.cos()),
                        seg.z + seg.radius * (h.sin() - seg.heading.sin()),
                        h,
                    )
                };
                out.push(Station { x, z, heading, at: seg.at + d });
            }
        }
        out
    }
}

/// Read the centreline out of a height file's trailing block.
///
/// The block's tail is fixed once the coverage masks are behind you, and the material table
/// is what says where that is — it opens with a name we can search for, which is how
/// [`crate::track::material_table_offset`] finds it without walking through mask bytes that
/// read as plausible headers. From there:
///
/// ```text
/// table - 40   f32 x, f32 z, f32 heading°     the start pose
/// table - 28   f32 length                     the lap
/// table - 24   f32 x6                         bounding box
/// table         u32 count, count x 52         materials
///  …            u32 count, count x 60         the centreline
/// ```
pub fn read(block: &[u8]) -> Option<Lap> {
    let table = crate::track::material_table_offset(block)?;
    let f32_at = |o: usize| -> Option<f32> {
        block.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()))
    };
    let u32_at = |o: usize| -> Option<u32> {
        block.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };

    let pose = table.checked_sub(40)?;
    let (x, z, heading) = (f32_at(pose)?, f32_at(pose + 4)?, f32_at(pose + 8)?);
    let length = f32_at(pose + 12)?;
    if !length.is_finite() || !(50.0..100_000.0).contains(&length) {
        return None;
    }

    let materials = u32_at(table)? as usize;
    if materials > 64 {
        return None;
    }
    let at = table + 4 + materials * 52;
    let count = u32_at(at)? as usize;
    if count == 0 || count > 8192 || at + 4 + count * RECORD > block.len() {
        return None;
    }

    let mut segments = Vec::with_capacity(count);
    let mut total = 0.0f32;
    for i in 0..count {
        let o = at + 4 + i * RECORD;
        let g = |k: usize| f32_at(o + k * 4);
        let seg = LineSegment {
            length: g(1)?,
            radius: g(2)?,
            angle: g(3)?.abs(),
            elevation: g(4)?,
            at: g(5)?,
            x: g(8)?,
            z: g(11)?,
            heading: g(7)?.atan2(g(6)?),
        };
        // Every field has to be a length, a radius and an angle, and the running total has to
        // land where the record says it starts. Mask bytes can look like one record; they
        // cannot look like a chain of them.
        if !seg.length.is_finite() || !(0.0..2000.0).contains(&seg.length) {
            return None;
        }
        if !seg.radius.is_finite() || seg.radius.abs() > 1e6 {
            return None;
        }
        if (seg.at - total).abs() > 0.5 {
            return None;
        }
        total += seg.length;
        segments.push(seg);
    }
    if (total - length).abs() > 1.0 {
        return None;
    }

    Some(Lap { start: (x, z), heading, length, segments })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trailing block shaped like a real one: a material table with `asphalt` at its head,
    /// preceded by the pose and followed by the centreline.
    fn block(segments: &[(f32, f32, f32)]) -> Vec<u8> {
        let mut b = vec![0u8; 64];
        let pose: [f32; 10] = [
            100.0, 200.0, -45.0,
            segments.iter().map(|s| s.0).sum(),
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        for v in pose {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&2u32.to_le_bytes());
        for name in ["asphalt", "grass"] {
            let mut n = [0u8; 16];
            n[..name.len()].copy_from_slice(name.as_bytes());
            b.extend_from_slice(&n);
            b.extend_from_slice(&[0u8; 36]);
        }
        b.extend_from_slice(&(segments.len() as u32).to_le_bytes());
        let mut at = 0.0f32;
        for &(length, radius, angle) in segments {
            let mut r = [0f32; 15];
            r[1] = length;
            r[2] = radius;
            r[3] = angle;
            r[5] = at;
            r[6] = 1.0;
            r[10] = 1.0;
            r[14] = 1.0;
            for v in r {
                b.extend_from_slice(&v.to_le_bytes());
            }
            at += length;
        }
        b
    }

    #[test]
    fn a_centreline_reads_back_out_of_a_trailing_block() {
        let b = block(&[(60.0, 0.0, 0.0), (15.7, 10.0, 90.0), (40.0, 0.0, 0.0)]);
        let lap = read(&b).expect("centreline");
        assert_eq!(lap.segments.len(), 3);
        assert_eq!(lap.start, (100.0, 200.0));
        assert!((lap.length - 115.7).abs() < 0.01);
        assert!(lap.segments[0].radius == 0.0 && !lap.segments[0].is_corner());
        assert!(lap.segments[1].is_corner());
        assert!((lap.segments[2].at - 75.7).abs() < 0.01);
    }

    #[test]
    fn straights_and_arcs_come_back_as_program_segments() {
        let b = block(&[(60.0, 0.0, 0.0), (15.7, -10.0, 90.0)]);
        let segs = read(&b).unwrap().program_segments();
        assert!(matches!(segs[0], Segment::Straight { length, .. } if (length - 60.0).abs() < 0.01));
        // A left corner keeps its direction through the conversion, in the radius.
        assert!(matches!(segs[1], Segment::Arc { radius, angle, .. }
            if radius < 0.0 && (angle - 90.0).abs() < 0.01));
    }

    #[test]
    fn a_corner_is_its_whole_run_of_arcs() {
        // Four arcs the same way with no straight between them are one 160° turn, and the
        // tightest of them is the one a rider has to make.
        let b = block(&[
            (20.0, 40.0, 28.0),
            (14.0, 16.0, 50.0),
            (10.0, 8.0, 71.0),
            (4.0, 22.0, 11.0),
            (50.0, 0.0, 0.0),
            (12.0, -18.0, 38.0),
        ]);
        let turns = read(&b).unwrap().turns();
        assert_eq!(turns.len(), 2, "{turns:?}");
        assert!((turns[0].0 - 160.0).abs() < 0.01 && turns[0].1 == 8.0);
        assert!((turns[1].0 - 38.0).abs() < 0.01);
    }

    #[test]
    fn nothing_is_read_out_of_bytes_that_are_not_one() {
        assert!(read(&[0u8; 4096]).is_none());
        // A block whose records don't chain is a misread, not a lap.
        let mut b = block(&[(60.0, 0.0, 0.0), (40.0, 0.0, 0.0)]);
        let n = b.len() - 60;
        b[n + 20..n + 24].copy_from_slice(&900.0f32.to_le_bytes());
        assert!(read(&b).is_none());
    }
}

#[cfg(test)]
mod corner_survey {
    use super::*;
    use std::path::Path;

    /// Four corners of a real track, as pictures and as numbers.
    ///
    /// Corners come from the centreline the builder typed, which the `.trh` still carries, so
    /// the radius and the angle are stated rather than inferred. The shape is then measured
    /// off the heightfield around each one: how wide the worked ground is, how far the berm
    /// stands over the apex, and how rough the surface is once the corner's own fall is taken
    /// out.
    ///
    /// ```text
    /// FROST_TRACK=… FROST_DUMP=/tmp/corners \
    ///   cargo test --bin mxb-app -- --ignored --nocapture four_corners
    /// ```
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK and FROST_DUMP"]
    fn four_corners() {
        let path = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let dir = std::env::var("FROST_DUMP").expect("set FROST_DUMP");
        let label = std::env::var("FROST_LABEL").unwrap_or_else(|_| "track".into());
        std::fs::create_dir_all(&dir).unwrap();

        let m = crate::track::decode_master(Path::new(&path)).expect("terrain");
        // The centreline sits in the trailing block, straight after the samples.
        let names = crate::track::entry_names(Path::new(&path)).expect("entries");
        let hf = crate::track::heightfield_entries(&names)
            .into_iter()
            .next()
            .expect("a heightfield");
        let hb = crate::track::read_entry(Path::new(&path), &hf).expect("read it");
        let layout = crate::heightfield::probe(&hb, None).expect("a terrain grid");
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let lap = read(hb.get(block_at..).unwrap_or(&[])).expect("a centreline in the .trh");
        let (gw, gh) = (m.info.width as usize, m.info.height as usize);
        let mpp = m.info.metres_per_sample;
        let half = gw as f32 * mpp * 0.5;
        // The centreline is stated from a corner of the plot, not its centre: Indiana's x
        // runs 22 to 470 across a 525 m site. Which way each axis then runs is not stated,
        // so it is chosen by looking: the mapping that puts the lap on rutted ground wins,
        // and the three that put it in a field are flat.
        let (fx_dir, fz_dir) = pick_axes(&m, &lap, mpp, gw, gh);
        let at = |x: f32, z: f32| -> f32 {
            let sx = if fx_dir { x } else { gw as f32 * mpp - x };
            let sz = if fz_dir { z } else { gh as f32 * mpp - z };
            let gx = (sx / mpp).clamp(0.0, gw as f32 - 1.001);
            let gz = (sz / mpp).clamp(0.0, gh as f32 - 1.001);
            let (x0, z0) = (gx as usize, gz as usize);
            let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
            let h = |a: usize, b: usize| m.heights[b.min(gh - 1) * gw + a.min(gw - 1)];
            let top = h(x0, z0) * (1.0 - fx) + h(x0 + 1, z0) * fx;
            let bot = h(x0, z0 + 1) * (1.0 - fx) + h(x0 + 1, z0 + 1) * fx;
            top * (1.0 - fz) + bot * fz
        };

        // The corners the file states, biggest turn first, spread round the lap.
        let mut corners: Vec<&LineSegment> =
            lap.segments.iter().filter(|s| s.is_corner()).collect();
        corners.sort_by(|a, b| b.angle.total_cmp(&a.angle));
        let mut picked: Vec<&LineSegment> = Vec::new();
        for c in corners {
            if picked.iter().all(|p| (p.at - c.at).abs() > lap.length * 0.12) {
                picked.push(c);
            }
            if picked.len() == 4 {
                break;
            }
        }

        let (mut xlo, mut xhi, mut zlo, mut zhi) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for sgm in &lap.segments {
            xlo = xlo.min(sgm.x); xhi = xhi.max(sgm.x);
            zlo = zlo.min(sgm.z); zhi = zhi.max(sgm.z);
        }
        println!(
            "  centreline x[{xlo:.0}, {xhi:.0}] z[{zlo:.0}, {zhi:.0}]   terrain half-extent {half:.0} m  ({} x {} @ {mpp:.3})",
            gw, gh
        );
        println!("\n== {label}: {} corners on a {:.0} m lap ==", 
            lap.segments.iter().filter(|s| s.is_corner()).count(), lap.length);
        println!(
            "{:<4} {:>7} {:>7} {:>8} {:>9} {:>8} {:>8} {:>8}",
            "#", "radius", "angle", "arc_len", "worked_w", "berm", "camber", "rough"
        );
        for (i, c) in picked.iter().enumerate() {
            let (fx, fz) = crate::trackprog::heading_vector(c.heading);
            let (rx, rz) = crate::trackprog::right_vector(c.heading);
            // Across the corner at its apex: sample out to 25 m each side.
            // Half way round the arc, not half way along its chord: a 108 degree turn puts
            // its apex nowhere near the start heading, and stepping straight lands in the
            // field beside the track.
            let half_ang = (c.angle * 0.5).to_radians() * c.radius.signum();
            let mid_heading = c.heading + half_ang;
            let (ax, az) = {
                let r = c.radius.abs();
                // Centre of the arc, then the point half way round it.
                let (cx, cz) = (c.x + rx * r * c.radius.signum(), c.z + rz * r * c.radius.signum());
                let (bx, bz) = (c.x - cx, c.z - cz);
                let (s, co) = (half_ang.sin(), half_ang.cos());
                (cx + bx * co - bz * s, cz + bx * s + bz * co)
            };
            let (px, pz) = (ax, az);
            let (fx, fz) = crate::trackprog::heading_vector(mid_heading);
            let (rx, rz) = crate::trackprog::right_vector(mid_heading);
            let _ = (fx, fz);
            const REACH: f32 = 25.0;
            const N: usize = 201;
            let profile: Vec<(f32, f32)> = (0..N)
                .map(|k| {
                    let d = (k as f32 / (N - 1) as f32 - 0.5) * 2.0 * REACH;
                    (d, at(px + rx * d, pz + rz * d))
                })
                .collect();
            let mid = profile[N / 2].1;
            // Worked ground: how far out the surface stays within a hand's breadth of the
            // apex's own plane before the field takes over.
            let flat = |sign: f32| -> f32 {
                let mut last = 0.0;
                for (d, h) in &profile {
                    if d.signum() != sign || d.abs() < 0.2 {
                        continue;
                    }
                    if (h - mid).abs() < 1.2 {
                        last = d.abs();
                    } else if d.abs() > last + 3.0 {
                        break;
                    }
                }
                last
            };
            let worked = flat(-1.0) + flat(1.0);
            // The berm: the most the ground stands above the apex on the outside of the turn.
            let outside = if c.radius > 0.0 { -1.0 } else { 1.0 };
            let berm = profile
                .iter()
                .filter(|(d, _)| d.signum() == outside && d.abs() <= 12.0)
                .map(|(_, h)| h - mid)
                .fold(f32::MIN, f32::max);
            // Camber across the worked width, degrees.
            let e = worked.max(2.0) * 0.5;
            let camber = ((at(px + rx * e, pz + rz * e) - at(px - rx * e, pz - rz * e))
                / (2.0 * e))
                .atan()
                .to_degrees();
            // Roughness along the corner: chatter left after the corner's own fall is out.
            let along: Vec<f32> = (0..=((c.length / 0.5) as usize).max(2))
                .map(|k| {
                    let d = k as f32 * 0.5;
                    let ang = (d / c.radius.abs()).min(c.angle.to_radians());
                    let hh = c.heading + ang * c.radius.signum();
                    let (hx, hz) = crate::trackprog::heading_vector(hh);
                    at(c.x + hx * d, c.z + hz * d)
                })
                .collect();
            let rough = detrended_rms(&along);
            println!(
                "{:<4} {:>6.1}m {:>6.0}° {:>7.1}m {:>8.1}m {:>7.2}m {:>7.1}° {:>7.3}m",
                i + 1,
                c.radius.abs(),
                c.angle,
                c.length,
                worked,
                berm.max(0.0),
                camber,
                rough
            );

            // The picture: relief round the corner, with the centreline on it.
            const PIX: usize = 300;
            const SPAN: f32 = 70.0;
            let mut img = vec![0u8; PIX * PIX * 3];
            for py in 0..PIX {
                for pxi in 0..PIX {
                    let wx = px + (pxi as f32 / PIX as f32 - 0.5) * SPAN;
                    let wz = pz + (py as f32 / PIX as f32 - 0.5) * SPAN;
                    let s = SPAN / PIX as f32;
                    let g = ((at(wx + s, wz) - at(wx - s, wz)).powi(2)
                        + (at(wx, wz + s) - at(wx, wz - s)).powi(2))
                    .sqrt();
                    let v = (255.0 * (1.0 - (-g * 9.0).exp())).clamp(0.0, 255.0) as u8;
                    let o = (py * PIX + pxi) * 3;
                    img[o] = v;
                    img[o + 1] = v;
                    img[o + 2] = v;
                }
            }
            for k in 0..=((c.length / 0.25) as usize) {
                let d = k as f32 * 0.25;
                let ang = (d / c.radius.abs()).min(c.angle.to_radians());
                let hh = c.heading + ang * c.radius.signum();
                let (hx, hz) = crate::trackprog::heading_vector(hh);
                let (wx, wz) = (c.x + hx * d, c.z + hz * d);
                let pxi = ((wx - px) / SPAN + 0.5) * PIX as f32;
                let py = ((wz - pz) / SPAN + 0.5) * PIX as f32;
                if (0.0..PIX as f32).contains(&pxi) && (0.0..PIX as f32).contains(&py) {
                    let o = (py as usize * PIX + pxi as usize) * 3;
                    img[o] = 255;
                    img[o + 1] = 40;
                    img[o + 2] = 40;
                }
            }
            image::RgbImage::from_raw(PIX as u32, PIX as u32, img)
                .unwrap()
                .save(format!("{dir}/{label}_corner{}.png", i + 1))
                .unwrap();
        }
    }

    /// Which way each axis of the centreline runs against the grid.
    ///
    /// Scored by relief: a lap laid on the track crosses ruts and berms, and the same lap
    /// laid in a field crosses almost nothing, so the spread of heights under it separates
    /// the four readings cleanly.
    fn pick_axes(
        m: &crate::track::Master,
        lap: &Lap,
        mpp: f32,
        gw: usize,
        gh: usize,
    ) -> (bool, bool) {
        let mut best = (true, true);
        let mut best_score = -1.0f64;
        for fx in [true, false] {
            for fz in [true, false] {
                let mut vals = Vec::new();
                for st in lap.stations(4.0) {
                    let sx = if fx { st.x } else { gw as f32 * mpp - st.x };
                    let sz = if fz { st.z } else { gh as f32 * mpp - st.z };
                    let gx = (sx / mpp).clamp(0.0, gw as f32 - 1.0) as usize;
                    let gz = (sz / mpp).clamp(0.0, gh as f32 - 1.0) as usize;
                    vals.push(m.heights[gz * gw + gx] as f64);
                }
                if vals.len() < 8 {
                    continue;
                }
                // Local spread along the lap: ruts and berms, not the site's overall fall.
                let mut d = 0.0;
                for w in vals.windows(2) {
                    d += (w[1] - w[0]).abs();
                }
                let score = d / vals.len() as f64;
                if score > best_score {
                    best_score = score;
                    best = (fx, fz);
                }
            }
        }
        best
    }

    /// RMS left after a straight line through the run is removed — the chatter, not the fall.
    fn detrended_rms(v: &[f32]) -> f32 {
        let n = v.len();
        if n < 3 {
            return 0.0;
        }
        let (a, b) = (v[0], v[n - 1]);
        let mut s = 0.0f64;
        for (i, h) in v.iter().enumerate() {
            let want = a + (b - a) * i as f32 / (n - 1) as f32;
            s += ((h - want) as f64).powi(2);
        }
        (s / n as f64).sqrt() as f32
    }
}
