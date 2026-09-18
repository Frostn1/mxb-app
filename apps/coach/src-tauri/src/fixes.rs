//! Setup fixes: each setup tip becomes the changes that fix it, in the order to try them, and a
//! copy of the rider's setup with those changes made.
//!
//! The coach only writes a setting when it knows the bike's list for it and which way the
//! list runs. For geometry that comes from the game itself: a later option raises the front
//! (fork height), lengthens the rod (lowering the rear) and adds offset. The swingarm's list is
//! the bike's `.geom`, which runs either way, so its direction is read from the positions.
//! Swingarm pivot and rake are never written: no OEM bike can change them.

use crate::analysis::Finding;
use crate::bikecfg::{BikeOptions, Options};
use crate::stp::{Field, Setup};
use serde::Serialize;
use std::collections::HashMap;

struct Step {
    field: Field,
    /// Steps firmer, more oil or more teeth; negative is the other way. For geometry: a higher
    /// front, a longer rod or swingarm, more offset.
    firmer: i32,
    why: &'static str,
}

const fn step(field: Field, firmer: i32, why: &'static str) -> Step {
    Step { field, firmer, why }
}

const BOTTOMING_FORK: &[Step] = &[
    step(Field::ForkCompression, 2, "Firmer compression holds the fork up in its stroke."),
    step(Field::ForkOil, 1, "More oil firms up the end of the stroke, where it bottoms."),
    step(Field::ForkSpring, 1, "Still bottoming after that: a stiffer spring."),
];
const BOTTOMING_SHOCK: &[Step] = &[
    step(Field::ShockHighCompression, 2, "Firmer high-speed compression catches the hard hits."),
    step(Field::ShockSpring, 1, "Still bottoming after that: a stiffer spring."),
    step(Field::ShockPreload, 1, "A little more preload keeps it higher in its stroke."),
];
const STIFF_FORK: &[Step] = &[
    step(Field::ForkCompression, -2, "Softer compression lets it use more of its stroke."),
    step(Field::ForkOil, -1, "Less oil softens the end of the stroke."),
    step(Field::ForkSpring, -1, "Still not using it: a softer spring."),
];
const STIFF_SHOCK: &[Step] = &[
    step(Field::ShockLowCompression, -1, "Softer low-speed compression over the small bumps."),
    step(Field::ShockHighCompression, -1, "Softer high-speed compression on the bigger hits."),
    step(Field::ShockSpring, -1, "Still not using it: a softer spring."),
];
const BOTTOMING_SHOCK_SLOW: &[Step] = &[
    step(Field::ShockLowCompression, 2, "Firmer low-speed compression holds the rear up under braking and in turns."),
    step(Field::ShockSpring, 1, "Still bottoming after that: a stiffer spring."),
    step(Field::ShockPreload, 1, "A little more preload keeps it higher in its stroke."),
];
const BRAKE_DIVE: &[Step] = &[
    step(Field::ForkCompression, 1, "Firmer compression holds the front up under braking."),
    step(Field::ForkOil, 1, "More oil firms up the deep part of the stroke."),
    step(Field::ForkPreload, 1, "A little more preload starts the fork higher."),
];
const EXIT_SQUAT: &[Step] = &[
    step(Field::ShockLowCompression, 1, "Firmer low-speed compression stops the rear squatting on the gas."),
    step(Field::ShockPreload, 1, "A little more preload lifts the rear and puts weight on the front."),
];
const SHOCK_KICK: &[Step] = &[step(Field::ShockRebound, 1, "Slower rebound stops the rear springing up off the lip.")];
const PACKING_FORK: &[Step] = &[
    step(Field::ForkRebound, -2, "Faster rebound lets the fork come back up between hits."),
    step(Field::ForkCompression, -1, "Softer compression, so each hit pushes it down less."),
];
const PACKING_SHOCK: &[Step] = &[
    step(Field::ShockRebound, -2, "Faster rebound lets the shock come back up between hits."),
    step(Field::ShockLowCompression, -1, "Softer low-speed compression, so each hit pushes it down less."),
];
const REAR_LOW: &[Step] = &[
    step(Field::ShockPreload, 1, "More shock preload lifts the rear and levels the bike."),
    step(Field::RodLength, -1, "Still low: a shorter linkage rod lifts the rear about 3 mm a step."),
];
const FRONT_LOW: &[Step] = &[
    step(Field::ForkPreload, 1, "More fork preload lifts the front."),
    step(Field::ShockPreload, -1, "Or a little less shock preload lowers the rear."),
    step(Field::ForkHeight, 1, "Still low: slide the fork down in the clamps to raise the front."),
];
const FRONT_PUSH: &[Step] = &[
    step(Field::ForkCompression, -1, "Softer fork compression lets the front dig in."),
    step(Field::ShockPreload, 1, "A little more shock preload puts more weight on the front."),
    step(Field::ForkHeight, -1, "Still pushing: the fork up in the clamps puts weight on the front. Less stable at speed."),
];
const GEARING_TALL: &[Step] =
    &[step(Field::RearSprocket, -1, "One tooth less on the rear: taller gearing, so it pulls longer before the limiter.")];
