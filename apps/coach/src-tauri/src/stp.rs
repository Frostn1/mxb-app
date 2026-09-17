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
/// What the game uses when a bike's cfg doesn't name a `setup_version` of its own — the exe
/// defaults to this and writes the bike's value when there is one.
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
    /// `version` is the bike's own `setup_version` from its cfg, which is what the game writes
    /// and what it checks a file against. Hardcoding 0x0100 was right only for a bike that
    /// doesn't declare one — the exe defaults to 0x0100 in exactly that case, and writes the
    /// bike's value otherwise, so a bike that declares its own would reject the file.
    pub fn build(bike_id: &str, gears: usize, slots: &[u32], version: u16) -> Result<Setup, String> {
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
        b.extend_from_slice(&version.to_le_bytes());
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

/// The game's own record of which setup a bike loads, in the same folder as the setups it
/// names: `[setup]` with one key per session type.
///
/// From the exe, which reads and writes it through its ini helper at two call sites
/// (0x14006b503 and 0x14009c4f3): the path is `profiles\<p>\setups\<track>\<bike>\default.ini`,
/// the section is `setup`, and the keys are `testing`, `wet_testing`, `qualify` and `race`.
/// The same helper reads `profile.ini`'s `[riding_style]` with the bike id as the key
/// (0x140092910), which is what settles which argument is the section and which the key.
pub const DEFAULT_INI: &str = "default.ini";
const SETUP_SECTION: &str = "setup";
/// The session a practice lap is ridden in, and the wet version of it.
const DRY_KEY: &str = "testing";
const WET_KEY: &str = "wet_testing";

/// Point the game at `name` for practice in `dir`, keeping every other key.
///
/// Only the practice keys are touched. `qualify` and `race` are the rider's own choices about
/// a race weekend, and a coach setup written from a practice lap has no business changing
/// what they line up on.
pub fn select_default(dir: &Path, name: &str, wet: bool) -> Result<(), String> {
    let mut keys: Vec<(&str, String)> = vec![(DRY_KEY, name.to_string())];
    if wet {
        keys.push((WET_KEY, name.to_string()));
    }
    crate::ini::write(&dir.join(DEFAULT_INI), SETUP_SECTION, &keys)
}

/// Where the game reads the record for this track, given any setup of the same bike in the
/// same profile: `<profile>\setups\<track>\<bike>`.
///
/// There is no record beside a `common` setup — the game only ever reads one under a track —
/// so a copy of one has to be pointed at from here.
pub fn track_dir(file: &Path, track: &str) -> Option<PathBuf> {
    let bike = file.parent()?;
    let setups = bike.parent()?.parent()?;
    Some(setups.join(track).join(bike.file_name()?))
}

/// How the record names the setup in `dir`: plainly for one under a track, and with the ':'
/// the game marks a `common` setup by (see `locate`), which is the spelling the plugin hands
/// back when the rider is riding it.
pub fn reference(dir: &Path, name: &str) -> String {
    match dir.parent().and_then(|p| p.file_name()) {
        Some(place) if place.eq_ignore_ascii_case("common") => format!(":{name}"),
        _ => name.to_string(),
    }
}

/// Which setup `dir` currently loads for practice, if it says.
pub fn selected_default(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(DEFAULT_INI)).ok()?;
    let v = crate::ini::get(&crate::ini::read_section(&text, SETUP_SECTION), DRY_KEY)?.trim().to_string();
    (!v.is_empty()).then_some(v)
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

/// Every folder that can hold this bike's setups for one place, deepest first.
///
/// The game uses `setups\<track>\<layout>\<bike>` on a track that has layouts and
/// `setups\<track>\<bike>` on one that doesn't — two different paths, picked at run time.
/// `setups\common\<bike>` never has a layout level. Looking only at the shallow one missed
/// every setup on a layout track, and wrote `default.ini` a directory above where the game
/// reads it, which is the same silent miss as saving to the wrong folder.
fn bike_dirs(setups: &Path, place: &str, bike: &str) -> Vec<PathBuf> {
    let here = setups.join(place);
    let mut out = Vec::new();
    // A layout is a folder under the track that holds the bike's folder, so it is told apart
    // from the bike's own folder by what is inside it rather than by name.
    for layout in std::fs::read_dir(&here).into_iter().flatten().flatten() {
        let d = layout.path().join(bike);
        if d.is_dir() {
            out.push(d);
        }
    }
    let shallow = here.join(bike);
    if shallow.is_dir() || out.is_empty() {
        out.push(shallow);
    }
    out
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
                for dir in bike_dirs(setups, &name, bike) {
                    let files = std::fs::read_dir(&dir).into_iter().flatten().flatten();
                    for f in files {
                        let p = f.path();
                        if p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("stp")) {
                            let when = f.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                            found.push((when, p));
                        }
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
    // Beside the rider's own where they have any here, which is what settles the layout level
    // without having to know the layout's name: the game already put them in the right place.
    Some(bike_dirs(home, track, bike).into_iter().next().unwrap_or_else(|| home.join(track).join(bike)))
}

/// Where a recorded setup lives. The plugin reports the name only: a leading ':' means a
/// setup for every track (`setups\common`), else it's saved under the track. `*` names a
/// `.stt` file, which isn't a setup the coach reads.
pub fn locate(profiles: &Path, name: &str, track: &str, bike: &str) -> Option<PathBuf> {
    let (common, name) = match name.strip_prefix(':') {
        Some(n) => (true, n),
        None => (false, name),
    };
    // `*` is a `.stt`, and a leading backslash is a setup shipped inside the bike itself
    // (`bikes\<bike>\setups\`), which is not under the profile at all. Neither is a file
    // this looks for, and building a path from one gave a wrong path rather than a clean miss.
    if name.is_empty() || name.starts_with('*') || name.starts_with('\\') || name.eq_ignore_ascii_case("default") {
        return None;
    }
    let file = format!("{name}.stp");
    let places: Vec<&str> = if common { vec!["common", track] } else { vec![track, "common"] };
    let all = setup_dirs(profiles);
    for place in places {
        for setups in &all {
            for dir in bike_dirs(setups, place, bike) {
                let p = dir.join(&file);
                if p.is_file() {
                    return Some(p);
                }
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
        let s = Setup::build("2027_K85M", 6, &all, SETUP_VERSION).unwrap();
        assert_eq!(s.bytes(), file("2027_K85M", &SAND_85).as_slice(), "byte for byte what the game writes");
        assert_eq!(s.gears, 6);
        assert_eq!(s.get(Field::RearSprocket), 5);
        // A gear count the slots don't fill is refused rather than written short.
        assert!(Setup::build("2027_K85M", 5, &all, SETUP_VERSION).is_err());
        assert!(Setup::build(&"x".repeat(40), 6, &all, SETUP_VERSION).is_err());
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

    /// The record lives under the track even for a setup kept for every track, and names a
    /// `common` setup the way the game does.
    #[test]
    fn the_record_for_a_common_setup_is_still_under_the_track() {
        let setups = Path::new("C:/profiles/Frost/setups");
        let common = setups.join("common").join("2027_K85M");
        let mine = setups.join("indiana").join("2027_K85M");
        assert_eq!(track_dir(&common.join("frost-race.stp"), "indiana"), Some(mine.clone()));
        assert_eq!(track_dir(&mine.join("wet.stp"), "indiana"), Some(mine.clone()));
        assert_eq!(track_dir(&mine.join("wet.stp"), "otherplace"), Some(setups.join("otherplace").join("2027_K85M")));
        assert_eq!(reference(&common, "frost-race"), ":frost-race");
        assert_eq!(reference(&mine, "frost-race (coach)"), "frost-race (coach)");
    }

    /// A track with layouts keeps its setups one folder deeper —
    /// `setups\\<track>\\<layout>\\<bike>` — and the game picks that path at run time. Looking
    /// only at the shallow one found none of the rider's setups there and wrote `default.ini`
    /// a directory above where the game reads it: the same silent miss as the wrong folder.
    #[test]
    fn a_track_with_layouts_keeps_its_setups_one_deeper() {
        let root = std::env::temp_dir().join(format!("coach-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let profiles = root.join("profiles");
        let setups = profiles.join("me").join("setups");
        let deep = setups.join("hangtown").join("national").join("2027_K85M");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("race.stp"), file("2027_K85M", &[0; SLOTS + 6])).unwrap();

        let found = locate(&profiles, "race", "hangtown", "2027_K85M").expect("a setup under the layout");
        assert_eq!(found, deep.join("race.stp"), "found through the layout folder");
        let all = setups_for_bike(&profiles, "hangtown", "2027_K85M");
        assert!(all.contains(&deep.join("race.stp")), "listed too: {all:?}");
        // A coach copy goes beside it, not a directory above where the game never looks.
        assert_eq!(fresh_dir(&profiles, "hangtown", "2027_K85M").unwrap(), deep, "the copy lands beside it");

        // A track with no layouts still uses the shallow path.
        let flat = setups.join("indiana").join("2027_K85M");
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(flat.join("race.stp"), file("2027_K85M", &[0; SLOTS + 6])).unwrap();
        assert_eq!(locate(&profiles, "race", "indiana", "2027_K85M").unwrap(), flat.join("race.stp"));
        assert_eq!(fresh_dir(&profiles, "indiana", "2027_K85M").unwrap(), flat);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A name the game hands over with a leading backslash is a setup shipped inside the bike
    /// (`bikes\\<bike>\\setups\\`), not one under the profile. Joining it built a
    /// drive-root-relative path — a wrong path rather than a clean miss.
    #[test]
    fn a_setup_that_came_with_the_bike_is_not_looked_for_in_the_profile() {
        let root = std::env::temp_dir().join(format!("coach-bundled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let profiles = root.join("profiles");
        std::fs::create_dir_all(profiles.join("me").join("setups")).unwrap();
        assert!(locate(&profiles, "\\stock", "indiana", "2027_K85M").is_none());
        assert!(locate(&profiles, "*telemetry", "indiana", "2027_K85M").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The game writes the bike's own `setup_version` and refuses a file that disagrees. It
    /// only uses 0x0100 when the bike's cfg doesn't name one, so a bike that does would have
    /// rejected every file the coach built.
    #[test]
    fn a_built_file_carries_the_bikes_own_setup_version() {
        let all = vec![0u32; SLOTS + 6];
        let s = Setup::build("2027_K85M", 6, &all, 0x0102).unwrap();
        let b = s.bytes();
        assert_eq!(u16::from_le_bytes([b[0x26], b[0x27]]), 0x0102, "the bike's version, not a constant");
        let plain = Setup::build("2027_K85M", 6, &all, SETUP_VERSION).unwrap();
        assert_eq!(u16::from_le_bytes([plain.bytes()[0x26], plain.bytes()[0x27]]), SETUP_VERSION);
    }
}
