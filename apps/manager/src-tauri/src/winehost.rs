//! Running the game on macOS, where a Windows title lives inside a Wine prefix.
//!
//! CrossOver, Whisky, Kegworks and plain Wine all put their fake `C:` drive at
//! `<prefix>/drive_c`, and that is the only layout fact this module needs: the prefix is
//! whatever sits above `drive_c` in the game's path. Everything else — which wrapper owns
//! that prefix, which binary launches it — falls out of where the prefix is.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// The fake `C:` drive every Wine prefix carries.
const DRIVE_C: &str = "drive_c";

/// Which wrapper a prefix belongs to. Decides who gets to launch it: a bottle is best
/// driven by the wrapper that built it, whose Wine build matches the prefix's version.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wrapper {
    CrossOver,
    Whisky,
    /// A bare prefix (`$WINEPREFIX`, `~/.wine`) — nobody owns it.
    Plain,
}

/// A Wine binary we can launch through, and how it wants to be called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    pub program: PathBuf,
    pub kind: RunnerKind,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerKind {
    /// CrossOver, addressed by bottle *name*. Its own `--bottle` form is what applies the
    /// bottle's graphics environment (D3DMetal/DXVK); a bare `WINEPREFIX` launch skips it.
    CrossOver { root: PathBuf, bottle: String },
    /// Any other Wine build, pointed at the prefix with `WINEPREFIX`.
    Wine { via: &'static str },
}

impl Runner {
    /// What to call this in logs and in the UI.
    pub fn via(&self) -> &str {
        match &self.kind {
            RunnerKind::CrossOver { .. } => "CrossOver",
            RunnerKind::Wine { via } => via,
        }
    }
}

/// The command that starts `exe` under `runner`, ready to spawn.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Split a path inside a Wine prefix into the prefix and the part below `drive_c`.
///
/// The one rule the whole module rests on, and pure, so every wrapper's layout is covered
/// by tests on a machine with no Wine installed at all.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn split_prefix(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut prefix = PathBuf::new();
    let mut parts = path.components();
    for part in parts.by_ref() {
        if part.as_os_str().to_string_lossy().eq_ignore_ascii_case(DRIVE_C) {
            // A path that *starts* at `drive_c` names no prefix to launch in.
            if prefix.as_os_str().is_empty() {
                return None;
            }
            return Some((prefix, parts.collect()));
        }
        prefix.push(part);
    }
    None
}

/// A host path as the bottle sees it, for the wrappers and the Windows programs that take
/// a Windows path rather than a host one.
///
/// Two shapes, and which applies is a fact about the path: anything under the prefix's own
/// `drive_c` is on `C:`, everything else on the machine is on `Z:`, the drive Wine maps to
/// `/`. Both matter here — the game is inside the bottle, FrostMod is installed beside our
/// own data and reached from in there as `Z:`. Same rule as [`crate::proton::windows_path`].
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn windows_path(prefix: &Path, path: &Path) -> String {
    let drive_c = prefix.join(DRIVE_C);
    let (drive, rest) = match path.strip_prefix(&drive_c) {
        Ok(rest) => ("C:", rest),
        Err(_) => ("Z:", path.strip_prefix("/").unwrap_or(path)),
    };
    let joined = rest
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\\");
    format!("{drive}\\{joined}")
}

/// Can a program inside `prefix` reach the rest of this Mac?
///
/// `Z:` is what makes FrostMod's folder addressable from in there, and a bottle whose drive
/// mapping has been stripped fails *invisibly*: FrostMod injects, `F8` still reloads, and
/// every button in this app writes a file nothing can see. Worth a check that names the fix.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn has_z_drive(prefix: &Path) -> bool {
    prefix.join("dosdevices/z:").exists()
}

/// Every Wine prefix on this Mac, for the install detectors to search.
pub fn bottles() -> Vec<PathBuf> {
    bottles_in(bottle_roots().into_iter().map(|(root, _)| root))
}

/// A prefix either *is* the root (`~/.wine`) or sits one level under it (a bottles folder).
fn bottles_in(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let here = if is_prefix(&root) {
            vec![root]
        } else {
            std::fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| is_prefix(path))
                .collect()
        };
        // The same prefix can sit under two roots (a per-user CrossOver beside a
        // system one), and a duplicate would make every detector search it twice.
        for path in here {
            if !found.contains(&path) {
                found.push(path);
            }
        }
    }
    found
}

