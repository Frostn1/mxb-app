//! The coach's words in the rider's language.
//!
//! The advice is written here, in Rust, out of pieces of the lap: a number, a section's name, a
//! clause picked by what happened. So it is finished here too, in the language the rider picked,
//! rather than shipped to the window as keys and arguments that the window would have to put
//! back together the same way. And the live cues and the HUD sheet go to the game, which never
//! sees the window at all.
//!
//! The English stays where it always was, inline beside the rule that says it:
//!
//! ```ignore
//! tr!("corner.brake_early.detail", "You start braking about {m} m before the fast lap into {name}.", m = num(early, 0), name = name)
//! ```
//!
//! The key names the sentence; `locales/fr.json` holds its French under the same key, with the
//! same `{placeholders}`. A key missing from it falls back to the English. The tests read every
//! `tr!` and `msg!` in the sources and hold the French dictionary to them: every key there,
//! nothing else there, and the same placeholders on both sides.
//!
//! The window tells the backend which language to speak with `coach_set_locale` before it asks
//! for anything (`api/coach.ts`).

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::{Display, Write};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    Fr,
}

impl Locale {
    /// A BCP 47 tag from the window; anything Coach has no French for is English.
    pub fn from_tag(tag: &str) -> Locale {
        if tag.to_ascii_lowercase().starts_with("fr") {
            Locale::Fr
        } else {
            Locale::En
        }
    }
}

/// The language the window asked for. One for the whole app: there is one rider.
static CURRENT: AtomicU8 = AtomicU8::new(0);

thread_local! {
    /// A language for this thread only, over `CURRENT`. The tests use it, so one in French
    /// can't change the words under another running in English beside it.
    static HERE: Cell<Option<Locale>> = const { Cell::new(None) };
}

pub fn locale() -> Locale {
    HERE.with(|h| h.get()).unwrap_or(if CURRENT.load(Ordering::Relaxed) == 1 { Locale::Fr } else { Locale::En })
}

pub fn set(l: Locale) {
    CURRENT.store(if l == Locale::Fr { 1 } else { 0 }, Ordering::Relaxed);
}

/// Runs `f` with this thread speaking `l`.
#[cfg(test)]
pub fn with<R>(l: Locale, f: impl FnOnce() -> R) -> R {
    let was = HERE.with(|h| h.replace(Some(l)));
    let out = f();
    HERE.with(|h| h.set(was));
    out
}

/// The window's language, before it asks for any advice.
#[tauri::command]
pub fn coach_set_locale(locale: String) {
    set(Locale::from_tag(&locale));
}

const FR_JSON: &str = include_str!("../locales/fr.json");

fn french() -> &'static HashMap<String, String> {
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    FR.get_or_init(|| serde_json::from_str(FR_JSON).expect("locales/fr.json"))
}

/// The sentence `key` in the current language, with each `{name}` filled from `args`.
pub fn render(key: &str, en: &str, args: &[(&str, &dyn Display)]) -> String {
    let template = match locale() {
        Locale::En => en,
        Locale::Fr => french().get(key).map(String::as_str).unwrap_or(en),
    };
    fill(template, args)
}

