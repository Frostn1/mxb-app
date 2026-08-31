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

/// Linux and macOS: FrostMod runs inside a Wine prefix — Proton's on Linux, a
/// CrossOver/Whisky bottle on macOS — and its reload event is a Wine kernel object we have
/// no way to open from out here, where this app is a native process outside that prefix.
/// The command file is the way in instead (see `send_command`), and `reload_mods` is the
/// verb FrostMod v0.13.0 added for exactly this. A FrostMod too old to read that file is
/// refused a start at all ([`reads_command_files`]), so anything running here can hear us.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn signal_reload() -> ReloadOutcome {
    match send_command(command_json("reload_mods", "")) {
        CommandOutcome::Signaled => ReloadOutcome::Signaled,
        CommandOutcome::NotRunning => ReloadOutcome::NotRunning,
        // A command we couldn't write is a reload that won't happen, and saying "not
        // running" about a FrostMod that is running would send the player looking in the
        // wrong place — but there is no truer answer in this enum, and the write failure
        // is logged where it happens.
        _ => ReloadOutcome::NotRunning,
    }
}

/// Linux: the launcher is a Windows process inside the prefix, so the process table is
/// where we can see it — its reload event isn't reachable from this side.
#[cfg(target_os = "linux")]
pub fn is_running() -> bool {
    crate::proton::running_exe("frostmod.exe")
}

/// macOS: the same answer from the same place, asked of `ps` rather than `/proc`. Under
/// Wine the launcher is a real macOS process whose argv still names `frostmod.exe`.
#[cfg(target_os = "macos")]
pub fn is_running() -> bool {
    crate::winehost::running_exe(
        &crate::winehost::process_table(),
        "frostmod.exe",
        std::process::id(),
    )
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn signal_reload() -> ReloadOutcome {
    ReloadOutcome::Unsupported
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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
//   `refresh_bike_model` — the named bike's model changed on disk. FrostMod logs it and
//       puts a notice on screen; it does NOT re-apply the bike (v0.9.11 removed that —
//       the replay crashed the game). The player has to switch bike category away and
//       back. NOTE: FrostMod's own notice still says "re-select the bike", which isn't
//       enough — fix it there.
//   `swap_bike` — switch the active bike outright. NOT implemented in FrostMod
//       yet (Stage B); it logs and ignores.
//
// A verb the running FrostMod predates is logged as unknown and dropped, which
// looks exactly like success from this side — see `supports_model_refresh`.
// ===========================================================================

/// Name of FrostMod's command event. Must match frostmod.cpp exactly.
#[cfg(windows)]
const COMMAND_EVENT_NAME: &[u8] = b"Local\\FrostModCommand\0";

/// Where a build outside the Wine prefix leaves commands: FrostMod's own folder, which
/// this app owns and which FrostMod (v0.13.0+) reads as well as `%TEMP%`.
///
/// Set once at startup rather than passed in, because the senders below are called from
/// folder watchers and install jobs that hold no Tauri handle to resolve a data dir with,
/// and threading one through every caller would buy nothing: there is only ever one
/// FrostMod folder per run.
#[cfg(any(target_os = "linux", target_os = "macos"))]
static COMMAND_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Tell the sender where FrostMod is installed. Called once, from setup.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn set_command_dir(dir: std::path::PathBuf) {
    let _ = COMMAND_DIR.set(dir);
}

/// Command file FrostMod reads when the command event fires. Same temp dir the
/// DLL uses — `std::env::temp_dir()` resolves to the `%TEMP%` that FrostMod's
/// `GetTempPathA` returns.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn command_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("frostmod_cmd.json")
}

/// Outside the prefix: FrostMod's folder, not our temp dir. `/tmp` here is not the `%TEMP%`
/// a program inside the Wine prefix resolves, and FrostMod's folder is one directory both
/// sides can name — it reads the file beside its own module, which is `Z:\…` from in there.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn command_file_path() -> std::path::PathBuf {
    COMMAND_DIR
        .get()
        .cloned()
        .unwrap_or_else(std::env::temp_dir)
        .join("frostmod_cmd.json")
}

