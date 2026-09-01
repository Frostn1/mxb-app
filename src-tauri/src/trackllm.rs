//! Asking a model for a track, and refusing to believe it.
//!
//! The model's whole job is to write a [`TrackProgram`] — a start pose, a run of straights and
//! arcs, and the jumps laid along them. It never sees a heightmap. That keeps the part a
//! language model is good at (what a lap should *be*) apart from the part it is bad at (four
//! million samples), and it means a bad answer is a short document you can read rather than a
//! terrain you have to look at.
//!
//! Nothing it returns is trusted. Every program is put through the synthesiser and then
//! **measured with the same code that measured the published tracks**, and anything landing
//! outside what real tracks do goes back with the numbers attached. A model told "corridor
//! width 31 m, published tracks run 10–17" fixes it; one told "invalid" does not.
//!
//! The API key lives in the control plane, never here — see `control-plane/src/trackgen.ts`,
//! which owns the system prompt and the schema so a stolen app token can only ever be spent
//! on generating tracks.

#![allow(dead_code)]

use anyhow::{bail, Context, Result};

use crate::trackprog::{Feature, Segment, TrackProgram};
use crate::tracksynth;

/// Samples a check builds at. A power of two plus one, like every other grid here.
///
/// Sixteen times less work than 2049, and everything measured off it — corridor width, how
/// connected it is, how steep the ground gets — survives the reduction. What wouldn't is the
/// fine texture, and no check asks about that.
const VALIDATION_SAMPLES: u32 = 513;

/// What published tracks measure, from `trackstats` over the installed corpus. These are the
/// numbers a generated track is held to, and the ones the prompt quotes.
pub mod corpus {
    /// Riding line, metres. Measured 10.0–17.1 across the tracks that read cleanly.
    pub const WIDTH_M: (f32, f32) = (8.0, 20.0);
    /// A lap. Measured 1299–1767 m; the bounds are wider because a supercross-style track is
    /// a legitimate thing to ask for and none of the corpus is one.
    pub const LAP_M: (f32, f32) = (500.0, 2600.0);
    /// How far a jump stands off the landscape under it. Measured p90 1.08–1.34 m.
    pub const RELIEF_M: (f32, f32) = (0.5, 2.0);
    /// The steepest ground on the riding line. Measured p99 27.0–40.9°.
    pub const SLOPE_P99_DEG: (f32, f32) = (18.0, 50.0);
    /// Jumps per kilometre of lap.
    ///
    /// Ten to twenty-five, counted along ten published tracks' own centrelines — not the
    /// 29–61 the corridor rule reports, which counts every roughness peak on the ground as a
    /// takeoff. A national is not a washboard: Indiana has forty features on a 2.2 km lap and
    /// only fourteen of them stand over a metre.
    pub const LIPS_PER_KM: (f32, f32) = (12.0, 45.0);
    /// One jump's height above the ground it sits on, as the program states it.
    ///
    /// Up to five metres because published ones are: measured lips top out at 3.0–5.9 m per
    /// track, and a lap whose biggest jump is 1.6 m has no big jump on it.
    pub const FEATURE_HEIGHT_M: (f32, f32) = (0.3, 5.0);
    /// Between the crests of a whoop section.
    pub const WHOOP_SPACING_M: (f32, f32) = (2.5, 8.0);
    /// A corner tight enough to need a berm, and one loose enough not to be a corner.
    pub const CORNER_RADIUS_M: (f32, f32) = (8.0, 200.0);
    /// How far the finish may miss the start before the lap isn't one.
    pub const CLOSURE_M: f32 = 20.0;

    // The layout, read out of published tracks' own centrelines. A `.trh` carries the `.tcl`
    // that built it, so these are not inferred from the ground — they are the segments the
    // builder typed. See `trackline`.

    /// Segments in a lap. Measured 52–150.
    pub const SEGMENTS: (f32, f32) = (30.0, 200.0);
    /// How much of a lap is arcs rather than straights. Measured 0.61–0.91.
    ///
    /// This is the single biggest thing a generated lap gets wrong. A published track is a
    /// chain of arcs with a few straights in it — Indiana runs 109 arcs against 11 straights
    /// — and a lap of long straights joined by corners is a shape, not a circuit.
    pub const ARC_FRACTION: (f32, f32) = (0.5, 1.0);
    /// Every degree a lap turns through, both ways added up. Measured 1726–2960°.
    ///
    /// Not the signed sum, which is ±360 on anything that closes. This is what says a lap
    /// wanders: at 900° it is a rounded rectangle, and no published track is under 1700.
    pub const TOTAL_TURN_DEG: (f32, f32) = (1400.0, 3600.0);
    /// Corners of 25° or more — runs of same-way arcs, which is what a rider meets. Measured
    /// 13–25 per lap.
    pub const TURNS: (f32, f32) = (10.0, 30.0);
    /// The tightest radius inside a turn, at the median. Measured 10.6–18.5 m.
    pub const TURN_RADIUS_M: (f32, f32) = (7.0, 30.0);
    /// Highest ground on the lap over lowest. Measured 2.2–66.3 m, and the spread is the
    /// point: Lambretta Lynds climbs 2 m and Millville 66. A track on a hillside rides
    /// nothing like a track on a field, and both are normal.
    pub const LAP_CLIMB_M: (f32, f32) = (2.0, 70.0);
    /// Steepest grade along the riding line at the ninetieth. Measured 9.1–22.5°.
    pub const GRADE_P90_DEG: (f32, f32) = (7.0, 26.0);
}

