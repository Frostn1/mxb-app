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
///
/// Six, not nine. At nine the track sat in a shelf whose ground rejoined the field 19.5 m from
/// the centreline against Indiana's 12.5, and it reads as a soft halo round every corner where
/// a real track's edge is nearly flush with the field. The measurement is a crude one — it
/// fits a straight line through the ground either side and asks where the profile leaves it,
/// and it returns nonsense on the flattest sections — so the picture decided this rather than
/// the number: side by side at 6 m the edge is crisp and at 9 m it is not.
const SHOULDER_M: f32 = 6.0;

/// Half the width of the packed strip the tyres leave, metres. Two of these across is a bike
/// and a bit either side, which is what a line worn into a track actually measures.
const RUT_HALF_WIDTH_M: f32 = 1.35;

/// The corner radius at which the racing line leans as far inside as it will go. Tighter than
/// this and it is already all the way over; a 40 m sweeper barely moves off centre.
const FULL_LEAN_RADIUS_M: f32 = 14.0;

/// How close to the edge of the track the racing line is willing to run, metres. It is a line,
/// not a wall-ride: leaving this much between it and the shoulder is what keeps a berm
/// readable as something outside the line rather than part of it.
const LINE_KEEPS_OFF_EDGE_M: f32 = 1.6;

/// Metres of lap the racing line's lean is smoothed over. This is what makes it enter a corner
/// wide, tighten through it and drift out again, out of nothing but per-station curvature.
const LINE_LEAN_SMOOTH_M: f32 = 34.0;

/// Metres of lap the track's own elevation is smoothed over. Short enough to follow a hill,
/// long enough not to follow a bush.
///
/// Not what sets how steep the lap gets, which is worth writing down because it looks like it
/// should be: taking it from 45 m to 24 m moved the steepest grade along the riding line from
/// 12.9° to 13.8° and made the track follow every hummock. The grade comes from the landscape
/// — its amplitude against its wavelength — and that is a choice per track rather than a
/// constant here. Published tracks run 9.1–22.5° at the ninetieth and the spread is real.
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

/// Over how much height the one becomes the other.
///
/// Which of the two a cell gets used to be a branch on whether the ground stood above the
/// deck or below it, and the two reach very different distances — three metres against ten on
/// a six-metre shoulder. So along every contour where the natural ground crosses the deck,
/// one cell stopped grading while the cell beside it was still pulling the ground a third of
/// the way down: a wall 1.3 m high, eleven metres off the line, running the length of the
/// crossing. Ridden, that is the step in the ground.
///
/// A metre, so the machine's reach changes over about the height of the face it is cutting.
const BENCH_BLEND_M: f32 = 1.0;

/// How far the grading reaches here: a cut's short face, a fill's long slope, or between.
fn bench_shoulder(ground: f32, deck: f32) -> f32 {
    let cut = smoothstep(((ground - deck) / BENCH_BLEND_M).clamp(0.0, 1.0));
    FILL_SHOULDER + (CUT_SHOULDER - FILL_SHOULDER) * cut
}

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
///
/// Under two, and measured rather than chosen. At 3.5 m it is too coarse to register at the
/// half-metre scale a rider feels: the surface read 1.26 cm two metres off the line where
/// Indiana reads 2.51. Ground that has been ridden is chopped up at the scale of a wheel, not
/// at the scale of a jump.
const TEXTURE_WAVELENGTH_M: f32 = 1.8;

/// And how far down the scales that texture carries.
///
/// Four octaves at a half gain put a fifth of the surface's energy below half a metre, and
/// measured as roughness after a one-metre detrend that is a lap running 0.012 m against
/// Indiana's 0.007 — half again as rough as a real track at the scale a wheel bounces on,
/// everywhere across the width. Worked ground is lumpy at the scale of a clod and smooth
/// under that; it is not fractal all the way down.
/// The steepest the ground outside the riding corridor is allowed to stand, how much of the
/// excess it sheds each pass, and how many passes it gets.
///
/// Thirty-eight degrees is about what bladed dirt holds; anything standing steeper than that
/// beside a track is a seam rather than a slope, and a rider hits it. The corridor itself is
/// never touched — a berm is meant to be steep.
/// How far clear of the corridor the slump starts, and over how much it comes in.
///
/// A berm is meant to be steep and it stands at the edge of the track, so the slump has to
/// begin outside it — and smoothly, because the corridor's own edge wobbles from cell to cell
/// and a hard boundary put the teeth back that the wobble was there to avoid.
const SEAM_KEEP_OUT_M: f32 = 2.5;
const SEAM_RAMP_M: f32 = 3.0;

/// How much of the carved line survives on a straight, where nobody is on one line.
const CARVE_STRAIGHT: f32 = 0.18;

const SEAM_SLOPE_DEG: f32 = 38.0;
const SEAM_SLUMP: f32 = 0.25;
const SEAM_PASSES: u32 = 60;

/// How far along the track the ridden ground is averaged, in how many taps, and how much of
/// the result is taken.
///
/// Half a metre either way: long enough to take out what a wheel would bounce on, short
/// enough to leave a braking bump — those run at two metres and up — most of its height.
const RIDDEN_SMOOTH_M: f32 = 0.5;
const RIDDEN_SMOOTH_TAPS: u32 = 3;
const RIDDEN_SMOOTH: f32 = 0.85;

const TEXTURE_OCTAVES: u32 = 3;
const TEXTURE_GAIN: f32 = 0.34;

/// The same, for the field that lays out where the grooves go.
const RUT_FIELD_OCTAVES: u32 = 2;
const RUT_FIELD_GAIN: f32 = 0.32;

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
///
/// This is how deep the tyre *cuts* at the deepest groove in the tightest corner on the lap,
/// and it is the smaller half of what a rider meets. The other half is the wall of material
/// the cut threw up beside it (see [`RUT_LIP_GAIN`]), and the published figures above measure
/// the pair — a groove's depth below the ground either side of it, not below where the ground
/// started. Reading them as a cut is what put a half-metre trench through every corner with
/// nothing beside it to lean on.
// Measured against Indiana on the same statistic the corpus survey prints, which is the only
// way to compare: at 0.38 a built lap came back with corner grooves at p50 0.13 and p90 0.24
// against Indiana's 0.21 and 0.44 — half the depth, and a corner you can see but not sit in.
const RUT_DEPTH_M: f32 = 0.50;
const RUT_DEPTH_STRAIGHT_M: f32 = 0.09;

/// The material the cut displaced, which does not disappear.
///
/// It stands outboard of the groove — to the outside of the bend, and on a straight to
/// whichever side of the bundle the groove already lies — as a low smooth wall about a tyre
/// across. This is a rut as anyone rides one: not a hole, but a shallow floor with a bank on
/// its outer side that a rider leans against.
///
/// Sampling the same field a lip-width inboard is what puts a wall beside every groove and
/// none on the ground between two of them. Placing them instead puts a mound wherever two
/// grooves merged, which is the one place there is no material to have come from.
const RUT_LIP_OFFSET_M: f32 = 1.05;
/// How high the wall stands, against how deep the cut is. Taller than the cut is deep: the
/// floor is packed down over a day and the bank is loose material piled on undisturbed ground.
// Raised so a groove comes with something to lean on. What was asked for from the seat is a
// "mini berm" — the bank is the half you use, and at parity with the cut it was barely there.
const RUT_LIP_GAIN: f32 = 1.45;

/// Where the field stops being ground and starts being wall, and where the wall tops out.
///
/// A rut is not a wave. Indiana's corners are flat on the bottom, flat between one groove and
/// the next, and change over about a metre in between — a rider settles *into* one and the
/// ground beside it is ground. Ours was the raw noise field, which curves everywhere and is
/// flat nowhere, so a corner came out as one continuous undulation with no floor to sit in.
/// That is what "rough" means with a cross-section measuring 0.24 m peak to peak against
/// Indiana's 0.64: not deeper, just never still.
///
/// So the field is saturated rather than scaled. Below the first number it is untouched
/// ground, above the second it is floor, and the span between the two is the wall. The lip
/// takes a wider span because piled material stands at a shallower angle than a tyre cuts.
/// Widened after riding one. Saturating hard gave a groove a flat floor and a wall that
/// arrives over a fifth of the field's range, and from the seat that reads as "just cut in the
/// ground" with "a rough edge, straight cut instead of round". A rut a rider wants is closer
/// to a mini berm: a rounded trough with material banked beside it. Over twice the span the
/// same depth arrives as a curve.
const RUT_EDGE: (f32, f32) = (0.02, 0.46);
const RUT_LIP_EDGE: (f32, f32) = (0.0, 0.52);

/// How much of the packed strip is there whatever the ground did, and how much of it is the
/// floor of a groove; and how far the wall beside a groove is left out of it.
///
/// Not all or nothing. A racing line is packed along its whole length — that is what makes it
/// a line — but it is packed *hardest* where the wheels run, and the ruts are the record of
/// where that is. Zero on the first number paints the line only inside its grooves, which
/// leaves gaps a rider reads as holes in the track.
/// Harder contrast, and taken from the right place. Dropping the floor from 0.45 to 0.20 was
/// the first answer to "the shadow from a rut to the ground has to be harder", and it is a
/// bad one: the floor is what the line is painted with *everywhere* — down a straight, and up
/// the fan of scuffs on a jump face — so lowering it made the whole line fainter and the
/// marks on a face all but disappear. Reported back as the texture being broken and the line
/// impossible to find, which was the opposite of the intent.
///
/// The contrast has to come from the gap between floor and wall, not from taking the floor
/// away: a line solid enough to follow, with the ground beside each groove pulled hard out of
/// it.
const RUT_PAINT_FLOOR: f32 = 0.38;
const RUT_PAINT_WALL: f32 = 1.55;

/// How much darker the floor of a groove is than the line it is worn into, and how much
/// lighter dry loose dirt is than the ground it lands on.
///
/// Both were the wrong way round after the bands were re-cut: the packed sheet is lighter
/// than the dark soil of the line, so a rut read paler than its own line and disappeared, and
/// the loose band was toned darker than a corridor that had become the field's own soil, so
/// it read as blotches rather than as dust.
const RUT_FLOOR_DARKEN: f32 = 0.78;
const LOOSE_DRY: f32 = 0.85;

/// How much lighter the worked corridor is than the line worn down the middle of it.
///
/// The corridor used to take the ridden sheet across its whole width, which is a black ribbon
/// edge to edge; then it took the field's own pale soil, which is a track with no dirt on it
/// at all and was worse. It is neither: bladed ground is dark brown, and the line ridden into
/// it is darker still.
const CORRIDOR_LIFT: f32 = 1.45;

/// How much of the packed sheet is available off the racing line, where the ground still has
/// grooves in it but no strip was ever painted.
const RUT_PAINT_OFF_LINE: f32 = 0.30;

/// Over how many metres the ground painted inside the corridor fades out at its edge.
///
/// Whatever this is, the masks have to stop *past* it. A cut inside the fade is a hard
/// boundary at the resolution of the terrain grid — a sawtooth a cell deep and two long,
/// running the length of the track — and it is the first thing the eye finds in a corner.
const RUT_CORRIDOR_FADE_M: f32 = 1.6;

/// How wide the ridden line is either side of the racing line, how much wider it gets through
/// a corner, and how far its own edge fades.
///
/// Measured off what a corner looks like rather than picked: riders take a straight in a file
/// about three metres wide and a corner across most of it.
const LINE_HALF_WIDTH_M: f32 = 2.1;
const LINE_CORNER_SPREAD: f32 = 0.85;
const LINE_FADE_M: f32 = 0.8;

/// And how strongly the wall beside a groove takes the dry, loose sheet instead, and how
/// quickly it gets there. A bank is loose over all of itself, not in proportion to how tall it
/// happens to be, so the signal saturates well before its own peak.
const RUT_LIP_LOOSE: f32 = 1.0;
const RUT_LIP_SHARP: f32 = 3.4;

/// Tyre marks up the face of a jump: the grade at which a face is fully marked, how much wider
/// the marks fan than the line that fed them, how far they lean towards the side the approach
/// delivers, and how far back "the approach" is.
///
/// Sixty metres because that is about a corner exit and the run to the next lip — far enough
/// back that the number says which way the last turn threw you, near enough that it is still
/// the same piece of track.
/// How steep the ground has to climb before it counts as a face worth marking. Lower, so the
/// whole ramp is marked rather than only its steepest third.
const RUT_MARK_FACE: f32 = 0.13;
const RUT_MARK_FAN_M: f32 = 2.4;
const RUT_MARK_LEAN: f32 = 0.55;
const RUT_MARK_LOOKBACK_M: f32 = 60.0;

/// Metres either side of a station the face grade is measured over. A jump's ramp is about ten
/// metres long, so three of them across is steep enough to be on the face and short enough not
/// to average the crest in with it.
const FACE_STEP_M: f32 = 1.5;

/// Ruts do not come one at a time.
///
/// Everyone takes roughly the same line through a corner, but nobody takes exactly the same
/// one, and what a day of practice leaves is a *bundle* of parallel grooves lying across most
/// of the track — six to ten of them through a tight turn, each a little shallower than its
/// neighbour. It is the single most recognisable thing in a real track's collision terrain:
/// Indiana's corners are combs, and one groove down the middle of a corner is the clearest
/// sign the turn was drawn rather than ridden.
/// Metres between one rut and the next — a tyre, plus the ridge pushed up beside it. It is the
/// field's width across the track, so the grooves it leaves come out about this far apart.
///
/// Two metres, counted off ten published tracks: they carry one to three grooves deep enough
/// to find at a time, 1.75–4.0 m apart, spanning six or seven metres of an eleven-metre line.
const RUT_SPACING_M: f32 = 2.75;

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

/// Metres of lap over which the rut field changes — how long a groove runs before it is a
/// different groove.
///
/// This against [`RUT_SPACING_M`] is what makes the field anisotropic, and the ratio is the
/// whole thing: twenty to one is a rut, one to one is gravel. Indiana's own figure is about
/// ten metres — a cross-section still matches the one two metres behind it four fifths of the
/// way, half of it at five metres, and by twenty it is different ground.
const RUT_ALONG_M: f32 = 34.0;

/// Metres between braking bumps.
///
/// How *far* they run is no longer a constant. It was 22 m before anything under a
/// forty-metre radius, which gave a 90 km/h approach to a hairpin and a 40 km/h approach to a
/// flat left the same washboard. A braking zone is as long as the braking is, and
/// [`crate::trackspeed`] is what knows that.
const BRAKING_WAVELENGTH_M: f32 = 2.2;

/// How much rougher the surface gets in and around a corner, as a multiplier on the texture.
///
/// One, not two. Measured along the centreline over a six-metre detrend, Indiana's corners run
/// 0.069 m rms against 0.097 on its straights — a corner there is *smoother* under the wheels
/// than the straight before it, because the ruts carry the relief and the ground between them
/// is packed flat. There is no case for making ours the rougher of the two.
///
/// It is not, though, the reason a corner felt rough: taking this from 1.8 to 0.9 moved the
/// measured corner roughness by 0.002 m, because the chatter in a corner is the braking and
/// acceleration bumps, which are sized on their own. What made a corner ride badly was the
/// cross-section — see [`RUT_EDGE`].
const CORNER_ROUGHNESS: f32 = 1.0;

/// How tall a braking bump stands, trough to crest, metres.
///
/// Stated outright rather than as a multiple of the surface texture. Braking bumps are a
/// feature of their own — a washboard laid across the track, deep enough to move a bike —
/// and tying their height to the fine grain meant a track with a smooth surface got no
/// braking bumps either, which is backwards.
const BRAKING_HEIGHT_M: f32 = 0.13;

/// How far apart the chop everyone's rear wheel leaves on the way out of a corner is, and how
/// tall it stands. Longer and lower than braking: acceleration bumps are stretched out by the
/// wheel spinning across them.
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
/// And the field is not ridden. Seven and a half centimetres of detail everywhere left the
/// field nearly as rough as the racing line — 1.07 cm against 1.26 — where a published track's
/// field is a quarter of its line.
/// How many octaves the landscape carries and how fast they fall away. See [`fbm_of`].
const LANDSCAPE_OCTAVES: u32 = 3;
const LANDSCAPE_GAIN: f32 = 0.30;

const FIELD_DETAIL_M: f32 = 4.5;
const FIELD_DETAIL_HEIGHT_M: f32 = 0.045;

/// The hollow a jump is dug out of, as a fraction of its height, and how far past its ends
/// that hollow reaches.
///
/// Every cubic metre standing in a jump came out of the ground beside it, and on a real track
/// you can see where. Indiana's average jump profile, normalised against its own height, sits
/// at −0.26 twenty metres before the crest and −0.32 twenty metres after, with the lip itself
/// at +0.97: the jump is a hump between two scoops.
const JUMP_HOLLOW: f32 = 0.30;
const JUMP_HOLLOW_M: f32 = 22.0;

/// Metres between samples of the profiles that run along the lap.
///
/// Everything built on the track is a function of how far round it you are, and the cells
/// look that function up rather than reading the nearest station's copy of it. Taking the
/// nearest station's value directly puts the chamfer's own jagged label boundaries into the
/// terrain: two neighbouring cells can be assigned stations several metres apart, and on the
/// face of a jump that is a step in the ground you can see across the whole straight.
const PROFILE_STEP: f32 = 0.1;

/// The resolution the exported `.tga` masks are written at.
///
/// The terrain's own, not half of it. A groove is a metre or so across and the paint now
/// follows the grooves; at 1024 over a 500 m track that is two pixels a rut, which is a
/// smudge. The `.map` the game actually reads has always written its masks at the grid's
/// resolution — this only brings the TerrainEd source files up to the same ground.
const MASK_DIM: usize = 2048;
/// How hard the ground's own luma pushes its normals. Enough to catch light on ruts and
/// chop without turning the soil grain into relief.
const NORMAL_STRENGTH: f32 = 6.0;
/// The same, for a source `_n.tga` rather than the encoding baked into a `.map`.
///
/// Gentler, because there the whole normal is written rather than just its direction. Two
/// lands on the example track's own sheet: blue averaging 235 against its 241, and red
/// spread across [19, 236] against its [9, 246].
const SHEET_NORMAL_STRENGTH: f32 = 2.0;

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
/// The loose dirt tiles coarser than the line it sits on, so the two read as different ground
/// and not as one sheet at two brightnesses.
const TILE_LOOSE_M: f32 = 4.1;
/// And the packed line finer, which is what being driven over does to it.
const TILE_RUT_M: f32 = 2.4;

/// The cube a wet layer reflects, per face. Small on purpose: it is seen smeared across a
/// film of water and never in focus. The example track's own faces are 128 too.
const ENV_FACE_DIM: usize = 128;

/// How far rain darkens the ground.
///
/// Measured against the example track's pair: `mud_wet.tga` averages (52.5, 35.8, 20.9)
/// where `mud.tga` averages (68.6, 52.2, 36.5) — about three quarters of it, with the
/// chroma left alone. Wet soil is darker soil, not bluer soil.
const WET_DARKEN: f32 = 0.74;

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
    /// What the ruts did to each cell, as a fraction of the local rut depth: -1 on the floor
    /// of a groove, positive on the wall standing beside it, 0 on ground no rut reached.
    ///
    /// Carried out of synthesis because the paint needs it. A rut is only half shape — the
    /// other half is being able to see it, and a mask keyed off the racing line alone has no
    /// way of knowing where inside that line the grooves actually fell.
    pub rut: Vec<f32>,
    /// How steeply the built ground climbs along the lap at each station — the grade of a
    /// jump's face, positive up a takeoff and negative down a landing.
    pub face: Vec<f32>,
    /// Metres from the centreline.
    pub dist: Vec<f32>,
    /// Metres round the lap.
    pub arc: Vec<f32>,
    /// Which station each cell is nearest. Lets a mask ask the line's heading and curvature
    /// where the cell is, which is what tells it the rider's left from their right.
    pub station: Vec<u32>,
    pub stations: Vec<Station>,
    /// Where the racing line sits across the track at each station, signed metres, positive
    /// to the rider's right. Smoothed along the lap, so it leans into a corner before the
    /// corner and drifts back out after it rather than stepping across at the seams.
    pub line_lat: Vec<f32>,
    /// The start straight beside the lap, and where each cell sits on it — how far off its
    /// centreline, and how far along. `spur_dist` is `f32::MAX` where there is no start.
    pub spur: Option<StartSpur>,
    pub spur_dist: Vec<f32>,
    pub spur_arc: Vec<f32>,
    /// What the terrain actually used of its budget, and what the budget was.
    pub used_m: f32,
    pub budget_m: f32,
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

/// The ground a track is built on, without the track.
///
/// Pulled out of `synthesise` so it can be sampled before a lap exists. That is what lets the
/// lap be *routed* to the terrain rather than dropped on it: the placement is chosen by asking
/// this what the ground does along a candidate line, which is the thing Indiana's builder did
/// with a map and we were not doing at all.
pub struct Landscape {
    tilt: f32,
    tilt_dir: (f32, f32),
    span: f32,
    amplitude: f32,
    wavelength: f32,
    seed: u32,
    /// `(x, z, long, across, height, bearing)` per mound.
    mounds: Vec<(f32, f32, f32, f32, f32, f32)>,
}

impl Landscape {
    pub fn of(prog: &TrackProgram) -> Self {
        let r = &prog.terrain.relief;
        let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
        let mut mounds = Vec::new();
        for n in 0..r.landforms.min(24) {
            // Long enough to be a hill a track is built over, small enough to leave ground
            // between them. With 110–260 m ones the map comes out 27.9% flat against
            // Indiana's 37.3 and curved everywhere at 0.29 m over fifteen metres against its
            // 0.19 — the landforms *are* the landscape. With none at all it is 68% flat,
            // which is flatter than Indiana. The truth is in between.
            //
            // And a mound has to be longer than the distance the lap's own elevation is
            // smoothed over, or it is a bump under the riding line rather than a hill the
            // track climbs.
            let long = 110.0 + 150.0 * (hash2(n as i32, 5, r.seed ^ 0x1A2D) * 0.5 + 0.5);
            if long < BENCH_SMOOTH_M * 1.6 {
                continue;
            }
            let across = long * (0.45 + 0.4 * (hash2(n as i32, 9, r.seed ^ 0x1A2D) * 0.5 + 0.5));
            let h1 = hash2(n as i32 * 71, 13, r.seed ^ 0x1A2D);
            let h2 = hash2(n as i32 * 71, 29, r.seed ^ 0x1A2D);
            let tall = r.landform_height * (0.45 + 0.55 * (hash2(n as i32, 17, r.seed ^ 0x1A2D) * 0.5 + 0.5));
            let bearing = hash2(n as i32, 23, r.seed ^ 0x1A2D) * std::f32::consts::PI;
            mounds.push((
                (h1 * 0.5 + 0.5) * sx,
                (h2 * 0.5 + 0.5) * sz,
                long,
                across,
                tall,
                bearing,
            ));
        }
        Landscape {
            tilt: r.tilt,
            tilt_dir: crate::trackprog::heading_vector(r.tilt_angle.to_radians()),
            span: sx.max(sz).max(1.0),
            amplitude: r.amplitude,
            wavelength: r.wavelength.max(1.0),
            seed: r.seed,
            mounds,
        }
    }

    /// The hillside, its bumps, and the metre-scale grain that makes it read as land.
    pub fn at(&self, x: f32, z: f32) -> f32 {
        let along = (x * self.tilt_dir.0 + z * self.tilt_dir.1) / self.span;
        -self.tilt * along
            + fbm_of(
                x / self.wavelength,
                z / self.wavelength,
                self.seed,
                LANDSCAPE_OCTAVES,
                LANDSCAPE_GAIN,
            ) * self.amplitude
            + fbm(x / FIELD_DETAIL_M, z / FIELD_DETAIL_M, self.seed ^ 0xF1E1D) * FIELD_DETAIL_HEIGHT_M
            + self.mounds_at(x, z)
    }

    /// Just the banks, for a caller that has the rest already.
    pub fn mounds_at(&self, x: f32, z: f32) -> f32 {
        let mut add = 0.0;
        for &(cx, cz, long, across, tall, bearing) in &self.mounds {
            let (dx, dz) = (x - cx, z - cz);
            let (c, s) = (bearing.cos(), bearing.sin());
            let (u, v) = (dx * c + dz * s, -dx * s + dz * c);
            // The outline is warped, not elliptical: an ellipse reads as a flying saucer
            // parked on the ground however you shade it.
            let warp = 0.42
                * fbm(
                    x / (long * 0.55),
                    z / (long * 0.55),
                    self.seed ^ 0x1A2D ^ (long as u32),
                );
            let t = ((u / long).powi(2) + (v / across).powi(2)).sqrt() + warp;
            if t < 1.0 {
                add += tall * (1.0 - smoothstep(t.max(0.0))).powf(0.7);
            }
        }
        add
    }
}

/// Where on its ground a lap should sit.
///
/// A lap is a shape; the plot is a piece of ground; nothing until now decided how the one lay
/// on the other beyond centring it. So the track met whatever the hills happened to be doing
/// there, and the trade came out wrong whichever size the hills were: mounds small enough to
/// give the lap a real climb left it off-camber everywhere, and mounds gentle enough to ride
/// left it flat. Measured against Indiana, 8.3° of grade at the ninetieth with 3.0 m of fall
/// across thirty metres of track — ours managed either 9.0° with 7.6 m, or 8.1° with 5.2 m.
///
/// Indiana's builder did not have that problem because he did not place the track and the
/// hills independently. He ran it along the contours where it wanted to be level and up the
/// fall line where it wanted to climb.
///
/// This is the cheap version of the same idea: the shape is fixed, so the only freedoms are
/// where it sits and which way round it faces, and both are searched. It costs a few hundred
/// samples of the landscape per candidate and buys the difference between a track laid on the
/// ground and one laid across it.
pub struct Placement {
    pub dx: f32,
    pub dz: f32,
    pub turn_deg: f32,
    /// What it scored, and what the untouched placement scored, so a caller can say whether
    /// the search was worth anything.
    pub cross_m: f32,
    pub was_cross_m: f32,
    pub grade_p90_deg: f32,
}

/// Turning a point about the origin by the same angle a heading is turned by.
///
/// Not the textbook rotation matrix. A heading of `theta` points along `(sin, cos)`, so it
/// grows *clockwise* in the x/z plane and the usual `(xc - zs, xs + zc)` turns positions the
/// other way. The search rotated positions with one and headings with the other, so what it
/// scored and what a caller then built were mirror images of each other — the lap came out
/// turned the wrong way round, and on a plot with no room to spare it came out off the edge.
fn turn_point(x: f32, z: f32, c: f32, s: f32) -> (f32, f32) {
    (x * c + z * s, -x * s + z * c)
}

