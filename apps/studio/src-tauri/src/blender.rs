//! Driving the rider's own Blender, for the bike builder.
//!
//! A modder with a drawer full of bike parts in `.blend` files, and no feel for Blender, is
//! who this is for: Studio picks the parts and says where they go, and Blender does the
//! importing, placing and exporting it is already good at. We never ship Blender. We find
//! the one the rider installed, and run it headless — `blender -b` — one fresh process per
//! job, with our script and a job file:
//!
//! ```text
//! blender -b --factory-startup --python-exit-code 1 --python frost_bike.py -- job.json
//! ```
//!
//! A fresh process each time, rather than a live bridge into a running Blender, because a
//! job then either finishes or is killed, a crash costs one job and not the session, and
//! nothing depends on an add-on staying installed across Blender updates.
//!
//! `frost_bike.py` imports `bpy`, which Blender's licence treats as making it GPL. It lives
//! in `blender/` as its own file under GPL-3.0-or-later and is written out next to each job
//! as-is, so it is always there to read; nothing else in Studio touches Blender's code.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One Blender at a time. A job is minutes of CPU and a gigabyte of memory at worst, two at
/// once help nobody, and it keeps every job's files its own.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
/// Makes each job folder and probe file name unique within this process.
static SEQ: AtomicU64 = AtomicU64::new(0);

fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// A new, empty folder under `root` for one job, named `<kind>-<pid>-<n>`. Earlier ones of
/// the same kind are cleared out, so the cache holds the latest result and not a history.
/// Only ever called with [`ONE_AT_A_TIME`] held (see [`job`]): the clearing out must never
/// reach a folder a running job is using.
fn fresh_job_dir(root: &Path, kind: &str) -> PathBuf {
    let prefix = format!("{kind}-");
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    root.join(format!("{kind}-{}-{}", std::process::id(), next_seq()))
}

/// The script every job runs. See `blender/frost_bike.py` for what each op does.
const SCRIPT: &str = include_str!("../blender/frost_bike.py");
/// Placeholder parts and the Part Maker's templates, which `frost_bike.py` imports.
const MAKE_SCRIPT: &str = include_str!("../blender/frost_make.py");

/// The oldest Blender the script is written against: 4.2 LTS, which has `wm.obj_import` and
/// the FBX/glTF exporters with the options we pin.
pub const MIN_VERSION: (u32, u32, u32) = (4, 2, 0);

/// How long one job may take before it is killed. Importing a heavy `.blend` is the slow
/// part, and a minute is already far past anything a single part should need.
pub const JOB_TIMEOUT: Duration = Duration::from_secs(120);
/// `--version` answers in well under a second; this only guards against a hung install.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// A Blender we found, and whether it will do.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlenderInfo {
    pub path: String,
    /// "4.2.3", as Blender printed it.
    pub version: String,
    pub supported: bool,
}

// ---------------------------------------------------------------------------
// finding it

/// "Blender 4.2.3 LTS" → (4, 2, 3). The first line of `blender --version`; a build of the
/// main branch says "Blender 4.3.0 Alpha", which parses the same.
pub fn parse_version(stdout: &str) -> Option<(u32, u32, u32)> {
    let line = stdout.lines().find(|l| l.trim_start().starts_with("Blender "))?;
    let ver = line.trim_start().strip_prefix("Blender ")?.split_whitespace().next()?;
    let mut parts = ver.split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

pub fn is_supported(v: (u32, u32, u32)) -> bool {
    v >= MIN_VERSION
}

/// The executable a registry command line starts: `"C:\…\blender-launcher.exe" "%1"`.
///
/// The installer registers `blender-launcher.exe`, which exists to open a window without a
/// console and prints nothing, so the `blender.exe` beside it is what we want — when there
/// is one. The Microsoft Store's copy has only the launcher on the outside (the real exe
/// sits in a package Windows won't let anything start directly), and there the launcher is
/// the way in: it waits for the job and passes it through, it just prints nothing.
pub fn exe_from_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let first = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        command.split_whitespace().next()?
    };
    if first.is_empty() {
        return None;
    }
    let path = PathBuf::from(first);
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    if name == "blender-launcher.exe" {
        let beside = path.with_file_name("blender.exe");
        if beside.is_file() {
            return Some(beside);
        }
    }
    Some(path)
}

/// The value `reg query … /ve` printed: the text after `REG_SZ` / `REG_EXPAND_SZ`.
pub fn reg_default_value(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let l = l.trim();
        ["REG_EXPAND_SZ", "REG_SZ"].iter().find_map(|kind| {
            let at = l.find(kind)?;
            let v = l[at + kind.len()..].trim();
            (!v.is_empty()).then(|| v.to_string())
        })
    })
}

