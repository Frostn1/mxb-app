//! The document a track is generated from.
//!
//! Not a heightmap. A model asked for a track emits *this* — a start point, a run of straights
//! and arcs, and the features laid along them by distance — and the synthesiser turns it into
//! terrain. Keeping the two apart is what makes the output editable: "tighten the rhythm
//! section" is an edit to a list of numbers, not to four million samples.
//!
//! The vocabulary isn't invented. MX Bikes' own centreline file, the `.tcl` that
//! `tracked -merge` reads, is a start position and a list of segments that are each either a
//! straight of some length or an arc of some radius through some angle. That is already how
//! track builders and riders describe a lap, so a program written in it converts to a `.tcl`
//! with nothing lost, and a corner is "radius 12 through 90°" rather than a row of control
//! points that only mean something once they're drawn.

#![allow(dead_code)]

use anyhow::{bail, Result};

/// How far past the finish a feature may end before it counts as a fault.
///
/// Not zero, and not a rounding tolerance either. The studio clamps a stranded jump to end
/// exactly at the line, and it works out where that is in double precision while this walks
/// the lap in single — so "exactly" differs between them by a fraction of a millimetre, and a
/// strict comparison rejects the very thing it just asked for. Half a metre is far below
/// anything that matters on a track and far above anything the two can disagree by.
const FEATURE_END_SLACK_M: f32 = 0.5;

/// Samples on the longest edge. Power of two plus one, as MX Bikes requires.
pub const DEFAULT_SAMPLES: u32 = 2049;

/// Which way `angle` faces: zero looks down +z, and it increases clockwise towards +x.
///
/// Chosen, not discovered — a `.tcl` states an angle without saying what it means. The
/// terrain and the `.tcl` are both written from this same convention, so if the game
/// disagrees the centreline lands beside the track rather than on it, and the fix is the sign
/// here. Nothing else in the pipeline depends on it.
pub fn heading_vector(theta: f32) -> (f32, f32) {
    (theta.sin(), theta.cos())
}

/// The rider's right, at a heading.
pub fn right_vector(theta: f32) -> (f32, f32) {
    (theta.cos(), -theta.sin())
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackProgram {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub location: String,
    pub terrain: Terrain,
    pub start: Start,
    pub segments: Vec<Segment>,
    /// Width of the riding line, metres. Published tracks measure 10–17 m.
    pub width: f32,
    #[serde(default)]
    pub features: Vec<Feature>,
    /// How far things ease into each other, in metres.
    ///
    /// One number for the whole track, because the three places it matters are the same
    /// question asked three times: where two jumps meet, where a straight becomes a corner,
    /// and how long a jump's own ramps are. Zero is every edge as sharp as the grid allows;
    /// a few metres is a track a machine shaped.
    #[serde(default = "default_blend")]
    pub blend: f32,
    /// Height the track is lifted or dropped by, at points round the lap.
    ///
    /// Empty means the track simply follows the ground it crosses, which is what it did
    /// before this existed. A `rise` on a segment says "climb four metres across this
    /// corner"; these say "be four metres up *here*" — the same shape stated as a curve
    /// rather than as a run of instructions, which is the form you can take hold of.
    #[serde(default)]
    pub elevation: Vec<Knot>,
}

/// One point on the lap's height curve.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Knot {
    /// Metres round the lap.
    pub at: f32,
    /// Metres above the ground the track would otherwise have followed.
    pub height: f32,
}

pub(crate) fn default_blend() -> f32 {
    // Enough to join two jumps that touch, little enough to leave a takeoff face crisp:
    // at 2.5 the demo track lost four degrees off its steepest ground.
    1.2
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Terrain {
    pub size_x: f32,
    pub size_z: f32,
    #[serde(default = "default_samples")]
    pub samples: u32,
    /// The whole height budget, metres. Every sample is quantised against this, so it is the
    /// resolution of the terrain as much as its range: at 2.2 m — what the official example
    /// track uses — a step is 34 microns, and at 200 m it is 3 mm and jump faces start to
    /// stair-step. Keep it just above the tallest thing on the track.
    pub scale: f32,
    #[serde(default)]
    pub relief: Relief,
    /// What the ground is. Decides the surfaces painted either side of the riding line, and
    /// with them what the track looks like.
    #[serde(default)]
    pub surface: Surface,
}

/// The ground a track is cut into.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Surface {
    /// Worked dirt with grass beyond it. The default, and most of the corpus.
    #[default]
    Soil,
    /// A sand track: the shoulder is sand rather than soil, and there is more of it.
    Sand,
    /// Grass right up to the riding line — a grasstrack or an early-season circuit.
    Grass,
}

fn default_samples() -> u32 {
    DEFAULT_SAMPLES
}

/// The landscape the track is cut into, before anything is built on it.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Relief {
    /// Peak-to-trough, metres.
    pub amplitude: f32,
    /// Metres between hills.
    pub wavelength: f32,
    pub seed: u32,
    /// Fine texture on the riding surface, metres peak-to-trough — braking bumps, ruts, the
    /// unevenness of ground that has been ridden on.
    ///
    /// Small, but not optional. A synthesised track is otherwise perfectly smooth, and
    /// perfectly smooth is a thing no real track is: a tabletop's top comes out flat to the
    /// millimetre across its whole width, which rides like glass and measures like nothing
    /// else in the corpus.
    #[serde(default = "default_texture")]
    pub texture: f32,
}

