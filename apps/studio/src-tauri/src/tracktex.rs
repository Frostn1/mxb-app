//! The rider's own ground images.
//!
//! A track's look can be painted with an image the rider brought rather than one of ours.
//! Nothing here reaches the network: an image arrives through the file picker, off the
//! rider's own disk, and is copied into the Studio's own folder so that a project still
//! builds after the file it came from has been moved, renamed or deleted.
//!
//! What gets stored is not the file that was picked. It is decoded, checked, cropped square
//! and resampled to [`SHEET_DIM`] before anything is written, so the store holds a fixed-size
//! ground sheet and never a 40-megapixel photograph the generator would have to deal with on
//! every build. The name it is stored under is the SHA-256 of those normalised bytes, so the
//! same image imported twice is one file, and an id in a saved project either resolves to
//! exactly the sheet it was built with or to nothing at all.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// What every stored sheet is resampled to.
///
/// Indiana's own terrain sheets are 1024 square and the generator resamples whatever it is
/// given to the band's working size anyway, so storing more is storing detail that is thrown
/// away on the way to the `.tga`.
pub const SHEET_DIM: u32 = 1024;

/// The smallest image worth taking. Below this there is nothing to resample *up* from and the
/// ground comes out as mush repeated a hundred and fifty times across the track.
const MIN_DIM: u32 = 256;

/// The largest. Bigger than any sheet a published track ships, and past it decoding is slow
/// enough to feel like the app has hung.
const MAX_DIM: u32 = 8192;

/// The biggest file the picker will open, before it is decoded. A decoded image costs four
/// bytes a pixel whatever its file says, so this is a guard on the decode as much as the read.
const MAX_BYTES: u64 = 48 * 1024 * 1024;

/// Where the sheets live. Set once at startup from the app's own data folder.
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Decoded sheets, kept for the life of the process: a build reads each of them once per
/// band and there are at most four.
type Cache = Mutex<std::collections::HashMap<String, Option<Arc<(usize, Vec<u8>)>>>>;
static CACHE: OnceLock<Cache> = OnceLock::new();

/// Point the store at the app's data folder. Called once, from `main`.
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// The folder sheets are stored in, creating it if it isn't there yet.
pub fn dir() -> PathBuf {
    DIR.get().cloned().unwrap_or_else(|| {
        // Only reached when nothing set one — a test, or a build that never imported
        // anything. A path that doesn't exist is fine: every read of it returns nothing.
        dirs_next::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("frost-studio")
            .join("track-textures")
    })
}

/// One stored sheet, as the picker shows it.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OwnTexture {
    /// What a program refers to it by.
    pub id: String,
    /// The name of the file it was imported from, for the rider to recognise it.
    pub name: String,
    /// A small PNG of it as a `data:` URL, so the picker can show it without the asset
    /// protocol being opened up to the whole folder.
    pub thumb: String,
}

/// How wide the thumbnail the picker shows is.
const THUMB_DIM: u32 = 96;

/// Take an image off the rider's disk into the store.
///
/// Every way this can fail says what was wrong with the file, because the alternative is a
/// picker that swallows a photograph and shows nothing.
pub fn import(src: &Path) -> Result<OwnTexture, String> {
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    let meta = std::fs::metadata(src).map_err(|e| format!("couldn't open {name}: {e}"))?;
    if meta.len() > MAX_BYTES {
        return Err(format!(
            "{name} is {:.0} MB. The biggest image the Studio will take is {} MB.",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_BYTES / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(src).map_err(|e| format!("couldn't read {name}: {e}"))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|_| format!("{name} isn't an image the Studio can read."))?;
    let normalised = normalise(&img).map_err(|e| format!("{name}: {e}"))?;

    // Named by what it holds rather than where it came from, so importing the same picture
    // twice is one file and an id always means the same pixels.
    let id = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(&normalised);
        format!("{:x}", h.finalize())[..32].to_string()
    };
    let dir = dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let png = dir.join(format!("{id}.png"));
    if !png.exists() {
        write_atomically(&png, &normalised)?;
    }
    write_atomically(&dir.join(format!("{id}.name")), name.as_bytes())?;
    Ok(OwnTexture { id, name, thumb: thumb_of(&image::load_from_memory(&normalised).unwrap()) })
}

/// Crop to a square about the middle, resample to [`SHEET_DIM`] and encode as PNG.
///
/// Square because every sheet in the generator is addressed as one — `resample_sheet` walks
/// a single dimension — and cropping keeps the middle of the picture rather than squashing
/// the whole of it, which is what stretches gravel into streaks.
fn normalise(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    let short = w.min(h);
    if short < MIN_DIM {
        return Err(format!(
            "it is {w} by {h}. A ground sheet has to be at least {MIN_DIM} across each way."
        ));
    }
    if w.max(h) > MAX_DIM {
        return Err(format!(
            "it is {w} by {h}. The biggest a ground sheet can be is {MAX_DIM} across."
        ));
    }
    let square = img.crop_imm((w - short) / 2, (h - short) / 2, short, short);
    let sheet = square.resize_exact(SHEET_DIM, SHEET_DIM, image::imageops::FilterType::Triangle);
    encode_png(&sheet.to_rgba8())
}

fn thumb_of(img: &image::DynamicImage) -> String {
    let small = img.resize_exact(THUMB_DIM, THUMB_DIM, image::imageops::FilterType::Triangle);
    match encode_png(&small.to_rgba8()) {
        Ok(png) => {
            use base64::Engine;
            format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
        }
        Err(_) => String::new(),
    }
}

fn encode_png(img: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("couldn't encode it: {e}"))?;
    Ok(out.into_inner())
}

/// Via a temp file, so a failed write can't leave half a sheet where a whole one was.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

