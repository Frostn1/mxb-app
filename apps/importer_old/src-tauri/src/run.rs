//! Running an import: unzip (a locked `.pkz`: read into memory), plan, then one headless Blender
//! per `.blend`.
//!
//! Blender runs are heavy jobs: one at a time (a lock across the app), and only once at least
//! 8 GB of memory is available. Only Blender processes this app started are ever stopped, by
//! their own handle. No file is overwritten: a taken name gets ` (2)`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::plan::{self, Options, Spec};

const SCRIPT: &str = include_str!("../py/frost_import.py");
const MIN_FREE: u64 = 8 << 30;

static HEAVY: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Built {
    pub name: String,
    pub blend: String,
    pub objects: u64,
    pub meshes: u64,
    pub empties: u64,
    pub materials: u64,
    pub images: u64,
    pub triangles: u64,
    /// Part → nodes per level (`"0"`, `"1"`, `"shadow"`).
    pub parts: serde_json::Value,
    pub seconds: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub built: Vec<Built>,
    /// Locked files or entries that were left alone.
    pub locked: Vec<String>,
    pub notes: Vec<String>,
    pub error: Option<String>,
}

/// Progress of one dropped item: `stage` is a short tag the window words.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub index: usize,
    pub done: u32,
    pub total: u32,
    pub stage: String,
}

// ------------------------------------------------------------------------------------- Blender

fn version_key(p: &Path) -> Vec<u32> {
    let s = p.to_string_lossy();
    s.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect()
}

/// The newest Blender on this machine: `C:\Tools\blender-*`, Program Files, Steam.
pub fn find_blender() -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut look = |dir: &Path, pattern: &str| {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with(pattern) {
                    let exe = e.path().join("blender.exe");
                    if exe.is_file() {
                        found.push(exe);
                    }
                }
            }
        }
    };
    look(Path::new(r"C:\Tools"), "blender");
    for pf in [std::env::var("ProgramFiles").ok(), std::env::var("ProgramW6432").ok()].into_iter().flatten() {
        look(&Path::new(&pf).join("Blender Foundation"), "blender");
    }
    found.sort_by_key(|p| version_key(p));
    if let Some(p) = found.pop() {
        return Some(p);
    }
    let steam = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Blender\blender.exe");
    steam.is_file().then_some(steam)
}

#[cfg(windows)]
pub fn available_memory() -> u64 {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page: u64,
        avail_page: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buf: *mut MemoryStatusEx) -> i32;
    }
    let mut m = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page: 0,
        avail_page: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended: 0,
    };
    // SAFETY: a plain Win32 call filling a struct of the documented layout.
    if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 {
        m.avail_phys
    } else {
        u64::MAX
    }
}

#[cfg(not(windows))]
pub fn available_memory() -> u64 {
    u64::MAX
}

fn script_path() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("frost-importer");
    std::fs::create_dir_all(&dir)?;
    let p = dir.join(format!("frost_import_{}.py", env!("CARGO_PKG_VERSION")));
    if std::fs::read_to_string(&p).ok().as_deref() != Some(SCRIPT) {
        std::fs::write(&p, SCRIPT)?;
    }
    Ok(p)
}

/// The plan and the files it names, as the script reads them from its stdin: little-endian
/// `u32` length + plan JSON, `u32` count, then per file `u32` length + path, `u64` length + bytes.
fn write_piped(w: &mut dyn Write, spec: &[u8], files: &[(String, Vec<u8>)]) -> std::io::Result<()> {
    w.write_all(&(spec.len() as u32).to_le_bytes())?;
    w.write_all(spec)?;
    w.write_all(&(files.len() as u32).to_le_bytes())?;
    for (p, b) in files {
        w.write_all(&(p.len() as u32).to_le_bytes())?;
        w.write_all(p.as_bytes())?;
        w.write_all(&(b.len() as u64).to_le_bytes())?;
        w.write_all(b)?;
    }
    w.flush()
}

