//! Running a Windows program inside the Proton prefix the game already uses (Linux).
//!
//! MX Bikes has no Linux build. On Linux it is the same `mxbikes.exe`, run by Steam
//! through Proton inside a Wine prefix at `steamapps/compatdata/<appid>`. FrostMod is a
//! Win32 launcher that injects a Win32 DLL into that process, so the only place it can
//! possibly run is *that* prefix — a copy started any other way is a stranger to the game
//! it is meant to attach to.
//!
//! So this module answers three questions, and nothing else:
//!   - which prefix does this game use, and has it ever been made? ([`find`])
//!   - which Proton owns it? (`config_info`, then whatever is installed)
//!   - how does a path we know as `/home/…` read from inside it? ([`windows_path`])
//!
//! The macOS equivalent is [`crate::winehost`], which answers the same questions for
//! CrossOver/Whisky bottles. They stay apart because nothing about the answers is shared:
//! there the player chose the bottle, here Steam made it.

use std::path::{Path, PathBuf};

/// The fake `C:` drive inside a prefix. Same fact [`crate::winehost`] is built on.
const DRIVE_C: &str = "drive_c";

/// Everything needed to start a Windows program in the game's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    /// `<library>/steamapps/compatdata/<appid>` — what Proton calls the compat data path.
    /// The prefix itself is `pfx` inside it.
    pub compat_data: PathBuf,
    /// The `proton` script that owns the prefix, or the runner the player named in
    /// Settings.
    pub program: PathBuf,
    /// Steam's own root. Proton refuses to run without being told where it is.
    pub client: PathBuf,
    /// Whether `program` is Proton (which takes a `run` verb) or something the player
    /// pointed us at, which is handed the exe directly.
    pub kind: RunnerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerKind {
    /// Valve's `proton` script: `proton run <exe>`, with the compat paths in the
    /// environment.
    Proton,
    /// A binary from Settings — umu-run, a wrapper script, a plain `wine`. Given the exe
    /// as its first argument and pointed at the prefix with `WINEPREFIX`, which is the one
    /// form every Wine-ish runner understands.
    Custom,
}

impl Runner {
    pub fn prefix(&self) -> PathBuf {
        self.compat_data.join("pfx")
    }

    /// What to call this in a log line.
    pub fn via(&self) -> &str {
        match self.kind {
            RunnerKind::Proton => "Proton",
            RunnerKind::Custom => "your runner",
        }
    }

    /// The command that starts `exe` with `args` inside the prefix.
    ///
    /// `STEAM_COMPAT_DATA_PATH` is the whole point: it is what makes this the *game's*
    /// prefix rather than a fresh one, and so what puts the launcher in the same Windows
    /// session as the process it wants to inject into.
    pub fn command(&self, exe: &Path, args: &[String]) -> std::process::Command {
        let mut cmd = std::process::Command::new(&self.program);
        match self.kind {
            RunnerKind::Proton => {
                cmd.arg("run").arg(exe);
            }
            RunnerKind::Custom => {
                cmd.arg(exe);
                cmd.env("WINEPREFIX", self.prefix());
            }
        }
        cmd.args(args);
        cmd.env("STEAM_COMPAT_DATA_PATH", &self.compat_data);
        cmd.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &self.client);
        cmd
    }
}

/// The runner for `game`, or an error saying which half is missing.
///
/// `override_path` is the Settings escape hatch (the same field macOS uses to name a Wine
/// binary): an explicit runner always wins, because a machine running Proton from
/// somewhere we don't look is a machine we can't guess about.
pub fn find(game: &crate::game::GameProfile, override_path: &str) -> anyhow::Result<Runner> {
    let (client, compat_data) = compat_data(game).ok_or_else(|| {
        anyhow::anyhow!(
            "No Proton prefix for {} yet — start the game through Steam once so Proton \
             creates it, then start FrostMod.",
            game.display
        )
    })?;

    let chosen = override_path.trim();
    if !chosen.is_empty() {
        let program = PathBuf::from(chosen);
        if !program.is_file() {
            anyhow::bail!("The Wine/Proton runner set in Settings isn't there: {chosen}");
        }
        return Ok(Runner { compat_data, program, client, kind: RunnerKind::Custom });
    }

    let program = proton_for(&compat_data).ok_or_else(|| {
        anyhow::anyhow!(
            "Couldn't find the Proton that runs {} — its prefix is at {}, but no Proton \
             install named by it (or installed beside it) is there. Reinstalling Proton \
             for the game in Steam is the fix; a `wineRunner` path in the app's \
             config.json overrides this if you run Proton from somewhere of your own.",
            game.display,
            compat_data.display(),
        )
    })?;
    Ok(Runner { compat_data, program, client, kind: RunnerKind::Proton })
}

