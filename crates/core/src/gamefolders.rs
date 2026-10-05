//! Keeping the two folders a game has pointed at the right places.
//!
//! A PiBoSo title lives in two folders that are easy to confuse: the **install** folder, which
//! holds the executable, its core archives and the `plugins` folder the game loads `.dlo` files
//! from (a Steam library, `…\steamapps\common\MX Bikes`), and the **user** folder,
//! `<Documents>\PiBoSo\<game>`, which holds `mods` and `profiles`. Mixing them up is silent:
//! a `Documents\PiBoSo` saved as the install folder still "exists", so FrostMod was copied into
//! a `plugins` folder the game never reads, and Play had to fall back to asking Steam.
//!
//! So both are checked rather than trusted:
//!
//! * an install folder is only an install folder when it holds the executable, and never when
//!   it is the user's Documents side; a bad one is replaced by what Steam says (every library
//!   in `libraryfolders.vdf`, the app manifest, the registry), or cleared;
//! * a saved user folder that has gone (moved out of OneDrive, say) is looked for again the way
//!   setup finds it — the Documents known folder, the registry, OneDrive, `%USERPROFILE%` —
//!   plus the old path with its OneDrive part taken out.

use crate::config::AppConfig;
use crate::docsdir::same_folder;
use crate::game::GameProfile;
use std::path::{Path, PathBuf};

/// Whether `dir` holds the game's executable (case-tolerant, for Proton's filesystems).
pub fn has_exe(dir: &Path, game: &GameProfile) -> bool {
    !dir.as_os_str().is_empty() && crate::library::resolve_child(dir, game.exe).is_file()
}

/// Folders that are the game's *user* side on this machine: `<docs>\PiBoSo` and
/// `<docs>\PiBoSo\<game>` for every Documents candidate, plus the saved mods folder and the
/// folder above it. None of them is ever an install folder.
pub fn user_side_folders(cfg: &AppConfig, docs: &[PathBuf]) -> Vec<PathBuf> {
    let game = cfg.game();
    let mut out: Vec<PathBuf> = Vec::new();
    for d in docs {
        let piboso = d.join("PiBoSo");
        out.push(piboso.join(game.user_dir));
        out.push(piboso);
    }
    let mods = cfg.mods_path.trim();
    if !mods.is_empty() {
        let mods = PathBuf::from(mods);
        if let Some(parent) = mods.parent().filter(|p| p.parent().is_some()) {
            out.push(parent.to_path_buf());
        }
        out.push(mods);
    }
    out
}

/// Whether `dir` is the user-data side rather than an install: one of `user_side`, or shaped
/// like it wherever Documents is — a `PiBoSo` folder, or a folder holding `profiles/` — and
/// without the executable in it.
pub fn is_user_data_folder(dir: &Path, game: &GameProfile, user_side: &[PathBuf]) -> bool {
    if user_side.iter().any(|u| same_folder(dir, u)) {
        return true;
    }
    if has_exe(dir, game) {
        return false;
    }
    let named_piboso = dir
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("PiBoSo"));
    named_piboso
        || crate::library::resolve_child(dir, "profiles").is_dir()
        || crate::library::resolve_child(dir, game.user_dir)
            .join("profiles")
            .is_dir()
}

/// Why `dir` can't be the install folder, or `None` when it can.
pub fn install_problem(dir: &Path, game: &GameProfile, user_side: &[PathBuf]) -> Option<String> {
    if dir.as_os_str().is_empty() {
        return Some("no install folder is set".into());
    }
    if is_user_data_folder(dir, game, user_side) {
        return Some(format!(
            "{} is {}'s user folder (mods and profiles), not where the game is installed",
            dir.display(),
            game.display
        ));
    }
    if !dir.is_dir() {
        return Some(format!("{} isn't there", dir.display()));
    }
    if !has_exe(dir, game) {
        return Some(format!("there is no {} in {}", game.exe, dir.display()));
    }
    None
}

