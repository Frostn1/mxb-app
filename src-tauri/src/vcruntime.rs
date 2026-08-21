//! Visual C++ runtime preflight for the FrostMod chain.
//!
//! Two different runtimes have to be present before FrostMod can attach to the game,
//! and when either is missing Windows says so with a bare "the code execution cannot
//! proceed because <something>.dll was not found" box over the game — a message with no
//! path from the error to the fix.
//!
//! **VC90 (Visual C++ 2008).** `mxbikes.exe` and `gpbikes.exe` are x64 PiBoSo builds that
//! import `MSVCR90.dll`. They don't import it by path: their manifest requests it as a
//! side-by-side assembly (`Microsoft.VC90.CRT`, amd64, publicKeyToken `1fc8b3b9a1e18e3b`,
//! versions `9.0.21022.8` and `9.0.30729.1`). That resolves machine-wide out of `WinSxS`,
//! or out of a private assembly folder sitting beside the exe.
//!
//! The redistributable does not put `msvcr90.dll` anywhere on the ordinary DLL search
//! path. It registers the side-by-side assembly under `WinSxS`, and reaching that requires
//! the loading module to *ask for it by manifest*. The game does. A module that doesn't —
//! anything with a plain `MSVCR90.dll` import and no VC90 manifest, which is most
//! community plugins built with VS2008 — gets the ordinary search order instead: the exe's
//! own folder, `System32`, `PATH`. The redistributable touches none of those.
//!
//! So the two failures read differently, and the wording in the dialog tells them apart:
//!
//! * *"the side-by-side configuration is incorrect"* (error 14001) — the **game's**
//!   manifested dependency is unsatisfiable. Installing the redistributable fixes it.
//! * *"MSVCR90.dll was not found"* / *"is missing from your computer"* — a **plain
//!   import** went unresolved.
//!
//! **The second one has no app-local cure, and an earlier version of this module shipped
//! one anyway.** It copied `msvcr90.dll` out of `WinSxS` and laid it beside the exe, on the
//! reasoning that a loose DLL is invisible to side-by-side binding and so can only ever
//! serve the plain imports the redistributable strands. The first half is true. The second
//! does not follow, because the VC9 CRT polices this itself: `msvcr90.dll` checks at load
//! time that it was resolved through a `Microsoft.VC90.CRT` activation context, and a loose
//! copy in the exe's own directory is by definition outside one. It refuses to initialise
//! and takes the process down with
//!
//! ```text
//! R6034 — An application has made an attempt to load the C runtime library incorrectly.
//! ```
//!
//! Which makes the copy never once helpful and frequently fatal. A module carrying a VC90
//! manifest resolves from `WinSxS` and never looks at it; a module without one finds it and
//! dies. We turned a plugin that quietly failed to load into a modal that killed the game —
//! see [`remove_stray_msvcr90`], which now takes back the copies we made.
//!
//! What does work app-locally is a *private assembly*: a `Microsoft.VC90.CRT` folder beside
//! the exe holding the DLL and its manifest, which is a real side-by-side identity and
//! satisfies the check. PiBoSo installers have historically laid one down, so [`vc90_present`]
//! counts it. We don't build one — it participates in binding, and a version-mismatched one
//! could hijack a binding that currently works. The redistributable is the cure we offer.
//!
//! **VC140 (Visual C++ 2015–2022).** `frostmod.dll` and `frostmod.exe` are modern MSVC
//! builds. Their import tables name `VCRUNTIME140.dll`, `VCRUNTIME140_1.dll`,
//! `MSVCP140.dll` and the UCRT — and, notably, never `MSVCR90.dll`. Injection failing is
//! therefore always a VC140 story, never a VC90 one. `VCRUNTIME140_1.dll` is the easy one
//! to miss: it only ships from the 2019 redistributable onward, so a machine still on the
//! original 2015 package has the other two and not it.
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
    /// Visual C++ 2015–2022, x64 (`vcruntime140` / `msvcp140`) — what `frostmod.dll` imports.
    Vc140,
    /// Visual C++ 2015–2022, x86.
    ///
    /// Nothing in our own chain needs this one: the app, FrostMod and the modern game
    /// builds are all x64. It is here because plenty of what players run alongside the
    /// game isn't — 32-bit plugins, tools, and the dedicated-server build, which really is
    /// a 32-bit binary — and because the pair is what Microsoft's own "Latest Supported
    /// Visual C++ Downloads" page hands out. Offered by the repair, never a banner: see
    /// [`missing`] for why an unprompted alarm about it would be wrong.
    Vc140X86,
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

