//! What a bike offers for each setting, from its own `.cfg`: how many options there are, and
//! what each one is where the file lists them (spring rates, oil levels, sprocket teeth).
//!
//! A `.stp` stores each setting as a position in these lists, so the coach needs them to know
//! how far a setting can go. Every list runs the same way: a later option is stiffer, more
//! damped, more preload, more oil gap or more teeth.

use crate::stp::Field;
use mxb_core::cfg::{self, CfgNode};
use mxb_core::{library, pkz};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub count: usize,
    /// The value behind each option, in the file's units.
    pub values: Vec<f64>,
}

pub type BikeOptions = HashMap<Field, Options>;

/// Reads `<bike>.cfg` beside the bike, else inside its `.pkz`. Locked OEM bikes open with the
/// locked-content reader, which release builds carry.
pub fn load(mods_path: &str, bike: &str) -> Option<BikeOptions> {
    load_cfg(mods_path, bike).map(|c| options(&c))
}

/// The bike's parsed cfg, for what else it names: its tyres, its gearbox.
pub fn load_cfg(mods_path: &str, bike: &str) -> Option<CfgNode> {
    let root = library::mods_subdir(mods_path, "mods/bikes");
    let want = format!("{bike}.cfg");
    let loose = std::fs::read(root.join(bike).join(&want)).ok();
    let bytes = loose.or_else(|| {
        let want = &want;
        let is_cfg = move |n: &str| n.rsplit('/').next().is_some_and(|f| f.eq_ignore_ascii_case(want));
        pkz::read_selected(&root.join(format!("{bike}.pkz")), is_cfg).ok()?.into_iter().next().map(|(_, b)| b)
    })?;
    Some(cfg::parse(&bytes))
}

/// Where each setting's list sits in the cfg; keys are lowercased by the parser.
const PLACES: &[(Field, &[&str])] = &[
    (Field::ForkSpring, &["front_suspension", "spring"]),
    (Field::ForkOil, &["front_suspension", "oil"]),
    (Field::ForkPreload, &["front_suspension", "preload"]),
    (Field::ForkHeight, &["front_suspension", "rideheight"]),
    (Field::ForkCompression, &["front_suspension", "damper", "bump"]),
    (Field::ForkRebound, &["front_suspension", "damper", "rebound"]),
    (Field::ShockSpring, &["rear_suspension", "spring"]),
    (Field::ShockPreload, &["rear_suspension", "preload"]),
    (Field::ShockLowCompression, &["rear_suspension", "damper", "bump"]),
    (Field::ShockHighCompression, &["rear_suspension", "damper", "fastbump"]),
    (Field::ShockRebound, &["rear_suspension", "damper", "rebound"]),
    (Field::RodLength, &["rear_suspension", "rodlength"]),
    (Field::ForkOffset, &["steer", "forkoffset"]),
];

pub fn options(root: &CfgNode) -> BikeOptions {
    let mut out = HashMap::new();
    for &(field, path) in PLACES {
        if let Some(o) = path.iter().try_fold(root, |n, k| n.block(k)).and_then(list) {
            out.insert(field, o);
        }
    }
    // The sprockets sit under the driveline or the gearbox depending on the bike.
    for (field, name) in [(Field::FrontSprocket, "fsprocket"), (Field::RearSprocket, "rsprocket")] {
        if let Some(o) = find(root, name).and_then(list) {
            out.insert(field, o);
        }
    }
    out
}

fn find<'a>(n: &'a CfgNode, name: &str) -> Option<&'a CfgNode> {
    n.block(name).or_else(|| n.blocks.values().find_map(|b| find(b, name)))
}

