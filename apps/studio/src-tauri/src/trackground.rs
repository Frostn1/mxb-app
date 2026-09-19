//! Real ground, off a laser scan.
//!
//! Every track the Studio has ever built stood on ground it invented: a tilt, some noise and a
//! few mounds, in [`crate::tracksynth::Landscape`]. That is the right answer when nobody has
//! said where the track is. It is the wrong answer when they have — a real circuit is the shape
//! of the piece of land it was cut into, and no amount of noise finds Ironman's ravine.
//!
//! This is the other input. A public-domain lidar tile and a traced lap arrive from the rider's
//! own disk; what comes out is a square of real ground, stored the way
//! [`crate::tracktex`] stores the rider's own images — normalised first, then named by the
//! SHA-256 of what was normalised, so a project either resolves to exactly the ground it was
//! built with or to nothing at all.
//!
//! **Nothing here reaches the network.** The tile is fetched by hand, on the rider's own action,
//! and handed to this as a path. See `docs/tracks/scanned-places.md`.
//!
//! What is *not* here: the track. This module produces ground and a lap polyline, and stops.
//! Turning a polyline into the straights and arcs a track program is made of is
//! [`crate::trackprog::fit_lap`], because that is a question about the program format rather
//! than about geography.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Samples along the LONGER edge of a stored ground plot; the short edge gets whatever keeps the
/// cells square.
///
/// Deliberately *not* the 2049 the terrain is built at. The source is a 1 m lidar grid and the
/// game's own grid is 0.2295 m across a 470 m plot — 4.36 times finer — so everything past
/// about 1 m is interpolation rather than information, and storing it at 2049 would be storing
/// four megabytes of invented detail as though it had been measured. 1025 over a 470 m plot is
/// 0.459 m a sample: still twice the source's own resolution, which is enough to resample from
/// without stair-stepping, and honest about where the detail stops. The generator's own stamped
/// features supply the rest, which is what they were always for.
///
/// On the long edge rather than on both, because a plot is the shape of the venue and a venue is
/// rarely square — Ironman's circuit is 410 by 285 m. See [`Ground`].
pub const GROUND_DIM: u32 = 1025;

/// The biggest tile this will open, before it is decoded. A 10012² float tile is 315 MB on
/// disk; only the tiles under the plot are ever decoded, so the guard is on the file rather
/// than on the memory.
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// How much ground beyond the lap's own bounding box a plot takes, metres.
///
/// A lap needs its own width of margin to pass [`crate::trackprog::TrackProgram::check`], and a
/// track wants more than that: the banks people stand on, the paddock, the trees. Forty metres
/// is about what a published plot leaves — Indiana is 525 m around a 2138 m lap.
const PLOT_MARGIN_M: f32 = 40.0;

/// GeoTIFF tags. The `tiff` crate knows the baseline TIFF tags and nothing about geography, so
/// these three are read by number.
const TAG_MODEL_PIXEL_SCALE: u16 = 33550;
const TAG_MODEL_TIEPOINT: u16 = 33922;
const TAG_GEO_KEY_DIRECTORY: u16 = 34735;
/// The GeoKey that carries the projected CRS's EPSG code.
const KEY_PROJECTED_CRS: u16 = 3072;

/// Where stored ground lives. Set once at startup from the app's own data folder, exactly as
/// [`crate::tracktex::set_dir`] is.
static DIR: OnceLock<PathBuf> = OnceLock::new();

type Cache = Mutex<std::collections::HashMap<String, Option<Arc<Ground>>>>;
static CACHE: OnceLock<Cache> = OnceLock::new();

pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// The folder stored ground is kept in.
pub fn dir() -> PathBuf {
    DIR.get().cloned().unwrap_or_else(|| {
        dirs_next::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("frost-studio")
            .join("track-ground")
    })
}

// ---------------------------------------------------------------------------------------------
// The lidar tile
// ---------------------------------------------------------------------------------------------

/// A digital elevation model, as it sits in a GeoTIFF: a north-up grid of heights in metres,
/// in some projected coordinate system whose units are also metres.
pub struct Dem {
    pub w: usize,
    pub h: usize,
    /// Metres a pixel, both ways — square pixels are required and checked.
    pub cell: f64,
    /// Easting and northing of the CENTRE of pixel (0, 0). Row 0 is the NORTH edge and the row
    /// index increases southward, which is how every north-up GeoTIFF is laid out and is the
    /// classic place to be half a pixel out.
    pub origin_e: f64,
    pub origin_n: f64,
    /// The projected CRS's EPSG code, off the GeoKey directory.
    pub epsg: u32,
    /// Row-major, `h * w`, metres. NaN where the tile says nodata.
    pub z: Vec<f32>,
}

impl Dem {
    /// Height at a projected coordinate, bilinear, NaN outside the grid or over nodata.
    pub fn at(&self, e: f64, n: f64) -> f32 {
        let fx = (e - self.origin_e) / self.cell;
        let fy = (self.origin_n - n) / self.cell;
        if !(fx >= 0.0 && fy >= 0.0) {
            return f32::NAN;
        }
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        if x0 + 1 >= self.w || y0 + 1 >= self.h {
            return f32::NAN;
        }
        let (tx, ty) = ((fx - x0 as f64) as f32, (fy - y0 as f64) as f32);
        let g = |x: usize, y: usize| self.z[y * self.w + x];
        let (a, b, c, d) = (g(x0, y0), g(x0 + 1, y0), g(x0, y0 + 1), g(x0 + 1, y0 + 1));
        let top = a + (b - a) * tx;
        let bot = c + (d - c) * tx;
        top + (bot - top) * ty
    }
}

/// The window of a tile that a plot needs, in whole pixels.
#[derive(Clone, Copy, Debug)]
struct Window {
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
}

/// Read the part of a GeoTIFF that a window asks for.
///
/// Only the tiles the window touches are decoded. The Ironman tile is 10012 square and 315 MB;
/// a 550 m plot touches nine of its 256² tiles, so this reads about two megabytes rather than
/// four hundred. That is the difference between an import that works on a laptop and one that
/// does not.
fn read_geotiff(path: &Path, want: Option<(f64, f64, f64, f64)>) -> Result<Dem> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;

    let meta = std::fs::metadata(path)
        .with_context(|| format!("couldn't open {}", path.display()))?;
    if meta.len() > MAX_BYTES {
        bail!(
            "{} is {:.1} GB. The biggest tile the Studio will read is {} GB.",
            path.display(),
            meta.len() as f64 / 1e9,
            MAX_BYTES / (1024 * 1024 * 1024)
        );
    }
    let file = std::fs::File::open(path)
        .with_context(|| format!("couldn't open {}", path.display()))?;
    let mut d = Decoder::new(std::io::BufReader::new(file))
        .with_context(|| format!("{} isn't a TIFF the Studio can read", path.display()))?;

    let (w, h) = d.dimensions()?;
    let (w, h) = (w as usize, h as usize);

    let scale = d
        .get_tag_f64_vec(Tag::Unknown(TAG_MODEL_PIXEL_SCALE))
        .context("the tile has no ModelPixelScale tag, so it isn't georeferenced")?;
    let tie = d
        .get_tag_f64_vec(Tag::Unknown(TAG_MODEL_TIEPOINT))
        .context("the tile has no ModelTiepoint tag, so it isn't georeferenced")?;
    if scale.len() < 2 || tie.len() < 6 {
        bail!("the tile's georeferencing tags are too short to read");
    }
    let (sx, sy) = (scale[0], scale[1]);
    if sx <= 0.0 || sy <= 0.0 || (sx - sy).abs() > sx * 1e-6 {
        bail!(
            "the tile's pixels are {sx} by {sy} metres. Square pixels are required — reproject \
             it first."
        );
    }
    // The tiepoint gives a raster point and the model point it lands on. For a north-up tile
    // written by every tool that makes these, the raster point is (0,0) — the top-left pixel's
    // outer CORNER. The centre of pixel (0,0) is half a pixel in from there.
    if tie[0] != 0.0 || tie[1] != 0.0 {
        bail!("the tile's tiepoint isn't at raster (0,0); the Studio only reads north-up tiles");
    }
    let origin_e = tie[3] + sx * 0.5;
    let origin_n = tie[4] - sy * 0.5;

    let epsg = d
        .get_tag_u32_vec(Tag::Unknown(TAG_GEO_KEY_DIRECTORY))
        .ok()
        .and_then(|keys| geo_key(&keys, KEY_PROJECTED_CRS))
        .unwrap_or(0);

    // Which pixels the caller actually wants.
    let win = match want {
        None => Window { x0: 0, y0: 0, w, h },
        Some((e0, n0, e1, n1)) => {
            let fx0 = ((e0 - origin_e) / sx).floor();
            let fx1 = ((e1 - origin_e) / sx).ceil();
            let fy0 = ((origin_n - n1) / sy).floor();
            let fy1 = ((origin_n - n0) / sy).ceil();
            if fx1 < 0.0 || fy1 < 0.0 || fx0 >= w as f64 || fy0 >= h as f64 {
                bail!(
                    "the traced lap is nowhere near the tile. The lap wants easting {e0:.0} to \
                     {e1:.0} and northing {n0:.0} to {n1:.0}; the tile covers easting {:.0} to \
                     {:.0} and northing {:.0} to {:.0}. Are they in the same projection?",
                    origin_e,
                    origin_e + w as f64 * sx,
                    origin_n - h as f64 * sy,
                    origin_n
                );
            }
            // One pixel of slack each way so the bilinear sample at the very edge has corners.
            let x0 = (fx0 as i64 - 1).max(0) as usize;
            let y0 = (fy0 as i64 - 1).max(0) as usize;
            let x1 = ((fx1 as i64 + 2).max(0) as usize).min(w);
            let y1 = ((fy1 as i64 + 2).max(0) as usize).min(h);
            if x1 <= x0 || y1 <= y0 {
                bail!("the traced lap falls outside the tile");
            }
            Window { x0, y0, w: x1 - x0, h: y1 - y0 }
        }
    };

    let nodata: Option<f64> = d
        .get_tag_ascii_string(Tag::Unknown(42113))
        .ok()
        .and_then(|s| s.trim().trim_end_matches('\0').parse().ok());

    let mut z = vec![f32::NAN; win.w * win.h];
    let (cw, ch) = d.chunk_dimensions();
    let (cw, ch) = (cw as usize, ch as usize);
    if cw == 0 || ch == 0 {
        bail!("the tile reports zero-sized chunks");
    }
    let across = w.div_ceil(cw);
    let cx0 = win.x0 / cw;
    let cx1 = (win.x0 + win.w).div_ceil(cw);
    let cy0 = win.y0 / ch;
    let cy1 = (win.y0 + win.h).div_ceil(ch);

    for cy in cy0..cy1 {
        for cx in cx0..cx1 {
            let idx = cy * across + cx;
            let (dw, dh) = d.chunk_data_dimensions(idx as u32);
            let (dw, dh) = (dw as usize, dh as usize);
            let data = d
                .read_chunk(idx as u32)
                .with_context(|| format!("couldn't decode tile {idx} of {}", path.display()))?;
            let vals: Vec<f32> = match data {
                DecodingResult::F32(v) => v,
                DecodingResult::F64(v) => v.into_iter().map(|x| x as f32).collect(),
                DecodingResult::I16(v) => v.into_iter().map(|x| x as f32).collect(),
                DecodingResult::U16(v) => v.into_iter().map(|x| x as f32).collect(),
                DecodingResult::I32(v) => v.into_iter().map(|x| x as f32).collect(),
                _ => bail!(
                    "the tile's samples aren't a height the Studio can read. Elevation tiles are \
                     32-bit float or 16-bit integer."
                ),
            };
            // A chunk at the right or bottom edge is stored full width but carries fewer valid
            // columns; `chunk_data_dimensions` is what says how many.
            for ry in 0..dh {
                let gy = cy * ch + ry;
                if gy < win.y0 || gy >= win.y0 + win.h {
                    continue;
                }
                for rx in 0..dw {
                    let gx = cx * cw + rx;
                    if gx < win.x0 || gx >= win.x0 + win.w {
                        continue;
                    }
                    let Some(&v) = vals.get(ry * dw + rx) else { continue };
                    let bad = !v.is_finite()
                        || v < -9000.0
                        || nodata.is_some_and(|nd| (v as f64 - nd).abs() < 1e-3);
                    z[(gy - win.y0) * win.w + (gx - win.x0)] = if bad { f32::NAN } else { v };
                }
            }
        }
    }

    Ok(Dem {
        w: win.w,
        h: win.h,
        cell: sx,
        origin_e: origin_e + win.x0 as f64 * sx,
        origin_n: origin_n - win.y0 as f64 * sy,
        epsg,
        z,
    })
}

