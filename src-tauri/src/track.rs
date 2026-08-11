//! Reading a track well enough to show it.
//!
//! A track is the largest thing the app handles — a few hundred megabytes of textures and
//! scenery around a terrain grid that is the only part worth drawing. Everything here is
//! arranged so that showing one costs the grid and nothing else:
//!
//! * The inventory ([`read_info`]) never inflates a byte. Naming a track's parts is a
//!   question the archive's index already answers, and answering it that way is the
//!   difference between a view that opens at once and one that waits on a 400 MB unpack.
//! * The terrain is inflated once, probed once, and reduced immediately to a master grid
//!   small enough to keep ([`MASTER_DIM`]). Every level of detail the viewer asks for after
//!   that is resampled from the master, in memory.
//! * The master is written to disk, so opening the same track tomorrow skips the inflate and
//!   the probe both. This is the cache that matters: the probe is milliseconds, while pulling
//!   a heightfield out of a large archive is most of a second.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::heightfield::{self, Layout};

/// The resolution the master grid is kept at. Chosen against the ground rather than the
/// screen: a track is a few hundred metres across, and at 512 one sample stood for more than
/// a metre — wider than the jump faces and ruts that make a track readable, so they were
/// averaged away before the viewer ever saw them.
const MASTER_DIM: u32 = 1024;

/// Cache generation. Bump when the blob layout or the probe changes shape, so entries
/// written by an older build are ignored rather than misread.
// v2: grids are metres where the file states a height scale, so v1's raw-unit masters
// would now be read as though they were already converted.
// v3: masters are kept at twice the resolution, and a v2 entry would pin a track to the
// coarser grid for as long as it stayed cached.
// v4: `.trh` samples are read signed, so every height below the datum has moved — a v3 entry
// holds the ground below a track standing as a wall around it.
const CACHE_DIR: &str = "track-terrain-v4";

/// How many cached masters to keep. Four megabytes each at [`MASTER_DIM`], so this holds the
/// same budget the smaller grid did rather than quadrupling what the cache costs on disk.
const CACHE_KEEP: usize = 16;

/// Files that carry a terrain grid. Just the one: PiBoSo's track guide has `.trh` as the
/// collision terrain and `.map` as the track *graphics*, so probing a `.map` would mean
/// inflating the largest file in the archive to discover it was never a heightfield.
const HEIGHTFIELD_EXTS: [&str; 1] = ["trh"];

/// What a track file is for, as far as the UI is concerned. A key, not prose — the app
/// translates it.
fn role_of(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "trh" => "heightfield",
        "map" => "graphics",
        "tsc" => "scenery",
        "rdf" => "road",
        "ssc" => "surfaces",
        "amb" => "ambience",
        "ini" | "cfg" => "config",
        "edf" => "model",
        "jpg" | "jpeg" | "png" | "dds" | "tga" | "bmp" => "image",
        "wav" | "ogg" => "sound",
        _ => "other",
    }
}

/// One file inside a track.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackFile {
    pub name: String,
    pub role: &'static str,
}

/// The shape of a track's terrain, once recovered.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TerrainInfo {
    /// Master grid dimensions — what the viewer can ask for, not the file's own resolution.
    pub width: u32,
    pub height: u32,
    /// The file's resolution before reduction, so the UI can say what it actually read.
    pub source_width: u32,
    pub source_height: u32,
    pub min_height: f32,
    pub max_height: f32,
    /// Metres covered by one master sample. Assumed when the track doesn't say (see
    /// `scale_known`) — the relief is still true, only its footprint is a guess.
    pub metres_per_sample: f32,
    pub scale_known: bool,
    /// Whether `min_height`/`max_height` are metres. When the height file didn't say what
    /// its samples mean they're raw units, and the relief is a number about nothing.
    pub heights_in_metres: bool,
    /// 0–1, from the probe. Anything recovered rather than stated is shown as inferred.
    pub confidence: f32,
    /// `header`, `ini` or `square` — how the grid's shape was decided.
    pub source: String,
    /// The archive entry it came out of.
    pub entry: String,
}

/// Everything the track view needs before it draws anything.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub meta: crate::pkz::PkzMeta,
    pub files: Vec<TrackFile>,
    /// Whether a heightfield-looking entry is present at all. Lets the view offer the 3D tab
    /// without first paying for the terrain, and say "no terrain in this track" when there
    /// genuinely isn't one rather than after a failed load.
    pub has_terrain: bool,
}

/// A decoded terrain, reduced to the master resolution.
#[derive(Clone)]
pub struct Master {
    pub info: TerrainInfo,
    pub heights: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Reading a track's parts, whether it's an archive or an unpacked folder
// ---------------------------------------------------------------------------

fn is_dir(path: &Path) -> bool {
    path.is_dir()
}

/// Every entry name in a track, without inflating any of them.
fn entry_names(path: &Path) -> Result<Vec<String>> {
    if is_dir(path) {
        let mut out = Vec::new();
        for entry in crate::linkwalk::walk_depth(path, 6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(path) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        return Ok(out);
    }
    crate::pkz::entry_names(path)
}

/// Pull one named entry's bytes out of a track.
fn read_entry(path: &Path, name: &str) -> Result<Vec<u8>> {
    if is_dir(path) {
        return std::fs::read(path.join(name)).with_context(|| format!("read {name}"));
    }
    let want = name.to_ascii_lowercase();
    let found = crate::pkz::read_selected(path, |n| {
        n.replace('\\', "/").to_ascii_lowercase() == want
    })?;
    match found.into_iter().next() {
        Some((_, bytes)) => Ok(bytes),
        None => bail!("{name} is not in {path:?}"),
    }
}

/// The entries that could hold a terrain grid, best-looking first.
fn heightfield_entries(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names
        .iter()
        .filter(|n| {
            let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            HEIGHTFIELD_EXTS.contains(&ext.as_str())
        })
        .cloned()
        .collect();
    out.sort_by_key(|n| {
        let ext = n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        HEIGHTFIELD_EXTS
            .iter()
            .position(|e| *e == ext)
            .unwrap_or(usize::MAX)
    });
    out
}

/// Grid dimensions and sample spacing named in the track's `.ini`, when it names them.
///
/// Nothing here is required — it's a hint handed to the probe, which still has to satisfy
/// itself that the shape reads as terrain. Keys vary between track tools, so this stays
/// deliberately loose about where it finds them.
fn ini_hints(text: &str) -> (Option<(u32, u32)>, Option<f32>) {
    let mut samples: (Option<u32>, Option<u32>) = (None, None);
    let mut metres: (Option<f32>, Option<f32>) = (None, None);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            // The grid, in samples.
            "samples_x" | "nx" | "width" => samples.0 = value.parse().ok(),
            "samples_y" | "ny" | "height" => samples.1 = value.parse().ok(),
            // The ground it covers, in metres. Deliberately not treated as a sample count:
            // `size_x` is the track's width in metres, and reading it as a grid width is
            // how a 480 m track becomes a 480-sample one.
            "size_x" => metres.0 = value.parse().ok(),
            "size_z" | "size_y" => metres.1 = value.parse().ok(),
            // `scale` is the total height of the terrain in metres, not a spacing. Nothing
            // here uses it — the height file carries its own — but naming it keeps the next
            // reader from adopting it as one.
            _ => {}
        }
    }

