//! Running PiBoSo's compilers over an exported track.
//!
//! MX Bikes' track build is two command lines and, optionally, a third:
//!
//! ```text
//! terrained.exe track.hmf mytrack/mytrack.map params.ini      graphics
//! terrained.exe track.tht mytrack/mytrack.trh trh_params.ini  collision
//! tracked.exe -merge mytrack/mytrack.trh cl track.tcl         the centreline
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

    // The centreline is merged into the collision file, so it can only run once that exists.
    if let (Some(tracked), true) = (&tools.tracked, dir.join(&trh).is_file()) {
        steps.push(run(
            "centerline",
            tracked,
            &["-merge", &trh, "cl", "track.tcl"],
            dir,
            game_path,
            Some(&trh),
        )?);
    }
    Ok(steps)
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
