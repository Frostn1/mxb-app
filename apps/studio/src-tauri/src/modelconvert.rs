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
    /// The `.shd` files written for its textures, each with the maps it names:
    /// `bike.shd: normal bike_n, reflection bike_r`.
    pub shd: Vec<String>,
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

/// The maps found beside a colour picture, by file name.
#[derive(Debug, Default, PartialEq)]
pub struct Maps {
    pub normal: Option<String>,
    pub reflection: Option<String>,
    pub specular: Option<String>,
}

impl Maps {
    fn any(&self) -> bool {
        self.normal.is_some() || self.reflection.is_some() || self.specular.is_some()
    }
}

const NORMAL: &[&str] = &["_n", "_normal", "_nrm"];
const REFLECTION: &[&str] = &["_r", "_refl", "_reflection"];
const SPECULAR: &[&str] = &["_s", "_spec"];
/// What the converter reads, the colour picture's own extension tried first.
const EXTS: &[&str] = &["tga", "png", "bmp", "jpg", "jpeg"];

/// `<name><suffix>.<ext>` in `dir` for each kind of map, matched without regard to case.
pub fn find_maps(dir: &Path, name: &str, ext: Option<&str>) -> Maps {
    let Ok(entries) = std::fs::read_dir(dir) else { return Maps::default() };
    let present: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut exts: Vec<String> = ext.map(|e| e.to_ascii_lowercase()).into_iter().collect();
    exts.extend(EXTS.iter().map(|e| e.to_string()).filter(|e| Some(e.as_str()) != ext));
    let find = |suffixes: &[&str]| {
        suffixes.iter().find_map(|s| {
            exts.iter().find_map(|e| {
                let want = format!("{name}{s}.{e}").to_lowercase();
                present.iter().find(|p| p.to_lowercase() == want).cloned()
            })
        })
    };
    Maps { normal: find(NORMAL), reflection: find(REFLECTION), specular: find(SPECULAR) }
}

/// A `.shd` naming `maps`, laid out the way `fbx2edf.exe` reads it: each block's name, braces
/// and keys on lines of their own (it skips a block written on one line). The numbers are the
/// stock bikes' paint: shininess 30, reflection 0 to 0.6 with exponent 1.5. With no specular
/// map, the specular mask is the normal map's alpha.
pub fn shd_text(maps: &Maps) -> String {
    let mut out = String::new();
    let mut block = |name: &str, lines: Vec<String>| {
        out.push_str(&format!("{name}\r\n{{\r\n"));
        for l in lines {
            out.push_str(&format!("\t{l}\r\n"));
        }
        out.push_str("}\r\n");
    };
    if maps.normal.is_some() || maps.specular.is_some() {
        let mut l = vec!["shininess = 30".to_string()];
        l.extend(maps.specular.iter().map(|m| format!("map = {m}")));
        block("specular", l);
    }
    if let Some(m) = &maps.reflection {
        block("reflection", vec!["factormin = 0".into(), "factormax = 0.6".into(), "factorexp = 1.5".into(), format!("map = {m}")]);
    }
    if let Some(m) = &maps.normal {
        block("bump", vec![format!("map = {m}"), "repetitions = 1".into()]);
    }
    out
}

/// `bike.shd: normal bike_n, reflection bike_r`.
fn shd_summary(file: &str, maps: &Maps) -> String {
    let stem = |m: &String| Path::new(m).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let parts: Vec<String> = [("normal", &maps.normal), ("reflection", &maps.reflection), ("specular", &maps.specular)]
        .into_iter()
        .filter_map(|(k, m)| m.as_ref().map(|m| format!("{k} {}", stem(m))))
        .collect();
    format!("{file}: {}", parts.join(", "))
}

/// The `.shd` to write for one picture, if any: in the first of its folders that has a map,
/// since the converter reads the maps a `.shd` names beside it. A `.shd` the converter already
/// reads is left alone unless `overwrite`. Path, text, summary.
pub fn plan_shd(site: &host::ShdSite, overwrite: bool) -> Option<(PathBuf, String, String)> {
    if site.shd.is_some() && !overwrite {
        return None;
    }
    let (dir, maps) = site.dirs.iter().map(|d| (d, find_maps(d, &site.name, site.ext.as_deref()))).find(|(_, m)| m.any())?;
    let file = format!("{}.shd", site.name);
    let path = dir.join(&file);
    if path.exists() && !overwrite {
        return None;
    }
    Some((path, shd_text(&maps), shd_summary(&file, &maps)))
}

/// Write the `.shd` files an FBX's textures lack, before it is converted, so the converter
/// builds their maps in. `done` holds the files written so far in the batch: a texture two
/// models share is written once.
fn write_shd(fbx: &Path, overwrite: bool, done: &mut std::collections::HashSet<PathBuf>) -> Vec<String> {
    let sites = match host::shd_sites(fbx) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[convert] no .shd files for {}: {e}", fbx.display());
            return Vec::new();
        }
    };
    let mut written = Vec::new();
    for site in &sites {
        let Some((path, text, summary)) = plan_shd(site, overwrite) else { continue };
        if !done.insert(path.clone()) {
            continue;
        }
        match std::fs::write(&path, text) {
            Ok(()) => written.push(summary),
            Err(e) => log::warn!("[convert] couldn't write {}: {e}", path.display()),
        }
    }
    written
}

