//! Native decoder for MX Bikes **`.pnt`** paint files — the textures that make up
//! a livery, helmet, suit, gloves, boots, etc. This is what lets the 3D viewer
//! show a user's actual paint on a model without shelling out to PiBoSo's
//! PaintEd tool.
//!
//! A `.pnt` is an **unencrypted** container (verified against `libpnt`, the MIT
//! reverse-engineering, and a round-tripped fixture). Layout, little-endian:
//!
//! ```text
//! Header (108 bytes):
//!   magic     [4]    = "PNT\0"
//!   basename  [100]  paint display name, null-padded
//!   count     u32    number of packed textures
//! Then `count` image records:
//!   filename  [100]  texture name WITHOUT extension (e.g. "livery"), null-padded
//!   width     u32
//!   height    u32
//!   md5       [16]   (not needed to decode)
//!   data_size u32    = 8 (padding) + compressed byte length
//!   padding   [8]    zero bytes
//!   data      [data_size-8]  raw DEFLATE (wbits -15) of BGRA pixels
//! ```
//!
//! The inflated payload is 4-bytes-per-pixel **BGRA**; we swap to RGBA for the
//! renderer. There is no offset table — records are walked sequentially using
//! `data_size` to reach the next one.

use anyhow::{bail, Context, Result};
use base64::Engine;
use flate2::read::DeflateDecoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use serde::Serialize;
use std::io::{Cursor, Read};
use std::path::Path;

const MAGIC: &[u8; 4] = b"PNT\x00";
const HEADER_SIZE: usize = 108;
const NAME_SIZE: usize = 100;
/// Fixed per-image header: name(100) + w(4) + h(4) + md5(16) + data_size(4).
const IMAGE_HEADER_SIZE: usize = NAME_SIZE + 4 + 4 + 16 + 4;
/// Zero padding between an image's header and its deflate payload.
const IMAGE_PADDING: usize = 8;

/// One decoded texture: RGBA8 pixels plus its internal name and dimensions.
#[derive(Debug, Clone)]
pub struct PntTexture {
    /// Internal texture name without extension (`livery`, `helmet`, `rider`…).
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA8, `width * height * 4` bytes, top-left origin.
    pub rgba: Vec<u8>,
}

/// A texture ready for the frontend: a PNG `data:` URI plus its metadata. Mirrors
/// the `data:`-URI approach the library thumbnails already use, so no asset
/// protocol wiring is needed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintTexture {
    /// Internal texture name without extension.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// `data:image/png;base64,…` — bind straight into a three.js `TextureLoader`.
    pub png: String,
}

/// Read a `u32` (little-endian) at `off`, bounds-checked.
fn read_u32(buf: &[u8], off: usize) -> Result<u32> {
    let end = off + 4;
    if end > buf.len() {
        bail!("truncated .pnt: wanted u32 at {off}, len {}", buf.len());
    }
    Ok(u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]))
}

/// Read a null-padded ASCII name field of `NAME_SIZE` bytes at `off`.
fn read_name(buf: &[u8], off: usize) -> Result<String> {
    let end = off + NAME_SIZE;
    if end > buf.len() {
        bail!("truncated .pnt: wanted name at {off}, len {}", buf.len());
    }
    let raw = &buf[off..end];
    let n = raw.iter().position(|&b| b == 0).unwrap_or(NAME_SIZE);
    Ok(String::from_utf8_lossy(&raw[..n]).into_owned())
}

