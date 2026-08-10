use crate::game::{Game, GameProfile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Where one title's content lives. The active game's copy is mirrored into
/// [`AppConfig`]'s own `mods_path` / `game_path` / `profiles_path`; this is how the
/// *other* titles' folders are remembered across a switch.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GamePaths {
    pub mods_path: String,
    pub game_path: String,
    pub profiles_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// The title the app is currently driving. Absent in every config written before
    /// multi-game support, which deserializes to [`Game::Mxb`] — the title those
    /// installs are for.
    pub active_game: Game,
    /// Saved folders per title, keyed by [`Game::id`]. The active game's entry is kept
    /// in sync with the flat fields below on every save.
    ///
    /// The flat fields stay the source of truth for the *active* game rather than being
    /// replaced by this map: roughly fifty call sites read `cfg.mods_path` directly, and
    /// a config that keeps meaning what it always meant is also one an older build can
    /// still read.
    pub games: BTreeMap<String, GamePaths>,
    /// Where the active game's content lives. Two shapes are legal and
    /// [`crate::library::mods_root`] is the one place that tells them apart: the game's
    /// *user* folder (`Documents\PiBoSo\MX Bikes`, holding `mods/` and `profiles/`), or
    /// the mods tree itself (`C:\mods`) for a player who relocated it with
    /// `mxbikes.ini`'s `[mods] folder`.
    ///
    /// Never join `mods` onto this by hand — go through
    /// [`crate::library::mods_subdir`] / [`crate::library::mods_root`], or a relocated
    /// tree resolves to `C:\mods\mods`.
    pub mods_path: String,
    /// Active game's install dir (its executable + core archives); distinct from
    /// `mods_path`.
    pub game_path: String,
    /// Override for the PiBoSo `profiles` folder. Empty (the normal case) means it's
    /// resolved from `mods_path` — see [`AppConfig::profiles_dir`]. Set it when the
    /// resolver can't find the folder, e.g. profiles on a drive we don't probe.
    pub profiles_path: String,
    /// macOS: the Wine binary that starts the game. Empty (the normal case) means it's
    /// auto-detected — see [`crate::winehost::resolve`]. Machine-wide rather than
    /// per-game: one Mac has one set of wrappers installed. Ignored on Windows and Linux.
    pub wine_runner: String,
    /// Hide to the tray on window close and keep running.
    pub run_in_background: bool,
    /// Start MXB App automatically on login.
    pub launch_at_startup: bool,
    /// Launch FrostMod automatically when the app opens.
    pub auto_run_frostmod: bool,
    /// Re-run the game's profile loader in place after applying a preset (Windows-only).
    pub instant_refresh: bool,
    /// Watch `<mods_path>/mods` and signal FrostMod to reload when tracks/bikes are
    /// added outside the app (e.g. a manual download dropped into the folder).
    pub watch_mods_reload: bool,
    /// The intro slideshow has been dismissed. Kept here rather than in the webview's
    /// `localStorage` so it survives that storage being cleared (WebView2 resets it on
    /// an app-data wipe, and an OS shutdown can kill the tray-resident process before
    /// it flushes) — losing it made the app re-run its first-run flow on every launch.
    pub welcome_seen: bool,
    /// The first-run guided tour has been finished or skipped. Persisted alongside
    /// `welcome_seen`, for the same reason.
    pub tour_done: bool,
    /// Register the global hotkey that summons the in-game overlay.
    pub overlay_enabled: bool,
    /// The combo that toggles the overlay, in Tauri accelerator syntax
    /// (`"CommandOrControl+Shift+X"`). Blank falls back to [`DEFAULT_OVERLAY_HOTKEY`].
    pub overlay_hotkey: String,
    /// The app version whose release showcase has already been shown. Blank means the
    /// install predates the showcase — an upgrade, so the newest showcase is due.
    /// A *fresh* install is stamped with the running version at setup (see
    /// `create_config`), because nothing in a version you just installed is new to you.
    pub seen_version: String,
    /// Dedicated servers this player administers, each with its agent address and bearer
    /// token. Stored here in clear, like the rest of the config — worth knowing before
    /// adding a server whose token protects anything beyond the game process it runs.
    pub servers: Vec<crate::servers::ServerRef>,
    /// Show the unfinished multiplayer features — the Servers tab and paint sync.
    ///
    /// Off by default even in a beta build: these talk to a live control plane and write
    /// files other players uploaded, so they're opt-in rather than something a player finds
    /// by accident. Also settable with `MXB_EXPERIMENTAL=1` for a run that doesn't touch
    /// the saved config (see [`AppConfig::experimental_enabled`]).
    pub experimental: bool,
    /// Bearer token for this player's control-plane account, from enrolling with an invite
    /// code. Empty until they enroll.
    pub cp_token: String,
    /// The in-game rider name this account enrolled with. Kept so the UI can show which
    /// identity the paints are published under.
    pub cp_rider_name: String,
    /// This player's MX Bikes GUID, once claimed. The stable identity the roster keys on —
    /// rider names are free text and change between sessions.
    pub cp_guid: String,
    /// What paint sync last did, so the UI can say so instead of the player having to guess
    /// from an empty grid. Written by the background tasks; never edited by hand.
    pub sync: SyncState,
}

/// The record of the last publish and the last pull.
///
/// Persisted rather than held in memory because the question it answers — "is my look
/// actually out there?" — is asked on a cold start, before anything has run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncState {
    /// Digest of the look last sent up. An identical look is never re-sent, which is what
    /// lets every path that might have changed one publish without checking first.
    pub published_digest: String,
    /// Unix milliseconds. `0` means never.
    pub published_at: u64,
    pub published_bikes: usize,
    pub published_paints: usize,
    /// Unix milliseconds of the last successful pull. `0` means never.
    pub pulled_at: u64,
    pub pulled_riders: usize,
    /// Paints another rider published that would have landed on a file this machine already
    /// had. Kept back rather than overwritten — see `paintsync::pull`.
    pub kept_yours: usize,
    /// Destinations two riders disagreed about, so neither was installed.
    pub conflicted: usize,
}

/// Set to `1` to force the experimental features on for one run.
pub const EXPERIMENTAL_ENV: &str = "MXB_EXPERIMENTAL";

impl AppConfig {
    /// Whether the experimental features should be visible.
    ///
    /// The environment variable wins so a build can be handed to a tester with a flag
    /// rather than a settings walkthrough, and so turning it on for one run leaves no
    /// trace in their saved config.
    pub fn experimental_enabled(&self) -> bool {
        if std::env::var(EXPERIMENTAL_ENV).map(|v| v == "1").unwrap_or(false) {
            return true;
        }
        self.experimental
    }
}

/// Toggle combo used until the player picks another one.
///
/// Ctrl+Shift+X is free in MX Bikes — its bindings are single keys and gamepad inputs —
/// and isn't claimed by Windows or by the apps that sit alongside a race: Discord,
/// Steam (Shift+Tab) and GeForce Experience (Alt+Z, Alt+F*).
pub const DEFAULT_OVERLAY_HOTKEY: &str = "CommandOrControl+Shift+X";