/// The install folder a folder-picker choice means, or why it isn't one.
///
/// A pick one or two levels above the install — the Steam library, or its `common` folder —
/// is taken down to the game's own folder, since those are one click apart in a picker.
pub fn check_install_pick(
    picked: &str,
    game: &GameProfile,
    user_side: &[PathBuf],
) -> Result<PathBuf, String> {
    let picked = picked.trim();
    let dir = PathBuf::from(picked);
    let candidates = [
        dir.clone(),
        dir.join(game.steam_common),
        dir.join("common").join(game.steam_common),
        dir.join("steamapps").join("common").join(game.steam_common),
    ];
    if let Some(found) = candidates
        .into_iter()
        .find(|c| install_problem(c, game, user_side).is_none())
    {
        return Ok(found);
    }
    let hint = format!(
        "Pick the folder that holds {} — for Steam that's …\\steamapps\\common\\{}.",
        game.exe, game.steam_common
    );
    if is_user_data_folder(&dir, game, user_side) {
        return Err(format!(
            "{picked} is {}'s user folder in Documents (your mods and profiles), not where the \
             game is installed. {hint}",
            game.display
        ));
    }
    Err(format!("There's no {} in {picked}. {hint}", game.exe))
}

/// Whether the drive or volume `path` lives on is missing — an unplugged USB disk, a network
/// share that isn't mounted. A folder there isn't gone, just out of reach, and must not be
/// forgotten.
pub fn volume_offline(path: &Path) -> bool {
    let Some(root) = path.ancestors().last() else {
        return false;
    };
    if cfg!(windows) {
        return !root.as_os_str().is_empty() && !root.exists();
    }
    // `/Volumes/<name>` (macOS) and `/media/<user>/<name>`, `/mnt/<name>` (Linux) are mount
    // points; anything else hangs off `/`, which is always there.
    let parts: Vec<_> = path.components().take(4).collect();
    let mount = match parts.get(1).map(|c| c.as_os_str().to_string_lossy().into_owned()) {
        Some(top) if top == "Volumes" || top == "mnt" => parts.iter().take(3).collect::<PathBuf>(),
        Some(top) if top == "media" => parts.iter().take(4).collect::<PathBuf>(),
        _ => return false,
    };
    !mount.exists()
}

/// What [`repair_install`] did to `game_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallRepair {
    /// Already right, or blank with nothing to fill it with.
    Fine,
    /// Replaced by (or, from blank, set to) the install Steam knows about.
    Fixed { from: String, to: String, why: String },
    /// Wrong, and nothing better was found: cleared, so nothing is put in it any more.
    Cleared { from: String, why: String },
    /// Wrong only because its drive is offline: left as it is.
    Offline { path: String },
}

impl InstallRepair {
    /// The folder that stopped being the install, when one did.
    pub fn replaced(&self) -> Option<&str> {
        match self {
            InstallRepair::Fixed { from, .. } | InstallRepair::Cleared { from, .. }
                if !from.trim().is_empty() =>
            {
                Some(from)
            }
            _ => None,
        }
    }

    pub fn changed(&self) -> bool {
        matches!(self, InstallRepair::Fixed { .. } | InstallRepair::Cleared { .. })
    }
}

/// Check `cfg.game_path` and put it right. `detect` is Steam detection, passed in so the order
/// can be tested without a Steam install; it only runs when the saved folder doesn't do.
/// `fill_blank` decides whether a blank folder is worth a detection pass.
pub fn repair_install(
    cfg: &mut AppConfig,
    docs: &[PathBuf],
    fill_blank: bool,
    detect: impl FnOnce() -> Option<String>,
) -> InstallRepair {
    let game = cfg.game();
    let saved = cfg.game_path.trim().to_string();
    if saved.is_empty() && !fill_blank {
        return InstallRepair::Fine;
    }
    let user_side = user_side_folders(cfg, docs);
    let why = match install_problem(Path::new(&saved), game, &user_side) {
        None => return InstallRepair::Fine,
        Some(why) => why,
    };
    let found = detect()
        .map(PathBuf::from)
        .filter(|d| install_problem(d, game, &user_side).is_none());
    if let Some(to) = found {
        let to = to.to_string_lossy().into_owned();
        cfg.game_path = to.clone();
        return InstallRepair::Fixed { from: saved, to, why };
    }
    if saved.is_empty() {
        return InstallRepair::Fine;
    }
    let saved_path = Path::new(&saved);
    if !is_user_data_folder(saved_path, game, &user_side) && volume_offline(saved_path) {
        return InstallRepair::Offline { path: saved };
    }
    cfg.game_path.clear();
    InstallRepair::Cleared { from: saved, why }
}

