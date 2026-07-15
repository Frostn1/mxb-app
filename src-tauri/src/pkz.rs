//! Read the structure of an installed `.pkz` for the library cards.
//!
//! MX Bikes `.pkz` files come in two forms:
//! - **Plain ZIP** (starts with the `PK\x03\x04` local-file-header magic) — a
//!   community/free track or bike. We open it, read the `.ini` metadata, and
//!   pull the preview image the author declared.
//! - **GUID-locked / encrypted** (any other leading bytes) — paid or protected
//!   content tied to the game's key. These are *not* zips and can't be
//!   inspected, so we report `locked` and the card falls back to name + size.
//!
//! Parsing is cached to the app cache dir (keyed by path + mtime + size) so the
//! library stays snappy after the first look at each file.

use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tauri::Manager;
use walkdir::WalkDir;

/// Local-file-header magic that begins every real ZIP (and thus every
/// unlocked `.pkz`).
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// Longest edge of the generated preview thumbnail, in pixels. The card tile is
/// ~76px wide, so 192 stays crisp at 2× DPI while keeping the `data:` URI light.
const THUMB_MAX: u32 = 192;

/// Longest edge for the full-size preview shown in the library detail lightbox.
const PREVIEW_MAX: u32 = 1100;

/// Parsed structure of an installed `.pkz`, surfaced on the library card.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PkzMeta {
    /// GUID-locked/encrypted archive we can't open — card shows name + size only.
    pub locked: bool,
    /// Display name from `[info] name` (falls back to `short_name`).
    pub name: Option<String>,
    /// Author from `[ui] author`.
    pub author: Option<String>,
    /// Location/description from `[ui] location`.
    pub location: Option<String>,
    /// Track length in metres, from `[info] length`.
    pub length: Option<u32>,
    /// Reference altitude in metres, from `[info] altitude`.
    pub altitude: Option<i32>,
    /// Downscaled preview as a `data:image/png;base64,…` URI, when one is found.
    pub thumbnail: Option<String>,
}

/// On-disk cache entry: the parsed meta plus the file identity it was read from.
#[derive(Serialize, Deserialize)]
struct CacheEntry {
    mtime_ns: u128,
    size: u64,
    meta: PkzMeta,
}

/// Read `.pkz` structure, using the app cache dir when the file is unchanged.
pub fn read_meta_cached(app: &tauri::AppHandle, path: &str) -> Result<PkzMeta> {
    let file_meta = std::fs::metadata(path).with_context(|| format!("stat {path}"))?;
    let size = file_meta.len();
    let mtime_ns = file_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let cache_file = cache_path(app, path);
    if let Some(cf) = &cache_file {
        if let Ok(bytes) = std::fs::read(cf) {
            if let Ok(entry) = serde_json::from_slice::<CacheEntry>(&bytes) {
                if entry.mtime_ns == mtime_ns && entry.size == size {
                    return Ok(entry.meta);
                }
            }
        }
    }

    let meta = read_meta(Path::new(path))?;

    if let Some(cf) = &cache_file {
        if let Some(parent) = cf.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let entry = CacheEntry {
            mtime_ns,
            size,
            meta: meta.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&entry) {
            let _ = std::fs::write(cf, bytes);
        }
    }

    Ok(meta)
}

/// Cache-file path for a given source `.pkz`: `<cache>/pkz-meta/<hash>.json`.
fn cache_path(app: &tauri::AppHandle, source: &str) -> Option<PathBuf> {
    let dir = app.path().app_cache_dir().ok()?.join("pkz-meta");
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    Some(dir.join(format!("{:016x}.json", hasher.finish())))
}

/// Parse a single `.pkz` (or extracted folder) into its [`PkzMeta`]. Never
/// errors for a *locked* file — that's a normal, expected result.
pub fn read_meta(path: &Path) -> Result<PkzMeta> {
    Ok(inspect(path)?.0)
}

/// Full-resolution preview image (a `data:` URI, larger than the card
/// thumbnail) for the library detail view's lightbox. `None` when the archive
/// is locked or carries no image.
pub fn read_preview(path: &Path) -> Result<Option<String>> {
    let (_, image) = inspect(path)?;
    Ok(image.and_then(|(name, bytes)| make_thumbnail(&name, &bytes, PREVIEW_MAX)))
}

/// The top-level `.ini` entry (fewest path segments, then shortest) — the mod's
/// own; deeper ones belong to sub-variants.
fn top_ini_index(names: &[String]) -> Option<usize> {
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.to_ascii_lowercase().ends_with(".ini"))
        .min_by_key(|(_, n)| (n.matches('/').count(), n.len()))
        .map(|(i, _)| i)
}

fn dir_of(name: &str) -> String {
    name.rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default()
}

