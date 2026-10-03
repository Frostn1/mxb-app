//! Warn when MX Bikes is pointed at a profile it can't load.
//!
//! `global.ini` keeps `[profile] lastprofile=<folder name>`. The game names a profile's folder
//! after the profile, spaces turned into `_`. If that name holds a character Windows won't allow
//! in a folder name (a profile called "Frost | mxbsecure" becomes `Frost_|_mxbsecure`), the folder
//! can never be created, and the game quietly falls back to a blank `unnamedProfile`: default
//! bike, number, setup and kit. It looks like the whole setup was wiped.
//!
//! The nickname (`nickname=`) is a display name and is not a folder, so it is not checked.
//!
//! Read-only: this only reports. It never edits `global.ini`.

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Event carrying the current problem (`null` when there is none).
pub const EVENT: &str = "profile-problem";

/// What the game's own fallback profile is called. Never offered as a suggestion.
const BLANK_PROFILE: &str = "unnamedProfile";

/// Characters Windows forbids in a file or folder name.
const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// A few names are listed in the message; more than this and the list is just noise.
const MAX_LISTED: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The profile name can't be a Windows folder name.
    IllegalName,
    /// The profile folder isn't there.
    MissingFolder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileProblem {
    pub kind: Kind,
    /// `lastprofile` as the game wrote it.
    pub profile: String,
    /// The forbidden characters found, de-duplicated, in order of appearance.
    pub bad_chars: Vec<String>,
    /// Real profile folders the rider could pick instead; empty when there are too many to list.
    pub existing: Vec<String>,
}

/// Tauri-managed latest result, so a window opened after the check can still ask for it.
#[derive(Default)]
pub struct ProfileCheckState(pub Mutex<Option<ProfileProblem>>);

/// Forbidden characters in `name`: the set above, control characters, and a trailing dot or
/// space (Windows strips those, so the folder would not match the name).
pub fn illegal_chars(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    for c in name.chars() {
        if ILLEGAL.contains(&c) {
            push(c.to_string());
        } else if c.is_control() {
            push(format!("U+{:04X}", c as u32));
        }
    }
    if name.ends_with('.') {
        push(".".into());
    } else if name.ends_with(' ') {
        push("trailing space".into());
    }
    out
}

/// `[profile] lastprofile=` from `global.ini` text (already decoded by the caller).
pub fn last_profile(text: &str) -> Option<String> {
    let mut in_profile = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_profile = line[1..line.len() - 1].trim().eq_ignore_ascii_case("profile");
            continue;
        }
        if !in_profile {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case("lastprofile") {
                return Some(v.trim_start().to_string());
            }
        }
    }
    None
}

