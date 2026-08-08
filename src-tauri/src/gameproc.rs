use serde::Serialize;

use crate::config::AppConfig;

/// Customization loader `fcn.1400ecd00` minus PE image base `0x140000000`.
#[cfg(windows)]
const LOADER_OFFSET: usize = 0x000e_cd00;

/// The game's main executable (matched case-insensitively).
const GAME_EXE: &str = "mxbikes.exe";

// Non-Windows builds only construct `Unsupported`/`Disabled`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveRefresh {
    /// We re-ran the loader in the live game; the look should be live.
    Refreshed,
    /// The refresh was attempted but failed.
    Failed,
    /// MX Bikes isn't running, so there was nothing to refresh.
    GameNotRunning,
    /// The instant-refresh setting was off — we didn't try.
    Disabled,
    /// This platform can't do it (non-Windows dev builds).
    Unsupported,
}

#[cfg(windows)]
mod ffi {
    use std::os::raw::{c_char, c_void};

    pub type Handle = *mut c_void;
    pub const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

    pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    pub const TH32CS_SNAPMODULE: u32 = 0x0000_0008;
    pub const TH32CS_SNAPMODULE32: u32 = 0x0000_0010;

    // Access rights needed to spawn a remote thread at a known address.
    pub const PROCESS_CREATE_THREAD: u32 = 0x0002;
    pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    pub const PROCESS_VM_OPERATION: u32 = 0x0008;
    pub const PROCESS_VM_WRITE: u32 = 0x0020;
    pub const PROCESS_VM_READ: u32 = 0x0010;

    pub const WAIT_TIMEOUT_MS: u32 = 5_000;

    /// `SHQueryUserNotificationState` — a DirectX app owns the screen exclusively.
    /// Deliberately *not* `QUNS_BUSY` (2): a borderless-fullscreen game reports that
    /// too, and borderless is exactly the case the overlay works in.
    pub const QUNS_RUNNING_D3D_FULL_SCREEN: i32 = 3;

    /// `ShowWindow` — un-minimize without changing the restored size/position.
    pub const SW_RESTORE: i32 = 9;

    #[repr(C)]
    pub struct ProcessEntry32 {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_process_id: u32,
        pub th32_default_heap_id: usize,
        pub th32_module_id: u32,
        pub cnt_threads: u32,
        pub th32_parent_process_id: u32,
        pub pc_pri_class_base: i32,
        pub dw_flags: u32,
        pub sz_exe_file: [c_char; 260],
    }

    #[repr(C)]
    pub struct ModuleEntry32 {
        pub dw_size: u32,
        pub th32_module_id: u32,
        pub th32_process_id: u32,
        pub glbl_cnt_usage: u32,
        pub proc_cnt_usage: u32,
        pub mod_base_addr: *mut u8,
        pub mod_base_size: u32,
        pub h_module: Handle,
        pub sz_module: [c_char; 256],
        pub sz_exe_path: [c_char; 260],
    }

    extern "system" {
        pub fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        pub fn Process32First(snapshot: Handle, entry: *mut ProcessEntry32) -> i32;
        pub fn Process32Next(snapshot: Handle, entry: *mut ProcessEntry32) -> i32;
        pub fn Module32First(snapshot: Handle, entry: *mut ModuleEntry32) -> i32;
        pub fn Module32Next(snapshot: Handle, entry: *mut ModuleEntry32) -> i32;
        pub fn OpenProcess(desired_access: u32, inherit: i32, process_id: u32) -> Handle;
        pub fn CreateRemoteThread(
            process: Handle,
            attrs: *mut c_void,
            stack_size: usize,
            start: *mut c_void,
            param: *mut c_void,
            flags: u32,
            thread_id: *mut u32,
        ) -> Handle;
        pub fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
        pub fn CloseHandle(handle: Handle) -> i32;
    }

    /// `EnumWindows` callback: return 0 to stop the walk, non-zero to continue.
    pub type EnumWindowsProc = unsafe extern "system" fn(hwnd: Handle, lparam: isize) -> i32;