/// Every file a plan reads.
fn spec_files(spec: &Spec) -> Vec<&str> {
    let mut v: Vec<&str> = spec.images.iter().chain(&spec.shd).chain(&spec.geom).map(String::as_str).collect();
    for p in &spec.parts {
        v.extend(p.levels.iter().chain(&p.shadow).map(|l| l.edf.as_str()));
    }
    v.sort();
    v.dedup();
    v
}

/// Build one `.blend`. `tick(done, total, stage)` follows the script's progress. With `mem`, the
/// plan's files come from memory over Blender's stdin: nothing of them is written to disk.
fn blender_job(
    blender: &Path,
    spec: &Spec,
    mem: Option<&plan::Mem>,
    work: &Path,
    tick: &mut dyn FnMut(u32, u32, &str),
) -> Result<Built, String> {
    let _heavy = HEAVY.lock().unwrap_or_else(|e| e.into_inner());
    let start = Instant::now();
    while available_memory() < MIN_FREE {
        tick(0, 0, "memory");
        if start.elapsed() > Duration::from_secs(30 * 60) {
            return Err("Not enough free memory".into());
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    let spec_json = serde_json::to_vec_pretty(spec).map_err(|e| e.to_string())?;
    let report_path = work.join("report.json");
    let script = script_path().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(blender);
    cmd.args(["-b", "--factory-startup", "--python-exit-code", "1", "--python"]).arg(&script).arg("--");
    if mem.is_some() {
        cmd.arg("--stdin").stdin(Stdio::piped());
    } else {
        let spec_path = work.join("plan.json");
        std::fs::write(&spec_path, &spec_json).map_err(|e| e.to_string())?;
        cmd.arg("--spec").arg(&spec_path).stdin(Stdio::null());
    }
    cmd.arg("--report").arg(&report_path).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Blender didn't start: {e}"))?;
    let feed = match (mem, child.stdin.take()) {
        (Some(mem), Some(mut stdin)) => {
            let files: Vec<(String, Vec<u8>)> = spec_files(spec)
                .into_iter()
                .filter_map(|p| mem.files.get(Path::new(p)).map(|b| (p.to_string(), b.clone())))
                .collect();
            // Its own thread: Blender's output is read below while it takes the files in.
            Some(std::thread::spawn(move || write_piped(&mut stdin, &spec_json, &files)))
        }
        _ => None,
    };
    let stderr = child.stderr.take();
    let err_tail = std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        if let Some(s) = stderr {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                tail.push(line);
                if tail.len() > 20 {
                    tail.remove(0);
                }
            }
        }
        tail
    });
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(rest) = line.strip_prefix("FI_PROGRESS ") {
                let mut it = rest.splitn(3, ' ');
                let done = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                let total = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                tick(done, total, it.next().unwrap_or(""));
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let tail = err_tail.join().unwrap_or_default();
    if let Some(Ok(Err(e))) = feed.map(|f| f.join()) {
        if !report_path.is_file() {
            return Err(format!("Blender didn't take the files: {e}"));
        }
    }
    let report: serde_json::Value = std::fs::read(&report_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| {
            let last = tail.iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_default();
            format!("Blender stopped ({status}). {last}")
        })?;
    if report["ok"] != serde_json::Value::Bool(true) {
        return Err(report["error"].as_str().unwrap_or("Blender couldn't build the file").to_string());
    }
    let n = |k: &str| report[k].as_u64().unwrap_or(0);
    Ok(Built {
        name: spec.name.clone(),
        blend: report["blend"].as_str().unwrap_or(&spec.out).to_string(),
        objects: n("objects"),
        meshes: n("meshes"),
        empties: n("empties"),
        materials: n("materials"),
        images: n("images"),
        triangles: n("triangles"),
        parts: report["parts"].clone(),
        seconds: report["seconds"].as_f64().unwrap_or(0.0),
    })
}

// ------------------------------------------------------------------------------------- items

