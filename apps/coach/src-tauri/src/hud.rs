//! What the recorder plugin draws and says in the game, as Coach sets it: `hud.ini` and
//! `cues/voice.ini`, both under `<game user folder>/mxbcoach`. The plugin reads them; a key
//! that isn't there takes the plugin's own default, so only what the rider picks is written.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;

use crate::coach::{load_config, session_dirs};

/// The HUD parts, in the order the overlay lists them.
pub const HUD_PARTS: &[(&str, &str)] = &[
    ("cue", "Live cue"),
    ("section", "Section and tip"),
    ("gap", "Gap to Coach's lap"),
    ("stance", "Sit / stand"),
    ("map", "Track map with ghost and cues"),
    ("setup", "Setup card (when stopped)"),
];

const DEFAULT_VOLUME: u8 = 80;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudPart {
    pub key: &'static str,
    pub label: &'static str,
    pub on: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud {
    pub enabled: bool,
    pub parts: Vec<HudPart>,
    pub file: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub enabled: bool,
    pub volume: u8,
}

/// `<user folder>/mxbcoach`, found the way the sessions are.
fn coach_dir(app: &AppHandle) -> Result<PathBuf, String> {
    session_dirs(&load_config(app))
        .into_iter()
        .find_map(|d| d.parent().map(Path::to_path_buf))
        .ok_or_else(|| "The game's user folder wasn't found.".to_string())
}

/// `key=value` lines of one `[section]`.
fn read_section(text: &str, section: &str) -> Vec<(String, String)> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name.trim().eq_ignore_ascii_case(section);
        } else if inside {
            if let Some((k, v)) = line.split_once('=') {
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

fn get<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
}

/// Set keys in one section, keeping every other line as it was.
fn set_keys(text: &str, section: &str, keys: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let head = lines.iter().position(|l| {
        l.trim()
            .strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .is_some_and(|n| n.trim().eq_ignore_ascii_case(section))
    });
    let head = match head {
        Some(i) => i,
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{section}]"));
            lines.len() - 1
        }
    };
    let mut end = lines[head + 1..]
        .iter()
        .position(|l| l.trim().starts_with('['))
        .map_or(lines.len(), |i| head + 1 + i);
    for (key, value) in keys {
        let found = (head + 1..end).find(|&i| {
            lines[i].split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        });
        match found {
            Some(i) => lines[i] = format!("{key}={value}"),
            None => {
                // After the section's last key, before any blank lines that close it.
                let mut at = end;
                while at > head + 1 && lines[at - 1].trim().is_empty() {
                    at -= 1;
                }
                lines.insert(at, format!("{key}={value}"));
                end += 1;
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Written aside and moved in, so the plugin never reads half a file.
fn write_ini(path: &Path, section: &str, keys: &[(&str, String)]) -> Result<(), String> {
    let dir = path.parent().ok_or("No folder for the settings file.")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = fs::read_to_string(path).unwrap_or_default();
    let tmp = path.with_extension("ini.tmp");
    fs::write(&tmp, set_keys(&text, section, keys)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn on(v: Option<&str>, default: bool) -> bool {
    v.map_or(default, |v| v != "0")
}

fn hud_of(path: &Path) -> Hud {
    let pairs = read_section(&fs::read_to_string(path).unwrap_or_default(), "hud");
    Hud {
        enabled: on(get(&pairs, "enabled"), true),
        parts: HUD_PARTS
            .iter()
            .map(|&(key, label)| HudPart { key, label, on: on(get(&pairs, key), true) })
            .collect(),
        file: path.display().to_string(),
    }
}

fn voice_of(path: &Path) -> Voice {
    let pairs = read_section(&fs::read_to_string(path).unwrap_or_default(), "voice");
    Voice {
        // The plugin treats a missing file or key as off.
        enabled: on(get(&pairs, "enabled"), false),
        volume: get(&pairs, "volume").and_then(|v| v.parse().ok()).map_or(DEFAULT_VOLUME, |v: u8| v.min(100)),
    }
}

fn hud_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(coach_dir(app)?.join("hud.ini"))
}

fn voice_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(coach_dir(app)?.join("cues").join("voice.ini"))
}

#[tauri::command]
pub fn coach_hud(app: AppHandle) -> Result<Hud, String> {
    Ok(hud_of(&hud_path(&app)?))
}

/// Turn one HUD part on or off, or the whole HUD with `enabled`.
#[tauri::command]
pub fn coach_set_hud(app: AppHandle, key: String, on: bool) -> Result<Hud, String> {
    let key = std::iter::once("enabled")
        .chain(HUD_PARTS.iter().map(|(k, _)| *k))
        .find(|k| *k == key)
        .ok_or_else(|| format!("\"{key}\" isn't a HUD part."))?;
    let path = hud_path(&app)?;
    write_ini(&path, "hud", &[(key, if on { "1" } else { "0" }.to_string())])?;
    Ok(hud_of(&path))
}

#[tauri::command]
pub fn coach_voice(app: AppHandle) -> Result<Voice, String> {
    Ok(voice_of(&voice_path(&app)?))
}

/// Whether the recorder speaks its cues, and how loud (0–100).
#[tauri::command]
pub fn coach_set_voice(app: AppHandle, enabled: bool, volume: u8) -> Result<Voice, String> {
    let path = voice_path(&app)?;
    let keys = [("enabled", if enabled { "1" } else { "0" }.to_string()), ("volume", volume.min(100).to_string())];
    write_ini(&path, "voice", &keys)?;
    Ok(voice_of(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_takes_the_plugins_defaults() {
        let hud = hud_of(Path::new("/nowhere/hud.ini"));
        assert!(hud.enabled && hud.parts.iter().all(|p| p.on));
        let voice = voice_of(Path::new("/nowhere/voice.ini"));
        assert!(!voice.enabled, "the plugin treats no file as off");
        assert_eq!(voice.volume, DEFAULT_VOLUME);
    }

    /// Only the key the rider touched is written: the plugin decides `map` for itself
    /// unless the file says `map=1` outright.
    #[test]
    fn setting_one_key_writes_only_that_key() {
        let out = set_keys("", "hud", &[("gap", "0".into())]);
        assert_eq!(out, "[hud]\ngap=0\n");
        assert!(!out.contains("map"));
    }

    #[test]
    fn other_lines_and_sections_survive_a_set() {
        let text = "; mine\n[hud]\nenabled=1\npos_map=10,20\n\n[other]\ngap=1\n";
        let out = set_keys(text, "hud", &[("gap", "0".into()), ("enabled", "0".into())]);
        assert_eq!(out, "; mine\n[hud]\nenabled=0\npos_map=10,20\ngap=0\n\n[other]\ngap=1\n");
        let pairs = read_section(&out, "hud");
        assert_eq!(get(&pairs, "gap"), Some("0"));
        assert_eq!(get(&read_section(&out, "other"), "gap"), Some("1"));
    }

    #[test]
    fn the_ini_is_written_aside_and_moved_in() {
        let dir = std::env::temp_dir().join(format!("coach-hud-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("cues").join("voice.ini");
        write_ini(&path, "voice", &[("enabled", "1".into()), ("volume", "55".into())]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[voice]\nenabled=1\nvolume=55\n");
        assert!(!path.with_extension("ini.tmp").exists());
        let v = voice_of(&path);
        assert!(v.enabled);
        assert_eq!(v.volume, 55);
        let _ = fs::remove_dir_all(&dir);
    }
}