fn is_prefix(dir: &Path) -> bool {
    dir.join(DRIVE_C).is_dir()
}

/// Where each wrapper keeps its bottles.
#[cfg(target_os = "macos")]
fn bottle_roots() -> Vec<(PathBuf, Wrapper)> {
    let mut roots: Vec<(PathBuf, Wrapper)> = Vec::new();
    if let Ok(custom) = std::env::var("CX_BOTTLE_PATH") {
        roots.push((PathBuf::from(custom), Wrapper::CrossOver));
    }
    roots.push((
        PathBuf::from("/Library/Application Support/CrossOver/Bottles"),
        Wrapper::CrossOver,
    ));
    if let Ok(prefix) = std::env::var("WINEPREFIX") {
        roots.push((PathBuf::from(prefix), Wrapper::Plain));
    }
    if let Some(home) = dirs_next::home_dir() {
        roots.push((
            home.join("Library/Application Support/CrossOver/Bottles"),
            Wrapper::CrossOver,
        ));
        // Whisky moved its container id at one point; both layouts are still out there.
        roots.push((
            home.join("Library/Containers/com.isaacmarovitz.Whisky/Bottles"),
            Wrapper::Whisky,
        ));
        roots.push((home.join("Library/Containers/Whisky/Bottles"), Wrapper::Whisky));
        roots.push((home.join(".wine"), Wrapper::Plain));
    }
    roots
}

/// Off macOS there are no bottles — the game is launched natively (Windows) or through
/// Steam (Linux).
#[cfg(not(target_os = "macos"))]
fn bottle_roots() -> Vec<(PathBuf, Wrapper)> {
    Vec::new()
}

/// Wine binaries to fall back on, best first. Only the ones that exist are ever used.
#[cfg(target_os = "macos")]
fn wine_candidates() -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();
    for root in crossover_roots() {
        out.push((root.join("bin/wine"), "CrossOver"));
    }
    if let Some(home) = dirs_next::home_dir() {
        let whisky = home.join("Library/Application Support/com.isaacmarovitz.Whisky/Libraries/Wine/bin");
        out.push((whisky.join("wine64"), "Whisky"));
        out.push((whisky.join("wine"), "Whisky"));
    }
    for dir in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/opt/homebrew/opt/game-porting-toolkit/bin",
        "/usr/local/opt/game-porting-toolkit/bin",
    ] {
        out.push((PathBuf::from(dir).join("wine64"), "Wine"));
        out.push((PathBuf::from(dir).join("wine"), "Wine"));
    }
    out
}

#[cfg(not(target_os = "macos"))]
fn wine_candidates() -> Vec<(PathBuf, &'static str)> {
    Vec::new()
}

/// CrossOver installs into `/Applications` normally, but a per-user copy is legal.
#[cfg(target_os = "macos")]
fn crossover_roots() -> Vec<PathBuf> {
    let suffix = "CrossOver.app/Contents/SharedSupport/CrossOver";
    let mut roots = vec![PathBuf::from("/Applications").join(suffix)];
    if let Some(home) = dirs_next::home_dir() {
        roots.push(home.join("Applications").join(suffix));
    }
    roots
}

/// The wrapper that built `prefix`, if we recognise where it sits.
fn owner_of(prefix: &Path) -> Option<Wrapper> {
    let parent = prefix.parent()?;
    bottle_roots()
        .into_iter()
        .find(|(root, _)| root == parent || root == prefix)
        .map(|(_, wrapper)| wrapper)
}

/// Pick the binary that will launch `prefix`.
///
/// `override_path` is the Settings escape hatch: an explicit Wine binary always wins, and
/// is always driven the plain `WINEPREFIX` way, because that is the one form every Wine
/// build understands.
pub fn resolve(override_path: &str, prefix: Option<&Path>) -> Option<Runner> {
    let chosen = override_path.trim();
    if !chosen.is_empty() {
        let program = PathBuf::from(chosen);
        return program.is_file().then_some(Runner {
            program,
            kind: RunnerKind::Wine { via: "your Wine runner" },
        });
    }

    // The wrapper that owns the bottle drives it: its Wine build is the one the prefix was
    // created with, and running a different build against a prefix can upgrade it in place.
    if let Some(prefix) = prefix {
        match owner_of(prefix) {
            Some(Wrapper::CrossOver) => {
                if let Some(runner) = crossover_runner(prefix) {
                    return Some(runner);
                }
            }
            Some(Wrapper::Whisky) => {
                if let Some(runner) = first_existing("Whisky") {
                    return Some(runner);
                }
            }
            _ => {}
        }
    }

    wine_candidates()
        .into_iter()
        .find(|(program, _)| program.is_file())
        .map(|(program, via)| Runner { program, kind: RunnerKind::Wine { via } })
}

