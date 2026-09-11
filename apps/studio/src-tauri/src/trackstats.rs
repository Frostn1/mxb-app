//! Measuring published tracks, so a generated one can be held to their shape.
//!
//! This is the corpus behind track generation. A model asked for "a sandy national with a
//! long rhythm section" will produce something plausible-looking and unrideable unless the
//! numbers it works to came from tracks people actually ride, and those numbers are not
//! written down anywhere — they have to be measured out of the `.trh` files themselves.
//!
//! What makes that possible is that a height file carries more than relief. Its trailing
//! block paints every cell with a surface, and the riding line is one of them: some tracks
//! paint it as surface 10, and the rest leave it as the one patch of ground no mask covers.
//! Either way the corridor falls out as a mask, and once the corridor is known the rest is
//! ordinary measurement — how wide it runs, how steep it gets, how far its jumps stand above
//! the landscape they sit on and how far apart they are.
//!
//! Every figure here is a distribution over published tracks, never a claim about one.

#![allow(dead_code)]

use anyhow::{anyhow, bail, Result};
use std::path::Path;

use crate::heightfield;
use crate::track;

/// Radius of the blur that separates a track's features from its landscape.
///
/// Twelve metres is longer than any jump and shorter than any hill, so what survives the
/// subtraction is the built terrain and what it leaves behind is the ground it was built on.
const BASE_RADIUS_M: f32 = 12.0;

/// How far a lip has to be the highest point around before it counts as one. Half a takeoff
/// to the next, so two faces of the same jump can't both be counted.
const LIP_RADIUS_M: f32 = 4.0;

/// How far a lip stands above the landscape before it's a jump rather than a bump.
const LIP_MIN_M: f32 = 0.35;

/// Coverage, 0–255, at which a painted surface is taken to cover a cell.
const PAINTED_AT: f32 = 128.0;

/// Coverage below which nothing is covering a cell at all — the rule that finds the riding
/// line on tracks that never paint it.
const UNCOVERED_BELOW: f32 = 96.0;

/// The surface id tracks use for the riding line itself, where they name it at all.
const RIDING_LINE_ID: u32 = 10;

/// How much of a map a riding line can be. Below the floor the rule found scraps; above the
/// ceiling it found the ground. Measured tracks run 1.5% to 12%, so both ends have room.
const CORRIDOR_AREA: std::ops::RangeInclusive<f32> = 0.01..=0.30;

/// How much of a corridor has to be one piece before its width and length mean anything.
///
/// A riding line is one loop. Set loosely this passes masks that trace the *seams* between
/// painted surfaces instead — a branching network that measures a plausible width and a
/// plausible length while following the edges of the track rather than the track. Every
/// corridor that survives a look at the picture is a single component; the ones that aren't
/// were all wrong.
const CORRIDOR_JOINED: f32 = 0.9;

// ---------------------------------------------------------------------------
// What a measurement looks like
// ---------------------------------------------------------------------------

/// A distribution, reported rather than averaged. A track's jumps are not all one height and
/// the mean of them describes nothing.
#[derive(serde::Serialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Spread {
    pub p10: f32,
    pub p50: f32,
    pub p90: f32,
    pub p99: f32,
    pub max: f32,
    pub mean: f32,
    pub count: usize,
}

fn spread(values: &mut Vec<f32>) -> Spread {
    if values.is_empty() {
        return Spread::default();
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let at = |q: f32| -> f32 {
        let i = ((values.len() - 1) as f32 * q).round() as usize;
        values[i.min(values.len() - 1)]
    };
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64;
    Spread {
        p10: at(0.10),
        p50: at(0.50),
        p90: at(0.90),
        p99: at(0.99),
        max: *values.last().unwrap(),
        mean: mean as f32,
        count: values.len(),
    }
}

/// One surface a track paints, and how much of it it covers.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStat {
    pub id: u32,
    pub name: String,
    pub fraction: f32,
}

/// The riding line, and everything measurable about it.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CorridorStats {
    /// `painted` when the track named the riding line, `uncovered` when it was found as the
    /// ground nothing else covers. Which rule fired matters: the two are different evidence.
    pub rule: &'static str,
    pub area_fraction: f32,
    /// How much of the corridor is one connected ribbon. A real riding line is nearly all of
    /// it; a low figure means the rule found scattered patches and the numbers below are
    /// measuring something that isn't a track.
    pub largest_component_fraction: f32,
    pub area_m2: f32,
    /// Corridor area over its width — the ribbon's length, without tracing it.
    pub length_m: f32,
    /// Two independent estimates. For a ribbon of width W the distance-to-edge is uniform on
    /// `[0, W/2]`, so the mean is `W/4` and the far tail is `W/2`. They should agree; where
    /// they don't, the corridor isn't ribbon-shaped.
    pub width_from_mean_m: f32,
    pub width_from_tail_m: f32,
    pub slope_deg: Spread,
    pub off_slope_deg: Spread,
    /// How far the built terrain stands off the landscape under it.
    pub feature_relief_m: Spread,
    pub lips: usize,
    pub lips_per_km: f32,
    pub lip_height_m: Spread,
    pub lip_spacing_m: Spread,
}

/// Everything one track has to say.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackStats {
    pub name: String,
    pub entry: String,
    pub source_grid: [u32; 2],
    pub analysis_grid: [u32; 2],
    pub size_x_m: f32,
    pub size_z_m: f32,
    pub metres_per_sample: f32,
    pub height_range_m: f32,
    pub surfaces: Vec<SurfaceStat>,
    pub corridor: Option<CorridorStats>,
    /// Why there is no corridor, when there isn't one. A track that can't be measured is
    /// evidence about the corpus, not a gap in it.
    pub corridor_note: Option<String>,
    /// What the track's own centreline says, when it carries one. Independent of the
    /// corridor: this is the only thing that measures a track which painted no surfaces.
    pub ridden: Option<RiddenStats>,
}

// ---------------------------------------------------------------------------
// Reading one track
// ---------------------------------------------------------------------------

pub fn analyse(path: &Path) -> Result<TrackStats> {
    let names = track::entry_names(path)?;
    let entry = track::heightfield_entries(&names)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no heightfield in this track"))?;
    let bytes = track::read_entry(path, &entry)?;

    let layout = heightfield::probe(&bytes, None)
        .ok_or_else(|| anyhow!("{entry} doesn't read as a terrain grid"))?;
    // Everything below is in metres. A track that doesn't state its own scale can still be
    // drawn — the viewer does it — but it cannot be measured, and a corpus of unit-less
    // numbers is worse than a smaller corpus.
    let Some(mps_src) = layout.metres_per_sample else {
        bail!("{entry} states no footprint, so nothing here would be in metres");
    };
    if layout.height_scale.is_none() {
        bail!("{entry} states no height scale, so its relief means nothing");
    }

    let size_x = mps_src * (layout.width.max(2) - 1) as f32;
    let size_z = mps_src * (layout.height.max(2) - 1) as f32;

    let block_at =
        layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
    let block = bytes.get(block_at..).unwrap_or(&[]);
    let masks = track::coverage_masks(block);
    let materials = material_names(block);

    // The masks' own resolution, so coverage and height are read on one grid. Masks are half
    // a step coarser than the heightfield — 2048 against 2049 — which costs nothing and
    // spares an interpolation on the side that matters.
    let want = match masks.first() {
        Some(m) => m.width.max(m.height),
        None => layout.width.max(layout.height),
    };
    let (gw, gh, heights) = heightfield::read_grid(&bytes, &layout, want);
    let (gw, gh) = (gw as usize, gh as usize);
    let mps = size_x / (gw.max(2) - 1) as f32;

    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for v in &heights {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }

    // One smoothed coverage grid per painted surface. The masks are dithered — a patch meant
    // to be half-covered stores half its cells at 255 and half at 0 — so a single cell says
    // nothing and everything here reads them over a metre of ground.
    let dither_r = (1.0 / mps).round().max(1.0) as usize;
    let mut covers: Vec<(u32, Vec<f32>)> = Vec::new();
    for m in &masks {
        let Some(grid) = mask_grid(block, m, gw, gh) else {
            continue;
        };
        covers.push((m.id, box_blur(&grid, gw, gh, dither_r)));
    }

    let cells = (gw * gh) as f32;
    let surfaces = covers
        .iter()
        .map(|(id, g)| SurfaceStat {
            id: *id,
            name: material_name(&materials, *id),
            fraction: g.iter().map(|v| (*v >= PAINTED_AT) as u32 as f32).sum::<f32>() / cells,
        })
        .collect();

    // Measured at the file's own resolution rather than the masks'. A groove is a metre
    // across and a quarter-metre sample can just hold one; halving the grid to match a
    // 1024-cell mask erases every rut on the track.
    let ridden = crate::trackline::read(block).and_then(|lap| {
        let (fw, fh, v) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        ridden(
            &lap,
            &Grid { w: fw as usize, h: fh as usize, size_x, size_z, v },
        )
    });

    let (corridor, corridor_note) = match corridor_mask(&covers, gw, gh, mps) {
        Ok((rule, mask)) => {
            // `FROST_SHAPES=/tmp/dir` draws what was found. Every figure below is a summary of
            // this picture, and a corridor that isn't the racing line summarises just as
            // convincingly as one that is — so the picture is the only real check.
            if let Ok(dir) = std::env::var("FROST_SHAPES") {
                dump_corridor(Path::new(&dir), path, &heights, &mask, gw, gh);
            }
            let c = measure(rule, &mask, &heights, gw, gh, mps);
            // A ribbon in pieces measures nothing: its width comes out as the width of the
            // widest scrap and its length as the area divided by that.
            if c.largest_component_fraction < CORRIDOR_JOINED {
                let note = format!(
                    "the {} corridor is in pieces — largest is {:.0}% of it",
                    c.rule,
                    c.largest_component_fraction * 100.0
                );
                (None, Some(note))
            } else {
                (Some(c), None)
            }
        }
        Err(why) => (None, Some(why)),
    };

    Ok(TrackStats {
        name: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        entry,
        source_grid: [layout.width, layout.height],
        analysis_grid: [gw as u32, gh as u32],
        size_x_m: size_x,
        size_z_m: size_z,
        metres_per_sample: mps,
        height_range_m: if lo.is_finite() && hi.is_finite() {
            hi - lo
        } else {
            0.0
        },
        surfaces,
        corridor,
        corridor_note,
        ridden,
    })
}

/// The riding line, by whichever rule the track supports.
///
/// A track that names surface 10 has told us where the line is; one that doesn't has still
/// told us, by painting everything that *isn't* it — the ribbon is the hole in the paint.
///
/// Two things make this harder than picking a mask. Masks **layer** rather than partition:
/// Briarcliff paints surface 10 over 99% of its map and grass over 85% of that again, so the
/// ribbon is what's left when the later coat wins, not what the first one covers. And a
/// single id can appear several times — I40 carries seven masks numbered 10 — so there is no
/// one mask to find. Compositing by argmax settles both at once, which is the same thing the
/// surface picture in `track.rs` does.
///
/// What comes out still has to look like a riding line. A track that paints almost nothing
/// leaves its whole map uncovered, and calling that a corridor would report a 600-metre-wide
/// track rather than admitting the track never said.
fn corridor_mask(
    covers: &[(u32, Vec<f32>)],
    gw: usize,
    gh: usize,
    mps: f32,
) -> Result<(&'static str, Vec<bool>), String> {
    if covers.is_empty() {
        return Err("track paints no surfaces".into());
    }

    // Whichever surface covers a cell most, or none of them.
    let mut winner: Vec<Option<u32>> = vec![None; gw * gh];
    let mut best = vec![UNCOVERED_BELOW; gw * gh];
    for (id, g) in covers {
        for (i, v) in g.iter().enumerate() {
            if *v > best[i] {
                best[i] = *v;
                winner[i] = Some(*id);
            }
        }
    }

    let area = |m: &[bool]| m.iter().filter(|v| **v).count() as f32 / (gw * gh) as f32;
    let painted: Vec<bool> = winner.iter().map(|w| *w == Some(RIDING_LINE_ID)).collect();
    let uncovered: Vec<bool> = winner.iter().map(|w| w.is_none()).collect();

    // The named line first — a track that says where it rides is better evidence than a hole.
    for (rule, mask) in [("painted", painted), ("uncovered", uncovered)] {
        let a = area(&mask);
        if a == 0.0 {
            continue;
        }
        if !CORRIDOR_AREA.contains(&a) {
            // Keep looking: a painted layer that covers the world is exactly Briarcliff, and
            // its real ribbon is the uncovered set underneath.
            continue;
        }
        // Dither leaves pinholes through the ribbon, and they cut it into pieces that a
        // connectivity test then reports as scattered. Close them first, over a metre.
        let mask = close(&mask, gw, gh, (1.0 / mps).round().max(1.0) as usize);
        return Ok((rule, mask));
    }

    let (pa, ua) = (
        area(&winner.iter().map(|w| *w == Some(RIDING_LINE_ID)).collect::<Vec<_>>()),
        area(&winner.iter().map(|w| w.is_none()).collect::<Vec<_>>()),
    );
    Err(format!(
        "no surface reads as a riding line — line {:.0}% of the map, unpainted {:.0}%",
        pa * 100.0,
        ua * 100.0
    ))
}

/// The corridor drawn over the terrain it was cut from, as a PPM.
///
/// Shaded by slope rather than by height, because a track's jumps are a metre of relief on a
/// landscape with twenty and disappear entirely under a height ramp.
fn dump_corridor(dir: &Path, track: &Path, heights: &[f32], mask: &[bool], gw: usize, gh: usize) {
    let _ = std::fs::create_dir_all(dir);
    let name = track.file_stem().unwrap_or_default().to_string_lossy();
    let base = box_blur(heights, gw, gh, 6);
    let mut ppm = format!("P6\n{gw} {gh}\n255\n").into_bytes();
    for i in 0..gw * gh {
        let shade = (((heights[i] - base[i]) * 90.0) + 128.0).clamp(0.0, 255.0) as u8;
        // The ribbon in red over a grey relief, so a corridor that has wandered off the
        // racing line is obvious rather than merely a different number.
        ppm.extend_from_slice(&if mask[i] {
            [255, shade / 3, shade / 3]
        } else {
            [shade, shade, shade]
        });
    }
    let _ = std::fs::write(dir.join(format!("{name}.ppm")), ppm);
}