/// Decode every texture in a `.pnt` byte buffer into RGBA8.
pub fn decode(buf: &[u8]) -> Result<Vec<PntTexture>> {
    if buf.len() < HEADER_SIZE || &buf[..4] != MAGIC {
        bail!("not a .pnt file (bad magic)");
    }
    let count = read_u32(buf, HEADER_SIZE - 4)? as usize;

    let mut out = Vec::with_capacity(count);
    let mut off = HEADER_SIZE;
    for i in 0..count {
        let name = read_name(buf, off)?;
        let width = read_u32(buf, off + NAME_SIZE)?;
        let height = read_u32(buf, off + NAME_SIZE + 4)?;
        let data_size = read_u32(buf, off + NAME_SIZE + 8 + 16)? as usize;
        if data_size < IMAGE_PADDING {
            bail!("texture {i} '{name}': data_size {data_size} < padding");
        }

        let data_start = off + IMAGE_HEADER_SIZE + IMAGE_PADDING;
        let comp_len = data_size - IMAGE_PADDING;
        let data_end = data_start + comp_len;
        if data_end > buf.len() {
            bail!("texture {i} '{name}': payload runs past end of file");
        }

        // Raw DEFLATE (no zlib header) → **RGBA** pixels, already in that order.
        // (An earlier BGRA→RGBA swap here corrupted every paint: it turned navy into
        // brown and red into blue. Verified against PiBoSo's own stock
        // `riders/default_mx/paints/white_navy.pnt` — its navy only reads as navy
        // when the bytes are taken as RGBA.)
        let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
        DeflateDecoder::new(&buf[data_start..data_end])
            .read_to_end(&mut rgba)
            .with_context(|| format!("inflate texture {i} '{name}'"))?;

        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() != expected {
            bail!(
                "texture {i} '{name}': inflated {} bytes, expected {expected} ({width}x{height} RGBA)",
                rgba.len()
            );
        }

        out.push(PntTexture {
            name,
            width,
            height,
            rgba,
        });
        off = data_start + comp_len;
    }
    Ok(out)
}

/// Decode a `.pnt` buffer, transparently opening a creator-**locked** paint. A
/// locked `.pnt` is an encrypted single-blob container whose plaintext is a plain
/// `PNT\0` file; if the bytes aren't already `PNT\0` we try to decrypt them and
/// decode the result. Falls back to [`decode`]'s error when neither works.
pub fn decode_any(buf: &[u8]) -> Result<Vec<PntTexture>> {
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return decode(buf);
    }
    if let Some(plain) = crate::pkz::decrypt_locked_blob(buf) {
        return decode(&plain);
    }
    decode(buf) // not PNT and not a container we can open → report bad magic
}

/// Encode an [`PntTexture`] as a PNG `data:` URI for the frontend.
fn to_png_uri(tex: &PntTexture) -> Result<String> {
    // Fast deflate + no row filter: these are large (up to 2048²) diffuse maps
    // headed straight for a `data:` URI in the local webview — encode speed matters
    // far more than a few % of file size, and the default (best-compression, adaptive
    // filter) PNG path was the viewer's dominant cost. Encode straight from the RGBA
    // slice (no intermediate `RgbaImage` clone).
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::NoFilter)
        .write_image(&tex.rgba, tex.width, tex.height, ExtendedColorType::Rgba8)
        .context("encode PNG")?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Extract **every** texture packed inside a bike's `model.edf`, under the name
/// the model itself gives it (`2021crf`, `exhaust_22`, `w_plate` on the Honda;
/// `plastics`, `450f_metals` on the KTM 450), downscaled for the viewer.
///
/// These names matter: `gfx.cfg` binds mesh groups to textures BY NAME, and a
/// `.pnt` paint replaces a model texture of the same name. Collapsing them to a
/// generic `albedo` (what this used to do, keeping only the largest) threw away
/// the only thing that says whether a paint applies to a given model at all.
///
/// Normal/roughness maps (`_n` / `_r`) are skipped — the viewer's material is
/// diffuse-only, and they're the bulk of the decode cost.
pub fn extract_edf_textures(edf: &[u8]) -> Vec<PaintTexture> {
    crate::edf::embedded_textures(edf)
        .iter()
        .filter(|t| {
            let n = t.name.to_ascii_lowercase();
            !n.ends_with("_n") && !n.ends_with("_r")
        })
        .filter_map(|t| {
            let rgba = crate::edf::inflate_texture(edf, t)?;
            // Embedded edf textures are stored **RGBA** already (unlike `.pnt`
            // textures, which are BGRA) — swapping turns the plastics blue.
            let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(
                t.width, t.height, rgba,
            )?);
            // Downscale so a 4096² body map becomes a light preview texture, and
            // encode as JPEG — these are photographic, so JPEG is ~10× smaller than
            // PNG over the IPC boundary (no alpha needed for a diffuse).
            let scaled = img.thumbnail(1024, 1024);
            let (sw, sh) = (scaled.width(), scaled.height());
            let mut jpg = Vec::new();
            image::DynamicImage::ImageRgb8(scaled.to_rgb8())
                .write_to(&mut Cursor::new(&mut jpg), image::ImageFormat::Jpeg)
                .ok()?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg);
            Some(PaintTexture {
                name: t.name.clone(),
                width: sw,
                height: sh,
                png: format!("data:image/jpeg;base64,{b64}"),
            })
        })
        .collect()
}

