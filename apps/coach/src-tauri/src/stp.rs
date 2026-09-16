//! MX Bikes setup files (`.stp`): read the rider's setup and write a changed copy the game loads.
//!
//! The layout is the game's own, from its reader and writer in `mxbikes.exe` (0x140198e43 and
//! 0x140199330), checked against two real setups and DialedMX's stock arrays:
//!
//! - `0x00` u32 magic, `0x04` u16 format version, `0x06` 32-byte bike id, `0x26` u16 setup version
//! - then u32 settings, each an index into the bike's own list of options
//! - one u32 per gear, 20 settings in, so everything after the gearbox moves with the gear count
//! - a u32 notes length and the notes at the end
//!
//! **The setup version is only in a `0x0116` file.** The reader takes it as `0x0100` for a
//! `0x0115` one and never reads those two bytes, so an older file's settings start at `0x26`
//! rather than `0x28` and every offset after it shifts with them.
//!
//! The game refuses a file whose bike id or setup version doesn't match the bike, so a copy
//! starts from a setup the game itself wrote wherever there is one.

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const MAGIC: u32 = 0x4593_53C8;
const CURRENT: u16 = 0x0116;
const OLD: u16 = 0x0115;
const BIKE_ID: std::ops::Range<usize> = 0x06..0x26;
/// The setup version a fresh file carries; every bike the coach has seen wants this one.
pub const SETUP_VERSION: u16 = 0x0100;
/// Settings before the gearbox.
const BEFORE_GEARS: usize = 20;
/// Settings either side of the gearbox — what a file holds besides one slot per gear.
pub(crate) const SLOTS: usize = 44;
pub(crate) const MAX_GEARS: usize = 12;

/// Where the settings start, which the format version decides: a `0x0115` file has no setup
/// version in its header, so everything sits two bytes earlier.
fn settings_at(version: u16) -> Option<usize> {
    match version {
        CURRENT => Some(0x28),
        OLD => Some(0x26),
        _ => None,
    }
}

/// The settings the coach changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    ForkOffset,
    SwingarmLength,
    ForkSpring,
    ForkCompression,
    ForkRebound,
    ForkPreload,
    ForkHeight,
    ForkOil,
    ShockSpring,
    ShockLowCompression,
    ShockHighCompression,
    ShockRebound,
    ShockPreload,
    RodLength,
    FrontSprocket,
    RearSprocket,
    /// Which of the bike's tyres each wheel runs.
    FrontTyre,
    RearTyre,
    /// Positions in the tyre's own pressure list.
    FrontPressure,
    RearPressure,
}

impl Field {
    /// Bytes from the first setting — the file offset is this plus where the settings start.
    pub(crate) fn at(self, gears: usize) -> usize {
        let t = 4 * (BEFORE_GEARS + gears);
        match self {
            Field::ForkOffset => 0x08,
            Field::SwingarmLength => 0x10,
            Field::ForkSpring => 0x14,
            Field::ForkCompression => 0x1C,
            Field::ForkRebound => 0x20,
            Field::ForkPreload => 0x24,
            Field::ForkHeight => 0x28,
            Field::ForkOil => 0x2C,
            Field::ShockSpring => 0x30,
            Field::ShockLowCompression => 0x34,
            Field::ShockHighCompression => 0x38,
            Field::ShockRebound => 0x3C,
            Field::ShockPreload => 0x40,
            Field::RodLength => 0x48,
            Field::FrontSprocket => t,
            Field::RearSprocket => t + 0x04,
            Field::FrontTyre => t + 0x08,
            Field::RearTyre => t + 0x0C,
            Field::FrontPressure => t + 0x18,
            Field::RearPressure => t + 0x1C,
        }
    }
}

/// The game writes the fork spring twice; both copies move together.
const FORK_SPRING_TWIN: usize = 0x18;
/// The gearbox: one slot per gear, and the first of them.
pub(crate) const GEARS_FROM: usize = 4 * BEFORE_GEARS;

