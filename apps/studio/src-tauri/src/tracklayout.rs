//! A lap from a seed, with the arithmetic to prove it before it is built.
//!
//! There are two halves to making a track and they fail differently. The *shape* — where the
//! lap goes — is geometry, and geometry is checkable: a lap either closes or it does not,
//! either crosses itself or does not, either carries the corners and jumps a published track
//! carries or does not. The *character* — sand or loam, tight or flowing — is taste, and taste
//! is what a model is for. This module does the first half, on its own, from a number.
//!
//! The shape is a closed loop whose radius wobbles with the angle round its centre. That gives
//! two properties for free, and they are the two that every attempt at this failed on:
//!
//! - it **closes**, because it goes all the way round;
//! - it **cannot cross itself**, because it is star-shaped — one radius per angle.
//!
//! Its corners are then filleted into arcs, which is what a `.tcl` is made of. What is left of
//! each edge is a straight.
//!
//! Everything after that is measurement. [`draw`] makes a lap; [`search`] makes laps until one
//! passes [`crate::trackllm::review`] *and* measures up as ground once synthesised — the
//! grooves it wears, the berms it grows, the faces its jumps stand at. A lap that reads well
//! on paper and rides like a ploughed field is one nobody wants, and the only way to know
//! which one this is, is to build it and measure.

use crate::trackllm::corpus;
use crate::trackprog::{
    Discipline, Feature, Relief, Segment, Start, Surface, Terrain, TrackProgram,
};

/// What a discipline changes about a lap: the ground it is drawn on, what the walk draws from,
/// where it starts and finishes, and what review holds it to. [`Discipline::rules`] picks one.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// The ground a lap is drawn on, metres across (x) and down (z).
    pub plot: (f32, f32),
    /// Samples on the plot's long edge.
    pub samples: u32,
    /// What the walk draws a lap's length, the riding line's width and the start straight from.
    pub lap_m: (f32, f32),
    pub width_m: (f32, f32),
    pub start_straight_draw_m: (f32, f32),
    /// The tightest radius in a corner, metres.
    pub apex_m: (f32, f32),
    /// Under this radius a corner is no place for a take-off: a rider cannot leave the ground
    /// square out of a turn that tight.
    pub tight_m: f32,
    /// How far round the lap is left bare for the gate row and the run to the first jump.
    pub start_clear_m: f32,
    /// The most ground between two corners, straight or gentler than [`GENTLE_R_M`].
    pub run_max_m: f32,
    /// What *one* stretch on the lap may reach, where a discipline allows a long one.
    ///
    /// Equal to [`Rules::run_max_m`] where none is allowed, which is every lap the walk drew
    /// before this existed. SuperMotocross is the exception: Charlotte runs 351 m down one
    /// side and nothing else on the lap comes near it, and a lap capped at 45 m everywhere
    /// cannot draw that shape at all.
    pub long_run_m: f32,
    /// The longest straight a rider may meet, and the longest stretch the way home may carry.
    /// See [`STRAIGHT_CAP_M`] and [`HOME_STRETCH_MAX_M`], which are these for motocross.
    pub straight_cap_m: f32,
    pub home_run_max_m: f32,
    /// How often a drawn corner is a 180 rather than the corpus's ordinary three-arc turn.
    ///
    /// Zero makes no roll at all, so a motocross seed draws exactly the sequence it always
    /// has. SuperMotocross wants 7–10 hairpins out of 12–22 corners, which the outdoor walk
    /// only reaches by accident on its own.
    pub hairpin_share: f32,
    /// The shortest straight a start fits beside.
    pub start_straight_m: f32,
    /// How far off the lap the gate row stands, and how long its sprint runs before it turns in.
    pub start_offset_m: f32,
    pub start_sprint_m: f32,
    /// The finish jump: how tall, how long its deck (the air a rider clears), and its take-off.
    pub finish_jump_m: (f32, f32),
    pub finish_deck_m: (f32, f32),
    pub finish_face_m: f32,
    /// Whether the lap finishes over a triple rather than a tabletop.
    ///
    /// Every measured stadium round ends on one — 22.6–25.3 m crest to crest, 2.0–2.2 m tall
    /// — and a triple is a drawn shape rather than one of the two kinds
    /// [`crate::trackprog::TrackProgram::finish_jump`] looks for, so it has to be told.
    /// Motocross keeps its tabletop: everybody lands on the finish jump there.
    pub finish_triple: bool,
    /// Gates in the start row. Sets how wide the start pad is, see [`Rules::fan_half_m`].
    pub gates: usize,
    /// The landscape the walk draws: hills, fall across the plot, banks, and how many
    /// straights climb or drop.
    pub amplitude_m: (f32, f32),
    pub tilt_m: (f32, f32),
    pub landforms: (i32, i32),
    pub elevation_changes: usize,
    /// A flat floor: no hills, no fall, no banks.
    pub flat: bool,
    /// What review holds the lap to.
    pub review: corpus::Bands,
    /// A stadium floor and how its lanes are laid. `None` walks an outdoor lap.
    pub stadium: Option<Stadium>,
    /// What the lap is built from, where a discipline is built from sections rather than from
    /// the corpus's mix of jumps. `None` places features the way motocross always has.
    pub sections: Option<Sections>,
}

impl Rules {
    /// Half the start pad's width: the gate row plus a margin.
    ///
    /// Forty gates at 1.2 m is 48 m across, and the pad has to hold it. The *layout* needs it
    /// as well as the synthesiser: a start line whose centreline clears the lap by fifteen
    /// metres still lays its pad straight over it.
    pub fn fan_half_m(&self) -> f32 {
        self.gates as f32 * crate::tracksynth::GRID_LANE_M * 0.5 + 3.0
    }
}

/// A stadium floor, and how a supercross lap's lanes are laid on it.
///
/// A lap is lanes across the floor, each ending where the draw puts it rather than at the wall,
/// and one lane down the side that brings it home. Where the next lane's end falls decides the
/// corner: back the way it came is a 180, on past it is a step across on two 90s. So the corner
/// mix and the spread of runs both come out of the run lengths, which is what the measured
/// tracks vary — median 64 m, longest 109.
#[derive(Clone, Copy, Debug)]
pub struct Stadium {
    /// The biggest floor a lap is laid on, metres along and across, centred on the plot.
    pub floor: (f32, f32),
    /// How many lanes across the floor.
    pub lanes: (i32, i32),
    /// Centre to centre between two lanes, and how long the floor is along them.
    pub lane_gap_m: (f32, f32),
    pub length_m: (f32, f32),
    /// From the floor's edge to a lane's centreline.
    pub edge_m: f32,
    /// How far a lane runs before its corner, and the start straight, which is lane one.
    pub lane_run_m: (f32, f32),
    pub start_run_m: (f32, f32),
    /// The odds a lane turns back the way it came rather than stepping across.
    pub switchback: f32,
    /// The odds a lane skips a row, which turns its 180 into two 90s round a short link.
    pub wide: f32,
}

/// The pieces a lap is built from, per lap unless it says otherwise.
#[derive(Clone, Copy, Debug)]
pub struct Sections {
    /// Rhythm lanes: how many, how long, how many hills, their pitch and height.
    pub rhythm_lanes: (u32, u32),
    pub rhythm_m: (f32, f32),
    pub rhythm_hills: (u32, u32),
    pub rhythm_pitch_m: (f32, f32),
    pub rhythm_h_m: (f32, f32),
    /// Whoops: how many sections, whoops in one, pitch, height, and a section's length.
    pub whoop_sections: (u32, u32),
    pub whoops: (u32, u32),
    pub whoop_pitch_m: (f32, f32),
    pub whoop_h_m: (f32, f32),
    pub whoops_m: f32,
    /// Triples and their crest-to-crest span.
    pub triples: (u32, u32),
    pub triple_span_m: (f32, f32),
    /// Standalone doubles and rhythm doubles, crest to crest.
    pub doubles: (u32, u32),
    pub double_m: (f32, f32),
    pub rhythm_doubles: (u32, u32),
    pub rhythm_double_m: (f32, f32),
    /// Tabletops and their decks.
    pub tables: (u32, u32),
    pub table_deck_m: (f32, f32),
    /// How far a corner banks, degrees: median and ninetieth.
    pub bank_deg: (f32, f32),
    /// Sand: how many stretches of the lap are laid with it, and how long one runs.
    ///
    /// A band `(0, 1)` is what "about half the rounds have sand in them" comes out as — the
    /// count is drawn per seed, so half of them get one.
    pub sand_sections: (u32, u32),
    pub sand_m: (f32, f32),
    /// Stadium blocks spliced into an outdoor lap: two or three parallel lanes joined by 180s,
    /// carrying a rhythm lane and bordered with tuff blocks. `(0, 0)` on a lap that is already
    /// a stadium floor.
    pub stadium_blocks: (u32, u32),
}

// ------------------------------------------------------------------------------------------
// The rules, one row per discipline.
//
// MX is what the walk has always drawn: every value is the constant it replaced, and a seed
// makes the same draws it always has (`northgate_keeps_its_layout`).
//
// SX and SMX: M = measured off 11 community tracks' own .trh/.rdf (7 SX, 3 SMX, Daytona) plus
// 15 real rounds, P = published, E = estimated. The measurements, their sources and each
// value's status are in `docs/tracks/real-track-corpus.md` §12. Change a value there too.
// ------------------------------------------------------------------------------------------

/// Motocross.
pub const MX_RULES: Rules = Rules {
    // Smaller than the 620 m it was, on purpose. A 1700 m lap inside a 620 m plot is shorter
    // than the plot's own perimeter, so a ring fits and the walk never folds inwards. Indiana
    // is 525 m across with a 2138 m lap: it has to double back through its own middle.
    plot: (470.0, 470.0),
    samples: 2049,
    // 1451 m rode as "overall small" and 2400 m "a bit too big". Indiana is 2138.
    lap_m: (2000.0, 2350.0),
    // Ridden and called "little skinny" at 10–13.5 m.
    width_m: (14.5, 18.0),
    start_straight_draw_m: (90.0, 130.0),
    apex_m: (8.0, 15.0),
    tight_m: 22.0,
    start_clear_m: 130.0,
    // The corpus runs 20–72 m (p50 27–30). The rider: no long stretches, no big chicanes.
    run_max_m: 55.0,
    // No long run anywhere: the rider asked for no long stretches outright.
    long_run_m: 55.0,
    straight_cap_m: STRAIGHT_CAP_M,
    home_run_max_m: HOME_STRETCH_MAX_M,
    // No roll, so a seed draws exactly the sequence it always has.
    hairpin_share: 0.0,
    start_straight_m: 60.0,
    // Measured off six published tracks' own start lines: Indiana 37.9, SandPoint 36.4,
    // Briarcliff 41.0, I40 47.2, SFDR 48.9, Smokey Pines 34.0. A spur beside the lap.
    start_offset_m: 40.0,
    // Published start lines run 79–91 m before their first corner. Ridden at 80 m as a long
    // drag with nothing to do; shorter, so the pack arrives at turn one still bunched.
    start_sprint_m: 55.0,
    // Full size: at three quarters it rode too small for the one jump a track is known by.
    finish_jump_m: (2.4, 3.0),
    // Past the published twelve: at 3 m the finish jump still rode small, and a longer deck
    // makes it bigger without making it taller.
    finish_deck_m: (crate::trackprog::TABLETOP_DECK_M, 20.0),
    // Longer and gentler than the angle gives a 3 m face on its own: at 13 m it leaves at 26°.
    finish_face_m: 16.0,
    // Everybody lands on a national's finish jump, so it is a table.
    finish_triple: false,
    // Indiana, Millville, Washougal, Maryland and the GP tracks all ship 40.
    gates: 40,
    amplitude_m: (4.2, 7.5),
    tilt_m: (6.0, 16.0),
    landforms: (2, 4),
    elevation_changes: 2,
    flat: false,
    review: corpus::MX,
    stadium: None,
    sections: None,
};

/// Supercross: a flat stadium floor of parallel lanes joined by 180s.
pub const SX_RULES: Rules = Rules {
    // Square, so a lap fits whichever way round its floor is laid — and 50 m of ground beyond
    // the biggest floor, which is what the start pad needs: it stands 20 m off the first lane,
    // is 16 m wide either side of that, and its bank reaches further still.
    plot: (240.0, 240.0),
    samples: 1025,
    // M: 770 / 779 / 880 (p10 / p50 / p90). Target 780.
    lap_m: (750.0, 900.0),
    // M: 9.3–11.3 on the stadium tracks, median 10. P: 6.1 m is the real minimum.
    width_m: (8.5, 10.0),
    // M: 35–110 from the gate to the holeshot, target 75.
    start_straight_draw_m: (35.0, 115.0),
    // M: 5.8 / 6.7 / 7.8.
    apex_m: (5.5, 8.0),
    // E: under any SX corner, so a take-off can come straight out of a turn.
    tight_m: 5.0,
    // E.
    start_clear_m: 20.0,
    // M: the longest run between corners is 90–130.
    run_max_m: 130.0,
    // The lanes are the runs, and the floor is what caps them: nothing here is special.
    long_run_m: 130.0,
    straight_cap_m: 140.0,
    home_run_max_m: 140.0,
    // The stadium walk lays its own corners; it never asks `a_corner` for one.
    hairpin_share: 0.0,
    // M: 35.
    start_straight_m: 35.0,
    // M: the gate stands 9.6–29.9 m off the line, median 16.8. 20 keeps the 22-gate row
    // (26.4 m) off the lane beside it.
    start_offset_m: 20.0,
    // E: the gate to the holeshot measures 34–108, median 76.
    start_sprint_m: 45.0,
    // M: 2.0–2.2 m tall.
    finish_jump_m: (2.0, 2.2),
    // M: usually a triple of 22.6–25.3 m crest to crest; built as a tabletop of that air.
    finish_deck_m: (20.0, 25.0),
    // E: the measured 30° lip wants about 6 m, and no face here is shorter than 9.
    finish_face_m: crate::trackprog::JUMP_FACE_MIN_M,
    // M: every one of the seven ends over a triple, not a table.
    finish_triple: true,
    // P.
    gates: 22,
    // M: a flat floor, the lap climbs about 0.
    amplitude_m: (0.0, 0.0),
    tilt_m: (0.0, 0.0),
    landforms: (0, 0),
    elevation_changes: 0,
    flat: true,
    review: corpus::Bands {
        // P: 6.1 m minimum. M: 9.3–11.3.
        width_m: (6.0, 12.0),
        // M: 670–907 across all seven.
        lap_m: (650.0, 1000.0),
        // M: 57–72 — but that is the corpus's own count, and it is not this measurement.
        //
        // `analyse.py` counts every stretch of ground standing off the floor, so a whoop, a
        // rhythm hill and each face of a triple are one apiece. Review counts `Feature::lips`,
        // which is what a *programme* says it throws a rider off. The two are close but not
        // the same unit, and a band copied across from one to the other is a band nothing can
        // ever satisfy. Stated in this unit: a section-built lap measures 27–52 with a median
        // of 41, and the floor is set below that rather than at it — low enough to still catch
        // a lap with bare lanes on it, which is the fault this band exists to find.
        lips_per_km: (25.0, 90.0),
        // M: about 0.
        lap_climb_m: (0.0, 5.0),
        segments: (10.0, 200.0),
        // Lanes are straights joined by turns, so about half the segments are straights.
        arc_fraction: (0.3, 1.0),
        // M: 1075–1618.
        total_turn_deg: (900.0, 2400.0),
        // M: 7–10.
        turns: (6.0, 12.0),
        // M: apex 5.6–8.2.
        turn_radius_m: (4.5, 10.0),
        amplitude_m: (0.0, 2.0),
        // E.
        lap_avg_kmh: (25.0, 60.0),
        ..corpus::MX
    },
    stadium: Some(Stadium {
        // M: 117–150 along and 72–139 across. The FSX floors are the tight ones (72–86); the
        // 2020 rounds are 124–139, and a lap needs that room to carry nine lanes.
        floor: (140.0, 120.0),
        // M: 7–10 corners, 5–8 of them 180s — one per lane, less one for each step or skip.
        lanes: (7, 9),
        // E: a 10 m lane and a row of tuff blocks; a 180 between two is 5.5–8 m.
        lane_gap_m: (12.0, 14.0),
        // M: the floors run 117–138 along; a lane has to hold the longest run.
        length_m: (125.0, 140.0),
        edge_m: 7.0,
        // M: runs measure 64 at the median and 90–130 at the longest.
        lane_run_m: (45.0, 110.0),
        // M: 35–110, typically about 75. The bottom is what a finish jump needs, which is
        // what keeps these at the top of the measured range.
        start_run_m: (75.0, 110.0),
        // E: enough steps across that a lap is not a boustrophedon, and few enough that the
        // 180s stay 5–8 of the corners.
        switchback: 0.75,
        // E: the odd wide turn round a short link rather than a 180, which is where the
        // measured 0–3 corners that are not 180s come from.
        wide: 0.2,
    }),
    // M, per lap.
    sections: Some(Sections {
        rhythm_lanes: (4, 7),
        rhythm_m: (40.0, 65.0),
        rhythm_hills: (5, 6),
        rhythm_pitch_m: (9.5, 10.5),
        rhythm_h_m: (0.9, 1.5),
        whoop_sections: (1, 2),
        whoops: (9, 10),
        whoop_pitch_m: (5.2, 5.6),
        whoop_h_m: (0.45, 0.7),
        whoops_m: 48.0,
        triples: (0, 2),
        triple_span_m: (22.6, 25.3),
        doubles: (0, 1),
        double_m: (17.0, 18.0),
        rhythm_doubles: (1, 3),
        rhythm_double_m: (10.0, 13.0),
        tables: (0, 4),
        table_deck_m: (6.5, 11.0),
        bank_deg: (2.0, 8.0),
        // P: Racer X's round previews put sand in about half of them, so half the seeds draw
        // one and half draw none.
        sand_sections: (0, 1),
        sand_m: (35.0, 60.0),
        // The whole lap is a stadium already.
        stadium_blocks: (0, 0),
    }),
};

/// SuperMotocross: the outdoor lap with the stadium's shape in it, and 22 gates.
///
/// Not motocross drawn smaller. The three measured rounds are outdoor laps carrying stadium
/// furniture — 12–22 corners with 7–10 of them 180s, one very long side, triples at 27 m, and
/// a block or two of parallel lanes in the middle of it. What it does not carry is whoops.
pub const SMX_RULES: Rules = Rules {
    // E: a square the outdoor walk can fill. M: up to 540 x 200.
    plot: (400.0, 400.0),
    samples: 2049,
    // M: 1141 / 1582 / 1925. Drawn inside the band review holds it to.
    lap_m: (1150.0, 1900.0),
    // M: 10.0–10.5.
    width_m: (10.0, 13.0),
    start_straight_draw_m: (70.0, 110.0),
    // M: 6.8–11.5.
    apex_m: (7.0, 11.5),
    tight_m: 18.0,
    start_clear_m: 110.0,
    // M: runs measure 42–52 at the median on the three. At 45 as a *ceiling* the median came
    // out near 25 and no lane was long enough to hold a rhythm section.
    run_max_m: 60.0,
    // M: Charlotte runs 351 m down one side and nothing else on that lap reaches 60. One
    // stretch a lap may go that far; every other run stays short.
    long_run_m: 350.0,
    straight_cap_m: 350.0,
    // At least as much as `long_run_m`: the check runs over the whole finished lap, so a
    // lower ceiling here refuses every way home once the long side has been laid.
    home_run_max_m: 350.0,
    // M: 7–10 of 12–22 corners are 180s. The outdoor walk reaches that only by accident —
    // `a_corner`'s three arcs sum past 170 on about one roll in twelve.
    hairpin_share: 0.45,
    start_straight_m: 60.0,
    // M: 10.5 and 28.3 off the line on two of three.
    start_offset_m: 30.0,
    start_sprint_m: 45.0,
    finish_jump_m: (2.2, 2.8),
    finish_deck_m: (crate::trackprog::TABLETOP_DECK_M, 16.0),
    finish_face_m: 14.0,
    // M: the three all end over a triple, as the stadium rounds do.
    finish_triple: true,
    gates: 22,
    // E: flatter than MX.
    amplitude_m: (1.0, 2.5),
    tilt_m: (0.0, 3.0),
    landforms: (0, 1),
    elevation_changes: 0,
    flat: false,
    review: corpus::Bands {
        // M: 1141–1925.
        lap_m: (1100.0, 1950.0),
        // M: 37 / 44 / 45 a kilometre by the corpus's count, which is not this measurement —
        // see the note on the supercross row. In this unit an SMX lap measures 25–33, which
        // is a national's density with the sections laid over the top of it, and that is what
        // SuperMotocross is.
        lips_per_km: (20.0, 55.0),
        lap_climb_m: (0.0, 20.0),
        total_turn_deg: (1000.0, 3600.0),
        // M: 12 / 15 / 22.
        turns: (12.0, 22.0),
        // M: apex 6.8–11.5.
        turn_radius_m: (6.0, 30.0),
        amplitude_m: (0.5, 30.0),
        // The FFM's 125 m is a motocross rule and this is not a motocross lap: Charlotte's own
        // long side is 351 m, so review has to allow the shape the corpus actually has.
        straight_m: 350.0,
        straight_with_obstacle_m: 360.0,
        ..corpus::MX
    },
    stadium: None,
    // M where the three tracks agree, E otherwise.
    sections: Some(Sections {
        // E: shorter than a stadium's, and three hills rather than five or six.
        rhythm_lanes: (2, 5),
        rhythm_m: (25.0, 40.0),
        rhythm_hills: (3, 3),
        rhythm_pitch_m: (9.5, 10.5),
        rhythm_h_m: (0.9, 1.5),
        // None. Whoops belong to the stadium, and none of the three measured laps carries a set.
        whoop_sections: (0, 0),
        whoops: (0, 0),
        whoop_pitch_m: (0.0, 0.0),
        whoop_h_m: (0.0, 0.0),
        whoops_m: 0.0,
        // M: 3, 6 and 7 of them, at 26.5–27.4 m crest to crest.
        triples: (3, 7),
        triple_span_m: (25.0, 29.0),
        doubles: (2, 4),
        double_m: (15.0, 20.0),
        rhythm_doubles: (0, 2),
        rhythm_double_m: (10.0, 13.0),
        tables: (0, 4),
        table_deck_m: (6.5, 12.0),
        bank_deg: (2.0, 8.0),
        // E: more than a stadium round, on a lap with twice the ground to put it on.
        sand_sections: (1, 3),
        sand_m: (40.0, 80.0),
        // E: one or two stretches of parallel lanes, which is what makes it look like SMX
        // rather than like a small national.
        stadium_blocks: (1, 2),
    }),
};