/// The DLLs the 2015–2022 redistributable puts in `System32`, as named by the import
/// tables of `frostmod.dll` and `frostmod.exe`.
///
/// `vcruntime140_1.dll` is load-bearing and was missing here until a player hit exactly
/// the gap it leaves: it carries the x64 exception-unwinding half of the runtime and only
/// began shipping with the 2019 redistributable. A machine on the original 2015 package
/// has the other two, so checking only those reports a clean bill of health while
/// injection dies naming a DLL we never looked for.
const VC140_DLLS: [&str; 3] = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"];

/// The x86 probe, deliberately shorter than [`VC140_DLLS`].
///
/// `vcruntime140_1.dll` is left out because we could not confirm the 32-bit package ships
/// it — the redistributable is a Burn bundle with its payload compressed, so the honest
/// answer from here is "unknown". Probing for a file that may never exist on x86 would
/// mark every machine permanently short of a runtime it already has, and a false alarm we
/// can't clear is worse than a gap we don't report. The two below are certain.
const VC140_X86_DLLS: [&str; 2] = ["vcruntime140.dll", "msvcp140.dll"];

/// The file every one of these paths is ultimately about.
const MSVCR90_DLL: &str = "msvcr90.dll";

/// An installer under this is a captive-portal login page or a truncated download, not a
/// redistributable. The real ones are ~5 MB (VC90), ~25.6 MB (VC140 x64) and ~14 MB
/// (VC140 x86), so a megabyte clears all three with room to spare.
const MIN_INSTALLER_BYTES: usize = 1024 * 1024;

impl Runtime {
    /// Microsoft's official download. Both were HEAD-checked when this landed; both are
    /// permanent URLs Microsoft publishes for redistribution.
    pub fn url(self) -> &'static str {
        match self {
            // Visual C++ 2008 SP1 redistributable, x64.
            Runtime::Vc90 => "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x64.exe",
            // Evergreen "latest supported" links off Microsoft's own downloads page —
            // always the current 2015–2022 build, so they don't rot the way a versioned
            // link does. HEAD-checked when this landed: 25.6 MB x64, 14.0 MB x86.
            Runtime::Vc140 => "https://aka.ms/vs/17/release/vc_redist.x64.exe",
            Runtime::Vc140X86 => "https://aka.ms/vs/17/release/vc_redist.x86.exe",
        }
    }

    /// What to call this in a sentence to the player. The architecture is part of the
    /// name: someone told to install "Visual C++" who already has the x64 package needs to
    /// know it's the other one being asked for.
    pub fn label(self) -> &'static str {
        match self {
            Runtime::Vc90 => "Microsoft Visual C++ 2008 (x64)",
            Runtime::Vc140 => "Microsoft Visual C++ 2015–2022 (x64)",
            Runtime::Vc140X86 => "Microsoft Visual C++ 2015–2022 (x86)",
        }
    }

    /// Silent-install switches. These differ by installer generation and are not
    /// interchangeable: the 2008 package is an IExpress/MSI wrapper that takes `/q`, while
    /// the 2015–2022 package is a Burn bundle that wants `/install /quiet /norestart` and
    /// treats a bare `/q` as unknown.
    pub fn installer_args(self) -> &'static str {
        match self {
            Runtime::Vc90 => "/q /norestart",
            Runtime::Vc140 | Runtime::Vc140X86 => "/install /quiet /norestart",
        }
    }

    /// Filename to stage the download under. Distinct per runtime so two staged installers
    /// can never overwrite one another mid-repair.
    fn installer_name(self) -> &'static str {
        match self {
            Runtime::Vc90 => "vcredist_x64_2008.exe",
            Runtime::Vc140 => "vc_redist.x64.exe",
            Runtime::Vc140X86 => "vc_redist.x86.exe",
        }
    }

    /// Every runtime the repair knows how to fetch, in the order it works through them:
    /// the game's own CRT first, since that is the one whose absence stops the game dead.
    pub const ALL: [Runtime; 3] = [Runtime::Vc90, Runtime::Vc140, Runtime::Vc140X86];
}