/// Defaults we've shipped and then moved away from.
///
/// Ctrl+Shift+M is Discord's mute toggle. Discord registers it globally and gets there
/// first, so our `register` call fails and the overlay never opens — invisibly, since a
/// hotkey that was never bound has nothing to report at the moment it isn't pressed.
/// A config still carrying one of these was never deliberately chosen, so it moves to
/// the current default. Anything else is the player's pick and is left alone.
pub const LEGACY_OVERLAY_HOTKEYS: &[&str] = &["CommandOrControl+Shift+M"];

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_game: Game::default(),
            games: BTreeMap::new(),
            mods_path: String::new(),
            game_path: String::new(),
            profiles_path: String::new(),
            wine_runner: String::new(),
            run_in_background: true,
            launch_at_startup: true,
            auto_run_frostmod: true,
            instant_refresh: true,
            watch_mods_reload: true,
            welcome_seen: false,
            tour_done: false,
            overlay_enabled: true,
            overlay_hotkey: DEFAULT_OVERLAY_HOTKEY.to_string(),
            seen_version: String::new(),
            servers: Vec::new(),
            experimental: false,
            cp_token: String::new(),
            cp_rider_name: String::new(),
            cp_guid: String::new(),
            sync: SyncState::default(),
        }
    }
}

/// Bring a config written by an older build up to date.
///
/// Applied on every read rather than in a one-shot upgrade step: the config is also
/// written by hand and by older builds still on disk, so "has this already been
/// migrated?" is only ever answerable from the values themselves.
pub fn migrate(mut cfg: AppConfig) -> AppConfig {
    if LEGACY_OVERLAY_HOTKEYS.contains(&cfg.overlay_hotkey.trim()) {
        log::info!(
            "moving the overlay hotkey off the retired default {} → {DEFAULT_OVERLAY_HOTKEY}",
            cfg.overlay_hotkey.trim(),
        );
        cfg.overlay_hotkey = DEFAULT_OVERLAY_HOTKEY.to_string();
    }
    // Pre-multi-game configs have folders but no `games` map. Seed the active game's
    // entry from them so switching away and back doesn't lose the folders someone has
    // been using — see `stash_active`, which is the same operation on the write side.
    cfg.stash_active();
    cfg
}

impl AppConfig {
    /// The active title's constants — folder names, executable, mods taxonomy, caps.
    pub fn game(&self) -> &'static GameProfile {
        self.active_game.profile()
    }

    /// Copy the live folders into `games[active]`. Called on every save and on every
    /// read, so the map is always current no matter which build wrote the file.
    pub fn stash_active(&mut self) {
        self.games.insert(
            self.active_game.id().to_string(),
            GamePaths {
                mods_path: self.mods_path.clone(),
                game_path: self.game_path.clone(),
                profiles_path: self.profiles_path.clone(),
            },
        );
    }

    /// Make `game` the active title, swapping the live folders for that game's saved
    /// ones. The outgoing game's folders are stashed first, so switching back restores
    /// them. A game with nothing saved comes up blank and is filled in by
    /// [`finalize`] — i.e. auto-detected on first switch.
    ///
    /// Returns whether anything changed, so callers can skip the work (restarting the
    /// mods watcher, re-scanning) when the user re-picks the game they're already on.
    pub fn switch_game(&mut self, game: Game) -> bool {
        if self.active_game == game {
            return false;
        }
        self.stash_active();
        let next = self.games.get(game.id()).cloned().unwrap_or_default();
        self.active_game = game;
        self.mods_path = next.mods_path;
        self.game_path = next.game_path;
        self.profiles_path = next.profiles_path;
        true
    }

    /// Folder that holds the per-player PiBoSo profiles (each a subdir with a
    /// `profile.ini`).
    ///
    /// An explicit `profiles_path` always wins. Otherwise it's `<mods_path>/profiles`
    /// — the normal, combined layout — falling back to the folder beside a relocated
    /// mods tree and then to the stock `Documents\PiBoSo\MX Bikes\profiles`.
    ///
    /// The fallbacks are not exotic: `mxbikes.ini` lets a player point the *mods* folder
    /// at another drive, and the game has no equivalent redirect for profiles, so it
    /// keeps writing them to `Documents`. Anyone who uses that documented feature ends
    /// up with a split layout, and without this the app looked for profiles in a folder
    /// that never existed.
    /// The game's *install* dir — where the executable and its core archives live.
    ///
    /// Deliberately never falls back to `mods_path` the way content lookups do. The two are
    /// different places on a stock install (`Documents\PiBoSo\…` vs the Steam library), and
    /// the things that want this one — ReShade's proxy DLL, the game's own files — are only
    /// ever beside the executable. Returns an empty string when it isn't configured and Steam
    /// detection finds nothing, which callers must treat as "don't know", not as "not there".
    pub fn install_dir(&self) -> String {
        let gp = self.game_path.trim();
        if !gp.is_empty() {
            return gp.to_string();
        }
        detect_game_path(self.game()).unwrap_or_default()
    }

    pub fn profiles_dir(&self) -> PathBuf {
        let custom = self.profiles_path.trim();
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
        // Case-tolerant join: under Proton the folder can come back as `Profiles`.
        let primary = crate::library::resolve_child(Path::new(self.mods_path.trim()), "profiles");
        let game = self.game();
        // The folder above a mods tree, when `mods_path` is one. Covers a pick of
        // `…\MX Bikes\mods` on an install the game has never run — no `profiles/` inside
        // the tree, and none to fall back to in `Documents` either, but the right folder
        // is one level up and about to be written.
        let beside = self.mods_tree_parent().map(|p| crate::library::resolve_child(&p, "profiles"));
        resolve_profiles_dir(primary, || {
            beside
                .filter(|p| p.is_dir())
                .or_else(|| default_user_dir(game).map(|d| d.join("profiles")))
        })
    }

    /// The folder above `mods_path` when `mods_path` is the mods tree itself, else
    /// `None`. A drive root (`C:\mods` → `C:\`) is no use to anyone, so it doesn't count.
    fn mods_tree_parent(&self) -> Option<PathBuf> {
        let dir = Path::new(self.mods_path.trim());
        if crate::library::mods_root(&self.mods_path) != dir {
            return None;
        }
        dir.parent()
            .filter(|p| p.parent().is_some() && p.is_dir())
            .map(Path::to_path_buf)
    }
}

/// Pick between the profiles folder implied by `mods_path` and the stock PiBoSo one.
///
/// Only a *missing* primary hands over to the fallback — an existing folder is the
/// player's, empty or not. When neither exists we still return the primary, so the
/// path the UI reports is the one derived from the folder they picked.
///
/// `fallback` is a closure because working it out means scanning Steam libraries on
/// Linux, and this runs on every preset read and write.
fn resolve_profiles_dir(primary: PathBuf, fallback: impl FnOnce() -> Option<PathBuf>) -> PathBuf {
    if primary.is_dir() {
        return primary;
    }
    match fallback() {
        Some(f) if f.is_dir() => f,
        _ => primary,
    }
}

/// The game's user folder where it puts it when nothing has been moved: inside the Wine
/// prefix wherever the game runs as a Windows process (Proton on Linux, a bottle on
/// macOS), `Documents\PiBoSo\<game>` on Windows.
fn default_user_dir(game: &GameProfile) -> Option<PathBuf> {
    if let Some(p) = detect_prefix_mods_path(game) {
        return Some(PathBuf::from(p));
    }
    Some(dirs_next::document_dir()?.join("PiBoSo").join(game.user_dir))
}

pub fn config_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("config.json")
}

pub fn exists(app: &AppHandle) -> bool {
    config_path(app).exists()
}

