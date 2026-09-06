//! What a published track stands beside its riding line, measured.
//!
//! The `.map` bakes every object into one mesh, and [`crate::map`] splits that back into
//! connected islands. An island is not an object, though: the exporter duplicates vertices,
//! so a track comes apart into a hundred and sixty thousand pieces of two or three triangles
//! each — one card, one quad, one face. A tree is a handful of them in the same place.
//!
//! So the work is three joins and a clustering. The islands come from the `.map`; their sheet
//! names come from the same file's texture table, which is the only statement a track makes
//! about *what* a thing is; the centreline comes from the `.trh`'s trailing block, which
//! [`crate::trackline`] reads. Islands of one class standing within a few metres of each
//! other are one object, and against the lap every object has a distance round it and a
//! lateral offset. That is what "where do objects go" means numerically.

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use std::path::Path;

use crate::map;

/// How close two islands of the same class have to be to count as one object, metres.
///
/// Three, because that is a tree: crossed cards sharing no vertex, standing on the same spot.
/// It also strings a fence's panels into one run, which is the right answer for a fence — a
/// fence is one object however many boards it is made of.
const LINK_M: f32 = 3.0;

/// Where a track's furniture ends and its landscape begins, metres from the centreline.
///
/// Not a round number picked for tidiness. Pooled over the corpus the two populations barely
/// overlap: what lines a riding line sits at 5–45 m, and the backdrop ring — the distant
/// treeline, the horizon fence, the far grandstand — sits at 100–700 m and stands 17 to 41 m
/// tall. Measured together they average into a tree that is nowhere and forty feet high.
const NEAR_M: f32 = 60.0;

/// What a sheet name says a thing is.
///
/// Named from the vocabulary the corpus actually uses rather than from what a track *might*
/// carry: twelve tracks between them name 60 sheets, and these are the groups they fall into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Class {
    Tree,
    Fence,
    Bale,
    Banner,
    Crowd,
    Structure,
    Pole,
    Vehicle,
    /// Dressing painted on the ground rather than standing on it — dirt, concrete, water,
    /// the pit lane, a tunnel mouth. Not an object, and excluded from every count.
    Ground,
    Unknown,
}

impl Class {
    /// Whether this is something placed rather than something painted.
    pub fn is_object(self) -> bool {
        !matches!(self, Class::Ground | Class::Unknown)
    }

    pub fn key(self) -> &'static str {
        match self {
            Class::Tree => "tree",
            Class::Fence => "fence",
            Class::Bale => "bale",
            Class::Banner => "banner",
            Class::Crowd => "crowd",
            Class::Structure => "structure",
            Class::Pole => "pole",
            Class::Vehicle => "vehicle",
            Class::Ground => "ground",
            Class::Unknown => "unknown",
        }
    }
}

/// Read a sheet name as a class.
///
/// Order matters: `ck_bridge_grate_c_a` is ground before it is anything else, and
/// `start_backdrop_c` is a banner rather than a building. Every rule here was written against
/// a name a track in the corpus actually ships.
pub fn classify(sheet: &str) -> Class {
    let s = sheet.to_ascii_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| s.contains(w));

    // Painted, not placed. First, because several of these also read as objects.
    if any(&[
        "dirt", "soil", "sand", "gravel", "mud", "concrete", "asphalt", "water", "rock",
        "pitlane", "tunnel", "grate", "bridge", "road", "kerb", "curb", "terrain", "ground",
    ]) {
        return Class::Ground;
    }
    if any(&["tree", "bark", "leaf", "leafs", "leaves", "bush", "foliage", "hedge", "palm"]) {
        return Class::Tree;
    }
    if any(&["fence", "net", "barrier", "railing", "hoarding"]) {
        return Class::Fence;
    }
    if any(&["haybale", "bale", "hay", "straw", "tuff", "tyre", "tire"]) {
        return Class::Bale;
    }
    if any(&[
        "inflate", "inflatable", "backdrop", "banner", "flag", "sign", "board", "advert",
        "sponsor", "logo", "arch",
    ]) {
        return Class::Banner;
    }
    if any(&["people", "person", "crowd", "spectator", "seat", "stand", "tribune"]) {
        return Class::Crowd;
    }
    if any(&[
        "vehicle", "trailer", "truck", "van", "car", "excavator", "tractor", "container",
        "ambulance", "quad",
    ]) {
        return Class::Vehicle;
    }
    if any(&["pole", "post", "powerline", "pylon", "mast", "tower"]) {
        return Class::Pole;
    }
    if any(&[
        "tent", "building", "castle", "clubhouse", "roof", "wall", "shed", "hut", "cabin",
        "office", "garage", "awning", "canopy", "metalsheet", "osb", "glass", "structure",
        "objects", "trash", "strap", "tearoff", "machines", "vmx", "easy_up",
    ]) {
        return Class::Structure;
    }
    Class::Unknown
}

