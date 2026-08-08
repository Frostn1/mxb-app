//! Making a `.pnt` out of ordinary images, and taking one back apart into ordinary images.
//!
//! MX Bikes reads a paint as a packed container of DEFLATE-compressed RGBA sheets, and no
//! image editor writes one. So a livery drawn in GIMP or Photoshop has, until now, had to
//! go through somebody else's converter before the game — or this app's 3D preview — would
//! look at it. Both directions live here:
//!
//! - [`build`] turns a set of `.tga`/`.png`/… files into the bytes of a `.pnt`.
//! - [`extract`] writes a `.pnt`'s sheets out as `.tga` files, which is how you get a
//!   *template*: open an existing paint, save its sheets, edit them, build them back.
//!
//! The names matter more than the pixels. A paint doesn't say which part of a bike or a
//! helmet it covers — it supplies textures *by name*, and the mesh binds whichever names it
//! asked for (`paint::texture_names`). A sheet extracted as `livery.tga` and rebuilt from a
//! file still called `livery.tga` therefore lands back on the same bodywork, which is the
//! whole reason the extract half exists.

use crate::paint::{self, PntTexture};
use anyhow::{bail, Context, Result};
use image::{DynamicImage, ExtendedColorType, ImageEncoder};
use serde::Serialize;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Texture edges the game's own art comes in — powers of two, `edf::embedded_textures`
/// validates records against this same set.
const MIN_EDGE: u32 = 64;
const MAX_EDGE: u32 = 4096;

/// Extensions [`load`] will read. TGA leads because it's what the paint community trades
/// templates in, but a PNG straight out of GIMP is the same pixels and works as well.
pub const SOURCE_EXTS: [&str; 6] = ["tga", "png", "jpg", "jpeg", "bmp", "webp"];

/// One source image, as the studio shows it before anything is written.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioImage {
    /// The file it was read from — the studio hands this back at build time, so an edit
    /// saved in GIMP in the meantime is the version that gets packed.
    pub path: String,
    /// The texture name it will be packed under: the file's stem, which is exactly what
    /// [`extract`] named it if this file came from a template.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// What the file itself measures. Differs from `width`/`height` only when the image
    /// had to be resized to an edge the game accepts — the UI says so when it does.
    pub source_width: u32,
    pub source_height: u32,
    pub resized: bool,
    /// Pixels held in [`crate::texstore`] for the preview, same as any decoded paint —
    /// but shrunk to [`PREVIEW_EDGE`], so `preview_width`/`preview_height` are what the
    /// token measures, not `width`/`height`.
    pub token: String,
    pub preview_width: u32,
    pub preview_height: u32,
}

/// How big the pixels kept for the on-screen thumbnail are.
///
/// The full sheet is not kept: a 4096² source is 67 MB, the studio holds several at once,
/// and they'd share the texture store's budget with the viewer's cached models — evicting
/// a bike someone is looking at to hold pixels nobody is looking at. The build path re-reads
/// the file from disk anyway, so nothing downstream depends on these.
const PREVIEW_EDGE: u32 = 512;

/// A power-of-two edge in `MIN_EDGE..=MAX_EDGE`, nearest to `edge`.
///
/// Rounding is geometric, not arithmetic: 1500 sits closer to 2048 than to 1024 in the
/// ratio that matters for how stretched the result looks, and `1500*1500 > 1024*2048` is
/// that comparison without the floating point.
fn nearest_edge(edge: u32) -> u32 {
    if edge <= MIN_EDGE {
        return MIN_EDGE;
    }
    if edge >= MAX_EDGE {
        return MAX_EDGE;
    }
    let lower = 1u32 << (31 - edge.leading_zeros());
    if lower == edge {
        return edge;
    }
    let upper = lower * 2;
    // (edge/lower) vs (upper/edge), cross-multiplied.
    if (edge as u64) * (edge as u64) >= (lower as u64) * (upper as u64) {
        upper
    } else {
        lower
    }
}

/// Whether `w`x`h` is a size the game will take as-is.
fn fits(w: u32, h: u32) -> bool {
    nearest_edge(w) == w && nearest_edge(h) == h
}

