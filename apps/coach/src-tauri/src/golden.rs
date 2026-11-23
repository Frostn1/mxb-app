//! Every piece of advice the engine writes for a spread of made-up laps, as one JSON document.
//!
//! `testdata/advice_en.json` is that document as the English engine wrote it before the advice
//! went through `i18n`. The test holds the English to it byte for byte, so moving a sentence
//! into the dictionary can't change a word of what an English rider reads. After a deliberate
//! change to the advice, write it again:
//! `COACH_WRITE_GOLDEN=1 cargo test -p mxb-coach english_advice_is_unchanged`

use serde_json::{json, Value};

use crate::analysis::tests::{lap, Style, CASED, FAST, HOT, OVER};
use crate::analysis::{self, Bike, Finding, Review, Trace};
use crate::cues::{self, Amount, History, Level};
use crate::lines::{self, LapId};

const BIKE: Bike =
    Bike { limiter: 13000.0, max_rpm: 14000.0, shift_rpm: 12500.0, travel: [0.3, 0.3], land_scale: 0.0, torque_scale: 0.0 };

fn styles() -> Vec<(&'static str, Style)> {
    vec![
        ("fast", FAST),
        ("over", OVER),
        ("cased", CASED),
        ("hot", HOT),
        ("slow_corner", Style { corner_v: 9.0, ..FAST }),
        ("soft_brake", Style { decel: 2.5, brake: 0.5, ..FAST }),
        ("floaty", Style { jump: (330.0, 356.0, 4.5), air_v: 17.0, ..FAST }),
        ("whip_bottom", Style { whip: 35.0, bottom: 0.29, ..FAST }),
        ("drag", Style { drag: 0.4, ..FAST }),
        ("shallow", Style { hill: (346.0, 0.08, 26.0), hit: 14.0, ..FAST }),
        ("hard", Style { hit: 14.0, ..FAST }),
        ("push", Style { push: 0.5, ..FAST }),
        ("soft_gas", Style { gas: 0.6, accel: 4.0, ..FAST }),
        ("tense", Style { torque: 60.0, corner_v: 8.5, ..FAST }),
        ("scrub", Style { whip: 40.0, ..FAST }),
        ("rolled", Style { jump: (0.0, 0.0, 0.0), ..FAST }),
        ("bottom", Style { bottom: 0.29, ..FAST }),
        ("hops", Style { jump: (330.0, 340.0, 1.5), hop: (348.0, 358.0, 1.5), air_v: 17.0, ..FAST }),
        ("bounced", Style { jump: (330.0, 350.0, 3.0), hop: (353.0, 360.0, 0.3), ..FAST }),
        ("low", Style { sink: 0.8, corner_v: 9.0, ..FAST }),
        ("early_jump", Style { jump: (215.0, 235.0, 3.0), ..FAST }),
        ("wide", Style { wide: 2.5, corner_v: 11.0, ..FAST }),
        ("cased_scrub", Style { whip: 40.0, ..CASED }),
        ("hot_soft", Style { hot: 45.0, decel: 2.0, corner_v: 9.0, gas: 0.6, ..FAST }),
        ("everything", Style { decel: 2.5, brake: 0.5, whip: 35.0, bottom: 0.29, torque: 60.0, corner_v: 8.5, gas: 0.6, ..FAST }),
    ]
}

fn finding(f: &Finding) -> Value {
    json!([f.skill, f.title, f.detail])
}

