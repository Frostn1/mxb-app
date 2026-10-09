//! Convert: FBX files to the `.edf` MX Bikes loads, on this machine, a batch at a time.
//!
//! The converter is the `fbx2edf` crate's, the one mxbsecure.com/convert runs in the browser,
//! built natively here. When this build doesn't carry it, `fbx_convert_available` says so and
//! the tab isn't offered.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use fbx2edf::host::{self, Event, Options};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;

/// One file an added path turned out to hold.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: String,
    /// "model", "shadow" (an FBX named like a bike's shadow) or "params" (an `.ini`).
    pub kind: &'static str,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct Pair {
    pub input: String,
    pub output: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub report: Option<Value>,
    pub error: Option<String>,
    /// The `.hrc` files written beside a bike's `.edf`.
    pub hrc: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    index: usize,
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent<'a> {
    index: usize,
    outcome: &'a Outcome,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn fbx_convert_available() -> bool {
    host::available()
}

fn kind_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "fbx" if name.contains("shadow") => Some("shadow"),
        "fbx" => Some("model"),
        "ini" => Some("params"),
        _ => None,
    }
}

/// What files and folders hold: their FBX files and parameter files, a folder searched all
/// the way down. Each once, in order.
pub fn scan(paths: &[String]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |p: &Path, out: &mut Vec<Found>| {
        let Some(kind) = kind_of(p) else { return };
        if !seen.insert(p.to_string_lossy().to_lowercase()) {
            return;
        }
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        out.push(Found { path: p.to_string_lossy().into_owned(), kind, size });
    };
    for raw in paths {
        let p = Path::new(raw);
        if p.is_dir() {
            let mut files: Vec<PathBuf> = walkdir::WalkDir::new(p)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .collect();
            files.sort();
            for f in &files {
                push(f, &mut out);
            }
        } else if p.is_file() {
            push(p, &mut out);
        }
    }
    out
}

#[tauri::command]
pub fn fbx_scan(paths: Vec<String>) -> Vec<Found> {
    scan(&paths)
}

