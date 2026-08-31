use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// FrostMod's GitHub repo — releases carry `frostmod.exe` + `frostmod.dll`.
const REPO: &str = "Frostn1/frostmod";
pub const UA: &str = "mxb-app";

/// The binaries a FrostMod release has to ship. Both land or neither does — a new
/// `frostmod.exe` beside an old `frostmod.dll` is a worse state than not updating.
const BINARIES: [&str; 2] = ["frostmod.exe", "frostmod.dll"];

/// Marks a binary moved aside because something still had it open. Swept on the
/// next install or start, by which point whatever held it has usually exited.
const RETIRED_MARK: &str = ".in-use-";

/// Managed FrostMod child process (only ever `Some` on Windows while running).
#[derive(Default)]
pub struct FrostmodProcess(pub Mutex<Option<std::process::Child>>);

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrostmodStatus {
    /// Whether `frostmod.exe` is present in our managed folder.
    pub installed: bool,
    /// Installed release tag, if known.
    pub version: Option<String>,
    /// Latest release tag on GitHub (None if the check failed / offline).
    pub latest: Option<String>,
    /// The binaries on disk aren't the ones the recorded version ships — an install
    /// that didn't fully apply. Reinstalling is the fix.
    pub needs_repair: bool,
    /// Whether FrostMod is currently running (its reload event exists).
    pub running: bool,
    /// Whether the installed build is safe to run against the active game. False means
    /// "installed, but too old for this title" — see `frostmod::supported_for_game`. The
    /// UI offers an update instead of a start; starting it anyway is what crashed GP
    /// Bikes, so `start` refuses too.
    pub supported_for_game: bool,
    /// Visual C++ runtimes this machine is short of. Empty is the normal case (and always
    /// the case off Windows). Non-empty means FrostMod will very likely fail to attach with
    /// a bare "…dll was not found" box over the game — see `crate::vcruntime`.
    ///
    /// Unlike the flags above this does **not** gate `start`: we can't prove from out here
    /// which machines inject fine, and refusing to launch would take FrostMod away from
    /// anyone the detection is wrong about. It's a warning with a fix attached.
    pub missing_runtimes: Vec<crate::vcruntime::Runtime>,
    /// A loose `msvcr90.dll` beside the game exe that we didn't remove. `Clear`/`Removed`
    /// mean there is nothing to say; the other two mean the game will die with R6034 the
    /// next time something plain-imports the CRT, and only the player can authorise the
    /// fix — see `crate::vcruntime::disable_stray_msvcr90`.
    pub stray_msvcr90: crate::vcruntime::Stray,
}

/// The folder we install FrostMod into and run it from — so also where anything it
/// writes relative to its working directory lands (see `start`, which sets `current_dir`
/// to it). Public for `logs`, which offers that folder up when a report needs it.
pub fn frostmod_dir(app: &AppHandle) -> PathBuf {
    // Local app-data dir (Windows: `%LOCALAPPDATA%\com.frost.mxbikes\frostmod`).
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("frostmod")
}

fn exe_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join("frostmod.exe")
}

fn version_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join("version.txt")
}

/// FrostMod's server-browser filter file (its stock default hides Kaizo).
const SERVERFILTER_FILE: &str = "frostmod_serverfilter.yaml";

/// Curated filter: v4 sentinel kept, spam regex kept, Kaizo rules removed.
const CURATED_SERVERFILTER: &str = "# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|\\.pr0\\b)'
";

/// FrostMod's stock v4 default (the one that hides Kaizo).
const STOCK_SERVERFILTER: &str = "# frostmod-filter v4
# FrostMod server filter - hide spam/ad servers from the online browser.
# Hidden if the name contains any 'names' entry or matches any 'regex'.
hideUnjoinable: false   # ping '---' - unreliable at list time, keep off
hideEmpty: false        # hide 0-player servers (many legit ones are just empty)
hideLocked: false       # hide password-locked servers
maxPerIP: 0             # 0 = off; else hide servers past N from one IP per refresh
names:                  # case-insensitive substrings
  - che4ts
  - kaizo
  - kalz0
regex:                  # ECMAScript regex; single-quote to keep backslashes literal
  - '(che[a4]ts|k[a4][il1]z[o0]|\\.pr0\\b)'
";

fn serverfilter_path(app: &AppHandle) -> PathBuf {
    frostmod_dir(app).join(SERVERFILTER_FILE)
}

/// Compare filter text ignoring line endings (CRLF) and trailing blank space.
fn filter_eq(a: &str, b: &str) -> bool {
    a.replace('\r', "").trim_end() == b.replace('\r', "").trim_end()
}

/// Write our curated server filter, unless the user has edited it. Best-effort.
pub fn ensure_serverfilter(app: &AppHandle) {
    let path = serverfilter_path(app);
    let should_write = match std::fs::read_to_string(&path) {
        Ok(cur) => filter_eq(&cur, STOCK_SERVERFILTER),
        Err(_) => true, // missing / unreadable -> lay down our copy
    };
    if !should_write {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, CURATED_SERVERFILTER) {
        Ok(()) => log::info!("wrote curated FrostMod server filter (Kaizo unhidden): {}", path.display()),
        Err(e) => log::warn!("could not write FrostMod server filter {}: {e}", path.display()),
    }
}

/// The release tag our installer recorded for the FrostMod on disk, if any.
pub fn installed_version(app: &AppHandle) -> Option<String> {
    std::fs::read_to_string(version_path(app))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn is_installed(app: &AppHandle) -> bool {
    exe_path(app).exists()
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// Byte length GitHub reports for the asset.
    size: u64,
    /// `sha256:<hex>`, when the release carries one (older releases may not).
    digest: Option<String>,
}

async fn latest_release() -> anyhow::Result<Release> {
    let client = reqwest::Client::builder().user_agent(UA).build()?;
    let rel = client
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json::<Release>()
        .await?;
    Ok(rel)
}

/// Does the file at `path` hold exactly what the release says the asset is?
///
/// Size first because it settles almost every mismatch without reading anything; the
/// digest then makes it exact. A release that advertises no digest gets the size check
/// alone rather than a free pass on nothing.
fn file_matches_asset(path: &Path, asset: &Asset) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() != asset.size {
        return false;
    }
    let Some(want) = asset.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")) else {
        return true;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    use sha2::{Digest, Sha256};
    let got = Sha256::digest(&bytes);
    format!("{got:x}").eq_ignore_ascii_case(want)
}

/// Are both installed binaries actually the ones `rel` ships?
fn install_matches_release(dir: &Path, rel: &Release) -> bool {
    BINARIES.iter().all(|name| {
        rel.assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .is_some_and(|asset| file_matches_asset(&dir.join(name), asset))
    })
}

