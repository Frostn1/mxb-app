use anyhow::{bail, Context, Result};
use base64::Engine;
use image::ImageDecoder;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, Once, OnceLock};
use tauri::Manager;
use walkdir::WalkDir;

/// ZIP local-file-header magic ("PK\x03\x04").
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// Longest edge of the card thumbnail, in pixels.
const THUMB_MAX: u32 = 192;

/// Longest edge of the full-size preview, in pixels.
const PREVIEW_MAX: u32 = 1100;

/// Ceiling on what a single preview decode may allocate. Track previews are often
/// uncompressed TGAs, and one oversized file shouldn't be able to claim hundreds of
/// megabytes just to end up as a 192px thumbnail.
const MAX_DECODE_BYTES: u64 = 64 * 1024 * 1024;

/// Widest/tallest preview we'll decode. Anything bigger is a mistake in the mod, not
/// something a card needs.
const MAX_DECODE_EDGE: u32 = 8192;

// ===========================================================================
// Inspection gate
//
// Cracking a `.pkz` open is expensive: a seek-heavy read off disk, then a preview
// image decoded to a full-size bitmap before it's downscaled. The Library renders a
// card per installed mod and every card asks for its metadata at once, so a large
// collection would otherwise fan out into hundreds of concurrent blocking tasks —
// each holding tens of megabytes of decoded pixels and competing for the same disk.
// That is enough to push the whole machine into swap, not just stall the app.
//
// A small permit count keeps the work bounded no matter how many callers pile in.
// ===========================================================================

struct Gate {
    free: Mutex<usize>,
    ready: Condvar,
}

static INSPECT_GATE: OnceLock<Gate> = OnceLock::new();

fn gate() -> &'static Gate {
    INSPECT_GATE.get_or_init(|| Gate {
        // Two to four at a time: enough to keep a disk busy, few enough that the
        // peak memory of concurrent image decodes stays bounded.
        free: Mutex::new(
            std::thread::available_parallelism()
                .map(|n| n.get().clamp(2, 4))
                .unwrap_or(2),
        ),
        ready: Condvar::new(),
    })
}