    let dims = match samples {
        (Some(w), Some(h)) if w >= 2 && h >= 2 => Some((w, h)),
        _ => None,
    };
    // Spacing is the two together: metres across, divided by the steps between samples.
    let spacing = match (dims, metres.0) {
        (Some((w, _)), Some(x)) if x > 0.0 => Some(x / (w - 1) as f32),
        _ => None,
    };
    (dims, spacing)
}

/// The track's top-level `.ini` text, if it has one worth reading.
fn ini_text(path: &Path, names: &[String]) -> Option<String> {
    let idx = names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.to_ascii_lowercase().ends_with(".ini"))
        .min_by_key(|(_, n)| (n.matches('/').count(), n.len()))
        .map(|(i, _)| i)?;
    let bytes = read_entry(path, &names[idx]).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

// ---------------------------------------------------------------------------
// The two things the app asks for
// ---------------------------------------------------------------------------

/// A track's metadata and contents, without inflating anything.
pub fn read_info(app: &tauri::AppHandle, path: &str) -> Result<TrackInfo> {
    let p = Path::new(path);
    let meta = crate::pkz::read_meta_cached(app, path)?;
    let names = entry_names(p).unwrap_or_default();
    let has_terrain = !heightfield_entries(&names).is_empty();

    let mut files: Vec<TrackFile> = names
        .into_iter()
        .map(|name| {
            let role = role_of(&name);
            TrackFile { name, role }
        })
        .collect();
    files.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));

    Ok(TrackInfo {
        meta,
        files,
        has_terrain,
    })
}

/// Decode a track's terrain to the master grid, going to disk and to the archive only when
/// nothing nearer has it.
pub fn load_master(app: &tauri::AppHandle, path: &str) -> Result<Master> {
    let stamp = stamp(path)?;
    let key = cache_key(path, stamp);

    if let Some(hit) = memory_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        return Ok(hit);
    }
    if let Some(hit) = cache_file(app, &key).and_then(|f| read_cache(&f)) {
        remember(key, hit.clone());
        return Ok(hit);
    }

    let master = decode_master(Path::new(path))?;
    if let Some(f) = cache_file(app, &key) {
        write_cache(&f, &master);
        prune_cache(app);
    }
    remember(key, master.clone());
    Ok(master)
}

/// The expensive path: inflate a heightfield, work out its layout, reduce it.
fn decode_master(path: &Path) -> Result<Master> {
    let names = entry_names(path)?;
    let candidates = heightfield_entries(&names);
    if candidates.is_empty() {
        bail!("no heightfield in {path:?}");
    }

    let ini = ini_text(path, &names);
    let (hint, ini_scale) = ini.as_deref().map(ini_hints).unwrap_or((None, None));

    for entry in candidates {
        let Ok(bytes) = read_entry(path, &entry) else {
            continue;
        };
        let Some(layout) = heightfield::probe(&bytes, hint) else {
            log::debug!("[track] {entry} in {path:?} doesn't read as a heightfield");
            continue;
        };
        return Ok(build_master(&bytes, &layout, &entry, ini_scale));
    }

    bail!("nothing in {path:?} reads as a terrain grid")
}

fn build_master(bytes: &[u8], layout: &Layout, entry: &str, ini_scale: Option<f32>) -> Master {
    let (w, h, heights) = heightfield::read_grid(bytes, layout, MASTER_DIM);

    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for v in &heights {
        min = min.min(*v);
        max = max.max(*v);
    }

    // The master covers the same ground as the source at fewer samples, so the spacing grows
    // by exactly the reduction factor.
    let reduction = layout.width as f32 / w.max(1) as f32;
    // What the file itself said, first: a `.trh` states its own footprint, and that beats
    // both a hint from the track's `.ini` and the fallback of one metre per sample.
    let stated = layout.metres_per_sample.or(ini_scale);
    let metres_per_sample = stated.unwrap_or(1.0) * reduction;

    Master {
        info: TerrainInfo {
            width: w,
            height: h,
            source_width: layout.width,
            source_height: layout.height,
            min_height: if min.is_finite() { min } else { 0.0 },
            max_height: if max.is_finite() { max } else { 0.0 },
            metres_per_sample,
            scale_known: stated.is_some(),
            heights_in_metres: layout.height_scale.is_some(),
            confidence: layout.confidence,
            source: layout.source.to_string(),
            entry: entry.to_string(),
        },
        heights,
    }
}

