use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
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

pub fn known_bikes(dir: &Path) -> Vec<String> {
    load(dir).bikes.into_keys().collect()
}