/// Current install + latest-available snapshot. `latest` is best-effort (network).
pub async fn status(app: &AppHandle) -> FrostmodStatus {
    let rel = latest_release().await.ok();
    let installed = is_installed(app);
    let version = installed_version(app);

    // Only worth checking when we already claim to be on the latest tag: any other
    // state is a plain update, and the button offers that anyway. This is what rescues
    // an install that recorded a version it never finished applying — before the
    // all-or-nothing swap existed, a locked `frostmod.dll` could leave exactly that,
    // and "Up to date" then had no way out.
    let needs_repair = match (&rel, &version) {
        (Some(rel), Some(version)) if installed && *version == rel.tag_name => {
            !install_matches_release(&frostmod_dir(app), rel)
        }
        _ => false,
    };

    let cfg = crate::config::load(app).ok();
    let active_game = cfg.as_ref().map(|c| c.active_game).unwrap_or_default();
    let supported_for_game =
        crate::frostmod::supported_for_game(active_game, version.as_deref());

    // The game folder is what makes the VC90 answer mean anything: it's where a copy of the
    // CRT may sit, and where a stray one has to be cleaned out of. `install_dir` hands back
    // an empty string for "don't know", which must not become the path `""`.
    let game_dir = cfg
        .as_ref()
        .map(|c| c.install_dir())
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from);

    // Take back the loose `msvcr90.dll` versions 0.9.2–0.10.0 laid beside the exe. It kills
    // the game with R6034 — see `crate::vcruntime` — and the status poll is the only thing
    // that reaches a player who never opens Settings, so the cleanup rides along here.
    //
    // Its verdict travels on: what the sweep declines to delete is still a file that stops
    // the game dead, and reporting it is the only way the player ever learns why.
    let stray_msvcr90 = game_dir
        .as_deref()
        .map(crate::vcruntime::remove_stray_msvcr90)
        .unwrap_or_default();

    FrostmodStatus {
        installed,
        version,
        latest: rel.map(|r| r.tag_name),
        needs_repair,
        running: crate::frostmod::is_running(),
        supported_for_game,
        missing_runtimes: crate::vcruntime::missing(game_dir.as_deref()),
        stray_msvcr90,
    }
}

/// What an install actually did, beyond succeeding.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    /// The release tag now on disk.
    pub version: String,
    /// The previous FrostMod is still mapped into a running MX Bikes, so the new
    /// one only takes over once the game is restarted.
    pub needs_game_restart: bool,
}

/// Where downloads land before anything live is touched.
fn staging_dir(dir: &Path) -> PathBuf {
    dir.join(".staging")
}

/// Delete leftover `*.in-use-*` copies. Best-effort — one that's still mapped into
/// a live process won't go, and gets another chance next time.
fn sweep_retired(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.contains(RETIRED_MARK))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// First free `<target>.in-use-<n>` name beside `target`.
fn retired_path(target: &Path) -> PathBuf {
    let base = target.as_os_str().to_string_lossy().into_owned();
    (0..)
        .map(|n| PathBuf::from(format!("{base}{RETIRED_MARK}{n}")))
        .find(|p| !p.exists())
        .expect("the range is unbounded")
}

/// Rename, retrying briefly — a virus scanner that grabbed a just-downloaded binary
/// lets go within a moment, and that shouldn't fail an update.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..15 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Err(last.expect("loop runs at least once"))
}

/// Move `staged` onto `target`, leaving the displaced binary aside for the caller
/// to dispose of. Returns where the old one went (`None` if there wasn't one).
///
/// The old file is renamed out of the way rather than overwritten because Windows
/// won't open a mapped image for writing — that's the `os error 32` an in-place
/// overwrite hits while MX Bikes has `frostmod.dll` loaded. Renaming it *is*
/// allowed (the loader opens images with `FILE_SHARE_DELETE`, and the section keeps
/// pointing at the moved file), so the update applies without closing the game.
fn swap_in(target: &Path, staged: &Path) -> std::io::Result<Option<PathBuf>> {
    if !target.exists() {
        std::fs::rename(staged, target)?;
        return Ok(None);
    }
    let retired = retired_path(target);
    rename_with_retry(target, &retired)?;
    if let Err(e) = std::fs::rename(staged, target) {
        // Put the old binary back rather than leave the install without one.
        let _ = std::fs::rename(&retired, target);
        return Err(e);
    }
    Ok(Some(retired))
}

/// Undo a `swap_in`: the new binary goes back to staging, the old one to its name.
fn undo_swap(target: &Path, retired: Option<&Path>, staged: &Path) {
    let _ = std::fs::rename(target, staged);
    if let Some(retired) = retired {
        let _ = std::fs::rename(retired, target);
    }
}

fn locked_file_error(name: &str, e: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Couldn't replace {name} — something still has it open. Close MX Bikes and try again. \
         Nothing was changed. ({e})"
    )
}

/// Move every staged binary into place, rolling back the ones already moved if any
/// of them fails. Returns whether a displaced binary is still in use, which is the
/// signal that the running game is on the old FrostMod until it restarts.
fn apply_staged(dir: &Path, staging: &Path) -> anyhow::Result<bool> {
    let mut done: Vec<(&str, Option<PathBuf>)> = Vec::new();
    for name in BINARIES {
        match swap_in(&dir.join(name), &staging.join(name)) {
            Ok(retired) => done.push((name, retired)),
            Err(e) => {
                for (applied, retired) in &done {
                    undo_swap(
                        &dir.join(applied),
                        retired.as_deref(),
                        &staging.join(applied),
                    );
                }
                return Err(locked_file_error(name, &e));
            }
        }
    }
    // Everything landed, so the displaced copies can go. One that refuses to delete
    // is still backing a loaded image — i.e. the game is running the old FrostMod.
    let mut needs_game_restart = false;
    for (_, retired) in &done {
        if let Some(retired) = retired {
            needs_game_restart |= std::fs::remove_file(retired).is_err();
        }
    }
    Ok(needs_game_restart)
}

/// Download URLs for both binaries, or an error naming what the release is missing.
///
/// A release short one binary used to be installed anyway, which stamped the new tag
/// into `version.txt` over a binary that had never been replaced — the app then
/// reported a version it wasn't running.
fn release_binaries(rel: &Release) -> anyhow::Result<Vec<(&'static str, &Asset)>> {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for want in BINARIES {
        match rel.assets.iter().find(|a| a.name.eq_ignore_ascii_case(want)) {
            Some(asset) => found.push((want, asset)),
            None => missing.push(want),
        }
    }
    if !missing.is_empty() {
        anyhow::bail!(
            "FrostMod release {} is missing {} — nothing was installed.",
            rel.tag_name,
            missing.join(" and ")
        );
    }
    Ok(found)
}