/// Names, so two tracks from two seeds are never the same track twice. Each takes its own
/// folder inside its own `.pkz`, which is what stops one installing over another.
const NAMES: [&str; 15] = [
    "Ashgrove National",
    "Blackpine Park",
    "Cold Harbour MX",
    "Draycott Valley",
    "Eastmoor Raceway",
    "Fallow Hill MX",
    "Greystone National",
    "Hollowbrook Park",
    "Ironbark Raceway",
    "Juniper Flats MX",
    "Kestrel Ridge National",
    "Long Marsh Park",
    "Millbrook MX",
    "Northgate Raceway",
    "Oakhanger National",
];

const PLACES: [&str; 12] = [
    "Somerset", "Wexford", "Alberta", "Nord-Pas", "Lombardy", "Otago", "Jutland", "Kalmar",
    "Wallonia", "Aragon", "Limburg", "Waikato",
];

/// A small deterministic generator. Not for anything that has to be unguessable — this only
/// has to give the same track for the same number, on every machine.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// A float in `[lo, hi)`.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() >> 11) as f32 / (1u64 << 53) as f32 * (hi - lo)
    }
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1).max(1) as u64) as i32
    }
    fn chance(&mut self, p: f32) -> bool {
        self.range(0.0, 1.0) < p
    }
}

/// What a lap is made of. `scripts/track-survey.py` prints this off the tracks' own `.trh`
/// centrelines; a corner is same-handed turning under a 300 m radius with under ten metres of
/// run let into it — Motorcycling Australia's own definition of a curve.
///
/// ```text
///                     Indiana   Southwick   all 18
///   lap                 2170 m     2217 m   1065-3055
///   corners           16 (7.4)   18 (8.1)   7.4-10.3 per km
///   arcs in a corner         4          5   1-6
///   a corner's angle      159°       166°   90-166 (p50 136)
///   apex radius         10.4 m     11.9 m   7.1-18.5
///   ground per corner     68 m       74 m   14-82
///   run between them      27 m       30 m   20-72
/// ```
const CORNERS_PER_KM: (f32, f32) = (7.4, 10.3);

/// `crate::trackllm::corpus::LAP_M` refuses anything over 2600.
const LAP_MAX_M: f32 = 2550.0;

/// How much of a lap is tight enough to wear a rut. Indiana measures 8.4% under a 14 m
/// radius and Southwick 7.3%.
const TIGHT_SHARE: f32 = 0.10;

/// The longest straight a rider may meet — the FFM's, and the only one any federation writes.
const STRAIGHT_CAP_M: f32 = 125.0;

/// The ground between the corners, as share of lap length by radius. A published lap is almost
/// never straight — 2.9% of Indiana and 0.8% of Southwick — and a third of it drifts through a
/// radius over 300 m, which rides as a straight and measures as an arc. Draw that band too
/// tight and it reads as part of the corner beside it, which moves two statistics at once.
///
/// ```text
///                    Indiana   Southwick
///   a true straight     2.9%        0.8%
///   R 40-80 m          16.7%       14.7%
///   R 80-160           14.1%       13.6%
///   R 160-300          11.2%       13.4%
///   R 300-1000         18.1%       19.0%
///   R over 1000        15.9%        8.2%
/// ```
const WANDER: [(f32, f32, f32); 5] = [
    (40.0, 80.0, 0.21),
    (80.0, 160.0, 0.19),
    (160.0, 300.0, 0.17),
    (300.0, 1000.0, 0.26),
    (1000.0, 3000.0, 0.17),
];

/// How far the walk may run before it must be closing, and how near it may come to itself.
const MARGIN_M: f32 = 26.0;
/// The start spur stands beside the lap and needs ground of its own.
const GATE_ROOM_M: f32 = 78.0;

fn seg_length(s: &Segment) -> f32 {
    match s {
        Segment::Straight { length, .. } => *length,
        Segment::Arc { radius, angle, .. } => radius.abs() * angle.to_radians(),
    }
}

fn chain_length(segs: &[Segment]) -> f32 {
    segs.iter().map(seg_length).sum()
}

/// Where a segment leaves you, in the app's own frame.
fn advance(pose: (f32, f32, f32), s: &Segment) -> (f32, f32, f32) {
    let (x, z, h) = pose;
    match s {
        Segment::Straight { length, .. } => {
            let (hx, hz) = crate::trackprog::heading_vector(h);
            (x + hx * length, z + hz * length, h)
        }
        Segment::Arc { radius, angle, .. } => {
            let a = angle.to_radians() * if *radius > 0.0 { 1.0 } else { -1.0 };
            let nh = h + a;
            (
                x + radius * (h.cos() - nh.cos()),
                z + radius * (nh.sin() - h.sin()),
                nh,
            )
        }
    }
}

/// The ground a segment covers: `(x, z, how far along the segment)`.
///
/// Each point carries its own distance, not the segment's. Giving a whole segment one age is
/// what made every walk paint itself in on the first move: the piece just laid ends where the
/// next one starts, so if its far end is dated from its own beginning it is still "old" track
/// sitting right under the new piece, and nothing is ever legal.
fn samples(pose: (f32, f32, f32), s: &Segment, step: f32, out: &mut Vec<(f32, f32, f32)>) {
    match s {
        Segment::Straight { length, .. } => {
            let n = ((length / step) as usize).max(1);
            let (hx, hz) = crate::trackprog::heading_vector(pose.2);
            for k in 1..=n {
                let t = length * k as f32 / n as f32;
                out.push((pose.0 + hx * t, pose.1 + hz * t, t));
            }
        }
        Segment::Arc { radius, angle, .. } => {
            let ang = angle.to_radians();
            let run = radius.abs() * ang;
            let n = ((run / step) as usize).max(1);
            for k in 1..=n {
                let a = ang * k as f32 / n as f32 * if *radius > 0.0 { 1.0 } else { -1.0 };
                let nh = pose.2 + a;
                out.push((
                    pose.0 + radius * (pose.2.cos() - nh.cos()),
                    pose.1 + radius * (nh.sin() - pose.2.sin()),
                    run * k as f32 / n as f32,
                ));
            }
        }
    }
}

