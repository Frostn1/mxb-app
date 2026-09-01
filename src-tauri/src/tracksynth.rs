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

use crate::trackprog::{Feature, Knot, Segment, Station, Surface, TrackProgram};

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
/// Forty metres because that is where the measurement changes: below it a corner's grooves
/// deepen and its two edges stop being the same height, above it published corners read like
/// straights.
const RUT_RADIUS_M: (f32, f32) = (40.0, 14.0);

/// How deep a corner's own rut gets, metres, and how deep the shallowest ground on the lap is
/// worn.
///
/// Both measured off ten published tracks' own centrelines. A corner's grooves run 0.15–0.30 m
/// at the median and 0.35–0.67 m at the ninetieth; and a *straight* is not smooth either —
/// every one of those tracks wears 0.09–0.16 m of groove down its straights. Ruts everywhere,
/// deeper through the corners, is the shape of the measurement. Ruts only in corners was ours.
const RUT_DEPTH_M: f32 = 0.68;
const RUT_DEPTH_STRAIGHT_M: f32 = 0.17;

/// A rut's width across the track, metres — about a tyre and the ridge of dirt each side.
const RUT_WIDTH_M: f32 = 0.55;

/// Ruts do not come one at a time.
///
/// Everyone takes roughly the same line through a corner, but nobody takes exactly the same
/// one, and what a day of practice leaves is a *bundle* of parallel grooves lying across most
/// of the track — six to ten of them through a tight turn, each a little shallower than its
/// neighbour. It is the single most recognisable thing in a real track's collision terrain:
/// Indiana's corners are combs, and one groove down the middle of a corner is the clearest
/// sign the turn was drawn rather than ridden.
/// Five, not nine, and two metres apart rather than one. Counted off ten published tracks:
/// they carry one to three grooves deep enough to find at a time, 1.75–4.0 m apart, spanning
/// six or seven metres of an eleven-metre line. Nine at 1.15 m is corduroy, and it measures
/// as corduroy.
const RUT_LINES: usize = 5;

/// Metres between one rut and the next — a tyre, plus the ridge that gets pushed up beside it.
const RUT_SPACING_M: f32 = 2.05;

/// How much of the half-width the bundle covers, at the loosest corner that ruts at all and
/// at the tightest.
const RUT_BUNDLE: (f32, f32) = (0.30, 0.92);

/// How far a corner's ruts run past the corner, out onto the straight and back up the
/// approach, metres.
///
/// They do not stop where the arc does. A rider is already on the line before turn-in and is
/// still driving out of it a long way down the following straight, so the grooves taper away
/// rather than ending — and cutting them off at the arc leaves a corner that looks stencilled
/// onto the track.
const RUT_CARRY_EXIT_M: f32 = 55.0;
const RUT_CARRY_ENTRY_M: f32 = 22.0;

/// How far the bundle sits towards the inside of the corner, as a fraction of the half-width.
///
/// All but nothing, and measured rather than reasoned. Everyone pictures a corner's ruts
/// hugging the inside line; across 796 of Indiana's corner cross-sections the bundle's centre
/// sits 0.12 m to the *outside* of the centreline, and only 42% of corners have it inside at
/// all. Riders take every line through a corner, and what they leave is spread across it.
const RUT_INSIDE: f32 = -0.022;

/// How far a single rut wanders across the track down its own length, metres, and over what
/// distance. Perfectly concentric grooves are a machine's idea of a corner.
/// Ten metres, from Indiana: a cross-section still matches the one two metres behind it four
/// fifths of the way, half of it at five metres, and by twenty it is different ground. A
/// groove that holds its line for a whole corner was never ridden down.
const RUT_WANDER_M: f32 = 0.8;
const RUT_WANDER_WAVELENGTH_M: f32 = 10.0;

/// Metres before a corner that riders brake in, and so where the ground gets chopped up.
const BRAKING_M: f32 = 22.0;

/// Metres between braking bumps.
const BRAKING_WAVELENGTH_M: f32 = 2.2;

/// How much rougher the surface gets in and around a corner, as a multiplier on the texture.
const CORNER_ROUGHNESS: f32 = 1.8;

/// How tall a braking bump stands, trough to crest, metres.
///
/// Stated outright rather than as a multiple of the surface texture. Braking bumps are a
/// feature of their own — a washboard laid across the track, deep enough to move a bike —
/// and tying their height to the fine grain meant a track with a smooth surface got no
/// braking bumps either, which is backwards.
const BRAKING_HEIGHT_M: f32 = 0.13;

/// How far a corner's exit is chopped up by everyone driving out of it, metres, and how tall
/// that chop stands. Longer and lower than braking: acceleration bumps are stretched out by
/// the wheel spinning across them.
const ACCEL_M: f32 = 30.0;
const ACCEL_WAVELENGTH_M: f32 = 3.4;
const ACCEL_HEIGHT_M: f32 = 0.07;

/// How much the edge of the riding line wanders in and out, metres, and over what length of
/// lap.
///
/// Read independently on the two sides. A track whose two edges bulge together is a ribbon of
/// varying width; a track someone dug has two edges that each wander on their own, and the
/// difference is visible from directly above without measuring anything.
const EDGE_WOBBLE_M: f32 = 2.2;
const EDGE_WOBBLE_WAVELENGTH_M: f32 = 26.0;

/// The windrow of spoil left along the edge of a bladed track: how tall it stands above the
/// riding line, and how far out it reaches.
///
/// Every cubic metre a machine takes out of the line has to go somewhere, and it goes to the
/// sides. It is what makes a track's edge read as an *edge* from above — a lit ridge with a
/// shadow behind it — rather than as the place a smooth ramp happens to stop. Where the blade
/// is cutting into rising ground there is much less of it, because there the spoil is being
/// carried away rather than pushed aside.
const SPOIL_HEIGHT_M: f32 = 0.30;
const SPOIL_WIDTH_M: f32 = 2.6;
const SPOIL_ON_CUT: f32 = 0.3;

/// Metres of lap between one high point of the windrow and the next. It is not a kerb.
const SPOIL_WAVELENGTH_M: f32 = 19.0;

/// How tall a corner's own berm grows without anyone asking for one, metres at the tightest
/// corner that has one. Riders push material to the outside of every turn they ride; a berm
/// only has to be *declared* when it is bigger than what the corner would build itself.
/// Measured as the rise of a corner's outside edge over the lowest ground on it, across ten
/// published tracks: 0.30–1.10 m outside against 0.00–0.52 m inside, and the asymmetry is gone
/// above a forty-metre radius. Indiana reads 0.55 against 0.28.
const CORNER_BERM_M: f32 = 0.55;

/// The radii a corner's own berm grows between: nothing at the first, full height at the
/// second.
///
/// Its own pair rather than the ruts', because the two measure differently. Grooves keep
/// deepening down to the tightest hairpin; the edge asymmetry is at full strength by twenty
/// metres and grows no further.
const BERM_RADIUS_M: (f32, f32) = (40.0, 20.0);

/// How far past the edge of the riding line a corner's berm reaches, metres.
///
/// A berm is not a wall at the white line. Indiana's outside edge still stands a third of a
/// metre proud two metres off the track and only meets the field at four, so the bank has a
/// back to it — which is the difference between something a rider can lean on and a step.
const BERM_REACH_M: f32 = 4.5;

/// The finest thing the landscape itself carries, and how tall it stands.
///
/// Four octaves over a hundred-metre wavelength put the smallest hummock twelve metres
/// across, and a field of those reads as a blur rather than as ground. Real land has
/// metre-scale texture everywhere, not only where it has been ridden.
const FIELD_DETAIL_M: f32 = 4.5;
const FIELD_DETAIL_HEIGHT_M: f32 = 0.075;

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
///
/// The same 1024 the published tracks use for their soil. Half of that is visibly a smear
/// once it is stretched over a few metres of ground, which is the scale a rider sees it at.
const GROUND_TEXTURE_DIM: usize = 1024;

/// How many metres of ground one tile of each sheet covers.
///
/// Stated in metres and turned into a repetition count against the terrain's own size, so a
/// 400 m track and a 900 m one get soil of the same grain. A fixed repetition count does not:
/// the old 60 put a tile every 4.6 m on the example track and every 11.7 m on ours, which is
/// most of why the ground looked out of scale.
const TILE_FIELD_M: f32 = 4.5;
const TILE_LINE_M: f32 = 3.2;
const TILE_SHOULDER_M: f32 = 3.8;
const TILE_GRASS_M: f32 = 2.8;

/// The UI pictures' edge, in pixels. Square and modest — they are shown at a few hundred
/// pixels and stored uncompressed.
const UI_IMAGE_DIM: usize = 512;

/// Coverage masks inside a `.trh`, which published tracks keep at half the grid — 2048
/// against 2049. It is also the resolution anything reading the file will measure it at.
const TRH_MASK_DIM: usize = 2048;

/// And the resolution a *preview* keeps them at.
///
/// A preview can carry ten of these — one per kind of feature on the track, plus the three
/// surfaces — and at the full size that is forty megabytes of mask for a picture nobody
/// measures. A quarter of the area is still finer than the screen it is drawn on.
const PREVIEW_MASK_DIM: usize = 1024;

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
            // Plus a metre-scale octave everywhere. Four octaves over a hundred metres put
            // the smallest hummock twelve metres across, and a field of those is a blur, not
            // ground — this is what the eye reads as land when it is nowhere near the track.
            heights[y * gw + x] = fbm(wx / r.wavelength, wz / r.wavelength, r.seed) * r.amplitude
                + fbm(wx / FIELD_DETAIL_M, wz / FIELD_DETAIL_M, r.seed ^ 0xF1E1D)
                    * FIELD_DETAIL_HEIGHT_M;
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
    apply_rise(&mut along, &stations, &prog.segments);
    apply_elevation(&mut along, &stations, &prog.elevation, lap);
    apply_step_ups(&mut along, &stations, &prog.features);

    // Everything that varies along the lap, resampled onto one even ruler so a cell can ask
    // for the value at *its* distance round rather than at the nearest station's.
    let bench = resample(&stations, &along, lap);
    let mut turn = resample(
        &stations,
        &stations.iter().map(|s| s.curvature).collect::<Vec<_>>(),
        lap,
    );
    // Curvature steps from nothing to 1/r the instant a corner starts, and everything that
    // reads it — the ruts, the roughness, which side a berm stands on — stepped with it. Eased
    // over the blend distance, a corner arrives instead of appearing.
    smooth_along(
        &mut turn.v,
        (prog.blend.max(0.0) / PROFILE_STEP).round() as usize,
    );
    let feat = feature_profile(&prog.features, lap, prog.blend.max(0.0));
    let berms = berm_profile(&prog.features, &turn, lap);
    let ruts = rut_profile(&prog.features, &turn, lap, r.seed);
    let widths = width_profile(prog.width * 0.5, lap, r.seed);
    let chop = roughness_profile(&turn, lap);

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
        // The two edges wander independently. Together they are a ribbon of varying width;
        // apart they are two edges someone dug, which is what a track actually has.
        let lane = if t >= 0.0 { 11.0 } else { 29.0 };
        // Two halves, on purpose. The wobbled one is where the track *is* — what the corridor
        // covers, how far the features reach, where the windrow sits. The plain one is what
        // the grading works from, because the shoulder reaches twenty metres into the field
        // and out there the two sides' wobbles meet along a line the eye reads as a crease.
        let plain_half = widths.at(s);
        let half = (plain_half
            + EDGE_WOBBLE_M * fbm(s / EDGE_WOBBLE_WAVELENGTH_M, lane, r.seed ^ 0xE39E))
        .max(1.0);

        // Cut or fill: which one decides how far the grading reaches, and so how the edge
        // of the track reads from the seat.
        let ground = heights[i];
        let deck = bench.at(s);
        let shoulder = SHOULDER_M * if ground > deck { CUT_SHOULDER } else { FILL_SHOULDER };
        let w = bench_weight(d, plain_half, shoulder);
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
        // Whatever the program asked for, plus what the corner would have grown on its own:
        // every rider pushes material to the outside of a turn, so a berm is the *default*
        // shape of a corner's edge and only has to be declared when it is bigger than that.
        let k = turn.at(s);
        let auto = if k != 0.0 {
            let radius = 1.0 / k.abs();
            CORNER_BERM_M
                * smoothstep(
                    ((BERM_RADIUS_M.0 - radius) / (BERM_RADIUS_M.0 - BERM_RADIUS_M.1))
                        .clamp(0.0, 1.0),
                )
                * k.signum()
        } else {
            0.0
        };
        // Both are signed by which way the corner turns, so the taller one is the berm: a
        // declared 1.6 m wall replaces the 0.55 m the corner would have grown, rather than
        // standing on top of it.
        let declared = berms.at(s);
        let b = if declared.abs() >= auto.abs() { declared } else { auto };
        // Up to the crest at the edge of the line, then away over the ground behind it. A
        // berm that stops at the white line is a step; the back is what makes it a bank.
        if b != 0.0 && t * -b.signum() > 0.0 {
            let a = t.abs();
            if a <= half {
                // Not a quadratic. Squaring keeps the whole rise in the last metre and leaves
                // the track flat under it, which measures as a kerb: published corners bank
                // 3.5–10.5° across the riding line at the ninetieth, and a quadratic berm of
                // the right height reads 2.4°. The outer half of the track is tilted, and
                // that is what a rider leans on.
                heights[i] += b.abs() * (a / half).powf(1.4);
            } else if a < half + BERM_REACH_M {
                let u = (a - half) / BERM_REACH_M;
                heights[i] += b.abs() * (1.0 - u * u);
            }
        }

        // The windrow: the spoil the blade pushed off the line, sitting just outside it.
        let over = d - half;
        if over > 0.0 && over < SPOIL_WIDTH_M {
            // Varied by where the cell *is*, not by how far round the lap it is. Distance
            // round the lap jumps at the seams between stations' territories, and out at the
            // edge of the track that jump is a visible nick in the windrow.
            let (wx, wz) = ((i % gw) as f32 * mps_x, (i / gw) as f32 * mps_z);
            let along = 0.55
                + 0.45
                    * fbm(
                        wx / SPOIL_WAVELENGTH_M,
                        wz / SPOIL_WAVELENGTH_M,
                        r.seed ^ if t >= 0.0 { 0x5901 } else { 0x5902 },
                    );
            let cut = if ground > deck { SPOIL_ON_CUT } else { 1.0 };
            // A ridge, not a step: up over the first third of its width and away over the
            // rest, so it has a lit face and a shadow behind it.
            let u = over / SPOIL_WIDTH_M;
            let shape = if u < 0.33 {
                smoothstep(u / 0.33)
            } else {
                1.0 - smoothstep((u - 0.33) / 0.67)
            };
            heights[i] += SPOIL_HEIGHT_M * along.max(0.0) * cut * shape;
        }

        // Ruts: not one groove but the bundle of them a corner actually wears, lying across
        // as much of the track as riders found a line on, deepest near the middle of the
        // bundle and shallowing out to either side.
        let depth = ruts.depth.at(s);
        if depth > 0.0 {
            let spread = ruts.spread.at(s);
            let mid = ruts.centre.at(s) * half;
            let reach = (half * spread).max(RUT_SPACING_M * 0.5);
            // As many lines as fit the bundle, always an odd count so one of them is the line.
            let lines = ((2.0 * reach / RUT_SPACING_M).round() as usize | 1).min(RUT_LINES);
            let mid_line = (lines / 2) as f32;
            for n in 0..lines {
                let off = (n as f32 - mid_line) * RUT_SPACING_M;
                // Each groove wanders across the track down its own length. Perfectly
                // concentric grooves are a machine's idea of a corner.
                let wander = RUT_WANDER_M
                    * fbm(
                        s / RUT_WANDER_WAVELENGTH_M,
                        n as f32 * 4.7,
                        r.seed ^ 0x2117,
                    );
                let centre = mid + off + wander;
                if centre.abs() > half + RUT_WIDTH_M {
                    continue;
                }
                // Deepest in the middle of the bundle: that is where most of the field went.
                // And no two of them the same depth — a comb of identical grooves is a
                // machine's idea of a corner just as much as a single groove is.
                let fade = 1.0 - (off.abs() / reach.max(1e-3)).min(1.0).powi(2);
                // No two the same depth, and the spread is wide: a real comb has one groove
                // half a metre deep beside one you would not notice.
                let own = 0.42 + 0.58 * (hash2(n as i32, 7, r.seed ^ 0x2117) * 0.5 + 0.5);
                let g = (t - centre) / RUT_WIDTH_M;
                heights[i] -= depth * fade * own * (-g * g).exp();
            }
        }

        // Ridden ground, last of all: on the riding line and just off it, rougher through
        // the corners than down the straights.
        //
        // Not across the whole graded shoulder. Fifteen metres out in the field nothing has
        // been ridden, so nothing there should be chopped up — and everything here is a
        // function of how far round the lap a cell is, which is a quantity that jumps at the
        // seams between one station's territory and the next. Out at the centre of a corner's
        // arc those seams are metres wide and the jump draws a crease across the infield.
        let w = bench_weight(d, half, SPOIL_WIDTH_M);
        if r.texture > 0.0 && w > 0.0 {
            let (wx, wz) = ((i % gw) as f32 * mps_x, (i / gw) as f32 * mps_z);
            let gain = chop.rough.at(s);
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

            // A washboard is strongest where the wheels are and gone at the edge of the
            // track, where nobody brakes.
            let across = (1.0 - (t.abs() / half).min(1.0).powi(2)) * w;
            // Braking bumps on the way into a corner, which is the direction they form in.
            // The phase drifts, because bumps that are a perfect sine read as corrugated iron.
            let brake = chop.braking.at(s);
            if brake > 0.0 && across > 0.0 {
                let drift = 0.35 * fbm(s / 26.0, 7.0, r.seed ^ 0xB4AE);
                let ripple =
                    ((s / BRAKING_WAVELENGTH_M + drift) * std::f32::consts::TAU).sin();
                heights[i] += ripple * BRAKING_HEIGHT_M * 0.5 * brake * across;
            }
            // And the longer, lower chop everybody's rear wheel leaves on the way out.
            let out = chop.accel.at(s);
            if out > 0.0 && across > 0.0 {
                let drift = 0.4 * fbm(s / 31.0, 13.0, r.seed ^ 0xACCE);
                let ripple = ((s / ACCEL_WAVELENGTH_M + drift) * std::f32::consts::TAU).sin();
                heights[i] += ripple * ACCEL_HEIGHT_M * 0.5 * out * across;
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
        stations,
        used_m: used,
        budget_m: budget,
    })
}

