//! MXB Coach's commands: find the recorder's session files, list them, review a lap against a
//! faster one, and put the recorder plugin where the game loads it.
//!
//! The recorder writes to `<game user folder>\mxbcoach\sessions\*.mxbc`. Summaries are cached
//! in `<data dir>\coach\index.json`, keyed by path, size and modified time, so listing a
//! folder of long sessions doesn't re-read every one of them.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use mxb_core::config::{self, AppConfig};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::analysis::{self, Ideal, Review, Trace};
use crate::telemetry::{self, Recording};

const PLUGIN: &str = "mxbcoach.dlo";
/// Built and released with FrostMod (`Frostn1/frostmod`, `src/mxbcoach.cpp`).
const PLUGIN_URL: &str = "https://github.com/Frostn1/frostmod/releases/latest/download/mxbcoach.dlo";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Where the recorder writes. More than one candidate: the user folder can be moved, and the
/// configured profiles folder may know better than the default.
fn session_dirs(cfg: &AppConfig) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let users = [cfg.profiles_dir().parent().map(Path::to_path_buf), config::default_user_dir(cfg.game())];
    for user in users.into_iter().flatten() {
        let dir = user.join("mxbcoach").join("sessions");
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

fn plugin_path(cfg: &AppConfig) -> Option<PathBuf> {
    let dir = cfg.install_dir();
    (!dir.trim().is_empty()).then(|| Path::new(&dir).join("plugins").join(PLUGIN))
}

fn load_config(app: &AppHandle) -> AppConfig {
    config::load(app).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub game_dir: String,
    pub plugin_path: Option<String>,
    pub plugin_installed: bool,
    pub session_dirs: Vec<String>,
}

#[tauri::command]
pub fn coach_status(app: AppHandle) -> Status {
    let cfg = load_config(&app);
    let plugin = plugin_path(&cfg);
    Status {
        game_dir: cfg.install_dir(),
        plugin_installed: plugin.as_ref().is_some_and(|p| p.is_file()),
        plugin_path: plugin.map(|p| p.to_string_lossy().into_owned()),
        session_dirs: session_dirs(&cfg).iter().map(|d| d.to_string_lossy().into_owned()).collect(),
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LapSummary {
    pub num: i32,
    pub time_ms: i32,
    pub invalid: bool,
    /// Started and finished at the line, no crash: it can be compared.
    pub whole: bool,
}

impl LapSummary {
    fn comparable(&self) -> bool {
        self.whole && !self.invalid
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub path: String,
    /// `yyyymmdd-hhmmss-mmm`, local time the stint started. Sorts by date.
    pub started: String,
    pub rider: String,
    pub track_id: String,
    pub track_name: String,
    pub bike_id: String,
    pub bike_name: String,
    pub category: String,
    pub track_length: f32,
    pub limiter: i32,
    /// False when the game quit mid-stint.
    pub complete: bool,
    pub laps: Vec<LapSummary>,
    pub best_ms: Option<i32>,
}

fn summarize(path: &Path, rec: &Recording) -> SessionSummary {
    let laps: Vec<LapSummary> = rec
        .laps()
        .iter()
        .map(|l| LapSummary { num: l.num, time_ms: l.time_ms, invalid: l.invalid, whole: l.whole })
        .collect();
    let e = &rec.event;
    SessionSummary {
        path: path.to_string_lossy().into_owned(),
        started: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        rider: e.rider.clone(),
        track_id: e.track_id.clone(),
        track_name: e.track_name.clone(),
        bike_id: e.bike_id.clone(),
        bike_name: e.bike_name.clone(),
        category: e.category.clone(),
        track_length: e.track_length,
        limiter: e.limiter,
        complete: rec.complete,
        best_ms: laps.iter().filter(|l| l.comparable()).map(|l| l.time_ms).min(),
        laps,
    }
}

#[derive(Serialize, Deserialize)]
struct Indexed {
    size: u64,
    modified: u64,
    summary: SessionSummary,
}

fn index_path(app: &AppHandle) -> Option<PathBuf> {
    Some(config::data_dir(app)?.join("coach").join("index.json"))
}

/// Every session on disk, newest first. A file that won't parse is skipped, not fatal.
fn all_sessions(app: &AppHandle) -> Vec<SessionSummary> {
    let cfg = load_config(app);
    let index_file = index_path(app);
    let mut index: HashMap<String, Indexed> = index_file
        .as_ref()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut changed = false;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in session_dirs(&cfg) {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mxbc") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let size = meta.len();
            let modified = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            let key = path.to_string_lossy().into_owned();
            seen.insert(key.clone());
            let summary = match index.get(&key) {
                Some(i) if i.size == size && i.modified == modified => i.summary.clone(),
                _ => {
                    let Some(rec) = fs::read(&path).ok().and_then(|b| telemetry::parse(&b).ok()) else {
                        continue;
                    };
                    let s = summarize(&path, &rec);
                    index.insert(key, Indexed { size, modified, summary: s.clone() });
                    changed = true;
                    s
                }
            };
            out.push(summary);
        }
    }
    let before = index.len();
    index.retain(|k, _| seen.contains(k));
    changed |= index.len() != before;
    if let (true, Some(file)) = (changed, index_file) {
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(&index) {
            let _ = fs::write(file, json);
        }
    }
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out
}

#[tauri::command]
pub fn coach_sessions(app: AppHandle) -> Vec<SessionSummary> {
    all_sessions(&app)
}

fn load(path: &str) -> Result<Recording, String> {
    telemetry::parse(&fs::read(path).map_err(err)?).map_err(err)
}

fn trace(rec: &Recording, num: i32) -> Result<Trace, String> {
    let lap = rec.laps().into_iter().find(|l| l.num == num).ok_or_else(|| format!("Lap {num} isn't in that session."))?;
    Trace::new(&lap, rec.event.track_length).ok_or_else(|| format!("Lap {num} is too short to review."))
}

/// A lap somewhere on disk.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapRef {
    pub path: String,
    pub lap: i32,
    pub time_ms: i32,
    pub started: String,
    pub bike_name: String,
}

/// The fastest comparable lap on this track, other than `exclude`. The same bike wins over a
/// faster lap on a different one: a 250 and a 450 don't take a corner the same way.
fn best_reference(sessions: &[SessionSummary], track_id: &str, bike_id: &str, exclude: Option<(&str, i32)>) -> Option<LapRef> {
    sessions
        .iter()
        .filter(|s| s.track_id == track_id)
        .flat_map(|s| s.laps.iter().filter(|l| l.comparable()).map(move |l| (s, l)))
        .filter(|(s, l)| exclude != Some((s.path.as_str(), l.num)))
        .min_by_key(|(s, l)| (s.bike_id != bike_id, l.time_ms))
        .map(|(s, l)| LapRef {
            path: s.path.clone(),
            lap: l.num,
            time_ms: l.time_ms,
            started: s.started.clone(),
            bike_name: s.bike_name.clone(),
        })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub summary: SessionSummary,
    /// The lap every lap here is reviewed against by default.
    pub reference: Option<LapRef>,
    /// This session's best sections added up, on the reference's sections.
    pub ideal: Option<Ideal>,
}

#[tauri::command]
pub fn coach_session(app: AppHandle, path: String) -> Result<SessionDetail, String> {
    let rec = load(&path)?;
    let summary = summarize(Path::new(&path), &rec);
    let sessions = all_sessions(&app);
    let reference = best_reference(&sessions, &summary.track_id, &summary.bike_id, None);
    let ideal = match &reference {
        Some(r) => {
            let ref_rec = if r.path == path { None } else { Some(load(&r.path)?) };
            let ref_trace = trace(ref_rec.as_ref().unwrap_or(&rec), r.lap)?;
            let sections = analysis::sections(&ref_trace);
            let laps: Vec<(i32, Trace)> = rec
                .laps()
                .iter()
                .filter(|l| l.whole && !l.invalid)
                .filter_map(|l| Some((l.num, Trace::new(l, rec.event.track_length)?)))
                .collect();
            analysis::ideal(&sections, &laps)
        }
        None => None,
    };
    Ok(SessionDetail { summary, reference, ideal })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOut {
    pub track_name: String,
    pub lap: LapRef,
    pub reference: LapRef,
    pub review: Review,
}

/// Reviews lap `lap` of `path` against `ref_path`/`ref_lap`, or against the fastest other lap
/// on the track when none is given.
#[tauri::command]
pub fn coach_review(
    app: AppHandle,
    path: String,
    lap: i32,
    ref_path: Option<String>,
    ref_lap: Option<i32>,
) -> Result<ReviewOut, String> {
    let rec = load(&path)?;
    let summary = summarize(Path::new(&path), &rec);
    let reference = match (ref_path, ref_lap) {
        (Some(p), Some(n)) => {
            let s = if p == path { summary.clone() } else { summarize(Path::new(&p), &load(&p)?) };
            let l = s.laps.iter().find(|l| l.num == n).ok_or_else(|| format!("Lap {n} isn't in that session."))?;
            LapRef { path: p, lap: n, time_ms: l.time_ms, started: s.started, bike_name: s.bike_name }
        }
        _ => best_reference(&all_sessions(&app), &summary.track_id, &summary.bike_id, Some((&path, lap)))
            .ok_or("There's no other whole lap on this track to compare with yet.")?,
    };
    let ref_rec = if reference.path == path { None } else { Some(load(&reference.path)?) };
    let ref_rec = ref_rec.as_ref().unwrap_or(&rec);
    if ref_rec.event.track_id != rec.event.track_id {
        return Err("The reference lap is on a different track.".into());
    }
    let (mine, theirs) = (trace(&rec, lap)?, trace(ref_rec, reference.lap)?);
    let bike = analysis::Bike { limiter: rec.event.limiter as f32, travel: rec.event.susp_max_travel };
    let review = analysis::review(&mine, &theirs, bike);
    let time_ms = summary.laps.iter().find(|l| l.num == lap).map_or(0, |l| l.time_ms);
    Ok(ReviewOut {
        track_name: summary.track_name.clone(),
        lap: LapRef { path, lap, time_ms, started: summary.started, bike_name: summary.bike_name },
        reference,
        review,
    })
}

/// Puts the recorder in `<game>\plugins`: downloaded from the latest FrostMod release, or
/// copied from `from` when given. Returns where it went.
#[tauri::command]
pub async fn coach_install_plugin(app: AppHandle, from: Option<String>) -> Result<String, String> {
    let cfg = load_config(&app);
    let target = plugin_path(&cfg).ok_or("Set the MX Bikes game folder in Settings first.")?;
    let bytes = match from {
        Some(f) => fs::read(&f).map_err(err)?,
        None => {
            let resp = reqwest::get(PLUGIN_URL).await.map_err(err)?;
            if !resp.status().is_success() {
                return Err(format!("Couldn't download the recorder ({}).", resp.status()));
            }
            resp.bytes().await.map_err(err)?.to_vec()
        }
    };
    if !bytes.starts_with(b"MZ") {
        return Err("That isn't a Windows plugin file.".into());
    }
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(err)?;
    }
    let part = target.with_extension("dlo.part");
    fs::write(&part, &bytes).map_err(err)?;
    if fs::rename(&part, &target).is_err() {
        let _ = fs::remove_file(&part);
        return Err("Couldn't replace the recorder. Close MX Bikes and try again.".into());
    }
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn coach_uninstall_plugin(app: AppHandle) -> Result<(), String> {
    let Some(target) = plugin_path(&load_config(&app)) else { return Ok(()) };
    match fs::remove_file(&target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Couldn't remove the recorder. Close MX Bikes and try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(path: &str, track: &str, bike: &str, laps: &[(i32, i32, bool)]) -> SessionSummary {
        SessionSummary {
            path: path.into(),
            started: path.into(),
            rider: String::new(),
            track_id: track.into(),
            track_name: String::new(),
            bike_id: bike.into(),
            bike_name: bike.into(),
            category: String::new(),
            track_length: 1000.0,
            limiter: 0,
            complete: true,
            laps: laps.iter().map(|&(num, time_ms, whole)| LapSummary { num, time_ms, invalid: false, whole }).collect(),
            best_ms: None,
        }
    }

    #[test]
    fn the_reference_is_the_fastest_whole_lap_on_the_same_bike() {
        let all = [
            session("a", "indiana", "kx450", &[(1, 60_000, true), (2, 58_000, false)]),
            session("b", "indiana", "yz250", &[(1, 55_000, true)]),
            session("c", "indiana", "kx450", &[(1, 59_000, true)]),
            session("d", "other", "kx450", &[(1, 40_000, true)]),
        ];
        let r = best_reference(&all, "indiana", "kx450", None).unwrap();
        assert_eq!((r.path.as_str(), r.lap), ("c", 1), "same bike beats a faster other bike; broken lap skipped");
        let r = best_reference(&all, "indiana", "kx450", Some(("c", 1))).unwrap();
        assert_eq!((r.path.as_str(), r.lap), ("a", 1), "never the lap under review");
        let r = best_reference(&all, "indiana", "crf250", None).unwrap();
        assert_eq!(r.path, "b", "no lap on this bike: the fastest on any");
        assert!(best_reference(&all, "nowhere", "kx450", None).is_none());
    }
}
