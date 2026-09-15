//! The game's process and window, as the overlays need them: is it running, where is its
//! window, give it focus back, and let a sibling app take the foreground.
//!
//! Windows does the real work; elsewhere these are stubs, since there is no game window to
//! sit over on a dev machine.

#[cfg(windows)]
mod ffi {
    use std::os::raw::{c_char, c_void};

    pub type Handle = *mut c_void;
    pub const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;

    /// `SHQueryUserNotificationState` — a DirectX app owns the screen exclusively.
    /// Not `QUNS_BUSY` (2): borderless fullscreen reports that too, and the overlay works there.
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
    #[derive(Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    /// `EnumWindows` callback: return 0 to stop the walk, non-zero to continue.
    pub type EnumWindowsProc = unsafe extern "system" fn(hwnd: Handle, lparam: isize) -> i32;

    extern "system" {
        pub fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        pub fn Process32First(snapshot: Handle, entry: *mut ProcessEntry32) -> i32;
        pub fn Process32Next(snapshot: Handle, entry: *mut ProcessEntry32) -> i32;
        pub fn CloseHandle(handle: Handle) -> i32;
        pub fn GetCurrentProcessId() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn EnumWindows(callback: EnumWindowsProc, lparam: isize) -> i32;
        pub fn GetForegroundWindow() -> Handle;
        pub fn GetWindowRect(hwnd: Handle, rect: *mut Rect) -> i32;
        pub fn GetWindowThreadProcessId(hwnd: Handle, process_id: *mut u32) -> u32;
        pub fn IsWindowVisible(hwnd: Handle) -> i32;
        pub fn IsIconic(hwnd: Handle) -> i32;
        pub fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
        pub fn SetForegroundWindow(hwnd: Handle) -> i32;
        pub fn AllowSetForegroundWindow(process_id: u32) -> i32;
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

/// Find the PID of a running process by executable name, if any.
#[cfg(windows)]
pub fn find_pid(exe_name: &str) -> Option<u32> {
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
                if ffi::field_eq_ignore_case(&entry.sz_exe_file, exe_name) {
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

/// Find the PID of the running game, if any.
#[cfg(windows)]
pub fn find_game_pid() -> Option<u32> {
    find_pid(crate::game::active().exe)
}

/// The running game's PID, for another module (e.g. secure-content injection) to act on it.
#[cfg(windows)]
pub fn game_pid() -> Option<u32> {
    find_game_pid()
}

/// Is MX Bikes currently running?
#[cfg(windows)]
pub fn is_game_running() -> bool {
    find_game_pid().is_some()
}

/// Under Wine the game is an ordinary macOS process whose argv still names the exe, so
/// `ps` finds it. Without this Play would cheerfully start a second copy.
#[cfg(target_os = "macos")]
pub fn is_game_running() -> bool {
    crate::winehost::running_exe(
        &crate::winehost::process_table(),
        crate::game::active().exe,
        std::process::id(),
    )
}

/// Under Proton the game is an ordinary Linux process whose command line still names
/// `mxbikes.exe`, so `/proc` finds it.
#[cfg(target_os = "linux")]
pub fn is_game_running() -> bool {
    crate::proton::running_exe(crate::game::active().exe)
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn is_game_running() -> bool {
    false
}

/// Is a program with this name running, other than us? `exe` is the Windows image name
/// (`MXB App.exe`); on macOS the app bundle's binary path is matched instead.
#[cfg(windows)]
pub fn process_running(exe: &str) -> bool {
    find_pid(exe).is_some_and(|pid| pid != std::process::id())
}

#[cfg(target_os = "macos")]
pub fn process_running(exe: &str) -> bool {
    let name = exe.trim_end_matches(".exe");
    let needle = format!("/{name}.app/Contents/MacOS/{name}");
    crate::winehost::running_exe(&crate::winehost::process_table(), &needle, std::process::id())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn process_running(_exe: &str) -> bool {
    false
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

/// Is the window the player is working in owned by some *other* process?
///
/// The overlay asks this after it loses focus, to tell "clicked back into the game"
/// apart from "opened our own file picker" — only the first means the player is done.
#[cfg(windows)]
pub fn foreground_is_another_app() -> bool {
    // SAFETY: both calls are reads. No foreground window at all (the desktop is
    // mid-switch) leaves `pid` at 0, which belongs to nobody and counts as neither.
    unsafe {
        let hwnd = ffi::GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0u32;
        ffi::GetWindowThreadProcessId(hwnd, &mut pid);
        pid != 0 && pid != ffi::GetCurrentProcessId()
    }
}

/// Where the game's window sits on the desktop, in physical pixels
/// (`left, top, right, bottom`).
///
/// `None` for a minimized game: minimizing keeps `WS_VISIBLE`, and `GetWindowRect` then
/// reports the iconic position around (-32000, -32000), off every monitor there is.
#[cfg(windows)]
pub fn game_window_rect() -> Option<(i32, i32, i32, i32)> {
    let hwnd = game_hwnd()?;
    // SAFETY: `hwnd` is a live handle from the walk above; both calls are reads, and both
    // are safe on a stale handle (they simply fail).
    unsafe {
        if ffi::IsIconic(hwnd) != 0 {
            return None;
        }
        let mut rect = ffi::Rect::default();
        // `GetWindowRect` only writes the four ints of the `RECT` we own.
        let ok = ffi::GetWindowRect(hwnd, &mut rect) != 0;
        ok.then_some((rect.left, rect.top, rect.right, rect.bottom))
    }
}

/// Is a DirectX app holding the screen in *exclusive* fullscreen right now?
///
/// Nothing can be drawn over that, overlay included. Advisory only: we still try to show
/// the overlay, and let the caller surface guidance alongside it.
#[cfg(windows)]
pub fn is_exclusive_fullscreen() -> bool {
    let mut state = 0i32;
    // SAFETY: shell32 writes one `QUNS_*` value through the pointer; a failed call
    // (non-zero HRESULT) leaves it at our initial 0, which matches no state.
    let hr = unsafe { ffi::SHQueryUserNotificationState(&mut state) };
    hr == 0 && state == ffi::QUNS_RUNNING_D3D_FULL_SCREEN
}

/// Let `pid` take the foreground. Called before asking the other app to show its overlay:
/// we hold the right (the player's click or hotkey was ours), and it doesn't.
#[cfg(windows)]
pub fn allow_foreground(pid: u32) -> bool {
    // SAFETY: takes a pid by value; a stale one simply fails.
    unsafe { ffi::AllowSetForegroundWindow(pid) != 0 }
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

/// A dev machine has no game to click back into, so the overlay never dismisses itself.
#[cfg(not(windows))]
pub fn foreground_is_another_app() -> bool {
    false
}

/// No game window to sit over — the overlay stays where Tauri centred it.
#[cfg(not(windows))]
pub fn game_window_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

/// Foreground rights are a Windows rule.
#[cfg(not(windows))]
pub fn allow_foreground(_pid: u32) -> bool {
    true
}