/// Resample a master down for a view that wants fewer points, and pack it for the IPC
/// channel. See [`BLOB_HEADER`] for the layout the app unpacks.
pub fn terrain_blob(master: &Master, max_dim: u32) -> Vec<u8> {
    let (w, h, heights) = resample(master, max_dim);

    let mut out = Vec::with_capacity(BLOB_HEADER + heights.len() * 4);
    out.extend_from_slice(b"FTRN");
    out.extend_from_slice(&1u16.to_le_bytes()); // version
    // Bit 0: the sample spacing was stated by the track rather than assumed. The app needs
    // this to caption the view honestly — with the spacing assumed, the relief is drawn
    // against a footprint we guessed, so its steepness is not something to trust.
    // Bit 1: the heights are metres rather than the file's own raw units.
    let flags = u16::from(master.info.scale_known) | (u16::from(master.info.heights_in_metres) << 1);
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&master.info.min_height.to_le_bytes());
    out.extend_from_slice(&master.info.max_height.to_le_bytes());
    // Spacing scales with the reduction, so the terrain keeps its real footprint at any
    // level of detail — a coarse pass and a fine one have to sit in the same place.
    let spacing = master.info.metres_per_sample * (master.info.width as f32 / w.max(1) as f32);
    out.extend_from_slice(&spacing.to_le_bytes());
    // How sure the probe was of the layout, so a marginal read can say so rather than be
    // presented as the track's terrain.
    out.extend_from_slice(&master.info.confidence.to_le_bytes());
    for v in &heights {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Bytes before the grid in a terrain blob. The grid starts 4-byte aligned so the app can
/// read it as a `Float32Array` without copying it first.
pub const BLOB_HEADER: usize = 32;

// ---------------------------------------------------------------------------
// The overview map, laid over the terrain
// ---------------------------------------------------------------------------

/// Bytes before the pixels in a texture blob. Four-byte aligned, same reasoning as
/// [`BLOB_HEADER`].
pub const TEXTURE_HEADER: usize = 16;

/// How far a texture's proportions may differ from the grid's and still be believed to cover
/// the same ground. A map is drawn to the track's footprint, so it matches closely or it is
/// not a map of it.
const ASPECT_TOLERANCE: f32 = 0.02;

/// The entry holding a picture of the track *as ground*, if the track ships one.
///
/// Tracks carry two picture slots in `[ui]`, and only one of them is a map: `pic` is the
/// artwork shown while the track loads — a photograph, usually with the track's name across
/// it — while `pic_info` is the overhead view. The ARL packs name them exactly that way,
/// `track_image` against `track_map`.
///
/// An author who fills both slots with the same file has supplied artwork twice rather than
/// a map, so that is read as having no map at all. Stretching a photograph of a truck over
/// the terrain would be worse than the plain relief it replaced.
fn overview_entry(names: &[String], ini: Option<&str>) -> Option<String> {
    let text = ini?;
    let value = |key: &str| -> Option<String> {
        for line in text.lines() {
            // Section headers and blanks are simply not `key = value` lines, so skip them
            // rather than letting one end the search.
            let Some((k, v)) = line.trim().split_once('=') else {
                continue;
            };
            if k.trim().eq_ignore_ascii_case(key) {
                let v = v.trim();
                return (!v.is_empty()).then(|| v.to_string());
            }
        }
        None
    };

    let map = value("pic_info")?;
    if value("pic").is_some_and(|p| p.eq_ignore_ascii_case(&map)) {
        return None;
    }

    // The `.ini` names the file, not its path inside the archive.
    let want = map.to_ascii_lowercase();
    names
        .iter()
        .find(|n| {
            n.rsplit('/')
                .next()
                .unwrap_or(n)
                .to_ascii_lowercase()
                == want
        })
        .cloned()
}

/// Decode one named picture out of a track.
///
/// The format is taken from the entry's extension before the bytes are consulted, because a
/// `.tga` can't be recognised any other way: TGA carries no leading magic, so guessing from
/// content silently fails on every one of them — which would leave only the DDS tracks
/// textured while looking like the maps simply weren't there.
fn decode_overview(path: &Path, entry: &str) -> Option<image::DynamicImage> {
    let bytes = read_entry(path, entry).ok()?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes));
    match entry
        .rsplit('.')
        .next()
        .and_then(image::ImageFormat::from_extension)
    {
        Some(format) => reader.set_format(format),
        None => reader = reader.with_guessed_format().ok()?,
    }
    match reader.decode() {
        Ok(img) => Some(img),
        Err(e) => {
            log::info!("track overview {entry} didn't decode: {e}");
            None
        }
    }
}

/// Surface colours, in the order surfaces are ranked by how much of the track they cover.
///
/// Keyed to rank rather than to the id in the file, because that id cannot be resolved to a
/// material: every track examined carries the same six names (asphalt, grass, sand, kerb,
/// soil, concrete) byte for byte, so that table is a default the editor writes, and the masks
/// reference ids past the end of it. Colouring by it put a farm's fields down as soil.
///
/// Rank is a property of the track rather than a guess about the format: what covers the most
/// ground is the vegetation the circuit is cut through, on every track examined, so it leads.
/// The rest descend through the surfaces a track is built from — apron, dirt, hard standing.
const SURFACE_RANK_COLOURS: [[u8; 3]; 6] = [
    [104, 132, 74],  // the ground it's all cut through — grass
    [206, 188, 142], // apron / sand
    [138, 108, 74],  // worked soil
    [128, 128, 132], // hard standing
    [122, 140, 88],  // rougher vegetation
    [150, 122, 86],
];

fn surface_colour(rank: usize) -> [u8; 3] {
    SURFACE_RANK_COLOURS[rank.min(SURFACE_RANK_COLOURS.len() - 1)]
}

/// Where the terrain data stops and the material table begins.
///
/// Found by the table's own first entry rather than by walking: mask bytes are arbitrary and
/// contain plenty of values that read as a plausible record header, so a walk with no end in
/// sight runs off into the data. With the end known, the walk is bounded and exact.
fn material_table_offset(block: &[u8]) -> Option<usize> {
    block
        .windows(8)
        .position(|w| w == b"asphalt\0")
        .and_then(|p| p.checked_sub(4))
}

/// One painted surface: which id it is, and a byte of coverage per cell.
struct Coverage {
    id: u32,
    width: u32,
    height: u32,
    at: usize,
}

