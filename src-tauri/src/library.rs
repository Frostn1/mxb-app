use serde::Serialize;
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
}

/// `<mods_path>/<subpath>`, where `subpath` is like `mods/tracks` or `mods/bikes`.
pub fn mods_subdir(mods_path: &str, subpath: &str) -> PathBuf {
    let mut p = PathBuf::from(mods_path);
    for seg in subpath.split(['/', '\\']).filter(|s| !s.is_empty()) {
        p.push(seg);
    }
    p
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

        items.push(InstalledMod {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            folder,
        });
    }

    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}
