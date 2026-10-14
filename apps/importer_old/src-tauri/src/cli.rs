//! The command line: the window's import, for scripts and tests.
//!
//! frost-importer.exe bike.pkz -o out\ [--lods] [--shadow] [--no-place] [--blender PATH] [--json]
//! frost-importer.exe lineup <swap folder> <new bike .pkz or folder> [--name N] [--anchor fit|pivot|head] [--dry-run] [--blend DIR]

use std::path::PathBuf;

use crate::lineup::{self, Anchor};
use crate::plan::Options;
use crate::run;

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

const HELP: &str = "Frost Importer: .pkz, .edf or .fbx to a .blend

  frost-importer.exe <files or folders> [options]

  -o, --out DIR     write every .blend in DIR (default: beside each file)
  --lods            include the LODs
  --shadow          include the shadow model
  --no-place        leave each part where its file has it
  --blender PATH    the blender.exe to use
  --json            print the outcome as JSON

  frost-importer.exe lineup <swap folder> <new bike .pkz or folder> [options]

  Moves a model swap built for one bike onto another, by the two bikes' .geom, and
  installs it in the new bike's FrostMod Models folder.

  --name N          the new model's name (default: the swap's)
  --anchor A        what the frame follows: fit (default), pivot or head
  --dry-run         only compare the two bikes
  --blend DIR       also build a .blend of it on the new bike in DIR
                    (with --dry-run: of the swap as it is)

Exit code: 0 when every file imported, 1 when one didn't, 2 for a bad command line.";

pub fn run(args: &[String]) -> i32 {
    if args.first().is_some_and(|a| a.eq_ignore_ascii_case("lineup")) {
        return run_lineup(&args[1..]);
    }
    let mut inputs = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut blender: Option<PathBuf> = None;
    let mut opts = Options::default();
    let mut json = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" | "/?" => {
                println!("{HELP}");
                return 0;
            }
            "-o" | "--out" => match it.next() {
                Some(v) => out = Some(PathBuf::from(v)),
                None => return bad("--out needs a folder"),
            },
            "--blender" => match it.next() {
                Some(v) => blender = Some(PathBuf::from(v)),
                None => return bad("--blender needs a path"),
            },
            "--lods" => opts.lods = true,
            "--shadow" => opts.shadow = true,
            "--no-place" => opts.place = false,
            "--json" => json = true,
            s if s.starts_with('-') => return bad(&format!("unknown option {s}")),
            s => inputs.push(s.to_string()),
        }
    }
    if inputs.is_empty() {
        return bad("no files given");
    }
    let mut failed = false;
    let mut all = Vec::new();
    for path in &inputs {
        if !json {
            println!("{path}");
        }
        let o = run::import_item(path, opts, out.as_deref(), blender.as_deref(), &mut |_, _, _| {});
        if !json {
            for b in &o.built {
                println!(
                    "  {}  {} objects, {} meshes, {} materials, {} images, {} triangles, {:.1} s",
                    b.blend, b.objects, b.meshes, b.materials, b.images, b.triangles, b.seconds
                );
            }
            if !o.locked.is_empty() {
                println!("  Locked content skipped: {}", o.locked.len());
            }
            if let Some(e) = &o.error {
                println!("  {e}");
            }
        }
        failed |= o.built.is_empty() || o.error.is_some();
        all.push(serde_json::json!({ "input": path, "outcome": o }));
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&all).unwrap_or_default());
    }
    i32::from(failed)
}

fn bad(msg: &str) -> i32 {
    eprintln!("{msg}\n\n{HELP}");
    2
}

fn run_lineup(args: &[String]) -> i32 {
    let mut pos = Vec::new();
    let (mut name, mut anchor, mut dry, mut json, mut blend, mut blender) = (None, Anchor::Fit, false, false, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--name" => match it.next() {
                Some(v) => name = Some(v.clone()),
                None => return bad("--name needs a name"),
            },
            "--anchor" => match it.next().and_then(|v| Anchor::parse(v)) {
                Some(v) => anchor = v,
                None => return bad("--anchor is fit, pivot or head"),
            },
            "--blend" => match it.next() {
                Some(v) => blend = Some(PathBuf::from(v)),
                None => return bad("--blend needs a folder"),
            },
            "--blender" => match it.next() {
                Some(v) => blender = Some(PathBuf::from(v)),
                None => return bad("--blender needs a path"),
            },
            "--dry-run" => dry = true,
            "--json" => json = true,
            s if s.starts_with('-') => return bad(&format!("unknown option {s}")),
            s => pos.push(PathBuf::from(s)),
        }
    }
    let [swap, target] = pos.as_slice() else {
        return bad("lineup needs a swap folder and a bike");
    };
    let o = match lineup::line_up(swap, target, name.as_deref(), anchor, dry) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e:#}");
            return 1;
        }
    };
    let mut built = None;
    // A dry run previews the swap as it stands, on the new bike.
    let shown = o.out.clone().or_else(|| dry.then(|| swap.to_string_lossy().to_string()));
    if let (Some(dir), Some(out)) = (&blend, &shown) {
        let geom = match lineup::bike_geom(target) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("{e:#}");
                return 1;
            }
        };
        built = Some(run::preview_lineup(std::path::Path::new(out), geom, dir, blender.as_deref(), &mut |_, _, _| {}));
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "lineup": o, "preview": built })).unwrap_or_default());
    } else {
        println!("{} -> {} ({:?})", o.from, o.to, o.plan.anchor);
        for r in &o.plan.mounts {
            println!("  {:<22} {:>24}  {:>24}  {:>6.1} {}", r.mount, r.from, r.to, r.diff, r.unit);
        }
        for m in &o.plan.moves {
            println!("  move {:<8} {:>7.1} {:>7.1} {:>7.1} mm  {:>5.2}°", m.part, m.mm[0], m.mm[1], m.mm[2], m.deg);
        }
        for i in &o.plan.issues {
            println!("  ! {i}");
        }
        for n in &o.notes {
            println!("  {n}");
        }
        if let Some(out) = &o.out {
            println!("  installed: {out} ({} files, gfx.cfg: {} values)", o.files.len(), o.gfx_changed.len());
        }
        if let Some(b) = &built {
            for x in &b.built {
                println!("  {}  {} objects, {:.1} s", x.blend, x.objects, x.seconds);
            }
            if let Some(e) = &b.error {
                println!("  preview: {e}");
            }
        }
    }
    i32::from(built.is_some_and(|b| b.error.is_some()))
}