/// The coverage masks in a height file's trailing block.
///
/// Records start at 44 and are `id, value, width, height` then `width * height` bytes —
/// except that some carry no `value`, so a header is twelve bytes as often as sixteen and
/// each has to be tried. A record with zero dimensions is a surface named but never painted.
fn coverage_masks(block: &[u8]) -> Vec<Coverage> {
    let Some(end) = material_table_offset(block) else {
        return Vec::new();
    };
    let u32_at = |o: usize| -> Option<u32> {
        block.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let plausible = |v: u32| (16..=8192).contains(&v);

    let mut out = Vec::new();
    let mut o = 44usize;
    while o + 16 <= end {
        let (id, w16, h16) = (u32_at(o), u32_at(o + 8), u32_at(o + 12));
        let (w12, h12) = (u32_at(o + 4), u32_at(o + 8));

        if let (Some(id), Some(w), Some(h)) = (id, w16, h16) {
            if plausible(w) && plausible(h) && o + 16 + (w as usize * h as usize) <= end {
                out.push(Coverage { id, width: w, height: h, at: o + 16 });
                o += 16 + w as usize * h as usize;
                continue;
            }
            if w == 0 && h == 0 && id < 64 {
                o += 16; // named but never painted
                continue;
            }
        }
        if let (Some(id), Some(w), Some(h)) = (u32_at(o), w12, h12) {
            if plausible(w) && plausible(h) && o + 12 + (w as usize * h as usize) <= end {
                out.push(Coverage { id, width: w, height: h, at: o + 12 });
                o += 12 + w as usize * h as usize;
                continue;
            }
        }
        o += 4;
    }
    out
}

/// A picture of what the track is *made of*, built from its coverage masks.
///
/// Every track carries these, including the ones that ship no overview picture at all, so
/// this is what gives those a surface instead of a bare elevation ramp. Each cell takes the
/// colour of whichever surface covers it most; cells nothing covers are left as bare dirt,
/// which is exactly what the riding line is.
fn surface_blob(path: &Path, max_dim: u32) -> Option<Vec<u8>> {
    let names = entry_names(path).ok()?;
    let entry = heightfield_entries(&names).into_iter().next()?;
    let bytes = read_entry(path, &entry).ok()?;

    let gw = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
    let gh = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?) as usize;
    let block = bytes.get(12 + gw.checked_mul(gh)?.checked_mul(2)?..)?;

    let masks = coverage_masks(block);
    if masks.is_empty() {
        return None;
    }

    // Ranked by how much ground each covers, sampled rather than summed in full — a dozen
    // masks of four million cells is a lot of adding to decide an ordering.
    let mut ranked: Vec<(usize, u64)> = masks
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let cells = m.width as usize * m.height as usize;
            let total: u64 = (0..cells)
                .step_by(97)
                .filter_map(|c| block.get(m.at + c))
                .map(|&v| v as u64)
                .sum();
            (i, total)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    let mut rank_of = vec![0usize; masks.len()];
    for (rank, (i, _)) in ranked.iter().enumerate() {
        rank_of[*i] = rank;
    }

    let src_w = masks[0].width as usize;
    let src_h = masks[0].height as usize;
    let scale = (src_w.max(src_h) as f32 / max_dim.max(1) as f32).ceil().max(1.0) as usize;
    let out_w = src_w.div_ceil(scale);
    let out_h = src_h.div_ceil(scale);

    // Bare dirt, for everything no surface was painted over.
    const BARE: [u8; 3] = [124, 96, 70];

    let mut px = Vec::with_capacity(out_w * out_h * 4);
    for y in 0..out_h {
        for x in 0..out_w {
            let mut best = 0u8;
            let mut colour = BARE;
            for (i, m) in masks.iter().enumerate() {
                // Masks may differ in size from one another; sample each in its own space.
                let mx = x * scale * m.width as usize / src_w.max(1);
                let my = y * scale * m.height as usize / src_h.max(1);
                let Some(&v) = block.get(m.at + my.min(m.height as usize - 1) * m.width as usize
                    + mx.min(m.width as usize - 1))
                else {
                    continue;
                };
                if v > best {
                    best = v;
                    colour = surface_colour(rank_of[i]);
                }
            }
            px.extend_from_slice(&colour);
            px.push(255);
        }
    }

    let mut out = Vec::with_capacity(TEXTURE_HEADER + px.len());
    out.extend_from_slice(b"FTEX");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(out_w as u32).to_le_bytes());
    out.extend_from_slice(&(out_h as u32).to_le_bytes());
    out.extend_from_slice(&px);
    Some(out)
}

/// A track's overview map, decoded and reduced, ready to lay over the terrain.
///
/// `None` — never an error — when the track has no map, when its map won't decode, or when
/// what it names doesn't cover the same ground as the grid. Every one of those is a track
/// that simply draws without a texture, which is what it did before.
pub fn overview_blob(path: &Path, max_dim: u32, grid_aspect: f32) -> Option<Vec<u8>> {
    // A picture of the track beats a diagram of it — the author's overhead view carries ruts
    // and tyre marks that no colour-per-surface can. Only the tracks without one fall through
    // to their coverage masks, and every track has those.
    picture_blob(path, max_dim, grid_aspect).or_else(|| surface_blob(path, max_dim))
}

/// The track's own overhead picture, when it ships one drawn to the same ground as the grid.
fn picture_blob(path: &Path, max_dim: u32, grid_aspect: f32) -> Option<Vec<u8>> {
    let names = entry_names(path).ok()?;
    let ini = ini_text(path, &names);
    let entry = overview_entry(&names, ini.as_deref())?;

    let img = decode_overview(path, &entry)?;

    // Proportions are the check that this is a map *of this track*. A square picture over a
    // grid twice as wide as it is tall would be drawn stretched across ground it never
    // described, and it would look plausible while being fiction.
    let aspect = img.width() as f32 / img.height().max(1) as f32;
    if grid_aspect > 0.0 && (aspect / grid_aspect - 1.0).abs() > ASPECT_TOLERANCE {
        log::info!(
            "track overview {entry} is {}x{} ({aspect:.2}:1) but the grid is {grid_aspect:.2}:1 — not a map of this track",
            img.width(),
            img.height(),
        );
        return None;
    }

    let img = if img.width().max(img.height()) > max_dim {
        img.thumbnail(max_dim, max_dim)
    } else {
        img
    };
    // RGBA rather than RGB: it's the only layout three.js still takes for a data texture,
    // and four bytes a pixel keeps the block aligned so the app can read it in place.
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());

    let mut out = Vec::with_capacity(TEXTURE_HEADER + (w * h * 4) as usize);
    out.extend_from_slice(b"FTEX");
    out.extend_from_slice(&1u16.to_le_bytes()); // version
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved, keeps the pixels aligned
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(rgba.as_raw());
    Some(out)
}

