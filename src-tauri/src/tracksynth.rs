//! Turning a track program into the files TerrainEd compiles.
//!
//! MX Bikes' track build is a command line, not a GUI:
//!
//! ```text
//! terrained.exe track.hmf mytrack/mytrack.map params.ini      graphics
//! terrained.exe track.tht mytrack/mytrack.trh trh_params.ini  collision
//! tracked -merge mytrack/mytrack.trh cl track.tcl sa start.tcl
//! ```
//!
//! Every input except the heightmap raster is text. So what this module has to produce is one
//! binary — a 16-bit raw — plus a handful of files that describe it, and the whole of "make me
//! a track" reduces to writing a folder and running two commands in it.
//!
//! The height mapping is not guessed. The official example track ships both its
//! `heightmap.raw` and the `example.trh` TerrainEd made from it, and reading one against the
//! other settles it: samples are **little-endian u16**, 96% of them survive the compile
//! byte-identical, and the `.trh`'s trailing block carries back exactly the `size_x`, `scale`
//! and `size_z` the `.hmf` declared. A raw value maps to metres as `v * scale / 65535`.
//!
//! The terrain itself is built in three layers, which is also how a track is actually made:
//!
//! 1. a landscape, from noise, that knows nothing about the track;
//! 2. the riding line **benched** into it — the corridor takes a smoothed version of the
//!    ground it crosses, so the track follows the land without inheriting its every bump;
//! 3. the features, added on top of the bench and faded out at the edge of the corridor so a
//!    jump never spills into the field beside it.

#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::trackprog::{Feature, Segment, Station, Surface, TrackProgram};

/// Metres of centreline between stations. Finer than the grid, so every cell finds a station
/// nearer than its own width.
const STATION_STEP: f32 = 0.5;

/// How far past the riding line the terrain is still pulled towards it, metres. This is the
/// shoulder — the graded ground either side that a track sits in rather than on.
const SHOULDER_M: f32 = 9.0;

/// Metres of lap the track's own elevation is smoothed over. Short enough to follow a hill,
/// long enough not to follow a bush.
const BENCH_SMOOTH_M: f32 = 45.0;

/// Where a feature stops being full height, as a fraction of the half-width, and where it has
/// faded out entirely.
///
/// The fade runs past the edge of the riding line on purpose. Ending it at the edge makes the
/// side of every jump a wall — a 2.4 m tabletop falling to nothing across two metres of track
/// is a 51° face, steeper than anything measured on a published track, and it lands inside
/// the corridor where it is exactly what a rider hits. Real jumps spill onto the shoulder,
/// and letting these do the same puts the slope back where the corpus has it.
const FEATURE_FULL: f32 = 0.8;
const FEATURE_EDGE: f32 = 1.75;

/// How much shorter a cut face is than a fill slope.
///
/// A track is bladed into the ground, not draped over it. Where the machine digs into rising
/// ground it leaves a short steep face; where it pushes the spoil out onto falling ground it
/// leaves a long shallow one. Grading both sides the same distance is the single clearest
/// tell that nobody built this — real benching is never symmetrical.
const CUT_SHOULDER: f32 = 0.5;
const FILL_SHOULDER: f32 = 1.7;

/// The ridge left between one pass of the machine and the next: how far apart they are, and
/// how proud the seam stands.
///
/// A blade's own fine grooves are a few centimetres apart and cannot be represented here at
/// all — at a third of a metre a sample they are far below what the grid can hold, and asking
/// for them produces aliasing noise rather than grooves. The *passes* are the feature at this
/// scale, they are a real thing you can see on a built track, and they run along the
/// direction of travel — so they vary across the track and not along it.
const PASS_SPACING_M: f32 = 2.6;
const PASS_DEPTH_M: f32 = 0.022;

/// Metres between the bumps of the riding surface's own texture.
const TEXTURE_WAVELENGTH_M: f32 = 3.5;

/// How much the riding line's width wanders, as a fraction. A track of exactly constant
/// width reads as machine-made from the first glance — real ones pinch into corners and open
/// out on the straights.
const WIDTH_WANDER: f32 = 0.14;

/// Metres of lap between one pinch and the next.
const WIDTH_WAVELENGTH_M: f32 = 70.0;

/// Corner radius at which ruts start to form, and the one where they are at full depth.
/// Everyone takes the same line through a tight corner, and that is what digs a rut.
const RUT_RADIUS_M: (f32, f32) = (45.0, 14.0);

/// How deep a corner's own rut gets, metres.
const RUT_DEPTH_M: f32 = 0.16;

/// A rut's width across the track, metres — about a tyre and the berm of dirt each side.
const RUT_WIDTH_M: f32 = 0.55;

/// Metres before a corner that riders brake in, and so where the ground gets chopped up.
const BRAKING_M: f32 = 18.0;

/// Metres between braking bumps.
const BRAKING_WAVELENGTH_M: f32 = 2.2;

/// How much rougher the surface gets in and around a corner, as a multiplier on the texture.
const CORNER_ROUGHNESS: f32 = 1.8;

/// How tall a braking bump is against the surface texture.
const BRAKING_GAIN: f32 = 2.5;

/// The short back face of a double's takeoff, and the short front face of its landing. This
/// is the lip itself — steep, but a face rather than a step.
const DOUBLE_BACK_M: f32 = 2.5;

/// Metres between samples of the profiles that run along the lap.
///
/// Everything built on the track is a function of how far round it you are, and the cells
/// look that function up rather than reading the nearest station's copy of it. Taking the
/// nearest station's value directly puts the chamfer's own jagged label boundaries into the
/// terrain: two neighbouring cells can be assigned stations several metres apart, and on the
/// face of a jump that is a step in the ground you can see across the whole straight.
const PROFILE_STEP: f32 = 0.1;

/// Masks are stretched over the terrain, so they cost nothing to keep coarser than it.
const MASK_DIM: usize = 1024;

/// The ground textures' edge, in pixels. A power of two, as MX Bikes requires.
const GROUND_TEXTURE_DIM: usize = 512;

/// The UI pictures' edge, in pixels. Square and modest — they are shown at a few hundred
/// pixels and stored uncompressed.
const UI_IMAGE_DIM: usize = 512;

/// Coverage masks inside a `.trh`, which published tracks keep at half the grid — 2048
/// against 2049. It is also the resolution anything reading the file will measure it at.
const TRH_MASK_DIM: usize = 2048;

/// How far below the top of the height budget the terrain is allowed to sit. Quantisation is
/// against the budget, so leaving room costs resolution for nothing — but landing exactly on
/// 0 or 65535 risks a clamp at the ends.
const BUDGET_MARGIN: f32 = 0.02;

