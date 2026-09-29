//! Typed editing of a server's `server.toml`: the fields the app offers, reading their current
//! values, writing changes back without disturbing anything else in the file (comments,
//! order, other sections), and a line diff of the result.
//!
//! The server stays the judge of validity: a candidate is always run through the server's own
//! parser before it is applied (see `remote.rs` / `local.rs`). The bounds here only catch
//! typos early; they mirror `crates/mxbserver/src/args.rs` in mxbserver.

use serde::Serialize;
use serde_json::{json, Value};
use toml_edit::{value, Array, DocumentMut, Item};

/// How a field is edited.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Kind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    Float {
        min: f64,
        max: f64,
    },
    Text,
    /// One of these words.
    Choice {
        options: &'static [&'static str],
    },
    /// `[ghost] racing`: off, or a mode (`true` in the file means "yield").
    Racing,
    /// `[ghost] bikes`: `"random"` or a list of bike ids.
    Bikes,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub section: &'static str,
    pub key: &'static str,
    /// Where the page shows it: "ghosts", "race", "events" or "advanced".
    pub group: &'static str,
    /// Tucked under "More settings" in its group.
    pub advanced: bool,
    pub label: &'static str,
    pub help: &'static str,
    /// What the server does when the setting is left out, in words.
    pub default_text: &'static str,
    /// Shown after numbers ("min", "%", "m", "s").
    pub unit: &'static str,
    pub kind: Kind,
}

struct F(Field);

impl F {
    const fn new(
        section: &'static str,
        key: &'static str,
        group: &'static str,
        label: &'static str,
        kind: Kind,
    ) -> Self {
        F(Field {
            section,
            key,
            group,
            advanced: false,
            label,
            help: "",
            default_text: "",
            unit: "",
            kind,
        })
    }
    const fn help(mut self, help: &'static str) -> Self {
        self.0.help = help;
        self
    }
    const fn default_is(mut self, text: &'static str) -> Self {
        self.0.default_text = text;
        self
    }
    const fn unit(mut self, unit: &'static str) -> Self {
        self.0.unit = unit;
        self
    }
    const fn advanced(mut self) -> Self {
        self.0.advanced = true;
        self
    }
    const fn done(self) -> Field {
        self.0
    }
}

const fn int(min: i64, max: i64) -> Kind {
    Kind::Int { min, max }
}
const fn float(min: f64, max: f64) -> Kind {
    Kind::Float { min, max }
}

