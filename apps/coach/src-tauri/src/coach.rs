//! MXB Coach's commands: find the recorder's session files, list them, review a lap against a
//! faster one, and put the recorder plugin where the game loads it.
//!
//! The recorder writes to `<game user folder>\mxbcoach\sessions\*.mxbc`. Summaries are cached
//! in `<data dir>\coach\index.json`, keyed by path, size and modified time, so listing a
//! folder of long sessions doesn't re-read every one of them.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use mxb_core::config::{self, AppConfig};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::analysis::{self, Ideal, Review, Trace};
use crate::telemetry::{self, Recording};

const PLUGIN: &str = "mxbcoach.dlo";
/// Built and released with FrostMod (`Frostn1/frostmod`, `src/mxbcoach.cpp`).
const PLUGIN_URL: &str = "https://github.com/Frostn1/frostmod/releases/latest/download/mxbcoach.dlo";
/// Which release that is. Same host the app's own updater already asks, once at startup.
const PLUGIN_RELEASE: &str = "https://api.github.com/repos/Frostn1/frostmod/releases/latest";
/// What Coach last put in the plugins folder, remembered so a recorder the game has never run
/// still has a known version. Nothing else can say: the recorder only writes `recorder.ini`
/// once the game has loaded it, and versions before 0.23 never wrote one at all — which is
/// exactly the case where every warning in the app stayed silent.
///
/// Kept in Coach's own folder rather than the shared `config.json`, because MXB App's `save`
/// writes that file from its own struct and serde drops every key the struct has no field
/// for: a note left there would last only until the manager next saved anything.
const INSTALLED_FILE: &str = "recorder-installed.json";

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

/// Where the `.cue` and `.hud` sheets go: `cues/` under the same `mxbcoach` folder `hud.ini`
/// is written to, which is the one the recorder actually uses. Taking the first candidate
/// instead put the sheets somewhere the plugin never opens whenever the configured profiles
/// folder wasn't the one the game writes to — and then the cue and the trail simply never
/// appeared, with nothing on screen to say why.
pub(crate) fn sheets_dir(cfg: &AppConfig) -> Option<PathBuf> {
    crate::hud::coach_dir_of(&session_dirs(cfg)).map(|c| c.join("cues"))
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
pub(crate) fn group_sessions(mut stints: Vec<SessionSummary>) -> Vec<SessionSummary> {
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

/// Every recording in `dirs`, summarised one stint at a time, with `index_file` as a cache
/// keyed by path, size and modified time. A file that won't parse is skipped, not fatal.
///
/// The imported trainer laps are read the same way out of their own folder, with their own
/// index: they are recordings like any other, they just aren't the rider's.
pub(crate) fn summaries(dirs: &[PathBuf], index_file: Option<PathBuf>) -> Vec<SessionSummary> {
    let mut index: HashMap<String, Indexed> = index_file
        .as_ref()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut changed = false;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else { continue };
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
    out
}

/// Every session on disk, newest first: the stints of one event as one session.
fn all_sessions(app: &AppHandle) -> Vec<SessionSummary> {
    let dirs = session_dirs(&load_config(app));
    let mut out = group_sessions(summaries(&dirs, index_path(app)));
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out
}

#[tauri::command]
pub fn coach_sessions(app: AppHandle) -> Vec<SessionSummary> {
    // The folder only exists once the recorder has written to it, so the watch that couldn't
    // start when the app opened gets another go here.
    ensure_watching(&app);
    all_sessions(&app)
}

/// The sessions folders, watched while the app is open.
#[derive(Default)]
pub struct SessionWatch(pub mxb_core::paintwatch::WatchSet);

/// Told to the frontend when a recording is written or grows: the session list and the open
/// session's laps re-read themselves rather than waiting for the rider to leave and come back.
pub const SESSIONS_CHANGED: &str = "coach-sessions-changed";

/// Slower than the paint watcher's. A recording is appended to all the way through a stint, so
/// events never stop while the rider is out; the answer to each one is re-reading a file that
/// is still growing, and nobody needs that several times a second.
const SESSION_DEBOUNCE: Duration = Duration::from_secs(2);

/// Watch the sessions folders, replacing any watch already running.
pub fn watch_sessions(app: &AppHandle) {
    let Some(state) = app.try_state::<SessionWatch>() else { return };
    let dirs: Vec<String> = session_dirs(&load_config(app)).iter().map(|d| d.to_string_lossy().into_owned()).collect();
    let handle = app.clone();
    mxb_core::paintwatch::watch_folders(&state.0, "session watcher", &dirs, SESSION_DEBOUNCE, move |paths| {
        log::info!("session watcher: {} file(s) changed", paths.len());
        let _ = handle.emit(SESSIONS_CHANGED, ());
    });
}

fn ensure_watching(app: &AppHandle) {
    let watching = app.try_state::<SessionWatch>().is_some_and(|s| mxb_core::paintwatch::is_watching(&s.0));
    if !watching {
        watch_sessions(app);
    }
}

fn load(path: &str) -> Result<Recording, String> {
    telemetry::parse(&fs::read(path).map_err(err)?).map_err(err)
}

fn trace(rec: &Recording, num: i32) -> Result<Trace, String> {
    let lap = rec.laps().into_iter().find(|l| l.num == num).ok_or_else(|| format!("Lap {num} isn't in that session."))?;
    Trace::new(&lap, rec.event.track_length).ok_or_else(|| format!("Lap {num} is too short to review."))
}

/// Where a reference lap came from. The rider is told, because it changes what the comparison
/// means: another rider's lap isn't theirs to ride back, and the ideal lap was never ridden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    /// A lap the rider rode themselves.
    Own,
    /// A lap from another rider, imported into the coach.
    Imported,
    /// The rider's own best sections added up: a time that was never ridden whole.
    Ideal,
}

/// A lap somewhere on disk — or the ideal lap, which is nowhere.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapRef {
    pub path: String,
    pub lap: i32,
    pub time_ms: i32,
    pub started: String,
    pub bike_name: String,
    /// The bike it was ridden on. A 250 and a 450 don't take a corner the same way, so the
    /// review says so when it isn't the same one.
    pub bike_id: String,
    /// Whose lap it is, as the recorder saved it.
    pub rider: String,
    pub kind: RefKind,
}