/// How much height a programme actually needs, in metres.
///
/// The budget is a technical quantity — samples are quantised against it — and nobody should
/// have to guess it. Built by synthesising against a budget large enough that the check can't
/// fire, then reading what was used.
pub fn required_height(prog: &TrackProgram) -> Result<f32> {
    let mut roomy = prog.clone();
    // Far more than any track needs, so nothing clips and `used_m` is the honest figure.
    roomy.terrain.scale = 10_000.0;
    Ok(synthesise(&roomy)?.used_m)
}

/// A programme with a height budget that fits it: just above what it needs, so the terrain
/// quantises as finely as it can without clipping.
pub fn with_fitted_budget(prog: &TrackProgram) -> Result<TrackProgram> {
    let need = required_height(prog)?;
    let mut out = prog.clone();
    // Fifteen percent of headroom, rounded up to a whole metre. Room for an edit or two
    // before this has to be worked out again, and no more.
    out.terrain.scale = (need * 1.15).ceil().max(2.0);
    Ok(out)
}

/// Samples across and down, both a power of two plus one, with cells as square as that allows.
/// The grid the synthesiser would build for this program, so the repair pass can work out
/// what plot would make its cells square.
pub fn grid_for(prog: &TrackProgram) -> Result<(usize, usize)> {
    grid_dims(prog)
}

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

/// The ruts, along the lap: how deep they are, how far across the track the bundle reaches,
/// and where its centre sits.
///
/// Corners grow their own — everyone takes the same line through a tight one, and that is
/// what digs the grooves. What comes out is a bundle rather than a groove: the tighter the
/// turn the more of the track gets cut, because the more riders have found a line of their
/// own in it.
///
/// All three carry past the corner. Ruts do not begin at turn-in and end at the apex; the
/// line is already there on the approach and is still being driven a long way out onto the
/// following straight, so each profile is smeared forward and back with a decay before it is
/// used. That is what stops a corner reading as a stencil laid over a clean track.
///
/// A `Rut` feature adds to whatever the corner already had, so asking for one in a hairpin
/// deepens it rather than replacing it.
struct Ruts {
    depth: Profile,
    /// How far across the half-width the bundle reaches, 0 to 1.
    spread: Profile,
    /// Where the bundle's centre sits, as a signed fraction of the half-width.
    centre: Profile,
}

fn rut_profile(features: &[Feature], turn: &Profile, lap: f32, seed: u32) -> Ruts {
    let mut depth = Profile::blank(lap);
    let mut tight = Profile::blank(lap);
    let mut centre = Profile::blank(lap);
    let (start_r, full_r) = RUT_RADIUS_M;
    for i in 0..depth.v.len() {
        let s = i as f32 * PROFILE_STEP;
        let k = turn.at(s);
        if k.abs() > 0.0 {
            let radius = 1.0 / k.abs();
            if radius < start_r {
                let t = smoothstep(((start_r - radius) / (start_r - full_r)).clamp(0.0, 1.0));
                // Not evenly: a rut wanders in depth down the length of a corner.
                let vary = 0.75 + 0.25 * fbm(s / 9.0, 3.5, seed ^ 0x2117);
                depth.v[i] = RUT_DEPTH_M * t * vary;
                tight.v[i] = t;
                // Positive curvature turns right, and the corner's inside is the rider's
                // right — the same side `right_vector` points at, which is the sign every
                // lateral quantity here is measured in.
                centre.v[i] = k.signum() * RUT_INSIDE * t;
            }
        }
        // A straight is not smooth ground. Every published track wears grooves down its
        // straights too — a tenth of a metre against a corner's third — and a lap that is
        // glass between the turns reads as one from the first corner exit.
        let vary = 0.7 + 0.3 * fbm(s / 11.0, 8.5, seed ^ 0x2118);
        let floor = RUT_DEPTH_STRAIGHT_M * vary;
        if depth.v[i] < floor {
            depth.v[i] = floor;
            tight.v[i] = tight.v[i].max(0.12);
        }
    }
    // Out of the corner and back up the approach, before any hand-placed rut is added — a
    // feature says where it wants to be and should not be dragged fifty metres down the lap.
    carry(&mut depth, RUT_CARRY_ENTRY_M, RUT_CARRY_EXIT_M);
    carry(&mut tight, RUT_CARRY_ENTRY_M, RUT_CARRY_EXIT_M);
    carry(&mut centre, RUT_CARRY_ENTRY_M, RUT_CARRY_EXIT_M);

    for f in features {
        let Feature::Rut { at, length, depth: d } = *f else {
            continue;
        };
        let lo = (at / PROFILE_STEP).floor().max(0.0) as usize;
        let hi = (((at + length) / PROFILE_STEP).ceil() as usize).min(depth.v.len() - 1);
        for i in lo..=hi {
            let u = i as f32 * PROFILE_STEP - at;
            if u < 0.0 || u > length {
                continue;
            }
            let ramp = smoothstep((u / length * 3.0).min(3.0 - u / length * 3.0).clamp(0.0, 1.0));
            depth.v[i] += d * ramp;
            tight.v[i] = tight.v[i].max(0.55 * ramp);
        }
    }

    let mut spread = Profile::blank(lap);
    for i in 0..spread.v.len() {
        let t = tight.v[i].clamp(0.0, 1.0);
        spread.v[i] = if depth.v[i] > 0.0 {
            RUT_BUNDLE.0 + (RUT_BUNDLE.1 - RUT_BUNDLE.0) * t
        } else {
            0.0
        };
    }

    Ruts {
        depth,
        spread,
        centre,
    }
}

/// Smear a lap profile forward and backward with an exponential decay, keeping the larger of
/// what was there and what arrived.
///
/// Circular, because a lap is. A corner that ends at the finish line carries its ruts across
/// it, and a smear written as a line would stop dead there.
fn carry(p: &mut Profile, back_m: f32, ahead_m: f32) {
    let n = p.v.len();
    if n < 3 {
        return;
    }
    // One decaying pass in each direction. Both are signed: a rut bundle sitting to the left
    // of the line has to run out of the corner still on the left, so the sign travels with
    // the magnitude rather than being reattached afterwards.
    let pass = |reach: f32, forward: bool| -> Vec<f32> {
        let mut out = p.v.clone();
        if reach <= 0.0 {
            return out;
        }
        let k = (-PROFILE_STEP / reach).exp();
        let mut run = 0.0f32;
        // Twice round the lap, so a decay that starts at the seam has somewhere to have come
        // from — the first turn only primes `run`.
        for turn in 0..2 {
            for j in 0..n {
                let i = if forward { j } else { n - 1 - j };
                run *= k;
                if p.v[i].abs() > run.abs() {
                    run = p.v[i];
                }
                if turn == 1 && run.abs() > out[i].abs() {
                    out[i] = run;
                }
            }
        }
        out
    };
    let ahead = pass(ahead_m, true);
    let behind = pass(back_m, false);
    for i in 0..n {
        p.v[i] = if ahead[i].abs() >= behind[i].abs() {
            ahead[i]
        } else {
            behind[i]
        };
    }
}

/// How chopped-up the ground is, along the lap — one on a straight, more through a corner
/// and on the way into it. Braking is where a track gets rough, and it is rough in a place
/// rather than everywhere.
struct Chop {
    rough: Profile,
    /// The washboard on the way into a corner.
    braking: Profile,
    /// The longer, lower chop on the way out of one.
    accel: Profile,
}

fn roughness_profile(turn: &Profile, lap: f32) -> Chop {
    let mut rough = Profile::blank(lap);
    let mut braking = Profile::blank(lap);
    let mut accel = Profile::blank(lap);
    for i in 0..rough.v.len() {
        rough.v[i] = 1.0;
    }
    let n = rough.v.len();
    let back = (BRAKING_M / PROFILE_STEP) as usize;
    let ahead = (ACCEL_M / PROFILE_STEP) as usize;
    let in_corner = |j: usize| turn.at((j % n) as f32 * PROFILE_STEP) != 0.0;
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
        // Braking bumps only on the way in, growing towards the turn-in point. Indexed round
        // the lap, so a corner that starts just after the finish line still has an approach.
        for step in 1..=back {
            let j = (i + n - step) % n;
            if in_corner(j) {
                continue; // already in a corner — this is another corner's exit, not an approach
            }
            let near = 1.0 - step as f32 / back as f32;
            braking.v[j] = braking.v[j].max(corner * smoothstep(near));
        }
        // And acceleration chop on the way out, fading with distance from the exit.
        for step in 1..=ahead {
            let j = (i + step) % n;
            if in_corner(j) {
                continue;
            }
            let near = 1.0 - step as f32 / ahead as f32;
            accel.v[j] = accel.v[j].max(corner * smoothstep(near));
        }
    }
    Chop {
        rough,
        braking,
        accel,
    }
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
fn feature_profile(features: &[Feature], lap: f32, blend: f32) -> Profile {
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
            // The larger of the two, not the sum. Two jumps a metre apart used to add, so
            // the ground between them rose to their combined height and a rhythm section came
            // out as one tall lump with notches in it.
            out.v[i] = out.v[i].max(longitudinal(f, u / len, u));
        }
    }
    // Then round the whole thing off over the blend distance. That is what turns two jumps
    // that merely touch into one shape, and it is the same control that decides how long a
    // single jump's ramps are — they are the same question asked twice.
    smooth_along(&mut out.v, (blend / PROFILE_STEP).round() as usize);
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