/// Every field the app edits, in page order. Anything else in the file is left exactly as it is.
pub const FIELDS: &[Field] = &[
    // ---- Ghosts ------------------------------------------------------------------------------
    F::new("ghost", "count", "ghosts", "Number of bots", int(0, 49))
        .help("Computer-controlled riders based on recorded laps. Set this to 0 for no bots.")
        .default_is("0 (off)")
        .done(),
    F::new("ghost", "skill_pct", "ghosts", "Bot pace", float(70.0, 110.0))
        .help("100% follows the recorded lap's pace; lower is slower and higher is faster.")
        .default_is("replay the recording exactly")
        .unit("%")
        .done(),
    F::new("ghost", "racing", "ghosts", "Racing behaviour", Kind::Racing)
        .help("Neutral races normally. Yield moves aside for players. Block defends its line aggressively.")
        .default_is("off")
        .done(),
    F::new("ghost", "lateral_m", "ghosts", "Line spread", float(0.0, 10.0))
        .help("How far bots may move left or right from the recorded line. More space helps passing and yielding.")
        .default_is("0 m (on the line)")
        .unit("m")
        .done(),
    F::new("ghost", "bikes", "ghosts", "Bikes", Kind::Bikes)
        .help("\"random\" picks from the server's bikes, or list bike ids to use in turn.")
        .default_is("everyone on the same bike")
        .done(),
    F::new("ghost", "laps", "ghosts", "Which laps to replay", Kind::Choice { options: &["best", "all", "recording"] })
        .help("Best: the fastest clean lap. All: every clean lap. Recording: everything, crashes included.")
        .default_is("best")
        .done(),
    F::new("ghost", "speed_jitter_pct", "ghosts", "Pace variety", float(0.0, 30.0))
        .help("Makes each ghost a little faster or slower than the next.")
        .default_is("0% (all the same)")
        .unit("%")
        .advanced()
        .done(),
    F::new("ghost", "name", "ghosts", "Name prefix", Kind::Text)
        .help("Ghosts are called <prefix> 1, <prefix> 2, …")
        .default_is("Bot")
        .advanced()
        .done(),
    F::new("ghost", "bike", "ghosts", "Single bike", Kind::Text)
        .help("One bike id for every ghost, or the class random bikes come from.")
        .default_is("the recording's bike")
        .advanced()
        .done(),
    F::new("ghost", "replay", "ghosts", "Recording file", Kind::Text)
        .help("The .mxgh line the ghosts ride, relative to the config file.")
        .default_is("none")
        .advanced()
        .done(),
    F::new("ghost", "library", "ghosts", "Lap library folder", Kind::Text)
        .help("Ghosts ride laps collected from real riders in this folder instead of one recording.")
        .default_is("off")
        .advanced()
        .done(),
    F::new("ghost", "library_min_laps", "ghosts", "Laps needed before using the library", int(1, 1000))
        .default_is("1")
        .advanced()
        .done(),
    F::new("ghost", "harvest", "ghosts", "Collect riders' laps into", Kind::Text)
        .help("Saves every rider's clean laps to this folder, per track, for the lap library.")
        .default_is("off")
        .advanced()
        .done(),
    F::new("ghost", "record", "ghosts", "Record a rider to", Kind::Text)
        .help("Writes one rider's line to this .mxgh file.")
        .default_is("off")
        .advanced()
        .done(),
    F::new("ghost", "record_plate", "ghosts", "Race number to record", int(0, 999))
        .default_is("the first rider seen")
        .advanced()
        .done(),
    // ---- Race and sessions -------------------------------------------------------------------
    F::new("sessions", "practice_minutes", "race", "Practice length", int(0, 600))
        .help("0 keeps practice running until you move the session on.")
        .default_is("20 min")
        .unit("min")
        .done(),
    F::new("sessions", "qualifying_minutes", "race", "Qualifying length", int(1, 600))
        .default_is("15 min")
        .unit("min")
        .done(),
    F::new("sessions", "warmup_minutes", "race", "Warm-up length", int(1, 600))
        .default_is("5 min")
        .unit("min")
        .done(),
    F::new("sessions", "race_minutes", "race", "Race length", int(1, 600))
        .default_is("20 min")
        .unit("min")
        .done(),
    F::new("sessions", "race_extra_laps", "race", "Laps after the clock runs out", int(0, 10))
        .default_is("2")
        .done(),
    F::new("sessions", "race_countdown_seconds", "race", "Start countdown", int(0, 300))
        .help("0 starts the race straight away.")
        .default_is("30 s")
        .unit("s")
        .done(),
    F::new("server", "max_clients", "race", "Player slots", int(1, 50))
        .help("How many players can join at once.")
        .default_is("20")
        .done(),
    // ---- Events ------------------------------------------------------------------------------
    F::new("events", "collisions", "events", "Track collisions", Kind::Bool)
        .help("Logs who hit whom and counts collisions per rider.")
        .default_is("off")
        .done(),
    // ---- Advanced switches (A/B tests from the protocol work) ---------------------------------
    F::new("native", "late_join_register", "advanced", "Late joiners see earlier riders like a stock server", Kind::Bool)
        .help("Compatibility option: send the existing rider list when a new player leaves the pits.")
        .default_is("off")
        .done(),
    F::new("native", "roster_refresh_on_track", "advanced", "Refresh other riders on track entry", Kind::Bool)
        .help("Re-send the human rider list when a player enters the track. Useful only for join-sync testing.")
        .default_is("off")
        .done(),
    F::new("native", "roster_refresh_bots", "advanced", "Refresh ghosts on track entry", Kind::Bool)
        .help("Re-send the bot list when a player enters the track. Useful only for join-sync testing.")
        .default_is("off")
        .done(),
    F::new("native", "bot_entries_on_track", "advanced", "Add ghosts to standings on track entry", Kind::Bool)
        .help("Wait until a player enters the track before adding bots to their standings.")
        .default_is("off")
        .done(),
    F::new("native", "resend_lap_history", "advanced", "Re-send lap history", Kind::Bool)
        .help("Send known lap results again after a roster refresh for compatibility testing.")
        .default_is("off")
        .done(),
    F::new("native", "relay_pit_status", "advanced", "Share pit/track status between riders", Kind::Bool)
        .help("Tell each player when another rider enters or leaves the track.")
        .default_is("off")
        .done(),
    F::new("native", "replay_stock_order", "advanced", "Stock-order join replay", Kind::Bool)
        .help("Reproduce the stock server's join-message order for compatibility testing.")
        .default_is("off")
        .done(),
    F::new("native", "resend_k6", "advanced", "Re-send entry details per rider", Kind::Bool)
        .help("Repeat low-level rider entry data. Leave off unless diagnosing a join problem.")
        .default_is("off")
        .done(),
    F::new("native", "unlink_on_leave", "advanced", "Free a leaver's race number first", Kind::Bool)
        .help("Release a disconnected rider's number before announcing that they left.")
        .default_is("off")
        .done(),
    F::new("native", "change_pits_gate", "advanced", "Only allow bike changes in the pits", Kind::Bool)
        .help("Reject bike or setup changes unless the rider is currently in the pits.")
        .default_is("off")
        .done(),
    F::new("native", "change_refuse_riding", "advanced", "Refuse bike changes while riding", Kind::Bool)
        .help("Reject bike or setup changes while the rider is actively on track.")
        .default_is("off")
        .done(),
    F::new("native", "slot_cooldown_secs", "advanced", "Wait before reusing a freed slot", int(0, 3600))
        .help("0 reuses a freed slot at once.")
        .default_is("120 s")
        .unit("s")
        .done(),
    F::new("native", "idle_timeout_secs", "advanced", "Drop a silent rider after", int(1, 3600))
        .help("Disconnect a client that stops sending packets for this long.")
        .default_is("60 s")
        .unit("s")
        .done(),
];

