use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// `profile.ini` sections that are *not* cosmetic slots and must never be treated as
/// one. `[info]` holds the active bike and race number, which have their own handling.
pub const NON_SLOT_SECTIONS: [&str; 1] = ["info"];

/// Sections the app has a named [`Loadout`] field for.
///
/// A superset across titles, not one game's list: MX Bikes uses all fifteen, GP Bikes
/// uses a subset (and adds sections of its own, which land in [`Loadout::extra`]).
/// Nothing is ever *written* to a section merely because it appears here — see
/// [`apply_loadout`].
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
    /// Slots read from the file that have no named field above — GP Bikes' own sections,
    /// and anything a future game patch adds.
    ///
    /// Skipped when empty so an MX Bikes preset serializes byte-identically to one saved
    /// before this existed, which is what keeps old share codes valid.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
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
            other => self.extra.get(other).map(String::as_str)?,
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
            other => {
                self.extra.insert(other.to_string(), val);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleRef {
    pub url: String,
    pub host: String,
    pub size: u64,
    /// Every slice of the bundle, in order, when it was too big for one upload. Empty means
    /// the whole thing is at `url` — which is also the first slice, so old codes and
    /// single-part codes stay byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
}

/// The content a preset needs on disk, beyond the cosmetics in its [`Loadout`].
///
/// A loadout says how the bike is *dressed*; this says what has to be *installed* for the
/// session it belongs to — the track it's ridden on, plus whatever the player pins as
/// always-needed (the OEM pack and friends). It's what lets Manage take everything else
/// out of the game's way before a race.
///
/// Paths are relative to the MX Bikes root, forward-slashed (`mods/tracks/EU/RedBud.pkz`),
/// so a preset survives the mods folder moving or being shared with another player.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresetContent {
    pub tracks: Vec<String>,
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub loadout: Loadout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<BundleRef>,
    /// Race content. Absent on every preset saved before Manage existed, and on presets
    /// that are only ever used to dress a bike — `None` and an empty one mean the same
    /// thing, but skipping it keeps old share codes byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<PresetContent>,
}

/// A `[section] key=value` file edited in place, line by line.
///
/// Rewriting one key must not disturb anything else in the file: `profile.ini` carries slots
/// this app has no name for, and `ReShade.ini` carries the player's keybinds and overlay
/// settings. Keeping the original lines and touching only the one that matches is what makes
/// that true by construction — see [`crate::reshade`], the other user of this.
pub(crate) struct IniDoc {
    lines: Vec<String>,
    crlf: bool,
}

impl IniDoc {
    pub(crate) fn parse(text: &str) -> Self {
        let crlf = text.contains("\r\n");
        let lines = text
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        IniDoc { lines, crlf }
    }