/// The four-part version embedded in a WinSxS entry name, as a sortable key.
///
/// `amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_08e793bfa83a89b5` carries
/// it between the public key token and the `_none_` culture segment. An entry we can't
/// parse sorts lowest rather than being discarded: it is still a real assembly, and having
/// one we can't name the version of beats concluding we have none.
fn entry_version(name: &str) -> [u32; 4] {
    let lower = name.to_ascii_lowercase();
    let mut out = [0u32; 4];
    let Some(rest) = lower.strip_prefix(VC90_SXS_PREFIX) else {
        return out;
    };
    for (slot, part) in out.iter_mut().zip(rest.split('_').next().unwrap_or_default().split('.')) {
        *slot = part.parse().unwrap_or(0);
    }
    out
}

/// Does this WinSxS entry name the amd64 VC90 CRT assembly?
///
/// The one place the matching rule lives, so the directory walk and the version pick can't
/// drift apart on it.
fn is_vc90_entry(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with(VC90_SXS_PREFIX)
}

/// Pick the highest-versioned amd64 VC90 CRT assembly out of a set of WinSxS entry names.
///
/// Split out from the directory walk so the matching rule — the part with the actual
/// judgement in it — is testable on any platform.
fn newest_vc90_entry<I, S>(names: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    names
        .into_iter()
        .filter(|n| is_vc90_entry(n.as_ref()))
        .max_by_key(|n| entry_version(n.as_ref()))
        .map(|n| n.as_ref().to_owned())
}

/// Does any of these WinSxS entry names name the amd64 VC90 CRT assembly?
fn names_contain_vc90<I, S>(names: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    newest_vc90_entry(names).is_some()
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

/// Every amd64 VC90 CRT assembly folder in `WinSxS`, newest first.
///
/// All of them, not just the newest, because this is what [`remove_stray_msvcr90`] compares
/// against: a stray copy was taken from whichever build was newest *on the day it was made*,
/// and a servicing update since then would leave the newest folder holding different bytes.
/// Matching against every assembly on the machine is what keeps the cleanup working after
/// Windows Update has moved on.
///
/// Empty covers both "no such assembly" and "couldn't read the directory".
#[cfg(windows)]
fn sxs_vc90_dirs() -> Vec<std::path::PathBuf> {
    let sxs = windir().join("WinSxS");
    let Ok(entries) = std::fs::read_dir(&sxs) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_vc90_entry(n))
        .collect();
    names.sort_by_key(|n| std::cmp::Reverse(entry_version(n)));
    names.into_iter().map(|n| sxs.join(n)).collect()
}

/// Where a loose `msvcr90.dll` sits when something has put one beside the game exe.
///
/// The exe's own directory is the first entry in the ordinary DLL search order — which is
/// exactly why a copy here is dangerous rather than useful. See [`remove_stray_msvcr90`].
fn app_local_msvcr90(game_dir: &std::path::Path) -> std::path::PathBuf {
    game_dir.join(MSVCR90_DLL)
}

/// Does the game folder itself carry a working route to the CRT?
///
/// The private assembly folder PiBoSo installers have historically laid down beside the exe.
/// A real side-by-side identity, so the CRT's own activation-context check is satisfied by
/// it — which is exactly what a loose `msvcr90.dll` next to it is not, and why only this
/// arrangement counts. Split out from [`vc90_present`] so that distinction is testable off
/// Windows, where the `WinSxS` half of the question can't be asked.
fn game_dir_has_vc90(game_dir: &std::path::Path) -> bool {
    game_dir.join("Microsoft.VC90.CRT").join(MSVCR90_DLL).is_file()
}