/// The four turn-straight-turn ways from one pose to another, shortest first.
///
/// Only the CSC families: with a lap this size the RLR and LRL cases only matter when the two
/// poses are almost on top of each other, and a lap that close to its own start has finished.
fn dubins(start: (f32, f32, f32), goal: (f32, f32, f32), r: f32) -> Vec<(f32, Vec<Segment>)> {
    let mut out: Vec<(f32, Vec<Segment>)> = Vec::new();
    for s1 in [1.0f32, -1.0] {
        for s2 in [1.0f32, -1.0] {
            let rv1 = crate::trackprog::right_vector(start.2);
            let rv2 = crate::trackprog::right_vector(goal.2);
            let c1 = (start.0 + s1 * r * rv1.0, start.1 + s1 * r * rv1.1);
            let c2 = (goal.0 + s2 * r * rv2.0, goal.1 + s2 * r * rv2.1);
            let (dx, dz) = (c2.0 - c1.0, c2.1 - c1.1);
            let d = dx.hypot(dz);
            if d < 1e-6 {
                continue;
            }
            let theta = dx.atan2(dz); // app frame: the heading of the line of centres
            let (tangent_h, run) = if s1 == s2 {
                // Same handedness: the straight is parallel to the line of centres.
                (theta, d)
            } else {
                // Opposite: the straight crosses between them, and only if they are far
                // enough apart to have a common internal tangent.
                if d < 2.0 * r {
                    continue;
                }
                let alpha = (2.0 * r / d).min(1.0).acos();
                (
                    theta + if s1 > 0.0 { alpha } else { -alpha },
                    (d * d - 4.0 * r * r).max(0.0).sqrt(),
                )
            };
            let a1 = ((tangent_h - start.2) * s1).rem_euclid(std::f32::consts::TAU);
            let a2 = ((goal.2 - tangent_h) * s2).rem_euclid(std::f32::consts::TAU);
            let mut segs = Vec::new();
            if a1 > 1e-4 {
                segs.push(Segment::Arc { radius: r * s1, angle: a1.to_degrees(), rise: 0.0 });
            }
            if run > 0.5 {
                segs.push(Segment::Straight { length: run, rise: 0.0 });
            }
            if a2 > 1e-4 {
                segs.push(Segment::Arc { radius: r * s2, angle: a2.to_degrees(), rise: 0.0 });
            }
            out.push((a1 * r + run + a2 * r, segs));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Which parts of the plot the lap has been near, on a coarse grid.
///
/// The walk used to be scored by how much open ground lay ahead of it, and the most open
/// ground is always the perimeter — so it followed the boundary round and left the middle
/// empty, which from above is a giant ring. Rewarding *new ground covered* instead makes it
/// fill the plot.
struct Cover {
    cell: f32,
    n: usize,
    seen: Vec<bool>,
}

impl Cover {
    fn new(plot: f32) -> Self {
        let cell = 26.0;
        let n = (plot / cell) as usize + 1;
        Cover { cell, n, seen: vec![false; n * n] }
    }

    fn index(&self, x: f32, z: f32) -> Option<usize> {
        if x < 0.0 || z < 0.0 {
            return None;
        }
        let (c, r) = ((x / self.cell) as usize, (z / self.cell) as usize);
        (c < self.n && r < self.n).then_some(r * self.n + c)
    }

    /// How many cells this piece would visit that nothing has visited yet.
    fn fresh(&self, pts: &[(f32, f32, f32)]) -> usize {
        let mut hit: Vec<usize> = Vec::new();
        for (x, z, _) in pts {
            if let Some(i) = self.index(*x, *z) {
                if !self.seen[i] && !hit.contains(&i) {
                    hit.push(i);
                }
            }
        }
        hit.len()
    }

    /// Mark the ground, and hand back what to unmark if the move is taken back.
    fn add(&mut self, pts: &[(f32, f32, f32)]) -> Vec<usize> {
        let mut marked = Vec::new();
        for (x, z, _) in pts {
            if let Some(i) = self.index(*x, *z) {
                if !self.seen[i] {
                    self.seen[i] = true;
                    marked.push(i);
                }
            }
        }
        marked
    }

    fn undo(&mut self, marked: &[usize]) {
        for i in marked {
            self.seen[*i] = false;
        }
    }
}

/// What the lap has used, on a coarse grid, so "is this clear" is cheap.
struct Ground {
    cell: f32,
    n: usize,
    plot: f32,
    used: Vec<Vec<(f32, f32, f32)>>,
}

impl Ground {
    fn new(plot: f32) -> Self {
        let cell = 8.0;
        let n = (plot / cell) as usize + 1;
        Ground { cell, n, plot, used: vec![Vec::new(); n * n] }
    }

    fn cells(&self, x: f32, z: f32, reach: f32) -> impl Iterator<Item = usize> + '_ {
        let c0 = ((x - reach) / self.cell).max(0.0) as usize;
        let c1 = (((x + reach) / self.cell) as usize).min(self.n - 1);
        let r0 = ((z - reach) / self.cell).max(0.0) as usize;
        let r1 = (((z + reach) / self.cell) as usize).min(self.n - 1);
        let n = self.n;
        (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| r * n + c))
    }

    /// Lay ground, and hand back how much each cell grew by so it can be taken back.
    fn add(&mut self, pts: &[(f32, f32, f32)], at: f32) -> Vec<(usize, usize)> {
        let mut grew: Vec<(usize, usize)> = Vec::new();
        for (x, z, along) in pts {
            if *x < 0.0 || *z < 0.0 {
                continue;
            }
            let i = (*z / self.cell) as usize * self.n + (*x / self.cell) as usize;
            if i < self.used.len() {
                self.used[i].push((*x, *z, at + along));
                match grew.iter_mut().find(|(c, _)| *c == i) {
                    Some(e) => e.1 += 1,
                    None => grew.push((i, 1)),
                }
            }
        }
        grew
    }

    fn undo(&mut self, grew: &[(usize, usize)]) {
        for (i, k) in grew {
            let keep = self.used[*i].len() - k;
            self.used[*i].truncate(keep);
        }
    }

    /// Is anything already laid within `clear` of here? Stops at the first one.
    ///
    /// `nearest` measures, which costs a sweep of every cell inside sixty metres — 225 of
    /// them — when the walk only ever asks whether one point is too close. This looks at 25.
    ///
    /// Both bounds are ages round the lap. `ignore_after` is the last thirty metres, because a
    /// corner may come close to the run that fed it. `ignore_before` is for the way home: a
    /// lap arriving back at its gate necessarily runs up beside the straight it left on.
    fn blocked(&self, x: f32, z: f32, ignore_after: f32, ignore_before: f32, clear: f32) -> bool {
        for i in self.cells(x, z, clear) {
            for (px, pz, age) in &self.used[i] {
                if *age > ignore_after || *age < ignore_before {
                    continue;
                }
                if (px - x) * (px - x) + (pz - z) * (pz - z) < clear * clear {
                    return true;
                }
            }
        }
        false
    }

    fn nearest(&self, x: f32, z: f32, ignore_after: f32) -> f32 {
        let reach = 60.0;
        let mut best = reach;
        for i in self.cells(x, z, reach) {
            for (px, pz, age) in &self.used[i] {
                if *age > ignore_after {
                    continue;
                }
                let d = (px - x).hypot(pz - z);
                if d < best {
                    best = d;
                }
            }
        }
        best
    }

    /// How open the ground is here — used to steer towards what has not been ridden.
    fn room(&self, x: f32, z: f32, ignore_after: f32) -> f32 {
        let edge = x.min(z).min(self.plot - x).min(self.plot - z);
        self.nearest(x, z, ignore_after).min(edge)
    }
}

/// A piece of the ground between corners: a radius off the corpus, and 15 to 45 m of it.
///
/// Drawn as a length with the angle following, because a fixed angle band makes a 500 m
/// sweeper 200 m long. Indiana's segments average eighteen metres.
fn a_wander(rng: &mut Rng) -> Vec<Segment> {
    let roll = rng.range(0.0, 1.0);
    let mut acc = 0.0;
    let mut band = WANDER[WANDER.len() - 1];
    for w in WANDER {
        acc += w.2;
        if roll <= acc {
            band = w;
            break;
        }
    }
    let r = rng.range(band.0.ln(), band.1.ln()).exp();
    let run = rng.range(15.0, 45.0);
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    vec![Segment::Arc {
        radius: r * side,
        angle: (run / r).to_degrees(),
        rise: 0.0,
    }]
}

/// One corner, built the way a real one is: in on a loosening arc, round, and out.
///
/// A published corner is not one arc — Indiana's median is four and Southwick's five — which
/// is why it covers seventy metres of ground turning through 160 degrees where a lone arc of
/// the same apex radius covers thirty. Offered one arc at a time the walk could only reach the
/// corpus's tight radii with a hairpin, and a same-handed wander landing beside it read as one
/// corner of 197 degrees.
fn a_corner(rng: &mut Rng, knobs: &LayoutKnobs) -> Vec<Segment> {
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    // A 180, where the discipline is built out of them. SuperMotocross wants 7–10 hairpins of
    // its 12–22 corners and the three arcs below only sum past 170 on about one roll in
    // twelve, so waiting for them to happen never draws the shape.
    //
    // Short-circuited on the share itself, so a motocross seed makes no roll here at all and
    // draws exactly the sequence of numbers it always has.
    if knobs.rules().hairpin_share > 0.0 && rng.chance(knobs.rules().hairpin_share) {
        return vec![
            Segment::Arc { radius: side * rng.range(22.0, 40.0), angle: rng.range(18.0, 32.0), rise: 0.0 },
            Segment::Arc { radius: side * rng.range(knobs.apex.0, knobs.apex.1), angle: rng.range(120.0, 150.0), rise: 0.0 },
            Segment::Arc { radius: side * rng.range(16.0, 30.0), angle: rng.range(14.0, 26.0), rise: 0.0 },
        ];
    }
    if rng.chance(knobs.sweep_share) {
        // Not every corner is a hairpin: a fifth of the corpus's are a single sweep.
        return vec![Segment::Arc {
            radius: side * rng.range(15.0, 32.0),
            angle: rng.range(40.0, 95.0),
            rise: 0.0,
        }];
    }
    vec![
        Segment::Arc { radius: side * rng.range(26.0, 60.0), angle: rng.range(22.0, 50.0), rise: 0.0 },
        Segment::Arc { radius: side * rng.range(knobs.apex.0, knobs.apex.1), angle: rng.range(55.0, 105.0), rise: 0.0 },
        Segment::Arc { radius: side * rng.range(18.0, 45.0), angle: rng.range(18.0, 45.0), rise: 0.0 },
    ]
}

/// Is this chain a corner, or the ground between two of them?
fn is_corner(chain: &[Segment]) -> bool {
    chain.len() > 1
        || matches!(chain.first(), Some(Segment::Arc { angle, .. }) if *angle >= 40.0)
}

/// The longest unbroken straight a rider meets, metres — merged the way the app merges.
///
/// Mirrors `Station::straight_runs`: consecutive straights are one straight, and so is the
/// pair either side of the finish line, because a lap that ends on a straight and begins on
/// one runs through the line without a corner in it.
fn longest_straight(segs: &[Segment], opening: f32) -> f32 {
    let mut runs = Vec::new();
    let mut run = 0.0;
    for s in segs {
        match s {
            Segment::Straight { length, .. } => run += length,
            _ => {
                if run > 0.0 {
                    runs.push(run);
                }
                run = 0.0;
            }
        }
    }
    // Through the line: whatever the lap ends on, plus the opening straight it rejoins.
    runs.push(run + opening);
    runs.into_iter().fold(0.0f32, f32::max)
}

/// Gentler than this and an arc counts as ground between corners, see [`Rules::run_max_m`].
const GENTLE_R_M: f32 = 60.0;
/// The same on the way home, a little looser or most laps never close. Long enough for a jump,
/// never long enough to be bent into a chicane (see [`STRAIGHT_MAX_M`]).
const HOME_STRETCH_MAX_M: f32 = 70.0;

/// The longest stretch between corners, metres, leaving out the start straight and the run into
/// the finish that joins it.
fn longest_stretch(segs: &[Segment]) -> f32 {
    let gentle = |s: &Segment| match s {
        Segment::Straight { .. } => true,
        Segment::Arc { radius, .. } => radius.abs() >= GENTLE_R_M,
    };
    let mut runs = Vec::new();
    let mut run = 0.0;
    let mut leading = true;
    for s in segs {
        if gentle(s) {
            run += seg_length(s);
        } else {
            if !leading && run > 0.0 {
                runs.push(run);
            }
            leading = false;
            run = 0.0;
        }
    }
    runs.into_iter().fold(0.0f32, f32::max)
}

/// One move on the lap, and what it would take to undo it.
struct Step {
    pose: (f32, f32, f32),
    laid: f32,
    n: usize,
    cands: Vec<Vec<Segment>>,
    cursor: usize,
    ground: Vec<(usize, usize)>,
    cover: Vec<usize>,
    since: f32,
    corners: u32,
    tight: f32,
    /// Whether the lap had already spent its one long stretch — see [`Rules::long_run_m`].
    long: bool,
}

/// Walk a lap out of the ground and dock it back onto its own start.
///
/// Each step offers the same handful of moves — a run, a gentle wander, or a whole corner
/// either way — and the one taken is whichever leaves the track in the most open ground while
/// clearing everything already laid. When the budget runs down, the shortest Dubins path back
/// to the start pose that is also clear closes the lap exactly, which is why there is no
/// return leg to route round anything.
///
/// And it takes moves back. Greedy, sixty seeds gave one lap and the other fifty-nine ran out
/// of legal moves 150 to 900 m in, because the first dead end was final. The moves live on a
/// stack, and a dead end pops one and takes the next-best instead.
fn walk(
    rng: &mut Rng,
    plot: f32,
    width: f32,
    want_m: f32,
    knobs: &LayoutKnobs,
) -> Option<(Vec<Segment>, Start)> {
    let clear = width + 5.0;
    // Drawn once: redrawn at every step it averages to the middle and the noise decides.
    let want_rate = rng.range(knobs.corners_per_km.0, knobs.corners_per_km.1) / 1000.0;
    let start = (plot * 0.5, MARGIN_M + GATE_ROOM_M, 0.0); // facing +z, up the plot
    let mut ground = Ground::new(plot);
    let mut cover = Cover::new(plot);
    let mut segs: Vec<Segment> = Vec::new();
    let mut pts = Vec::new();

    // The first stretch is the start straight, and nothing may be built on it.
    let opening = Segment::Straight {
        length: rng.range(knobs.start_straight.0, knobs.start_straight.1),
        rise: 0.0,
    };
    samples(start, &opening, 3.0, &mut pts);
    ground.add(&pts, 0.0);
    cover.add(&pts);
    let mut pose = advance(start, &opening);
    let mut laid = seg_length(&opening);
    let opening_m = laid;
    segs.push(opening);

    let cap = (want_m * 1.25).min(LAP_MAX_M);
    let mut stack: Vec<Step> = Vec::new();
    let mut budget = 9000u32;
    // The start straight counts: it runs straight into turn one.
    let (mut since_corner, mut corners_laid, mut tight_m) = (opening_m, 0u32, 0.0f32);
    // And whether the one long stretch a lap is allowed has been spent. The start straight can
    // be it: on a discipline with no long run the two caps are equal and this is never true.
    let mut long_used = opening_m > knobs.rules().run_max_m;
    let mut node: Option<(Vec<Vec<Segment>>, usize)> = None;

    while budget > 0 {
        if node.is_none() {
            if laid > want_m * 0.86 {
                if let Some(home) = home_from(
                    rng, &ground, plot, clear, start, pose, laid, &segs, opening_m, knobs,
                ) {
                    segs.extend(home);
                    return Some((
                        segs,
                        Start {
                            x: start.0,
                            z: start.1,
                            angle: start.2.to_degrees().rem_euclid(360.0),
                        },
                    ));
                }
            }
            let cands = if laid < cap {
                offers(rng, &ground, &cover, &segs, pose, laid, since_corner, corners_laid,
                       tight_m, long_used, want_rate, knobs)
            } else {
                Vec::new()
            };
            node = Some((cands, 0));
        }
        budget -= 1;

        // Take the best move left here that is actually clear. Ignore the last thirty metres
        // of track when checking: a corner is allowed to come close to the run that fed it.
        let (cands, cursor) = node.as_mut().unwrap();
        let mut chain: Option<Vec<Segment>> = None;
        while *cursor < cands.len() {
            let c = cands[*cursor].clone();
            *cursor += 1;
            if legal(&ground, plot, clear, pose, &c, laid - 34.0, 0.0) {
                chain = Some(c);
                break;
            }
        }

        let Some(chain) = chain else {
            // Painted in. Take the last move back and try this node's next-best instead.
            let back = stack.pop()?;
            ground.undo(&back.ground);
            cover.undo(&back.cover);
            segs.truncate(segs.len() - back.n);
            pose = back.pose;
            laid = back.laid;
            since_corner = back.since;
            corners_laid = back.corners;
            tight_m = back.tight;
            long_used = back.long;
            node = Some((back.cands, back.cursor));
            continue;
        };

        let (cands, cursor) = node.take().unwrap();
        let mut grew = Vec::new();
        let mut marked = Vec::new();
        let (mut p, mut at) = (pose, laid);
        for seg in &chain {
            pts.clear();
            samples(p, seg, 3.0, &mut pts);
            for (i, k) in ground.add(&pts, at) {
                match grew.iter_mut().find(|(c, _): &&mut (usize, usize)| *c == i) {
                    Some(e) => e.1 += k,
                    None => grew.push((i, k)),
                }
            }
            marked.extend(cover.add(&pts));
            at += seg_length(seg);
            p = advance(p, seg);
        }
        stack.push(Step {
            pose,
            laid,
            n: chain.len(),
            cands,
            cursor,
            ground: grew,
            cover: marked,
            since: since_corner,
            corners: corners_laid,
            tight: tight_m,
            long: long_used,
        });
        let run = chain_length(&chain);
        if is_corner(&chain) {
            corners_laid += 1;
            since_corner = 0.0;
            tight_m += chain
                .iter()
                .filter(|s| matches!(s, Segment::Arc { radius, .. } if radius.abs() < 14.0))
                .map(|s| seg_length(s))
                .sum::<f32>();
        } else {
            since_corner += run;
            // The moment a stretch runs past the ordinary cap, the lap's one long run is spent.
            if since_corner > knobs.rules().run_max_m {
                long_used = true;
            }
        }
        pose = p;
        laid += run;
        segs.extend(chain);
    }
    None
}

/// Would this chain of segments fit on ground nothing has used?
#[allow(clippy::too_many_arguments)]
fn legal(
    ground: &Ground,
    plot: f32,
    clear: f32,
    from: (f32, f32, f32),
    chain: &[Segment],
    age_cut: f32,
    skip_before: f32,
) -> bool {
    let mut p = from;
    let mut pts = Vec::new();
    for seg in chain {
        pts.clear();
        samples(p, seg, 2.5, &mut pts);
        for (x, z, _) in &pts {
            if !(MARGIN_M..=plot - MARGIN_M).contains(x)
                || !(MARGIN_M..=plot - MARGIN_M).contains(z)
            {
                return false;
            }
            if ground.blocked(*x, *z, age_cut, skip_before, clear) {
                return false;
            }
        }
        p = advance(p, seg);
    }
    true
}

/// Every move worth trying here, best first.
///
/// Scored on ground it would be the first to visit — per metre travelled, not per move,
/// because rewarding raw coverage buys it with long straights, and a lap of long runs with
/// angles between them is not a track.
#[allow(clippy::too_many_arguments)]
fn offers(
    rng: &mut Rng,
    ground: &Ground,
    cover: &Cover,
    segs: &[Segment],
    pose: (f32, f32, f32),
    laid: f32,
    since_corner: f32,
    corners_laid: u32,
    tight_m: f32,
    long_used: bool,
    want_rate: f32,
    knobs: &LayoutKnobs,
) -> Vec<Vec<Segment>> {
    let mut running = 0.0;
    for prev in segs.iter().rev() {
        match prev {
            Segment::Straight { length, .. } => running += length,
            _ => break,
        }
    }
    let mut cand: Vec<Vec<Segment>> = Vec::new();
    // A straight, unless the lap is already on one long enough. Consecutive straights are
    // colinear, so the app's `straight_runs` reads a row of them as ONE straight.
    // And never a long stretch between corners: straights and wanders chained with no limit
    // rode as long straights, and the longer ones came back as big chicanes.
    let rules = knobs.rules();
    // One stretch a lap may run long, where the discipline allows one — Charlotte's 351 m side
    // against nothing else on that lap over sixty. Once it is spent every run is back to the
    // ordinary cap, and on a discipline with no long run the two are the same number, so this
    // is the line motocross has always drawn.
    let cap = if long_used { rules.run_max_m } else { rules.long_run_m };
    let left = cap - since_corner;
    let room = (rules.straight_cap_m - running).min(left).min(rules.home_run_max_m);
    if room > 20.0 {
        cand.push(vec![Segment::Straight { length: rng.range(20.0, room), rise: 0.0 }]);
    }
    // The lap's own wander, and most of the ground between corners.
    for _ in 0..7 {
        let w = a_wander(rng);
        if is_corner(&w) || chain_length(&w) <= left {
            cand.push(w);
        }
    }
    // And corners, whole — but only once the lap has run far enough since the last one for the
    // two to be told apart, or a same-handed pair reads as one corner of twice the angle. The
    // corpus's p50 run between corners is 27 to 30 m.
    if since_corner >= 20.0 {
        for _ in 0..4 {
            cand.push(a_corner(rng, knobs));
        }
    }

    let behind = (corners_laid as f32) < laid * want_rate;
    let mut scored: Vec<(f32, Vec<Segment>)> = Vec::with_capacity(cand.len());
    let mut pts = Vec::new();
    for chain in cand {
        let run_m = chain_length(&chain);
        pts.clear();
        let mut p = pose;
        for seg in &chain {
            samples(p, seg, 6.0, &mut pts);
            p = advance(p, seg);
        }
        let mut score = 190.0 * cover.fresh(&pts) as f32 / run_m.max(1.0);
        let (hx, hz) = crate::trackprog::heading_vector(p.2);
        score += 0.35 * ground.room(p.0 + hx * 26.0, p.1 + hz * 26.0, laid - 34.0);
        if is_corner(&chain) {
            // Corners are the point — but eight a kilometre, not as many as will fit.
            score += if behind { 34.0 } else { -22.0 };
            let apex = chain
                .iter()
                .map(|s| match s {
                    Segment::Arc { radius, .. } => radius.abs(),
                    _ => f32::MAX,
                })
                .fold(f32::MAX, f32::min);
            if apex < 14.0 && tight_m < laid * TIGHT_SHARE {
                score += 20.0;
            }
        }
        scored.push((score, chain));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// A way back onto the start pose that lands, is clear, and breaks no rule.
#[allow(clippy::too_many_arguments)]
fn home_from(
    rng: &mut Rng,
    ground: &Ground,
    plot: f32,
    clear: f32,
    start: (f32, f32, f32),
    pose: (f32, f32, f32),
    laid: f32,
    segs: &[Segment],
    opening: f32,
    knobs: &LayoutKnobs,
) -> Option<Vec<Segment>> {
    // Several radii, not one. A Dubins path is fixed by the radius you give it, so a single
    // draw is a single shape: with the gate exemption tightened to an age, one shape closed on
    // two seeds in three and the rest walked on to the cap, which showed up as 12.6 corners a
    // kilometre. Five radii is five shapes, and the shortest that clears wins.
    let mut ways: Vec<(f32, Vec<Segment>)> = Vec::new();
    for r in [14.0f32, 18.0, 23.0, 29.0, 36.0] {
        ways.extend(dubins(pose, start, r * rng.range(0.92, 1.08)));
    }
    ways.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, home) in ways {
        // Walked, not trusted: one of the four families has a sign in it that only bites on
        // some geometries, and the symptom is a lap that misses itself by thirty metres.
        let landed = home.iter().fold(pose, |p, s| advance(p, s));
        if (landed.0 - start.0).hypot(landed.1 - start.1) > 0.25 {
            continue;
        }
        // The way home may run up beside the start straight — that is where it is going — so
        // the opening straight is not an obstacle to it, and nothing else is excused.
        //
        // This used to be a disc round the start pose, which excused every *other* piece of
        // lap that happened to pass through the disc too: `review` caught a way home running
        // 12 m from a mid-lap straight on a 15 m track. Ages are exact where a radius is not.
        if !legal(ground, plot, clear, pose, &home, laid - 34.0, opening) {
            continue;
        }
        // The way home is part of the lap and its own length counts: laps came out at 2627 and
        // 2820 m because the walk stopped at the cap and Dubins added on top of it.
        if laid + chain_length(&home) > LAP_MAX_M {
            continue;
        }
        // And no straight on the finished lap may run past the FFM's 125 m. Checked on the
        // whole lap: consecutive straights are colinear so `straight_runs` reads a row of them
        // as one, and a Dubins path's straight sits in its middle.
        let mut all: Vec<Segment> = segs.to_vec();
        all.extend(home.iter().copied());
        if longest_straight(&all, opening) > knobs.rules().straight_cap_m {
            continue;
        }
        // Nor a long stretch on the way home: one became two big chicanes before the finish.
        if longest_stretch(&all) > knobs.rules().home_run_max_m {
            continue;
        }
        if home
            .iter()
            .all(|s| !matches!(s, Segment::Arc { angle, .. } if *angle >= 200.0))
        {
            return Some(home);
        }
    }
    None
}

/// Where each segment sits round the lap, and how tight it is there.
fn spans(segs: &[Segment]) -> (f32, Vec<(f32, f32, Option<f32>)>) {
    let mut at = 0.0;
    let mut out = Vec::with_capacity(segs.len());
    for s in segs {
        let run = match s {
            Segment::Straight { length, .. } => *length,
            Segment::Arc { radius, angle, .. } => radius.abs() * angle.to_radians(),
        };
        out.push((
            at,
            at + run,
            match s {
                Segment::Straight { .. } => None,
                Segment::Arc { radius, .. } => Some(radius.abs()),
            },
        ));
        at += run;
    }
    (at, out)
}

/// How much run a jump's own faces take, metres — through the app's own arithmetic.
///
/// `scripts/track-walk.py` mirrors this by hand and nothing enforces the mirror, which is how
/// it came to hold a 27 degree face angle after `JUMP_FACE_DEG` moved to 38. Here it is the
/// same call the builder makes.
fn faces(height: f32) -> f32 {
    lip_run(height) + landing_run(height)
}

fn lip_run(height: f32) -> f32 {
    crate::trackprog::face_run(
        height,
        crate::trackprog::JUMP_FACE_DEG,
        crate::trackprog::JUMP_FACE_MIN_M,
    )
}

/// A take-off meant to be jumped: see [`crate::trackprog::air_face_run`].
fn air_run(height: f32) -> f32 {
    crate::trackprog::air_face_run(height)
}

fn landing_run(height: f32) -> f32 {
    crate::trackprog::face_run(
        height,
        crate::trackprog::JUMP_LANDING_DEG,
        crate::trackprog::JUMP_LANDING_MIN_M,
    )
}

/// Clear ground a jump wants before its face, off the last tight corner, metres: a run-up.
const JUMP_RUNUP_M: f32 = 20.0;

/// Jumps, placed by distance round the lap rather than by which segment they land on.
///
/// Published tracks put them everywhere: Indiana is 109 arcs to 11 straights and still carries
/// between twelve and forty-five lips per kilometre, so most of its jumps are in or beside
/// corners. Placing them on straights only leaves a lap made of corners — which is what a lap
/// is — with almost nothing built on it: five per kilometre, measured.
/// No straight longer than [`STRAIGHT_MAX_M`] past the opening one: the middle of a longer one
/// becomes an S — a bend, one back twice as far and a bend again — which leaves the lap on the
/// same line and heading, so it still closes. "Limit the straights to a third of that one."
fn break_long_straights(segs: &mut Vec<Segment>, start: (f32, f32, f32), width: f32, cap: f32) {
    let wiggle = |length: f32, rise: f32, side: f32| -> Vec<Segment> {
        let chord = length - 2.0 * WIGGLE_END_M;
        // No tighter than a sweeper: at a fixed 25° a 53 m straight became a 19 m chicane that
        // rode as a straight and kept every jump off it.
        let lo = WIGGLE_MIN_DEG.to_radians().sin();
        let hi = WIGGLE_DEG.to_radians().sin();
        let theta = (chord / (4.0 * WIGGLE_MIN_R_M)).clamp(lo, hi).asin();
        let (r, deg) = (chord / (4.0 * theta.sin()), theta.to_degrees());
        let share = |l: f32| rise * l / length;
        vec![
            Segment::Straight { length: WIGGLE_END_M, rise: share(WIGGLE_END_M) },
            Segment::Arc { radius: r * side, angle: deg, rise: share(chord * 0.25) },
            Segment::Arc { radius: -r * side, angle: deg * 2.0, rise: share(chord * 0.5) },
            Segment::Arc { radius: r * side, angle: deg, rise: share(chord * 0.25) },
            Segment::Straight { length: WIGGLE_END_M, rise: share(WIGGLE_END_M) },
        ]
    };
    // Either way round, whichever brings the lap no nearer another leg of itself; a straight
    // with no room to bend either way stays as it is.
    // Not the last either: it runs into the finish and on down the start straight.
    let mut i = 1;
    while i + 1 < segs.len() {
        if let Segment::Straight { length, rise } = segs[i] {
            if length > cap {
                let before = near_misses(segs, start, width);
                let pick = [1.0f32, -1.0].into_iter().find_map(|side| {
                    let mut trial = segs.clone();
                    trial.splice(i..=i, wiggle(length, rise, side));
                    (near_misses(&trial, start, width) <= before).then_some(trial)
                });
                if let Some(trial) = pick {
                    *segs = trial;
                    i += 5;
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// How many pairs of points on the lap, far apart along it, stand closer than a track's width
/// plus a margin: the lap running over its own ground.
fn near_misses(segs: &[Segment], start: (f32, f32, f32), width: f32) -> usize {
    let mut pts: Vec<(f32, f32, f32)> = Vec::new();
    let mut pose = start;
    let mut run = 0.0f32;
    for s in segs {
        let from = pts.len();
        samples(pose, s, 3.0, &mut pts);
        for p in &mut pts[from..] {
            p.2 += run;
        }
        run += seg_length(s);
        pose = advance(pose, s);
    }
    let clear = (width + 3.0).powi(2);
    let mut n = 0;
    for a in 0..pts.len() {
        for b in a + 1..pts.len() {
            let along = (pts[b].2 - pts[a].2).abs();
            if along.min(run - along) < width * 4.0 {
                continue;
            }
            if (pts[a].0 - pts[b].0).powi(2) + (pts[a].1 - pts[b].1).powi(2) < clear {
                n += 1;
            }
        }
    }
    n
}

/// The longest straight past the opening one, and the S a longer one is turned into.
const STRAIGHT_MAX_M: f32 = HOME_STRETCH_MAX_M;
// A real chicane, 45° each way: at 25° and a 45 m radius the S rode as the straight it was.
const WIGGLE_DEG: f32 = 45.0;
const WIGGLE_MIN_DEG: f32 = 8.0;
const WIGGLE_MIN_R_M: f32 = 25.0;
const WIGGLE_END_M: f32 = 10.0;

/// Singles on one side of the track, in the stretches the draw left empty: a choice of line
/// down a straight rather than nothing. They take nothing from the layout's own generator, so
/// the rest of a seed's lap is the same with them.
fn side_singles(out: &mut Vec<Feature>, segs: &[Segment], seed: u64, rules: &Rules) {
    let (total, spans) = spans(segs);
    let tight = |at: f32, length: f32| {
        spans
            .iter()
            .any(|(a, b, r)| r.is_some_and(|r| r < rules.tight_m) && *b > at && *a < at + length)
    };
    // A grade is ground, not a feature: straights carrying only a step rode as empty.
    let mut taken: Vec<(f32, f32)> = out
        .iter()
        .filter(|f| !matches!(f, Feature::StepUp { .. } | Feature::Berm { .. } | Feature::Rut { .. }))
        .map(|f| (f.at(), f.at() + f.length()))
        .collect();
    taken.sort_by(|a, b| a.0.total_cmp(&b.0));
    taken.push((total - 20.0, total));
    let pick = |pos: f32| {
        let x = (seed ^ ((pos * 8.0) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_mul(0xBF58_476D_1CE4_E5B9);
        (((x >> 11) as f64 / (1u64 << 53) as f64) as f32, (x >> 7) & 1 == 1)
    };
    let mut added = Vec::new();
    let mut from = rules.start_clear_m;
    for (a, b) in taken {
        let mut pos = from + SIDE_SINGLE_CLEAR_M;
        loop {
            let (frac, right) = pick(pos);
            let h = SIDE_SINGLE_H.0 + (SIDE_SINGLE_H.1 - SIDE_SINGLE_H.0) * frac;
            let (up, down) = (air_run(h), landing_run(h));
            let span = up + SIDE_SINGLE_CREST_M + down;
            if pos + span + SIDE_SINGLE_CLEAR_M > a {
                break;
            }
            if tight(pos - 10.0, span + 15.0)
                || !single_fits(&spans, pos, span)
                || single_near(out, pos, span)
                || single_near(&added, pos, span)
            {
                pos += 4.0;
                continue;
            }
            let marks = [(0.0, 0.0), (up, h), (up + SIDE_SINGLE_CREST_M, h), (span, 0.0)];
            added.push(Feature::Custom {
                at: pos,
                length: span,
                shape: marks
                    .iter()
                    .map(|(m, v)| crate::trackprog::ShapePoint { u: m / span, h: *v })
                    .collect(),
                side: if right { 1.0 } else { -1.0 },
            });
            pos += span + SIDE_SINGLE_GAP_M;
        }
        from = from.max(b);
    }
    out.extend(added);
    out.sort_by(|x, y| x.at().total_cmp(&y.at()));
}

/// Rollers in any stretch the jumps left bare: a long sweeper between two tight corners rode as
/// a straight with nothing on it. Keyed on position, like the side singles.
fn fill_gaps(out: &mut Vec<Feature>, segs: &[Segment], seed: u64, rules: &Rules) {
    let (total, spans) = spans(segs);
    let hairpin = |at: f32, len: f32| {
        spans
            .iter()
            .any(|(a, b, r)| r.is_some_and(|r| r.abs() < FILL_HAIRPIN_M) && *b > at && *a < at + len)
    };
    // A grade is ground, not a feature: straights carrying only a step rode as empty.
    let mut taken: Vec<(f32, f32)> = out
        .iter()
        .filter(|f| !matches!(f, Feature::StepUp { .. } | Feature::Berm { .. } | Feature::Rut { .. }))
        .map(|f| (f.at(), f.at() + f.length()))
        .collect();
    taken.sort_by(|a, b| a.0.total_cmp(&b.0));
    taken.push((total - 20.0, total));
    let mut added = Vec::new();
    let mut from = rules.start_clear_m;
    for (a, b) in taken {
        if a - from > FILL_GAP_M {
            let mut pos = from + FILL_CLEAR_M;
            while pos + FILL_ROLLER_M + FILL_CLEAR_M <= a {
                if hairpin(pos, FILL_ROLLER_M) {
                    pos += 3.0;
                    continue;
                }
                let x = (seed ^ ((pos * 8.0) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                    .wrapping_mul(0xBF58_476D_1CE4_E5B9);
                let frac = ((x >> 11) as f64 / (1u64 << 53) as f64) as f32;
                // Rollers and, now and then, a small single: an empty chicane rode as nothing.
                // Never near another single: one every 10 m rode as "a lot of singles".
                if frac > 0.55 {
                    let h = 1.1 + 0.5 * frac;
                    let (up, down) = (air_run(h), landing_run(h));
                    let span = up + 1.2 + down;
                    let lone = !single_near(out, pos, span) && !single_near(&added, pos, span);
                    if lone && pos + span + FILL_CLEAR_M <= a && !hairpin(pos, span) {
                        let marks = [(0.0, 0.0), (up, h), (up + 1.2, h), (span, 0.0)];
                        added.push(Feature::Custom {
                            at: pos,
                            length: span,
                            shape: marks.iter().map(|(m, v)| crate::trackprog::ShapePoint { u: m / span, h: *v }).collect(),
                            side: 0.0,
                        });
                        pos += span + FILL_SPACING_M;
                        continue;
                    }
                }
                // Tall enough to ride: at 0.5–0.8 m a chicane full of them rode as empty.
                added.push(Feature::Roller { at: pos, length: FILL_ROLLER_M, height: FILL_ROLLER_H.0 + (FILL_ROLLER_H.1 - FILL_ROLLER_H.0) * frac });
                pos += FILL_ROLLER_M + FILL_SPACING_M;
            }
        }
        from = from.max(b);
    }
    out.extend(added);
    out.sort_by(|x, y| x.at().total_cmp(&y.at()));
}

/// The bare stretch worth filling, the ground kept clear either end, a roller's length, the
/// gap between two, and the tightest bend one may sit on.
const FILL_GAP_M: f32 = 35.0;
/// At most this many wave sections to a lap: they were everywhere.
const MAX_WAVE_SECTIONS: usize = 2;
const FILL_CLEAR_M: f32 = 8.0;
const FILL_ROLLER_M: f32 = 12.0;
const FILL_SPACING_M: f32 = 10.0;
const FILL_HAIRPIN_M: f32 = 15.0;
const FILL_ROLLER_H: (f32, f32) = (0.9, 1.19);

/// The least clear ground between two singles, metres.
const SINGLE_APART_M: f32 = 50.0;

/// Whether a single (not a wave run) stands within [`SINGLE_APART_M`] of `at..at + len`.
fn single_near(fs: &[Feature], at: f32, len: f32) -> bool {
    fs.iter().any(|f| {
        matches!(f, Feature::Custom { .. })
            && f.height() >= 1.2
            && f.at() < at + len + SINGLE_APART_M
            && f.at() + f.length() > at - SINGLE_APART_M
    })
}

/// Whether the lap turns little enough under a jump that one built along its straight line
/// still lands on the track: the stray of an arc over it, `len × turning / 8`, stays under a
/// few metres. Only a real bend under a jump fails this, not every turn near one.
fn flies_straight(spans: &[(f32, f32, Option<f32>)], at: f32, len: f32) -> bool {
    let turning: f32 = spans
        .iter()
        .filter_map(|(a, b, r)| {
            let r = (*r)?;
            let over = (b.min(at + len) - a.max(at)).max(0.0);
            Some(over / r.abs().max(1.0))
        })
        .sum();
    len * turning / 8.0 <= FLIGHT_MAX_SAG_M
}
const FLIGHT_MAX_SAG_M: f32 = 4.0;

/// Whether a single at `at` running `len` metres sits before a turn and not straight out of one:
/// singles thrown in after every corner had riders launching everything everywhere.
fn single_fits(spans: &[(f32, f32, Option<f32>)], at: f32, len: f32) -> bool {
    let turn = |r: &Option<f32>| r.is_some_and(|r| r.abs() < SINGLE_TURN_M);
    let ahead = spans
        .iter()
        .filter(|(a, _, r)| turn(r) && *a > at)
        .map(|(a, ..)| a - (at + len))
        .fold(f32::INFINITY, f32::min);
    let behind = spans
        .iter()
        .filter(|(_, b, r)| turn(r) && *b <= at)
        .map(|(_, b, _)| at - b)
        .fold(f32::INFINITY, f32::min);
    (SINGLE_BEFORE_TURN_M.0..=SINGLE_BEFORE_TURN_M.1).contains(&ahead) && behind >= SINGLE_AFTER_TURN_M
}

/// A turn a single goes before: its radius, how far ahead of it the single ends, the least
/// ground after the last turn, and a single's length before its height is drawn.
const SINGLE_TURN_M: f32 = 30.0;
const SINGLE_BEFORE_TURN_M: (f32, f32) = (5.0, 60.0);
const SINGLE_AFTER_TURN_M: f32 = 15.0;
const SINGLE_NOMINAL_M: f32 = 28.0;

/// A side single: how tall, its crest, the clear ground either side, and the gap to the next.
const SIDE_SINGLE_H: (f32, f32) = (1.3, 1.8);
const SIDE_SINGLE_CREST_M: f32 = 1.2;
const SIDE_SINGLE_CLEAR_M: f32 = 10.0;
const SIDE_SINGLE_GAP_M: f32 = 30.0;

// ------------------------------------------------------------------------------------------
// Sections: what a lap built out of lanes carries.
//
// A national's features are drawn one after another down a continuous ribbon — `features`
// below walks the lap rolling a kind every few dozen metres — and that is the wrong shape for
// a stadium. A supercross lap is a handful of *lanes* joined by 180s, and a lane is one thing:
// a rhythm section, the whoops, a triple, or the finish. So the kind is chosen per lane and
// the lane is then filled with it end to end.
// ------------------------------------------------------------------------------------------

/// One stretch of ground between two corners — the lane a section is built on.
struct Lane {
    from: f32,
    to: f32,
    /// How far the corner feeding it turned, degrees. What the measured sequencing keys off:
    /// after a 180 the next thing is a rhythm lane about half the time.
    entry_deg: f32,
}

impl Lane {
    fn after_180(&self) -> bool {
        self.entry_deg >= 150.0
    }
    fn length(&self) -> f32 {
        self.to - self.from
    }
}

/// The lanes of a lap: every stretch straight or gentler than [`GENTLE_R_M`], with the corner
/// that feeds it.
fn lanes_of(segs: &[Segment]) -> Vec<Lane> {
    let (_, spans) = spans(segs);
    let gentle = |s: &Segment| match s {
        Segment::Straight { .. } => true,
        Segment::Arc { radius, .. } => radius.abs() >= GENTLE_R_M,
    };
    let mut out: Vec<Lane> = Vec::new();
    let mut run: Option<(f32, f32)> = None;
    let (mut turn, mut entry) = (0.0f32, 0.0f32);
    for (seg, (a, b, _)) in segs.iter().zip(&spans) {
        if gentle(seg) {
            match &mut run {
                Some(r) => r.1 = *b,
                None => {
                    run = Some((*a, *b));
                    entry = turn;
                    turn = 0.0;
                }
            }
        } else {
            if let Some((from, to)) = run.take() {
                out.push(Lane { from, to, entry_deg: entry });
            }
            if let Segment::Arc { angle, .. } = seg {
                turn += angle.abs();
            }
        }
    }
    if let Some((from, to)) = run.take() {
        out.push(Lane { from, to, entry_deg: entry });
    }
    // A lap wraps, so the opening lane is fed by the lap's last corner rather than by nothing.
    if let Some(first) = out.first_mut() {
        if first.from <= 0.01 {
            first.entry_deg = turn;
        }
    }
    out
}

/// The ground left clear at each end of a lane, metres: a section begins once the bike is out
/// of the corner and ends before the next one is turned into.
///
/// Four, not seven. A stadium lane runs 45 m at the short end and seven at each end took
/// fourteen of them — which is a third of the lane, and it left nothing that a rhythm section
/// would fit in. A supercross lane's first face really is a few metres off the berm.
const LANE_CLEAR_M: f32 = 4.0;

/// The shortest thing still worth calling a triple, metres crest to crest. Under this it is a
/// double with a lump in the middle of it.
const TRIPLE_LEAST_M: f32 = 17.0;

/// The fewest hills a rhythm section may be built with, and the room three of them need as a
/// multiple of the pitch — a quarter-pitch in, three hills, a quarter-pitch out.
const RHYTHM_FIT_LOW: u32 = 6;
const RHYTHM_ROOM_PITCHES: f32 = 3.5;

/// How many of something a lap gets, from its measured band. A free function rather than a
/// closure over the generator: a closure holds it borrowed for the whole of `section_features`.
fn count_in(rng: &mut Rng, band: (u32, u32)) -> i32 {
    rng.int(band.0 as i32, band.1 as i32).max(0)
}

/// A drawn shape from marks in metres along it.
fn drawn(at: f32, span: f32, marks: &[(f32, f32)]) -> Feature {
    Feature::Custom {
        at,
        length: span,
        side: 0.0,
        shape: marks
            .iter()
            .map(|(m, h)| crate::trackprog::ShapePoint { u: (m / span.max(1e-3)).clamp(0.0, 1.0), h: *h })
            .collect(),
    }
}

/// A run of evenly pitched hills, drawn as one shape so nothing is dug between them.
///
/// The troughs go to within a tenth of grade rather than sagging between crests. A rhythm lane
/// is a row of jumps a rider picks a line through — double-double, three-two, whatever the
/// pitch allows — and the ground coming back down between them is what makes each hill a lip
/// instead of a wave.
fn rhythm_lane(rng: &mut Rng, at: f32, room: f32, sec: &Sections) -> Option<Feature> {
    let pitch = rng.range(sec.rhythm_pitch_m.0, sec.rhythm_pitch_m.1);
    // As many hills as the lane actually holds, and never fewer than three: under three it is
    // a double with a roller after it, which is a different thing to ride. The measured five
    // or six is what a 60 m lane gives; a 45 m one gives three, and that is a real lane too.
    let hills = (count_in(rng, sec.rhythm_hills) as usize).min(RHYTHM_FIT_LOW as usize)
        .max(3)
        .min(((room / pitch) - 0.5).floor().max(0.0) as usize);
    if hills < 3 {
        return None;
    }
    // A quarter-pitch lead-in and a quarter-pitch run-out, so the first face is a ramp off flat
    // ground rather than a step out of the corner.
    let span = pitch * (hills as f32 + 0.5);
    if span > room {
        return None;
    }
    let mut marks: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut x = pitch * 0.25;
    for _ in 0..hills {
        let h = rng.range(sec.rhythm_h_m.0, sec.rhythm_h_m.1);
        marks.push((x + pitch * 0.34, h));
        marks.push((x + pitch * 0.50, h));
        marks.push((x + pitch * 0.92, h * 0.07));
        x += pitch;
    }
    marks.push((span, 0.0));
    Some(drawn(at, span, &marks))
}

/// Two hills at a jumpable pitch — the rhythm double the corpus counts one to three of a lap.
///
/// What goes on a lane too short for a whole rhythm section and too long to leave bare. The
/// pitch is clamped to the room rather than the lane being skipped: ten metres crest to crest
/// is a double either way, and a bare lane is the one thing every ride report has been about.
fn rhythm_double(rng: &mut Rng, at: f32, room: f32, sec: &Sections) -> Option<Feature> {
    let pitch = rng
        .range(sec.rhythm_double_m.0, sec.rhythm_double_m.1)
        .min(room / 2.5);
    if pitch < sec.rhythm_double_m.0 * 0.85 {
        return None;
    }
    let span = pitch * 2.5;
    let mut marks: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut x = pitch * 0.25;
    for _ in 0..2 {
        let h = rng.range(sec.rhythm_h_m.0, sec.rhythm_h_m.1);
        marks.push((x + pitch * 0.34, h));
        marks.push((x + pitch * 0.50, h));
        marks.push((x + pitch * 0.92, h * 0.07));
        x += pitch;
    }
    marks.push((span, 0.0));
    Some(drawn(at, span, &marks))
}

/// A triple: a take-off, a lump in the middle and a landing, the two outer crests `span` apart.
///
/// Both valleys go to within a sixteenth of grade. A triple whose middle never comes down is a
/// tabletop with a bump on it — and it is the ground between the crests that makes it a jump,
/// which is also how the corpus measured one in the first place.
fn triple(at: f32, h: f32, span: f32, lip_deg: f32) -> Feature {
    let up = crate::trackprog::lip_face_run(h, lip_deg);
    let down = landing_run(h);
    let c1 = up + 0.6;
    let total = up + 1.2 + span + down;
    let marks = [
        (0.0, 0.0),
        (up, h),
        (up + 1.2, h),
        (c1 + span * 0.28, h * 0.06),
        (c1 + span * 0.50, h * 0.62),
        (c1 + span * 0.72, h * 0.06),
        (c1 + span - 0.6, h),
        (c1 + span + 0.6, h),
        (total, 0.0),
    ];
    drawn(at, total, &marks)
}

/// The finish jump a discipline with [`Rules::finish_triple`] ends on, at `span` crest to crest.
pub(crate) fn finish_triple(at: f32, height: f32, span: f32) -> Feature {
    // 30°, which is what the seven measured rounds' finish lips come out at.
    triple(at, height, span, 30.0)
}

/// A tabletop, drawn rather than stated.
///
/// `Feature::Tabletop` sizes its faces off `JUMP_FACE_MIN_M` and `JUMP_LANDING_MIN_M`, which
/// are a national's numbers — nine metres of ramp and eighteen of landing on a jump whose deck
/// is seven. That is a thirty-four metre footprint inside a fifty-metre lane, and it leaves no
/// stadium lap room for anything else.
fn table(at: f32, h: f32, deck: f32, lip_deg: f32) -> Feature {
    let up = crate::trackprog::lip_face_run(h, lip_deg);
    let down = crate::trackprog::lip_face_run(h, lip_deg * 0.62);
    let total = up + deck + down;
    drawn(at, total, &[(0.0, 0.0), (up, h), (up + deck, h), (total, 0.0)])
}

/// How far apart a triple's crests may go here, metres, or `None` where none fits.
///
/// **This is the trap the speed model sets, and what was done about it.** The measured rounds
/// carry a triple of 22.6–25.3 m crest to crest. `trackspeed` says a rider leaving a 6.5 m 180
/// and driving down a sixty-metre lane arrives at about 50 km/h, which off a 30° lip carries
/// sixteen metres — so the measurement and the model disagree, and by a lot.
///
/// There were two ways out: place the big jumps after the long runs, or change the model. The
/// model is calibrated on ridden motocross laps — `A_LAT`, `P_SPEC` and `LAUNCH_SHARE` each
/// came back from the seat — and moving it for supercross would move every national with it.
/// So this does the first. A triple goes on the longest lanes the lap has, and it is built at
/// the span that lane actually carries rather than at the measured median; where even
/// [`TRIPLE_LEAST_M`] is out of reach, no triple is placed there at all.
///
/// The cost is honest and worth writing down: a lap comes out with triples a few metres
/// shorter than the corpus's, and on a short lap with fewer of them.
fn triple_span(
    speed: &crate::trackspeed::Speed,
    lip_at: f32,
    lip_deg: f32,
    band: (f32, f32),
) -> Option<f32> {
    // A tenth of margin off what it will just carry, because "just" is not a margin.
    let span = speed.carry(lip_at, lip_deg) * 0.9;
    (span >= TRIPLE_LEAST_M).then(|| span.min(band.1))
}

/// What a lane is used for.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Section {
    Triple,
    Whoops,
    Rhythm,
    Double,
    Table,
    /// Two hills at a jumpable pitch, on a lane too short for a whole rhythm section.
    RhythmDouble,
    /// One hill, on a lane too short to hold anything else.
    Single,
}

/// Everything a lap built from sections carries, placed lane by lane.
///
/// Taken on the built programme rather than on its segments, because the triples need the
/// speed model and that reads a lap.
fn section_features(
    rng: &mut Rng,
    prog: &TrackProgram,
    knobs: &LayoutKnobs,
    seed: u64,
) -> Vec<Feature> {
    let rules = prog.discipline.rules();
    let Some(sec) = rules.sections else {
        return Vec::new();
    };
    let lap = prog.lap_length();
    let speed = crate::trackspeed::of(prog);
    let lanes = lanes_of(&prog.segments);
    if lanes.len() < 2 {
        return Vec::new();
    }

    let (mut triples, mut whoops) = (count_in(rng, sec.triples), count_in(rng, sec.whoop_sections));
    let (mut rhythms, mut doubles) = (count_in(rng, sec.rhythm_lanes), count_in(rng, sec.doubles));
    let (mut tables, mut sands) = (count_in(rng, sec.tables), count_in(rng, sec.sand_sections));

    // Lane zero is the start straight: the gate row stands on it and `trackllm::repair` builds
    // the finish jump into it, so nothing else goes there.
    let room = |l: &Lane| (l.from + LANE_CLEAR_M, (l.length() - 2.0 * LANE_CLEAR_M).max(0.0));
    let mut kind: Vec<Option<Section>> = vec![None; lanes.len()];

    // 1. Triples, longest lane first — the one thing here whose size the run-up decides.
    let mut longest: Vec<usize> = (1..lanes.len()).collect();
    longest.sort_by(|a, b| lanes[*b].length().total_cmp(&lanes[*a].length()));
    for i in longest {
        if triples <= 0 {
            break;
        }
        let (at, r) = room(&lanes[i]);
        let h = rng.range(rules.finish_jump_m.0, rules.finish_jump_m.1);
        let up = crate::trackprog::lip_face_run(h, 26.0);
        let Some(span) = triple_span(&speed, at + up, 26.0, sec.triple_span_m) else {
            continue;
        };
        if up + 1.2 + span + landing_run(h) <= r {
            kind[i] = Some(Section::Triple);
            triples -= 1;
        }
    }

    // 2. The whoops: one set, straight out of a 180, in the second half of the lap. Measured
    //    that way on every one of the seven — they are the section a lap is remembered for and
    //    they are never the first thing after the gate.
    for i in 1..lanes.len() {
        if whoops <= 0 {
            break;
        }
        let (_, r) = room(&lanes[i]);
        if kind[i].is_none() && lanes[i].after_180() && lanes[i].from > lap * 0.45 && r >= sec.whoops_m {
            kind[i] = Some(Section::Whoops);
            whoops -= 1;
        }
    }

    // 3. Rhythm lanes. The first pass takes the measured pairing — a rhythm lane follows a 180
    //    about half the time — and the second fills whatever is still owed on what is left.
    //    The count is the measured thing; the sequencing is a tendency, and on a lap of seven
    //    lanes a tendency alone leaves half of them bare.
    // Measured off the *short* end of the pitch band: sized off the long end, a 37 m lane fell
    // a metre short of the bar and SuperMotocross drew no rhythm lanes at all.
    let need = sec.rhythm_pitch_m.0 * RHYTHM_ROOM_PITCHES;
    for pass in 0..2 {
        for i in 1..lanes.len() {
            if rhythms <= 0 {
                break;
            }
            if kind[i].is_some() {
                continue;
            }
            let (_, r) = room(&lanes[i]);
            if r < need || (pass == 0 && !(lanes[i].after_180() && rng.chance(0.51))) {
                continue;
            }
            kind[i] = Some(Section::Rhythm);
            rhythms -= 1;
        }
    }

    // 4. Then the standalone doubles and the tabletops, on what is left.
    for i in 1..lanes.len() {
        if kind[i].is_some() {
            continue;
        }
        let (at, r) = room(&lanes[i]);
        let h = rng.range(1.8, 2.3);
        if doubles > 0 && r >= sec.double_m.1 + 24.0 && speed.carry(at + 8.0, 26.0) >= sec.double_m.0 {
            kind[i] = Some(Section::Double);
            doubles -= 1;
        } else if tables > 0 && r >= sec.table_deck_m.1 + 2.0 * crate::trackprog::lip_face_run(h, 24.0) {
            kind[i] = Some(Section::Table);
            tables -= 1;
        }
    }

    // And nothing is left bare. A lane with nothing built on it is a straight with nothing to
    // do on it, which is what every round of feedback on a generated lap has been about — and
    // on a stadium floor, where the lanes are the lap, a bare one is a quarter of the track.
    for i in 1..lanes.len() {
        if kind[i].is_some() {
            continue;
        }
        let (_, r) = room(&lanes[i]);
        if r >= sec.rhythm_double_m.0 * 2.5 {
            kind[i] = Some(Section::RhythmDouble);
        } else if r >= 22.0 {
            kind[i] = Some(Section::Single);
        }
    }

    // And build what was chosen.
    //
    // An outdoor lap starts from the national's own mix — `features` fills a ribbon end to end
    // — and the sections are laid over the top of it. One section per lane is the right shape
    // for a stadium, where the lanes *are* the lap; on a 1600 m SuperMotocross lap with twenty
    // short runs it left the track at nine features a kilometre against a measured forty-four.
    // A stadium floor is only its lanes, so there the sections are the whole of it.
    let mut out: Vec<Feature> = if rules.stadium.is_none() {
        let mut f = features(rng, &prog.segments, knobs);
        side_singles(&mut f, &prog.segments, seed, rules);
        fill_gaps(&mut f, &prog.segments, seed, rules);
        f
    } else {
        Vec::new()
    };
    for (i, lane) in lanes.iter().enumerate() {
        let (at, r) = room(lane);
        let Some(k) = kind[i] else { continue };
        match k {
            Section::Triple => {
                let h = rng.range(rules.finish_jump_m.0, rules.finish_jump_m.1);
                let up = crate::trackprog::lip_face_run(h, 26.0);
                if let Some(span) = triple_span(&speed, at + up, 26.0, sec.triple_span_m) {
                    let total = up + 1.2 + span + landing_run(h);
                    lay(&mut out, triple(at + (r - total) * 0.5, h, span, 26.0));
                }
            }
            Section::Whoops => {
                let count = count_in(rng, sec.whoops).max(4) as u32;
                let spacing = rng.range(sec.whoop_pitch_m.0, sec.whoop_pitch_m.1);
                let height = rng.range(sec.whoop_h_m.0, sec.whoop_h_m.1);
                let span = count as f32 * spacing;
                lay(
                    &mut out,
                    Feature::Whoops { at: at + (r - span).max(0.0) * 0.5, count, spacing, height },
                );
            }
            Section::Rhythm => {
                if let Some(f) = rhythm_lane(rng, at, r.min(sec.rhythm_m.1), &sec) {
                    let slack = r - f.length();
                    lay(&mut out, shifted(f, slack * 0.5));
                }
            }
            Section::Double => {
                let height = rng.range(1.9, 2.4);
                let faces = crate::trackprog::double_faces(height, 0.0);
                let span = rng.range(sec.double_m.0, sec.double_m.1);
                let gap = (span - faces.back - faces.face).max(2.0);
                let total = faces.total(gap);
                lay(
                    &mut out,
                    Feature::Double {
                        at: at + (r - total).max(0.0) * 0.5,
                        height,
                        gap,
                        lip: 0.0,
                        finish: false,
                    },
                );
            }
            Section::RhythmDouble => {
                if let Some(f) = rhythm_double(rng, at, r, &sec) {
                    let slack = r - f.length();
                    lay(&mut out, shifted(f, slack.max(0.0) * 0.5));
                }
            }
            Section::Table | Section::Single => {
                let height = rng.range(1.5, 2.2);
                let up = crate::trackprog::lip_face_run(height, 24.0);
                let down = crate::trackprog::lip_face_run(height, 24.0 * 0.62);
                // A single is a table with no top to speak of: one lip, a crest, and down.
                let want = if k == Section::Single {
                    rng.range(1.2, 2.4)
                } else {
                    rng.range(sec.table_deck_m.0, sec.table_deck_m.1)
                };
                let deck = want.min((r - up - down).max(1.2));
                if up + deck + down <= r {
                    let f = table(at, height, deck, 24.0);
                    let slack = r - f.length();
                    lay(&mut out, shifted(f, slack.max(0.0) * 0.5));
                }
            }
        }
    }

    // 5. And the sand, on a lane nothing was built on, or failing that round a corner. Both
    //    halves of it live elsewhere — the band in `tracksynth::layers` and the half-metre
    //    material layer in `tht` — and this only says where.
    for i in 1..lanes.len() {
        if sands <= 0 {
            break;
        }
        if kind[i].is_some() {
            continue;
        }
        let length = rng.range(sec.sand_m.0, sec.sand_m.1).min(lanes[i].length());
        out.push(Feature::Sand { at: lanes[i].from, length });
        kind[i] = Some(Section::Table);
        sands -= 1;
    }
    // Nothing spare: put it through the corner after the longest lane instead, which is where a
    // real round lays one — a sand turn rather than a sand lane.
    if sands > 0 {
        if let Some(l) = lanes.iter().skip(1).max_by(|a, b| a.length().total_cmp(&b.length())) {
            let length = rng.range(sec.sand_m.0, sec.sand_m.1);
            out.push(Feature::Sand { at: (l.to + 1.0).min(lap - length - 1.0).max(0.0), length });
        }
    }

    out.sort_by(|a, b| a.at().total_cmp(&b.at()));
    out
}

/// The same feature, `by` metres further round the lap.
fn shifted(mut f: Feature, by: f32) -> Feature {
    *f.at_mut() += by.max(0.0);
    f
}

/// Lay a section down, and let whatever stood on that ground give way.
///
/// Two jumps blended into each other are one shape with a dip in it, so a rhythm lane laid
/// over a national's tabletop has to take the tabletop's ground rather than share it.
fn lay(out: &mut Vec<Feature>, f: Feature) {
    let (a, b) = (f.at(), f.at() + f.length());
    out.retain(|g| {
        // Sand is under the jumps rather than among them, so it never gives way to one.
        matches!(g, Feature::Sand { .. }) || g.at() + g.length() <= a + 0.01 || g.at() >= b - 0.01
    });
    out.push(f);
}

fn features(rng: &mut Rng, segs: &[Segment], knobs: &LayoutKnobs) -> Vec<Feature> {
    let [table, whale, single, double, triple, table_single, waves_at, step] = knobs.picks;
    let js = knobs.jump_scale;
    let rules = knobs.rules();
    let (total, spans) = spans(segs);
    let tight = |at: f32, length: f32| {
        spans
            .iter()
            .any(|(a, b, r)| r.is_some_and(|r| r < rules.tight_m) && *b > at && *a < at + length)
    };
    let mut waves_laid = 0usize;
    let mut out = Vec::new();
    // The first stretch is where the gate row goes, and forty riders arrive at the first jump
    // in a pack. Leave it bare.
    let mut pos = rules.start_clear_m;
    while pos < total - 40.0 {
        let room = total - 20.0 - pos;
        if tight(pos - JUMP_RUNUP_M, JUMP_RUNUP_M + 34.0) {
            // A short step, so a jump lands as soon as the run-up is clear rather than up to
            // twelve metres past it.
            pos += 4.0;
            continue;
        }
        // No whoops. Asked for outright, after riding a lap with them: they are the one thing
        // on a track that has to be built right or not at all, and ours are not.
        let pick = rng.range(0.0, 1.0);
        let length;
        if pick < table && room > 38.0 && flies_straight(&spans, pos, 45.0) {
            // A table's size is its *deck*, with the faces added on. The faces are set by the
            // published lip and landing angles and come to thirty-odd metres on their own, so
            // stating a 40 m table asks for a 6 m top and gets a long rounded hill.
            //
            // Capped at three metres: Motorcycling Australia and Motorcycling New Zealand both
            // write "jumps must not exceed 3m in height", and `corpus::FEATURE_HEIGHT_M` holds
            // a program to it. This used to draw up to 3.4 and every table was outside it.
            // 70% of the 2.4-3.0 m drawn here before: 85% rode too big again.
            // Taller: ridden as "the jumps are still very small height-wise".
            let height = rng.range(2.4, 3.0) * js;
            length = (rng.range(11.2, 18.9) + faces(height)).min(room);
            out.push(Feature::Tabletop { at: pos, length, height, lip: 0.0, finish: false });
        } else if pick < whale && room > 40.0 && flies_straight(&spans, pos, 55.0) {
            // A table is not always flat end to end. A whale tail rises, dips over its middle
            // and rises again before the landing — two crests a rider can either double or
            // roll — which is a shape a tabletop's three numbers cannot describe.
            //
            // Drawn in metres and normalised afterwards, so the take-off gets the same run a
            // tabletop of this height gets. Drawn as fractions it had 3.6 m of lip in 8.5 m of
            // ground, and from the seat that is a wall.
            let h = rng.range(2.3, 2.9) * js;
            let dip = rng.range(0.30, 0.40);
            let (up, down) = (lip_run(h), landing_run(h));
            let near = up + 2.8 + down * 0.55;
            let marks = [
                (0.0, 0.0),
                (up, h),
                (up + 2.8, h),
                (near, h * dip),
                (near + 7.7, h * 0.66),
                (near + 7.7 + down * 0.7, h * 0.16),
                (near + 7.7 + down, 0.0),
            ];
            let span = marks[marks.len() - 1].0;
            length = span.min(room);
            let scale = length / span;
            out.push(Feature::Custom {
                at: pos,
                length,
                side: 0.0,
                shape: marks
                    .iter()
                    .map(|(m, v)| crate::trackprog::ShapePoint { u: m / span, h: (v * scale).min(2.9) })
                    .collect(),
            });
        } else if pick < single && room > 32.0 && single_fits(&spans, pos, SINGLE_NOMINAL_M) && flies_straight(&spans, pos, SINGLE_NOMINAL_M) {
            // A single: one mound, jumped off its face and landed on its own back.
            let h = rng.range(1.8, 2.5) * js;
            let (up, down) = (air_run(h), landing_run(h));
            let crest = 1.5;
            let span = up + crest + down;
            length = span.min(room);
            let scale = length / span;
            let marks = [(0.0, 0.0), (up, h), (up + crest, h), (span, 0.0)];
            out.push(Feature::Custom {
                at: pos,
                length,
                side: 0.0,
                shape: marks
                    .iter()
                    .map(|(m, v)| crate::trackprog::ShapePoint { u: m / span, h: (v * scale).min(2.9) })
                    .collect(),
            });
        } else if pick < double && room > 56.0 && flies_straight(&spans, pos, 45.0) {
            // A double: a take-off, a gap and a landing ramp, cleared in one.
            let height = rng.range(2.2, 2.8) * js;
            let lip = if rng.range(0.0, 1.0) < 0.5 { 0.0 } else { 10.0 };
            // Crest to crest stays near what it was: the gentler back and front take the rest.
            let gap = rng.range(4.0, 9.0);
            length = crate::trackprog::double_faces(height, lip).total(gap);
            out.push(Feature::Double { at: pos, height, gap, lip, finish: false });
        } else if pick < triple && room > 64.0 && flies_straight(&spans, pos, 55.0) {
            // A triple: a take-off, a middle lump and a landing ramp. The fast clear it in one;
            // everyone else jumps it as a double and a single.
            let h = rng.range(1.9, 2.5) * js;
            let (up, down) = (air_run(h), landing_run(h));
            let (g1, g2) = (rng.range(7.0, 9.0), rng.range(7.0, 9.0));
            let mut x = 0.0f32;
            let mut marks = vec![(0.0f32, 0.0f32)];
            for (run, v) in [(up, h), (1.2, h), (g1, 0.35 * h), (3.0, 0.8 * h), (g2, 0.35 * h), (3.0, 0.9 * h), (down, 0.0)] {
                x += run;
                marks.push((x, v));
            }
            length = x;
            out.push(Feature::Custom {
                at: pos,
                length,
                side: 0.0,
                shape: marks
                    .iter()
                    .map(|(m, v)| crate::trackprog::ShapePoint { u: m / x, h: *v })
                    .collect(),
            });
        } else if pick < table_single && room > 70.0 && flies_straight(&spans, pos, 68.0) {
            // A table with a single after it: roll the table and jump the single, clear the
            // deck onto the single's back as a double, or go further still.
            let h = rng.range(2.3, 2.8) * js;
            let h2 = rng.range(1.8, 2.2) * js;
            let (up, down) = (lip_run(h), landing_run(h));
            let (up2, down2) = (air_run(h2), landing_run(h2));
            let deck = rng.range(8.0, 12.0);
            let gap = rng.range(4.0, 8.0);
            let mut x = 0.0f32;
            let mut marks = vec![(0.0f32, 0.0f32)];
            for (run, v) in [
                (up, h),
                (deck, h),
                (down * 0.6, 0.2 * h),
                (gap * 0.5, 0.15 * h),
                (gap * 0.5, 0.2 * h),
                (up2, h2),
                (1.5, h2),
                (down2, 0.0),
            ] {
                x += run;
                marks.push((x, v));
            }
            length = x;
            out.push(Feature::Custom {
                at: pos,
                length,
                side: 0.0,
                shape: marks
                    .iter()
                    .map(|(m, v)| crate::trackprog::ShapePoint { u: m / x, h: *v })
                    .collect(),
            });
        } else if pick < waves_at && room > 90.0 && waves_laid < knobs.waves {
            waves_laid += 1;
            // A wave section, drawn as one shape so no hollow is dug between them: a small
            // kicker to flow in, the waves at one size, and a small one out.
            // Long enough for a bike to sit in, and each one its own: they all rode the same.
            let waves = rng.range(3.0, 5.99) as usize;
            let wave = rng.range(12.0, 16.0);
            let h = rng.range(0.8, 1.2);
            let mut bumps = vec![(wave * 0.7, h * 0.5)];
            for _ in 0..waves {
                bumps.push((wave * rng.range(0.8, 1.25), h * rng.range(0.8, 1.2)));
            }
            bumps.push((wave * 0.7, h * 0.5));
            let span: f32 = bumps.iter().map(|b| b.0).sum();
            length = span;
            let mut shape = Vec::new();
            let mut x0 = 0.0f32;
            for (len, bh) in bumps {
                let n = len.ceil() as usize;
                for i in 0..n {
                    let x = i as f32 / n as f32;
                    let v = bh * 0.5 * (1.0 - (std::f32::consts::TAU * x).cos());
                    shape.push(crate::trackprog::ShapePoint { u: (x0 + x * len) / span, h: v });
                }
                x0 += len;
            }
            shape.push(crate::trackprog::ShapePoint { u: 1.0, h: 0.0 });
            out.push(Feature::Custom { at: pos, length, side: 0.0, shape });
        } else if pick < step && room > 24.0 {
            // A climb rather than a wall with a ramp on it, or a drop down one.
            // Shorter and taller: at under a metre over forty the step rode as a flat straight.
            length = rng.range(20.0, 28.0).min(room);
            let height = rng.range(1.2, 1.9) * if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
            out.push(Feature::StepUp { at: pos, length, height });
        } else {
            length = rng.range(10.0, 16.0).min(room);
            if length < 8.0 {
                break;
            }
            out.push(Feature::Roller { at: pos, length, height: rng.range(0.49, 0.84) });
        }
        // Closer than 6-15 m, so the run-up a jump now keeps off a corner does not thin the lap
        // below the published twelve lips a kilometre.
        // Closer together now the jumps are bigger, or a lap falls short of the corpus's count.
        pos += length + rng.range(knobs.feature_gap.0, knobs.feature_gap.1);
    }
    out
}

/// What a track is like, in the words a brief uses. A model reads the brief and fills this in;
/// [`LayoutKnobs::from_settings`] turns it into the bands the walk draws from. The seed still
/// decides the lap itself, so two seeds with the same settings are two tracks of one kind.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackSettings {
    /// Empty leaves the seed's own name.
    pub name: String,
    /// Empty leaves the seed's own place.
    pub location: String,
    /// Metres, 1700–2300: at 2400 under half of seeds closed with everything else at its top.
    pub lap_length: f32,
    /// Metres, 12–18.
    pub width: f32,
    /// 6.5–12.
    pub corners_per_km: f32,
    /// Metres, the tightest radius in a normal corner, 8–18.
    pub apex_radius: f32,
    /// 0–0.5, the share of corners that are one long sweep.
    pub sweep_share: f32,
    /// Metres, 60–125.
    pub start_straight: f32,
    /// 0 sparse … 1 packed.
    pub jump_density: f32,
    /// 0 mostly rollers … 1 mostly real jumps.
    pub big_jump_share: f32,
    /// 0.8–1.0, a multiplier on jump heights.
    pub jump_scale: f32,
    /// At most this many wave sections, 0–2.
    pub waves: u32,
    pub surface: Surface,
    /// 0–1, see [`Terrain::wear`].
    pub wear: f32,
    /// 1–2: ARL is anything above 1.
    pub roughness: f32,
    /// Relief amplitude, metres, 2–10.
    pub hills: f32,
    /// Metres the plot falls side to side, 0–25.
    pub tilt: f32,
    /// 0–5.
    pub landforms: u32,
    /// 0–4 long straights that climb or drop.
    pub elevation_changes: u32,
    /// Set by the app from the discipline switch, never by the model. `None` is motocross.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discipline: Option<Discipline>,
}

impl Default for TrackSettings {
    /// The middle of what [`draw`] draws on its own.
    fn default() -> Self {
        TrackSettings {
            name: String::new(),
            location: String::new(),
            lap_length: 2175.0,
            width: 16.25,
            corners_per_km: 8.85,
            apex_radius: 11.5,
            sweep_share: 0.22,
            start_straight: 110.0,
            jump_density: 0.65,
            big_jump_share: 0.5,
            jump_scale: 1.0,
            waves: 2,
            surface: Surface::Soil,
            wear: crate::trackprog::default_wear(),
            roughness: crate::trackprog::default_roughness(),
            hills: 6.0,
            tilt: 11.0,
            landforms: 3,
            elevation_changes: 2,
            discipline: None,
        }
    }
}

impl TrackSettings {
    /// Inside the bounds the walk still closes in. A model writes whatever it likes; a NaN
    /// takes the default rather than poisoning every band it feeds.
    pub fn clamped(&self) -> Self {
        let d = TrackSettings::default();
        let c = |v: f32, lo: f32, hi: f32, or: f32| if v.is_nan() { or } else { v.clamp(lo, hi) };
        let trim = |s: &str| s.trim().chars().take(60).collect::<String>().trim().to_string();
        TrackSettings {
            name: trim(&self.name),
            location: trim(&self.location),
            lap_length: c(self.lap_length, 1700.0, 2300.0, d.lap_length),
            width: c(self.width, 12.0, 18.0, d.width),
            corners_per_km: c(self.corners_per_km, 6.5, 12.0, d.corners_per_km),
            apex_radius: c(self.apex_radius, 8.0, 18.0, d.apex_radius),
            sweep_share: c(self.sweep_share, 0.0, 0.5, d.sweep_share),
            start_straight: c(self.start_straight, 60.0, 125.0, d.start_straight),
            jump_density: c(self.jump_density, 0.0, 1.0, d.jump_density),
            big_jump_share: c(self.big_jump_share, 0.0, 1.0, d.big_jump_share),
            jump_scale: c(self.jump_scale, 0.8, 1.0, d.jump_scale),
            waves: self.waves.min(2),
            surface: self.surface,
            wear: c(self.wear, 0.0, 1.0, d.wear),
            roughness: c(self.roughness, 1.0, 2.0, d.roughness),
            hills: c(self.hills, 2.0, 10.0, d.hills),
            tilt: c(self.tilt, 0.0, 25.0, d.tilt),
            landforms: self.landforms.min(5),
            elevation_changes: self.elevation_changes.min(4),
            discipline: self.discipline,
        }
    }
}

/// Every choice the walk makes that a setting can move, as the band it draws from.
///
/// `Default` is exactly what [`draw`] has always drawn, and a knob that only overrides a
/// roll still makes the roll: a seed is its sequence of draws, and one fewer moves every lap.
#[derive(Clone, Debug)]
pub struct LayoutKnobs {
    pub(crate) width: (f32, f32),
    pub(crate) lap: (f32, f32),
    /// The finished lap must measure inside this, or it is walked again.
    pub(crate) lap_within: Option<(f32, f32)>,
    /// How many walks a seed gets before it gives up.
    pub(crate) attempts: usize,
    pub(crate) corners_per_km: (f32, f32),
    pub(crate) start_straight: (f32, f32),
    pub(crate) sweep_share: f32,
    pub(crate) apex: (f32, f32),
    /// Cumulative odds: table, whale, single, double, triple, table and single, waves, step;
    /// a roller past the last.
    pub(crate) picks: [f32; 8],
    pub(crate) jump_scale: f32,
    pub(crate) waves: usize,
    pub(crate) feature_gap: (f32, f32),
    pub(crate) elevation_changes: usize,
    pub(crate) surface: Option<Surface>,
    pub(crate) amplitude: (f32, f32),
    pub(crate) tilt: (f32, f32),
    pub(crate) landforms: (i32, i32),
    pub(crate) name: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) wear: Option<f32>,
    pub(crate) roughness: Option<f32>,
    /// Which rules the lap is drawn by, see [`Rules`].
    pub(crate) discipline: Discipline,
}

/// The feature odds [`draw`] has always used, cumulative. Written out rather than summed so
/// the default compares against the very same floats.
const PICKS: [f32; 8] = [0.22, 0.30, 0.40, 0.50, 0.57, 0.64, 0.68, 0.86];

impl Default for LayoutKnobs {
    fn default() -> Self {
        LayoutKnobs::for_discipline(Discipline::Mx)
    }
}

impl LayoutKnobs {
    /// What [`draw`] draws for a discipline, from its row of [`Rules`].
    pub fn for_discipline(discipline: Discipline) -> Self {
        let r = discipline.rules();
        LayoutKnobs {
            width: r.width_m,
            lap: r.lap_m,
            // A lap that strays outside the band it was drawn from is walked again.
            //
            // Motocross has never done this and does not start now — `draw` takes whatever
            // closes. SuperMotocross has to: once one stretch is allowed to run 350 m, the
            // way home can add a couple of hundred more, and a seed in six came out over
            // 2100 m against a band that tops out at 1900.
            lap_within: (!discipline.is_mx()).then_some(r.lap_m),
            attempts: if discipline.is_mx() { 6 } else { 12 },
            corners_per_km: CORNERS_PER_KM,
            start_straight: r.start_straight_draw_m,
            sweep_share: 0.22,
            apex: r.apex_m,
            picks: PICKS,
            jump_scale: 1.0,
            waves: MAX_WAVE_SECTIONS,
            feature_gap: (3.0, 8.0),
            elevation_changes: r.elevation_changes,
            // A stadium floor is dirt.
            surface: r.flat.then_some(Surface::Soil),
            amplitude: r.amplitude_m,
            tilt: r.tilt_m,
            landforms: r.landforms,
            name: None,
            location: None,
            wear: None,
            roughness: None,
            discipline,
        }
    }

    pub(crate) fn rules(&self) -> &'static Rules {
        self.discipline.rules()
    }

    /// Settings into bands. Each is a narrow band round what was asked rather than the point
    /// itself: a point makes every seed walk the same way into the same dead ends.
    pub fn from_settings(s: &TrackSettings) -> Self {
        let s = s.clamped();
        let band = |v: f32, by: f32, lo: f32, hi: f32| ((v - by).max(lo), (v + by).min(hi));
        // Jumps against everything else. 0.5 is today's 64%; the kinds keep their proportions.
        let jumps = if s.big_jump_share <= 0.5 {
            0.2 + 0.44 * s.big_jump_share / 0.5
        } else {
            0.64 + 0.28 * (s.big_jump_share - 0.5) / 0.5
        };
        let (kj, kr) = (jumps / 0.64, (1.0 - jumps) / 0.36);
        let weights = [0.22 * kj, 0.08 * kj, 0.10 * kj, 0.10 * kj, 0.07 * kj, 0.07 * kj, 0.04 * kr, 0.18 * kr];
        let mut picks = [0.0f32; 8];
        let mut acc = 0.0;
        for (p, w) in picks.iter_mut().zip(weights) {
            acc += w;
            *p = acc;
        }
        // The gap after a feature: 12 m sparse, 2 m packed; 0.65 is today's 3–8.
        let gap = 12.0 - 10.0 * s.jump_density;
        let some = |t: &str| (!t.is_empty()).then(|| t.to_string());
        // Another discipline keeps its own lap, width and ground, which the settings' ranges
        // are motocross's for. The brief still sets what is built on it and the name.
        if let Some(d) = s.discipline.filter(|d| !d.is_mx()) {
            return LayoutKnobs {
                picks,
                jump_scale: s.jump_scale,
                waves: s.waves as usize,
                feature_gap: ((gap - 2.5).max(1.0), gap + 2.5),
                surface: Some(s.surface),
                name: some(&s.name),
                location: some(&s.location),
                wear: Some(s.wear),
                roughness: Some(s.roughness),
                ..LayoutKnobs::for_discipline(d)
            };
        }
        LayoutKnobs {
            width: band(s.width, 0.3, 12.0, 18.0),
            lap: (s.lap_length * 0.96, s.lap_length * 1.04),
            lap_within: Some((s.lap_length * 0.93, s.lap_length * 1.07)),
            // The window turns away about a third of the laps that close, so a seed gets more
            // walks to find one that fits.
            attempts: 12,
            corners_per_km: band(s.corners_per_km, 0.5, 6.0, 12.5),
            // The start straight's top is today's: past it the lap's merged straight runs over
            // the FFM's 125 m.
            start_straight: band(s.start_straight, 20.0, 60.0, 130.0),
            sweep_share: s.sweep_share,
            // 11.5 m, today's middle, gives today's 8–15.
            apex: band(s.apex_radius, 3.5, 7.0, 21.5),
            picks,
            jump_scale: s.jump_scale,
            waves: s.waves as usize,
            feature_gap: ((gap - 2.5).max(1.0), gap + 2.5),
            elevation_changes: s.elevation_changes as usize,
            surface: Some(s.surface),
            amplitude: (s.hills * 0.85, s.hills * 1.15),
            tilt: (s.tilt * 0.75, s.tilt * 1.25),
            landforms: (s.landforms as i32, s.landforms as i32),
            name: some(&s.name),
            location: some(&s.location),
            wear: Some(s.wear),
            roughness: Some(s.roughness),
            discipline: Discipline::Mx,
        }
    }
}

/// One lap, from one number. Not checked — see [`search`] for that.
pub fn draw(seed: u64) -> Option<TrackProgram> {
    draw_with(seed, &LayoutKnobs::default())
}

/// One lap, from one number and the bands to draw it from.
pub fn draw_with(seed: u64, knobs: &LayoutKnobs) -> Option<TrackProgram> {
    let rules = knobs.rules();
    let mut rng = Rng::new(seed);
    let width = rng.range(knobs.width.0, knobs.width.1);
    // A lap that strays from the length asked for is walked again: the walk only aims at its
    // target, and short ones run long. With no length asked for, every lap fits.
    let fits = |segs: &[Segment]| {
        knobs.lap_within.is_none_or(|(lo, hi)| (lo..=hi).contains(&chain_length(segs)))
    };
    let mut grown = None;
    for _ in 0..knobs.attempts {
        let want = rng.range(knobs.lap.0, knobs.lap.1);
        grown = match &rules.stadium {
            // Laid lane by lane, inside the floor and the rules: nothing to break up.
            Some(st) => walk_stadium(&mut rng, rules, st, width, knobs.lap),
            None => walk(&mut rng, rules.plot.0, width, want, knobs).map(|(mut segs, start)| {
                // No rng in here, so measuring after it moves nothing.
                // Never past the discipline's own ceiling: bending SuperMotocross's one long
                // side into an S is exactly what it is there not to be.
                let cap = rules.home_run_max_m.max(STRAIGHT_MAX_M);
                break_long_straights(&mut segs, (start.x, start.z, start.angle.to_radians()), width, cap);
                (segs, start)
            }),
        };
        if grown.as_ref().is_some_and(|(segs, _)| fits(segs)) {
            break;
        }
    }
    let (mut segments, start) = grown.filter(|(segs, _)| fits(segs))?;
    // A discipline built from sections places them at the bottom of this function instead, once
    // there is a programme for the speed model to read — how big a triple may be is a question
    // about the run-up, and a run-up is a lap rather than a chain of segments.
    let features = if rules.sections.is_some() {
        Vec::new()
    } else {
        let mut f = features(&mut rng, &segments, knobs);
        side_singles(&mut f, &segments, seed, rules);
        fill_gaps(&mut f, &segments, seed, rules);
        f
    };
    // Up and down: a climb on one long straight and a drop on another. Drawn after the jumps,
    // so the lap's shape and what is built on it stay where they were; the lap hands any net
    // rise back evenly.
    let mut long: Vec<(u64, usize)> = segments
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, s)| matches!(s, Segment::Straight { length, .. } if *length >= 45.0))
        .map(|(i, _)| ((rng.range(0.0, 1.0) * 1e6) as u64, i))
        .collect();
    long.sort();
    for (j, &(_, i)) in long.iter().take(knobs.elevation_changes).enumerate() {
        if let Segment::Straight { rise, .. } = &mut segments[i] {
            *rise = rng.range(2.5, 4.0) * if j % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    let rolled = match rng.int(0, 9) {
        0..=6 => Surface::Soil,
        7..=8 => Surface::Sand,
        _ => Surface::Grass,
    };
    let surface = knobs.surface.unwrap_or(rolled);
    let mut prog = TrackProgram {
        name: knobs
            .name
            .clone()
            .unwrap_or_else(|| NAMES[(seed % NAMES.len() as u64) as usize].to_string()),
        author: "MXB App".into(),
        location: knobs
            .location
            .clone()
            .unwrap_or_else(|| PLACES[((seed / 7) % PLACES.len() as u64) as usize].to_string()),
        width,
        blend: crate::trackprog::default_blend(),
        terrain: Terrain {
            size_x: rules.plot.0,
            size_z: rules.plot.1,
            samples: rules.samples,
            scale: if rng.chance(0.5) { 63.0 } else { 70.0 },
            surface,
            wear: knobs.wear.unwrap_or_else(crate::trackprog::default_wear),
            roughness: knobs.roughness.unwrap_or_else(crate::trackprog::default_roughness),
            // Gently rolling, and no more. A lap is benched into whatever it crosses, so
            // ground with twenty metres of landform in it puts the track in a trench with the
            // banners along the rim of the cut. A motocross venue is a field with shape in it.
            relief: Relief {
                amplitude: rng.range(knobs.amplitude.0, knobs.amplitude.1),
                wavelength: rng.range(320.0, 480.0),
                seed: (seed % 9973) as u32,
                texture: 0.085,
                tilt: rng.range(knobs.tilt.0, knobs.tilt.1),
                tilt_angle: rng.range(0.0, 359.0),
                landforms: rng.int(knobs.landforms.0, knobs.landforms.1) as u32,
                landform_height: rng.range(2.0, 5.0),
            },
        },
        start,
        segments,
        features,
        elevation: Vec::new(),
        discipline: knobs.discipline,
        // Soft, which is what a stadium round lines its lanes with. A person changes it per
        // track; the walk has no opinion.
        tuff: crate::trackprog::TuffBlocks::default(),
    };
    if rules.sections.is_some() {
        prog.features = section_features(&mut rng, &prog, knobs, seed);
    }
    Some(prog)
}

/// How many floors a stadium lap is tried on before the seed gives up.
const STADIUM_TRIES: usize = 40;

/// A supercross lap: lanes across a stadium floor, and one lane down the side that brings it
/// back to the start straight.
///
/// Each lane ends where its run puts it rather than at the wall, and the next lane's end decides
/// what the corner is: back the way it came is a 180, on past it is a step across on two 90s.
/// The measured tracks are 5–8 180s and 0–3 other corners with runs spread round a 64 m median,
/// and both fall out of drawing the runs.
///
/// It closes and cannot cross itself by construction: every lane has a row to itself, the
/// return lane runs outside every lane's end, and each corner is filleted inside its own square.
/// Returns the lap and its start pose — lane one, which is the start straight, on a long side
/// and into the holeshot 180.
fn walk_stadium(
    rng: &mut Rng,
    rules: &Rules,
    st: &Stadium,
    width: f32,
    lap: (f32, f32),
) -> Option<(Vec<Segment>, Start)> {
    for _ in 0..STADIUM_TRIES {
        let n = rng.int(st.lanes.0, st.lanes.1) as usize;
        // How many rows each lane moves over: one is a 180, two is a wide turn round a short
        // link, which reads as two 90s. The holeshot is always a 180.
        let skips: Vec<usize> = (0..n - 1)
            .map(|k| if k > 0 && rng.chance(st.wide) { 2 } else { 1 })
            .collect();
        let rows = 1 + skips.iter().sum::<usize>();
        let widest = (st.floor.1 - 2.0 * st.edge_m) / (rows - 1) as f32;
        if widest < st.lane_gap_m.0 {
            continue;
        }
        let gap = rng.range(st.lane_gap_m.0, st.lane_gap_m.1.min(widest));
        let long = rng.range(st.length_m.0, st.length_m.1.min(st.floor.0));
        let across = (rows - 1) as f32 * gap + 2.0 * st.edge_m;
        // How the floor is laid on the plot: which end the return lane runs down, which side
        // the start straight is on, and whether the lanes run along the plot or across it.
        // Eight ways in all, so two seeds of the same shape are not the same track.
        let flip = |rng: &mut Rng| if rng.chance(0.5) { 1.0f32 } else { -1.0 };
        let (fx, fz) = (flip(rng), flip(rng));
        let turn = rng.chance(0.5);
        // The floor's own frame: `u` along the lanes from the return end, `v` across from the
        // start straight. Every lane ends clear of the return lane by a lane's width.
        let ret = st.edge_m;
        let inner = ret + width + 5.0;
        let wall = long - st.edge_m;
        let run = st.lane_run_m;

        // Where each lane ends, `ret` either end: the lap leaves the return lane down lane one
        // and comes back to it down the last.
        let mut p: Vec<f32> = vec![ret];
        // The start straight holds the finish jump as well as the gates, and the fillets at
        // both ends take their radii off it.
        let lo = st.start_run_m.0.max(crate::trackprog::finish_straight_m(rules) + 14.0);
        let hi = st.start_run_m.1.min(wall - ret);
        if lo > hi {
            continue;
        }
        p.push(ret + rng.range(lo, hi));
        let mut dir = 1.0f32;
        let mut laid = true;
        for k in 1..n {
            if k == n - 1 {
                // Home: the last lane ends on the return lane, and has to be a run from it.
                laid = (run.0..=run.1).contains(&(p[k] - ret));
                p.push(ret);
                break;
            }
            // A lane's end, going one way or the other: inside the floor, a run from the lane
            // before it, and — for the one before last — a run from the return lane too.
            let window = |d: f32| {
                let (mut lo, mut hi) = if d > 0.0 {
                    (p[k] + run.0, p[k] + run.1)
                } else {
                    (p[k] - run.1, p[k] - run.0)
                };
                lo = lo.max(inner);
                hi = hi.min(wall);
                if k + 1 == n - 1 {
                    lo = lo.max(ret + run.0);
                    hi = hi.min(ret + run.1);
                }
                (lo <= hi).then_some((lo, hi))
            };
            // Turning back the way it came is a 180; carrying on is a step across on two 90s.
            // The first corner is always the holeshot 180.
            let first = if k == 1 || rng.chance(st.switchback) { -dir } else { dir };
            let Some((d, lo, hi)) = window(first)
                .map(|w| (first, w.0, w.1))
                .or_else(|| window(-first).map(|w| (-first, w.0, w.1)))
            else {
                laid = false;
                break;
            };
            p.push(rng.range(lo, hi));
            dir = d;
        }
        if !laid {
            continue;
        }
        let v = |r: usize| st.edge_m + r as f32 * gap;
        let mut corners: Vec<(f32, f32)> = vec![(p[0], v(0))];
        let mut row = 0usize;
        for k in 0..n - 1 {
            corners.push((p[k + 1], v(row)));
            row += skips[k];
            corners.push((p[k + 1], v(row)));
        }
        corners.push((p[n], v(row)));
        let (cx, cz) = (rules.plot.0 * 0.5, rules.plot.1 * 0.5);
        let world: Vec<(f32, f32)> = corners
            .iter()
            .map(|&(u, w)| {
                let (a, b) = (fx * (u - long * 0.5), fz * (w - across * 0.5));
                if turn {
                    (cx + b, cz + a)
                } else {
                    (cx + a, cz + b)
                }
            })
            .collect();
        let Some((segs, start)) = fillet(rng, &world, rules.apex_m) else { continue };
        let straights = segs.iter().filter_map(|s| match s {
            Segment::Straight { length, .. } => Some(*length),
            _ => None,
        });
        let longest = straights.fold(0.0f32, f32::max);
        let opening = match segs.first() {
            Some(Segment::Straight { length, .. }) => *length,
            _ => 0.0,
        };
        // Long enough for the finish jump as well as the start: the measured laps all end on one.
        let (lo, hi) = rules.start_straight_draw_m;
        let lo = lo.max(crate::trackprog::finish_straight_m(rules));
        if (lap.0..=lap.1).contains(&chain_length(&segs))
            && (lo..=hi).contains(&opening)
            && longest <= rules.run_max_m
        {
            return Some((segs, start));
        }
    }
    None
}

/// A closed polygon of right angles as straights and arcs: each corner rounded on a radius
/// drawn from `radius`, but never more than half the shorter leg beside it, so no two corners
/// overlap. The lap starts where the first leg leaves its corner.
fn fillet(
    rng: &mut Rng,
    corners: &[(f32, f32)],
    radius: (f32, f32),
) -> Option<(Vec<Segment>, Start)> {
    let n = corners.len();
    // Length and heading (the app's frame: 0 down +z, clockwise to +x) of the leg out of `i`.
    let leg = |i: usize| {
        let (a, b) = (corners[i], corners[(i + 1) % n]);
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        (dx.hypot(dz), dx.atan2(dz))
    };
    let r: Vec<f32> = (0..n)
        .map(|i| {
            let most = leg((i + n - 1) % n).0.min(leg(i).0) * 0.5;
            rng.range(radius.0, radius.1).min(most)
        })
        .collect();
    let mut segs = Vec::with_capacity(n * 2);
    for i in 0..n {
        let j = (i + 1) % n;
        let (len, h) = leg(i);
        let run = len - r[i] - r[j];
        if run < -1e-3 {
            return None;
        }
        if run > 0.05 {
            segs.push(Segment::Straight { length: run, rise: 0.0 });
        }
        let turn = (leg(j).1 - h + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        segs.push(Segment::Arc {
            radius: r[j] * turn.signum(),
            angle: turn.abs().to_degrees(),
            rise: 0.0,
        });
    }
    let h = leg(0).1;
    let (hx, hz) = crate::trackprog::heading_vector(h);
    let start = Start {
        x: corners[0].0 + hx * r[0],
        z: corners[0].1 + hz * r[0],
        angle: h.to_degrees().rem_euclid(360.0),
    };
    Some((segs, start))
}

/// What a lap came out measuring, once it was built.
#[derive(Debug, Clone)]
pub struct Measured {
    pub seed: u64,
    pub program: TrackProgram,
    /// Anything [`crate::trackllm::review`] had to say. Empty is the point.
    pub review: Vec<String>,
    /// And what the ground itself measures — see [`ground_notes`].
    pub ground: Vec<String>,
}

/// Measure the ground a lap actually makes, against what a published one makes.
///
/// The program can be right and the ground still wrong: a lap whose corners are all sweepers
/// wears no grooves, and one whose jumps sit where the ground already falls away stands them
/// at faces nobody can land. Neither shows up in the program — only in the terrain it
/// synthesises to — which is why this is worth the two seconds it costs.
pub fn ground_notes(prog: &TrackProgram) -> Vec<String> {
    let mut notes = Vec::new();
    // A start straight with no room beside the opening straight is laid across the lap's own
    // return leg: a tester found the gate row standing in the middle of the track. A drawn lap
    // like that is passed over rather than built.
    if let Some(line) = prog.start_line() {
        let need = crate::trackprog::StartLine::room_needed(prog.discipline.rules(), prog.width);
        if line.room < need {
            notes.push(format!(
                "the start straight has {:.0} m beside the opening straight and needs {need:.0}: \
                 its gate row would stand on the lap",
                line.room
            ));
        }
    }
    let Ok(syn) = crate::tracksynth::synthesise(prog) else {
        notes.push("the lap doesn't synthesise".into());
        return notes;
    };
    let grid = crate::trackstats::Grid {
        w: syn.gw,
        h: syn.gh,
        size_x: prog.terrain.size_x,
        size_z: prog.terrain.size_z,
        v: syn.heights.clone(),
    };
    let stations: Vec<(f32, f32, f32)> =
        syn.stations.iter().map(|st| (st.x, st.z, st.heading)).collect();
    if let Some(r) = crate::trackstats::rut_shape(&stations, crate::tracksynth::STATION_STEP, &grid)
    {
        // Indiana, measured: 2.2 grooves at 2.67 m, floor 0.90 m, chatter 0.007 m.
        if r.grooves < GROOVES.0 || r.grooves > GROOVES.1 {
            notes.push(format!(
                "the ground wears {:.1} grooves a section; a published track wears {}–{}",
                r.grooves, GROOVES.0, GROOVES.1
            ));
        }
        if r.chatter_m > CHATTER_M {
            notes.push(format!(
                "the surface is {:.3} m rough at the scale a wheel feels; a published track \
                 is under {CHATTER_M:.3}",
                r.chatter_m
            ));
        }
        if r.anisotropy < ANISOTROPY {
            notes.push(format!(
                "the ground is {:.2} times rougher across than along; ridden ground is at \
                 least {ANISOTROPY:.2} — under that it is shaken rather than driven on",
                r.anisotropy
            ));
        }
    }
    // Berms: the outside of a corner has to stand higher than its inside, or nothing on the
    // lap is bankable.
    let mut stood = 0usize;
    let mut corners = 0usize;
    for (i, st) in syn.stations.iter().enumerate().step_by(8) {
        let k = st.curvature;
        if k.abs() * FULL_LEAN < 0.6 {
            continue;
        }
        corners += 1;
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let half = prog.width * 0.5;
        let at = |t: f32| {
            let (x, z) = (st.x + rx * t, st.z + rz * t);
            crate::trackstats::Grid::at(&grid, x, z)
        };
        let outside = -k.signum() * (half - 0.5);
        let inside = k.signum() * (half - 0.5);
        if at(outside) > at(inside) + BERM_STAND_M {
            stood += 1;
        }
        let _ = i;
    }
    if corners > 0 && (stood as f32 / corners as f32) < BERM_SHARE {
        notes.push(format!(
            "only {stood} of {corners} corners grew a berm; a lap wants {:.0}% of them",
            BERM_SHARE * 100.0
        ));
    }
    notes
}

/// What the ground has to measure before a lap is worth building. All off published tracks —
/// see `trackstats::rut_shape` and the numbers in `tracksynth`.
const GROOVES: (f32, f32) = (1.4, 3.4);
const CHATTER_M: f32 = 0.012;
const ANISOTROPY: f32 = 1.35;
const BERM_STAND_M: f32 = 0.12;
const BERM_SHARE: f32 = 0.55;
/// The radius at which a corner is leaned on hard enough to bank — see `tracksynth`.
const FULL_LEAN: f32 = 22.0;

/// Draw laps until one passes, and say what the rejected ones got wrong.
///
/// Both halves have to hold: the program has to read like a published track's, and the ground
/// it makes has to measure like one. `ground` costs a synthesis apiece, so it is only asked
/// about laps that already pass review.
pub fn search(from: u64, tries: u32) -> Result<Measured, Vec<Measured>> {
    let mut rejected = Vec::new();
    for i in 0..tries as u64 {
        let seed = from + i;
        // A seed the walk paints itself in on is not a lap; it is the next seed's turn.
        let Some(mut program) = draw(seed) else { continue };
        crate::trackllm::repair_for_tests(&mut program);
        let review = crate::trackllm::review(&program);
        let mut notes = review.problems.clone();
        notes.extend(review.notes.clone());
        if !notes.is_empty() {
            rejected.push(Measured { seed, program, review: notes, ground: Vec::new() });
            continue;
        }
        let ground = ground_notes(&program);
        let out = Measured { seed, program, review: Vec::new(), ground };
        if out.ground.is_empty() {
            return Ok(out);
        }
        rejected.push(out);
    }
    Err(rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Northgate (seed 103) as the rider signed it off, 2026-09-13: "KEEP THIS LAYOUT". A change
    /// that moves its lap or what's built on it has to be ridden and signed off again.
    #[test]
    fn northgate_keeps_its_layout() {
        let p = match search(103, 1) { Ok(m) => m.program, Err(v) => v[0].program.clone() };
        assert_eq!(p.segments.len(), 90);
        assert!((p.lap_length() - 2271.3).abs() < 0.1, "lap {}", p.lap_length());
        assert_eq!(p.features.len(), 73);
        let at: f32 = p.features.iter().map(|f| f.at()).sum();
        assert!((at - 89644.8).abs() < 0.5, "features moved: {at}");
    }

    /// A seed that draws, for the tests that need one. Roughly nine in ten do.
    fn drawn(seed: u64) -> TrackProgram {
        (0..24)
            .find_map(|i| draw(seed + i))
            .unwrap_or_else(|| panic!("no seed near {seed} drew a lap"))
    }

    /// Corners, grouped the way `scripts/track-survey.py` groups a published one: same-handed
    /// turning under a 300 m radius, with under ten metres of run let into it.
    ///
    /// Both halves of that matter. Over 300 m is a sweeper — Motorcycling Australia's own
    /// definition of a curve is a direction change over 15 degrees with a radius under 300 —
    /// and counting arcs instead of corners counts a published corner four or five times over.
    /// Grouping any same-signed arc, which is what this used to do, reported Indiana as
    /// carrying 615-degree corners.
    fn corners(segs: &[Segment]) -> Vec<(f32, f32, f32, usize)> {
        const SWEEP: f32 = 300.0;
        const JOIN: f32 = 10.0;
        let mut out: Vec<(f32, f32, f32, usize)> = Vec::new(); // angle, ground, apex, arcs
        let (mut sign, mut gap) = (0.0f32, f32::MAX);
        for s in segs {
            let turning = matches!(s, Segment::Arc { radius, .. } if radius.abs() <= SWEEP);
            match s {
                Segment::Arc { radius, angle, .. } if turning => {
                    let run = radius.abs() * angle.to_radians();
                    if radius.signum() == sign && gap <= JOIN {
                        let c = out.last_mut().unwrap();
                        c.0 += angle;
                        c.1 += run + gap;
                        c.2 = c.2.min(radius.abs());
                        c.3 += 1;
                    } else {
                        out.push((*angle, run, radius.abs(), 1));
                    }
                    sign = radius.signum();
                    gap = 0.0;
                }
                _ => gap += seg_length(s),
            }
        }
        out.retain(|c| c.0 >= 25.0);
        out
    }

    fn median(mut v: Vec<f32>) -> f32 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    }

    /// The two properties the shape exists to guarantee, over a spread of seeds.
    #[test]
    fn every_lap_closes_and_none_crosses_itself() {
        for seed in [1u64, 7, 42, 777, 1234, 31337] {
            let mut p = drawn(seed);
            crate::trackllm::repair_for_tests(&mut p);
            // The walk docks onto its own start pose with a Dubins path it has walked, so
            // this is exact rather than nearly: it used to be allowed 20 m.
            assert!(
                p.closure_error() < 2.0,
                "seed {seed}: the lap misses itself by {:.1} m",
                p.closure_error()
            );
            let crossing = crate::trackllm::review(&p)
                .problems
                .into_iter()
                .find(|n| n.contains("crosses itself"));
            assert!(crossing.is_none(), "seed {seed}: {}", crossing.unwrap_or_default());
        }
    }

    /// And the shape of it, against what Indiana and Southwick measure.
    ///
    /// These are the numbers the walk exists to hit, so they are asserted rather than printed.
    /// The bands are the corpus's own, from `scripts/track-survey.py` over the eighteen laps
    /// in `~/Projects/pkz`; the medians of Indiana and Southwick sit in the middle of each.
    #[test]
    fn a_drawn_lap_is_shaped_like_a_published_one() {
        for seed in [1u64, 7, 42, 777, 1234, 31337] {
            let mut p = drawn(seed);
            crate::trackllm::repair_for_tests(&mut p);
            let lap = p.lap_length();
            let turning: f32 = p
                .segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Arc { radius, angle, .. } if radius.abs() <= 300.0 => {
                        Some(radius.abs() * angle.to_radians())
                    }
                    _ => None,
                })
                .sum();
            let gross: f32 = p
                .segments
                .iter()
                .map(|s| match s {
                    Segment::Arc { angle, .. } => angle.abs(),
                    _ => 0.0,
                })
                .sum();
            let cs = corners(&p.segments);
            let per_km = cs.len() as f32 / lap * 1000.0;
            let angle = median(cs.iter().map(|c| c.0).collect());
            let apex = median(cs.iter().map(|c| c.2).collect());
            let ground = median(cs.iter().map(|c| c.1).collect());
            let jumps = p.features.iter().map(|f| f.lips()).sum::<usize>() as f32 / lap * 1000.0;
            let tallest = p
                .features
                .iter()
                .map(|f| f.height())
                .fold(0.0f32, f32::max);

            let say = format!(
                "seed {seed}: {lap:.0} m, {} corners ({per_km:.1}/km), {angle:.0}° each, \
                 apex {apex:.1} m, {ground:.0} m of ground, {:.2} turning, {gross:.0}° gross, \
                 {jumps:.0} jumps/km, tallest {tallest:.1} m",
                cs.len(),
                turning / lap
            );
            // Indiana 2170, Southwick 2217; the corpus runs 1065-3055.
            assert!((1700.0..2600.0).contains(&lap), "{say}");
            // Indiana 7.4, Southwick 8.1; the corpus 7.4-10.3.
            // To 16, not 12: straights past 50 m are bent into sweepers, at a rider's asking.
            assert!((6.5..16.0).contains(&per_km), "{say}");
            // Indiana 159, Southwick 166; the corpus p50 90-166.
            // From 70, not 85: the sweepers long straights become are small bends of their own.
            assert!((70.0..185.0).contains(&angle), "{say}");
            // Indiana 10.4, Southwick 11.9; the corpus p50 7.1-18.5.
            assert!((7.0..20.0).contains(&apex), "{say}");
            // Indiana 68, Southwick 74; the corpus p50 14-82.
            assert!((40.0..95.0).contains(&ground), "{say}");
            // Indiana 0.63, Southwick 0.72.
            assert!((0.55..0.88).contains(&(turning / lap)), "{say}");
            // `trackllm::corpus::LIPS_PER_KM`, and the height ceiling two federations write.
            assert!((12.0..45.0).contains(&jumps), "{say}");
            assert!(tallest <= 3.0 + 1e-3, "{say}");
        }
    }

    /// What `random_track_program` hands the studio: a value with every defaulted field
    /// filled in, that parses straight back into the type.
    ///
    /// This is the trap [`crate::trackprog::BLANK`] documents from the other side. A track
    /// program has fields with serde defaults — `blend`, `elevation`, the ground's `wear` —
    /// and a value missing any of them puts an `undefined` into a number input the moment the
    /// studio loads it. Serialising the *type* is what fills them; nothing else does.
    #[test]
    fn a_random_track_arrives_whole() {
        let p = drawn(4242);
        let v = serde_json::to_value(&p).expect("serialises");
        for key in ["name", "author", "location", "width", "terrain", "start", "segments",
                    "features", "blend", "elevation"] {
            assert!(v.get(key).is_some(), "the studio is handed no `{key}`");
        }
        for key in ["sizeX", "sizeZ", "samples", "scale", "surface", "wear", "relief"] {
            assert!(v["terrain"].get(key).is_some(), "no `terrain.{key}`");
        }
        let back: TrackProgram = serde_json::from_value(v).expect("and parses back");
        assert_eq!(back.name, p.name);
        assert_eq!(back.segments.len(), p.segments.len());
        back.check().expect("and it is a track");
    }

    /// The same number gives the same track, on any machine. Seeds are how a track is named
    /// and found again, so this is not a detail.
    #[test]
    fn a_seed_draws_the_same_lap_every_time() {
        let a = drawn(99);
        let b = drawn(99);
        assert_eq!(a.name, b.name);
        assert_eq!(a.segments.len(), b.segments.len());
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    /// Most seeds draw. The walk backtracks, and this is what that bought: greedy, one seed in
    /// sixty produced a lap and the rest ran out of legal moves a fifth of the way round.
    #[test]
    fn most_seeds_draw_a_lap() {
        let drew = (200u64..240).filter(|s| draw(*s).is_some()).count();
        // Measured at 27-30 of 40. `random_track_program` searches forward from its seed, so
        // what this guards is a regression to the greedy walk's one in sixty.
        assert!(drew >= 24, "only {drew} of 40 seeds drew a lap");
    }

    /// Every setting at the bottom of its range.
    fn all_min() -> TrackSettings {
        TrackSettings {
            name: String::new(),
            location: String::new(),
            lap_length: 1700.0,
            width: 12.0,
            corners_per_km: 6.5,
            apex_radius: 8.0,
            sweep_share: 0.0,
            start_straight: 60.0,
            jump_density: 0.0,
            big_jump_share: 0.0,
            jump_scale: 0.8,
            waves: 0,
            surface: Surface::Soil,
            wear: 0.0,
            roughness: 1.0,
            hills: 2.0,
            tilt: 0.0,
            landforms: 0,
            elevation_changes: 0,
            discipline: None,
        }
    }

    /// And at the top.
    fn all_max() -> TrackSettings {
        TrackSettings {
            name: String::new(),
            location: String::new(),
            lap_length: 2300.0,
            width: 18.0,
            corners_per_km: 12.0,
            apex_radius: 18.0,
            sweep_share: 0.5,
            start_straight: 125.0,
            jump_density: 1.0,
            big_jump_share: 1.0,
            jump_scale: 1.0,
            waves: 2,
            surface: Surface::Grass,
            wear: 1.0,
            roughness: 2.0,
            hills: 10.0,
            tilt: 25.0,
            landforms: 5,
            elevation_changes: 4,
            discipline: None,
        }
    }

    /// How many of forty seeds draw a lap from these settings that review has no problem with.
    fn passing(s: &TrackSettings) -> usize {
        let knobs = LayoutKnobs::from_settings(s);
        (500u64..540)
            .filter(|seed| {
                let Some(mut p) = draw_with(*seed, &knobs) else { return false };
                crate::trackllm::repair_for_tests(&mut p);
                let r = crate::trackllm::review(&p);
                r.fatal.is_empty() && r.problems.is_empty()
            })
            .count()
    }

    /// A setting at the edge of its range still draws: the clamps are where the walk stops
    /// closing, not where a brief stops making sense.
    ///
    /// Ignored by default: each extreme repairs and reviews up to forty laps, nine minutes for
    /// the six. Run with `--ignored` after touching the walker or a clamp.
    fn holds_up(what: &str, s: TrackSettings) {
        let n = passing(&s);
        println!("{what}: {n} of 40");
        assert!(n >= 20, "{what}: only {n} of 40 seeds drew a lap review passes");
    }

    #[test]
    #[ignore = "slow: reviews forty laps"]
    fn settings_at_the_bottom_draw() {
        holds_up("all min", all_min());
    }

    #[test]
    #[ignore = "slow: reviews forty laps"]
    fn settings_at_the_top_draw() {
        holds_up("all max", all_max());
    }

    #[test]
    #[ignore = "slow: reviews forty laps"]
    fn a_rough_hilly_sand_track_draws() {
        holds_up(
            "sand, rough, hilly",
            TrackSettings {
                surface: Surface::Sand,
                roughness: 2.0,
                wear: 0.9,
                hills: 10.0,
                tilt: 20.0,
                landforms: 5,
                elevation_changes: 4,
                ..TrackSettings::default()
            },
        );
    }

    #[test]
    #[ignore = "slow: reviews forty laps"]
    fn a_packed_track_of_big_jumps_draws() {
        holds_up(
            "packed, big jumps",
            TrackSettings {
                jump_density: 1.0,
                big_jump_share: 1.0,
                jump_scale: 1.0,
                waves: 2,
                ..TrackSettings::default()
            },
        );
    }

    #[test]
    #[ignore = "slow: reviews forty laps"]
    fn a_long_lap_of_tight_corners_draws() {
        holds_up(
            "long lap, many corners",
            TrackSettings {
                lap_length: 2300.0,
                corners_per_km: 12.0,
                apex_radius: 8.0,
                ..TrackSettings::default()
            },
        );
    }

    #[test]
    #[ignore = "slow: reviews forty laps"]
    fn a_short_open_lap_draws() {
        holds_up(
            "short lap, few corners",
            TrackSettings {
                lap_length: 1700.0,
                corners_per_km: 6.5,
                apex_radius: 18.0,
                sweep_share: 0.5,
                ..TrackSettings::default()
            },
        );
    }

    /// What was asked for is what arrives.
    #[test]
    fn settings_are_respected() {
        for (lap_length, width, surface) in
            [(1800.0f32, 13.0f32, Surface::Sand), (2300.0, 17.5, Surface::Grass)]
        {
            let s = TrackSettings {
                name: "Test Park".into(),
                location: "Nowhere".into(),
                lap_length,
                width,
                surface,
                wear: 0.2,
                roughness: 1.6,
                ..TrackSettings::default()
            };
            let knobs = LayoutKnobs::from_settings(&s);
            let laps: Vec<TrackProgram> = (600u64..620).filter_map(|n| draw_with(n, &knobs)).collect();
            assert!(laps.len() >= 10, "only {} of 20 drew", laps.len());
            for p in &laps {
                assert_eq!(p.terrain.surface, surface);
                assert_eq!((p.name.as_str(), p.location.as_str()), ("Test Park", "Nowhere"));
                assert_eq!((p.terrain.wear, p.terrain.roughness), (0.2, 1.6));
                assert!((p.width - width).abs() <= 0.5, "width {} for {width}", p.width);
                let off = (p.lap_length() - lap_length).abs() / lap_length;
                assert!(off <= 0.08, "lap {:.0} m for {lap_length:.0}", p.lap_length());
            }
        }
    }

    /// Big jumps, full size, and still under the three metres two federations write.
    #[test]
    fn the_biggest_jumps_stay_under_three_metres() {
        let knobs = LayoutKnobs::from_settings(&TrackSettings {
            big_jump_share: 1.0,
            jump_scale: 1.0,
            jump_density: 1.0,
            ..TrackSettings::default()
        });
        let mut drew = 0;
        for seed in 700u64..730 {
            let Some(mut p) = draw_with(seed, &knobs) else { continue };
            drew += 1;
            crate::trackllm::repair_for_tests(&mut p);
            let tallest = p.features.iter().map(|f| f.height()).fold(0.0f32, f32::max);
            assert!(tallest <= 3.0 + 1e-3, "seed {seed}: {tallest:.2} m");
        }
        assert!(drew >= 15, "only {drew} of 30 drew");
    }

    /// The model writes camelCase JSON, and gets back what it wrote.
    #[test]
    fn settings_round_trip_through_json() {
        let s = TrackSettings { surface: Surface::Sand, name: "Ridge".into(), ..all_max() };
        let v = serde_json::to_value(&s).unwrap();
        for key in ["lapLength", "cornersPerKm", "apexRadius", "sweepShare", "startStraight",
                    "jumpDensity", "bigJumpShare", "jumpScale", "elevationChanges"] {
            assert!(v.get(key).is_some(), "no `{key}`");
        }
        assert_eq!(v["surface"], "sand");
        assert_eq!(serde_json::from_value::<TrackSettings>(v).unwrap(), s);
    }

    /// Out of range is pulled in, NaN takes the default, and a name is kept to a name.
    #[test]
    fn settings_are_clamped() {
        let s = TrackSettings {
            name: "x".repeat(200),
            lap_length: 9000.0,
            width: f32::NAN,
            hills: -3.0,
            waves: 9,
            ..TrackSettings::default()
        }
        .clamped();
        assert_eq!(s.name.chars().count(), 60);
        assert_eq!(s.lap_length, 2300.0);
        assert_eq!(s.width, TrackSettings::default().width);
        assert_eq!(s.hills, 2.0);
        assert_eq!(s.waves, 2);
    }

    fn sx(seed: u64) -> Option<TrackProgram> {
        draw_with(seed, &LayoutKnobs::for_discipline(Discipline::Sx))
    }

    /// Whether a lap's ground fits a floor, whichever way round the floor is laid: the lap is
    /// centred on the plot, so this measures what it covers against the floor's two sides.
    fn sx_on_the_floor(p: &TrackProgram) -> bool {
        let st = SX_RULES.stadium.expect("SX has a floor");
        let half = p.width * 0.5;
        let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for q in p.stations(2.0) {
            x0 = x0.min(q.x - half);
            x1 = x1.max(q.x + half);
            z0 = z0.min(q.z - half);
            z1 = z1.max(q.z + half);
        }
        let (w, h) = (x1 - x0, z1 - z0);
        let (long, across) = (st.floor.0 + 1.0, st.floor.1 + 1.0);
        // On the plot at all, and inside the floor one way round or the other.
        let on_plot = x0 >= 0.0 && z0 >= 0.0 && x1 <= SX_RULES.plot.0 && z1 <= SX_RULES.plot.1;
        on_plot && ((w <= long && h <= across) || (w <= across && h <= long))
    }

    /// Which way a lap's lanes run: `true` when its ground is wider than it is deep.
    fn sx_lies_along_x(p: &TrackProgram) -> bool {
        let st = p.stations(2.0);
        let w = st.iter().fold(f32::MIN, |a, q| a.max(q.x)) - st.iter().fold(f32::MAX, |a, q| a.min(q.x));
        let h = st.iter().fold(f32::MIN, |a, q| a.max(q.z)) - st.iter().fold(f32::MAX, |a, q| a.min(q.z));
        w > h
    }

    /// The corners a lap turns: how many, and how many of them are 180s.
    fn sx_corners(p: &TrackProgram) -> (usize, usize) {
        let turns = crate::trackprog::turns(&p.segments);
        let all = turns.iter().filter(|t| t.0 >= 25.0).count();
        (all, turns.iter().filter(|t| t.0 > 170.0).count())
    }

    /// The ground between a lap's corners, metres.
    ///
    /// Past the eight metres `trackprog::turns` merges across, so the metre or two of link
    /// inside a 180 is part of that corner rather than a run of its own — which is how the
    /// survey counts a published track's runs.
    fn sx_runs(p: &TrackProgram) -> Vec<f32> {
        p.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Straight { length, .. } if *length > 8.0 => Some(*length),
                _ => None,
            })
            .collect()
    }

    /// Fifty stadium laps: on the floor, riding line and all, with the lap, the corner mix and
    /// the spread of runs the measured SX tracks carry.
    #[test]
    fn sx_laps_fit_the_floor() {
        let turns_band = SX_RULES.review.turns;
        for seed in 0..50u64 {
            let p = sx(seed).unwrap_or_else(|| panic!("seed {seed} drew no SX lap"));
            assert_eq!(p.discipline, Discipline::Sx);
            assert!(sx_on_the_floor(&p), "seed {seed}: the lap is off the floor");
            let lap = p.lap_length();
            assert!((SX_RULES.lap_m.0..=SX_RULES.lap_m.1).contains(&lap), "seed {seed}: {lap:.0} m");
            let (turns, hairpins) = sx_corners(&p);
            assert!(
                (turns_band.0..=turns_band.1).contains(&(turns as f32)),
                "seed {seed}: {turns} corners"
            );
            // The measured tracks are 5–8 180s with 0–3 corners that are not: a lap of nothing
            // but 180s is the boustrophedon this walker exists not to draw.
            assert!((5..=8).contains(&hairpins), "seed {seed}: {hairpins} 180s of {turns}");
            assert!(hairpins < turns, "seed {seed}: every one of {turns} corners is a 180");
            // And its runs vary: every lane the same length is the same tell.
            let mut runs = sx_runs(&p);
            let mean = runs.iter().sum::<f32>() / runs.len() as f32;
            let spread =
                (runs.iter().map(|r| (r - mean).powi(2)).sum::<f32>() / runs.len() as f32).sqrt();
            assert!(spread > 10.0, "seed {seed}: runs spread {spread:.1} m, mean {mean:.0}");
            runs.sort_by(f32::total_cmp);
            // The corpus measures 57–77 m at the median and 90–130 at the longest.
            let median = runs[runs.len() / 2];
            assert!((40.0..=90.0).contains(&median), "seed {seed}: runs median {median:.0} m");
            let longest = *runs.last().expect("a lap has runs");
            assert!(longest <= SX_RULES.run_max_m, "seed {seed}: a {longest:.0} m run");
            // Exactly, up to what f32 accumulates walking thirty segments across a 240 m
            // plot. Review calls a lap open at 20 m.
            assert!(p.closure_error() < 0.5, "seed {seed}: misses by {:.2} m", p.closure_error());
        }
    }

    /// And the fifty are not one shape repeated: the corner mix differs, and so do the runs
    /// inside a lap. A boustrophedon of full-width lanes is one shape at one length, which is
    /// what this is here to catch.
    #[test]
    fn sx_laps_are_not_all_one_shape() {
        let mut mixes = std::collections::BTreeSet::new();
        let mut hairpin_counts = std::collections::BTreeSet::new();
        let mut medians = std::collections::BTreeSet::new();
        for seed in 0..50u64 {
            let p = sx(seed).expect("an SX lap");
            let (turns, hairpins) = sx_corners(&p);
            mixes.insert((turns, hairpins));
            hairpin_counts.insert(hairpins);
            let mut runs = sx_runs(&p);
            runs.sort_by(f32::total_cmp);
            medians.insert(runs[runs.len() / 2] as i32);
        }
        println!("50 seeds: corner mixes {mixes:?}, {} run medians", medians.len());
        assert!(mixes.len() >= 3, "only {} corner mixes: {mixes:?}", mixes.len());
        assert!(hairpin_counts.len() >= 2, "every lap has {hairpin_counts:?} 180s");
        // Lane lengths that vary lap to lap, not one shape stretched.
        assert!(medians.len() >= 10, "only {} distinct run medians", medians.len());
    }

    /// No two parts of a stadium lap share dirt: anything far apart round the lap stays more
    /// than a riding line apart on the floor.
    #[test]
    fn sx_laps_never_cross() {
        for seed in 0..50u64 {
            let p = sx(seed).expect("an SX lap");
            let st = p.stations(2.0);
            let lap = p.lap_length();
            for (i, a) in st.iter().enumerate() {
                for b in &st[i + 1..] {
                    let along = (b.s - a.s).abs();
                    if along.min(lap - along) < 60.0 {
                        continue;
                    }
                    let d = (a.x - b.x).hypot(a.z - b.z);
                    assert!(
                        d > p.width + 1.0,
                        "seed {seed}: {:.0} m and {:.0} m round are {d:.1} m apart",
                        a.s,
                        b.s
                    );
                }
            }
        }
    }

    /// And review has nothing blocking to say about them once repaired.
    #[test]
    fn sx_random_passes_review() {
        for seed in 0..8u64 {
            let mut p = sx(seed).expect("an SX lap");
            crate::trackllm::repair_for_tests(&mut p);
            let r = crate::trackllm::review(&p);
            println!("seed {seed}: {:.0} m, notes {:?}", p.lap_length(), r.notes);
            assert!(r.fatal.is_empty(), "seed {seed}: {:?}", r.fatal);
            assert!(r.problems.is_empty(), "seed {seed}: {:?}", r.problems);
            assert_eq!(p.discipline, Discipline::Sx, "repair keeps the discipline");
        }
    }

    /// SMX walks the outdoor lap on its own rules, and review holds it to them.
    #[test]
    fn smx_random_passes_review() {
        let knobs = LayoutKnobs::for_discipline(Discipline::Smx);
        let laps: Vec<TrackProgram> = (0..12u64).filter_map(|s| draw_with(s, &knobs)).take(4).collect();
        assert!(laps.len() >= 4, "only {} SMX laps drew", laps.len());
        for mut p in laps {
            crate::trackllm::repair_for_tests(&mut p);
            let r = crate::trackllm::review(&p);
            println!("{:.0} m, notes {:?}", p.lap_length(), r.notes);
            assert!(r.fatal.is_empty() && r.problems.is_empty(), "{:?} {:?}", r.fatal, r.problems);
            assert_eq!(p.terrain.size_x, SMX_RULES.plot.0);
        }
    }

    /// And they are not all laid the same way round: a floor goes on the plot under any of the
    /// eight symmetries, so the lanes run along the plot on some seeds and across it on others.
    #[test]
    fn sx_laps_are_laid_both_ways_round() {
        let along_x = (0..50u64).filter(|s| sx_lies_along_x(&sx(*s).expect("an SX lap"))).count();
        println!("{along_x} of 50 laps run along x");
        assert!((10..=40).contains(&along_x), "{along_x} of 50 laps run along x");
    }

    /// A discipline is written only when it isn't motocross, and read back as written; a
    /// program or settings without one is motocross.
    #[test]
    fn the_discipline_round_trips() {
        let p = sx(1).expect("an SX lap");
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["discipline"], "sx");
        let back: TrackProgram = serde_json::from_value(v).unwrap();
        assert_eq!(back.discipline, Discipline::Sx);

        let mx = drawn(4242);
        let v = serde_json::to_value(&mx).unwrap();
        assert!(v.get("discipline").is_none(), "MX writes no discipline");
        assert_eq!(serde_json::from_value::<TrackProgram>(v).unwrap().discipline, Discipline::Mx);

        let s = TrackSettings { discipline: Some(Discipline::Smx), ..TrackSettings::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["discipline"], "smx");
        assert_eq!(serde_json::from_value::<TrackSettings>(v).unwrap(), s);
        let plain = serde_json::to_value(TrackSettings::default()).unwrap();
        assert!(plain.get("discipline").is_none());
    }

    /// Settings for another discipline draw that discipline's lap, with the brief's character.
    #[test]
    fn sx_settings_draw_an_sx_lap() {
        let s = TrackSettings {
            name: "Stadium".into(),
            surface: Surface::Sand,
            discipline: Some(Discipline::Sx),
            ..TrackSettings::default()
        };
        let knobs = LayoutKnobs::from_settings(&s);
        let p = (0..8u64).find_map(|n| draw_with(n, &knobs)).expect("an SX lap from settings");
        assert_eq!(p.discipline, Discipline::Sx);
        assert_eq!((p.name.as_str(), p.terrain.surface), ("Stadium", Surface::Sand));
        assert!((SX_RULES.lap_m.0..=SX_RULES.lap_m.1).contains(&p.lap_length()));
    }

    /// Which features of a lap are rhythm lanes: a drawn shape with four crests or more.
    /// Three is a triple and one is a table, so the bar sits above both.
    fn rhythm_lanes(p: &TrackProgram) -> Vec<&Feature> {
        p.features
            .iter()
            .filter(|f| matches!(f, Feature::Custom { .. }) && f.lips() >= 4)
            .collect()
    }

    /// Fifty stadium laps, against the sections they were measured into.
    ///
    /// The counts are asserted as ceilings and the shapes as bands. A lane either holds a
    /// section or it does not — a lap whose lanes came out short is allowed to carry fewer
    /// than the median, and that is a different thing from carrying one built wrong.
    #[test]
    fn sx_laps_carry_the_sections_they_were_measured_from() {
        let sec = SX_RULES.sections.expect("supercross is built from sections");
        let (mut rhythm_total, mut whoop_total, mut with_sand, mut triples) = (0usize, 0usize, 0usize, 0usize);
        for seed in 0..50u64 {
            let p = sx(seed).unwrap_or_else(|| panic!("seed {seed} drew no SX lap"));
            let lap = p.lap_length();

            // Nothing may run off the end of the lap, and nothing stands on the gate row.
            for f in &p.features {
                assert!(f.at() >= 0.0, "seed {seed}: a {} at {:.0} m", f.name(), f.at());
                assert!(
                    f.at() + f.length() <= lap + 0.5,
                    "seed {seed}: a {} ends {:.0} m past a {lap:.0} m lap",
                    f.name(),
                    f.at() + f.length() - lap
                );
            }

            let whoops: Vec<&Feature> =
                p.features.iter().filter(|f| matches!(f, Feature::Whoops { .. })).collect();
            assert!(
                whoops.len() as u32 <= sec.whoop_sections.1,
                "seed {seed}: {} whoop sections",
                whoops.len()
            );
            for f in &whoops {
                let Feature::Whoops { at, count, spacing, height } = f else {
                    unreachable!()
                };
                assert!((sec.whoops.0..=sec.whoops.1).contains(count), "seed {seed}: {count} whoops");
                assert!(
                    (sec.whoop_pitch_m.0..=sec.whoop_pitch_m.1).contains(spacing),
                    "seed {seed}: whoops {spacing:.2} m apart"
                );
                assert!(
                    (sec.whoop_h_m.0..=sec.whoop_h_m.1).contains(height),
                    "seed {seed}: whoops {height:.2} m tall"
                );
                // Out of a 180 in the second half of the lap, which is where all seven of the
                // measured rounds put theirs. Never the first thing after the gate.
                assert!(*at > lap * 0.45, "seed {seed}: whoops at {at:.0} m of {lap:.0}");
            }
            whoop_total += whoops.len();

            let lanes = rhythm_lanes(&p);
            assert!(
                lanes.len() as u32 <= sec.rhythm_lanes.1,
                "seed {seed}: {} rhythm lanes",
                lanes.len()
            );
            for f in &lanes {
                let Feature::Custom { length, shape, .. } = f else {
                    unreachable!()
                };
                assert!(
                    *length <= sec.rhythm_m.1 + 12.0,
                    "seed {seed}: a {length:.0} m rhythm lane"
                );
                let tall = shape.iter().map(|q| q.h).fold(0.0f32, f32::max);
                assert!(
                    (sec.rhythm_h_m.0 - 0.01..=sec.rhythm_h_m.1 + 0.01).contains(&tall),
                    "seed {seed}: a rhythm hill {tall:.2} m tall"
                );
                // The hills come back down between crests, or it is a wave and not a lane.
                let floor = shape.iter().map(|q| q.h).fold(f32::MAX, f32::min);
                assert!(floor < tall * 0.25, "seed {seed}: troughs at {floor:.2} of {tall:.2}");
            }
            rhythm_total += lanes.len();
            triples += p.features.iter().filter(|f| matches!(f, Feature::Custom { .. }) && f.lips() == 3).count();
            with_sand += usize::from(p.features.iter().any(|f| matches!(f, Feature::Sand { .. })));
        }
        println!(
            "50 SX laps: {rhythm_total} rhythm lanes, {whoop_total} whoop sections, \
             {triples} triples, {with_sand} laps with sand"
        );
        // Every lap should carry a rhythm lane or two, and most of them the whoops.
        assert!(rhythm_total >= 50, "only {rhythm_total} rhythm lanes over fifty laps");
        assert!(whoop_total >= 25, "only {whoop_total} whoop sections over fifty laps");
        // Sand on about half the seeds, which is what the round previews report.
        assert!((8..=42).contains(&with_sand), "{with_sand} of fifty laps carry sand");
    }

    /// SuperMotocross has its own shape, and it is not motocross drawn smaller.
    #[test]
    fn smx_laps_are_shaped_like_the_measured_rounds() {
        let knobs = LayoutKnobs::for_discipline(Discipline::Smx);
        let laps: Vec<TrackProgram> = (0..40u64).filter_map(|s| draw_with(s, &knobs)).collect();
        assert!(laps.len() >= 8, "only {} SMX laps drew of forty seeds", laps.len());
        let mut hairpins = Vec::new();
        for p in &laps {
            let lap = p.lap_length();
            assert!(
                (SMX_RULES.lap_m.0..=SMX_RULES.lap_m.1).contains(&lap),
                "a {lap:.0} m SMX lap"
            );
            // No supercross whoops: they belong to the stadium, and none of the three measured
            // SuperMotocross laps carries a set.
            assert!(
                !p.features.iter().any(|f| matches!(f, Feature::Whoops { .. })),
                "an SMX lap with whoops on it"
            );
            // How many corners a lap turns is review's business, and `smx_random_passes_review`
            // holds it to the band. What is asserted here is the thing review has no opinion
            // about: how many of them are 180s, which is what makes it look like SMX.
            let turns = crate::trackprog::turns(&p.segments);
            hairpins.push(turns.iter().filter(|t| t.0 > 150.0).count());
        }
        let median = {
            let mut h = hairpins.clone();
            h.sort_unstable();
            h[h.len() / 2]
        };
        println!("{} SMX laps: 180s {hairpins:?}", laps.len());
        // The measured three carry 7–10 of their 12–22 corners as 180s. The walk reaches that
        // through `hairpin_share` rather than by accident, so the median is what is asserted —
        // an individual lap is allowed to come out either side of the band.
        assert!((4..=14).contains(&median), "median {median} hairpins a lap");
    }

    /// And motocross draws exactly what it drew before: no sections, and the same mix.
    #[test]
    fn motocross_still_places_its_features_the_old_way() {
        assert!(MX_RULES.sections.is_none(), "motocross is not built from sections");
        let p = drawn(103);
        assert!(
            !p.features.iter().any(|f| matches!(f, Feature::Sand { .. })),
            "a national with a sand section on it"
        );
        // `northgate_keeps_its_layout` pins the seed the rider signed off; this is the cheaper
        // check that the walk did not start rolling for a discipline that makes no rolls.
        assert_eq!(p.discipline, Discipline::Mx);
    }

    /// The whole point: a seed range yields a lap that passes both halves.
    ///
    /// Slow — every candidate that passes review is synthesised and measured — so it runs on
    /// demand rather than in the suite.
    ///
    /// ```text
    /// cargo test -p mxb-app --bin mxb-app -- --ignored --nocapture a_search_finds_a_lap_that_measures_up
    /// ```
    #[test]
    #[ignore = "synthesises every candidate — slow"]
    fn a_search_finds_a_lap_that_measures_up() {
        match search(1000, 12) {
            Ok(m) => {
                println!(
                    "seed {}: {} — {:.0} m, {} segments, {} jumps",
                    m.seed,
                    m.program.name,
                    m.program.lap_length(),
                    m.program.segments.len(),
                    m.program.features.len()
                );
            }
            Err(tried) => {
                for m in &tried {
                    println!(
                        "  seed {} rejected: {}",
                        m.seed,
                        m.review.iter().chain(m.ground.iter()).cloned().collect::<Vec<_>>().join("; ")
                    );
                }
                panic!("no lap out of {} passed", tried.len());
            }
        }
    }
}

#[cfg(test)]
mod corner_shape_tests {
    use super::*;

    /// Write drawn programmes where `scripts/track-survey.py` can measure them, which is how
    /// the Rust walk is checked against the Python one it was ported from.
    ///
    /// ```text
    /// LAYOUT_OUT=/tmp/laps LAYOUT_SEEDS=40 \
    ///   cargo test --bin mxb-app -- --ignored --nocapture dump_programs
    /// ```
    #[test]
    #[ignore]
    fn dump_programs() {
        let Ok(dir) = std::env::var("LAYOUT_OUT") else {
            println!("set LAYOUT_OUT");
            return;
        };
        let n: u64 = std::env::var("LAYOUT_SEEDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40);
        let from: u64 = std::env::var("LAYOUT_FROM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        // `LAYOUT_DISCIPLINE=sx` (or smx) for another discipline's laps.
        let discipline: Discipline = std::env::var("LAYOUT_DISCIPLINE")
            .ok()
            .and_then(|d| serde_json::from_value(serde_json::Value::String(d)).ok())
            .unwrap_or_default();
        let knobs = LayoutKnobs::for_discipline(discipline);
        std::fs::create_dir_all(&dir).expect("create");
        let mut drew = 0;
        for seed in from..from + n {
            let Some(p) = draw_with(seed, &knobs) else {
                println!("seed {seed}: painted in");
                continue;
            };
            drew += 1;
            let out = std::path::Path::new(&dir).join(format!("seed{seed}.json"));
            std::fs::write(&out, serde_json::to_vec_pretty(&p).unwrap()).expect("write");
            println!(
                "seed {seed}: {} — {:.0} m, {} segments, {} features",
                p.name,
                p.lap_length(),
                p.segments.len(),
                p.features.len()
            );
        }
        println!("{drew} of {n} seeds drew a lap; programmes in {dir}");
    }

    /// Print a drawn lap as a `trackprog::EXAMPLE` literal — this is how the base track is
    /// made, so it can be remade when the walk changes rather than being hand-kept.
    ///
    /// ```text
    /// BASE_SEED=24 cargo test --bin mxb-app -- --ignored --nocapture emit_base_track
    /// ```
    #[test]
    #[ignore]
    fn emit_base_track() {
        let seed: u64 = std::env::var("BASE_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);
        let mut p = draw(seed).expect("that seed paints itself in");
        p.name = "Corpus National".into();
        p.location = "Generated".into();
        // Left exactly as the repair pass leaves it, so the base track loads with nothing to
        // say about it: begun on a straight, on ground sized and centred for where the lap
        // ends up rather than where it started.
        crate::trackllm::repair_for_tests(&mut p);
        let n = |v: f32| {
            let t = format!("{v:.4}");
            let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
            if t == "-0" { "0".into() } else { t }
        };
        let mut out = String::from("pub const EXAMPLE: &str = r#\"{\n");
        out += &format!("      \"name\": \"{}\",\n", p.name);
        out += &format!("      \"author\": \"{}\",\n", p.author);
        out += &format!("      \"location\": \"{}\",\n", p.location);
        out += &format!("      \"width\": {},\n", n(p.width));
        out += "      \"terrain\": {\n";
        out += &format!(
            "        \"sizeX\": {}, \"sizeZ\": {}, \"samples\": {}, \"scale\": {}, \"surface\": \"{}\",\n",
            n(p.terrain.size_x), n(p.terrain.size_z), p.terrain.samples, n(p.terrain.scale),
            serde_json::to_value(p.terrain.surface).unwrap().as_str().unwrap()
        );
        let r = &p.terrain.relief;
        out += &format!(
            "        \"relief\": {{ \"amplitude\": {}, \"wavelength\": {}, \"seed\": {}, \"texture\": {}, \"tilt\": {}, \"tiltAngle\": {}, \"landforms\": {}, \"landformHeight\": {} }}\n",
            n(r.amplitude), n(r.wavelength), r.seed, n(r.texture), n(r.tilt),
            n(r.tilt_angle), r.landforms, n(r.landform_height)
        );
        out += "      },\n";
        out += &format!(
            "      \"start\": {{ \"x\": {}, \"z\": {}, \"angle\": {} }},\n",
            n(p.start.x), n(p.start.z), n(p.start.angle)
        );
        out += "      \"segments\": [\n";
        let segs: Vec<String> = p
            .segments
            .iter()
            .map(|s| match s {
                Segment::Straight { length, .. } => {
                    format!("        {{ \"kind\": \"straight\", \"length\": {} }}", n(*length))
                }
                Segment::Arc { radius, angle, .. } => format!(
                    "        {{ \"kind\": \"arc\", \"radius\": {}, \"angle\": {} }}",
                    n(*radius),
                    n(*angle)
                ),
            })
            .collect();
        out += &segs.join(",\n");
        out += "\n      ],\n      \"features\": [\n";
        let feats: Vec<String> = p
            .features
            .iter()
            .map(|f| {
                let v = serde_json::to_value(f).unwrap();
                let o = v.as_object().unwrap();
                let body: Vec<String> = o
                    .iter()
                    .map(|(k, val)| {
                        let val = match (val.as_f64(), val.as_array()) {
                            (Some(x), _) => n(x as f32),
                            (_, Some(pts)) => {
                                let marks: Vec<String> = pts
                                    .iter()
                                    .map(|m| {
                                        let m = m.as_object().unwrap();
                                        format!(
                                            "{{ \"u\": {}, \"h\": {} }}",
                                            n(m["u"].as_f64().unwrap() as f32),
                                            n(m["h"].as_f64().unwrap() as f32)
                                        )
                                    })
                                    .collect();
                                format!("[{}]", marks.join(", "))
                            }
                            _ => serde_json::to_string(val).unwrap(),
                        };
                        format!("\"{k}\": {val}")
                    })
                    .collect();
                format!("        {{ {} }}", body.join(", "))
            })
            .collect();
        out += &feats.join(",\n");
        out += "\n      ]\n    }\"#;\n";
        println!("{out}");

        let review = crate::trackllm::review(&p);
        println!(
            "\n// seed {seed}: {:.0} m, {} segments, {} features, closes to {:.2} m",
            p.lap_length(),
            p.segments.len(),
            p.features.len(),
            p.closure_error()
        );
        for note in review.problems.iter().chain(review.notes.iter()) {
            println!("// REVIEW: {note}");
        }
        if review.problems.is_empty() && review.notes.is_empty() {
            println!("// review: nothing to say");
        }
    }

    /// Write a seed's `.trh` where the comparison scripts can read it.
    /// `TRH_OUT=/path/dir cargo test -- --ignored --nocapture dump_trh`
    #[test]
    #[ignore]
    fn dump_trh() {
        let Ok(dir) = std::env::var("TRH_OUT") else {
            println!("set TRH_OUT");
            return;
        };
        let seed: u64 = std::env::var("TRH_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(16);
        let mut p = draw(seed).expect("that seed paints itself in");
        crate::trackllm::repair_for_tests(&mut p);
        let syn = crate::tracksynth::synthesise(&p).expect("synthesise");
        let bytes = crate::tracksynth::trh(&p, &syn, true);
        let out = std::path::Path::new(&dir).join(format!("seed{seed}.trh"));
        std::fs::write(&out, &bytes).expect("write");
        println!("wrote {} ({} bytes) name={} lap={:.0} m", out.display(), bytes.len(), p.name, p.lap_length());
    }
}