const GEARING_SHORT: &[Step] =
    &[step(Field::RearSprocket, 1, "One tooth more on the rear: shorter gearing, so it pulls out of slow corners.")];
const SWINGARM: &[Step] = &[
    step(Field::SwingarmLength, 1, "A longer swingarm puts weight on the front and keeps it down."),
    step(Field::ShockLowCompression, 1, "Firmer low-speed compression stops the rear squatting when you get on the gas."),
];

// Only the rider feels these; telemetry can't see them, so they come from the feel check.
const UNSTABLE: &[Step] = &[
    step(Field::ForkHeight, 1, "Raise the front: slide the fork down in the clamps. Calmer at speed, a little slower to turn."),
    step(Field::ForkOffset, -1, "Less offset: more trail, so the front holds its line."),
    step(Field::SwingarmLength, 1, "Still loose: a longer swingarm is steadier and finds more drive."),
];
const TURNS_SLOW: &[Step] = &[
    step(Field::ForkHeight, -1, "Lower the front: slide the fork up in the clamps. Turns in quicker, less calm at speed."),
    step(Field::ForkOffset, 1, "More offset: lighter, quicker steering."),
    step(Field::SwingarmLength, -1, "Still slow: a shorter swingarm turns tighter."),
];

// The ground the lap is mostly on (`soil::finding`).
const SAND: &[Step] = &[
    step(Field::ShockLowCompression, 1, "Firmer low-speed compression stops the rear squatting in the sand."),
    step(Field::ForkCompression, 1, "Firmer fork compression stops the front diving into it."),
    step(Field::RearSprocket, 1, "One tooth more on the rear: sand drags, so shorter gearing keeps it pulling."),
];
const HARDPACK: &[Step] = &[
    step(Field::ForkCompression, -1, "Softer fork compression keeps the front tyre on the slick ground."),
    step(Field::ShockLowCompression, -1, "Softer low-speed compression lets the rear follow the ground for drive."),
];
const MUD: &[Step] =
    &[step(Field::RearSprocket, -1, "One tooth less on the rear: taller gearing is smoother on the gas when it spins up.")];

fn steps(skill: &str) -> &'static [Step] {
    match skill {
        "setup_sand" => SAND,
        "setup_hardpack" => HARDPACK,
        "setup_mud" => MUD,
        "setup_unstable" => UNSTABLE,
        "setup_turns_slow" => TURNS_SLOW,
        "setup_bottoming_fork" => BOTTOMING_FORK,
        "setup_bottoming_shock" => BOTTOMING_SHOCK,
        "setup_bottoming_shock_slow" => BOTTOMING_SHOCK_SLOW,
        "setup_brake_dive" => BRAKE_DIVE,
        "setup_exit_squat" => EXIT_SQUAT,
        "setup_shock_kick" => SHOCK_KICK,
        "setup_packing_fork" => PACKING_FORK,
        "setup_packing_shock" => PACKING_SHOCK,
        "setup_rear_low" => REAR_LOW,
        "setup_front_low" => FRONT_LOW,
        "setup_front_push" => FRONT_PUSH,
        "setup_stiff_fork" => STIFF_FORK,
        "setup_stiff_shock" => STIFF_SHOCK,
        "setup_gearing_tall" => GEARING_TALL,
        "setup_gearing_short" => GEARING_SHORT,
        "setup_swingarm" => SWINGARM,
        _ => &[],
    }
}

