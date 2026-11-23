//! Coach telling the recorder it is open: `<game user folder>/mxbcoach/coach.ini`.
//!
//! The recorder draws the line, the hints and the cue box from sheets Coach leaves on disk, so
//! before this it kept drawing them after Coach was closed, and nothing the rider switched off
//! in a Coach that wasn't running could reach it (GAMING PROFI 2, recorder 0.49.7). Now Coach
//! rewrites this file every [`BEAT`] while it runs and writes `on=0` on the way out. The
//! recorder (FrostMod 0.49.13, `coachlink.h`) clears what it draws and stops drawing when the
//! file says `on=0` or hasn't been written for a few seconds — Coach closed, crashed or hung.
//! With no file at all it draws as before, so an older Coach keeps working.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;

use crate::coach::{load_config, session_dirs};

pub const FILE: &str = "coach.ini";
/// How often the file is rewritten. The recorder gives up after three of these go by.
pub const BEAT: Duration = Duration::from_secs(2);
/// The first recorder that reads this file and clears when Coach is off or gone.
pub(crate) const CLEAR_NEEDS: &str = "0.49.13";

/// What the file says: Coach is on (`on=1`) or has gone (`on=0`), and which Coach.
pub fn text(on: bool, version: &str) -> String {
    format!("[coach]\non={}\nversion={version}\n", u8::from(on))
}

/// Written aside and moved in, so the recorder never reads half a file.
pub fn write(dir: &Path, on: bool, version: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    fs::write(&tmp, text(on, version))?;
    fs::rename(&tmp, dir.join(FILE))
}

fn beat(app: &AppHandle, on: bool) {
    let Some(dir) = crate::hud::coach_dir_of(&session_dirs(&load_config(app))) else {
        return;
    };
    // Only where the recorder has been: the folder is the game's, not Coach's to create.
    if !dir.is_dir() {
        return;
    }
    let version = app.package_info().version.to_string();
    if let Err(e) = write(&dir, on, &version) {
        log::warn!("coach.ini not written: {e}");
    }
}

/// Set on the way out, so a beat already under way doesn't write `on=1` over the `on=0`.
static STOPPED: AtomicBool = AtomicBool::new(false);
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Beat for as long as Coach runs.
pub fn start(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        {
            let _w = WRITING.lock();
            if STOPPED.load(Ordering::SeqCst) {
                return;
            }
            beat(&handle, true);
        }
        std::thread::sleep(BEAT);
    });
}

/// On the way out: tell the recorder now rather than let it wait for the beat to stop.
pub fn stop(app: &AppHandle) {
    let _w = WRITING.lock();
    STOPPED.store(true, Ordering::SeqCst);
    beat(app, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_says_on_or_off() {
        assert_eq!(text(true, "0.1.25"), "[coach]\non=1\nversion=0.1.25\n");
        assert_eq!(text(false, "0.1.25"), "[coach]\non=0\nversion=0.1.25\n");
    }

    #[test]
    fn written_in_one_piece_where_the_recorder_reads() {
        let dir = std::env::temp_dir().join(format!("coach-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write(&dir, true, "1").unwrap();
        write(&dir, false, "1").unwrap();
        assert_eq!(fs::read_to_string(dir.join(FILE)).unwrap(), text(false, "1"));
        assert!(!dir.join(format!("{FILE}.tmp")).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
