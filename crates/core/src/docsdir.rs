//! Where Windows really keeps the user's Documents folder.
//!
//! The game writes its user folder to `<Documents>\PiBoSo\<game>`, and "Documents" there is
//! the shell's known folder, not `%USERPROFILE%\Documents`. The two part ways on a lot of
//! machines: OneDrive's Known Folder Move puts it at `%OneDrive%\Documents`, and
//! Properties > Location lets anyone move it to another drive (`D:\Documents`). Building the
//! path from the profile folder then points at a folder the game has never written to, so
//! detection misses the real one and a player who picks it by hand is told it isn't where
//! the game keeps things.
//!
//! So the answer is asked of Windows in the order Windows itself would give it:
//!
//! 1. `SHGetKnownFolderPath(FOLDERID_Documents)` (what `dirs_next::document_dir` calls);
//! 2. the registry's `User Shell Folders\Personal`, with `%VARS%` expanded, then the older
//!    `Shell Folders\Personal` cache;
//! 3. the OneDrive roots, for a Documents folder moved there;
//! 4. `%USERPROFILE%\Documents`, the stock location.
//!
//! Every one of those is a *candidate*: [`user_dir_in`] takes the first that actually holds
//! the game's folder, and falls back to the first candidate when none does yet.

use std::path::{Path, PathBuf};

/// Where the registry keeps the shell's folder locations for the current user.
#[cfg(windows)]
const USER_SHELL_FOLDERS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders";
#[cfg(windows)]
const SHELL_FOLDERS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders";

/// What the machine says about Documents, gathered before any decision is made — split out
/// so the ordering and expansion rules can be tested with paths from any machine.
#[derive(Debug, Default, Clone)]
pub struct DocsSources {
    /// `SHGetKnownFolderPath(FOLDERID_Documents)`.
    pub known_folder: Option<PathBuf>,
    /// `User Shell Folders\Personal`, raw (may hold `%USERPROFILE%`).
    pub user_shell_personal: Option<String>,
    /// `Shell Folders\Personal`, raw.
    pub shell_personal: Option<String>,
}

/// Expand `%NAME%` references the way `ExpandEnvironmentStrings` does: a name that isn't
/// set is left as written, and `%%` is not special.
pub fn expand_env(raw: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Two spellings of one folder: Windows paths are case-insensitive and take either slash,
/// and a trailing separator means nothing.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    fn key(p: &Path) -> String {
        let s = p.to_string_lossy().replace('/', "\\");
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
        s.trim_end_matches('\\').to_lowercase()
    }
    let (ka, kb) = (key(a), key(b));
    !ka.is_empty() && ka == kb
}

/// Every place Documents might be, best first, without duplicates. Pure: `env` answers
/// for the environment so a test can describe any machine.
pub fn candidates_from(sources: &DocsSources, env: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let env = &env;
    let expanded = |raw: &Option<String>| {
        raw.as_deref()
            .map(|r| expand_env(r.trim(), env))
            // Still holding a `%VAR%` means it named something unset — not a path.
            .filter(|p| !p.is_empty() && !p.contains('%'))
            .map(PathBuf::from)
    };
    let mut ordered: Vec<PathBuf> = Vec::new();
    ordered.extend(sources.known_folder.clone());
    ordered.extend(expanded(&sources.user_shell_personal));
    ordered.extend(expanded(&sources.shell_personal));
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(root) = env(var).filter(|r| !r.trim().is_empty()) {
            ordered.push(PathBuf::from(root.trim()).join("Documents"));
        }
    }
    if let Some(profile) = env("USERPROFILE").filter(|p| !p.trim().is_empty()) {
        ordered.push(PathBuf::from(profile.trim()).join("Documents"));
    }

    let mut out: Vec<PathBuf> = Vec::new();
    for p in ordered {
        // Only absolute paths: a relative one would resolve against wherever the app
        // happened to start.
        if !is_absolute_windows(&p) {
            continue;
        }
        if !out.iter().any(|seen| same_folder(seen, &p)) {
            out.push(p);
        }
    }
    out
}

