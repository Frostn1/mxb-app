//! Is anything foreign attached to the running game?
//!
//! MX Bikes ships no anti-cheat of its own, so a client that has been hooked looks exactly
//! like one that hasn't — to the server, to the other riders, and to the game itself. The
//! cheats going round the Kaizo servers are injected DLLs and external trainers, and an
//! injected DLL has one property worth building on: to hook the game it has to be *inside*
//! the game, which means it is in the process's module list. That list is readable from out
//! here, and this module reads it.
//!
//! The shape of an answer is deliberately three-valued, not two:
//!
//!   * **Clean** — we read the module list and everything in it is accounted for.
//!   * **Suspect** — something is loaded that we can't account for. Not an accusation: a
//!     capture tool, an overlay nobody has told us about, or a driver shim all land here.
//!   * **Flagged** — it matched the rule list by name or by hash. This one is an accusation.
//!
//! and the fourth state, **Unknown**, is the one that matters most for honesty: the game
//! isn't up, or the platform can't be read, or the snapshot failed. Every path that cannot
//! see must say so rather than fall through to "clean" — a scanner that reports a healthy
//! machine when it simply didn't look is worse than no scanner, because people believe it.
//!
//! ## What this can and cannot do
//!
//! This is client-side attestation, and it is worth being blunt about the limit: a cheater
//! who does not run MXB App is not scanned, and one who patches this check out reports
//! whatever they like. It cannot make cheating impossible. What it does is make an honest
//! client able to *prove* it is honest — which is the thing a server admin currently has no
//! way to ask for — and raise the cost of the cheats actually in circulation, which are
//! downloaded DLLs run by people who are not going to rebuild the mod manager to hide them.
//! A manually-mapped module that never registers with the loader is likewise invisible here.
//! Treating a Clean report as proof of innocence is a mistake; treating a Flagged one as
//! worth a look is not.
//!
//! ## Rules
//!
//! Signatures live in [`RuleSet`], fetched from `integrity-rules.json` on `main` and cached
//! on disk — the same arrangement `supporters.json` uses, and for the same reason: a cheat
//! that appears on a Tuesday cannot wait for a release to be detectable. What ships in the
//! binary is [`RuleSet::bundled`], which carries the allowlist (the part that prevents false
//! accusations) and no signatures at all, so a build that never reaches the network is
//! cautious rather than wrong.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where the live rule list lives. Read from `main` rather than from a tag, so a new cheat
/// is one commit away from being detected on every install.
pub const RULES_URL: &str =
    "https://raw.githubusercontent.com/Frostn1/mxb-app/main/integrity-rules.json";

/// Cache file name, under the app's local data dir.
const RULES_CACHE: &str = "integrity-rules.json";

/// Long enough for a cold CDN, short enough that a scan isn't held up by a dead network.
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// Refuse to hash anything larger than this. A cheat DLL is a few megabytes; a hundred-
/// megabyte one would only be a way to make every scan stall on disk I/O.
const MAX_HASH_BYTES: u64 = 96 * 1024 * 1024;

/// Caps on what the remote list may contain. Nothing hostile is expected from our own repo,
/// but a rule file with a million entries shouldn't be able to make every scan quadratic.
const MAX_RULES: usize = 2000;
const MAX_ALLOW: usize = 2000;
const MAX_PATTERN_CHARS: usize = 120;

/// How bad a finding is.
///
/// Ordered worst-last so a report's verdict is `max()` over its findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Loaded, unrecognised, and not matched by any rule.
    Suspect,
    /// Matched the rule list.
    Flagged,
}

/// The overall answer for one scan. Ordered so the worst finding decides the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// We could not look. Never means "clean".
    Unknown,
    Clean,
    Suspect,
    Flagged,
}

/// Where a finding was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A DLL mapped into the game's own address space.
    Module,
    /// A separate process on the machine.
    Process,
}

/// Why we are reporting it, as a stable key the UI translates.
///
/// An enum rather than a prose string, for the reason [`crate::dropzone::DetectReason`] is:
/// the line shown to the user has to survive translation into six languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// Its name matched a rule.
    RuleName,
    /// Its contents matched a rule's hash — the strong one; renaming the file doesn't help.
    RuleHash,
    /// Loaded into the game from somewhere that isn't the game, the system, or anything we
    /// installed ourselves.
    ForeignModule,
    /// A DLL in the game's own folder, under one of the names the Windows loader will pull
    /// in for the executable beside it. The classic way to get code into a process without
    /// injecting anything — and, once ReShade is ruled out, not a shape anything innocent
    /// has.
    LoaderHijack,
}

