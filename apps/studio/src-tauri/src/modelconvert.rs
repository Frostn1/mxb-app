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
    /// What writing those files decided for the modder: a map used by name because the
    /// material doesn't link it, `.hrc` files kept or replaced.
    pub notes: Vec<String>,
}

/// When a bike's `.hrc` files are written beside its `.edf`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HrcMode {
    /// Every one; one already there and different is kept as `<name>.bak` first.
    Always,
    /// Only into a folder with no `.hrc` at all.
    #[default]
    IfNone,
    Never,
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

/// `<name>.bak`, or `<name>.bak2`, `.bak3`… when that is taken: never over another file.
fn backup_path(p: &Path) -> PathBuf {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut n = 1;
    loop {
        let candidate = p.with_file_name(if n == 1 { format!("{name}.bak") } else { format!("{name}.bak{n}") });
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Write a bike's `.hrc` files beside its `.edf` as `mode` says. A modder's own are often
/// edited by hand, so none is ever written over unsaid: `IfNone` writes nothing into a folder
/// that has one, and `Always` keeps a different one as `<name>.bak` first. Written, and notes.
fn write_hrc(report: &Value, edf: &Path, mode: HrcMode) -> (Vec<String>, Vec<String>) {
    let names: Vec<String> = report["objects"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| o["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let edf_name = edf.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = edf.parent().map(Path::to_path_buf).unwrap_or_default();
    let files = hrc_files(&names, &edf_name);
    let (mut written, mut notes) = (Vec::new(), Vec::new());
    if files.is_empty() || mode == HrcMode::Never {
        return (written, notes);
    }
    if mode == HrcMode::IfNone {
        let has_hrc = std::fs::read_dir(&dir).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("hrc")))
        });
        if has_hrc {
            notes.push("The folder already has .hrc files; none written.".into());
            return (written, notes);
        }
    }
    for (name, text) in files {
        let p = dir.join(&name);
        if p.exists() {
            if std::fs::read_to_string(&p).is_ok_and(|old| old == text) {
                continue;
            }
            let bak = backup_path(&p);
            if let Err(e) = std::fs::rename(&p, &bak) {
                log::warn!("[convert] couldn't keep {} as {}: {e}", p.display(), bak.display());
                notes.push(format!("{name} kept: it couldn't be backed up."));
                continue;
            }
            notes.push(format!("{name} replaced; the old one is {}.", bak.file_name().unwrap_or_default().to_string_lossy()));
        }
        match std::fs::write(&p, text) {
            Ok(()) => written.push(name),
            Err(e) => log::warn!("[convert] couldn't write {}: {e}", p.display()),
        }
    }
    (written, notes)
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

/// The shininess a written `.shd` gets unless the modder picks another: what working
/// modded bikes use.
pub const SHININESS: f32 = 6.0;

/// A `.shd` naming `maps`, laid out the way `fbx2edf.exe` reads it: each block's name, braces
/// and keys on lines of their own (it skips a block written on one line), in the stock order
/// bump, specular, reflection. Reflection runs 0 to 0.5 with exponent 1.0, as modders' working
/// bikes have it. With no specular map, the specular mask is the normal map's alpha.
pub fn shd_text(maps: &Maps, shininess: f32) -> String {
    let mut out = String::new();
    let mut block = |name: &str, lines: Vec<String>| {
        out.push_str(&format!("{name}\r\n{{\r\n"));
        for l in lines {
            out.push_str(&format!("\t{l}\r\n"));
        }
        out.push_str("}\r\n");
    };
    if let Some(m) = &maps.normal {
        block("bump", vec![format!("map = {m}"), "repetitions = 1".into()]);
    }
    if maps.normal.is_some() || maps.specular.is_some() {
        let mut l = vec![format!("shininess = {shininess}")];
        l.extend(maps.specular.iter().map(|m| format!("map = {m}")));
        block("specular", l);
    }
    if let Some(m) = &maps.reflection {
        block("reflection", vec!["factormin = 0".into(), "factormax = 0.5".into(), "factorexp = 1.0".into(), format!("map = {m}")]);
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

/// One `.shd` to write: where, what, the summary line, and notes for the modder.
#[derive(Debug, PartialEq)]
pub struct ShdPlan {
    pub path: PathBuf,
    pub text: String,
    pub summary: String,
    pub notes: Vec<String>,
}

/// A linked map as the `.shd` in `dir` names it: its file name when it sits there, else its
/// full path (the converter reads either).
fn map_name(map: &Path, dir: &Path) -> String {
    match map.parent() {
        Some(p) if p == dir => map.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        _ => map.to_string_lossy().into_owned(),
    }
}

/// The `.shd` to write for one picture, if any. The maps the material links come first; a
/// normal or reflection map it doesn't link but that sits beside the picture by name
/// (`X_n`, `X_r`) is used too, with a note saying so. The `.shd` goes in the first of the
/// picture's folders with a map, since the converter reads the maps it names beside it. A
/// `.shd` the converter already reads is left alone unless `overwrite`.
pub fn plan_shd(site: &host::ShdSite, overwrite: bool, shininess: f32) -> Option<ShdPlan> {
    if site.shd.is_some() && !overwrite {
        return None;
    }
    let linked = site.normal.is_some() || site.reflection.is_some();
    let found = site.dirs.iter().map(|d| (d, find_maps(d, &site.name, site.ext.as_deref()))).find(|(_, m)| m.any());
    let (dir, mut maps) = match found {
        Some((d, m)) => (d.clone(), m),
        None if linked => (site.dirs.first()?.clone(), Maps::default()),
        None => return None,
    };
    let mut notes = Vec::new();
    match &site.normal {
        Some(n) => maps.normal = Some(map_name(n, &dir)),
        None => notes.extend(maps.normal.iter().map(|m| format!("normal map not connected, using {m}"))),
    }
    match &site.reflection {
        Some(r) => maps.reflection = Some(map_name(r, &dir)),
        None => notes.extend(maps.reflection.iter().map(|m| format!("reflection map not connected, using {m}"))),
    }
    let file = format!("{}.shd", site.name);
    let path = dir.join(&file);
    if path.exists() && !overwrite {
        return None;
    }
    Some(ShdPlan { text: shd_text(&maps, shininess), summary: shd_summary(&file, &maps), path, notes })
}

/// Write the `.shd` files an FBX's textures lack, before it is converted, so the converter
/// builds their maps in. `done` holds the files written so far in the batch: a texture two
/// models share is written once. Summaries, and notes.
fn write_shd(fbx: &Path, overwrite: bool, shininess: f32, done: &mut std::collections::HashSet<PathBuf>) -> (Vec<String>, Vec<String>) {
    let sites = match host::shd_sites(fbx) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[convert] no .shd files for {}: {e}", fbx.display());
            return (Vec::new(), Vec::new());
        }
    };
    let (mut written, mut notes) = (Vec::new(), Vec::new());
    for site in &sites {
        let Some(plan) = plan_shd(site, overwrite, shininess) else { continue };
        if !done.insert(plan.path.clone()) {
            continue;
        }
        match std::fs::write(&plan.path, plan.text) {
            Ok(()) => {
                written.push(plan.summary);
                notes.extend(plan.notes);
            }
            Err(e) => log::warn!("[convert] couldn't write {}: {e}", plan.path.display()),
        }
    }
    (written, notes)
}

/// Convert a batch. Each pair's progress and outcome is sent as it happens (`fbx-convert-progress`,
/// `fbx-convert-done`), and every outcome is returned at the end, in order. With `shd`, the
/// `.shd` files its textures lack are written first (see [`write_shd`]).
#[tauri::command]
pub async fn fbx_convert(
    app: tauri::AppHandle,
    pairs: Vec<Pair>,
    options: Options,
    hrc: HrcMode,
    shd: Option<bool>,
    overwrite_shd: Option<bool>,
    shininess: Option<f32>,
) -> Result<Vec<Outcome>, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("A batch is already converting.".into());
    }
    let (shd, overwrite_shd) = (shd.unwrap_or(false), overwrite_shd.unwrap_or(false));
    let shininess = shininess.filter(|s| s.is_finite() && *s >= 0.0).unwrap_or(SHININESS);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|p| (PathBuf::from(p.input), PathBuf::from(p.output))).collect();
        let started = std::time::Instant::now();
        let mut done = std::collections::HashSet::new();
        let mut shd_files: Vec<(Vec<String>, Vec<String>)> = pairs
            .iter()
            .map(|(fbx, _)| if shd { write_shd(fbx, overwrite_shd, shininess, &mut done) } else { Default::default() })
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
            .map(|(i, (r, o))| o.unwrap_or_else(|| outcome_of(r, hrc, &pairs[i].1, Default::default())))
            .collect())
    })
    .await
    .map_err(|e| e.to_string());
    RUNNING.store(false, Ordering::SeqCst);
    result?
}

