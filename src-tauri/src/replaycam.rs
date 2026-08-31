//! Replay camera paths — reading, editing and writing FrostMod's `.fcam` files.
//!
//! FrostMod's editor is a text panel drawn over a running game: fine for setting a key
//! while you scrub, useless for looking at a path you made last week. Nine unnamed slots
//! and no way to see what is in them is the gap this closes — the app can list them, show
//! the keys on a timeline, retime one, change the whole-path settings, and import or export
//! a path so it can be shared.
//!
//! The format is FrostMod's, and the definition lives in its `src/replaycam.h`. Plain text,
//! one key per line, hand-editable on purpose. The enums are carried as the same words the
//! file uses rather than numbers, so the two sides cannot disagree about what `2` meant.
//!
//! Writes are validated the way FrostMod would parse them, because a file it refuses to
//! load leaves someone staring at a slot that silently does nothing.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const MAGIC: &str = "frostmod-replaycam";
pub const VERSION: u32 = 2;
/// The replay stream's own sample rate. Every key time is a multiple of it.
pub const SAMPLE_MS: i32 = 30;
pub const MAX_KEYS: usize = 512;
pub const SLOTS: u8 = 9;

const EASES: [&str; 3] = ["smooth", "hold", "cut"];
const CURVES: [&str; 2] = ["centripetal", "uniform"];
const RIGS: [&str; 4] = ["locked", "handheld", "drone", "crane"];
const ANCHORS: [&str; 2] = ["clock", "track"];
/// FrostMod's `kNoTarget`.
pub const NO_TARGET: i32 = -1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    /// Replay clock, ms. Always a multiple of [`SAMPLE_MS`].
    pub t: i32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub fov: f32,
    /// What the path does when it leaves this key: `smooth`, `hold` or `cut`.
    pub ease: String,
    /// Race number this key aims at, or -1.
    pub target: i32,
    /// Metres to that rider when the key was set — what the framing is held against.
    pub aim_dist: f32,
    /// Lap fraction 0..1 when the key was set; negative means it was never known.
    pub tp: f32,
}

impl Default for Key {
    fn default() -> Self {
        Self {
            t: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov: 45.0,
            ease: "smooth".into(),
            target: NO_TARGET,
            aim_dist: 0.0,
            tp: -1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CamPath {
    pub keys: Vec<Key>,
    pub curve: String,
    pub rig: String,
    pub rig_amount: f32,
    pub anchor: String,
    pub auto_fov: bool,
}

impl Default for CamPath {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            curve: "centripetal".into(),
            rig: "locked".into(),
            rig_amount: 1.0,
            anchor: "clock".into(),
            auto_fov: false,
        }
    }
}

/// What the slot list shows without opening every file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub slot: u8,
    pub exists: bool,
    pub file: String,
    pub keys: usize,
    pub shots: usize,
    pub first_ms: i32,
    pub last_ms: i32,
    pub curve: String,
    pub rig: String,
    pub rig_amount: f32,
    pub anchor: String,
    pub auto_fov: bool,
    /// Race numbers this path aims at, in the order they first appear.
    pub targets: Vec<i32>,
    /// Set when the file is there but unreadable — better surfaced than shown as empty.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// the format
// ---------------------------------------------------------------------------

fn snap(ms: i32) -> i32 {
    if ms >= 0 {
        ((ms + SAMPLE_MS / 2) / SAMPLE_MS) * SAMPLE_MS
    } else {
        -(((-ms + SAMPLE_MS / 2) / SAMPLE_MS) * SAMPLE_MS)
    }
}

fn wrap_deg(d: f32) -> f32 {
    let mut d = d % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d <= -180.0 {
        d += 360.0;
    }
    d
}

fn one_of(word: &str, set: &[&str]) -> Option<String> {
    set.iter()
        .find(|w| w.eq_ignore_ascii_case(word))
        .map(|w| (*w).to_string())
}

/// Parse a `.fcam`. Version 1 files load too: the columns they never had take the values
/// the version that wrote them behaved as if it had.
pub fn parse(text: &str) -> Result<CamPath, String> {
    let mut out = CamPath::default();
    let mut have_header = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tok: Vec<&str> = line.split_whitespace().collect();
        if tok.is_empty() {
            continue;
        }

        if !have_header {
            if tok.len() < 2 || tok[0] != MAGIC {
                return Err("not a FrostMod replay camera path".into());
            }
            let ver: u32 = tok[1].parse().map_err(|_| "unreadable version".to_string())?;
            if ver == 0 || ver > VERSION {
                return Err(format!("unsupported path file version {ver}"));
            }
            have_header = true;
            continue;
        }

        if tok[0] == "path" {
            if tok.len() < 6 {
                return Err("malformed path line".into());
            }
            out.curve = one_of(tok[1], &CURVES).ok_or_else(|| format!("unknown curve {}", tok[1]))?;
            out.rig = one_of(tok[2], &RIGS).ok_or_else(|| format!("unknown rig {}", tok[2]))?;
            out.rig_amount = tok[3].parse::<f32>().unwrap_or(1.0).clamp(0.0, 4.0);
            out.anchor =
                one_of(tok[4], &ANCHORS).ok_or_else(|| format!("unknown anchor {}", tok[4]))?;
            out.auto_fov = tok[5] != "0";
            continue;
        }

        if tok.len() < 8 {
            return Err("malformed key line".into());
        }
        let num = |i: usize| -> Result<f32, String> {
            tok[i]
                .parse::<f32>()
                .map_err(|_| format!("{} is not a number", tok[i]))
        };
        let mut k = Key {
            t: snap(
                tok[0]
                    .parse::<i32>()
                    .map_err(|_| format!("{} is not a time", tok[0]))?,
            ),
            x: num(1)?,
            y: num(2)?,
            z: num(3)?,
            yaw: wrap_deg(num(4)?),
            pitch: wrap_deg(num(5)?),
            roll: wrap_deg(num(6)?),
            fov: num(7)?,
            ..Default::default()
        };
        if tok.len() >= 9 {
            k.ease = one_of(tok[8], &EASES).ok_or_else(|| format!("unknown ease {}", tok[8]))?;
        }
        if tok.len() >= 10 {
            k.target = tok[9].parse().unwrap_or(NO_TARGET);
        }
        if tok.len() >= 11 {
            k.aim_dist = num(10)?;
        }
        if tok.len() >= 12 {
            k.tp = num(11)?;
        }
        if out.keys.len() >= MAX_KEYS {
            return Err("too many keys".into());
        }
        out.keys.push(k);
    }

