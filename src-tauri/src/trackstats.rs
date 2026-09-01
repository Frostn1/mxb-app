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

fn ridden(lap: &crate::trackline::Lap, g: &Grid) -> Option<RiddenStats> {
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
                             turning {:>5.0}°",
                            "  centreline", r.lap_m, r.segments, r.arcs, r.straights,
                            r.turns, r.turn_deg.p50, r.turn_radius_m.p50, r.total_turn_deg,
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
}