/// Blender's own folders under a Program Files: `Blender Foundation\Blender 4.2\blender.exe`,
/// newest version first so the best candidate is probed first.
fn installed_under(program_files: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(Option<(u32, u32, u32)>, PathBuf)> = std::fs::read_dir(
        program_files.join("Blender Foundation"),
    )
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(|e| {
        let exe = e.path().join("blender.exe");
        exe.is_file().then(|| {
            // The folder is named the way `--version` starts: "Blender 4.2".
            (parse_version(&e.file_name().to_string_lossy()), exe)
        })
    })
    .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Every place a Blender might be, most deliberate first: the one the rider picked, the
/// one `.blend` files open with, the installer's folders, Steam's copy, then `PATH`.
/// Candidates only — [`probe`] decides.
pub fn candidates(saved: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.iter().any(|q| q.as_os_str().eq_ignore_ascii_case(p.as_os_str())) {
            out.push(p);
        }
    };

    if !saved.trim().is_empty() {
        push(PathBuf::from(saved.trim()));
    }
    #[cfg(windows)]
    {
        for key in [
            r"HKCU\Software\Classes\blendfile\shell\open\command",
            r"HKLM\Software\Classes\blendfile\shell\open\command",
        ] {
            if let Some(exe) = reg_query_default(key).as_deref().and_then(exe_from_command) {
                push(exe);
            }
        }
        for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var) {
                for exe in installed_under(Path::new(&pf)) {
                    push(exe);
                }
            }
        }
        for lib in steam_libraries() {
            push(lib.join("steamapps").join("common").join("Blender").join("blender.exe"));
        }
        // The Microsoft Store's copy: only its launcher alias can be started.
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            push(Path::new(&local).join("Microsoft").join("WindowsApps").join("blender-launcher.exe"));
        }
    }
    #[cfg(not(windows))]
    {
        push(PathBuf::from("/Applications/Blender.app/Contents/MacOS/Blender"));
        push(PathBuf::from("/usr/bin/blender"));
        push(PathBuf::from("/snap/bin/blender"));
    }
    // Last, whatever `blender` on PATH resolves to (portable installs, package managers).
    if let Some(path) = std::env::var_os("PATH") {
        let name = if cfg!(windows) { "blender.exe" } else { "blender" };
        for dir in std::env::split_paths(&path) {
            let exe = dir.join(name);
            if exe.is_file() {
                push(exe);
            }
        }
    }
    out
}

#[cfg(windows)]
fn reg_query_default(key: &str) -> Option<String> {
    let mut cmd = Command::new("reg");
    cmd.args(["query", key, "/ve"]).stdin(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    out.status.success().then(|| reg_default_value(&String::from_utf8_lossy(&out.stdout)))?
}

/// Steam library roots, from `libraryfolders.vdf`: Blender is on Steam too.
#[cfg(windows)]
fn steam_libraries() -> Vec<PathBuf> {
    let Some(pf86) = std::env::var_os("ProgramFiles(x86)") else {
        return Vec::new();
    };
    let steam = Path::new(&pf86).join("Steam");
    let mut libs = vec![steam.clone()];
    if let Ok(vdf) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        libs.extend(vdf_library_paths(&vdf));
    }
    libs
}

/// The `"path"` values in a `libraryfolders.vdf`, unescaped.
pub fn vdf_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("\"path\"")?.trim();
            let v = rest.strip_prefix('"')?.strip_suffix('"')?;
            Some(PathBuf::from(v.replace("\\\\", "\\")))
        })
        .collect()
}

/// Ask a candidate what it is. `None` when it isn't there or isn't Blender.
///
/// `--version` first, which is instant. The Store's launcher prints nothing, so a candidate
/// that stays quiet is asked again the slow way: start it headless and have it write its
/// version to a file, which is also exactly how every real job answers.
pub fn probe(exe: &Path) -> Option<BlenderInfo> {
    // An App Execution Alias is a reparse point `is_file` can't follow; being there is enough.
    if std::fs::symlink_metadata(exe).is_err() {
        return None;
    }
    let mut cmd = Command::new(exe);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    no_window(&mut cmd);
    let out = run_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    let v = match parse_version(&out.stdout) {
        Some(v) => v,
        None => parse_version(&version_by_script(exe)?)?,
    };
    Some(BlenderInfo {
        path: exe.to_string_lossy().into_owned(),
        version: format!("{}.{}.{}", v.0, v.1, v.2),
        supported: is_supported(v),
    })
}

