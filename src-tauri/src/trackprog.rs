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

fn default_texture() -> f32 {
    0.06
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

impl Segment {
    pub fn length(&self) -> f32 {
        match self {
            Segment::Straight { length, .. } => *length,
            Segment::Arc { radius, angle, .. } => radius.abs() * angle.abs().to_radians(),
        }
    }
}

/// Something built on the riding line, placed by how far round the lap it is.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
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
}

fn default_lip() -> f32 {
    6.0
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
            | Feature::Rut { at, .. } => *at,
        }
    }

    /// How much of the lap it occupies.
    pub fn length(&self) -> f32 {
        match self {
            Feature::Tabletop { length, .. }
            | Feature::Roller { length, .. }
            | Feature::StepUp { length, .. }
            | Feature::Berm { length, .. }
            | Feature::Rut { length, .. } => *length,
            // Two faces up and two back down, with the gap between them.
            Feature::Double { gap, lip, .. } => (lip + lip.min(5.0)) * 2.0 + gap,
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
        }
    }
}

/// A worked example of a track program: what a good one looks like.
///
/// Point-symmetric — the same half-lap twice, each turning 180°, so it closes on itself
/// exactly whatever the straights are doing. Built to the corpus: 13 m wide, jumps standing
/// about 1.2 m off the ground and spaced about 16 m apart, jumps on the straights and berms
/// in the corners.
///
/// This is the schema's own test. It is parsed by the test suite, synthesised, and measured
/// against published tracks, so it cannot drift away from what the code accepts.
pub const EXAMPLE: &str = r#"{
      "name": "Corpus National",
      "author": "MXB App",
      "location": "Generated",
      "width": 13.0,
      "terrain": {
        "sizeX": 700.0, "sizeZ": 700.0, "samples": 2049, "scale": 45.0,
        "relief": { "amplitude": 11.0, "wavelength": 210.0, "seed": 7 }
      },
      "start": { "x": 180.0, "z": 150.0, "angle": 0.0 },
      "segments": [
        { "kind": "straight", "length": 230.0 },
        { "kind": "arc", "radius": 25.0, "angle": 90.0 },
        { "kind": "straight", "length": 80.0 },
        { "kind": "arc", "radius": -18.0, "angle": 75.0 },
        { "kind": "straight", "length": 120.0 },
        { "kind": "arc", "radius": 22.0, "angle": 100.0 },
        { "kind": "straight", "length": 70.0 },
        { "kind": "arc", "radius": -15.0, "angle": 65.0 },
        { "kind": "straight", "length": 90.0 },
        { "kind": "arc", "radius": 20.0, "angle": 130.0 },
        { "kind": "straight", "length": 230.0 },
        { "kind": "arc", "radius": 25.0, "angle": 90.0 },
        { "kind": "straight", "length": 80.0 },
        { "kind": "arc", "radius": -18.0, "angle": 75.0 },
        { "kind": "straight", "length": 120.0 },
        { "kind": "arc", "radius": 22.0, "angle": 100.0 },
        { "kind": "straight", "length": 70.0 },
        { "kind": "arc", "radius": -15.0, "angle": 65.0 },
        { "kind": "straight", "length": 90.0 },
        { "kind": "arc", "radius": 20.0, "angle": 130.0 }
      ],
      "features": [
        { "kind": "tabletop", "at": 25.0, "length": 22.0, "height": 1.5 },
        { "kind": "roller", "at": 59.0, "length": 15.0, "height": 0.85 },
        { "kind": "tabletop", "at": 86.0, "length": 26.0, "height": 1.6 },
        { "kind": "double", "at": 124.0, "height": 1.4, "gap": 11.0, "lip": 6.5 },
        { "kind": "roller", "at": 170.0, "length": 13.0, "height": 0.7 },
        { "kind": "tabletop", "at": 195.0, "length": 20.0, "height": 1.3 },
        { "kind": "berm", "at": 232.0, "length": 37.0, "height": 1.7 },
        { "kind": "tabletop", "at": 278.0, "length": 22.0, "height": 1.5 },
        { "kind": "stepUp", "at": 300.0, "length": 30.0, "height": 3.0 },
        { "kind": "roller", "at": 312.0, "length": 15.0, "height": 0.85 },
        { "kind": "berm", "at": 351.0, "length": 21.0, "height": 1.7 },
        { "kind": "double", "at": 382.0, "height": 1.4, "gap": 11.0, "lip": 6.5 },
        { "kind": "roller", "at": 428.0, "length": 13.0, "height": 0.7 },
        { "kind": "whoops", "at": 452.0, "count": 5, "spacing": 6.0, "height": 0.7 },
        { "kind": "tabletop", "at": 453.0, "length": 20.0, "height": 1.3 },
        { "kind": "berm", "at": 495.0, "length": 34.0, "height": 1.7 },
        { "kind": "roller", "at": 540.0, "length": 15.0, "height": 0.85 },
        { "kind": "tabletop", "at": 567.0, "length": 26.0, "height": 1.6 },
        { "kind": "berm", "at": 603.0, "length": 15.0, "height": 1.7 },
        { "kind": "double", "at": 628.0, "height": 1.4, "gap": 11.0, "lip": 6.5 },
        { "kind": "stepUp", "at": 640.0, "length": 30.0, "height": -3.0 },
        { "kind": "roller", "at": 674.0, "length": 13.0, "height": 0.7 },
        { "kind": "berm", "at": 710.0, "length": 42.0, "height": 1.7 },
        { "kind": "tabletop", "at": 778.5, "length": 22.0, "height": 1.5 },
        { "kind": "roller", "at": 812.5, "length": 15.0, "height": 0.85 },
        { "kind": "tabletop", "at": 839.5, "length": 26.0, "height": 1.6 },
        { "kind": "double", "at": 877.5, "height": 1.4, "gap": 11.0, "lip": 6.5 },
        { "kind": "roller", "at": 923.5, "length": 13.0, "height": 0.7 },
        { "kind": "tabletop", "at": 948.5, "length": 20.0, "height": 1.3 },
        { "kind": "berm", "at": 985.5, "length": 37.0, "height": 1.7 },
        { "kind": "tabletop", "at": 1031.5, "length": 22.0, "height": 1.5 },
        { "kind": "stepUp", "at": 1053.5, "length": 30.0, "height": 3.0 },
        { "kind": "roller", "at": 1065.5, "length": 15.0, "height": 0.85 },
        { "kind": "berm", "at": 1104.5, "length": 21.0, "height": 1.7 },
        { "kind": "double", "at": 1135.5, "height": 1.4, "gap": 11.0, "lip": 6.5 },
        { "kind": "roller", "at": 1181.5, "length": 13.0, "height": 0.7 },
        { "kind": "whoops", "at": 1205.5, "count": 5, "spacing": 6.0, "height": 0.7 },
        { "kind": "tabletop", "at": 1206.5, "length": 20.0, "height": 1.3 },
        { "kind": "berm", "at": 1248.5, "length": 34.0, "height": 1.7 },
        { "kind": "roller", "at": 1293.5, "length": 15.0, "height": 0.85 },
        { "kind": "tabletop", "at": 1320.5, "length": 26.0, "height": 1.6 },
        { "kind": "berm", "at": 1356.5, "length": 15.0, "height": 1.7 },
        { "kind": "double", "at": 1381.5, "height": 1.4, "gap": 11.0, "lip": 6.5 },
        { "kind": "stepUp", "at": 1393.5, "length": 30.0, "height": -3.0 },
        { "kind": "roller", "at": 1427.5, "length": 13.0, "height": 0.7 },
        { "kind": "berm", "at": 1463.5, "length": 42.0, "height": 1.7 }
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
        p.features[last] = match p.features[last] {
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
