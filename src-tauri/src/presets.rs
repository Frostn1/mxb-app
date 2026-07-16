//! Customization **presets** (per-bike loadouts) for MX Bikes.
//!
//! MX Bikes stores the current cosmetic selection in a single per-profile INI at
//! `<MX Bikes>/profiles/<profile>/profile.ini`. The file is organized as **one
//! section per customization slot** (`[paint]`, `[helmet]`, `[helmet_paint]`, …),
//! and inside each section every line is `<bikeid>=<selected value>` — so the game
//! keeps a *separate* full look for each bike. `[info]` holds the currently active
//! selection (`bikeid=`, `race_number=`, …); on launch the game reads
//! `[info].bikeid` and pulls that bike's row out of every slot section.
//!
//! A **preset** here is a bike-agnostic bundle of all slot values (a "look"). You
//! build it (capture a bike's current column, or pick each slot from installed
//! mods), save it, then **apply** it to a chosen bike — which writes that bike's
//! row across all slot sections. Values are plain string references to installed
//! mod/paint folders (empty = stock/none), so a recipient of a shared preset needs
//! the referenced mods installed for it to show.
//!
//! Editing is line-oriented on purpose: values contain spaces, `#`, `()` and other
//! characters a generic INI writer would quote or mangle, so we only ever rewrite
//! the exact `<bikeid>=` lines we target and leave every other byte untouched.

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The slot sections that make up a full look, in display order. Each is keyed by
/// bikeid inside `profile.ini`. `race_number` is handled separately (it's a scalar
/// in `[info]`, not per-bike).
pub const SLOT_SECTIONS: [&str; 15] = [
    "paint",
    "bike_font",
    "rider",
    "helmet",
    "helmet_paint",
    "goggles_paint",
    "suit_paint",
    "suit_font",
    "boots",
    "boots_paint",
    "gloves_paint",
    "protection",
    "protection_paint",
    "riding_style",
    "tyres",
];

/// A full cosmetic loadout — one value per slot (empty = stock/none). Field names
/// mirror the `profile.ini` section names; `race_number` is the active `[info]`
/// scalar, carried along so a preset can also set the rider number.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Loadout {
    pub paint: String,
    pub bike_font: String,
    pub rider: String,
    pub helmet: String,
    pub helmet_paint: String,
    pub goggles_paint: String,
    pub suit_paint: String,
    pub suit_font: String,
    pub boots: String,
    pub boots_paint: String,
    pub gloves_paint: String,
    pub protection: String,
    pub protection_paint: String,
    pub riding_style: String,
    pub tyres: String,
    pub race_number: String,
    /// Bike **model swap** variant to apply (Locker / `FrostMod Models/`). Not a
    /// `profile.ini` value — it's a filesystem swap handled at apply time. Empty =
    /// leave the bike's current model untouched.
    pub model_swap: String,
}

impl Loadout {
    /// Read the value for a slot section name.
    fn slot(&self, section: &str) -> Option<&str> {
        Some(match section {
            "paint" => &self.paint,
            "bike_font" => &self.bike_font,
            "rider" => &self.rider,
            "helmet" => &self.helmet,
            "helmet_paint" => &self.helmet_paint,
            "goggles_paint" => &self.goggles_paint,
            "suit_paint" => &self.suit_paint,
            "suit_font" => &self.suit_font,
            "boots" => &self.boots,
            "boots_paint" => &self.boots_paint,
            "gloves_paint" => &self.gloves_paint,
            "protection" => &self.protection,
            "protection_paint" => &self.protection_paint,
            "riding_style" => &self.riding_style,
            "tyres" => &self.tyres,
            _ => return None,
        })
    }

    /// Write the value for a slot section name.
    fn set_slot(&mut self, section: &str, val: String) {
        match section {
            "paint" => self.paint = val,
            "bike_font" => self.bike_font = val,
            "rider" => self.rider = val,
            "helmet" => self.helmet = val,
            "helmet_paint" => self.helmet_paint = val,
            "goggles_paint" => self.goggles_paint = val,
            "suit_paint" => self.suit_paint = val,
            "suit_font" => self.suit_font = val,
            "boots" => self.boots = val,
            "boots_paint" => self.boots_paint = val,
            "gloves_paint" => self.gloves_paint = val,
            "protection" => self.protection = val,
            "protection_paint" => self.protection_paint = val,
            "riding_style" => self.riding_style = val,
            "tyres" => self.tyres = val,
            _ => {}
        }
    }
}