/// Settings whose lists the coach knows the direction of.
const WRITES: &[Field] = &[
    Field::ForkSpring,
    Field::ForkCompression,
    Field::ForkRebound,
    Field::ForkPreload,
    Field::ForkOil,
    Field::ShockSpring,
    Field::ShockLowCompression,
    Field::ShockHighCompression,
    Field::ShockRebound,
    Field::ShockPreload,
    Field::FrontSprocket,
    Field::RearSprocket,
    Field::FrontPressure,
    Field::RearPressure,
    Field::ForkHeight,
    Field::RodLength,
    Field::ForkOffset,
    Field::SwingarmLength,
];

/// Positions to move in the bike's list. Oil is listed as the air gap above it, so more oil is
/// an earlier option; a swingarm listed long to short runs backwards too.
fn positions(field: Field, firmer: i32, o: &Options) -> i64 {
    let backwards = match field {
        Field::ForkOil => true,
        Field::SwingarmLength => o.values.last() < o.values.first(),
        _ => false,
    };
    if backwards {
        -(firmer as i64)
    } else {
        firmer as i64
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub field: Field,
    /// Steps firmer (or more oil, more teeth); negative is softer.
    pub steps: i32,
    pub why: String,
    /// The option the setup has now and the one to pick, as positions in the bike's list.
    pub from: Option<u32>,
    pub to: Option<u32>,
    /// What those options are, where the bike's file says, like "5.5 N/mm" or "13T".
    pub from_value: Option<String>,
    pub to_value: Option<String>,
    /// The coach can make this change in a copy of the setup.
    pub writes: bool,
    /// Another tip wants this setting the other way, so the coach leaves it to the rider.
    #[serde(default)]
    pub conflict: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub skill: String,
    pub changes: Vec<Change>,
}

fn value(field: Field, o: &Options, i: u32) -> Option<String> {
    let v = *o.values.get(i as usize)?;
    Some(match field {
        Field::ForkSpring | Field::ShockSpring => format!("{:.1} N/mm", v / 1000.0),
        Field::ForkOil | Field::ForkPreload | Field::ShockPreload => format!("{:.0} mm", v * 1000.0),
        Field::FrontSprocket | Field::RearSprocket => format!("{v:.0}T"),
        Field::FrontPressure | Field::RearPressure => format!("{v:.1} kPa"),
        Field::RodLength | Field::ForkOffset => format!("{:+.0} mm", v * 1000.0),
        // From the shortest it goes, which is what the garage's step number counts.
        Field::SwingarmLength => {
            let short = o.values.iter().copied().fold(f64::INFINITY, f64::min);
            format!("+{:.0} mm", (v - short) * 1000.0)
        }
        _ => return None,
    })
}

fn target(from: u32, by: i64, o: &Options) -> u32 {
    (from as i64 + by).clamp(0, o.count as i64 - 1) as u32
}

pub fn plan(skills: &[String], setup: Option<&Setup>, opts: Option<&BikeOptions>) -> Vec<Fix> {
    skills
        .iter()
        .filter(|s| !steps(s).is_empty())
        .map(|s| Fix {
            skill: s.clone(),
            changes: steps(s)
                .iter()
                .map(|st| make(st.field, st.firmer, st.why.to_string(), setup, opts))
                .collect(),
        })
        .collect()
}

/// One change against the setup and the bike's list for the setting.
fn make(field: Field, firmer: i32, why: String, setup: Option<&Setup>, opts: Option<&BikeOptions>) -> Change {
    let o = opts.and_then(|m| m.get(&field)).filter(|o| o.count > 0);
    let from = setup.map(|s| s.get(field));
    let to = from.zip(o).map(|(f, o)| target(f, positions(field, firmer, o), o));
    // A setting already past the bike's list (a tool wrote it; the game doesn't clamp every
    // one) is left alone: stepping back into the list could go the wrong way.
    let in_list = from.zip(o).is_some_and(|(f, o)| (f as usize) < o.count);
    Change {
        field,
        steps: firmer,
        why,
        from,
        to,
        from_value: from.zip(o).and_then(|(i, o)| value(field, o, i)),
        to_value: to.zip(o).and_then(|(i, o)| value(field, o, i)),
        writes: WRITES.contains(&field) && in_list && to.is_some() && to != from,
        conflict: false,
    }
}

/// The rear's standing sag back on target: preload by as many steps as it's off, and a spring
/// step when preload can't go that far. `off` is metres of the shock's stroke, positive for too
/// much sag.
pub fn sag_fix(off: f32, setup: Option<&Setup>, opts: Option<&BikeOptions>) -> Fix {
    let preload = opts.and_then(|m| m.get(&Field::ShockPreload));
    // A preload step's size from the bike's list: 1 mm on the OEM bikes.
    let step = preload
        .and_then(|o| o.values.get(1).zip(o.values.first()).map(|(b, a)| (b - a).abs() as f32))
        .filter(|s| *s > 0.0)
        .unwrap_or(0.001);
    let steps = ((off / step).round() as i32).clamp(-25, 25);
    let side = if off > 0.0 { "below" } else { "above" };
    let mut changes = vec![make(
        Field::ShockPreload,
        steps,
        format!("The rear sits {:.0} mm {side} where it should with you on.", off.abs() * 1000.0),
        setup,
        opts,
    )];
    if let (Some(s), Some(o)) = (setup, preload) {
        let at = s.get(Field::ShockPreload) as i32;
        let room = if steps > 0 { o.count as i32 - 1 - at } else { at };
        if steps.abs() > room {
            let spring = if steps > 0 { "a stiffer spring" } else { "a softer spring" };
            changes.push(make(
                Field::ShockSpring,
                steps.signum(),
                format!("Preload can't go that far on its own: {spring} too."),
                setup,
                opts,
            ));
        }
    }
    Fix { skill: if off > 0.0 { "setup_sag_rear_deep" } else { "setup_sag_rear_high" }.into(), changes }
}

/// Further than this from the pressure a tyre is made for, kPa, is worth putting back.
const PRESSURE_OFF_KPA: f32 = 10.0;

/// Each tyre far from the pressure it's made for: the setting, what it's at, what it's made for.
fn pressure_off(setup: &Setup, opts: &BikeOptions, optimal: [Option<f32>; 2]) -> Vec<(Field, f32, f32)> {
    [Field::FrontPressure, Field::RearPressure]
        .into_iter()
        .zip(optimal)
        .filter_map(|(f, opt)| {
            let opt = opt?;
            let now = *opts.get(&f)?.values.get(setup.get(f) as usize)? as f32;
            ((now - opt).abs() > PRESSURE_OFF_KPA).then_some((f, now, opt))
        })
        .collect()
}

/// Each tyre that's far off back to the pressure it's made for.
pub fn pressure_fix(setup: &Setup, opts: &BikeOptions, optimal: [Option<f32>; 2]) -> Option<Fix> {
    let off = pressure_off(setup, opts, optimal);
    if off.is_empty() {
        return None;
    }
    let changes = off
        .iter()
        .map(|&(f, now, opt)| {
            let o = &opts[&f];
            let best = (0..o.count)
                .min_by(|&a, &b| (o.values[a] as f32 - opt).abs().total_cmp(&(o.values[b] as f32 - opt).abs()))
                .unwrap_or(0) as i32;
            let _ = now;
            let why = "Back to what the tyre is made for.".to_string();
            make(f, best - setup.get(f) as i32, why, Some(setup), Some(opts))
        })
        .collect();
    Some(Fix { skill: "setup_pressure".into(), changes })
}

/// The tip for tyres far from the pressure they're made for.
pub fn pressure_finding(setup: &Setup, opts: &BikeOptions, optimal: [Option<f32>; 2]) -> Option<Finding> {
    let off = pressure_off(setup, opts, optimal);
    if off.is_empty() {
        return None;
    }
    let parts: Vec<String> = off
        .iter()
        .map(|&(f, now, opt)| {
            let end = if f == Field::FrontPressure { "front" } else { "rear" };
            format!("the {end} tyre is at {now:.1} kPa and made for {opt:.0}")
        })
        .collect();
    Some(Finding {
        skill: "setup_pressure",
        title: "Tyre pressure is off".into(),
        detail: format!(
            "In this setup {}. The coach can put it back to what the tyre is made for.",
            parts.join(", and ")
        ),
        at: 0,
        weight: 0.4,
        safety: false,
        absolute: false,
    })
}

/// Every setting a save would really move, and where to. Changes to one setting add up; a
/// setting two tips pull opposite ways stays where it is, because the coach has no way to
/// pick between them.
fn net(setup: &Setup, fixes: &[Fix], opts: &BikeOptions) -> HashMap<Field, u32> {
    let mut by: HashMap<Field, Vec<i64>> = HashMap::new();
    for c in fixes.iter().flat_map(|f| &f.changes).filter(|c| c.writes) {
        if let (Some(from), Some(to)) = (c.from, c.to) {
            by.entry(c.field).or_default().push(to as i64 - from as i64);
        }
    }
    let mut out = HashMap::new();
    for (field, deltas) in by {
        if deltas.iter().any(|&d| d > 0) && deltas.iter().any(|&d| d < 0) {
            continue;
        }
        let Some(o) = opts.get(&field) else { continue };
        let from = setup.get(field);
        let to = target(from, deltas.iter().sum(), o);
        if to != from {
            out.insert(field, to);
        }
    }
    out
}

/// Takes back the claim on every change the save won't actually make.
///
/// Without this the plan and the save disagreed. Two tips can want one setting both ways —
/// sand wants a tooth more on the rear, the rev limiter a tooth less — and the save quietly
/// left it alone while the plan had already told the rider it would be changed, and the toast
/// still said the setup was saved. The rider then found their gearing untouched in the garage.
/// Deciding it once, here, is what keeps what they read and what gets written the same thing.
pub fn settle(fixes: &mut [Fix], setup: Option<&Setup>, opts: Option<&BikeOptions>) {
    let (Some(setup), Some(opts)) = (setup, opts) else { return };
    let moving = net(setup, fixes, opts);
    for c in fixes.iter_mut().flat_map(|f| &mut f.changes) {
        if c.writes && !moving.contains_key(&c.field) {
            c.writes = false;
            c.conflict = true;
        }
    }
}

/// The setup with every change the coach can make, and the settings that moved.
pub fn apply(setup: &Setup, fixes: &[Fix], opts: &BikeOptions) -> (Setup, Vec<Field>) {
    let moving = net(setup, fixes, opts);
    let mut out = setup.clone();
    let mut moved: Vec<Field> = Vec::new();
    for (&field, &to) in &moving {
        out.set(field, to);
        moved.push(field);
    }
    // A stable order, so the rider is told what changed the same way twice running.
    moved.sort_by_key(|f| WRITES.iter().position(|w| w == f).unwrap_or(usize::MAX));
    (out, moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bikecfg::{options, tests::CFG_85};
    use crate::stp::tests::{file, SAND_85};

    fn sand() -> (Setup, BikeOptions) {
        let s = Setup::parse(&file("2027_K85M", &SAND_85), None).unwrap();
        (s, options(&mxb_core::cfg::parse(CFG_85.as_bytes())))
    }

    fn skills(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_bottoming_fork_gets_firmer_in_the_order_to_try() {
        let (s, o) = sand();
        let f = plan(&skills(&["setup_bottoming_fork"]), Some(&s), Some(&o));
        let c = &f[0].changes;
        assert_eq!(c.iter().map(|c| c.field).collect::<Vec<_>>(), [Field::ForkCompression, Field::ForkOil, Field::ForkSpring]);
        assert_eq!((c[0].from, c[0].to), (Some(3), Some(5)));
        // The oil is already at its fullest option: nothing to do there.
        assert_eq!((c[1].from, c[1].to, c[1].writes), (Some(0), Some(0), false));
        assert_eq!(c[2].from_value.as_deref(), Some("6.1 N/mm"));
        assert_eq!(c[2].to_value.as_deref(), Some("6.3 N/mm"));
    }

    #[test]
    fn every_setup_tip_the_review_gives_has_a_fix() {
        let tips = [
            "setup_bottoming_fork", "setup_bottoming_shock", "setup_bottoming_shock_slow", "setup_stiff_fork",
            "setup_stiff_shock", "setup_gearing_tall", "setup_gearing_short", "setup_swingarm", "setup_brake_dive",
            "setup_exit_squat", "setup_shock_kick", "setup_packing_fork", "setup_packing_shock", "setup_rear_low",
            "setup_front_low", "setup_front_push",
        ];
        for s in tips {
            assert!(!steps(s).is_empty(), "{s}");
        }
    }

    #[test]
    fn too_much_sag_takes_preload_by_the_millimetre_and_the_spring_when_it_runs_out() {
        let (s, o) = sand();
        // Shock preload is at 7 of 26 (0..25 mm in 1 mm steps).
        let fix = sag_fix(0.005, Some(&s), Some(&o));
        assert_eq!(fix.skill, "setup_sag_rear_deep");
        assert_eq!((fix.changes[0].from, fix.changes[0].to), (Some(7), Some(12)));
        assert_eq!(fix.changes.len(), 1);
        let big = sag_fix(0.030, Some(&s), Some(&o));
        assert_eq!(big.changes[0].to, Some(25));
        assert_eq!(big.changes[1].field, Field::ShockSpring);
    }

    #[test]
    fn a_tyre_far_off_its_pressure_goes_back_to_it() {
        let (mut s, mut o) = sand();
        let list = crate::bikecfg::Options { count: 29, values: (0..29).map(|i| 55.0 + 2.5 * i as f64).collect() };
        o.insert(Field::FrontPressure, list.clone());
        o.insert(Field::RearPressure, list);
        s.set(Field::FrontPressure, 2); // 60 kPa
        s.set(Field::RearPressure, 11); // 82.5 kPa, close enough
        let fix = pressure_fix(&s, &o, [Some(85.0), Some(85.0)]).unwrap();
        assert_eq!(fix.changes.len(), 1);
        assert_eq!((fix.changes[0].from_value.as_deref(), fix.changes[0].to_value.as_deref()), (Some("60.0 kPa"), Some("85.0 kPa")));
        assert!(pressure_finding(&s, &o, [Some(85.0), Some(85.0)]).is_some());
        s.set(Field::FrontPressure, 12);
        assert!(pressure_fix(&s, &o, [Some(85.0), Some(85.0)]).is_none());
    }

    #[test]
    fn gearing_moves_the_rear_sprocket_in_teeth() {
        let (s, o) = sand();
        let c = &plan(&skills(&["setup_gearing_tall"]), Some(&s), Some(&o))[0].changes[0];
        assert_eq!((c.from_value.as_deref(), c.to_value.as_deref()), (Some("52T"), Some("51T")));
        assert!(c.writes);
    }

    fn with_swingarm(o: &mut BikeOptions, short_to_long: bool) {
        let mut values: Vec<f64> = (0..9).map(|i| 0.5758 + 0.00505 * i as f64).collect();
        if !short_to_long {
            values.reverse();
        }
        o.insert(Field::SwingarmLength, Options { count: 9, values });
    }

    #[test]
    fn a_longer_swingarm_goes_the_way_the_bike_lists_it() {
        let (mut s, mut o) = sand();
        s.set(Field::SwingarmLength, 4);
        with_swingarm(&mut o, true);
        let c = &plan(&skills(&["setup_swingarm"]), Some(&s), Some(&o))[0].changes[0];
        assert_eq!((c.to, c.writes), (Some(5), true));
        assert_eq!((c.from_value.as_deref(), c.to_value.as_deref()), (Some("+20 mm"), Some("+25 mm")));
        with_swingarm(&mut o, false);
        let c = &plan(&skills(&["setup_swingarm"]), Some(&s), Some(&o))[0].changes[0];
        assert_eq!((c.to, c.writes), (Some(3), true));
        assert_eq!(c.to_value.as_deref(), Some("+25 mm"));
    }

    #[test]
    fn a_setting_already_past_the_list_is_left_alone() {
        let (mut s, mut o) = sand();
        with_swingarm(&mut o, true);
        s.set(Field::SwingarmLength, 12);
        let c = &plan(&skills(&["setup_swingarm"]), Some(&s), Some(&o))[0].changes[0];
        assert!(!c.writes);
        // Without the bike's list for it, it stays advice.
        o.remove(&Field::SwingarmLength);
        assert!(!plan(&skills(&["setup_swingarm"]), Some(&s), Some(&o))[0].changes[0].writes);
    }

    #[test]
    fn without_the_setup_it_is_only_advice() {
        let f = plan(&skills(&["setup_stiff_fork", "setup_shift_late"]), None, None);
        assert_eq!(f.len(), 1);
        assert!(f[0].changes.iter().all(|c| !c.writes && c.from.is_none()));
    }

    #[test]
    fn a_copy_makes_the_changes_and_skips_a_setting_pulled_both_ways() {
        let (s, o) = sand();
        // Squat wants a firmer low-speed compression, a stiff shock a softer one.
        let f = plan(&skills(&["setup_swingarm", "setup_stiff_shock", "setup_bottoming_fork"]), Some(&s), Some(&o));
        let (out, moved) = apply(&s, &f, &o);
        assert_eq!(out.get(Field::ShockLowCompression), s.get(Field::ShockLowCompression));
        assert_eq!(out.get(Field::ForkCompression), 5);
        assert_eq!(out.get(Field::ForkSpring), 4);
        assert_eq!(out.get(Field::ShockSpring), 2);
        assert_eq!(moved.len(), 3);
        assert!(!moved.contains(&Field::ShockLowCompression));
        assert_eq!(out.bike_id(), "2027_K85M");
    }

    /// The fault a rider reported: the coach listed a gearing change, said it had saved it,
    /// and the garage showed the old sprocket. A sandy lap wants a tooth more on the rear and
    /// the rev limiter wants one less, so the save left the gearing alone — while the plan the
    /// rider read still claimed it.
    #[test]
    fn a_setting_two_tips_pull_both_ways_is_never_claimed_as_changed() {
        let (s, o) = sand();
        let mut f = plan(&skills(&["setup_sand", "setup_gearing_tall"]), Some(&s), Some(&o));
        let rear = |f: &[Fix]| -> Vec<(bool, bool)> {
            f.iter().flat_map(|x| &x.changes).filter(|c| c.field == Field::RearSprocket).map(|c| (c.writes, c.conflict)).collect()
        };
        // Each tip on its own is a change the coach can make, which is why it used to claim both.
        assert_eq!(rear(&f), [(true, false), (true, false)]);
        settle(&mut f, Some(&s), Some(&o));
        assert_eq!(rear(&f), [(false, true), (false, true)], "both are handed back to the rider");
        let (out, moved) = apply(&s, &f, &o);
        assert_eq!(out.get(Field::RearSprocket), s.get(Field::RearSprocket), "the gearing really doesn't move");
        assert!(!moved.contains(&Field::RearSprocket));
        // The rest of the sand fix still lands, so the save is still worth making.
        assert!(moved.contains(&Field::ForkCompression) && moved.contains(&Field::ShockLowCompression));
    }

    #[test]
    fn gearing_the_laps_agree_on_is_still_written() {
        let (s, o) = sand();
        // Sand and bogging both want a tooth more: one setting, two tips, one direction.
        let mut f = plan(&skills(&["setup_sand", "setup_gearing_short"]), Some(&s), Some(&o));
        settle(&mut f, Some(&s), Some(&o));
        let (out, moved) = apply(&s, &f, &o);
        assert_eq!(out.get(Field::RearSprocket), s.get(Field::RearSprocket) + 2, "the two steps add up");
        assert!(moved.contains(&Field::RearSprocket));
        assert!(f.iter().flat_map(|x| &x.changes).all(|c| !c.conflict));
    }

    #[test]
    fn settling_without_a_setup_leaves_the_advice_alone() {
        let mut f = plan(&skills(&["setup_gearing_tall"]), None, None);
        settle(&mut f, None, None);
        assert!(f.iter().flat_map(|x| &x.changes).all(|c| !c.conflict));
    }
}