/// A synthesised terrain, and everything about the track that shaped it.
pub struct Synth {
    pub gw: usize,
    pub gh: usize,
    /// Metres per sample. Held as one figure, so the grid's cells are square.
    pub mps: f32,
    /// Metres, already inside the program's height budget.
    pub heights: Vec<f32>,
    /// The riding line.
    pub corridor: Vec<bool>,
    /// Metres from the centreline.
    pub dist: Vec<f32>,
    /// Metres round the lap.
    pub arc: Vec<f32>,
    /// Which station is nearest — the index that carries arc length and heading.
    pub station: Vec<u32>,
    pub stations: Vec<Station>,
    /// What the terrain actually used of its budget, and what the budget was.
    pub used_m: f32,
    pub budget_m: f32,
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

pub fn synthesise(prog: &TrackProgram) -> Result<Synth> {
    prog.check()?;

    let (gw, gh) = grid_dims(prog)?;
    let mps_x = prog.terrain.size_x / (gw - 1) as f32;
    let mps_z = prog.terrain.size_z / (gh - 1) as f32;
    // Everything downstream measures distance in one unit. Cells that aren't square would
    // make a berm wider one way than the other.
    if (mps_x - mps_z).abs() > 0.05 * mps_x.max(mps_z) {
        bail!(
            "terrain cells aren't square: {mps_x:.3} m across against {mps_z:.3} m down. Pick \
             sample counts that match the ground's shape."
        );
    }
    let mps = 0.5 * (mps_x + mps_z);

    let stations = prog.stations(STATION_STEP);
    if stations.is_empty() {
        bail!("the centreline has no length");
    }

    // 1. The landscape, which knows nothing about the track.
    let r = &prog.terrain.relief;
    let mut heights = vec![0.0f32; gw * gh];
    for y in 0..gh {
        for x in 0..gw {
            let (wx, wz) = (x as f32 * mps_x, y as f32 * mps_z);
            heights[y * gw + x] = fbm(wx / r.wavelength, wz / r.wavelength, r.seed) * r.amplitude;
        }
    }

    // 2. Which station each cell belongs to, and how far off the line it is.
    let (mut dist, station) = nearest_station(&stations, gw, gh, mps_x, mps_z);

    // The track's own elevation: the landscape under the centreline, smoothed so the riding
    // line rides the hills instead of every hummock, plus whatever the step-ups asked for.
    let lap = prog.lap_length();
    let mut along: Vec<f32> = stations
        .iter()
        .map(|st| sample(&heights, gw, gh, st.x / mps_x, st.z / mps_z))
        .collect();
    smooth_along(&mut along, (BENCH_SMOOTH_M / STATION_STEP) as usize);
    apply_step_ups(&mut along, &stations, &prog.features);

    // Everything that varies along the lap, resampled onto one even ruler so a cell can ask
    // for the value at *its* distance round rather than at the nearest station's.
    let bench = resample(&stations, &along, lap);
    let turn = resample(
        &stations,
        &stations.iter().map(|s| s.curvature).collect::<Vec<_>>(),
        lap,
    );
    let feat = feature_profile(&prog.features, lap);
    let berms = berm_profile(&prog.features, &turn, lap);
    let ruts = rut_profile(&prog.features, &turn, lap, r.seed);
    let widths = width_profile(prog.width * 0.5, lap, r.seed);
    let (rough, braking) = roughness_profile(&turn, lap);

    // 3. Bench the corridor in, then build on it.
    let mut corridor = vec![false; gw * gh];
    let mut arc = vec![0.0f32; gw * gh];
    for i in 0..gw * gh {
        let (d, s, t) = local_frame(
            &stations,
            station[i] as usize,
            (i % gw) as f32 * mps_x,
            (i / gw) as f32 * mps_z,
        );

        dist[i] = d;
        arc[i] = s;
        let half = widths.at(s);

        // Cut or fill: which one decides how far the grading reaches, and so how the edge
        // of the track reads from the seat.
        let ground = heights[i];
        let deck = bench.at(s);
        let shoulder = SHOULDER_M * if ground > deck { CUT_SHOULDER } else { FILL_SHOULDER };
        let w = bench_weight(d, half, shoulder);
        if w > 0.0 {
            heights[i] = ground * (1.0 - w) + deck * w;
        }
        if d <= half {
            corridor[i] = true;
        }
        let f = feat.at(s);
        if f != 0.0 {
            heights[i] += f * lateral(d, half);
        }
        // A berm stands on the outside of the corner, which is the side away from the turn.
        let b = berms.at(s);
        if b != 0.0 && t * -b.signum() > 0.0 && t.abs() <= half {
            heights[i] += b.abs() * (t.abs() / half).powi(2);
        }

        // Ruts: a groove worn into the line, offset towards the inside of the corner because
        // that is the line everybody takes.
        let (depth, side) = (ruts.at(s), turn.at(s));
        if depth > 0.0 {
            let centre = side.signum() * half * 0.35;
            let g = (t - centre) / RUT_WIDTH_M;
            heights[i] -= depth * (-g * g).exp();
        }

        // Ridden ground, last of all: strongest on the line and gone by the shoulder, and
        // rougher through the corners than down the straights.
        if r.texture > 0.0 && w > 0.0 {
            let (wx, wz) = ((i % gw) as f32 * mps_x, (i / gw) as f32 * mps_z);
            let gain = rough.at(s);
            heights[i] += fbm(
                wx / TEXTURE_WAVELENGTH_M,
                wz / TEXTURE_WAVELENGTH_M,
                r.seed ^ 0x5EED,
            ) * r.texture
                * gain
                * w;
            // The seams between the machine's passes, running the way it drove.
            heights[i] -= ((t / PASS_SPACING_M) * std::f32::consts::TAU).sin().abs()
                * PASS_DEPTH_M
                * w;

            // Braking bumps: a washboard across the track on the way into a corner, which is
            // the direction they actually form in.
            let brake = braking.at(s);
            if brake > 0.0 {
                let ripple = (s / BRAKING_WAVELENGTH_M * std::f32::consts::TAU).sin();
                heights[i] += ripple * r.texture * BRAKING_GAIN * brake * w;
            }
        }
    }

    // Everything has to fit the budget, because that is what the samples are quantised
    // against — and a track that overflows it is silently clipped flat at the top.
    let (lo, hi) = heights.iter().fold((f32::MAX, f32::MIN), |(a, b), v| {
        (a.min(*v), b.max(*v))
    });
    let used = hi - lo;
    let budget = prog.terrain.scale;
    if used > budget * (1.0 - BUDGET_MARGIN) {
        bail!(
            "the terrain needs {used:.1} m of height and the budget is {budget:.1} m. Raise \
             terrain.scale to about {:.0}.",
            (used * 1.15).ceil()
        );
    }
    let floor = budget * BUDGET_MARGIN;
    for v in &mut heights {
        *v = *v - lo + floor;
    }

    Ok(Synth {
        gw,
        gh,
        mps,
        heights,
        corridor,
        dist,
        arc,
        station,
        stations,
        used_m: used,
        budget_m: budget,
    })
}

/// Samples across and down, both a power of two plus one, with cells as square as that allows.
fn grid_dims(prog: &TrackProgram) -> Result<(usize, usize)> {
    let n = prog.terrain.samples as usize;
    let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
    let short = |long_n: usize, ratio: f32| -> usize {
        let want = (long_n - 1) as f32 * ratio;
        let mut p = 128usize;
        while p * 2 < want as usize {
            p *= 2;
        }
        // Whichever power of two is nearer the wanted count.
        if (p * 2) as f32 - want < want - p as f32 {
            p *= 2;
        }
        p.max(128) + 1
    };
    Ok(if sx >= sz {
        (n, short(n, sz / sx))
    } else {
        (short(n, sx / sz), n)
    })
}

/// For every cell, the nearest station and the distance to it.
///
/// A chamfer pass carries the station index outwards from the cells the centreline lands in,
/// which costs two sweeps instead of a search per cell. The distance it propagates is only
/// approximate, so it is thrown away at the end and recomputed exactly against the station it
/// found — the label is what the sweep is for.
fn nearest_station(
    st: &[Station],
    gw: usize,
    gh: usize,
    mps_x: f32,
    mps_z: f32,
) -> (Vec<f32>, Vec<u32>) {
    const BIG: i32 = 1 << 24;
    let mut d = vec![BIG; gw * gh];
    let mut label = vec![0u32; gw * gh];

    for (i, s) in st.iter().enumerate() {
        let x = (s.x / mps_x).round();
        let y = (s.z / mps_z).round();
        if x < 0.0 || y < 0.0 || x >= gw as f32 || y >= gh as f32 {
            continue;
        }
        let at = y as usize * gw + x as usize;
        if d[at] != 0 {
            d[at] = 0;
            label[at] = i as u32;
        }
    }

    let relax = |at: usize, from: usize, cost: i32, d: &mut Vec<i32>, l: &mut Vec<u32>| {
        if d[from] + cost < d[at] {
            d[at] = d[from] + cost;
            l[at] = l[from];
        }
    };
    for y in 0..gh {
        for x in 0..gw {
            let at = y * gw + x;
            if y > 0 {
                relax(at, at - gw, 3, &mut d, &mut label);
                if x > 0 {
                    relax(at, at - gw - 1, 4, &mut d, &mut label);
                }
                if x + 1 < gw {
                    relax(at, at - gw + 1, 4, &mut d, &mut label);
                }
            }
            if x > 0 {
                relax(at, at - 1, 3, &mut d, &mut label);
            }
        }
    }
    for y in (0..gh).rev() {
        for x in (0..gw).rev() {
            let at = y * gw + x;
            if y + 1 < gh {
                relax(at, at + gw, 3, &mut d, &mut label);
                if x > 0 {
                    relax(at, at + gw - 1, 4, &mut d, &mut label);
                }
                if x + 1 < gw {
                    relax(at, at + gw + 1, 4, &mut d, &mut label);
                }
            }
            if x + 1 < gw {
                relax(at, at + 1, 3, &mut d, &mut label);
            }
        }
    }

    let mut out = vec![0.0f32; gw * gh];
    for y in 0..gh {
        for x in 0..gw {
            let at = y * gw + x;
            let s = &st[label[at] as usize];
            out[at] = ((x as f32 * mps_x - s.x).powi(2) + (y as f32 * mps_z - s.z).powi(2)).sqrt();
        }
    }
    (out, label)
}

/// Where a cell sits relative to the riding line: how far off it, how far round it, and which
/// side — against the centreline **segments** either side of the labelled station, not the
/// station point itself.
///
/// The label comes from a chamfer sweep, so its boundaries are jagged: two neighbouring cells
/// can be handed stations metres apart. Measuring to the station point carries that jaggedness
/// straight into the terrain and leaves a rippled crease down both edges of every straight.
/// Measuring to the segments doesn't, because the two candidate segments overlap whenever the
/// label moves — so the answer is the same either side of the jump. It is also the true
/// distance to the line rather than to a sample of it, which matters through a tight corner
/// where the two differ.
fn local_frame(st: &[Station], k: usize, px: f32, pz: f32) -> (f32, f32, f32) {
    let mut best = (f32::MAX, 0.0f32, 0.0f32);
    let lo = k.saturating_sub(1);
    for i in lo..=(k + 1).min(st.len() - 1) {
        let Some(j) = (i + 1 < st.len()).then_some(i + 1) else {
            continue;
        };
        let (a, b) = (&st[i], &st[j]);
        let (ax, az) = (b.x - a.x, b.z - a.z);
        let len2 = ax * ax + az * az;
        if len2 <= 1e-9 {
            continue;
        }
        let u = (((px - a.x) * ax + (pz - a.z) * az) / len2).clamp(0.0, 1.0);
        let (qx, qz) = (a.x + ax * u, a.z + az * u);
        let d = ((px - qx).powi(2) + (pz - qz).powi(2)).sqrt();
        if d < best.0 {
            let (rx, rz) = crate::trackprog::right_vector(a.heading);
            best = (
                d,
                a.s + (b.s - a.s) * u,
                (px - a.x) * rx + (pz - a.z) * rz,
            );
        }
    }
    if best.0 == f32::MAX {
        let a = &st[k.min(st.len() - 1)];
        let (rx, rz) = crate::trackprog::right_vector(a.heading);
        return (
            ((px - a.x).powi(2) + (pz - a.z).powi(2)).sqrt(),
            a.s,
            (px - a.x) * rx + (pz - a.z) * rz,
        );
    }
    best
}

/// How much of the track's own elevation a cell takes: all of it across the riding line,
/// none of it past the shoulder.
fn bench_weight(d: f32, half: f32, shoulder: f32) -> f32 {
    if d <= half {
        1.0
    } else if d >= half + shoulder {
        0.0
    } else {
        smoothstep(1.0 - (d - half) / shoulder)
    }
}

/// How much of a feature reaches a cell. Full height across most of the track, gone by the
/// edge, so a jump doesn't run off into the field.
fn lateral(d: f32, half: f32) -> f32 {
    let full = half * FEATURE_FULL;
    let edge = half * FEATURE_EDGE;
    if d <= full {
        1.0
    } else if d >= edge {
        0.0
    } else {
        smoothstep(1.0 - (d - full) / (edge - full))
    }
}

/// How wide the riding line is, along the lap.
///
/// Not a constant. A track of exactly one width for its whole length reads as machine-made
/// before you have looked at anything else — real ones pinch into corners and open onto the
/// straights, and a tenth of the width is enough to break the tell.
fn width_profile(half: f32, lap: f32, seed: u32) -> Profile {
    let mut out = Profile::blank(lap);
    for i in 0..out.v.len() {
        let s = i as f32 * PROFILE_STEP;
        out.v[i] = half * (1.0 + WIDTH_WANDER * fbm(s / WIDTH_WAVELENGTH_M, 0.5, seed ^ 0x1D77));
    }
    out
}

/// How deep the rut is, along the lap.
///
/// Corners grow their own: everyone takes the same line through a tight one, and that is
/// what digs the groove. A `Rut` feature adds to whatever the corner already had, so asking
/// for one in a hairpin deepens it rather than replacing it.
fn rut_profile(features: &[Feature], turn: &Profile, lap: f32, seed: u32) -> Profile {
    let mut out = Profile::blank(lap);
    let (start_r, full_r) = RUT_RADIUS_M;
    for i in 0..out.v.len() {
        let s = i as f32 * PROFILE_STEP;
        let k = turn.at(s).abs();
        if k > 0.0 {
            let radius = 1.0 / k;
            if radius < start_r {
                let t = ((start_r - radius) / (start_r - full_r)).clamp(0.0, 1.0);
                // Not evenly: a rut wanders in depth down the length of a corner.
                let vary = 0.75 + 0.25 * fbm(s / 9.0, 3.5, seed ^ 0x2117);
                out.v[i] = RUT_DEPTH_M * smoothstep(t) * vary;
            }
        }
    }
    for f in features {
        let Feature::Rut { at, length, depth } = *f else {
            continue;
        };
        let lo = (at / PROFILE_STEP).floor().max(0.0) as usize;
        let hi = (((at + length) / PROFILE_STEP).ceil() as usize).min(out.v.len() - 1);
        for i in lo..=hi {
            let u = i as f32 * PROFILE_STEP - at;
            if u < 0.0 || u > length {
                continue;
            }
            let ramp = smoothstep((u / length * 3.0).min(3.0 - u / length * 3.0).clamp(0.0, 1.0));
            out.v[i] += depth * ramp;
        }
    }
    out
}

/// How chopped-up the ground is, along the lap — one on a straight, more through a corner
/// and on the way into it. Braking is where a track gets rough, and it is rough in a place
/// rather than everywhere.
fn roughness_profile(turn: &Profile, lap: f32) -> (Profile, Profile) {
    let mut rough = Profile::blank(lap);
    let mut braking = Profile::blank(lap);
    for i in 0..rough.v.len() {
        rough.v[i] = 1.0;
    }
    let n = rough.v.len();
    let back = (BRAKING_M / PROFILE_STEP) as usize;
    for i in 0..n {
        let s = i as f32 * PROFILE_STEP;
        let k = turn.at(s).abs();
        if k <= 0.0 {
            continue;
        }
        let radius = 1.0 / k;
        if radius >= RUT_RADIUS_M.0 {
            continue;
        }
        let corner = smoothstep((RUT_RADIUS_M.0 - radius) / (RUT_RADIUS_M.0 - RUT_RADIUS_M.1));
        // Inside the corner the ground is chopped up, but not in ridges: that is the rut's
        // job, and noise's.
        rough.v[i] = rough.v[i].max(1.0 + (CORNER_ROUGHNESS - 1.0) * corner);
        // Braking bumps only on the way in, growing towards the turn-in point.
        for j in i.saturating_sub(back)..i {
            if turn.at(j as f32 * PROFILE_STEP) != 0.0 {
                continue; // already in a corner — this is another corner's exit, not an approach
            }
            let near = (j as f32 - (i as f32 - back as f32)) / back as f32;
            braking.v[j] = braking.v[j].max(corner * smoothstep(near));
        }
    }
    (rough, braking)
}

/// A quantity that varies along the lap, on an even ruler.
struct Profile {
    v: Vec<f32>,
}

impl Profile {
    fn blank(lap: f32) -> Self {
        Profile {
            v: vec![0.0; (lap / PROFILE_STEP).ceil() as usize + 2],
        }
    }