/// Morphological close — dilate then erode, so holes smaller than the radius fill in while
/// the outline stays where it was.
fn close(mask: &[bool], gw: usize, gh: usize, r: usize) -> Vec<bool> {
    let v: Vec<f32> = mask.iter().map(|m| if *m { 1.0 } else { 0.0 }).collect();
    let dilated = max_filter(&v, gw, gh, r);
    let negated: Vec<f32> = dilated.iter().map(|x| -x).collect();
    let eroded = max_filter(&negated, gw, gh, r);
    eroded.iter().map(|x| -x >= 0.5).collect()
}

pub(crate) fn measure(
    rule: &'static str,
    mask: &[bool],
    heights: &[f32],
    gw: usize,
    gh: usize,
    mps: f32,
) -> CorridorStats {
    let cells = (gw * gh) as f32;
    let area_cells = mask.iter().filter(|v| **v).count();
    let largest = largest_component(mask, gw, gh);

    // Distance to the corridor's edge, in metres, over the corridor only.
    let dist = distance_transform(mask, gw, gh, mps);
    let mut edge: Vec<f32> = mask
        .iter()
        .zip(&dist)
        .filter(|(m, _)| **m)
        .map(|(_, d)| *d)
        .collect();
    let edge_spread = spread(&mut edge);

    let slope = slope_degrees(heights, gw, gh, mps);
    let mut on: Vec<f32> = Vec::with_capacity(area_cells);
    let mut off: Vec<f32> = Vec::with_capacity(gw * gh - area_cells);
    for (i, s) in slope.iter().enumerate() {
        if mask[i] {
            on.push(*s)
        } else {
            off.push(*s)
        }
    }

    // The built terrain, with the landscape it sits on subtracted away.
    let base = box_blur(heights, gw, gh, (BASE_RADIUS_M / mps).round().max(1.0) as usize);
    let feature: Vec<f32> = heights.iter().zip(&base).map(|(h, b)| h - b).collect();
    let mut relief: Vec<f32> = feature
        .iter()
        .zip(mask)
        .filter(|(_, m)| **m)
        .map(|(f, _)| f.abs())
        .collect();

    let lip_r = (LIP_RADIUS_M / mps).round().max(1.0) as usize;
    let peak = max_filter(&feature, gw, gh, lip_r);
    let mut lips: Vec<(f32, f32, f32)> = Vec::new(); // x m, z m, height m
    for y in 0..gh {
        for x in 0..gw {
            let i = y * gw + x;
            if !mask[i] || feature[i] < LIP_MIN_M || feature[i] < peak[i] - 1e-4 {
                continue;
            }
            lips.push((x as f32 * mps, y as f32 * mps, feature[i]));
        }
    }
    // A flat lip ties with itself across every cell of its plateau. One per takeoff.
    let lips = thin(lips, LIP_RADIUS_M);

    let mut heights_m: Vec<f32> = lips.iter().map(|l| l.2).collect();
    let mut spacing = nearest_spacing(&lips);

    let width_mean = edge_spread.mean * 4.0;
    let width_tail = edge_spread.p99 * 2.0;
    let area_m2 = area_cells as f32 * mps * mps;
    let width = if width_mean > 0.0 { width_mean } else { 1.0 };
    let length_m = area_m2 / width;

    CorridorStats {
        rule,
        area_fraction: area_cells as f32 / cells,
        largest_component_fraction: if area_cells == 0 {
            0.0
        } else {
            largest as f32 / area_cells as f32
        },
        area_m2,
        length_m,
        width_from_mean_m: width_mean,
        width_from_tail_m: width_tail,
        slope_deg: spread(&mut on),
        off_slope_deg: spread(&mut off),
        feature_relief_m: spread(&mut relief),
        lips: lips.len(),
        lips_per_km: if length_m > 0.0 {
            lips.len() as f32 * 1000.0 / length_m
        } else {
            0.0
        },
        lip_height_m: spread(&mut heights_m),
        lip_spacing_m: spread(&mut spacing),
    }
}

// ---------------------------------------------------------------------------
// The measurements themselves
// ---------------------------------------------------------------------------

/// Slope of the ground, in degrees, by central difference.
fn slope_degrees(h: &[f32], gw: usize, gh: usize, mps: f32) -> Vec<f32> {
    let mut out = vec![0.0f32; gw * gh];
    let step = 2.0 * mps;
    for y in 0..gh {
        for x in 0..gw {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(gw - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(gh - 1);
            let dx = (h[y * gw + xp] - h[y * gw + xm]) / step;
            let dy = (h[yp * gw + x] - h[ym * gw + x]) / step;
            out[y * gw + x] = (dx * dx + dy * dy).sqrt().atan().to_degrees();
        }
    }
    out
}

/// Metres from each corridor cell to the nearest cell outside it. Chamfer 3-4, which is
/// within a few percent of Euclidean and one pass each way instead of a search.
fn distance_transform(mask: &[bool], gw: usize, gh: usize, mps: f32) -> Vec<f32> {
    const BIG: i32 = 1 << 20;
    let mut d: Vec<i32> = mask.iter().map(|m| if *m { BIG } else { 0 }).collect();
    let at = |x: usize, y: usize| y * gw + x;

    for y in 0..gh {
        for x in 0..gw {
            let mut v = d[at(x, y)];
            if y > 0 {
                v = v.min(d[at(x, y - 1)] + 3);
                if x > 0 {
                    v = v.min(d[at(x - 1, y - 1)] + 4);
                }
                if x + 1 < gw {
                    v = v.min(d[at(x + 1, y - 1)] + 4);
                }
            }
            if x > 0 {
                v = v.min(d[at(x - 1, y)] + 3);
            }
            d[at(x, y)] = v;
        }
    }
    for y in (0..gh).rev() {
        for x in (0..gw).rev() {
            let mut v = d[at(x, y)];
            if y + 1 < gh {
                v = v.min(d[at(x, y + 1)] + 3);
                if x > 0 {
                    v = v.min(d[at(x - 1, y + 1)] + 4);
                }
                if x + 1 < gw {
                    v = v.min(d[at(x + 1, y + 1)] + 4);
                }
            }
            if x + 1 < gw {
                v = v.min(d[at(x + 1, y)] + 3);
            }
            d[at(x, y)] = v;
        }
    }
    d.iter().map(|v| *v as f32 / 3.0 * mps).collect()
}

/// Size of the largest 4-connected run of set cells.
fn largest_component(mask: &[bool], gw: usize, gh: usize) -> usize {
    let mut seen = vec![false; gw * gh];
    let mut best = 0usize;
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..gw * gh {
        if !mask[start] || seen[start] {
            continue;
        }
        let mut size = 0usize;
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % gw, i / gw);
            let push = |j: usize, seen: &mut Vec<bool>, stack: &mut Vec<usize>| {
                if mask[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1, &mut seen, &mut stack);
            }
            if x + 1 < gw {
                push(i + 1, &mut seen, &mut stack);
            }
            if y > 0 {
                push(i - gw, &mut seen, &mut stack);
            }
            if y + 1 < gh {
                push(i + gw, &mut seen, &mut stack);
            }
        }
        best = best.max(size);
    }
    best
}

/// One point per plateau: the highest in each cell of a grid the lip radius across.
fn thin(mut pts: Vec<(f32, f32, f32)>, radius_m: f32) -> Vec<(f32, f32, f32)> {
    use std::collections::HashMap;
    pts.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: HashMap<(i32, i32), (f32, f32, f32)> = HashMap::new();
    for p in pts {
        let key = (
            (p.0 / radius_m).floor() as i32,
            (p.1 / radius_m).floor() as i32,
        );
        kept.entry(key).or_insert(p);
    }
    kept.into_values().collect()
}

/// Distance from each lip to its nearest neighbour — the rhythm of a track, without having
/// to trace its line.
fn nearest_spacing(pts: &[(f32, f32, f32)]) -> Vec<f32> {
    if pts.len() < 2 {
        return Vec::new();
    }
    // Quadratic, but bounded: thinning leaves hundreds of lips on a track, not thousands.
    let take = pts.len().min(4000);
    let mut out = Vec::with_capacity(take);
    for (i, a) in pts.iter().take(take).enumerate() {
        let mut best = f32::INFINITY;
        for (j, b) in pts.iter().take(take).enumerate() {
            if i == j {
                continue;
            }
            let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
            best = best.min(d);
        }
        if best.is_finite() {
            out.push(best);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Grid plumbing
// ---------------------------------------------------------------------------

/// One coverage mask, sampled onto the analysis grid.
fn mask_grid(block: &[u8], c: &track::Coverage, gw: usize, gh: usize) -> Option<Vec<f32>> {
    let (mw, mh) = (c.width as usize, c.height as usize);
    let data = block.get(c.at..c.at + mw * mh)?;
    let mut out = vec![0.0f32; gw * gh];
    for y in 0..gh {
        let sy = (y * mh / gh).min(mh.saturating_sub(1));
        for x in 0..gw {
            let sx = (x * mw / gw).min(mw.saturating_sub(1));
            out[y * gw + x] = data[sy * mw + sx] as f32;
        }
    }
    Some(out)
}

/// Mean over a square window, separable, in two passes of prefix sums.
pub(crate) fn box_blur(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return v.to_vec();
    }
    let mut tmp = vec![0.0f32; w * h];
    let mut pre = vec![0.0f64; w.max(h) + 1];
    for y in 0..h {
        let row = &v[y * w..(y + 1) * w];
        for x in 0..w {
            pre[x + 1] = pre[x] + row[x] as f64;
        }
        for x in 0..w {
            let a = x.saturating_sub(r);
            let b = (x + r + 1).min(w);
            tmp[y * w + x] = ((pre[b] - pre[a]) / (b - a) as f64) as f32;
        }
    }
    let mut out = vec![0.0f32; w * h];
    for x in 0..w {
        for y in 0..h {
            pre[y + 1] = pre[y] + tmp[y * w + x] as f64;
        }
        for y in 0..h {
            let a = y.saturating_sub(r);
            let b = (y + r + 1).min(h);
            out[y * w + x] = ((pre[b] - pre[a]) / (b - a) as f64) as f32;
        }
    }
    out
}

/// Maximum over a square window. Separable too, by transposing between the passes rather
/// than writing the sliding window twice.
fn max_filter(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return v.to_vec();
    }
    let rows = slide_max_rows(v, w, h, r);
    let t = transpose(&rows, w, h);
    let cols = slide_max_rows(&t, h, w, r);
    transpose(&cols, h, w)
}

/// Sliding-window maximum along each row, by monotonic deque — one pass whatever the radius.
fn slide_max_rows(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut out = vec![f32::NEG_INFINITY; w * h];
    let mut dq: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for y in 0..h {
        let row = &v[y * w..(y + 1) * w];
        dq.clear();
        for i in 0..w {
            while let Some(&b) = dq.back() {
                if row[b] <= row[i] {
                    dq.pop_back();
                } else {
                    break;
                }
            }
            dq.push_back(i);
            if i >= r {
                let x = i - r;
                while let Some(&f) = dq.front() {
                    if f + r < x {
                        dq.pop_front();
                    } else {
                        break;
                    }
                }
                out[y * w + x] = row[*dq.front().unwrap()];
            }
        }
        // The last `r` outputs never see a full window on the right; they take what's left.
        for x in w.saturating_sub(r)..w {
            while let Some(&f) = dq.front() {
                if f + r < x {
                    dq.pop_front();
                } else {
                    break;
                }
            }
            if let Some(&f) = dq.front() {
                out[y * w + x] = row[f];
            }
        }
    }
    out
}

fn transpose(v: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            out[x * h + y] = v[y * w + x];
        }
    }
    out
}

/// The surface names in a height file's material table.
fn material_names(block: &[u8]) -> Vec<String> {
    let Some(at) = track::material_table_offset(block) else {
        return Vec::new();
    };
    let Some(count) = block
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    else {
        return Vec::new();
    };
    // Sixteen bytes of name then nine floats of physics, per the tracks this was read off.
    let mut out = Vec::new();
    for i in 0..count.min(64) as usize {
        let o = at + 4 + i * 52;
        let Some(raw) = block.get(o..o + 16) else { break };
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        out.push(String::from_utf8_lossy(&raw[..end]).into_owned());
    }
    out
}

fn material_name(names: &[String], id: u32) -> String {
    names
        .get(id as usize)
        .cloned()
        .unwrap_or_else(|| match id {
            RIDING_LINE_ID => "riding line".into(),
            _ => format!("surface {id}"),
        })
}

// ---------------------------------------------------------------------------
// The riding line, read rather than found
// ---------------------------------------------------------------------------

/// Metres of centreline between the stations the profiles are taken at.
const RIDDEN_STEP_M: f32 = 0.5;

/// How far either side of the line a profile reaches, and how finely it is sampled.
const RIDDEN_REACH_M: f32 = 12.0;
const RIDDEN_LATERAL_M: f32 = 0.125;

/// Half the riding line, for the purpose of measuring what is on it. Deliberately a constant
/// rather than the corridor's own width: a rut is only a rut where riders go, and a corridor
/// rule that includes the graded shoulder would count the field's texture as grooves.
const RIDDEN_HALF_M: f32 = 5.5;

/// Metres the cross-profile is smoothed over before the grooves are read off it. Wide enough
/// that a rut is a departure from the profile rather than part of it, narrow enough not to
/// swallow the camber.
const RUT_DETREND_M: f32 = 6.0;

/// How far a groove has to stand below the crests either side of it to be one.
const RUT_PROMINENCE_M: f32 = 0.05;

/// A corner, for the purpose of measuring what corners do to the ground.
const RIDDEN_CORNER_R_M: f32 = 40.0;

/// Metres the along-line elevation is smoothed over to separate the jumps from the land.
const RIDDEN_BASE_M: f32 = 60.0;

/// How far a lip stands above the land under it before it is one.
const RIDDEN_LIP_M: f32 = 0.30;

/// A lip of this height is a jump; below it is a roller. Indiana has both, and pooling them
/// says the average jump on a national is knee-high.
const RIDDEN_BIG_LIP_M: f32 = 1.0;