/// Can anything in the game resolve `MSVCR90` at all?
///
/// This is the question the banner asks, so it stays broad on purpose: the machine-wide
/// assembly or a private assembly folder beside the exe each count. Either means there is a
/// working route to the CRT and nothing worth alarming about.
///
/// A bare `msvcr90.dll` in the game folder is deliberately **not** counted. It is not a
/// route to the CRT — it is the R6034 trap described at the top of this module — and
/// counting it would let a file we ourselves once planted silence the detector that should
/// be reporting the machine has no VC90.
///
/// It deliberately does *not* try to answer the narrower "will a plain import resolve"
/// question. That one is false on the overwhelming majority of perfectly healthy machines,
/// so asking it here would put an amber bar in front of everyone — and, as the module doc
/// sets out, there is nothing we could safely do about the answer anyway.
///
/// Returns `true` ("present") when `WinSxS` can't be read at all. We'd rather miss a real
/// problem than raise a false alarm on a machine we simply couldn't inspect.
#[cfg(windows)]
fn vc90_present(game_dir: Option<&std::path::Path>) -> bool {
    if game_dir.is_some_and(game_dir_has_vc90) {
        return true;
    }
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

/// Take back a loose `msvcr90.dll` we laid beside the game exe.
///
/// Versions 0.9.2 through 0.10.0 copied the CRT out of `WinSxS` into the game folder on
/// every status poll, believing it served the plain imports the redistributable strands. It
/// does not — the VC9 CRT aborts with R6034 when it is loaded outside a `Microsoft.VC90.CRT`
/// activation context, which a loose copy always is. Deleting the code that made the copy
/// does nothing for the players already carrying one, so the app removes it.
///
/// **Only ever deletes a file whose bytes match a VC90 assembly on this machine.** That is
/// where our copy came from, so the compare is what stops us reaching for a `msvcr90.dll`
/// somebody else put there for a reason we don't know. A byte-identical file is equally
/// R6034-fatal whoever laid it, so provenance is the right test and a marker file — which
/// the version that did the damage never wrote — would not have helped.
///
/// A `Microsoft.VC90.CRT` folder beside the exe is left strictly alone: that one works.
///
/// Best-effort, and safe on a hot path. The settled case costs one failed `read`. A locked
/// file — the game running with the DLL mapped — just means the next poll tries again, and
/// only the first failure per folder is logged so a stuck one doesn't fill the log.
#[cfg(windows)]
pub fn remove_stray_msvcr90(game_dir: &std::path::Path) -> bool {
    remove_stray_msvcr90_against(game_dir, &sxs_vc90_dirs())
}

/// The decision and the deletion, with the `WinSxS` lookup handed in.
///
/// Split out the way [`newest_vc90_entry`] is: the judgement — *is this file ours to
/// delete* — is the part worth testing, and it can't be reached on a machine that has no
/// `WinSxS` to enumerate.
fn remove_stray_msvcr90_against(
    game_dir: &std::path::Path,
    sxs_dirs: &[std::path::PathBuf],
) -> bool {
    let stray = app_local_msvcr90(game_dir);
    let Ok(found) = std::fs::read(&stray) else {
        return false;
    };
    let ours = sxs_dirs
        .iter()
        .any(|d| std::fs::read(d.join(MSVCR90_DLL)).is_ok_and(|sxs| sxs == found));
    if !ours {
        if first_time(&LEFT_ALONE, game_dir) {
            log::info!(
                "leaving {} alone — it matches no VC90 assembly on this PC, so it isn't ours",
                stray.display()
            );
        }
        return false;
    }
    match std::fs::remove_file(&stray) {
        Ok(()) => {
            log::info!(
                "removed {} — a loose VC9 CRT there aborts the game with R6034",
                stray.display()
            );
            true
        }
        Err(e) => {
            if first_time(&MOANED, game_dir) {
                log::warn!(
                    "could not remove {} ({e}) — will retry (the game holding it open is the usual reason)",
                    stray.display()
                );
            }
            false
        }
    }
}

/// Have we not yet said this about this game folder?
///
/// The work is repeated on every call by design — the game closing, or the player swapping
/// the file, changes the answer — but saying so again doesn't help anyone reading the log.
/// A poisoned lock reports "first time" so a diagnostic is never swallowed by a bookkeeping
/// failure.
type Folders = std::sync::Mutex<std::collections::BTreeSet<std::path::PathBuf>>;
fn first_time(seen: &Folders, game_dir: &std::path::Path) -> bool {
    seen.lock().map_or(true, |mut s| s.insert(game_dir.to_path_buf()))
}

/// Folders holding a `msvcr90.dll` we decided isn't ours to delete.
static LEFT_ALONE: Folders = std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Folders where the delete failed — a locked file complains once, not on every call.
static MOANED: Folders = std::sync::Mutex::new(std::collections::BTreeSet::new());

#[cfg(not(windows))]
pub fn remove_stray_msvcr90(_game_dir: &std::path::Path) -> bool {
    false
}

/// The 2015–2022 runtime installs into `System32` rather than WinSxS. This process is
/// x64, so `System32` is the genuine 64-bit directory — no WOW64 redirection to unpick.
#[cfg(windows)]
fn vc140_present() -> bool {
    let sys32 = windir().join("System32");
    VC140_DLLS.iter().all(|dll| sys32.join(dll).exists())
}

/// The 32-bit runtime lands in `SysWOW64` — the 64-bit-Windows home for 32-bit system
/// DLLs, despite a name that reads the other way round.
///
/// Naming the path literally is what makes this correct from here: WOW64 file-system
/// redirection rewrites `System32` for 32-bit processes, and this process is 64-bit, so
/// nothing is rewritten in either direction and `SysWOW64` means exactly what it says.
#[cfg(windows)]
fn vc140_x86_present() -> bool {
    let syswow = windir().join("SysWOW64");
    VC140_X86_DLLS.iter().all(|dll| syswow.join(dll).exists())
}

/// Cached detection result, together with the game folder it was computed for.
///
/// Walking WinSxS means enumerating tens of thousands of entries, and the *absent* case —
/// the one a player in trouble hits — is the one that walks all of them. The answer only
/// changes when we install something (`invalidate` covers that) or when the player points
/// the app at a different game folder, which is why the folder is part of the key rather
/// than something a stale entry can outlive.
#[cfg(windows)]
static CACHE: std::sync::Mutex<Option<(Option<std::path::PathBuf>, Vec<Runtime>)>> =
    std::sync::Mutex::new(None);

/// Drop the cached probe so the next `missing()` re-reads the disk. Installing a runtime is
/// the one thing that changes the answer under us.
#[cfg(windows)]
fn invalidate() {
    if let Ok(mut c) = CACHE.lock() {
        *c = None;
    }
}

/// Which runtimes the FrostMod chain needs and this machine doesn't have.
///
/// `game_dir` is where the active title is installed, when the app knows it. Without it
/// the VC90 probe can only consult `WinSxS` and will call a game folder carrying its own
/// copy of the CRT "missing" — so pass it wherever it is to hand.
///
/// [`Runtime::Vc140X86`] is deliberately absent from this list. It drives a banner, and
/// nothing we ship is 32-bit, so its absence proves nothing is broken — raising an amber
/// bar over it would be telling a working player their PC is wrong. The repair installs it
/// on request; see [`short_of`], which is the same question asked without that restraint.
#[cfg(windows)]
pub fn missing(game_dir: Option<&std::path::Path>) -> Vec<Runtime> {
    if let Ok(cache) = CACHE.lock() {
        if let Some((for_dir, hit)) = cache.as_ref() {
            if for_dir.as_deref() == game_dir {
                return hit.clone();
            }
        }
    }
    let mut out = Vec::new();
    if !vc90_present(game_dir) {
        out.push(Runtime::Vc90);
    }
    if !vc140_present() {
        out.push(Runtime::Vc140);
    }
    if !out.is_empty() {
        log::info!("missing Visual C++ runtimes: {out:?}");
    }
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((game_dir.map(|d| d.to_path_buf()), out.clone()));
    }
    out
}

