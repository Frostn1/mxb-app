//! Model-swap identities carried by PaintSync and handed to FrostMod.

use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_FILES: usize = 64;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
pub const MAX_RIDERS: usize = 64;
pub const MAX_BIKES: usize = 16;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderModels {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helmet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boots: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bikes: Vec<BikeModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeModel {
    pub bike_id: String,
    pub swap: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    path: PathBuf,
    size: u64,
    mtime: u128,
}

#[derive(Clone)]
struct Cached {
    files: Vec<Stamp>,
    id: String,
}

static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();

fn stamp(path: PathBuf) -> Option<Stamp> {
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(Stamp {
        path,
        size: meta.len(),
        mtime,
    })
}

fn hex16(hash: impl AsRef<[u8]>) -> String {
    hash.as_ref()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn cached_id<F>(key: String, files: Vec<(String, Stamp)>, prefix: &[u8], build: F) -> Option<String>
where
    F: FnOnce(&mut Sha256),
{
    let stamps: Vec<Stamp> = files.iter().map(|(_, s)| s.clone()).collect();
    if let Some(hit) = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .filter(|hit| hit.files == stamps)
    {
        return Some(hit.id);
    }

    let mut hash = Sha256::new();
    hash.update(prefix);
    build(&mut hash);
    let mut buf = [0u8; 64 * 1024];
    for (name, file) in &files {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(file.size.to_le_bytes());
        let mut input = std::fs::File::open(&file.path).ok()?;
        loop {
            let n = input.read(&mut buf).ok()?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
    }
    let id = hex16(hash.finalize());
    if let Ok(mut cache) = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        cache.insert(
            key,
            Cached {
                files: stamps,
                id: id.clone(),
            },
        );
    }
    Some(id)
}

fn top_files(
    dir: &Path,
    accepts: impl Fn(&str) -> bool,
    gear_limits: bool,
) -> Option<Vec<(String, Stamp)>> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_str()?.to_ascii_lowercase();
        if !accepts(&name) {
            continue;
        }
        // Top-level files only: a folder that happens to be named `x.dds` is not part of the set.
        let Some(file) = stamp(entry.path()) else { continue };
        if gear_limits && file.size > MAX_FILE {
            return None;
        }
        // Bike IdBuilder canonicalizes to unique lowercase names. GearHasher hashes the
        // complete canonical list (case-colliding files cannot normally exist on Windows).
        if !gear_limits && !seen.insert(name.clone()) {
            continue;
        }
        files.push((name, file));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    (!gear_limits
        || (files.len() <= MAX_FILES
            && files.iter().map(|(_, f)| f.size).sum::<u64>() <= MAX_TOTAL))
        .then_some(files)
}

/// FrostMod's `ridersync::GearHasher`, over one loose gear folder.
pub fn gear_id(part: &str, dir: &Path) -> Option<String> {
    if !matches!(part, "rider" | "helmet" | "boots") || !dir.is_dir() {
        return None;
    }
    let visual = ["edf", "hrc", "dds", "tga", "png", "jpg", "bmp"];
    let files = top_files(
        dir,
        |name| {
            name == "gfx.cfg"
                || name
                    .rsplit_once('.')
                    .is_some_and(|(_, ext)| visual.contains(&ext))
        },
        true,
    )?;
    if !files.iter().any(|(name, _)| name.ends_with(".edf")) {
        return None;
    }
    let key = format!("gear:{part}:{}", dir.to_string_lossy());
    cached_id(key, files, b"frostmod-gear-v1\0", |hash| {
        hash.update(part.as_bytes());
        hash.update([0]);
    })
}

/// FrostMod's `bikesync::IdBuilder`, over a bike's active loose model set.
pub fn bike_swap_id(bike: &str, dir: &Path) -> Option<String> {
    let bike = bike.trim();
    if bike.is_empty() || !dir.is_dir() {
        return None;
    }
    let files = top_files(
        dir,
        |name| matches!(name, "model.edf" | "model_s.edf" | "gfx.cfg") || name.ends_with(".hrc"),
        false,
    )?;
    let lower = bike.to_ascii_lowercase();
    let key = format!("bike:{lower}:{}", dir.to_string_lossy());
    cached_id(key, files, lower.as_bytes(), |hash| hash.update([0]))
}

/// Model ids for the gear and bikes this profile can actually wear.
pub fn own_models(cfg: &AppConfig, profile: &str) -> RiderModels {
    let profiles = cfg.profiles_dir();
    let Ok(mut loadouts) = crate::presets::read_all_loadouts(&profiles, profile) else {
        return RiderModels::default();
    };
    if let Some(active) = crate::presets::active_bike(&profiles, profile) {
        if let Some(at) = loadouts
            .iter()
            .position(|(bike, _)| bike.eq_ignore_ascii_case(&active))
        {
            loadouts.swap(0, at);
        }
    }
    let mods = crate::library::mods_root(&cfg.mods_path);
    let gear = loadouts.first().map(|(_, look)| look);
    let mut out = RiderModels::default();
    if let Some(look) = gear {
        for (part, area, name) in [
            ("rider", "riders", look.rider.trim()),
            ("helmet", "helmets", look.helmet.trim()),
            ("boots", "boots", look.boots.trim()),
        ] {
            if name.is_empty() {
                continue;
            }
            let id = gear_id(part, &mods.join("rider").join(area).join(name));
            match part {
                "rider" => out.rider = id,
                "helmet" => out.helmet = id,
                "boots" => out.boots = id,
                _ => {}
            }
        }
    }
    for (bike, _) in loadouts {
        if out.bikes.len() >= MAX_BIKES || crate::library::is_frostmod_shadow_bike(&bike) {
            continue;
        }
        let dir = mods.join("bikes").join(&bike);
        if !dir.join(crate::library::LIB_DIR).is_dir() {
            continue;
        }
        if let Some(swap) = bike_swap_id(&bike, &dir) {
            out.bikes.push(BikeModel {
                bike_id: bike,
                swap,
            });
        }
    }
    out
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn encoded(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let safe = byte.is_ascii_alphanumeric() || b" _.~()[]-".contains(byte);
        if safe && *byte != b'%' {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Encode the complete replacing LOOKS set FrostMod consumes.
pub fn encode_rider_looks<'a>(
    riders: impl IntoIterator<Item = (&'a str, &'a RiderModels)>,
) -> String {
    let mut riders: Vec<_> = riders.into_iter().collect();
    riders.sort_by_key(|(name, _)| name.trim().to_ascii_lowercase());
    riders
        .into_iter()
        .take(MAX_RIDERS)
        .map(|(name, models)| {
            // A malformed id is sent as empty, which FrostMod treats as stock for that part.
            let id = |value: &Option<String>| -> String {
                value.as_deref().filter(|v| valid_id(v)).unwrap_or("").to_string()
            };
            let mut fields = vec![
                encoded(name.trim()),
                id(&models.rider),
                id(&models.helmet),
                id(&models.boots),
            ];
            fields.extend(
                models
                    .bikes
                    .iter()
                    .take(MAX_BIKES)
                    .filter(|b| valid_id(&b.swap))
                    .map(|b| format!("{}:{}", encoded(b.bike_id.trim()), b.swap)),
            );
            fields.join("|")
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("mxb-modelsync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn gear_vector_matches_frostmod() {
        let dir = temp("gear-vector");
        std::fs::write(dir.join("model.edf"), b"abc").unwrap();
        std::fs::write(dir.join("helmet.dds"), b"0123456789").unwrap();
        // Mirrored in FrostMod's tests/ridersync_test.cpp.
        assert_eq!(
            gear_id("helmet", &dir).as_deref(),
            Some("87aceb9d2ac61ec6d2528e213a303cc5")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bike_vector_matches_frostmod() {
        let dir = temp("bike-vector");
        std::fs::write(dir.join("model.edf"), b"abc").unwrap();
        std::fs::write(dir.join("gfx.cfg"), b"x=1").unwrap();
        // Mirrored in FrostMod's tests/ridersync_test.cpp.
        assert_eq!(
            bike_swap_id("TestBike", &dir).as_deref(),
            Some("a3ce7f3829886ed93fde62091278ff9d")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn looks_use_the_contract_encoding_and_limits() {
        let models = RiderModels {
            helmet: Some("a".repeat(32)),
            bikes: vec![BikeModel {
                bike_id: "Bike:One".into(),
                swap: "b".repeat(32),
            }],
            ..Default::default()
        };
        assert_eq!(
            encode_rider_looks([(" A|B;雪 ", &models)]),
            format!(
                "A%7CB%3B%E9%9B%AA||{}||Bike%3AOne:{}",
                "a".repeat(32),
                "b".repeat(32)
            )
        );
    }
}
