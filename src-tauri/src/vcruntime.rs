//! Visual C++ runtime preflight for the FrostMod chain.
//!
//! Two different runtimes have to be present before FrostMod can attach to the game,
//! and when either is missing Windows says so with a bare "the code execution cannot
//! proceed because <something>.dll was not found" box over the game — a message with no
//! path from the error to the fix.
//!
//! **VC90 (Visual C++ 2008).** `mxbikes.exe` and `gpbikes.exe` are x64 PiBoSo builds that
//! import `MSVCR90.dll`. They don't import it by path: their manifest requests it as a
//! side-by-side assembly (`Microsoft.VC90.CRT`, amd64, publicKeyToken `1fc8b3b9a1e18e3b`).
//! That resolves either machine-wide out of `WinSxS`, *or* out of a private copy sitting
//! in the game folder — and on a machine living on the second one, the game launches fine
//! while nothing loaded from outside the game folder can resolve `MSVCR90`. Which is
//! exactly what FrostMod is: an absolute path under `%LOCALAPPDATA%`, injected with
//! `CreateRemoteThread` + `LoadLibraryA`. Game works, FrostMod doesn't, and the two look
//! unrelated to the player.
//!
//! **VC140 (Visual C++ 2015–2022).** `frostmod.dll` is a modern MSVC build, so it needs
//! `vcruntime140` / `msvcp140` / the UCRT. Same failure, different DLL in the dialog.
//!
//! We detect both, and can install either. Detection is deliberately conservative: when
//! we can't tell, we report nothing missing. Telling a working player their PC is broken
//! is worse than staying quiet.

// Detection and installation are Windows-only by construction; everywhere else this module
// compiles down to `missing() -> []` and the rest of it reads as dead. Left visible, that's
// a handful of warnings on every macOS build, which only teaches us to skim past them.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// A Visual C++ runtime the FrostMod chain depends on.
///
/// `Deserialize` too: the UI names one of these when it asks us to install it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    /// Visual C++ 2008 (`MSVCR90`) — what the *game* imports.
    Vc90,
    /// Visual C++ 2015–2022 (`vcruntime140` / `msvcp140`) — what `frostmod.dll` imports.
    Vc140,
}

/// What an install attempt actually did.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallOutcome {
    /// Installed, and re-detection confirms the runtime is now present.
    Installed,
    /// The user dismissed the UAC prompt. Not an error — offer the manual link.
    Cancelled,
}

/// The WinSxS directory name every VC90 CRT assembly starts with. Real entries look like
/// `amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_08e793bfa83a89b5`.
///
/// The architecture prefix is load-bearing: an `x86_…` assembly is present on plenty of
/// machines and does nothing for a 64-bit game. So is anchoring at the start — WinSxS also
/// carries `amd64_policy.9.0.microsoft.vc90.crt_…` publisher-policy entries, which are
/// redirection rules, not the CRT.
const VC90_SXS_PREFIX: &str = "amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_";

/// The DLLs the 2015–2022 redistributable puts in `System32`.
const VC140_DLLS: [&str; 2] = ["vcruntime140.dll", "msvcp140.dll"];

/// An installer under this is a captive-portal login page or a truncated download, not a
/// redistributable. The real ones are ~5 MB (VC90) and ~25 MB (VC140).
const MIN_INSTALLER_BYTES: usize = 1024 * 1024;

impl Runtime {
    /// Microsoft's official download. Both were HEAD-checked when this landed; both are
    /// permanent URLs Microsoft publishes for redistribution.
    pub fn url(self) -> &'static str {
        match self {
            // Visual C++ 2008 SP1 redistributable, x64.
            Runtime::Vc90 => "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x64.exe",
            // Evergreen "latest supported x64" link — always the current 2015–2022 build.
            Runtime::Vc140 => "https://aka.ms/vs/17/release/vc_redist.x64.exe",
        }
    }

    /// Silent-install switches. These differ by installer generation and are not
    /// interchangeable: the 2008 package is an IExpress/MSI wrapper that takes `/q`, while
    /// the 2015–2022 package is a Burn bundle that wants `/install /quiet /norestart` and
    /// treats a bare `/q` as unknown.
    pub fn installer_args(self) -> &'static str {
        match self {
            Runtime::Vc90 => "/q /norestart",
            Runtime::Vc140 => "/install /quiet /norestart",
        }
    }

    /// Filename to stage the download under.
    fn installer_name(self) -> &'static str {
        match self {
            Runtime::Vc90 => "vcredist_x64_2008.exe",
            Runtime::Vc140 => "vc_redist.x64.exe",
        }
    }
}

