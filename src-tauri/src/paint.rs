use anyhow::{bail, Context, Result};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use md5::{Digest, Md5};
use rayon::prelude::*;
use serde::Serialize;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: &[u8; 4] = b"PNT\x00";
const HEADER_SIZE: usize = 108;
const NAME_SIZE: usize = 100;
const IMAGE_HEADER_SIZE: usize = NAME_SIZE + 4 + 4 + 16 + 4;
const IMAGE_PADDING: usize = 8;
/// The digest between a texture's dimensions and its payload length — see [`encode`].
const DIGEST_SIZE: usize = 16;

/// Ceilings on the two numbers a `.pnt` header supplies before anything has checked them:
/// how many textures follow, and how big each one inflates to. Both are read straight out
/// of the file and both feed an allocation, so a corrupt or truncated paint would otherwise
/// ask for one the size of the machine. Set well above any real paint — the game's own
/// sheets top out at 4096² — so they bound the damage without rejecting anything.
const MAX_TEXTURES: usize = 256;
const MAX_TEXTURE_BYTES: u64 = 16384 * 16384 * 4;
/// Pre-reserve up to a 4096² sheet. A bigger texture still decodes; the vector grows.
const MAX_RESERVE: usize = 4096 * 4096 * 4;

/// A texture's inflated size, refusing dimensions that describe no real texture.
fn texture_bytes(width: u32, height: u32) -> Result<usize> {
    let n = (width as u64)
        .checked_mul(height as u64)
        .and_then(|n| n.checked_mul(4))
        .unwrap_or(u64::MAX);
    if n > MAX_TEXTURE_BYTES {
        bail!("{width}x{height} is not a texture any paint carries");
    }
    Ok(n as usize)
}

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

/// One image's header, and where its payload sits in the file.
struct ImageHeader {
    name: String,
    width: u32,
    height: u32,
    /// The deflate payload's byte range within the whole `.pnt`.
    data: std::ops::Range<usize>,
}

/// Walk the image table without inflating a pixel — the same walk [`texture_names`] does,
/// keeping the payload ranges so the planes can then be inflated in any order.
fn image_headers(buf: &[u8]) -> Result<Vec<ImageHeader>> {
    if buf.len() < HEADER_SIZE || &buf[..4] != MAGIC {
        bail!("not a .pnt file (bad magic)");
    }
    let count = read_u32(buf, HEADER_SIZE - 4)? as usize;

    let mut out = Vec::with_capacity(count.min(MAX_TEXTURES));
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
        let data_end = data_start + (data_size - IMAGE_PADDING);
        if data_end > buf.len() {
            bail!("texture {i} '{name}': payload runs past end of file");
        }

        // Refused here rather than at the inflate, so a header that describes no real
        // texture is rejected before anything downstream reserves for it.
        texture_bytes(width, height).with_context(|| format!("texture {i} '{name}'"))?;

        out.push(ImageHeader {
            name,
            width,
            height,
            data: data_start..data_end,
        });
        off = data_end;
    }
    Ok(out)
}

pub fn decode(buf: &[u8]) -> Result<Vec<PntTexture>> {
    decode_where(buf, |_| true)
}

