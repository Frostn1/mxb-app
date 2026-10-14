//! What to import: finding the models in a dropped file or folder, and the plan the Blender
//! script builds from.
//!
//! A creator-locked `.pkz` is opened the way MXB App opens it (`mxb_core::pkz`), in memory only:
//! its files are held in a [`Mem`] tree and handed to Blender over a pipe, never written out.
//! An `.mxbsecure` blob, an encrypted zip entry, an `.edf` without the EDF magic or a sealed paint
//! is locked content: it is counted, named in the outcome and never opened further.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A dropped path, as the window lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub path: String,
    /// `pkz`, `edf`, `fbx` or `folder`.
    pub kind: String,
    pub locked: bool,
    /// The model folders inside a `.pkz`, or the folder's own name.
    pub models: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub lods: bool,
    pub shadow: bool,
    pub place: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { lods: false, shadow: false, place: true }
    }
}

/// One EDF object to load: `object` None = every object in the file, "" = the main one.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Level {
    pub edf: String,
    pub object: Option<String>,
}

/// A `gfx.cfg` entry of a part: a lever (`name`, `axis`, `maxrot`) or a position (`pos`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Marker {
    pub section: String,
    pub name: Option<String>,
    pub axis: Option<String>,
    pub maxrot: Option<f64>,
    pub pos: Option<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub name: String,
    /// `chassis`, `steer`, `fsusp` or `rsusp` for a bike's parts.
    pub role: Option<String>,
    pub levels: Vec<Level>,
    pub shadow: Option<Level>,
    pub markers: Vec<Marker>,
}

/// One `.blend` to build: what the Blender script reads (`frost_import.py`).
#[derive(Debug, Clone, Serialize)]
pub struct Spec {
    pub name: String,
    pub out: String,
    pub tex_dir: String,
    pub place: bool,
    pub geom: Option<String>,
    pub parts: Vec<Part>,
    pub fbx: Option<String>,
    pub images: Vec<String>,
    pub shd: Vec<String>,
    /// Textures go inside the `.blend`, never beside it: set for a locked `.pkz`.
    pub pack: bool,
}

// ------------------------------------------------------------------------------------- trees

/// Where a model's files are read from: the disk, or a locked `.pkz` held in memory.
pub trait Tree {
    fn read(&self, p: &Path) -> Option<Vec<u8>>;
    fn head(&self, p: &Path, n: usize) -> Vec<u8>;
    fn is_file(&self, p: &Path) -> bool;
    fn is_dir(&self, p: &Path) -> bool;
    /// The files and folders directly in `dir`, sorted.
    fn list(&self, dir: &Path) -> Vec<PathBuf>;
}

pub struct Disk;

impl Tree for Disk {
    fn read(&self, p: &Path) -> Option<Vec<u8>> {
        std::fs::read(p).ok()
    }
    fn head(&self, p: &Path, n: usize) -> Vec<u8> {
        head(p, n)
    }
    fn is_file(&self, p: &Path) -> bool {
        p.is_file()
    }
    fn is_dir(&self, p: &Path) -> bool {
        p.is_dir()
    }
    fn list(&self, dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        v.sort();
        v
    }
}

/// A `.pkz`'s files in memory, under the `.pkz`'s own path as if it were a folder.
pub struct Mem {
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Mem {
    pub fn new(root: &Path, entries: Vec<(String, Vec<u8>)>) -> Mem {
        let files = entries.into_iter().filter_map(|(n, b)| Some((root.join(safe_rel(&n)?), b))).collect();
        Mem { files }
    }
}

impl Tree for Mem {
    fn read(&self, p: &Path) -> Option<Vec<u8>> {
        self.files.get(p).cloned()
    }
    fn head(&self, p: &Path, n: usize) -> Vec<u8> {
        self.files.get(p).map(|b| b[..b.len().min(n)].to_vec()).unwrap_or_default()
    }
    fn is_file(&self, p: &Path) -> bool {
        self.files.contains_key(p)
    }
    fn is_dir(&self, p: &Path) -> bool {
        self.files.keys().any(|k| k != p && k.starts_with(p))
    }
    fn list(&self, dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self
            .files
            .keys()
            .filter_map(|k| k.strip_prefix(dir).ok()?.components().next().map(|c| dir.join(c)))
            .collect();
        v.dedup();
        v
    }
}

// ------------------------------------------------------------------------------------- scan

pub fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn head(p: &Path, n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    match File::open(p).and_then(|mut f| f.read(&mut b)) {
        Ok(k) => {
            b.truncate(k);
            b
        }
        Err(_) => Vec::new(),
    }
}

pub fn is_plain_edf(p: &Path) -> bool {
    head(p, 4) == b"EDF\0"
}

fn is_plain_edf_in(t: &dyn Tree, p: &Path) -> bool {
    t.head(p, 4) == b"EDF\0"
}

fn is_secured_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".mxbsecure") || n.ends_with(".mxbsecurekey") || n.ends_with(".mxbkey")
}