/// The first candidate belonging to `via` that is actually installed.
fn first_existing(via: &'static str) -> Option<Runner> {
    wine_candidates()
        .into_iter()
        .find(|(program, owner)| *owner == via && program.is_file())
        .map(|(program, via)| Runner { program, kind: RunnerKind::Wine { via } })
}

/// CrossOver driving one of its own bottles, by name.
fn crossover_runner(prefix: &Path) -> Option<Runner> {
    let bottle = prefix.file_name()?.to_string_lossy().into_owned();
    #[cfg(target_os = "macos")]
    for root in crossover_roots() {
        let wine = root.join("bin/wine");
        if wine.is_file() {
            return Some(Runner {
                program: wine,
                kind: RunnerKind::CrossOver { root, bottle },
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = bottle;
    None
}

/// The exact command that starts `exe` under `runner`.
///
/// `extra` is appended last in both forms, as separate argv entries — the game's parser
/// reads the connect address as the argument *after* the flag, so they must not be joined.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn plan(runner: &Runner, prefix: &Path, exe: &Path, extra: &[String]) -> Launch {
    let mut args: Vec<String> = Vec::new();
    let mut env: Vec<(String, String)> = Vec::new();
    match &runner.kind {
        RunnerKind::CrossOver { root, bottle } => {
            args.push("--bottle".into());
            args.push(bottle.clone());
            args.push("--cx-app".into());
            // A host path works too, but the Windows path is the form CodeWeavers
            // documents, and the one `--cx-app` is built to resolve. `C:` for the game,
            // which lives in the bottle; `Z:` for FrostMod, which doesn't.
            args.push(windows_path(prefix, exe));
            env.push(("CX_ROOT".into(), root.to_string_lossy().into_owned()));
        }
        RunnerKind::Wine { .. } => {
            args.push(exe.to_string_lossy().into_owned());
            env.push(("WINEPREFIX".into(), prefix.to_string_lossy().into_owned()));
        }
    }
    args.extend(extra.iter().cloned());
    Launch { program: runner.program.clone(), args, env }
}

/// Is a process running `exe`? Reads `ps -Ao pid=,args=` output.
///
/// Under Wine the game is a real process whose argv still names `mxbikes.exe`, so the
/// command line is what identifies it — the executable is Wine's. Our own pid is skipped:
/// this app's argv can carry the path it is about to launch.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn running_exe(ps_output: &str, exe: &str, own_pid: u32) -> bool {
    !pids_running(ps_output, exe, own_pid).is_empty()
}

/// Every pid in `ps_output` whose argv names `exe`, ours excluded.
///
/// More than one is normal and correct: the wrapper we spawned and the Wine process doing
/// the work both carry the path, and stopping FrostMod means stopping both.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn pids_running(ps_output: &str, exe: &str, own_pid: u32) -> Vec<u32> {
    let needle = exe.to_ascii_lowercase();
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, args) = line.trim_start().split_once(char::is_whitespace)?;
            let pid = pid.parse::<u32>().ok()?;
            (pid != own_pid && args.to_ascii_lowercase().contains(&needle)).then_some(pid)
        })
        .collect()
}

/// The process table, as `ps` reports it. Empty when we couldn't ask.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn process_table() -> String {
    std::process::Command::new("ps")
        .args(["-Ao", "pid=,args="])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// Stop everything running `exe`. Best-effort, and TERM rather than KILL: FrostMod's
/// launcher tidies up after itself when asked politely, and a Wine process killed outright
/// can leave the prefix's server holding its handles.
///
/// Shells out because the app links no libc of its own for a single `kill(2)`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn kill_exe(exe: &str) {
    let pids = pids_running(&process_table(), exe, std::process::id());
    if pids.is_empty() {
        return;
    }
    let mut cmd = std::process::Command::new("kill");
    cmd.arg("-TERM");
    for pid in &pids {
        cmd.arg(pid.to_string());
    }
    match cmd.output() {
        Ok(out) if !out.status.success() => log::warn!(
            "asking {exe} ({pids:?}) to stop failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) => log::warn!("couldn't run kill to stop {exe}: {e}"),
        _ => {}
    }
}

