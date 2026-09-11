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
    /// How raced the ground arrives, 0 to 1.
    ///
    /// A generated track used to ship one state of ground: fully raced. The corner grooves,
    /// the braking washboard and the acceleration chop were all baked into the terrain at
    /// full depth, and the game then went on deforming from there — so every session began
    /// at the end of the third moto and only ever got worse. There was nowhere for a track
    /// to *become* rough, because it already was.
    ///
    /// This is the dial between the two. At 0 the ground is freshly prepped: the shapes are
    /// there but the wear is not, and the deformable stack underneath is at full depth so a
    /// session cuts its own lines. At 1 it is a Sunday afternoon — everything baked in, and
    /// the stack thinned to match, because material already cut into the terrain is material
    /// the ground no longer has to give. The sum of the two is near enough constant, which
    /// is what stops a heavily raced track digging itself to pieces.
    ///
    /// Defaults to a track that has seen a session rather than to either end.
    #[serde(default = "default_wear")]
    pub wear: f32,
}

/// Half-worn: shapes settled, grooves started, most of the ground still to give.
pub(crate) fn default_wear() -> f32 {
    0.55
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
    /// How many banks and spoil hills the venue has around the track, and how tall the
    /// tallest of them stands.
    ///
    /// What separates a real plot from a plain of noise is not roughness, it is these.
    /// Indiana's ground climbs twelve metres in twenty-five at the ninety-ninth percentile
    /// and twenty at the worst; noise tuned to its median exactly reaches 3.6 and 6.0. The
    /// steepest of it sits 40–120 m from the riding line — the banks people stand on, not the
    /// cut and fill either side of the track.
    #[serde(default)]
    pub landforms: u32,
    #[serde(default = "default_landform_height")]
    pub landform_height: f32,
}