    fn at(&self, s: f32) -> f32 {
        if self.v.is_empty() {
            return 0.0;
        }
        let x = (s / PROFILE_STEP).clamp(0.0, (self.v.len() - 1) as f32);
        let i = x.floor() as usize;
        let a = self.v[i];
        let b = self.v[(i + 1).min(self.v.len() - 1)];
        a + (b - a) * (x - i as f32)
    }
}

/// A station-indexed quantity, put onto the even ruler. Stations are not evenly spaced —
/// each segment divides its own length — so this can't be a straight copy.
fn resample(st: &[Station], vals: &[f32], lap: f32) -> Profile {
    let mut out = Profile::blank(lap);
    if st.is_empty() {
        return out;
    }
    let mut k = 0usize;
    for i in 0..out.v.len() {
        let s = i as f32 * PROFILE_STEP;
        while k + 1 < st.len() && st[k + 1].s < s {
            k += 1;
        }
        let j = (k + 1).min(st.len() - 1);
        let span = (st[j].s - st[k].s).max(1e-6);
        let f = ((s - st[k].s) / span).clamp(0.0, 1.0);
        out.v[i] = vals[k] + (vals[j] - vals[k]) * f;
    }
    out
}

/// Height added by everything built on the line, along the lap.
fn feature_profile(features: &[Feature], lap: f32) -> Profile {
    let mut out = Profile::blank(lap);
    for f in features {
        if matches!(f, Feature::StepUp { .. } | Feature::Berm { .. }) {
            continue;
        }
        let (at, len) = (f.at(), f.length());
        let lo = (at / PROFILE_STEP).floor().max(0.0) as usize;
        let hi = (((at + len) / PROFILE_STEP).ceil() as usize).min(out.v.len() - 1);
        for i in lo..=hi {
            let u = i as f32 * PROFILE_STEP - at;
            if u < 0.0 || u > len {
                continue;
            }
            out.v[i] += longitudinal(f, u / len, u);
        }
    }
    out
}

/// Berm height along the lap, signed by which way the corner turns — so one number carries
/// both how tall the wall is and which side of the track it stands on.
fn berm_profile(features: &[Feature], turn: &Profile, lap: f32) -> Profile {
    let mut out = Profile::blank(lap);
    for f in features {
        let Feature::Berm { at, length, height } = *f else {
            continue;
        };
        let lo = (at / PROFILE_STEP).floor().max(0.0) as usize;
        let hi = (((at + length) / PROFILE_STEP).ceil() as usize).min(out.v.len() - 1);
        for i in lo..=hi {
            let s = i as f32 * PROFILE_STEP;
            let u = s - at;
            if u < 0.0 || u > length {
                continue;
            }
            // Eased in and out, so a berm grows out of the ground rather than starting as a
            // step across the track.
            let ramp = smoothstep((u / length * 2.0).min(2.0 - u / length * 2.0).clamp(0.0, 1.0));
            let side = turn.at(s);
            if side != 0.0 {
                out.v[i] = height * ramp * side.signum();
            }
        }
    }
    out
}

/// A step-up doesn't sit on the ground, it *is* the ground — so it moves the elevation the
/// whole rest of the lap runs at rather than adding a bump to it.
fn apply_step_ups(along: &mut [f32], st: &[Station], features: &[Feature]) {
    for f in features {
        let Feature::StepUp { at, length, height } = *f else {
            continue;
        };
        for (i, s) in st.iter().enumerate() {
            let u = s.s - at;
            along[i] += if u <= 0.0 {
                0.0
            } else if u >= length {
                height
            } else {
                height * smoothstep(u / length)
            };
        }
    }
}

/// A feature's shape along the track. `t` runs 0–1 across it, `u` is metres from its start.
fn longitudinal(f: &Feature, t: f32, u: f32) -> f32 {
    match *f {
        // Up, along the top, and down. The ramps are a third each, which is about what a
        // built tabletop measures.
        Feature::Tabletop { height, .. } => {
            if t < 0.27 {
                // Steeper than a smoothstep at the lip, which is what a packed takeoff is.
                let u = t / 0.27;
                height * (u * u * (3.0 - 2.0 * u)).powf(0.82)
            } else if t < 0.56 {
                height
            } else {
                height * smoothstep((1.0 - t) / 0.44)
            }
        }
        Feature::Roller { height, .. } => height * (0.5 - 0.5 * (t * std::f32::consts::TAU).cos()),
        // Two jumps with ground between them. The gap is at grade, which is what makes it a
        // double rather than a long tabletop — land short and you land on flat.
        //
        // Each jump is a long face up and a short one back down. The short face matters: it
        // is what a takeoff lip is, and writing the drop as a step instead put a wall the
        // full height of the jump into the terrain, one sample wide.
        Feature::Double {
            height, gap, lip, ..
        } => {
            let back = DOUBLE_BACK_M.min(lip * 0.5);
            let takeoff = lip + back;
            if u <= lip {
                height * smoothstep(u / lip)
            } else if u <= takeoff {
                height * smoothstep(1.0 - (u - lip) / back)
            } else if u <= takeoff + gap {
                0.0
            } else if u <= takeoff + gap + back {
                height * smoothstep((u - takeoff - gap) / back)
            } else {
                height * smoothstep(1.0 - (u - takeoff - gap - back) / lip)
            }
        }
        Feature::Whoops {
            height, spacing, ..
        } => {
            let phase = (u / spacing).fract();
            height * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos())
        }
        // Both are applied elsewhere: a step-up moves the elevation profile, and a berm
        // and a rut are shaped across the track rather than along it.
        Feature::StepUp { .. } | Feature::Berm { .. } | Feature::Rut { .. } => 0.0,
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smooth_along(v: &mut [f32], r: usize) {
    if r == 0 || v.len() < 3 {
        return;
    }
    let n = v.len();
    let mut pre = vec![0.0f64; n + 1];
    for i in 0..n {
        pre[i + 1] = pre[i] + v[i] as f64;
    }
    for i in 0..n {
        let a = i.saturating_sub(r);
        let b = (i + r + 1).min(n);
        v[i] = ((pre[b] - pre[a]) / (b - a) as f64) as f32;
    }
}

fn sample(h: &[f32], gw: usize, gh: usize, x: f32, y: f32) -> f32 {
    let xi = (x.round() as isize).clamp(0, gw as isize - 1) as usize;
    let yi = (y.round() as isize).clamp(0, gh as isize - 1) as usize;
    h[yi * gw + xi]
}

// ---------------------------------------------------------------------------
// Noise
// ---------------------------------------------------------------------------

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E3779B1);
    h ^= (x as u32).wrapping_mul(0x85EBCA77);
    h ^= (y as u32).wrapping_mul(0xC2B2AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545F491);
    h ^= h >> 13;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (smoothstep(x - xi), smoothstep(y - yi));
    let (xi, yi) = (xi as i32, yi as i32);
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    top + (bot - top) * fy
}