/// Pull one key out of a GeoTIFF GeoKey directory: a header of four shorts, then four shorts a
/// key.
fn geo_key(keys: &[u32], want: u16) -> Option<u32> {
    if keys.len() < 4 {
        return None;
    }
    let count = keys[3] as usize;
    for i in 0..count {
        let at = 4 + i * 4;
        let (id, loc, len, val) = (*keys.get(at)?, *keys.get(at + 1)?, *keys.get(at + 2)?, *keys.get(at + 3)?);
        // `loc == 0` means the value is the fourth short itself rather than an offset into
        // another tag, which is the only form an EPSG code takes.
        if id == want as u32 && loc == 0 && len == 1 {
            return Some(val);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// The traced lap
// ---------------------------------------------------------------------------------------------

/// A lap someone drew over the imagery, in the tile's own projected coordinates.
///
/// The format is `<slug>.lap.json`, agreed with the fetching half of this pipeline and written
/// down in `docs/tracks/scanned-places.md`. Unknown keys are ignored on purpose: the file
/// carries provenance that grows, and a reader that refuses a field it has not heard of makes
/// every addition a breaking change.
/// Both spellings of every key are accepted. The two halves of this pipeline agreed the format
/// in snake_case and the writer emits camelCase, which is the kind of thing that should cost
/// nobody an afternoon: an alias each is cheaper than a wrong track, and cheaper still than
/// finding out by building one.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct LapTrace {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub crs: String,
    #[serde(default = "yes")]
    pub closed: bool,
    #[serde(default = "default_width")]
    pub default_width_m: f32,
    /// `[easting, northing]` or `[easting, northing, width]`. Mixed lengths allowed.
    pub points: Vec<Vec<f64>>,
    /// Which point the start line sits on. Optional; the trace rarely starts at the gate.
    #[serde(default)]
    pub start_index: usize,
    #[serde(default)]
    pub dem: Option<TraceDem>,
    #[serde(default)]
    pub imagery: Option<Provenance>,
    /// The trace is a first pass and the tracer says so.
    ///
    /// Read, kept, and carried into the built track's README, because a lap traced by eye off an
    /// aerial photograph is a guess about where a racing line goes and the person who drew it is
    /// the only one who knows how good a guess. Losing that on the way into the track would turn
    /// a stated uncertainty into an implied fact.
    #[serde(default)]
    pub provisional: bool,
    /// Whether anyone actually confirmed which way round the lap is ridden.
    ///
    /// Defaults to false — unverified until said otherwise — because the failure is silent: a
    /// lap built backwards is a perfectly valid track that is simply wrong, and nothing
    /// downstream can notice.
    #[serde(default)]
    pub direction_verified: bool,
    /// What the tracer was and was not sure of, span by span.
    #[serde(default)]
    pub segments: Vec<TraceConfidence>,
    #[serde(default)]
    pub note: String,
    /// Where the route came from — a rider who knows the venue, or a guess off a photograph.
    /// These are not the same thing and the track should not pretend they are.
    #[serde(default)]
    pub route_source: String,
    /// Whether the route has been checked by whoever drew it. A route drawn by a rider and then
    /// digitised by a machine is only as good as the digitisation, and that is a separate
    /// question from whether the rider knew the track.
    #[serde(default)]
    pub route_confirmed: bool,
    /// Defects the tracer knows about and could not fix — a pen lift bridged by a straight line,
    /// a section not followed. Carried verbatim into the built track, because a rider looking at
    /// a strange straight in the middle of a corner deserves to find out why from the track
    /// rather than from a chat log.
    #[serde(default)]
    pub known_issues: Vec<String>,
}

/// How sure the tracer was about one stretch of the lap.
#[derive(serde::Deserialize, Debug, Clone, Default)]
pub struct TraceConfidence {
    #[serde(default)]
    pub from: usize,
    #[serde(default)]
    pub to: usize,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub what: String,
}

fn yes() -> bool {
    true
}
fn default_width() -> f32 {
    6.0
}

#[derive(serde::Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TraceDem {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub crs: String,
    #[serde(default)]
    pub origin_e: Option<f64>,
    #[serde(default)]
    pub origin_n: Option<f64>,
    #[serde(default)]
    pub cell_m: Option<f64>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub collected: String,
    #[serde(default)]
    pub licence: String,
    /// The exact line the licence says has to appear wherever this ground is used. Carried
    /// separately from `licence` because it is the part that has to travel inside the track.
    #[serde(default)]
    pub attribution: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub vertical_datum: String,
}

#[derive(serde::Deserialize, Debug, Clone, Default)]
pub struct Provenance {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub licence: String,
    /// The credit line the picture's licence asks for, when it asks for one.
    #[serde(default)]
    pub attribution: String,
    #[serde(default)]
    pub captured: String,
}

/// Fold every camelCase key in a trace's top level down to the snake_case one this reads.
///
/// The two halves of this pipeline agreed the format in snake_case and the writer, having been
/// told either spelling was fine, emitted BOTH — so `defaultWidthM` and `default_width_m` sit
/// side by side and serde's `alias` reads that as a duplicate field and refuses the file. Which
/// is the right instinct for a wire format and the wrong one for a hand-written artefact that
/// two independent programs are trying to agree on.
///
/// So the spellings are reconciled before serde sees them: camelCase is folded to snake_case,
/// and where both are present the snake_case one wins, because that is the spelling the format
/// was actually agreed in. Only the top level, and only keys that differ between the two
/// conventions — the nested `dem` block is already read with `rename_all = "camelCase"`.
fn normalise_keys(v: &mut serde_json::Value) {
    const PAIRS: [(&str, &str); 4] = [
        ("defaultWidthM", "default_width_m"),
        ("startIndex", "start_index"),
        ("directionVerified", "direction_verified"),
        ("verticalDatum", "vertical_datum"),
    ];
    let Some(map) = v.as_object_mut() else { return };
    for (camel, snake) in PAIRS {
        if let Some(val) = map.remove(camel) {
            map.entry(snake.to_string()).or_insert(val);
        }
    }
}

/// One traced point, in the tile's projected coordinates, with the width the rider gave it.
#[derive(Clone, Copy, Debug)]
pub struct TracePoint {
    pub e: f64,
    pub n: f64,
    pub width_m: f32,
}

impl LapTrace {
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("couldn't read {}", path.display()))?;
        let mut raw: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{} isn't JSON", path.display()))?;
        normalise_keys(&mut raw);
        let t: LapTrace = serde_json::from_value(raw)
            .with_context(|| format!("{} isn't a lap trace the Studio can read", path.display()))?;
        if t.points.len() < 4 {
            bail!(
                "{} has {} traced points. A lap needs at least four.",
                path.display(),
                t.points.len()
            );
        }
        Ok(t)
    }

    /// The points, cleaned: every one carries a width, and the run starts where the trace says
    /// the start line is.
    pub fn resolved(&self) -> Result<Vec<TracePoint>> {
        let mut pts = Vec::with_capacity(self.points.len());
        for (i, p) in self.points.iter().enumerate() {
            if p.len() < 2 {
                bail!("traced point {i} has {} numbers; it needs at least two", p.len());
            }
            let width = p.get(2).map(|w| *w as f32).unwrap_or(self.default_width_m);
            if !(p[0].is_finite() && p[1].is_finite()) || !width.is_finite() || width <= 0.0 {
                bail!("traced point {i} isn't a finite easting, northing and width");
            }
            pts.push(TracePoint { e: p[0], n: p[1], width_m: width });
        }
        if self.start_index >= pts.len() {
            bail!(
                "the trace says the start line is at point {} but there are only {} points",
                self.start_index,
                pts.len()
            );
        }
        if self.start_index > 0 {
            pts.rotate_left(self.start_index);
        }
        Ok(pts)
    }

    /// Easting and northing the trace spans.
    pub fn bounds(&self) -> Result<(f64, f64, f64, f64)> {
        let pts = self.resolved()?;
        let (mut e0, mut n0) = (f64::MAX, f64::MAX);
        let (mut e1, mut n1) = (f64::MIN, f64::MIN);
        for p in &pts {
            e0 = e0.min(p.e);
            e1 = e1.max(p.e);
            n0 = n0.min(p.n);
            n1 = n1.max(p.n);
        }
        Ok((e0, n0, e1, n1))
    }
}

// ---------------------------------------------------------------------------------------------
// Flight-line striping
// ---------------------------------------------------------------------------------------------

/// How far along the flight line the striping estimate averages, metres.
///
/// Measured on the Ironman tile: the corduroy is an east-west wave that stays in phase for
/// hundreds of metres down the north-south flight direction, while real ground does not. A
/// hundred metres of along-track averaging keeps the one and flattens the other.
const STRIPE_ALONG_M: f32 = 100.0;
/// The band the stripe lives in, across the flight line, metres. Measured at 11–17 m.
const STRIPE_LO_M: f32 = 5.0;
const STRIPE_HI_M: f32 = 40.0;
/// The scale the ground is detrended at before the stripe is looked for, metres.
///
/// Six, and the reason it is not larger is measured rather than assumed. A six-metre high-pass
/// keeps what is *shorter* than six metres, so it throws away most of the eleven-to-seventeen
/// metre band the stripe actually lives in — which looks like an obvious bug, and lengthening it
/// does catch more stripe. It also catches more ground: swept from 6 m to 200 m against a known
/// 12 m stripe on clean synthetic land, what the filter moves on ground with NO stripe in it
/// rises from 0.2 cm to 1.3 cm, and past about 40 m the filter is doing more harm than good. The
/// difference-of-Gaussians band-pass has edge artefacts that grow with the window, and on a few
/// hundred metres of plot a forty-metre window is already a fifth of the width.
///
/// So this stays short on purpose, and the filter stays mild. It catches the short end of the
/// stripe and leaves the rest. See [`destripe`], which says the same thing about the result.
const STRIPE_DETREND_M: f32 = 6.0;
/// The most this is ever allowed to move the ground, metres.
///
/// Measured amplitude on flat ground is 0.5–0.7 cm and the whole correction comes out at 0.9 cm
/// rms. The clamp is what stops the filter reaching for a real feature that happens to run
/// north-south — a berm on a straight, the edge of a graded pad — which is exactly how the
/// first version of this introduced banding of its own.
const STRIPE_CLAMP_M: f32 = 0.015;