/// What the app found to launch with, for Settings to show. Reported rather than assumed:
/// a player whose bottle we can't see needs to know that before they press Play.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    /// The binary we'd launch through, or empty when nothing was found.
    pub runner: String,
    /// Which wrapper that binary belongs to.
    pub via: String,
    /// Prefixes we can see, so a missing bottle is visible as missing.
    pub bottles: Vec<String>,
}

/// Describe the launch environment without needing a game to launch.
pub fn describe(override_path: &str) -> HostInfo {
    let found = resolve(override_path, None);
    HostInfo {
        runner: found
            .as_ref()
            .map(|r| r.program.to_string_lossy().into_owned())
            .unwrap_or_default(),
        via: found.as_ref().map(|r| r.via().to_string()).unwrap_or_default(),
        bottles: bottles()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("frost-winehost-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_the_prefix_above_drive_c() {
        let exe = PathBuf::from(
            "/Users/x/Library/Application Support/CrossOver/Bottles/MXB/drive_c/Program Files/MX Bikes/mxbikes.exe",
        );
        let (prefix, rel) = split_prefix(&exe).expect("a bottled exe names its prefix");
        assert_eq!(
            prefix,
            PathBuf::from("/Users/x/Library/Application Support/CrossOver/Bottles/MXB")
        );
        assert_eq!(rel, PathBuf::from("Program Files/MX Bikes/mxbikes.exe"));
    }

    /// A native install has no prefix to launch in, and neither does a bare `drive_c`.
    #[test]
    fn a_path_outside_a_prefix_names_none() {
        assert!(split_prefix(Path::new("/Applications/MX Bikes/mxbikes.exe")).is_none());
        assert!(split_prefix(Path::new("drive_c/Program Files/MX Bikes/mxbikes.exe")).is_none());
    }

    #[test]
    fn rewrites_the_exe_as_the_bottle_sees_it() {
        let prefix = Path::new("/b/MXB");
        let exe = Path::new("/b/MXB/drive_c/Program Files/MX Bikes/mxbikes.exe");
        assert_eq!(
            windows_path(prefix, exe),
            "C:\\Program Files\\MX Bikes\\mxbikes.exe"
        );
    }

    /// FrostMod is installed in our own data folder, which is nowhere near the bottle —
    /// and a program inside the bottle reaches it as `Z:`, the drive Wine maps to `/`.
    /// Without this the launcher is handed a path it can't open.
    #[test]
    fn a_path_outside_the_bottle_is_on_the_z_drive() {
        let prefix = Path::new("/b/MXB");
        assert_eq!(
            windows_path(
                prefix,
                Path::new("/Users/x/Library/Application Support/MXB App/frostmod/frostmod.exe")
            ),
            "Z:\\Users\\x\\Library\\Application Support\\MXB App\\frostmod\\frostmod.exe"
        );
    }

    /// A bottle with its `Z:` mapping stripped can't see FrostMod's folder, and nothing
    /// about that failure is visible from in-game — so it is checked before starting.
    ///
    /// Unix only for the symlink `dosdevices/z:` actually is — and there are no bottles on
    /// Windows to check anyway.
    #[cfg(unix)]
    #[test]
    fn a_bottle_without_a_z_drive_is_spotted() {
        let root = temp_dir("z-drive");
        let with = root.join("with");
        std::fs::create_dir_all(with.join("dosdevices")).unwrap();
        std::os::unix::fs::symlink("/", with.join("dosdevices/z:")).unwrap();
        let without = root.join("without");
        std::fs::create_dir_all(without.join("dosdevices")).unwrap();

        assert!(has_z_drive(&with));
        assert!(!has_z_drive(&without));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Stopping FrostMod has to reach both processes that carry its name — the wrapper we
    /// spawned and the Wine process under it — and never this app.
    #[test]
    fn every_process_carrying_the_name_is_listed_except_our_own() {
        let ps = "\
  501 /sbin/launchd
 8123 /Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wine --bottle MXB --cx-app Z:\\Users\\x\\frostmod.exe
 8124 /Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wineserver
 8125 C:\\windows\\system32\\explorer.exe /desktop FrostMod.exe --game mxb
 9000 /Applications/MXB App.app/Contents/MacOS/mxb-app";
        let mut pids = pids_running(ps, "frostmod.exe", 9000);
        pids.sort();
        assert_eq!(pids, [8123, 8125], "both the wrapper and the Wine process: {pids:?}");
        assert!(pids_running(ps, "frostmod.exe", 8123).iter().all(|p| *p != 8123));
        assert!(pids_running(ps, "gpbikes.exe", 9000).is_empty());
    }

    #[test]
    fn crossover_is_driven_by_bottle_name_and_keeps_the_connect_args_apart() {
        let runner = Runner {
            program: PathBuf::from("/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wine"),
            kind: RunnerKind::CrossOver {
                root: PathBuf::from("/Applications/CrossOver.app/Contents/SharedSupport/CrossOver"),
                bottle: "MXB".into(),
            },
        };
        let launch = plan(
            &runner,
            Path::new("/b/MXB"),
            Path::new("/b/MXB/drive_c/MX Bikes/mxbikes.exe"),
            &["-directconnect".to_string(), "203.0.113.10:54210".to_string()],
        );
        assert_eq!(
            launch.args,
            [
                "--bottle",
                "MXB",
                "--cx-app",
                "C:\\MX Bikes\\mxbikes.exe",
                "-directconnect",
                "203.0.113.10:54210",
            ]
        );
        assert!(launch.env.iter().any(|(k, _)| k == "CX_ROOT"));
    }

    #[test]
    fn plain_wine_is_pointed_at_the_prefix_and_given_the_host_path() {
        let runner = Runner {
            program: PathBuf::from("/opt/homebrew/bin/wine64"),
            kind: RunnerKind::Wine { via: "Wine" },
        };
        let launch = plan(
            &runner,
            Path::new("/b/MXB"),
            Path::new("/b/MXB/drive_c/MX Bikes/mxbikes.exe"),
            &["-directconnect".to_string(), "203.0.113.10:54210".to_string()],
        );
        assert_eq!(
            launch.args,
            [
                "/b/MXB/drive_c/MX Bikes/mxbikes.exe",
                "-directconnect",
                "203.0.113.10:54210",
            ]
        );
        assert_eq!(
            launch.env,
            [("WINEPREFIX".to_string(), "/b/MXB".to_string())]
        );
    }

    #[test]
    fn collects_bottles_one_level_down_and_roots_that_are_prefixes_themselves() {
        let root = temp_dir("bottles");
        let bottles = root.join("Bottles");
        for name in ["MXB", "Other"] {
            std::fs::create_dir_all(bottles.join(name).join(DRIVE_C)).unwrap();
        }
        // A folder that isn't a prefix must not be offered as one.
        std::fs::create_dir_all(bottles.join("NotABottle")).unwrap();
        let bare = root.join("dotwine");
        std::fs::create_dir_all(bare.join(DRIVE_C)).unwrap();

        let mut found = bottles_in([bottles.clone(), bare.clone(), root.join("missing")]);
        found.sort();
        let mut want = vec![bottles.join("MXB"), bottles.join("Other"), bare];
        want.sort();
        assert_eq!(found, want);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spots_the_game_in_a_process_list_but_never_ourselves() {
        let ps = "\
  501 /sbin/launchd
 8123 /Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wine C:\\MX Bikes\\MXBikes.exe
 9000 /Applications/MXB App.app/Contents/MacOS/mxb-app";
        assert!(running_exe(ps, "mxbikes.exe", 9000));
        // Our own process is the one that carries the path it just launched.
        assert!(!running_exe(ps, "mxbikes.exe", 8123));
        assert!(!running_exe(ps, "gpbikes.exe", 9000));
        assert!(!running_exe("", "mxbikes.exe", 9000));
    }

    #[test]
    fn an_explicit_runner_wins_and_a_missing_one_is_not_used() {
        let dir = temp_dir("override");
        let bin = dir.join("wine64");
        std::fs::write(&bin, b"#!/bin/sh\n").unwrap();

        let runner = resolve(&bin.to_string_lossy(), None).expect("an existing binary is used");
        assert_eq!(runner.program, bin);
        assert!(matches!(runner.kind, RunnerKind::Wine { .. }));

        assert!(resolve(&dir.join("gone").to_string_lossy(), None).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
