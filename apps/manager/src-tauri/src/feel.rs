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
//! * `profiles/<name>/controls.txt` — one line per control: its name, its binding, then
//!   its feel (deadzone, linearity, gain, smoothing, force feedback).
//!
//! `controls.txt` was read off MX Bikes' own writer (0x14028cdb0) and reader (0x14028c9a0).
//! Each line is whitespace-separated tokens, written with a space after each:
//!
//! ```text
//! CTRL_THROTTLE AXIS 6F1D2B60-D5A0-11CF-BFC7-444553540000 2 0 0.020000 0.000000 1.000000 1 0.200000 0.100000 0 1.000000 0.000000 0.000000
//! CTRL_SITDirect KEY 18 0.000000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000
//! ```
//!
//! The name is case-sensitive. The binding in the middle varies by type (`KEY`, `BUTTON`,
//! `AXIS`, `POV`, the plugin `C_*` kinds, or nothing when unbound), but the line always
//! ends in the same ten tuning values, [`TUNING_KEYS`]. So the tuning is found by counting
//! from the end, and the binding never has to be understood to be left alone.
//!
//! A preset carries only settings — never the bindings, never the device. This is
//! MX Bikes' format; GP Bikes' `controls.txt` hasn't been checked.

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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

/// The ten tuning values that end every `controls.txt` line, in the order the game writes
/// them (`%f %f %f %d %f %f %d %f %f %f`).
///
/// Deadzone, linearity and smooth enable are confirmed from the binary. Gain, and press
/// before release, are inferred; the force-feedback order is inferred from the
/// `controllers\*.cfg` loader.
///
/// Everything before these on the line is the binding, which names a physical axis on a
/// physical device. Carrying it would mean a preset shared between two riders rebinds the
/// receiver's controller. So the tuning travels and the binding never does.
pub const TUNING_KEYS: [&str; 10] = [
    "deadzone",
    "linearity",
    "gain",
    "smooth/enable",
    "smooth/press",
    "smooth/release",
    "forcefeedback/enable",
    "forcefeedback/maxforce",
    "forcefeedback/deadzone",
    "forcefeedback/linearity",
];

/// Positions in [`TUNING_KEYS`] the game writes with `%d` rather than `%f`.
const INT_SLOTS: [usize; 2] = [3, 6];

/// Riding controls. `RPLY_*` and `SYST_*` lines are replay and menu keys, not feel.
const CONTROL_PREFIX: &str = "CTRL_";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Feel {
    pub name: String,
    /// `profile.ini` values: section → key → value. Sections are stored as the file spells
    /// them rather than as named fields, because the two games don't write the same set and
    /// a game patch can add a key without this needing to know about it.
    pub ini: BTreeMap<String, BTreeMap<String, String>>,
    /// `controls.txt` tuning: the game's control name (`CTRL_THROTTLE`) → a
    /// [`TUNING_KEYS`] name → value.
    ///
    /// Keyed by name, never by line number: the order is just how the game happened to
    /// write the file, and applying by position would put the throttle's smoothing on
    /// whatever control moved into its line.
    pub controls: BTreeMap<String, BTreeMap<String, String>>,
}

impl Feel {
    /// Whether there is anything here to apply. A preset captured from a profile with no
    /// `controls.txt` and an empty `profile.ini` would silently do nothing.
    pub fn is_empty(&self) -> bool {
        self.ini.values().all(BTreeMap::is_empty) && self.controls.is_empty()
    }

    /// Drop controls saved under names the game doesn't use.
    ///
    /// Presets from before the format fix keyed controls as `Throttle` or `Lean`. No real
    /// `controls.txt` has those names, so whatever was saved under them came from some
    /// other file and was never the rider's own tuning. The settings half still works.
    fn drop_unknown_controls(&mut self) {
        self.controls.retain(|name, _| name.starts_with(CONTROL_PREFIX));
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

/// One `controls.txt` line, split into the parts this module cares about.
struct ControlLine<'a> {
    name: &'a str,
    /// Whether anything sits between the name and the tuning.
    bound: bool,
    /// Byte spans of the ten tuning tokens, in [`TUNING_KEYS`] order.
    tuning: [(usize, usize); 10],
}

/// Byte spans of every whitespace-separated token on a line.
fn token_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                spans.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        spans.push((s, line.len()));
    }
    spans
}

