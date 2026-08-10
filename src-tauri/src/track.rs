//! Reading a track well enough to show it.
//!
//! A track is the largest thing the app handles — a few hundred megabytes of textures and
//! scenery around a terrain grid that is the only part worth drawing. Everything here is
//! arranged so that showing one costs the grid and nothing else:
//!
//! * The inventory ([`read_info`]) never inflates a byte. Naming a track's parts is a
//!   question the archive's index already answers, and answering it that way is the
//!   difference between a view that opens at once and one that waits on a 400 MB unpack.
//! * The terrain is inflated once, probed once, and reduced immediately to a master grid
//!   small enough to keep ([`MASTER_DIM`]). Every level of detail the viewer asks for after
//!   that is resampled from the master, in memory.
//! * The master is written to disk, so opening the same track tomorrow skips the inflate and
//!   the probe both. This is the cache that matters: the probe is milliseconds, while pulling
//!   a heightfield out of a large archive is most of a second.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::heightfield::{self, Layout};

/// The resolution the master grid is kept at. Chosen to be more than any view resolves —
/// the viewer draws at most a quarter of this across — while staying a megabyte on disk, so
/// caching every track a player looks at stays bounded.
const MASTER_DIM: u32 = 512;

/// Cache generation. Bump when the blob layout or the probe changes shape, so entries
/// written by an older build are ignored rather than misread.
const CACHE_DIR: &str = "track-terrain-v1";

/// How many cached masters to keep. At roughly a megabyte each this is a bounded cost that
/// only grows with tracks actually opened.
const CACHE_KEEP: usize = 64;

/// Files that carry a terrain grid, best first. The probe validates whatever it is handed,
/// so trying several costs only the read of the ones that don't pan out.
const HEIGHTFIELD_EXTS: [&str; 3] = ["trh", "map", "hf"];

/// What a track file is for, as far as the UI is concerned. A key, not prose — the app
/// translates it.
fn role_of(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "trh" | "hf" => "heightfield",
        "map" => "terrain",
        "tsc" => "scenery",
        "rdf" => "road",
        "ssc" => "surfaces",
        "ini" | "cfg" => "config",
        "edf" => "model",
        "jpg" | "jpeg" | "png" | "dds" | "tga" | "bmp" => "image",
        "wav" | "ogg" => "sound",
        _ => "other",
    }
}

/// One file inside a track.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackFile {
    pub name: String,
    pub role: &'static str,
}

/// The shape of a track's terrain, once recovered.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TerrainInfo {
    /// Master grid dimensions — what the viewer can ask for, not the file's own resolution.
    pub width: u32,
    pub height: u32,
    /// The file's resolution before reduction, so the UI can say what it actually read.
    pub source_width: u32,
    pub source_height: u32,
    pub min_height: f32,
    pub max_height: f32,
    /// Metres covered by one master sample. Assumed when the track doesn't say (see
    /// `scale_known`) — the relief is still true, only its footprint is a guess.
    pub metres_per_sample: f32,
    pub scale_known: bool,
    /// 0–1, from the probe. Anything recovered rather than stated is shown as inferred.
    pub confidence: f32,
    /// `header`, `ini` or `square` — how the grid's shape was decided.
    pub source: String,
    /// The archive entry it came out of.
    pub entry: String,
}

/// Everything the track view needs before it draws anything.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub meta: crate::pkz::PkzMeta,
    pub files: Vec<TrackFile>,
    /// Whether a heightfield-looking entry is present at all. Lets the view offer the 3D tab
    /// without first paying for the terrain, and say "no terrain in this track" when there
    /// genuinely isn't one rather than after a failed load.
    pub has_terrain: bool,
}