/// An option list: `range = first, step, last`, or numbered `setting0`, `gear0` … entries.
pub(crate) fn list(n: &CfgNode) -> Option<Options> {
    if let Some(r) = n.get("range") {
        let v: Vec<f64> = r.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let &[a, s, b] = v.as_slice() else { return None };
        if s <= 0.0 || b < a {
            return None;
        }
        let count = ((b - a) / s).round() as usize + 1;
        return Some(Options { count, values: (0..count).map(|i| a + s * i as f64).collect() });
    }
    let mut numbered: Vec<(usize, f64)> = n
        .values
        .iter()
        .filter_map(|(k, v)| {
            let i = k.strip_prefix("setting").or_else(|| k.strip_prefix("gear"))?.parse().ok()?;
            Some((i, v.split(',').next()?.trim().parse().unwrap_or(0.0)))
        })
        .collect();
    numbered.sort_by_key(|&(i, _)| i);
    // Only a whole list, 0, 1, 2 …: a gap means it isn't one.
    if numbered.is_empty() || numbered.iter().enumerate().any(|(k, &(i, _))| k != i) {
        return None;
    }
    Some(Options { count: numbered.len(), values: numbered.into_iter().map(|(_, v)| v).collect() })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The setup lists of a real 85cc bike's cfg, trimmed.
    pub(crate) const CFG_85: &str = "
gearbox
{
	NumGears = 6
	FSprocket
	{
		gear0 = 11
		gear1 = 12
		gear2 = 13
		gear3 = 14
		gear4 = 15
	}
	FSprocketSetting = 2
	RSprocket
	{
		gear0 = 47
		gear1 = 48
		gear2 = 49
		gear3 = 50
		gear4 = 51
		gear5 = 52
		gear6 = 53
		gear7 = 54
	}
}
front_suspension
{
	Spring
	{
		range = 5500, 200, 6500
		setting = 5
	}
	Oil
	{
		range = 0.08, 0.005, 0.13
		setting = 4
	}
	Damper
	{
		Bump
		{
			setting0 = 350, 315, 1400
			setting1 = 375, 320, 1400
			setting2 = 400, 325, 1400
			setting3 = 425, 330, 1400
			setting4 = 450, 335, 1400
			setting5 = 475, 340, 1400
			setting6 = 500, 345, 1400
			setting7 = 525, 350, 1400
		}
		BumpSetting = 3
	}
	Preload
	{
		range = 0, 0.001, 0.015
		setting = 8
	}
}
rear_suspension
{
	Spring
	{
		range = 45000, 3000, 60000
		setting = 4
	}
	Preload
	{
		range = 0, 0.001, 0.025
		setting = 12
	}
	Damper
	{
		Bump
		{
			setting0 = 5800, 5500, 24800
			setting1 = 6300, 5600, 24800
			setting2 = 6800, 5700, 24800
			setting3 = 7300, 5800, 24800
			setting4 = 7800, 5900, 24800
			setting5 = 8300, 6000, 24800
			setting6 = 8800, 6100, 24800
			setting7 = 9300, 6200, 24800
		}
	}
}
";

    #[test]
    fn reads_every_list_a_bike_has() {
        let o = options(&cfg::parse(CFG_85.as_bytes()));
        assert_eq!(o[&Field::ForkSpring].count, 6);
        assert_eq!(o[&Field::ForkSpring].values[5], 6500.0);
        assert_eq!(o[&Field::ForkOil].count, 11);
        assert_eq!(o[&Field::ForkPreload].count, 16);
        assert_eq!(o[&Field::ForkCompression].count, 8);
        assert_eq!(o[&Field::ShockSpring].count, 6);
        assert_eq!(o[&Field::ShockLowCompression].count, 8);
        assert_eq!(o[&Field::FrontSprocket].values, vec![11.0, 12.0, 13.0, 14.0, 15.0]);
        assert_eq!(o[&Field::RearSprocket].count, 8);
        // What the bike doesn't list, the coach doesn't claim to know.
        assert!(!o.contains_key(&Field::ShockHighCompression));
        assert!(!o.contains_key(&Field::SwingarmLength));
    }
}
