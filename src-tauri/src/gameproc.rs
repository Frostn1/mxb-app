use serde::Serialize;

use crate::config::AppConfig;

/// Customization loader `fcn.1400ecd00` minus PE image base `0x140000000`.
///
/// An `mxbikes.exe` offset, so it is only ever used when MX Bikes is the active game —
/// see [`crate::game::Caps::instant_refresh`], which gates every path that reaches here.
/// It is an address in *one build* of that exe; [`KNOWN_GOOD_BUILDS`] and
/// [`loader_looks_runnable`] are what keep it from being run in a different one.
#[cfg(windows)]
const LOADER_OFFSET: usize = 0x000e_cd00;

/// Where the game holds the local player's own GUID: `mxbikes.exe` VA `0x140e5522c` minus
/// the `0x140000000` image base.
///
/// The GUID is not in any file — it is derived from Steam at sign-in and only ever reaches
/// disk as a side effect of online play — so a running game is the one place to read it. Both
/// the profile screen's "show GUID" and its copy button read this same buffer.
///
/// Two ways this can be wrong, and both are handled by validating what comes back rather
/// than trusting it: the buffer is empty until Steam has authenticated, and the offset moves
/// whenever PiBoSo ships a build. A read that doesn't look like a GUID is reported as "not
/// found", never as a GUID.
#[cfg(windows)]
const GUID_OFFSET: usize = 0x000e_5522c;

/// Builds of `mxbikes.exe` the offsets above are known to be right for, as the PE
/// `TimeDateStamp` the linker wrote into the image.
///
/// The offsets are addresses in one particular build. PiBoSo ships new ones, code moves,
/// and nothing about `base + 0xecd00` announces that it is no longer the customization
/// loader — it is simply whatever now lives at that address. [`local_guid`] survives that
/// by checking what it reads back; [`refresh_look`] cannot, because it does not read
/// anything. It starts a thread there. On the wrong build that is a jump into the middle of
/// an unrelated function, on the game's own process, with the player's session behind it.
///
/// So the offsets are qualified by the build they came from. **Empty means unqualified**,
/// and unqualified is what shipped before this list existed: the structural checks in
/// [`loader_looks_runnable`] still apply, but a build match is not required, because
/// requiring a match against an empty list would take the feature away from everyone.
/// Filling this in is what turns the guard on, and `refresh_look` logs the running build's
/// stamp every time so any player's log yields the value to put here.
#[cfg(windows)]
const KNOWN_GOOD_BUILDS: &[u32] = &[];

/// The flag that makes a fresh game process connect straight to a server.
///
/// Undocumented: PiBoSo's docs list only `-dedicated` and `-clientport`, but the argv
/// parser at `0x140133640` (image base `0x140000000`) also handles `-directconnect` and a
/// sibling `-connect`. Each takes the *next* argv as a string, copies it to a global, and
/// sets a startup-mode enum — 2 for `-directconnect`, 3 for `-connect`.
///
/// Which of the two we want, and the exact address form the game expects, still need
/// confirming against the in-game `log.txt` on Windows. Both live here as single constants
/// so that test flips one line rather than reshaping the call site.
const CONNECT_FLAG: &str = "-directconnect";

