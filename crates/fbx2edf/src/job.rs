//! A batch as Frost's Studio and Frost Converter both run it: what dropped paths hold, where
//! each model is written, the `.shd` files written before converting and the `.hrc` files
//! after. Both apps (and the converter's command line) call this, so the same settings make
//! the same files in each.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::host::{self, Event, Options};

/// One file an added path turned out to hold.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: String,
    /// "model", "shadow" (an FBX named like a bike's shadow) or "params" (an export script,
    /// see [`is_export_script`]).
    pub kind: &'static str,
    pub size: u64,
}

/// Whether a dropped file is an export script: the files the Convert screen's script slot
/// loads (`.ini`, `.params`, `.txt`). An `.ini` or `.params` always is; a `.txt` only when it
/// holds a `[Hierarchy]` or `[params]` section, so notes and readmes stay out.
pub fn is_export_script(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "ini" | "params" => true,
        "txt" => {
            std::fs::metadata(path).is_ok_and(|m| m.len() <= 1 << 20)
                && std::fs::read(path).is_ok_and(|b| {
                    let t = String::from_utf8_lossy(&b).to_ascii_lowercase();
                    t.lines().map(str::trim).any(|l| l == "[hierarchy]" || l == "[params]")
                })
        }
        _ => false,
    }
}

fn kind_of(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "fbx" if name.contains("shadow") => Some("shadow"),
        "fbx" => Some("model"),
        _ if is_export_script(path) => Some("params"),
        _ => None,
    }
}

/// What files and folders hold: their FBX files and parameter files, a folder searched all
/// the way down. Each once, in order.
pub fn scan(paths: &[String]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |p: &Path, out: &mut Vec<Found>| {
        let Some(kind) = kind_of(p) else { return };
        if !seen.insert(p.to_string_lossy().to_lowercase()) {
            return;
        }
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        out.push(Found { path: p.to_string_lossy().into_owned(), kind, size });
    };
    for raw in paths {
        let p = Path::new(raw);
        if p.is_dir() {
            let mut files: Vec<PathBuf> = walkdir::WalkDir::new(p)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .collect();
            files.sort();
            for f in &files {
                push(f, &mut out);
            }
        } else if p.is_file() {
            push(p, &mut out);
        }
    }
    out
}

/// How long a watched source rests before it counts as saved: an exporter writes in pieces.
pub const WATCH_SETTLE: std::time::Duration = std::time::Duration::from_millis(700);

/// The watched source `changed` is, if any: the same file whatever the case or the slashes,
/// and not a `.bak` or temporary neighbour.
pub fn watched_match<'a>(watched: &'a [PathBuf], changed: &Path) -> Option<&'a PathBuf> {
    let key = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    let c = key(changed);
    watched.iter().find(|w| key(w) == c)
}

/// A model to write: its FBX, and whether it is a bike's shadow.
#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub path: String,
    pub shadow: bool,
}

/// The base name (no `.edf`) a typed output name comes to: path parts dropped, characters
/// Windows file names can't hold removed, trailing dots and spaces trimmed, a `.edf` suffix
/// taken off. `None` when nothing usable is left, so the default name applies.
pub fn sanitize_output_name(raw: &str) -> Option<String> {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c)).collect();
    let mut stem = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if stem.len() >= 4 && stem[stem.len() - 4..].eq_ignore_ascii_case(".edf") {
        stem.truncate(stem.len() - 4);
        stem = stem.trim().trim_end_matches(['.', ' ']).to_string();
    }
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5",
        "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if stem.is_empty() {
        return None;
    }
    if RESERVED.contains(&stem.to_ascii_lowercase().as_str()) {
        stem.push('_');
    }
    Some(stem)
}

/// Where each model is written: beside it, or in `folder` when one is given. A bike's shadow
/// model is always `shadow_model.edf` and, with `bike`, the bike itself is `model.edf`: the
/// names MX Bikes looks for. Otherwise a model keeps its FBX's name. With `name`, the one
/// non-shadow model takes it instead (see [`sanitize_output_name`]); with several models the
/// name is ignored, as every one would land on it. Two models landing on one name are kept
/// apart with a number, as the browser converter does.
pub fn output_paths(models: &[Model], folder: Option<&Path>, bike: bool, name: Option<&str>) -> Vec<PathBuf> {
    let mut taken = HashSet::new();
    let custom = name.and_then(sanitize_output_name).filter(|_| models.iter().filter(|m| !m.shadow).count() == 1);
    models
        .iter()
        .map(|m| {
            let input = Path::new(&m.path);
            let dir = folder.map(Path::to_path_buf).unwrap_or_else(|| input.parent().map(Path::to_path_buf).unwrap_or_default());
            let stem = if m.shadow {
                "shadow_model".to_string()
            } else if let Some(n) = &custom {
                n.clone()
            } else if bike {
                "model".to_string()
            } else {
                input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
            };
            let mut out = dir.join(format!("{stem}.edf"));
            let mut n = 2;
            while taken.contains(&out.to_string_lossy().to_lowercase()) {
                out = dir.join(format!("{stem}_{n}.edf"));
                n += 1;
            }
            taken.insert(out.to_string_lossy().to_lowercase());
            out
        })
        .collect()
}

/// Whether Parts-planned object names look like a bike's, as the browser converter guesses.
pub fn looks_like_bike(names: &[String]) -> bool {
    let has = |n: &str| names.iter().any(|x| x.eq_ignore_ascii_case(n));
    let others = ["fsusp", "rsusp", "steer"].iter().filter(|n| has(n)).count();
    others >= 2 || (has("chassis") && others >= 1)
}

/// Whether an FBX is a bike: Parts layout and `model.edf` by default.
pub fn is_bike(fbx: &Path) -> Result<bool, String> {
    host::part_names(fbx).map(|names| looks_like_bike(&names))
}

/// When a bike's `.hrc` files are written beside its `.edf`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HrcMode {
    /// Every one; one already there and different is kept as `<name>.bak` first.
    Always,
    /// Only into a folder with no `.hrc` at all.
    #[default]
    IfNone,
    Never,
}

/// The `.hrc` files a converted bike needs beside its `.edf`, in the game's own format, as the
/// browser converter writes them: one per bike part the model has, each naming the `.edf` by
/// its bare file name. `chassis` is the main object, so it goes unnamed. Empty when the model
/// has no steering or suspension part.
pub fn hrc_files(object_names: &[String], edf_name: &str) -> Vec<(String, String)> {
    let names: Vec<String> = object_names.iter().map(|n| n.to_lowercase()).collect();
    let has = |n: &str| names.iter().any(|x| x == n);
    if !names.iter().any(|n| ["steer", "fsusp", "rsusp", "steera", "fsuspa", "rsuspa"].contains(&n.as_str())) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for part in ["chassis", "steer", "fsusp", "rsusp"] {
        let object = if part == "chassis" {
            has("").then(String::new)
        } else if has(part) {
            Some(part.to_string())
        } else if has(&format!("{part}a")) {
            Some(format!("{part}a"))
        } else {
            None
        };
        let Some(object) = object else { continue };
        let mut body = vec![format!("\tscene = {edf_name}")];
        if !object.is_empty() {
            body.push(format!("\tname = {object}"));
        }
        body.push("\tswitch = 0".into());
        let mut lines = vec!["level0".to_string(), "{".into()];
        lines.extend(body);
        lines.extend(["}".to_string(), String::new()]);
        out.push((format!("{part}.hrc"), lines.join("\r\n")));
    }
    out
}