    #[repr(C)]
    #[derive(Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn EnumWindows(callback: EnumWindowsProc, lparam: isize) -> i32;
        pub fn GetWindowRect(hwnd: Handle, rect: *mut Rect) -> i32;
        pub fn GetWindowThreadProcessId(hwnd: Handle, process_id: *mut u32) -> u32;
        pub fn IsWindowVisible(hwnd: Handle) -> i32;
        pub fn IsIconic(hwnd: Handle) -> i32;
        pub fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
        pub fn SetForegroundWindow(hwnd: Handle) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        pub fn SHQueryUserNotificationState(state: *mut i32) -> i32;
    }

    /// Compare a fixed-size NUL-padded ANSI field against `name`, case-insensitively.
    pub fn field_eq_ignore_case(field: &[c_char], name: &str) -> bool {
        let bytes: Vec<u8> = field
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        std::str::from_utf8(&bytes)
            .map(|s| s.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    }
}

/// Find the PID of the running game, if any.
#[cfg(windows)]
fn find_game_pid() -> Option<u32> {
    // SAFETY: standard Toolhelp process walk; we close the snapshot handle before
    // returning and only read fields the API populated.
    unsafe {
        let snap = ffi::CreateToolhelp32Snapshot(ffi::TH32CS_SNAPPROCESS, 0);
        if snap == ffi::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: ffi::ProcessEntry32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<ffi::ProcessEntry32>() as u32;
        let mut pid = None;
        if ffi::Process32First(snap, &mut entry) != 0 {
            loop {
                if ffi::field_eq_ignore_case(&entry.sz_exe_file, GAME_EXE) {
                    pid = Some(entry.th32_process_id);
                    break;
                }
                if ffi::Process32Next(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        ffi::CloseHandle(snap);
        pid
    }
}

/// Runtime base address of `mxbikes.exe` in `pid` (retries transient snapshot failures).
#[cfg(windows)]
fn module_base(pid: u32) -> Option<*mut u8> {
    for _ in 0..8 {
        // SAFETY: module snapshot for a known pid; handle closed before return.
        let base = unsafe {
            let snap = ffi::CreateToolhelp32Snapshot(
                ffi::TH32CS_SNAPMODULE | ffi::TH32CS_SNAPMODULE32,
                pid,
            );
            if snap == ffi::INVALID_HANDLE_VALUE {
                None
            } else {
                let mut me: ffi::ModuleEntry32 = std::mem::zeroed();
                me.dw_size = std::mem::size_of::<ffi::ModuleEntry32>() as u32;
                let found = if ffi::Module32First(snap, &mut me) != 0 {
                    // The first module is always the process's own exe.
                    Some(me.mod_base_addr)
                } else {
                    None
                };
                ffi::CloseHandle(snap);
                found
            }
        };
        if base.is_some() {
            return base;
        }
    }
    None
}

/// Is MX Bikes currently running?
#[cfg(windows)]
pub fn is_game_running() -> bool {
    find_game_pid().is_some()
}

/// State threaded through the `EnumWindows` walk: the pid we want, the handle we found.
#[cfg(windows)]
struct WindowSearch {
    pid: u32,
    hwnd: Option<ffi::Handle>,
}

/// `EnumWindows` callback — keep the first visible top-level window owned by `pid`.
///
/// SAFETY: called by the OS with a live window handle and the `lparam` we passed to
/// `EnumWindows`, which is a pointer to a `WindowSearch` that outlives the walk.
#[cfg(windows)]
unsafe extern "system" fn collect_game_window(hwnd: ffi::Handle, lparam: isize) -> i32 {
    let search = &mut *(lparam as *mut WindowSearch);
    let mut pid = 0u32;
    ffi::GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == search.pid && ffi::IsWindowVisible(hwnd) != 0 {
        search.hwnd = Some(hwnd);
        return 0; // found it — stop walking
    }
    1
}

/// The game's main window handle, if the game is up and has drawn one.
#[cfg(windows)]
fn game_hwnd() -> Option<ffi::Handle> {
    let pid = find_game_pid()?;
    let mut search = WindowSearch { pid, hwnd: None };
    // SAFETY: `search` lives for the whole (synchronous) walk, and the callback only
    // ever dereferences the pointer we hand it here.
    unsafe {
        ffi::EnumWindows(collect_game_window, &mut search as *mut WindowSearch as isize);
    }
    search.hwnd
}

/// Hand keyboard focus back to MX Bikes after the overlay closes.
///
/// Windows only grants foreground rights to a process that "owns" the last input —
/// which we do, because the caller got here through our registered global hotkey.
/// Best-effort: a refused activation just leaves the player one alt-tab away.
#[cfg(windows)]
pub fn focus_game() -> bool {
    let Some(hwnd) = game_hwnd() else {
        return false;
    };
    // SAFETY: `hwnd` came from an `EnumWindows` walk in this same call; both calls
    // are read/activate-only and safe on a stale handle (they simply fail).
    unsafe {
        if ffi::IsIconic(hwnd) != 0 {
            ffi::ShowWindow(hwnd, ffi::SW_RESTORE);
        }
        ffi::SetForegroundWindow(hwnd) != 0
    }
}

/// Where the game's window sits on the desktop, in physical pixels
/// (`left, top, right, bottom`).
///
/// Used to put the overlay over the game rather than wherever the primary monitor is —
/// a triple-screen sim rig would otherwise get it on the wrong display.
#[cfg(windows)]
pub fn game_window_rect() -> Option<(i32, i32, i32, i32)> {
    let hwnd = game_hwnd()?;
    let mut rect = ffi::Rect::default();
    // SAFETY: `hwnd` is a live handle from the walk above; `GetWindowRect` only writes
    // the four ints of the `RECT` we own.
    let ok = unsafe { ffi::GetWindowRect(hwnd, &mut rect) != 0 };
    ok.then_some((rect.left, rect.top, rect.right, rect.bottom))
}

/// Is a DirectX app holding the screen in *exclusive* fullscreen right now?
///
/// Nothing can be drawn over that, overlay included — the player has to switch the
/// game to borderless/windowed. Advisory only: we still try to show the overlay, and
/// let the caller surface guidance alongside it.
#[cfg(windows)]
pub fn is_exclusive_fullscreen() -> bool {
    let mut state = 0i32;
    // SAFETY: shell32 writes one `QUNS_*` value through the pointer; a failed call
    // (non-zero HRESULT) leaves it at our initial 0, which matches no state.
    let hr = unsafe { ffi::SHQueryUserNotificationState(&mut state) };
    hr == 0 && state == ffi::QUNS_RUNNING_D3D_FULL_SCREEN
}

/// Experimental: re-run the game's profile-load routine in the live process. Best-effort.
#[cfg(windows)]
pub fn refresh_look() -> LiveRefresh {
    let Some(pid) = find_game_pid() else {
        return LiveRefresh::GameNotRunning;
    };
    let Some(base) = module_base(pid) else {
        return LiveRefresh::Failed;
    };

    let access = ffi::PROCESS_CREATE_THREAD
        | ffi::PROCESS_QUERY_INFORMATION
        | ffi::PROCESS_VM_OPERATION
        | ffi::PROCESS_VM_WRITE
        | ffi::PROCESS_VM_READ;

    // SAFETY: we open the process for thread creation, spawn a thread at the
    // resolved loader address, wait briefly, and close every handle we open. The
    // start address is the module base plus a fixed code offset within the exe.
    unsafe {
        let proc = ffi::OpenProcess(access, 0, pid);
        if proc.is_null() {
            return LiveRefresh::Failed;
        }
        let start = base.add(LOADER_OFFSET) as *mut std::os::raw::c_void;
        let thread = ffi::CreateRemoteThread(
            proc,
            std::ptr::null_mut(),
            0,
            start,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
        );
        let outcome = if thread.is_null() {
            LiveRefresh::Failed
        } else {
            ffi::WaitForSingleObject(thread, ffi::WAIT_TIMEOUT_MS);
            ffi::CloseHandle(thread);
            LiveRefresh::Refreshed
        };
        ffi::CloseHandle(proc);
        outcome
    }
}

#[cfg(not(windows))]
pub fn is_game_running() -> bool {
    false
}

#[cfg(not(windows))]
pub fn refresh_look() -> LiveRefresh {
    LiveRefresh::Unsupported
}

/// No game to focus on a dev machine — the overlay just stays a normal window.
#[cfg(not(windows))]
pub fn focus_game() -> bool {
    false
}

/// Only Windows has an exclusive-fullscreen mode to be blocked by.
#[cfg(not(windows))]
pub fn is_exclusive_fullscreen() -> bool {
    false
}

/// No game window to sit over — the overlay stays where Tauri centred it.
#[cfg(not(windows))]
pub fn game_window_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

/// What happened when the user pressed Play.
// macOS dev builds only ever construct `AlreadyRunning` (the launch bails first).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchOutcome {
    /// We started the game.
    Launched,
    /// MX Bikes was already up — we left it alone rather than spawning a second copy.
    AlreadyRunning,
}

/// The MX Bikes install folder and its `mxbikes.exe`, for launching.
///
/// Falls back to Steam detection when `gamePath` is blank: the setting only ever gets
/// filled by that same detector, so an install we can find is one we can launch even if
/// the config predates the setting.
fn resolve_exe(cfg: &AppConfig) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = match cfg.game_path.trim() {
        "" => crate::config::detect_game_path().ok_or_else(|| {
            anyhow::anyhow!(
                "MX Bikes install folder isn't set — set it in Settings, under MX Bikes install folder."
            )
        })?,
        p => p.to_string(),
    };
    let dir = std::path::PathBuf::from(dir);
    let exe = crate::library::resolve_child(&dir, GAME_EXE);
    if !exe.is_file() {
        anyhow::bail!(
            "Couldn't find {GAME_EXE} in {} — check the MX Bikes install folder in Settings.",
            dir.display()
        );
    }
    Ok((dir, exe))
}

/// Start MX Bikes, unless it's already running.
///
/// Windows runs the exe directly rather than going through `steam://`: that works for
/// standalone (non-Steam) copies too, and doesn't need Steam to be up. Under Proton the
/// exe isn't ours to spawn — Steam has to set up the prefix — so Linux hands the Steam
/// URL to the desktop instead.
pub fn launch(cfg: &AppConfig) -> anyhow::Result<LaunchOutcome> {
    if is_game_running() {
        return Ok(LaunchOutcome::AlreadyRunning);
    }
    let (dir, exe) = resolve_exe(cfg)?;
    log::info!("launching MX Bikes: {}", exe.display());

    #[cfg(windows)]
    {
        // No CREATE_NO_WINDOW here (unlike the headless FrostMod child) — the game
        // draws its own window and hiding the console would gain nothing.
        std::process::Command::new(&exe)
            .current_dir(&dir)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Couldn't start {}: {e}", exe.display()))?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(target_os = "linux")]
    {
        let _ = dir;
        let url = format!("steam://rungameid/{}", crate::config::MX_BIKES_APPID);
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Couldn't ask Steam to launch MX Bikes ({url}): {e}"))?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = dir;
        anyhow::bail!("Launching MX Bikes is supported on Windows and Linux only")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("frost-launch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_the_exe_in_the_configured_folder() {
        let dir = temp_dir("found");
        std::fs::write(dir.join(GAME_EXE), b"stub").unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = dir.to_string_lossy().into_owned();
        let (found_dir, exe) = resolve_exe(&cfg).expect("a folder holding mxbikes.exe launches");
        assert_eq!(found_dir, dir);
        assert_eq!(exe, dir.join(GAME_EXE));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Proton installs on a case-sensitive filesystem can hold `MXBikes.exe`; an exact
    /// match would call a perfectly good install missing.
    #[test]
    fn matches_the_exe_case_insensitively() {
        let dir = temp_dir("case");
        std::fs::write(dir.join("MXBikes.exe"), b"stub").unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = dir.to_string_lossy().into_owned();
        // The name it comes back under depends on the host filesystem's case rules
        // (macOS resolves the exact join); what matters is that it resolves at all.
        let (_, exe) = resolve_exe(&cfg).expect("a differently-cased exe still resolves");
        assert!(exe
            .file_name()
            .unwrap()
            .to_string_lossy()
            .eq_ignore_ascii_case(GAME_EXE));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_the_exe_says_where_to_fix_it() {
        let dir = temp_dir("empty");

        let mut cfg = AppConfig::default();
        cfg.game_path = dir.to_string_lossy().into_owned();
        let err = resolve_exe(&cfg).expect_err("no exe means no launch");
        let msg = format!("{err:#}");
        assert!(msg.contains(GAME_EXE), "names what's missing: {msg}");
        assert!(msg.contains("Settings"), "points at the fix: {msg}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