/// Nothing to detect off Windows. FrostMod runs under Proton and in a Wine bottle too, but
/// what it needs there is the prefix's own runtime, installed by the wrapper (`winetricks`,
/// CrossOver's own installers) and not by anything we could reach from out here.
#[cfg(not(windows))]
pub fn missing(_game_dir: Option<&std::path::Path>) -> Vec<Runtime> {
    Vec::new()
}

/// Every runtime this machine is short of, including the ones we'd never raise a banner
/// about. This is what the repair works from: the player asked, so the restraint that
/// governs [`missing`] doesn't apply.
///
/// Uncached on purpose — it runs once per button press, and it must see the disk as it is
/// now rather than as some earlier poll found it.
#[cfg(windows)]
pub fn short_of(game_dir: Option<&std::path::Path>) -> Vec<Runtime> {
    Runtime::ALL
        .into_iter()
        .filter(|r| match r {
            Runtime::Vc90 => !vc90_present(game_dir),
            Runtime::Vc140 => !vc140_present(),
            Runtime::Vc140X86 => !vc140_x86_present(),
        })
        .collect()
}

#[cfg(not(windows))]
pub fn short_of(_game_dir: Option<&std::path::Path>) -> Vec<Runtime> {
    Vec::new()
}

/// What a repair run did, in the terms the player needs to hear it.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    /// Runtimes that went on during this run.
    pub installed: Vec<Runtime>,
    /// Runtimes that were already there. Worth saying: "nothing to do" is a real answer
    /// and reads as a broken button when it's silent.
    pub already_present: Vec<Runtime>,
    /// Still absent afterwards — a declined UAC prompt, a failed download, an install that
    /// needs a reboot to finish. Each one is a link the UI hands over.
    pub still_missing: Vec<Runtime>,
    /// Whether a stray `msvcr90.dll` beside the game exe was cleaned up — see
    /// [`remove_stray_msvcr90`]. Left by 0.9.2–0.10.0, and fatal to the game.
    pub msvcr90_removed: bool,
    /// Set when we had nowhere to place it — no game folder configured.
    pub game_dir_known: bool,
}

