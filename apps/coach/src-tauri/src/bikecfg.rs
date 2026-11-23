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
    read_file(mods_path, bike, &format!("{bike}.cfg"))
}

/// The bike's `.geom`, named by its cfg: where the swingarm lengths are.
pub fn load_geom(mods_path: &str, bike: &str, bike_cfg: &CfgNode) -> Option<CfgNode> {
    read_file(mods_path, bike, bike_cfg.get("geom")?.trim())
}

/// One of the bike's files, beside it or inside its `.pkz`.
fn read_file(mods_path: &str, bike: &str, want: &str) -> Option<CfgNode> {
    let root = library::mods_subdir(mods_path, "mods/bikes");
    let loose = std::fs::read(root.join(bike).join(want)).ok();
    let bytes = loose.or_else(|| {
        let is_it = move |n: &str| n.rsplit('/').next().is_some_and(|f| f.eq_ignore_ascii_case(want));
        pkz::read_selected(&root.join(format!("{bike}.pkz")), is_it).ok()?.into_iter().next().map(|(_, b)| b)
    })?;
    Some(cfg::parse(&bytes))
}

/// The swingarm's options from the `.geom`: `swingarm_steps + 1` axle positions from
/// `rwheel_min` to `rwheel_max`. Each value is how far back the axle sits, metres, so a larger
/// one is a longer swingarm. Most bikes list short to long; a few run the other way.
pub fn swingarm(geom: &CfgNode) -> Option<Options> {
    let steps: usize = geom.get("swingarm_steps")?.trim().parse().ok().filter(|&s| s > 0)?;
    let back = |k: &str| geom.get(k)?.split(',').nth(2)?.trim().parse::<f64>().ok().map(|z| -z);
    let (a, b) = (back("rwheel_min")?, back("rwheel_max")?);
    Some(Options { count: steps + 1, values: (0..=steps).map(|i| a + (b - a) * i as f64 / steps as f64).collect() })
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

fn block_at<'a>(root: &'a CfgNode, path: &[&str]) -> Option<&'a CfgNode> {
    path.iter().try_fold(root, |n, k| n.block(k))
}

/// A whole-number setting, from the first field of the value.
fn num(n: &CfgNode, key: &str) -> Option<u32> {
    let v: f64 = n.get(key)?.split(',').next()?.trim().parse().ok()?;
    (v >= 0.0).then_some(v.round() as u32)
}

/// A setting by name, wherever the bike keeps it — for the few whose name is its own address
/// (`FSprocketSetting`, `CapacitySetting`). Anything with a name a bike uses twice, like
/// `BumpSetting`, is read by path instead.
fn find_num(n: &CfgNode, key: &str) -> Option<u32> {
    num(n, key).or_else(|| n.blocks.values().find_map(|b| find_num(b, key)))
}

/// What the bike itself sets each setting to. This is the setup a rider is on before they
/// save one of their own, and what a setup built from nothing has to start from.
/// The setup version this bike's cfg declares, which the game writes into a setup file and
/// checks a file against. Absent, the game uses `stp::SETUP_VERSION`.
pub fn setup_version(root: &CfgNode) -> Option<u16> {
    let raw = root.get("setup_version")?.trim().to_string();
    let hex = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"));
    match hex {
        Some(h) => u16::from_str_radix(h, 16).ok(),
        None => raw.parse().ok(),
    }
}

pub fn defaults(root: &CfgNode) -> HashMap<Field, u32> {
    let mut out = HashMap::new();
    let at = |path: &[&str], key: &str| block_at(root, path).and_then(|n| num(n, key));
    let mut put = |f: Field, v: Option<u32>| {
        if let Some(v) = v {
            out.insert(f, v);
        }
    };
    put(Field::ForkSpring, at(&["front_suspension", "spring"], "setting"));
    put(Field::ForkOil, at(&["front_suspension", "oil"], "setting"));
    put(Field::ForkPreload, at(&["front_suspension", "preload"], "setting"));
    put(Field::ForkHeight, at(&["front_suspension", "rideheight"], "setting"));
    put(Field::ForkCompression, at(&["front_suspension", "damper"], "bumpsetting"));
    put(Field::ForkRebound, at(&["front_suspension", "damper"], "reboundsetting"));
    put(Field::ForkOffset, at(&["steer", "forkoffset"], "setting"));
    put(Field::ShockSpring, at(&["rear_suspension", "spring"], "setting"));
    put(Field::ShockPreload, at(&["rear_suspension", "preload"], "setting"));
    put(Field::ShockLowCompression, at(&["rear_suspension", "damper"], "bumpsetting"));
    put(Field::ShockHighCompression, at(&["rear_suspension", "damper"], "fastbumpsetting"));
    put(Field::ShockRebound, at(&["rear_suspension", "damper"], "reboundsetting"));
    put(Field::RodLength, at(&["rear_suspension"], "rodlengthsetting"));
    put(Field::SwingarmLength, at(&["rear_suspension"], "lengthsetting"));
    put(Field::FrontSprocket, find_num(root, "fsprocketsetting"));
    put(Field::RearSprocket, find_num(root, "rsprocketsetting"));
    put(Field::FrontTyre, block_at(root, &["wheel0"]).and_then(|n| num(n, "tyresetting")));
    put(Field::RearTyre, block_at(root, &["wheel1"]).and_then(|n| num(n, "tyresetting")));
    put(Field::FrontPressure, block_at(root, &["wheel0"]).and_then(|n| num(n, "pressuresetting")));
    put(Field::RearPressure, block_at(root, &["wheel1"]).and_then(|n| num(n, "pressuresetting")));
    out
}

