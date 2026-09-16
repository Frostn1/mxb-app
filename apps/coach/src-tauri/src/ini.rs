//! The small slice of `.ini` the coach needs: read one section, and set keys in one section
//! without disturbing anything else in the file.
//!
//! Two very different files are written through here and both belong to somebody else:
//! `hud.ini`, which the recorder plugin reads, and the game's own `default.ini`, which says
//! which setup a bike loads. In both cases every key the coach doesn't set is the owner's and
//! has to come back out exactly as it went in — a key the coach has never heard of is a
//! setting a later game or plugin version added, not litter.

use std::fs;
use std::path::Path;

/// `key=value` lines of one `[section]`.
pub fn read_section(text: &str, section: &str) -> Vec<(String, String)> {
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

pub fn get<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
}

/// Set keys in one section, keeping every other line as it was.
pub fn set_keys(text: &str, section: &str, keys: &[(&str, String)]) -> String {
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

/// Written aside and moved in, so nothing ever reads half a file.
pub fn write(path: &Path, section: &str, keys: &[(&str, String)]) -> Result<(), String> {
    let dir = path.parent().ok_or("No folder for the settings file.")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = fs::read_to_string(path).unwrap_or_default();
    let tmp = path.with_extension("ini.tmp");
    fs::write(&tmp, set_keys(&text, section, keys)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// An on/off key, where anything that isn't `0` or `1` leaves the default alone.
pub fn on(v: Option<&str>, default: bool) -> bool {
    match v {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the key the caller touched is written: everything else in the file decides itself.
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

    /// The game writes `default.ini` with a key per session type. Selecting a setup for one
    /// must not touch what the rider picked for the others.
    #[test]
    fn a_key_per_session_type_leaves_the_others_alone() {
        let text = "[setup]\ntesting=mine\nrace=my race setup\n";
        let out = set_keys(text, "setup", &[("testing", "Coach indiana".into())]);
        let pairs = read_section(&out, "setup");
        assert_eq!(get(&pairs, "testing"), Some("Coach indiana"));
        assert_eq!(get(&pairs, "race"), Some("my race setup"), "the race setup is the rider's");
    }

    #[test]
    fn an_unreadable_value_keeps_the_default() {
        assert!(on(None, true));
        assert!(!on(None, false));
        assert!(on(Some("1"), false));
        assert!(!on(Some("0"), true));
        assert!(on(Some("yes"), true), "not 0 or 1: the default stands");
        assert!(!on(Some("yes"), false));
    }

    #[test]
    fn the_file_is_written_aside_and_moved_in() {
        let dir = std::env::temp_dir().join(format!("coach-ini-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("cues").join("voice.ini");
        write(&path, "voice", &[("enabled", "1".into()), ("volume", "55".into())]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[voice]\nenabled=1\nvolume=55\n");
        assert!(!path.with_extension("ini.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