/// Which game a set of folders belongs to, judged only by the folders themselves.
///
/// Used to recover from a config that has folders but no `activeGame` — see [`load`].
/// The evidence is ordered strongest first: an install folder holding a particular
/// executable is conclusive, while the user folder's name is merely conventional (it's
/// `Documents\PiBoSo\<game>` unless someone moved it).
fn infer_game(cfg: &AppConfig) -> Option<Game> {
    let install = cfg.game_path.trim();
    if !install.is_empty() {
        for g in Game::ALL {
            if crate::library::resolve_child(Path::new(install), g.profile().exe).is_file() {
                return Some(g);
            }
        }
    }
    let leaf = Path::new(cfg.mods_path.trim()).file_name()?.to_string_lossy().to_string();
    Game::ALL.into_iter().find(|g| leaf.eq_ignore_ascii_case(g.profile().user_dir))
}

pub fn load(app: &AppHandle) -> anyhow::Result<AppConfig> {
    let path = config_path(app);
    let text = std::fs::read_to_string(path)?;
    // A truncated/corrupt file is an error, not a silent empty config: callers that
    // can rebuild one (see `load_or_detect`) get the chance to, instead of the app
    // coming up pointed at nothing.
    let mut cfg: AppConfig = serde_json::from_str(&text)?;

    // Recover the active game when the file doesn't name one.
    //
    // Builds that predate multi-game support don't have `activeGame`/`games` on their
    // `AppConfig` and don't set `deny_unknown_fields`, so they read such a config fine
    // but *rewrite it without those keys*. Running an older build once — a downgrade, a
    // second install, the shipped app alongside a dev build — therefore erases which
    // game was active while leaving `modsPath` pointing at that game's folder.
    //
    // Defaulting to MX Bikes there is actively harmful: a GP Bikes folder would be
    // driven as if it were an MX Bikes one. `activeGame` absent is not the same as
    // `activeGame: "mxb"`, so check the raw JSON rather than the deserialized value,
    // and re-derive the game from the folders instead of assuming.
    let names_game = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("activeGame").cloned())
        .is_some();
    if !names_game {
        if let Some(g) = infer_game(&cfg) {
            if g != cfg.active_game {
                log::warn!(
                    "config.json has no activeGame (an older build rewrote it) — folders \
                     look like {}, adopting that instead of the {} default",
                    g.profile().display,
                    cfg.active_game.profile().display,
                );
            }
            cfg.active_game = g;
        }
    }

    let cfg = migrate(cfg);
    crate::game::set_active(cfg.active_game);
    Ok(cfg)
}

/// Whether `path` is a folder the app can actually work from — the game's user folder
/// (the game keeps `profiles/` there, and `mods/` shows up as soon as anything is
/// installed), or a relocated mods tree itself.
///
/// Checking for those rather than just "the folder exists" keeps auto-detection from
/// quietly adopting an empty `Documents\PiBoSo\MX Bikes` for someone whose setup lives
/// elsewhere; they still get the setup screen to point us at it.
fn looks_like_mods_dir(path: &str) -> bool {
    let dir = Path::new(path.trim());
    if path.trim().is_empty() || !dir.is_dir() {
        return false;
    }
    // Case-insensitive: under Proton these can come back as `Mods`/`Profiles` on a
    // case-sensitive filesystem, and an exact match would reject a perfectly good folder.
    is_user_dir(dir)
        || crate::library::resolve_child(dir, "mods").is_dir()
        // `mxbikes.ini` can put the mods tree anywhere, and there is nothing above such a
        // folder to point at instead — so the tree itself has to be an acceptable answer.
        || crate::library::is_mods_tree(dir)
}

/// Whether `dir` is the game's *user* folder. `profiles/` is the marker: the game writes
/// it on first run and never puts one inside a mods tree, which is what makes it the one
/// signal that separates the two folders a picker offers.
fn is_user_dir(dir: &Path) -> bool {
    crate::library::resolve_child(dir, "profiles").is_dir()
}

/// The parent of `path` when the pick is the `mods` folder *inside the game's user
/// folder*, else `None`.
///
/// In a folder picker those two are one click apart, and getting it wrong used to be
/// invisible afterwards: every scan then looked under `<mods>/mods`, found nothing, and
/// the library came up empty next to a path that read perfectly reasonably in Settings.
///
/// What it must *not* do is climb out of a **relocated** tree. `mxbikes.ini`'s
/// `[mods] folder` lets a player keep their content anywhere — `C:\mods` is the shape
/// people actually use, because junctioning one rider paint into six model folders needs
/// short paths off OneDrive — and the folder above that is the drive root. Rewriting the
/// pick to `C:\` left the app writing `mxbapp_disabled` to a folder that needs admin and
/// handing FrostMod a mods folder that doesn't exist. So the climb is gated on the parent
/// actually being the user folder; anything else is adopted as the mods root it is, which
/// [`crate::library::mods_root`] then resolves.
fn climb_out_of_mods(path: &str) -> Option<PathBuf> {
    let dir = Path::new(path.trim());
    if path.trim().is_empty() || !dir.is_dir() {
        return None;
    }
    // Already the user folder: it holds the `mods`/`profiles` children itself.
    if is_user_dir(dir) || crate::library::resolve_child(dir, "mods").is_dir() {
        return None;
    }
    if !crate::library::is_mods_tree(dir) {
        return None;
    }
    dir.parent().filter(|p| is_user_dir(p)).map(Path::to_path_buf)
}

/// The saved config, or one built on the spot when the MX Bikes folder sits where it
/// normally does. Returns `None` only when there's genuinely nothing to go on — the
/// one case where the setup screen has something to ask.
///
/// The setup screen never gathered more than this: its default action just runs the
/// same detection. So when the config file goes missing — an app-data wipe, a config
/// written under a different Windows account, a failed write — the app re-detects and
/// carries on rather than walking the user through setup again on every launch.
pub fn load_or_detect(app: &AppHandle) -> Option<AppConfig> {
    if exists(app) {
        match load(app) {
            Ok(cfg) => return Some(cfg),
            Err(e) => log::warn!("config.json unreadable ({e:#}) — re-detecting"),
        }
    }

    let cfg = finalize(AppConfig::default());
    if !looks_like_mods_dir(&cfg.mods_path) {
        return None;
    }
    log::info!(
        "no usable config — auto-detected {} folder: {}",
        cfg.game().display,
        cfg.mods_path
    );
    if let Err(e) = save(app, &cfg) {
        log::warn!("couldn't save the auto-detected config: {e:#}");
    }
    Some(cfg)
}

pub fn save(app: &AppHandle, cfg: &AppConfig) -> anyhow::Result<()> {
    let path = config_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Never persist a `games` map that disagrees with the live folders: callers mutate
    // `mods_path` and friends directly (`set_mods_path`, `set_game_path`), and a stale
    // entry would resurrect an old folder the next time the user switched back.
    let mut cfg = cfg.clone();
    cfg.stash_active();
    std::fs::write(path, serde_json::to_string_pretty(&cfg)?)?;
    crate::game::set_active(cfg.active_game);
    Ok(())
}