/// Serialize a command. Kept pure (no I/O) so it can be unit-tested and so the
/// on-disk contract with frostmod.cpp is exercised without a game.
///
/// `at` is what makes two identical commands two different documents. It costs nothing on
/// Windows, where an event says "read this now" — but off it the file *is* the signal,
/// and FrostMod decides a command is new by comparing what it last acted on with what is
/// on disk now. Without a stamp, pressing Reload twice would write the same bytes twice
/// and the second press would be indistinguishable from no press at all.
fn command_json_at(verb: &str, bike_id: &str, at: u128) -> String {
    serde_json::json!({ "verb": verb, "bikeId": bike_id, "at": at.to_string() }).to_string()
}

fn command_json(verb: &str, bike_id: &str) -> String {
    command_json_at(verb, bike_id, now_millis())
}

/// Milliseconds since the epoch, or 0 from a clock we can't read — a stamp that never
/// moves is no worse than the no-stamp behaviour it replaced.
fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
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
    /// Deliberately not sent: the installed FrostMod isn't one we'll hand this verb to
    /// (too old to understand it, or old enough to mishandle it — see
    /// `MODEL_REFRESH_MIN_VERSION`), or its version couldn't be read at all. Only the
    /// caller knows which verb it wanted and which release made it safe, so this is
    /// never produced by `send_command` itself.
    Withheld,
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