/// What [`repair_user_folder`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserRepair {
    /// The mods folder moved: `(from, to)`.
    pub mods: Option<(String, String)>,
    /// A profiles override that no longer exists was dropped (it was this path).
    pub profiles_cleared: Option<String>,
}

impl UserRepair {
    pub fn changed(&self) -> bool {
        self.mods.is_some() || self.profiles_cleared.is_some()
    }
}

/// `path` with its OneDrive component taken out: `C:\Users\r\OneDrive\Documents\PiBoSo\MX Bikes`
/// → `C:\Users\r\Documents\PiBoSo\MX Bikes`. What a folder moved out of OneDrive becomes.
pub fn without_onedrive(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut dropped = false;
    for c in path.components() {
        let name = c.as_os_str().to_string_lossy();
        let lower = name.to_ascii_lowercase();
        if !dropped && (lower == "onedrive" || lower.starts_with("onedrive - ")) {
            dropped = true;
            continue;
        }
        out.push(c.as_os_str());
    }
    dropped.then_some(out)
}

/// Where a vanished user folder may have gone, best first: the game's folder under every
/// Documents candidate, then the old path with OneDrive taken out.
pub fn user_folder_candidates(old: &Path, game: &GameProfile, docs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.iter().any(|seen| same_folder(seen, &p)) && !same_folder(&p, old) {
            out.push(p);
        }
    };
    for d in docs {
        push(d.join("PiBoSo").join(game.user_dir));
    }
    if let Some(p) = without_onedrive(old) {
        push(p);
    }
    out
}

/// The game's *install* folder typed or picked into the mods field (`…\steamapps\common\MX
/// Bikes`): it holds the executable, which no user folder does. It moves to the install field
/// when that one isn't already a real install, and the mods field is cleared for detection to
/// fill. Answers the folder taken out of the mods field.
pub fn take_install_out_of_mods(cfg: &mut AppConfig, docs: &[PathBuf]) -> Option<String> {
    let game = cfg.game();
    let mods = cfg.mods_path.trim().to_string();
    if mods.is_empty() || !has_exe(Path::new(&mods), game) {
        return None;
    }
    let mut without = cfg.clone();
    without.mods_path.clear();
    let side = user_side_folders(&without, docs);
    if install_problem(Path::new(cfg.game_path.trim()), game, &side).is_some() {
        cfg.game_path = mods.clone();
    }
    cfg.mods_path.clear();
    Some(mods)
}

/// The game's user folder under the first Documents candidate that has one.
fn first_user_folder(game: &GameProfile, docs: &[PathBuf]) -> Option<PathBuf> {
    docs.iter()
        .map(|d| d.join("PiBoSo").join(game.user_dir))
        .find(|c| crate::config::looks_like_mods_dir(&c.to_string_lossy()))
}