/// Encode a decoded [`PntTexture`] into a frontend-ready [`PaintTexture`] (PNG
/// `data:` URI), downscaled to a preview cap. Returns a 1×1 transparent placeholder
/// if PNG encoding fails.
///
/// The cap is load-bearing: a `.pnt` body map is up to 4096² — an ~80 MB data URI
/// each — and a bike paint ships several (diffuse + normal + roughness), so at full
/// size the frontend gets 150+ MB of textures, exhausts the webview's WebGL memory,
/// and the model silently fails to render entirely (the exact "paint doesn't show"
/// bug). 1024² is the same budget the model's own textures use ([`extract_edf_textures`])
/// and is ample for a preview.
pub fn to_texture(t: &PntTexture) -> PaintTexture {
    const MAX: u32 = 1024;
    if t.width.max(t.height) > MAX {
        if let Some(img) = image::RgbaImage::from_raw(t.width, t.height, t.rgba.clone()) {
            let scaled = image::DynamicImage::ImageRgba8(img).thumbnail(MAX, MAX);
            let (w, h) = (scaled.width(), scaled.height());
            let scaled = PntTexture {
                name: t.name.clone(),
                width: w,
                height: h,
                rgba: scaled.to_rgba8().into_raw(),
            };
            return PaintTexture {
                name: t.name.clone(),
                width: w,
                height: h,
                png: to_png_uri(&scaled).unwrap_or_default(),
            };
        }
    }
    PaintTexture {
        name: t.name.clone(),
        width: t.width,
        height: t.height,
        png: to_png_uri(t).unwrap_or_default(),
    }
}

/// Decode a raw image (`.tga`/`.png`/…) into a frontend-ready [`PaintTexture`].
/// Used for a bike's own textures shipped inside its `.pkz`. `name` is the
/// texture's base name (no extension). `None` if it can't be decoded.
pub fn decode_image(name: &str, bytes: &[u8]) -> Option<PaintTexture> {
    // TGA has no magic, so try it explicitly first, then fall back to sniffing.
    let img = image::codecs::tga::TgaDecoder::new(Cursor::new(bytes))
        .ok()
        .and_then(|d| image::DynamicImage::from_decoder(d).ok())
        .or_else(|| image::load_from_memory(bytes).ok())?;
    let rgba = img.to_rgba8();
    let tex = PntTexture {
        name: name.to_string(),
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    };
    to_png_uri(&tex).ok().map(|png| PaintTexture {
        name: tex.name,
        width: tex.width,
        height: tex.height,
        png,
    })
}