/// Linux and macOS: the file is the whole signal. There is no event to pulse —
/// `Local\FrostModCommand` belongs to the Wine prefix FrostMod runs in, and this process is
/// outside it — so the write lands in FrostMod's folder and its poll picks it up within
/// about a fifth of a second. Whether it's running is asked of the process table first, so
/// a command isn't left on disk for a FrostMod that will read it at some unrelated future
/// start-up.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn send_command(json: String) -> CommandOutcome {
    if !is_running() {
        return CommandOutcome::NotRunning;
    }
    let path = command_file_path();
    match std::fs::write(&path, json) {
        Ok(()) => CommandOutcome::Signaled,
        Err(e) => {
            log::warn!("couldn't write the FrostMod command file {}: {e}", path.display());
            CommandOutcome::WriteFailed
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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

/// Tell FrostMod that `bike_id`'s model changed on disk. It answers with an in-game
/// notice; the mesh does not reload on its own, and only a bike-category switch away
/// and back re-reads it (see the verb table above).
///
/// NOT safe to fire at every FrostMod — see `model_refresh_is_safe`, which every
/// caller must clear first.
pub fn signal_refresh_model(bike_id: &str) -> CommandOutcome {
    send_command(command_json("refresh_bike_model", bike_id))
}

/// The oldest FrostMod we will send `refresh_bike_model` to.
///
/// This is a **safety** floor, not a capability floor. v0.9.9 introduced the verb and
/// handles it — badly: it replays the game's captured bike-apply call with a descriptor
/// it does not own, never validates the object that call mutates, and swallows the
/// resulting access violation, so the game keeps running on a half-swapped machine and
/// crashes to desktop at the *next* bike the player selects by hand. Withholding the
/// command is the only fix available from this side; v0.9.11 is the release that stops
/// replaying. Raise this again if a later FrostMod changes how the verb behaves.
///
/// Why v0.9.11 and not v0.9.10: a `v0.9.10-rc1` was published from a branch that never
/// removed the replay, and its own `version.h` reads 0.9.10. Since this gate compares
/// numerically, a v0.9.10 floor would read that pre-release as new enough and hand it the
/// verb that crashes it. FrostMod took the next number to put itself unambiguously above
/// that tag; the two constants must stay in step.
pub const MODEL_REFRESH_MIN_VERSION: &str = "v0.9.11";

/// Parse a release tag (`v0.9.9`, `0.10.0`, `v1.0.0-rc1`) into comparable parts.
/// `None` when the tag isn't a version we can read.
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

/// May we send `refresh_bike_model` to the installed FrostMod, tagged `tag`?
///
/// A tag we can't read — absent `version.txt`, or one holding something that isn't a
/// version — counts as **unsafe**. That inverts the old rule, which gave an unreadable
/// tag the benefit of the doubt because the cost of being wrong was an over-cautious
/// toast. The cost is now a crash to desktop (see `MODEL_REFRESH_MIN_VERSION`), so an
/// unknown build is one we don't poke; the player re-selects the bike instead.
pub fn model_refresh_is_safe(tag: Option<&str>) -> bool {
    match (tag.and_then(version_parts), version_parts(MODEL_REFRESH_MIN_VERSION)) {
        (Some(have), Some(min)) => have >= min,
        _ => false,
    }
}

/// The oldest FrostMod that can be driven from outside a Wine prefix — Linux and macOS.
///
/// Not a safety floor like the two around it — a capability one. Every FrostMod before
/// v0.13.0 reads its command file only when the command *event* fires, and that event is a
/// Wine kernel object: a native app out here can't pulse it, so an older build under Proton
/// or in a bottle would take every write and never look. It would inject fine and reload on
/// `F8`, and every button in this app would quietly do nothing. v0.13.0 is the release that
/// polls the file, so off Windows it is the floor for starting FrostMod at all.
// Off-Windows question: on Windows the event channel is used instead.
#[cfg_attr(windows, allow(dead_code))]
pub const FILE_CHANNEL_MIN_VERSION: &str = "v0.13.0";

/// Does the installed FrostMod, tagged `tag`, read commands from a file?
///
/// An unreadable tag counts as no — same reasoning as the floors around it, with a milder
/// cost: the player is told to update rather than left with buttons that do nothing.
#[cfg_attr(windows, allow(dead_code))]
pub fn reads_command_files(tag: Option<&str>) -> bool {
    match (tag.and_then(version_parts), version_parts(FILE_CHANNEL_MIN_VERSION)) {
        (Some(have), Some(min)) => have >= min,
        _ => false,
    }
}

/// The oldest FrostMod that is safe to run against GP Bikes.
///
/// Same shape as [`MODEL_REFRESH_MIN_VERSION`], and the same kind of reason. FrostMod
/// v0.10.0 is the first build that attaches to `gpbikes.exe` at all — but its reload ran
/// MX Bikes' step table inside GP Bikes, calling arbitrary functions and zeroing arbitrary
/// globals, which crashed the game on the first reload. v0.11.0 gives GP Bikes its own
/// table and refuses the reload outright for any title that has none.
///
/// So the floor is not "does this build know about GP Bikes" (v0.10.0 does) but "will this
/// build avoid taking the game down" — which starts at v0.11.0.
pub const GPB_MIN_VERSION: &str = "v0.11.0";

/// Is the installed FrostMod, tagged `tag`, safe to offer for `game`?
///
/// MX Bikes is unaffected: every FrostMod that ever shipped was built for it. An
/// unreadable tag is treated as unsafe for the same reason as `model_refresh_is_safe` —
/// the cost of guessing wrong is a crash to desktop, not an over-cautious toast.
pub fn supported_for_game(game: crate::game::Game, tag: Option<&str>) -> bool {
    let min = match game {
        crate::game::Game::Mxb => return true,
        crate::game::Game::Gpb => GPB_MIN_VERSION,
    };
    match (tag.and_then(version_parts), version_parts(min)) {
        (Some(have), Some(floor)) => have >= floor,
        _ => false,
    }
}

// ===========================================================================
// Did it actually get in?
//
// `is_running` answers "is FrostMod up?", which everything so far has treated as "is
// FrostMod working?". They come apart in one important case: the game running at a higher
// integrity level than the app. FrostMod is launched by us, so it inherits our level, and
// an injector cannot open a process above it — the DLL never goes in, the pill in the game
// never appears, and the app cheerfully reports FrostMod as running the whole time.
//
// So ask the game instead. A DLL that is in the process is in its module list however it
// got there, and the walk that reads that list is refused by exactly the same barrier that
// refuses the injection — which makes "we can't look" not a gap in the answer but the
// answer itself.
// ===========================================================================

/// The DLL FrostMod injects. Matched by file name: wherever it was loaded from, its being
/// in the game's module list is what "attached" means.
const INJECTED_DLL: &str = "frostmod.dll";

/// How long to give an injection before calling it a failure.
///
/// FrostMod polls for the game and injects when it sees it, so there is always a window
/// where the game is up and the DLL legitimately isn't in yet. Complaining inside that
/// window would fire a warning on every single launch.
const ATTACH_GRACE: std::time::Duration = std::time::Duration::from_secs(15);

/// When we first saw a game with no FrostMod in it, for [`ATTACH_GRACE`]. Cleared whenever
/// the answer is anything else, so each game session gets its own grace period.
static WAITING_SINCE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Whether FrostMod's DLL is inside the running game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachState {
    /// No game running, so nothing to be attached to.
    GameNotRunning,
    /// `frostmod.dll` is in the game.
    Attached,
    /// The game is up, the DLL isn't in yet, and it hasn't been long enough to worry.
    Attaching,
    /// The game is up, FrostMod's DLL is not in it, and it has had time to be.
    NotAttached,
    /// Windows won't let us see inside the game — and won't let FrostMod in either.
    Blocked,
    /// This platform can't answer the question.
    Unknown,
}

/// The attach answer, plus what to do about it when it's bad news.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub state: AttachState,
    /// What is wrong and how to fix it. Empty unless the state calls for it — the good
    /// states are their own explanation.
    pub reason: String,
}