/// Whether `token` is a number the game could have written in tuning slot `slot`.
fn valid_tuning(slot: usize, token: &str) -> bool {
    if INT_SLOTS.contains(&slot) {
        token.parse::<i64>().is_ok()
    } else {
        token.parse::<f64>().is_ok_and(f64::is_finite)
    }
}

/// Read a control line, or `None` for anything that doesn't end in ten tuning values.
fn parse_control(line: &str) -> Option<ControlLine<'_>> {
    let spans = token_spans(line);
    // The name plus ten tuning values; an unbound control has nothing in between.
    if spans.len() < 1 + TUNING_KEYS.len() {
        return None;
    }
    let tail = &spans[spans.len() - TUNING_KEYS.len()..];
    let mut tuning = [(0, 0); 10];
    for (slot, &(s, e)) in tail.iter().enumerate() {
        if !valid_tuning(slot, &line[s..e]) {
            return None;
        }
        tuning[slot] = (s, e);
    }
    let (s, e) = spans[0];
    Some(ControlLine {
        name: &line[s..e],
        bound: spans.len() > 1 + TUNING_KEYS.len(),
        tuning,
    })
}

/// A stored value in the shape the game writes for `slot`: `%d` or `%f`.
fn format_tuning(slot: usize, value: &str) -> Option<String> {
    let v = value.trim();
    if INT_SLOTS.contains(&slot) {
        let n = v
            .parse::<i64>()
            .ok()
            .or_else(|| v.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| f.round() as i64))?;
        Some(n.to_string())
    } else {
        let f = v.parse::<f64>().ok().filter(|f| f.is_finite())?;
        Some(format!("{f:.6}"))
    }
}

