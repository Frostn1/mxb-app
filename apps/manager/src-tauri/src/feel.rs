//! Riding-feel presets: the settings half of a profile, saved and re-applied by name.
//!
//! A rider does not want one set of controls. Supercross wants a soft throttle and less
//! direct lean; outdoor motocross wants the opposite, and a different draw distance while
//! it's at it. The game has one slot for all of that, so switching disciplines means
//! walking the Options screens twice a night.
//!
//! The settings live in two files the game rewrites together when Options is closed:
//!
//! * `profiles/<name>/profile.ini` — `[input]`, `[aids]`, `[view]`, `[ext_view]`, `[gfx]`,
//!   alongside the cosmetic slots [`crate::presets`] already owns.
//! * `profiles/<name>/controls.txt` — every control's binding *and* its feel: gain,
//!   deadzone, linearity and the smoothing that makes a throttle snappy or soft.
//!
//! Both section lists were read off the game binary's own writer rather than guessed, so
//! they are the keys the game actually round-trips. A preset carries only settings —
//! never the bindings, never the device — see [`TUNING_KEYS`].

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::presets::{decode_ini, encode_ini, IniDoc};

/// `profile.ini` sections a feel preset owns, whole.
///
/// Deliberately not `[sound]`, `[misc]` or `[autochat]`: volumes, units, date format and
/// canned chat lines are the rider's, not the discipline's, and nobody wants a shared
/// preset to change them.
pub const FEEL_SECTIONS: [&str; 4] = ["input", "aids", "view", "ext_view"];

/// `[gfx]` is split rather than taken whole. The quality keys are what changes between a
/// tight indoor track and an outdoor one; the rest describe the player's monitor.
pub const GFX_SECTION: &str = "gfx";

/// `[gfx]` keys that describe hardware, not looks. A preset that carried these could hand
/// someone a refresh rate their monitor can't drive, from a share code they only wanted a
/// draw distance out of.
const GFX_DISPLAY_KEYS: [&str; 5] = [
    "fullscreen",
    "refresh",
    "vsync",
    "multisample",
    "display_ratio",
];

