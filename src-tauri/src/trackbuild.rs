//! Running PiBoSo's compilers over an exported track.
//!
//! MX Bikes' track build is two command lines and, optionally, a third:
//!
//! ```text
//! terrained.exe track.hmf mytrack/mytrack.map params.ini      graphics
//! terrained.exe track.tht mytrack/mytrack.trh trh_params.ini  collision
//! tracked.exe -merge mytrack/mytrack.trh cl track.tcl sa track_start.tcl   the lines
//! ```
//!
//! Which the app can run for you, so exporting and compiling are one button rather than a
//! folder and a set of instructions. The tools are a separate download and not ours to ship,
//! so nothing here is offered until someone points at them.
//!
//! The arguments are relative, and the working directory is the export folder — deliberately,
//! because that is exactly what the batch files in PiBoSo's example do and the `.hmf` resolves
//! its own `data` and `map` entries the same way. On macOS the exes are Windows binaries, so
//! they go through the same Wine host the game does.

#![allow(dead_code)]

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// The compilers, once found.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub terrained: PathBuf,
    /// Optional: without it the terrain still builds, it just has no centreline yet.
    pub tracked: Option<PathBuf>,
}

/// Look for the compilers in a folder.
///
/// Case-insensitively, and one level down as well: people point at the zip they extracted
/// rather than at the directory inside it, and being wrong about that is not worth an error
/// message.
pub fn find(dir: &Path) -> Option<Tools> {
    let hunt = |name: &str| -> Option<PathBuf> {
        let mut roots = vec![dir.to_path_buf()];
        if let Ok(entries) = std::fs::read_dir(dir) {
            roots.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                    return Some(e.path());
                }
            }
        }
        None
    };
    Some(Tools {
        terrained: hunt("terrained.exe")?,
        tracked: hunt("tracked.exe"),
    })
}

/// One compiler run, and what it said.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// `map`, `trh` or `centerline`.
    pub name: &'static str,
    pub ok: bool,
    pub code: Option<i32>,
    /// Both streams, together and trimmed. TerrainEd says why it stopped on stdout.
    pub output: String,
    /// The file it was supposed to write, when it wrote one.
    pub produced: Option<String>,
}

/// What a build is doing, and where that sits on the studio's bar.
///
/// The compilers say nothing useful while they run. TerrainEd prints a line per stage, but
/// its stdout is block-buffered whenever it isn't a console, so the whole log arrives at
/// once when the process exits — measured on 2026-09-06, a 45-second graphics pass delivered
/// every one of its 90 stage lines inside the last 100 ms. Watching that output would give a
/// bar that sat still and then finished, so the bar is driven by the phases of the build
/// instead: each one says where it starts and ends, and how long it is expected to run, and
/// the studio eases across that span over that long.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// One of the names in [`Plan`] — the studio has a line of its own for each.
    pub phase: &'static str,
    /// Where this phase starts and ends on the bar, 0–1.
    pub from: f32,
    pub to: f32,
    /// Seconds it is expected to run for.
    pub expect: f32,
}

/// Seconds a phase takes per million terrain samples, as a first guess.
///
/// Measured on 2026-09-06 against a generated 2049² track — 4.2 megasamples — with PiBoSo's
/// compilers under Wine on an M4: synthesis 1.7 s, writing the source folder 2.5 s, the
/// graphics pass 45 s, collision 2 s, the centreline merge 2 s, packaging the 74 MB archive
/// 2 s. Per sample rather than flat because that is what every one of these phases is
/// chewing through, and a track can be a quarter the size or four times it.
///
/// Only ever the first build's guess — [`Plan::finish`] replaces each figure with what this
/// machine actually did. Windows without Wine is several times quicker, and a bar paced by a
/// Mac's numbers there would crawl and then jump.
const PHASES: [(&str, f32); 7] = [
    // Measured on a debug build, so a shipped one beats it — which only means the bar leaves
    // this phase early on the first build, and the figure below is its own after that.
    ("synthesising", 0.4),
    ("writing", 0.6),
    ("map", 10.7),
    ("trh", 0.5),
    ("centerline", 0.5),
    ("packaging", 0.5),
    ("installing", 0.1),
];