/// Parse a `.pkz` (zip) or extracted mod folder into its [`PkzMeta`] (with a
/// small card thumbnail) and hand back the chosen preview image's raw bytes so
/// a caller (e.g. the detail lightbox) can render it at a different size.
/// Dispatches on whether `path` is a directory; both arms reuse `parse_ini` /
/// `pick_image` so cards look identical either way.
fn inspect(path: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    if path.is_dir() {
        inspect_dir(path)
    } else {
        inspect_zip(path)
    }
}

fn inspect_zip(path: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    let mut file = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;

    // A real (unlocked) `.pkz` is a ZIP and starts with the local-file magic.
    // Anything else is GUID-locked/encrypted — inspectable only by the game.
    let mut magic = [0u8; 4];
    if file.read(&mut magic).unwrap_or(0) < 4 || magic != ZIP_MAGIC {
        return Ok((locked(), None));
    }
    file.seek(SeekFrom::Start(0))?;

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        // Had the magic but won't open (truncated/odd) — treat like locked.
        Err(_) => return Ok((locked(), None)),
    };

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    let mut meta = PkzMeta::default();
    let mut pic: Option<String> = None;
    let mut ini_dir = String::new();

    if let Some(idx) = top_ini_index(&names) {
        ini_dir = dir_of(&names[idx]);
        if let Ok(mut f) = archive.by_index(idx) {
            let mut bytes = Vec::new();
            if f.read_to_end(&mut bytes).is_ok() {
                parse_ini(&String::from_utf8_lossy(&bytes), &mut meta, &mut pic);
            }
        }
    }

    let mut image = None;
    if let Some(img_idx) = pick_image(&names, &ini_dir, pic.as_deref()) {
        if let Ok(mut f) = archive.by_index(img_idx) {
            let mut bytes = Vec::new();
            if f.read_to_end(&mut bytes).is_ok() {
                meta.thumbnail = make_thumbnail(&names[img_idx], &bytes, THUMB_MAX);
                image = Some((names[img_idx].clone(), bytes));
            }
        }
    }

    Ok((meta, image))
}

/// Directory equivalent of [`inspect_zip`] for extracted tracks/bikes that
/// aren't packed into a single `.pkz`.
fn inspect_dir(dir: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    // Loose files, relative to the folder. A few levels deep is plenty to find
    // the `.ini` and a preview without walking a whole track's asset tree.
    let mut rels: Vec<(String, PathBuf)> = Vec::new();
    for entry in WalkDir::new(dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(r) = entry.path().strip_prefix(dir) {
            rels.push((
                r.to_string_lossy().replace('\\', "/"),
                entry.path().to_path_buf(),
            ));
        }
    }
    let names: Vec<String> = rels.iter().map(|(n, _)| n.clone()).collect();

    let mut meta = PkzMeta::default();
    let mut pic: Option<String> = None;
    let mut ini_dir = String::new();

    if let Some(idx) = top_ini_index(&names) {
        ini_dir = dir_of(&names[idx]);
        if let Ok(bytes) = std::fs::read(&rels[idx].1) {
            parse_ini(&String::from_utf8_lossy(&bytes), &mut meta, &mut pic);
        }
    }

    let mut image = None;
    if let Some(img_idx) = pick_image(&names, &ini_dir, pic.as_deref()) {
        if let Ok(bytes) = std::fs::read(&rels[img_idx].1) {
            meta.thumbnail = make_thumbnail(&names[img_idx], &bytes, THUMB_MAX);
            image = Some((names[img_idx].clone(), bytes));
        }
    }

    Ok((meta, image))
}

fn locked() -> PkzMeta {
    PkzMeta {
        locked: true,
        ..Default::default()
    }
}

/// Parse the INI text, filling `[info]`/`[ui]` fields we care about.
fn parse_ini(text: &str, meta: &mut PkzMeta, pic: &mut Option<String>) {
    let mut section = String::new();
    let clean = |v: &str| {
        let v = v.trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = inner.trim().to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        match (section.as_str(), key.as_str()) {
            ("info", "name") => meta.name = clean(value),
            // Fall back to short_name only if a real name wasn't given.
            ("info", "short_name") => {
                if meta.name.is_none() {
                    meta.name = clean(value);
                }
            }
            ("info", "length") => meta.length = value.parse().ok().filter(|&l| l > 0),
            ("info", "altitude") => meta.altitude = value.parse().ok(),
            ("ui", "author") => meta.author = clean(value),
            ("ui", "location") => meta.location = clean(value),
            ("ui", "pic") => *pic = clean(value),
            _ => {}
        }
    }
}

/// Choose which archive entry to render as the thumbnail: the author-declared
/// `pic` first, then any decodable image (preferring ones that look like a
/// track/preview image).
fn pick_image(names: &[String], ini_dir: &str, pic: Option<&str>) -> Option<usize> {
    if let Some(pic) = pic {
        let want = join_entry(ini_dir, pic).to_ascii_lowercase();
        if let Some(i) = names.iter().position(|n| n.to_ascii_lowercase() == want) {
            return Some(i);
        }
    }

    // No usable `pic` — scan for an image, scoring "trackimage"/"preview"/"info"
    // names higher so we grab the intended preview over a random texture.
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| is_image(n))
        .max_by_key(|(_, n)| image_score(n))
        .map(|(i, _)| i)
}

