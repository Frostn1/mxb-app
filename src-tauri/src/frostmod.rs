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
// Command channel — payload-carrying commands to FrostMod.
//
// The reload event carries no payload, so anything needing an argument (a bike
// id) needs its own channel: mxb-app writes a small JSON command file, then
// signals a DEDICATED event so FrostMod can't confuse a command with a mods
// rescan. FrostMod reads the file on wake and dispatches on its render thread
// (refusals are logged there, not returned here — this side is fire-and-forget).
// Must match `HandleFrostModCommand` in frostmod.cpp, verb names included.
//
// Verbs:
//   `refresh_bike_model` — re-apply the named bike so a just-swapped model shows
//       in the garage without the class-switch away-and-back. FrostMod no-ops
//       unless that bike is the one currently selected.
//   `swap_bike` — switch the active bike outright. NOT implemented in FrostMod
//       yet (Stage B); it logs and ignores.
//
// A verb the running FrostMod predates is logged as unknown and dropped, which
// looks exactly like success from this side — see `supports_model_refresh`.
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

/// Serialize a command. Kept pure (no I/O) so it can be unit-tested and so the
/// on-disk contract with frostmod.cpp is exercised without a game.
fn command_json(verb: &str, bike_id: &str) -> String {
    serde_json::json!({ "verb": verb, "bikeId": bike_id }).to_string()
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
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
    /// Sent, but the installed FrostMod predates the verb — it logs "unknown verb"
    /// and does nothing. Only the caller knows which verb it sent and which release
    /// introduced it, so this is never produced by `send_command` itself.
    TooOld,
}

/// Write the command file (so it's there before FrostMod wakes), then pulse the
/// command event. Best-effort: FrostMod decides whether to act.
#[cfg(windows)]
fn send_command(json: String) -> CommandOutcome {
    if std::fs::write(command_file_path(), json).is_err() {
        return CommandOutcome::WriteFailed;
    }
    // SAFETY: valid NUL-terminated ANSI name; null return means the event doesn't
    // exist (FrostMod not running) or access was denied.
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

#[cfg(not(windows))]
fn send_command(json: String) -> CommandOutcome {
    // Still write the command file on dev builds so the contract can be inspected.
    let _ = std::fs::write(command_file_path(), json);
    CommandOutcome::Unsupported
}

/// Ask FrostMod to swap the active bike to `bike_id`.
/// NOTE: FrostMod does not implement this verb yet — it logs and ignores it.
pub fn signal_swap_bike(bike_id: &str) -> CommandOutcome {
    send_command(command_json("swap_bike", bike_id))
}

/// Ask FrostMod to re-apply `bike_id` so a just-swapped model shows in the garage
/// straight away. A no-op inside FrostMod unless that bike is the selected one, so
/// this is safe to fire after every model swap.
pub fn signal_refresh_model(bike_id: &str) -> CommandOutcome {
    send_command(command_json("refresh_bike_model", bike_id))
}

/// The FrostMod release that added the command listener and `refresh_bike_model`.
/// Anything older creates no `Local\FrostModCommand` event at all, so the send comes
/// back `NotRunning` — but a *future* verb on a listening-but-older FrostMod would be
/// taken and dropped, which is the case this gate really guards.
pub const MODEL_REFRESH_MIN_VERSION: &str = "v0.9.9";

/// Parse a release tag (`v0.9.9`, `0.10.0`, `v1.0.0-rc1`) into comparable parts.
/// Anything unparseable is `None` — we then assume support rather than cry wolf.
fn version_parts(tag: &str) -> Option<(u32, u32, u32)> {
    let core = tag.trim().trim_start_matches(['v', 'V']);
    // Drop any pre-release/build tail: `0.9.9-rc1` is 0.9.9 for our purposes, since
    // the verb either exists in that build or it doesn't.
    let core = core.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    // A tag may be `v1` or `v1.2` — a missing part is zero, not a parse failure.
    let minor = it.next().map_or(Some(0), |s| s.parse().ok())?;
    let patch = it.next().map_or(Some(0), |s| s.parse().ok())?;
    Some((major, minor, patch))
}

/// Does the FrostMod release tagged `tag` understand `refresh_bike_model`?
///
/// A tag we can't read counts as supported: `version.txt` is written by our own
/// installer, so an unreadable one means something unusual, and warning "update
/// FrostMod" at a user who already has the newest build is worse than staying quiet.
pub fn supports_model_refresh(tag: &str) -> bool {
    match (version_parts(tag), version_parts(MODEL_REFRESH_MIN_VERSION)) {
        (Some(have), Some(min)) => have >= min,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_command_json_shape_and_escaping() {
        assert_eq!(
            command_json("swap_bike", "MX2OEM_2023_KTM_250_SX-F"),
            r#"{"bikeId":"MX2OEM_2023_KTM_250_SX-F","verb":"swap_bike"}"#
        );
        // Ids are arbitrary folder names — ensure quotes/backslashes are escaped.
        // frostmod.cpp's JsonStringField unescapes \" and \\ to match.
        assert_eq!(
            command_json("swap_bike", r#"a"b\c"#),
            r#"{"bikeId":"a\"b\\c","verb":"swap_bike"}"#
        );
    }

    #[test]
    fn the_refresh_verb_needs_the_release_that_introduced_it() {
        assert!(supports_model_refresh("v0.9.9"));
        assert!(supports_model_refresh("0.9.9")); // tags are written with the v, but be lenient
        assert!(!supports_model_refresh("v0.9.8"));
        assert!(!supports_model_refresh("v0.8.12"));
    }

    #[test]
    fn versions_compare_by_number_not_by_string() {
        // The trap: "v0.9.10" < "v0.9.9" lexically, and "v0.10.0" < "v0.9.9" too.
        assert!(supports_model_refresh("v0.9.10"));
        assert!(supports_model_refresh("v0.10.0"));
        assert!(supports_model_refresh("v1.0.0"));
    }

    #[test]
    fn a_prerelease_of_the_minimum_counts_as_the_minimum() {
        // Our own release flow tags pre-releases with a `-` suffix (see release.yml),
        // and such a build carries the verb.
        assert!(supports_model_refresh("v0.9.9-rc1"));
        assert!(!supports_model_refresh("v0.9.8-rc1"));
    }

    #[test]
    fn an_unreadable_tag_is_given_the_benefit_of_the_doubt() {
        // Better silent than telling someone on the newest build to update.
        assert!(supports_model_refresh(""));
        assert!(supports_model_refresh("nightly"));
        assert!(supports_model_refresh("v-broken-"));
    }

    #[test]
    fn short_tags_fill_the_missing_parts_with_zero() {
        assert!(supports_model_refresh("v1"));
        assert!(!supports_model_refresh("v0.9"));
    }

    #[test]
    fn refresh_command_json_uses_the_verb_frostmod_dispatches_on() {
        // The verb string is the contract with HandleFrostModCommand in frostmod.cpp.
        assert_eq!(
            command_json("refresh_bike_model", "MX1OEM_1996_Honda_CR250"),
            r#"{"bikeId":"MX1OEM_1996_Honda_CR250","verb":"refresh_bike_model"}"#
        );
    }
}