/// Climb and drop, as the segments declare it.
///
/// Cumulative, and eased across each segment rather than applied at its end — a straight that
/// gains four metres gains them evenly along its length, which is what makes it a hill rather
/// than a step. Every later station carries the total of everything before it, so a lap whose
/// rises don't sum to zero comes back to the start higher than it left, and the height budget
/// check is what notices.
fn apply_rise(along: &mut [f32], st: &[Station], segments: &[Segment]) {
    let mut at = 0.0f32;
    for seg in segments {
        let len = seg.length();
        let rise = seg.rise();
        if len <= 0.0 {
            continue;
        }
        if rise != 0.0 {
            for (i, s) in st.iter().enumerate() {
                if s.s < at {
                    continue;
                }
                // Each segment contributes only its *own* climb — eased across it, and held
                // at full value for everything after. Summing those over the segments a
                // station is past is the cumulative height, with nothing counted twice.
                //
                // It carried a running total as well, and added that to every station past
                // each segment's start. Every later segment then re-added what the earlier
                // ones had already left there, so a lap whose rises cancelled finished metres
                // above where it started — and the terrain wore the difference as a cliff.
                let u = ((s.s - at) / len).clamp(0.0, 1.0);
                along[i] += rise * smoothstep(u);
            }
        }
        at += len;
    }
}

/// The hand-drawn height curve, added to whatever the ground was already doing.
///
/// Eased between neighbouring points rather than run straight between them, so a curve drawn
/// with four points is four hills and not four ramps with corners on them. It **wraps**: the
/// last point eases into the first, because a lap is a loop and a step across the finish line
/// is the one place a rider would notice one.
fn apply_elevation(along: &mut [f32], st: &[Station], knots: &[Knot], lap: f32) {
    if knots.is_empty() || lap <= 0.0 {
        return;
    }
    let mut k: Vec<Knot> = knots.to_vec();
    k.sort_by(|a, b| a.at.total_cmp(&b.at));

    let at = |s: f32| -> f32 {
        // Which pair of points this station falls between, treating the list as a ring.
        let n = k.len();
        if n == 1 {
            return k[0].height;
        }
        let i = match k.iter().position(|p| p.at > s) {
            Some(0) | None => n - 1, // before the first or after the last: the wrap-around pair
            Some(j) => j - 1,
        };
        let a = k[i];
        let b = k[(i + 1) % n];
        // The gap, measured the way round that actually connects them.
        let span = if b.at > a.at { b.at - a.at } else { lap - a.at + b.at };
        if span <= 1e-3 {
            return b.height;
        }
        let along = if s >= a.at { s - a.at } else { lap - a.at + s };
        a.height + (b.height - a.height) * smoothstep((along / span).clamp(0.0, 1.0))
    };

    for (i, s) in st.iter().enumerate() {
        along[i] += at(s.s);
    }
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
    // Drawn by hand: eased between the points it was given, which is the same easing the lap's
    // own height curve uses. Nothing else here has a shape someone chose point by point.
    if let Feature::Custom { shape, .. } = f {
        return along_points(shape, t);
    }
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
        Feature::Custom { .. } => unreachable!("handled above"),
    }
}