pub fn finalize(mut cfg: AppConfig) -> AppConfig {
    let game = cfg.game();
    // Picked one level too deep — take the folder above `mods`, which is the one every
    // other path in the app is built from. Done here so it covers both ways in: first-run
    // setup (`create_config`) and Change… in Settings (`set_mods_path`).
    if let Some(up) = climb_out_of_mods(&cfg.mods_path) {
        log::info!(
            "{} folder was set to the mods folder ({}) — using the folder above it: {}",
            game.display,
            cfg.mods_path,
            up.display()
        );
        cfg.mods_path = up.to_string_lossy().into_owned();
    }
    // A folder that isn't there any more is worse than none: it reads as "configured",
    // so the app comes up on a dashboard scanning nothing instead of asking where the
    // game went. Covers a game uninstalled or moved, and a path a previous build adopted
    // without checking. Only reached on a game switch or a fresh setup — never on plain
    // startup — so a drive that happens to be offline right now isn't forgotten.
    if !cfg.mods_path.trim().is_empty() && !Path::new(cfg.mods_path.trim()).is_dir() {
        log::info!("saved {} folder is gone ({}) — re-detecting", game.display, cfg.mods_path);
        cfg.mods_path.clear();
    }
    if cfg.mods_path.trim().is_empty() {
        // On Linux the game runs under Proton and writes into the Wine prefix, not the
        // user's real Documents — `default_user_dir` checks there first, since
        // `document_dir()` would otherwise hand back a path the game has never written
        // to (and often `None`, because it depends on `~/.config/user-dirs.dirs`).
        //
        // Only adopt it if the folder is actually there. `default_user_dir` builds a path
        // whether or not it exists, so switching to a title you don't have installed used
        // to leave the app pointed at a folder that was never created — and, because the
        // path was non-empty, showing a full (empty) dashboard instead of the setup
        // screen. Existence, not `looks_like_mods_dir`: a game installed but never
        // launched has the folder without the `mods`/`profiles` children yet, and that is
        // still the right folder to use.
        if let Some(dir) = default_user_dir(game).filter(|d| d.is_dir()) {
            cfg.mods_path = dir.to_string_lossy().into_owned();
        }
    }
    // Auto-detect the Steam game install so the 3D preview works out of the box. Only
    // fills a blank — never overrides a manual pick. Before the mods folder is settled,
    // because the game's own `.ini` lives in there and is what names a relocated one.
    if cfg.game_path.trim().is_empty() {
        if let Some(gp) = detect_game_path(game) {
            cfg.game_path = gp;
        }
    }
    adopt_relocated_mods_folder(&mut cfg);
    cfg.stash_active();
    cfg
}

/// Follow `mxbikes.ini`'s `[mods] folder` when the player has pointed the game at a mods
/// tree somewhere else.
///
/// Auto-detection alone lands on `Documents\PiBoSo\MX Bikes` because that folder exists —
/// it holds `profiles/` — and then every scan looks in its `mods` child, which for these
/// players is empty or absent. The library comes up missing gear, paints and bikes with
/// no hint as to why. The game already knows the answer; this reads it.
///
/// Only ever *replaces a folder whose `mods` child isn't there*: a player whose content is
/// where the app already found it is never second-guessed, and neither is one who picked a
/// folder by hand and has content in it. The profiles folder is pinned at the same time,
/// because the game has no redirect for profiles — they stay behind in the user folder,
/// and after this `mods_path` no longer points anywhere near them.
fn adopt_relocated_mods_folder(cfg: &mut AppConfig) {
    let user_dir = cfg.mods_path.trim().to_string();
    if user_dir.is_empty() || crate::library::resolve_child(Path::new(&user_dir), "mods").is_dir() {
        return;
    }
    let game = cfg.game();
    let Some(relocated) = ini_mods_folder(game, &cfg.game_path, &user_dir) else {
        return;
    };
    let relocated = relocated.to_string_lossy().into_owned();
    if relocated == user_dir {
        return;
    }
    log::info!(
        "{}'s {} points its mods folder at {relocated} — using that instead of {user_dir}",
        game.display,
        game_ini_name(game),
    );
    cfg.mods_path = relocated;
    // `profiles/` doesn't move with the mods tree, and the folder we just left is where it
    // stayed. Recording it beats re-deriving it later: `profiles_dir`'s stock fallback
    // assumes `Documents\PiBoSo\<game>`, which is only right while nothing was moved.
    let profiles = crate::library::resolve_child(Path::new(&user_dir), "profiles");
    if cfg.profiles_path.trim().is_empty() && profiles.is_dir() {
        cfg.profiles_path = profiles.to_string_lossy().into_owned();
    }
}

/// The game's own config file — `mxbikes.exe` → `mxbikes.ini`. Derived rather than listed
/// per title so a new `GameProfile` can't forget it.
fn game_ini_name(game: &GameProfile) -> String {
    let stem = game.exe.strip_suffix(".exe").unwrap_or(game.exe);
    format!("{stem}.ini")
}

/// The mods folder named by the game's `.ini`, if it names one that's really there.
///
/// Looked for beside the executable first (where the stock file ships) and then in the
/// user folder. Nothing here is trusted blind: the value is only used when it resolves to
/// a folder that is actually a mods tree, so a key we've misread, a stale path, or an
/// offline drive all fall through to the folder we already had.
fn ini_mods_folder(game: &GameProfile, game_path: &str, user_dir: &str) -> Option<PathBuf> {
    let name = game_ini_name(game);
    [game_path.trim(), user_dir.trim()]
        .into_iter()
        .filter(|d| !d.is_empty())
        .map(|d| crate::library::resolve_child(Path::new(d), &name))
        .filter(|p| p.is_file())
        .find_map(|ini| {
            let raw = parse_ini_mods_folder(&std::fs::read(&ini).ok()?)?;
            // A relative value is relative to the `.ini` itself, which is how the game
            // reads it — `folder = mods` in the install folder means the one beside it.
            let candidate = match Path::new(&raw).is_absolute() {
                true => PathBuf::from(&raw),
                false => ini.parent()?.join(&raw),
            };
            crate::library::is_mods_tree(&candidate).then_some(candidate)
        })
}

/// Pull `[mods] folder = …` out of a PiBoSo `.ini`.
///
/// Their files are Windows-1252 and `[section]`/`key = value`, with `;` comments. Both a
/// `folder` key under `[mods]` and a bare `mods = …` are accepted — the section layout is
/// read off the binary's string table rather than documented anywhere, so the reader is
/// deliberately forgiving. It costs nothing: [`ini_mods_folder`] throws away anything that
/// doesn't resolve to a real mods tree.
fn parse_ini_mods_folder(bytes: &[u8]) -> Option<String> {
    // Latin-1: lossless byte→char, same as the profile reader. `read_to_string` would
    // fail outright on an accented path.
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let mut section = String::new();
    let mut fallback = None;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_ascii_lowercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().trim_matches('"').trim());
        if v.is_empty() {
            continue;
        }
        if section == "mods" && k == "folder" {
            return Some(v.to_string());
        }
        if k == "mods" {
            fallback.get_or_insert_with(|| v.to_string());
        }
    }
    fallback
}

/// The game's user folder inside a Wine prefix.
///
/// Wherever the game runs as a Windows process — Proton on Linux, CrossOver/Whisky on
/// macOS — `Documents` is the prefix's fake `C:` drive, and nothing is ever written to the
/// user's real `~/Documents`. Without this a player lands on the setup screen with no
/// working default to accept.
///
/// Returns the first prefix that actually looks like a mods dir, so a stale prefix from an
/// uninstalled copy can't win over a real one.
fn detect_prefix_mods_path(game: &GameProfile) -> Option<String> {
    wine_prefixes(game)
        .iter()
        .find_map(|prefix| mods_dir_in_prefix(prefix, game))
}