/// A decoded terrain, reduced to the master resolution.
#[derive(Clone)]
pub struct Master {
    pub info: TerrainInfo,
    pub heights: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Reading a track's parts, whether it's an archive or an unpacked folder
// ---------------------------------------------------------------------------

fn is_dir(path: &Path) -> bool {
    path.is_dir()
}

/// Every entry name in a track, without inflating any of them.
fn entry_names(path: &Path) -> Result<Vec<String>> {
    if is_dir(path) {
        let mut out = Vec::new();
        for entry in crate::linkwalk::walk_depth(path, 6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(path) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        return Ok(out);
    }
    crate::pkz::entry_names(path)
}

/// Pull one named entry's bytes out of a track.
fn read_entry(path: &Path, name: &str) -> Result<Vec<u8>> {
    if is_dir(path) {
        return std::fs::read(path.join(name)).with_context(|| format!("read {name}"));
    }
    let want = name.to_ascii_lowercase();
    let found = crate::pkz::read_selected(path, |n| {
        n.replace('\\', "/").to_ascii_lowercase() == want
    })?;
    match found.into_iter().next() {
        Some((_, bytes)) => Ok(bytes),
        None => bail!("{name} is not in {path:?}"),
    }
}

/// The entries that could hold a terrain grid, best-looking first.
fn heightfield_entries(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names
        .iter()
        .filter(|n| {
            let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            HEIGHTFIELD_EXTS.contains(&ext.as_str())
        })
        .cloned()
        .collect();
    out.sort_by_key(|n| {
        let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        HEIGHTFIELD_EXTS
            .iter()
            .position(|e| *e == ext)
            .unwrap_or(usize::MAX)
    });
    out
}

/// Grid dimensions and sample spacing named in the track's `.ini`, when it names them.
///
/// Nothing here is required — it's a hint handed to the probe, which still has to satisfy
/// itself that the shape reads as terrain. Keys vary between track tools, so this stays
/// deliberately loose about where it finds them.
fn ini_hints(text: &str) -> (Option<(u32, u32)>, Option<f32>) {
    let mut w: Option<u32> = None;
    let mut h: Option<u32> = None;
    let mut square: Option<u32> = None;
    let mut scale: Option<f32> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "width" | "nx" | "cx" | "xsize" | "size_x" => w = value.parse().ok(),
            "height" | "ny" | "cy" | "ysize" | "size_y" => h = value.parse().ok(),
            // A single figure means a square grid.
            "size" | "resolution" | "grid" => square = value.parse().ok(),
            "scale" | "step" | "spacing" | "cell" | "cellsize" => {
                scale = value.parse().ok().filter(|v: &f32| *v > 0.0 && *v < 1000.0)
            }
            _ => {}
        }
    }

    let dims = match (w, h) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => square.map(|s| (s, s)),
    };
    (dims, scale)
}

/// The track's top-level `.ini` text, if it has one worth reading.
fn ini_text(path: &Path, names: &[String]) -> Option<String> {
    let idx = names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.to_ascii_lowercase().ends_with(".ini"))
        .min_by_key(|(_, n)| (n.matches('/').count(), n.len()))
        .map(|(i, _)| i)?;
    let bytes = read_entry(path, &names[idx]).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

// ---------------------------------------------------------------------------
// The two things the app asks for
// ---------------------------------------------------------------------------

/// A track's metadata and contents, without inflating anything.
pub fn read_info(app: &tauri::AppHandle, path: &str) -> Result<TrackInfo> {
    let p = Path::new(path);
    let meta = crate::pkz::read_meta_cached(app, path)?;
    let names = entry_names(p).unwrap_or_default();
    let has_terrain = !heightfield_entries(&names).is_empty();

    let mut files: Vec<TrackFile> = names
        .into_iter()
        .map(|name| {
            let role = role_of(&name);
            TrackFile { name, role }
        })
        .collect();
    files.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));

    Ok(TrackInfo {
        meta,
        files,
        has_terrain,
    })
}

/// Decode a track's terrain to the master grid, going to disk and to the archive only when
/// nothing nearer has it.
pub fn load_master(app: &tauri::AppHandle, path: &str) -> Result<Master> {
    let stamp = stamp(path)?;
    let key = cache_key(path, stamp);

    if let Some(hit) = memory_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        return Ok(hit);
    }
    if let Some(hit) = cache_file(app, &key).and_then(|f| read_cache(&f)) {
        remember(key, hit.clone());
        return Ok(hit);
    }

    let master = decode_master(Path::new(path))?;
    if let Some(f) = cache_file(app, &key) {
        write_cache(&f, &master);
        prune_cache(app);
    }
    remember(key, master.clone());
    Ok(master)
}