/// Rewrite a line's tuning tokens from a preset, and nothing else.
///
/// Only the tokens are replaced; the name, the binding and every byte of spacing between
/// them stay exactly as the game wrote them. A key the preset doesn't carry, or a value
/// that isn't a number, leaves the file's own value in place. Returns the new line and how
/// many values the preset set.
fn retune(line: &str, control: &ControlLine, tuning: &BTreeMap<String, String>) -> (String, usize) {
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    let mut written = 0;
    for (slot, &(s, e)) in control.tuning.iter().enumerate() {
        out.push_str(&line[at..s]);
        match tuning.get(TUNING_KEYS[slot]).and_then(|v| format_tuning(slot, v)) {
            Some(v) => {
                out.push_str(&v);
                written += 1;
            }
            None => out.push_str(&line[s..e]),
        }
        at = e;
    }
    out.push_str(&line[at..]);
    (out, written)
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
    if let Ok(bytes) = fs::read(controls_path(profiles_dir, profile)) {
        let text = decode_ini(&bytes).0;
        for line in text.lines() {
            let Some(control) = parse_control(line) else {
                continue;
            };
            // An unbound control's tuning shapes nothing the rider feels, so it isn't part
            // of the discipline. Apply still reaches it if the receiver has it bound.
            if !control.bound || !control.name.starts_with(CONTROL_PREFIX) {
                continue;
            }
            let tuning = TUNING_KEYS
                .iter()
                .zip(control.tuning)
                .map(|(k, (s, e))| (k.to_string(), line[s..e].to_string()))
                .collect();
            controls.insert(control.name.to_string(), tuning);
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
    /// Controls named by the preset that this profile's `controls.txt` doesn't have.
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
    let mut out = String::with_capacity(text.len());
    let mut found = BTreeSet::new();
    // Line by line with each line's own ending kept, so a CRLF file stays CRLF and every
    // line this doesn't tune comes back byte-for-byte.
    for raw in text.split_inclusive('\n') {
        let body = raw.trim_end_matches(['\r', '\n']);
        let eol = &raw[body.len()..];
        // A control missing from the file is *not* added: the game writes every control it
        // knows, so a missing one is a control this install doesn't have.
        let hit = parse_control(body)
            .and_then(|c| feel.controls.get(c.name).map(|tuning| (c, tuning)));
        match hit {
            Some((control, tuning)) => {
                found.insert(control.name.to_string());
                let (line, written) = retune(body, &control, tuning);
                report.tuning += written;
                out.push_str(&line);
            }
            None => out.push_str(body),
        }
        out.push_str(eol);
    }
    report.missing_controls = feel
        .controls
        .keys()
        .filter(|name| !found.contains(*name))
        .cloned()
        .collect();
    if out != text {
        fs::write(&path, encode_ini(&out, was_utf8))
            .with_context(|| format!("writing {}", path.display()))?;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// storage
// ---------------------------------------------------------------------------

fn store_path(dir: &Path) -> PathBuf {
    dir.join("feels.json")
}

pub fn load_feels(dir: &Path) -> Vec<Feel> {
    let mut feels: Vec<Feel> = match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    feels.iter_mut().for_each(Feel::drop_unknown_controls);
    feels
}

fn write_feels(dir: &Path, feels: &[Feel]) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(feels)?)?;
    Ok(())
}

pub fn save_feel(dir: &Path, mut feel: Feel) -> anyhow::Result<()> {
    if feel.name.trim().is_empty() {
        anyhow::bail!("a feel preset needs a name");
    }
    feel.drop_unknown_controls();
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
        if name.chars().any(|c| c.is_control() || c.is_whitespace()) {
            anyhow::bail!("share code has a malformed control name");
        }
        for (key, value) in tuning {
            if !TUNING_KEYS.contains(&key.as_str()) {
                anyhow::bail!("share code carries a control key that isn't a setting ('{key}')");
            }
            if !value.trim().parse::<f64>().is_ok_and(f64::is_finite) {
                anyhow::bail!("share code has a malformed '{name}' value");
            }
        }
    }
    Ok(())
}

pub fn decode_code(text: &str) -> anyhow::Result<Feel> {
    let mut feel = parse_code(text)?;
    feel.drop_unknown_controls();
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

    const PAD: &str = "6F1D2B60-D5A0-11CF-BFC7-444553540000";

    /// The shape the game writes: a space after every token, one control per line.
    fn controls() -> String {
        [
            format!("CTRL_THROTTLE AXIS {PAD} 2 0 0.020000 0.000000 1.000000 1 0.200000 0.100000 0 1.000000 0.000000 0.000000 "),
            format!("CTRL_LEAN AXIS {PAD} 0 1 0.050000 0.000000 0.750000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 "),
            format!("CTRL_SIT BUTTON {PAD} 1 0.000000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 "),
            "CTRL_SITDirect KEY 18 0.000000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 ".to_string(),
            "CTRL_SHIFTUP KEY 200 17 0.000000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 ".to_string(),
            "CTRL_CLUTCH 0.000000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 ".to_string(),
            "RPLY_PLAY KEY 57 0.000000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 ".to_string(),
        ]
        .map(|l| l + "\n")
        .concat()
    }

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

    fn line_of<'a>(text: &'a str, name: &str) -> &'a str {
        text.lines()
            .find(|l| l.split_whitespace().next() == Some(name))
            .unwrap_or_else(|| panic!("no {name} line"))
    }

    #[test]
    fn capture_takes_the_feel_sections_and_leaves_the_rest() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
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
    fn capture_reads_the_ten_tuning_values_by_control_name() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
        let feel = capture(&root.join("profiles"), "main").unwrap();

        let throttle = &feel.controls["CTRL_THROTTLE"];
        assert_eq!(throttle.len(), 10);
        assert_eq!(throttle["deadzone"], "0.020000");
        assert_eq!(throttle["linearity"], "0.000000");
        assert_eq!(throttle["gain"], "1.000000");
        assert_eq!(throttle["smooth/enable"], "1");
        assert_eq!(throttle["smooth/press"], "0.200000");
        assert_eq!(throttle["smooth/release"], "0.100000");
        assert_eq!(throttle["forcefeedback/enable"], "0");
        assert_eq!(throttle["forcefeedback/maxforce"], "1.000000");
        assert_eq!(feel.controls["CTRL_LEAN"]["gain"], "0.750000");
        // Every binding kind reads the same way, counted from the end.
        assert!(feel.controls.contains_key("CTRL_SIT"));
        assert!(feel.controls.contains_key("CTRL_SITDirect"));
        assert!(feel.controls.contains_key("CTRL_SHIFTUP"));
    }

    #[test]
    fn capture_skips_unbound_controls_and_non_riding_keys() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
        let feel = capture(&root.join("profiles"), "main").unwrap();
        assert!(!feel.controls.contains_key("CTRL_CLUTCH"));
        assert!(!feel.controls.contains_key("RPLY_PLAY"));
    }

    #[test]
    fn capture_ignores_the_old_key_value_shape() {
        let root = tmp();
        profile_at(&root, Some("control0/name = Throttle\ncontrol0/gain = 1.000000\n"));
        let feel = capture(&root.join("profiles"), "main").unwrap();
        assert!(feel.controls.is_empty());
    }

    #[test]
    fn apply_rewrites_only_the_tuning_and_keeps_the_binding() {
        let root = tmp();
        let text = controls();
        profile_at(&root, Some(&text));
        let profiles = root.join("profiles");

        let mut sx = capture(&profiles, "main").unwrap();
        sx.name = "Supercross".into();
        sx.ini.get_mut("aids").unwrap().insert("leanhelp_scale".into(), "0.900000".into());
        let throttle = sx.controls.get_mut("CTRL_THROTTLE").unwrap();
        throttle.insert("smooth/press".into(), "0.45".into());
        throttle.insert("forcefeedback/enable".into(), "1".into());

        let report = apply(&profiles, "main", &sx).unwrap();
        assert!(report.missing_controls.is_empty());
        assert_eq!(report.tuning, 10 * sx.controls.len());

        let after = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        assert_eq!(
            line_of(&after, "CTRL_THROTTLE"),
            format!("CTRL_THROTTLE AXIS {PAD} 2 0 0.020000 0.000000 1.000000 1 0.450000 0.100000 1 1.000000 0.000000 0.000000 ")
        );
        // Every other line comes back exactly as the game wrote it, in the same order.
        let before_rest: Vec<_> = text.lines().filter(|l| !l.starts_with("CTRL_THROTTLE")).collect();
        let after_rest: Vec<_> = after.lines().filter(|l| !l.starts_with("CTRL_THROTTLE")).collect();
        assert_eq!(before_rest, after_rest);

        let recaptured = capture(&profiles, "main").unwrap();
        assert_eq!(recaptured.ini["aids"]["leanhelp_scale"], "0.900000");
        assert_eq!(recaptured.controls["CTRL_THROTTLE"]["smooth/press"], "0.450000");

        // The cosmetic half of `profile.ini` is untouched.
        let ini = fs::read_to_string(profiles.join("main/profile.ini")).unwrap();
        assert!(ini.contains("YZ450F=TC 222"));
        assert!(ini.contains("race_number=92"));
        assert!(ini.contains("master_volume=0.800000"));
    }

    #[test]
    fn apply_follows_the_control_name_when_lines_move() {
        let root = tmp();
        let text = controls();
        profile_at(&root, Some(&text));
        let profiles = root.join("profiles");

        let mut sx = capture(&profiles, "main").unwrap();
        sx.name = "Supercross".into();
        sx.controls.get_mut("CTRL_THROTTLE").unwrap().insert("gain".into(), "0.6".into());

        // The game rewrites the file with the lines in another order.
        let mut lines: Vec<&str> = text.lines().collect();
        lines.reverse();
        fs::write(profiles.join("main/controls.txt"), lines.join("\n") + "\n").unwrap();

        apply(&profiles, "main", &sx).unwrap();

        let after = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        let throttle = parse_control(line_of(&after, "CTRL_THROTTLE")).unwrap();
        let (s, e) = throttle.tuning[2];
        assert_eq!(&line_of(&after, "CTRL_THROTTLE")[s..e], "0.600000");
        // The lean axis kept its own gain rather than inheriting the throttle's.
        assert!(line_of(&after, "CTRL_LEAN").contains(" 0.750000 "));
        assert!(after.starts_with("RPLY_PLAY"), "line order kept");
    }

    #[test]
    fn apply_tunes_an_unbound_line_without_binding_it() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
        let profiles = root.join("profiles");

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.controls
            .insert("CTRL_CLUTCH".into(), [("deadzone".to_string(), "0.1".to_string())].into());
        let report = apply(&profiles, "main", &feel).unwrap();
        assert!(report.missing_controls.is_empty());
        assert_eq!(report.tuning, 1);

        let after = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        assert_eq!(
            line_of(&after, "CTRL_CLUTCH"),
            "CTRL_CLUTCH 0.100000 0.000000 1.000000 0 0.000000 0.000000 0 0.000000 0.000000 0.000000 "
        );
    }

    #[test]
    fn apply_keeps_a_two_code_key_binding() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
        let profiles = root.join("profiles");

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.controls.insert(
            "CTRL_SHIFTUP".into(),
            [
                ("smooth/enable".to_string(), "1".to_string()),
                ("smooth/press".to_string(), "0.3".to_string()),
            ]
            .into(),
        );
        apply(&profiles, "main", &feel).unwrap();

        let after = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        assert_eq!(
            line_of(&after, "CTRL_SHIFTUP"),
            "CTRL_SHIFTUP KEY 200 17 0.000000 0.000000 1.000000 1 0.300000 0.000000 0 0.000000 0.000000 0.000000 "
        );
    }

    #[test]
    fn apply_keeps_crlf_line_endings() {
        let root = tmp();
        let crlf = controls().replace('\n', "\r\n");
        profile_at(&root, Some(&crlf));
        let profiles = root.join("profiles");

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.controls
            .insert("CTRL_LEAN".into(), [("linearity".to_string(), "0.25".to_string())].into());
        apply(&profiles, "main", &feel).unwrap();

        let after = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        assert_eq!(after.matches("\r\n").count(), crlf.matches("\r\n").count());
        assert_eq!(after.matches('\n').count(), crlf.matches('\n').count(), "no bare LF");
        assert_eq!(after.len(), crlf.len(), "only the one value changed");
        assert!(after.contains(&format!("CTRL_LEAN AXIS {PAD} 0 1 0.050000 0.250000 0.750000 ")));
    }

    #[test]
    fn an_apply_that_changes_nothing_leaves_the_file_byte_identical() {
        let root = tmp();
        let text = controls().replace('\n', "\r\n");
        profile_at(&root, Some(&text));
        let profiles = root.join("profiles");

        let mut feel = capture(&profiles, "main").unwrap();
        feel.name = "Same".into();
        apply(&profiles, "main", &feel).unwrap();
        assert_eq!(fs::read_to_string(profiles.join("main/controls.txt")).unwrap(), text);
    }

    #[test]
    fn apply_leaves_a_value_that_isnt_a_number() {
        let line = format!("CTRL_THROTTLE AXIS {PAD} 2 0 0.020000 0.000000 1.000000 1 0.200000 0.100000 0 1.000000 0.000000 0.000000 ");
        let control = parse_control(&line).unwrap();
        let tuning = [("gain".to_string(), "loud".to_string())].into();
        let (out, written) = retune(&line, &control, &tuning);
        assert_eq!(out, line);
        assert_eq!(written, 0);
    }

    #[test]
    fn apply_backs_both_files_up_first() {
        let root = tmp();
        let text = controls();
        profile_at(&root, Some(&text));
        let profiles = root.join("profiles");

        let mut sx = capture(&profiles, "main").unwrap();
        sx.name = "Supercross".into();
        sx.ini.get_mut("aids").unwrap().insert("leanhelp".into(), "0".into());
        apply(&profiles, "main", &sx).unwrap();

        assert_eq!(fs::read_to_string(profiles.join("main/profile.ini.bak")).unwrap(), PROFILE);
        assert_eq!(fs::read_to_string(profiles.join("main/controls.txt.bak")).unwrap(), text);
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
    fn apply_reports_a_control_the_file_doesnt_have_and_wont_add_it() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
        let profiles = root.join("profiles");

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.controls
            .insert("CTRL_REARBRAKE".into(), [("gain".to_string(), "1".to_string())].into());
        let report = apply(&profiles, "main", &feel).unwrap();
        assert_eq!(report.missing_controls, vec!["CTRL_REARBRAKE".to_string()]);
        let after = fs::read_to_string(profiles.join("main/controls.txt")).unwrap();
        assert!(!after.contains("CTRL_REARBRAKE"));
    }

    #[test]
    fn control_names_match_case_sensitively() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
        let profiles = root.join("profiles");

        let mut feel = Feel::default();
        feel.name = "Supercross".into();
        feel.controls
            .insert("CTRL_SITDIRECT".into(), [("gain".to_string(), "0.5".to_string())].into());
        let report = apply(&profiles, "main", &feel).unwrap();
        assert_eq!(report.missing_controls, vec!["CTRL_SITDIRECT".to_string()]);
    }

    #[test]
    fn a_latin1_profile_survives_the_round_trip() {
        let root = tmp();
        let profiles = root.join("profiles");
        fs::create_dir_all(profiles.join("main")).unwrap();
        // 0xF6 is `ö` in Windows-1252 and not valid UTF-8.
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
    fn old_presets_lose_their_made_up_control_names() {
        let dir = tmp();
        let old = r#"[{"name":"Old","ini":{"aids":{"leanhelp":"1"}},
            "controls":{"Throttle":{"gain":"0.5"},"CTRL_LEAN":{"gain":"0.7"}}}]"#;
        fs::write(dir.join("feels.json"), old).unwrap();

        let feel = find_feel(&dir, "Old").unwrap();
        assert_eq!(feel.ini["aids"]["leanhelp"], "1");
        assert!(!feel.controls.contains_key("Throttle"));
        assert_eq!(feel.controls["CTRL_LEAN"]["gain"], "0.7");
    }

    #[test]
    fn an_old_share_code_still_imports_its_settings() {
        let mut feel = Feel::default();
        feel.name = "Old".into();
        feel.ini.insert("aids".into(), [("leanhelp".to_string(), "0".to_string())].into());
        feel.controls.insert("Throttle".into(), [("gain".to_string(), "0.5".to_string())].into());

        let decoded = decode_code(&encode_code(&feel)).unwrap();
        assert_eq!(decoded.ini["aids"]["leanhelp"], "0");
        assert!(decoded.controls.is_empty());
    }

    #[test]
    fn a_code_round_trips() {
        let root = tmp();
        profile_at(&root, Some(&controls()));
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
            "CTRL_THROTTLE".into(),
            [("input/num".to_string(), "7".to_string())].into(),
        );
        assert!(decode_code(&encode_code(&feel)).is_err());
    }

    #[test]
    fn a_code_cant_carry_a_tuning_value_that_isnt_a_number() {
        let mut feel = Feel::default();
        feel.name = "Nasty".into();
        feel.controls.insert(
            "CTRL_THROTTLE".into(),
            [("gain".to_string(), "1 KEY 18".to_string())].into(),
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
