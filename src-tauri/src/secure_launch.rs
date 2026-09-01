//! Arming secure content in the running game — the app half of the injected client, so a
//! player does none of it by hand.
//!
//! When you lock a file and bind it to your account, the app remembers the mapping (below).
//! When the game starts, this writes the manifest next to `mxbsecure.dll` and injects the DLL
//! into the running process. No environment to set, no manifest to write, no `inject.exe`.
//!
//! Injection is into the **running** game rather than at launch, because MX Bikes is usually
//! started by Steam and we don't create that process. The DLL goes in shortly after the game
//! appears — at the menu, before a track is loaded — which is in time for the reads that
//! matter. The DLL reads its manifest from its own directory (no inherited environment), which
//! is why the manifest is written there.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// One secured asset the app knows how to serve: the name the game opens, the blob, and the
/// key sealed to the buyer (`.mxbkey`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureAsset {
    /// The file name the game opens (e.g. `pinehill.pkz`).
    pub game_name: String,
    pub blob_path: String,
    pub mxbkey_path: String,
}

/// Where the registry of secured assets lives.
fn registry_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_local_data_dir().ok()?.join("secure_assets.json"))
}

/// The assets provisioned on this machine, or an empty list.
pub fn load_assets(app: &AppHandle) -> Vec<SecureAsset> {
    registry_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// The secured assets actually present on disk: every `*.mxbsecure` under the tracks tree
/// that has a sibling `.mxbkey`. This is how a **buyer** works with no provisioning — they drop
/// the two files into their tracks folder and the manifest is built from what's there, the same
/// way MX Bikes discovers tracks by scanning. Anything provisioned on this machine is folded in.
pub fn scan_secured(app: &AppHandle) -> Vec<SecureAsset> {
    let mut found: Vec<SecureAsset> = Vec::new();
    if let Ok(cfg) = crate::config::load(app) {
        let tracks = crate::library::mods_subdir(&cfg.mods_path, "mods/tracks");
        collect_mxbsecure(&tracks, &mut found);
    }
    for a in load_assets(app) {
        if !found.iter().any(|f| f.blob_path.eq_ignore_ascii_case(&a.blob_path)) {
            found.push(a);
        }
    }
    found
}

/// Walk `dir` for `<name>.mxbsecure` blobs that have a `<name>.mxbsecure.mxbkey` beside them,
/// pushing a [`SecureAsset`] for each. Recursive, because tracks live in sub-folders.
fn collect_mxbsecure(dir: &std::path::Path, out: &mut Vec<SecureAsset>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mxbsecure(&path, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with(".mxbsecure") {
            continue; // its `.mxbkey` sibling ends in .mxbkey, so it's skipped here
        }
        let mxbkey = format!("{}.mxbkey", path.to_string_lossy());
        if !std::path::Path::new(&mxbkey).exists() {
            continue; // a blob with no key can't be opened — don't list it
        }
        out.push(SecureAsset {
            game_name: name.trim_end_matches(".mxbsecure").to_string(),
            blob_path: path.to_string_lossy().to_string(),
            mxbkey_path: mxbkey,
        });
    }
}

/// Record a newly provisioned asset, replacing any earlier entry for the same game name so a
/// re-lock doesn't leave two. Only the full (mxbsecure) build provisions, so it is otherwise
/// unused.
#[cfg_attr(not(mxbsecure), allow(dead_code))]
pub fn record_asset(app: &AppHandle, asset: SecureAsset) -> Result<(), String> {
    let path = registry_path(app).ok_or("no app data dir")?;
    let mut assets = load_assets(app);
    assets.retain(|a| !a.game_name.eq_ignore_ascii_case(&asset.game_name));
    assets.push(asset);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_vec_pretty(&assets).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Where the shipped `mxbsecure.dll` is found, in priority order: an explicit override, the
/// Tauri resource dir (a packaged build bundles it there), then beside the app's own
/// executable (a dev build's `build.rs` copies it there). The build places it, so there is
/// nothing to configure.
fn source_dll(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MXB_SECURE_DLL") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("mxbsecure.dll");
        if p.exists() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("mxbsecure.dll");
    p.exists().then_some(p)
}

/// The writable directory the DLL is run from — `<app-data>/secure/`. The shipped DLL may sit
/// in a read-only place (a packaged app's resource dir under Program Files), and the DLL needs
/// to read a `manifest.tsv` written beside it, so it is staged here where both can live.
fn run_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_local_data_dir().ok()?.join("secure");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Stage the DLL into the run dir, copying it only when it isn't already the same bytes — so a
/// running game holding the previous copy open doesn't block a launch, but an updated DLL does
/// replace it.
fn stage_dll(app: &AppHandle, run_dir: &std::path::Path) -> Result<PathBuf, String> {
    let src = source_dll(app).ok_or("no mxbsecure.dll shipped with the app")?;
    let dst = run_dir.join("mxbsecure.dll");
    let same = std::fs::metadata(&dst)
        .ok()
        .zip(std::fs::metadata(&src).ok())
        .map(|(a, b)| a.len() == b.len())
        .unwrap_or(false);
    if !same {
        std::fs::copy(&src, &dst).map_err(|e| format!("staging the DLL: {e}"))?;
    }
    Ok(dst)
}

/// Write the manifest the DLL reads — `manifest.tsv` in the run dir, one tab-separated line per
/// asset: game name, blob, `.mxbkey`.
fn write_manifest(assets: &[SecureAsset], dir: &std::path::Path) -> Result<(), String> {
    let mut out = String::new();
    for a in assets {
        out.push_str(&format!("{}\t{}\t{}\n", a.game_name, a.blob_path, a.mxbkey_path));
    }
    std::fs::write(dir.join("manifest.tsv"), out).map_err(|e| e.to_string())
}

/// Watch for the game and inject the moment it appears — as early as possible, because MX
/// Bikes reads a track's content to list it, so the hook has to be live *before* the track
/// browser opens or a protected track won't show. A tight poll (every couple of seconds)
/// rather than the 15-second session watcher, and it injects once per run of the game.
///
/// A no-op run to run when nothing is protected: it only wakes to check a process list.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut injected_this_session = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let running = crate::gameproc::is_game_running();
            if running {
                if !injected_this_session && !scan_secured(&app).is_empty() {
                    arm(&app);
                    injected_this_session = true;
                }
            } else {
                // Game gone: re-arm for the next launch.
                injected_this_session = false;
            }
        }
    });
}