/// Measured, not chosen. The surface roughness of a published track — the mean absolute
/// second difference along the direction of travel — runs 2.56 cm on the riding line and
/// 2.51 cm two metres off it on Indiana. At 0.06 a generated track read 1.93 and 1.82: a
/// quarter smoother than real ground everywhere a rider actually is.
fn default_texture() -> f32 {
    0.085
}

impl Default for Relief {
    fn default() -> Self {
        Relief {
            amplitude: 8.0,
            wavelength: 180.0,
            seed: 1,
            texture: default_texture(),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Start {
    pub x: f32,
    pub z: f32,
    /// Degrees, per [`heading_vector`].
    pub angle: f32,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Segment {
    Straight {
        length: f32,
        /// Metres the ground climbs over this segment; negative drops. Zero follows the
        /// landscape, which is what a track does unless someone cut into it.
        #[serde(default)]
        rise: f32,
    },
    /// Signed radius — positive turns right, negative left — through `angle` degrees. Arc
    /// length falls out as `|radius| * angle`, which is exactly how a `.tcl` states it.
    Arc {
        radius: f32,
        angle: f32,
        #[serde(default)]
        rise: f32,
    },
}

impl Segment {
    pub fn rise(&self) -> f32 {
        match self {
            Segment::Straight { rise, .. } | Segment::Arc { rise, .. } => *rise,
        }
    }
}

/// The steepest face a jump is allowed, degrees.
///
/// Thirty because that is the ceiling across every published track measured, not a judgement:
/// their steepest faces run 19.2–29.6° at the ninetieth percentile and Millville, the steepest
/// of the ten, does not reach 30. A dirt lip pushed up by a machine cannot stand steeper than
/// the material holds.
pub const JUMP_FACE_DEG: f32 = 30.0;

/// The shortest a face may be however small the jump.
///
/// The angle alone makes a small jump *worse*: at 30° a 1.2 m double would get a two-metre
/// face where a flat four gives it twenty-four degrees. The angle is a ceiling on the tall
/// ones, not a target for all of them.
pub const JUMP_FACE_MIN_M: f32 = 4.0;

/// How long a double's ramp and its lip's back face are, in metres, for a given height.
///
/// One definition, used by both the shape and by [`Feature::length`], because they have to
/// agree about where the feature ends.
///
/// The 1.5 is not a fudge, and leaving it out is why this was still a wall after being fixed
/// once: a smoothstep's steepest point is half again as steep as its average, so a face sized
/// to *average* 30° peaks at 45. A 3.6 m double dropped at 53.5° at a flat four metres, 40.9°
/// sized by the average, and 30.0° sized by the peak.
pub fn double_faces(height: f32, lip: f32) -> (f32, f32) {
    let face = 1.5 * height.abs() / JUMP_FACE_DEG.to_radians().tan();
    let back = face.max(JUMP_FACE_MIN_M);
    // A short lip on a tall jump is a wall whichever side of it you are on.
    (lip.max(back), back)
}

/// Under this many degrees an arc is a drift, not a corner — a builder nudging a straight
/// back onto line.
pub const CORNER_DEG: f32 = 1.0;

/// A corner, as a rider meets it: a run of same-way arcs uninterrupted by anything longer
/// than a nudge, reported as `(degrees turned, tightest radius)`.
///
/// Published corners are never one arc. Indiana's are three to eighteen, each a little
/// tighter or looser than the last, and counting them singly says a track is made of 2° bends
/// when what you actually ride is a 160° turn.
pub fn turns(segments: &[Segment]) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::new();
    let mut cur: Option<(f32, f32, f32)> = None; // way, degrees, tightest
    for seg in segments {
        match *seg {
            Segment::Arc { radius, angle, .. }
                if radius != 0.0 && angle.abs() >= CORNER_DEG =>
            {
                // The radius carries which way, and only the radius — `stations` sweeps
                // `angle.abs()` and takes its direction from `radius.signum()`. Reading the
                // sign off the product merges a left turn into the right one before it,
                // because a program that signs both writes them the same way.
                let way = radius.signum();
                match cur {
                    Some((w, deg, r)) if w == way => {
                        cur = Some((w, deg + angle.abs(), r.min(radius.abs())))
                    }
                    other => {
                        if let Some((_, deg, r)) = other {
                            out.push((deg, r));
                        }
                        cur = Some((way, angle.abs(), radius.abs()));
                    }
                }
            }
            _ => {
                if seg.length() > 8.0 {
                    if let Some((_, deg, r)) = cur.take() {
                        out.push((deg, r));
                    }
                }
            }
        }
    }
    if let Some((_, deg, r)) = cur {
        out.push((deg, r));
    }
    out
}

impl Segment {
    pub fn length(&self) -> f32 {
        match self {
            Segment::Straight { length, .. } => *length,
            Segment::Arc { radius, angle, .. } => radius.abs() * angle.abs().to_radians(),
        }
    }
}

/// Something built on the riding line, placed by how far round the lap it is.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Feature {
    /// Up, along, down. The safe jump, and the commonest thing on a track.
    Tabletop { at: f32, length: f32, height: f32 },
    /// Two lips with air between them. `gap` is ground the rider must clear.
    Double {
        at: f32,
        height: f32,
        gap: f32,
        #[serde(default = "default_lip")]
        lip: f32,
    },
    /// One smooth rise, small enough to roll.
    Roller { at: f32, length: f32, height: f32 },
    /// A run of them, `spacing` apart.
    Whoops {
        at: f32,
        count: u32,
        spacing: f32,
        height: f32,
    },
    /// Ground that is higher after than before. Part of the landscape rather than built on
    /// it, so it moves the elevation profile instead of adding to it.
    StepUp { at: f32, length: f32, height: f32 },
    /// A banked wall on the outside of a corner. Which side that is comes from the corner.
    Berm { at: f32, length: f32, height: f32 },
    /// A groove worn into the line by everyone riding it. Corners grow their own — this is
    /// for putting one somewhere a corner wouldn't.
    Rut { at: f32, length: f32, depth: f32 },
    /// A shape drawn by hand: heights along the feature, from its start to its end.
    ///
    /// Its own kind rather than a field on the others, because once a jump has been shaped
    /// point by point it is no longer a tabletop with a taller top — it is a shape, and the
    /// parameters a tabletop has stop meaning anything about it.
    Custom {
        at: f32,
        length: f32,
        shape: Vec<ShapePoint>,
    },
}

/// One point of a hand-drawn feature: how far along it, and how high.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShapePoint {
    /// 0 at the feature's start, 1 at its end.
    pub u: f32,
    pub h: f32,
}

/// Measured against Indiana rather than chosen: its jumps reach half their height five
/// metres before the crest and full height at it, so the ramp is about ten metres long. At
/// six the takeoff is still at a tenth of its height five metres out — a spike rather than a
/// ramp, and it rides like one.
fn default_lip() -> f32 {
    10.0
}

impl Feature {
    pub fn at(&self) -> f32 {
        match self {
            Feature::Tabletop { at, .. }
            | Feature::Double { at, .. }
            | Feature::Roller { at, .. }
            | Feature::Whoops { at, .. }
            | Feature::StepUp { at, .. }
            | Feature::Berm { at, .. }
            | Feature::Rut { at, .. }
            | Feature::Custom { at, .. } => *at,
        }
    }

