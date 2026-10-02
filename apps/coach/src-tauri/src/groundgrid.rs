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
//! A locked (`.mxbsecure`) track can't be read here, so it gets no file, and the recorder falls
//! back to snapping the line to what the game draws.

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
    if src.locked {
        return Err("the track is locked (.mxbsecure), so its terrain can't be read; the recorder snaps the line to what the game draws".into());
    }
    let master = mxb_core::track::load_master(app, &src.path, src.prefix.as_deref()).map_err(|e| e.to_string())?;
    let i = &master.info;
    let bytes = write(i.width as usize, i.height as usize, i.metres_per_sample, &master.heights)
        .ok_or("the terrain is empty or too large")?;
    let dir = sheets_dir(&cfg).ok_or("the game's user folder wasn't found")?;
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
}
