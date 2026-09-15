//! Live cues: the few short calls the recorder plugin shows during a lap in practice, like
//! "Brake" or "Stand up", picked from where the rider loses time.
//!
//! The fast lap says where each thing happens (`analysis::cue_points`); the rider's review says
//! where it costs time. The rider chooses how much coaching and at what level: a new rider is
//! told the basics everywhere, a pro only the calls that matter. The pick is written per track
//! and bike as a `.cue` file the plugin reads: FrostMod's `src/coachcue.h`, `MXCQ` version 1,
//! pinned byte for byte by `tests/coachcue_test.cpp` there and by the test here.

use serde::{Deserialize, Serialize};

use crate::analysis::{cue, CuePoint, Review};

pub const MAGIC: &[u8; 4] = b"MXCQ";
pub const VERSION: u32 = 1;
/// How long before its spot a cue shows, at the bike's speed, and how long it stays up.
const LEAD_S: f32 = 1.2;
const SHOW_S: f32 = 1.5;
/// Two cues closer than this would crowd each other: the more important one is kept.
const APART_M: usize = 30;
/// The plugin draws at most 99 bytes of text.
const MAX_TEXT: usize = 99;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    New,
    Intermediate,
    SubPro,
    Pro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Amount {
    Few,
    Normal,
    Lots,
}

/// Cues a lap, and at least this long between two.
fn timing(a: Amount) -> (u32, f32) {
    match a {
        Amount::Few => (2, 5.0),
        Amount::Normal => (4, 3.0),
        Amount::Lots => (6, 2.0),
    }
}

fn text(kind: u8) -> &'static str {
    match kind {
        cue::BRAKE => "Brake",
        cue::OFF_BRAKES => "Off the brakes",
        cue::THROTTLE => "Gas",
        cue::UPSHIFT => "Shift up",
        cue::DOWNSHIFT => "Shift down",
        cue::WIDE => "Go wide",
        cue::INSIDE => "Cut inside",
        cue::SCRUB => "Scrub it",
        cue::STAND => "Stand up",
        cue::SIT => "Sit down",
        _ => "",
    }
}

/// What a rider at each level is told about. The basics first; the finer calls as they go up.
fn allowed(level: Level, kind: u8) -> bool {
    let basics = [cue::BRAKE, cue::THROTTLE, cue::STAND, cue::SIT];
    let gears = [cue::OFF_BRAKES, cue::UPSHIFT, cue::DOWNSHIFT];
    let fine = [cue::WIDE, cue::INSIDE, cue::SCRUB];
    match level {
        Level::New => basics.contains(&kind),
        Level::Intermediate => basics.contains(&kind) || gears.contains(&kind),
        // Past the basics a rider knows to sit; everything else is fair game.
        Level::SubPro | Level::Pro => {
            kind != cue::SIT && (basics.contains(&kind) || gears.contains(&kind) || fine.contains(&kind))
        }
    }
}

/// Time a section has to lose before a rider at this level is told about it, seconds. A new
/// rider hears the basics everywhere.
fn min_loss(level: Level) -> f32 {
    match level {
        Level::New => 0.0,
        Level::Intermediate => 0.05,
        Level::SubPro => 0.1,
        Level::Pro => 0.15,
    }
}

/// How basic a call is, and how much that counts at each level.
fn basic(kind: u8) -> f32 {
    match kind {
        cue::BRAKE => 1.0,
        cue::THROTTLE => 0.9,
        cue::OFF_BRAKES => 0.7,
        cue::STAND => 0.6,
        cue::WIDE | cue::INSIDE => 0.6,
        cue::UPSHIFT | cue::DOWNSHIFT | cue::SCRUB => 0.5,
        cue::SIT => 0.4,
        _ => 0.0,
    }
}
fn basics_weight(level: Level) -> f32 {
    match level {
        Level::New => 0.3,
        Level::Intermediate => 0.2,
        Level::SubPro => 0.1,
        Level::Pro => 0.0,
    }
}

