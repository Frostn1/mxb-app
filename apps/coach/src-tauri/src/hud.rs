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

/// The recorder this Coach asks for: older ones are called out of date and refreshed. 0.38 is
/// the first to pick up Coach's first sheet mid-event, so coaching starts without a restart.
pub(crate) const RECORDER_NEEDS: &str = "0.38";
/// The oldest recorder that draws the HUD parts and speaks the cues at all.
pub(crate) const HUD_NEEDS: &str = "0.23";
/// The version that reads where the cue box goes, the suspension bars, the reference trail and
/// the choice of voice. Coach writes them either way: an older recorder ignores what it doesn't
/// know, so nothing breaks by setting them early — the rider is just told they need 0.24.
pub(crate) const EXTRAS_NEED: &str = "0.24";
/// The first recorder that paints the line to take on the track itself (FrostMod v0.42.0,
/// `ground` in hud.ini). An older one only draws the trail inside the map.
pub(crate) const GROUND_NEEDS: &str = "0.42";
/// The first recorder that draws pace hints over that line (FrostMod v0.44.0, `pace` in hud.ini):
/// chevrons when coming in too fast or too slow for Coach's lap, and MORE SPEED before a jump.
pub(crate) const PACE_NEEDS: &str = "0.44";
/// The first recorder that hints gear changes (FrostMod v0.45.0, `gear` in hud.ini): an arrow and
/// the target gear on the line where Coach's lap shifts, when the rider is in another gear.
pub(crate) const GEAR_NEEDS: &str = "0.45";

/// The HUD parts, in the order the overlay lists them: key, label, whether the plugin draws it
/// when the file doesn't say, and the recorder it needs.
///
/// Two of them are off until asked for. They are extra information over the game's own screen
/// rather than coaching, so a rider who never opens this panel should not find their view
/// covered in bars they didn't ask for.
pub const HUD_PARTS: &[Part] = &[
    Part { key: "cue", label: "Live cue", default_on: true, needs: HUD_NEEDS },
    Part { key: "section", label: "Section and tip", default_on: true, needs: HUD_NEEDS },
    Part { key: "gap", label: "Gap to Coach's lap", default_on: true, needs: HUD_NEEDS },
    Part { key: "stance", label: "Sit / stand", default_on: true, needs: HUD_NEEDS },
    Part { key: "map", label: "Track map with ghost and cues", default_on: true, needs: HUD_NEEDS },
    Part { key: "susp", label: "Suspension bars", default_on: false, needs: EXTRAS_NEED },
    Part { key: "trail", label: "Blue trail of the line to take", default_on: false, needs: EXTRAS_NEED },
    // Follows the trail until the rider sets it on its own: the plugin reads a missing `ground`
    // as whatever `trail` says, and `hud_of` reports it the same way.
    Part { key: "ground", label: "Blue line on the track", default_on: false, needs: GROUND_NEEDS },
    // Drawn over the line on the track, so turning it on turns that on too.
    Part { key: "pace", label: "Pace hints", default_on: false, needs: PACE_NEEDS },
    // Drawn on the line on the track too (and beside the cue box either way).
    Part { key: "gear", label: "Gear hints", default_on: false, needs: GEAR_NEEDS },
    Part { key: "setup", label: "Setup card (when stopped)", default_on: true, needs: HUD_NEEDS },
];

pub struct Part {
    pub key: &'static str,
    pub label: &'static str,
    pub default_on: bool,
    pub needs: &'static str,
}

const DEFAULT_VOLUME: u8 = 80;

/// The voices the recorder has clips for. Anything else in the file is the first of them, which
/// is what the plugin does with a name it doesn't know.
const VOICES: [&str; 2] = ["female", "male"];

/// The plugin's own cue box (FrostMod `src/coachhud.h`, `kCueBox`): centred across the screen,
/// just under MXBMRP3's panels. `cue_x` is the box's **centre**, `cue_y` its **top edge**, so
/// this pair is exactly where the cue has always been.
pub const DEFAULT_CUE_POS: [f32; 2] = [0.5, 0.285];
/// Kept on the screen. The plugin stops a box at the edge anyway; this keeps the file sensible.
const CUE_POS_RANGE: (f32, f32) = (0.05, 0.95);