/// The expensive path: inflate a heightfield, work out its layout, reduce it.
fn decode_master(path: &Path) -> Result<Master> {
    let names = entry_names(path)?;
    let candidates = heightfield_entries(&names);
    if candidates.is_empty() {
        bail!("no heightfield in {path:?}");
    }

    let ini = ini_text(path, &names);
    let (hint, ini_scale) = ini.as_deref().map(ini_hints).unwrap_or((None, None));

    for entry in candidates {
        let Ok(bytes) = read_entry(path, &entry) else {
            continue;
        };
        let Some(layout) = heightfield::probe(&bytes, hint) else {
            log::debug!("[track] {entry} in {path:?} doesn't read as a heightfield");
            continue;
        };
        return Ok(build_master(&bytes, &layout, &entry, ini_scale));
    }

    bail!("nothing in {path:?} reads as a terrain grid")
}

fn build_master(bytes: &[u8], layout: &Layout, entry: &str, ini_scale: Option<f32>) -> Master {
    let (w, h, heights) = heightfield::read_grid(bytes, layout, MASTER_DIM);

    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for v in &heights {
        min = min.min(*v);
        max = max.max(*v);
    }

    // The master covers the same ground as the source at fewer samples, so the spacing grows
    // by exactly the reduction factor.
    let reduction = layout.width as f32 / w.max(1) as f32;
    let metres_per_sample = ini_scale.unwrap_or(1.0) * reduction;

    Master {
        info: TerrainInfo {
            width: w,
            height: h,
            source_width: layout.width,
            source_height: layout.height,
            min_height: if min.is_finite() { min } else { 0.0 },
            max_height: if max.is_finite() { max } else { 0.0 },
            metres_per_sample,
            scale_known: ini_scale.is_some(),
            confidence: layout.confidence,
            source: layout.source.to_string(),
            entry: entry.to_string(),
        },
        heights,
    }
}

/// Resample a master down for a view that wants fewer points, and pack it for the IPC
/// channel. See [`BLOB_HEADER`] for the layout the app unpacks.
pub fn terrain_blob(master: &Master, max_dim: u32) -> Vec<u8> {
    let (w, h, heights) = resample(master, max_dim);

    let mut out = Vec::with_capacity(BLOB_HEADER + heights.len() * 4);
    out.extend_from_slice(b"FTRN");
    out.extend_from_slice(&1u16.to_le_bytes()); // version
    // Bit 0: the sample spacing was stated by the track rather than assumed. The app needs
    // this to caption the view honestly — with the spacing assumed, the relief is drawn
    // against a footprint we guessed, so its steepness is not something to trust.
    out.extend_from_slice(&(u16::from(master.info.scale_known)).to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&master.info.min_height.to_le_bytes());
    out.extend_from_slice(&master.info.max_height.to_le_bytes());
    // Spacing scales with the reduction, so the terrain keeps its real footprint at any
    // level of detail — a coarse pass and a fine one have to sit in the same place.
    let spacing = master.info.metres_per_sample * (master.info.width as f32 / w.max(1) as f32);
    out.extend_from_slice(&spacing.to_le_bytes());
    // How sure the probe was of the layout, so a marginal read can say so rather than be
    // presented as the track's terrain.
    out.extend_from_slice(&master.info.confidence.to_le_bytes());
    for v in &heights {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Bytes before the grid in a terrain blob. The grid starts 4-byte aligned so the app can
/// read it as a `Float32Array` without copying it first.
pub const BLOB_HEADER: usize = 32;

fn resample(master: &Master, max_dim: u32) -> (u32, u32, Vec<f32>) {
    let w = master.info.width as usize;
    let h = master.info.height as usize;
    let step = ((w.max(h) as f32) / max_dim.max(1) as f32).ceil().max(1.0) as usize;
    if step <= 1 {
        return (master.info.width, master.info.height, master.heights.clone());
    }

    let out_w = w.div_ceil(step);
    let out_h = h.div_ceil(step);
    let mut out = Vec::with_capacity(out_w * out_h);
    for oy in 0..out_h {
        for ox in 0..out_w {
            let mut total = 0.0f64;
            let mut count = 0usize;
            for y in oy * step..((oy + 1) * step).min(h) {
                for x in ox * step..((ox + 1) * step).min(w) {
                    total += master.heights[y * w + x] as f64;
                    count += 1;
                }
            }
            out.push(if count == 0 {
                0.0
            } else {
                (total / count as f64) as f32
            });
        }
    }
    (out_w as u32, out_h as u32, out)
}

// ---------------------------------------------------------------------------
// Caches
// ---------------------------------------------------------------------------

/// Keyed on name, size and mtime rather than the full path, for the same reason `pkz` is:
/// pointing the app at a moved install shouldn't cold-start every track it has ever opened.
#[derive(Clone, Copy)]
struct Stamp {
    size: u64,
    mtime_ns: u128,
}

fn stamp(path: &str) -> Result<Stamp> {
    let m = std::fs::metadata(path).with_context(|| format!("stat {path}"))?;
    Ok(Stamp {
        size: m.len(),
        mtime_ns: m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    })
}

fn cache_key(path: &str, stamp: Stamp) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{name}:{}:{}", stamp.size, stamp.mtime_ns)
}

fn memory_cache() -> &'static std::sync::Mutex<crate::lru::Lru<Master>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<crate::lru::Lru<Master>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(crate::lru::Lru::new(3)))
}

