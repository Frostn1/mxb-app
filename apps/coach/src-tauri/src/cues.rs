//! Live cues: the few short calls the recorder plugin shows during a lap in practice, like
//! "Brake here" or "Stand up", picked from where the rider loses time.
//!
//! The fast lap says where each thing happens (`analysis::cue_points`); the rider's review says
//! where it costs time. The rider chooses how much coaching and at what level: a new rider is
//! told the basics everywhere, a pro only the calls that matter. The pick is written per track
//! and bike as a `.cue` file the plugin reads: FrostMod's `src/coachcue.h`, `MXCQ` version 1,
//! pinned byte for byte by `tests/coachcue_test.cpp` there and by the test here.
//!
//! **Two rules about what a cue may say**, both from a rider who rode a sheet of them:
//!
//! * A cue is an instruction at a spot, never a summary. "Take the fast line" told them
//!   nothing; "Go inside here" is a thing to do with the bars. Every line here names a side,
//!   a control or a body position, and the line cues say inside or outside in those words.
//! * A shift is only called where the gearing rules found it costing time. Calling a shift
//!   because the fast lap happened to change gear there reads as "go slower" to a rider who
//!   is carrying speed on purpose, which is exactly how it was taken.
//!
//! And the sheet itself moves on. `pick` is given what the rider has already been told
//! ([`History`]); a call that has been up for the last few laps rests while something else
//! that is costing time now takes its place. A cue the rider has actually fixed leaves on its
//! own, because the section it belongs to stops losing time and stops qualifying.

use serde::{Deserialize, Serialize};

use crate::analysis::{cue, CuePoint, Review};

pub const MAGIC: &[u8; 4] = b"MXCQ";
/// Stays 1 when a cue kind is added. The plugin refuses the whole sheet on a version it does
/// not know, so bumping it to announce a kind would silence every cue for every rider still on
/// the old recorder — whereas a kind it has no clip for simply draws in white and says nothing,
/// the way `CUSTOM` always has. That is why `cue::ROLL` needs no version gate.
pub const VERSION: u32 = 1;
/// How long before its spot a cue shows, at the bike's speed, and how long it stays up.
const LEAD_S: f32 = 1.2;
const SHOW_S: f32 = 1.5;
/// Two cues closer than this would crowd each other: the more important one is kept.
const APART_M: usize = 30;
/// The plugin draws at most 99 bytes of text.
const MAX_TEXT: usize = 99;

/// Sheets in a row a call may be on before it rests, and how many it then sits out. A rider
/// who has heard "Brake here" at the same corner four laps running has either taken it or
/// isn't going to, and either way there is something else worth the space.
const REPEATS: u32 = 3;
const REST: u32 = 2;
/// What a resting call's score is multiplied by. Not zero: on a track where only one thing is
/// losing time, hearing it again beats hearing nothing.
const RESTING: f32 = 0.2;
/// What a judgement is worth on the sheet when the clock says nothing. Scored like a tenth of
/// a second lost, so a section that measurably costs more than that always outranks it.
const JUDGED: f32 = 0.1;

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

/// What a cue says. Short enough to read at a glance — the plugin's cue box fits about 26
/// characters — and every one of them an instruction at the spot it fires.
///
/// The plugin picks the spoken clip by `kind`, not by this text, so the wording can change
/// without touching the voice files.
fn text(kind: u8) -> &'static str {
    match kind {
        cue::BRAKE => "Brake here",
        cue::OFF_BRAKES => "Off the brakes",
        cue::THROTTLE => "Gas now",
        // Named as a gear, not as a moment: "shift earlier" is what read as "go slower".
        cue::UPSHIFT => "One gear higher",
        cue::DOWNSHIFT => "One gear lower",
        cue::WIDE => "Stay wide",
        cue::INSIDE => "Go inside here",
        cue::SCRUB => "Scrub it flat",
        cue::STAND => "Stand up",
        cue::SIT => "Sit down",
        // Not "roll it": in the engine's own words that already means not jumping the thing at
        // all — `jump_it`'s tip reads "the fast lap jumps it and you roll it" — which is the
        // opposite of the fix here. "Roll off" is the throttle.
        cue::ROLL => "Roll off here",
        _ => "",
    }
}