fn default_landform_height() -> f32 {
    12.0
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
            landforms: 0,
            landform_height: default_landform_height(),
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

/// The steepest face a jump is allowed, degrees — measured at the lip, which is where an
/// arc is steepest.
///
/// Twenty-seven, which is what a published takeoff measures at its ninetieth percentile.
///
/// Thirty is a different number and it was the one here: the ceiling across all ten tracks,
/// the steepest faces on the steepest of them. Now that the face is an arc the ceiling lands
/// exactly on the lip, so every jump tall enough for the angle to bind — anything over about
/// a metre — was built with the steepest lip in the corpus. A generated lap measured 17.8° at
/// the median face against Indiana's 12.0, which is what "the jumps are too big" turns out to
/// mean: not their height, which already matches, but that every one of them is as abrupt as
/// the worst one on a real track.
/// Thirty-eight, and it is a ceiling that now rarely binds — which is the point.
///
/// Twenty-seven was read off the *terrain* statistic: Indiana's faces measure 27.4° at the
/// ninetieth percentile, sampled off the ground. That is not the same quantity as the angle a
/// builder builds to, and taking it as one made every big face too long. Sized by the half
/// angle, a 27° lip puts a 12.5 m ramp under a 3 m jump — a mean gradient of 13.5°, where
/// every real source puts a built take-off between 2:1 and 3:1, or 26.6° down to 18.4° of
/// mean. See `docs/tracks/real-track-corpus.md` §4.1: the ratio a builder quotes describes the
/// *mean* of a concave face, and its lip runs about 1.8x steeper.
///
/// At 38 the angle stops lengthening faces past [`JUMP_FACE_MIN_M`] for anything under about
/// 3.1 m, so the floor governs the whole realistic range and the angle is what the comment on
/// [`TABLETOP_DECK_M`] always said it should be: a ceiling for the tall ones, not a target for
/// all of them. A 3 m jump comes out on the floor at 9 m — 18.4° of mean, which is 3:1 exactly.
///
/// The tension worth knowing: our lip is now geometrically steeper than the 27.4° Indiana
/// *measures*, because a windowed terrain slope under-reads a real lip. If a generated lap
/// starts measuring past Indiana at the ninetieth, this is the number that did it.
pub const JUMP_FACE_DEG: f32 = 38.0;

// A straight lip is NOT built here, and the reason is geometric rather than an oversight.
//
// The terrain-park literature builds the last ~2 m of a take-off straight, so the ground is
// not still turning under the wheel at the moment of release — Petrone's constant-EFH jump
// states it outright. Adding that here was tried and backed out.
//
// [`DoubleShape::height_at`] trims the crown off the top of a face and rescales what is left,
// and that trick is exact *only* because the face is a pure arc: a chord cut off an arc keeps
// rise and run in the same `tan(sweep/2)` ratio, so the trimmed face is still the face `up`
// describes and the crown still meets it tangentially. Splice a straight into the top and the
// identity fails — `sin(u)·tan(u) != 1 − cos(u)` — so the crown joins a face that is no longer
// at the crown's own angle and the ramp comes out steeper than it states. Measured: a 4 m jump
// over a 12 m gap stood at 39.1° against a stated 38.
//
// Doing it properly means rebuilding the crown construction so it does not lean on
// self-similarity. That is worth doing and it is not a one-line change. Note the crown already
// rounds the crest over `0.9·h` of radius, which on a 4 m jump is more ground than the 2 m
// straight would have occupied — so the release is less abrupt than the bare arc suggests.
// See `docs/tracks/real-track-corpus.md` §6.2.


/// The gentlest face — the one a landing gets. Published landings measure 19.0° at the
/// ninetieth against a takeoff's 27.0: a built takeoff is short because that is what throws
/// you, and the landing is long because that is what catches you.
pub const JUMP_LANDING_DEG: f32 = 19.0;

/// The steepest a face nobody rides may stand, degrees.
///
/// The back of a takeoff lip and the wall a landing presents to the gap are cut faces: dirt
/// dumped and left at the angle it holds, not ground a blade shapes for a wheel. So they are
/// steeper than a ridden face and — this is the part that matters — much shorter, because
/// what a rider has to clear is measured crest to crest and both of them are in it.
///
/// Sizing them like ramps is what made a 3.6 m double 41 m of air across a 14.6 m gap, and
/// the worked example's own jumps stopped being jumpable the moment the faces grew.
pub const JUMP_CUT_DEG: f32 = 38.0;

/// The shortest deck a tabletop may have, metres.
///
/// A tabletop is the safe jump precisely because it has a top: land anywhere along it and you
/// have landed. Its deck was whatever the two faces left of the stated length, and the faces
/// are sized from the height — so on the four biggest tables of the worked example, 2.5 m and
/// up, the faces ate the whole length and the two ramps met at a point. That is a double, in
/// the only way a rider can tell one, and it is why a lap that lists fifteen tabletops rides
/// as though it has none.
///
/// Six because published decks run six to twelve metres across. It is a floor and not a
/// target: a program that asks for a longer top gets it, and one that asks for a jump too
/// short for its own height gets a longer footprint instead of a peak.
pub const TABLETOP_DECK_M: f32 = 6.0;

/// The shortest a takeoff may be however small the jump.
///
/// This is what gives a lap a *spread* of faces instead of one. The angle is a ceiling, and a
/// ceiling alone makes every jump that reaches it identical: at a four-metre floor the angle
/// bound on anything over 1.07 m, so all but the smallest ground on a track stood at the same
/// steepest-allowed lip. Published tracks are not like that — Indiana's faces run 12.0° at the
/// median against 27.4 at the ninetieth, because a small jump there is a long low rise and
/// only the big ones are abrupt.
///
/// Nine metres puts the crossover at 2.16 m. Under it the floor governs and the lip angle
/// falls with the height — 12.7° at a metre, 18.9° at a metre and a half, 24.9° at two — and
/// over it the angle takes over at 27. That is the published spread, from the same two numbers.
pub const JUMP_FACE_MIN_M: f32 = 9.0;

/// The shortest a landing may be, metres. Longer than a takeoff, for the reason
/// [`JUMP_LANDING_DEG`] is gentler than [`JUMP_FACE_DEG`]: it is the side that catches you.
pub const JUMP_LANDING_MIN_M: f32 = 12.0;

/// The shortest the back of a lip may be — the short face nobody lands on.
///
/// Four, where it has always been. It is the one face a takeoff floor must not reach: the back
/// of a lip is short on purpose, and stretching it to nine turns every double into a hump.
pub const JUMP_CUT_MIN_M: f32 = 4.0;

/// The shape of a jump's face: a circle's quadrant, not a smoothstep.
///
/// This is what a machine actually leaves. A smoothstep is flat at both ends and steepest
/// halfway up, so it rounds the lip off — and the lip is the one part of a takeoff that has
/// to be an edge. Sized to peak at thirty degrees it spends its steepest metre in the middle
/// of the ramp and arrives at the top already flattening, which is why our jumps rode like
/// rollers however tall they were built.
///
/// A blade pushing dirt up into a lip sweeps an arc: tangent to the ground where the face
/// starts, and steepest where it ends. `t` runs 0 at the foot to 1 at the lip and the return
/// is the fraction of the jump's height, so the curve is concave all the way up and the
/// steepest ground on it is the last of it.
///
/// Run backwards it is the same curve convex — steep off the crest and flattening into the
/// ground — which is a landing, and the reason one function serves both. A face and the
/// landing that answers it are the same arc ridden in opposite directions.
///
/// `sweep` is how far round the circle the face goes, in radians, which is twice the angle it
/// would average: an arc that ends at θ has risen `tan(θ/2)` for every metre it ran.
pub fn face_arc(t: f32, sweep: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    // Below about a degree the arc and its own chord differ by less than the grid can hold,
    // and the divide below loses all its significance. A quadratic is the arc's own limit
    // there — still tangent at the foot, still steepest at the lip.
    if !sweep.is_finite() || sweep <= 1e-3 {
        return t * t;
    }
    let (sin_s, cos_s) = (sweep.sin(), sweep.cos());
    // Where along the sweep this point sits, from how far along the chord it is.
    let phi = (t * sin_s).clamp(-1.0, 1.0).asin();
    (1.0 - phi.cos()) / (1.0 - cos_s)
}


/// How far a face has to run to reach `height` without standing steeper than `deg` at its lip.
///
/// The half-angle, because the shape is an arc: a quadrant that ends at θ has risen
/// `tan(θ/2)` per metre of run, so a 30° face is 3.73 times as long as it is tall. The old
/// shape was a smoothstep and carried a 1.5 for the same reason — a smoothstep peaks at half
/// again its average — and this replaces that fudge rather than joining it. Faces come out
/// about 44% longer, which is the length a built one actually is.
pub fn face_run(height: f32, deg: f32, min_m: f32) -> f32 {
    let half = (deg * 0.5).to_radians().tan().max(1e-4);
    (height.abs() / half).max(min_m)
}

/// The sweep angle a face of this run and rise actually turns through, radians.
///
/// Derived from the two lengths rather than from [`JUMP_FACE_DEG`], so a face that came out
/// longer than the angle asked for — because [`JUMP_FACE_MIN_M`] caught it, or because a
/// programme stated its own lip — is a shallower arc rather than the same arc stretched. The
/// curve is always tangent to the ground it leaves.
pub fn face_sweep(height: f32, run: f32) -> f32 {
    2.0 * (height.abs() / run.max(1e-3)).atan()
}

/// A double's four faces, in metres, for a given height.
///
/// One definition, used by both the shape and by [`Feature::length`], because they have to
/// agree about where the feature ends.
///
/// Four rather than two, and that is the whole point. The old pair made a double a mirror of
/// itself: the same ramp up either side of the gap, so the face a rider cases into stood as
/// steep as the one that launched them. [`JUMP_LANDING_DEG`] had been measured off published
/// ground and was used by nothing but the tabletop.
///
/// - `ramp` — what you ride up. Concave, steepest at the lip, at least as long as the
///   programme's own `lip` if it asked for a longer one.
/// - `back` — the back of that lip, falling into the gap. A cut face at [`JUMP_CUT_DEG`]:
///   short and steep, because it is dumped dirt rather than a shaped ramp and because every
///   metre of it is a metre the rider has to carry.
/// - `face` — out of the gap to the landing's crest. Also a cut face: it is the wall you hit
///   coming up short, and making it gentle would turn the gap into something you ride through.
/// - `run` — the far side of the landing, which is the ground that actually catches you.
///   Long and shallow, at the landing angle.
pub struct DoubleFaces {
    pub ramp: f32,
    pub back: f32,
    pub face: f32,
    pub run: f32,
}

impl DoubleFaces {
    /// The whole footprint, gap included.
    pub fn total(&self, gap: f32) -> f32 {
        self.ramp + self.back + gap + self.face + self.run
    }
}

pub fn double_faces(height: f32, lip: f32) -> DoubleFaces {
    let ramp = face_run(height, JUMP_FACE_DEG, JUMP_FACE_MIN_M);
    // Dumped, not bladed: a smoothstep rather than an arc, so the half-angle relation does
    // not apply and the run is `1.5 h / tan(deg)` — the smoothstep's own peak-to-average.
    let cut = (1.5 * height.abs() / JUMP_CUT_DEG.to_radians().tan()).max(JUMP_CUT_MIN_M);
    DoubleFaces {
        // A short lip on a tall jump is a wall whichever side of it you are on.
        ramp: lip.max(ramp),
        back: cut,
        face: cut,
        run: face_run(height, JUMP_LANDING_DEG, JUMP_LANDING_MIN_M),
    }
}

/// How far the ground turns over a lip and over a landing's crest: a radius in metres per
/// metre of jump height, and the least it may be.
///
/// A crest built without one is a corner. The ramp arrived at its lip angle and the back of
/// the lip left at [`JUMP_CUT_DEG`], so the profile changed slope by sixty degrees between two
/// samples and the jump read as a triangle with a point on top. Nothing builds that: a blade
/// has a width, and the last of the dirt pushed over a lip settles on a radius.
///
/// Per height, because the crown is part of the jump rather than a fillet applied to it — a
/// four-metre lip turns over more ground than a one-metre one — with a floor, so a low rise
/// gets a rounded top rather than a scaled-down point.
pub const JUMP_CROWN_R: (f32, f32) = (0.9, 1.5);

/// The same for the valley between the two jumps: the ground the takeoff's back falls into and
/// the landing's face climbs out of.
///
/// Rounder than a crest, because it is a floor rather than an edge and everything in it was
/// pushed there rather than shaped. The gap used to be a flat pan the two cut faces dropped
/// onto, which put a crease across the bottom of every double at exactly the place a rider who
/// comes up short arrives.
///
/// The radius is kept whatever the gap, and it is the *floor* that gives way instead — see
/// [`double_shape`]. The two jumps stand on the ground between them rather than having a slot
/// cut down to grade to satisfy a rule about where the bottom is.
pub const JUMP_TROUGH_R: (f32, f32) = (2.0, 3.0);

/// The floor a valley wants at the bottom of it, metres.
///
/// The bottom of a double is where a rider who came up short arrives, so what is worth having
/// there is ground to land on rather than the point two faces meet at. A bike length: fall to
/// grade if there is room to do it and still leave that much floor, and stop higher if not.
pub const JUMP_PAN_M: f32 = 2.0;

/// How much of its own height a double falls between its crests. The rest is the pad the pair
/// stands on.
///
/// **The gap is never on the ground.** It used to be dug to grade by definition — a double is
/// a pair of piles with nothing between them — and that is a drawing-board double, not a built
/// one. Dirt for a rhythm section is pushed into a bank and the jumps are cut out of it, so the
/// bottom between two of them is a dip in that bank rather than the field it was built on. Cut
/// to grade it reads as a trench, and the rider who came up short arrives in the bottom of one.
///
/// Three quarters, so the fall between the crests is still nearly the whole jump and the floor
/// stands a quarter of it up. Less than the ground allows in a tight gap still wins: this is a
/// ceiling on the fall, not a target for it.
pub const JUMP_VALLEY_FALL: f32 = 0.75;

/// A double's profile: two crowned crests with a circular valley between them.
///
/// Every part of it is a circular arc or the straight joining two of them, and no two meet at
/// an angle. The ramp leaves the ground tangent, turns over the lip on [`JUMP_CROWN_R`], falls
/// at [`JUMP_CUT_DEG`], rounds into the gap floor on [`JUMP_TROUGH_R`], and does the same in
/// reverse up the far side.
///
/// The crests still stand exactly `height` above grade at exactly the distances
/// [`double_faces`] puts them, and the floor still touches grade — so what the rider is asked
/// to clear has not moved. What changed is that the shape between those points is one a
/// machine could have left.
pub struct DoubleShape {
    pub faces: DoubleFaces,
    height: f32,
    gap: f32,
    /// The ramp's own sweep, and the landing's. Both are what [`face_sweep`] makes of the run
    /// the face was given, so a face held open by [`JUMP_FACE_MIN_M`] stays the shallow arc it
    /// is rather than being bent to the ceiling.
    up: f32,
    land: f32,
    /// What the inner faces actually stand at. [`JUMP_CUT_DEG`] wherever the height needs it,
    /// and less on a jump whose crown and valley shed the whole height between them — a
    /// ceiling rather than a target, which is how the ridden faces read theirs.
    inner: f32,
    crown: f32,
    trough: f32,
    /// The straight cut face between the crown and the valley, each side of the gap.
    straight: f32,
    /// How high the bottom of the valley sits above the ground the jumps stand on. Zero
    /// wherever there is room for it, which is most doubles.
    floor: f32,
    /// What is left of the gap floor once the valley has taken its radius.
    flat: f32,
}

/// The shape of a double, built from the same four faces [`double_faces`] reports.
///
/// The rounding is fitted *inside* those faces rather than added to them, so the footprint
/// [`Feature::length`] states is untouched and the crests stay where every other part of the
/// generator — the speed model, the ruts, the scenery — expects to find them.
///
/// **The floor gives way before the radius does, and it never reaches the ground.** The fall
/// between the crests is [`JUMP_VALLEY_FALL`] of the jump's height, or less where the span
/// cannot pay for that much and still leave a [`JUMP_PAN_M`] of floor at the bottom. So the
/// radii are what the height asks for, the fall is the smaller of what the jump wants and what
/// the span allows, and the floor sits at the difference — a dip in the ground the pair stands
/// on, which is what a rhythm section is cut out of.
pub fn double_shape(height: f32, gap: f32, lip: f32) -> DoubleShape {
    let faces = double_faces(height, lip);
    let h = height.abs();
    let cut = JUMP_CUT_DEG.to_radians();
    let (k, tan) = (1.0 - cut.cos(), cut.tan().max(1e-4));
    let half = (cut * 0.5).tan().max(1e-4);
    // Crest to crest: the whole inside of the double, and all the ground the rounding has to
    // fit into.
    let span = faces.back + gap + faces.face;

    // A crown and a valley shed height between them at the cost of ground, and `x` — the two
    // radii added together — is all the arithmetic below needs to know about how much of each.
    let crown = (JUMP_CROWN_R.0 * h).max(JUMP_CROWN_R.1);
    let trough = (JUMP_TROUGH_R.0 * h).max(JUMP_TROUGH_R.1);
    let x = crown + trough;
    // How far the ground can fall away from the crests and come back, in the span left once
    // the pan at the bottom has had its share. Two branches of one curve: past `2·x·sin(cut)`
    // there is room for a straight cut face between the crown and the valley, and under it
    // there is not and the faces stand shallower instead.
    let room = (span - JUMP_PAN_M).max(0.0);
    let drop = if room >= 2.0 * x * cut.sin() {
        // Solve `2·(x·tan(cut/2) + d/tan cut) = room` for the fall `d`.
        (room * 0.5 - x * half) * tan
    } else {
        // All arc: solve `2·√(d·(2x − d)) = room`.
        x - (x * x - room * room * 0.25).max(0.0).sqrt()
    }
    .clamp(0.0, JUMP_VALLEY_FALL * h);
    let (inner, straight) = if x * k >= drop {
        // The crown and the valley take the whole fall on their own, so the face between them
        // is a point and the angle it passes through is whatever that costs.
        ((1.0 - drop / x.max(1e-4)).clamp(-1.0, 1.0).acos(), 0.0)
    } else {
        (cut, (drop - x * k) / tan)
    };
    DoubleShape {
        up: face_sweep(h, faces.ramp),
        land: face_sweep(h, faces.run),
        floor: h - drop,
        flat: (span - 2.0 * (x * inner.sin() + straight)).max(0.0),
        crown,
        trough,
        faces,
        height,
        gap,
        inner,
        straight,
    }
}

impl DoubleShape {
    /// Metres above grade, `u` metres into the feature.
    pub fn height_at(&self, u: f32) -> f32 {
        let h = self.height.abs();
        let (rc, rt, inner) = (self.crown, self.trough, self.inner);
        let ramp = self.faces.ramp;
        // Both crests keep their nominal place with the crown's apex on them: a crown eats
        // into the two faces that meet there rather than moving or lowering the point they
        // meet at.
        let crest = ramp + self.faces.back + self.gap + self.faces.face;
        let end = crest + self.faces.run;
        let x1 = ramp - rc * self.up.sin();
        let x2 = ramp + rc * inner.sin();
        let x3 = x2 + self.straight;
        let x4 = x3 + rt * inner.sin();
        let x5 = x4 + self.flat;
        let x6 = x5 + rt * inner.sin();
        let x7 = x6 + self.straight;
        let x8 = crest + rc * self.land.sin();
        // How far a circle of radius `r` has fallen `d` along from where it is level.
        let round = |r: f32, d: f32| {
            let a = (d / r.max(1e-4)).clamp(-1.0, 1.0).asin();
            r * (1.0 - a.cos())
        };
        let y = if u <= x1 {
            // The ramp, less what the crown took off the top of it. The crown's own rise and
            // run are in the same ratio as the chord it is trimmed from, so what is left is
            // still the face `up` describes and still tangent to the ground it leaves.
            (h - round(rc, rc * self.up.sin())) * face_arc(u / x1.max(1e-4), self.up)
        } else if u <= x2 {
            h - round(rc, u - ramp)
        } else if u <= x3 {
            h - round(rc, rc * inner.sin()) - (u - x2) * inner.tan()
        } else if u <= x4 {
            self.floor + round(rt, x4 - u)
        } else if u <= x5 {
            self.floor
        } else if u <= x6 {
            self.floor + round(rt, u - x5)
        } else if u <= x7 {
            self.floor + round(rt, rt * inner.sin()) + (u - x6) * inner.tan()
        } else if u <= x8 {
            h - round(rc, u - crest)
        } else {
            let run = (end - x8).max(1e-4);
            (h - round(rc, rc * self.land.sin())) * face_arc((end - u) / run, self.land)
        };
        y.max(0.0) * self.height.signum()
    }
}

/// A tabletop's ramp up, its flat top and its ramp down, in metres.
///
/// The ramps used to be fixed fractions of the feature's length — 27% up and 44% down — so a
/// short tabletop got a short ramp however tall it was asked to be, and how steep it came out
/// depended on nothing but the ratio of the two numbers. A 3 m tabletop 16 m long ramped at
/// 39°. Sized from the height and an angle instead, the way a double's faces are.
///
/// The stated length is what the *top* is measured against: the ramps are added to it, so a
/// tabletop's footprint is longer than the number asked for and [`Feature::length`] reports
/// the whole thing.
pub fn tabletop_faces(height: f32, length: f32, lip: f32) -> (f32, f32, f32) {
    // Whichever is longer: the angle's, or the fraction of the stated length the ramps used
    // to be. The angle alone makes a *short* jump steeper than it was — at 30° a one-metre
    // tabletop gets a 2.6 m ramp where 27% of a 22 m length gave it 5.9 m — which is the same
    // way round as it bit on the double. The angle is a ceiling for the tall ones, not a
    // target for all of them.
    // Sized by the angle alone. Keeping the old fractions as a floor made the ramps grow
    // with the stated length, so asking for a *longer* table bought ramp rather than deck:
    // a 3.6 m tabletop asked for at 49 m got 35 m of ramp and a 14 m top, which from the
    // seat is a long rounded hill with a crest on it and not a table at all. A table's size
    // is its deck.
    let up = face_run(height, JUMP_FACE_DEG, JUMP_FACE_MIN_M).max(lip);
    let down = face_run(height, JUMP_LANDING_DEG, JUMP_LANDING_MIN_M);
    // Whatever the asked-for length has left once the faces are in it — but never less than a
    // deck. The deck wins and the footprint grows; the other way round, keeping the length by
    // steepening the faces to fit a top inside it, is the same jump built worse.
    let top = (length - up - down).max(TABLETOP_DECK_M);
    (up, top, down)
}

/// How tall the finish jump is built, metres.
///
/// The jump the lap ends and begins on, and on a national it is one of the biggest on the
/// track — a long tabletop on the main straight with the line painted past its landing. The
/// range is the top of the published spread rather than the middle of it: this is the one
/// jump a track is photographed on.
// Full size: at three quarters it rode too small for the one jump a track is known by.
pub const FINISH_JUMP_M: (f32, f32) = (2.4, 3.0);

/// The longest deck a finish jump gets, metres. Published tabletop decks run six to twelve,
/// and the finish one is at the long end because it is the one everybody lands on.
// Past the published twelve: at 3 m, the regulated ceiling, the finish jump still rode small, and
// a longer deck is the way to make it bigger without making it taller.
pub const FINISH_DECK_MAX_M: f32 = 20.0;

/// How far the finish jump's take-off runs, metres. Longer and gentler than the angle gives a
/// 3 m face on its own (9 m, 37 degrees at the lip): at 13 it leaves at 26.
pub const FINISH_FACE_M: f32 = 13.0;

/// Bare ground off the last corner before the finish jump's face, metres. A takeoff at the
/// corner exit is a takeoff nobody has any drive at.
pub const FINISH_RUNUP_M: f32 = 18.0;

/// Ground past the landing before the straight runs out, metres — where the line is painted
/// and where a rider gets back on the brakes for the next corner.
pub const FINISH_RUNOUT_M: f32 = 10.0;

/// How far past the landing the line itself goes, metres. A finish line marks the ground a
/// rider comes down on, so it sits just off the end of the ramp rather than on it.
pub const FINISH_LINE_PAST_M: f32 = 4.0;

/// The whole footprint of a finish jump of this height with this deck, metres.
///
/// A tabletop's ramps are the longer of an angle and a fraction of the stated length, so the
/// length and the faces define each other. Solved by iterating: the fraction is 0.44 at
/// worst, so it converges geometrically and eight passes is far past the millimetre.
pub fn finish_jump_length(height: f32, deck: f32) -> f32 {
    let deck = deck.max(TABLETOP_DECK_M);
    let mut len = height.abs() + deck;
    for _ in 0..8 {
        let (up, _, down) = tabletop_faces(height, len, FINISH_FACE_M);
        len = up + deck + down;
    }
    len
}

/// The shortest straight a start will fit beside, metres.
///
/// A motocross start is a gate row and a sprint at the first turn, all of it in a line,
/// because forty gates cannot be laid round a bend. The lap needs a straight this long for
/// the start to run alongside.
pub const START_STRAIGHT_M: f32 = 60.0;

/// How far off the lap the gate row stands, metres.
///
/// Measured off six published tracks, whose start lines are carried in their own height
/// files: Indiana 37.9, SandPoint 36.4, Briarcliff 41.0, I40 47.2, SFDR 48.9, Smokey Pines
/// 34.0. The start is *not* part of the lap on any of them — it is a spur that runs beside it
/// and merges in, so a rider on a flying lap never crosses the gates.
pub const START_OFFSET_M: f32 = 40.0;

/// How long the gate straight is before it starts turning in, metres.
///
/// Published start lines run 79–91 m of straight before their first corner, and 67–208 m all
/// told. Shorter than the middle of that on purpose: ridden, 85 m of sprint and 90 m of
/// turn-in is a long way to the first corner, and the whole point of a start straight is that
/// it ends at one.
pub const START_SPRINT_M: f32 = 80.0;

/// How far the start straight is angled towards the lap, degrees. Over the sprint it closes
/// about a fifth of the offset, which leaves one corner to do the rest.
pub const START_CONVERGE_DEG: f32 = 10.0;

/// Tighter than this and an arc is turn one rather than a bend the straight is drifting
/// through. Published first corners run 10–46 m; this sits above them so it catches the whole
/// corner rather than only its apex.
pub const TURN_ONE_RADIUS_M: f32 = 60.0;

/// Half the width of the start pad, metres: the gate row plus a margin.
///
/// Forty gates at 1.2 m is 48 m across, and the pad has to hold it. Stated here rather than
/// in the synthesiser because the *layout* needs it — a start line whose centreline clears
/// the lap by fifteen metres still lays its pad straight over it.
pub const START_FAN_HALF_M: f32 = 27.0;

/// How much a start line is expected to turn on its way onto the lap. Published ones sweep
/// 100–180°, and what that buys is a pack that arrives *in* the corner.
pub const TURN_ONE_SWEEP_DEG: f32 = 110.0;

/// The tightest the merge back onto the lap may turn, metres. Published start lines join
/// through 10–46 m radii; this is the floor, and a tight one keeps turn one close to the
/// gates rather than a long sweep away from them.
pub const START_MERGE_RADIUS_M: f32 = 15.0;

/// The start straight: where the gate row stands, and the line from it into the lap.
///
/// Its own line, not a stretch of the lap. `tracked -merge` takes it as `sa` beside the
/// racing line's `cl`, and every published track carries one: a straight off to the side of
/// the circuit, then a corner or two that feeds into it, 67–208 m all told.
#[derive(Clone, Debug)]
pub struct StartLine {
    /// The gate row's pose: the line begins where the gates stand.
    pub start: Start,
    pub segments: Vec<Segment>,
    /// Metres round the lap where it merges in.
    pub joins_at: f32,
    /// Which side of the lap it stands on: +1 is the rider's right.
    pub side: f32,
}

impl StartLine {
    pub fn length(&self) -> f32 {
        self.segments.iter().map(|s| s.length()).sum()
    }
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

/// A corner, and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerRun {
    /// Metres round the lap the corner starts and ends at.
    pub start_m: f32,
    pub end_m: f32,
    /// Unsigned degrees turned through.
    pub degrees: f32,
    /// The tightest radius anywhere in it, metres.
    pub tightest_m: f32,
    /// How many arcs the builder used. A published corner is 9-19 of them; one arc is a
    /// compass sweep, and it rides like one.
    pub arcs: usize,
}

/// The same corners [`turns`] counts, carrying where each one is.
///
/// `turns` answers "how many corners, how tight" and is used to judge a whole lap. This
/// answers "and where", which is what anything that has to go and look at the ground there
/// needs — and what the corner atlas is built on.
pub fn corner_runs(segments: &[Segment]) -> Vec<CornerRun> {
    let mut out: Vec<CornerRun> = Vec::new();
    let mut at = 0.0f32;
    let mut cur: Option<(f32, CornerRun)> = None; // way, run
    for seg in segments {
        let end = at + seg.length();
        match *seg {
            Segment::Arc { radius, angle, .. } if radius != 0.0 && angle.abs() >= CORNER_DEG => {
                let way = radius.signum();
                match cur {
                    Some((w, ref mut r)) if w == way => {
                        r.end_m = end;
                        r.degrees += angle.abs();
                        r.tightest_m = r.tightest_m.min(radius.abs());
                        r.arcs += 1;
                    }
                    other => {
                        if let Some((_, r)) = other {
                            out.push(r);
                        }
                        cur = Some((
                            way,
                            CornerRun {
                                start_m: at,
                                end_m: end,
                                degrees: angle.abs(),
                                tightest_m: radius.abs(),
                                arcs: 1,
                            },
                        ));
                    }
                }
            }
            _ => {
                // Same break as `turns`: anything straight and longer than a nudge ends it.
                if seg.length() > 8.0 {
                    if let Some((_, r)) = cur.take() {
                        out.push(r);
                    }
                }
            }
        }
        at = end;
    }
    if let Some((_, r)) = cur {
        out.push(r);
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
    Tabletop {
        at: f32,
        length: f32,
        height: f32,
        /// How far the take-off runs, metres, when the program wants it longer and gentler
        /// than the angle would make it. Zero leaves it to the angle.
        #[serde(default, skip_serializing_if = "is_zero")]
        lip: f32,
    },
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

fn is_zero(v: &f32) -> bool {
    *v == 0.0
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

    /// Where it sits, to move it. Rotating the lap's start moves everything placed by
    /// distance round it, and a feature is placed by distance round it.
    pub fn at_mut(&mut self) -> &mut f32 {
        match self {
            Feature::Tabletop { at, .. }
            | Feature::Double { at, .. }
            | Feature::Roller { at, .. }
            | Feature::Whoops { at, .. }
            | Feature::StepUp { at, .. }
            | Feature::Berm { at, .. }
            | Feature::Rut { at, .. }
            | Feature::Custom { at, .. } => at,
        }
    }

    /// How much of the lap it occupies.
    pub fn length(&self) -> f32 {
        match self {
            // The ramps are sized from the height, so the footprint is longer than the
            // stated length and this has to say so — the profile is only written as far as
            // `at + length`, and anything past it is cut off into a step.
            Feature::Tabletop { height, length, lip, .. } => {
                let (up, top, down) = tabletop_faces(*height, *length, *lip);
                up + top + down
            }
            Feature::Roller { length, .. }
            | Feature::StepUp { length, .. }
            | Feature::Berm { length, .. }
            | Feature::Rut { length, .. }
            | Feature::Custom { length, .. } => *length,
            // Ramp, lip's back, gap, landing face, landing run-off. The lengths come from
            // `double_faces` rather than being written out again here: they used to be, and
            // when the faces were lengthened this went on reporting the old figure, so the
            // profile was written up to a point eight metres short of where the shape
            // actually ended and the last ramp was cut off into a step.
            Feature::Double {
                height, gap, lip, ..
            } => double_faces(*height, *lip).total(*gap),
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
/// Walked, not written. [`crate::tracklayout::draw`] grows a lap piece by piece out of a 470 m
/// plot, refusing any piece that would put the track through ground it has already used, and
/// docks it back onto its own start with a Dubins path — so it closes to 0.00 m and never
/// crosses itself, without either being asserted. Seed 24, then left exactly as the repair
/// pass leaves it, which is why loading it has nothing to say about it.
///
/// It measures like a published national, because that is what the walk is calibrated against
/// — see `scripts/track-survey.py`, which reads these numbers off a track's own `.trh`:
///
/// ```text
///                          here    Indiana   Southwick
///   lap                   1966 m     2170 m     2217 m
///   corners            19 (9.7)   16 (7.4)   18 (8.1)  per km
///   a corner's angle        142°       159°       166°
///   apex radius           12.1 m     10.4 m     11.9 m
///   ground per corner       64 m       68 m       74 m
///   run between them        30 m       27 m       30 m
///   turning share           0.71       0.63       0.72
/// ```
///
/// 81 segments, 75 of them arcs, 2576° of turning gross. It begins on its longest straight,
/// all 123 m of it — the gate row, the finish line and the run at turn one all sit there, and
/// a lap that opens on a corner puts forty gates round a bend. Twenty-seven features, sixteen
/// of them over 2.2 m and the rest small ground, none over the 3 m two federations write.
///
/// This is the schema's own test. It is parsed by the test suite, synthesised, and measured
/// against published tracks, so it cannot drift away from what the code accepts. Remake it
/// with `BASE_SEED=<n> cargo test --bin mxb-app -- --ignored --nocapture emit_base_track`.
pub const EXAMPLE: &str = r#"{
      "name": "Corpus National",
      "author": "Frost's Mod Manager",
      "location": "Generated",
      "width": 17.915,
      "terrain": {
        "sizeX": 470, "sizeZ": 470, "samples": 2049, "scale": 22, "surface": "soil",
        "relief": { "amplitude": 6.1914, "wavelength": 435.8793, "seed": 24, "texture": 0.085, "tilt": 14.009, "tiltAngle": 62.7413, "landforms": 2, "landformHeight": 2.6008 }
      },
      "start": { "x": 256.1954, "z": 152.6315, "angle": 0 },
      "segments": [
        { "kind": "straight", "length": 122.9492 },
        { "kind": "arc", "radius": -133.2064, "angle": 14.839 },
        { "kind": "arc", "radius": -53.4614, "angle": 48.8316 },
        { "kind": "arc", "radius": -11.244, "angle": 60.0766 },
        { "kind": "arc", "radius": -28.0832, "angle": 35.8188 },
        { "kind": "arc", "radius": 1958.4766, "angle": 0.8702 },
        { "kind": "arc", "radius": 32.2781, "angle": 45.6807 },
        { "kind": "arc", "radius": 12.7373, "angle": 72.4345 },
        { "kind": "arc", "radius": 35.9925, "angle": 23.9197 },
        { "kind": "arc", "radius": 1993.6255, "angle": 0.4758 },
        { "kind": "straight", "length": 55.4307 },
        { "kind": "arc", "radius": -39.4175, "angle": 37.6541 },
        { "kind": "arc", "radius": -9.9936, "angle": 59.5987 },
        { "kind": "arc", "radius": -30.7245, "angle": 25.1118 },
        { "kind": "straight", "length": 40.7213 },
        { "kind": "arc", "radius": -56.9071, "angle": 34.9567 },
        { "kind": "arc", "radius": -18.3666, "angle": 64.2324 },
        { "kind": "arc", "radius": 54.0919, "angle": 22.8846 },
        { "kind": "arc", "radius": 30.8657, "angle": 73.3281 },
        { "kind": "arc", "radius": -804.506, "angle": 1.0815 },
        { "kind": "arc", "radius": -96.2804, "angle": 15.7937 },
        { "kind": "arc", "radius": -26.151, "angle": 31.7055 },
        { "kind": "arc", "radius": -9.4828, "angle": 75.9252 },
        { "kind": "arc", "radius": -22.5444, "angle": 40.3391 },
        { "kind": "arc", "radius": 205.7876, "angle": 7.0174 },
        { "kind": "arc", "radius": 32.7481, "angle": 30.6139 },
        { "kind": "arc", "radius": 9.3417, "angle": 89.4145 },
        { "kind": "arc", "radius": 44.4781, "angle": 25.0302 },
        { "kind": "straight", "length": 57.5223 },
        { "kind": "arc", "radius": -30.1948, "angle": 23.1322 },
        { "kind": "arc", "radius": -11.573, "angle": 73.1673 },
        { "kind": "arc", "radius": -30.0339, "angle": 31.7977 },
        { "kind": "arc", "radius": 222.5027, "angle": 4.0167 },
        { "kind": "arc", "radius": 90.9703, "angle": 12.6145 },
        { "kind": "arc", "radius": -787.2003, "angle": 2.1754 },
        { "kind": "arc", "radius": -39.9979, "angle": 29.184 },
        { "kind": "arc", "radius": -9.8889, "angle": 73.56 },
        { "kind": "arc", "radius": -37.1907, "angle": 32.554 },
        { "kind": "arc", "radius": -2507.4094, "angle": 0.3772 },
        { "kind": "arc", "radius": 62.0363, "angle": 31.5193 },
        { "kind": "arc", "radius": 30.7817, "angle": 26.7053 },
        { "kind": "arc", "radius": 9.0443, "angle": 71.4329 },
        { "kind": "arc", "radius": 18.3599, "angle": 40.3008 },
        { "kind": "arc", "radius": 93.959, "angle": 22.7176 },
        { "kind": "arc", "radius": 244.5581, "angle": 6.957 },
        { "kind": "arc", "radius": 40.8589, "angle": 40.1394 },
        { "kind": "arc", "radius": -70.3812, "angle": 23.6812 },
        { "kind": "arc", "radius": -38.254, "angle": 26.1157 },
        { "kind": "arc", "radius": -8.589, "angle": 61.385 },
        { "kind": "arc", "radius": -33.5193, "angle": 29.2947 },
        { "kind": "arc", "radius": 158.4616, "angle": 10.9687 },
        { "kind": "arc", "radius": 180.7058, "angle": 7.8077 },
        { "kind": "arc", "radius": -276.5016, "angle": 5.2418 },
        { "kind": "arc", "radius": 990.5795, "angle": 2.2139 },
        { "kind": "arc", "radius": -34.6546, "angle": 40.9371 },
        { "kind": "arc", "radius": -12.114, "angle": 73.2081 },
        { "kind": "arc", "radius": -44.6829, "angle": 33.5183 },
        { "kind": "arc", "radius": 59.0933, "angle": 21.1865 },
        { "kind": "arc", "radius": 123.7654, "angle": 7.4816 },
        { "kind": "arc", "radius": 41.8533, "angle": 33.118 },
        { "kind": "arc", "radius": 13.7662, "angle": 63.2443 },
        { "kind": "arc", "radius": 18.169, "angle": 39.8962 },
        { "kind": "arc", "radius": 651.9402, "angle": 2.2312 },
        { "kind": "arc", "radius": -129.2143, "angle": 15.616 },
        { "kind": "arc", "radius": -33.9027, "angle": 33.336 },
        { "kind": "arc", "radius": -12.3871, "angle": 79.5806 },
        { "kind": "arc", "radius": -28.5095, "angle": 19.5129 },
        { "kind": "straight", "length": 48.0517 },
        { "kind": "arc", "radius": 900.189, "angle": 0.959 },
        { "kind": "arc", "radius": -808.1417, "angle": 1.7771 },
        { "kind": "arc", "radius": -47.6576, "angle": 37.8741 },
        { "kind": "arc", "radius": -11.8378, "angle": 89.9026 },
        { "kind": "arc", "radius": -32.5569, "angle": 24.4252 },
        { "kind": "arc", "radius": -740.1279, "angle": 1.6309 },
        { "kind": "arc", "radius": 17.8338, "angle": 72.9269 },
        { "kind": "arc", "radius": -87.758, "angle": 23.2023 },
        { "kind": "arc", "radius": -128.2221, "angle": 9.8403 },
        { "kind": "arc", "radius": -70.2375, "angle": 25.9744 },
        { "kind": "arc", "radius": 13.4996, "angle": 27.8083 },
        { "kind": "straight", "length": 13.9521 },
        { "kind": "arc", "radius": 13.4996, "angle": 170.0518 }
      ],
      "features": [
        { "at": 42.5618, "height": 3, "kind": "tabletop", "length": 38.9273 },
        { "at": 130, "height": 1.7432, "kind": "stepUp", "length": 38.59 },
        { "at": 217.0695, "kind": "custom", "length": 48.9877, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1837, "h": 2.6977 }, { "u": 0.2654, "h": 2.6977 }, { "u": 0.4464, "h": 0.9474 }, { "u": 0.6709, "h": 1.7805 }, { "u": 0.9013, "h": 0.4316 }, { "u": 1, "h": 0 }] },
        { "at": 315.3342, "height": 0.7529, "kind": "roller", "length": 13.8284 },
        { "at": 338.3765, "kind": "custom", "length": 47.1762, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1908, "h": 2.5022 }, { "u": 0.2756, "h": 2.5022 }, { "u": 0.4499, "h": 0.9406 }, { "u": 0.6831, "h": 1.6514 }, { "u": 0.9049, "h": 0.4003 }, { "u": 1, "h": 0 }] },
        { "at": 435.0512, "kind": "custom", "length": 51.1081, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1761, "h": 2.9267 }, { "u": 0.2544, "h": 2.9267 }, { "u": 0.4426, "h": 1.0054 }, { "u": 0.6578, "h": 1.9316 }, { "u": 0.8973, "h": 0.4683 }, { "u": 1, "h": 0 }] },
        { "at": 540.3735, "height": 2.4679, "kind": "tabletop", "length": 44.8531 },
        { "at": 594.4503, "height": 1.1548, "kind": "roller", "length": 14.3485 },
        { "at": 619.9902, "height": 1.4442, "kind": "stepUp", "length": 46.3114 },
        { "at": 680.6862, "height": 1.7585, "kind": "stepUp", "length": 44.2904 },
        { "at": 746.8671, "height": 2.7924, "kind": "tabletop", "length": 49.9226 },
        { "at": 852.6547, "height": 0.8124, "kind": "roller", "length": 12.3339 },
        { "at": 879.6068, "height": 0.9514, "kind": "roller", "length": 11.3021 },
        { "at": 901.0566, "height": 1.5473, "kind": "stepUp", "length": 41.2666 },
        { "at": 965.8376, "height": 0.8404, "kind": "roller", "length": 14.8294 },
        { "at": 989.4913, "height": 2.5286, "kind": "tabletop", "length": 45.4786 },
        { "at": 1073.5754, "height": 2.513, "kind": "tabletop", "length": 48.9711 },
        { "at": 1130.5054, "kind": "custom", "length": 47.0286, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1914, "h": 2.4862 }, { "u": 0.2764, "h": 2.4862 }, { "u": 0.4502, "h": 0.7911 }, { "u": 0.6841, "h": 1.6409 }, { "u": 0.9052, "h": 0.3978 }, { "u": 1, "h": 0 }] },
        { "at": 1222.0911, "kind": "custom", "length": 51.0641, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1762, "h": 2.9219 }, { "u": 0.2546, "h": 2.9219 }, { "u": 0.4426, "h": 0.9918 }, { "u": 0.6581, "h": 1.9285 }, { "u": 0.8974, "h": 0.4675 }, { "u": 1, "h": 0 }] },
        { "at": 1282.8604, "height": 2.9493, "kind": "tabletop", "length": 52.8953 },
        { "at": 1342.0234, "height": 2.4605, "kind": "tabletop", "length": 48.5582 },
        { "at": 1402.2888, "height": 0.8952, "kind": "roller", "length": 10.168 },
        { "at": 1420.5527, "kind": "custom", "length": 50.9471, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1767, "h": 2.9093 }, { "u": 0.2552, "h": 2.9093 }, { "u": 0.4428, "h": 0.9301 }, { "u": 0.6588, "h": 1.9201 }, { "u": 0.8976, "h": 0.4655 }, { "u": 1, "h": 0 }] },
        { "at": 1517.1718, "height": 1.217, "kind": "stepUp", "length": 46.508 },
        { "at": 1618.7429, "kind": "custom", "length": 50.6683, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1776, "h": 2.8792 }, { "u": 0.2566, "h": 2.8792 }, { "u": 0.4433, "h": 1.0154 }, { "u": 0.6604, "h": 1.9003 }, { "u": 0.8981, "h": 0.4607 }, { "u": 1, "h": 0 }] },
        { "at": 1678.3564, "height": 2.6433, "kind": "tabletop", "length": 45.2331 },
        { "at": 1822.2726, "kind": "custom", "length": 46.8515, "shape": [{ "u": 0, "h": 0 }, { "u": 0.1921, "h": 2.4671 }, { "u": 0.2775, "h": 2.4671 }, { "u": 0.4505, "h": 0.8094 }, { "u": 0.6853, "h": 1.6283 }, { "u": 0.9056, "h": 0.3947 }, { "u": 1, "h": 0 }] }
      ]
    }"#;