/// One round trip: what the model was last told, and what was wrong with what it sent.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    /// The program that failed, verbatim, so the model edits rather than starts again.
    pub previous: Option<String>,
    pub problems: Vec<String>,
}

/// Where a track program comes from. A trait so the loop can be tested without a network or
/// a key — the failure modes worth testing are all in what comes *back*.
pub trait Ask {
    /// The brief, plus whatever went wrong last time. Returns the model's JSON, unparsed.
    fn ask(&self, brief: &str, attempt: &Attempt)
        -> impl std::future::Future<Output = Result<String>>;
}

/// Where the lap runs over its own ground, if it does.
///
/// Returns the two distances round the lap and how close they come. Nothing else caught this:
/// the lap closed, it fitted the plot, every feature measured, and the track still crossed
/// itself twice — because "does this shape overlap itself" is not a question any of the other
/// checks ask.
///
/// It matters more here than it would on a drawing. The synthesiser has no concept of a
/// bridge: it benches whatever the centreline passes over, so where a lap crosses, both
/// passes are graded into the same cells and what comes out is a scar — one pass cutting
/// through the other's jumps, with the ground fighting over which height it should be.
///
/// Not repairable, so it goes to the model rather than to `repair`. Uncrossing a lap means
/// re-routing it, which is the layout itself and the one thing here that is genuinely a
/// design decision.
fn self_crossing(prog: &TrackProgram) -> Option<(f32, f32, f32)> {
    let st = prog.stations(2.0);
    if st.len() < 8 {
        return None;
    }
    let lap = prog.lap_length().max(1.0);
    // Touching, not merely near: two ribbons this far apart centre-to-centre are already
    // sharing dirt.
    let near = prog.width * 1.1;
    // How far apart along the lap two points must be before their nearness means anything.
    // Consecutive stations are always close together, and so are the two sides of a hairpin,
    // which is a real shape and not a fault — it takes a corner's own length to come back on
    // yourself, so the bar is a few of them.
    let apart = (prog.width * 6.0).max(60.0);
    let mut worst: Option<(f32, f32, f32)> = None;
    for (i, a) in st.iter().enumerate() {
        for b in st.iter().skip(i + 1) {
            // Round the lap, not along it — the finish line is not a discontinuity, and
            // measuring it as one makes every track "cross itself" at the start.
            let along = (b.s - a.s).abs();
            if along.min(lap - along) < apart {
                continue;
            }
            let gap = ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if gap < near && worst.map(|(.., w)| gap < w).unwrap_or(true) {
                worst = Some((a.s, b.s, gap));
            }
        }
    }
    worst
}

/// Which segment a distance round the lap falls in.
fn segment_at(prog: &TrackProgram, at: f32) -> usize {
    let mut run = 0.0f32;
    for (i, seg) in prog.segments.iter().enumerate() {
        run += seg.length();
        if at <= run {
            return i;
        }
    }
    prog.segments.len().saturating_sub(1)
}

/// Whether a berm at this distance round the lap is on a corner.
///
/// One definition, used by both the check and the repair. They had one each, they disagreed,
/// and the disagreement was invisible until a berm sat in the gap between them.
fn berm_on_a_corner(turn: &[crate::trackprog::Station], at: f32) -> bool {
    turn.iter()
        .find(|s| s.s >= at)
        .map(|s| s.curvature != 0.0)
        .unwrap_or(false)
}

