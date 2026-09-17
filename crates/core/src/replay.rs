//! The Replay Mod, as the Studio needs to see it: where its files live, which camera paths
//! are saved, and the one file the mod writes to say a take is running.
//!
//! The mod itself is a paid plugin — a DLL that runs inside MX Bikes and a panel that runs in
//! the Studio — and neither half is in this repository. What is here is the part both halves
//! have to agree about: a folder, a file extension, and a small JSON handshake. It lives in
//! the shared core rather than in `apps/studio` because the manager also has to find the same
//! folder to install the payload into, and two copies of a path is how the two apps start
//! disagreeing about where a rider's camera paths are.
//!
//! **The handshake, in one paragraph.** The mod writes `FrostReplay/take.json` when it starts
//! flying a path and rewrites it when it stops. The Studio watches that file, and while it
//! says `playing` it records the game window to a video file. That is the whole contract, and
//! it is deliberately a file rather than a socket or a named pipe: the game is a Windows
//! process that may be running inside a Wine prefix with the Studio outside it, and a file in
//! the game's own user folder is the one channel that works in every arrangement the app
//! already supports — the same reasoning as FrostMod's command file.
//!
//! See `docs/replay/RECORDING.md` for the format in full, including what the mod must write.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The mod's folder inside the game's user folder (`Documents\PiBoSo\MX Bikes`).
pub const FOLDER: &str = "FrostReplay";

/// The file the mod writes while a take is running. See [`Take`].
pub const TAKE_FILE: &str = "take.json";

/// What a saved camera path is called on disk. Nine slots, one file each, shareable.
pub const SLOT_EXT: &str = "fcam";

/// How long after a take's last heartbeat we stop believing it. See [`Take::is_stale`].
///
/// The mod rewrites `take.json` as it plays, so a file that has stopped moving means the game
/// went away — a crash, an alt-F4, a Wine prefix that died. Ten seconds is long enough to
/// survive a stutter or a slow disk and short enough that a dead take does not leave a
/// recorder running over a black screen for the rest of the evening.
pub const HEARTBEAT_GRACE_MS: u64 = 10_000;

/// The mod's folder, whether or not it exists yet.
pub fn folder(user_dir: &Path) -> PathBuf {
    user_dir.join(FOLDER)
}

/// One saved camera path.
///
/// Nothing here reads inside a `.fcam`: the format belongs to the mod and the Studio has no
/// business parsing a file it did not write. What it can honestly say is which slots are
/// filled and when each was last saved, which is what a list of them is for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    /// File name without the extension — `slot1`, or whatever a shared path was called.
    pub name: String,
    pub path: String,
    /// The slot number, when the name is one of the mod's own nine. `None` for a path that
    /// arrived from somebody else under its own name.
    pub slot: Option<u32>,
    pub bytes: u64,
    /// Unix milliseconds, `0` when the file system will not say.
    pub modified: u64,
}

/// The camera paths in a folder, newest first.
///
/// An unreadable or absent folder is an empty list, not an error: "the mod has never been
/// run" and "the mod is not installed" are the same answer to the only question being asked.
pub fn slots(dir: &Path) -> Vec<Slot> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Slot> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case(SLOT_EXT)) {
                return None;
            }
            let name = path.file_stem()?.to_string_lossy().into_owned();
            let meta = e.metadata().ok();
            Some(Slot {
                slot: slot_number(&name),
                bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                path: path.to_string_lossy().into_owned(),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    out
}

/// `slot3` -> `Some(3)`. Anything else is a path with a name of its own.
fn slot_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("slot").or_else(|| name.strip_prefix("Slot"))?;
    rest.parse().ok()
}

/// What the mod is doing, as `take.json` reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TakeState {
    /// A camera path is being flown right now. This is the one that starts a recording.
    Playing,
    /// The mod is loaded and nothing is being flown.
    #[default]
    Idle,
}

/// The take signal: what the mod says it is doing, and enough about it to name a file.
///
/// Every field but `state` is optional on purpose. A recorder that refuses to start because a
/// mod build left the track name out would be trading the whole feature for a tidier file
/// name, and the file name has a fallback while the recording does not.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Take {
    /// Format version. A newer one is not read — see [`read_take`].
    pub v: u32,
    pub state: TakeState,
    /// Which slot is playing, when it came from one.
    pub slot: Option<u32>,
    /// The track the replay is on, for the file name.
    pub track: String,
    /// The rider being followed, when the path follows one.
    pub rider: String,
    /// Unix milliseconds the take started.
    pub started_at: u64,
    /// Unix milliseconds the mod last rewrote this file. See [`Take::is_stale`].
    pub beat_at: u64,
    /// How long the path runs, when the mod knows. Milliseconds; `0` means it does not.
    pub duration_ms: u64,
}

