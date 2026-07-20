use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Model-swap variant; not a `profile.ini` value — a filesystem swap at apply time. Empty = leave current model.
    pub model_swap: String,
}

impl Loadout {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleRef {
    pub url: String,
    pub host: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub loadout: Loadout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<BundleRef>,
}

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

    fn header_name(line: &str) -> Option<&str> {
        let t = line.trim();
        if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
            Some(t[1..t.len() - 1].trim())
        } else {
            None
        }
    }

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

fn profile_ini_path(profiles_dir: &Path, profile: &str) -> PathBuf {
    profiles_dir.join(profile).join("profile.ini")
}

pub fn list_profiles(profiles_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(profiles_dir) {
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

pub fn list_bikes(profiles_dir: &Path, profile: &str) -> anyhow::Result<Vec<String>> {
    let path = profile_ini_path(profiles_dir, profile);
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

pub fn read_loadout(profiles_dir: &Path, profile: &str, bikeid: &str) -> anyhow::Result<Loadout> {
    let path = profile_ini_path(profiles_dir, profile);
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

pub fn apply_loadout(
    profiles_dir: &Path,
    profile: &str,
    bikeid: &str,
    loadout: &Loadout,
    make_active: bool,
) -> anyhow::Result<()> {
    let path = profile_ini_path(profiles_dir, profile);
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

fn store_path(dir: &Path) -> PathBuf {
    dir.join("presets.json")
}

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

pub fn save_preset(dir: &Path, mut preset: Preset) -> anyhow::Result<()> {
    preset.bundle = None;
    let mut all = load_presets(dir);
    all.retain(|p| !p.name.eq_ignore_ascii_case(&preset.name));
    all.push(preset);
    all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    write_presets(dir, &all)
}

pub fn delete_preset(dir: &Path, name: &str) -> anyhow::Result<()> {
    let mut all = load_presets(dir);
    all.retain(|p| !p.name.eq_ignore_ascii_case(name));
    write_presets(dir, &all)
}

pub fn find_preset(dir: &Path, name: &str) -> Option<Preset> {
    load_presets(dir)
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

const CODE_PREFIX: &str = "MXBP1-";

pub fn export_code(dir: &Path, name: &str) -> anyhow::Result<String> {
    let preset = find_preset(dir, name)
        .ok_or_else(|| anyhow::anyhow!("no preset named '{name}'"))?;
    Ok(encode_code(&preset))
}

fn encode_code(preset: &Preset) -> String {
    let json = serde_json::to_vec(preset).unwrap_or_default();
    format!("{CODE_PREFIX}{}", STANDARD.encode(json))
}

pub fn encode_code_public(preset: &Preset) -> String {
    encode_code(preset)
}

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
        let profiles = root.join("profiles");
        assert_eq!(list_profiles(&profiles), vec!["main"]);
        let bikes = list_bikes(&profiles, "main").unwrap();
        assert_eq!(bikes, vec!["KTM250", "YZ450F"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_bike_column() {
        let root = tmp("read");
        write_sample(&root, "main");
        let lo = read_loadout(&root.join("profiles"), "main", "YZ450F").unwrap();
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
        let profiles = root.join("profiles");

        let mut lo = Loadout::default();
        lo.paint = "SnowWhite".into();
        lo.helmet = "Shoei".into();
        lo.race_number = "7".into();
        apply_loadout(&profiles, "main", "KTM250", &lo, true).unwrap();

        let after = read_loadout(&profiles, "main", "KTM250").unwrap();
        assert_eq!(after.paint, "SnowWhite");
        assert_eq!(after.helmet, "Shoei");

        // The other bike's row is untouched.
        let other = read_loadout(&profiles, "main", "YZ450F").unwrap();
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
            bundle: None,
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