    /// How much of the lap it occupies.
    pub fn length(&self) -> f32 {
        match self {
            Feature::Tabletop { length, .. }
            | Feature::Roller { length, .. }
            | Feature::StepUp { length, .. }
            | Feature::Berm { length, .. }
            | Feature::Rut { length, .. }
            | Feature::Custom { length, .. } => *length,
            // Two faces up and two back down, with the gap between them. The two lengths
            // come from `double_faces` rather than being written out again here: they used
            // to be, and when the faces were lengthened this went on reporting the old
            // figure, so the profile was written up to a point eight metres short of where
            // the shape actually ended and the last ramp was cut off into a step.
            Feature::Double {
                height, gap, lip, ..
            } => {
                let (lip, back) = double_faces(*height, *lip);
                (lip + back) * 2.0 + gap
            }
            Feature::Whoops {
                count, spacing, ..
            } => *count as f32 * spacing,
        }
    }

    /// What to call it in a sentence.
    pub fn name(&self) -> &'static str {
        match self {
            Feature::Tabletop { .. } => "tabletop",
            Feature::Double { .. } => "double",
            Feature::Roller { .. } => "roller",
            Feature::Whoops { .. } => "whoop section",
            Feature::StepUp { .. } => "step-up",
            Feature::Berm { .. } => "berm",
            Feature::Rut { .. } => "rut",
            Feature::Custom { .. } => "shape",
        }
    }

    pub fn height(&self) -> f32 {
        match self {
            Feature::Tabletop { height, .. }
            | Feature::Double { height, .. }
            | Feature::Roller { height, .. }
            | Feature::Whoops { height, .. }
            | Feature::StepUp { height, .. }
            | Feature::Berm { height, .. } => *height,
            // A rut goes down rather than up, and its depth is the figure that matters.
            Feature::Rut { depth, .. } => -*depth,
            // The tallest point it was drawn with.
            Feature::Custom { shape, .. } => shape
                .iter()
                .map(|p| p.h)
                .fold(0.0f32, |a, b| if b.abs() > a.abs() { b } else { a }),
        }
    }
}

