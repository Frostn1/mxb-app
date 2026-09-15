//! MX Bikes setup files (`.stp`): read the rider's setup and write a changed copy the game loads.
//!
//! The layout is the game's own, from its reader and writer in `mxbikes.exe` (0x140198e43 and
//! 0x140199330), checked against two real setups and DialedMX's stock arrays:
//!
//! - `0x00` u32 magic, `0x04` u16 format version, `0x06` 32-byte bike id, `0x26` u16 setup version
//! - `0x28` u32 settings, each an index into the bike's own list of options
//! - one u32 per gear from `0x78`, so everything after the gearbox moves with the gear count
//! - a u32 notes length and the notes at the end
//!
//! The game refuses a file whose bike id or setup version doesn't match the bike, so a copy
//! always starts from the rider's own file and changes only settings.

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const MAGIC: u32 = 0x4593_53C8;
const VERSIONS: [u16; 2] = [0x0116, 0x0115];
const BIKE_ID: std::ops::Range<usize> = 0x06..0x26;
const GEARS_AT: usize = 0x78;
/// A file with no gears and no notes.
const BARE: usize = 0xDC;
const MAX_GEARS: usize = 12;

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
}

impl Field {
    fn offset(self, gears: usize) -> usize {
        let t = GEARS_AT + 4 * gears;
        match self {
            Field::ForkOffset => 0x30,
            Field::SwingarmLength => 0x38,
            Field::ForkSpring => 0x3C,
            Field::ForkCompression => 0x44,
            Field::ForkRebound => 0x48,
            Field::ForkPreload => 0x4C,
            Field::ForkHeight => 0x50,
            Field::ForkOil => 0x54,
            Field::ShockSpring => 0x58,
            Field::ShockLowCompression => 0x5C,
            Field::ShockHighCompression => 0x60,
            Field::ShockRebound => 0x64,
            Field::ShockPreload => 0x68,
            Field::RodLength => 0x70,
            Field::FrontSprocket => t,
            Field::RearSprocket => t + 0x04,
        }
    }
}

/// The game writes the fork spring twice; both copies move together.
const FORK_SPRING_TWIN: usize = 0x40;

#[derive(Clone, Debug)]
pub struct Setup {
    bytes: Vec<u8>,
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
        if !VERSIONS.contains(&version) {
            return Err(format!("setup format {version:#06x} isn't one the coach knows"));
        }
        // The notes length sits right after the last setting, and the notes fill the rest.
        let fits: Vec<usize> = (0..=MAX_GEARS)
            .filter(|&n| {
                u32_at(bytes, BARE - 4 + 4 * n)
                    .is_some_and(|notes| bytes.len() == BARE + 4 * n + notes as usize)
            })
            .collect();
        let gears = match (fits.as_slice(), gears) {
            ([one], _) => *one,
            (many, Some(g)) if many.contains(&g) => g,
            ([], _) => return Err("the setup's size doesn't match its layout".into()),
            _ => return Err("the setup's gear count is ambiguous".into()),
        };
        Ok(Setup { bytes: bytes.to_vec(), gears })
    }

    pub fn bike_id(&self) -> String {
        let raw = &self.bytes[BIKE_ID];
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    }

    pub fn get(&self, f: Field) -> u32 {
        u32_at(&self.bytes, f.offset(self.gears)).unwrap_or(0)
    }

    pub fn set(&mut self, f: Field, v: u32) {
        let at = f.offset(self.gears);
        self.bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
        if f == Field::ForkSpring {
            self.bytes[FORK_SPRING_TWIN..FORK_SPRING_TWIN + 4].copy_from_slice(&v.to_le_bytes());
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
    let mut all: Vec<PathBuf> = std::fs::read_dir(profiles)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("setups"))
        .filter(|p| p.is_dir())
        .collect();
    all.sort();
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
        let pressures = GEARS_AT + 4 * 6 + 0x18;
        assert_eq!((u32_at(&bytes, pressures), u32_at(&bytes, pressures + 4)), (Some(13), Some(10)));
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