/// Join an INI-relative `pic` onto the ini's directory inside the archive.
fn join_entry(dir: &str, pic: &str) -> String {
    let pic = pic.replace('\\', "/");
    if dir.is_empty() {
        pic
    } else {
        format!("{dir}/{pic}")
    }
}

fn is_image(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".png") || n.ends_with(".jpg") || n.ends_with(".jpeg") || n.ends_with(".tga") || n.ends_with(".bmp")
}

/// Rank candidate images so a real preview wins over incidental textures.
fn image_score(name: &str) -> i32 {
    let n = name.to_ascii_lowercase();
    let mut score = 0;
    if n.contains("trackimage") || n.contains("preview") {
        score += 30;
    }
    if n.contains("image") || n.contains("info") || n.contains("thumb") {
        score += 10;
    }
    // Browser-native formats are cheaper/safer to decode than TGA.
    if n.ends_with(".png") || n.ends_with(".jpg") || n.ends_with(".jpeg") {
        score += 2;
    }
    score
}

/// Decode `bytes` (handling TGA, which has no magic so it needs the explicit
/// decoder), downscale to a small PNG, and return a `data:` URI.
fn make_thumbnail(name: &str, bytes: &[u8], max: u32) -> Option<String> {
    let img = if name.to_ascii_lowercase().ends_with(".tga") {
        let dec = image::codecs::tga::TgaDecoder::new(Cursor::new(bytes)).ok()?;
        image::DynamicImage::from_decoder(dec).ok()?
    } else {
        image::load_from_memory(bytes).ok()?
    };

    // Track previews are photographic, so JPEG is far smaller than PNG. Drop to
    // RGB first (JPEG can't hold the alpha a TGA may decode to).
    let thumb = image::DynamicImage::ImageRgb8(img.thumbnail(max, max).to_rgb8());
    let mut jpg = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
        .ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
    Some(format!("data:image/jpeg;base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_info_and_ui_sections() {
        let ini = "[info]\nname = FLRMX\nshort_name = FLR\nlength = 1235\naltitude = 67\n\n[ui]\npic = TrackImage.tga\nauthor = Mack\nlocation = Florida\n";
        let mut meta = PkzMeta::default();
        let mut pic = None;
        parse_ini(ini, &mut meta, &mut pic);
        assert_eq!(meta.name.as_deref(), Some("FLRMX"));
        assert_eq!(meta.length, Some(1235));
        assert_eq!(meta.altitude, Some(67));
        assert_eq!(meta.author.as_deref(), Some("Mack"));
        assert_eq!(meta.location.as_deref(), Some("Florida"));
        assert_eq!(pic.as_deref(), Some("TrackImage.tga"));
    }

    #[test]
    fn short_name_only_fills_when_name_absent() {
        let mut meta = PkzMeta::default();
        let mut pic = None;
        parse_ini("[info]\nshort_name = OnlyShort\n", &mut meta, &mut pic);
        assert_eq!(meta.name.as_deref(), Some("OnlyShort"));
    }

    #[test]
    fn zero_length_is_dropped() {
        let mut meta = PkzMeta::default();
        let mut pic = None;
        parse_ini("[info]\nlength = 0\n", &mut meta, &mut pic);
        assert_eq!(meta.length, None);
    }

    #[test]
    fn pic_joins_onto_ini_dir_and_normalizes_slashes() {
        assert_eq!(join_entry("FLRMX", "TrackImage.tga"), "FLRMX/TrackImage.tga");
        assert_eq!(join_entry("", "x.png"), "x.png");
        assert_eq!(join_entry("A", "sub\\y.tga"), "A/sub/y.tga");
    }

    #[test]
    fn declared_pic_is_matched_case_insensitively() {
        let names = vec![
            "FLRMX/FLRMX.ini".to_string(),
            "FLRMX/TrackImage.PNG".to_string(),
            "FLRMX/FLRMX.map".to_string(),
        ];
        assert_eq!(pick_image(&names, "FLRMX", Some("trackimage.png")), Some(1));
    }

    #[test]
    fn falls_back_to_best_scoring_image() {
        let names = vec![
            "T/T.map".to_string(),
            "T/road.tga".to_string(),
            "T/TrackImage.png".to_string(),
        ];
        assert_eq!(pick_image(&names, "T", None), Some(2));
    }

    #[test]
    fn no_image_returns_none() {
        let names = vec!["T/T.ini".to_string(), "T/T.map".to_string()];
        assert_eq!(pick_image(&names, "T", None), None);
    }
}
