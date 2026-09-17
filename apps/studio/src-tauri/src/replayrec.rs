//! Recording a replay, so nobody has to run OBS beside the game.
//!
//! The Replay Mod flies a camera path inside MX Bikes and always has. What it could not do
//! was keep the result: the mod drew the shot and the rider was on their own for the part
//! that turns a shot into a video — find OBS, set up a scene, remember to press record, crop
//! the overlay out afterwards. Every one of those is a place to lose a take you cannot get
//! back, because a replay is a session that has already ended.
//!
//! So the Studio records it. The mod says a take has started (see [`mxb_core::replay`]), this
//! module starts an encoder against the game's own window, and when the take ends the file is
//! already on disk under the track's name. Nothing to arm, nothing to crop, nothing to
//! remember.
//!
//! **ffmpeg does the encoding, in a child process.** Not a library linked into the Studio:
//! this is a video encoder running flat out beside a game the person is watching, and a
//! separate process is one we can stop dead, cap, and let crash without taking the Studio's
//! window with it. It also keeps a GPL encoder out of this binary. The Studio fetches a build
//! if there isn't one on the machine — see [`fetch`].
//!
//! **Windows only, like the mod and the game.** The capture APIs here are Windows' own, and a
//! Mac running the game under CrossOver is a rig this has never been proven on. Elsewhere the
//! commands answer honestly instead of half-working.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mxb_core::replay::{self, Encoder, Recording, Take, TakeState};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// How often the watcher looks at the take file.
///
/// Polled rather than watched with the file-system notifier the manager uses for the mods
/// folder. A take file is rewritten every frame or two by a Windows process that may be
/// inside a Wine prefix, on a path the host's notifier does not necessarily report — and the
/// whole job is reading one small JSON file. Half a second costs nothing and works
/// everywhere, which is the trade a recorder that must not miss a take wants.
const POLL: Duration = Duration::from_millis(500);

/// How long to give ffmpeg to finish the file after being asked to stop.
///
/// It has to write the trailer and move the index; killing it before that leaves an mp4 that
/// no player will open, which is the same as not having recorded at all.
const FLUSH_GRACE: Duration = Duration::from_secs(8);

/// The event the UI listens on. Carries [`Status`].
pub const EVENT: &str = "replay-status";

// ---------------------------------------------------------------------------
// state
// ---------------------------------------------------------------------------

/// What started the recording that is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The mod said a take had started. The point of the feature.
    Auto,
    /// Somebody pressed record, in the Studio or on the hotkey.
    Manual,
}

/// The recording in flight.
struct Live {
    child: Child,
    file: PathBuf,
    started: Instant,
    source: Source,
    /// When to stop whatever anyone says — the `maxMinutes` backstop.
    deadline: Instant,
}

/// The recorder, as Tauri manages it.
#[derive(Default)]
pub struct Recorder {
    live: Mutex<Option<Live>>,
    /// What the resolved ffmpeg can do, learnt once by running it. `None` until then.
    probe: Mutex<Option<Probe>>,
    /// The last thing that went wrong, so the UI can show it after the fact — a recording
    /// that failed while the person was in the game is one they find out about afterwards.
    fault: Mutex<Option<String>>,
    /// The config and the process table, as of a moment ago. See [`Snapshot`].
    snapshot: Mutex<Snapshot>,
}

/// The two answers the watcher would otherwise ask the machine for twice a second: what the
/// settings say, and whether the game is up.
///
/// Both are far more expensive than they look — one is a file read, the other enumerates every
/// process on Windows — and neither changes on that timescale. Two seconds is under the
/// interval anybody perceives and a quarter of the work.
#[derive(Default)]
struct Snapshot {
    at: Option<Instant>,
    cfg: mxb_core::config::AppConfig,
    game_running: bool,
}

/// How long a [`Snapshot`] is believed.
const SYS_TTL: Duration = Duration::from_secs(2);

/// The settings and whether the game is running, from the snapshot or freshly taken.
fn sys(app: &tauri::AppHandle) -> (mxb_core::config::AppConfig, bool) {
    let rec = app.state::<Recorder>();
    let Ok(mut snap) = rec.snapshot.lock() else {
        return (mxb_core::config::load_or_detect(app).unwrap_or_default(), false);
    };
    if snap.at.map(|t| t.elapsed() < SYS_TTL).unwrap_or(false) {
        return (snap.cfg.clone(), snap.game_running);
    }
    snap.cfg = mxb_core::config::load_or_detect(app).unwrap_or_default();
    snap.game_running = mxb_core::gamewindow::is_game_running();
    snap.at = Some(Instant::now());
    (snap.cfg.clone(), snap.game_running)
}

/// Forget the snapshot, so the next read is of what is actually there. Called after a save,
/// where waiting two seconds to honour a setting somebody just changed reads as a bug.
fn forget_snapshot(app: &tauri::AppHandle) {
    if let Ok(mut snap) = app.state::<Recorder>().snapshot.lock() {
        snap.at = None;
    }
}

/// What the ffmpeg we are about to run supports. Probed once per resolved binary, because
/// asking costs a process launch and the answer cannot change under us.
#[derive(Debug, Clone, Default)]
struct Probe {
    binary: PathBuf,
    /// Desktop Duplication capture. The one that works while the game is in exclusive
    /// fullscreen, which is how most people play.
    ddagrab: bool,
    nvenc: bool,
    amf: bool,
    qsv: bool,
}

