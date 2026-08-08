use anyhow::{bail, Context, Result};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use md5::{Digest, Md5};
use serde::Serialize;
use std::io::{Cursor, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 4] = b"PNT\x00";
const HEADER_SIZE: usize = 108;
const NAME_SIZE: usize = 100;
const IMAGE_HEADER_SIZE: usize = NAME_SIZE + 4 + 4 + 16 + 4;
const IMAGE_PADDING: usize = 8;
/// The digest between a texture's dimensions and its payload length — see [`encode`].
const DIGEST_SIZE: usize = 16;

#[derive(Debug, Clone)]
pub struct PntTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
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

/// The texture names a `.pnt` supplies, read from the headers alone.
///
/// Every image header is a fixed size and states its payload's length, so the names can be
/// walked without inflating a single pixel — which matters because they're wanted for their
/// own sake: they name the textures a model declares and leaves to a paint, and that decides
/// which slot each of its materials points at.
pub fn texture_names(buf: &[u8]) -> Result<Vec<String>> {
    if buf.len() < HEADER_SIZE || &buf[..4] != MAGIC {
        bail!("not a .pnt file (bad magic)");
    }
    let count = read_u32(buf, HEADER_SIZE - 4)? as usize;
    let mut out = Vec::with_capacity(count);
    let mut off = HEADER_SIZE;
    for _ in 0..count {
        out.push(read_name(buf, off)?);
        let data_size = read_u32(buf, off + NAME_SIZE + 8 + 16)? as usize;
        if data_size < IMAGE_PADDING {
            break;
        }
        off = off + IMAGE_HEADER_SIZE + data_size;
    }
    Ok(out)
}

/// [`texture_names`], for a `.pnt` that may be sealed — the same pairing as
/// [`decode`]/[`decode_any`].
pub fn texture_names_any(buf: &[u8]) -> Result<Vec<String>> {
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return texture_names(buf);
    }
    if let Some(plain) = crate::pkz::read_sidecar_blob(buf) {
        return texture_names(&plain);
    }
    texture_names(buf)
}

pub fn decode_any(buf: &[u8]) -> Result<Vec<PntTexture>> {
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return decode(buf);
    }
    if let Some(plain) = crate::pkz::read_sidecar_blob(buf) {
        return decode(&plain);
    }
    decode(buf)
}

/// A texture name as the format stores it: `NAME_SIZE` bytes, NUL-terminated and
/// NUL-padded. Names are the whole binding mechanism — a paint replaces a mesh's texture
/// by matching this string — so a name that wouldn't survive the round trip is refused
/// rather than silently truncated into one that binds to nothing.
fn fixed_name(name: &str) -> Result<[u8; NAME_SIZE]> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("a texture name can't be empty");
    }
    if !trimmed.is_ascii() {
        bail!("texture name '{trimmed}': the game reads these as plain ASCII");
    }
    // One byte short of the field: the terminator has to fit too.
    if trimmed.len() >= NAME_SIZE {
        bail!("texture name '{trimmed}' is longer than {} characters", NAME_SIZE - 1);
    }
    let mut out = [0u8; NAME_SIZE];
    out[..trimmed.len()].copy_from_slice(trimmed.as_bytes());
    Ok(out)
}

/// Build a `.pnt` carrying `textures`, in the layout [`decode`] reads back.
///
/// This is the write half of the format, and it exists so a paint made anywhere else — a
/// GIMP export, a template edited in Photoshop — can become a file MX Bikes loads. Pixels
/// go in **RGBA**, top-left origin, exactly as [`decode`] hands them out; the game's own
/// files are that way round (see the `white_navy` guard below), so a round trip through
/// here has to leave a paint's colours alone.
///
/// The 16 bytes between a texture's dimensions and its payload length are a digest in
/// every real file we've read, and `edf.rs` documents them as an MD5. Nothing we can
/// observe says what it's taken over, so this writes the MD5 of the uncompressed pixels:
/// the one reading a validator would most plausibly agree with, and harmless if — as the
/// loader's own behaviour suggests — the field is never checked at all.
pub fn encode(paint_name: &str, textures: &[PntTexture]) -> Result<Vec<u8>> {
    if textures.is_empty() {
        bail!("a paint needs at least one texture");
    }
    let mut seen: Vec<String> = Vec::with_capacity(textures.len());
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&fixed_name(paint_name).context("paint name")?);
    out.extend_from_slice(&(textures.len() as u32).to_le_bytes());

    for t in textures {
        let name = fixed_name(&t.name)?;
        // Two textures under one name is a paint whose second half can never be reached:
        // the loader binds by name and takes the first match.
        if seen.iter().any(|s| s.eq_ignore_ascii_case(t.name.trim())) {
            bail!("texture '{}' is in this paint twice", t.name.trim());
        }
        seen.push(t.name.trim().to_string());

        if t.width == 0 || t.height == 0 {
            bail!("texture '{}': {}x{} has no pixels", t.name, t.width, t.height);
        }
        let expected = (t.width as usize) * (t.height as usize) * 4;
        if t.rgba.len() != expected {
            bail!(
                "texture '{}': {} bytes of pixels, expected {expected} ({}x{} RGBA)",
                t.name,
                t.rgba.len(),
                t.width,
                t.height
            );
        }

        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&t.rgba).with_context(|| format!("deflate texture '{}'", t.name))?;
        let payload = enc.finish().with_context(|| format!("deflate texture '{}'", t.name))?;
        let digest: [u8; DIGEST_SIZE] = Md5::digest(&t.rgba).into();

        out.extend_from_slice(&name);
        out.extend_from_slice(&t.width.to_le_bytes());
        out.extend_from_slice(&t.height.to_le_bytes());
        out.extend_from_slice(&digest);
        out.extend_from_slice(&((payload.len() + IMAGE_PADDING) as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; IMAGE_PADDING]);
        out.extend_from_slice(&payload);
    }
    Ok(out)
}

