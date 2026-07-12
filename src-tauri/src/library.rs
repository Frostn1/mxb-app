use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModFolder {
    pub path: String,
    pub name: String,
    pub mods: Vec<InstalledMod>,
}

/// `<mods_path>/<subpath>`, where `subpath` is like `mods/tracks` or `mods/bikes`.
pub fn mods_subdir(mods_path: &str, subpath: &str) -> PathBuf {
    let mut p = PathBuf::from(mods_path);
    for seg in subpath.split(['/', '\\']).filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
}

/// Scan installed mods under `<mods_path>/<subpath>`. Each subfolder becomes a
/// folder with its files; loose files (e.g. a bare `.pkz`) become a folder entry
/// with no children.
pub fn scan_mods(mods_path: &str, subpath: &str) -> anyhow::Result<Vec<InstalledModFolder>> {
    let dir = mods_subdir(mods_path, subpath);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut folders = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        let mods = if path.is_dir() {
            list_files(&path)
        } else {
            Vec::new()
        };

        folders.push(InstalledModFolder {
            path: path.to_string_lossy().into_owned(),
            name,
            mods,
        });
    }

    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(folders)
}

fn list_files(dir: &Path) -> Vec<InstalledMod> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| InstalledMod {
            path: e.path().to_string_lossy().into_owned(),
            name: e.file_name().to_string_lossy().into_owned(),
        })
        .collect()
}