/// A worked example of a track program: what a good one looks like.
///
/// Grown as a simple closed loop on a lattice, then smoothed and fitted back to arcs and
/// straights. That construction is the point. A lap that goes round a centre once can never
/// cross itself, which is why the prompt says to build one that way — and it can only ever
/// draw a star, because every part of it faces outwards from the middle. Published tracks are
/// not star-shaped: Indiana folds back across its own infield four times. A loop grown on a
/// lattice folds as often as it likes and is still simple, so it cannot cross either.
///
/// 1900 m, 114 segments of which 88 are arcs, nineteen corners at 167° apiece, 2634° of
/// turning, and it closes to 0.01 m. Thirty-six features, seven of them over 2.2 m and the
/// rest small ground, which is the ratio Indiana has. It climbs 22 m round the lap, which is
/// Indiana's 21.3 — half the published corpus is a hillside and a flat plot rides like one.
///
/// This is the schema's own test. It is parsed by the test suite, synthesised, and measured
/// against published tracks, so it cannot drift away from what the code accepts.
pub const EXAMPLE: &str = r#"{
      "name": "Corpus National",
      "author": "MXB App",
      "location": "Generated",
      "width": 12.0,
      "terrain": {
        "sizeX": 500.0, "sizeZ": 480.0, "samples": 2049, "scale": 60.0,
        "relief": { "amplitude": 22.0, "wavelength": 150.0, "seed": 7 }
      },
      "start": { "x": 261.50, "z": 259.20, "angle": 0.0 },
      "segments": [
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -123.5678, "angle": 13.6508 },
        { "kind": "arc", "radius": -13.0062, "angle": 44.0527 },
        { "kind": "arc", "radius": -11.8595, "angle": 48.7934 },
        { "kind": "arc", "radius": -31.8091, "angle": 12.6087 },
        { "kind": "arc", "radius": -32.7943, "angle": 17.4713 },
        { "kind": "arc", "radius": -26.6682, "angle": 49.9128 },
        { "kind": "straight", "length": 167.7996 },
        { "kind": "arc", "radius": 84.4199, "angle": 5.4104 },
        { "kind": "arc", "radius": 21.4397, "angle": 21.3793 },
        { "kind": "arc", "radius": 15.8310, "angle": 25.3344 },
        { "kind": "arc", "radius": 22.6995, "angle": 28.2454 },
        { "kind": "arc", "radius": 60.8936, "angle": 49.3880 },
        { "kind": "arc", "radius": 26.7283, "angle": 45.0164 },
        { "kind": "arc", "radius": 80.1768, "angle": 4.2877 },
        { "kind": "straight", "length": 15.1274 },
        { "kind": "arc", "radius": -126.8340, "angle": 6.0338 },
        { "kind": "arc", "radius": -26.7551, "angle": 12.8490 },
        { "kind": "arc", "radius": -17.6921, "angle": 64.4226 },
        { "kind": "straight", "length": 29.5264 },
        { "kind": "arc", "radius": 115.0737, "angle": 2.9874 },
        { "kind": "arc", "radius": 52.4542, "angle": 14.1619 },
        { "kind": "arc", "radius": 24.7542, "angle": 57.8647 },
        { "kind": "arc", "radius": 30.8559, "angle": 29.7101 },
        { "kind": "arc", "radius": 23.9270, "angle": 62.3633 },
        { "kind": "arc", "radius": 87.2519, "angle": 3.9400 },
        { "kind": "straight", "length": 12.5492 },
        { "kind": "arc", "radius": 126.6426, "angle": 1.3573 },
        { "kind": "straight", "length": 10.9990 },
        { "kind": "arc", "radius": 96.5602, "angle": 8.3072 },
        { "kind": "straight", "length": 11.6032 },
        { "kind": "arc", "radius": -43.4426, "angle": 5.2755 },
        { "kind": "arc", "radius": -20.5875, "angle": 80.1157 },
        { "kind": "arc", "radius": -30.9801, "angle": 16.6449 },
        { "kind": "arc", "radius": -18.6291, "angle": 76.7464 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 110.9314, "angle": 17.8080 },
        { "kind": "arc", "radius": 37.0623, "angle": 46.3780 },
        { "kind": "arc", "radius": 19.8834, "angle": 23.0527 },
        { "kind": "arc", "radius": 11.9757, "angle": 85.0650 },
        { "kind": "arc", "radius": 60.6897, "angle": 5.6645 },
        { "kind": "straight", "length": 57.4345 },
        { "kind": "arc", "radius": -61.3775, "angle": 12.9956 },
        { "kind": "arc", "radius": -21.3694, "angle": 55.0159 },
        { "kind": "arc", "radius": -61.0776, "angle": 8.4427 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 112.8374, "angle": 22.8614 },
        { "kind": "arc", "radius": 38.4760, "angle": 38.2794 },
        { "kind": "arc", "radius": 22.9913, "angle": 19.9365 },
        { "kind": "arc", "radius": 15.8116, "angle": 78.2909 },
        { "kind": "straight", "length": 24.9105 },
        { "kind": "arc", "radius": -57.5949, "angle": 4.9740 },
        { "kind": "arc", "radius": -33.7874, "angle": 18.6535 },
        { "kind": "arc", "radius": -23.7864, "angle": 36.1315 },
        { "kind": "arc", "radius": -30.9063, "angle": 33.3694 },
        { "kind": "arc", "radius": -21.6414, "angle": 66.1877 },
        { "kind": "arc", "radius": -41.7145, "angle": 13.8512 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 65.8709, "angle": 20.9058 },
        { "kind": "arc", "radius": 24.3858, "angle": 65.7876 },
        { "kind": "arc", "radius": 14.5056, "angle": 99.6379 },
        { "kind": "straight", "length": 24.4722 },
        { "kind": "arc", "radius": -47.3968, "angle": 7.2531 },
        { "kind": "arc", "radius": -27.3969, "angle": 23.0046 },
        { "kind": "arc", "radius": -18.1854, "angle": 31.5065 },
        { "kind": "arc", "radius": -23.5319, "angle": 33.6835 },
        { "kind": "straight", "length": 12.6349 },
        { "kind": "arc", "radius": 120.7074, "angle": 0.4747 },
        { "kind": "arc", "radius": 55.0316, "angle": 12.7710 },
        { "kind": "arc", "radius": 27.8369, "angle": 66.3307 },
        { "kind": "arc", "radius": 25.5462, "angle": 17.9426 },
        { "kind": "arc", "radius": 12.9619, "angle": 94.9762 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -132.0002, "angle": 17.3623 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -92.8368, "angle": 4.3202 },
        { "kind": "arc", "radius": -51.5586, "angle": 25.8798 },
        { "kind": "arc", "radius": -24.4260, "angle": 21.1112 },
        { "kind": "arc", "radius": -28.9972, "angle": 43.7999 },
        { "kind": "arc", "radius": -22.6760, "angle": 17.6870 },
        { "kind": "arc", "radius": -13.3076, "angle": 81.5663 },
        { "kind": "straight", "length": 8.0076 },
        { "kind": "arc", "radius": 74.1820, "angle": 18.5368 },
        { "kind": "arc", "radius": 120.3460, "angle": 9.5218 },
        { "kind": "straight", "length": 13.0951 },
        { "kind": "arc", "radius": 100.0467, "angle": 8.7709 },
        { "kind": "arc", "radius": 34.8919, "angle": 25.7139 },
        { "kind": "arc", "radius": 14.3054, "angle": 79.7910 },
        { "kind": "arc", "radius": 45.2650, "angle": 34.7343 },
        { "kind": "straight", "length": 8.5671 },
        { "kind": "arc", "radius": -65.1851, "angle": 3.5159 },
        { "kind": "arc", "radius": -32.4336, "angle": 35.3312 },
        { "kind": "arc", "radius": -54.5624, "angle": 14.0811 },
        { "kind": "straight", "length": 21.8650 },
        { "kind": "arc", "radius": 52.8372, "angle": 5.4219 },
        { "kind": "arc", "radius": 27.4465, "angle": 22.9630 },
        { "kind": "arc", "radius": 21.1475, "angle": 37.9308 },
        { "kind": "arc", "radius": 28.0883, "angle": 12.2390 },
        { "kind": "arc", "radius": 37.5941, "angle": 27.4332 },
        { "kind": "arc", "radius": 25.5494, "angle": 60.7410 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -93.0568, "angle": 2.9565 },
        { "kind": "arc", "radius": -27.7322, "angle": 18.5943 },
        { "kind": "arc", "radius": -19.0394, "angle": 56.9968 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 58.9191, "angle": 5.4042 },
        { "kind": "arc", "radius": 24.0861, "angle": 35.6818 },
        { "kind": "arc", "radius": 35.5859, "angle": 17.7108 },
        { "kind": "arc", "radius": 60.8506, "angle": 10.3574 },
        { "kind": "arc", "radius": 108.6282, "angle": 4.1319 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 89.3081, "angle": 3.8493 },
        { "kind": "straight", "length": 8.0000 }
      ],
      "features": [
        { "kind": "tabletop", "at": 18.7, "length": 25.7, "height": 1.10 },
        { "kind": "berm", "at": 47.4, "length": 10.1, "height": 1.70 },
        { "kind": "double", "at": 120.5, "height": 3.60, "gap": 16.4, "lip": 11.0 },
        { "kind": "roller", "at": 153.1, "length": 14.6, "height": 0.64 },
        { "kind": "tabletop", "at": 185.7, "length": 23.4, "height": 1.20 },
        { "kind": "tabletop", "at": 218.2, "length": 24.6, "height": 1.40 },
        { "kind": "roller", "at": 250.8, "length": 11.5, "height": 0.77 },
        { "kind": "roller", "at": 316.1, "length": 11.6, "height": 0.80 },
        { "kind": "double", "at": 335.8, "height": 1.90, "gap": 9.2, "lip": 9.5 },
        { "kind": "tabletop", "at": 390.5, "length": 30.4, "height": 1.60 },
        { "kind": "tabletop", "at": 449.0, "length": 34.0, "height": 1.50 },
        { "kind": "stepUp", "at": 466.7, "length": 19.3, "height": 2.20 },
        { "kind": "tabletop", "at": 566.9, "length": 29.6, "height": 2.90 },
        { "kind": "roller", "at": 589.5, "length": 15.7, "height": 0.70 },
        { "kind": "roller", "at": 687.9, "length": 13.5, "height": 0.46 },
        { "kind": "whoops", "at": 702.6, "count": 7, "spacing": 4.2, "height": 0.57 },
        { "kind": "berm", "at": 754.5, "length": 17.8, "height": 1.70 },
        { "kind": "double", "at": 789.5, "height": 3.20, "gap": 13.7, "lip": 11.0 },
        { "kind": "double", "at": 811.0, "height": 1.30, "gap": 8.9, "lip": 9.5 },
        { "kind": "roller", "at": 832.4, "length": 15.9, "height": 0.54 },
        { "kind": "tabletop", "at": 888.9, "length": 32.9, "height": 2.50 },
        { "kind": "roller", "at": 913.4, "length": 14.4, "height": 0.82 },
        { "kind": "double", "at": 1002.4, "height": 1.50, "gap": 8.8, "lip": 9.5 },
        { "kind": "tabletop", "at": 1112.5, "length": 24.8, "height": 1.10 },
        { "kind": "roller", "at": 1197.0, "length": 11.1, "height": 0.79 },
        { "kind": "tabletop", "at": 1260.0, "length": 32.5, "height": 1.10 },
        { "kind": "tabletop", "at": 1353.4, "length": 34.4, "height": 3.10 },
        { "kind": "roller", "at": 1377.8, "length": 11.3, "height": 0.80 },
        { "kind": "tabletop", "at": 1402.2, "length": 30.5, "height": 1.50 },
        { "kind": "double", "at": 1495.8, "height": 2.90, "gap": 15.6, "lip": 11.0 },
        { "kind": "roller", "at": 1518.3, "length": 14.4, "height": 0.75 },
        { "kind": "roller", "at": 1540.8, "length": 11.5, "height": 0.66 },
        { "kind": "double", "at": 1607.3, "height": 1.60, "gap": 8.2, "lip": 9.5 },
        { "kind": "roller", "at": 1620.8, "length": 14.3, "height": 0.79 },
        { "kind": "double", "at": 1667.4, "height": 1.20, "gap": 12.7, "lip": 9.5 },
        { "kind": "tabletop", "at": 1681.0, "length": 22.8, "height": 1.00 }
      ]
    }"#;

