use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

/// The largest rider field accepted by the headless bootstrap parser.
pub const MAX_STALLS: usize = 50;

/// A timing gate expressed against one of the track's centre lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimingLine {
    pub line: u32,
    pub long: f32,
    pub left: f32,
    pub right: f32,
}

/// A rider position expressed in centre-line coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stall {
    pub long: f32,
    pub lat: f32,
    pub angle: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PitLane {
    pub start_stalls: Vec<Stall>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PitBoard {
    pub height: f32,
    pub stalls: Vec<Stall>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StartingGrid {
    pub stalls: Vec<Stall>,
}

/// The thirty-seconds board position expressed in centre-line coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThirtySecondsBoard {
    pub long: f32,
    pub lat: f32,
    pub angle: f32,
}

/// The subset of an RDF required to construct fresh dedicated-server bootstrap state.
#[derive(Clone, Debug, PartialEq)]
pub struct RdfBootstrap {
    pub finish: TimingLine,
    pub split1: TimingLine,
    pub split2: TimingLine,
    /// Absent from some community tracks that beta21e still loads.
    pub holeshot: Option<TimingLine>,
    pub pit_lane: PitLane,
    pub pit_board: PitBoard,
    pub starting_grid: StartingGrid,
    pub thirty_seconds_board: ThirtySecondsBoard,
}

impl RdfBootstrap {
    /// Parse a text RDF, requiring every bootstrap field exactly once. `holeshot` alone may be
    /// absent, but it is never accepted duplicated or malformed.
    pub fn parse(text: &str) -> Result<Self> {
        let document = Document::parse(text)?;
        let finish = timing_line(document.one_block("finish_line")?, "finish_line")?;
        let split1 = timing_line(document.one_block("split1")?, "split1")?;
        let split2 = timing_line(document.one_block("split2")?, "split2")?;
        let holeshot = document
            .optional_block("holeshot")?
            .map(|block| timing_line(block, "holeshot"))
            .transpose()?;

        let pit_lane_block = document.one_block("pit_lane")?;
        let pit_lane_count = stall_count(pit_lane_block, "pit_lane")?;
        let pit_lane = PitLane {
            start_stalls: indexed_stalls(
                pit_lane_block,
                "start_stall",
                pit_lane_count,
                "pit_lane",
            )?,
        };

        let pit_board_block = document.one_block("pit_board")?;
        let pit_board = PitBoard {
            height: finite_f32(pit_board_block.one_value("height")?, "pit_board.height")?,
            // The RDF format carries no second pit-board count. The dedicated server indexes
            // this table with the pit-lane rider count, so require the same complete range.
            stalls: indexed_stalls(pit_board_block, "stall", pit_lane_count, "pit_board")?,
        };

        let starting_grid_block = document.one_block("starting_grid")?;
        let starting_grid_count = stall_count(starting_grid_block, "starting_grid")?;
        let starting_grid = StartingGrid {
            stalls: indexed_stalls(
                starting_grid_block,
                "stall",
                starting_grid_count,
                "starting_grid",
            )?,
        };

        let thirty_seconds_board_block = document.one_block("30seconds_board")?;
        let thirty_seconds_board = ThirtySecondsBoard {
            long: finite_f32(
                thirty_seconds_board_block.one_value("long")?,
                "30seconds_board.long",
            )?,
            lat: finite_f32(
                thirty_seconds_board_block.one_value("lat")?,
                "30seconds_board.lat",
            )?,
            angle: finite_f32(
                thirty_seconds_board_block.one_value("angle")?,
                "30seconds_board.angle",
            )?,
        };

        Ok(Self {
            finish,
            split1,
            split2,
            holeshot,
            pit_lane,
            pit_board,
            starting_grid,
            thirty_seconds_board,
        })
    }
}

fn timing_line(block: &Scope, name: &str) -> Result<TimingLine> {
    Ok(TimingLine {
        line: block
            .one_value("line")?
            .parse()
            .with_context(|| format!("{name}.line is not an unsigned integer"))?,
        long: finite_f32(block.one_value("long")?, &format!("{name}.long"))?,
        left: finite_f32(block.one_value("left")?, &format!("{name}.left"))?,
        right: finite_f32(block.one_value("right")?, &format!("{name}.right"))?,
    })
}

fn stall_count(block: &Scope, name: &str) -> Result<usize> {
    let count: usize = block
        .one_value("numstalls")?
        .parse()
        .with_context(|| format!("{name}.numstalls is not an unsigned integer"))?;
    if !(1..=MAX_STALLS).contains(&count) {
        bail!("{name}.numstalls must be between 1 and {MAX_STALLS}, got {count}");
    }
    Ok(count)
}

fn indexed_stalls(block: &Scope, prefix: &str, count: usize, owner: &str) -> Result<Vec<Stall>> {
    let mut indexed = BTreeMap::new();
    for entry in &block.entries {
        let name = entry.name();
        let Some(suffix) = name.strip_prefix(prefix) else {
            continue;
        };
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("{owner} contains malformed indexed block {name:?}");
        }
        let index: usize = suffix
            .parse()
            .with_context(|| format!("{owner} has an invalid stall index in {name:?}"))?;
        if index >= count || index >= MAX_STALLS {
            bail!("{owner} stall index {index} is outside 0..{count}");
        }
        let Entry::Block { scope, .. } = entry else {
            bail!("{owner}.{name} must be a block");
        };
        if indexed.insert(index, scope).is_some() {
            bail!("{owner} contains duplicate {name}");
        }
    }

    (0..count)
        .map(|index| {
            let scope = indexed
                .remove(&index)
                .with_context(|| format!("{owner} is missing {prefix}{index}"))?;
            let field = |key: &str| -> Result<f32> {
                finite_f32(
                    scope.one_value(key)?,
                    &format!("{owner}.{prefix}{index}.{key}"),
                )
            };
            Ok(Stall {
                long: field("long")?,
                lat: field("lat")?,
                angle: field("angle")?,
            })
        })
        .collect()
}