/// Blender's version as `bpy` reports it, written to a file by a headless run.
fn version_by_script(exe: &Path) -> Option<String> {
    let out = std::env::temp_dir().join(format!(
        "frost-blender-version-{}-{}.txt",
        std::process::id(),
        next_seq()
    ));
    let _ = std::fs::remove_file(&out);
    // A JSON string is a valid Python string literal, backslashes and quotes included.
    let target = serde_json::to_string(&out.to_string_lossy()).ok()?;
    let mut cmd = Command::new(exe);
    cmd.args(["-b", "--factory-startup", "--python-expr"])
        .arg(format!(
            "import bpy; open({target}, 'w', encoding='utf-8').write('Blender ' + bpy.app.version_string)"
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    no_window(&mut cmd);
    run_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    let text = std::fs::read_to_string(&out).ok();
    let _ = std::fs::remove_file(&out);
    text
}

/// The best Blender on this machine: the first supported candidate, else the first that
/// answered at all (so the UI can say "too old" rather than "not found").
pub fn detect(saved: &str) -> Option<BlenderInfo> {
    let mut fallback = None;
    for exe in candidates(saved) {
        if let Some(info) = probe(&exe) {
            if info.supported {
                return Some(info);
            }
            fallback.get_or_insert(info);
        }
    }
    fallback
}

// ---------------------------------------------------------------------------
// running a job

pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// How long to wait for the output after the process has gone. A Blender started through
/// the Store's launcher can outlive it holding the same pipes; its output is not worth a hang.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Run to completion or kill it at `limit`. Output is drained on threads, so a chatty Blender
/// can't fill a pipe and stall before we ever look, and waited for only briefly after exit.
fn run_with_timeout(mut cmd: Command, limit: Duration) -> anyhow::Result<Output> {
    use std::io::Read;
    use std::sync::mpsc;
    let mut child = cmd.spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
        });
        rx
    };
    let out = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > limit {
            kill_tree(&mut child);
            anyhow::bail!("Blender took longer than {}s and was stopped", limit.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(Output {
        stdout: out.recv_timeout(DRAIN_GRACE).unwrap_or_default(),
        stderr: err.recv_timeout(DRAIN_GRACE).unwrap_or_default(),
        success: status.success(),
    })
}

/// Stop a job and everything it started. The Store's launcher is only the front of it: the
/// Blender doing the work is its child, and killing the launcher alone leaves that running.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        no_window(&mut tk);
        let _ = tk.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Run one job of `kind` in a folder of its own under `root`. `make` is handed that folder
/// and returns the job, so the paths in it (exports, previews) point inside it.
///
/// The folder is chosen, the old ones cleared and the job run all under the one lock, so no
/// job's files are ever touched by another.
pub fn job(
    blender: &Path,
    root: &Path,
    kind: &str,
    make: impl FnOnce(&Path) -> serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    job_then(blender, root, kind, make, Ok)
}

/// [`job`], then `keep` the answer while the lock is still held: the next job of the same
/// kind clears this one's folder, so anything `keep` moves out of it has to go first.
pub fn job_then<R>(
    blender: &Path,
    root: &Path,
    kind: &str,
    make: impl FnOnce(&Path) -> serde_json::Value,
    keep: impl FnOnce(serde_json::Value) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    // A job that panicked while holding it poisoned nothing worth protecting.
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let work = fresh_job_dir(root, kind);
    let spec = make(&work);
    keep(run_job(blender, &work, spec)?)
}

/// Run one op of `frost_bike.py` in `work`, and hand back the JSON it wrote. Callers go
/// through [`job`], which holds the lock.
///
/// The job is `{ "op": …, …, "result": "<work>/result.json" }`: the script answers through
/// that file rather than stdout, which Blender also prints its own chatter to. A job that
/// wrote no result failed, and the last lines of its output say why.
fn run_job(
    blender: &Path,
    work: &Path,
    mut job: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    std::fs::create_dir_all(work)?;
    let script = work.join("frost_bike.py");
    std::fs::write(&script, SCRIPT)?;
    std::fs::write(work.join("frost_make.py"), MAKE_SCRIPT)?;
    let result = work.join("result.json");
    let _ = std::fs::remove_file(&result);
    job["result"] = serde_json::Value::String(result.to_string_lossy().into_owned());
    let job_path = work.join("job.json");
    std::fs::write(&job_path, serde_json::to_vec_pretty(&job)?)?;

    let mut cmd = Command::new(blender);
    cmd.args(["-b", "--factory-startup", "--python-exit-code", "1", "--python"])
        .arg(&script)
        .arg("--")
        .arg(&job_path)
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_window(&mut cmd);
    let out = run_with_timeout(cmd, JOB_TIMEOUT)?;

    match std::fs::read(&result) {
        Ok(bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
                anyhow::bail!("{err}");
            }
            Ok(value)
        }
        Err(_) => anyhow::bail!(
            "Blender {} without an answer:\n{}",
            if out.success { "finished" } else { "failed" },
            tail(&format!("{}\n{}", out.stdout, out.stderr), 12)
        ),
    }
}