/// Wine prefixes that could hold the game's user folder: Proton's per-game `compatdata`
/// on Linux, every CrossOver/Whisky/Wine bottle on macOS. Empty on Windows, where the
/// game is native and writes to the real `Documents`.
fn wine_prefixes(game: &GameProfile) -> Vec<PathBuf> {
    let mut prefixes: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "linux") {
        for lib in steam_libraries() {
            prefixes.push(
                lib.join("steamapps")
                    .join("compatdata")
                    .join(game.steam_appid)
                    .join("pfx"),
            );
        }
    }
    prefixes.extend(crate::winehost::bottles());
    prefixes
}

/// `<prefix>/drive_c/users/<user>/Documents/PiBoSo/<game>`, if it's really there.
///
/// The Windows user varies by wrapper — `steamuser` under Proton, `crossover` under
/// CrossOver, the login name under plain Wine — so every user in the prefix is tried
/// rather than guessing which one built it.
fn mods_dir_in_prefix(prefix: &Path, game: &GameProfile) -> Option<String> {
    let users = std::fs::read_dir(prefix.join("drive_c").join("users")).ok()?;
    users.flatten().find_map(|user| {
        let candidate = user
            .path()
            .join("Documents")
            .join("PiBoSo")
            .join(game.user_dir);
        let as_str = candidate.to_string_lossy().into_owned();
        looks_like_mods_dir(&as_str).then_some(as_str)
    })
}

/// Locate the game's install folder (the one holding its `install_marker`) by scanning
/// Steam libraries. Returns `None` when it can't be found (e.g. non-Steam install —
/// GP Bikes is also sold directly by PiBoSo, so this missing is unremarkable there).
pub fn detect_game_path(game: &GameProfile) -> Option<String> {
    for lib in steam_libraries() {
        let dir = lib.join("steamapps").join("common").join(game.steam_common);
        // Case-tolerant: a case-sensitive filesystem can hold `GPBikes.exe`.
        if crate::library::resolve_child(&dir, game.install_marker).is_file() {
            return Some(dir.to_string_lossy().into_owned());
        }
    }
    None
}

/// Candidate Steam library roots: the default install locations plus any extra
/// libraries registered in `steamapps/libraryfolders.vdf`.
fn steam_libraries() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |roots: &mut Vec<PathBuf>, p: PathBuf| {
        if !roots.contains(&p) {
            roots.push(p);
        }
    };

    #[cfg(windows)]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(pf) = std::env::var(var) {
                push(&mut roots, PathBuf::from(pf).join("Steam"));
            }
        }
        for drive in ['C', 'D', 'E', 'F'] {
            push(&mut roots, PathBuf::from(format!("{drive}:\\Program Files (x86)\\Steam")));
            push(&mut roots, PathBuf::from(format!("{drive}:\\Steam")));
            push(&mut roots, PathBuf::from(format!("{drive}:\\SteamLibrary")));
        }
    }

    #[cfg(not(windows))]
    {
        // Steam on macOS/Linux — lets the detector run (and tests exercise it) off-Windows.
        if let Some(home) = dirs_next::home_dir() {
            push(&mut roots, home.join("Library/Application Support/Steam"));
            push(&mut roots, home.join(".steam/steam"));
            push(&mut roots, home.join(".steam/root"));
            push(&mut roots, home.join(".local/share/Steam"));
            // Flatpak and snap Steam keep their own home, so the paths above miss them
            // entirely — which for a Flatpak user means no detection at all.
            push(
                &mut roots,
                home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
            );
            push(&mut roots, home.join("snap/steam/common/.local/share/Steam"));
        }
        // macOS: the game is a Windows title, so a Steam that owns it is a *Windows* Steam
        // installed inside a Wine bottle. The native paths above can never hold it.
        for bottle in crate::winehost::bottles() {
            let c = bottle.join("drive_c");
            push(&mut roots, c.join("Program Files (x86)/Steam"));
            push(&mut roots, c.join("Program Files/Steam"));
        }
    }

    // Extra libraries the user added on other drives, per libraryfolders.vdf.
    for root in roots.clone() {
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for lib in parse_library_paths(&text) {
                push(&mut roots, PathBuf::from(lib));
            }
        }
    }

    roots
}

