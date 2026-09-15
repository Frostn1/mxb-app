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
    /// Started and finished at the line: it can be compared.
    pub whole: bool,
    /// Why it can't be compared, when it can't: `out lap`, `unfinished`, `gap in the recording`.
    #[serde(default)]
    pub issue: Option<String>,
    #[serde(default)]
    pub crashed: bool,
    /// How long it took by the recording, for a lap the game left untimed.
    #[serde(default)]
    pub ridden_ms: i32,
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
        .map(|l| LapSummary {
            num: l.num,
            time_ms: l.time_ms,
            invalid: l.invalid,
            whole: l.whole,
            issue: l.issue.map(str::to_owned),
            crashed: l.crashed,
            ridden_ms: l.ridden_ms,
        })
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
    // v2: laps say why they can't be compared, and a crash no longer makes one partial.
    Some(config::data_dir(app)?.join("coach").join("index-v2.json"))
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
    pub track_id: String,
    pub track_name: String,
    pub lap: LapRef,
    pub reference: LapRef,
    pub review: Review,
}

/// Reviews lap `lap` of `path` against `ref_path`/`ref_lap`, or against the fastest other lap
/// on the track when none is given. `solo`, or no other lap to compare with, reviews it on its
/// own instead.
#[tauri::command]
pub fn coach_review(
    app: AppHandle,
    path: String,
    lap: i32,
    ref_path: Option<String>,
    ref_lap: Option<i32>,
    solo: Option<bool>,
) -> Result<ReviewOut, String> {
    let rec = load(&path)?;
    let summary = summarize(Path::new(&path), &rec);
    let reference = if solo.unwrap_or(false) {
        None
    } else {
        match (ref_path, ref_lap) {
            (Some(p), Some(n)) => {
                let s = if p == path { summary.clone() } else { summarize(Path::new(&p), &load(&p)?) };
                let l = s.laps.iter().find(|l| l.num == n).ok_or_else(|| format!("Lap {n} isn't in that session."))?;
                Some(LapRef { path: p, lap: n, time_ms: l.time_ms, started: s.started, bike_name: s.bike_name })
            }
            _ => best_reference(&all_sessions(&app), &summary.track_id, &summary.bike_id, Some((&path, lap))),
        }
    };
    let e = &rec.event;
    let bike = analysis::Bike {
        limiter: e.limiter as f32,
        max_rpm: e.max_rpm as f32,
        shift_rpm: e.shift_rpm as f32,
        travel: e.susp_max_travel,
    };
    let mine = trace(&rec, lap)?;
    let time_ms = summary.laps.iter().find(|l| l.num == lap).map_or(0, |l| l.time_ms);
    let this = LapRef { path, lap, time_ms, started: summary.started.clone(), bike_name: summary.bike_name.clone() };
    let (review, reference) = match reference {
        Some(reference) => {
            let ref_rec = if reference.path == this.path { None } else { Some(load(&reference.path)?) };
            let ref_rec = ref_rec.as_ref().unwrap_or(&rec);
            if ref_rec.event.track_id != rec.event.track_id {
                return Err("The reference lap is on a different track.".into());
            }
            (analysis::review(&mine, &trace(ref_rec, reference.lap)?, bike), reference)
        }
        None => (analysis::solo(&mine, bike), this.clone()),
    };
    Ok(ReviewOut { track_id: summary.track_id, track_name: summary.track_name, lap: this, reference, review })
}

/// The setup the rider had on for a recording, as far as the coach could find and read it.
struct RiderSetup {
    name: String,
    file: Option<PathBuf>,
    setup: Option<crate::stp::Setup>,
    opts: Option<crate::bikecfg::BikeOptions>,
    why: Option<String>,
}

fn rider_setup(app: &AppHandle, path: &str) -> Result<RiderSetup, String> {
    let rec = load(path)?;
    let cfg = load_config(app);
    let e = &rec.event;
    let raw = rec.session.setup.clone();
    let name = raw.trim_start_matches(':').to_string();
    let opts = crate::bikecfg::load(&cfg.mods_path, &e.bike_id);
    let file = crate::stp::locate(&cfg.profiles_dir(), &raw, &e.track_id, &e.bike_id);
    let setup = file
        .as_ref()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| crate::stp::Setup::parse(&b, usize::try_from(e.gears).ok()).ok())
        .filter(|s| s.bike_id() == e.bike_id);
    let why = if name.is_empty() || name.eq_ignore_ascii_case("default") {
        Some("You rode the bike's default setup. Save it under a name in the garage, and the coach can change it for you.".into())
    } else if file.is_none() {
        Some(format!("Your setup \"{name}\" wasn't found in your profiles folder."))
    } else if setup.is_none() {
        Some(format!("Your setup \"{name}\" couldn't be read."))
    } else if opts.is_none() {
        Some("The bike's own settings couldn't be read, so the coach can't tell how far each one goes.".into())
    } else {
        None
    };
    Ok(RiderSetup { name, file, setup, opts, why })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPlan {
    /// The setup the rider had on.
    pub name: String,
    pub file: Option<String>,
    /// The name a saved copy gets: the next free "(coach)", "(coach 2)" … beside it.
    pub save_as: Option<String>,
    /// Why the coach can't make the changes itself, when it can't.
    pub why: Option<String>,
    pub fixes: Vec<crate::fixes::Fix>,
}

/// The rider's setup name without any "(coach)" the coach added: copies of a copy are numbered.
fn setup_base(file: &Path) -> String {
    crate::stp::base_name(file.file_stem().and_then(|s| s.to_str()).unwrap_or("setup")).to_string()
}