// ---------------------------------------------------------------------------
// Walking the centreline
// ---------------------------------------------------------------------------

/// An angle brought into (-pi, pi].
fn wrap(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut x = (a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    if x <= -std::f32::consts::PI {
        x += tau;
    }
    x
}

/// One point on the riding line, and which way it faces there.
#[derive(Clone, Copy, Debug)]
pub struct Station {
    pub x: f32,
    pub z: f32,
    /// Radians.
    pub heading: f32,
    /// Metres from the start.
    pub s: f32,
    /// Signed curvature, 1/metres — positive turning right. Zero on a straight. This is what
    /// tells a berm which side of the track to stand on.
    pub curvature: f32,
}

impl TrackProgram {
    pub fn lap_length(&self) -> f32 {
        self.segments.iter().map(|s| s.length()).sum()
    }

    /// The centreline, sampled every `step` metres.
    ///
    /// Arcs are evaluated from their centre rather than integrated a step at a time, so a
    /// long sweeping corner ends exactly where its radius and angle say it does and a lap
    /// closes on itself to the millimetre.
    pub fn stations(&self, step: f32) -> Vec<Station> {
        let step = step.max(0.01);
        let mut out = Vec::new();
        let mut x = self.start.x;
        let mut z = self.start.z;
        let mut theta = self.start.angle.to_radians();
        let mut s = 0.0f32;

        for seg in &self.segments {
            let len = seg.length();
            if len <= 0.0 {
                continue;
            }
            let n = (len / step).ceil().max(1.0) as usize;
            match *seg {
                Segment::Straight { .. } => {
                    let (dx, dz) = heading_vector(theta);
                    for i in 0..n {
                        let u = len * i as f32 / n as f32;
                        out.push(Station {
                            x: x + dx * u,
                            z: z + dz * u,
                            heading: theta,
                            s: s + u,
                            curvature: 0.0,
                        });
                    }
                    x += dx * len;
                    z += dz * len;
                }
                Segment::Arc { radius, angle, .. } => {
                    let sweep = angle.abs().to_radians();
                    let turn = radius.signum();
                    let r = radius.abs().max(0.01);
                    let (rx, rz) = right_vector(theta);
                    let (cx, cz) = (x + rx * r * turn, z + rz * r * turn);
                    let curvature = turn / r;
                    for i in 0..n {
                        let phi = sweep * i as f32 / n as f32;
                        let th = theta + turn * phi;
                        let (rx, rz) = right_vector(th);
                        out.push(Station {
                            x: cx - rx * r * turn,
                            z: cz - rz * r * turn,
                            heading: th,
                            s: s + r * phi,
                            curvature,
                        });
                    }
                    theta += turn * sweep;
                    let (rx, rz) = right_vector(theta);
                    x = cx - rx * r * turn;
                    z = cz - rz * r * turn;
                }
            }
            s += len;
        }
        // Segments emit their start but not their end, so the last one leaves the finish
        // itself unsampled — up to a step short. Close it, or a lap reads as not quite
        // meeting itself and the corridor stops just before the line.
        if let Some(last) = out.last().copied() {
            out.push(Station {
                x,
                z,
                heading: theta,
                s,
                curvature: last.curvature,
            });
        }
        out
    }

    /// Everything that would make the synthesiser produce nonsense, said before it does.
    pub fn check(&self) -> Result<()> {
        let t = &self.terrain;
        if t.samples < 129 || (t.samples - 1) & (t.samples - 2) != 0 {
            bail!(
                "samples must be a power of two plus one (2049 is the usual), not {}",
                t.samples
            );
        }
        if !(t.size_x > 0.0 && t.size_z > 0.0) {
            bail!("terrain size must be positive");
        }
        if t.scale <= 0.0 {
            bail!("height budget must be positive");
        }
        if self.width <= 0.0 {
            bail!("track width must be positive");
        }
        if self.segments.is_empty() {
            bail!("a track needs at least one segment");
        }

        // A feature hanging off the end of the lap is silently dropped by the synthesiser,
        // which looks like the model forgot to write it.
        let lap = self.lap_length();
        for f in &self.features {
            if f.at() < 0.0 {
                bail!("the {} sits at {:.0} m, before the start", f.name(), f.at());
            }
            let end = f.at() + f.length();
            if end > lap + FEATURE_END_SLACK_M {
                // Says how to fix it, not just that it is broken: this happens when a corner
                // is shortened or removed under a jump that was already there, and the way
                // out is a number rather than an insight.
                bail!(
                    "the {} at {:.0} m ends {:.0} m past the finish — move it back to {:.0} m \
                     or earlier, or give the lap its length back",
                    f.name(),
                    f.at(),
                    end - lap,
                    (lap - f.length()).max(0.0)
                );
            }
        }

        // The lap has to fit on the ground it's drawn on, with room for the track's width.
        let margin = self.width;
        for st in self.stations(2.0) {
            if st.x < margin
                || st.z < margin
                || st.x > t.size_x - margin
                || st.z > t.size_z - margin
            {
                bail!(
                    "the lap leaves the terrain at {:.0} m round ({:.0}, {:.0}) — the ground is \
                     {:.0} x {:.0} m",
                    st.s,
                    st.x,
                    st.z,
                    t.size_x,
                    t.size_z
                );
            }
        }
        Ok(())
    }

    /// Segments that bring the finish back to the start, or `None` if it is already there.
    ///
    /// A turn, a straight, and a turn — the shortest of the two same-direction Dubins paths
    /// between two poses. Both are tried and the shorter kept, which is enough for a lap that
    /// has drifted open under an edit; the mixed-direction paths matter only when the two
    /// poses are closer together than the turning circles, and a lap that has come apart
    /// never is.
    ///
    /// Closing the *pose* is what closes the lap. Position alone leaves a kink at the line;
    /// heading alone leaves it parallel and somewhere else.
    pub fn closing_segments(&self, radius: f32) -> Option<Vec<Segment>> {
        let st = self.stations(1.0);
        let last = *st.last()?;
        let r = radius.abs().max(1.0);
        let goal = (self.start.x, self.start.z, self.start.angle.to_radians());

        let gap = ((last.x - goal.0).powi(2) + (last.z - goal.1).powi(2)).sqrt();
        let turned = wrap(goal.2 - last.heading).abs();
        if gap < 0.5 && turned < 0.02 {
            return None;
        }

        // `turn` is +1 for a pair of right-hand circles, -1 for left.
        let solve = |turn: f32| -> Option<(f32, f32, f32)> {
            let centre = |x: f32, z: f32, th: f32| {
                let (rx, rz) = right_vector(th);
                (x + rx * r * turn, z + rz * r * turn)
            };
            let c1 = centre(last.x, last.z, last.heading);
            let c2 = centre(goal.0, goal.1, goal.2);
            let (dx, dz) = (c2.0 - c1.0, c2.1 - c1.1);
            let run = (dx * dx + dz * dz).sqrt();
            if run < 1e-3 {
                return None;
            }
            // The straight's heading, in the same convention the walk uses.
            let th_s = dx.atan2(dz);
            let sweep = |from: f32, to: f32| {
                let d = wrap(to - from) * turn;
                if d < 0.0 { d + std::f32::consts::TAU } else { d }
            };
            Some((sweep(last.heading, th_s), run, sweep(th_s, goal.2)))
        };

        let mut best: Option<(f32, f32, Vec<Segment>)> = None;
        for turn in [1.0f32, -1.0] {
            let Some((a1, run, a2)) = solve(turn) else {
                continue;
            };
            let cost = (a1 + a2) * r + run;
            let segs = vec![
                Segment::Arc { radius: r * turn, angle: a1.to_degrees(), rise: 0.0 },
                Segment::Straight { length: run, rise: 0.0 },
                Segment::Arc { radius: r * turn, angle: a2.to_degrees(), rise: 0.0 },
            ];
            if best.as_ref().map(|b| cost < b.0).unwrap_or(true) {
                best = Some((cost, run, segs));
            }
        }
        best.map(|(_, _, segs)| segs)
    }

    /// How far the finish is from the start. A lap that doesn't close is a dead end, and the
    /// error is easier to read as a distance than as a drawing.
    pub fn closure_error(&self) -> f32 {
        let st = self.stations(1.0);
        match st.last() {
            Some(l) => ((l.x - self.start.x).powi(2) + (l.z - self.start.z).powi(2)).sqrt(),
            None => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prog(segments: Vec<Segment>) -> TrackProgram {
        TrackProgram {
            name: "t".into(),
            author: String::new(),
            location: String::new(),
            terrain: Terrain {
                size_x: 400.0,
                size_z: 400.0,
                samples: 2049,
                scale: 20.0,
                relief: Relief::default(),
                surface: Surface::default(),
            },
            start: Start {
                x: 200.0,
                z: 200.0,
                angle: 0.0,
            },
            segments,
            width: 12.0,
            features: Vec::new(),
            blend: default_blend(),
            elevation: Vec::new(),
        }
    }

    #[test]
    fn an_arc_states_its_own_length() {
        // The example track's second segment: radius 4.974413 through 179.492554°, which its
        // own file calls 15.583522 m long.
        let seg = Segment::Arc { radius: 4.974413, angle: 179.492554, rise: 0.0 };
        assert!((seg.length() - 15.583522).abs() < 1e-3, "{}", seg.length());
    }

    #[test]
    fn four_right_angles_close_a_square() {
        let p = prog(vec![
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
        ]);
        assert!(p.closure_error() < 0.05, "{} m", p.closure_error());
        assert!((p.lap_length() - (200.0 + 4.0 * 20.0 * std::f32::consts::FRAC_PI_2)).abs() < 0.01);
    }

    #[test]
    fn a_left_turn_goes_the_other_way() {
        let right = prog(vec![Segment::Arc { radius: 30.0, angle: 90.0, rise: 0.0 }]);
        let left = prog(vec![Segment::Arc { radius: -30.0, angle: 90.0, rise: 0.0 }]);
        let (r, l) = (
            *right.stations(1.0).last().unwrap(),
            *left.stations(1.0).last().unwrap(),
        );
        // Both start heading +z from the same point, so they end either side of it.
        assert!(r.x > 200.0 && l.x < 200.0, "right {} left {}", r.x, l.x);
        assert!((r.z - l.z).abs() < 0.1);
    }

    #[test]
    fn curvature_points_into_the_corner() {
        let p = prog(vec![Segment::Arc { radius: 25.0, angle: 45.0, rise: 0.0 }]);
        let st = p.stations(1.0);
        assert!((st[0].curvature - 1.0 / 25.0).abs() < 1e-4);
        let straight = prog(vec![Segment::Straight { length: 10.0, rise: 0.0 }]);
        assert_eq!(straight.stations(1.0)[0].curvature, 0.0);
    }

    #[test]
    fn an_open_lap_can_be_closed() {
        // Three quarters of a rounded square: it ends facing the wrong way, a long way off.
        let mut p = prog(vec![
            Segment::Straight { length: 60.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 60.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 60.0, rise: 0.0 },
        ]);
        assert!(p.closure_error() > 50.0, "the fixture should be open");

        let add = p.closing_segments(20.0).expect("it has somewhere to go");
        p.segments.extend(add);
        assert!(
            p.closure_error() < 1.0,
            "still {:.1} m from home",
            p.closure_error()
        );
        // And facing the way it started, or the line has a kink in it.
        let end = *p.stations(1.0).last().unwrap();
        assert!(
            wrap(end.heading - p.start.angle.to_radians()).abs() < 0.05,
            "ends {:.1} degrees off",
            wrap(end.heading - p.start.angle.to_radians()).to_degrees()
        );
    }

    #[test]
    fn a_closed_lap_needs_nothing_added() {
        let p = prog(vec![
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 20.0, angle: 90.0, rise: 0.0 },
        ]);
        assert!(p.closing_segments(20.0).is_none());
    }

    #[test]
    fn a_lap_that_leaves_the_ground_says_so() {
        let p = prog(vec![Segment::Straight { length: 500.0, rise: 0.0 }]);
        let err = p.check().unwrap_err().to_string();
        assert!(err.contains("leaves the terrain"), "{err}");
    }

    /// The studio pulls a stranded jump back to end on the line, and works out where that is
    /// in double precision while this walks the lap in single. A check that rejects what the
    /// editor just did leaves someone with an error and no field to fix it in.
    ///
    /// Reproduced the way it actually happens: the real programme, the lap summed in f64 the
    /// way the browser sums it, and the clamped position taken back through JSON — which is
    /// where the f64 becomes an f32.
    #[test]
    fn a_feature_clamped_to_the_finish_is_not_past_it() {
        let mut p: TrackProgram = serde_json::from_str(EXAMPLE).unwrap();

        // `lapLength` in `api/trackgen.ts`, in the precision the browser has.
        let lap64: f64 = p
            .segments
            .iter()
            .map(|s| match s {
                Segment::Straight { length, .. } => *length as f64,
                Segment::Arc { radius, angle, .. } => {
                    (*radius as f64).abs() * (*angle as f64).abs() * std::f64::consts::PI / 180.0
                }
            })
            .sum();

        let last = p.features.len() - 1;
        let length = p.features[last].length() as f64;
        let at = (lap64 - length) as f32;
        p.features[last] = match p.features[last].clone() {
            Feature::Berm { length, height, .. } => Feature::Berm { at, length, height },
            other => other,
        };

        // Through JSON and back, because that is the trip the number really takes.
        let round: TrackProgram = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(
            round.check().is_ok(),
            "{}",
            round.check().unwrap_err().to_string()
        );
    }

    #[test]
    fn a_feature_past_the_finish_says_so() {
        let mut p = prog(vec![Segment::Straight { length: 100.0, rise: 0.0 }]);
        p.features.push(Feature::Tabletop {
            at: 90.0,
            length: 20.0,
            height: 2.0,
        });
        let err = p.check().unwrap_err().to_string();
        assert!(err.contains("past the"), "{err}");
    }

    #[test]
    fn samples_must_be_a_power_of_two_plus_one() {
        let mut p = prog(vec![Segment::Straight { length: 50.0, rise: 0.0 }]);
        p.terrain.samples = 2048;
        assert!(p.check().is_err());
        p.terrain.samples = 2049;
        assert!(p.check().is_ok());
        p.terrain.samples = 1025;
        assert!(p.check().is_ok());
    }
}