/// The folders in a `.pkz` that hold an `.edf`; None when it can't be opened. A locked one that
/// lists nothing is a format the reader doesn't know.
fn pkz_models(path: &Path) -> Option<Vec<String>> {
    let names = mxb_core::pkz::entry_names(path).ok()?;
    if names.is_empty() && is_locked_pkz(path) {
        return None;
    }
    let mut dirs: Vec<String> = names
        .iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".edf"))
        .map(|n| match n.rfind('/') {
            Some(i) => n[..i].rsplit('/').next().unwrap_or("").to_string(),
            None => String::new(),
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    Some(dirs)
}

fn has_model_files(t: &dyn Tree, dir: &Path) -> bool {
    t.list(dir).iter().any(|p| {
        t.is_file(p) && (ext_of(p) == "edf" || p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("gfx.cfg")))
    })
}

fn found_file(p: &Path) -> Option<Found> {
    let ext = ext_of(p);
    let name = p.to_string_lossy().to_string();
    let bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let f = |kind: &str, locked: bool, models: Vec<String>| Found { path: name.clone(), kind: kind.into(), locked, models, bytes };
    if is_secured_name(&name) {
        return Some(f("pkz", true, vec![]));
    }
    match ext.as_str() {
        "pkz" | "zip" => match pkz_models(p) {
            Some(models) => Some(f("pkz", false, models)),
            None => Some(f("pkz", true, vec![])),
        },
        "edf" => Some(f("edf", !is_plain_edf(p), vec![])),
        "fbx" => Some(f("fbx", false, vec![])),
        _ => None,
    }
}

/// Dropped paths → the list. A folder with model files is one entry; any other folder is
/// searched for `.pkz`, `.edf`, `.fbx` and model folders.
pub fn scan(paths: &[String]) -> Vec<Found> {
    let mut out = Vec::new();
    for p in paths {
        walk(Path::new(p), 0, &mut out);
    }
    out
}