/// Read one image file as a texture, resized to power-of-two edges if it isn't already.
///
/// The resize is not a nicety: MX Bikes is a DirectX 9 title and its textures are powers of
/// two throughout. A 1000x1000 export — GIMP will happily make one — would otherwise be
/// packed into a file the game either refuses or renders as noise, and the failure would
/// surface in-game rather than here. The caller is told it happened (`resized`) so the UI
/// can say so before anything is saved.
pub fn load(path: &Path) -> Result<(PntTexture, u32, u32, bool)> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // `image::load_from_memory` sniffs by content and TGA has no magic to sniff, so a
    // `.tga` has to be handed to its decoder by name — the same order `paint::decode_image`
    // uses for the textures loose beside a bike model.
    let img = if ext == "tga" {
        image::codecs::tga::TgaDecoder::new(Cursor::new(&bytes))
            .ok()
            .and_then(|d| DynamicImage::from_decoder(d).ok())
            .or_else(|| image::load_from_memory(&bytes).ok())
    } else {
        image::load_from_memory(&bytes).ok()
    };
    let img = img.with_context(|| format!("{} isn't an image this can read", path.display()))?;

    let (sw, sh) = (img.width(), img.height());
    if sw == 0 || sh == 0 {
        bail!("{} has no pixels", path.display());
    }
    let resized = !fits(sw, sh);
    let img = if resized {
        img.resize_exact(
            nearest_edge(sw),
            nearest_edge(sh),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    let rgba = img.to_rgba8();
    let name = texture_name_for(path);
    Ok((
        PntTexture { name, width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() },
        sw,
        sh,
        resized,
    ))
}

/// The texture name a source file suggests: its stem, unchanged.
///
/// Unchanged is the point. `extract` writes `livery.tga`, the player edits `livery.tga`,
/// and this reads back `livery` — the name the mesh binds. Tidying the case or the spaces
/// here would quietly break that loop.
fn texture_name_for(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Read `path` for the studio: the texture, plus what the UI needs to describe it.
pub fn inspect(path: &Path) -> Result<StudioImage> {
    let (tex, sw, sh, resized) = load(path)?;
    let preview = image::RgbaImage::from_raw(tex.width, tex.height, tex.rgba)
        .map(|img| DynamicImage::ImageRgba8(img).thumbnail(PREVIEW_EDGE, PREVIEW_EDGE))
        .with_context(|| format!("{}: pixels don't fill its dimensions", path.display()))?;
    Ok(StudioImage {
        path: path.to_string_lossy().into_owned(),
        name: tex.name,
        width: tex.width,
        height: tex.height,
        source_width: sw,
        source_height: sh,
        resized,
        preview_width: preview.width(),
        preview_height: preview.height(),
        token: crate::texstore::put(preview.to_rgba8().into_raw()),
    })
}

/// One texture of a paint being built: a file on disk, packed under `name`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildTexture {
    pub path: String,
    pub name: String,
}

/// Build the bytes of a `.pnt` from source files.
///
/// Files are re-read here rather than reused from [`inspect`]'s preview: the studio is
/// meant to be left open beside an image editor, and re-reading is what makes "save in
/// GIMP, build again" pick up the save.
pub fn build(paint_name: &str, textures: &[BuildTexture]) -> Result<Vec<u8>> {
    if textures.is_empty() {
        bail!("pick at least one image to build a paint from");
    }
    let mut packed = Vec::with_capacity(textures.len());
    for t in textures {
        let (mut tex, _, _, _) = load(Path::new(&t.path))?;
        tex.name = t.name.trim().to_string();
        packed.push(tex);
    }
    paint::encode(paint_name, &packed)
}

/// Write a paint's sheets into `dir` as `.tga`, returning the files written.
///
/// TGA because that's the format MX Bikes' own loose textures use and every editor reads
/// it, and lossless because a template that shifted colours on the way out would shift
/// them again on every round trip.
pub fn extract(pnt: &[u8], dir: &Path) -> Result<Vec<PathBuf>> {
    let textures = paint::decode_any(pnt).context("read the paint")?;
    if textures.is_empty() {
        bail!("this paint carries no textures");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let mut out = Vec::with_capacity(textures.len());
    for t in &textures {
        let file = dir.join(format!("{}.tga", crate::install::sanitize(&t.name)));
        let mut buf = Vec::new();
        image::codecs::tga::TgaEncoder::new(&mut buf)
            .write_image(&t.rgba, t.width, t.height, ExtendedColorType::Rgba8)
            .with_context(|| format!("encode '{}' as TGA", t.name))?;
        std::fs::write(&file, &buf).with_context(|| format!("write {}", file.display()))?;
        out.push(file);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("frost-studio-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 7 % 256) as u8, (y * 11 % 256) as u8, 0x20, 0xff])
        });
        img.save(path).unwrap();
    }

    #[test]
    fn nearest_edge_rounds_to_a_size_the_game_takes() {
        assert_eq!(nearest_edge(1024), 1024, "already a power of two — untouched");
        assert_eq!(nearest_edge(1500), 2048, "geometrically closer to 2048");
        assert_eq!(nearest_edge(1200), 1024);
        assert_eq!(nearest_edge(10), MIN_EDGE, "clamped up");
        assert_eq!(nearest_edge(9000), MAX_EDGE, "clamped down");
    }

    #[test]
    fn loads_a_png_and_leaves_a_power_of_two_alone() {
        let dir = tmpdir("png");
        let file = dir.join("livery.png");
        write_png(&file, 64, 64);
        let (tex, sw, sh, resized) = load(&file).expect("load");
        assert_eq!(tex.name, "livery", "the stem is the texture name");
        assert_eq!((tex.width, tex.height), (64, 64));
        assert_eq!((sw, sh), (64, 64));
        assert!(!resized);
        assert_eq!(tex.rgba.len(), 64 * 64 * 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resizes_a_non_power_of_two_source() {
        let dir = tmpdir("npot");
        let file = dir.join("rider.png");
        write_png(&file, 100, 300);
        let (tex, sw, sh, resized) = load(&file).expect("load");
        assert!(resized);
        assert_eq!((sw, sh), (100, 300));
        assert_eq!((tex.width, tex.height), (128, 256));
        assert_eq!(tex.rgba.len(), 128 * 256 * 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The loop the feature exists for: a paint out to TGA, the TGAs back into a paint,
    /// same names and same pixels — so an edit in between is the only thing that changed.
    #[test]
    fn tga_round_trip_preserves_names_and_pixels() {
        let dir = tmpdir("roundtrip");
        let original = paint::encode(
            "Original",
            &[
                PntTexture {
                    name: "livery".into(),
                    width: 64,
                    height: 64,
                    rgba: (0..64 * 64)
                        .flat_map(|i| [(i % 256) as u8, (i / 64) as u8, 0x40, 0xff])
                        .collect(),
                },
                PntTexture {
                    name: "numberplate".into(),
                    width: 64,
                    height: 128,
                    rgba: vec![0x10; 64 * 128 * 4],
                },
            ],
        )
        .unwrap();

        let files = extract(&original, &dir).expect("extract");
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("livery.tga"));
        assert!(files[1].ends_with("numberplate.tga"));

        let rebuilt = build(
            "Rebuilt",
            &files
                .iter()
                .map(|f| BuildTexture {
                    path: f.to_string_lossy().into_owned(),
                    name: f.file_stem().unwrap().to_string_lossy().into_owned(),
                })
                .collect::<Vec<_>>(),
        )
        .expect("build");

        let before = paint::decode(&original).unwrap();
        let after = paint::decode(&rebuilt).unwrap();
        assert_eq!(after.len(), before.len());
        for (a, b) in after.iter().zip(&before) {
            assert_eq!(a.name, b.name);
            assert_eq!((a.width, a.height), (b.width, b.height));
            assert_eq!(a.rgba, b.rgba, "'{}' survived TGA and came back unchanged", a.name);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_uses_the_name_it_is_given_not_the_file_name() {
        let dir = tmpdir("rename");
        let file = dir.join("my export final FINAL.png");
        write_png(&file, 64, 64);
        let bytes = build(
            "Custom",
            &[BuildTexture {
                path: file.to_string_lossy().into_owned(),
                name: "  livery  ".into(),
            }],
        )
        .expect("build");
        assert_eq!(paint::texture_names(&bytes).unwrap(), vec!["livery"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_refuses_a_file_that_is_not_an_image() {
        let dir = tmpdir("notimage");
        let file = dir.join("livery.tga");
        std::fs::write(&file, b"this is not a targa").unwrap();
        let err = build(
            "Custom",
            &[BuildTexture { path: file.to_string_lossy().into_owned(), name: "livery".into() }],
        )
        .expect_err("should not pack garbage");
        assert!(format!("{err:#}").contains("isn't an image"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
