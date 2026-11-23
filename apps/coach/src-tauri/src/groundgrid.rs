//! The track's own ground for the recorder's line on the track: `<track>.ground` beside the cue
//! sheets, written from the track's heightfield (its `.trh`, read by `mxb_core::track`) the moment
//! the track is first seen being ridden, so the line lies on the dirt from the first second of the
//! first lap, with no lap needed.
//!
//! FrostMod's `src/coachline.h` (`GroundGrid`), `MXGR` version 1, little-endian:
//! magic, version, columns, rows, metres between samples, world x and z of the first sample, base
//! height, metres per unit, then columns x rows u16 row-major (row = z): base + unit * value, with
//! 0xFFFF where the height isn't known. The grid and the telemetry share the game's world frame
//! (`ground.rs`); the recorder checks that once from the rider's own samples and logs it.
//!
//! A secured (`.mxbsecure`) track gets no file, whether or not a key is installed for it: secured
//! content is never written to disk in a usable form, and a terrain grid is exactly that. The
//! plugin falls back to snapping the line to what the game draws. (Simplest safe option: skip,
//! rather than a memory-only channel.) Any `.ground` an earlier build left for one is swept away
//! on start.
//!
//! Grids are also made ahead of any session: on start, and when `mods/tracks` changes, for every
//! unsecured track that has none (or whose file is older than the track's), one at a time.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::AppHandle;

use crate::coach::{load_config, sheets_dir};

pub const MAGIC: &[u8; 4] = b"MXGR";
pub const VERSION: u32 = 1;
/// No finer than this between samples: a metre is what the recorder's ribbon can use, and keeps a
/// kilometre-square track to a couple of megabytes.
const MIN_STEP_M: f32 = 1.0;
const MAX_DIM: usize = 4096;
const UNKNOWN: u16 = 0xFFFF;

/// The file for this track.
pub fn file_name(track: &str) -> String {
    format!("{}.ground", crate::cues::safe_name(track))
}