fn resample(master: &Master, max_dim: u32) -> (u32, u32, Vec<f32>) {
    let w = master.info.width as usize;
    let h = master.info.height as usize;
    let max_dim = max_dim.max(1) as usize;
    let longest = w.max(h);
    if longest <= max_dim {
        return (master.info.width, master.info.height, master.heights.clone());
    }

    // Land on exactly the size asked for. Reducing by a whole number of samples instead
    // overshoots badly at the sizes that matter: a 410-sample master asked for 320 can only
    // step by 2, which draws 205 — half the detail requested, and a jump face is a couple of
    // metres wide, so the samples lost are the track itself.
    let out_w = ((w * max_dim) / longest).max(1);
    let out_h = ((h * max_dim) / longest).max(1);

    let mut out = Vec::with_capacity(out_w * out_h);
    for oy in 0..out_h {
        // The source rows this output row stands for. Ends are derived from the ratio rather
        // than a fixed stride, so the blocks tile the grid exactly however it divides.
        let y0 = oy * h / out_h;
        let y1 = (((oy + 1) * h) / out_h).max(y0 + 1).min(h);
        for ox in 0..out_w {
            let x0 = ox * w / out_w;
            let x1 = (((ox + 1) * w) / out_w).max(x0 + 1).min(w);

            let mut total = 0.0f64;
            let mut count = 0usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    total += master.heights[y * w + x] as f64;
                    count += 1;
                }
            }
            out.push(if count == 0 {
                0.0
            } else {
                (total / count as f64) as f32
            });
        }
    }
    (out_w as u32, out_h as u32, out)
}

/// An account of what this track's terrain looks like to the reader, in plain text.
///
/// The height format is undocumented, so "no terrain to show" is not something a player —
/// or whoever has to fix the reader — can act on. This is the answer that is actually
/// useful: what the track contains, what its `.ini` claimed, every layout the probe
/// considered for each candidate file, and what it settled on. Surfaced in the viewer when
/// a track won't load, so diagnosing a format we've never seen needs nothing but the app.
pub fn diagnose(path: &Path) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let names = match entry_names(path) {
        Ok(n) => n,
        Err(e) => return format!("couldn't list {path:?}: {e:#}"),
    };
    let _ = writeln!(out, "{}\n{} entries", path.display(), names.len());
    for n in &names {
        let role = role_of(n);
        if !matches!(role, "other" | "image") {
            let _ = writeln!(out, "  [{role:<11}] {n}");
        }
    }

    let ini = ini_text(path, &names);
    let (hint, scale) = ini.as_deref().map(ini_hints).unwrap_or((None, None));
    let _ = writeln!(out, "ini dimensions {hint:?}, spacing {scale:?}");

    let heightfields = heightfield_entries(&names);
    if heightfields.is_empty() {
        out.push_str("no heightfield-looking entries — nothing for the probe to read\n");
    }
    for entry in heightfields {
        let _ = writeln!(out, "\n--- {entry} ---");
        match read_entry(path, &entry) {
            // The first bytes are worth having verbatim: if the probe found nothing, a magic
            // string or a dimension pair in here is what tells us how to widen it.
            Ok(bytes) => {
                let _ = writeln!(out, "first bytes: {}", hex_preview(&bytes, 64));
                let _ = writeln!(out, "{}", heightfield::report(&bytes, hint));
            }
            Err(e) => {
                let _ = writeln!(out, "couldn't read it: {e:#}");
            }
        }
    }

    match decode_master(path) {
        Ok(m) => {
            let _ = writeln!(out, "\nterrain: {:#?}", m.info);
        }
        Err(e) => {
            let _ = writeln!(out, "\nno terrain: {e:#}");
        }
    }
    out
}

/// The leading bytes as hex and printable ASCII — the one view of an unknown format that is
/// always worth having, since a magic string or a stated dimension shows up here or nowhere.
fn hex_preview(bytes: &[u8], count: usize) -> String {
    let take = bytes.len().min(count);
    let hex: Vec<String> = bytes[..take].iter().map(|b| format!("{b:02x}")).collect();
    let ascii: String = bytes[..take]
        .iter()
        .map(|b| if b.is_ascii_graphic() { *b as char } else { '.' })
        .collect();
    format!("{}  |{ascii}|", hex.join(" "))
}

// ---------------------------------------------------------------------------
// Caches
// ---------------------------------------------------------------------------

/// Keyed on name, size and mtime rather than the full path, for the same reason `pkz` is:
/// pointing the app at a moved install shouldn't cold-start every track it has ever opened.
#[derive(Clone, Copy)]
struct Stamp {
    size: u64,
    mtime_ns: u128,
}

fn stamp(path: &str) -> Result<Stamp> {
    let m = std::fs::metadata(path).with_context(|| format!("stat {path}"))?;
    Ok(Stamp {
        size: m.len(),
        mtime_ns: m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    })
}

fn cache_key(path: &str, stamp: Stamp) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{name}:{}:{}", stamp.size, stamp.mtime_ns)
}

fn memory_cache() -> &'static std::sync::Mutex<crate::lru::Lru<Master>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<crate::lru::Lru<Master>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(crate::lru::Lru::new(3)))
}

fn remember(key: String, master: Master) {
    if let Ok(mut c) = memory_cache().lock() {
        c.insert(key, master);
    }
}

fn cache_file(app: &tauri::AppHandle, key: &str) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_cache_dir().ok()?.join(CACHE_DIR);
    // A key holds a file name and two numbers; anything that isn't safe in a path is
    // flattened rather than escaped, since collisions are caught by the header check.
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Some(dir.join(format!("{safe}.bin")))
}

