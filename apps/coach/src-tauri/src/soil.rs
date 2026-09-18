//! What the ground is, from the rear wheel's surface: hardpack, soft soil, sand and so on.
//!
//! The recorder gives each wheel's surface as the game's own material list plus one (0 is in
//! the air), so this needs no track file and works on locked tracks. The list, from the game:
//! 0 asphalt, 1 asphalt 2, 2 asphalt 3, 3 concrete, 4 grass, 5 sand, 6 kerb, 7 soil, 8 paint,
//! 9 artificial turf, 10 soft soil, 11 compact soil, 12 gravel, 13 rock. There is no mud: it's
//! soil in the wet.

use crate::analysis::{Finding, Point};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Soil {
    /// Made ground: asphalt, concrete, kerbs, paint, turf.
    Hard,
    Hardpack,
    Intermediate,
    Soft,
    Sand,
    Grass,
    Rocky,
    Mud,
}

impl Soil {
    /// The wheel's surface as the recorder gives it, in dry or wet conditions.
    pub fn from_wheel(v: u8, wet: bool) -> Option<Soil> {
        let dirt = |s: Soil| if wet { Soil::Mud } else { s };
        Some(match v.checked_sub(1)? {
            11 => dirt(Soil::Hardpack),
            7 => dirt(Soil::Intermediate),
            10 => dirt(Soil::Soft),
            5 => Soil::Sand,
            4 => Soil::Grass,
            12 | 13 => Soil::Rocky,
            0..=3 | 6 | 8 | 9 => Soil::Hard,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// The ground most of it is on.
    pub kind: Soil,
    /// Its share of the metres on the ground.
    pub share: f32,
    /// Sand's share, which matters even as the lesser part.
    pub sand: f32,
}

/// The ground under these metres of a lap, or None when the wheels were never down.
pub fn profile(pts: &[Point], wet: bool) -> Option<Profile> {
    let kinds: Vec<Soil> = pts.iter().filter_map(|p| Soil::from_wheel(p.ground, wet)).collect();
    let n = kinds.len() as f32;
    let count = |k: Soil| kinds.iter().filter(|&&x| x == k).count() as f32;
    let kind = *kinds.iter().max_by(|a, b| count(**a).total_cmp(&count(**b)))?;
    Some(Profile { kind, share: count(kind) / n, sand: count(Soil::Sand) / n })
}

/// A lap this much on one ground gets setup advice for it. Sand counts from less.
const LAP_SHARE: f32 = 0.5;
const LAP_SAND: f32 = 0.3;

/// The setup tip for the ground a lap is mostly on, when that ground wants its own setup.
pub fn finding(lap: &Profile) -> Option<Finding> {
    let (skill, title, detail) = if lap.sand >= LAP_SAND {
        (
            "setup_sand",
            "Set the bike up for sand",
            format!(
                "{:.0}% of this lap is sand. It lets the rear squat and the front dive, and drags on the engine: \
                 firmer low-speed compression on the shock, firmer fork compression and a tooth more on the rear.",
                lap.sand * 100.0
            ),
        )
    } else if lap.share < LAP_SHARE {
        return None;
    } else {
        match lap.kind {
            Soil::Hardpack => (
                "setup_hardpack",
                "Set the bike up for hardpack",
                format!(
                    "{:.0}% of this lap is hardpack. It gives less grip, and the suspension has to keep the \
                     tyres on it: softer compression at both ends.",
                    lap.share * 100.0
                ),
            ),
            Soil::Mud => (
                "setup_mud",
                "Set the bike up for mud",
                format!(
                    "{:.0}% of this lap is wet ground. The rear spins up easily: taller gearing, a tooth less on the \
                     rear, is smoother on the gas.",
                    lap.share * 100.0
                ),
            ),
            _ => return None,
        }
    };
    Some(Finding { skill, title: title.into(), detail, at: 0, weight: 0.5, safety: false, absolute: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ground: u8) -> Point {
        Point { ground, ..Point::default() }
    }

    #[test]
    fn names_the_ground_from_the_recorder_s_ids() {
        // Real laps: 11 over most of Indiana, 6 in its sand, 0 in the air.
        assert_eq!(Soil::from_wheel(11, false), Some(Soil::Soft));
        assert_eq!(Soil::from_wheel(6, false), Some(Soil::Sand));
        assert_eq!(Soil::from_wheel(12, false), Some(Soil::Hardpack));
        assert_eq!(Soil::from_wheel(0, false), None);
        assert_eq!(Soil::from_wheel(11, true), Some(Soil::Mud));
        assert_eq!(Soil::from_wheel(6, true), Some(Soil::Sand));
    }

    #[test]
    fn a_lap_part_sand_gets_sand_setup() {
        let mut pts: Vec<Point> = (0..60).map(|_| at(11)).collect();
        pts.extend((0..40).map(|_| at(6)));
        pts.extend((0..20).map(|_| at(0)));
        let p = profile(&pts, false).unwrap();
        assert_eq!((p.kind, p.share, p.sand), (Soil::Soft, 0.6, 0.4));
        assert_eq!(finding(&p).unwrap().skill, "setup_sand");
        // Soft soil alone needs nothing of its own.
        assert!(finding(&profile(&pts[..60], false).unwrap()).is_none());
        assert!(profile(&pts[100..], false).is_none());
    }
}