/// One thing worth reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub kind: Kind,
    /// File name, lowercased — what the user recognises and what rules match on.
    pub name: String,
    /// Full path when we have one. Empty for a process: the Windows process walk carries
    /// names only.
    pub path: String,
    pub reason: Reason,
    pub severity: Severity,
    /// SHA-256 of the file, lowercase hex, when we could read it. This is what turns "some
    /// unknown DLL" into a signature somebody can add to the rule list, so it is computed
    /// for findings and never for the hundreds of files that check out fine.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    /// The rule's own label, when a rule matched.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// One scan's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub verdict: Verdict,
    pub findings: Vec<Finding>,
    /// Unix ms. Zero before the first scan.
    pub scanned_at: u64,
    /// Which rule list produced it, so a stale answer is recognisable as one.
    pub rules_version: u32,
    /// How many modules we looked at — the difference between a thorough Clean and an empty
    /// one, and the number that makes a report auditable.
    pub modules_scanned: usize,
    /// The game was running when we looked. A scan with no game is always [`Verdict::Unknown`].
    pub game_running: bool,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            verdict: Verdict::Unknown,
            findings: Vec::new(),
            scanned_at: 0,
            rules_version: 0,
            modules_scanned: 0,
            game_running: false,
        }
    }
}

impl Report {
    /// The verdict implied by a set of findings: the worst of them, or clean if there are
    /// none. Only ever called on a scan that actually read a module list.
    fn from_findings(findings: Vec<Finding>, modules_scanned: usize, rules_version: u32) -> Self {
        let verdict = findings
            .iter()
            .map(|f| match f.severity {
                Severity::Suspect => Verdict::Suspect,
                Severity::Flagged => Verdict::Flagged,
            })
            .max()
            .unwrap_or(Verdict::Clean);
        Self {
            verdict,
            findings,
            scanned_at: now_ms(),
            rules_version,
            modules_scanned,
            game_running: true,
        }
    }

    /// The answer for a machine we could not read. Distinct from clean, on purpose.
    pub fn unknown(game_running: bool, rules_version: u32) -> Self {
        Self {
            verdict: Verdict::Unknown,
            findings: Vec::new(),
            scanned_at: now_ms(),
            rules_version,
            modules_scanned: 0,
            game_running,
        }
    }
}

/// One signature. A rule matches on a name substring **or** a hash, never both — an entry
/// carrying each is two rules that happen to share a label.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    /// Lowercase substring, matched against a file name. Substring rather than equality
    /// because these things ship as `kaizo_v3.dll`, `kaizo (1).dll`, `KaizoLoader.dll`.
    pub contains: String,
    /// Lowercase hex SHA-256 of the whole file.
    pub sha256: String,
    /// What to call it in the report. Free text — the one string here that isn't translated,
    /// because a cheat's name is its name.
    pub label: String,
}

/// What the scanner matches against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleSet {
    /// Bumped by whoever edits the list. Reported alongside every verdict so an admin
    /// looking at a flag knows which list produced it.
    pub version: u32,
    /// Things that are cheats. Empty in the bundled set — see the module docs.
    pub cheats: Vec<Rule>,
    /// File names that are known-good wherever they load from. The escape hatch for a false
    /// positive: a new overlay or capture tool can be silenced within the hour.
    pub allow: Vec<String>,
    /// Lowercase path substrings whose contents are known-good. For whole toolchains that
    /// load a dozen DLLs each — a Wine build, a GPU driver stack.
    pub allow_paths: Vec<String>,
}

