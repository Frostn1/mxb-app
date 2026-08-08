//! Reading and patching the dedicated server's `.ini`.
//!
//! Hand-rolled rather than pulled from a crate because the game's format is narrower than
//! any general INI parser assumes, and because the one operation that matters — change a
//! single key, leave every other byte alone — is what general parsers are worst at. A
//! round-trip through a parse/serialise pair reorders keys and drops comments, and this
//! file is one a server owner edits by hand.

/// A `section` / `key` pair, since the same key name lives under more than one section
/// (`port` appears under both `[connection]` and `[live]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub section: &'static str,
    pub name: &'static str,
}

pub const TRACK: Key = Key { section: "event", name: "track" };
pub const NAME: Key = Key { section: "connection", name: "name" };
pub const MAX_CLIENT: Key = Key { section: "connection", name: "maxclient" };

// Not reachable from the API yet — they're here as the vocabulary of keys this module is
// known to handle correctly, and the tests exercise them as the two awkward cases: a key
// whose value is legitimately empty, and one that is absent from a section that exists.
#[allow(dead_code)]
pub const TRACK_LAYOUT: Key = Key { section: "event", name: "track_layout" };
#[allow(dead_code)]
pub const PASSWORD: Key = Key { section: "connection", name: "password" };

/// The value of `key`, or `None` when the section or the key is absent.
///
/// Values are returned trimmed: the game tolerates `track = Victoria` and `track=Victoria`
/// alike, so the surrounding space is formatting rather than content.
pub fn get(text: &str, key: Key) -> Option<String> {
    let mut section = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(header) = section_header(trimmed) {
            section = header.to_ascii_lowercase();
            continue;
        }
        if section != key.section {
            continue;
        }
        if let Some((found, value)) = split_pair(trimmed) {
            if found.eq_ignore_ascii_case(key.name) {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Set `key` to `value`, returning the new file contents.
///
/// Rewrites the key in place when it exists — preserving its position, and therefore any
/// comment above it — and otherwise appends it to the end of its section. A missing section
/// is created at the end of the file.
pub fn set(text: &str, key: Key, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut written = false;
    // Where the target section ended, so a key we never found can be appended inside it
    // rather than after some later section's keys.
    let mut section_end: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(header) = section_header(trimmed) {
            // Leaving the target section without having written: remember this spot.
            if section == key.section && !written && section_end.is_none() {
                section_end = Some(out.len());
            }
            section = header.to_ascii_lowercase();
            out.push(line.to_string());
            continue;
        }
        if section == key.section && !written {
            if let Some((found, _)) = split_pair(trimmed) {
                if found.eq_ignore_ascii_case(key.name) {
                    out.push(format!("{} = {}", key.name, value));
                    written = true;
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }

    if !written {
        let entry = format!("{} = {}", key.name, value);
        match section_end {
            // The section existed but the key didn't — insert at the section's end.
            Some(at) => out.insert(at, entry),
            None if section == key.section => out.push(entry),
            // No such section anywhere: create it.
            None => {
                if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                    out.push(String::new());
                }
                out.push(format!("[{}]", key.section));
                out.push(entry);
            }
        }
    }

    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// `[connection]` -> `connection`; anything else -> `None`.
fn section_header(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix('[')?;
    rest.strip_suffix(']')
}

/// `key = value` -> `(key, value)`, skipping comments and blank lines.
fn split_pair(trimmed: &str) -> Option<(&str, &str)> {
    if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
        return None;
    }
    let (k, v) = trimmed.split_once('=')?;
    Some((k.trim(), v.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[connection]\nname = Frost Test EU\nmaxclient = 20\n\n[event]\ntrack = Victoria\ntrack_layout =\n\n[live]\nenable = 1\nport = 30010\n";

    #[test]
    fn reads_a_value_from_the_right_section() {
        assert_eq!(get(SAMPLE, TRACK).as_deref(), Some("Victoria"));
        assert_eq!(get(SAMPLE, NAME).as_deref(), Some("Frost Test EU"));
    }

    #[test]
    fn an_empty_value_reads_as_empty_not_missing() {
        // `track_layout =` means "default layout", which is different from the key being
        // absent — the caller has to be able to tell those apart.
        assert_eq!(get(SAMPLE, TRACK_LAYOUT).as_deref(), Some(""));
    }

    #[test]
    fn a_missing_key_reads_as_none() {
        assert_eq!(get(SAMPLE, PASSWORD), None);
    }

    #[test]
    fn replaces_in_place_and_leaves_everything_else_byte_identical() {
        let out = set(SAMPLE, TRACK, "Redwick");
        assert_eq!(get(&out, TRACK).as_deref(), Some("Redwick"));
        // Every other key survives untouched, in its original order.
        assert_eq!(get(&out, NAME).as_deref(), Some("Frost Test EU"));
        assert_eq!(get(&out, MAX_CLIENT).as_deref(), Some("20"));
        assert!(out.contains("[live]"), "later sections must survive");
        assert_eq!(out.matches("track =").count(), 1, "must not duplicate the key");
    }

    #[test]
    fn adds_a_missing_key_inside_its_own_section() {
        let out = set(SAMPLE, PASSWORD, "hunter2");
        assert_eq!(get(&out, PASSWORD).as_deref(), Some("hunter2"));
        // It has to land in [connection] — appending at EOF would put it under [live],
        // where the game would read it as a live-timing password instead.
        let connection = out.split("[event]").next().unwrap();
        assert!(
            connection.contains("password = hunter2"),
            "expected the key inside [connection], got:\n{out}"
        );
    }

    #[test]
    fn creates_a_missing_section() {
        let out = set("[connection]\nname = X\n", TRACK, "Victoria");
        assert!(out.contains("[event]"));
        assert_eq!(get(&out, TRACK).as_deref(), Some("Victoria"));
    }

    #[test]
    fn tolerates_spacing_and_case_the_game_tolerates() {
        let text = "[EVENT]\nTRACK=Victoria\n";
        assert_eq!(get(text, TRACK).as_deref(), Some("Victoria"));
        let out = set(text, TRACK, "Redwick");
        assert_eq!(get(&out, TRACK).as_deref(), Some("Redwick"));
    }

    #[test]
    fn keeps_comments_above_the_key_it_rewrites() {
        let text = "[event]\n; the track everyone voted for\ntrack = Victoria\n";
        let out = set(text, TRACK, "Redwick");
        assert!(out.contains("; the track everyone voted for"));
    }
}