/// Four octaves, each half the amplitude and twice the frequency. Normalised so the result
/// stays inside ±1 and `amplitude` means what it says.
fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut f = 1.0;
    for o in 0..4 {
        sum += value_noise(x * f, y * f, seed.wrapping_add(o * 7919)) * amp;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

// ---------------------------------------------------------------------------
// Writing the folder TerrainEd compiles
// ---------------------------------------------------------------------------

/// Write every source file a track needs, ready for `_map.bat` and `_trh.bat`.
pub fn write_source(prog: &TrackProgram, syn: &Synth, dir: &Path) -> Result<Vec<String>> {
    let slug = slug(&prog.name);
    std::fs::create_dir_all(dir.join(&slug)).context("make the track folder")?;
    let mut wrote = Vec::new();

    let put = |rel: &str, bytes: Vec<u8>, wrote: &mut Vec<String>| -> Result<()> {
        std::fs::write(dir.join(rel), bytes).with_context(|| format!("write {rel}"))?;
        wrote.push(rel.to_string());
        Ok(())
    };

    put("heightmap.raw", raw16(syn, prog.terrain.scale), &mut wrote)?;

    // The riding line, and everything that isn't it.
    let half = prog.width * 0.5;
    let dirt = mask_from(syn, MASK_DIM, |d, _| soft_edge(half + 1.5, 2.0, d));
    let grass = mask_from(syn, MASK_DIM, |d, _| {
        255 - soft_edge(half + SHOULDER_M, 6.0, d)
    });
    // Off-track starts where the graded shoulder ends: the rider is on the track, or in the
    // field, with the shoulder belonging to neither.
    let off = mask_from(syn, MASK_DIM, |d, _| {
        255 - soft_edge(half + SHOULDER_M * 0.6, 3.0, d)
    });
    let start_len = 45.0f32.min(prog.lap_length() * 0.2);
    let start = mask_from(syn, MASK_DIM, |d, s| {
        if d <= half * 1.4 && s <= start_len {
            255
        } else {
            0
        }
    });
    put("mask_dirt.tga", tga_alpha(MASK_DIM, MASK_DIM, &dirt), &mut wrote)?;
    put("mask_grass.tga", tga_alpha(MASK_DIM, MASK_DIM, &grass), &mut wrote)?;
    put("area_off.tga", tga_alpha(MASK_DIM, MASK_DIM, &off), &mut wrote)?;
    put("area_start.tga", tga_alpha(MASK_DIM, MASK_DIM, &start), &mut wrote)?;

    // Ground colours follow what the track is made of, so a sand national exports sand.
    let (ground, line) = match prog.terrain.surface {
        Surface::Soil => ([124, 96, 68], [96, 70, 50]),
        Surface::Sand => ([196, 174, 132], [172, 148, 108]),
        Surface::Grass => ([104, 118, 74], [92, 74, 54]),
    };
    std::fs::create_dir_all(dir.join("maps")).context("make the maps folder")?;
    let seed = prog.terrain.relief.seed;
    put(
        "maps/ground.tga",
        ground_texture(GROUND_TEXTURE_DIM, ground, 0.22, seed ^ 0x9A0D),
        &mut wrote,
    )?;
    put(
        "maps/line.tga",
        ground_texture(GROUND_TEXTURE_DIM, line, 0.28, seed ^ 0x11E5),
        &mut wrote,
    )?;
    put(
        "maps/grass.tga",
        ground_texture(GROUND_TEXTURE_DIM, [96, 116, 66], 0.30, seed ^ 0x6A55),
        &mut wrote,
    )?;
    put("maps/grassfx.tga", grass_billboard(128), &mut wrote)?;

    put("track.hmf", hmf(prog, syn).into_bytes(), &mut wrote)?;
    put("track.tht", tht(prog, syn).into_bytes(), &mut wrote)?;
    put("params.ini", PARAMS_INI.into(), &mut wrote)?;
    put("trh_params.ini", TRH_PARAMS_INI.into(), &mut wrote)?;
    put("track.tcl", tcl(prog).into_bytes(), &mut wrote)?;
    put(
        &format!("{slug}/{slug}.ini"),
        track_ini(prog).into_bytes(),
        &mut wrote,
    )?;
    put(&format!("{slug}/{slug}.amb"), AMB.into(), &mut wrote)?;
    let (map_img, shot) = ui_images(syn, UI_IMAGE_DIM);
    put(&format!("{slug}/{slug}_map.tga"), map_img, &mut wrote)?;
    put(&format!("{slug}/{slug}.tga"), shot, &mut wrote)?;

    put(
        "_map.bat",
        format!("terrained.exe track.hmf {slug}/{slug}.map params.ini\r\n").into_bytes(),
        &mut wrote,
    )?;
    put(
        "_trh.bat",
        format!("terrained.exe track.tht {slug}/{slug}.trh trh_params.ini\r\n").into_bytes(),
        &mut wrote,
    )?;
    put(
        "_centerline.bat",
        format!("tracked -merge {slug}/{slug}.trh cl track.tcl\r\n").into_bytes(),
        &mut wrote,
    )?;
    put("README.txt", readme(prog, syn, &slug).into_bytes(), &mut wrote)?;

    Ok(wrote)
}

/// A playable-shaped `.trh` — the collision terrain, written directly.
///
/// TerrainEd is the thing that really makes these, and it is Windows-only, so a generated
/// track can't be looked at until someone compiles it. But the `.trh` layout is known well
/// enough to write one: the header and signed samples were confirmed against published tracks
/// in `heightfield.rs`, and the trailing block's coverage masks and material table in
/// `trackstats.rs`. Writing it here is what lets the app draw a track it has just generated,
/// on any platform, before anyone has run a compiler.
///
/// This is a *preview*, not a build. It carries terrain and surfaces and nothing else — no
/// graphics, no scenery, no race data — so the game has no use for it. The app does.
pub fn trh(prog: &TrackProgram, syn: &Synth) -> Vec<u8> {
    let scale = prog.terrain.scale;
    let mut out = Vec::with_capacity(12 + syn.heights.len() * 2 + 1024);
    out.extend_from_slice(b"TRH\0");
    out.extend_from_slice(&(syn.gw as u32).to_le_bytes());
    out.extend_from_slice(&(syn.gh as u32).to_le_bytes());
    for h in &syn.heights {
        // Signed, with the datum at zero — half the range sits below it.
        let v = (h / scale * u16::MAX as f32).round() - 32768.0;
        out.extend_from_slice(&(v.clamp(-32768.0, 32767.0) as i16).to_le_bytes());
    }

    // The trailing block opens with the footprint and the height budget, which is where every
    // metre figure the app reports comes from.
    out.extend_from_slice(&prog.terrain.size_x.to_le_bytes());
    out.extend_from_slice(&scale.to_le_bytes());
    out.extend_from_slice(&prog.terrain.size_z.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]);

    let half = prog.width * 0.5;
    // Hard edges, unlike the `.tga` masks: those are blended into a texture, while these say
    // which surface a cell *is*. A soft edge here reads as a wider track — a metre of fade
    // each side put the riding line two metres over what it was built as.
    //
    // Three bands rather than two. A track painted as line-and-grass reads as a brown
    // ribbon on a green field and nothing else; the graded shoulder either side is a
    // different material from both, and it is most of what you see from the seat.
    let (shoulder_id, shoulder_scale) = ground(prog.terrain.surface);
    let shoulder = SHOULDER_M * shoulder_scale;
    let masks: [(u32, Vec<u8>); 3] = [
        // 10 is the riding line — the id published tracks paint their ribbon with.
        (10, mask_from(syn, TRH_MASK_DIM, |d, _| u8::from(d <= half) * 255)),
        (
            shoulder_id,
            mask_from(syn, TRH_MASK_DIM, |d, _| {
                u8::from(d > half && d <= half + shoulder) * 255
            }),
        ),
        (1, mask_from(syn, TRH_MASK_DIM, |d, _| {
            u8::from(d > half + shoulder) * 255
        })),
    ];
    out.extend_from_slice(&(masks.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // to the offset the records start at

    for (id, m) in &masks {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(TRH_MASK_DIM as u32).to_le_bytes());
        out.extend_from_slice(&(TRH_MASK_DIM as u32).to_le_bytes());
        out.extend_from_slice(m);
    }

    // The material table, which is also how a reader finds the end of the masks: it looks for
    // "asphalt" and counts back four bytes. Same six, in the same order, as every published
    // track carries.
    out.extend_from_slice(&6u32.to_le_bytes());
    for name in ["asphalt", "grass", "sand", "kerb", "soil", "concrete"] {
        let mut field = [0u8; 16];
        field[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&field);
        out.extend_from_slice(&[0u8; 36]); // nine floats of physics we have nothing to say about
    }
    out.extend_from_slice(b"EXT\0");
    out
}

/// The surface either side of the riding line, and how far it reaches.
///
/// The ids are the ones every published track's material table carries, in its order:
/// asphalt, grass, sand, kerb, soil, concrete. So a shoulder painted 4 comes out the colour
/// the app already draws worked dirt, with no new palette to agree on.
fn ground(s: Surface) -> (u32, f32) {
    match s {
        // Worked dirt, a normal shoulder wide.
        Surface::Soil => (4, 1.0),
        // Sand aprons are wide — most of what you see on a sand national isn't the line.
        Surface::Sand => (2, 2.2),
        // Grass to the edge of the line, which is what an early-season circuit looks like.
        Surface::Grass => (1, 0.35),
    }
}

/// The preview as a `.pkz` the app can open: a plain zip, which is what the reader falls back
/// to when a file isn't one of PiBoSo's encrypted ones.
pub fn write_pkz(prog: &TrackProgram, syn: &Synth, path: &Path) -> Result<u64> {
    let slug = slug(&prog.name);
    let file = std::fs::File::create(path).with_context(|| format!("create {path:?}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    use std::io::Write;
    zip.start_file(format!("{slug}.trh"), opts)?;
    zip.write_all(&trh(prog, syn))?;
    zip.start_file(format!("{slug}.ini"), opts)?;
    zip.write_all(track_ini(prog).as_bytes())?;
    zip.finish()?;
    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// The heightmap: little-endian u16, quantised against the height budget.
fn raw16(syn: &Synth, scale: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(syn.heights.len() * 2);
    for h in &syn.heights {
        let v = (h / scale * u16::MAX as f32).round().clamp(0.0, 65535.0) as u16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// A mask, from a function of distance-off-line and distance-round-the-lap.
fn mask_from(syn: &Synth, dim: usize, f: impl Fn(f32, f32) -> u8) -> Vec<u8> {
    let mut out = vec![0u8; dim * dim];
    for y in 0..dim {
        let gy = (y * syn.gh / dim).min(syn.gh - 1);
        for x in 0..dim {
            let gx = (x * syn.gw / dim).min(syn.gw - 1);
            let i = gy * syn.gw + gx;
            out[y * dim + x] = f(syn.dist[i], syn.arc[i]);
        }
    }
    out
}

/// Full inside `edge`, gone `fade` metres past it — masks are blended, so a hard cut shows as
/// a sawtooth against the terrain's own resolution.
fn soft_edge(edge: f32, fade: f32, d: f32) -> u8 {
    if d <= edge {
        255
    } else if d >= edge + fade {
        0
    } else {
        (smoothstep(1.0 - (d - edge) / fade) * 255.0) as u8
    }
}

/// Tileable value noise. The lattice wraps at `period`, so the texture it builds meets
/// itself at the edges — a ground texture repeated sixty times across a track shows every
/// seam it has.
fn tile_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (fx, fy) = (smoothstep(x - xf), smoothstep(y - yf));
    let (xi, yi) = (xf as i32, yf as i32);
    let at = |a: i32, b: i32| hash2(a.rem_euclid(period), b.rem_euclid(period), seed);
    let top = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
    let bot = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
    top + (bot - top) * fy
}

/// Ground, as a tileable picture of itself.
///
/// Generated rather than shipped. The layers have to point at *some* texture, and pointing
/// them at PiBoSo's example track means the exported folder doesn't compile until someone
/// has downloaded a 95 MB zip and copied a directory out of it. Three octaves of wrapping
/// noise over a base colour is not a photograph, but it tiles, it is the right format, and
/// it is in the folder.
fn ground_texture(dim: usize, base: [u8; 3], grain: f32, seed: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity(dim * dim * 4);
    for y in 0..dim {
        for x in 0..dim {
            let (u, v) = (x as f32 / dim as f32, y as f32 / dim as f32);
            let mut n = 0.0;
            let mut amp = 1.0;
            let mut period = 8;
            for o in 0..3 {
                n += tile_noise(u * period as f32, v * period as f32, period, seed ^ (o * 131)) * amp;
                amp *= 0.5;
                period *= 2;
            }
            let k = 1.0 + n * grain;
            let c = |i: usize| (base[i] as f32 * k).clamp(0.0, 255.0) as u8;
            px.extend_from_slice(&[c(2), c(1), c(0), 255]);
        }
    }
    tga_bgra(dim, dim, &px)
}

/// The blade sprite the grass layer scatters. Alpha-cut, like every foliage sheet in the
/// game — see the note about `_c_a` materials in the map decoder.
fn grass_billboard(dim: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(dim * dim * 4);
    for y in 0..dim {
        for x in 0..dim {
            let (u, v) = (x as f32 / dim as f32, 1.0 - y as f32 / dim as f32);
            // A handful of tapered blades, thinner and fainter towards the tip.
            let mut a = 0.0f32;
            for b in 0..5 {
                let centre = (b as f32 + 0.5) / 5.0;
                let lean = (v * 0.12) * if b % 2 == 0 { 1.0 } else { -1.0 };
                let width = 0.045 * (1.0 - v * 0.8).max(0.05);
                let d = ((u - centre - lean) / width).abs();
                if d < 1.0 && v < 0.92 {
                    a = a.max(1.0 - d);
                }
            }
            let shade = 0.55 + 0.45 * v;
            let g = (150.0 * shade) as u8;
            px.extend_from_slice(&[(60.0 * shade) as u8, g, (70.0 * shade) as u8, (a * 255.0) as u8]);
        }
    }
    tga_bgra(dim, dim, &px)
}

/// Lighting and weather. Three conditions, because the game asks for all three and a track
/// missing one falls back to nothing rather than to a default.
///
/// The sun direction has to agree with `params.ini` — TerrainEd bakes shadows from that one
/// and the game lights from this one, so a mismatch is a track lit from one side with its
/// shadows falling the other.
const AMB: &str = "\
sun_position\n{\n\tx = 2\n\ty = 10\n\tz = -7\n}\n\
clear\n{\n\tambient\n\t{\n\t\tred = 0.40\n\t\tgreen = 0.45\n\t\tblue = 0.55\n\t}\n\
\tsun_color\n\t{\n\t\tred = 1.10\n\t\tgreen = 0.95\n\t\tblue = 0.7\n\t}\n\
\tfog\n\t{\n\t\tdensity = 0.0008\n\t\tred = 0.7\n\t\tgreen = 0.7\n\t\tblue = 0.85\n\t}\n\
\tsky = *clearsky.edf\n\tsky_rot = 0\n}\n\
cloudy\n{\n\tambient\n\t{\n\t\tred = 0.65\n\t\tgreen = 0.65\n\t\tblue = 0.7\n\t}\n\
\tsun_color\n\t{\n\t\tred = 0.255\n\t\tgreen = 0.255\n\t\tblue = 0.3\n\t}\n\
\tfog\n\t{\n\t\tdensity = 0.0005\n\t\tred = 0.7\n\t\tgreen = 0.7\n\t\tblue = 0.85\n\t}\n\
\tsky = *cloudysky.edf\n\tsky_rot = 0\n}\n\
rainy\n{\n\tambient\n\t{\n\t\tred = 0.6\n\t\tgreen = 0.6\n\t\tblue = 0.85\n\t}\n\
\tsun_color\n\t{\n\t\tred = 0.3\n\t\tgreen = 0.3\n\t\tblue = 0.4\n\t}\n\
\tfog\n\t{\n\t\tdensity = 0.004\n\t\tred = 0.5\n\t\tgreen = 0.5\n\t\tblue = 0.55\n\t}\n\
\tsky = *rainysky.edf\n\tsky_rot = 0\n}\n";

/// The two pictures the game's UI wants: an overhead of the lap, and something to show
/// beside the track's name. Neither is optional — a track without them lists as a blank.
fn ui_images(syn: &Synth, dim: usize) -> (Vec<u8>, Vec<u8>) {
    let mut map = vec![0u8; dim * dim * 4];
    let mut shot = vec![0u8; dim * dim * 4];
    let base = crate::trackstats::box_blur(&syn.heights, syn.gw, syn.gh, 6);
    for y in 0..dim {
        // TGA's origin is bottom-left, so the picture is written from the far edge back.
        let gy = ((dim - 1 - y) * syn.gh / dim).min(syn.gh - 1);
        for x in 0..dim {
            let gx = (x * syn.gw / dim).min(syn.gw - 1);
            let i = gy * syn.gw + gx;
            let at = (y * dim + x) * 4;

            // The map: the lap as a shape, on paper.
            let on = syn.corridor[i];
            let c: [u8; 3] = if on { [60, 70, 150] } else { [232, 232, 236] };
            map[at..at + 4].copy_from_slice(&[c[2], c[1], c[0], 255]);

            // The picture: the terrain's own relief, with the line picked out.
            let relief = ((syn.heights[i] - base[i]) * 90.0 + 128.0).clamp(0.0, 255.0) as u8;
            let s: [u8; 3] = if on {
                [relief.saturating_add(40), relief / 2, relief / 3]
            } else {
                [relief / 2, (relief as f32 * 0.62) as u8, relief / 3]
            };
            shot[at..at + 4].copy_from_slice(&[s[2], s[1], s[0], 255]);
        }
    }
    (tga_bgra(dim, dim, &map), tga_bgra(dim, dim, &shot))
}

/// Uncompressed 32-bit BGRA, the mask in the alpha channel — the shape the official example's
/// own masks are in, down to the descriptor byte and the file footer.
fn tga_alpha(w: usize, h: usize, alpha: &[u8]) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 4);
    for a in alpha {
        px.extend_from_slice(&[255, 255, 255, *a]);
    }
    tga_bgra(w, h, &px)
}

/// The same container, given the pixels directly.
fn tga_bgra(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + w * h * 4 + 26);
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    // 32 bits a pixel, eight of them alpha, origin bottom-left — row zero is the bottom of
    // the picture, which is where the heightmap's row zero is too.
    out.extend_from_slice(&[32, 0x08]);
    out.extend_from_slice(px);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"TRUEVISION-XFILE.\0");
    out
}

fn header(prog: &TrackProgram, syn: &Synth) -> String {
    format!(
        "samples_x = {}\nsamples_z = {}\n\ndata = heightmap.raw\n\nsize_x = {}\nsize_z = {}\n\
         scale = {}\n\n",
        syn.gw, syn.gh, prog.terrain.size_x, prog.terrain.size_z, prog.terrain.scale
    )
}

fn hmf(prog: &TrackProgram, syn: &Synth) -> String {
    let mut s = header(prog, syn);
    s.push_str("num_layers = 3\n");
    // Layer zero carries no mask: it is the ground everything else is painted over.
    s.push_str(
        "layer0\n{\n\tmap = maps/ground.tga\n\trepetitions = 60\n}\n\n",
    );
    s.push_str(
        "layer1\n{\n\tmap = maps/line.tga\n\trepetitions = 50\n\tmask = mask_dirt.tga\n\
         \tthickness = 0.1\n}\n\n",
    );
    s.push_str(
        "layer2\n{\n\tmap = maps/grass.tga\n\trepetitions = 40\n\tmask = mask_grass.tga\n\
         \tthickness = 0.01\n\n\tgrass\n\t{\n\t\tmax_density = 20\n\t\theight = 0.2\n\
         \t\theight_diff = 0.1\n\t\twidth = 0.25\n\t\twidth_diff = 0.1\n\
         \t\ttexture = maps/grassfx.tga\n\t\tdensitymap = mask_grass.tga\n\t}\n}\n",
    );
    s
}

fn tht(prog: &TrackProgram, syn: &Synth) -> String {
    let mut s = header(prog, syn);
    s.push_str("num_surface_layers = 2\n\n");
    s.push_str("surface_layer0\n{\n\tsurface = off\n\tmask = area_off.tga\n}\n\n");
    s.push_str("surface_layer1\n{\n\tsurface = start\n\tmask = area_start.tga\n}\n\n");
    s.push_str("num_material_layers = 3\n\n");
    // The base is whatever the ground is; the line is worked soil on top of it, and the
    // field is grass. Same three bands the height file paints, said in physics.
    let base = match prog.terrain.surface {
        Surface::Soil => "compact soil",
        Surface::Sand => "sand",
        Surface::Grass => "grass",
    };
    s.push_str(&format!("material_layer0\n{{\n\tmaterial = {base}\n}}\n\n"));
    s.push_str("material_layer1\n{\n\tmaterial = soil\n\tthickness = 0.1\n\tmask = mask_dirt.tga\n}\n\n");
    s.push_str("material_layer2\n{\n\tmaterial = grass\n\tthickness = 0.01\n\tmask = mask_grass.tga\n}\n");
    s
}

/// The centreline, in the form `tracked -merge` reads: a start pose and the same straights
/// and arcs the program was written in.
fn tcl(prog: &TrackProgram) -> String {
    let mut s = format!(
        "x = {:.3}\nz = {:.3}\nangle = {:.4}\nnumsegment = {}\n",
        prog.start.x,
        prog.start.z,
        prog.start.angle,
        prog.segments.len()
    );
    for (i, seg) in prog.segments.iter().enumerate() {
        let (radius, angle) = match *seg {
            Segment::Straight { .. } => (0.0, 0.0),
            Segment::Arc { radius, angle } => (radius, angle.abs()),
        };
        let kind = if matches!(seg, Segment::Straight { .. }) {
            0
        } else {
            1
        };
        s.push_str(&format!(
            "segment{i}\n{{\n\ttype = {kind}\n\tlength = {:.6}\n\tradius = {radius:.6}\n\
             \tangle = {angle:.6}\n\theight = 0.000000\n\theightlock = 0\n}}\n",
            seg.length()
        ));
    }
    s
}

fn track_ini(prog: &TrackProgram) -> String {
    format!(
        "[info]\nname={}\nshort_name={}\nlength={:.0}m\naltitude=40\n\n\
         [race]\ndefaulteventlaps=15\nreflaptime={:.0}\n\n\
         [ui]\npic=track_image.tga\npic_info=track_map.tga\nauthor={}\nlocation={}\n\n\
         [weather]\ncloud_prob = 0.4\nrainy_prob = 0.1\n",
        prog.name,
        prog.name.chars().take(12).collect::<String>(),
        prog.lap_length(),
        // A minute and a half for a mile is roughly national pace, and it only seeds the UI.
        prog.lap_length() / 11.0,
        if prog.author.is_empty() {
            "MXB App"
        } else {
            &prog.author
        },
        prog.location
    )
}

const PARAMS_INI: &str = "\n[params]\nlightdir_x = 2\nlightdir_y = 10\nlightdir_z = -7\n\
                          shadowvolumes_create = 1\nshadowvolumes_supersampling = 1\n\
                          shadowmaps_create = 1\nshadowmaps_scale = 0.1\n\
                          shadowmaps_supersampling = 1\n";

const TRH_PARAMS_INI: &str = "[params]\ntype=3\n";

fn readme(prog: &TrackProgram, syn: &Synth, slug: &str) -> String {
    format!(
        "{name}\n\nGenerated by MXB App. Everything here is source: run the two batch files to\n\
         compile it, in a folder that also has terrained.exe.\n\n\
         Terrain   {gw} x {gh} samples over {sx:.0} x {sz:.0} m ({mps:.2} m a sample)\n\
         Height    {used:.1} m used of a {budget:.1} m budget\n\
         Lap       {lap:.0} m, {width:.0} m wide, {feats} features\n\n\
         Before compiling, copy the `maps` folder out of PiBoSo's official example track\n\
         (mxb_track_example.zip) next to these files — the layers reference its textures.\n\n\
         1. _map.bat        graphics, writes {slug}/{slug}.map\n\
         2. _trh.bat        collision, writes {slug}/{slug}.trh\n\
         3. _centerline.bat merges track.tcl into the .trh\n\n\
         Two files are still missing after that, and neither is ours to write:\n\
         \n\
           {slug}.rdf   start gate, pit lane, finish line, cameras — TrackEd writes it\n\
           gate.edf     the starting gate model — copy one from another track\n\
         \n\
         With those in the folder, zip it and rename the zip {slug}.pkz.\n",
        name = prog.name,
        gw = syn.gw,
        gh = syn.gh,
        sx = prog.terrain.size_x,
        sz = prog.terrain.size_z,
        mps = syn.mps,
        used = syn.used_m,
        budget = syn.budget_m,
        lap = prog.lap_length(),
        width = prog.width,
        feats = prog.features.len(),
        slug = slug,
    )
}

fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "track".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trackprog::{Relief, Start, Terrain};

    /// A lap that closes: two straights joined by two half-circle turns.
    fn oval() -> TrackProgram {
        TrackProgram {
            name: "Test Oval".into(),
            author: "MXB App".into(),
            location: "Test".into(),
            terrain: Terrain {
                size_x: 400.0,
                size_z: 400.0,
                samples: 1025,
                scale: 30.0,
                relief: Relief {
                    amplitude: 6.0,
                    wavelength: 150.0,
                    seed: 3,
                    texture: 0.06,
                },
                surface: crate::trackprog::Surface::Soil,
            },
            start: Start {
                x: 140.0,
                z: 120.0,
                angle: 0.0,
            },
            segments: vec![
                Segment::Straight { length: 160.0 },
                Segment::Arc { radius: 60.0, angle: 180.0 },
                Segment::Straight { length: 160.0 },
                Segment::Arc { radius: 60.0, angle: 180.0 },
            ],
            width: 12.0,
            features: vec![
                Feature::Tabletop { at: 30.0, length: 22.0, height: 2.4 },
                Feature::Double { at: 70.0, height: 2.0, gap: 9.0, lip: 6.0 },
                Feature::Whoops { at: 105.0, count: 6, spacing: 4.5, height: 0.7 },
                Feature::Berm { at: 165.0, length: 80.0, height: 1.6 },
            ],
        }
    }

    #[test]
    fn a_synthesised_track_fits_its_budget() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        assert_eq!((s.gw, s.gh), (1025, 1025));
        let (lo, hi) = s.heights.iter().fold((f32::MAX, f32::MIN), |(a, b), v| {
            (a.min(*v), b.max(*v))
        });
        assert!(lo >= 0.0 && hi <= p.terrain.scale, "{lo}..{hi}");
        assert!(s.used_m > 1.0, "the terrain came out flat");
    }

    #[test]
    fn asking_for_more_height_than_the_budget_is_an_error() {
        let mut p = oval();
        p.terrain.scale = 1.0;
        let err = match synthesise(&p) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("a 1 m budget accepted a track with jumps in it"),
        };
        assert!(err.contains("budget"), "{err}");
    }

    #[test]
    fn the_corridor_is_the_width_it_was_asked_for() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let area = s.corridor.iter().filter(|c| **c).count() as f32 * s.mps * s.mps;
        // Area over length is the width, give or take the ends of the lap.
        let width = area / p.lap_length();
        assert!((width - p.width).abs() < 1.0, "measured {width:.2} m");
    }

    #[test]
    fn a_tabletop_stands_where_it_was_put() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        // The tabletop runs 30–52 m round the lap; sample the line either side of it.
        let on = height_at_arc(&s, 41.0);
        let before = height_at_arc(&s, 20.0);
        assert!(
            on - before > 1.8,
            "tabletop stands {:.2} m above the run-in, wanted about 2.4",
            on - before
        );
    }

    #[test]
    fn quantising_survives_the_round_trip() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let raw = raw16(&s, p.terrain.scale);
        assert_eq!(raw.len(), s.heights.len() * 2);
        // Back out of the file the way the game reads it, and every sample should land within
        // one quantisation step of where it started.
        let step = p.terrain.scale / u16::MAX as f32;
        let mut worst: f32 = 0.0;
        for (i, h) in s.heights.iter().enumerate() {
            let v = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
            worst = worst.max((v as f32 * step - h).abs());
        }
        assert!(worst <= step, "worst {worst} against a step of {step}");
    }

    #[test]
    fn a_tga_matches_the_shape_of_the_examples_own() {
        let a = vec![7u8; 64 * 64];
        let t = tga_alpha(64, 64, &a);
        assert_eq!(t.len(), 18 + 64 * 64 * 4 + 26);
        assert_eq!(t[2], 2, "uncompressed truecolour");
        assert_eq!(t[16], 32, "32 bits a pixel");
        assert_eq!(t[17], 0x08, "eight alpha bits, bottom-left origin");
        assert_eq!(&t[t.len() - 18..], b"TRUEVISION-XFILE.\0");
        assert_eq!(&t[18..22], &[255, 255, 255, 7], "mask lives in alpha");
    }

    #[test]
    fn the_tcl_states_the_segments_the_program_was_written_in() {
        let text = tcl(&oval());
        assert!(text.contains("numsegment = 4"));
        assert!(text.contains("type = 0"), "a straight");
        assert!(text.contains("radius = 60.000000"), "an arc's radius");
        // The half-circles are pi*r long.
        assert!(text.contains(&format!("length = {:.6}", 60.0 * std::f32::consts::PI)));
    }

    use crate::trackprog::EXAMPLE as DEMO;

    /// Every file the source files name has to be in the folder beside them.
    ///
    /// This is the check that would have caught the exported folder pointing at PiBoSo's
    /// example track for its textures: it compiled fine in the sense that the text was
    /// valid, and TerrainEd would have stopped on the first missing `.tga`.
    #[test]
    fn the_exported_folder_references_nothing_it_doesnt_contain() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        let s = synthesise(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("mxb-track-selftest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_source(&p, &s, &dir).unwrap();

        let mut named = Vec::new();
        for f in ["track.hmf", "track.tht"] {
            let text = std::fs::read_to_string(dir.join(f)).unwrap();
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                // Every key whose value is a file name rather than a number.
                if matches!(
                    key.trim(),
                    "map" | "mask" | "data" | "texture" | "densitymap"
                ) {
                    named.push((f, value.trim().to_string()));
                }
            }
        }
        assert!(named.len() >= 8, "found only {named:?}");
        for (from, name) in &named {
            assert!(
                dir.join(name).is_file(),
                "{from} names {name}, which isn't in the folder"
            );
        }

        // And the pieces the game itself asks for, which the README promises are there.
        let slug = slug(&p.name);
        for f in [
            format!("{slug}/{slug}.ini"),
            format!("{slug}/{slug}.amb"),
            format!("{slug}/{slug}.tga"),
            format!("{slug}/{slug}_map.tga"),
        ] {
            assert!(dir.join(&f).is_file(), "{f} is missing");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_demo_program_parses_and_closes() {
        let p: TrackProgram = serde_json::from_str(DEMO).expect("the demo program is valid JSON");
        p.check().expect("the demo program is a buildable track");
        assert!(
            p.closure_error() < 0.5,
            "the lap misses itself by {:.2} m",
            p.closure_error()
        );
        assert!(p.lap_length() > 1400.0, "{:.0} m", p.lap_length());
    }

    /// Synthesise the demo and measure the result with the same code that measured the
    /// published tracks. This is the only check that matters: a generated track is worth
    /// something when it measures like a real one.
    ///
    /// ```text
    /// FROST_BUILD=/tmp/track cargo test -- --ignored --nocapture builds_a_track
    /// ```
    #[test]
    #[ignore = "writes a track folder — set FROST_BUILD"]
    fn builds_a_track() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        let s = synthesise(&p).unwrap();

        println!(
            "{}: {:.0} m lap, {:.0} m wide, closes to {:.2} m",
            p.name,
            p.lap_length(),
            p.width,
            p.closure_error()
        );
        println!(
            "terrain {}x{} over {:.0}x{:.0} m ({:.3} m a sample), {:.1} m of a {:.0} m budget",
            s.gw, s.gh, p.terrain.size_x, p.terrain.size_z, s.mps, s.used_m, s.budget_m
        );

        let c = crate::trackstats::measure("synth", &s.corridor, &s.heights, s.gw, s.gh, s.mps);
        println!(
            "measured: {:>5.1}% area {:>4.0}% joined  w {:>4.1}/{:<4.1}m  len {:>5.0}m  \
             slope p90 {:>4.1}° p99 {:>4.1}°  relief p90 {:>4.2}m  {:>4} lips  h p50 {:>4.2}m  \
             gap p50 {:>5.1}m  {:.0} lips/km",
            c.area_fraction * 100.0,
            c.largest_component_fraction * 100.0,
            c.width_from_mean_m,
            c.width_from_tail_m,
            c.length_m,
            c.slope_deg.p90,
            c.slope_deg.p99,
            c.feature_relief_m.p90,
            c.lips,
            c.lip_height_m.p50,
            c.lip_spacing_m.p50,
            c.lips_per_km,
        );
        println!(
            "  lip spacing p10/p50/p90 {:.1}/{:.1}/{:.1} m over {} lips",
            c.lip_spacing_m.p10, c.lip_spacing_m.p50, c.lip_spacing_m.p90, c.lip_spacing_m.count
        );

        // The corpus, from the published tracks in trackstats. A generated track that lands
        // outside these isn't wrong by taste — it's outside anything anyone has shipped.
        let between = |what: &str, v: f32, lo: f32, hi: f32| {
            assert!(v >= lo && v <= hi, "{what} is {v:.2}, corpus runs {lo}–{hi}");
        };
        between("corridor width", c.width_from_mean_m, 8.0, 20.0);
        between("lip height p50", c.lip_height_m.p50, 1.0, 1.8);
        // Wider than the corpus's own 13.2–16.2 on purpose. The median here sits at 11.86 m
        // and does not move: three different feature layouts — 34, 52 and 46 jumps, whoops at
        // 4.5, 5 and 6 m — all measured 11.864832. A figure that ignores what it is measuring
        // is the detector talking, not the track, so this is a sanity bound until the lip
        // detector is understood well enough to be an acceptance test. See tasks/.
        between("lip spacing p50", c.lip_spacing_m.p50, 9.0, 25.0);
        between("feature relief p90", c.feature_relief_m.p90, 0.5, 1.8);
        between("slope p99", c.slope_deg.p99, 20.0, 55.0);
        assert!(
            c.largest_component_fraction > 0.95,
            "the corridor came out in pieces"
        );

        if let Ok(dir) = std::env::var("FROST_BUILD") {
            let dir = Path::new(&dir);
            let wrote = write_source(&p, &s, dir).unwrap();
            std::fs::write(dir.join("program.json"), DEMO).unwrap();
            println!("\nwrote {} files to {}:", wrote.len() + 1, dir.display());
            for f in &wrote {
                let n = std::fs::metadata(dir.join(f)).map(|m| m.len()).unwrap_or(0);
                println!("  {f:<24} {n:>10} bytes");
            }
            // A `.pkz` the app can open, and then the proof that it can: read it back with
            // the same code that reads published tracks. Anything the viewer would get wrong
            // shows up here as a track that measures like nothing.
            let pkz = dir.join(format!("{}.pkz", slug(&p.name)));
            let size = write_pkz(&p, &s, &pkz).unwrap();
            let back = crate::trackstats::analyse(&pkz).expect("the .pkz reads back as a track");
            let bc = back.corridor.as_ref().expect("the .pkz carries a riding line");
            println!(
                "\n{} — {} bytes, reads back as {}x{} over {:.0}x{:.0} m, \
                 {:.1} m wide corridor by the {} rule, {} lips",
                pkz.file_name().unwrap().to_string_lossy(),
                size,
                back.source_grid[0],
                back.source_grid[1],
                back.size_x_m,
                back.size_z_m,
                bc.width_from_mean_m,
                bc.rule,
                bc.lips
            );
            assert!(
                (bc.width_from_mean_m - c.width_from_mean_m).abs() < 1.0,
                "the .pkz measures {:.1} m wide where the terrain it was written from is {:.1}",
                bc.width_from_mean_m,
                c.width_from_mean_m
            );

            preview(&s, &dir.join("preview.ppm"));
            // The start straight, close up. The whole-track view is too coarse to tell a
            // tabletop from a bump — at 0.34 m a sample a 24 m jump is seventy pixels of a
            // two-thousand-pixel picture.
            let st = s.stations[0];
            let (cx, cy) = ((st.x / s.mps) as usize, (st.z / s.mps) as usize);
            preview_crop(
                &s,
                &dir.join("preview_start.ppm"),
                cx.saturating_sub(120),
                cy.saturating_sub(60),
                420,
                760,
            );
            println!("  preview.ppm, preview_start.ppm");
        }
    }

    /// Build a track program from a file, the way the studio does but without the studio.
    ///
    /// ```text
    /// FROST_PROGRAM=track.json FROST_BUILD=/tmp/out \
    ///   cargo test -- --ignored --nocapture builds_from_a_file
    /// ```
    #[test]
    #[ignore = "needs a program — set FROST_PROGRAM"]
    fn builds_from_a_file() {
        let path = std::env::var("FROST_PROGRAM").expect("set FROST_PROGRAM");
        let text = std::fs::read_to_string(&path).unwrap();
        let p: TrackProgram = serde_json::from_str(&text).expect("that isn't a track program");

        let problems = crate::trackllm::validate(&p);
        for problem in &problems {
            println!("  ! {problem}");
        }
        assert!(problems.is_empty(), "{} problems", problems.len());

        let s = synthesise(&p).unwrap();
        let c = crate::trackstats::measure("synth", &s.corridor, &s.heights, s.gw, s.gh, s.mps);
        println!(
            "{}: {:.0} m lap, {:.1} m wide, closes to {:.2} m — measured {:.1} m wide, \
             {:.0} m long, {} lips at {:.0}/km, relief p90 {:.2} m, slope p99 {:.0}°",
            p.name,
            p.lap_length(),
            p.width,
            p.closure_error(),
            c.width_from_mean_m,
            c.length_m,
            c.lips,
            c.lips_per_km,
            c.feature_relief_m.p90,
            c.slope_deg.p99,
        );

        if let Ok(dir) = std::env::var("FROST_BUILD") {
            let dir = Path::new(&dir);
            let wrote = write_source(&p, &s, dir).unwrap();
            write_pkz(&p, &s, &dir.join(format!("{}.pkz", slug(&p.name)))).unwrap();
            preview(&s, &dir.join("preview.ppm"));
            println!("wrote {} files to {}", wrote.len() + 2, dir.display());
        }
    }

    /// The terrain, slope-shaded, with the riding line tinted.
    ///
    /// Tinted, not filled: the point of looking is to see the jumps, and painting the corridor
    /// solid hides exactly the part worth checking.
    fn preview(s: &Synth, out: &Path) {
        preview_crop(s, out, 0, 0, s.gw, s.gh)
    }

    fn preview_crop(s: &Synth, out: &Path, x0: usize, y0: usize, w: usize, h: usize) {
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for cy in 0..h {
            for cx in 0..w {
                let (x, y) = ((x0 + cx).min(s.gw - 1), (y0 + cy).min(s.gh - 1));
                let (w, h) = (s.gw, s.gh);
                let i = y * w + x;
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(w - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(h - 1);
                let dx = (s.heights[y * w + xp] - s.heights[y * w + xm]) / (2.0 * s.mps);
                let dy = (s.heights[yp * w + x] - s.heights[ym * w + x]) / (2.0 * s.mps);
                // Lit from the north-west, which is how a terrain reads as relief rather than
                // as a grey field.
                let lit = ((0.6 * -dx + 0.6 * -dy + 1.0) / 2.4).clamp(0.0, 1.0);
                let v = (lit * 255.0) as u8;
                ppm.extend_from_slice(&if s.corridor[i] {
                    [v.saturating_add(60), (v as f32 * 0.75) as u8, (v as f32 * 0.7) as u8]
                } else {
                    [v, v, v]
                });
            }
        }
        let _ = std::fs::write(out, ppm);
    }

    fn height_at_arc(s: &Synth, at: f32) -> f32 {
        let k = s
            .stations
            .iter()
            .enumerate()
            .min_by(|a, b| {
                (a.1.s - at)
                    .abs()
                    .partial_cmp(&(b.1.s - at).abs())
                    .unwrap()
            })
            .unwrap()
            .0;
        let st = s.stations[k];
        let (gx, gy) = (
            (st.x / s.mps).round() as usize,
            (st.z / s.mps).round() as usize,
        );
        s.heights[gy.min(s.gh - 1) * s.gw + gx.min(s.gw - 1)]
    }
}