fn walk(p: &Path, depth: usize, out: &mut Vec<Found>) {
    if p.is_file() {
        if let Some(f) = found_file(p) {
            out.push(f);
        }
        return;
    }
    if !p.is_dir() || depth > 6 {
        return;
    }
    if has_model_files(&Disk, p) {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        out.push(Found { path: p.to_string_lossy().to_string(), kind: "folder".into(), locked: false, models: vec![name], bytes: 0 });
        return;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    entries.sort();
    for e in entries {
        walk(&e, depth + 1, out);
    }
}

// ------------------------------------------------------------------------------------- pkz

/// What a `.pkz` held that was left alone.
pub struct Extracted {
    pub locked: Vec<String>,
}

const KEEP: &[&str] = &["edf", "hrc", "cfg", "geom", "shd", "tga", "dds", "png", "jpg", "jpeg", "bmp", "ini", "fbx"];

fn safe_rel(name: &str) -> Option<PathBuf> {
    let n = name.replace('\\', "/");
    if n.starts_with('/') || n.contains('\0') {
        return None;
    }
    let p = PathBuf::from(&n);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(p)
}

/// Is this `.pkz` creator-locked (not a plain zip)? Such a one is only ever read into memory.
pub fn is_locked_pkz(pkz: &Path) -> bool {
    !mxb_core::pkz::is_plain_zip(pkz)
}

/// A creator-locked `.pkz`'s model files, decrypted in memory by the reader MXB App uses.
pub fn read_locked_pkz(pkz: &Path) -> anyhow::Result<Mem> {
    let entries = mxb_core::pkz::read_selected(pkz, |n| KEEP.contains(&ext_of(Path::new(n)).as_str()))?;
    Ok(Mem::new(pkz, entries))
}

/// Unzip the files a model needs into `dir`. Never opens anything locked: an encrypted entry,
/// a secured blob, an EDF without its magic or a sealed paint is only named in `locked`.
pub fn extract_pkz(pkz: &Path, dir: &Path) -> anyhow::Result<Extracted> {
    if !mxb_core::pkz::is_plain_zip(pkz) {
        anyhow::bail!("locked");
    }
    let mut zip = zip::ZipArchive::new(File::open(pkz)?)?;
    let mut locked = Vec::new();
    for i in 0..zip.len() {
        let listed = zip.name_for_index(i).unwrap_or("?").to_string();
        let mut e = match zip.by_index(i) {
            Ok(e) => e,
            Err(_) => {
                // An entry the reader won't open without a password.
                locked.push(listed);
                continue;
            }
        };
        if !e.is_file() {
            continue;
        }
        let name = e.name().to_string();
        if e.encrypted() || is_secured_name(&name) {
            locked.push(name);
            continue;
        }
        let ext = ext_of(Path::new(&name));
        if ext == "pnt" {
            let mut magic = [0u8; 4];
            if e.read_exact(&mut magic).is_err() || &magic != b"PNT\0" {
                locked.push(name);
            }
            continue;
        }
        if !KEEP.contains(&ext.as_str()) {
            continue;
        }
        let Some(rel) = safe_rel(&name) else { continue };
        let to = dir.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = File::create(&to)?;
        std::io::copy(&mut e, &mut f)?;
        f.flush()?;
        drop(f);
        if ext == "edf" && !is_plain_edf(&to) {
            locked.push(name);
        }
    }
    Ok(Extracted { locked })
}

// ------------------------------------------------------------------------------------- cfg

/// A `.cfg`/`.hrc` entry: `key = value`, or `key { ... }`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cfg {
    pub key: String,
    pub value: Option<String>,
    pub children: Vec<Cfg>,
}

impl Cfg {
    pub fn get(&self, key: &str) -> Option<&Cfg> {
        self.children.iter().find(|c| c.key.eq_ignore_ascii_case(key))
    }
    pub fn val(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|c| c.value.as_deref())
    }
}

pub fn parse_cfg(text: &str) -> Vec<Cfg> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    parse_block(&chars, &mut i)
}

fn parse_block(c: &[char], i: &mut usize) -> Vec<Cfg> {
    let mut out = Vec::new();
    loop {
        while *i < c.len() && c[*i].is_whitespace() {
            *i += 1;
        }
        if *i >= c.len() {
            return out;
        }
        match c[*i] {
            '}' => {
                *i += 1;
                return out;
            }
            '{' => {
                // A block with no key: read and drop it.
                *i += 1;
                parse_block(c, i);
                continue;
            }
            '#' | ';' => {
                while *i < c.len() && c[*i] != '\n' {
                    *i += 1;
                }
                continue;
            }
            '/' if c.get(*i + 1) == Some(&'/') => {
                while *i < c.len() && c[*i] != '\n' {
                    *i += 1;
                }
                continue;
            }
            _ => {}
        }
        let start = *i;
        while *i < c.len() && !c[*i].is_whitespace() && !matches!(c[*i], '=' | '{' | '}') {
            *i += 1;
        }
        let key: String = c[start..*i].iter().collect();
        while *i < c.len() && (c[*i] == ' ' || c[*i] == '\t') {
            *i += 1;
        }
        if *i < c.len() && c[*i] == '=' {
            *i += 1;
            let vs = *i;
            while *i < c.len() && c[*i] != '\n' && c[*i] != '}' {
                *i += 1;
            }
            // `a = x  b = y` on one line: the value stops before the next `key =`.
            if let Some(k) = next_key_at(&c[vs..*i]) {
                *i = vs + k;
            }
            let v: String = c[vs..*i].iter().collect();
            out.push(Cfg { key, value: Some(v.trim().to_string()), children: vec![] });
            continue;
        }
        let mut j = *i;
        while j < c.len() && c[j].is_whitespace() {
            j += 1;
        }
        if j < c.len() && c[j] == '{' {
            *i = j + 1;
            let children = parse_block(c, i);
            out.push(Cfg { key, value: None, children });
        } else if !key.is_empty() {
            out.push(Cfg { key, value: None, children: vec![] });
        } else {
            *i += 1;
        }
    }
}