/// `<name>.bak`, or `<name>.bak2`, `.bak3`… when that is taken: never over another file.
fn backup_path(p: &Path) -> PathBuf {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut n = 1;
    loop {
        let candidate = p.with_file_name(if n == 1 { format!("{name}.bak") } else { format!("{name}.bak{n}") });
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Write a bike's `.hrc` files beside its `.edf` as `mode` says. A modder's own are often
/// edited by hand, so none is ever written over unsaid: `IfNone` writes nothing into a folder
/// that has one, and `Always` keeps a different one as `<name>.bak` first. Written, and notes.
fn write_hrc(report: &Value, edf: &Path, mode: HrcMode) -> (Vec<String>, Vec<String>) {
    let names: Vec<String> = report["objects"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| o["name"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let edf_name = edf.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = edf.parent().map(Path::to_path_buf).unwrap_or_default();
    let files = hrc_files(&names, &edf_name);
    let (mut written, mut notes) = (Vec::new(), Vec::new());
    if files.is_empty() || mode == HrcMode::Never {
        return (written, notes);
    }
    if mode == HrcMode::IfNone {
        let has_hrc = std::fs::read_dir(&dir).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("hrc")))
        });
        if has_hrc {
            notes.push("The folder already has .hrc files; none written.".into());
            return (written, notes);
        }
    }
    for (name, text) in files {
        let p = dir.join(&name);
        if p.exists() {
            if std::fs::read_to_string(&p).is_ok_and(|old| old == text) {
                continue;
            }
            let bak = backup_path(&p);
            if let Err(e) = std::fs::rename(&p, &bak) {
                log::warn!("[convert] couldn't keep {} as {}: {e}", p.display(), bak.display());
                notes.push(format!("{name} kept: it couldn't be backed up."));
                continue;
            }
            notes.push(format!("{name} replaced; the old one is {}.", bak.file_name().unwrap_or_default().to_string_lossy()));
        }
        match std::fs::write(&p, text) {
            Ok(()) => written.push(name),
            Err(e) => log::warn!("[convert] couldn't write {}: {e}", p.display()),
        }
    }
    (written, notes)
}

/// The maps found beside a colour picture, by file name.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Maps {
    pub normal: Option<String>,
    pub reflection: Option<String>,
    pub specular: Option<String>,
}

impl Maps {
    fn any(&self) -> bool {
        self.normal.is_some() || self.reflection.is_some() || self.specular.is_some()
    }
}

const NORMAL: &[&str] = &["_n", "_normal", "_nrm"];
const REFLECTION: &[&str] = &["_r", "_refl", "_reflection"];
const SPECULAR: &[&str] = &["_s", "_spec"];
/// What the converter reads, the colour picture's own extension tried first.
const EXTS: &[&str] = &["tga", "png", "bmp", "jpg", "jpeg"];

/// `<name><suffix>.<ext>` in `dir` for each kind of map, matched without regard to case.
pub fn find_maps(dir: &Path, name: &str, ext: Option<&str>) -> Maps {
    let Ok(entries) = std::fs::read_dir(dir) else { return Maps::default() };
    let present: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut exts: Vec<String> = ext.map(|e| e.to_ascii_lowercase()).into_iter().collect();
    exts.extend(EXTS.iter().map(|e| e.to_string()).filter(|e| Some(e.as_str()) != ext));
    let find = |suffixes: &[&str]| {
        suffixes.iter().find_map(|s| {
            exts.iter().find_map(|e| {
                let want = format!("{name}{s}.{e}").to_lowercase();
                present.iter().find(|p| p.to_lowercase() == want).cloned()
            })
        })
    };
    Maps { normal: find(NORMAL), reflection: find(REFLECTION), specular: find(SPECULAR) }
}

/// The shininess a written `.shd` gets unless the modder picks another: what working
/// modded bikes use.
pub const SHININESS: f32 = 6.0;

/// What a `.shd` sets, besides the maps it names: the specular shininess and the reflection's
/// strength range. Stock is shininess 6 and a reflection from 0 to 0.5 with exponent 1.0, as
/// modders' working bikes have it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShdValues {
    pub shininess: f32,
    pub reflection_min: f32,
    pub reflection_max: f32,
    pub reflection_exp: f32,
}

impl Default for ShdValues {
    fn default() -> Self {
        ShdValues { shininess: SHININESS, reflection_min: 0.0, reflection_max: 0.5, reflection_exp: 1.0 }
    }
}

impl ShdValues {
    /// Each value as it is, or the stock one where it isn't a number the game can use.
    pub fn sane(self) -> Self {
        let d = ShdValues::default();
        let ok = |v: f32, d: f32| if v.is_finite() && v >= 0.0 { v } else { d };
        ShdValues {
            shininess: ok(self.shininess, d.shininess),
            reflection_min: ok(self.reflection_min, d.reflection_min),
            reflection_max: ok(self.reflection_max, d.reflection_max),
            reflection_exp: ok(self.reflection_exp, d.reflection_exp),
        }
    }
}

/// A `.shd` naming `maps`, laid out the way `fbx2edf.exe` reads it: each block's name, braces
/// and keys on lines of their own (it skips a block written on one line), in the stock order
/// bump, specular, reflection. Specular and reflection are always there, so every material
/// has its shininess and reflection to tune; a map is named where there is one. With no
/// specular map, the specular mask is the normal map's alpha.
pub fn shd_text(maps: &Maps, values: &ShdValues) -> String {
    let mut out = String::new();
    let mut block = |name: &str, lines: Vec<String>| {
        out.push_str(&format!("{name}\r\n{{\r\n"));
        for l in lines {
            out.push_str(&format!("\t{l}\r\n"));
        }
        out.push_str("}\r\n");
    };
    if let Some(m) = &maps.normal {
        block("bump", vec![format!("map = {m}"), "repetitions = 1".into()]);
    }
    let mut spec = vec![format!("shininess = {}", values.shininess)];
    spec.extend(maps.specular.iter().map(|m| format!("map = {m}")));
    block("specular", spec);
    let exp = if values.reflection_exp.fract() == 0.0 { format!("{:.1}", values.reflection_exp) } else { values.reflection_exp.to_string() };
    let mut refl = vec![format!("factormin = {}", values.reflection_min), format!("factormax = {}", values.reflection_max), format!("factorexp = {exp}")];
    refl.extend(maps.reflection.iter().map(|m| format!("map = {m}")));
    block("reflection", refl);
    out
}