impl RuleSet {
    /// What the binary ships with: the allowlist, and no signatures.
    ///
    /// Signatures deliberately start empty. A shipped list would be out of date by the time
    /// anyone installed it, and a wrong entry accuses a real person of cheating — so the
    /// build errs toward saying "something unrecognised is loaded" and leaves naming it to
    /// the list that can be corrected in minutes.
    ///
    /// The allowlist is the opposite: it has to ship, because it is what stops an ordinary
    /// machine — Steam overlay, GPU driver, Discord, a capture tool — from lighting up on
    /// first launch, before anything has been fetched.
    pub fn bundled() -> Self {
        Self {
            version: 0,
            cheats: Vec::new(),
            allow: BUNDLED_ALLOW.iter().map(|s| s.to_string()).collect(),
            allow_paths: BUNDLED_ALLOW_PATHS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Fold a fetched list into the bundled one.
    ///
    /// The remote list *adds* to the bundled allowlist rather than replacing it. A remote
    /// file that arrived truncated, or an editing mistake that drops a line, must not be
    /// able to turn every machine's Steam overlay into a finding.
    pub fn merged_with_bundled(mut self) -> Self {
        let bundled = Self::bundled();
        self.allow.extend(bundled.allow);
        self.allow_paths.extend(bundled.allow_paths);
        self.normalize()
    }

    /// Lowercase everything, drop empties, and apply the caps. Run on anything from the
    /// network before it is matched against.
    pub fn normalize(mut self) -> Self {
        let clean = |v: Vec<String>, cap: usize| -> Vec<String> {
            let mut out: Vec<String> = v
                .into_iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty() && s.chars().count() <= MAX_PATTERN_CHARS)
                .collect();
            out.sort();
            out.dedup();
            out.truncate(cap);
            out
        };
        self.allow = clean(self.allow, MAX_ALLOW);
        // Written with either slash in the file; matched against paths we normalize the
        // same way.
        self.allow_paths =
            clean(self.allow_paths, MAX_ALLOW).into_iter().map(|p| p.replace('\\', "/")).collect();
        self.cheats.truncate(MAX_RULES);
        self.cheats.retain_mut(|r| {
            r.contains = r.contains.trim().to_ascii_lowercase();
            r.sha256 = r.sha256.trim().to_ascii_lowercase();
            r.label = r.label.trim().chars().take(MAX_PATTERN_CHARS).collect();
            // A rule with neither pattern matches everything or nothing depending on how it
            // is read. Neither is a thing anyone meant, so it is dropped.
            !r.contains.is_empty() || is_sha256(&r.sha256)
        });
        self
    }

    /// Does any rule name this file? Returns the matching rule.
    fn name_rule(&self, name: &str) -> Option<&Rule> {
        self.cheats.iter().find(|r| !r.contains.is_empty() && name.contains(&r.contains))
    }

    /// Does any rule carry this hash?
    fn hash_rule(&self, sha256: &str) -> Option<&Rule> {
        self.cheats.iter().find(|r| !r.sha256.is_empty() && r.sha256 == sha256)
    }

    /// Is this file name explicitly known-good?
    fn allows_name(&self, name: &str) -> bool {
        self.allow.iter().any(|a| a == name)
    }

    /// Is this path inside something explicitly known-good?
    fn allows_path(&self, path: &str) -> bool {
        self.allow_paths.iter().any(|a| path.contains(a.as_str()))
    }

    /// Do we hold any signatures at all?
    ///
    /// A list with none can still find things — the heuristics don't need it — but it can
    /// never say *what* it found, and the UI says so rather than implying an all-clear was
    /// checked against something.
    pub fn has_signatures(&self) -> bool {
        !self.cheats.is_empty()
    }
}

/// True for a well-formed lowercase hex SHA-256.
fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Names that legitimately live inside a game process.
///
/// Not an exhaustive list of good software — it can't be — but the things that would
/// otherwise make the first scan on an ordinary gaming PC produce a page of findings.
/// Everything here loads from somewhere the path rules don't already cover: an overlay
/// injects from its own install, which is exactly the shape a cheat has.
const BUNDLED_ALLOW: [&str; 29] = [
    // Steam: the overlay is injected into every game Steam starts.
    "gameoverlayrenderer64.dll",
    "gameoverlayrenderer.dll",
    "steamclient64.dll",
    "steamclient.dll",
    "steam_api64.dll",
    "steam_api.dll",
    "steamoverlayvulkanlayer64.dll",
    // Valve's own runtime, which `steamclient` pulls in from the Steam install rather than
    // from the game folder or the system folder. Missing these made every scan on every
    // Steam machine report two Suspect findings — the shape of a false positive that
    // teaches people to ignore the verdict.
    "tier0_s64.dll",
    "vstdlib_s64.dll",
    "tier0.dll",
    "vstdlib.dll",
    // Discord's in-game overlay.
    "discordhook64.dll",
    "discordhook.dll",
    // RivaTuner Statistics Server — ships with MSI Afterburner, so it is on a large share
    // of the machines this will ever run on.
    "rtsshooks64.dll",
    "rtsshooks.dll",
    // OBS and the game-capture path Streamlabs shares with it.
    "graphics-hook64.dll",
    "graphics-hook32.dll",
    // NVIDIA: GeForce Experience / ShadowPlay overlay and the Reflex + upscaling runtime.
    "nvspcap64.dll",
    "nvspcap.dll",
    "nvngx.dll",
    "nvngx_dlss.dll",
    "nvcamera64.dll",
    // AMD's equivalent.
    "amfrt64.dll",
    "amf-mft-decvce-avc64.dll",
    // ReShade, when it is installed under its own name rather than as a proxy DLL. The
    // proxy case is handled by asking the file — see [`is_loader_hijack`].
    "reshade64.dll",
    "reshade32.dll",
    // FrostMod, which is the whole point of this app being able to talk to the game.
    "frostmod.dll",
    "frostmod64.dll",
    // Wine's own loader stubs, which appear under the game's folder inside a prefix.
    "winemenubuilder.exe",
];

/// Path fragments whose contents are known-good, lowercased, forward slashes.
///
/// Where [`BUNDLED_ALLOW`] names individual files, these cover whole toolchains that map a
/// dozen libraries each and would be tedious and fragile to enumerate.
const BUNDLED_ALLOW_PATHS: [&str; 8] = [
    // Wine and Proton builtins on Linux. Under Proton the game's `kernel32.dll` genuinely
    // does come from `/…/Proton - Experimental/files/lib/wine/…`, which is neither the
    // system folder nor the game folder.
    "/wine/",
    "/proton",
    "/dist/lib/",
    "/files/lib/",
    // Steam's own runtime and overlay libraries, on both platforms.
    "/steamworks shared/",
    "/linux64/",
    // GPU drivers, which map into every 3D process.
    "/nvidia/",
    "/amd/",
];

/// DLL names the Windows loader will pull in from the executable's own folder before it
/// looks anywhere else.
///
/// A file with one of these names sitting next to `mxbikes.exe` is loaded into the game
/// without anything having to inject it — which makes it the cheapest way to hook a game
/// that has no anti-cheat, and the one shape worth calling out even though the file is
/// technically "in the game folder" and would otherwise be trusted.
///
/// `opengl32.dll` is on the list and is also how ReShade legitimately installs here (MX
/// Bikes is an OpenGL title — see [`crate::reshade`]), which is why the check reads the
/// file rather than trusting the name.
const LOADER_NAMES: [&str; 12] = [
    "opengl32.dll",
    "dxgi.dll",
    "d3d9.dll",
    "d3d10.dll",
    "d3d11.dll",
    "d3d12.dll",
    "dinput8.dll",
    "dsound.dll",
    "winmm.dll",
    "version.dll",
    "wininet.dll",
    "xinput1_3.dll",
];

/// Everything one scan needs to know about this machine.
///
/// Built once per scan and passed to the pure classifier, which is what lets the whole
/// decision be tested with made-up paths on a machine with no game installed.
#[derive(Debug, Clone, Default)]
pub struct ScanContext {
    /// Folders whose contents are the player's own install: the game, FrostMod, ReShade.
    /// Lowercased with forward slashes.
    pub trusted_roots: Vec<String>,
    /// The game's install folder specifically — the one place a loader hijack can happen.
    pub game_dir: String,
}

impl ScanContext {
    pub fn new(game_dir: &Path, extra_roots: &[PathBuf]) -> Self {
        let mut trusted_roots: Vec<String> = extra_roots.iter().map(|p| norm(p)).collect();
        let game_dir = norm(game_dir);
        if !game_dir.is_empty() {
            trusted_roots.push(game_dir.clone());
        }
        trusted_roots.retain(|r| !r.is_empty());
        Self { trusted_roots, game_dir }
    }