pub fn field(section: &str, key: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.section == section && f.key == key)
}

fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|e| format!("the config is not valid TOML: {e}"))
}

/// The current value of every field, keyed `section.key`; `null` when unset.
pub fn read(text: &str) -> Result<serde_json::Map<String, Value>, String> {
    let doc = parse(text)?;
    let mut out = serde_json::Map::new();
    for field in FIELDS {
        let item = doc
            .get(field.section)
            .and_then(|s| s.get(field.key))
            .and_then(Item::as_value);
        let v = match item {
            None => Value::Null,
            Some(v) => {
                if let Some(b) = v.as_bool() {
                    json!(b)
                } else if let Some(i) = v.as_integer() {
                    json!(i)
                } else if let Some(x) = v.as_float() {
                    json!(x)
                } else if let Some(s) = v.as_str() {
                    json!(s)
                } else if let Some(a) = v.as_array() {
                    Value::Array(
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| json!(s)))
                            .collect(),
                    )
                } else {
                    Value::Null
                }
            }
        };
        out.insert(format!("{}.{}", field.section, field.key), v);
    }
    Ok(out)
}

/// Check one change against its field and turn it into a TOML value; `None` unsets it.
fn to_item(field: &Field, v: &Value) -> Result<Option<Item>, String> {
    let bad = || format!("{}: not a valid value", field.label);
    if v.is_null() {
        return Ok(None);
    }
    let item = match &field.kind {
        Kind::Bool => value(v.as_bool().ok_or_else(bad)?),
        Kind::Int { min, max } => {
            let n = v.as_i64().ok_or_else(bad)?;
            if n < *min || n > *max {
                return Err(format!("{} must be {min} to {max}", field.label));
            }
            value(n)
        }
        Kind::Float { min, max } => {
            let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(bad)?;
            if x < *min || x > *max {
                return Err(format!("{} must be {min} to {max}", field.label));
            }
            value(x)
        }
        Kind::Text => {
            let s = v.as_str().ok_or_else(bad)?.trim();
            if s.is_empty() {
                return Ok(None);
            }
            if s.len() > 512 || s.contains(['\n', '\r', '\0']) {
                return Err(bad());
            }
            value(s)
        }
        Kind::Choice { options } => {
            let s = v.as_str().ok_or_else(bad)?;
            if !options.contains(&s) {
                return Err(format!(
                    "{} must be one of {}",
                    field.label,
                    options.join(", ")
                ));
            }
            value(s)
        }
        Kind::Racing => match v {
            Value::Bool(false) => value(false),
            Value::String(s) if ["neutral", "yield", "block"].contains(&s.as_str()) => {
                value(s.as_str())
            }
            _ => {
                return Err(format!(
                    "{} must be off, neutral, yield or block",
                    field.label
                ))
            }
        },
        Kind::Bikes => match v {
            Value::String(s) if s == "random" => value("random"),
            Value::Array(list) => {
                let mut array = Array::new();
                for id in list {
                    let id = id.as_str().ok_or_else(bad)?.trim();
                    let ok = !id.is_empty()
                        && id.len() <= 128
                        && id
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
                    if !ok {
                        return Err(format!("{}: '{id}' is not a bike id", field.label));
                    }
                    array.push(id);
                }
                if array.is_empty() {
                    return Ok(None);
                }
                value(array)
            }
            _ => {
                return Err(format!(
                    "{} must be \"random\" or a list of bike ids",
                    field.label
                ))
            }
        },
    };
    Ok(Some(item))
}

