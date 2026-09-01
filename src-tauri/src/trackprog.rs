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
    /// Metres the ground falls across the plot, and which way — a hillside rather than a
    /// plain.
    ///
    /// Not a refinement. Measured as how much the ground rises and falls over a given
    /// distance, Indiana runs 0.15 m over 5 m and 9.36 m over 200 — a ratio of 62 where a
    /// noise field of any wavelength saturates around 25, because noise flattens out past
    /// half its wavelength and a slope does not. It is 0.030 m per metre at five and 0.047 at
    /// two hundred: near enough a constant grade. Indiana is not a bumpy plain with a track
    /// on it, it is a hillside, and so are Millville, Washougal, Flanders and Sardegna —
    /// their laps climb 48 to 66 m.
    ///
    /// Zero is a flat plot, which is also real: Lambretta Lynds climbs 2.2 m.
    #[serde(default)]
    pub tilt: f32,
    /// Which way it falls, degrees, in the same convention as a heading.
    #[serde(default)]
    pub tilt_angle: f32,
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
            tilt: 0.0,
            tilt_angle: 0.0,
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

/// The gentlest face — the one a landing gets. Published landings measure 19.0° at the
/// ninetieth against a takeoff's 27.0: a built takeoff is short because that is what throws
/// you, and the landing is long because that is what catches you.
pub const JUMP_LANDING_DEG: f32 = 22.0;