/// Where a second `key =` starts in a value (after whitespace), if one does.
fn next_key_at(v: &[char]) -> Option<usize> {
    let mut k = 0;
    while k < v.len() {
        if v[k].is_whitespace() {
            let ws = k;
            while k < v.len() && v[k].is_whitespace() {
                k += 1;
            }
            let ks = k;
            while k < v.len() && (v[k].is_alphanumeric() || v[k] == '_') {
                k += 1;
            }
            if k > ks {
                let mut j = k;
                while j < v.len() && (v[j] == ' ' || v[j] == '\t') {
                    j += 1;
                }
                if j < v.len() && v[j] == '=' && v[..ws].iter().any(|c| !c.is_whitespace()) {
                    return Some(ws);
                }
            }
        } else {
            k += 1;
        }
    }
    None
}

fn read_text(t: &dyn Tree, p: &Path) -> Option<String> {
    t.read(p).map(|b| b.iter().map(|&x| x as char).collect())
}

/// Find a file in `dir` by name, in any case.
fn find_in(t: &dyn Tree, dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if t.is_file(&direct) {
        return Some(direct);
    }
    t.list(dir).into_iter().find(|p| {
        t.is_file(p) && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
    })
}

/// `.hrc` levels in order: level0, level1, ...
fn hrc_levels(t: &dyn Tree, dir: &Path, hrc: &Path) -> Vec<Level> {
    let Some(text) = read_text(t, hrc) else { return vec![] };
    let mut levels: Vec<(u32, Level)> = parse_cfg(&text)
        .into_iter()
        .filter_map(|c| {
            let n: u32 = c.key.to_ascii_lowercase().strip_prefix("level")?.parse().ok()?;
            let scene = c.val("scene")?;
            let edf = find_in(t, dir, scene)?;
            Some((n, Level { edf: edf.to_string_lossy().to_string(), object: Some(c.val("name").unwrap_or("").to_string()) }))
        })
        .collect();
    levels.sort_by_key(|(n, _)| *n);
    levels.into_iter().map(|(_, l)| l).collect()
}

/// A `model`/`shadow` entry: `model = x.hrc` or `model { file = x.edf  name = y }`.
fn model_levels(t: &dyn Tree, dir: &Path, entry: &Cfg) -> Vec<Level> {
    let (file, name) = match &entry.value {
        Some(v) => (v.as_str(), None),
        None => match entry.val("file") {
            Some(f) => (f, entry.val("name")),
            None => return vec![],
        },
    };
    if file.to_ascii_lowercase().ends_with(".hrc") {
        return find_in(t, dir, file).map(|h| hrc_levels(t, dir, &h)).unwrap_or_default();
    }
    match find_in(t, dir, file) {
        Some(p) => vec![Level { edf: p.to_string_lossy().to_string(), object: Some(name.unwrap_or("").to_string()) }],
        None => vec![],
    }
}

const NOT_MARKERS: &[&str] = &["model", "shadow", "temporary", "ambientshadow"];

fn markers(section: &Cfg) -> Vec<Marker> {
    section
        .children
        .iter()
        .filter(|c| c.value.is_none() && !NOT_MARKERS.iter().any(|k| c.key.eq_ignore_ascii_case(k)))
        .filter_map(|c| {
            let pos = c.get("pos").and_then(|p| {
                let n = |k: &str| p.val(k).and_then(|v| v.parse::<f64>().ok());
                Some([n("x")?, n("y")?, n("z")?])
            });
            let m = Marker {
                section: c.key.clone(),
                name: c.val("name").map(str::to_string),
                axis: c.val("axis").map(str::to_string),
                maxrot: c.val("maxrot").and_then(|v| v.parse().ok()),
                pos,
            };
            (m.name.is_some() || m.pos.is_some()).then_some(m)
        })
        .collect()
}

