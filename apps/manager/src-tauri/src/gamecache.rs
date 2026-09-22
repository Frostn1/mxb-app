//! MX Bikes' disposable track-texture cache.
//!
//! PiBoSo stores it in the game user-data folder beside `mods`.  The app may be configured
//! either with that user-data folder or with its `mods` child, so derive the cache path from
//! the resolved mods root instead of ever accepting a path from the webview.

use std::path::{Path, PathBuf};

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    pub files: usize,
}

pub fn cache_dir(mods_root: &Path) -> Option<PathBuf> {
    mods_root.parent().map(|parent| parent.join("cache"))
}

pub fn inspect(mods_root: &Path) -> Result<CacheInfo, String> {
    let Some(path) = cache_dir(mods_root) else {
        return Ok(CacheInfo::default());
    };
    let mut info = CacheInfo { path: path.to_string_lossy().into_owned(), ..Default::default() };
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(info),
        Err(e) => return Err(format!("couldn't inspect the game cache: {e}")),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err("the game cache path is not a normal folder".into());
    }
    info.exists = true;
    measure(&path, &mut info)?;
    Ok(info)
}

pub fn clear(mods_root: &Path) -> Result<CacheInfo, String> {
    let info = inspect(mods_root)?;
    if !info.exists {
        return Ok(info);
    }
    let path = Path::new(&info.path);
    // `inspect` rejected a symlink and a non-directory immediately before this removal.
    std::fs::remove_dir_all(path).map_err(|e| format!("couldn't clear the game cache: {e}"))?;
    Ok(info)
}

fn measure(dir: &Path, info: &mut CacheInfo) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("couldn't read the game cache: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("couldn't read the game cache: {e}"))?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("couldn't inspect the game cache: {e}"))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            measure(&path, info)?;
        } else if meta.is_file() {
            info.files += 1;
            info.bytes += meta.len();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mxb-gamecache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("mods")).unwrap();
        root
    }

    #[test]
    fn cache_is_a_sibling_of_the_resolved_mods_root() {
        assert_eq!(
            cache_dir(Path::new("C:/Users/rider/Documents/PiBoSo/MX Bikes/mods")),
            Some(PathBuf::from("C:/Users/rider/Documents/PiBoSo/MX Bikes/cache")),
        );
    }

    #[test]
    fn measures_and_clears_only_the_cache_folder() {
        let root = test_root("measure-clear");
        let cache = root.join("cache");
        std::fs::create_dir_all(cache.join("nested")).unwrap();
        std::fs::write(cache.join("first.bin"), [0_u8; 10]).unwrap();
        std::fs::write(cache.join("nested").join("second.bin"), [0_u8; 20]).unwrap();

        let info = inspect(&root.join("mods")).unwrap();
        assert_eq!(info.bytes, 30);
        assert_eq!(info.files, 2);
        assert!(root.join("mods").is_dir());

        let cleared = clear(&root.join("mods")).unwrap();
        assert_eq!(cleared.bytes, 30);
        assert!(!cache.exists());
        assert!(root.join("mods").is_dir());
        let _ = std::fs::remove_dir_all(root);
    }
}