fn work_dir() -> std::io::Result<PathBuf> {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join("frost-importer").join(format!("job-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// Where the `.blend` goes: the chosen folder, else beside the source.
fn out_dir(src: &Path, folder: Option<&Path>) -> PathBuf {
    match folder {
        Some(f) => f.to_path_buf(),
        None => src.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
    }
}

/// Import one dropped item. `tick` gets (done, total, stage) for the whole item.
pub fn import_item(
    path: &str,
    opts: Options,
    folder: Option<&Path>,
    blender: Option<&Path>,
    tick: &mut dyn FnMut(u32, u32, &str),
) -> Outcome {
    let mut o = Outcome::default();
    let src = PathBuf::from(path);
    let Some(found) = plan::scan(&[path.to_string()]).into_iter().next() else {
        o.error = Some("Not a .pkz, .edf or .fbx".into());
        return o;
    };
    if found.locked {
        o.locked.push(path.to_string());
        return o;
    }
    let blender = match blender.map(Path::to_path_buf).or_else(find_blender) {
        Some(b) if b.is_file() => b,
        _ => {
            o.error = Some("Blender not found".into());
            return o;
        }
    };
    let dest = out_dir(&src, folder);
    if let Err(e) = std::fs::create_dir_all(&dest) {
        o.error = Some(format!("{}: {e}", dest.display()));
        return o;
    }
    let work = match work_dir() {
        Ok(w) => w,
        Err(e) => {
            o.error = Some(e.to_string());
            return o;
        }
    };
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());

    // Every .blend this item makes, with the folder it was planned from.
    let mut specs: Vec<Spec> = Vec::new();
    // A locked .pkz's files, held only here.
    let mut mem: Option<plan::Mem> = None;
    match found.kind.as_str() {
        "fbx" => specs.push(plan::spec_for_fbx(&src, &plan::free_blend(&dest, &stem))),
        "edf" => {
            let m = plan::plan_edf(&src, opts);
            o.notes.extend(m.notes.iter().cloned());
            if !m.parts.is_empty() {
                specs.push(plan::spec_for(&m, &plan::free_blend(&dest, &m.name), opts));
            }
        }
        "folder" => {
            for d in plan::model_dirs(&plan::Disk, &src) {
                let m = plan::plan_folder(&d, opts);
                o.notes.extend(m.notes.iter().cloned());
                if !m.parts.is_empty() {
                    let name = if d == src { stem.clone() } else { format!("{stem} - {}", m.name) };
                    specs.push(plan::spec_for(&m, &plan::free_blend(&dest, &name), opts));
                }
            }
        }
        _ if plan::is_locked_pkz(&src) => {
            // Read the way MXB App reads it, and kept in memory: Blender gets the files over a
            // pipe and packs the textures into the .blend.
            tick(0, 0, "unzip");
            match plan::read_locked_pkz(&src) {
                Ok(m) => mem = Some(m),
                Err(e) => {
                    o.error = Some(e.to_string());
                    cleanup(&work);
                    return o;
                }
            }
            let t = mem.as_ref().unwrap();
            let dirs = plan::model_dirs(t, &src);
            let single = dirs.len() == 1;
            for d in dirs {
                let mut m = plan::plan_folder_in(t, &d, opts);
                o.notes.extend(m.notes.iter().cloned());
                if m.parts.is_empty() {
                    continue;
                }
                let name = if single { stem.clone() } else { format!("{stem} - {}", m.name) };
                m.name = name.clone();
                let mut spec = plan::spec_for(&m, &plan::free_blend(&dest, &name), opts);
                spec.pack = true;
                specs.push(spec);
            }
        }
        _ => {
            tick(0, 0, "unzip");
            let x = work.join("pkz");
            match plan::extract_pkz(&src, &x) {
                Ok(e) => o.locked.extend(e.locked),
                Err(e) => {
                    o.error = Some(e.to_string());
                    return o;
                }
            }
            let dirs = plan::model_dirs(&plan::Disk, &x);
            let single = dirs.len() == 1;
            for d in dirs {
                let mut m = plan::plan_folder(&d, opts);
                o.notes.extend(m.notes.iter().cloned());
                if m.parts.is_empty() {
                    continue;
                }
                let name = if single { stem.clone() } else { format!("{stem} - {}", m.name) };
                m.name = name.clone();
                specs.push(plan::spec_for(&m, &plan::free_blend(&dest, &name), opts));
            }
            // An FBX shipped in the archive.
            if specs.is_empty() {
                for f in walk_files(&x, "fbx") {
                    let n = f.file_stem().unwrap().to_string_lossy().to_string();
                    specs.push(plan::spec_for_fbx(&f, &plan::free_blend(&dest, &format!("{stem} - {n}"))));
                }
            }
        }
    }
    o.notes.sort();
    o.notes.dedup();
    if o.notes.iter().any(|n| n == "locked") {
        o.notes.retain(|n| n != "locked");
        if o.locked.is_empty() {
            o.locked.push(path.to_string());
        }
    }
    if specs.is_empty() {
        if o.locked.is_empty() {
            o.error = Some("No model found".into());
        }
        cleanup(&work);
        return o;
    }
    let total = specs.len() as u32;
    for (k, spec) in specs.iter().enumerate() {
        let jw = work.join(format!("blend-{k}"));
        let _ = std::fs::create_dir_all(&jw);
        let mut inner = |d: u32, t: u32, stage: &str| {
            // Each .blend is an equal share of the item's bar.
            let (d, t) = if t == 0 { (0, 0) } else { (k as u32 * t + d, total * t) };
            tick(d, t, stage);
        };
        match blender_job(&blender, spec, mem.as_ref(), &jw, &mut inner) {
            Ok(b) => o.built.push(b),
            Err(e) => {
                o.error = Some(e);
                break;
            }
        }
    }
    cleanup(&work);
    o
}

fn walk_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_files(&p, ext));
            } else if plan::ext_of(&p) == ext {
                out.push(p);
            }
        }
    }
    out
}