/// The other plugin that draws a track map. Beside the recorder in the game's plugins folder.
const MXBMRP3: &str = "mxbmrp3.dlo";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudPart {
    pub key: &'static str,
    pub label: &'static str,
    pub on: bool,
    /// The recorder this part needs, so the panel can say when the rider's is older.
    pub needs: &'static str,
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
    /// Where the live cue sits: the box's centre across, its top edge down, as fractions.
    pub cue_pos: [f32; 2],
    /// The recorder that last ran is older than the one the newer settings need.
    pub pre_extras: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub enabled: bool,
    pub volume: u8,
    /// `female` or `male`.
    pub voice: String,
    /// The recorder that last ran is older than the one that can change voice.
    pub pre_extras: bool,
}

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

/// Whether the recorder that ran is older than `want`. Unknown — the game has never run it —
/// says nothing rather than warning about a version nobody has seen.
fn older_than(dir: &Path, want: &str) -> bool {
    recorder_version(dir).is_some_and(|v| !at_least(&v, want))
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
fn hud_of(path: &Path, mxbmrp3: bool, pre_extras: bool) -> Hud {
    let pairs = ini::read_section(&fs::read_to_string(path).unwrap_or_default(), "hud");
    Hud {
        enabled: ini::on(ini::get(&pairs, "enabled"), true),
        parts: HUD_PARTS
            .iter()
            .map(|p| {
                // Each part's own default, except the map, which the plugin turns off for
                // itself when MXBMRP3 is there to draw one. Reporting that as "on" is what
                // made the switch look broken: the rider turned on a map that was already on,
                // and none appeared.
                let default = match p.key {
                    "map" => !mxbmrp3,
                    // The line on the track follows the trail when the file doesn't name it,
                    // as the plugin does.
                    "ground" => ini::on(ini::get(&pairs, "trail"), false),
                    _ => p.default_on,
                };
                HudPart { key: p.key, label: p.label, on: ini::on(ini::get(&pairs, p.key), default), needs: p.needs }
            })
            .collect(),
        file: path.display().to_string(),
        mxbmrp3,
        cue_pos: [
            cue_axis(&pairs, "cue_x", DEFAULT_CUE_POS[0]),
            cue_axis(&pairs, "cue_y", DEFAULT_CUE_POS[1]),
        ],
        pre_extras,
    }
}

fn voice_of(path: &Path, pre_extras: bool) -> Voice {
    let pairs = ini::read_section(&fs::read_to_string(path).unwrap_or_default(), "voice");
    let voice = ini::get(&pairs, "voice").map(str::trim).unwrap_or_default().to_ascii_lowercase();
    Voice {
        // The plugin treats a missing file or key as off.
        enabled: ini::on(ini::get(&pairs, "enabled"), false),
        volume: ini::get(&pairs, "volume").and_then(|v| v.parse().ok()).map_or(DEFAULT_VOLUME, |v: u8| v.min(100)),
        // A name the recorder has no clips for is the first voice, as the plugin reads it.
        voice: if VOICES.contains(&voice.as_str()) { voice } else { VOICES[0].to_string() },
        pre_extras,
    }
}

fn hud_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(coach_dir(app)?.join("hud.ini"))
}

fn voice_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(coach_dir(app)?.join("cues").join("voice.ini"))
}

/// The folder, whether MXBMRP3 is beside the recorder, and whether the recorder is too old for
/// the newer settings — everything both readers need.
fn where_and_what(app: &AppHandle) -> Result<(PathBuf, bool, bool), String> {
    let cfg = load_config(app);
    let dir = coach_dir_of(&session_dirs(&cfg)).ok_or("The game's user folder wasn't found.")?;
    let pre = older_than(&dir, EXTRAS_NEED);
    Ok((dir, mxbmrp3_installed(&cfg), pre))
}

#[tauri::command]
pub fn coach_hud(app: AppHandle) -> Result<Hud, String> {
    let (dir, mxbmrp3, pre) = where_and_what(&app)?;
    Ok(hud_of(&dir.join("hud.ini"), mxbmrp3, pre))
}

