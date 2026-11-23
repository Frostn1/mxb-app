//! The rider's tyres: the pressure range and the pressure each is made for, from the game's
//! `.tyre` files. A bike's cfg names its tyre pack (`tyres { id }`) and, per wheel, the tyres it
//! can run (`wheel0 { tyre0 { id } }`); the setup picks one. The OEM packs are locked and open
//! with the locked-content reader, as bike files do.

use crate::bikecfg::{list, Options};
use mxb_core::cfg::{self, CfgNode};
use mxb_core::{library, pkz};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Tyre {
    pub id: String,
    pub name: String,
    /// Pressure options, kPa.
    pub pressure: Options,
    /// The pressure the tyre is made for, kPa.
    pub optimal: f32,
}

/// The tyre id a wheel (0 front, 1 rear) runs, for the setup's tyre choice.
pub fn tyre_id(bike: &CfgNode, wheel: usize, choice: u32) -> Option<(String, String)> {
    let w = bike.block(&format!("wheel{wheel}"))?;
    let t = w.block(&format!("tyre{choice}")).or_else(|| w.block("tyre0"))?;
    Some((t.get("id")?.trim().to_string(), t.get("name").unwrap_or("").trim().to_string()))
}

/// Where a tyre pack can be: the user's mods, then the game's own folder.
fn pack_places(mods_path: &str, install_dir: &str, pack: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [library::mods_subdir(mods_path, "mods/tyres"), Path::new(install_dir).join("tyres")] {
        out.push(root.join(format!("{pack}.pkz")));
        out.push(root.join(pack));
    }
    out
}

fn is_tyre(n: &str) -> bool {
    n.to_ascii_lowercase().ends_with(".tyre")
}

/// Every `.tyre` file in a pack, loose or packed.
fn tyre_files(place: &Path) -> Vec<Vec<u8>> {
    if place.is_dir() {
        return std::fs::read_dir(place)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_str().is_some_and(is_tyre))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .collect();
    }
    if place.is_file() {
        return pkz::read_selected(place, is_tyre).map(|v| v.into_iter().map(|(_, b)| b).collect()).unwrap_or_default();
    }
    Vec::new()
}

/// Reads one `.tyre` file's pressure range and optimum.
pub fn parse(bytes: &[u8]) -> Option<(String, Options, f32)> {
    let t = cfg::parse(bytes);
    let id = t.get("id")?.trim().to_string();
    let pressure = list(t.block("pressure")?)?;
    let optimal: f32 = t.get("optimalpressure")?.trim().parse().ok()?;
    Some((id, pressure, optimal))
}

/// The tyre a wheel runs on this bike with this setup choice, when its file can be found.
pub fn load(mods_path: &str, install_dir: &str, bike: &CfgNode, wheel: usize, choice: u32) -> Option<Tyre> {
    let pack = bike.block("tyres")?.get("id")?.trim().to_string();
    let (id, name) = tyre_id(bike, wheel, choice)?;
    for place in pack_places(mods_path, install_dir, &pack) {
        for bytes in tyre_files(&place) {
            if let Some((found, pressure, optimal)) = parse(&bytes) {
                if found.eq_ignore_ascii_case(&id) {
                    return Some(Tyre { id, name, pressure, optimal });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed OEM front tyre file.
    const FRONT: &str = "
type = bike
id = OEM_MXF_IS8010021
OptimalPressure = 85.00
pressure
{
	range = 55.0, 2.5, 125.0
	setting = 12
}
";

    #[test]
    fn reads_a_tyre_files_pressure_range_and_optimum() {
        let (id, p, optimal) = parse(FRONT.as_bytes()).unwrap();
        assert_eq!(id, "OEM_MXF_IS8010021");
        assert_eq!(p.count, 29);
        assert_eq!(p.values[12], 85.0);
        assert_eq!(optimal, 85.0);
    }

    #[test]
    fn a_wheel_runs_the_tyre_its_setup_picks() {
        let bike = cfg::parse(
            b"tyres\n{\n\tid = oem_mx\n}\nwheel1\n{\n\ttyre0\n\t{\n\t\tid = OEM_MXR_IS1009019\n\t}\n\ttyre1\n\t{\n\t\tid = OEM_MXR_IS1109019\n\t\tname = OEM MX IS 110/90-19\n\t}\n}\n",
        );
        assert_eq!(tyre_id(&bike, 1, 1).unwrap().0, "OEM_MXR_IS1109019");
        assert_eq!(tyre_id(&bike, 1, 0).unwrap().0, "OEM_MXR_IS1009019");
        // A choice the bike doesn't have falls back to its first tyre.
        assert_eq!(tyre_id(&bike, 1, 7).unwrap().0, "OEM_MXR_IS1009019");
    }
}
