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

use crate::trackprog::{Feature, TrackProgram};
use crate::tracksynth;

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
    /// Jumps per kilometre of lap. Measured 29–61.
    pub const LIPS_PER_KM: (f32, f32) = (12.0, 80.0);
    /// One jump's height above the ground it sits on, as the program states it.
    pub const FEATURE_HEIGHT_M: (f32, f32) = (0.3, 4.0);
    /// Between the crests of a whoop section.
    pub const WHOOP_SPACING_M: (f32, f32) = (2.5, 8.0);
    /// A corner tight enough to need a berm, and one loose enough not to be a corner.
    pub const CORNER_RADIUS_M: (f32, f32) = (8.0, 200.0);
    /// How far the finish may miss the start before the lap isn't one.
    pub const CLOSURE_M: f32 = 20.0;
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

/// Ask for a track, and keep asking until it measures like one.
///
/// `tries` counts total attempts, not retries. Two is the useful minimum: models get the
/// shape right and the *numbers* wrong, and the numbers are exactly what a measured
/// complaint fixes.
pub async fn generate(brief: &str, ask: &impl Ask, tries: usize) -> Result<TrackProgram> {
    let mut attempt = Attempt::default();
    let mut last: Option<String> = None;

    for round in 0..tries.max(1) {
        let raw = ask
            .ask(brief, &attempt)
            .await
            .with_context(|| format!("asking for a track (attempt {})", round + 1))?;

        match serde_json::from_str::<TrackProgram>(&raw) {
            Ok(prog) => {
                let problems = validate(&prog);
                if problems.is_empty() {
                    return Ok(prog);
                }
                log::info!(
                    "[trackllm] attempt {} came back with {} problems",
                    round + 1,
                    problems.len()
                );
                attempt = Attempt {
                    previous: Some(raw.clone()),
                    problems,
                };
            }
            Err(e) => {
                // A schema violation is the model's to fix too, and saying which field
                // beats saying "invalid JSON".
                attempt = Attempt {
                    previous: Some(raw.clone()),
                    problems: vec![format!("that didn't parse as a track program: {e}")],
                };
            }
        }
        last = Some(attempt.problems.join("; "));
    }

    bail!(
        "gave up after {} attempts — last time: {}",
        tries.max(1),
        last.unwrap_or_else(|| "no answer".into())
    )
}

/// Everything wrong with a program, said the way the model needs to hear it.
///
/// Each problem names the measurement, the value, and what published tracks do. That last
/// part is what makes it fixable: "too wide" is an opinion, "31 m against a corpus of 10–17"
/// is an instruction.
pub fn validate(prog: &TrackProgram) -> Vec<String> {
    let mut out = Vec::new();
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
        return out;
    }

    let closure = prog.closure_error();
    if closure > corpus::CLOSURE_M {
        out.push(format!(
            "the lap doesn't close: the finish is {closure:.0} m from the start. The turns have \
             to add up to a whole number of full circles — check that the signed angles sum to \
             ±360°."
        ));
    }
    between("the riding line", prog.width, corpus::WIDTH_M, " m", &mut out);
    between("the lap", prog.lap_length(), corpus::LAP_M, " m", &mut out);

    let per_km = prog.features.len() as f32 * 1000.0 / prog.lap_length().max(1.0);
    between("feature density", per_km, corpus::LIPS_PER_KM, " per km", &mut out);

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
            let straight = turn
                .iter()
                .find(|s| s.s >= *at)
                .map(|s| s.curvature == 0.0)
                .unwrap_or(true);
            if straight {
                out.push(format!(
                    "the berm at {at:.0} m is on a straight. Berms bank the outside of a \
                     corner — put it where an arc segment is."
                ));
            }
        }
    }

    // Then the real test: build it, and measure what came out.
    let syn = match tracksynth::synthesise(prog) {
        Ok(s) => s,
        Err(e) => {
            out.push(e.to_string());
            return out;
        }
    };
    let c = crate::trackstats::measure("synth", &syn.corridor, &syn.heights, syn.gw, syn.gh, syn.mps);

    between("the built relief", c.feature_relief_m.p90, corpus::RELIEF_M, " m", &mut out);
    between("the steepest ground", c.slope_deg.p99, corpus::SLOPE_P99_DEG, "°", &mut out);
    if c.largest_component_fraction < 0.95 {
        out.push(format!(
            "the riding line comes out in {:.0}% pieces — the lap probably crosses itself",
            (1.0 - c.largest_component_fraction) * 100.0
        ));
    }
    out
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
        assert_eq!(v["segments"][0]["kind"], "straight");
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
        assert_eq!(got.width, 13.0);

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
        let p = tweaked(|p| {
            p.segments.truncate(12);
            p.features.retain(|f| f.at() < 400.0);
        });
        let problems = validate(&p);
        assert!(
            problems.iter().any(|s| s.contains("doesn't close")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_berm_on_a_straight_is_caught() {
        let p = tweaked(|p| {
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
}