/// Does any of these WinSxS entry names name the amd64 VC90 CRT assembly?
///
/// Split out from the directory walk so the matching rule — the part with the actual
/// judgement in it — is testable on any platform.
fn names_contain_vc90<I, S>(names: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    names
        .into_iter()
        .any(|n| n.as_ref().to_ascii_lowercase().starts_with(VC90_SXS_PREFIX))
}

/// Is the payload we downloaded plausibly a Windows installer?
fn looks_like_installer(bytes: &[u8]) -> bool {
    bytes.len() >= MIN_INSTALLER_BYTES && bytes.starts_with(b"MZ")
}

#[cfg(windows)]
fn windir() -> std::path::PathBuf {
    std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
}

/// Walk `WinSxS` looking for the VC90 CRT.
///
/// Returns `true` ("present") when the directory can't be read at all. We'd rather miss a
/// real problem than raise a false alarm on a machine we simply couldn't inspect.
#[cfg(windows)]
fn vc90_present() -> bool {
    let sxs = windir().join("WinSxS");
    match std::fs::read_dir(&sxs) {
        Ok(entries) => names_contain_vc90(
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned()),
        ),
        Err(e) => {
            log::warn!("could not read {} ({e}) — assuming VC90 is present", sxs.display());
            true
        }
    }
}

/// The 2015–2022 runtime installs into `System32` rather than WinSxS. This process is
/// x64, so `System32` is the genuine 64-bit directory — no WOW64 redirection to unpick.
#[cfg(windows)]
fn vc140_present() -> bool {
    let sys32 = windir().join("System32");
    VC140_DLLS.iter().all(|dll| sys32.join(dll).exists())
}

/// Cached detection result. Walking WinSxS means enumerating tens of thousands of entries,
/// and the *absent* case — the one a player in trouble hits — is the one that walks all of
/// them. The answer only changes when we install something, and `invalidate` covers that.
#[cfg(windows)]
static CACHE: std::sync::Mutex<Option<Vec<Runtime>>> = std::sync::Mutex::new(None);

/// Drop the cached probe so the next `missing()` re-reads the disk.
#[cfg(windows)]
fn invalidate() {
    if let Ok(mut c) = CACHE.lock() {
        *c = None;
    }
}

/// Which runtimes the FrostMod chain needs and this machine doesn't have.
#[cfg(windows)]
pub fn missing() -> Vec<Runtime> {
    if let Ok(cache) = CACHE.lock() {
        if let Some(hit) = cache.as_ref() {
            return hit.clone();
        }
    }
    let mut out = Vec::new();
    if !vc90_present() {
        out.push(Runtime::Vc90);
    }
    if !vc140_present() {
        out.push(Runtime::Vc140);
    }
    if !out.is_empty() {
        log::info!("missing Visual C++ runtimes: {out:?}");
    }
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(out.clone());
    }
    out
}

/// Nothing to detect off Windows — FrostMod only runs there.
#[cfg(not(windows))]
pub fn missing() -> Vec<Runtime> {
    Vec::new()
}

// ===========================================================================
// Elevated install
// ===========================================================================

#[cfg(windows)]
mod ffi {
    use std::os::raw::c_void;

    pub type Handle = *mut c_void;

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

    extern "system" {
        pub fn ShellExecuteExW(info: *mut ShellExecuteInfoW) -> i32;
        pub fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
        pub fn CloseHandle(handle: Handle) -> i32;
        pub fn GetLastError() -> u32;
    }

    /// Hand back the process handle so we can wait on it.
    pub const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
    /// We aren't pumping messages on this thread; without it the call can return before
    /// the shell has finished with our string buffers.
    pub const SEE_MASK_NOASYNC: u32 = 0x0000_0100;
    pub const SW_HIDE: i32 = 0;
    pub const WAIT_TIMEOUT: u32 = 0x0000_0102;
    /// The user dismissed the UAC prompt.
    pub const ERROR_CANCELLED: u32 = 1223;
}