/// `Path::is_absolute`, but judged by Windows rules on every host, so the tests describing
/// a Windows machine pass on the macOS and Linux CI runners too.
fn is_absolute_windows(p: &Path) -> bool {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
    drive || s.starts_with(r"\\") || s.starts_with("//") || p.is_absolute()
}

/// The game's user folder (`<Documents>\PiBoSo\<game>`) in the first candidate that has
/// one; when none does yet, where it would go in the best candidate.
pub fn user_dir_in(candidates: &[PathBuf], game_user_dir: &str) -> Option<PathBuf> {
    let under = |docs: &PathBuf| docs.join("PiBoSo").join(game_user_dir);
    candidates
        .iter()
        .map(under)
        .find(|p| p.is_dir())
        .or_else(|| candidates.first().map(under))
}

/// What this machine says, read live.
pub fn sources() -> DocsSources {
    DocsSources {
        known_folder: dirs_next::document_dir(),
        user_shell_personal: read_personal(true),
        shell_personal: read_personal(false),
    }
}

/// `Personal` from `User Shell Folders` (the live value, may hold `%VARS%`) or, with
/// `user_shell` false, from the older `Shell Folders` cache.
#[cfg(windows)]
fn read_personal(user_shell: bool) -> Option<String> {
    let key = if user_shell { USER_SHELL_FOLDERS } else { SHELL_FOLDERS };
    windows_registry::CURRENT_USER
        .open(key)
        .and_then(|k| k.get_string("Personal"))
        .ok()
        .filter(|v| !v.trim().is_empty())
}

#[cfg(not(windows))]
fn read_personal(_user_shell: bool) -> Option<String> {
    None
}

/// Every place Documents might be on this machine, best first.
pub fn candidates() -> Vec<PathBuf> {
    if cfg!(windows) {
        candidates_from(&sources(), |name| std::env::var(name).ok())
    } else {
        // Elsewhere there is one answer, and the Windows-only fallbacks would only add
        // folders that can't exist.
        dirs_next::document_dir().into_iter().collect()
    }
}

