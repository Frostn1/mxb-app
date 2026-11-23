//! Finding Blender, and the one Blender job Cinema ever runs.
//!
//! Every job is a single `blender -b` process: two threads (`-t 2`), below-normal priority,
//! no window, and `gpu.py` first so Cycles renders on the NVIDIA card through OptiX rather than
//! silently on the CPU. Nothing retries: a job that fails says why and stays failed. And it
//! won't start while MX Bikes is running, or while another Blender is, because either would
//! cost the rider frames or the PC its memory.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

/// A command run for its output, never shown.
fn quiet(exe: &str) -> Command {
    let mut c = Command::new(exe);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// A Blender install: the executable and the version its install folder carries.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    pub path: String,
    /// `5.2`, from the version folder beside the executable. Empty when there is none.
    pub version: String,
    /// Older than the 4.2 the bundled scripts are written against.
    pub too_old: bool,
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "blender.exe" } else { "blender" }
}

/// The `X.Y` folder a Blender install keeps its scripts and data files in.
fn version_of(exe: &Path) -> String {
    let Some(dir) = exe.parent() else { return String::new() };
    let mut found: Vec<(u32, u32)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let (a, b) = n.split_once('.')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .collect();
    found.sort();
    found.last().map(|(a, b)| format!("{a}.{b}")).unwrap_or_default()
}

pub fn install(exe: &Path) -> Option<Install> {
    if !exe.is_file() {
        return None;
    }
    let version = version_of(exe);
    let too_old = version
        .split_once('.')
        .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
        .is_some_and(|v| v < (4, 2));
    Some(Install { path: exe.to_string_lossy().into_owned(), version, too_old })
}

fn version_key(i: &Install) -> (u32, u32) {
    i.version
        .split_once('.')
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .unwrap_or((0, 0))
}

/// Every Blender this PC has where Blender usually lives, newest first: the installer's
/// folder, Steam's, a portable copy under `C:\Tools` or the user's own folder, what `.blend`
/// files open with, and `PATH`.
pub fn detect(game_dir: &str) -> Vec<Install> {
    let mut cands: Vec<PathBuf> = Vec::new();
    let mut scan = |dir: PathBuf, prefix: &str| {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            if n.starts_with(prefix) {
                cands.push(e.path().join(exe_name()));
            }
        }
    };
    #[cfg(windows)]
    {
        for pf in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Ok(p) = std::env::var(pf) {
                scan(PathBuf::from(p).join("Blender Foundation"), "blender");
            }
        }
        for root in ["C:\\Tools", "D:\\Tools", "C:\\Blender"] {
            scan(PathBuf::from(root), "blender");
        }
        if let Ok(home) = std::env::var("USERPROFILE") {
            scan(PathBuf::from(&home), "blender");
            scan(PathBuf::from(&home).join("Downloads"), "blender");
        }
        // Steam keeps Blender beside the game the rider already has.
        let game = PathBuf::from(game_dir);
        if let Some(common) = game.parent() {
            cands.push(common.join("Blender").join(exe_name()));
        }
        cands.push(PathBuf::from("C:\\Program Files (x86)\\Steam\\steamapps\\common\\Blender\\blender.exe"));
        // What .blend files open with.
        if let Ok(out) = quiet("reg").args(["query", "HKCR\\blendfile\\shell\\open\\command", "/ve"]).output() {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            if let Some(start) = text.find('"') {
                if let Some(len) = text[start + 1..].find('"') {
                    cands.push(PathBuf::from(&text[start + 1..start + 1 + len]));
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    cands.push(PathBuf::from("/Applications/Blender.app/Contents/MacOS/Blender"));
    #[cfg(target_os = "linux")]
    {
        cands.push(PathBuf::from("/usr/bin/blender"));
        cands.push(PathBuf::from("/snap/bin/blender"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe_name()));
        }
    }
    let mut out: Vec<Install> = Vec::new();
    for c in cands {
        let Some(i) = install(&c) else { continue };
        let key = i.path.to_ascii_lowercase();
        if !out.iter().any(|o| o.path.to_ascii_lowercase() == key) {
            out.push(i);
        }
    }
    out.sort_by_key(|i| std::cmp::Reverse(version_key(i)));
    out
}

// ---------------------------------------------------------------- the job

/// Where a job is.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    #[default]
    Idle,
    Preparing,
    Blender,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    /// The pipeline stage Blender last announced (`scene`, `animate`, ..., `render`).
    pub step: String,
    /// The last lines of the job's log.
    pub log: Vec<String>,
    /// A code the frontend words, with any detail after a colon.
    pub error: Option<String>,
    pub blend: Option<String>,
    pub still: Option<String>,
    pub clip: Option<String>,
    pub folder: Option<String>,
    /// Seconds since the job started.
    pub elapsed: f32,
}