/// The take format this build understands.
pub const TAKE_VERSION: u32 = 1;

impl Take {
    /// Has the mod stopped saying anything?
    ///
    /// A take whose heartbeat has gone quiet is over whatever the file still claims: the game
    /// is not there to write a closing `idle` when it crashes, and a recorder that waits for
    /// one records until the disk fills.
    pub fn is_stale(&self, now_ms: u64) -> bool {
        let beat = self.beat_at.max(self.started_at);
        beat == 0 || now_ms.saturating_sub(beat) > HEARTBEAT_GRACE_MS
    }

    /// A recording's base name, without an extension.
    ///
    /// Three parts, each dropped when it is not known: the track, the slot, and the time it
    /// was taken. The time is always there, which is what keeps two takes on the same slot
    /// from being one file.
    pub fn recording_name(&self, stamp: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let track = sanitize(&self.track);
        if !track.is_empty() {
            parts.push(track);
        }
        if let Some(n) = self.slot {
            parts.push(format!("slot {n}"));
        }
        parts.push(stamp.to_string());
        parts.join(" - ")
    }
}

/// Read the take signal, or `None` when there isn't one to read.
///
/// A file that is half-written (the mod is rewriting it as we look) parses as nothing and is
/// simply read again on the next pass — which is why the watcher polls rather than acting on
/// a single failed read.
pub fn read_take(dir: &Path) -> Option<Take> {
    let body = std::fs::read(dir.join(TAKE_FILE)).ok()?;
    let take: Take = serde_json::from_slice(&body).ok()?;
    // A newer format is refused rather than half-read: the fields we know may have been given
    // new meanings, and guessing produces a recording of the wrong thing rather than an error.
    (take.v <= TAKE_VERSION).then_some(take)
}

/// Strip what a file name may not hold, and flatten the runs of spaces that leaves behind.
///
/// A track name is free text out of a mod folder and it ends up in a path, so the separators
/// go first. What is left after that can still be `..`, which is not a traversal once the
/// slashes are gone but is not a name either — so a word made only of dots is dropped rather
/// than left at the front of somebody's recording.
fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if (c as u32) < 0x20 => ' ',
            c => c,
        })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|word| !word.chars().all(|c| c == '.'))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// what the recorder is set to do
// ---------------------------------------------------------------------------

/// How much the encoder is asked to keep. Three names rather than a CRF box, because the
/// number is meaningless to everyone who has not read the x264 documentation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// Near-transparent. What you want if the file is going into an editor.
    High,
    /// The default: an evening of takes fits on a laptop and still uploads well.
    #[default]
    Balanced,
    /// Small enough to send to someone.
    Small,
}

impl Quality {
    /// The x264/x265-style constant-rate factor this asks for. Lower keeps more.
    pub fn crf(self) -> u32 {
        match self {
            Quality::High => 17,
            Quality::Balanced => 21,
            Quality::Small => 26,
        }
    }

    /// What a hardware encoder gets instead, which take a quality scale rather than a CRF.
    pub fn cq(self) -> u32 {
        match self {
            Quality::High => 19,
            Quality::Balanced => 23,
            Quality::Small => 28,
        }
    }
}

/// Which encoder does the work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoder {
    /// Let the recorder pick: the GPU's encoder when ffmpeg has one built in, else x264.
    /// Chosen because encoding a 60 fps capture on the CPU is the one setting that can cost
    /// the rider frames in the game they are recording.
    #[default]
    Auto,
    /// libx264. Always there, always works, costs CPU.
    X264,
    /// NVIDIA.
    Nvenc,
    /// AMD.
    Amf,
    /// Intel Quick Sync.
    Qsv,
}

/// What the Studio's recorder is set to do. Stored in the app config, shared by both apps
/// because the config is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Recording {
    /// Record on its own when the mod says a take has started.
    ///
    /// **On by default**, which is the whole point of the feature: the thing it replaces is
    /// remembering to press record in OBS, and a switch you have to find first replaces
    /// nothing. It cannot start a recording without the mod saying so, so "on" costs a
    /// stopped watcher thread to anybody who does not own the Replay Mod.
    pub auto: bool,
    /// Where finished recordings land. Blank means the system's Videos folder, in
    /// `Frost Replays`.
    pub dir: String,
    /// Frames a second. 60 by default: these are replays of a motorcycle.
    pub fps: u32,
    pub quality: Quality,
    pub encoder: Encoder,
    /// ffmpeg to run. Blank means the one the Studio fetched, then whatever is on `PATH`.
    pub ffmpeg_path: String,
    /// A DirectShow audio device to record with, exactly as ffmpeg names it. Blank records
    /// silence — which is the right default, because the alternative on Windows is naming a
    /// device that may not exist and failing the whole recording over the audio half of it.
    pub audio_device: String,
    /// Start and stop a recording by hand without leaving the game. Tauri accelerator syntax;
    /// blank falls back to [`DEFAULT_RECORD_HOTKEY`].
    pub hotkey: String,
    /// Stop a recording after this many minutes whatever the mod says.
    ///
    /// The backstop for the case the heartbeat cannot cover: a take that really is playing,
    /// for hours, because somebody left a looping path running. Twenty minutes is longer than
    /// any single camera path and far shorter than a full disk.
    pub max_minutes: u32,
}

