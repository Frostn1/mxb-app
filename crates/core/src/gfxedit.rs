//! Editing a bike's `gfx.cfg` in place.
//!
//! [`crate::cfg`] reads the file into maps, which is all the viewer needs and loses
//! everything a person wrote around the values: order, indentation, comments, keys nobody
//! here knows about. This module keeps the text and only ever swaps the bytes of the values
//! it was asked to change, so a save that touches one grip leaves every other line exactly
//! as it was.
//!
//! The grammar is the one [`crate::cfg::parse`] accepts: `name { … }` blocks (the brace may
//! sit on the next line), `key = value` to the end of the line or a closing brace, and `;`
//! starting a comment. Keys match case-insensitively; the first match wins at each level.

use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Debug, Clone)]
struct Block {
    name: String,
    /// Byte offset of the `}`, or the end of the text for the root (and an unclosed block).
    close: usize,
    items: Vec<Item>,
}

#[derive(Debug, Clone)]
enum Item {
    Block(Block),
    Value { key: String, span: Range<usize> },
}

/// A parsed `gfx.cfg` that remembers where everything came from.
#[derive(Debug, Clone)]
pub struct Doc {
    text: String,
    root: Block,
}

impl Doc {
    pub fn parse(text: &str) -> Doc {
        let mut stack: Vec<Block> = vec![Block {
            name: String::new(),
            close: text.len(),
            items: Vec::new(),
        }];
        // A name waiting for its `{`, which may be on a later line.
        let mut pending: Option<String> = None;
        let mut line_start = 0;
        for line in text.split_inclusive('\n') {
            let content_end = line.find(';').unwrap_or(line.len());
            let content = &line[..content_end];
            let b = content.as_bytes();
            let mut i = 0;
            while i < b.len() {
                let c = b[i];
                if c.is_ascii_whitespace() {
                    i += 1;
                    continue;
                }
                let at = line_start + i;
                if c == b'{' {
                    stack.push(Block {
                        name: pending.take().unwrap_or_default().to_ascii_lowercase(),
                        close: text.len(),
                        items: Vec::new(),
                    });
                    i += 1;
                    continue;
                }
                if c == b'}' {
                    pending = None;
                    if stack.len() > 1 {
                        let mut done = stack.pop().unwrap();
                        done.close = at;
                        stack.last_mut().unwrap().items.push(Item::Block(done));
                    }
                    i += 1;
                    continue;
                }
                let start = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'{' | b'}' | b'=') {
                    i += 1;
                }
                let word = &content[start..i];
                let mut j = i;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && b[j] == b'=' {
                    // The value runs to a closing brace or the end of the line.
                    let mut v0 = j + 1;
                    while v0 < b.len() && b[v0].is_ascii_whitespace() && b[v0] != b'\n' {
                        v0 += 1;
                    }
                    let mut v1 = v0;
                    while v1 < b.len() && b[v1] != b'}' && b[v1] != b'\n' && b[v1] != b'\r' {
                        v1 += 1;
                    }
                    let mut end = v1;
                    while end > v0 && b[end - 1].is_ascii_whitespace() {
                        end -= 1;
                    }
                    if !word.is_empty() {
                        stack.last_mut().unwrap().items.push(Item::Value {
                            key: word.to_ascii_lowercase(),
                            span: line_start + v0..line_start + end,
                        });
                    }
                    pending = None;
                    i = v1;
                } else if !word.is_empty() {
                    pending = Some(word.to_string());
                } else {
                    // A stray `=` with nothing before it.
                    i = j + 1;
                }
            }
            line_start += line.len();
        }
        // Anything left open closes at the end of the file.
        while stack.len() > 1 {
            let done = stack.pop().unwrap();
            stack.last_mut().unwrap().items.push(Item::Block(done));
        }
        Doc {
            text: text.to_string(),
            root: stack.pop().unwrap(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn block(&self, path: &[&str]) -> Option<&Block> {
        let mut b = &self.root;
        for seg in path {
            b = b.items.iter().find_map(|it| match it {
                Item::Block(c) if c.name.eq_ignore_ascii_case(seg) => Some(c),
                _ => None,
            })?;
        }
        Some(b)
    }

    fn span(&self, path: &str) -> Option<Range<usize>> {
        let segs: Vec<&str> = path.split('/').collect();
        let (leaf, parents) = segs.split_last()?;
        let b = self.block(parents)?;
        b.items.iter().find_map(|it| match it {
            Item::Value { key, span } if key.eq_ignore_ascii_case(leaf) => Some(span.clone()),
            _ => None,
        })
    }

    /// The value at `path` (`steer/leftgrip/pos/x`), as written.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.span(path).map(|r| &self.text[r])
    }

    /// Whether the block at `path` (`cockpit/steer`) exists.
    pub fn has_block(&self, path: &str) -> bool {
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        self.block(&segs).is_some()
    }

    /// Set `path` to `value`. An existing value has just its bytes replaced; a missing one is
    /// written into the deepest block on its path that exists, creating the rest.
    pub fn set(&mut self, path: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if value.is_empty() || value.contains(['\n', '\r', '{', '}', ';']) {
            return Err(format!("{path}: '{value}' can't be written to gfx.cfg"));
        }
        if let Some(r) = self.span(path) {
            self.text.replace_range(r, value);
        } else {
            let segs: Vec<&str> = path.split('/').collect();
            let mut have = 0;
            while have < segs.len() - 1 && self.block(&segs[..=have]).is_some() {
                have += 1;
            }
            let host = self.block(&segs[..have]).expect("the root always exists").clone();
            let unit = self.indent_unit();
            let depth = have;
            let mut add = String::new();
            for (k, seg) in segs[have..segs.len() - 1].iter().enumerate() {
                let pad = unit.repeat(depth + k);
                add.push_str(&format!("{pad}{seg}\n{pad}{{\n"));
            }
            let inner = depth + segs.len() - 1 - have;
            add.push_str(&format!("{}{} = {value}\n", unit.repeat(inner), segs[segs.len() - 1]));
            for k in (0..segs.len() - 1 - have).rev() {
                add.push_str(&format!("{}}}\n", unit.repeat(depth + k)));
            }
            let nl = if self.text.contains("\r\n") { "\r\n" } else { "\n" };
            let add = add.replace('\n', nl);
            let at = if have == 0 {
                // The root: at the end of the file, on a line of its own.
                if !self.text.is_empty() && !self.text.ends_with('\n') {
                    self.text.push_str(nl);
                }
                self.text.len()
            } else {
                // Before the closing brace, at the start of its line when it has one to itself.
                let close = host.close;
                let line0 = self.text[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
                if self.text[line0..close].trim().is_empty() {
                    line0
                } else {
                    self.text.insert_str(close, nl);
                    close + nl.len()
                }
            };
            self.text.insert_str(at, &add);
        }
        *self = Doc::parse(&self.text);
        Ok(())
    }

    fn indent_unit(&self) -> String {
        if self.text.lines().any(|l| l.starts_with('\t')) {
            "\t".into()
        } else {
            "    ".into()
        }
    }
}

/// The values the editor shows, relative to a part block. The same paths exist under
/// `cockpit/` for the first-person copy.
pub const FIELDS: &[&str] = &[
    "chassis/rearbrakepedal/name",
    "chassis/rearbrakepedal/axis",
    "chassis/rearbrakepedal/maxrot",
    "chassis/shifter/name",
    "chassis/shifter/axis",
    "chassis/shifter/maxrot",
    "chassis/chain/pos/x",
    "chassis/chain/pos/y",
    "chassis/chain/pos/z",
    "chassis/chain/engine/x",
    "chassis/chain/engine/y",
    "chassis/chain/engine/z",
    "chassis/chain/engine/link_obj",
    "chassis/exhaust/pos/x",
    "chassis/exhaust/pos/y",
    "chassis/exhaust/pos/z",
    "chassis/exhaust/dir/x",
    "chassis/exhaust/dir/y",
    "chassis/exhaust/dir/z",
    "steer/throttlegrip/name",
    "steer/throttlegrip/axis",
    "steer/throttlegrip/maxrot",
    "steer/brakelever/name",
    "steer/brakelever/axis",
    "steer/brakelever/maxrot",
    "steer/clutchlever/name",
    "steer/clutchlever/axis",
    "steer/clutchlever/maxrot",
    "steer/leftgrip/pos/x",
    "steer/leftgrip/pos/y",
    "steer/leftgrip/pos/z",
    "steer/rightgrip/pos/x",
    "steer/rightgrip/pos/y",
    "steer/rightgrip/pos/z",
    "rider/xform/x",
    "rider/xform/y",
    "rider/xform/z",
];

/// Every known field that's present: the main copy under its own path, the first-person
/// copy under `cockpit/…`.
pub fn read_fields(text: &str) -> BTreeMap<String, String> {
    let doc = Doc::parse(text);
    let mut out = BTreeMap::new();
    for f in FIELDS {
        if let Some(v) = doc.get(f) {
            out.insert((*f).to_string(), v.to_string());
        }
        let c = format!("cockpit/{f}");
        if let Some(v) = doc.get(&c) {
            out.insert(c, v.to_string());
        }
    }
    out
}

/// Apply `edits` (main-copy paths) to `text`, keeping the `cockpit` copy in step.
///
/// The main copy always takes the edit. The cockpit copy takes it where it already has the
/// value, or at least the feature's block (`cockpit/steer/leftgrip`): a cockpit that never
/// animated a lever doesn't start because the main model's lever moved.
pub fn apply(text: &str, edits: &[(String, String)]) -> Result<String, String> {
    let mut doc = Doc::parse(text);
    for (path, value) in edits {
        let path = path.trim_matches('/');
        if path.starts_with("cockpit/") || !FIELDS.iter().any(|f| f.eq_ignore_ascii_case(path)) {
            return Err(format!("{path} isn't an editable gfx.cfg field"));
        }
        doc.set(path, value)?;
        let c = format!("cockpit/{path}");
        let feature: Vec<&str> = path.split('/').take(2).collect();
        if doc.get(&c).is_some() || doc.has_block(&format!("cockpit/{}", feature.join("/"))) {
            doc.set(&c, value)?;
        }
    }
    Ok(doc.text)
}

/// Decode a `gfx.cfg` read off disk. UTF-8 when it is; otherwise one char per byte, which
/// [`encode`] turns back into the same bytes.
pub fn decode(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), true),
        Err(_) => (bytes.iter().map(|&b| b as char).collect(), false),
    }
}

