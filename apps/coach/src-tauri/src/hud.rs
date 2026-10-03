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
/// The first recorder that calls the jumps on that line (FrostMod v0.44.1, `jumps` in hud.ini):
/// a bar at the lip, a mark where Coach's lap landed and SINGLE / DOUBLE / TRIPLE over the lip.
pub(crate) const JUMPS_NEED: &str = "0.44.1";
/// The first recorder that hints gear changes (FrostMod v0.45.0, `gear` in hud.ini): an arrow and
/// the target gear on the line where Coach's lap shifts, when the rider is in another gear.
pub(crate) const GEAR_NEEDS: &str = "0.45";
/// The version that reads the line's look: its width, opacity and colours, and the text on it.
pub(crate) const LOOK_NEEDS: &str = "0.46.1";
/// The first recorder that fades the line out near the rider (`line_fade`), steadies the jump marks
/// and draws the pace and gear hints without being asked.
pub(crate) const FADE_NEEDS: &str = "0.48";
/// The first recorder that gives each text item (jump call, MORE SPEED, gear badge, cue box) its own
/// style, size and place (FrostMod 0.49.0, `<item>_style` and friends in hud.ini).
pub(crate) const ITEMS_NEED: &str = "0.49";

/// The HUD parts, in the order the overlay lists them: key, label, whether the plugin draws it
/// when the file doesn't say, and the recorder it needs.
///
/// Two of them (the suspension bars and the trail) are off until asked for. They are extra
/// information over the game's own screen rather than coaching, so a rider who never opens this
/// panel should not find their view covered in bars they didn't ask for. The pace and gear hints
/// are coaching and show with the line on the track; they were off until asked for, and a rider
/// who never opened this panel never saw either.
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
    // Drawn over the line on the track, so it shows only with that on; on by default there.
    Part { key: "pace", label: "Pace hints", default_on: true, needs: PACE_NEEDS },
    // Drawn on the line on the track, so it shows only with that on; on by default there, since
    // a rider who turned the line on asked to be shown the track.
    Part { key: "jumps", label: "Jump calls on the line", default_on: true, needs: JUMPS_NEED },
    // Drawn on the line on the track too (and beside the cue box either way).
    Part { key: "gear", label: "Gear hints", default_on: true, needs: GEAR_NEEDS },
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

/// The look of the line on the track and of the text on it, as `[hud]` keys in `hud.ini`. FrostMod
/// 0.46.1 reads them; a key that isn't there is the look the line has always had, which is what
/// [`LineLook::default`] is.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineLook {
    /// Times the line's own width, 0.25 to 3.
    pub width: f32,
    /// 0.1 to 1.
    pub opacity: f32,
    pub colours: LineColours,
    /// The words and the gear sign on the line: jump calls, MORE SPEED, gear hints.
    pub text: bool,
    /// Times the text's own size, 0.5 to 2.
    pub text_size: f32,
    /// `block`, `bold` or `italic`: what the plugin's block font can draw.
    pub text_style: String,
    /// The line fades out toward the rider so the ruts it runs through stay visible: clear at the
    /// bike, solid this many metres ahead. 0 is off, up to 30. FrostMod 0.48 reads it.
    pub near_fade: f32,
}

/// `#rrggbb` each: the line's gradient from gas to heavy braking, and the pace hints.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct LineColours {
    pub gas: String,
    pub coast: String,
    pub light: String,
    pub heavy: String,
    pub fast: String,
    pub slow: String,
}

/// The plugin's own colours (FrostMod `coachline::ToneColour`, `coachpace::kFastColour` and
/// `kSlowColour`) to the nearest #rrggbb.
impl Default for LineColours {
    fn default() -> Self {
        Self {
            gas: "#26d933".into(),
            coast: "#f7f7f7".into(),
            light: "#ffdb0d".into(),
            heavy: "#f21f14".into(),
            fast: "#ff40cc".into(),
            slow: "#33d9ff".into(),
        }
    }
}

impl Default for LineLook {
    fn default() -> Self {
        Self { width: 1.0, opacity: 0.7, colours: LineColours::default(), text: true, text_size: 1.0, text_style: "block".into(), near_fade: 8.0 }
    }
}

const TEXT_STYLES: [&str; 3] = ["block", "bold", "italic"];
const WIDTH_RANGE: (f32, f32) = (0.25, 3.0);
const OPACITY_RANGE: (f32, f32) = (0.1, 1.0);
const TEXT_SIZE_RANGE: (f32, f32) = (0.5, 2.0);
const NEAR_FADE_RANGE: (f32, f32) = (0.0, 30.0);