/// Port used when an address doesn't carry one. The dedicated server's default, and what
/// every published MX Bikes server address omits.
const DEFAULT_SERVER_PORT: u16 = 54210;

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
    /// The running game isn't a build the loader offset is known good for, so nothing was
    /// run in it. The look stays as it is until the game is restarted, which is the
    /// outcome the offset was buying us anyway — and a great deal better than a thread
    /// started at whatever that address holds now.
    UnknownBuild,
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

    /// Enough to ask a process for its token, and the most an unelevated caller can be
    /// granted on a process of its own user. Deliberately not `PROCESS_QUERY_INFORMATION`:
    /// that one is refused across an integrity line for reasons of its own, which would
    /// blur the very distinction [`super::steam_elevation_conflict`] reads out of it.
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    /// Enough to wait on a process handle. Paired with
    /// `PROCESS_QUERY_LIMITED_INFORMATION` it is the whole access set
    /// [`super::GameSession`] needs, and it is granted on a process of our own user
    /// without elevation.
    pub const SYNCHRONIZE: u32 = 0x0010_0000;

    /// `WaitForSingleObject` said the handle is signalled — for a process handle, that
    /// means the process has exited.
    pub const WAIT_OBJECT_0: u32 = 0x0000_0000;

    pub const TOKEN_QUERY: u32 = 0x0008;
    /// `TokenElevation` in `TOKEN_INFORMATION_CLASS`.
    pub const TOKEN_ELEVATION_CLASS: i32 = 20;

    pub const ERROR_ACCESS_DENIED: u32 = 5;

    pub const WAIT_TIMEOUT_MS: u32 = 5_000;

    /// `SHQueryUserNotificationState` — a DirectX app owns the screen exclusively.
    /// Deliberately *not* `QUNS_BUSY` (2): a borderless-fullscreen game reports that
    /// too, and borderless is exactly the case the overlay works in.
    pub const QUNS_RUNNING_D3D_FULL_SCREEN: i32 = 3;

    /// `ShowWindow` — un-minimize without changing the restored size/position.
    pub const SW_RESTORE: i32 = 9;
    /// `ShellExecuteEx` — let the handler open the way it normally would.
    pub const SW_SHOWNORMAL: i32 = 1;

    /// We aren't pumping messages on this thread; without it the shell can return before
    /// it has finished with the string buffers we lent it.
    pub const SEE_MASK_NOASYNC: u32 = 0x0000_0100;

    /// `MEMORY_BASIC_INFORMATION`. The two padding slots are the x64 layout's own
    /// `__alignment` members; without them `region_size` lands four bytes early and every
    /// field after it is read from the wrong place.
    // No `Default`: raw pointers have none. Every caller zeroes it, as the rest of this
    // module does for the toolhelp structs. Most of it is never read — the fields are here
    // to put `state` and `protect` at the offsets the kernel writes them to.
    #[repr(C)]
    #[derive(Clone, Copy)]
    #[allow(dead_code)]
    pub struct MemoryBasicInformation {
        pub base_address: *mut c_void,
        pub allocation_base: *mut c_void,
        pub allocation_protect: u32,
        pub partition_id: u16,
        pub _alignment1: u16,
        pub region_size: usize,
        pub state: u32,
        pub protect: u32,
        pub kind: u32,
        pub _alignment2: u32,
    }

    /// `TOKEN_ELEVATION` — one `BOOL`, non-zero when the token is an elevated one.
    #[repr(C)]
    pub struct TokenElevation {
        pub token_is_elevated: u32,
    }

    /// `SHELLEXECUTEINFOW`. Field order and the two implicit padding slots (after
    /// `n_show` and after `dw_hot_key`) match the Win32 x64 layout under `repr(C)`.
    #[repr(C)]
    pub struct ShellExecuteInfoW {
        pub cb_size: u32,
        pub f_mask: u32,
        pub hwnd: Handle,
        pub lp_verb: *const u16,
        pub lp_file: *const u16,
        pub lp_parameters: *const u16,
        pub lp_directory: *const u16,
        pub n_show: i32,
        pub h_inst_app: Handle,
        pub lp_id_list: *mut c_void,
        pub lp_class: *const u16,
        pub hkey_class: Handle,
        pub dw_hot_key: u32,
        pub h_icon_or_monitor: Handle,
        pub h_process: Handle,
    }

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

    /// A Windows `FILETIME` — 100-nanosecond ticks since 1601, split across two `u32`s
    /// because the struct is not guaranteed to be 8-byte aligned.
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct FileTime {
        pub low: u32,
        pub high: u32,
    }

    impl FileTime {
        pub fn ticks(self) -> u64 {
            ((self.high as u64) << 32) | self.low as u64
        }
    }

    /// `TH32CS_SNAPTHREAD` — the thread list, which is per-machine and filtered by owner.
    pub const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;

    /// Enough to read a thread's start address and its debug registers. Both are granted on
    /// a thread of a process of our own user.
    pub const THREAD_QUERY_INFORMATION: u32 = 0x0040;
    pub const THREAD_GET_CONTEXT: u32 = 0x0008;

    /// `ThreadQuerySetWin32StartAddress` in `THREADINFOCLASS` — the address the thread was
    /// created to run, which is the one thing about a thread that says where it came from.
    pub const THREAD_START_ADDRESS_CLASS: i32 = 9;

    /// `CONTEXT_AMD64 | CONTEXT_DEBUG_REGISTERS` — ask for `Dr0`–`Dr7` and nothing else.
    pub const CONTEXT_DEBUG_REGISTERS: u32 = 0x0010_0010;

    /// The x64 `CONTEXT`, as far as the debug registers and no further.
    ///
    /// `GetThreadContext` writes a whole `CONTEXT` whatever it was asked for, so the buffer
    /// has to be the full 1232 bytes and 16-byte aligned or the call scribbles past it. The
    /// tail is named `rest` because nothing here reads it: the fields up to `dr7` are at the
    /// offsets Windows writes them to, and that is all this struct is for.
    #[repr(C, align(16))]
    pub struct Context {
        pub p1_home: u64,
        pub p2_home: u64,
        pub p3_home: u64,
        pub p4_home: u64,
        pub p5_home: u64,
        pub p6_home: u64,
        pub context_flags: u32,
        pub mx_csr: u32,
        pub seg_cs: u16,
        pub seg_ds: u16,
        pub seg_es: u16,
        pub seg_fs: u16,
        pub seg_gs: u16,
        pub seg_ss: u16,
        pub eflags: u32,
        pub dr0: u64,
        pub dr1: u64,
        pub dr2: u64,
        pub dr3: u64,
        pub dr6: u64,
        pub dr7: u64,
        pub rest: [u8; 1232 - 0x78],
    }

    /// Windows writes a whole `CONTEXT` whatever it was asked for, so getting this wrong
    /// does not produce a wrong answer — it produces a write past the end of the buffer.
    const _: () = assert!(std::mem::size_of::<Context>() == 1232);
    const _: () = assert!(std::mem::align_of::<Context>() == 16);

    #[repr(C)]
    pub struct ThreadEntry32 {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_thread_id: u32,
        pub th32_owner_process_id: u32,
        pub tp_base_pri: i32,
        pub tp_delta_pri: i32,
        pub dw_flags: u32,
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
        pub fn ReadProcessMemory(
            process: Handle,
            address: *const c_void,
            buffer: *mut c_void,
            size: usize,
            read: *mut usize,
        ) -> i32;
        /// What the process has mapped at an address: enough to say whether a start
        /// address is committed, executable, and part of the exe's own image.
        pub fn VirtualQueryEx(
            process: Handle,
            address: *const c_void,
            buffer: *mut MemoryBasicInformation,
            length: usize,
        ) -> usize;
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
        pub fn GetExitCodeProcess(process: Handle, exit_code: *mut u32) -> i32;
        /// Creation and exit stamps, so a session's length is the kernel's own answer
        /// rather than the gap between two of our polls.
        pub fn GetProcessTimes(
            process: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        pub fn CloseHandle(handle: Handle) -> i32;
        pub fn GetCurrentProcessId() -> u32;
        /// A pseudo-handle for our own process — a constant, not a handle to close.
        pub fn GetCurrentProcess() -> Handle;
        pub fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry32) -> i32;
        pub fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry32) -> i32;
        pub fn OpenThread(desired_access: u32, inherit: i32, thread_id: u32) -> Handle;
        /// Only ever asked for `CONTEXT_DEBUG_REGISTERS` here: a hardware breakpoint is set
        /// in `Dr0`–`Dr3` and armed in `Dr7`, and it is how code hooks a function without
        /// writing a byte of it.
        pub fn GetThreadContext(thread: Handle, context: *mut Context) -> i32;
        pub fn GetLastError() -> u32;
    }

    #[link(name = "ntdll")]
    extern "system" {
        /// The thread's Win32 start address. There is no documented Win32 call for it, and
        /// it is the only field that says whether a thread began inside a module or inside
        /// something written into the process.
        pub fn NtQueryInformationThread(
            thread: Handle,
            class: i32,
            info: *mut c_void,
            len: u32,
            out_len: *mut u32,
        ) -> i32;
    }
    #[link(name = "advapi32")]
    extern "system" {
        pub fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        pub fn GetTokenInformation(
            token: Handle,
            class: i32,
            info: *mut c_void,
            len: u32,
            out_len: *mut u32,
        ) -> i32;
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
        pub fn GetForegroundWindow() -> Handle;
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
        pub fn ShellExecuteExW(info: *mut ShellExecuteInfoW) -> i32;
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
fn find_pid(exe_name: &str) -> Option<u32> {
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
fn find_game_pid() -> Option<u32> {
    find_pid(crate::game::active().exe)
}

/// The running game's PID, for another module (e.g. secure-content injection) to act on it.
#[cfg(windows)]
pub fn game_pid() -> Option<u32> {
    find_game_pid()
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

/// A handle held on the running game so that *how it ended* survives it.
///
/// Without this the app watches the game through a process-table poll, which can only ever
/// report presence: the game is there, and then it isn't. That is the same observation
/// whether the player quit or the process died on an access violation, and support threads
/// were being argued from logs that could not tell those apart.
///
/// A handle opened while the process is alive keeps the exit code readable after it is
/// gone, and `GetProcessTimes` keeps the session's true length — so a crash four seconds
/// into a load reads as one, instead of as "somewhere inside the last poll interval".
#[cfg(windows)]
pub struct GameSession {
    handle: ffi::Handle,
    pid: u32,
}

#[cfg(windows)]
impl GameSession {
    /// Take a handle to the game if it is up. `None` if it isn't, or if the handle is
    /// refused — which is not worth surfacing, since the only cost is a session whose
    /// ending we can't describe.
    pub fn open() -> Option<Self> {
        let pid = find_game_pid()?;
        // SAFETY: `pid` came from a process snapshot; the handle is closed in `Drop`.
        let handle = unsafe {
            ffi::OpenProcess(ffi::SYNCHRONIZE | ffi::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
        };
        if handle.is_null() {
            log::debug!("[session] couldn't hold a handle on pid {pid}: exit code won't be readable");
            return None;
        }
        Some(Self { handle, pid })
    }

    /// `Some(code)` once the process is gone, `None` while it is still up.
    fn exit_code(&self) -> Option<u32> {
        // SAFETY: `self.handle` is live until `Drop`. A zero timeout polls without blocking.
        unsafe {
            if ffi::WaitForSingleObject(self.handle, 0) != ffi::WAIT_OBJECT_0 {
                return None;
            }
            let mut code = 0u32;
            if ffi::GetExitCodeProcess(self.handle, &mut code) == 0 {
                return Some(u32::MAX);
            }
            Some(code)
        }
    }

    /// How long the process lived, straight from the kernel.
    fn lifetime(&self) -> Option<std::time::Duration> {
        let (mut created, mut exited) = (ffi::FileTime::default(), ffi::FileTime::default());
        let (mut kernel, mut user) = (ffi::FileTime::default(), ffi::FileTime::default());
        // SAFETY: `self.handle` is live and carries `PROCESS_QUERY_LIMITED_INFORMATION`.
        let ok = unsafe {
            ffi::GetProcessTimes(self.handle, &mut created, &mut exited, &mut kernel, &mut user)
        };
        let (created, exited) = (created.ticks(), exited.ticks());
        // A still-running process reports a zero exit stamp.
        if ok == 0 || exited <= created {
            return None;
        }
        // FILETIME counts 100ns ticks.
        Some(std::time::Duration::from_nanos((exited - created) * 100))
    }

    /// If the game has ended, say how, and consume the session. `None` means it's still up.
    ///
    /// The severity is the point: a clean quit is `info` and a crash is `warn`, so the one
    /// question a support log has to answer is answerable by grep.
    pub fn report_if_ended(self) -> Option<Self> {
        // Deliberately not `?`: that would drop `self` — and with it the handle — every
        // poll the game is still up, so the exit code would never be readable.
        let Some(code) = self.exit_code() else { return Some(self) };
        let lived = self
            .lifetime()
            .map(|d| format!("{:.1}s", d.as_secs_f64()))
            .unwrap_or_else(|| "unknown".into());
        let pid = self.pid;
        match describe_exit(code) {
            None => log::info!("[session] game exited cleanly after {lived} (pid {pid})"),
            Some(what) => log::warn!(
                "[session] game CRASHED after {lived} — {what} (exit 0x{code:08X}, pid {pid})"
            ),
        }
        None
    }
}

// SAFETY: a Windows `HANDLE` belongs to the process, not to the thread that opened it —
// it stays valid in any thread and `CloseHandle` may be called from any of them. The
// handle is owned solely by this struct, so there is no aliasing to race over. Needed
// because the watcher that holds a session is an async task, which may be resumed on a
// different worker thread across an `.await`.
#[cfg(windows)]
unsafe impl Send for GameSession {}

#[cfg(windows)]
impl Drop for GameSession {
    fn drop(&mut self) {
        // SAFETY: opened in `open`, closed exactly once here.
        unsafe { ffi::CloseHandle(self.handle) };
    }
}

/// Name the NTSTATUS values a game actually dies on. `None` means "not a crash".
///
/// Exit code 0 is a clean quit. Anything with the high bit set is an unhandled exception
/// the loader turned into an exit code, and those names are the difference between a
/// diagnosable report and "it crashed".
#[cfg(windows)]
fn describe_exit(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => return None,
        // The one that matters most here: a memory-mapped read that the filesystem could
        // not satisfy. A cloud-backed file that is a placeholder rather than real bytes
        // fails exactly this way, which is why the mods folder gets checked at launch.
        0xC000_0006 => "in-page error — a file the game had mapped couldn't be read (cloud placeholder, failing disk, or removed media)",
        0xC000_0005 => "access violation",
        0xC000_001D => "illegal instruction",
        0xC000_0025 => "non-continuable exception",
        0xC000_008C => "array bounds exceeded",
        0xC000_008E => "float divide by zero",
        0xC000_0094 => "integer divide by zero",
        0xC000_00FD => "stack overflow",
        0xC000_0135 => "a required DLL was not found",
        0xC000_0142 => "a DLL failed to initialise",
        0xC000_0374 => "heap corruption",
        0xC000_0409 => "stack buffer overrun",
        0xC000_0017 => "out of memory",
        0x8000_0003 => "breakpoint (a debug build or an attached debugger)",
        u32::MAX => "exit code unreadable",
        // Anything else non-zero is still an abnormal exit worth seeing.
        _ => "abnormal exit",
    })
}

/// Read a fixed-size NUL-padded ANSI field as a `String`, stopping at the terminator.
#[cfg(windows)]
fn ansi_field(field: &[std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Every DLL currently mapped into the running game, by full path.
///
/// A module is *in* this list no matter how it arrived — `LoadLibrary`, a proxy DLL the
/// loader pulled in for the exe, or a manual map that registered itself. (One that never
/// registers is invisible here.)
///
/// `None` means we could not look — the game isn't up, or the snapshot failed — which is
/// deliberately different from `Some(vec![])`, "we looked and there was nothing to list".
#[cfg(windows)]
#[allow(dead_code)] // the Windows callers ask game_modules() for the refusal too.
pub fn game_module_paths() -> Option<Vec<String>> {
    match game_modules() {
        GameModules::Loaded(paths) => Some(paths),
        _ => None,
    }
}

/// What a module walk over the game produced.
///
/// The reason `game_module_paths`' `None` was split apart: "Windows refused to let us look"
/// is a different fact from "the walk came up empty", and it is the one worth acting on.
/// A refusal means the game is running at a higher integrity level than we are — and that
/// is the same wall an injector hits, so it is also the answer to "why isn't FrostMod in
/// the game?" (see [`crate::frostmod::attachment`]).
// Only the Windows walk can be refused; elsewhere the variant is unreachable by design.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum GameModules {
    /// Everything mapped into the game right now.
    Loaded(Vec<String>),
    /// Windows refused the snapshot. Retrying will not help.
    Denied,
    /// No game to look at.
    NotRunning,
    /// The walk produced nothing usable, and not because we were refused.
    Unavailable,
}

/// Walk the game's loaded modules, saying *why* when it doesn't work.
#[cfg(windows)]
pub fn game_modules() -> GameModules {
    let Some(pid) = find_game_pid() else {
        return GameModules::NotRunning;
    };
    // A snapshot taken while the game is loading its own modules fails with
    // ERROR_BAD_LENGTH; the documented remedy is simply to ask again. A refusal is not
    // that, so it breaks out rather than burning the retries on an answer that won't change.
    for _ in 0..8 {
        match module_paths(pid) {
            Ok(paths) => return GameModules::Loaded(paths),
            Err(WalkFailure::Denied) => return GameModules::Denied,
            Err(WalkFailure::Transient) => continue,
        }
    }
    GameModules::Unavailable
}

/// Why one module walk didn't produce a list.
#[cfg(windows)]
enum WalkFailure {
    /// `ERROR_ACCESS_DENIED` — the process is above us.
    Denied,
    /// Anything else, including the `ERROR_BAD_LENGTH` that just wants asking again.
    Transient,
}

/// One Toolhelp module walk over `pid`.
#[cfg(windows)]
fn module_paths(pid: u32) -> Result<Vec<String>, WalkFailure> {
    // SAFETY: module snapshot for a known pid; the handle is closed on every path out,
    // and we only read fields the API filled in.
    unsafe {
        let snap =
            ffi::CreateToolhelp32Snapshot(ffi::TH32CS_SNAPMODULE | ffi::TH32CS_SNAPMODULE32, pid);
        if snap == ffi::INVALID_HANDLE_VALUE {
            // Reading another process's module list needs rights UIPI withholds across an
            // integrity boundary, so this is what an elevated game looks like from an
            // unelevated app — the one failure here that retrying cannot fix.
            return Err(if ffi::GetLastError() == ffi::ERROR_ACCESS_DENIED {
                WalkFailure::Denied
            } else {
                WalkFailure::Transient
            });
        }
        let mut me: ffi::ModuleEntry32 = std::mem::zeroed();
        me.dw_size = std::mem::size_of::<ffi::ModuleEntry32>() as u32;
        let mut paths = Vec::new();
        if ffi::Module32First(snap, &mut me) != 0 {
            loop {
                let path = ansi_field(&me.sz_exe_path);
                if !path.is_empty() {
                    paths.push(path);
                }
                if ffi::Module32Next(snap, &mut me) == 0 {
                    break;
                }
            }
        }
        ffi::CloseHandle(snap);
        // An empty walk means the snapshot gave us nothing usable, not that a live process
        // has no modules — every process has at least its own exe. Treat it as a failure so
        // the caller retries rather than reporting an empty machine.
        if paths.is_empty() {
            Err(WalkFailure::Transient)
        } else {
            Ok(paths)
        }
    }
}

/// Every process on the machine, by executable name.
///
/// Names only: `Process32Next` carries `szExeFile` and nothing else, and asking each pid for
/// its full path means opening it, which fails on anything running as another user or at a
/// higher integrity level. A name is enough for what this is used for.
#[cfg(windows)]
// No caller in the app yet; each platform answers the same question.
#[allow(dead_code)]
pub fn running_process_names() -> Option<Vec<String>> {
    // SAFETY: standard Toolhelp process walk; the snapshot handle is closed before return.
    unsafe {
        let snap = ffi::CreateToolhelp32Snapshot(ffi::TH32CS_SNAPPROCESS, 0);
        if snap == ffi::INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: ffi::ProcessEntry32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<ffi::ProcessEntry32>() as u32;
        let mut names = Vec::new();
        if ffi::Process32First(snap, &mut entry) != 0 {
            loop {
                let name = ansi_field(&entry.sz_exe_file);
                if !name.is_empty() {
                    names.push(name);
                }
                if ffi::Process32Next(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        ffi::CloseHandle(snap);
        if names.is_empty() {
            None
        } else {
            Some(names)
        }
    }
}

/// What Windows reports about a region of address space.
///
/// Out here rather than in the Windows-only FFI block on purpose: these are numbers Windows
/// hands *back*, and everything that reads them — deciding whether a region can execute,
/// naming what backs it — is the same code on every platform and is tested on one that has
/// never seen a Windows process.
///
/// `MEM_COMMIT` means the page is backed rather than merely reserved. `MEM_IMAGE` is a
/// mapped executable, `MEM_MAPPED` a mapped data file, and `MEM_PRIVATE` memory the process
/// allocated for itself — which is where a manually mapped image ends up, because nothing
/// mapped it as an image.
pub const MEM_COMMIT: u32 = 0x0000_1000;
pub const MEM_IMAGE: u32 = 0x0100_0000;
pub const MEM_MAPPED: u32 = 0x0004_0000;
pub const MEM_PRIVATE: u32 = 0x0002_0000;

/// The four page protections that permit execution. A start address whose page is not one
/// of these is not code, whatever the offset says it should be.
pub const PAGE_EXECUTE: u32 = 0x10;
pub const PAGE_EXECUTE_READ: u32 = 0x20;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;
pub const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;

/// What backs a region, in a word.
pub fn region_kind(kind: u32) -> &'static str {
    match kind {
        MEM_IMAGE => "image",
        MEM_MAPPED => "mapped",
        MEM_PRIVATE => "private",
        _ => "other",
    }
}

/// A page protection as the letters it means: `r`, `w`, `x`, and `c` for copy-on-write.
///
/// `rwx` is the one worth recognising on sight. Ordinary code is `rx`; memory that is both
/// writable and executable at once is what something building code at runtime needs, and
/// almost nothing else does.
pub fn protection(protect: u32) -> String {
    let mut out = String::new();
    // The low byte carries the protection; the high bits are `PAGE_GUARD` and friends.
    match protect & 0xff {
        PAGE_EXECUTE => out.push('x'),
        PAGE_EXECUTE_READ => out.push_str("rx"),
        PAGE_EXECUTE_READWRITE => out.push_str("rwx"),
        PAGE_EXECUTE_WRITECOPY => out.push_str("rwxc"),
        0x01 => out.push('-'),
        0x02 => out.push('r'),
        0x04 => out.push_str("rw"),
        0x08 => out.push_str("rwc"),
        _ => out.push('?'),
    }
    out
}

/// Is this region backed, rather than merely reserved?
pub fn is_committed(state: u32) -> bool {
    state == MEM_COMMIT
}

/// Can this page protection execute?
pub fn is_executable(protect: u32) -> bool {
    matches!(
        protect & 0xff,
        PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY
    )
}

/// Where one loaded module sits in the game's address space.
///
/// Kept apart from the module *list* the rest of the app uses, which is a list of paths: the
/// only question these answer is "does anything the loader knows about cover this address",
/// and a path cannot answer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleRange {
    pub base: u64,
    pub size: u64,
}

/// Does any module the loader lists cover `addr`?
///
/// The whole of what separates ordinary code from injected code, and it is one line: every
/// byte of every legitimately loaded library is inside one of these ranges, because that is
/// what loading a library does.
pub fn covered(ranges: &[ModuleRange], addr: u64) -> bool {
    // Compared as an offset rather than against `base + size`, which would have to either
    // overflow or saturate at the top of the address space — and saturating quietly loses
    // the last byte of the range that reaches it.
    ranges.iter().any(|r| addr >= r.base && addr - r.base < r.size)
}

/// One committed, executable region of the game's address space that no loaded module covers.
///
/// The reason this exists at all: a DLL is only in the loader's list because it asked to be.
/// Code written into a process by hand and started with a thread never asks, so it is in no
/// list, has no path and no name — and is still, unavoidably, executable memory. That much
/// cannot be hidden from anything that can read the mapping list, because the CPU needs it
/// to be true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExecRegion {
    pub base: u64,
    pub size: u64,
    pub protect: u32,
    /// `MEM_IMAGE`, `MEM_MAPPED` or `MEM_PRIVATE` — what the kernel says backs it. An
    /// `MEM_IMAGE` region here is the loud one: something mapped as an image that the
    /// loader's own list does not mention, which is what unlinking a module looks like.
    pub kind: u32,
    /// A thread in the game was created to start inside this region.
    pub thread: bool,
}

/// What one walk of the game's threads found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadSurvey {
    pub total: u32,
    /// Threads whose start address is in no loaded module.
    pub foreign: u32,
    /// Threads with a hardware breakpoint armed — the way to hook a function without
    /// altering a byte of it, and something no ordinary game thread has set.
    pub breakpoints: u32,
    /// The start addresses of the foreign ones, so a region can be told it holds one.
    pub starts: Vec<u64>,
}

/// A read-only look inside the running game.
///
/// One open handle, used for a mapping walk, a thread walk and any number of memory reads,
/// because opening the process is the expensive part and all three questions are asked
/// together. Read access only: nothing here writes to the game, and the handle is not asked
/// for the rights that would let it.
///
/// It never looks at any process but the game. The thread walk is the one place that could —
/// Windows' thread snapshot is machine-wide and takes no pid — and it is filtered to the
/// game's own threads before anything is read.
#[cfg(windows)]
pub struct GameProbe {
    handle: ffi::Handle,
    pid: u32,
}

/// The most one read will fetch. Everything read here is a header or a string.
#[cfg(windows)]
const MAX_READ: usize = 64 * 1024;

/// User-mode address space ends here on x64; a walk that reaches it is done.
#[cfg(windows)]
const USER_SPACE_END: u64 = 0x0000_7fff_ffff_0000;

/// A bound on the mapping walk. A real process has a few thousand regions.
#[cfg(windows)]
const MAX_REGION_STEPS: usize = 100_000;

/// A bound on what the walk keeps. The count is still exact past this; only the list stops
/// growing, and the caller reads at most a couple of dozen of them anyway.
#[cfg(windows)]
const MAX_REGIONS_KEPT: usize = 4096;

#[cfg(windows)]
impl GameProbe {
    /// Open the running game for reading, or `None` if there is nothing to open or we are
    /// not allowed to. A refusal here is the same wall the module walk hits, and means the
    /// same thing: the game is above us.
    pub fn open() -> Option<Self> {
        let pid = find_game_pid()?;
        // SAFETY: opening a process by pid; the handle is closed in `Drop` and nowhere else.
        let handle = unsafe {
            ffi::OpenProcess(ffi::PROCESS_QUERY_INFORMATION | ffi::PROCESS_VM_READ, 0, pid)
        };
        if handle.is_null() {
            return None;
        }
        Some(Self { handle, pid })
    }

    /// Bytes at an address in the game, as far as the read got.
    ///
    /// A short answer is a real one: a read that runs off the end of a region returns what
    /// it managed, and that is usually the whole header we were after. Only a read that
    /// produced nothing is `None`.
    pub fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        if addr == 0 || len == 0 || len > MAX_READ {
            return None;
        }
        let mut buf = vec![0u8; len];
        let mut got: usize = 0;
        // SAFETY: `buf` is `len` bytes and stays alive across the call; the API writes at
        // most `len` and reports how much through `got`.
        unsafe {
            ffi::ReadProcessMemory(
                self.handle,
                addr as *const std::os::raw::c_void,
                buf.as_mut_ptr() as *mut std::os::raw::c_void,
                len,
                &mut got,
            );
        }
        if got == 0 {
            return None;
        }
        buf.truncate(got.min(len));
        Some(buf)
    }

    /// Where every module the loader lists is mapped.
    pub fn module_ranges(&self) -> Vec<ModuleRange> {
        // SAFETY: module snapshot for our own game pid; the handle is closed on every path.
        unsafe {
            let snap = ffi::CreateToolhelp32Snapshot(
                ffi::TH32CS_SNAPMODULE | ffi::TH32CS_SNAPMODULE32,
                self.pid,
            );
            if snap == ffi::INVALID_HANDLE_VALUE {
                return Vec::new();
            }
            let mut me: ffi::ModuleEntry32 = std::mem::zeroed();
            me.dw_size = std::mem::size_of::<ffi::ModuleEntry32>() as u32;
            let mut out = Vec::new();
            if ffi::Module32First(snap, &mut me) != 0 {
                loop {
                    out.push(ModuleRange {
                        base: me.mod_base_addr as u64,
                        size: me.mod_base_size as u64,
                    });
                    if ffi::Module32Next(snap, &mut me) == 0 {
                        break;
                    }
                }
            }
            ffi::CloseHandle(snap);
            out
        }
    }

    /// Walk the game's address space for executable memory no module accounts for.
    ///
    /// Returns the regions and, separately, how many executable regions there were in total.
    /// The second number is the denominator: "four unaccounted" means one thing in a process
    /// with forty executable regions and another in one with four hundred.
    pub fn exec_regions(&self, ranges: &[ModuleRange]) -> (Vec<ExecRegion>, u32) {
        let mut out = Vec::new();
        let mut total: u32 = 0;
        let mut addr: u64 = 0;
        for _ in 0..MAX_REGION_STEPS {
            // SAFETY: querying our own game handle at an address inside user space; the
            // struct is zeroed and only read back after a non-zero return.
            let mbi = unsafe {
                let mut mbi: ffi::MemoryBasicInformation = std::mem::zeroed();
                let got = ffi::VirtualQueryEx(
                    self.handle,
                    addr as *const std::os::raw::c_void,
                    &mut mbi,
                    std::mem::size_of::<ffi::MemoryBasicInformation>(),
                );
                if got == 0 {
                    break;
                }
                mbi
            };
            let base = mbi.base_address as u64;
            let size = mbi.region_size as u64;
            if size == 0 {
                break;
            }
            if is_committed(mbi.state) && is_executable(mbi.protect) {
                total = total.saturating_add(1);
                if !covered(ranges, base) && out.len() < MAX_REGIONS_KEPT {
                    out.push(ExecRegion {
                        base,
                        size,
                        protect: mbi.protect,
                        kind: mbi.kind,
                        thread: false,
                    });
                }
            }
            let next = base.saturating_add(size);
            if next <= addr || next >= USER_SPACE_END {
                break;
            }
            addr = next;
        }
        (out, total)
    }

    /// Walk the game's threads: how many, how many started outside every loaded module, and
    /// how many carry an armed hardware breakpoint.
    pub fn threads(&self, ranges: &[ModuleRange]) -> ThreadSurvey {
        let mut survey = ThreadSurvey::default();
        // SAFETY: the thread snapshot takes no pid — it is machine-wide by design — and
        // every entry not owned by the game is skipped before anything is opened or read.
        unsafe {
            let snap = ffi::CreateToolhelp32Snapshot(ffi::TH32CS_SNAPTHREAD, 0);
            if snap == ffi::INVALID_HANDLE_VALUE {
                return survey;
            }
            let mut te: ffi::ThreadEntry32 = std::mem::zeroed();
            te.dw_size = std::mem::size_of::<ffi::ThreadEntry32>() as u32;
            if ffi::Thread32First(snap, &mut te) != 0 {
                loop {
                    if te.th32_owner_process_id == self.pid {
                        survey.total = survey.total.saturating_add(1);
                        self.inspect_thread(te.th32_thread_id, ranges, &mut survey);
                    }
                    if ffi::Thread32Next(snap, &mut te) == 0 {
                        break;
                    }
                }
            }
            ffi::CloseHandle(snap);
        }
        survey.starts.sort_unstable();
        survey.starts.dedup();
        survey
    }

    /// One thread: where it was created to start, and whether it is carrying breakpoints.
    #[cfg(windows)]
    fn inspect_thread(&self, tid: u32, ranges: &[ModuleRange], survey: &mut ThreadSurvey) {
        // SAFETY: opening a thread of the game by id; closed before return on every path.
        unsafe {
            let thread = ffi::OpenThread(
                ffi::THREAD_QUERY_INFORMATION | ffi::THREAD_GET_CONTEXT,
                0,
                tid,
            );
            if thread.is_null() {
                return;
            }
            let mut start: u64 = 0;
            let ok = ffi::NtQueryInformationThread(
                thread,
                ffi::THREAD_START_ADDRESS_CLASS,
                &mut start as *mut u64 as *mut std::os::raw::c_void,
                std::mem::size_of::<u64>() as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 && start != 0 && !covered(ranges, start) {
                survey.foreign = survey.foreign.saturating_add(1);
                if survey.starts.len() < 64 {
                    survey.starts.push(start);
                }
            }

            let mut context: ffi::Context = std::mem::zeroed();
            context.context_flags = ffi::CONTEXT_DEBUG_REGISTERS;
            if ffi::GetThreadContext(thread, &mut context) != 0 && armed(context.dr7) {
                survey.breakpoints = survey.breakpoints.saturating_add(1);
            }
            ffi::CloseHandle(thread);
        }
    }
}

#[cfg(windows)]
impl Drop for GameProbe {
    fn drop(&mut self) {
        // SAFETY: the handle came from `OpenProcess` and is closed exactly once.
        unsafe {
            ffi::CloseHandle(self.handle);
        }
    }
}

/// Off Windows there is nothing to open.
///
/// On Linux the game is a Wine process whose *mappings* `/proc` will describe but whose
/// threads, module bases and PE headers it will not, and on macOS the game runs in a bottle
/// this app does not own the process tree of. Rather than half-answer, `open` says no and
/// every question above it comes back empty — the same "we could not look" the module walk
/// already reports, and deliberately not "we looked and it was clean".
///
/// The field is [`std::convert::Infallible`], so the methods are unreachable by type rather
/// than by promise: there is no way to construct one of these, and the compiler knows it.
#[cfg(not(windows))]
pub struct GameProbe(std::convert::Infallible);

#[cfg(not(windows))]
impl GameProbe {
    pub fn open() -> Option<Self> {
        None
    }

    pub fn read(&self, _addr: u64, _len: usize) -> Option<Vec<u8>> {
        match self.0 {}
    }

    pub fn module_ranges(&self) -> Vec<ModuleRange> {
        match self.0 {}
    }

    pub fn exec_regions(&self, _ranges: &[ModuleRange]) -> (Vec<ExecRegion>, u32) {
        match self.0 {}
    }

    pub fn threads(&self, _ranges: &[ModuleRange]) -> ThreadSurvey {
        match self.0 {}
    }
}

/// Is any of `Dr0`–`Dr3` armed?
///
/// The low eight bits of `Dr7` are the local and global enable bits, two per register. Every
/// other bit says what kind of breakpoint it would be, which is not a question we are asking.
#[cfg(windows)]
fn armed(dr7: u64) -> bool {
    dr7 & 0xff != 0
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
/// (or into any other app) apart from "opened our own file picker" — both take focus
/// off the overlay, only the first means the player is done with it.
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
/// Used to put the overlay over the game rather than wherever the primary monitor is —
/// a triple-screen sim rig would otherwise get it on the wrong display.
///
/// `None` for a minimized game, which is the case that made this worth a comment:
/// minimizing does not clear `WS_VISIBLE`, so [`collect_game_window`] hands one back like
/// any other window, and `GetWindowRect` then reports the iconic position — around
/// (-32000, -32000), off every monitor there is. Anything centred on that rect goes with
/// it. The caller wants "no answer" there, not an answer from another world.
///
/// Deliberately filtered here rather than in the walk: [`focus_game`] wants the minimized
/// window precisely so it can restore it.
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

/// The PE `TimeDateStamp` of the image mapped at `base`, read out of the live process.
///
/// The linker writes it once per build, so it is the cheapest exact answer to "is this the
/// game the offsets were read from?" — cheaper and far more specific than a file version,
/// which PiBoSo leaves at `0.0.0.0`. Read from the process rather than off disk on purpose:
/// the question is about the image that is actually running.
///
/// `None` when the headers can't be read or don't look like a PE, which is treated the same
/// as a build that doesn't match.
#[cfg(windows)]
unsafe fn image_build_stamp(proc: ffi::Handle, base: *mut u8) -> Option<u32> {
    /// `e_magic` — "MZ".
    const DOS_MAGIC: u16 = 0x5A4D;
    /// `Signature` — "PE\0\0".
    const PE_MAGIC: u32 = 0x0000_4550;
    /// `e_lfanew`, the offset of the NT headers, at a fixed place in the DOS header.
    const E_LFANEW: usize = 0x3C;
    /// `TimeDateStamp` sits two `WORD`s into `IMAGE_FILE_HEADER`, itself just past the
    /// four-byte signature.
    const STAMP_FROM_NT: usize = 4 + 4;

    // SAFETY: every read is a fixed-size read into a local, at an address inside the image
    // the caller resolved; a short or failed read yields `None` rather than a value.
    unsafe fn read<T: Copy>(proc: ffi::Handle, at: *const u8) -> Option<T> {
        let mut out = std::mem::zeroed::<T>();
        let mut got = 0usize;
        let ok = ffi::ReadProcessMemory(
            proc,
            at as *const std::os::raw::c_void,
            &mut out as *mut T as *mut std::os::raw::c_void,
            std::mem::size_of::<T>(),
            &mut got,
        ) != 0
            && got == std::mem::size_of::<T>();
        ok.then_some(out)
    }

    if read::<u16>(proc, base)? != DOS_MAGIC {
        return None;
    }
    let nt = base.add(read::<u32>(proc, base.add(E_LFANEW))? as usize);
    if read::<u32>(proc, nt)? != PE_MAGIC {
        return None;
    }
    read::<u32>(proc, nt.add(STAMP_FROM_NT))
}

/// Why [`LOADER_OFFSET`] must not be run in this process, or `Ok(())` if it may be.
///
/// Two layers, because they fail differently. The structural one asks the kernel what is
/// mapped at the address — a start address that isn't committed, executable code belonging
/// to the exe's own image is disqualifying whatever build this is, and it needs no
/// knowledge of any build to say so. The build match is the sharper check and the one that
/// catches the case that actually happens: the address is still perfectly good code, just
/// not the function we meant. It only applies once [`KNOWN_GOOD_BUILDS`] has entries.
#[cfg(windows)]
unsafe fn loader_looks_runnable(proc: ffi::Handle, base: *mut u8) -> Result<(), String> {
    let start = base.add(LOADER_OFFSET);

    let mut info = std::mem::zeroed::<ffi::MemoryBasicInformation>();
    let wrote = ffi::VirtualQueryEx(
        proc,
        start as *const std::os::raw::c_void,
        &mut info,
        std::mem::size_of::<ffi::MemoryBasicInformation>(),
    );
    if wrote == 0 {
        return Err(format!("VirtualQueryEx failed (error {})", ffi::GetLastError()));
    }
    if info.state != MEM_COMMIT {
        return Err("the loader address isn't committed memory".into());
    }
    let executable = PAGE_EXECUTE
        | PAGE_EXECUTE_READ
        | PAGE_EXECUTE_READWRITE
        | PAGE_EXECUTE_WRITECOPY;
    if info.protect & executable == 0 {
        return Err(format!("the loader address isn't executable (protect {:#x})", info.protect));
    }
    if info.allocation_base != base as *mut std::os::raw::c_void {
        return Err("the loader address isn't inside the game's own image".into());
    }

    let stamp = image_build_stamp(proc, base);
    match stamp {
        Some(s) => log::info!("[look] running game build: TimeDateStamp {s:#010x}"),
        None => log::warn!("[look] could not read the running game's build stamp"),
    }
    if KNOWN_GOOD_BUILDS.is_empty() {
        return Ok(());
    }
    match stamp {
        Some(s) if KNOWN_GOOD_BUILDS.contains(&s) => Ok(()),
        Some(s) => Err(format!("build {s:#010x} is not one the loader offset is known good for")),
        None => Err("the running game's build stamp is unreadable".into()),
    }
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

    // SAFETY: we open the process for thread creation, check what is actually mapped at
    // the resolved loader address, and only then spawn a thread there, wait briefly, and
    // close every handle we open — on the refusing path too. The start address is the
    // module base plus a fixed code offset within the exe.
    unsafe {
        let proc = ffi::OpenProcess(access, 0, pid);
        if proc.is_null() {
            return LiveRefresh::Failed;
        }
        // Nothing below this point can be undone, so everything that could say "not this
        // process" has to have said it by here.
        if let Err(why) = loader_looks_runnable(proc, base) {
            log::warn!("[look] not running the loader in the live game: {why}");
            ffi::CloseHandle(proc);
            return LiveRefresh::UnknownBuild;
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

/// The local player's own GUID, read out of the running game. `None` when the game isn't
/// running, hasn't signed in yet, or this build's offset no longer points at a GUID.
#[cfg(windows)]
pub fn local_guid() -> Option<String> {
    let pid = find_game_pid()?;
    let base = module_base(pid)?;
    let mut buf = [0u8; GUID_TEXT_LEN];

    // SAFETY: opens the process for reading only, reads a fixed-size buffer at a fixed
    // offset inside the module, and closes the handle on every path. The bytes are trusted
    // only if the call reported reading all of them.
    let read_ok = unsafe {
        let proc = ffi::OpenProcess(
            ffi::PROCESS_VM_READ | ffi::PROCESS_QUERY_INFORMATION,
            0,
            pid,
        );
        if proc.is_null() {
            return None;
        }
        let mut got = 0usize;
        let ok = ffi::ReadProcessMemory(
            proc,
            base.add(GUID_OFFSET) as *const std::os::raw::c_void,
            buf.as_mut_ptr() as *mut std::os::raw::c_void,
            buf.len(),
            &mut got,
        ) != 0
            && got == buf.len();
        ffi::CloseHandle(proc);
        ok
    };
    read_ok.then_some(()).and_then(|()| valid_guid(&buf))
}

#[cfg(not(windows))]
pub fn local_guid() -> Option<String> {
    None
}

/// A GUID as the game prints it: 18 hex characters.
#[cfg(windows)]
const GUID_TEXT_LEN: usize = 18;

/// Read `buf` as a GUID, or nothing.
///
/// All-zero is what the buffer holds before Steam has authenticated the player, and it is a
/// value the format itself uses to mean "bound to nobody" — so it is a miss here, not a GUID
/// worth handing to anyone.
#[cfg(windows)]
fn valid_guid(buf: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(buf).ok()?.to_ascii_uppercase();
    let shaped = s.len() == GUID_TEXT_LEN && s.bytes().all(|b| b.is_ascii_hexdigit());
    (shaped && s.bytes().any(|b| b != b'0')).then_some(s)
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
/// `mxbikes.exe`, wrappers and all, so `/proc` finds it. Without this the app believes the
/// game is never running: Play never greys out, and everything that asks "is it up?" —
/// whether a paint will show now or next launch, whether it's safe to move files — has
/// been answering for a machine where the game can't run at all.
#[cfg(target_os = "linux")]
pub fn is_game_running() -> bool {
    crate::proton::running_exe(crate::game::active().exe)
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn is_game_running() -> bool {
    false
}

/// The game's mapped DLLs, read out of Proton's view of the process.
///
/// Wine maps a PE the same way the loader would, file-backed, so `/proc/<pid>/maps` names
/// every DLL the game has loaded — including one an injector pushed in. More than one pid
/// can name the exe (Proton leaves its wrapper around it), so the union across all of them
/// is what we want rather than a guess at which is the real one.
#[cfg(target_os = "linux")]
pub fn game_module_paths() -> Option<Vec<String>> {
    let pids = crate::proton::exe_pids(crate::game::active().exe);
    if pids.is_empty() {
        return None;
    }
    let mut paths: Vec<String> = Vec::new();
    for pid in pids {
        paths.extend(crate::proton::mapped_pe_files(pid));
    }
    if paths.is_empty() {
        None
    } else {
        paths.sort();
        paths.dedup();
        Some(paths)
    }
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn running_process_names() -> Option<Vec<String>> {
    let names = crate::proton::process_names();
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

/// macOS runs the game through a Wine bottle we don't own the process tree of, and there is
/// no `/proc` to read a mapping list out of. Rather than half-answer, say we couldn't look —
/// `None` reports "not supported here" rather than an empty list that reads as "nothing".
#[cfg(not(any(windows, target_os = "linux")))]
pub fn game_module_paths() -> Option<Vec<String>> {
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
#[allow(dead_code)]
pub fn running_process_names() -> Option<Vec<String>> {
    None
}

/// The same answer off Windows, from whatever each platform can see.
///
/// No `Denied` arm: the integrity levels that produce one are a Windows idea, and the game
/// here is a Wine process we either can read the mappings of (Linux, via `/proc`) or cannot
/// look inside at all (macOS). Both are `Unavailable` rather than a refusal to explain.
#[cfg(not(windows))]
pub fn game_modules() -> GameModules {
    if !is_game_running() {
        return GameModules::NotRunning;
    }
    match game_module_paths() {
        Some(paths) => GameModules::Loaded(paths),
        None => GameModules::Unavailable,
    }
}

#[cfg(not(windows))]
pub fn refresh_look() -> LiveRefresh {
    LiveRefresh::Unsupported
}

/// Exit codes are a Windows story: the crash reports this exists to answer come from
/// Windows machines, and neither Wine nor a dev build has the same notion of an NTSTATUS
/// exit. The session opens as `None` and nothing else changes.
#[cfg(not(windows))]
pub struct GameSession;

#[cfg(not(windows))]
impl GameSession {
    pub fn open() -> Option<Self> {
        None
    }

    pub fn report_if_ended(self) -> Option<Self> {
        None
    }
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

/// A dev machine has no game to click back into, and the overlay there is an ordinary
/// window that behaves itself when it loses focus — so it never dismisses itself.
#[cfg(not(windows))]
pub fn foreground_is_another_app() -> bool {
    false
}

/// No game window to sit over — the overlay stays where Tauri centred it.
#[cfg(not(windows))]
pub fn game_window_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

/// What happened when the user pressed Play.
// Platforms with no launch arm of their own only ever construct `AlreadyRunning`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchOutcome {
    /// We started the game.
    Launched,
    /// MX Bikes was already up — we left it alone rather than spawning a second copy.
    AlreadyRunning,
}

/// The active game's install folder and its executable, for launching.
///
/// Falls back to Steam detection when `gamePath` is blank: the setting only ever gets
/// filled by that same detector, so an install we can find is one we can launch even if
/// the config predates the setting.
fn resolve_exe(cfg: &AppConfig) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let game = cfg.game();
    let dir = match cfg.game_path.trim() {
        "" => crate::config::detect_game_path(game).ok_or_else(|| {
            anyhow::anyhow!(
                "{0} install folder isn't set — set it in Settings, under {0} install folder.",
                game.display
            )
        })?,
        p => p.to_string(),
    };
    let dir = std::path::PathBuf::from(dir);
    let exe = crate::library::resolve_child(&dir, game.exe);
    if !exe.is_file() {
        anyhow::bail!(
            "Couldn't find {} in {} — check the {} install folder in Settings.",
            game.exe,
            dir.display(),
            game.display
        );
    }
    Ok((dir, exe))
}

/// The Wine prefix `exe` lives in, and the runner that should drive it (macOS).
///
/// The game is a Windows binary here, so it runs inside a prefix, and the prefix is
/// whatever sits above `drive_c` in the exe's own path — see [`crate::winehost`]. Shared
/// with FrostMod's start, which has to land in that same prefix to inject into the game:
/// both fail with the same words, because it is the same thing missing.
#[cfg(target_os = "macos")]
pub fn prefix_and_runner(
    cfg: &AppConfig,
    exe: &std::path::Path,
) -> anyhow::Result<(std::path::PathBuf, crate::winehost::Runner)> {
    let game = cfg.game().display;
    let (prefix, _) = crate::winehost::split_prefix(exe).ok_or_else(|| {
        anyhow::anyhow!(
            "{game} is a Windows game — on macOS the install folder has to be the copy \
             inside your CrossOver, Whisky or Wine bottle (a path with drive_c in it). \
             Set it in Settings, under {game} install folder."
        )
    })?;
    let runner = crate::winehost::resolve(&cfg.wine_runner, Some(&prefix)).ok_or_else(|| {
        anyhow::anyhow!(
            "No Wine runner found — install CrossOver, Whisky or Wine to run {game} on \
             macOS, or point Settings at one you already have."
        )
    })?;
    Ok((prefix, runner))
}

/// The same pair for a caller that doesn't already have the exe in hand.
#[cfg(target_os = "macos")]
pub fn game_prefix_and_runner(
    cfg: &AppConfig,
) -> anyhow::Result<(std::path::PathBuf, crate::winehost::Runner)> {
    let (_, exe) = resolve_exe(cfg)?;
    prefix_and_runner(cfg, &exe)
}

/// The Steam client's own executable.
#[cfg(windows)]
const STEAM_EXE: &str = "steam.exe";

/// Did Steam install this copy of the game?
///
/// The path alone decides it. A Steam library can sit on any drive under any name, but
/// every install inside one lands under `steamapps/common`, and nothing else puts a game
/// there.
///
/// Deliberately *not* also sniffing for `steam_api64.dll` beside the exe, which would
/// catch the rarer case of a Steam copy moved out of its library: the standalone build
/// PiBoSo sells direct can carry the same Steam library, and reading that as a Steam copy
/// would hand Play to a Steam that doesn't own the game. The two mistakes aren't
/// symmetrical — missing a Steam copy costs nothing but the old behaviour, while claiming
/// one wrongly leaves Play doing nothing at all.
// Only the Windows arm picks a route, but the tests cover this on every platform.
#[cfg_attr(not(windows), allow(dead_code))]
fn steam_owned(dir: &std::path::Path) -> bool {
    dir.components().any(|c| c.as_os_str().eq_ignore_ascii_case("steamapps"))
}

/// The `steam://` URL that asks Steam to start the game, connect flag and all.
///
/// Launch arguments go after a `//` separator, space-separated. The separator is
/// percent-encoded because the whole thing is a URL and a raw space is not one — and it
/// is the only thing that needs encoding, since [`parse_server_address`] has already
/// limited the address to characters a URL carries as they are.
// Windows and Linux both hand Steam a URL; macOS goes through Wine and never does.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn steam_url(appid: &str, address: Option<&str>) -> String {
    match address {
        Some(addr) => format!("steam://rungameid/{appid}//{}", connect_args(addr).join("%20")),
        None => format!("steam://rungameid/{appid}"),
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Hand `url` to whichever program Windows has registered for its scheme.
#[cfg(windows)]
fn shell_open(url: &str) -> anyhow::Result<()> {
    let verb = wide("open");
    let file = wide(url);
    let mut info = ffi::ShellExecuteInfoW {
        cb_size: std::mem::size_of::<ffi::ShellExecuteInfoW>() as u32,
        f_mask: ffi::SEE_MASK_NOASYNC,
        hwnd: std::ptr::null_mut(),
        lp_verb: verb.as_ptr(),
        lp_file: file.as_ptr(),
        lp_parameters: std::ptr::null(),
        lp_directory: std::ptr::null(),
        n_show: ffi::SW_SHOWNORMAL,
        h_inst_app: std::ptr::null_mut(),
        lp_id_list: std::ptr::null_mut(),
        lp_class: std::ptr::null(),
        hkey_class: std::ptr::null_mut(),
        dw_hot_key: 0,
        h_icon_or_monitor: std::ptr::null_mut(),
        h_process: std::ptr::null_mut(),
    };
    // SAFETY: `info` is a correctly sized, fully initialised SHELLEXECUTEINFOW whose
    // string pointers refer to NUL-terminated buffers that outlive the call —
    // `SEE_MASK_NOASYNC` is what makes that last part true off a message-pumping thread.
    // We ask for no process handle back, so there is none to close.
    if unsafe { ffi::ShellExecuteExW(&mut info) } == 0 {
        let err = unsafe { ffi::GetLastError() };
        anyhow::bail!("Windows wouldn't open {url} (error {err})");
    }
    Ok(())
}

/// Is the token elevated? `None` when Windows won't say.
///
/// SAFETY: `process` must be a live process handle opened with at least
/// `PROCESS_QUERY_LIMITED_INFORMATION`. The token handle we open here is closed before
/// returning; `process` is the caller's to close.
#[cfg(windows)]
unsafe fn token_elevated(process: ffi::Handle) -> Option<bool> {
    let mut token: ffi::Handle = std::ptr::null_mut();
    if ffi::OpenProcessToken(process, ffi::TOKEN_QUERY, &mut token) == 0 {
        return None;
    }
    let mut elevation = ffi::TokenElevation { token_is_elevated: 0 };
    let mut written = 0u32;
    let ok = ffi::GetTokenInformation(
        token,
        ffi::TOKEN_ELEVATION_CLASS,
        &mut elevation as *mut ffi::TokenElevation as *mut std::os::raw::c_void,
        std::mem::size_of::<ffi::TokenElevation>() as u32,
        &mut written,
    ) != 0;
    ffi::CloseHandle(token);
    ok.then(|| elevation.token_is_elevated != 0)
}

/// Is *this* process elevated? `None` when Windows won't say.
#[cfg(windows)]
pub fn we_are_elevated() -> Option<bool> {
    // SAFETY: `GetCurrentProcess` hands back a pseudo-handle to ourselves, which is a
    // constant rather than a handle to close.
    unsafe { token_elevated(ffi::GetCurrentProcess()) }
}

/// Off Windows there is no elevation to be on the wrong side of.
#[cfg(not(windows))]
pub fn we_are_elevated() -> Option<bool> {
    Some(false)
}

/// Why Steam is about to refuse us, when it is.
///
/// Steam will not start a game for a program running at a different Windows integrity
/// level. The request comes back `ERROR_ACCESS_DENIED`, and what the player sees is
/// Steam's own "Access is denied. (0x5)" box — never anything we said. Worth naming
/// *before* we ask, because both routes to a Steam copy end at that same dialog: the
/// URL, and the exe, whose Steam layer hands itself back to the client anyway.
///
/// `None` when the two agree, when Steam isn't up (the shell starts it at our own level),
/// or when Windows won't tell us — a guess here would be worse than the dialog.
#[cfg(windows)]
fn steam_elevation_conflict() -> Option<&'static str> {
    const WE_ARE_ELEVATED: &str = "MXB App is running as administrator and Steam isn't. \
        Steam refuses to start a game for a program above it, and that refusal is the \
        \"Access is denied. (0x5)\" box. Close MXB App and start it normally — or run \
        Steam as administrator too — then press Play again.";
    const STEAM_IS_ELEVATED: &str = "Steam is running as administrator and MXB App isn't, \
        so Steam won't take a launch from us — the refusal comes back as \"Access is \
        denied. (0x5)\". Restart Steam normally — or run MXB App as administrator too — \
        then press Play again.";

    let pid = find_pid(STEAM_EXE)?;
    let ours = we_are_elevated()?;
    let theirs = process_elevated(pid, ours)?;

    (ours != theirs).then_some(if ours { WE_ARE_ELEVATED } else { STEAM_IS_ELEVATED })
}

/// Is `pid` elevated, as seen from here? `None` when Windows won't say.
///
/// `ours` is our own elevation, which the refusal case needs: a process of our own user
/// that we may not even look at is one sitting above us — which only leaves elevation, and
/// only when we're the unelevated one. Any other failure (it exited between the walk and
/// here) says nothing, and a guess would be worse than silence.
#[cfg(windows)]
fn process_elevated(pid: u32, ours: bool) -> Option<bool> {
    // SAFETY: the handle is opened for a query-only right and closed on every path out.
    unsafe {
        let proc = ffi::OpenProcess(ffi::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if proc.is_null() {
            if ours || ffi::GetLastError() != ffi::ERROR_ACCESS_DENIED {
                return None;
            }
            return Some(true);
        }
        let elevated = token_elevated(proc);
        ffi::CloseHandle(proc);
        elevated
    }
}

/// Turn a refused `CreateProcess` into something the player can act on.
///
/// `PermissionDenied` is Windows' `ERROR_ACCESS_DENIED`, the 0x5 that antivirus and
/// folder permissions produce. None of it is ours to fix, so the message names where it
/// comes from rather than only repeating the number.
#[cfg(windows)]
fn spawn_error(exe: &std::path::Path, err: &std::io::Error) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return anyhow::anyhow!(
            "Windows refused to start {} — access denied (0x5). Something outside the game \
             is blocking it: add the install folder to your antivirus / Windows Security \
             exclusions, and check the folder isn't one your account can only read. \
             Starting the game straight from Explorer hits the same block, which is the \
             quickest way to confirm it isn't MXB App.",
            exe.display()
        );
    }
    anyhow::anyhow!("Couldn't start {}: {err}", exe.display())
}

/// Start MX Bikes, unless it's already running.
///
/// A Steam copy is Steam's to start, on Windows as much as on Linux — see [`launch_with`].
/// A standalone (non-Steam) copy has nobody to ask, so Windows runs its exe directly and
/// needs no Steam to be up. Under Proton the exe isn't ours to spawn at all — Steam has to
/// set up the prefix — so Linux always hands the Steam URL to the desktop. macOS runs the
/// exe too, but through the Wine wrapper that owns the prefix it lives in (see
/// [`crate::winehost`]).
pub fn launch(cfg: &AppConfig) -> anyhow::Result<LaunchOutcome> {
    launch_with(cfg, None)
}

/// Start MX Bikes and connect it straight to `address`, unless it's already running.
///
/// The game only reads the connect flag at startup, so a running copy can't be steered
/// into a server — the caller gets [`LaunchOutcome::AlreadyRunning`] and has to close it
/// first.
pub fn join(cfg: &AppConfig, address: &str) -> anyhow::Result<LaunchOutcome> {
    let address = parse_server_address(address)?;
    launch_with(cfg, Some(&address))
}

/// Normalize a user-supplied server address into the `host:port` form the connect flag
/// takes, filling in [`DEFAULT_SERVER_PORT`] when it's left off.
///
/// Deliberately strict, because the result is handed to the game as a command-line
/// argument: anything that could smuggle a *second* argument past us — whitespace, a
/// leading `-` — has to be rejected here, or a pasted address could quietly turn into
/// another flag entirely.
pub fn parse_server_address(input: &str) -> anyhow::Result<String> {
    let raw = input.trim();
    if raw.is_empty() {
        anyhow::bail!("Enter a server address, like 203.0.113.10:54210.");
    }
    // MX Bikes networking is IPv4-only, so a bracketed literal is a mistake worth naming
    // rather than letting it fail as an unresolvable host.
    if raw.starts_with('[') {
        anyhow::bail!("MX Bikes servers are IPv4 only — use an address like 203.0.113.10:54210.");
    }

    let (host, port) = match raw.rsplit_once(':') {
        Some((host, port)) => {
            let parsed: u16 = port
                .parse()
                .ok()
                .filter(|p| *p > 0)
                .ok_or_else(|| anyhow::anyhow!("\"{port}\" isn't a valid port — expected 1-65535."))?;
            (host, parsed)
        }
        None => (raw, DEFAULT_SERVER_PORT),
    };

    if host.is_empty() {
        anyhow::bail!("Enter a server address, like 203.0.113.10:54210.");
    }
    if host.starts_with('-') {
        anyhow::bail!("\"{host}\" isn't a valid server address.");
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        anyhow::bail!(
            "\"{host}\" isn't a valid server address — use an IP or hostname, like 203.0.113.10:54210."
        );
    }

    Ok(format!("{host}:{port}"))
}

/// The argv entries that make the game join `address`.
///
/// Two separate entries, not `flag=value`: the parser matches the flag and then reads the
/// *following* argv as the address.
fn connect_args(address: &str) -> [String; 2] {
    [CONNECT_FLAG.to_string(), address.to_string()]
}

/// Shared body of [`launch`] and [`join`] — `address` is already normalized.
/// When the app last asked for the game to start. Steam usually spawns the process, so there
/// is no child handle and no pid to match on — the launch *instant* is the only provenance we
/// get, and anything that reaches into the game process checks it before acting.
static LAST_APP_LAUNCH: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// How long after a Play click a game that appears still counts as one the app started. Steam
/// can take a while to get from the URL to a running process, so this is generous; it only has
/// to separate "the app asked for this" from "the player started it themselves hours ago".
const APP_LAUNCH_WINDOW: std::time::Duration = std::time::Duration::from_secs(180);

/// Record that the app asked for the game to start.
fn note_app_launch() {
    if let Ok(mut slot) = LAST_APP_LAUNCH.lock() {
        *slot = Some(std::time::Instant::now());
    }
}

/// When the current game session was first noticed, or `None` if the game isn't up.
static SESSION_SINCE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Called by [`crate::sessionwatch`] as a session begins and ends, so anything that needs to
/// know "how long has the game been up" can ask without a second process poll.
pub fn note_session(started: bool) {
    if let Ok(mut slot) = SESSION_SINCE.lock() {
        *slot = started.then(std::time::Instant::now);
    }
}

/// How long the game has been running, as far as the session poll has noticed. `None` when
/// no session is up.
pub fn session_age() -> Option<std::time::Duration> {
    SESSION_SINCE.lock().ok().and_then(|slot| *slot).map(|at| at.elapsed())
}

/// Whether the game now running is one the app launched, as far as we can tell.
///
/// Used to keep the app from reaching into a process it had no hand in starting.
pub fn launched_by_app() -> bool {
    LAST_APP_LAUNCH
        .lock()
        .ok()
        .and_then(|slot| *slot)
        .is_some_and(|at| at.elapsed() < APP_LAUNCH_WINDOW)
}

fn launch_with(cfg: &AppConfig, address: Option<&str>) -> anyhow::Result<LaunchOutcome> {
    if is_game_running() {
        return Ok(LaunchOutcome::AlreadyRunning);
    }
    let (dir, exe) = resolve_exe(cfg)?;
    let game = cfg.game().display;
    match address {
        Some(addr) => log::info!("launching {game} into {addr}: {}", exe.display()),
        None => log::info!("launching {game}: {}", exe.display()),
    }
    note_app_launch();

    #[cfg(windows)]
    {
        // Running a Steam copy's exe ourselves does work — the build's Steam layer hands
        // itself straight back to the client — but it puts us on the path a launch has to
        // survive, and that path is where the "Access is denied. (0x5)" reports come from:
        // security software vetoing *our* CreateProcess of a game exe, or an install
        // folder this account may read and not execute out of. Asking Steam to start its
        // own game takes us out of it, and gets the playtime counted besides.
        if steam_owned(&dir) {
            // Not a fallback: while the integrity line is crossed nothing we spawn will
            // work, and running the exe would only move the same refusal into a dialog
            // that doesn't say who to blame.
            if let Some(reason) = steam_elevation_conflict() {
                anyhow::bail!("{reason}");
            }
            let url = steam_url(cfg.game().steam_appid, address);
            match shell_open(&url) {
                Ok(()) => return Ok(LaunchOutcome::Launched),
                // A Steam whose own URL scheme isn't registered is a broken install, not a
                // reason to leave Play dead — the exe is still sitting right there.
                Err(e) => log::warn!("{e:#}; running {} directly instead", exe.display()),
            }
        }

        // No CREATE_NO_WINDOW here (unlike the headless FrostMod child) — the game
        // draws its own window and hiding the console would gain nothing.
        let mut cmd = std::process::Command::new(&exe);
        cmd.current_dir(&dir);
        if let Some(addr) = address {
            cmd.args(connect_args(addr));
        }
        cmd.spawn().map_err(|e| spawn_error(&exe, &e))?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(target_os = "linux")]
    {
        let _ = dir;
        // Under Proton this is the only way to reach the exe's argv, since Steam — not
        // us — spawns it.
        let url = steam_url(cfg.game().steam_appid, address);
        std::process::Command::new("xdg-open").arg(&url).spawn().map_err(|e| {
            anyhow::anyhow!("Couldn't ask Steam to launch {} ({url}): {e}", cfg.game().display)
        })?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(target_os = "macos")]
    {
        let (prefix, runner) = prefix_and_runner(cfg, &exe)?;

        let extra: Vec<String> = address.map(|a| connect_args(a).to_vec()).unwrap_or_default();
        let plan = crate::winehost::plan(&runner, &prefix, &exe, &extra);
        log::info!(
            "via {}: {} {:?}",
            runner.via(),
            plan.program.display(),
            plan.args
        );

        let mut cmd = std::process::Command::new(&plan.program);
        cmd.current_dir(&dir).args(&plan.args);
        for (key, value) in &plan.env {
            cmd.env(key, value);
        }
        cmd.spawn().map_err(|e| {
            anyhow::anyhow!("Couldn't start {} through {}: {e}", exe.display(), runner.via())
        })?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (dir, address);
        anyhow::bail!("Launching MX Bikes is supported on Windows, macOS and Linux only")
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
    fn fills_in_the_default_port_when_the_address_omits_one() {
        assert_eq!(parse_server_address("203.0.113.10").unwrap(), "203.0.113.10:54210");
        assert_eq!(parse_server_address("mx.example.com").unwrap(), "mx.example.com:54210");
    }

    #[test]
    fn keeps_an_explicit_port_and_trims_surrounding_space() {
        assert_eq!(parse_server_address("  203.0.113.10:9000  ").unwrap(), "203.0.113.10:9000");
    }

    #[test]
    fn rejects_addresses_that_could_smuggle_a_second_argument() {
        // The whole point of validating: each of these would otherwise reach the game's
        // argv parser as an extra flag rather than as part of the address.
        for bad in ["203.0.113.10 -mod evil", "-directconnect", "203.0.113.10:54210 -log"] {
            assert!(
                parse_server_address(bad).is_err(),
                "{bad:?} must not survive validation"
            );
        }
    }

    #[test]
    fn rejects_empty_and_malformed_ports() {
        for bad in ["", "   ", "203.0.113.10:", "203.0.113.10:0", "203.0.113.10:99999", ":54210"] {
            assert!(parse_server_address(bad).is_err(), "{bad:?} must not survive validation");
        }
    }

    #[test]
    fn names_ipv6_rather_than_failing_on_lookup() {
        let err = parse_server_address("[::1]:54210").unwrap_err().to_string();
        assert!(err.contains("IPv4"), "expected an IPv4-only message, got {err:?}");
    }

    #[test]
    fn passes_the_flag_and_address_as_separate_argv_entries() {
        // `flag=value` would be read as one argument and never matched.
        assert_eq!(
            connect_args("203.0.113.10:54210"),
            ["-directconnect".to_string(), "203.0.113.10:54210".to_string()]
        );
    }

    #[test]
    fn asks_steam_for_the_game_by_appid() {
        assert_eq!(steam_url("655500", None), "steam://rungameid/655500");
    }

    /// A raw space would end the URL at the flag and drop the address, so the game would
    /// come up on the main menu with no sign of why.
    #[test]
    fn encodes_the_separator_between_steam_launch_arguments() {
        assert_eq!(
            steam_url("655500", Some("203.0.113.10:54210")),
            "steam://rungameid/655500//-directconnect%20203.0.113.10:54210"
        );
    }

    #[test]
    fn a_folder_in_a_steam_library_belongs_to_steam() {
        let root = temp_dir("steam-library");
        let dir = root.join("SteamLibrary/steamapps/common/MX Bikes");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(steam_owned(&dir), "a steamapps path is a Steam copy even when it's empty");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The standalone copy PiBoSo sells direct: nobody to ask, so Play runs the exe.
    /// The Steam library beside the exe is exactly the false signal `steam_owned` refuses
    /// to read — asking Steam for a game it doesn't own would leave Play doing nothing.
    #[test]
    fn a_standalone_install_does_not_belong_to_steam() {
        let dir = temp_dir("standalone");
        std::fs::write(dir.join(crate::game::MXB.exe), b"stub").unwrap();
        std::fs::write(dir.join("steam_api64.dll"), b"stub").unwrap();
        assert!(!steam_owned(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_the_exe_in_the_configured_folder() {
        let dir = temp_dir("found");
        std::fs::write(dir.join(crate::game::MXB.exe), b"stub").unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = dir.to_string_lossy().into_owned();
        let (found_dir, exe) = resolve_exe(&cfg).expect("a folder holding mxbikes.exe launches");
        assert_eq!(found_dir, dir);
        assert_eq!(exe, dir.join(crate::game::MXB.exe));

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
            .eq_ignore_ascii_case(crate::game::MXB.exe));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_the_exe_says_where_to_fix_it() {
        let dir = temp_dir("empty");

        let mut cfg = AppConfig::default();
        cfg.game_path = dir.to_string_lossy().into_owned();
        let err = resolve_exe(&cfg).expect_err("no exe means no launch");
        let msg = format!("{err:#}");
        assert!(msg.contains(crate::game::MXB.exe), "names what's missing: {msg}");
        assert!(msg.contains("Settings"), "points at the fix: {msg}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole macOS launch, end to end, against a stub standing in for Wine.
    ///
    /// Everything up to the wrapper is ours and is exercised here: the prefix comes out of
    /// the exe's path, the runner override is honoured, the game folder becomes the working
    /// directory, and the connect flag reaches argv as two entries. Only whether Wine then
    /// runs a Windows binary is out of our hands.
    #[cfg(target_os = "macos")]
    #[test]
    fn launches_through_the_runner_with_the_prefix_and_the_connect_args() {
        let root = temp_dir("mac-launch");
        let game_dir = root.join("Bottles/MXB/drive_c/Program Files/MX Bikes");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join(crate::game::MXB.exe), b"stub").unwrap();

        // A stub "Wine" that records how it was called, so the assertion is on the real
        // spawn rather than on the plan we handed to it.
        let record = root.join("argv.txt");
        let runner = root.join("fake-wine");
        std::fs::write(
            &runner,
            format!(
                "#!/bin/sh\n{{ echo \"$WINEPREFIX\"; pwd; for a in \"$@\"; do echo \"$a\"; done; }} > {}\n",
                record.display()
            ),
        )
        .unwrap();
        std::process::Command::new("chmod").arg("+x").arg(&runner).status().unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = game_dir.to_string_lossy().into_owned();
        cfg.wine_runner = runner.to_string_lossy().into_owned();

        let outcome = join(&cfg, "203.0.113.10").expect("a stub runner is enough to launch");
        assert!(matches!(outcome, LaunchOutcome::Launched));

        // The child is spawned, not waited on — give it a moment to write.
        let mut written = String::new();
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&record) {
                if !text.is_empty() {
                    written = text;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.first().copied(),
            Some(root.join("Bottles/MXB").to_string_lossy().as_ref()),
            "WINEPREFIX is the folder above drive_c, not the game folder: {written:?}"
        );
        // `pwd` resolves symlinks, and macOS puts the temp dir behind `/private`.
        assert!(
            lines.get(1).is_some_and(|cwd| cwd.ends_with("drive_c/Program Files/MX Bikes")),
            "the game folder is the working directory: {written:?}"
        );
        assert_eq!(
            &lines[2..],
            [
                game_dir.join(crate::game::MXB.exe).to_string_lossy().as_ref(),
                "-directconnect",
                "203.0.113.10:54210",
            ],
            "the exe and the connect flag arrive as separate argv entries: {written:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// With no wrapper installed and none configured, Play has to say what to install —
    /// "supported on Windows and Linux only" was the old answer and is no longer true.
    #[cfg(target_os = "macos")]
    #[test]
    fn without_a_runner_the_error_names_the_wrappers_to_install() {
        let root = temp_dir("mac-no-runner");
        let game_dir = root.join("Bottles/MXB/drive_c/MX Bikes");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join(crate::game::MXB.exe), b"stub").unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = game_dir.to_string_lossy().into_owned();
        // Point at a runner that isn't there: auto-detection then decides, and this test
        // only holds on a machine with no wrapper installed — which is the case it covers.
        cfg.wine_runner = root.join("not-here").to_string_lossy().into_owned();

        if crate::winehost::resolve("", Some(&root.join("Bottles/MXB"))).is_none() {
            let msg = format!("{:#}", launch(&cfg).expect_err("no runner means no launch"));
            assert!(msg.contains("CrossOver"), "names a wrapper to install: {msg}");
            assert!(msg.contains("Wine"), "names a wrapper to install: {msg}");
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A native folder pick on a Mac isn't inside any prefix — say so, rather than
    /// failing later inside a wrapper that was never going to be involved.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_install_outside_a_bottle_is_named_as_the_problem() {
        let dir = temp_dir("mac-no-prefix");
        std::fs::write(dir.join(crate::game::MXB.exe), b"stub").unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = dir.to_string_lossy().into_owned();
        let msg = format!("{:#}", launch(&cfg).expect_err("no prefix means no launch"));
        assert!(msg.contains("drive_c"), "names what's missing from the path: {msg}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A range covers its own bytes and stops. This is the whole of what separates code the
    /// loader put in the process from code that arrived some other way, so it is worth
    /// pinning both ends of.
    #[test]
    fn a_module_covers_its_own_bytes_and_no_others() {
        let ranges = [
            ModuleRange { base: 0x1000, size: 0x100 },
            ModuleRange { base: 0x8000, size: 0x10 },
        ];
        assert!(covered(&ranges, 0x1000), "the first byte is inside");
        assert!(covered(&ranges, 0x10ff), "the last byte is inside");
        assert!(!covered(&ranges, 0x1100), "one past the end is outside");
        assert!(!covered(&ranges, 0x0fff), "one before the start is outside");
        assert!(covered(&ranges, 0x8008), "a second range is looked at too");
        assert!(!covered(&[], 0x1000), "nothing covers anything");
    }

    #[test]
    fn a_range_at_the_top_of_the_address_space_does_not_wrap() {
        let ranges = [ModuleRange { base: u64::MAX - 4, size: 0x100 }];
        assert!(covered(&ranges, u64::MAX));
        assert!(!covered(&ranges, 0));
    }

    #[test]
    fn a_page_that_can_execute_is_recognised_whatever_else_it_can_do() {
        assert!(is_executable(PAGE_EXECUTE));
        assert!(is_executable(PAGE_EXECUTE_READ));
        assert!(is_executable(PAGE_EXECUTE_READWRITE));
        assert!(is_executable(PAGE_EXECUTE_WRITECOPY));
        // PAGE_GUARD and PAGE_NOCACHE ride in the high bits; the protection is the low byte.
        assert!(is_executable(PAGE_EXECUTE_READ | 0x100));
        assert!(!is_executable(0x02), "read-only is not code");
        assert!(!is_executable(0x04), "read-write is not code");
    }

    #[test]
    fn only_committed_memory_is_memory() {
        assert!(is_committed(MEM_COMMIT));
        assert!(!is_committed(0x2000), "reserved is not backed by anything");
    }

    #[test]
    fn a_region_is_named_by_what_backs_it() {
        assert_eq!(region_kind(MEM_IMAGE), "image");
        assert_eq!(region_kind(MEM_MAPPED), "mapped");
        assert_eq!(region_kind(MEM_PRIVATE), "private");
        assert_eq!(region_kind(0), "other");
    }

    #[test]
    fn a_protection_reads_as_the_letters_it_means() {
        assert_eq!(protection(PAGE_EXECUTE_READ), "rx");
        assert_eq!(protection(PAGE_EXECUTE_READWRITE), "rwx");
        assert_eq!(protection(PAGE_EXECUTE_WRITECOPY), "rwxc");
        assert_eq!(protection(0x04), "rw");
        assert_eq!(protection(PAGE_EXECUTE_READWRITE | 0x100), "rwx", "guard bits are not it");
    }
}