fn bike_role(section: &str) -> Option<&'static str> {
    match section.to_ascii_lowercase().as_str() {
        "chassis" => Some("chassis"),
        "steer" => Some("steer"),
        "front_susp" | "fsusp" => Some("fsusp"),
        "rear_susp" | "rsusp" => Some("rsusp"),
        _ => None,
    }
}

fn is_shadow_name(stem: &str) -> bool {
    let s = stem.to_ascii_lowercase();
    s.ends_with("_s") || s.ends_with("_as") || s.contains("shadow")
}

/// The model in one folder: its parts from `gfx.cfg`, else its `.hrc` files, else its EDFs.
pub struct Model {
    pub name: String,
    pub geom: Option<PathBuf>,
    pub parts: Vec<Part>,
    pub images: Vec<String>,
    pub shd: Vec<String>,
    pub notes: Vec<String>,
}

fn files_with(t: &dyn Tree, dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    t.list(dir).into_iter().filter(|p| t.is_file(p) && exts.contains(&ext_of(p).as_str())).collect()
}

fn images_under(t: &dyn Tree, dir: &Path, depth: usize, out: &mut Vec<String>, shd: &mut Vec<String>) {
    for p in files_with(t, dir, &["tga", "dds", "png", "jpg", "jpeg", "bmp"]) {
        out.push(p.to_string_lossy().to_string());
    }
    for p in files_with(t, dir, &["shd"]) {
        shd.push(p.to_string_lossy().to_string());
    }
    if depth == 0 {
        return;
    }
    for p in t.list(dir) {
        // A subfolder with its own EDFs is another model, not this one's textures.
        if t.is_dir(&p) && files_with(t, &p, &["edf"]).is_empty() {
            images_under(t, &p, depth - 1, out, shd);
        }
    }
}

pub fn plan_folder(dir: &Path, opts: Options) -> Model {
    plan_folder_in(&Disk, dir, opts)
}

pub fn plan_folder_in(t: &dyn Tree, dir: &Path, opts: Options) -> Model {
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());
    let mut notes = Vec::new();
    let usable = |l: &Level| is_plain_edf_in(t, Path::new(&l.edf));
    let mut parts = Vec::new();
    let mut bike = false;
    if let Some(text) = find_in(t, dir, "gfx.cfg").and_then(|p| read_text(t, &p)) {
        let cfg = parse_cfg(&text);
        bike = cfg.iter().any(|c| c.key.eq_ignore_ascii_case("chassis"));
        for sec in cfg.iter().filter(|c| !c.key.eq_ignore_ascii_case("cockpit")) {
            let Some(model) = sec.get("model") else { continue };
            let levels = model_levels(t, dir, model);
            if levels.is_empty() {
                continue;
            }
            let role = if bike { bike_role(&sec.key).map(str::to_string) } else { None };
            let shadow = sec.get("shadow").and_then(|s| model_levels(t, dir, s).into_iter().next());
            let pname = role.clone().unwrap_or_else(|| sec.key.clone());
            parts.push(Part { name: pname, role, levels, shadow, markers: markers(sec) });
        }
    }
    if parts.is_empty() {
        for h in files_with(t, dir, &["hrc"]) {
            let levels = hrc_levels(t, dir, &h);
            if levels.is_empty() {
                continue;
            }
            let stem = h.file_stem().unwrap().to_string_lossy().to_string();
            let role = bike_role(&stem).map(str::to_string);
            bike |= role.is_some();
            parts.push(Part { name: stem, role, levels, shadow: None, markers: vec![] });
        }
    }
    if parts.is_empty() {
        let edfs = files_with(t, dir, &["edf"]);
        for e in &edfs {
            let stem = e.file_stem().unwrap().to_string_lossy().to_string();
            if is_shadow_name(&stem) {
                continue;
            }
            let shadow = edfs
                .iter()
                .find(|s| s.file_stem().is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(&format!("{stem}_s"))))
                .map(|s| Level { edf: s.to_string_lossy().to_string(), object: None });
            parts.push(Part {
                name: stem,
                role: None,
                levels: vec![Level { edf: e.to_string_lossy().to_string(), object: None }],
                shadow,
                markers: vec![],
            });
        }
    }
    // Locked or missing EDFs, and what the options leave out.
    let mut skipped_locked = false;
    for p in &mut parts {
        let before = p.levels.len();
        p.levels.retain(|l| usable(l));
        if p.levels.len() < before {
            skipped_locked = true;
        }
        if !opts.lods {
            p.levels.truncate(1);
        }
        if !opts.shadow || p.shadow.as_ref().is_some_and(|s| !usable(s)) {
            p.shadow = None;
        }
    }
    parts.retain(|p| !p.levels.is_empty());
    if skipped_locked {
        notes.push("locked".into());
    }
    let geom = if bike { files_with(t, dir, &["geom"]).into_iter().next() } else { None };
    let mut images = Vec::new();
    let mut shd = Vec::new();
    images_under(t, dir, 2, &mut images, &mut shd);
    Model { name, geom, parts, images, shd, notes }
}

