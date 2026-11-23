//! The rides Cinema can make a shot from.
//!
//! Today that is MXB Coach's recordings: `<user>/mxbcoach/sessions/*.mxbc`, the same folders
//! Coach lists. A game replay (`.rpy`) is the other source the picker names; its format isn't
//! read yet, so [`Source::Replay`] lists nothing and says so rather than pretending.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use base64::Engine;
use mxb_core::config::{self, AppConfig};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::mxbc::{self, Jump, Recording};

pub fn load_config(app: &AppHandle) -> AppConfig {
    config::load(app).unwrap_or_default()
}

/// Where the recorder writes: under the configured user folder, and the game's default one.
pub fn session_dirs(cfg: &AppConfig) -> Vec<PathBuf> {
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

/// Where a ride came from.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// An MXB Coach recording.
    Coach,
    /// A game replay. Not read yet.
    Replay,
}

/// One ride in the picker.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub path: String,
    pub source: Source,
    /// When the recording started, `YYYY-MM-DDTHH:MM:SS`, from the file name the recorder
    /// gives it; the file's own time when the name says nothing.
    pub started: String,
    pub rider: String,
    pub bike_id: String,
    pub bike_name: String,
    pub track_id: String,
    pub track_name: String,
    /// Seconds of riding.
    pub duration: f32,
    pub jumps: usize,
    /// The longest time in the air, seconds.
    pub best_air: f32,
    /// m/s.
    pub top_speed: f32,
    pub size: u64,
}

#[derive(Serialize, Deserialize)]
struct Indexed {
    size: u64,
    modified: u64,
    summary: SessionSummary,
}

fn index_path(app: &AppHandle) -> Option<PathBuf> {
    Some(config::data_dir(app)?.join("cinema").join("sessions-v1.json"))
}

/// `20261025-204750-030.mxbc` -> `2026-10-25T20:47:50`.
fn started_from_name(name: &str) -> Option<String> {
    let b = name.as_bytes();
    if b.len() < 15 || b[8] != b'-' || !b[..8].iter().chain(&b[9..15]).all(u8::is_ascii_digit) {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:{}:{}",
        &name[0..4],
        &name[4..6],
        &name[6..8],
        &name[9..11],
        &name[11..13],
        &name[13..15]
    ))
}

fn summarise(path: &Path, size: u64, rec: &Recording) -> SessionSummary {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let s = &rec.samples;
    let duration = match (s.first(), s.last()) {
        (Some(a), Some(b)) => (b.t - a.t).max(0.0),
        _ => 0.0,
    };
    let js = mxbc::jumps(s);
    let e = &rec.event;
    SessionSummary {
        path: path.to_string_lossy().into_owned(),
        source: Source::Coach,
        started: started_from_name(name).unwrap_or_default(),
        rider: e.rider.clone(),
        bike_id: e.bike_id.clone(),
        bike_name: if e.bike_name.is_empty() { e.bike_id.clone() } else { e.bike_name.clone() },
        track_id: e.track_id.clone(),
        track_name: if e.track_name.is_empty() { e.track_id.clone() } else { e.track_name.clone() },
        duration,
        jumps: js.len(),
        best_air: js.iter().map(|j| j.end - j.start).fold(0.0, f32::max),
        top_speed: s.iter().map(|x| x.speed).fold(0.0, f32::max),
        size,
    }
}

pub fn load(path: &str) -> Result<Recording, String> {
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    mxbc::parse(&bytes).map_err(|e| format!("{path}: {e:#}"))
}