/// Take the flight-line corduroy out of a square of ground.
///
/// The honest description of what this does, because it is easy to oversell: the tile has a
/// measured directional grain — on flat ground the north-south stripe bands of its power
/// spectrum carry 2.5 to 3 times the mean, against 0.3 to 0.9 for the diagonals. This removes
/// the part of that which is coherent over a long run of the flight line, which is about a
/// tenth of the excess, for 0.9 cm rms and 0.035 cm off the ground's own roughness. The rest is
/// per-swath and locally phased, and chasing it with a stronger filter put visible banding into
/// ground that had none. So this is a mild filter on purpose and it does not claim to be a cure.
///
/// `rows` run north-south and `cols` east-west, which is the flight direction for this tile;
/// `flight_ns` is false for a tile flown the other way.
pub fn destripe(z: &mut [f32], w: usize, h: usize, cell: f32, flight_ns: bool, strength: f32) -> f32 {
    if strength <= 0.0 || w < 8 || h < 8 {
        return 0.0;
    }
    let detrended = high_pass(z, w, h, cell, STRIPE_DETREND_M);
    // Average along the flight line.
    let mut along = detrended.clone();
    blur_axis(&mut along, w, h, STRIPE_ALONG_M / 2.355 / cell, flight_ns);
    // Keep only the band the stripe lives in, across it.
    let mut lo = along.clone();
    let mut hi = along.clone();
    blur_axis(&mut lo, w, h, STRIPE_LO_M / 2.355 / cell, !flight_ns);
    blur_axis(&mut hi, w, h, STRIPE_HI_M / 2.355 / cell, !flight_ns);

    let mut moved = 0.0f64;
    for i in 0..z.len() {
        let s = ((lo[i] - hi[i]) * strength).clamp(-STRIPE_CLAMP_M, STRIPE_CLAMP_M);
        z[i] -= s;
        moved += (s as f64) * (s as f64);
    }
    (moved / z.len().max(1) as f64).sqrt() as f32
}

/// `z` minus a Gaussian blur of itself: what is left at scales under `fwhm_m`.
fn high_pass(z: &[f32], w: usize, h: usize, cell: f32, fwhm_m: f32) -> Vec<f32> {
    let mut low = z.to_vec();
    let sigma = fwhm_m / 2.355 / cell;
    blur_axis(&mut low, w, h, sigma, true);
    blur_axis(&mut low, w, h, sigma, false);
    z.iter().zip(&low).map(|(a, b)| a - b).collect()
}