/// A dropped `.edf`: the folder's plan, kept to the parts that use this file. An EDF no part
/// names (a shadow, a cockpit model, one dropped on its own) is imported whole.
pub fn plan_edf(edf: &Path, opts: Options) -> Model {
    let dir = edf.parent().unwrap_or(Path::new("."));
    let mut m = plan_folder(dir, opts);
    let me = edf.to_string_lossy().to_lowercase();
    m.parts.retain(|p| p.levels.iter().any(|l| l.edf.to_lowercase() == me));
    if m.parts.is_empty() {
        m.parts.push(Part {
            name: edf.file_stem().unwrap().to_string_lossy().to_string(),
            role: None,
            levels: vec![Level { edf: edf.to_string_lossy().to_string(), object: None }],
            shadow: None,
            markers: vec![],
        });
        m.geom = None;
    }
    m.name = edf.file_stem().unwrap().to_string_lossy().to_string();
    m
}

/// Every folder under `root` that holds a model (an `.edf` or a `gfx.cfg`).
pub fn model_dirs(t: &dyn Tree, root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn go(t: &dyn Tree, d: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        if has_model_files(t, d) {
            out.push(d.to_path_buf());
        }
        if depth == 0 {
            return;
        }
        for s in t.list(d).into_iter().filter(|p| t.is_dir(p)) {
            go(t, &s, out, depth - 1);
        }
    }
    go(t, root, &mut out, 6);
    out
}

/// `X.blend` in `dir`, or `X (2).blend` when that is taken: nothing is ever overwritten.
pub fn free_blend(dir: &Path, stem: &str) -> PathBuf {
    let stem: String = stem.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let first = dir.join(format!("{stem}.blend"));
    if !first.exists() && !dir.join(format!("{stem}_textures")).exists() {
        return first;
    }
    (2..)
        .map(|i| (dir.join(format!("{stem} ({i}).blend")), dir.join(format!("{stem} ({i})_textures"))))
        .find(|(b, t)| !b.exists() && !t.exists())
        .map(|(b, _)| b)
        .unwrap()
}

pub fn spec_for(model: &Model, out: &Path, opts: Options) -> Spec {
    let tex_dir = out.with_file_name(format!("{}_textures", out.file_stem().unwrap().to_string_lossy()));
    Spec {
        name: model.name.clone(),
        out: out.to_string_lossy().to_string(),
        tex_dir: tex_dir.to_string_lossy().to_string(),
        place: opts.place,
        geom: model.geom.as_ref().map(|g| g.to_string_lossy().to_string()),
        parts: model.parts.clone(),
        fbx: None,
        images: model.images.clone(),
        shd: model.shd.clone(),
        pack: false,
    }
}