/// This app's own scratch folder for the job, under %TEMP%\frost-importer: the unzipped copy
/// and the plan. Nothing outside it is touched.
fn cleanup(work: &Path) {
    let root = std::env::temp_dir().join("frost-importer");
    if work.starts_with(&root) && work != root {
        let _ = std::fs::remove_dir_all(work);
    }
}

/// A `.blend` of a lined-up swap on its new bike: the swap's files from `dir`, placed by the new
/// bike's `.geom` (`geom`, read in memory). The swap's locked `.hrc`s are opened in memory too,
/// and every texture is packed into the `.blend`. Nothing but the `.blend` is written.
pub fn preview_lineup(
    dir: &Path,
    geom: (String, Vec<u8>),
    out: &Path,
    blender: Option<&Path>,
    tick: &mut dyn FnMut(u32, u32, &str),
) -> Outcome {
    let mut o = Outcome::default();
    let blender = match blender.map(Path::to_path_buf).or_else(find_blender) {
        Some(b) if b.is_file() => b,
        _ => {
            o.error = Some("Blender not found".into());
            return o;
        }
    };
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let n = e.file_name().to_string_lossy().to_string();
            if !p.is_file() || !matches!(plan::ext_of(&p).as_str(), "edf" | "hrc" | "cfg" | "tga" | "dds" | "png" | "shd") {
                continue;
            }
            let Ok(b) = std::fs::read(&p) else { continue };
            let b = mxb_core::pkz::read_sidecar_blob(&b).unwrap_or(b);
            entries.push((n, b));
        }
    }
    entries.push(geom);
    let mem = plan::Mem::new(dir, entries);
    let opts = Options { lods: false, shadow: false, place: true };
    let mut m = plan::plan_folder_in(&mem, dir, opts);
    o.notes.extend(m.notes.iter().cloned());
    if m.parts.is_empty() || m.geom.is_none() {
        o.error = Some("No bike model found".into());
        return o;
    }
    if let Err(e) = std::fs::create_dir_all(out) {
        o.error = Some(format!("{}: {e}", out.display()));
        return o;
    }
    let work = match work_dir() {
        Ok(w) => w,
        Err(e) => {
            o.error = Some(e.to_string());
            return o;
        }
    };
    let stem = format!("{} on {}", m.name, dir.parent().and_then(Path::parent).and_then(Path::file_name).map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    m.name = stem.clone();
    let mut spec = plan::spec_for(&m, &plan::free_blend(out, &stem), opts);
    spec.pack = true;
    match blender_job(&blender, &spec, Some(&mem), &work, tick) {
        Ok(b) => o.built.push(b),
        Err(e) => o.error = Some(e),
    }
    cleanup(&work);
    o
}