/// One object, placed against the lap.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub class: Class,
    /// The sheet most of it is painted with, lower-cased.
    pub sheet: String,
    /// How many islands it was clustered from.
    pub pieces: u32,
    pub tris: u32,
    /// Metres, world frame.
    pub centre: [f32; 3],
    /// Bounding box extents, metres. `size[1]` is how tall it stands.
    pub size: [f32; 3],
    /// How far round the lap the nearest centreline point is, metres.
    pub along_m: f32,
    /// Lateral offset from the centreline, metres. Signed: positive is right of travel, the
    /// side a positive radius turns towards everywhere else in the pipeline.
    pub offset_m: f32,
}

/// The spread of one measurement, as the rest of the pipeline reports it.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spread {
    pub count: usize,
    pub p10: f32,
    pub p50: f32,
    pub p90: f32,
}

fn spread(v: &mut [f32]) -> Spread {
    if v.is_empty() {
        return Spread::default();
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |f: f32| v[(((v.len() - 1) as f32) * f).round() as usize];
    Spread {
        count: v.len(),
        p10: q(0.1),
        p50: q(0.5),
        p90: q(0.9),
    }
}

/// What one class amounts to on one track.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassStats {
    pub class: Class,
    /// True for the track's own furniture, false for the landscape beyond [`NEAR_M`].
    pub near: bool,
    pub objects: usize,
    pub per_km: f32,
    /// Distance from the centreline, unsigned.
    pub offset_m: Spread,
    /// How tall they stand.
    pub height_m: Spread,
    /// Longest footprint dimension — a fence run against a bale.
    pub span_m: Spread,
    /// Metres round the lap between one of these and the next. What a placement rule needs.
    pub gap_m: Spread,
    /// How much of the lap has one of these within 60 m of it, as a fraction. A fence that
    /// follows the whole track reads near 1; a start structure reads near 0.
    pub lap_covered: f32,
}

/// What one track carries.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSurvey {
    pub track: String,
    pub lap_m: f32,
    pub map_entry: String,
    /// Islands the `.map` split into, before clustering.
    pub islands: usize,
    /// Whether the map's sheets bound to its materials at all. False means every object here
    /// is `Unknown` and the track tells us nothing.
    pub bound: bool,
    /// `declared` where the sheet table bound the way the viewer binds it, `fallback` where
    /// it refused and every record was taken in table order instead. A fallback track's
    /// *offsets* are still its own; its *classes* may be a sheet out, so it is evidence with
    /// a caveat rather than evidence.
    pub binding: &'static str,
    pub objects: Vec<Placed>,
    pub classes: Vec<ClassStats>,
    /// Sheet name against islands wearing it, commonest first. The vocabulary.
    pub sheets: Vec<(String, usize)>,
}

