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
    let img = decode(path)?;
    let (sw, sh) = (img.width(), img.height());
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

/// Read an image file, whatever it is, at whatever size it is.
fn decode(path: &Path) -> Result<DynamicImage> {
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
    if img.width() == 0 || img.height() == 0 {
        bail!("{} has no pixels", path.display());
    }
    Ok(img)
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

/// Read `path` at its own size and its own shape, for the layer editor to draw with.
///
/// Deliberately not [`inspect`], not [`paint::to_texture`], and not even [`load`]: the first
/// shrinks to [`PREVIEW_EDGE`] for a thumbnail, the second caps at the viewer's 1024, and the
/// third rounds both edges to powers of two. Each is right where it's used and wrong here.
///
/// The power-of-two rounding is the one that would bite hardest: it's what the *game* needs of
/// a finished sheet, and applying it to a 300×200 sponsor logo on its way onto a layer would
/// stretch it to 256×256 — a squashed decal, from a step the painter never asked for. The
/// rounding still happens, once, to the composited sheet on its way into the `.pnt`.
///
/// The cost is honest: these are big, and they share [`crate::texstore`]'s budget with the
/// viewer's models. That store is an LRU, so the price of holding one is evicting something
/// nobody is looking at.
pub fn pixels(path: &Path) -> Result<paint::PaintTexture> {
    let rgba = decode(path)?.to_rgba8();
    Ok(paint::PaintTexture {
        name: texture_name_for(path),
        width: rgba.width(),
        height: rgba.height(),
        token: crate::texstore::put(rgba.into_raw()),
    })
}

/// Write composited pixels out as a PNG the build path can read, returning its path.
///
/// The editor's sheets only exist as a canvas in the webview, and [`build`] reads files. This
/// is the join between them, and it stages rather than writing to the destination: nothing
/// lands in the game folder until `paint_studio_save` runs, so an editor that never saves
/// leaves the mods tree untouched.
///
/// PNG rather than TGA because it's what a canvas encodes to natively, and both are lossless
/// — the sheet is re-decoded by [`load`] on the way into the paint either way.
pub fn stage_sheet(dir: &Path, name: &str, png: &[u8]) -> Result<PathBuf> {
    if png.is_empty() {
        bail!("no pixels to stage for '{name}'");
    }
    // The name becomes the texture name the mesh binds, so it has to survive the round trip
    // through a file name intact — but it still can't be allowed to name a path.
    let stem = crate::install::sanitize(name.trim());
    let stem = stem.trim();
    if stem.is_empty() {
        bail!("a sheet needs a name before it can be saved");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(format!("{stem}.png"));
    std::fs::write(&path, png).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
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
///
/// Only a paint stored in the open format is unpacked — see [`paint::is_plain`]. The
/// viewer reads more than that, and this is the one place a paint leaves the app as files
/// somebody can edit, so it takes the narrower reading and `decode` rather than
/// `decode_any` keeps that true even if the check above it ever moved.
pub fn extract(pnt: &[u8], dir: &Path) -> Result<Vec<PathBuf>> {
    if !paint::is_plain(pnt) {
        bail!("this paint can't be unpacked into sheets");
    }
    let textures = paint::decode(pnt).context("read the paint")?;
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
    fn extract_only_unpacks_a_paint_stored_in_the_open_format() {
        let dir = tmpdir("notplain");
        let out = dir.join("sheets");
        // A paint the viewer can still render, but which wasn't stored openly, reaches here
        // as a buffer that isn't the plain format. Rendering it is fine; turning it into
        // files somebody can edit and republish is not, so nothing is written at all.
        let err = extract(b"NOTPNT\x00\x00 followed by a paint", &out).expect_err("must refuse");
        assert!(format!("{err:#}").contains("can't be unpacked"));
        assert!(!out.exists(), "refused before anything reached disk");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Designer's whole save path, minus the canvas: stage the sheet a browser would have
    /// encoded, then pack it. What this guards is the join — a staged file the packer can't
    /// read, or a name mangled on the way through one, only shows up as a paint that loads
    /// blank in game.
    #[test]
    fn a_staged_sheet_packs_into_a_paint() {
        let dir = tmpdir("stage");
        let png = {
            let mut buf = Vec::new();
            image::RgbaImage::from_fn(64, 64, |x, y| image::Rgba([x as u8, y as u8, 9, 0xff]))
                .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
                .unwrap();
            buf
        };
        let staged = stage_sheet(&dir, "livery", &png).expect("stage");
        assert!(staged.is_file());

        let bytes = build(
            "Designed",
            &[BuildTexture {
                path: staged.to_string_lossy().into_owned(),
                name: "livery".into(),
            }],
        )
        .expect("pack the staged sheet");
        let back = paint::decode(&bytes).expect("decode what we just packed");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "livery", "the name the mesh binds survives the staging file");
        assert_eq!((back[0].width, back[0].height), (64, 64));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sheet name is a texture name, not a path. It reaches the backend as a header on the
    /// upload, so it is the one part of a save that arrives as free text from the webview.
    #[test]
    fn staging_refuses_a_name_that_would_escape_its_directory() {
        let dir = tmpdir("stagename");
        let png = std::fs::read(Path::new("/dev/null")).unwrap_or_default();
        assert!(stage_sheet(&dir, "livery", &png).is_err(), "no pixels, no file");

        let real = {
            let mut buf = Vec::new();
            image::RgbaImage::new(2, 2)
                .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
                .unwrap();
            buf
        };
        let out = stage_sheet(&dir, "../escaped", &real).expect("stage");
        assert_eq!(out.parent(), Some(dir.as_path()), "stays where it was put");
        assert!(stage_sheet(&dir, "   ", &real).is_err(), "a blank name is not a texture name");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `pixels` feeds the layer editor, and a layer must not be reshaped on the way in — that
    /// rounding belongs to the finished sheet, not to a sponsor logo being placed on one.
    #[test]
    fn pixels_keeps_a_source_at_its_own_shape() {
        let dir = tmpdir("pixels");
        let file = dir.join("logo.png");
        write_png(&file, 300, 200);
        let tex = pixels(&file).expect("read");
        assert_eq!((tex.width, tex.height), (300, 200), "not rounded to a power of two");
        assert_eq!(tex.name, "logo");
        // `load`, which feeds the packer, still rounds — the two answers differ on purpose.
        let (packed, _, _, resized) = load(&file).expect("load");
        assert!(resized);
        assert_eq!((packed.width, packed.height), (256, 256));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sheet must come back the same way up it went out.
    ///
    /// The Designer draws the extracted sheet on a canvas and hands that same canvas to the 3D
    /// preview, so a vertical flip anywhere in extract → read would put the drawing upside down
    /// relative to the paint it started from. TGA is the format with an origin bit and two
    /// conventions, which makes this the one hop where that can happen quietly.
    #[test]
    fn a_sheet_survives_extract_and_reread_the_same_way_up() {
        let dir = tmpdir("flip");
        // Deliberately asymmetric top-to-bottom: the first row is the only opaque red one.
        let (w, h) = (4u32, 4u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for x in 0..w as usize {
            let at = x * 4;
            rgba[at..at + 4].copy_from_slice(&[0xff, 0x00, 0x00, 0xff]);
        }
        let original = PntTexture { name: "livery".into(), width: w, height: h, rgba };

        let pnt = paint::encode("Flip", std::slice::from_ref(&original)).expect("encode");
        let files = extract(&pnt, &dir).expect("extract");
        let back = pixels(&files[0]).expect("read the sheet back");

        assert_eq!((back.width, back.height), (w, h));
        let bytes = crate::texstore::get(&back.token).expect("pixels are still held");
        assert_eq!(
            &bytes[0..4],
            &[0xff, 0x00, 0x00, 0xff],
            "the top-left pixel is still the top-left pixel — a flip would put it on the last row",
        );
        let last_row = ((h - 1) * w * 4) as usize;
        assert_eq!(
            &bytes[last_row..last_row + 4],
            &[0, 0, 0, 0],
            "and the bottom row is still the empty one",
        );
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