#[derive(Clone, Debug)]
pub struct Setup {
    bytes: Vec<u8>,
    /// Where the settings start, which the format version decides.
    base: usize,
    pub gears: usize,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

impl Setup {
    /// Reads a setup. `gears` settles the rare file whose size fits two gear counts.
    pub fn parse(bytes: &[u8], gears: Option<usize>) -> Result<Setup, String> {
        if u32_at(bytes, 0) != Some(MAGIC) {
            return Err("not an MX Bikes setup".into());
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let Some(base) = settings_at(version) else {
            return Err(format!("setup format {version:#06x} isn't one the coach knows"));
        };
        // The notes length sits right after the last setting, and the notes fill the rest.
        let fits: Vec<usize> = (0..=MAX_GEARS)
            .filter(|&n| {
                let end = base + 4 * (SLOTS + n);
                u32_at(bytes, end).is_some_and(|notes| bytes.len() == end + 4 + notes as usize)
            })
            .collect();
        let gears = match (fits.as_slice(), gears) {
            ([one], _) => *one,
            (many, Some(g)) if many.contains(&g) => g,
            ([], _) => return Err("the setup's size doesn't match its layout".into()),
            _ => return Err("the setup's gear count is ambiguous".into()),
        };
        Ok(Setup { bytes: bytes.to_vec(), base, gears })
    }

    /// A setup file built from scratch: the header the bike wants, then one index per slot.
    /// For a rider who has never saved one, so there is nothing of theirs to copy.
    pub fn build(bike_id: &str, gears: usize, slots: &[u32]) -> Result<Setup, String> {
        if bike_id.len() >= BIKE_ID.len() {
            return Err("that bike's name is too long for a setup file".into());
        }
        if slots.len() != SLOTS + gears {
            return Err("the bike's settings don't fill a setup file".into());
        }
        let mut b = MAGIC.to_le_bytes().to_vec();
        b.extend_from_slice(&CURRENT.to_le_bytes());
        let mut id = [0u8; 32];
        id[..bike_id.len()].copy_from_slice(bike_id.as_bytes());
        b.extend_from_slice(&id);
        b.extend_from_slice(&SETUP_VERSION.to_le_bytes());
        for v in slots {
            b.extend_from_slice(&v.to_le_bytes());
        }
        // No notes.
        b.extend_from_slice(&0u32.to_le_bytes());
        Setup::parse(&b, Some(gears))
    }

    pub fn bike_id(&self) -> String {
        let raw = &self.bytes[BIKE_ID];
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    }

    pub fn get(&self, f: Field) -> u32 {
        u32_at(&self.bytes, self.base + f.at(self.gears)).unwrap_or(0)
    }

    pub fn set(&mut self, f: Field, v: u32) {
        let mut put = |at: usize| {
            let at = self.base + at;
            if let Some(slot) = self.bytes.get_mut(at..at + 4) {
                slot.copy_from_slice(&v.to_le_bytes());
            }
        };
        put(f.at(self.gears));
        if f == Field::ForkSpring {
            put(FORK_SPRING_TWIN);
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A setup's name without the " (coach)" or " (coach 3)" the coach adds, so a copy of a copy
/// is numbered rather than nested.
pub fn base_name(stem: &str) -> &str {
    let Some(open) = stem.rfind(" (coach") else { return stem };
    let Some(n) = stem[open + " (coach".len()..].strip_suffix(')') else { return stem };
    let numbered = n.strip_prefix(' ').is_some_and(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()));
    if n.is_empty() || numbered {
        &stem[..open]
    } else {
        stem
    }
}

/// The names the coach saves copies of `base` under, in order.
pub fn coach_names(base: &str) -> impl Iterator<Item = String> + '_ {
    (1..100).map(move |n| if n == 1 { format!("{base} (coach)") } else { format!("{base} (coach {n})") })
}

/// The longest setup name the coach writes. The game hands a plugin the name in a 100-byte
/// field, and a name is a file name besides, so a fresh one stays well short of both.
const MAX_NAME: usize = 48;

/// The names a setup the coach makes from nothing goes under: "Coach <track>", then numbered.
/// The rider rode the game's default, so there is no name of theirs to build on.
pub fn fresh_names(track: &str) -> impl Iterator<Item = String> + '_ {
    let clean: String = track.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').collect();
    let clean = clean.trim().to_string();
    let base = if clean.is_empty() { "Coach".to_string() } else { format!("Coach {clean}") };
    let base = base.chars().take(MAX_NAME - 4).collect::<String>();
    (1..100).map(move |n| if n == 1 { base.clone() } else { format!("{base} {n}") })
}

/// Every profile's `setups` folder, in a fixed order.
pub fn setup_dirs(profiles: &Path) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(profiles)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("setups"))
        .filter(|p| p.is_dir())
        .collect();
    all.sort();
    all
}