/// Held for the duration of one archive inspection; releases its slot on drop.
struct Permit(&'static Gate);

impl Drop for Permit {
    fn drop(&mut self) {
        // Recover from poisoning rather than propagating it — a panic in one
        // inspection must not wedge the gate for every later caller.
        *self.0.free.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.0.ready.notify_one();
    }
}

fn acquire() -> Permit {
    let gate = gate();
    let mut free = gate.free.lock().unwrap_or_else(|e| e.into_inner());
    while *free == 0 {
        free = gate.ready.wait(free).unwrap_or_else(|e| e.into_inner());
    }
    *free -= 1;
    Permit(gate)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PkzMeta {
    pub locked: bool,
    pub name: Option<String>,
    pub author: Option<String>,
    pub location: Option<String>,
    /// In metres.
    pub length: Option<u32>,
    /// In metres.
    pub altitude: Option<i32>,
    pub thumbnail: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    mtime_ns: u128,
    size: u64,
    /// File name the entry was built from. Guards the (vanishingly unlikely) case of
    /// two different mods hashing to the same cache file.
    #[serde(default)]
    name: String,
    meta: PkzMeta,
}

/// Identity of a mod file for caching purposes: its name, size and mtime.
///
/// Deliberately *not* its full path. Pointing the app at a different MX Bikes folder —
/// a moved install, a second copy on another drive — used to change every key at once,
/// so a warm library went cold and re-inspected every archive in one burst. Windows
/// preserves both timestamps and sizes across a move or copy, so identity survives.
#[derive(Clone, Copy)]
struct Stamp {
    size: u64,
    mtime_ns: u128,
}

fn stamp(path: &str) -> Result<Stamp> {
    let file_meta = std::fs::metadata(path).with_context(|| format!("stat {path}"))?;
    Ok(Stamp {
        size: file_meta.len(),
        mtime_ns: file_meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    })
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn read_meta_cached(app: &tauri::AppHandle, path: &str) -> Result<PkzMeta> {
    let stamp = stamp(path)?;
    let cache_file = cache_path(app, path, stamp);
    if let Some(meta) = cache_file.as_deref().and_then(|cf| read_cache(cf, path, stamp)) {
        return Ok(meta);
    }

    // Only genuine misses queue for the disk + decode work.
    let _permit = acquire();
    // Someone ahead of us in the queue may have been inspecting this very file.
    if let Some(meta) = cache_file.as_deref().and_then(|cf| read_cache(cf, path, stamp)) {
        return Ok(meta);
    }

    let meta = read_meta(Path::new(path))?;
    if let Some(cf) = &cache_file {
        write_cache(cf, path, stamp, &meta);
    }
    Ok(meta)
}

/// Metadata for `path` only if it's already cached — never opens the archive.
///
/// Lets the Library paint every card it has seen before in one pass, leaving the
/// gated inspection above for the handful of entries that are genuinely new.
pub fn read_meta_if_cached(app: &tauri::AppHandle, path: &str) -> Option<PkzMeta> {
    let stamp = stamp(path).ok()?;
    let cache_file = cache_path(app, path, stamp)?;
    read_cache(&cache_file, path, stamp)
}

fn read_cache(cache_file: &Path, path: &str, stamp: Stamp) -> Option<PkzMeta> {
    let bytes = std::fs::read(cache_file).ok()?;
    let entry: CacheEntry = serde_json::from_slice(&bytes).ok()?;
    let same_file =
        entry.mtime_ns == stamp.mtime_ns && entry.size == stamp.size && entry.name == file_name_of(path);
    same_file.then_some(entry.meta)
}

fn write_cache(cache_file: &Path, path: &str, stamp: Stamp, meta: &PkzMeta) {
    if let Some(parent) = cache_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entry = CacheEntry {
        mtime_ns: stamp.mtime_ns,
        size: stamp.size,
        name: file_name_of(path),
        meta: meta.clone(),
    };
    if let Ok(bytes) = serde_json::to_vec(&entry) {
        let _ = std::fs::write(cache_file, bytes);
    }
}

/// Bumped when the key changes shape, so stale entries are ignored rather than
/// misread. The previous generation was keyed on the absolute path.
const CACHE_DIR: &str = "pkz-meta-v2";

fn cache_path(app: &tauri::AppHandle, source: &str, stamp: Stamp) -> Option<PathBuf> {
    let cache_root = app.path().app_cache_dir().ok()?;
    drop_stale_cache(&cache_root);

    let mut hasher = DefaultHasher::new();
    file_name_of(source).hash(&mut hasher);
    stamp.size.hash(&mut hasher);
    stamp.mtime_ns.hash(&mut hasher);
    Some(cache_root.join(CACHE_DIR).join(format!("{:016x}.json", hasher.finish())))
}

/// Clear out the path-keyed generation once per run — nothing will ever read it again.
fn drop_stale_cache(cache_root: &Path) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = std::fs::remove_dir_all(cache_root.join("pkz-meta"));
    });
}

pub fn read_meta(path: &Path) -> Result<PkzMeta> {
    Ok(inspect(path)?.0)
}

pub fn read_preview(path: &Path) -> Result<Option<String>> {
    let _permit = acquire();
    let (_, image) = inspect(path)?;
    Ok(image.and_then(|(name, bytes)| make_thumbnail(&name, &bytes, PREVIEW_MAX)))
}

/// Top-level `.ini`: fewest path segments, then shortest.
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

fn inspect(path: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    if path.is_dir() {
        inspect_dir(path)
    } else {
        inspect_zip(path)
    }
}