/// What a published track's own centreline says, and what the ground either side of it looks
/// like.
///
/// The corridor rule in [`corridor_mask`] measures the tracks that painted their riding line.
/// This measures the ones that carry a centreline, which is a different and larger set — and
/// a better one, because it knows where the line *is* rather than inferring it, so a groove
/// can be measured across the track instead of merely as roughness.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RiddenStats {
    pub lap_m: f32,
    pub segments: usize,
    pub arcs: usize,
    pub straights: usize,
    pub straight_m: Spread,
    pub arc_radius_m: Spread,
    /// Runs of same-way arcs of 25° or more — what a rider would call a corner.
    pub turns: usize,
    pub turn_deg: Spread,
    /// The tightest radius inside each turn: the bit that actually has to be made.
    pub turn_radius_m: Spread,
    /// Every degree the lap turns through, both ways. A lap that closes turns 360° net; this
    /// is the gross figure, and it is what says whether a track is a shape or a scribble.
    pub total_turn_deg: f32,
    /// Highest ground on the lap over lowest — how much of a hill the track is built on.
    pub lap_climb_m: f32,
    /// Steepest grade along the riding line, degrees.
    pub grade_deg: Spread,

    /// Grooves counted across the line, and how far apart they lie.
    pub rut_lines: f32,
    pub rut_spacing_m: Spread,
    pub rut_depth_corner_m: Spread,
    pub rut_depth_straight_m: Spread,

    /// How far the outside and inside edges of a corner stand above the lowest ground on it.
    pub berm_outside_m: f32,
    pub berm_inside_m: f32,
    /// Cross-slope through a corner; positive banks into it.
    pub bank_deg: Spread,

    pub lips_per_km: f32,
    pub big_lips_per_km: f32,
    pub lip_height_m: Spread,
    pub big_lip_height_m: Spread,
    /// Steepest face over three metres, either side of a lip.
    pub lip_face_deg: Spread,
    pub lip_spacing_m: Spread,
}

/// One station of the centreline, with the ground across it.
struct Cross {
    radius: f32,
    /// Heights across the line, from `-RIDDEN_REACH_M` to `+RIDDEN_REACH_M`. Index 0 is the
    /// rider's left.
    p: Vec<f32>,
}

/// What a rut is shaped like, as distinct from how deep it is.
///
/// Depth was already measured and it is not what tells a real corner from a generated one.
/// Three things do:
///
/// - **Anisotropy.** A rut is a groove somebody drove down: sharp across, smooth along. Noise
///   is rough both ways. This is the ratio of the two, and it is the number that says whether
///   the ground was ridden or shaken.
/// - **Floor.** A worn groove is flat-bottomed — a tyre's width of it sits within a fifth of
///   its own depth. A sine wave has no floor at all.
/// - **Wall.** How steeply the ground climbs out of one, in degrees.
///
/// Measured on a band `RIDDEN_HALF_M` either side of the line, detrended over six metres so
/// the track's own shape drops out and only what is cut into it is left.
#[derive(Debug, Clone, Copy)]
pub struct RutShape {
    /// Roughness across the line, RMS metres after the six-metre detrend.
    pub across_rms_m: f32,
    /// And along it, the same way.
    pub along_rms_m: f32,
    /// `across / along`. One is noise; a ridden surface is well above it.
    pub anisotropy: f32,
    /// How much of a groove's width sits within a fifth of its depth, metres.
    pub floor_m: f32,
    /// The steepest wall out of a groove, degrees.
    pub wall_deg: f32,
    /// Grooves per cross-section, and how far apart they are.
    pub grooves: f32,
    pub spacing_m: f32,
    /// Roughness along the line at the scale a wheel feels rather than a lap: RMS metres
    /// after a one-metre detrend. Long undulations drop out; chatter does not.
    pub chatter_m: f32,
    /// The same, by how far off the line it is: on it (within a metre), two metres off, and
    /// four. A real track is polished where the wheels go and chopped up either side, and one
    /// number for the whole width cannot tell that from a lap that is rough everywhere.
    pub chatter_zones_m: [f32; 3],
}

/// How deep a dip has to be, and how wide, before it counts as a groove rather than as the
/// grain of the ground. Six centimetres is about a third of a knobbly tyre's height.
const GROOVE_MIN_M: f32 = 0.06;
const GROOVE_MIN_WIDE_M: f32 = 0.25;

/// Detrend a series against a running mean `half` samples either side, wrapping.
fn detrend(v: &[f32], half: usize) -> Vec<f32> {
    let n = v.len();
    if n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            let mut sum = 0.0;
            let mut count = 0.0;
            for k in 0..=2 * half {
                let j = (i + n + k - half.min(i + n)) % n;
                sum += v[j];
                count += 1.0;
            }
            v[i] - sum / count
        })
        .collect()
}

fn rms(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
}

/// How much a corner is the same cut swept round an arc.
///
/// Take the profile across the line at the corner's entry, apex and exit and correlate the
/// three. A published corner scores about **0.1** — the ground at its exit has nothing to do
/// with the ground at its entry, because ruts braid, merge and die out. A corner extruded
/// along one radius scores **0.9**: one profile, dragged round.
///
/// This is the measure that separates a track that reads as built from one that reads as
/// generated, and no amount of getting the rut *statistics* right moves it — a swept corner
/// can have exactly the right groove count, spacing and depth at every station and still be
/// obviously machine-made, because it has the same ones at every station.
///
/// `None` when the corner is too short to cut three independent sections through.
pub fn section_sweep(stations: &[(f32, f32, f32)], g: &Grid) -> Option<f32> {
    section_sweep_inner(stations, g, false)
}

/// The same, with the corner's mean cross-section removed first — see `strip_mean_profile`.
pub fn section_sweep_ruts(stations: &[(f32, f32, f32)], g: &Grid) -> Option<f32> {
    section_sweep_inner(stations, g, true)
}

fn section_sweep_inner(
    stations: &[(f32, f32, f32)],
    g: &Grid,
    strip_mean_profile: bool,
) -> Option<f32> {
    if stations.len() < 12 {
        return None;
    }
    const HALF_WIDTH_M: f32 = 9.0;
    const SAMPLES: usize = 72;
    let cut = |i: usize| -> Vec<f32> {
        let (x, z, heading) = stations[i];
        let (rx, rz) = crate::trackprog::right_vector(heading);
        let mut v: Vec<f32> = (0..SAMPLES)
            .map(|k| {
                let t = -HALF_WIDTH_M + 2.0 * HALF_WIDTH_M * k as f32 / (SAMPLES - 1) as f32;
                g.at(x + rx * t, z + rz * t)
            })
            .collect();
        // The corner's own slope is not its shape: a banked entry would correlate with a
        // banked exit on tilt alone and say the ruts matched when they don't.
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        for h in &mut v {
            *h -= mean;
        }
        v
    };
    let n = stations.len();
    let (mut a, mut b, mut c) = (cut(n * 15 / 100), cut(n / 2), cut(n * 85 / 100));
    // Optionally take the corner's *average* cross-section out of all three first. What is
    // left is only what differs station to station — the ruts — so the pair of numbers says
    // whether a corner repeats because its ruts repeat, or because the shape they sit in
    // (camber, width, the berm) is the same shape all the way round.
    if strip_mean_profile {
        let mut avg = vec![0.0f32; SAMPLES];
        let taken = 24.min(n);
        for k in 0..taken {
            let v = cut(k * (n - 1) / taken.max(1));
            for (m, h) in avg.iter_mut().zip(&v) {
                *m += h / taken as f32;
            }
        }
        for v in [&mut a, &mut b, &mut c] {
            for (h, m) in v.iter_mut().zip(&avg) {
                *h -= m;
            }
        }
    }
    let r = |x: &[f32], y: &[f32]| -> f32 {
        let (mx, my) = (
            x.iter().sum::<f32>() / x.len() as f32,
            y.iter().sum::<f32>() / y.len() as f32,
        );
        let (mut num, mut dx, mut dy) = (0.0f32, 0.0f32, 0.0f32);
        for (p, q) in x.iter().zip(y) {
            num += (p - mx) * (q - my);
            dx += (p - mx) * (p - mx);
            dy += (q - my) * (q - my);
        }
        if dx <= 0.0 || dy <= 0.0 {
            return 0.0;
        }
        num / (dx * dy).sqrt()
    };
    Some((r(&a, &b) + r(&b, &c) + r(&a, &c)) / 3.0)
}

/// How much a corner's cross-slope changes as you go round it, as the standard deviation of
/// the across-line gradient.
///
/// A berm whose height is a function of the corner's radius alone is the same height all the
/// way round a constant-radius corner, so every cross-section gets the same tilt and the
/// corner reads as extruded however well its ruts are made. Measured over four corners each:
/// Indiana 0.039, Southwick 0.095 — sand piles up far more unevenly — against 0.016-0.022 for
/// ours. This is the quantity behind [`section_sweep`], and the one worth aiming at, because
/// it names what to change rather than only saying that something is wrong.
pub fn camber_spread(stations: &[(f32, f32, f32)], g: &Grid) -> Option<f32> {
    if stations.len() < 12 {
        return None;
    }
    const HALF_WIDTH_M: f32 = 9.0;
    const SAMPLES: usize = 48;
    const CUTS: usize = 20;
    let n = stations.len();
    let mut slopes = Vec::with_capacity(CUTS);
    for c in 0..CUTS {
        let i = (n - 1) * c / (CUTS - 1);
        let (x, z, heading) = stations[i];
        let (rx, rz) = crate::trackprog::right_vector(heading);
        // Least squares slope of height against offset — the cut's tilt, in metres per metre.
        let (mut sx, mut sy, mut sxy, mut sxx) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for k in 0..SAMPLES {
            let off = -HALF_WIDTH_M + 2.0 * HALF_WIDTH_M * k as f32 / (SAMPLES - 1) as f32;
            let h = g.at(x + rx * off, z + rz * off);
            sx += off;
            sy += h;
            sxy += off * h;
            sxx += off * off;
        }
        let m = SAMPLES as f32;
        let den = m * sxx - sx * sx;
        if den.abs() > 1e-6 {
            slopes.push((m * sxy - sx * sy) / den);
        }
    }
    if slopes.len() < 4 {
        return None;
    }
    let mean = slopes.iter().sum::<f32>() / slopes.len() as f32;
    Some((slopes.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / slopes.len() as f32).sqrt())
}

/// [`RutShape`] for a lap over a heightfield, published or generated.
///
/// `stations` is the centreline: `(x, z, heading)` every `step_m` metres.
pub fn rut_shape(stations: &[(f32, f32, f32)], step_m: f32, g: &Grid) -> Option<RutShape> {
    if stations.len() < 64 {
        return None;
    }
    let lat = RIDDEN_LATERAL_M;
    let n = (2.0 * RIDDEN_HALF_M / lat) as usize + 1;
    let u_at = |i: usize| i as f32 * lat - RIDDEN_HALF_M;
    // Heights across the line at every station, and the same grid read the other way.
    let rows: Vec<Vec<f32>> = stations
        .iter()
        .map(|&(x, z, h)| {
            let (rx, rz) = crate::trackprog::right_vector(h);
            (0..n).map(|i| g.at(x + u_at(i) * rx, z + u_at(i) * rz)).collect()
        })
        .collect();

    // Across: detrend each row over six metres of *width*.
    let half_across = ((3.0 / lat) as usize).max(1);
    let mut across: Vec<f32> = Vec::new();
    let mut floors: Vec<f32> = Vec::new();
    let mut walls: Vec<f32> = Vec::new();
    let mut counts: Vec<f32> = Vec::new();
    let mut gaps: Vec<f32> = Vec::new();
    for row in &rows {
        let d = detrend(row, half_across);
        across.extend(d.iter().copied());
        // Grooves: runs cut at least `GROOVE_MIN_M` below the ground either side of them.
        //
        // It used to be "below a fifth of this section's own deepest cut", which is not a
        // groove detector at all: a section with no ruts in it has a deepest dip too, and a
        // fifth of that is grain. Proved by turning the rut field's own parameters up and
        // down and watching the count sit at 3.7 either way — a statistic nothing in the
        // generator could move, being tuned against.
        let deepest = -d.iter().copied().fold(f32::INFINITY, f32::min);
        if deepest < GROOVE_MIN_M {
            continue;
        }
        let cut = -GROOVE_MIN_M;
        let (mut run, mut last_centre, mut found) = (0usize, None::<f32>, 0usize);
        for i in 0..d.len() {
            if d[i] <= cut {
                run += 1;
            } else if run > 0 {
                // And wide enough to put a wheel in.
                if (run as f32 * lat) < GROOVE_MIN_WIDE_M {
                    run = 0;
                    continue;
                }
                found += 1;
                floors.push(run as f32 * lat);
                let centre = (i as f32 - run as f32 * 0.5) * lat - RIDDEN_HALF_M;
                if let Some(prev) = last_centre {
                    gaps.push(centre - prev);
                }
                last_centre = Some(centre);
                run = 0;
            }
        }
        counts.push(found as f32);
        for i in 1..d.len() {
            walls.push(((d[i] - d[i - 1]).abs() / lat).atan().to_degrees());
        }
    }

    // Along: the same detrend down each lateral offset, over six metres of *track*.
    // A metre either side, not three.
    //
    // A rut is smooth along its own length at every scale a wheel feels; a jump is thirty
    // metres long. Detrending the along-track series over six metres leaves the jumps in it,
    // so a lap with a table every twenty metres measured *rougher along than across* —
    // anisotropy 0.65 where ridden ground is 1.70 — and the number said nothing about the
    // surface at all.
    let half_along = ((1.0 / step_m) as usize).max(1);
    let half_chatter = ((0.5 / step_m) as usize).max(1);
    let mut along: Vec<f32> = Vec::new();
    let mut chatter: Vec<f32> = Vec::new();
    let mut zones: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for i in 0..n {
        let col: Vec<f32> = rows.iter().map(|r| r[i]).collect();
        along.extend(detrend(&col, half_along));
        let fine = detrend(&col, half_chatter);
        let u = u_at(i).abs();
        // On the line, two metres off it, four metres off it.
        let zone = if u <= 1.0 {
            Some(0)
        } else if (1.5..2.5).contains(&u) {
            Some(1)
        } else if (3.5..4.5).contains(&u) {
            Some(2)
        } else {
            None
        };
        if let Some(z) = zone {
            zones[z].extend(fine.iter().copied());
        }
        chatter.extend(fine);
    }

    let mut walls_sorted = walls;
    walls_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let wall_deg = walls_sorted
        .get((walls_sorted.len() as f32 * 0.98) as usize)
        .copied()
        .unwrap_or(0.0);
    let mean = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
    let (a, b) = (rms(&across), rms(&along));
    Some(RutShape {
        across_rms_m: a,
        along_rms_m: b,
        anisotropy: if b > 1e-6 { a / b } else { 0.0 },
        floor_m: mean(&floors),
        wall_deg,
        grooves: mean(&counts),
        spacing_m: mean(&gaps),
        chatter_m: rms(&chatter),
        chatter_zones_m: [rms(&zones[0]), rms(&zones[1]), rms(&zones[2])],
    })
}

