use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// FrostMod's GitHub repo — supported releases carry `frostmod.dll`.
const REPO: &str = "Frostn1/frostmod";
pub const UA: &str = "mxb-app";

/// The plugin is a byte-identical copy of this DLL. The legacy executable is intentionally
/// never downloaded, launched, or used to determine installation state.
const BINARIES: [&str; 1] = ["frostmod.dll"];

/// The binaries the release tagged `tag` is installed as.
fn binaries_for(tag: &str) -> &'static [&'static str] {
    let _ = tag;
    &BINARIES
}

/// Marks a binary moved aside because something still had it open. Swept on the
/// next install or start, by which point whatever held it has usually exited.
const RETIRED_MARK: &str = ".in-use-";

/// Kept as Tauri state for compatibility with the old UI command surface. Plugin-mode
/// FrostMod does not create a managed child process.
#[derive(Default)]
pub struct FrostmodProcess(pub std::sync::Mutex<Option<std::process::Child>>);

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrostmodStatus {
    /// Whether a supported FrostMod DLL is present in our managed folder.
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
    /// What became of a hand-installed `frostmod.dlo` in the game's own `plugins` folder.
    /// [`PluginCopy::Absent`] is the normal case — see [`refresh_game_plugin`].
    pub game_plugin: PluginCopy,
    /// What became of *our* `frostmod_session.dlo` in that same folder. Unlike the one
    /// above this is a copy the app installs, and [`PluginCopy::Current`] is the state we
    /// want everyone in — see [`ensure_session_plugin`].
    ///
    /// Plugin-only FrostMod turns that round: the full plugin publishes the server itself,
    /// so the session copy is removed and [`PluginCopy::Absent`] is the state we want.
    pub session_plugin: PluginCopy,
    /// The installed FrostMod runs as a game plugin alone — see
    /// [`crate::frostmod::PLUGIN_ONLY_MIN_VERSION`]. `game_plugin` is then the copy *we*
    /// install, and `frostmod.exe` is never started: there is nothing to start or stop, and
    /// FrostMod is running whenever the game is.
    pub plugin_only: bool,
}

/// The state of a FrostMod plugin copy sitting in the game's `plugins` folder.
///
/// `frostmod.exe --install-plugin` drops `frostmod.dlo` there so the game loads FrostMod at
/// startup with no injector. Nothing has ever updated that copy afterwards — not the app,
/// which manages only `frostmod.exe` and `frostmod.dll` in its own folder, and not FrostMod,
/// which prints "re-run --install-plugin to refresh the installed .dlo" and leaves it at that.
/// So it outlives every update, silently, and the game goes on loading it: one player was
/// running a **v0.12** plugin against a v0.16.2 install, and it hung the game at a black
/// screen before the loading screen — with `frostmod.exe` not even running, because a plugin
/// doesn't need it.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginCopy {
    /// No plugin installed in the game folder. The normal case, and nothing to do.
    #[default]
    Absent,
    /// Present and already the build we manage.
    Current,
    /// Was stale and has been brought up to date.
    Refreshed,
    /// Was stale, couldn't be updated, and has been renamed so the game stops loading it.
    /// Nothing is destroyed — see [`disable_game_plugin`].
    Disabled,
    /// Stale, couldn't be updated, and couldn't be moved aside either. The game is still
    /// loading it; another poll will try again.
    Locked,
    /// A plugin is installed but the app doesn't manage a FrostMod to judge it against, so
    /// there is nothing to compare and nothing to copy from. Left strictly alone: it may be
    /// perfectly current, and it isn't ours to touch on a machine we aren't managing.
    Unmanaged,
}