/// What a rider at each level is told about. The basics first; the finer calls as they go up.
fn allowed(level: Level, kind: u8) -> bool {
    let basics = [cue::BRAKE, cue::THROTTLE, cue::STAND, cue::SIT];
    let gears = [cue::OFF_BRAKES, cue::UPSHIFT, cue::DOWNSHIFT];
    // `ROLL` sits with these: telling a rider to ease off before a lip is only safe once they
    // clear it every lap. Say it to a new one and the next attempt lands on the up-face, which
    // is the mistake that hurts. Until then the lesson they need is the opposite one, commit.
    let fine = [cue::WIDE, cue::INSIDE, cue::SCRUB, cue::ROLL];
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
        // Nothing: this is the bonus that floats a call up where little time was lost, and a
        // rider who is jumping long on purpose and losing nothing for it hears "Roll off here"
        // as "go slower" — the same way a shift called off the fast lap's gear change read.
        // It goes on the sheet for the time it costs or not at all.
        cue::ROLL => 0.0,
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
        // Coming in hot is fixed by braking sooner, and the brake call already sits at the fast
        // lap's brake point: the right metre is the whole tip, so it counts double there.
        "brake_early" | "brake_late" | "brake_harder" | "front_lock" | "rear_lock" | "in_too_hot" => cue::BRAKE,
        // `land_short` asks for the same thing as `chop_face` — gas held to the lip — so it
        // doubles the same call.
        "coasting" | "late_throttle" | "throttle_room" | "exit_speed" | "whoops_throttle" | "chop_face"
        | "land_short" => cue::THROTTLE,
        "gear_up" => cue::UPSHIFT,
        "gear_down" => cue::DOWNSHIFT,
        "scrub" => cue::SCRUB,
        "whoops_bucking" | "whoops_speed" | "stance_stand" => cue::STAND,
        "stance_sit" => cue::SIT,
        // `apex_early` is about when the bike is turned, not what a control does at a spot.
        // There is no call that says it: "Go inside here" names a side and would send them
        // tighter still. It stays a review tip with the map beside it.
        _ => return None,
    })
}

/// The cue a section's own tip asks for, where the tip is about a thing a call can fix at a
/// spot. This is the only way a shift cue is ever made: `gear_up` and `gear_down` come from
/// the gearing rule in `analysis::corner`, which fires only where the rider's gear leaves
/// them slower out of the corner than the fast lap.
fn from_finding(skill: &str, title: &str) -> Option<u8> {
    Some(match skill {
        "gear_up" => cue::UPSHIFT,
        "gear_down" => cue::DOWNSHIFT,
        // The over-jump verdict is read off the ground, so it says where without a fast lap to
        // compare against, and its `at` is the takeoff: the call fires at the face, which is
        // the last place the rider can change where they land.
        "overjump" => cue::ROLL,
        // Which way the fast line goes, in the words the tip uses.
        "line" => {
            if title.contains("outside") {
                cue::WIDE
            } else {
                cue::INSIDE
            }
        }
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
    /// That section's stable id, which the rotation keys on.
    pub section_id: String,
}

/// One call the rider has already been given, and how long it has been running.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// The section's stable id, not its name: a name can be renumbered and then the rotation
    /// silently forgets what the rider has already been told, or worse, credits it elsewhere.
    pub section: String,
    pub kind: u8,
    /// Sheets in a row this call has been on.
    #[serde(default)]
    pub runs: u32,
    /// Sheets it still sits out before it can come back.
    #[serde(default)]
    pub resting: u32,
}

/// Bumped when a stored history can no longer be read as meaning what it says. v1 keyed on
/// section names, which a new personal best could renumber.
pub const HISTORY_VERSION: u32 = 2;

/// What the rider has been told on this track and bike so far. Kept beside the session index
/// and handed back to [`pick`] each time a sheet is written, so every sheet knows what the
/// last one said.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    /// Bumped when the meaning of `Sent.section` changes. An older file is dropped: it keys on
    /// names that could drift, which is the very thing the stable ids fixed.
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub sent: Vec<Sent>,
}