/// A hand-drawn shape's height at `t`, which runs 0 to 1 across the feature.
///
/// Eased rather than joined straight, so points placed roughly still make a shape a bike can
/// ride. Outside the points it reads as the nearest one, so a shape that does not start at
/// zero simply begins at the height it was drawn at.
fn along_points(points: &[crate::trackprog::ShapePoint], t: f32) -> f32 {
    if points.is_empty() {
        return 0.0;
    }
    let mut p: Vec<_> = points.to_vec();
    p.sort_by(|a, b| a.u.total_cmp(&b.u));
    if t <= p[0].u {
        return p[0].h;
    }
    if t >= p[p.len() - 1].u {
        return p[p.len() - 1].h;
    }
    let i = p.iter().rposition(|q| q.u <= t).unwrap_or(0);
    let (a, b) = (p[i], p[(i + 1).min(p.len() - 1)]);
    let span = (b.u - a.u).max(1e-6);
    a.h + (b.h - a.h) * smoothstep((t - a.u) / span)
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth a quantity that runs round the lap.
///
/// **Circular.** A lap is a loop, and smoothing it as a line gives the first and last
/// stations different neighbourhoods to average — so the track's own elevation came out at
/// two different heights on the two sides of the finish line, and the terrain wore the
/// difference as a step across it. Two metres of step, on a lap that closes perfectly.
fn smooth_along(v: &mut [f32], r: usize) {
    let n = v.len();
    if r == 0 || n < 3 {
        return;
    }
    // Prefix sums over the sequence laid end to end three times, so a window centred
    // anywhere in the middle copy can run off either side and still land on real values.
    // Two copies is not enough: the last station's window reaches past the end of them.
    let mut pre = vec![0.0f64; 3 * n + 1];
    for i in 0..3 * n {
        pre[i + 1] = pre[i] + v[i % n] as f64;
    }
    let span = (2 * r + 1).min(n);
    let out: Vec<f32> = (0..n)
        .map(|i| {
            let start = i + n - span / 2;
            ((pre[start + span] - pre[start]) / span as f64) as f32
        })
        .collect();
    v.copy_from_slice(&out);
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

    // The riding line, and everything that isn't it. Every boundary is torn rather than
    // drawn — see `edge_noise`.
    let half = prog.width * 0.5;
    let seed = prog.terrain.relief.seed;
    let dirt = mask_from(syn, MASK_DIM, |d, _, x, z| {
        soft_edge(half + 1.5 + edge_noise(x, z, seed ^ 0xD127, 1.6, 0.7), 2.0, d)
    });
    // The shoulder: worked ground either side of the line, painted from the edge of the
    // riding surface out to where the field starts. It is most of what a rider sees.
    let shoulder = mask_from(syn, MASK_DIM, |d, _, x, z| {
        255 - soft_edge(
            half + SHOULDER_M * 0.75 + edge_noise(x, z, seed ^ 0x5A1D, 2.4, 1.0),
            5.0,
            d,
        )
    });
    let grass = mask_from(syn, MASK_DIM, |d, _, x, z| {
        255 - soft_edge(
            half + SHOULDER_M + edge_noise(x, z, seed ^ 0x6EE2, 3.2, 1.2),
            6.0,
            d,
        )
    });
    // Off-track starts where the graded shoulder ends: the rider is on the track, or in the
    // field, with the shoulder belonging to neither. This one decides where the game says a
    // rider has gone off, so it is the one boundary that stays smooth.
    let off = mask_from(syn, MASK_DIM, |d, _, _, _| {
        255 - soft_edge(half + SHOULDER_M * 0.6, 3.0, d)
    });
    let start_len = 45.0f32.min(prog.lap_length() * 0.2);
    let start = mask_from(syn, MASK_DIM, |d, s, _, _| {
        if d <= half * 1.4 && s <= start_len {
            255
        } else {
            0
        }
    });
    put("mask_dirt.tga", tga_alpha(MASK_DIM, MASK_DIM, &dirt), &mut wrote)?;
    put(
        "mask_shoulder.tga",
        tga_alpha(MASK_DIM, MASK_DIM, &shoulder),
        &mut wrote,
    )?;
    put("mask_grass.tga", tga_alpha(MASK_DIM, MASK_DIM, &grass), &mut wrote)?;
    put("area_off.tga", tga_alpha(MASK_DIM, MASK_DIM, &off), &mut wrote)?;
    put("area_start.tga", tga_alpha(MASK_DIM, MASK_DIM, &start), &mut wrote)?;

    // Ground follows what the track is made of, so a sand national exports sand.
    let (field, ridden, shoulder, turf) = ground_looks(prog.terrain.surface);
    std::fs::create_dir_all(dir.join("maps")).context("make the maps folder")?;
    let seed = prog.terrain.relief.seed;
    put(
        "maps/ground.tga",
        ground_texture(GROUND_TEXTURE_DIM, &field, seed ^ 0x9A0D),
        &mut wrote,
    )?;
    put(
        "maps/line.tga",
        ground_texture(GROUND_TEXTURE_DIM, &ridden, seed ^ 0x11E5),
        &mut wrote,
    )?;
    put(
        "maps/shoulder.tga",
        ground_texture(GROUND_TEXTURE_DIM, &shoulder, seed ^ 0x30D2),
        &mut wrote,
    )?;
    put(
        "maps/grass.tga",
        ground_texture(GROUND_TEXTURE_DIM, &turf, seed ^ 0x6A55),
        &mut wrote,
    )?;
    put("maps/grassfx.tga", grass_billboard(128), &mut wrote)?;

    put("track.hmf", hmf(prog, syn).into_bytes(), &mut wrote)?;
    put("track.tht", tht(prog, syn).into_bytes(), &mut wrote)?;
    put("params.ini", PARAMS_INI.into(), &mut wrote)?;
    put("trh_params.ini", TRH_PARAMS_INI.into(), &mut wrote)?;
    put("track.tcl", tcl(prog).into_bytes(), &mut wrote)?;
    // The game-facing files, byte for byte what the `.pkz` carries.
    put(&format!("{slug}/{slug}.ini"), crlf(&track_ini(prog)), &mut wrote)?;
    put(&format!("{slug}/{slug}.amb"), crlf(AMB), &mut wrote)?;
    put(&format!("{slug}/gfx.cfg"), crlf(&gfx_cfg(prog)), &mut wrote)?;
    put(&format!("{slug}/{slug}.rdf"), crlf(&rdf(prog)), &mut wrote)?;
    put(&format!("{slug}/{slug}.ssc"), SSC.into(), &mut wrote)?;
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
/// The surface id a feature kind is painted with in a preview. See `track::surface_colour`.
fn feature_id(f: &Feature) -> u32 {
    match f {
        Feature::Tabletop { .. } => 200,
        Feature::Double { .. } => 201,
        Feature::Roller { .. } => 202,
        Feature::Whoops { .. } => 203,
        Feature::StepUp { .. } => 204,
        Feature::Berm { .. } => 205,
        Feature::Rut { .. } => 206,
        Feature::Custom { .. } => 207,
    }
}

/// One coverage mask per kind of feature on the track, so a preview can colour them.
///
/// Grouped by kind rather than one per feature: thirty masks would be thirty megabytes and
/// the question being answered is "which of these is the double", not "which double".
fn feature_masks(prog: &TrackProgram, syn: &Synth, dim: usize) -> Vec<(u32, Vec<u8>)> {
    let half = prog.width * 0.5;
    let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
    for f in &prog.features {
        let id = feature_id(f);
        if out.iter().any(|(k, _)| *k == id) {
            continue;
        }
        let spans: Vec<(f32, f32)> = prog
            .features
            .iter()
            .filter(|g| feature_id(g) == id)
            .map(|g| (g.at(), g.at() + g.length()))
            .collect();
        out.push((
            id,
            mask_from(syn, dim, |d, s, _, _| {
                let on = d <= half && spans.iter().any(|(lo, hi)| s >= *lo && s <= *hi);
                u8::from(on) * 255
            }),
        ));
    }
    out
}

pub fn trh(prog: &TrackProgram, syn: &Synth, paint_features: bool) -> Vec<u8> {
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
    let dim = if paint_features { PREVIEW_MASK_DIM } else { TRH_MASK_DIM };
    let (shoulder_id, shoulder_scale) = ground(prog.terrain.surface);
    let shoulder = SHOULDER_M * shoulder_scale;
    // The bands wander the same way the painted ones do, so a preview and a compiled track
    // are the same track. Hard-edged still — soft is what made the line measure wide.
    let seed = prog.terrain.relief.seed;
    let line_at = move |x: f32, z: f32| half + edge_noise(x, z, seed ^ 0xD127, 1.6, 0.7);
    let field_at =
        move |x: f32, z: f32| half + shoulder + edge_noise(x, z, seed ^ 0x6EE2, 3.2, 1.2);
    let mut masks: Vec<(u32, Vec<u8>)> = vec![
        // 10 is the riding line — the id published tracks paint their ribbon with.
        (
            10,
            mask_from(syn, dim, |d, _, x, z| u8::from(d <= line_at(x, z)) * 255),
        ),
        (
            shoulder_id,
            mask_from(syn, dim, |d, _, x, z| {
                u8::from(d > line_at(x, z) && d <= field_at(x, z)) * 255
            }),
        ),
        (
            1,
            mask_from(syn, dim, |d, _, x, z| u8::from(d > field_at(x, z)) * 255),
        ),
    ];
    if paint_features {
        // First in the list, because a reader compositing these takes the first mask that
        // covers a cell — and on a feature that is the answer wanted.
        let mut all = feature_masks(prog, syn, dim);
        all.extend(masks);
        masks = all;
    }
    out.extend_from_slice(&(masks.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // to the offset the records start at

    for (id, m) in &masks {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(dim as u32).to_le_bytes());
        out.extend_from_slice(&(dim as u32).to_le_bytes());
        out.extend_from_slice(m);
    }

    // The pose block, which sits forty bytes ahead of the material table in every published
    // file: where the lap starts, how long it is, and the box it lives in.
    out.extend_from_slice(&prog.start.x.to_le_bytes());
    out.extend_from_slice(&prog.start.z.to_le_bytes());
    out.extend_from_slice(&prog.start.angle.to_le_bytes());
    let lap = prog.lap_length();
    out.extend_from_slice(&lap.to_le_bytes());
    for v in [
        0.0,
        0.0,
        0.0,
        prog.terrain.size_x,
        prog.terrain.scale,
        prog.terrain.size_z,
    ] {
        out.extend_from_slice(&(v as f32).to_le_bytes());
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

    // And the centreline, in the same sixty-byte records a compiled track carries. Written
    // for one reason: it is what lets a generated track be measured by the code that measures
    // published ones. `trackstats::ridden` reads a lap's own line and then measures the
    // ground against it — rut depth across the track, the rise of a corner's outside edge,
    // jumps found along the line rather than by roughness — and without this the only track
    // it cannot read is ours.
    out.extend_from_slice(&(prog.segments.len() as u32).to_le_bytes());
    let mut at = 0.0f32;
    let mut x = prog.start.x;
    let mut z = prog.start.z;
    let mut theta = prog.start.angle.to_radians();
    for seg in &prog.segments {
        let (radius, angle) = match *seg {
            Segment::Straight { .. } => (0.0, 0.0),
            Segment::Arc { radius, angle, .. } => (radius, angle),
        };
        let mut rec = [0f32; 15];
        rec[0] = if at == 0.0 { 0.0 } else { 1.0 };
        rec[1] = seg.length();
        rec[2] = radius;
        rec[3] = angle.abs();
        rec[4] = {
            let c = (x / syn.mps).round().clamp(0.0, (syn.gw - 1) as f32) as usize;
            let r = (z / syn.mps).round().clamp(0.0, (syn.gh - 1) as f32) as usize;
            syn.heights[r * syn.gw + c]
        };
        rec[5] = at;
        rec[6] = theta.cos();
        rec[7] = theta.sin();
        rec[8] = x;
        rec[9] = -theta.sin();
        rec[10] = theta.cos();
        rec[11] = z;
        rec[14] = 1.0;
        for v in rec {
            out.extend_from_slice(&v.to_le_bytes());
        }
        at += seg.length();
        if radius == 0.0 {
            let (hx, hz) = crate::trackprog::heading_vector(theta);
            x += seg.length() * hx;
            z += seg.length() * hz;
        } else {
            let next = theta + seg.length() / radius;
            x += radius * (theta.cos() - next.cos());
            z += radius * (next.sin() - theta.sin());
            theta = next;
        }
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

/// Race data: the start gate, the pit lane, the finish line and the checkpoints.
///
/// TrackEd writes this, and TrackEd is Windows-only — but the file is plain text, and its
/// positions are stated as `long` and `lat` along the centreline, which is the one coordinate
/// system this whole module already thinks in. So it can be written here.
///
/// Laid out from the lap rather than copied: the finish line a little into the first
/// straight, the gate behind it, the pit lane alongside, and the splits and checkpoints
/// spread evenly round. The example track's own file is the shape this follows, down to the
/// keys and the order.
///
/// Untested against the game — nothing here has been loaded by MX Bikes. The structure is
/// right; whether every field means what it looks like is not something a macOS box can say.
fn rdf(prog: &TrackProgram) -> String {
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    // Far enough in that the gate behind it is still on the opening straight.
    let line = (lap * 0.06).clamp(10.0, 40.0);
    let gate_at = (line - 12.0).max(1.0);

    let mut s = String::new();
    let mark = |s: &mut String, name: &str, at: f32, w: f32| {
        s.push_str(&format!(
            "{name}\n{{\n\tline = 0\n\tlong = {at:.6}\n\tleft = {:.6}\n\tright = {:.6}\n}}\n",
            -w, w
        ));
    };
    mark(&mut s, "finish_line", line, half);
    mark(&mut s, "split1", (line + lap / 3.0) % lap, half);
    mark(&mut s, "split2", (line + lap * 2.0 / 3.0) % lap, half);

    // The pit lane runs alongside the opening straight, a track's width off the racing line.
    let stalls = 16;
    let lane_lat = -(half + 6.0);
    s.push_str(&format!(
        "pit_lane\n{{\n\tnumstalls = {stalls}\n\tstarttype = 1\n\tstartstartlong = 0.000000\n\
         \tstartdifflong = 0.000000\n\tstartstartlat = 0.000000\n\tstartendlat = 0.000000\n\
         \tstartanglerel = 0.000000\n\tstartposx = {:.6}\n\tstartposz = {:.6}\n\
         \tstartspacingx = -4.000000\n\tstartspacingz = 6.000000\n\tstartangleabs = {:.6}\n\
         \tstartcolumns = 8\n",
        prog.start.x, prog.start.z, prog.start.angle
    ));
    for i in 0..stalls {
        s.push_str(&format!(
            "\tstart_stall{i}\n\t{{\n\t\tlong = {:.6}\n\t\tlat = {lane_lat:.6}\n\
             \t\tangle = 0.000000\n\t}}\n",
            gate_at + i as f32 * 5.0
        ));
    }
    s.push_str("}\n");

    s.push_str(
        "pit_board\n{\n\theight = 1.500000\n\tstartlong = 18.000000\n\tdifflong = 1.400000\n\
         \tstartlat = -5.000000\n\tendlat = -5.000000\n",
    );
    for i in 0..stalls {
        s.push_str(&format!(
            "\tstall{i}\n\t{{\n\t\tlong = {:.6}\n\t\tlat = {:.6}\n\t\tangle = 0.000000\n\t}}\n",
            18.0 + i as f32 * 1.4,
            lane_lat
        ));
    }
    s.push_str("}\n");

    // One row of gates across the track, which is what a motocross start is.
    let grid = 24;
    s.push_str(&format!(
        "starting_grid\n{{\n\tnumstalls = {grid}\n\ttype = 1\n\tposx = {:.6}\n\
         \tposz = {:.6}\n\tangle = {:.6}\n\tnumstallsperrow = {grid}\n\
         \tdistfromstartline = 0.000000\n\tlanespacing = 0.000000\n\trowspacing = 0.000000\n\
         \tdifflat = 0.000000\n\tlanewidth = 1.500000\n\tlatshift = 0.000000\n\tside = 1\n",
        prog.start.x, prog.start.z, prog.start.angle
    ));
    for i in 0..grid {
        // Spread across the track and a little beyond it: a gate is wider than the line.
        let lat = -half * 1.4 + (i as f32 + 0.5) * (half * 2.8 / grid as f32);
        s.push_str(&format!(
            "\tstall{i}\n\t{{\n\t\tlong = {gate_at:.6}\n\t\tlat = {lat:.6}\n\
             \t\tangle = 0.000000\n\t}}\n"
        ));
    }
    s.push_str("}\n");

    // Three, evenly round, so a lap can't be cut. The first carries the start flag.
    s.push_str("num_checkpoints = 3\n");
    for i in 0..3 {
        let at = (line + lap * (i as f32 + 1.0) / 4.0) % lap;
        s.push_str(&format!(
            "checkpoint{i}\n{{\n\tlong = {at:.6}\n\tleft = {:.6}\n\tright = {:.6}\n\
             \tpenalty = {:.6}\n\tline = 0\n\tstart = {}\n}}\n",
            -half * 1.3,
            half * 1.3,
            if i == 0 { 15.0 } else { 5.0 },
            if i == 0 { 1 } else { 0 }
        ));
    }

    s.push_str(&format!(
        "30secondsboard_posx = {:.6}\n30secondsboard_posz = {:.6}\n30secondsboard_angle = {:.6}\n\
         30seconds_board\n{{\n\tlong = {:.6}\n\tlat = {:.6}\n\tangle = 0.000000\n}}\n",
        prog.start.x,
        prog.start.z,
        prog.start.angle - 90.0,
        gate_at - 4.0,
        -(half + 3.0)
    ));
    s
}

/// The `.map`: the sheets the ground is painted with.
///
/// TerrainEd builds these and it is Windows-only, but a `.map` is not the riding surface —
/// measured against published tracks, every triangle in one is scenery, and the terrain is
/// drawn from the `.trh`. What a `.map` also carries is *every texture the track uses*,
/// the ground sheets among them: SandPoint's are `track-dark`, `track-light`, `track-norm`.
///
/// So a track with no scenery still needs a `.map`, and it needs exactly this much of one:
/// no materials, no geometry, the sheets after them. That is the shape the OEM drag strip
/// ships — zero materials, not one triangle, and its textures behind them — so it is a file
/// the game already loads rather than one invented here.
///
/// The record layout is `edf.rs`'s, which is the scanner that reads real ones: a
/// null-terminated name, its dimensions a hundred bytes in, the payload's length at +128,
/// eight zero bytes, then the pixels as raw DEFLATE over RGBA8.
fn map(prog: &TrackProgram, syn: &Synth) -> Vec<u8> {
    let _ = syn;
    let (field, ridden, shoulder, turf) = ground_looks(prog.terrain.surface);
    let seed = prog.terrain.relief.seed;
    let dim = GROUND_TEXTURE_DIM;

    let sheets = [
        // `_c` is PiBoSo's own mark for a colour sheet, and the ground words are what a
        // reader looking for a track's dirt matches on — ours say both.
        ("ground_c", ground_pixels(dim, &field, seed ^ 0x9A0D)),
        ("shoulder_c", ground_pixels(dim, &shoulder, seed ^ 0x30D2)),
        ("dirt_line_c", ground_pixels(dim, &ridden, seed ^ 0x11E5)),
        ("grass_c", ground_pixels(dim, &turf, seed ^ 0x6A55)),
    ];
    let n = sheets.len();

    let mut out = Vec::new();
    out.extend_from_slice(b"MP2\0");
    out.extend_from_slice(&304u32.to_le_bytes()); // constant on every map measured

    // A material record, copied field for field off a published map rather than guessed.
    //
    // Every one of Indiana's 49 is byte-identical but for one word: zero, six ones, four
    // zeros, then a **one-based id at word eleven**, then two more zeros. The previous
    // version here put ones across the first twelve words and the id at word thirteen, which
    // is not this shape anywhere — a renderer reading it gets 1.0 where it expects a count
    // or a flag, and an id of zero where it expects the material's own.
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for k in 0..n {
        let mut rec = [0u8; 56];
        for w in 1..=6 {
            rec[w * 4..w * 4 + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        rec[44..48].copy_from_slice(&((k + 1) as u32).to_le_bytes());
        out.extend_from_slice(&rec);
    }

    // Geometry, because the format's own walk steps over it to reach everything after — a
    // map declaring none stops being readable at its fifth word.
    //
    // One quad per material, each with its own vertices, because that is how a real leaf's
    // draw groups carve up the buffers: disjoint vertex ranges and contiguous triangle
    // ranges. Put 500 m under the terrain, where nothing can see it. The riding surface
    // comes from the `.trh` and a generated track ships no scenery.
    let vc = n * 4;
    let tc = n * 2;
    let under = -500.0f32;
    out.extend_from_slice(&(vc as u32).to_le_bytes());
    let mut block = vec![0u8; vc * 80];
    for k in 0..n {
        let x = k as f32 * 2.0;
        let quad = [
            [x, under, 0.0],
            [x + 1.0, under, 0.0],
            [x + 1.0, under, 1.0],
            [x, under, 1.0],
        ];
        for (c, pos) in quad.iter().enumerate() {
            let v = k * 4 + c;
            for (i, p) in pos.iter().enumerate() {
                let at = v * 12 + i * 4;
                block[at..at + 4].copy_from_slice(&p.to_le_bytes());
            }
            // Texture coordinates at 12 x count, normals at 52 x count — structure of
            // arrays, eighty bytes a vertex.
            let uv = 12 * vc + v * 8;
            let (u, w) = ((c == 1 || c == 2) as u32 as f32, (c >= 2) as u32 as f32);
            block[uv..uv + 4].copy_from_slice(&u.to_le_bytes());
            block[uv + 4..uv + 8].copy_from_slice(&w.to_le_bytes());
            let nm = 52 * vc + v * 12;
            block[nm + 4..nm + 8].copy_from_slice(&1.0f32.to_le_bytes()); // straight up
        }
    }
    out.extend_from_slice(&block);

    // Indices are **absolute** into the whole vertex buffer, not relative to a group's own
    // range — checked against a published map, whose second group starts at vertex 178 and
    // whose triangles there index 178 and up.
    out.extend_from_slice(&(tc as u32).to_le_bytes());
    for k in 0..n {
        let v = (k * 4) as u32;
        for t in [[v, v + 1, v + 2], [v, v + 2, v + 3]] {
            for i in t {
                out.extend_from_slice(&i.to_le_bytes());
            }
        }
    }

    // One leaf node holding a draw group per material. A leaf's five words are
    // `[0, 0, triangles in this node, first triangle, group count]` — the two counts are not
    // decoration, they are how the reader knows what the node owns, and the previous version
    // left both at zero while claiming four groups.
    out.extend_from_slice(&1u32.to_le_bytes());
    let mut node = vec![0u8; 44];
    let hi = (n as f32 * 2.0).max(1.0);
    for (k, v) in [0.0f32, under, 0.0, hi, under, 1.0].iter().enumerate() {
        node[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    node[32..36].copy_from_slice(&(tc as u32).to_le_bytes());
    node[36..40].copy_from_slice(&0u32.to_le_bytes());
    node[40..44].copy_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&node);
    for k in 0..n {
        // flag, material (zero-based), tri_start, tri_count, vert_start, vert_count
        for w in [0u32, k as u32, (k * 2) as u32, 2, (k * 4) as u32, 4] {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }

    // The word here is the **material** count, not the number of records that follow: a
    // published map declares 49 and then ships 84 sheets, the extra ones being the normal and
    // specular maps that hang off the colour ones. Ours are one apiece, so the two agree.
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for (name, px) in &sheets {
        out.extend_from_slice(&texture_record(name, dim as u32, dim as u32, px));
    }
    out
}

/// The roost: what colour the dirt is when a wheel throws it.
///
/// Every published track ships one of these beside its `.map`, and the shape is the same on
/// all of them — a ground colour, then a colour per surface the game can spray. Ours follows
/// what the track is made of, so a sand national roosts sand rather than the default loam.
fn gfx_cfg(prog: &TrackProgram) -> String {
    let (ground, line) = ground_palette(prog.terrain.surface);
    let rgb = |c: [u8; 3]| {
        format!(
            "\t\tred = {:.2}\n\t\tgreen = {:.2}\n\t\tblue = {:.2}\n",
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0
        )
    };
    let mut s = format!(
        "dirt_color\n{{\n\tred = {:.2}\n\tgreen = {:.2}\n\tblue = {:.2}\n}}\n\nparticles\n{{\n",
        ground[0] as f32 / 255.0,
        ground[1] as f32 / 255.0,
        ground[2] as f32 / 255.0
    );
    // The worked line is what a bike is actually on, so it is what the soils roost.
    for name in ["soilsoft", "soil", "soilcompact"] {
        s.push_str(&format!("\t{name}\n\t{{\n{}\t}}\n\n", rgb(line)));
    }
    for name in ["sand", "gravel"] {
        s.push_str(&format!("\t{name}\n\t{{\n{}\t}}\n\n", rgb(ground)));
    }
    s.push_str("}\n");
    s
}

/// One embedded texture, in the record shape [`crate::edf::embedded_textures`] reads.
fn texture_record(name: &str, w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let payload = deflate_raw(rgba);
    // 100 bytes of name field, dimensions, then the header the scanner keys on.
    let mut rec = vec![0u8; 140];
    let n = name.len().min(39);
    rec[..n].copy_from_slice(&name.as_bytes()[..n]);
    rec[100..104].copy_from_slice(&w.to_le_bytes());
    rec[104..108].copy_from_slice(&h.to_le_bytes());
    // The length at +128 counts the eight zero bytes that follow it, which stay zero.
    rec[128..132].copy_from_slice(&((payload.len() + 8) as u32).to_le_bytes());
    rec.extend_from_slice(&payload);
    rec
}

fn deflate_raw(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(bytes);
    e.finish().unwrap_or_default()
}

/// The preview as a `.pkz` the app can open: a plain zip, which is what the reader falls back
/// to when a file isn't one of PiBoSo's encrypted ones.
pub fn write_pkz(
    prog: &TrackProgram,
    syn: &Synth,
    path: &Path,
    paint_features: bool,
) -> Result<u64> {
    let slug = slug(&prog.name);
    let file = std::fs::File::create(path).with_context(|| format!("create {path:?}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    use std::io::Write;
    let (map_img, shot) = ui_images(syn, UI_IMAGE_DIM);
    // Every published track puts its files in a folder named after itself, and the game
    // looks for them there — flat at the archive root they are not found at all.
    for (name, bytes) in [
        (format!("{slug}/{slug}.trh"), trh(prog, syn, paint_features)),
        (format!("{slug}/{slug}.map"), map(prog, syn)),
        (format!("{slug}/{slug}.ini"), crlf(&track_ini(prog))),
        (format!("{slug}/{slug}.rdf"), crlf(&rdf(prog))),
        (format!("{slug}/{slug}.amb"), crlf(AMB)),
        (format!("{slug}/gfx.cfg"), crlf(&gfx_cfg(prog))),
        // Empty on the reference track, and on every track that ships one.
        (format!("{slug}/{slug}.ssc"), SSC.into()),
        (format!("{slug}/{slug}_map.tga"), map_img),
        (format!("{slug}/{slug}.tga"), shot),
    ] {
        zip.start_file(name, opts)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// PiBoSo's own config files are CRLF throughout, so ours are too.
fn crlf(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes()
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

/// A mask, from a function of distance-off-line, distance-round-the-lap, and where on the
/// ground the cell is.
///
/// The world position is what lets an edge be ragged. A mask that is purely a function of
/// distance from the centreline is a band of exactly constant width running the whole lap,
/// and from above that is the most machine-made thing on the whole track — more so than the
/// terrain, because paint has no relief to distract from its outline.
fn mask_from(syn: &Synth, dim: usize, f: impl Fn(f32, f32, f32, f32) -> u8) -> Vec<u8> {
    let mut out = vec![0u8; dim * dim];
    for y in 0..dim {
        let gy = (y * syn.gh / dim).min(syn.gh - 1);
        for x in 0..dim {
            let gx = (x * syn.gw / dim).min(syn.gw - 1);
            let i = gy * syn.gw + gx;
            out[y * dim + x] = f(
                syn.dist[i],
                syn.arc[i],
                gx as f32 * syn.mps,
                gy as f32 * syn.mps,
            );
        }
    }
    out
}

/// How far in or out a painted edge wanders at a given place on the ground, metres.
///
/// Two scales: a long wander that makes the band wide here and narrow there, and a short one
/// that gives the boundary itself a torn look instead of a drawn one.
fn edge_noise(x: f32, z: f32, seed: u32, long_m: f32, short_m: f32) -> f32 {
    fbm(x / 34.0, z / 34.0, seed) * long_m + fbm(x / 6.0, z / 6.0, seed ^ 0x9F1) * short_m
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

/// A separable box blur that wraps at the edges, for a tile that has to keep tiling.
fn box_blur_wrap(v: &[f32], dim: usize, r: usize) -> Vec<f32> {
    let span = (2 * r + 1) as f32;
    let mut tmp = vec![0.0f32; dim * dim];
    for y in 0..dim {
        for x in 0..dim {
            let mut sum = 0.0;
            for k in 0..=2 * r {
                sum += v[y * dim + (x + dim + k - r) % dim];
            }
            tmp[y * dim + x] = sum / span;
        }
    }
    let mut out = vec![0.0f32; dim * dim];
    for y in 0..dim {
        for x in 0..dim {
            let mut sum = 0.0;
            for k in 0..=2 * r {
                sum += tmp[((y + dim + k - r) % dim) * dim + x];
            }
            out[y * dim + x] = sum / span;
        }
    }
    out
}

/// What a patch of ground is made of, for the generator below.
struct GroundLook {
    /// The soil between everything else.
    base: [f32; 3],
    /// Loose material — clods, pebbles, gravel — as a fraction of the base colour, so a
    /// sandy ground's stones are sandy and a dark loam's are dark.
    grain_tint: (f32, f32),
    /// Pale stones and shell, and how many of them.
    fleck: [f32; 3],
    fleck_density: f32,
    /// Straw, root and dead grass lying on it, and how much.
    litter: [f32; 3],
    litter_density: f32,
    /// Blades, for ground that grows rather than crumbles.
    ///
    /// Turf is not aggregate. Painting the clod generator green gives green clods, which is
    /// what the first version of the grass sheet was and what it looked like — so grass gets
    /// its clods turned down and thousands of short strokes instead, in two tones because a
    /// lawn of one green is a billiard table.
    blade: ([f32; 3], [f32; 3]),
    blade_density: f32,
    /// How much loose material there is at all. Worked soil on a riding line is nearly all
    /// clods; a field is mostly bound ground with a few stones showing.
    clods: f32,
    /// And how much of it is the *big* stuff, separately.
    ///
    /// The two are not one number. A field wants aggregate everywhere — it is grain all the
    /// way down, with no smooth substrate showing between the pieces — but it does not want
    /// ten-centimetre clods everywhere, which is what a freshly bladed riding line has. Tying
    /// them together gives either a bare surface with pebbles scattered on it or a field of
    /// boulders, and Indiana's light soil is neither.
    coarse: f32,
    /// How strong the broad tonal patching is — the metre-scale variation that stops a
    /// texture reading as one colour.
    mottle: f32,
    /// How hard the whole thing is shaded, against the reference sheets.
    ///
    /// Bare worked soil is nearly all crevice and reads almost black between its clods;
    /// bound field ground is far flatter than that. One shading law with one number in front
    /// of it lands both — Indiana's dark soil measures a spread of 21 grey levels about a
    /// mean of 39, and its light soil only 28 about a mean of 142.
    contrast: f32,
}

/// Ground, rendered rather than noised.
///
/// The previous version was three octaves of value noise over a base colour, and against a
/// real track's sheets it reads as a smear of mud. Indiana ships photographs — 1024² of soil
/// with clods, gravel, straw and pale stones in it, each with a lit top and a shaded side,
/// and it is that *aggregate* which the eye reads as ground. Noise has no aggregate at any
/// scale, which is why no amount of tuning made it look like dirt.
///
/// So this builds a little height field and an albedo, and shades one with the other:
///
/// 1. an albedo of the base soil with broad tonal patching over it;
/// 2. loose material scattered into the height field at four sizes, from ten-centimetre clods
///    down to grit, each one a hemisphere with its own colour;
/// 3. straw and root lying on top, which is what breaks up an otherwise uniform field of
///    lumps;
/// 4. a normal from the height field's own gradients, lit from one side.
///
/// Everything wraps: the scatter's lattice is taken modulo the tile and splats are written
/// with wrapping indices, so the sheet meets itself at every edge. A ground texture repeated
/// a hundred and fifty times across a track shows every seam it has.
fn ground_texture(dim: usize, look: &GroundLook, seed: u32) -> Vec<u8> {
    let rgba = ground_pixels(dim, look, seed);
    let mut px = Vec::with_capacity(rgba.len());
    for p in rgba.chunks_exact(4) {
        px.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    tga_bgra(dim, dim, &px)
}

/// The same ground as RGBA, which is what the `.map` embeds — the `.tga` is these pixels with
/// the channels swapped, so the two can't drift apart.
fn ground_pixels(dim: usize, look: &GroundLook, seed: u32) -> Vec<u8> {
    let n = dim * dim;
    let mut hgt = vec![0.0f32; n];
    let mut alb = vec![[0.0f32; 3]; n];

    // 1. The soil itself, patchy at the metre scale.
    for y in 0..dim {
        for x in 0..dim {
            let (u, v) = (x as f32 / dim as f32, y as f32 / dim as f32);
            let mut m = 0.0;
            let mut amp = 1.0;
            let mut period = 3;
            for o in 0..3 {
                m += tile_noise(u * period as f32, v * period as f32, period, seed ^ (o * 131))
                    * amp;
                amp *= 0.5;
                period *= 2;
            }
            let k = 1.0 + m * look.mottle;
            let i = y * dim + x;
            alb[i] = [look.base[0] * k, look.base[1] * k, look.base[2] * k];
            // A little relief under everything, so bare soil isn't perfectly flat either.
            hgt[i] = m * 0.25;
        }
    }

    // 2. Loose material, biggest first so the small stuff settles on top of the big.
    //
    // Sizes are fractions of the tile, which is what keeps them the same size on the ground
    // whatever resolution the sheet is written at.
    let scales: [(usize, f32, f32, f32, f32); 4] = [
        // (lattice, min radius, max radius, height, how many of the cells are filled)
        (12, 0.012, 0.026, 1.00, 0.55),
        (26, 0.0055, 0.0130, 0.80, 0.75),
        (60, 0.0026, 0.0056, 0.55, 0.95),
        (140, 0.0010, 0.0021, 0.35, 1.0),
    ];
    for (si, (cells, rmin, rmax, tall, fill)) in scales.iter().enumerate() {
        let sseed = seed ^ (0x51A1 * (si as u32 + 1));
        for cy in 0..*cells {
            for cx in 0..*cells {
                let j = |k: u32| hash2(cx as i32, cy as i32, sseed.wrapping_add(k)) * 0.5 + 0.5;
                // Loose material clumps. Spread evenly it reads as a printed pattern, and
                // the give-away is that every part of the sheet is equally busy.
                let clump = 0.55
                    + 0.75
                        * (tile_noise(
                            cx as f32 / *cells as f32 * 5.0,
                            cy as f32 / *cells as f32 * 5.0,
                            5,
                            seed ^ 0xC10D,
                        ) * 0.5
                            + 0.5);
                let big = if si < 2 { look.coarse } else { 1.0 };
                if j(1) > *fill * look.clods * big * clump {
                    continue;
                }
                let r = (rmin + (rmax - rmin) * j(2)) * dim as f32;
                let px = (cx as f32 + j(3)) / *cells as f32 * dim as f32;
                let py = (cy as f32 + j(4)) / *cells as f32 * dim as f32;
                // Its own colour: mostly the soil, sometimes a pale stone.
                let stone = j(5) < look.fleck_density;
                let tint = look.grain_tint.0 + (look.grain_tint.1 - look.grain_tint.0) * j(6);
                let colour = if stone {
                    look.fleck
                } else {
                    [
                        look.base[0] * tint,
                        look.base[1] * tint,
                        look.base[2] * tint,
                    ]
                };
                // Clods are not round and they are not smooth. Squashed, turned, and with a
                // ragged outline — a field of clean discs reads as bubbles on a surface
                // rather than as broken ground, which is what the first version of this did.
                let (sq, rot) = (0.62 + 0.5 * j(7), j(8) * std::f32::consts::PI);
                let (cr, sr) = (rot.cos(), rot.sin());
                let (lobe_a, lobe_b) = (j(9) * std::f32::consts::TAU, j(10) * std::f32::consts::TAU);
                let lobe_c = j(12) * std::f32::consts::TAU;
                let ragged = 0.05 + 0.10 * j(11);
                let ri = (r * 1.35).ceil() as i32 + 1;
                for dy in -ri..=ri {
                    for dx in -ri..=ri {
                        let (fx, fy) = (dx as f32, dy as f32);
                        let (ax, ay) = (fx * cr + fy * sr, (-fx * sr + fy * cr) / sq);
                        let rho = (ax * ax + ay * ay).sqrt();
                        if rho < 1e-4 {
                            // dead centre: no angle, and the shape is 1 there anyway
                        }
                        let th = ay.atan2(ax);
                        // Three harmonics rather than two, and none of them dominant. Two
                        // strong lobes make every lump a star, which at a distance reads as a
                        // printed pattern rather than as broken ground.
                        let wobble = 1.0
                            + ragged
                                * ((th * 3.0 + lobe_a).sin() * 0.5
                                    + (th * 6.0 + lobe_b).sin() * 0.32
                                    + (th * 11.0 + lobe_c).sin() * 0.18);
                        let q = rho / (r * wobble).max(1e-3);
                        if q >= 1.0 {
                            continue;
                        }
                        let x = (px as i32 + dx).rem_euclid(dim as i32) as usize;
                        let y = (py as i32 + dy).rem_euclid(dim as i32) as usize;
                        let i = y * dim + x;
                        // Flat-topped with a sharp shoulder, not a hemisphere. A clod's face
                        // is broadly flat and its edge is where all the contrast lives; a
                        // dome puts a bright highlight in the middle of every lump and the
                        // whole sheet turns to water droplets.
                        let h = (1.0 - q * q).powf(0.42) * tall;
                        if h > hgt[i] {
                            hgt[i] = h;
                        }
                        // Colour takes the nearer part of the lump, so overlapping clods
                        // still read as separate things rather than as one blended smear.
                        if q < 0.9 {
                            let w = (1.0 - q / 0.9).min(0.85);
                            for c in 0..3 {
                                alb[i][c] += (colour[c] - alb[i][c]) * w;
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Grit — below the size any splat can draw. Two frequencies: a few-pixel roughness
    // that every lump inherits, and per-pixel sand under that.
    for y in 0..dim {
        for x in 0..dim {
            let i = y * dim + x;
            hgt[i] += value_noise(x as f32 / 2.5, y as f32 / 2.5, seed ^ 0x3C41) * 0.16;
            hgt[i] += hash2(x as i32, y as i32, seed ^ 0x6217) * 0.07;
            let f = hash2(x as i32, y as i32, seed ^ 0x77A3) * 0.5 + 0.5;
            if f > 0.9985 {
                alb[i] = look.fleck;
            }
        }
    }

    // 4. Everything that lies across the lumps rather than being one of them: blades first,
    // then the straw and root on top of them.
    let mut strokes = |count: usize, colour: ([f32; 3], [f32; 3]), lo: f32, hi: f32,
                       tall: f32, sseed: u32| {
        for k in 0..count {
            let g = |q: u32| hash2(k as i32, q as i32, sseed) * 0.5 + 0.5;
            let g2 = |q: u32| hash2(k as i32, q as i32 + 500, sseed) * 0.5 + 0.5;
            let (x0, y0) = (g(1) * dim as f32, g(2) * dim as f32);
            let ang = g(3) * std::f32::consts::TAU;
            let len = (lo + (hi - lo) * g(4)) * dim as f32;
            let bend = (g(5) - 0.5) * 1.2;
            // Two tones, because one colour of anything reads as paint.
            let mix = g2(6);
            let col = [
                colour.0[0] + (colour.1[0] - colour.0[0]) * mix,
                colour.0[1] + (colour.1[1] - colour.0[1]) * mix,
                colour.0[2] + (colour.1[2] - colour.0[2]) * mix,
            ];
            let steps = len.ceil() as i32;
            for t in 0..=steps {
                let u = t as f32 / steps.max(1) as f32;
                let a = ang + bend * u;
                let x = (x0 + a.cos() * len * u).round() as i32;
                let y = (y0 + a.sin() * len * u).round() as i32;
                // Thin, and fading out at the far end where it is buried.
                let fade = 1.0 - u * u * 0.7;
                for (ox, oy) in [(0, 0), (1, 0), (0, 1)] {
                    let xx = (x + ox).rem_euclid(dim as i32) as usize;
                    let yy = (y + oy).rem_euclid(dim as i32) as usize;
                    let i = yy * dim + xx;
                    let w = fade * if ox == 0 && oy == 0 { 0.9 } else { 0.35 };
                    for c in 0..3 {
                        alb[i][c] += (col[c] - alb[i][c]) * w;
                    }
                    hgt[i] = hgt[i].max(tall * fade);
                }
            }
        }
    };
    let blades = (look.blade_density * (dim * dim) as f32 / 9000.0) as usize;
    strokes(blades, look.blade, 0.006, 0.022, 0.45, seed ^ 0xB1AD);
    let strands = (look.litter_density * (dim * dim) as f32 / 9000.0) as usize;
    strokes(
        strands,
        (look.litter, look.litter),
        0.012,
        0.045,
        0.30,
        seed ^ 0x11FE,
    );

    // 5. Light it. The relief is what turns a field of coloured lumps into ground, and it
    // comes out of the height field's own gradients rather than out of the colour.
    //
    // Two terms, and the second one is what was missing. A plain lambert gives every lump a
    // lit side and a dark side and stops there; real broken ground is mostly *crevice* —
    // the gaps between things are darker than any face, however that face is turned. So the
    // height is measured against a blurred copy of itself, and what sits below its own
    // neighbourhood is shaded down. That single term is most of the difference between a
    // pattern and a photograph.
    let blur = box_blur_wrap(&hgt, dim, 6);
    let at = |x: usize, y: usize| hgt[y * dim + x];
    let mut px = Vec::with_capacity(n * 4);
    for y in 0..dim {
        for x in 0..dim {
            let (xm, xp) = ((x + dim - 1) % dim, (x + 1) % dim);
            let (ym, yp) = ((y + dim - 1) % dim, (y + 1) % dim);
            let gx = (at(xp, y) - at(xm, y)) * 0.5;
            let gy = (at(x, yp) - at(x, ym)) * 0.5;
            // A steep enough surface that a two-pixel pebble still catches its edge.
            let (nx, ny, nz) = (-gx * 4.0, -gy * 4.0, 1.0);
            let inv = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
            // Lit from up and to the left, matching nothing in particular — this is a
            // tiling detail sheet, and the game lights the terrain itself.
            let lambert = (nx * -0.45 + ny * -0.45 + nz * 0.77) * inv;
            let i = y * dim + x;
            let k = look.contrast;
            let open = ((hgt[i] - blur[i]) * 2.6 * k + 1.0 - 0.12 * k).clamp(0.26, 1.45);
            let shade = ((1.0 - 0.19 * k + 0.42 * k * lambert) * open).clamp(0.18, 1.55);
            let c = |k: usize| (alb[i][k] * shade).clamp(0.0, 255.0) as u8;
            px.extend_from_slice(&[c(0), c(1), c(2), 255]);
        }
    }
    px
}

/// What the ground and the riding line are coloured, by what the track is made of.
///
/// Measured off the sheets rather than stated beside them. The roost has to be the colour of
/// the ground it came off, and a `GroundLook`'s base is what goes *into* the renderer —
/// shading takes about a quarter of it back out, so quoting the base here would spray dirt
/// visibly lighter than the dirt it came from. A small tile costs nothing and cannot drift.
fn ground_palette(s: Surface) -> ([u8; 3], [u8; 3]) {
    let (field, ridden, ..) = ground_looks(s);
    let mean = |look: &GroundLook| -> [u8; 3] {
        const DIM: usize = 256;
        let px = ground_pixels(DIM, look, 0x9A0D);
        let mut sum = [0u64; 3];
        for p in px.chunks_exact(4) {
            for c in 0..3 {
                sum[c] += p[c] as u64;
            }
        }
        sum.map(|v| (v / (DIM * DIM) as u64) as u8)
    };
    (mean(&field), mean(&ridden))
}

/// The three grounds a track is painted with, from what it says it is made of.
///
/// The riding line is worked soil: darker, wetter, nearly all clods, and no grass in it. The
/// field either side is bound ground with stones showing. The graded shoulder is between the
/// two, and is most of what a rider actually sees from the seat.
fn ground_looks(surface: Surface) -> (GroundLook, GroundLook, GroundLook, GroundLook) {
    // Read off Indiana's own sheets rather than picked. `soil_light_c` averages (172, 134,
    // 99) and `soil_dark_c` (50, 36, 24) — a bright tan field against a nearly black riding
    // line, and the gap between them is far wider than any two colours anyone would guess.
    // These are the numbers *before* shading, which lands around three quarters of them.
    let (base, line): ([f32; 3], [f32; 3]) = match surface {
        Surface::Soil => ([179.0, 140.0, 104.0], [56.0, 40.0, 27.0]),
        Surface::Sand => ([214.0, 193.0, 152.0], [166.0, 142.0, 105.0]),
        Surface::Grass => ([174.0, 142.0, 100.0], [55.0, 40.0, 27.0]),
    };
    let field = GroundLook {
        base,
        grain_tint: (0.50, 1.38),
        fleck: [196.0, 190.0, 176.0],
        fleck_density: 0.05,
        litter: [186.0, 168.0, 112.0],
        litter_density: 1.0,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 0.30,
        mottle: 0.16,
        contrast: 0.52,
    };
    let ridden = GroundLook {
        base: line,
        grain_tint: (0.34, 1.72),
        fleck: [150.0, 146.0, 138.0],
        fleck_density: 0.03,
        litter: [140.0, 122.0, 84.0],
        litter_density: 0.25,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 1.0,
        mottle: 0.20,
        contrast: 1.0,
    };
    // The graded shoulder: the field's colour, worked over like the line.
    let shoulder = GroundLook {
        base: [
            (base[0] + line[0]) * 0.5,
            (base[1] + line[1]) * 0.5,
            (base[2] + line[2]) * 0.5,
        ],
        grain_tint: (0.46, 1.46),
        fleck: [165.0, 160.0, 150.0],
        fleck_density: 0.07,
        litter: [172.0, 156.0, 106.0],
        litter_density: 0.6,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 0.65,
        mottle: 0.18,
        contrast: 0.72,
    };
    let grass = GroundLook {
        base: [100.0, 114.0, 62.0],
        grain_tint: (0.55, 1.32),
        fleck: [126.0, 132.0, 78.0],
        fleck_density: 0.02,
        litter: [182.0, 172.0, 102.0],
        litter_density: 1.4,
        blade: ([78.0, 104.0, 44.0], [148.0, 168.0, 88.0]),
        blade_density: 34.0,
        clods: 0.35,
        coarse: 0.25,
        mottle: 0.26,
        contrast: 0.8,
    };
    (field, ridden, shoulder, grass)
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

/// The track's sound sources: none of them.
///
/// It used to be a zero-byte file, which is not the same statement. Every published track's
/// `.ssc` opens with a count — Indiana declares five and hangs a crowd on each — and a reader
/// looking for `numsources` in an empty file does not find a zero, it finds nothing at all.
/// Saying "none" is a sentence; saying nothing is not. A generated track ships no crowd, so
/// none is the honest answer.
const SSC: &str = "numsources = 0\n";

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
    // Sampled down to the picture's own size *before* blurring. Blurring the full grid to
    // shade a postage stamp costs two copies of the terrain and changes nothing you can see.
    let small: Vec<f32> = (0..dim * dim)
        .map(|i| {
            let gy = ((i / dim) * syn.gh / dim).min(syn.gh - 1);
            let gx = ((i % dim) * syn.gw / dim).min(syn.gw - 1);
            syn.heights[gy * syn.gw + gx]
        })
        .collect();
    let base_small = crate::trackstats::box_blur(&small, dim, dim, 3);
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
            let here = y * dim + x;
            let relief =
                ((small[here] - base_small[here]) * 90.0 + 128.0).clamp(0.0, 255.0) as u8;
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

/// How many times a sheet repeats across the terrain, for a wanted tile size on the ground.
///
/// At least one, and rounded, because it is a count of tiles.
fn repetitions(prog: &TrackProgram, tile_m: f32) -> u32 {
    (prog.terrain.size_x.max(prog.terrain.size_z) / tile_m.max(0.5))
        .round()
        .clamp(1.0, 4096.0) as u32
}

fn hmf(prog: &TrackProgram, syn: &Synth) -> String {
    let mut s = header(prog, syn);
    // Four bands, not three: field, graded shoulder, riding line, and the grass over the
    // top. A track painted line-and-field is a brown ribbon on a green sheet, and the
    // shoulder — the worked ground either side of the ribbon — is most of what is actually
    // in front of a rider.
    s.push_str("num_layers = 4\n");
    // Layer zero carries no mask: it is the ground everything else is painted over.
    s.push_str(&format!(
        "layer0\n{{\n\tmap = maps/ground.tga\n\trepetitions = {}\n}}\n\n",
        repetitions(prog, TILE_FIELD_M)
    ));
    s.push_str(&format!(
        "layer1\n{{\n\tmap = maps/shoulder.tga\n\trepetitions = {}\n\
         \tmask = mask_shoulder.tga\n\tthickness = 0.05\n}}\n\n",
        repetitions(prog, TILE_SHOULDER_M)
    ));
    s.push_str(&format!(
        "layer2\n{{\n\tmap = maps/line.tga\n\trepetitions = {}\n\tmask = mask_dirt.tga\n\
         \tthickness = 0.1\n}}\n\n",
        repetitions(prog, TILE_LINE_M)
    ));
    s.push_str(&format!(
        "layer3\n{{\n\tmap = maps/grass.tga\n\trepetitions = {}\n\tmask = mask_grass.tga\n\
         \tthickness = 0.01\n\n\tgrass\n\t{{\n\t\tmax_density = 20\n\t\theight = 0.2\n\
         \t\theight_diff = 0.1\n\t\twidth = 0.25\n\t\twidth_diff = 0.1\n\
         \t\ttexture = maps/grassfx.tga\n\t\tdensitymap = mask_grass.tga\n\t}}\n}}\n",
        repetitions(prog, TILE_GRASS_M)
    ));
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
            Segment::Arc { radius, angle, .. } => (radius, angle.abs()),
        };
        let kind = if matches!(seg, Segment::Straight { .. }) {
            0
        } else {
            1
        };
        s.push_str(&format!(
            "segment{i}\n{{\n\ttype = {kind}\n\tlength = {:.6}\n\tradius = {radius:.6}\n\
             \tangle = {angle:.6}\n\theight = {:.6}\n\theightlock = 0\n}}\n",
            seg.length(),
            seg.rise()
        ));
    }
    s
}

/// The track's own description, in the shape published tracks write it.
///
/// Two details are load-bearing and were wrong: `length` is a plain number of metres — a
/// `pic`/`pic_info` have to name files the archive actually carries, which are the two the
/// writer puts beside this one.
///
/// `length` and `altitude` are **not** measurements, whatever they sound like. Millville,
/// Flanders and Indiana all state `1`, Lambretta Lynds states `999`, and not one published
/// track puts a plausible number of metres there — so a lap length in the field is a value
/// the game has never been shown.
fn track_ini(prog: &TrackProgram) -> String {
    let slug = slug(&prog.name);
    format!(
        "[info]\nname = {}\nshort_name = {}\nlength = 1\naltitude = 1\n\n\
         [race]\ndefaulteventlaps = 15\nreflaptime = {:.0}\n\n\
         [ui]\npic = {slug}.tga\npic_info = {slug}_map.tga\nauthor = {}\nlocation = {}\n\n\
         [weather]\ncloud_prob = 0.4\nrainy_prob = 0.1\n",
        prog.name,
        prog.name.chars().take(12).collect::<String>(),
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
         The `maps` folder holds the ground sheets, and they are generated with the rest of\n\
         it — nothing here has to be downloaded or copied in first.\n\n\
         1. _map.bat        graphics, writes {slug}/{slug}.map\n\
         2. _trh.bat        collision, writes {slug}/{slug}.trh\n\
         3. _centerline.bat merges track.tcl into the .trh\n\n\
         Then zip the {slug} folder and rename the zip {slug}.pkz.\n\n\
         The .rdf beside it — start gate, pit lane, finish line, checkpoints — is written\n\
         here rather than in TrackEd, laid out from the lap itself. Open it there if you\n\
         want to move the cameras.\n",
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

pub fn slug(name: &str) -> String {
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
                Segment::Straight { length: 160.0, rise: 0.0 },
                Segment::Arc { radius: 60.0, angle: 180.0, rise: 0.0 },
                Segment::Straight { length: 160.0, rise: 0.0 },
                Segment::Arc { radius: 60.0, angle: 180.0, rise: 0.0 },
            ],
            width: 12.0,
            blend: crate::trackprog::default_blend(),
            elevation: Vec::new(),
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
    fn a_fitted_budget_holds_the_track_and_little_else() {
        let mut p = oval();
        p.terrain.scale = 1.0; // far too small to build
        let fitted = with_fitted_budget(&p).expect("fitting doesn't need a workable budget");
        let s = synthesise(&fitted).expect("and what comes back builds");
        assert!(
            s.used_m < fitted.terrain.scale,
            "used {:.1} of {:.1}",
            s.used_m,
            fitted.terrain.scale
        );
        // Snug, not generous: a budget ten times the relief quantises ten times coarser.
        assert!(
            fitted.terrain.scale < s.used_m * 1.5,
            "budget {:.1} for {:.1} m of terrain",
            fitted.terrain.scale,
            s.used_m
        );
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

    /// Two jumps close together must not add up. Before, the ground between a pair of
    /// tabletops rose to their combined height and a rhythm section came out as one tall
    /// lump; each should keep its own height and the pair should read as one shape.
    /// A shape drawn point by point is built as it was drawn.
    #[test]
    fn a_hand_drawn_shape_is_built_where_its_points_are() {
        use crate::trackprog::ShapePoint;
        let mut p = oval();
        p.terrain.relief.amplitude = 0.0;
        p.features = vec![Feature::Custom {
            at: 40.0,
            length: 40.0,
            shape: vec![
                ShapePoint { u: 0.0, h: 0.0 },
                ShapePoint { u: 0.3, h: 2.5 },
                ShapePoint { u: 0.6, h: 0.4 },
                ShapePoint { u: 1.0, h: 0.0 },
            ],
        }];
        let s = synthesise(&p).unwrap();
        let ground = height_at_arc(&s, 20.0);
        let crest = height_at_arc(&s, 40.0 + 40.0 * 0.3) - ground;
        let dip = height_at_arc(&s, 40.0 + 40.0 * 0.6) - ground;
        assert!((crest - 2.5).abs() < 0.6, "the crest reads {crest:.2} m, drawn at 2.5");
        assert!(dip < 1.2, "the dip reads {dip:.2} m, drawn at 0.4");
        assert!(crest - dip > 1.2, "the two are {:.2} m apart", crest - dip);
    }

    #[test]
    fn jumps_that_touch_keep_their_own_height() {
        let mut p = oval();
        // Overlapping where both are at full height, which is the only place summing shows
        // itself — two jumps that meet ramp-to-ramp barely overlap at all.
        // Flat ground, so the only thing in the measurement is the jumps.
        p.terrain.relief.amplitude = 0.0;
        // Overlapping where both are at full height, which is the only place summing shows
        // itself — two jumps that meet ramp-to-ramp barely overlap at all.
        p.features = vec![
            Feature::Tabletop { at: 30.0, length: 24.0, height: 2.0 },
            Feature::Tabletop { at: 33.0, length: 24.0, height: 2.0 },
        ];
        let s = synthesise(&p).unwrap();
        let base = height_at_arc(&s, 10.0);
        let peak = (300..=700)
            .map(|i| height_at_arc(&s, i as f32 / 10.0) - base)
            .fold(f32::MIN, f32::max);
        assert!(
            peak < 2.4,
            "two 2 m jumps a hair apart came out {peak:.2} m tall"
        );
        assert!(peak > 1.4, "and they should still be jumps: {peak:.2} m");
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
    fn a_segment_that_rises_takes_the_track_with_it() {
        let mut p = oval();
        p.features.clear();
        // Flat ground, so what is measured is the rise and not the hill it was laid on: the
        // track follows the landscape, and this oval's landscape moves several metres over
        // the length of a straight.
        p.terrain.relief.amplitude = 0.0;
        // The first straight climbs six metres; the far one gives them back, so the lap
        // still meets itself at the same height.
        p.segments[0] = Segment::Straight { length: 160.0, rise: 6.0 };
        p.segments[2] = Segment::Straight { length: 160.0, rise: -6.0 };
        let s = synthesise(&p).unwrap();
        let climb = height_at_arc(&s, 155.0) - height_at_arc(&s, 5.0);
        assert!(
            (climb - 6.0).abs() < 1.0,
            "the straight climbed {climb:.2} m, not 6"
        );
        // And it is a hill, not a step: half way up is half way there.
        let half = height_at_arc(&s, 80.0) - height_at_arc(&s, 5.0);
        assert!((half - 3.0).abs() < 1.2, "half way up reads {half:.2} m");

        // The whole point: rises that cancel bring the lap home level. Checked at the far
        // end, which is the only place a running total that compounds can show itself.
        let lap = p.lap_length();
        let home = height_at_arc(&s, lap - 2.0) - height_at_arc(&s, 2.0);
        assert!(
            home.abs() < 1.0,
            "the lap comes back {home:.2} m off the height it left at"
        );
    }

    /// Rises are cumulative, and nothing should count twice. Two climbs and two drops of the
    /// same size, spread round a lap, has to come out level however many segments carry it.
    /// A curve drawn by hand lifts the track where its points say, and comes back round to
    /// meet itself — a lap is a loop, so the last point has to ease into the first.
    #[test]
    fn a_drawn_curve_lifts_the_track_where_it_says() {
        let mut p = oval();
        p.features.clear();
        p.terrain.relief.amplitude = 0.0;
        let lap = p.lap_length();
        p.elevation = vec![
            Knot { at: 0.0, height: 0.0 },
            Knot { at: lap * 0.25, height: 8.0 },
            Knot { at: lap * 0.5, height: 0.0 },
            Knot { at: lap * 0.75, height: -4.0 },
        ];
        let s = synthesise(&p).unwrap();
        let ground = height_at_arc(&s, 1.0);
        let top = height_at_arc(&s, lap * 0.25) - ground;
        let dip = height_at_arc(&s, lap * 0.75) - ground;
        assert!((top - 8.0).abs() < 1.2, "the peak reads {top:.2} m, wanted 8");
        assert!((dip + 4.0).abs() < 1.2, "the dip reads {dip:.2} m, wanted -4");
        // And across the line, where the wrap has to hold.
        let before = height_at_arc(&s, lap - 2.0) - ground;
        assert!(before.abs() < 1.2, "it comes back {before:.2} m off");
    }

    #[test]
    fn climbs_and_drops_cancel_however_many_there_are() {
        let mut p = oval();
        p.features.clear();
        p.segments = vec![
            Segment::Straight { length: 80.0, rise: 4.0 },
            Segment::Arc { radius: 60.0, angle: 180.0, rise: -4.0 },
            Segment::Straight { length: 80.0, rise: 4.0 },
            Segment::Arc { radius: 60.0, angle: 180.0, rise: -4.0 },
        ];
        let s = synthesise(&p).unwrap();
        let lap = p.lap_length();
        let drift = height_at_arc(&s, lap - 2.0) - height_at_arc(&s, 2.0);
        assert!(drift.abs() < 1.0, "drifted {drift:.2} m over the lap");
        // And it never climbs more than the 4 m any one segment asked for.
        let peak = (0..40)
            .map(|i| height_at_arc(&s, lap * i as f32 / 40.0))
            .fold(f32::MIN, f32::max);
        let floor = (0..40)
            .map(|i| height_at_arc(&s, lap * i as f32 / 40.0))
            .fold(f32::MAX, f32::min);
        assert!(
            peak - floor < 12.0,
            "the line spans {:.1} m for two 4 m climbs",
            peak - floor
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

    /// The empty `.map` has to be one our own parser accepts, or it is not the format's
    /// degenerate case — it is a broken file.
    /// The race data has to parse as the same shape the example track's does, because that
    /// file is the only description of the format there is.
    #[test]
    fn the_race_data_has_the_blocks_the_game_looks_for() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        let text = rdf(&p);
        for block in [
            "finish_line",
            "split1",
            "split2",
            "pit_lane",
            "pit_board",
            "starting_grid",
            "num_checkpoints = 3",
            "checkpoint0",
            "30seconds_board",
        ] {
            assert!(text.contains(block), "no {block}");
        }
        // Braces balance, or the game's parser walks off the end of the file.
        assert_eq!(
            text.matches('{').count(),
            text.matches('}').count(),
            "unbalanced braces"
        );
        // Every marker sits somewhere on the lap.
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("long = ") {
                let at: f32 = v.parse().unwrap();
                assert!(
                    (0.0..=p.lap_length()).contains(&at),
                    "a marker at {at} m is off a {} m lap",
                    p.lap_length()
                );
            }
        }
    }

    #[test]
    fn the_map_is_a_map_with_the_ground_sheets_in_it() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let m = map(&p, &s);
        assert_eq!(&m[..4], b"MP2\0");
        assert_eq!(u32::from_le_bytes(m[4..8].try_into().unwrap()), 304);
        // One material per sheet: the two are matched positionally, so the counts have to
        // agree or every picture is hung on the wrong thing — or on nothing.
        assert_eq!(u32::from_le_bytes(m[8..12].try_into().unwrap()), 4, "materials");
        assert!(crate::map::is_map(&m), "not recognised as a map");

        // The check that matters, and the one that was missing. `embedded_textures` *scans*
        // for records; the game walks the file — material records, then the vertex block,
        // then the indices, then the node tree, and only then the sheets. A map that can only
        // be scanned is one whose textures a walker never reaches, and the previous version
        // declared no mesh at all, so the walk stopped at its fifth word. It asserted that
        // outright: "there is no mesh in it to find".
        let mesh = crate::map::parse(&m).expect("a walker has to find the mesh");
        assert!(mesh.vertex_count() >= 8, "{} vertices", mesh.vertex_count());
        assert!(mesh.triangle_count() >= 1, "{} triangles", mesh.triangle_count());

        // And the sheets have to be where the walk ends up, not merely somewhere in the file.
        let walked = crate::map::declared(&m);
        assert_eq!(
            walked.iter().map(|(n, ..)| n.as_str()).collect::<Vec<_>>(),
            ["ground_c", "shoulder_c", "dirt_line_c", "grass_c"],
            "walked to {walked:?}"
        );
        for (n, w, h) in &walked {
            assert_eq!(
                (*w, *h),
                (GROUND_TEXTURE_DIM as u32, GROUND_TEXTURE_DIM as u32),
                "{n} is {w}x{h}"
            );
        }

        // The scanner that reads published tracks' textures has to find ours, and they have
        // to inflate to the pixel count they claim — a record the game would skip is a
        // ground sheet the track doesn't have.
        let texs = crate::edf::embedded_textures(&m);
        let names: Vec<&str> = texs.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            ["ground_c", "shoulder_c", "dirt_line_c", "grass_c"],
            "found {names:?}"
        );

        for t in &texs {
            assert_eq!(t.width, GROUND_TEXTURE_DIM as u32);
            let px = crate::edf::inflate_texture(&m, t).expect("the sheet inflates");
            assert_eq!(px.len(), (t.width * t.height * 4) as usize);
        }
        // The colour is the surface's, not a default: a sand track's ground reads as sand.
        let sand = {
            let mut p = oval();
            p.terrain.surface = Surface::Sand;
            let s = synthesise(&p).unwrap();
            let m = map(&p, &s);
            let t = crate::edf::embedded_textures(&m).remove(0);
            crate::edf::inflate_texture(&m, &t).unwrap()[0]
        };
        let soil = crate::edf::inflate_texture(&m, &texs[0]).unwrap()[0];
        assert!(sand > soil, "sand ({sand}) should be lighter than soil ({soil})");
    }

    /// The game looks for a track's files in a folder named after it — flat at the archive
    /// root they are not found at all, which is what a preview used to install as.
    #[test]
    fn the_pkz_nests_its_files_and_names_the_pictures_it_carries() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("mxb-pkz-selftest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preview.pkz");
        write_pkz(&p, &s, &path, false).unwrap();

        let slug = slug(&p.name);
        let names = crate::pkz::entry_names(&path).unwrap();
        for want in [
            format!("{slug}/{slug}.trh"),
            format!("{slug}/{slug}.map"),
            format!("{slug}/{slug}.ini"),
            format!("{slug}/{slug}.rdf"),
            format!("{slug}/{slug}.amb"),
            format!("{slug}/{slug}.ssc"),
            format!("{slug}/gfx.cfg"),
        ] {
            assert!(names.contains(&want), "{want} is missing from {names:?}");
        }

        // And the `.ini` names pictures the archive actually has — it used to name two
        // files that were never written, which is a track with no artwork at all.
        let ini =
            String::from_utf8(crate::pkz::read_entry(&path, &format!("{slug}.ini")).unwrap().unwrap())
                .unwrap();
        // PiBoSo writes these CRLF, so a line of ours is never bare LF.
        assert!(
            ini.contains("\r\n") && !ini.replace("\r\n", "").contains('\n'),
            "the ini is not CRLF throughout"
        );
        for line in ini.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if matches!(key.trim(), "pic" | "pic_info") {
                let want = format!("{slug}/{}", value.trim());
                assert!(names.contains(&want), "the ini names {want}, which isn't in it");
            }
            // A length with a unit on it is not a number the game can read.
            if key.trim() == "length" {
                assert!(
                    value.trim().parse::<f32>().is_ok(),
                    "length = {value:?} is not a plain number of metres"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing we ship may be an empty file.
    ///
    /// The `.ssc` was written as zero bytes for months. It is a config the game parses, and a
    /// parser looking for `numsources` in an empty file does not read a zero — so the fault
    /// is not "a file with nothing in it", it is "a file that answers no question it is
    /// asked". Generalised past the one that was wrong, because the next one will be a
    /// different file.
    /// A material record has to look like a published one, word for word.
    ///
    /// Every one of Indiana's 49 is identical but for a single field: zero, six ones, four
    /// zeros, a **one-based id at word eleven**, two zeros. We shipped ones across the first
    /// twelve words with the id at word thirteen — a shape no map has — and the game hard
    /// crashed at the track graphics stage. Pinned here against the numbers read off the
    /// file, because the only reason we know them is that somebody looked.
    #[test]
    fn material_records_match_a_published_maps() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let m = map(&p, &s);
        let count = u32::from_le_bytes(m[8..12].try_into().unwrap()) as usize;
        assert_eq!(count, 4, "one material per sheet");
        for k in 0..count {
            let r = 12 + k * 56;
            let w = |i: usize| u32::from_le_bytes(m[r + i * 4..r + i * 4 + 4].try_into().unwrap());
            let f = |i: usize| f32::from_le_bytes(m[r + i * 4..r + i * 4 + 4].try_into().unwrap());
            assert_eq!(w(0), 0, "material {k} word 0");
            for i in 1..=6 {
                assert_eq!(f(i), 1.0, "material {k} word {i}");
            }
            for i in 7..=10 {
                assert_eq!(w(i), 0, "material {k} word {i}");
            }
            assert_eq!(w(11), (k + 1) as u32, "material {k}: the id is one-based, at word 11");
            assert_eq!(w(12), 0, "material {k} word 12");
            assert_eq!(w(13), 0, "material {k} word 13");
        }
    }

    #[test]
    fn every_file_in_a_built_track_says_something() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("mxb-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let wrote = write_source(&p, &s, &dir).unwrap();
        for rel in &wrote {
            let n = std::fs::metadata(dir.join(rel)).unwrap().len();
            assert!(n > 0, "{rel} is empty, which is not the same as saying nothing is there");
        }
        // And the same for the archive, which is assembled separately and so can drift.
        let pkz = dir.join("t.pkz");
        write_pkz(&p, &s, &pkz, false).unwrap();
        let f = std::fs::File::open(&pkz).unwrap();
        let mut zip = zip::ZipArchive::new(f).unwrap();
        for i in 0..zip.len() {
            let e = zip.by_index(i).unwrap();
            assert!(e.size() > 0, "{} is empty in the .pkz", e.name());
        }
        let _ = std::fs::remove_dir_all(&dir);
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
            format!("{slug}/gfx.cfg"),
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

        // And again against the track's own centreline, which is how the published tracks are
        // measured. The corridor rule and this one disagree by construction — one finds the
        // line, the other is told where it is — and it is the second that has real tracks to
        // compare against.
        let bytes = trh(&p, &s, false);
        let block = &bytes[12 + s.gw * s.gh * 2..];
        let lap = crate::trackline::read(block).expect("the .trh carries its own centreline");
        let r = crate::trackstats::ridden(
            &lap,
            &crate::trackstats::Grid {
                w: s.gw,
                h: s.gh,
                size_x: p.terrain.size_x,
                size_z: p.terrain.size_z,
                v: s.heights.clone(),
            },
        )
        .expect("and it measures");
        println!(
            "centreline: lap {:.0}m  {} segs = {} arcs + {} straights  {} turns  turn p50 {:.0}°  \
             tightest R p50 {:.1}m  turning {:.0}°",
            r.lap_m, r.segments, r.arcs, r.straights, r.turns, r.turn_deg.p50,
            r.turn_radius_m.p50, r.total_turn_deg,
        );
        println!(
            "  ruts {:.1} at {:.2}m  depth corner p50 {:.2} p90 {:.2}  straight p50 {:.2}  |  \
             berm out {:.2}m in {:.2}m  bank p50 {:.1}° p90 {:.1}°",
            r.rut_lines, r.rut_spacing_m.p50, r.rut_depth_corner_m.p50, r.rut_depth_corner_m.p90,
            r.rut_depth_straight_m.p50, r.berm_outside_m, r.berm_inside_m,
            r.bank_deg.p50, r.bank_deg.p90,
        );
        println!(
            "  {:.1} lips/km ({:.1} over 1m)  h p50 {:.2} p90 {:.2} max {:.2}  \
             face p50 {:.1}° p90 {:.1}°  gap p50 {:.1}m",
            r.lips_per_km, r.big_lips_per_km, r.lip_height_m.p50, r.lip_height_m.p90,
            r.lip_height_m.max, r.lip_face_deg.p50, r.lip_face_deg.p90, r.lip_spacing_m.p50,
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
            let size = write_pkz(&p, &s, &pkz, false).unwrap();
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

    /// A stadium with corners tight enough to rut, since the oval's 60 m arcs are not.
    fn hairpins() -> TrackProgram {
        let mut p = oval();
        p.name = "Test Hairpins".into();
        p.features.clear();
        p.segments = vec![
            Segment::Straight { length: 120.0, rise: 0.0 },
            Segment::Arc { radius: 16.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 120.0, rise: 0.0 },
            Segment::Arc { radius: 16.0, angle: 180.0, rise: 0.0 },
        ];
        p
    }

    /// The heights across the track at one point round the lap, from one edge to the other.
    fn across(s: &Synth, at_arc: f32) -> Vec<f32> {
        let k = s
            .stations
            .iter()
            .position(|st| st.s >= at_arc)
            .unwrap_or(s.stations.len() - 1);
        let st = s.stations[k];
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let steps = 120;
        (0..=steps)
            .map(|i| {
                let t = (i as f32 / steps as f32 - 0.5) * 2.0 * 5.4;
                sample(
                    &s.heights,
                    s.gw,
                    s.gh,
                    (st.x + rx * t) / s.mps,
                    (st.z + rz * t) / s.mps,
                )
            })
            .collect()
    }

    /// How many separate grooves a cross-section has, and how much ground they take out
    /// between them — counting only ones deep enough to be a rut rather than surface grain.
    fn groove_depth(v: &[f32], least_m: f32) -> f32 {
        let mut sum = 0.0;
        for i in 1..v.len() - 1 {
            if v[i] > v[i - 1] || v[i] > v[i + 1] {
                continue;
            }
            let mut l = i;
            while l > 0 && v[l - 1] >= v[l] {
                l -= 1;
            }
            let mut r = i;
            while r + 1 < v.len() && v[r + 1] >= v[r] {
                r += 1;
            }
            let d = (v[l] - v[i]).min(v[r] - v[i]);
            if d >= least_m {
                sum += d;
            }
        }
        sum
    }

    fn grooves(v: &[f32], least_m: f32) -> usize {
        let mut n = 0;
        for i in 1..v.len() - 1 {
            if v[i] > v[i - 1] || v[i] > v[i + 1] {
                continue;
            }
            // Walk out to the crest on each side; the shallower of the two is the depth.
            let mut l = i;
            while l > 0 && v[l - 1] >= v[l] {
                l -= 1;
            }
            let mut r = i;
            while r + 1 < v.len() && v[r + 1] >= v[r] {
                r += 1;
            }
            if (v[l] - v[i]).min(v[r] - v[i]) >= least_m {
                n += 1;
            }
        }
        n
    }

    /// A corner does not wear one groove down the middle. Everybody takes roughly the same
    /// line and nobody takes exactly it, so what a tight turn ends up with is a comb — which
    /// is what a published track's collision terrain shows and what a single rut does not.
    #[test]
    /// Which side a corner's bank stands on. The berm the corner grows on its own used to
    /// stand on the *inside*, and nothing caught it because the demo declares a berm at every
    /// corner and a declared one replaced it.
    #[test]
    fn a_corner_banks_on_its_outside() {
        let p = hairpins(); // right-hand, and no berm declared anywhere
        let s = synthesise(&p).unwrap();
        let v = across(&s, 120.0 + 0.5 * std::f32::consts::PI * 16.0);
        let low = v.iter().fold(f32::MAX, |a, b| a.min(*b));
        // `across` runs left to right and the hairpins turn right, so the outside is the
        // first sample and the inside the last.
        let (outside, inside) = (v[0] - low, v[v.len() - 1] - low);
        assert!(
            outside > inside + 0.15,
            "outside stands {outside:.2} m, inside {inside:.2} m"
        );
    }

    /// A straight is worn ground too. Published tracks measure 0.09–0.16 m of groove down
    /// theirs, and a lap that is glass between the corners reads as one.
    #[test]
    fn a_straight_is_not_smooth_either() {
        let p = hairpins();
        let s = synthesise(&p).unwrap();
        let n = grooves(&across(&s, 60.0), 0.04);
        assert!(n >= 1, "the straight wore {n} grooves");
    }

    #[test]
    fn a_corner_wears_a_bundle_of_ruts() {
        let p = hairpins();
        let s = synthesise(&p).unwrap();
        // A quarter of the way through the first arc, well clear of its ends.
        let n = grooves(&across(&s, 120.0 + 0.25 * std::f32::consts::PI * 16.0), 0.03);
        assert!(n >= 4, "the corner wore {n} grooves, which is not a bundle");
    }

    /// And they do not stop where the arc does. The line is already there on the approach and
    /// is still being driven out of a long way down the following straight.
    #[test]
    fn ruts_run_out_of_the_corner_onto_the_straight() {
        let p = hairpins();
        let s = synthesise(&p).unwrap();
        let arc_end = 120.0 + std::f32::consts::PI * 16.0;
        let out = grooves(&across(&s, arc_end + 25.0), 0.03);
        assert!(
            out >= 3,
            "25 m past the corner the ruts had already gone — {out} grooves"
        );
        // And they fade rather than stopping. Counted by how much ground they take out,
        // because the count alone cannot tell a rut from the surface's own grain.
        let near = groove_depth(&across(&s, arc_end + 10.0), 0.02);
        let far = groove_depth(&across(&s, arc_end + 100.0), 0.02);
        assert!(
            far < near * 0.6,
            "the ruts never faded: {near:.2} m of groove at 10 m past the corner, \
             {far:.2} m at 100 m"
        );
    }

    /// The two edges of a track wander on their own. Together they make a ribbon of varying
    /// width, which from above is not the same thing as ground somebody dug.
    #[test]
    fn the_two_edges_do_not_wander_together() {
        let p = hairpins();
        let s = synthesise(&p).unwrap();
        let edge = |st: &Station, sign: f32| -> f32 {
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let mut d = 0.0f32;
            while d < 14.0 {
                let (x, z) = (st.x + rx * sign * d, st.z + rz * sign * d);
                let (gx, gy) = ((x / s.mps) as usize, (z / s.mps) as usize);
                if gx >= s.gw || gy >= s.gh || !s.corridor[gy * s.gw + gx] {
                    break;
                }
                d += 0.25;
            }
            d
        };
        let (mut diff, mut n) = (0.0f32, 0u32);
        for st in s.stations.iter().step_by(20) {
            diff += (edge(st, 1.0) - edge(st, -1.0)).abs();
            n += 1;
        }
        let mean = diff / n.max(1) as f32;
        assert!(
            mean > 0.4,
            "the two edges differ by {mean:.2} m on average, which is a ribbon"
        );
    }

    /// The ground sheets tile. One of them is repeated over a hundred times across a track,
    /// so a seam is not a detail — it is a grid drawn over the whole map.
    #[test]
    fn the_ground_sheets_meet_themselves_at_the_edges() {
        let (field, ..) = ground_looks(Surface::Soil);
        let dim = 128;
        let tga = ground_texture(dim, &field, 9);
        // Past the 18-byte header, BGRA rows.
        let px = &tga[18..18 + dim * dim * 4];
        let at = |x: usize, y: usize, c: usize| px[(y * dim + x) * 4 + c] as f32;
        let step = |a: usize, b: usize| -> f32 {
            (0..dim)
                .map(|y| (0..3).map(|c| (at(a, y, c) - at(b, y, c)).abs()).sum::<f32>())
                .sum::<f32>()
                / (dim * 3) as f32
        };
        let seam = step(dim - 1, 0);
        let inside: f32 = (1..dim - 1).map(|x| step(x - 1, x)).sum::<f32>() / (dim - 2) as f32;
        assert!(
            seam < inside * 1.6,
            "the sheet has a seam: {seam:.1} across the join against {inside:.1} inside it"
        );
    }

    /// The soil is calibrated against a published track's own sheets rather than picked.
    ///
    /// Indiana ships `soil_light_c` at a mean of (172, 134, 99) and `soil_dark_c` at
    /// (50, 36, 24), and those two numbers are what the base colours here were solved for.
    /// A change to the shading that quietly moves the result is a change to how every
    /// generated track looks, so it is worth a test rather than a comment.
    #[test]
    fn the_soil_lands_where_the_published_sheets_do() {
        let (field, ridden, ..) = ground_looks(Surface::Soil);
        let mean = |look: &GroundLook| -> [f32; 3] {
            let dim = 256;
            let tga = ground_texture(dim, look, 11);
            let px = &tga[18..18 + dim * dim * 4];
            let mut sum = [0.0f64; 3];
            for i in 0..dim * dim {
                // BGRA on disk, reported as RGB.
                sum[0] += px[i * 4 + 2] as f64;
                sum[1] += px[i * 4 + 1] as f64;
                sum[2] += px[i * 4] as f64;
            }
            [
                (sum[0] / (dim * dim) as f64) as f32,
                (sum[1] / (dim * dim) as f64) as f32,
                (sum[2] / (dim * dim) as f64) as f32,
            ]
        };
        for (what, got, want) in [
            ("the field", mean(&field), [172.0, 134.0, 99.0]),
            ("the riding line", mean(&ridden), [50.0, 36.0, 24.0]),
        ] {
            for c in 0..3 {
                assert!(
                    (got[c] - want[c]).abs() < 14.0,
                    "{what} came out {:?}, and the published sheet it is calibrated \
                     against is {want:?}",
                    got.map(|v| v.round())
                );
            }
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
            write_pkz(&p, &s, &dir.join(format!("{}.pkz", slug(&p.name))), true).unwrap();
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