impl Attachment {
    fn plain(state: AttachState) -> Self {
        Self { state, reason: String::new() }
    }
}

/// Is this module path FrostMod's injected DLL?
fn is_injected_dll(path: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(INJECTED_DLL))
}

/// Decide how long a game has been sitting there without FrostMod in it.
///
/// Split from [`attachment`] so the grace period is testable without a game: `now` is the
/// clock, and the `Option` is the stored "first seen like this" the caller keeps.
fn waiting_verdict(
    first_seen: &mut Option<std::time::Instant>,
    now: std::time::Instant,
) -> AttachState {
    let since = *first_seen.get_or_insert(now);
    if now.duration_since(since) >= ATTACH_GRACE {
        AttachState::NotAttached
    } else {
        AttachState::Attaching
    }
}

/// Is FrostMod actually in the game — and if not, why not?
pub fn attachment() -> Attachment {
    use crate::gameproc::GameModules;

    // Every answer but "the game is up and FrostMod isn't in it yet" ends the wait, so a
    // later session that comes back around to that state gets a fresh grace period rather
    // than inheriting the last one's — which would have it warn instantly.
    let forget_the_wait = || {
        if let Ok(mut slot) = WAITING_SINCE.lock() {
            *slot = None;
        }
    };

    match crate::gameproc::game_modules() {
        GameModules::NotRunning => {
            forget_the_wait();
            Attachment::plain(AttachState::GameNotRunning)
        }
        GameModules::Unavailable => {
            forget_the_wait();
            Attachment::plain(AttachState::Unknown)
        }
        GameModules::Denied => {
            forget_the_wait();
            Attachment { state: AttachState::Blocked, reason: blocked_reason() }
        }
        GameModules::Loaded(paths) if paths.iter().any(|p| is_injected_dll(p)) => {
            forget_the_wait();
            Attachment::plain(AttachState::Attached)
        }
        GameModules::Loaded(_) => {
            let now = std::time::Instant::now();
            let state = match WAITING_SINCE.lock() {
                Ok(mut slot) => waiting_verdict(&mut slot, now),
                // A poisoned lock must not be what turns into a false alarm.
                Err(_) => AttachState::Attaching,
            };
            let reason = match state {
                AttachState::NotAttached => not_attached_reason(),
                _ => String::new(),
            };
            Attachment { state, reason }
        }
    }
}

