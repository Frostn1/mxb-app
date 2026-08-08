//! `.pnt` layout, little-endian:
//! ```text
//! Header (108 bytes): magic[4]="PNT\0", basename[100], count u32
//! Per record: filename[100], width u32, height u32, md5[16],
//!             data_size u32 (= 8 padding + compressed len), padding[8],
//!             data[data_size-8]  raw DEFLATE (wbits -15)
//! ```

use anyhow::{bail, Context, Result};
use flate2::read::DeflateDecoder;
use serde::Serialize;
use std::io::{Cursor, Read};
use std::path::Path;

const MAGIC: &[u8; 4] = b"PNT\x00";
const HEADER_SIZE: usize = 108;
const NAME_SIZE: usize = 100;
const IMAGE_HEADER_SIZE: usize = NAME_SIZE + 4 + 4 + 16 + 4;
const IMAGE_PADDING: usize = 8;

#[derive(Debug, Clone)]
pub struct PntTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA8, `width * height * 4` bytes, top-left origin.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Names the pixels in [`crate::texstore`]. Cloning this struct is cheap, which is why
    /// a paint can carry the model's whole base texture set without duplicating megabytes.
    pub token: String,
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32> {
    let end = off + 4;
    if end > buf.len() {
        bail!("truncated .pnt: wanted u32 at {off}, len {}", buf.len());
    }
    Ok(u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]))
}

fn read_name(buf: &[u8], off: usize) -> Result<String> {
    let end = off + NAME_SIZE;
    if end > buf.len() {
        bail!("truncated .pnt: wanted name at {off}, len {}", buf.len());
    }
    let raw = &buf[off..end];
    let n = raw.iter().position(|&b| b == 0).unwrap_or(NAME_SIZE);
    Ok(String::from_utf8_lossy(&raw[..n]).into_owned())
}

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

        // Raw DEFLATE (no zlib header) → RGBA, already in order (no channel swap).
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

pub fn decode_any(buf: &[u8]) -> Result<Vec<PntTexture>> {
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return decode(buf);
    }
    if let Some(plain) = crate::pkz::read_sidecar_blob(buf) {
        return decode(&plain);
    }
    decode(buf) // not PNT and no sidecar reader for it → report bad magic
}

/// Hand pixels to the store and describe them. The viewer reads RGBA straight into a
/// `DataTexture`, so nothing is encoded here — this used to be a PNG compress plus a base64
/// pass per texture, which is what made loading a bike peg every core for seconds.
fn store_rgba(name: &str, width: u32, height: u32, rgba: Vec<u8>) -> PaintTexture {
    PaintTexture {
        name: name.to_string(),
        width,
        height,
        token: crate::texstore::put(rgba),
    }
}

/// Longest edge the viewer is given. Beyond this the extra pixels cost memory without
/// showing on a preview-sized model.
const MAX_EDGE: u32 = 1024;

pub fn extract_edf_textures(edf: &[u8]) -> Vec<PaintTexture> {
    extract_edf_textures_where(edf, |_| true)
}

/// The textures of [`extract_edf_textures`] whose names `want` accepts.
///
/// Inflating a 2048² blob and re-encoding it is the expensive part of this by a wide margin
/// — on a rider body it dwarfs parsing the mesh. A caller that already knows it will throw a
/// texture away shouldn't pay to decode it first.
pub fn extract_edf_textures_where(
    edf: &[u8],
    want: impl Fn(&str) -> bool,
) -> Vec<PaintTexture> {
    crate::edf::embedded_textures(edf)
        .iter()
        .filter(|t| {
            let n = t.name.to_ascii_lowercase();
            !n.ends_with("_n") && !n.ends_with("_r")
        })
        .filter(|t| want(&t.name))
        .filter_map(|t| {
            let rgba = crate::edf::inflate_texture(edf, t)?;
            // Embedded edf textures are already RGBA — no channel swap.
            let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(
                t.width, t.height, rgba,
            )?);
            let scaled = img.thumbnail(MAX_EDGE, MAX_EDGE);
            Some(store_rgba(
                &t.name,
                scaled.width(),
                scaled.height(),
                scaled.to_rgba8().into_raw(),
            ))
        })
        .collect()
}