    fn is_trusted_root(&self, path: &str) -> bool {
        self.trusted_roots.iter().any(|root| is_inside(path, root))
    }

    fn is_in_game_dir(&self, path: &str) -> bool {
        !self.game_dir.is_empty() && is_inside(path, &self.game_dir)
    }
}

/// A path lowercased with forward slashes, so Windows and Wine spellings of the same folder
/// compare equal.
fn norm(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
}

/// Is `path` inside `root`? Compared on path *boundaries*, so `C:/games/mx` does not swallow
/// `C:/games/mxcheat`.
fn is_inside(path: &str, root: &str) -> bool {
    path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// Is this one of Windows' own DLLs, by where it lives?
///
/// Matched on the shape of the path rather than on a substring, because a substring test
/// would hand a free pass to anything that put itself in `C:\kaizo\windows\system32\`. The
/// two legitimate spellings are a drive root (`C:\Windows\System32\…`) and the same folder
/// inside a Wine prefix (`…/pfx/drive_c/windows/system32/…`).
fn is_system_path(path: &str) -> bool {
    const SYSTEM_DIRS: [&str; 4] = ["system32/", "syswow64/", "winsxs/", "globalization/"];
    let Some((root, rest)) = path.split_once("/windows/") else { return false };
    let rooted = root.len() == 2 && root.ends_with(':') || root.ends_with("/drive_c");
    rooted && SYSTEM_DIRS.iter().any(|d| rest.starts_with(d))
}

/// The file name from a path, lowercased.
fn file_name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase()
}

/// Whether a module in the game's own folder is there to hijack the loader.
///
/// Only asks the question for the names the loader actually preloads, and only answers yes
/// once the file has been read and is not ReShade. `probe` is the ReShade test, passed in so
/// the classifier stays pure and testable.
fn is_loader_hijack(name: &str, path: &str, ctx: &ScanContext, probe: &dyn Fn(&Path) -> bool) -> bool {
    LOADER_NAMES.contains(&name) && ctx.is_in_game_dir(path) && !probe(Path::new(path))
}

/// Turn one module path into a finding, or `None` if it checks out.
///
/// The order here is the whole policy, and it is deliberate:
///
///   1. **Rules first.** A name or hash on the list is a cheat wherever it loaded from — a
///      cheat that copies itself into the game folder must not be trusted for being there.
///   2. **Allowlist next**, so a false positive can be silenced without a release.
///   3. **Loader hijack**, which is the one case where being in the game folder is itself
///      the evidence rather than a defence.
///   4. **Trusted roots and the system folder**, which is where the overwhelming majority of
///      an ordinary module list ends up.
///   5. Anything left is loaded into the game from somewhere unaccounted for: Suspect.
fn classify_module(
    path: &str,
    ctx: &ScanContext,
    rules: &RuleSet,
    hash: &dyn Fn(&Path) -> String,
    is_reshade: &dyn Fn(&Path) -> bool,
) -> Option<Finding> {
    let path = path.replace('\\', "/");
    let lower = path.to_ascii_lowercase();
    let name = file_name_of(&lower);

    let finding = |reason: Reason, severity: Severity, label: String, sha256: String| {
        Some(Finding {
            kind: Kind::Module,
            name: name.clone(),
            path: path.clone(),
            reason,
            severity,
            sha256,
            label,
        })
    };

    if let Some(rule) = rules.name_rule(&name) {
        // Hash it anyway: a name match is the weak half of a signature, and the digest is
        // what confirms it later.
        return finding(Reason::RuleName, Severity::Flagged, rule.label.clone(), hash(Path::new(&path)));
    }
    // Hashing is the expensive step, so it happens once, here, and only when there are
    // hash rules to check against — never for the hundreds of modules that will pass.
    let digest = if rules.cheats.iter().any(|r| !r.sha256.is_empty()) {
        hash(Path::new(&path))
    } else {
        String::new()
    };
    if !digest.is_empty() {
        if let Some(rule) = rules.hash_rule(&digest) {
            return finding(Reason::RuleHash, Severity::Flagged, rule.label.clone(), digest);
        }
    }

    if rules.allows_name(&name) || rules.allows_path(&lower) {
        return None;
    }
    if is_loader_hijack(&name, &lower, ctx, is_reshade) {
        return finding(
            Reason::LoaderHijack,
            Severity::Suspect,
            String::new(),
            hash(Path::new(&path)),
        );
    }
    if ctx.is_trusted_root(&lower) || is_system_path(&lower) {
        return None;
    }
    finding(Reason::ForeignModule, Severity::Suspect, String::new(), hash(Path::new(&path)))
}

/// Turn one running process name into a finding, or `None`.
///
/// Processes are matched by rule only. There is no heuristic worth having here: a machine
/// runs two hundred processes and none of them being recognised is normal, so "unrecognised
/// process" would flag everybody. This exists so the rule list can name an external trainer
/// that never injects anything.
fn classify_process(name: &str, rules: &RuleSet) -> Option<Finding> {
    let name = name.to_ascii_lowercase();
    if rules.allows_name(&name) {
        return None;
    }
    let rule = rules.name_rule(&name)?;
    Some(Finding {
        kind: Kind::Process,
        name,
        path: String::new(),
        reason: Reason::RuleName,
        severity: Severity::Flagged,
        sha256: String::new(),
        label: rule.label.clone(),
    })
}

/// Classify a whole machine. Pure — every fact it needs is an argument.
pub fn classify(
    modules: &[String],
    processes: &[String],
    ctx: &ScanContext,
    rules: &RuleSet,
    hash: &dyn Fn(&Path) -> String,
    is_reshade: &dyn Fn(&Path) -> bool,
) -> Report {
    let mut findings: Vec<Finding> = modules
        .iter()
        .filter_map(|m| classify_module(m, ctx, rules, hash, is_reshade))
        .collect();
    findings.extend(processes.iter().filter_map(|p| classify_process(p, rules)));

    // One entry per file. A DLL is mapped many times over — once per section on Linux — and
    // the module list can spell the same path either way, so the identity is the *lowercased*
    // path rather than the one we display. Two genuinely distinct files on a case-sensitive
    // filesystem differing only in case would merge; that is a trade worth making against
    // every Linux player seeing each finding six times.
    let mut seen = std::collections::HashSet::new();
    findings.retain(|f| seen.insert((f.name.clone(), f.path.to_ascii_lowercase())));
    // Worst first, so the UI's first line is the one that matters.
    findings.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then_with(|| a.name.cmp(&b.name)).then_with(|| a.path.cmp(&b.path))
    });
    Report::from_findings(findings, modules.len(), rules.version)
}

