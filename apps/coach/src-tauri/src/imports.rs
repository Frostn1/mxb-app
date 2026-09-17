//! Trainer laps: recordings from another rider, kept apart from the rider's own sessions.
//!
//! A `.mxbc` the rider drops in is copied into `<data dir>/coach/imported/`, never into the
//! game's own sessions folder. Nothing somebody else sends can then turn up among their laps,
//! their bests or their ideal lap: an imported recording is only ever something to be reviewed
//! against, and the review says whose it is. Everything downstream takes one by its path like
//! any other recording.

use std::fs;
use std::path::{Path, PathBuf};

use mxb_core::config;
use serde::Serialize;
use tauri::AppHandle;

use crate::coach::{group_sessions, summaries, SessionSummary};
use crate::telemetry;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Where imported recordings are kept: under the coach's own data, not the game's folder.
pub(crate) fn dir(app: &AppHandle) -> Option<PathBuf> {
    Some(config::data_dir(app)?.join("coach").join("imported"))
}

/// Their summaries, cached the way the rider's own sessions are, and separately.
fn index_path(app: &AppHandle) -> Option<PathBuf> {
    Some(config::data_dir(app)?.join("coach").join("imported-v1.json"))
}

/// Whether a recording is one of the imported ones rather than one of the rider's own.
pub(crate) fn is_import(app: &AppHandle, path: &str) -> bool {
    dir(app).is_some_and(|d| Path::new(path).starts_with(d))
}

/// Every imported recording, newest first.
#[tauri::command]
pub fn coach_imports(app: AppHandle) -> Vec<SessionSummary> {
    let Some(d) = dir(&app) else { return Vec::new() };
    let mut out = group_sessions(summaries(&[d], index_path(&app)));
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out
}

/// A file that couldn't be imported, and why, in words the rider can do something about.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub file: String,
    pub why: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    /// How many recordings went in.
    pub added: i32,
    pub skipped: Vec<Skipped>,
}

/// What a picked path stands for: the file itself, or every recording in a folder.
fn recordings(p: &Path) -> Vec<PathBuf> {
    if !p.is_dir() {
        return vec![p.to_path_buf()];
    }
    let Ok(entries) = fs::read_dir(p) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|f| f.extension().and_then(|x| x.to_str()) == Some("mxbc"))
        .collect();
    out.sort();
    out
}

/// The name the copy takes. The recorder names a file after the moment it started and the
/// session list reads the date back out of that name, so the stamp is kept and only made
/// unique — two riders can be out at the same second.
fn free_name(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.mxbc"));
    if !first.exists() {
        return first;
    }
    (2..1000).map(|n| dir.join(format!("{stem}-{n}.mxbc"))).find(|p| !p.exists()).unwrap_or(first)
}

/// The same recording again: the recorder's stamp is the file's name, and the size settles it.
fn already_here(dir: &Path, stem: &str, size: u64) -> bool {
    let Ok(entries) = fs::read_dir(dir) else { return false };
    entries.flatten().any(|e| {
        let same_stamp = e
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s == stem || s.strip_prefix(stem).is_some_and(|rest| rest.starts_with('-')));
        same_stamp && e.metadata().is_ok_and(|m| m.len() == size)
    })
}

/// Copies recordings in: the files picked, or every recording in a folder picked.
#[tauri::command]
pub fn coach_import_laps(app: AppHandle, paths: Vec<String>) -> Result<Imported, String> {
    let target = dir(&app).ok_or("The coach has nowhere to keep imported laps.")?;
    fs::create_dir_all(&target).map_err(err)?;
    let mut added = 0;
    let mut skipped: Vec<Skipped> = Vec::new();
    for file in paths.iter().map(Path::new).flat_map(recordings) {
        let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut skip = |why: &str| skipped.push(Skipped { file: name.clone(), why: why.to_string() });
        let Ok(bytes) = fs::read(&file) else {
            skip("It couldn't be read.");
            continue;
        };
        let Ok(rec) = telemetry::parse(&bytes) else {
            skip("That isn't a recording the coach can read. It wants a .mxbc file the recorder wrote.");
            continue;
        };
        if !rec.laps().iter().any(|l| l.whole && !l.invalid) {
            skip("There's no whole, valid lap in it, so there's nothing to compare with.");
            continue;
        }
        let stem = file.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        if already_here(&target, &stem, bytes.len() as u64) {
            skip("You've already imported this one.");
            continue;
        }
        if let Err(e) = fs::write(free_name(&target, &stem), &bytes) {
            skip(&format!("It couldn't be copied in: {e}"));
            continue;
        }
        added += 1;
    }
    Ok(Imported { added, skipped })
}

/// Removes an imported lap. Only ever one of those: nothing here can reach the rider's own
/// recordings.
#[tauri::command]
pub fn coach_remove_import(app: AppHandle, path: String) -> Result<(), String> {
    if !is_import(&app, &path) {
        return Err("That isn't an imported lap.".into());
    }
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(err(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("coach-import-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_copy_keeps_the_recorders_stamp_and_is_only_made_unique() {
        let d = tmp("name");
        let stem = "20260915-100000-000";
        assert_eq!(free_name(&d, stem), d.join("20260915-100000-000.mxbc"));
        fs::write(d.join(format!("{stem}.mxbc")), b"a").unwrap();
        assert_eq!(free_name(&d, stem), d.join("20260915-100000-000-2.mxbc"), "two riders can be out at the same second");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_same_recording_twice_is_seen_by_its_stamp_and_its_size() {
        let d = tmp("dupe");
        let stem = "20260915-100000-000";
        assert!(!already_here(&d, stem, 3));
        fs::write(d.join(format!("{stem}.mxbc")), b"abc").unwrap();
        assert!(already_here(&d, stem, 3));
        assert!(!already_here(&d, stem, 4), "somebody else's lap of the same second is not the same file");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_stands_for_the_recordings_in_it() {
        let d = tmp("folder");
        fs::write(d.join("a.mxbc"), b"a").unwrap();
        fs::write(d.join("notes.txt"), b"a").unwrap();
        assert_eq!(recordings(&d), vec![d.join("a.mxbc")], "only the recordings");
        assert_eq!(recordings(&d.join("a.mxbc")), vec![d.join("a.mxbc")], "a file stands for itself");
        let _ = fs::remove_dir_all(&d);
    }
}