impl LapRef {
    /// Lap `l` of session `s`, as something to be reviewed against.
    fn of(s: &SessionSummary, l: &LapSummary, kind: RefKind) -> LapRef {
        LapRef {
            // The lap's own file and the stint it was ridden in: a session spans several.
            path: l.path.clone(),
            lap: l.num,
            time_ms: l.time_ms,
            started: s.stints.get(l.stint.max(0) as usize).map_or(&s.started, |x| &x.started).clone(),
            bike_name: s.bike_name.clone(),
            bike_id: s.bike_id.clone(),
            rider: s.rider.clone(),
            kind,
        }
    }
}

/// The fastest comparable lap on this track, other than `exclude`, across every session the
/// rider has ridden here. The same bike wins over a faster lap on a different one: a 250 and a
/// 450 don't take a corner the same way.
fn best_reference(sessions: &[SessionSummary], track_id: &str, bike_id: &str, exclude: Option<(&str, i32)>) -> Option<LapRef> {
    sessions
        .iter()
        .filter(|s| s.track_id == track_id)
        .flat_map(|s| s.laps.iter().filter(|l| l.comparable()).map(move |l| (s, l)))
        .filter(|(_, l)| exclude != Some((l.path.as_str(), l.num)))
        .min_by_key(|(s, l)| (s.bike_id != bike_id, l.time_ms))
        .map(|(s, l)| LapRef::of(s, l, RefKind::Own))
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
    /// How many of the rider's own laps the ideal lap was stitched from, when that is what
    /// this lap is held against. None otherwise.
    pub ideal_from: Option<i32>,
    /// Other riders in the session worth comparing with, where the recorder saw them.
    pub rivals: Vec<crate::others::Rival>,
    /// No lap of the rider's on this track is faster than this one. Held against a slower lap
    /// the review has nothing to say — every section is a gain — so the page says why rather
    /// than showing an empty list of tips, and offers the ideal lap instead.
    pub best_here: bool,
}

/// Whether no whole lap of the rider's on this track is quicker than `time_ms`. `this` is the
/// lap being asked about, which isn't compared with itself.
fn nothing_faster(sessions: &[SessionSummary], track_id: &str, this: (&str, i32), time_ms: i32) -> bool {
    let faster = sessions
        .iter()
        .filter(|s| s.track_id == track_id)
        .flat_map(|s| s.laps.iter())
        .filter(|l| l.comparable() && (l.path.as_str(), l.num) != this)
        .any(|l| l.time_ms < time_ms);
    // A lap the game never timed has no time to be anybody's best.
    time_ms > 0 && !faster
}

/// How far two centrelines can differ and still be the same track. A rebuilt layout is a
/// different length, and the two laps then sit on different grids.
const TRACK_LENGTH_SLACK_M: f32 = 1.0;

