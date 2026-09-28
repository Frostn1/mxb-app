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
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Text,
    /// One of these words.
    Choice { options: &'static [&'static str] },
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
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
}

const fn f(section: &'static str, key: &'static str, label: &'static str, help: &'static str, kind: Kind) -> Field {
    Field {
        section,
        key,
        label,
        help,
        kind,
    }
}

/// Every field the app edits. Anything else in the file is left exactly as it is.
pub const FIELDS: &[Field] = &[
    // [ghost]
    f("ghost", "count", "Ghost bots", "How many ghost bots ride (0 turns them off).", Kind::Int { min: 0, max: 49 }),
    f("ghost", "name", "Name prefix", "Bots are named <prefix> 1, <prefix> 2, ...", Kind::Text),
    f("ghost", "replay", "Recording", "The .mxgh line the bots replay, relative to the config file.", Kind::Text),
    f("ghost", "bike", "Bike", "Every bot's bike id, or the class for random bikes.", Kind::Text),
    f("ghost", "bikes", "Bikes", "\"random\" from the server's bike set, or a list of bike ids in turn.", Kind::Bikes),
    f("ghost", "skill_pct", "Skill %", "Ride laps by distance at this % of their own pace (70 to 110). Unset replays by time.", Kind::Float { min: 70.0, max: 110.0 }),
    f("ghost", "racing", "Racing", "Neutral: gaps and passing. Yield: also make way for humans on straights. Block: move into them.", Kind::Racing),
    f("ghost", "lateral_m", "Lateral spread (m)", "Metres either side of the recorded line, on straights (0 to 10).", Kind::Float { min: 0.0, max: 10.0 }),
    f("ghost", "speed_jitter_pct", "Pace spread (%)", "Percent either side of the recording (0 to 30).", Kind::Float { min: 0.0, max: 30.0 }),
    f("ghost", "laps", "Laps", "Best: the fastest clean lap. All: every clean lap. Recording: all of it.", Kind::Choice { options: &["best", "all", "recording"] }),
    f("ghost", "harvest", "Harvest folder", "Save every rider's clean laps here, per track (unset: off).", Kind::Text),
    f("ghost", "library", "Library folder", "Replay harvested laps from here (usually the harvest folder).", Kind::Text),
    f("ghost", "library_min_laps", "Library minimum laps", "Laps a track needs before bots use the library.", Kind::Int { min: 1, max: 1000 }),
    f("ghost", "record", "Record to", "Write one rider's line to this .mxgh (unset: off).", Kind::Text),
    f("ghost", "record_plate", "Record plate", "Race number to record (unset: the first rider seen).", Kind::Int { min: 0, max: 999 }),
    // [events]
    f("events", "collisions", "Collisions", "Log who hit whom, and count collisions per rider for the admin API.", Kind::Bool),
    // [native]
    f("native", "late_join_register", "Late-join register", "Earlier riders and bots join a late joiner on its first pits->track, as on a stock server.", Kind::Bool),
    f("native", "roster_refresh_on_track", "Roster refresh (riders)", "Re-register every other rider to a rider on its first pits->track.", Kind::Bool),
    f("native", "roster_refresh_bots", "Roster refresh (bots)", "Re-register every bot to a rider on its first pits->track.", Kind::Bool),
    f("native", "bot_entries_on_track", "Bot entries on track", "Create bots' race entries for a joiner on its first pits->track, not in its join replay.", Kind::Bool),
    f("native", "resend_lap_history", "Re-send lap history", "Add each rider's lap history to the runtime re-send.", Kind::Bool),
    f("native", "relay_pit_status", "Relay pit status", "Relay a rider's pits/track status to the others.", Kind::Bool),
    f("native", "replay_stock_order", "Stock-order join replay", "A/B: #91's stock-order join replay.", Kind::Bool),
    f("native", "resend_k6", "Re-send kind 6", "A/B: #91's kind 6 per rider in the runtime re-send.", Kind::Bool),
    f("native", "unlink_on_leave", "Unlink on leave", "Unlink a leaver's race number before its entry is removed.", Kind::Bool),
    f("native", "change_pits_gate", "Pits gate for changes", "Refuse a bike/gear change while the rider is on track.", Kind::Bool),
    f("native", "change_refuse_riding", "Refuse changes while riding", "Refuse a change while the rider is on the bike.", Kind::Bool),
    f("native", "slot_cooldown_secs", "Slot cooldown (s)", "Seconds a freed slot waits while another is free (0: reuse at once).", Kind::Int { min: 0, max: 3600 }),
    f("native", "idle_timeout_secs", "Idle timeout (s)", "Seconds of silence before a rider is timed out (default 60).", Kind::Int { min: 5, max: 3600 }),
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
                    Value::Array(a.iter().filter_map(|x| x.as_str().map(|s| json!(s))).collect())
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
                return Err(format!("{} must be one of {}", field.label, options.join(", ")));
            }
            value(s)
        }
        Kind::Racing => match v {
            Value::Bool(false) => value(false),
            Value::String(s) if ["neutral", "yield", "block"].contains(&s.as_str()) => value(s.as_str()),
            _ => return Err(format!("{} must be off, neutral, yield or block", field.label)),
        },
        Kind::Bikes => match v {
            Value::String(s) if s == "random" => value("random"),
            Value::Array(list) => {
                let mut array = Array::new();
                for id in list {
                    let id = id.as_str().ok_or_else(bad)?.trim();
                    let ok = !id.is_empty()
                        && id.len() <= 128
                        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
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
            _ => return Err(format!("{} must be \"random\" or a list of bike ids", field.label)),
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
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
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
                ("ghost.bikes", json!(["MX2OEM_2023_KTM_250_SX-F", "MX2OEM_2025_Fantic_XXF_250"])),
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
        assert_eq!(back["ghost.bikes"], json!(["MX2OEM_2023_KTM_250_SX-F", "MX2OEM_2025_Fantic_XXF_250"]));
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