impl History {
    fn find(&self, section: &str, kind: u8) -> Option<&Sent> {
        self.sent.iter().find(|s| s.kind == kind && s.section == section)
    }

    /// Whether this call is resting, and so should be passed over while there is anything else.
    fn resting(&self, section: &str, kind: u8) -> bool {
        self.find(section, kind).is_some_and(|s| s.resting > 0)
    }

    /// How much a call's score is worth now: full, or a fifth of it while it rests.
    fn weight(&self, section: &str, kind: u8) -> f32 {
        match self.find(section, kind) {
            Some(s) if s.resting > 0 => RESTING,
            _ => 1.0,
        }
    }

    /// The history after a sheet holding `chosen`. A call that was picked counts another run,
    /// and goes to rest once it has had its share of them; one that wasn't loses its run and
    /// serves a sheet of any rest it owed. A call that is neither running nor resting is
    /// dropped — the rider fixed it, or moved on, and it is nothing to carry.
    fn after(&self, chosen: &[CueOut]) -> History {
        let mut out: Vec<Sent> = Vec::new();
        for c in chosen {
            let was = self.find(&c.section_id, c.kind);
            let runs = was.map_or(0, |s| s.runs) + 1;
            let rest = was.map_or(0, |s| s.resting);
            let (runs, resting) = if runs >= REPEATS { (0, REST) } else { (runs, rest.saturating_sub(1)) };
            out.push(Sent { section: c.section_id.clone(), kind: c.kind, runs, resting });
        }
        for s in &self.sent {
            if chosen.iter().any(|c| c.kind == s.kind && c.section_id == s.section) {
                continue;
            }
            let resting = s.resting.saturating_sub(1);
            if resting > 0 {
                out.push(Sent { section: s.section.clone(), kind: s.kind, runs: 0, resting });
            }
        }
        History { version: HISTORY_VERSION, sent: out }
    }
}

/// A sheet of cues, and the history to keep for the next one.
pub struct Picked {
    pub cues: Vec<CueOut>,
    pub history: History,
}