pub fn ridden(lap: &crate::trackline::Lap, g: &Grid) -> Option<RiddenStats> {
    let n = (2.0 * RIDDEN_REACH_M / RIDDEN_LATERAL_M) as usize + 1;
    let u_at = |i: usize| i as f32 * RIDDEN_LATERAL_M - RIDDEN_REACH_M;
    let inside: Vec<usize> = (0..n).filter(|i| u_at(*i).abs() <= RIDDEN_HALF_M).collect();
    if inside.len() < 8 {
        return None;
    }

    // Walk the lap the way its own records do: each segment states where it starts, so a
    // station is placed from its own segment rather than integrated from the one before.
    let mut cross: Vec<Cross> = Vec::new();
    let mut along: Vec<f32> = Vec::new();
    for seg in &lap.segments {
        let steps = ((seg.length / RIDDEN_STEP_M) as usize).max(1);
        for k in 0..steps {
            let d = k as f32 * seg.length / steps as f32;
            let (x, z, h) = if seg.radius == 0.0 {
                let (hx, hz) = crate::trackprog::heading_vector(seg.heading);
                (seg.x + d * hx, seg.z + d * hz, seg.heading)
            } else {
                let h = seg.heading + d / seg.radius;
                (
                    seg.x + seg.radius * (seg.heading.cos() - h.cos()),
                    seg.z + seg.radius * (h.sin() - seg.heading.sin()),
                    h,
                )
            };
            let (rx, rz) = crate::trackprog::right_vector(h);
            let p: Vec<f32> = (0..n)
                .map(|i| {
                    let u = u_at(i);
                    g.at(x + u * rx, z + u * rz)
                })
                .collect();
            // The line's own height, taken over the middle of it so a single groove doesn't
            // carry the whole lap's elevation.
            let mut band: Vec<f32> = (0..n)
                .filter(|i| u_at(*i).abs() <= 3.0)
                .map(|i| p[i])
                .collect();
            band.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            along.push(band[band.len() / 2]);
            cross.push(Cross { radius: seg.radius, p });
        }
    }
    if cross.len() < 32 {
        return None;
    }

    // ---- layout ----------------------------------------------------------
    let mut straight_m: Vec<f32> = lap
        .segments
        .iter()
        .filter(|s| !s.is_corner())
        .map(|s| s.length)
        .collect();
    let mut arc_radius_m: Vec<f32> =
        lap.segments.iter().filter(|s| s.is_corner()).map(|s| s.radius.abs()).collect();
    let arcs = arc_radius_m.len();
    let turns_all = lap.turns();
    let mut turn_deg: Vec<f32> = turns_all.iter().filter(|t| t.0 >= 25.0).map(|t| t.0).collect();
    let mut turn_radius_m: Vec<f32> =
        turns_all.iter().filter(|t| t.0 >= 25.0).map(|t| t.1).collect();
    let total_turn_deg = lap.segments.iter().map(|s| s.angle).sum();
    // Taken from the segments' own elevations rather than the ground under them: this is the
    // height the lap was *built* to, and it is what a rider climbs.
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for s in &lap.segments {
        lo = lo.min(s.elevation);
        hi = hi.max(s.elevation);
    }
    let lap_climb_m = if lo.is_finite() { hi - lo } else { 0.0 };
    let mut grade_deg: Vec<f32> = (0..along.len())
        .map(|i| {
            let j = (i + 1) % along.len();
            ((along[j] - along[i]).abs() / RIDDEN_STEP_M).atan().to_degrees()
        })
        .collect();

    // ---- ruts ------------------------------------------------------------
    let mut lines: Vec<f32> = Vec::new();
    let mut spacing: Vec<f32> = Vec::new();
    let mut depth_corner: Vec<f32> = Vec::new();
    let mut depth_straight: Vec<f32> = Vec::new();
    let win = (RUT_DETREND_M / RIDDEN_LATERAL_M / 2.0) as usize;
    for c in &cross {
        let base = smooth_ring(&c.p, win, false);
        let r: Vec<f32> = (0..n).map(|i| c.p[i] - base[i]).collect();
        let mut found: Vec<(f32, f32)> = Vec::new();
        for w in inside.windows(3) {
            let (a, i, b) = (w[0], w[1], w[2]);
            if !(r[i] <= r[a] && r[i] < r[b]) {
                continue;
            }
            let mut l = i;
            while l > inside[0] && r[l - 1] >= r[l] {
                l -= 1;
            }
            let mut k = i;
            while k < inside[inside.len() - 1] && r[k + 1] >= r[k] {
                k += 1;
            }
            let prom = (r[l] - r[i]).min(r[k] - r[i]);
            if prom >= RUT_PROMINENCE_M {
                found.push((u_at(i), prom));
            }
        }
        lines.push(found.len() as f32);
        for pair in found.windows(2) {
            spacing.push(pair[1].0 - pair[0].0);
        }
        let corner = c.radius != 0.0 && c.radius.abs() < RIDDEN_CORNER_R_M;
        for (_, prom) in &found {
            if corner {
                depth_corner.push(*prom);
            } else {
                depth_straight.push(*prom);
            }
        }
    }

    // ---- berms -----------------------------------------------------------
    // The centre of curvature lies to the rider's right of a positive-radius turn, so the
    // outside of one is its left. Getting this backwards reads a berm as an inside bank,
    // which is what the ground would look like if nobody had ridden it.
    let mut out_edge: Vec<f32> = Vec::new();
    let mut in_edge: Vec<f32> = Vec::new();
    let mut bank_deg: Vec<f32> = Vec::new();
    let at_u = |p: &[f32], u: f32| -> f32 {
        let i = ((u + RIDDEN_REACH_M) / RIDDEN_LATERAL_M).round().clamp(0.0, (n - 1) as f32);
        p[i as usize]
    };
    for c in &cross {
        if c.radius == 0.0 || c.radius.abs() >= RIDDEN_CORNER_R_M {
            continue;
        }
        let out = -c.radius.signum();
        let low = inside.iter().map(|i| c.p[*i]).fold(f32::INFINITY, f32::min);
        out_edge.push(at_u(&c.p, RIDDEN_HALF_M * out) - low);
        in_edge.push(at_u(&c.p, -RIDDEN_HALF_M * out) - low);

        let (mut sx, mut sy, mut sxy, mut sxx) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for i in &inside {
            let (x, y) = (u_at(*i) as f64, c.p[*i] as f64);
            sx += x;
            sy += y;
            sxy += x * y;
            sxx += x * x;
        }
        let m = inside.len() as f64;
        let slope = (sxy - sx * sy / m) / (sxx - sx * sx / m);
        bank_deg.push((slope as f32 * out).atan().to_degrees());
    }

    // ---- jumps -----------------------------------------------------------
    let base = smooth_ring(&along, (RIDDEN_BASE_M / RIDDEN_STEP_M / 2.0) as usize, true);
    let rel: Vec<f32> = (0..along.len()).map(|i| along[i] - base[i]).collect();
    let m = rel.len();
    let mut lips: Vec<(f32, f32, f32)> = Vec::new(); // at, height, steepest face
    for i in 0..m {
        let prev = rel[(i + m - 1) % m];
        let next = rel[(i + 1) % m];
        if !(rel[i] >= prev && rel[i] > next) {
            continue;
        }
        let (mut l, mut k) = (0usize, 0usize);
        while l < m / 2 && rel[(i + m - l - 1) % m] <= rel[(i + m - l) % m] {
            l += 1;
        }
        while k < m / 2 && rel[(i + k + 1) % m] <= rel[(i + k) % m] {
            k += 1;
        }
        let prom = (rel[i] - rel[(i + m - l) % m]).min(rel[i] - rel[(i + k) % m]);
        if prom < RIDDEN_LIP_M {
            continue;
        }
        // The steepest three metres either side, not the average over the whole face — a
        // twenty-metre ramp with a lip on the end averages out to nothing.
        let span = (3.0 / RIDDEN_STEP_M) as usize;
        let mut face = 0.0f32;
        for t in 0..(l + k) {
            let a = (i + m - l + t) % m;
            let b = (a + span) % m;
            face = face.max((along[b] - along[a]).abs() / 3.0);
        }
        lips.push((i as f32 * RIDDEN_STEP_M, prom, face.atan().to_degrees()));
    }
    lips.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // One jump, one lip: two samples four metres apart are the same takeoff seen twice.
    let mut kept: Vec<(f32, f32, f32)> = Vec::new();
    for l in lips {
        match kept.last_mut() {
            Some(p) if l.0 - p.0 < 4.0 => {
                if l.1 > p.1 {
                    *p = l;
                }
            }
            _ => kept.push(l),
        }
    }
    let km = (lap.length / 1000.0).max(1e-3);
    let mut lip_height_m: Vec<f32> = kept.iter().map(|l| l.1).collect();
    let mut big: Vec<f32> = lip_height_m.iter().copied().filter(|h| *h >= RIDDEN_BIG_LIP_M).collect();
    let big_n = big.len();
    let mut lip_face_deg: Vec<f32> = kept.iter().map(|l| l.2).collect();
    let mut lip_spacing_m: Vec<f32> =
        kept.windows(2).map(|w| w[1].0 - w[0].0).collect();

    Some(RiddenStats {
        lap_m: lap.length,
        segments: lap.segments.len(),
        arcs,
        straights: lap.segments.len() - arcs,
        straight_m: spread(&mut straight_m),
        arc_radius_m: spread(&mut arc_radius_m),
        turns: turn_deg.len(),
        turn_deg: spread(&mut turn_deg),
        turn_radius_m: spread(&mut turn_radius_m),
        total_turn_deg,
        lap_climb_m,
        grade_deg: spread(&mut grade_deg),
        rut_lines: lines.iter().sum::<f32>() / lines.len().max(1) as f32,
        rut_spacing_m: spread(&mut spacing),
        rut_depth_corner_m: spread(&mut depth_corner),
        rut_depth_straight_m: spread(&mut depth_straight),
        berm_outside_m: median(&mut out_edge),
        berm_inside_m: median(&mut in_edge),
        bank_deg: spread(&mut bank_deg),
        lips_per_km: kept.len() as f32 / km,
        big_lips_per_km: big_n as f32 / km,
        lip_height_m: spread(&mut lip_height_m),
        big_lip_height_m: spread(&mut big),
        lip_face_deg: spread(&mut lip_face_deg),
        lip_spacing_m: spread(&mut lip_spacing_m),
    })
}

fn median(v: &mut Vec<f32>) -> f32 {
    spread(v).p50
}

/// A moving average, optionally wrapping — a lap does, a cross-section doesn't.
fn smooth_ring(v: &[f32], half: usize, wrap: bool) -> Vec<f32> {
    let n = v.len();
    let half = half.max(1);
    (0..n)
        .map(|i| {
            let (mut sum, mut count) = (0.0f32, 0usize);
            for d in 0..=(2 * half) {
                let j = i as isize + d as isize - half as isize;
                let j = if wrap {
                    (j.rem_euclid(n as isize)) as usize
                } else if j < 0 || j >= n as isize {
                    continue;
                } else {
                    j as usize
                };
                sum += v[j];
                count += 1;
            }
            sum / count.max(1) as f32
        })
        .collect()
}

/// A height grid with metres on both axes, so a point on the centreline can be asked for its
/// ground height without the caller doing the arithmetic.
pub struct Grid {
    pub w: usize,
    pub h: usize,
    pub size_x: f32,
    pub size_z: f32,
    pub v: Vec<f32>,
}