/// Give a stuck installer a bound rather than hanging the command forever. A declined UAC
/// prompt doesn't reach this — `ShellExecuteExW` fails immediately with `ERROR_CANCELLED`.
#[cfg(windows)]
const INSTALL_TIMEOUT_MS: u32 = 10 * 60 * 1000;

#[cfg(windows)]
fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Run an installer elevated and wait for it to finish.
///
/// `std::process::Command` cannot do this: it goes through `CreateProcess`, which refuses
/// an executable manifested `requireAdministrator` with `ERROR_ELEVATION_REQUIRED` (740)
/// instead of prompting. Only the shell raises the UAC dialog, so it has to be
/// `ShellExecuteExW` with the `runas` verb.
///
/// Returns `Ok(false)` when the user declined the prompt.
#[cfg(windows)]
fn run_elevated(exe: &std::path::Path, args: &str) -> anyhow::Result<bool> {
    let verb = wide(std::ffi::OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    let params = wide(std::ffi::OsStr::new(args));

    let mut info = ffi::ShellExecuteInfoW {
        cb_size: std::mem::size_of::<ffi::ShellExecuteInfoW>() as u32,
        f_mask: ffi::SEE_MASK_NOCLOSEPROCESS | ffi::SEE_MASK_NOASYNC,
        hwnd: std::ptr::null_mut(),
        lp_verb: verb.as_ptr(),
        lp_file: file.as_ptr(),
        lp_parameters: params.as_ptr(),
        lp_directory: std::ptr::null(),
        n_show: ffi::SW_HIDE,
        h_inst_app: std::ptr::null_mut(),
        lp_id_list: std::ptr::null_mut(),
        lp_class: std::ptr::null(),
        hkey_class: std::ptr::null_mut(),
        dw_hot_key: 0,
        h_icon_or_monitor: std::ptr::null_mut(),
        h_process: std::ptr::null_mut(),
    };

    // SAFETY: `info` is a correctly sized, fully initialised SHELLEXECUTEINFOW, and every
    // string pointer refers to a NUL-terminated buffer that outlives the call.
    let ok = unsafe { ffi::ShellExecuteExW(&mut info) } != 0;
    if !ok {
        let err = unsafe { ffi::GetLastError() };
        if err == ffi::ERROR_CANCELLED {
            return Ok(false);
        }
        anyhow::bail!("Windows wouldn't start the installer (error {err})");
    }

    if info.h_process.is_null() {
        // Shouldn't happen with SEE_MASK_NOCLOSEPROCESS, but without a handle we can't
        // claim it finished — and re-detection below is what actually decides.
        return Ok(true);
    }
    // SAFETY: `h_process` is a live process handle the shell handed us; closed below.
    let waited = unsafe { ffi::WaitForSingleObject(info.h_process, INSTALL_TIMEOUT_MS) };
    unsafe { ffi::CloseHandle(info.h_process) };
    if waited == ffi::WAIT_TIMEOUT {
        anyhow::bail!("the installer is still running after 10 minutes — finish it in the window Windows opened, then check again");
    }
    // The exit code is deliberately ignored: these packages return 3010 for "installed,
    // reboot pending" and 1638 for "a newer one is already here", both of which are fine.
    // Whether the runtime is actually there now is settled by re-detection.
    Ok(true)
}

/// Download Microsoft's redistributable and install it, then prove it worked.
#[cfg(windows)]
pub async fn install(app: &tauri::AppHandle, runtime: Runtime) -> anyhow::Result<InstallOutcome> {
    use tauri::Manager;

    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow::anyhow!("could not resolve the app data dir: {e}"))?
        .join("runtime");
    std::fs::create_dir_all(&dir)?;
    let staged = dir.join(runtime.installer_name());

    let client = reqwest::Client::builder()
        .user_agent(crate::frostmod_manage::UA)
        .build()?;
    let bytes = client
        .get(runtime.url())
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    // A download that didn't land is worth catching before we ask for admin rights.
    if !looks_like_installer(&bytes) {
        anyhow::bail!(
            "The download from Microsoft didn't arrive intact ({} bytes) — nothing was installed. Check your connection and try again.",
            bytes.len()
        );
    }
    std::fs::write(&staged, &bytes)?;

    let args = runtime.installer_args();
    let path = staged.clone();
    // ShellExecuteExW + the wait are blocking, and this is running on the async runtime.
    let accepted = tauri::async_runtime::spawn_blocking(move || run_elevated(&path, args))
        .await
        .map_err(|e| anyhow::anyhow!("the installer task failed: {e}"))??;

    let _ = std::fs::remove_file(&staged);

    if !accepted {
        return Ok(InstallOutcome::Cancelled);
    }

    // Trust the disk, not the installer's exit code.
    invalidate();
    if missing().contains(&runtime) {
        anyhow::bail!(
            "The installer ran but Windows still can't find the component. A restart usually finishes it off."
        );
    }
    log::info!("installed Visual C++ runtime {runtime:?}");
    Ok(InstallOutcome::Installed)
}