/// The changes behind a lap's setup tips, against the setup the rider had on.
#[tauri::command]
pub fn coach_setup_plan(app: AppHandle, path: String, skills: Vec<String>) -> Result<SetupPlan, String> {
    let r = rider_setup(&app, &path)?;
    let fixes = crate::fixes::plan(&skills, r.setup.as_ref(), r.opts.as_ref());
    let save_as = r.file.as_deref().and_then(|f| {
        let dir = f.parent()?;
        crate::stp::coach_names(&setup_base(f)).find(|n| !dir.join(format!("{n}.stp")).exists())
    });
    Ok(SetupPlan { name: r.name, file: r.file.map(|p| p.display().to_string()), save_as, why: r.why, fixes })
}

/// Saves a lap's setup fixes as a new setup beside the rider's own and returns its name.
/// Never overwrites a file: the rider's setup stays as it was.
#[tauri::command]
pub fn coach_save_setup(app: AppHandle, path: String, skills: Vec<String>) -> Result<String, String> {
    let r = rider_setup(&app, &path)?;
    let (Some(file), Some(setup), Some(opts)) = (r.file, r.setup, r.opts) else {
        return Err(r.why.unwrap_or_else(|| "The coach can't change this setup.".into()));
    };
    let fixes = crate::fixes::plan(&skills, Some(&setup), Some(&opts));
    let (out, moved) = crate::fixes::apply(&setup, &fixes, &opts);
    if moved == 0 {
        return Err("There's nothing in this setup the coach can change.".into());
    }
    let dir = file.parent().ok_or("The setup's folder couldn't be found.")?;
    let base = setup_base(&file);
    for name in crate::stp::coach_names(&base) {
        match fs::OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{name}.stp"))) {
            Ok(mut f) => {
                std::io::Write::write_all(&mut f, out.bytes()).map_err(err)?;
                return Ok(name);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(err(e)),
        }
    }
    Err("There are already too many coach setups for this one.".into())
}

/// How the session's lines and the track changed, against the fastest lap on the track; see
/// `lines.rs`. None until there is a lap to compare with.
#[tauri::command]
pub fn coach_lines(app: AppHandle, path: String) -> Result<Option<crate::lines::Lines>, String> {
    let rec = load(&path)?;
    let summary = summarize(Path::new(&path), &rec);
    let Some(r) = best_reference(&all_sessions(&app), &summary.track_id, &summary.bike_id, None) else {
        return Ok(None);
    };
    let ref_rec = if r.path == path { None } else { Some(load(&r.path)?) };
    let reference = trace(ref_rec.as_ref().unwrap_or(&rec), r.lap)?;
    let laps: Vec<(i32, Trace)> = rec
        .laps()
        .iter()
        .filter(|l| l.whole && !l.invalid)
        .filter_map(|l| Some((l.num, Trace::new(l, rec.event.track_length)?)))
        .collect();
    Ok(Some(crate::lines::lines(&laps, &reference)))
}

/// The track's own terrain for a session: installed, readable, and lined up with the laps.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ground {
    pub path: String,
    pub prefix: Option<String>,
    pub name: String,
    /// How high the bike rides above this terrain, metres.
    pub lift: f32,
}

/// The track's own terrain, or why the ground built from the laps is drawn instead.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundAnswer {
    pub ground: Option<Ground>,
    pub why: Option<String>,
}

/// Off the main thread: the first read of a big track is most of a second.
#[tauri::command]
pub async fn coach_ground(app: AppHandle, path: String) -> Result<GroundAnswer, String> {
    tauri::async_runtime::spawn_blocking(move || ground_for(&app, &path)).await.map_err(err)?
}

fn ground_for(app: &AppHandle, path: &str) -> Result<GroundAnswer, String> {
    let no = |why: &str| Ok(GroundAnswer { ground: None, why: Some(why.into()) });
    let rec = load(path)?;
    let Some(src) = mxb_core::tracksource::resolve(&load_config(app), &rec.event.track_id) else {
        return no("the track isn't in your mods");
    };
    // A GUID-locked track opens here the way it does in MXB App, through the private reader
    // in a release build; only one that still can't be read counts as locked.
    if src.locked {
        return no("the track is locked");
    }
    let Ok(master) = mxb_core::track::load_master(app, &src.path, src.prefix.as_deref()) else {
        return no("its terrain couldn't be read");
    };
    let points: Vec<[f32; 3]> =
        rec.samples.iter().filter(|s| !s.airborne() && !s.crashed).step_by(5).map(|s| [s.x, s.y, s.z]).collect();
    let i = &master.info;
    match crate::ground::fit(i.width as usize, i.height as usize, i.metres_per_sample, &master.heights, &points) {
        Some(lift) => Ok(GroundAnswer { ground: Some(Ground { path: src.path, prefix: src.prefix, name: src.name, lift }), why: None }),
        None => no("its terrain doesn't line up with your laps"),
    }
}

/// The ground under a session's laps, built from the laps; see `surface.rs`.
#[tauri::command]
pub fn coach_surface(path: String) -> Result<Option<crate::surface::Surface>, String> {
    Ok(crate::surface::build(&load(&path)?.samples, 400))
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
            laps: laps
                .iter()
                .map(|&(num, time_ms, whole)| LapSummary { num, time_ms, invalid: false, whole, issue: None, crashed: false, ridden_ms: 0 })
                .collect(),
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