/// SHA-256 of a file, lowercase hex. Empty when it can't be read or is implausibly large —
/// a missing digest costs a weaker report, never a wrong one.
pub fn hash_file(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let Ok(meta) = std::fs::metadata(path) else { return String::new() };
    if !meta.is_file() || meta.len() > MAX_HASH_BYTES {
        return String::new();
    }
    let Ok(mut file) = std::fs::File::open(path) else { return String::new() };
    let mut hasher = Sha256::new();
    if std::io::copy(&mut file, &mut hasher).is_err() {
        return String::new();
    }
    format!("{:x}", hasher.finalize())
}

/// Scan this machine now.
///
/// `None` from either enumeration is what makes the difference between a report and an
/// [`Verdict::Unknown`]: the game not being up, or a platform with nothing to read, must
/// never produce a clean bill of health.
pub fn scan(ctx: &ScanContext, rules: &RuleSet) -> Report {
    let game_running = crate::gameproc::is_game_running();
    let Some(modules) = crate::gameproc::game_module_paths() else {
        return Report::unknown(game_running, rules.version);
    };
    // Processes are a bonus: the module list is the real answer, and a failed process walk
    // shouldn't throw it away.
    let processes = crate::gameproc::running_process_names().unwrap_or_default();
    classify(&modules, &processes, ctx, rules, &hash_file, &crate::reshade::is_reshade_dll)
}

// ---------------------------------------------------------------------------
// The rule list
// ---------------------------------------------------------------------------

fn cache_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    app.path().app_local_data_dir().ok().map(|dir| dir.join(RULES_CACHE))
}

/// The best rule list available without touching the network: the cache if we have one,
/// otherwise what the binary shipped with.
pub fn cached_rules(app: &tauri::AppHandle) -> RuleSet {
    cache_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<RuleSet>(&text).ok())
        .map(RuleSet::merged_with_bundled)
        .unwrap_or_else(RuleSet::bundled)
}

