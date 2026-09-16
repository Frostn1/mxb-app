//! What the recorder plugin draws and says in the game, as Coach sets it: `hud.ini` and
//! `cues/voice.ini`, both under `<game user folder>/mxbcoach`. The plugin reads them; a key
//! that isn't there takes the plugin's own default, so only what the rider picks is written.

use std::fs;
use std::path::{Path, PathBuf};

use mxb_core::config::AppConfig;
use serde::Serialize;
use tauri::AppHandle;

use crate::coach::{load_config, session_dirs};
use crate::ini;

/// The HUD parts, in the order the overlay lists them.
pub const HUD_PARTS: &[(&str, &str)] = &[
    ("cue", "Live cue"),
    ("section", "Section and tip"),
    ("gap", "Gap to Coach's lap"),
    ("stance", "Sit / stand"),
    ("map", "Track map with ghost and cues"),
    ("setup", "Setup card (when stopped)"),
];

const DEFAULT_VOLUME: u8 = 80;

/// The plugin's own cue box: centred across the screen, just under MXBMRP3's panels
/// (FrostMod `src/coachhud.h`, `kCueBox`). Coach writes the centre of that box, so the
/// middle-centre position is exactly where the cue has always been.
pub const DEFAULT_CUE_POS: [f32; 2] = [0.5, 0.31];
/// Kept off the very edge, where the game would clip the text.
const CUE_POS_RANGE: (f32, f32) = (0.05, 0.95);

/// The other plugin that draws a track map. Beside the recorder in the game's plugins folder.
const MXBMRP3: &str = "mxbmrp3.dlo";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudPart {
    pub key: &'static str,
    pub label: &'static str,
    pub on: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud {
    pub enabled: bool,
    pub parts: Vec<HudPart>,
    pub file: String,
    /// MXBMRP3 is installed beside the recorder, so it draws a track map of its own. The map
    /// switch still decides whether Coach draws one — this only explains why two might show.
    pub mxbmrp3: bool,
    /// Where the live cue sits on screen, as a fraction of the width and height.
    pub cue_pos: [f32; 2],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub enabled: bool,
    pub volume: u8,
}

/// The version of the recorder the HUD and the spoken cues need.
pub(crate) const RECORDER_NEEDS: &str = "0.23";
/// The version that reads where the cue box goes. Coach writes the keys either way: an older
/// recorder ignores them, so nothing breaks by setting a position early.
pub(crate) const CUE_POS_NEEDS: &str = "0.24";

/// `<user folder>/mxbcoach`, out of the folders the sessions are looked for in: the one the
/// recorder actually uses, so the plugin reads what Coach writes. The first when the recorder
/// hasn't written anywhere yet.
pub(crate) fn coach_dir_of(dirs: &[PathBuf]) -> Option<PathBuf> {
    let coach: Vec<&Path> = dirs.iter().filter_map(|d| d.parent()).collect();
    let used = coach.iter().find(|c| c.is_dir() || c.join("sessions").is_dir());
    used.or(coach.first()).map(|c| c.to_path_buf())
}

fn coach_dir(app: &AppHandle) -> Result<PathBuf, String> {
    coach_dir_of(&session_dirs(&load_config(app)))
        .ok_or_else(|| "The game's user folder wasn't found.".to_string())
}

/// Whether MXBMRP3 sits next to the recorder, which is the same test the plugin makes.
pub(crate) fn mxbmrp3_installed(cfg: &AppConfig) -> bool {
    let dir = cfg.install_dir();
    !dir.trim().is_empty() && Path::new(&dir).join("plugins").join(MXBMRP3).is_file()
}

/// The recorder's own version, from the `recorder.ini` it writes when the game runs it.
pub(crate) fn recorder_version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("recorder.ini")).ok()?;
    let version = ini::get(&ini::read_section(&text, "recorder"), "version")?.trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// `have` is `want` or newer, by number and not by text: 0.9 is older than 0.23, though it
/// sorts after it. Anything after the numbers ("0.23.0-beta.1") counts as that version.
pub(crate) fn at_least(have: &str, want: &str) -> bool {
    fn parts(v: &str) -> Option<[u32; 3]> {
        let v = v.trim().trim_start_matches('v');
        let mut out = [0; 3];
        let mut read = false;
        for (i, p) in v.split(['-', '+']).next().unwrap_or(v).split('.').take(3).enumerate() {
            out[i] = p.trim().parse().ok()?;
            read = true;
        }
        read.then_some(out)
    }
    match (parts(have), parts(want)) {
        (Some(a), Some(b)) => a >= b,
        _ => false,
    }
}