/// The rider's own best time for each section of this track, across every session they have
/// ridden here: the ideal lap. The sections come from the fastest lap on the track, which is
/// the grid every lap is timed on; the times come from every whole lap ridden here, on the bike
/// this lap is on where there are any — a 250 and a 450 don't take a corner the same way.
///
/// Gives back the sections, a target for each, how many laps went into it, and the bike they
/// were ridden on.
fn ideal_targets(
    sessions: &[SessionSummary],
    summary: &SessionSummary,
) -> Result<(Vec<analysis::Section>, Vec<f32>, i32, String), String> {
    let grid = best_reference(sessions, &summary.track_id, &summary.bike_id, None)
        .ok_or("Ride one whole lap on this track first: the ideal lap is built out of your own laps.")?;
    let sections = analysis::sections(&trace(&load(&grid.path)?, grid.lap)?);
    let on_this_bike = sessions
        .iter()
        .any(|s| s.track_id == summary.track_id && s.bike_id == summary.bike_id && s.laps.iter().any(|l| l.comparable()));
    let mut laps: Vec<(i32, Trace)> = Vec::new();
    for s in sessions.iter().filter(|s| s.track_id == summary.track_id) {
        if on_this_bike && s.bike_id != summary.bike_id {
            continue;
        }
        for st in &s.stints {
            let Ok(rec) = load(&st.path) else { continue };
            for l in rec.laps().iter().filter(|l| l.whole && !l.invalid) {
                if let Some(t) = Trace::new(l, rec.event.track_length) {
                    laps.push((laps.len() as i32, t));
                }
            }
        }
    }
    let ideal = analysis::ideal(&sections, &laps)
        .ok_or("There aren't enough whole laps of yours on this track to build an ideal lap.")?;
    let bike = if on_this_bike { summary.bike_name.clone() } else { grid.bike_name.clone() };
    Ok((sections, ideal.sections.iter().map(|b| b.best).collect(), laps.len() as i32, bike))
}