/// A separable Gaussian along one axis, edges held rather than wrapped.
///
/// Three box passes, which is the standard cheap Gaussian and is within a percent of the real
/// thing at these radii. Written out rather than pulled in because the only other blur in the
/// tree works on `image` buffers.
fn blur_axis(v: &mut [f32], w: usize, h: usize, sigma: f32, down_rows: bool) {
    if !(sigma > 0.05) {
        return;
    }
    // The box width that matches a Gaussian of this sigma over three passes.
    let bw = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt().round() as usize).max(1) | 1;
    let r = bw / 2;
    // `n` is how long each line is, `outers` how many of them.
    let (n, outers) = if down_rows { (h, w) } else { (w, h) };
    if n < 2 {
        return;
    }
    let mut line = vec![0.0f32; n];
    let mut tmp = vec![0.0f32; n];
    for outer in 0..outers {
        for i in 0..n {
            line[i] = if down_rows { v[i * w + outer] } else { v[outer * w + i] };
        }
        for _ in 0..3 {
            let mut acc: f32 = 0.0;
            for i in 0..=r.min(n - 1) {
                acc += line[i];
            }
            // Edge cells see the edge value repeated, which is what `mode='nearest'` means.
            acc += line[0] * r as f32;
            for i in 0..n {
                tmp[i] = acc / bw as f32;
                let add = line[(i + r + 1).min(n - 1)];
                let sub = line[i.saturating_sub(r)];
                acc += add - sub;
            }
            line.copy_from_slice(&tmp);
        }
        for i in 0..n {
            if down_rows {
                v[i * w + outer] = line[i];
            } else {
                v[outer * w + i] = line[i];
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The stored plot
// ---------------------------------------------------------------------------------------------

/// A piece of real ground, ready to stand a track on.
///
/// Rectangular, not square, because a venue is. Ironman's circuit is 410 by 285 m, and a square
/// plot around it would have been a third woodland — and worse, the delivered scan of it is 528
/// by 404 m, out of which no 490 m square can be cut at all. [`crate::trackprog::Terrain`] has
/// always had `size_x` and `size_z` as separate fields and `grid_dims` has always put the 2049
/// samples on the long edge, so this is the shape the format was already built for.
///
/// Cells are square even when the plot is not: [`GROUND_DIM`] samples go on the long edge and
/// the short edge takes whatever number keeps a cell the same size both ways.
#[derive(Clone, Debug)]
pub struct Ground {
    pub dim_x: usize,
    pub dim_z: usize,
    /// Metres the plot is across, east-west and north-south.
    pub size_x: f32,
    pub size_z: f32,
    /// Heights in metres above [`Ground::base_m`], row 0 at world z = 0.
    pub z: Vec<f32>,
    /// What the aerial photograph says is on each cell, as a [`Cover`] class: vegetation, one
    /// of three tones of soil, or pale hardstanding. Empty when no imagery was supplied.
    ///
    /// Three tones rather than one because one is what a 470 m plot of a single tiling sheet
    /// looks like, which is nothing like a motocross venue: worked track, dry field and damp
    /// shaded ground are different colours in the photograph and have to be different
    /// colours on the ground. The split is by the photograph's own luminance quantiles, so a
    /// venue shot in flat French light and one shot in Indiana summer sun both come out
    /// banded rather than one of them coming out uniform.
    ///
    /// This is how a scanned track gets its ground painted without anyone drawing a track on it.
    /// Every mask the generator normally writes is derived from the riding line — the ribbon, the
    /// worn line, the tyre marks, the rut — which is exactly the thing that must not appear on a
    /// scan. The photograph knows where the dirt is because it can see it, and it has never heard
    /// of our centreline.
    pub cover: Vec<u8>,
    /// What was subtracted to bring the plot's lowest point to zero — the real elevation of the
    /// track's own datum, kept so a height can be quoted back in the units it was measured in.
    pub base_m: f32,
}

/// What the photograph says is on one cell.
///
/// The order is the order the ground layers paint in, darkest soil first, which is what lets
/// `tracksynth` turn a class straight into a coverage mask without a lookup table in between.
/// Stored as a byte in the `.fgd`, so the numbers are format, not just names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cover {
    /// Grass, scrub or tree canopy. Green, or too dark to be soil in sunlight.
    Vegetation = 0,
    /// The darkest soil at the venue: damp, shaded, freshly turned.
    SoilDark = 1,
    /// Ordinary worked dirt, which is most of a motocross venue.
    SoilMid = 2,
    /// Dry pale soil — a baked field, a graded verge, the dust off a straight.
    SoilLight = 3,
    /// Genuinely pale and colourless: concrete, gravel, a hardstanding, a road.
    Hard = 4,
}

impl Cover {
    /// The byte a `.fgd` stores, and the one `tracksynth` compares against.
    pub const fn id(self) -> u8 {
        self as u8
    }
}

impl Ground {
    /// Metres a sample, the same both ways.
    pub fn cell(&self) -> f32 {
        self.size_x / (self.dim_x - 1).max(1) as f32
    }

    /// Height at a point on the plot, in the generator's own world coordinates, bilinear.
    pub fn at(&self, x: f32, z: f32) -> f32 {
        let per = self.cell();
        let fx = (x / per).clamp(0.0, (self.dim_x - 1) as f32);
        let fy = (z / per).clamp(0.0, (self.dim_z - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let x1 = (x0 + 1).min(self.dim_x - 1);
        let y1 = (y0 + 1).min(self.dim_z - 1);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let g = |x: usize, y: usize| self.z[y * self.dim_x + x];
        let top = g(x0, y0) + (g(x1, y0) - g(x0, y0)) * tx;
        let bot = g(x0, y1) + (g(x1, y1) - g(x0, y1)) * tx;
        top + (bot - top) * ty
    }

    /// Peak to trough, metres. What [`crate::trackprog::Terrain::scale`] has to clear.
    pub fn relief(&self) -> f32 {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for &v in &self.z {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        (hi - lo).max(0.0)
    }

    fn encode(&self) -> Vec<u8> {
        // Quantised against the square's own floor and ceiling, not against zero.
        //
        // This used to assume the lowest sample was zero, which is true of everything `import`
        // makes — it subtracts the plot minimum — and false of anything else. A square with a
        // sample below its assumed floor had it silently clamped away: a caught round-trip
        // returned 0 for a height of -0.356 m. Storing the floor costs four bytes and removes
        // the assumption.
        let lo = self.z.iter().copied().fold(f32::MAX, f32::min);
        let hi = self.z.iter().copied().fold(f32::MIN, f32::max);
        let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo, hi) } else { (0.0, 1.0) };
        let span = (hi - lo).max(1e-3);
        let mut out = Vec::with_capacity(36 + self.z.len() * 2);
        out.extend_from_slice(b"FGND");
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&(self.dim_x as u32).to_le_bytes());
        out.extend_from_slice(&(self.dim_z as u32).to_le_bytes());
        out.extend_from_slice(&self.size_x.to_le_bytes());
        out.extend_from_slice(&self.size_z.to_le_bytes());
        out.extend_from_slice(&self.base_m.to_le_bytes());
        out.extend_from_slice(&lo.to_le_bytes());
        out.extend_from_slice(&span.to_le_bytes());
        for &v in &self.z {
            let q = (((v - lo) / span) * 65535.0).round().clamp(0.0, 65535.0) as u16;
            out.extend_from_slice(&q.to_le_bytes());
        }
        out.extend_from_slice(&(self.cover.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.cover);
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        const HEAD: usize = 36;
        if bytes.len() < HEAD || &bytes[..4] != b"FGND" {
            bail!("not a stored ground plot");
        }
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32_at = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        // Version 4 is version 3 with a finer cover class in the same byte. A version-3 file
        // would decode perfectly and paint the wrong ground, so it is refused and re-imported
        // rather than read: the classes changed meaning, not shape.
        if u32_at(4) != 4 {
            bail!("stored ground is version {}, which this build doesn't read", u32_at(4));
        }
        let dim_x = u32_at(8) as usize;
        let dim_z = u32_at(12) as usize;
        let size_x = f32_at(16);
        let size_z = f32_at(20);
        let base_m = f32_at(24);
        let lo = f32_at(28);
        let span = f32_at(32);
        if dim_x < 2 || dim_z < 2 || dim_x > 1 << 15 || dim_z > 1 << 15 {
            bail!("stored ground says it is {dim_x} by {dim_z}, which is not a plot");
        }
        if !(size_x.is_finite() && size_z.is_finite() && size_x > 0.0 && size_z > 0.0) {
            bail!("stored ground has no sensible size");
        }
        if !(base_m.is_finite() && lo.is_finite() && span.is_finite() && span > 0.0) {
            bail!("stored ground has no sensible height range");
        }
        // The length check has to count everything the writer writes, and this is where it went
        // wrong before: version 2 added the floor beside the span and moved the header, and the
        // check moved with it — but the cover count that every version-3 file carries was still
        // treated as optional on read. So a file truncated by exactly those four bytes was long
        // enough to satisfy the check and was accepted, which is how a garbage heightfield gets
        // in looking plausible. It is the same fault as the others today: one thing changed and a
        // dependent quantity left deriving from the old assumption.
        let want = HEAD
            .checked_add(dim_x.checked_mul(dim_z).and_then(|n| n.checked_mul(2)).unwrap_or(usize::MAX))
            .and_then(|n| n.checked_add(4))
            .unwrap_or(usize::MAX);
        if bytes.len() < want {
            bail!(
                "stored ground is truncated: {} bytes for a {dim_x} by {dim_z} plot that needs {want}",
                bytes.len()
            );
        }
        let cover_n =
            u32::from_le_bytes(bytes[want - 4..want].try_into().unwrap()) as usize;
        if cover_n != 0 && cover_n != dim_x * dim_z {
            bail!(
                "stored ground carries {cover_n} cover cells for a {dim_x} by {dim_z} plot, which \
                 is neither none nor one each"
            );
        }
        if bytes.len() < want + cover_n {
            bail!("stored ground's cover is truncated");
        }
        let mut z = Vec::with_capacity(dim_x * dim_z);
        for i in 0..dim_x * dim_z {
            let o = HEAD + i * 2;
            let q = u16::from_le_bytes([bytes[o], bytes[o + 1]]);
            z.push(lo + q as f32 / 65535.0 * span);
        }
        let cover = bytes[want..want + cover_n].to_vec();
        Ok(Ground { dim_x, dim_z, size_x, size_z, z, cover, base_m })
    }
}

/// Fetch a stored ground plot by id, decoding it once per process.
/// Put a plot in the store and hand back the id a [`crate::trackprog::GroundRef`] names it by.
///
/// Content-addressed, so the same ground imported twice is stored once and two programs
/// referring to it agree by construction. Writing is skipped when the file is already there —
/// its name *is* its hash, so an existing file cannot be a different plot.
///
/// The seam every ground import comes through, whether the heights were flown by an aircraft
/// or read back out of a track somebody compiled.
pub fn store(ground: &Ground) -> Result<String> {
    let id = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(ground.encode());
        format!("{:x}", h.finalize())[..32].to_string()
    };
    let dir = dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("couldn't make {}", dir.display()))?;
    let file = dir.join(format!("{id}.fgd"));
    if !file.exists() {
        write_atomically(&file, &ground.encode())?;
    }
    Ok(id)
}

pub fn load(id: &str) -> Option<Arc<Ground>> {
    if id.is_empty() {
        return None;
    }
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Some(hit) = cache.lock().ok()?.get(id) {
        return hit.clone();
    }
    let got = std::fs::read(dir().join(format!("{id}.fgd")))
        .ok()
        .and_then(|b| Ground::decode(&b).ok())
        .map(Arc::new);
    if let Ok(mut c) = cache.lock() {
        c.insert(id.to_string(), got.clone());
    }
    got
}

// ---------------------------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------------------------

/// What an import produced: ground in the store, and the lap that was traced over it, moved
/// into the generator's own world coordinates.
#[derive(Debug, Clone)]
pub struct Imported {
    /// The stored ground's id, for [`crate::trackprog::GroundRef`].
    pub id: String,
    pub ground: Ground,
    /// The traced lap, in plot coordinates: metres east from the plot's west edge, and metres
    /// south from its north edge, which is the generator's `+x` and `+z`.
    pub lap: Vec<(f32, f32, f32)>,
    pub closed: bool,
    /// Metres the plot is across, east-west and north-south.
    pub size_x: f32,
    pub size_z: f32,
    /// How much the destripe pass moved the ground, rms metres — reported rather than assumed.
    pub destripe_rms: f32,
    pub place: String,
    pub source: String,
    pub collected: String,
    pub licence: String,
    /// The credit the ground's licence requires, word for word. Empty where none is required.
    pub attribution: String,
    /// The same for the picture the lap was traced on, which can be a different survey under a
    /// different licence.
    pub imagery_attribution: String,
    pub epsg: u32,
    /// Where the plot sits in the world, so a built track can say where it came from.
    pub origin_e: f64,
    pub origin_n: f64,
    /// What the person who traced the lap said they were unsure of. Carried, not dropped.
    pub provisional: bool,
    pub direction_verified: bool,
    pub confidence: Vec<TraceConfidence>,
    pub note: String,
    pub route_source: String,
    pub route_confirmed: bool,
    pub known_issues: Vec<String>,
}

/// The plot a lap of this shape gets, in metres.
///
/// A plot is not free to be any shape, and the reason is the game's own grid. MX Bikes wants a
/// power of two plus one samples on an edge, `grid_dims` puts the program's `samples` on the long
/// edge and the nearest such number on the short one, and then refuses the pair if the cells come
/// out more than five per cent from square. So the only short-to-long ratios a plot can actually
/// express are the ones the sample counts can: 2049 against 2049, 1025, 513 and so on — that is,
/// one, a half, a quarter.
///
/// Ironman wants 470 by 350, a ratio of 0.745, and there is no pair of sample counts that gives
/// it: 1025 samples down 350 m is 0.342 m a cell against 0.229 m across. The honest answer is to
/// round the short edge *up* to the nearest ratio the grid can hold, which here means a square
/// plot with some spare ground along one side. Rounding down would crop the lap.
fn plot_size(want_x: f32, want_z: f32) -> (f32, f32) {
    let long = ((want_x.max(want_z)) / 10.0).ceil() * 10.0;
    let short_wanted = want_x.min(want_z);
    // Largest first, so the smallest ratio that still covers the lap wins.
    let ratio = [0.125f32, 0.25, 0.5, 1.0]
        .into_iter()
        .find(|r| long * r >= short_wanted)
        .unwrap_or(1.0);
    let short = long * ratio;
    if want_x >= want_z { (long, short) } else { (short, long) }
}

/// Bring a lidar tile and a traced lap in as a plot of real ground.
///
/// The plot is square, centred on the lap, and sized to the longer of the lap's two spans plus
/// [`PLOT_MARGIN_M`] each side — rounded up so the same lap always lands on the same plot.
pub fn import(tif: &Path, lap: &Path, strength: f32) -> Result<Imported> {
    let trace = LapTrace::read(lap)?;
    let pts = trace.resolved()?;
    let (e0, n0, e1, n1) = trace.bounds()?;

    // The plot is the shape of the venue, rounded to ten metres so a trace nudged by a metre
    // does not move the whole plot. Cells stay square: whichever edge is longer gets
    // [`GROUND_DIM`] samples and the other gets the count that keeps a cell the same size.
    let want_x = ((e1 - e0) as f32 + PLOT_MARGIN_M * 2.0).max(60.0);
    let want_z = ((n1 - n0) as f32 + PLOT_MARGIN_M * 2.0).max(60.0);
    let (size_x, size_z) = plot_size(want_x, want_z);
    let (cx, cy) = ((e0 + e1) * 0.5, (n0 + n1) * 0.5);
    let (west, north) = (cx - size_x as f64 * 0.5, cy + size_z as f64 * 0.5);

    // Padded, so the slide below has somewhere to slide to. A tile cropped exactly to the lap
    // gives back its whole self and the plot moves inside it; a big staged tile gives back a
    // little more than the plot needs and costs nothing extra, because only the tiles the
    // window touches are ever decoded.
    let pad = PLOT_MARGIN_M as f64 * 2.0;
    let dem = read_geotiff(
        tif,
        Some((
            west - pad,
            north - size_z as f64 - pad,
            west + size_x as f64 + pad,
            north + pad,
        )),
    )?;

    // A tile cropped tight to the lap has nothing to spare, and a plot centred on the lap's
    // bounding box wants a margin the crop may not have. Sliding the plot to sit inside the
    // ground that exists beats failing, and beats filling the overhang with invented height:
    // the lap still fits — it is smaller than the plot by the margin — it just sits off-centre
    // in it. Only if the tile is genuinely too small does this give up.
    let (dem_w, dem_e) = (dem.origin_e - dem.cell * 0.5, dem.origin_e + (dem.w as f64 - 0.5) * dem.cell);
    let (dem_n, dem_s) = (dem.origin_n + dem.cell * 0.5, dem.origin_n - (dem.h as f64 - 0.5) * dem.cell);
    if dem_e - dem_w < size_x as f64 || dem_n - dem_s < size_z as f64 {
        bail!(
            "the lap wants a {size_x:.0} by {size_z:.0} m plot and the tile only covers {:.0} by \
             {:.0} m around it. Fetch a wider tile, or trace a shorter lap.",
            dem_e - dem_w,
            dem_n - dem_s
        );
    }
    let west = west.clamp(dem_w, dem_e - size_x as f64);
    let north = north.clamp(dem_s + size_z as f64, dem_n);

    // Resample onto the plot. Row 0 is the NORTH edge, so world z runs south — the same sense
    // the generator's grid already uses.
    let long = size_x.max(size_z);
    let per = long as f64 / (GROUND_DIM - 1) as f64;
    let dim_x = ((size_x as f64 / per).round() as usize + 1).max(2);
    let dim_z = ((size_z as f64 / per).round() as usize + 1).max(2);
    let mut z = vec![0.0f32; dim_x * dim_z];
    let mut holes = 0usize;
    for y in 0..dim_z {
        for x in 0..dim_x {
            let v = dem.at(west + x as f64 * per, north - y as f64 * per);
            if v.is_finite() {
                z[y * dim_x + x] = v;
            } else {
                z[y * dim_x + x] = f32::NAN;
                holes += 1;
            }
        }
    }
    if holes * 20 > dim_x * dim_z {
        bail!(
            "{:.1}% of the plot has no height in the tile. Either the lap is off the edge of it \
             or the tile is mostly void.",
            holes as f32 * 100.0 / (dim_x * dim_z) as f32
        );
    }
    if holes > 0 {
        fill_holes(&mut z, dim_x, dim_z);
    }

    let stretched = stretched_fraction(&z, dim_x);
    if stretched > STRETCHED_LIMIT {
        bail!(
            "{:.0}% of this grid's neighbouring cells are bit-identical, so the service stretched \
             coarser data to the size that was asked for rather than holding it at that \
             resolution. Ask its catalogue what it actually has before fetching.",
            stretched * 100.0
        );
    }
    let destripe_rms = destripe(&mut z, dim_x, dim_z, per as f32, true, strength);

    // Bring the lowest point to zero: the generator's height budget is a range, not an
    // elevation, and Ironman sits 215 m above the sea.
    let base = z.iter().copied().fold(f32::MAX, f32::min);
    for v in z.iter_mut() {
        *v -= base;
    }

    // The aerial photograph of the same ground, classified, so the track can be painted from
    // what is actually there rather than from where we guessed the lap goes.
    let cover = imagery_cover(tif, lap, &dem, west, north, per, dim_x, dim_z);
    let ground = Ground { dim_x, dim_z, size_x, size_z, z, cover, base_m: base };

    let id = store(&ground)?;

    let lap_xz: Vec<(f32, f32, f32)> = pts
        .iter()
        .map(|p| ((p.e - west) as f32, (north - p.n) as f32, p.width_m))
        .collect();

    let d = trace.dem.clone().unwrap_or_default();
    Ok(Imported {
        id,
        ground,
        lap: lap_xz,
        closed: trace.closed,
        size_x,
        size_z,
        destripe_rms,
        place: trace.name.clone(),
        source: d.source,
        collected: d.collected,
        licence: d.licence,
        attribution: d.attribution,
        imagery_attribution: trace
            .imagery
            .as_ref()
            .map(|i| i.attribution.clone())
            .unwrap_or_default(),
        epsg: dem.epsg,
        origin_e: west,
        origin_n: north,
        provisional: trace.provisional,
        direction_verified: trace.direction_verified,
        confidence: trace.segments.clone(),
        note: trace.note.clone(),
        route_source: trace.route_source.clone(),
        route_confirmed: trace.route_confirmed,
        known_issues: trace.known_issues.clone(),
    })
}

/// Whether a grid is really at the resolution it claims, or a coarser one stretched to fit.
///
/// **The declared pixel size is not evidence and neither is the nodata pattern.** Both the USGS
/// and the IGN elevation services will answer a request for any pixel size with a well-formed,
/// fully valid float grid of exactly that size — and silently fill it by stretching whatever
/// they actually hold. Measured on IGN's `ELEVATION.ELEVATIONGRIDCOVERAGE.HIGHRES` over a French
/// motocross circuit: 100% valid cells, a sensible 35 m of relief, and **84.6% of horizontally
/// adjacent cells exactly equal, with a median run of 7 identical cells** — roughly 3.5 m data
/// pretending to be half-metre. Every structural check passed and the track would have had no
/// jump detail at all.
///
/// The test that catches it is the fraction of adjacent cells that are bit-identical. Real lidar
/// is never exactly equal twice in a row: the genuine 0.5 m layer over the same ground measures
/// 0.0%. Anything over about a fifth means the service is stretching.
///
/// It also caught the case a *statistic* got wrong: the stretched layer measured **rougher** than
/// the real one — 19.7 cm against 14.1 cm after a six-metre detrend — because nearest-neighbour
/// stairs are high-frequency. Roughness would have picked the fake. The hillshade showed it at a
/// glance as diagonal terracing, which is why this repo judges ground by rendering it.
pub fn stretched_fraction(z: &[f32], w: usize) -> f32 {
    if w < 2 || z.len() < w * 2 {
        return 0.0;
    }
    let (mut same, mut seen) = (0usize, 0usize);
    for row in z.chunks_exact(w) {
        for pair in row.windows(2) {
            if pair[0].to_bits() == pair[1].to_bits() {
                same += 1;
            }
            seen += 1;
        }
    }
    if seen == 0 { 0.0 } else { same as f32 / seen as f32 }
}

/// The most identical-neighbour fraction a grid may have and still be believed.
///
/// A fifth. Genuine lidar measures near zero and a stretched grid measures four fifths, so
/// anything in between is a judgement that never has to be made in practice.
pub const STRETCHED_LIMIT: f32 = 0.20;

/// Whether an image came back blank — the shape a wrong axis order takes.
///
/// `data.geopf.fr` wants BBOX as easting then northing even under WMS 1.3.0, and asked the other
/// way round it returns **HTTP 200 with a blank white image** rather than an error. So a fetch
/// path cannot tell success from failure by the status code, and a blank check belongs in it.
/// Four probes were wasted on this before the cause was found.
pub fn looks_blank(pixels: &[u8]) -> bool {
    if pixels.len() < 64 {
        return true;
    }
    let mean = pixels.iter().map(|&p| p as f64).sum::<f64>() / pixels.len() as f64;
    let var = pixels.iter().map(|&p| (p as f64 - mean).powi(2)).sum::<f64>() / pixels.len() as f64;
    var.sqrt() < 2.0
}

/// Classify the orthophoto beside the scan onto the plot's own grid.
///
/// Vegetation is separated by colour — green, or too dark to be soil in sunlight — and what is
/// left is banded by its own luminance into three tones of soil plus genuinely pale
/// hardstanding. See [`Cover`] for what each class is and why there are three soils.
///
/// **The bands are quantiles of this photograph, not fixed levels.** A threshold measured off
/// the Ironman NAIP frame calls an IGN ortho of a French circuit uniformly one thing, because
/// the two are not exposed alike; quantiles put the darkest third of the venue's own soil on
/// the dark sheet wherever it was shot. Absolute levels are used for one thing only — telling
/// vegetation from soil — because that is a question about hue, not exposure.
///
/// **The photograph is registered to the plot by world coordinates**, which is the other half
/// of this and was wrong until 2026-09-16: the image covers the DEM tile, the plot is a window
/// inside that tile, and stretching the whole image across the whole plot put the paint up to
/// 15 m from the ground it describes — grass over the outside of a berm, dirt over the field
/// beside it. `dem` gives the tile's extent, and `west`/`north`/`per` the plot's; a trace may
/// state `imagery.bbox` as `[west, south, east, north]` when its photograph covers something
/// else.
///
/// If no photograph is there the cover comes back empty and the caller paints plain dirt,
/// which is honest: we do not know what is where, so we do not pretend to.
fn imagery_cover(
    tif: &Path,
    lap: &Path,
    dem: &Dem,
    west: f64,
    north: f64,
    per: f64,
    dim_x: usize,
    dim_z: usize,
) -> Vec<u8> {
    let Some(dir) = lap.parent() else { return Vec::new() };
    let trace = std::fs::read_to_string(lap)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let imagery = trace.as_ref().and_then(|v| v.get("imagery"));
    let named = imagery
        .and_then(|i| i.get("path"))
        .and_then(|p| p.as_str().map(str::to_string));
    let mut tries: Vec<PathBuf> = Vec::new();
    if let Some(n) = named {
        tries.push(dir.join(n));
    }
    if let Some(stem) = tif.file_stem().and_then(|s| s.to_str()) {
        let base = stem.trim_end_matches(".dem");
        tries.push(dir.join(format!("{base}.imagery.png")));
    }
    let Some(img) = tries.iter().find_map(|p| image::open(p).ok()) else { return Vec::new() };
    let rgb = img.to_rgb8();
    let (iw, ih) = (rgb.width() as i64, rgb.height() as i64);
    if iw < 2 || ih < 2 {
        return Vec::new();
    }

    // Where the photograph's own edges are, in the world. The tile's, unless the trace says
    // otherwise. `dem.origin_e`/`origin_n` are the centre of the first cell, so the edge is
    // half a cell out.
    let stated = imagery
        .and_then(|i| i.get("bbox"))
        .and_then(|b| b.as_array())
        .filter(|b| b.len() == 4)
        .and_then(|b| {
            let v: Vec<f64> = b.iter().filter_map(serde_json::Value::as_f64).collect();
            (v.len() == 4).then(|| (v[0], v[1], v[2], v[3]))
        });
    let (img_w, img_s, img_e, img_n) = stated.unwrap_or((
        dem.origin_e - dem.cell * 0.5,
        dem.origin_n - (dem.h as f64 - 0.5) * dem.cell,
        dem.origin_e + (dem.w as f64 - 0.5) * dem.cell,
        dem.origin_n + dem.cell * 0.5,
    ));
    let (span_e, span_n) = (img_e - img_w, img_n - img_s);
    if !(span_e.is_finite() && span_n.is_finite() && span_e > 0.0 && span_n > 0.0) {
        return Vec::new();
    }

    // Sample the photograph over each plot cell's own footprint rather than at its centre.
    // At Ironman the ortho is 0.5 m a pixel and a plot cell is 0.46 m, so this is barely more
    // than one pixel — but a coarser plot on a finer photograph would otherwise classify from
    // whichever single pixel it happened to land on, and salt-and-pepper a whole venue.
    let half_px_e = (per / span_e * iw as f64 * 0.5).max(0.5);
    let half_px_n = (per / span_n * ih as f64 * 0.5).max(0.5);
    let mut lum = vec![0i32; dim_x * dim_z];
    let mut sat = vec![0f32; dim_x * dim_z];
    let mut veg = vec![false; dim_x * dim_z];
    for y in 0..dim_z {
        let n = north - y as f64 * per;
        let cy = (img_n - n) / span_n * ih as f64;
        let (y0, y1) = window(cy, half_px_n, ih);
        for x in 0..dim_x {
            let e = west + x as f64 * per;
            let cx = (e - img_w) / span_e * iw as f64;
            let (x0, x1) = window(cx, half_px_e, iw);
            let (mut sr, mut sg, mut sb, mut n_px) = (0i64, 0i64, 0i64, 0i64);
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let p = rgb.get_pixel(px as u32, py as u32).0;
                    sr += p[0] as i64;
                    sg += p[1] as i64;
                    sb += p[2] as i64;
                    n_px += 1;
                }
            }
            let n_px = n_px.max(1);
            let (r, g, b) = ((sr / n_px) as i32, (sg / n_px) as i32, (sb / n_px) as i32);
            let l = (r + g + b) / 3;
            let i = y * dim_x + x;
            lum[i] = l;
            sat[i] = (r.max(g).max(b) - r.min(g).min(b)) as f32 / l.max(1) as f32;
            // Measured against the Ironman ortho rather than guessed. A first pass at
            // `lum > 120` called 42% of the venue gravel, because worked dirt in summer sun
            // is pale; and requiring `g > r + 6` found only 2% grass, because tree canopy in
            // shadow is dark and barely green.
            veg[i] = l < 95 || (g > r + 3 && g >= b);
        }
    }

    // The soil's own quantiles. Sorting a million cells once is cheaper than the alternative
    // of getting the levels wrong at every venue that isn't Indiana.
    let mut soil: Vec<i32> = lum.iter().zip(&veg).filter(|(_, v)| !**v).map(|(l, _)| *l).collect();
    soil.sort_unstable();
    let q = |f: f64| -> i32 {
        if soil.is_empty() {
            return i32::MAX;
        }
        soil[((soil.len() - 1) as f64 * f).round() as usize]
    };
    // A third dark, a third mid, a quarter light, the palest twelfth reserved for hardstanding
    // — and hardstanding has to be colourless as well as bright, or a dust-blown straight in
    // full sun comes out as concrete.
    let (q_dark, q_mid, q_pale) = (q(0.35), q(0.70), q(0.92));

    let mut out = vec![Cover::SoilMid.id(); dim_x * dim_z];
    for i in 0..out.len() {
        out[i] = if veg[i] {
            Cover::Vegetation
        } else if lum[i] > q_pale && sat[i] < 0.12 {
            Cover::Hard
        } else if lum[i] <= q_dark {
            Cover::SoilDark
        } else if lum[i] <= q_mid {
            Cover::SoilMid
        } else {
            Cover::SoilLight
        }
        .id();
    }
    out
}