    if !have_header {
        return Err("empty path file".into());
    }
    out.keys.sort_by_key(|k| k.t);
    out.keys.dedup_by_key(|k| k.t);
    if out.anchor == "track" && out.keys.iter().any(|k| k.tp < 0.0) {
        return Err("track-anchored path has a key with no lap fraction".into());
    }
    Ok(out)
}

pub fn serialize(p: &CamPath) -> String {
    let mut s = format!("{MAGIC} {VERSION}\n");
    s.push_str(&format!(
        "path {} {} {:.3} {} {}\n",
        p.curve,
        p.rig,
        p.rig_amount,
        p.anchor,
        if p.auto_fov { 1 } else { 0 }
    ));
    s.push_str("# t_ms x y z yaw pitch roll fov ease target aimdist trackpos\n");
    for k in &p.keys {
        s.push_str(&format!(
            "{} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {} {} {:.3} {:.5}\n",
            k.t, k.x, k.y, k.z, k.yaw, k.pitch, k.roll, k.fov, k.ease, k.target, k.aim_dist, k.tp
        ));
    }
    s
}

/// Everything FrostMod's own parser would refuse, refused here first — with a sentence
/// saying what is wrong, rather than a slot that quietly does nothing in game.
pub fn validate(p: &CamPath) -> Result<(), String> {
    if p.keys.len() > MAX_KEYS {
        return Err(format!("a path can hold {MAX_KEYS} keys"));
    }
    if one_of(&p.curve, &CURVES).is_none() {
        return Err(format!("unknown curve {}", p.curve));
    }
    if one_of(&p.rig, &RIGS).is_none() {
        return Err(format!("unknown rig {}", p.rig));
    }
    if one_of(&p.anchor, &ANCHORS).is_none() {
        return Err(format!("unknown anchor {}", p.anchor));
    }
    if !(0.0..=4.0).contains(&p.rig_amount) {
        return Err("rig amount is 0 to 4".into());
    }
    for (i, k) in p.keys.iter().enumerate() {
        if one_of(&k.ease, &EASES).is_none() {
            return Err(format!("unknown ease {}", k.ease));
        }
        if k.t % SAMPLE_MS != 0 {
            return Err(format!("key {} is not on the {SAMPLE_MS} ms grid", i + 1));
        }
        if i > 0 && k.t <= p.keys[i - 1].t {
            return Err("two keys share a time".into());
        }
        if !k.x.is_finite() || !k.y.is_finite() || !k.z.is_finite() || !k.fov.is_finite() {
            return Err("a key has a value that is not a number".into());
        }
    }
    if p.anchor == "track" {
        if p.keys.iter().any(|k| k.tp < 0.0) {
            return Err("a lap-anchored path needs a lap fraction on every key".into());
        }
        if !p.keys.iter().any(|k| k.target != NO_TARGET) {
            return Err("a lap-anchored path has to follow a rider — aim a key first".into());
        }
    }
    Ok(())
}