/// A lap with nothing on it: somewhere to start from scratch.
///
/// Deliberately the plainest thing that is still a track — an oval on a small plot, 12 m
/// wide, no jumps at all. It validates, it builds, and everything on it is yours.
///
/// Text like [`EXAMPLE`], and parsed the same way, so the two starting points cannot drift
/// apart: whatever the type fills in for one, it fills in for the other.
pub const BLANK: &str = r#"{
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

/// Segments that ride from one pose to another: a turn, a straight and a turn.
///
/// The shorter of the two same-direction Dubins paths. Both are tried and the shorter kept,
/// which is enough for the two things that need it — a lap that has drifted open under an
/// edit, and the start straight coming back onto the lap — because in both the two poses are
/// much further apart than the turning circles. The mixed-direction paths only matter when
/// they are not.
///
/// `None` when the poses are already the same one.
pub fn join(from: Start, to: Start, radius: f32) -> Option<Vec<Segment>> {
    let r = radius.abs().max(1.0);
    let (fx, fz, fh) = (from.x, from.z, from.angle.to_radians());
    let goal = (to.x, to.z, to.angle.to_radians());

    let gap = ((fx - goal.0).powi(2) + (fz - goal.1).powi(2)).sqrt();
    if gap < 0.5 && wrap(goal.2 - fh).abs() < 0.02 {
        return None;
    }

    // `turn` is +1 for a pair of right-hand circles, -1 for left.
    let solve = |turn: f32| -> Option<(f32, f32, f32)> {
        let centre = |x: f32, z: f32, th: f32| {
            let (rx, rz) = right_vector(th);
            (x + rx * r * turn, z + rz * r * turn)
        };
        let c1 = centre(fx, fz, fh);
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
        Some((sweep(fh, th_s), run, sweep(th_s, goal.2)))
    };

    let mut best: Option<(f32, Vec<Segment>)> = None;
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
            best = Some((cost, segs));
        }
    }
    best.map(|(_, segs)| segs)
}