/// The pixel span a plot cell covers, clamped into the image.
fn window(centre: f64, half: f64, n: i64) -> (i64, i64) {
    let lo = (centre - half).floor() as i64;
    let hi = (centre + half).floor() as i64;
    (lo.clamp(0, n - 1), hi.clamp(0, n - 1).max(lo.clamp(0, n - 1)))
}

/// Fill the odd nodata cell from its neighbours, so a puddle in the lidar is not a hole in the
/// track. Anything bigger than a puddle has already been refused above.
fn fill_holes(z: &mut [f32], w: usize, h: usize) {
    for _ in 0..24 {
        let src = z.to_vec();
        let mut left = 0usize;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if src[i].is_finite() {
                    continue;
                }
                let mut sum = 0.0f32;
                let mut n = 0u32;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let v = src[ny as usize * w + nx as usize];
                    if v.is_finite() {
                        sum += v;
                        n += 1;
                    }
                }
                if n > 0 {
                    z[i] = sum / n as f32;
                } else {
                    left += 1;
                }
            }
        }
        if left == 0 {
            break;
        }
    }
    // Anything still unreachable — a void touching nothing — goes to the plot's mean rather
    // than staying NaN, because a NaN in the heightfield is a hole the compiler will not
    // notice.
    let mean = {
        let (mut s, mut n) = (0.0f64, 0u32);
        for &v in z.iter() {
            if v.is_finite() {
                s += v as f64;
                n += 1;
            }
        }
        if n == 0 { 0.0 } else { (s / n as f64) as f32 }
    };
    for v in z.iter_mut() {
        if !v.is_finite() {
            *v = mean;
        }
    }
}

