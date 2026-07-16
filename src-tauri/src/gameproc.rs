//! Talk to a **running MX Bikes** process directly.
//!
//! Two jobs, both best-effort and Windows-only:
//!
//! 1. **Detect** whether `mxbikes.exe` is running (so the UI can tell the user a
//!    preset needs a profile reselect vs. "loads next launch").
//!
//! 2. **Live-refresh the rider look** (experimental). MX Bikes parses
//!    `profile.ini` into in-memory globals *once*, when a profile is (re)selected
//!    — the bike-select screen renders from those globals, not the file. So
//!    rewriting `profile.ini` never refreshes the look on its own. Reverse
//!    engineering of `mxbikes.exe` (see the `mxbikes-profile-load-re` notes)
//!    found the loader at image offset [`LOADER_OFFSET`]: it re-reads
//!    `profile.ini` and repopulates those globals. We re-run it in the live
//!    process via `CreateRemoteThread`, reusing the game's own code path instead
//!    of poking individual globals.
//!
//! ⚠️ The offset is derived from an **unpacked** build. `CreateRemoteThread`
//! resolves the module's ASLR base at runtime and adds the offset, so it targets
//! the right code as long as the shipping exe's layout matches. This is a spike:
//! it runs a game function off the render thread, which can race or crash the
//! game. It is gated behind an explicit per-apply flag and never runs by default.

use serde::Serialize;

/// The customization loader (`fcn.1400ecd00`) relative to the PE image base
/// `0x140000000`: `0x1400ecd00 - 0x140000000`. Re-running it re-parses
/// `profile.ini` and repopulates the in-memory rider-look globals.
#[cfg(windows)]
const LOADER_OFFSET: usize = 0x000e_cd00;

/// The game's main executable, matched case-insensitively when scanning the
/// process list.
#[cfg(windows)]
const GAME_EXE: &str = "mxbikes.exe";

/// Result of the experimental live-refresh attempt — surfaced to the UI so it
/// can say whether the look is already live or the user must reselect.
// Non-Windows builds only ever construct `Unsupported`/`Disabled`; the rest are
// live on Windows.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveRefresh {
    /// We re-ran the loader in the live game; the look should be live.
    Refreshed,
    /// The refresh was attempted but failed (couldn't open the process, find
    /// the module, or spawn the thread).
    Failed,
    /// MX Bikes isn't running, so there was nothing to refresh.
    GameNotRunning,
    /// The experimental flag was off — we didn't try.
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

/// The runtime base address of `mxbikes.exe` inside process `pid` (its first
/// module). Retries a few times because a module snapshot can transiently fail
/// with `ERROR_BAD_LENGTH` while the loader is busy.
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

/// Experimental: re-run the game's profile-load routine in the live process so a
/// freshly written `profile.ini` takes effect without a restart or manual
/// reselect. Best-effort; see the module docs for the risks.
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
