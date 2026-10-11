//! `frost-converter in.fbx -o out\`: the window's batch from a terminal or a script.
//!
//! The defaults are the window's, and so are the files: the same `job::run` writes them.

use std::path::{Path, PathBuf};

use fbx2edf::host::{self, Options};
use fbx2edf::job::{self, HrcMode, Model, Outcome, Progress, Shd};
use serde_json::Value;

const USAGE: &str = "\
Frost Converter: FBX to EDF for MX Bikes.

Usage: frost-converter <FBX files or folders> [options]

  -o, --out DIR        Write every .edf into DIR. Default: beside each FBX.
  --parts | --whole    Layout. Default: bike parts when the first model is a bike.
  --main NAME          The main part with --parts. Default: chassis.
  --fbx-name           Name a bike's file after its FBX. Default: model.edf.
  --hrc MODE           always, if-none or never. Default: if-none.
  --no-shd             Don't write .shd files.
  --overwrite-shd      Write over .shd files already there.
  --shininess N        Shininess in written .shd files. Default: 6.
  --scale S            Default: 1.
  --merge DIST         Merge vertices closer than DIST.
  --flat               Flat shading.
  --recalc ANGLE       Recalculate normals with this hard-edge angle.
  --params FILE        An fbx2edf.exe parameter file or export script.
  --low-memory         Two textures at once.
  --json               Print each result as JSON.
  -h, --help           This text.

Exit code 0 when every file converted, 1 when one failed, 2 for a bad command line.";

/// Whether there is a console to write to. A release build is a windowed program, so from a
/// terminal it attaches to the terminal's console; when its output is redirected it already
/// has somewhere to write.
#[cfg(windows)]
pub fn console() -> bool {
    type Handle = *mut std::ffi::c_void;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ATTACH_PARENT_PROCESS: u32 = -1i32 as u32;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> Handle;
        fn AttachConsole(pid: u32) -> i32;
    }
    // SAFETY: plain Win32 calls with constant arguments.
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if !out.is_null() && out as isize != -1 {
            return true;
        }
        AttachConsole(ATTACH_PARENT_PROCESS) != 0
    }
}

#[cfg(not(windows))]
pub fn console() -> bool {
    true
}

#[derive(Debug)]
struct Args {
    inputs: Vec<String>,
    out: Option<PathBuf>,
    layout: Option<&'static str>,
    main: Option<String>,
    fbx_name: bool,
    hrc: HrcMode,
    shd: Shd,
    options: Options,
    params: Option<PathBuf>,
    json: bool,
}

fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let mut a = Args {
        inputs: Vec::new(),
        out: None,
        layout: None,
        main: None,
        fbx_name: false,
        hrc: HrcMode::IfNone,
        shd: Shd { make: true, ..Shd::default() },
        // The window's defaults: scale 1, no merging, the file's normals, every core.
        options: Options { scale: Some(1.0), merge_distance: Some(0.0), use_file_normals: true, ..Options::default() },
        params: None,
        json: false,
    };
    let mut it = args.iter();
    let value = |it: &mut std::slice::Iter<String>, flag: &str| it.next().cloned().ok_or(format!("{flag} needs a value"));
    let number = |v: String, flag: &str| v.parse::<f32>().ok().filter(|n| n.is_finite()).ok_or(format!("{flag}: {v} isn't a number"));
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" | "/?" => return Ok(None),
            "-o" | "--out" => a.out = Some(PathBuf::from(value(&mut it, arg)?)),
            "--parts" => a.layout = Some("parts"),
            "--whole" => a.layout = Some("whole"),
            "--main" => a.main = Some(value(&mut it, arg)?),
            "--fbx-name" => a.fbx_name = true,
            "--hrc" => {
                a.hrc = match value(&mut it, arg)?.to_ascii_lowercase().as_str() {
                    "always" => HrcMode::Always,
                    "if-none" | "ifnone" => HrcMode::IfNone,
                    "never" => HrcMode::Never,
                    other => return Err(format!("--hrc: {other} is not always, if-none or never")),
                }
            }
            "--no-shd" => a.shd.make = false,
            "--overwrite-shd" => a.shd.overwrite = true,
            "--shininess" => a.shd.shininess = number(value(&mut it, arg)?, arg)?,
            "--scale" => a.options.scale = Some(number(value(&mut it, arg)?, arg)?),
            "--merge" => a.options.merge_distance = Some(number(value(&mut it, arg)?, arg)?),
            "--flat" => {
                a.options.use_file_normals = false;
                a.options.recalc_normals = Some(0.0);
            }
            "--recalc" => {
                a.options.use_file_normals = false;
                a.options.recalc_normals = Some(number(value(&mut it, arg)?, arg)?);
            }
            "--params" => a.params = Some(PathBuf::from(value(&mut it, arg)?)),
            "--low-memory" => a.options.low_memory = true,
            "--json" => a.json = true,
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => a.inputs.push(arg.clone()),
        }
    }
    if a.inputs.is_empty() {
        return Err("no FBX files or folders given".into());
    }
    Ok(Some(a))
}

/// Run the command line; the process's exit code.
pub fn run(args: &[String]) -> i32 {
    let a = match parse(args) {
        Ok(Some(a)) => a,
        Ok(None) => {
            println!("{USAGE}");
            return 0;
        }
        Err(e) => {
            eprintln!("frost-converter: {e}\n\n{USAGE}");
            return 2;
        }
    };
    if !host::available() {
        eprintln!("frost-converter: the converter isn't in this build");
        return 1;
    }
    match convert(a) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            eprintln!("frost-converter: {e}");
            1
        }
    }
}