fn remember(key: String, master: Master) {
    if let Ok(mut c) = memory_cache().lock() {
        c.insert(key, master);
    }
}

fn cache_file(app: &tauri::AppHandle, key: &str) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_cache_dir().ok()?.join(CACHE_DIR);
    // A key holds a file name and two numbers; anything that isn't safe in a path is
    // flattened rather than escaped, since collisions are caught by the header check.
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Some(dir.join(format!("{safe}.bin")))
}

/// On-disk master: the terrain descriptor as JSON, then the raw grid.
fn write_cache(file: &Path, master: &Master) {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(head) = serde_json::to_vec(&master.info) else {
        return;
    };
    let mut out = Vec::with_capacity(8 + head.len() + master.heights.len() * 4);
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    for v in &master.heights {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let _ = std::fs::write(file, out);
}

fn read_cache(file: &Path) -> Option<Master> {
    let bytes = std::fs::read(file).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    let head_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let grid_at = 4 + head_len;
    if bytes.len() < grid_at {
        return None;
    }
    let info: TerrainInfo = serde_json::from_slice(&bytes[4..grid_at]).ok()?;

    let expected = info.width as usize * info.height as usize;
    let grid = &bytes[grid_at..];
    if grid.len() != expected * 4 {
        return None;
    }
    let heights = grid
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Some(Master { info, heights })
}

/// Keep the cache from growing without limit. Oldest first, since the newest are the tracks
/// being looked at now.
fn prune_cache(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Ok(base) = app.path().app_cache_dir() else {
        return;
    };
    let dir = base.join(CACHE_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let modified = e.metadata().ok()?.modified().ok()?;
            p.is_file().then_some((modified, p))
        })
        .collect();
    if files.len() <= CACHE_KEEP {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    for (_, p) in files.iter().take(files.len() - CACHE_KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_come_from_the_extension() {
        assert_eq!(role_of("Hangtown/Hangtown.trh"), "heightfield");
        assert_eq!(role_of("Hangtown/Hangtown.tsc"), "scenery");
        assert_eq!(role_of("Hangtown/Hangtown.ini"), "config");
        assert_eq!(role_of("Hangtown/preview.jpg"), "image");
        assert_eq!(role_of("Hangtown/notes.txt"), "other");
    }

    #[test]
    fn heightfields_are_offered_best_first() {
        let names = vec![
            "T/T.map".to_string(),
            "T/T.ini".to_string(),
            "T/T.trh".to_string(),
        ];
        assert_eq!(heightfield_entries(&names), vec!["T/T.trh", "T/T.map"]);
    }

    #[test]
    fn a_track_with_no_heightfield_offers_nothing() {
        let names = vec!["T/T.ini".to_string(), "T/preview.jpg".to_string()];
        assert!(heightfield_entries(&names).is_empty());
    }

    #[test]
    fn ini_hints_read_paired_dimensions() {
        let (dims, scale) = ini_hints("[terrain]\nnx = 512\nny = 256\nscale = 0.5\n");
        assert_eq!(dims, Some((512, 256)));
        assert_eq!(scale, Some(0.5));
    }

    #[test]
    fn ini_hints_read_a_single_square_size() {
        let (dims, _) = ini_hints("[terrain]\nsize=1024\n");
        assert_eq!(dims, Some((1024, 1024)));
    }

    #[test]
    fn ini_hints_ignore_a_track_that_says_nothing() {
        let (dims, scale) = ini_hints("[info]\nname = Hangtown\n");
        assert_eq!(dims, None);
        assert_eq!(scale, None);
    }

    #[test]
    fn ini_hints_reject_an_absurd_scale() {
        let (_, scale) = ini_hints("[terrain]\nscale = 99999\n");
        assert_eq!(scale, None, "a kilometre-wide sample is a misread, not a hint");
    }

    /// A master built from a known grid, for the blob and resample tests.
    fn master(dim: u32) -> Master {
        let heights: Vec<f32> = (0..dim * dim)
            .map(|i| ((i % dim) as f32 / dim as f32) * 10.0)
            .collect();
        Master {
            info: TerrainInfo {
                width: dim,
                height: dim,
                source_width: dim * 2,
                source_height: dim * 2,
                min_height: 0.0,
                max_height: 10.0,
                metres_per_sample: 2.0,
                scale_known: true,
                confidence: 0.9,
                source: "header".into(),
                entry: "T/T.trh".into(),
            },
            heights,
        }
    }

    #[test]
    fn a_blob_carries_its_grid_behind_an_aligned_header() {
        let m = master(64);
        let blob = terrain_blob(&m, 64);
        assert_eq!(&blob[0..4], b"FTRN");
        assert_eq!(BLOB_HEADER % 4, 0, "the grid has to be readable in place");
        assert_eq!(blob.len(), BLOB_HEADER + 64 * 64 * 4);

        let w = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
        let h = u32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
        assert_eq!((w, h), (64, 64));
    }

    #[test]
    fn a_blob_carries_how_far_the_terrain_was_inferred() {
        let mut m = master(64);
        let known = terrain_blob(&m, 64);
        assert_eq!(u16::from_le_bytes([known[6], known[7]]) & 1, 1);
        assert!((f32::from_le_bytes([known[28], known[29], known[30], known[31]]) - 0.9).abs() < 1e-6);

        // A track that never stated its sample spacing has to come through as such — the
        // relief is real, the footprint it's drawn against is a guess.
        m.info.scale_known = false;
        let assumed = terrain_blob(&m, 64);
        assert_eq!(u16::from_le_bytes([assumed[6], assumed[7]]) & 1, 0);
    }

    #[test]
    fn a_coarse_blob_keeps_the_terrain_the_same_size_on_the_ground() {
        let m = master(64);
        let fine = terrain_blob(&m, 64);
        let coarse = terrain_blob(&m, 16);

        let spacing = |b: &[u8]| f32::from_le_bytes([b[24], b[25], b[26], b[27]]);
        let dims = |b: &[u8]| u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as f32;

        // Fewer samples, proportionally further apart — the footprint is unchanged, which is
        // what lets a coarse pass be replaced by a fine one without the terrain moving.
        assert!((dims(&fine) * spacing(&fine) - dims(&coarse) * spacing(&coarse)).abs() < 0.01);
    }

    #[test]
    fn resampling_below_the_master_is_a_no_op() {
        let m = master(64);
        let (w, h, grid) = resample(&m, 256);
        assert_eq!((w, h), (64, 64));
        assert_eq!(grid.len(), 64 * 64);
    }

    #[test]
    fn a_cached_master_round_trips() {
        let dir = std::env::temp_dir().join(format!("frost-track-cache-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("m.bin");

        let m = master(32);
        write_cache(&file, &m);
        let back = read_cache(&file).expect("a master just written should read back");

        assert_eq!(back.info.width, m.info.width);
        assert_eq!(back.info.entry, m.info.entry);
        assert_eq!(back.heights, m.heights);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_cache_entry_is_ignored() {
        let dir = std::env::temp_dir().join(format!("frost-track-trunc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("m.bin");

        let m = master(32);
        write_cache(&file, &m);
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 16]).unwrap();

        assert!(
            read_cache(&file).is_none(),
            "a short grid must be re-decoded, not read past",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
