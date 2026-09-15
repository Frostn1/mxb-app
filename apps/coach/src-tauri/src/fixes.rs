//! Setup fixes: each setup tip becomes the changes that fix it, in the order to try them, and a
//! copy of the rider's setup with those changes made.
//!
//! The coach only writes a setting when it knows the bike's list for it and which way the
//! list runs. Geometry (swingarm, fork height, rod, offset) stays advice: its direction in
//! the lists isn't confirmed yet.

use crate::bikecfg::{BikeOptions, Options};
use crate::stp::{Field, Setup};
use serde::Serialize;
use std::collections::HashMap;

struct Step {
    field: Field,
    /// Steps firmer, more oil or more teeth; negative is the other way.
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
const REAR_LOW: &[Step] = &[step(Field::ShockPreload, 1, "More shock preload lifts the rear and levels the bike.")];
const FRONT_LOW: &[Step] = &[
    step(Field::ForkPreload, 1, "More fork preload lifts the front."),
    step(Field::ShockPreload, -1, "Or a little less shock preload lowers the rear."),
];
const FRONT_PUSH: &[Step] = &[
    step(Field::ForkCompression, -1, "Softer fork compression lets the front dig in."),
    step(Field::ShockPreload, 1, "A little more shock preload puts more weight on the front."),
];
const GEARING_TALL: &[Step] =
    &[step(Field::RearSprocket, -1, "One tooth less on the rear: taller gearing, so it pulls longer before the limiter.")];
const GEARING_SHORT: &[Step] =
    &[step(Field::RearSprocket, 1, "One tooth more on the rear: shorter gearing, so it pulls out of slow corners.")];
const SWINGARM: &[Step] = &[
    step(Field::SwingarmLength, 1, "A longer swingarm puts weight on the front and keeps it down."),
    step(Field::ShockLowCompression, 1, "Firmer low-speed compression stops the rear squatting when you get on the gas."),
];

fn steps(skill: &str) -> &'static [Step] {
    match skill {
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
];

/// Positions to move in the bike's list. Oil is listed as the air gap above it, so more oil is
/// an earlier option.
fn positions(s: &Step) -> i64 {
    if s.field == Field::ForkOil {
        -(s.firmer as i64)
    } else {
        s.firmer as i64
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
                .map(|st| {
                    let o = opts.and_then(|m| m.get(&st.field)).filter(|o| o.count > 0);
                    let from = setup.map(|s| s.get(st.field));
                    let to = from.zip(o).map(|(f, o)| target(f, positions(st), o));
                    Change {
                        field: st.field,
                        steps: st.firmer,
                        why: st.why.into(),
                        from,
                        to,
                        from_value: from.zip(o).and_then(|(i, o)| value(st.field, o, i)),
                        to_value: to.zip(o).and_then(|(i, o)| value(st.field, o, i)),
                        writes: WRITES.contains(&st.field) && to.is_some() && to != from,
                    }
                })
                .collect(),
        })
        .collect()
}

/// The setup with every change the coach can make. Changes to one setting add up, and a
/// setting the fixes pull both ways is left alone. Returns the copy and how many settings moved.
pub fn apply(setup: &Setup, fixes: &[Fix], opts: &BikeOptions) -> (Setup, usize) {
    let mut by: HashMap<Field, Vec<i64>> = HashMap::new();
    for c in fixes.iter().flat_map(|f| &f.changes).filter(|c| c.writes) {
        if let (Some(from), Some(to)) = (c.from, c.to) {
            by.entry(c.field).or_default().push(to as i64 - from as i64);
        }
    }
    let mut out = setup.clone();
    let mut moved = 0;
    for (field, deltas) in by {
        if deltas.iter().any(|&d| d > 0) && deltas.iter().any(|&d| d < 0) {
            continue;
        }
        let Some(o) = opts.get(&field) else { continue };
        let from = setup.get(field);
        let to = target(from, deltas.iter().sum(), o);
        if to != from {
            out.set(field, to);
            moved += 1;
        }
    }
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
    fn gearing_moves_the_rear_sprocket_in_teeth() {
        let (s, o) = sand();
        let c = &plan(&skills(&["setup_gearing_tall"]), Some(&s), Some(&o))[0].changes[0];
        assert_eq!((c.from_value.as_deref(), c.to_value.as_deref()), (Some("52T"), Some("51T")));
        assert!(c.writes);
    }

    #[test]
    fn geometry_stays_advice() {
        let (s, o) = sand();
        let c = &plan(&skills(&["setup_swingarm"]), Some(&s), Some(&o))[0].changes;
        assert!(!c[0].writes, "swingarm");
        assert!(c[1].writes, "shock low-speed compression");
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
        assert_eq!(moved, 3);
        assert_eq!(out.bike_id(), "2027_K85M");
    }
}