/// Two arcs from one pose to another, tangent where they meet: a corner rather than a
/// dog-leg.
///
/// [`join`] answers the same question with a turn, a straight and a turn, which is right for
/// closing a lap that has drifted open — the two ends are far apart and mostly need
/// travelling between. It is wrong for a start straight coming back onto the lap: what that
/// wants is *turn one*, and every published start line is exactly that, a corner of 100–180°
/// with no straight in it at all. Given a Dubins path, our merges came out as a seven-metre
/// arc, a ninety-four metre straight and another arc — a start straight that never ends.
///
/// The equal-chord biarc: the joint is placed so the two arcs have the same chord length,
/// which is the standard construction and needs no search.
///
/// `None` when the poses are already the same, or when the geometry wants an arc tighter
/// than `min_radius`.
pub fn biarc(from: Start, to: Start, min_radius: f32) -> Option<Vec<Segment>> {
    let (fx, fz) = heading_vector(from.angle.to_radians());
    let (tx, tz) = heading_vector(to.angle.to_radians());
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let chord = (dx * dx + dz * dz).sqrt();
    if chord < 1.0 {
        return None;
    }

    // How far along each tangent the joint sits.
    let dot_t = fx * tx + fz * tz;
    let denom = 2.0 * (1.0 - dot_t);
    let b = dx * (fx + tx) + dz * (fz + tz);
    let c = dx * dx + dz * dz;
    let d1 = if denom.abs() < 1e-4 {
        // Tangents parallel: the joint falls out of the chord alone.
        let along = dx * tx + dz * tz;
        if along.abs() < 1e-4 {
            return None;
        }
        c / (4.0 * along)
    } else {
        let disc = b * b + denom * c;
        if disc < 0.0 {
            return None;
        }
        (-b + disc.sqrt()) / denom
    };
    if !d1.is_finite() || d1 <= 0.1 {
        return None;
    }
    let joint = (
        ((from.x + fx * d1) + (to.x - tx * d1)) * 0.5,
        ((from.z + fz * d1) + (to.z - tz * d1)) * 0.5,
    );

    // Each arc from its own end, through the chord to the joint.
    let arc = |px: f32, pz: f32, hx: f32, hz: f32, qx: f32, qz: f32| -> Option<Segment> {
        let (cx, cz) = (qx - px, qz - pz);
        let len = (cx * cx + cz * cz).sqrt();
        if len < 0.01 {
            return None;
        }
        let (ux, uz) = (cx / len, cz / len);
        // The chord subtends twice the angle between the tangent and it; which way round is
        // the cross product, negative being a right-hand turn in this convention.
        let cross = hx * uz - hz * ux;
        let half = (hx * ux + hz * uz).clamp(-1.0, 1.0).acos();
        if half < 1e-4 {
            return Some(Segment::Straight { length: len, rise: 0.0 });
        }
        let radius = len / (2.0 * half.sin());
        Some(Segment::Arc {
            radius: radius * if cross > 0.0 { -1.0 } else { 1.0 },
            angle: (2.0 * half).to_degrees(),
            rise: 0.0,
        })
    };

    let first = arc(from.x, from.z, fx, fz, joint.0, joint.1)?;
    // The second runs from the joint to the goal, and its tangent there is the goal's — so it
    // is fitted backwards from the goal and then turned round.
    let second = arc(to.x, to.z, -tx, -tz, joint.0, joint.1).map(|s| match s {
        Segment::Arc { radius, angle, rise } => Segment::Arc { radius: -radius, angle, rise },
        other => other,
    })?;

    for seg in [&first, &second] {
        if let Segment::Arc { radius, angle, .. } = seg {
            if radius.abs() < min_radius || angle.abs() > 200.0 {
                return None;
            }
        }
    }
    Some(vec![first, second])
}