/// `key = value` pairs in `body`, one per entry: the value runs up to where the next
/// `key =` starts, so a path with spaces in it stays whole. Text with no `=` comes back as is.
fn shd_pairs(body: &str) -> Vec<String> {
    let starts: Vec<usize> = body
        .match_indices('=')
        .filter_map(|(eq, _)| {
            let before = body[..eq].trim_end();
            let key_len = before.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').count();
            (key_len > 0).then(|| before.len() - before.chars().rev().take(key_len).map(char::len_utf8).sum::<usize>())
        })
        .collect();
    if starts.is_empty() {
        return Some(body.trim()).filter(|b| !b.is_empty()).map(str::to_string).into_iter().collect();
    }
    let mut out: Vec<String> = Vec::new();
    // Anything before the first key is a block name written on the same line.
    let lead = body[..starts[0]].trim();
    if !lead.is_empty() {
        out.push(lead.to_string());
    }
    for (i, &s) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(body.len());
        let pair = body[s..end].trim().trim_end_matches([';', ',']).trim();
        out.push(pair.to_string());
    }
    out
}

/// A `.shd` as one entry per line: block names, `key = value` pairs and braces, whether the
/// file is laid out the way the converter reads it (a block per few lines) or the way other
/// tools write it (`specular { shininess = 6 }` on one line).
fn shd_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let mut rest = line;
        while !rest.is_empty() {
            match rest.find(['{', '}']) {
                Some(i) => {
                    out.extend(shd_pairs(&rest[..i]));
                    out.push(rest[i..=i].to_string());
                    rest = rest[i + 1..].trim();
                }
                None => {
                    out.extend(shd_pairs(rest));
                    break;
                }
            }
        }
    }
    out
}

/// What a `.shd` on disk names and sets. A key it doesn't have keeps its stock value.
pub fn parse_shd(text: &str) -> (Maps, ShdValues) {
    let (mut maps, mut v) = (Maps::default(), ShdValues::default());
    let mut block = String::new();
    for raw in shd_lines(text) {
        let line = raw.trim();
        if line.is_empty() || line == "{" || line == "}" {
            continue;
        }
        let Some((k, val)) = line.split_once('=') else {
            block = line.to_ascii_lowercase();
            continue;
        };
        let (k, val) = (k.trim().to_ascii_lowercase(), val.trim());
        let num = val.parse::<f32>().ok();
        match (block.as_str(), k.as_str()) {
            ("bump", "map") => maps.normal = Some(val.into()),
            ("specular", "map") => maps.specular = Some(val.into()),
            ("reflection", "map") => maps.reflection = Some(val.into()),
            ("specular", "shininess") => v.shininess = num.unwrap_or(v.shininess),
            ("reflection", "factormin") => v.reflection_min = num.unwrap_or(v.reflection_min),
            ("reflection", "factormax") => v.reflection_max = num.unwrap_or(v.reflection_max),
            ("reflection", "factorexp") => v.reflection_exp = num.unwrap_or(v.reflection_exp),
            _ => {}
        }
    }
    (maps, v)
}

/// `bike.shd: normal bike_n, reflection bike_r`.
fn shd_summary(file: &str, maps: &Maps) -> String {
    let stem = |m: &String| Path::new(m).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let parts: Vec<String> = [("normal", &maps.normal), ("reflection", &maps.reflection), ("specular", &maps.specular)]
        .into_iter()
        .filter_map(|(k, m)| m.as_ref().map(|m| format!("{k} {}", stem(m))))
        .collect();
    format!("{file}: {}", if parts.is_empty() { "no maps".to_string() } else { parts.join(", ") })
}

/// One `.shd` to write: where, what, the summary line, and notes for the modder.
#[derive(Debug, PartialEq)]
pub struct ShdPlan {
    pub path: PathBuf,
    pub text: String,
    pub summary: String,
    pub notes: Vec<String>,
}

/// Whether two folders are the same one, however each is spelled.
fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// A linked map as the `.shd` in `dir` names it: its file name when it sits there, else its
/// full path (the converter reads either). The engine hands linked maps back canonical
/// (`\\?\C:\...`), so folders are compared canonical too, and a full path is written without
/// the `\\?\` prefix.
fn map_name(map: &Path, dir: &Path) -> String {
    match map.parent() {
        Some(p) if same_dir(p, dir) => map.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        _ => strip_verbatim(map),
    }
}

/// A path as the user would type it: `\\?\C:\x` is `C:\x`, `\\?\UNC\host\x` is `\\host\x`.
fn strip_verbatim(path: &Path) -> String {
    let s = path.to_string_lossy();
    match (s.strip_prefix(r"\\?\UNC\"), s.strip_prefix(r"\\?\")) {
        (Some(rest), _) => format!(r"\\{rest}"),
        (None, Some(rest)) => rest.to_string(),
        _ => s.into_owned(),
    }
}

/// The folder and maps a picture's `.shd` names, with notes for the modder. The maps the
/// material links come first; a normal or reflection map it doesn't link but that sits beside
/// the picture by name (`X_n`, `X_r`) is used too, with a note saying so. The folder is the
/// first of the picture's with a map, since the converter reads the maps it names beside it.
/// `None` when the picture has no maps at all.
fn site_maps(site: &host::ShdSite) -> Option<(PathBuf, Maps, Vec<String>)> {
    let linked = site.normal.is_some() || site.reflection.is_some();
    let found = site.dirs.iter().map(|d| (d, find_maps(d, &site.name, site.ext.as_deref()))).find(|(_, m)| m.any());
    let (dir, mut maps) = match found {
        Some((d, m)) => (d.clone(), m),
        None if linked => (site.dirs.first()?.clone(), Maps::default()),
        None => return None,
    };
    let mut notes = Vec::new();
    match &site.normal {
        Some(n) => maps.normal = Some(map_name(n, &dir)),
        None => notes.extend(maps.normal.iter().map(|m| format!("normal map not connected, using {m}"))),
    }
    match &site.reflection {
        Some(r) => maps.reflection = Some(map_name(r, &dir)),
        None => notes.extend(maps.reflection.iter().map(|m| format!("reflection map not connected, using {m}"))),
    }
    Some((dir, maps, notes))
}

/// The `.shd` to write for one picture before converting, if any, in the picture's own folder
/// (the model's folder, never beside the `.edf`): the converter reads `<name>.shd` there and
/// builds the maps it names, and the shininess and reflection, into the model. A picture with
/// maps gets one beside them; one with none gets none, because the converter would bake a
/// values-only `.shd` into the model, which the conversions before it never had. A `.shd` the converter
/// already reads is left alone unless `overwrite`.
pub fn plan_shd(site: &host::ShdSite, overwrite: bool, values: &ShdValues) -> Option<ShdPlan> {
    if site.shd.is_some() && !overwrite {
        return None;
    }
    let (dir, maps, notes) = site_maps(site)?;
    let file = format!("{}.shd", site.name);
    let path = dir.join(&file);
    if path.exists() && !overwrite {
        return None;
    }
    Some(ShdPlan { text: shd_text(&maps, values), summary: shd_summary(&file, &maps), path, notes })
}

/// The `.shd` files written before a model is converted.
#[derive(Default)]
struct PreShd {
    summaries: Vec<String>,
    notes: Vec<String>,
    paths: Vec<PathBuf>,
    sites: Vec<host::ShdSite>,
}

/// Write the `.shd` files an FBX's textures lack, before it is converted, so the converter
/// builds their maps in. `done` holds the files written so far in the batch: a texture two
/// models share is written once.
fn write_shd(fbx: &Path, overwrite: bool, values: &ShdValues, done: &mut HashSet<PathBuf>) -> PreShd {
    let sites = match host::shd_sites(fbx) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[convert] no .shd files for {}: {e}", fbx.display());
            return PreShd::default();
        }
    };
    let mut pre = PreShd::default();
    for site in &sites {
        let Some(plan) = plan_shd(site, overwrite, values) else { continue };
        if !done.insert(plan.path.clone()) {
            continue;
        }
        match std::fs::write(&plan.path, plan.text) {
            Ok(()) => {
                pre.summaries.push(plan.summary);
                pre.notes.extend(plan.notes);
                pre.paths.push(plan.path);
            }
            Err(e) => log::warn!("[convert] couldn't write {}: {e}", plan.path.display()),
        }
    }
    pre.sites = sites;
    pre
}