/// [`decode`], inflating only the textures `want` accepts.
///
/// The filter runs on the name, before any of that texture's pixels exist: a 4096² plane the
/// caller is going to throw away otherwise costs a 67 MB allocation and its share of the
/// inflate. Planes are independent, so they inflate in parallel — the payload ranges come off
/// the header walk, which is the only part that has to be sequential.
pub fn decode_where(
    buf: &[u8],
    want: impl Fn(&str) -> bool + Sync,
) -> Result<Vec<PntTexture>> {
    image_headers(buf)?
        .into_par_iter()
        .enumerate()
        .filter(|(_, h)| want(&h.name))
        .map(|(i, h)| {
            let expected = texture_bytes(h.width, h.height)?;
            let mut rgba = Vec::with_capacity(expected.min(MAX_RESERVE));
            // Stop one byte past what the header promised. Deflate expands ~1000:1, so a
            // mis-framed payload inflates until it runs the machine out of memory otherwise —
            // and the length check below turns the overrun into an error either way.
            DeflateDecoder::new(&buf[h.data])
                .take(expected as u64 + 1)
                .read_to_end(&mut rgba)
                .with_context(|| format!("inflate texture {i} '{}'", h.name))?;

            if rgba.len() != expected {
                bail!(
                    "texture {i} '{}': inflated {} bytes, expected {expected} ({}x{} RGBA)",
                    h.name,
                    rgba.len(),
                    h.width,
                    h.height
                );
            }

            Ok(PntTexture {
                name: h.name,
                width: h.width,
                height: h.height,
                rgba,
            })
        })
        .collect()
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
    let mut out = Vec::with_capacity(count.min(MAX_TEXTURES));
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

/// [`texture_names`] for a paint on disk, without reading the paint.
///
/// The same walk, done with seeks: each image header states its payload's length, so the
/// next name is reachable without touching the pixels in between. A 38 MB outfit is answered
/// by a few hundred bytes rather than a 38 MB read — which is the whole difference when the
/// question is only "which model does this paint?" and it is being asked of every file in a
/// dropped folder.
///
/// A sealed paint has no headers until it is decrypted, so that one reads whole.
pub fn texture_names_at(path: &Path) -> Result<Vec<String>> {
    let mut f = std::fs::File::open(path).with_context(|| format!("open {path:?}"))?;
    let mut head = [0u8; HEADER_SIZE];
    if f.read_exact(&mut head).is_err() || &head[..4] != MAGIC {
        return texture_names_any(&std::fs::read(path)?);
    }
    let count = read_u32(&head, HEADER_SIZE - 4)? as usize;

    let mut out = Vec::with_capacity(count.min(MAX_TEXTURES));
    let mut off = HEADER_SIZE as u64;
    let mut image = [0u8; IMAGE_HEADER_SIZE];
    for _ in 0..count {
        f.seek(SeekFrom::Start(off))?;
        f.read_exact(&mut image)
            .with_context(|| format!("truncated .pnt: wanted an image header at {off}"))?;
        out.push(read_name(&image, 0)?);
        let data_size = read_u32(&image, NAME_SIZE + 8 + DIGEST_SIZE)? as u64;
        if data_size < IMAGE_PADDING as u64 {
            break;
        }
        off += IMAGE_HEADER_SIZE as u64 + data_size;
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
    decode_any_where(buf, |_| true)
}

/// [`decode_any`], for a caller that only wants some of the textures — the same pairing as
/// [`decode`]/[`decode_where`].
pub fn decode_any_where(
    buf: &[u8],
    want: impl Fn(&str) -> bool + Sync,
) -> Result<Vec<PntTexture>> {
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return decode_where(buf, want);
    }
    if let Some(plain) = crate::pkz::read_sidecar_blob(buf) {
        return decode_where(&plain, want);
    }
    decode_where(buf, want)
}

/// Whether `buf` is a paint stored in the open format this module also writes.
///
/// [`decode_any`] deliberately reads a wider set than this, because a preview should show
/// whatever the game would show. Writing a paint back out as editable sheets is not that:
/// it hands someone an author's work in a form they can change and republish, so it is
/// offered only for a paint that was already stored openly. Rendering is not permission.
pub fn is_plain(buf: &[u8]) -> bool {
    buf.len() >= 4 && &buf[..4] == MAGIC
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

/// The `_n` normal maps a mesh embeds, for the sheets `want` accepts.
///
/// Separate from [`extract_edf_textures_where`], which drops every companion map, because a
/// normal map is not a look: it can never stand in for a colour sheet, and every caller that
/// asks "what does this model wear" would be wrong to see one. Only the viewer wants them,
/// and only to hang off a colour sheet it has already bound. `_r` and the exporter's own
/// companions stay out — nothing draws those.
///
/// `want` is given the *colour* sheet's name, not the map's, so a caller can ask for the
/// normals belonging to the sheets it is actually going to draw.
pub fn extract_edf_normal_maps(edf: &[u8], want: impl Fn(&str) -> bool) -> Vec<PaintTexture> {
    crate::edf::embedded_textures(edf)
        .iter()
        .filter_map(|t| {
            let n = t.name.to_ascii_lowercase();
            let base = n.strip_suffix("_n")?;
            want(base).then_some(t)
        })
        .filter_map(|t| {
            let rgba = crate::edf::inflate_texture(edf, t)?;
            if rgba.len() != (t.width as usize) * (t.height as usize) * 4 {
                return None;
            }
            store_capped(&t.name, t.width, t.height, rgba)
        })
        .collect()
}

/// Store a decoded sheet, downscaled to the edge the viewer draws at if it is bigger.
///
/// Shared with [`extract_edf_textures_where`] so the two can't come to different views about
/// how much memory a sheet is allowed to occupy.
fn store_capped(name: &str, width: u32, height: u32, rgba: Vec<u8>) -> Option<PaintTexture> {
    if width.max(height) <= MAX_EDGE {
        return Some(store_rgba(name, width, height, rgba));
    }
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(width, height, rgba)?);
    let scaled = img.thumbnail(MAX_EDGE, MAX_EDGE);
    Some(store_rgba(name, scaled.width(), scaled.height(), scaled.to_rgba8().into_raw()))
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
            // Same rejection `RgbaImage::from_raw` performed: a plane whose pixels don't
            // match its dimensions is dropped rather than stored at the wrong size.
            if rgba.len() != (t.width as usize) * (t.height as usize) * 4 {
                return None;
            }
            // Already within the budget — store the pixels as they are. `thumbnail` resizes
            // whatever it is given, and bike sheets are authored at exactly `MAX_EDGE`, so
            // this used to resample 1024² to 1024² once per sheet: 78 ms of the 120 ms a
            // bike spent on textures, for pixels that came out identical. [`into_texture`]
            // has always had this guard; this path was missing it.
            store_capped(&t.name, t.width, t.height, rgba)
        })
        .collect()
}