/// `{name}` replaced by its argument. Anything else in braces is left as it is.
fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_end = after.find('}');
        match name_end.and_then(|e| args.iter().find(|(n, _)| *n == &after[..e]).map(|(_, v)| (e, v))) {
            Some((e, v)) => {
                let _ = write!(out, "{v}");
                rest = &after[e + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A sentence kept for later, for the tables of fixed advice built at compile time.
#[derive(Clone, Copy, Debug)]
pub struct Msg {
    pub key: &'static str,
    pub en: &'static str,
}

impl Msg {
    pub fn text(&self) -> String {
        render(self.key, self.en, &[])
    }
}

/// A number with `places` decimals, the way the current language writes it: a decimal comma in
/// French. English is exactly `format!("{x:.places$}")`.
pub fn num(x: impl Into<f64>, places: usize) -> String {
    let s = format!("{:.*}", places, x.into());
    match locale() {
        Locale::En => s,
        Locale::Fr => s.replace('.', ","),
    }
}

/// The same with its sign always written, `{:+.places$}`.
pub fn signed(x: impl Into<f64>, places: usize) -> String {
    let s = format!("{:+.*}", places, x.into());
    match locale() {
        Locale::En => s,
        Locale::Fr => s.replace('.', ","),
    }
}

/// Whether `n` of something takes the singular: just 1 in English, 0 and 1 in French.
pub fn one(n: usize) -> bool {
    match locale() {
        Locale::En => n == 1,
        Locale::Fr => n <= 1,
    }
}

/// Text for the game's own files, which draw ASCII only: the recorder's block font has A to Z
/// and nothing else, so an accent would be a hole in the word. French reads fine without them
/// at a glance, and better than with gaps.
pub fn ascii(s: &str) -> String {
    s.chars()
        .flat_map(|c| -> Vec<char> {
            match c {
                'à' | 'â' | 'ä' => vec!['a'],
                'À' | 'Â' | 'Ä' => vec!['A'],
                'é' | 'è' | 'ê' | 'ë' => vec!['e'],
                'É' | 'È' | 'Ê' | 'Ë' => vec!['E'],
                'î' | 'ï' => vec!['i'],
                'Î' | 'Ï' => vec!['I'],
                'ô' | 'ö' => vec!['o'],
                'Ô' | 'Ö' => vec!['O'],
                'ù' | 'û' | 'ü' => vec!['u'],
                'Ù' | 'Û' | 'Ü' => vec!['U'],
                'ç' => vec!['c'],
                'Ç' => vec!['C'],
                'œ' => vec!['o', 'e'],
                'Œ' => vec!['O', 'E'],
                '’' | '‘' => vec!['\''],
                '«' | '»' | '“' | '”' => vec!['"'],
                '–' | '—' => vec!['-'],
                '…' => vec!['.', '.', '.'],
                '\u{a0}' | '\u{202f}' => vec![' '],
                c if c.is_ascii() => vec![c],
                _ => vec![],
            }
        })
        .collect()
}

/// A sentence in the current language: `tr!("key", "English with {name}", name = value)`.
macro_rules! tr {
    ($key:literal, $en:literal $(, $name:ident = $val:expr)* $(,)?) => {
        $crate::i18n::render($key, $en, &[$((stringify!($name), &$val as &dyn ::std::fmt::Display)),*])
    };
}

/// A sentence for later, as a `const`: `msg!("key", "English")`.
macro_rules! msg {
    ($key:literal, $en:literal) => {
        $crate::i18n::Msg { key: $key, en: $en }
    };
}

pub(crate) use {msg, tr};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// One `tr!` or `msg!` in the sources.
    struct Use {
        file: String,
        key: String,
        en: String,
        args: BTreeSet<String>,
        msg: bool,
    }

    /// A Rust string literal at the start of `s`: its value and the bytes it took. Handles the
    /// escapes the advice uses, and `\` at a line's end, which drops the break and the indent.
    fn literal(s: &str) -> Option<(String, usize)> {
        let s_bytes = s.as_bytes();
        if s_bytes.first() != Some(&b'"') {
            return None;
        }
        let mut out = String::new();
        let mut chars = s[1..].char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return Some((out, i + 2)),
                '\\' => match chars.next()?.1 {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    '\n' | '\r' => {
                        while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                            chars.next();
                        }
                    }
                    'u' => {
                        // \u{...}
                        let mut hex = String::new();
                        chars.next();
                        for (_, c) in chars.by_ref() {
                            if c == '}' {
                                break;
                            }
                            hex.push(c);
                        }
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    other => panic!("an escape the scanner doesn't know: \\{other}"),
                },
                c => out.push(c),
            }
        }
        None
    }

    /// The names given after the two literals, up to the macro's closing bracket.
    fn arg_names(s: &str) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let (mut depth, mut start, mut in_str) = (0i32, true, false);
        let mut word = String::new();
        let mut prev = ' ';
        for c in s.chars() {
            if in_str {
                if c == '"' && prev != '\\' {
                    in_str = false;
                }
                prev = if prev == '\\' && c == '\\' { ' ' } else { c };
                continue;
            }
            match c {
                '"' => in_str = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => break,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    start = true;
                    word.clear();
                }
                '=' if depth == 0 && start => {
                    names.insert(word.trim().to_string());
                    start = false;
                }
                c if depth == 0 && start => word.push(c),
                _ => {}
            }
            prev = c;
        }
        names.remove("");
        names
    }

    fn uses() -> Vec<Use> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            if path.extension().is_none_or(|x| x != "rs") || path.file_name().is_some_and(|n| n == "i18n.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for (mac, msg) in [("tr!(", false), ("msg!(", true)] {
                let mut at = 0;
                while let Some(i) = src[at..].find(mac) {
                    let start = at + i + mac.len();
                    at = start;
                    // `include_str!(` is not `tr!(`.
                    if src[..start - mac.len()].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = src[start..].trim_start();
                    let Some((key, n)) = literal(rest) else { panic!("{}: {mac} without a literal key", path.display()) };
                    let rest = rest[n..].trim_start().strip_prefix(',').unwrap().trim_start();
                    let Some((en, n)) = literal(rest) else { panic!("{}: {key} without a literal English", path.display()) };
                    out.push(Use {
                        file: path.file_name().unwrap().to_string_lossy().into(),
                        key,
                        en,
                        args: if msg { BTreeSet::new() } else { arg_names(&rest[n..]) },
                        msg,
                    });
                }
            }
        }
        out
    }

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            if let Some(close) = rest.find('}') {
                let name = &rest[..close];
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    out.insert(name.to_string());
                }
            }
        }
        out
    }

    #[test]
    fn the_scanner_reads_the_sources() {
        let all = uses();
        assert!(all.len() > 300, "only {} sentences found", all.len());
        let (key, en) = ("x.y", "a \\\n     b {n}");
        let src = format!("\"{key}\", \"{en}\", n = f(1, 2), m = 3)");
        let (k, n) = literal(&src).unwrap();
        assert_eq!(k, "x.y");
        let rest = src[n..].trim_start().strip_prefix(',').unwrap().trim_start();
        let (e, n) = literal(rest).unwrap();
        assert_eq!(e, "a b {n}");
        assert_eq!(arg_names(&rest[n..]), ["m", "n"].iter().map(|s| s.to_string()).collect());
    }

    #[test]
    fn every_sentence_has_its_french_with_the_same_placeholders() {
        let fr = french();
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        let mut problems = Vec::new();
        for u in uses() {
            if let Some(was) = seen.get(&u.key) {
                if *was != u.en {
                    problems.push(format!("{} ({}): one key, two English sentences", u.key, u.file));
                }
            }
            seen.insert(u.key.clone(), u.en.clone());
            let want = placeholders(&u.en);
            if !u.msg && want != u.args {
                problems.push(format!("{} ({}): the English wants {want:?}, the call gives {:?}", u.key, u.file, u.args));
            }
            match fr.get(&u.key) {
                None => problems.push(format!("{} ({}): no French", u.key, u.file)),
                Some(f) if placeholders(f) != want => {
                    problems.push(format!("{}: French {:?} against English {want:?}", u.key, placeholders(f)))
                }
                Some(f) if f.trim().is_empty() => problems.push(format!("{}: empty French", u.key)),
                _ => {}
            }
        }
        for k in fr.keys() {
            if !seen.contains_key(k) {
                problems.push(format!("{k}: French for a sentence no code says"));
            }
        }
        assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
    }

    #[test]
    fn numbers_and_fills() {
        assert_eq!(fill("{a} and {b}, {not one} {c}", &[("a", &1), ("b", &"two")]), "1 and two, {not one} {c}");
        assert_eq!(num(1.25f32, 1), format!("{:.1}", 1.25f32));
        with(Locale::Fr, || {
            assert_eq!(num(0.354f32, 2), "0,35");
            assert_eq!(signed(3.0f32, 0), "+3");
            assert!(one(0));
        });
        assert!(!one(0));
        assert_eq!(ascii("Lâche les freins à l'entrée — œil"), "Lache les freins a l'entree - oeil");
    }

    #[test]
    fn french_advice_is_french() {
        use crate::analysis::tests::{lap, Style, FAST};
        let bike = crate::analysis::Bike { travel: [0.3, 0.3], limiter: 13000.0, max_rpm: 14000.0, shift_rpm: 12500.0, ..Default::default() };
        let slow = lap(&Style { decel: 2.5, brake: 0.5, ..FAST });
        let (en, fr) = (
            crate::analysis::review(&slow, &lap(&FAST), bike),
            with(Locale::Fr, || crate::analysis::review(&slow, &lap(&FAST), bike)),
        );
        let words = |r: &crate::analysis::Review| -> Vec<String> {
            r.sections.iter().flat_map(|s| s.findings.iter().map(|f| format!("{} {}", f.title, f.detail))).collect()
        };
        let (en, fr) = (words(&en), words(&fr));
        assert!(!fr.is_empty());
        assert_eq!(en.len(), fr.len(), "the same advice, only the words change");
        for (e, f) in en.iter().zip(&fr) {
            assert_ne!(e, f);
        }
    }
}