/// Every setting of a setup file, as the bike's own defaults, for a rider who has never saved
/// one. `None` when the bike doesn't say enough to build one.
///
/// Three slots have no garage label and are filled anyway, because leaving them at zero is a
/// change rather than a default: each gear's ratio (all six the same gear otherwise), the
/// steer offset, and the fuel load — which at zero is very nearly an empty tank.
/// Slots the bike says nothing about stay at zero, which is the first option in their list.
pub fn default_slots(root: &CfgNode, gears: usize) -> Option<Vec<u32>> {
    if gears == 0 || gears > crate::stp::MAX_GEARS {
        return None;
    }
    let gearbox = root.block("gearbox")?;
    let mut slots = vec![0u32; crate::stp::SLOTS + gears];
    let mut put = |at: usize, v: u32| {
        if let Some(s) = slots.get_mut(at / 4) {
            *s = v;
        }
    };
    for (f, v) in defaults(root) {
        put(f.at(gears), v);
    }
    if let Some(v) = block_at(root, &["steer", "offset"]).and_then(|n| num(n, "setting")) {
        put(4, v);
    }
    for i in 0..gears {
        put(crate::stp::GEARS_FROM + 4 * i, num(gearbox, &format!("gear{i}")).unwrap_or(i as u32));
    }
    if let Some(v) = find_num(root, "capacitysetting") {
        put(crate::stp::GEARS_FROM + 4 * gears + 0x20, v);
    }
    Some(slots)
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

    fn close(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    /// A rider on the game's default setup has no file to copy, so the bike's own cfg has to
    /// supply every slot. The ones with no garage label matter most: all six gears at ratio 0
    /// is a bike with one gear, and the fuel load at 0 is the smallest tank the bike offers.
    #[test]
    fn a_bike_says_what_its_own_default_setup_is() {
        let cfg = format!(
            "{CFG_85}\nfueltank\n{{\n\tCapacityRange = 1, 0.5, 7.5\n\tCapacitySetting = 13\n}}\n\
             steer\n{{\n\toffset\n\t{{\n\t\trange = -0.005, 0.001, 0.005\n\t\tsetting = 5\n\t}}\n}}\n"
        );
        let root = cfg::parse(cfg.as_bytes());
        let d = defaults(&root);
        assert_eq!(d[&Field::FrontSprocket], 2);
        assert_eq!(d[&Field::ForkSpring], 5);
        assert_eq!(d[&Field::ForkCompression], 3);
        assert_eq!(d[&Field::ShockPreload], 12);
        // The bike lists no fork ride height, so the coach claims no default for it.
        assert!(!d.contains_key(&Field::ForkHeight));

        let slots = default_slots(&root, 6).expect("a six-speed's defaults");
        assert_eq!(slots.len(), crate::stp::SLOTS + 6);
        let s = crate::stp::Setup::build("2027_K85M", 6, &slots, crate::stp::SETUP_VERSION).expect("a setup file");
        assert_eq!(s.get(Field::FrontSprocket), 2);
        assert_eq!(s.get(Field::ForkSpring), 5);
        assert_eq!(s.get(Field::ShockPreload), 12);
        // Each gear on its own ratio, the steer offset and the fuel, none of them zero.
        let gears: Vec<u32> = (0..6).map(|i| slots[(crate::stp::GEARS_FROM + 4 * i) / 4]).collect();
        assert_eq!(gears, [0, 1, 2, 3, 4, 5]);
        assert_eq!(slots[1], 5, "the steer offset");
        assert_eq!(slots[(crate::stp::GEARS_FROM + 4 * 6 + 0x20) / 4], 13, "the fuel load");
        // A gear count the bike can't have builds nothing rather than a short file.
        assert!(default_slots(&root, 0).is_none());
        assert!(default_slots(&cfg::parse(b"front_suspension\n{\n}\n"), 5).is_none());
    }

    #[test]
    fn the_swingarm_list_comes_from_the_geom_either_way_round() {
        // A 2023 KTM 450: short to long, 9 positions over 40 mm.
        let ktm = swingarm(&cfg::parse(b"rwheel_min = 0, 0.0331, -0.5758\nrwheel_max = 0, 0.0331, -0.6162\nswingarm_steps = 8\n")).unwrap();
        assert_eq!(ktm.count, 9);
        assert!(close(&[ktm.values[0], ktm.values[8]], &[0.5758, 0.6162]));
        // A 2003 RM250 lists long to short.
        let rm = swingarm(&cfg::parse(b"rwheel_min = 0, -0.0409, -0.6065\nrwheel_max = 0, -0.0457, -0.566\nswingarm_steps = 8\n")).unwrap();
        assert!(rm.values[8] < rm.values[0]);
        // No steps: the bike can't change it.
        assert!(swingarm(&cfg::parse(b"rwheel_min = 0, 0, -0.5\nrwheel_max = 0, 0, -0.5\nswingarm_steps = 0\n")).is_none());
    }
}
