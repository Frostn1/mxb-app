//! What FrostMod's crash guards caught on this PC, and the table of known crashes.
//!
//! FrostMod appends one line to `frostmod-protection.jsonl`, beside its crash reports, for
//! each kind of crash a guard stopped in a session (FrostMod's `src/protection.h` says what
//! counts as one). This reads that file for the home card, the note after a session and the
//! Crash protection page.
//!
//! The table of known crashes — what each one is, its share of the crash reports, and whether
//! FrostMod fixes it — is bundled (`data/crash_sites.json`), built from the reports FrostMod
//! sends. It changes with FrostMod releases, which ship with app releases anyway.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file FrostMod writes. One JSON object per line.
pub const FILE_NAME: &str = "frostmod-protection.jsonl";

/// Never read more than this: FrostMod trims the file to 2000 lines, so anything far larger is
/// not one of ours.
const MAX_BYTES: u64 = 4 << 20;

const TABLE: &str = include_str!("data/crash_sites.json");

/// One crash a guard stopped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caught {
    /// Unix seconds.
    pub t: i64,
    /// FrostMod's kind id, e.g. `physics-nan`. Matches an `id` in the table.
    pub kind: String,
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub track: String,
    #[serde(default)]
    pub server: String,
    /// FrostMod's version when it caught it.
    #[serde(default)]
    pub v: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Newest first.
    pub caught: Vec<Caught>,
    /// The bundled table, passed through as JSON.
    pub table: serde_json::Value,
}

/// Every line of `text` that parses, newest first. A torn last line (the game crashed while
/// FrostMod was writing it) or anything else that isn't ours is skipped.
pub fn parse(text: &str) -> Vec<Caught> {
    let mut out: Vec<Caught> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Caught>(l.trim()).ok())
        .filter(|c| c.t > 0 && !c.kind.is_empty())
        .collect();
    out.sort_by(|a, b| b.t.cmp(&a.t));
    out
}

pub fn read(frostmod_dir: &Path) -> Vec<Caught> {
    let path = frostmod_dir.join(FILE_NAME);
    match std::fs::metadata(&path) {
        Ok(m) if m.len() <= MAX_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read_to_string(&path).map(|t| parse(&t)).unwrap_or_default()
}

pub fn table() -> serde_json::Value {
    serde_json::from_str(TABLE).unwrap_or(serde_json::Value::Null)
}

pub fn report(frostmod_dir: &Path) -> Report {
    Report { caught: read(frostmod_dir), table: table() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lines_newest_first_and_skips_a_torn_one() {
        let text = concat!(
            r#"{"t":100,"kind":"physics-nan","site":"mxbikes.exe+0x1A29AE","track":"A","server":"S","v":"0.52.0"}"#,
            "\n",
            r#"{"t":300,"kind":"render-batch","site":"mxbikes.exe+0x24085C","track":"B","server":"","v":"0.52.0"}"#,
            "\n",
            r#"{"t":200,"kind":"rider-info"}"#,
            "\n",
            r#"{"t":400,"kind":"physics-n"#,
        );
        let got = parse(text);
        assert_eq!(got.iter().map(|c| c.t).collect::<Vec<_>>(), [300, 200, 100]);
        assert_eq!(got[0].kind, "render-batch");
        assert_eq!(got[1].track, "");
    }

    #[test]
    fn missing_file_is_empty() {
        let d = std::env::temp_dir().join("mxb-protection-test-missing");
        assert!(read(&d).is_empty());
    }

    #[test]
    fn the_bundled_table_parses_and_names_every_frostmod_kind() {
        let t = table();
        let ids: Vec<&str> = t["crashes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["id"].as_str())
            .collect();
        for kind in [
            "physics-nan", "render-batch", "bad-text", "trainer-close", "bike-list", "wake",
            "list-box", "stack-pointer", "rider-info", "menu-image",
        ] {
            assert!(ids.contains(&kind), "{kind} missing from crash_sites.json");
        }
        assert!(t["coverage"].as_f64().unwrap() > 0.0);
    }
}