/// The last `n` non-empty lines, for an error a person can act on.
fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Windows: no console flashing up behind Studio for a process nobody looks at.
#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_read_the_way_blender_prints_them() {
        assert_eq!(parse_version("Blender 4.2.3 LTS\n\tbuild date: 2024-10-15\n"), Some((4, 2, 3)));
        assert_eq!(parse_version("Blender 4.3.0 Alpha\n"), Some((4, 3, 0)));
        assert_eq!(parse_version("Blender 5.0\n"), Some((5, 0, 0)));
        // Some builds print a warning first.
        assert_eq!(parse_version("Color management: ...\nBlender 4.5.1\n"), Some((4, 5, 1)));
        assert_eq!(parse_version("not blender"), None);
        assert!(is_supported((4, 2, 0)) && is_supported((5, 1, 0)));
        assert!(!is_supported((4, 1, 9)) && !is_supported((3, 6, 12)));
    }

    #[test]
    fn the_launcher_is_swapped_for_the_real_exe() {
        // An installer's folder has blender.exe beside the launcher: that is what runs.
        let dir = std::env::temp_dir().join(format!("frost-blender-launcher-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("blender.exe"), b"").unwrap();
        let launcher = dir.join("blender-launcher.exe");
        assert_eq!(
            exe_from_command(&format!("\"{}\" \"%1\"", launcher.display())),
            Some(dir.join("blender.exe"))
        );
        let _ = std::fs::remove_dir_all(&dir);
        // The Store's alias has nothing beside it: the launcher is the way in.
        assert_eq!(
            exe_from_command(r#""Z:\WindowsApps\blender-launcher.exe" "%1""#),
            Some(PathBuf::from(r"Z:\WindowsApps\blender-launcher.exe"))
        );
        assert_eq!(
            exe_from_command(r#""D:\Tools\blender\blender.exe" "%1""#),
            Some(PathBuf::from(r"D:\Tools\blender\blender.exe"))
        );
        assert_eq!(exe_from_command(r"C:\blender\blender.exe %1"), Some(PathBuf::from(r"C:\blender\blender.exe")));
        assert_eq!(exe_from_command("  "), None);
    }

    #[test]
    fn a_reg_query_answer_gives_its_value() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Classes\\blendfile\\shell\\open\\command\r\n    (Default)    REG_SZ    \"C:\\B\\blender-launcher.exe\" \"%1\"\r\n\r\n";
        assert_eq!(reg_default_value(out).as_deref(), Some(r#""C:\B\blender-launcher.exe" "%1""#));
        assert_eq!(reg_default_value("ERROR: not found"), None);
    }

    #[test]
    fn steam_library_paths_are_unescaped() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(
            vdf_library_paths(vdf),
            vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\SteamLibrary")]
        );
    }

    #[test]
    fn install_folders_are_tried_newest_first() {
        let pf = std::env::temp_dir().join(format!("frost-blender-pf-{}", std::process::id()));
        for v in ["Blender 3.6", "Blender 4.2", "Blender 4.10"] {
            let d = pf.join("Blender Foundation").join(v);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("blender.exe"), b"").unwrap();
        }
        let found = installed_under(&pf);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Blender 4.10", "Blender 4.2", "Blender 3.6"]);
        let _ = std::fs::remove_dir_all(&pf);
    }

    #[test]
    fn the_saved_path_is_tried_first() {
        let c = candidates(r"E:\portable\blender.exe");
        assert_eq!(c.first(), Some(&PathBuf::from(r"E:\portable\blender.exe")));
    }

    #[test]
    fn a_missing_exe_is_not_blender() {
        assert_eq!(probe(Path::new(r"Z:\nowhere\blender.exe")), None);
    }

    /// The whole round trip on a real Blender: `cargo test blender -- --ignored` on a machine
    /// that has one. A hand-written OBJ cube, then a `.blend` Blender saves itself (a parent
    /// empty with a child mesh, the shape a bike part comes in), each imported and exported.
    #[test]
    #[ignore = "needs Blender installed"]
    fn a_part_goes_through_real_blender() {
        let blender = detect("").expect("a Blender on this machine");
        assert!(blender.supported, "{blender:?}");
        let exe = PathBuf::from(&blender.path);
        let root = std::env::temp_dir().join(format!("frost-blender-real-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        let obj = root.join("cube.obj");
        std::fs::write(
            &obj,
            "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\n\
             f 1 2 3 4\nf 5 8 7 6\nf 1 5 6 2\nf 2 6 7 3\nf 3 7 8 4\nf 5 1 4 8\n",
        )
        .unwrap();
        let got = job(&exe, &root, "obj", |w| {
            serde_json::json!({ "op": "inspect", "part": obj, "fbx": w.join("p.fbx"), "glb": w.join("p.glb") })
        })
        .expect("inspect an OBJ");
        assert_eq!(got["tris"], 12, "{got}");
        let fbx = PathBuf::from(got["fbx"].as_str().unwrap());
        assert!(fbx.is_file() && fbx.with_extension("glb").is_file());

        // The library's op: the same cube, catalogued with a thumbnail Blender renders.
        let got = job(&exe, &root, "catalog", |w| {
            serde_json::json!({ "op": "catalog", "part": obj, "thumb": w.join("t.png"), "thumbSize": 64, "glb": w.join("p.glb") })
        })
        .expect("catalog an OBJ");
        assert_eq!(got["tris"], 12, "{got}");
        assert!(got.get("thumbError").is_none(), "{got}");
        let thumb = std::fs::read(got["thumb"].as_str().unwrap()).expect("a thumbnail");
        assert!(thumb.starts_with(b"\x89PNG"), "the thumbnail is a PNG");
        assert!(PathBuf::from(got["glb"].as_str().unwrap()).is_file());

        // A .blend, made by Blender: `mount` (an empty) with `fender` (a mesh) under it.
        let blend = root.join("fender.blend");
        let target = serde_json::to_string(&blend.to_string_lossy()).unwrap();
        let status = Command::new(&exe)
            .args(["-b", "--factory-startup", "--python-expr"])
            .arg(format!(
                "import bpy\n\
                 bpy.ops.wm.read_factory_settings(use_empty=True)\n\
                 bpy.ops.object.empty_add(location=(0, 0.8, 0.6)); m = bpy.context.object; m.name = 'mount'\n\
                 bpy.ops.mesh.primitive_cube_add(size=0.4, location=(0, 1.0, 0.6)); f = bpy.context.object; f.name = 'fender'\n\
                 f.parent = m; f.matrix_parent_inverse = m.matrix_world.inverted()\n\
                 bpy.ops.wm.save_as_mainfile(filepath={target})\n"
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success() && blend.is_file(), "Blender saved the test part");
        let got = job(&exe, &root, "blend", |_| serde_json::json!({ "op": "inspect", "part": blend }))
            .expect("inspect a .blend");
        let names: Vec<&str> = got["objects"].as_array().unwrap().iter().filter_map(|o| o["name"].as_str()).collect();
        assert!(names.contains(&"mount") && names.contains(&"fender"), "{got}");
        let fender = got["objects"].as_array().unwrap().iter().find(|o| o["name"] == "fender").unwrap();
        assert_eq!(fender["parent"], "mount", "the hierarchy survives the append: {got}");
        assert_eq!(fender["tris"], 12);
        let got = job(&exe, &root, "catalog", |_| serde_json::json!({ "op": "catalog", "part": blend }))
            .expect("catalog a .blend");
        let empties = got["empties"].as_array().unwrap();
        assert_eq!(empties.len(), 1, "{got}");
        assert_eq!(empties[0]["name"], "mount");
        assert!((empties[0]["location"][1].as_f64().unwrap() - 0.8).abs() < 1e-4, "{got}");

        // And a part that isn't one answers with an error, not a hang.
        let err = job(&exe, &root, "bad", |_| serde_json::json!({ "op": "inspect", "part": root.join("x.png") }))
            .unwrap_err();
        assert!(format!("{err:#}").contains(".blend, .fbx or .obj"), "{err:#}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn each_job_gets_its_own_folder_and_old_ones_go() {
        let root = std::env::temp_dir().join(format!("frost-blender-jobs-{}", std::process::id()));
        let a = fresh_job_dir(&root, "inspect");
        std::fs::create_dir_all(&a).unwrap();
        let b = fresh_job_dir(&root, "inspect");
        assert_ne!(a, b);
        assert!(!a.exists(), "the previous inspection is cleared");
        std::fs::create_dir_all(root.join("build-1")).unwrap();
        let _ = fresh_job_dir(&root, "inspect");
        assert!(root.join("build-1").exists(), "other kinds are left alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn errors_keep_the_last_lines() {
        let text = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        assert_eq!(tail(&text, 2), "line 19\nline 20");
    }
}