fn inspect_zip(path: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    let mut file = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;

    // Plain `.pkz` starts with the ZIP local-file magic; else it's a non-plain
    // archive. If this build has the optional reader it can still surface the name +
    // preview (see `inspect_locked`); otherwise it stays an anonymous locked entry.
    let mut magic = [0u8; 4];
    if file.read(&mut magic).unwrap_or(0) < 4 || magic != ZIP_MAGIC {
        return inspect_locked(path);
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

fn inspect_dir(dir: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    // Walk a few levels deep — enough to find the `.ini` and a preview.
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

/// Small metadata files worth pulling out of a non-plain archive to build its
/// preview: the descriptor `.ini` and any candidate preview image. Keeps us from
/// decoding a whole (possibly huge) track just to read its name.
fn is_meta_entry(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".ini")
        || [".jpg", ".jpeg", ".png", ".dds", ".tga", ".bmp"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

/// Build a `PkzMeta` (+ preview image) from already-decoded `(name, bytes)` entries,
/// reusing the same `.ini`/image selection as the plain-zip path.
fn meta_from_entries(entries: &[(String, Vec<u8>)]) -> (PkzMeta, Option<(String, Vec<u8>)>) {
    let names: Vec<String> = entries.iter().map(|(n, _)| n.replace('\\', "/")).collect();
    let mut meta = PkzMeta::default();
    let mut pic: Option<String> = None;
    let mut ini_dir = String::new();

    if let Some(idx) = top_ini_index(&names) {
        ini_dir = dir_of(&names[idx]);
        parse_ini(&String::from_utf8_lossy(&entries[idx].1), &mut meta, &mut pic);
    }

    let mut image = None;
    if let Some(img_idx) = pick_image(&names, &ini_dir, pic.as_deref()) {
        let bytes = &entries[img_idx].1;
        meta.thumbnail = make_thumbnail(&names[img_idx], bytes, THUMB_MAX);
        image = Some((names[img_idx].clone(), bytes.clone()));
    }
    (meta, image)
}

/// A non-plain (creator-locked) archive. If this build carries the optional reader,
/// pull just the descriptor + preview so the entry shows its real name/author/thumb
/// (still flagged `locked`); otherwise `read_selected` bails and it stays anonymous.
fn inspect_locked(path: &Path) -> Result<(PkzMeta, Option<(String, Vec<u8>)>)> {
    match read_selected(path, is_meta_entry) {
        Ok(entries) if !entries.is_empty() => {
            let (mut meta, image) = meta_from_entries(&entries);
            // It's still creator-locked — keep the badge; we only surfaced its preview.
            meta.locked = true;
            Ok((meta, image))
        }
        _ => Ok((locked(), None)),
    }
}

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

fn pick_image(names: &[String], ini_dir: &str, pic: Option<&str>) -> Option<usize> {
    if let Some(pic) = pic {
        let want = join_entry(ini_dir, pic).to_ascii_lowercase();
        if let Some(i) = names.iter().position(|n| n.to_ascii_lowercase() == want) {
            return Some(i);
        }
    }

    // No usable `pic` — pick the best-scoring image.
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| is_image(n))
        .max_by_key(|(_, n)| image_score(n))
        .map(|(i, _)| i)
}

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

/// Allocation ceiling applied to every preview decode. Without it a single
/// mis-authored image can claim far more memory than the thumbnail it produces.
fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::no_limits();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    limits.max_image_width = Some(MAX_DECODE_EDGE);
    limits.max_image_height = Some(MAX_DECODE_EDGE);
    limits
}

fn make_thumbnail(name: &str, bytes: &[u8], max: u32) -> Option<String> {
    let img = if name.to_ascii_lowercase().ends_with(".tga") {
        let mut dec = image::codecs::tga::TgaDecoder::new(Cursor::new(bytes)).ok()?;
        dec.set_limits(decode_limits()).ok()?;
        image::DynamicImage::from_decoder(dec).ok()?
    } else {
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        reader.limits(decode_limits());
        reader.decode().ok()?
    };

    // Drop to RGB — JPEG can't hold the alpha a TGA may decode to.
    let thumb = image::DynamicImage::ImageRgb8(img.thumbnail(max, max).to_rgb8());
    let mut jpg = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
        .ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
    Some(format!("data:image/jpeg;base64,{b64}"))
}

pub fn is_plain_zip(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut magic).map(|n| n))
        .map(|n| n >= 4 && magic == ZIP_MAGIC)
        .unwrap_or(false)
}

pub fn extract(path: &Path, out_dir: &Path) -> Result<Vec<String>> {
    if is_plain_zip(path) {
        return extract_plain(path, out_dir);
    }
    #[cfg(sidecar)]
    {
        if let Some(written) = crate::sidecar::try_extract(path, out_dir)? {
            return Ok(written);
        }
    }
    bail!("unsupported .pkz (can't extract) for {path:?}");
}

pub fn read_sidecar_blob(bytes: &[u8]) -> Option<Vec<u8>> {
    #[cfg(sidecar)]
    {
        if crate::sidecar::handles(bytes) {
            return crate::sidecar::read_blob(bytes).ok();
        }
    }
    let _ = bytes;
    None
}