/// Download `frostmod.exe` + `frostmod.dll` from the latest release and put them in
/// place as one unit.
///
/// Everywhere the game runs. FrostMod is a Win32 DLL injected into the game, and the game
/// is a Win32 process on all three platforms — natively on Windows, under Proton on Linux
/// ([`crate::proton`]), in a CrossOver/Whisky bottle on macOS ([`crate::winehost`]) — so
/// the same two binaries go into the same prefix as the game and do the same job.
pub async fn install(app: &AppHandle) -> anyhow::Result<InstallReport> {
    if cfg!(not(any(windows, target_os = "linux", target_os = "macos"))) {
        anyhow::bail!("FrostMod runs on Windows, Linux (Proton) and macOS (Wine)");
    }
    let rel = latest_release().await?;
    let assets = release_binaries(&rel)?;

    let dir = frostmod_dir(app);
    std::fs::create_dir_all(&dir)?;
    sweep_retired(&dir);

    // Download both before touching either live file: a download that dies halfway
    // then costs nothing instead of leaving a mismatched pair behind.
    let staging = staging_dir(&dir);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let client = reqwest::Client::builder().user_agent(UA).build()?;
    for (name, asset) in &assets {
        let bytes = client
            .get(&asset.browser_download_url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let staged = staging.join(name);
        std::fs::write(&staged, &bytes)?;
        // A truncated download is worth catching here, where the previous install is
        // still untouched, rather than after it's been replaced with a broken binary.
        if !file_matches_asset(&staged, asset) {
            let _ = std::fs::remove_dir_all(&staging);
            anyhow::bail!(
                "The download of {name} didn't match what the release advertises — \
                 nothing was changed. Try again."
            );
        }
    }

    let applied = apply_staged(&dir, &staging);
    let _ = std::fs::remove_dir_all(&staging);
    let needs_game_restart = applied?;

    // Written last, and only once both binaries are actually in place, so the version
    // we report can never describe an install that didn't happen.
    std::fs::write(version_path(app), &rel.tag_name)?;
    // Ship our curated server filter. Best-effort.
    ensure_serverfilter(app);
    Ok(InstallReport {
        version: rel.tag_name,
        needs_game_restart,
    })
}

/// What every platform needs settled before FrostMod can be started, and nothing about how
/// it is started — that is where they part company.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
struct StartPlan {
    exe: PathBuf,
    /// `mxb` / `gpb`, for `--game`.
    game: &'static str,
    /// The mods *tree*, when the player has a folder set. Still a host path: anything
    /// running inside a prefix has to rewrite it as the prefix sees it before handing over.
    mods_root: Option<PathBuf>,
}

/// Check what has to be true before starting, and work out what to tell FrostMod.
///
/// `None` means FrostMod is already running and there is nothing to do.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn plan_start(app: &AppHandle) -> anyhow::Result<Option<StartPlan>> {
    if crate::frostmod::is_running() {
        return Ok(None);
    }
    let exe = exe_path(app);
    if !exe.exists() {
        anyhow::bail!("FrostMod isn't installed yet");
    }
    // Refuse to point a build at a title it isn't safe on. v0.10.0 attaches to GP Bikes
    // and then offers an in-game reload that runs MX Bikes' offsets — starting it there
    // hands the player a crash behind an F8 keypress. Updating is the fix, so say so.
    let active_game = crate::config::load(app)
        .map(|c| c.active_game)
        .unwrap_or_default();
    if !crate::frostmod::supported_for_game(active_game, installed_version(app).as_deref()) {
        anyhow::bail!(
            "This FrostMod build isn't safe on {} — update FrostMod to {} or newer.",
            active_game.profile().display,
            crate::frostmod::GPB_MIN_VERSION,
        );
    }
    // Refresh the curated filter before FrostMod loads it.
    ensure_serverfilter(app);
    // Nothing holds the previous binaries once the game that mapped them is gone,
    // so a start is a good moment to clear what the last update had to leave behind.
    sweep_retired(&frostmod_dir(app));
    // Tell FrostMod which game to wait for. Without this it defaults to `mxbikes.exe`, so
    // on GP Bikes it would sit running and never attach — the status pill would say
    // "running" while reload silently did nothing. `--game` landed in FrostMod v0.10.0;
    // older binaries ignore an unknown flag and keep their MX Bikes default, which is the
    // right fallback for the only game they support.
    //
    // `--mods` matters for the same reason and then some: FrostMod's own default was
    // `Documents\PiBoSo\MX Bikes\mods` whatever `--game` said, so on GP Bikes its track
    // manager and model swap operated on the wrong game's folders. We already know the
    // real folder — the user may well have moved it — so send it rather than let FrostMod
    // guess. Harmless on every FrostMod that ever shipped: `--mods` predates `--game`.
    let cfg = crate::config::load(app).unwrap_or_default();
    // The *mods tree*, not the folder above it. FrostMod appends `\tracks` and `\bikes`
    // to whatever `--mods` gives it (its own default is `…\MX Bikes\mods`), so sending
    // `cfg.mods_path` pointed its track manager and model swap at folders that don't
    // exist — silently, since neither reports an empty root as an error.
    let mods_root = (!cfg.mods_path.trim().is_empty())
        .then(|| crate::library::mods_root(&cfg.mods_path));
    Ok(Some(StartPlan { exe, game: cfg.active_game.id(), mods_root }))
}

/// Launch `frostmod.exe` hidden as a managed child.
#[cfg(windows)]
pub fn start(app: &AppHandle, state: &FrostmodProcess) -> anyhow::Result<bool> {
    use std::os::windows::process::CommandExt;
    /// Don't pop a console window for the headless reloader.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Some(plan) = plan_start(app)? else { return Ok(false) };
    let mut args: Vec<String> = vec!["--game".into(), plan.game.into()];
    if let Some(mods) = &plan.mods_root {
        args.extend(["--mods".into(), mods.to_string_lossy().into_owned()]);
    }
    // Logged on both sides, as Linux and macOS already are. FrostMod not working is the
    // single most reported thing about this app, and until now the Windows path — the one
    // nearly every report comes from — said nothing at all in the log, whether it worked
    // or not.
    log::info!("starting FrostMod: {} {:?}", plan.exe.display(), args);
    let child = std::process::Command::new(&plan.exe)
        .current_dir(frostmod_dir(app))
        .args(&args)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Couldn't start {}: {e}", plan.exe.display()))?;
    log::info!("FrostMod started (pid {})", child.id());
    *state.0.lock().unwrap() = Some(child);
    Ok(true)
}