/// Everything the Replay screen draws, in one object.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub recording: bool,
    /// The file being written, or the one just finished.
    pub file: Option<String>,
    pub seconds: u64,
    pub source: Option<Source>,
    /// What the mod is saying about itself, when it is saying anything.
    pub take: Option<Take>,
    /// Whether the mod has ever written a take file here — "install the mod" and "press play
    /// in the game" are different sentences and the screen has to pick the right one.
    pub mod_seen: bool,
    pub game_running: bool,
    /// The ffmpeg that would be used, if there is one.
    pub ffmpeg: Option<String>,
    /// Whether that ffmpeg can capture through Desktop Duplication. `None` until it has been
    /// asked — see [`replay_check`], which is what the screen calls when it opens.
    pub ddagrab: Option<bool>,
    /// The game has taken the screen exclusively. Harmless with `ddagrab`, and a black
    /// recording without it, which is the one thing worth warning about before the fact.
    pub exclusive_fullscreen: bool,
    /// What went wrong last, if anything.
    pub error: Option<String>,
}

/// One finished recording on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorded {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    /// Unix milliseconds.
    pub modified: u64,
}

// ---------------------------------------------------------------------------
// where things are
// ---------------------------------------------------------------------------

/// The mod's folder for the configured game, if the game folder is known at all.
pub fn mod_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let (cfg, _) = sys(app);
    Some(replay::folder(&mxb_core::config::default_user_dir(cfg.game())?))
}

/// Where finished recordings land: the configured folder, else `Videos\Frost Replays`.
///
/// Not inside the game's folder, and not inside the Studio's app data. A recording is the
/// one thing here that belongs to the person rather than to the game — they will want to drag
/// it into an editor, and Videos is where every other capture tool on the machine puts one.
pub fn out_dir(cfg: &mxb_core::config::AppConfig) -> PathBuf {
    let configured = cfg.replay.dir.trim();
    if !configured.is_empty() {
        return PathBuf::from(configured);
    }
    dirs_next::video_dir()
        .or_else(dirs_next::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Frost Replays")
}

/// Our own fetched ffmpeg, whether or not it has been fetched.
fn our_ffmpeg(app: &tauri::AppHandle) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    Some(app.path().app_data_dir().ok()?.join("ffmpeg").join(exe))
}

/// The ffmpeg to run: the configured one, then ours, then whatever is on `PATH`.
///
/// In that order because each step is a stronger statement of intent than the next. Somebody
/// who named a binary means that one — a build with the encoders their GPU wants, usually —
/// and must not be silently given ours instead.
pub fn resolve_ffmpeg(app: &tauri::AppHandle, cfg: &mxb_core::config::AppConfig) -> Option<PathBuf> {
    let configured = cfg.replay.ffmpeg_path.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        return p.is_file().then_some(p);
    }
    if let Some(ours) = our_ffmpeg(app) {
        if ours.is_file() {
            return Some(ours);
        }
    }
    on_path()
}

/// `ffmpeg` on `PATH`, resolved to a real file so the answer can be shown.
fn on_path() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(exe))
            .find(|p| p.is_file())
    })
}

// ---------------------------------------------------------------------------
// building the command line
// ---------------------------------------------------------------------------

/// Which capture the encoder reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capture {
    /// Desktop Duplication, through ffmpeg's `ddagrab` filter. Reads frames the compositor
    /// already has, so it keeps working when the game takes the screen exclusively — which
    /// `gdigrab` does not, and a black recording is the single worst outcome here.
    Dda,
    /// GDI, cropped to the game's window. The fallback for an ffmpeg without `ddagrab`.
    Gdi { x: i32, y: i32, w: u32, h: u32 },
    /// GDI, the whole desktop. When the game's window cannot be found.
    GdiDesktop,
}

/// The arguments to run, given what the machine can do.
///
/// Split out from spawning so the interesting half can be tested: this is a command line that
/// nobody sees, built from a settings screen and a window rectangle, and every one of the
/// ways it can be wrong ends in a file that is black, silent, or unplayable.
fn args(
    cfg: &Recording,
    probe: &Probe,
    capture: Capture,
    out: &Path,
) -> Vec<String> {
    let fps = cfg.fps.clamp(24, 120);
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
        // Never wait for an answer on a console nobody can see.
        "-y".into(),
    ];

    match capture {
        Capture::Dda => {
            a.push("-f".into());
            a.push("lavfi".into());
            a.push("-i".into());
            a.push(format!("ddagrab=output_idx=0:framerate={fps}:draw_mouse=0"));
            // ddagrab hands over frames that live on the GPU; every encoder below is happy
            // to take them from system memory, and the download is what makes one command
            // line work for all of them.
            a.push("-vf".into());
            a.push("hwdownload,format=bgra".into());
        }
        Capture::Gdi { x, y, w, h } => {
            a.push("-f".into());
            a.push("gdigrab".into());
            a.push("-framerate".into());
            a.push(fps.to_string());
            a.push("-draw_mouse".into());
            a.push("0".into());
            a.push("-offset_x".into());
            a.push(x.to_string());
            a.push("-offset_y".into());
            a.push(y.to_string());
            a.push("-video_size".into());
            // Odd dimensions and 4:2:0 chroma cannot both be had, and the encoder's failure
            // for it is a wall of text about a pixel format nobody asked about.
            a.push(format!("{}x{}", w & !1, h & !1));
            a.push("-i".into());
            a.push("desktop".into());
        }
        Capture::GdiDesktop => {
            a.push("-f".into());
            a.push("gdigrab".into());
            a.push("-framerate".into());
            a.push(fps.to_string());
            a.push("-draw_mouse".into());
            a.push("0".into());
            a.push("-i".into());
            a.push("desktop".into());
        }
    }

    let device = cfg.audio_device.trim();
    if !device.is_empty() {
        a.push("-f".into());
        a.push("dshow".into());
        a.push("-i".into());
        a.push(format!("audio={device}"));
        a.push("-c:a".into());
        a.push("aac".into());
        a.push("-b:a".into());
        a.push("192k".into());
    }

    for part in encoder_args(cfg, probe) {
        a.push(part);
    }

    a.push("-pix_fmt".into());
    a.push("yuv420p".into());
    // Put the index at the front, so a file dropped into an editor — or a browser — starts
    // playing instead of being read end to end first.
    a.push("-movflags".into());
    a.push("+faststart".into());
    a.push(out.to_string_lossy().into_owned());
    a
}