/// One material's `.shd` beside the model, as it is on disk now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShdFile {
    /// The material's picture, without extension: the `.shd`'s name.
    pub name: String,
    pub path: String,
    /// Written by this conversion, rather than one that was already there.
    pub written: bool,
    pub maps: Maps,
    pub values: ShdValues,
}

/// Write a `.shd` with these maps and values: what the sliders do.
pub fn save_shd(path: &Path, maps: &Maps, values: &ShdValues) -> Result<(), String> {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("shd")) {
        return Err(format!("{} isn't a .shd file.", path.display()));
    }
    std::fs::write(path, shd_text(maps, &values.sane())).map_err(|e| format!("{}: {e}", path.display()))
}

/// Each material's `.shd` in the model's folder, as it is on disk now: the one the converter
/// read, or the one written for it. Nothing is written here, and nothing goes beside the `.edf`.
fn source_shd_files(pre: &PreShd, values: &ShdValues) -> Vec<ShdFile> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for site in &pre.sites {
        if site.name.is_empty() || !seen.insert(site.name.to_lowercase()) {
            continue;
        }
        let Some(path) = site.shd.clone().or_else(|| plan_shd(site, true, values).map(|p| p.path)) else { continue };
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let (maps, now) = parse_shd(&text);
        let written = pre.paths.contains(&path);
        out.push(ShdFile { name: site.name.clone(), path: path.to_string_lossy().into_owned(), written, maps, values: now });
    }
    out
}

/// The `.shd` files a batch writes.
#[derive(Debug, Clone, Copy)]
pub struct Shd {
    /// Write a `.shd` for every material. On unless asked otherwise (`--no-shd`).
    pub make: bool,
    /// Write over the ones there.
    pub overwrite: bool,
    pub values: ShdValues,
}

impl Default for Shd {
    fn default() -> Self {
        Shd { make: true, overwrite: false, values: ShdValues::default() }
    }
}

/// How one model went.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub report: Option<Value>,
    pub error: Option<String>,
    /// The `.hrc` files written beside a bike's `.edf`.
    pub hrc: Vec<String>,
    /// The `.shd` files written in the model's folder, each with the maps it names:
    /// `bike.shd: normal bike_n, reflection bike_r`.
    pub shd: Vec<String>,
    /// What this model wrote beside its `.edf`: the `.edf` and its `.hrc` files.
    pub files: Vec<String>,
    /// The `.shd` files this conversion made in the model's folder, where the converter reads them.
    pub shd_files: Vec<String>,
    /// Each material's `.shd` in the model's folder, written now or already there, for tuning.
    pub shaders: Vec<ShdFile>,
    /// What writing those files decided for the modder: a map used by name because the
    /// material doesn't link it, `.hrc` files kept or replaced.
    pub notes: Vec<String>,
}

/// What a batch says as it goes. `index` is the pair's place in the batch.
pub enum Progress<'a> {
    /// `done` of `total` textures made.
    Pictures { index: usize, done: usize, total: usize },
    /// One model finished, written or not.
    Done { index: usize, outcome: &'a Outcome },
}

/// Convert a batch: the `.shd` files its textures need first (with `shd.make`), then every
/// model through one pool, then each bike's `.hrc`
/// files as `hrc` says. One outcome per pair, in order; `Err` only when the options are
/// refused, before anything is read.
pub fn run(pairs: &[(PathBuf, PathBuf)], options: &Options, hrc: HrcMode, shd: Shd, on: &mut dyn FnMut(Progress)) -> Result<Vec<Outcome>, String> {
    let values = shd.values.sane();
    let mut done = HashSet::new();
    let mut pre: Vec<PreShd> = pairs
        .iter()
        .map(|(fbx, _)| if shd.make { write_shd(fbx, shd.overwrite, &values, &mut done) } else { Default::default() })
        .collect();
    let mut outcomes: Vec<Option<Outcome>> = (0..pairs.len()).map(|_| None).collect();
    let results = host::convert(pairs, options, &mut |event| match event {
        Event::Pictures { index, done, total } => on(Progress::Pictures { index, done, total }),
        Event::Finished { index, result } => {
            let pre = std::mem::take(&mut pre[index]);
            let outcome = outcome_of(result, hrc, &pairs[index].1, pre, shd.make.then_some(&values));
            on(Progress::Done { index, outcome: &outcome });
            outcomes[index] = Some(outcome);
        }
    })?;
    Ok(results
        .into_iter()
        .zip(outcomes)
        .enumerate()
        .map(|(i, (r, o))| o.unwrap_or_else(|| outcome_of(r, hrc, &pairs[i].1, Default::default(), None)))
        .collect())
}