    pub(crate) fn render(&self) -> String {
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

    pub(crate) fn get(&self, section: &str, key: &str) -> Option<String> {
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

    pub(crate) fn set(&mut self, section: &str, key: &str, value: &str) {
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

    /// Every section header in the file, in the order they appear.
    ///
    /// This is what lets the app read a `profile.ini` it has no hardcoded knowledge of.
    /// GP Bikes' slot set is not MX Bikes' — no `goggles_paint`, no `boots`, no
    /// `protection`, since it bakes those into the rider model — so rather than ship a
    /// guessed list per title, the file is asked what it has.
    fn sections(&self) -> Vec<String> {
        self.lines.iter().filter_map(|l| Self::header_name(l).map(str::to_string)).collect()
    }

    fn has_section(&self, section: &str) -> bool {
        self.section_span(section).is_some()
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

/// Decode a `profile.ini` the game wrote. MX Bikes uses Windows-1252/Latin-1, so
/// the bytes are not always valid UTF-8 (`read_to_string` fails on them). Returns
/// the text plus whether it was valid UTF-8, so a write can round-trip the
/// original single-byte encoding instead of silently converting it.
fn decode_ini(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), true),
        // Latin-1 is a lossless byte<->char map we reverse in `encode_ini`.
        Err(_) => (bytes.iter().map(|&b| b as char).collect(), false),
    }
}

/// Re-encode INI text for writing, reversing [`decode_ini`] so a Latin-1 file is
/// written back byte-for-byte rather than upgraded to UTF-8. Edited values are
/// ASCII, so every char is <= U+00FF when the source was Latin-1.
fn encode_ini(text: &str, was_utf8: bool) -> Vec<u8> {
    if was_utf8 {
        text.as_bytes().to_vec()
    } else {
        text.chars().map(|c| c as u32 as u8).collect()
    }
}

fn read_profile_ini(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(decode_ini(&bytes).0)
}

/// The profiles folder as the app sees it: where it looked, whether that folder is
/// even there, and what it found.
///
/// `exists` is the point of this type. A missing folder and a folder holding zero
/// profiles both used to surface as an empty list, so the UI could only ever say
/// "no profiles" — never "that folder isn't there", which is the far more common
/// cause and the only one the player can act on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesScan {
    pub dir: String,
    pub exists: bool,
    pub profiles: Vec<String>,
}

pub fn scan_profiles(profiles_dir: &Path) -> ProfilesScan {
    let mut profiles = Vec::new();
    let exists = match fs::read_dir(profiles_dir) {
        Ok(rd) => {
            for e in rd.flatten() {
                if e.path().is_dir() && e.path().join("profile.ini").is_file() {
                    if let Some(n) = e.file_name().to_str() {
                        profiles.push(n.to_string());
                    }
                }
            }
            true
        }
        // Unreadable counts as missing: either way there's nothing to list, and the
        // folder can't be used as-is.
        Err(_) => false,
    };
    profiles.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    ProfilesScan {
        dir: profiles_dir.to_string_lossy().into_owned(),
        exists,
        profiles,
    }
}

pub fn list_profiles(profiles_dir: &Path) -> Vec<String> {
    scan_profiles(profiles_dir).profiles
}

pub fn list_bikes(profiles_dir: &Path, profile: &str) -> anyhow::Result<Vec<String>> {
    let path = profile_ini_path(profiles_dir, profile);
    let text = read_profile_ini(&path)?;
    Ok(bikes_in(&IniDoc::parse(&text)))
}

/// The bikes a profile carries, from whichever section is keyed by bike id.
fn bikes_in(doc: &IniDoc) -> Vec<String> {
    // `[rider]` is the cleanest bikeid-keyed section; fall back to `[paint]`.
    let mut bikes = doc.section_keys("rider");
    if bikes.is_empty() {
        bikes = doc.section_keys("paint");
    }
    bikes.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    bikes.dedup();
    bikes
}

pub fn read_loadout(profiles_dir: &Path, profile: &str, bikeid: &str) -> anyhow::Result<Loadout> {
    let path = profile_ini_path(profiles_dir, profile);
    let text = read_profile_ini(&path)?;
    Ok(loadout_in(&IniDoc::parse(&text), bikeid))
}

/// Every bike's loadout, from a single read and parse.
///
/// [`read_loadout`] re-reads and re-parses the whole `profile.ini` per call, which is the
/// right shape when the UI is asking about one bike. Publishing a rider's look asks about
/// all of them — a profile holds a column per bike they have ever sat on — and doing that
/// one bike at a time is the same file parsed a dozen times over.
///
/// Returned in `list_bikes` order so the caller's own ordering decisions are stable.
pub fn read_all_loadouts(
    profiles_dir: &Path,
    profile: &str,
) -> anyhow::Result<Vec<(String, Loadout)>> {
    let path = profile_ini_path(profiles_dir, profile);
    let text = read_profile_ini(&path)?;
    let doc = IniDoc::parse(&text);
    Ok(bikes_in(&doc)
        .into_iter()
        .map(|bike| {
            let lo = loadout_in(&doc, &bike);
            (bike, lo)
        })
        .collect())
}

/// The bike the game will start on — `[info] bikeid`.
///
/// Profile-global rather than per-bike, and the only thing in the file that says which of
/// those columns is the one the rider is actually using.
pub fn active_bike(profiles_dir: &Path, profile: &str) -> Option<String> {
    let text = read_profile_ini(&profile_ini_path(profiles_dir, profile)).ok()?;
    IniDoc::parse(&text)
        .get("info", "bikeid")
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
}

/// Put the rider back on a bike, without touching what any bike is wearing.
///
/// The narrow counterpart to [`apply_loadout`]'s `make_active`: undoing a join that switched
/// bikes to clear a server's gate has to restore `[info] bikeid` alone, because the loadout
/// being restored belongs to the bike that was *written*, which is a different one.
pub fn set_active_bike(profiles_dir: &Path, profile: &str, bikeid: &str) -> anyhow::Result<()> {
    let path = profile_ini_path(profiles_dir, profile);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let (text, was_utf8) = decode_ini(&bytes);
    let mut doc = IniDoc::parse(&text);
    doc.set("info", "bikeid", bikeid);
    fs::write(&path, encode_ini(&doc.render(), was_utf8))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn loadout_in(doc: &IniDoc, bikeid: &str) -> Loadout {
    let mut lo = Loadout::default();
    for section in SLOT_SECTIONS {
        lo.set_slot(section, doc.get(section, bikeid).unwrap_or_default());
    }
    // Anything else the file carries is a slot too — this is how GP Bikes' sections get
    // read without the app having a hardcoded list for it. `set_slot` routes these into
    // `extra`, since by definition they have no named field.
    for section in doc.sections() {
        if is_slot_section(&section) && !SLOT_SECTIONS.contains(&section.as_str()) {
            lo.set_slot(&section, doc.get(&section, bikeid).unwrap_or_default());
        }
    }
    lo.race_number = doc.get("info", "race_number").unwrap_or_default();
    lo
}

/// Whether a `profile.ini` section is a cosmetic slot rather than bookkeeping.
fn is_slot_section(section: &str) -> bool {
    !NON_SLOT_SECTIONS.iter().any(|s| s.eq_ignore_ascii_case(section))
}

/// The slots a profile actually has, in file order. Lets the UI show the pickers this
/// game offers instead of a fixed MX Bikes list with dead rows in it.
pub fn slots_for(profiles_dir: &Path, profile: &str) -> anyhow::Result<Vec<String>> {
    let text = read_profile_ini(&profile_ini_path(profiles_dir, profile))?;
    Ok(IniDoc::parse(&text)
        .sections()
        .into_iter()
        .filter(|s| is_slot_section(s))
        .collect())
}

pub fn apply_loadout(
    profiles_dir: &Path,
    profile: &str,
    bikeid: &str,
    loadout: &Loadout,
    make_active: bool,
) -> anyhow::Result<()> {
    let path = profile_ini_path(profiles_dir, profile);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

    // Roll a backup of the pre-change file (overwrite each apply → "undo last").
    // Back up the raw bytes so the backup stays byte-identical to the original.
    let bak = PathBuf::from(format!("{}.bak", path.display()));
    let _ = fs::write(&bak, &bytes);

    let (text, was_utf8) = decode_ini(&bytes);
    let mut doc = IniDoc::parse(&text);
    // Write a slot only when the file already has that section, or when there's a real
    // value to record.
    //
    // Without the first condition, applying an MX-Bikes-shaped loadout to a GP Bikes
    // profile would invent `[goggles_paint]`, `[boots]` and `[protection]` sections that
    // game has no concept of. MX Bikes' own `profile.ini` carries every section it uses,
    // so nothing about applying to one changes.
    let sections: Vec<String> = SLOT_SECTIONS
        .iter()
        .map(|s| s.to_string())
        .chain(loadout.extra.keys().cloned())
        .collect();
    for section in sections {
        if let Some(val) = loadout.slot(&section) {
            if doc.has_section(&section) || !val.is_empty() {
                doc.set(&section, bikeid, val);
            }
        }
    }
    if make_active {
        doc.set("info", "bikeid", bikeid);
        if !loadout.race_number.trim().is_empty() {
            doc.set("info", "race_number", &loadout.race_number);
        }
    }

    fs::write(&path, encode_ini(&doc.render(), was_utf8))
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

    /// A GP Bikes `profile.ini`: the sections it shares with MX Bikes and — critically —
    /// no `goggles_paint`, `boots` or `protection`, which GP bakes into the rider model.
    ///
    /// `riding_style` is one of the shared ones. Both executables carry the same
    /// `%sprofiles\%s\profile.ini` / `riding_style` pair beside their `ID_CURRIDINGSTYLE`
    /// control, and both load the style itself from `rider\animations\<name>\<name>.ini`.
    /// (`animation` does appear in both binaries, but in their `usage.ini` cluster — it is
    /// the content-type label written into mod-usage bookkeeping, not a profile section.)
    const GP_SAMPLE: &str = "\
[info]
bikeid=BSB23_Ducati_V4R
race_number=46

[paint]
BSB23_Ducati_V4R=Team

[rider]
BSB23_Ducati_V4R=(S) Suit 1 + Boots Alpinestars

[helmet]
BSB23_Ducati_V4R=AGV Pista GP RR

[helmet_paint]
BSB23_Ducati_V4R=Mugello

[suit_paint]
BSB23_Ducati_V4R=Team

[riding_style]
BSB23_Ducati_V4R=Elbow Down

[tyres]
BSB23_Ducati_V4R=BS_Racing_Battlax
";

    fn write_ini(dir: &Path, profile: &str, text: &str) -> PathBuf {
        let p = dir.join("profiles").join(profile);
        fs::create_dir_all(&p).unwrap();
        let ini = p.join("profile.ini");
        fs::write(&ini, text).unwrap();
        ini
    }

    fn write_gp_sample(dir: &Path, profile: &str) -> PathBuf {
        write_ini(dir, profile, GP_SAMPLE)
    }

    /// The slot list comes from the file, so a game the app has no hardcoded list for
    /// still reports the right pickers — and never reports `[info]`, which is bookkeeping.
    #[test]
    fn slots_come_from_the_file_not_a_hardcoded_list() {
        let root = tmp("gp-slots");
        write_gp_sample(&root, "rider1");
        let slots = slots_for(&root.join("profiles"), "rider1").unwrap();

        assert!(slots.contains(&"riding_style".to_string()), "shared slot: {slots:?}");
        assert!(slots.contains(&"helmet_paint".to_string()));
        assert!(!slots.contains(&"info".to_string()), "[info] is not a slot");
        // The proof the list is the file's and not MX Bikes': these three are in
        // `SLOT_SECTIONS` but not in this profile, so a hardcoded list would offer them.
        for mx_only in ["goggles_paint", "boots", "protection"] {
            assert!(!slots.contains(&mx_only.to_string()), "GP has no {mx_only}: {slots:?}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// A section with no named `Loadout` field still round-trips: read into `extra`,
    /// written back to the same place.
    ///
    /// `visor_tint` is invented — no shipped title writes it. That is the point: this is the
    /// forward-compatibility path for a section a *future* game patch adds, which by
    /// definition can't be named here in advance. Every section either title writes today
    /// has a `Loadout` field, so only a made-up one exercises `extra`.
    #[test]
    fn an_unknown_slot_round_trips_through_extra() {
        let root = tmp("gp-extra");
        let text = format!("{GP_SAMPLE}\n[visor_tint]\nBSB23_Ducati_V4R=Smoke\n");
        let ini = write_ini(&root, "rider1", &text);
        let profiles = root.join("profiles");

        let mut lo = read_loadout(&profiles, "rider1", "BSB23_Ducati_V4R").unwrap();
        assert_eq!(lo.extra.get("visor_tint").map(String::as_str), Some("Smoke"));
        assert_eq!(lo.helmet, "AGV Pista GP RR", "shared slots still use their fields");
        assert_eq!(lo.riding_style, "Elbow Down", "riding_style is a named field, not extra");

        lo.set_slot("visor_tint", "Clear".into());
        apply_loadout(&profiles, "rider1", "BSB23_Ducati_V4R", &lo, false).unwrap();

        let text = fs::read_to_string(&ini).unwrap();
        assert!(text.contains("BSB23_Ducati_V4R=Clear"), "visor_tint was written: {text}");
        let back = read_loadout(&profiles, "rider1", "BSB23_Ducati_V4R").unwrap();
        assert_eq!(back.extra.get("visor_tint").map(String::as_str), Some("Clear"));
        let _ = fs::remove_dir_all(&root);
    }

    /// The reason `apply_loadout` checks `has_section`: an MX-Bikes-shaped loadout must
    /// not invent gear sections in a GP Bikes profile, or the file grows slots that
    /// game will never read and the app would then offer as if they were real.
    #[test]
    fn applying_does_not_invent_sections_the_game_lacks() {
        let root = tmp("gp-no-invent");
        let ini = write_gp_sample(&root, "rider1");
        let profiles = root.join("profiles");

        let lo = read_loadout(&profiles, "rider1", "BSB23_Ducati_V4R").unwrap();
        apply_loadout(&profiles, "rider1", "BSB23_Ducati_V4R", &lo, false).unwrap();

        let text = fs::read_to_string(&ini).unwrap();
        for absent in ["[goggles_paint]", "[boots]", "[protection]", "[gloves_paint]"] {
            assert!(!text.contains(absent), "{absent} must not appear:\n{text}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// An MX Bikes preset must serialize exactly as it did before `extra` existed, or
    /// every share code already in circulation changes.
    #[test]
    fn an_mx_bikes_preset_serializes_without_the_extra_field() {
        let preset = Preset {
            name: "Race day".into(),
            loadout: Loadout {
                paint: "RedBud".into(),
                ..Default::default()
            },
            bundle: None,
            content: None,
        };
        let json = serde_json::to_string(&preset).unwrap();
        assert!(!json.contains("extra"), "empty extra is skipped: {json}");
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

    /// A folder that isn't there and a folder with nothing in it both list zero
    /// profiles — the scan has to tell them apart, since only the first one means
    /// "you're pointed at the wrong place".
    #[test]
    fn scan_tells_a_missing_folder_from_an_empty_one() {
        let root = tmp("scan");

        let missing = root.join("not-here");
        let scan = scan_profiles(&missing);
        assert!(!scan.exists);
        assert!(scan.profiles.is_empty());
        assert_eq!(scan.dir, missing.to_string_lossy());

        let empty = root.join("profiles");
        fs::create_dir_all(&empty).unwrap();
        let scan = scan_profiles(&empty);
        assert!(scan.exists, "the folder is there, it just holds no profiles");
        assert!(scan.profiles.is_empty());

        // A subdir without a profile.ini isn't a profile, but the folder still exists.
        fs::create_dir_all(empty.join("junk")).unwrap();
        let scan = scan_profiles(&empty);
        assert!(scan.exists);
        assert!(scan.profiles.is_empty());

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

    // Paint sync publishes every bike a profile holds, and reading them one at a time
    // re-parses the same file per bike. What matters is that the batch answers identically.
    #[test]
    fn reading_every_bike_at_once_matches_reading_them_one_by_one() {
        let root = tmp("read-all");
        write_sample(&root, "main");
        let profiles = root.join("profiles");

        let all = read_all_loadouts(&profiles, "main").unwrap();
        assert_eq!(
            all.iter().map(|(b, _)| b.clone()).collect::<Vec<_>>(),
            list_bikes(&profiles, "main").unwrap(),
            "same bikes, same order"
        );
        for (bike, lo) in &all {
            assert_eq!(*lo, read_loadout(&profiles, "main", bike).unwrap(), "{bike}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    // `[info] bikeid` is the only thing in the file saying which of those columns the rider
    // is actually using — it decides which bike survives the publish cap.
    #[test]
    fn the_active_bike_is_the_one_the_game_will_start_on() {
        let root = tmp("active");
        write_sample(&root, "main");
        assert_eq!(active_bike(&root.join("profiles"), "main").as_deref(), Some("YZ450F"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_profile_means_no_active_bike_rather_than_a_panic() {
        let root = tmp("active-missing");
        assert_eq!(active_bike(&root.join("profiles"), "nobody"), None);
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
    fn reads_and_round_trips_latin1_profile() {
        let root = tmp("latin1");
        let p = root.join("profiles").join("main");
        fs::create_dir_all(&p).unwrap();
        let ini = p.join("profile.ini");
        // 0xE9 is 'é' in Windows-1252/Latin-1 but an invalid UTF-8 lead byte,
        // which is exactly what made `read_to_string` fail in the presets tab.
        let mut bytes = SAMPLE.as_bytes().to_vec();
        bytes.extend_from_slice(b"\n[info]\nrider_name=Andr\xE9\n");
        fs::write(&ini, &bytes).unwrap();
        let profiles = root.join("profiles");

        // Read no longer errors on the non-UTF-8 byte.
        assert_eq!(list_bikes(&profiles, "main").unwrap(), vec!["KTM250", "YZ450F"]);

        let mut lo = Loadout::default();
        lo.paint = "SnowWhite".into();
        apply_loadout(&profiles, "main", "KTM250", &lo, true).unwrap();

        // The Latin-1 byte survives write-back untouched (not mangled to U+FFFD).
        let after = fs::read(&ini).unwrap();
        assert!(after.windows(6).any(|w| w == b"Andr\xE9\n"));
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
            content: Some(PresetContent {
                tracks: vec!["mods/tracks/RedBud.pkz".into()],
                keep: vec!["mods/bikes/OEM Pack.pkz".into()],
            }),
        };
        let code = encode_code(&preset);
        assert!(code.starts_with(CODE_PREFIX));
        let back = decode_code(&code).unwrap();
        assert_eq!(back.name, "RedBud #92");
        assert_eq!(back.loadout.helmet_paint, "CLUTCH Deeg F REDB");
        assert_eq!(back.content.unwrap().tracks, vec!["mods/tracks/RedBud.pkz"]);
        let _ = round_trip_raw_json(&preset);
    }

    /// Presets saved before Manage existed have no `content` key at all. They have to keep
    /// loading — both from `presets.json` and from a share code someone posted last month.
    #[test]
    fn a_preset_without_content_still_loads() {
        let json = r#"{"name":"Old","loadout":{"paint":"RedBud"}}"#;
        let back = decode_code(json).unwrap();
        assert_eq!(back.name, "Old");
        assert!(back.content.is_none());
        // And it round-trips back out without growing a `content` key.
        let out = serde_json::to_string(&back).unwrap();
        assert!(!out.contains("content"), "{out}");
    }

    /// Codes shared before bundles could be sliced carry no `parts` key, and a one-part
    /// bundle must not grow one — an old app reading a new single-part code has to see
    /// exactly what it saw before.
    #[test]
    fn a_bundle_without_parts_still_loads() {
        let json = r#"{"name":"Old","loadout":{},"bundle":{"url":"https://x/a.zip","host":"catbox","size":10}}"#;
        let back = decode_code(json).unwrap();
        let bundle = back.bundle.clone().unwrap();
        assert!(bundle.parts.is_empty());

        let out = serde_json::to_string(&back).unwrap();
        assert!(!out.contains("parts"), "{out}");
    }

    #[test]
    fn a_sliced_bundle_carries_its_parts_in_order() {
        let json = r#"{"name":"Big","loadout":{},"bundle":{"url":"https://x/1.zip","host":"catbox","size":300,"parts":["https://x/1.zip","https://x/2.zip"]}}"#;
        let back = decode_code(json).unwrap();
        let bundle = back.bundle.clone().unwrap();
        assert_eq!(bundle.parts, vec!["https://x/1.zip", "https://x/2.zip"]);
        // The first slice is also the plain `url`, so nothing has to look in two places.
        assert_eq!(bundle.url, bundle.parts[0]);

        let back = decode_code(&encode_code(&back)).unwrap();
        assert_eq!(back.bundle.unwrap().parts.len(), 2);
    }

    fn round_trip_raw_json(preset: &Preset) -> anyhow::Result<()> {
        let json = serde_json::to_string(preset)?;
        let back = decode_code(&json)?;
        assert_eq!(back.name, preset.name);
        Ok(())
    }
}