/// The encoder half of the command line.
fn encoder_args(cfg: &Recording, probe: &Probe) -> Vec<String> {
    let s = |v: &str| v.to_string();
    let chosen = match cfg.encoder {
        Encoder::Auto => {
            // Hardware first, and by preference rather than by taste: the CPU is busy running
            // the game whose replay this is, and an encoder that steals frames from it is
            // worse than one that keeps slightly less detail.
            if probe.nvenc {
                Encoder::Nvenc
            } else if probe.amf {
                Encoder::Amf
            } else if probe.qsv {
                Encoder::Qsv
            } else {
                Encoder::X264
            }
        }
        other => other,
    };
    let crf = cfg.quality.crf().to_string();
    let cq = cfg.quality.cq().to_string();
    match chosen {
        Encoder::Nvenc => vec![s("-c:v"), s("h264_nvenc"), s("-preset"), s("p5"), s("-rc"), s("vbr"), s("-cq"), cq],
        Encoder::Amf => vec![s("-c:v"), s("h264_amf"), s("-quality"), s("balanced"), s("-rc"), s("cqp"), s("-qp_i"), cq.clone(), s("-qp_p"), cq],
        Encoder::Qsv => vec![s("-c:v"), s("h264_qsv"), s("-global_quality"), cq],
        // `veryfast` and not `medium`: dropping frames of the game to keep a few kilobytes is
        // the wrong way round for a capture nobody is going to re-encode.
        Encoder::X264 | Encoder::Auto => vec![s("-c:v"), s("libx264"), s("-preset"), s("veryfast"), s("-crf"), crf],
    }
}

/// Which capture to use, given what ffmpeg has and where the game's window is.
fn pick_capture(probe: &Probe, rect: Option<(i32, i32, i32, i32)>) -> Capture {
    if probe.ddagrab {
        return Capture::Dda;
    }
    match rect {
        Some((l, t, r, b)) if r > l && b > t => Capture::Gdi {
            x: l,
            y: t,
            w: (r - l) as u32,
            h: (b - t) as u32,
        },
        _ => Capture::GdiDesktop,
    }
}

/// Ask an ffmpeg what it can do. One process, once per binary.
fn probe_ffmpeg(binary: &Path) -> Probe {
    let text = |args: &[&str]| -> String {
        let mut cmd = Command::new(binary);
        cmd.args(args).stdin(Stdio::null());
        no_window(&mut cmd);
        cmd.output()
            .map(|o| {
                let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
                s.push_str(&String::from_utf8_lossy(&o.stderr));
                s
            })
            .unwrap_or_default()
    };
    let encoders = text(&["-hide_banner", "-encoders"]);
    let filters = text(&["-hide_banner", "-filters"]);
    Probe {
        binary: binary.to_path_buf(),
        ddagrab: filters.contains("ddagrab"),
        nvenc: encoders.contains("h264_nvenc"),
        amf: encoders.contains("h264_amf"),
        qsv: encoders.contains("h264_qsv"),
    }
}

/// Windows: no console window flashing up behind the Studio for a child nobody looks at.
#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

// ---------------------------------------------------------------------------
// starting and stopping
// ---------------------------------------------------------------------------

impl Recorder {
    fn is_live(&self) -> bool {
        self.live.lock().ok().map(|l| l.is_some()).unwrap_or(false)
    }

    fn fault(&self, message: impl Into<String>) {
        if let Ok(mut slot) = self.fault.lock() {
            *slot = Some(message.into());
        }
    }

    fn clear_fault(&self) {
        if let Ok(mut slot) = self.fault.lock() {
            *slot = None;
        }
    }
}

