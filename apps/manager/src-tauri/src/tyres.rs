//! Tyre folders under `mods/tyres` that break the bike list.
//!
//! A folder or `.pkz` in `mods/tyres` named like a stock tyre (`p_mx`, `m_sm`, from the
//! game's `tyres.pkz`) replaces the stock one. When that replacement has no files in it,
//! every bike on that tyre crashes the game as soon as the bike list opens. An empty
//! `mods/tyres/p_mx` reproduces it; so does one holding only `desktop.ini`, which
//! Explorer and OneDrive drop into folders on their own.
//!
//! The fix moves the empty folder to `<PiBoSo>/mxbapp_moved/tyres`, outside `mods`, so the
//! game no longer sees it. Nothing is deleted.

use crate::config::AppConfig;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Used when `tyres.pkz` can't be read: the two tyres the game ships.
const FALLBACK_STOCK: &[&str] = &["m_sm", "p_mx"];

/// Where moved-out folders go, beside `mods` and outside anything the game mounts.
pub const MOVED_DIR: &str = "mxbapp_moved";

/// Files Windows, OneDrive and macOS create on their own. A folder holding only these is
/// as empty as one holding nothing, as far as the game is concerned.
fn is_junk(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(n.as_str(), "desktop.ini" | "thumbs.db" | "ehthumbs.db" | ".ds_store" | "icon\r")
        || n.starts_with("._")
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TyreHealth {
    /// Folders in `mods/tyres` with no real files in them. These crash the bike list.
    pub empty: Vec<String>,
    /// Folders or `.pkz` files that replace a stock tyre and do have files.
    pub overrides: Vec<String>,
}

impl TyreHealth {
    pub fn summary_line(&self) -> String {
        if self.empty.is_empty() && self.overrides.is_empty() {
            return "tyres: ok".into();
        }
        let mut parts = Vec::new();
        if !self.empty.is_empty() {
            parts.push(format!("EMPTY {}", self.empty.join(", ")));
        }
        if !self.overrides.is_empty() {
            parts.push(format!("replaces stock {}", self.overrides.join(", ")));
        }
        format!("tyres: {}", parts.join("; "))
    }
}

/// Stock tyre names from `tyres.pkz` entry paths (`tyres/p_mx/...` → `p_mx`), lowercased.
pub fn stock_from_entries(entries: &[String]) -> Vec<String> {
    let mut out: Vec<String> = entries
        .iter()
        .filter_map(|e| {
            let e = e.replace('\\', "/");
            let mut segs = e.split('/').filter(|s| !s.is_empty());
            let first = segs.next()?;
            let name = segs.next()?;
            // Only folders count: `tyres/<name>/<file>` has a third segment.
            segs.next()?;
            first.eq_ignore_ascii_case("tyres").then(|| name.to_ascii_lowercase())
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Stock tyre names for this install, read once per `tyres.pkz` path.
fn stock_names(install_dir: &str) -> Vec<String> {
    static CACHE: Mutex<Option<(PathBuf, Vec<String>)>> = Mutex::new(None);
    let fallback = || FALLBACK_STOCK.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    if install_dir.trim().is_empty() {
        return fallback();
    }
    let pkz = crate::library::resolve_child(Path::new(install_dir.trim()), "tyres.pkz");
    if let Ok(guard) = CACHE.lock() {
        if let Some((p, names)) = guard.as_ref() {
            if *p == pkz {
                return names.clone();
            }
        }
    }
    let names = crate::pkz::entry_names(&pkz)
        .ok()
        .map(|e| stock_from_entries(&e))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(fallback);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((pkz, names.clone()));
    }
    names
}

/// True when `dir` holds at least one file that isn't system junk, at any depth.
fn has_real_files(dir: &Path, depth: usize) -> bool {
    if depth > 16 {
        // Something this deep isn't an empty tyre folder; don't call it one.
        return true;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        // Can't look inside: don't claim it's empty.
        return true;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { return true };
        if ft.is_dir() {
            if has_real_files(&entry.path(), depth + 1) {
                return true;
            }
        } else if !is_junk(&entry.file_name().to_string_lossy()) {
            return true;
        }
    }
    false
}

/// Check one `mods/tyres` folder against the stock tyre names.
pub fn scan(tyres_dir: &Path, stock: &[String]) -> TyreHealth {
    let mut h = TyreHealth::default();
    let Ok(rd) = fs::read_dir(tyres_dir) else { return h };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if !has_real_files(&entry.path(), 0) {
                h.empty.push(name);
            } else if stock.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                h.overrides.push(name);
            }
        } else {
            let p = Path::new(&name);
            let is_pkz = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz"));
            let stem = p.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
            if is_pkz && stock.iter().any(|s| s.eq_ignore_ascii_case(&stem)) {
                h.overrides.push(name);
            }
        }
    }
    h.empty.sort_by_key(|s| s.to_ascii_lowercase());
    h.overrides.sort_by_key(|s| s.to_ascii_lowercase());
    h
}

fn tyres_dir(cfg: &AppConfig) -> Option<PathBuf> {
    if cfg.mods_path.trim().is_empty() {
        return None;
    }
    Some(crate::library::resolve_child(&crate::library::mods_root(&cfg.mods_path), "tyres"))
}

pub fn check(cfg: &AppConfig) -> TyreHealth {
    let Some(dir) = tyres_dir(cfg) else { return TyreHealth::default() };
    if !dir.is_dir() {
        return TyreHealth::default();
    }
    scan(&dir, &stock_names(&cfg.install_dir()))
}