/// A saved, named, bike-agnostic preset. Serialized to `presets.json` and to the
/// portable share code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub loadout: Loadout,
}

// --- profile.ini line editor ------------------------------------------------

/// A `profile.ini` held as raw lines so edits touch only the targeted `key=value`
/// lines and preserve everything else (order, blank lines, unknown sections, and
/// the exact spacing/characters of untouched values).
struct IniDoc {
    lines: Vec<String>,
    crlf: bool,
}

impl IniDoc {
    fn parse(text: &str) -> Self {
        let crlf = text.contains("\r\n");
        let lines = text
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        IniDoc { lines, crlf }
    }

    fn render(&self) -> String {
        let sep = if self.crlf { "\r\n" } else { "\n" };
        self.lines.join(sep)
    }

    /// The section name if `line` is a `[section]` header.
    fn header_name(line: &str) -> Option<&str> {
        let t = line.trim();
        if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
            Some(t[1..t.len() - 1].trim())
        } else {
            None
        }
    }

    /// `(header_index, body_end_exclusive)` for a section, or `None` if absent.
    /// The body is the lines after the header up to the next header (or EOF).
    fn section_span(&self, section: &str) -> Option<(usize, usize)> {
        let mut header = None;
        for (i, line) in self.lines.iter().enumerate() {
            if let Some(name) = Self::header_name(line) {
                if name.eq_ignore_ascii_case(section) {
                    header = Some(i);
                    break;
                }
            }
        }
        let h = header?;
        let mut end = self.lines.len();
        for (j, line) in self.lines.iter().enumerate().skip(h + 1) {
            if Self::header_name(line).is_some() {
                end = j;
                break;
            }
        }
        Some((h, end))
    }

    /// Value of `key` within `section` (raw, everything right of the first `=`).
    fn get(&self, section: &str, key: &str) -> Option<String> {
        let (h, end) = self.section_span(section)?;
        for line in &self.lines[h + 1..end] {
            if let Some(eq) = line.find('=') {
                if line[..eq].trim() == key {
                    return Some(line[eq + 1..].to_string());
                }
            }
        }
        None
    }

    /// Set `key=value` within `section`, updating the existing line, inserting a
    /// new one at the end of the section body, or creating the section if absent.
    fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some((h, end)) = self.section_span(section) {
            for idx in (h + 1)..end {
                if let Some(eq) = self.lines[idx].find('=') {
                    if self.lines[idx][..eq].trim() == key {
                        self.lines[idx] = format!("{key}={value}");
                        return;
                    }
                }
            }
            // Insert before any trailing blank lines that pad the section.
            let mut insert = end;
            while insert > h + 1 && self.lines[insert - 1].trim().is_empty() {
                insert -= 1;
            }
            self.lines.insert(insert, format!("{key}={value}"));
        } else {
            if self.lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                self.lines.push(String::new());
            }
            self.lines.push(format!("[{section}]"));
            self.lines.push(format!("{key}={value}"));
        }
    }

    /// All bikeid keys under a slot section, in file order (skips blank/valueless
    /// stray keys are kept — callers pick the canonical section).
    fn section_keys(&self, section: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some((h, end)) = self.section_span(section) {
            for line in &self.lines[h + 1..end] {
                if let Some(eq) = line.find('=') {
                    let k = line[..eq].trim();
                    if !k.is_empty() {
                        out.push(k.to_string());
                    }
                }
            }
        }
        out
    }
}

// --- profile discovery / read / apply ---------------------------------------

/// `<mods_path>/profiles/<profile>/profile.ini`.
fn profile_ini_path(mods_path: &str, profile: &str) -> PathBuf {
    PathBuf::from(mods_path)
        .join("profiles")
        .join(profile)
        .join("profile.ini")
}