/// The cue a tip is about, so a cue that answers one of the section's tips counts double.
fn answers(skill: &str) -> Option<u8> {
    Some(match skill {
        "brake_early" | "brake_late" | "brake_harder" | "front_lock" | "rear_lock" => cue::BRAKE,
        "coasting" | "late_throttle" | "throttle_room" | "exit_speed" | "whoops_throttle" | "chop_face" => cue::THROTTLE,
        "gear_up" => cue::UPSHIFT,
        "gear_down" => cue::DOWNSHIFT,
        "scrub" => cue::SCRUB,
        "whoops_bucking" | "whoops_speed" => cue::STAND,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueOut {
    /// Metres into the lap.
    pub at: f32,
    pub kind: u8,
    /// Higher shows first when two are due together.
    pub priority: u8,
    pub text: String,
    /// The section it belongs to, for showing the list.
    pub section: String,
}

/// The cues for a rider at `level` who asked for `amount`, from where the fast lap does each
/// thing and where this lap loses time. In lap order.
pub fn pick(points: &[CuePoint], review: &Review, level: Level, amount: Amount) -> Vec<CueOut> {
    let (max, _) = timing(amount);
    let mut cands: Vec<(f32, usize, u8, usize)> = Vec::new();
    let mut consider = |at: usize, kind: u8, si: usize| {
        let Some(sr) = review.sections.get(si) else { return };
        let lost = sr.lost.max(0.0);
        if !allowed(level, kind) || lost < min_loss(level) || (level != Level::New && lost <= 0.0) {
            return;
        }
        let answered = sr.findings.iter().any(|f| answers(f.skill) == Some(kind));
        let score = lost * if answered { 2.0 } else { 1.0 } + basic(kind) * basics_weight(level);
        cands.push((score, at, kind, si));
    };
    for p in points {
        consider(p.at, p.kind, p.section);
    }
    // Lines come from the tips: which way the fast line goes, at its apex.
    for (si, sr) in review.sections.iter().enumerate() {
        for f in sr.findings.iter().filter(|f| f.skill == "line") {
            let kind = if f.title.contains("wider") { cue::WIDE } else { cue::INSIDE };
            consider(f.at, kind, si);
        }
    }
    cands.sort_by(|x, y| y.0.total_cmp(&x.0));
    let mut chosen: Vec<(f32, usize, u8, usize)> = Vec::new();
    for c in cands {
        if chosen.len() >= max as usize {
            break;
        }
        if chosen.iter().any(|k| k.1.abs_diff(c.1) < APART_M) {
            continue;
        }
        chosen.push(c);
    }
    let n = chosen.len();
    let mut out: Vec<CueOut> = chosen
        .into_iter()
        .enumerate()
        .map(|(rank, (_, at, kind, si))| CueOut {
            at: at as f32,
            kind,
            priority: (255 - (rank * 200 / n.max(1)) as i32).max(1) as u8,
            text: text(kind).to_string(),
            section: review.sections[si].section.name.clone(),
        })
        .collect();
    out.sort_by(|x, y| x.at.total_cmp(&y.at));
    out
}

/// The `.cue` file for these cues.
pub fn write(track_len: f32, cues: &[CueOut], amount: Amount) -> Vec<u8> {
    let (max, gap) = timing(amount);
    let mut b = MAGIC.to_vec();
    b.extend_from_slice(&VERSION.to_le_bytes());
    for f in [track_len, LEAD_S, SHOW_S, gap] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b.extend_from_slice(&max.to_le_bytes());
    b.extend_from_slice(&(cues.len() as u32).to_le_bytes());
    for c in cues {
        let t = &c.text.as_bytes()[..c.text.len().min(MAX_TEXT)];
        b.extend_from_slice(&c.at.to_le_bytes());
        b.push(c.kind);
        b.push(c.priority);
        b.extend_from_slice(&(t.len() as u16).to_le_bytes());
        b.extend_from_slice(t);
    }
    b
}

/// An id made safe for a file name, the same way the plugin makes it.
pub fn safe_name(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') { c } else { '_' }).collect()
}

/// The file the plugin looks for first: this bike on this track.
pub fn file_name(track: &str, bike: &str) -> String {
    format!("{}.{}.cue", safe_name(track), safe_name(bike))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{lap, Style, FAST};
    use crate::analysis::{cue_points, review, sections, Bike};

    const BIKE: Bike =
        Bike { limiter: 13000.0, max_rpm: 14000.0, shift_rpm: 12500.0, travel: [0.3, 0.3], land_scale: 0.0, torque_scale: 0.0 };

    fn cues_for(st: &Style, level: Level, amount: Amount) -> Vec<CueOut> {
        let (fast, mine) = (lap(&FAST), lap(st));
        let rv = review(&mine, &fast, BIKE);
        pick(&cue_points(&fast, &sections(&fast)), &rv, level, amount)
    }

    #[test]
    fn a_new_rider_braking_early_is_told_where_to_brake() {
        let c = cues_for(&Style { decel: 2.5, brake: 0.5, ..FAST }, Level::New, Amount::Normal);
        assert!(c.iter().any(|c| c.kind == cue::BRAKE), "{c:?}");
        assert!(c.iter().all(|c| [cue::BRAKE, cue::THROTTLE, cue::STAND, cue::SIT].contains(&c.kind)), "{c:?}");
        assert!(c.len() <= 4);
        assert!(c.windows(2).all(|w| w[0].at <= w[1].at), "in lap order");
    }

    #[test]
    fn how_much_coaching_caps_the_cues() {
        let slow = Style { decel: 2.5, brake: 0.5, corner_v: 9.0, ..FAST };
        assert!(cues_for(&slow, Level::New, Amount::Few).len() <= 2);
        assert!(cues_for(&slow, Level::New, Amount::Lots).len() > cues_for(&slow, Level::New, Amount::Few).len());
    }

    #[test]
    fn a_pro_hears_nothing_where_no_time_is_lost() {
        assert!(cues_for(&FAST, Level::Pro, Amount::Lots).is_empty());
        // A new rider still gets the basics.
        assert!(!cues_for(&FAST, Level::New, Amount::Normal).is_empty());
    }

    #[test]
    fn writes_the_file_the_plugin_reads() {
        let c = [CueOut { at: 500.0, kind: cue::BRAKE, priority: 200, text: "Brake".into(), section: "Turn 1".into() }];
        let b = write(1000.0, &c, Amount::Normal);
        assert_eq!(&b[..4], b"MXCQ");
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let f32_at = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!(u32_at(4), 1);
        assert_eq!((f32_at(8), f32_at(12), f32_at(16), f32_at(20)), (1000.0, 1.2, 1.5, 3.0));
        assert_eq!((u32_at(24), u32_at(28)), (4, 1));
        assert_eq!(f32_at(32), 500.0);
        assert_eq!((b[36], b[37]), (cue::BRAKE, 200));
        assert_eq!(u16::from_le_bytes([b[38], b[39]]), 5);
        assert_eq!(&b[40..], b"Brake");
    }

    #[test]
    fn file_names_match_the_plugin() {
        assert_eq!(file_name("indiana nationals", "MX2OEM_2023_KTM_250_SX-F"), "indiana_nationals.MX2OEM_2023_KTM_250_SX-F.cue");
    }
}
