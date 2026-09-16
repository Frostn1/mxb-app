//! Does a track's terrain line up with the laps ridden on it?
//!
//! The terrain grid and the plugin's positions share the game's world frame, grid corner at
//! the origin, `col = x / m`, `row = z / m`. That has been checked against the track's own
//! placements but never against telemetry, and the wrong track (a layout, a renamed folder)
//! would draw confidently wrong ground. So the laps check it: on the right terrain, the bike
//! rides the same height above the ground everywhere.

/// Grounded samples that have to land on the grid for a verdict.
const MIN_POINTS: usize = 100;
/// The gap between bike and ground may wander this much (median absolute deviation, metres).
///
/// This is a "is it the right terrain" test, not a "does it match to the centimetre" one, and
/// half a metre was too strict to be the first. The `.trh` is the track as built, so none of
/// the ruts cut into it over a session are there; `load_master` downsamples to 2048 across,
/// which rounds off jump faces; and the bike's own suspension moves under it. A rutted track
/// on a big venue can pass every one of those and still wander past half a metre, and then
/// the rider got a blurred grid of their own telemetry instead of their circuit.
///
/// The wrong terrain isn't marginal — a different layout or a renamed folder puts the bike
/// metres out and usually off the grid entirely — so this can be loosened without the check
/// losing its point.
const MAX_SPREAD_M: f32 = 1.2;
/// And sit no further than this from the ground at all.
const MAX_LIFT_M: f32 = 3.0;

/// Bilinear height at world `x`, `z`, or None off the grid.
pub fn height_at(width: usize, height: usize, mps: f32, heights: &[f32], x: f32, z: f32) -> Option<f32> {
    if mps <= 0.0 || width < 2 || height < 2 {
        return None;
    }
    let (gx, gz) = (x / mps, z / mps);
    if gx < 0.0 || gz < 0.0 || gx > (width - 1) as f32 || gz > (height - 1) as f32 {
        return None;
    }
    let (c0, r0) = (gx.floor() as usize, gz.floor() as usize);
    let (c1, r1) = ((c0 + 1).min(width - 1), (r0 + 1).min(height - 1));
    let (fx, fz) = (gx - c0 as f32, gz - r0 as f32);
    let h = |c: usize, r: usize| heights[r * width + c];
    let top = h(c0, r0) + (h(c1, r0) - h(c0, r0)) * fx;
    let bottom = h(c0, r1) + (h(c1, r1) - h(c0, r1)) * fx;
    Some(top + (bottom - top) * fz)
}

