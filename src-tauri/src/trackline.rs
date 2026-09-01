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
                    Segment::Arc {
                        radius: s.radius,
                        angle: s.angle * s.radius.signum(),
                        rise: 0.0,
                    }
                }
            })
            .collect()
    }

    /// The lap's corners, by the one definition of what a corner is.
    pub fn turns(&self) -> Vec<(f32, f32)> {
        crate::trackprog::turns(&self.program_segments())
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
        // A left corner keeps its sign through the conversion.
        assert!(matches!(segs[1], Segment::Arc { radius, angle, .. }
            if radius < 0.0 && angle < 0.0));
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