/// `text` with `changes` (`section.key` -> value, `null` to unset) applied. Keys and sections
/// not named are untouched, and so are comments on untouched lines.
pub fn apply(text: &str, changes: &serde_json::Map<String, Value>) -> Result<String, String> {
    let mut doc = parse(text)?;
    for (name, v) in changes {
        let (section, key) = name
            .split_once('.')
            .ok_or_else(|| format!("unknown field {name}"))?;
        let field = field(section, key).ok_or_else(|| format!("unknown field {name}"))?;
        match to_item(field, v)? {
            Some(mut item) => {
                // Keep the old value's spacing and trailing comment on the new one.
                let old = doc
                    .get(section)
                    .and_then(|s| s.get(key))
                    .and_then(Item::as_value)
                    .map(|v| v.decor().clone());
                if let (Some(decor), Item::Value(new)) = (old, &mut item) {
                    *new.decor_mut() = decor;
                }
                if !doc.contains_table(section) {
                    if doc.contains_key(section) {
                        return Err(format!("[{section}] is not a table in this file"));
                    }
                    doc[section] = toml_edit::table();
                }
                doc[section][key] = item;
            }
            None => {
                if let Some(table) = doc.get_mut(section).and_then(Item::as_table_like_mut) {
                    table.remove(key);
                }
            }
        }
    }
    Ok(doc.to_string())
}

/// A unified diff of two versions of the file (3 lines of context).
pub fn diff(old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header("server.toml (now)", "server.toml (after)")
        .to_string()
}

pub fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
# Lightsail
[server]
name = \"Native MXB Server\" # the list name

[ghost]
count = 4 # four bots
replay = \"ghosts/line.mxgh\"
racing = true

[native]
late_join_register = false
";

    fn changes(pairs: &[(&str, Value)]) -> serde_json::Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn reads_what_is_set_and_null_for_the_rest() {
        let values = read(FILE).unwrap();
        assert_eq!(values["ghost.count"], json!(4));
        assert_eq!(values["ghost.racing"], json!(true));
        assert_eq!(values["native.late_join_register"], json!(false));
        assert_eq!(values["events.collisions"], Value::Null);
    }

    #[test]
    fn edits_keep_comments_and_everything_else() {
        let out = apply(
            FILE,
            &changes(&[
                ("ghost.count", json!(6)),
                ("ghost.racing", json!("neutral")),
                ("ghost.replay", Value::Null),
                ("events.collisions", json!(true)),
                (
                    "ghost.bikes",
                    json!(["MX2OEM_2023_KTM_250_SX-F", "MX2OEM_2025_Fantic_XXF_250"]),
                ),
            ]),
        )
        .unwrap();
        assert!(out.contains("# Lightsail"));
        assert!(out.contains("name = \"Native MXB Server\" # the list name"));
        assert!(out.contains("count = 6"));
        assert!(out.contains("racing = \"neutral\""));
        assert!(!out.contains("replay"));
        assert!(out.contains("[events]\ncollisions = true"));
        let back = read(&out).unwrap();
        assert_eq!(
            back["ghost.bikes"],
            json!(["MX2OEM_2023_KTM_250_SX-F", "MX2OEM_2025_Fantic_XXF_250"])
        );
        assert_eq!(back["native.late_join_register"], json!(false));
    }

    #[test]
    fn out_of_range_and_unknown_are_refused() {
        for (k, v) in [
            ("ghost.skill_pct", json!(150.0)),
            ("ghost.count", json!(-1)),
            ("ghost.laps", json!("some")),
            ("ghost.racing", json!(true)),
            ("ghost.bikes", json!(["bad id; rm"])),
            ("ghost.name", json!("a\nb")),
            ("server.password", json!("x")),
            ("nope", json!(1)),
        ] {
            assert!(apply(FILE, &changes(&[(k, v.clone())])).is_err(), "{k} {v}");
        }
    }

    #[test]
    fn diff_shows_only_the_change() {
        let out = apply(FILE, &changes(&[("ghost.count", json!(2))])).unwrap();
        let d = diff(FILE, &out);
        assert!(d.contains("-count = 4 # four bots"));
        assert!(d.contains("+count = 2 # four bots"));
        assert!(!d.contains("+[server]"));
        assert_eq!(diff(FILE, FILE), "");
    }
}