/// The cues for a rider at `level` who asked for `amount`, from where the fast lap does each
/// thing and where this lap loses time, with `seen` holding what the last sheets said. In lap
/// order.
pub fn pick(points: &[CuePoint], review: &Review, level: Level, amount: Amount, seen: &History) -> Picked {
    let (max, _) = timing(amount);
    let mut cands: Vec<(f32, usize, u8, usize, bool)> = Vec::new();
    let mut consider = |at: usize, kind: u8, si: usize| {
        let Some(sr) = review.sections.get(si) else { return };
        let lost = sr.lost.max(0.0);
        // A judgement holds whatever the clock says — that is what makes it one — so a section
        // carrying one qualifies on the judgement rather than on the time. Without this a lap
        // ridden with no fast lap to compare against shows nothing lost anywhere and the sheet
        // comes out empty, so the rider would never be told in game about a jump they over-jump
        // every single lap. Which is the case the absolute rules exist for.
        let judged = sr
            .findings
            .iter()
            .any(|f| f.absolute && (from_finding(f.skill, &f.title) == Some(kind) || answers(f.skill) == Some(kind)));
        let too_cheap = lost < min_loss(level) || (level != Level::New && lost <= 0.0);
        if !allowed(level, kind) || (too_cheap && !judged) {
            return;
        }
        // A cue made from a tip is answering that tip, so it counts double the same way a cue
        // the fast lap placed does. Without the second arm the line calls were the only kind
        // that could never be doubled: `line` has no `answers` entry, while `gear_up` and
        // `gear_down` sit in both tables and always were.
        let answered = sr
            .findings
            .iter()
            .any(|f| answers(f.skill) == Some(kind) || from_finding(f.skill, &f.title) == Some(kind));
        let resting = seen.resting(&sr.section.id, kind);
        let score = (lost * if answered { 2.0 } else { 1.0 }
            + if judged { JUDGED } else { 0.0 }
            + basic(kind) * basics_weight(level))
            * seen.weight(&sr.section.id, kind);
        cands.push((score, at, kind, si, resting));
    };
    for p in points {
        consider(p.at, p.kind, p.section);
    }
    // The calls that come from a section's own tips: which way its line goes, and a gear that
    // is costing time. Both say where on track they belong, so they fire at the spot.
    for (si, sr) in review.sections.iter().enumerate() {
        for f in &sr.findings {
            if let Some(kind) = from_finding(f.skill, &f.title) {
                consider(f.at, kind, si);
            }
        }
    }
    cands.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    // Anything resting is passed over entirely while there is something else to say, and only
    // taken on the second pass when there isn't. Scoring it down was not enough: a fifth of a
    // big number still beats a small one, and where the candidates barely fill the sheet the
    // same calls came back however long the rider had been hearing them. Resting has to mean
    // "not this time" for the sheet to move on at all.
    let mut chosen: Vec<(f32, usize, u8, usize, bool)> = Vec::new();
    for pass_resting in [false, true] {
        for c in &cands {
            if chosen.len() >= max as usize {
                break;
            }
            if c.4 != pass_resting {
                continue;
            }
            if chosen.iter().any(|k| k.1.abs_diff(c.1) < APART_M) {
                continue;
            }
            chosen.push(*c);
        }
    }
    // Back into score order: the sheet is read in order, and two passes put the rested ones
    // at the end regardless of how much time they cost.
    chosen.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let n = chosen.len();
    let mut cues: Vec<CueOut> = chosen
        .into_iter()
        .enumerate()
        .map(|(rank, (_, at, kind, si, _))| CueOut {
            at: at as f32,
            kind,
            priority: (255 - (rank * 200 / n.max(1)) as i32).max(1) as u8,
            text: text(kind).to_string(),
            section: review.sections[si].section.name.clone(),
            section_id: review.sections[si].section.id.clone(),
        })
        .collect();
    cues.sort_by(|x, y| x.at.total_cmp(&y.at));
    let history = seen.after(&cues);
    Picked { cues, history }
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

/// Where this track and bike's history is kept, under the coach's own data folder.
pub fn history_name(track: &str, bike: &str) -> String {
    format!("{}.{}.json", safe_name(track), safe_name(bike))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{lap, Style, CASED, FAST, OVER};
    use crate::analysis::{cue_points, review, sections, Bike};

    const BIKE: Bike =
        Bike { limiter: 13000.0, max_rpm: 14000.0, shift_rpm: 12500.0, travel: [0.3, 0.3], land_scale: 0.0, torque_scale: 0.0 };

    fn picked(st: &Style, level: Level, amount: Amount, seen: &History) -> Picked {
        let (fast, mine) = (lap(&FAST), lap(st));
        let rv = review(&mine, &fast, BIKE);
        pick(&cue_points(&fast, &sections(&fast)), &rv, level, amount, seen)
    }

    fn cues_for(st: &Style, level: Level, amount: Amount) -> Vec<CueOut> {
        picked(st, level, amount, &History::default()).cues
    }

    /// The case the absolute rules exist for: riding on your own, with no faster lap to be held
    /// against. Every section then shows no time lost, so before judgements qualified a section
    /// on their own merit this sheet came out empty and the rider heard nothing all session.
    #[test]
    fn a_judgement_earns_its_place_on_the_sheet_with_no_fast_lap() {
        let rv = crate::analysis::solo(&lap(&OVER), BIKE);
        assert!(
            rv.sections.iter().all(|s| s.lost == 0.0),
            "a solo review has no time lost to rank by, which is the whole difficulty"
        );
        let fast = lap(&FAST);
        let sheet = pick(
            &cue_points(&fast, &sections(&fast)),
            &rv,
            Level::Pro,
            Amount::Normal,
            &History::default(),
        );
        assert!(
            sheet.cues.iter().any(|c| c.kind == cue::ROLL),
            "the over-jump is still the thing worth saying: {:?}",
            sheet.cues.iter().map(|c| (&c.section, c.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_measured_loss_still_outranks_a_judgement() {
        // The judgement is worth saying, not worth saying first. A corner that demonstrably
        // costs half a second has to come off the sheet ahead of a jump that costs nothing.
        let slow = Style { decel: 2.5, brake: 0.5, corner_v: 8.0, ..OVER };
        let sheet = cues_for(&slow, Level::Pro, Amount::Few);
        let roll = sheet.iter().position(|c| c.kind == cue::ROLL);
        let braking = sheet.iter().position(|c| c.kind == cue::BRAKE);
        if let (Some(r), Some(b)) = (roll, braking) {
            let by_priority = sheet[b].priority >= sheet[r].priority;
            assert!(by_priority, "braking that costs real time should not rank under the judgement");
        }
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

    /// Every cue is an instruction, and the ones about the line say which side of the track.
    #[test]
    fn every_cue_is_something_to_do_at_a_spot() {
        for kind in (1..=10u8).chain([cue::ROLL]) {
            let t = text(kind);
            assert!(!t.is_empty(), "kind {kind} has no words");
            assert!(t.len() <= 26, "\"{t}\" is too long for the cue box");
            assert!(!t.contains("fast line"), "\"{t}\" is a summary, not an instruction");
        }
        assert_eq!(text(cue::WIDE), "Stay wide");
        assert_eq!(text(cue::INSIDE), "Go inside here");
        // A shift is named as a gear to be in, never as a moment to go slower.
        for kind in [cue::UPSHIFT, cue::DOWNSHIFT] {
            assert!(text(kind).contains("gear"), "{}", text(kind));
            assert!(!text(kind).contains("earlier"), "{}", text(kind));
        }
    }

    /// A line tip names the side, and the cue it makes matches it.
    #[test]
    fn a_line_tip_becomes_the_side_it_names() {
        assert_eq!(from_finding("line", "Take the outside line"), Some(cue::WIDE));
        assert_eq!(from_finding("line", "Take the inside line"), Some(cue::INSIDE));
        assert_eq!(from_finding("gear_up", ""), Some(cue::UPSHIFT));
        assert_eq!(from_finding("gear_down", ""), Some(cue::DOWNSHIFT));
        assert_eq!(from_finding("carry_speed", "Carry more speed"), None);
    }

    /// A line call is the section's own tip, so it counts double the way a gear call always
    /// has. Before this, `line` was missing from `answers` while `gear_up` sat in both tables,
    /// so on a short sheet the line call lost to whatever else the corner was costing.
    #[test]
    fn a_line_call_counts_double_like_a_gear_call_does() {
        let off_line = Style { wide: 3.0, corner_v: 9.0, decel: 2.5, brake: 0.5, ..FAST };
        let (fast, mine) = (lap(&FAST), lap(&off_line));
        let rv = review(&mine, &fast, BIKE);
        let line_tips: Vec<_> =
            rv.sections.iter().flat_map(|s| s.findings.iter()).filter(|f| f.skill == "line").collect();
        assert!(!line_tips.is_empty(), "the lap needs a line tip to be a test of one");
        for f in &line_tips {
            let kind = from_finding(f.skill, &f.title).expect("a line tip names a side");
            assert!(
                answers(f.skill) != Some(kind),
                "this test is about the case `answers` does not cover"
            );
        }
        // On the tightest sheet there is, the line call still earns its place.
        let c = pick(&cue_points(&fast, &sections(&fast)), &rv, Level::SubPro, Amount::Few, &History::default()).cues;
        assert!(
            c.iter().any(|c| c.kind == cue::WIDE || c.kind == cue::INSIDE),
            "the line call should survive a two-cue sheet: {c:?}"
        );
    }

    /// Where the fast lap shifts is not on its own a reason to call a shift: only the gearing
    /// rule, which fires where the gear costs time, can put one on the sheet.
    #[test]
    fn a_shift_is_only_called_where_the_gearing_rule_found_one() {
        let fast = lap(&FAST);
        let points = cue_points(&fast, &sections(&fast));
        assert!(
            points.iter().all(|p| p.kind != cue::UPSHIFT && p.kind != cue::DOWNSHIFT),
            "the fast lap's own gear changes are not cues"
        );
        // So a lap whose review has no gearing tip gets no shift cue at any level.
        let rv = review(&lap(&Style { decel: 2.5, brake: 0.5, ..FAST }), &fast, BIKE);
        let has_gear_tip = rv.sections.iter().any(|s| s.findings.iter().any(|f| f.skill.starts_with("gear_")));
        let c = pick(&points, &rv, Level::Intermediate, Amount::Lots, &History::default()).cues;
        let shifts = c.iter().filter(|c| c.kind == cue::UPSHIFT || c.kind == cue::DOWNSHIFT).count();
        assert_eq!(shifts > 0, has_gear_tip, "{c:?}");
    }

    /// The rider's complaint in one test: the same call, lap after lap. After a few sheets it
    /// stands down and the next thing that is costing time takes its place.
    #[test]
    fn a_call_the_rider_has_heard_for_three_sheets_rests() {
        let slow = Style { decel: 2.5, brake: 0.5, corner_v: 9.0, ..FAST };
        let mut seen = History::default();
        let mut sheets: Vec<Vec<(String, u8)>> = Vec::new();
        for _ in 0..5 {
            let out = picked(&slow, Level::New, Amount::Few, &seen);
            sheets.push(out.cues.iter().map(|c| (c.section.clone(), c.kind)).collect());
            seen = out.history;
        }
        let first = &sheets[0];
        assert!(!first.is_empty(), "there is something to say to begin with");
        // The opening call is on the first sheets, then stands down.
        let head = first[0].clone();
        assert!(sheets[..3].iter().all(|s| s.contains(&head)), "{sheets:?}");
        assert!(!sheets[3].contains(&head), "after three sheets it rests: {sheets:?}");
        assert!(sheets.iter().any(|s| *s != *first), "the sheet moves on: {sheets:?}");
        // And it comes back once it has served its rest, rather than being lost for good.
        assert!(sheets[4..].iter().any(|s| s.contains(&head)) || sheets[3..].iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn a_history_from_before_the_ids_is_not_mis_attributed() {
        // v1 stored section names. A name can have been renumbered since, so the rotation
        // would credit the wrong corner; the reader drops anything that isn't this version.
        let v1 = br#"{"sent":[{"section":"Turn 5","kind":1,"runs":2,"resting":0}]}"#;
        let read: History = serde_json::from_slice(v1).expect("v1 still parses");
        assert_ne!(read.version, HISTORY_VERSION, "a versionless file must not pass the gate");
        // And what this version writes does pass it.
        assert_eq!(History::default().after(&[]).version, HISTORY_VERSION);
    }

    /// A call the rider has taken leaves on its own: its section stops losing time, so it
    /// stops qualifying and nothing is carried for it.
    #[test]
    fn a_call_that_was_taken_is_not_kept_alive() {
        let seen = History { version: HISTORY_VERSION, sent: vec![Sent { section: "t1".into(), kind: cue::BRAKE, runs: 2, resting: 0 }] };
        // A clean lap at pro level qualifies nothing at all.
        let out = picked(&FAST, Level::Pro, Amount::Normal, &seen);
        assert!(out.cues.is_empty());
        assert!(out.history.sent.is_empty(), "nothing running and nothing resting is nothing to keep");
    }

    #[test]
    fn writes_the_file_the_plugin_reads() {
        let c = [CueOut { at: 500.0, kind: cue::BRAKE, priority: 200, text: "Brake here".into(), section: "Turn 1".into(), section_id: "t1".into() }];
        let b = write(1000.0, &c, Amount::Normal);
        assert_eq!(&b[..4], b"MXCQ");
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let f32_at = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        assert_eq!(u32_at(4), 1);
        assert_eq!((f32_at(8), f32_at(12), f32_at(16), f32_at(20)), (1000.0, 1.2, 1.5, 3.0));
        assert_eq!((u32_at(24), u32_at(28)), (4, 1));
        assert_eq!(f32_at(32), 500.0);
        assert_eq!((b[36], b[37]), (cue::BRAKE, 200));
        assert_eq!(u16::from_le_bytes([b[38], b[39]]), 10);
        assert_eq!(&b[40..], b"Brake here");
    }

    #[test]
    fn file_names_match_the_plugin() {
        assert_eq!(file_name("indiana nationals", "MX2OEM_2023_KTM_250_SX-F"), "indiana_nationals.MX2OEM_2023_KTM_250_SX-F.cue");
        assert_eq!(history_name("indiana nationals", "KX450"), "indiana_nationals.KX450.json");
    }

    /// A resting call is passed over while there is anything else to say, not merely scored
    /// down. Scoring it down was not enough: a fifth of a big number still beats a small one,
    /// so where the candidates barely filled the sheet the rider heard the same calls forever.
    #[test]
    fn a_resting_call_gives_up_its_place_to_a_fresh_one() {
        let slow = Style { decel: 2.5, brake: 0.5, corner_v: 9.0, ..FAST };
        let fresh = picked(&slow, Level::New, Amount::Normal, &History::default());
        assert!(fresh.cues.len() >= 2, "need a couple of calls to swap between: {:?}", fresh.cues.len());
        let head = &fresh.cues[0];

        // Rest the top call, and nothing else.
        let seen = History {
            version: HISTORY_VERSION,
            sent: vec![Sent { section: head.section_id.clone(), kind: head.kind, runs: 0, resting: 2 }],
        };
        let after = picked(&slow, Level::New, Amount::Normal, &seen);
        let still_there = after.cues.iter().any(|c| c.section == head.section && c.kind == head.kind);
        assert!(!still_there, "the rested call kept its place: {:?}", after.cues.iter().map(|c| (&c.section, c.kind)).collect::<Vec<_>>());
        assert!(!after.cues.is_empty(), "and something else was said instead");

        // With nothing else to say it comes back rather than leaving the rider in silence:
        // resting means "not while there is better", not "never".
        // On ids, not names: `History::find` compares ids, so a sheet of names rests nothing
        // and what follows would pass without testing anything.
        assert_ne!(head.section, head.section_id, "a history keyed on the name would be inert");
        let only = History {
            version: HISTORY_VERSION,
            sent: fresh.cues.iter().map(|c| Sent { section: c.section_id.clone(), kind: c.kind, runs: 0, resting: 2 }).collect(),
        };
        let all_rested = picked(&slow, Level::New, Amount::Normal, &only);
        assert!(!all_rested.cues.is_empty(), "silence is worse than a repeat");
        // And it is the rested calls themselves that come back, not the sheet quietly filling
        // with whatever was left over: that would be silence on everything that matters.
        let back = |c: &CueOut| fresh.cues.iter().any(|f| f.section_id == c.section_id && f.kind == c.kind);
        assert!(all_rested.cues.iter().any(back), "{:?}", all_rested.cues.iter().map(|c| (&c.section_id, c.kind)).collect::<Vec<_>>());
    }

    /// Over-jumping asks for a roll, and asks at the lip: once the bike is in the air there is
    /// nothing left to do about where it comes down.
    #[test]
    fn over_jumping_asks_for_a_roll_at_the_face() {
        // `OVER` carries the fast lap's speed through a longer flight, so the jump costs it
        // nothing and the sheet, which is scored on time lost, never sees it. Bleed the speed
        // it holds in the air and the same over-jump costs time.
        let over = Style { air_v: 16.0, ..OVER };
        let c = cues_for(&over, Level::Pro, Amount::Lots);
        let roll = c.iter().find(|c| c.kind == cue::ROLL).unwrap_or_else(|| panic!("{c:?}"));
        assert_eq!(roll.text, "Roll off here");
        // The lip is at 330 m and the bike lands out on the flat at 375: the call belongs at
        // the first of those, where the throttle still decides the rest.
        assert!(roll.at < 340.0, "called at {} m, which is the landing, not the face", roll.at);
        // And a rider who can't clear it every lap is not told to ease off.
        assert!(!cues_for(&over, Level::New, Amount::Lots).iter().any(|c| c.kind == cue::ROLL));
    }

    /// Coming in too fast is fixed at the brake point, and the brake call already stands there:
    /// so it doubles that one rather than inventing a call of its own.
    #[test]
    fn coming_in_too_fast_doubles_the_brake_call_in_that_corner() {
        // Hot into Turn 1 and slow out of it, so the corner actually loses time.
        let hot = Style { hot: 45.0, corner_v: 6.0, accel: 3.0, gas: 0.6, ..FAST };
        let (fast, mine) = (lap(&FAST), lap(&hot));
        let points = cue_points(&fast, &sections(&fast));
        let mut rv = review(&mine, &fast, BIKE);
        let t1 = rv.sections.iter().position(|s| s.section.name == "Turn 1").expect("Turn 1");
        let skills: Vec<&str> = rv.sections[t1].findings.iter().map(|f| f.skill).collect();
        assert!(skills.contains(&"in_too_hot"), "{skills:?}");
        // Nothing else in the corner answers the brake call, so what follows is this tip's doing.
        assert_eq!(skills.iter().filter(|s| answers(s) == Some(cue::BRAKE)).count(), 1, "{skills:?}");
        let top = |cues: &[CueOut]| {
            let c = cues.iter().max_by_key(|c| c.priority).expect("a sheet").clone();
            (c.kind, c.section)
        };
        let with = pick(&points, &rv, Level::Intermediate, Amount::Normal, &History::default()).cues;
        assert_eq!(top(&with), (cue::BRAKE, "Turn 1".to_string()), "{with:?}");

        // Take the tip away and the brake call drops behind the gas call. Same lap, same time
        // lost, same cue points and the same level: the doubling is all that moved.
        rv.sections[t1].findings.retain(|f| f.skill != "in_too_hot");
        let without = pick(&points, &rv, Level::Intermediate, Amount::Normal, &History::default()).cues;
        assert_eq!(top(&without), (cue::THROTTLE, "Turn 1".to_string()), "{without:?}");
    }

    /// Casing asks for the gas — the same call `chop_face` asks for, because it is the same
    /// fix — and never for a roll, which would put the rider further onto the up-face.
    #[test]
    fn casing_a_jump_asks_for_the_gas() {
        let rv = review(&lap(&Style { air_v: 16.0, ..CASED }), &lap(&FAST), BIKE);
        let j = rv.sections.iter().find(|s| s.section.name == "Jump 1").expect("Jump 1");
        let tip = j.findings.iter().find(|f| f.skill == "land_short").unwrap_or_else(|| {
            panic!("{:?}", j.findings.iter().map(|f| f.skill).collect::<Vec<_>>())
        });
        // It stops at the mapping because a jump of its own has no gas call to double: a jump
        // section's cue points are the scrub and nothing else. The doubling lands where the
        // jump is folded into a corner, which is where `chop_face`'s has always landed too.
        assert_eq!(answers(tip.skill), Some(cue::THROTTLE));
        assert_eq!(answers("land_short"), answers("chop_face"));
        assert_eq!(from_finding(tip.skill, &tip.title), None);
    }
}
