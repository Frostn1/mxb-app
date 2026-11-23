//! A height map of the ground under the laps, built from the laps themselves.
//!
//! Every grounded telemetry sample is a point on the track surface (plus the bike's own
//! height), so a session's laps trace the ground they rode and its relief. It needs no track
//! files, so it works for a locked track too. Cells nobody rode near stay empty.

use serde::Serialize;

use crate::telemetry::Sample;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    /// World x/z of the grid's corner.
    pub x0: f32,
    pub z0: f32,
    /// Metres per cell.
    pub cell: f32,
    pub width: usize,
    pub height: usize,
    /// Row-major, row along z; None where nobody rode.
    pub heights: Vec<Option<f32>>,
}

/// Empty cells this close to a ridden one are filled from it: laps a few metres apart still
/// leave gaps between them, and the track is wider than any one line.
const SPREAD: i32 = 3;

/// `max_side` caps the cells along the longer edge; a big venue gets bigger cells.
pub fn build(samples: &[Sample], max_side: usize) -> Option<Surface> {
    let ground: Vec<&Sample> = samples.iter().filter(|s| !s.airborne() && !s.crashed).collect();
    if ground.len() < 50 || max_side < 8 {
        return None;
    }
    let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for s in &ground {
        (x0, x1, z0, z1) = (x0.min(s.x), x1.max(s.x), z0.min(s.z), z1.max(s.z));
    }
    // Room for the padding both sides, the rounding up and the corner cell.
    let cell = ((x1 - x0).max(z1 - z0) / (max_side - 2 * SPREAD as usize - 2) as f32).max(1.0);
    let pad = SPREAD as f32 * cell;
    let (x0, z0) = (x0 - pad, z0 - pad);
    let width = ((x1 + pad - x0) / cell).ceil() as usize + 1;
    let height = ((z1 + pad - z0) / cell).ceil() as usize + 1;

    let (mut sum, mut n) = (vec![0.0f64; width * height], vec![0u32; width * height]);
    for s in &ground {
        let (c, r) = (((s.x - x0) / cell) as usize, ((s.z - z0) / cell) as usize);
        let k = r.min(height - 1) * width + c.min(width - 1);
        sum[k] += s.y as f64;
        n[k] += 1;
    }
    let raw: Vec<Option<f32>> = (0..width * height).map(|k| (n[k] > 0).then(|| (sum[k] / n[k] as f64) as f32)).collect();

    let mut heights = raw.clone();
    for r in 0..height {
        for c in 0..width {
            let k = r * width + c;
            if raw[k].is_some() {
                continue;
            }
            let (mut acc, mut weight) = (0.0f32, 0.0f32);
            for dr in -SPREAD..=SPREAD {
                for dc in -SPREAD..=SPREAD {
                    let d2 = (dr * dr + dc * dc) as f32;
                    let (rr, cc) = (r as i32 + dr, c as i32 + dc);
                    if d2 > (SPREAD * SPREAD) as f32 || rr < 0 || cc < 0 || rr >= height as i32 || cc >= width as i32 {
                        continue;
                    }
                    if let Some(h) = raw[rr as usize * width + cc as usize] {
                        acc += h / d2;
                        weight += 1.0 / d2;
                    }
                }
            }
            if weight > 0.0 {
                heights[k] = Some(acc / weight);
            }
        }
    }
    Some(Surface { x0, z0, cell, width, height, heights })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ride(points: impl Iterator<Item = (f32, f32, f32)>) -> Vec<Sample> {
        points
            .map(|(x, y, z)| Sample { x, y, z, wheel_material: [1, 1], ..Default::default() })
            .collect()
    }

    #[test]
    fn a_ridden_ramp_rises_where_it_was_ridden_and_nowhere_else() {
        // 100 m north up a 5% slope, then 100 m east on the flat.
        let north = (0..=200).map(|k| (0.0, k as f32 * 0.025, k as f32 * 0.5));
        let east = (0..=200).map(|k| (k as f32 * 0.5, 5.0, 100.0));
        let s = build(&ride(north.chain(east)), 400).unwrap();
        assert_eq!(s.cell, 1.0, "a small track gets metre cells");
        let at = |x: f32, z: f32| s.heights[((z - s.z0) / s.cell) as usize * s.width + ((x - s.x0) / s.cell) as usize];
        assert!((at(0.2, 40.2).unwrap() - 2.0).abs() < 0.1);
        assert!(at(1.5, 40.2).is_some(), "the gap beside a line is filled");
        assert!(at(60.0, 40.0).is_none(), "open ground nobody rode stays empty");
        assert!((at(80.2, 100.2).unwrap() - 5.0).abs() < 0.1);
    }

    #[test]
    fn airborne_and_crashed_samples_are_not_ground() {
        let mut samples = ride((0..100).map(|k| (k as f32, 0.0, 0.0)));
        samples[50].wheel_material = [0, 0];
        samples[50].y = 9.0;
        samples[51].crashed = true;
        samples[51].y = 9.0;
        let s = build(&samples, 400).unwrap();
        assert!(s.heights.iter().flatten().all(|h| *h < 1.0));
    }

    #[test]
    fn a_big_venue_gets_bigger_cells() {
        let s = build(&ride((0..=2000).map(|k| (k as f32, 0.0, 0.0))), 400).unwrap();
        assert!(s.cell > 4.0 && s.width <= 400, "cell {} width {}", s.cell, s.width);
    }
}