/// Arm secure content for the session that just started: stage the DLL, write the manifest
/// beside it, and inject. Best-effort and quiet on the common "nothing to secure" — a player
/// with no locked content should see no trace of this.
pub fn arm(app: &AppHandle) {
    let assets = scan_secured(app);
    if assets.is_empty() {
        return;
    }
    let Some(dir) = run_dir(app) else {
        log::warn!("[secure] no writable run dir for secured content");
        return;
    };
    let dll = match stage_dll(app, &dir) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[secure] {e} (have {} secured asset(s))", assets.len());
            return;
        }
    };
    if let Err(e) = write_manifest(&assets, &dir) {
        log::warn!("[secure] couldn't write the manifest: {e}");
        return;
    }
    match inject(&dll) {
        Ok(()) => log::info!("[secure] injected mxbsecure.dll for {} asset(s)", assets.len()),
        Err(e) => log::warn!("[secure] injection failed: {e}"),
    }
}

/// Inject `dll` into the running game.
#[cfg(windows)]
fn inject(dll: &std::path::Path) -> Result<(), String> {
    let pid = crate::gameproc::game_pid().ok_or("the game isn't running")?;
    win::inject_into(pid, dll)
}

#[cfg(not(windows))]
fn inject(_dll: &std::path::Path) -> Result<(), String> {
    Err("injection is Windows-only".into())
}

#[cfg(windows)]
mod win {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    const PROCESS_ACCESS: u32 = 0x0002 | 0x0008 | 0x0010 | 0x0020 | 0x0400; // VM ops + create thread + query
    const MEM_COMMIT_RESERVE: u32 = 0x1000 | 0x2000;
    const PAGE_READWRITE: u32 = 0x04;

    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
        fn VirtualAllocEx(p: *mut c_void, addr: *mut c_void, size: usize, typ: u32, prot: u32) -> *mut c_void;
        fn WriteProcessMemory(p: *mut c_void, addr: *mut c_void, buf: *const c_void, size: usize, wrote: *mut usize) -> i32;
        fn CreateRemoteThread(p: *mut c_void, attr: *mut c_void, stack: usize, start: *mut c_void, param: *mut c_void, flags: u32, tid: *mut u32) -> *mut c_void;
        fn WaitForSingleObject(h: *mut c_void, ms: u32) -> u32;
        fn CloseHandle(h: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Load `dll` into process `pid` via the standard `LoadLibraryW` remote thread.
    pub fn inject_into(pid: u32, dll: &std::path::Path) -> Result<(), String> {
        let dll_w: Vec<u16> = wide(&dll.to_string_lossy());
        // SAFETY: a textbook remote-thread injection into a process we opened for it; every
        // handle is closed, and the one remote allocation holds only the DLL path string.
        unsafe {
            let proc = OpenProcess(PROCESS_ACCESS, 0, pid);
            if proc.is_null() {
                return Err(format!("OpenProcess({pid}) failed: {}", GetLastError()));
            }
            let bytes = dll_w.len() * 2;
            let remote = VirtualAllocEx(proc, null_mut(), bytes, MEM_COMMIT_RESERVE, PAGE_READWRITE);
            if remote.is_null() {
                CloseHandle(proc);
                return Err(format!("VirtualAllocEx failed: {}", GetLastError()));
            }
            if WriteProcessMemory(proc, remote, dll_w.as_ptr() as *const c_void, bytes, null_mut()) == 0 {
                CloseHandle(proc);
                return Err(format!("WriteProcessMemory failed: {}", GetLastError()));
            }
            let k32 = GetModuleHandleW(wide("kernel32.dll").as_ptr());
            let load = GetProcAddress(k32, c"LoadLibraryW".as_ptr());
            if load.is_null() {
                CloseHandle(proc);
                return Err("LoadLibraryW not found".into());
            }
            let thread = CreateRemoteThread(proc, null_mut(), 0, load, remote, 0, null_mut());
            if thread.is_null() {
                CloseHandle(proc);
                return Err(format!("CreateRemoteThread failed: {}", GetLastError()));
            }
            WaitForSingleObject(thread, 10_000);
            CloseHandle(thread);
            CloseHandle(proc);
        }
        Ok(())
    }
}