/// The folder we install FrostMod into and where its plugin reads its managed files.
pub fn frostmod_dir(app: &AppHandle) -> PathBuf {
    // Local app-data dir (Windows: `%LOCALAPPDATA%\com.frost.mxbikes\frostmod`).
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("frostmod")
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

/// Is FrostMod on disk in a form we can run?
///
pub fn is_installed(app: &AppHandle) -> bool {
    plugin_only(app) && frostmod_dir(app).join("frostmod.dll").exists()
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

/// Are the installed binaries actually the ones `rel` ships? Only the ones that release is
/// installed as — see [`binaries_for`].
fn install_matches_release(dir: &Path, rel: &Release) -> bool {
    binaries_for(&rel.tag_name).iter().all(|name| {
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

    let plugin_only = crate::frostmod::plugin_only(version.as_deref());
    let (game_plugin, session_plugin) = if plugin_only {
        // The plugin is now the whole of FrostMod, and the one thing keeping it installed
        // and current for the player who never opens Settings — same reason as the rest
        // of this poll. It also drops the session copy the full plugin has replaced.
        let sync = sync_plugin(app, &cfg.clone().unwrap_or_default());
        (sync.game_plugin, sync.session_plugin)
    } else {
        // Fail closed: this app no longer supports injector-era FrostMod. Do not copy,
        // refresh, launch, or otherwise activate a legacy install.
        crate::frostmod::set_plugin_mode(None);
        (PluginCopy::Absent, PluginCopy::Absent)
    };

    FrostmodStatus {
        installed,
        version,
        latest: rel.map(|r| r.tag_name),
        needs_repair,
        running: crate::frostmod::is_running(),
        supported_for_game,
        missing_runtimes: crate::vcruntime::missing(game_dir.as_deref()),
        stray_msvcr90,
        game_plugin,
        session_plugin,
        plugin_only,
    }
}

/// `<game>\plugins\frostmod.dlo` — where `--install-plugin` puts the plugin copy.
fn game_plugin_path(game_dir: &Path) -> PathBuf {
    game_dir.join("plugins").join("frostmod.dlo")
}

/// Bring a hand-installed `frostmod.dlo` up to the build we manage, if one is there.
///
/// **Only ever refreshes a file that already exists — it never installs one.** The app drives
/// FrostMod by injection and has no use for plugin mode; the whole job here is to stop a copy
/// somebody else installed from rotting in the game folder. Creating one would change how
/// FrostMod loads on machines that never asked for it, and would double-load it besides.
///
/// The `.dlo` is a byte-identical copy of the `.dll` (FrostMod's own CMake makes it one), so
/// this is a plain copy — there is no separate asset to fetch.
///
/// Staleness is judged on size and mtime rather than by reading a quarter-megabyte twice on
/// every status poll: a copy we made is the same size and newer, and that pair converges.
fn refresh_game_plugin(dir: &Path, game_dir: &Path) -> PluginCopy {
    let dlo = game_plugin_path(game_dir);
    let Ok(dlo_meta) = std::fs::metadata(&dlo) else {
        return PluginCopy::Absent; // no plugin installed — the normal case
    };
    let Ok(dll_meta) = std::fs::metadata(dir.join("frostmod.dll")) else {
        // A plugin we have no way to judge: nothing to compare against and nothing to copy
        // from. Leaving it is the only honest move — it may well be current, and a machine
        // whose FrostMod we don't manage isn't one to start renaming files on.
        return PluginCopy::Unmanaged;
    };
    let fresh = dlo_meta.len() == dll_meta.len()
        && match (dlo_meta.modified(), dll_meta.modified()) {
            (Ok(a), Ok(b)) => a >= b,
            // No mtimes to compare: same size is all we have, and re-copying every poll
            // would be worse than trusting it.
            _ => true,
        };
    if fresh {
        return PluginCopy::Current;
    }

    // Same rename-then-replace the binaries use: the game maps this file while it runs, and
    // Windows won't open a mapped image for writing, but it will let it be renamed away.
    let staged = dlo.with_extension("dlo.staging");
    if std::fs::copy(dir.join("frostmod.dll"), &staged).is_err() {
        return disable_game_plugin(&dlo);
    }
    match swap_in(&dlo, &staged) {
        Ok(retired) => {
            if let Some(retired) = retired {
                let _ = std::fs::remove_file(retired);
            }
            log::info!(
                "[frostmod] refreshed the stale plugin copy at {} — it was older than the \
                 FrostMod we manage, and the game loads it at startup",
                dlo.display()
            );
            PluginCopy::Refreshed
        }
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            log::warn!("[frostmod] couldn't refresh {}: {e}", dlo.display());
            disable_game_plugin(&dlo)
        }
    }
}

/// `<game>\plugins\frostmod_session.dlo` — the copy we install, and the name that puts
/// FrostMod in session-only mode. Must match `frostmod::session::kSessionPluginFileName`.
fn session_plugin_path(game_dir: &Path) -> PathBuf {
    game_dir.join("plugins").join("frostmod_session.dlo")
}

/// Put the session plugin in the game's plugins folder, and keep it current.
///
/// **Why the app installs a plugin at all.** The server name only ever arrives through
/// `EventInit`, and the game only calls that on a plugin it loaded itself from
/// `plugins\*.dlo`. FrostMod injected as a `.dll` is never asked. So for every player the
/// app drives, the block's `serverName` stayed empty forever — and with it, paint sync had
/// no roster to scope itself to and voice chat had no room to join. In the seven days to
/// 2026-09-04, 172 riders had the injected `frostmod.dll` in their game and 2 had a
/// plugin; presence was reported for a real server exactly once.
///
/// This is a byte-identical copy of the `frostmod.dll` we already manage — the same trick
/// `frostmod.dlo` uses — under a name FrostMod recognises as "publish the session and do
/// nothing else": no hooks, no overlay, no offsets, its own shared block. That is the whole
/// reason it is safe to load beside the injected copy.
///
/// It is only ever installed from a build that knows the name. An older one under it would
/// run as a full plugin next to the injected copy, and two FrostMods hooking the same
/// functions is what hangs the game at a black screen — see
/// [`crate::frostmod::SESSION_PLUGIN_MIN_VERSION`]. A copy already there from a build that
/// has since been rolled back is parked rather than left loading.
fn ensure_session_plugin(dir: &Path, game_dir: &Path, tag: Option<&str>) -> PluginCopy {
    let dlo = session_plugin_path(game_dir);
    let installed = std::fs::metadata(&dlo);

    if !crate::frostmod::session_plugin_is_safe(tag) {
        // Not a build we may install from. If one of ours is already there it came from a
        // build that was, and this one would load it as a full plugin: park it.
        return match installed {
            Ok(_) => disable_game_plugin(&dlo),
            Err(_) => PluginCopy::Absent,
        };
    }

    let Ok(dll_meta) = std::fs::metadata(dir.join("frostmod.dll")) else {
        // Nothing to copy from. An existing copy is left exactly where it is — it may be
        // perfectly current, and we can't tell.
        return if installed.is_ok() { PluginCopy::Unmanaged } else { PluginCopy::Absent };
    };

    // Same staleness test as the hand-installed copy: a copy we made is the same size and
    // no older, and that pair converges without reading a quarter-megabyte twice a poll.
    if let Ok(dlo_meta) = &installed {
        let fresh = dlo_meta.len() == dll_meta.len()
            && match (dlo_meta.modified(), dll_meta.modified()) {
                (Ok(a), Ok(b)) => a >= b,
                _ => true,
            };
        if fresh {
            return PluginCopy::Current;
        }
    }

    let Some(plugins) = dlo.parent() else {
        return PluginCopy::Absent;
    };
    if std::fs::create_dir_all(plugins).is_err() {
        return PluginCopy::Locked;
    }

    // Rename-then-replace, as the binaries do: the game maps this file while it runs, and
    // Windows won't open a mapped image for writing — but it will let it be renamed away.
    let staged = dlo.with_extension("dlo.staging");
    if std::fs::copy(dir.join("frostmod.dll"), &staged).is_err() {
        let _ = std::fs::remove_file(&staged);
        return if installed.is_ok() { PluginCopy::Locked } else { PluginCopy::Absent };
    }
    match swap_in(&dlo, &staged) {
        Ok(retired) => {
            if let Some(retired) = retired {
                let _ = std::fs::remove_file(retired);
            }
            log::info!(
                "[frostmod] session plugin in place at {} — the game hands it the server \
                 name, which nothing injected is ever told",
                dlo.display()
            );
            if installed.is_ok() { PluginCopy::Refreshed } else { PluginCopy::Current }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            log::warn!("[frostmod] couldn't install the session plugin at {}: {e}", dlo.display());
            if installed.is_ok() { PluginCopy::Locked } else { PluginCopy::Absent }
        }
    }
}

/// Move a stale plugin out of the way, when it can't be brought up to date.
///
/// A stale plugin is not a neutral thing to leave lying there: the game loads it at startup
/// on its own, and a stale enough one hangs the game before the loading screen — which is a
/// rider who cannot play at all, with nothing on screen to say why. So if we can't fix it,
/// we stop it loading.
///
/// Renamed, never deleted. Two reasons: the app didn't install this file, so destroying it
/// isn't ours to do; and a rename works even while MX Bikes has the plugin mapped (the
/// loader opens images with `FILE_SHARE_DELETE`), so the fix lands on the *next* launch
/// without waiting for the player to close the game. The new name deliberately does not end
/// in `.dlo` — anything that does, in this folder, gets loaded as a plugin.
fn disable_game_plugin(dlo: &Path) -> PluginCopy {
    // A rider who has been through this twice shouldn't have the first parked copy silently
    // replaced by the second — on Windows the rename would simply fail, and on Unix it would
    // overwrite. Number them instead.
    let first = dlo.with_extension("dlo.disabled");
    let parked = if first.exists() {
        (1..)
            .map(|n| dlo.with_extension(format!("dlo.disabled-{n}")))
            .find(|p| !p.exists())
            .expect("the range is unbounded")
    } else {
        first
    };
    match rename_with_retry(dlo, &parked) {
        Ok(()) => {
            log::warn!(
                "[frostmod] the plugin at {} is older than the FrostMod we manage and couldn't \
                 be updated, so it has been renamed to {} — the game loads a plugin at startup \
                 whether or not frostmod.exe is running, and a stale one can stop it opening. \
                 Rename it back to re-enable plugin mode.",
                dlo.display(),
                parked.display()
            );
            PluginCopy::Disabled
        }
        Err(e) => {
            log::warn!("[frostmod] couldn't move {} aside: {e}", dlo.display());
            PluginCopy::Locked
        }
    }
}

// ===========================================================================
// Plugin-only FrostMod (v0.41.0 and newer).
//
// The injector is `frostmod.exe`, and an injector is what Windows Defender takes FrostMod
// for. From v0.41.0 FrostMod runs as a PiBoSo game plugin alone: the game loads
// `plugins\frostmod.dlo` itself at startup, before it scans the mods folder, and hands it
// the session events the injected copy never got. So for those builds the app installs the
// plugin, points it at our FrostMod folder, and never starts `frostmod.exe` at all — see
// `crate::frostmod::PLUGIN_ONLY_MIN_VERSION`. Older builds keep the injector, unchanged.
//
// What `frostmod.exe` used to do on its way up is now ours: it wrote `frostmod_mods.txt`
// and the `.flag` files the dll reads at init. Those land in the same folder as before,
// and `frostmod.dir` is what sends the plugin there to read them.
// ===========================================================================

/// `<game>\plugins\frostmod.dir` — one line, UTF-8, naming the folder FrostMod keeps its
/// files in. Beside the `.dlo` because that is where FrostMod looks for it.
fn dir_pointer_path(game_dir: &Path) -> PathBuf {
    game_dir.join("plugins").join("frostmod.dir")
}

/// Past this FrostMod may not have room to put a file name on the folder, and falls back to
/// its own. FrostMod's real limit is a little over 210 characters of ANSI; this warns early.
const DIR_POINTER_WARN_LEN: usize = 200;

/// Why FrostMod might ignore `pointer`, if it might — for the log.
///
/// It is not refused here: FrostMod checks for itself and falls back to its own folder,
/// which still works, just with its log and flags in the game's `plugins` folder where
/// "Send logs" never looks. That is worth a line in the log when a report comes in.
fn dir_pointer_warning(pointer: &str) -> Option<String> {
    let len = pointer.chars().count();
    if len > DIR_POINTER_WARN_LEN {
        return Some(format!(
            "FrostMod's folder path is {len} characters long; FrostMod gives up on a folder \
             much past {DIR_POINTER_WARN_LEN} and keeps its files beside the plugin instead"
        ));
    }
    if !pointer.is_ascii() {
        return Some(
            "FrostMod's folder path has non-ASCII characters in it; FrostMod only uses it if \
             Windows' code page can spell them, and keeps its files beside the plugin otherwise"
                .into(),
        );
    }
    None
}

/// Write `frostmod.dir` if it doesn't already say `pointer`. Returns whether it wrote.
///
/// No BOM and no line ending: FrostMod strips both, but the fewer things it has to strip the
/// fewer ways there are to get it wrong.
fn write_dir_pointer(game_dir: &Path, pointer: &str) -> std::io::Result<bool> {
    let path = dir_pointer_path(game_dir);
    if std::fs::read_to_string(&path).is_ok_and(|cur| cur == pointer) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, pointer.as_bytes())?;
    Ok(true)
}

/// Delete `path`, or at least stop it being loaded.
///
/// A game that is running has its plugins mapped, and Windows refuses to delete a mapped
/// image — but it allows the rename (the loader opens images with `FILE_SHARE_DELETE`). The
/// new name carries [`RETIRED_MARK`], which doesn't end in `.dlo`, so the next launch won't
/// load it, and [`sweep_retired`] clears it once the game has let go. Returns whether the
/// original name is free now.
fn remove_or_retire(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => {
            let aside = retired_path(path);
            // Plain rename, no retry: this runs on the status poll and on game exit, and a
            // file that won't move now gets another go on the next one.
            if std::fs::rename(path, &aside).is_ok() {
                let _ = std::fs::remove_file(&aside);
                true
            } else {
                false
            }
        }
    }
}

/// Put `frostmod.dll` in the game's `plugins` folder as `frostmod.dlo`, and keep it current.
///
/// Compared byte for byte rather than on size and mtime, as the injector-era copies are:
/// this one *is* FrostMod now, so it has to be exactly the build we report, and it is only
/// read on the status poll, an install and a game exit — not often enough to matter.
///
/// **Never replaced while the game runs.** The game holds its plugins open for the whole
/// session; the rename trick would get a new file in, but the running game keeps the old
/// one loaded and FrostMod would be two versions at once as far as anyone could tell. So a
/// stale copy in a running game is [`PluginCopy::Locked`] and the next pass after the game
/// exits replaces it — before the next launch, which is the only moment that loads it.
fn install_game_plugin(dir: &Path, game_dir: &Path, game_running: bool) -> PluginCopy {
    let dlo = game_plugin_path(game_dir);
    let Ok(want) = std::fs::read(dir.join("frostmod.dll")) else {
        // Nothing to copy from. An existing copy is left alone; we can't judge it.
        return if dlo.exists() { PluginCopy::Unmanaged } else { PluginCopy::Absent };
    };
    let have = std::fs::read(&dlo).ok();
    if have.as_deref() == Some(want.as_slice()) {
        return PluginCopy::Current;
    }
    if game_running {
        log::debug!(
            "[frostmod] the plugin at {} is due an update; waiting for the game to close",
            dlo.display()
        );
        return if have.is_some() { PluginCopy::Locked } else { PluginCopy::Absent };
    }
    let failed = if have.is_some() { PluginCopy::Locked } else { PluginCopy::Absent };

    let Some(plugins) = dlo.parent() else {
        return failed;
    };
    if let Err(e) = std::fs::create_dir_all(plugins) {
        log::warn!("[frostmod] couldn't create {}: {e}", plugins.display());
        return failed;
    }
    // Staged beside the target, then swapped in: a half-written `.dlo` is one the game would
    // load. The staging name doesn't end in `.dlo`, so a crash here leaves nothing loadable.
    let staged = dlo.with_extension("dlo.staging");
    if let Err(e) = std::fs::write(&staged, &want) {
        let _ = std::fs::remove_file(&staged);
        log::warn!("[frostmod] couldn't stage the plugin at {}: {e}", staged.display());
        return failed;
    }
    match swap_in(&dlo, &staged) {
        Ok(retired) => {
            if let Some(retired) = retired {
                let _ = std::fs::remove_file(retired);
            }
            log::info!(
                "[frostmod] plugin in place at {} — the game loads FrostMod from it at startup, \
                 nothing injected",
                dlo.display()
            );
            if have.is_some() { PluginCopy::Refreshed } else { PluginCopy::Current }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            log::warn!("[frostmod] couldn't install the plugin at {}: {e}", dlo.display());
            failed
        }
    }
}

/// Take `frostmod.dlo` and `frostmod.dir` out of the game's `plugins` folder — Game
/// Integration switched off.
///
/// The pointer goes only once the plugin has: it is how [`sync_plugin`] knows a leftover
/// `.dlo` is ours to finish removing, rather than something the player put there.
fn remove_game_plugin(game_dir: &Path) -> PluginCopy {
    let dlo = game_plugin_path(game_dir);
    if !remove_or_retire(&dlo) {
        log::warn!("[frostmod] couldn't remove {}; will try again", dlo.display());
        return PluginCopy::Locked;
    }
    match std::fs::remove_file(dir_pointer_path(game_dir)) {
        Ok(()) => log::info!("[frostmod] removed the FrostMod plugin from {}", game_dir.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("[frostmod] couldn't remove frostmod.dir: {e}"),
    }
    PluginCopy::Absent
}

/// Take `frostmod_session.dlo` out: the full plugin publishes the server name in its own
/// block, so the session-only copy has nothing left to do. Left in, it would be a second
/// FrostMod module in the game — one that stands down, but one more thing loaded for nothing.
fn remove_session_plugin(game_dir: &Path) -> PluginCopy {
    let dlo = session_plugin_path(game_dir);
    if !dlo.exists() {
        return PluginCopy::Absent;
    }
    if remove_or_retire(&dlo) {
        log::info!(
            "[frostmod] removed {} — the FrostMod plugin publishes the server name itself",
            dlo.display()
        );
        PluginCopy::Absent
    } else {
        log::warn!("[frostmod] couldn't remove {}; will try again", dlo.display());
        PluginCopy::Locked
    }
}

/// What `frostmod.exe` would have left in FrostMod's folder for the dll, worked out from the
/// flags the player typed. `(file, Some(contents))` is written, `(file, None)` deleted.
///
/// Mirrors the launcher's own argument loop, file for file, because a plugin has no argv:
/// these files are the only way a flag reaches it. The server filter is the one that is on
/// unless switched off, exactly as the launcher has it — without its flag the plugin would
/// quietly stop filtering spam servers for every player on the switch.
///
/// Also returns a `--mods` the player typed, which the launcher would have preferred to the
/// one we send.
#[derive(Debug, Default, PartialEq, Eq)]
struct LauncherFiles {
    mods_override: Option<String>,
    flags: Vec<(&'static str, Option<String>)>,
}

fn launcher_files(extra: &[String]) -> LauncherFiles {
    let (mut probe, mut dump, mut capture, mut switch) = (false, false, false, false);
    let (mut probe_oj, mut force_oj, mut unsafe_reload) = (false, false, false);
    let mut reload_from: i64 = 0;
    let mut filter = true;
    let mut mods_override = None;
    let mut args = extra.iter();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--mods" => mods_override = args.next().cloned(),
            // Take a value we have no file for; skipping it keeps it from being read as a flag.
            "--wait" | "--game" | "--process" => {
                let _ = args.next();
            }
            "--probe-mount" => probe = true,
            "--dump-serverlist" => dump = true,
            "--capture-master" => capture = true,
            "--switch-live" => switch = true,
            "--probe-overjump" => probe_oj = true,
            // Implies the probe, as the launcher has it: forcing with no record is worse.
            "--force-overjump-off" => (force_oj, probe_oj) = (true, true),
            "--unsafe-reload" => unsafe_reload = true,
            "--filter-servers" => filter = true,
            "--no-filter-servers" => filter = false,
            other => {
                if let Some(n) = other.strip_prefix("--unsafe-reload-from=") {
                    // The launcher refuses to start on a step below 1; here that is simply
                    // not a flag, since there is no start to refuse.
                    if let Ok(n) = n.parse::<i64>() {
                        if n >= 1 {
                            reload_from = n;
                            unsafe_reload = true;
                        }
                    }
                }
            }
        }
    }
    let on = |set: bool| set.then(String::new);
    LauncherFiles {
        mods_override,
        flags: vec![
            ("frostmod_probe.flag", on(probe)),
            ("frostmod_dumplist.flag", on(dump)),
            ("frostmod_capture.flag", on(capture)),
            ("frostmod_trackswitch.flag", on(switch)),
            (
                "frostmod_overjump.flag",
                (probe_oj || force_oj).then(|| {
                    format!("{}{}", if probe_oj { "hex " } else { "" }, if force_oj { "force" } else { "" })
                }),
            ),
            (
                "frostmod_unsafe_reload.flag",
                unsafe_reload.then(|| if reload_from > 1 { reload_from.to_string() } else { String::new() }),
            ),
            ("frostmod_filter.flag", on(filter)),
        ],
    }
}

/// Write what [`launcher_files`] worked out, plus `frostmod_mods.txt`, into `dir`.
///
/// `frostmod_mods.txt` is the one the plugin can't do without: it is how FrostMod learns the
/// mods tree for its track manager and model swap, and only the launcher ever wrote it. It is
/// left as it is when we don't know the folder, rather than emptied.
fn write_launcher_files(dir: &Path, mods: Option<&str>, extra: &[String]) {
    let files = launcher_files(extra);
    let mut writes: Vec<(&str, Option<String>)> = files.flags;
    if let Some(mods) = files.mods_override.as_deref().or(mods) {
        writes.push(("frostmod_mods.txt", Some(mods.to_string())));
    }
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    for (name, contents) in writes {
        let path = dir.join(name);
        match contents {
            Some(c) => {
                if std::fs::read_to_string(&path).is_ok_and(|cur| cur == c) {
                    continue;
                }
                if let Err(e) = std::fs::write(&path, c) {
                    log::warn!("[frostmod] couldn't write {}: {e}", path.display());
                }
            }
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// A host path as the game sees it — which is also how FrostMod, inside the game, has to be
/// told it. On Windows that is the path itself; under Proton or Wine it is the prefix's
/// `C:\…` or `Z:\…` spelling of it. `None` when we can't work out the prefix.
#[cfg(windows)]
fn as_game_sees_it(_cfg: &crate::config::AppConfig, path: &Path) -> Option<String> {
    Some(path.to_string_lossy().into_owned())
}

#[cfg(target_os = "linux")]
fn as_game_sees_it(cfg: &crate::config::AppConfig, path: &Path) -> Option<String> {
    match crate::proton::find(cfg.game(), &cfg.wine_runner) {
        Ok(runner) => Some(crate::proton::windows_path(&runner.prefix(), path)),
        Err(e) => {
            log::warn!("[frostmod] no Proton prefix to point the plugin at: {e:#}");
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn as_game_sees_it(cfg: &crate::config::AppConfig, path: &Path) -> Option<String> {
    match crate::gameproc::game_prefix_and_runner(cfg) {
        Ok((prefix, _)) => {
            // FrostMod's folder is outside the bottle, so it is only reachable as `Z:`.
            // Without it FrostMod falls back to its own folder — it still runs, but its
            // files are where the app never looks.
            if !crate::winehost::has_z_drive(&prefix) {
                log::warn!(
                    "[frostmod] this bottle has no Z: drive, so the plugin can't reach \
                     FrostMod's folder and will keep its files beside itself"
                );
            }
            Some(crate::winehost::windows_path(&prefix, path))
        }
        Err(e) => {
            log::warn!("[frostmod] no Wine bottle to point the plugin at: {e:#}");
            None
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn as_game_sees_it(_cfg: &crate::config::AppConfig, _path: &Path) -> Option<String> {
    None
}

/// The game's install folder, or `None` for "don't know" — `install_dir` hands back an empty
/// string for that, which must not become the path `""`.
fn game_dir_of(cfg: &crate::config::AppConfig) -> Option<PathBuf> {
    Some(cfg.install_dir())
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
}

/// Every game folder the config knows: the active one first, then each title's saved one.
/// Game Integration is one switch for all of them, so turning it off has to reach a plugin
/// installed while another title was active.
fn known_game_dirs(cfg: &crate::config::AppConfig) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = game_dir_of(cfg).into_iter().collect();
    for paths in cfg.games.values() {
        let gp = paths.game_path.trim();
        if gp.is_empty() {
            continue;
        }
        let dir = PathBuf::from(gp);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Remove every plugin of ours — the ones with our `frostmod.dir` beside them — from every
/// game folder we know. For Game Integration off, and for a rollback below
/// [`crate::frostmod::PLUGIN_ONLY_MIN_VERSION`]: an older build would otherwise have its dll
/// copied over our `.dlo` by the injector-era refresh, and load as a full plugin that ignores
/// `frostmod.dir`, next to the injector it has just gone back to.
///
/// Also used with `keep_active` to clear every game *but* the active one: FrostMod's folder
/// describes one title at a time, so the plugin lives in one game at a time.
///
/// Returns what became of the one in `active`.
fn remove_our_plugins(
    cfg: &crate::config::AppConfig,
    active: Option<&Path>,
    keep_active: bool,
) -> PluginCopy {
    let mut result = PluginCopy::Absent;
    for dir in known_game_dirs(cfg) {
        if keep_active && active == Some(dir.as_path()) {
            continue;
        }
        if !dir_pointer_path(&dir).exists() {
            continue;
        }
        let copy = remove_game_plugin(&dir);
        if active == Some(dir.as_path()) {
            result = copy;
        }
    }
    result
}

/// Undo plugin-only mode for a FrostMod below v0.41.0: see [`remove_our_plugins`]. Cheap
/// when there is nothing to undo — one `exists` per known game folder.
fn leave_plugin_mode(cfg: &crate::config::AppConfig) {
    crate::frostmod::set_plugin_mode(None);
    let _ = remove_our_plugins(cfg, None, false);
}

/// What a plugin-only sync found and did, for the status report.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PluginSync {
    pub game_plugin: PluginCopy,
    pub session_plugin: PluginCopy,
}

/// The file half of [`sync_plugin`], with everything the app knows already resolved — so it
/// runs in a test against two temp folders. Pointer first, then the plugin: a `.dlo` that
/// landed without its pointer would load with its files in the wrong place.
fn sync_plugin_files(
    dir: &Path,
    game_dir: &Path,
    pointer: Option<&str>,
    game_running: bool,
) -> PluginSync {
    let session_plugin = remove_session_plugin(game_dir);
    match pointer {
        Some(pointer) => {
            if let Some(why) = dir_pointer_warning(pointer) {
                log::warn!("[frostmod] {why}: {pointer}");
            }
            match write_dir_pointer(game_dir, pointer) {
                Ok(true) => log::info!("[frostmod] frostmod.dir points the plugin at {pointer}"),
                Ok(false) => {}
                Err(e) => log::warn!("[frostmod] couldn't write frostmod.dir: {e}"),
            }
        }
        None => log::warn!(
            "[frostmod] couldn't work out FrostMod's folder as the game sees it; the plugin \
             will keep its files beside itself"
        ),
    }
    let game_plugin = install_game_plugin(dir, game_dir, game_running);
    if !game_running {
        // Whatever an earlier pass had to rename aside while the game held it.
        sweep_retired(&game_dir.join("plugins"));
    }
    PluginSync { game_plugin, session_plugin }
}

/// Bring the game's `plugins` folder in line with a plugin-only FrostMod, and tell
/// `frostmod::is_running` which world it is in.
///
/// With Game Integration on: the plugin and its pointer installed (the plugin only while the
/// game is shut — see [`install_game_plugin`]), the session copy gone, and FrostMod's
/// folder holding what the launcher used to leave there. With it off: the session copy gone
/// and nothing installed — and a plugin of ours still there from a removal the running game
/// blocked is removed now. "Ours" is the pointer beside it, which only this app writes; a
/// `.dlo` the player put there by hand is not touched.
///
/// Only for a FrostMod at [`crate::frostmod::PLUGIN_ONLY_MIN_VERSION`] or newer — callers
/// check. Idempotent, so every caller can just call it.
pub fn sync_plugin(app: &AppHandle, cfg: &crate::config::AppConfig) -> PluginSync {
    let game_dir = game_dir_of(cfg);

    if !cfg.auto_run_frostmod || !is_installed(app) {
        crate::frostmod::set_plugin_mode(None);
        // Every known game, even with the active one's folder unknown: the switch is off
        // for all of them.
        let ours = remove_our_plugins(cfg, game_dir.as_deref(), false);
        let Some(game_dir) = game_dir else {
            return PluginSync::default();
        };
        let session_plugin = remove_session_plugin(&game_dir);
        let game_plugin = if ours != PluginCopy::Absent {
            ours
        } else if game_plugin_path(&game_dir).exists() {
            PluginCopy::Unmanaged
        } else {
            PluginCopy::Absent
        };
        return PluginSync { game_plugin, session_plugin };
    }

    let Some(game_dir) = game_dir else {
        crate::frostmod::set_plugin_mode(None);
        return PluginSync::default();
    };
    let game_running = crate::gameproc::is_game_running();

    // One game at a time, as the injector was: FrostMod's folder — `frostmod_mods.txt`, the
    // flags — describes the active title, and a plugin left in another game would read the
    // wrong mods tree the next time that game was launched from Steam.
    let _ = remove_our_plugins(cfg, Some(&game_dir), true);

    crate::frostmod::set_plugin_mode(Some(game_plugin_path(&game_dir)));
    let dir = frostmod_dir(app);
    ensure_serverfilter(app);
    // The mods *tree*, as `plan_start` sends it — see the note there on why not `mods_path`.
    let mods = (!cfg.mods_path.trim().is_empty())
        .then(|| crate::library::mods_root(&cfg.mods_path))
        .and_then(|root| as_game_sees_it(cfg, &root));
    write_launcher_files(&dir, mods.as_deref(), &split_args(&cfg.frostmod_args));
    let pointer = as_game_sees_it(cfg, &dir);
    sync_plugin_files(&dir, &game_dir, pointer.as_deref(), game_running)
}

/// Is the installed FrostMod a plugin-only build?
pub fn plugin_only(app: &AppHandle) -> bool {
    crate::frostmod::plugin_only(installed_version(app).as_deref())
}

/// [`sync_plugin`], if the installed FrostMod is plugin-only and the game is shut.
///
/// For the moments the plugin has to be right before the next launch: the game has just
/// exited (an update that landed during the session is waiting), the app has just started,
/// or Play is about to be pressed.
pub fn sync_if_shut(app: &AppHandle) {
    if !plugin_only(app) || crate::gameproc::is_game_running() {
        return;
    }
    let cfg = crate::config::load(app).unwrap_or_default();
    let _ = sync_plugin(app, &cfg);
}

/// Game Integration switched on or off. A plugin-only FrostMod has no process to start or
/// stop, so the switch *is* the plugin: on installs it, off removes it and its pointer.
pub fn integration_changed(app: &AppHandle, cfg: &crate::config::AppConfig) {
    if !plugin_only(app) {
        return;
    }
    let sync = sync_plugin(app, cfg);
    log::info!(
        "[frostmod] Game Integration {}: plugin {:?}",
        if cfg.auto_run_frostmod { "on" } else { "off" },
        sync.game_plugin
    );
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
fn apply_staged(dir: &Path, staging: &Path, names: &[&'static str]) -> anyhow::Result<bool> {
    let mut done: Vec<(&str, Option<PathBuf>)> = Vec::new();
    for &name in names {
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
    if !crate::frostmod::plugin_only(Some(&rel.tag_name)) {
        anyhow::bail!(
            "FrostMod {} is no longer supported. Update to {} or newer; MXB only supports the game-plugin installation.",
            rel.tag_name,
            crate::frostmod::PLUGIN_ONLY_MIN_VERSION
        );
    }
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for &want in binaries_for(&rel.tag_name) {
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

    let applied = apply_staged(&dir, &staging, binaries_for(&rel.tag_name));
    let _ = std::fs::remove_dir_all(&staging);
    let mut needs_game_restart = applied?;

    // Written last, and only once both binaries are actually in place, so the version
    // we report can never describe an install that didn't happen.
    std::fs::write(version_path(app), &rel.tag_name)?;

    // Ship our curated server filter. Best-effort.
    ensure_serverfilter(app);

    // A plugin-only build is the plugin: put it in the game now rather than on the next
    // status poll. A running game keeps the copy it loaded until it closes — the new one
    // goes in on its exit (`sync_if_shut`) — so that is a restart, whatever the dll did.
    let cfg = crate::config::load(app).unwrap_or_default();
    let sync = sync_plugin(app, &cfg);
    needs_game_restart |= cfg.auto_run_frostmod
        && matches!(sync.game_plugin, PluginCopy::Locked | PluginCopy::Absent)
        && crate::gameproc::is_game_running();
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
    /// Whatever the player typed into the FrostMod flags box, split into arguments. Passed
    /// through untouched and unvalidated: FrostMod ignores a flag it doesn't know, which is
    /// what lets a diagnostic ship in a FrostMod build before the app knows about it.
    extra: Vec<String>,
}

/// Split a typed flag line into arguments, keeping double-quoted runs together so a path
/// with spaces survives (`--mods "C:\My Mods"`). Everything else is whitespace-separated.
fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Check what has to be true before starting, and work out what to tell FrostMod.
///
/// `None` means FrostMod is already running and there is nothing to do.
#[cfg(any())]
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
    let extra = split_args(&cfg.frostmod_args);
    Ok(Some(StartPlan { exe, game: cfg.active_game.id(), mods_root, extra }))
}

/// Ensure the supported FrostMod plugin is installed for the active game.
pub fn start(app: &AppHandle, state: &FrostmodProcess) -> anyhow::Result<bool> {
    if !plugin_only(app) {
        anyhow::bail!(
            "This FrostMod version is no longer supported. Update FrostMod to {} or newer; MXB only supports the game-plugin installation.",
            crate::frostmod::PLUGIN_ONLY_MIN_VERSION
        );
    }
    Ok(start_plugin(app, state))
}

/// Synchronize the plugin files without starting a process.
fn start_plugin(app: &AppHandle, _state: &FrostmodProcess) -> bool {
    let cfg = crate::config::load(app).unwrap_or_default();
    let sync = sync_plugin(app, &cfg);
    log::info!(
        "FrostMod runs as a game plugin (plugin {:?})",
        sync.game_plugin
    );
    matches!(sync.game_plugin, PluginCopy::Current | PluginCopy::Refreshed)
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
    // The game loads the plugin itself. This hook deliberately does not re-arm a process.
    let _ = sync_plugin(app, cfg);
    log::debug!("FrostMod is a game plugin; the game loaded it itself");
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
    // The player's own flags, already split. Appended last, so one of them can override a
    // flag we sent — which is the point of being able to type them.
    extra: &[String],
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
    args.extend(extra.iter().cloned());
    Ok((
        crate::winehost::plan(&runner, &prefix, exe, &args),
        runner.via().to_string(),
    ))
}

/// Kill the managed FrostMod child, if we started one.
pub fn stop(_state: &FrostmodProcess) {
}

/// Compatibility no-op: plugin-mode FrostMod has no separate process to terminate.
pub fn force_stop_exe() {}

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
pub fn stop_running(_app: &AppHandle, _state: &FrostmodProcess) -> bool {
    log::info!("FrostMod runs as a game plugin: it stops when the game closes (turn Game Integration off to remove it)");
    true
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
            mac_launch(&cfg, &exe, "mxb", Some(&mods), &[]).expect("a stub runner is enough");
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

    /// The flags box is a line of text, and a path in it has spaces. Quoted runs stay one
    /// argument; everything else splits on whitespace.
    #[test]
    fn typed_flags_split_the_way_a_shell_would() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("   "), Vec::<String>::new());
        assert_eq!(
            split_args("--probe-overjump  --force-overjump-off"),
            ["--probe-overjump", "--force-overjump-off"]
        );
        assert_eq!(
            split_args("--mods \"C:\\My Mods\" --wait 2000"),
            ["--mods", "C:\\My Mods", "--wait", "2000"]
        );
    }

    /// What the box is for: a flag typed there reaches FrostMod's argv, after the ones we
    /// always send. Same stub-runner spawn as the start test, so it is the real argv.
    #[cfg(target_os = "macos")]
    #[test]
    fn typed_flags_reach_frostmods_argv() {
        let root = temp_dir("mac-extra-args");
        let prefix = root.join("Bottles/MXB");
        let game_dir = prefix.join("drive_c/Program Files/MX Bikes");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        std::os::unix::fs::symlink("/", prefix.join("dosdevices/z:")).unwrap();
        std::fs::write(game_dir.join(crate::game::MXB.exe), b"stub").unwrap();

        let frostmod = root.join("data/frostmod");
        std::fs::create_dir_all(&frostmod).unwrap();
        let exe = frostmod.join("frostmod.exe");
        std::fs::write(&exe, b"stub").unwrap();

        let record = root.join("argv.txt");
        let runner = root.join("fake-wine");
        std::fs::write(
            &runner,
            format!(
                "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; }} > {}\n",
                record.display()
            ),
        )
        .unwrap();
        std::process::Command::new("chmod").arg("+x").arg(&runner).status().unwrap();

        let mut cfg = crate::config::AppConfig::default();
        cfg.game_path = game_dir.to_string_lossy().into_owned();
        cfg.wine_runner = runner.to_string_lossy().into_owned();
        cfg.frostmod_args = "--probe-overjump --wait 2000".into();

        let (launch, _) = mac_launch(&cfg, &exe, "mxb", None, &split_args(&cfg.frostmod_args))
            .expect("a stub runner is enough");
        let mut cmd = std::process::Command::new(&launch.program);
        cmd.args(&launch.args).current_dir(&frostmod);
        for (key, value) in &launch.env {
            cmd.env(key, value);
        }
        cmd.spawn().unwrap().wait().unwrap();

        let written = std::fs::read_to_string(&record).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            &lines[..],
            [exe.to_string_lossy().as_ref(), "--game", "mxb", "--probe-overjump", "--wait", "2000"],
            "typed flags follow the ones we always send: {written:?}"
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

        let err = mac_launch(&cfg, &root.join("frostmod.exe"), "mxb", None, &[])
            .expect_err("no Z: drive, no way in");
        let msg = format!("{err:#}");
        assert!(msg.contains("Z:"), "names what's missing: {msg}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_release_missing_a_binary_installs_nothing() {
        // Skipping the missing one and installing the rest is what used to stamp a new
        // tag into version.txt over a binary that had never been replaced. The tag must be
        // plugin-only-supported (v0.41.0+) or `release_binaries` bails on that check first,
        // before it ever gets to asking what the release's assets are.
        let rel = Release {
            tag_name: "v0.41.0".into(),
            assets: vec![asset("Release.zip")],
        };
        let err = format!("{:#}", release_binaries(&rel).expect_err("no dll is no release"));
        assert!(err.contains("frostmod.dll"), "names what's missing: {err}");
        assert!(err.contains("v0.41.0"), "names the release: {err}");
    }

    #[test]
    fn a_complete_release_yields_both_download_urls() {
        // Real releases carry more than the one binary we manage, and have shipped mixed
        // case. Plugin-only FrostMod (v0.41.0+) installs only `frostmod.dll` — the injector
        // `frostmod.exe` is intentionally never downloaded — so "both download urls" is the
        // one url for the one binary `release_binaries` looks for.
        let rel = Release {
            tag_name: "v0.41.0".into(),
            assets: vec![
                asset("FrostServer.zip"),
                asset("FrostMod.DLL"),
                asset("frostmod.exe"),
            ],
        };
        let found = release_binaries(&rel).expect("the dll is there");
        let names: Vec<&str> = found.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["frostmod.dll"]);
        assert!(
            found[0].1.browser_download_url.ends_with("FrostMod.DLL"),
            "keeps the asset's own url"
        );
    }

    #[test]
    fn applying_swaps_both_binaries_and_leaves_nothing_behind() {
        let (dir, staging) = installed_pair("apply");

        let needs_restart = apply_staged(&dir, &staging, &BINARIES).expect("nothing holds these files");

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

    /// The bug this whole path exists for: a binary that can't land used to leave the
    /// managed folder without a usable copy at all.
    #[test]
    fn a_binary_that_cant_land_puts_the_earlier_one_back() {
        let (dir, staging) = installed_pair("rollback");
        // Nothing to move into place for the dll — stands in for the locked target
        // that `swap_in` can't complete.
        std::fs::remove_file(staging.join("frostmod.dll")).unwrap();

        let err = format!(
            "{:#}",
            apply_staged(&dir, &staging, &BINARIES).expect_err("a missing staged binary can't land")
        );
        assert!(err.contains("frostmod.dll"), "names the binary: {err}");
        assert!(err.contains("MX Bikes"), "says how to fix it: {err}");

        for name in BINARIES {
            assert_eq!(read(dir.join(name)), "old", "{name} is back as it was");
        }

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

#[cfg(test)]
mod plugin_copy_tests {
    use super::*;

    fn dirs(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("frostmod-dlo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let managed = root.join("managed");
        let game = root.join("game");
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::create_dir_all(game.join("plugins")).unwrap();
        (managed, game)
    }

    /// A tag new enough to install the session plugin from, and one that isn't.
    const NEW: &str = "v0.17.0";
    const OLD: &str = "v0.16.3";

    #[test]
    fn the_session_plugin_is_installed_and_then_left_alone() {
        let (managed, game) = dirs("session-install");
        std::fs::write(managed.join("frostmod.dll"), b"v0.17.0-bytes").unwrap();

        assert_eq!(ensure_session_plugin(&managed, &game, Some(NEW)), PluginCopy::Current);
        assert_eq!(
            std::fs::read(session_plugin_path(&game)).unwrap(),
            b"v0.17.0-bytes",
            "the plugin is a byte-identical copy of the dll we inject"
        );
        // Every status poll runs this. It must settle, not re-copy a quarter-megabyte.
        assert_eq!(ensure_session_plugin(&managed, &game, Some(NEW)), PluginCopy::Current);
    }

    /// The rule that keeps this from being the thing that hangs the game: a build that
    /// doesn't know the name would run as a *full* plugin beside the injected copy.
    #[test]
    fn a_build_that_predates_session_mode_never_installs_one() {
        let (managed, game) = dirs("session-old");
        std::fs::write(managed.join("frostmod.dll"), b"v0.16.3-bytes").unwrap();

        assert_eq!(ensure_session_plugin(&managed, &game, Some(OLD)), PluginCopy::Absent);
        assert!(!session_plugin_path(&game).exists());
        // An unreadable tag is the same answer, for the same reason.
        assert_eq!(ensure_session_plugin(&managed, &game, None), PluginCopy::Absent);
        assert!(!session_plugin_path(&game).exists());
    }

    /// A downgrade leaves ours behind, and the older build would load it as a full plugin.
    #[test]
    fn a_rollback_parks_the_session_plugin_it_left_behind() {
        let (managed, game) = dirs("session-rollback");
        std::fs::write(managed.join("frostmod.dll"), b"v0.17.0-bytes").unwrap();
        assert_eq!(ensure_session_plugin(&managed, &game, Some(NEW)), PluginCopy::Current);

        assert_eq!(ensure_session_plugin(&managed, &game, Some(OLD)), PluginCopy::Disabled);
        assert!(
            !session_plugin_path(&game).exists(),
            "a plugin an older build would misread must stop being a .dlo"
        );
    }

    #[test]
    fn a_new_frostmod_refreshes_the_session_plugin() {
        let (managed, game) = dirs("session-stale");
        std::fs::write(managed.join("frostmod.dll"), b"old-bytes").unwrap();
        assert_eq!(ensure_session_plugin(&managed, &game, Some(NEW)), PluginCopy::Current);

        // A longer file with a newer mtime: an update landed.
        std::fs::write(managed.join("frostmod.dll"), b"much-newer-bytes").unwrap();
        assert_eq!(ensure_session_plugin(&managed, &game, Some(NEW)), PluginCopy::Refreshed);
        assert_eq!(std::fs::read(session_plugin_path(&game)).unwrap(), b"much-newer-bytes");
    }

    /// A hand-installed `frostmod.dlo` is a different file with a different job, and this
    /// must not touch it — it is somebody's deliberate full plugin-mode install.
    #[test]
    fn the_hand_installed_plugin_is_left_where_it_is() {
        let (managed, game) = dirs("session-beside");
        std::fs::write(managed.join("frostmod.dll"), b"v0.17.0-bytes").unwrap();
        std::fs::write(game_plugin_path(&game), b"hand-installed").unwrap();

        assert_eq!(ensure_session_plugin(&managed, &game, Some(NEW)), PluginCopy::Current);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"hand-installed");
        assert!(session_plugin_path(&game).exists());
    }

    /// The case that cost a player their game: a plugin installed by hand months ago, which
    /// nothing has ever updated, still loaded by the game at every startup.
    #[test]
    fn a_stale_plugin_is_brought_up_to_date() {
        let (managed, game) = dirs("stale");
        std::fs::write(managed.join("frostmod.dll"), b"v0.16.2-bytes").unwrap();
        std::fs::write(game_plugin_path(&game), b"v0.12").unwrap();

        assert_eq!(refresh_game_plugin(&managed, &game), PluginCopy::Refreshed);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"v0.16.2-bytes");
    }

    /// Never install one. The app drives FrostMod by injection; creating a plugin would
    /// change how it loads on a machine that never asked for plugin mode.
    #[test]
    fn no_plugin_means_nothing_to_do() {
        let (managed, game) = dirs("absent");
        std::fs::write(managed.join("frostmod.dll"), b"whatever").unwrap();

        assert_eq!(refresh_game_plugin(&managed, &game), PluginCopy::Absent);
        assert!(!game_plugin_path(&game).exists(), "a plugin must never be created");
    }

    /// A plugin that already matches is left alone — and, because staleness is judged on
    /// size and mtime, re-checking it does not rewrite it on every status poll.
    #[test]
    fn a_current_plugin_is_left_alone_and_stays_current() {
        let (managed, game) = dirs("current");
        std::fs::write(managed.join("frostmod.dll"), b"same-bytes").unwrap();
        std::fs::write(game_plugin_path(&game), b"same-bytes").unwrap();

        // First pass may refresh (the fixture's mtimes are whatever the FS gave them);
        // what matters is that it converges and then stays put.
        let _ = refresh_game_plugin(&managed, &game);
        assert_eq!(refresh_game_plugin(&managed, &game), PluginCopy::Current);
        assert_eq!(refresh_game_plugin(&managed, &game), PluginCopy::Current);
    }

    /// Nothing to compare against and nothing to copy from is not a licence to start
    /// renaming files in somebody's game folder.
    #[test]
    fn no_managed_dll_leaves_the_plugin_untouched() {
        let (managed, game) = dirs("nodll");
        std::fs::write(game_plugin_path(&game), b"v0.12").unwrap();

        assert_eq!(refresh_game_plugin(&managed, &game), PluginCopy::Unmanaged);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"v0.12");
    }

    /// The force: a stale plugin that can't be updated stops loading rather than being left
    /// to hang the game. Renamed, not deleted — and to a name that isn't `.dlo`, or the game
    /// would just load it again.
    #[test]
    fn a_stale_plugin_that_cannot_be_updated_is_moved_aside() {
        let (managed, game) = dirs("disable");
        let dlo = game_plugin_path(&game);
        std::fs::write(&dlo, b"v0.12").unwrap();

        let parked = disable_game_plugin(&dlo);

        assert_eq!(parked, PluginCopy::Disabled);
        assert!(!dlo.exists(), "the game must stop loading it");
        let kept = dlo.with_extension("dlo.disabled");
        assert_eq!(std::fs::read(&kept).unwrap(), b"v0.12", "nothing is destroyed");
        assert_ne!(
            kept.extension().and_then(|e| e.to_str()),
            Some("dlo"),
            "a parked copy still ending in .dlo would be loaded as a plugin"
        );
    }

    /// Twice through the same fix must not overwrite the first parked copy.
    #[test]
    fn a_second_disable_does_not_clobber_the_first() {
        let (managed, game) = dirs("disable-twice");
        let dlo = game_plugin_path(&game);

        std::fs::write(&dlo, b"first").unwrap();
        assert_eq!(disable_game_plugin(&dlo), PluginCopy::Disabled);
        std::fs::write(&dlo, b"second").unwrap();
        assert_eq!(disable_game_plugin(&dlo), PluginCopy::Disabled);

        assert_eq!(std::fs::read(dlo.with_extension("dlo.disabled")).unwrap(), b"first");
        assert_eq!(std::fs::read(dlo.with_extension("dlo.disabled-1")).unwrap(), b"second");
        let _ = managed;
    }
}

#[cfg(test)]
mod plugin_only_tests {
    use super::*;

    fn dirs(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("frostmod-po-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let managed = root.join("managed");
        let game = root.join("game");
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::create_dir_all(game.join("plugins")).unwrap();
        (managed, game)
    }

    /// The whole switch on a shut game: the plugin is a byte copy of the dll, the pointer
    /// names our folder, and the session copy the full plugin replaces is gone.
    #[test]
    fn a_shut_game_gets_the_plugin_the_pointer_and_loses_the_session_copy() {
        let (managed, game) = dirs("install");
        std::fs::write(managed.join("frostmod.dll"), b"v0.41.0-bytes").unwrap();
        std::fs::write(session_plugin_path(&game), b"session").unwrap();
        let pointer = managed.to_string_lossy().into_owned();

        let sync = sync_plugin_files(&managed, &game, Some(&pointer), false);

        assert_eq!(sync.game_plugin, PluginCopy::Current);
        assert_eq!(sync.session_plugin, PluginCopy::Absent);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"v0.41.0-bytes");
        assert_eq!(std::fs::read_to_string(dir_pointer_path(&game)).unwrap(), pointer);
        assert!(!session_plugin_path(&game).exists());
        // Nothing half-written left where the game would load it.
        let names: Vec<String> = std::fs::read_dir(game.join("plugins"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| n == "frostmod.dlo" || n == "frostmod.dir"), "{names:?}");

        // Every poll runs it; the second pass changes nothing.
        let again = sync_plugin_files(&managed, &game, Some(&pointer), false);
        assert_eq!(again.game_plugin, PluginCopy::Current);
    }

    /// One line, the path, nothing else: FrostMod reads the first line as the folder.
    #[test]
    fn the_pointer_is_the_path_alone_and_only_rewritten_when_it_changes() {
        let (_, game) = dirs("pointer");
        let path = r"C:\Users\rider\AppData\Local\com.frost.mxbikes\frostmod";
        assert!(write_dir_pointer(&game, path).unwrap());
        assert_eq!(std::fs::read(dir_pointer_path(&game)).unwrap(), path.as_bytes());
        assert!(!write_dir_pointer(&game, path).unwrap(), "same text, no write");
        assert!(write_dir_pointer(&game, r"Z:\home\rider\frostmod").unwrap());
        assert_eq!(std::fs::read_to_string(dir_pointer_path(&game)).unwrap(), r"Z:\home\rider\frostmod");
    }

    #[test]
    fn a_pointer_frostmod_may_refuse_is_warned_about() {
        assert!(dir_pointer_warning(r"C:\Users\rider\AppData\Local\x\frostmod").is_none());
        assert!(dir_pointer_warning(r"C:\Users\Jürgen\AppData\Local\x\frostmod").is_some());
        let long = format!(r"C:\{}", "a".repeat(DIR_POINTER_WARN_LEN));
        assert!(dir_pointer_warning(&long).is_some());
    }

    /// An update replaces a stale copy while the game is shut...
    #[test]
    fn a_stale_plugin_is_replaced_while_the_game_is_shut() {
        let (managed, game) = dirs("replace");
        std::fs::write(managed.join("frostmod.dll"), b"new").unwrap();
        std::fs::write(game_plugin_path(&game), b"old-and-longer").unwrap();
        assert_eq!(install_game_plugin(&managed, &game, false), PluginCopy::Refreshed);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"new");
    }

    /// ...and is left alone while it runs: the game holds its plugins open for the session,
    /// and the next pass after it exits does the swap.
    #[test]
    fn nothing_in_plugins_is_touched_while_the_game_runs() {
        let (managed, game) = dirs("running");
        std::fs::write(managed.join("frostmod.dll"), b"new").unwrap();
        std::fs::write(game_plugin_path(&game), b"old").unwrap();
        assert_eq!(install_game_plugin(&managed, &game, true), PluginCopy::Locked);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"old");

        let (managed, game) = dirs("running-absent");
        std::fs::write(managed.join("frostmod.dll"), b"new").unwrap();
        assert_eq!(install_game_plugin(&managed, &game, true), PluginCopy::Absent);
        assert!(!game_plugin_path(&game).exists());
    }

    /// A same-size copy with different bytes is still stale: this copy is compared exactly.
    #[test]
    fn same_size_different_bytes_is_replaced() {
        let (managed, game) = dirs("same-size");
        std::fs::write(managed.join("frostmod.dll"), b"AAAA").unwrap();
        std::fs::write(game_plugin_path(&game), b"BBBB").unwrap();
        assert_eq!(install_game_plugin(&managed, &game, false), PluginCopy::Refreshed);
        assert_eq!(std::fs::read(game_plugin_path(&game)).unwrap(), b"AAAA");
    }

    #[test]
    fn no_dll_means_no_plugin_is_invented() {
        let (managed, game) = dirs("no-dll");
        assert_eq!(install_game_plugin(&managed, &game, false), PluginCopy::Absent);
        assert!(!game_plugin_path(&game).exists());
    }

    /// Game Integration off: both files go.
    #[test]
    fn integration_off_removes_the_plugin_and_its_pointer() {
        let (_, game) = dirs("remove");
        std::fs::write(game_plugin_path(&game), b"plugin").unwrap();
        std::fs::write(dir_pointer_path(&game), b"C:\\x").unwrap();
        assert_eq!(remove_game_plugin(&game), PluginCopy::Absent);
        assert!(!game_plugin_path(&game).exists());
        assert!(!dir_pointer_path(&game).exists());
        // Nothing there is nothing to do.
        assert_eq!(remove_game_plugin(&game), PluginCopy::Absent);
    }

    /// Integration off, or a rollback, reaches every game the config knows — but only the
    /// plugins with our pointer beside them. A hand-installed `.dlo` is left alone.
    #[test]
    fn our_plugins_are_removed_from_every_known_game_and_only_ours() {
        let (_, mxb) = dirs("all-mxb");
        let (_, gpb) = dirs("all-gpb");
        let (_, hand) = dirs("all-hand");
        for game in [&mxb, &gpb] {
            std::fs::write(game_plugin_path(game), b"ours").unwrap();
            std::fs::write(dir_pointer_path(game), b"C:\\x").unwrap();
        }
        std::fs::write(game_plugin_path(&hand), b"hand-installed").unwrap();

        let mut cfg = crate::config::AppConfig::default();
        cfg.game_path = mxb.to_string_lossy().into_owned();
        for (id, game) in [("gpb", &gpb), ("other", &hand)] {
            cfg.games.insert(
                id.into(),
                crate::config::GamePaths {
                    game_path: game.to_string_lossy().into_owned(),
                    ..Default::default()
                },
            );
        }

        assert_eq!(remove_our_plugins(&cfg, Some(&mxb), false), PluginCopy::Absent);
        assert!(!game_plugin_path(&mxb).exists() && !dir_pointer_path(&mxb).exists());
        assert!(
            !game_plugin_path(&gpb).exists() && !dir_pointer_path(&gpb).exists(),
            "the game that wasn't active is cleaned too"
        );
        assert_eq!(std::fs::read(game_plugin_path(&hand)).unwrap(), b"hand-installed");

        // With integration on, only the active game keeps it: FrostMod's folder describes
        // one title, and the other would read the wrong mods tree.
        for game in [&mxb, &gpb] {
            std::fs::write(game_plugin_path(game), b"ours").unwrap();
            std::fs::write(dir_pointer_path(game), b"C:\\x").unwrap();
        }
        let _ = remove_our_plugins(&cfg, Some(&mxb), true);
        assert!(game_plugin_path(&mxb).exists(), "the active game keeps its plugin");
        assert!(!game_plugin_path(&gpb).exists(), "the other game's goes");
    }

    /// A plugin-only release is installed as the dll alone: a quarantined or missing
    /// `frostmod.exe` neither fails the install nor flags it for repair.
    #[test]
    fn a_plugin_only_release_needs_only_the_dll() {
        assert_eq!(binaries_for("v0.41.0"), &["frostmod.dll"]);
        assert_eq!(binaries_for("v0.40.4"), &["frostmod.dll"]);

        let (managed, _) = dirs("dll-only");
        std::fs::write(managed.join("frostmod.dll"), b"the 0.41 dll").unwrap();
        let rel = Release {
            tag_name: "v0.41.0".into(),
            assets: vec![
                Asset {
                    name: "frostmod.dll".into(),
                    browser_download_url: "https://example.invalid/frostmod.dll".into(),
                    size: 12,
                    digest: None,
                },
            ],
        };
        assert!(release_binaries(&rel).is_ok(), "no exe in the release is fine");
        assert!(install_matches_release(&managed, &rel), "no exe on disk is fine");

        let old = Release { tag_name: "v0.40.4".into(), ..rel };
        assert!(release_binaries(&old).is_err(), "legacy releases fail closed");
    }

    #[test]
    fn no_session_plugin_is_nothing_to_remove() {
        let (_, game) = dirs("no-session");
        assert_eq!(remove_session_plugin(&game), PluginCopy::Absent);
    }

    fn flag<'a>(files: &'a LauncherFiles, name: &str) -> Option<&'a str> {
        files
            .flags
            .iter()
            .find(|(n, _)| *n == name)
            .expect("every launcher flag is listed")
            .1
            .as_deref()
    }

    /// No flags typed: what the launcher leaves with no arguments — the server filter on,
    /// everything else off.
    #[test]
    fn with_no_flags_only_the_server_filter_is_on() {
        let files = launcher_files(&[]);
        assert_eq!(flag(&files, "frostmod_filter.flag"), Some(""));
        for name in [
            "frostmod_probe.flag",
            "frostmod_dumplist.flag",
            "frostmod_capture.flag",
            "frostmod_trackswitch.flag",
            "frostmod_overjump.flag",
            "frostmod_unsafe_reload.flag",
        ] {
            assert_eq!(flag(&files, name), None, "{name} is deleted");
        }
        assert_eq!(files.mods_override, None);
    }

    #[test]
    fn typed_flags_become_the_files_the_launcher_wrote() {
        let args = split_args(
            "--no-filter-servers --force-overjump-off --unsafe-reload-from=3 --wait 2000 \
             --mods \"D:\\My Mods\\mods\" --probe-mount",
        );
        let files = launcher_files(&args);
        assert_eq!(flag(&files, "frostmod_filter.flag"), None);
        // Forcing implies the probe, as it does in the launcher.
        assert_eq!(flag(&files, "frostmod_overjump.flag"), Some("hex force"));
        assert_eq!(flag(&files, "frostmod_unsafe_reload.flag"), Some("3"));
        assert_eq!(flag(&files, "frostmod_probe.flag"), Some(""));
        assert_eq!(files.mods_override.as_deref(), Some(r"D:\My Mods\mods"));

        let plain = launcher_files(&split_args("--unsafe-reload --probe-overjump"));
        assert_eq!(flag(&plain, "frostmod_unsafe_reload.flag"), Some(""));
        assert_eq!(flag(&plain, "frostmod_overjump.flag"), Some("hex "));
    }

    /// `frostmod_mods.txt` is written from what we know, a typed `--mods` wins, and flags
    /// switched off are deleted rather than left armed from a previous run.
    #[test]
    fn launcher_files_land_in_frostmods_folder() {
        let (managed, _) = dirs("launcher");
        std::fs::write(managed.join("frostmod_probe.flag"), b"").unwrap();
        write_launcher_files(&managed, Some(r"C:\Users\r\Documents\PiBoSo\MX Bikes\mods"), &[]);
        assert_eq!(
            std::fs::read_to_string(managed.join("frostmod_mods.txt")).unwrap(),
            r"C:\Users\r\Documents\PiBoSo\MX Bikes\mods"
        );
        assert!(managed.join("frostmod_filter.flag").exists());
        assert!(!managed.join("frostmod_probe.flag").exists(), "a stale flag is disarmed");

        write_launcher_files(&managed, Some(r"C:\ignored"), &split_args(r"--mods D:\mods"));
        assert_eq!(std::fs::read_to_string(managed.join("frostmod_mods.txt")).unwrap(), r"D:\mods");

        // Not knowing the folder leaves the last one in place rather than emptying it.
        write_launcher_files(&managed, None, &[]);
        assert_eq!(std::fs::read_to_string(managed.join("frostmod_mods.txt")).unwrap(), r"D:\mods");
    }
}
