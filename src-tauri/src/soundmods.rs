//! Provenance for installed **sound mods**.
//!
//! A sound mod (`engine.scl` + `sfx.cfg` + samples) merges *into* an OEM bike
//! folder, so on disk it's indistinguishable from a stock bike — it can't be
//! detected by inspection alone. Instead we record, at install time, which bike
//! folders a sound wrote into, and the Library surfaces exactly those (still
//! cross-checked against disk) as `sound` entries. The store lives in the app's
//! local data dir as `sound-mods.json`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
    /// Bike folder name (e.g. `MX2OEM_2023_KTM_250_SX-F`) → the mod that set its
    /// sound (a slug/title, kept for display).
    #[serde(default)]
    bikes: BTreeMap<String, String>,
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("sound-mods.json")
}

fn load(dir: &Path) -> Store {
    match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

/// Record that `mod_name` installed a sound into each of `bikes` (by folder name).
/// No-op for an empty list.
pub fn record(dir: &Path, bikes: &[String], mod_name: &str) -> anyhow::Result<()> {
    if bikes.is_empty() {
        return Ok(());
    }
    let mut store = load(dir);
    for b in bikes {
        store.bikes.insert(b.clone(), mod_name.to_string());
    }
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(&store)?)?;
    Ok(())
}

/// Bike folder names known to carry an installed sound mod (unpruned — the caller
/// cross-checks against what's actually on disk).
pub fn known_bikes(dir: &Path) -> Vec<String> {
    load(dir).bikes.into_keys().collect()
}