/// Start recording. Returns the file being written.
///
/// Every refusal here is a sentence somebody can act on, because each one sends them
/// somewhere different: no ffmpeg is a download, no game is a game to start, and a recorder
/// already running is not an error at all.
pub fn start(app: &tauri::AppHandle, source: Source) -> Result<PathBuf, String> {
    let rec = app.state::<Recorder>();
    if rec.is_live() {
        return Err("Already recording.".into());
    }
    if !cfg!(windows) {
        return Err("Recording a replay needs Windows, where MX Bikes and the Replay Mod run.".into());
    }

    let (cfg, _) = sys(app);
    let binary = resolve_ffmpeg(app, &cfg)
        .ok_or("No ffmpeg yet — the Studio can fetch one for you from the Replay screen.")?;

    // Probe once per binary; a settings change that points at another one re-probes.
    let probe = {
        let mut slot = rec.probe.lock().map_err(|_| "recorder state is poisoned")?;
        if slot.as_ref().map(|p| p.binary != binary).unwrap_or(true) {
            *slot = Some(probe_ffmpeg(&binary));
        }
        slot.clone().unwrap_or_default()
    };

    let take = mod_dir(app).and_then(|d| replay::read_take(&d)).unwrap_or_default();
    let dir = out_dir(&cfg);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
    let file = unique(&dir, &take.recording_name(&stamp()));

    let capture = pick_capture(&probe, mxb_core::gamewindow::game_window_rect());
    let argv = args(&cfg.replay, &probe, capture, &file);

    // ffmpeg's own account of the run, kept next to the app's data. A recording that comes
    // out black or never starts is diagnosed from this file and nowhere else.
    let log = app
        .path()
        .app_log_dir()
        .or_else(|_| app.path().app_data_dir())
        .map(|d| {
            let _ = std::fs::create_dir_all(&d);
            d.join("replay-ffmpeg.log")
        })
        .ok();

    let mut cmd = Command::new(&binary);
    cmd.args(&argv)
        // Piped, because 'q' on this pipe is how a clean stop is asked for.
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(match log.as_ref().and_then(|p| std::fs::File::create(p).ok()) {
            Some(f) => Stdio::from(f),
            None => Stdio::null(),
        });
    no_window(&mut cmd);

    let child = cmd
        .spawn()
        .map_err(|e| format!("Couldn't start ffmpeg ({}): {e}", binary.display()))?;

    let minutes = cfg.replay.max_minutes.clamp(1, 240) as u64;
    let live = Live {
        child,
        file: file.clone(),
        started: Instant::now(),
        source,
        deadline: Instant::now() + Duration::from_secs(minutes * 60),
    };
    *rec.live.lock().map_err(|_| "recorder state is poisoned")? = Some(live);
    rec.clear_fault();
    mxb_core::usage::track(match source {
        Source::Auto => "replay.record.auto",
        Source::Manual => "replay.record.manual",
    });
    log::info!("[replay] recording to {}", file.display());
    announce(app);
    Ok(file)
}

/// Stop the recording and let ffmpeg finish the file. Returns the finished file.
pub fn stop(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let rec = app.state::<Recorder>();
    let mut live = match rec.live.lock().map_err(|_| "recorder state is poisoned")?.take() {
        Some(l) => l,
        None => return Ok(None),
    };

    // 'q' is ffmpeg's own "stop now, tidily". Killing instead leaves an mp4 with no index,
    // which is a recording nobody can open — the failure this whole feature exists to avoid.
    if let Some(stdin) = live.child.stdin.as_mut() {
        let _ = stdin.write_all(b"q");
        let _ = stdin.flush();
    }
    drop(live.child.stdin.take());

    let until = Instant::now() + FLUSH_GRACE;
    loop {
        match live.child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                // It would not go. Better a truncated file than a process left encoding the
                // desktop for the rest of the session.
                let _ = live.child.kill();
                let _ = live.child.wait();
                rec.fault("The recording had to be stopped the hard way — the file may be short.");
                break;
            }
            Err(e) => {
                rec.fault(format!("Lost track of ffmpeg: {e}"));
                break;
            }
        }
    }

    log::info!("[replay] recorded {}", live.file.display());
    announce(app);
    Ok(Some(live.file))
}

/// A file name nothing is using yet. Two takes in the same minute do not overwrite.
fn unique(dir: &Path, base: &str) -> PathBuf {
    let first = dir.join(format!("{base}.mp4"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let p = dir.join(format!("{base} ({n}).mp4"));
        if !p.exists() {
            return p;
        }
    }
    first
}

/// `2026-09-17 18-40-02`, in the rider's own time zone. Colons are not allowed in a Windows
/// file name, which is why this is not an ISO timestamp.
fn stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let local = now + local_offset_secs();
    let (y, mo, d, h, mi, s) = civil(local);
    format!("{y:04}-{mo:02}-{d:02} {h:02}-{mi:02}-{s:02}")
}

/// Seconds this machine is ahead of UTC right now.
///
/// Worked out rather than asked of a date crate: the workspace has none, and one whole
/// dependency for the name of a file is not a trade worth making. `localtime`'s answer is
/// recovered by formatting the same instant both ways through the platform's own clock.
fn local_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        // `GetTimeZoneInformation` reports the bias in minutes *west* of UTC.
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        #[repr(C)]
        struct TimeZoneInformation {
            bias: i32,
            standard_name: [u16; 32],
            standard_date: SystemTime,
            standard_bias: i32,
            daylight_name: [u16; 32],
            daylight_date: SystemTime,
            daylight_bias: i32,
        }
        const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
        extern "system" {
            fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
        }
        // SAFETY: the call only writes into the struct we hand it, and every field is plain
        // data. A failure returns `TIME_ZONE_ID_INVALID` and leaves us on UTC.
        unsafe {
            let mut info: TimeZoneInformation = std::mem::zeroed();
            let id = GetTimeZoneInformation(&mut info);
            if id == u32::MAX {
                return 0;
            }
            let extra = if id == TIME_ZONE_ID_DAYLIGHT { info.daylight_bias } else { info.standard_bias };
            -((info.bias + extra) as i64) * 60
        }
    }
    #[cfg(not(windows))]
    {
        // Off Windows this only names a file on a developer's machine, and UTC is honest.
        0
    }
}