/// The grid file for a heightfield `width` x `height` at `mps` metres a sample (row-major, row = z,
/// first sample at the world origin, as `ground::height_at` reads it).
pub fn write(width: usize, height: usize, mps: f32, heights: &[f32]) -> Option<Vec<u8>> {
    if width < 2 || height < 2 || !(mps > 0.0) || heights.len() < width * height {
        return None;
    }
    let stride = ((MIN_STEP_M / mps).round() as usize).max(1);
    let (w, h) = ((width - 1) / stride + 1, (height - 1) / stride + 1);
    if w < 2 || h < 2 || w > MAX_DIM || h > MAX_DIM {
        return None;
    }
    let at = |c: usize, r: usize| heights[(r * stride) * width + c * stride];
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for r in 0..h {
        for c in 0..w {
            let v = at(c, r);
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
    }
    if !lo.is_finite() {
        return None;
    }
    // A millimetre a unit where the relief allows it, coarser only for a track over 65 m tall.
    let unit = ((hi - lo) / 65000.0).max(0.001);
    let mut b = MAGIC.to_vec();
    for v in [VERSION, w as u32, h as u32] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for v in [mps * stride as f32, 0.0, 0.0, lo, unit] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for r in 0..h {
        for c in 0..w {
            let v = at(c, r);
            let q = if v.is_finite() { (((v - lo) / unit).round() as u32).min(65534) as u16 } else { UNKNOWN };
            b.extend_from_slice(&q.to_le_bytes());
        }
    }
    Some(b)
}

/// Tracks done this run, so a session file growing every second doesn't re-read a terrain.
fn done() -> &'static Mutex<HashSet<String>> {
    static DONE: std::sync::OnceLock<Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    DONE.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The track a recorder session is on, from its first record (the event), without reading the
/// rest of a file that may be tens of megabytes and still growing.
pub fn track_of(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = vec![0u8; 8 + 8 + 820];
    let mut f = fs::File::open(path).ok()?;
    f.read_exact(&mut head).ok()?;
    if &head[..4] != b"MXBC" || head[8] != 1 {
        return None;
    }
    let len = u32::from_le_bytes(head[12..16].try_into().ok()?) as usize;
    if len < 544 {
        return None;
    }
    // SPluginsBikeEvent_t::m_szTrackID, char[100] at 444.
    let id = &head[16 + 444..16 + 544];
    let end = id.iter().position(|&c| c == 0).unwrap_or(id.len());
    let s = String::from_utf8_lossy(&id[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Writes `<track>.ground` for the track this session is on, once per run per track, when its
/// terrain can be read. Called from the session watcher; slow the first time (the heightfield is
/// inflated and cached), so off the watcher's thread.
pub fn ensure_for(app: &AppHandle, session: &Path) {
    let Some(track) = track_of(session) else { return };
    if !done().lock().map(|mut d| d.insert(track.clone())).unwrap_or(false) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || match build(&app, &track) {
        Ok(p) => log::info!("track ground for {track} written to {}", p.display()),
        Err(why) => log::info!("no track ground for {track}: {why}"),
    });
}

fn build(app: &AppHandle, track: &str) -> Result<PathBuf, String> {
    let cfg = load_config(app);
    let src = mxb_core::tracksource::resolve(&cfg, track).ok_or("the track isn't in the mods folder")?;
    build_from(app, &cfg, track, &src)
}

/// Whether a track's terrain must never reach disk: locked, or secured with or without a key.
fn withheld(src: &mxb_core::tracksource::TrackSource) -> bool {
    src.locked || (!src.stock && protected(Path::new(&src.path)))
}

/// Secured, or a GUID-locked archive. A GUID-locked `.pkz` is not a plain zip (it doesn't start
/// with the zip magic), and the app can read it only through its sidecar, so `locked` is false
/// whenever that reader works; the container itself is the tell. Folders and missing files are
/// not protected.
fn protected(path: &Path) -> bool {
    mxb_core::securesource::is_secured(path) || (path.is_file() && !mxb_core::pkz::is_plain_zip(path))
}

fn build_from(
    app: &AppHandle,
    cfg: &mxb_core::config::AppConfig,
    track: &str,
    src: &mxb_core::tracksource::TrackSource,
) -> Result<PathBuf, String> {
    if withheld(src) {
        return Err("the track is secured (.mxbsecure), so its terrain is never written out; the recorder snaps the line to what the game draws".into());
    }
    let master = mxb_core::track::load_master(app, &src.path, src.prefix.as_deref()).map_err(|e| e.to_string())?;
    let i = &master.info;
    let bytes = write(i.width as usize, i.height as usize, i.metres_per_sample, &master.heights)
        .ok_or("the terrain is empty or too large")?;
    let dir = sheets_dir(cfg).ok_or("the game's user folder wasn't found")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(file_name(track));
    let tmp = dir.join(format!("{}.tmp", file_name(track)));
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    // Moved in whole, so the recorder never reads half a grid.
    if file.exists() {
        let _ = fs::remove_file(&file);
    }
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Removes `.ground` files in `dir` whose track is in `secured` (compared by letters and digits,
/// as `tracksource::key`). Returns how many went.
fn sweep_dir(dir: &Path, secured: &HashSet<String>) -> usize {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()).is_none_or(|x| !x.eq_ignore_ascii_case("ground")) {
            continue;
        }
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if secured.contains(&mxb_core::tracksource::key(&stem)) && fs::remove_file(&p).is_ok() {
            log::info!("deleted the track ground for locked track {stem}");
            n += 1;
        }
    }
    n
}

/// Whether `file` is missing or older than the track at `pkz`.
fn stale(file: &Path, pkz: &Path) -> bool {
    let m = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    match (m(file), m(pkz)) {
        (None, _) => true,
        (Some(f), Some(t)) => f < t,
        _ => false,
    }
}

static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Between tracks, so a large library doesn't hold the disk and a core for a minute.
const THROTTLE: std::time::Duration = std::time::Duration::from_millis(750);

/// In the background: removes grids left for secured tracks, then makes one for every unsecured
/// installed track that lacks a current one. Safe to call often; a run already going wins.
pub fn prepare_all(app: &AppHandle) {
    if BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        run_all(&app);
        BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

fn run_all(app: &AppHandle) {
    let cfg = load_config(app);
    let Some(dir) = sheets_dir(&cfg) else { return };
    let entries = mxb_core::library::scan_library(&cfg.mods_path, "mods/tracks", &[], cfg.game()).unwrap_or_default();
    let secured: HashSet<String> = entries
        .iter()
        .filter(|e| e.secured || e.locked || protected(Path::new(&e.path)))
        .map(|e| mxb_core::tracksource::key(&mxb_core::library::strip_ext(&e.name)))
        .collect();
    let gone = sweep_dir(&dir, &secured);
    if gone > 0 {
        log::info!("removed {gone} track ground file(s) of secured tracks");
    }
    for e in entries {
        if e.secured || e.locked || protected(Path::new(&e.path)) {
            continue;
        }
        let id = mxb_core::track::folder_name(Path::new(&e.path)).unwrap_or_else(|| mxb_core::library::strip_ext(&e.name));
        if !stale(&dir.join(file_name(&id)), Path::new(&e.path)) {
            continue;
        }
        let Some(src) = mxb_core::tracksource::resolve(&cfg, &id) else { continue };
        match build_from(app, &cfg, &id, &src) {
            Ok(p) => log::info!("track ground for {id} written to {}", p.display()),
            Err(why) => log::info!("no track ground for {id}: {why}"),
        }
        std::thread::sleep(THROTTLE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_at(b: &[u8], i: usize) -> f32 {
        f32::from_le_bytes(b[i..i + 4].try_into().unwrap())
    }
    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
    }

    #[test]
    fn writes_the_grid_the_recorder_reads() {
        // 4 x 3 at half a metre: thinned to every other sample, a metre apart.
        let heights: Vec<f32> = (0..12).map(|i| 10.0 + i as f32 * 0.25).collect();
        let b = write(4, 3, 0.5, &heights).unwrap();
        assert_eq!(&b[..4], b"MXGR");
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!((u32_at(4), u32_at(8), u32_at(12)), (1, 2, 2));
        assert_eq!((f32_at(&b, 16), f32_at(&b, 20), f32_at(&b, 24)), (1.0, 0.0, 0.0));
        let (base, unit) = (f32_at(&b, 28), f32_at(&b, 32));
        assert_eq!(base, 10.0);
        // Samples (0,0), (2,0), (0,2), (2,2) of the original: 10, 10.5, 12, 12.5.
        let back: Vec<f32> = (0..4).map(|k| base + unit * u16_at(&b, 36 + k * 2) as f32).collect();
        for (got, want) in back.iter().zip([10.0, 10.5, 12.0, 12.5]) {
            assert!((got - want).abs() < 0.002, "{got} vs {want}");
        }
        assert_eq!(b.len(), 36 + 4 * 2);
    }

    #[test]
    fn unknown_heights_stay_unknown_and_nothing_empty_is_written() {
        let mut heights = vec![1.0f32; 9];
        heights[4] = f32::NAN;
        let b = write(3, 3, 1.0, &heights).unwrap();
        assert_eq!(u16_at(&b, 36 + 4 * 2), UNKNOWN);
        assert!(write(3, 3, 1.0, &[f32::NAN; 9]).is_none());
        assert!(write(1, 3, 1.0, &[1.0; 3]).is_none());
    }

    #[test]
    fn the_track_is_read_from_the_session_head() {
        let dir = std::env::temp_dir().join(format!("coach-gg-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("s.mxbc");
        let mut b = b"MXBC".to_vec();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&[1, 0, 0, 0]);
        b.extend_from_slice(&820u32.to_le_bytes());
        let mut ev = vec![0u8; 820];
        ev[444..444 + 9].copy_from_slice(b"Ridgedale");
        b.extend_from_slice(&ev);
        fs::write(&path, &b).unwrap();
        assert_eq!(track_of(&path).as_deref(), Some("Ridgedale"));
        assert_eq!(file_name("Steezy Mx - SMX - Ridgedale"), "Steezy_Mx_-_SMX_-_Ridgedale.ground");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_secured_track_is_withheld_with_or_without_a_key_and_its_old_ground_is_swept() {
        let src = |path: &str| mxb_core::tracksource::TrackSource {
            path: path.into(),
            prefix: None,
            name: "x".into(),
            stock: false,
            locked: false, // a key is installed: not "locked"
        };
        assert!(withheld(&src("C:/mods/tracks/755 Compound.mxbsecure")));
        assert!(!withheld(&src("C:/mods/tracks/Ridgedale.pkz")));

        // A GUID-locked .pkz: not a zip, yet `locked` is false because the reader opened it.
        let tdir = std::env::temp_dir().join(format!("coach-gg-guid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tdir);
        fs::create_dir_all(&tdir).unwrap();
        let opaque = tdir.join("Fake Locked.pkz");
        fs::write(&opaque, b" not a zip, fake guid-locked body").unwrap();
        let plain = tdir.join("Fake Open.pkz");
        fs::write(&plain, b"PK empty zip").unwrap();
        assert!(withheld(&src(&opaque.to_string_lossy())));
        assert!(!withheld(&src(&plain.to_string_lossy())));
        assert!(protected(&opaque) && !protected(&plain) && !protected(&tdir));
        let _ = fs::remove_dir_all(&tdir);

        let dir = std::env::temp_dir().join(format!("coach-gg-sweep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for f in ["755_Compound.ground", "Ridgedale.ground", "755_Compound.cue"] {
            fs::write(dir.join(f), b"x").unwrap();
        }
        let secured: HashSet<String> = [mxb_core::tracksource::key("755 Compound")].into();
        assert_eq!(sweep_dir(&dir, &secured), 1);
        assert!(!dir.join("755_Compound.ground").exists());
        assert!(dir.join("Ridgedale.ground").exists() && dir.join("755_Compound.cue").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_track_without_a_ground_is_due_and_a_current_one_is_not() {
        let dir = std::env::temp_dir().join(format!("coach-gg-stale-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pkz = dir.join("t.pkz");
        fs::write(&pkz, b"z").unwrap();
        let ground = dir.join("t.ground");
        assert!(stale(&ground, &pkz));
        fs::write(&ground, b"g").unwrap();
        assert!(!stale(&ground, &pkz));
        let _ = fs::remove_dir_all(&dir);
    }
}