fn outcome_of(result: Result<Value, String>, hrc: HrcMode, edf: &Path, (shd, mut notes): (Vec<String>, Vec<String>)) -> Outcome {
    match result {
        Ok(report) => {
            let (hrc, hrc_notes) = write_hrc(&report, edf, hrc);
            notes.extend(hrc_notes);
            Outcome { report: Some(report), error: None, hrc, shd, notes }
        }
        Err(e) => Outcome { error: Some(e), shd, notes, ..Default::default() },
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
        host::ShdSite {
            name: name.into(),
            ext: Some("tga".into()),
            dirs: dirs.iter().map(|d| d.to_path_buf()).collect(),
            shd,
            normal: None,
            reflection: None,
        }
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
    fn a_shd_is_written_in_the_stock_order_the_way_fbx2edf_exe_reads_it() {
        let maps = Maps { normal: Some("X_n.tga".into()), reflection: Some("X_r.tga".into()), specular: None };
        // Husk's working file: bump{map=X_n.tga, repetitions=1} specular{shininess=6}
        // reflection{factormin=0, factormax=0.5, factorexp=1.0, map=X_r.tga}, one key a line.
        assert_eq!(
            shd_text(&maps, SHININESS),
            "bump\r\n{\r\n\tmap = X_n.tga\r\n\trepetitions = 1\r\n}\r\n\
             specular\r\n{\r\n\tshininess = 6\r\n}\r\n\
             reflection\r\n{\r\n\tfactormin = 0\r\n\tfactormax = 0.5\r\n\tfactorexp = 1.0\r\n\tmap = X_r.tga\r\n}\r\n"
        );
        let spec = Maps { specular: Some("bike_s.tga".into()), ..Maps::default() };
        assert_eq!(shd_text(&spec, 12.5), "specular\r\n{\r\n\tshininess = 12.5\r\n\tmap = bike_s.tga\r\n}\r\n");
        let refl = Maps { reflection: Some("bike_r.tga".into()), ..Maps::default() };
        assert!(shd_text(&refl, SHININESS).starts_with("reflection\r\n"), "an _r alone still gets its reflection");
        assert_eq!(shd_summary("X.shd", &maps), "X.shd: normal X_n, reflection X_r");
    }

    #[test]
    fn a_shd_goes_where_the_maps_are_and_never_over_one_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (fbm, root) = (dir.path().join("bike.fbm"), dir.path().to_path_buf());
        std::fs::create_dir(&fbm).unwrap();
        std::fs::write(root.join("bike_n.tga"), b"x").unwrap();
        // Embedded paint: the .fbm folder has no maps, the FBX's folder has.
        let plan = plan_shd(&site("bike", &[&fbm, &root], None), false, SHININESS).unwrap();
        assert_eq!(plan.path, root.join("bike.shd"));
        assert!(plan.text.contains("map = bike_n.tga"));
        assert_eq!(plan.summary, "bike.shd: normal bike_n");

        let existing = Some(root.join("bike.shd"));
        std::fs::write(root.join("bike.shd"), "mine").unwrap();
        assert!(plan_shd(&site("bike", &[&root], existing.clone()), false, SHININESS).is_none(), "the converter's own is kept");
        assert!(plan_shd(&site("bike", &[&root], None), false, SHININESS).is_none(), "nor one it would miss");
        assert_eq!(plan_shd(&site("bike", &[&root], existing), true, SHININESS).unwrap().path, root.join("bike.shd"));
        assert!(plan_shd(&site("frame", &[&root], None), true, SHININESS).is_none(), "no maps, no .shd");
    }

    #[test]
    fn maps_found_by_name_say_so_and_linked_ones_win() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        for f in ["X_n.tga", "X_r.tga"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        let plan = plan_shd(&site("X", &[&root], None), false, SHININESS).unwrap();
        assert_eq!(plan.notes, ["normal map not connected, using X_n.tga", "reflection map not connected, using X_r.tga"]);
        assert!(plan.text.contains("map = X_r.tga"));

        // Linked in Blender: those are used, by name when beside the .shd, and nothing is said.
        let maps = root.join("maps");
        std::fs::create_dir(&maps).unwrap();
        std::fs::write(maps.join("X_normal.tga"), b"x").unwrap();
        let linked = host::ShdSite { normal: Some(maps.join("X_normal.tga")), reflection: Some(root.join("X_r.tga")), ..site("X", &[&root], None) };
        let plan = plan_shd(&linked, false, SHININESS).unwrap();
        assert!(plan.notes.is_empty(), "{:?}", plan.notes);
        assert!(plan.text.contains(&format!("map = {}", maps.join("X_normal.tga").display())), "{}", plan.text);
        assert!(plan.text.contains("map = X_r.tga"));

        // Linked maps alone, nothing beside the picture by name: still a .shd.
        let empty = tempfile::tempdir().unwrap();
        let only = host::ShdSite { reflection: Some(root.join("X_r.tga")), ..site("Y", &[empty.path()], None) };
        let plan = plan_shd(&only, false, SHININESS).unwrap();
        assert_eq!(plan.path, empty.path().join("Y.shd"));
        assert!(plan.text.contains("factormax = 0.5"));
    }

    fn bike_report() -> Value {
        serde_json::json!({ "objects": [{ "name": "" }, { "name": "steer" }] })
    }

    #[test]
    fn hrc_if_none_writes_only_into_a_folder_without_any() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("model.edf");
        let (written, notes) = write_hrc(&bike_report(), &edf, HrcMode::IfNone);
        assert_eq!(written, ["chassis.hrc", "steer.hrc"]);
        assert!(notes.is_empty());

        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("steer.hrc"), "mine").unwrap();
        let (written, notes) = write_hrc(&bike_report(), &other.path().join("model.edf"), HrcMode::IfNone);
        assert!(written.is_empty(), "one .hrc there is enough to write none");
        assert_eq!(notes, ["The folder already has .hrc files; none written."]);
        assert!(!other.path().join("chassis.hrc").exists());
        assert_eq!(std::fs::read_to_string(other.path().join("steer.hrc")).unwrap(), "mine");
    }

    #[test]
    fn hrc_never_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(write_hrc(&bike_report(), &dir.path().join("model.edf"), HrcMode::Never), (vec![], vec![]));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn hrc_always_keeps_a_different_one_as_a_backup_first() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("model.edf");
        std::fs::write(dir.path().join("steer.hrc"), "mine").unwrap();
        std::fs::write(dir.path().join("steer.hrc.bak"), "older").unwrap();
        let (written, notes) = write_hrc(&bike_report(), &edf, HrcMode::Always);
        assert_eq!(written, ["chassis.hrc", "steer.hrc"]);
        assert_eq!(notes, ["steer.hrc replaced; the old one is steer.hrc.bak2."]);
        assert_eq!(std::fs::read_to_string(dir.path().join("steer.hrc.bak2")).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(dir.path().join("steer.hrc.bak")).unwrap(), "older", "never over another file");
        assert!(std::fs::read_to_string(dir.path().join("steer.hrc")).unwrap().contains("name = steer"));

        // The same again: nothing differs, so nothing is backed up or said.
        let (written, notes) = write_hrc(&bike_report(), &edf, HrcMode::Always);
        assert!(written.is_empty() && notes.is_empty(), "{written:?} {notes:?}");
    }

    #[test]
    fn hrc_modes_read_as_the_page_sends_them() {
        for (s, m) in [("\"always\"", HrcMode::Always), ("\"ifNone\"", HrcMode::IfNone), ("\"never\"", HrcMode::Never)] {
            assert_eq!(serde_json::from_str::<HrcMode>(s).unwrap(), m);
        }
    }
}