/// Civil date from a Unix timestamp. Howard Hinnant's `civil_from_days`, which is exact for
/// every date this will ever be handed and fits in a function.
fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3_600) as u32, ((rem % 3_600) / 60) as u32, (rem % 60) as u32)
}

// ---------------------------------------------------------------------------
// the watcher
// ---------------------------------------------------------------------------

/// Watch the mod's take file and record what it announces.
///
/// One thread for the life of the Studio. It does nothing at all until a take file appears,
/// which for everyone without the Replay Mod is forever — a sleeping thread and a `stat` of a
/// path that isn't there, twice a second.
pub fn watch(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut last: Option<Status> = None;
        loop {
            std::thread::sleep(POLL);
            tick(&app);
            // Only tell the window when something changed. A status event twice a second
            // would re-render the Replay screen for its whole life.
            let now = status(&app);
            if last.as_ref().map(|s| changed(s, &now)).unwrap_or(true) {
                let _ = app.emit(EVENT, &now);
                last = Some(now);
            }
        }
    });
}

/// Has anything the screen draws actually moved? The clock is deliberately included at
/// second resolution — a running timer is the one thing that should re-render.
fn changed(a: &Status, b: &Status) -> bool {
    a.recording != b.recording
        || a.seconds != b.seconds
        || a.file != b.file
        || a.error != b.error
        || a.game_running != b.game_running
        || a.mod_seen != b.mod_seen
        || a.take.as_ref().map(|t| t.state) != b.take.as_ref().map(|t| t.state)
        || a.ffmpeg != b.ffmpeg
}