/// Convert a batch. Each pair's progress and outcome is sent as it happens (`fbx-convert-progress`,
/// `fbx-convert-done`), and every outcome is returned at the end, in order. With `shd`, the
/// `.shd` files its textures lack are written first (see [`write_shd`]).
#[tauri::command]
pub async fn fbx_convert(
    app: tauri::AppHandle,
    pairs: Vec<Pair>,
    options: Options,
    hrc: bool,
    shd: Option<bool>,
    overwrite_shd: Option<bool>,
) -> Result<Vec<Outcome>, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A batch is already converting.".into());
    }
    let (shd, overwrite_shd) = (shd.unwrap_or(false), overwrite_shd.unwrap_or(false));
    let result = tauri::async_runtime::spawn_blocking(move || {
        let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|p| (PathBuf::from(p.input), PathBuf::from(p.output))).collect();
        let started = std::time::Instant::now();
        let mut done = std::collections::HashSet::new();
        let mut shd_files: Vec<Vec<String>> = pairs
            .iter()
            .map(|(fbx, _)| if shd { write_shd(fbx, overwrite_shd, &mut done) } else { Vec::new() })
            .collect();
        let mut outcomes: Vec<Option<Outcome>> = (0..pairs.len()).map(|_| None).collect();
        let results = host::convert(&pairs, &options, &mut |event| match event {
            Event::Pictures { index, done, total } => {
                let _ = app.emit("fbx-convert-progress", ProgressEvent { index, done, total });
            }
            Event::Finished { index, result } => {
                let outcome = outcome_of(result, hrc, &pairs[index].1, std::mem::take(&mut shd_files[index]));
                let _ = app.emit("fbx-convert-done", DoneEvent { index, outcome: &outcome });
                outcomes[index] = Some(outcome);
            }
        })?;
        log::info!("[convert] {} files in {} ms", pairs.len(), started.elapsed().as_millis());
        Ok(results
            .into_iter()
            .zip(outcomes)
            .enumerate()
            .map(|(i, (r, o))| o.unwrap_or_else(|| outcome_of(r, hrc, &pairs[i].1, Vec::new())))
            .collect())
    })
    .await
    .map_err(|e| e.to_string());
    RUNNING.store(false, Ordering::SeqCst);
    result?
}

fn outcome_of(result: Result<Value, String>, hrc: bool, edf: &Path, shd: Vec<String>) -> Outcome {
    match result {
        Ok(report) => {
            let hrc = if hrc { write_hrc(&report, edf) } else { Vec::new() };
            Outcome { report: Some(report), error: None, hrc, shd }
        }
        Err(e) => Outcome { error: Some(e), shd, ..Default::default() },
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

    fn site(name: &str, dirs: &[&Path], shd: Option<PathBuf>) -> host::ShdSite {
        host::ShdSite { name: name.into(), ext: Some("tga".into()), dirs: dirs.iter().map(|d| d.to_path_buf()).collect(), shd }
    }

    #[test]
    fn maps_are_found_by_their_suffix_whatever_the_case() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["bike.tga", "Bike_N.TGA", "bike_r.png", "bike_spec.tga", "other_n.tga"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let m = find_maps(dir.path(), "bike", Some("tga"));
        assert_eq!(m.normal.as_deref(), Some("Bike_N.TGA"));
        assert_eq!(m.reflection.as_deref(), Some("bike_r.png"));
        assert_eq!(m.specular.as_deref(), Some("bike_spec.tga"));
        assert!(!find_maps(dir.path(), "frame", Some("tga")).any());
    }

    #[test]
    fn a_shd_is_written_the_way_fbx2edf_exe_reads_it() {
        let maps = Maps { normal: Some("bike_n.tga".into()), reflection: Some("bike_r.tga".into()), specular: None };
        assert_eq!(
            shd_text(&maps),
            "specular\r\n{\r\n\tshininess = 30\r\n}\r\n\
             reflection\r\n{\r\n\tfactormin = 0\r\n\tfactormax = 0.6\r\n\tfactorexp = 1.5\r\n\tmap = bike_r.tga\r\n}\r\n\
             bump\r\n{\r\n\tmap = bike_n.tga\r\n\trepetitions = 1\r\n}\r\n"
        );
        let spec = Maps { specular: Some("bike_s.tga".into()), ..Maps::default() };
        assert_eq!(shd_text(&spec), "specular\r\n{\r\n\tshininess = 30\r\n\tmap = bike_s.tga\r\n}\r\n");
        assert_eq!(shd_summary("bike.shd", &maps), "bike.shd: normal bike_n, reflection bike_r");
    }

    #[test]
    fn a_shd_goes_where_the_maps_are_and_never_over_one_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (fbm, root) = (dir.path().join("bike.fbm"), dir.path().to_path_buf());
        std::fs::create_dir(&fbm).unwrap();
        std::fs::write(root.join("bike_n.tga"), b"x").unwrap();
        // Embedded paint: the .fbm folder has no maps, the FBX's folder has.
        let (path, text, summary) = plan_shd(&site("bike", &[&fbm, &root], None), false).unwrap();
        assert_eq!(path, root.join("bike.shd"));
        assert!(text.contains("map = bike_n.tga"));
        assert_eq!(summary, "bike.shd: normal bike_n");

        let existing = Some(root.join("bike.shd"));
        std::fs::write(root.join("bike.shd"), "mine").unwrap();
        assert!(plan_shd(&site("bike", &[&root], existing.clone()), false).is_none(), "the converter's own is kept");
        assert!(plan_shd(&site("bike", &[&root], None), false).is_none(), "nor one it would miss");
        assert_eq!(plan_shd(&site("bike", &[&root], existing), true).unwrap().0, root.join("bike.shd"));
        assert!(plan_shd(&site("frame", &[&root], None), true).is_none(), "no maps, no .shd");
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