/// A decoded texture, downscaled if it is larger than the viewer needs, moved into the
/// texture store.
///
/// Takes the pixels rather than borrowing them: a 4096² plane is 67 MB, and copying one is
/// the same order of work as the downscale it feeds.
pub fn into_texture(t: PntTexture) -> PaintTexture {
    let PntTexture {
        name,
        width,
        height,
        rgba,
    } = t;
    // `from_raw` takes the buffer and hands back nothing when it isn't `w*h*4`, so the length
    // is checked here instead — a truncated plane then goes through verbatim, exactly as
    // before, and the viewer renders it grey.
    if width.max(height) > MAX_EDGE && rgba.len() == (width as usize) * (height as usize) * 4 {
        if let Some(img) = image::RgbaImage::from_raw(width, height, rgba) {
            let scaled = image::DynamicImage::ImageRgba8(img).thumbnail(MAX_EDGE, MAX_EDGE);
            return store_rgba(
                &name,
                scaled.width(),
                scaled.height(),
                scaled.to_rgba8().into_raw(),
            );
        }
        // Unreachable — the only thing `from_raw` rejects is the length checked above.
        return store_rgba(&name, width, height, Vec::new());
    }
    store_rgba(&name, width, height, rgba)
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

/// Whether the viewer can do anything with a texture of this name.
///
/// It samples a diffuse and — the one companion it does read — a `_n` normal map. Every other
/// map is a full-size plane inflated, downscaled, shipped over IPC and turned into a mipmapped
/// `DataTexture` that nothing ever binds: a gear paint's roughness sheet alone is 4096², and
/// dropping it is the difference between allocating 67 MB and not.
///
/// Safe to drop rather than merely ignore, because the binder never points a submesh at one:
/// [`crate::edf::is_companion_texture`] is what keeps them out of the declared list in the
/// first place, and reusing it here is what stops the two notions drifting apart.
fn viewer_binds(name: &str) -> bool {
    !crate::edf::is_companion_texture(name) || name.to_ascii_lowercase().ends_with("_n")
}

pub fn unpack_file(path: &Path) -> Result<Vec<PaintTexture>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {path:?}"))?;
    Ok(decode_any_where(&bytes, viewer_binds)?
        .into_par_iter()
        .map(into_texture)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What opening a paint in the viewer actually costs, against a real one. Run under the
    /// `dev` profile — that's what `tauri dev` builds, and it's where this was ever slow:
    /// `MXB_PNT=<paint.pnt> cargo test --bins unpack_file_timing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn unpack_file_timing() {
        let path = std::env::var("MXB_PNT").expect("set MXB_PNT");
        let t = std::time::Instant::now();
        let texs = unpack_file(std::path::Path::new(&path)).expect("unpack");
        eprintln!("unpack_file({path}) -> {} texture(s) in {:?}", texs.len(), t.elapsed());
        for x in &texs {
            eprintln!("  '{}' {}x{}", x.name, x.width, x.height);
        }
    }

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

    /// Both name walks over a folder of real paints, sealed ones included: same answers,
    /// and what each costs to get them. `MXB_PNT_DIR=<folder> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn texture_names_at_against_real_paints() {
        let Ok(dir) = std::env::var("MXB_PNT_DIR") else {
            return;
        };
        let paints: Vec<_> = std::fs::read_dir(&dir)
            .expect("read dir")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pnt")))
            .collect();
        assert!(!paints.is_empty(), "no .pnt files in {dir}");

        for p in &paints {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);

            let t = std::time::Instant::now();
            let seeked = texture_names_at(p);
            let seek_took = t.elapsed();

            let bytes = std::fs::read(p).expect("read");
            let t = std::time::Instant::now();
            let inflated =
                decode_any(&bytes).map(|v| v.iter().map(|x| x.name.clone()).collect::<Vec<_>>());
            let full_took = t.elapsed();

            eprintln!(
                "{name} ({:.1} MB)  names {seek_took:?}  vs  decode {full_took:?}",
                size as f64 / 1e6
            );
            match (seeked, inflated) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "{name}: the two walks must agree"),
                (Err(a), Err(_)) => eprintln!("  both refused: {a:#}"),
                (a, b) => panic!("{name}: one walk read it and the other didn't: {a:?} / {b:?}"),
            }
        }
    }

    fn write_tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join(format!("frost-pnt-{name}-{}.pnt", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// The names a paint on disk declares, without the paint being read.
    ///
    /// Proven by corrupting the pixels rather than by timing: the headers stay intact and
    /// the payloads become garbage, so a walk that inflates anything fails and a walk that
    /// only seeks does not. The second texture's name sits past the first's payload, so this
    /// also covers the seek actually landing where the header said it would.
    #[test]
    fn texture_names_at_reads_headers_and_never_the_pixels() {
        let bytes = encode("Outfit", &[tex("rider", 32, 32), tex("rider_n", 16, 16)]).unwrap();
        let clean = write_tmp("clean", &bytes);
        assert_eq!(
            texture_names_at(&clean).unwrap(),
            vec!["rider", "rider_n"],
            "same answer as the in-memory walk"
        );

        // The first texture's payload, flipped byte for byte. Every header — including the
        // second texture's, which sits on the far side of it — is left exactly as written.
        let mut wrecked = bytes.clone();
        let payload = HEADER_SIZE + IMAGE_HEADER_SIZE + IMAGE_PADDING;
        let end = payload
            + read_u32(&bytes, HEADER_SIZE + NAME_SIZE + 8 + DIGEST_SIZE).unwrap() as usize
            - IMAGE_PADDING;
        for b in wrecked[payload..end].iter_mut() {
            *b = !*b;
        }
        let bad = write_tmp("wrecked", &wrecked);
        assert!(decode(&wrecked).is_err(), "the pixels really are unreadable");
        assert_eq!(
            texture_names_at(&bad).unwrap(),
            vec!["rider", "rider_n"],
            "classification never touches them"
        );

        let _ = std::fs::remove_file(&clean);
        let _ = std::fs::remove_file(&bad);
    }

    /// A sealed paint has no headers to seek through, so it takes the whole-file path — and
    /// a file that is neither is an error, not a guess.
    #[test]
    fn texture_names_at_rejects_what_is_not_a_paint() {
        let p = write_tmp("garbage", b"this is not a paint file, not even close");
        assert!(texture_names_at(&p).is_err());
        let _ = std::fs::remove_file(&p);
    }

    /// A corrupt header states dimensions no texture has. The reservation it implies is the
    /// machine's memory, so it has to be refused before the allocator sees it.
    #[test]
    fn absurd_dimensions_are_refused_rather_than_allocated() {
        assert!(texture_bytes(4096, 4096).is_ok(), "a real sheet still decodes");
        assert!(texture_bytes(u32::MAX, u32::MAX).is_err());

        let mut bytes = encode("Paint", &[tex("livery", 4, 4)]).unwrap();
        let dims = HEADER_SIZE + NAME_SIZE;
        bytes[dims..dims + 8].copy_from_slice(&[0xFF; 8]);
        let err = decode(&bytes).unwrap_err();
        assert!(format!("{err:#}").contains("not a texture"), "{err:#}");
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
    fn into_texture_stores_pixels_verbatim() {
        let mut texs = decode(FIXTURE_PNT).unwrap();
        let out = into_texture(texs.remove(0));
        assert_eq!((out.width, out.height), (4, 4));
        assert_eq!(
            *crate::texstore::get(&out.token).expect("token resolves"),
            fixture_stored_pixels()
        );
    }

    /// The decode went parallel; the file format did not. Every texture must still come back
    /// in the order the image table lists it — material indices count that list, so a shuffle
    /// would silently move a paint's textures onto the wrong parts.
    #[test]
    fn decode_keeps_the_files_texture_order() {
        let texs = [tex("livery", 4, 4), tex("livery_n", 2, 2), tex("plate", 8, 8)];
        let bytes = encode("p", &texs).unwrap();
        let out = decode(&bytes).expect("decode what we just encoded");
        assert_eq!(
            out.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["livery", "livery_n", "plate"],
        );
        for (a, b) in out.iter().zip(&texs) {
            assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba));
        }
    }

    /// Filtering happens before the inflate, and it must not disturb what survives it.
    #[test]
    fn decode_where_skips_only_what_it_is_told_to() {
        let texs = [tex("livery", 4, 4), tex("livery_n", 2, 2), tex("livery_r", 8, 8)];
        let bytes = encode("p", &texs).unwrap();
        let out = decode_where(&bytes, |n| !n.ends_with("_r")).expect("decode");
        assert_eq!(
            out.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["livery", "livery_n"],
        );
        assert_eq!(out[1].rgba, texs[1].rgba, "the kept planes decode unchanged");
    }

    /// The viewer samples a diffuse and a `_n` normal map. Dropping `_n` would strip the
    /// relief off every gear paint, which is exactly the mistake this filter invites.
    #[test]
    fn viewer_binds_keeps_the_normal_map_and_nothing_else() {
        assert!(viewer_binds("rider"), "the livery itself");
        assert!(viewer_binds("rider_n"), "the normal map the viewer binds");
        assert!(!viewer_binds("rider_r"), "roughness");
        assert!(!viewer_binds("rider_s"), "specular");
        assert!(!viewer_binds("rider_roughness"), "the exporter's spelling");
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