/// What to tell someone whose game we aren't allowed to look inside.
///
/// Both ways out are offered because either genuinely works, and which one is right isn't
/// ours to decide — running the game unelevated is the better habit, running the app
/// elevated is the quicker fix.
fn blocked_reason() -> String {
    let game = crate::game::active().display;
    match crate::gameproc::we_are_elevated() {
        // The ordinary case, and the one worth naming outright.
        Some(false) => format!(
            "{game} is running as administrator and MXB App isn't, so FrostMod can't get \
             into it — no in-game pill, no live reloads, no model swaps. Close {game} and \
             start it without administrator, or run MXB App as administrator too, then \
             launch the game again."
        ),
        // We are the elevated one, or Windows wouldn't say. Either way "run as admin" is
        // no longer advice we can give straight-faced, so describe the shape of the fix.
        _ => format!(
            "Windows won't let MXB App see inside {game}, so FrostMod can't get into it \
             either — no in-game pill, no live reloads, no model swaps. {game} and MXB App \
             have to run at the same level: either both as administrator, or neither."
        ),
    }
}

/// The game is readable, FrostMod is not in it, and the grace period is up.
fn not_attached_reason() -> String {
    let game = crate::game::active().display;
    format!(
        "FrostMod is running but hasn't got into {game} — no in-game pill, no live reloads, \
         no model swaps. Stop FrostMod and start it again; if it keeps happening, close \
         {game} first so FrostMod is up before the game is."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_command_json_shape_and_escaping() {
        assert_eq!(
            command_json_at("swap_bike", "MX2OEM_2023_KTM_250_SX-F", 1_723_600_000_000),
            r#"{"at":"1723600000000","bikeId":"MX2OEM_2023_KTM_250_SX-F","verb":"swap_bike"}"#
        );
        // Ids are arbitrary folder names — ensure quotes/backslashes are escaped.
        // frostmod.cpp's JsonStringField unescapes \" and \\ to match.
        assert_eq!(
            command_json_at("swap_bike", r#"a"b\c"#, 1),
            r#"{"at":"1","bikeId":"a\"b\\c","verb":"swap_bike"}"#
        );
    }

    /// What the stamp is for: on Linux the file is the signal, and FrostMod tells a new
    /// command from an old one by comparing bytes. Two presses of the same button have to
    /// come out as two different documents or the second one never happens.
    #[test]
    fn the_same_command_twice_is_two_different_documents() {
        assert_ne!(
            command_json_at("reload_mods", "", 1_723_600_000_000),
            command_json_at("reload_mods", "", 1_723_600_000_001),
        );
    }

    /// The floor exists because an older FrostMod fails *silently* off Windows: it takes
    /// the write, never polls, and every button in the app looks like it worked.
    #[test]
    fn only_a_frostmod_that_polls_is_driven_from_outside_the_prefix() {
        assert!(reads_command_files(Some("v0.13.0")));
        assert!(reads_command_files(Some("v0.13.1")));
        assert!(reads_command_files(Some("v1.0.0")));
        assert!(!reads_command_files(Some("v0.12.1")));
        // The numeric-not-lexical trap again: "v0.9.11" sorts above "v0.13.0" as a string.
        assert!(!reads_command_files(Some("v0.9.11")));
        assert!(!reads_command_files(None));
        assert!(!reads_command_files(Some("nightly")));
    }

    #[test]
    fn the_release_that_replays_unsafely_is_withheld_from() {
        // v0.9.9 has the verb and crashes the game with it — the floor is above it.
        assert!(!model_refresh_is_safe(Some("v0.9.9")));
        assert!(!model_refresh_is_safe(Some("v0.9.8")));
        assert!(!model_refresh_is_safe(Some("v0.8.12")));
        assert!(model_refresh_is_safe(Some("v0.9.11")));
        assert!(model_refresh_is_safe(Some("0.9.11"))); // tags carry the v, but be lenient
    }

    #[test]
    fn the_0_9_10_that_still_replays_is_withheld_from() {
        // A v0.9.10-rc1 was published from a branch that never removed the replay, which
        // is why the floor is 0.9.11 and not 0.9.10. Sending to it crashes the game, so
        // this is the case the floor exists to exclude — not a version-parsing nicety.
        assert!(!model_refresh_is_safe(Some("v0.9.10-rc1")));
        assert!(!model_refresh_is_safe(Some("v0.9.10")));
    }

    #[test]
    fn versions_compare_by_number_not_by_string() {
        // The trap: "v0.9.11" < "v0.9.9" lexically, and "v0.10.0" < "v0.9.9" too.
        assert!(model_refresh_is_safe(Some("v0.9.11")));
        assert!(model_refresh_is_safe(Some("v0.10.0")));
        assert!(model_refresh_is_safe(Some("v1.0.0")));
    }

    #[test]
    fn a_prerelease_of_the_minimum_counts_as_the_minimum() {
        // Our own release flow tags pre-releases with a `-` suffix (see release.yml),
        // and such a build carries the fix.
        assert!(model_refresh_is_safe(Some("v0.9.11-rc1")));
        assert!(!model_refresh_is_safe(Some("v0.9.9-rc1")));
    }

    #[test]
    fn a_version_we_cannot_read_is_withheld_from() {
        // Inverted deliberately: the old rule assumed support, because the cost of
        // guessing wrong was a needless "update FrostMod". It is now a crash.
        assert!(!model_refresh_is_safe(None)); // no version.txt at all
        assert!(!model_refresh_is_safe(Some("")));
        assert!(!model_refresh_is_safe(Some("nightly")));
        assert!(!model_refresh_is_safe(Some("v-broken-")));
    }

    /// Every FrostMod ever released was built for MX Bikes, so the GP floor must not
    /// touch it — including builds too old to have heard of GP Bikes, and unreadable ones.
    #[test]
    fn mx_bikes_accepts_every_build() {
        for tag in [Some("v0.9.9"), Some("v0.10.0"), Some("v0.11.0"), Some("nightly"), None] {
            assert!(
                supported_for_game(crate::game::Game::Mxb, tag),
                "MX Bikes should accept {tag:?}"
            );
        }
    }

    /// The case this floor exists for: v0.10.0 is the first build that attaches to GP
    /// Bikes, and its reload runs MX Bikes' offsets there. "Knows about GP Bikes" and
    /// "is safe on GP Bikes" are different versions, and this is the gap between them.
    #[test]
    fn gp_bikes_rejects_the_build_that_crashes_it() {
        assert!(!supported_for_game(crate::game::Game::Gpb, Some("v0.10.0")));
        assert!(!supported_for_game(crate::game::Game::Gpb, Some("v0.10.0-rc1")));
        assert!(!supported_for_game(crate::game::Game::Gpb, Some("v0.9.11")));
        assert!(supported_for_game(crate::game::Game::Gpb, Some("v0.11.0")));
        assert!(supported_for_game(crate::game::Game::Gpb, Some("v0.11.0-rc1")));
        assert!(supported_for_game(crate::game::Game::Gpb, Some("v1.0.0")));
    }

    /// Same numeric-not-lexical trap the model-refresh floor documents: "v0.11.0" sorts
    /// below "v0.9.11" as a string, and a GP floor is exactly where that would bite.
    #[test]
    fn the_gp_floor_compares_numerically() {
        assert!(supported_for_game(crate::game::Game::Gpb, Some("v0.11.0")));
        assert!(!supported_for_game(crate::game::Game::Gpb, Some("v0.9.99")));
    }

    /// An unreadable tag is a build we can't vouch for, and the cost of guessing wrong
    /// here is the game going down — so GP Bikes withholds, as the model refresh does.
    #[test]
    fn gp_bikes_withholds_from_a_version_it_cannot_read() {
        assert!(!supported_for_game(crate::game::Game::Gpb, None));
        assert!(!supported_for_game(crate::game::Game::Gpb, Some("")));
        assert!(!supported_for_game(crate::game::Game::Gpb, Some("nightly")));
    }

    #[test]
    fn short_tags_fill_the_missing_parts_with_zero() {
        assert!(model_refresh_is_safe(Some("v1")));
        assert!(!model_refresh_is_safe(Some("v0.9")));
    }

    #[test]
    fn refresh_command_json_uses_the_verb_frostmod_dispatches_on() {
        // The verb string is the contract with HandleFrostModCommand in frostmod.cpp.
        assert_eq!(
            command_json_at("refresh_bike_model", "MX1OEM_1996_Honda_CR250", 7),
            r#"{"at":"7","bikeId":"MX1OEM_1996_Honda_CR250","verb":"refresh_bike_model"}"#
        );
    }

    /// The Reload button's verb on Linux, where there is no event to send instead. Same
    /// contract file, same dispatcher — `reload_mods` in frostmod.cpp.
    #[test]
    fn reload_has_a_verb_of_its_own_for_a_platform_with_no_event() {
        assert_eq!(
            command_json_at("reload_mods", "", 7),
            r#"{"at":"7","bikeId":"","verb":"reload_mods"}"#
        );
    }

    /// Module lists come back as full paths, in whatever case the loader recorded — and on
    /// Linux the same DLL is reached through a `/proc` mapping with forward slashes.
    #[test]
    fn the_injected_dll_is_recognised_wherever_it_was_loaded_from() {
        for path in [
            r"C:\Users\me\AppData\Local\com.frost.mxbikes\frostmod\frostmod.dll",
            r"C:\Users\me\AppData\Local\com.frost.mxbikes\frostmod\FrostMod.DLL",
            "/home/me/.steam/steamapps/compatdata/pfx/drive_c/frostmod/frostmod.dll",
            "frostmod.dll",
        ] {
            assert!(is_injected_dll(path), "should be FrostMod's DLL: {path}");
        }
    }

    /// The near-misses that must not read as an attached FrostMod — the launcher is a
    /// sibling of the DLL in the same folder, and it is the thing that is always running.
    #[test]
    fn nothing_else_counts_as_attached() {
        for path in [
            r"C:\frostmod\frostmod.exe",
            r"C:\Program Files\MX Bikes\mxbikes.exe",
            r"C:\frostmod\frostmod.dll.bak",
            r"C:\frostmod\notfrostmod.dll",
            "",
        ] {
            assert!(!is_injected_dll(path), "should not be FrostMod's DLL: {path}");
        }
    }

    /// FrostMod injects a moment *after* the game process appears, so the first look is
    /// always a miss. Warning then would fire on every launch the app ever saw.
    #[test]
    fn a_game_that_just_started_is_given_time_to_be_injected() {
        let start = std::time::Instant::now();
        let mut first_seen = None;
        assert_eq!(waiting_verdict(&mut first_seen, start), AttachState::Attaching);
        assert_eq!(
            waiting_verdict(&mut first_seen, start + ATTACH_GRACE - std::time::Duration::from_secs(1)),
            AttachState::Attaching,
            "still inside the grace period",
        );
    }

    /// Past the grace period it is no longer "any moment now", and saying so is the whole
    /// point — this is the state the player is currently left to work out for themselves.
    #[test]
    fn a_game_that_never_got_injected_is_reported() {
        let start = std::time::Instant::now();
        let mut first_seen = None;
        waiting_verdict(&mut first_seen, start);
        assert_eq!(
            waiting_verdict(&mut first_seen, start + ATTACH_GRACE),
            AttachState::NotAttached,
        );
    }

    /// The clock starts when we first see the game without FrostMod, not when we're asked.
    /// Without this a slow poll could hand out a fresh grace period every time.
    #[test]
    fn the_grace_period_runs_from_the_first_sighting() {
        let start = std::time::Instant::now();
        let mut first_seen = Some(start);
        assert_eq!(
            waiting_verdict(&mut first_seen, start + ATTACH_GRACE),
            AttachState::NotAttached,
        );
        assert_eq!(first_seen, Some(start), "the first sighting is not moved by a later look");
    }
}