/// Decode a `.pnt` file at `path` into frontend-ready textures (PNG `data:` URIs).
pub fn unpack_file(path: &Path) -> Result<Vec<PaintTexture>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    decode_any(&bytes)?
        .iter()
        .map(|t| {
            Ok(PaintTexture {
                name: t.name.clone(),
                width: t.width,
                height: t.height,
                png: to_png_uri(t)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture: a 4×4 32-bit paint packed with `libpnt` (a third-party packer).
    ///
    /// **Channel-order caveat:** `libpnt` writes pixels in the opposite byte order
    /// to the paints MX Bikes actually ships. The game's are plain **RGBA** —
    /// verified against PiBoSo's own stock
    /// `rider/riders/default_mx/paints/white_navy.pnt`, whose navy only reads as
    /// navy (blue-dominant) when the bytes are taken as RGBA; decoding it the other
    /// way turns navy into brown and red into blue. So this fixture still exercises
    /// the header + DEFLATE path, but its payload must be channel-swapped to line up
    /// with our (real-file-correct) decode.
    const FIXTURE_PNT: &[u8] = include_bytes!("fixtures/test_paint.pnt");
    const FIXTURE_RGBA: &[u8] = include_bytes!("fixtures/test_paint_rgba.bin");

    /// `FIXTURE_RGBA` with R/B swapped — i.e. the bytes actually stored in the
    /// libpnt fixture, which our decoder must return verbatim.
    fn fixture_stored_pixels() -> Vec<u8> {
        let mut v = FIXTURE_RGBA.to_vec();
        for px in v.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        v
    }

    #[test]
    fn decodes_fixture_to_expected_rgba() {
        let texs = decode(FIXTURE_PNT).expect("decode fixture");
        assert_eq!(texs.len(), 1, "one texture packed");
        let t = &texs[0];
        assert_eq!(t.name, "livery");
        assert_eq!((t.width, t.height), (4, 4));
        // Pixels come back exactly as stored — no channel reordering. (Guards
        // against re-introducing the BGRA→RGBA swap that corrupted every paint.)
        assert_eq!(
            t.rgba,
            fixture_stored_pixels(),
            "pixels returned verbatim (no channel swap)"
        );
    }

    /// Local-only: `MXB_REAL_EDF=<model.edf> cargo test extract_edf_textures -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn extract_edf_textures_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let t = std::time::Instant::now();
        let texs = extract_edf_textures(&bytes);
        eprintln!("extracted {} texture(s) in {:?}", texs.len(), t.elapsed());
        assert!(!texs.is_empty(), "the model packs its own textures");
        for x in &texs {
            eprintln!("  '{}' {}x{} uri_len={}", x.name, x.width, x.height, x.png.len());
            assert!(x.width <= 1024 && x.height <= 1024 && x.png.len() > 100);
        }
    }

    #[test]
    fn rejects_non_pnt() {
        assert!(decode(b"not a paint file at all........").is_err());
    }

    #[test]
    fn encodes_png_uri() {
        let texs = decode(FIXTURE_PNT).unwrap();
        let uri = to_png_uri(&texs[0]).unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
    }

    /// A non-`PNT\0` buffer that isn't a locked container we can open still surfaces
    /// the bad-magic error via `decode_any` (no panic, no false decode).
    #[test]
    fn decode_any_rejects_garbage() {
        assert!(decode_any(b"not a paint file at all........").is_err());
    }

    /// `decode_any` passes a plain `PNT\0` file straight through to `decode`.
    #[test]
    fn decode_any_handles_plain_pnt() {
        let texs = decode_any(FIXTURE_PNT).expect("decode plain via decode_any");
        assert_eq!(texs.len(), 1);
        assert_eq!(texs[0].rgba, fixture_stored_pixels());
    }

    /// Real-file guard for the channel order, using PiBoSo's own stock paint:
    /// `MXB_STOCK_PNT=<…/rider.pkz extracted>/white_navy.pnt cargo test -- --ignored`.
    /// A paint named *white_navy* must decode with its navy **blue-dominant**; if a
    /// channel swap ever creeps back in, the navy reads brown (red-dominant).
    #[test]
    #[ignore]
    fn stock_white_navy_decodes_navy_not_brown() {
        let Ok(path) = std::env::var("MXB_STOCK_PNT") else {
            eprintln!("set MXB_STOCK_PNT to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read stock paint");
        let texs = decode_any(&bytes).expect("decode stock paint");
        let t = texs.iter().find(|t| t.name == "rider").expect("'rider' texture");
        // The darkest saturated pixels are the navy: blue must beat red.
        let (mut blue_dom, mut red_dom) = (0u32, 0u32);
        for px in t.rgba.chunks_exact(4) {
            let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
            if r.max(g).max(b) < 90 && (b - r).abs() > 8 {
                if b > r {
                    blue_dom += 1;
                } else {
                    red_dom += 1;
                }
            }
        }
        eprintln!("dark pixels: blue-dominant={blue_dom} red-dominant={red_dom}");
        assert!(
            blue_dom > red_dom * 2,
            "white_navy's dark colour must be NAVY (blue-dominant), got blue={blue_dom} red={red_dom} — channel order is wrong"
        );
    }

    /// Local-only proof against a real **creator-locked** paint: set `MXB_REAL_PNT`
    /// to a locked `.pnt` and run `cargo test -- --ignored`. `decode_any` must
    /// transparently decrypt it and recover real textures.
    #[test]
    #[ignore]
    fn decodes_locked_paint_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_PNT") else {
            eprintln!("set MXB_REAL_PNT to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read locked paint");
        assert_ne!(&bytes[..4], MAGIC, "fixture should be a LOCKED (non-PNT) paint");
        let texs = decode_any(&bytes).expect("decode_any opens the locked paint");
        assert!(!texs.is_empty(), "recovered at least one texture");
        for t in &texs {
            eprintln!("texture '{}' {}x{} ({} px)", t.name, t.width, t.height, t.rgba.len() / 4);
            assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize);
        }
    }
}