fn store_rgba(name: &str, width: u32, height: u32, rgba: Vec<u8>) -> PaintTexture {
    PaintTexture {
        name: name.to_string(),
        width,
        height,
        token: crate::texstore::put(rgba),
    }
}

const MAX_EDGE: u32 = 1024;

pub fn extract_edf_textures(edf: &[u8]) -> Vec<PaintTexture> {
    extract_edf_textures_where(edf, |_| true)
}

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
        assert_eq!(
            t.rgba,
            fixture_stored_pixels(),
            "pixels returned verbatim (no channel swap)"
        );
    }

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

    fn tex(name: &str, w: u32, h: u32) -> PntTexture {
        // A gradient rather than a flat fill: flat pixels deflate to almost nothing, which
        // would hide a payload-length mistake behind a length that happens to be tiny.
        let rgba = (0..w * h)
            .flat_map(|i| [(i % 251) as u8, (i % 97) as u8, (i % 13) as u8, 0xff])
            .collect();
        PntTexture { name: name.to_string(), width: w, height: h, rgba }
    }

    #[test]
    fn encode_round_trips_through_decode() {
        let want = vec![tex("livery", 8, 4), tex("numberplate", 2, 2)];
        let bytes = encode("My Paint", &want).expect("encode");
        let back = decode(&bytes).expect("decode what we just wrote");
        assert_eq!(back.len(), want.len());
        for (a, b) in back.iter().zip(&want) {
            assert_eq!(a.name, b.name);
            assert_eq!((a.width, a.height), (b.width, b.height));
            assert_eq!(a.rgba, b.rgba, "pixels survive the round trip unswapped");
        }
        assert_eq!(texture_names(&bytes).unwrap(), vec!["livery", "numberplate"]);
    }

    /// The header the game reads: magic, the paint's own name, then the texture count.
    #[test]
    fn encode_writes_the_documented_header() {
        let bytes = encode("Paint Name", &[tex("livery", 4, 4)]).unwrap();
        assert_eq!(&bytes[..4], MAGIC);
        assert_eq!(read_name(&bytes, 4).unwrap(), "Paint Name");
        assert_eq!(read_u32(&bytes, HEADER_SIZE - 4).unwrap(), 1);
        // Payload length counts the 8 pad bytes, and the pad itself is zeroed.
        let size = read_u32(&bytes, HEADER_SIZE + NAME_SIZE + 8 + 16).unwrap() as usize;
        let pad_at = HEADER_SIZE + IMAGE_HEADER_SIZE;
        assert_eq!(&bytes[pad_at..pad_at + IMAGE_PADDING], &[0u8; IMAGE_PADDING]);
        assert_eq!(bytes.len(), pad_at + size, "the file ends where the header says it does");
    }

    /// The digest field is the one part of the record we can't observe the meaning of, so
    /// pin what we write: the MD5 of the uncompressed pixels.
    #[test]
    fn encode_digests_the_uncompressed_pixels() {
        let t = tex("livery", 4, 4);
        let bytes = encode("p", std::slice::from_ref(&t)).unwrap();
        let at = HEADER_SIZE + NAME_SIZE + 8;
        assert_eq!(&bytes[at..at + DIGEST_SIZE], &Md5::digest(&t.rgba)[..]);
    }

    #[test]
    fn encode_rejects_what_the_game_could_not_read() {
        let ok = tex("livery", 4, 4);
        assert!(encode("p", &[]).is_err(), "a paint with no textures");
        assert!(encode("", &[ok.clone()]).is_err(), "an unnamed paint");
        assert!(
            encode("p", &[tex("", 4, 4)]).is_err(),
            "an unnamed texture binds to nothing"
        );
        assert!(
            encode("p", &[tex(&"x".repeat(NAME_SIZE), 4, 4)]).is_err(),
            "a name that wouldn't fit its field"
        );
        assert!(
            encode("p", &[tex("livery", 4, 4), tex("LIVERY", 2, 2)]).is_err(),
            "the same name twice — the second could never be reached"
        );
        let mut short = ok.clone();
        short.rgba.truncate(4);
        assert!(encode("p", &[short]).is_err(), "pixels that don't fill the dimensions");
        assert!(encode("p", &[tex("livery", 0, 4)]).is_err(), "a zero dimension");
    }

    #[test]
    fn to_texture_stores_pixels_verbatim() {
        let texs = decode(FIXTURE_PNT).unwrap();
        let out = to_texture(&texs[0]);
        assert_eq!((out.width, out.height), (4, 4));
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