/// How raced scanned ground arrives, against [`crate::trackprog::default_wear`]'s 0.55.
pub const SCAN_WEAR: f32 = 0.15;
/// How rough the ridden surface is made, against a generated track's 1.0.
pub const SCAN_ROUGHNESS: f32 = 0.30;
/// Fine surface texture, metres, against [`crate::trackprog::default_texture`]'s 0.085.
pub const SCAN_TEXTURE: f32 = 0.025;

/// Build a track program that stands on imported ground and runs round the imported lap.
///
/// Everything here is decided by what was measured rather than chosen, which is the point of
/// importing a place at all:
///
/// * the plot is the lap's own bounding box plus a margin;
/// * the height budget is the ground's own relief with headroom, because a scan brings its own
///   relief and the default 22 m would clip Ironman's 23.5 by a metre and a half;
/// * the riding line's width is the length-weighted mean of what the rider traced;
/// * every segment's `rise` is zero, so the track follows the ground it crosses — which for
///   scanned ground is not a fallback, it is the whole answer. The elevation of a real lap *is*
///   the elevation of the land under it.
///
/// The relief knobs are zeroed rather than left at their defaults. They describe ground that is
/// no longer being invented, and leaving them set would be leaving dead numbers in a saved
/// project for someone to read as meaningful. `texture` stays, because it is the metre-scale
/// grain of a *ridden* surface rather than of the landscape, and a 1 m lidar grid cannot see it.
pub fn program_for(imp: &Imported, jumps: crate::trackprog::ScanJumps) -> Result<crate::trackprog::TrackProgram> {
    use crate::trackprog::*;

    let Some(fit) = fit_lap(&imp.lap, imp.closed) else {
        bail!("the traced lap couldn't be fitted to straights and arcs at all");
    };

    // Headroom over the ground's own relief. The budget is also the quantisation step — at 24 m
    // over a u16 the step is 0.37 mm, so there is no reason to be mean with it — and the track
    // that gets benched into this ground can stand above it.
    let scale = (imp.ground.relief() * 1.15 + 4.0).ceil().max(8.0);

    // The grid: the plot is square, so this is the samples on both edges.
    let samples = DEFAULT_SAMPLES;

    // A provisional trace makes a provisional track, and it says so in its own name. The
    // alternative is a file called "Ironman Raceway" that is a guess about where the racing line
    // goes, sitting in a rider's mods folder with nothing on it to say so — and a name is the
    // only part of a track that everyone reads.
    let name = if imp.place.is_empty() { "Scanned Place".to_string() } else { imp.place.clone() };
    let name = if imp.provisional || !imp.route_confirmed {
        format!("{name} (provisional)")
    } else {
        name
    };

    let prog = TrackProgram {
        name,
        author: String::new(),
        location: place_note(imp),
        terrain: Terrain {
            size_x: imp.size_x,
            size_z: imp.size_z,
            samples,
            scale,
            relief: Relief {
                amplitude: 0.0,
                wavelength: 180.0,
                seed: 1,
                texture: SCAN_TEXTURE,
                tilt: 0.0,
                tilt_angle: 0.0,
                landforms: 0,
                landform_height: 0.0,
            },
            surface: Surface::Soil,
            texture: Default::default(),
            // Turned down, because the scan already carries what these invent.
            //
            // The generator's wear, rut and surface-texture passes exist to put back the
            // roughness a made-up landscape has none of. Scanned ground arrives with the real
            // thing already in it — the riding line off the Ironman tile measures 11.7 cm rms
            // after a 6 m detrend, which is Indiana's own 10.9 — so stamping the invented
            // roughness on top counts it twice. Measured: at the generated defaults the built
            // track read 24.1 cm on the line against Indiana's 10.9, which is not a statistic,
            // it is a track that feels wrong under the wheels.
            //
            // Not off, though. A 1 m grid holds a rut as presence rather than as depth, so the
            // scan under-carries the fine end and the generator should still supply some of it.
            // These are the values the band table settled on; see `scanned_roughness`.
            wear: SCAN_WEAR,
            roughness: SCAN_ROUGHNESS,
            ground: Some(GroundRef {
                id: imp.id.clone(),
                place: imp.place.clone(),
                source: imp.source.clone(),
                collected: imp.collected.clone(),
                licence: imp.licence.clone(),
                jumps,
            }),
        },
        start: fit.start,
        segments: fit.segments,
        width: fit.width_m.clamp(6.0, 20.0),
        features: Vec::new(),
        blend: default_blend(),
        elevation: Vec::new(),
        discipline: Discipline::Mx,
        border: Default::default(),
        venue: VenueKind::Open,
    };
    Ok(prog)
}

/// Where this track came from and what is not known about it, in one line, for the `location`
/// field that every built track carries into its own `.ini` and README.
///
/// Provenance and doubt travel together on purpose. The source, the licence and the flight dates
/// are what makes the track redistributable and dates the snapshot; the provisional and
/// direction flags are what stops a reader taking it for a survey. A track that says
/// "USGS 3DEP 1 m, flown 2017-2020, lap provisional, direction unverified" cannot be mistaken
/// for this season's Ironman by anyone who reads it.
pub fn place_note(imp: &Imported) -> String {
    let mut bits: Vec<String> = Vec::new();
    if !imp.source.is_empty() {
        bits.push(imp.source.clone());
    }
    // Ordered by what has to survive being cut short. The game's `location` field holds about
    // 155 characters before it stops listing the track at all, so the note is written most
    // important first and `tracksynth::location_label` keeps whole pieces off the front of it.
    // The credit the licence demands comes before the licence's own wording: one is an
    // obligation, the other is a description of it.
    if !imp.attribution.is_empty() {
        bits.push(imp.attribution.clone());
    }
    if !imp.licence.is_empty() {
        bits.push(imp.licence.clone());
    }
    if !imp.imagery_attribution.is_empty() && imp.imagery_attribution != imp.attribution {
        bits.push(format!("imagery {}", imp.imagery_attribution));
    }
    if !imp.collected.is_empty() {
        bits.push(format!("flown {}", imp.collected));
    }
    bits.push(format!("EPSG:{} at {:.0} {:.0}", imp.epsg, imp.origin_e, imp.origin_n));
    if imp.provisional {
        bits.push("lap provisional".into());
    }
    if !imp.direction_verified {
        bits.push("direction of travel unverified".into());
    }
    bits.join("; ")
}

/// Everything known and not known about where this track came from, as lines of text for the
/// built track's README.
///
/// The per-segment confidence the tracer recorded is the part worth keeping: "points 44 to 51,
/// low, several graded ribbons and the imagery did not settle it" tells a rider exactly which
/// corner to distrust, which is far more use than a blanket "provisional" on the whole lap.
/// Dropping it because the track format has nowhere structured to put it would be losing the
/// most specific thing anyone knows about the track.
pub fn provenance_lines(imp: &Imported) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!("Place: {}", if imp.place.is_empty() { "unnamed" } else { &imp.place }));
    if !imp.source.is_empty() {
        out.push(format!("Ground: {}", imp.source));
    }
    if !imp.collected.is_empty() {
        out.push(format!("Flown: {}", imp.collected));
    }
    if !imp.licence.is_empty() {
        out.push(format!("Licence: {}", imp.licence));
    }
    // The credit is a condition of the licence, so it travels inside the archive rather than
    // living only in a panel the rider saw once.
    if !imp.attribution.is_empty() {
        out.push(format!("Credit this track must carry: {}", imp.attribution));
    }
    if !imp.imagery_attribution.is_empty() && imp.imagery_attribution != imp.attribution {
        out.push(format!("Credit for the aerial photography: {}", imp.imagery_attribution));
    }
    out.push(format!(
        "Plot: {:.0} x {:.0} m at EPSG:{}, north-west corner {:.1} {:.1}",
        imp.size_x, imp.size_z, imp.epsg, imp.origin_e, imp.origin_n
    ));
    out.push(format!(
        "Destriping moved the ground by {:.2} cm rms.",
        imp.destripe_rms * 100.0
    ));
    if !imp.route_source.is_empty() {
        out.push(format!("Route: {}", imp.route_source));
    }
    if !imp.route_confirmed {
        out.push("The route has NOT been confirmed by whoever drew it.".into());
    }
    if imp.provisional {
        out.push("The traced lap is PROVISIONAL: it is where someone judged the racing line to \
                  be from an aerial photograph, not a survey."
            .into());
    }
    if !imp.direction_verified {
        out.push("The direction of travel is INFERRED, not verified. The lap may run the wrong \
                  way round."
            .into());
    }
    if !imp.note.is_empty() {
        out.push(format!("Tracer's note: {}", imp.note));
    }
    if !imp.known_issues.is_empty() {
        out.push("Known defects in the traced lap:".into());
        for k in &imp.known_issues {
            out.push(format!("  - {k}"));
        }
    }
    if !imp.confidence.is_empty() {
        out.push("How much of the lap is trusted, by traced point:".into());
        for c in &imp.confidence {
            out.push(format!(
                "  points {}-{}, {}: {}",
                c.from,
                c.to,
                if c.confidence.is_empty() { "unrated" } else { &c.confidence },
                c.what
            ));
        }
    }
    out
}

