use serde::Serialize;

// Must match the event name in frostmod's launcher.cpp / frostmod.cpp exactly.
#[cfg(windows)]
const RELOAD_EVENT_NAME: &[u8] = b"Local\\FrostModReload\0";

// Non-Windows builds only construct `Unsupported`; silence the dead-code lint.
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

    // kernel32 is auto-linked on Windows.
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

/// Signal FrostMod to re-scan the mods folder. Best-effort.
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
    // Signal failed (e.g. FrostMod is elevated and we aren't) — treat as not usable.
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

// ===========================================================================
// Command channel — swap the active bike (offline, in-garage) via FrostMod.
//
// The reload event carries no payload, so a bike swap needs its own channel:
// mxb-app writes a small JSON command file, then signals a DEDICATED event so
// FrostMod can't confuse a swap with a mods rescan. FrostMod reads the file on
// wake and dispatches the swap on its render thread, where its own offline +
// in-garage guard runs (a rejected swap is logged there, not returned here —
// this side is fire-and-forget). Must match the reader in frostmod.cpp.
// ===========================================================================

/// Name of FrostMod's command event. Must match frostmod.cpp exactly.
#[cfg(windows)]
const COMMAND_EVENT_NAME: &[u8] = b"Local\\FrostModCommand\0";

/// Command file FrostMod reads when the command event fires. Same temp dir the
/// DLL uses — `std::env::temp_dir()` resolves to the `%TEMP%` that FrostMod's
/// `GetTempPathA` returns.
fn command_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("frostmod_cmd.json")
}

/// Serialize a swap-bike command. Kept pure (no I/O) so it can be unit-tested and
/// so the on-disk contract with frostmod.cpp is exercised without a game.
fn swap_command_json(bike_id: &str) -> String {
    serde_json::json!({ "verb": "swap_bike", "bikeId": bike_id }).to_string()
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapOutcome {
    /// Command file written and FrostMod signalled.
    Signaled,
    /// FrostMod isn't running (the command event doesn't exist).
    NotRunning,
    /// The command file couldn't be written.
    WriteFailed,
    /// Non-Windows dev build — can't talk to FrostMod.
    Unsupported,
}

/// Ask FrostMod to swap the active bike to `bike_id`. Writes the command file
/// first (so it's there before FrostMod wakes), then pulses the command event.
/// Best-effort: the actual offline/in-garage decision happens inside FrostMod.
#[cfg(windows)]
pub fn signal_swap_bike(bike_id: &str) -> SwapOutcome {
    if std::fs::write(command_file_path(), swap_command_json(bike_id)).is_err() {
        return SwapOutcome::WriteFailed;
    }
    // SAFETY: valid NUL-terminated ANSI name; null return means the event doesn't
    // exist (FrostMod not running) or access was denied.
    let handle =
        unsafe { ffi::OpenEventA(ffi::EVENT_MODIFY_STATE, 0, COMMAND_EVENT_NAME.as_ptr()) };
    if handle.is_null() {
        return SwapOutcome::NotRunning;
    }
    // SAFETY: `handle` is a valid event we just opened and close below.
    let ok = unsafe { ffi::SetEvent(handle) } != 0;
    unsafe { ffi::CloseHandle(handle) };
    if ok {
        SwapOutcome::Signaled
    } else {
        SwapOutcome::NotRunning
    }
}

#[cfg(not(windows))]
pub fn signal_swap_bike(_bike_id: &str) -> SwapOutcome {
    // Still write the command file on dev builds so the contract can be inspected.
    let _ = std::fs::write(command_file_path(), swap_command_json(_bike_id));
    SwapOutcome::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_command_json_shape_and_escaping() {
        assert_eq!(
            swap_command_json("MX2OEM_2023_KTM_250_SX-F"),
            r#"{"bikeId":"MX2OEM_2023_KTM_250_SX-F","verb":"swap_bike"}"#
        );
        // Ids are arbitrary folder names — ensure quotes/backslashes are escaped.
        assert_eq!(
            swap_command_json(r#"a"b\c"#),
            r#"{"bikeId":"a\"b\\c","verb":"swap_bike"}"#
        );
    }
}