/// What the phases really cost on this machine, per megasample. Empty until a build has
/// finished one, and never written to disk: a stale figure from another version of the
/// compilers is worth less than one honest build's worth of measuring.
static LEARNED: Mutex<BTreeMap<&'static str, f32>> = Mutex::new(BTreeMap::new());

/// The phases one build will go through, and how far along each of them sits.
pub struct Plan {
    /// Name and expected seconds, in the order they run.
    phases: Vec<(&'static str, f32)>,
    total: f32,
    /// Terrain samples in millions — what every expectation is scaled by.
    mega: f32,
    running: Option<(&'static str, Instant)>,
}

impl Plan {
    /// `samples` is the terrain's edge in samples, the same number the program carries.
    pub fn new(samples: u32, centerline: bool, install: bool) -> Self {
        let mega = ((samples as f32).powi(2) / 1.0e6).max(0.05);
        let learned = LEARNED.lock().unwrap_or_else(|e| e.into_inner());
        let phases: Vec<(&'static str, f32)> = PHASES
            .iter()
            .filter(|(name, _)| match *name {
                "centerline" => centerline,
                "installing" => install,
                _ => true,
            })
            .map(|(name, per)| (*name, learned.get(name).copied().unwrap_or(*per) * mega))
            .collect();
        let total = phases.iter().map(|(_, s)| s).sum::<f32>().max(0.001);
        Self { phases, total, mega, running: None }
    }

    /// Begin a phase, closing the one before it.
    ///
    /// By name rather than in turn, so a build that skips one — no `tracked.exe`, nothing to
    /// install — lands on the right span instead of shifting everything after it.
    pub fn start(&mut self, phase: &'static str) -> Progress {
        self.finish();
        let at = self.phases.iter().position(|(n, _)| *n == phase).unwrap_or(0);
        let from = self.phases[..at].iter().map(|(_, s)| s).sum::<f32>() / self.total;
        let expect = self.phases[at].1;
        self.running = Some((phase, Instant::now()));
        Progress { phase, from, to: from + expect / self.total, expect }
    }

    /// Close the running phase, remembering what it really took.
    pub fn finish(&mut self) {
        let Some((phase, since)) = self.running.take() else {
            return;
        };
        let ran = since.elapsed().as_secs_f32();
        // A phase that was over before it started did nothing worth timing — an install onto
        // a warm cache, or a step that bailed. Learning from it would tell the next build
        // that the whole thing is instant.
        if ran < 0.05 {
            return;
        }
        let per = ran / self.mega;
        // Blended with what was already known rather than replacing it: one build that
        // fought a cold disk cache shouldn't set the pace for every build after it.
        let mut learned = LEARNED.lock().unwrap_or_else(|e| e.into_inner());
        let now = learned.entry(phase).or_insert(per);
        *now = *now * 0.5 + per * 0.5;
    }
}

/// The three runs, in order. Stops at the first failure that makes the next one pointless.
///
/// `game_path` is only used to find a Wine prefix on macOS — the compilers are Windows
/// binaries and the prefix that runs the game is the one that has the runtime they need.
///
/// `starting` is called with each run's name just before it begins. It is the only sign of
/// life there is while a build is going: see [`Progress`] for why the compilers' own output
/// can't be watched.
pub fn compile(
    tools: &Tools,
    dir: &Path,
    slug: &str,
    game_path: &str,
    starting: &mut dyn FnMut(&'static str),
) -> Result<Vec<Step>> {
    if !dir.join("track.hmf").is_file() {
        bail!("{dir:?} doesn't look like an exported track — there's no track.hmf in it");
    }
    let mut steps = Vec::new();

    let map = format!("{slug}/{slug}.map");
    starting("map");
    steps.push(run(
        "map",
        &tools.terrained,
        &["track.hmf", &map, "params.ini"],
        dir,
        game_path,
        Some(&map),
    )?);

    let trh = format!("{slug}/{slug}.trh");
    starting("trh");
    steps.push(run(
        "trh",
        &tools.terrained,
        &["track.tht", &trh, "trh_params.ini"],
        dir,
        game_path,
        Some(&trh),
    )?);

    // The lines are merged into the collision file, so this can only run once that exists.
    // Both of them, as PiBoSo's own example does: `cl` is the racing line and `sa` the start,
    // and a track merged without the second starts its races off the line it drew.
    if let (Some(tracked), true) = (&tools.tracked, dir.join(&trh).is_file()) {
        starting("centerline");
        let args = merge_args(&trh, dir.join("track_start.tcl").is_file());
        steps.push(run("centerline", tracked, &args, dir, game_path, Some(&trh))?);
    }
    Ok(steps)
}

/// The archive the game reads, from the folder the compilers just filled.
///
/// Everything under `<dir>/<slug>/` and nothing else. The source beside it — the heightmap,
/// the masks, the sheets and their shaders — is a couple of hundred megabytes the game never
/// opens, and every published track ships only what is inside the folder named after it.
pub fn package(dir: &Path, slug: &str, to: &Path) -> Result<u64> {
    let root = dir.join(slug);
    if !root.is_dir() {
        bail!("nothing was compiled: there's no {slug} folder in {dir:?}");
    }
    let file = std::fs::File::create(to).with_context(|| format!("create {to:?}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut n = 0usize;
    let mut stack = vec![root.clone()];
    while let Some(at) = stack.pop() {
        for e in std::fs::read_dir(&at)
            .with_context(|| format!("read {at:?}"))?
            .flatten()
        {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            // Named from the track folder up, so the archive nests the way the game expects.
            let rel = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            use std::io::Write;
            zip.start_file(rel, opts)?;
            zip.write_all(&std::fs::read(&path)?)?;
            n += 1;
        }
    }
    zip.finish()?;
    if n == 0 {
        bail!("{root:?} is empty — nothing to package");
    }
    Ok(std::fs::metadata(to).map(|m| m.len()).unwrap_or(0))
}

/// Put the archive where the game lists it.
///
/// Overwrites: a rebuild of the same track is the same track, and leaving the old one beside
/// it is two entries in the game's list with one name.
pub fn install(pkz: &Path, tracks_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(tracks_dir)
        .with_context(|| format!("make {tracks_dir:?}"))?;
    let to = tracks_dir.join(
        pkz.file_name()
            .ok_or_else(|| anyhow::anyhow!("{pkz:?} has no file name"))?,
    );
    std::fs::copy(pkz, &to).with_context(|| format!("copy to {to:?}"))?;
    Ok(to)
}

/// What `tracked -merge` is given: the racing line always, the start line when there is one.
fn merge_args(trh: &str, has_start: bool) -> Vec<&str> {
    let mut args = vec!["-merge", trh, "cl", "track.tcl"];
    if has_start {
        args.extend_from_slice(&["sa", "track_start.tcl"]);
    }
    args
}

fn run(
    name: &'static str,
    exe: &Path,
    args: &[&str],
    dir: &Path,
    game_path: &str,
    produces: Option<&str>,
) -> Result<Step> {
    let mut cmd = command(exe, args, game_path)?;
    cmd.current_dir(dir);
    let out = cmd
        .output()
        .with_context(|| format!("running {}", exe.display()))?;

    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push('\n');
        text.push_str(&err);
    }

    // The exit code is not the whole story: TerrainEd has been seen to exit non-zero on a
    // successful run, so the file it was asked for is the thing that decides.
    let produced = produces.filter(|p| dir.join(p).is_file());
    Ok(Step {
        name,
        ok: produced.is_some() || (produces.is_none() && out.status.success()),
        code: out.status.code(),
        output: text.trim().to_string(),
        produced: produced.map(str::to_string),
    })
}

/// The command that runs a Windows executable here.
#[cfg(target_os = "windows")]
fn command(exe: &Path, args: &[&str], _game_path: &str) -> Result<std::process::Command> {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    Ok(cmd)
}

/// On macOS the compilers are Windows binaries, so they go through the same Wine host the
/// game does — and through the game's own prefix, which already has whatever runtime PiBoSo's
/// tools were built against.
#[cfg(not(target_os = "windows"))]
fn command(exe: &Path, args: &[&str], game_path: &str) -> Result<std::process::Command> {
    let Some((prefix, _)) = crate::winehost::split_prefix(Path::new(game_path)) else {
        bail!(
            "the compilers are Windows programs. Set the game path to a copy inside a Wine \
             prefix — CrossOver, Whisky or plain Wine — and they'll run through that."
        );
    };
    // `FROST_WINE` names a runner outside the places the resolver looks, which is how the
    // build harness drives a Wine build that lives beside the compilers rather than installed.
    // Empty unless set, so nothing changes for the app.
    let over = std::env::var("FROST_WINE").unwrap_or_default();
    let Some(runner) = crate::winehost::resolve(&over, Some(&prefix)) else {
        bail!("found a Wine prefix at {prefix:?} but nothing that can run it");
    };
    let extra: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let launch = crate::winehost::plan(&runner, &prefix, exe, &extra);
    let mut cmd = std::process::Command::new(&launch.program);
    cmd.args(&launch.args);
    for (k, v) in &launch.env {
        cmd.env(k, v);
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxb-tools-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_empty_folder_has_no_tools_in_it() {
        assert!(find(&scratch("empty")).is_none());
    }

    #[test]
    fn the_compilers_are_found_whatever_their_case() {
        let dir = scratch("case");
        std::fs::write(dir.join("TerrainEd.exe"), b"").unwrap();
        std::fs::write(dir.join("TrackEd.EXE"), b"").unwrap();
        let tools = find(&dir).expect("both are there");
        assert!(tools.terrained.ends_with("TerrainEd.exe"));
        assert!(tools.tracked.is_some());
    }

    #[test]
    fn tracked_is_optional() {
        let dir = scratch("terrain-only");
        std::fs::write(dir.join("terrained.exe"), b"").unwrap();
        let tools = find(&dir).expect("terrained alone is enough");
        assert!(tools.tracked.is_none());
    }

    /// People extract the download and point at the folder they extracted, not at the one
    /// inside it. Both work.
    #[test]
    fn a_folder_holding_the_tools_folder_works_too() {
        let dir = scratch("nested");
        let inner = dir.join("MXB Track Tools");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("terrained.exe"), b"").unwrap();
        assert!(find(&dir).is_some());
    }

    #[test]
    fn packaging_nests_the_track_folder_and_leaves_the_source_behind() {
        let dir = scratch("package");
        std::fs::create_dir_all(dir.join("mytrack/sub")).unwrap();
        std::fs::write(dir.join("mytrack/mytrack.map"), b"map").unwrap();
        std::fs::write(dir.join("mytrack/sub/deep.tga"), b"tga").unwrap();
        // The source beside it, which the game never opens.
        std::fs::write(dir.join("heightmap.raw"), vec![0u8; 4096]).unwrap();

        let pkz = dir.join("mytrack.pkz");
        assert!(package(&dir, "mytrack", &pkz).unwrap() > 0);

        let mut zip = zip::ZipArchive::new(std::fs::File::open(&pkz).unwrap()).unwrap();
        let mut names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["mytrack/mytrack.map", "mytrack/sub/deep.tga"]);
    }

    #[test]
    fn packaging_a_folder_that_was_never_compiled_says_so() {
        let dir = scratch("package-empty");
        let err = package(&dir, "mytrack", &dir.join("x.pkz"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("nothing was compiled"), "{err}");
    }

    #[test]
    fn installing_replaces_the_track_that_was_there() {
        let dir = scratch("install");
        let tracks = dir.join("mods/tracks");
        std::fs::write(dir.join("t.pkz"), b"new").unwrap();
        std::fs::create_dir_all(&tracks).unwrap();
        std::fs::write(tracks.join("t.pkz"), b"old").unwrap();

        let at = install(&dir.join("t.pkz"), &tracks).unwrap();
        assert_eq!(at, tracks.join("t.pkz"));
        assert_eq!(std::fs::read(&at).unwrap(), b"new");
    }

    /// PiBoSo's example merges both lines, and a track without the start one starts its
    /// races off the line it drew.
    #[test]
    fn both_lines_are_merged_when_both_are_there() {
        assert_eq!(
            merge_args("t/t.trh", true),
            ["-merge", "t/t.trh", "cl", "track.tcl", "sa", "track_start.tcl"]
        );
        assert_eq!(
            merge_args("t/t.trh", false),
            ["-merge", "t/t.trh", "cl", "track.tcl"]
        );
    }

    /// The bar runs 0 to 1 once, in order, with no gap between one phase and the next.
    #[test]
    fn the_phases_tile_the_whole_bar() {
        let mut plan = Plan::new(2049, true, true);
        let mut at = 0.0f32;
        for phase in ["synthesising", "writing", "map", "trh", "centerline", "packaging", "installing"] {
            let p = plan.start(phase);
            assert!((p.from - at).abs() < 1e-4, "{phase} starts at {} not {at}", p.from);
            assert!(p.to > p.from, "{phase} takes no time");
            at = p.to;
        }
        assert!((at - 1.0).abs() < 1e-4, "the bar ends at {at}");
    }

    /// Without `tracked.exe` there is no centreline step, and nothing after it should be
    /// left waiting on a phase that never runs.
    #[test]
    fn a_skipped_phase_leaves_no_hole() {
        let mut plan = Plan::new(2049, false, false);
        let packaging = plan.start("packaging");
        assert!(packaging.to > 0.99, "packaging is the last phase, ending at {}", packaging.to);
        assert!(packaging.from < packaging.to);
    }

    /// The graphics pass is the one worth waiting for, so it has to own most of the bar —
    /// measured at 45 s of a 56 s build.
    #[test]
    fn the_graphics_pass_owns_most_of_the_bar() {
        let mut plan = Plan::new(2049, true, true);
        let map = plan.start("map");
        assert!(map.to - map.from > 0.6, "the map pass covers {:.2}", map.to - map.from);
    }

    /// A bigger terrain is more of everything, so it is expected to take proportionally
    /// longer — the spans stay put, only the seconds grow.
    #[test]
    fn a_bigger_terrain_expects_longer() {
        let small = Plan::new(1025, true, true).start("map").expect;
        let big = Plan::new(2049, true, true).start("map").expect;
        assert!(big > small * 3.5, "{big} vs {small}");
    }

    #[test]
    fn compiling_something_that_isnt_a_track_says_so() {
        let dir = scratch("not-a-track");
        let tools = Tools {
            terrained: dir.join("terrained.exe"),
            tracked: None,
        };
        let err = compile(&tools, &dir, "x", "", &mut |_| {}).unwrap_err().to_string();
        assert!(err.contains("no track.hmf"), "{err}");
    }

    /// The bar is driven by these calls and nothing else, so a run that never announces
    /// itself is a build that looks frozen. Checked on the failing path deliberately: the
    /// first compiler can't start here, and the phase still has to be reported before it is
    /// tried, or the studio would sit on "writing the source" through the whole graphics pass.
    #[test]
    fn each_run_says_so_before_it_starts() {
        let dir = scratch("announces");
        std::fs::write(dir.join("track.hmf"), b"").unwrap();
        let tools = Tools {
            terrained: dir.join("terrained.exe"),
            tracked: None,
        };
        let mut said = Vec::new();
        let _ = compile(&tools, &dir, "x", "", &mut |phase| said.push(phase));
        assert_eq!(said.first(), Some(&"map"), "said {said:?}");
    }

    /// Not on Windows, and not inside a Wine prefix, the failure has to name the reason
    /// rather than surfacing whatever the OS says about an unrunnable file.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn without_a_prefix_it_explains_itself() {
        let err = command(Path::new("/nowhere/terrained.exe"), &["a"], "/Applications/Game")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Wine prefix"), "{err}");
    }
}


#[cfg(test)]
mod build_one {
    use super::*;

    /// Build a whole track from a program and leave a `.pkz` where the game can open it.
    ///
    /// Export, then TerrainEd for the `.map` and the `.trh`, then `tracked -merge` for the
    /// lines, then pack. What the app does, driven from a program on disk so a generated
    /// track can be looked at in the game and in the viewer.
    ///
    /// ```text
    /// FROST_PROGRAM=/tmp/prog.json FROST_OUT=/tmp/build \
    /// FROST_TOOLS=~/Downloads/mxb-trackbuild/tools \
    /// FROST_PREFIX=~/Downloads/mxb-trackbuild/prefix \
    /// FROST_WINE="~/Downloads/mxb-trackbuild/Wine Devel.app/Contents/Resources/wine/bin/wine" \
    ///   cargo test --bin mxb-app -- --ignored --nocapture build_a_track_to_pkz
    /// ```
    #[test]
    #[ignore = "needs PiBoSo's compilers and a Wine prefix"]
    fn build_a_track_to_pkz() {
        let prog_path = std::env::var("FROST_PROGRAM").expect("set FROST_PROGRAM");
        let out = PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        let tools_dir = PathBuf::from(std::env::var("FROST_TOOLS").expect("set FROST_TOOLS"));
        let tools = find(&tools_dir).expect("terrained.exe under FROST_TOOLS");

        // A seed goes through the Rust layout generator, which is the one that measures the
        // lap it drew against the corpus and rejects what does not pass. A path still reads a
        // program from disk, for a shape that came from somewhere else.
        let prog: crate::trackprog::TrackProgram = if prog_path.starts_with("seed:") {
            let from: u64 = prog_path[5..].parse().expect("seed:<number>");
            match crate::tracklayout::search(from, 400) {
                Ok(m) => {
                    println!("  seed {} passed review and ground notes", m.seed);
                    m.program
                }
                Err(rejected) => {
                    let best = rejected
                        .iter()
                        .min_by_key(|m| m.review.len() + m.ground.len())
                        .expect("something tried");
                    println!(
                        "  no seed passed cleanly in 400; best is {} with {:?} {:?}",
                        best.seed, best.review, best.ground
                    );
                    best.program.clone()
                }
            }
        } else {
            serde_json::from_str(&std::fs::read_to_string(&prog_path).unwrap()).unwrap()
        };
        let prog = crate::tracksynth::with_fitted_budget(&prog).expect("a height budget");
        let syn = crate::tracksynth::synthesise(&prog).expect("synthesise");

        let slug = prog.name.replace(' ', "_");
        let dir = out.join(&slug);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wrote = crate::tracksynth::write_source(&prog, &syn, &dir).expect("export");
        println!("  exported {} files to {}", wrote.len(), dir.display());

        let mut phase = |p: &'static str| println!("  .. {p}");
        let game = std::env::var("FROST_GAME").unwrap_or_default();
        let steps = compile(&tools, &dir, &slug, &game, &mut phase).expect("compile");
        for s in &steps {
            println!("  {} -> {}", s.name, if s.ok { "ok" } else { "FAILED" });
            if !s.ok {
                println!("{}", s.output);
            }
        }
        let pkz = out.join(format!("{slug}.pkz"));
        let n = package(&dir, &slug, &pkz).expect("package");
        println!("  {} -- {:.1} MB", pkz.display(), n as f64 / 1_048_576.0);
    }
}