pub fn encode(text: &str, utf8: bool) -> Vec<u8> {
    if utf8 {
        text.as_bytes().to_vec()
    } else {
        text.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 2025 KTM with Handguards blend's gfx.cfg, as shipped (cockpit indentation and all).
    const KTM: &str = "\n\n\nchassis\n{\n\tmodel\n\t{\n\t\tfile = chassis.hrc\n\t}\n\tshadow\n\t{\n\t\tfile = model_shadow.edf\n\t}\n\n\trearbrakepedal\n\t{\n\t\tname = rearbrake_lever\n\t\taxis = x-\n\t\tmaxrot = -10\n\t}\n\n\tchain\n\t{\n\t\tname = chain\n\t\tpos\n\t\t{\n\t\t\tx = -0.65\n\t\t\ty = 0\n\t\t\tz = 0\n\t\t}\n\t\tengine\n\t\t{\n\t\t\tx = -0.074\n\t\t\ty = 0.485\n\t\t\tz = -0.109\n\t\t}\n\t\ttexture = chain\n\t\taxis = u-\n\t\tratio = 0.0\n\t}\n\n\texhaust\n\t{\n\t\tpos\n\t\t{\n\t\t\tx = 0.13\n\t\t\ty = 0.9\n\t\t\tz = -0.8\n\t\t}\n\t\tdir\n\t\t{\n\t\t\tx = 0\n\t\t\ty = 0\n\t\t\tz = -1\n\t\t}\t\n\t}\n}\n\nsteer\n{\n\tmodel\n\t{\n\t\tfile = steer.hrc\n\t}\n\tleftgrip\n\t{\n\t\ttype = 1\n\t\tpos\n\t\t{\n\t\t\tx = -0.33\n\t\t\ty = 0.22\n\t\t\tz = -0.025\n\t\t}\n\t}\n\trightgrip\n\t{\n\t\ttype = 1\n\t\tpos\n\t\t{\n\t\t\tx = 0.33\n\t\t\ty = 0.22\n\t\t\tz = -0.025\n\t\t}\n\t}\n}\n\ntyres = oem_mx\n\ndirt_color=1\ncombined_dirt=1\n\ncockpit\n{\n\tchassis\n\t{\n\t\tmodel\n\t\t{\n\t\t\tfile = model.edf\n\t\t}\n\tchain\n\t{\n\t\tname = chain\n\t\tpos\n\t\t{\n\t\t\tx = -0.65\n\t\t\ty = 0\n\t\t\tz = 0\n\t\t}\n\t\tengine\n\t\t{\n\t\t\tx = -0.074\n\t\t\ty = 0.485\n\t\t\tz = -0.109\n\t\t}\n\t}\n\t}\n\tsteer\n\t{\n\t\tleftgrip\n\t\t{\n\t\t\ttype = 1\n\t\t\tpos\n\t\t\t{\n\t\t\t\tx = -0.33\n\t\t\t\ty = 0.22\n\t\t\t\tz = -0.025\n\t\t\t}\n\t\t}\n\t\trightgrip\n\t\t{\n\t\t\ttype = 1\n\t\t\tpos\n\t\t\t{\n\t\t\t\tx = 0.33\n\t\t\t\ty = 0.22\n\t\t\t\tz = -0.025\n\t\t\t}\n\t\t}\n\t}\n}\n";

    fn edit(text: &str, e: &[(&str, &str)]) -> String {
        let e: Vec<(String, String)> = e.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        apply(text, &e).unwrap()
    }

    #[test]
    fn reads_what_cfg_reads() {
        let doc = Doc::parse(KTM);
        let cfg = crate::cfg::parse(KTM.as_bytes());
        assert_eq!(doc.get("steer/leftgrip/pos/x"), Some("-0.33"));
        assert_eq!(
            doc.get("chassis/chain/engine/z"),
            cfg.block("chassis").and_then(|c| c.block("chain")).and_then(|c| c.block("engine")).and_then(|e| e.get("z"))
        );
        assert_eq!(doc.get("tyres"), Some("oem_mx"));
        assert_eq!(doc.get("dirt_color"), Some("1"));
        assert_eq!(doc.get("chassis/exhaust/dir/z"), Some("-1"));
        assert_eq!(doc.get("cockpit/steer/rightgrip/pos/x"), Some("0.33"));
        assert_eq!(doc.get("cockpit/chassis/chain/engine/y"), Some("0.485"));
        assert_eq!(doc.get("STEER/LeftGrip/POS/x"), Some("-0.33"));
        assert_eq!(doc.get("steer/leftgrip/pos/w"), None);
    }

    #[test]
    fn no_edit_is_a_byte_for_byte_round_trip() {
        assert_eq!(edit(KTM, &[]), KTM);
        let crlf = KTM.replace('\n', "\r\n");
        assert_eq!(edit(&crlf, &[]), crlf);
    }

    #[test]
    fn writing_the_same_value_changes_nothing() {
        assert_eq!(edit(KTM, &[("steer/leftgrip/pos/x", "-0.33"), ("chassis/exhaust/pos/y", "0.9")]), KTM);
    }

    #[test]
    fn an_edit_changes_only_its_bytes_and_the_cockpit_copy() {
        let out = edit(KTM, &[("steer/leftgrip/pos/y", "0.245")]);
        let doc = Doc::parse(&out);
        assert_eq!(doc.get("steer/leftgrip/pos/y"), Some("0.245"));
        assert_eq!(doc.get("cockpit/steer/leftgrip/pos/y"), Some("0.245"));
        assert_eq!(doc.get("steer/rightgrip/pos/y"), Some("0.22"));
        // Everything else is the original text: undo the two swaps and it's byte-identical.
        assert_eq!(out.matches("0.245").count(), 2);
        let back = edit(&out, &[("steer/leftgrip/pos/y", "0.22")]);
        assert_eq!(back, KTM);
    }

    #[test]
    fn keeps_comments_and_unknown_keys() {
        let src = "; my bike\nchassis\n{\n\tfoo = bar ; keep me\n\texhaust { pos { x = 1 }\n\t\tpos2 = 4\n\t}\n\texhaust2 { pos {\n y = 2 } }\n}\nwhatever = 3\n";
        let out = edit(src, &[("chassis/exhaust/pos/x", "0.5")]);
        assert_eq!(out, src.replace("x = 1", "x = 0.5"));
    }

    #[test]
    fn a_value_before_a_closing_brace_on_its_line() {
        let src = "steer\n{\n\tleftgrip { pos { x = -0.3 } }\n}\n";
        let out = edit(src, &[("steer/leftgrip/pos/x", "-0.31")]);
        assert_eq!(out, "steer\n{\n\tleftgrip { pos { x = -0.31 } }\n}\n");
    }

    #[test]
    fn a_missing_value_is_added_inside_its_block() {
        let src = "chassis\n{\n\tmodel\n\t{\n\t\tfile = chassis.hrc\n\t}\n}\n";
        let out = edit(src, &[("chassis/exhaust/pos/x", "0.13"), ("chassis/exhaust/pos/y", "0.9")]);
        assert_eq!(
            out,
            "chassis\n{\n\tmodel\n\t{\n\t\tfile = chassis.hrc\n\t}\n\texhaust\n\t{\n\t\tpos\n\t\t{\n\t\t\tx = 0.13\n\t\t\ty = 0.9\n\t\t}\n\t}\n}\n"
        );
        // It reads back the way the game's own reader would.
        let cfg = crate::cfg::parse(out.as_bytes());
        let pos = cfg.block("chassis").and_then(|c| c.block("exhaust")).and_then(|e| e.block("pos")).unwrap();
        assert_eq!((pos.get("x"), pos.get("y")), (Some("0.13"), Some("0.9")));
    }

    #[test]
    fn a_missing_part_block_is_added_at_the_end() {
        let out = edit("tyres = oem_mx", &[("steer/leftgrip/pos/x", "-0.3")]);
        assert_eq!(out, "tyres = oem_mx\nsteer\n{\n    leftgrip\n    {\n        pos\n        {\n            x = -0.3\n        }\n    }\n}\n");
        assert_eq!(Doc::parse(&out).get("steer/leftgrip/pos/x"), Some("-0.3"));
    }

    #[test]
    fn cockpit_only_follows_features_it_has() {
        // The KTM cockpit has no exhaust or brake pedal: they stay out of it.
        let out = edit(KTM, &[("chassis/exhaust/pos/x", "0.2"), ("chassis/rearbrakepedal/maxrot", "-12")]);
        let doc = Doc::parse(&out);
        assert_eq!(doc.get("chassis/exhaust/pos/x"), Some("0.2"));
        assert_eq!(doc.get("chassis/rearbrakepedal/maxrot"), Some("-12"));
        assert!(!doc.has_block("cockpit/chassis/exhaust"));
        assert!(!doc.has_block("cockpit/chassis/rearbrakepedal"));
        // The chain is in both.
        let out = edit(KTM, &[("chassis/chain/engine/x", "-0.08")]);
        assert_eq!(Doc::parse(&out).get("cockpit/chassis/chain/engine/x"), Some("-0.08"));
    }

    #[test]
    fn cockpit_gets_a_missing_value_inside_a_feature_it_has() {
        let src = "steer { leftgrip { pos { x = 1 } } }\ncockpit\n{\n\tsteer\n\t{\n\t\tleftgrip\n\t\t{\n\t\t\ttype = 1\n\t\t}\n\t}\n}\n";
        let out = edit(src, &[("steer/leftgrip/pos/x", "2")]);
        let doc = Doc::parse(&out);
        assert_eq!(doc.get("steer/leftgrip/pos/x"), Some("2"));
        assert_eq!(doc.get("cockpit/steer/leftgrip/pos/x"), Some("2"));
        assert_eq!(doc.get("cockpit/steer/leftgrip/type"), Some("1"));
    }

    #[test]
    fn refuses_bad_paths_and_values() {
        assert!(apply(KTM, &[("tyres".into(), "x".into())]).is_err());
        assert!(apply(KTM, &[("cockpit/steer/leftgrip/pos/x".into(), "1".into())]).is_err());
        assert!(apply(KTM, &[("steer/leftgrip/pos/x".into(), "1 }".into())]).is_err());
        assert!(apply(KTM, &[("steer/leftgrip/pos/x".into(), "".into())]).is_err());
    }

    #[test]
    fn read_fields_lists_both_copies() {
        let f = read_fields(KTM);
        assert_eq!(f.get("steer/rightgrip/pos/z").map(String::as_str), Some("-0.025"));
        assert_eq!(f.get("cockpit/steer/rightgrip/pos/z").map(String::as_str), Some("-0.025"));
        assert_eq!(f.get("chassis/rearbrakepedal/name").map(String::as_str), Some("rearbrake_lever"));
        assert!(!f.contains_key("cockpit/chassis/exhaust/pos/x"));
        assert!(!f.contains_key("steer/brakelever/axis"));
    }

    #[test]
    fn non_utf8_bytes_survive() {
        let bytes = b"; caf\xe9\nsteer { leftgrip { pos { x = 1 } } }\n".to_vec();
        let (text, utf8) = decode(&bytes);
        assert!(!utf8);
        let out = apply(&text, &[("steer/leftgrip/pos/x".into(), "2".into())]).unwrap();
        assert_eq!(encode(&out, utf8), b"; caf\xe9\nsteer { leftgrip { pos { x = 2 } } }\n".to_vec());
    }
}
