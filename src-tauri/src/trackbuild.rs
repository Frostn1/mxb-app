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
use std::path::{Path, PathBuf};

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

/// The three runs, in order. Stops at the first failure that makes the next one pointless.
///
/// `game_path` is only used to find a Wine prefix on macOS — the compilers are Windows
/// binaries and the prefix that runs the game is the one that has the runtime they need.
pub fn compile(tools: &Tools, dir: &Path, slug: &str, game_path: &str) -> Result<Vec<Step>> {
    if !dir.join("track.hmf").is_file() {
        bail!("{dir:?} doesn't look like an exported track — there's no track.hmf in it");
    }
    let mut steps = Vec::new();

    let map = format!("{slug}/{slug}.map");
    steps.push(run(
        "map",
        &tools.terrained,
        &["track.hmf", &map, "params.ini"],
        dir,
        game_path,
        Some(&map),
    )?);

    let trh = format!("{slug}/{slug}.trh");
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
    let Some(runner) = crate::winehost::resolve("", Some(&prefix)) else {
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

    #[test]
    fn compiling_something_that_isnt_a_track_says_so() {
        let dir = scratch("not-a-track");
        let tools = Tools {
            terrained: dir.join("terrained.exe"),
            tracked: None,
        };
        let err = compile(&tools, &dir, "x", "").unwrap_err().to_string();
        assert!(err.contains("no track.hmf"), "{err}");
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