/// Re-arm FrostMod for a game session that has just begun.
///
/// FrostMod injects into one game process. When that process goes, so does the injection —
/// and nothing used to bring it back, because the only automatic start was at app launch.
/// So the second race of a session ran without it: no live reloads, no model swaps, and no
/// indication that anything was different from the first.
///
/// Called from [`crate::sessionwatch`], which already polls for the game starting, and so
/// covers a launch from Steam or the desktop exactly as well as one from the Play button.
pub fn on_game_started(app: &AppHandle, cfg: &crate::config::AppConfig) {
    if !cfg.auto_run_frostmod || !is_installed(app) {
        return;
    }
    let state = app.state::<FrostmodProcess>();
    match start(app, &state) {
        Ok(true) => log::info!("FrostMod re-armed for the new game session"),
        // Already up and holding its reload event, which is the ordinary case when the
        // launcher outlives a game. Whether it got *into* this game is a separate
        // question, and `frostmod::attachment` is what answers it.
        Ok(false) => log::debug!("FrostMod was already running when the game started"),
        Err(e) => log::warn!("couldn't start FrostMod for the new game session: {e:#}"),
    }
}

/// Where the wrapper's own output goes — Proton's on Linux, Wine's on macOS — appended to
/// across a session so a start that worked and a later one that didn't are both in it.
/// Falls back to discarding the output rather than failing a start over a log file.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn runner_log(app: &AppHandle, name: &str) -> std::process::Stdio {
    /// Past this, the interesting part is the end anyway — and a log the player is asked
    /// to attach to a report has to stay attachable.
    const MAX_BYTES: u64 = 1024 * 1024;

    let path = frostmod_dir(app).join(name);
    let overgrown = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES);
    std::fs::OpenOptions::new()
        .create(true)
        .append(!overgrown)
        .write(overgrown)
        .truncate(overgrown)
        .open(&path)
        .map(std::process::Stdio::from)
        .unwrap_or_else(|_| std::process::Stdio::null())
}

/// Launch `frostmod.exe` inside the Proton prefix the game runs in.
///
/// Not "run the Windows binary somehow": FrostMod injects a DLL into `mxbikes.exe`, which
/// only works from inside the same prefix — the same Wine session, sharing the same
/// process namespace. [`crate::proton`] is what finds that prefix and the Proton build
/// that owns it.
#[cfg(target_os = "linux")]
pub fn start(app: &AppHandle, state: &FrostmodProcess) -> anyhow::Result<bool> {
    let Some(plan) = plan_start(app)? else { return Ok(false) };

    // A FrostMod that doesn't poll its command file can't be driven from here at all: it
    // would inject and reload on F8, while every button in this app wrote a file nothing
    // ever read. Better to say so than to hand over a half-working install.
    needs_the_file_channel(app, "Linux", "Under Proton")?;

    let cfg = crate::config::load(app).unwrap_or_default();
    let runner = crate::proton::find(cfg.game(), &cfg.wine_runner)?;

    let mut args: Vec<String> = vec!["--game".into(), plan.game.into()];
    if let Some(mods) = &plan.mods_root {
        // FrostMod is a Windows program: it takes `C:\users\…`, not the `/home/…` this
        // side of the wall calls the same folder.
        args.extend(["--mods".into(), crate::proton::windows_path(&runner.prefix(), mods)]);
    }

    log::info!(
        "starting FrostMod via {}: {} run {} {:?} (prefix {})",
        runner.via(),
        runner.program.display(),
        plan.exe.display(),
        args,
        runner.prefix().display(),
    );
    let child = runner
        .command(&plan.exe, &args)
        .current_dir(frostmod_dir(app))
        // FrostMod is a console program and Proton says a great deal on its way to
        // starting one — none of which would otherwise be anywhere, since the app's own
        // stdout goes nowhere a player can reach. It lands in FrostMod's folder, where
        // the log collector already picks up everything that isn't one of our binaries:
        // if injection under Proton ever fails, this is the file that says why.
        .stdout(runner_log(app, "proton.log"))
        .stderr(runner_log(app, "proton.log"))
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!("Couldn't start FrostMod through {}: {e}", runner.via())
        })?;
    *state.0.lock().unwrap() = Some(child);
    Ok(true)
}

/// The one thing a FrostMod started from outside a Wine prefix has to be able to do: read
/// a command from a file. Refusing here is what stops a player being handed an install
/// where the in-game `F8` works and every button in this app silently doesn't.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn needs_the_file_channel(app: &AppHandle, platform: &str, inside: &str) -> anyhow::Result<()> {
    if crate::frostmod::reads_command_files(installed_version(app).as_deref()) {
        return Ok(());
    }
    anyhow::bail!(
        "This FrostMod build can't be driven from {platform} — update FrostMod to {} or \
         newer. ({inside} the app can only reach FrostMod through a file, which older \
         builds don't read.)",
        crate::frostmod::FILE_CHANNEL_MIN_VERSION,
    )
}

/// Everything about the macOS start that doesn't need a running app: which wrapper, which
/// prefix, and the argv FrostMod is handed. Split out so the whole of it can be driven in a
/// test against a stub standing in for Wine — only whether Wine then runs a Windows binary
/// is out of our hands.
///
/// The wrapper's name comes back with the launch because it is what a failure has to be
/// reported against: "couldn't start FrostMod through CrossOver" names something the player
/// can act on, and the runner itself doesn't outlive this call.
#[cfg(target_os = "macos")]
fn mac_launch(
    cfg: &crate::config::AppConfig,
    exe: &Path,
    game: &str,
    mods_root: Option<&Path>,
) -> anyhow::Result<(crate::winehost::Launch, String)> {
    let (prefix, runner) = crate::gameproc::game_prefix_and_runner(cfg)?;
    // Without a Z: drive nothing inside the bottle can see FrostMod's folder — not the
    // launcher we are about to start, and not the command file every button here writes.
    if !crate::winehost::has_z_drive(&prefix) {
        anyhow::bail!(
            "This bottle has no Z: drive, so FrostMod can't be reached from inside it. Add \
             one mapped to / in your wrapper's drive settings (CrossOver: Bottle → Control \
             Panel → Drives), then try again."
        );
    }

    let mut args: Vec<String> = vec!["--game".into(), game.into()];
    if let Some(mods) = mods_root {
        // FrostMod is a Windows program: it takes the path as the bottle sees it, which for
        // the mods folder — inside the bottle — is `C:\users\…`.
        args.extend(["--mods".into(), crate::winehost::windows_path(&prefix, mods)]);
    }
    Ok((
        crate::winehost::plan(&runner, &prefix, exe, &args),
        runner.via().to_string(),
    ))
}