pub fn spec_for_fbx(fbx: &Path, out: &Path) -> Spec {
    let tex_dir = out.with_file_name(format!("{}_textures", out.file_stem().unwrap().to_string_lossy()));
    Spec {
        name: fbx.file_stem().unwrap().to_string_lossy().to_string(),
        out: out.to_string_lossy().to_string(),
        tex_dir: tex_dir.to_string_lossy().to_string(),
        place: false,
        geom: None,
        parts: vec![],
        fbx: Some(fbx.to_string_lossy().to_string()),
        images: vec![],
        shd: vec![],
        pack: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GFX: &str = "chassis\n{\n\tmodel\n\t{\n\t\tfile = chassis.hrc\n\t}\n\tshadow\n\t{\n\t\tfile = b_s.edf\n\t}\n\
        \trearbrakepedal\n\t{\n\t\tname = rearbrake_lever\n\t\taxis = y\n\t\tmaxrot = 10\n\t}\n\
        \texhaust\n\t{\n\t\tpos\n\t\t{\n\t\t\tx = 0.13\n\t\t\ty = 0.83\n\t\t\tz = -0.7\n\t\t}\n\t}\n}\n\
        steer\n{\n\tmodel\n\t{\n\t\tfile = b.edf\n\t\tname = steer\n\t}\n}\n\
        front_susp\n{\n\tmodel\n\t{\n\t\tfile = fsusp.hrc\n\t}\n}\n\
        cockpit\n{\n\tchassis\n\t{\n\t\tmodel\n\t\t{\n\t\t\tfile = c.edf\n\t\t}\n\t}\n}\ntyres = oem_mx\n";
    const HRC: &str = "\r\nlevel0\r\n{\r\n\tscene = b.edf\r\n\tswitch = 0\r\n}\r\nlevel1\r\n{\r\n\tscene = b.edf\r\n\tname = chassisb\r\n\tswitch = 5\r\n}\r\n";
    const FHRC: &str = "level0 { scene = b.edf  name = fsuspa }\nlevel1 { scene = b.edf name = fsuspb }\n";

    fn bike_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("gfx.cfg"), GFX).unwrap();
        std::fs::write(d.path().join("chassis.hrc"), HRC).unwrap();
        std::fs::write(d.path().join("fsusp.hrc"), FHRC).unwrap();
        std::fs::write(d.path().join("b.edf"), b"EDF\0rest").unwrap();
        std::fs::write(d.path().join("b_s.edf"), b"EDF\0rest").unwrap();
        std::fs::write(d.path().join("c.edf"), b"EDF\0rest").unwrap();
        std::fs::write(d.path().join("b.geom"), "chassis_steer = 0, 1, 0.3\n").unwrap();
        std::fs::write(d.path().join("w_plate.tga"), [0u8; 18]).unwrap();
        d
    }

    #[test]
    fn cfg_reads_blocks_values_and_one_line_blocks() {
        let c = parse_cfg(FHRC);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].val("scene"), Some("b.edf"));
        assert_eq!(c[0].val("name"), Some("fsuspa"));
        let c = parse_cfg(GFX);
        let ch = c.iter().find(|x| x.key == "chassis").unwrap();
        assert_eq!(ch.get("model").unwrap().val("file"), Some("chassis.hrc"));
        assert_eq!(c.iter().find(|x| x.key == "tyres").unwrap().value.as_deref(), Some("oem_mx"));
    }

    #[test]
    fn a_bike_folder_plans_its_parts_levels_and_markers() {
        let d = bike_dir();
        let all = Options { lods: true, shadow: true, place: true };
        let m = plan_folder(d.path(), all);
        let names: Vec<_> = m.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["chassis", "steer", "fsusp"]);
        let ch = &m.parts[0];
        assert_eq!(ch.levels.iter().map(|l| l.object.clone().unwrap()).collect::<Vec<_>>(), ["", "chassisb"]);
        assert!(ch.shadow.is_some());
        assert_eq!(ch.markers.len(), 2);
        assert_eq!(ch.markers[0].maxrot, Some(10.0));
        assert_eq!(ch.markers[1].pos, Some([0.13, 0.83, -0.7]));
        assert_eq!(m.parts[1].levels[0].object.as_deref(), Some("steer"));
        assert!(m.geom.is_some());
        assert_eq!(m.images.len(), 1);
        // The defaults: level 0 only, no shadow.
        let m = plan_folder(d.path(), Options::default());
        assert!(m.parts.iter().all(|p| p.levels.len() == 1 && p.shadow.is_none()));
    }

    #[test]
    fn a_locked_edf_is_left_out() {
        let d = bike_dir();
        std::fs::write(d.path().join("b.edf"), b"\x8a\x11scrambled").unwrap();
        let m = plan_folder(d.path(), Options::default());
        assert!(m.parts.is_empty());
        assert_eq!(m.notes, ["locked"]);
    }

    #[test]
    fn a_dropped_shadow_edf_imports_whole() {
        let d = bike_dir();
        let m = plan_edf(&d.path().join("b_s.edf"), Options::default());
        assert_eq!(m.parts.len(), 1);
        assert_eq!(m.parts[0].levels[0].object, None);
        let m = plan_edf(&d.path().join("b.edf"), Options::default());
        assert_eq!(m.parts.len(), 3);
    }

    #[test]
    fn pkz_extraction_skips_locked_entries() {
        let d = tempfile::tempdir().unwrap();
        let pkz = d.path().join("m.pkz");
        {
            let mut w = zip::ZipWriter::new(File::create(&pkz).unwrap());
            let o: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            for (n, b) in [
                ("bike/b.edf", &b"EDF\0x"[..]),
                ("bike/sealed.edf", &b"\x01\x02\x03\x04"[..]),
                ("bike/paints/a.pnt", &b"\x99sealed"[..]),
                ("bike/paints/b.pnt", &b"PNT\0plain"[..]),
                ("bike/x.mxbsecure", &b"blob"[..]),
                ("bike/engine.wav", &b"RIFF"[..]),
            ] {
                w.start_file(n, o).unwrap();
                w.write_all(b).unwrap();
            }
            w.finish().unwrap();
        }
        let out = d.path().join("x");
        let e = extract_pkz(&pkz, &out).unwrap();
        assert_eq!(e.locked, ["bike/sealed.edf", "bike/paints/a.pnt", "bike/x.mxbsecure"]);
        assert!(out.join("bike/b.edf").is_file());
        assert!(!out.join("bike/engine.wav").exists());
        // A pkz that isn't a plain zip is not opened at all.
        let opaque = d.path().join("o.pkz");
        std::fs::write(&opaque, b"\x14\xbb\xd8\x54opaque").unwrap();
        assert!(extract_pkz(&opaque, &out).is_err());
        let f = scan(&[opaque.to_string_lossy().to_string()]);
        assert!(f[0].locked);
    }

    #[test]
    fn a_pkz_held_in_memory_plans_like_its_folder() {
        let root = Path::new(r"C:\mods\b.pkz");
        let e = |n: &str, b: &[u8]| (n.to_string(), b.to_vec());
        let mem = Mem::new(
            root,
            vec![
                e("bike/gfx.cfg", GFX.as_bytes()),
                e("bike/chassis.hrc", HRC.as_bytes()),
                e("bike/fsusp.hrc", FHRC.as_bytes()),
                e("bike/b.edf", b"EDF\0rest"),
                e("bike/b_s.edf", b"EDF\0rest"),
                e("bike/c.edf", b"EDF\0rest"),
                e("bike/b.geom", b"chassis_steer = 0, 1, 0.3\n"),
                e("bike/w_plate.tga", &[0u8; 18]),
                e("../escape.edf", b"EDF\0"),
            ],
        );
        let dirs = model_dirs(&mem, root);
        assert_eq!(dirs, [root.join("bike")]);
        let m = plan_folder_in(&mem, &dirs[0], Options { lods: true, shadow: true, place: true });
        assert_eq!(m.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["chassis", "steer", "fsusp"]);
        assert_eq!(m.parts[0].levels[0].edf, root.join("bike").join("b.edf").to_string_lossy());
        assert!(m.geom.is_some());
        assert_eq!(m.images.len(), 1);
        assert_eq!(mem.files.len(), 8);
    }

    #[test]
    fn blends_are_never_overwritten() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(free_blend(d.path(), "a").file_name().unwrap(), "a.blend");
        std::fs::write(d.path().join("a.blend"), b"").unwrap();
        assert_eq!(free_blend(d.path(), "a").file_name().unwrap(), "a (2).blend");
    }
}