/// Install whatever this machine is short of, then finish the VC90 job by hand.
///
/// The one entry point that doesn't depend on detection having raised a flag. That matters
/// more than it sounds: the machine this was written for reported every runtime present
/// and still couldn't start the game, so anything reachable only through the banner would
/// never have run there.
///
/// Never bails part-way. One runtime failing says nothing about the next, and a repair that
/// stops at the first problem leaves the player worse off than one that does what it can
/// and reports the rest — which the UI turns into download links.
#[cfg(windows)]
pub async fn repair(
    app: &tauri::AppHandle,
    game_dir: Option<&std::path::Path>,
) -> RepairReport {
    let mut report = RepairReport {
        game_dir_known: game_dir.is_some(),
        ..Default::default()
    };
    let short = short_of(game_dir);
    for runtime in Runtime::ALL {
        if !short.contains(&runtime) {
            report.already_present.push(runtime);
            continue;
        }
        match install(app, runtime, game_dir).await {
            Ok(InstallOutcome::Installed) => report.installed.push(runtime),
            Ok(InstallOutcome::Cancelled) => {
                log::info!("repair: {runtime:?} declined at the UAC prompt");
                report.still_missing.push(runtime);
            }
            Err(e) => {
                log::warn!("repair: {runtime:?} failed: {e:#}");
                report.still_missing.push(runtime);
            }
        }
    }

    // Sweep up the loose CRT older builds of this app left beside the exe. Worth doing here
    // as well as on the status poll: a player pressing repair is a player whose game is
    // already misbehaving, and this is the likeliest reason.
    if let Some(dir) = game_dir {
        report.msvcr90_removed = remove_stray_msvcr90(dir);
    }

    invalidate();
    log::info!(
        "repair: installed {:?}, still missing {:?}, stray msvcr90 removed: {}",
        report.installed,
        report.still_missing,
        report.msvcr90_removed
    );
    report
}