/// The game's user folder on this machine, following a redirected Documents folder.
pub fn user_dir(game_user_dir: &str) -> Option<PathBuf> {
    user_dir_in(&candidates(), game_user_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_ascii_uppercase(), v.to_string()))
            .collect();
        move |name: &str| map.get(&name.to_ascii_uppercase()).cloned()
    }

    #[test]
    fn expands_variables_and_leaves_unknown_ones() {
        let env = env_of(&[("USERPROFILE", r"C:\Users\rider")]);
        assert_eq!(expand_env(r"%USERPROFILE%\Documents", &env), r"C:\Users\rider\Documents");
        assert_eq!(expand_env(r"%NOPE%\Documents", &env), r"%NOPE%\Documents");
        assert_eq!(expand_env("100% sure", &env), "100% sure");
        assert_eq!(expand_env(r"D:\Documents", &env), r"D:\Documents");
    }

    /// Properties > Location moved Documents to another drive: the known folder and the
    /// registry both say `D:\Documents`, and the profile folder is only the last resort.
    #[test]
    fn documents_moved_to_another_drive_comes_first() {
        let sources = DocsSources {
            known_folder: Some(PathBuf::from(r"D:\Documents")),
            user_shell_personal: Some(r"D:\Documents".into()),
            shell_personal: Some(r"D:\Documents".into()),
        };
        let env = env_of(&[("USERPROFILE", r"C:\Users\rider")]);
        let got = candidates_from(&sources, env);
        assert_eq!(
            got,
            vec![PathBuf::from(r"D:\Documents"), PathBuf::from(r"C:\Users\rider\Documents")]
        );
    }

    /// The known-folder API failed (or isn't there): the registry value still names the
    /// moved folder, with its variables expanded, ahead of `%USERPROFILE%`.
    #[test]
    fn registry_value_is_used_and_expanded_when_the_api_has_no_answer() {
        let sources = DocsSources {
            known_folder: None,
            user_shell_personal: Some(r"%OneDrive%\Documents".into()),
            shell_personal: None,
        };
        let env = env_of(&[
            ("USERPROFILE", r"C:\Users\rider"),
            ("OneDrive", r"C:\Users\rider\OneDrive"),
        ]);
        let got = candidates_from(&sources, env);
        assert_eq!(got[0], PathBuf::from(r"C:\Users\rider\OneDrive\Documents"));
        assert_eq!(got.last().unwrap(), &PathBuf::from(r"C:\Users\rider\Documents"));
        // The OneDrive root named again by the env var isn't listed twice.
        assert_eq!(got.len(), 2, "{got:?}");
    }

    /// OneDrive Known Folder Move with OneDrive itself on another drive.
    #[test]
    fn onedrive_documents_on_another_drive() {
        let sources = DocsSources {
            known_folder: Some(PathBuf::from(r"E:\OneDrive - Contoso\Documents")),
            user_shell_personal: Some(r"E:\OneDrive - Contoso\Documents".into()),
            shell_personal: None,
        };
        let env = env_of(&[
            ("USERPROFILE", r"C:\Users\rider"),
            ("OneDriveCommercial", r"E:\OneDrive - Contoso"),
            ("OneDriveConsumer", r"C:\Users\rider\OneDrive"),
        ]);
        let got = candidates_from(&sources, env);
        assert_eq!(
            got,
            vec![
                PathBuf::from(r"E:\OneDrive - Contoso\Documents"),
                PathBuf::from(r"C:\Users\rider\OneDrive\Documents"),
                PathBuf::from(r"C:\Users\rider\Documents"),
            ]
        );
    }

    /// A value naming an unset variable, or a relative path, is not a folder.
    #[test]
    fn unresolvable_values_are_dropped() {
        let sources = DocsSources {
            known_folder: None,
            user_shell_personal: Some(r"%GONE%\Documents".into()),
            shell_personal: Some("Documents".into()),
        };
        let env = env_of(&[("USERPROFILE", r"C:\Users\rider")]);
        assert_eq!(candidates_from(&sources, env), vec![PathBuf::from(r"C:\Users\rider\Documents")]);
    }

    #[test]
    fn same_folder_ignores_case_slashes_and_trailing_separators() {
        assert!(same_folder(Path::new(r"D:\Documents\PiBoSo\MX Bikes"), Path::new("d:/documents/piboso/mx bikes/")));
        assert!(same_folder(Path::new(r"\\?\D:\Documents"), Path::new(r"D:\Documents")));
        assert!(!same_folder(Path::new(r"D:\Documents"), Path::new(r"C:\Users\rider\Documents")));
        assert!(!same_folder(Path::new(""), Path::new("")));
    }

    /// The first candidate holding the game's folder wins, even when it isn't the first
    /// candidate — a Documents redirected after the game had already written elsewhere.
    #[test]
    fn picks_the_candidate_that_actually_holds_the_game_folder() {
        let tmp = std::env::temp_dir().join(format!("mxb-docsdir-{}", std::process::id()));
        let moved = tmp.join("D").join("Documents");
        let stock = tmp.join("C").join("Users").join("rider").join("Documents");
        std::fs::create_dir_all(stock.join("PiBoSo").join("MX Bikes")).unwrap();
        std::fs::create_dir_all(&moved).unwrap();

        let candidates = vec![moved.clone(), stock.clone()];
        assert_eq!(
            user_dir_in(&candidates, "MX Bikes"),
            Some(stock.join("PiBoSo").join("MX Bikes"))
        );

        // Once the game writes to the moved folder, that one is preferred.
        std::fs::create_dir_all(moved.join("PiBoSo").join("MX Bikes")).unwrap();
        assert_eq!(
            user_dir_in(&candidates, "MX Bikes"),
            Some(moved.join("PiBoSo").join("MX Bikes"))
        );

        // Nothing written anywhere: where it would go in the best candidate.
        assert_eq!(
            user_dir_in(&candidates, "GP Bikes"),
            Some(moved.join("PiBoSo").join("GP Bikes"))
        );
        assert_eq!(user_dir_in(&[], "MX Bikes"), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