/// Reviews lap `lap` of `path` against `ref_path`/`ref_lap` — which can be an imported lap of
/// another rider's — against the rider's ideal lap on this track when `ideal`, or against the
/// fastest other lap on the track when none is given. `solo`, or no other lap to compare with,
/// reviews it on its own instead.
#[tauri::command]
pub fn coach_review(
    app: AppHandle,
    path: String,
    lap: i32,
    ref_path: Option<String>,
    ref_lap: Option<i32>,
    solo: Option<bool>,
    ideal: Option<bool>,
) -> Result<ReviewOut, String> {
    let rec = load(&path)?;
    let summary = summarize(Path::new(&path), &rec);
    // Read once: the reference, the ideal lap's targets and whether anything here is faster
    // all ask the same question of the same list.
    let sessions = all_sessions(&app);
    let alone = solo.unwrap_or(false);
    let want_ideal = ideal.unwrap_or(false) && !alone;
    let reference = if alone || want_ideal {
        None
    } else {
        match (ref_path, ref_lap) {
            (Some(p), Some(n)) => {
                let s = if p == path { summary.clone() } else { summarize(Path::new(&p), &load(&p)?) };
                let l = s.laps.iter().find(|l| l.num == n).ok_or_else(|| format!("Lap {n} isn't in that session."))?;
                let kind = if crate::imports::is_import(&app, &p) { RefKind::Imported } else { RefKind::Own };
                Some(LapRef::of(&s, l, kind))
            }
            _ => best_reference(&sessions, &summary.track_id, &summary.bike_id, Some((&path, lap))),
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
    let this = LapRef {
        path,
        lap,
        time_ms,
        started: summary.started.clone(),
        bike_name: summary.bike_name.clone(),
        bike_id: summary.bike_id.clone(),
        rider: summary.rider.clone(),
        kind: RefKind::Own,
    };
    let mut ideal_from = None;
    let (review, reference) = if want_ideal {
        let (secs, targets, from, bike_name) = ideal_targets(&sessions, &summary)?;
        ideal_from = Some(from);
        // The ideal lap is nowhere on disk: it is a time for each section, so it has no file,
        // no lap number and no day it was ridden.
        let r = LapRef {
            path: String::new(),
            lap: -1,
            time_ms: (targets.iter().sum::<f32>() * 1000.0).round() as i32,
            started: String::new(),
            bike_name,
            bike_id: summary.bike_id.clone(),
            rider: summary.rider.clone(),
            kind: RefKind::Ideal,
        };
        (analysis::against_targets(&mine, &secs, &targets, bike), r)
    } else {
        match reference {
            Some(reference) => {
                let ref_rec = if reference.path == this.path { None } else { Some(load(&reference.path)?) };
                let ref_rec = ref_rec.as_ref().unwrap_or(&rec);
                if ref_rec.event.track_id != rec.event.track_id {
                    return Err("That lap is on a different track.".into());
                }
                // A recording from another rider can carry the right track id and still be a
                // different build of it. A changed layout is a different centreline, so the two
                // laps sit on different grids and every section would be measured elsewhere.
                if (ref_rec.event.track_length - rec.event.track_length).abs() > TRACK_LENGTH_SLACK_M {
                    return Err(format!(
                        "That lap is on a different version of this track: its centreline is {:.0} m against your {:.0} m, \
                         so the two can't be lined up.",
                        ref_rec.event.track_length, rec.event.track_length
                    ));
                }
                (analysis::review(&mine, &trace(ref_rec, reference.lap)?, bike), reference)
            }
            None => (analysis::solo(&mine, bike), this.clone()),
        }
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
    let best_here = nothing_faster(&sessions, &summary.track_id, (&this.path, this.lap), this.time_ms);
    Ok(ReviewOut {
        track_id: summary.track_id,
        track_name: summary.track_name,
        lap: this,
        reference,
        review,
        ideal_from,
        rivals,
        best_here,
    })
}

/// Where a setup the coach saves goes, and what it is called. It never writes over a file.
enum Save {
    /// Beside the rider's own setup, numbered off its name.
    Beside(PathBuf),
    /// Under this track, named after it. The rider rode the game's default, so there is no
    /// name of theirs to build on.
    Fresh(PathBuf, String),
}

impl Save {
    fn dir(&self) -> &Path {
        match self {
            Save::Beside(f) => f.parent().unwrap_or(Path::new(".")),
            Save::Fresh(d, _) => d,
        }
    }

    /// The names to try, in order.
    fn names(&self) -> Vec<String> {
        match self {
            Save::Beside(f) => crate::stp::coach_names(&setup_base(f)).collect(),
            Save::Fresh(_, track) => crate::stp::fresh_names(track).collect(),
        }
    }

    /// The first name nothing has taken.
    fn free(&self) -> Option<String> {
        let dir = self.dir();
        self.names().into_iter().find(|n| !dir.join(format!("{n}.stp")).exists())
    }
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
    /// Where a copy with the coach's changes would go.
    save: Option<Save>,
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
    let profiles = cfg.profiles_dir();
    let gears = usize::try_from(e.gears).ok().filter(|&g| g > 0);
    let read = |p: &Path| {
        fs::read(p)
            .ok()
            .and_then(|b| crate::stp::Setup::parse(&b, gears).ok())
            .filter(|s| s.bike_id() == e.bike_id)
    };
    let file = crate::stp::locate(&profiles, &raw, &e.track_id, &e.bike_id);
    // Riding the game's default used to be the end of it: the coach asked the rider to go and
    // save a setup in the garage first. Now it writes them one. Best is another setup of their
    // own for this bike, which keeps every slot the coach doesn't model at a value the game
    // itself wrote; failing that, the bike's own defaults out of its cfg.
    let (setup, save) = match file.as_deref().and_then(read) {
        Some(s) => (Some(s), file.clone().map(Save::Beside)),
        None => {
            let donor = crate::stp::setups_for_bike(&profiles, &e.track_id, &e.bike_id).into_iter().find_map(|p| read(&p));
            let built = || {
                let g = gears?;
                let slots = crate::bikecfg::default_slots(bike_cfg.as_ref()?, g)?;
                crate::stp::Setup::build(&e.bike_id, g, &slots).ok()
            };
            let s = donor.or_else(built);
            let where_to = crate::stp::fresh_dir(&profiles, &e.track_id, &e.bike_id);
            let save = match (&s, where_to) {
                (Some(_), Some(d)) => Some(Save::Fresh(d, e.track_id.clone())),
                _ => None,
            };
            (s, save)
        }
    };
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
    let why = if opts.is_none() {
        Some("The bike's own settings couldn't be read, so the coach can't tell how far each one goes.".into())
    } else if setup.is_none() {
        Some(if name.is_empty() || name.eq_ignore_ascii_case("default") {
            format!("The coach couldn't read {}'s own settings, so it has nothing to build a setup from.", e.bike_name)
        } else {
            format!("Your setup \"{name}\" couldn't be read, and the coach found no other setup for this bike.")
        })
    } else if save.is_none() {
        Some("The coach couldn't find a setups folder of yours to save into.".into())
    } else {
        None
    };
    RiderSetup { name, file, setup, opts, why, sag: crate::sag::measure(rec), optimal, save }
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
    // Last, over the whole list: what the save will really do decides what the rider is told
    // it will do. Both commands come through here, so they can't disagree.
    crate::fixes::settle(&mut fixes, r.setup.as_ref(), r.opts.as_ref());
    fixes
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPlan {
    /// The setup the rider had on.
    pub name: String,
    pub file: Option<String>,
    /// The name a saved copy gets: the next free "(coach)", "(coach 2)" … beside the rider's
    /// own, or "Coach <track>" when they rode the game's default and have none here.
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
    let save_as = r.save.as_ref().and_then(Save::free);
    let (sag, travel_used) = (r.sag, crate::sag::travel_used(&rec));
    Ok(SetupPlan { name: r.name, file: r.file.map(|p| p.display().to_string()), save_as, why: r.why, fixes, sag, travel_used })
}

/// A setup the coach wrote: what it is called, what it really changed, and whether the game
/// will load it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSetup {
    pub name: String,
    /// Named rather than counted, so the rider can check each one in the garage. A setting two
    /// tips wanted opposite ways is not in here, and was never claimed as a change either.
    pub changed: Vec<crate::stp::Field>,
    /// The game is pointed at it for practice on this track.
    pub selected: bool,
    /// It wasn't, because MX Bikes is open. The rider can pick it in the garage now, or press
    /// Select once the game is closed.
    pub game_open: bool,
}

/// Point the game at a setup for practice on this track, by writing its own `default.ini`
/// beside the setups (see `stp::select_default`).
///
/// Only with the game closed. MX Bikes holds the garage in memory and writes this file itself
/// when it closes, so a change made while it runs is silently undone — which would be worse
/// than not making it, because the rider would be told it had worked.
fn select(dir: &Path, name: &str, wet: bool) -> Result<bool, String> {
    if mxb_core::gamewindow::is_game_running() {
        return Ok(false);
    }
    crate::stp::select_default(dir, name, wet)?;
    Ok(true)
}

/// Saves a lap's setup fixes as a new setup — beside the rider's own, or as one of their own
/// when they rode the game's default — and points the game at it. Never overwrites a file, and
/// only the practice keys of `default.ini` are touched.
#[tauri::command]
pub fn coach_save_setup(app: AppHandle, path: String, skills: Vec<String>) -> Result<SavedSetup, String> {
    let rec = load(&path)?;
    let r = rider_setup(&app, &rec);
    let fixes = all_fixes(&r, &skills, rec.event.susp_max_travel);
    let (Some(setup), Some(opts), Some(save)) = (r.setup, r.opts, r.save) else {
        return Err(r.why.unwrap_or_else(|| "The coach can't change this setup.".into()));
    };
    let (out, changed) = crate::fixes::apply(&setup, &fixes, &opts);
    if changed.is_empty() {
        return Err("There's nothing in this setup the coach can change.".into());
    }
    let dir = save.dir().to_path_buf();
    fs::create_dir_all(&dir).map_err(err)?;
    for name in save.names() {
        match fs::OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{name}.stp"))) {
            Ok(mut f) => {
                std::io::Write::write_all(&mut f, out.bytes()).map_err(err)?;
                // A setup the rider has to go and find in the garage is a setup they ride
                // without. Failing to select it is not failing to save it, so it is reported
                // rather than raised.
                let selected = select(&dir, &name, rec.session.conditions == 2).unwrap_or(false);
                return Ok(SavedSetup { name, changed, selected, game_open: !selected });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(err(e)),
        }
    }
    Err("There are already too many coach setups for this one.".into())
}

/// Point the game at a setup the coach already saved: for when it was written while MX Bikes
/// was open, so selecting it then would not have stuck.
#[tauri::command]
pub fn coach_select_setup(app: AppHandle, path: String, name: String) -> Result<SavedSetup, String> {
    let rec = load(&path)?;
    let e = &rec.event;
    let cfg = load_config(&app);
    // Through `locate`, so the coach can only ever select a setup that is really there.
    let file = crate::stp::locate(&cfg.profiles_dir(), &name, &e.track_id, &e.bike_id)
        .ok_or_else(|| format!("\"{name}\" isn't in your profiles folder any more."))?;
    let dir = file.parent().ok_or("The setup's folder couldn't be found.")?;
    let selected = select(dir, &name, rec.session.conditions == 2)?;
    // Nothing was written to the setup itself here: this only points the game at one.
    Ok(SavedSetup { name, changed: Vec::new(), selected, game_open: !selected })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CuesOut {
    /// The file the recorder reads, next to the sessions.
    pub file: String,
    pub cues: Vec<crate::cues::CueOut>,
    /// The lap the HUD's gap and ghost run against, which the rider is shown. It is the
    /// reference they picked wherever that is a lap somebody rode; the ideal lap never was,
    /// so there the HUD races the fastest lap on the track instead.
    pub ghost: LapRef,
}

/// What the rider has already been called on this track and bike, beside the session index.
/// Losing it is no worse than a fresh start: the next sheet simply repeats itself once.
fn history_path(app: &AppHandle, track: &str, bike: &str) -> Option<PathBuf> {
    Some(config::data_dir(app)?.join("coach").join("cues").join(crate::cues::history_name(track, bike)))
}

fn read_history(app: &AppHandle, track: &str, bike: &str) -> crate::cues::History {
    history_path(app, track, bike)
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_history(app: &AppHandle, track: &str, bike: &str, h: &crate::cues::History) {
    let Some(path) = history_path(app, track, bike) else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec(h) {
        let _ = fs::write(path, json);
    }
}

/// The newest lap of this track and bike worth coaching from, so a sheet written mid-session
/// is about the laps the rider is riding now rather than the one they opened the page on.
fn newest_lap(app: &AppHandle, track: &str, bike: &str) -> Option<(String, i32)> {
    let sessions = all_sessions(app);
    sessions
        .iter()
        .filter(|s| s.track_id == track && s.bike_id == bike)
        .flat_map(|s| s.laps.iter())
        .filter(|l| l.comparable())
        .next_back()
        .map(|l| (l.path.clone(), l.num))
}

/// Writes the live cues for this lap's track and bike: the few calls the recorder shows in
/// practice, from where this lap loses time to the lap it is held against. The reference is the
/// one the rider picked in the review, so the cue sheet and the HUD sheet beside it are both
/// against the lap they chose.
///
/// `latest` writes them for the newest lap on this track and bike instead of the one named,
/// which is what the review page asks for while the rider is still out: a sheet is only worth
/// anything if it is about the laps they are riding now. The reference stands either way: it is
/// the lap they chose to be held against, not the lap being reviewed.
#[tauri::command]
pub fn coach_write_cues(
    app: AppHandle,
    path: String,
    lap: i32,
    level: crate::cues::Level,
    amount: crate::cues::Amount,
    ref_path: Option<String>,
    ref_lap: Option<i32>,
    ideal: Option<bool>,
    latest: Option<bool>,
) -> Result<CuesOut, String> {
    // Which lap is coached is settled before the review, so the reference the rider picked is
    // applied to the lap the calls actually come from.
    let (path, lap) = match latest.unwrap_or(false) {
        true => {
            let head = summarize(Path::new(&path), &load(&path)?);
            newest_lap(&app, &head.track_id, &head.bike_id).unwrap_or((path, lap))
        }
        false => (path, lap),
    };
    let out = coach_review(app.clone(), path.clone(), lap, ref_path, ref_lap, None, ideal)?;
    let rec = load(&path)?;
    // The lap the calls are placed on, and the one the HUD races. The ideal lap can't be
    // either: there is no line to draw a ghost from and no ghost to run a gap against, so the
    // fastest lap on the track stands in and the rider is told which lap that is.
    let ghost = match out.reference.kind {
        RefKind::Ideal => best_reference(&all_sessions(&app), &out.track_id, &rec.event.bike_id, None)
            .ok_or("There's no whole lap on this track for the in-game HUD to race against.")?,
        _ => out.reference.clone(),
    };
    let r = &ghost;
    let ref_rec = if r.path == path { None } else { Some(load(&r.path)?) };
    let fast = trace(ref_rec.as_ref().unwrap_or(&rec), r.lap)?;
    let points = analysis::cue_points(&fast, &analysis::sections(&fast));
    // What the last sheets said, so this one moves on rather than repeating itself.
    let seen = read_history(&app, &rec.event.track_id, &rec.event.bike_id);
    let picked = crate::cues::pick(&points, &out.review, level, amount, &seen);
    let (cues, next) = (picked.cues, picked.history);
    let cfg = load_config(&app);
    let dir = sheets_dir(&cfg).ok_or("The game's user folder wasn't found.")?;
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
    // Only once the sheet is really on disk: a write that failed is a sheet the rider never
    // heard, and it would be wrong to count it against them.
    write_history(&app, &rec.event.track_id, &rec.event.bike_id, &next);
    Ok(CuesOut { file: file.display().to_string(), cues, ghost })
}

/// The session's lines and its track, with the stint the lap under review was ridden in.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinesOut {
    #[serde(flatten)]
    pub lines: crate::lines::Lines,
    /// Which stint of the session the recording under review is. The game numbers laps per
    /// stint, so it takes both to pick the rider's own line out of the rest.
    pub stint: i32,
}

/// How the session's lines and the track changed, against the fastest lap on the track; see
/// `lines.rs`. None until there is a lap to compare with.
///
/// Every stint of the session, not just the one recording: a rider who goes out, comes in and
/// goes back out has one session in three files, and the notes take several laps through the
/// same corner before one line can be told from another.
///
/// Off the main thread: reading every stint of a long session is several recordings' worth of
/// parsing, the same reason [`coach_ground`] is off it.
#[tauri::command]
pub async fn coach_lines(app: AppHandle, path: String) -> Result<Option<LinesOut>, String> {
    tauri::async_runtime::spawn_blocking(move || lines_for(&app, &path)).await.map_err(err)?
}

fn lines_for(app: &AppHandle, path: &str) -> Result<Option<LinesOut>, String> {
    let sessions = all_sessions(app);
    let rec = load(path)?;
    // The session this recording belongs to. A recording that isn't on disk to be grouped —
    // one the rider opened from somewhere else — is a session of its own.
    let summary = match sessions.iter().find(|s| s.stints.iter().any(|x| x.path == path)) {
        Some(s) => s.clone(),
        None => summarize(Path::new(path), &rec),
    };
    let Some(r) = best_reference(&sessions, &summary.track_id, &summary.bike_id, None) else {
        return Ok(None);
    };
    let ref_rec = if r.path == path { None } else { Some(load(&r.path)?) };
    let reference = trace(ref_rec.as_ref().unwrap_or(&rec), r.lap)?;
    let mut laps: Vec<(crate::lines::LapId, Trace)> = Vec::new();
    // Everyone else the recorder saw, for where the track will wear.
    let mut others: Vec<[f32; 2]> = Vec::new();
    for (i, st) in summary.stints.iter().enumerate() {
        let Ok(rec) = load(&st.path) else { continue };
        let stint = i as i32;
        for l in rec.laps().iter().filter(|l| l.whole && !l.invalid) {
            if let Some(t) = Trace::new(l, rec.event.track_length) {
                laps.push((crate::lines::LapId { lap: l.num, stint }, t));
            }
        }
        let me = crate::others::local_num(&rec);
        others.extend(
            rec.frames.iter().flat_map(|f| &f.bikes).filter(|b| Some(b.num) != me && !b.crashed).map(|b| [b.x, b.z]),
        );
    }
    let stint = summary.stints.iter().position(|x| x.path == path).unwrap_or(0) as i32;
    Ok(Some(LinesOut { lines: crate::lines::lines(&laps, &reference, &others), stint }))
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
    /// The laps didn't sit steadily above this terrain, so `lift` is the best guess rather
    /// than a measurement and the lines may float or sink a little. The track is still drawn:
    /// a rider looking at their lap wants to see their track, and the ground built from the
    /// laps is a poor substitute for it.
    pub rough_fit: bool,
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
    let fit = crate::ground::measure(i.width as usize, i.height as usize, i.metres_per_sample, &master.heights, &points);
    // The track is shown whenever its terrain can be read, exactly as MXB App shows it: the
    // check below decides how high to hang the lines over it, not whether the rider gets to
    // see their track at all. Refusing on a poor fit meant a rutted lap came back as a blurred
    // grid of its own telemetry, which is nobody's idea of the circuit they just rode.
    let ground = Ground {
        path: src.path,
        prefix: src.prefix,
        name: src.name,
        lift: fit.lift.unwrap_or(fit.median_lift),
        rough_fit: fit.lift.is_none(),
    };
    // Said only when it is true, and only about the lines: what was measured, so a lap sitting
    // oddly over the ground can be looked into rather than guessed at.
    let why = fit.lift.is_none().then(|| {
        if fit.on_grid < points.len() / 2 {
            format!(
                "only {} of {} points on your laps land on this terrain, so your lines may not sit on it",
                fit.on_grid, fit.offered
            )
        } else {
            format!(
                "the height of your laps above the ground wanders by {:.1} m, so your lines may float or sink a little",
                fit.spread
            )
        }
    });
    Ok(GroundAnswer { ground: Some(ground), why })
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

/// Point Coach at the MX Bikes folder itself.
///
/// It shares one `config.json` with MXB App, so this is the same setting the manager has — but
/// until now only the manager could set it, and everything Coach does with the recorder is
/// gated on it. A rider whose game isn't where autodetect looks was told to go and open MXB
/// App, which is the whole "open the app, set it, close it, come back" dance.
#[tauri::command]
pub fn coach_set_game_dir(app: AppHandle, dir: String) -> Result<Status, String> {
    let dir = dir.trim().to_string();
    if !dir.is_empty() {
        let path = Path::new(&dir);
        if !path.is_dir() {
            return Err("That folder isn't there.".into());
        }
        // The folder the game actually runs from, so a rider who picks the mods folder or the
        // Steam library root is told now rather than after a download that goes nowhere.
        if !path.join("mxbikes.exe").is_file() {
            return Err("That isn't the MX Bikes folder: it has no mxbikes.exe in it.".into());
        }
    }
    let mut keys = serde_json::Map::new();
    keys.insert("game_path".into(), serde_json::Value::String(dir));
    config::patch_json(&app, keys).map_err(|e| format!("{e:#}"))?;
    Ok(coach_status(app))
}

fn installed_note_path(app: &AppHandle) -> Option<PathBuf> {
    config::data_dir(app).map(|d| d.join("coach").join(INSTALLED_FILE))
}

fn installed_note(app: &AppHandle) -> Option<String> {
    let bytes = fs::read(installed_note_path(app)?).ok()?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    doc.get("version").and_then(|v| v.as_str()).map(str::to_string)
}

fn write_installed_note(app: &AppHandle, version: &str) {
    let Some(path) = installed_note_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, serde_json::json!({ "version": version }).to_string());
}

/// The newest FrostMod release's version, without the `v`.
async fn latest_recorder() -> Result<String, String> {
    let client = reqwest::Client::builder().user_agent("mxb-coach").build().map_err(err)?;
    let resp = client.get(PLUGIN_RELEASE).send().await.map_err(err)?;
    if !resp.status().is_success() {
        return Err(format!("GitHub answered {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(err)?;
    let tag = body.get("tag_name").and_then(|t| t.as_str()).ok_or("no tag in the release")?;
    Ok(tag.trim_start_matches('v').to_string())
}

/// Keep the recorder current without being asked.
///
/// The manager has refreshed its own `frostmod.dlo` from `status()` for a while; nothing ever
/// did the same for `mxbcoach.dlo`, so a rider installed it once and kept it forever. A
/// recorder older than the app it serves draws nothing and says nothing, and before 0.23 it
/// couldn't even report its own version — so the app insisted everything was fine while the
/// cue, the section tip, the gap and the setup card were all silently dropped.
///
/// Returns the version now installed when it changed anything.
#[tauri::command]
pub async fn coach_refresh_plugin(app: AppHandle) -> Result<Option<String>, String> {
    let cfg = load_config(&app);
    let Some(target) = plugin_path(&cfg) else { return Ok(None) };
    let latest = latest_recorder().await?;
    // What is there now: what the game last ran, else what Coach last installed. Neither, with
    // a file present, means a recorder of unknown age — treat that as out of date, because the
    // versions that can't say are precisely the ones that are.
    let dirs = session_dirs(&cfg);
    let ran = crate::hud::coach_dir_of(&dirs).as_deref().and_then(crate::hud::recorder_version);
    let noted = installed_note(&app);
    let have = ran.or(noted);
    let current = target.is_file() && have.as_deref().is_some_and(|v| crate::hud::at_least(v, &latest));
    if current {
        return Ok(None);
    }
    coach_install_plugin(app.clone(), None).await?;
    write_installed_note(&app, &latest);
    Ok(Some(latest))
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

    /// The lap a rider is most likely to open is their fastest one, and "your best ever here"
    /// leaves that lap out — so the reference is a slower lap, every section is a gain and the
    /// review finds nothing. The page says so, and this is how it knows.
    #[test]
    fn a_lap_with_nothing_quicker_behind_it_is_their_best_here() {
        let sessions = vec![
            session("a", "indiana", "kx450", &[(0, 60_000, true), (1, 58_000, true), (2, 50_000, false)]),
            session("b", "indiana", "kx450", &[(0, 59_000, true)]),
            session("c", "erzberg", "kx450", &[(0, 50_000, true)]),
        ];
        // The 50 in that session is an out lap: a lap that can't be compared can't beat one.
        assert!(nothing_faster(&sessions, "indiana", ("a", 1), 58_000), "nothing here is quicker");
        assert!(!nothing_faster(&sessions, "indiana", ("b", 0), 59_000), "the 58 is quicker");
        // Another track's quicker laps say nothing about this one.
        assert!(nothing_faster(&sessions, "erzberg", ("c", 0), 50_000));
        // A lap the game never timed has no time to be the best with.
        assert!(!nothing_faster(&sessions, "indiana", ("a", 3), 0));
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

    /// The `.cue` and `.hud` sheets and `hud.ini` must land under one `mxbcoach` folder. They
    /// were chosen two different ways, and when the candidates disagreed the sheets went where
    /// the plugin never looks: the HUD switches worked while the cue and the trail showed
    /// nothing at all.
    #[test]
    fn the_sheets_go_where_hud_ini_goes() {
        let dir = std::env::temp_dir().join(format!("coach-sheets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (moved, default) = (dir.join("moved"), dir.join("default"));
        let dirs = [moved.join("mxbcoach").join("sessions"), default.join("mxbcoach").join("sessions")];
        // The recorder writes to the second candidate, not the first.
        std::fs::create_dir_all(&dirs[1]).unwrap();
        let coach = crate::hud::coach_dir_of(&dirs).expect("a coach folder");
        assert_eq!(coach, default.join("mxbcoach"), "the one the recorder uses");
        assert_eq!(
            coach.join("cues"),
            crate::hud::coach_dir_of(&dirs).map(|c| c.join("cues")).unwrap(),
            "the sheets sit beside hud.ini, never under the first candidate blindly"
        );
        assert_ne!(coach.join("cues"), moved.join("mxbcoach").join("cues"), "not the unused candidate");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
