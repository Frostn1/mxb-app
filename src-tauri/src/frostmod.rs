//! Talk to FrostMod (https://github.com/Frostn1/frostmod) when it's running.
//!
//! FrostMod live-reloads MX Bikes' content folders on demand. Its console
//! (`frostmod.exe`) and its injected `frostmod.dll` coordinate through a named,
//! auto-reset Windows event, `Local\FrostModReload`: pressing `R` in the console
//! is literally just `SetEvent` on that handle, and the DLL's render loop polls
//! it and re-scans the mods folder. The name lives in the per-session `Local\`
//! namespace, so any process in the same logon session — including this app —
//! can open and signal it. That lets us trigger a live reload right after we
//! drop a new `.pkz` into the mods folder, with no changes to FrostMod.
//!
//! Everything here is best-effort: if FrostMod isn't running the event simply
//! doesn't exist (open fails → `NotRunning`) and installs are unaffected.

use serde::Serialize;

/// The event FrostMod's console and DLL both create/wait on. Must match the
/// name in frostmod's launcher.cpp / frostmod.cpp exactly.
#[cfg(windows)]
const RELOAD_EVENT_NAME: &[u8] = b"Local\\FrostModReload\0";

/// Outcome of asking FrostMod to reload — surfaced to the UI so it can tell the
/// user whether the new mod is already live or needs a manual reload.
// On non-Windows dev builds only `Unsupported` is ever constructed; the other
// variants are live on Windows, so silence the platform-specific dead-code lint.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReloadOutcome {
    /// FrostMod was running and we signalled it to reload.
    Signaled,
    /// FrostMod isn't running (the event doesn't exist).
    NotRunning,
    /// This platform can't talk to FrostMod (non-Windows dev builds).
    Unsupported,
}

#[cfg(windows)]
mod ffi {
    use std::os::raw::c_void;

    pub type Handle = *mut c_void;

    // kernel32 is auto-linked on Windows; declaring these avoids pulling in a
    // whole win32 crate just for three calls.
    extern "system" {
        pub fn OpenEventA(desired_access: u32, inherit_handle: i32, name: *const u8) -> Handle;
        pub fn SetEvent(handle: Handle) -> i32;
        pub fn CloseHandle(handle: Handle) -> i32;
    }

    /// Right to `SetEvent`/`ResetEvent` — all we need to poke the reload event.
    pub const EVENT_MODIFY_STATE: u32 = 0x0002;
}

/// Open FrostMod's reload event, returning a live handle if it exists.
#[cfg(windows)]
fn open_reload_event() -> ffi::Handle {
    // SAFETY: passing a valid NUL-terminated ANSI name; a null return just means
    // the event doesn't exist (FrostMod not running) or access was denied.
    unsafe { ffi::OpenEventA(ffi::EVENT_MODIFY_STATE, 0, RELOAD_EVENT_NAME.as_ptr()) }
}

/// Signal FrostMod to re-scan the mods folder (equivalent to pressing `R` in its
/// console or picking Reload from the in-game F8 menu). Best-effort.
#[cfg(windows)]
pub fn signal_reload() -> ReloadOutcome {
    let handle = open_reload_event();
    if handle.is_null() {
        return ReloadOutcome::NotRunning;
    }
    // SAFETY: `handle` is a valid event handle we just opened; we own it and
    // close it below.
    let ok = unsafe { ffi::SetEvent(handle) } != 0;
    unsafe { ffi::CloseHandle(handle) };
    // A signal failure here means FrostMod exists but we couldn't poke it (e.g.
    // it's running elevated and we aren't, so the mandatory label blocks the
    // write) — treat it like "not usable right now" so the UI prompts a manual
    // reload rather than falsely claiming success.
    if ok {
        ReloadOutcome::Signaled
    } else {
        ReloadOutcome::NotRunning
    }
}

/// Is FrostMod currently running? (Can we open its reload event?)
#[cfg(windows)]
pub fn is_running() -> bool {
    let handle = open_reload_event();
    if handle.is_null() {
        return false;
    }
    unsafe { ffi::CloseHandle(handle) };
    true
}

#[cfg(not(windows))]
pub fn signal_reload() -> ReloadOutcome {
    ReloadOutcome::Unsupported
}

#[cfg(not(windows))]
pub fn is_running() -> bool {
    false
}
