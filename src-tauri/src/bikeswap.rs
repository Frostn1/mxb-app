//! In-garage bike switching — class model + identity reading (cross-platform core).
//!
//! Lets the player swap their whole bike mid-session (offline only) restricted to
//! the race's class. This module is the platform-neutral half: it reads a bike's
//! **identity** (id / display name / class) from its PiBoSo config files and decides
//! **class membership**. The actual in-game load is driven by FrostMod (see
//! `frostmod.rs` for the command channel); nothing here touches the running game.
//!
//! Where the fields come from (verified against an OEM bike):
//!   `<Name>.ini`  `[info] name`  → display name
//!                 `[data] cat`   → class/category, e.g. "Classic MX1 OEM"
//!   `<Name>.cfg`  `ID = ...`     → bike ID (matches the folder; server `allowed_bikes`)
//!
//! Class matching mirrors the dedicated-server `[event] category` semantics: an empty
//! spec means Open (any bike), and multiple categories are separated by `/`.

use crate::{cfg, pkz};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BikeIdentity {
    /// Bike ID from `<name>.cfg` `ID` — matches the folder name and the server's
    /// `allowed_bikes` ids. Falls back to the folder/pkz stem when the cfg is missing.
    pub id: String,
    /// Human-readable name from `<name>.ini` `[info] name`. Falls back to the stem.
    pub name: String,
    /// Class/category from `<name>.ini` `[data] cat` (e.g. "Classic MX1 OEM").
    /// Empty when the bike declares none (treated as unclassified).
    pub class: String,
    /// Absolute path to the bike folder or `.pkz`.
    pub path: String,
}

/// Class/category from a bike `.ini`'s `[data] cat`. `cfg::parse` flattens section
/// headers, so a flat `cat` lookup is correct. Empty when absent.
pub fn class_from_ini(bytes: &[u8]) -> String {
    cfg::parse(bytes).get("cat").unwrap_or("").trim().to_string()
}

/// Display name from a bike `.ini`'s `[info] name`. Empty when absent.
pub fn name_from_ini(bytes: &[u8]) -> String {
    cfg::parse(bytes).get("name").unwrap_or("").trim().to_string()
}

/// Bike ID from a bike `.cfg`'s top-level `ID`. `None` when absent. (Engine-mapping
/// blocks also carry `id`, but those are nested, so a root lookup is unambiguous.)
pub fn id_from_cfg(bytes: &[u8]) -> Option<String> {
    let v = cfg::parse(bytes).get("id").unwrap_or("").trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Case-insensitive, whitespace-trimmed class equality.
pub fn class_eq(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Does `bike_class` satisfy the race's `allowed` spec? Mirrors the server's
/// `[event] category`: empty spec = Open (anything), otherwise a `/`-separated
/// list where matching any member passes.
pub fn class_matches(bike_class: &str, allowed: &str) -> bool {
    let allowed = allowed.trim();
    if allowed.is_empty() {
        return true;
    }
    allowed.split('/').any(|c| class_eq(bike_class, c))
}

/// Bikes from `bikes` whose class satisfies `allowed`. Preserves input order.
pub fn bikes_in_class<'a>(bikes: &'a [BikeIdentity], allowed: &str) -> Vec<&'a BikeIdentity> {
    bikes.iter().filter(|b| class_matches(&b.class, allowed)).collect()
}

/// Read a bike's identity from a loose folder (`<dir>/<name>.ini` + `.cfg`) or a
/// `.pkz` (entries matched by basename). Assumes the OEM convention that the ini/cfg
/// basenames equal the folder/pkz stem. Returns `None` when the path is not a bike
/// (neither config present) so non-bike folders don't surface; a bike missing only
/// one file still resolves, falling back to the stem for absent fields.
pub fn read_identity(path: &Path) -> Option<BikeIdentity> {
    let (stem, ini, cfg_bytes) = if path.is_dir() {
        let stem = path.file_name()?.to_string_lossy().into_owned();
        let ini = std::fs::read(path.join(format!("{stem}.ini"))).ok();
        let cfg = std::fs::read(path.join(format!("{stem}.cfg"))).ok();
        (stem, ini, cfg)
    } else {
        let stem = path.file_stem()?.to_string_lossy().into_owned();
        let ini = pkz::read_entry(path, &format!("{stem}.ini")).ok().flatten();
        let cfg = pkz::read_entry(path, &format!("{stem}.cfg")).ok().flatten();
        (stem, ini, cfg)
    };

    // Not a bike if neither config file is present.
    if ini.is_none() && cfg_bytes.is_none() {
        return None;
    }

    let class = ini.as_deref().map(class_from_ini).unwrap_or_default();
    let name = ini
        .as_deref()
        .map(name_from_ini)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| stem.clone());
    let id = cfg_bytes
        .as_deref()
        .and_then(id_from_cfg)
        .unwrap_or_else(|| stem.clone());

    Some(BikeIdentity {
        id,
        name,
        class,
        path: path.to_string_lossy().into_owned(),
    })
}