fn median(v: &mut [f32]) -> f32 {
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// How high the bike rides above this terrain, when the laps say it's the right terrain.
/// `points` are grounded world positions (x, y, z).
pub fn fit(width: usize, height: usize, mps: f32, heights: &[f32], points: &[[f32; 3]]) -> Option<f32> {
    measure(width, height, mps, heights, points).lift
}

/// What the check saw, so a refusal can say why rather than leaving the rider to guess at a
/// grid of their own laps.
#[derive(Debug, Default)]
pub struct Fit {
    /// How high the bike rides above this terrain, when the laps say it is the right terrain.
    pub lift: Option<f32>,
    /// How many grounded samples landed on the grid, out of how many were offered.
    pub on_grid: usize,
    pub offered: usize,
    /// The median gap, and how much it wandered. Both zero when too little landed to say.
    pub median_lift: f32,
    pub spread: f32,
}

pub fn measure(width: usize, height: usize, mps: f32, heights: &[f32], points: &[[f32; 3]]) -> Fit {
    let mut gaps: Vec<f32> = points
        .iter()
        .filter_map(|&[x, y, z]| Some(y - height_at(width, height, mps, heights, x, z)?))
        .collect();
    let mut out = Fit { on_grid: gaps.len(), offered: points.len(), ..Fit::default() };
    if gaps.len() < MIN_POINTS || gaps.len() * 2 < points.len() {
        return out;
    }
    let lift = median(&mut gaps);
    let mut spread: Vec<f32> = gaps.iter().map(|g| (g - lift).abs()).collect();
    out.median_lift = lift;
    out.spread = median(&mut spread);
    out.lift = (out.spread <= MAX_SPREAD_M && lift.abs() <= MAX_LIFT_M).then_some(lift);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 200 m square of rolling ground at 2 m a sample.
    fn hills() -> (usize, usize, f32, Vec<f32>) {
        let (w, h, m) = (101, 101, 2.0);
        let heights = (0..w * h)
            .map(|k| {
                let (c, r) = ((k % w) as f32 * m, (k / w) as f32 * m);
                10.0 + 3.0 * (c / 23.0).sin() + 2.0 * (r / 17.0).cos()
            })
            .collect();
        (w, h, m, heights)
    }

    fn ride(heights: &(usize, usize, f32, Vec<f32>), lift: f32, flip: bool) -> Vec<[f32; 3]> {
        let (w, h, m, hs) = heights;
        (0..400)
            .map(|k| {
                let t = k as f32 / 400.0 * std::f32::consts::TAU;
                let (x, z) = (100.0 + 60.0 * t.cos(), 100.0 + 40.0 * t.sin());
                // A mirrored track reads its heights from the wrong side of the map.
                let (sx, sz) = if flip { (200.0 - x, z) } else { (x, z) };
                [x, height_at(*w, *h, *m, hs, sx, sz).unwrap() + lift, z]
            })
            .collect()
    }

    #[test]
    fn laps_on_the_right_terrain_ride_a_steady_height_above_it() {
        let t = hills();
        let lift = fit(t.0, t.1, t.2, &t.3, &ride(&t, 0.6, false)).unwrap();
        assert!((lift - 0.6).abs() < 0.01, "{lift}");
    }

    #[test]
    fn a_mirrored_or_wrong_terrain_is_refused() {
        let t = hills();
        assert!(fit(t.0, t.1, t.2, &t.3, &ride(&t, 0.6, true)).is_none());
    }

    #[test]
    fn laps_off_the_grid_are_refused() {
        let t = hills();
        let far: Vec<[f32; 3]> = ride(&t, 0.6, false).iter().map(|&[x, y, z]| [x + 500.0, y, z]).collect();
        assert!(fit(t.0, t.1, t.2, &t.3, &far).is_none());
    }

    #[test]
    fn heights_are_bilinear_between_samples() {
        let hs = vec![0.0, 2.0, 4.0, 6.0];
        assert_eq!(height_at(2, 2, 1.0, &hs, 0.5, 0.5), Some(3.0));
        assert_eq!(height_at(2, 2, 1.0, &hs, 1.5, 0.0), None);
    }

    /// The track as built has none of the ruts cut into it over a session, and the master grid
    /// rounds off jump faces — so the bike's height above it wanders on exactly the tracks a
    /// rider cares about. That has to still count as the same track: refusing it dropped the
    /// 3D view back to a blurred grid of the rider's own telemetry.
    #[test]
    fn a_rutted_track_is_still_the_same_track() {
        let t = hills();
        let rutted: Vec<[f32; 3]> = ride(&t, 0.6, false)
            .iter()
            .enumerate()
            // Down into a rut, up over a rounded-off jump face: within a metre either way.
            .map(|(k, &[x, y, z])| [x, y + if k % 3 == 0 { -0.8 } else { 0.7 }, z])
            .collect();
        let got = fit(t.0, t.1, t.2, &t.3, &rutted);
        assert!(got.is_some(), "a rutted lap on its own terrain must still be recognised");

        // And the measurement is reported either way, so a refusal can say what it saw.
        let m = measure(t.0, t.1, t.2, &t.3, &ride(&t, 0.6, true));
        assert!(m.lift.is_none(), "still refuses the wrong terrain");
        assert!(m.spread > MAX_SPREAD_M, "and says how far out it was: {}", m.spread);
        assert_eq!(m.offered, 400);
    }
}