/// A saved mods folder that has disappeared is looked for again; a profiles override that has
/// disappeared is dropped; an install folder saved as the mods folder is moved to where it
/// belongs. Nothing changes while the old folder is there, while its drive is offline, or when
/// no replacement that really holds the game's files turns up.
pub fn repair_user_folder(cfg: &mut AppConfig, docs: &[PathBuf]) -> UserRepair {
    let mut out = UserRepair::default();
    let game = cfg.game();
    if let Some(install) = take_install_out_of_mods(cfg, docs) {
        match first_user_folder(game, docs) {
            Some(user) => {
                let user = user.to_string_lossy().into_owned();
                cfg.mods_path = user.clone();
                out.mods = Some((install, user));
            }
            // Nowhere better yet: keep the old value rather than a blank one, which would send
            // the app back to first-run setup. The install field is fixed either way.
            None => cfg.mods_path = install,
        }
    }
    let mods = cfg.mods_path.trim().to_string();
    if !mods.is_empty() {
        let old = PathBuf::from(&mods);
        if !old.is_dir() && !volume_offline(&old) {
            if let Some(new) = user_folder_candidates(&old, game, docs)
                .into_iter()
                .find(|c| crate::config::looks_like_mods_dir(&c.to_string_lossy()))
            {
                let new = new.to_string_lossy().into_owned();
                cfg.mods_path = new.clone();
                out.mods = Some((mods, new));
            }
        }
    }
    let profiles = cfg.profiles_path.trim().to_string();
    if !profiles.is_empty() {
        let p = Path::new(&profiles);
        if !p.is_dir() && !volume_offline(p) {
            cfg.profiles_path.clear();
            out.profiles_cleared = Some(profiles);
        }
    }
    out
}

/// Both repairs, against this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    pub install: InstallRepair,
    pub user: UserRepair,
}

impl Repair {
    pub fn changed(&self) -> bool {
        self.install.changed() || self.user.changed()
    }
}

/// [`repair_user_folder`] then [`repair_install`], with this machine's Documents candidates and
/// Steam detection.
pub fn repair(cfg: &mut AppConfig, fill_blank: bool) -> Repair {
    let docs = crate::docsdir::candidates();
    let before = cfg.game_path.trim().to_string();
    let user = repair_user_folder(cfg, &docs);
    let game = cfg.game();
    let mut install =
        repair_install(cfg, &docs, fill_blank, || crate::config::detect_game_path(game));
    // The user-folder repair can fill the install field itself (an install folder entered
    // as the mods folder); report that as the install fix it is.
    if install == InstallRepair::Fine && cfg.game_path.trim() != before {
        install = InstallRepair::Fixed {
            from: before,
            to: cfg.game_path.trim().to_string(),
            why: "the install folder had been entered as the mods folder".into(),
        };
    }
    Repair { install, user }
}