/// On-disk master: the terrain descriptor as JSON, then the raw grid.
fn write_cache(file: &Path, master: &Master) {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(head) = serde_json::to_vec(&master.info) else {
        return;
    };
    let mut out = Vec::with_capacity(8 + head.len() + master.heights.len() * 4);
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    for v in &master.heights {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let _ = std::fs::write(file, out);
}

fn read_cache(file: &Path) -> Option<Master> {
    let bytes = std::fs::read(file).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    let head_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let grid_at = 4 + head_len;
    if bytes.len() < grid_at {
        return None;
    }
    let info: TerrainInfo = serde_json::from_slice(&bytes[4..grid_at]).ok()?;

    let expected = info.width as usize * info.height as usize;
    let grid = &bytes[grid_at..];
    if grid.len() != expected * 4 {
        return None;
    }
    let heights = grid
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Some(Master { info, heights })
}

/// Keep the cache from growing without limit. Oldest first, since the newest are the tracks
/// being looked at now.
fn prune_cache(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Ok(base) = app.path().app_cache_dir() else {
        return;
    };
    let dir = base.join(CACHE_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let modified = e.metadata().ok()?.modified().ok()?;
            p.is_file().then_some((modified, p))
        })
        .collect();
    if files.len() <= CACHE_KEEP {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    for (_, p) in files.iter().take(files.len() - CACHE_KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point this at a real track — `.pkz` or unpacked folder — to see whether its terrain
    /// reads, and if not, how close each candidate layout came:
    ///
    /// ```text
    /// FROST_TRACK="C:/.../mods/tracks/Hangtown.pkz" \
    ///   cargo test -- --ignored --nocapture read_a_real_track
    /// ```
    ///
    /// Prints the inventory, any `.ini` hints, a full probe report per heightfield
    /// candidate, and what the decoder settled on. This is the whole diagnostic in one
    /// command, so a format that doesn't read can be diagnosed from its output alone.
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK"]
    fn read_a_real_track() {
        let path = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track .pkz/folder");
        println!("{}", diagnose(Path::new(&path)));
    }

    const UI_INI: &str = "[info]\nname = Test\n\n[ui]\npic = Thumb.tga\npic_info = TMa.tga\n";

    #[test]
    fn the_overview_map_is_taken_from_the_map_slot_not_the_loading_picture() {
        let names = vec!["T/Thumb.tga".to_string(), "T/TMa.tga".to_string()];
        assert_eq!(
            overview_entry(&names, Some(UI_INI)).as_deref(),
            Some("T/TMa.tga"),
            "`pic` is the artwork shown while the track loads; `pic_info` is the overhead view",
        );
    }

    #[test]
    fn one_picture_in_both_slots_is_artwork_rather_than_a_map() {
        // Briarcliff does exactly this — and its `Main.tga` is a photograph with the track's
        // name painted across it, which over the terrain would be worse than no texture.
        let ini = "[ui]\npic = Main.tga\npic_info = Main.tga\n";
        let names = vec!["T/Main.tga".to_string()];
        assert_eq!(overview_entry(&names, Some(ini)), None);
    }

    #[test]
    fn a_track_that_names_no_map_has_none() {
        let names = vec!["T/track_image.tga".to_string()];
        assert_eq!(overview_entry(&names, Some("[ui]\npic = track_image.tga\n")), None);
        assert_eq!(overview_entry(&names, Some("[ui]\npic_info =\n")), None);
        assert_eq!(overview_entry(&names, None), None);
    }

    #[test]
    fn a_named_map_that_isnt_in_the_archive_is_not_invented() {
        let names = vec!["T/Thumb.tga".to_string()];
        assert_eq!(overview_entry(&names, Some(UI_INI)), None);
    }

    /// Point this at a real track to see whether its overview map is found, decoded and
    /// accepted as covering the same ground:
    ///
    /// ```text
    /// FROST_TRACK="…/SandPointMX.pkz" \
    ///   cargo test -- --ignored --nocapture read_a_real_overview
    /// ```
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK"]
    fn read_a_real_overview() {
        let path = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track .pkz/folder");
        let path = Path::new(&path);
        let names = entry_names(path).unwrap();
        let ini = ini_text(path, &names);
        let slot = overview_entry(&names, ini.as_deref());
        println!("map slot: {slot:?}");
        // Its own proportions, so a rejection can be told apart from a decode that failed.
        if let Some(e) = &slot {
            match decode_overview(path, e) {
                Some(img) => println!(
                    "  decodes to {}x{} ({:.2}:1)",
                    img.width(),
                    img.height(),
                    img.width() as f32 / img.height().max(1) as f32
                ),
                None => println!("  did not decode"),
            }
        }

        let aspect = match decode_master(path) {
            Ok(m) => m.info.source_width as f32 / m.info.source_height as f32,
            Err(e) => {
                println!("no terrain ({e:#}) — using 1.0");
                1.0
            }
        };
        match overview_blob(path, 1024, aspect) {
            Some(b) => {
                let w = u32::from_le_bytes(b[8..12].try_into().unwrap());
                let h = u32::from_le_bytes(b[12..16].try_into().unwrap());
                println!(
                    "grid aspect {aspect:.2}:1 -> texture {w}x{h}, {} bytes ({} px)",
                    b.len(),
                    w * h
                );
                assert_eq!(b.len(), TEXTURE_HEADER + (w * h * 4) as usize);
            }
            None => println!("grid aspect {aspect:.2}:1 -> no usable overview map"),
        }
    }

    #[test]
    fn a_view_gets_the_resolution_it_asked_for() {
        // The bug this guards: a whole-sample stride could only halve a 410-sample master,
        // so a view asking for 320 was handed 205 and drew half the detail it requested.
        let master = Master {
            info: TerrainInfo {
                width: 410,
                height: 410,
                source_width: 2049,
                source_height: 2049,
                min_height: 0.0,
                max_height: 1.0,
                metres_per_sample: 1.0,
                scale_known: true,
                heights_in_metres: true,
                confidence: 1.0,
                source: "trh".into(),
                entry: "t.trh".into(),
            },
            heights: vec![0.5; 410 * 410],
        };
        let (w, h, heights) = resample(&master, 320);
        assert_eq!((w, h), (320, 320));
        assert_eq!(heights.len(), 320 * 320);
    }

    #[test]
    fn a_view_bigger_than_the_master_is_left_alone() {
        let master = Master {
            info: TerrainInfo {
                width: 64,
                height: 32,
                source_width: 64,
                source_height: 32,
                min_height: 0.0,
                max_height: 1.0,
                metres_per_sample: 1.0,
                scale_known: true,
                heights_in_metres: true,
                confidence: 1.0,
                source: "trh".into(),
                entry: "t.trh".into(),
            },
            heights: vec![0.25; 64 * 32],
        };
        let (w, h, _) = resample(&master, 512);
        assert_eq!((w, h), (64, 32), "never invented detail the master hasn't got");
    }

    #[test]
    fn a_reduced_grid_keeps_its_proportions() {
        let master = Master {
            info: TerrainInfo {
                width: 2049,
                height: 1025,
                source_width: 2049,
                source_height: 1025,
                min_height: 0.0,
                max_height: 1.0,
                metres_per_sample: 1.0,
                scale_known: true,
                heights_in_metres: true,
                confidence: 1.0,
                source: "trh".into(),
                entry: "t.trh".into(),
            },
            heights: vec![0.0; 2049 * 1025],
        };
        let (w, h, heights) = resample(&master, 512);
        assert_eq!(w, 512);
        // Half the width, as the source is — a stride would have rounded both the same way
        // and squashed the track.
        assert!((h as i32 - 256).abs() <= 1, "got {h}");
        assert_eq!(heights.len(), (w * h) as usize);
    }

    /// A trailing block with `masks` painted surfaces, laid out the way a real one is.
    fn block_with(masks: &[(u32, u32, u32, bool)]) -> Vec<u8> {
        let mut b = vec![0u8; 44];
        for &(id, w, h, wide) in masks {
            b.extend_from_slice(&id.to_le_bytes());
            if wide {
                b.extend_from_slice(&1.0f32.to_le_bytes());
            }
            b.extend_from_slice(&w.to_le_bytes());
            b.extend_from_slice(&h.to_le_bytes());
            b.extend(std::iter::repeat(id as u8).take((w * h) as usize));
        }
        // The material table the editor writes into every track, which is what bounds the walk.
        b.extend_from_slice(&6u32.to_le_bytes());
        for name in ["asphalt", "grass", "sand", "kerb", "soil", "concrete"] {
            let mut n = name.as_bytes().to_vec();
            n.resize(16, 0);
            b.extend_from_slice(&n);
            b.extend_from_slice(&[0u8; 36]);
        }
        b
    }

    #[test]
    fn coverage_masks_are_walked_out_of_the_trailing_block() {
        let b = block_with(&[(4, 64, 32, true), (1, 64, 32, true)]);
        let m = coverage_masks(&b);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id, m[0].width, m[0].height), (4, 64, 32));
        assert_eq!((m[1].id, m[1].width, m[1].height), (1, 64, 32));
        // Each mask's bytes are its own, so the second is found at the right place rather
        // than somewhere inside the first.
        assert_eq!(b[m[1].at], 1);
    }

    #[test]
    fn a_record_without_its_value_field_is_still_read() {
        // Real files mix the two: the same track carries sixteen-byte headers and twelve-byte
        // ones, so a walk that assumes one size stops partway and loses the rest.
        let b = block_with(&[(12, 32, 32, true), (5, 32, 32, false)]);
        let m = coverage_masks(&b);
        assert_eq!(m.len(), 2, "a short header must not end the walk");
        assert_eq!(m[1].id, 5);
        assert_eq!(b[m[1].at], 5);
    }

    #[test]
    fn a_surface_that_was_never_painted_takes_no_space() {
        let b = block_with(&[(7, 0, 0, true), (1, 48, 48, true)]);
        let m = coverage_masks(&b);
        assert_eq!(m.len(), 1, "an empty record is skipped, not treated as a mask");
        assert_eq!(m[0].id, 1);
    }

    #[test]
    fn without_a_material_table_nothing_is_walked() {
        // The table is the only reliable end. Mask bytes read as plausible record headers all
        // the time, so a walk with no end runs off into the data and invents surfaces.
        assert!(material_table_offset(&[0u8; 512]).is_none());
        assert!(coverage_masks(&[0u8; 512]).is_empty());
    }

    #[test]
    fn every_rank_has_its_own_colour_and_ranks_beyond_the_last_still_answer() {
        let all: Vec<[u8; 3]> = (0..SURFACE_RANK_COLOURS.len()).map(surface_colour).collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "two surfaces sharing a colour cannot be told apart");
            }
        }
        // A track may paint more surfaces than there are colours; that must not panic.
        assert_eq!(surface_colour(99), *SURFACE_RANK_COLOURS.last().unwrap());
    }

    #[test]
    fn a_hex_preview_shows_bytes_and_their_ascii() {
        let line = hex_preview(b"TRH\0\x01\x02", 64);
        assert!(line.starts_with("54 52 48 00 01 02"), "{line}");
        assert!(line.ends_with("|TRH...|"), "{line}");
    }

    #[test]
    fn roles_come_from_the_extension() {
        assert_eq!(role_of("Hangtown/Hangtown.trh"), "heightfield");
        // Track *graphics*, not height data — so never a heightfield candidate.
        assert_eq!(role_of("Hangtown/Hangtown.map"), "graphics");
        assert_eq!(role_of("Hangtown/Hangtown.tsc"), "scenery");
        assert_eq!(role_of("Hangtown/Hangtown.ini"), "config");
        assert_eq!(role_of("Hangtown/preview.jpg"), "image");
        assert_eq!(role_of("localcross.amb"), "ambience");
        assert_eq!(role_of("gfx.cfg"), "config");
        assert_eq!(role_of("Hangtown/notes.txt"), "other");
    }

    #[test]
    fn only_the_trh_is_offered_as_a_heightfield() {
        let names = vec![
            "T/T.map".to_string(),
            "T/T.ini".to_string(),
            "T/T.trh".to_string(),
        ];
        assert_eq!(
            heightfield_entries(&names),
            vec!["T/T.trh"],
            "a .map is graphics — inflating it to probe would cost the most for nothing",
        );
    }

    #[test]
    fn a_track_with_no_heightfield_offers_nothing() {
        let names = vec!["T/T.ini".to_string(), "T/preview.jpg".to_string()];
        assert!(heightfield_entries(&names).is_empty());
    }

    /// A real MX Bikes track's descriptor, straight out of a published `.pkz`.
    const REAL_INI: &str = include_str!("fixtures/localcross.ini");

    #[test]
    fn a_real_track_ini_offers_the_probe_nothing() {
        // This matters more than it looks. A published track's `.ini` describes the race —
        // name, weather, photo pose — and says nothing whatever about the terrain grid. So
        // the `ini` hint path is close to dead in practice, and the probe has to recover the
        // shape from the height file itself. Anything that reads dimensions out of here is
        // inventing them.
        let (dims, scale) = ini_hints(REAL_INI);
        assert_eq!(dims, None, "a real track states no grid dimensions");
        assert_eq!(scale, None, "a real track states no sample spacing");
    }

    #[test]
    fn ini_hints_read_a_terrain_ini() {
        // The shape the track creation guide documents.
        let (dims, spacing) = ini_hints(
            "samples_x = 2049\nsamples_y = 2049\ndata = heightmap.raw\n\
             size_x = 480\nsize_z = 480\nscale = 3.2\n",
        );
        assert_eq!(dims, Some((2049, 2049)));
        // 480 m across 2048 steps.
        let spacing = spacing.expect("size and samples together give a spacing");
        assert!((spacing - 480.0 / 2048.0).abs() < 1e-6, "got {spacing}");
    }

    #[test]
    fn ini_hints_dont_read_metres_as_a_sample_count() {
        // `size_x` is the track's width in metres. Taken for a grid width it would turn a
        // 480 m track into a 480-sample one, which is the kind of wrong that still renders.
        let (dims, spacing) = ini_hints("size_x = 480\nsize_z = 480\n");
        assert_eq!(dims, None, "metres are not samples");
        assert_eq!(spacing, None, "spacing needs a sample count to divide by");
    }

    #[test]
    fn ini_hints_dont_take_scale_for_a_spacing() {
        // `scale` is the terrain's total height in metres. It is not metres-per-sample, and
        // a viewer that used it as one would draw the ground at a fraction of its size.
        let (_, spacing) = ini_hints("samples_x = 2049\nsamples_y = 2049\nscale = 3.2\n");
        assert_eq!(spacing, None);
    }

    #[test]
    fn ini_hints_ignore_a_track_that_says_nothing() {
        let (dims, spacing) = ini_hints("[info]\nname = Hangtown\n");
        assert_eq!(dims, None);
        assert_eq!(spacing, None);
    }

    /// A master built from a known grid, for the blob and resample tests.
    fn master(dim: u32) -> Master {
        let heights: Vec<f32> = (0..dim * dim)
            .map(|i| ((i % dim) as f32 / dim as f32) * 10.0)
            .collect();
        Master {
            info: TerrainInfo {
                width: dim,
                height: dim,
                source_width: dim * 2,
                source_height: dim * 2,
                min_height: 0.0,
                max_height: 10.0,
                metres_per_sample: 2.0,
                scale_known: true,
                heights_in_metres: true,
                confidence: 0.9,
                source: "header".into(),
                entry: "T/T.trh".into(),
            },
            heights,
        }
    }

    #[test]
    fn a_blob_carries_its_grid_behind_an_aligned_header() {
        let m = master(64);
        let blob = terrain_blob(&m, 64);
        assert_eq!(&blob[0..4], b"FTRN");
        assert_eq!(BLOB_HEADER % 4, 0, "the grid has to be readable in place");
        assert_eq!(blob.len(), BLOB_HEADER + 64 * 64 * 4);

        let w = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
        let h = u32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
        assert_eq!((w, h), (64, 64));
    }

    #[test]
    fn a_blob_carries_how_far_the_terrain_was_inferred() {
        let mut m = master(64);
        let known = terrain_blob(&m, 64);
        assert_eq!(u16::from_le_bytes([known[6], known[7]]) & 1, 1);
        assert!((f32::from_le_bytes([known[28], known[29], known[30], known[31]]) - 0.9).abs() < 1e-6);

        // A track that never stated its sample spacing has to come through as such — the
        // relief is real, the footprint it's drawn against is a guess.
        m.info.scale_known = false;
        let assumed = terrain_blob(&m, 64);
        assert_eq!(u16::from_le_bytes([assumed[6], assumed[7]]) & 1, 0);
    }

    #[test]
    fn a_coarse_blob_keeps_the_terrain_the_same_size_on_the_ground() {
        let m = master(64);
        let fine = terrain_blob(&m, 64);
        let coarse = terrain_blob(&m, 16);

        let spacing = |b: &[u8]| f32::from_le_bytes([b[24], b[25], b[26], b[27]]);
        let dims = |b: &[u8]| u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as f32;

        // Fewer samples, proportionally further apart — the footprint is unchanged, which is
        // what lets a coarse pass be replaced by a fine one without the terrain moving.
        assert!((dims(&fine) * spacing(&fine) - dims(&coarse) * spacing(&coarse)).abs() < 0.01);
    }

    #[test]
    fn resampling_below_the_master_is_a_no_op() {
        let m = master(64);
        let (w, h, grid) = resample(&m, 256);
        assert_eq!((w, h), (64, 64));
        assert_eq!(grid.len(), 64 * 64);
    }

    #[test]
    fn a_cached_master_round_trips() {
        let dir = std::env::temp_dir().join(format!("frost-track-cache-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("m.bin");

        let m = master(32);
        write_cache(&file, &m);
        let back = read_cache(&file).expect("a master just written should read back");

        assert_eq!(back.info.width, m.info.width);
        assert_eq!(back.info.entry, m.info.entry);
        assert_eq!(back.heights, m.heights);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_cache_entry_is_ignored() {
        let dir = std::env::temp_dir().join(format!("frost-track-trunc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("m.bin");

        let m = master(32);
        write_cache(&file, &m);
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 16]).unwrap();

        assert!(
            read_cache(&file).is_none(),
            "a short grid must be re-decoded, not read past",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}







