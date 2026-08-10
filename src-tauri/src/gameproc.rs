use serde::Serialize;

use crate::config::AppConfig;

/// Customization loader `fcn.1400ecd00` minus PE image base `0x140000000`.
///
/// An `mxbikes.exe` offset, so it is only ever used when MX Bikes is the active game —
/// see [`crate::game::Caps::instant_refresh`], which gates every path that reaches here.
#[cfg(windows)]
const LOADER_OFFSET: usize = 0x000e_cd00;

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
        pub fn GetCurrentProcessId() -> u32;
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
                if ffi::field_eq_ignore_case(&entry.sz_exe_file, crate::game::active().exe) {
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

/// Under Wine the game is an ordinary macOS process whose argv still names the exe, so
/// `ps` finds it. Without this Play would cheerfully start a second copy.
#[cfg(target_os = "macos")]
pub fn is_game_running() -> bool {
    let Ok(out) = std::process::Command::new("ps").args(["-Ao", "pid=,args="]).output() else {
        return false;
    };
    crate::winehost::running_exe(
        &String::from_utf8_lossy(&out.stdout),
        crate::game::active().exe,
        std::process::id(),
    )
}

#[cfg(not(any(windows, target_os = "macos")))]
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

/// Start MX Bikes, unless it's already running.
///
/// Windows runs the exe directly rather than going through `steam://`: that works for
/// standalone (non-Steam) copies too, and doesn't need Steam to be up. Under Proton the
/// exe isn't ours to spawn — Steam has to set up the prefix — so Linux hands the Steam
/// URL to the desktop instead. macOS runs the exe too, but through the Wine wrapper that
/// owns the prefix it lives in (see [`crate::winehost`]).
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

    #[cfg(windows)]
    {
        // No CREATE_NO_WINDOW here (unlike the headless FrostMod child) — the game
        // draws its own window and hiding the console would gain nothing.
        let mut cmd = std::process::Command::new(&exe);
        cmd.current_dir(&dir);
        if let Some(addr) = address {
            cmd.args(connect_args(addr));
        }
        cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Couldn't start {}: {e}", exe.display()))?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(target_os = "linux")]
    {
        let _ = dir;
        // Steam takes launch arguments after a `//` separator. Under Proton this is the
        // only way to reach the exe's argv, since Steam — not us — spawns it.
        let appid = cfg.game().steam_appid;
        let url = match address {
            Some(addr) => format!("steam://rungameid/{appid}//{}", connect_args(addr).join(" ")),
            None => format!("steam://rungameid/{appid}"),
        };
        std::process::Command::new("xdg-open").arg(&url).spawn().map_err(|e| {
            anyhow::anyhow!("Couldn't ask Steam to launch {} ({url}): {e}", cfg.game().display)
        })?;
        Ok(LaunchOutcome::Launched)
    }

    #[cfg(target_os = "macos")]
    {
        // The game is a Windows binary here, so it runs inside a Wine prefix. The prefix
        // is whatever sits above `drive_c` in the exe's own path — see `winehost`.
        let (prefix, _) = crate::winehost::split_prefix(&exe).ok_or_else(|| {
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
}