/// Everything in the store, newest first.
pub fn list() -> Vec<OwnTexture> {
    let dir = dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, OwnTexture)> = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("png") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let name = std::fs::read_to_string(dir.join(format!("{id}.name")))
            .unwrap_or_else(|_| id.clone());
        let when = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        found.push((when, OwnTexture { id, name, thumb: thumb_of(&img) }));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, t)| t).collect()
}

/// Drop one from the store. A project still naming it falls back to the built-in ground.
pub fn forget(id: &str) -> Result<(), String> {
    if !is_id(id) {
        return Err("that isn't an image the Studio stored.".into());
    }
    let dir = dir();
    let _ = std::fs::remove_file(dir.join(format!("{id}.png")));
    let _ = std::fs::remove_file(dir.join(format!("{id}.name")));
    if let Some(cache) = CACHE.get() {
        if let Ok(mut c) = cache.lock() {
            c.remove(id);
        }
    }
    Ok(())
}

/// One stored sheet's pixels, RGBA, square, `(dim, pixels)`.
///
/// `None` for an id nothing is stored under — a project carried to another machine, or an
/// image the rider has since deleted. The band then paints with the built-in ground it was
/// standing in for, which is a track that looks ordinary rather than a track that won't
/// build.
pub fn sheet(id: &str) -> Option<Arc<(usize, Vec<u8>)>> {
    if !is_id(id) {
        return None;
    }
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut c = cache.lock().ok()?;
    if let Some(hit) = c.get(id) {
        return hit.clone();
    }
    let loaded = std::fs::read(dir().join(format!("{id}.png")))
        .ok()
        .and_then(|b| image::load_from_memory(&b).ok())
        .map(|img| {
            let img = img.to_rgba8();
            Arc::new((img.width() as usize, img.into_raw()))
        })
        // A sheet the generator would then index off the end of. Stored sheets are square by
        // construction; a hand-edited folder is not the generator's problem to survive.
        .filter(|s| s.0 > 0 && s.1.len() == s.0 * s.0 * 4);
    c.insert(id.to_string(), loaded.clone());
    loaded
}

/// Ids are hex, and a hex id cannot walk out of the folder it names a file in.
fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        encode_png(&img).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-tracktex-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_picture_comes_out_a_square_sheet() {
        let img = image::load_from_memory(&png(1600, 900)).unwrap();
        let out = normalise(&img).unwrap();
        let back = image::load_from_memory(&out).unwrap();
        assert_eq!((back.width(), back.height()), (SHEET_DIM, SHEET_DIM));
    }

    #[test]
    fn a_tiny_picture_is_refused_with_its_size_in_the_reason() {
        let img = image::load_from_memory(&png(64, 64)).unwrap();
        let why = normalise(&img).unwrap_err();
        assert!(why.contains("64 by 64"), "{why}");
        assert!(why.contains("256"), "{why}");
    }

    #[test]
    fn a_file_that_is_not_an_image_says_so() {
        let dir = scratch("notanimage");
        let path = dir.join("notes.txt");
        std::fs::write(&path, b"this is not a picture").unwrap();
        let why = import(&path).unwrap_err();
        assert!(why.contains("notes.txt"), "{why}");
        assert!(why.contains("isn't an image"), "{why}");
    }

    #[test]
    fn a_missing_file_says_so_rather_than_panicking() {
        let why = import(Path::new("/no/such/picture.png")).unwrap_err();
        assert!(why.contains("picture.png"), "{why}");
    }

    /// The whole chain, from a file on disk to the pixels a band is painted with.
    ///
    /// The only test that sets the store's folder, because the folder is set once for the
    /// life of the process — which is also how the app uses it.
    #[test]
    fn an_imported_file_comes_back_out_as_the_ground_a_band_paints_with() {
        set_dir(scratch("endtoend"));
        let src = dir().join("source.png");
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&src, png(900, 600)).unwrap();
        let t = import(&src).expect("it imported");
        assert!(t.thumb.starts_with("data:image/png;base64,"), "no picture for the picker");
        assert_eq!(t.name, "source.png");

        let sheet = sheet(&t.id).expect("the store hands it back");
        assert_eq!(sheet.0, SHEET_DIM as usize);
        assert_eq!(sheet.1.len(), sheet.0 * sheet.0 * 4);

        // Importing it a second time is the same sheet, not a second copy.
        let again = import(&src).expect("it imported again");
        assert_eq!(again.id, t.id);

        // And a program naming it paints with it. Compared against the same track without
        // it, because "different from the built-in" is the whole claim.
        use crate::trackprog::{OwnSheet, SheetSlot, TexturePreset, TextureSet};
        let mut p = crate::tracksynth::oval_for_test();
        let plain = crate::tracksynth::band_for_test(&p, "soil_light_c");
        p.terrain.texture = TextureSet {
            preset: TexturePreset::Ride,
            sheets: vec![OwnSheet { slot: SheetSlot::Ground, id: t.id.clone() }],
        };
        let mine = crate::tracksynth::band_for_test(&p, "soil_light_c");
        assert_ne!(plain, mine, "the imported image never reached the ground");

        // The bands it was not picked for are untouched.
        assert_eq!(
            crate::tracksynth::band_for_test(&p, "hm_grass"),
            crate::tracksynth::band_for_test(&crate::tracksynth::oval_for_test(), "hm_grass"),
            "it painted a band nobody picked it for"
        );
    }

    #[test]
    fn an_id_cannot_name_a_file_outside_the_store() {
        assert!(!is_id("../../etc/passwd"));
        assert!(!is_id(""));
        assert!(is_id("0123456789abcdef0123456789abcdef"));
        assert!(sheet("../../../etc/passwd").is_none());
    }
}