fn review(rv: &Review) -> Value {
    json!({
        "sections": rv.sections.iter().map(|s| json!({
            "name": s.section.name,
            "findings": s.findings.iter().map(finding).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "overall": rv.overall.iter().map(|t| json!([t.name, t.sections, t.tip])).collect::<Vec<_>>(),
        "setup": rv.setup.iter().map(finding).collect::<Vec<_>>(),
    })
}

fn cue_sheet(fast: &Trace, rv: &Review) -> Value {
    let secs: Vec<analysis::Section> = rv.sections.iter().map(|s| s.section.clone()).collect();
    let points = analysis::cue_points(fast, &secs);
    let mut out = Vec::new();
    for level in [Level::New, Level::Intermediate, Level::SubPro, Level::Pro] {
        let picked = cues::pick(&points, rv, level, Amount::Lots, &History::default());
        out.push(picked.cues.iter().map(|c| json!([c.text, c.section])).collect::<Vec<_>>());
    }
    json!(out)
}

fn fixes() -> Value {
    use crate::bikecfg::{options, tests::CFG_85};
    use crate::stp::tests::{file, SAND_85};
    use crate::stp::Setup;
    let s = Setup::parse(&file("2027_K85M", &SAND_85), None).unwrap();
    let o = options(&mxb_core::cfg::parse(CFG_85.as_bytes()));
    let skills: Vec<String> = [
        "setup_sand", "setup_hardpack", "setup_mud", "setup_unstable", "setup_turns_slow", "setup_bottoming_fork",
        "setup_bottoming_shock", "setup_bottoming_shock_slow", "setup_brake_dive", "setup_exit_squat",
        "setup_shock_kick", "setup_packing_fork", "setup_packing_shock", "setup_rear_low", "setup_front_low",
        "setup_front_push", "setup_stiff_fork", "setup_stiff_shock", "setup_gearing_tall", "setup_gearing_short",
        "setup_swingarm",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let change = |c: &crate::fixes::Change| json!([c.why, c.from_value, c.to_value]);
    let fix = |f: &crate::fixes::Fix| json!([f.skill, f.changes.iter().map(change).collect::<Vec<_>>()]);
    let mut out: Vec<Value> = crate::fixes::plan(&skills, Some(&s), Some(&o)).iter().map(fix).collect();
    out.extend(crate::fixes::plan(&skills, None, None).iter().map(fix));
    for off in [0.012, -0.009, 0.08, -0.08] {
        out.push(fix(&crate::fixes::sag_fix(off, Some(&s), Some(&o))));
    }
    for optimal in [[Some(60.0), Some(200.0)], [None, Some(40.0)]] {
        if let Some(f) = crate::fixes::pressure_fix(&s, &o, optimal) {
            out.push(fix(&f));
        }
        if let Some(f) = crate::fixes::pressure_finding(&s, &o, optimal) {
            out.push(finding(&f));
        }
    }
    json!(out)
}

fn ground() -> Value {
    use crate::sag::{finding as sag, Sag};
    use crate::soil::{finding as soil, Profile, Soil};
    let mut out = Vec::new();
    for (metres, share) in [(0.13, 0.43), (0.07, 0.23)] {
        if let Some(f) = sag(&Sag { still: true, metres: [0.05, metres], share: [0.2, share] }, [0.3, 0.3]) {
            out.push(finding(&f));
        }
    }
    for kind in [Soil::Hard, Soil::Hardpack, Soil::Intermediate, Soil::Soft, Soil::Sand, Soil::Grass, Soil::Rocky, Soil::Mud] {
        for (share, sand) in [(0.9, 0.0), (0.6, 0.35)] {
            if let Some(f) = soil(&Profile { kind, share, sand }) {
                out.push(finding(&f));
            }
        }
    }
    json!(out)
}

fn session_lines() -> Value {
    let id = |n: i32| LapId { lap: n, stint: 0 };
    let fast = lap(&FAST);
    let secs = analysis::sections(&fast);
    let note = |l: &lines::Lines| l.notes.iter().map(|n| json!([n.kind, n.name, n.title, n.detail])).collect::<Vec<_>>();
    let mut out = Vec::new();
    for (a, b) in [
        (Style { wide: 2.5, corner_v: 11.0, ..FAST }, Style { wide: 2.5, corner_v: 11.0, ..FAST }),
        (Style { wide: 2.5, corner_v: 10.2, ..FAST }, Style { wide: 2.5, corner_v: 10.2, ..FAST }),
        (Style { wide: -2.5, corner_v: 11.0, ..FAST }, Style { wide: -2.5, corner_v: 11.0, ..FAST }),
    ] {
        let laps = vec![(id(0), lap(&FAST)), (id(1), lap(&FAST)), (id(2), lap(&a)), (id(3), lap(&b))];
        out.push(note(&lines::lines(&laps, &fast, &[], &secs)));
    }
    let sinking: Vec<(LapId, Trace)> = [0.0, 0.0, 0.02, 0.05, 0.1, 0.12]
        .iter()
        .enumerate()
        .map(|(n, &sink)| (id(n as i32), lap(&Style { sink, ..FAST })))
        .collect();
    out.push(note(&lines::lines(&sinking, &fast, &[], &secs)));
    // Other riders on track: everyone on the fast line, and everyone a couple of metres out.
    let others = |off: f32| -> Vec<[f32; 2]> {
        let w = lap(&Style { wide: off, ..FAST });
        (0..40).flat_map(|_| w.pts.iter().map(|p| [p.x, p.z]).collect::<Vec<_>>()).collect()
    };
    let laps: Vec<(LapId, Trace)> = (0..4).map(|n| (id(n), lap(&FAST))).collect();
    for off in [0.0, 2.5, -2.5] {
        out.push(note(&lines::lines(&laps, &fast, &others(off), &secs)));
    }
    json!(out)
}

pub fn dump() -> String {
    let fast = lap(&FAST);
    let secs = analysis::sections(&fast);
    let targets: Vec<f32> = secs.iter().map(|s| fast.span(s.start, s.end) * 0.97).collect();
    let mut laps = serde_json::Map::new();
    for (name, st) in styles() {
        let l = lap(&st);
        let against = analysis::review(&l, &fast, BIKE);
        laps.insert(
            name.into(),
            json!({
                "review": review(&against),
                "reversed": review(&analysis::review(&fast, &l, BIKE)),
                "untravelled": review(&analysis::review(&l, &fast, Bike { travel: [0.0; 2], ..BIKE })),
                "limiter": review(&analysis::review(&l, &fast, Bike { limiter: 7900.0, max_rpm: 18000.0, ..BIKE })),
                "solo": review(&analysis::solo(&l, BIKE)),
                "targets": review(&analysis::against_targets(&l, &secs, &targets, BIKE)),
                "cues": cue_sheet(&fast, &against),
            }),
        );
    }
    let out = json!({ "laps": laps, "fixes": fixes(), "ground": ground(), "lines": session_lines() });
    serde_json::to_string_pretty(&out).unwrap()
}

#[test]
fn english_advice_is_unchanged() {
    let now = dump();
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/advice_en.json");
    if std::env::var("COACH_WRITE_GOLDEN").is_ok() {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, &now).unwrap();
        return;
    }
    let was = std::fs::read_to_string(&file).expect("testdata/advice_en.json");
    // Line by line, so a failure names the sentence that moved rather than one huge string.
    for (i, (a, b)) in was.lines().zip(now.lines()).enumerate() {
        assert_eq!(a, b, "advice_en.json line {}", i + 1);
    }
    assert_eq!(was.lines().count(), now.lines().count(), "advice_en.json length");
}

/// The same document for real recordings, every whole lap against the session's fastest and on
/// its own. Not committed: it is the rider's own data. Run it before and after a change to the
/// advice and diff the two files.
/// `COACH_LAPS=<sessions dir> COACH_DUMP=<file> cargo test -p mxb-coach dump_real_sessions -- --ignored`
#[test]
#[ignore]
fn dump_real_sessions() {
    let (Ok(dir), Ok(dump)) = (std::env::var("COACH_LAPS"), std::env::var("COACH_DUMP")) else { return };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "mxbc"))
        .collect();
    files.sort();
    let mut out = serde_json::Map::new();
    for path in files {
        let Ok(rec) = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| crate::telemetry::parse(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        let len = rec.event.track_length;
        let laps: Vec<(i32, Trace)> = rec
            .laps()
            .iter()
            .filter(|l| l.whole && !l.invalid)
            .filter_map(|l| Some((l.num, Trace::new(l, len)?)))
            .collect();
        let Some((_, fast)) = laps.iter().min_by(|a, b| a.1.time().total_cmp(&b.1.time())) else { continue };
        let bike = Bike { travel: [0.3, 0.3], ..BIKE };
        let mut doc = Vec::new();
        for (num, t) in &laps {
            let rv = analysis::review(t, fast, bike);
            doc.push(json!({ "lap": num, "review": review(&rv), "solo": review(&analysis::solo(t, bike)), "cues": cue_sheet(fast, &rv) }));
        }
        let ids: Vec<(LapId, Trace)> = laps.iter().map(|(n, t)| (LapId { lap: *n, stint: 0 }, t.clone())).collect();
        let l = lines::lines(&ids, fast, &[], &analysis::sections(fast));
        doc.push(json!(l.notes.iter().map(|n| json!([n.kind, n.name, n.title, n.detail])).collect::<Vec<_>>()));
        out.insert(path.file_name().unwrap().to_string_lossy().into(), json!(doc));
    }
    std::fs::write(dump, serde_json::to_string_pretty(&out).unwrap()).unwrap();
}

/// What the window would be handed for one real recording, in French, for looking at the app
/// headless with the Tauri calls mocked: the session, a review of every whole lap, its lines,
/// the setup plan and the cue sheet.
/// `COACH_MOCK_SESSION=<file.mxbc> COACH_MOCK_OUT=<file.json> cargo test -p mxb-coach dump_mock -- --ignored`
#[test]
#[ignore]
fn dump_mock() {
    let (Ok(file), Ok(dump)) = (std::env::var("COACH_MOCK_SESSION"), std::env::var("COACH_MOCK_OUT")) else { return };
    let locale = if std::env::var("COACH_MOCK_LOCALE").is_ok_and(|l| l == "en") { crate::i18n::Locale::En } else { crate::i18n::Locale::Fr };
    crate::i18n::with(locale, || {
        let path = std::path::Path::new(&file);
        let rec = crate::telemetry::parse(&std::fs::read(path).unwrap()).unwrap();
        let summary = crate::coach::summarize(path, &rec);
        let len = rec.event.track_length;
        let laps: Vec<(i32, Trace)> = rec
            .laps()
            .iter()
            .filter(|l| l.whole && !l.invalid)
            .filter_map(|l| Some((l.num, Trace::new(l, len)?)))
            .collect();
        let (best_num, fast) = laps.iter().min_by(|a, b| a.1.time().total_cmp(&b.1.time())).map(|(n, t)| (*n, t.clone())).unwrap();
        let lap_ref = |num: i32| {
            let ms = summary.laps.iter().find(|l| l.num == num).map_or(0, |l| l.time_ms);
            json!({ "path": file, "lap": num, "timeMs": ms, "started": summary.started, "bikeName": summary.bike_name,
                    "bikeId": summary.bike_id, "rider": summary.rider, "kind": "own" })
        };
        let secs = analysis::sections(&fast);
        let bike = Bike { travel: [0.3, 0.3], ..BIKE };
        let mut reviews = serde_json::Map::new();
        let mut setup_skills: Vec<String> = Vec::new();
        for (num, t) in &laps {
            let mut rv = if *num == best_num { analysis::solo(t, bike) } else { analysis::review(t, &fast, bike) };
            if let Some(f) = crate::soil::profile(&t.pts, false).and_then(|p| crate::soil::finding(&p)) {
                rv.setup.push(f);
            }
            setup_skills.extend(rv.setup.iter().map(|f| f.skill.to_string()));
            reviews.insert(num.to_string(), json!({
                "trackId": summary.track_id, "trackName": summary.track_name, "lap": lap_ref(*num),
                "reference": lap_ref(best_num), "review": rv, "idealFrom": null, "rivals": [], "bestHere": *num == best_num,
            }));
        }
        let ids: Vec<(LapId, Trace)> = laps.iter().map(|(n, t)| (LapId { lap: *n, stint: 0 }, t.clone())).collect();
        let mut lines = serde_json::to_value(lines::lines(&ids, &fast, &[], &secs)).unwrap();
        lines["stint"] = json!(0);
        let ideal = analysis::ideal(&secs, &laps);

        use crate::bikecfg::{options, tests::CFG_85};
        use crate::stp::tests::{file as stp_file, SAND_85};
        let s = crate::stp::Setup::parse(&stp_file("2027_K85M", &SAND_85), None).unwrap();
        let o = options(&mxb_core::cfg::parse(CFG_85.as_bytes()));
        setup_skills.sort();
        setup_skills.dedup();
        let fixes = crate::fixes::plan(&setup_skills, Some(&s), Some(&o));

        let worst = laps.iter().max_by(|a, b| a.1.time().total_cmp(&b.1.time())).unwrap();
        let rv = analysis::review(&worst.1, &fast, bike);
        let picked = cues::pick(&analysis::cue_points(&fast, &secs), &rv, Level::Intermediate, Amount::Normal, &History::default());

        let out = json!({
            "sessions": [summary],
            "session": { "summary": summary, "reference": lap_ref(best_num), "ideal": ideal },
            "reviews": reviews,
            "lines": lines,
            "setupPlan": { "travelUsed": [0.92, 0.97], "name": "Coach sable", "file": "Coach sable.stp", "saveAs": "Coach sable (coach)",
                           "why": null, "fixes": fixes, "sag": null },
            "cues": { "file": "mock.cue", "cues": picked.cues, "ghost": lap_ref(best_num),
                      "line": { "kind": "fastest", "from": [], "notes": [] } },
        });
        std::fs::write(dump, serde_json::to_string(&out).unwrap()).unwrap();
    });
}