#[cfg(not(windows))]
pub async fn repair(
    _app: &tauri::AppHandle,
    _game_dir: Option<&std::path::Path>,
) -> RepairReport {
    RepairReport::default()
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
///
/// For VC90 the redistributable is only half the job — it registers the side-by-side
/// assembly and leaves the ordinary search path untouched — so this finishes by placing a
/// copy beside the game exe. See the module docs for why that second half is the one that
/// stops the "MSVCR90.dll was not found" box.
#[cfg(windows)]
pub async fn install(
    app: &tauri::AppHandle,
    runtime: Runtime,
    game_dir: Option<&std::path::Path>,
) -> anyhow::Result<InstallOutcome> {
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

    if missing(game_dir).contains(&runtime) {
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
    _game_dir: Option<&std::path::Path>,
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

    /// The copy we lay beside the exe should be the newest servicing build on the machine,
    /// not whichever entry `read_dir` yielded first. `9635` beats `6161` beats `.1`, and
    /// the ordering has to be numeric — lexically, "9.0.30729.6161" sorts above
    /// "9.0.30729.9635" is false but "10.0" vs "9.0" is exactly where string order breaks.
    #[test]
    fn picks_the_newest_assembly() {
        let newest = newest_vc90_entry([
            "amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.6161_none_08e7a8e6a52ec5b5",
            "amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_08e793bfa83a89b5",
            "amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.21022.8_none_750b37ff97f4f68b",
        ]);
        assert!(
            newest.is_some_and(|n| n.contains("9.0.30729.9635")),
            "the highest servicing build wins"
        );
    }

    /// Version parsing is ordering, not validation: an entry whose version we can't read is
    /// still a real assembly and must stay eligible rather than vanish.
    #[test]
    fn an_unparseable_version_still_counts_as_present() {
        assert!(names_contain_vc90([
            "amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_deadbeef_none_08e793bfa83a89b5",
        ]));
    }

    /// The exception-unwinding half only ships from the 2019 redistributable onward, and
    /// leaving it out is what let a machine on the original 2015 package read as healthy
    /// while injection died naming it.
    #[test]
    fn vc140_probe_covers_the_unwinder() {
        assert!(
            VC140_DLLS.contains(&"vcruntime140_1.dll"),
            "frostmod.dll imports vcruntime140_1.dll, so it has to be probed: {VC140_DLLS:?}"
        );
    }

    /// Both 2015–2022 builds are offered, off Microsoft's "Latest Supported Visual C++
    /// Downloads" page. Installing x64 does nothing for a 32-bit consumer and vice versa,
    /// so a single link would leave half the cases unfixable.
    #[test]
    fn offers_both_architectures_of_the_modern_runtime() {
        assert!(Runtime::ALL.contains(&Runtime::Vc140));
        assert!(Runtime::ALL.contains(&Runtime::Vc140X86));
        assert!(Runtime::Vc140.url().ends_with("vc_redist.x64.exe"));
        assert!(Runtime::Vc140X86.url().ends_with("vc_redist.x86.exe"));
    }

    /// Every runtime needs its own staging filename and its own URL — two sharing either
    /// would have one silently overwrite or stand in for the other mid-repair.
    #[test]
    fn every_runtime_is_distinct_on_disk_and_on_the_wire() {
        for (i, a) in Runtime::ALL.iter().enumerate() {
            for b in &Runtime::ALL[i + 1..] {
                assert_ne!(a.installer_name(), b.installer_name(), "{a:?} vs {b:?}");
                assert_ne!(a.url(), b.url(), "{a:?} vs {b:?}");
                assert_ne!(a.label(), b.label(), "{a:?} vs {b:?}");
            }
        }
    }

    /// The architecture belongs in the name. Someone who already has the x64 package and is
    /// told to install "Visual C++" will reasonably conclude they already did.
    #[test]
    fn labels_name_the_architecture() {
        assert!(Runtime::Vc140.label().contains("x64"));
        assert!(Runtime::Vc140X86.label().contains("x86"));
    }

    /// The x86 probe stays off `vcruntime140_1.dll` on purpose — we could not confirm the
    /// 32-bit package ships it, and probing for a file that may never exist would mark
    /// every machine permanently short of a runtime it already has.
    #[test]
    fn the_x86_probe_stays_conservative() {
        assert!(!VC140_X86_DLLS.contains(&"vcruntime140_1.dll"));
        for dll in VC140_X86_DLLS {
            assert!(VC140_DLLS.contains(&dll), "{dll} should be in both probes");
        }
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

    /// A scratch tree, named per test so a parallel run doesn't collide. The house pattern
    /// — see `paintsync`'s tests — rather than a dev-dependency for four directories.
    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir()
            .join(format!("mxb-vcruntime-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A stand-in `WinSxS` assembly folder holding a `msvcr90.dll` of known bytes.
    fn fake_sxs(root: &std::path::Path, bytes: &[u8]) -> std::path::PathBuf {
        let dir = root.join("amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.9635_none_x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MSVCR90_DLL), bytes).unwrap();
        dir
    }

    /// The whole point of the change: the copy versions 0.9.2–0.10.0 laid beside the exe is
    /// taken back. It came out of `WinSxS`, so it matches byte for byte.
    #[test]
    fn a_copy_taken_from_winsxs_is_removed() {
        let root = scratch("ours");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let sxs = fake_sxs(&root, b"the real VC9 CRT");
        std::fs::write(game.join(MSVCR90_DLL), b"the real VC9 CRT").unwrap();

        assert!(remove_stray_msvcr90_against(&game, &[sxs]));
        assert!(!game.join(MSVCR90_DLL).exists(), "the stray copy must be gone");
    }

    /// A `msvcr90.dll` we didn't put there is somebody else's decision. It is still an R6034
    /// waiting to happen, but deleting a file of unknown provenance out of someone's game
    /// folder is not ours to do.
    #[test]
    fn a_file_we_didnt_place_is_left_alone() {
        let root = scratch("theirs");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let sxs = fake_sxs(&root, b"the real VC9 CRT");
        std::fs::write(game.join(MSVCR90_DLL), b"something else entirely").unwrap();

        assert!(!remove_stray_msvcr90_against(&game, &[sxs]));
        assert!(game.join(MSVCR90_DLL).exists(), "an unrecognised file must survive");
    }

    /// Our copy was taken from whichever assembly was newest the day it was made. A
    /// servicing update since then leaves the newest folder holding different bytes, and the
    /// cleanup still has to fire — which is why every assembly is compared, not just the top
    /// one.
    #[test]
    fn a_copy_from_a_superseded_assembly_is_still_ours() {
        let root = scratch("superseded");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let old_dir = root.join("amd64_microsoft.vc90.crt_1fc8b3b9a1e18e3b_9.0.30729.6161_none_x");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join(MSVCR90_DLL), b"the older servicing build").unwrap();
        let newest = fake_sxs(&root, b"the newest servicing build");
        std::fs::write(game.join(MSVCR90_DLL), b"the older servicing build").unwrap();

        assert!(remove_stray_msvcr90_against(&game, &[newest, old_dir]));
        assert!(!game.join(MSVCR90_DLL).exists());
    }

    /// Nothing to do, and no source to compare against, are both quiet no-ops.
    #[test]
    fn an_empty_game_folder_is_a_no_op() {
        let root = scratch("empty");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        assert!(!remove_stray_msvcr90_against(&game, &[fake_sxs(&root, b"crt")]));
        std::fs::write(game.join(MSVCR90_DLL), b"crt").unwrap();
        assert!(!remove_stray_msvcr90_against(&game, &[]), "no WinSxS means no verdict");
        assert!(game.join(MSVCR90_DLL).exists());
    }

    /// The private assembly folder is the arrangement that actually works, and the cleanup
    /// reaches only the loose file beside it.
    #[test]
    fn the_private_assembly_folder_is_never_touched() {
        let root = scratch("private");
        let game = root.join("game");
        let private = game.join("Microsoft.VC90.CRT");
        std::fs::create_dir_all(&private).unwrap();
        std::fs::write(private.join(MSVCR90_DLL), b"the real VC9 CRT").unwrap();
        let sxs = fake_sxs(&root, b"the real VC9 CRT");

        assert!(!remove_stray_msvcr90_against(&game, &[sxs]));
        assert!(private.join(MSVCR90_DLL).is_file(), "a working private assembly must survive");
    }

    /// A private assembly is a route to the CRT; a loose DLL beside the exe is the R6034
    /// trap. Counting the latter would let a file we planted silence the detector that
    /// should be reporting this machine has no VC90 at all.
    #[test]
    fn only_the_private_assembly_counts_as_a_route_to_the_crt() {
        let root = scratch("present");
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::write(bare.join(MSVCR90_DLL), b"loose").unwrap();
        assert!(!game_dir_has_vc90(&bare), "a loose msvcr90.dll is not a working route");

        let proper = root.join("proper");
        std::fs::create_dir_all(proper.join("Microsoft.VC90.CRT")).unwrap();
        std::fs::write(proper.join("Microsoft.VC90.CRT").join(MSVCR90_DLL), b"crt").unwrap();
        assert!(game_dir_has_vc90(&proper));
    }
}