pub fn to_texture(t: &PntTexture) -> PaintTexture {
    if t.width.max(t.height) > MAX_EDGE {
        if let Some(img) = image::RgbaImage::from_raw(t.width, t.height, t.rgba.clone()) {
            let scaled = image::DynamicImage::ImageRgba8(img).thumbnail(MAX_EDGE, MAX_EDGE);
            return store_rgba(
                &t.name,
                scaled.width(),
                scaled.height(),
                scaled.to_rgba8().into_raw(),
            );
        }
    }
    store_rgba(&t.name, t.width, t.height, t.rgba.clone())
}

pub fn decode_image(name: &str, bytes: &[u8]) -> Option<PaintTexture> {
    // TGA has no magic, so try it explicitly first, then fall back to sniffing.
    let img = image::codecs::tga::TgaDecoder::new(Cursor::new(bytes))
        .ok()
        .and_then(|d| image::DynamicImage::from_decoder(d).ok())
        .or_else(|| image::load_from_memory(bytes).ok())?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some(store_rgba(name, w, h, rgba.into_raw()))
}

pub fn unpack_file(path: &Path) -> Result<Vec<PaintTexture>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    Ok(decode_any(&bytes)?.iter().map(to_texture).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MXB_PNT='…/gloves/x.pnt' cargo test dump_pnt_names -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_pnt_names() {
        let path = std::env::var("MXB_PNT").expect("set MXB_PNT");
        let buf = std::fs::read(&path).expect("read pnt");
        let texs = decode_any(&buf).expect("decode");
        eprintln!("{} textures in {path}", texs.len());
        for t in &texs {
            eprintln!("  '{}'  {}x{}", t.name, t.width, t.height);
        }
    }

    const FIXTURE_PNT: &[u8] = include_bytes!("fixtures/test_paint.pnt");
    const FIXTURE_RGBA: &[u8] = include_bytes!("fixtures/test_paint_rgba.bin");

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
        // Pixels come back exactly as stored — no channel reordering.
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
            let px = crate::texstore::get(&x.token).expect("token resolves to pixels");
            eprintln!("  '{}' {}x{} bytes={}", x.name, x.width, x.height, px.len());
            assert!(x.width <= MAX_EDGE && x.height <= MAX_EDGE);
            assert_eq!(px.len() as u32, x.width * x.height * 4, "RGBA, no padding");
        }
    }

    #[test]
    fn rejects_non_pnt() {
        assert!(decode(b"not a paint file at all........").is_err());
    }

    #[test]
    fn to_texture_stores_pixels_verbatim() {
        let texs = decode(FIXTURE_PNT).unwrap();
        let out = to_texture(&texs[0]);
        assert_eq!((out.width, out.height), (4, 4));
        // Under MAX_EDGE, so the pixels reach the viewer untouched — no resample, no encode.
        assert_eq!(
            *crate::texstore::get(&out.token).expect("token resolves"),
            fixture_stored_pixels()
        );
    }

    #[test]
    fn decode_any_rejects_garbage() {
        assert!(decode_any(b"not a paint file at all........").is_err());
    }

    #[test]
    fn decode_any_handles_plain_pnt() {
        let texs = decode_any(FIXTURE_PNT).expect("decode plain via decode_any");
        assert_eq!(texs.len(), 1);
        assert_eq!(texs[0].rgba, fixture_stored_pixels());
    }

    /// `MXB_STOCK_PNT=<…/rider.pkz extracted>/white_navy.pnt cargo test -- --ignored`.
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

    /// `MXB_REAL_PNT=<non-PNT container> cargo test -- --ignored`
    #[test]
    #[ignore]
    fn decodes_sidecar_paint_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_PNT") else {
            eprintln!("set MXB_REAL_PNT to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read paint");
        assert_ne!(&bytes[..4], MAGIC, "fixture should be a non-PNT container");
        let texs = decode_any(&bytes).expect("decode_any reads the container");
        assert!(!texs.is_empty(), "recovered at least one texture");
        for t in &texs {
            eprintln!("texture '{}' {}x{} ({} px)", t.name, t.width, t.height, t.rgba.len() / 4);
            assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize);
        }
    }
}