/// Where a run of segments leaves you, ridden from `from`.
///
/// The same walk [`TrackProgram::stations`] does, without the sampling: arcs are stepped from
/// their centre, so a hundred segments end exactly where their radii and angles say.
fn end_pose(from: Start, segs: &[Segment]) -> Start {
    let (mut x, mut z) = (from.x, from.z);
    let mut theta = from.angle.to_radians();
    for seg in segs {
        match *seg {
            Segment::Straight { .. } => {
                let (dx, dz) = heading_vector(theta);
                x += dx * seg.length();
                z += dz * seg.length();
            }
            Segment::Arc { radius, angle, .. } => {
                let turn = radius.signum();
                let r = radius.abs().max(0.01);
                let (rx, rz) = right_vector(theta);
                let (cx, cz) = (x + rx * r * turn, z + rz * r * turn);
                theta += turn * angle.abs().to_radians();
                let (rx, rz) = right_vector(theta);
                x = cx - rx * r * turn;
                z = cz - rz * r * turn;
            }
        }
    }
    Start { x, z, angle: theta.to_degrees() }
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

    /// How much straight the lap opens with, metres.
    ///
    /// The leading run, not the first segment: consecutive straights are colinear, so three
    /// of them in a row are one straight to everything that rides it. This is what the start
    /// gets laid out on — the gate row, the finish line and the run at turn one all sit on
    /// it, and every file that mentions the start measures from where it begins.
    pub fn opening_straight(&self) -> f32 {
        self.segments
            .iter()
            .take_while(|s| matches!(s, Segment::Straight { .. }))
            .map(|s| s.length())
            .sum()
    }

    /// Every straight on the lap: `(segment index, metres round the lap, length)`.
    ///
    /// Runs of consecutive straights are merged for the same reason as above, and so is the
    /// pair either side of the finish — a lap that ends on a straight and begins on one has
    /// a single straight through the line, and it is usually the longest thing on the track.
    pub fn straight_runs(&self) -> Vec<(usize, f32, f32)> {
        let mut out: Vec<(usize, f32, f32)> = Vec::new();
        let mut at = 0.0f32;
        for (i, seg) in self.segments.iter().enumerate() {
            let len = seg.length();
            if matches!(seg, Segment::Straight { .. }) {
                match out.last_mut() {
                    // Carrying on the run the segment before it started.
                    Some(run) if (run.1 + run.2 - at).abs() < 1e-3 => run.2 += len,
                    _ => out.push((i, at, len)),
                }
            }
            at += len;
        }
        // Across the finish, when the lap both ends and begins on one — and only when it
        // closes, because two straights either side of a lap that doesn't meet itself are two
        // straights pointing different ways in different places.
        if out.len() > 1 && self.closes() {
            let first = out[0];
            let last = *out.last().expect("more than one run");
            if first.0 == 0 && (last.1 + last.2 - at).abs() < 1e-3 {
                out.pop();
                out.remove(0);
                out.push((last.0, last.1, last.2 + first.2));
            }
        }
        out
    }

    /// Whether the finish meets the start, pose and all — near enough that the two ends are
    /// the same piece of track.
    pub fn closes(&self) -> bool {
        let end = end_pose(self.start, &self.segments);
        let gap = ((end.x - self.start.x).powi(2) + (end.z - self.start.z).powi(2)).sqrt();
        gap < 1.0 && wrap((end.angle - self.start.angle).to_radians()).abs() < 0.02
    }

    /// Where the race starts: a straight off to the side of the opening straight, and the
    /// turn that brings it back onto the lap.
    ///
    /// Built rather than stated, because it is not a design decision — it is where the lap
    /// already is. The gate row stands [`START_OFFSET_M`] off the opening straight on
    /// whichever side has the room, runs [`START_SPRINT_M`] alongside it, and then joins the
    /// lap wherever the join is shortest, which is turn one.
    ///
    /// `None` when the lap has no straight to run beside — the gates would have nothing to
    /// line up against, and [`crate::trackllm::review`] says so.
    pub fn start_line(&self) -> Option<StartLine> {
        let run = self.opening_straight();
        if run < START_SPRINT_M * 0.5 || self.segments.is_empty() {
            return None;
        }
        let st = self.stations(4.0);
        if st.len() < 8 {
            return None;
        }
        // The side with the room. The lap folds back on itself, so one side of the opening
        // straight is usually its own next pass and the other is the field.
        let (fx, fz) = heading_vector(self.start.angle.to_radians());
        let (rx, rz) = right_vector(self.start.angle.to_radians());
        let room = |side: f32| -> f32 {
            let mut worst = f32::MAX;
            for a in st.iter().filter(|q| q.s <= run) {
                for b in &st {
                    if (b.s - a.s).abs().min(self.lap_length() - (b.s - a.s).abs()) < 40.0 {
                        continue;
                    }
                    let (dx, dz) = (b.x - a.x, b.z - a.z);
                    if (dx * fx + dz * fz).abs() > 20.0 {
                        continue;
                    }
                    let lat = dx * rx + dz * rz;
                    if lat * side > 0.0 {
                        worst = worst.min(lat.abs());
                    }
                }
            }
            worst
        };
        // Which side to stand on. The outside of turn one, so the sprint delivers the pack
        // into the corner rather than across it — unless that side has no room, in which case
        // room wins, because a start straight that doesn't fit isn't one.
        let (room_r, room_l) = (room(1.0), room(-1.0));
        let outside = st
            .iter()
            .find(|q| q.s > run * 0.8 && q.curvature.abs() > 1.0 / TURN_ONE_RADIUS_M)
            .map(|q| -q.curvature.signum())
            .unwrap_or(0.0);
        let side = if outside != 0.0
            && (if outside > 0.0 { room_r } else { room_l }) > START_OFFSET_M * 1.15
        {
            outside
        } else if room_r >= room_l {
            1.0
        } else {
            -1.0
        };

        // The gate row: beside the lap's own start, far enough out that the lap never runs
        // through it.
        // Angled a little towards the lap rather than parallel to it, so what brings the two
        // together is one corner instead of an S. That is the shape published start lines
        // have: Indiana's straight runs 90 m and then turns *once*, through 170°, onto the
        // racing line.
        let start = Start {
            x: self.start.x + rx * side * START_OFFSET_M,
            z: self.start.z + rz * side * START_OFFSET_M,
            angle: self.start.angle - side * START_CONVERGE_DEG,
        };
        // Where turn one is: the first station past the opening straight that is properly
        // turning, not drifting. Everything below is measured against it, because a start
        // straight that lands on a straight is a start that leaves a rider guessing which way
        // to go — what a real one does is deliver the pack into the first corner.
        let tight = |q: &Station| q.curvature.abs() > 1.0 / TURN_ONE_RADIUS_M;
        let corner_at = st
            .iter()
            .find(|q| q.s > run * 0.8 && tight(q))
            .map(|q| q.s)
            .unwrap_or(run);
        // And how far it runs. The start joins *inside* turn one rather than at its mouth:
        // that is what published start lines do — Indiana's own turns through 46 m, then 10,
        // then 17 before it meets the racing line — and it is what turns the pack instead of
        // handing it a fork.
        let corner_end = st
            .iter()
            .find(|q| q.s > corner_at + 12.0 && !tight(q))
            .map(|q| q.s)
            .unwrap_or(corner_at + 60.0);
        let land_at = corner_at + (corner_end - corner_at).min(30.0) * 0.35;

        // Down the sprint, and then into it.
        //
        // Three goes, each asking for less: land inside turn one without the pad crossing the
        // lap; failing that, land anywhere it can without crossing; failing that, take the
        // shortest merge there is. A track whose ground will not hold the ideal start still
        // gets one, and it is the same search each time.
        let after = end_pose(start, &[Segment::Straight { length: START_SPRINT_M, rise: 0.0 }]);
        let search = |aim_at_the_corner: bool, keep_clear: bool| -> Option<(f32, Vec<Segment>, f32)> {
        let mut best: Option<(f32, Vec<Segment>, f32)> = None;
        for q in st.iter().filter(|q| q.s >= run * 0.5 && q.s <= corner_end + 120.0) {
            let onto = Start { x: q.x, z: q.z, angle: q.heading.to_degrees() };
            // A corner, not a dog-leg: two arcs that meet tangentially, which is the shape
            // every published start line joins the lap with. The Dubins path is the fallback
            // for a geometry the biarc cannot fit.
            let Some(merge) = biarc(after, onto, START_MERGE_RADIUS_M)
                .or_else(|| join(after, onto, START_MERGE_RADIUS_M))
            else {
                continue;
            };
            let turned: f32 = merge
                .iter()
                .map(|s| match s {
                    Segment::Arc { angle, .. } => angle.abs(),
                    _ => 0.0,
                })
                .sum();
            // A join that has to swing right round is not a merge, it is a detour.
            if turned > 200.0 {
                continue;
            }
            // Anything in a merge that isn't turning is a start straight that has not ended —
            // a straight, or an arc so open it rides as one. Both cost the same.
            let straight: f32 = merge
                .iter()
                .filter(|s| match s {
                    Segment::Straight { .. } => true,
                    Segment::Arc { radius, .. } => radius.abs() > TURN_ONE_RADIUS_M * 2.5,
                })
                .map(|s| s.length())
                .sum();
            // And the sooner it is back on the lap the better: a merge that picks a join a
            // hundred metres further round is a gentle sweep that reads as more straight.
            // Published start lines turn 100–180° through 10–46 m radii and are done with it.
            // And it lands in the corner. A join onto straight track is cheap to build and
            // reads as a slip road: the pack arrives on the racing line with nothing telling
            // it where the track goes. Joining where the lap is already turning is what makes
            // the start straight end in turn one, which is what every real one does.
            // Two things it is scored on beyond its length: landing well inside turn one, and
            // turning like one. Published merges turn 100–180° all told; a merge that turns
            // twenty is a slip road, whatever else is right about it.
            let into_the_turn = if aim_at_the_corner { (q.s - land_at).abs() } else { 0.0 };
            // One-way, and the same way as the corner it lands in. An S — right then left —
            // is what you get joining straight track from a parallel offset, and it rides as
            // a chicane on the way to a fork. A published start turns *with* turn one and
            // hands the pack to it already leant over.
            let ways: Vec<f32> = merge
                .iter()
                .filter_map(|s| match s {
                    Segment::Arc { radius, angle, .. } if angle.abs() > 8.0 => {
                        Some(radius.signum())
                    }
                    _ => None,
                })
                .collect();
            let s_shaped = ways.windows(2).any(|w| w[0] != w[1]);
            let with_the_corner = match (ways.last(), q.curvature) {
                (Some(way), k) if k.abs() > 1.0 / TURN_ONE_RADIUS_M => *way == k.signum(),
                _ => true,
            };
            // It may not cross the lap to get there. Reaching a corner two hundred metres round
            // by sweeping over the main straight is a start straight that runs through the
            // track — which is the thing this whole spur exists to avoid.
            let path = TrackProgram {
                start: after,
                segments: merge.clone(),
                features: Vec::new(),
                elevation: Vec::new(),
                ..self.clone()
            };
            let mut crosses = false;
            for a in path.stations(3.0) {
                for b in &st {
                    let apart = (b.s - q.s).abs().min(self.lap_length() - (b.s - q.s).abs());
                    // Near the join the two are meant to be together: a start straight and the
                    // approach it merges into share their ground for the last stretch, which
                    // is what makes one surface out of two at turn one. What the rule is for
                    // is the pad lying across a *different* part of the lap.
                    if apart < 90.0 {
                        continue;
                    }
                    // Nor is the opening straight a thing to cross: the start runs beside it
                    // by construction, forty metres out, and the pad reaching towards it is
                    // the whole idea. What matters is the rest of the lap.
                    if b.s <= run + 10.0 {
                        continue;
                    }
                    // Against the *pad*, not the centreline: the start is 54 m across at the
                    // gates and still twenty by the middle of the merge, and a line that
                    // clears the lap by fifteen metres lays its pad straight over it.
                    let taper = (a.s / (START_SPRINT_M * 0.9).max(1.0)).clamp(0.0, 1.0);
                    let pad = START_FAN_HALF_M + (self.width * 0.5 - START_FAN_HALF_M) * taper;
                    // The riding line has to stay outside the pad — that is what "crossing"
                    // means here. Anything more generous is unsatisfiable on a lap that folds
                    // back on itself every eighty metres.
                    let want = pad + 2.0;
                    if (b.x - a.x).powi(2) + (b.z - a.z).powi(2) < want * want {
                        crosses = true;
                        break;
                    }
                }
                if crosses {
                    break;
                }
            }
            if crosses && keep_clear {
                continue;
            }
            // Landing in the corner is what this is for, so it outweighs everything else. The
            // straight in a merge still costs — a start straight that has not ended reads as
            // one — but only enough to break a tie: Indiana runs 90 m of straight before its
            // own corner, and a gentle approach into turn one is exactly right.
            let cost: f32 = merge.iter().map(|s| s.length()).sum::<f32>()
                + straight * 0.6
                + into_the_turn * 3.0
                + (TURN_ONE_SWEEP_DEG - turned).max(0.0) * 0.9
                + if s_shaped { 90.0 } else { 0.0 }
                + if with_the_corner { 0.0 } else { 90.0 };
            if best.as_ref().map(|b| cost < b.0).unwrap_or(true) {
                best = Some((cost, merge, q.s));
            }
        }
        best
        };
        let (_, merge, joins_at) = search(true, true)
            .or_else(|| search(false, true))
            .or_else(|| search(false, false))?;
        let mut segments = vec![Segment::Straight { length: START_SPRINT_M, rise: 0.0 }];
        segments.extend(merge.into_iter().filter(|s| s.length() > 0.5));
        Some(StartLine { start, segments, joins_at, side })
    }

    /// The stretch of the main straight a finish jump may stand on: metres round the lap,
    /// from the corner exit to where the ground runs out.
    ///
    /// `None` when the lap has no start — a track whose gates end up on the racing line has
    /// forty riders coming through here off a standing start, and nothing big belongs in
    /// front of them. Otherwise it ends at the shorter of the straight's own end and where
    /// the start spur merges back in, for the same reason.
    pub fn finish_window(&self) -> Option<(f32, f32)> {
        let run = self.opening_straight();
        let line = self.start_line()?;
        let mut to = run - FINISH_RUNOUT_M;
        if line.joins_at < to {
            to = line.joins_at - 5.0;
        }
        let from = FINISH_RUNUP_M;
        (to - from >= finish_jump_length(FINISH_JUMP_M.0, TABLETOP_DECK_M)).then_some((from, to))
    }

    /// The finish jump, if the lap has one: the tallest jump standing in that window.
    ///
    /// Found rather than recorded. A jump is the finish jump because of where it is and how
    /// big it is, and asking the ground is what keeps the answer true after an edit moves
    /// something — there is no flag to go stale.
    pub fn finish_jump(&self) -> Option<&Feature> {
        let (from, to) = self.finish_window()?;
        self.features
            .iter()
            .filter(|f| matches!(f, Feature::Tabletop { .. } | Feature::Double { .. }))
            .filter(|f| f.height() >= FINISH_JUMP_M.0 - 0.1)
            .filter(|f| f.at() >= from - 1.0 && f.at() + f.length() <= to + 1.0)
            .max_by(|a, b| a.height().total_cmp(&b.height()))
    }

    /// Begin the lap at segment `index`: the same track, ridden from a different point on it.
    ///
    /// Where a lap starts is not part of its shape — it decides where the gate row stands and
    /// what `long = 0` means in every file that states a distance round the lap, and nothing
    /// else. So a lap drawn starting halfway round a corner can be given a proper start
    /// straight without redrawing it: turn the list of segments round and walk the start pose
    /// to where the new first segment begins.
    ///
    /// Everything placed by distance moves with it — the features and the elevation knots —
    /// and a feature left straddling the new finish is pulled back to end on it, which is
    /// what the studio does to a stranded jump.
    ///
    /// Returns how far round the old lap the new start is.
    pub fn rotate_start(&mut self, index: usize) -> f32 {
        if self.segments.is_empty() || index == 0 || index >= self.segments.len() {
            return 0.0;
        }
        let lap = self.lap_length();
        let shift: f32 = self.segments[..index].iter().map(|s| s.length()).sum();
        self.start = end_pose(self.start, &self.segments[..index]);
        // Back into a circle: the walk adds every turn to the heading, so starting partway
        // round a lap that turns 2651° leaves the pose facing 541 degrees.
        self.start.angle = self.start.angle.rem_euclid(360.0);
        self.segments.rotate_left(index);

        for f in &mut self.features {
            let len = f.length();
            let at = (f.at() - shift).rem_euclid(lap.max(1.0));
            *f.at_mut() = if at + len > lap { (lap - len).max(0.0) } else { at };
        }
        self.features.sort_by(|a, b| a.at().total_cmp(&b.at()));
        for k in &mut self.elevation {
            k.at = (k.at - shift).rem_euclid(lap.max(1.0));
        }
        self.elevation.sort_by(|a, b| a.at.total_cmp(&b.at));
        shift
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
        let last = *self.stations(1.0).last()?;
        let from = Start { x: last.x, z: last.z, angle: last.heading.to_degrees() };
        join(from, self.start, radius)
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
                wear: default_wear(),
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

    /// An oval: two straights of different lengths, so which one the start lands on is
    /// visible in the numbers.
    fn oval() -> TrackProgram {
        prog(vec![
            Segment::Arc { radius: 40.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 30.0, rise: 0.0 },
            Segment::Arc { radius: 40.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 30.0, rise: 0.0 },
        ])
    }

    #[test]
    fn straight_runs_merge_what_rides_as_one_straight() {
        let p = prog(vec![
            Segment::Straight { length: 20.0, rise: 0.0 },
            Segment::Straight { length: 25.0, rise: 0.0 },
            Segment::Arc { radius: 30.0, angle: 90.0, rise: 0.0 },
            Segment::Straight { length: 10.0, rise: 0.0 },
        ]);
        let runs = p.straight_runs();
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert_eq!(runs[0].0, 0);
        assert!((runs[0].2 - 45.0).abs() < 0.01, "{runs:?}");
    }

    #[test]
    fn a_straight_through_the_finish_is_one_straight() {
        // An oval whose finish sits in the middle of a straight: 20 m of it before the line
        // and 30 m after, which is one 50 m straight and not two.
        let p = prog(vec![
            Segment::Straight { length: 30.0, rise: 0.0 },
            Segment::Arc { radius: 40.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 50.0, rise: 0.0 },
            Segment::Arc { radius: 40.0, angle: 180.0, rise: 0.0 },
            Segment::Straight { length: 20.0, rise: 0.0 },
        ]);
        let runs = p.straight_runs();
        assert_eq!(runs.len(), 2, "{runs:?}");
        let through = runs.iter().find(|r| r.0 == 4).expect("the run through the finish");
        assert!((through.2 - 50.0).abs() < 0.01, "{runs:?}");
    }

    #[test]
    fn a_rotated_lap_is_the_same_lap() {
        let before = oval();
        let mut after = before.clone();
        let shift = after.rotate_start(1);
        assert!((shift - 40.0 * std::f32::consts::PI).abs() < 0.01, "{shift}");
        assert!((after.lap_length() - before.lap_length()).abs() < 0.01);
        assert!(after.closure_error() < 0.05, "{}", after.closure_error());

        // Every point of the rotated lap is a point of the original, at the shifted distance.
        let lap = before.lap_length();
        let was = before.stations(1.0);
        for st in after.stations(1.0) {
            let s = (st.s + shift).rem_euclid(lap);
            let near = was
                .iter()
                .map(|q| ((q.x - st.x).powi(2) + (q.z - st.z).powi(2)).sqrt())
                .fold(f32::MAX, f32::min);
            assert!(near < 0.6, "{:.1} m off the old lap at {s:.0} m", near);
        }
    }

    #[test]
    fn rotating_carries_the_features_round() {
        let mut p = oval();
        p.features = vec![Feature::Tabletop { at: 150.0, length: 20.0, height: 1.5, lip: 0.0 }];
        p.elevation = vec![Knot { at: 150.0, height: 2.0 }];
        let shift = p.rotate_start(1);
        assert!((p.features[0].at() - (150.0 - shift)).abs() < 0.01, "{:?}", p.features[0]);
        assert!((p.elevation[0].at - (150.0 - shift)).abs() < 0.01, "{:?}", p.elevation[0]);
        p.check().expect("and the lap is still one the synthesiser will take");
    }

    #[test]
    fn a_feature_left_straddling_the_finish_is_pulled_back_onto_it() {
        let mut p = oval();
        // Sits over the point the lap is about to start at, which after the rotation would
        // put it half before the start and half past the finish.
        let at = 40.0 * std::f32::consts::PI - 5.0;
        p.features = vec![Feature::Tabletop { at, length: 20.0, height: 1.5, lip: 0.0 }];
        p.rotate_start(1);
        p.check().expect("nothing hangs off the end of the lap");
    }

    /// Prints [`EXAMPLE`] as the repair pass leaves it — begun on a straight, on ground big
    /// enough to hold the start beside it. Run when the example changes:
    ///
    /// ```text
    /// cargo test -- --ignored --nocapture print_the_example
    /// ```
    #[test]
    #[ignore = "prints a program"]
    fn print_the_example() {
        let mut p: TrackProgram = serde_json::from_str(EXAMPLE).unwrap();
        let done = crate::trackllm::repair_for_tests(&mut p);
        for line in &done {
            println!("// {line}");
        }
        let line = p.start_line();
        println!(
            "// opens on {:.0} m of straight, closes to {:.2} m, start straight {:.0} m",
            p.opening_straight(),
            p.closure_error(),
            line.map(|l| l.length()).unwrap_or(0.0),
        );
        println!("{}", serde_json::to_string(&p).unwrap());
    }

    #[test]
    fn the_example_starts_on_a_straight_long_enough_for_a_start() {
        let p: TrackProgram = serde_json::from_str(EXAMPLE).unwrap();
        assert!(
            p.opening_straight() >= START_STRAIGHT_M,
            "the example opens with {:.0} m of straight",
            p.opening_straight()
        );
        assert!(p.closes(), "and it still meets itself");
    }

    /// The blank track is a track, and it arrives whole.
    ///
    /// Both halves matter. It has to validate — an unusable starting point is worse than
    /// none — and what the studio is handed has to carry every field, including the ones the
    /// source text leaves to a default. The studio types numbers into `blend`; an absent one
    /// took the whole tab down the moment anyone clicked Blank.
    #[test]
    fn the_blank_track_is_a_track_and_arrives_whole() {
        let p: TrackProgram = serde_json::from_str(BLANK).expect("the blank track parses");
        p.check().expect("and it validates");
        assert!(p.closes(), "and it meets itself");
        assert!(
            p.opening_straight() >= START_STRAIGHT_M,
            "and it has room for a start: {:.0} m",
            p.opening_straight()
        );

        // The source text leaves every defaulted field out — that is the whole hazard, so
        // it is stated here rather than left for someone to rediscover.
        let literal: serde_json::Value = serde_json::from_str(BLANK).unwrap();
        assert!(
            literal.get("blend").is_none(),
            "the literal now states `blend`, so this test no longer proves anything"
        );

        // What the command actually answers with. Serialising the *type* is what fills the
        // defaults in; handing back the source text does not, and an absent `blend` put an
        // undefined into a number field and took the studio down.
        let sent = tauri::async_runtime::block_on(crate::blank_track_program())
            .expect("the command answers");
        for key in ["blend", "elevation", "features", "segments", "width"] {
            assert!(sent.get(key).is_some(), "the studio is handed no `{key}`");
        }
        assert!(
            sent["terrain"].get("wear").is_some(),
            "the studio is handed no ground `wear`"
        );
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
            height: 2.0, lip: 0.0
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

    /// How steep the face runs, in degrees, over `n` even steps along its length.
    ///
    /// The step count is a measurement decision rather than a detail. A face differenced at
    /// two thousand steps reads as much as 0.013° *backwards* here and there — the arc is
    /// monotonic, but a single-precision difference between two neighbouring samples of it
    /// is mostly rounding by then. Two hundred and fifty steps is 6 cm of a 15 m face, far
    /// finer than the terrain grid can hold, and the noise falls below a thousandth of a
    /// degree.
    fn face_slopes_n(height: f32, run: f32, n: usize) -> Vec<f32> {
        let sweep = face_sweep(height, run);
        (0..n)
            .map(|i| {
                let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let dy = (face_arc(t1, sweep) - face_arc(t0, sweep)) * height;
                let dx = (t1 - t0) * run;
                dy.atan2(dx).to_degrees()
            })
            .collect()
    }

    fn face_slopes(height: f32, run: f32) -> Vec<f32> {
        face_slopes_n(height, run, 250)
    }

    #[test]
    fn a_jump_face_is_steepest_at_its_lip() {
        // The whole point of the arc, and what a smoothstep cannot do. A smoothstep peaks
        // halfway up and arrives at the lip at 0.03° — dead flat over the last metre before
        // the rider leaves the ground, which is why a jump built on one rode like a roller
        // however tall it stood.
        for height in [1.0f32, 2.5, 4.0] {
            let run = face_run(height, JUMP_FACE_DEG, JUMP_FACE_MIN_M);
            let s = face_slopes(height, run);
            let peak = s.iter().copied().fold(f32::MIN, f32::max);
            assert!(s[0].abs() < 0.5, "the foot leaves the ground tangent: {:.2}°", s[0]);
            assert!(
                (s[s.len() - 1] - peak).abs() < 0.1,
                "the lip is the steepest of it: lip {:.2}° against peak {:.2}°",
                s[s.len() - 1],
                peak
            );
            // Monotonic: it never eases off partway up and then steepens again.
            assert!(
                s.windows(2).all(|w| w[1] >= w[0] - 0.01),
                "the face steepens the whole way up"
            );
        }
    }

    #[test]
    fn a_face_never_stands_steeper_than_the_corpus_allows() {
        // Sized by the half-angle, so the ceiling lands on the lip rather than being
        // overshot there the way the smoothstep's 1.5 fudge had to correct for.
        for height in [0.4f32, 1.0, 2.5, 4.0, 5.9] {
            let run = face_run(height, JUMP_FACE_DEG, JUMP_FACE_MIN_M);
            let peak = face_slopes(height, run).into_iter().fold(f32::MIN, f32::max);
            assert!(peak <= JUMP_FACE_DEG + 0.1, "{height} m peaks at {peak:.2}°");
            // And a jump held above the minimum length is not made gentler than it is:
            // only the ones the floor catches come out shallower.
            if run > JUMP_FACE_MIN_M + 0.01 {
                assert!(peak > JUMP_FACE_DEG - 0.5, "{height} m only reaches {peak:.2}°");
            }
        }
    }

    #[test]
    fn a_landing_is_longer_and_gentler_than_the_takeoff_that_feeds_it() {
        // The asymmetry the corpus measures — takeoffs 27° at the ninetieth, landings 19° —
        // which `JUMP_LANDING_DEG` carried while nothing but the tabletop read it.
        let f = double_faces(2.5, 10.0);
        assert!(
            f.run > f.ramp,
            "the run-off catches over more ground than the ramp throws over: {:.1} m against {:.1}",
            f.run,
            f.ramp
        );
        let steepest = |run: f32| {
            face_slopes(2.5, run).into_iter().fold(f32::MIN, f32::max)
        };
        assert!(
            steepest(f.run) < steepest(f.back) - 3.0,
            "and it is the gentler of the two: {:.1}° against {:.1}°",
            steepest(f.run),
            steepest(f.back)
        );
    }

    /// Every 5 cm along a double, and the angle the ground turns through between one step
    /// and the next.
    fn double_slopes(height: f32, gap: f32, lip: f32) -> Vec<f32> {
        let shape = double_shape(height, gap, lip);
        let len = shape.faces.total(gap);
        let step = 0.05f32;
        let n = (len / step) as usize;
        (0..n)
            .map(|i| {
                let (a, b) = (i as f32 * step, (i + 1) as f32 * step);
                ((shape.height_at(b) - shape.height_at(a)) / step).atan().to_degrees()
            })
            .collect()
    }

    /// The profile of a double, in columns, for plotting.
    ///
    /// ```text
    /// FROST_H=2.5 FROST_GAP=9 FROST_LIP=6 cargo test --bins -- --ignored --nocapture \
    ///   diag_double_profile
    /// ```
    #[test]
    #[ignore = "prints a profile"]
    fn diag_double_profile() {
        let num = |k: &str, d: f32| {
            std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
        };
        let (h, gap, lip) = (num("FROST_H", 2.5), num("FROST_GAP", 9.0), num("FROST_LIP", 6.0));
        let s = double_shape(h, gap, lip);
        let mut u = 0.0f32;
        while u <= s.faces.total(gap) {
            println!("{u:.3} {:.4}", s.height_at(u));
            u += 0.05;
        }
    }

    #[test]
    fn a_double_turns_rather_than_folding() {
        // What was wrong with the old shape, in one number. Four faces meeting at points put
        // a sixty-degree change of slope into a single 5 cm step at the lip — a triangle,
        // which is what it looked like on the ground. Arcs joined tangentially cannot do
        // that: the ground turns, and how fast it turns is the radius it turns on.
        for (height, gap, lip) in [
            (2.5f32, 9.0f32, 6.0f32),
            (0.6, 5.0, 0.0),
            (4.0, 12.0, 0.0),
            (2.5, 0.0, 0.0),
            (1.3, 2.0, 5.5),
            (3.0, 24.0, 10.0),
        ] {
            let s = double_slopes(height, gap, lip);
            let kink = s.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
            assert!(
                kink < 4.0,
                "{height} m over {gap} m folds through {kink:.1}° in 5 cm"
            );
            let steepest = s.iter().copied().fold(f32::MIN, |a, b| a.max(b.abs()));
            assert!(
                steepest <= JUMP_CUT_DEG + 0.5,
                "{height} m over {gap} m stands at {steepest:.1}°"
            );
        }
    }

    #[test]
    fn the_gap_is_never_on_the_ground() {
        // The whole point of the valley. A double dug to grade is a trench between two piles,
        // and the bottom of it is where the rider who came up short arrives. The pair stands
        // on the ground instead, and the gap is a dip in it.
        for (height, gap, lip) in [
            (0.9f32, 2.0f32, 0.0f32),
            (1.4, 8.0, 0.0),
            (2.2, 0.0, 0.0),
            (2.5, 9.0, 6.0),
            (3.0, 12.0, 20.0),
            (3.6, 6.0, 0.0),
        ] {
            let s = double_shape(height, gap, lip);
            let f = &s.faces;
            let (mut u, mut floor) = (f.ramp, f32::MAX);
            while u <= f.ramp + f.back + gap + f.face {
                floor = floor.min(s.height_at(u));
                u += 0.05;
            }
            assert!(
                floor > 0.05,
                "{height} m over {gap} m digs its gap to {floor:.2} m"
            );
            // And it is still two jumps rather than one hump with a dent in it. Not the
            // full [`JUMP_VALLEY_FALL`]: that is a ceiling, and a gap of nothing at all has
            // no ground to pay for it.
            assert!(
                height - floor >= height * 0.35,
                "{height} m over {gap} m only falls {:.2} m between its crests",
                height - floor
            );
            assert!(
                height - floor <= height * JUMP_VALLEY_FALL + 1e-3,
                "{height} m over {gap} m falls {:.2} m, past the ceiling",
                height - floor
            );
        }
    }

    #[test]
    fn the_crests_stand_where_the_faces_say() {
        // The rounding is fitted inside the four faces, so nothing that reads `double_faces`
        // — the speed model, the ruts, the scenery — is looking in the wrong place.
        for (height, gap, lip) in [
            (2.5f32, 9.0f32, 6.0f32),
            (0.6, 5.0, 0.0),
            (4.0, 12.0, 0.0),
            (1.3, 2.0, 5.5),
            // Tight enough that the valley rides a saddle: the crests still have to be where
            // they are said to be, and nothing may stand taller than the jump was asked for.
            (2.5, 0.0, 0.0),
        ] {
            let s = double_shape(height, gap, lip);
            let f = &s.faces;
            let land = f.ramp + f.back + gap + f.face;
            assert!(
                (s.height_at(f.ramp) - height).abs() < 1e-3,
                "the lip is {:.3} m, not {height}",
                s.height_at(f.ramp)
            );
            assert!(
                (s.height_at(land) - height).abs() < 1e-3,
                "the landing's crest is {:.3} m, not {height}",
                s.height_at(land)
            );
            // Nothing stands taller than the jump was asked for, and nothing is dug below
            // grade: land short and you land on flat, which is the whole definition of a
            // double.
            let mut u = 0.0;
            let (mut peak, mut floor) = (f32::MIN, f32::MAX);
            while u <= f.total(gap) {
                let y = s.height_at(u);
                peak = peak.max(y);
                floor = floor.min(y);
                u += 0.05;
            }
            assert!(peak <= height + 1e-3, "peaks at {peak:.3} against {height}");
            assert!(floor >= 0.0, "digs below the ground it stands on: {floor:.3}");

        }
    }

    #[test]
    fn a_shape_and_the_length_it_reports_end_at_the_same_place() {
        // These two used to be written out separately and drifted apart, cutting the last
        // ramp off into a step. Now `total` is the only statement of it.
        for (height, gap, lip) in [(2.5f32, 8.0f32, 10.0f32), (1.2, 4.0, 6.0), (4.0, 14.0, 12.0)] {
            let f = double_faces(height, lip);
            let stated = Feature::Double { at: 0.0, height, gap, lip }.length();
            assert!(
                (stated - f.total(gap)).abs() < 1e-3,
                "{stated} against {}",
                f.total(gap)
            );
        }
    }
}

#[cfg(test)]
mod our_own_trh {
    use crate::track::decode_master;
    use crate::trackprog::TrackProgram;
    use std::path::Path;

    /// A track we built reads back the way we wrote it.
    ///
    /// The scenery is placed from the synthesis and the terrain comes from the `.trh`
    /// TerrainEd baked out of our own heightmap. If the two disagree about which way round
    /// the ground goes, the props land mirrored against it — trees on the riding line.
    #[test]
    #[ignore = "needs a built track — set FROST_TRACK and FROST_PROGRAM"]
    fn a_built_track_reads_back_the_way_we_wrote_it() {
        let track = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let prog_path = std::env::var("FROST_PROGRAM").expect("set FROST_PROGRAM");
        let prog: TrackProgram =
            serde_json::from_str(&std::fs::read_to_string(&prog_path).unwrap()).unwrap();
        let m = decode_master(Path::new(&track)).expect("terrain");
        let (gw, gh) = (m.info.width as usize, m.info.height as usize);
        let mpp = m.info.metres_per_sample;
        // On the track the ground is graded and rutted; off it, it is not. The reading that
        // puts the lap on the rougher ground is the one that matches.
        let rough = |flip: bool| -> f64 {
            let (mut total, mut n) = (0.0f64, 0.0f64);
            for st in prog.stations(6.0) {
                let gx = (st.x / mpp).clamp(1.0, gw as f32 - 2.0) as usize;
                let raw = (st.z / mpp).clamp(1.0, gh as f32 - 2.0) as usize;
                let gz = if flip { gh - 1 - raw } else { raw };
                let h = |a: usize, b: usize| m.heights[b.min(gh - 1) * gw + a.min(gw - 1)];
                total += ((h(gx + 1, gz) - h(gx - 1, gz)).abs()
                    + (h(gx, gz + 1) - h(gx, gz - 1)).abs()) as f64;
                n += 1.0;
            }
            total / n.max(1.0)
        };
        let (straight, flipped) = (rough(false), rough(true));
        println!("  as written {straight:.4}   flipped {flipped:.4}");
        assert!(
            straight > flipped,
            "the built terrain is mirrored against the synthesis that placed its scenery"
        );
    }
}
