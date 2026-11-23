//! The workspace lockfile must not hold a version with a RustSec advisory we have acted on.
//! Read straight from `Cargo.lock`, so a careless `cargo update --precise` back down fails here.

use std::collections::HashMap;

/// (name, version, what depends on it) for every package in the lockfile.
fn packages() -> Vec<(String, String, Vec<String>)> {
    let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
        .expect("the workspace Cargo.lock");
    let mut all = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key} = \"")).and_then(|r| r.strip_suffix('"')))
                .map(str::to_string)
        };
        let deps = block
            .split("dependencies = [")
            .nth(1)
            .and_then(|d| d.split(']').next())
            .map(|d| d.lines().map(|l| l.trim().trim_matches(|c| c == '"' || c == ',').to_string()).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default();
        if let (Some(n), Some(v)) = (field("name"), field("version")) {
            all.push((n, v, deps));
        }
    }
    all
}

fn version(v: &str) -> (u64, u64, u64) {
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

#[test]
fn tls_stack_is_patched() {
    let floors: HashMap<&str, (u64, u64, u64)> =
        [("rustls", (0, 23, 46)), ("rustls-webpki", (0, 103, 16))].into_iter().collect();
    for (name, v, _) in packages() {
        if let Some(floor) = floors.get(name.as_str()) {
            // Only the 0.23 / 0.103 lines are the ones the advisories name a fix for.
            let have = version(&v);
            if (have.0, have.1) == (floor.0, floor.1) {
                assert!(have >= *floor, "{name} {v} is below the patched {floor:?}");
            }
        }
    }
}

#[test]
fn our_crates_use_a_patched_zip() {
    let pkgs = packages();
    let old: Vec<String> = pkgs
        .iter()
        .filter(|(n, v, _)| n == "zip" && version(v) < (9, 0, 5))
        .map(|(_, v, _)| format!("zip {v}"))
        .collect();
    for (name, _, deps) in &pkgs {
        for d in deps {
            // A dependency line is `zip` (only one version present) or `zip 4.6.1`.
            let hits_old = old.iter().any(|o| d == o) || (d == "zip" && !old.is_empty() && pkgs.iter().filter(|(n, ..)| n == "zip").count() == 1);
            // The Tauri updater unpacks only archives whose minisign signature it has already
            // checked, and pins its own zip; it is the one allowed exception.
            assert!(!hits_old || name == "tauri-plugin-updater", "{name} depends on {d}, which predates zip 9.0.5");
        }
    }
}