/// Turn and shift a lap the way a [`Placement`] says.
///
/// Here rather than in the caller because it has to be the same arithmetic the search scored
/// with: a placement applied differently from how it was measured is a number about a track
/// nobody built.
pub fn place(prog: &mut TrackProgram, p: &Placement) {
    let (c, s) = (p.turn_deg.to_radians().cos(), p.turn_deg.to_radians().sin());
    let st = prog.stations(SEARCH_STATION_M);
    let (mut lo_x, mut hi_x, mut lo_z, mut hi_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for q in &st {
        lo_x = lo_x.min(q.x);
        hi_x = hi_x.max(q.x);
        lo_z = lo_z.min(q.z);
        hi_z = hi_z.max(q.z);
    }
    let (cx, cz) = ((lo_x + hi_x) * 0.5, (lo_z + hi_z) * 0.5);
    let (rx, rz) = turn_point(prog.start.x - cx, prog.start.z - cz, c, s);
    prog.start.x = cx + rx + p.dx;
    prog.start.z = cz + rz + p.dz;
    prog.start.angle += p.turn_deg;
}

/// How much fall across the track the search will trade a degree of grade for.
///
/// Both matter and they pull against each other. Cross-slope is weighted the harder of the
/// two because a track that is off-camber everywhere is unrideable in a way that a gentle one
/// is merely dull.
const ROUTE_GRADE_TARGET_DEG: f32 = 8.3;
const ROUTE_GRADE_WEIGHT: f32 = 0.35;

/// How often the placement search samples the lap. Shared with [`place`], which turns the lap
/// about the box these points make.
const SEARCH_STATION_M: f32 = 2.0;

/// Route a lap over its ground: pick the rotation and offset whose line has the least fall
/// across it, at about the grade a published track climbs at.
pub fn place_on_ground(prog: &TrackProgram) -> Option<Placement> {
    let land = Landscape::of(prog);
    let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
    let base = prog.stations(SEARCH_STATION_M);
    if base.len() < 16 {
        return None;
    }
    // The lap's own centre, so a rotation turns it about itself rather than about the corner
    // of the plot.
    let (mut lo_x, mut hi_x, mut lo_z, mut hi_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for st in &base {
        lo_x = lo_x.min(st.x);
        hi_x = hi_x.max(st.x);
        lo_z = lo_z.min(st.z);
        hi_z = hi_z.max(st.z);
    }
    let (cx, cz) = ((lo_x + hi_x) * 0.5, (lo_z + hi_z) * 0.5);
    let half = ((hi_x - lo_x).max(hi_z - lo_z)) * 0.5;
    // Room enough that the corridor and its shoulder are inside the plot, not just the
    // centreline — and a little over, because the placement is scored on stations two metres
    // apart and the lap between two of them can bulge past both.
    let margin = prog.width * 2.0 + SHOULDER_M;
    // The start needs more of it than the rest of the lap: the opening straight fans out to
    // hold a 48 m gate row, and a fan hanging off the edge of the plot is gates in the void.
    let run = prog.opening_straight();
    let fan_margin = margin.max(START_FAN_HALF_M + SHOULDER_M);

    let score = |turn: f32, dx: f32, dz: f32| -> Option<(f32, f32, f32)> {
        let (c, s) = (turn.cos(), turn.sin());
        let mut cross: Vec<f32> = Vec::with_capacity(base.len());
        let mut grade: Vec<f32> = Vec::with_capacity(base.len());
        let mut placed: Vec<(f32, f32, f32)> = Vec::with_capacity(base.len());
        for st in &base {
            let (rx, rz) = turn_point(st.x - cx, st.z - cz, c, s);
            let (x, z) = (cx + rx + dx, cz + rz + dz);
            // Anything off the plot disqualifies the whole placement — a lap that leaves its
            // ground is not a candidate however well the rest of it lies.
            let m = if st.s <= run { fan_margin } else { margin };
            if x < m || z < m || x > sx - m || z > sz - m {
                return None;
            }
            placed.push((x, z, st.heading + turn));
        }
        for (i, &(x, z, h)) in placed.iter().enumerate() {
            let (rx, rz) = crate::trackprog::right_vector(h);
            cross.push((land.at(x + 15.0 * rx, z + 15.0 * rz) - land.at(x - 15.0 * rx, z - 15.0 * rz)).abs());
            let (nx, nz, _) = placed[(i + 5) % placed.len()];
            grade.push(((land.at(nx, nz) - land.at(x, z)) / 20.0).abs());
        }
        cross.sort_by(f32::total_cmp);
        grade.sort_by(f32::total_cmp);
        let cross_p90 = cross[cross.len() * 9 / 10];
        let grade_p90 = grade[grade.len() * 9 / 10].atan().to_degrees();
        Some((
            cross_p90 + ROUTE_GRADE_WEIGHT * (grade_p90 - ROUTE_GRADE_TARGET_DEG).abs(),
            cross_p90,
            grade_p90,
        ))
    };

    let was = score(0.0, 0.0, 0.0);
    let mut best: Option<(f32, Placement)> = None;
    for t in 0..24 {
        let turn = t as f32 * std::f32::consts::TAU / 24.0;
        // A rotated lap needs room for its diagonal, so it is nudged back towards the middle
        // rather than left where the rotation put it.
        for gx in -2..=2 {
            for gz in -2..=2 {
                let room = (sx.min(sz) * 0.5 - half - margin).max(0.0);
                let (dx, dz) = (gx as f32 * room * 0.4, gz as f32 * room * 0.4);
                let Some((total, cross_m, grade_p90_deg)) = score(turn, dx, dz) else {
                    continue;
                };
                if best.as_ref().map(|b| total < b.0).unwrap_or(true) {
                    best = Some((
                        total,
                        Placement {
                            dx,
                            dz,
                            turn_deg: turn.to_degrees(),
                            cross_m,
                            was_cross_m: was.map(|w| w.1).unwrap_or(cross_m),
                            grade_p90_deg,
                        },
                    ));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

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
    let land = Landscape::of(prog);
    let mut heights = vec![0.0f32; gw * gh];
    for y in 0..gh {
        for x in 0..gw {
            heights[y * gw + x] = land.at(x as f32 * mps_x, y as f32 * mps_z);
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
    // And cut a pad under everything built on the line, before anything is built on it.
    level_pads(&mut along, &stations, &prog.features);

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
    let mut feel = ride(prog.terrain.surface);
    // How raced the ground arrives. It thins the deformable stack in `tht` and deepens what
    // is already cut here, so the two move opposite ways and their sum stays near constant —
    // which is what stops a heavily raced track digging itself to pieces.
    let worn = worn(prog);
    feel.rut_depth *= worn;
    feel.rut_straight *= worn;
    feel.brake.1 *= worn;
    feel.accel.1 *= worn;
    let ruts = rut_profile(&prog.features, &turn, lap, r.seed, &feel);
    let widths = width_profile(prog.width * 0.5, lap, r.seed);
    // The start straight: its own line off to the side of the lap, cut to the height of the
    // lap beside it. Built here because it takes that deck, and used twice below — to bench
    // it into the ground, and by everything that then paints the track.
    let spur = StartSpur::of(prog, &stations, &along, &land);
    let speed = crate::trackspeed::of(prog);
    let chop = roughness_profile(&turn, &speed, lap);

    // Where the racing line runs across the track. A rider hugs the inside of a corner, so
    // the line leans to whichever side the curvature points at and by more the tighter the
    // corner. Smoothing that over a stretch of lap is what turns it into a line rather than a
    // set of steps: it starts moving across before the corner arrives and drifts back out
    // after it, which is the shape the real one takes and the reason it reads as a line to
    // follow rather than a stripe down the middle.
    let reach = (prog.width * 0.5 - LINE_KEEPS_OFF_EDGE_M).max(0.0);
    let mut line_lat: Vec<f32> = stations
        .iter()
        .map(|st| {
            let lean = (st.curvature.abs() * FULL_LEAN_RADIUS_M).clamp(0.0, 1.0);
            st.curvature.signum() * lean * reach
        })
        .collect();
    smooth_along(&mut line_lat, (LINE_LEAN_SMOOTH_M / STATION_STEP) as usize);

    // On the same even ruler as everything else, so a cell can ask where the line is at
    // *its* distance round the lap. This is what the ruts follow: the painted line and the
    // cut one were computed eight hundred lines apart and never read each other, so the dark
    // ribbon a rider steers by sat metres from the groove they actually dropped into.
    let line = resample(&stations, &line_lat, lap);

    // 3. Bench the corridor in, then build on it.
    let mut corridor = vec![false; gw * gh];
    let mut rut = vec![0.0f32; gw * gh];
    // How ridden each cell is, and which way the track runs there — read by the pass that
    // smooths the ground along its own direction.
    let mut ridden_at = vec![0.0f32; gw * gh];
    let mut heading_at = vec![0.0f32; gw * gh];
    // How wide the corridor is at each cell, so the slump can keep clear of the track by a
    // margin rather than by a boolean. The corridor's own edge wobbles cell to cell: slumping
    // right up to it took bites out of the outside of every berm and left the crest with
    // teeth in it, which is a fault the slump introduced rather than one it fixed.
    let mut edge_at = vec![f32::MAX; gw * gh];
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
        let shoulder = SHOULDER_M * bench_shoulder(ground, deck);
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
            feel.berm
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

        // Ruts: the ground a day of practice leaves, as a field rather than as a row of
        // grooves.
        //
        // Drawing N gaussians at a fixed spacing is the obvious way and it is wrong. Each
        // groove then runs the whole length of the corner at its own constant depth, and five
        // of them come out as five parallel tramlines — regular enough to read as machine-made
        // from directly above, which is what it did. What riders actually leave is one surface:
        // grooves that merge, split, fade out and pick up again, because nobody takes the same
        // line twice. So it is one noise field, stretched along the direction of travel — a
        // couple of metres across, tens of metres long — and every one of those properties
        // falls out of it instead of being arranged.
        let depth = ruts.depth.at(s) * ruts.damp.at(s);
        if depth > 0.0 {
            let spread = ruts.spread.at(s);
            let mid = ruts.centre.at(s) * half;
            let reach = (half * spread).max(feel.rut_spacing) + feel.rut_spacing;
            let focus = ruts.focus.at(s);
            let on_line = line.at(s);

            // The field says what the ground either side looks like. It does not say where
            // anybody rides, and those are different questions.
            //
            // Weighting the field towards the racing line was tried and cannot work: it
            // varies by more than any envelope that still leaves a spread either side, so the
            // deepest groove kept landing wherever the noise happened to peak — 1.9 m off the
            // paint on a hairpin, which is a different line from the one a rider steers by. A
            // rut is not a noise peak that happens to lie under the paint. It is there
            // because that is where everybody rides, so it is stated.
            //
            // Rounded, because a tyre is. The field's own grooves have flat floors and walls
            // — that is `RUT_EDGE`'s job and it is measured — but a carved line given a flat
            // floor has no low point of its own, so what reads as the groove is whichever bit
            // of field noise dips through it.
            let trough = |centre: f32, width: f32| -> f32 {
                let d = ((t - centre) / width).clamp(-1.0, 1.0);
                1.0 - d * d
            };
            // The main line, where the paint says it is, wandering in depth down the lap so
            // it is a rut rather than a channel.
            // Only where a corner puts everybody on the same line. A straight does not carry
            // one groove down the middle of it — riders are spread across it and what they
            // leave is the field's own grooves and the chop under braking. Carved down every
            // straight as well, it reads as a channel somebody dug, which is exactly what it
            // was called from the seat.
            let carve = CARVE_STRAIGHT
                + (1.0 - CARVE_STRAIGHT)
                    * (turn.at(s).abs() * FULL_LEAN_RADIUS_M).clamp(0.0, 1.0);
            let main = trough(on_line, feel.groove)
                * (0.82 + 0.18 * fbm(s / 13.0, 21.0, r.seed ^ 0x11E5))
                * carve;
            // And the corner's other way through: outside the first, shallower, and only
            // where the turn has run long enough to have grown one. Its own variation, or it
            // is the same groove drawn twice.
            //
            // Two more of them, and both sides. One line and a spread either side is not what
            // a ridden corner has: there are three or four ways through it, people take all of
            // them, and which one is deepest changes down the length of the turn. The inside
            // one only exists where the turn is tight enough to have an inside.
            let side = if on_line >= 0.0 { -1.0 } else { 1.0 };
            let other = ruts.second.at(s) * (1.0 - focus);
            let extra = |at: f32, depth: f32, salt: u32, wave: f32| {
                trough(at, feel.groove * 0.9)
                    * depth
                    * other
                    * (0.5 + 0.5 * fbm(s / wave, 39.0, r.seed ^ salt))
            };
            let second = if other > 0.0 {
                // Two lines and the field, not four. Three carved plus the field's own put
                // grooves across the whole width and a rider reported, plainly, too many.
                let outer = extra(on_line + side * RUT_SECOND_M, RUT_SECOND_DEPTH, 0x5EC0, 17.0);
                let inner = extra(
                    on_line - side * RUT_SECOND_M * 0.9,
                    RUT_SECOND_DEPTH * 0.62,
                    0x5EC2,
                    29.0,
                );
                outer.max(inner)
            } else {
                0.0
            };

            // And the marks a face carries. A takeoff is the one place everybody's line is
            // written down: they arrive off the last corner wherever it left them and scrub up
            // the ramp from there, so a face wears a fan of scuffs rather than the single
            // groove a straight does. Cut here as well as painted, because a mark you can only
            // see is a decal.
            let marks = if focus > 0.0 {
                let span = feel.groove + RUT_MARK_FAN_M * focus;
                let d = (t - on_line) / span;
                if d.abs() < 1.0 {
                    let comb = 0.5
                        + 0.5
                            * ((t - on_line) / TYRE_MARK_SPACING_M * std::f32::consts::TAU).cos();
                    (1.0 - d * d) * comb * TYRE_MARK_DEPTH * focus
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let carved = main.max(second).max(marks);

            let off = t - mid;
            if off.abs() <= reach || carved > 0.0 {
                let fade = 1.0 - (off.abs() / reach).min(1.0).powi(2);
                // Two octaves, not four. A groove field summed over four octaves carries as
                // much shape at a third of the spacing as at the spacing itself, so a corner
                // came out with half again as many grooves as a real one, packed 2.0 m apart
                // against Indiana's 2.5, each with a floor a fifth narrower. A rut is one
                // wavelength with a bottom on it — see `RUT_EDGE` — not a spectrum.
                let field = |at: f32| {
                    fbm_of(
                        at / feel.rut_spacing,
                        s / RUT_ALONG_M,
                        r.seed ^ 0x2117,
                        RUT_FIELD_OCTAVES,
                        RUT_FIELD_GAIN,
                    )
                };
                // Ground, wall, floor — see `RUT_EDGE`. Saturating the field rather than
                // scaling it is what gives a groove a bottom to sit on and leaves the ground
                // between two of them flat.
                let shape = |v: f32, e: (f32, f32)| {
                    smoothstep(((v - e.0) / (e.1 - e.0)).clamp(0.0, 1.0))
                };
                let cut = shape(field(off), RUT_EDGE);
                // And the material that came out of it, standing on the groove's outer side.
                //
                // Which side that is comes from the bend, and on a straight from which side
                // of the bundle the cell is on — blended between the two rather than switched,
                // because a hard flip at the middle of a straight's bundle draws a seam down
                // the length of it. The two candidate walls are sampled and mixed, not the
                // offset: mixing the offset instead samples the groove itself at the middle
                // and fills in the very rut it is meant to bank.
                let k = turn.at(s);
                let bend = (k.abs() * FULL_LEAN_RADIUS_M).clamp(0.0, 1.0);
                let outward = -k.signum() * bend
                    + (off / feel.rut_spacing).clamp(-1.0, 1.0) * (1.0 - bend);
                let w = 0.5 * (1.0 + outward.clamp(-1.0, 1.0));
                let lip = shape(field(off + RUT_LIP_OFFSET_M), RUT_LIP_EDGE) * (1.0 - w)
                    + shape(field(off - RUT_LIP_OFFSET_M), RUT_LIP_EDGE) * w;
                // A jump sits on a straight, so it used to take the straight's whole bundle
                // across its width: half a dozen gouges up a takeoff ramp and no line among
                // them. Everybody hits a face in the same place, so the field goes and the
                // carved line stays.
                let spread = (lip * RUT_LIP_GAIN - cut) * fade * (1.0 - focus);
                // Held as a signal of its own as well as added to the ground: what a rider
                // reads off a rut is half its shape and half the paint on it, and the paint
                // cannot follow a groove it has no way of knowing is there. The carved lines
                // go into the same signal, or the paint would follow the field and miss them.
                let relief = if carved > 0.0 { spread.min(-carved) } else { spread };
                heights[i] += depth * relief;
                rut[i] = relief;
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
        ridden_at[i] = w;
        heading_at[i] = stations[station[i] as usize].heading;
        edge_at[i] = half;
        // Worn hardest where the wheels are. Riders use the middle of a track and the edges
        // barely at all, so the ridden texture tapers across it rather than covering the
        // corridor evenly — which is what it did, and it is measurable: Indiana's surface
        // roughness halves between two metres off the line and six, while ours barely moved.
        // The braking chop below already tapers this way; the surface it sits on did not.
        let across = (1.0 - (d / half.max(1e-3)).min(1.0).powi(2)) * w;
        // But smoothest of all *on* the line, which is the opposite of what tapering alone
        // does. A packed line is the one strip of a track a thousand wheels have polished, and
        // covering it in the same grain as the ground beside it is a lap that will not let a
        // rider carry any speed — reported from the seat as chop everywhere on the straights
        // taking away everything the bike makes. Where the line is deepest the grain is least.
        let polished = 1.0 - POLISHED * trough_at(t, line.at(s), feel.groove * 2.1);
        if r.texture > 0.0 && w > 0.0 {
            let (wx, wz) = ((i % gw) as f32 * mps_x, (i / gw) as f32 * mps_z);
            let gain = chop.rough.at(s);
            heights[i] += fbm_of(
                wx / TEXTURE_WAVELENGTH_M,
                wz / TEXTURE_WAVELENGTH_M,
                r.seed ^ 0x5EED,
                TEXTURE_OCTAVES,
                TEXTURE_GAIN,
            ) * r.texture
                * gain
                * polished
                * (0.25 * w + 0.75 * across);
            // The seams between the machine's passes, running the way it drove.
            heights[i] -= ((t / PASS_SPACING_M) * std::f32::consts::TAU).sin().abs()
                * PASS_DEPTH_M
                * w;

            // Braking bumps on the way into a corner, which is the direction they form in.
            // The phase drifts, because bumps that are a perfect sine read as corrugated iron.
            let brake = chop.braking.at(s);
            if brake > 0.0 && across > 0.0 {
                let drift = 0.35 * fbm(s / 26.0, 7.0, r.seed ^ 0xB4AE);
                let ripple =
                    ((s / feel.brake.0 + drift) * std::f32::consts::TAU).sin();
                heights[i] += ripple * feel.brake.1 * 0.5 * brake * across * polished;
            }
            // And the longer, lower chop everybody's rear wheel leaves on the way out.
            let out = chop.accel.at(s);
            if out > 0.0 && across > 0.0 {
                let drift = 0.4 * fbm(s / 31.0, 13.0, r.seed ^ 0xACCE);
                let ripple = ((s / feel.accel.0 + drift) * std::f32::consts::TAU).sin();
                heights[i] += ripple * feel.accel.1 * 0.5 * out * across * polished;
            }
        }
    }

    // 3a. Break the walls where two parts of the track grade into the same ground.
    //
    // Every cell takes its arc position from the station nearest it, and out in the field
    // between two branches of the lap that assignment flips: measured on the demo, two
    // neighbouring samples eleven metres off the line belong to stations a hundred and ten
    // metres apart round the lap. The deck height, a jump reaching out of the corridor and
    // the berm are all functions of that position, so where it flips they disagree — and the
    // ground between a hairpin's two legs came out with a wall 1.3 m high standing in it,
    // one sample wide. Ridden, that is a step in the ground you can hit.
    //
    // A machine grading between two legs of a track leaves one surface, so this makes one:
    // outside the riding corridor, anything standing far off its own neighbours is pulled
    // back towards them. Soft-thresholded, so ordinary relief is untouched and only a wall is
    // treated as a wall, and the corridor itself is never moved.
    // Averaging cannot do it: run to convergence a blur turns a wall into a ramp of exactly
    // the same drop, and a 60-degree ramp one sample wide is the same thing to ride into. So
    // this is the slump instead — ground steeper than it can stand loses material downhill,
    // pass after pass, until nothing outside the corridor is steeper than a graded slope.
    {
        let limit = SEAM_SLOPE_DEG.to_radians().tan() * mps_x.min(mps_z);
        for _ in 0..SEAM_PASSES {
            let src = heights.clone();
            let mut moved = 0.0f32;
            for i in 0..gw * gh {
                let (x, y) = (i % gw, i / gw);
                if corridor[i] || x == 0 || y == 0 || x + 1 == gw || y + 1 == gh {
                    continue;
                }
                // Clear of the track and whatever stands at its edge, then fading in.
                let past = dist[i] - edge_at[i] - SEAM_KEEP_OUT_M;
                let strength = smoothstep((past / SEAM_RAMP_M).clamp(0.0, 1.0));
                if strength <= 0.0 {
                    continue;
                }
                let mut drop = 0.0;
                for j in [i - 1, i + 1, i - gw, i + gw] {
                    // Never into the corridor. The face where a track is cut into rising
                    // ground is meant to be steep, and shedding material into it digs a moat
                    // down the side of the track instead of taking a seam out of the field.
                    if corridor[j] {
                        continue;
                    }
                    let over = src[i] - src[j] - limit;
                    if over > 0.0 {
                        drop += over * SEAM_SLUMP;
                    }
                }
                if drop > 0.0 {
                    heights[i] -= drop * strength;
                    moved += drop * strength;
                }
            }
            if moved < 1e-3 {
                break;
            }
        }
    }

    // 3b. Smooth the ridden ground along the way it was ridden.
    //
    // A rut is a groove somebody drove down: sharp across, smooth along. Everything above
    // builds it out of fields that are equally rough in both directions, and the difference
    // is measurable — Indiana's ground carries 0.007 m of roughness after a one-metre
    // detrend and ours carried 0.012, half again as much, at exactly the scale a wheel
    // bounces on. Two thirds of that came from the rut carving and the ridden texture, and
    // neither can simply be turned down: they are also what makes the groove.
    //
    // So it is taken out where it does not belong rather than never put in. Averaging along
    // the track direction leaves anything that runs *with* the track — a groove, a berm, the
    // line — untouched, and takes the edge off anything that does not.
    //
    // Braking chop runs across the track and so is attenuated by this; `RIDDEN_SMOOTH_M` is
    // kept under a quarter of the shortest chop wavelength for that reason.
    {
        let src = heights.clone();
        let taps = RIDDEN_SMOOTH_TAPS as i32;
        for i in 0..gw * gh {
            let w = ridden_at[i];
            if w <= 0.0 {
                continue;
            }
            let (fx, fz) = crate::trackprog::heading_vector(heading_at[i]);
            let (cx, cz) = ((i % gw) as f32 * mps_x, (i / gw) as f32 * mps_z);
            let mut sum = 0.0;
            let mut norm = 0.0;
            for k in -taps..=taps {
                let d = k as f32 * RIDDEN_SMOOTH_M / taps as f32;
                let g = sample_smooth(&src, gw, gh, (cx + fx * d) / mps_x, (cz + fz * d) / mps_z);
                let weight = 1.0 - (k.abs() as f32 / (taps + 1) as f32);
                sum += g * weight;
                norm += weight;
            }
            let smoothed = sum / norm.max(1e-6);
            heights[i] += (smoothed - heights[i]) * w * RIDDEN_SMOOTH;
        }
    }

    // 4. The start straight, benched in beside the lap.
    //
    // A second pass rather than more work in the first, because it is a second line: its own
    // stations, its own width, its own deck. Cells belong to whichever they are nearer the
    // edge of, and where the two meet — at the merge — both are cut to the same height, so
    // there is nothing to fall down.
    let (mut spur_dist, mut spur_arc) = (vec![f32::MAX; gw * gh], vec![0.0f32; gw * gh]);
    if let Some(spur) = &spur {
        let (_, which) = nearest_station(&spur.stations, gw, gh, mps_x, mps_z);
        for i in 0..gw * gh {
            let (wx, wz) = ((i % gw) as f32 * mps_x, (i / gw) as f32 * mps_z);
            let (mut d, s, t) = local_frame(&spur.stations, which[i] as usize, wx, wz);
            // Behind the gate row the frame runs out and every distance becomes radial, which
            // rounds the back of the pad off into a lollipop. A start has a straight back edge
            // — the bank the gates are set against — so back there the width is measured
            // across the line rather than from the end of it.
            let mut back = 1.0f32;
            if s <= 0.5 {
                let q = &spur.stations[0];
                let (fx, fz) = crate::trackprog::heading_vector(q.heading);
                let (rx, rz) = crate::trackprog::right_vector(q.heading);
                let behind = -((wx - q.x) * fx + (wz - q.z) * fz);
                if behind > 0.0 {
                    d = ((wx - q.x) * rx + (wz - q.z) * rz).abs();
                    back = smoothstep(
                        ((BACK_OF_THE_GATE_M - behind) / BACK_OF_THE_GATE_M).clamp(0.0, 1.0),
                    );
                    // And it ends there. Everything that paints the ground reads `spur_dist`,
                    // and a station's frame runs on for ever — so without this the start is
                    // surfaced as track for as far behind the gate row as the grid goes, which
                    // from the gate is a wide slab running off into nothing.
                    if behind > BACK_OF_THE_GATE_M {
                        d = f32::MAX;
                    }
                }
            }
            spur_dist[i] = d;
            spur_arc[i] = s;
            let wide = spur.at(s);
            // How much of this cell the start straight has a claim on: all of it well inside
            // the pad, none of it on the lap, and a few metres of crossfade between.
            //
            // A hard boundary — whichever line the cell is nearer the edge of — is what left a
            // wall down the middle of the wedge where the two converge: either side of the
            // line the ground was benched by a different rule, and the two rules do not meet.
            let spur_e = d - wide;
            let lap_e = dist[i] - widths.at(arc[i]);
            let lap_e = dist[i] - widths.at(arc[i]);
            // Track surface wherever the start covers it, whatever the ground under it is
            // doing: this is the same width the `.trh` and the `.map` paint, and a corridor
            // that disagreed with them measured eight metres narrower than the file it wrote.
            if d <= wide && back > 0.0 {
                corridor[i] = true;
            }
            let claim = smoothstep(((lap_e - spur_e) / SHOULDER_M).clamp(0.0, 1.0)) * back;
            if claim <= 0.0 {
                continue;
            }
            // Cut and fill from what is already there, so nothing the lap built is undone by
            // a pad that only half reaches it.
            let ground = heights[i];
            let deck = spur.deck_at(s);
            // A start pad's cut and fill spread further than a track's shoulder: it is a much
            // wider thing, and the banks round one are what a rider sees from the gate.
            let shoulder = SHOULDER_M * START_BANK * bench_shoulder(ground, deck);
            let w = bench_weight(d, wide, shoulder) * claim;
            heights[i] = ground * (1.0 - w) + deck * w;

            if d > wide {
                continue;
            }
            // Ridden ground. The pad is track, and a track with no texture on it is a slab —
            // the same fine roughness the lap gets, tapered across the width the way it is
            // there, because the middle of a start is used and its far corners are not.
            let across = 1.0 - (d / wide.max(1e-3)).min(1.0).powi(2);
            if r.texture > 0.0 {
                heights[i] += fbm(
                    wx / TEXTURE_WAVELENGTH_M,
                    wz / TEXTURE_WAVELENGTH_M,
                    r.seed ^ 0x5EED,
                ) * r.texture
                    * (0.25 + 0.75 * across)
                    * claim;
            }
            // And the grooves the gate leaves: forty bikes pulling out of forty stalls dig
            // forty lines, deepest a few metres off the row and gone by the time the pack has
            // spread. The one piece of ground on a track whose ruts are laid out in a comb.
            let from_gate = s - spur.gate_at();
            let row = GRID_STALLS as f32 * GRID_LANE_M * 0.5;
            if from_gate > -1.0 && from_gate < GATE_RUT_M && t.abs() < row {
                let along = smoothstep(1.0 - (from_gate.max(0.0) / GATE_RUT_M));
                let lane = (t / GRID_LANE_M) * std::f32::consts::TAU;
                let groove = 0.5 - 0.5 * lane.cos();
                heights[i] -= GATE_RUT_DEPTH_M * along * groove * claim;
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

    // How steeply the ground the features built climbs along the lap, per station: the faces
    // of every jump on it. Taken from the feature profile rather than from the finished
    // terrain, so a hill the lap was routed over is not read as a takeoff — a jump is
    // something built on the line, and this is the record of what was built.
    let face: Vec<f32> = stations
        .iter()
        .map(|st| {
            let h = FACE_STEP_M;
            (feat.at(st.s + h) - feat.at(st.s - h)) / (2.0 * h)
        })
        .collect();

    Ok(Synth {
        gw,
        gh,
        mps,
        heights,
        corridor,
        rut,
        face,
        dist,
        arc,
        station,
        line_lat,
        stations,
        spur,
        spur_dist,
        spur_arc,
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

    // The chamfer is a guess, and a scanline-shaped one. Its metric is an integer
    // approximation to a circle propagated in raster order, so its labels are wrong in a
    // pattern that runs along the scan — and everything downstream reads the label to find out
    // how far round the lap a cell is. The result was fine axis-aligned striping fanning off
    // the outside of every corner and single-cell scratches across the field, both of them
    // visible in a hillshade and in neither of the numbers we measure.
    //
    // So the labels are relaxed against the real distance afterwards: a cell takes a
    // neighbour's station whenever that station is genuinely nearer than its own. Three passes
    // in alternating directions is enough — the chamfer is never far wrong, it is only wrong
    // in stripes, and one sweep of true distances flattens them.
    let mut out = vec![0.0f32; gw * gh];
    let true_d = |at: usize, l: u32| -> f32 {
        let (x, y) = ((at % gw) as f32 * mps_x, (at / gw) as f32 * mps_z);
        let s = &st[l as usize];
        (x - s.x).powi(2) + (y - s.z).powi(2)
    };
    for at in 0..gw * gh {
        out[at] = true_d(at, label[at]);
    }
    for pass in 0..3 {
        let order: Vec<usize> = if pass % 2 == 0 {
            (0..gw * gh).collect()
        } else {
            (0..gw * gh).rev().collect()
        };
        for at in order {
            let (x, y) = (at % gw, at / gw);
            let mut best = (out[at], label[at]);
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= gw as i32 || ny >= gh as i32 {
                    continue;
                }
                let l = label[ny as usize * gw + nx as usize];
                if l == best.1 {
                    continue;
                }
                let d = true_d(at, l);
                if d < best.0 {
                    best = (d, l);
                }
            }
            out[at] = best.0;
            label[at] = best.1;
        }
    }
    for v in &mut out {
        *v = v.sqrt();
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
    /// How much of a *second* line the corner has grown, 0 to 1.
    ///
    /// A corner after three motos does not carry one line, it carries two or three, and the
    /// choice between them is most of what makes a turn worth riding twice. This is the one
    /// that runs outside the main line and picks up through the exit, which is why it is
    /// carried forward far and back barely at all.
    second: Profile,
    /// How much the bundle collapses to a single groove, 0 to 1.
    ///
    /// One on a jump, where everybody hits the face in the same place and packs it hard;
    /// zero on ordinary ground, where the field spreads across the track. Without this a
    /// takeoff ramp came out with half a dozen parallel gouges across it and no line at all
    /// — the opposite of how a face actually wears.
    focus: Profile,
    /// What survives of the depth here, 0 to 1. Near zero over a lip and across a gap: a
    /// takeoff edge is maintained, and a rutted lip is one nobody can see.
    damp: Profile,
}

/// How much of the surface grain a packed line polishes away, 0 to 1.
///
/// Not all of it: a line is smoother than the ground beside it, not glass. What it must not
/// be is the same, which is what it was — the grain, the braking washboard and the
/// acceleration chop were all laid across the corridor without asking whether this strip is
/// the one everybody rides.
const POLISHED: f32 = 0.72;

/// A rounded trough across the track, 1 at its centre and 0 by `width` either side.
///
/// Free-standing because two places need the same shape: the ruts carve with it, and the
/// surface grain is taken off with it, and a line that is polished somewhere other than where
/// it is cut is two lines.
fn trough_at(t: f32, centre: f32, width: f32) -> f32 {
    let d = ((t - centre) / width.max(1e-3)).clamp(-1.0, 1.0);
    1.0 - d * d
}

/// The scuffs a jump face wears: how far apart they lie across it, and how deep they cut as a
/// fraction of the ground's rut depth.
///
/// Shallow on purpose. These are not ruts — nothing sits in them — they are the record of a
/// hundred riders dragging a back wheel up the same ramp from slightly different places, and
/// on a real face you read them long before you feel them.
const TYRE_MARK_SPACING_M: f32 = 0.62;
/// As a multiple of the ground's rut depth — and a jump sits on a straight, where that is
/// `rut_straight`, about nine centimetres. At 0.34 the scuffs cut three: real enough, and far
/// too little to see from the seat. A face that has been ridden all day is visibly combed.
const TYRE_MARK_DEPTH: f32 = 1.15;

/// Half the width of one carved groove, metres.
///
/// A tyre and the ridge shouldered up beside it, and emphatically not
/// [`RUT_HALF_WIDTH_M`]: that is the width of the *strip* the line packs down, which is a
/// bike wide and then some. Carving the groove itself that wide made a four-metre trough
/// with a floor flat enough that its own low point was whichever way the field noise
/// happened to lean — a channel, not a rut, and it measured as one.
///
/// The field's grooves get their width from [`RUT_SPACING_M`] and their shape from
/// [`RUT_EDGE`]; this is only for the two lines that are stated rather than sampled.
const RUT_GROOVE_M: f32 = 0.55;

/// How far outside the main line a corner's second line sits, in metres, and how deep it
/// runs against the first.
///
/// Deeper than the field it stands in or it is not a line; shallower than the main one or it
/// *is* the main one — and shallower than the main one's *shallowest*, not its average. At
/// 0.92 a station where the main line's own variation was low and the second's was high put
/// the deepest ground 3.2 m off the paint, which is the fault this was meant to fix. Both
/// lines are carved rather than sampled out of the field, so this only has to clear what the
/// field typically reaches, not what it peaks at.
const RUT_SECOND_M: f32 = 2.9;
const RUT_SECOND_DEPTH: f32 = 0.66;

/// How far a second line is carried back up the approach and out onto the exit, metres.
///
/// Barely at all, and a long way. An outside line is not something a rider is on at turn-in;
/// it is where they end up, so it appears late and runs out down the following straight.
const RUT_SECOND_ENTRY_M: f32 = 6.0;
const RUT_SECOND_EXIT_M: f32 = 48.0;

fn rut_profile(
    features: &[Feature],
    turn: &Profile,
    lap: f32,
    seed: u32,
    feel: &Ride,
) -> Ruts {
    let mut depth = Profile::blank(lap);
    let mut tight = Profile::blank(lap);
    let mut centre = Profile::blank(lap);
    let mut second = Profile::blank(lap);
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
                depth.v[i] = feel.rut_depth * t * vary;
                tight.v[i] = t;
                // Positive curvature turns right, and the corner's inside is the rider's
                // right — the same side `right_vector` points at, which is the sign every
                // lateral quantity here is measured in.
                centre.v[i] = k.signum() * RUT_INSIDE * t;
                // A second line needs a corner wide enough for two of them. Below the radius
                // that grows a full-depth rut there is only one way through.
                second.v[i] = t;
            }
        }
        // A straight is not smooth ground. Every published track wears grooves down its
        // straights too — a tenth of a metre against a corner's third — and a lap that is
        // glass between the turns reads as one from the first corner exit.
        let vary = 0.7 + 0.3 * fbm(s / 11.0, 8.5, seed ^ 0x2118);
        let floor = feel.rut_straight * vary;
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
    // Its own reach, which is the whole point of it being a different line.
    carry(&mut second, RUT_SECOND_ENTRY_M, RUT_SECOND_EXIT_M);

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

    let (focus, damp) = built_ground(features, lap);
    Ruts {
        depth,
        spread,
        centre,
        second,
        focus,
        damp,
    }
}

/// What the ground built on the lap does to the ruts over it: where the bundle narrows to one
/// line, and where there is no rut at all.
///
/// The rut field knew only about curvature, so a jump — which sits on a straight — took the
/// straight's uniform groove floor across its whole width. A takeoff face came out with
/// grooves spread four metres either side of the line and no line among them, which is the
/// opposite of how a face wears: everyone hits it in the same place, packs that hard, and
/// leaves the ground beside it soft.
fn built_ground(features: &[Feature], lap: f32) -> (Profile, Profile) {
    let mut focus = Profile::blank(lap);
    let mut damp = Profile::blank(lap);
    for v in damp.v.iter_mut() {
        *v = 1.0;
    }
    // `at` and `at + span` in metres, how hard the bundle collapses over it, and what is
    // left of the depth.
    let mut mark = |from: f32, to: f32, f: f32, d: f32| {
        if to <= from {
            return;
        }
        let lo = (from / PROFILE_STEP).floor().max(0.0) as usize;
        let hi = ((to / PROFILE_STEP).ceil() as usize).min(focus.v.len() - 1);
        for i in lo..=hi {
            let s = i as f32 * PROFILE_STEP;
            if s < from || s > to {
                continue;
            }
            // Eased at both ends over a metre, so a face does not step from a bundle to a
            // single groove between two samples.
            let e = smoothstep(((s - from) / 1.0).min((to - s) / 1.0).clamp(0.0, 1.0));
            focus.v[i] = focus.v[i].max(f * e);
            damp.v[i] = damp.v[i].min(1.0 - (1.0 - d) * e);
        }
    };

    for feat in features {
        let at = feat.at();
        match *feat {
            Feature::Tabletop { height, length, .. } => {
                let (up, top, down) = crate::trackprog::tabletop_faces(height, length);
                // One line up the face and one off the landing.
                mark(at, at + up, 1.0, 1.0);
                // The top is maintained ground: swept flat between motos, and a lip nobody
                // can see is a lip nobody clears.
                mark(at + up, at + up + top, 1.0, 0.22);
                mark(at + up + top, at + up + top + down, 1.0, 1.0);
            }
            Feature::Double { height, gap, lip, .. } => {
                let f = crate::trackprog::double_faces(height, lip);
                let crest = at + f.ramp + f.back;
                mark(at, at + f.ramp, 1.0, 1.0);
                // The back of the lip and the gap floor: a cut face and ground nobody's
                // wheels touch on a jump that works.
                mark(at + f.ramp, crest + gap, 1.0, 0.12);
                mark(crest + gap, crest + gap + f.face, 1.0, 0.35);
                mark(crest + gap + f.face, crest + gap + f.face + f.run, 1.0, 1.0);
            }
            // Ridden across rather than launched off, so the field narrows without
            // collapsing: a whoop section carries lines, not a line.
            Feature::Whoops { count, spacing, .. } => {
                mark(at, at + count as f32 * spacing, 0.7, 1.0);
            }
            Feature::Roller { length, .. } => mark(at, at + length, 0.45, 1.0),
            Feature::Custom { length, .. } => mark(at, at + length, 1.0, 1.0),
            Feature::StepUp { .. } | Feature::Berm { .. } | Feature::Rut { .. } => {}
        }
    }
    (focus, damp)
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

fn roughness_profile(turn: &Profile, speed: &crate::trackspeed::Speed, lap: f32) -> Chop {
    let mut rough = Profile::blank(lap);
    let mut braking = Profile::blank(lap);
    let mut accel = Profile::blank(lap);
    for i in 0..rough.v.len() {
        rough.v[i] = 1.0;
    }
    for i in 0..rough.v.len() {
        let s = i as f32 * PROFILE_STEP;
        // Where the rider is braking, for as long as they are braking, and hard in
        // proportion to how hard — which is the only definition of a braking bump there is.
        //
        // It used to be a fixed twenty-two metres before anything under a forty-metre
        // radius, and thirty metres of chop after it. So a 90 km/h approach to a hairpin and
        // a 40 km/h approach to a flat left got identical washboard, and a corner whose
        // approach happened to be another corner got none at all — the exclusion that rule
        // needed to stop a corner exit reading as the next one's approach. None of that
        // survives: the speed profile already knows the difference, because it is the
        // difference.
        braking.v[i] = speed.braking(s);
        accel.v[i] = speed.driving(s);

        // How chopped up the ground is, which is a different question from where the bumps
        // are: a corner is worked over by every wheel that turns in it.
        let k = turn.at(s).abs();
        if k > 0.0 {
            let radius = 1.0 / k;
            if radius < RUT_RADIUS_M.0 {
                let corner =
                    smoothstep((RUT_RADIUS_M.0 - radius) / (RUT_RADIUS_M.0 - RUT_RADIUS_M.1));
                rough.v[i] = rough.v[i].max(1.0 + (CORNER_ROUGHNESS - 1.0) * corner);
            }
        }
    }
    // Both are read off a one-metre profile and land on a half-metre one, so they step where
    // the speed does. A metre of easing takes the stair out without moving anything.
    smooth_along(&mut braking.v, (1.0 / PROFILE_STEP).round() as usize);
    smooth_along(&mut accel.v, (1.0 / PROFILE_STEP).round() as usize);
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
    // And the hollow each one was dug out of.
    //
    // Every cubic metre standing in a jump came out of the ground beside it, and on a real
    // track you can see where. Subtracted rather than maxed, and applied after the humps are
    // in, so a jump sitting inside another feature's hollow still stands its full height —
    // what the hollow lowers is the ground between jumps, not the jumps.
    let mut dig = vec![0.0f32; out.v.len()];
    for f in features {
        if matches!(f, Feature::StepUp { .. } | Feature::Berm { .. } | Feature::Rut { .. }) {
            continue;
        }
        let h = f.height().abs();
        if h <= 0.0 {
            continue;
        }
        let (at, len) = (f.at(), f.length());
        for (side, edge) in [(-1.0f32, at), (1.0f32, at + len)] {
            let span = JUMP_HOLLOW_M;
            let steps = (span / PROFILE_STEP).ceil() as usize;
            for k in 0..=steps {
                let d = k as f32 * PROFILE_STEP;
                let s_at = edge + side * d;
                if s_at < 0.0 || s_at >= lap {
                    continue;
                }
                let i = (s_at / PROFILE_STEP).round() as usize;
                if i >= dig.len() {
                    continue;
                }
                // Deepest a third of the way out and back to grade by the end, so the ground
                // dips away from the jump's foot rather than stepping down at it.
                let x = d / span;
                let bowl = (x * std::f32::consts::PI).sin().powf(0.8);
                dig[i] = dig[i].max(h * JUMP_HOLLOW * bowl);
            }
        }
    }
    for i in 0..out.v.len() {
        out.v[i] -= dig[i];
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
/// Ground that is higher after a feature than before it.
///
/// And then the lap is levelled again. A step-up raises everything after it and nothing puts
/// it back, so a lap with one 2.2 m step-up on it finishes 2.2 m above where it started — and
/// the start line, where the two ends of the lap meet in the ground, is a 2.2 m cliff across
/// the track. `rise` is checked for exactly this ("a lap that climbs has to come back down")
/// and step-ups went round the check.
///
/// The drift comes out as a constant grade rather than as a complaint, because it is one: a
/// track that steps up somewhere has to fall the same amount over the rest of the lap, and
/// spreading it evenly is what a builder would do. Over a 1900 m lap, 2.2 m is a grade of one
/// part in 900 — under a centimetre between one station and the next.
/// The steepest the deck is allowed to run under something built on it, as a gradient.
///
/// A builder cuts a pad before shaping a jump. We never did: the deck followed the landscape
/// smoothed over [`BENCH_SMOOTH_M`], and on a hillside that is three metres of fall across a
/// twenty-metre tabletop — so a 1.26 m jump peaked 0.23 m above its own foot and a rider
/// reported, correctly, that the track had no tables on it at all.
///
/// Not zero. A pad on a hillside is cut roughly level, not perfectly, and a jump built into a
/// slope is a real thing; six per cent is a slope you can see and not one that eats a jump.
const PAD_GRADE: f32 = 0.06;

/// Level the deck under everything built on the line.
///
/// Each footprint gets a line fitted through the deck it sits on, that line's slope clamped to
/// [`PAD_GRADE`], and the deck pulled onto it — eased out either side so the pad meets the
/// ground it came from instead of stepping off it.
///
/// Every pad is worked out against the *original* deck and the strongest pull wins, rather
/// than each one being applied in turn. Applied in turn they compound: two jumps a metre apart
/// each level the other's ground and a pair of 2 m tables came out 2.49 m tall.
fn level_pads(along: &mut [f32], st: &[Station], features: &[Feature]) {
    if st.len() < 3 {
        return;
    }
    let was = along.to_vec();
    // Blended, not winner-takes-all. Where two pads' influence overlaps, taking the stronger
    // one's level makes the answer jump the moment the winner changes — a cliff across the
    // track at the boundary, which measured as a 0.71 m step between neighbouring samples.
    let mut pull = vec![0.0f32; st.len()];
    let mut acc = vec![0.0f32; st.len()];
    let mut wsum = vec![0.0f32; st.len()];

    for f in features {
        // A rut is not built; a berm is shaped across the track rather than along it; and a
        // step-up *is* a change of level, so levelling it would undo it.
        if matches!(f, Feature::Rut { .. } | Feature::Berm { .. } | Feature::StepUp { .. }) {
            continue;
        }
        let (from, span) = (f.at(), f.length());
        if span <= 1.0 {
            continue;
        }
        let inside: Vec<usize> =
            (0..st.len()).filter(|&i| st[i].s >= from && st[i].s <= from + span).collect();
        if inside.len() < 2 {
            continue;
        }
        // Least squares through the deck over the footprint, about its middle.
        let mid = from + span * 0.5;
        let n = inside.len() as f32;
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for &i in &inside {
            let x = st[i].s - mid;
            sx += x;
            sy += was[i];
            sxx += x * x;
            sxy += x * was[i];
        }
        let denom = n * sxx - sx * sx;
        let slope = if denom.abs() < 1e-6 { 0.0 } else { (n * sxy - sx * sy) / denom };
        let level = sy / n;
        // A pad on a hillside is cut roughly level, not perfectly — but never at the grade the
        // hill runs at, which is the whole point.
        let slope = slope.clamp(-PAD_GRADE, PAD_GRADE);
        let ease = (span * 0.6).clamp(6.0, 30.0);

        for i in 0..st.len() {
            let d = (st[i].s - mid).abs() - span * 0.5;
            let w = if d <= 0.0 {
                1.0
            } else if d >= ease {
                0.0
            } else {
                smoothstep(1.0 - d / ease)
            };
            if w <= 0.0 {
                continue;
            }
            pull[i] = pull[i].max(w);
            acc[i] += w * (level + (st[i].s - mid) * slope);
            wsum[i] += w;
        }
    }

    for i in 0..st.len() {
        if wsum[i] <= 0.0 {
            continue;
        }
        let want = acc[i] / wsum[i];
        along[i] = was[i] + (want - was[i]) * pull[i];
    }
}

fn apply_step_ups(along: &mut [f32], st: &[Station], features: &[Feature]) {
    let mut net = 0.0f32;
    for f in features {
        let Feature::StepUp { at, length, height } = *f else {
            continue;
        };
        net += height;
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
    if net == 0.0 {
        return;
    }
    let lap = st.last().map(|s| s.s).unwrap_or(0.0);
    if lap <= 0.0 {
        return;
    }
    for (i, s) in st.iter().enumerate() {
        along[i] -= net * (s.s / lap);
    }
}

/// A feature's shape along the track. `t` runs 0–1 across it, `u` is metres from its start.
/// A jump face's own shape, at `t` from its foot to its lip.
///
/// The sweep comes from the face's real run and rise rather than from the angle it was sized
/// against, so a face lengthened by [`crate::trackprog::JUMP_FACE_MIN_M`] or by a programme's
/// own `lip` is a shallower arc rather than the same arc stretched over more ground. Either
/// way it leaves the ground tangent and arrives at the lip steepest.
fn arc_up(t: f32, height: f32, run: f32) -> f32 {
    crate::trackprog::face_arc(t, crate::trackprog::face_sweep(height, run))
}

fn longitudinal(f: &Feature, t: f32, u: f32) -> f32 {
    // Drawn by hand: eased between the points it was given, which is the same easing the lap's
    // own height curve uses. Nothing else here has a shape someone chose point by point.
    if let Feature::Custom { shape, .. } = f {
        return along_points(shape, t);
    }
    match *f {
        // Up, along the top, and down. The ramps are a third each, which is about what a
        // built tabletop measures.
        // Up, along the top, and down — with the two ramps sized from the height and an
        // angle rather than as fractions of the length, so a short tabletop is not a steep
        // one. Shared with `Feature::length`, which has to agree about where it ends.
        Feature::Tabletop { height, length, .. } => {
            let (up, top, down) = crate::trackprog::tabletop_faces(height, length);
            if u <= up {
                // Concave: tangent to the ground at the foot and steepest at the lip, which
                // is the edge the rider leaves the ground over.
                height * arc_up(u / up, height, up)
            } else if u <= up + top {
                height
            } else {
                // The same arc ridden the other way, so the far side is convex — steep off
                // the crest and flattening into the ground that catches you.
                height * arc_up(1.0 - (u - up - top) / down, height, down)
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
            // All four faces from one definition, shared with `Feature::length` so the two
            // cannot disagree about where the shape ends.
            let f = crate::trackprog::double_faces(height, lip);
            let crest = f.ramp + f.back;
            let land = crest + gap;
            if u <= f.ramp {
                height * arc_up(u / f.ramp, height, f.ramp)
            } else if u <= crest {
                // Dumped faces, so a smoothstep: rounded at the lip it leaves and at the
                // ground it meets, which is how a tipped load settles.
                height * smoothstep(1.0 - (u - f.ramp) / f.back)
            } else if u <= land {
                0.0
            } else if u <= land + f.face {
                height * smoothstep((u - land) / f.face)
            } else {
                height * arc_up(1.0 - (u - land - f.face) / f.run, height, f.run)
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

/// The same, between the samples.
///
/// The game draws the terrain as a mesh and lights it off interpolated normals. Anything that
/// lights it off [`sample`] instead gets one flat normal per cell, which at riding scale is a
/// staircase down every slope — a picture of the grid rather than of the ground.
fn sample_smooth(h: &[f32], gw: usize, gh: usize, x: f32, y: f32) -> f32 {
    let (fx, fy) = (x.clamp(0.0, (gw - 1) as f32), y.clamp(0.0, (gh - 1) as f32));
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(gw - 1), (y0 + 1).min(gh - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |xi: usize, yi: usize| h[yi * gw + xi];
    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
    let bot = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
    top + (bot - top) * ty
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
    fbm_of(x, y, seed, 4, 0.5)
}

/// Value noise summed over `octaves`, each half the wavelength of the last and `gain` times
/// its amplitude.
///
/// The two are exposed because the *landscape* wants a far steeper roll-off than anything
/// else here. Measured as how much ground rises and falls over a given distance, Indiana runs
/// 0.15 m over 5 m and 9.36 m over 200 — one big smooth landform with almost nothing on it at
/// small scales. Four octaves at a half gain spread the energy across every scale instead, and
/// the terrain came out two to four times rougher than a real one everywhere, worst at the
/// short distances a rider actually sees.
fn fbm_of(x: f32, y: f32, seed: u32, octaves: u32, gain: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut f = 1.0;
    for o in 0..octaves {
        sum += value_noise(x * f, y * f, seed.wrapping_add(o * 7919)) * amp;
        norm += amp;
        amp *= gain;
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

    put("heightmap.raw", heightmap_raw(syn, prog.terrain.scale), &mut wrote)?;

    // The riding line, and everything that isn't it. Every boundary is torn rather than
    // drawn — see `edge_noise`.
    let seed = prog.terrain.relief.seed;
    let half = prog.width * 0.5;
    // Every band is measured from the edge of the track — the lap's, or the start straight's,
    // whichever the cell is nearer. The start is a track surface too, and painting by the lap
    // alone leaves forty gates standing in a field.
    // Off `layers`, like everything else here. These three used to be a second set of edges
    // written out by hand — dirt 1.5 m wider than its band and fading over two metres, the
    // shoulder at three quarters of its width fading over five, grass over six — so the masks
    // TerrainEd compiles into the shipped track were not the bands the `.map` writer and every
    // picture of the ground draw. The ground nobody could judge from a picture was this.
    let band_of = |b: BandMask| band_mask(syn, b, half, seed, MASK_DIM, MASK_DIM);
    let bands = layers(prog);
    let band_named = |name: &str| -> Vec<u8> {
        let l = bands.iter().find(|l| l.name == name).expect("a band by that name");
        band_of(l.band)
    };
    let dirt = band_named("dirt");
    let line = band_named("line");
    let shoulder = band_named("shoulder");
    let grass = band_named("grass");
    // Off-track starts where the graded shoulder ends: the rider is on the track, or in the
    // field, with the shoulder belonging to neither. This one decides where the game says a
    // rider has gone off, so it is the one boundary that stays smooth.
    let off = mask_outside(syn, MASK_DIM, half, |e, _, _| {
        255 - soft_edge(SHOULDER_M * 0.6, 3.0, e)
    });
    // The start area is the start straight: the gate row, the sprint and the turn that takes
    // it onto the lap, and nothing of the lap itself.
    let start = mask_from(syn, MASK_DIM, |_, _, _, _| 0);
    let start = match &syn.spur {
        Some(spur) => {
            let mut m = start;
            for y in 0..MASK_DIM {
                let gy = (y * syn.gh / MASK_DIM).min(syn.gh - 1);
                for x in 0..MASK_DIM {
                    let gx = (x * syn.gw / MASK_DIM).min(syn.gw - 1);
                    let i = gy * syn.gw + gx;
                    let d = syn.spur_dist[i];
                    m[y * MASK_DIM + x] =
                        u8::from(d.is_finite() && d <= spur.at(syn.spur_arc[i]) * 1.05) * 255;
                }
            }
            m
        }
        None => start,
    };
    // These two keep the riding line's own width: a rut is worn by everyone taking the same
    // line and the loose stuff piles up beside it, and neither happens out on a start pad.
    let rut = rut_mask(syn, half, seed, MASK_DIM, MASK_DIM);
    let loose = loose_mask(syn, half, seed, MASK_DIM, MASK_DIM);
    put("mask_dirt.tga", tga_alpha(MASK_DIM, MASK_DIM, &dirt), &mut wrote)?;
    put("mask_line.tga", tga_alpha(MASK_DIM, MASK_DIM, &line), &mut wrote)?;
    put("mask_loose.tga", tga_alpha(MASK_DIM, MASK_DIM, &loose), &mut wrote)?;
    put("mask_rut.tga", tga_alpha(MASK_DIM, MASK_DIM, &rut), &mut wrote)?;
    put(
        "mask_shoulder.tga",
        tga_alpha(MASK_DIM, MASK_DIM, &shoulder),
        &mut wrote,
    )?;
    // The pit lane, in the same place the race data puts its stalls. It runs along the
    // opening straight, so the straight's own frame gives the side the lane is on — the
    // distance the other masks read is unsigned and would paint a lane on both sides.
    let pits = pit_lane(prog);
    let (fx, fz) = crate::trackprog::heading_vector(prog.start.angle.to_radians());
    let (rx, rz) = crate::trackprog::right_vector(prog.start.angle.to_radians());
    let pit_area = mask_from(syn, MASK_DIM, |_, _, x, z| {
        let (dx, dz) = (x - prog.start.x, z - prog.start.z);
        let along = dx * fx + dz * fz;
        let lat = dx * rx + dz * rz;
        u8::from(
            along >= pits.from - 6.0
                && along <= pits.to + 6.0
                && (lat - pits.lat).abs() <= pits.half_width,
        ) * 255
    });
    // What the 3D grass is coloured by. Terrain-wide, like the density map it sits beside in
    // the same block, rather than tiled like the blade sprite: the two `*map` keys are
    // siblings and read the ground the same way.
    let turf = ground_looks(prog.terrain.surface).turf;
    let grass_color = mask_rect(syn, MASK_DIM, MASK_DIM, |_, _, x, z| {
        // One channel is enough to carry the variation; the tint itself is written below.
        (140.0 + 90.0 * fbm(x * 0.02, z * 0.02, seed ^ 0x4B12)).clamp(0.0, 255.0) as u8
    });

    put("mask_grass.tga", tga_alpha(MASK_DIM, MASK_DIM, &grass), &mut wrote)?;
    put(
        "grass_color.tga",
        tga_tinted(MASK_DIM, MASK_DIM, &grass_color, turf.base),
        &mut wrote,
    )?;
    put("area_off.tga", tga_alpha(MASK_DIM, MASK_DIM, &off), &mut wrote)?;
    put("area_pits.tga", tga_alpha(MASK_DIM, MASK_DIM, &pit_area), &mut wrote)?;
    put("area_start.tga", tga_alpha(MASK_DIM, MASK_DIM, &start), &mut wrote)?;

    // Each band writes four things, not one: the sheet, the normal map that gives it relief,
    // the shader that ties the two together, and the sheet it wears in the rain.
    std::fs::create_dir_all(dir.join("maps/env")).context("make the maps folder")?;
    let seed = prog.terrain.relief.seed;
    for l in layers(prog) {
        let px = band_pixels(GROUND_TEXTURE_DIM, &l.look, seed ^ l.salt);
        // A shader's bump takes one repetition count whatever shape the terrain is, so on a
        // rectangular one it follows x and the sheet's own two counts do the rest.
        let (rx, _) = repetitions(prog, l.tile_m);
        let name = l.name;
        put(
            &format!("maps/{name}.tga"),
            rgba_tga(GROUND_TEXTURE_DIM, &px),
            &mut wrote,
        )?;
        put(
            &format!("maps/{name}_n.tga"),
            normal_tga(&px, GROUND_TEXTURE_DIM, SHEET_NORMAL_STRENGTH, l.spec),
            &mut wrote,
        )?;
        put(
            &format!("maps/{name}.shd"),
            crlf(&shd(&format!("{name}_n.tga"), rx, l.shininess, None)),
            &mut wrote,
        )?;
        if l.wet {
            // The wet frame reuses the dry normal map — the example's does too, because
            // rain darkens ground without reshaping it. What changes is the shader: a tight
            // highlight and the sky coming back off the water.
            put(
                &format!("maps/{name}_wet.tga"),
                rgba_tga(GROUND_TEXTURE_DIM, &wet_pixels(&px)),
                &mut wrote,
            )?;
            put(
                &format!("maps/{name}_wet.shd"),
                crlf(&shd(
                    &format!("{name}_n.tga"),
                    rx,
                    80,
                    Some(Reflect {
                        min: 0.0,
                        max: 0.8,
                        exp: 6.0,
                    }),
                )),
                &mut wrote,
            )?;
        }
    }
    put("maps/grassfx.tga", grass_billboard(128), &mut wrote)?;
    for (face, bytes) in env_faces(ENV_FACE_DIM) {
        put(&format!("maps/env/env_{face}.tga"), bytes, &mut wrote)?;
    }

    // What stands beside the track: the models, and the blocks that place them. The `.hmf`
    // draws them; the `.tht` gets only what should stop a bike, so a rider clips a foliage
    // card and rides on but hits a bale.
    let scenery = crate::trackscenery::build(prog, syn);
    for (name, bytes) in &scenery.files {
        put(name, bytes.clone(), &mut wrote)?;
    }

    // PiBoSo's own source files are CRLF, so ours are.
    let hmf_text = hmf(prog, syn) + &crate::trackscenery::blocks(&scenery.drawn);
    let tht_text = tht(prog, syn) + &crate::trackscenery::blocks(&scenery.solid);
    put("track.hmf", crlf(&hmf_text), &mut wrote)?;
    put("track.tht", crlf(&tht_text), &mut wrote)?;
    put("params.ini", crlf(PARAMS_INI), &mut wrote)?;
    put("trh_params.ini", crlf(TRH_PARAMS_INI), &mut wrote)?;
    put("track.tcl", crlf(&tcl(prog)), &mut wrote)?;
    if let Some(start) = start_tcl(prog) {
        put("track_start.tcl", crlf(&start), &mut wrote)?;
    }
    // The game-facing files, byte for byte what the `.pkz` carries.
    put(&format!("{slug}/{slug}.ini"), crlf(&track_ini(prog)), &mut wrote)?;
    put(&format!("{slug}/{slug}.amb"), crlf(AMB), &mut wrote)?;
    put(&format!("{slug}/gfx.cfg"), crlf(&gfx_cfg(prog)), &mut wrote)?;
    put(&format!("{slug}/{slug}.rdf"), crlf(&rdf(prog, syn.spur.as_ref())), &mut wrote)?;
    put(&format!("{slug}/{slug}.ssc"), SSC.into(), &mut wrote)?;
    let (map_img, shot) = ui_images(prog, syn, UI_IMAGE_DIM);
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
        format!("tracked -merge {slug}/{slug}.trh cl track.tcl sa track_start.tcl\r\n")
            .into_bytes(),
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
            mask_from(syn, dim, move |d, s, _, _| {
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
    let half = prog.width * 0.5;
    // Measured from the edge of the track rather than from the lap's centreline, so the start
    // straight is surfaced as track. This file is what the game reads to decide whether a
    // wheel is on the track at all: by the lap alone, the gate row stands on grass.
    let line_at = |x: f32, z: f32| edge_noise(x, z, seed ^ 0xD127, 1.6, 0.7);
    let field_at = |x: f32, z: f32| shoulder + edge_noise(x, z, seed ^ 0x6EE2, 3.2, 1.2);
    let mut masks: Vec<(u32, Vec<u8>)> = vec![
        // 10 is the riding line — the id published tracks paint their ribbon with.
        (
            10,
            mask_outside(syn, dim, half, |e, x, z| u8::from(e <= line_at(x, z)) * 255),
        ),
        (
            shoulder_id,
            mask_outside(syn, dim, half, |e, x, z| {
                u8::from(e > line_at(x, z) && e <= field_at(x, z)) * 255
            }),
        ),
        (
            1,
            mask_outside(syn, dim, half, |e, x, z| u8::from(e > field_at(x, z)) * 255),
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
    // The records follow the count directly. There used to be sixteen zero bytes here, put
    // in because a published file's first *real* mask sits well past the count and the gap
    // read as padding. It is not padding: it is empty records, which are the same sixteen
    // bytes with a zero width and height, and they are included in the count.
    //
    //   Indiana    count 3, first real record at +60 = 28 + 2 empty
    //   Millville  count 3, first real record at +60 = 28 + 2 empty
    //   Lambretta  count 8, first real record at +92 = 28 + 4 empty, and four real ones
    //
    // Ours declared three and then wrote sixteen bytes the reader takes as a fourth. It
    // counted our padding, read two of the three masks, stopped, and carried on into the
    // middle of the third one's pixels looking for the pose block, the material table and
    // the centreline -- none of which it can have found.

    for (id, m) in &masks {
        out.extend_from_slice(&id.to_le_bytes());
        // How deep the layer is. Indiana 0.5, Millville 0.4, Lambretta Lynds 0.3/0.4/0.2/1.0
        // -- ours were zero, the same empty physics field the material table had.
        let depth: f32 = match id {
            10 => 0.2,
            4 => 0.4,
            _ => 0.3,
        };
        out.extend_from_slice(&depth.to_le_bytes());
        out.extend_from_slice(&(dim as u32).to_le_bytes());
        out.extend_from_slice(&(dim as u32).to_le_bytes());
        out.extend_from_slice(m);
    }

    // One zero word closes the mask list, and the loader branches on it.
    //
    // Watched under emulation: after the last mask it reads a word at 0x1401f522c. Millville
    // has a zero there and goes on to 0x1401f5317 and the eleven hundred reads that make up
    // the rest of the file. Ours had no word at all, so the loader took the first float of
    // the pose block -- 282.02, which is 1,133,314,703 -- and left down a branch that reads
    // three more words and stops.
    //
    // Thirty reads against Millville's 1,165. Everything past this point in our file has
    // been written and never once read.
    out.extend_from_slice(&0u32.to_le_bytes());

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
    //
    // The nine floats after each name are how the surface gives under a wheel: four
    // (sinkage, load) points -- at 2, 5, 10 and 35 kPa -- and then how much of it stays as a
    // rut. We wrote thirty-six zero bytes, which is a load curve whose every point sits at
    // the origin, handed to the tyre model the instant a wheel touches the ground.
    //
    // These are not per-track values. Indiana, Millville, Flanders and Lambretta Lynds carry
    // byte-identical tables: three hard surfaces, two medium, and sand. Read straight off
    // them.
    // The nine floats after each name are how the surface gives under a wheel: four
    // (sinkage, load) points -- at 2, 5, 10 and 35 kPa -- and how much stays as a rut.
    // Identical in Indiana, Millville, Flanders and Lambretta Lynds, so read off them.
    const SURFACES: [(&str, f32, f32, f32, f32); 6] = [
        ("asphalt", 0.0012, 0.0025, 0.005, 0.0),
        ("grass", 0.0037, 0.0075, 0.015, -0.02),
        ("sand", 0.0075, 0.015, 0.03, -0.04),
        ("kerb", 0.0012, 0.0025, 0.005, 0.0),
        ("soil", 0.0037, 0.0075, 0.015, -0.02),
        ("concrete", 0.0012, 0.0025, 0.005, 0.0),
    ];
    out.extend_from_slice(&(SURFACES.len() as u32).to_le_bytes());
    for (name, a, b, c, rut) in SURFACES {
        let mut field = [0u8; 16];
        field[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&field);
        for v in [a, 2.0, b, 5.0, c, 10.0, 0.0, 35.0, rut] {
            out.extend_from_slice(&v.to_le_bytes());
        }
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
        // Word zero is an integer, not a float, and it is the one field in the record that
        // is. Every published line reads back as a plain 1 on every segment but the first --
        // Indiana's 120 records say `0, 1, 1, 1, ...` -- while ours said 1.0, which is
        // 1,065,353,216 to anything reading it as a count or a flag. The other fourteen
        // words are floats and match in kind.
        // Word zero is an integer, not a float: Indiana's line reads 0, 1, 1, ... and ours
        // wrote 1.0, which is 1,065,353,216 to anything taking it as a flag.
        let first = at == 0.0;
        out.extend_from_slice(&u32::from(!first).to_le_bytes());
        for v in &rec[1..] {
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

    // `EXT` closes the file, immediately after the centreline.
    //
    // A published `.trh` has a run of lists in between -- occluder boxes and more, the ones
    // `tracked.exe` fills from its `occluder%d/*` keys -- and emulating the loader over ours
    // showed it asking for a 66 MB record off the end where those lists should start. Ten
    // zero words made the emulator walk out cleanly, so they were written.
    //
    // In the game that change is what broke the track: the build without them loaded and
    // could be ridden, and every build with them crashed at track selection, before the
    // loading bar. The emulator models the read helper and not the loader's own idea of
    // where a file stops; `EXT` is that, and putting anything in front of it is worse than
    // the overrun it was meant to prevent.
    //
    // So: what the reader does past this marker is still unknown, and guessing at it cost a
    // working track. Leave it alone until the real loader has been watched deciding.
    // The lists that follow the centreline, every one of them empty.
    //
    // A published `.trh` carries occluder boxes and more here. Ten zero words is what the
    // loader takes to walk out: with none it asks for a 66 MB record off the end, with four
    // it is 40 bytes over, with eight 4, and with ten it reaches the last byte and returns.
    //
    // This was written once before and reverted, because the build carrying it crashed. It
    // was not the cause: the loader was bailing out four megabytes upstream, at the mask
    // list, and never reached these words at all. The measurement was sound and taken on a
    // file the game had already stopped reading.
    for _ in 0..10 {
        out.extend_from_slice(&0u32.to_le_bytes());
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
/// How many gates the row holds, and how wide a lane each one gets.
///
/// Forty at 1.2 m, because that is what every modern track ships: Indiana, Millville,
/// Washougal, Maryland and the GP tracks all say 40, and their lane widths run 1.1 to 1.3.
const GRID_STALLS: usize = 40;
const GRID_LANE_M: f32 = 1.2;

/// Where the finish line sits, in metres round the lap.
///
/// On the lap's opening straight, which is the main straight — riders cross it on every lap.
/// The gate row is not here at all any more: it stands on the start straight, which is its
/// own line beside the lap. See [`StartSpur`].
fn finish_at(prog: &TrackProgram) -> f32 {
    let run = prog.opening_straight();
    if run < 20.0 {
        return (prog.lap_length() * 0.06).clamp(10.0, 40.0);
    }
    (run * 0.4).clamp(10.0, 40.0)
}

/// How wide the start fans out, as a half-width in metres.
///
/// The gate row is 48 m across and it stands *on the track*, so the track has to be that wide
/// where it stands. This is not the riding line being too wide: a start straight is a fan
/// that funnels into turn one, and the published tracks measure 13 to 15 m at the finish line
/// with a 45 to 53 m row of gates on the start.
pub const START_FAN_HALF_M: f32 = GRID_STALLS as f32 * GRID_LANE_M * 0.5 + 3.0;

/// How far back from the gate row the fan reaches, metres. The lap's last corner feeds onto
/// the start straight, so the width has somewhere to come from.
const START_FAN_BEHIND_M: f32 = 25.0;

/// The start straight, as terrain: the spur beside the lap, how wide it is, and the ground
/// under it.
///
/// The geometry is [`crate::trackprog::StartLine`] — a gate row off to the side of the lap, a
/// sprint, and a corner that merges in. This is what the synthesiser needs on top of it: the
/// stations along it, the height it is cut to, and how far it opens out.
///
/// It is a fan, not a ribbon. Forty gates at 1.2 m are 48 m across and they stand *on* the
/// track, so the start is 54 m wide where they are and narrows to the riding line's own width
/// by the time it reaches the lap — which is what a start straight is: a wide pad funnelling
/// into turn one.
///
/// The pad is cut at the height of the lap beside it rather than at the height of the ground
/// it crosses. A start is a big earthwork on a real track for exactly that reason, and it is
/// also what makes the merge seamless: where the spur meets the lap the two are already the
/// same height, so there is no step to fall down.
#[derive(Clone, Debug)]
pub struct StartSpur {
    pub line: crate::trackprog::StartLine,
    pub stations: Vec<Station>,
    /// The height each station is cut to.
    pub deck: Vec<f32>,
    /// Half-width at the gate row, and the riding line's own.
    half: f32,
    line_half: f32,
    /// How far the sprint runs before the fan has narrowed to the riding line.
    funnel: f32,
    len: f32,
}

/// How far past the gate row its grooves run, and how deep they are at their deepest.
///
/// A start is the one place on a track where the ruts are a comb: every rider pulls out of
/// their own stall in a line, and the lines stay parallel until the pack starts to spread.
const GATE_RUT_M: f32 = 35.0;
const GATE_RUT_DEPTH_M: f32 = 0.09;

/// How much of the start straight's last stretch is already the width of the lap it joins.
const MERGE_TAIL_M: f32 = 12.0;

/// How much further the start pad's cut and fill reach than a track's shoulder does.
const START_BANK: f32 = 2.0;

/// How near the lap the start straight has to come before it is cut to the lap's height
/// rather than to the ground's, and how far away it stops caring. Inside the first figure the
/// two are the same surface; outside the second the start is its own earthwork.
const TIE_NEAR_M: f32 = 22.0;
const TIE_FAR_M: f32 = 70.0;

/// How much of the start's last stretch counts as the join, where the two surfaces are meant
/// to be one.
const MERGE_JOIN_M: f32 = 45.0;

/// How much ground the pad keeps behind the gate row, metres — where a real start has its
/// bank and its scoring tower.
const BACK_OF_THE_GATE_M: f32 = 8.0;

/// How far into the start straight the gate row stands, and how far past it the line is.
///
/// The row is a few metres in so the gates and the thirty-second board have dirt behind them.
const GATE_INSET_M: f32 = 5.0;

impl StartSpur {
    /// The spur a programme implies, with the ground it is cut into.
    ///
    /// `lap` and `lap_deck` are the lap's own stations and the height they are benched to —
    /// the pad takes its level from them.
    pub fn of(
        prog: &TrackProgram,
        lap: &[Station],
        lap_deck: &[f32],
        land: &Landscape,
    ) -> Option<StartSpur> {
        let line = prog.start_line()?;
        let walk = TrackProgram {
            start: line.start,
            segments: line.segments.clone(),
            features: Vec::new(),
            elevation: Vec::new(),
            ..prog.clone()
        };
        let stations = walk.stations(STATION_STEP);
        if stations.is_empty() || lap.len() != lap_deck.len() || lap.is_empty() {
            return None;
        }
        // It has to fit on the ground, banks and all. A start straight hanging off the edge of
        // the plot is benched to ground that isn't there — and a lap with no room beside it
        // for a start is a lap that gets none, which `trackllm::review` says out loud rather
        // than quietly building something broken.
        let margin = START_FAN_HALF_M + SHOULDER_M * START_BANK;
        let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
        if stations.iter().any(|q| {
            q.x < margin || q.z < margin || q.x > sx - margin || q.z > sz - margin
        }) {
            return None;
        }
        // What height the pad is cut at: the mean level of the ground it crosses, smoothed
        // hard along its length, and tied to the lap's own deck where the two meet.
        //
        // Not a straight ramp from one end to the other, which was the first try — a start
        // pad that ignores the ground it is on ends up eight metres into a hillside, and the
        // face of that cut is a wall whatever you batter it with. Not the nearest lap
        // station's deck either: as the spur turns in, the nearest bit of lap jumps from one
        // pass to another and takes four metres of height with it.
        let joins_at = line.joins_at;
        let landing = lap
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1.s - joins_at).abs().total_cmp(&(b.1.s - joins_at).abs()))
            .map(|(i, _)| lap_deck[i])
            .unwrap_or(0.0);
        // Averaged across the pad's width, so a hummock under one corner doesn't set the
        // level for the whole row.
        let (sx, sz) = (prog.terrain.size_x, prog.terrain.size_z);
        let across = |q: &Station, wide: f32| -> f32 {
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let n = 9;
            (0..n)
                .map(|k| {
                    let t = (k as f32 / (n - 1) as f32 - 0.5) * 2.0 * wide;
                    // Kept on the plot: the landscape is a formula and answers anywhere, but
                    // what it says past the edge is ground the track will never be built on.
                    let (x, z) = (
                        (q.x + rx * t).clamp(0.0, sx),
                        (q.z + rz * t).clamp(0.0, sz),
                    );
                    land.at(x, z)
                })
                .sum::<f32>()
                / n as f32
        };
        let half = START_FAN_HALF_M.max(prog.width * 0.5);
        let funnel = crate::trackprog::START_SPRINT_M;
        let len_all = stations.last().map(|q| q.s).unwrap_or(1.0).max(1.0);
        // The same width the pad comes out — see `at`, which this has to agree with or the
        // level is taken across ground the start does not cover.
        let wide_at = |s: f32| {
            let (from, to) = (funnel, (len_all - MERGE_TAIL_M).max(funnel + 1.0));
            let u = ((s - from) / (to - from)).clamp(0.0, 1.0);
            half + (prog.width * 0.5 - half) * smoothstep(u)
        };
        let mut deck: Vec<f32> = stations.iter().map(|q| across(q, wide_at(q.s))).collect();
        smooth_along(&mut deck, (BENCH_SMOOTH_M * 2.0 / STATION_STEP) as usize);
        // And where it comes near the lap, it takes the lap's height rather than the ground's.
        // By how close it is, not by how far along it is: the two only ever meet at the merge,
        // and that is where a difference between them would be a wall — the lap is benched
        // into the hill there and the ground beside it is not.
        let _ = landing;
        for (i, q) in stations.iter().enumerate() {
            let mut best = (f32::MAX, deck[i]);
            for (j, l) in lap.iter().enumerate() {
                let d = (l.x - q.x).powi(2) + (l.z - q.z).powi(2);
                if d < best.0 {
                    best = (d, lap_deck[j]);
                }
            }
            let d = best.0.sqrt();
            let u = ((TIE_FAR_M - d) / (TIE_FAR_M - TIE_NEAR_M)).clamp(0.0, 1.0);
            deck[i] += (best.1 - deck[i]) * smoothstep(u);
        }
        let len = stations.last().map(|q| q.s).unwrap_or(0.0);
        Some(StartSpur {
            line,
            stations,
            deck,
            half: START_FAN_HALF_M.max(prog.width * 0.5),
            line_half: prog.width * 0.5,
            funnel: crate::trackprog::START_SPRINT_M,
            len,
        })
    }

    /// How high the pad is cut, this far along it.
    ///
    /// Read off the distance along the line rather than off the nearest station's index: the
    /// label a distance transform puts on a cell is only reliable near the line it belongs
    /// to, and out on the edge of a 54 m pad a label that is fifteen metres out takes a metre
    /// of height with it — which is a wall.
    pub fn deck_at(&self, s: f32) -> f32 {
        if self.deck.is_empty() {
            return 0.0;
        }
        let x = (s / STATION_STEP).clamp(0.0, (self.deck.len() - 1) as f32);
        let i = x.floor() as usize;
        let (a, b) = (self.deck[i], self.deck[(i + 1).min(self.deck.len() - 1)]);
        a + (b - a) * (x - i as f32)
    }

    /// How wide the start is, this far along it.
    ///
    /// Full width the whole way down the sprint, and then narrowing *through the turn* — not
    /// tapering along the straight. Forty riders leave the gate abreast and are still abreast
    /// when they arrive at turn one; what squeezes them into single file is the corner, which
    /// is why a start straight reads as a wide slab with a funnel on the end of it rather than
    /// as a wedge.
    pub fn at(&self, s: f32) -> f32 {
        // The turn: from where the sprint ends to a little short of the merge, so the last
        // few metres are already the width of the track they join.
        let (from, to) = (self.funnel, (self.len - MERGE_TAIL_M).max(self.funnel + 1.0));
        let u = ((s - from) / (to - from)).clamp(0.0, 1.0);
        self.half + (self.line_half - self.half) * smoothstep(u)
    }

    pub fn length(&self) -> f32 {
        self.len
    }

    /// How wide the row of gates comes out, across the whole start.
    pub fn width_m(&self) -> f32 {
        self.half * 2.0
    }

    /// Whether the start is at the point of joining the lap. Everywhere before that the two
    /// are separate pieces of track with ground between them.
    pub fn merging(&self, s: f32) -> bool {
        s > self.len - MERGE_JOIN_M
    }

    /// Where the gate row stands, in metres along the start straight.
    pub fn gate_at(&self) -> f32 {
        GATE_INSET_M
    }
}

/// The pit lane: alongside the opening straight, a track's width off the racing line.
///
/// Stated once, because the race data puts the stalls here and the collision file has to
/// paint the same strip as pit surface. A pit lane the ground disagrees with is one the game
/// scores you off the track for using.
struct PitLane {
    stalls: usize,
    /// Signed metres from the centreline. Negative is the rider's left.
    lat: f32,
    /// Metres round the lap: the first stall, and the last.
    from: f32,
    to: f32,
    /// Half the width of the strip of pit surface under them.
    half_width: f32,
}

fn pit_lane(prog: &TrackProgram) -> PitLane {
    // Beside the lap's opening straight, on the side the start straight is not: the two both
    // want the ground alongside the main straight, and the start has first claim on it.
    let side = prog.start_line().map(|l| -l.side).unwrap_or(-1.0);
    let run = prog.opening_straight().max(prog.lap_length() * 0.1);
    let from = 10.0f32.min(run * 0.1);
    let stalls = (((run - from) / 5.0).floor() as usize).clamp(4, 16);
    PitLane {
        stalls,
        lat: side * (prog.width * 0.5 + 6.0),
        from,
        to: from + (stalls - 1) as f32 * 5.0,
        half_width: 4.0,
    }
}

fn rdf(prog: &TrackProgram, spur: Option<&StartSpur>) -> String {
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    let line = finish_at(prog);
    // Everything the file places by a `long` and a `lat` is placed *on the lap*, whatever it
    // is standing on — read straight off published tracks, whose gate rows sit out on a start
    // spur and are still stated in lap coordinates: Indiana's row is `long 457, lat -30`, and
    // the row it draws lands 30 m off the racing line at that point. Getting this wrong is
    // not subtle. Written along the start straight instead, the game read `long 5, lat -24`
    // as five metres round the lap and put forty riders across the middle of the track.
    let stations = prog.stations(0.5);
    let nearest = |x: f32, z: f32| -> Option<&Station> {
        stations.iter().min_by(|a, b| {
            let da = (a.x - x).powi(2) + (a.z - z).powi(2);
            let db = (b.x - x).powi(2) + (b.z - z).powi(2);
            da.total_cmp(&db)
        })
    };
    // A world position read as a position on the lap, in one station's frame.
    //
    // Projected onto that frame rather than taking the station's own distance round: stations
    // are half a metre apart, so a row of forty gates written to the nearest one comes out as
    // a staircase. The frame is passed in rather than looked up per point, which is the other
    // half of it — forty gates spread across fifty metres each find a *different* nearest
    // station, and on a lap that folds back on itself the nearest station to a gate at one end
    // can be on another pass altogether. One frame for the row, and it is a row.
    let project = |q: &Station, x: f32, z: f32, heading_deg: f32| -> (f32, f32, f32) {
        let (fx, fz) = crate::trackprog::heading_vector(q.heading);
        let (rx, rz) = crate::trackprog::right_vector(q.heading);
        let along = (x - q.x) * fx + (z - q.z) * fz;
        let lat = (x - q.x) * rx + (z - q.z) * rz;
        // And the angle relative to the lap's own direction there, which is how a published
        // stall states which way its bike faces.
        let angle = (heading_deg - q.heading.to_degrees()).rem_euclid(360.0);
        (q.s + along, lat, angle)
    };
    let onto_lap = |x: f32, z: f32, heading_deg: f32| -> (f32, f32, f32) {
        match nearest(x, z) {
            Some(q) => project(q, x, z, heading_deg),
            None => (0.0, 0.0, 0.0),
        }
    };

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

    let pits = pit_lane(prog);
    let (stalls, lane_lat) = (pits.stalls, pits.lat);
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
            pits.from + i as f32 * 5.0
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

    // One row of gates across the start straight, which is what a motocross start is.
    //
    // On the start straight and not on the lap: the row stands 40 m off the circuit, and the
    // `long` its stalls are stated at is measured along the start line — the `sa` line merged
    // into the height file beside the lap's own — not round the lap. That is what published
    // tracks do, and it is why a rider on a flying lap never crosses the gates.
    //
    // `posx`/`posz` is one *end* of the row, not its middle, so the anchor sits half a
    // row-width across and the gates come out centred on the line they stand on.
    let grid = GRID_STALLS;
    let lane = GRID_LANE_M;
    let span = grid as f32 * lane;
    let (gate, gate_at) = match spur {
        Some(spur) => (spur.line.start, spur.gate_at()),
        // No start straight — a lap with no straight to run one beside. The row goes at the
        // lap's own start, which is where it used to go, and the studio says why.
        None => (prog.start, 5.0),
    };
    let (fx, fz) = crate::trackprog::heading_vector(gate.angle.to_radians());
    let (rx, rz) = crate::trackprog::right_vector(gate.angle.to_radians());
    // The row's own middle, on the line it stands on — not one end of it. Indiana anchors at
    // (224, 148.5) and its start line begins at (220, 148); Briarcliff nineteen metres along
    // its own. Both sit on the centreline with the gates spread either side.
    let (mid_x, mid_z) = (gate.x + fx * gate_at, gate.z + fz * gate_at);
    s.push_str(&format!(
        "starting_grid\n{{\n\tnumstalls = {grid}\n\ttype = 1\n\tposx = {mid_x:.6}\n\
         \tposz = {mid_z:.6}\n\tangle = {:.6}\n\tnumstallsperrow = {grid}\n\
         \tdistfromstartline = 0.000000\n\tlanespacing = 0.000000\n\trowspacing = 0.000000\n\
         \tdifflat = 0.000000\n\tlanewidth = {:.6}\n\tlatshift = 0.000000\n\tside = 1\n",
        gate.angle, -lane
    ));
    // One frame for the whole row: the lap where the row's own middle meets it.
    let row_frame = nearest(mid_x, mid_z);
    for i in 0..grid {
        // Where the gate actually is, in the world, and then that point read back as a
        // position on the lap.
        let t = -span * 0.5 + (i as f32 + 0.5) * lane;
        let (x, z) = (mid_x + rx * t, mid_z + rz * t);
        let (long, lat, angle) = match row_frame {
            Some(q) => project(q, x, z, gate.angle),
            None => onto_lap(x, z, gate.angle),
        };
        s.push_str(&format!(
            "\tstall{i}\n\t{{\n\t\tlong = {long:.6}\n\t\tlat = {lat:.6}\n\
             \t\tangle = {angle:.6}\n\t}}\n"
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

    // The thirty-second board stands beside the gate row, out past the edge of the start
    // straight — and it too is stated on the lap.
    let board_lat = spur.map(|sp| sp.at(sp.gate_at()) + 3.0).unwrap_or(half + 3.0);
    let (bx, bz) = (
        mid_x - fx * 4.0 - rx * board_lat,
        mid_z - fz * 4.0 - rz * board_lat,
    );
    let (board_long, board_off, board_angle) = onto_lap(bx, bz, gate.angle);
    s.push_str(&format!(
        "30secondsboard_posx = {bx:.6}\n30secondsboard_posz = {bz:.6}\n\
         30secondsboard_angle = {:.6}\n\
         30seconds_board\n{{\n\tlong = {board_long:.6}\n\tlat = {board_off:.6}\n\
         \tangle = {board_angle:.6}\n}}\n",
        gate.angle - 90.0,
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
impl Synth {
    /// Metres outside the edge of the track — the lap's, or the start straight's, whichever
    /// this cell is nearer. Negative on the track itself.
    ///
    /// Every band that paints or surfaces the ground is measured from this rather than from
    /// the distance to the lap, because the start straight is track too: paint it by the lap
    /// alone and forty gates stand in a field.
    pub fn outside(&self, i: usize, half: f32) -> f32 {
        let lap = self.dist[i] - half;
        let mut e = lap;
        if let Some(spur) = &self.spur {
            let d = self.spur_dist[i];
            // No gap between the two. Measured off Mt Morris, whose start pad is flat ground
            // from its gate row right up to the racing line the whole way along: a start
            // straight and the lap it feeds are one surface, and a strip of field between
            // them is a ditch a rider drops into on the way to turn one.
            if d.is_finite() {
                e = e.min(d - spur.at(self.spur_arc[i]));
            }
        }
        e
    }

    /// How far outside the start straight's edge a point in the world is — negative on it,
    /// `None` where the track has no start straight.
    ///
    /// For anything that has to keep off the start: the pad is track, and a fence post or a
    /// hay bale standing on it is standing where forty riders are about to be.
    pub fn outside_the_start(&self, x: f32, z: f32) -> Option<f32> {
        let spur = self.spur.as_ref()?;
        let gx = (x / self.mps).round().clamp(0.0, (self.gw - 1) as f32) as usize;
        let gy = (z / self.mps).round().clamp(0.0, (self.gh - 1) as f32) as usize;
        let i = gy * self.gw + gx;
        let d = self.spur_dist[i];
        d.is_finite().then(|| d - spur.at(self.spur_arc[i]))
    }

    /// Whether a cell is on the start straight rather than on the lap.
    pub fn on_the_start(&self, i: usize, half: f32) -> bool {
        match &self.spur {
            Some(spur) => {
                let d = self.spur_dist[i];
                d.is_finite() && d - spur.at(self.spur_arc[i]) < self.dist[i] - half
            }
            None => false,
        }
    }
}

/// How many quads across the terrain the graphics mesh is built at.
///
/// Not the heightfield's own resolution — that would be four million vertices and a third of
/// a gigabyte. A published map carries about a million; this is a preview, and 256 quads over
/// a 700 m plot is under three metres a quad, which reads as ground from a bike.
const MAP_QUADS: usize = 256;

/// The `.map`: the terrain the game draws.
///
/// Written to the layout the game's own loader reads, which was not deduced but *traced*:
/// `scripts/map-loader-trace.py` runs the real loader under emulation with its read helper
/// hooked, so the byte layout below is what the shipped binary actually asks for. Every field
/// here was read off that trace against PiBoSo's OEM drag strip, and the result is checked the
/// same way — the loader walks a map written by this function to its last byte and returns
/// cleanly, exactly as it does on a published one.
///
/// This is version **304**. PiBoSo's own `mxb_track_example` is 288 and its records differ;
/// don't mix them.
///
/// ```text
/// 312 bytes   four empty mesh blocks: magic, 304, then only 0, 1 and +/-FLT_MAX
/// u32 A, B    the heightfield's dimensions, then A*B*2 bytes of 16-bit samples
/// f32 x3      ground size in metres, the height budget, and 5.0
/// u32 x3      zero
/// u32 n       how many layers follow
/// per layer:
///   56 bytes  a zero word and the layer's name
///   u32 1     one sheet
///     100 B   its name
///     u32 0, u32 width, u32 height
///     16 B    a hash the loader keeps and we don't need
///     u32 0   no sub-records
///     u32     the payload's length, counting the eight zero bytes after it
///     8 B     zero
///     bytes   the sheet as raw DEFLATE over RGBA
///     u32 0   no secondary map hangs off it
///   f32 x2    how many times the sheet tiles across the ground
///   u32       1 if a mask follows, 0 for the base layer that covers everything
///     u32 w, u32 h, u32 len, then the mask as raw DEFLATE over one byte a texel
///   f32 1.0
/// 32 bytes    zero: the vegetation and detail lists, all empty
/// ```
/// What the game keys its texture cache on: the MD5 of a sheet's pixels, before deflating.
///
/// Measured on every sheet of PiBoSo's drag strip -- and it is content-addressed, so two
/// layers sharing a texture share a hash.
fn sheet_hash(rgba: &[u8]) -> [u8; 16] {
    use md5::Digest;
    md5::Md5::digest(rgba).into()
}

/// A tangent-space normal map derived from a ground sheet's own luma.
///
/// Every terrain layer on a published map carries one of these beside its colour sheet. The
/// encoding is two-channel — the normal lives in red and green, blue is a constant 255 and
/// alpha a constant 3 — measured off PiBoSo's drag strip.
fn normal_pixels(rgba: &[u8], dim: usize, strength: f32) -> Vec<u8> {
    let luma = |x: usize, y: usize| -> f32 {
        let i = (y % dim) * dim * 4 + (x % dim) * 4;
        (0.299 * rgba[i] as f32 + 0.587 * rgba[i + 1] as f32 + 0.114 * rgba[i + 2] as f32) / 255.0
    };
    let mut out = vec![0u8; dim * dim * 4];
    for y in 0..dim {
        for x in 0..dim {
            let dx = luma((x + 1) % dim, y) - luma((x + dim - 1) % dim, y);
            let dy = luma(x, (y + 1) % dim) - luma(x, (y + dim - 1) % dim);
            let (nx, ny, nz) = (-dx * strength, -dy * strength, 1.0);
            let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
            let i = (y * dim + x) * 4;
            out[i] = (((nx / len) * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            out[i + 1] = (((ny / len) * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            out[i + 2] = 255;
            out[i + 3] = 3;
        }
    }
    out
}

fn map(prog: &TrackProgram, syn: &Synth) -> Vec<u8> {
    const FMAX: u32 = 0x7F7F_FFFF;
    const NFMAX: u32 = 0xFF7F_FFFF;
    let mut out: Vec<u8> = Vec::new();
    let u = |v: u32| v.to_le_bytes();
    let f = |v: f32| v.to_le_bytes();

    // Four empty mesh blocks, and nothing in them but zero, one and +/-FLT_MAX.
    //
    // The mesh at the front of a `.map` is scenery, not ground. Measured on a published
    // track (JV Spain): of its thirty-two materials exactly two lie on the terrain surface,
    // twenty draw groups of eight hundred and fifty-eight, and the rest stands above it --
    // tents, fences, banners -- and runs a kilometre past the terrain square. PiBoSo's own
    // OEM drag strip declares no materials, no vertices and no triangles at all and is a
    // hundred and twenty megabytes, every one of them in the trailing block.
    //
    // We built a ground mesh here from the heightfield and textured it with the base sheet.
    // It was added when the ground came out black, on the belief that a map declaring no
    // geometry draws nothing; the black was really the layer records being read out of
    // phase, which is fixed. What the mesh left behind was a surface lying exactly on the
    // terrain -- something no published map has -- covering the bands that draw underneath.
    let mut node = Vec::new();
    for w in [FMAX, FMAX, FMAX, NFMAX, NFMAX, NFMAX] {
        node.extend_from_slice(&u(w));
    }
    node.extend_from_slice(&[0u8; 20]);

    out.extend_from_slice(b"MP2\0");
    out.extend_from_slice(&u(304));
    out.extend_from_slice(&u(0)); // materials
    out.extend_from_slice(&u(0)); // vertices
    out.extend_from_slice(&u(0)); // triangles
    out.extend_from_slice(&u(1));
    out.extend_from_slice(&node);
    out.extend_from_slice(&u(0)); // no mesh textures
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&u(1));
    out.extend_from_slice(&node);
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&u(1));
    out.extend_from_slice(&node);
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&u(1));
    out.extend_from_slice(&node);
    out.extend_from_slice(&[0u8; 68]);
    debug_assert_eq!(out.len(), 312, "the prefix is a fixed 312 bytes");

    // The terrain, read by 0x14025f180: the grid, its samples, then the ground's size in both
    // axes, the height budget and three zeros. Rancho reads all of this before its layers.
    out.extend_from_slice(&u(syn.gw as u32));
    out.extend_from_slice(&u(syn.gh as u32));
    out.extend_from_slice(&raw16(syn, prog.terrain.scale));
    out.extend_from_slice(&f(prog.terrain.size_x));
    out.extend_from_slice(&f(prog.terrain.size_z));
    out.extend_from_slice(&f(prog.terrain.scale));
    out.extend_from_slice(&[0u8; 12]);

    // One layer per painted band, in the order they are painted over each other. The first
    // covers everything and so carries no mask; the rest are masked.
    //
    // The same list the exported track is painted from, so what the studio draws is what gets
    // ridden. It used to be a second copy of it here, and two lists that have to agree by hand
    // are two lists that will not.
    let seed = prog.terrain.relief.seed;
    let dim = GROUND_TEXTURE_DIM;
    let half = prog.width * 0.5;
    let bands = layers(prog);
    out.extend_from_slice(&u(bands.len() as u32));
    for l in bands {
        let (sheet, look, salt, tile, mask_to) = (l.sheet, &l.look, l.salt, l.tile_m, l.band);
        // A material record: fourteen floats, the shape every published map uses. This is not
        // a name field -- writing text here hands the renderer garbage coefficients.
        out.extend_from_slice(&f(0.0));
        for _ in 0..6 {
            out.extend_from_slice(&f(1.0));
        }
        out.extend_from_slice(&[0u8; 28]);

        // The sheet count, taken at 0x14025f2ca between the material record and the sheet.
        out.extend_from_slice(&u(1));
        let mut rec = vec![0u8; 100];
        let n = sheet.len().min(99);
        rec[..n].copy_from_slice(&sheet.as_bytes()[..n]);
        out.extend_from_slice(&rec);
        out.extend_from_slice(&u(0));
        out.extend_from_slice(&u(dim as u32));
        out.extend_from_slice(&u(dim as u32));
        let rgba = band_pixels(dim, look, seed ^ salt);
        // The game keys its texture cache on this, and it is the MD5 of the pixels before
        // they are deflated. Leave it zero and every sheet in the file is the same texture.
        out.extend_from_slice(&sheet_hash(&rgba));
        out.extend_from_slice(&u(0));
        let px = deflate_raw(&rgba);
        out.extend_from_slice(&u(px.len() as u32 + 8));
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&px);

        // A secondary map hangs off the colour sheet, and it is what gives the band relief.
        //
        // Measured by tracing the game's loader over a published map (JV Spain, version 304,
        // the same as ours): its `soil_light_c` layer sets this word to one, follows it with
        // four words, and then a sheet record for `soil_white_n_s` — after which the loader
        // goes straight to the tiling floats, with no second flag. Its `soil_dark_c` layer
        // sets the word to zero and the tiling follows immediately, which is what we used to
        // write for every band: colour with nothing to catch the light.
        //
        // The secondary's header is NOT the primary's. The loader takes its dimensions at
        // name+100 where a colour sheet's are at name+104 — there is no flag word in front
        // of them — and then the hash, a zero, and a length counting the eight bytes behind
        // it. Reusing the colour record's shape here puts every field one word out.
        out.extend_from_slice(&u(1));
        for w in [0u32, 0, 1, 0] {
            out.extend_from_slice(&u(w));
        }
        let nrm = normal_pixels(&rgba, dim, NORMAL_STRENGTH);
        let packed = deflate_raw(&nrm);
        // Name, dimensions, hash, a zero, and the length: 132 bytes, then the eight the
        // length counts, then the pixels. Every offset off the trace of Spain's own.
        let mut rec = vec![0u8; 132];
        // Published maps name them off the colour sheet: `soil_light_c` carries
        // `soil_white_n_s`. Ours drop the `_c` and take `_n_s`.
        let name = format!("{}_n_s", sheet.trim_end_matches("_c"));
        let n = name.len().min(99);
        rec[..n].copy_from_slice(&name.as_bytes()[..n]);
        rec[100..104].copy_from_slice(&(dim as u32).to_le_bytes());
        rec[104..108].copy_from_slice(&(dim as u32).to_le_bytes());
        rec[108..124].copy_from_slice(&sheet_hash(&nrm));
        rec[128..132].copy_from_slice(&((packed.len() + 8) as u32).to_le_bytes());
        out.extend_from_slice(&rec);
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&packed);

        // How often the sheet repeats, the mask that says where this band covers the ground,
        // and the float that closes the layer. The terrain's layer reader takes these at
        // 0x14025f370 through 0x14025f405.
        let (rx, rz) = repetitions(prog, tile);
        out.extend_from_slice(&f(rx as f32));
        out.extend_from_slice(&f(rz as f32));
        match mask_to {
            BandMask::Everywhere => out.extend_from_slice(&u(0)),
            edge => {
                let (mw, mh) = (syn.gw - 1, syn.gh - 1);
                let m = band_mask(syn, edge, half, seed, mw, mh);
                let packed = deflate_raw(&m);
                out.extend_from_slice(&u(1));
                out.extend_from_slice(&u(mw as u32));
                out.extend_from_slice(&u(mh as u32));
                out.extend_from_slice(&u(packed.len() as u32 + 8));
                out.extend_from_slice(&u(2));
                out.extend_from_slice(&u(0));
                out.extend_from_slice(&packed);
            }
        }
        // One word closes a layer, and only one. Measured by running the game's own loader
        // over the file: after a layer's sheet it reads five words -- the secondary-map
        // flag, the two tiling floats, the mask flag and this one -- and then goes straight
        // to the next layer's fifty-six byte material record.
        //
        // We wrote three words here instead of one, so every layer after the first was read
        // eight bytes out of phase. Layer 1's material came back as (1.4e-45, 0.0, ...)
        // where a real one is (0.0, 1.0, 1.0, ...), its sheet count as zero, and the loader
        // walked on through the rest as empty records. The shoulder, the riding line and the
        // grass were never drawn: a generated track's ground was the base sheet alone, one
        // flat colour from fence to fence.
        out.extend_from_slice(&u(0));
    }

    // The trailing lists, every one of them empty. After the layers the loader reads a run of
    // count words -- 0x140211ab3, 0x140211af6, 0x140211bb0, 0x140211d01, 0x140211d44,
    // 0x140211d87, 0x140211e64 and on -- each the length of a list it then allocates and
    // reads. A zero apiece says the list is empty. Thirty-two is comfortably more than it
    // asks for; the walk test pins how many it actually takes.
    // The lists that close the file, every one of them empty.
    out.extend_from_slice(&[0u8; 128]);
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
/// A short texture record — the shape the game reads after a `kind` word. It has no leading
/// flag word, so its dimensions sit at name+100 and its length at name+124, and the word
/// after that length is the pixel format: 0 for RGBA, 2 for one byte a texel.
fn short_record(name: &str, w: u32, h: u32, fmt: u32, px: &[u8]) -> Vec<u8> {
    let payload = deflate_raw(px);
    let mut rec = vec![0u8; 136];
    let n = name.len().min(99);
    rec[..n].copy_from_slice(&name.as_bytes()[..n]);
    rec[100..104].copy_from_slice(&w.to_le_bytes());
    rec[104..108].copy_from_slice(&h.to_le_bytes());
    rec[108..124].copy_from_slice(&sheet_hash(px));
    rec[124..128].copy_from_slice(&((payload.len() + 8) as u32).to_le_bytes());
    rec[128..132].copy_from_slice(&fmt.to_le_bytes());
    rec.extend_from_slice(&payload);
    rec
}

fn texture_record(name: &str, w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let payload = deflate_raw(rgba);
    // A **primary** record's header, measured on a published map: 104 bytes of name field,
    // the dimensions, the payload length at +132, eight zero bytes, and the pixels at +144.
    //
    // Ours used the 104-byte-earlier variant, and our own scanner accepts either — it tries
    // both. That is how this went unnoticed: the two layouts do both occur in a real map, but
    // not interchangeably. Every *colour* sheet in Indiana is at +104 and every *secondary*
    // — the `_n_s` normal maps hanging off them — is at +100. We write colour sheets, so
    // ours belong at +104.
    let mut rec = vec![0u8; 144];
    let n = name.len().min(39);
    rec[..n].copy_from_slice(&name.as_bytes()[..n]);
    rec[104..108].copy_from_slice(&w.to_le_bytes());
    rec[108..112].copy_from_slice(&h.to_le_bytes());
    // The game keys its texture cache on the MD5 of the pixels before they are deflated.
    rec[112..128].copy_from_slice(&sheet_hash(rgba));
    // The length at +132 counts the eight zero bytes that follow it, which stay zero.
    rec[132..136].copy_from_slice(&((payload.len() + 8) as u32).to_le_bytes());
    rec.extend_from_slice(&payload);

    // Every colour record in a published map is followed by a descriptor, and ours were
    // followed by the next record's name. A reader that *walks* — which is what the game
    // does, whatever our own scanner gets away with — then takes eleven bytes of "grass_c"
    // as five words and a count, and everything after it is wherever those bytes point.
    //
    // Read off Indiana: `1, 1, 0, 1.0f, 1.0f, N`, where N is how many secondary maps hang
    // off this one. `pitlane_c` says 1 and is followed inline by `pitlane_n_s`; that is how
    // 49 materials come to ship 84 sheets. We generate colour and nothing else, so ours say
    // none. The five leading words are the same on every sample.
    for w in [1u32, 1, 0] {
        rec.extend_from_slice(&w.to_le_bytes());
    }
    rec.extend_from_slice(&1.0f32.to_le_bytes());
    rec.extend_from_slice(&1.0f32.to_le_bytes());
    rec.extend_from_slice(&0u32.to_le_bytes()); // no secondary maps
    rec
}

fn deflate_raw(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(bytes);
    e.finish().unwrap_or_default()
}

/// The preview as a `.pkz` **the app** can open: a plain zip, which is what the reader falls
/// back to when a file isn't one of PiBoSo's encrypted ones.
///
/// **The game cannot load what this writes, and it is not meant to.** The `.map` and `.trh` in
/// here are ours; the game's are `terrained.exe`'s, and every generated-track crash from
/// 2026-08-31 to 2026-09-05 came from installing one of these. It exists for the studio's own
/// viewer, which parses it, and for the tests. Anything going to a game folder is built by
/// [`write_source`] and then compiled — `_map.bat`, `_trh.bat` and `_centerline.bat` beside the
/// exported files are the three commands, in that order.
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
    let (map_img, shot) = ui_images(prog, syn, UI_IMAGE_DIM);
    // Every published track puts its files in a folder named after itself, and the game
    // looks for them there — flat at the archive root they are not found at all.
    for (name, bytes) in [
        (format!("{slug}/{slug}.trh"), trh(prog, syn, paint_features)),
        (format!("{slug}/{slug}.map"), map(prog, syn)),
        (format!("{slug}/{slug}.ini"), crlf(&track_ini(prog))),
        (format!("{slug}/{slug}.rdf"), crlf(&rdf(prog, syn.spur.as_ref()))),
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

/// The heightmap samples, little-endian u16, quantised against the height budget, in the
/// order the rest of this module indexes rows: `z = row * mps_z`.
fn raw16(syn: &Synth, scale: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(syn.heights.len() * 2);
    for h in &syn.heights {
        let v = (h / scale * u16::MAX as f32).round().clamp(0.0, 65535.0) as u16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// The same samples as `heightmap.raw` wants them: **bottom row first**.
///
/// TerrainEd reads the raw the way it reads a TGA — from the bottom of the picture up — so
/// the last row of the file is the one that lands at `z = 0`. Everything else this module
/// writes counts rows the other way: the masks (bottom-left-origin TGAs, which come out the
/// right way round on their own), the centreline, the start line. Handing TerrainEd the rows
/// in `syn` order put the ground upside down under paint that wasn't, and a track's jumps
/// ended up nowhere near where it was painted.
///
/// Measured, not guessed. Compile a track, then correlate the `.trh` heightfield against the
/// raw that made it: in `syn` order it comes back +0.067 as-is and +1.0000 vertically
/// flipped; written this way it is +1.0000 as-is, which is the score the masks already had.
fn heightmap_raw(syn: &Synth, scale: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(syn.heights.len() * 2);
    for row in syn.heights.chunks_exact(syn.gw).rev() {
        for h in row {
            let v = (h / scale * u16::MAX as f32).round().clamp(0.0, 65535.0) as u16;
            out.extend_from_slice(&v.to_le_bytes());
        }
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
fn mask_rect(syn: &Synth, mw: usize, mh: usize, f: impl Fn(f32, f32, f32, f32) -> u8) -> Vec<u8> {
    let mut out = vec![0u8; mw * mh];
    for y in 0..mh {
        let gy = (y * syn.gh / mh).min(syn.gh - 1);
        for x in 0..mw {
            let gx = (x * syn.gw / mw).min(syn.gw - 1);
            let i = gy * syn.gw + gx;
            out[y * mw + x] = f(
                syn.dist[i],
                syn.arc[i],
                gx as f32 * syn.mps,
                gy as f32 * syn.mps,
            );
        }
    }
    out
}

/// How big the patches of grass in the field are, metres.
const TURF_PATCH_M: f32 = 55.0;

/// How much turf covers the field at a point: 1 where grass has taken, 0 where the ground is
/// bare.
///
/// The field used to be one flat sheet of grass from the shoulder to the fence, which is a
/// lawn — and no venue is a lawn. Two scales of noise, thresholded, so what comes out is
/// patches of grass in worked ground with bare tracks between them, which is what the ground
/// round a motocross track actually looks like.
fn turf_cover(x: f32, z: f32, seed: u32) -> f32 {
    let broad = fbm(x / TURF_PATCH_M, z / TURF_PATCH_M, seed ^ 0x3C71);
    let fine = fbm(x / (TURF_PATCH_M * 0.28), z / (TURF_PATCH_M * 0.28), seed ^ 0x3C72);
    let n = broad * 0.72 + fine * 0.28;
    // `fbm` runs either side of zero, so the threshold does too: bare where the field dips
    // well below it, full turf where it rises, and a soft edge in between. About two thirds
    // of the ground comes out grassed, which is what a venue looks like from the air.
    smoothstep(((n + 0.25) * 2.5).clamp(0.0, 1.0))
}

/// A mask of the track's edge — the lap's and the start straight's together.
///
/// `f` is handed how far outside the edge the cell is, and where it is, which is everything a
/// band needs: the riding surface is `<= 0`, the shoulder a few metres past it, the field
/// beyond that.
fn mask_rect_outside(
    syn: &Synth,
    mw: usize,
    mh: usize,
    half: f32,
    f: impl Fn(f32, f32, f32) -> u8,
) -> Vec<u8> {
    let mut out = vec![0u8; mw * mh];
    for y in 0..mh {
        let gy = (y * syn.gh / mh).min(syn.gh - 1);
        for x in 0..mw {
            let gx = (x * syn.gw / mw).min(syn.gw - 1);
            let i = gy * syn.gw + gx;
            out[y * mw + x] = f(
                syn.outside(i, half),
                gx as f32 * syn.mps,
                gy as f32 * syn.mps,
            );
        }
    }
    out
}

fn mask_outside(syn: &Synth, dim: usize, half: f32, f: impl Fn(f32, f32, f32) -> u8) -> Vec<u8> {
    mask_rect_outside(syn, dim, dim, half, f)
}

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

/// A mask whose value depends on which side of the line a cell is, not just how far off it.
///
/// `mask_from` hands out an unsigned distance, which is all a band symmetric about the
/// centreline needs. Anything that reads as a *track* rather than a stripe is not symmetric:
/// the racing line sits inside the corner, the berm is outside it, the roost lands on one
/// shoulder. Those need the rider's left told from their right, which is what the nearest
/// station's heading gives.
///
/// The callback gets a [`Where`], which is everything a mask can ask about one cell.
///
/// Sized as a width and a height rather than one edge. The `.map` asks for a mask the shape of
/// the terrain grid, and on a track that is not square this handed it a square one — the right
/// number of bytes only when `gw == gh`, and a mask read row-shifted against the ground on
/// every rectangular track.
fn mask_across(syn: &Synth, mw: usize, mh: usize, f: impl Fn(Where) -> u8) -> Vec<u8> {
    let back = (RUT_MARK_LOOKBACK_M / STATION_STEP) as usize;
    let n = syn.stations.len();
    let mut out = vec![0u8; mw * mh];
    for y in 0..mh {
        let gy = (y * syn.gh / mh).min(syn.gh - 1);
        for x in 0..mw {
            let gx = (x * syn.gw / mw).min(syn.gw - 1);
            let i = gy * syn.gw + gx;
            let at = syn.station[i] as usize;
            let st = &syn.stations[at];
            let (wx, wz) = (gx as f32 * syn.mps, gy as f32 * syn.mps);
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let lat = (wx - st.x) * rx + (wz - st.z) * rz;
            out[y * mw + x] = f(Where {
                i,
                lat,
                off: lat - syn.line_lat[at],
                k: st.curvature,
                s: syn.arc[i],
                x: wx,
                z: wz,
                rut: syn.rut[i],
                face: syn.face[at],
                lead: syn.line_lat[(at + n - back % n) % n] - syn.line_lat[at],
            });
        }
    }
    out
}

/// One cell of the ground, as a mask sees it.
struct Where {
    /// Which cell it is, for anything that has to read the start straight as well as the lap.
    i: usize,

    /// Metres off the centreline, signed, positive to the rider's right.
    lat: f32,
    /// Metres off the *racing* line, same sign.
    off: f32,
    /// The line's curvature here, positive turning right.
    k: f32,
    /// Metres round the lap.
    s: f32,
    /// And where on the ground the cell actually is.
    x: f32,
    z: f32,
    /// What the ruts did here — see [`Synth::rut`].
    rut: f32,
    /// How steeply the built ground climbs along the lap here: positive up a takeoff, negative
    /// down a landing, zero on the flat. See [`Synth::face`].
    face: f32,
    /// Where the racing line was a corner's-length back, relative to where it is here, in the
    /// same frame as `off`. This is the side riders are arriving from.
    lead: f32,
}

/// Which cells a band of the preview map covers.
#[derive(Clone, Copy)]
enum BandMask {
    /// The base band: everything, and so no mask at all.
    Everywhere,
    /// Out to the riding line's own width, plus this many metres. Not a fixed distance: the
    /// start fans out to hold the gate row, and dirt painted at one width leaves it green.
    Out(f32),
    /// Past the shoulder — the field and the turf on it.
    Beyond,
    /// The ridden line: a strip about where people actually go, this many metres either side
    /// of it before the edge starts to tear.
    ///
    /// Not `Out(0.0)`. That is the whole corridor, and painting the ridden colour over all of
    /// it is a black ribbon from edge to edge with nothing for a mark to stand against — the
    /// corridor is worked dirt, and the line worn through it is the dark part.
    Line(f32),
    /// The packed racing line.
    Rut,
    /// Loose dirt off the line and round the outside of a bend.
    Loose,
}

/// Where one painted band goes, as coverage per cell.
///
/// Factored out so the `.map` writer and anything that wants to *look* at the ground cannot
/// disagree about where a band is — the picture beside a track and the track itself have to
/// be the same track.
fn band_mask(
    syn: &Synth,
    band: BandMask,
    half: f32,
    seed: u32,
    mw: usize,
    mh: usize,
) -> Vec<u8> {
    match band {
        // Nothing to mask: it is the ground everything else is painted over.
        BandMask::Everywhere => vec![255; mw * mh],
        BandMask::Rut => rut_mask(syn, half, seed, mw, mh),
        BandMask::Loose => loose_mask(syn, half, seed, mw, mh),
        BandMask::Beyond => mask_rect_outside(syn, mw, mh, half, |e, x, z| {
            (band_beyond(e, x, z, SHOULDER_M, seed ^ 0xB3ED) as f32 * turf_cover(x, z, seed)) as u8
        }),
        BandMask::Out(extra) => mask_rect_outside(syn, mw, mh, half, move |e, x, z| {
            band_edge(e, x, z, extra, seed ^ 0xB3ED)
        }),
        BandMask::Line(w) => line_mask(syn, half, w, seed, mw, mh),
    }
}

/// The racing line: the strip the tyres actually pack down, damp and dark.
///
/// It leans to the inside of a corner because `line_lat` does, and it wanders, because a band
/// of exactly constant width running the whole lap reads as a stripe painted down the middle
/// rather than as ground anyone has ridden on.
///
/// Shared by the exported masks and the preview map, so the studio cannot show a line in a
/// different place from the one the game gets.
/// How much of the start pad a cell is, and what its ground is doing there.
///
/// `None` where the cell is not on the start straight at all, so the lap's own rules run.
fn on_start_pad(syn: &Synth, i: usize) -> Option<(f32, f32, f32)> {
    let spur = syn.spur.as_ref()?;
    let d = syn.spur_dist[i];
    if !d.is_finite() {
        return None;
    }
    let s = syn.spur_arc[i];
    let wide = spur.at(s);
    if d > wide {
        return None;
    }
    // How far in from the edge, how far along from the gate row, and the comb itself.
    let across = 1.0 - (d / wide.max(1e-3)).clamp(0.0, 1.0);
    let from_gate = s - spur.gate_at();
    Some((across, from_gate, d))
}

/// The comb of grooves the gate leaves, as coverage.
fn start_rut(syn: &Synth, i: usize, seed: u32) -> Option<u8> {
    let (_, from_gate, d) = on_start_pad(syn, i)?;
    if !(-1.0..GATE_RUT_M).contains(&from_gate) {
        return Some(0);
    }
    let row = GRID_STALLS as f32 * GRID_LANE_M * 0.5;
    if d > row {
        return Some(0);
    }
    let along = smoothstep(1.0 - (from_gate.max(0.0) / GATE_RUT_M));
    // The same comb the ground is cut with, and the same phase — it is even in the offset, so
    // it lines up either side of the line without needing to know which side it is.
    let lane = (d / GRID_LANE_M) * std::f32::consts::TAU;
    let groove = (0.5 - 0.5 * lane.cos()).powf(1.6);
    let wobble = 0.85 + 0.3 * fbm(d * 0.4, from_gate * 0.12, seed ^ 0x71F3);
    Some((255.0 * (groove * along * wobble).clamp(0.0, 1.0)) as u8)
}

/// Churned ground over the pad, thinner where the grooves packed it down.
fn start_loose(syn: &Synth, i: usize, seed: u32) -> Option<u8> {
    let (across, from_gate, d) = on_start_pad(syn, i)?;
    let patchy = (0.45 + 0.55 * fbm(d * 0.05, from_gate * 0.05, seed ^ 0x2D19)).clamp(0.0, 1.0);
    let packed = if (-1.0..GATE_RUT_M).contains(&from_gate) {
        let lane = (d / GRID_LANE_M) * std::f32::consts::TAU;
        1.0 - 0.55 * (0.5 - 0.5 * lane.cos())
    } else {
        1.0
    };
    // Fading out at the edges, where the pad meets ground nothing has driven on.
    let edge = smoothstep((across * 3.0).clamp(0.0, 1.0));
    Some((255.0 * (patchy * packed * edge * 0.8).clamp(0.0, 1.0)) as u8)
}

/// The ridden line: the strip of the corridor people actually ride, worn dark.
///
/// It leans and wanders with the racing line, tears at its edges like every other band, and
/// thins where the ground is loose — a line is not a stripe of constant width, and the places
/// it frays are the places a rider reads to find it.
fn line_mask(syn: &Synth, half: f32, w: f32, seed: u32, mw: usize, mh: usize) -> Vec<u8> {
    mask_across(syn, mw, mh, |c| {
        // The start pad is ridden ground too — forty bikes leave it dark.
        if let Some(v) = start_loose(syn, c.i, seed) {
            return (v as f32 * 0.85) as u8;
        }
        if c.lat.abs() > half + RUT_CORRIDOR_FADE_M + 0.1 {
            return 0;
        }
        // Wider up the face of a jump, where everybody's line is written down, and wider
        // again through a corner, where they spread across the whole of it.
        let spread = 1.0 + LINE_CORNER_SPREAD * (c.k.abs() * FULL_LEAN_RADIUS_M).clamp(0.0, 1.0);
        let width = w * spread + edge_noise(c.x, c.z, seed ^ 0x64B1, 1.1, 0.8);
        let strip = soft_edge(width, LINE_FADE_M, c.off.abs()) as f32 / 255.0;
        let corridor = soft_edge(half, RUT_CORRIDOR_FADE_M, c.lat.abs()) as f32 / 255.0;
        // Never solid. A line packs unevenly and the gaps are what make it read as ground
        // somebody rode rather than as a stripe somebody painted.
        let patchy = (0.72 + 0.40 * fbm(c.x * 0.07, c.z * 0.07, seed ^ 0x51A9)).clamp(0.55, 1.0);
        (255.0 * strip * corridor * patchy) as u8
    })
}

fn rut_mask(syn: &Synth, half: f32, seed: u32, mw: usize, mh: usize) -> Vec<u8> {
    mask_across(syn, mw, mh, |c| {
        // The start pad's own marks: forty bikes pulling out of forty stalls leave a comb of
        // lines, and the ground has them cut into it — see the gate grooves in `synthesise`.
        // Painted here so they are something a rider can see rather than only feel.
        if let Some(v) = start_rut(syn, c.i, seed) {
            return v;
        }
        // Past where the corridor's own fade reaches, and not a step inside it: the fade
        // below runs 1.6 m out from the edge, so cutting at half a metre truncated it at
        // about seventy per cent coverage — a hard boundary at the resolution of the terrain
        // grid, which is the staircase that ran down the edge of every corner.
        if c.lat.abs() > half + RUT_CORRIDOR_FADE_M + 0.1 {
            return 0;
        }
        // Up the face of a jump the strip widens and slides towards the side riders are
        // arriving from.
        //
        // A takeoff is the one place on a track where everybody's line is written down. They
        // come off the last corner wherever it left them and scrub up the ramp from there, so
        // a face after a corner that runs you wide is black on the outside and clean on the
        // inside — which is a thing a rider reads the approach off, and the reason the marks
        // are worth having at all rather than being a texture detail.
        let up = (c.face / RUT_MARK_FACE).clamp(0.0, 1.0);
        let w = RUT_HALF_WIDTH_M
            + edge_noise(c.x, c.z, seed ^ 0x51C7, 1.5, 0.7)
            + RUT_MARK_FAN_M * up;
        let off = c.off - c.lead * RUT_MARK_LEAN * up;
        // Wherever the ground has a groove, not only inside the packed strip.
        //
        // The strip is about a metre and a half either side of the racing line, and the
        // grooves are not: measured across a built corner, 4.6 m of a twelve-metre width
        // carries a rut signal past 0.15. So the sheet that makes a groove readable was
        // painting a fifth of the grooves, and a rider reported exactly that — the line
        // legible in places and simply missing in others.
        //
        // The strip still leads: it is full strength on the line and falls to a floor across
        // the rest of the corridor, and `keyed` below decides where within that the sheet
        // actually lands. The ground's own rut signal is what places it.
        // A ridden strip has an edge. Fading it over a metre and a third is what turned the
        // line into a smear with no boundary — a rider reads where the good dirt stops.
        let strip = soft_edge(w, 0.6, off.abs()) as f32 / 255.0;
        let corridor = soft_edge(half, RUT_CORRIDOR_FADE_M, c.lat.abs()) as f32 / 255.0;
        let band = (strip + (1.0 - strip) * RUT_PAINT_OFF_LINE) * corridor;
        // Into the grooves and off the walls beside them.
        //
        // The packed strip is not the whole width of the line: it is the floor a tyre
        // polished, and the bank thrown up beside it is material nothing has driven on. Keying
        // the coverage to the ground's own rut signal is what makes the two read apart at
        // riding speed — without it a rut is only a shape, and a shape painted the colour of
        // the ground it is cut into is a shape nobody sees until they are in it.
        // Sharpened. `c.rut` runs -1 on a groove's floor to +1 on the bank beside it, and
        // taking it straight paints a gradient across the pair — which at riding scale is a
        // soft stripe, not a groove with a lit side and a shaded one. Raised to a power the
        // floor is dark over its whole width and the bank is not, so the eye gets an edge.
        let floor = (-c.rut).clamp(0.0, 1.0).powf(0.7);
        let wall = c.rut.clamp(0.0, 1.0).powf(0.45);
        let keyed = (RUT_PAINT_FLOOR + (1.0 - RUT_PAINT_FLOOR) * floor - RUT_PAINT_WALL * wall)
            .clamp(0.0, 1.0);
        // And never a solid sheet of it. A line packs unevenly — damp here, blown out there —
        // and one texture at full coverage down the whole lap is the single thing that made
        // the line read as a stripe of paint rather than as ground.
        let patchy = (0.66 + 0.42 * fbm(c.x * 0.085, c.z * 0.085, seed ^ 0x3A71)
            + 0.16 * fbm(c.x * 0.31, c.z * 0.31, seed ^ 0x77C2))
            .clamp(0.48, 1.0);
        (255.0 * band * keyed * patchy) as u8
    })
}

/// Loose dirt: the outside of a bend and the edges of the track, where the roost lands and
/// nothing packs it down.
///
/// Two things put it there — how far a cell is off the racing line, and how far round the
/// outside of a corner it is — and it takes the stronger of the two, so a straight still gets
/// dry edges without the corner term inventing any.
fn loose_mask(syn: &Synth, half: f32, seed: u32, mw: usize, mh: usize) -> Vec<u8> {
    mask_across(syn, mw, mh, |c| {
        // The pad is churned ground: forty bikes stand on it once and tear it up, and nothing
        // rides it again. Patchy loose dirt over the whole of it, thinner where the grooves
        // are because that is where the tyres packed it.
        if let Some(v) = start_loose(syn, c.i, seed) {
            return v;
        }
        if c.lat.abs() > half + RUT_CORRIDOR_FADE_M + 0.1 {
            return 0;
        }
        let bend = (c.k.abs() * FULL_LEAN_RADIUS_M).clamp(0.0, 1.0);
        let outside = (-c.k.signum() * c.lat / half.max(0.1)).clamp(0.0, 1.0) * bend;
        let edge = ((c.off.abs() - RUT_HALF_WIDTH_M - 1.1) / 2.0).clamp(0.0, 1.0);
        // And the wall beside every groove, which is the loosest ground on the track: material
        // a tyre threw there this morning and nothing has driven on since. It is also the half
        // of a rut that catches the light, so painting it the dry colour is what turns a
        // groove from a dark smear into something with a lit side and a shaded one.
        let wall = (c.rut * RUT_LIP_SHARP).clamp(0.0, 1.0) * RUT_LIP_LOOSE;
        let patchy =
            (0.54 + 0.46 * fbm(c.x * 0.045, c.z * 0.045, seed ^ 0x2D18)).clamp(0.0, 0.92);
        // Loose ground off the line is patchy; the bank beside a groove is not. Letting the
        // same patchiness eat into it is what left the wall at a quarter coverage and the rut
        // reading nine levels off its own floor.
        // Faded at the corridor's edge for the same reason the line is. Loose dirt ran to
        // full coverage and stopped dead at the boundary, which put the same staircase down
        // the outside of the track that the line had down its inside.
        let corridor = soft_edge(half, RUT_CORRIDOR_FADE_M, c.lat.abs()) as f32 / 255.0;
        let v = (edge.max(outside) * patchy).max(wall * (0.8 + 0.2 * patchy)) * corridor;
        (255.0 * v) as u8
    })
}

/// How far in or out a painted edge wanders at a given place on the ground, metres.
///
/// Two scales: a long wander that makes the band wide here and narrow there, and a short one
/// that gives the boundary itself a torn look instead of a drawn one.
fn edge_noise(x: f32, z: f32, seed: u32, long_m: f32, short_m: f32) -> f32 {
    // Three scales, not two. At 34 m and 6 m a band edge is a straight line for metres at a
    // time — rendered at riding scale it reads as a painted lane, which is what a track's
    // ground must not look like. The third is at the scale of the ground itself.
    fbm(x / 34.0, z / 34.0, seed) * long_m
        + fbm(x / 6.0, z / 6.0, seed ^ 0x9F1) * short_m
        // The tearing one, and it stops here rather than going finer.
        //
        // A mask is compiled onto the terrain's own grid — 0.39 m a cell on a 400 m plot — so
        // an edge that wanders at 0.7 m cannot be drawn: it comes out as a row of stair-steps
        // a cell deep, which is what "sharp teeth" turned out to be from the seat. Nothing
        // below about four cells is worth asking a mask for.
        + fbm(x / 1.6, z / 1.6, seed ^ 0x3C7) * short_m * 0.8
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

/// Over how many metres a band cut by distance from the line fades out at its edge.
///
/// [`soft_edge`] says why a hard cut cannot be used, and the bands cut by distance — the
/// shoulder, the riding line, the grass beyond them — were the ones still using one. A mask
/// is blended, so at a quarter of a metre a sample the boundary came back as a staircase a
/// sample deep and two long, on every band edge on the track. It is what made a generated
/// track's ground read as jagged teeth rather than as ground somebody dug.
const BAND_FADE_M: f32 = 0.9;

/// And how far that edge wanders off the distance defining it, metres.
///
/// Feathered but straight, a band edge is still a line a fixed distance from the centre of the
/// track, and it reads as one. The dirt on a real track reaches where the machine reached.
const BAND_WANDER_M: f32 = 0.9;

/// Where a band cut by distance from the riding line ends: full inside, gone a fade past it,
/// and the edge itself wandering.
///
/// One definition, because the same bands are cut three times over — into the `.map` the game
/// reads, into the `.tga` masks TerrainEd is handed, and into the preview the app draws — and
/// three edges that disagree are a track whose picture is not its ground.
fn band_edge(e: f32, x: f32, z: f32, at: f32, seed: u32) -> u8 {
    soft_edge(at + edge_noise(x, z, seed, BAND_WANDER_M, BAND_WANDER_M * 0.45), BAND_FADE_M, e)
}

/// The ground past every band, which is the same edge read from the other side.
fn band_beyond(e: f32, x: f32, z: f32, at: f32, seed: u32) -> u8 {
    255 - band_edge(e, x, z, at, seed)
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
    /// The published sheet this band is painted with — see [`photo`].
    ///
    /// Ground is a photograph. Everything above draws one instead, and only gets the chance
    /// when the asset will not decode.
    photo: Option<&'static str>,
    /// What to multiply that photograph by, so a sand track comes out sand.
    ///
    /// `[1.0; 3]` on soil, which is what the sheets were shot on.
    tone: [f32; 3],
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
    rgba_tga(dim, &band_pixels(dim, look, seed))
}

/// A published track's own ground, as a photograph.
///
/// Indiana Pro's own terrain sheets, lifted out of its `.map` by [`tests::dump_ground_sheets`]:
/// the light soil over the whole site, the dark soil of its riding line, the packed bottom its
/// ruts wear down to, and its grass. [`ground_pixels`] draws ground instead of photographing
/// it, and is the fallback behind these.
///
/// Returns `(dim, rgba)`; the sheets are square.
fn photo(name: &str) -> Option<&'static (usize, Vec<u8>)> {
    macro_rules! sheet_of {
        ($cell:ident, $file:literal) => {{
            static $cell: std::sync::OnceLock<(usize, Vec<u8>)> = std::sync::OnceLock::new();
            let sheet = $cell.get_or_init(|| {
                match image::load_from_memory(include_bytes!($file)) {
                    Ok(img) => {
                        let img = img.to_rgba8();
                        (img.width() as usize, img.into_raw())
                    }
                    Err(_) => (0, Vec::new()),
                }
            });
            (sheet.0 > 0).then_some(sheet)
        }};
    }
    match name {
        "soil_light" => sheet_of!(A, "../assets/ground/soil_light_c.jpg"),
        "soil_dark" => sheet_of!(B, "../assets/ground/soil_dark_c.jpg"),
        "packed" => sheet_of!(C, "../assets/ground/sand_bottom.jpg"),
        "grass" => sheet_of!(D, "../assets/ground/hm_grass.jpg"),
        _ => None,
    }
}

/// One band's sheet at `dim`: the photograph it names, toned and resampled.
///
/// The one place a band's pixels come from — the exported `.tga`, the sheet baked into the
/// `.map` and every picture drawn of the ground all come through here.
fn band_pixels(dim: usize, look: &GroundLook, seed: u32) -> Vec<u8> {
    let Some((sheet_dim, src)) = look.photo.and_then(photo) else {
        return ground_pixels(dim, look, seed);
    };
    let mut px = flatten_tile(resample_sheet(src, *sheet_dim, dim), dim);
    if look.tone != [1.0; 3] {
        for p in px.chunks_exact_mut(4) {
            for c in 0..3 {
                p[c] = (p[c] as f32 * look.tone[c]).clamp(0.0, 255.0) as u8;
            }
        }
    }
    px
}

/// Take the slow variation out of a sheet, so tiling it does not draw a grid.
///
/// A ground sheet is laid a hundred times across a track. Anything it carries at the scale of
/// its own tile — one corner a little darker, a broad patch of lighter soil — repeats with it,
/// and from above that is a chequerboard: reported from the seat as squares of texture stuck
/// together. A photograph of ground always has some, because the light on the day it was shot
/// had some.
///
/// So each pixel is measured against a heavily blurred copy of itself and the difference is
/// what survives, about the sheet's own mean. Grain — which is what makes it read as dirt —
/// is untouched; the gradient that makes the tile visible is not.
fn flatten_tile(mut px: Vec<u8>, dim: usize) -> Vec<u8> {
    // Measured on a small copy of the sheet rather than on the sheet. A low-pass is a low-pass
    // at any resolution, and blurring a 1024 square over a sixth of its own width directly is
    // two billion additions a channel — enough to make exporting a track feel broken.
    const COARSE: usize = 64;
    if dim < COARSE * 2 {
        return px;
    }
    let block = dim / COARSE;
    for c in 0..3 {
        let mut small = vec![0.0f32; COARSE * COARSE];
        for y in 0..COARSE {
            for x in 0..COARSE {
                let mut sum = 0.0;
                for by in 0..block {
                    for bx in 0..block {
                        let i = (y * block + by) * dim + x * block + bx;
                        sum += px[i * 4 + c] as f32;
                    }
                }
                small[y * COARSE + x] = sum / (block * block) as f32;
            }
        }
        let blur = box_blur_wrap(&small, COARSE, (COARSE / 6).max(1));
        let mean = blur.iter().sum::<f32>() / blur.len() as f32;
        // Bilinear back up, so the correction has no edges of its own.
        let at = |u: f32, v: f32| -> f32 {
            let (fx, fy) = (u * COARSE as f32 - 0.5, v * COARSE as f32 - 0.5);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            let g = |ix: f32, iy: f32| {
                let ix = (ix as isize).rem_euclid(COARSE as isize) as usize;
                let iy = (iy as isize).rem_euclid(COARSE as isize) as usize;
                blur[iy * COARSE + ix]
            };
            let top = g(x0, y0) + (g(x0 + 1.0, y0) - g(x0, y0)) * tx;
            let bot = g(x0, y0 + 1.0) + (g(x0 + 1.0, y0 + 1.0) - g(x0, y0 + 1.0)) * tx;
            top + (bot - top) * ty
        };
        for y in 0..dim {
            for x in 0..dim {
                let i = y * dim + x;
                let slow = at((x as f32 + 0.5) / dim as f32, (y as f32 + 0.5) / dim as f32);
                let flat = px[i * 4 + c] as f32 - (slow - mean) * TILE_FLATTEN;
                px[i * 4 + c] = flat.clamp(0.0, 255.0) as u8;
            }
        }
    }
    px
}

/// How much of a sheet's slow variation is taken out. All of it is flat; none of it tiles
/// visibly. Three quarters leaves the ground looking like ground.
const TILE_FLATTEN: f32 = 0.75;

/// Box-average a square RGBA sheet to `dim`. Nearest where that would be an enlargement,
/// which nothing shipped asks for — the sheets are 1024 and so is the size they go out at.
fn resample_sheet(src: &[u8], sheet_dim: usize, dim: usize) -> Vec<u8> {
    if sheet_dim == dim {
        return src.to_vec();
    }
    let mut out = Vec::with_capacity(dim * dim * 4);
    for y in 0..dim {
        let y0 = y * sheet_dim / dim;
        let y1 = ((y + 1) * sheet_dim / dim).max(y0 + 1).min(sheet_dim);
        for x in 0..dim {
            let x0 = x * sheet_dim / dim;
            let x1 = ((x + 1) * sheet_dim / dim).max(x0 + 1).min(sheet_dim);
            let mut sum = [0u32; 3];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let i = (yy * sheet_dim + xx) * 4;
                    for c in 0..3 {
                        sum[c] += src[i + c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out.extend_from_slice(&[
                (sum[0] / n) as u8,
                (sum[1] / n) as u8,
                (sum[2] / n) as u8,
                255,
            ]);
        }
    }
    out
}

/// RGBA pixels in the container TerrainEd reads: the same bytes with the channels swapped.
fn rgba_tga(dim: usize, rgba: &[u8]) -> Vec<u8> {
    let mut px = Vec::with_capacity(rgba.len());
    for p in rgba.chunks_exact(4) {
        px.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    tga_bgra(dim, dim, &px)
}

/// A ground sheet's normal map, in the form TerrainEd reads from a `.shd`.
///
/// Not the same encoding as the one baked into a compiled `.map`, which is two-channel with
/// a constant blue — see [`normal_pixels`]. A source `_n.tga` carries the whole normal: the
/// example track's `dirt_n.tga` averages (128, 127, 241) across its three colour channels,
/// with red and green spread the full width of the byte. Its alpha is the specular level,
/// which is where the shader picks the highlight up when `specular` names no map of its own.
///
/// The specular is modulated by the sheet's own luma, because the pale stones in soil catch
/// light and the crevices between them do not.
fn normal_tga(rgba: &[u8], dim: usize, strength: f32, spec: u8) -> Vec<u8> {
    let luma = |x: usize, y: usize| -> f32 {
        let i = (y % dim) * dim * 4 + (x % dim) * 4;
        (0.299 * rgba[i] as f32 + 0.587 * rgba[i + 1] as f32 + 0.114 * rgba[i + 2] as f32) / 255.0
    };
    let mut px = Vec::with_capacity(dim * dim * 4);
    for y in 0..dim {
        for x in 0..dim {
            let dx = luma((x + 1) % dim, y) - luma((x + dim - 1) % dim, y);
            let dy = luma(x, (y + 1) % dim) - luma(x, (y + dim - 1) % dim);
            let (nx, ny, nz) = (-dx * strength, -dy * strength, 1.0);
            let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
            let enc = |v: f32| ((v / len * 0.5 + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            let a = (spec as f32 * (0.35 + 0.65 * luma(x, y))).clamp(0.0, 255.0) as u8;
            // The container is BGRA, so the normal's z goes down first.
            px.extend_from_slice(&[enc(nz), enc(ny), enc(nx), a]);
        }
    }
    tga_bgra(dim, dim, &px)
}

/// The same ground after rain.
///
/// Water darkens soil: the film on top stops the surface scattering, so less light comes
/// back. It does not tint it — see [`WET_DARKEN`], which is measured off the example track's
/// own pair of sheets. The shine comes from the wet sheet's shader, not from its colour.
fn wet_pixels(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let w = |v: u8| (v as f32 * WET_DARKEN) as u8;
            [w(p[0]), w(p[1]), w(p[2]), p[3]]
        })
        .collect()
}

/// How much of the sky a wet sheet throws back, and how sharply.
struct Reflect {
    min: f32,
    max: f32,
    exp: f32,
}

/// The shader that goes beside a ground sheet.
///
/// TerrainEd looks for `<sheet>.shd` next to every texture the `.hmf` names and bakes what it
/// finds into the map. A sheet without one is lit flat — no relief, no highlight — which is
/// what every terrain layer this generated used to be, and most of why the ground read as a
/// painted surface rather than dirt.
///
/// Paths inside are relative to the sheet's own folder, not to the project: `maps/line.shd`
/// says `line_n.tga`, not `maps/line_n.tga`.
fn shd(normal: &str, repetitions: u32, shininess: u32, reflect: Option<Reflect>) -> String {
    let mut s = format!(
        "bump\n{{\n\tmap = {normal}\n\trepetitions = {repetitions}\n}}\n\n\
         specular\n{{\n\tshininess = {shininess}\n}}\n"
    );
    if let Some(r) = reflect {
        s.push_str(&format!(
            "\nreflection\n{{\n\tfactormin = {}\n\tfactormax = {}\n\tfactorexp = {}\n\
             \tenvmap = env/env.tga\n\tenvmap_add = 1\n}}\n",
            r.min, r.max, r.exp
        ));
    }
    s
}

/// The cube a wet layer reflects.
///
/// Six faces, because TerrainEd resolves `envmap = env/env.tga` to `env_right`, `env_left`,
/// `env_top`, `env_bottom`, `env_front` and `env_back` beside it. What a film of water on
/// soil actually shows is sky above the horizon and ground below it, so that is all these
/// are — and they are never seen in focus.
///
/// The sky matches the `.amb`'s own fog, because the two are looked at together.
///
/// The two colours are up here rather than inside `env_faces` because the track's picture is
/// rendered under the same sky the game puts over it, and a picture with its own weather in it
/// is a picture of a different track.
const ZENITH: [f32; 3] = [96.0, 140.0, 196.0];
const HORIZON: [f32; 3] = [179.0, 179.0, 217.0];
fn env_faces(dim: usize) -> Vec<(&'static str, Vec<u8>)> {
    const GROUND: [f32; 3] = [86.0, 74.0, 60.0];

    let mix = |a: [f32; 3], b: [f32; 3], t: f32| -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        [
            (a[0] + (b[0] - a[0]) * t) as u8,
            (a[1] + (b[1] - a[1]) * t) as u8,
            (a[2] + (b[2] - a[2]) * t) as u8,
        ]
    };
    let face = |shade: &dyn Fn(f32) -> [u8; 3]| -> Vec<u8> {
        let mut px = Vec::with_capacity(dim * dim * 4);
        for y in 0..dim {
            let c = shade((y as f32 + 0.5) / dim as f32);
            for _ in 0..dim {
                px.extend_from_slice(&[c[2], c[1], c[0], 255]);
            }
        }
        tga_bgra(dim, dim, &px)
    };
    // Row zero is the bottom of a TGA, so a side face runs ground first and sky last.
    let side = face(&|v: f32| {
        if v < 0.5 {
            mix(GROUND, HORIZON, v * 2.0)
        } else {
            mix(HORIZON, ZENITH, (v - 0.5) * 2.0)
        }
    });
    vec![
        ("right", side.clone()),
        ("left", side.clone()),
        ("front", side.clone()),
        ("back", side),
        ("top", face(&|_| mix(ZENITH, ZENITH, 0.0))),
        ("bottom", face(&|_| mix(GROUND, GROUND, 0.0))),
    ]
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
            // Starting at three cells across the tile put a metre-and-a-half blotch in every
            // copy of a four-metre sheet, and a sheet laid a hundred times across a track
            // repeats every blotch with it — which from the seat is a chequerboard printed on
            // the ground. Indiana's soil has no blotch at any scale: it is grain, and its
            // whole tile spreads 21 grey levels about its mean where ours spread nearly thirty
            // on the low frequency alone. So the patching starts finer and carries less, and
            // the variation a rider actually reads across a track comes from the masks, which
            // are stretched over the terrain once and do not repeat at all.
            let mut m = 0.0;
            let mut amp = 1.0;
            let mut period = 7;
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
    let g = ground_looks(s);
    let (field, ridden) = (g.field, g.ridden);
    let mean = |look: &GroundLook| -> [u8; 3] {
        const DIM: usize = 256;
        let px = band_pixels(DIM, look, 0x9A0D);
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

/// Every ground a track is painted with, from what it says it is made of.
struct Grounds {
    /// Bound ground with stones showing: everything the track is not.
    field: GroundLook,
    /// The corridor's base — worked soil, darker and wetter, nearly all clods.
    ridden: GroundLook,
    /// And the strip of it people actually ride, darker again.
    line: GroundLook,
    /// The graded shoulder, between the two, and most of what a rider sees from the seat.
    shoulder: GroundLook,
    /// The strip the tyres pack down: darker again, and smooth where the clods are gone.
    rut: GroundLook,
    /// Dry chewed dirt off the line and round the outside of a bend, where the roost lands.
    loose: GroundLook,
    /// The turf over the top.
    turf: GroundLook,
}

/// The grounds, from what the track says it is made of.
///
/// A track that is one colour from edge to edge is one a rider cannot read: nothing says
/// where the line goes or where the track stops until they are already there. So the corridor
/// is not one band but three — the base, the packed line inside it and the loose stuff at its
/// edges — and they are spread far enough apart in tone to tell apart at speed.
fn ground_looks(surface: Surface) -> Grounds {
    // Read off Indiana's own sheets rather than picked. `soil_light_c` averages (172, 134,
    // 99) and `soil_dark_c` (50, 36, 24) — a bright tan field against a nearly black riding
    // line, and the gap between them is far wider than any two colours anyone would guess.
    // These are the numbers *before* shading, which lands around three quarters of them.
    let (base, line): ([f32; 3], [f32; 3]) = match surface {
        // The line is lighter than Indiana's own (50, 36, 24) on purpose. That figure is what
        // a sheet averages under a photographer's light; in the game, with the track's sky
        // over it and its own shadows on it, a line that dark stops reading as a line at all —
        // ridden, you cannot see where the groove is. Lifted until it does, and no further:
        // the gap to the field is what makes a racing line visible, and that gap is still
        // more than a hundred levels.
        Surface::Soil => ([179.0, 140.0, 104.0], [86.0, 63.0, 44.0]),
        Surface::Sand => ([214.0, 193.0, 152.0], [176.0, 152.0, 114.0]),
        Surface::Grass => ([174.0, 142.0, 100.0], [84.0, 62.0, 43.0]),
    };
    // The sheets were shot on Indiana, which is soil, so a soil track takes them as they are
    // and a sand or grass one pulls them to its own palette by the ratio of the two bases.
    let soil = |b: [f32; 3], of: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|c| if of[c] > 0.0 { b[c] / of[c] } else { 1.0 })
    };
    let (soil_base, soil_line) = ([179.0, 140.0, 104.0], [86.0, 63.0, 44.0]);
    let ground_tone = soil(base, soil_base);
    let line_tone = soil(line, soil_line);
    let field = GroundLook {
        base,
        photo: Some("soil_light"),
        tone: ground_tone,
        grain_tint: (0.82, 1.13),
        fleck: [196.0, 190.0, 176.0],
        fleck_density: 0.03,
        litter: [186.0, 168.0, 112.0],
        litter_density: 1.0,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 0.30,
        mottle: 0.05,
        contrast: 0.34,
    };
    let ridden = GroundLook {
        base: line,
        photo: Some("soil_dark"),
        tone: line_tone,
        grain_tint: (0.70, 1.28),
        fleck: [150.0, 146.0, 138.0],
        fleck_density: 0.03,
        litter: [140.0, 122.0, 84.0],
        litter_density: 0.25,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 1.0,
        mottle: 0.06,
        contrast: 0.62,
    };
    // The graded shoulder: the field's colour, worked over like the line.
    let shoulder = GroundLook {
        // Graded and dry, and brighter than the field it runs beside. Sitting it halfway
        // between the field and the line put it at the same brightness as the turf, which
        // left the edge of the track with no step in it — only a change of hue, and hue is
        // the first thing to go at speed and in flat light.
        base: [
            base[0] * 1.06 + 6.0,
            base[1] * 1.04 + 5.0,
            base[2] * 1.02 + 4.0,
        ],
        photo: Some("soil_light"),
        tone: [
            ground_tone[0] * 1.06,
            ground_tone[1] * 1.04,
            ground_tone[2] * 1.02,
        ],
        grain_tint: (0.85, 1.11),
        fleck: [165.0, 160.0, 150.0],
        fleck_density: 0.03,
        litter: [172.0, 156.0, 106.0],
        litter_density: 0.6,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 0.65,
        mottle: 0.05,
        contrast: 0.31,
    };
    let grass = GroundLook {
        base: [100.0, 114.0, 62.0],
        photo: Some("grass"),
        tone: [1.0; 3],
        grain_tint: (0.55, 1.32),
        fleck: [126.0, 132.0, 78.0],
        fleck_density: 0.02,
        litter: [182.0, 172.0, 102.0],
        litter_density: 1.4,
        blade: ([78.0, 104.0, 44.0], [148.0, 168.0, 88.0]),
        blade_density: 34.0,
        clods: 0.35,
        coarse: 0.25,
        mottle: 0.08,
        contrast: 0.7,
    };
    // The packed line: darker and wetter than the ground it is worn into, and smoother —
    // the clods are gone where a tyre has been over them a thousand times. Kept a good way
    // off `ridden` in tone, because two shades of the same brown at riding speed is one
    // shade.
    let rut = GroundLook {
        // main's, not this branch's 0.55. Darkening the sheet was one way to answer "the
        // shadow from a rut to the ground has to be harder", and it is the wrong one: under
        // the track's own sky and shadows a line that dark is one a rider cannot find, which
        // is the other half of the same report. The contrast this branch wanted comes from
        // `RUT_PAINT_FLOOR`, `RUT_PAINT_WALL` and `RUT_LIP_SHARP` keying the paint to the
        // ground's own rut signal — which is a contrast *within* the line rather than of the
        // line against everything else.
        base: [line[0] * 0.78, line[1] * 0.78, line[2] * 0.76],
        // Not the line's sheet darkened: a rut's floor is polished rather than worked, and
        // Indiana ships that as its own photograph. Toned down, because that photograph is
        // *lighter* than the dark soil of the line — 61 against 50 — and a groove painted
        // lighter than the line it is cut into is a groove nobody can find.
        photo: Some("packed"),
        tone: [
            line_tone[0] * RUT_FLOOR_DARKEN,
            line_tone[1] * RUT_FLOOR_DARKEN,
            line_tone[2] * RUT_FLOOR_DARKEN,
        ],
        // Polished is not featureless. Measured against the sheets a published track bakes
        // into its own `.map`, this one read a spread of 6.1 grey levels and a pixel-to-pixel
        // grain of 2.59, where Indiana's three terrain sheets run 16-20 and 11-17 — near
        // enough a solid colour. A solid colour laid down the middle of the track is what
        // reads from the seat as the texture being broken and the line impossible to find. A
        // packed rut is smooth in its *shape*; the dirt in it is still dirt.
        grain_tint: (0.44, 1.52),
        fleck: [128.0, 124.0, 118.0],
        fleck_density: 0.045,
        litter: [120.0, 104.0, 72.0],
        litter_density: 0.28,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 0.72,
        coarse: 0.42,
        mottle: 0.18,
        contrast: 1.00,
    };
    // Loose dirt: dry, so it reads light against everything around it, and coarse, because
    // it is the stuff that has been thrown there rather than driven on.
    let loose = GroundLook {
        base: [
            line[0] + (base[0] - line[0]) * 0.52,
            line[1] + (base[1] - line[1]) * 0.52,
            line[2] + (base[2] - line[2]) * 0.52,
        ],
        // Dry roost, and dry ground is *pale*. It used to be toned to a colour part way
        // between the line and the field, which on a corridor that is now the field's own
        // soil put dark blotches over light ground in no pattern anybody could read. What is
        // thrown off a line and never driven on again dries out and goes lighter than what is
        // around it.
        photo: Some("soil_light"),
        tone: [
            ground_tone[0] * LOOSE_DRY,
            ground_tone[1] * LOOSE_DRY,
            ground_tone[2] * LOOSE_DRY,
        ],
        grain_tint: (0.84, 1.14),
        fleck: [188.0, 182.0, 168.0],
        fleck_density: 0.04,
        litter: [180.0, 162.0, 108.0],
        litter_density: 0.5,
        blade: ([0.0; 3], [0.0; 3]),
        blade_density: 0.0,
        clods: 1.0,
        coarse: 1.0,
        mottle: 0.06,
        contrast: 0.40,
    };
    // The line is the corridor's own soil, worn down to what a published track paints its
    // riding line with. The corridor around it is lifted off that: a track from above is a
    // dark brown ribbon with a darker line down it, not a pale one with a black stripe.
    let line_band = GroundLook { tone: line_tone, ..ridden };
    let ridden = GroundLook {
        tone: [
            line_tone[0] * CORRIDOR_LIFT,
            line_tone[1] * CORRIDOR_LIFT,
            line_tone[2] * CORRIDOR_LIFT,
        ],
        ..ridden
    };
    Grounds { field, ridden, line: line_band, shoulder, rut, loose, turf: grass }
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
///
/// They are not the same kind of picture and never were. The map is a diagram: the game draws
/// the route and the riders over it, so it stays flat, north-up and unshaded. The other one is
/// a photograph of the place, and it is rendered as one — see [`ui_shot`].
fn ui_images(prog: &TrackProgram, syn: &Synth, dim: usize) -> (Vec<u8>, Vec<u8>) {
    let mut map = vec![0u8; dim * dim * 4];
    for y in 0..dim {
        // Row zero of a TGA is the bottom of the picture, and row zero of the grid is `z = 0`,
        // so the read runs from the far edge back. Get this wrong and the lap comes out
        // mirrored against the route the game draws over it.
        let row = dim - 1 - y;
        let gy = (row * syn.gh / dim).min(syn.gh - 1);
        for x in 0..dim {
            let gx = (x * syn.gw / dim).min(syn.gw - 1);
            let c: [u8; 3] = if syn.corridor[gy * syn.gw + gx] {
                [60, 70, 150]
            } else {
                [232, 232, 236]
            };
            let at = (y * dim + x) * 4;
            map[at..at + 4].copy_from_slice(&[c[2], c[1], c[0], 255]);
        }
    }
    (tga_bgra(dim, dim, &map), ui_shot(prog, syn, dim))
}

/// The picture beside the track's name: the terrain rendered from above and off to one side.
///
/// It used to be a false-colour relief of the heightfield with the corridor tinted orange over
/// it — the same plan view as the map, in worse colours, and it told a player nothing about
/// the place they were about to ride. This is the ground as it will actually look: the six
/// painted bands the `.map` ships, lit by the sun the `.amb` declares, seen from a camera that
/// places itself to fit the lap and to keep the sun behind it.
fn ui_shot(prog: &TrackProgram, syn: &Synth, dim: usize) -> Vec<u8> {
    // The camera has to frame the lap, not the terrain — a track in one corner of a big
    // landscape would otherwise be a smudge in the middle of a field. Every eighth corridor
    // cell is plenty to bound a shape with.
    let mut focus = Vec::new();
    for gy in (0..syn.gh).step_by(8) {
        for gx in (0..syn.gw).step_by(8) {
            if syn.corridor[gy * syn.gw + gx] {
                focus.push((gx as f32 * syn.mps, gy as f32 * syn.mps));
            }
        }
    }
    // A track with no corridor at all is not one anybody asked for, but the camera still has
    // to go somewhere: the terrain itself.
    if focus.is_empty() {
        focus = vec![
            (0.0, 0.0),
            (prog.terrain.size_x, 0.0),
            (0.0, prog.terrain.size_z),
            (prog.terrain.size_x, prog.terrain.size_z),
        ];
    }

    let albedo = ground_sheet(prog, syn, dim);
    // Straight off the `.amb`: `sun_position`, and the `clear` condition's light. The picture
    // is of the track in the weather the game opens it in.
    let sun = {
        let v = [2.0f32, 10.0, -7.0];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let scene = crate::trackshot::Scene {
        gw: syn.gw,
        gh: syn.gh,
        mps: syn.mps,
        heights: &syn.heights,
        albedo: &albedo,
        adim: dim,
        focus: &focus,
        sun,
        sun_colour: [1.10, 0.95, 0.70],
        ambient: [0.40, 0.45, 0.55],
        zenith: ZENITH,
        horizon: HORIZON,
        // The `.amb`'s own fog colour, which is what the distance goes to in the game too.
        haze: [179.0, 179.0, 217.0],
        tilt_deg: crate::trackshot::TILT_DEG,
    };
    let rgb = crate::trackshot::render(&scene, dim);
    // The renderer hands back rows from the top; a TGA's row zero is the bottom.
    let mut px = Vec::with_capacity(dim * dim * 4);
    for y in (0..dim).rev() {
        for x in 0..dim {
            let c = rgb[y * dim + x];
            px.extend_from_slice(&[c[2], c[1], c[0], 255]);
        }
    }
    tga_bgra(dim, dim, &px)
}

/// The ground's colour across the terrain: the same bands the `.map` paints, composited in
/// the same order they are painted in.
///
/// It walks `layers` rather than listing the bands again, so the picture cannot show a track
/// painted differently from the one shipped beside it. What it does not do is tile the sheets
/// — at a couple of metres to the pixel a 4.5 m tile of soil is below the picture's own
/// resolution — so each band contributes its base colour with the broad mottle that survives
/// at this scale and nothing finer.
fn ground_sheet(prog: &TrackProgram, syn: &Synth, dim: usize) -> Vec<[f32; 3]> {
    let seed = prog.terrain.relief.seed;
    let half = prog.width * 0.5;
    let mut px = vec![[0.0f32; 3]; dim * dim];
    for l in layers(prog) {
        // What this band's sheet actually averages, generated the way the shipped one is.
        //
        // It used to take the look's own `base` and a flat 0.78 for the shading, which is a
        // guess about a sheet the code can simply make: the ground sheet averages 0.98 of its
        // base and the turf 0.55 of its own, because a sheet of grass is mostly blades. That
        // gap is why a track that comes out green in the game was a desert in its picture.
        let mean = sheet_mean(&l.look, seed ^ l.salt);
        let cover = band_mask(syn, l.band, half, seed, dim, dim);
        for y in 0..dim {
            let gy = (y * syn.gh / dim).min(syn.gh - 1);
            for x in 0..dim {
                let a = cover[y * dim + x] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let gx = (x * syn.gw / dim).min(syn.gw - 1);
                let (wx, wz) = (gx as f32 * syn.mps, gy as f32 * syn.mps);
                // The same patching `ground_pixels` gives the sheets, at the only scale a
                // picture this size can hold it: without it the ground is flat colour and the
                // track reads as a drawing again.
                let k = 1.0 + l.look.mottle * fbm(wx * 0.06, wz * 0.06, seed ^ l.salt);
                let at = y * dim + x;
                for c in 0..3 {
                    px[at][c] += (mean[c] * k - px[at][c]) * a;
                }
            }
        }
    }
    px
}

/// How much of its base colour a painted sheet keeps once it is shaded.
///
/// `ground_pixels` draws each band as clods and crevice rather than as flat colour, and what
/// comes out lands around three quarters of the colour that went in. The picture composites
/// the base colours directly, so it has to take the same cut or every band in it is brighter
/// than the ground it is a picture of.
/// The average colour of a band's sheet, made the way the shipped one is made.
///
/// Small on purpose — 32 px is 1024 samples of the same generator, which settles the mean of
/// anything the sheet does — and cheap enough to call per band per picture.
fn sheet_mean(look: &GroundLook, salt: u32) -> [f32; 3] {
    let px = band_pixels(32, look, salt);
    let mut sum = [0.0f32; 3];
    let n = (px.len() / 4).max(1);
    for p in px.chunks_exact(4) {
        sum[0] += p[0] as f32;
        sum[1] += p[1] as f32;
        sum[2] += p[2] as f32;
    }
    [sum[0] / n as f32, sum[1] / n as f32, sum[2] / n as f32]
}

const SHEET_SHADE: f32 = 0.78;

/// Uncompressed 32-bit BGRA, the mask in the alpha channel — the shape the official example's
/// own masks are in, down to the descriptor byte and the file footer.
/// A colour sheet from one channel of variation and a base tint.
///
/// The 3D grass reads its colour off one of these, so it wants the turf's own green with
/// enough patching across the terrain that a field of blades isn't one flat colour.
fn tga_tinted(w: usize, h: usize, level: &[u8], base: [f32; 3]) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 4);
    for &l in level {
        let k = l as f32 / 190.0;
        let c = |i: usize| (base[i] * k).clamp(0.0, 255.0) as u8;
        px.extend_from_slice(&[c(2), c(1), c(0), 255]);
    }
    tga_bgra(w, h, &px)
}

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
/// Per axis, at least one, and rounded, because it is a count of tiles. A rectangular
/// terrain given a single count stretches its ground along the long axis — which is what
/// TerrainEd's separate `repetitions_x` and `repetitions_z` are for.
fn repetitions(prog: &TrackProgram, tile_m: f32) -> (u32, u32) {
    let n = |m: f32| (m / tile_m.max(0.5)).round().clamp(1.0, 4096.0) as u32;
    (n(prog.terrain.size_x), n(prog.terrain.size_z))
}

/// One band of ground: the sheet it paints with, what lets it through, and what it takes to
/// wear away.
///
/// The `.hmf` names these files and [`write_source`] writes them, so both read this one
/// list. Two lists drift, and a layer naming a sheet nobody wrote is a track TerrainEd stops
/// on before it has drawn anything.
struct Layer {
    /// The sheet's base name under `maps/`. Everything beside it is named from this: the
    /// normal map, the shader, and the wet frame.
    name: &'static str,
    /// And what the same band is called inside a `.map`, which is not the same word.
    sheet: &'static str,
    look: GroundLook,
    salt: u32,
    /// Which cells the band covers, for the `.map` and for anything else compositing the
    /// ground. The exported `.tga` masks are built from the same functions this names.
    band: BandMask,
    /// How many metres of ground one tile of the sheet covers.
    tile_m: f32,
    /// `None` on layer zero: it is the ground everything else is painted over.
    mask: Option<&'static str>,
    /// And nothing under it to wear through to, so no thickness either.
    thickness: Option<f32>,
    /// How much of the sheet catches the light, in the normal map's alpha.
    ///
    /// Dirt is nearly matte. The example's own normal maps carry a specular that averages
    /// about 10 of 255 and peaks near 65, and wet ground does not raise it — the shine in
    /// the rain comes from the shader's reflection block, not from here.
    spec: u8,
    /// How tight the highlight is. The example's dry sheets say 12 and its wet ones 20 to 80.
    shininess: u32,
    /// Soil darkens and shines in the rain; turf does not, which is why the example gives a
    /// wet frame to its mud, dirt and sand and none to its grass.
    wet: bool,
    /// Whether the layer scatters 3D grass over itself.
    grass: bool,
}

/// The four bands, bottom first.
///
/// Field, graded shoulder, riding line, and the grass over the top. A track painted
/// line-and-field is a brown ribbon on a green sheet, and the shoulder — the worked ground
/// either side of the ribbon — is most of what is actually in front of a rider.
fn layers(prog: &TrackProgram) -> Vec<Layer> {
    // Ground follows what the track is made of, so a sand national exports sand.
    let Grounds { field, ridden, line, shoulder, rut, loose, turf } =
        ground_looks(prog.terrain.surface);
    let (_, shoulder_scale) = ground(prog.terrain.surface);
    vec![
        Layer {
            name: "ground",
            sheet: "ground_c",
            band: BandMask::Everywhere,
            look: field,
            salt: 0x9A0D,
            tile_m: TILE_FIELD_M,
            mask: None,
            thickness: None,
            spec: 18,
            shininess: 12,
            wet: true,
            grass: false,
        },
        Layer {
            name: "shoulder",
            sheet: "shoulder_c",
            band: BandMask::Out(SHOULDER_M * shoulder_scale),
            look: shoulder,
            salt: 0x30D2,
            tile_m: TILE_SHOULDER_M,
            mask: Some("mask_shoulder.tga"),
            thickness: Some(0.05),
            spec: 20,
            shininess: 12,
            wet: true,
            grass: false,
        },
        // The riding surface: every metre of the corridor, opaque. It used to be masked as a
        // strip about the racing line, which left the pale shoulder showing through the gaps
        // in its own patchiness — dark dirt with holes in it, and pale ground underneath for
        // no reason a rider could see.
        Layer {
            name: "dirt",
            sheet: "dirt_c",
            band: BandMask::Out(0.0),
            look: ridden,
            salt: 0x11E5,
            tile_m: TILE_LINE_M,
            mask: Some("mask_dirt.tga"),
            thickness: Some(0.1),
            spec: 22,
            shininess: 12,
            wet: true,
            grass: false,
        },
        // Painted over the corridor's base, in the order the ground gets that way: the loose
        // stuff is thrown over the worked soil, and the line is worn back through it.
        Layer {
            name: "loose",
            sheet: "loose_c",
            band: BandMask::Loose,
            look: loose,
            salt: 0x7C41,
            tile_m: TILE_LOOSE_M,
            mask: Some("mask_loose.tga"),
            thickness: Some(0.14),
            spec: 16,
            shininess: 10,
            wet: true,
            grass: false,
        },
        // The strip people actually ride, worn into the corridor and darker than it. Painted
        // over the loose, because a line is worn back through what was thrown onto it.
        Layer {
            name: "line",
            sheet: "dirt_line_c",
            band: BandMask::Line(LINE_HALF_WIDTH_M),
            look: line,
            salt: 0x2C7B,
            tile_m: TILE_LINE_M,
            mask: Some("mask_line.tga"),
            thickness: Some(0.09),
            spec: 24,
            shininess: 14,
            wet: true,
            grass: false,
        },
        Layer {
            name: "rut",
            sheet: "rut_c",
            band: BandMask::Rut,
            look: rut,
            salt: 0x5B93,
            tile_m: TILE_RUT_M,
            mask: Some("mask_rut.tga"),
            thickness: Some(0.08),
            spec: 30,
            shininess: 20,
            wet: true,
            grass: false,
        },
        Layer {
            name: "grass",
            sheet: "grass_c",
            band: BandMask::Beyond,
            look: turf,
            salt: 0x6A55,
            tile_m: TILE_GRASS_M,
            mask: Some("mask_grass.tga"),
            thickness: Some(0.01),
            spec: 14,
            shininess: 8,
            wet: false,
            grass: true,
        },
    ]
}

fn hmf(prog: &TrackProgram, syn: &Synth) -> String {
    let layers = layers(prog);
    let mut s = header(prog, syn);
    s.push_str(&format!("num_layers = {}\n", layers.len()));
    for (i, l) in layers.iter().enumerate() {
        s.push_str(&format!("layer{i}\n{{\n\tmap = maps/{}.tga\n", l.name));
        if l.wet {
            // The rainy-weather sheet. TerrainEd bakes both frames into the map and the game
            // picks between them with the weather — a layer without one keeps its dry
            // ground in the rain, which is what every track this generated used to do.
            s.push_str(&format!(
                "\tframe1\n\t{{\n\t\tmap = maps/{}_wet.tga\n\t}}\n",
                l.name
            ));
        }
        let (rx, rz) = repetitions(prog, l.tile_m);
        if rx == rz {
            s.push_str(&format!("\trepetitions = {rx}\n"));
        } else {
            s.push_str(&format!("\trepetitions_x = {rx}\n\trepetitions_z = {rz}\n"));
        }
        if let Some(mask) = l.mask {
            s.push_str(&format!("\tmask = {mask}\n"));
        }
        if let Some(t) = l.thickness {
            s.push_str(&format!("\tthickness = {t}\n"));
        }
        if l.grass {
            s.push_str(
                "\n\tgrass\n\t{\n\t\tmax_density = 20\n\t\theight = 0.2\n\
                 \t\theight_diff = 0.1\n\t\twidth = 0.25\n\t\twidth_diff = 0.1\n\
                 \t\ttexture = maps/grassfx.tga\n\t\tcolormap = grass_color.tga\n\
                 \t\tdensitymap = mask_grass.tga\n\t}\n",
            );
        }
        s.push_str("}\n\n");
    }
    s
}

/// How a surface rides, as distinct from how it looks.
///
/// [`ground_looks`] has always answered "what colour is this track" per surface, and
/// [`dig`] now answers "how deep can it be cut". Nothing answered "how does it wear", so a
/// sand national was a soil track with a sand palette: the same 0.38 m corner grooves two
/// metres apart, the same half-metre berms, the same 2.2 m braking washboard, all tuned on
/// worked loam.
///
/// A sand track is a different physical object. Its ruts are deeper and further apart because
/// the material moves rather than packs; its berms are enormous and soft; the sharp washboard
/// a hard surface builds under braking becomes long low swells, because sand cannot hold a
/// ridge that steep. Grass is the other way in every respect — root-bound ground barely cuts
/// up at all.
///
/// The face angles are deliberately not here. [`crate::trackprog::JUMP_FACE_DEG`] is read by
/// `Feature::length`, which is asked how long a jump is in places that have no track to ask
/// what it is made of, and the 30° figure is a ceiling measured across every published track
/// rather than a soil-specific one.
struct Ride {
    /// The deepest groove in the tightest corner, and the floor a straight wears.
    rut_depth: f32,
    rut_straight: f32,
    /// Metres between one groove of the field and the next.
    rut_spacing: f32,
    /// Half the width of a carved line.
    groove: f32,
    /// How tall a corner banks itself without being asked.
    berm: f32,
    /// The washboard under braking: metres between crests, and how tall it stands.
    brake: (f32, f32),
    /// And the longer, lower chop under power.
    accel: (f32, f32),
}

/// How much of a surface's wear is already cut into the terrain, from `terrain.wear`.
///
/// Anchored on the default rather than on either end, and that is the whole of it: every rut
/// figure in this module is measured off published `.trh` files, and a published `.trh` is a
/// track as its builder shipped it — worked in, not groomed flat and not the end of a long
/// day. So [`crate::trackprog::default_wear`] has to come out at exactly 1.0 or the
/// measurements stop meaning anything, and the dial moves either side of it.
///
/// The stack under the ground moves the opposite way in [`tht`]: ground already cut into the
/// heightmap is ground the surface no longer has to give.
fn worn(prog: &TrackProgram) -> f32 {
    let w = prog.terrain.wear.clamp(0.0, 1.0);
    (1.0 - crate::trackprog::default_wear() + w).max(0.05)
}

fn ride(s: Surface) -> Ride {
    match s {
        // The measured case. Every figure here is the one the corpus was read into and the
        // rest of this module's comments explain; the other two surfaces are stated against
        // it rather than measured separately, because nothing in the survey is a sand
        // national or a grasstrack.
        Surface::Soil => Ride {
            rut_depth: RUT_DEPTH_M,
            rut_straight: RUT_DEPTH_STRAIGHT_M,
            rut_spacing: RUT_SPACING_M,
            groove: RUT_GROOVE_M,
            berm: CORNER_BERM_M,
            brake: (BRAKING_WAVELENGTH_M, BRAKING_HEIGHT_M),
            accel: (ACCEL_WAVELENGTH_M, ACCEL_HEIGHT_M),
        },
        // Deeper, wider, softer, and smoother between the ruts. Sand does not hold a
        // two-metre washboard — under braking it builds long swells instead, and that is most
        // of why a sand national rides nothing like a hardpack one however it is painted.
        Surface::Sand => Ride {
            rut_depth: RUT_DEPTH_M * 1.55,
            rut_straight: RUT_DEPTH_STRAIGHT_M * 1.7,
            rut_spacing: RUT_SPACING_M * 1.35,
            groove: RUT_GROOVE_M * 1.4,
            berm: CORNER_BERM_M * 1.9,
            brake: (BRAKING_WAVELENGTH_M * 2.1, BRAKING_HEIGHT_M * 0.7),
            accel: (ACCEL_WAVELENGTH_M * 1.8, ACCEL_HEIGHT_M * 0.8),
        },
        // Root-bound: it takes a season to wear a line into a grasstrack and it never grows a
        // berm worth leaning on.
        Surface::Grass => Ride {
            rut_depth: RUT_DEPTH_M * 0.45,
            rut_straight: RUT_DEPTH_STRAIGHT_M * 0.35,
            rut_spacing: RUT_SPACING_M * 0.9,
            groove: RUT_GROOVE_M * 0.85,
            berm: CORNER_BERM_M * 0.4,
            brake: (BRAKING_WAVELENGTH_M * 0.9, BRAKING_HEIGHT_M * 0.55),
            accel: (ACCEL_WAVELENGTH_M * 0.9, ACCEL_HEIGHT_M * 0.5),
        },
    }
}

/// The deformable stack a surface is made of: what the wheels dig through, and how far.
///
/// This is where a track's *durability* lives, and it is the one part of the pipeline that
/// nothing measured. MX Bikes deforms terrain through these layers — each `thickness` is how
/// deep that material goes before the wheel reaches the one beneath, and the base layer at
/// the bottom has none, so it is where digging stops.
///
/// PiBoSo's own example track ships six layers with two of them unmasked, so every square
/// metre of the plot has 0.2 m of ground that can move. We shipped three, and the only
/// deformable one was masked to the riding line — off the line the surface was bare
/// `compact soil`, which is hardpack. That is why a generated track never grew a second line
/// however long it was ridden: there was nothing off the main one for a second line to be cut
/// into, and the main one bottomed out on rock after ten centimetres.
///
/// The material names are not a guess. They are the whole vocabulary out of `terrained.exe`'s
/// own string table — `compact soil`, `soil`, `soft soil`, `sand`, `gravel`, `rock`, `grass`
/// — and anything else fails to parse.
struct Dig {
    /// The floor. No thickness: nothing digs past it.
    base: &'static str,
    /// The bed, over the whole plot.
    bed: (&'static str, f32),
    /// What sits on the bed, also over the whole plot. Together with it, this is how deep
    /// ordinary ground can be cut.
    top: (&'static str, f32),
    /// The chewed-up stuff off the line and round the outside of a bend, which is deeper
    /// than the ground beside it because nothing packs it down.
    loose: (&'static str, f32),
    /// The packed racing line: a firm crust over softer ground, which is what a line worn
    /// into a track actually is.
    packed: (&'static str, f32),
}

fn dig(s: Surface) -> Dig {
    match s {
        // Worked loam: a hand's depth of workable ground over hardpack.
        Surface::Soil => Dig {
            base: "compact soil",
            bed: ("soil", 0.10),
            top: ("soft soil", 0.10),
            loose: ("soft soil", 0.16),
            packed: ("soil", 0.04),
        },
        // Sand is deep everywhere, and that is the whole character of a sand national — the
        // ruts are what you ride, not what you avoid.
        Surface::Sand => Dig {
            base: "compact soil",
            bed: ("soil", 0.10),
            top: ("sand", 0.22),
            loose: ("sand", 0.32),
            packed: ("sand", 0.08),
        },
        // A grasstrack barely cuts up at all: root-bound ground over firm soil.
        Surface::Grass => Dig {
            base: "compact soil",
            bed: ("soil", 0.06),
            top: ("soft soil", 0.05),
            loose: ("soft soil", 0.09),
            packed: ("soil", 0.03),
        },
    }
}

fn tht(prog: &TrackProgram, syn: &Synth) -> String {
    let mut s = header(prog, syn);
    // Off, pit, start — the order PiBoSo's example writes them in, and it matters: the pit
    // lane sits outside the shoulder, so the layer that says "pit" has to come after the one
    // that says "off the track".
    s.push_str("num_surface_layers = 3\n\n");
    s.push_str("surface_layer0\n{\n\tsurface = off\n\tmask = area_off.tga\n}\n\n");
    s.push_str("surface_layer1\n{\n\tsurface = pit\n\tmask = area_pits.tga\n}\n\n");
    s.push_str("surface_layer2\n{\n\tsurface = start\n\tmask = area_start.tga\n}\n\n");

    let d = dig(prog.terrain.surface);
    // Ground already cut into the terrain is ground the surface no longer has to give. A
    // freshly prepped track carries its whole depth; a fully raced one has spent half of it,
    // and the ruts baked into the heightmap are where it went. Without this the two add up:
    // we would hand the game a surface already dug half a metre and then tell it there is
    // another twenty centimetres underneath.
    let left = 1.0 - 0.5 * prog.terrain.wear.clamp(0.0, 1.0);
    let layer = |n: usize, (material, thickness): (&str, f32), mask: Option<&str>| {
        let mut b = format!("material_layer{n}\n{{\n\tmaterial = {material}\n");
        b.push_str(&format!("\tthickness = {:.3}\n", (thickness * left).max(0.005)));
        if let Some(m) = mask {
            b.push_str(&format!("\tmask = {m}\n"));
        }
        b.push_str("}\n\n");
        b
    };

    s.push_str("num_material_layers = 6\n\n");
    // The base carries no thickness, which is what makes it the floor.
    s.push_str(&format!("material_layer0\n{{\n\tmaterial = {}\n}}\n\n", d.base));
    // Two unmasked layers over the whole plot, as the example has. This is the change that
    // lets a line form anywhere rather than only where we painted one.
    s.push_str(&layer(1, d.bed, None));
    s.push_str(&layer(2, d.top, None));
    // Then the places that differ from ordinary ground.
    s.push_str(&layer(3, d.loose, Some("mask_loose.tga")));
    s.push_str(&layer(4, d.packed, Some("mask_rut.tga")));
    s.push_str(&layer(5, ("grass", 0.01), Some("mask_grass.tga")));
    s
}

/// A centreline in the form `tracked -merge` reads: a start pose, and a run of straights and
/// arcs with the length each one is to be written at.
fn tcl_of(x: f32, z: f32, angle: f32, segs: &[(&Segment, f32)]) -> String {
    let mut s = format!(
        "x = {x:.3}\nz = {z:.3}\nangle = {angle:.4}\nnumsegment = {}\n",
        segs.len()
    );
    for (i, (seg, length)) in segs.iter().enumerate() {
        let (radius, angle) = match **seg {
            Segment::Straight { .. } => (0.0, 0.0),
            Segment::Arc { radius, angle, .. } => (radius, angle.abs()),
        };
        let kind = if matches!(seg, Segment::Straight { .. }) {
            0
        } else {
            1
        };
        s.push_str(&format!(
            "segment{i}\n{{\n\ttype = {kind}\n\tlength = {length:.6}\n\tradius = {radius:.6}\n\
             \tangle = {angle:.6}\n\theight = {:.6}\n\theightlock = 0\n}}\n",
            seg.rise()
        ));
    }
    s
}

/// The racing line: the same straights and arcs the program was written in.
fn tcl(prog: &TrackProgram) -> String {
    let segs: Vec<(&Segment, f32)> = prog.segments.iter().map(|s| (s, s.length())).collect();
    tcl_of(prog.start.x, prog.start.z, prog.start.angle, &segs)
}

/// How much of the lap the start line covers: far enough that the field has funnelled in.
const START_LINE_M: f32 = 150.0;

/// The start line, which `tracked -merge` takes under `sa` as it takes the racing line under
/// `cl` — PiBoSo's own example merges both.
///
/// The gate row sits on the lap's opening straight, so the start line is the lap's own
/// leading segments with the first shortened to begin where the gates do.
fn start_tcl(prog: &TrackProgram) -> Option<String> {
    let line = prog.start_line()?;
    let segs: Vec<(&Segment, f32)> = line.segments.iter().map(|s| (s, s.length())).collect();
    Some(tcl_of(line.start.x, line.start.z, line.start.angle, &segs))
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

    /// Hunt for a step in a compiled `.trh`: where the ground jumps between one sample and
    /// the next, and what that place has in common with the lap.
    ///
    /// A rider feels a step long before a picture shows one, so this asks the file rather
    /// than the eye: every neighbouring pair of samples, the worst of them, and where they
    /// sit relative to the centreline the same file carries.
    ///
    /// ```text
    /// FROST_TRH=…/Corpus_National.trh cargo test --bin mxb-app -- --ignored --nocapture step_hunt
    /// ```
    #[test]
    #[ignore = "needs a compiled .trh — set FROST_TRH"]
    fn step_hunt() {
        let path = std::env::var("FROST_TRH").expect("set FROST_TRH");
        let bytes = std::fs::read(&path).expect("read the trh");
        let layout = crate::heightfield::probe(&bytes, None).expect("a terrain grid");
        let mps = layout.metres_per_sample.expect("a stated footprint");
        let (gw, gh, v) =
            crate::heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        let (gw, gh) = (gw as usize, gh as usize);
        println!("  {gw}x{gh} at {mps:.3} m, {} samples", v.len());

        // The centreline the file carries, so a step can be placed against the track.
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let lap = crate::trackline::read(bytes.get(block_at..).unwrap_or(&[]));
        let stations: Vec<(f32, f32)> = lap
            .as_ref()
            .map(|l| l.stations(1.0).into_iter().map(|st| (st.x, st.z)).collect())
            .unwrap_or_default();
        let off_line = |x: f32, z: f32| -> f32 {
            stations
                .iter()
                .map(|&(sx, sz)| ((x - sx).powi(2) + (z - sz).powi(2)).sqrt())
                .fold(f32::INFINITY, f32::min)
        };

        // Every neighbouring pair, as a slope in metres per metre.
        let mut worst: Vec<(f32, usize)> = Vec::new();
        let mut over = 0usize;
        for y in 0..gh {
            for x in 0..gw {
                let i = y * gw + x;
                let mut d: f32 = 0.0;
                if x + 1 < gw {
                    d = d.max((v[i + 1] - v[i]).abs());
                }
                if y + 1 < gh {
                    d = d.max((v[i + gw] - v[i]).abs());
                }
                if d > 0.25 {
                    over += 1;
                }
                worst.push((d, i));
            }
        }
        worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let steps: Vec<f32> = worst.iter().map(|w| w.0).collect();
        let at = |q: f32| steps[((steps.len() - 1) as f32 * q) as usize];
        println!(
            "  step between samples: p50 {:.3} m  p99 {:.3}  p99.9 {:.3}  max {:.3}",
            at(0.5),
            at(0.01),
            at(0.001),
            steps[0]
        );
        println!("  {over} pairs over 0.25 m ({:.4}% of the plot)", over as f32 / v.len() as f32 * 100.0);
        println!("  the twelve worst, and how far each is from the centreline:");
        for (d, i) in worst.iter().take(12) {
            let (x, z) = ((i % gw) as f32 * mps, (i / gw) as f32 * mps);
            let off = if stations.is_empty() { f32::NAN } else { off_line(x, z) };
            println!("    {d:.2} m at ({x:6.1}, {z:6.1})   {off:6.1} m off the line");
        }
        // The shape of the thing: where every step over half a metre is, on a coarse grid.
        // A step that is a line reads as a line here, and one that is a patch as a patch.
        const CELLS: usize = 40;
        let mut map = vec![0u32; CELLS * CELLS];
        for (d, i) in &worst {
            if *d < 0.5 {
                break;
            }
            let (cx, cy) = ((i % gw) * CELLS / gw, (i / gw) * CELLS / gh);
            map[cy * CELLS + cx] += 1;
        }
        println!("  steps over 0.5 m, {:.0} m a cell:", gw as f32 * mps / CELLS as f32);
        for y in 0..CELLS {
            let row: String = (0..CELLS)
                .map(|x| match map[y * CELLS + x] {
                    0 => '.',
                    1..=9 => '-',
                    10..=99 => '+',
                    _ => '#',
                })
                .collect();
            println!("    {row}");
        }
    }

    /// The same hunt on the ground we synthesise, before any compiler sees it.
    ///
    /// If a step is here it is ours; if it is only in the compiled `.trh` it is TerrainEd's.
    #[test]
    #[ignore = "slow — synthesises a lap"]
    fn step_hunt_ours() {
        let p: TrackProgram = match std::env::var("FROST_PROGRAM") {
            Ok(path) => serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap(),
            Err(_) => serde_json::from_str(DEMO).unwrap(),
        };
        let s = synthesise(&p).unwrap();
        let (gw, gh) = (s.gw, s.gh);
        let mps = s.mps;
        let v = &s.heights;
        let mut worst: Vec<(f32, usize)> = Vec::new();
        for y in 0..gh {
            for x in 0..gw {
                let i = y * gw + x;
                let mut d: f32 = 0.0;
                if x + 1 < gw {
                    d = d.max((v[i + 1] - v[i]).abs());
                }
                if y + 1 < gh {
                    d = d.max((v[i + gw] - v[i]).abs());
                }
                worst.push((d, i));
            }
        }
        worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let steps: Vec<f32> = worst.iter().map(|w| w.0).collect();
        let at = |q: f32| steps[((steps.len() - 1) as f32 * q) as usize];
        println!("  {gw}x{gh} at {mps:.3} m", );
        // The riding surface on its own. Everything outside it slumps; a berm is inside it
        // and is meant to be steep, so a step there has to be judged separately.
        let mut inside: Vec<f32> = worst
            .iter()
            .filter(|(_, i)| s.corridor[*i])
            .map(|(d, _)| *d)
            .collect();
        inside.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        if !inside.is_empty() {
            let q = |f: f32| inside[((inside.len() - 1) as f32 * f) as usize];
            println!(
                "  inside the corridor: p50 {:.3} m  p99 {:.3}  p99.9 {:.3}  max {:.3}  over {} cells",
                q(0.5), q(0.01), q(0.001), inside[0], inside.len()
            );
        }
        println!(
            "  step between samples: p50 {:.3} m  p99 {:.3}  p99.9 {:.3}  max {:.3}",
            at(0.5), at(0.01), at(0.001), steps[0]
        );
        // What each of the worst ones is near: the lap, the start spur, the corridor.
        println!("  the twelve worst:");
        for (d, i) in worst.iter().take(12) {
            let (x, z) = ((i % gw) as f32 * mps, (i / gw) as f32 * mps);
            let lap = s
                .stations
                .iter()
                .map(|st| ((x - st.x).powi(2) + (z - st.z).powi(2)).sqrt())
                .fold(f32::INFINITY, f32::min);
            let spur = s
                .spur
                .as_ref()
                .map(|sp| {
                    sp.stations
                        .iter()
                        .map(|st| ((x - st.x).powi(2) + (z - st.z).powi(2)).sqrt())
                        .fold(f32::INFINITY, f32::min)
                })
                .unwrap_or(f32::NAN);
            // What the two cells either side of the step disagree about. A jump's height is
            // a function of how far round the lap a cell is, and that is a quantity which
            // jumps where one station's territory ends and the next begins.
            let (x1, y1) = (i % gw, i / gw);
            let j = if x1 + 1 < gw && (v[i + 1] - v[*i]).abs() > (v[i + gw] - v[*i]).abs() {
                i + 1
            } else {
                i + gw
            };
            println!(
                "    {d:.2} m at ({x:6.1}, {z:6.1})  lap {lap:6.1} m  spur {spur:6.1} m  corridor {}  \
                 arc {:7.1} -> {:7.1} ({:+.1} m)  station {} -> {}  dist {:.2} -> {:.2}",
                s.corridor[*i],
                s.arc[*i], s.arc[j], s.arc[j] - s.arc[*i],
                s.station[*i], s.station[j],
                s.dist[*i], s.dist[j]
            );
            let _ = y1;
        }
    }

    /// What our ruts are shaped like, on the same statistic a published track is measured by.
    ///
    /// ```text
    /// cargo test --bin mxb-app -- --ignored --nocapture our_rut_shape
    /// ```
    /// Indiana, for comparison: across 0.115 m rms, along 0.068, anisotropy 1.70, floor
    /// 0.99 m, wall 35 deg, 2.9 grooves at 2.48 m.
    #[test]
    #[ignore = "slow — synthesises a lap"]
    fn our_rut_shape() {
        let p: TrackProgram = match std::env::var("FROST_PROGRAM") {
            Ok(path) => serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap(),
            Err(_) => serde_json::from_str(DEMO).unwrap(),
        };
        let s = synthesise(&p).unwrap();
        let g = crate::trackstats::Grid {
            w: s.gw,
            h: s.gh,
            size_x: p.terrain.size_x,
            size_z: p.terrain.size_z,
            v: s.heights.clone(),
        };
        let step = STATION_STEP;
        let stations: Vec<(f32, f32, f32)> =
            s.stations.iter().map(|st| (st.x, st.z, st.heading)).collect();
        let r = crate::trackstats::rut_shape(&stations, step, &g).expect("a rut shape");
        println!("  {} stations over {:.0} m", stations.len(), stations.len() as f32 * step);
        println!(
            "  across {:.3} m rms   along {:.3} m rms   anisotropy {:.2}",
            r.across_rms_m, r.along_rms_m, r.anisotropy
        );
        println!(
            "  floor {:.2} m   wall {:.0} deg   {:.1} grooves at {:.2} m   chatter {:.3} m",
            r.floor_m, r.wall_deg, r.grooves, r.spacing_m, r.chatter_m
        );
        println!(
            "  chatter on the line {:.3} m   2 m off {:.3}   4 m off {:.3}",
            r.chatter_zones_m[0], r.chatter_zones_m[1], r.chatter_zones_m[2]
        );
    }

    /// Pull a published track's ground sheets out of its `.map`, as PNGs.
    ///
    /// How `assets/ground/*.jpg` were made. Indiana Pro bakes its terrain sheets into its
    /// `.map` at 1024²; this lists every sheet it carries and writes the ones named in
    /// `FROST_SHEETS` out where they can be looked at and re-encoded.
    ///
    /// `FROST_ALL` drops the `_c` filter, and it is the one that matters here: a track's
    /// *scenery* sheets carry PiBoSo's suffixes, but the ones its terrain is painted with
    /// are named by whoever built it — Indiana's grass is `hm_grass` and the bottom of its
    /// ruts is `sand_bottom`, and neither shows up in a list of `_c` names.
    ///
    /// ```text
    /// FROST_ALL=1 FROST_MAP=…/2024_ARLMX_RD11_INDIANA_PRO.map FROST_DUMP=/tmp/sheets \
    ///   FROST_SHEETS=soil_dark_c,soil_light_c,sand_bottom,hm_grass \
    ///   cargo test --bin mxb-app -- --ignored --nocapture dump_ground_sheets
    /// ```
    #[test]
    #[ignore = "needs a real .map — set FROST_MAP"]
    fn dump_ground_sheets() {
        let path = std::env::var("FROST_MAP").expect("set FROST_MAP");
        let bytes = std::fs::read(&path).expect("read the map");
        let texs = crate::edf::embedded_textures(&bytes);
        let want: Vec<String> = std::env::var("FROST_SHEETS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let dump = std::env::var("FROST_DUMP").ok();
        if let Some(d) = &dump {
            std::fs::create_dir_all(d).unwrap();
        }
        for t in &texs {
            let all = std::env::var("FROST_ALL").is_ok();
            if !all && !(t.name.ends_with("_c") || t.name.ends_with("_c_a")) {
                continue;
            }
            let Some(px) = crate::edf::inflate_texture(&bytes, t) else {
                continue;
            };
            if px.len() != t.width as usize * t.height as usize * 4 {
                continue;
            }
            let mut sum = [0u64; 3];
            for p in px.chunks_exact(4) {
                for c in 0..3 {
                    sum[c] += p[c] as u64;
                }
            }
            let n = (t.width * t.height) as u64;
            println!(
                "{:<34} {}x{}  mean ({}, {}, {})",
                t.name,
                t.width,
                t.height,
                sum[0] / n,
                sum[1] / n,
                sum[2] / n
            );
            if let Some(d) = &dump {
                if want.iter().any(|w| w == &t.name) {
                    // Bottom-up in the file, like every PiBoSo sheet.
                    let mut px = px.clone();
                    for y in 0..(t.height as usize / 2) {
                        let (a, b) = (y, t.height as usize - 1 - y);
                        for i in 0..t.width as usize * 4 {
                            px.swap(a * t.width as usize * 4 + i, b * t.width as usize * 4 + i);
                        }
                    }
                    let img: image::RgbaImage =
                        image::ImageBuffer::from_raw(t.width, t.height, px).unwrap();
                    img.save(format!("{d}/{}.png", t.name)).unwrap();
                    println!("   -> {d}/{}.png", t.name);
                }
            }
        }
    }

    /// A lap that closes: two straights joined by two half-circle turns.
    pub(super) fn oval() -> TrackProgram {
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
                                    tilt: 0.0,
                    tilt_angle: 0.0,
                                    landforms: 0,
                    landform_height: 12.0,
                                },
                surface: crate::trackprog::Surface::Soil,
                wear: crate::trackprog::default_wear(),
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
        // The lap's own corridor, away from the start straight, which is a fan and is not
        // part of the lap.
        let cells = (0..s.gw * s.gh)
            .filter(|i| s.corridor[*i] && !s.on_the_start(*i, p.width * 0.5))
            .count();
        // Area over length is the width, give or take the ends of the lap.
        let width = cells as f32 * s.mps * s.mps / p.lap_length();
        assert!((width - p.width).abs() < 1.0, "measured {width:.2} m");
    }

    /// The gate row is 48 m across. Forty gates on a 12 m track is thirty-six metres of them
    /// standing in the field, which is what a start straight that isn't one looks like.
    #[test]
    fn the_start_fans_out_far_enough_to_hold_the_gate_row() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let spur = s.spur.as_ref().expect("a lap with a straight has a start");
        let span = GRID_STALLS as f32 * GRID_LANE_M;
        let gate = spur.gate_at();
        // The widest cell on the row, measured off the start line the way the game measures a
        // stall's `lat`.
        let reach = (0..s.gw * s.gh)
            .filter(|i| s.corridor[*i] && (s.spur_arc[*i] - gate).abs() < 2.0)
            .map(|i| s.spur_dist[i])
            .filter(|d| d.is_finite())
            .fold(0.0f32, f32::max);
        assert!(
            reach >= span * 0.5,
            "the row is {span:.0} m across and the start reaches {reach:.1} m off its line"
        );
        // And down to the track's own width by the time it meets the lap.
        let narrowed = (0..s.gw * s.gh)
            .filter(|i| {
                s.corridor[*i]
                    && s.on_the_start(*i, p.width * 0.5)
                    && (s.spur_arc[*i] - spur.length()).abs() < 3.0
            })
            .map(|i| s.spur_dist[i])
            .filter(|d| d.is_finite())
            .fold(0.0f32, f32::max);
        assert!(
            narrowed < p.width,
            "it is still {narrowed:.1} m wide where it meets the lap"
        );
    }

    /// And they have to be in a row. Every position in a `.rdf` is stated on the lap, and if
    /// each gate takes the distance-round of the *nearest station* rather than projecting onto
    /// it, forty gates half a metre apart in the file come out as a staircase in the game.
    #[test]
    fn the_gate_row_is_a_row() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let spur = s.spur.as_ref().expect("a start straight");
        let text = rdf(&p, Some(spur));

        let st = p.stations(0.5);
        let block = &text[text.find("starting_grid").expect("a grid")..];
        let mut it = block.lines().map(|l| l.trim());
        let mut placed = Vec::new();
        while let Some(l) = it.next() {
            if !l.starts_with("stall") {
                continue;
            }
            let (mut long, mut lat) = (0.0f32, 0.0f32);
            for _ in 0..5 {
                match it.next() {
                    Some(v) if v.starts_with("long = ") => long = v[7..].parse().unwrap(),
                    Some(v) if v.starts_with("lat = ") => lat = v[6..].parse().unwrap(),
                    Some("}") => break,
                    _ => {}
                }
            }
            let q = st
                .iter()
                .min_by(|a, b| (a.s - long).abs().total_cmp(&(b.s - long).abs()))
                .unwrap();
            let (fx, fz) = crate::trackprog::heading_vector(q.heading);
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let along = long - q.s;
            placed.push((q.x + fx * along + rx * lat, q.z + fz * along + rz * lat));
        }
        assert_eq!(placed.len(), GRID_STALLS);

        // Every gate on one straight line: fit the row's own direction from its ends and check
        // nothing wanders off it.
        let (first, last) = (placed[0], placed[placed.len() - 1]);
        let (dx, dz) = (last.0 - first.0, last.1 - first.1);
        let len = (dx * dx + dz * dz).sqrt();
        assert!(len > 40.0, "the row is only {len:.0} m across");
        let (ux, uz) = (dx / len, dz / len);
        for (i, (x, z)) in placed.iter().enumerate() {
            let off = (x - first.0) * -uz + (z - first.1) * ux;
            assert!(off.abs() < 0.1, "gate {i} stands {off:.2} m off the row");
        }
        // And evenly spaced along it.
        for w in placed.windows(2) {
            let step = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            assert!(
                (step - GRID_LANE_M).abs() < 0.15,
                "gates {step:.2} m apart where the lane is {GRID_LANE_M}"
            );
        }
    }

    /// The gates the game reads have to be the gates we built. Every position in a `.rdf` is
    /// stated on the lap, so a row standing out on the start straight is written as a long
    /// way round the lap and a big lateral offset — and if it is written as a distance along
    /// the start straight instead, the game puts forty riders across the middle of the track.
    #[test]
    fn the_grid_the_game_reads_lands_on_the_start_straight() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let spur = s.spur.as_ref().expect("a start straight");
        let text = rdf(&p, Some(spur));

        // Read the stalls back the way the game does: a distance round the lap and an offset
        // across it.
        let st = p.stations(0.5);
        let block = &text[text.find("starting_grid").expect("a grid")..];
        let mut it = block.lines().map(|l| l.trim());
        let mut placed = Vec::new();
        while let Some(l) = it.next() {
            if !l.starts_with("stall") {
                continue;
            }
            let (mut long, mut lat) = (0.0f32, 0.0f32);
            for _ in 0..5 {
                match it.next() {
                    Some(v) if v.starts_with("long = ") => long = v[7..].parse().unwrap(),
                    Some(v) if v.starts_with("lat = ") => lat = v[6..].parse().unwrap(),
                    Some("}") => break,
                    _ => {}
                }
            }
            let q = st
                .iter()
                .min_by(|a, b| (a.s - long).abs().total_cmp(&(b.s - long).abs()))
                .unwrap();
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            placed.push((q.x + rx * lat, q.z + rz * lat));
        }
        assert_eq!(placed.len(), GRID_STALLS, "every gate is written");

        // Each one on the start straight, and none of them on the lap.
        let line = p.start_line().expect("a start line");
        let walk = TrackProgram { start: line.start, segments: line.segments.clone(), ..p.clone() };
        let spur_st = walk.stations(1.0);
        for (i, (x, z)) in placed.iter().enumerate() {
            let to_spur = spur_st
                .iter()
                .map(|q| ((q.x - x).powi(2) + (q.z - z).powi(2)).sqrt())
                .fold(f32::MAX, f32::min);
            let to_lap = st
                .iter()
                .map(|q| ((q.x - x).powi(2) + (q.z - z).powi(2)).sqrt())
                .fold(f32::MAX, f32::min);
            assert!(
                to_spur < spur.at(spur.gate_at()) + 2.0,
                "gate {i} lands {to_spur:.0} m off the start straight"
            );
            assert!(to_lap > p.width, "gate {i} lands on the lap, {to_lap:.0} m from its line");
        }
    }

    /// A rider on a flying lap must never cross the gates: the start is a spur beside the
    /// circuit, not a stretch of it.
    #[test]
    fn the_lap_does_not_run_through_the_gate_row() {
        let p = oval();
        let line = p.start_line().expect("a start");
        let gate = line.start;
        let nearest = p
            .stations(1.0)
            .iter()
            .map(|q| ((q.x - gate.x).powi(2) + (q.z - gate.z).powi(2)).sqrt())
            .fold(f32::MAX, f32::min);
        assert!(
            nearest > crate::trackprog::START_OFFSET_M * 0.5,
            "the lap passes {nearest:.0} m from the gate row"
        );
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

    /// A lap has to come back to where it started, in height as well as in position. One
    /// step-up used to raise everything after it and nothing put it back, so the start line —
    /// where the two ends of the lap meet in the ground — was a cliff the height of the
    /// step-up. It measured 1.30 m in a single sample on the demo, and no test saw it.
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

    /// The fault this guards: the heightmap went to TerrainEd in `syn` row order while the
    /// masks went in theirs, so the ground came out mirrored north-south under its own paint
    /// and a track's jumps were nowhere near where it was painted.
    #[test]
    fn the_heightmap_lands_on_the_same_rows_as_the_masks() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let raw = heightmap_raw(&s, p.terrain.scale);
        assert_eq!(raw.len(), s.heights.len() * 2);
        // TerrainEd reads the raw from the bottom up, so reading it back that way has to
        // give the rows in `syn` order — which is the order the masks, the centreline and
        // the start line all count in.
        let step = p.terrain.scale / u16::MAX as f32;
        for gy in 0..s.gh {
            let from_bottom = s.gh - 1 - gy;
            for gx in (0..s.gw).step_by(7) {
                let v = u16::from_le_bytes([
                    raw[(from_bottom * s.gw + gx) * 2],
                    raw[(from_bottom * s.gw + gx) * 2 + 1],
                ]);
                let want = s.heights[gy * s.gw + gx];
                assert!(
                    (v as f32 * step - want).abs() <= step,
                    "row {gy} col {gx}: file says {:.3} m, the terrain is {want:.3} m",
                    v as f32 * step
                );
            }
        }
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
        let st = p.stations(STATION_STEP);
        let spur = StartSpur::of(&p, &st, &vec![0.0; st.len()], &Landscape::of(&p));
        let text = rdf(&p, spur.as_ref());
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
            format!("{slug}/{slug}.ini"),
            format!("{slug}/{slug}.rdf"),
            format!("{slug}/{slug}.amb"),
            format!("{slug}/{slug}.ssc"),
            format!("{slug}/gfx.cfg"),
        ] {
            assert!(names.contains(&want), "{want} is missing from {names:?}");
        }
        assert!(
            names.contains(&format!("{slug}/{slug}.map")),
            "the track has no graphics: {names:?}"
        );

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
    /// A texture record ends with the descriptor a published map puts there.
    ///
    /// Without it a reader walking the table runs the next record's *name* through the
    /// descriptor's fields. Ours declare no secondary maps, which is true — we generate
    /// colour sheets and nothing else.
    #[test]
    fn texture_records_carry_their_descriptor() {
        let px = vec![0u8; 64 * 64 * 4];
        let rec = texture_record("ground_c", 64, 64, &px);
        let tail = &rec[rec.len() - 24..];
        let w = |i: usize| u32::from_le_bytes(tail[i * 4..i * 4 + 4].try_into().unwrap());
        let f = |i: usize| f32::from_le_bytes(tail[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((w(0), w(1), w(2)), (1, 1, 0), "the three leading words");
        assert_eq!((f(3), f(4)), (1.0, 1.0), "the two scales");
        assert_eq!(w(5), 0, "how many secondary maps follow");
    }

    /// No draw group may reach past a 16-bit index.
    ///
    /// Every group in a published map is small — Indiana carries 1313 of them over a million
    /// vertices, the largest 8363 and the median 224, and not one is over 65535. Ours was
    /// four groups over the whole terrain, one of them 182512 vertices, which is nearly three
    /// times what a 16-bit index reaches. That overflows where the index buffer is built,
    /// Every vertex attribute has to be what a published map puts there.
    ///
    /// Three of these were wrong at some point and each was invisible: a zero-length tangent
    /// (a divide by zero in anything that shades it), a zero-length normal, and four floats
    /// written into the wrong vertex's slot. That last one read back correct against the
    /// reference because *every* value in that region is 1.0 on a real map, so any offset
    /// The `.map` has to walk exactly the way the game walks it.
    ///
    /// Not "does it look plausible" — the layout in `map()` came out of tracing the real
    /// loader (`scripts/map-loader-trace.py`), and the loader consumes a file written by it to
    /// the final byte and returns. This walks the same schema in-process, so a change that
    /// breaks the shape fails here rather than in front of a rider.
    #[test]
    fn the_map_walks_to_its_last_byte() {
        // The mesh at the front is empty, and the terrain follows it at a fixed 312 bytes.
        // PiBoSo's own OEM drag strip is shaped this way -- no materials, no vertices, no
        // triangles, and a hundred and twenty megabytes of trailing block -- and a published
        // map's mesh is scenery standing above the ground, never the ground itself.
        let p = oval();
        let s = synthesise(&p).unwrap();
        let m = map(&p, &s);
        let u = |o: usize| u32::from_le_bytes(m[o..o + 4].try_into().unwrap());

        assert_eq!(&m[..4], b"MP2\0");
        assert_eq!(u(4), 304, "version");
        assert_eq!((u(8), u(12), u(16)), (0, 0, 0), "materials, vertices, triangles");

        // Four node blocks, each one node with inverted bounds: +FLT_MAX min, -FLT_MAX max,
        // which is how an empty tree says it contains nothing.
        for at in [20, 88, 144, 196] {
            assert_eq!(u(at), 1, "one node at {at}");
            let f = |o: usize| f32::from_le_bytes(m[o..o + 4].try_into().unwrap());
            for w in 0..3 {
                assert_eq!(f(at + 4 + w * 4), f32::MAX, "node at {at} min {w}");
                assert_eq!(f(at + 16 + w * 4), -f32::MAX, "node at {at} max {w}");
            }
        }
        // And then the terrain, which is the thing that actually draws the ground.
        assert_eq!((u(312) as usize, u(316) as usize), (s.gw, s.gh), "the grid at 312");
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

        // The source files, and every shader they pull in beside a sheet — a `.shd` naming a
        // normal map nobody wrote stops TerrainEd exactly as a missing texture does.
        let mut files = vec!["track.hmf".to_string(), "track.tht".to_string()];
        for l in layers(&p) {
            files.push(format!("maps/{}.shd", l.name));
            if l.wet {
                files.push(format!("maps/{}_wet.shd", l.name));
            }
        }

        let mut named = Vec::new();
        for f in &files {
            let text = std::fs::read_to_string(dir.join(f)).unwrap();
            // Paths inside a shader are relative to the sheet's own folder, not the project.
            let base = std::path::Path::new(f).parent().unwrap().to_path_buf();
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let value = value.trim();
                match key.trim() {
                    // Every key whose value is a file name rather than a number.
                    "map" | "mask" | "data" | "texture" | "densitymap" | "colormap" => {
                        named.push((f.clone(), base.join(value).to_string_lossy().into_owned()));
                    }
                    // A cube map names six files at once, by suffix.
                    "envmap" => {
                        let (stem, ext) = value.rsplit_once('.').unwrap();
                        for side in ["right", "left", "top", "bottom", "front", "back"] {
                            named.push((
                                f.clone(),
                                base.join(format!("{stem}_{side}.{ext}"))
                                    .to_string_lossy()
                                    .into_owned(),
                            ));
                        }
                    }
                    _ => {}
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

    /// A layer without a shader is lit flat, which is what the exported tracks were.
    #[test]
    fn every_ground_sheet_is_shaded() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("mxb-shd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_source(&p, &s, &dir).unwrap();

        let hmf = std::fs::read_to_string(dir.join("track.hmf")).unwrap();
        let sheets: Vec<&str> = hmf
            .lines()
            .filter_map(|l| l.split_once("map = maps/"))
            .map(|(_, v)| v.trim().trim_end_matches(".tga"))
            .filter(|v| *v != "grassfx")
            .collect();
        assert!(sheets.len() >= 4, "found {sheets:?}");
        for sheet in sheets {
            let shd = std::fs::read_to_string(dir.join(format!("maps/{sheet}.shd")))
                .unwrap_or_else(|_| panic!("{sheet} has no shader beside it"));
            assert!(shd.contains("bump"), "{sheet} has no normal map");
            assert!(shd.contains("specular"), "{sheet} takes no highlight");
            // Relative to the sheet, not the project — `line_n.tga`, not `maps/line_n.tga`.
            assert!(
                shd.contains("map = ") && !shd.contains("map = maps/"),
                "the shader for {sheet} points out of its own folder:\n{shd}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Rain has to change the ground, and only the ground that rain changes.
    #[test]
    fn the_soil_layers_carry_a_wet_frame_and_the_turf_does_not() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("mxb-wet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_source(&p, &s, &dir).unwrap();

        let hmf = std::fs::read_to_string(dir.join("track.hmf")).unwrap();
        assert_eq!(
            hmf.matches("frame1").count(),
            6,
            "every soil band gets a wet sheet, the grass does not:\n{hmf}"
        );
        for l in layers(&p) {
            let wet = dir.join(format!("maps/{}_wet.tga", l.name));
            assert_eq!(wet.is_file(), l.wet, "{} wet sheet", l.name);
        }
        // And it is the same ground, darker — not a different sheet.
        let dry = std::fs::metadata(dir.join("maps/line.tga")).unwrap().len();
        let wet = std::fs::metadata(dir.join("maps/line_wet.tga"))
            .unwrap()
            .len();
        assert_eq!(dry, wet, "the wet sheet is the dry one shaded, same shape");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against the example track's own `dirt_n.tga`, which averages (128, 127, 241) with its
    /// red spread across [9, 246] and a specular in alpha averaging 12.
    #[test]
    fn a_normal_map_matches_the_shape_of_the_examples_own() {
        let field = ground_looks(Surface::Soil).field;
        let dim = 256;
        let px = ground_pixels(dim, &field, 7);
        let t = normal_tga(&px, dim, SHEET_NORMAL_STRENGTH, 18);
        let body = &t[18..18 + dim * dim * 4];
        // The container is BGRA, so blue is first and red third.
        let mean = |o: usize| {
            body.iter()
                .skip(o)
                .step_by(4)
                .map(|&v| v as f64)
                .sum::<f64>()
                / (dim * dim) as f64
        };
        let (b, g, r, a) = (mean(0), mean(1), mean(2), mean(3));
        assert!((120.0..136.0).contains(&r), "red is off centre at {r:.1}");
        assert!((120.0..136.0).contains(&g), "green is off centre at {g:.1}");
        assert!(
            (215.0..252.0).contains(&b),
            "blue at {b:.1} — the sheet is tilted far harder than the example's"
        );
        assert!((4.0..30.0).contains(&a), "specular at {a:.1}, not dirt's");
    }

    /// A rectangular terrain given one repetition count stretches its ground sideways.
    #[test]
    fn a_rectangular_terrain_tiles_each_axis_for_itself() {
        // Twice as wide as it is deep, so the grid stays square-celled — the synthesiser
        // insists on that and the ratio has to be a power of two for it.
        let mut p = oval();
        p.terrain.size_x = 800.0;
        let s = synthesise(&p).unwrap();
        let text = hmf(&p, &s);
        assert!(
            !text.contains("\trepetitions = "),
            "still one count:\n{text}"
        );
        assert!(text.contains("repetitions_x = "));
        assert!(text.contains("repetitions_z = "));

        // And a square one keeps saying it the short way, the way the example does.
        let sq = oval();
        let s = synthesise(&sq).unwrap();
        assert!(hmf(&sq, &s).contains("\trepetitions = "));
    }

    /// The ground has to agree with the race data about where the pit lane is.
    #[test]
    fn the_pit_strip_lands_where_the_race_data_puts_its_stalls() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let dir = std::env::temp_dir().join(format!("mxb-pits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_source(&p, &s, &dir).unwrap();

        let tga = std::fs::read(dir.join("area_pits.tga")).unwrap();
        let px = &tga[18..18 + MASK_DIM * MASK_DIM * 4];
        let pits = pit_lane(&p);
        let (fx, fz) = crate::trackprog::heading_vector(p.start.angle.to_radians());
        let (rx, rz) = crate::trackprog::right_vector(p.start.angle.to_radians());

        let (mut n, mut lat, mut along) = (0u32, 0.0f64, 0.0f64);
        for y in 0..MASK_DIM {
            for x in 0..MASK_DIM {
                if px[(y * MASK_DIM + x) * 4 + 3] < 128 {
                    continue;
                }
                // The mask is stretched over the terrain, so a texel is a fraction of it.
                let wx = x as f32 / MASK_DIM as f32 * p.terrain.size_x - p.start.x;
                let wz = y as f32 / MASK_DIM as f32 * p.terrain.size_z - p.start.z;
                lat += (wx * rx + wz * rz) as f64;
                along += (wx * fx + wz * fz) as f64;
                n += 1;
            }
        }
        assert!(n > 200, "the pit strip is {n} texels, which is nothing");
        let (lat, along) = (lat / n as f64, along / n as f64);
        assert!(
            (lat - pits.lat as f64).abs() < 2.0,
            "the strip sits at {lat:.1} m across, the stalls at {:.1}",
            pits.lat
        );
        let middle = (pits.from + pits.to) as f64 / 2.0;
        assert!(
            (along - middle).abs() < 8.0,
            "the strip sits at {along:.1} m round, the stalls at {middle:.1}"
        );
        assert!(
            std::fs::read_to_string(dir.join("track.tht"))
                .unwrap()
                .contains("surface = pit"),
            "nothing tells the game that strip is pit lane"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `tracked -merge` takes a start line beside the racing one, and it is its own line: the
    /// gate row off to the side of the lap, the sprint, and the turn that merges in.
    #[test]
    fn the_start_line_is_the_spur_and_not_the_lap() {
        let p = oval();
        let line = p.start_line().expect("a start");
        let text = start_tcl(&p).expect("and a `.tcl` for it");

        let value = |key: &str| -> f32 {
            text.lines()
                .find_map(|l| l.trim().strip_prefix(&format!("{key} = ")))
                .unwrap_or_else(|| panic!("no {key} in\n{text}"))
                .parse()
                .unwrap()
        };
        assert!((value("x") - line.start.x).abs() < 0.01);
        assert!((value("z") - line.start.z).abs() < 0.01);
        // It begins beside the lap, not on it.
        assert!(
            (value("x") - p.start.x).abs() + (value("z") - p.start.z).abs() > 20.0,
            "the start line begins on the lap"
        );
        // The sprint, then the turn in: more than one segment, fewer than the lap's.
        assert!(value("numsegment") >= 2.0);
        assert!(
            (value("numsegment") as usize) < p.segments.len(),
            "the start line is the whole lap again"
        );
        assert!(
            (value("length") - crate::trackprog::START_SPRINT_M).abs() < 0.01,
            "the sprint is {} m",
            value("length")
        );
    }

    /// Every band's material record has to be where the loader looks for it.
    ///
    /// It reads five words after a layer's sheet — the secondary-map flag, the two tiling
    /// floats, the mask flag and one closing word — and then takes the next fifty-six bytes
    /// as the following layer's material. Writing one word too many there is invisible to a
    /// walk of the file, because the drift is self-consistent and nothing ever reads past
    /// the end; it is fatal to the ground, because every band after the base sheet is then
    /// read as an empty record and never drawn.
    ///
    /// Measured by emulating the game's own loader over our output: with three words here
    /// it made 1,327,319 reads and loaded a single sheet; with one it makes 144 and loads
    /// all four bands, each with its normal map.
    #[test]
    fn every_ground_band_is_where_the_loader_looks_for_it() {
        let p = oval();
        let s = synthesise(&p).unwrap();
        let m = map(&p, &s);
        let u = |o: usize| u32::from_le_bytes(m[o..o + 4].try_into().unwrap()) as usize;
        let fl = |o: usize| f32::from_le_bytes(m[o..o + 4].try_into().unwrap());

        // The empty mesh prefix is a fixed 312 bytes, and the terrain follows it.
        let mut o = 312;
        // The terrain: the grid, its samples, the ground's size and the height budget.
        let (gw, gh) = (u(o), u(o + 4));
        o += 8 + gw * gh * 2 + 12 + 12;

        let count = u(o);
        o += 4;
        assert_eq!(count, layers(&p).len(), "one layer per painted band");
        for band in 0..count {
            // A material record, which is the thing that goes wrong: read a word out of
            // step and it is (1.4e-45, 0.0, ...) rather than (0.0, 1.0, 1.0, ...).
            assert_eq!(fl(o), 0.0, "band {band} material word 0");
            for w in 1..=6 {
                assert_eq!(fl(o + w * 4), 1.0, "band {band} material word {w}");
            }
            o += 56;
            assert_eq!(u(o), 1, "band {band} declares one sheet");
            o += 4;
            // The sheet: a hundred-byte name, the flag, the dimensions, the hash, and a
            // length that counts the eight zero bytes behind it.
            let dim = GROUND_TEXTURE_DIM;
            assert_eq!((u(o + 104), u(o + 108)), (dim as u32 as usize, dim), "band {band} sheet size");
            // The length counts the eight zero bytes behind it, and the pixels follow those.
            o += 136 + u(o + 132);
            // A secondary map, and the four words in front of it. Its header is a word
            // shorter than the colour sheet's: dimensions at name+100, not name+104.
            assert_eq!(u(o), 1, "band {band} declares its normal map");
            o += 4 + 16;
            assert_eq!((u(o + 100), u(o + 104)), (dim, dim), "band {band} normal map size");
            o += 132 + u(o + 128);
            let (rx, rz) = (fl(o), fl(o + 4));
            assert!(rx >= 1.0 && rz >= 1.0, "band {band} tiles {rx} x {rz}");
            o += 8;
            // Layer zero covers everything and carries no mask; the rest are masked.
            let masked = u(o);
            assert_eq!(masked == 1, band > 0, "band {band} mask");
            o += 4;
            if masked == 1 {
                o += 12 + u(o + 8);
            }
            o += 4;
        }
        // What is left is the trailing lists, which are zero and deliberately roomy.
        assert!(o <= m.len(), "the layers run past the end of the file");
        assert!(
            m[o..].iter().all(|&b| b == 0),
            "{} bytes after the last band are not the empty lists",
            m.len() - o
        );
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

        // The riding line only. The start pad is track — 54 m of it across the gate row — but
        // it is not riding line, and the corpus figures this is held to describe the ribbon a
        // rider goes round on.
        let lap_only: Vec<bool> = (0..s.gw * s.gh)
            .map(|i| s.corridor[i] && !s.on_the_start(i, p.width * 0.5))
            .collect();
        let c = crate::trackstats::measure("synth", &lap_only, &s.heights, s.gw, s.gh, s.mps);
        // And the whole track surface, pad and all, which is what a reader of the written
        // file sees — the `.trh` surfaces the start as track, because it is.
        let all = crate::trackstats::measure("synth", &s.corridor, &s.heights, s.gw, s.gh, s.mps);
        if let Some(spur) = &s.spur {
            let pad = (0..s.gw * s.gh).filter(|i| s.corridor[*i] && !lap_only[*i]).count();
            println!(
                "start: {:.0} m long, {:.0} m across the gate row, {:.0} m² of pad",
                spur.length(),
                spur.width_m(),
                pad as f32 * s.mps * s.mps,
            );
        }
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
             tightest R p50 {:.1}m  turning {:.0}°  climbs {:.1}m  grade p90 {:.1}°",
            r.lap_m, r.segments, r.arcs, r.straights, r.turns, r.turn_deg.p50,
            r.turn_radius_m.p50, r.total_turn_deg, r.lap_climb_m, r.grade_deg.p90,
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
        // Wider than the corpus's own 13.2–16.2 on purpose, and wider again since the ruts
        // were given floors: this detector counts a corner's chop as lips, so smoothing the
        // ground between the grooves took a crowd of phantom jumps out of it and the gaps
        // between the real ones roughly doubled — 20.3 m before, 28.3 m after, over one fewer
        // lip. The track's jumps did not move. Indiana's own centreline puts 34.5 m between
        // its lips, so the number this now reports is the plausible one and the number it used
        // to report was the chop talking. Still a sanity bound rather than an acceptance test,
        // for the same reason as before. See tasks/.
        between("lip spacing p50", c.lip_spacing_m.p50, 9.0, 36.0);
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
                (bc.width_from_mean_m - all.width_from_mean_m).abs() < 1.5,
                "the .pkz measures {:.1} m of track where the terrain it was written from is \
                 {:.1} — riding line and start pad together",
                bc.width_from_mean_m,
                all.width_from_mean_m
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

    /// No straight line across the terrain, whichever direction it runs.
    ///
    /// A straight line in ground is always a defect — real terrain has no reason to step along
    /// a row of samples — and it is the one class of fault none of the measurements we take
    /// would report. The 2.2 m wall a step-up used to leave across the start line passed the
    /// corridor width, the slope percentiles and the lip count without a murmur.
    ///
    /// A raw count of stepping samples does not separate the two: Indiana's worst row steps on
    /// 17% of its samples, more than any generated track, because a row that runs the length
    /// of a straight crosses a lot of jump faces. What separates them is *coherence* — a cliff
    /// steps the same way all along its length and a row of jump faces does not. Measured on
    /// Indiana the worst coherent line averages 0.28 m a step; the demo's is 0.25 m; the wall
    /// was 1.35 m.
    #[test]
    fn the_terrain_has_no_straight_lines_in_it() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        let s = synthesise(&p).unwrap();
        let step = 0.12f32; // below this it is texture, not a face

        let mut worst = (0.0f32, 0usize, String::new());
        let mut consider = |total: f32, n: usize, what: String| {
            if n >= 20 {
                let mean = total.abs() / n as f32;
                if mean > worst.0 {
                    worst = (mean, n, what);
                }
            }
        };
        for y in 1..s.gh {
            let (mut total, mut n) = (0.0f32, 0usize);
            for x in (0..s.gw).step_by(2) {
                let d = s.heights[y * s.gw + x] - s.heights[(y - 1) * s.gw + x];
                if d.abs() > step {
                    total += d;
                    n += 1;
                }
            }
            consider(total, n, format!("row {y}"));
        }
        for x in 1..s.gw {
            let (mut total, mut n) = (0.0f32, 0usize);
            for y in (0..s.gh).step_by(2) {
                let d = s.heights[y * s.gw + x] - s.heights[y * s.gw + x - 1];
                if d.abs() > step {
                    total += d;
                    n += 1;
                }
            }
            consider(total, n, format!("column {x}"));
        }
        // A wall is only half the news; the other half is which surface put it there — the
        // lap, the start straight, or the seam between two passes of the lap.
        if worst.0 >= 0.60 {
            if let Some(k) = worst.2.strip_prefix("row ") {
                let y: usize = k.parse().unwrap();
                let sp = s.spur.as_ref();
                let mut shown = 0;
                for x in (0..s.gw).step_by(2) {
                    let (i, j) = (y * s.gw + x, (y - 1) * s.gw + x);
                    if (s.heights[i] - s.heights[j]).abs() <= 0.5 || shown >= 4 {
                        continue;
                    }
                    shown += 1;
                    let f = |i: usize| {
                        format!(
                            "lap {:.1}@{:.0} | start {:.1}@{:.0} wide {:.1} deck {:.2} | h {:.2}",
                            s.dist[i], s.arc[i], s.spur_dist[i], s.spur_arc[i],
                            sp.map(|q| q.at(s.spur_arc[i])).unwrap_or(0.0),
                            sp.map(|q| q.deck_at(s.spur_arc[i])).unwrap_or(0.0),
                            s.heights[i],
                        )
                    };
                    println!("  ({:.0}, {:.0})\n    here  {}\n    above {}",
                        x as f32 * s.mps, y as f32 * s.mps, f(i), f(j));
                }
            }
        }
        assert!(
            worst.0 < 0.60,
            "{} steps the same way {} times, {:.2} m each — a published track's worst \
             coherent line averages 0.28 m",
            worst.2,
            worst.1,
            worst.0
        );
    }

    /// A track is worn and a field is not.
    ///
    /// Measured as the mean absolute second difference along the direction of travel, which
    /// is the scale a rider feels. Indiana reads 2.51 cm two metres off the riding line and
    /// 0.67 cm twelve metres out in the field — the field is a quarter of the track. Ours read
    /// 1.26 against 1.07, which is to say the field was very nearly as chopped up as the
    /// racing line, because the landscape carried 7.5 cm of detail everywhere and the ridden
    /// texture was too coarse to register at half a metre.
    #[test]
    fn the_field_is_smoother_than_the_track() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        let s = synthesise(&p).unwrap();
        let rough = |off: f32| -> f32 {
            let mut v: Vec<f32> = Vec::new();
            for st in s.stations.iter().step_by(37) {
                let (rx, rz) = crate::trackprog::right_vector(st.heading);
                let (hx, hz) = crate::trackprog::heading_vector(st.heading);
                let (px, pz) = (st.x + rx * off, st.z + rz * off);
                // Bilinear, not `sample`'s nearest cell: half-metre steps on a quarter-metre
                // grid snap to alternating cells, and that aliasing is larger than the
                // texture being measured.
                let at = |d: f32| {
                    let (fx, fy) = ((px + hx * d) / s.mps, (pz + hz * d) / s.mps);
                    let x0 = (fx.floor() as isize).clamp(0, s.gw as isize - 2) as usize;
                    let y0 = (fy.floor() as isize).clamp(0, s.gh as isize - 2) as usize;
                    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                    let g = |r: usize, c: usize| s.heights[r * s.gw + c];
                    (g(y0, x0) * (1.0 - tx) + g(y0, x0 + 1) * tx) * (1.0 - ty)
                        + (g(y0 + 1, x0) * (1.0 - tx) + g(y0 + 1, x0 + 1) * tx) * ty
                };
                let h: Vec<f32> = (-2..=2).map(|k| at(k as f32 * 0.5)).collect();
                v.push((h[0] - 2.0 * h[1] + h[2]).abs() + (h[2] - 2.0 * h[3] + h[4]).abs());
            }
            v.sort_by(f32::total_cmp);
            v[v.len() / 2]
        };
        let (track, field) = (rough(2.0), rough(12.0));
        assert!(
            field < track * 0.60,
            "the track reads {:.2} cm and the field {:.2} cm — a published track's field is a \
             quarter of its racing line",
            track * 100.0,
            field * 100.0
        );
    }

    /// A lap is routed to its ground, not dropped on it.
    ///
    /// The shape is fixed, so the only freedoms are where it sits and which way round it
    /// faces — and on a plot with hills on it those two decide whether the track climbs the
    /// ground or lies across it. Indiana measures 3.0 m of fall across thirty metres of track;
    /// ours managed 5.2 to 7.6 depending only on how big the hills happened to be.
    #[test]
    fn a_lap_is_turned_to_lie_along_the_ground() {
        let mut p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        p.terrain.relief.landforms = 8;
        p.terrain.relief.landform_height = 18.0;
        let placed = place_on_ground(&p).expect("a lap that fits has a placement");
        assert!(
            placed.cross_m <= placed.was_cross_m,
            "routing made it worse: {:.2} m against {:.2}",
            placed.cross_m,
            placed.was_cross_m
        );
    }

    #[test]
    #[ignore = "prints numbers"]
    fn print_the_start() {
        for (name, json) in [("demo", DEMO), ("example", crate::trackprog::EXAMPLE)] {
            let mut p: TrackProgram = serde_json::from_str(json).unwrap();
            let _ = crate::trackllm::repair_for_tests(&mut p);
            let s = synthesise(&p).unwrap();
            match (&s.spur, p.start_line()) {
                (Some(spur), Some(line)) => {
                for (i, sg) in line.segments.iter().enumerate() {
                    match sg {
                        crate::trackprog::Segment::Straight { length, .. } =>
                            println!("    {i}: straight {length:.0} m"),
                        crate::trackprog::Segment::Arc { radius, angle, .. } =>
                            println!("    {i}: arc r{radius:.0} through {angle:.0}° = {:.0} m", sg.length()),
                    }
                } println!(
                    "{name}: start straight {:.0} m off the lap, {:.0} m long in {} segments, \
                     {:.0} m wide at the gates against a {:.0} m track; joins the lap at \
                     {:.0} m of {:.0}",
                    crate::trackprog::START_OFFSET_M,
                    spur.length(),
                    line.segments.len(),
                    spur.width_m(),
                    p.width,
                    line.joins_at,
                    p.lap_length(),
                ) },
                _ => println!("{name}: no start straight"),
            }
        }
    }

    /// The lap that gets placed has to be the lap that was scored. Positions were turned one
    /// way and headings the other, so applying a placement built the mirror of what the search
    /// had measured — a lap routed to lie along the hill came out lying across it, and on a
    /// plot with no room to spare it came out over the edge.
    #[test]
    fn the_lap_that_gets_placed_is_the_lap_that_was_scored() {
        let mut p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        p.terrain.relief.landforms = 8;
        p.terrain.relief.landform_height = 18.0;
        let placed = place_on_ground(&p).expect("a lap that fits has a placement");
        place(&mut p, &placed);
        // Scored again where it now lies: the untouched figure this time is the one the
        // search promised last time.
        let after = place_on_ground(&p).expect("and it can be scored where it landed");
        assert!(
            (after.was_cross_m - placed.cross_m).abs() < 0.15,
            "placed a lap measuring {:.2} m where {:.2} m was scored",
            after.was_cross_m,
            placed.cross_m
        );
        p.check().expect("and it is still on its ground");
    }

    #[test]
    fn a_step_up_does_not_leave_a_cliff_at_the_start() {
        let mut p = hairpins();
        p.terrain.relief.amplitude = 0.0;
        p.features = vec![Feature::StepUp { at: 60.0, length: 20.0, height: 2.2 }];
        let s = synthesise(&p).unwrap();
        let lap = p.lap_length();
        let (before, after) = (height_at_arc(&s, lap - 6.0), height_at_arc(&s, 6.0));
        assert!(
            (before - after).abs() < 0.35,
            "the ground is {:.2} m at the finish and {:.2} m at the start",
            before,
            after
        );
    }

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

    /// A corner does not wear one groove down the middle. Everybody takes roughly the same
    /// line and nobody takes exactly it, so what a tight turn ends up with is a comb — which
    /// is what a published track's collision terrain shows and what a single rut does not.
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
        //
        // Sampled 85 m out rather than 100. This lap is two 120 m straights and two hairpins,
        // so 100 m past one corner's exit is 20 m before the next one's entry — inside
        // `RUT_CARRY_ENTRY_M`, where the ruts are on their way back up again. Measured along
        // the straight it falls 2.71, 1.49, 1.15, 1.20, 0.97, 0.90 and then *rises* to 1.64,
        // and that last figure is the carry working rather than failing.
        let near = groove_depth(&across(&s, arc_end + 10.0), 0.02);
        let far = groove_depth(&across(&s, arc_end + 85.0), 0.02);
        assert!(
            far < near * 0.6,
            "the ruts never faded: {near:.2} m of groove at 10 m past the corner, \
             {far:.2} m at 85 m"
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
        let field = ground_looks(Surface::Soil).field;
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

    /// Every band is painted with a published sheet, and the sheet decodes.
    ///
    /// [`band_pixels`] falls back to the generator when an asset will not load, which is the
    /// right thing to do at runtime and a silent regression in a build: the track still comes
    /// out, painted with noise. So the assets are checked here, where it is not silent.
    #[test]
    fn every_band_is_painted_with_a_published_sheet() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        for l in layers(&p) {
            let name = l.look.photo.unwrap_or_else(|| panic!("{} names no sheet", l.name));
            let (dim, px) = photo(name)
                .unwrap_or_else(|| panic!("{}'s sheet {name} did not decode", l.name));
            assert_eq!(*dim, GROUND_TEXTURE_DIM, "{name} is {dim} and the bands go out at 1024");
            assert_eq!(px.len(), dim * dim * 4, "{name} is not whole");
        }
    }

    /// And what goes out is the published pixels, not a version of them.
    ///
    /// A soil track is what these sheets were shot on, so its tone is 1 and the `.tga` it
    /// exports is Indiana's own sheet level for level. Anything that quietly re-tints them —
    /// a shading pass, a palette — moves these means.
    #[test]
    fn a_soil_track_ships_the_published_sheets_untouched() {
        let g = ground_looks(Surface::Soil);
        let mean = |look: &GroundLook| -> [f32; 3] {
            let px = band_pixels(GROUND_TEXTURE_DIM, look, 3);
            let mut sum = [0.0f64; 3];
            for p in px.chunks_exact(4) {
                for c in 0..3 {
                    sum[c] += p[c] as f64;
                }
            }
            let n = (px.len() / 4) as f64;
            std::array::from_fn(|c| (sum[c] / n) as f32)
        };
        // Indiana's own, measured off its `.map` by `dump_ground_sheets`.
        for (what, look, want) in [
            ("the field", &g.field, [171.0, 134.0, 99.0]),
            ("the riding line", &g.line, [49.0, 35.0, 23.0]),
            ("the grass", &g.turf, [93.0, 97.0, 50.0]),
        ] {
            let got = mean(look);
            for c in 0..3 {
                assert!(
                    (got[c] - want[c]).abs() < 3.0,
                    "{what} came out {:?} against the published {want:?}",
                    got.map(|v| v.round())
                );
            }
        }
    }

    /// The soil is calibrated against a published track's own sheets rather than picked.
    ///
    /// Indiana ships `soil_light_c` at a mean of (172, 134, 99) and `soil_dark_c` at
    /// (50, 36, 24), and those two numbers are what the base colours here were solved for.
    /// A change to the shading that quietly moves the result is a change to how every
    /// generated track looks, so it is worth a test rather than a comment.
    #[test]
    fn the_soil_lands_where_the_published_sheets_do() {
        let g = ground_looks(Surface::Soil);
        let (field, ridden) = (g.field, g.ridden);
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
        // The field is Indiana's, level for level. The riding line is deliberately lighter
        // than its (50, 36, 24): that figure is what a sheet averages on its own, and in the
        // game — under the track's own sky, with its own shadows on it — a line that dark is
        // one a rider cannot find. What has to survive is the *gap*, because the gap is what
        // makes a racing line visible from the seat.
        for c in 0..3 {
            assert!(
                (mean(&field)[c] - [172.0, 134.0, 99.0][c]).abs() < 14.0,
                "the field came out {:?}, and Indiana's sheet is [172, 134, 99]",
                mean(&field).map(|v| v.round())
            );
        }
        let (f, r) = (mean(&field), mean(&ridden));
        let gap = (f[0] - r[0] + f[1] - r[1] + f[2] - r[2]) / 3.0;
        assert!(
            (60.0..130.0).contains(&gap),
            "the line stands {gap:.0} levels off the field: {:?} against {:?}",
            r.map(|v| v.round()),
            f.map(|v| v.round())
        );
    }


    /// And they are not blotchy.
    ///
    /// A ground sheet is laid over a hundred times across a track, so anything it carries at
    /// the scale of the tile repeats at the scale of the tile — a metre-wide patch of lighter
    /// soil becomes a chequerboard printed on the ground, and that is what "the ground reads
    /// coarse" turns out to mean when you go and measure it. Indiana's sheets carry none: its
    /// dark soil spreads 21 grey levels about a mean of 39 and its light soil 28 about 142,
    /// which is grain and nothing else.
    #[test]
    fn the_ground_sheets_are_grain_and_not_blotches() {
        let g = ground_looks(Surface::Soil);
        for (what, look, most) in [
            ("the field", &g.field, 34.0),
            // The line's bound is above the field's own 26 because the sheet is lighter than
            // Indiana's: the same relative grain lands in more grey levels on a brighter
            // base, and the grain is what stops a track reading as painted plastic.
            ("the riding line", &g.ridden, 30.0),
            ("the shoulder", &g.shoulder, 34.0),
            ("the loose dirt", &g.loose, 34.0),
        ] {
            let dim = 256;
            let px = ground_pixels(dim, look, 11);
            let l: Vec<f32> = px
                .chunks_exact(4)
                .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
                .collect();
            let mean = l.iter().sum::<f32>() / l.len() as f32;
            let sd = (l.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / l.len() as f32)
                .sqrt();
            // The grain is allowed its own spread; what is not allowed is the *blur* of it
            // carrying one, because that is the part that survives being seen from a distance
            // and turns into a grid. Measured on a copy blurred over an eighth of the tile.
            let blur = box_blur_wrap(&l, dim, dim / 16);
            let bmean = blur.iter().sum::<f32>() / blur.len() as f32;
            let bsd = (blur.iter().map(|v| (v - bmean) * (v - bmean)).sum::<f32>()
                / blur.len() as f32)
                .sqrt();
            println!("  {what:<16} mean {mean:>5.1}  grain sd {sd:>5.1}  blotch sd {bsd:>4.1}");
            assert!(
                sd <= most,
                "{what} spreads {sd:.1} grey levels, and Indiana's widest sheet spreads 28"
            );
            assert!(
                bsd <= 5.0,
                "{what} still spreads {bsd:.1} levels once the grain is blurred out — that is \
                 a patch the size of the tile, and it repeats with the tile"
            );
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



    /// What a corner looks like, and whether the rut in it can be seen.
    ///
    /// Two questions, and the second is the one the shape alone never answered. A groove is
    /// only a rut to a rider if the eye can find it at speed, and that is a contrast between
    /// the floor and the bank beside it — the paint and the light on it together. So this
    /// renders the ground through the same bands the `.map` carries and then counts what it
    /// rendered: how much darker the floor of a groove reads than the wall outboard of it,
    /// across the tightest corners on the lap.
    ///
    /// ```text
    /// FROST_BUILD=/tmp/out cargo test -- --ignored --nocapture the_ruts_can_be_seen
    /// ```
    #[test]
    #[ignore = "renders ground — slow, and writes pictures with FROST_BUILD"]
    fn the_ruts_can_be_seen() {
        let p: TrackProgram = serde_json::from_str(DEMO).unwrap();
        let s = synthesise(&p).unwrap();

        // The tightest corners on the lap, spread round it rather than four samples of one.
        let mut picks: Vec<usize> = Vec::new();
        let mut order: Vec<usize> = (0..s.stations.len())
            .filter(|i| s.stations[*i].curvature.abs() > 1.0 / 20.0)
            .collect();
        order.sort_by(|a, b| {
            s.stations[*b]
                .curvature
                .abs()
                .partial_cmp(&s.stations[*a].curvature.abs())
                .unwrap()
        });
        for i in order {
            if picks.iter().all(|k| (s.stations[*k].s - s.stations[i].s).abs() > 80.0) {
                picks.push(i);
            }
            if picks.len() == 3 {
                break;
            }
        }
        assert!(!picks.is_empty(), "the demo has no corner tight enough to rut");
        let look = Painted::of(&p, &s);

        // Across the line at each of them: the ground's own rut signal against the luma the
        // paint and the light give it. A rut that reads is one where the two agree.
        let mut floors: Vec<f32> = Vec::new();
        let mut walls: Vec<f32> = Vec::new();
        for &k in &picks {
            let st = s.stations[k];
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            // One pixel every 4 cm across twelve metres of track, at the station itself.
            let steps = 300;
            let mut row: Vec<(f32, f32, f32)> = Vec::new();
            for i in 0..=steps {
                let t = (i as f32 / steps as f32 - 0.5) * 12.0;
                let (x, z) = (st.x + rx * t, st.z + rz * t);
                let (gx, gy) = (
                    (x / s.mps).round().clamp(0.0, (s.gw - 1) as f32) as usize,
                    (z / s.mps).round().clamp(0.0, (s.gh - 1) as f32) as usize,
                );
                let i = gy * s.gw + gx;
                row.push((t, s.rut[i], look.luma(&s, x, z)));
            }
            for (_, r, l) in &row {
                if *r < -0.35 {
                    floors.push(*l);
                } else if *r > 0.35 {
                    walls.push(*l);
                }
            }
            let mean = |v: &[(f32, f32, f32)], f: fn(f32) -> bool| -> (f32, usize) {
                let hit: Vec<f32> = v.iter().filter(|(_, r, _)| f(*r)).map(|(_, _, l)| *l).collect();
                (
                    if hit.is_empty() { 0.0 } else { hit.iter().sum::<f32>() / hit.len() as f32 },
                    hit.len(),
                )
            };
            let (fl, nf) = mean(&row, |r| r < -0.35);
            let (wl, nw) = mean(&row, |r| r > 0.35);
            println!(
                "  corner at {:>5.0} m, R {:>4.1} m: floor {:.0} over {nf} samples, \
                 wall {:.0} over {nw}, difference {:+.0}",
                st.s,
                1.0 / st.curvature.abs(),
                fl,
                wl,
                wl - fl
            );
        }

        let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let (fl, wl) = (avg(&floors), avg(&walls));
        println!(
            "\n  floor {:.1} against wall {:.1}: {:+.1} levels of 255, {:.0}% of the floor",
            fl,
            wl,
            wl - fl,
            (wl - fl).abs() / fl.max(1.0) * 100.0
        );
        if let Ok(dir) = std::env::var("FROST_BUILD") {
            let dir = Path::new(&dir);
            for (n, &k) in picks.iter().enumerate() {
                let st = s.stations[k];
                look.crop(&s, &dir.join(format!("corner{n}.ppm")), (st.x, st.z), 36.0, 720);
                preview_crop(
                    &s,
                    &dir.join(format!("corner{n}_shape.ppm")),
                    ((st.x - 18.0) / s.mps).max(0.0) as usize,
                    ((st.z - 18.0) / s.mps).max(0.0) as usize,
                    (36.0 / s.mps) as usize,
                    (36.0 / s.mps) as usize,
                );
            }
            println!("  wrote {} corner pictures to {}", picks.len() * 2, dir.display());

            // And the steepest takeoff on the lap, which is where the tyre marks are.
            let up = (0..s.stations.len())
                .max_by(|a, b| s.face[*a].partial_cmp(&s.face[*b]).unwrap())
                .unwrap();
            let st = s.stations[up];
            look.crop(&s, &dir.join("jumpface.ppm"), (st.x, st.z), 30.0, 700);
            println!(
                "  and the face at {:.0} m, climbing {:.2} m a metre",
                st.s, s.face[up]
            );

            // Where the marks actually sit, against where the approach says they should. The
            // picture cannot tell a fan that leans from one that only got wider, and leaning
            // is the whole claim.
            let mark = rut_mask(&s, p.width * 0.5, p.terrain.relief.seed, s.gw - 1, s.gh - 1);
            let centroid = |at: usize| -> (f32, f32) {
                let station = &s.stations[at];
                let (rx, rz) = crate::trackprog::right_vector(station.heading);
                let (mut sum, mut wsum) = (0.0f32, 0.0f32);
                let mut t = -6.0f32;
                while t <= 6.0 {
                    let (x, z) = (station.x + rx * t, station.z + rz * t);
                    let (gx, gy) = (
                        (x / s.mps).round().clamp(0.0, (s.gw - 2) as f32) as usize,
                        (z / s.mps).round().clamp(0.0, (s.gh - 2) as f32) as usize,
                    );
                    let a = mark[gy * (s.gw - 1) + gx] as f32 / 255.0;
                    sum += (t - s.line_lat[at]) * a;
                    wsum += a;
                    t += 0.05;
                }
                (if wsum > 0.0 { sum / wsum } else { 0.0 }, wsum * 0.05)
            };
            let n = s.stations.len();
            let back = (RUT_MARK_LOOKBACK_M / STATION_STEP) as usize;
            let lead = s.line_lat[(up + n - back % n) % n] - s.line_lat[up];
            let flat = (0..n)
                .min_by(|a, b| s.face[*a].abs().partial_cmp(&s.face[*b].abs()).unwrap())
                .unwrap();
            let (fc, fw) = centroid(up);
            let (gc, gw_) = centroid(flat);
            println!(
                "  marks on the face sit {fc:+.2} m off the line and are {fw:.1} m wide; on                  flat ground {gc:+.2} m and {gw_:.1} m. The approach comes from {lead:+.2} m."
            );
        }

        // Ten levels is about where a step stops being a gradient and starts being an edge on
        // a screen; a rut you cannot pick out at riding speed is decoration. Asserted last, so
        // a failure still leaves the pictures that explain it.
        assert!(
            (wl - fl).abs() >= 10.0,
            "the wall reads {:.0} and the floor {:.0} — a rut nobody can see",
            wl,
            fl
        );
    }

    /// The painted ground, composited the way the `.map` says it and lit like terrain.
    ///
    /// The slope preview answers whether the *shape* is right. It cannot answer whether a
    /// rider can see a rut, which is a different question with a different answer: a groove
    /// painted the colour of the ground it is cut into is invisible until you are in it, and
    /// no amount of depth fixes that. So every band's sheet is tiled over the ground at its
    /// own pitch, laid through its own mask in the order the game blends them, and shaded by
    /// the terrain's own normal under the light the track ships in `params.ini`.
    struct Painted {
        /// Sheet pixels, metres per tile, and the band's mask over the whole terrain.
        bands: Vec<(Vec<u8>, f32, Option<Vec<u8>>)>,
        mw: usize,
        mh: usize,
    }

    impl Painted {
        /// Small sheets on purpose: 512 at a 3 m tile is 6 mm a pixel, finer than any crop
        /// this is drawn into, and the shipped 1024 costs a minute a band to scatter.
        const SHEET: usize = 512;

        fn of(prog: &TrackProgram, syn: &Synth) -> Self {
            let seed = prog.terrain.relief.seed;
            let half = prog.width * 0.5;
            let (mw, mh) = (syn.gw - 1, syn.gh - 1);
            let bands = layers(prog)
                .into_iter()
                .map(|l| {
                    let sheet = band_pixels(Self::SHEET, &l.look, seed ^ l.salt);
                    let mask = match l.band {
                        BandMask::Everywhere => None,
                        band => Some(band_mask(syn, band, half, seed, mw, mh)),
                    };
                    (sheet, l.tile_m, mask)
                })
                .collect();
            Painted { bands, mw, mh }
        }

        /// The colour of one point of ground, lit.
        fn at(&self, syn: &Synth, x: f32, z: f32) -> [u8; 3] {
            let (gu, gv) = (x / syn.mps, z / syn.mps);
            let bi = |v: &[u8], w: usize, h: usize, u: f32, t: f32| -> f32 {
                let (u, t) = (u.max(0.0), t.max(0.0));
                let (x0, y0) = (u.floor() as usize, t.floor() as usize);
                let (fx, fy) = (u - x0 as f32, t - y0 as f32);
                let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                let (x0, y0) = (x0.min(w - 1), y0.min(h - 1));
                let a =
                    v[y0 * w + x0] as f32 + (v[y0 * w + x1] as f32 - v[y0 * w + x0] as f32) * fx;
                let b =
                    v[y1 * w + x0] as f32 + (v[y1 * w + x1] as f32 - v[y1 * w + x0] as f32) * fx;
                a + (b - a) * fy
            };
            let mut col = [0.0f32; 3];
            for (k, (sheet, tile, mask)) in self.bands.iter().enumerate() {
                let d = Self::SHEET as f32;
                let sx = (x / tile * d).rem_euclid(d) as usize % Self::SHEET;
                let sy = (z / tile * d).rem_euclid(d) as usize % Self::SHEET;
                let i = (sy * Self::SHEET + sx) * 4;
                let c = [sheet[i] as f32, sheet[i + 1] as f32, sheet[i + 2] as f32];
                let a = match mask {
                    Some(m) if k > 0 => {
                        bi(
                            m,
                            self.mw,
                            self.mh,
                            gu * self.mw as f32 / syn.gw as f32,
                            gv * self.mh as f32 / syn.gh as f32,
                        ) / 255.0
                    }
                    _ => 1.0,
                };
                for ch in 0..3 {
                    col[ch] += (c[ch] - col[ch]) * a;
                }
            }
            // The terrain's own relief, which is the other half of what a rider reads off a
            // rut — and the half the paint has to agree with rather than fight. Lit by the sun
            // the track ships: `params.ini` states it.
            let h =
                |ox: f32, oz: f32| sample_smooth(&syn.heights, syn.gw, syn.gh, gu + ox, gv + oz);
            let dx = (h(1.0, 0.0) - h(-1.0, 0.0)) / (2.0 * syn.mps);
            let dz = (h(0.0, 1.0) - h(0.0, -1.0)) / (2.0 * syn.mps);
            let (nx, ny, nz) = (-dx, 1.0, -dz);
            let inv = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
            let (lx, ly, lz) = (2.0f32, 10.0f32, -7.0f32);
            let ln = (lx * lx + ly * ly + lz * lz).sqrt();
            let lam = ((nx * lx + ny * ly + nz * lz) * inv / ln).max(0.0);
            let shade = 0.42 + 0.58 * lam;
            [
                (col[0] * shade).clamp(0.0, 255.0) as u8,
                (col[1] * shade).clamp(0.0, 255.0) as u8,
                (col[2] * shade).clamp(0.0, 255.0) as u8,
            ]
        }

        fn luma(&self, syn: &Synth, x: f32, z: f32) -> f32 {
            let c = self.at(syn, x, z);
            0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32
        }

        /// A square of ground, written as a PPM.
        ///
        /// Supersampled, because the sheets are far finer than the crop: a 3 m tile over 512
        /// pixels is 6 mm a pixel and a 36 m crop is 50 mm, so point-sampling picks one grain
        /// in eight and draws the tiling as a checkerboard that is not there. The game has
        /// mipmaps for this; sixteen samples a pixel is the cheap stand-in.
        fn crop(&self, syn: &Synth, out: &Path, centre: (f32, f32), span_m: f32, px: usize) {
            const SUB: usize = 4;
            let mut ppm = format!("P6\n{px} {px}\n255\n").into_bytes();
            for py in 0..px {
                for pxi in 0..px {
                    let mut acc = [0u32; 3];
                    for sy in 0..SUB {
                        for sx in 0..SUB {
                            let n = (px * SUB) as f32;
                            let u = (pxi * SUB + sx) as f32 + 0.5;
                            let v = (py * SUB + sy) as f32 + 0.5;
                            let x = centre.0 - span_m * 0.5 + span_m * u / n;
                            let z = centre.1 - span_m * 0.5 + span_m * v / n;
                            let c = self.at(syn, x, z);
                            for ch in 0..3 {
                                acc[ch] += c[ch] as u32;
                            }
                        }
                    }
                    let n = (SUB * SUB) as u32;
                    ppm.extend_from_slice(&[
                        (acc[0] / n) as u8,
                        (acc[1] / n) as u8,
                        (acc[2] / n) as u8,
                    ]);
                }
            }
            let _ = std::fs::write(out, ppm);
        }
    }

    /// The picture the game lists a track by, as rows from the top.
    fn shot_rows(p: &TrackProgram, dim: usize) -> Vec<[u8; 3]> {
        let s = synthesise(p).unwrap();
        let tga = ui_shot(p, &s, dim);
        let px = &tga[18..18 + dim * dim * 4];
        // A TGA's row zero is the bottom of the picture, and it is stored BGRA.
        let mut out = Vec::with_capacity(dim * dim);
        for y in (0..dim).rev() {
            for x in 0..dim {
                let at = (y * dim + x) * 4;
                out.push([px[at + 2], px[at + 1], px[at]]);
            }
        }
        out
    }

    /// The distance is at the top of the picture, which is the only place a camera looking
    /// down at the ground can put it.
    ///
    /// Air is the tell: the far ground is hazed towards the sky's own colour and the near
    /// ground is not, so the top of the picture has to be the paler half. This is the whole
    /// chain — the camera, the render and the flip into a bottom-up TGA — and every one of
    /// them has turned it over at some point.
    #[test]
    fn the_track_picture_is_the_right_way_up() {
        let dim = 160;
        let rows = shot_rows(&oval(), dim);
        let haze = [179.0f32, 179.0, 217.0];
        let off = |from: usize, to: usize| -> f32 {
            let mut d = 0.0;
            for y in from..to {
                for x in 0..dim {
                    let c = rows[y * dim + x];
                    d += (0..3).map(|k| (c[k] as f32 - haze[k]).abs()).sum::<f32>();
                }
            }
            d / ((to - from) * dim) as f32
        };
        // The oval measures about three quarters. Upside down it would measure about four
        // thirds, so anything under one separates the two — this leaves room for a track that
        // hazes less without letting a flipped one through.
        let (top, bottom) = (off(0, dim / 8), off(dim - dim / 8, dim));
        assert!(
            top < bottom * 0.85,
            "the top of the picture should be the hazy distance: {top:.0} against {bottom:.0} \
             at the bottom"
        );
    }

    /// And the lap is in it, across most of it.
    ///
    /// The camera places itself to fit the corridor rather than the terrain, so a track built
    /// on one corner of a big landscape is still the subject. If the fit gives up, the lap
    /// ends up a smudge in the middle of a field — which is what a picture framed on the
    /// terrain looks like, and it is not obviously wrong until you measure it.
    #[test]
    fn the_lap_fills_the_track_picture() {
        let dim = 160;
        let rows = shot_rows(&oval(), dim);
        // The ridden line is far darker than the ground it is cut into — Indiana's own soil
        // measures a mean of 39 against its field's 142 — so the darkest of the picture is
        // the track and nothing else.
        let luma = |c: [u8; 3]| 0.3 * c[0] as f32 + 0.6 * c[1] as f32 + 0.1 * c[2] as f32;
        let mut sorted: Vec<f32> = rows.iter().map(|&c| luma(c)).collect();
        sorted.sort_by(f32::total_cmp);
        let dark = sorted[rows.len() / 25];
        let (mut x0, mut x1, mut y0, mut y1) = (dim, 0usize, dim, 0usize);
        for (i, &c) in rows.iter().enumerate() {
            if luma(c) <= dark {
                let (x, y) = (i % dim, i / dim);
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        let (w, h) = (x1 + 1 - x0, y1 + 1 - y0);
        assert!(
            w * 10 >= dim * 7,
            "the lap spans {w} of {dim} across the picture"
        );
        assert!(h * 10 >= dim * 2, "the lap spans {h} of {dim} down the picture");
    }

    /// Look at the pictures the game lists a track by.
    ///
    /// The shot is a render, and a render is judged by looking at it — so this writes both of
    /// them out as `.ppm` beside each other, for a couple of laps.
    ///
    /// ```text
    /// FROST_SHOT=/tmp/shots cargo test -- --ignored --nocapture the_track_pictures
    /// ```
    #[test]
    #[ignore = "writes pictures to look at — set FROST_SHOT"]
    fn the_track_pictures() {
        let dir = std::env::var("FROST_SHOT").expect("set FROST_SHOT");
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        // A flat stadium, a tight one, and one cut into real ground — the last is the only
        // one that says whether the picture shows relief at all.
        let rolling = {
            let mut p = oval();
            p.name = "Test Rolling".into();
            p.terrain.relief.landforms = 8;
            p.terrain.relief.landform_height = 14.0;
            p.terrain.scale = 50.0;
            p
        };
        for p in [oval(), hairpins(), rolling] {
            let s = synthesise(&p).unwrap();
            let (map, shot) = ui_images(&p, &s, UI_IMAGE_DIM);
            let name = slug(&p.name);
            tga_to_ppm(&map, UI_IMAGE_DIM, &dir.join(format!("{name}_map.ppm")));
            tga_to_ppm(&shot, UI_IMAGE_DIM, &dir.join(format!("{name}.ppm")));
            println!("wrote {name}.ppm and {name}_map.ppm to {}", dir.display());
        }
    }

    /// Our own 32-bit BGRA TGA, bottom-up, back into something a viewer opens.
    fn tga_to_ppm(tga: &[u8], dim: usize, out: &Path) {
        let px = &tga[18..18 + dim * dim * 4];
        let mut ppm = format!("P6\n{dim} {dim}\n255\n").into_bytes();
        for y in (0..dim).rev() {
            for x in 0..dim {
                let at = (y * dim + x) * 4;
                ppm.extend_from_slice(&[px[at + 2], px[at + 1], px[at]]);
            }
        }
        std::fs::write(out, ppm).unwrap();
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

    /// What the surface stack comes out as, for eyeballing as much as for asserting.
    fn stack(surface: crate::trackprog::Surface, wear: f32) -> String {
        let mut p = oval();
        p.terrain.surface = surface;
        p.terrain.wear = wear;
        let syn = synthesise(&p).expect("synthesise");
        tht(&p, &syn)
    }

    /// Every thickness the stack declares, added up: how deep the ground can be cut.
    fn dig_depth(s: &str) -> f32 {
        s.lines()
            .filter_map(|l| l.trim().strip_prefix("thickness = "))
            .filter_map(|v| v.parse::<f32>().ok())
            .sum()
    }

    #[test]
    fn the_whole_plot_has_ground_that_can_move() {
        // The durability fix, and the thing three layers could not do. Two unmasked
        // deformable layers, as PiBoSo's own example ships — so a line can be cut anywhere
        // rather than only where we painted one.
        let s = stack(crate::trackprog::Surface::Soil, 0.55);
        assert!(s.contains("num_material_layers = 6"), "{s}");
        let unmasked = s
            .split("material_layer")
            .filter(|b| b.contains("thickness") && !b.contains("mask ="))
            .count();
        assert_eq!(unmasked, 2, "two layers over the whole plot:\n{s}");
        // And the floor still carries none, which is what makes it the floor.
        let base = s.split("material_layer1").next().unwrap();
        assert!(
            base.contains("material = compact soil") && !base.contains("thickness"),
            "{base}"
        );
    }

    #[test]
    fn every_material_named_is_one_the_compiler_knows() {
        // The whole vocabulary out of `terrained.exe`'s own string table. Anything else is a
        // track that will not compile, and the failure would surface far from here.
        const KNOWN: [&str; 7] =
            ["compact soil", "soil", "soft soil", "sand", "gravel", "rock", "grass"];
        for surface in [
            crate::trackprog::Surface::Soil,
            crate::trackprog::Surface::Sand,
            crate::trackprog::Surface::Grass,
        ] {
            for line in stack(surface, 0.55).lines() {
                if let Some(m) = line.trim().strip_prefix("material = ") {
                    assert!(KNOWN.contains(&m), "{surface:?} names an unknown material {m:?}");
                }
            }
        }
    }

    #[test]
    fn a_sand_track_is_sand_where_the_tyres_are() {
        // It was not. The riding line was written as `soil` whatever the track was made of,
        // and the sand only appeared as the base layer underneath — masked out at exactly
        // the place anyone rides. A sand national rode on soil.
        let sand = stack(crate::trackprog::Surface::Sand, 0.55);
        let soil = stack(crate::trackprog::Surface::Soil, 0.55);
        assert!(sand.contains("material = sand"), "{sand}");
        assert!(!soil.contains("material = sand"), "{soil}");
        // And it is deeper than worked loam, which is most of what a sand track rides like.
        assert!(
            dig_depth(&sand) > dig_depth(&soil),
            "{:.2} m against {:.2}",
            dig_depth(&sand),
            dig_depth(&soil)
        );
    }

    #[test]
    fn a_raced_track_has_already_spent_half_its_depth() {
        // The knob. Ground cut into the heightmap is ground the surface no longer has to
        // give, so the two cannot both be at full depth — that is what would have made a
        // heavily raced track dig itself to pieces once the stack got deeper.
        let (fresh, raced) = (
            dig_depth(&stack(crate::trackprog::Surface::Soil, 0.0)),
            dig_depth(&stack(crate::trackprog::Surface::Soil, 1.0)),
        );
        assert!(fresh > raced, "fresh {fresh:.3} m against raced {raced:.3}");
        assert!(
            (raced / fresh - 0.5).abs() < 0.02,
            "a fully raced track keeps half of it: {:.3}",
            raced / fresh
        );
    }




    /// A lap with one tabletop on its opening straight, for asking what built ground does to
    /// the ruts over it.
    fn with_a_tabletop() -> TrackProgram {
        let mut p = hairpins();
        p.features = vec![Feature::Tabletop { at: 40.0, length: 36.0, height: 2.4 }];
        p
    }



    /// A cross-section with everything longer than a few metres taken out of it: the bench,
    /// the berm and the camber go, and what is left is the grooves.
    ///
    /// A straight-line detrend is not enough, and getting that wrong is how this was nearly
    /// mismeasured. A berm is a `(a/half)^1.4` rise with a back on it, so what a linear fit
    /// leaves behind is a curve — and the lowest point of that curve is wherever the berm is
    /// not, which reads as a rut three metres off the line that is not there.
    fn groove_residual(v: &[f32]) -> Vec<f32> {
        // Over the riding surface only, and cut down to it *before* the fit rather than
        // after. The windrow and the berm's back stand at the corridor's edge and rise half a
        // metre in the last two of the section's ten; no low-order fit over the whole width
        // can hold that, so what it leaves behind is a trough just inboard of the berm that
        // no wheel ever made. Measured at 160 m on the test lap it read -0.20 against the
        // real line's -0.16 and won.
        let v = &v[ridden_window(v.len())];
        // A quadratic, fitted across what is left and taken off it.
        //
        // Two simpler things were tried and both mismeasure. A straight line leaves the
        // berm's own curvature behind — a berm is a `(a/half)^1.4` rise with a back on it —
        // and the lowest point of what is left is wherever the berm is not, which reads as a
        // rut three metres off the line that is not there. A running mean over a few metres
        // has the opposite fault: the window is short enough to follow the ground the groove
        // itself sits in, so a deep line in generally low ground comes out shallower than a
        // shallow one on a rise. At 160 m on the test lap that put the "deepest groove" at
        // -3.6 m when the lowest ground in the section was at +2.5, under the paint.
        //
        // A parabola can hold a bench, a camber and a berm. It cannot hold a rut, which is
        // why what is left over is one.
        let n = v.len() as f32;
        let x = |i: usize| i as f32 / (v.len() - 1) as f32 - 0.5;
        let s0 = n;
        let (mut s1, mut s2, mut s3, mut s4) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let (mut t0, mut t1, mut t2) = (0.0f32, 0.0f32, 0.0f32);
        for (i, h) in v.iter().enumerate() {
            let (a, h) = (x(i), *h);
            s1 += a;
            s2 += a * a;
            s3 += a * a * a;
            s4 += a * a * a * a;
            t0 += h;
            t1 += a * h;
            t2 += a * a * h;
        }
        // Normal equations for h = c + b*x + a*x², solved by elimination.
        let d = s0 * (s2 * s4 - s3 * s3) - s1 * (s1 * s4 - s3 * s2) + s2 * (s1 * s3 - s2 * s2);
        if d.abs() < 1e-9 {
            return v.to_vec();
        }
        let c = (t0 * (s2 * s4 - s3 * s3) - s1 * (t1 * s4 - s3 * t2) + s2 * (t1 * s3 - s2 * t2))
            / d;
        let b = (s0 * (t1 * s4 - s3 * t2) - t0 * (s1 * s4 - s3 * s2) + s2 * (s1 * t2 - t1 * s2))
            / d;
        let a = (s0 * (s2 * t2 - t1 * s3) - s1 * (s1 * t2 - t1 * s2) + t0 * (s1 * s3 - s2 * s2))
            / d;
        (0..v.len())
            .map(|i| {
                let u = x(i);
                v[i] - (c + b * u + a * u * u)
            })
            .collect()
    }

    /// How far across a section its samples reach, either side of the centreline.
    const SECTION_M: f32 = 5.4;

    /// The slice of a section that is riding surface rather than the edge of the track.
    ///
    /// The windrow of spoil along the corridor's edge leaves a dip just inside it, and any
    /// detrend reads that dip as a groove — it turned up at 4.8 m in every section measured,
    /// on a jump's lip as readily as on open ground. It is not a rut and nobody rides it.
    fn ridden_window(n: usize) -> std::ops::Range<usize> {
        let keep = (RIDDEN_M / SECTION_M).clamp(0.0, 1.0);
        let edge = ((1.0 - keep) * 0.5 * n as f32).round() as usize;
        edge..(n - edge)
    }

    /// Where a sample of the *ridden* window sits, in metres right of the centreline.
    fn lateral_of(i: usize, n: usize) -> f32 {
        let w = ridden_window(n);
        (i as f32 / (w.len() - 1) as f32 - 0.5) * 2.0 * RIDDEN_M
    }

    /// Where the deepest groove in a section sits, in metres right of the centreline.
    ///
    /// Over the riding surface only, for the same reason [`lines_across`] is: the dip inside
    /// the windrow at the corridor's edge is not a rut and nobody rides it.
    fn deepest_at(v: &[f32]) -> f32 {
        let g = groove_residual(v);
        let i = (0..g.len()).min_by(|&a, &b| g[a].total_cmp(&g[b])).unwrap();
        lateral_of(i, v.len())
    }

    /// How far out a section is still riding surface. See [`ridden_window`].
    const RIDDEN_M: f32 = 3.8;

    /// The distinct lines in a section: troughs at least `least_m` below the ground either
    /// side of them, and at least a bike apart.
    fn lines_across(v: &[f32], least_m: f32) -> Vec<f32> {
        let g = groove_residual(v);
        let mut out: Vec<f32> = Vec::new();
        for i in 1..g.len() - 1 {
            if g[i] > g[i - 1] || g[i] > g[i + 1] || -g[i] < least_m {
                continue;
            }
            let lat = lateral_of(i, v.len());
            if out.last().is_none_or(|p| (lat - p).abs() > 1.8) {
                out.push(lat);
            }
        }
        out
    }

    /// How much ground the grooves take out of the riding surface of a section.
    fn ridden_groove_depth(v: &[f32]) -> f32 {
        let g = groove_residual(v);
        g.iter().map(|d| (-d).max(0.0)).sum::<f32>() / g.len() as f32
    }

    #[test]
    fn the_deepest_groove_lies_under_the_painted_line() {
        // The two halves of "where is the line" used to be computed eight hundred lines
        // apart and never read each other: the paint leaned up to `half -
        // LINE_KEEPS_OFF_EDGE_M` into the corner while the cut sat on the centreline, four
        // metres apart on a twelve-metre track. A rider steered by one and dropped into the
        // other.
        let s = synthesise(&hairpins()).unwrap();
        for at in [130.0f32, 140.0, 150.0, 160.0, 170.0, 180.0, 190.0] {
            let k = s.stations.iter().position(|st| st.s >= at).unwrap();
            let off = deepest_at(&across(&s, at)) - s.line_lat[k];
            assert!(
                off.abs() <= RUT_HALF_WIDTH_M,
                "at {at} m the deepest groove is {off:+.2} m off the line, and the painted \
                 strip is only {RUT_HALF_WIDTH_M:.2} m wide"
            );
        }
    }

    #[test]
    fn a_corner_grows_more_than_one_line() {
        // A corner after three motos carries two or three ways through, and the choice
        // between them is most of what makes a turn worth riding twice. One noise field with
        // one centre could only ever produce a single bundle of parallel grooves.
        let s = synthesise(&hairpins()).unwrap();
        let most = (130..200)
            .step_by(5)
            .map(|at| lines_across(&across(&s, at as f32), 0.02).len())
            .max()
            .unwrap_or(0);
        assert!(most >= 2, "the corner never grew a second line — {most} at best");
    }

    #[test]
    fn a_jump_face_carries_a_line_rather_than_a_bundle() {
        // The rut field knew only about curvature, so a jump — which sits on a straight —
        // took the straight's groove floor across its whole width: half a dozen parallel
        // gouges up a takeoff ramp and no line among them. Everyone hits a face in the same
        // place, packs that hard, and leaves the ground beside it alone.
        let p = with_a_tabletop();
        let s = synthesise(&p).unwrap();
        let (up, top, _) = crate::trackprog::tabletop_faces(2.4, 36.0);
        let face = lines_across(&across(&s, 40.0 + up * 0.6), 0.02);
        let plain = lines_across(&across(&s, 100.0), 0.02);
        // A comb, not a single groove and not the open ground's spread. Everybody arrives at a
        // face off the same corner and scrubs up it from wherever that left them, so what a
        // ramp wears is a fan of scuffs gathered about the line — which is the thing a rider
        // reads the approach off, and the reason it is worth cutting rather than only
        // painting. This asked for exactly one line until the marks were made deep enough to
        // see, which is the opposite of what a ridden face looks like.
        assert!(face.len() >= 2, "the face carries no marks at all: {face:.1?}");
        // Wider than the grooves on open ground, and that is right rather than a fault: a
        // bundle down a straight is where a few lines have worn in, while a face is marked
        // across everywhere anybody arrived. What it must be is *gathered about the line* —
        // the marks say which way the approach delivers you, and marks centred somewhere else
        // say nothing.
        let k = s.stations.iter().position(|st| st.s >= 40.0 + up * 0.6).unwrap_or(0);
        let mid = face.iter().sum::<f32>() / face.len() as f32;
        assert!(
            (mid - s.line_lat[k]).abs() < 1.8,
            "the face's marks sit {:.1} m off the line they should be gathered on: {face:.1?}",
            mid - s.line_lat[k]
        );
        let _ = &plain;
        // And the lip itself is swept: a takeoff edge is maintained, and a rutted lip is one
        // nobody can see until they are on it.
        let lip = ridden_groove_depth(&across(&s, 40.0 + up + top * 0.5));
        let on_face = ridden_groove_depth(&across(&s, 40.0 + up * 0.6));
        assert!(
            lip < on_face * 0.6,
            "the lip is as cut up as the face below it: {lip:.4} against {on_face:.4}"
        );
    }

    #[test]
    fn ordinary_ground_still_wears_the_way_it_was_measured() {
        // The line emphasis must not have come out of the spread. Published straights wear
        // 0.09–0.16 m of groove and their corners three times that; a lap that is glass
        // between the lines reads as one from the first corner exit.
        let s = synthesise(&hairpins()).unwrap();
        for at in [40.0f32, 60.0, 80.0, 100.0] {
            let v = across(&s, at);
            // One to four. The survey's per-track figure averages 1.0-2.3 lines, but that is a
            // mean over the whole lap: Indiana's own cross-sections carry three and four, and
            // this station sits inside `RUT_CARRY_ENTRY_M` of a hairpin, where the lines are
            // fanning into the turn. What must not happen is none — the emphasis on the racing
            // line flattening everything either side of it.
            let n = lines_across(&v, 0.02).len();
            assert!(
                (1..=4).contains(&n),
                "at {at} m the straight wears {n} lines; published ground carries one to four"
            );
            assert!(
                ridden_groove_depth(&v) > 0.002,
                "at {at} m the straight came out glass"
            );
        }
    }

    #[test]
    fn a_sand_track_rides_like_sand_and_not_only_looks_like_it() {
        // Surface reached three things: the colour palettes, the shoulder's id and width, and
        // the base material name. Every constant that decides how a track *wears* was a
        // module-level const tuned on worked loam, so a sand national was a soil track with a
        // sand palette.
        // Differenced, because a peak-to-peak measurement cannot answer this. A section's
        // relief is mostly things a surface does not change — the machine's passes, the
        // windrow, the ground's own grain — so soil against sand came out 1.10x when the rut
        // constants differ by 1.7. Two synths of the same lap with the same seed differ *only*
        // by the surface, so the difference between them is exactly what the surface did.
        let ground = |surface: crate::trackprog::Surface| {
            let mut p = hairpins();
            p.terrain.surface = surface;
            synthesise(&p).expect("synthesise")
        };
        let (soil, sand, grass) = (
            ground(crate::trackprog::Surface::Soil),
            ground(crate::trackprog::Surface::Sand),
            ground(crate::trackprog::Surface::Grass),
        );
        // How far a surface moves the ground away from soil's, over ground anyone rides.
        let moved = |other: &Synth| {
            let mut worst = 0.0f32;
            for at in (40..=200).step_by(10) {
                let (a, b) = (across(&soil, at as f32), across(other, at as f32));
                let w = ridden_window(a.len());
                for i in w {
                    worst = worst.max((a[i] - b[i]).abs());
                }
            }
            worst
        };
        let (to_sand, to_grass) = (moved(&sand), moved(&grass));
        assert!(
            to_sand > 0.10,
            "sand rides the same as soil: it moves the ground {to_sand:.3} m"
        );
        assert!(
            to_grass > 0.05,
            "a grasstrack rides the same as soil: it moves the ground {to_grass:.3} m"
        );
        // And which way each goes, which is the whole claim: sand cuts deeper than soil and
        // a grasstrack barely cuts at all.
        let cut = |s: &Synth| {
            (40..=200)
                .step_by(10)
                .map(|at| {
                    let v = across(s, at as f32);
                    let w = ridden_window(v.len());
                    let r = &v[w];
                    r.iter().copied().fold(f32::MIN, f32::max)
                        - r.iter().copied().fold(f32::MAX, f32::min)
                })
                .fold(0.0f32, f32::max)
        };
        assert!(cut(&sand) > cut(&soil), "{:.3} against {:.3}", cut(&sand), cut(&soil));
        assert!(cut(&grass) < cut(&soil), "{:.3} against {:.3}", cut(&grass), cut(&soil));
        assert!(
            ride(crate::trackprog::Surface::Sand).rut_depth
                > ride(crate::trackprog::Surface::Soil).rut_depth * 1.3,
            "sand's own figures are not deeper than soil's"
        );
    }

    #[test]
    fn every_surface_states_a_whole_ride() {
        // A surface added to the enum without a row here would fall through to soil's
        // numbers, which is exactly the fault this replaced.
        for s in [
            crate::trackprog::Surface::Soil,
            crate::trackprog::Surface::Sand,
            crate::trackprog::Surface::Grass,
        ] {
            let r = ride(s);
            assert!(r.rut_depth > 0.0 && r.rut_straight > 0.0, "{s:?} wears nothing");
            assert!(r.rut_straight < r.rut_depth, "{s:?} wears its straights as hard as its corners");
            assert!(r.rut_spacing > r.groove * 2.0, "{s:?} has grooves wider than the gap between them");
            assert!(r.brake.0 < r.accel.0, "{s:?} brakes in longer waves than it drives in");
            assert!(r.brake.1 > r.accel.1, "{s:?} builds taller bumps under power than under braking");
        }
    }

    #[test]
    fn wear_moves_the_ground_and_the_stack_opposite_ways() {
        // The dial's whole claim. Ground already cut into the heightmap is ground the surface
        // no longer has to give, so a raced track arrives with deeper grooves and less left
        // underneath — and a prepped one the other way round. Without both halves it is just
        // a way to make a track shallower.
        let cut = |w: f32| {
            let mut p = hairpins();
            p.terrain.wear = w;
            let s = synthesise(&p).expect("synthesise");
            (130..=170)
                .step_by(5)
                .map(|at| {
                    let g = groove_residual(&across(&s, at as f32));
                    -g.iter().copied().fold(f32::MAX, f32::min)
                })
                .fold(0.0f32, f32::max)
        };
        let stack = |w: f32| {
            let mut p = oval();
            p.terrain.wear = w;
            let s = synthesise(&p).expect("synthesise");
            dig_depth(&tht(&p, &s))
        };
        assert!(cut(1.0) > cut(0.0) * 1.5, "{:.3} against {:.3}", cut(1.0), cut(0.0));
        assert!(stack(1.0) < stack(0.0), "{:.3} against {:.3}", stack(1.0), stack(0.0));
        // And the default is the point every rut figure in this module was measured at, so a
        // programme that says nothing about wear gets the corpus's own ground.
        let d = crate::trackprog::default_wear();
        assert!((worn(&{ let mut p = oval(); p.terrain.wear = d; p }) - 1.0).abs() < 1e-6);
    }

    #[test]
    #[ignore]
    fn diag_relief() {
        let path = std::env::var("FROST_PROGRAM").unwrap();
        let p: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let s = synthesise(&p).unwrap();
        // What a feature stands, against what the ruts take out right beside it. If the two
        // are the same size the features are invisible — which is what a rider reported.
        for f in p.features.iter().take(40) {
            let at = f.at() + f.length() * 0.5;
            let k = s.stations.iter().position(|st| st.s >= at).unwrap_or(0);
            let v = across(&s, at);
            let g = groove_residual(&v);
            let rut = -g.iter().copied().fold(f32::MAX, f32::min);
            let before = across(&s, (f.at() - 12.0).max(1.0));
            let hi = v.iter().copied().fold(f32::MIN, f32::max);
            let lo = before.iter().copied().fold(f32::MIN, f32::max);
            println!(
                "  {:>7.1} {:<9} asked {:.2} m — stands {:+.2} over the ground 12 m before, \
                 ruts cut {:.2} beside it",
                f.at(), f.name(), f.height().abs(), hi - lo, rut
            );
            let _ = k;
        }
    }

    #[test]
    #[ignore]
    fn diag_profile() {
        let path = std::env::var("FROST_PROGRAM").unwrap();
        let p: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let s = synthesise(&p).unwrap();
        // The ground under the racing line, which is the only profile a rider meets.
        let at_line = |arc: f32| -> f32 {
            let k = s
                .stations
                .iter()
                .position(|st| st.s >= arc)
                .unwrap_or(s.stations.len() - 1);
            let st = s.stations[k];
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let t = s.line_lat[k];
            sample(&s.heights, s.gw, s.gh, (st.x + rx * t) / s.mps, (st.z + rz * t) / s.mps)
        };
        for f in p.features.iter().take(6) {
            let a = f.at();
            let base = at_line(a - 4.0);
            let mut peak = f32::MIN;
            let mut d = 0.0;
            while d <= f.length() {
                peak = peak.max(at_line(a + d) - base);
                d += 1.0;
            }
            // What the landscape alone does over the same distance, just before it.
            let drift = at_line(a - 4.0) - at_line(a - 4.0 - f.length());
            println!(
                "  {:>7.1} {:<9} asked {:.2} m — peaks {:+.2} above its own foot; the land \
                 moves {:+.2} over the same run just before",
                a, f.name(), f.height().abs(), peak, drift
            );
        }
    }

    #[test]
    #[ignore]
    fn diag_marks() {
        let path = std::env::var("FROST_PROGRAM").unwrap();
        let p: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let s = synthesise(&p).unwrap();
        for f in p.features.iter().filter(|f| f.height().abs() > 1.0).take(4) {
            let up = match f {
                Feature::Tabletop { height, length, .. } =>
                    crate::trackprog::tabletop_faces(*height, *length).0,
                Feature::Double { height, lip, .. } =>
                    crate::trackprog::double_faces(*height, *lip).ramp,
                _ => continue,
            };
            let at = f.at() + up * 0.55;
            let v = across(&s, at);
            let w = ridden_window(v.len());
            let r = &v[w];
            let g = groove_residual(&v);
            println!(
                "  {} at {:.0}: face relief across the ridden width {:.3} m; deepest mark {:.3} m",
                f.name(), f.at(),
                r.iter().copied().fold(f32::MIN, f32::max) - r.iter().copied().fold(f32::MAX, f32::min),
                -g.iter().copied().fold(f32::MAX, f32::min),
            );
        }
    }

    /// The ground at riding scale, with the sheets tiled the way the game tiles them.
    ///
    /// [`ui_shot`] answers a different question and cannot answer this one: it composites each
    /// band's *average* colour, because at a couple of metres to the pixel a 3 m tile of soil
    /// is below the picture's own resolution. So it shows where the bands are and nothing at
    /// all about what they look like — and every argument about whether a change to a sheet
    /// or a rut can be seen has to be settled somewhere.
    ///
    /// ```text
    /// FROST_PROGRAM=t.json FROST_AT=300 FROST_SPAN=26 FROST_OUT=/tmp/x.ppm \
    ///   cargo test --release -- --ignored --nocapture ground_closeup
    /// ```
    #[test]
    #[ignore = "needs a program — set FROST_PROGRAM"]
    fn ground_closeup() {
        let path = std::env::var("FROST_PROGRAM").expect("set FROST_PROGRAM");
        let p: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let s = synthesise(&p).unwrap();
        let at: f32 = std::env::var("FROST_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(300.0);
        let span: f32 =
            std::env::var("FROST_SPAN").ok().and_then(|v| v.parse().ok()).unwrap_or(26.0);
        let dim = 800usize;

        // Every band's own sheet, generated exactly as the shipped one is, plus the mask that
        // says where it goes. Walking `layers` rather than listing the bands again is what
        // stops this showing a track painted differently from the one that ships.
        const SHEET: usize = 512;
        let seed = p.terrain.relief.seed;
        let half = p.width * 0.5;
        let bands: Vec<(Vec<u8>, f32, Option<Vec<u8>>)> = layers(&p)
            .into_iter()
            .map(|l| {
                let sheet = band_pixels(SHEET, &l.look, seed ^ l.salt);
                let (mw, mh) = (s.gw - 1, s.gh - 1);
                let mask = match l.band {
                    BandMask::Everywhere => None,
                    other => Some(band_mask(&s, other, half, seed, mw, mh)),
                };
                (sheet, l.tile_m, mask)
            })
            .collect();

        let k = s.stations.iter().position(|st| st.s >= at).unwrap_or(0);
        let st = s.stations[k];
        let (fx, fz) = crate::trackprog::heading_vector(st.heading);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let mut px = Vec::with_capacity(dim * dim * 3);
        for y in 0..dim {
            for x in 0..dim {
                // Plan view over `span` metres, the line running up the picture.
                let u = (x as f32 / dim as f32 - 0.5) * span;
                let w = (0.5 - y as f32 / dim as f32) * span;
                let (wx, wz) = (st.x + rx * u + fx * w, st.z + rz * u + fz * w);
                let (cx, cy) = (wx / s.mps, wz / s.mps);

                let mut c = [0.0f32; 3];
                for (sheet, tile_m, mask) in &bands {
                    let cover = match mask {
                        None => 1.0,
                        Some(m) => {
                            // Bilinear, because the game filters its masks and a
                            // nearest-neighbour read invents a staircase along every band edge
                            // that is not in the shipped track at all. Reading one before
                            // fixing it would have had me chasing a fault in the renderer.
                            let (mw, mh) = (s.gw - 1, s.gh - 1);
                            let fx = (wx / p.terrain.size_x) * mw as f32 - 0.5;
                            let fy = (wz / p.terrain.size_z) * mh as f32 - 0.5;
                            let (x0, y0) = (fx.floor(), fy.floor());
                            let (tx, ty) = (fx - x0, fy - y0);
                            let at = |ix: f32, iy: f32| -> f32 {
                                let ix = (ix as isize).clamp(0, mw as isize - 1) as usize;
                                let iy = (iy as isize).clamp(0, mh as isize - 1) as usize;
                                m[iy * mw + ix] as f32 / 255.0
                            };
                            let top = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
                            let bot =
                                at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
                            top + (bot - top) * ty
                        }
                    };
                    if cover <= 0.001 {
                        continue;
                    }
                    // Tiled, which is the whole point: the sheet repeats every `tile_m`.
                    //
                    // Averaged over the pixel's own footprint rather than sampled at its
                    // centre. A 26 m crop is 33 mm a pixel and the sheet is 9 mm a texel, so
                    // a point sample shows one grain in sixteen — which is the sheet's noise,
                    // not the sheet. The game mipmaps for the same reason.
                    let sx = ((wx / tile_m).rem_euclid(1.0) * SHEET as f32) as usize % SHEET;
                    let sy = ((wz / tile_m).rem_euclid(1.0) * SHEET as f32) as usize % SHEET;
                    let step = ((span / dim as f32) / tile_m * SHEET as f32).round().max(1.0) as usize;
                    let mut t = [0.0f32; 3];
                    for oy in 0..step {
                        for ox in 0..step {
                            let i = (((sy + oy) % SHEET) * SHEET + (sx + ox) % SHEET) * 4;
                            for j in 0..3 {
                                t[j] += sheet[i + j] as f32;
                            }
                        }
                    }
                    let n = (step * step) as f32;
                    for j in 0..3 {
                        c[j] = c[j] * (1.0 - cover) + t[j] / n * cover;
                    }
                }

                // Lit off the ground's own slope, so relief reads the way it does in the game.
                let h =
                    |ox: f32, oz: f32| sample_smooth(&s.heights, s.gw, s.gh, cx + ox, cy + oz);
                let (dx, dz) = (h(1.0, 0.0) - h(-1.0, 0.0), h(0.0, 1.0) - h(0.0, -1.0));
                let n = [-dx, 2.0 * s.mps, -dz];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                let sun = [0.42f32, 0.78, -0.46];
                let d = ((n[0] * sun[0] + n[1] * sun[1] + n[2] * sun[2]) / len).max(0.0);
                let shade = 0.40 + 0.75 * d;
                for j in 0..3 {
                    px.push((c[j] * shade).clamp(0.0, 255.0) as u8);
                }
            }
        }

        let out = std::env::var("FROST_OUT").unwrap_or_else(|_| "/tmp/closeup.ppm".into());
        let mut f = format!("P6\n{dim} {dim}\n255\n").into_bytes();
        f.extend_from_slice(&px);
        std::fs::write(&out, f).unwrap();
        println!("wrote {out} — {span:.0} m of ground at {at:.0} m round the lap");
    }

    #[test]
    #[ignore]
    fn diag_spur() {
        let path = std::env::var("FROST_PROGRAM").unwrap();
        let p: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let s = synthesise(&p).unwrap();
        match &s.spur {
            None => println!("  NO START SPUR — the lap is riding its own opening straight"),
            Some(sp) => println!("  spur: {} stations", sp.stations.len()),
        }
        let st = p.stations(2.0);
        let (mut lo_x, mut hi_x, mut lo_z, mut hi_z) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for q in &st {
            lo_x = lo_x.min(q.x); hi_x = hi_x.max(q.x);
            lo_z = lo_z.min(q.z); hi_z = hi_z.max(q.z);
        }
        println!(
            "  plot {:.0}x{:.0}; lap x {lo_x:.0}..{hi_x:.0} z {lo_z:.0}..{hi_z:.0}; \
             nearest edge {:.0} m",
            p.terrain.size_x, p.terrain.size_z,
            lo_x.min(lo_z).min(p.terrain.size_x - hi_x).min(p.terrain.size_z - hi_z)
        );
        println!("  a spur needs {:.0} m of clear ground beside the opening straight",
                 START_FAN_HALF_M + SHOULDER_M * START_BANK);
    }
}


#[cfg(test)]
mod map_emit {
    use super::*;
    /// Write a `.map` where `scripts/map-loader-trace.py` can read it, and print the offset
    /// its trailing block starts at — the cursor the trace has to be given.
    #[test]
    #[ignore]
    fn emit_map() {
        let p: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        let s = synthesise(&p).unwrap();
        let m = map(&p, &s);
        let out = std::env::var("MXB_MAP_OUT").unwrap();
        std::fs::write(&out, &m).unwrap();
        let u = |o: usize| u32::from_le_bytes(m[o..o + 4].try_into().unwrap()) as usize;
        let mut o = 12 + u(8) * 56;
        let vc = u(o);
        o += 4 + vc * 80;
        o += 4 + u(o) * 12;
        let nodes = u(o);
        o += 4;
        for _ in 0..nodes {
            let g = u(o + 40);
            o += 44 + g * 24;
        }
        println!("wrote {} bytes; mesh ends at {o}", m.len());
    }
}

#[cfg(test)]
mod pkz_emit {
    use super::*;
    /// Build a **preview** `.pkz` from the demo program under a chosen name.
    ///
    /// Not playable, whatever the old name of this test suggested: it carries a `.map` and
    /// `.trh` we wrote, and the game crashes at the track-graphics stage on those. Dropping one
    /// of these into a tracks folder is the mistake that produced every generated-track crash
    /// in the record, and it was made again on 2026-09-06 because this said "playable".
    ///
    /// To get a track that actually loads, export the source with `builds_a_track` and run the
    /// three commands it writes beside them — `_map.bat`, `_trh.bat`, `_centerline.bat` — then
    /// zip the folder. They run under Wine on macOS.
    #[test]
    #[ignore]
    fn emit_preview_pkz() {
        let mut p: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        p.name = std::env::var("MXB_TRACK_NAME").unwrap_or_else(|_| "Testing 112".into());
        let s = synthesise(&p).unwrap();
        let out = std::path::PathBuf::from(std::env::var("MXB_PKZ_OUT").unwrap());
        let n = write_pkz(&p, &s, &out, false).unwrap();
        println!("wrote {} ({n} bytes) as \"{}\"", out.display(), p.name);
        println!("  this is a PREVIEW archive — the game cannot load it. Compile the exported");
        println!("  source with terrained/tracked for a track that runs.");
    }
}

#[cfg(test)]
mod blank_repro {
    /// The exact JSON `blank_track_program` hands the UI.
    fn blank_json() -> serde_json::Value {
        serde_json::json!({
            "name": "New Track",
            "author": "",
            "location": "",
            "width": 12.0,
            "terrain": {
                "sizeX": 400.0, "sizeZ": 400.0, "samples": 2049, "scale": 20.0,
                "relief": { "amplitude": 4.0, "wavelength": 130.0, "seed": 1, "texture": 0.06 },
                "surface": "soil"
            },
            "start": { "x": 120.0, "z": 260.0, "angle": 90.0 },
            "segments": [
                { "kind": "straight", "length": 120.0, "rise": 0.0 },
                { "kind": "arc", "radius": 45.0, "angle": 180.0, "rise": 0.0 },
                { "kind": "straight", "length": 120.0, "rise": 0.0 },
                { "kind": "arc", "radius": 45.0, "angle": 180.0, "rise": 0.0 }
            ],
            "features": []
        })
    }

    #[test]
    fn blank_track_survives_the_whole_path() {
        let prog: crate::trackprog::TrackProgram =
            serde_json::from_value(blank_json()).expect("deserialise");
        println!("check() -> {:?}", prog.check());
        let review = crate::trackllm::review(&prog);
        println!("problems = {:#?}", review.problems);
        println!("notes = {:#?}", review.notes);
        match crate::tracksynth::synthesise(&prog) {
            Ok(_) => println!("synthesise: OK"),
            Err(e) => println!("synthesise FAILED: {e:#}"),
        }
    }
}