/// Everything wrong with a program that we can simply work out, worked out.
///
/// Three of the complaints the validator used to send back were not judgement at all — they
/// were arithmetic, and we have exact code for each of them. Handing them to a language model
/// and asking it to try again is both the slowest way to fix them and the least reliable:
/// closing a lap means making the signed turn angles sum to a whole number of circles *and*
/// the straights bring it home, which a small model gets wrong over and over. Pointed at
/// Haiku 4.5, three rounds of that failed every time on lap closure alone.
///
/// So the loop repairs first and complains second. What reaches the model afterwards is the
/// part it can actually act on — the track is too flat, there are not enough jumps, the
/// corners are too tight — rather than a sum it cannot do.
///
/// Returns a line per repair, for the log. Nothing here can fail: each fix either applies or
/// is not needed, and anything left over is the validator's business.
fn repair(prog: &mut TrackProgram) -> Vec<String> {
    let mut done = Vec::new();

    // 1. Put the lap on the ground, and make the cells square, which is one problem and
    //    not two.
    //
    //    They were two steps and they fought: fitting the plot to the lap and then squaring
    //    the cells let the squaring shrink a side back below what the lap needed, and the
    //    check reported a lap leaving a 525 x 1050 m plot — a ratio that is exactly what the
    //    snapping produces. The synthesiser sizes the short side of the grid to a power of
    //    two plus one, so the plot's two jobs — hold the lap, and divide into square cells —
    //    only have a common answer if you look for it.
    //
    //    Solved by iterating: grow to fit, square by growing, and if squaring would leave a
    //    side short, grow the *other* side instead and go round again. The snapping means
    //    this steps rather than slides, so it needs a few passes, and it converges in two or
    //    three. Whatever it lands on, the lap's shape is untouched — only the ground under it
    //    changes size, and the start pose slides to centre the lap on it.
    {
        let st = prog.stations(2.0);
        if !st.is_empty() {
            let margin = prog.width.max(1.0) * 1.5;
            let (mut lo_x, mut hi_x) = (f32::MAX, f32::MIN);
            let (mut lo_z, mut hi_z) = (f32::MAX, f32::MIN);
            for s in &st {
                lo_x = lo_x.min(s.x);
                hi_x = hi_x.max(s.x);
                lo_z = lo_z.min(s.z);
                hi_z = hi_z.max(s.z);
            }
            let need_x = (hi_x - lo_x) + margin * 2.0;
            let need_z = (hi_z - lo_z) + margin * 2.0;
            let (was_x, was_z) = (prog.terrain.size_x, prog.terrain.size_z);
            let (mut sx, mut sz) = (was_x.max(need_x), was_z.max(need_z));

            for _ in 0..8 {
                let mut probe = prog.clone();
                probe.terrain.size_x = sx;
                probe.terrain.size_z = sz;
                let Ok((gw, gh)) = crate::tracksynth::grid_for(&probe) else {
                    break;
                };
                let (gw, gh) = ((gw.max(2) - 1) as f32, (gh.max(2) - 1) as f32);
                let (want_x, want_z) = if sx >= sz {
                    (sx, sx / gw * gh)
                } else {
                    (sz / gh * gw, sz)
                };
                // Square, and still big enough: done.
                if want_x >= need_x && want_z >= need_z {
                    sx = want_x;
                    sz = want_z;
                    break;
                }
                // Squaring would cut into the lap. Grow the long side until it doesn't.
                let short = (need_x / want_x.max(1.0)).max(need_z / want_z.max(1.0));
                if sx >= sz {
                    sx *= short.max(1.02);
                } else {
                    sz *= short.max(1.02);
                }
            }

            // Centre the lap on whatever ground we settled on.
            let dx = sx * 0.5 - (lo_x + hi_x) * 0.5;
            let dz = sz * 0.5 - (lo_z + hi_z) * 0.5;
            let grew = (sx - was_x).abs() > 0.5 || (sz - was_z).abs() > 0.5;
            if grew || dx.abs() > 0.5 || dz.abs() > 0.5 {
                if grew {
                    done.push(format!(
                        "ground {was_x:.0}x{was_z:.0} m becomes {sx:.0}x{sz:.0}, cells square"
                    ));
                }
                if dx.abs() > 0.5 || dz.abs() > 0.5 {
                    done.push(format!("start moves by ({dx:.0}, {dz:.0}) m to centre the lap"));
                }
                prog.terrain.size_x = sx;
                prog.terrain.size_z = sz;
                prog.start.x += dx;
                prog.start.z += dz;
            }
        }
    }

    // 2. Close the lap, with a turn no tighter than the tightest already on it — the same
    //    rule the studio's own "Close the lap" button uses.
    let closure = prog.closure_error();
    if closure > corpus::CLOSURE_M {
        let radius = prog
            .segments
            .iter()
            .filter_map(|s| match s {
                crate::trackprog::Segment::Arc { radius, .. } => Some(radius.abs()),
                _ => None,
            })
            .fold(f32::MAX, f32::min);
        let radius = if radius.is_finite() { radius } else { 25.0 };
        if let Some(add) = prog.closing_segments(radius) {
            let n = add.len();
            prog.segments.extend(add);
            done.push(format!(
                "closed the lap: {closure:.0} m gap shut with {n} segment(s), now {:.2} m",
                prog.closure_error()
            ));
        }
    }

    // 3. Put berms on corners. A berm on a straight silently does nothing, and it is never
    //    what was meant — a model that asks for one has decided the corner wants banking and
    //    then got the distance round the lap wrong. Where the corners are is not a matter of
    //    opinion, so slide it to the nearest one rather than sending the whole program back.
    let turn = prog.stations(1.0);
    let corner_at = |at: f32| -> Option<f32> {
        turn.iter()
            .filter(|st| st.curvature != 0.0)
            .min_by(|a, b| (a.s - at).abs().total_cmp(&(b.s - at).abs()))
            .map(|st| st.s)
    };
    for f in &mut prog.features {
        let crate::trackprog::Feature::Berm { at, .. } = f else {
            continue;
        };
        // `berm_on_a_corner` and nothing else. The first version of this asked whether the
        // berm *reached* a corner anywhere along its length, which is a laxer question than
        // the one the validator asks — so a berm starting 19 m short of a turn passed the
        // repair and was then rejected, and the loop spent every remaining attempt on a
        // fault it had already decided was fine.
        if berm_on_a_corner(&turn, *at) {
            continue;
        }
        if let Some(to) = corner_at(*at) {
            done.push(format!("moved the berm at {at:.0} m onto the corner at {to:.0} m"));
            *at = to;
        }
    }

    // 4. Fit the height budget. It exists only because samples are quantised against it, and
    //    it is a number the synthesiser already knows — there was never a reason to make the
    //    model guess it and then be told off for guessing wrong.
    if let Ok(fitted) = crate::tracksynth::with_fitted_budget(prog) {
        if (fitted.terrain.scale - prog.terrain.scale).abs() > 0.5 {
            done.push(format!(
                "fitted the height budget: {:.0} m becomes {:.0}",
                prog.terrain.scale, fitted.terrain.scale
            ));
            prog.terrain.scale = fitted.terrain.scale;
        }
    }

    done
}

