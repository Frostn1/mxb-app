//! FrostMod's command channel: the JSON file it reads and the event that wakes it.
//!
//! Moved here from MXB App so the Studio writes exactly the same document the same way. The
//! verbs, and which FrostMod release understands each, stay with the app that sends them —
//! see `apps/manager/src-tauri/src/frostmod.rs` for the verb table. Must match
//! `HandleFrostModCommand` in frostmod.cpp.

use serde::Serialize;

#[cfg(windows)]
mod ffi {
    use std::os::raw::c_void;

    pub type Handle = *mut c_void;

    extern "system" {
        pub fn OpenEventA(desired_access: u32, inherit_handle: i32, name: *const u8) -> Handle;
        pub fn SetEvent(handle: Handle) -> i32;
        pub fn CloseHandle(handle: Handle) -> i32;
    }

    pub const EVENT_MODIFY_STATE: u32 = 0x0002;
}

/// Name of FrostMod's command event. Must match frostmod.cpp exactly.
#[cfg(windows)]
const COMMAND_EVENT_NAME: &[u8] = b"Local\\FrostModCommand\0";

/// Where a build outside the Wine prefix leaves commands: FrostMod's own folder, which
/// MXB App owns and which FrostMod (v0.13.0+) reads as well as `%TEMP%`.
///
/// Set once at startup rather than passed in, because the senders are called from folder
/// watchers and install jobs that hold no Tauri handle to resolve a data dir with.
#[cfg(any(target_os = "linux", target_os = "macos"))]
static COMMAND_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Tell the sender where FrostMod is installed. Called once, from setup.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn set_command_dir(dir: std::path::PathBuf) {
    let _ = COMMAND_DIR.set(dir);
}

/// Command file FrostMod reads when the command event fires. Same temp dir the DLL uses —
/// `std::env::temp_dir()` resolves to the `%TEMP%` that FrostMod's `GetTempPathA` returns.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn command_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("frostmod_cmd.json")
}

/// Outside the prefix: FrostMod's folder, not our temp dir. `/tmp` here is not the `%TEMP%`
/// a program inside the Wine prefix resolves, and FrostMod's folder is one directory both
/// sides can name.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn command_file_path() -> std::path::PathBuf {
    COMMAND_DIR
        .get()
        .cloned()
        .unwrap_or_else(std::env::temp_dir)
        .join("frostmod_cmd.json")
}

/// Serialize a command. Pure, so the on-disk contract with frostmod.cpp is testable without
/// a game.
///
/// `at` is what makes two identical commands two different documents. Off Windows the file
/// *is* the signal, and FrostMod decides a command is new by comparing what it last acted on
/// with what is on disk now — without a stamp, pressing Reload twice would write the same
/// bytes twice and the second press would be indistinguishable from no press at all.
pub fn command_json_at(verb: &str, bike_id: &str, at: u128) -> String {
    serde_json::json!({ "verb": verb, "bikeId": bike_id, "at": at.to_string() }).to_string()
}

pub fn command_json(verb: &str, bike_id: &str) -> String {
    command_json_at(verb, bike_id, now_millis())
}

/// Milliseconds since the epoch, or 0 from a clock we can't read.
fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    /// Command file written and FrostMod signalled.
    Signaled,
    /// FrostMod isn't running (the command event doesn't exist).
    NotRunning,
    /// The command file couldn't be written.
    WriteFailed,
    /// Non-Windows dev build — can't talk to FrostMod.
    Unsupported,
    /// Deliberately not sent: the installed FrostMod isn't one we'll hand this verb to, or
    /// its version couldn't be read. Only the caller knows which verb it wanted and which
    /// release made it safe, so nothing in this module produces it.
    Withheld,
}

/// Write the command file (so it's there before FrostMod wakes), then pulse the command
/// event. Best-effort: FrostMod decides whether to act.
#[cfg(windows)]
pub fn write_and_signal(json: String) -> CommandOutcome {
    if std::fs::write(command_file_path(), json).is_err() {
        return CommandOutcome::WriteFailed;
    }
    // SAFETY: valid NUL-terminated ANSI name; null return means the event doesn't exist
    // (FrostMod not running) or access was denied.
    let handle =
        unsafe { ffi::OpenEventA(ffi::EVENT_MODIFY_STATE, 0, COMMAND_EVENT_NAME.as_ptr()) };
    if handle.is_null() {
        return CommandOutcome::NotRunning;
    }
    // SAFETY: `handle` is a valid event we just opened and close below.
    let ok = unsafe { ffi::SetEvent(handle) } != 0;
    unsafe { ffi::CloseHandle(handle) };
    if ok {
        CommandOutcome::Signaled
    } else {
        CommandOutcome::NotRunning
    }
}

/// Write the command file and nothing else — off Windows, where the file is the whole
/// signal. Whether FrostMod is running is the caller's question to ask first.
pub fn write_command_file(json: &str) -> CommandOutcome {
    let path = command_file_path();
    match std::fs::write(&path, json) {
        Ok(()) => CommandOutcome::Signaled,
        Err(e) => {
            log::warn!("couldn't write the FrostMod command file {}: {e}", path.display());
            CommandOutcome::WriteFailed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape() {
        assert_eq!(
            command_json_at("reload_bike_gfx", "MX2OEM_2023_KTM_250_SX-F", 7),
            r#"{"at":"7","bikeId":"MX2OEM_2023_KTM_250_SX-F","verb":"reload_bike_gfx"}"#
        );
    }
}