fn outcome_of(result: Result<Value, String>, hrc: HrcMode, edf: &Path, pre: PreShd, finish: Option<&ShdValues>) -> Outcome {
    let notes_before = pre.notes.clone();
    let shd = pre.summaries.clone();
    match result {
        Ok(report) => {
            let shaders = finish.map(|values| source_shd_files(&pre, values)).unwrap_or_default();
            let mut notes = notes_before;
            let (hrc, hrc_notes) = write_hrc(&report, edf, hrc);
            notes.extend(hrc_notes);
            let dir = edf.parent().map(Path::to_path_buf).unwrap_or_default();
            let files = std::iter::once(edf.to_path_buf())
                .chain(hrc.iter().map(|h| dir.join(h)))
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let shd_files = pre.paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            Outcome { report: Some(report), error: None, hrc, shd, files, shd_files, shaders, notes }
        }
        Err(e) => Outcome { error: Some(e), shd, notes: notes_before, ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_gives_its_fbx_and_ini_files_once() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("sub")).unwrap();
        for f in ["bike.FBX", "Shadow_bike.fbx", "sub/helmet.fbx", "params.ini", "paint.png", "bike.edf"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let root = d.to_string_lossy().into_owned();
        let one = d.join("bike.FBX").to_string_lossy().into_owned();
        let found = scan(&[root, one]);
        let kinds: Vec<(String, &str)> = found
            .iter()
            .map(|f| (Path::new(&f.path).file_name().unwrap().to_string_lossy().into_owned(), f.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("Shadow_bike.fbx".to_string(), "shadow"),
                ("bike.FBX".to_string(), "model"),
                ("params.ini".to_string(), "params"),
                ("helmet.fbx".to_string(), "model"),
            ]
        );
    }

    #[test]
    fn export_scripts_are_told_from_models_and_other_text() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        // PiBoSo's bike template's export_script.ini, shortened.
        let template = "[Hierarchy]\r\nsplit = 1\r\nchassis = C:\\MX Bikes\\bikes\\template\\model.edf\r\nchassis_rot = 0 180 0\r\nsteer_rot = -90 180 0\r\n";
        std::fs::write(d.join("export_script.ini"), template).unwrap();
        std::fs::write(d.join("export_script.txt"), template).unwrap();
        std::fs::write(d.join("tweaks.params"), "[params]\nscaling = 2\n").unwrap();
        std::fs::write(d.join("readme.txt"), "How to install: copy the folder.").unwrap();
        std::fs::write(d.join("bike.fbx"), b"x").unwrap();
        for (file, script) in [("export_script.ini", true), ("export_script.txt", true), ("tweaks.params", true), ("readme.txt", false), ("bike.fbx", false)] {
            assert_eq!(is_export_script(&d.join(file)), script, "{file}");
        }
        // Dropped together, a script is "params", never a model to convert.
        let paths: Vec<String> = ["bike.fbx", "export_script.txt", "readme.txt"].iter().map(|f| d.join(f).to_string_lossy().into_owned()).collect();
        let kinds: Vec<&str> = scan(&paths).iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["model", "params"]);
    }

    fn model(path: &str, shadow: bool) -> Model {
        Model { path: path.into(), shadow }
    }

    #[test]
    fn outputs_take_the_names_the_game_loads_and_never_collide() {
        let models = [model("a/ktm.fbx", false), model("a/Shadow_ktm.fbx", true), model("b/ktm.fbx", false)];
        let names = |v: Vec<PathBuf>| v.into_iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
        assert_eq!(names(output_paths(&models, None, true, None)), ["a/model.edf", "a/shadow_model.edf", "b/model.edf"]);
        assert_eq!(names(output_paths(&models, None, false, None)), ["a/ktm.edf", "a/shadow_model.edf", "b/ktm.edf"]);
        assert_eq!(names(output_paths(&models, Some(Path::new("out")), false, None)), ["out/ktm.edf", "out/shadow_model.edf", "out/ktm_2.edf"]);
    }

    #[test]
    fn a_typed_name_is_cleaned_and_gets_the_edf_suffix() {
        assert_eq!(sanitize_output_name("  my bike "), Some("my bike".into()));
        assert_eq!(sanitize_output_name("my bike.EDF"), Some("my bike".into()));
        assert_eq!(sanitize_output_name("C:\\models/ktm:250?.edf"), Some("ktm250".into()));
        assert_eq!(sanitize_output_name("a<b>c|d*e\"f"), Some("abcdef".into()));
        assert_eq!(sanitize_output_name("name..."), Some("name".into()));
        assert_eq!(sanitize_output_name("nul"), Some("nul_".into()));
        assert_eq!(sanitize_output_name(""), None);
        assert_eq!(sanitize_output_name("  .edf "), None);
        assert_eq!(sanitize_output_name("???"), None);
    }

    #[test]
    fn a_typed_name_replaces_the_one_models_name_and_empty_keeps_the_default() {
        let one = [model("a/ktm.fbx", false)];
        let names = |v: Vec<PathBuf>| v.iter().map(|p| p.to_string_lossy().replace('\\', "/")).collect::<Vec<_>>();
        assert_eq!(names(output_paths(&one, None, false, Some("race"))), ["a/race.edf"]);
        assert_eq!(names(output_paths(&one, None, true, Some("race.edf"))), ["a/race.edf"]);
        assert_eq!(names(output_paths(&one, Some(Path::new("out")), false, Some("race"))), ["out/race.edf"]);
        assert_eq!(names(output_paths(&one, None, true, Some(""))), ["a/model.edf"]);
        assert_eq!(names(output_paths(&one, None, false, Some("  "))), ["a/ktm.edf"]);
        // A bike's shadow keeps the name the game looks for.
        let bike = [model("a/ktm.fbx", false), model("a/ktm_shadow.fbx", true)];
        assert_eq!(names(output_paths(&bike, None, true, Some("race"))), ["a/race.edf", "a/shadow_model.edf"]);
        // Several models would land on one name: the field is ignored.
        let many = [model("a/ktm.fbx", false), model("b/yz.fbx", false)];
        assert_eq!(names(output_paths(&many, None, false, Some("race"))), ["a/ktm.edf", "b/yz.edf"]);
    }

    #[test]
    fn a_shd_keeps_its_picture_s_name_and_the_hrc_names_the_typed_edf() {
        // .shd files are named by their material's picture, whatever the .edf is called.
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("race.edf");
        std::fs::write(dir.path().join("chain_n.tga"), b"x").unwrap();
        let sites = vec![site("chain", &[dir.path()], None)];
        let pre_shd = write_shd_with(&sites, &ShdValues::default(), &mut HashSet::new());
        let o = outcome_of(Ok(report(&["", "steer", "fsusp"])), HrcMode::Always, &edf, pre_shd, Some(&ShdValues::default()));
        assert!(dir.path().join("chain.shd").exists());
        assert_eq!(o.files[0], edf.to_string_lossy());
        assert!(o.shd_files.iter().any(|f| f.ends_with("chain.shd")));
        let hrc = std::fs::read_to_string(dir.path().join("steer.hrc")).unwrap();
        assert!(hrc.contains("scene = race.edf"), "{hrc}");
    }

    #[test]
    fn a_change_matches_only_a_watched_file_whatever_its_spelling() {
        let watched = vec![PathBuf::from("C:/m/Bike.fbx"), PathBuf::from("C:/m/shadow.fbx")];
        assert_eq!(watched_match(&watched, Path::new(r"c:\m\bike.FBX")), Some(&watched[0]));
        assert_eq!(watched_match(&watched, Path::new("C:/m/bike.fbx.tmp")), None);
        assert_eq!(watched_match(&watched, Path::new("C:/m/other.fbx")), None);
    }

    #[test]
    fn a_bike_is_told_by_its_parts() {
        let v = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(looks_like_bike(&v(&["", "steer", "fsusp", "rsusp"])));
        assert!(looks_like_bike(&v(&["chassis", "Steer"])));
        assert!(!looks_like_bike(&v(&["", "steer"])));
        assert!(!looks_like_bike(&v(&[""])));
    }

    #[test]
    fn a_bike_gets_the_browser_converter_s_hrc_files() {
        let names: Vec<String> = ["", "steer", "fsuspa", "rsusp"].iter().map(|s| s.to_string()).collect();
        let files = hrc_files(&names, "ktm.edf");
        let got: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(got, ["chassis.hrc", "steer.hrc", "fsusp.hrc", "rsusp.hrc"]);
        assert_eq!(files[0].1, "level0\r\n{\r\n\tscene = ktm.edf\r\n\tswitch = 0\r\n}\r\n");
        assert_eq!(files[2].1, "level0\r\n{\r\n\tscene = ktm.edf\r\n\tname = fsuspa\r\n\tswitch = 0\r\n}\r\n");
        assert!(hrc_files(&["".to_string()], "helmet.edf").is_empty(), "a single model has none");
    }

    fn site(name: &str, dirs: &[&Path], shd: Option<PathBuf>) -> host::ShdSite {
        host::ShdSite {
            name: name.into(),
            ext: Some("tga".into()),
            dirs: dirs.iter().map(|d| d.to_path_buf()).collect(),
            shd,
            normal: None,
            reflection: None,
        }
    }

    #[test]
    fn maps_are_found_by_their_suffix_whatever_the_case() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["bike.tga", "Bike_N.TGA", "bike_r.png", "bike_spec.tga", "other_n.tga"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let m = find_maps(dir.path(), "bike", Some("tga"));
        assert_eq!(m.normal.as_deref(), Some("Bike_N.TGA"));
        assert_eq!(m.reflection.as_deref(), Some("bike_r.png"));
        assert_eq!(m.specular.as_deref(), Some("bike_spec.tga"));
        assert!(!find_maps(dir.path(), "frame", Some("tga")).any());
    }

    #[test]
    fn a_shd_is_written_in_the_stock_order_the_way_fbx2edf_exe_reads_it() {
        let maps = Maps { normal: Some("X_n.tga".into()), reflection: Some("X_r.tga".into()), specular: None };
        // Husk's working file: bump{map=X_n.tga, repetitions=1} specular{shininess=6}
        // reflection{factormin=0, factormax=0.5, factorexp=1.0, map=X_r.tga}, one key a line.
        assert_eq!(
            shd_text(&maps, &ShdValues::default()),
            "bump\r\n{\r\n\tmap = X_n.tga\r\n\trepetitions = 1\r\n}\r\n\
             specular\r\n{\r\n\tshininess = 6\r\n}\r\n\
             reflection\r\n{\r\n\tfactormin = 0\r\n\tfactormax = 0.5\r\n\tfactorexp = 1.0\r\n\tmap = X_r.tga\r\n}\r\n"
        );
        let spec = Maps { specular: Some("bike_s.tga".into()), ..Maps::default() };
        let tuned = ShdValues { shininess: 12.5, reflection_max: 0.8, reflection_exp: 2.0, ..ShdValues::default() };
        assert_eq!(
            shd_text(&spec, &tuned),
            "specular\r\n{\r\n\tshininess = 12.5\r\n\tmap = bike_s.tga\r\n}\r\n\
             reflection\r\n{\r\n\tfactormin = 0\r\n\tfactormax = 0.8\r\n\tfactorexp = 2.0\r\n}\r\n"
        );
        let refl = Maps { reflection: Some("bike_r.tga".into()), ..Maps::default() };
        assert!(shd_text(&refl, &ShdValues::default()).ends_with("map = bike_r.tga\r\n}\r\n"), "an _r alone still gets its reflection");
        // No maps at all: still the stock shininess and the reflection block.
        assert_eq!(
            shd_text(&Maps::default(), &ShdValues::default()),
            "specular\r\n{\r\n\tshininess = 6\r\n}\r\n\
             reflection\r\n{\r\n\tfactormin = 0\r\n\tfactormax = 0.5\r\n\tfactorexp = 1.0\r\n}\r\n"
        );
        assert_eq!(shd_summary("X.shd", &maps), "X.shd: normal X_n, reflection X_r");
        assert_eq!(shd_summary("X.shd", &Maps::default()), "X.shd: no maps");
    }

    #[test]
    fn a_shd_goes_where_the_maps_are_and_never_over_one_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (fbm, root) = (dir.path().join("bike.fbm"), dir.path().to_path_buf());
        std::fs::create_dir(&fbm).unwrap();
        std::fs::write(root.join("bike_n.tga"), b"x").unwrap();
        // Embedded paint: the .fbm folder has no maps, the FBX's folder has.
        let plan = plan_shd(&site("bike", &[&fbm, &root], None), false, &ShdValues::default()).unwrap();
        assert_eq!(plan.path, root.join("bike.shd"));
        assert!(plan.text.contains("map = bike_n.tga"));
        assert_eq!(plan.summary, "bike.shd: normal bike_n");

        let existing = Some(root.join("bike.shd"));
        std::fs::write(root.join("bike.shd"), "mine").unwrap();
        assert!(plan_shd(&site("bike", &[&root], existing.clone()), false, &ShdValues::default()).is_none(), "the converter's own is kept");
        assert!(plan_shd(&site("bike", &[&root], None), false, &ShdValues::default()).is_none(), "nor one it would miss");
        assert_eq!(plan_shd(&site("bike", &[&root], existing), true, &ShdValues::default()).unwrap().path, root.join("bike.shd"));
        assert!(plan_shd(&site("frame", &[&root], None), true, &ShdValues::default()).is_none(), "no maps, no .shd: the model must convert as it always did");
    }

    #[test]
    fn maps_found_by_name_say_so_and_linked_ones_win() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        for f in ["X_n.tga", "X_r.tga"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        let plan = plan_shd(&site("X", &[&root], None), false, &ShdValues::default()).unwrap();
        assert_eq!(plan.notes, ["normal map not connected, using X_n.tga", "reflection map not connected, using X_r.tga"]);
        assert!(plan.text.contains("map = X_r.tga"));

        // Linked in Blender: those are used, by name when beside the .shd, and nothing is said.
        let maps = root.join("maps");
        std::fs::create_dir(&maps).unwrap();
        std::fs::write(maps.join("X_normal.tga"), b"x").unwrap();
        let linked = host::ShdSite { normal: Some(maps.join("X_normal.tga")), reflection: Some(root.join("X_r.tga")), ..site("X", &[&root], None) };
        let plan = plan_shd(&linked, false, &ShdValues::default()).unwrap();
        assert!(plan.notes.is_empty(), "{:?}", plan.notes);
        assert!(plan.text.contains(&format!("map = {}", maps.join("X_normal.tga").display())), "{}", plan.text);
        assert!(plan.text.contains("map = X_r.tga"));

        // Linked maps alone, nothing beside the picture by name: still a .shd.
        let empty = tempfile::tempdir().unwrap();
        let only = host::ShdSite { reflection: Some(root.join("X_r.tga")), ..site("Y", &[empty.path()], None) };
        let plan = plan_shd(&only, false, &ShdValues::default()).unwrap();
        assert_eq!(plan.path, empty.path().join("Y.shd"));
        assert!(plan.text.contains("factormax = 0.5"));
    }

    /// The engine hands linked maps back canonical (`\\?\C:\...`): one beside the `.shd` is
    /// still named bare, and one elsewhere is written without the prefix.
    #[cfg(windows)]
    #[test]
    fn verbatim_linked_maps_are_named_by_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(root.join("bike_n.tga"), b"x").unwrap();
        std::fs::write(other.path().join("bike_r.tga"), b"x").unwrap();
        let normal = root.join("bike_n.tga").canonicalize().unwrap();
        let reflection = other.path().join("bike_r.tga").canonicalize().unwrap();
        assert!(normal.to_string_lossy().starts_with(r"\\?\"), "{}", normal.display());
        let linked = host::ShdSite { normal: Some(normal), reflection: Some(reflection.clone()), ..site("bike", &[&root], None) };
        let plan = plan_shd(&linked, false, &ShdValues::default()).unwrap();
        assert!(plan.text.contains("map = bike_n.tga"), "{}", plan.text);
        assert!(!plan.text.contains(r"\\?\"), "{}", plan.text);
        assert!(plan.text.contains(&format!("map = {}", strip_verbatim(&reflection))), "{}", plan.text);
        assert_eq!(strip_verbatim(Path::new(r"\\?\UNC\host\share\a.tga")), r"\\host\share\a.tga");
    }

    #[test]
    fn a_shd_reads_back_as_it_was_written() {
        let maps = Maps { normal: Some("X_n.tga".into()), reflection: Some("X_r.tga".into()), specular: Some("X_s.tga".into()) };
        let values = ShdValues { shininess: 20.0, reflection_min: 0.1, reflection_max: 0.9, reflection_exp: 2.5 };
        assert_eq!(parse_shd(&shd_text(&maps, &values)), (maps, values));
        assert_eq!(parse_shd("garbage"), (Maps::default(), ShdValues::default()));
        let bad = ShdValues { shininess: f32::NAN, reflection_min: -1.0, ..ShdValues::default() };
        assert_eq!(bad.sane(), ShdValues::default());
    }

    /// Other tools write each block on one line; the converter's own layout and these must read
    /// the same.
    #[test]
    fn a_shd_reads_the_same_on_one_line_as_over_several() {
        let multi = "bump\n{\n\tmap = X_n.tga\n\trepetitions = 1\n}\nspecular\n{\n\tshininess = 12\n\tmap = X_s.tga\n}\nreflection\n{\n\tfactormin = 0.1\n\tfactormax = 0.7\n\tfactorexp = 2.0\n\tmap = X_r.tga\n}\n";
        let one = "bump { map = X_n.tga repetitions = 1 }\nspecular { shininess = 12 map = X_s.tga }\nreflection { factormin = 0.1 factormax = 0.7 factorexp = 2.0 map = X_r.tga }\n";
        let (m, v) = parse_shd(multi);
        assert_eq!((m.normal.as_deref(), m.specular.as_deref(), m.reflection.as_deref()), (Some("X_n.tga"), Some("X_s.tga"), Some("X_r.tga")));
        assert_eq!((v.shininess, v.reflection_min, v.reflection_max, v.reflection_exp), (12.0, 0.1, 0.7, 2.0));
        assert_eq!(parse_shd(one), (m, v));
        // Braces on their own lines, pairs on the name's line, a path with spaces, `;` between pairs.
        let mixed = "specular {\n shininess = 12; map = C:\\My Maps\\X_s.tga }\nbump{map=X_n.tga}";
        let (m, v) = parse_shd(mixed);
        assert_eq!((m.specular.as_deref(), m.normal.as_deref(), v.shininess), (Some("C:\\My Maps\\X_s.tga"), Some("X_n.tga"), 12.0));
    }

    /// What the Blender kit writes (`export/shd.py`), spacing and all.
    #[test]
    fn a_shd_written_by_the_kit_reads_back() {
        let kit = "specular   { shininess = 30 }\nreflection { factormin = 0.25  factormax = 1  factorexp = 1  map = mypage_r.tga }\nbump       { map = mypage_n.tga  repetitions = 1 }\n";
        let (m, v) = parse_shd(kit);
        assert_eq!((m.normal.as_deref(), m.reflection.as_deref(), m.specular), (Some("mypage_n.tga"), Some("mypage_r.tga"), None));
        assert_eq!((v.shininess, v.reflection_min, v.reflection_max, v.reflection_exp), (30.0, 0.25, 1.0, 1.0));
    }

    fn report(objects: &[&str]) -> Value {
        serde_json::json!({ "objects": objects.iter().map(|n| serde_json::json!({ "name": n })).collect::<Vec<_>>() })
    }

    fn pre(sites: Vec<host::ShdSite>) -> PreShd {
        PreShd { sites, ..Default::default() }
    }

    /// `write_shd` without the engine reading an FBX for its sites.
    fn write_shd_with(sites: &[host::ShdSite], values: &ShdValues, done: &mut HashSet<PathBuf>) -> PreShd {
        let mut pre = PreShd::default();
        for s in sites {
            if let Some(plan) = plan_shd(s, false, values) {
                if done.insert(plan.path.clone()) {
                    std::fs::write(&plan.path, plan.text).unwrap();
                    pre.paths.push(plan.path);
                }
            }
        }
        pre.sites = sites.to_vec();
        pre
    }

    /// The `.edf` goes to another folder than the model's pictures. The `.shd` files live with
    /// the pictures, where the converter reads them, and nothing lands beside the `.edf`.
    #[test]
    fn shd_files_are_made_in_the_models_folder_and_never_beside_the_edf() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("bike_n.tga"), b"x").unwrap();
        std::fs::write(src.path().join("chain_n.tga"), b"x").unwrap();
        let edf = out.path().join("bike.edf");
        let sites = vec![site("bike", &[src.path()], None), site("chain", &[src.path()], None)];
        let pre_shd = write_shd_with(&sites, &ShdValues::default(), &mut HashSet::new());
        let o = outcome_of(Ok(report(&[""])), HrcMode::Never, &edf, pre_shd, Some(&ShdValues::default()));
        assert!(src.path().join("bike.shd").exists() && src.path().join("chain.shd").exists());
        assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0, "nothing is written to the output folder");
        assert_eq!(o.files, [edf.to_string_lossy()]);
        assert_eq!(o.shd_files.len(), 2);
        assert!(o.shaders.iter().all(|f| Path::new(&f.path).starts_with(src.path()) && f.written));
        assert_eq!(o.shaders[0].maps.normal.as_deref(), Some("bike_n.tga"));
    }

    /// 0.1.6 wrote a values-only `.shd` for a picture with no maps before converting, so the
    /// converter baked it into the model: nothing is written for one now.
    #[test]
    fn a_picture_with_no_maps_gets_no_shd_before_converting() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("model.edf");
        let sites = vec![site("frame", &[dir.path()], None)];
        let pre_shd = write_shd_with(&sites, &ShdValues::default(), &mut HashSet::new());
        let o = outcome_of(Ok(report(&[""])), HrcMode::Never, &edf, pre_shd, Some(&ShdValues::default()));
        assert!(!dir.path().join("frame.shd").exists());
        assert!(o.shd_files.is_empty() && o.shaders.is_empty());
    }

    /// A chain worn round the neck: one object, one plain colour picture, no maps.
    #[test]
    fn bike_parts_get_a_shd_per_material_and_every_file_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("model.edf");
        for f in ["plastics_n.tga", "frame_n.tga"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let sites = vec![site("plastics", &[dir.path()], None), site("frame", &[dir.path()], None), site("plastics", &[dir.path()], None)];
        let pre_shd = write_shd_with(&sites, &ShdValues::default(), &mut HashSet::new());
        let o = outcome_of(Ok(report(&["", "steer"])), HrcMode::Always, &edf, pre_shd, Some(&ShdValues::default()));
        assert_eq!(o.shaders.len(), 2, "a picture is one material however often it is named");
        let names = |v: &[String]| v.iter().map(|f| Path::new(f).file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(names(&o.files), ["model.edf", "chassis.hrc", "steer.hrc"]);
        assert_eq!(names(&o.shd_files), ["plastics.shd", "frame.shd"]);
    }

    #[test]
    fn a_shd_already_there_is_never_written_over_and_shows_its_own_values() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("chain.edf");
        let tuned = ShdValues { shininess: 30.0, ..ShdValues::default() };
        let mine = shd_text(&Maps::default(), &tuned);
        std::fs::write(dir.path().join("chain.shd"), &mine).unwrap();
        let sites = vec![site("chain", &[dir.path()], Some(dir.path().join("chain.shd")))];
        let pre_shd = write_shd_with(&sites, &ShdValues::default(), &mut HashSet::new());
        let o = outcome_of(Ok(report(&[""])), HrcMode::Never, &edf, pre_shd, Some(&ShdValues::default()));
        assert_eq!(std::fs::read_to_string(dir.path().join("chain.shd")).unwrap(), mine);
        assert!(!o.shaders[0].written && o.shaders[0].values == tuned);
        assert!(o.shd_files.is_empty(), "only what was written is listed");
    }

    /// Make missing .shd off: none is written and one already there is left as it is.
    #[test]
    fn off_writes_no_shd_and_leaves_an_existing_one_alone() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("chain.edf");
        std::fs::write(dir.path().join("chain.shd"), "mine").unwrap();
        let sites = vec![site("chain", &[dir.path()], None), site("frame", &[dir.path()], None)];
        let o = outcome_of(Ok(report(&[""])), HrcMode::Never, &edf, pre(sites), None);
        assert_eq!(std::fs::read_to_string(dir.path().join("chain.shd")).unwrap(), "mine");
        assert!(!dir.path().join("frame.shd").exists());
        assert_eq!(o.files, [edf.to_string_lossy()]);
        assert!(o.shaders.is_empty() && o.shd.is_empty() && o.shd_files.is_empty());
    }

    #[test]
    fn saving_a_shd_writes_the_sliders_values_and_only_to_a_shd() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chain.shd");
        let values = ShdValues { shininess: 40.0, reflection_max: 0.2, ..ShdValues::default() };
        save_shd(&path, &Maps::default(), &values).unwrap();
        assert_eq!(parse_shd(&std::fs::read_to_string(&path).unwrap()).1, values);
        assert!(save_shd(&dir.path().join("chain.edf"), &Maps::default(), &values).is_err());
    }

    fn bike_report() -> Value {
        serde_json::json!({ "objects": [{ "name": "" }, { "name": "steer" }] })
    }

    #[test]
    fn hrc_if_none_writes_only_into_a_folder_without_any() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("model.edf");
        let (written, notes) = write_hrc(&bike_report(), &edf, HrcMode::IfNone);
        assert_eq!(written, ["chassis.hrc", "steer.hrc"]);
        assert!(notes.is_empty());

        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("steer.hrc"), "mine").unwrap();
        let (written, notes) = write_hrc(&bike_report(), &other.path().join("model.edf"), HrcMode::IfNone);
        assert!(written.is_empty(), "one .hrc there is enough to write none");
        assert_eq!(notes, ["The folder already has .hrc files; none written."]);
        assert!(!other.path().join("chassis.hrc").exists());
        assert_eq!(std::fs::read_to_string(other.path().join("steer.hrc")).unwrap(), "mine");
    }

    #[test]
    fn hrc_never_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(write_hrc(&bike_report(), &dir.path().join("model.edf"), HrcMode::Never), (vec![], vec![]));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn hrc_always_keeps_a_different_one_as_a_backup_first() {
        let dir = tempfile::tempdir().unwrap();
        let edf = dir.path().join("model.edf");
        std::fs::write(dir.path().join("steer.hrc"), "mine").unwrap();
        std::fs::write(dir.path().join("steer.hrc.bak"), "older").unwrap();
        let (written, notes) = write_hrc(&bike_report(), &edf, HrcMode::Always);
        assert_eq!(written, ["chassis.hrc", "steer.hrc"]);
        assert_eq!(notes, ["steer.hrc replaced; the old one is steer.hrc.bak2."]);
        assert_eq!(std::fs::read_to_string(dir.path().join("steer.hrc.bak2")).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(dir.path().join("steer.hrc.bak")).unwrap(), "older", "never over another file");
        assert!(std::fs::read_to_string(dir.path().join("steer.hrc")).unwrap().contains("name = steer"));

        // The same again: nothing differs, so nothing is backed up or said.
        let (written, notes) = write_hrc(&bike_report(), &edf, HrcMode::Always);
        assert!(written.is_empty() && notes.is_empty(), "{written:?} {notes:?}");
    }

    #[test]
    fn hrc_modes_read_as_the_page_sends_them() {
        for (s, m) in [("\"always\"", HrcMode::Always), ("\"ifNone\"", HrcMode::IfNone), ("\"never\"", HrcMode::Never)] {
            assert_eq!(serde_json::from_str::<HrcMode>(s).unwrap(), m);
        }
    }
}