/// One pass of the watcher: start, stop, or leave it alone.
fn tick(app: &tauri::AppHandle) {
    let rec = app.state::<Recorder>();
    let (cfg, _) = sys(app);
    let take = mod_dir(app).and_then(|d| replay::read_take(&d));
    let now_ms = now_ms();

    // A take that is playing and still beating. Anything else is not a take.
    let playing = take
        .as_ref()
        .is_some_and(|t| t.state == TakeState::Playing && !t.is_stale(now_ms));

    let live = rec.is_live();
    if !live {
        if playing && cfg.replay.auto {
            if let Err(e) = start(app, Source::Auto) {
                // Said once, and kept: the person is inside the game and will read it when
                // they come back out to a screen that says nothing was recorded.
                log::warn!("[replay] auto-record didn't start: {e}");
                rec.fault(e);
            }
        }
        return;
    }

    // Running. Three things end it: the take stopping, the cap, and ffmpeg dying.
    let over_cap = rec
        .live
        .lock()
        .ok()
        .and_then(|l| l.as_ref().map(|x| Instant::now() >= x.deadline))
        .unwrap_or(false);
    let died = rec
        .live
        .lock()
        .ok()
        .and_then(|mut l| l.as_mut().map(|x| matches!(x.child.try_wait(), Ok(Some(_)))))
        .unwrap_or(false);

    if died {
        // ffmpeg is gone and the file is whatever it managed to write. Take the state down
        // and say so rather than leaving the screen claiming a recording that is not running.
        let _ = stop(app);
        rec.fault("ffmpeg stopped on its own — see replay-ffmpeg.log next to the Studio's data.");
        return;
    }

    if over_cap {
        let _ = stop(app);
        rec.fault(format!(
            "Stopped after {} minutes — the recording cap in Settings.",
            cfg.replay.max_minutes
        ));
        return;
    }

    let started_by_mod = rec
        .live
        .lock()
        .ok()
        .and_then(|l| l.as_ref().map(|x| x.source == Source::Auto))
        .unwrap_or(false);

    // A recording somebody started by hand is theirs to stop. Only the ones the mod started
    // end when the mod says so.
    if started_by_mod && !playing {
        let _ = stop(app);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Everything the screen needs, assembled fresh.
pub fn status(app: &tauri::AppHandle) -> Status {
    let rec = app.state::<Recorder>();
    let (cfg, game_running) = sys(app);
    let dir = mod_dir(app);
    let take = dir.as_ref().and_then(|d| replay::read_take(d));
    let (recording, file, seconds, source) = rec
        .live
        .lock()
        .ok()
        .and_then(|l| {
            l.as_ref().map(|x| {
                (
                    true,
                    Some(x.file.to_string_lossy().into_owned()),
                    x.started.elapsed().as_secs(),
                    Some(x.source),
                )
            })
        })
        .unwrap_or((false, None, 0, None));

    Status {
        recording,
        file,
        seconds,
        source,
        mod_seen: dir.as_ref().is_some_and(|d| d.join(replay::TAKE_FILE).exists()),
        take,
        game_running,
        ffmpeg: resolve_ffmpeg(app, &cfg).map(|p| p.to_string_lossy().into_owned()),
        // Read, never taken: this is called twice a second by the watcher, and probing spawns
        // two processes. [`replay_check`] is where the asking happens.
        ddagrab: rec.probe.lock().ok().and_then(|p| p.as_ref().map(|p| p.ddagrab)),
        exclusive_fullscreen: mxb_core::gamewindow::is_exclusive_fullscreen(),
        error: rec.fault.lock().ok().and_then(|f| f.clone()),
    }
}

fn announce(app: &tauri::AppHandle) {
    let _ = app.emit(EVENT, status(app));
}

// ---------------------------------------------------------------------------
// fetching ffmpeg
// ---------------------------------------------------------------------------

/// The build the Studio fetches on Windows, and the digest published beside it.
///
/// Gyan's essentials build: no installer, one exe, and the encoders that matter (`libx264`,
/// NVENC, AMF, Quick Sync) plus `ddagrab`. The URL names a moving "latest release" rather
/// than a version, which is a deliberate trade — a pinned build would go stale and the person
/// who needed it most would be the one with a new GPU.
///
/// The digest beside it is checked, and it is worth being precise about what that buys: the
/// two files come from the same host, so this catches a truncated or corrupted download and
/// not a compromised one. It is the same guarantee the publisher offers everyone.
#[cfg(windows)]
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
#[cfg(windows)]
const FFMPEG_SHA_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip.sha256";

/// Download an ffmpeg into the Studio's own data folder. Windows only — see [`fetch`].
#[cfg(windows)]
async fn fetch(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};

    let dest = our_ffmpeg(app).ok_or("no data directory to put ffmpeg in")?;
    let client = reqwest::Client::new();

    let want = client
        .get(FFMPEG_SHA_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't reach {FFMPEG_SHA_URL}: {e}"))?
        .text()
        .await
        .map_err(|e| format!("couldn't read the published digest: {e}"))?;
    let want = want.split_whitespace().next().unwrap_or_default().to_lowercase();

    let bytes = client
        .get(FFMPEG_URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't reach {FFMPEG_URL}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("the ffmpeg download stopped early: {e}"))?;

    let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !want.is_empty() && got != want {
        return Err(format!(
            "that ffmpeg download doesn't match the digest published with it ({want} vs {got}) — nothing was installed"
        ));
    }

    let dest2 = dest.clone();
    tauri::async_runtime::spawn_blocking(move || unpack(&bytes, &dest2))
        .await
        .map_err(|e| format!("unpacking ffmpeg failed: {e}"))??;
    mxb_core::usage::track("replay.ffmpeg.fetch");
    Ok(dest)
}

/// Pull `bin/ffmpeg.exe` out of the zip and write it where we said.
///
/// Only that one file: the build also carries `ffplay`, `ffprobe` and a documentation tree,
/// none of which anything here runs, and 80 MB of them is not ours to leave on somebody's
/// disk for a feature that needs one binary.
#[cfg(windows)]
fn unpack(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("that ffmpeg download isn't a zip we can read: {e}"))?;
    let idx = (0..zip.len()).find(|i| {
        zip.by_index(*i)
            .map(|f| f.name().replace('\\', "/").ends_with("bin/ffmpeg.exe"))
            .unwrap_or(false)
    });
    let idx = idx.ok_or("that ffmpeg download has no bin/ffmpeg.exe in it")?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let mut entry = zip.by_index(idx).map_err(|e| e.to_string())?;
    // Beside the destination and then renamed: a half-written ffmpeg.exe that looks like a
    // whole one is a recording that fails at the moment it is needed.
    let part = dest.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    std::io::copy(&mut entry, &mut out).map_err(|e| format!("writing ffmpeg: {e}"))?;
    drop(out);
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(())
}

/// Nothing to fetch anywhere else: the game is Windows software, and installing system
/// packages behind somebody's back is not this app's business.
#[cfg(not(windows))]
async fn fetch(_app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Err("Fetch ffmpeg with your package manager (`ffmpeg` in every distribution, `brew install ffmpeg` on a Mac) — the Studio will find it on PATH.".into())
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

/// What the Replay screen shows when it opens.
#[tauri::command]
pub fn replay_status(app: tauri::AppHandle) -> Status {
    status(&app)
}

/// Ask the resolved ffmpeg what it can do, and answer with the status that follows.
///
/// A separate command from [`replay_status`] because it costs two process launches: the
/// Replay screen calls it once when it opens and after a settings change, and the watcher —
/// which runs twice a second forever — never does.
#[tauri::command]
pub async fn replay_check(app: tauri::AppHandle) -> Status {
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let (cfg, _) = sys(&handle);
        let Some(binary) = resolve_ffmpeg(&handle, &cfg) else {
            return;
        };
        let fresh = probe_ffmpeg(&binary);
        if let Ok(mut slot) = handle.state::<Recorder>().probe.lock() {
            *slot = Some(fresh);
        }
    })
    .await;
    status(&app)
}

/// The camera paths saved on this machine.
#[tauri::command]
pub fn replay_slots(app: tauri::AppHandle) -> Vec<replay::Slot> {
    mod_dir(&app).map(|d| replay::slots(&d)).unwrap_or_default()
}

/// The recorder's settings, as stored.
#[tauri::command]
pub fn replay_settings(app: tauri::AppHandle) -> Recording {
    sys(&app).0.replay
}

/// Save them. Returns what was actually stored, since some of it is clamped.
#[tauri::command]
pub fn replay_save_settings(app: tauri::AppHandle, settings: Recording) -> Result<Recording, String> {
    // Loaded rather than taken from the snapshot: this one writes the file back, and saving a
    // copy that is two seconds old would undo whatever the other app changed in between.
    let mut cfg = mxb_core::config::load_or_detect(&app).unwrap_or_default();
    let mut next = settings;
    next.fps = next.fps.clamp(24, 120);
    next.max_minutes = next.max_minutes.clamp(1, 240);
    cfg.replay = next.clone();
    mxb_core::config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    forget_snapshot(&app);
    // A new ffmpeg has new answers; make the next recording ask again.
    if let Ok(mut slot) = app.state::<Recorder>().probe.lock() {
        *slot = None;
    }
    bind_hotkey(&app);
    announce(&app);
    Ok(next)
}

/// Record now, without waiting for the mod to say anything.
///
/// Off the main thread: the first start on a machine probes ffmpeg, which is two process
/// launches, and a button that freezes the window while it thinks is a button people press
/// twice.
#[tauri::command]
pub async fn replay_record(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start(&app, Source::Manual).map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("replay_record task failed: {e}"))?
}