/// The install folder as it really is right now: the saved one when it holds the executable,
/// else what Steam detection finds. `None` when neither does.
pub fn real_install(cfg: &AppConfig) -> Option<PathBuf> {
    let game = cfg.game();
    let saved = PathBuf::from(cfg.game_path.trim());
    if has_exe(&saved, game) {
        return Some(saved);
    }
    crate::config::detect_game_path(game)
        .map(PathBuf::from)
        .filter(|d| has_exe(d, game))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;

    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "frost-gamefolders-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn install_at(root: &Path) -> PathBuf {
        let dir = root.join("SteamLibrary").join("steamapps").join("common").join("MX Bikes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mxbikes.exe"), b"MZ").unwrap();
        dir
    }

    fn user_at(docs: &Path) -> PathBuf {
        let user = docs.join("PiBoSo").join("MX Bikes");
        std::fs::create_dir_all(user.join("profiles")).unwrap();
        std::fs::create_dir_all(user.join("mods")).unwrap();
        user
    }

    /// The reported case: the install folder was saved as `Documents\PiBoSo`. It is rejected,
    /// and Steam's real install takes its place.
    #[test]
    fn a_documents_install_folder_is_replaced_by_the_steam_install() {
        let root = scratch("docs-install");
        let docs = root.join("OneDrive").join("Documents");
        let user = user_at(&docs);
        let install = install_at(&root);

        let mut cfg = AppConfig {
            mods_path: user.to_string_lossy().into_owned(),
            game_path: docs.join("PiBoSo").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let found = install.to_string_lossy().into_owned();
        let fix = repair_install(&mut cfg, &[docs.clone()], true, || Some(found.clone()));
        assert!(matches!(&fix, InstallRepair::Fixed { to, .. } if *to == found), "{fix:?}");
        assert_eq!(fix.replaced(), Some(docs.join("PiBoSo").to_string_lossy().as_ref()));
        assert_eq!(cfg.game_path, found);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Documents is never an install, even when Steam detection finds nothing: cleared, so
    /// nothing is installed into its `plugins` folder any more.
    #[test]
    fn a_documents_install_folder_is_cleared_when_nothing_better_is_found() {
        let root = scratch("docs-clear");
        let docs = root.join("Documents");
        let user = user_at(&docs);
        let piboso = docs.join("PiBoSo");
        let mut cfg = AppConfig {
            mods_path: user.to_string_lossy().into_owned(),
            game_path: piboso.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let fix = repair_install(&mut cfg, &[docs.clone()], true, || None);
        assert!(matches!(fix, InstallRepair::Cleared { .. }), "{fix:?}");
        assert!(cfg.game_path.is_empty());

        // Detection is never trusted blind either: a "found" user folder is not an install.
        cfg.game_path = piboso.to_string_lossy().into_owned();
        let user_s = user.to_string_lossy().into_owned();
        let fix = repair_install(&mut cfg, &[docs], true, || Some(user_s.clone()));
        assert!(matches!(fix, InstallRepair::Cleared { .. }), "{fix:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stale install (the game moved to another library) resolves to where it went; a good
    /// one is left alone and detection is never asked.
    #[test]
    fn a_stale_install_path_auto_resolves_and_a_good_one_is_kept() {
        let root = scratch("stale");
        let install = install_at(&root);
        let gone = root.join("OldLibrary").join("steamapps").join("common").join("MX Bikes");
        let mut cfg = AppConfig {
            game_path: gone.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let found = install.to_string_lossy().into_owned();
        let fix = repair_install(&mut cfg, &[], true, || Some(found.clone()));
        assert!(matches!(fix, InstallRepair::Fixed { .. }), "{fix:?}");
        assert_eq!(cfg.game_path, found);

        let fix = repair_install(&mut cfg, &[], true, || panic!("a good folder needs no detection"));
        assert_eq!(fix, InstallRepair::Fine);

        // A folder that exists but has no exe in it is wrong too.
        let empty = root.join("Empty");
        std::fs::create_dir_all(&empty).unwrap();
        cfg.game_path = empty.to_string_lossy().into_owned();
        let fix = repair_install(&mut cfg, &[], true, || Some(found.clone()));
        assert!(matches!(fix, InstallRepair::Fixed { .. }), "{fix:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A blank folder is filled only when asked to be (startup, Play), not on every poll.
    #[test]
    fn a_blank_install_is_filled_only_when_asked() {
        let root = scratch("blank");
        let install = install_at(&root);
        let found = install.to_string_lossy().into_owned();
        let mut cfg = AppConfig::default();
        assert_eq!(
            repair_install(&mut cfg, &[], false, || panic!("not asked")),
            InstallRepair::Fine
        );
        assert!(matches!(
            repair_install(&mut cfg, &[], true, || Some(found.clone())),
            InstallRepair::Fixed { .. }
        ));
        assert_eq!(cfg.game_path, found);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The picker refuses the Documents side with a message that says why, and takes a pick of
    /// the Steam library down to the game's folder.
    #[test]
    fn a_documents_pick_is_rejected_and_a_library_pick_is_resolved() {
        let root = scratch("pick");
        let docs = root.join("Documents");
        let user = user_at(&docs);
        let install = install_at(&root);
        let game = Game::Mxb.profile();
        let cfg = AppConfig { mods_path: user.to_string_lossy().into_owned(), ..Default::default() };
        let side = user_side_folders(&cfg, &[docs.clone()]);

        let err = check_install_pick(&docs.join("PiBoSo").to_string_lossy(), game, &side).unwrap_err();
        assert!(err.contains("user folder"), "{err}");
        let err = check_install_pick(&user.to_string_lossy(), game, &side).unwrap_err();
        assert!(err.contains("user folder"), "{err}");
        // A `PiBoSo` folder anywhere is the user side too, wherever Documents is now.
        let elsewhere = root.join("Elsewhere").join("PiBoSo");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let err = check_install_pick(&elsewhere.to_string_lossy(), game, &side).unwrap_err();
        assert!(err.contains("user folder"), "{err}");

        let lib = root.join("SteamLibrary");
        assert_eq!(check_install_pick(&lib.to_string_lossy(), game, &side).unwrap(), install);
        assert_eq!(check_install_pick(&install.to_string_lossy(), game, &side).unwrap(), install);
        let err = check_install_pick(&root.join("nope").to_string_lossy(), game, &side).unwrap_err();
        assert!(err.contains("mxbikes.exe"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The reported move: the user folder left OneDrive. The saved path is gone, the new one is
    /// found under the moved Documents, and a dead profiles override is dropped.
    #[test]
    fn a_moved_user_folder_is_redetected() {
        let root = scratch("moved");
        let old_docs = root.join("Users").join("rider").join("OneDrive").join("Documents");
        let new_docs = root.join("Users").join("rider").join("Documents");
        let new_user = user_at(&new_docs);
        let old_user = old_docs.join("PiBoSo").join("MX Bikes");

        let mut cfg = AppConfig {
            mods_path: old_user.to_string_lossy().into_owned(),
            profiles_path: old_user.join("profiles").to_string_lossy().into_owned(),
            ..Default::default()
        };
        // The known folder may still say OneDrive; the OneDrive-less old path finds it anyway.
        let fix = repair_user_folder(&mut cfg, &[old_docs.clone()]);
        assert_eq!(
            fix.mods,
            Some((old_user.to_string_lossy().into_owned(), new_user.to_string_lossy().into_owned()))
        );
        assert!(fix.profiles_cleared.is_some());
        assert!(cfg.profiles_path.is_empty());
        assert_eq!(cfg.profiles_dir(), new_user.join("profiles"));

        // Present again: nothing to do.
        assert!(!repair_user_folder(&mut cfg, &[new_docs.clone()]).changed());

        // Gone with nowhere to go: kept, rather than pointing at something worse.
        let mut lost = AppConfig {
            mods_path: root.join("Nowhere").join("MX Bikes").to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert!(!repair_user_folder(&mut lost, &[root.join("NoDocs")]).changed());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The third report: the Steam install typed into the mods field, the install field still
    /// on `Documents\PiBoSo`. The install moves to its own field, and the mods field goes back
    /// to the game folder in Documents.
    #[test]
    fn an_install_entered_as_the_mods_folder_is_moved_to_the_install_field() {
        let root = scratch("install-as-mods");
        let docs = root.join("Documents");
        let user = user_at(&docs);
        let install = install_at(&root);
        let mut cfg = AppConfig {
            mods_path: install.to_string_lossy().into_owned(),
            game_path: docs.join("PiBoSo").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let fix = repair_user_folder(&mut cfg, &[docs.clone()]);
        assert_eq!(cfg.game_path, install.to_string_lossy());
        assert_eq!(cfg.mods_path, user.to_string_lossy());
        assert!(fix.mods.is_some());
        let side = user_side_folders(&cfg, &[docs.clone()]);
        assert!(install_problem(Path::new(&cfg.game_path), Game::Mxb.profile(), &side).is_none());

        // A good install field is kept; the mods field is still cleared for detection.
        let other = root.join("Other").join("MX Bikes");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("mxbikes.exe"), b"MZ").unwrap();
        let mut cfg = AppConfig {
            mods_path: other.to_string_lossy().into_owned(),
            game_path: install.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert_eq!(take_install_out_of_mods(&mut cfg, &[docs]), Some(other.to_string_lossy().into_owned()));
        assert_eq!(cfg.game_path, install.to_string_lossy());
        assert!(cfg.mods_path.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn onedrive_is_taken_out_of_a_path() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(
            without_onedrive(&p("/Users/r/OneDrive/Documents/PiBoSo/MX Bikes")),
            Some(p("/Users/r/Documents/PiBoSo/MX Bikes"))
        );
        assert_eq!(
            without_onedrive(&p("/Users/r/OneDrive - Contoso/Documents")),
            Some(p("/Users/r/Documents"))
        );
        assert_eq!(without_onedrive(&p("/Users/r/Documents")), None);
    }
}