/// Pull the `"path"  "..."` values out of a Steam `libraryfolders.vdf`.
fn parse_library_paths(vdf: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in vdf.lines() {
        let rest = match line.trim().strip_prefix("\"path\"") {
            Some(r) => r,
            None => continue,
        };
        let start = match rest.find('"') {
            Some(i) => i + 1,
            None => continue,
        };
        if let Some(len) = rest[start..].find('"') {
            // VDF escapes backslashes; normalize `\\` back to `\`.
            out.push(rest[start..start + len].replace("\\\\", "\\"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole compatibility contract for existing installs: a `config.json` written
    /// before GP Bikes support has no `activeGame` and no `games`, and must come back as
    /// an MX Bikes config pointing at exactly the folders it named.
    #[test]
    fn a_pre_multi_game_config_migrates_to_mx_bikes() {
        let json = r#"{
            "modsPath": "/games/MX Bikes",
            "gamePath": "/steam/common/MX Bikes",
            "profilesPath": "",
            "runInBackground": true,
            "overlayHotkey": "CommandOrControl+Shift+X"
        }"#;
        let cfg = migrate(serde_json::from_str::<AppConfig>(json).unwrap());

        assert_eq!(cfg.active_game, Game::Mxb, "an old config is an MX Bikes one");
        assert_eq!(cfg.mods_path, "/games/MX Bikes", "folders are untouched");
        assert_eq!(cfg.game_path, "/steam/common/MX Bikes");
        // ...and they've been seeded into the map, so switching away and back keeps them.
        assert_eq!(
            cfg.games.get("mxb").map(|g| g.mods_path.as_str()),
            Some("/games/MX Bikes"),
        );
    }

    /// Switching parks the outgoing game's folders and restores the incoming one's, so a
    /// player who set both up once never has to pick them again.
    #[test]
    fn switching_games_parks_and_restores_folders() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/mx/mods".into();
        cfg.game_path = "/mx/install".into();

        assert!(cfg.switch_game(Game::Gpb), "a real switch reports the change");
        assert_eq!(cfg.active_game, Game::Gpb);
        assert_eq!(cfg.mods_path, "", "GP Bikes has nothing saved yet");

        cfg.mods_path = "/gp/mods".into();
        cfg.game_path = "/gp/install".into();

        assert!(cfg.switch_game(Game::Mxb));
        assert_eq!(cfg.mods_path, "/mx/mods", "MX Bikes' folders came back");
        assert_eq!(cfg.game_path, "/mx/install");

        assert!(cfg.switch_game(Game::Gpb));
        assert_eq!(cfg.mods_path, "/gp/mods", "and so did GP Bikes'");
    }

    /// Re-picking the active game must be a no-op, not a round-trip that could clobber
    /// folders the user just edited.
    #[test]
    fn switching_to_the_active_game_changes_nothing() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/mx/mods".into();
        assert!(!cfg.switch_game(Game::Mxb));
        assert_eq!(cfg.mods_path, "/mx/mods");
    }

    /// Switching to a title that isn't installed must leave the folder blank, because a
    /// blank folder is what puts the setup screen up. `default_user_dir` happily builds a
    /// path for a game that was never installed, and adopting it left the app pointed at
    /// a folder that doesn't exist — showing a full, empty dashboard instead of asking
    /// where the game is.
    #[test]
    fn a_game_that_isnt_installed_leaves_the_folder_blank() {
        let missing = std::env::temp_dir().join(format!("frost-no-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);

        let mut cfg = AppConfig::default();
        cfg.active_game = Game::Gpb;
        // Stand in for `default_user_dir` returning a path that was never created.
        assert!(!missing.is_dir(), "precondition: the folder isn't there");
        assert_eq!(
            Some(missing.clone()).filter(|d| d.is_dir()),
            None,
            "a non-existent default is not adopted",
        );

        // And the real thing: with nothing installed, `finalize` must not invent a path.
        let out = finalize(cfg);
        assert!(
            out.mods_path.is_empty() || Path::new(&out.mods_path).is_dir(),
            "either blank, or a folder that actually exists — got {:?}",
            out.mods_path,
        );
    }

    /// A released build with no knowledge of `activeGame` rewrites `config.json` without
    /// it, leaving folders that belong to one game and no record of which. Defaulting to
    /// MX Bikes there would drive a GP Bikes folder as an MX Bikes one, so the game is
    /// re-derived from the folders.
    #[test]
    fn a_config_stripped_of_its_game_is_recovered_from_the_folders() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/Users/x/Documents/PiBoSo/GP Bikes".into();
        assert_eq!(infer_game(&cfg), Some(Game::Gpb), "the user folder names the game");

        cfg.mods_path = "/Users/x/Documents/PiBoSo/MX Bikes".into();
        assert_eq!(infer_game(&cfg), Some(Game::Mxb));

        // Trailing separators and case shouldn't matter.
        cfg.mods_path = "/Users/x/Documents/PiBoSo/gp bikes/".into();
        assert_eq!(infer_game(&cfg), Some(Game::Gpb));
    }

    /// The install folder is the stronger signal, so it wins over a mods folder whose
    /// name says otherwise — that's the moved-mods-folder case.
    #[test]
    fn the_executable_outranks_the_folder_name_when_inferring() {
        let root = std::env::temp_dir().join(format!("frost-infer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(crate::game::GPB.exe), b"stub").unwrap();

        let mut cfg = AppConfig::default();
        cfg.game_path = root.to_string_lossy().into_owned();
        cfg.mods_path = "/somewhere/custom/my mods".into();
        assert_eq!(infer_game(&cfg), Some(Game::Gpb), "gpbikes.exe is conclusive");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Nothing to go on must stay `None` so the caller keeps its existing value rather
    /// than being handed a guess.
    #[test]
    fn inference_gives_up_rather_than_guessing() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/somewhere/custom/mods".into();
        assert_eq!(infer_game(&cfg), None);
        assert_eq!(infer_game(&AppConfig::default()), None, "blank config");
    }

    /// Each title resolves its own `Documents\PiBoSo\<game>` folder.
    ///
    /// `default_user_dir` is `None` when the host has no Documents folder to build on —
    /// which is the case on a headless CI runner, where `dirs_next::document_dir()` reads
    /// `~/.config/user-dirs.dirs` and finds nothing. That's a property of the runner, not
    /// a failure, so the assertion is on the *shape* of a path when there is one.
    #[test]
    fn each_game_has_its_own_user_folder() {
        let (Some(mxb), Some(gpb)) =
            (default_user_dir(&crate::game::MXB), default_user_dir(&crate::game::GPB))
        else {
            return; // no Documents folder on this host — nothing to assert about
        };
        assert!(mxb.ends_with("PiBoSo/MX Bikes"), "{}", mxb.display());
        assert!(gpb.ends_with("PiBoSo/GP Bikes"), "{}", gpb.display());
        assert_ne!(mxb, gpb, "the two titles must not share a folder");
    }

    #[test]
    fn profiles_dir_defaults_to_mods_subfolder() {
        let root = std::env::temp_dir().join(format!("frost-profiles-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("profiles")).unwrap();

        let mut cfg = AppConfig::default();
        cfg.mods_path = root.to_string_lossy().into_owned();
        assert_eq!(cfg.profiles_dir(), root.join("profiles"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Picking `…/MX Bikes/mods` instead of `…/MX Bikes` — the folder one level down, and
    /// the one a player is far more likely to have open when the picker appears.
    #[test]
    fn finalize_climbs_out_of_the_mods_folder() {
        let root = std::env::temp_dir().join(format!("frost-climb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("MX Bikes");
        std::fs::create_dir_all(game.join("mods").join("bikes")).unwrap();
        std::fs::create_dir_all(game.join("profiles")).unwrap();

        let mut cfg = AppConfig::default();
        cfg.mods_path = game.join("mods").to_string_lossy().into_owned();
        let out = finalize(cfg);
        assert_eq!(Path::new(&out.mods_path), game);

        // A tree under any other name climbs too, as long as the folder above it is
        // really the game's — `profiles/` is what says so.
        let odd = root.join("MX Bikes 2");
        std::fs::create_dir_all(odd.join("Mods_backup").join("tracks")).unwrap();
        std::fs::create_dir_all(odd.join("profiles")).unwrap();
        let mut cfg = AppConfig::default();
        cfg.mods_path = odd.join("Mods_backup").to_string_lossy().into_owned();
        assert_eq!(Path::new(&finalize(cfg).mods_path), odd);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The case this whole resolver exists for: `mxbikes.ini` puts the mods tree on its own
    /// at `C:\mods`, and the folder above it is the drive root. Climbing there used to
    /// rewrite the pick to `C:\`, which then had the app writing `mxbapp_disabled` to a
    /// folder that needs admin and handing FrostMod a mods folder that isn't there.
    ///
    /// The pick must survive exactly as given, and resolve to itself.
    #[test]
    fn a_relocated_mods_tree_is_kept_not_climbed_out_of() {
        let root = std::env::temp_dir().join(format!("frost-reloc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Stands in for `C:\mods`: a mods tree whose parent is not a game folder.
        let tree = root.join("mods");
        std::fs::create_dir_all(tree.join("rider").join("helmets")).unwrap();
        std::fs::create_dir_all(tree.join("tracks")).unwrap();

        let mut cfg = AppConfig::default();
        cfg.mods_path = tree.to_string_lossy().into_owned();
        let out = finalize(cfg);
        assert_eq!(Path::new(&out.mods_path), tree, "the pick is kept as-is");

        // ...and every content lookup lands inside it rather than in a `mods` child.
        assert_eq!(crate::library::mods_root(&out.mods_path), tree);
        assert_eq!(
            crate::library::mods_subdir(&out.mods_path, "mods/rider"),
            tree.join("rider"),
        );

        // A tree that hasn't had anything installed into it yet is recognised by name
        // alone — otherwise a freshly relocated folder can't be picked at all.
        let empty = root.join("elsewhere").join("mods");
        std::fs::create_dir_all(&empty).unwrap();
        let mut cfg = AppConfig::default();
        cfg.mods_path = empty.to_string_lossy().into_owned();
        assert_eq!(Path::new(&finalize(cfg).mods_path), empty);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The right folder must survive untouched, including the one case that looks like the
    /// wrong one: a game folder that is itself *named* `mods`.
    #[test]
    fn finalize_leaves_the_game_folder_alone() {
        let root = std::env::temp_dir().join(format!("frost-noclimb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for game in [root.join("MX Bikes"), root.join("mods")] {
            std::fs::create_dir_all(game.join("mods").join("bikes")).unwrap();
            std::fs::create_dir_all(game.join("profiles")).unwrap();
            let mut cfg = AppConfig::default();
            cfg.mods_path = game.to_string_lossy().into_owned();
            assert_eq!(Path::new(&finalize(cfg).mods_path), game, "{}", game.display());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn profiles_dir_uses_override_when_set() {
        let mut cfg = AppConfig::default();
        cfg.mods_path = "/games/mxb".into();
        cfg.profiles_path = "/other/drive/profiles".into();
        assert_eq!(cfg.profiles_dir(), PathBuf::from("/other/drive/profiles"));
    }

    /// The `mxbikes.ini` split-layout case: mods on another drive, profiles still in
    /// `Documents`. The primary folder doesn't exist, so the stock one takes over —
    /// but only then, and only if it's really there.
    #[test]
    fn profiles_dir_falls_back_to_the_stock_folder_when_the_mods_one_is_missing() {
        let root = std::env::temp_dir().join(format!("frost-profiles-fb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let relocated = root.join("A/MX Bikes/profiles");
        let stock = root.join("Documents/PiBoSo/MX Bikes/profiles");
        std::fs::create_dir_all(&stock).unwrap();

        // Primary missing, fallback present → fallback.
        assert_eq!(
            resolve_profiles_dir(relocated.clone(), || Some(stock.clone())),
            stock
        );

        // Primary present → it wins, and the fallback isn't even worked out.
        std::fs::create_dir_all(&relocated).unwrap();
        assert_eq!(
            resolve_profiles_dir(relocated.clone(), || {
                panic!("fallback must not be consulted when the primary is there")
            }),
            relocated
        );

        // Neither exists → keep the primary, so the UI names the folder they picked.
        let nowhere = root.join("gone/profiles");
        assert_eq!(
            resolve_profiles_dir(nowhere.clone(), || Some(root.join("also-gone"))),
            nowhere
        );
        assert_eq!(resolve_profiles_dir(nowhere.clone(), || None), nowhere);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Picking `…\MX Bikes\mods` on an install the game has never run: no `profiles/`
    /// inside the tree to find, and none in `Documents` either, but the right folder is
    /// one level up and about to be written. Without this step the app pointed at a
    /// profiles folder on the wrong drive entirely.
    #[test]
    fn profiles_are_found_beside_a_mods_tree() {
        let root = std::env::temp_dir().join(format!("frost-profiles-beside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("D_Games").join("MX Bikes");
        std::fs::create_dir_all(game.join("mods").join("bikes")).unwrap();
        std::fs::create_dir_all(game.join("profiles").join("Player")).unwrap();

        let mut cfg = AppConfig::default();
        cfg.mods_path = game.join("mods").to_string_lossy().into_owned();
        assert_eq!(cfg.profiles_dir(), game.join("profiles"));

        // A tree with no usable parent (the `C:\mods` shape) must not reach for a
        // drive-root `profiles` — it falls through to the stock folder instead.
        let mut cfg = AppConfig::default();
        cfg.mods_path = root.join("mods").to_string_lossy().into_owned();
        std::fs::create_dir_all(root.join("mods").join("tracks")).unwrap();
        assert_ne!(cfg.profiles_dir(), root.join("profiles"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `mxbikes.ini`'s `[mods] folder` is the game's own answer to "where is my content",
    /// and for a split setup it is the *only* one — auto-detection lands on
    /// `Documents\PiBoSo\MX Bikes` because `profiles/` is there, then finds no `mods`
    /// child and comes up empty with no explanation.
    #[test]
    fn a_relocated_mods_folder_is_read_out_of_the_games_ini() {
        let root = std::env::temp_dir().join(format!("frost-ini-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let install = root.join("Steam").join("MX Bikes");
        let user = root.join("Documents").join("PiBoSo").join("MX Bikes");
        let tree = root.join("mods");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(user.join("profiles")).unwrap();
        std::fs::create_dir_all(tree.join("rider")).unwrap();
        std::fs::write(
            install.join("mxbikes.ini"),
            format!("[master]\nserver = master.mx-bikes.com:54200\n\n[mods]\nfolder = {}\n", tree.display()),
        )
        .unwrap();

        let mut cfg = AppConfig::default();
        cfg.mods_path = user.to_string_lossy().into_owned();
        cfg.game_path = install.to_string_lossy().into_owned();
        let out = finalize(cfg);

        assert_eq!(Path::new(&out.mods_path), tree, "the ini's folder wins");
        // ...and profiles, which don't move with it, are pinned to where they stayed.
        assert_eq!(out.profiles_dir(), user.join("profiles"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A folder that already has content in it is never second-guessed, however the ini
    /// reads — someone who moved back, or who runs two setups, keeps what they picked.
    #[test]
    fn the_ini_never_overrides_a_folder_that_already_has_mods() {
        let root = std::env::temp_dir().join(format!("frost-ini-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let user = root.join("MX Bikes");
        let tree = root.join("elsewhere").join("mods");
        std::fs::create_dir_all(user.join("mods").join("bikes")).unwrap();
        std::fs::create_dir_all(user.join("profiles")).unwrap();
        std::fs::create_dir_all(tree.join("tracks")).unwrap();
        std::fs::write(
            user.join("mxbikes.ini"),
            format!("[mods]\nfolder = {}\n", tree.display()),
        )
        .unwrap();

        let mut cfg = AppConfig::default();
        cfg.mods_path = user.to_string_lossy().into_owned();
        assert_eq!(Path::new(&finalize(cfg).mods_path), user);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The section layout is read off the binary's string table rather than any published
    /// spec, so the parser is forgiving — and everything it returns is checked against the
    /// filesystem before it's used, which is what makes forgiving safe.
    #[test]
    fn parses_the_mods_folder_out_of_a_piboso_ini() {
        let ini = b"; comment\n[master]\nserver = master.mx-bikes.com:54200\n\n[mods]\nfolder = C:\\mods   ; where my stuff is\n";
        assert_eq!(parse_ini_mods_folder(ini), Some("C:\\mods".to_string()));

        // Quoted, and a bare `mods =` outside any section.
        assert_eq!(
            parse_ini_mods_folder(b"[mods]\nfolder = \"D:\\My Mods\"\n"),
            Some("D:\\My Mods".to_string()),
        );
        assert_eq!(parse_ini_mods_folder(b"mods = E:\\stuff\n"), Some("E:\\stuff".to_string()));

        // A `folder` key belonging to some other section is not ours.
        assert_eq!(parse_ini_mods_folder(b"[replay]\nfolder = C:\\replays\n"), None);
        // Nothing to say, an empty value, and a non-UTF-8 file must all be survivable.
        assert_eq!(parse_ini_mods_folder(b"[mods]\nfolder =\n"), None);
        assert_eq!(parse_ini_mods_folder(b""), None);
        assert_eq!(
            parse_ini_mods_folder(b"[mods]\nfolder = C:\\mods\xe9\n"),
            Some("C:\\modsé".to_string()),
            "Windows-1252, like every other file these games write",
        );
    }

    /// A value that doesn't resolve to a real mods tree is thrown away rather than
    /// adopted, so a misread key or a stale path costs nothing.
    #[test]
    fn an_ini_pointing_nowhere_is_ignored() {
        let root = std::env::temp_dir().join(format!("frost-ini-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let install = root.join("install");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::write(install.join("mxbikes.ini"), "[mods]\nfolder = Z:\\gone\n").unwrap();
        let gone = install.to_string_lossy().into_owned();
        assert_eq!(ini_mods_folder(&crate::game::MXB, &gone, ""), None);

        // A folder that exists but holds nothing of ours is not a mods tree either.
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::write(
            install.join("mxbikes.ini"),
            format!("[mods]\nfolder = {}\n", bare.display()),
        )
        .unwrap();
        assert_eq!(ini_mods_folder(&crate::game::MXB, &gone, ""), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Derived from the executable so a new `GameProfile` can't forget it.
    #[test]
    fn each_game_looks_for_its_own_ini() {
        assert_eq!(game_ini_name(&crate::game::MXB), "mxbikes.ini");
        assert_eq!(game_ini_name(&crate::game::GPB), "gpbikes.ini");
    }

    #[test]
    fn only_a_real_mx_bikes_folder_is_adopted_automatically() {
        let root = std::env::temp_dir()
            .join(format!("frost-config-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        assert!(!looks_like_mods_dir(""));
        assert!(!looks_like_mods_dir(&root.join("nope").to_string_lossy()));

        // The folder exists but holds nothing of ours — don't assume it's the one.
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        assert!(!looks_like_mods_dir(&bare.to_string_lossy()));

        // `profiles/` (the game writes it) or `mods/` (we do) makes it recognizable.
        let played = root.join("played");
        std::fs::create_dir_all(played.join("profiles")).unwrap();
        assert!(looks_like_mods_dir(&played.to_string_lossy()));

        let modded = root.join("modded");
        std::fs::create_dir_all(modded.join("mods")).unwrap();
        assert!(looks_like_mods_dir(&modded.to_string_lossy()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn older_configs_keep_their_settings_and_default_the_new_flags() {
        // A config written before the intro flags existed must still load — a parse
        // failure now makes the app re-detect and overwrite it.
        let cfg: AppConfig = serde_json::from_str(
            r#"{"modsPath":"C:\\MXB","gamePath":"C:\\Steam\\MX Bikes","runInBackground":false}"#,
        )
        .expect("older config still parses");
        assert_eq!(cfg.mods_path, "C:\\MXB");
        assert_eq!(cfg.game_path, "C:\\Steam\\MX Bikes");
        assert!(!cfg.run_in_background);
        assert!(cfg.launch_at_startup, "unset fields fall back to the defaults");
        assert!(!cfg.welcome_seen);
        assert!(!cfg.tour_done);
        assert!(
            cfg.seen_version.is_empty(),
            "a config from before the showcase has nothing stamped, so the newest one is due",
        );
    }

    /// The retired default was Discord's mute toggle, so an install still carrying it
    /// has an overlay that silently never binds.
    #[test]
    fn the_retired_default_hotkey_moves_to_the_current_one() {
        let mut cfg = AppConfig::default();
        cfg.overlay_hotkey = "CommandOrControl+Shift+M".into();
        assert_eq!(migrate(cfg).overlay_hotkey, DEFAULT_OVERLAY_HOTKEY);
    }

    #[test]
    fn a_hotkey_the_player_picked_survives_migration() {
        let mut cfg = AppConfig::default();
        cfg.overlay_hotkey = "Alt+F1".into();
        assert_eq!(migrate(cfg).overlay_hotkey, "Alt+F1");
    }

    /// Blank means "use the default" (see `overlay::hotkey_of`) — filling it in here
    /// would turn a follow-the-default config into a pinned one.
    #[test]
    fn a_blank_hotkey_is_left_blank() {
        let mut cfg = AppConfig::default();
        cfg.overlay_hotkey = String::new();
        assert!(migrate(cfg).overlay_hotkey.is_empty());
    }

    #[test]
    fn corrupt_config_is_an_error_not_an_empty_config() {
        assert!(serde_json::from_str::<AppConfig>(r#"{"modsPath":"C:\\MX"#).is_err());
    }

    #[test]
    fn parses_library_paths_from_vdf() {
        let vdf = r#"
"libraryfolders"
{
    "0"
    {
        "path"        "C:\\Program Files (x86)\\Steam"
    }
    "1"
    {
        "path"        "D:\\SteamLibrary"
    }
}
"#;
        assert_eq!(
            parse_library_paths(vdf),
            vec![
                "C:\\Program Files (x86)\\Steam".to_string(),
                "D:\\SteamLibrary".to_string(),
            ]
        );
    }
}

#[cfg(test)]
mod prefix_paths_tests {
    use super::*;

    /// A Proton prefix laid out the way Steam actually makes one, checked through the
    /// same `looks_like_mods_dir` gate detection uses. Runs on every OS: the layout is
    /// what's under test, not the host.
    #[test]
    fn recognises_a_proton_prefix_layout() {
        let root = std::env::temp_dir().join(format!("frost-proton-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mods = root
            .join("steamapps/compatdata")
            .join(crate::game::MXB.steam_appid)
            .join("pfx/drive_c/users/steamuser/Documents/PiBoSo/MX Bikes");
        std::fs::create_dir_all(mods.join("mods").join("bikes")).unwrap();
        std::fs::create_dir_all(mods.join("profiles")).unwrap();

        assert!(
            looks_like_mods_dir(&mods.to_string_lossy()),
            "the prefix's MX Bikes folder is a valid mods dir"
        );
        // And the path the app builds from a library root matches where Steam put it.
        let built = root
            .join("steamapps")
            .join("compatdata")
            .join(crate::game::MXB.steam_appid)
            .join("pfx/drive_c/users/steamuser/Documents/PiBoSo/MX Bikes");
        assert_eq!(built, mods);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prefix_detection_never_runs_on_windows() {
        // On Windows the game is native and writes to the real Documents folder, so the
        // prefix probe must never hijack detection there.
        if cfg!(windows) {
            assert!(detect_prefix_mods_path(&crate::game::MXB).is_none());
        }
    }

    /// A CrossOver bottle's user folder, found without being told which Windows user
    /// built it — `crossover` here, `steamuser` under Proton, a login name under Wine.
    #[test]
    fn finds_the_user_folder_in_a_bottle_whatever_the_windows_user_is_called() {
        let root = std::env::temp_dir().join(format!("frost-bottle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let users = root.join("drive_c/users");
        // A user with nothing in it must not be mistaken for the one that has the game.
        std::fs::create_dir_all(users.join("Public")).unwrap();
        let mods = users.join("crossover/Documents/PiBoSo/MX Bikes");
        std::fs::create_dir_all(mods.join("mods").join("bikes")).unwrap();
        std::fs::create_dir_all(mods.join("profiles")).unwrap();

        assert_eq!(
            mods_dir_in_prefix(&root, &crate::game::MXB).map(PathBuf::from),
            Some(mods)
        );
        // Wrong title: GP Bikes isn't in this bottle.
        assert!(mods_dir_in_prefix(&root, &crate::game::GPB).is_none());
        // Not a prefix at all.
        assert!(mods_dir_in_prefix(&root.join("nope"), &crate::game::MXB).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }
}