fn existing_profiles(profiles_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(profiles_dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// The pure check: `global.ini` text against the folders actually present.
pub fn check_text(global_ini: &str, profiles_dir: &Path) -> Option<ProfileProblem> {
    let profile = last_profile(global_ini)?;
    if profile.trim().is_empty() {
        return None;
    }
    let folders = existing_profiles(profiles_dir);
    let suggestions = |folders: &[String]| -> Vec<String> {
        let real: Vec<String> = folders
            .iter()
            .filter(|n| !n.eq_ignore_ascii_case(BLANK_PROFILE))
            .cloned()
            .collect();
        if real.len() <= MAX_LISTED {
            real
        } else {
            Vec::new()
        }
    };

    let bad = illegal_chars(&profile);
    if !bad.is_empty() {
        return Some(ProfileProblem {
            kind: Kind::IllegalName,
            profile,
            bad_chars: bad,
            existing: suggestions(&folders),
        });
    }
    // The game's own fallback is a real folder and not a mistake to report.
    if profile.eq_ignore_ascii_case(BLANK_PROFILE) {
        return None;
    }
    // A profiles folder that can't be read at all says nothing about the profile: stay quiet
    // rather than blame a rider whose folder simply isn't set up yet.
    if !profiles_dir.is_dir() {
        return None;
    }
    if folders.iter().any(|n| n.eq_ignore_ascii_case(&profile)) {
        return None;
    }
    Some(ProfileProblem {
        kind: Kind::MissingFolder,
        profile,
        bad_chars: Vec::new(),
        existing: suggestions(&folders),
    })
}

/// `global.ini` sits beside the `profiles` folder.
pub fn global_ini_path(profiles_dir: &Path) -> Option<std::path::PathBuf> {
    profiles_dir.parent().map(|p| p.join("global.ini"))
}

/// Is this path the `global.ini` we care about?
pub fn is_global_ini(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("global.ini"))
        .unwrap_or(false)
}

pub fn check(profiles_dir: &Path) -> Option<ProfileProblem> {
    let path = global_ini_path(profiles_dir)?;
    let bytes = std::fs::read(&path).ok()?;
    let (text, _) = crate::presets::decode_ini(&bytes);
    check_text(&text, profiles_dir)
}

/// Re-run the check, remember it, and tell the UI if it changed.
pub fn refresh(app: &AppHandle, profiles_dir: &Path) {
    let problem = check(profiles_dir);
    let state = app.state::<ProfileCheckState>();
    {
        let mut cur = state.0.lock().unwrap();
        if *cur == problem {
            return;
        }
        *cur = problem.clone();
    }
    if let Some(p) = &problem {
        log::warn!("profile check: {:?} for profile {:?}", p.kind, p.profile);
    }
    if let Err(e) = app.emit(EVENT, problem) {
        log::warn!("profile check: couldn't tell the UI: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str, folders: &[&str]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("profilecheck-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for f in folders {
            std::fs::create_dir_all(d.join(f)).unwrap();
        }
        d
    }

    fn ini(profile: &str) -> String {
        format!("[profile]\nlastprofile={profile}\nnickname=Rider | Name\n")
    }

    #[test]
    fn pipe_in_profile_name_is_illegal() {
        let d = dir("pipe", &["Rider", "unnamedProfile"]);
        let p = check_text(&ini("Rider_|_Name"), &d).unwrap();
        assert_eq!(p.kind, Kind::IllegalName);
        assert_eq!(p.profile, "Rider_|_Name");
        assert_eq!(p.bad_chars, vec!["|"]);
        assert_eq!(p.existing, vec!["Rider"]);
    }

    #[test]
    fn nickname_alone_is_never_flagged() {
        let d = dir("nick", &["Rider"]);
        assert_eq!(check_text(&ini("Rider"), &d), None);
    }

    #[test]
    fn missing_folder_is_reported() {
        let d = dir("missing", &["Rider"]);
        let p = check_text(&ini("Ghost"), &d).unwrap();
        assert_eq!(p.kind, Kind::MissingFolder);
        assert_eq!(p.existing, vec!["Rider"]);
    }

    #[test]
    fn existing_folder_matches_any_case() {
        let d = dir("case", &["Rider"]);
        assert_eq!(check_text(&ini("rider"), &d), None);
    }

    #[test]
    fn blank_profile_and_empty_value_are_fine() {
        let d = dir("blank", &["unnamedProfile"]);
        assert_eq!(check_text(&ini("unnamedProfile"), &d), None);
        assert_eq!(check_text(&ini(""), &d), None);
        assert_eq!(check_text("[other]\nx=1\n", &d), None);
    }

    #[test]
    fn many_profiles_are_not_listed() {
        let d = dir("many", &["a", "b", "c", "d", "e"]);
        assert!(check_text(&ini("Ghost"), &d).unwrap().existing.is_empty());
    }

    #[test]
    fn unreadable_profiles_folder_stays_quiet() {
        let d = std::env::temp_dir().join("profilecheck-nonexistent-xyz");
        assert_eq!(check_text(&ini("Ghost"), &d), None);
    }

    #[test]
    fn illegal_char_rules() {
        assert_eq!(
            illegal_chars("A<B>C:D\"E/F\\G|H?I*J"),
            vec!["<", ">", ":", "\"", "/", "\\", "|", "?", "*"]
        );
        assert_eq!(illegal_chars("a|b|c"), vec!["|"]);
        assert_eq!(illegal_chars("tab\there"), vec!["U+0009"]);
        assert_eq!(illegal_chars("dot."), vec!["."]);
        assert_eq!(illegal_chars("space "), vec!["trailing space"]);
        assert!(illegal_chars("Fine_Name-1").is_empty());
    }

    #[test]
    fn reads_lastprofile_only_from_profile_section() {
        assert_eq!(
            last_profile("[x]\nlastprofile=No\n[Profile]\nLastProfile = Yes\r\n"),
            Some("Yes".into())
        );
        assert_eq!(last_profile("[x]\nlastprofile=No\n"), None);
    }

    #[test]
    fn global_ini_sits_beside_profiles() {
        let p = global_ini_path(Path::new("/mx/profiles")).unwrap();
        assert!(is_global_ini(&p));
        assert_eq!(p, Path::new("/mx").join("global.ini"));
    }
}
