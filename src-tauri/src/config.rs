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
    pub mods_path: String,
    /// Active game's install dir (its executable + core archives); distinct from
    /// `mods_path`.
    pub game_path: String,
    /// Override for the PiBoSo `profiles` folder. Empty (the normal case) means it's
    /// resolved from `mods_path` — see [`AppConfig::profiles_dir`]. Set it when the
    /// resolver can't find the folder, e.g. profiles on a drive we don't probe.
    pub profiles_path: String,
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
    /// code. Empty until they enrol.
    pub cp_token: String,
    /// The in-game rider name this account enrolled with. Kept so the UI can show which
    /// identity the paints are published under.
    pub cp_rider_name: String,
    /// This player's MX Bikes GUID, once claimed. The stable identity the roster keys on —
    /// rider names are free text and change between sessions.
    pub cp_guid: String,
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
    /// — the normal, combined layout — falling back to the stock
    /// `Documents\PiBoSo\MX Bikes\profiles` when that folder doesn't exist.
    ///
    /// The fallback is not exotic: `mxbikes.ini` lets a player point the *mods* folder
    /// at another drive, and the game has no equivalent redirect for profiles, so it
    /// keeps writing them to `Documents`. Anyone who uses that documented feature ends
    /// up with a split layout, and without this the app looked for profiles in a folder
    /// that never existed.
    pub fn profiles_dir(&self) -> PathBuf {
        let custom = self.profiles_path.trim();
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
        // Case-tolerant join: under Proton the folder can come back as `Profiles`.
        let primary = crate::library::resolve_child(Path::new(self.mods_path.trim()), "profiles");
        resolve_profiles_dir(primary, || {
            default_user_dir(self.game()).map(|d| d.join("profiles"))
        })
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

/// The game's user folder where it puts it when nothing has been moved: the Proton
/// prefix on Linux, `Documents\PiBoSo\<game>` elsewhere.
fn default_user_dir(game: &GameProfile) -> Option<PathBuf> {
    if let Some(p) = detect_proton_mods_path(game) {
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

/// Whether `path` is really a player's MX Bikes folder — the game keeps `profiles/`
/// there, and `mods/` shows up as soon as anything is installed. Checking for those
/// rather than just "the folder exists" keeps auto-detection from quietly adopting an
/// empty `Documents\PiBoSo\MX Bikes` for someone whose setup lives elsewhere; they
/// still get the setup screen to point us at it.
fn looks_like_mods_dir(path: &str) -> bool {
    let dir = Path::new(path.trim());
    if path.trim().is_empty() || !dir.is_dir() {
        return false;
    }
    // Case-insensitive: under Proton these can come back as `Mods`/`Profiles` on a
    // case-sensitive filesystem, and an exact match would reject a perfectly good folder.
    crate::library::resolve_child(dir, "profiles").is_dir()
        || crate::library::resolve_child(dir, "mods").is_dir()
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
    // fills a blank — never overrides a manual pick.
    if cfg.game_path.trim().is_empty() {
        if let Some(gp) = detect_game_path(game) {
            cfg.game_path = gp;
        }
    }
    cfg.stash_active();
    cfg
}

/// The game's user folder inside a Proton prefix.
///
/// Under Proton the game is a Windows process, so `Documents` is the prefix's fake
/// `C:` drive — `compatdata/<appid>/pfx/drive_c/users/steamuser/Documents/PiBoSo/<game>`
/// — and nothing is ever written to the user's real `~/Documents`. Without this a Linux
/// player lands on the setup screen with no working default to accept.
///
/// Returns the first prefix that actually looks like a mods dir, so a stale prefix from an
/// uninstalled copy can't win over a real one.
fn detect_proton_mods_path(game: &GameProfile) -> Option<String> {
    if cfg!(not(target_os = "linux")) {
        return None;
    }
    for lib in steam_libraries() {
        let candidate = lib
            .join("steamapps")
            .join("compatdata")
            .join(game.steam_appid)
            .join("pfx/drive_c/users/steamuser/Documents/PiBoSo")
            .join(game.user_dir);
        let as_str = candidate.to_string_lossy().into_owned();
        if looks_like_mods_dir(&as_str) {
            return Some(as_str);
        }
    }
    None
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
mod linux_paths_tests {
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
    fn proton_detection_is_linux_only() {
        // On Windows/macOS the game is native and writes to the real Documents folder,
        // so the prefix probe must never hijack detection there.
        if cfg!(not(target_os = "linux")) {
            assert!(detect_proton_mods_path(&crate::game::MXB).is_none());
        }
    }
}