/// A cut ends one shot and starts the next.
pub fn shot_count(keys: &[Key]) -> usize {
    if keys.is_empty() {
        return 0;
    }
    1 + keys[..keys.len() - 1].iter().filter(|k| k.ease == "cut").count()
}

/// Respace the keys so the time between them follows the distance between them, inside the
/// same first and last time. A cut keeps the length it was given: a shot is a duration
/// somebody chose, not a distance. Mirrors `RetimeByDistance` in FrostMod.
pub fn retime_by_distance(keys: &mut [Key]) -> Result<(), String> {
    let n = keys.len();
    if n < 3 {
        return Err("a path needs three keys before respacing means anything".into());
    }
    let (first, last) = (keys[0].t, keys[n - 1].t);
    let mut chord = vec![0.0f32; n - 1];
    let mut moving = 0.0f32;
    let mut held = 0i32;
    let mut moving_segs = 0i32;
    for i in 0..n - 1 {
        if keys[i].ease == "cut" {
            held += keys[i + 1].t - keys[i].t;
            continue;
        }
        let (dx, dy, dz) = (
            keys[i + 1].x - keys[i].x,
            keys[i + 1].y - keys[i].y,
            keys[i + 1].z - keys[i].z,
        );
        chord[i] = (dx * dx + dy * dy + dz * dz).sqrt();
        moving += chord[i];
        moving_segs += 1;
    }
    if moving <= 1e-3 {
        return Err("the path barely moves, so there is nothing to respace".into());
    }
    let budget = (last - first) - held;
    if budget < moving_segs * SAMPLE_MS {
        return Err("the path is too short to respace".into());
    }

    let mut dur = vec![0i32; n - 1];
    let mut total = 0i32;
    let mut widest: Option<usize> = None;
    for i in 0..n - 1 {
        dur[i] = if keys[i].ease == "cut" {
            keys[i + 1].t - keys[i].t
        } else {
            let d = snap((budget as f32 * (chord[i] / moving)) as i32).max(SAMPLE_MS);
            if widest.map_or(true, |w| d > dur[w]) {
                widest = Some(i);
            }
            d
        };
        total += dur[i];
    }
    // Snapping leaves the tail a sample or two off the end it started on; the longest moving
    // segment absorbs it so the path still finishes when it used to.
    if let Some(w) = widest {
        let slack = (last - first) - total;
        if dur[w] + slack >= SAMPLE_MS {
            dur[w] += slack;
        }
    }
    let mut t = first;
    for i in 0..n - 1 {
        keys[i].t = t;
        t += dur[i];
    }
    keys[n - 1].t = t;
    Ok(())
}

// ---------------------------------------------------------------------------
// where the files are
// ---------------------------------------------------------------------------

/// Every folder a `.fcam` could be in, best first.
///
/// FrostMod writes beside whichever copy of itself is running: the app's own install, a
/// `.dlo` dropped in the game's plugins folder, or — in plugin mode — the save path PiBoSo
/// hands it, which is the game's user folder. Looking in only one of them would show an
/// empty list to exactly the people using the feature most.
pub fn dirs(app: &AppHandle, cfg: &crate::config::AppConfig) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };

    // Plugin mode: PiBoSo's save path is the game's user folder, which is the folder the
    // profiles live in the parent of.
    let profiles = cfg.profiles_dir();
    if let Some(user) = profiles.parent() {
        if user.as_os_str().len() > 0 {
            push(user.join("FrostMod").join("replaycam"));
        }
    }
    let game = cfg.install_dir();
    if !game.trim().is_empty() {
        push(
            Path::new(game.trim())
                .join("plugins")
                .join("FrostMod")
                .join("replaycam"),
        );
    }
    push(
        app.path()
            .app_local_data_dir()
            .map(|d| d.join("frostmod"))
            .unwrap_or_default()
            .join("FrostMod")
            .join("replaycam"),
    );
    out
}

