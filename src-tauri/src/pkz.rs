use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tauri::Manager;
use walkdir::WalkDir;

/// ZIP local-file-header magic ("PK\x03\x04").
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// Longest edge of the card thumbnail, in pixels.
const THUMB_MAX: u32 = 192;

/// Longest edge of the full-size preview, in pixels.
const PREVIEW_MAX: u32 = 1100;

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
    meta: PkzMeta,
}

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

fn cache_path(app: &tauri::AppHandle, source: &str) -> Option<PathBuf> {
    let dir = app.path().app_cache_dir().ok()?.join("pkz-meta");
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    Some(dir.join(format!("{:016x}.json", hasher.finish())))
}

pub fn read_meta(path: &Path) -> Result<PkzMeta> {
    Ok(inspect(path)?.0)
}

pub fn read_preview(path: &Path) -> Result<Option<String>> {
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

    // Plain `.pkz` starts with the ZIP local-file magic; else locked.
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

fn make_thumbnail(name: &str, bytes: &[u8], max: u32) -> Option<String> {
    let img = if name.to_ascii_lowercase().ends_with(".tga") {
        let dec = image::codecs::tga::TgaDecoder::new(Cursor::new(bytes)).ok()?;
        image::DynamicImage::from_decoder(dec).ok()?
    } else {
        image::load_from_memory(bytes).ok()?
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