/// Fetch the live rule list and cache it.
///
/// Best-effort by design, and the failure mode is the point: an unreachable network leaves
/// the last good list in place, and a machine that has never reached it still scans with the
/// bundled allowlist. Nothing about a scan waits on this succeeding.
pub async fn refresh_rules(app: &tauri::AppHandle) -> anyhow::Result<RuleSet> {
    let text = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()?
        .get(RULES_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    // Parse before writing: a cache holding something we can't read would keep failing on
    // every launch until someone cleared it by hand.
    let rules: RuleSet = serde_json::from_str(&text)?;
    if let Some(path) = cache_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &text) {
            log::warn!("[integrity] couldn't cache the rule list: {e}");
        }
    }
    log::info!("[integrity] rule list v{} ({} signatures)", rules.version, rules.cheats.len());
    Ok(rules.merged_with_bundled())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with the game installed on Windows and nothing unusual on it.
    fn ctx() -> ScanContext {
        ScanContext::new(
            Path::new("C:\\Games\\MX Bikes"),
            &[PathBuf::from("C:\\Users\\rider\\AppData\\Local\\com.frost.mxbikes\\frostmod")],
        )
    }

    fn no_hash(_: &Path) -> String {
        String::new()
    }
    fn not_reshade(_: &Path) -> bool {
        false
    }
    fn is_reshade(_: &Path) -> bool {
        true
    }

    fn scan_with(modules: &[&str], rules: &RuleSet) -> Report {
        let modules: Vec<String> = modules.iter().map(|s| s.to_string()).collect();
        classify(&modules, &[], &ctx(), rules, &no_hash, &not_reshade)
    }

    /// The baseline that decides whether anyone keeps the feature turned on. An ordinary
    /// module list — the game, Windows, the Steam overlay, a GPU driver, FrostMod — has to
    /// come back clean, or the first scan buries the real signal in noise.
    #[test]
    fn an_ordinary_machine_is_clean() {
        let report = scan_with(
            &[
                "C:\\Games\\MX Bikes\\mxbikes.exe",
                "C:\\Games\\MX Bikes\\core.dll",
                "C:\\WINDOWS\\System32\\kernel32.dll",
                "C:\\Windows\\SysWOW64\\user32.dll",
                "C:\\Windows\\WinSxS\\x86_amd64_something\\comctl32.dll",
                "C:\\Program Files (x86)\\Steam\\GameOverlayRenderer64.dll",
                "C:\\Users\\rider\\AppData\\Local\\com.frost.mxbikes\\frostmod\\FrostMod.dll",
                "C:\\Program Files\\NVIDIA Corporation\\nvspcap64.dll",
            ],
            &RuleSet::bundled(),
        );
        assert_eq!(report.verdict, Verdict::Clean, "findings: {:?}", report.findings);
        assert_eq!(report.modules_scanned, 8);
    }

    /// The case the whole feature exists for: a DLL loaded into the game from a download
    /// folder. No rule names it and none has to — being in there at all is the finding.
    #[test]
    fn a_dll_injected_from_somewhere_else_is_suspect() {
        let report =
            scan_with(&["C:\\Users\\rider\\Downloads\\kaizo_v3.dll"], &RuleSet::bundled());
        assert_eq!(report.verdict, Verdict::Suspect);
        assert_eq!(report.findings[0].reason, Reason::ForeignModule);
        assert_eq!(report.findings[0].name, "kaizo_v3.dll");
    }

    /// With a signature it stops being "something unrecognised" and gets a name.
    #[test]
    fn a_rule_turns_a_suspect_module_into_a_flagged_one() {
        let rules = RuleSet {
            version: 7,
            cheats: vec![Rule {
                contains: "kaizo".into(),
                label: "Kaizo trainer".into(),
                ..Default::default()
            }],
            ..RuleSet::bundled()
        }
        .normalize();
        let report = scan_with(&["C:\\Users\\rider\\Downloads\\Kaizo_v3.dll"], &rules);
        assert_eq!(report.verdict, Verdict::Flagged);
        assert_eq!(report.findings[0].reason, Reason::RuleName);
        assert_eq!(report.findings[0].label, "Kaizo trainer");
        assert_eq!(report.rules_version, 7);
    }

    /// Rules outrank location. A cheat that copies itself in beside the game is still a
    /// cheat — this is the ordering the classifier is built around.
    #[test]
    fn a_named_cheat_inside_the_game_folder_is_still_flagged() {
        let rules = RuleSet {
            cheats: vec![Rule { contains: "kaizo".into(), label: "Kaizo".into(), ..Default::default() }],
            ..RuleSet::bundled()
        }
        .normalize();
        let report = scan_with(&["C:\\Games\\MX Bikes\\kaizo.dll"], &rules);
        assert_eq!(report.verdict, Verdict::Flagged);
    }

    /// A hash rule catches the same file renamed, which is the first thing anyone tries.
    #[test]
    fn a_hash_rule_catches_a_renamed_cheat() {
        let digest = "a".repeat(64);
        let rules = RuleSet {
            cheats: vec![Rule {
                sha256: digest.clone(),
                label: "Kaizo v3".into(),
                ..Default::default()
            }],
            ..RuleSet::bundled()
        }
        .normalize();
        let hash = |_: &Path| digest.clone();
        let modules = vec!["C:\\Games\\MX Bikes\\perfectly_innocent.dll".to_string()];
        let report = classify(&modules, &[], &ctx(), &rules, &hash, &not_reshade);
        assert_eq!(report.verdict, Verdict::Flagged);
        assert_eq!(report.findings[0].reason, Reason::RuleHash);
        assert_eq!(report.findings[0].sha256, digest);
    }

    /// The allowlist is the release valve for a false positive, and it has to beat the
    /// heuristics — that is the entire reason it exists.
    #[test]
    fn the_allowlist_silences_a_heuristic_finding() {
        let rules = RuleSet { allow: vec!["newoverlay.dll".into()], ..RuleSet::bundled() }.normalize();
        let report = scan_with(&["C:\\Program Files\\NewOverlay\\NewOverlay.dll"], &rules);
        assert_eq!(report.verdict, Verdict::Clean);
    }

    /// …but it must not be able to un-flag a known cheat. Otherwise one bad line in a file
    /// anyone can open a PR against turns the whole thing off.
    #[test]
    fn the_allowlist_cannot_override_a_signature() {
        let rules = RuleSet {
            cheats: vec![Rule { contains: "kaizo".into(), label: "Kaizo".into(), ..Default::default() }],
            allow: vec!["kaizo.dll".into()],
            ..RuleSet::bundled()
        }
        .normalize();
        assert_eq!(scan_with(&["C:\\anywhere\\kaizo.dll"], &rules).verdict, Verdict::Flagged);
    }

    /// A DLL under one of the loader's preload names, in the game folder, that isn't
    /// ReShade — code in the process with nothing injected.
    #[test]
    fn a_proxy_dll_in_the_game_folder_is_suspect() {
        let report = scan_with(&["C:\\Games\\MX Bikes\\dinput8.dll"], &RuleSet::bundled());
        assert_eq!(report.verdict, Verdict::Suspect);
        assert_eq!(report.findings[0].reason, Reason::LoaderHijack);
    }

    /// ReShade installs as exactly that shape and is not a cheat, so the check reads the
    /// file instead of trusting the name.
    #[test]
    fn a_reshade_opengl32_in_the_game_folder_is_clean() {
        let modules = vec!["C:\\Games\\MX Bikes\\opengl32.dll".to_string()];
        let report = classify(&modules, &[], &ctx(), &RuleSet::bundled(), &no_hash, &is_reshade);
        assert_eq!(report.verdict, Verdict::Clean);
        // The same file, not ReShade, is the hijack it looks like.
        let report = classify(&modules, &[], &ctx(), &RuleSet::bundled(), &no_hash, &not_reshade);
        assert_eq!(report.verdict, Verdict::Suspect);
    }

    /// Windows' own folders are trusted by shape, not by substring — or anything could put
    /// itself in a folder called `windows\system32` and be waved through.
    #[test]
    fn only_a_real_system_folder_is_a_system_folder() {
        assert!(is_system_path("c:/windows/system32/kernel32.dll"));
        assert!(is_system_path("c:/windows/syswow64/user32.dll"));
        assert!(is_system_path(
            "/home/rider/.steam/steamapps/compatdata/655500/pfx/drive_c/windows/system32/kernel32.dll"
        ));
        assert!(!is_system_path("c:/kaizo/windows/system32/evil.dll"));
        assert!(!is_system_path("c:/windows/temp/evil.dll"));
    }

    /// Path containment is on boundaries. A folder that merely starts with the game's path
    /// is not inside it.
    #[test]
    fn a_sibling_folder_is_not_inside_the_game_folder() {
        let report = scan_with(&["C:\\Games\\MX Bikes Cheats\\loader.dll"], &RuleSet::bundled());
        assert_eq!(report.verdict, Verdict::Suspect);
    }

    /// Under Proton the game's own runtime comes out of the Proton build, which is neither
    /// the system folder nor the game folder. Without the path allowlist every Linux player
    /// would see a hundred findings.
    #[test]
    fn proton_builtins_are_clean() {
        let ctx = ScanContext::new(
            Path::new("/home/rider/.steam/steam/steamapps/common/MX Bikes"),
            &[],
        );
        let modules: Vec<String> = [
            "/home/rider/.steam/steam/steamapps/common/MX Bikes/mxbikes.exe",
            "/home/rider/.steam/steam/steamapps/common/Proton - Experimental/files/lib/wine/x86_64-windows/kernel32.dll",
            "/home/rider/.steam/steam/steamapps/compatdata/655500/pfx/drive_c/windows/system32/ntdll.dll",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let report =
            classify(&modules, &[], &ctx, &RuleSet::bundled(), &no_hash, &not_reshade);
        assert_eq!(report.verdict, Verdict::Clean, "findings: {:?}", report.findings);
    }

    /// A process is only ever reported by name, and only when a rule names it.
    #[test]
    fn processes_are_matched_by_rule_only() {
        let rules = RuleSet {
            cheats: vec![Rule {
                contains: "kaizotrainer".into(),
                label: "Kaizo trainer".into(),
                ..Default::default()
            }],
            ..RuleSet::bundled()
        }
        .normalize();
        let processes: Vec<String> =
            ["explorer.exe", "steam.exe", "KaizoTrainer.exe"].iter().map(|s| s.to_string()).collect();
        let report = classify(&[], &processes, &ctx(), &rules, &no_hash, &not_reshade);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].kind, Kind::Process);
        assert_eq!(report.findings[0].name, "kaizotrainer.exe");
    }

    /// Worst finding decides, and the list leads with it.
    #[test]
    fn the_worst_finding_decides_the_verdict_and_sorts_first() {
        let rules = RuleSet {
            cheats: vec![Rule { contains: "kaizo".into(), label: "Kaizo".into(), ..Default::default() }],
            ..RuleSet::bundled()
        }
        .normalize();
        let report = scan_with(
            &["C:\\Users\\rider\\Downloads\\unknown.dll", "C:\\Users\\rider\\Downloads\\kaizo.dll"],
            &rules,
        );
        assert_eq!(report.verdict, Verdict::Flagged);
        assert_eq!(report.findings[0].name, "kaizo.dll");
        assert_eq!(report.findings.len(), 2);
    }

    /// A module mapped more than once is one finding. On Linux every DLL appears in the
    /// mapping list several times over.
    #[test]
    fn the_same_module_twice_is_one_finding() {
        let report = scan_with(
            &["C:\\Downloads\\x.dll", "C:\\Downloads\\x.dll", "C:\\Downloads\\X.DLL"],
            &RuleSet::bundled(),
        );
        assert_eq!(report.findings.len(), 1);
    }

    /// The honesty property. Everything that could not look reports Unknown, and Unknown is
    /// not Clean — including the ordering, which the UI relies on to rank a grid.
    #[test]
    fn a_machine_we_could_not_read_is_unknown_not_clean() {
        let report = Report::unknown(false, 3);
        assert_eq!(report.verdict, Verdict::Unknown);
        assert!(report.findings.is_empty());
        assert_eq!(report.rules_version, 3);
        assert!(Verdict::Unknown < Verdict::Clean);
        assert!(Verdict::Clean < Verdict::Suspect);
        assert!(Verdict::Suspect < Verdict::Flagged);
    }

    /// What the binary ships with: an allowlist and no accusations.
    #[test]
    fn the_bundled_set_carries_no_signatures() {
        let bundled = RuleSet::bundled();
        assert!(!bundled.has_signatures());
        assert!(!bundled.allow.is_empty());
    }

    /// A remote list adds to the bundled allowlist rather than replacing it, so a truncated
    /// fetch can't make every machine's Steam overlay a finding.
    #[test]
    fn a_remote_list_cannot_drop_the_bundled_allowlist() {
        let remote = RuleSet { version: 4, ..Default::default() }.merged_with_bundled();
        assert!(remote.allows_name("gameoverlayrenderer64.dll"));
        assert_eq!(remote.version, 4);
    }

    /// Whatever the file's spelling, matching is lowercase — and a rule with no pattern at
    /// all is dropped rather than being read as "match everything".
    #[test]
    fn normalizing_lowercases_patterns_and_drops_empty_rules() {
        let rules = RuleSet {
            cheats: vec![
                Rule { contains: "  KAIZO  ".into(), ..Default::default() },
                Rule { label: "nothing to match on".into(), ..Default::default() },
                Rule { sha256: "NOTAHASH".into(), ..Default::default() },
            ],
            allow: vec!["  Overlay.DLL ".into(), "   ".into()],
            allow_paths: vec!["C:\\Vendor\\Tool".into()],
            ..Default::default()
        }
        .normalize();
        assert_eq!(rules.cheats.len(), 1);
        assert_eq!(rules.cheats[0].contains, "kaizo");
        assert_eq!(rules.allow, ["overlay.dll"]);
        assert_eq!(rules.allow_paths, ["c:/vendor/tool"]);
    }

    /// The rule file is JSON with camelCase keys — the shape anyone editing it will write.
    #[test]
    fn the_rule_file_parses_as_written() {
        let text = r#"{
          "version": 12,
          "cheats": [
            { "contains": "kaizo", "label": "Kaizo trainer" },
            { "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
              "label": "Kaizo v3 loader" }
          ],
          "allow": ["someoverlay.dll"],
          "allowPaths": ["/vendor/tool/"]
        }"#;
        let rules: RuleSet = serde_json::from_str(text).unwrap();
        let rules = rules.merged_with_bundled();
        assert_eq!(rules.version, 12);
        assert_eq!(rules.cheats.len(), 2);
        assert!(rules.has_signatures());
        assert!(rules.allows_name("someoverlay.dll"));
        assert!(rules.allows_path("/vendor/tool/x.dll"));
    }
}