fn finite_f32(raw: &str, field: &str) -> Result<f32> {
    let value: f32 = raw
        .parse()
        .with_context(|| format!("{field} is not a number"))?;
    if !value.is_finite() {
        bail!("{field} must be finite");
    }
    Ok(value)
}

#[derive(Debug)]
struct Document(Scope);

impl Document {
    fn parse(text: &str) -> Result<Self> {
        let lines: Vec<_> = text
            .strip_prefix('\u{feff}')
            .unwrap_or(text)
            .lines()
            .enumerate()
            .filter_map(|(index, raw)| {
                let without_comment = raw.split_once("//").map_or(raw, |(before, _)| before);
                let line = without_comment.trim();
                (!line.is_empty()).then_some((index + 1, line))
            })
            .collect();
        let mut at = 0;
        let scope = parse_scope(&lines, &mut at, false)?;
        if at != lines.len() {
            bail!("unexpected RDF content after top-level scope");
        }
        Ok(Self(scope))
    }

    fn one_block(&self, name: &str) -> Result<&Scope> {
        self.0.one_block(name)
    }

    fn optional_block(&self, name: &str) -> Result<Option<&Scope>> {
        if self.0.entries.iter().any(|entry| entry.name() == name) {
            self.0.one_block(name).map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
struct Scope {
    entries: Vec<Entry>,
}

impl Scope {
    fn one_block(&self, wanted: &str) -> Result<&Scope> {
        let mut matches = self.entries.iter().filter(|entry| entry.name() == wanted);
        let entry = matches
            .next()
            .with_context(|| format!("RDF is missing {wanted} block"))?;
        if matches.next().is_some() {
            bail!("RDF contains duplicate {wanted} blocks");
        }
        match entry {
            Entry::Block { scope, .. } => Ok(scope),
            Entry::Value { .. } => bail!("RDF {wanted} must be a block"),
        }
    }

    fn one_value(&self, wanted: &str) -> Result<&str> {
        let mut matches = self.entries.iter().filter(|entry| entry.name() == wanted);
        let entry = matches
            .next()
            .with_context(|| format!("RDF is missing {wanted} value"))?;
        if matches.next().is_some() {
            bail!("RDF contains duplicate {wanted} values");
        }
        match entry {
            Entry::Value { value, .. } => Ok(value),
            Entry::Block { .. } => bail!("RDF {wanted} must be a value"),
        }
    }
}

#[derive(Debug)]
enum Entry {
    Value { name: String, value: String },
    Block { name: String, scope: Scope },
}

impl Entry {
    fn name(&self) -> &str {
        match self {
            Self::Value { name, .. } | Self::Block { name, .. } => name,
        }
    }
}

fn parse_scope(lines: &[(usize, &str)], at: &mut usize, nested: bool) -> Result<Scope> {
    let mut entries = Vec::new();
    while let Some(&(line_number, line)) = lines.get(*at) {
        if line == "}" {
            if !nested {
                bail!("unexpected closing brace on RDF line {line_number}");
            }
            *at += 1;
            return Ok(Scope { entries });
        }
        if line == "{" {
            bail!("orphan opening brace on RDF line {line_number}");
        }
        if let Some((name, value)) = line.split_once('=') {
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() || value.contains('=') {
                bail!("malformed RDF assignment on line {line_number}");
            }
            entries.push(Entry::Value {
                name: name.to_string(),
                value: value.to_string(),
            });
            *at += 1;
            continue;
        }
        if line.contains(['{', '}']) {
            bail!("malformed RDF block name on line {line_number}");
        }
        let name = line.to_string();
        *at += 1;
        let Some(&(brace_line, "{")) = lines.get(*at) else {
            bail!("RDF block {name:?} on line {line_number} is not followed by an opening brace");
        };
        let _ = brace_line;
        *at += 1;
        entries.push(Entry::Block {
            name,
            scope: parse_scope(lines, at, true)?,
        });
    }
    if nested {
        bail!("unterminated RDF block");
    }
    Ok(Scope { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
type = 3
finish_line
{
    line = 0
    long = 100.5
    left = -6
    right = 6
}
holeshot
{
    line = 0
    long = 12
    left = -5
    right = 5
}
split1
{
    line = 0
    long = 30
    left = -4
    right = 4
}
split2
{
    line = 0
    long = 60
    left = -4.5
    right = 4.5
}
pit_lane
{
    numstalls = 2
    start_stall0
    {
        long = 1
        lat = 2
        angle = 3
    }
    start_stall1
    {
        long = 4
        lat = 5
        angle = 6
    }
}
pit_board
{
    height = 1.5
    stall0
    {
        long = 7
        lat = 8
        angle = 9
    }
    stall1
    {
        long = 10
        lat = 11
        angle = 12
    }
}
starting_grid
{
    numstalls = 2
    stall0
    {
        long = 13
        lat = 14
        angle = 15
    }
    stall1
    {
        long = 16
        lat = 17
        angle = 18
    }
}
30secondsboard_posx = 20
30secondsboard_posz = 21.25
30secondsboard_angle = -75
30seconds_board
{
    long = 22
    lat = -3.5
    angle = 323.1
}
"#;

    #[test]
    fn parses_the_complete_bootstrap_subset() {
        let rdf = RdfBootstrap::parse(VALID).unwrap();
        assert_eq!(rdf.finish.long, 100.5);
        assert_eq!(rdf.holeshot.map(|line| line.long), Some(12.0));
        assert_eq!(rdf.pit_lane.start_stalls[1].angle, 6.0);
        assert_eq!(rdf.pit_board.height, 1.5);
        assert_eq!(rdf.pit_board.stalls.len(), 2);
        assert_eq!(rdf.starting_grid.stalls[0].lat, 14.0);
        assert_eq!(rdf.thirty_seconds_board.long, 22.0);
        assert_eq!(rdf.thirty_seconds_board.lat, -3.5);
        assert_eq!(rdf.thirty_seconds_board.angle, 323.1);
    }

    #[test]
    fn rejects_missing_duplicate_and_non_finite_fields() {
        assert!(RdfBootstrap::parse(&VALID.replace("split2", "unused")).is_err());

        let duplicate = VALID.replace("split2\n{", "split2\n{\n}\nsplit2\n{");
        assert!(RdfBootstrap::parse(&duplicate).is_err());

        assert!(RdfBootstrap::parse(&VALID.replace("angle = 323.1", "angle = NaN")).is_err());
    }

    #[test]
    fn holeshot_is_optional_but_strict_when_present() {
        let start = VALID.find("holeshot\n").unwrap();
        let end = VALID.find("split1\n").unwrap();
        let without = format!("{}{}", &VALID[..start], &VALID[end..]);
        let rdf = RdfBootstrap::parse(&without).unwrap();
        assert_eq!(rdf.holeshot, None);
        assert_eq!(rdf.split1.long, 30.0);

        let duplicate = VALID.replace("holeshot\n{", "holeshot\n{\n}\nholeshot\n{");
        assert!(RdfBootstrap::parse(&duplicate).is_err());
        assert!(RdfBootstrap::parse(&VALID.replace("long = 12", "long = NaN")).is_err());
        assert!(RdfBootstrap::parse(&VALID.replace("left = -5", "unused = -5")).is_err());
        assert!(
            RdfBootstrap::parse(&VALID.replace("holeshot\n{", "holeshot = 1\nunused\n{")).is_err()
        );
    }

    #[test]
    fn rejects_out_of_range_missing_duplicate_and_extra_stalls() {
        assert!(RdfBootstrap::parse(&VALID.replace("numstalls = 2", "numstalls = 51")).is_err());
        assert!(RdfBootstrap::parse(&VALID.replace("start_stall1", "unused1")).is_err());
        assert!(RdfBootstrap::parse(&VALID.replace("start_stall1", "start_stall0")).is_err());
        assert!(RdfBootstrap::parse(&VALID.replace("start_stall1", "start_stall2")).is_err());
    }

    #[test]
    fn rejects_malformed_structure_instead_of_partially_parsing_it() {
        assert!(RdfBootstrap::parse(&VALID.replace("split1\n{", "split1 {")).is_err());
        assert!(RdfBootstrap::parse(&VALID.replace("angle = 18", "angle = 18\n{")).is_err());
    }
}