#[derive(Default)]
pub struct Job {
    pub status: Mutex<Status>,
    pub child: Mutex<Option<Child>>,
    pub started: Mutex<Option<std::time::Instant>>,
}

pub type Shared = Arc<Job>;

const LOG_LINES: usize = 400;

impl Job {
    pub fn push(&self, line: &str) {
        log::info!("[cinema] {line}");
        if let Ok(mut s) = self.status.lock() {
            if let Some(step) = line.strip_prefix("CINEMA STEP ") {
                s.step = step.trim().to_string();
            }
            s.log.push(line.to_string());
            let n = s.log.len();
            if n > LOG_LINES {
                s.log.drain(..n - LOG_LINES);
            }
        }
    }

    pub fn set(&self, f: impl FnOnce(&mut Status)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    pub fn busy(&self) -> bool {
        self.status.lock().map(|s| matches!(s.state, State::Preparing | State::Blender)).unwrap_or(false)
    }

    pub fn snapshot(&self) -> Status {
        let mut s = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        if let Ok(t) = self.started.lock() {
            s.elapsed = t.map_or(0.0, |t| t.elapsed().as_secs_f32());
        }
        s
    }

    /// Stop the running Blender, if any. The job then ends as cancelled.
    pub fn cancel(&self) {
        if let Ok(mut c) = self.child.lock() {
            if let Some(ch) = c.as_mut() {
                let _ = ch.kill();
            }
        }
        self.set(|s| {
            if matches!(s.state, State::Preparing | State::Blender) {
                s.state = State::Cancelled;
            }
        });
    }
}

/// What a job asks Blender for, written to `job.json` beside the shot.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BlenderJob {
    pub clip: String,
    pub track_dir: String,
    pub bike_dir: String,
    pub rider_dir: Option<String>,
    pub out_blend: String,
    /// `none`, `still` or `clip`.
    pub render: String,
    pub out_still: String,
    pub out_clip: String,
    pub res: [u32; 2],
    pub samples: u32,
    /// `chase`, `trackside`, `detail`, or `auto` (trackside at a jump's apex, else chase).
    pub camera: String,
    pub fx: bool,
    /// The app's preview camera: world position (game axes), the view-frame quaternion and the
    /// vertical field of view. When set it is the shot's camera.
    pub view_camera: Option<serde_json::Value>,
    /// The still's look and camera, and the sliders (0..1), read by `cinema_run.py`:
    /// `matched` / `cinematic` / `realism`; `view` / `photo` / `close` / `tight`.
    pub look: String,
    pub shot: String,
    /// `sun` or `shade`: which side of the rider a photo camera stands.
    pub side: String,
    pub mud: f32,
    pub blur: f32,
    pub motion: f32,
    /// Bundled: the realism pass's light (Poly Haven, CC0) and the bike's mud layer.
    pub hdri: Option<String>,
    pub mud_map: Option<String>,
}

/// Runs Blender on `job_file` and waits for it. Blocking: call it off the main thread.
pub fn run(job: &Job, exe: &str, scripts: &Path, job_file: &Path, need_gpu: bool) -> Result<(), String> {
    let gpu = scripts.join("gpu.py");
    let main = scripts.join("cinema_run.py");
    let mut cmd = Command::new(exe);
    cmd.args(["-b", "--factory-startup", "-noaudio", "-t", "2", "-P"])
        .arg(&gpu)
        .arg("-P")
        .arg(&main)
        .arg("--")
        .arg("--job")
        .arg(job_file)
        .env("CINEMA_NEED_GPU", if need_gpu { "1" } else { "0" })
        // The bundled scripts sit in the install folder: no __pycache__ written beside them.
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(scripts)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    let mut child = cmd.spawn().map_err(|e| format!("blenderStart: {e}"))?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    if let Ok(mut c) = job.child.lock() {
        *c = Some(child);
    }
    // Both pipes drained as they fill, or a chatty Blender blocks on a full one.
    std::thread::scope(|scope| {
        if let Some(e) = err {
            scope.spawn(move || {
                for line in BufReader::new(e).lines().map_while(Result::ok) {
                    job.push(&line);
                }
            });
        }
        if let Some(o) = out {
            for line in BufReader::new(o).lines().map_while(Result::ok) {
                job.push(&line);
            }
        }
    });
    let status = job.child.lock().ok().and_then(|mut c| c.take()).map(|mut ch| ch.wait());
    match status {
        Some(Ok(s)) if s.success() => Ok(()),
        Some(Ok(s)) => Err(format!("blenderFailed: exit {}", s.code().unwrap_or(-1))),
        Some(Err(e)) => Err(format!("blenderFailed: {e}")),
        None => Err("blenderFailed".into()),
    }
}