/// The prefix folder inside a compat data dir.
const PREFIX_DIR: &str = "pfx";

/// The Steam client root, and the game's compat data folder.
///
/// Two different things, and a machine with a second drive is where that shows: the
/// prefix lives beside the game, in whichever *library* holds it, while the client path
/// Proton wants is where Steam itself is installed. Handing it a library root works
/// until someone's games are on the other disk.
///
/// Only a folder that really holds a `pfx` counts. Steam creates `compatdata/<appid>` for
/// anything it has *considered* running under Proton, and an empty one names no prefix to
/// join.
fn compat_data(game: &crate::game::GameProfile) -> Option<(PathBuf, PathBuf)> {
    let (library, compat) = crate::config::steam_libraries().into_iter().find_map(|lib| {
        let dir = lib.join("steamapps").join("compatdata").join(game.steam_appid);
        dir.join(PREFIX_DIR).is_dir().then_some((lib, dir))
    })?;
    // Falling back to the library that holds the prefix costs nothing on the common
    // single-drive install, where they are the same folder — and is a better guess than
    // refusing to start over a Steam installed somewhere we don't know to look.
    Some((steam_client_root().unwrap_or(library), compat))
}

/// Where Steam itself is installed. The Flatpak and snap builds keep their own home, so
/// they are looked for too — a Flatpak player is exactly the sort who has never seen
/// `~/.steam` in their life.
fn steam_client_root() -> Option<PathBuf> {
    let home = dirs_next::home_dir()?;
    [
        ".steam/steam",
        ".local/share/Steam",
        ".steam/root",
        ".var/app/com.valvesoftware.Steam/data/Steam",
        "snap/steam/common/.local/share/Steam",
    ]
    .into_iter()
    .map(|rel| home.join(rel))
    .find(|dir| dir.join("steamapps").is_dir())
}

/// The Proton that owns `compat_data`, preferring the one Steam recorded for it.
fn proton_for(compat_data: &Path) -> Option<PathBuf> {
    if let Ok(text) = std::fs::read_to_string(compat_data.join("config_info")) {
        if let Some(root) = proton_root_in_config_info(&text) {
            if let Some(script) = proton_script(&root) {
                return Some(script);
            }
        }
    }
    installed_protons().into_iter().find_map(|dir| proton_script(&dir))
}

/// Pull the Proton install out of a prefix's `config_info`.
///
/// Proton writes that file when it builds the prefix, and every path in it points back
/// into its own install — `…/Proton 9.0/files/share/fonts/`, `…/dist/share/default_pfx/`,
/// depending on the Proton version's layout. So rather than depend on the file's shape,
/// take the first line that names one of those two directories and keep what comes before
/// it. That is the install root whatever the rest of the file says.
fn proton_root_in_config_info(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        for marker in ["/files/", "/dist/"] {
            if let Some(cut) = line.find(marker) {
                return Some(PathBuf::from(&line[..cut]));
            }
        }
    }
    None
}

/// The runnable `proton` script inside an install, if it's there.
fn proton_script(root: &Path) -> Option<PathBuf> {
    let script = root.join("proton");
    script.is_file().then_some(script)
}

/// Every Proton install we can see, best first.
///
/// Only reached when `config_info` didn't answer — a prefix from a Proton that has since
/// been uninstalled, say. Ranked by [`proton_rank`].
fn installed_protons() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for root in crate::config::steam_libraries() {
        // Valve's own builds, and anything dropped into compatibilitytools.d (GE-Proton
        // and friends, which is where a lot of players actually run from).
        for dir in [
            root.join("steamapps").join("common"),
            root.join("compatibilitytools.d"),
        ] {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains("proton") && !found.contains(&path) {
                    found.push(path);
                }
            }
        }
    }
    found.sort_by_key(|p| {
        std::cmp::Reverse(proton_rank(&p.file_name().unwrap_or_default().to_string_lossy()))
    });
    found
}