/// Launch `frostmod.exe` inside the Wine bottle the game runs in (macOS).
///
/// Same requirement as Proton, a different wrapper: the DLL can only be injected from
/// inside the prefix that holds `mxbikes.exe`, so FrostMod is started through whichever of
/// CrossOver, Whisky or Wine owns that bottle — [`crate::winehost`] answers both questions,
/// and [`crate::gameproc::prefix_and_runner`] is where Play asks them too.
///
/// FrostMod itself stays in our data folder rather than being copied into the bottle: it is
/// reached from in there as `Z:\…`, one directory both sides can name, which is also what
/// makes the command file work.
#[cfg(target_os = "macos")]
pub fn start(app: &AppHandle, state: &FrostmodProcess) -> anyhow::Result<bool> {
    let Some(plan) = plan_start(app)? else { return Ok(false) };

    needs_the_file_channel(app, "macOS", "Inside a Wine bottle")?;

    let cfg = crate::config::load(app).unwrap_or_default();
    let (launch, via) = mac_launch(&cfg, &plan.exe, plan.game, plan.mods_root.as_deref())?;
    log::info!(
        "starting FrostMod via {via}: {} {:?}",
        launch.program.display(),
        launch.args,
    );
    let mut cmd = std::process::Command::new(&launch.program);
    cmd.args(&launch.args)
        // FrostMod writes its log, its flag files and its command file beside itself, and
        // resolves them from its own module path — but the working directory is what the
        // wrapper's own output is relative to, and that output is the only account of a
        // failed injection a player can send us.
        .current_dir(frostmod_dir(app))
        .stdout(runner_log(app, "wine.log"))
        .stderr(runner_log(app, "wine.log"));
    for (key, value) in &launch.env {
        cmd.env(key, value);
    }
    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Couldn't start FrostMod through {via}: {e}"))?;
    *state.0.lock().unwrap() = Some(child);
    Ok(true)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn start(_app: &AppHandle, _state: &FrostmodProcess) -> anyhow::Result<bool> {
    anyhow::bail!("FrostMod runs on Windows, Linux (Proton) and macOS (Wine)")
}

/// Kill the managed FrostMod child, if we started one.
pub fn stop(state: &FrostmodProcess) {
    if let Some(mut child) = state.0.lock().unwrap().take() {
        let _ = child.kill();
    }
}

/// Force-terminate any running `frostmod.exe` (even one we didn't spawn). Best-effort.
#[cfg(windows)]
pub fn force_stop_exe() {
    use std::os::windows::process::CommandExt;
    /// Don't flash a console window for the kill.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/IM", "frostmod.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Linux: killing the managed child only reaches the Proton script we spawned, and the
/// launcher it started inside the prefix outlives it — the status pill would keep saying
/// "running" because, correctly, it still is. So the process table is asked for everything
/// running `frostmod.exe`, which is that wrapper *and* the Wine process under it.
#[cfg(target_os = "linux")]
pub fn force_stop_exe() {
    crate::proton::kill_exe("frostmod.exe");
}

/// macOS: the same problem as Linux — the wrapper we spawned and the Wine process it
/// started both carry the name, and killing our child only reaches the first.
#[cfg(target_os = "macos")]
pub fn force_stop_exe() {
    crate::winehost::kill_exe("frostmod.exe");
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn force_stop_exe() {}

/// How long to wait for a stopped FrostMod to actually go before calling it a failure.
const STOP_TIMEOUT: Duration = Duration::from_millis(1000);
const STOP_POLL: Duration = Duration::from_millis(50);

/// Stop FrostMod however it was started, reporting whether it's actually gone.
///
/// `stop` alone only reaches a child *this* app session spawned, so a FrostMod left behind
/// by a previous session — or one the player launched by hand — walked away from it while
/// the status pill kept reading "running". `force_stop_exe` is what reaches those; the two
/// together are the same pair `set_active_game` uses to make a game switch take.
///
/// `taskkill` returns before the process has finished exiting, so the reload event can
/// outlive the call by a moment. Wait for it to go rather than report a kill we never saw
/// land: a "FrostMod stopped" toast over a FrostMod that's still running is worse than no
/// button at all, and the honest failure is actionable (it's elevated, or another user's).
pub fn stop_running(state: &FrostmodProcess) -> bool {
    stop(state);
    force_stop_exe();
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        if !crate::frostmod::is_running() {
            return true;
        }
        if Instant::now() >= deadline {
            log::warn!("FrostMod is still running {STOP_TIMEOUT:?} after being asked to stop");
            return false;
        }
        std::thread::sleep(STOP_POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("frostmod-inst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A managed folder holding the old pair, plus a staging folder holding the new
    /// one. Returns `(dir, staging)`.
    fn installed_pair(tag: &str) -> (PathBuf, PathBuf) {
        let dir = temp_dir(tag);
        let staging = staging_dir(&dir);
        std::fs::create_dir_all(&staging).unwrap();
        for name in BINARIES {
            std::fs::write(dir.join(name), b"old").unwrap();
            std::fs::write(staging.join(name), b"new").unwrap();
        }
        (dir, staging)
    }

    fn read(path: impl AsRef<Path>) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    fn asset(name: &str) -> Asset {
        asset_for(name, b"")
    }

    /// An asset advertising exactly `body` — size and sha256 as GitHub reports them.
    fn asset_for(name: &str, body: &[u8]) -> Asset {
        use sha2::{Digest, Sha256};
        Asset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
            size: body.len() as u64,
            digest: Some(format!("sha256:{:x}", Sha256::digest(body))),
        }
    }

    /// The whole macOS start, end to end, against a stub standing in for Wine.
    ///
    /// Everything up to the wrapper is ours and is exercised here: the prefix comes out of
    /// the game exe's path, the runner override is honoured, FrostMod's own folder is the
    /// working directory, `--mods` arrives as the bottle sees it, and `frostmod.exe` — which
    /// lives *outside* the bottle — is reachable at all.
    #[cfg(target_os = "macos")]
    #[test]
    fn starts_frostmod_in_the_bottle_with_the_mods_path_the_bottle_understands() {
        let root = temp_dir("mac-start");
        let prefix = root.join("Bottles/MXB");
        let game_dir = prefix.join("drive_c/Program Files/MX Bikes");
        let mods = prefix.join("drive_c/users/crossover/Documents/PiBoSo/MX Bikes/mods");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        std::os::unix::fs::symlink("/", prefix.join("dosdevices/z:")).unwrap();
        std::fs::write(game_dir.join(crate::game::MXB.exe), b"stub").unwrap();

        // FrostMod is installed in our data folder, not in the bottle — the case `Z:` exists
        // for, and the reason the command file works at all.
        let frostmod = root.join("data/frostmod");
        std::fs::create_dir_all(&frostmod).unwrap();
        let exe = frostmod.join("frostmod.exe");
        std::fs::write(&exe, b"stub").unwrap();

        // A stub "Wine" that records how it was called, so the assertion is on a real spawn
        // rather than on the plan we handed to it. `printf`, not `echo`: a Windows path is
        // full of backslashes and `echo` would eat them (`\c` alone ends its output).
        let record = root.join("argv.txt");
        let runner = root.join("fake-wine");
        std::fs::write(
            &runner,
            format!(
                "#!/bin/sh\n{{ printf '%s\\n' \"$WINEPREFIX\"; pwd; for a in \"$@\"; do printf '%s\\n' \"$a\"; done; }} > {}\n",
                record.display()
            ),
        )
        .unwrap();
        std::process::Command::new("chmod").arg("+x").arg(&runner).status().unwrap();

        let mut cfg = crate::config::AppConfig::default();
        cfg.game_path = game_dir.to_string_lossy().into_owned();
        cfg.wine_runner = runner.to_string_lossy().into_owned();

        let (launch, _) =
            mac_launch(&cfg, &exe, "mxb", Some(&mods)).expect("a stub runner is enough");
        let mut cmd = std::process::Command::new(&launch.program);
        cmd.args(&launch.args).current_dir(&frostmod);
        for (key, value) in &launch.env {
            cmd.env(key, value);
        }
        cmd.spawn().unwrap().wait().unwrap();

        let written = std::fs::read_to_string(&record).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.first().copied(),
            Some(prefix.to_string_lossy().as_ref()),
            "the prefix is the folder above the game's drive_c: {written:?}"
        );
        // `pwd` resolves symlinks, and macOS puts the temp dir behind `/private`.
        assert!(
            lines.get(1).is_some_and(|cwd| cwd.ends_with("data/frostmod")),
            "FrostMod's own folder is the working directory: {written:?}"
        );
        assert_eq!(
            &lines[2..],
            [
                exe.to_string_lossy().as_ref(),
                "--game",
                "mxb",
                "--mods",
                "C:\\users\\crossover\\Documents\\PiBoSo\\MX Bikes\\mods",
            ],
            "the mods tree arrives as the bottle names it, not as /Users/…: {written:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A bottle with no `Z:` can't see FrostMod's folder, and every button in the app would
    /// write a command file nothing ever reads. Refused, with the fix named.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_bottle_with_no_z_drive_is_refused_before_anything_starts() {
        let root = temp_dir("mac-no-z");
        let game_dir = root.join("Bottles/MXB/drive_c/MX Bikes");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join(crate::game::MXB.exe), b"stub").unwrap();
        let runner = root.join("fake-wine");
        std::fs::write(&runner, "#!/bin/sh\n").unwrap();
        std::process::Command::new("chmod").arg("+x").arg(&runner).status().unwrap();

        let mut cfg = crate::config::AppConfig::default();
        cfg.game_path = game_dir.to_string_lossy().into_owned();
        cfg.wine_runner = runner.to_string_lossy().into_owned();

        let err = mac_launch(&cfg, &root.join("frostmod.exe"), "mxb", None)
            .expect_err("no Z: drive, no way in");
        let msg = format!("{err:#}");
        assert!(msg.contains("Z:"), "names what's missing: {msg}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_release_missing_a_binary_installs_nothing() {
        // Skipping the missing one and installing the rest is what used to stamp a new
        // tag into version.txt over a binary that had never been replaced.
        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets: vec![asset("frostmod.exe"), asset("Release.zip")],
        };
        let err = format!("{:#}", release_binaries(&rel).expect_err("half a pair is no pair"));
        assert!(err.contains("frostmod.dll"), "names what's missing: {err}");
        assert!(err.contains("v0.9.9"), "names the release: {err}");
    }

    #[test]
    fn a_complete_release_yields_both_download_urls() {
        // Real releases carry more than the two binaries, and have shipped mixed case.
        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets: vec![
                asset("FrostServer.zip"),
                asset("FrostMod.DLL"),
                asset("frostmod.exe"),
            ],
        };
        let found = release_binaries(&rel).expect("both binaries are there");
        let names: Vec<&str> = found.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["frostmod.exe", "frostmod.dll"]);
        assert!(
            found[1].1.browser_download_url.ends_with("FrostMod.DLL"),
            "keeps the asset's own url"
        );
    }

    #[test]
    fn applying_swaps_both_binaries_and_leaves_nothing_behind() {
        let (dir, staging) = installed_pair("apply");

        let needs_restart = apply_staged(&dir, &staging).expect("nothing holds these files");

        for name in BINARIES {
            assert_eq!(read(dir.join(name)), "new", "{name} was replaced");
        }
        // Nothing had the old pair open, so it's gone rather than parked aside.
        assert!(!needs_restart);
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        assert!(
            !std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains(RETIRED_MARK)),
            "no retired copies left over"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bug this whole path exists for: a failure on the second binary used to
    /// leave a new exe beside an old dll.
    #[test]
    fn a_binary_that_cant_land_puts_the_earlier_one_back() {
        let (dir, staging) = installed_pair("rollback");
        // Nothing to move into place for the dll — stands in for the locked target
        // that `swap_in` can't complete.
        std::fs::remove_file(staging.join("frostmod.dll")).unwrap();

        let err = format!(
            "{:#}",
            apply_staged(&dir, &staging).expect_err("a missing staged binary can't land")
        );
        assert!(err.contains("frostmod.dll"), "names the binary: {err}");
        assert!(err.contains("MX Bikes"), "says how to fix it: {err}");

        for name in BINARIES {
            assert_eq!(read(dir.join(name)), "old", "{name} is back as it was");
        }
        assert_eq!(
            read(staging.join("frostmod.exe")),
            "new",
            "the new exe went back to staging"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_displaced_binary_that_wont_delete_asks_for_a_game_restart() {
        let dir = temp_dir("retired");
        let target = dir.join("frostmod.dll");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(dir.join("staged.dll"), b"new").unwrap();

        let retired = swap_in(&target, &dir.join("staged.dll"))
            .expect("a rename works even on a mapped image")
            .expect("the old binary was moved aside, not overwritten");

        assert_eq!(read(&target), "new");
        // Windows can't delete this while it backs a loaded image; that failure is
        // exactly what tells us the game is still on the old FrostMod.
        assert_eq!(read(&retired), "old");
        assert!(retired.to_string_lossy().contains(RETIRED_MARK));

        // A second swap doesn't fight the first for the name.
        std::fs::write(dir.join("staged.dll"), b"newer").unwrap();
        let second = swap_in(&target, &dir.join("staged.dll")).unwrap().unwrap();
        assert_ne!(second, retired, "each displaced copy gets its own slot");

        sweep_retired(&dir);
        assert!(!retired.exists() && !second.exists(), "the sweep clears them");
        assert_eq!(read(target), "newer", "and leaves the live binary alone");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The state the old installer could strand people in: `version.txt` says v0.9.9
    /// because the exe landed, but the dll is still the v0.9.8 one the running game had
    /// locked. Settings called that "Up to date" and disabled the button, so there was
    /// no way out — spotting the mismatch is what turns it back into a repair.
    #[test]
    fn a_binary_that_isnt_what_the_release_ships_is_a_mismatch() {
        let dir = temp_dir("verify");
        std::fs::write(dir.join("frostmod.exe"), b"the 0.9.9 exe").unwrap();
        std::fs::write(dir.join("frostmod.dll"), b"the 0.9.8 dll").unwrap();

        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets: vec![
                asset_for("frostmod.exe", b"the 0.9.9 exe"),
                asset_for("frostmod.dll", b"the 0.9.9 dll"),
            ],
        };
        assert!(!install_matches_release(&dir, &rel), "the stale dll is caught");

        // Same length, different bytes — size alone would wave this through.
        std::fs::write(dir.join("frostmod.dll"), b"the 0.9.9 dll").unwrap();
        assert!(install_matches_release(&dir, &rel), "a matching pair verifies");

        // A binary that never landed at all is a mismatch, not a crash.
        std::fs::remove_file(dir.join("frostmod.dll")).unwrap();
        assert!(!install_matches_release(&dir, &rel));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_release_without_digests_still_gets_a_size_check() {
        let dir = temp_dir("verify-nodigest");
        std::fs::write(dir.join("frostmod.exe"), b"exe").unwrap();
        std::fs::write(dir.join("frostmod.dll"), b"dll-but-longer").unwrap();

        let mut assets = vec![
            asset_for("frostmod.exe", b"exe"),
            asset_for("frostmod.dll", b"dll"),
        ];
        for a in &mut assets {
            a.digest = None;
        }
        let rel = Release {
            tag_name: "v0.9.9".into(),
            assets,
        };
        assert!(
            !install_matches_release(&dir, &rel),
            "the wrong-length dll is caught without a digest"
        );

        std::fs::write(dir.join("frostmod.dll"), b"dll").unwrap();
        assert!(install_matches_release(&dir, &rel));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sweep_only_touches_retired_copies() {
        let dir = temp_dir("sweep");
        std::fs::write(dir.join("frostmod.exe"), b"live").unwrap();
        std::fs::write(dir.join("version.txt"), b"v0.9.9").unwrap();
        std::fs::write(dir.join(format!("frostmod.dll{RETIRED_MARK}0")), b"old").unwrap();

        sweep_retired(&dir);

        assert!(dir.join("frostmod.exe").exists());
        assert!(dir.join("version.txt").exists());
        assert!(!dir.join(format!("frostmod.dll{RETIRED_MARK}0")).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn curated_filter_unhides_kaizo_but_keeps_sentinel() {
        // FrostMod only respects a config whose first line is the v4 sentinel.
        assert!(CURATED_SERVERFILTER.starts_with("# frostmod-filter v4"));
        // Kaizo must no longer be matched, by name or the spam regex.
        let lc = CURATED_SERVERFILTER.to_lowercase();
        assert!(!lc.contains("kaizo"));
        assert!(!lc.contains("kalz0"));
        assert!(!CURATED_SERVERFILTER.contains("k[a4][il1]z[o0]"));
        // Spam rules we keep.
        assert!(CURATED_SERVERFILTER.contains("che4ts"));
        assert!(CURATED_SERVERFILTER.contains(r"\.pr0\b"));
    }

    #[test]
    fn stock_default_is_the_kaizo_blocking_one() {
        // Guards our overwrite trigger: the stock text must actually block Kaizo.
        assert!(STOCK_SERVERFILTER.contains("- kaizo"));
        assert!(STOCK_SERVERFILTER.contains("k[a4][il1]z[o0]"));
    }

    #[test]
    fn filter_eq_ignores_line_endings_and_trailing_space() {
        let crlf = STOCK_SERVERFILTER.replace('\n', "\r\n");
        assert!(filter_eq(&crlf, STOCK_SERVERFILTER));
        assert!(filter_eq(&format!("{STOCK_SERVERFILTER}\n\n"), STOCK_SERVERFILTER));
        // A real edit (curated vs stock) must NOT compare equal.
        assert!(!filter_eq(CURATED_SERVERFILTER, STOCK_SERVERFILTER));
    }
}

// ---------------------------------------------------------------------------
// Replay camera key bindings
//
// FrostMod's replay camera editor polls the keyboard, so the game sees the same key on
// the same frame: a player with `S` bound to move-backwards could not save a path without
// moving the camera. FrostMod made the keys configurable; this is the half that edits
// them, because a config file next to a DLL is not somewhere anyone should have to go.
//
// They live in FrostMod's existing `frostmod_radar.cfg` as `rcam_save=F9` lines, which
// also carries radar/overlay settings — so every write here is a merge, never a rewrite.
// ---------------------------------------------------------------------------

/// One editor action, and the key it is on. `key` is FrostMod's own text form
/// (`F9`, `Ctrl+Numpad1`, `none`) — kept as a string so the two sides agree on one
/// spelling rather than each inventing an encoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keybind {
    pub id: String,
    pub key: String,
}

/// Mirrors `kb::Actions()` in FrostMod's `src/keybinds.h`, in the same order. The ids are
/// the config keys (`rcam_<id>`); the UI labels them itself, translated.
const RCAM_ACTIONS: [(&str, &str); 9] = [
    ("setkey", "K"),
    ("delete", "X"),
    ("clear", "C"),
    ("play", "P"),
    ("prev", "Comma"),
    ("next", "Period"),
    ("save", "S"),
    ("load", "L"),
    // Hiding the overlay defaults to a function key, not a letter: it is pressed while
    // recording, so a key the game might have bound would be the wrong default.
    ("clean", "F7"),
];

/// Key names FrostMod can parse, beyond the plain letters and digits. Mirrors the table in
/// `keybinds.h`. We validate against it rather than trusting the front end: a name FrostMod
/// cannot read back leaves someone with a key that does nothing and no error to explain it.
const NAMED_KEYS: [&str; 57] = [
    "Space", "Tab", "Enter", "Backspace", "Insert", "Delete", "Home", "End",
    "PageUp", "PageDown", "Left", "Up", "Right", "Down",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "F13", "F14", "F15", "F16",
    "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6",
    "Numpad7", "Numpad8", "Numpad9",
    "NumpadMultiply", "NumpadAdd", "NumpadSubtract", "NumpadDecimal", "NumpadDivide",
    "Comma", "Period", "Semicolon", "Slash", "Backtick", "Minus", "Equals",
    "LeftBracket", "Backslash", "RightBracket", "Quote", "Escape",
];

const RCAM_CFG: &str = "frostmod_radar.cfg";

/// Every config FrostMod might be reading. There are two installs to think about: the one
/// this app manages, and a `.dlo` the player dropped in the game's own plugins folder —
/// which keeps its config there, next to the DLL. Editing only ours would look like the
/// setting did nothing for anyone running in plugin mode.
fn rcam_cfg_paths(app: &AppHandle, game_path: &str) -> Vec<PathBuf> {
    let mut out = vec![frostmod_dir(app).join(RCAM_CFG)];
    if !game_path.is_empty() {
        let plugins = Path::new(game_path).join("plugins");
        if plugins.join("frostmod.dlo").exists() || plugins.join(RCAM_CFG).exists() {
            out.push(plugins.join(RCAM_CFG));
        }
    }
    out
}

/// Is this something FrostMod's `ParseBind` will accept? Same rules: optional `Ctrl+`,
/// `Alt+`, `Shift+` prefixes, then a single letter or digit, or one of the named keys.
fn valid_bind(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    if text.eq_ignore_ascii_case("none") {
        return true;
    }
    let mut rest = text;
    loop {
        let Some(plus) = rest.find('+') else { break };
        // a trailing '+' is the key itself, which we do not accept as a name
        if plus == 0 || plus + 1 >= rest.len() {
            return false;
        }
        let (modifier, tail) = rest.split_at(plus);
        if !["ctrl", "control", "alt", "shift"]
            .iter()
            .any(|m| modifier.eq_ignore_ascii_case(m))
        {
            return false;
        }
        rest = tail[1..].trim_start();
    }
    if rest.len() == 1 {
        let c = rest.as_bytes()[0];
        if c.is_ascii_alphanumeric() {
            return true;
        }
    }
    NAMED_KEYS.iter().any(|k| k.eq_ignore_ascii_case(rest))
}

/// The bindings in force: FrostMod's defaults, with whatever the config overrides. Reading
/// the app's own copy first and letting a plugin-folder config win matches what the game
/// would actually load if both exist.
pub fn rcam_keybinds(app: &AppHandle, game_path: &str) -> Vec<Keybind> {
    let mut binds: Vec<Keybind> = RCAM_ACTIONS
        .iter()
        .map(|(id, def)| Keybind { id: (*id).to_string(), key: (*def).to_string() })
        .collect();

    for path in rcam_cfg_paths(app, game_path) {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            let Some((name, value)) = line.split_once('=') else { continue };
            let Some(id) = name.trim().strip_prefix("rcam_") else { continue };
            let value = value.trim();
            if !valid_bind(value) {
                continue;
            }
            if let Some(b) = binds.iter_mut().find(|b| b.id == id) {
                b.key = value.to_string();
            }
        }
    }
    binds
}

/// Write the bindings into every config FrostMod might read, keeping every other line.
/// FrostMod re-reads the file each time the editor opens, so this lands without a restart.
pub fn set_rcam_keybinds(
    app: &AppHandle,
    game_path: &str,
    binds: &[Keybind],
) -> Result<(), String> {
    for b in binds {
        if !RCAM_ACTIONS.iter().any(|(id, _)| *id == b.id) {
            return Err(format!("unknown replay camera action '{}'", b.id));
        }
        if !valid_bind(&b.key) {
            return Err(format!("'{}' is not a key FrostMod can read", b.key));
        }
    }

    let mut wrote = 0usize;
    let mut last_err = None;
    for path in rcam_cfg_paths(app, game_path) {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut out: Vec<String> = Vec::new();
        for line in existing.lines() {
            let is_ours = line
                .split_once('=')
                .map(|(name, _)| name.trim().starts_with("rcam_"))
                .unwrap_or(false);
            if !is_ours {
                out.push(line.to_string());
            }
        }
        for b in binds {
            out.push(format!("rcam_{}={}", b.id, b.key.trim()));
        }
        let body = out.join("\n") + "\n";

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, body) {
            Ok(()) => {
                wrote += 1;
                log::info!("wrote replay camera key bindings to {}", path.display());
            }
            Err(e) => {
                log::warn!("could not write {}: {e}", path.display());
                last_err = Some(e.to_string());
            }
        }
    }

    if wrote == 0 {
        return Err(last_err.unwrap_or_else(|| "no FrostMod config folder to write to".into()));
    }
    Ok(())
}

/// FrostMod's shipped defaults, for the reset button.
pub fn default_rcam_keybinds() -> Vec<Keybind> {
    RCAM_ACTIONS
        .iter()
        .map(|(id, def)| Keybind { id: (*id).to_string(), key: (*def).to_string() })
        .collect()
}

#[cfg(test)]
mod rcam_tests {
    use super::*;

    #[test]
    fn accepts_what_frostmod_can_parse() {
        for good in ["K", "k", "9", "F9", "Numpad1", "Comma", "none", "NONE",
                     "Ctrl+S", "ctrl+shift+Numpad0", "Alt+F4"] {
            assert!(valid_bind(good), "{good} should be accepted");
        }
    }

    #[test]
    fn refuses_what_it_cannot() {
        for bad in ["", "  ", "Meta+K", "Ctrl+", "NotAKey", "F99", "KK", "+"] {
            assert!(!valid_bind(bad), "{bad} should be refused");
        }
    }

    #[test]
    fn defaults_match_frostmods() {
        // If FrostMod's defaults move, this is the reminder to move ours with them.
        let d = default_rcam_keybinds();
        assert_eq!(d.len(), 9);
        assert_eq!(d[0].id, "setkey");
        assert_eq!(d[0].key, "K");
        assert_eq!(d[6].key, "S");
        assert_eq!(d[8].id, "clean");
        assert_eq!(d[8].key, "F7");
        for b in &d {
            assert!(valid_bind(&b.key), "default {} is not valid", b.key);
        }
    }
}
