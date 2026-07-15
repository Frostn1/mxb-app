use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    /// File name, e.g. `Mosctesting.pkz`.
    pub name: String,
    /// Absolute path on disk.
    pub path: String,
    /// Relative parent folder under the subpath (`""` if top-level). Tracks are
    /// often nested a few folders deep, so this preserves where they live.
    pub folder: String,
    /// File size on disk, in bytes (shown on the library card immediately;
    /// available even for non-plain archives we can't open).
    pub size: u64,
}

/// `<mods_path>/<subpath>`, where `subpath` is like `mods/tracks` or `mods/bikes`.
pub fn mods_subdir(mods_path: &str, subpath: &str) -> PathBuf {
    let mut p = PathBuf::from(mods_path);
    for seg in subpath.split(['/', '\\']).filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
}

fn sanitize_seg(seg: &str) -> String {
    seg.chars()
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

/// Move an installed mod file into a different folder (relative to the type dir,
/// e.g. `Supercross` or `New/Sub`; `""` = the type root). Creates the folder if
/// needed. The source must live under the type dir (guards against stray paths).
pub fn move_mod(
    mods_path: &str,
    from_path: &str,
    to_folder: &str,
    subpath: &str,
) -> anyhow::Result<()> {
    let from = PathBuf::from(from_path);
    if !from.is_file() {
        anyhow::bail!("file not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to move a file outside the {subpath} folder");
    }

    let mut dest_dir = type_dir;
    for seg in to_folder.split(['/', '\\']).filter(|s| !s.is_empty()) {
        dest_dir.push(sanitize_seg(seg));
    }
    fs::create_dir_all(&dest_dir)?;

    let name = from
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("bad file name"))?;
    let dest = dest_dir.join(name);
    if dest == from {
        return Ok(());
    }
    if dest.exists() {
        anyhow::bail!("a mod named '{}' is already in that folder", name.to_string_lossy());
    }

    // Rename when possible; fall back to copy+remove across volumes.
    if fs::rename(&from, &dest).is_err() {
        fs::copy(&from, &dest)?;
        fs::remove_file(&from)?;
    }
    Ok(())
}

/// Move an installed mod file to the OS Recycle Bin / Trash. The file must live
/// under the type dir (same guard as `move_mod`) so a stray path can't be trashed.
pub fn uninstall_mod(mods_path: &str, from_path: &str, subpath: &str) -> anyhow::Result<()> {
    let from = PathBuf::from(from_path);
    if !from.is_file() {
        anyhow::bail!("file not found: {from_path}");
    }
    let type_dir = mods_subdir(mods_path, subpath);
    if !from.starts_with(&type_dir) {
        anyhow::bail!("refusing to uninstall a file outside the {subpath} folder");
    }
    trash::delete(&from)?;
    Ok(())
}

/// Reveal a file in the OS file manager, selecting it when supported.
pub fn reveal_in_explorer(path: &str) -> anyhow::Result<()> {
    let p = PathBuf::from(path);
    if !p.exists() {
        anyhow::bail!("path not found: {path}");
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&p)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-R").arg(&p).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // No portable "select the file" on Linux — open its parent folder.
        let target = p.parent().unwrap_or(&p);
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

/// Recursively find installed `.pkz` mod files under `<mods_path>/<subpath>` at
/// any depth (tracks/bikes are frequently nested inside sub-folders).
pub fn scan_mods(mods_path: &str, subpath: &str) -> anyhow::Result<Vec<InstalledMod>> {
    let dir = mods_subdir(mods_path, subpath);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut items = Vec::new();
    for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_pkz = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pkz"))
            .unwrap_or(false);
        if !is_pkz {
            continue;
        }

        let folder = path
            .parent()
            .and_then(|p| p.strip_prefix(&dir).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        items.push(InstalledMod {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            folder,
            size,
        });
    }

    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-lib-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn moves_mod_between_folders() {
        let root = tmp("move");
        let old = root.join("mods").join("tracks").join("Old");
        fs::create_dir_all(&old).unwrap();
        let file = old.join("t.pkz");
        fs::write(&file, b"x").unwrap();

        move_mod(
            root.to_str().unwrap(),
            file.to_str().unwrap(),
            "New Folder",
            "mods/tracks",
        )
        .unwrap();

        assert!(!file.exists());
        assert!(root.join("mods/tracks/New Folder/t.pkz").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn move_rejects_file_outside_type_dir() {
        let root = tmp("move-guard");
        fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside.pkz");
        fs::write(&outside, b"x").unwrap();

        let res = move_mod(
            root.to_str().unwrap(),
            outside.to_str().unwrap(),
            "X",
            "mods/tracks",
        );
        assert!(res.is_err());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
