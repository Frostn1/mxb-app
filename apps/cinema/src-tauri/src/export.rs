//! A marked moment, made into a shot: the slice as replay JSON, the track, bike and rider
//! prepared for Blender, then one Blender run that builds the `.blend` and, when asked,
//! renders a still or a short clip from it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::blender::{self, BlenderJob, Shared, State};
use crate::{clip, prep, sessions, settings};

/// What the rider asked for.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub path: String,
    /// Seconds from the recording's first sample. A single frame comes as `start == end`.
    pub start: f32,
    pub end: f32,
    /// The moment the still is taken at, and the frame the `.blend` opens on.
    pub mark: f32,
    /// `none` (the .blend only), `still` or `clip`.
    pub render: String,
    pub res: [u32; 2],
    pub samples: u32,
    pub camera: String,
    pub rider: bool,
    pub fx: bool,
    /// The preview's camera (`ViewerCamera`): the shot is taken from where the user left it.
    #[serde(default)]
    pub view: Option<serde_json::Value>,
    /// The still's look: `matched` (the game's frame), `cinematic`, or `realism` (cinematic plus
    /// the realism pass).
    #[serde(default = "matched")]
    pub look: String,
    /// The still's camera: `view` (the preview's, as F10 framed it) or a photographer's `photo`,
    /// `close` or `tight`.
    #[serde(default = "view_shot")]
    pub shot: String,
    /// A photo camera on the `sun` side (lit) or the `shade` side (backlit, with a fill light).
    #[serde(default = "sun")]
    pub side: String,
    /// 0..1: mud on the bike and dust on the gear (realism), background blur, motion blur
    /// (cinematic).
    #[serde(default = "one")]
    pub mud: f32,
    #[serde(default = "blur")]
    pub blur: f32,
    #[serde(default = "half")]
    pub motion: f32,
}

fn matched() -> String {
    "matched".into()
}
fn view_shot() -> String {
    "view".into()
}
fn sun() -> String {
    "sun".into()
}
fn one() -> f32 {
    1.0
}
fn blur() -> f32 {
    0.8
}
fn half() -> f32 {
    0.5
}

/// A single frame still gets this much ride either side, so the rider's body has motion to
/// settle into and the cameras a path to aim along.
const FRAME_CONTEXT_S: f32 = 1.5;
/// The shortest slice a shot is built from.
const MIN_SPAN_S: f32 = 2.0;
/// The longest: every frame is posed and dressed one at a time, and a clip is a render each.
pub const MAX_SPAN_S: f32 = 20.0;

/// Where the bundled Blender scripts are: the app's resources in a build, the source folder
/// in a dev run.
pub fn scripts_dir(app: &AppHandle) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("blender");
    if cfg!(debug_assertions) && src.join("cinema_run.py").is_file() {
        return src;
    }
    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join("blender");
        if p.join("cinema_run.py").is_file() {
            return p;
        }
    }
    src
}

/// A bundled resource (`src-tauri/resources/<name>`): the HDRI and the mud layer.
pub fn resource(app: &AppHandle, name: &str) -> Option<String> {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join(name);
    let inst = app.path().resource_dir().ok().map(|d| d.join("resources").join(name));
    let p = if cfg!(debug_assertions) && src.is_file() { Some(src) } else { inst.filter(|p| p.is_file()) };
    p.map(|p| p.to_string_lossy().into_owned())
}

/// The prepared-inputs cache: per-machine, rebuildable, never the rider's documents.
pub fn cache_dir() -> PathBuf {
    dirs_cache().join("MXB Cinema").join("cache")
}

fn dirs_cache() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(|| std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)).unwrap_or_else(std::env::temp_dir)
}

fn safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    t.trim_matches('_').chars().take(40).collect()
}

fn shot_folder(out: &Path, track: &str, started: &str, mark: f32) -> PathBuf {
    // `<track> <yyyy-mm-dd hh.mm> <s>s`: readable in Explorer, sorted by ride.
    let d: String = started.chars().filter(char::is_ascii_digit).collect();
    let when = if d.len() >= 12 { format!("{}-{}-{} {}.{}", &d[0..4], &d[4..6], &d[6..8], &d[8..10], &d[10..12]) } else { d };
    let base = format!("{} {} {:.1}s", safe(track).replace('_', " ").trim(), when, mark);
    let mut dir = out.join(&base);
    let mut k = 2;
    while dir.exists() {
        dir = out.join(format!("{base}_{k}"));
        k += 1;
    }
    dir
}

/// Starts a job unless one is running or the PC is busy with the game or another Blender.
pub fn start(app: &AppHandle, job: Shared, req: ExportRequest) -> Result<(), String> {
    if job.busy() {
        return Err("jobRunning".into());
    }
    let set = settings::load(app);
    let exe = settings::blender_exe(app, &set).ok_or("noBlender")?;
    if req.render != "none" && req.render != "still" && req.render != "clip" {
        return Err("badRequest".into());
    }
    if let Ok(mut t) = job.started.lock() {
        *t = Some(std::time::Instant::now());
    }
    job.set(|s| *s = blender::Status { state: State::Preparing, step: "prepare".into(), ..Default::default() });
    let app = app.clone();
    std::thread::spawn(move || {
        let res = run(&app, &job, &exe, &set, &req);
        job.set(|s| match res {
            Ok(()) if s.state != State::Cancelled => s.state = State::Done,
            Ok(()) => {}
            Err(e) => {
                if s.state != State::Cancelled {
                    s.state = State::Failed;
                    s.error = Some(e);
                }
            }
        });
    });
    Ok(())
}