/// A free spot for `name` under `dest`: `p_mx`, then `p_mx (2)`, `p_mx (3)`…
fn free_slot(dest: &Path, name: &str) -> PathBuf {
    let first = dest.join(name);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|i| dest.join(format!("{name} ({i})")))
        .find(|p| !p.exists())
        .expect("an unbounded range finds a free name")
}

/// Move every empty tyre folder in `tyres_dir` to `moved_root/tyres`. Re-scans rather than
/// taking names from the UI, so only folders that are empty right now are touched.
pub fn move_empty(tyres_dir: &Path, moved_root: &Path, game_running: bool) -> anyhow::Result<Vec<PathBuf>> {
    if game_running {
        anyhow::bail!("Close the game first.");
    }
    let empty = scan(tyres_dir, &[]).empty;
    if empty.is_empty() {
        return Ok(Vec::new());
    }
    let dest = moved_root.join("tyres");
    fs::create_dir_all(&dest)?;
    let mut moved = Vec::new();
    for name in empty {
        let to = free_slot(&dest, &name);
        fs::rename(tyres_dir.join(&name), &to)?;
        moved.push(to);
    }
    Ok(moved)
}

/// [`move_empty`] for the configured mods folder.
pub fn move_empty_for(cfg: &AppConfig, game_running: bool) -> anyhow::Result<Vec<PathBuf>> {
    let Some(dir) = tyres_dir(cfg).filter(|d| d.is_dir()) else {
        return Ok(Vec::new());
    };
    let moved_root = crate::library::resolve_child(Path::new(cfg.mods_path.trim()), MOVED_DIR);
    move_empty(&dir, &moved_root, game_running)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("frost-tyres-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn stock() -> Vec<String> {
        vec!["m_sm".into(), "p_mx".into()]
    }

    #[test]
    fn stock_names_come_from_the_tyres_pkz_folders() {
        let entries = vec![
            "tyres/p_mx/front.edf".to_string(),
            "tyres\\P_MX\\rear.edf".to_string(),
            "tyres/m_sm/front.edf".to_string(),
            "tyres/readme.txt".to_string(),
            "bikes/p_mx/x".to_string(),
        ];
        assert_eq!(stock_from_entries(&entries), vec!["m_sm", "p_mx"]);
    }

    #[test]
    fn an_empty_folder_is_flagged_even_with_empty_subfolders() {
        let root = tmp("empty");
        fs::create_dir_all(root.join("p_mx")).unwrap();
        fs::create_dir_all(root.join("custom/a/b")).unwrap();
        write(&root.join("good/tyre.edf"), "x");
        let h = scan(&root, &stock());
        assert_eq!(h.empty, vec!["custom", "p_mx"]);
        assert!(h.overrides.is_empty());
        assert!(h.summary_line().starts_with("tyres: EMPTY custom, p_mx"));
    }

    #[test]
    fn a_folder_with_only_desktop_ini_is_empty() {
        let root = tmp("desktopini");
        write(&root.join("p_mx/desktop.ini"), "[.ShellClassInfo]");
        write(&root.join("m_sm/sub/Thumbs.db"), "x");
        write(&root.join("m_sm/.DS_Store"), "x");
        let h = scan(&root, &stock());
        assert_eq!(h.empty, vec!["m_sm", "p_mx"]);
    }

    #[test]
    fn a_full_folder_or_pkz_with_a_stock_name_is_an_override() {
        let root = tmp("override");
        write(&root.join("P_MX/front.edf"), "x");
        write(&root.join("m_sm.pkz"), "x");
        write(&root.join("other.pkz"), "x");
        write(&root.join("mine/front.edf"), "x");
        let h = scan(&root, &stock());
        assert!(h.empty.is_empty());
        assert_eq!(h.overrides, vec!["m_sm.pkz", "P_MX"]);
        assert_eq!(h.summary_line(), "tyres: replaces stock m_sm.pkz, P_MX");
    }

    #[test]
    fn nothing_wrong_is_ok_and_a_missing_folder_is_ok() {
        let root = tmp("ok");
        write(&root.join("mine/front.edf"), "x");
        assert_eq!(scan(&root, &stock()).summary_line(), "tyres: ok");
        assert_eq!(scan(&root.join("nope"), &stock()), TyreHealth::default());
    }

    #[test]
    fn moving_out_keeps_the_folder_and_its_junk_and_never_overwrites() {
        let base = tmp("move");
        let tyres = base.join("mods/tyres");
        let moved = base.join(MOVED_DIR);
        write(&tyres.join("p_mx/desktop.ini"), "ini");
        write(&tyres.join("keep/front.edf"), "x");
        fs::create_dir_all(moved.join("tyres/p_mx")).unwrap();

        assert!(move_empty(&tyres, &moved, true).is_err(), "refused while the game runs");
        assert!(tyres.join("p_mx").is_dir());

        let out = move_empty(&tyres, &moved, false).unwrap();
        assert_eq!(out, vec![moved.join("tyres/p_mx (2)")]);
        assert!(!tyres.join("p_mx").exists());
        assert!(moved.join("tyres/p_mx (2)/desktop.ini").is_file());
        assert!(tyres.join("keep/front.edf").is_file(), "real tyres stay");
        assert!(move_empty(&tyres, &moved, false).unwrap().is_empty());
    }
}