/// A tabletop's ramp up, its flat top and its ramp down, in metres.
///
/// The ramps used to be fixed fractions of the feature's length — 27% up and 44% down — so a
/// short tabletop got a short ramp however tall it was asked to be, and how steep it came out
/// depended on nothing but the ratio of the two numbers. A 3 m tabletop 16 m long ramped at
/// 39°. Sized from the height and an angle instead, the way a double's faces now are.
///
/// The stated length is what the *top* is measured against: the ramps are added to it, so a
/// tabletop's footprint is longer than the number asked for and [`Feature::length`] reports
/// the whole thing.
pub fn tabletop_faces(height: f32, length: f32) -> (f32, f32, f32) {
    let h = height.abs();
    // Whichever is longer: the angle's, or the fraction of the stated length the ramps used
    // to be. The angle alone makes a *short* jump steeper than it was — at 30° a one-metre
    // tabletop gets a 2.6 m ramp where 27% of a 22 m length gave it 5.9 m — which is the same
    // way round as it bit on the double. The angle is a ceiling for the tall ones, not a
    // target for all of them.
    let up = (1.5 * h / JUMP_FACE_DEG.to_radians().tan())
        .max(length * 0.27)
        .max(JUMP_FACE_MIN_M);
    let down = (1.5 * h / JUMP_LANDING_DEG.to_radians().tan())
        .max(length * 0.44)
        .max(JUMP_FACE_MIN_M);
    // Whatever the asked-for length has left once the faces are in it, and never negative:
    // a tabletop too short for its own height is a peaked jump, which is a real thing.
    let top = (length - up - down).max(0.0);
    (up, top, down)
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
            // The ramps are sized from the height, so the footprint is longer than the
            // stated length and this has to say so — the profile is only written as far as
            // `at + length`, and anything past it is cut off into a step.
            Feature::Tabletop { height, length, .. } => {
                let (up, top, down) = tabletop_faces(*height, *length);
                up + top + down
            }
            Feature::Roller { length, .. }
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
/// 1905 m, 137 segments of which 109 are arcs, seventeen corners at a median 12.6 m through
/// their tightest point — Indiana's is 10.6 — and 2651° of turning. Forty features, seven of them over 2.2 m and the
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
        "sizeX": 500.0, "sizeZ": 480.0, "samples": 2049, "scale": 62.0,
        "relief": { "amplitude": 6.0, "wavelength": 260.0, "seed": 3, "tilt": 26.0, "tiltAngle": 35.0 }
      },
      "start": { "x": 231.10, "z": 262.90, "angle": 0.0 },
      "segments": [
        { "kind": "arc", "radius": -32.8954, "angle": 13.9340 },
        { "kind": "straight", "length": 11.1325 },
        { "kind": "arc", "radius": 125.4045, "angle": 0.9138 },
        { "kind": "straight", "length": 90.3427 },
        { "kind": "arc", "radius": -29.7662, "angle": 15.3989 },
        { "kind": "arc", "radius": -12.1796, "angle": 32.9296 },
        { "kind": "arc", "radius": -14.3360, "angle": 35.9696 },
        { "kind": "arc", "radius": -26.7612, "angle": 17.1280 },
        { "kind": "arc", "radius": -62.8691, "angle": 10.0249 },
        { "kind": "straight", "length": 14.8925 },
        { "kind": "arc", "radius": 85.2827, "angle": 2.0155 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 58.4038, "angle": 6.6786 },
        { "kind": "arc", "radius": 30.0985, "angle": 17.1325 },
        { "kind": "arc", "radius": 20.2107, "angle": 34.8569 },
        { "kind": "arc", "radius": 58.4731, "angle": 11.9787 },
        { "kind": "arc", "radius": 81.8716, "angle": 8.8265 },
        { "kind": "arc", "radius": 48.2785, "angle": 9.4942 },
        { "kind": "arc", "radius": 16.6843, "angle": 24.0389 },
        { "kind": "arc", "radius": 11.4412, "angle": 75.1175 },
        { "kind": "arc", "radius": 36.3257, "angle": 12.0854 },
        { "kind": "straight", "length": 24.1297 },
        { "kind": "arc", "radius": -118.9746, "angle": 0.4816 },
        { "kind": "straight", "length": 145.1504 },
        { "kind": "arc", "radius": 63.4878, "angle": 8.1222 },
        { "kind": "arc", "radius": 24.3538, "angle": 16.4685 },
        { "kind": "arc", "radius": 14.1670, "angle": 28.3103 },
        { "kind": "arc", "radius": 12.3171, "angle": 37.2139 },
        { "kind": "arc", "radius": 17.9501, "angle": 19.1517 },
        { "kind": "arc", "radius": 23.3857, "angle": 58.4283 },
        { "kind": "arc", "radius": -104.9259, "angle": 6.3486 },
        { "kind": "arc", "radius": -19.5275, "angle": 38.7176 },
        { "kind": "arc", "radius": -39.7687, "angle": 14.4072 },
        { "kind": "arc", "radius": -74.5822, "angle": 6.1458 },
        { "kind": "arc", "radius": -42.0795, "angle": 13.6161 },
        { "kind": "arc", "radius": -16.5471, "angle": 23.1113 },
        { "kind": "arc", "radius": -11.6523, "angle": 39.3369 },
        { "kind": "arc", "radius": -14.0358, "angle": 37.2584 },
        { "kind": "straight", "length": 15.3679 },
        { "kind": "arc", "radius": 64.0898, "angle": 9.6823 },
        { "kind": "arc", "radius": 14.7865, "angle": 81.0247 },
        { "kind": "arc", "radius": 53.3343, "angle": 12.5608 },
        { "kind": "arc", "radius": 43.7892, "angle": 39.4672 },
        { "kind": "arc", "radius": 12.5892, "angle": 54.7641 },
        { "kind": "arc", "radius": 38.9548, "angle": 15.2833 },
        { "kind": "arc", "radius": -58.6517, "angle": 28.7574 },
        { "kind": "arc", "radius": -11.7162, "angle": 144.0096 },
        { "kind": "arc", "radius": -43.0843, "angle": 15.9680 },
        { "kind": "arc", "radius": -64.3372, "angle": 9.7961 },
        { "kind": "straight", "length": 62.0060 },
        { "kind": "arc", "radius": 132.9446, "angle": 0.4310 },
        { "kind": "arc", "radius": 38.3394, "angle": 11.9555 },
        { "kind": "arc", "radius": 16.8713, "angle": 30.5645 },
        { "kind": "arc", "radius": 13.3686, "angle": 51.6012 },
        { "kind": "arc", "radius": 45.9996, "angle": 8.9355 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -111.1202, "angle": 8.4608 },
        { "kind": "arc", "radius": -88.9165, "angle": 9.8254 },
        { "kind": "arc", "radius": -71.2502, "angle": 15.2018 },
        { "kind": "arc", "radius": 38.6483, "angle": 11.1248 },
        { "kind": "arc", "radius": 12.5988, "angle": 70.8392 },
        { "kind": "arc", "radius": 29.6001, "angle": 15.4853 },
        { "kind": "arc", "radius": 70.1511, "angle": 8.9842 },
        { "kind": "straight", "length": 8.0007 },
        { "kind": "arc", "radius": 48.7480, "angle": 36.4587 },
        { "kind": "arc", "radius": 12.3140, "angle": 27.9175 },
        { "kind": "arc", "radius": 16.6110, "angle": 31.0434 },
        { "kind": "arc", "radius": 35.6166, "angle": 13.9455 },
        { "kind": "straight", "length": 11.8299 },
        { "kind": "arc", "radius": -51.7270, "angle": 8.8613 },
        { "kind": "arc", "radius": -20.9836, "angle": 21.8441 },
        { "kind": "arc", "radius": -12.4799, "angle": 32.1373 },
        { "kind": "arc", "radius": -14.0539, "angle": 51.9955 },
        { "kind": "arc", "radius": -36.5470, "angle": 25.3047 },
        { "kind": "arc", "radius": -31.1949, "angle": 22.8600 },
        { "kind": "arc", "radius": -41.6514, "angle": 16.7050 },
        { "kind": "arc", "radius": -64.1241, "angle": 9.8286 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 77.0328, "angle": 21.3500 },
        { "kind": "arc", "radius": 16.8378, "angle": 27.2225 },
        { "kind": "arc", "radius": 13.2154, "angle": 60.6973 },
        { "kind": "arc", "radius": 15.7352, "angle": 67.2981 },
        { "kind": "arc", "radius": 79.9602, "angle": 7.1655 },
        { "kind": "straight", "length": 11.4033 },
        { "kind": "arc", "radius": -41.7771, "angle": 13.7146 },
        { "kind": "arc", "radius": -16.6347, "angle": 82.6642 },
        { "kind": "arc", "radius": -40.3388, "angle": 15.6240 },
        { "kind": "arc", "radius": -61.1380, "angle": 17.3928 },
        { "kind": "arc", "radius": -44.5320, "angle": 45.3140 },
        { "kind": "arc", "radius": -36.7480, "angle": 16.0456 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -111.5436, "angle": 2.5683 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -97.5813, "angle": 2.3486 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -95.6497, "angle": 2.3961 },
        { "kind": "straight", "length": 14.7615 },
        { "kind": "arc", "radius": 79.0345, "angle": 3.6247 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 76.4078, "angle": 3.7493 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 75.1038, "angle": 3.8144 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": 31.3451, "angle": 16.4511 },
        { "kind": "arc", "radius": 17.9834, "angle": 19.1163 },
        { "kind": "arc", "radius": 25.3736, "angle": 22.5809 },
        { "kind": "arc", "radius": 43.9072, "angle": 14.3956 },
        { "kind": "arc", "radius": 65.2845, "angle": 19.3079 },
        { "kind": "arc", "radius": 25.1877, "angle": 20.4727 },
        { "kind": "arc", "radius": 13.3985, "angle": 25.6577 },
        { "kind": "arc", "radius": 14.0484, "angle": 36.7060 },
        { "kind": "arc", "radius": 27.1469, "angle": 15.9588 },
        { "kind": "straight", "length": 13.1010 },
        { "kind": "arc", "radius": 123.9773, "angle": 1.3864 },
        { "kind": "straight", "length": 46.3870 },
        { "kind": "arc", "radius": -128.6030, "angle": 7.0432 },
        { "kind": "arc", "radius": -111.1204, "angle": 8.1209 },
        { "kind": "arc", "radius": -96.7222, "angle": 8.9334 },
        { "kind": "arc", "radius": -90.6860, "angle": 6.3180 },
        { "kind": "straight", "length": 9.2034 },
        { "kind": "arc", "radius": 128.0951, "angle": 6.6157 },
        { "kind": "arc", "radius": 113.2117, "angle": 6.9247 },
        { "kind": "arc", "radius": 106.5941, "angle": 11.2878 },
        { "kind": "straight", "length": 51.2131 },
        { "kind": "arc", "radius": 44.5350, "angle": 10.9382 },
        { "kind": "arc", "radius": 16.0928, "angle": 100.9716 },
        { "kind": "arc", "radius": 17.8110, "angle": 23.8868 },
        { "kind": "arc", "radius": 15.3306, "angle": 30.4143 },
        { "kind": "arc", "radius": 18.6944, "angle": 24.3946 },
        { "kind": "straight", "length": 8.0000 },
        { "kind": "arc", "radius": -65.9762, "angle": 9.2278 },
        { "kind": "arc", "radius": -15.9124, "angle": 27.9868 },
        { "kind": "arc", "radius": -15.6179, "angle": 41.1691 },
        { "kind": "arc", "radius": -54.6225, "angle": 12.4011 },
        { "kind": "arc", "radius": -52.9898, "angle": 32.2688 },
        { "kind": "arc", "radius": -12.0022, "angle": 36.2527 },
        { "kind": "arc", "radius": -12.8808, "angle": 31.1370 }
      ],
      "features": [
        { "kind": "tabletop", "at": 25.8, "length": 28.7, "height": 2.50 },
        { "kind": "double", "at": 48.4, "height": 1.70, "gap": 10.2, "lip": 5.5 },
        { "kind": "double", "at": 71.0, "height": 1.30, "gap": 8.9, "lip": 5.5 },
        { "kind": "roller", "at": 93.7, "length": 15.9, "height": 0.54 },
        { "kind": "roller", "at": 157.7, "length": 14.3, "height": 0.79 },
        { "kind": "tabletop", "at": 173.0, "length": 19.0, "height": 1.10 },
        { "kind": "tabletop", "at": 224.9, "length": 18.4, "height": 1.10 },
        { "kind": "berm", "at": 248.3, "length": 15.0, "height": 1.70 },
        { "kind": "tabletop", "at": 294.1, "length": 31.3, "height": 3.10 },
        { "kind": "roller", "at": 327.4, "length": 11.3, "height": 0.80 },
        { "kind": "tabletop", "at": 360.6, "length": 22.6, "height": 1.50 },
        { "kind": "stepUp", "at": 393.9, "length": 20.9, "height": 2.00 },
        { "kind": "roller", "at": 427.1, "length": 14.4, "height": 0.75 },
        { "kind": "berm", "at": 561.6, "length": 8.0, "height": 1.70 },
        { "kind": "double", "at": 591.8, "height": 1.50, "gap": 8.8, "lip": 5.5 },
        { "kind": "roller", "at": 704.8, "length": 11.1, "height": 0.79 },
        { "kind": "tabletop", "at": 777.7, "length": 29.5, "height": 3.40 },
        { "kind": "roller", "at": 798.0, "length": 11.6, "height": 0.80 },
        { "kind": "double", "at": 818.3, "height": 1.90, "gap": 9.2, "lip": 5.5 },
        { "kind": "double", "at": 879.3, "height": 3.60, "gap": 14.6, "lip": 6.0 },
        { "kind": "stepUp", "at": 896.9, "length": 19.3, "height": 2.20 },
        { "kind": "roller", "at": 914.5, "length": 13.5, "height": 0.46 },
        { "kind": "whoops", "at": 976.6, "count": 7, "spacing": 4.2, "height": 0.57 },
        { "kind": "double", "at": 995.1, "height": 1.20, "gap": 12.7, "lip": 5.5 },
        { "kind": "tabletop", "at": 1044.5, "length": 20.3, "height": 1.20 },
        { "kind": "tabletop", "at": 1138.0, "length": 16.9, "height": 1.00 },
        { "kind": "double", "at": 1155.4, "height": 1.60, "gap": 8.2, "lip": 5.5 },
        { "kind": "stepUp", "at": 1221.8, "length": 25.2, "height": 1.90 },
        { "kind": "roller", "at": 1286.7, "length": 14.8, "height": 0.60 },
        { "kind": "tabletop", "at": 1361.0, "length": 24.4, "height": 2.50 },
        { "kind": "roller", "at": 1386.9, "length": 14.4, "height": 0.82 },
        { "kind": "roller", "at": 1412.8, "length": 13.2, "height": 0.58 },
        { "kind": "tabletop", "at": 1479.3, "length": 24.1, "height": 1.10 },
        { "kind": "double", "at": 1550.0, "height": 3.60, "gap": 16.4, "lip": 6.0 },
        { "kind": "roller", "at": 1593.2, "length": 14.6, "height": 0.64 },
        { "kind": "tabletop", "at": 1636.4, "length": 17.3, "height": 1.20 },
        { "kind": "tabletop", "at": 1679.6, "length": 18.2, "height": 1.40 },
        { "kind": "roller", "at": 1722.8, "length": 11.5, "height": 0.77 },
        { "kind": "whoops", "at": 1820.6, "count": 7, "spacing": 5.0, "height": 0.75 },
        { "kind": "tabletop", "at": 1862.6, "length": 22.5, "height": 1.60 }
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