/// Ask for a track, and keep asking until it measures like one.
///
/// `tries` counts total attempts, not retries. Two is the useful minimum: models get the
/// shape right and the *numbers* wrong, and the numbers are exactly what a measured
/// complaint fixes.
/// What an answer starts with when the service is reporting the model's mistake rather than
/// its own. Not a track, and not a failure either — a thing to send back and have fixed.
const REJECTED: &str = "\u{0}rejected\u{0}";

pub async fn generate(brief: &str, ask: &impl Ask, tries: usize) -> Result<TrackProgram> {
    let mut attempt = Attempt::default();
    let mut last: Option<String> = None;

    for round in 0..tries.max(1) {
        let raw = ask
            .ask(brief, &attempt)
            .await
            .with_context(|| format!("asking for a track (attempt {})", round + 1))?;

        if let Some(why) = raw.strip_prefix(REJECTED) {
            log::info!("[trackllm] attempt {} was rejected: {why}", round + 1);
            attempt = Attempt {
                previous: attempt.previous.clone(),
                problems: vec![why.to_string()],
            };
            last = Some(why.to_string());
            continue;
        }

        match serde_json::from_str::<TrackProgram>(&raw) {
            Ok(mut prog) => {
                // Fix what arithmetic can fix before complaining about it. See `repair`.
                for fixed in repair(&mut prog) {
                    log::info!("[trackllm] {fixed}");
                }
                let problems = validate(&prog);
                if problems.is_empty() {
                    return Ok(prog);
                }
                // Each one, not just how many. A run that ends "gave up after 4 attempts"
                // with four lines saying `came back with 1 problems` records nothing about
                // why — and the one line that does carry the reason goes to the caller, so
                // it is gone as soon as the dialog is dismissed.
                log::info!(
                    "[trackllm] attempt {} came back with {} problem(s):",
                    round + 1,
                    problems.len()
                );
                for p in &problems {
                    log::info!("[trackllm]   - {p}");
                }
                attempt = Attempt {
                    previous: Some(raw.clone()),
                    problems,
                };
            }
            Err(e) => {
                // A schema violation is the model's to fix too, and saying which field
                // beats saying "invalid JSON".
                log::info!("[trackllm] attempt {} didn't parse: {e}", round + 1);
                attempt = Attempt {
                    previous: Some(raw.clone()),
                    problems: vec![format!("that didn't parse as a track program: {e}")],
                };
            }
        }
        last = Some(attempt.problems.join("; "));
    }

    log::warn!(
        "[trackllm] gave up after {} attempts; last: {}",
        tries.max(1),
        last.clone().unwrap_or_else(|| "no answer".into())
    );
    bail!(
        "gave up after {} attempts — last time: {}",
        tries.max(1),
        last.unwrap_or_else(|| "no answer".into())
    )
}

/// What is wrong with a program, and what is merely unlike a published one.
///
/// The difference matters: a blank lap with nothing built on it is not broken, it is empty,
/// and telling someone their brand new track has "0.0 features per km" as though it were a
/// fault is how a starting point stops being one. Problems block; notes do not.
#[derive(serde::Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    /// Structural: it won't build, or it isn't a lap, or a feature does nothing where it is.
    pub problems: Vec<String>,
    /// It builds and it is a lap — it just doesn't measure like the tracks people ride.
    pub notes: Vec<String>,
}

/// Everything wrong with a program, said the way the model needs to hear it.
///
/// Each problem names the measurement, the value, and what published tracks do. That last
/// part is what makes it fixable: "too wide" is an opinion, "31 m against a corpus of 10–17"
/// is an instruction.
pub fn validate(prog: &TrackProgram) -> Vec<String> {
    let r = review(prog);
    r.problems.into_iter().chain(r.notes).collect()
}