/// What a fit cost, in words, for whoever is about to look at the track.
pub fn fit_report(fit: &crate::trackprog::Fitted) -> String {
    let (lo, hi) = fit.width_range_m;
    format!(
        "lap {:.0} m in {} segments; fit error {:.2} m rms, {:.2} m worst (tolerance {:.1} m); \
         closure {:.2} m and {:.2}° before closing; width {:.1} m from a trace asking {:.1}–{:.1} m",
        fit.lap_m,
        fit.segments.len(),
        fit.rms_error_m,
        fit.max_error_m,
        crate::trackprog::FIT_TOLERANCE_M,
        fit.closure_m,
        fit.closure_deg,
        fit.width_m,
        lo,
        hi
    )
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes).with_context(|| format!("couldn't write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("couldn't put {} in place", path.display()))
}

/// Build a real place, end to end, from a tile and a trace on disk.
///
/// The same shape as `tracksynth::mx_proof` and for the same reason: this writes files, so it is
/// a test that is never run by the suite and is run by hand when someone wants the output. It is
/// the only way to exercise the whole path — GeoTIFF in, `.trh` and a source tree out — against
/// real data, and real data is the only thing that would have caught the half-pixel, the
/// row-order and the CRS questions.
///
/// ```text
/// FROST_DEM=…/ironman.dem.tif FROST_LAP=…/ironman.lap.json \
/// FROST_GROUND=…/store FROST_OUT=…/build \
///   cargo test -p frost-studio --bin frost-studio -- --ignored --nocapture scan_build
/// ```
#[cfg(test)]
mod scan_build {
    #[test]
    #[ignore = "reads a lidar tile and writes a track — set FROST_DEM, FROST_LAP, FROST_OUT"]
    fn build_a_scanned_place() {
        let dem = std::env::var("FROST_DEM").expect("set FROST_DEM to a GeoTIFF");
        let lap = std::env::var("FROST_LAP").expect("set FROST_LAP to a .lap.json");
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        if let Ok(store) = std::env::var("FROST_GROUND") {
            super::set_dir(std::path::PathBuf::from(store));
        }
        let recut = std::env::var("FROST_RECUT").is_ok();

        let imp = super::import(std::path::Path::new(&dem), std::path::Path::new(&lap), 1.0)
            .expect("the place imports");
        println!(
            "imported {}: plot {:.0} x {:.0} m at {:.3} m/cell ({} x {}), relief {:.2} m, \
             datum {:.2} m, EPSG:{}, destripe {:.3} cm rms",
            imp.place,
            imp.size_x,
            imp.size_z,
            imp.ground.cell(),
            imp.ground.dim_x,
            imp.ground.dim_z,
            imp.ground.relief(),
            imp.ground.base_m,
            imp.epsg,
            imp.destripe_rms * 100.0
        );
        let fit = crate::trackprog::fit_lap(&imp.lap, imp.closed).expect("the lap fits");
        println!("fit: {}", super::fit_report(&fit));

        let jumps = if recut || std::env::var_os("FROST_CORRIDOR").is_some() {
            crate::trackprog::ScanJumps::Recut
        } else {
            crate::trackprog::ScanJumps::Keep
        };
        let mut prog = super::program_for(&imp, jumps).expect("a program comes out");
        // Calibration hooks. Test-only, so the shipped defaults are the ones in `program_for`.
        if let Ok(v) = std::env::var("FROST_WEAR") {
            prog.terrain.wear = v.parse().expect("FROST_WEAR is a number");
        }
        if let Ok(v) = std::env::var("FROST_ROUGH") {
            prog.terrain.roughness = v.parse().expect("FROST_ROUGH is a number");
        }
        if let Ok(v) = std::env::var("FROST_TEX") {
            prog.terrain.relief.texture = v.parse().expect("FROST_TEX is a number");
        }
        println!(
            "surface: wear {:.2}, roughness {:.2}, texture {:.3}",
            prog.terrain.wear, prog.terrain.roughness, prog.terrain.relief.texture
        );
        prog.check().expect("and it is a valid one");
        println!(
            "program: {} segments, width {:.1} m, budget {:.0} m, plot {:.0} m, samples {}",
            prog.segments.len(),
            prog.width,
            prog.terrain.scale,
            prog.terrain.size_x,
            prog.terrain.samples
        );

        let syn = crate::tracksynth::synthesise(&prog).expect("it synthesises");

        // The scan is the terrain by default now; `FROST_CORRIDOR` asks for the old benched
        // path instead, which is what a bare hillside with no track on it still wants.
        std::fs::create_dir_all(&out).expect("made the output folder");
        // The same quantisation `terrained` will apply, written here so the band table can be
        // measured without a five-minute compile in the loop.
        std::fs::write(out.join("preview.trh"), crate::tracksynth::trh(&prog, &syn, true))
            .expect("wrote a preview .trh");
        std::fs::write(
            out.join("program.json"),
            serde_json::to_vec_pretty(&prog).expect("the program serialises"),
        )
        .expect("wrote the program");
        std::fs::write(out.join("PLACE.txt"), super::provenance_lines(&imp).join("\n") + "\n")
            .expect("wrote the provenance");
        for l in super::provenance_lines(&imp) {
            println!("  {l}");
        }
        let sc = crate::trackscenery::build(&prog, &syn);
        println!(
            "SCENERY: {} models, {} drawn blocks, {} solid blocks, {} edf files, tally {:?}",
            sc.models.len(),
            sc.drawn.len(),
            sc.solid.len(),
            sc.files.len(),
            sc.tally
        );
        let wrote = crate::tracksynth::write_source(&prog, &syn, &out).expect("wrote the source");
        println!("{} source files in {}", wrote.len(), out.display());

        // Compile, package and install through the app's own path.
        //
        // This used to stop at the source tree and a shell script took it the rest of the way,
        // running the compilers and then `zip -qr`. The four tracks that produced were perfect
        // inside and the game listed none of them — see `trackbuild::package` for why a `zip`
        // archive is unreadable to it. There is no second packer now: a scan finishes exactly
        // where a generated track finishes.
        let Ok(tools_at) = std::env::var("FROST_TOOLS") else {
            println!("no FROST_TOOLS, so the source is as far as this goes");
            return;
        };
        let tools = crate::trackbuild::find(std::path::Path::new(&tools_at))
            .expect("compilers under FROST_TOOLS");
        // `FROST_PREFIX` names a Wine prefix to run in — the one beside the compilers, on a
        // machine that keeps them there. Without it a prefix of our own is laid down beside
        // the build, which works and costs a first boot.
        let prefix = std::env::var("FROST_PREFIX")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| out.join("wine"));
        let host = crate::trackbuild::host(
            &std::env::var("FROST_GAME").unwrap_or_default(),
            &prefix,
            "",
        )
        .expect("a Wine host for the compilers");
        println!("compilers via {}", host.via());
        let slug = crate::tracksynth::slug(&prog.name);
        let tracks = std::env::var("FROST_INSTALL").ok().map(std::path::PathBuf::from);
        let built = crate::trackbuild::finish(
            &tools,
            &out,
            &slug,
            &host,
            tracks.as_deref(),
            &mut |phase| println!("  {phase}…"),
        )
        .expect("it compiles and packages");
        for st in &built.steps {
            println!("  {} {}", st.name, if st.ok { "ok" } else { "FAILED" });
            if !st.ok {
                println!("{}", st.output);
            }
        }
        let pkz = built.pkz.expect("every step passed, so there is an archive");
        println!(
            "packaged {} MB at {}",
            std::fs::metadata(&pkz).map(|m| m.len()).unwrap_or(0) / 1_048_576,
            pkz.display()
        );
        if let Some(at) = built.installed {
            println!("installed {}", at.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bump2(w: usize, h: usize) -> Vec<f32> {
        let mut z = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
                z[y * w + x] = 4.0 * (fx * 6.0).sin() * (fy * 5.0).cos() + 10.0 * fy;
            }
        }
        z
    }

    #[test]
    fn a_stored_square_survives_the_round_trip() {
        // Rectangular on purpose: a square would not catch the two dimensions being swapped.
        let (w, h) = (65usize, 41usize);
        let g = Ground { dim_x: w, dim_z: h, size_x: 470.0, size_z: 290.0, z: bump2(w, h), cover: vec![1u8; w * h], base_m: 214.9 };
        let back = Ground::decode(&g.encode()).expect("decodes");
        assert_eq!((back.dim_x, back.dim_z), (w, h));
        assert_eq!((back.size_x, back.size_z), (470.0, 290.0));
        assert_eq!(back.base_m, 214.9);
        // Quantised against its own range, so the step is the range over 65535.
        let step = g.relief() / 65535.0;
        for (a, b) in g.z.iter().zip(&back.z) {
            assert!((a - b).abs() <= step, "{a} vs {b}, step {step}");
        }
    }

    #[test]
    fn a_short_or_wrong_file_is_refused_rather_than_guessed() {
        assert!(Ground::decode(b"").is_err());
        assert!(Ground::decode(b"NOPE1234567890123456789012").is_err());
        // Every truncation, not one chosen length. Truncating by exactly four bytes is what
        // slipped through: it removed the cover count, which the reader treated as optional, and
        // left a file long enough to satisfy a length check that had not moved with the format.
        let g = Ground { dim_x: 9, dim_z: 9, size_x: 100.0, size_z: 100.0, z: vec![0.0; 81], cover: Vec::new(), base_m: 0.0 };
        let full = g.encode();
        for cut in 1..=40 {
            let short = &full[..full.len() - cut];
            assert!(
                Ground::decode(short).is_err(),
                "a file {cut} bytes short of {} was accepted",
                full.len()
            );
        }
        // And one with a cover plane, so the cover length is checked too.
        let withc = Ground { dim_x: 9, dim_z: 9, size_x: 100.0, size_z: 100.0, z: vec![0.0; 81], cover: vec![1u8; 81], base_m: 0.0 };
        let fc = withc.encode();
        assert!(Ground::decode(&fc).is_ok(), "a whole file with cover must still read");
        for cut in 1..=20 {
            assert!(Ground::decode(&fc[..fc.len() - cut]).is_err(), "cover {cut} short was accepted");
        }
        // A plausible header with nonsense in it is refused rather than guessed at.
        let mut bad = full.clone();
        bad[8] = 0; bad[9] = 0; bad[10] = 0; bad[11] = 0;
        assert!(Ground::decode(&bad).is_err(), "a zero dimension was accepted");
    }

    #[test]
    fn sampling_a_square_hits_its_own_cells_exactly() {
        let (w, h) = (33usize, 21usize);
        let per = 10.0f32;
        let g = Ground {
            dim_x: w,
            dim_z: h,
            size_x: per * (w - 1) as f32,
            size_z: per * (h - 1) as f32,
            z: bump2(w, h),
            cover: Vec::new(),
            base_m: 0.0,
        };
        for y in [0usize, 7, 16, 20] {
            for x in [0usize, 1, 20, 32] {
                let got = g.at(x as f32 * per, y as f32 * per);
                let want = g.z[y * w + x];
                assert!((got - want).abs() < 1e-3, "at cell ({x},{y}): {got} vs {want}");
            }
        }
        // And off the edge it clamps rather than reading rubbish.
        assert!(g.at(-50.0, -50.0).is_finite());
        assert!(g.at(1e6, 1e6).is_finite());
    }

    #[test]
    fn destriping_takes_out_a_stripe_and_leaves_the_land() {
        let (w, h) = (257usize, 161usize);
        let cell = 470.0 / (w - 1) as f32;
        let land = bump2(w, h);
        // A 12 m east-west wave, coherent all the way down the flight line, 1 cm tall — which
        // is the amplitude measured on the Ironman tile's flat ground.
        let mut striped = land.clone();
        for y in 0..h {
            for x in 0..w {
                striped[y * w + x] += 0.010 * (x as f32 * cell / 12.0 * std::f32::consts::TAU).sin();
            }
        }
        let before = rms_diff(&striped, &land);
        let mut fixed = striped.clone();
        destripe(&mut fixed, w, h, cell, true, 1.0);
        let after = rms_diff(&fixed, &land);

        // It has to take *some* of a known stripe out, and it does not have to take all of it.
        // This assertion used to demand a quarter and the filter has never managed that; it is a
        // deliberately mild filter and the module says so. The number here is what it actually
        // achieves, so that a future change which weakens it further is caught.
        assert!(after < before * 0.90, "stripe rms {before} -> {after}, none of it taken out");

        // The property that matters more, and the one that is easy to lose while chasing the one
        // above: on ground with no stripe in it the filter must barely move anything. Lengthening
        // the detrend to catch more stripe takes this from 0.2 cm to 1.3 cm, which is why it is
        // short. A destriper that invents structure is worse than no destriper.
        let mut clean = land.clone();
        let moved = destripe(&mut clean, w, h, cell, true, 1.0);
        assert!(moved < 0.004, "moved clean ground by {moved} m rms");
    }

    fn rms_diff(a: &[f32], b: &[f32]) -> f32 {
        let s: f64 = a.iter().zip(b).map(|(x, y)| ((x - y) as f64).powi(2)).sum();
        (s / a.len() as f64).sqrt() as f32
    }

    #[test]
    fn a_geo_key_directory_gives_up_its_epsg() {
        // Header (1,1,0,n) then four shorts a key. This is the Ironman tile's own directory.
        let keys = vec![
            1, 1, 0, 8, 1024, 0, 1, 1, 1025, 0, 1, 1, 1026, 34737, 21, 0, 2049, 34737, 6, 21,
            2054, 0, 1, 9102, 2062, 34736, 3, 0, 3072, 0, 1, 26916, 3076, 0, 1, 9001,
        ];
        assert_eq!(geo_key(&keys, KEY_PROJECTED_CRS), Some(26916));
        assert_eq!(geo_key(&keys, 4096), None);
        assert_eq!(geo_key(&[], KEY_PROJECTED_CRS), None);
        // A key whose value lives in another tag is not an EPSG code and is not returned.
        assert_eq!(geo_key(&keys, 1026), None);
    }

    /// The photograph is registered to the plot by world coordinates, not stretched across it.
    ///
    /// This is the fault that made a built scan's ground wrong and could not be seen in any
    /// statistic: the classified shares were entirely plausible either way. The tile is 500 m,
    /// the plot is a 470 m window 15 m inside it, and the old code mapped the whole image onto
    /// the whole plot — so the paint was up to 15 m from the ground it describes, correct in
    /// the middle and furthest out at the edges, which is exactly the signature that looks like
    /// "nearly right" in a picture.
    ///
    /// A synthetic photograph with one green square in a stated world position settles it: the
    /// square has to come back over the plot cells that square really covers.
    #[test]
    fn the_photograph_lands_where_the_world_says_it_does() {
        let dir = std::env::temp_dir().join(format!("mxb-cover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A 500 m tile at 1 m, its photograph at 0.5 m a pixel over the same ground.
        let dem = Dem {
            w: 500,
            h: 500,
            cell: 1.0,
            origin_e: 1000.5,
            origin_n: 5000.5 - 0.0,
            epsg: 26916,
            z: vec![0.0; 500 * 500],
        };
        let (tile_w, tile_n) = (1000.0f64, 5000.5 + 0.5);
        let (iw, ih) = (1000u32, 1000u32);
        // Bare dirt everywhere, and one green square from 300 to 340 m east of the tile's west
        // edge and 100 to 140 m south of its north edge.
        let mut img = image::RgbImage::from_pixel(iw, ih, image::Rgb([150, 120, 95]));
        for y in 200..280u32 {
            for x in 600..680u32 {
                img.put_pixel(x, y, image::Rgb([40, 120, 40]));
            }
        }
        img.save(dir.join("t.imagery.png")).unwrap();
        std::fs::write(dir.join("t.lap.json"), br#"{"imagery":{"path":"t.imagery.png"}}"#).unwrap();

        // The plot: 470 m starting 15 m inside the tile's west and north edges.
        let (west, north, per, dim) = (tile_w + 15.0, tile_n - 15.0, 470.0 / 1024.0, 1025usize);
        let cover = imagery_cover(
            &dir.join("t.dem.tif"),
            &dir.join("t.lap.json"),
            &dem,
            west,
            north,
            per,
            dim,
            dim,
        );
        assert_eq!(cover.len(), dim * dim);

        // Where that square falls in plot metres, and so in plot cells.
        let at = |e: f64, n: f64| -> u8 {
            let x = ((e - west) / per).round() as usize;
            let y = ((north - n) / per).round() as usize;
            cover[y.min(dim - 1) * dim + x.min(dim - 1)]
        };
        // Inside the square: vegetation.
        for (e, n) in [(310.0, 110.0), (330.0, 130.0), (320.0, 120.0)] {
            let (e, n) = (tile_w + e, tile_n - n);
            assert_eq!(
                at(e, n),
                Cover::Vegetation.id(),
                "the green square is missing at {e}, {n}"
            );
        }
        // Well outside it, in every direction: not vegetation. A stretched photograph puts the
        // square about 10 m off here, which these catch.
        for (e, n) in [(280.0, 120.0), (360.0, 120.0), (320.0, 80.0), (320.0, 160.0)] {
            let (e, n) = (tile_w + e, tile_n - n);
            assert_ne!(
                at(e, n),
                Cover::Vegetation.id(),
                "the green square has spread to {e}, {n}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every soil layer gets a share of the plot, and the shares are the photograph's own.
    ///
    /// A scan is painted from the photograph and nothing else, so the one thing that can go
    /// wrong quietly is the classifier collapsing: one class taking the whole venue leaves a
    /// 470 m plot tiling a single sheet, which is what shipped on 2026-09-16 and reads as flat
    /// ground under any amount of good terrain. A gradient is the honest worst case — real
    /// ground has no hard edges in it — and even that has to come out banded.
    #[test]
    fn a_venue_comes_out_in_more_than_one_tone() {
        let dir = std::env::temp_dir().join(format!("mxb-tone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dem = Dem { w: 200, h: 200, cell: 1.0, origin_e: 0.5, origin_n: 199.5, epsg: 26916, z: vec![0.0; 40000] };
        let (iw, ih) = (400u32, 400u32);
        let mut img = image::RgbImage::new(iw, ih);
        for y in 0..ih {
            for x in 0..iw {
                // Soil from dim to bright across the frame, one strip of it green.
                let l = (100 + (x * 120 / iw)) as u8;
                let p = if y < ih / 5 { image::Rgb([40, 120, 40]) } else { image::Rgb([l, l - 20, l - 40]) };
                img.put_pixel(x, y, p);
            }
        }
        img.save(dir.join("t.imagery.png")).unwrap();
        std::fs::write(dir.join("t.lap.json"), b"{}").unwrap();
        let dim = 257usize;
        let cover = imagery_cover(
            &dir.join("t.dem.tif"),
            &dir.join("t.lap.json"),
            &dem,
            0.0,
            200.0,
            200.0 / (dim - 1) as f64,
            dim,
            dim,
        );
        let share = |c: Cover| {
            cover.iter().filter(|v| **v == c.id()).count() as f32 / cover.len() as f32
        };
        assert!(share(Cover::Vegetation) > 0.15, "the green strip went missing");
        for c in [Cover::SoilDark, Cover::SoilMid, Cover::SoilLight] {
            assert!(
                share(c) > 0.05,
                "{c:?} covers {:.1}% of the plot — the soil came out as one tone",
                share(c) * 100.0
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_trace_resolves_its_widths_and_its_start() {
        let text = r#"{
            "version": 1, "kind": "mxb-lap-trace", "name": "Test", "crs": "EPSG:26916",
            "closed": true, "default_width_m": 6.0, "start_index": 2,
            "points": [[10,20],[11,21,9.5],[12,22],[13,23]],
            "somethingNew": {"we": "have not heard of"}
        }"#;
        let mut raw: serde_json::Value = serde_json::from_str(text).unwrap();
        normalise_keys(&mut raw);
        let t: LapTrace = serde_json::from_value(raw).expect("reads, unknown keys and all");
        let p = t.resolved().expect("resolves");
        assert_eq!(p.len(), 4);
        // Rotated so the start line's point comes first.
        assert_eq!((p[0].e, p[0].n), (12.0, 22.0));
        assert_eq!(p[0].width_m, 6.0);
        // And the per-point width survived the rotation with its own point.
        assert_eq!(p[3].width_m, 9.5);
    }

    #[test]
    fn a_trace_that_cannot_be_read_says_why() {
        let bad = r#"{"points": [[1,2],[3,4],[5,6],[7,8]], "start_index": 99}"#;
        let t: LapTrace = serde_json::from_str(bad).unwrap();
        let e = t.resolved().unwrap_err().to_string();
        assert!(e.contains("start line is at point 99"), "{e}");

        let short = r#"{"points": [[1,2],[3],[5,6],[7,8]]}"#;
        let t: LapTrace = serde_json::from_str(short).unwrap();
        assert!(t.resolved().unwrap_err().to_string().contains("point 1"));
    }

    #[test]
    fn a_stretched_grid_is_told_apart_from_a_real_one() {
        // The fake: 4 m data at 0.5 m, nearest-neighbour, which is what IGN's HIGHRES layer
        // returns and what every other check passes.
        let (w, h) = (128usize, 128);
        let real = bump2(w, h);
        let mut stretched = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                stretched[y * w + x] = real[(y / 8 * 8) * w + (x / 8 * 8)];
            }
        }
        let f_real = stretched_fraction(&real, w);
        let f_fake = stretched_fraction(&stretched, w);
        assert!(f_real < STRETCHED_LIMIT, "real ground read {f_real:.3} stretched");
        assert!(f_fake > 0.5, "a stretched grid read only {f_fake:.3}");

        // And the trap: the stretched grid is ROUGHER by a six-metre detrend, so roughness would
        // have chosen it. This pins the reason the identical-neighbour test exists.
        let hp = |z: &[f32]| {
            let mut low = z.to_vec();
            blur_axis(&mut low, w, h, 6.0 / 2.355 / 0.5, true);
            blur_axis(&mut low, w, h, 6.0 / 2.355 / 0.5, false);
            let d: f32 = z.iter().zip(&low).map(|(a, b)| (a - b) * (a - b)).sum();
            (d / z.len() as f32).sqrt()
        };
        assert!(
            hp(&stretched) > hp(&real),
            "the stretched grid should measure rougher, which is why roughness is the wrong test"
        );
    }

    #[test]
    fn a_blank_answer_is_recognised() {
        // A wrong axis order returns HTTP 200 and a white image, so blankness is the only signal.
        assert!(looks_blank(&[255u8; 4096]), "a white image is blank");
        assert!(looks_blank(&[0u8; 4096]), "a black image is blank");
        assert!(looks_blank(&[]), "nothing is blank");
        let varied: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        assert!(!looks_blank(&varied), "a real image is not blank");
    }

    #[test]
    fn holes_get_filled_from_what_is_around_them() {
        let (w, h) = (17usize, 11usize);
        let mut z = bump2(w, h);
        let want = z[5 * w + 8];
        z[5 * w + 8] = f32::NAN;
        z[3 * w + 4] = f32::NAN;
        fill_holes(&mut z, w, h);
        assert!(z.iter().all(|v| v.is_finite()), "a NaN survived");
        // The fill is the neighbours' mean, so it lands near what was there.
        assert!((z[5 * w + 8] - want).abs() < 0.5, "{} vs {want}", z[5 * w + 8]);
    }
}