#[cfg(not(windows))]
pub async fn install(
    _app: &tauri::AppHandle,
    _runtime: Runtime,
) -> anyhow::Result<InstallOutcome> {
    anyhow::bail!("Visual C++ runtimes only apply on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real WinSxS entry names, as they appear on a machine that has the assembly.
    #[test]
    fn matches_a_real_vc90_assembly_folder() {
        assert!(names_contain_vc90([
            "amd64_microsoft.windows.common-controls_6595b64144ccf1df_6.0.22621.1_none_9f6e2b2b",
            "amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_08e793bfa83a89b5",
        ]));
    }

    /// NTFS is case-insensitive and the casing of these entries isn't ours to rely on.
    #[test]
    fn matching_ignores_case() {
        assert!(names_contain_vc90([
            "AMD64_Microsoft.VC90.CRT_1FC8B3B9A1E18E3B_9.0.30729.6161_none_08e7a8e6a52ec5b5",
        ]));
    }

    /// The x86 CRT is on plenty of machines and does nothing for a 64-bit game, so it must
    /// not read as "present".
    #[test]
    fn x86_assembly_does_not_count() {
        assert!(!names_contain_vc90([
            "x86_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_508ef7e4bcbbe589",
        ]));
    }

    /// Publisher-policy entries are redirection rules, not the CRT itself. They sit right
    /// next to the real thing and share most of the name.
    #[test]
    fn publisher_policy_does_not_count() {
        assert!(!names_contain_vc90([
            "amd64_policy.9.0.microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_a0b4c0f2",
        ]));
    }

    #[test]
    fn an_empty_or_unrelated_winsxs_reads_as_absent() {
        assert!(!names_contain_vc90(Vec::<String>::new()));
        assert!(!names_contain_vc90([
            "amd64_microsoft.vc80.crt_1fc8b3b9a1e18e3b_8.0.50727.9585_none_88df89932faf0bf6",
            "msil_system.web_b03f5f7f11d50a3a_4.0.15805.0_none_b3a4e5c9",
        ]));
    }

    /// The two packages take different switches; swapping them silently does nothing.
    #[test]
    fn each_runtime_carries_its_own_silent_switches() {
        assert_eq!(Runtime::Vc90.installer_args(), "/q /norestart");
        assert_eq!(Runtime::Vc140.installer_args(), "/install /quiet /norestart");
        assert_ne!(Runtime::Vc90.url(), Runtime::Vc140.url());
    }

    /// A captive-portal HTML page returns 200 and is not an installer.
    #[test]
    fn rejects_a_payload_that_isnt_an_installer() {
        let html = b"<!doctype html><html>sign in to continue</html>".repeat(1000);
        assert!(!looks_like_installer(&html));
        // Right magic, far too small — a truncated download.
        let mut stub = b"MZ".to_vec();
        stub.resize(4096, 0);
        assert!(!looks_like_installer(&stub));
    }

    #[test]
    fn accepts_a_plausible_installer() {
        let mut exe = b"MZ".to_vec();
        exe.resize(MIN_INSTALLER_BYTES + 1, 0);
        assert!(looks_like_installer(&exe));
    }

    /// The UI keys off these strings; renaming a variant would silently break the banner.
    #[test]
    fn runtimes_serialize_as_the_ui_expects() {
        assert_eq!(serde_json::to_string(&Runtime::Vc90).unwrap(), "\"vc90\"");
        assert_eq!(serde_json::to_string(&Runtime::Vc140).unwrap(), "\"vc140\"");
        assert_eq!(
            serde_json::to_string(&InstallOutcome::Cancelled).unwrap(),
            "\"cancelled\""
        );
    }
}