/// The objects an FBX makes as bike parts, for telling a bike from a single model.
#[tauri::command]
pub async fn fbx_part_names(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || host::part_names(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// A parameter file or export script, as text. Small files only: these are a few lines.
#[tauri::command]
pub fn fbx_read_text(path: String) -> Result<String, String> {
    let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if len > 1 << 20 {
        return Err(format!("{path} is too large to be a parameter file"));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
}

/// The `.hrc` files a converted bike needs beside its `.edf`, in the game's own format, as the
/// browser converter writes them: one per bike part the model has, each naming the `.edf` by
/// its bare file name. `chassis` is the main object, so it goes unnamed. Empty when the model
/// has no steering or suspension part.
pub fn hrc_files(object_names: &[String], edf_name: &str) -> Vec<(String, String)> {
    let names: Vec<String> = object_names.iter().map(|n| n.to_lowercase()).collect();
    let has = |n: &str| names.iter().any(|x| x == n);
    if !names.iter().any(|n| ["steer", "fsusp", "rsusp", "steera", "fsuspa", "rsuspa"].contains(&n.as_str())) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for part in ["chassis", "steer", "fsusp", "rsusp"] {
        let object = if part == "chassis" {
            has("").then(String::new)
        } else if has(part) {
            Some(part.to_string())
        } else if has(&format!("{part}a")) {
            Some(format!("{part}a"))
        } else {
            None
        };
        let Some(object) = object else { continue };
        let mut body = vec![format!("\tscene = {edf_name}")];
        if !object.is_empty() {
            body.push(format!("\tname = {object}"));
        }
        body.push("\tswitch = 0".into());
        let mut lines = vec!["level0".to_string(), "{".into()];
        lines.extend(body);
        lines.extend(["}".to_string(), String::new()]);
        out.push((format!("{part}.hrc"), lines.join("\r\n")));
    }
    out
}

/// Write a bike's `.hrc` files beside its `.edf`, leaving any that are already there alone:
/// a modder's own are often edited by hand.
fn write_hrc(report: &Value, edf: &Path) -> Vec<String> {
    let names: Vec<String> = report["objects"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| o["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let edf_name = edf.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = edf.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut written = Vec::new();
    for (name, text) in hrc_files(&names, &edf_name) {
        let p = dir.join(&name);
        if p.exists() {
            continue;
        }
        match std::fs::write(&p, text) {
            Ok(()) => written.push(name),
            Err(e) => log::warn!("[convert] couldn't write {}: {e}", p.display()),
        }
    }
    written
}

/// Convert a batch. Each pair's progress and outcome is sent as it happens (`fbx-convert-progress`,
/// `fbx-convert-done`), and every outcome is returned at the end, in order.
#[tauri::command]
pub async fn fbx_convert(app: tauri::AppHandle, pairs: Vec<Pair>, options: Options, hrc: bool) -> Result<Vec<Outcome>, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A batch is already converting.".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|p| (PathBuf::from(p.input), PathBuf::from(p.output))).collect();
        let started = std::time::Instant::now();
        let mut outcomes: Vec<Option<Outcome>> = (0..pairs.len()).map(|_| None).collect();
        let results = host::convert(&pairs, &options, &mut |event| match event {
            Event::Pictures { index, done, total } => {
                let _ = app.emit("fbx-convert-progress", ProgressEvent { index, done, total });
            }
            Event::Finished { index, result } => {
                let outcome = outcome_of(result, hrc, &pairs[index].1);
                let _ = app.emit("fbx-convert-done", DoneEvent { index, outcome: &outcome });
                outcomes[index] = Some(outcome);
            }
        })?;
        log::info!("[convert] {} files in {} ms", pairs.len(), started.elapsed().as_millis());
        Ok(results
            .into_iter()
            .zip(outcomes)
            .enumerate()
            .map(|(i, (r, o))| o.unwrap_or_else(|| outcome_of(r, hrc, &pairs[i].1)))
            .collect())
    })
    .await
    .map_err(|e| e.to_string());
    RUNNING.store(false, Ordering::SeqCst);
    result?
}

fn outcome_of(result: Result<Value, String>, hrc: bool, edf: &Path) -> Outcome {
    match result {
        Ok(report) => {
            let hrc = if hrc { write_hrc(&report, edf) } else { Vec::new() };
            Outcome { report: Some(report), error: None, hrc }
        }
        Err(e) => Outcome { error: Some(e), ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_gives_its_fbx_and_ini_files_once() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("sub")).unwrap();
        for f in ["bike.FBX", "Shadow_bike.fbx", "sub/helmet.fbx", "params.ini", "paint.png", "bike.edf"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let root = d.to_string_lossy().into_owned();
        let one = d.join("bike.FBX").to_string_lossy().into_owned();
        let found = scan(&[root, one]);
        let kinds: Vec<(String, &str)> = found
            .iter()
            .map(|f| (Path::new(&f.path).file_name().unwrap().to_string_lossy().into_owned(), f.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("Shadow_bike.fbx".to_string(), "shadow"),
                ("bike.FBX".to_string(), "model"),
                ("params.ini".to_string(), "params"),
                ("helmet.fbx".to_string(), "model"),
            ]
        );
    }

    #[test]
    fn a_bike_gets_the_browser_converter_s_hrc_files() {
        let names: Vec<String> = ["", "steer", "fsuspa", "rsusp"].iter().map(|s| s.to_string()).collect();
        let files = hrc_files(&names, "ktm.edf");
        let got: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(got, ["chassis.hrc", "steer.hrc", "fsusp.hrc", "rsusp.hrc"]);
        assert_eq!(files[0].1, "level0\r\n{\r\n\tscene = ktm.edf\r\n\tswitch = 0\r\n}\r\n");
        assert_eq!(files[2].1, "level0\r\n{\r\n\tscene = ktm.edf\r\n\tname = fsuspa\r\n\tswitch = 0\r\n}\r\n");
        assert!(hrc_files(&["".to_string()], "helmet.edf").is_empty(), "a single model has none");
    }

    #[test]
    fn hrc_files_already_there_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("ktm.edf");
        std::fs::write(dir.path().join("steer.hrc"), "mine").unwrap();
        let report = serde_json::json!({ "objects": [{ "name": "" }, { "name": "steer" }] });
        assert_eq!(write_hrc(&report, &edf), vec!["chassis.hrc".to_string()]);
        assert_eq!(std::fs::read_to_string(dir.path().join("steer.hrc")).unwrap(), "mine");
    }
}