pub fn read_all(path: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    if is_plain_zip(path) {
        let file = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;
        let mut archive =
            zip::ZipArchive::new(file).with_context(|| format!("open zip {path:?}"))?;
        let mut out = Vec::new();
        for idx in 0..archive.len() {
            let mut e = archive.by_index(idx)?;
            if !e.is_file() {
                continue;
            }
            let name = e.name().replace('\\', "/");
            let mut buf = Vec::with_capacity(e.size() as usize);
            e.read_to_end(&mut buf)?;
            out.push((name, buf));
        }
        return Ok(out);
    }
    #[cfg(sidecar)]
    {
        return crate::sidecar::read_all(path);
    }
    #[cfg(not(sidecar))]
    bail!("unsupported .pkz (can't read) for {path:?}");
}

pub fn read_selected(
    path: &Path,
    keep: impl Fn(&str) -> bool + Copy,
) -> Result<Vec<(String, Vec<u8>)>> {
    if is_plain_zip(path) {
        let file = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;
        let mut archive =
            zip::ZipArchive::new(file).with_context(|| format!("open zip {path:?}"))?;
        let mut out = Vec::new();
        for idx in 0..archive.len() {
            let mut e = archive.by_index(idx)?;
            if !e.is_file() || !keep(e.name()) {
                continue;
            }
            let name = e.name().replace('\\', "/");
            let mut buf = Vec::with_capacity(e.size() as usize);
            e.read_to_end(&mut buf)?;
            out.push((name, buf));
        }
        return Ok(out);
    }
    #[cfg(sidecar)]
    {
        return crate::sidecar::read_selected(path, keep);
    }
    #[cfg(not(sidecar))]
    bail!("unsupported .pkz (can't read) for {path:?}");
}

pub fn read_entry(path: &Path, file_name: &str) -> Result<Option<Vec<u8>>> {
    if is_plain_zip(path) {
        let file = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;
        let mut archive =
            zip::ZipArchive::new(file).with_context(|| format!("open zip {path:?}"))?;
        for idx in 0..archive.len() {
            let mut e = archive.by_index(idx)?;
            let base = e.name().replace('\\', "/");
            let base = base.rsplit('/').next().unwrap_or(&base);
            if base.eq_ignore_ascii_case(file_name) {
                let mut buf = Vec::with_capacity(e.size() as usize);
                e.read_to_end(&mut buf)?;
                return Ok(Some(buf));
            }
        }
        return Ok(None);
    }
    #[cfg(sidecar)]
    {
        if let Some(bytes) = crate::sidecar::read_entry(path, file_name)? {
            return Ok(Some(bytes));
        }
        return Ok(None);
    }
    #[cfg(not(sidecar))]
    bail!("unsupported .pkz (can't read {file_name}) for {path:?}");
}

/// Resolve entry name under `out_dir`, dropping `..`/absolute (zip-slip guard).
pub(crate) fn safe_dest(out_dir: &Path, name: &str) -> Option<PathBuf> {
    let safe: PathBuf = name
        .replace('\\', "/")
        .split('/')
        .filter(|c| !c.is_empty() && *c != "." && *c != "..")
        .collect();
    if safe.as_os_str().is_empty() {
        None
    } else {
        Some(out_dir.join(safe))
    }
}

