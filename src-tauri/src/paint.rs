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

        // Raw DEFLATE (no zlib header) → BGRA pixels.
        let mut bgra = Vec::with_capacity((width as usize) * (height as usize) * 4);
        DeflateDecoder::new(&buf[data_start..data_end])
            .read_to_end(&mut bgra)
            .with_context(|| format!("inflate texture {i} '{name}'"))?;

        let expected = (width as usize) * (height as usize) * 4;
        if bgra.len() != expected {
            bail!(
                "texture {i} '{name}': inflated {} bytes, expected {expected} ({width}x{height} BGRA)",
                bgra.len()
            );
        }

        // BGRA → RGBA in place.
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        out.push(PntTexture {
            name,
            width,
            height,
            rgba: bgra,
        });
        off = data_start + comp_len;
    }
    Ok(out)
}

/// Encode an [`PntTexture`] as a PNG `data:` URI for the frontend.
fn to_png_uri(tex: &PntTexture) -> Result<String> {
    let img = image::RgbaImage::from_raw(tex.width, tex.height, tex.rgba.clone())
        .context("assemble RGBA image")?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .context("encode PNG")?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Decode a `.pnt` file at `path` into frontend-ready textures (PNG `data:` URIs).
pub fn unpack_file(path: &Path) -> Result<Vec<PaintTexture>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    decode(&bytes)?
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

    /// Ground-truth fixture: a 4×4 32-bit paint packed with `libpnt`'s exact
    /// byte layout, and the RGBA payload it must decode back to.
    const FIXTURE_PNT: &[u8] = include_bytes!("fixtures/test_paint.pnt");
    const FIXTURE_RGBA: &[u8] = include_bytes!("fixtures/test_paint_rgba.bin");

    #[test]
    fn decodes_fixture_to_expected_rgba() {
        let texs = decode(FIXTURE_PNT).expect("decode fixture");
        assert_eq!(texs.len(), 1, "one texture packed");
        let t = &texs[0];
        assert_eq!(t.name, "livery");
        assert_eq!((t.width, t.height), (4, 4));
        // The .pnt stores BGRA; after our BGRA→RGBA swap it must match the exact
        // RGBA payload the fixture was built from.
        assert_eq!(t.rgba, FIXTURE_RGBA, "recovered RGBA matches ground truth");
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
}
