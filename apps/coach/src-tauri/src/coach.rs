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
pub(crate) fn session_dirs(cfg: &AppConfig) -> Vec<PathBuf> {
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

pub(crate) fn load_config(app: &AppHandle) -> AppConfig {
    config::load(app).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub game_dir: String,
    pub plugin_path: Option<String>,
    pub plugin_installed: bool,
    pub session_dirs: Vec<String>,
    /// The recorder's own version, as it wrote it the last time the game ran it. None until
    /// the game has run it once.
    pub recorder_version: Option<String>,
    /// The recorder that ran is older than the one the HUD and the spoken cues need.
    pub recorder_outdated: bool,
}

#[tauri::command]
pub fn coach_status(app: AppHandle) -> Status {
    let cfg = load_config(&app);
    let plugin = plugin_path(&cfg);
    let dirs = session_dirs(&cfg);
    let recorder_version = crate::hud::coach_dir_of(&dirs).as_deref().and_then(crate::hud::recorder_version);
    Status {
        game_dir: cfg.install_dir(),
        plugin_installed: plugin.as_ref().is_some_and(|p| p.is_file()),
        plugin_path: plugin.map(|p| p.to_string_lossy().into_owned()),
        session_dirs: dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect(),
        recorder_outdated: recorder_version
            .as_deref()
            .is_some_and(|v| !crate::hud::at_least(v, crate::hud::RECORDER_NEEDS)),
        recorder_version,
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LapSummary {
    pub num: i32,
    /// The recording this lap is in: a session is every stint of one event.
    #[serde(default)]
    pub path: String,
    /// Which stint of the session it was ridden in, from 0. The game numbers laps per stint,
    /// so two stints both have a lap 1.
    #[serde(default)]
    pub stint: i32,
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

/// One stint on track: one recording. A session is every stint of one event.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stint {
    pub path: String,
    pub started: String,
    /// The setup it was ridden on, as the game names it.
    pub setup: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// The first stint's file. Every stint is in `stints`.
    pub path: String,
    /// `yyyymmdd-hhmmss-mmm`, local time the first stint started. Sorts by date.
    pub started: String,
    pub rider: String,
    pub track_id: String,
    pub track_name: String,
    pub bike_id: String,
    pub bike_name: String,
    pub category: String,
    /// 1 = testing, 2 = race, 4 = straight rhythm. A stint of another kind is another session.
    #[serde(default)]
    pub event_type: i32,
    pub track_length: f32,
    pub limiter: i32,
    /// False when the game quit mid-stint.
    pub complete: bool,
    pub laps: Vec<LapSummary>,
    pub best_ms: Option<i32>,
    /// The setup the last stint was ridden on, as the game names it (without a common
    /// setup's ':').
    #[serde(default)]
    pub setup: String,
    /// Every stint this session was ridden in, oldest first.
    #[serde(default)]
    pub stints: Vec<Stint>,
}

fn summarize(path: &Path, rec: &Recording) -> SessionSummary {
    let file = path.to_string_lossy().into_owned();
    let laps: Vec<LapSummary> = rec
        .laps()
        .iter()
        .map(|l| LapSummary {
            num: l.num,
            path: file.clone(),
            stint: 0,
            time_ms: l.time_ms,
            invalid: l.invalid,
            whole: l.whole,
            issue: l.issue.map(str::to_owned),
            crashed: l.crashed,
            ridden_ms: l.ridden_ms,
        })
        .collect();
    let e = &rec.event;
    let started = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let setup = rec.session.setup.trim_start_matches(':').to_string();
    SessionSummary {
        path: file.clone(),
        rider: e.rider.clone(),
        track_id: e.track_id.clone(),
        track_name: e.track_name.clone(),
        bike_id: e.bike_id.clone(),
        bike_name: e.bike_name.clone(),
        category: e.category.clone(),
        event_type: e.event_type,
        track_length: e.track_length,
        limiter: e.limiter,
        complete: rec.complete,
        best_ms: laps.iter().filter(|l| l.comparable()).map(|l| l.time_ms).min(),
        laps,
        stints: vec![Stint { path: file, started: started.clone(), setup: setup.clone() }],
        started,
        setup,
    }
}

/// How long a break makes the next stint a new session. Going out and back in during one
/// event writes another file every time, and those are all the same session.
const SAME_EVENT_GAP_S: i64 = 3 * 60 * 60;

/// Days since 1970-01-01, by Howard Hinnant's `days_from_civil`.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A `yyyymmdd-hhmmss-mmm` stamp as seconds, so two stints can be held apart in time.
fn stamp_secs(stamp: &str) -> Option<i64> {
    let date: i64 = stamp.get(..8)?.parse().ok()?;
    let time: i64 = stamp.get(9..15)?.parse().ok()?;
    let day = days_from_civil(date / 10_000, date / 100 % 100, date % 100);
    Some(day * 86_400 + time / 10_000 * 3600 + time / 100 % 100 * 60 + time % 100)
}

/// How long a stint was on track, seconds, by its laps.
fn ridden_s(s: &SessionSummary) -> i64 {
    s.laps.iter().map(|l| i64::from(l.time_ms.max(l.ridden_ms)).max(0)).sum::<i64>() / 1000
}

/// Whether two stints are the same event: the same rider on the same bike at the same track,
/// in the same kind of event. Not the game's session — one race event runs through practice,
/// qualifying and the race, and all of it is one session.
fn same_event(a: &SessionSummary, b: &SessionSummary) -> bool {
    (&a.rider, &a.track_id, &a.bike_id, a.event_type) == (&b.rider, &b.track_id, &b.bike_id, b.event_type)
}

/// Adds a stint to the session it belongs to.
fn merge_stint(into: &mut SessionSummary, s: SessionSummary) {
    let stint = into.stints.len() as i32;
    into.laps.extend(s.laps.into_iter().map(|l| LapSummary { stint, ..l }));
    into.best_ms = [into.best_ms, s.best_ms].into_iter().flatten().min();
    // The last stint says how the session ended and what it was ridden on.
    into.complete = s.complete;
    into.setup = s.setup;
    into.stints.extend(s.stints);
}

/// One session per event: stints of the same event, ridden back to back, become one session
/// with all their laps. Oldest first.
fn group_sessions(mut stints: Vec<SessionSummary>) -> Vec<SessionSummary> {
    stints.sort_by(|a, b| a.started.cmp(&b.started));
    // Each group with the time its last stint came off track.
    let mut out: Vec<(SessionSummary, i64)> = Vec::new();
    for s in stints {
        let start = stamp_secs(&s.started);
        let end = start.unwrap_or(0) + ridden_s(&s);
        let joins = out.last().is_some_and(|(g, off)| {
            same_event(g, &s) && start.map_or(true, |t| t - off <= SAME_EVENT_GAP_S)
        });
        match out.len().checked_sub(1).filter(|_| joins) {
            Some(i) => {
                let (g, off) = &mut out[i];
                merge_stint(g, s);
                *off = end;
            }
            None => out.push((s, end)),
        }
    }
    out.into_iter().map(|(g, _)| g).collect()
}

#[derive(Serialize, Deserialize)]
struct Indexed {
    size: u64,
    modified: u64,
    summary: SessionSummary,
}

fn index_path(app: &AppHandle) -> Option<PathBuf> {
    // v4: stints carry the event they belong to, and laps the file they're in.
    Some(config::data_dir(app)?.join("coach").join("index-v4.json"))
}

/// Every session on disk, newest first: the stints of one event as one session. A file that
/// won't parse is skipped, not fatal.
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
    let mut out = group_sessions(out);
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
        .filter(|(_, l)| exclude != Some((l.path.as_str(), l.num)))
        .min_by_key(|(s, l)| (s.bike_id != bike_id, l.time_ms))
        .map(|(s, l)| LapRef {
            // The lap's own file and the stint it was ridden in: a session spans several.
            path: l.path.clone(),
            lap: l.num,
            time_ms: l.time_ms,
            started: s.stints.get(l.stint.max(0) as usize).map_or(&s.started, |x| &x.started).clone(),
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

/// The whole session `path` belongs to: every stint of that event, with all their laps.
#[tauri::command]
pub fn coach_session(app: AppHandle, path: String) -> Result<SessionDetail, String> {
    let sessions = all_sessions(&app);
    let summary = match sessions.iter().find(|s| s.stints.iter().any(|x| x.path == path)) {
        Some(s) => s.clone(),
        None => summarize(Path::new(&path), &load(&path)?),
    };
    let reference = best_reference(&sessions, &summary.track_id, &summary.bike_id, None);
    let ideal = match &reference {
        Some(r) => {
            let ref_trace = trace(&load(&r.path)?, r.lap)?;
            let sections = analysis::sections(&ref_trace);
            // Keyed by where each lap sits in the session's list, not by its number: every
            // stint starts counting at lap 1 again.
            let mut laps: Vec<(i32, Trace)> = Vec::new();
            for (i, st) in summary.stints.iter().enumerate() {
                let Ok(rec) = load(&st.path) else { continue };
                for l in rec.laps().iter().filter(|l| l.whole && !l.invalid) {
                    let at = summary.laps.iter().position(|x| x.stint as usize == i && x.num == l.num);
                    if let (Some(at), Some(t)) = (at, Trace::new(l, rec.event.track_length)) {
                        laps.push((at as i32, t));
                    }
                }
            }
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
    /// Other riders in the session worth comparing with, where the recorder saw them.
    pub rivals: Vec<crate::others::Rival>,
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
    // The whole session, so one lap's floors are this rider's on this bike.
    let laps: Vec<analysis::Trace> = rec.laps().iter().filter_map(|l| analysis::Trace::new(l, e.track_length)).collect();
    let (land_scale, torque_scale) = analysis::norm(&laps);
    let bike = analysis::Bike {
        limiter: e.limiter as f32,
        max_rpm: e.max_rpm as f32,
        shift_rpm: e.shift_rpm as f32,
        travel: e.susp_max_travel,
        land_scale,
        torque_scale,
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
    // The ground under each section and the lap, from the rear wheel: the review can't know
    // the weather, and wet soil is mud.
    let mut review = review;
    let wet = rec.session.conditions == 2;
    for s in &mut review.sections {
        let end = s.section.end.min(mine.pts.len().saturating_sub(1));
        s.soil = mine.pts.get(s.section.start..=end).and_then(|p| crate::soil::profile(p, wet));
    }
    review.setup.extend(crate::soil::profile(&mine.pts, wet).as_ref().and_then(crate::soil::finding));
    // Lap-wide setup tips that come from the recording and the setup file, not the lap: sag, tyres.
    let r = rider_setup(&app, &rec);
    review.setup.extend(r.sag.as_ref().and_then(|s| crate::sag::finding(s, rec.event.susp_max_travel)));
    if let (Some(s), Some(o)) = (&r.setup, &r.opts) {
        review.setup.extend(crate::fixes::pressure_finding(s, o, r.optimal));
    }
    // The other riders, timed over this lap's own sections.
    let parts: Vec<crate::others::Part> = review
        .sections
        .iter()
        .map(|s| crate::others::Part { name: s.section.name.clone(), start: s.section.start, end: s.section.end, time: s.lap_time })
        .collect();
    let ended = rec.laps().into_iter().find(|l| l.num == this.lap).and_then(|l| l.samples.last().map(|s| s.t)).unwrap_or(0.0);
    let rivals = crate::others::rivals(&rec, &parts, this.time_ms, ended, rec.event.event_type == 2);
    Ok(ReviewOut { track_id: summary.track_id, track_name: summary.track_name, lap: this, reference, review, rivals })
}

/// The setup the rider had on for a recording, as far as the coach could find and read it.
struct RiderSetup {
    name: String,
    file: Option<PathBuf>,
    setup: Option<crate::stp::Setup>,
    opts: Option<crate::bikecfg::BikeOptions>,
    why: Option<String>,
    sag: Option<crate::sag::Sag>,
    /// The pressure each tyre the setup runs is made for, kPa.
    optimal: [Option<f32>; 2],
}

fn rider_setup(app: &AppHandle, rec: &Recording) -> RiderSetup {
    let cfg = load_config(app);
    let e = &rec.event;
    let raw = rec.session.setup.clone();
    let name = raw.trim_start_matches(':').to_string();
    let bike_cfg = crate::bikecfg::load_cfg(&cfg.mods_path, &e.bike_id);
    let mut opts = bike_cfg.as_ref().map(crate::bikecfg::options);
    // The swingarm's lengths are in the bike's `.geom`, not its cfg.
    if let (Some(bc), Some(o)) = (&bike_cfg, opts.as_mut()) {
        if let Some(sw) = crate::bikecfg::load_geom(&cfg.mods_path, &e.bike_id, bc).as_ref().and_then(crate::bikecfg::swingarm) {
            o.insert(crate::stp::Field::SwingarmLength, sw);
        }
    }
    let file = crate::stp::locate(&cfg.profiles_dir(), &raw, &e.track_id, &e.bike_id);
    let setup = file
        .as_ref()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| crate::stp::Setup::parse(&b, usize::try_from(e.gears).ok()).ok())
        .filter(|s| s.bike_id() == e.bike_id);
    // The tyres the setup runs: their pressure lists join the bike's, with what they're made for.
    let mut optimal = [None, None];
    if let (Some(bc), Some(s), Some(o)) = (&bike_cfg, &setup, opts.as_mut()) {
        use crate::stp::Field;
        let wheels = [(Field::FrontTyre, Field::FrontPressure), (Field::RearTyre, Field::RearPressure)];
        for (k, (tyre, pressure)) in wheels.into_iter().enumerate() {
            if let Some(t) = crate::tyres::load(&cfg.mods_path, &cfg.install_dir(), bc, k, s.get(tyre)) {
                o.insert(pressure, t.pressure);
                optimal[k] = Some(t.optimal);
            }
        }
    }
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
    RiderSetup { name, file, setup, opts, why, sag: crate::sag::measure(rec), optimal }
}

/// Every fix for the lap's setup tips, the sag and tyre ones included when they're asked for.
fn all_fixes(r: &RiderSetup, skills: &[String], travel: [f32; 2]) -> Vec<crate::fixes::Fix> {
    let mut fixes = crate::fixes::plan(skills, r.setup.as_ref(), r.opts.as_ref());
    let asked = |s: &str| skills.iter().any(|k| k == s);
    if let Some(off) = r.sag.as_ref().and_then(|s| crate::sag::rear_off(s, travel[1])) {
        let fix = crate::fixes::sag_fix(off, r.setup.as_ref(), r.opts.as_ref());
        if asked(&fix.skill) {
            fixes.push(fix);
        }
    }
    if let (Some(s), Some(o)) = (&r.setup, &r.opts) {
        if let Some(fix) = crate::fixes::pressure_fix(s, o, r.optimal) {
            if asked(&fix.skill) {
                fixes.push(fix);
            }
        }
    }
    fixes
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
    /// The sag measured in this session, standing still or riding.
    pub sag: Option<crate::sag::Sag>,
    /// The most of each end's travel this session used, as a share: what the feel check
    /// holds "it bottoms" and "it's harsh" against.
    pub travel_used: Option<[f32; 2]>,
}

/// The rider's setup name without any "(coach)" the coach added: copies of a copy are numbered.
fn setup_base(file: &Path) -> String {
    crate::stp::base_name(file.file_stem().and_then(|s| s.to_str()).unwrap_or("setup")).to_string()
}

/// The changes behind a lap's setup tips, against the setup the rider had on.
#[tauri::command]
pub fn coach_setup_plan(app: AppHandle, path: String, skills: Vec<String>) -> Result<SetupPlan, String> {
    let rec = load(&path)?;
    let r = rider_setup(&app, &rec);
    let fixes = all_fixes(&r, &skills, rec.event.susp_max_travel);
    let save_as = r.file.as_deref().and_then(|f| {
        let dir = f.parent()?;
        crate::stp::coach_names(&setup_base(f)).find(|n| !dir.join(format!("{n}.stp")).exists())
    });
    let (sag, travel_used) = (r.sag, crate::sag::travel_used(&rec));
    Ok(SetupPlan { name: r.name, file: r.file.map(|p| p.display().to_string()), save_as, why: r.why, fixes, sag, travel_used })
}

/// Saves a lap's setup fixes as a new setup beside the rider's own and returns its name.
/// Never overwrites a file: the rider's setup stays as it was.
#[tauri::command]
pub fn coach_save_setup(app: AppHandle, path: String, skills: Vec<String>) -> Result<String, String> {
    let rec = load(&path)?;
    let r = rider_setup(&app, &rec);
    let fixes = all_fixes(&r, &skills, rec.event.susp_max_travel);
    let (Some(file), Some(setup), Some(opts)) = (r.file, r.setup, r.opts) else {
        return Err(r.why.unwrap_or_else(|| "The coach can't change this setup.".into()));
    };
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuesOut {
    /// The file the recorder reads, next to the sessions.
    pub file: String,
    pub cues: Vec<crate::cues::CueOut>,
}

/// Writes the live cues for this lap's track and bike: the few calls the recorder shows in
/// practice, from where this lap loses time to the fastest one.
#[tauri::command]
pub fn coach_write_cues(
    app: AppHandle,
    path: String,
    lap: i32,
    level: crate::cues::Level,
    amount: crate::cues::Amount,
) -> Result<CuesOut, String> {
    let out = coach_review(app.clone(), path.clone(), lap, None, None, None)?;
    let rec = load(&path)?;
    // The fast lap says where each call goes.
    let r = &out.reference;
    let ref_rec = if r.path == path { None } else { Some(load(&r.path)?) };
    let fast = trace(ref_rec.as_ref().unwrap_or(&rec), r.lap)?;
    let points = analysis::cue_points(&fast, &analysis::sections(&fast));
    let cues = crate::cues::pick(&points, &out.review, level, amount);
    let cfg = load_config(&app);
    let dir = session_dirs(&cfg)
        .into_iter()
        .find_map(|d| d.parent().map(|p| p.join("cues")))
        .ok_or("The game's user folder wasn't found.")?;
    fs::create_dir_all(&dir).map_err(err)?;
    let file = dir.join(crate::cues::file_name(&rec.event.track_id, &rec.event.bike_id));
    // Written aside and moved in, so the recorder never reads half a file.
    let tmp = dir.join(format!("{}.tmp", crate::cues::file_name(&rec.event.track_id, &rec.event.bike_id)));
    fs::write(&tmp, crate::cues::write(rec.event.track_length, &cues, amount)).map_err(err)?;
    fs::rename(&tmp, &file).map_err(err)?;
    // The HUD sheet beside it: the fast lap for the gap and the ghost, and each section's tip.
    // The sag prompt asks for a stop when this session has no standing-still sag yet.
    let hud_name = crate::hudsheet::file_name(&rec.event.track_id, &rec.event.bike_id);
    let parts = crate::hudsheet::parts(&out.review, rec.event.track_length);
    let flags = if crate::sag::measure(&rec).is_some_and(|s| s.still) { 0 } else { crate::hudsheet::SAG_PROMPT };
    let hud_tmp = dir.join(format!("{hud_name}.tmp"));
    fs::write(&hud_tmp, crate::hudsheet::write(rec.event.track_length, &fast, &parts, flags)).map_err(err)?;
    fs::rename(&hud_tmp, dir.join(&hud_name)).map_err(err)?;
    Ok(CuesOut { file: file.display().to_string(), cues })
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
    // Everyone else the recorder saw, for where the track will wear.
    let me = crate::others::local_num(&rec);
    let others: Vec<[f32; 2]> =
        rec.frames.iter().flat_map(|f| &f.bikes).filter(|b| Some(b.num) != me && !b.crashed).map(|b| [b.x, b.z]).collect();
    Ok(Some(crate::lines::lines(&laps, &reference, &others)))
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
            event_type: 1,
            track_length: 1000.0,
            limiter: 0,
            complete: true,
            laps: laps
                .iter()
                .map(|&(num, time_ms, whole)| LapSummary {
                    num,
                    path: path.into(),
                    stint: 0,
                    time_ms,
                    invalid: false,
                    whole,
                    issue: None,
                    crashed: false,
                    ridden_ms: time_ms,
                })
                .collect(),
            setup: String::new(),
            best_ms: laps.iter().filter(|l| l.2).map(|l| l.1).min(),
            stints: vec![Stint { path: path.into(), started: path.into(), setup: String::new() }],
        }
    }

    /// A stint of one event. Its file is its stamp, and each lap took as long as it says.
    fn stint(started: &str, bike: &str, event_type: i32, laps: &[(i32, i32)]) -> SessionSummary {
        let whole: Vec<(i32, i32, bool)> = laps.iter().map(|&(num, time_ms)| (num, time_ms, true)).collect();
        let mut s = session(started, "indiana", bike, &whole);
        s.rider = "Frost".into();
        s.event_type = event_type;
        s
    }

    #[test]
    fn the_stints_of_one_event_are_one_session() {
        let out = group_sessions(vec![
            stint("20260915-100000-000", "kx450", 1, &[(0, 60_000), (1, 59_000)]),
            stint("20260915-101500-000", "kx450", 1, &[(0, 58_000)]),
            stint("20260915-104500-000", "kx450", 1, &[(0, 61_000)]),
        ]);
        assert_eq!(out.len(), 1, "going out and back in is still one session");
        assert_eq!(out[0].laps.len(), 4);
        assert_eq!(out[0].best_ms, Some(58_000), "the best lap is across the whole session");
        assert_eq!(out[0].started, "20260915-100000-000", "it started when its first stint did");
        let stints: Vec<i32> = out[0].laps.iter().map(|l| l.stint).collect();
        assert_eq!(stints, [0, 0, 1, 2], "every lap knows the stint it was ridden in");
        assert_eq!(out[0].laps[3].path, "20260915-104500-000", "and the file it's in");
        assert_eq!(out[0].stints.len(), 3);
    }

    #[test]
    fn another_event_is_another_session() {
        let out = group_sessions(vec![
            stint("20260915-100000-000", "kx450", 1, &[(0, 60_000)]),
            stint("20260915-101000-000", "yz250", 1, &[(0, 60_000)]),
            stint("20260915-102000-000", "kx450", 2, &[(0, 60_000)]),
        ]);
        assert_eq!(out.len(), 3, "another bike, or a race rather than testing, stands on its own");
    }

    #[test]
    fn a_long_break_starts_a_new_session() {
        let out = group_sessions(vec![
            stint("20260915-100000-000", "kx450", 1, &[(0, 60_000)]),
            stint("20260915-140000-000", "kx450", 1, &[(0, 60_000)]),
        ]);
        assert_eq!(out.len(), 2, "four hours later is a new session");
    }

    /// The break is measured from when the rider came off track, not from when the stint
    /// started: two hours on track and another ride half an hour later is one session.
    #[test]
    fn the_break_is_measured_from_the_end_of_a_stint() {
        let long: Vec<(i32, i32)> = (0..120).map(|k| (k, 60_000)).collect();
        let out = group_sessions(vec![
            stint("20260915-100000-000", "kx450", 1, &long),
            stint("20260915-143000-000", "kx450", 1, &[(0, 60_000)]),
        ]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_stint_after_midnight_belongs_to_the_evening_before() {
        let out = group_sessions(vec![
            stint("20260930-235000-000", "kx450", 1, &[(0, 60_000)]),
            stint("20261001-000500-000", "kx450", 1, &[(0, 60_000)]),
        ]);
        assert_eq!(out.len(), 1, "fifteen minutes, over the end of a month");
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