/// How much we'd like to use a Proton, by its folder name. Higher wins.
///
/// A guess, and only ever used when Steam's own record of which Proton built the prefix
/// has already failed us — so it aims at "the one a player most likely runs games with"
/// rather than at precision. Experimental first (it is what Steam defaults to and is
/// always current), then the highest version number, with a name that carries no number
/// at the bottom rather than treated as version zero.
fn proton_rank(name: &str) -> (u32, u32, u32) {
    let lower = name.to_lowercase();
    if lower.contains("experimental") {
        return (u32::MAX, 0, 0);
    }
    // First number in the name is the major version: `Proton 9.0`, `GE-Proton9-20`.
    let mut nums = name
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok());
    (0, nums.next().unwrap_or(0), nums.next().unwrap_or(0))
}

/// A host path as the prefix sees it.
///
/// Two shapes, and which one applies is a fact about the path rather than a preference:
/// anything under the prefix's own `drive_c` is on `C:`, and everything else on the
/// machine is reachable as `Z:`, the drive Wine maps to `/`. FrostMod is handed Windows
/// paths (`--mods`) because it is a Windows program; it has no idea either of these
/// started life with forward slashes.
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

/// Is a process on this machine running `exe`?
///
/// `/proc` rather than `ps`: the AppImage carries no `ps`, and reading the process table
/// directly is both cheaper and one less thing to be missing on a slim install. Under
/// Proton the game is an ordinary Linux process whose command line still names
/// `mxbikes.exe`, and so is FrostMod's launcher — which is the only handle we have on it
/// from out here, since its reload event lives inside the prefix where we can't reach.
pub fn running_exe(exe: &str) -> bool {
    !exe_pids(exe).is_empty()
}

/// Every process whose command line names `exe`, ours excluded.
///
/// More than one is normal and correct: Proton leaves its own wrapper around the program
/// it started, so `frostmod.exe` is named by both the wrapper and the Wine process doing
/// the work, and stopping FrostMod means stopping both.
pub fn exe_pids(exe: &str) -> Vec<u32> {
    let own = std::process::id();
    let mut pids = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else { return pids };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
        if pid == own {
            continue;
        }
        // Unreadable is normal — another user's process, or one that exited between the
        // listing and the read.
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else { continue };
        if cmdline_names_exe(&String::from_utf8_lossy(&cmdline), exe) {
            pids.push(pid);
        }
    }
    pids
}