/// One cue-box coordinate as the plugin will read it, or the default when the file doesn't
/// say. A value outside the screen is the default too: it would put the cue where nobody can
/// read it, and the rider would have no way to tell that from the HUD being broken.
fn cue_axis(pairs: &[(String, String)], key: &str, default: f32) -> f32 {
    ini::get(pairs, key)
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= CUE_POS_RANGE.0 && *v <= CUE_POS_RANGE.1)
        .unwrap_or(default)
}

/// `hud.ini` as the plugin will read it. `mxbmrp3` decides the map's default, exactly as the
/// plugin decides it: the switch the rider sees has to be the answer they'll get in the game.
fn hud_of(path: &Path, mxbmrp3: bool) -> Hud {
    let pairs = ini::read_section(&fs::read_to_string(path).unwrap_or_default(), "hud");
    Hud {
        enabled: ini::on(ini::get(&pairs, "enabled"), true),
        parts: HUD_PARTS
            .iter()
            .map(|&(key, label)| {
                // Every part is on unless the file says otherwise — except the map, which the
                // plugin turns off for itself when MXBMRP3 is there to draw one. Reporting
                // that as "on" is what made the switch look broken: the rider turned a map on
                // that was already on, and none appeared.
                let default = key != "map" || !mxbmrp3;
                HudPart { key, label, on: ini::on(ini::get(&pairs, key), default) }
            })
            .collect(),
        file: path.display().to_string(),
        mxbmrp3,
        cue_pos: [
            cue_axis(&pairs, "cue_x", DEFAULT_CUE_POS[0]),
            cue_axis(&pairs, "cue_y", DEFAULT_CUE_POS[1]),
        ],
    }
}

fn voice_of(path: &Path) -> Voice {
    let pairs = ini::read_section(&fs::read_to_string(path).unwrap_or_default(), "voice");
    Voice {
        // The plugin treats a missing file or key as off.
        enabled: ini::on(ini::get(&pairs, "enabled"), false),
        volume: ini::get(&pairs, "volume").and_then(|v| v.parse().ok()).map_or(DEFAULT_VOLUME, |v: u8| v.min(100)),
    }
}

fn hud_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(coach_dir(app)?.join("hud.ini"))
}

fn voice_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(coach_dir(app)?.join("cues").join("voice.ini"))
}

#[tauri::command]
pub fn coach_hud(app: AppHandle) -> Result<Hud, String> {
    let cfg = load_config(&app);
    Ok(hud_of(&hud_path(&app)?, mxbmrp3_installed(&cfg)))
}

/// Turn one HUD part on or off, or the whole HUD with `enabled`.
///
/// Always written out as a `1` or a `0`, never left to the plugin's default — that is what
/// makes the map switch mean something when MXBMRP3 is installed.
#[tauri::command]
pub fn coach_set_hud(app: AppHandle, key: String, on: bool) -> Result<Hud, String> {
    let key = std::iter::once("enabled")
        .chain(HUD_PARTS.iter().map(|(k, _)| *k))
        .find(|k| *k == key)
        .ok_or_else(|| format!("\"{key}\" isn't a HUD part."))?;
    let path = hud_path(&app)?;
    ini::write(&path, "hud", &[(key, if on { "1" } else { "0" }.to_string())])?;
    Ok(hud_of(&path, mxbmrp3_installed(&load_config(&app))))
}

/// Where the live cue sits on screen: the centre of the cue line, as a fraction of the width
/// and the height, with (0,0) at the top left. FrostMod 0.24 reads `cue_x` and `cue_y`.
#[tauri::command]
pub fn coach_set_cue_pos(app: AppHandle, x: f32, y: f32) -> Result<Hud, String> {
    let clamp = |v: f32, default: f32| {
        if v.is_finite() { v.clamp(CUE_POS_RANGE.0, CUE_POS_RANGE.1) } else { default }
    };
    let (x, y) = (clamp(x, DEFAULT_CUE_POS[0]), clamp(y, DEFAULT_CUE_POS[1]));
    let path = hud_path(&app)?;
    ini::write(&path, "hud", &[("cue_x", format!("{x:.3}")), ("cue_y", format!("{y:.3}"))])?;
    Ok(hud_of(&path, mxbmrp3_installed(&load_config(&app))))
}

#[tauri::command]
pub fn coach_voice(app: AppHandle) -> Result<Voice, String> {
    Ok(voice_of(&voice_path(&app)?))
}