/// Measure a track's objects against its own centreline.
pub fn survey(path: &Path) -> Result<ObjectSurvey> {
    let names = crate::track::entry_names(path)?;
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();

    // The lap, from the height file's trailing block.
    let hf = crate::track::heightfield_entries(&names)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no heightfield in this track"))?;
    let hb = crate::track::read_entry(path, &hf)?;
    let layout = crate::heightfield::probe(&hb, None)
        .ok_or_else(|| anyhow!("{hf} doesn't read as a terrain grid"))?;
    let block_at =
        layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
    let lap = crate::trackline::read(hb.get(block_at..).unwrap_or(&[]))
        .ok_or_else(|| anyhow!("{hf} carries no centreline"))?;

    // The scenery.
    let mut entry = String::new();
    let mut mesh = None;
    let mut sheets: Vec<(String, u32, u32)> = Vec::new();
    let mut binding = "none";
    for name in names.iter().filter(|n| {
        Path::new(n)
            .extension()
            .map(|e| e.eq_ignore_ascii_case("map"))
            .unwrap_or(false)
    }) {
        let Ok(bytes) = crate::track::read_entry(path, name) else {
            continue;
        };
        if !map::is_map(&bytes) {
            continue;
        }
        let Some(m) = map::parse(&bytes) else { continue };
        let named = Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase() == stem)
            .unwrap_or(false);
        if mesh.is_none() || named {
            entry = name.clone();
            // `declared` is the binding the viewer uses and it refuses a table it cannot
            // trust. Where it refuses, every record is better than none for a survey: the
            // names are still in the file, they just may not line up with the materials.
            let declared = map::declared(&bytes);
            binding = if declared.is_empty() { "fallback" } else { "declared" };
            sheets = if declared.is_empty() {
                map::primaries(&bytes)
            } else {
                declared
            };
            mesh = Some(m);
        }
        if named {
            break;
        }
    }
    let mesh = mesh.ok_or_else(|| anyhow!("no readable .map in {path:?}"))?;
    let bound = !sheets.is_empty();

    let sheet_of = |material: u32| -> &str {
        sheets
            .get(material as usize)
            .map(|(n, ..)| n.as_str())
            .unwrap_or("")
    };

    // Islands, named and classed.
    struct Island {
        class: Class,
        sheet: String,
        tris: u32,
        min: [f32; 3],
        max: [f32; 3],
        cx: f32,
        cz: f32,
    }
    let islands: Vec<Island> = mesh
        .objects
        .iter()
        .map(|o| {
            let sheet = sheet_of(o.material).to_ascii_lowercase();
            Island {
                class: classify(&sheet),
                sheet,
                tris: o.tri_count,
                min: o.min,
                max: o.max,
                cx: (o.min[0] + o.max[0]) * 0.5,
                cz: (o.min[2] + o.max[2]) * 0.5,
            }
        })
        .collect();

    // Cluster within a class: same kind of thing, standing within LINK_M of each other.
    // Grid-hashed so this stays linear in the island count rather than quadratic.
    let mut uf: Vec<usize> = (0..islands.len()).collect();
    fn find(uf: &mut Vec<usize>, mut i: usize) -> usize {
        while uf[i] != i {
            uf[i] = uf[uf[i]];
            i = uf[i];
        }
        i
    }
    let mut cells: std::collections::HashMap<(i32, i32, u8), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, is) in islands.iter().enumerate() {
        if !is.class.is_object() {
            continue;
        }
        let key = (
            (is.cx / LINK_M).floor() as i32,
            (is.cz / LINK_M).floor() as i32,
            is.class as u8,
        );
        cells.entry(key).or_default().push(i);
    }
    for (&(gx, gz, c), members) in &cells {
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(other) = cells.get(&(gx + dx, gz + dz, c)) else {
                    continue;
                };
                for &i in members {
                    for &j in other {
                        if i >= j {
                            continue;
                        }
                        let (a, b) = (&islands[i], &islands[j]);
                        let (dx, dz) = (a.cx - b.cx, a.cz - b.cz);
                        if dx * dx + dz * dz <= LINK_M * LINK_M {
                            let (ra, rb) = (find(&mut uf, i), find(&mut uf, j));
                            if ra != rb {
                                uf[ra] = rb;
                            }
                        }
                    }
                }
            }
        }
    }

    // Roll the islands up into objects.
    let mut by_root: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..islands.len() {
        if !islands[i].class.is_object() {
            continue;
        }
        let r = find(&mut uf, i);
        by_root.entry(r).or_default().push(i);
    }

    let stations = lap.stations(1.0);
    let mut objects: Vec<Placed> = by_root
        .values()
        .map(|members| {
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            let mut tris = 0u32;
            let mut tally: std::collections::HashMap<&str, u32> =
                std::collections::HashMap::new();
            for &i in members {
                let is = &islands[i];
                for k in 0..3 {
                    min[k] = min[k].min(is.min[k]);
                    max[k] = max[k].max(is.max[k]);
                }
                tris += is.tris;
                *tally.entry(is.sheet.as_str()).or_default() += is.tris;
            }
            let sheet = tally
                .into_iter()
                .max_by_key(|&(_, n)| n)
                .map(|(s, _)| s.to_string())
                .unwrap_or_default();
            let centre = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            let (along_m, offset_m) = nearest(&stations, centre[0], centre[2]);
            Placed {
                class: islands[members[0]].class,
                sheet,
                pieces: members.len() as u32,
                tris,
                centre,
                size: [max[0] - min[0], max[1] - min[1], max[2] - min[2]],
                along_m,
                offset_m,
            }
        })
        .collect();
    objects.sort_by(|a, b| a.along_m.partial_cmp(&b.along_m).unwrap_or(std::cmp::Ordering::Equal));

    let km = (lap.length / 1000.0).max(0.001);
    let mut classes: Vec<ClassStats> = Vec::new();
    for class in [
        Class::Tree,
        Class::Fence,
        Class::Bale,
        Class::Banner,
        Class::Crowd,
        Class::Structure,
        Class::Pole,
        Class::Vehicle,
    ] {
        for near in [true, false] {
            let mine: Vec<&Placed> = objects
                .iter()
                .filter(|p| p.class == class && (p.offset_m.abs() <= NEAR_M) == near)
                .collect();
            if mine.is_empty() {
                continue;
            }
            // How much of the lap has one of these beside it. Bucket at ten metres: an
            // object covers the stretch it stands next to.
            let buckets = ((lap.length / 10.0).ceil() as usize).max(1);
            let mut seen = vec![false; buckets];
            for p in &mine {
                let b = ((p.along_m / 10.0) as usize).min(buckets - 1);
                seen[b] = true;
            }
            // Spacing round the lap. `objects` is already sorted by `along_m`, and so is
            // this filtered view of it, so consecutive differences are the gaps.
            let mut gaps: Vec<f32> = mine
                .windows(2)
                .map(|w| w[1].along_m - w[0].along_m)
                .filter(|g| *g > 0.0)
                .collect();
            classes.push(ClassStats {
                class,
                near,
                objects: mine.len(),
                per_km: mine.len() as f32 / km,
                offset_m: spread(&mut mine.iter().map(|p| p.offset_m.abs()).collect::<Vec<_>>()),
                height_m: spread(&mut mine.iter().map(|p| p.size[1]).collect::<Vec<_>>()),
                span_m: spread(
                    &mut mine
                        .iter()
                        .map(|p| p.size[0].max(p.size[2]))
                        .collect::<Vec<_>>(),
                ),
                gap_m: spread(&mut gaps),
                lap_covered: seen.iter().filter(|s| **s).count() as f32 / buckets as f32,
            });
        }
    }

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for is in &islands {
        *counts.entry(is.sheet.clone()).or_default() += 1;
    }
    let mut sheets: Vec<(String, usize)> = counts.into_iter().collect();
    sheets.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    Ok(ObjectSurvey {
        track: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        lap_m: lap.length,
        map_entry: entry,
        islands: mesh.objects.len(),
        bound,
        binding,
        objects,
        classes,
        sheets,
    })
}