/// Start/stop recording without alt-tabbing out of the game.
pub const DEFAULT_RECORD_HOTKEY: &str = "CommandOrControl+Shift+R";

impl Default for Recording {
    fn default() -> Self {
        Self {
            auto: true,
            dir: String::new(),
            fps: 60,
            quality: Quality::default(),
            encoder: Encoder::default(),
            ffmpeg_path: String::new(),
            audio_device: String::new(),
            hotkey: DEFAULT_RECORD_HOTKEY.to_string(),
            max_minutes: 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "frost-replay-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn slots_are_the_fcam_files_and_nothing_else() {
        let d = tempdir();
        std::fs::write(d.join("slot1.fcam"), b"x").unwrap();
        std::fs::write(d.join("slot2.FCAM"), b"xx").unwrap();
        std::fs::write(d.join("notes.txt"), b"xxx").unwrap();
        std::fs::write(d.join("take.json"), b"{}").unwrap();

        let got = slots(&d);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got.iter().all(|s| s.slot.is_some()), "{got:?}");
    }

    /// A path someone was sent is not one of the nine, and has to list anyway — the whole
    /// point of `.fcam` being a file is that it can be passed to another rider.
    #[test]
    fn a_shared_path_lists_without_a_slot_number() {
        let d = tempdir();
        std::fs::write(d.join("dobels-whip-cam.fcam"), b"x").unwrap();
        let got = slots(&d);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].slot, None);
        assert_eq!(got[0].name, "dobels-whip-cam");
    }

    #[test]
    fn a_missing_folder_lists_nothing_rather_than_failing() {
        assert!(slots(Path::new("/nowhere/at/all/FrostReplay")).is_empty());
    }

    #[test]
    fn a_take_from_a_newer_mod_is_refused_rather_than_half_read() {
        let d = tempdir();
        std::fs::write(d.join(TAKE_FILE), br#"{"v":99,"state":"playing"}"#).unwrap();
        assert_eq!(read_take(&d), None);
    }

    #[test]
    fn a_take_reads_what_the_mod_wrote() {
        let d = tempdir();
        std::fs::write(
            d.join(TAKE_FILE),
            br#"{"v":1,"state":"playing","slot":3,"track":"Aztec MX","startedAt":1000,"beatAt":2000}"#,
        )
        .unwrap();
        let take = read_take(&d).expect("a take");
        assert_eq!(take.state, TakeState::Playing);
        assert_eq!(take.slot, Some(3));
        assert_eq!(take.track, "Aztec MX");
    }

    /// The crash case: the game died mid-take and nobody wrote the closing `idle`.
    #[test]
    fn a_take_whose_heartbeat_stopped_is_over() {
        let take = Take { beat_at: 1_000, ..Take::default() };
        assert!(!take.is_stale(1_000 + HEARTBEAT_GRACE_MS));
        assert!(take.is_stale(1_000 + HEARTBEAT_GRACE_MS + 1));
    }

    /// A take that has never beaten at all is not a take. Without this a `take.json` left on
    /// disk by a previous session reads as a live one the moment the Studio opens.
    #[test]
    fn a_take_with_no_heartbeat_at_all_is_over() {
        assert!(Take::default().is_stale(0));
    }

    #[test]
    fn a_recording_is_named_after_the_track_and_the_slot() {
        let take = Take { slot: Some(3), track: "Aztec MX".into(), ..Take::default() };
        assert_eq!(take.recording_name("2026-09-17 18-40-02"), "Aztec MX - slot 3 - 2026-09-17 18-40-02");
    }

    /// A track name is free text from a mod folder, and lands in a file name.
    #[test]
    fn a_track_name_cannot_write_outside_the_folder() {
        let take = Take { track: "../../evil: name".into(), ..Take::default() };
        let name = take.recording_name("now");
        assert_eq!(name, "evil name - now");
    }

    /// Nothing known but the clock still names a file, because the alternative is a take that
    /// is not recorded over a detail that does not matter.
    #[test]
    fn a_take_that_says_nothing_still_names_a_file() {
        assert_eq!(Take::default().recording_name("now"), "now");
    }
}