/// Whether the recorder speaks its cues, and how loud (0–100).
#[tauri::command]
pub fn coach_set_voice(app: AppHandle, enabled: bool, volume: u8) -> Result<Voice, String> {
    let path = voice_path(&app)?;
    let keys = [("enabled", if enabled { "1" } else { "0" }.to_string()), ("volume", volume.min(100).to_string())];
    ini::write(&path, "voice", &keys)?;
    Ok(voice_of(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_takes_the_plugins_defaults() {
        let hud = hud_of(Path::new("/nowhere/hud.ini"), false);
        assert!(hud.enabled && hud.parts.iter().all(|p| p.on));
        assert_eq!(hud.cue_pos, DEFAULT_CUE_POS);
        let voice = voice_of(Path::new("/nowhere/voice.ini"));
        assert!(!voice.enabled, "the plugin treats no file as off");
        assert_eq!(voice.volume, DEFAULT_VOLUME);
    }

    /// The reported map state has to be the one the rider will actually get, or the switch
    /// lies: with MXBMRP3 installed the plugin draws no map of its own until the file says to.
    #[test]
    fn the_map_switch_says_what_the_plugin_will_do() {
        let map = |h: &Hud| h.parts.iter().find(|p| p.key == "map").unwrap().on;
        let alone = hud_of(Path::new("/nowhere/hud.ini"), false);
        assert!(map(&alone), "nothing else draws a map, so ours does");
        let beside = hud_of(Path::new("/nowhere/hud.ini"), true);
        assert!(!map(&beside), "MXBMRP3 draws one, so ours stays off until asked");
        assert!(beside.mxbmrp3, "and the rider is told why");
        // Every other part is unaffected by the other plugin.
        assert!(beside.parts.iter().filter(|p| p.key != "map").all(|p| p.on));

        let dir = std::env::temp_dir().join(format!("coach-map-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        ini::write(&path, "hud", &[("map", "1".into())]).unwrap();
        assert!(map(&hud_of(&path, true)), "asked for outright, it is on even beside MXBMRP3");
        ini::write(&path, "hud", &[("map", "0".into())]).unwrap();
        assert!(!map(&hud_of(&path, false)), "and off when it says off");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_cue_position_is_read_back_and_a_silly_one_is_ignored() {
        let dir = std::env::temp_dir().join(format!("coach-cuepos-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        ini::write(&path, "hud", &[("cue_x", "0.200".into()), ("cue_y", "0.860".into())]).unwrap();
        assert_eq!(hud_of(&path, false).cue_pos, [0.2, 0.86]);
        // Off the screen, or not a number at all: the plugin's own place, not a cue nobody sees.
        ini::write(&path, "hud", &[("cue_x", "12".into()), ("cue_y", "what".into())]).unwrap();
        assert_eq!(hud_of(&path, false).cue_pos, DEFAULT_CUE_POS);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_recorder_version_is_read_by_number_not_by_text() {
        assert!(at_least("0.23.0", RECORDER_NEEDS));
        assert!(at_least("0.23", RECORDER_NEEDS));
        assert!(at_least("1.0.0", RECORDER_NEEDS));
        assert!(at_least("0.23.0-beta.1", RECORDER_NEEDS), "a beta of it has what it needs");
        assert!(!at_least("0.22.9", RECORDER_NEEDS));
        assert!(!at_least("0.9.0", RECORDER_NEEDS), "0.9 is older than 0.23, though it sorts after it");
        assert!(!at_least("", RECORDER_NEEDS));
        assert!(!at_least("what", RECORDER_NEEDS));
        assert!(!at_least("0.23.0", CUE_POS_NEEDS), "moving the cue box needs the newer one");
        assert!(at_least("0.24.0", CUE_POS_NEEDS));
    }

    #[test]
    fn the_version_comes_from_the_file_the_recorder_writes() {
        let dir = std::env::temp_dir().join(format!("coach-rec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(recorder_version(&dir), None, "nothing until the game has run the recorder");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("recorder.ini"), "[recorder]\nversion=0.23.0\n").unwrap();
        assert_eq!(recorder_version(&dir).as_deref(), Some("0.23.0"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// The user folder can be moved, so there is more than one candidate. Writing to one the
    /// recorder never reads leaves the HUD and the cues off with nothing to show for it.
    #[test]
    fn the_coach_folder_is_the_one_the_recorder_uses() {
        let dir = std::env::temp_dir().join(format!("coach-pick-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (moved, default) = (dir.join("moved"), dir.join("default"));
        let dirs = [moved.join("mxbcoach").join("sessions"), default.join("mxbcoach").join("sessions")];
        assert_eq!(coach_dir_of(&dirs), Some(moved.join("mxbcoach")), "the first when neither is there yet");
        fs::create_dir_all(&dirs[1]).unwrap();
        assert_eq!(coach_dir_of(&dirs), Some(default.join("mxbcoach")), "the one with the sessions in it");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_voice_file_is_written_where_the_plugin_reads_it() {
        let dir = std::env::temp_dir().join(format!("coach-hud-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("cues").join("voice.ini");
        ini::write(&path, "voice", &[("enabled", "1".into()), ("volume", "55".into())]).unwrap();
        let v = voice_of(&path);
        assert!(v.enabled);
        assert_eq!(v.volume, 55);
        let _ = fs::remove_dir_all(&dir);
    }
}