fn extract_plain(path: &Path, out_dir: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;
    let mut archive = zip::ZipArchive::new(file).with_context(|| format!("open zip {path:?}"))?;
    std::fs::create_dir_all(out_dir).with_context(|| format!("mkdir {out_dir:?}"))?;

    let mut written = Vec::new();
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        if !entry.is_file() {
            continue;
        }
        let rel = entry.name().replace('\\', "/");
        let Some(dest) = safe_dest(out_dir, &rel) else {
            continue;
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("mkdir {parent:?}"))?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        std::fs::write(&dest, &buf).with_context(|| format!("write {dest:?}"))?;
        written.push(rel);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-pkz-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn cached_metadata_survives_the_library_moving() {
        let dir = tmp_dir("cache-move");
        let cache_file = dir.join("entry.json");
        let stamp = Stamp { size: 1234, mtime_ns: 999 };
        let meta = PkzMeta {
            name: Some("Red Bud".into()),
            ..Default::default()
        };
        write_cache(&cache_file, "/old/mods/tracks/Red Bud.pkz", stamp, &meta);

        // Same file, different folder: pointing the app at a moved MX Bikes install
        // must not re-inspect every archive it already knows.
        let hit = read_cache(&cache_file, "/new/drive/mods/tracks/Red Bud.pkz", stamp);
        assert_eq!(hit.and_then(|m| m.name).as_deref(), Some("Red Bud"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_replaced_or_different_mod_is_not_a_cache_hit() {
        let dir = tmp_dir("cache-miss");
        let cache_file = dir.join("entry.json");
        let stamp = Stamp { size: 1234, mtime_ns: 999 };
        write_cache(&cache_file, "/mods/tracks/Red Bud.pkz", stamp, &PkzMeta::default());

        // Updated in place — same name and path, new contents.
        let resized = Stamp { size: 4321, ..stamp };
        assert!(read_cache(&cache_file, "/mods/tracks/Red Bud.pkz", resized).is_none());
        let retouched = Stamp { mtime_ns: 1000, ..stamp };
        assert!(read_cache(&cache_file, "/mods/tracks/Red Bud.pkz", retouched).is_none());
        // A different mod that happens to hash to the same cache file.
        assert!(read_cache(&cache_file, "/mods/tracks/Other.pkz", stamp).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inspection_gate_bounds_concurrent_reads() {
        // The Library asks for every card's metadata at once. Letting all of those
        // open archives and decode previews simultaneously is what locked machines
        // up, so the gate has to hold regardless of how many callers arrive.
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let threads: Vec<_> = (0..32)
            .map(|_| {
                let (live, peak) = (Arc::clone(&live), Arc::clone(&peak));
                std::thread::spawn(move || {
                    let _permit = acquire();
                    let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    live.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }

        let peak = peak.load(Ordering::SeqCst);
        assert!(peak >= 1, "the gate must let work through at all");
        assert!(peak <= 4, "at most 4 inspections at once, saw {peak}");
    }

    /// `MXB_DUMP_PKZ='…/rider.pkz' cargo test dump_pkz_layout -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_pkz_layout() {
        let path = std::env::var("MXB_DUMP_PKZ").expect("set MXB_DUMP_PKZ");
        let entries = read_all(std::path::Path::new(&path)).expect("read pkz");
        eprintln!("=== {} entries in {path} ===", entries.len());
        // Group by extension so the shape is legible.
        let mut by_ext: std::collections::BTreeMap<String, usize> = Default::default();
        for (name, _) in &entries {
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            *by_ext.entry(ext).or_default() += 1;
        }
        eprintln!("--- extensions: {by_ext:?}");
        for (name, data) in &entries {
            eprintln!("  {name}  ({} bytes)", data.len());
        }
        // Dump text of every config-ish, non-huge entry.
        let is_text = |n: &str| {
            let l = n.to_ascii_lowercase();
            [".cfg", ".ini", ".skl", ".txt", ".xml", ".bones", ".rig", ".hrc", ".prm"]
                .iter()
                .any(|e| l.ends_with(e))
        };
        for (name, data) in &entries {
            if is_text(name) && data.len() < 200_000 {
                eprintln!("\n########## {name} ##########");
                eprintln!("{}", String::from_utf8_lossy(data));
            }
        }
    }

    /// `MXB_DUMP_PKZ='…/rider.pkz' MXB_ENTRY='rider/riders/default_mx/rider.edf' \
    ///  MXB_OUT='/tmp/rider.edf' cargo test extract_pkz_entry -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn extract_pkz_entry() {
        let path = std::env::var("MXB_DUMP_PKZ").expect("set MXB_DUMP_PKZ");
        let want = std::env::var("MXB_ENTRY").expect("set MXB_ENTRY").to_ascii_lowercase();
        let out = std::env::var("MXB_OUT").expect("set MXB_OUT");
        let got = read_selected(std::path::Path::new(&path), |n| {
            n.to_ascii_lowercase() == want
        })
        .expect("read pkz");
        let (name, data) = got.into_iter().next().expect("entry not found");
        std::fs::write(&out, &data).expect("write out");
        eprintln!("wrote {} bytes of {name} to {out}", std::fs::metadata(&out).unwrap().len());
    }

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

    /// `MXB_REAL_PKZ=<file> MXB_OUT=<dir> cargo test extract_pkz_to_env -- --ignored`
    #[test]
    #[ignore]
    fn extract_pkz_to_env() {
        let (Ok(src), Ok(out)) = (std::env::var("MXB_REAL_PKZ"), std::env::var("MXB_OUT")) else {
            eprintln!("set MXB_REAL_PKZ and MXB_OUT to run");
            return;
        };
        let written = extract(Path::new(&src), Path::new(&out)).expect("extract");
        eprintln!("wrote {} files to {out}", written.len());
        for w in written.iter().take(40) {
            eprintln!("  {w}");
        }
    }
}
