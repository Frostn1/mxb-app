//! Uninstalling an app from its own Settings, for all three apps.
//!
//! Each platform removes an app its own way, and this only starts that. Nothing here writes a
//! registry key or removes a program file itself:
//!
//! - **Windows**: the NSIS installer Tauri builds (`installMode: currentUser`) leaves
//!   `uninstall.exe` beside the executable and registers it under
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\<productName>`. The
//!   uninstaller shows its own window and removes the Start-menu and registry entries.
//! - **macOS**: an app is a bundle. It goes to the Trash through Finder, so it can be restored.
//! - **Linux AppImage**: the AppImage is one file, so it's deleted.
//! - **Linux .deb/.rpm**: removing a system package needs root, so the rider gets the command
//!   to run and nothing is started.
//!
//! The work happens in a detached helper that waits for this process to exit first. While the
//! app runs, WebView2 holds its profile under the local data folder open, and the NSIS
//! uninstaller refuses to run over a program that's still running.
//!
//! "Also delete my data" is opt-in. It removes only folders named after this app's own bundle
//! identifier, never the game folder or another app's data. MXB App's identifier is special:
//! [`crate::config::DATA_ID`] is the folder Coach and Studio read their settings from too, so it's
//! only offered while neither of them is installed.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// The three apps, by product name and bundle identifier.
pub const APPS: [(&str, &str); 3] = [
    ("MXB App", "com.frost.mxbikes"),
    ("MXB Coach", "com.frost.mxbcoach"),
    ("Frost Studio", "com.frost.froststudio"),
];

/// How this install is removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// Run this uninstaller once the app has exited (Windows).
    Launch(PathBuf),
    /// Move this `.app` bundle to the Trash (macOS).
    TrashBundle(PathBuf),
    /// Delete this AppImage (Linux).
    DeleteFile(PathBuf),
    /// A system package: show this command, start nothing.
    Package(String),
    /// Nothing recognisable, e.g. a dev build run from `target/`.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        }
    }
}

/// What the running install looks like, gathered once so [`plan`] can be tested with fakes.
pub struct Probe<'a> {
    pub os: Os,
    pub exe: &'a Path,
    pub product: &'a str,
    /// `$APPIMAGE`, which the AppImage runtime sets to the file being run.
    pub appimage: Option<&'a Path>,
    /// The registered `UninstallString` for a product name, when there is one.
    pub registered: &'a dyn Fn(&str) -> Option<String>,
    pub exists: &'a dyn Fn(&Path) -> bool,
}

/// The package name Tauri gives a .deb and an .rpm: the product name, kebab-cased.
pub fn package_name(product: &str) -> String {
    product
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

/// The path inside an `UninstallString`, which NSIS writes quoted.
fn uninstall_path(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let path = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => command.split(" /").next()?,
    };
    (!path.trim().is_empty()).then(|| PathBuf::from(path.trim()))
}

pub fn plan(p: &Probe) -> Method {
    match p.os {
        Os::Windows => {
            if let Some(dir) = p.exe.parent() {
                let beside = dir.join("uninstall.exe");
                if (p.exists)(&beside) {
                    return Method::Launch(beside);
                }
            }
            (p.registered)(p.product)
                .as_deref()
                .and_then(uninstall_path)
                .filter(|path| (p.exists)(path))
                .map_or(Method::Unknown, Method::Launch)
        }
        Os::Mac => p
            .exe
            .ancestors()
            .find(|a| a.extension().is_some_and(|e| e == "app"))
            .map_or(Method::Unknown, |bundle| Method::TrashBundle(bundle.to_path_buf())),
        Os::Linux => {
            if let Some(image) = p.appimage.filter(|path| (p.exists)(path)) {
                return Method::DeleteFile(image.to_path_buf());
            }
            if !p.exe.starts_with("/usr") {
                return Method::Unknown;
            }
            let pkg = package_name(p.product);
            let dpkg = Path::new("/var/lib/dpkg/info").join(format!("{pkg}.list"));
            Method::Package(if (p.exists)(&dpkg) {
                format!("sudo apt remove {pkg}")
            } else {
                format!("sudo dnf remove {pkg}")
            })
        }
    }
}

/// This app's own folders: settings, data, local data, cache and logs, the ones that exist.
/// Each must name `identifier` somewhere in its path. A folder resolved to anything else is
/// dropped rather than deleted.
pub fn own_dirs(candidates: &[PathBuf], identifier: &str, exists: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in candidates {
        let named = dir.components().any(|c| c.as_os_str() == identifier);
        if named && exists(dir) && !out.iter().any(|kept| dir.starts_with(kept)) {
            out.retain(|kept| !kept.starts_with(dir));
            out.push(dir.clone());
        }
    }
    out
}

/// The other apps that read this app's data folder, when deleting it would reset them.
///
/// Only [`crate::config::DATA_ID`] is shared, so only MXB App is ever blocked.
pub fn sharing_with(identifier: &str, installed: &dyn Fn(&str, &str) -> bool) -> Vec<String> {
    if identifier != crate::config::DATA_ID {
        return Vec::new();
    }
    APPS.iter()
        .filter(|(_, id)| *id != identifier)
        .filter(|(name, id)| installed(name, id))
        .map(|(name, _)| name.to_string())
        .collect()
}

// ───────────────────────────── the helper that runs after exit ─────────────────────────────

/// The detached helper's program and arguments: wait for `pid` to exit, delete `dirs`, then
/// carry out `method`. Paths go in as arguments (or PowerShell literals), never pasted into
/// a shell line unquoted.
pub fn helper(os: Os, pid: u32, method: &Method, dirs: &[PathBuf]) -> Option<(String, Vec<String>)> {
    match os {
        Os::Windows => {
            let lit = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "''"));
            let mut script = format!("Wait-Process -Id {pid} -ErrorAction SilentlyContinue; ");
            if !dirs.is_empty() {
                let list: Vec<String> = dirs.iter().map(|d| lit(d)).collect();
                script += &format!(
                    "Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue; ",
                    list.join(",")
                );
            }
            if let Method::Launch(exe) = method {
                script += &format!("Start-Process -FilePath {}", lit(exe));
            }
            Some((
                "powershell.exe".into(),
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-WindowStyle".into(),
                    "Hidden".into(),
                    "-Command".into(),
                    script,
                ],
            ))
        }
        Os::Mac | Os::Linux => {
            // `$1` is the pid, `$2` the target (or empty), the rest the folders.
            let action = match method {
                Method::TrashBundle(_) => {
                    "[ -n \"$T\" ] && osascript -e 'on run argv' -e 'tell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)' -e 'end run' \"$T\""
                }
                Method::DeleteFile(_) => "[ -n \"$T\" ] && rm -f -- \"$T\"",
                _ => ":",
            };
            let script = format!(
                "P=$1; T=$2; shift 2; while kill -0 \"$P\" 2>/dev/null; do sleep 0.2; done; \
                 for d in \"$@\"; do rm -rf -- \"$d\"; done; {action}"
            );
            let target = match method {
                Method::TrashBundle(p) | Method::DeleteFile(p) => p.display().to_string(),
                _ => String::new(),
            };
            let mut args = vec!["-c".into(), script, "uninstall".into(), pid.to_string(), target];
            args.extend(dirs.iter().map(|d| d.display().to_string()));
            Some(("/bin/sh".into(), args))
        }
    }
}

// ───────────────────────────── gathering the facts ─────────────────────────────

#[cfg(windows)]
fn registry_uninstall_string(product: &str) -> Option<String> {
    use std::os::raw::c_void;

    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const HKEY_LOCAL_MACHINE: isize = -2147483646; // 0x80000002
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hkey: isize,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            typ: *mut u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let subkey = wide(&format!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{product}"));
    let value = wide("UninstallString");
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: a read-only registry query into a fixed stack buffer, length passed in bytes.
        let rc = unsafe {
            RegGetValueW(
                hive,
                subkey.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        };
        if rc == 0 {
            let chars = (len as usize / 2).saturating_sub(1);
            let s = String::from_utf16_lossy(&buf[..chars]);
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn registry_uninstall_string(_product: &str) -> Option<String> {
    None
}

/// Whether another app is installed on this machine: registered with Windows, a bundle in
/// an Applications folder, or its own data folder exists (created on its first run).
fn installed(name: &str, identifier: &str) -> bool {
    if registry_uninstall_string(name).is_some() {
        return true;
    }
    if cfg!(target_os = "macos") {
        let bundle = format!("{name}.app");
        let home = dirs_next::home_dir().map(|h| h.join("Applications").join(&bundle));
        if Path::new("/Applications").join(&bundle).exists() || home.is_some_and(|p| p.exists()) {
            return true;
        }
    }
    [dirs_next::config_dir(), dirs_next::data_dir(), dirs_next::data_local_dir()]
        .into_iter()
        .flatten()
        .any(|base| base.join(identifier).is_dir())
}

fn candidates(app: &AppHandle) -> Vec<PathBuf> {
    let p = app.path();
    [p.app_config_dir(), p.app_data_dir(), p.app_local_data_dir(), p.app_cache_dir(), p.app_log_dir()]
        .into_iter()
        .flatten()
        .collect()
}

fn method_for(app: &AppHandle) -> Method {
    let exe = std::env::current_exe().unwrap_or_default();
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let product = app.package_info().name.clone();
    plan(&Probe {
        os: Os::current(),
        exe: &exe,
        product: &product,
        appimage: appimage.as_deref(),
        registered: &registry_uninstall_string,
        exists: &|p: &Path| p.exists(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallInfo {
    pub product: String,
    /// `launch`, `trash`, `delete`, `package` or `none`.
    pub method: &'static str,
    /// The command for a system package.
    pub command: Option<String>,
    /// The folders "also delete my data" would remove.
    pub data_dirs: Vec<String>,
    /// Apps that share this app's data folder and are installed, which rules deleting it out.
    pub shared_with: Vec<String>,
    /// Windows: the NSIS uninstaller has its own "delete app data" box, which removes the same
    /// folder. For MXB App that's the shared one.
    pub nsis: bool,
}

#[tauri::command]
pub async fn uninstall_info(app: AppHandle) -> Result<UninstallInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let identifier = app.config().identifier.clone();
        let method = method_for(&app);
        let (method_tag, command) = match &method {
            Method::Launch(_) => ("launch", None),
            Method::TrashBundle(_) => ("trash", None),
            Method::DeleteFile(_) => ("delete", None),
            Method::Package(c) => ("package", Some(c.clone())),
            Method::Unknown => ("none", None),
        };
        UninstallInfo {
            product: app.package_info().name.clone(),
            method: method_tag,
            command,
            data_dirs: own_dirs(&candidates(&app), &identifier, &|p| p.exists())
                .iter()
                .map(|d| d.display().to_string())
                .collect(),
            shared_with: sharing_with(&identifier, &installed),
            nsis: matches!(method, Method::Launch(_)),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Start the helper and exit. `delete_data` is refused while another app shares the folder.
#[tauri::command]
pub async fn uninstall_app(app: AppHandle, delete_data: bool) -> Result<(), String> {
    let handle = app.clone();
    let (program, args) = tauri::async_runtime::spawn_blocking(move || {
        let identifier = handle.config().identifier.clone();
        let method = method_for(&handle);
        if matches!(method, Method::Package(_) | Method::Unknown) {
            return Err("This install can't remove itself; see the instructions shown.".to_string());
        }
        let dirs = if delete_data {
            let sharing = sharing_with(&identifier, &installed);
            if !sharing.is_empty() {
                return Err(format!("{} still use this app's data folder.", sharing.join(" and ")));
            }
            own_dirs(&candidates(&handle), &identifier, &|p| p.exists())
        } else {
            Vec::new()
        };
        helper(Os::current(), std::process::id(), &method, &dirs)
            .ok_or_else(|| "no uninstall helper for this platform".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let mut cmd = std::process::Command::new(&program);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    cmd.spawn().map_err(|e| format!("couldn't start the uninstaller: {e}"))?;
    log::info!("uninstall: helper started ({program}); exiting");
    crate::usage::flush_on_exit(&app);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe<'a>(os: Os, exe: &'a Path, reg: &'a dyn Fn(&str) -> Option<String>, exists: &'a dyn Fn(&Path) -> bool) -> Probe<'a> {
        Probe { os, exe, product: "MXB Coach", appimage: None, registered: reg, exists }
    }

    #[test]
    fn windows_runs_the_uninstaller_beside_the_exe() {
        let exe = Path::new(r"C:\Users\r\AppData\Local\MXB Coach\mxb-coach.exe");
        let none = |_: &str| None;
        let there = |p: &Path| p.ends_with("uninstall.exe");
        assert_eq!(
            plan(&probe(Os::Windows, exe, &none, &there)),
            Method::Launch(PathBuf::from(r"C:\Users\r\AppData\Local\MXB Coach\uninstall.exe"))
        );
    }

    #[test]
    fn windows_falls_back_to_the_registered_uninstaller() {
        let exe = Path::new(r"D:\portable\mxb-coach.exe");
        let reg = |name: &str| {
            (name == "MXB Coach").then(|| r#""C:\Programs\MXB Coach\uninstall.exe" /S"#.to_string())
        };
        let only_registered = |p: &Path| p == Path::new(r"C:\Programs\MXB Coach\uninstall.exe");
        assert_eq!(
            plan(&probe(Os::Windows, exe, &reg, &only_registered)),
            Method::Launch(PathBuf::from(r"C:\Programs\MXB Coach\uninstall.exe"))
        );
        // A registered uninstaller that's no longer on disk is no uninstaller.
        assert_eq!(plan(&probe(Os::Windows, exe, &reg, &|_| false)), Method::Unknown);
    }

    #[test]
    fn a_mac_app_goes_to_the_trash_as_a_bundle() {
        let exe = Path::new("/Applications/MXB Coach.app/Contents/MacOS/mxb-coach");
        assert_eq!(
            plan(&probe(Os::Mac, exe, &|_| None, &|_| true)),
            Method::TrashBundle(PathBuf::from("/Applications/MXB Coach.app"))
        );
        assert_eq!(plan(&probe(Os::Mac, Path::new("/tmp/target/debug/coach"), &|_| None, &|_| true)), Method::Unknown);
    }

    #[test]
    fn linux_deletes_an_appimage_and_only_describes_a_package() {
        let image = Path::new("/home/r/Apps/MXB_Coach.AppImage");
        let mut p = probe(Os::Linux, Path::new("/tmp/.mount_x/usr/bin/mxb-coach"), &|_| None, &|_| true);
        p.appimage = Some(image);
        assert_eq!(plan(&p), Method::DeleteFile(image.to_path_buf()));

        let exe = Path::new("/usr/bin/mxb-coach");
        let deb = |q: &Path| q == Path::new("/var/lib/dpkg/info/mxb-coach.list");
        assert_eq!(plan(&probe(Os::Linux, exe, &|_| None, &deb)), Method::Package("sudo apt remove mxb-coach".into()));
        assert_eq!(plan(&probe(Os::Linux, exe, &|_| None, &|_| false)), Method::Package("sudo dnf remove mxb-coach".into()));
        assert_eq!(package_name("Frost Studio"), "frost-studio");
    }

    /// Only folders named after the app are ever deleted, and a folder inside another is
    /// deleted once, as part of its parent.
    #[test]
    fn only_the_apps_own_folders_are_deleted() {
        let id = "com.frost.mxbcoach";
        let dirs = [
            PathBuf::from("/home/r/.config/com.frost.mxbcoach"),
            PathBuf::from("/home/r/.local/share/com.frost.mxbcoach"),
            PathBuf::from("/home/r/.local/share/com.frost.mxbcoach/logs"),
            PathBuf::from("/home/r/.cache/com.frost.mxbikes"),
            PathBuf::from("/home/r/Documents/mxbcoach"),
        ];
        assert_eq!(
            own_dirs(&dirs, id, &|_| true),
            [PathBuf::from("/home/r/.config/com.frost.mxbcoach"), PathBuf::from("/home/r/.local/share/com.frost.mxbcoach")]
        );
        assert!(own_dirs(&dirs, id, &|_| false).is_empty(), "nothing that isn't there");
    }

    #[test]
    fn mxb_apps_folder_is_kept_while_coach_or_studio_use_it() {
        let coach_only = |name: &str, _: &str| name == "MXB Coach";
        assert_eq!(sharing_with("com.frost.mxbikes", &coach_only), ["MXB Coach"]);
        assert!(sharing_with("com.frost.mxbikes", &|_, _| false).is_empty());
        // Coach's and Studio's own folders are theirs alone.
        assert!(sharing_with("com.frost.mxbcoach", &|_, _| true).is_empty());
        assert!(sharing_with("com.frost.froststudio", &|_, _| true).is_empty());
    }

    #[test]
    fn the_windows_helper_waits_then_deletes_then_uninstalls() {
        let dirs = [PathBuf::from(r"C:\Users\O'Neil\AppData\Roaming\com.frost.mxbcoach")];
        let (prog, args) = helper(Os::Windows, 42, &Method::Launch(PathBuf::from(r"C:\A\uninstall.exe")), &dirs).unwrap();
        assert_eq!(prog, "powershell.exe");
        let script = args.last().unwrap();
        let wait = script.find("Wait-Process -Id 42").unwrap();
        let remove = script.find("Remove-Item -LiteralPath 'C:\\Users\\O''Neil\\").unwrap();
        let start = script.find("Start-Process -FilePath 'C:\\A\\uninstall.exe'").unwrap();
        assert!(wait < remove && remove < start, "{script}");

        let (_, args) = helper(Os::Windows, 7, &Method::Launch(PathBuf::from(r"C:\A\uninstall.exe")), &[]).unwrap();
        assert!(!args.last().unwrap().contains("Remove-Item"), "no data deleted unless asked");
    }

    #[test]
    fn the_unix_helper_passes_paths_as_arguments() {
        let bundle = PathBuf::from("/Applications/MXB Coach.app");
        let dir = PathBuf::from("/Users/r/Library/Application Support/com.frost.mxbcoach");
        let (prog, args) = helper(Os::Mac, 9, &Method::TrashBundle(bundle.clone()), &[dir.clone()]).unwrap();
        assert_eq!(prog, "/bin/sh");
        assert_eq!(&args[2..], ["uninstall", "9", "/Applications/MXB Coach.app", dir.to_str().unwrap()]);
        assert!(!args[1].contains("MXB Coach"), "no path is pasted into the script");
        assert!(args[1].contains("Finder"));
    }
}