/// The per-control suffixes in `controls.txt` that describe *feel*.
///
/// Everything else under `control<N>/` is the binding — `input/type`, `input/name`,
/// `input/num`, `input/sign` — which names a physical axis on a physical device. Carrying
/// those would mean a preset shared between two riders rebinds the receiver's controller,
/// and a preset saved before replugging a pad would rebind its own author. So the tuning
/// travels and the binding never does.
pub const TUNING_KEYS: [&str; 10] = [
    "gain",
    "deadzone",
    "linearity",
    "smooth/enable",
    "smooth/press",
    "smooth/release",
    "forcefeedback/enable",
    "forcefeedback/maxforce",
    "forcefeedback/deadzone",
    "forcefeedback/linearity",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Feel {
    pub name: String,
    /// `profile.ini` values: section → key → value. Sections are stored as the file spells
    /// them rather than as named fields, because the two games don't write the same set and
    /// a game patch can add a key without this needing to know about it.
    pub ini: BTreeMap<String, BTreeMap<String, String>>,
    /// `controls.txt` tuning: control *name* → key → value.
    ///
    /// Keyed by name, never by the `control<N>` index it was found at. The index is just
    /// the order the game happened to store bindings in; it moves when a rider rebinds, so
    /// an index-keyed preset would quietly apply the throttle's smoothing to the clutch.
    pub controls: BTreeMap<String, BTreeMap<String, String>>,
}

impl Feel {
    /// Whether there is anything here to apply. A preset captured from a profile with no
    /// `controls.txt` and an empty `profile.ini` would silently do nothing.
    pub fn is_empty(&self) -> bool {
        self.ini.values().all(BTreeMap::is_empty) && self.controls.is_empty()
    }
}

fn profile_dir(profiles_dir: &Path, profile: &str) -> PathBuf {
    profiles_dir.join(profile)
}

fn profile_ini_path(profiles_dir: &Path, profile: &str) -> PathBuf {
    profile_dir(profiles_dir, profile).join("profile.ini")
}

fn controls_path(profiles_dir: &Path, profile: &str) -> PathBuf {
    profile_dir(profiles_dir, profile).join("controls.txt")
}

/// Whether a `[gfx]` key is one a preset carries.
fn is_gfx_quality(key: &str) -> bool {
    !GFX_DISPLAY_KEYS
        .iter()
        .any(|d| d.eq_ignore_ascii_case(key))
}

/// Whether a `profile.ini` section belongs to a feel preset, and which of its keys.
fn wanted_ini_key(section: &str, key: &str) -> bool {
    if FEEL_SECTIONS.iter().any(|s| s.eq_ignore_ascii_case(section)) {
        return true;
    }
    section.eq_ignore_ascii_case(GFX_SECTION) && is_gfx_quality(key)
}

// ---------------------------------------------------------------------------
// controls.txt
// ---------------------------------------------------------------------------

/// `controls.txt` as a list of lines, edited in place.
///
/// Its keys are paths — `control3/smooth/press` — which are unique across the whole file,
/// so this reads it flat and ignores section headers entirely rather than guessing whether
/// the game writes any. The separator is preserved per line: the file is written by the
/// game's own config layer, and a rewrite that swapped `key value` for `key=value` would be
/// a format change made on a hunch.
struct ControlsDoc {
    lines: Vec<String>,
    crlf: bool,
}

/// Split one line into its key, its value, and the offset the value starts at.
///
/// The offset is what lets a rewrite keep the line's own shape. PiBoSo writes `key = value`
/// with spaces; re-rendering it as `key=value` would work but would churn every line the
/// app touched, which is exactly what makes a diff of the game's own file unreadable.
fn split_line(line: &str) -> Option<(&str, &str, usize)> {
    let indent = line.len() - line.trim_start().len();
    let body = &line[indent..];
    if body.is_empty() || body.starts_with(';') || body.starts_with('[') {
        return None;
    }
    let at = body.find('=').or_else(|| body.find(char::is_whitespace))?;
    let key = body[..at].trim_end();
    if key.is_empty() {
        return None;
    }
    let rest = &body[at + 1..];
    let start = indent + at + 1 + (rest.len() - rest.trim_start().len());
    Some((key, line[start..].trim_end(), start))
}

impl ControlsDoc {
    fn parse(text: &str) -> Self {
        let crlf = text.contains("\r\n");
        let lines = text
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        ControlsDoc { lines, crlf }
    }

    fn render(&self) -> String {
        let sep = if self.crlf { "\r\n" } else { "\n" };
        self.lines.join(sep)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|l| {
            let (k, v, _) = split_line(l)?;
            k.eq_ignore_ascii_case(key).then_some(v)
        })
    }

    /// Overwrite `key` if it's there. A key the file doesn't already carry is *not* added:
    /// the game writes every control it knows about, so a missing key means this install
    /// has no such control — inventing one would put a line in the file the game never
    /// wrote and can't read back onto anything.
    fn set(&mut self, key: &str, value: &str) -> bool {
        for line in self.lines.iter_mut() {
            let Some((k, _, start)) = split_line(line) else {
                continue;
            };
            if !k.eq_ignore_ascii_case(key) {
                continue;
            }
            // Keep everything up to where the value began — indent, key and separator.
            let head = line[..start].to_string();
            *line = format!("{head}{value}");
            return true;
        }
        false
    }

    /// Every `control<N>` index in the file paired with the control's name.
    fn controls(&self) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for i in 0.. {
            match self.get(&format!("control{i}/name")) {
                Some(name) if !name.trim().is_empty() => out.push((i, name.trim().to_string())),
                // The indices run 0..n with no gaps; the first one missing a name is the end.
                _ => break,
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// capture / apply
// ---------------------------------------------------------------------------

/// Read the settings a profile is currently running as a preset body.
///
/// A missing `controls.txt` is not an error: a profile that has never had its controls
/// opened doesn't have one, and the `profile.ini` half is still worth capturing.
pub fn capture(profiles_dir: &Path, profile: &str) -> anyhow::Result<Feel> {
    let path = profile_ini_path(profiles_dir, profile);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let doc = IniDoc::parse(&decode_ini(&bytes).0);

    let mut ini: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for section in doc.sections() {
        for key in doc.section_keys(&section) {
            if !wanted_ini_key(&section, &key) {
                continue;
            }
            if let Some(value) = doc.get(&section, &key) {
                ini.entry(section.clone())
                    .or_default()
                    .insert(key, value.trim().to_string());
            }
        }
    }

    let mut controls: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    if let Ok(text) = fs::read(controls_path(profiles_dir, profile)) {
        let doc = ControlsDoc::parse(&decode_ini(&text).0);
        for (idx, name) in doc.controls() {
            let mut tuning = BTreeMap::new();
            for key in TUNING_KEYS {
                if let Some(v) = doc.get(&format!("control{idx}/{key}")) {
                    tuning.insert(key.to_string(), v.to_string());
                }
            }
            if !tuning.is_empty() {
                controls.insert(name, tuning);
            }
        }
    }

    Ok(Feel {
        name: String::new(),
        ini,
        controls,
    })
}

/// What an apply actually changed, so the UI can say so rather than claiming success over
/// a profile that ignored half the preset.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    /// `profile.ini` keys written.
    pub settings: usize,
    /// `controls.txt` tuning values written.
    pub tuning: usize,
    /// Controls named by the preset that this profile doesn't have bound.
    pub missing_controls: Vec<String>,
}

/// Write a feel preset back into a profile.
///
/// Both files get the same rolling `.bak` [`crate::presets::apply_loadout`] writes, for the
/// same reason: one apply is always undoable, and the backup is the raw bytes so it stays
/// byte-identical to what the game wrote.
///
/// The caller must have checked the game isn't running. The game holds both files in memory
/// for the whole session and writes them out when Options is closed, so anything written
/// underneath it is overwritten without trace.
pub fn apply(profiles_dir: &Path, profile: &str, feel: &Feel) -> anyhow::Result<ApplyReport> {
    let mut report = ApplyReport::default();

    let path = profile_ini_path(profiles_dir, profile);
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let _ = fs::write(format!("{}.bak", path.display()), &bytes);

    let (text, was_utf8) = decode_ini(&bytes);
    let mut doc = IniDoc::parse(&text);
    for (section, keys) in &feel.ini {
        // Only into a section the profile already has. GP Bikes has no `[ext_view]`, and a
        // section this app invented would be one the game never reads and never clears.
        if !doc.has_section(section) {
            continue;
        }
        for (key, value) in keys {
            doc.set(section, key, value);
            report.settings += 1;
        }
    }
    fs::write(&path, encode_ini(&doc.render(), was_utf8))
        .with_context(|| format!("writing {}", path.display()))?;

    if feel.controls.is_empty() {
        return Ok(report);
    }
    let path = controls_path(profiles_dir, profile);
    let Ok(bytes) = fs::read(&path) else {
        // No `controls.txt` to write into: the rider has never opened the controls screen
        // on this profile, so there are no controls to tune. Say so rather than inventing
        // a file whose binding half we deliberately don't carry.
        report.missing_controls = feel.controls.keys().cloned().collect();
        return Ok(report);
    };
    let _ = fs::write(format!("{}.bak", path.display()), &bytes);

    let (text, was_utf8) = decode_ini(&bytes);
    let mut doc = ControlsDoc::parse(&text);
    let here: BTreeMap<String, usize> = doc
        .controls()
        .into_iter()
        .map(|(idx, name)| (name.to_ascii_lowercase(), idx))
        .collect();
    for (name, tuning) in &feel.controls {
        let Some(&idx) = here.get(&name.to_ascii_lowercase()) else {
            report.missing_controls.push(name.clone());
            continue;
        };
        for (key, value) in tuning {
            if doc.set(&format!("control{idx}/{key}"), value) {
                report.tuning += 1;
            }
        }
    }
    fs::write(&path, encode_ini(&doc.render(), was_utf8))
        .with_context(|| format!("writing {}", path.display()))?;

    Ok(report)
}

// ---------------------------------------------------------------------------
// storage
// ---------------------------------------------------------------------------

fn store_path(dir: &Path) -> PathBuf {
    dir.join("feels.json")
}

pub fn load_feels(dir: &Path) -> Vec<Feel> {
    match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn write_feels(dir: &Path, feels: &[Feel]) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(feels)?)?;
    Ok(())
}

pub fn save_feel(dir: &Path, feel: Feel) -> anyhow::Result<()> {
    if feel.name.trim().is_empty() {
        anyhow::bail!("a feel preset needs a name");
    }
    let mut all = load_feels(dir);
    all.retain(|f| !f.name.eq_ignore_ascii_case(&feel.name));
    all.push(feel);
    all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    write_feels(dir, &all)
}

pub fn delete_feel(dir: &Path, name: &str) -> anyhow::Result<()> {
    let mut all = load_feels(dir);
    all.retain(|f| !f.name.eq_ignore_ascii_case(name));
    write_feels(dir, &all)
}

pub fn find_feel(dir: &Path, name: &str) -> Option<Feel> {
    load_feels(dir)
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------
// share codes
// ---------------------------------------------------------------------------

const CODE_PREFIX: &str = "MXBF1-";

pub fn export_code(dir: &Path, name: &str) -> anyhow::Result<String> {
    let feel = find_feel(dir, name).ok_or_else(|| anyhow::anyhow!("no feel preset named '{name}'"))?;
    Ok(encode_code(&feel))
}

pub fn encode_code(feel: &Feel) -> String {
    let json = serde_json::to_vec(feel).unwrap_or_default();
    format!("{CODE_PREFIX}{}", STANDARD.encode(json))
}

/// Reject a decoded preset that could write something other than a setting.
///
/// Every value here ends up as a line in a file the game parses. A newline in one would
/// let whoever wrote the code append arbitrary lines to the receiver's `profile.ini` —
/// including `[info] bikeid`, which is not a setting at all.
fn check_feel(feel: &Feel) -> anyhow::Result<()> {
    let clean = |s: &str| !s.chars().any(|c| c.is_control());
    if !clean(&feel.name) {
        anyhow::bail!("share code has a malformed preset name");
    }
    for (section, keys) in &feel.ini {
        if !clean(section) || section.contains(['[', ']']) {
            anyhow::bail!("share code has a malformed section ('{section}')");
        }
        let known = FEEL_SECTIONS.iter().any(|s| s.eq_ignore_ascii_case(section))
            || section.eq_ignore_ascii_case(GFX_SECTION);
        if !known {
            anyhow::bail!("share code carries settings outside a feel preset ('{section}')");
        }
        for (key, value) in keys {
            if !clean(key) || !clean(value) || key.contains('=') {
                anyhow::bail!("share code has a malformed '{section}' value");
            }
            if !wanted_ini_key(section, key) {
                anyhow::bail!("share code carries a '{section}' key a preset can't set ('{key}')");
            }
        }
    }
    for (name, tuning) in &feel.controls {
        if !clean(name) {
            anyhow::bail!("share code has a malformed control name");
        }
        for (key, value) in tuning {
            if !TUNING_KEYS.contains(&key.as_str()) {
                anyhow::bail!("share code carries a control key that isn't a setting ('{key}')");
            }
            if !clean(value) {
                anyhow::bail!("share code has a malformed '{name}' value");
            }
        }
    }
    Ok(())
}

pub fn decode_code(text: &str) -> anyhow::Result<Feel> {
    let feel = parse_code(text)?;
    check_feel(&feel)?;
    Ok(feel)
}

fn parse_code(text: &str) -> anyhow::Result<Feel> {
    let t = text.trim();
    if let Some(b64) = t.strip_prefix(CODE_PREFIX) {
        let bytes = STANDARD
            .decode(b64.trim())
            .context("feel code isn't valid (bad base64)")?;
        return serde_json::from_slice(&bytes).context("feel code isn't a valid preset");
    }
    if t.starts_with('{') {
        return serde_json::from_str(t).context("that JSON isn't a valid feel preset");
    }
    let bytes = STANDARD
        .decode(t)
        .context("that doesn't look like a feel preset code")?;
    serde_json::from_slice(&bytes).context("feel code isn't a valid preset")
}

pub fn import_code(dir: &Path, text: &str) -> anyhow::Result<Feel> {
    let feel = decode_code(text)?;
    save_feel(dir, feel.clone())?;
    Ok(feel)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = "\
[info]
bikeid=YZ450F
race_number=92

[paint]
YZ450F=TC 222

[input]
controller_profile=Custom
gearbox_preload=0
sit_direct=1
combined_brakes=0
rider_tracking=0
rumble=1

[aids]
leanhelp=1
leanhelp_scale=0.500000
autoshift=0
brakehelp=0

[view]
tilt=0.000000
lean_heading_scale=1.000000
show_HUD=1

[ext_view]
mode=0
distance=4.000000

[gfx]
fullscreen=1
refresh=144
drawdistance=1500.000000
3d_grass=1
shadow_disable=0

[sound]
master_volume=0.800000
";

    const CONTROLS: &str = "\
control0/name=Throttle
control0/gain=1.000000
control0/deadzone=0.000000
control0/linearity=0.000000
control0/smooth/enable=1
control0/smooth/press=0.200000
control0/smooth/release=0.100000
control0/input/type=axis
control0/input/name=Gamepad
control0/input/num=2
control1/name=Lean
control1/gain=0.750000
control1/deadzone=0.050000
control1/linearity=0.000000
control1/input/type=axis
control1/input/name=Gamepad
control1/input/num=0
";

    fn profile_at(root: &Path, controls: Option<&str>) {
        let dir = root.join("profiles").join("main");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("profile.ini"), PROFILE).unwrap();
        if let Some(c) = controls {
            fs::write(dir.join("controls.txt"), c).unwrap();
        }
    }

    fn tmp() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "mxb-feel-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn capture_takes_the_feel_sections_and_leaves_the_rest() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let feel = capture(&root.join("profiles"), "main").unwrap();

        assert_eq!(feel.ini["input"]["sit_direct"], "1");
        assert_eq!(feel.ini["aids"]["leanhelp_scale"], "0.500000");
        assert_eq!(feel.ini["view"]["lean_heading_scale"], "1.000000");
        assert_eq!(feel.ini["ext_view"]["distance"], "4.000000");

        // Cosmetics and the rider's own settings are not a discipline.
        assert!(!feel.ini.contains_key("paint"));
        assert!(!feel.ini.contains_key("info"));
        assert!(!feel.ini.contains_key("sound"));
    }

    #[test]
    fn capture_takes_gfx_quality_but_not_the_monitor() {
        let root = tmp();
        profile_at(&root, None);
        let feel = capture(&root.join("profiles"), "main").unwrap();

        assert_eq!(feel.ini["gfx"]["drawdistance"], "1500.000000");
        assert_eq!(feel.ini["gfx"]["3d_grass"], "1");
        assert!(!feel.ini["gfx"].contains_key("fullscreen"));
        assert!(!feel.ini["gfx"].contains_key("refresh"));
    }

    #[test]
    fn capture_takes_tuning_but_never_the_binding() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let feel = capture(&root.join("profiles"), "main").unwrap();

        let throttle = &feel.controls["Throttle"];
        assert_eq!(throttle["smooth/press"], "0.200000");
        assert_eq!(throttle["gain"], "1.000000");
        // The device and the axis it sits on stay with the rider who owns them.
        assert!(!throttle.contains_key("input/name"));
        assert!(!throttle.contains_key("input/num"));
        assert!(!throttle.contains_key("input/type"));
    }

    #[test]
    fn apply_round_trips_and_leaves_every_other_line_alone() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let profiles = root.join("profiles");

        let mut sx = capture(&profiles, "main").unwrap();
        sx.name = "Supercross".into();
        sx.ini.get_mut("aids").unwrap().insert("leanhelp_scale".into(), "0.900000".into());
        sx.controls
            .get_mut("Throttle")
            .unwrap()
            .insert("smooth/press".into(), "0.450000".into());

        let report = apply(&profiles, "main", &sx).unwrap();
        assert!(report.missing_controls.is_empty());
        assert!(report.tuning > 0);

        let after = capture(&profiles, "main").unwrap();
        assert_eq!(after.ini["aids"]["leanhelp_scale"], "0.900000");
        assert_eq!(after.controls["Throttle"]["smooth/press"], "0.450000");

        // The cosmetic half of `profile.ini` is untouched.
        let text = fs::read_to_string(profiles.join("main/profile.ini")).unwrap();
        assert!(text.contains("YZ450F=TC 222"));
        assert!(text.contains("race_number=92"));
        assert!(text.contains("master_volume=0.800000"));
        // And so is every binding.
        let controls = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        assert!(controls.contains("control0/input/num=2"));
        assert!(controls.contains("control1/input/name=Gamepad"));
    }

    #[test]
    fn apply_follows_the_control_name_when_indices_move() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let profiles = root.join("profiles");

        let mut sx = capture(&profiles, "main").unwrap();
        sx.name = "Supercross".into();
        sx.controls.get_mut("Throttle").unwrap().insert("gain".into(), "0.600000".into());

        // The rider rebinds, and the game rewrites the file with the order swapped.
        let swapped = CONTROLS
            .replace("control0/", "controlX/")
            .replace("control1/", "control0/")
            .replace("controlX/", "control1/");
        fs::write(profiles.join("main/controls.txt"), &swapped).unwrap();

        apply(&profiles, "main", &sx).unwrap();

        let doc = ControlsDoc::parse(&fs::read_to_string(profiles.join("main/controls.txt")).unwrap());
        // Throttle is at index 1 now, and that is where its gain landed.
        assert_eq!(doc.get("control1/name"), Some("Throttle"));
        assert_eq!(doc.get("control1/gain"), Some("0.600000"));
        // The lean axis kept its own gain rather than inheriting the throttle's.
        assert_eq!(doc.get("control0/name"), Some("Lean"));
        assert_eq!(doc.get("control0/gain"), Some("0.750000"));
    }

    #[test]
    fn apply_backs_both_files_up_first() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let profiles = root.join("profiles");

        let mut sx = capture(&profiles, "main").unwrap();
        sx.name = "Supercross".into();
        sx.ini.get_mut("aids").unwrap().insert("leanhelp".into(), "0".into());
        apply(&profiles, "main", &sx).unwrap();

        assert_eq!(fs::read_to_string(profiles.join("main/profile.ini.bak")).unwrap(), PROFILE);
        assert_eq!(fs::read_to_string(profiles.join("main/controls.txt.bak")).unwrap(), CONTROLS);
    }

    #[test]
    fn apply_never_invents_a_section_the_game_doesnt_have() {
        let root = tmp();
        let profiles = root.join("profiles");
        fs::create_dir_all(profiles.join("gpb")).unwrap();
        // GP Bikes has no `[ext_view]`.
        fs::write(profiles.join("gpb/profile.ini"), "[info]\nbikeid=X\n\n[aids]\nleanhelp=1\n").unwrap();

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.ini.insert("aids".into(), [("leanhelp".to_string(), "0".to_string())].into());
        feel.ini.insert("ext_view".into(), [("distance".to_string(), "9".to_string())].into());
        apply(&profiles, "gpb", &feel).unwrap();

        let text = fs::read_to_string(profiles.join("gpb/profile.ini")).unwrap();
        assert!(text.contains("leanhelp=0"));
        assert!(!text.contains("ext_view"));
    }

    #[test]
    fn apply_reports_a_control_the_profile_hasnt_bound() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let profiles = root.join("profiles");

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.controls.insert("Clutch".into(), [("gain".to_string(), "1".to_string())].into());
        let report = apply(&profiles, "main", &feel).unwrap();
        assert_eq!(report.missing_controls, vec!["Clutch".to_string()]);
    }

    #[test]
    fn controls_doc_keeps_a_space_separated_file_space_separated() {
        let mut doc = ControlsDoc::parse("control0/name Throttle\ncontrol0/gain 1.000000\n");
        assert_eq!(doc.get("control0/gain"), Some("1.000000"));
        assert!(doc.set("control0/gain", "0.500000"));
        assert!(doc.render().contains("control0/gain 0.500000"));
        assert!(!doc.render().contains('='));
    }

    #[test]
    fn controls_doc_keeps_pibosos_spacing_around_the_equals() {
        let mut doc = ControlsDoc::parse("control0/name = Throttle\ncontrol0/gain = 1.000000\n");
        assert_eq!(doc.get("control0/gain"), Some("1.000000"));
        assert!(doc.set("control0/gain", "0.500000"));
        assert!(doc.render().contains("control0/gain = 0.500000"));
    }

    #[test]
    fn controls_doc_wont_add_a_control_the_file_doesnt_have() {
        let mut doc = ControlsDoc::parse(CONTROLS);
        assert!(!doc.set("control9/gain", "1.0"));
        assert!(!doc.render().contains("control9"));
    }

    #[test]
    fn a_latin1_profile_survives_the_round_trip() {
        let root = tmp();
        let profiles = root.join("profiles");
        fs::create_dir_all(profiles.join("main")).unwrap();
        // 0xE9 is `é` in Windows-1252 and not valid UTF-8.
        let mut bytes = b"[info]\nbikeid=Bj\xf6rn\n\n[aids]\nleanhelp=1\n".to_vec();
        bytes.push(b'\n');
        fs::write(profiles.join("main/profile.ini"), &bytes).unwrap();

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.ini.insert("aids".into(), [("leanhelp".to_string(), "0".to_string())].into());
        apply(&profiles, "main", &feel).unwrap();

        let after = fs::read(profiles.join("main/profile.ini")).unwrap();
        assert!(after.windows(2).any(|w| w == [b'j', 0xf6]), "latin-1 byte was re-encoded");
        assert!(String::from_utf8_lossy(&after).contains("leanhelp=0"));
    }

    #[test]
    fn a_code_round_trips() {
        let root = tmp();
        profile_at(&root, Some(CONTROLS));
        let mut feel = capture(&root.join("profiles"), "main").unwrap();
        feel.name = "Supercross".into();

        let code = encode_code(&feel);
        assert!(code.starts_with(CODE_PREFIX));
        assert_eq!(decode_code(&code).unwrap(), feel);
    }

    #[test]
    fn a_code_cant_smuggle_a_line_into_profile_ini() {
        let mut feel = Feel::default();
        feel.name = "Nasty".into();
        feel.ini.insert(
            "aids".into(),
            [("leanhelp".to_string(), "1\n[info]\nbikeid=gone".to_string())].into(),
        );
        assert!(decode_code(&encode_code(&feel)).is_err());
    }

    #[test]
    fn a_code_cant_carry_a_section_that_isnt_feel() {
        let mut feel = Feel::default();
        feel.name = "Nasty".into();
        feel.ini.insert("paint".into(), [("YZ450F".to_string(), "theirs".to_string())].into());
        assert!(decode_code(&encode_code(&feel)).is_err());
    }

    #[test]
    fn a_code_cant_carry_a_binding() {
        let mut feel = Feel::default();
        feel.name = "Nasty".into();
        feel.controls.insert(
            "Throttle".into(),
            [("input/num".to_string(), "7".to_string())].into(),
        );
        assert!(decode_code(&encode_code(&feel)).is_err());
    }

    #[test]
    fn a_code_cant_set_the_monitor() {
        let mut feel = Feel::default();
        feel.name = "Nasty".into();
        feel.ini.insert("gfx".into(), [("refresh".to_string(), "23".to_string())].into());
        assert!(decode_code(&encode_code(&feel)).is_err());
    }

    #[test]
    fn saving_and_deleting_by_name_is_case_insensitive() {
        let dir = tmp();
        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.ini.insert("aids".into(), [("leanhelp".to_string(), "1".to_string())].into());
        save_feel(&dir, feel.clone()).unwrap();

        feel.ini.insert("aids".into(), [("leanhelp".to_string(), "0".to_string())].into());
        save_feel(&dir, feel).unwrap();
        assert_eq!(load_feels(&dir).len(), 1, "a re-save replaces rather than duplicates");
        assert_eq!(find_feel(&dir, "SUPERCROSS").unwrap().ini["aids"]["leanhelp"], "0");

        delete_feel(&dir, "supercross").unwrap();
        assert!(load_feels(&dir).is_empty());
    }

    #[test]
    fn a_nameless_preset_is_refused() {
        let dir = tmp();
        assert!(save_feel(&dir, Feel::default()).is_err());
    }
}