impl Grid {
    /// Bilinear, because a rut is two samples wide and nearest-neighbour would alias it into
    /// and out of existence along its own length.
    pub fn at(&self, x: f32, z: f32) -> f32 {
        let c = (x / self.size_x * (self.w - 1) as f32).clamp(0.0, (self.w - 1) as f32 - 1e-3);
        let r = (z / self.size_z * (self.h - 1) as f32).clamp(0.0, (self.h - 1) as f32 - 1e-3);
        let (ci, ri) = (c as usize, r as usize);
        let (fc, fr) = (c - ci as f32, r - ri as f32);
        let g = |rr: usize, cc: usize| self.v[rr * self.w + cc];
        (g(ri, ci) * (1.0 - fc) + g(ri, ci + 1) * fc) * (1.0 - fr)
            + (g(ri + 1, ci) * (1.0 - fc) + g(ri + 1, ci + 1) * fc) * fr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_of_a_constant_is_the_constant() {
        let v = vec![7.0f32; 64];
        for r in [0usize, 1, 3, 9] {
            let out = box_blur(&v, 8, 8, r);
            assert!(out.iter().all(|x| (x - 7.0).abs() < 1e-4), "r={r}");
        }
    }

    #[test]
    fn max_filter_finds_the_peak_within_its_radius() {
        let (w, h) = (9usize, 9usize);
        let mut v = vec![0.0f32; w * h];
        v[4 * w + 4] = 5.0;
        let out = max_filter(&v, w, h, 2);
        // Every cell within two of the middle sees it, and nothing further out does.
        assert_eq!(out[4 * w + 4], 5.0);
        assert_eq!(out[2 * w + 4], 5.0);
        assert_eq!(out[6 * w + 6], 5.0);
        assert_eq!(out[1 * w + 4], 0.0);
        assert_eq!(out[4 * w + 1], 0.0);
    }

    #[test]
    fn distance_transform_measures_a_ribbon_in_metres() {
        // A ten-cell band down a 40-cell grid, at half a metre per cell: five metres across,
        // so the centre sits 2.5 m from an edge.
        let (w, h) = (40usize, 20usize);
        let mut mask = vec![false; w * h];
        for y in 0..h {
            for x in 15..25 {
                mask[y * w + x] = true;
            }
        }
        let d = distance_transform(&mask, w, h, 0.5);
        let mid = d[10 * w + 19];
        assert!((mid - 2.5).abs() < 0.35, "centre reads {mid} m, wanted 2.5");
        assert_eq!(d[10 * w + 5], 0.0);
    }

    #[test]
    fn largest_component_ignores_the_scatter() {
        let (w, h) = (10usize, 10usize);
        let mut mask = vec![false; w * h];
        for y in 2..8 {
            mask[y * w + 5] = true; // a six-cell line
        }
        mask[0] = true; // and a speck
        assert_eq!(largest_component(&mask, w, h), 6);
    }

    #[test]
    fn thinning_keeps_one_point_per_plateau() {
        let pts = vec![
            (10.0, 10.0, 1.0),
            (10.5, 10.2, 2.0), // same plateau, taller
            (40.0, 40.0, 1.5), // its own
        ];
        let kept = thin(pts, 4.0);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|p| p.2 == 2.0));
    }

    /// Measure every track under a folder. This is the corpus:
    ///
    /// ```text
    /// FROST_TRACKS="…/MX Bikes/mods/tracks" FROST_OUT=/tmp/corpus.json \
    ///   cargo test -- --ignored --nocapture corpus
    /// ```
    #[test]
    #[ignore = "needs real tracks — set FROST_TRACKS"]
    fn corpus() {
        let root = std::env::var("FROST_TRACKS").expect("set FROST_TRACKS to a tracks folder");
        let mut paths: Vec<std::path::PathBuf> = crate::linkwalk::walk_depth(Path::new(&root), 3)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.path().to_path_buf())
            .filter(|p| {
                p.extension()
                    .map(|e| e.eq_ignore_ascii_case("pkz"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "no .pkz tracks under {root}");

        let mut all = Vec::new();
        for p in &paths {
            let name = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            match analyse(p) {
                Ok(s) => {
                    match &s.corridor {
                        Some(c) => println!(
                            "{name:<34} {:>5.0}x{:<5.0}m  {:<10} {:>5.1}% area {:>4.0}% joined  \
                             w {:>4.1}/{:<4.1}m  len {:>5.0}m  slope p90 {:>4.1}° p99 {:>4.1}°  \
                             relief p90 {:>4.2}m  {:>4} lips  h p50 {:>4.2}m  gap p50 {:>5.1}m",
                            s.size_x_m, s.size_z_m, c.rule,
                            c.area_fraction * 100.0, c.largest_component_fraction * 100.0,
                            c.width_from_mean_m, c.width_from_tail_m, c.length_m,
                            c.slope_deg.p90, c.slope_deg.p99, c.feature_relief_m.p90,
                            c.lips, c.lip_height_m.p50, c.lip_spacing_m.p50,
                        ),
                        None => println!(
                            "{name:<34} {:>5.0}x{:<5.0}m  no corridor: {}",
                            s.size_x_m,
                            s.size_z_m,
                            s.corridor_note.as_deref().unwrap_or("")
                        ),
                    }
                    if let Some(r) = &s.ridden {
                        println!(
                            "{:<34} lap {:>5.0}m  {:>3} segs = {:>3} arcs + {:>2} straights  \
                             {:>2} turns  turn p50 {:>4.0}°  tightest R p50 {:>4.1}m  \
                             turning {:>5.0}°  climbs {:>4.1}m  grade p90 {:>4.1}°",
                            "  centreline", r.lap_m, r.segments, r.arcs, r.straights,
                            r.turns, r.turn_deg.p50, r.turn_radius_m.p50, r.total_turn_deg,
                            r.lap_climb_m, r.grade_deg.p90,
                        );
                        println!(
                            "{:<34} ruts {:>4.1} at {:>4.2}m  depth corner p50 {:>4.2} p90 {:>4.2}  \
                             straight p50 {:>4.2}  |  berm out {:>4.2}m in {:>4.2}m  \
                             bank p50 {:>4.1}° p90 {:>4.1}°",
                            "  ridden ground", r.rut_lines, r.rut_spacing_m.p50,
                            r.rut_depth_corner_m.p50, r.rut_depth_corner_m.p90,
                            r.rut_depth_straight_m.p50,
                            r.berm_outside_m, r.berm_inside_m, r.bank_deg.p50, r.bank_deg.p90,
                        );
                        println!(
                            "{:<34} {:>4.1} lips/km ({:>4.1} over 1m)  h p50 {:>4.2} p90 {:>4.2} max {:>4.2}  \
                             face p50 {:>4.1}° p90 {:>4.1}°  gap p50 {:>5.1}m",
                            "  jumps", r.lips_per_km, r.big_lips_per_km,
                            r.lip_height_m.p50, r.lip_height_m.p90, r.lip_height_m.max,
                            r.lip_face_deg.p50, r.lip_face_deg.p90, r.lip_spacing_m.p50,
                        );
                    }
                    all.push(s);
                }
                Err(e) => println!("{name:<34} skipped: {e:#}"),
            }
        }

        println!("\n{} of {} tracks measured", all.len(), paths.len());
        let mut pooled: Vec<(&str, Vec<f32>)> = vec![
            ("corridor width m", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.width_from_mean_m).collect()),
            ("corridor length m", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.length_m).collect()),
            ("slope p99 deg", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.slope_deg.p99).collect()),
            ("feature relief p90 m", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.feature_relief_m.p90).collect()),
            ("lip height p50 m", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.lip_height_m.p50).collect()),
            ("lip spacing p50 m", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.lip_spacing_m.p50).collect()),
            ("lips per km", all.iter().filter_map(|s| s.corridor.as_ref()).map(|c| c.lips_per_km).collect()),
        ];
        for (label, values) in &mut pooled {
            let s = spread(values);
            println!("{label:<22} p10 {:>7.2}  p50 {:>7.2}  p90 {:>7.2}  over {} tracks", s.p10, s.p50, s.p90, s.count);
        }

        if let Ok(out) = std::env::var("FROST_OUT") {
            std::fs::write(&out, serde_json::to_vec_pretty(&all).unwrap()).unwrap();
            println!("wrote {out}");
        }
    }

    /// The ground across the riding line at a track's tightest corners, drawn.
    ///
    /// Percentiles say how deep a groove is. They cannot say what *shape* it is, and shape is
    /// the whole question when the complaint is that a corner rides like a trench: a cut with
    /// nothing beside it and the same cut with a wall on its outer side measure the same
    /// prominence and ride nothing alike. This prints the profile itself, detrended exactly
    /// the way [`ridden`] detrends it before counting grooves, so a generated corner can be
    /// held against a published one.
    ///
    /// ```text
    /// Where a published grid's stalls actually are: its `long`/`lat` walked along the lap,
    /// against the start line's own start.
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn published_stalls_in_world() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let rdf = names.iter().find(|n| n.to_lowercase().ends_with(".rdf")).unwrap();
        let text = String::from_utf8_lossy(&crate::track::read_entry(path, rdf).unwrap()).to_string();

        let entry = crate::track::heightfield_entries(&names).into_iter().next().unwrap();
        let bytes = crate::track::read_entry(path, &entry).unwrap();
        let layout = crate::heightfield::probe(&bytes, None).unwrap();
        let at = layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let lap = crate::trackline::read(&bytes[at..]).expect("a lap");
        let prog = crate::trackprog::TrackProgram {
            name: String::new(), author: String::new(), location: String::new(),
            terrain: crate::trackprog::Terrain {
                size_x: 1000.0, size_z: 1000.0, samples: 513, scale: 100.0,
                relief: Default::default(), surface: Default::default(),
                wear: crate::trackprog::default_wear(),
            },
            start: crate::trackprog::Start { x: lap.start.0, z: lap.start.1, angle: lap.heading },
            segments: lap.program_segments(), width: 12.0, features: Vec::new(),
            blend: 1.2, elevation: Vec::new(),
        };
        let st = prog.stations(1.0);
        let place = |long: f32, lat: f32| -> (f32, f32) {
            let q = st.iter().min_by(|a, b| (a.s - long).abs().total_cmp(&(b.s - long).abs())).unwrap();
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            (q.x + rx * lat, q.z + rz * lat)
        };
        // The grid's own anchor, and the first three stalls read as lap coordinates.
        let val = |key: &str| -> Option<f32> {
            text.lines().find_map(|l| l.trim().strip_prefix(&format!("{key} = "))).and_then(|v| v.parse().ok())
        };
        println!("  grid anchor ({:.1}, {:.1}) angle {:.0}", val("posx").unwrap_or(0.0), val("posz").unwrap_or(0.0), val("angle").unwrap_or(0.0));
        // The grid's own stalls, read straight out of the block rather than by scanning for
        // the word "stall" — the pit lane's are called `start_stall` and come first.
        if let Some(gi) = text.find("starting_grid") {
            let tail = &text[gi..];
            let mut seen = 0;
            let mut it = tail.lines().map(|l| l.trim()).peekable();
            while let Some(l) = it.next() {
                if l.starts_with("stall") && !l.starts_with("start_stall") && seen < 4 {
                    let (mut long, mut lat, mut ang) = (0.0f32, 0.0f32, 0.0f32);
                    for _ in 0..5 {
                        match it.next() {
                            Some(v) if v.starts_with("long = ") => long = v[7..].parse().unwrap_or(0.0),
                            Some(v) if v.starts_with("lat = ") => lat = v[6..].parse().unwrap_or(0.0),
                            Some(v) if v.starts_with("angle = ") => ang = v[8..].parse().unwrap_or(0.0),
                            Some("}") => break,
                            _ => {}
                        }
                    }
                    let (x, z) = place(long, lat);
                    let q = st.iter().min_by(|a, b| (a.s - long).abs().total_cmp(&(b.s - long).abs())).unwrap();
                    println!(
                        "  grid stall {seen}: long {long:.1} lat {lat:.1} angle {ang:.0} -> ({x:.1}, {z:.1}); \
                         lap heading there {:.0}, stall angle - lap heading {:.0}",
                        q.heading.to_degrees(),
                        ang - q.heading.to_degrees(),
                    );
                    seen += 1;
                }
            }
        }
        let mut it = text.lines().map(|l| l.trim());
        let mut shown = 0;
        while let Some(l) = it.next() {
            if l.starts_with("stall") && shown < 3 {
                let (mut long, mut lat) = (0.0, 0.0);
                for _ in 0..5 {
                    match it.next() {
                        Some(v) if v.starts_with("long = ") => long = v[7..].parse().unwrap_or(0.0),
                        Some(v) if v.starts_with("lat = ") => lat = v[6..].parse().unwrap_or(0.0),
                        Some("}") => break,
                        _ => {}
                    }
                }
                let (x, z) = place(long, lat);
                println!("  stall {shown}: long {long:.1} lat {lat:.1} -> ({x:.1}, {z:.1})");
                shown += 1;
            }
        }
    }

    /// Everything a published track ships, by name and size — what files it carries at all.
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn published_files() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        for n in crate::track::entry_names(path).unwrap() {
            println!("  {n}");
        }
    }

    /// The ground across a published start straight: how wide the flat pad is, and whether it
    /// runs into the lap or stops short of it.
    ///
    /// ```text
    /// FROST_TRACK=…/track.pkz cargo test -- --ignored --nocapture published_start_ground
    /// ```
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn published_start_ground() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let entry = crate::track::heightfield_entries(&names).into_iter().next().unwrap();
        let bytes = crate::track::read_entry(path, &entry).unwrap();
        let layout = crate::heightfield::probe(&bytes, None).unwrap();
        let mps = layout.metres_per_sample.expect("a track in metres");
        let (gw, gh, heights) =
            crate::heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        let (gw, gh) = (gw as usize, gh as usize);
        let at = |x: f32, z: f32| -> f32 {
            let (ix, iz) = ((x / mps) as usize, (z / mps) as usize);
            heights[iz.min(gh - 1) * gw + ix.min(gw - 1)]
        };

        // The lap, and the start line beside it.
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let block = &bytes[block_at..];
        let lap = crate::trackline::read(block).expect("a lap");
        let prog = crate::trackprog::TrackProgram {
            name: String::new(), author: String::new(), location: String::new(),
            terrain: crate::trackprog::Terrain {
                size_x: mps * (gw - 1) as f32, size_z: mps * (gh - 1) as f32,
                samples: 513, scale: 100.0,
                relief: Default::default(), surface: Default::default(),
                wear: Default::default(),
            },
            start: crate::trackprog::Start { x: lap.start.0, z: lap.start.1, angle: lap.heading },
            segments: lap.program_segments(), width: 12.0, features: Vec::new(),
            blend: 1.2, elevation: Vec::new(),
        };
        let lap_st = prog.stations(1.0);

        // Find the start line the way `second_line` does.
        let table = crate::track::material_table_offset(block).unwrap();
        let u32_at = |o: usize| u32::from_le_bytes(block[o..o + 4].try_into().unwrap());
        let f32_at = |o: usize| f32::from_le_bytes(block[o..o + 4].try_into().unwrap());
        let materials = u32_at(table) as usize;
        let after = table + 4 + materials * 52 + 4 + u32_at(table + 4 + materials * 52) as usize * 60;
        let mut line: Option<(f32, f32, f32, Vec<(f32, f32, f32)>)> = None;
        let mut k = after;
        while k + 16 < block.len() {
            let (x, z, ang) = (f32_at(k), f32_at(k + 4), f32_at(k + 8));
            let n = u32_at(k + 12) as usize;
            let sane = (0.0..4000.0).contains(&x) && (0.0..4000.0).contains(&z)
                && ang.abs() <= 720.0 && n > 0 && n < 64 && k + 16 + n * 60 <= block.len();
            if sane {
                let (mut total, mut ok, mut segs) = (0.0f32, true, Vec::new());
                for i in 0..n {
                    let o = k + 16 + i * 60;
                    let (len, r, a, s0) = (f32_at(o + 4), f32_at(o + 8), f32_at(o + 12), f32_at(o + 20));
                    if !(0.0..2000.0).contains(&len) || (s0 - total).abs() > 0.5 {
                        ok = false;
                        break;
                    }
                    total += len;
                    segs.push((len, r, a));
                }
                if ok && total > 20.0 {
                    line = Some((x, z, ang, segs));
                    break;
                }
            }
            k += 4;
        }
        let Some((sx, sz, sang, segs)) = line else {
            println!("  no start line");
            return;
        };
        let walk = crate::trackprog::TrackProgram {
            start: crate::trackprog::Start { x: sx, z: sz, angle: sang },
            segments: segs
                .iter()
                .map(|(len, r, a)| {
                    if *r == 0.0 || *a == 0.0 {
                        crate::trackprog::Segment::Straight { length: *len, rise: 0.0 }
                    } else {
                        crate::trackprog::Segment::Arc { radius: *r, angle: a.abs(), rise: 0.0 }
                    }
                })
                .collect(),
            ..prog.clone()
        };

        println!("  along  flat±   to the lap   step at the join");
        for q in walk.stations(1.0).iter().step_by(10) {
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            // How far the ground stays flat either side: walk out until it tilts hard.
            let mut reach = [0.0f32; 2];
            for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
                let mut last = at(q.x, q.z);
                for m in 1..60 {
                    let t = m as f32;
                    let (x, z) = (q.x + rx * t * side, q.z + rz * t * side);
                    if x < 1.0 || z < 1.0 || x > prog.terrain.size_x - 1.0 || z > prog.terrain.size_z - 1.0 {
                        break;
                    }
                    let h = at(x, z);
                    if (h - last).abs() > 0.55 {
                        break;
                    }
                    last = h;
                    reach[i] = t;
                }
            }
            let (near, _) = lap_st.iter().fold((f32::MAX, 0.0f32), |b, p| {
                let d = ((p.x - q.x).powi(2) + (p.z - q.z).powi(2)).sqrt();
                if d < b.0 { (d, p.s) } else { b }
            });
            println!(
                "  {:>5.0}m  {:>4.0}/{:<4.0} {:>8.0} m   {}",
                q.s, reach[0], reach[1], near,
                if near < reach[0].max(reach[1]) + 2.0 { "flat right up to it" } else { "" },
            );
        }
    }

    /// What a published track's ruts are shaped like.
    ///
    /// ```text
    /// FROST_TRACK=…/indiana.pkz cargo test -p mxb-app --bin mxb-app -- --ignored --nocapture rut_shape_of
    /// ```
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn rut_shape_of() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = track::entry_names(path).unwrap();
        let entry = track::heightfield_entries(&names)
            .into_iter()
            .next()
            .expect("a heightfield");
        let bytes = track::read_entry(path, &entry).unwrap();
        let layout = heightfield::probe(&bytes, None).expect("a terrain grid");
        let mps_src = layout.metres_per_sample.expect("a stated footprint");
        let size_x = mps_src * (layout.width.max(2) - 1) as f32;
        let size_z = mps_src * (layout.height.max(2) - 1) as f32;
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let block = bytes.get(block_at..).unwrap_or(&[]);
        let lap = crate::trackline::read(block).expect("a centreline");
        let (fw, fh, v) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        let g = &Grid { w: fw as usize, h: fh as usize, size_x, size_z, v };
        let step = 0.5f32;
        let stations: Vec<(f32, f32, f32)> =
            lap.stations(step).into_iter().map(|st| (st.x, st.z, st.heading)).collect();
        let r = super::rut_shape(&stations, step, g).expect("a rut shape");
        println!("  {} stations over {:.0} m", stations.len(), stations.len() as f32 * step);
        println!("  across {:.3} m rms   along {:.3} m rms   anisotropy {:.2}", r.across_rms_m, r.along_rms_m, r.anisotropy);
        println!("  floor {:.2} m   wall {:.0} deg   {:.1} grooves at {:.2} m   chatter {:.3} m", r.floor_m, r.wall_deg, r.grooves, r.spacing_m, r.chatter_m);
        println!("  chatter on the line {:.3} m   2 m off {:.3}   4 m off {:.3}", r.chatter_zones_m[0], r.chatter_zones_m[1], r.chatter_zones_m[2]);
    }


    /// Four corners from a published track: their shape, their bumps, and the ground on them.
    ///
    /// Writes one folder per track — a params JSON and, per corner, a height patch and the
    /// ground as the game paints it — for `scripts/corner-atlas.py` to draw.
    ///
    /// ```text
    /// FROST_TRACK=…/indiana.pkz FROST_OUT=/tmp/atlas/indiana \
    ///   cargo test --bin mxb-app -- --ignored --nocapture corner_atlas
    /// ```
    #[test]
    #[ignore = "needs a track — set FROST_TRACK and FROST_OUT"]
    fn corner_atlas() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        std::fs::create_dir_all(&out).unwrap();
        let path = std::path::Path::new(&var);

        let names = track::entry_names(path).unwrap();
        let entry = track::heightfield_entries(&names).into_iter().next().expect("a heightfield");
        let bytes = track::read_entry(path, &entry).unwrap();
        let layout = heightfield::probe(&bytes, None).expect("a terrain grid");
        let mps_src = layout.metres_per_sample.expect("a stated footprint");
        let size_x = mps_src * (layout.width.max(2) - 1) as f32;
        let size_z = mps_src * (layout.height.max(2) - 1) as f32;
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let lap = crate::trackline::read(bytes.get(block_at..).unwrap_or(&[])).expect("a centreline");
        // Full resolution: a corner's bumps are the point, and a downsample is exactly what
        // would smooth them away.
        let (fw, fh, v) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        let g = Grid { w: fw as usize, h: fh as usize, size_x, size_z, v };

        // The ground stack, for the colour/texture/mask half of the question.
        let map_name = names.iter().find(|n| n.to_lowercase().ends_with(".map"));
        let layers = match map_name {
            Some(n) => {
                let mb = track::read_entry(path, n).unwrap();
                crate::map::ground_layers(&mb)
            }
            None => Vec::new(),
        };
        println!("{} — {:.0}x{:.0} m, {}x{} samples, {} ground layers",
            entry, size_x, size_z, fw, fh, layers.len());

        let step = 0.25f32;
        let all = lap.stations(step);
        let runs = crate::trackprog::corner_runs(&lap.program_segments());
        println!("lap {:.0} m, {} segments, {} corners", lap.length, lap.segments.len(), runs.len());

        // The four that bend furthest — the ones a rider would name — put back into lap order
        // so the atlas reads like a lap.
        let mut ranked = runs.clone();
        ranked.sort_by(|x, y| y.degrees.total_cmp(&x.degrees));
        let mut chosen: Vec<(f32, f32, f32)> =
            ranked.into_iter().take(4).map(|r| (r.degrees, r.start_m, r.end_m)).collect();
        chosen.sort_by(|x, y| x.1.total_cmp(&y.1));

        let mut json = String::from("{\n");
        json.push_str(&format!("  \"track\": {:?},\n", path.file_stem().unwrap().to_string_lossy()));
        json.push_str(&format!("  \"lapLengthM\": {:.1},\n  \"sizeXm\": {:.1},\n  \"sizeZm\": {:.1},\n", lap.length, size_x, size_z));
        json.push_str(&format!("  \"metresPerSample\": {:.4},\n", mps_src));
        json.push_str("  \"groundLayers\": [\n");
        for (i, l) in layers.iter().enumerate() {
            let tile_m = size_x / l.tile_u.max(0.001);
            json.push_str(&format!(
                "    {{\"i\": {i}, \"sheet\": {:?}, \"px\": [{}, {}], \"tileU\": {:.1}, \"tileV\": {:.1}, \"tileMetres\": {:.2}, \"mask\": {}}}{}\n",
                l.sheet.name, l.sheet.width, l.sheet.height, l.tile_u, l.tile_v, tile_m,
                match &l.mask { Some(m) => format!("[{}, {}]", m.width, m.height), None => "null".into() },
                if i + 1 < layers.len() { "," } else { "" }));
        }
        json.push_str("  ],\n  \"corners\": [\n");

        for (n, &(deg, a, b)) in chosen.iter().enumerate() {
            let sel: Vec<&crate::trackline::Station> =
                all.iter().filter(|s| s.at >= a && s.at <= b).collect();
            if sel.len() < 8 {
                continue;
            }
            let stations: Vec<(f32, f32, f32)> =
                sel.iter().map(|s| (s.x, s.z, s.heading)).collect();
            let shape = super::rut_shape(&stations, step, &g);
            let sweep = super::section_sweep(&stations, &g).unwrap_or(f32::NAN);

            // The arcs the builder actually typed, which is the shape of the turn.
            let arcs: Vec<&crate::trackline::LineSegment> =
                lap.segments.iter().filter(|s| s.at >= a - 0.5 && s.at < b + 0.5).collect();
            let radii: Vec<f32> = arcs.iter().filter(|s| s.radius != 0.0).map(|s| s.radius).collect();
            let len_m = b - a;
            let mean_r = if radii.is_empty() { 0.0 } else { radii.iter().sum::<f32>() / radii.len() as f32 };
            let tightest = radii.iter().cloned().fold(f32::INFINITY, |m, r| m.min(r.abs()));
            let widest = radii.iter().cloned().fold(0.0f32, |m, r| m.max(r.abs()));

            // Elevation over the turn, off the heightfield under the line.
            let hs: Vec<f32> = sel.iter().filter_map(|s| Some(g.at(s.x, s.z))).collect();
            let (hmin, hmax) = hs.iter().fold((f32::MAX, f32::MIN), |(a, b), &h| (a.min(h), b.max(h)));

            let patch = write_corner_patch(&out, n, &sel, &g, &layers, size_x, size_z);

            json.push_str(&format!(
                "    {{\"n\": {n}, \"atM\": {a:.1}, \"endM\": {b:.1}, \"lengthM\": {len_m:.1}, \"turnDeg\": {deg:.1},\n",
            ));
            json.push_str(&format!(
                "      \"arcs\": {}, \"radiusMeanM\": {mean_r:.1}, \"radiusTightestM\": {tightest:.1}, \"radiusWidestM\": {widest:.1},\n",
                arcs.iter().filter(|s| s.radius != 0.0).count(),
            ));
            json.push_str(&format!(
                "      \"direction\": {:?}, \"riseM\": {:.2}, \"sweep\": {:.3}, \"patch\": {:?},\n",
                if mean_r > 0.0 { "right" } else { "left" },
                hmax - hmin,
                sweep,
                patch,
            ));
            match shape {
                Some(r) => {
                    json.push_str(&format!(
                        "      \"grooves\": {:.2}, \"spacingM\": {:.2}, \"floorM\": {:.2}, \"wallDeg\": {:.0},\n",
                        r.grooves, r.spacing_m, r.floor_m, r.wall_deg,
                    ));
                    json.push_str(&format!(
                        "      \"acrossRmsM\": {:.3}, \"alongRmsM\": {:.3}, \"anisotropy\": {:.2}, \"chatterM\": {:.3},\n",
                        r.across_rms_m, r.along_rms_m, r.anisotropy, r.chatter_m,
                    ));
                    json.push_str(&format!(
                        "      \"chatterZonesM\": [{:.3}, {:.3}, {:.3}]}}{}\n",
                        r.chatter_zones_m[0], r.chatter_zones_m[1], r.chatter_zones_m[2],
                        if n + 1 < chosen.len() { "," } else { "" },
                    ));
                }
                None => json.push_str(&format!("      \"grooves\": null}}{}\n", if n + 1 < chosen.len() { "," } else { "" })),
            }
            println!(
                "  corner {n}: {deg:.0} deg over {len_m:.0} m, {} arcs, r {tightest:.0}–{widest:.0} m, sweep {sweep:.2}",
                radii.len()
            );
        }
        json.push_str("  ]\n}\n");
        std::fs::write(out.join("corners.json"), &json).unwrap();
        println!("wrote {}", out.join("corners.json").display());
    }



    /// One corner's patch: heights, and the ground the game paints on them.
    ///
    /// The patch is axis-aligned around the corner's own bounding box with a margin, at the
    /// heightfield's own resolution — the bumps are the subject, so nothing is resampled.
    fn write_corner_patch(
        out: &std::path::Path,
        n: usize,
        sel: &[&crate::trackline::Station],
        g: &Grid,
        layers: &[crate::map::GroundLayer],
        size_x: f32,
        size_z: f32,
    ) -> String {
        const MARGIN_M: f32 = 12.0;
        let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for s in sel {
            x0 = x0.min(s.x); x1 = x1.max(s.x);
            z0 = z0.min(s.z); z1 = z1.max(s.z);
        }
        x0 -= MARGIN_M; x1 += MARGIN_M; z0 -= MARGIN_M; z1 += MARGIN_M;

        let mps = size_x / (g.w.max(2) - 1) as f32;
        let pw = (((x1 - x0) / mps).ceil() as usize).clamp(16, 2048);
        let ph = (((z1 - z0) / mps).ceil() as usize).clamp(16, 2048);

        let mut buf: Vec<u8> = Vec::with_capacity(32 + pw * ph * 7 + sel.len() * 8);
        buf.extend_from_slice(b"FRCA");
        buf.extend_from_slice(&(pw as u32).to_le_bytes());
        buf.extend_from_slice(&(ph as u32).to_le_bytes());
        buf.extend_from_slice(&x0.to_le_bytes());
        buf.extend_from_slice(&z0.to_le_bytes());
        buf.extend_from_slice(&mps.to_le_bytes());

        // Heights, then the composited ground as RGB.
        let mut rgb: Vec<u8> = Vec::with_capacity(pw * ph * 3);
        for j in 0..ph {
            for i in 0..pw {
                let x = x0 + i as f32 * mps;
                let z = z0 + j as f32 * mps;
                buf.extend_from_slice(&g.at(x, z).to_le_bytes());
                let [r, gg, b] = ground_at(layers, x, z, size_x, size_z);
                rgb.push(r); rgb.push(gg); rgb.push(b);
            }
        }
        buf.extend_from_slice(&rgb);
        buf.extend_from_slice(&(sel.len() as u32).to_le_bytes());
        for s in sel {
            buf.extend_from_slice(&s.x.to_le_bytes());
            buf.extend_from_slice(&s.z.to_le_bytes());
        }
        let name = format!("corner{n}.bin");
        std::fs::write(out.join(&name), &buf).unwrap();
        name
    }

    /// The ground stack sampled at one point — base sheet, then every layer its mask lets through.
    fn ground_at(
        layers: &[crate::map::GroundLayer],
        x: f32,
        z: f32,
        size_x: f32,
        size_z: f32,
    ) -> [u8; 3] {
        let mut out = [90u8, 80, 70];
        for (i, l) in layers.iter().enumerate() {
            let cover = match &l.mask {
                None => 255u8, // the base layer covers everything
                Some(m) => {
                    let mx = ((x / size_x) * m.width as f32) as i64;
                    let mz = ((z / size_z) * m.height as f32) as i64;
                    if mx < 0 || mz < 0 || mx >= m.width as i64 || mz >= m.height as i64 {
                        0
                    } else {
                        m.coverage[mz as usize * m.width as usize + mx as usize]
                    }
                }
            };
            if cover == 0 && i > 0 {
                continue;
            }
            let (sw, sh) = (l.sheet.width.max(1) as f32, l.sheet.height.max(1) as f32);
            let u = ((x / size_x) * l.tile_u).rem_euclid(1.0) * sw;
            let v = ((z / size_z) * l.tile_v).rem_euclid(1.0) * sh;
            let si = ((v as usize).min(sh as usize - 1) * sw as usize + (u as usize).min(sw as usize - 1)) * 4;
            if si + 2 >= l.sheet.rgba.len() {
                continue;
            }
            let a = cover as f32 / 255.0;
            for c in 0..3 {
                out[c] = (out[c] as f32 * (1.0 - a) + l.sheet.rgba[si + c] as f32 * a) as u8;
            }
        }
        out
    }

    /// A published track's race data: where it puts its grid, and in what coordinates.
    ///
    /// ```text
    /// FROST_TRACK=…/indiana.pkz cargo test -- --ignored --nocapture published_rdf
    /// ```
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn published_rdf() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let rdf = names
            .iter()
            .find(|n| n.to_lowercase().ends_with(".rdf"))
            .expect("a .rdf");
        let bytes = crate::track::read_entry(path, rdf).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        // The grid block and the first few of its stalls, plus anything that names a position.
        let mut in_grid = false;
        let mut stalls = 0usize;
        for line in text.lines() {
            let t = line.trim();
            if t == "starting_grid" {
                in_grid = true;
            }
            if in_grid {
                if t.starts_with("stall") {
                    stalls += 1;
                }
                if stalls <= 3 {
                    println!("  {t}");
                }
                if stalls > 40 {
                    in_grid = false;
                }
            } else if t.starts_with("30secondsboard") || t.starts_with("pit_lane") {
                println!("  {t}");
            }
        }
    }

    /// FROST_TRACK=…/indiana.pkz cargo test -- --ignored --nocapture cross_sections
    /// ```
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn cross_sections() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track .pkz/folder");
        let path = Path::new(&var);
        let names = track::entry_names(path).unwrap();
        let entry = track::heightfield_entries(&names)
            .into_iter()
            .next()
            .expect("a heightfield");
        let bytes = track::read_entry(path, &entry).unwrap();
        let layout = heightfield::probe(&bytes, None).expect("a terrain grid");
        let mps_src = layout.metres_per_sample.expect("a stated footprint");
        let size_x = mps_src * (layout.width.max(2) - 1) as f32;
        let size_z = mps_src * (layout.height.max(2) - 1) as f32;
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let block = bytes.get(block_at..).unwrap_or(&[]);
        let lap = crate::trackline::read(block).expect("a centreline");
        let (fw, fh, v) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        let g = Grid { w: fw as usize, h: fh as usize, size_x, size_z, v };

        let n = (2.0 * RIDDEN_REACH_M / RIDDEN_LATERAL_M) as usize + 1;
        let u_at = |i: usize| i as f32 * RIDDEN_LATERAL_M - RIDDEN_REACH_M;
        let win = (RUT_DETREND_M / RIDDEN_LATERAL_M / 2.0) as usize;

        // Every station on the lap, with the radius it was drawn at, so the tightest ground
        // can be picked out of it afterwards.
        let mut cross: Vec<(f32, f32, Vec<f32>)> = Vec::new();
        let mut s_at = 0.0f32;
        for seg in &lap.segments {
            let steps = ((seg.length / RIDDEN_STEP_M) as usize).max(1);
            for k in 0..steps {
                let d = k as f32 * seg.length / steps as f32;
                let (x, z, h) = if seg.radius == 0.0 {
                    let (hx, hz) = crate::trackprog::heading_vector(seg.heading);
                    (seg.x + d * hx, seg.z + d * hz, seg.heading)
                } else {
                    let h = seg.heading + d / seg.radius;
                    (
                        seg.x + seg.radius * (seg.heading.cos() - h.cos()),
                        seg.z + seg.radius * (h.sin() - seg.heading.sin()),
                        h,
                    )
                };
                let (rx, rz) = crate::trackprog::right_vector(h);
                let p: Vec<f32> = (0..n).map(|i| g.at(x + u_at(i) * rx, z + u_at(i) * rz)).collect();
                cross.push((seg.radius, s_at + d, p));
            }
            s_at += seg.length;
        }

        // The tightest corners on the lap, spread round it rather than eight samples of the
        // same hairpin: one station per corner, taken where the radius is smallest.
        let mut picks: Vec<usize> = Vec::new();
        let mut order: Vec<usize> = (0..cross.len())
            .filter(|i| cross[*i].0 != 0.0 && cross[*i].0.abs() < RIDDEN_CORNER_R_M)
            .collect();
        order.sort_by(|a, b| cross[*a].0.abs().partial_cmp(&cross[*b].0.abs()).unwrap());
        for i in order {
            if picks.iter().all(|p| (cross[*p].1 - cross[i].1).abs() > 60.0) {
                picks.push(i);
            }
            if picks.len() == 6 {
                break;
            }
        }
        picks.sort_by(|a, b| cross[*a].1.partial_cmp(&cross[*b].1).unwrap());

        println!(
            "\n{}  —  {:.0} m lap, ground at {:.3} m a sample\n",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            s_at,
            size_x / (g.w.max(2) - 1) as f32,
        );

        // The profile as a picture. Detrended over the same six metres the rut count uses, so
        // what is drawn is what gets counted — the camber is taken out and what is left is
        // the grooves and whatever stands between them.
        const ROWS: usize = 15;
        const SPAN_M: f32 = 0.30; // half the height of the plot
        let mut all: Vec<f32> = Vec::new();
        for &i in &picks {
            let (radius, s, p) = &cross[i];
            let base = smooth_ring(p, win, false);
            let r: Vec<f32> = (0..n).map(|k| p[k] - base[k]).collect();
            // Only the ridden half of the profile is worth drawing; past that it is field.
            let cols: Vec<usize> = (0..n).filter(|k| u_at(*k).abs() <= 6.0).collect();
            let mut grid = vec![b' '; ROWS * cols.len()];
            for (cx, &k) in cols.iter().enumerate() {
                let t = ((SPAN_M - r[k]) / (2.0 * SPAN_M) * ROWS as f32).round();
                let row = t.clamp(0.0, ROWS as f32 - 1.0) as usize;
                grid[row * cols.len() + cx] = b'#';
            }
            let lo = r[cols[0]..=cols[cols.len() - 1]].iter().fold(f32::MAX, |a, b| a.min(*b));
            let hi = r[cols[0]..=cols[cols.len() - 1]].iter().fold(f32::MIN, |a, b| a.max(*b));
            println!(
                "  {:>5.0} m round, R {:>5.1} m   relief {:+.2} to {:+.2} m, {:.2} m peak to peak",
                s,
                radius.abs(),
                lo,
                hi,
                hi - lo
            );
            for row in 0..ROWS {
                let h = SPAN_M - (row as f32 + 0.5) / ROWS as f32 * 2.0 * SPAN_M;
                println!(
                    "    {:+.2} |{}",
                    h,
                    String::from_utf8_lossy(&grid[row * cols.len()..(row + 1) * cols.len()])
                );
            }
            println!("          |{}", "-".repeat(cols.len()));
            let mut axis = vec![b' '; cols.len()];
            axis[..4].copy_from_slice(b"-6 m");
            axis[cols.len() / 2] = b'0';
            axis[cols.len() - 4..].copy_from_slice(b"+6 m");
            println!("           {}", String::from_utf8_lossy(&axis));

            // And the grooves the rut count would find here, so the picture and the number
            // are the same measurement.
            let inside: Vec<usize> = (0..n).filter(|k| u_at(*k).abs() <= RIDDEN_HALF_M).collect();
            let mut found: Vec<(f32, f32)> = Vec::new();
            for w in inside.windows(3) {
                let (a, k, b) = (w[0], w[1], w[2]);
                if !(r[k] <= r[a] && r[k] < r[b]) {
                    continue;
                }
                let mut l = k;
                while l > inside[0] && r[l - 1] >= r[l] {
                    l -= 1;
                }
                let mut m = k;
                while m < inside[inside.len() - 1] && r[m + 1] >= r[m] {
                    m += 1;
                }
                let prom = (r[l] - r[k]).min(r[m] - r[k]);
                if prom >= RUT_PROMINENCE_M {
                    found.push((u_at(k), prom));
                }
            }
            let listed: Vec<String> =
                found.iter().map(|(u, d)| format!("{u:+.1}m/{d:.2}")).collect();
            println!("           {} grooves: {}\n", found.len(), listed.join("  "));
            all.extend(found.iter().map(|(_, d)| *d));
        }
        let s = spread(&mut all);
        println!(
            "  over {} grooves in {} corners: p10 {:.2}  p50 {:.2}  p90 {:.2}  max {:.2} m",
            s.count, picks.len(), s.p10, s.p50, s.p90, s.max
        );
        // ---- along the line -------------------------------------------------
        //
        // Across the track says what a corner's shape is. It says nothing about how the
        // ground feels *under* the wheels, which is a different measurement entirely — a
        // corner can be smooth across and still buzz, and buzz is what a rider calls rough.
        // So the centreline's own height, detrended over the same six metres, and the size of
        // what is left.
        let mid = n / 2;
        let line: Vec<f32> = cross.iter().map(|c| c.2[mid]).collect();
        let along_win = (RUT_DETREND_M / RIDDEN_STEP_M / 2.0) as usize;
        let base = smooth_ring(&line, along_win, true);
        let res: Vec<f32> = (0..line.len()).map(|i| line[i] - base[i]).collect();
        let rms = |v: &[f32]| -> f32 {
            if v.is_empty() {
                return 0.0;
            }
            (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
        };
        let corner: Vec<f32> = (0..res.len())
            .filter(|i| cross[*i].0 != 0.0 && cross[*i].0.abs() < RIDDEN_CORNER_R_M)
            .map(|i| res[i])
            .collect();
        let straight: Vec<f32> = (0..res.len())
            .filter(|i| cross[*i].0 == 0.0)
            .map(|i| res[i])
            .collect();
        println!(
            "  along the line, {:.0} m detrended: rms {:.3} m overall, {:.3} in corners, \
             {:.3} on straights",
            RUT_DETREND_M,
            rms(&res),
            rms(&corner),
            rms(&straight),
        );

        // And the strip through the tightest corner, drawn the same way as the sections.
        if let Some(&i0) = picks.first() {
            let span = (60.0 / RIDDEN_STEP_M) as usize;
            let from = i0.saturating_sub(span / 2);
            let cols: Vec<usize> = (from..(from + span).min(res.len())).collect();
            let mut grid = vec![b' '; ROWS * cols.len()];
            for (cx, &k) in cols.iter().enumerate() {
                let t = ((SPAN_M - res[k]) / (2.0 * SPAN_M) * ROWS as f32).round();
                let row = t.clamp(0.0, ROWS as f32 - 1.0) as usize;
                grid[row * cols.len() + cx] = b'#';
            }
            println!(
                "\n  60 m of centreline through the {:.1} m corner at {:.0} m",
                cross[i0].0.abs(),
                cross[i0].1
            );
            for row in 0..ROWS {
                let h = SPAN_M - (row as f32 + 0.5) / ROWS as f32 * 2.0 * SPAN_M;
                println!(
                    "    {:+.2} |{}",
                    h,
                    String::from_utf8_lossy(&grid[row * cols.len()..(row + 1) * cols.len()])
                );
            }
            println!("          |{}", "-".repeat(cols.len()));
        }

    }


    /// Every jump on a lap as a shape rather than a height: its footprint, how much flat deck
    /// sits on top of it, and how steep the two faces are.
    ///
    /// The pooled `lip_height_m` is a prominence against a sixty-metre mean, and a jump long
    /// enough hides inside its own window — so a table that reads 1.9 m there can still be the
    /// 3.6 m the program asked for. This prints the shape, which is the thing to compare.
    ///
    /// ```text
    /// FROST_TRACK=…/indiana.pkz cargo test -- --ignored --nocapture jump_profiles
    /// ```
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn jump_profiles() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track .pkz/folder");
        let path = Path::new(&var);
        let names = track::entry_names(path).unwrap();
        let entry = track::heightfield_entries(&names).into_iter().next().expect("a heightfield");
        let bytes = track::read_entry(path, &entry).unwrap();
        let layout = heightfield::probe(&bytes, None).expect("a terrain grid");
        let mps_src = layout.metres_per_sample.expect("a stated footprint");
        let size_x = mps_src * (layout.width.max(2) - 1) as f32;
        let size_z = mps_src * (layout.height.max(2) - 1) as f32;
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let block = bytes.get(block_at..).unwrap_or(&[]);
        let lap = crate::trackline::read(block).expect("a centreline");
        let (fw, fh, v) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
        let g = Grid { w: fw as usize, h: fh as usize, size_x, size_z, v };

        let n = (2.0 * RIDDEN_REACH_M / RIDDEN_LATERAL_M) as usize + 1;
        let u_at = |i: usize| i as f32 * RIDDEN_LATERAL_M - RIDDEN_REACH_M;
        let mut along: Vec<f32> = Vec::new();
        for seg in &lap.segments {
            let steps = ((seg.length / RIDDEN_STEP_M) as usize).max(1);
            for k in 0..steps {
                let d = k as f32 * seg.length / steps as f32;
                let (x, z, h) = if seg.radius == 0.0 {
                    let (hx, hz) = crate::trackprog::heading_vector(seg.heading);
                    (seg.x + d * hx, seg.z + d * hz, seg.heading)
                } else {
                    let h = seg.heading + d / seg.radius;
                    (
                        seg.x + seg.radius * (seg.heading.cos() - h.cos()),
                        seg.z + seg.radius * (h.sin() - seg.heading.sin()),
                        h,
                    )
                };
                let (rx, rz) = crate::trackprog::right_vector(h);
                let mut band: Vec<f32> = (0..n)
                    .filter(|i| u_at(*i).abs() <= 3.0)
                    .map(|i| g.at(x + u_at(i) * rx, z + u_at(i) * rz))
                    .collect();
                band.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                along.push(band[band.len() / 2]);
            }
        }

        // A longer baseline than the pooled statistic uses, because a big table is longer than
        // sixty metres end to end and vanishes into its own window at that width.
        let base = smooth_ring(&along, (140.0 / RIDDEN_STEP_M / 2.0) as usize, true);
        let rel: Vec<f32> = (0..along.len()).map(|i| along[i] - base[i]).collect();
        let m = rel.len();
        const DECK_M: f32 = 0.12;

        let mut rows: Vec<(f32, f32, f32, f32, f32, f32)> = Vec::new();
        for i in 0..m {
            if !(rel[i] >= rel[(i + m - 1) % m] && rel[i] > rel[(i + 1) % m]) {
                continue;
            }
            let (mut l, mut k) = (0usize, 0usize);
            while l < m / 2 && rel[(i + m - l - 1) % m] <= rel[(i + m - l) % m] {
                l += 1;
            }
            while k < m / 2 && rel[(i + k + 1) % m] <= rel[(i + k) % m] {
                k += 1;
            }
            let prom = (rel[i] - rel[(i + m - l) % m]).min(rel[i] - rel[(i + k) % m]);
            if prom < RIDDEN_LIP_M {
                continue;
            }
            let mut a = 0usize;
            while a < l && rel[(i + m - a - 1) % m] >= rel[i] - DECK_M {
                a += 1;
            }
            let mut b = 0usize;
            while b < k && rel[(i + b + 1) % m] >= rel[i] - DECK_M {
                b += 1;
            }
            let deck = (a + b) as f32 * RIDDEN_STEP_M;
            let foot = (l + k) as f32 * RIDDEN_STEP_M;
            let span = (3.0 / RIDDEN_STEP_M) as usize;
            let (mut up, mut down) = (0.0f32, 0.0f32);
            for t in 0..l.saturating_sub(span) {
                let p = (i + m - l + t) % m;
                up = up.max((along[(p + span) % m] - along[p]) / 3.0);
            }
            for t in 0..k.saturating_sub(span) {
                let p = (i + t) % m;
                down = down.max((along[p] - along[(p + span) % m]) / 3.0);
            }
            rows.push((
                i as f32 * RIDDEN_STEP_M,
                prom,
                deck,
                foot,
                up.atan().to_degrees(),
                down.atan().to_degrees(),
            ));
        }
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut kept: Vec<(f32, f32, f32, f32, f32, f32)> = Vec::new();
        for r in rows {
            match kept.last_mut() {
                Some(p) if r.0 - p.0 < 6.0 => {
                    if r.1 > p.1 {
                        *p = r;
                    }
                }
                _ => kept.push(r),
            }
        }

        println!(
            "\n{}  —  {:.0} m lap\n      at      h    deck  footprint   up   down",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            lap.length,
        );
        for r in &kept {
            println!(
                "  {:>6.0} {:>6.2} {:>6.1} {:>9.1} {:>5.1} {:>6.1}{}",
                r.0, r.1, r.2, r.3, r.4, r.5,
                if r.1 >= 1.0 { "  <" } else { "" },
            );
        }
        let big: Vec<_> = kept.iter().filter(|r| r.1 >= 1.0).collect();
        let tables = big.iter().filter(|r| r.2 >= 5.0).count();
        let mut decks: Vec<f32> = big.iter().map(|r| r.2).collect();
        decks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut hs: Vec<f32> = big.iter().map(|r| r.1).collect();
        hs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "\n  {} lips, {} over 1 m ({:.1}/km); of those {} carry a deck of 5 m or more",
            kept.len(),
            big.len(),
            big.len() as f32 / (lap.length / 1000.0),
            tables,
        );
        if !big.is_empty() {
            println!(
                "  over 1 m: h p50 {:.2} max {:.2}   deck p50 {:.1} m max {:.1} m",
                hs[hs.len() / 2], hs[hs.len() - 1],
                decks[decks.len() / 2], decks[decks.len() - 1],
            );
        }
    }

    /// Roughness split by where on the lap it is — straights, corners, jump faces — read the
    /// same way off a published track and a generated one.
    ///
    /// Sampled along the lap at an eighth of a metre: the chop a wheel feels runs at about
    /// half a metre, which a half-metre station step aliases away.
    ///
    /// ```text
    /// FROST_TRACK=…/indiana.pkz cargo test -p frost-studio -- --ignored --nocapture zone_roughness
    /// cargo test -p frost-studio -- --ignored --nocapture zone_roughness   # the base track
    /// ```
    #[test]
    #[ignore = "slow — reads or synthesises a lap"]
    fn zone_roughness() {
        use std::f32::consts::{PI, TAU};
        let step = 0.5f32;
        let (stations, g): (Vec<(f32, f32, f32)>, Grid) = match std::env::var("FROST_TRACK") {
            Ok(var) => {
                let path = std::path::Path::new(&var);
                let names = track::entry_names(path).unwrap();
                let entry = track::heightfield_entries(&names).into_iter().next().expect("a heightfield");
                let bytes = track::read_entry(path, &entry).unwrap();
                let layout = heightfield::probe(&bytes, None).expect("a terrain grid");
                let mps_src = layout.metres_per_sample.expect("a stated footprint");
                let size_x = mps_src * (layout.width.max(2) - 1) as f32;
                let size_z = mps_src * (layout.height.max(2) - 1) as f32;
                let block_at = layout.offset
                    + layout.width as usize * layout.height as usize * layout.sample.size();
                let lap = crate::trackline::read(bytes.get(block_at..).unwrap_or(&[])).expect("a centreline");
                let (fw, fh, v) = heightfield::read_grid(&bytes, &layout, layout.width.max(layout.height));
                let st = lap.stations(step).into_iter().map(|q| (q.x, q.z, q.heading)).collect();
                (st, Grid { w: fw as usize, h: fh as usize, size_x, size_z, v })
            }
            Err(_) => {
                let p: crate::trackprog::TrackProgram = match std::env::var("FROST_PROGRAM") {
                    Ok(f) => serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap(),
                    Err(_) => serde_json::from_str(crate::trackprog::EXAMPLE).unwrap(),
                };
                let s = crate::tracksynth::synthesise(&p).unwrap();
                let st = s.stations.iter().map(|q| (q.x, q.z, q.heading)).collect();
                let (size_x, size_z) = (p.terrain.size_x, p.terrain.size_z);
                (st, Grid { w: s.gw, h: s.gh, size_x, size_z, v: s.heights })
            }
        };
        let n = stations.len();
        // FROST_DUMP=<prefix> also writes the heights and the lap, to draw pictures from.
        if let Ok(out) = std::env::var("FROST_DUMP") {
            let mut b = Vec::with_capacity(g.v.len() * 4 + 16);
            for v in [g.w as f32, g.h as f32, g.size_x, g.size_z] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for v in &g.v {
                b.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(format!("{out}.heights"), b).unwrap();
            let csv: String = stations.iter().map(|(x, z, h)| format!("{x},{z},{h}\n")).collect();
            std::fs::write(format!("{out}.stations.csv"), csv).unwrap();
        }

        // Straight, corner (under 40 m radius), or a jump face (climbing or falling over 13%).
        let (k, m) = ((5.0 / step) as usize, (1.5 / step) as usize);
        let h0: Vec<f32> = stations.iter().map(|&(x, z, _)| g.at(x, z)).collect();
        let zone: Vec<usize> = (0..n)
            .map(|i| {
                let mut d = stations[(i + k) % n].2 - stations[(i + n - k) % n].2;
                while d > PI { d -= TAU; }
                while d < -PI { d += TAU; }
                let curv = d.abs() / (2.0 * k as f32 * step);
                let grade = (h0[(i + m) % n] - h0[(i + n - m) % n]) / (2.0 * m as f32 * step);
                if grade.abs() > 0.13 { 2 } else if curv > 1.0 / 40.0 { 1 } else { 0 }
            })
            .collect();
        // Straight ground in the 25 m before a corner: where the braking bumps are.
        let ahead = (25.0 / step) as usize;
        let zone: Vec<usize> = (0..n)
            .map(|i| {
                if zone[i] == 0 && (1..=ahead).any(|a| zone[(i + a) % n] == 1) { 3 } else { zone[i] }
            })
            .collect();

        // Along the line, an eighth of a metre at a time, 1.5 m either side of the centre.
        const SUB: usize = 4;
        let fine_step = step / SUB as f32;
        let half = ((0.5 / fine_step) as usize).max(1);
        let mut chat: [Vec<f32>; 4] = Default::default();
        let mut bumps = [0usize; 4];
        // Swells: bumps a braking wheel feels, 2 m and up, against a 3 m mean, 3 cm proud.
        let mut swells = [0usize; 4];
        let wide = ((1.5 / fine_step) as usize).max(1);
        let cols = 13;
        for j in 0..cols {
            let u = -1.5 + j as f32 * 0.25;
            let mut col = Vec::with_capacity(n * SUB);
            let mut zs = Vec::with_capacity(n * SUB);
            for i in 0..n {
                let (a, b) = (stations[i], stations[(i + 1) % n]);
                for t in 0..SUB {
                    let f = t as f32 / SUB as f32;
                    let (x, z, h) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2);
                    let (rx, rz) = crate::trackprog::right_vector(h);
                    col.push(g.at(x + u * rx, z + u * rz));
                    zs.push(zone[i]);
                }
            }
            let fine = detrend(&col, half);
            for i in 0..fine.len() {
                chat[zs[i]].push(fine[i]);
            }
            // A bump: the highest point within a quarter metre, standing 1.5 cm proud.
            for i in 2..fine.len().saturating_sub(2) {
                let v = fine[i];
                if v > 0.015 && (i - 2..=i + 2).all(|q| q == i || fine[q] < v) {
                    bumps[zs[i]] += 1;
                }
            }
            let broad = detrend(&col, wide);
            for i in 4..broad.len().saturating_sub(4) {
                let v = broad[i];
                if v > 0.03 && (i - 4..=i + 4).all(|q| q == i || broad[q] < v) {
                    swells[zs[i]] += 1;
                }
            }
        }

        // Across, six metres detrended, 5.5 m either side.
        let lat = RIDDEN_LATERAL_M;
        let w = (2.0 * RIDDEN_HALF_M / lat) as usize + 1;
        let half_across = ((3.0 / lat) as usize).max(1);
        // How much of the ridden width is in a rut: across samples 5 cm or more below the ground
        // around them, inside 5.5 m of the line. The share a rider cannot ride round.
        let mut rutted = [(0usize, 0usize); 4];
        let mut p2p: [Vec<f32>; 4] = Default::default();
        let mut walls: [Vec<f32>; 4] = Default::default();
        for (i, &(x, z, h)) in stations.iter().enumerate() {
            let (rx, rz) = crate::trackprog::right_vector(h);
            let row: Vec<f32> = (0..w)
                .map(|q| {
                    let u = q as f32 * lat - RIDDEN_HALF_M;
                    g.at(x + u * rx, z + u * rz)
                })
                .collect();
            let d = detrend(&row, half_across);
            let (lo, hi) = d.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
            p2p[zone[i]].push(hi - lo);
            rutted[zone[i]].1 += d.len();
            rutted[zone[i]].0 += d.iter().filter(|&&v| v < -0.05).count();
            for q in 1..d.len() {
                walls[zone[i]].push(((d[q] - d[q - 1]).abs() / lat).atan().to_degrees());
            }
        }
        let pct = |v: &mut Vec<f32>, p: f32| -> f32 {
            if v.is_empty() {
                return 0.0;
            }
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[((v.len() - 1) as f32 * p) as usize]
        };
        println!("  zone      metres  chatter  bumps/10m  across p2p p50/p90/max   wall p98  swells/10m  rutted");
        for (z, name) in ["straight", "corner", "jump face", "approach"].iter().enumerate() {
            let metres = zone.iter().filter(|&&q| q == z).count() as f32 * step;
            let per10 = bumps[z] as f32 / (metres * cols as f32).max(1e-3) * 10.0;
            println!(
                "  {name:<9} {metres:>6.0}  {:>7.3}  {per10:>9.1}  {:>6.2} {:>5.2} {:>5.2}        {:>4.0}°  {:>9.1}  {:>5.0}%",
                rms(&chat[z]),
                pct(&mut p2p[z], 0.5),
                pct(&mut p2p[z], 0.9),
                pct(&mut p2p[z], 1.0),
                pct(&mut walls[z], 0.98),
                swells[z] as f32 / (metres * cols as f32).max(1e-3) * 10.0,
                100.0 * rutted[z].0 as f32 / rutted[z].1.max(1) as f32,
            );
        }
        // How steep the jump faces stand along the line: a metre of climb, uphill only.
        let one = ((1.0 / step) as usize).max(1);
        let mut up_deg: Vec<f32> = (0..n)
            .filter(|&i| zone[i] == 2)
            .map(|i| (h0[(i + one) % n] - h0[i]).atan().to_degrees())
            .filter(|d| *d > 0.0)
            .collect();
        println!(
            "  jump faces uphill, per metre: p50 {:.0}°  p90 {:.0}°  p99 {:.0}°  max {:.0}°",
            pct(&mut up_deg, 0.5),
            pct(&mut up_deg, 0.9),
            pct(&mut up_deg, 0.99),
            pct(&mut up_deg, 1.0)
        );
    }
}