/// Every recording on disk, newest first. Summaries are cached by path, size and time, so only
/// a new or still-growing recording is read again.
pub fn list(app: &AppHandle) -> Vec<SessionSummary> {
    let cfg = load_config(app);
    let mut files: Vec<(PathBuf, u64, u64)> = Vec::new();
    for dir in session_dirs(&cfg) {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mxbc")) {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            files.push((p, m.len(), modified));
        }
    }
    let index_file = index_path(app);
    let mut index: HashMap<String, Indexed> = index_file
        .as_ref()
        .and_then(|f| fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let todo: Vec<&(PathBuf, u64, u64)> = files
        .iter()
        .filter(|(p, size, modified)| {
            index
                .get(&p.to_string_lossy().into_owned())
                .is_none_or(|i| i.size != *size || i.modified != *modified)
        })
        .collect();
    let fresh: Vec<(String, Indexed)> = todo
        .par_iter()
        .filter_map(|(p, size, modified)| {
            let bytes = fs::read(p).ok()?;
            let rec = mxbc::parse(&bytes).ok()?;
            let summary = summarise(p, *size, &rec);
            Some((p.to_string_lossy().into_owned(), Indexed { size: *size, modified: *modified, summary }))
        })
        .collect();
    let changed = !fresh.is_empty();
    index.extend(fresh);
    let seen: std::collections::HashSet<String> = files.iter().map(|(p, ..)| p.to_string_lossy().into_owned()).collect();
    let before = index.len();
    index.retain(|k, _| seen.contains(k));
    if let (true, Some(f)) = (changed || index.len() != before, index_file) {
        if let Some(parent) = f.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(&index) {
            let _ = fs::write(f, json);
        }
    }
    let mut out: Vec<SessionSummary> = index
        .into_values()
        .map(|mut i| {
            if i.summary.started.is_empty() {
                i.summary.started = format!("@{}", i.modified);
            }
            i.summary
        })
        // A recording with no riding in it — a menu, a crash on the gate — has nothing to film.
        .filter(|s| s.duration >= 1.0)
        .collect();
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out
}