/// Walk the lap and return `(distance round it, signed lateral offset)` for a world point.
///
/// Coarse on purpose — a metre a step over a two-kilometre lap, against an object that is
/// metres across. Positive offset is to the **right** of travel, which is the side a positive
/// radius turns towards everywhere else in the pipeline.
fn nearest(stations: &[crate::trackline::Station], x: f32, z: f32) -> (f32, f32) {
    let mut best = (0.0f32, f32::INFINITY, 0.0f32);
    for p in stations {
        let (dx, dz) = (x - p.x, z - p.z);
        let d2 = dx * dx + dz * dz;
        if d2 < best.1 {
            let (rx, rz) = crate::trackprog::right_vector(p.heading);
            best = (p.at, d2, dx * rx + dz * rz);
        }
    }
    (best.0, best.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_read_the_corpus_vocabulary() {
        // Every one of these is a sheet a track in `~/Projects/pkz` actually ships.
        assert_eq!(classify("ck_tree_hawkstone_noleaf_atlas_c_a"), Class::Tree);
        assert_eq!(classify("wood_leafs_c_a"), Class::Tree);
        assert_eq!(classify("bark_qp_c"), Class::Tree);
        assert_eq!(classify("ck_fence_01_c_a"), Class::Fence);
        assert_eq!(classify("ck_orange_net_c_a"), Class::Fence);
        assert_eq!(classify("ck_haybale_c_a"), Class::Bale);
        assert_eq!(classify("inflate_tilable_c"), Class::Banner);
        assert_eq!(classify("start_backdrop_c"), Class::Banner);
        assert_eq!(classify("ck_cutout_people_atlas_c_a"), Class::Crowd);
        assert_eq!(classify("ck_seat_red_c_a"), Class::Crowd);
        assert_eq!(classify("big_tent_c"), Class::Structure);
        assert_eq!(classify("clubhouse_c"), Class::Structure);
        assert_eq!(classify("metal_pole_c"), Class::Pole);
        assert_eq!(classify("flagpost_c"), Class::Pole);
        assert_eq!(classify("semi_trailers_c"), Class::Vehicle);
        // Painted, not placed — and these are the ones that read as objects if the ground
        // rules don't come first.
        assert_eq!(classify("ck_bridge_grate_c_a"), Class::Ground);
        assert_eq!(classify("pitlane_c"), Class::Ground);
        assert_eq!(classify("soil_dark_c"), Class::Ground);
        assert_eq!(classify("water_c"), Class::Ground);
    }

    /// What published tracks stand beside their riding line:
    ///
    /// ```text
    /// FROST_TRACKS=~/Projects/pkz/tracks cargo test -- --ignored --nocapture object_corpus
    /// ```
    #[test]
    #[ignore = "needs real tracks — set FROST_TRACKS"]
    fn object_corpus() {
        let root = std::env::var("FROST_TRACKS").expect("set FROST_TRACKS to a tracks folder");
        let mut paths: Vec<std::path::PathBuf> =
            crate::linkwalk::walk_depth(Path::new(&root), 3)
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
            match survey(p) {
                Ok(s) => {
                    if !s.bound {
                        println!("\n{:<30} lap {:>5.0}m  binds no sheets — skipped", s.track, s.lap_m);
                        continue;
                    }
                    println!(
                        "\n{:<30} lap {:>5.0}m  {:>6} islands -> {:>5} objects  [{}]",
                        s.track,
                        s.lap_m,
                        s.islands,
                        s.objects.len(),
                        s.binding,
                    );
                    for c in s.classes.iter().filter(|c| c.near) {
                        println!(
                            "  {:<10} {:>5} ({:>6.1}/km)  offset p10 {:>5.1} p50 {:>5.1} p90 {:>5.1} m  \
                             tall p50 {:>4.1}  span p50 {:>5.1}  gap p50 {:>5.1} m  lap {:>3.0}%",
                            c.class.key(), c.objects, c.per_km,
                            c.offset_m.p10, c.offset_m.p50, c.offset_m.p90,
                            c.height_m.p50, c.span_m.p50, c.gap_m.p50, c.lap_covered * 100.0,
                        );
                    }
                    let far: Vec<&ClassStats> = s.classes.iter().filter(|c| !c.near).collect();
                    if !far.is_empty() {
                        let n: usize = far.iter().map(|c| c.objects).sum();
                        println!(
                            "  {:<10} {:>5} beyond {NEAR_M:.0} m — landscape, not furniture: {}",
                            "backdrop", n,
                            far.iter()
                                .map(|c| format!("{} {}", c.objects, c.class.key()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    all.push(s);
                }
                Err(e) => println!("{name:<30} skipped: {e:#}"),
            }
        }

        println!("\n=== the track's own furniture, pooled over {} tracks ===", all.len());
        println!(
            "{:<10} {:>8}  {:>21}  {:>18}  {:>16}  {:>14}",
            "", "tracks", "per km  p10/p50/p90", "offset p10/p50/p90", "gap p10/p50/p90", "tall p50",
        );
        for class in [
            Class::Tree, Class::Fence, Class::Bale, Class::Banner,
            Class::Crowd, Class::Structure, Class::Pole, Class::Vehicle,
        ] {
            let mine: Vec<&ClassStats> = all
                .iter()
                .flat_map(|s| s.classes.iter())
                .filter(|c| c.near && c.class == class)
                .collect();
            if mine.is_empty() {
                continue;
            }
            let mut per_km: Vec<f32> = mine.iter().map(|c| c.per_km).collect();
            let mut off: Vec<f32> = mine.iter().map(|c| c.offset_m.p50).collect();
            let mut gap: Vec<f32> = mine.iter().filter(|c| c.gap_m.count > 0).map(|c| c.gap_m.p50).collect();
            let mut tall: Vec<f32> = mine.iter().map(|c| c.height_m.p50).collect();
            let mut cover: Vec<f32> = mine.iter().map(|c| c.lap_covered).collect();
            let (n, k, o, g, t, v) = (
                mine.len(), spread(&mut per_km), spread(&mut off),
                spread(&mut gap), spread(&mut tall), spread(&mut cover),
            );
            println!(
                "{:<10} {n:>8}  {:>6.1} {:>6.1} {:>6.1}  {:>5.1} {:>5.1} {:>5.1} m  \
                 {:>4.1} {:>4.1} {:>4.1} m  {:>4.1} m   lap {:>3.0}-{:.0}%",
                class.key(), k.p10, k.p50, k.p90, o.p10, o.p50, o.p90,
                g.p10, g.p50, g.p90, t.p50, v.p10 * 100.0, v.p90 * 100.0,
            );
        }

        if let Ok(out) = std::env::var("FROST_OUT") {
            std::fs::write(&out, serde_json::to_vec_pretty(&all).unwrap()).unwrap();
            println!("wrote {out}");
        }
    }
}