/// Turn one HUD part on or off, or the whole HUD with `enabled`.
///
/// Always written out as a `1` or a `0`, never left to the plugin's default — that is what
/// makes the map switch mean something when MXBMRP3 is installed.
#[tauri::command]
pub fn coach_set_hud(app: AppHandle, key: String, on: bool) -> Result<Hud, String> {
    let key = std::iter::once("enabled")
        .chain(HUD_PARTS.iter().map(|p| p.key))
        .find(|k| *k == key)
        .ok_or_else(|| format!("\"{key}\" isn't a HUD part."))?;
    let path = hud_path(&app)?;
    ini::write(&path, "hud", &keys_for(key, on))?;
    let (dir, mxbmrp3, pre) = where_and_what(&app)?;
    Ok(hud_of(&dir.join("hud.ini"), mxbmrp3, pre))
}

/// What switching `key` writes: the key itself, and any part it can't show without.
fn keys_for(key: &'static str, on: bool) -> Vec<(&'static str, String)> {
    let mut keys = vec![(key, if on { "1" } else { "0" }.to_string())];
    // The recorder draws the trail inside the map, so a trail with the map off is a switch that
    // can never do anything — and the map is off by default wherever MXBMRP3 is installed.
    // Turning the trail on turns the map on with it rather than leaving the rider to find out.
    if key == "trail" && on {
        keys.push(("map", "1".to_string()));
    }
    // Pace hints are drawn over the line on the track: on their own they would show nothing.
    if (key == "pace" || key == "gear") && on {
        keys.push(("ground", "1".to_string()));
    }
    keys
}

/// Where the live cue sits on screen: `cue_x` is the centre of the cue box across the screen,
/// `cue_y` its top edge down it, both fractions with (0,0) at the top left. The section line
/// follows the box. FrostMod 0.24 reads them.
#[tauri::command]
pub fn coach_set_cue_pos(app: AppHandle, x: f32, y: f32) -> Result<Hud, String> {
    let clamp = |v: f32, default: f32| {
        if v.is_finite() { v.clamp(CUE_POS_RANGE.0, CUE_POS_RANGE.1) } else { default }
    };
    let (x, y) = (clamp(x, DEFAULT_CUE_POS[0]), clamp(y, DEFAULT_CUE_POS[1]));
    let path = hud_path(&app)?;
    ini::write(&path, "hud", &[("cue_x", format!("{x:.3}")), ("cue_y", format!("{y:.3}"))])?;
    let (dir, mxbmrp3, pre) = where_and_what(&app)?;
    Ok(hud_of(&dir.join("hud.ini"), mxbmrp3, pre))
}

#[tauri::command]
pub fn coach_voice(app: AppHandle) -> Result<Voice, String> {
    let (dir, _, pre) = where_and_what(&app)?;
    Ok(voice_of(&dir.join("cues").join("voice.ini"), pre))
}