fn slot_name(slot: u8) -> String {
    format!("slot{slot}.fcam")
}

/// The file this slot is actually in, if any.
pub fn slot_file(app: &AppHandle, cfg: &crate::config::AppConfig, slot: u8) -> Option<PathBuf> {
    dirs(app, cfg)
        .into_iter()
        .map(|d| d.join(slot_name(slot)))
        .find(|f| f.is_file())
}

/// Where a new file for this slot goes: beside the paths that already exist, so a save from
/// the app lands where the game will look for it.
pub fn write_target(app: &AppHandle, cfg: &crate::config::AppConfig, slot: u8) -> PathBuf {
    if let Some(existing) = slot_file(app, cfg, slot) {
        return existing;
    }
    let cands = dirs(app, cfg);
    cands
        .iter()
        .find(|d| d.is_dir())
        .cloned()
        .or_else(|| cands.first().cloned())
        .unwrap_or_default()
        .join(slot_name(slot))
}

pub fn read(app: &AppHandle, cfg: &crate::config::AppConfig, slot: u8) -> Result<CamPath, String> {
    let file = slot_file(app, cfg, slot).ok_or_else(|| "no path saved in that slot".to_string())?;
    let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    parse(&text)
}

pub fn write(
    app: &AppHandle,
    cfg: &crate::config::AppConfig,
    slot: u8,
    p: &CamPath,
) -> Result<PathBuf, String> {
    validate(p)?;
    let file = write_target(app, cfg, slot);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(&file, serialize(p)).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

pub fn delete(app: &AppHandle, cfg: &crate::config::AppConfig, slot: u8) -> Result<(), String> {
    let mut removed = false;
    // Every copy, not the first: leaving one behind means the slot reappears next time the
    // list is drawn, which reads as the delete having failed.
    for dir in dirs(app, cfg) {
        let f = dir.join(slot_name(slot));
        if f.is_file() {
            fs::remove_file(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            removed = true;
        }
    }
    if removed {
        Ok(())
    } else {
        Err("no path saved in that slot".into())
    }
}

pub fn list(app: &AppHandle, cfg: &crate::config::AppConfig) -> Vec<Summary> {
    (1..=SLOTS)
        .map(|slot| {
            let file = slot_file(app, cfg, slot);
            let mut s = Summary {
                slot,
                exists: file.is_some(),
                file: file
                    .as_ref()
                    .map(|f| f.display().to_string())
                    .unwrap_or_default(),
                keys: 0,
                shots: 0,
                first_ms: 0,
                last_ms: 0,
                curve: "centripetal".into(),
                rig: "locked".into(),
                rig_amount: 1.0,
                anchor: "clock".into(),
                auto_fov: false,
                targets: Vec::new(),
                error: None,
            };
            let Some(file) = file else { return s };
            match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| parse(&t)) {
                Ok(p) => {
                    s.keys = p.keys.len();
                    s.shots = shot_count(&p.keys);
                    s.first_ms = p.keys.first().map(|k| k.t).unwrap_or(0);
                    s.last_ms = p.keys.last().map(|k| k.t).unwrap_or(0);
                    s.curve = p.curve;
                    s.rig = p.rig;
                    s.rig_amount = p.rig_amount;
                    s.anchor = p.anchor;
                    s.auto_fov = p.auto_fov;
                    for k in &p.keys {
                        if k.target != NO_TARGET && !s.targets.contains(&k.target) {
                            s.targets.push(k.target);
                        }
                    }
                }
                Err(e) => s.error = Some(e),
            }
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: i32, x: f32) -> Key {
        Key { t, x, ..Default::default() }
    }

    #[test]
    fn round_trip_keeps_everything() {
        let p = CamPath {
            keys: vec![
                Key {
                    t: 0,
                    x: 1.5,
                    y: 2.25,
                    z: -3.5,
                    yaw: 90.0,
                    pitch: -12.0,
                    roll: 7.5,
                    fov: 40.0,
                    ease: "hold".into(),
                    target: 42,
                    aim_dist: 18.5,
                    tp: 0.25,
                },
                Key { t: 990, ease: "cut".into(), ..key(990, 2.5) },
            ],
            curve: "uniform".into(),
            rig: "drone".into(),
            rig_amount: 1.25,
            anchor: "clock".into(),
            auto_fov: true,
        };
        let back = parse(&serialize(&p)).expect("round trip");
        assert_eq!(back, p);
    }

    #[test]
    fn v1_files_still_load() {
        let p = parse("frostmod-replaycam 1\n0 0 0 0 0 0 0 45\n300 1 0 0 0 0 0 45\n")
            .expect("a v1 file must still load");
        assert_eq!(p.keys.len(), 2);
        assert_eq!(p.keys[0].ease, "smooth");
        assert_eq!(p.keys[0].target, NO_TARGET);
        assert_eq!(p.anchor, "clock");
        assert_eq!(p.rig, "locked");
    }

    #[test]
    fn rubbish_is_refused_rather_than_half_loaded() {
        assert!(parse("").is_err());
        assert!(parse("something else 1\n0 0 0 0 0 0 0 45\n").is_err());
        assert!(parse("frostmod-replaycam 99\n").is_err());
        assert!(parse("frostmod-replaycam 1\n0 0 0\n").is_err());
        assert!(parse("frostmod-replaycam 2\n0 0 0 0 0 0 0 45 sideways -1 0 -1\n").is_err());
        assert!(parse("frostmod-replaycam 2\npath spiral locked 1.0 clock 0\n").is_err());
    }

    #[test]
    fn keys_are_sorted_and_snapped_on_load() {
        let p = parse("frostmod-replaycam 1\n600 1 0 0 190 0 0 45\n10 0 0 0 0 0 0 45\n").unwrap();
        assert_eq!(p.keys[0].t, 0);
        assert_eq!(p.keys[1].t, 600);
        assert!((p.keys[1].yaw - -170.0).abs() < 1e-3, "190 should fold to -170");
    }

    #[test]
    fn validate_catches_what_frostmod_would_refuse() {
        let mut p = CamPath { keys: vec![key(0, 0.0), key(300, 1.0)], ..Default::default() };
        assert!(validate(&p).is_ok());

        p.keys[1].t = 301;
        assert!(validate(&p).is_err(), "off-grid key must be refused");
        p.keys[1].t = 0;
        assert!(validate(&p).is_err(), "two keys on one time must be refused");
        p.keys[1].t = 300;

        p.anchor = "track".into();
        assert!(validate(&p).is_err(), "a lap-anchored path with no rider must be refused");
        p.keys[0].target = 7;
        assert!(validate(&p).is_err(), "a lap-anchored path with no lap fractions must be refused");
        p.keys[0].tp = 0.1;
        p.keys[1].tp = 0.2;
        assert!(validate(&p).is_ok());
    }

    #[test]
    fn shots_are_counted_by_cuts() {
        let mut keys = vec![key(0, 0.0), key(300, 1.0), key(600, 2.0)];
        assert_eq!(shot_count(&keys), 1);
        keys[1].ease = "cut".into();
        assert_eq!(shot_count(&keys), 2);
        // A cut on the last key ends nothing: there is no shot after it.
        keys[2].ease = "cut".into();
        assert_eq!(shot_count(&keys), 2);
    }

    #[test]
    fn retiming_follows_distance_and_leaves_cuts_alone() {
        let mut keys = vec![key(0, 0.0), key(300, 1.0), key(900, 100.0)];
        retime_by_distance(&mut keys).expect("plenty of room");
        assert_eq!(keys[0].t, 0);
        assert_eq!(keys[2].t, 900);
        assert!(keys[1].t < 60, "the one-metre leg kept {} ms", keys[1].t);

        let mut held = vec![key(0, 0.0), key(300, 1.0), key(900, 100.0)];
        held[0].ease = "cut".into();
        retime_by_distance(&mut held).expect("a cut is not a reason to refuse");
        assert_eq!(held[1].t - held[0].t, 300, "a cut keeps the length it was given");

        let mut still = vec![key(0, 0.0), key(300, 0.0), key(900, 0.0)];
        assert!(retime_by_distance(&mut still).is_err());
    }
}
