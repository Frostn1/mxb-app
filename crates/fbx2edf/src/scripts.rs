//! Export scripts the Convert screen keeps between runs: the last one used, and a short list
//! saved by name. Kept as one small JSON file in the app's own config folder, never beside
//! the modder's files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The most scripts kept in the saved list.
pub const MAX_SAVED: usize = 20;
/// The longest script kept, in bytes: export scripts are a few lines.
pub const MAX_BYTES: usize = 256 * 1024;
const MAX_NAME: usize = 60;
const FILE: &str = "export-scripts.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Script {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scripts {
    /// The script used last, so the next start has it in its slot.
    pub last: Option<Script>,
    /// Saved by name, newest first.
    pub saved: Vec<Script>,
}

fn file(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// A name as kept: trimmed, control characters out, cut to a sensible length.
pub fn clean_name(raw: &str) -> String {
    let n: String = raw.chars().filter(|c| !c.is_control()).collect();
    n.trim().chars().take(MAX_NAME).collect()
}

/// What is stored in `dir`. Nothing, or a file that can't be read, is an empty store.
pub fn load(dir: &Path) -> Scripts {
    std::fs::read_to_string(file(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn store(dir: &Path, scripts: &Scripts) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    let json = serde_json::to_string_pretty(scripts).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file(dir)).map_err(|e| e.to_string())
}

fn check(text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err("That script is too large to keep.".into());
    }
    Ok(())
}

/// Remember `text` as the last script used; empty text forgets it.
pub fn set_last(dir: &Path, name: &str, text: &str) -> Result<Scripts, String> {
    check(text)?;
    let mut s = load(dir);
    s.last = (!text.trim().is_empty()).then(|| Script { name: clean_name(name), text: text.to_string() });
    store(dir, &s)?;
    Ok(s)
}

/// Save `text` under `name` (a script already saved by that name, in any case, is replaced),
/// newest first, at most [`MAX_SAVED`]. A blank name is "Script".
pub fn save(dir: &Path, name: &str, text: &str) -> Result<Scripts, String> {
    check(text)?;
    if text.trim().is_empty() {
        return Err("There is no script to save.".into());
    }
    let mut name = clean_name(name);
    if name.is_empty() {
        name = "Script".into();
    }
    let mut s = load(dir);
    s.saved.retain(|x| !x.name.eq_ignore_ascii_case(&name));
    s.saved.insert(0, Script { name, text: text.to_string() });
    s.saved.truncate(MAX_SAVED);
    store(dir, &s)?;
    Ok(s)
}

/// Take the script saved as `name` out of the list.
pub fn remove(dir: &Path, name: &str) -> Result<Scripts, String> {
    let mut s = load(dir);
    s.saved.retain(|x| !x.name.eq_ignore_ascii_case(name.trim()));
    store(dir, &s)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "[Hierarchy]\nsteer_rot = -90 180 0";

    #[test]
    fn nothing_stored_is_an_empty_store_and_a_broken_file_too() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load(d.path()), Scripts::default());
        std::fs::write(d.path().join(FILE), "{ not json").unwrap();
        assert_eq!(load(d.path()), Scripts::default());
    }

    #[test]
    fn the_last_script_is_remembered_across_loads_and_cleared_by_empty_text() {
        let d = tempfile::tempdir().unwrap();
        set_last(d.path(), "bike.ini", TEXT).unwrap();
        assert_eq!(load(d.path()).last, Some(Script { name: "bike.ini".into(), text: TEXT.into() }));
        set_last(d.path(), "", "  \n").unwrap();
        assert_eq!(load(d.path()).last, None);
    }

    #[test]
    fn saving_keeps_newest_first_replaces_a_name_and_caps_the_list() {
        let d = tempfile::tempdir().unwrap();
        save(d.path(), "a", "[params]\nx=1").unwrap();
        save(d.path(), " B ", TEXT).unwrap();
        let s = save(d.path(), "A", "[params]\nx=2").unwrap();
        let names: Vec<&str> = s.saved.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(s.saved[0].text, "[params]\nx=2");
        for i in 0..MAX_SAVED + 5 {
            save(d.path(), &format!("s{i}"), TEXT).unwrap();
        }
        let s = load(d.path());
        assert_eq!(s.saved.len(), MAX_SAVED);
        assert_eq!(s.saved[0].name, format!("s{}", MAX_SAVED + 4));
    }

    #[test]
    fn a_blank_name_gets_a_default_and_empty_or_huge_scripts_are_refused() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(save(d.path(), "  ", TEXT).unwrap().saved[0].name, "Script");
        assert!(save(d.path(), "x", "   ").is_err());
        assert!(save(d.path(), "x", &"a".repeat(MAX_BYTES + 1)).is_err());
        assert!(set_last(d.path(), "x", &"a".repeat(MAX_BYTES + 1)).is_err());
        assert_eq!(clean_name("a\u{7}b\n"), "ab");
        assert_eq!(clean_name(&"n".repeat(100)).len(), MAX_NAME);
    }

    #[test]
    fn removing_takes_a_script_out_by_name_and_keeps_the_rest() {
        let d = tempfile::tempdir().unwrap();
        save(d.path(), "a", TEXT).unwrap();
        save(d.path(), "b", TEXT).unwrap();
        let s = remove(d.path(), "A").unwrap();
        assert_eq!(s.saved.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["b"]);
        assert_eq!(remove(d.path(), "missing").unwrap().saved.len(), 1);
    }
}