/// Stop whatever is recording. Answering with the file it wrote, or nothing.
///
/// Also off the main thread, and for a bigger reason: stopping waits for ffmpeg to write the
/// trailer, which is up to [`FLUSH_GRACE`] seconds of doing nothing.
#[tauri::command]
pub async fn replay_stop(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        stop(&app).map(|p| p.map(|p| p.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| format!("replay_stop task failed: {e}"))?
}

/// The recordings on disk, newest first.
#[tauri::command]
pub fn replay_recordings(app: tauri::AppHandle) -> Vec<Recorded> {
    let (cfg, _) = sys(&app);
    let dir = out_dir(&cfg);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Recorded> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4")) {
                return None;
            }
            let meta = e.metadata().ok();
            Some(Recorded {
                name: path.file_stem()?.to_string_lossy().into_owned(),
                bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Delete one recording.
///
/// Only from the folder recordings are written to, and only an `.mp4`: this command takes a
/// path from the window, and a delete that accepts any path is one a bug elsewhere turns into
/// a deleted project.
#[tauri::command]
pub fn replay_delete(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let (cfg, _) = sys(&app);
    let dir = out_dir(&cfg).canonicalize().map_err(|e| format!("{e}"))?;
    let target = PathBuf::from(&path).canonicalize().map_err(|e| format!("{e}"))?;
    if !target.starts_with(&dir) || !target.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4")) {
        return Err("That file isn't one of the Studio's recordings.".into());
    }
    std::fs::remove_file(&target).map_err(|e| format!("Couldn't delete it: {e}"))?;
    Ok(())
}

/// Where recordings are going, whether or not anything is there yet.
#[tauri::command]
pub fn replay_out_dir(app: tauri::AppHandle) -> String {
    out_dir(&sys(&app).0).to_string_lossy().into_owned()
}

/// Fetch an ffmpeg. Resolves with where it landed.
#[tauri::command]
pub async fn replay_fetch_ffmpeg(app: tauri::AppHandle) -> Result<String, String> {
    let path = fetch(&app).await?;
    if let Ok(mut slot) = app.state::<Recorder>().probe.lock() {
        *slot = None;
    }
    announce(&app);
    Ok(path.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// the hotkey
// ---------------------------------------------------------------------------

/// Point the configured combo at record/stop.
///
/// The reason this exists at all: the rider is inside a full-screen game and the Studio is
/// behind it. Alt-tabbing out to press a button in another window loses the shot, which is
/// the same problem OBS has and the same one everybody solves with a hotkey.
pub fn bind_hotkey(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let (cfg, _) = sys(app);
    let combo = match cfg.replay.hotkey.trim() {
        "" => replay::DEFAULT_RECORD_HOTKEY.to_string(),
        c => c.to_string(),
    };
    let shortcut = match combo.parse::<tauri_plugin_global_shortcut::Shortcut>() {
        Ok(s) => s,
        Err(_) => {
            log::warn!("[replay] {combo:?} isn't a shortcut we can register");
            return;
        }
    };
    // Unregister everything this app holds first: re-binding after a settings change would
    // otherwise leave the old combo live as well as the new one.
    let _ = app.global_shortcut().unregister_all();
    let res = app.global_shortcut().on_shortcut(shortcut, move |app, _s, event| {
        if event.state() != ShortcutState::Pressed {
            return;
        }
        let handle = app.clone();
        // Off the shortcut thread: stopping waits for ffmpeg to write its trailer, and a
        // hotkey callback that blocks holds up every other shortcut on the machine.
        std::thread::spawn(move || {
            let outcome = if handle.state::<Recorder>().is_live() {
                stop(&handle).map(|_| ())
            } else {
                start(&handle, Source::Manual).map(|_| ())
            };
            if let Err(e) = outcome {
                handle.state::<Recorder>().fault(e);
                announce(&handle);
            }
        });
    });
    match res {
        Ok(()) => log::info!("[replay] record hotkey registered: {combo}"),
        Err(e) => log::warn!("[replay] couldn't register {combo}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(ddagrab: bool, nvenc: bool) -> Probe {
        Probe { binary: PathBuf::from("ffmpeg"), ddagrab, nvenc, amf: false, qsv: false }
    }

    fn joined(cfg: &Recording, p: &Probe, c: Capture) -> String {
        args(cfg, p, c, Path::new("out.mp4")).join(" ")
    }

    /// The capture that works while the game owns the screen wins whenever it is there.
    /// Getting this backwards is a folder full of black videos nobody notices until later.
    #[test]
    fn desktop_duplication_is_preferred_when_ffmpeg_has_it() {
        assert_eq!(pick_capture(&probe(true, false), Some((0, 0, 1920, 1080))), Capture::Dda);
    }

    #[test]
    fn without_it_the_game_window_is_captured_where_it_sits() {
        let c = pick_capture(&probe(false, false), Some((100, 50, 1380, 770)));
        assert_eq!(c, Capture::Gdi { x: 100, y: 50, w: 1280, h: 720 });
    }

    /// No window to find — the game is not up, or we are on a machine that cannot say.
    /// The desktop is a worse recording than the window and a far better one than none.
    #[test]
    fn with_no_window_at_all_it_falls_back_to_the_desktop() {
        assert_eq!(pick_capture(&probe(false, false), None), Capture::GdiDesktop);
    }

    /// 4:2:0 chroma cannot describe an odd number of pixels, and ffmpeg's refusal names a
    /// pixel format rather than the window that was an odd size.
    #[test]
    fn an_odd_sized_window_is_rounded_to_something_encodable() {
        let cfg = Recording::default();
        let line = joined(&cfg, &probe(false, false), Capture::Gdi { x: 0, y: 0, w: 1921, h: 1081 });
        assert!(line.contains("1920x1080"), "{line}");
    }

    #[test]
    fn auto_takes_the_gpu_encoder_when_there_is_one() {
        let cfg = Recording { encoder: Encoder::Auto, ..Recording::default() };
        let line = joined(&cfg, &probe(true, true), Capture::Dda);
        assert!(line.contains("h264_nvenc"), "{line}");
    }

    #[test]
    fn auto_falls_back_to_the_cpu_encoder_that_is_always_there() {
        let cfg = Recording { encoder: Encoder::Auto, ..Recording::default() };
        let line = joined(&cfg, &probe(true, false), Capture::Dda);
        assert!(line.contains("libx264"), "{line}");
    }

    /// A choice made in Settings is a choice, not a suggestion: somebody who picked x264 on a
    /// machine with a flaky NVENC driver must get x264.
    #[test]
    fn a_named_encoder_wins_over_the_hardware_that_is_there() {
        let cfg = Recording { encoder: Encoder::X264, ..Recording::default() };
        let line = joined(&cfg, &probe(true, true), Capture::Dda);
        assert!(line.contains("libx264"), "{line}");
        assert!(!line.contains("nvenc"), "{line}");
    }

    /// Silence is the default, and the default must not put an input on the command line:
    /// a dshow device that doesn't exist fails the whole recording, video included.
    #[test]
    fn no_audio_device_means_no_audio_input() {
        let line = joined(&Recording::default(), &probe(true, false), Capture::Dda);
        assert!(!line.contains("dshow"), "{line}");
    }

    #[test]
    fn a_named_audio_device_is_recorded_alongside() {
        let cfg = Recording { audio_device: "Stereo Mix (Realtek)".into(), ..Recording::default() };
        let line = joined(&cfg, &probe(true, false), Capture::Dda);
        assert!(line.contains("audio=Stereo Mix (Realtek)"), "{line}");
        assert!(line.contains("aac"), "{line}");
    }

    /// Whatever else changes, the file has to be one a player and an editor will both open.
    #[test]
    fn every_command_line_ends_in_a_playable_file() {
        for capture in [Capture::Dda, Capture::GdiDesktop] {
            let line = joined(&Recording::default(), &probe(true, false), capture);
            assert!(line.contains("-pix_fmt yuv420p"), "{line}");
            assert!(line.contains("+faststart"), "{line}");
            assert!(line.ends_with("out.mp4"), "{line}");
        }
    }

    /// A frame rate typed into a config by hand cannot ask for something the encoder will
    /// refuse — and 0 fps is a recording that never produces a frame.
    #[test]
    fn a_silly_frame_rate_is_brought_back_into_range() {
        let cfg = Recording { fps: 0, ..Recording::default() };
        let line = joined(&cfg, &probe(false, false), Capture::GdiDesktop);
        assert!(line.contains("-framerate 24"), "{line}");
    }

    #[test]
    fn the_stamp_is_a_name_windows_will_accept() {
        let s = stamp();
        assert!(!s.contains(':'), "{s}");
        assert_eq!(s.len(), "2026-09-17 18-40-02".len(), "{s}");
    }

    #[test]
    fn the_calendar_agrees_with_a_date_we_know() {
        // 2026-09-17T18:40:02Z.
        assert_eq!(civil(1_789_670_402), (2026, 9, 17, 18, 40, 2));
        // The epoch itself, and a leap day.
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_709_164_800).0, 2024);
        assert_eq!((civil(1_709_164_800).1, civil(1_709_164_800).2), (2, 29));
    }

    #[test]
    fn a_second_take_in_the_same_second_does_not_overwrite_the_first() {
        let dir = std::env::temp_dir().join(format!("frost-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique(&dir, "take");
        std::fs::write(&first, b"x").unwrap();
        let second = unique(&dir, "take");
        assert_ne!(first, second);
        let _ = std::fs::remove_file(&first);
    }
}