/// Whether the recorder speaks its cues, how loud (0–100), and in which voice.
#[tauri::command]
pub fn coach_set_voice(app: AppHandle, enabled: bool, volume: u8, voice: Option<String>) -> Result<Voice, String> {
    let path = voice_path(&app)?;
    let who = voice.unwrap_or_default().trim().to_ascii_lowercase();
    let who = if VOICES.contains(&who.as_str()) { who } else { VOICES[0].to_string() };
    let keys = [
        ("enabled", if enabled { "1" } else { "0" }.to_string()),
        ("volume", volume.min(100).to_string()),
        ("voice", who),
    ];
    ini::write(&path, "voice", &keys)?;
    let (dir, _, pre) = where_and_what(&app)?;
    Ok(voice_of(&dir.join("cues").join("voice.ini"), pre))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part<'a>(h: &'a Hud, key: &str) -> &'a HudPart {
        h.parts.iter().find(|p| p.key == key).unwrap()
    }

    #[test]
    fn a_missing_file_takes_the_plugins_defaults() {
        let hud = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(hud.enabled);
        assert_eq!(hud.cue_pos, DEFAULT_CUE_POS);
        let voice = voice_of(Path::new("/nowhere/voice.ini"), false);
        assert!(!voice.enabled, "the plugin treats no file as off");
        assert_eq!(voice.volume, DEFAULT_VOLUME);
        assert_eq!(voice.voice, "female", "the recorder's own default voice");
    }

    /// The two newest parts are extra information, not coaching, so they wait to be asked for.
    #[test]
    fn the_suspension_bars_and_the_trail_are_off_until_asked_for() {
        let hud = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(!part(&hud, "susp").on && !part(&hud, "trail").on);
        assert!(part(&hud, "cue").on && part(&hud, "gap").on && part(&hud, "setup").on);
        // And they say which recorder they need, so the panel can warn.
        assert_eq!(part(&hud, "susp").needs, EXTRAS_NEED);
        assert_eq!(part(&hud, "trail").needs, EXTRAS_NEED);
        assert_eq!(part(&hud, "cue").needs, HUD_NEEDS);
    }

    /// The line on the track is its own switch, but until the rider sets it, it follows the
    /// trail - which is what the plugin does with a hud.ini that has no `ground` key.
    #[test]
    fn the_line_on_the_track_follows_the_trail_until_set() {
        let none = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(!part(&none, "ground").on, "off with the trail off");
        assert_eq!(part(&none, "ground").needs, GROUND_NEEDS);
        assert_eq!(part(&none, "ground").label, "Blue line on the track");

        let dir = std::env::temp_dir().join(format!("coach-ground-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        ini::write(&path, "hud", &[("trail", "1".into())]).unwrap();
        assert!(part(&hud_of(&path, false, false), "ground").on, "the trail on brings it with it");
        ini::write(&path, "hud", &[("trail", "1".into()), ("ground", "0".into())]).unwrap();
        assert!(!part(&hud_of(&path, false, false), "ground").on, "unless it is switched off itself");
        ini::write(&path, "hud", &[("trail", "0".into()), ("ground", "1".into())]).unwrap();
        assert!(part(&hud_of(&path, false, false), "ground").on, "and it can be on without the trail");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Pace hints wait to be asked for, need the recorder that draws them, and bring the line on
    /// the track with them, since they are drawn over it.
    #[test]
    fn pace_hints_are_off_until_asked_for_and_bring_the_line() {
        let none = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(!part(&none, "pace").on, "off by default, as the plugin reads it");
        assert_eq!(part(&none, "pace").needs, PACE_NEEDS);
        assert_eq!(part(&none, "pace").label, "Pace hints");
        assert_eq!(keys_for("pace", true), vec![("pace", "1".to_string()), ("ground", "1".to_string())]);
        assert_eq!(keys_for("pace", false), vec![("pace", "0".to_string())], "off leaves the line alone");

        let dir = std::env::temp_dir().join(format!("coach-pace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        ini::write(&path, "hud", &keys_for("pace", true)).unwrap();
        let hud = hud_of(&path, false, false);
        assert!(part(&hud, "pace").on && part(&hud, "ground").on, "both on");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Gear hints wait to be asked for, need the recorder that draws them, and bring the line on
    /// the track with them, like the pace hints.
    #[test]
    fn gear_hints_are_off_until_asked_for_and_bring_the_line() {
        let none = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(!part(&none, "gear").on, "off by default, as the plugin reads it");
        assert_eq!(part(&none, "gear").needs, GEAR_NEEDS);
        assert_eq!(part(&none, "gear").label, "Gear hints");
        assert_eq!(keys_for("gear", true), vec![("gear", "1".to_string()), ("ground", "1".to_string())]);
        assert_eq!(keys_for("gear", false), vec![("gear", "0".to_string())], "off leaves the line alone");
    }

    /// The reported map state has to be the one the rider will actually get, or the switch
    /// lies: with MXBMRP3 installed the plugin draws no map of its own until the file says to.
    #[test]
    fn the_map_switch_says_what_the_plugin_will_do() {
        let alone = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(part(&alone, "map").on, "nothing else draws a map, so ours does");
        let beside = hud_of(Path::new("/nowhere/hud.ini"), true, false);
        assert!(!part(&beside, "map").on, "MXBMRP3 draws one, so ours stays off until asked");
        assert!(beside.mxbmrp3, "and the rider is told why");
        // Every other part keeps its own default whatever the other plugin does.
        assert!(part(&beside, "cue").on && !part(&beside, "susp").on);

        let dir = std::env::temp_dir().join(format!("coach-map-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        ini::write(&path, "hud", &[("map", "1".into())]).unwrap();
        assert!(part(&hud_of(&path, true, false), "map").on, "asked for outright, it is on even beside MXBMRP3");
        ini::write(&path, "hud", &[("map", "0".into())]).unwrap();
        assert!(!part(&hud_of(&path, false, false), "map").on, "and off when it says off");
        let _ = fs::remove_dir_all(&dir);
    }

    /// `cue_x` is the box's centre and `cue_y` its top edge, which is what the plugin reads.
    #[test]
    fn the_cue_position_is_read_back_and_a_silly_one_is_ignored() {
        let dir = std::env::temp_dir().join(format!("coach-cuepos-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        ini::write(&path, "hud", &[("cue_x", "0.250".into()), ("cue_y", "0.820".into())]).unwrap();
        assert_eq!(hud_of(&path, false, false).cue_pos, [0.25, 0.82]);
        // Off the screen, or not a number at all: the plugin's own place, not a cue nobody sees.
        ini::write(&path, "hud", &[("cue_x", "12".into()), ("cue_y", "what".into())]).unwrap();
        assert_eq!(hud_of(&path, false, false).cue_pos, DEFAULT_CUE_POS);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_voice_is_one_the_recorder_has_clips_for() {
        let dir = std::env::temp_dir().join(format!("coach-voice-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("voice.ini");
        ini::write(&path, "voice", &[("voice", "male".into())]).unwrap();
        assert_eq!(voice_of(&path, false).voice, "male");
        ini::write(&path, "voice", &[("voice", "MALE".into())]).unwrap();
        assert_eq!(voice_of(&path, false).voice, "male", "however it is spelled");
        ini::write(&path, "voice", &[("voice", "robot".into())]).unwrap();
        assert_eq!(voice_of(&path, false).voice, "female", "a voice with no clips is the first one");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_recorder_version_is_read_by_number_not_by_text() {
        assert!(at_least("0.23.0", HUD_NEEDS));
        assert!(at_least("0.23", HUD_NEEDS));
        assert!(at_least("1.0.0", HUD_NEEDS));
        assert!(at_least("0.23.0-beta.1", HUD_NEEDS), "a beta of it has what it needs");
        assert!(!at_least("0.22.9", HUD_NEEDS));
        assert!(!at_least("0.9.0", HUD_NEEDS), "0.9 is older than 0.23, though it sorts after it");
        assert!(!at_least("", HUD_NEEDS));
        // The recorder this Coach asks for: 0.37 draws the HUD but can't start coaching without
        // a restart, so it's out of date.
        assert!(at_least("0.38.0", RECORDER_NEEDS));
        assert!(!at_least("0.37.0", RECORDER_NEEDS));
        assert!(at_least("0.37.0", HUD_NEEDS), "and still draws everything it did");
        assert!(!at_least("what", RECORDER_NEEDS));
        assert!(!at_least("0.23.0", EXTRAS_NEED), "the newer settings need 0.24");
        assert!(at_least("0.24.0", EXTRAS_NEED));
    }

    /// A recorder the game has never run says nothing: warning about a version nobody has
    /// seen would put a notice on a fresh install that is nothing to do with the rider.
    #[test]
    fn an_unrun_recorder_is_not_called_old() {
        let dir = std::env::temp_dir().join(format!("coach-rec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(recorder_version(&dir), None, "nothing until the game has run the recorder");
        assert!(!older_than(&dir, EXTRAS_NEED), "unknown is not old");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("recorder.ini"), "[recorder]\nversion=0.23.0\n").unwrap();
        assert_eq!(recorder_version(&dir).as_deref(), Some("0.23.0"));
        assert!(older_than(&dir, EXTRAS_NEED), "0.23 is older than the newer settings need");
        assert!(!older_than(&dir, HUD_NEEDS));
        assert!(older_than(&dir, RECORDER_NEEDS), "and out of date for this Coach");
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
        ini::write(&path, "voice", &[("enabled", "1".into()), ("volume", "55".into()), ("voice", "male".into())]).unwrap();
        let v = voice_of(&path, false);
        assert!(v.enabled);
        assert_eq!((v.volume, v.voice.as_str()), (55, "male"));
        let _ = fs::remove_dir_all(&dir);
    }
}