pub fn review(prog: &TrackProgram) -> Review {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let between = |what: &str, v: f32, (lo, hi): (f32, f32), unit: &str, out: &mut Vec<String>| {
        if v < lo || v > hi {
            out.push(format!(
                "{what} is {v:.1}{unit}; published tracks run {lo:.0}–{hi:.0}{unit}"
            ));
        }
    };

    // The structural checks come first and stop everything: a lap that leaves the terrain
    // can't be synthesised, so there would be nothing to measure.
    if let Err(e) = prog.check() {
        out.push(e.to_string());
        return Review { problems: out, notes };
    }

    let closure = prog.closure_error();
    if closure > corpus::CLOSURE_M {
        out.push(format!(
            "the lap doesn't close: the finish is {closure:.0} m from the start. The turns have \
             to add up to a whole number of full circles — check that the signed angles sum to \
             ±360°."
        ));
    }
    if let Some((a, b, gap)) = self_crossing(prog) {
        // Named in the model's own terms. It wrote a list of segments, not a distance round
        // a lap, so "1632 m comes within 0 m of 2066 m" makes it do the dead reckoning it is
        // already bad at just to find out which two lines to change. Saying "segment 7 runs
        // over segment 11" points at the edit.
        let (i, j) = (segment_at(prog, a), segment_at(prog, b));
        let name = |k: usize| match prog.segments.get(k) {
            Some(crate::trackprog::Segment::Straight { length, .. }) => {
                format!("segment {k} (the {length:.0} m straight)")
            }
            Some(crate::trackprog::Segment::Arc { radius, angle, .. }) => format!(
                "segment {k} (the {:.0}° {} of radius {:.0} m)",
                angle.abs(),
                if *radius >= 0.0 { "right" } else { "left" },
                radius.abs()
            ),
            None => format!("segment {k}"),
        };
        out.push(format!(
            "the lap crosses itself: {} runs within {gap:.0} m of {}, and the track is \
             {:.0} m wide. Two parts of a lap cannot share the same ground. Shorten whichever \
             of the two overshoots, or turn earlier so they miss — and remember the lap still \
             has to close afterwards.",
            name(i),
            name(j),
            prog.width
        ));
    }
    between("the riding line", prog.width, corpus::WIDTH_M, " m", &mut notes);
    between("the lap", prog.lap_length(), corpus::LAP_M, " m", &mut notes);

    // Only worth saying once there is something to count. A lap with nothing built on it is
    // The layout, against what published tracks' own centrelines say. Notes rather than
    // problems: a lap of six straights and four corners builds and rides, it just isn't
    // shaped like anything anybody has released.
    let arcs = prog
        .segments
        .iter()
        .filter(|s| matches!(s, Segment::Arc { radius, angle, .. }
            if *radius != 0.0 && angle.abs() >= crate::trackprog::CORNER_DEG))
        .count();
    let turning: f32 = prog
        .segments
        .iter()
        .map(|s| match s {
            Segment::Arc { angle, .. } => angle.abs(),
            _ => 0.0,
        })
        .sum();
    let corners = crate::trackprog::turns(&prog.segments);
    let real: Vec<&(f32, f32)> = corners.iter().filter(|t| t.0 >= 25.0).collect();
    between("the lap", prog.segments.len() as f32, corpus::SEGMENTS, " segments", &mut notes);
    if !prog.segments.is_empty() {
        let fraction = arcs as f32 / prog.segments.len() as f32;
        if fraction < corpus::ARC_FRACTION.0 {
            notes.push(format!(
                "the lap is {arcs} arcs and {} straights — {:.0}% arcs, where published tracks                  run 61–91%. A circuit is a chain of corners with a few straights in it, not                  straights joined by corners: Indiana is 109 arcs against 11 straights.",
                prog.segments.len() - arcs,
                fraction * 100.0
            ));
        }
    }
    between("the lap's total turning", turning, corpus::TOTAL_TURN_DEG, "°", &mut notes);
    // How much of a hill it is. A generated lap lands on flat by default — the landscape
    // amplitude is the only thing that decides it, and a track on a field rides nothing like
    // a track on a hillside. Half the published corpus climbs more than 20 m.
    between(
        "the landscape's amplitude",
        prog.terrain.relief.amplitude,
        (4.0, 30.0),
        " m",
        &mut notes,
    );
    between("the lap", real.len() as f32, corpus::TURNS, " corners", &mut notes);
    if !real.is_empty() {
        let mut r: Vec<f32> = real.iter().map(|t| t.1).collect();
        r.sort_by(f32::total_cmp);
        between(
            "the median corner's tightest radius",
            r[r.len() / 2],
            corpus::TURN_RADIUS_M,
            " m",
            &mut notes,
        );
    }

    // a starting point, and "0 features per km" is not news to whoever just asked for one.
    if !prog.features.is_empty() {
        let per_km = prog.features.len() as f32 * 1000.0 / prog.lap_length().max(1.0);
        between("feature density", per_km, corpus::LIPS_PER_KM, " per km", &mut notes);
    }

    // Per-feature, where the complaint can name the thing that's wrong.
    let turn = prog.stations(1.0);
    for f in &prog.features {
        let h = f.height().abs();
        if h < corpus::FEATURE_HEIGHT_M.0 || h > corpus::FEATURE_HEIGHT_M.1 {
            out.push(format!(
                "a feature at {:.0} m stands {h:.1} m; jumps run {:.1}–{:.1} m",
                f.at(),
                corpus::FEATURE_HEIGHT_M.0,
                corpus::FEATURE_HEIGHT_M.1
            ));
        }
        if let Feature::Whoops { at, spacing, .. } = f {
            if *spacing < corpus::WHOOP_SPACING_M.0 || *spacing > corpus::WHOOP_SPACING_M.1 {
                out.push(format!(
                    "the whoops at {at:.0} m are {spacing:.1} m apart; whoops run {:.1}–{:.1} m",
                    corpus::WHOOP_SPACING_M.0,
                    corpus::WHOOP_SPACING_M.1
                ));
            }
        }
        // A berm is a banked wall on the outside of a corner. On a straight there is no
        // outside, so it silently does nothing — which reads as the synthesiser dropping it.
        if let Feature::Berm { at, .. } = f {
            if !berm_on_a_corner(&turn, *at) {
                // Say which corner, not just "a corner". This turns up after an edit moves
                // the corners out from under a berm that was on one, and the way out is a
                // number.
                let nearest = turn
                    .iter()
                    .filter(|s| s.curvature != 0.0)
                    .min_by(|a, b| {
                        (a.s - at).abs().total_cmp(&(b.s - at).abs())
                    })
                    .map(|s| format!(" The nearest corner starts around {:.0} m.", s.s))
                    .unwrap_or_else(|| " This lap has no corners at all.".into());
                out.push(format!(
                    "the berm at {at:.0} m is on a straight, where it does nothing — a berm \
                     banks the outside of a corner.{nearest}"
                ));
            }
        }
    }

    // Then the real test: build it, and measure what came out.
    //
    // At a quarter of the samples the track ships with. This runs on every edit — every drag
    // of a curve point, every nudge of a jump's height — and a full-resolution synthesis is
    // most of a second of work to answer a question that only needs to know whether the
    // corridor holds together and roughly how steep it is. The terrain that gets written is
    // still built at full size.
    let mut coarse = prog.clone();
    coarse.terrain.samples = prog.terrain.samples.min(VALIDATION_SAMPLES);
    let syn = match tracksynth::synthesise(&coarse) {
        Ok(s) => s,
        Err(e) => {
            out.push(e.to_string());
            return Review { problems: out, notes };
        }
    };
    let c = crate::trackstats::measure("synth", &syn.corridor, &syn.heights, syn.gw, syn.gh, syn.mps);

    // Both of these measure what was *built* on the ground, so an empty lap has nothing to
    // say about them either.
    if !prog.features.is_empty() {
        between("the built relief", c.feature_relief_m.p90, corpus::RELIEF_M, " m", &mut notes);
        between("the steepest ground", c.slope_deg.p99, corpus::SLOPE_P99_DEG, "°", &mut notes);
    }
    if c.largest_component_fraction < 0.95 {
        out.push(format!(
            "the riding line comes out in {:.0}% pieces — the lap probably crosses itself",
            (1.0 - c.largest_component_fraction) * 100.0
        ));
    }
    Review { problems: out, notes }
}