/// A float channel, base64'd little-endian f32: a long ride is a few hundred thousand samples
/// and JSON numbers would cost ten times the bytes.
fn b64(v: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for x in v {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Rolling radius used to turn ground speed into wheel spin for the preview, metres.
const ROLL_RADIUS_M: f32 = 0.31;

/// The most samples the preview is sent. The export always reads the file at full rate.
const PREVIEW_MAX: usize = 90_000;

/// A whole ride, for the scrubber and the preview.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub summary: SessionSummary,
    pub susp_max_travel: [f32; 2],
    pub lean_known: [bool; 2],
    pub stance_known: bool,
    pub jumps: Vec<Jump>,
    /// Every `stride`-th sample of the recording.
    pub stride: usize,
    pub count: usize,
    /// Base64 f32 channels, one value per kept sample. `t` is seconds from the first sample;
    /// angles are degrees; `used` is the share of the travel in use (0 extended, 1 bottomed);
    /// `spin` is degrees the wheels have turned; `air`, `stance` and `gear` are whole numbers.
    pub channels: HashMap<&'static str, String>,
}

pub fn data(path: &str) -> Result<SessionData, String> {
    let rec = load(path)?;
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let summary = summarise(Path::new(path), size, &rec);
    let s = &rec.samples;
    if s.is_empty() {
        return Err("empty".into());
    }
    let stride = s.len().div_ceil(PREVIEW_MAX).max(1);
    let t0 = s[0].t;
    let travel = rec.event.susp_max_travel;
    let mut ch: HashMap<&'static str, Vec<f32>> = HashMap::new();
    let mut spin = 0.0f32;
    let mut prev = s[0].t;
    for (k, x) in s.iter().enumerate() {
        spin += x.speed * (x.t - prev).max(0.0) / ROLL_RADIUS_M * 57.29578;
        prev = x.t;
        if k % stride != 0 {
            continue;
        }
        let used = |i: usize| if travel[i] > 0.0 { ((travel[i] - x.susp[i]) / travel[i]).clamp(0.0, 1.2) } else { 0.0 };
        for (name, v) in [
            ("t", x.t - t0),
            ("x", x.pos[0]),
            ("y", x.pos[1]),
            ("z", x.pos[2]),
            ("yaw", x.yaw),
            ("pitch", x.pitch),
            ("roll", x.roll),
            ("steer", x.steer),
            ("usedF", used(0)),
            ("usedR", used(1)),
            ("speed", x.speed),
            ("spin", spin),
            ("throttle", x.throttle),
            ("brake", x.brake[0].max(x.brake[1])),
            ("gear", x.gear as f32),
            ("air", if x.air { 1.0 } else { 0.0 }),
            ("crashed", if x.crashed { 1.0 } else { 0.0 }),
            ("leanLR", x.lean[0]),
            ("leanFB", x.lean[1]),
            ("stance", x.stance as f32),
        ] {
            ch.entry(name).or_default().push(v);
        }
    }
    let count = ch.get("t").map_or(0, Vec::len);
    Ok(SessionData {
        summary,
        susp_max_travel: travel,
        lean_known: rec.lean_known,
        stance_known: rec.stance_known,
        jumps: mxbc::jumps(s),
        stride,
        count,
        channels: ch.into_iter().map(|(k, v)| (k, b64(&v))).collect(),
    })
}

/// The track's own terrain under a ride: where to read it, and how high the bike rides above
/// it. The same answer Coach's review gives, so the preview stands the bike on the ground.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ground {
    pub path: String,
    pub prefix: Option<String>,
    pub name: String,
    pub lift: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundAnswer {
    pub ground: Option<Ground>,
    /// A code the frontend words: `notInstalled`, `locked`, `unreadable`.
    pub why: Option<&'static str>,
}

/// Bilinear terrain height at world `x`, `z`, or None off the grid.
fn height_at(w: usize, h: usize, mps: f32, heights: &[f32], x: f32, z: f32) -> Option<f32> {
    if mps <= 0.0 || w < 2 || h < 2 {
        return None;
    }
    let (gx, gz) = (x / mps, z / mps);
    if gx < 0.0 || gz < 0.0 || gx > (w - 1) as f32 || gz > (h - 1) as f32 {
        return None;
    }
    let (c0, r0) = (gx.floor() as usize, gz.floor() as usize);
    let (c1, r1) = ((c0 + 1).min(w - 1), (r0 + 1).min(h - 1));
    let (fx, fz) = (gx - c0 as f32, gz - r0 as f32);
    let at = |c: usize, r: usize| heights[r * w + c];
    let top = at(c0, r0) + (at(c1, r0) - at(c0, r0)) * fx;
    let bot = at(c0, r1) + (at(c1, r1) - at(c0, r1)) * fx;
    Some(top + (bot - top) * fz)
}

pub fn ground(app: &AppHandle, path: &str) -> Result<GroundAnswer, String> {
    let rec = load(path)?;
    let cfg = load_config(app);
    let Some(src) = mxb_core::tracksource::resolve(&cfg, &rec.event.track_id) else {
        return Ok(GroundAnswer { ground: None, why: Some("notInstalled") });
    };
    if src.locked {
        return Ok(GroundAnswer { ground: None, why: Some("locked") });
    }
    let Ok(master) = mxb_core::track::load_master(app, &src.path, src.prefix.as_deref()) else {
        return Ok(GroundAnswer { ground: None, why: Some("unreadable") });
    };
    let i = &master.info;
    // The median gap between the grounded bike and the terrain under it.
    let mut gaps: Vec<f32> = rec
        .samples
        .iter()
        .filter(|s| !s.air && !s.crashed)
        .step_by(5)
        .filter_map(|s| {
            height_at(i.width as usize, i.height as usize, i.metres_per_sample, &master.heights, s.pos[0], s.pos[2])
                .map(|g| s.pos[1] - g)
        })
        .collect();
    gaps.sort_by(f32::total_cmp);
    let lift = gaps.get(gaps.len() / 2).copied().unwrap_or(0.0);
    Ok(GroundAnswer { ground: Some(Ground { path: src.path, prefix: src.prefix, name: src.name, lift }), why: None })
}