/// Stop everything running `exe`. Best-effort, and TERM rather than KILL: FrostMod's
/// launcher tidies up after itself when asked politely, and a Wine process killed outright
/// can leave the prefix's server holding its handles.
///
/// Shells out because the app links no libc of its own for a single `kill(2)`. `kill` is
/// part of a base install everywhere this could plausibly run.
pub fn kill_exe(exe: &str) {
    let pids = exe_pids(exe);
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

/// Does one `/proc/<pid>/cmdline` name `exe`?
///
/// The arguments are NUL-separated and the exe may appear as any of them, in either slash
/// form and any case — `Z:\home\…\frostmod.exe`, `/…/common/MX Bikes/mxbikes.exe`. So the
/// test is a case-insensitive substring, which is what makes it work through however many
/// wrappers Steam and Proton put in front of the process.
fn cmdline_names_exe(cmdline: &str, exe: &str) -> bool {
    cmdline.to_ascii_lowercase().contains(&exe.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_inside_the_prefix_is_on_c_drive() {
        let prefix = Path::new("/home/rider/.steam/steam/steamapps/compatdata/655500/pfx");
        let mods = prefix.join("drive_c/users/steamuser/Documents/PiBoSo/MX Bikes/mods");
        assert_eq!(
            windows_path(prefix, &mods),
            "C:\\users\\steamuser\\Documents\\PiBoSo\\MX Bikes\\mods"
        );
    }

    /// Everything else on the machine is still reachable — Wine maps `/` to `Z:` — which
    /// is how the app's own folder (where it leaves commands for FrostMod) is nameable
    /// from inside the prefix at all.
    #[test]
    fn a_path_outside_the_prefix_is_on_z_drive() {
        let prefix = Path::new("/home/rider/.steam/steam/steamapps/compatdata/655500/pfx");
        assert_eq!(
            windows_path(prefix, Path::new("/home/rider/.local/share/com.frost.mxbikes/frostmod")),
            "Z:\\home\\rider\\.local\\share\\com.frost.mxbikes\\frostmod"
        );
    }

    /// Both layouts Proton has used. The point of reading this file at all is to run the
    /// build that *made* the prefix rather than the newest one installed: a different Wine
    /// build against the same prefix can upgrade it in place.
    #[test]
    fn config_info_names_the_proton_that_built_the_prefix() {
        let modern = "9.0-303\n/home/rider/.steam/steam/steamapps/common/Proton 9.0/files/share/fonts/\n";
        assert_eq!(
            proton_root_in_config_info(modern).unwrap(),
            PathBuf::from("/home/rider/.steam/steam/steamapps/common/Proton 9.0")
        );

        let older = "5.13-6\n/home/rider/.steam/steam/steamapps/common/Proton 5.13/dist/share/default_pfx/\n";
        assert_eq!(
            proton_root_in_config_info(older).unwrap(),
            PathBuf::from("/home/rider/.steam/steam/steamapps/common/Proton 5.13")
        );

        // A file that names neither is one we learn nothing from — the fallback scan is
        // the answer there, not a half-parsed path.
        assert!(proton_root_in_config_info("9.0-303\n").is_none());
        assert!(proton_root_in_config_info("").is_none());
    }

    /// The fallback ordering, for when `config_info` is gone. Experimental is what Steam
    /// itself defaults to; after that a bigger version number is the better guess.
    #[test]
    fn experimental_outranks_a_numbered_proton_and_numbers_sort_numerically() {
        assert!(proton_rank("Proton - Experimental") > proton_rank("Proton 9.0"));
        assert!(proton_rank("Proton 10.0") > proton_rank("Proton 9.0"));
        assert!(proton_rank("GE-Proton9-20") > proton_rank("GE-Proton8-32"));
        // The trap a string sort falls into.
        assert!(proton_rank("Proton 10.0") > proton_rank("Proton 5.13"));
    }

    #[test]
    fn a_command_line_names_the_exe_whatever_wraps_it() {
        // Proton's own argv for the game, wrappers and all.
        let game = "/home/rider/.steam/steam/steamapps/common/Proton - Experimental/proton\0waitforexitandrun\0/home/rider/.steam/steam/steamapps/common/MX Bikes/mxbikes.exe\0";
        assert!(cmdline_names_exe(game, "mxbikes.exe"));
        assert!(!cmdline_names_exe(game, "gpbikes.exe"));
        // FrostMod as we start it: a Windows path, backslashes and all.
        let frostmod = "…/proton\0run\0Z:\\home\\rider\\.local\\share\\com.frost.mxbikes\\frostmod\\FrostMod.exe\0--game\0mxb\0";
        assert!(cmdline_names_exe(frostmod, "frostmod.exe"));
        assert!(!cmdline_names_exe("", "frostmod.exe"));
    }

    #[test]
    fn proton_takes_the_run_verb_and_a_custom_runner_takes_the_exe() {
        let runner = Runner {
            compat_data: PathBuf::from("/steam/steamapps/compatdata/655500"),
            program: PathBuf::from("/steam/steamapps/common/Proton 9.0/proton"),
            client: PathBuf::from("/steam"),
            kind: RunnerKind::Proton,
        };
        let cmd = runner.command(Path::new("/data/frostmod/frostmod.exe"), &["--game".into(), "mxb".into()]);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["run", "/data/frostmod/frostmod.exe", "--game", "mxb"]);
        let env: Vec<_> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        assert!(env.contains(&(
            "STEAM_COMPAT_DATA_PATH".to_string(),
            Some("/steam/steamapps/compatdata/655500".to_string())
        )));
        assert!(!env.iter().any(|(k, _)| k == "WINEPREFIX"), "Proton is told the compat path, not the prefix");

        let custom = Runner { kind: RunnerKind::Custom, ..runner };
        let cmd = custom.command(Path::new("/data/frostmod/frostmod.exe"), &[]);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["/data/frostmod/frostmod.exe"]);
        let env: Vec<_> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        assert!(env.contains(&(
            "WINEPREFIX".to_string(),
            Some("/steam/steamapps/compatdata/655500/pfx".to_string())
        )));
    }
}
