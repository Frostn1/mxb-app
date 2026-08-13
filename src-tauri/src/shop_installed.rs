//! What each purchase actually put on disk, so the "Installed" badge is a fact.
//!
//! It used to compare the product name to library folder names, which routinely disagree —
//! `2022 ARL MX Round 1` ships as `2022.ARL.MX.RD01.pkz` and lands in a folder named after the
//! file. At install time we simply know, so we record it; the fuzzy match stays as the fallback
//! for anything installed before this existed. Best-effort: a badge isn't worth failing an
//! install over. Same shape as [`crate::soundmods::record`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
    /// Product name → the folder names its files were placed under.
    #[serde(default)]
    products: BTreeMap<String, Vec<String>>,
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("shop-installed.json")
}

fn load(dir: &Path) -> Store {
    match fs::read_to_string(store_path(dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

/// Note that `product` now owns `folders`.
///
/// Replaces rather than merges: a reinstall that lands somewhere new should not leave the old
/// location claimed forever, or the badge outlives the files.
pub fn record(dir: &Path, product: &str, folders: &[String]) -> anyhow::Result<()> {
    if product.trim().is_empty() || folders.is_empty() {
        return Ok(());
    }
    let mut store = load(dir);
    store
        .products
        .insert(product.to_string(), folders.to_vec());
    fs::create_dir_all(dir)?;
    fs::write(store_path(dir), serde_json::to_string_pretty(&store)?)?;
    Ok(())
}

/// Every product we have installed something for, with the folders it claims.
///
/// The caller checks those folders against what is actually on disk — a record is a claim about
/// the past, and files get deleted outside the app.
pub fn recorded(dir: &Path) -> BTreeMap<String, Vec<String>> {
    load(dir).products
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own. Named per test, not just per process: `cargo test` runs
    /// them in parallel threads, so a shared path means each one's `remove_dir_all` wipes the
    /// store another is midway through asserting on.
    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "frost-shopinstalled-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_record_round_trips() {
        let dir = tmp("roundtrip");
        record(&dir, "2022 ARL MX Round 1", &["2022.ARL.MX.RD01".to_string()]).unwrap();

        let back = recorded(&dir);
        assert_eq!(
            back.get("2022 ARL MX Round 1").map(Vec::as_slice),
            Some(["2022.ARL.MX.RD01".to_string()].as_slice()),
            "the folder a purchase installed to is what makes the badge exact"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A reinstall that lands elsewhere must not leave the old folder claimed, or the badge
    /// keeps lighting up for files that are no longer there.
    #[test]
    fn recording_again_replaces_rather_than_accumulates() {
        let dir = tmp("replace");
        record(&dir, "Track", &["old".to_string()]).unwrap();
        record(&dir, "Track", &["new".to_string()]).unwrap();

        assert_eq!(
            recorded(&dir).get("Track").map(Vec::as_slice),
            Some(["new".to_string()].as_slice())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Nothing here is worth failing an install over, and nothing here should invent a claim.
    #[test]
    fn empty_input_and_missing_files_are_survivable() {
        let dir = tmp("empty");
        assert!(recorded(&dir).is_empty(), "no file yet reads as no records");

        record(&dir, "", &["x".to_string()]).unwrap();
        record(&dir, "Track", &[]).unwrap();
        assert!(recorded(&dir).is_empty(), "neither is a real claim");

        fs::write(store_path(&dir), "not json").unwrap();
        assert!(recorded(&dir).is_empty(), "a corrupt store reads as empty");
        let _ = fs::remove_dir_all(&dir);
    }
}