/// The brief, as the app sends it. Kept small on purpose: everything that shapes the output
/// beyond this is the control plane's system prompt, which the app can't reach.
#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GenerateRequest<'a> {
    brief: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    problems: Vec<String>,
}

#[derive(serde::Deserialize)]
struct GenerateResponse {
    program: Option<serde_json::Value>,
    error: Option<String>,
}

/// The real transport: our control plane, which holds the key and the system prompt.
pub struct ControlPlane {
    pub base: String,
    pub token: String,
}

impl Ask for ControlPlane {
    async fn ask(&self, brief: &str, attempt: &Attempt) -> Result<String> {
        let body = GenerateRequest {
            brief,
            previous: attempt.previous.as_deref(),
            problems: attempt.problems.clone(),
        };
        // Generously long: the model thinks before it writes, and a lap is a few thousand
        // tokens of output.
        let res = reqwest::Client::new()
            .post(format!("{}/v1/track/generate", self.base.trim_end_matches('/')))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
            .context("couldn't reach the track service")?;

        let status = res.status();
        let text = res.text().await.context("reading the track service's answer")?;
        if !status.is_success() {
            // A 4xx carries the reason in its body, which is more use than the status.
            let why = serde_json::from_str::<GenerateResponse>(&text)
                .ok()
                .and_then(|g| g.error)
                .unwrap_or(text);
            // 422 means the model answered and the answer was wrong — a made-up feature
            // kind, a refused brief. That is a thing to send back and have fixed, so it
            // comes back as an unparseable answer and the loop spends another attempt on
            // it. Everything else — no key, rate limited, service down — is fatal, because
            // asking again would only fail the same way.
            if status.as_u16() == 422 {
                return Ok(format!("{REJECTED}{why}"));
            }
            bail!("the track service answered {}: {why}", status.as_u16());
        }

        let parsed: GenerateResponse =
            serde_json::from_str(&text).context("the track service sent something unreadable")?;
        match (parsed.program, parsed.error) {
            (Some(p), _) => Ok(serde_json::to_string(&p)?),
            (None, Some(e)) => bail!("{e}"),
            (None, None) => bail!("the track service sent no program and no reason"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trackprog::EXAMPLE;
    use std::cell::RefCell;

    /// The loop is async because the transport is. Nothing in these tests actually awaits
    /// anything, so a current-thread runtime is all it takes to drive them.
    fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    /// Answers with whatever it was given, in order, and keeps what it was asked.
    struct Canned {
        answers: RefCell<Vec<String>>,
        seen: RefCell<Vec<Attempt>>,
    }

    impl Canned {
        fn new(answers: &[&str]) -> Self {
            Canned {
                answers: RefCell::new(answers.iter().rev().map(|s| s.to_string()).collect()),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl Ask for Canned {
        async fn ask(&self, _brief: &str, attempt: &Attempt) -> Result<String> {
            self.seen.borrow_mut().push(attempt.clone());
            self.answers
                .borrow_mut()
                .pop()
                .ok_or_else(|| anyhow::anyhow!("asked more times than there are answers"))
        }
    }

    fn tweaked(f: impl Fn(&mut TrackProgram)) -> TrackProgram {
        let mut p: TrackProgram = serde_json::from_str(EXAMPLE).unwrap();
        f(&mut p);
        p
    }

    /// A lap with nothing built on it is a starting point, not a broken track. It must not
    /// come back with problems, or the Blank button hands you an error.
    #[test]
    fn a_blank_lap_is_empty_rather_than_broken() {
        let blank = tweaked(|p| p.features.clear());
        let r = review(&blank);
        assert_eq!(r.problems, Vec::<String>::new(), "a blank lap should build");
        // And nothing about what was built on it, because nothing was.
        assert!(
            !r.notes.iter().any(|n| n.contains("density") || n.contains("built relief")),
            "{:?}",
            r.notes
        );
    }

    #[test]
    fn the_worked_example_has_nothing_wrong_with_it() {
        let p: TrackProgram = serde_json::from_str(EXAMPLE).unwrap();
        assert_eq!(validate(&p), Vec::<String>::new());
    }

    /// The Zod schema in `control-plane/src/trackgen.ts` names every field, and nothing but
    /// this test stops the two drifting apart. A rename here fails loudly rather than
    /// producing a model that confidently writes a field the app throws away.
    #[test]
    fn the_program_serialises_with_the_names_the_schema_uses() {
        let p: TrackProgram = serde_json::from_str(EXAMPLE).unwrap();
        let v = serde_json::to_value(&p).unwrap();
        for key in ["name", "author", "location", "width", "terrain", "start", "segments", "features"] {
            assert!(v.get(key).is_some(), "the schema names `{key}`");
        }
        for key in ["sizeX", "sizeZ", "samples", "scale", "relief"] {
            assert!(v["terrain"].get(key).is_some(), "the schema names `terrain.{key}`");
        }
        for key in ["amplitude", "wavelength", "seed", "texture"] {
            assert!(v["terrain"]["relief"].get(key).is_some(), "`relief.{key}`");
        }
        for key in ["x", "z", "angle"] {
            assert!(v["start"].get(key).is_some(), "`start.{key}`");
        }
        // A lap may open on either — this one starts into a corner. What the check is for is
        // that the tag is one of the two the schema uses, not which one it happens to be.
        assert!(matches!(
            v["segments"][0]["kind"].as_str(),
            Some("straight") | Some("arc")
        ));
        let kinds: Vec<&str> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["kind"].as_str())
            .collect();
        // camelCase on the wire, so a step-up is `stepUp` and not `step_up`.
        assert!(kinds.contains(&"stepUp"), "{kinds:?}");
        assert!(kinds.contains(&"tabletop") && kinds.contains(&"berm"));
    }

    #[test]
    fn a_good_answer_is_taken_first_time() {
        let ask = Canned::new(&[EXAMPLE]);
        let got = block_on(generate("a sandy national", &ask, 3)).unwrap();
        assert_eq!(got.name, "Corpus National");
        assert_eq!(ask.seen.borrow().len(), 1);
    }

    #[test]
    fn a_bad_answer_goes_back_with_the_numbers() {
        let wide = serde_json::to_string(&tweaked(|p| p.width = 31.0)).unwrap();
        let ask = Canned::new(&[&wide, EXAMPLE]);
        let got = block_on(generate("a national", &ask, 3)).unwrap();
        assert_eq!(got.width, 12.0);

        let seen = ask.seen.borrow();
        assert_eq!(seen.len(), 2, "it should have asked twice");
        assert!(seen[0].problems.is_empty(), "the first ask carries nothing");
        let second = &seen[1];
        assert!(second.previous.is_some(), "the model gets its own answer back");
        assert!(
            second.problems.iter().any(|p| p.contains("31.0 m") && p.contains("10–17")
                || p.contains("31.0 m") && p.contains("8–20")),
            "the complaint should carry both the value and the corpus: {:?}",
            second.problems
        );
    }

    #[test]
    fn unparseable_json_is_a_problem_the_model_can_fix() {
        let ask = Canned::new(&["{\"name\": \"half a track\"", EXAMPLE]);
        assert!(block_on(generate("a track", &ask, 2)).is_ok());
        assert!(ask.seen.borrow()[1].problems[0].contains("didn't parse"));
    }

    #[test]
    fn giving_up_says_what_was_wrong_last() {
        let wide = serde_json::to_string(&tweaked(|p| p.width = 31.0)).unwrap();
        let ask = Canned::new(&[&wide, &wide]);
        let err = block_on(generate("a track", &ask, 2)).unwrap_err().to_string();
        assert!(err.contains("gave up after 2"), "{err}");
        assert!(err.contains("riding line"), "{err}");
    }

    #[test]
    fn a_lap_that_doesnt_close_is_caught() {
        // Two thirds of the lap: the corners no longer add up to a full circle and the
        // finish lands a long way from the start. Everything built past the new finish goes
        // with it, so the complaint that comes back is about the lap and not about a jump.
        let p = tweaked(|p| {
            p.segments.truncate(52);
            let lap: f32 = p.segments.iter().map(|s| s.length()).sum();
            p.features.retain(|f| f.at() < lap - 60.0);
        });
        let problems = validate(&p);
        assert!(
            problems.iter().any(|s| s.contains("doesn't close")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_lap_that_runs_over_itself_is_caught() {
        // Out, round more than half a circle, and back across where it came from. Tested on
        // the geometry rather than through `validate`, which returns early on a lap that
        // doesn't close — and a shape built to cross is easier to write than one built to
        // cross *and* close *and* fit its plot.
        let p = tweaked(|p| {
            p.segments = vec![
                crate::trackprog::Segment::Straight { length: 220.0, rise: 0.0 },
                crate::trackprog::Segment::Arc { radius: 30.0, angle: 200.0, rise: 0.0 },
                crate::trackprog::Segment::Straight { length: 220.0, rise: 0.0 },
            ];
        });
        let found = self_crossing(&p);
        assert!(found.is_some(), "a lap that doubles back over itself wasn't seen");
        let (a, b, gap) = found.unwrap();
        assert!(gap < p.width * 1.1, "reported a gap of {gap:.1} m as a crossing");
        assert!(
            (a - b).abs() > 60.0,
            "the two points are {a:.0} m and {b:.0} m round — that is the same place twice"
        );
    }

    #[test]
    fn a_berm_on_a_straight_is_caught() {
        let p = tweaked(|p| {
            // The lap's longest straight, which runs from 21 m to 111 m.
            p.features.push(Feature::Berm {
                at: 60.0,
                length: 20.0,
                height: 1.6,
            })
        });
        let problems = validate(&p);
        assert!(
            problems.iter().any(|s| s.contains("berm at 60 m is on a straight")),
            "{problems:?}"
        );
    }

    #[test]
    fn whoops_a_metre_apart_are_caught() {
        let p = tweaked(|p| {
            p.features.push(Feature::Whoops {
                at: 60.0,
                count: 6,
                spacing: 1.0,
                height: 0.7,
            })
        });
        assert!(
            validate(&p).iter().any(|s| s.contains("1.0 m apart")),
            "{:?}",
            validate(&p)
        );
    }

    #[test]
    fn a_height_budget_too_small_comes_back_as_a_number() {
        let p = tweaked(|p| p.terrain.scale = 2.0);
        let problems = validate(&p);
        assert!(
            problems.iter().any(|s| s.contains("budget") && s.contains("Raise")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_lap_off_the_edge_stops_before_the_synthesiser() {
        let p = tweaked(|p| p.terrain.size_x = 200.0);
        let problems = validate(&p);
        assert_eq!(problems.len(), 1, "structural faults stop everything else");
        assert!(problems[0].contains("leaves the terrain"), "{problems:?}");
    }

    /// The whole loop, against a real control plane and a real model.
    ///
    /// Every other test here runs against a stub, which is what let the request shape be
    /// wrong for months without anyone noticing — the schema compiled to a grammar the API
    /// rejects, and a stub cannot tell you that. This is the one that would have caught it.
    ///
    /// ```text
    /// cd control-plane && npx wrangler dev --port 8787 --local     # ANTHROPIC_API_KEY in .dev.vars
    /// FROST_CP=http://localhost:8787 cargo test -- --ignored --nocapture asks_a_real_model
    /// ```
    ///
    /// Costs real money — a few tenths of a cent per attempt on the model this is pointed at.
    #[test]
    #[ignore = "spends money against a live control plane — set FROST_CP"]
    fn asks_a_real_model() {
        let base = std::env::var("FROST_CP").expect("set FROST_CP to a control plane's base URL");
        let brief = std::env::var("FROST_BRIEF").unwrap_or_else(|_| {
            "A long start straight into a right hairpin, then a rhythm section of three \
             doubles, a sweeping left, and a set of whoops before the finish."
                .into()
        });
        let tries: usize = std::env::var("FROST_TRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        let ask = ControlPlane {
            base,
            // A local `wrangler dev` has no accounts to enroll with, and the endpoint sits
            // above the auth gate for exactly that reason.
            token: String::new(),
        };
        let started = std::time::Instant::now();
        let out = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(generate(&brief, &ask, tries));

        match out {
            Ok(prog) => {
                println!(
                    "{} — {:.0} m lap over {} segments, {} features, closes to {:.2} m, in {:.0}s",
                    prog.name,
                    prog.lap_length(),
                    prog.segments.len(),
                    prog.features.len(),
                    prog.closure_error(),
                    started.elapsed().as_secs_f32()
                );
                assert!(validate(&prog).is_empty(), "a returned track has no problems");

                // `FROST_BUILD=/tmp/out` turns the answer into a folder and a `.pkz` — the
                // whole way from a sentence to something the app can install, in one command.
                if let Ok(dir) = std::env::var("FROST_BUILD") {
                    let dir = std::path::Path::new(&dir);
                    std::fs::create_dir_all(dir).unwrap();
                    let syn = crate::tracksynth::synthesise(&prog).unwrap();
                    let wrote = crate::tracksynth::write_source(&prog, &syn, dir).unwrap();
                    std::fs::write(
                        dir.join("program.json"),
                        serde_json::to_string_pretty(&prog).unwrap(),
                    )
                    .unwrap();
                    let slug: String = prog
                        .name
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '_' })
                        .collect();
                    let pkz = dir.join(format!("{slug}.pkz"));
                    let size = crate::tracksynth::write_pkz(&prog, &syn, &pkz, false).unwrap();
                    println!(
                        "wrote {} source files to {}, and {} ({} bytes)",
                        wrote.len() + 1,
                        dir.display(),
                        pkz.display(),
                        size
                    );
                }
            }
            Err(e) => {
                // Not an assertion failure dressed up: a small model that cannot close a lap
                // in three goes is a real result and the numbers are the useful part.
                panic!("gave up after {tries} attempts in {:.0}s: {e:#}", started.elapsed().as_secs_f32());
            }
        }
    }
}