/// `#rrggbb`, lower case, or None for anything else.
fn hex_colour(v: &str) -> Option<String> {
    let h = v.trim().trim_start_matches('#');
    (h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{}", h.to_ascii_lowercase()))
}

/// The line's look, with every value clamped and checked the way the plugin reads it.
fn tidy_look(l: LineLook) -> LineLook {
    let d = LineLook::default();
    let num = |v: f32, (lo, hi): (f32, f32), def: f32| if v.is_finite() { v.clamp(lo, hi) } else { def };
    let col = |v: &str, def: &str| hex_colour(v).unwrap_or_else(|| def.to_string());
    let c = &l.colours;
    let dc = &d.colours;
    let style = l.text_style.trim().to_ascii_lowercase();
    LineLook {
        width: num(l.width, WIDTH_RANGE, d.width),
        opacity: num(l.opacity, OPACITY_RANGE, d.opacity),
        colours: LineColours {
            gas: col(&c.gas, &dc.gas),
            coast: col(&c.coast, &dc.coast),
            light: col(&c.light, &dc.light),
            heavy: col(&c.heavy, &dc.heavy),
            fast: col(&c.fast, &dc.fast),
            slow: col(&c.slow, &dc.slow),
        },
        text: l.text,
        text_size: num(l.text_size, TEXT_SIZE_RANGE, d.text_size),
        text_style: if TEXT_STYLES.contains(&style.as_str()) { style } else { d.text_style },
        near_fade: num(l.near_fade, NEAR_FADE_RANGE, d.near_fade),
    }
}

/// The look as the plugin will read it from `hud.ini`'s `[hud]` pairs.
fn look_of(pairs: &[(String, String)]) -> LineLook {
    let d = LineLook::default();
    let num = |key: &str, def: f32| ini::get(pairs, key).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(def);
    let col = |key: &str, def: &str| ini::get(pairs, key).and_then(hex_colour).unwrap_or_else(|| def.to_string());
    let dc = &d.colours;
    tidy_look(LineLook {
        width: num("line_width", d.width),
        opacity: num("line_opacity", d.opacity),
        colours: LineColours {
            gas: col("col_gas", &dc.gas),
            coast: col("col_coast", &dc.coast),
            light: col("col_light", &dc.light),
            heavy: col("col_heavy", &dc.heavy),
            fast: col("col_fast", &dc.fast),
            slow: col("col_slow", &dc.slow),
        },
        text: ini::on(ini::get(pairs, "line_text"), true),
        text_size: num("text_size", d.text_size),
        text_style: ini::get(pairs, "text_style").unwrap_or_default().to_string(),
        near_fade: num("line_fade", d.near_fade),
    })
}

/// The `[hud]` keys for a look, all of them, so what the rider sees in Coach is what is in the file.
fn look_keys(l: &LineLook) -> Vec<(&'static str, String)> {
    let c = &l.colours;
    vec![
        ("line_width", format!("{:.2}", l.width)),
        ("line_opacity", format!("{:.2}", l.opacity)),
        ("col_gas", c.gas.clone()),
        ("col_coast", c.coast.clone()),
        ("col_light", c.light.clone()),
        ("col_heavy", c.heavy.clone()),
        ("col_fast", c.fast.clone()),
        ("col_slow", c.slow.clone()),
        ("line_text", if l.text { "1" } else { "0" }.to_string()),
        ("text_size", format!("{:.2}", l.text_size)),
        ("text_style", l.text_style.clone()),
        ("line_fade", format!("{:.1}", l.near_fade)),
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineLookState {
    pub look: LineLook,
    /// What "reset" goes back to: the line as the plugin draws it with no keys at all.
    pub defaults: LineLook,
    /// The recorder that last ran is older than the one that reads the look.
    pub pre_look: bool,
    /// The recorder that last ran is older than the one that fades the line near the rider.
    pub pre_fade: bool,
}

fn look_state(dir: &Path) -> LineLookState {
    let pairs = ini::read_section(&fs::read_to_string(dir.join("hud.ini")).unwrap_or_default(), "hud");
    LineLookState {
        look: look_of(&pairs),
        defaults: LineLook::default(),
        pre_look: older_than(dir, LOOK_NEEDS),
        pre_fade: older_than(dir, FADE_NEEDS),
    }
}

#[tauri::command]
pub fn coach_line_look(app: AppHandle) -> Result<LineLookState, String> {
    Ok(look_state(&coach_dir(&app)?))
}

/// Write the whole look. The recorder re-reads `hud.ini` about once a second, so it shows live.
#[tauri::command]
pub fn coach_set_line_look(app: AppHandle, look: LineLook) -> Result<LineLookState, String> {
    let dir = coach_dir(&app)?;
    ini::write(&dir.join("hud.ini"), "hud", &look_keys(&tidy_look(look)))?;
    Ok(look_state(&dir))
}

// ---------------------------------------------------------------------------------------
// Text items: a style, size and place for each word the plugin draws (FrostMod 0.49.0)

/// One text item the plugin draws: the jump call, MORE SPEED, the gear badge and the cue box.
/// `[hud]` keys are `<id>_style`, `<id>_size`, `<id>_x`, `<id>_y`, `<id>_anchor` and, for the
/// jump call, `jump_place`; the on/off key is `on_key`. The defaults are where the plugin draws
/// each one with none of them set, so a file without them is the look it has always had.
struct ItemDef {
    id: &'static str,
    label: &'static str,
    on_key: &'static str,
    /// Where it sits with no position written, as screen fractions: x by `anchor`, y its top.
    x: f32,
    y: f32,
    anchor: &'static str,
    /// Its size follows the line's text size (`text_size`) until it has one of its own.
    follows_text_size: bool,
}

const TEXT_ITEMS: [ItemDef; 4] = [
    // On the line at the lip by default; the position is for "fixed on screen".
    ItemDef { id: "jump", label: "Jump call", on_key: "jump_text", x: 0.5, y: 0.45, anchor: "center", follows_text_size: true },
    // Under the gap row (FrostMod `kRowDefaultX`, `kRowDefaultY` + `kRowH` + 0.006).
    ItemDef { id: "pace", label: "MORE SPEED", on_key: "pace_text", x: 0.5, y: 0.396, anchor: "center", follows_text_size: true },
    // Beside the cue box (`kCueBox`.x1 + 0.008), left edge.
    ItemDef { id: "gear", label: "Gear badge", on_key: "gear_badge", x: 0.658, y: 0.285, anchor: "left", follows_text_size: false },
    // The cue box: `cue_x` / `cue_y` are always its place, the middle and the top by default.
    ItemDef { id: "cue", label: "Live cue", on_key: "cue", x: 0.5, y: 0.285, anchor: "center", follows_text_size: false },
];

/// What the plugin can draw an item in: its own look (the game's font on the HUD, the line's text
/// style on the line) or the block font, as it is, fatter or leaning.
const ITEM_STYLES: [&str; 4] = ["default", "block", "bold", "italic"];
const ITEM_ANCHORS: [&str; 3] = ["left", "center", "right"];
const ITEM_SIZE_RANGE: (f32, f32) = (0.5, 3.0);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextItemState {
    pub id: &'static str,
    pub label: &'static str,
    pub on: bool,
    pub style: String,
    /// Times the item's own size; None until the rider sets one, which is today's size.
    pub size: Option<f32>,
    /// Screen fractions, 0..1: `x` by `anchor`, `y` the top of the text. The default place until
    /// `placed`.
    pub x: f32,
    pub y: f32,
    /// x and y are written: the item is fixed there rather than where it always sat.
    pub placed: bool,
    pub anchor: String,
    pub default_x: f32,
    pub default_y: f32,
    pub default_anchor: &'static str,
    /// The jump call only: `line` (over the lip) or `screen` (fixed at x, y).
    pub place: Option<String>,
    /// Its size is the line's text size until it has its own.
    pub follows_text_size: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextItems {
    pub items: Vec<TextItemState>,
    /// The line's own text size (`text_size`), what jump and MORE SPEED are drawn at with no size of their own.
    pub text_size: f32,
    /// The recorder that last ran is older than FrostMod 0.49, which reads the items.
    pub pre_items: bool,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextItemIn {
    pub id: String,
    pub on: bool,
    pub style: String,
    pub size: Option<f32>,
    pub placed: bool,
    pub x: f32,
    pub y: f32,
    pub anchor: String,
    pub place: Option<String>,
    /// Put every key of this item back to unset.
    #[serde(default)]
    pub reset: bool,
}

fn fraction(pairs: &[(String, String)], key: &str) -> Option<f32> {
    ini::get(pairs, key).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

/// The items as the plugin will read them from `hud.ini`'s `[hud]` pairs.
fn items_of(pairs: &[(String, String)]) -> Vec<TextItemState> {
    TEXT_ITEMS
        .iter()
        .map(|d| {
            let key = |s: &str| format!("{}_{s}", d.id);
            let style = ini::get(pairs, &key("style")).map(|v| v.trim().to_ascii_lowercase()).unwrap_or_default();
            let anchor = ini::get(pairs, &key("anchor")).map(|v| v.trim().to_ascii_lowercase()).unwrap_or_default();
            let (x, y) = (fraction(pairs, &key("x")), fraction(pairs, &key("y")));
            TextItemState {
                id: d.id,
                label: d.label,
                on: ini::on(ini::get(pairs, d.on_key), true),
                style: if ITEM_STYLES.contains(&style.as_str()) { style } else { "default".into() },
                size: ini::get(pairs, &key("size"))
                    .and_then(|v| v.trim().parse::<f32>().ok())
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .map(|v| v.clamp(ITEM_SIZE_RANGE.0, ITEM_SIZE_RANGE.1)),
                x: x.zip(y).map_or(d.x, |(x, _)| x),
                y: x.zip(y).map_or(d.y, |(_, y)| y),
                placed: x.is_some() && y.is_some(),
                anchor: if ITEM_ANCHORS.contains(&anchor.as_str()) { anchor } else { d.anchor.into() },
                default_x: d.x,
                default_y: d.y,
                default_anchor: d.anchor,
                place: (d.id == "jump")
                    .then(|| if ini::get(pairs, "jump_place").map(str::trim) == Some("screen") { "screen" } else { "line" }.into()),
                follows_text_size: d.follows_text_size,
            }
        })
        .collect()
}

fn items_state(dir: &Path) -> TextItems {
    let pairs = ini::read_section(&fs::read_to_string(dir.join("hud.ini")).unwrap_or_default(), "hud");
    TextItems {
        items: items_of(&pairs),
        text_size: look_of(&pairs).text_size,
        pre_items: older_than(dir, ITEMS_NEED),
    }
}

/// The keys one item is written as and the keys it is taken out as, from what the rider set. A
/// value the plugin would call the default is left out, so "default" stays unset in the file.
fn item_edit(i: &TextItemIn) -> Result<(Vec<(String, String)>, Vec<String>), String> {
    let d = TEXT_ITEMS.iter().find(|d| d.id == i.id).ok_or_else(|| format!("\"{}\" isn't a text item.", i.id))?;
    let key = |s: &str| format!("{}_{s}", d.id);
    let mut all = vec![key("style"), key("size"), key("x"), key("y"), key("anchor")];
    if d.id == "jump" {
        all.push("jump_place".to_string());
    }
    if i.reset {
        // The on key too: switched back on, which is what the plugin does with none.
        let mut remove: Vec<String> = all;
        remove.push(d.on_key.to_string());
        return Ok((Vec::new(), remove));
    }
    let mut set = vec![(d.on_key.to_string(), if i.on { "1" } else { "0" }.to_string())];
    let mut remove = Vec::new();
    let style = i.style.trim().to_ascii_lowercase();
    match style.as_str() {
        "block" | "bold" | "italic" => set.push((key("style"), style)),
        _ => remove.push(key("style")),
    }
    match i.size.filter(|v| v.is_finite()) {
        Some(v) => set.push((key("size"), format!("{:.2}", v.clamp(ITEM_SIZE_RANGE.0, ITEM_SIZE_RANGE.1)))),
        None => remove.push(key("size")),
    }
    if i.placed && i.x.is_finite() && i.y.is_finite() {
        set.push((key("x"), format!("{:.3}", i.x.clamp(0.0, 1.0))));
        set.push((key("y"), format!("{:.3}", i.y.clamp(0.0, 1.0))));
    } else {
        remove.push(key("x"));
        remove.push(key("y"));
    }
    let anchor = i.anchor.trim().to_ascii_lowercase();
    if ITEM_ANCHORS.contains(&anchor.as_str()) && anchor != d.anchor {
        set.push((key("anchor"), anchor));
    } else {
        remove.push(key("anchor"));
    }
    if d.id == "jump" {
        if i.place.as_deref() == Some("screen") {
            set.push(("jump_place".into(), "screen".into()));
        } else {
            remove.push("jump_place".into());
        }
    }
    Ok((set, remove))
}

#[tauri::command]
pub fn coach_text_items(app: AppHandle) -> Result<TextItems, String> {
    Ok(items_state(&coach_dir(&app)?))
}

/// Write one item. The recorder re-reads `hud.ini` about once a second, so it shows live.
#[tauri::command]
pub fn coach_set_text_item(app: AppHandle, item: TextItemIn) -> Result<TextItems, String> {
    let dir = coach_dir(&app)?;
    let (set, remove) = item_edit(&item)?;
    let set: Vec<(&str, String)> = set.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let remove: Vec<&str> = remove.iter().map(String::as_str).collect();
    ini::write_edit(&dir.join("hud.ini"), "hud", &set, &remove)?;
    Ok(items_state(&dir))
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

    fn pairs(text: &str) -> Vec<(String, String)> {
        ini::read_section(text, "hud")
    }

    /// No keys: every item is where and as the plugin draws it with none, and nothing is "placed".
    #[test]
    fn no_text_item_keys_is_todays_look() {
        let items = items_of(&[]);
        assert_eq!(items.len(), 4);
        for i in &items {
            assert!(i.on && !i.placed && i.size.is_none() && i.style == "default", "{}", i.id);
            assert_eq!((i.x, i.y, i.anchor.as_str()), (i.default_x, i.default_y, i.default_anchor));
        }
        assert_eq!(items[0].place.as_deref(), Some("line"), "the jump call is on the line");
        assert!(items[1..].iter().all(|i| i.place.is_none()));
    }

    #[test]
    fn text_items_read_what_the_plugin_reads() {
        let p = pairs(&format!(
            "[hud]{n}pace_x=0.4{n}pace_y=0.8{n}pace_style=Bold{n}pace_size=9{n}pace_anchor=left{n}pace_text=0{n}jump_place=screen{n}gear_x=0.5{n}cue_style=gothic{n}",
            n = '\n'
        ));
        let items = items_of(&p);
        let pace = &items[1];
        assert!(!pace.on && pace.placed && (pace.x, pace.y) == (0.4, 0.8));
        assert_eq!((pace.style.as_str(), pace.anchor.as_str(), pace.size), ("bold", "left", Some(3.0)));
        assert_eq!(items[0].place.as_deref(), Some("screen"));
        assert!(!items[2].placed, "half a position is no position, as in the plugin");
        assert_eq!(items[3].style, "default", "a style the font can't draw is the default");
    }

    #[test]
    fn editing_an_item_writes_only_what_differs_from_the_default() {
        let item = |f: &dyn Fn(&mut TextItemIn)| {
            let mut i = TextItemIn {
                id: "pace".into(), on: true, style: "default".into(), size: None, placed: false, x: 0.5, y: 0.396,
                anchor: "center".into(), place: None, reset: false,
            };
            f(&mut i);
            item_edit(&i).unwrap()
        };
        let (set, remove) = item(&|_| {});
        assert_eq!(set, vec![("pace_text".to_string(), "1".to_string())]);
        assert!(remove.contains(&"pace_x".to_string()) && remove.contains(&"pace_style".to_string()));
        let (set, _) = item(&|i| {
            i.placed = true;
            i.x = 1.7;
            i.y = 0.8;
            i.style = "Italic".into();
            i.size = Some(1.5);
            i.anchor = "right".into();
        });
        let get = |k: &str| set.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!((get("pace_x"), get("pace_y"), get("pace_style")), (Some("1.000"), Some("0.800"), Some("italic")));
        assert_eq!((get("pace_size"), get("pace_anchor")), (Some("1.50"), Some("right")));
        let (set, remove) = item(&|i| i.reset = true);
        assert!(set.is_empty() && remove.contains(&"pace_text".to_string()) && remove.contains(&"pace_size".to_string()));
        assert!(!remove.contains(&"jump_place".to_string()), "resetting one item leaves the jump call's place alone");
        assert!(item_edit(&TextItemIn { id: "gothic".into(), on: true, style: String::new(), size: None, placed: false, x: 0.0, y: 0.0, anchor: String::new(), place: None, reset: false }).is_err());
    }

    /// Setting and resetting through the file leaves Coach's other keys and the rider's own alone.
    #[test]
    fn an_item_round_trips_through_the_file_and_resets_to_nothing() {
        let file = "[hud]\nenabled=1\nline_width=1.50\n";
        let i = TextItemIn {
            id: "jump".into(), on: true, style: "bold".into(), size: Some(2.0), placed: true, x: 0.3, y: 0.6,
            anchor: "left".into(), place: Some("screen".into()), reset: false,
        };
        let (set, remove) = item_edit(&i).unwrap();
        let removes: Vec<&str> = remove.iter().map(String::as_str).collect();
        let sets: Vec<(&str, String)> = set.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let written = ini::set_keys(&ini::remove_keys(file, "hud", &removes), "hud", &sets);
        let back = &items_of(&pairs(&written))[0];
        assert_eq!((back.style.as_str(), back.size, back.placed, back.anchor.as_str()), ("bold", Some(2.0), true, "left"));
        assert_eq!(back.place.as_deref(), Some("screen"));
        let (set, remove) = item_edit(&TextItemIn { reset: true, ..i }).unwrap();
        assert!(set.is_empty());
        let removes: Vec<&str> = remove.iter().map(String::as_str).collect();
        let reset = ini::remove_keys(&written, "hud", &removes);
        assert_eq!(reset, file, "back to the file as it was: Coach's keys kept, the item's all gone");
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

    /// Jump calls ride on the line on the track and are on with it until the rider says.
    #[test]
    fn jump_calls_are_on_by_default_and_need_the_recorder_that_draws_them() {
        let hud = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(part(&hud, "jumps").on);
        assert_eq!(part(&hud, "jumps").needs, JUMPS_NEED);
        assert!(at_least("0.44.1", JUMPS_NEED) && !at_least("0.44.0", JUMPS_NEED));
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

    /// Pace hints are on by default, need the recorder that draws them, and bring the line on
    /// the track with them, since they are drawn over it.
    #[test]
    fn pace_hints_are_on_by_default_and_bring_the_line() {
        let none = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(part(&none, "pace").on, "on by default, as the plugin reads it (FrostMod 0.48)");
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

    /// Gear hints are on by default, need the recorder that draws them, and bring the line on
    /// the track with them, like the pace hints.
    #[test]
    fn gear_hints_are_on_by_default_and_bring_the_line() {
        let none = hud_of(Path::new("/nowhere/hud.ini"), false, false);
        assert!(part(&none, "gear").on, "on by default, as the plugin reads it (FrostMod 0.48)");
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

    /// No keys is the plugin's own look; what is written reads back; nonsense is the default.
    #[test]
    fn the_line_look_round_trips_through_hud_ini() {
        assert_eq!(look_of(&[]), LineLook::default(), "no keys: the line as it has always been");
        let dir = std::env::temp_dir().join(format!("coach-look-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hud.ini");
        // The other HUD keys are kept alongside it.
        ini::write(&path, "hud", &[("cue", "0".into())]).unwrap();
        let want = LineLook {
            width: 2.0,
            opacity: 0.45,
            colours: LineColours { gas: "#0000ff".into(), slow: "#123abc".into(), ..LineColours::default() },
            text: false,
            text_size: 1.5,
            text_style: "italic".into(),
            near_fade: 14.5,
        };
        ini::write(&path, "hud", &look_keys(&want)).unwrap();
        let pairs = ini::read_section(&fs::read_to_string(&path).unwrap(), "hud");
        assert_eq!(look_of(&pairs), want);
        assert_eq!(ini::get(&pairs, "cue"), Some("0"), "the rest of the file left alone");
        assert_eq!(ini::get(&pairs, "col_gas"), Some("#0000ff"));
        let _ = fs::remove_dir_all(&dir);

        let silly = tidy_look(LineLook {
            width: 9.0,
            opacity: f32::NAN,
            colours: LineColours { gas: "green".into(), coast: "#ABCDEF".into(), ..LineColours::default() },
            text: true,
            text_size: 0.0,
            text_style: "Gothic".into(),
            near_fade: 80.0,
        });
        assert_eq!(silly.width, 3.0, "clamped");
        assert_eq!(silly.opacity, 0.7, "not a number: the default");
        assert_eq!(silly.colours.gas, LineColours::default().gas, "not a colour: the default");
        assert_eq!(silly.colours.coast, "#abcdef", "any case of hex");
        assert_eq!(silly.text_size, 0.5);
        assert_eq!(silly.near_fade, 30.0, "clamped");
        assert_eq!(look_of(&[("line_fade".into(), "0".into())]).near_fade, 0.0, "off is allowed");
        assert_eq!(silly.text_style, "block", "a style the font can't draw is block");
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