/// Profile folder names (under `<MX Bikes>/profiles/`) that contain a
/// `profile.ini`, sorted case-insensitively.
pub fn list_profiles(mods_path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let base = PathBuf::from(mods_path).join("profiles");
    if let Ok(rd) = fs::read_dir(&base) {
        for e in rd.flatten() {
            if e.path().is_dir() && e.path().join("profile.ini").is_file() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    out
}

/// Bike ids present in a profile. Uses the `[rider]` section — its keys are always
/// bikeids (unlike `[protection_paint]`, which can carry stray model-name keys).
pub fn list_bikes(mods_path: &str, profile: &str) -> anyhow::Result<Vec<String>> {
    let path = profile_ini_path(mods_path, profile);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let doc = IniDoc::parse(&text);
    // `[rider]` is the cleanest bikeid-keyed section; fall back to `[paint]`.
    let mut bikes = doc.section_keys("rider");
    if bikes.is_empty() {
        bikes = doc.section_keys("paint");
    }
    bikes.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    bikes.dedup();
    Ok(bikes)
}

/// The current loadout column for one bike in a profile.
pub fn read_loadout(mods_path: &str, profile: &str, bikeid: &str) -> anyhow::Result<Loadout> {
    let path = profile_ini_path(mods_path, profile);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let doc = IniDoc::parse(&text);

    let mut lo = Loadout::default();
    for section in SLOT_SECTIONS {
        lo.set_slot(section, doc.get(section, bikeid).unwrap_or_default());
    }
    lo.race_number = doc.get("info", "race_number").unwrap_or_default();
    Ok(lo)
}

/// Write a loadout into a bike's row across every slot section. When `make_active`
/// is set, also point `[info].bikeid` (and `race_number`) at this bike so it's the
/// one the game loads next. A one-shot `profile.ini.bak` is written before the
/// first change so the previous state can be restored.
pub fn apply_loadout(
    mods_path: &str,
    profile: &str,
    bikeid: &str,
    loadout: &Loadout,
    make_active: bool,
) -> anyhow::Result<()> {
    let path = profile_ini_path(mods_path, profile);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;

    // Roll a backup of the pre-change file (overwrite each apply → "undo last").
    let bak = PathBuf::from(format!("{}.bak", path.display()));
    let _ = fs::write(&bak, &text);

    let mut doc = IniDoc::parse(&text);
    for section in SLOT_SECTIONS {
        if let Some(val) = loadout.slot(section) {
            doc.set(section, bikeid, val);
        }
    }
    if make_active {
        doc.set("info", "bikeid", bikeid);
        if !loadout.race_number.trim().is_empty() {
            doc.set("info", "race_number", &loadout.race_number);
        }
    }

    fs::write(&path, doc.render())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

// --- preset store (presets.json in app local data dir) ----------------------

fn store_path(dir: &Path) -> PathBuf {
    dir.join("presets.json")
}

/// Load all saved presets (empty if the store doesn't exist yet).
pub fn load_presets(dir: &Path) -> Vec<Preset> {
    match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn write_presets(dir: &Path, presets: &[Preset]) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(presets)?)?;
    Ok(())
}

/// Save a preset, replacing any existing one with the same name (case-insensitive).
pub fn save_preset(dir: &Path, preset: Preset) -> anyhow::Result<()> {
    let mut all = load_presets(dir);
    all.retain(|p| !p.name.eq_ignore_ascii_case(&preset.name));
    all.push(preset);
    all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    write_presets(dir, &all)
}

/// Delete a preset by name (case-insensitive). No error if it doesn't exist.
pub fn delete_preset(dir: &Path, name: &str) -> anyhow::Result<()> {
    let mut all = load_presets(dir);
    all.retain(|p| !p.name.eq_ignore_ascii_case(name));
    write_presets(dir, &all)
}

fn find_preset(dir: &Path, name: &str) -> Option<Preset> {
    load_presets(dir)
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

// --- sharing (portable codes) -----------------------------------------------

/// Prefix that tags a share code so an importer can recognize it (and so a pasted
/// code round-trips cleanly). `1` is the format version.
const CODE_PREFIX: &str = "MXBP1-";

/// Encode a preset as a portable one-line share code (`MXBP1-<base64 json>`).
pub fn export_code(dir: &Path, name: &str) -> anyhow::Result<String> {
    let preset = find_preset(dir, name)
        .ok_or_else(|| anyhow::anyhow!("no preset named '{name}'"))?;
    Ok(encode_code(&preset))
}

fn encode_code(preset: &Preset) -> String {
    let json = serde_json::to_vec(preset).unwrap_or_default();
    format!("{CODE_PREFIX}{}", STANDARD.encode(json))
}

/// Parse a share code (prefixed base64, bare base64, or raw JSON) into a preset.
pub fn decode_code(text: &str) -> anyhow::Result<Preset> {
    let t = text.trim();
    if let Some(b64) = t.strip_prefix(CODE_PREFIX) {
        let bytes = STANDARD
            .decode(b64.trim())
            .context("share code isn't valid (bad base64)")?;
        return serde_json::from_slice(&bytes).context("share code isn't a valid preset");
    }
    if t.starts_with('{') {
        return serde_json::from_str(t).context("that JSON isn't a valid preset");
    }
    let bytes = STANDARD
        .decode(t)
        .context("that doesn't look like a preset code")?;
    serde_json::from_slice(&bytes).context("share code isn't a valid preset")
}

/// Import a share code: decode it, save it (deduping the name so a re-import or a
/// clashing name doesn't overwrite silently — the caller passes a resolved name),
/// and return the stored preset.
pub fn import_code(dir: &Path, text: &str) -> anyhow::Result<Preset> {
    let preset = decode_code(text)?;
    save_preset(dir, preset.clone())?;
    Ok(preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[info]
bikeid=YZ450F
race_number=92

[paint]
YZ450F=RedBud
KTM250=

[helmet]
YZ450F=Fox
KTM250=default

[helmet_paint]
YZ450F=CLUTCH
KTM250=

[rider]
YZ450F=default_mx
KTM250=default_mx

[tyres]
YZ450F=
KTM250=p_mx
";

    fn write_sample(dir: &Path, profile: &str) -> PathBuf {
        let p = dir.join("profiles").join(profile);
        fs::create_dir_all(&p).unwrap();
        let ini = p.join("profile.ini");
        fs::write(&ini, SAMPLE).unwrap();
        ini
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-presets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lists_profiles_and_bikes() {
        let root = tmp("list");
        write_sample(&root, "main");
        let mp = root.to_str().unwrap();
        assert_eq!(list_profiles(mp), vec!["main"]);
        let bikes = list_bikes(mp, "main").unwrap();
        assert_eq!(bikes, vec!["KTM250", "YZ450F"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_bike_column() {
        let root = tmp("read");
        write_sample(&root, "main");
        let lo = read_loadout(root.to_str().unwrap(), "main", "YZ450F").unwrap();
        assert_eq!(lo.paint, "RedBud");
        assert_eq!(lo.helmet, "Fox");
        assert_eq!(lo.helmet_paint, "CLUTCH");
        assert_eq!(lo.tyres, "");
        assert_eq!(lo.race_number, "92");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn applies_loadout_only_to_target_bike_and_backs_up() {
        let root = tmp("apply");
        let ini = write_sample(&root, "main");
        let mp = root.to_str().unwrap();

        let mut lo = Loadout::default();
        lo.paint = "SnowWhite".into();
        lo.helmet = "Shoei".into();
        lo.race_number = "7".into();
        apply_loadout(mp, "main", "KTM250", &lo, true).unwrap();

        let after = read_loadout(mp, "main", "KTM250").unwrap();
        assert_eq!(after.paint, "SnowWhite");
        assert_eq!(after.helmet, "Shoei");

        // The other bike's row is untouched.
        let other = read_loadout(mp, "main", "YZ450F").unwrap();
        assert_eq!(other.paint, "RedBud");
        assert_eq!(other.helmet, "Fox");

        // [info] now points at the applied bike, and a backup exists.
        let doc = IniDoc::parse(&fs::read_to_string(&ini).unwrap());
        assert_eq!(doc.get("info", "bikeid").as_deref(), Some("KTM250"));
        assert_eq!(doc.get("info", "race_number").as_deref(), Some("7"));
        assert!(root.join("profiles/main/profile.ini.bak").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn share_code_round_trips() {
        let preset = Preset {
            name: "RedBud #92".into(),
            loadout: {
                let mut l = Loadout::default();
                l.paint = "RedBud".into();
                l.helmet_paint = "CLUTCH Deeg F REDB".into();
                l
            },
        };
        let code = encode_code(&preset);
        assert!(code.starts_with(CODE_PREFIX));
        let back = decode_code(&code).unwrap();
        assert_eq!(back.name, "RedBud #92");
        assert_eq!(back.loadout.helmet_paint, "CLUTCH Deeg F REDB");
        let _ = round_trip_raw_json(&preset);
    }

    fn round_trip_raw_json(preset: &Preset) -> anyhow::Result<()> {
        let json = serde_json::to_string(preset)?;
        let back = decode_code(&json)?;
        assert_eq!(back.name, preset.name);
        Ok(())
    }
}