/// Every setup the rider has for this bike, the most useful first: this track, then the ones
/// for every track, then any other track's. Newest first within each, so a copy starts from
/// the tune they last worked on.
pub fn setups_for_bike(profiles: &Path, track: &str, bike: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let dirs = setup_dirs(profiles);
    let mut tier = |pick: &dyn Fn(&str) -> bool| {
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for setups in &dirs {
            let places = std::fs::read_dir(setups).into_iter().flatten().flatten();
            for place in places.filter(|e| e.path().is_dir()) {
                let name = place.file_name().to_string_lossy().into_owned();
                if !pick(&name) {
                    continue;
                }
                let files = std::fs::read_dir(place.path().join(bike)).into_iter().flatten().flatten();
                for f in files {
                    let p = f.path();
                    if p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("stp")) {
                        let when = f.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                        found.push((when, p));
                    }
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        out.extend(found.into_iter().map(|(_, p)| p));
    };
    tier(&|n: &str| n.eq_ignore_ascii_case(track));
    tier(&|n: &str| n.eq_ignore_ascii_case("common"));
    tier(&|n: &str| !n.eq_ignore_ascii_case(track) && !n.eq_ignore_ascii_case("common"));
    out
}

/// Where a setup the coach makes for this track and bike goes: under the profile that already
/// keeps setups, beside the rider's own if they have any here.
pub fn fresh_dir(profiles: &Path, track: &str, bike: &str) -> Option<PathBuf> {
    let dirs = setup_dirs(profiles);
    let holds_setups = |d: &PathBuf| std::fs::read_dir(d).into_iter().flatten().flatten().next().is_some();
    let home = dirs.iter().find(|d| holds_setups(d)).or_else(|| dirs.first())?;
    Some(home.join(track).join(bike))
}

/// Where a recorded setup lives. The plugin reports the name only: a leading ':' means a
/// setup for every track (`setups\common`), else it's saved under the track. `*` names a
/// `.stt` file, which isn't a setup the coach reads.
pub fn locate(profiles: &Path, name: &str, track: &str, bike: &str) -> Option<PathBuf> {
    let (common, name) = match name.strip_prefix(':') {
        Some(n) => (true, n),
        None => (false, name),
    };
    if name.is_empty() || name.starts_with('*') || name.eq_ignore_ascii_case("default") {
        return None;
    }
    let file = format!("{name}.stp");
    let places: Vec<&str> = if common { vec!["common", track] } else { vec![track, "common"] };
    let all = setup_dirs(profiles);
    for place in places {
        for setups in &all {
            let p = setups.join(place).join(bike).join(&file);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A setup as the game writes it: header, rake and steer offset, then `settings` from
    /// the fork offset on, ending with the notes length.
    pub(crate) fn file(bike: &str, settings: &[u32]) -> Vec<u8> {
        let mut b = MAGIC.to_le_bytes().to_vec();
        b.extend_from_slice(&0x0116u16.to_le_bytes());
        let mut id = [0u8; 32];
        id[..bike.len()].copy_from_slice(bike.as_bytes());
        b.extend_from_slice(&id);
        b.extend_from_slice(&0x0100u16.to_le_bytes());
        for v in [0u32, 5].iter().chain(settings) {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    /// A real 85 SX sand setup (six gears, 244 bytes), setting for setting.
    pub(crate) const SAND_85: [u32; 49] = [
        0, 0, 2, 3, 3, 3, 2, 13, 0, 0, 3, 3, 0, 4, 7, 0, 0, 0, 0, 1, 2, 3, 4, 5, 2, 5, 0, 0, 0, 0, 13, 10,
        13, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    #[test]
    fn reads_a_real_six_speed_setup() {
        let bytes = file("2027_K85M", &SAND_85);
        assert_eq!(bytes.len(), 244);
        let s = Setup::parse(&bytes, None).unwrap();
        assert_eq!(s.gears, 6);
        assert_eq!(s.bike_id(), "2027_K85M");
        assert_eq!(s.get(Field::ForkCompression), 3);
        assert_eq!(s.get(Field::ShockPreload), 7);
        // After the six gears: 13T front (index 2), 52T rear (index 5), pressures 13 and 10.
        assert_eq!((s.get(Field::FrontSprocket), s.get(Field::RearSprocket)), (2, 5));
        let pressures = 0x28 + GEARS_FROM + 4 * 6 + 0x18;
        assert_eq!((u32_at(&bytes, pressures), u32_at(&bytes, pressures + 4)), (Some(13), Some(10)));
        // The gearbox starts where the game's own reader puts it.
        assert_eq!(0x28 + GEARS_FROM, 0x78);
    }

    /// The exe reads the setup version only from a `0x0116` file, so a `0x0115` one has two
    /// bytes fewer before its settings and everything after shifts with them. The coach used
    /// to accept those files and then read them at the newer layout's offsets.
    #[test]
    fn an_older_file_has_no_setup_version_and_its_settings_start_earlier() {
        let new = file("2027_K85M", &SAND_85);
        // The same settings, written the way the older format holds them.
        let mut old = new[..6].to_vec();
        old[4..6].copy_from_slice(&0x0115u16.to_le_bytes());
        old.extend_from_slice(&new[6..0x26]);
        old.extend_from_slice(&new[0x28..]);
        assert_eq!(old.len(), new.len() - 2);
        let s = Setup::parse(&old, None).unwrap();
        assert_eq!((s.gears, s.bike_id()), (6, "2027_K85M".to_string()));
        assert_eq!((s.get(Field::FrontSprocket), s.get(Field::RearSprocket)), (2, 5));
        assert_eq!(s.get(Field::ShockPreload), 7);
        // And a change still lands on the setting it names, two bytes earlier.
        let mut s = s;
        s.set(Field::RearSprocket, 3);
        assert_eq!(Setup::parse(s.bytes(), None).unwrap().get(Field::RearSprocket), 3);
        assert_eq!(s.bytes().len(), old.len());
    }

    #[test]
    fn a_setup_built_from_nothing_reads_back_as_the_game_writes_one() {
        // The rake and steer offset the helper writes, then every setting but the last, which
        // is the notes length `build` puts on itself.
        let mut all = vec![0u32, 5];
        all.extend_from_slice(&SAND_85[..SAND_85.len() - 1]);
        assert_eq!(all.len(), SLOTS + 6);
        let s = Setup::build("2027_K85M", 6, &all).unwrap();
        assert_eq!(s.bytes(), file("2027_K85M", &SAND_85).as_slice(), "byte for byte what the game writes");
        assert_eq!(s.gears, 6);
        assert_eq!(s.get(Field::RearSprocket), 5);
        // A gear count the slots don't fill is refused rather than written short.
        assert!(Setup::build("2027_K85M", 5, &all).is_err());
        assert!(Setup::build(&"x".repeat(40), 6, &all).is_err());
    }

    #[test]
    fn a_fresh_setup_is_named_after_the_track_and_never_endless() {
        let names: Vec<String> = fresh_names("indiana").take(3).collect();
        assert_eq!(names, ["Coach indiana", "Coach indiana 2", "Coach indiana 3"]);
        assert_eq!(fresh_names("").next().unwrap(), "Coach");
        // A track with punctuation in its id still makes a file name, and a long one is cut.
        assert_eq!(fresh_names("my/track:2").next().unwrap(), "Coach mytrack2");
        assert!(fresh_names(&"x".repeat(200)).next().unwrap().len() <= MAX_NAME);
    }

    #[test]
    fn the_riders_other_setups_for_a_bike_come_back_this_track_first() {
        let root = std::env::temp_dir().join(format!("coach-donor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bike = "2027_K85M";
        for (place, name) in [("indiana", "here"), ("common", "every"), ("elsewhere", "there")] {
            let dir = root.join("Frost").join("setups").join(place).join(bike);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{name}.stp")), b"x").unwrap();
        }
        // Another bike's setups are never offered.
        let other = root.join("Frost").join("setups").join("indiana").join("MX1OEM_other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("nope.stp"), b"x").unwrap();

        let found = setups_for_bike(&root, "indiana", bike);
        let names: Vec<String> = found.iter().map(|p| p.file_stem().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["here", "every", "there"]);
        assert_eq!(fresh_dir(&root, "indiana", bike), Some(root.join("Frost").join("setups").join("indiana").join(bike)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_gearbox_moves_everything_after_it() {
        let mut five = SAND_85.to_vec();
        five.remove(18);
        let s = Setup::parse(&file("MX2OEM_TEST", &five), None).unwrap();
        assert_eq!(s.gears, 5);
        assert_eq!(s.bytes().len(), 240);
        assert_eq!(s.get(Field::RearSprocket), 5);
    }

    #[test]
    fn a_change_keeps_everything_else_and_both_fork_springs() {
        let bytes = file("2027_K85M", &SAND_85);
        let mut s = Setup::parse(&bytes, None).unwrap();
        s.set(Field::ForkSpring, 5);
        s.set(Field::RearSprocket, 3);
        let out = s.bytes();
        assert_eq!(out.len(), bytes.len());
        assert_eq!(u32_at(out, 0x3C), Some(5));
        assert_eq!(u32_at(out, 0x40), Some(5));
        let changed: Vec<usize> = (0..out.len()).filter(|&i| out[i] != bytes[i]).collect();
        assert!(changed.iter().all(|&i| (0x3C..0x44).contains(&i) || (0x94..0x98).contains(&i)), "{changed:?}");
        assert_eq!(Setup::parse(out, None).unwrap().get(Field::RearSprocket), 3);
    }

    #[test]
    fn notes_stay_where_they_are() {
        let mut bytes = file("2027_K85M", &SAND_85);
        let len = bytes.len();
        bytes[len - 4..].copy_from_slice(&5u32.to_le_bytes());
        bytes.extend_from_slice(b"sandy");
        let s = Setup::parse(&bytes, None).unwrap();
        assert_eq!(s.gears, 6);
        assert!(s.bytes().ends_with(b"sandy"));
    }

    #[test]
    fn refuses_what_is_not_a_setup() {
        assert!(Setup::parse(b"PK\x03\x04 and more bytes than a header needs", None).is_err());
        let mut bytes = file("2027_K85M", &SAND_85);
        bytes.push(0);
        assert!(Setup::parse(&bytes, None).is_err());
    }

    #[test]
    fn a_copy_of_a_copy_is_numbered_not_nested() {
        assert_eq!(base_name("frost-race"), "frost-race");
        assert_eq!(base_name("frost-race (coach)"), "frost-race");
        assert_eq!(base_name("frost-race (coach 3)"), "frost-race");
        assert_eq!(base_name("my (coach) setup"), "my (coach) setup");
        assert_eq!(base_name("frost-race (coach x)"), "frost-race (coach x)");
        let names: Vec<String> = coach_names("frost-race").take(3).collect();
        assert_eq!(names, ["frost-race (coach)", "frost-race (coach 2)", "frost-race (coach 3)"]);
    }

    #[test]
    fn finds_a_common_setup_and_a_track_setup() {
        let root = std::env::temp_dir().join(format!("coach-stp-{}", std::process::id()));
        let common = root.join("Frost").join("setups").join("common").join("2027_K85M");
        let track = root.join("Frost").join("setups").join("indiana").join("2027_K85M");
        std::fs::create_dir_all(&common).unwrap();
        std::fs::create_dir_all(&track).unwrap();
        std::fs::write(common.join("frost-race.stp"), b"x").unwrap();
        std::fs::write(track.join("wet.stp"), b"x").unwrap();
        assert_eq!(locate(&root, ":frost-race", "indiana", "2027_K85M"), Some(common.join("frost-race.stp")));
        assert_eq!(locate(&root, "wet", "indiana", "2027_K85M"), Some(track.join("wet.stp")));
        assert_eq!(locate(&root, "Default", "indiana", "2027_K85M"), None);
        assert_eq!(locate(&root, ":missing", "indiana", "2027_K85M"), None);
        std::fs::remove_dir_all(&root).ok();
    }
}