/// Scan installed bikes under `<mods_path>/mods/bikes` — each a loose folder or a
/// `.pkz` — returning their identities sorted by display name. Non-bike entries are
/// skipped. Reuses the same `mods/bikes` location as the library scanner.
pub fn scan_installed_bikes(mods_path: &str) -> Vec<BikeIdentity> {
    let dir = crate::library::mods_subdir(mods_path, "mods/bikes");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let is_candidate = p.is_dir()
                || p.extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("pkz"));
            if is_candidate {
                if let Some(b) = read_identity(&p) {
                    out.push(b);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_bike(dir: &std::path::Path, name: &str, cat: &str) {
        let bd = dir.join(name);
        std::fs::create_dir_all(&bd).unwrap();
        std::fs::write(
            bd.join(format!("{name}.ini")),
            format!("[info]\nname = {name}\n[data]\ncat = {cat}\n"),
        )
        .unwrap();
        std::fs::write(
            bd.join(format!("{name}.cfg")),
            format!("type = bike\nID = {name}\n"),
        )
        .unwrap();
    }

    // Trimmed from a real OEM bike (MX1OEM_1996_Honda_CR250).
    const CR250_INI: &[u8] = b"\
[info]
name = Honda CR250 1996
short_name = OEM CR250 '96
engine_type = 2 stroke

[paint]
default = 96STOCK

[data]
code = 0
cat = Classic MX1 OEM

[race]
length = 30
";

    const CR250_CFG: &[u8] = b"\
type = bike
ID = MX1OEM_1996_Honda_CR250
geom_id = 96cr250

engine
{
    mapping0
    {
        id = 1996_cr250
        name = CR
    }
}
";

    #[test]
    fn reads_class_name_and_id_from_configs() {
        assert_eq!(class_from_ini(CR250_INI), "Classic MX1 OEM");
        assert_eq!(name_from_ini(CR250_INI), "Honda CR250 1996");
        // The nested engine-mapping `id`/`name` must not shadow the top-level ID.
        assert_eq!(id_from_cfg(CR250_CFG).as_deref(), Some("MX1OEM_1996_Honda_CR250"));
    }

    #[test]
    fn missing_fields_are_empty_or_none() {
        assert_eq!(class_from_ini(b"[info]\nname = x\n"), "");
        assert_eq!(id_from_cfg(b"type = bike\n"), None);
    }

    #[test]
    fn class_eq_is_trimmed_and_case_insensitive() {
        assert!(class_eq(" Classic MX1 OEM ", "classic mx1 oem"));
        assert!(!class_eq("MX1 OEM", "MX2 OEM"));
    }

    #[test]
    fn class_matches_open_single_and_multi() {
        // Empty spec = Open → anything passes.
        assert!(class_matches("Classic MX1 OEM", ""));
        assert!(class_matches("", "   "));
        // Single category.
        assert!(class_matches("MX2 OEM", "MX2 OEM"));
        assert!(!class_matches("MX1 OEM", "MX2 OEM"));
        // Slash-separated list (server multi-category event).
        assert!(class_matches("MX2 OEM", "MX1 OEM/MX2 OEM"));
        assert!(!class_matches("125", "MX1 OEM/MX2 OEM"));
    }

    #[test]
    fn scan_reads_identities_skips_non_bikes_and_filters_by_class() {
        let root = std::env::temp_dir().join(format!("frost-bikeswap-{}", std::process::id()));
        let bikes = root.join("mods/bikes");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&bikes).unwrap();
        tmp_bike(&bikes, "MX1_A", "MX1 OEM");
        tmp_bike(&bikes, "MX2_B", "MX2 OEM");
        tmp_bike(&bikes, "MX1_C", "MX1 OEM");
        // A non-bike folder (no .ini/.cfg) must be skipped.
        std::fs::create_dir_all(bikes.join("_screenshots")).unwrap();

        let all = scan_installed_bikes(&root.to_string_lossy());
        let names: Vec<&str> = all.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(names, vec!["MX1_A", "MX1_C", "MX2_B"], "sorted, non-bike skipped");

        let mx1: Vec<&str> = bikes_in_class(&all, "MX1 OEM")
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(mx1, vec!["MX1_A", "MX1_C"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bikes_in_class_filters_and_preserves_order() {
        let mk = |id: &str, class: &str| BikeIdentity {
            id: id.into(),
            name: id.into(),
            class: class.into(),
            path: String::new(),
        };
        let bikes = vec![
            mk("a", "MX1 OEM"),
            mk("b", "MX2 OEM"),
            mk("c", "MX1 OEM"),
        ];
        let got: Vec<&str> = bikes_in_class(&bikes, "MX1 OEM")
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(got, vec!["a", "c"]);
    }
}