fn convert(mut a: Args) -> Result<bool, String> {
    let found = job::scan(&a.inputs);
    let models: Vec<Model> = found.iter().filter(|f| f.kind != "params").map(|f| Model { path: f.path.clone(), shadow: f.kind == "shadow" }).collect();
    if models.is_empty() {
        return Err("no FBX files found".into());
    }
    // A parameter file named, else the first one found among the inputs, as in the window.
    let params = a.params.clone().or_else(|| found.iter().find(|f| f.kind == "params").map(|f| PathBuf::from(&f.path)));
    if let Some(p) = &params {
        a.options.params = Some(std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?);
    }
    let layout = match a.layout {
        Some(l) => l,
        None => {
            let first = models.iter().find(|m| !m.shadow).unwrap_or(&models[0]);
            if job::is_bike(Path::new(&first.path)).unwrap_or(false) { "parts" } else { "whole" }
        }
    };
    a.options.layout = Some(layout.into());
    if layout == "parts" {
        a.options.main = a.main.clone().filter(|m| !m.trim().is_empty());
    }
    if let Some(out) = &a.out {
        std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let outputs = job::output_paths(&models, a.out.as_deref(), layout == "parts" && !a.fbx_name);
    let pairs: Vec<(PathBuf, PathBuf)> = models.iter().map(|m| PathBuf::from(&m.path)).zip(outputs).collect();
    let total = pairs.len();
    let json = a.json;
    let mut finished = 0;
    let outcomes = job::run(&pairs, &a.options, a.hrc, a.shd, &mut |p| {
        if let Progress::Done { index, outcome } = p {
            finished += 1;
            if json {
                let line = serde_json::json!({ "input": pairs[index].0, "output": pairs[index].1, "outcome": outcome });
                println!("{line}");
            } else {
                print_outcome(finished, total, &pairs[index], outcome);
            }
        }
    })?;
    let failed = outcomes.iter().filter(|o| o.report.is_none()).count();
    if !json {
        println!("{} of {total} converted", total - failed);
    }
    Ok(failed == 0)
}

fn print_outcome(n: usize, total: usize, (input, output): &(PathBuf, PathBuf), o: &Outcome) {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(r) = &o.report else {
        println!("[{n}/{total}] {} failed: {}", name(input), o.error.as_deref().unwrap_or("unknown error"));
        return;
    };
    let mb = r["bytes"].as_f64().unwrap_or(0.0) / 1e6;
    let secs = r["millis"].as_f64().unwrap_or(0.0) / 1000.0;
    println!("[{n}/{total}] {} -> {}  {mb:.1} MB  {secs:.1} s", name(input), output.display());
    let objects: Vec<String> = list(r, "objects")
        .iter()
        .map(|x| format!("{} {}", x["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("main"), x["triangles"]))
        .collect();
    println!("  objects: {}", objects.join(", "));
    if !o.hrc.is_empty() {
        println!("  hrc: {}", o.hrc.join(", "));
    }
    for line in &o.shd {
        println!("  {line}");
    }
    for t in list(r, "textures") {
        let flags = t["flags"].as_u64().unwrap_or(0);
        let what: Vec<&str> = [(1, "colour key"), (2, "alpha blend")].iter().filter(|(b, _)| flags & b != 0).map(|(_, w)| *w).collect();
        if !what.is_empty() {
            println!("  {}: {}", t["name"].as_str().unwrap_or(""), what.join(", "));
        }
    }
    for s in list(r, "skipped") {
        println!("  skipped {}: {}", s["name"].as_str().unwrap_or(""), s["reason"].as_str().unwrap_or(""));
    }
    for w in o.notes.iter().map(String::as_str).chain(list(r, "warnings").iter().filter_map(Value::as_str)) {
        println!("  note: {w}");
    }
}

fn list<'a>(r: &'a Value, key: &str) -> &'a [Value] {
    r[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn the_defaults_are_the_window_s() {
        let a = parse(&args("bike.fbx")).unwrap().unwrap();
        assert_eq!(a.inputs, ["bike.fbx"]);
        assert!(a.out.is_none() && a.layout.is_none() && !a.fbx_name && !a.json);
        assert_eq!(a.hrc, HrcMode::IfNone);
        assert!(a.shd.make && !a.shd.overwrite && a.shd.shininess == job::SHININESS);
        assert_eq!((a.options.scale, a.options.merge_distance, a.options.use_file_normals), (Some(1.0), Some(0.0), true));
    }

    #[test]
    fn options_are_read() {
        let a = parse(&args("a.fbx b -o out --parts --main frame --hrc always --no-shd --recalc 45 --json")).unwrap().unwrap();
        assert_eq!(a.inputs, ["a.fbx", "b"]);
        assert_eq!(a.out, Some(PathBuf::from("out")));
        assert_eq!((a.layout, a.main.as_deref(), a.hrc), (Some("parts"), Some("frame"), HrcMode::Always));
        assert!(!a.shd.make && a.json);
        assert_eq!((a.options.use_file_normals, a.options.recalc_normals), (false, Some(45.0)));
    }

    #[test]
    fn a_bad_command_line_says_why() {
        assert!(parse(&args("--hrc sometimes a.fbx")).unwrap_err().contains("--hrc"));
        assert!(parse(&args("a.fbx --scale big")).unwrap_err().contains("isn't a number"));
        assert!(parse(&args("a.fbx --what")).unwrap_err().contains("unknown option"));
        assert!(parse(&args("-o out")).unwrap_err().contains("no FBX"));
        assert!(parse(&args("--help")).unwrap().is_none());
    }
}