fn run(app: &AppHandle, job: &blender::Job, exe: &str, set: &settings::Settings, req: &ExportRequest) -> Result<(), String> {
    let log = |l: &str| job.push(l);
    let cfg = sessions::load_config(app);
    let rec = sessions::load(&req.path)?;
    let summary_started = Path::new(&req.path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (mut a, mut b) = (req.start.min(req.end), req.start.max(req.end));
    if b - a < 0.1 {
        a = (req.mark - FRAME_CONTEXT_S).max(0.0);
        b = req.mark + FRAME_CONTEXT_S;
    }
    if b - a > MAX_SPAN_S {
        return Err("tooLong".into());
    }
    // The cameras need a couple of seconds of path to pick their spots along.
    if b - a < MIN_SPAN_S {
        let grow = (MIN_SPAN_S - (b - a)) / 2.0;
        a = (a - grow).max(0.0);
        b = a + MIN_SPAN_S;
    }
    let out_root = settings::output_dir(set);
    let dir = shot_folder(&out_root, &rec.event.track_id, &summary_started, req.mark);
    fs::create_dir_all(&dir).map_err(|e| format!("outputFolder: {e}"))?;
    job.set(|s| s.folder = Some(dir.to_string_lossy().into_owned()));
    log(&format!("shot: {} {:.2}-{:.2} s, mark {:.2} s -> {}", rec.event.track_id, a, b, req.mark, dir.display()));

    let clip_json = clip::slice(&rec, &req.path, a, b, req.mark)?;
    let clip_file = dir.join("clip.json");
    fs::write(&clip_file, serde_json::to_vec(&clip_json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;

    let cache = cache_dir();
    let track_dir = prep::track(&cfg, &cache, &rec.event.track_id, &log).map_err(|e| format!("{e:#}"))?;
    if job.snapshot().state == State::Cancelled {
        return Ok(());
    }
    let profile = prep::profile_for(&cfg, &rec.event.rider);
    let lo = prep::loadout(&cfg, &profile, &rec.event.bike_id);
    let bike_dir = prep::bike(&cfg, &cache, &rec.event.bike_id, &lo, &log).map_err(|e| format!("{e:#}"))?;
    if job.snapshot().state == State::Cancelled {
        return Ok(());
    }
    let rider_dir = if req.rider {
        Some(prep::rider(app, &cfg, &cache, &profile, &rec.event.bike_id, &log).map_err(|e| format!("{e:#}"))?)
    } else {
        None
    };
    if job.snapshot().state == State::Cancelled {
        return Ok(());
    }

    let s = |p: PathBuf| p.to_string_lossy().into_owned();
    let bj = BlenderJob {
        clip: s(clip_file),
        track_dir: s(track_dir),
        bike_dir: s(bike_dir),
        rider_dir: rider_dir.map(s),
        out_blend: s(dir.join("shot.blend")),
        render: req.render.clone(),
        out_still: s(dir.join("still.png")),
        out_clip: s(dir.join("clip.mp4")),
        res: [req.res[0].clamp(320, 7680), req.res[1].clamp(240, 4320)],
        samples: req.samples.clamp(1, 4096),
        camera: req.camera.clone(),
        fx: req.fx,
        view_camera: req.view.clone(),
        look: match req.look.as_str() {
            "cinematic" | "realism" => req.look.clone(),
            _ => "matched".into(),
        },
        shot: match req.shot.as_str() {
            "photo" | "close" | "tight" => req.shot.clone(),
            _ => "view".into(),
        },
        side: if req.side == "shade" { "shade".into() } else { "sun".into() },
        mud: req.mud.clamp(0.0, 1.0),
        blur: req.blur.clamp(0.0, 1.0),
        motion: req.motion.clamp(0.0, 1.0),
        hdri: resource(app, "ninomaru_teien_4k.exr"),
        mud_map: resource(app, "dgsbike_mud.png"),
    };
    let job_file = dir.join("job.json");
    fs::write(&job_file, serde_json::to_vec_pretty(&bj).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    job.set(|st| {
        st.state = State::Blender;
        st.step = "start".into();
    });
    blender::run(job, exe, &scripts_dir(app), &job_file, req.render != "none")?;
    if job.snapshot().state == State::Cancelled {
        return Ok(());
    }
    let exists = |p: &str| Path::new(p).is_file().then(|| p.to_string());
    let blend = exists(&bj.out_blend).ok_or("noBlend")?;
    let still = exists(&bj.out_still);
    let clip = exists(&bj.out_clip);
    if req.render == "still" && still.is_none() {
        return Err("noStill".into());
    }
    if req.render == "clip" && clip.is_none() {
        return Err("noClip".into());
    }
    job.set(|st| {
        st.blend = Some(blend);
        st.still = still;
        st.clip = clip;
    });
    Ok(())
}
