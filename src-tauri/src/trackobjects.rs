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
    // Before the banner rule, because a `flagpost` is a pole that happens to fly a flag and
    // the banner rule would take it on the word "flag" alone.
    if any(&["pole", "post", "powerline", "pylon", "mast", "tower"]) {
        return Class::Pole;
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

#[cfg(test)]
mod edge_marking {
    use super::*;

    /// What a published track marks its riding line with, island by island.
    ///
    /// `survey` clusters, and clustering is wrong for this question: the exporter cuts a
    /// marker line into one piece per metre, so what a placement rule needs — how tall a
    /// stake is, how wide, how far out and how far apart — is only in the raw pieces. They
    /// are grouped by how bright the sheet is where the piece samples it, because a marker
    /// stake is whatever thin pale thing stands beside the line, whichever sheet it wears.
    ///
    /// ```text
    /// FROST_TRACK=~/Projects/pkz/tracks/2024_ARLMX_RD11_INDIANA_PRO.pkz \
    /// FROST_DUMP=/tmp/sheets cargo test --bin mxb-app -- --ignored --nocapture edge_marking
    /// ```
    ///
    /// `FROST_DUMP` writes every sheet out as a PNG — the only way to read what a banner
    /// says. `FROST_SHEET` widens the net from thin uprights to every piece of one sheet.
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK"]
    fn edge_marking() {
        let track = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::PathBuf::from(&track);
        let names = crate::track::entry_names(&path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();

        // The lap, the same way `survey` reads it.
        let hf = crate::track::heightfield_entries(&names).into_iter().next().unwrap();
        let hb = crate::track::read_entry(&path, &hf).unwrap();
        let layout = crate::heightfield::probe(&hb, None).unwrap();
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let stations = crate::trackline::read(&hb[block_at..]).unwrap().stations(1.0);

        let entry = names
            .iter()
            .find(|n| n.to_ascii_lowercase() == format!("{stem}/{stem}.map"))
            .or_else(|| names.iter().find(|n| n.to_ascii_lowercase().ends_with(".map")))
            .expect("a .map")
            .clone();
        let bytes = crate::track::read_entry(&path, &entry).unwrap();
        let mesh = map::parse(&bytes).expect("parses");
        let sheets = map::declared(&bytes);
        let tex = map::textures(&bytes, 512);
        println!("{entry}: {} islands, {} sheets", mesh.objects.len(), sheets.len());

        // Mean colour of a material over the UV box one piece uses. Sheets come back already
        // row-flipped, so V maps straight to a row.
        let colour = |mat: u32, u0: f32, u1: f32, v0: f32, v1: f32| -> Option<[f32; 3]> {
            let t = tex.iter().find(|t| t.material == mat)?;
            if t.width == 0 || t.height == 0 {
                return None;
            }
            let px = |f: f32, d: u32| ((f.fract() + 1.0).fract() * d as f32) as u32 % d;
            let (mut sum, mut n) = ([0f64; 3], 0u32);
            for i in 0..12 {
                for j in 0..12 {
                    let u = u0 + (u1 - u0) * i as f32 / 11.0;
                    let v = v0 + (v1 - v0) * j as f32 / 11.0;
                    let o = ((px(1.0 - v, t.height) * t.width + px(u, t.width)) * 4) as usize;
                    if t.rgba[o + 3] < 32 {
                        continue;
                    }
                    for k in 0..3 {
                        sum[k] += t.rgba[o + k] as f64;
                    }
                    n += 1;
                }
            }
            (n > 0).then(|| std::array::from_fn(|k| (sum[k] / n as f64) as f32))
        };

        let want = std::env::var("FROST_SHEET").unwrap_or_default().to_ascii_lowercase();
        #[derive(Default)]
        struct Row {
            off: Vec<f32>,
            along: Vec<f32>,
            h: Vec<f32>,
            w: Vec<f32>,
            col: Vec<[f32; 3]>,
        }
        let mut by: std::collections::BTreeMap<String, Row> = Default::default();
        for o in &mesh.objects {
            let (w, h, d) = (o.max[0] - o.min[0], o.max[1] - o.min[1], o.max[2] - o.min[2]);
            let named = sheets
                .get(o.material as usize)
                .map(|(n, ..)| n.to_ascii_lowercase())
                .unwrap_or_default();
            if want.is_empty() {
                // A stake: a hand's breadth in plan, knee to shoulder high.
                if w.max(d) > 0.5 || !(0.4..2.6).contains(&h) {
                    continue;
                }
            } else if !named.contains(&want) {
                continue;
            }
            let (cx, cz) = ((o.min[0] + o.max[0]) * 0.5, (o.min[2] + o.max[2]) * 0.5);
            let (along, off) = nearest(&stations, cx, cz);
            if off.abs() > NEAR_M {
                continue;
            }
            let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for t in o.tri_start as usize..(o.tri_start + o.tri_count) as usize {
                for k in 0..3 {
                    let i = mesh.indices[t * 3 + k] as usize;
                    let (u, v) = (mesh.uvs[i * 2], mesh.uvs[i * 2 + 1]);
                    (u0, u1) = (u0.min(u), u1.max(u));
                    (v0, v1) = (v0.min(v), v1.max(v));
                }
            }
            let c = colour(o.material, u0, u1, v0, v1);
            let luma = c.map(|c| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]).unwrap_or(0.0);
            let shade = if luma > 170.0 {
                "bright"
            } else if luma > 110.0 {
                "light"
            } else {
                "dark"
            };
            let span = w.max(d);
            let size = if want.is_empty() {
                String::new()
            } else if span < 0.5 {
                " post    ".into()
            } else if span < 8.0 {
                format!(" panel{span:>2.0}m ")
            } else {
                " big     ".into()
            };
            let e = by.entry(format!("{size}{shade:<7}{named}")).or_default();
            e.off.push(off);
            e.along.push(along);
            e.h.push(h);
            e.w.push(span);
            e.col.extend(c);
        }

        println!("\n=== pieces within {NEAR_M:.0} m of the line ===");
        for (name, mut r) in by {
            if r.off.len() < 6 {
                continue;
            }
            let mut absoff: Vec<f32> = r.off.iter().map(|v| v.abs()).collect();
            let left = r.off.iter().filter(|v| **v < 0.0).count();
            // Gap along the lap, per side — the two sides interleave, so pooling them halves
            // the figure a placement rule wants.
            let mut gaps = Vec::new();
            for side in [false, true] {
                let mut a: Vec<f32> = r
                    .off
                    .iter()
                    .zip(&r.along)
                    .filter(|(o, _)| (**o > 0.0) == side)
                    .map(|(_, a)| *a)
                    .collect();
                a.sort_by(|x, y| x.partial_cmp(y).unwrap());
                gaps.extend(a.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 0.05));
            }
            let mean = |k: usize| {
                if r.col.is_empty() {
                    0.0
                } else {
                    r.col.iter().map(|c| c[k]).sum::<f32>() / r.col.len() as f32
                }
            };
            let (rr, gg, bb) = (mean(0), mean(1), mean(2));
            let n = r.off.len();
            println!(
                "  {name:<38} n {n:>5}  L/R {left}/{}  off {:>5.1}/{:>5.1}/{:>5.1}  \
                 tall {:>4.2}/{:>4.2}/{:>4.2}  wide {:>4.2}  gap {:>5.1}/{:>5.1}/{:>5.1}  \
                 rgb ({rr:>3.0},{gg:>3.0},{bb:>3.0})",
                n - left,
                spread(&mut absoff).p10, spread(&mut absoff).p50, spread(&mut absoff).p90,
                spread(&mut r.h).p10, spread(&mut r.h).p50, spread(&mut r.h).p90,
                spread(&mut r.w).p50,
                spread(&mut gaps).p10, spread(&mut gaps).p50, spread(&mut gaps).p90,
            );
        }

        // Which way up a sheet is meant to be read, which is the one thing about a printed
        // banner that cannot be checked by looking at the numbers. `V` against world height,
        // over the pieces that stand up: positive means `V` grows upward, so `V` zero is a
        // card's foot and the file's first row is the picture's bottom. Anything generated has
        // to agree with the sign a published track uses, or its wordmarks come out on their
        // heads in the game and right way up in every dump.
        let (mut sxy, mut sxx, mut syy, mut sx, mut sy, mut n) = (0f64, 0f64, 0f64, 0f64, 0f64, 0u64);
        for o in &mesh.objects {
            if o.max[1] - o.min[1] < 0.6 {
                continue;
            }
            // `FROST_SHEET` narrows this to one sheet, which is how a banner's own convention
            // is read rather than the average of everything that stands up.
            if !want.is_empty()
                && !sheets
                    .get(o.material as usize)
                    .map(|(n, ..)| n.to_ascii_lowercase().contains(&want))
                    .unwrap_or(false)
            {
                continue;
            }
            for t in o.tri_start as usize..(o.tri_start + o.tri_count) as usize {
                for k in 0..3 {
                    let i = mesh.indices[t * 3 + k] as usize;
                    let (y, v) = (mesh.positions[i * 3 + 1] as f64, mesh.uvs[i * 2 + 1] as f64);
                    sx += y;
                    sy += v;
                    sxx += y * y;
                    syy += v * v;
                    sxy += y * v;
                    n += 1;
                }
            }
        }
        if n > 2 {
            let d = ((n as f64 * sxx - sx * sx) * (n as f64 * syy - sy * sy)).sqrt();
            let r = if d > 0.0 { (n as f64 * sxy - sx * sy) / d } else { 0.0 };
            println!(
                "\ncorr(world Y, V) = {r:+.3} over {n} vertices of standing geometry \
                 — {} ",
                if r > 0.0 { "V grows upward, file row 0 is the picture's bottom" }
                else { "V grows downward, file row 0 is the picture's top" }
            );
        }

        // The one thing about a printed banner that numbers cannot settle: which way up it is
        // seen. A correlation of world height against `V` says nothing on an atlas, because
        // each panel sits in its own band of it. So rebuild the widest standing panel the way
        // the game sees it — world across against world up, sampled through the panel's own
        // UVs and the sheet as `map::textures` hands it over — and look at it.
        if let Ok(dump) = std::env::var("FROST_DUMP") {
            std::fs::create_dir_all(&dump).ok();
            let mut best: Option<(&map::MapObject, f32)> = None;
            for o in &mesh.objects {
                let (w, h, d) = (o.max[0] - o.min[0], o.max[1] - o.min[1], o.max[2] - o.min[2]);
                let span = w.max(d);
                if h < 0.6 || span < 1.5 || h > span {
                    continue;
                }
                if !want.is_empty()
                    && !sheets
                        .get(o.material as usize)
                        .map(|(n, ..)| n.to_ascii_lowercase().contains(&want))
                        .unwrap_or(false)
                {
                    continue;
                }
                if best.map(|(_, s)| span > s).unwrap_or(true) {
                    best = Some((o, span));
                }
            }
            if let Some((o, _)) = best {
                let t = tex.iter().find(|t| t.material == o.material);
                let (w, d) = (o.max[0] - o.min[0], o.max[2] - o.min[2]);
                // The panel's long axis in plan, so a banner facing any direction rebuilds.
                let across_x = w >= d;
                let (out_w, out_h) = (720u32, 200u32);
                let mut img = image::RgbaImage::new(out_w, out_h);
                for py in 0..out_h {
                    for px in 0..out_w {
                        let fx = px as f32 / (out_w - 1) as f32;
                        // Down the image is down the world.
                        let fy = 1.0 - py as f32 / (out_h - 1) as f32;
                        let wx = if across_x {
                            o.min[0] + w * fx
                        } else {
                            o.min[2] + d * fx
                        };
                        let wy = o.min[1] + (o.max[1] - o.min[1]) * fy;
                        // The triangle this point falls in, and its UV interpolated across
                        // it. Nearest-vertex was tried and it samples four texels for the
                        // whole panel, which reads as two flat bands and settles nothing.
                        let mut buv: Option<(f32, f32)> = None;
                        for tri in o.tri_start as usize..(o.tri_start + o.tri_count) as usize {
                            let at = |k: usize| {
                                let i = mesh.indices[tri * 3 + k] as usize;
                                let vx = if across_x {
                                    mesh.positions[i * 3]
                                } else {
                                    mesh.positions[i * 3 + 2]
                                };
                                ((vx, mesh.positions[i * 3 + 1]), (mesh.uvs[i * 2], mesh.uvs[i * 2 + 1]))
                            };
                            let (a, ua) = at(0);
                            let (b, ub) = at(1);
                            let (c, uc) = at(2);
                            let area = (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1);
                            if area.abs() < 1e-9 {
                                continue;
                            }
                            let w0 = ((b.0 - wx) * (c.1 - wy) - (c.0 - wx) * (b.1 - wy)) / area;
                            let w1 = ((c.0 - wx) * (a.1 - wy) - (a.0 - wx) * (c.1 - wy)) / area;
                            let w2 = 1.0 - w0 - w1;
                            if w0 < -1e-3 || w1 < -1e-3 || w2 < -1e-3 {
                                continue;
                            }
                            buv = Some((
                                ua.0 * w0 + ub.0 * w1 + uc.0 * w2,
                                ua.1 * w0 + ub.1 * w1 + uc.1 * w2,
                            ));
                            break;
                        }
                        let Some(buv) = buv else {
                            img.put_pixel(px, py, image::Rgba([90, 90, 96, 255]));
                            continue;
                        };
                        let col = match t {
                            Some(t) if t.width > 0 => {
                                let sx = ((buv.0.rem_euclid(1.0)) * t.width as f32) as u32
                                    % t.width;
                                let sy = ((1.0 - buv.1.rem_euclid(1.0)) * t.height as f32) as u32
                                    % t.height;
                                let o = ((sy * t.width + sx) * 4) as usize;
                                [t.rgba[o], t.rgba[o + 1], t.rgba[o + 2], 255]
                            }
                            _ => [255, 0, 255, 255],
                        };
                        img.put_pixel(px, py, image::Rgba(col));
                    }
                }
                let file = format!("{dump}/panel-as-seen.png");
                img.save(&file).unwrap();
                println!("\nrebuilt the widest standing panel as the game frames it: {file}");
            }
        }

        // And the pictures, because a banner's text is only in its sheet.
        let Ok(dump) = std::env::var("FROST_DUMP") else { return };
        std::fs::create_dir_all(&dump).unwrap();
        for t in map::textures(&bytes, 1024) {
            let file = format!("{dump}/{:02}_{}.png", t.material, t.name);
            image::RgbaImage::from_raw(t.width, t.height, t.rgba.clone())
                .unwrap()
                .save(&file)
                .unwrap();
        }
        println!("\nsheets written to {dump}");
    }
}

#[cfg(test)]
mod banner_facing {
    use super::*;

    /// Which way a published track turns a printed banner, and how it repeats one.
    ///
    /// A hoarding is drawn from both sides, so nothing about culling says which way it faces.
    /// What says it is the print: `u` runs one way along a board's own length, and the
    /// question is whether that direction agrees with the lap's heading or opposes it on each
    /// side of the track. Deriving it from the game's handedness gives two answers depending
    /// on which convention you assume; Indiana settles it, because its sponsors read the
    /// right way round.
    ///
    /// The same walk answers the other question a generated hoarding has to get right: how a
    /// track *repeats* a banner. Indiana's is `inflate_tilable_c` — one design, printed on
    /// piece after piece bolted together — so what to look at is how many pieces share a UV
    /// box and how far apart they sit.
    ///
    /// ```text
    /// FROST_TRACK=~/Projects/pkz/tracks/2024_ARLMX_RD11_INDIANA_PRO.pkz \
    /// cargo test --bin mxb-app -- --ignored --nocapture which_way_a_banner_faces
    /// ```
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK"]
    fn which_way_a_banner_faces() {
        let track = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::PathBuf::from(&track);
        let names = crate::track::entry_names(&path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();

        let hf = crate::track::heightfield_entries(&names).into_iter().next().unwrap();
        let hb = crate::track::read_entry(&path, &hf).unwrap();
        let layout = crate::heightfield::probe(&hb, None).unwrap();
        let block_at =
            layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
        let stations = crate::trackline::read(&hb[block_at..]).unwrap().stations(1.0);

        let entry = names
            .iter()
            .find(|n| n.to_ascii_lowercase() == format!("{stem}/{stem}.map"))
            .or_else(|| names.iter().find(|n| n.to_ascii_lowercase().ends_with(".map")))
            .expect("a .map")
            .clone();
        let bytes = crate::track::read_entry(&path, &entry).unwrap();
        let mesh = map::parse(&bytes).expect("parses");
        let sheets = map::declared(&bytes);

        // The nearest station, with its heading — `nearest` drops the heading and the whole
        // question is asked against it.
        let nearest_at = |x: f32, z: f32| -> (f32, f32, (f32, f32)) {
            let mut best = (0.0f32, f32::INFINITY, 0.0f32, (0.0f32, 0.0f32));
            for p in &stations {
                let (dx, dz) = (x - p.x, z - p.z);
                let d2 = dx * dx + dz * dz;
                if d2 < best.1 {
                    let (rx, rz) = crate::trackprog::right_vector(p.heading);
                    best = (p.at, d2, dx * rx + dz * rz, crate::trackprog::heading_vector(p.heading));
                }
            }
            (best.0, best.2, best.3)
        };

        struct Board {
            along: f32,
            off: f32,
            /// `+1` when every readable face grows `u` toward its own viewer's right,
            /// `-1` when they all grow it to the left. A board printed the same way on both
            /// faces lands near `+1` or `-1`; one printed once and mirrored on the back
            /// lands near zero.
            with_lap: f32,
            span: f32,
            tall: f32,
            /// The UV box the piece samples, rounded, so identical prints group.
            box_key: (i32, i32, i32, i32),
        }
        let mut boards: Vec<Board> = Vec::new();
        let mut skipped = 0usize;

        for o in &mesh.objects {
            let named = sheets
                .get(o.material as usize)
                .map(|(n, ..)| n.to_ascii_lowercase())
                .unwrap_or_default();
            if classify(&named) != Class::Banner {
                continue;
            }
            let (w, h, d) = (o.max[0] - o.min[0], o.max[1] - o.min[1], o.max[2] - o.min[2]);
            let span = w.max(d);
            // A board: stands up, wider than it is tall, and not the start backdrop.
            if !(0.5..4.0).contains(&h) || !(0.4..12.0).contains(&span) {
                continue;
            }
            let (cx, cz) = ((o.min[0] + o.max[0]) * 0.5, (o.min[2] + o.max[2]) * 0.5);
            let (along, off, _fwd) = nearest_at(cx, cz);
            if off.abs() > 40.0 {
                continue;
            }

            // Which way the print runs on each face of the piece.
            //
            // Asked of the island as a whole this averages to nothing, because a banner is
            // drawn from both sides and a real one is *printed* on both: the two faces carry
            // the same picture and their `u` runs opposite ways in the world. So ask it of
            // each triangle. Its winding normal is the face a viewer would be looking at,
            // and `up x n` is that viewer's right; `u` either grows that way or the other,
            // and that — not the geometry — is what "facing the track" means for a board.
            let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            let (mut to_right, mut to_left) = (0usize, 0usize);
            for t in o.tri_start as usize..(o.tri_start + o.tri_count) as usize {
                let vi = |k: usize| mesh.indices[t * 3 + k] as usize;
                let pos = |k: usize| {
                    let i = vi(k);
                    [mesh.positions[i * 3], mesh.positions[i * 3 + 1], mesh.positions[i * 3 + 2]]
                };
                for k in 0..3 {
                    let i = vi(k);
                    let (u, v) = (mesh.uvs[i * 2], mesh.uvs[i * 2 + 1]);
                    (u0, u1) = (u0.min(u), u1.max(u));
                    (v0, v1) = (v0.min(v), v1.max(v));
                }
                let (a, b, c) = (pos(0), pos(1), pos(2));
                let e1: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
                let e2: [f32; 3] = std::array::from_fn(|k| c[k] - a[k]);
                let n = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let flat = (n[0] * n[0] + n[2] * n[2]).sqrt();
                // A face you can read: standing up, not the hem or a cap.
                if flat < 1e-6 || n[1].abs() > flat {
                    continue;
                }
                // The viewer's right, for someone this face is pointing at.
                let r = [n[2] / flat, -n[0] / flat];
                let along = |k: usize| pos(k)[0] * r[0] + pos(k)[2] * r[1];
                let mut best = (0.0f32, 0.0f32);
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let dt = along(j) - along(i);
                    if dt.abs() > best.0.abs() {
                        best = (dt, mesh.uvs[vi(j) * 2] - mesh.uvs[vi(i) * 2]);
                    }
                }
                if best.0.abs() < 1e-4 || best.1.abs() < 1e-6 {
                    continue;
                }
                if best.1 / best.0 > 0.0 {
                    to_right += 1;
                } else {
                    to_left += 1;
                }
            }
            if to_right + to_left == 0 {
                skipped += 1;
                continue;
            }
            let with_lap = (to_right as f32 - to_left as f32) / (to_right + to_left) as f32;
            boards.push(Board {
                along,
                off,
                with_lap,
                span,
                tall: h,
                box_key: (
                    (u0 * 20.0).round() as i32,
                    (u1 * 20.0).round() as i32,
                    (v0 * 20.0).round() as i32,
                    (v1 * 20.0).round() as i32,
                ),
            });
        }

        println!(
            "\n{}: {} banner boards, {skipped} too small to orient",
            entry,
            boards.len()
        );
        assert!(!boards.is_empty(), "no banner boards found — check the sheet vocabulary");

        // 1. The facing. `u` running with the lap on one side and against it on the other is
        //    a track that turns its print to face the riding line; the same sign on both is a
        //    track that does not care.
        println!("\n=== which way the print runs, per readable face ===");
        for (name, left) in [("left  (off < 0)", true), ("right (off > 0)", false)] {
            let mine: Vec<&Board> = boards.iter().filter(|b| (b.off < 0.0) == left).collect();
            if mine.is_empty() {
                continue;
            }
            let right = mine.iter().filter(|b| b.with_lap > 0.5).count();
            let lefty = mine.iter().filter(|b| b.with_lap < -0.5).count();
            let mixed = mine.len() - right - lefty;
            println!(
                "  {name}  n {:>4}   u to the viewer's right {right:>4}   to their left {lefty:>4}   \
                 mixed {mixed:>4}",
                mine.len(),
            );
        }

        // 3. And the picture, because everything above is a sign and this is the thing itself.
        //
        // Rebuild the widest board the way the viewer its own face is turned towards sees it:
        // across the image is that viewer's right, down the image is down the world. Run it on
        // a published track and on a generated one and the two pictures answer the question
        // outright — `scripts/track-render.py` cannot, because its camera and the game's
        // disagree about which way round the world is and it took a mirrored lap of banners
        // to notice.
        if let Ok(dump) = std::env::var("FROST_DUMP") {
            std::fs::create_dir_all(&dump).ok();
            let tex = map::textures(&bytes, 1024);
            let mut best: Option<(&map::MapObject, f32)> = None;
            for o in &mesh.objects {
                let named = sheets
                    .get(o.material as usize)
                    .map(|(n, ..)| n.to_ascii_lowercase())
                    .unwrap_or_default();
                let (w, h, d) = (o.max[0] - o.min[0], o.max[1] - o.min[1], o.max[2] - o.min[2]);
                let span = w.max(d);
                let (cx, cz) = ((o.min[0] + o.max[0]) * 0.5, (o.min[2] + o.max[2]) * 0.5);
                if classify(&named) != Class::Banner
                    || !(0.5..2.0).contains(&h)
                    || span < 1.0
                    || o.tri_count < 2
                    || nearest_at(cx, cz).1.abs() > 20.0
                {
                    continue;
                }
                if best.map(|(_, s)| span > s).unwrap_or(true) {
                    best = Some((o, span));
                }
            }
            let (o, _) = best.expect("a board to draw");
            // The face's own viewer stands where the winding normal points away from — that is
            // the rule the game culls by — and looks back along `n`. Their right is `up x n`,
            // which is the same relation the counts above are taken on.
            let tris: Vec<usize> = (o.tri_start as usize..(o.tri_start + o.tri_count) as usize)
                .collect();
            let at = |t: usize, k: usize| {
                let i = mesh.indices[t * 3 + k] as usize;
                (
                    [mesh.positions[i * 3], mesh.positions[i * 3 + 1], mesh.positions[i * 3 + 2]],
                    [mesh.uvs[i * 2], mesh.uvs[i * 2 + 1]],
                )
            };
            let t0 = tris[0];
            let (a, _) = at(t0, 0);
            let (b, _) = at(t0, 1);
            let (c, _) = at(t0, 2);
            let e1: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
            let e2: [f32; 3] = std::array::from_fn(|k| c[k] - a[k]);
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let flat = (n[0] * n[0] + n[2] * n[2]).sqrt();
            let r = [n[2] / flat, -n[0] / flat];
            // Only the triangles of that one face, so the copy behind it does not draw over it.
            let face: Vec<usize> = tris
                .iter()
                .copied()
                .filter(|t| {
                    let (a, _) = at(*t, 0);
                    let (b, _) = at(*t, 1);
                    let (c, _) = at(*t, 2);
                    let e1: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
                    let e2: [f32; 3] = std::array::from_fn(|k| c[k] - a[k]);
                    let m = [
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ];
                    m[0] * n[0] + m[1] * n[1] + m[2] * n[2] > 0.0
                })
                .collect();
            let across = |p: [f32; 3]| p[0] * r[0] + p[2] * r[1];
            let (mut x0, mut x1) = (f32::MAX, f32::MIN);
            for t in &face {
                for k in 0..3 {
                    let v = across(at(*t, k).0);
                    (x0, x1) = (x0.min(v), x1.max(v));
                }
            }
            let (out_w, out_h) = (720u32, 220u32);
            let t = tex.iter().find(|t| t.material == o.material);
            let mut img = image::RgbaImage::new(out_w, out_h);
            for py in 0..out_h {
                for px in 0..out_w {
                    let wx = x0 + (x1 - x0) * px as f32 / (out_w - 1) as f32;
                    let wy = o.min[1]
                        + (o.max[1] - o.min[1]) * (1.0 - py as f32 / (out_h - 1) as f32);
                    let mut buv = None;
                    for tri in &face {
                        let g = |k: usize| {
                            let (p, uv) = at(*tri, k);
                            ((across(p), p[1]), uv)
                        };
                        let (pa, ua) = g(0);
                        let (pb, ub) = g(1);
                        let (pc, uc) = g(2);
                        let area =
                            (pb.0 - pa.0) * (pc.1 - pa.1) - (pc.0 - pa.0) * (pb.1 - pa.1);
                        if area.abs() < 1e-9 {
                            continue;
                        }
                        let w0 = ((pb.0 - wx) * (pc.1 - wy) - (pc.0 - wx) * (pb.1 - wy)) / area;
                        let w1 = ((pc.0 - wx) * (pa.1 - wy) - (pa.0 - wx) * (pc.1 - wy)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < -1e-3 || w1 < -1e-3 || w2 < -1e-3 {
                            continue;
                        }
                        buv = Some((
                            ua[0] * w0 + ub[0] * w1 + uc[0] * w2,
                            ua[1] * w0 + ub[1] * w1 + uc[1] * w2,
                        ));
                        break;
                    }
                    let col = match (buv, t) {
                        (Some(uv), Some(t)) if t.width > 0 => {
                            let sx = (uv.0.rem_euclid(1.0) * t.width as f32) as u32 % t.width;
                            let sy =
                                ((1.0 - uv.1.rem_euclid(1.0)) * t.height as f32) as u32 % t.height;
                            let i = ((sy * t.width + sx) * 4) as usize;
                            [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2], 255]
                        }
                        _ => [90, 90, 96, 255],
                    };
                    img.put_pixel(px, py, image::Rgba(col));
                }
            }
            let file = format!("{dump}/face-as-its-viewer-sees-it.png");
            img.save(&file).unwrap();
            println!("\nthe widest board, from the side its face is turned to: {file}");
        }

        // 2. The repeat. How many boards print the same UV box, and how far apart they are —
        //    which is the difference between a run of sponsors and one banner tiled.
        let mut by_box: std::collections::HashMap<(i32, i32, i32, i32), Vec<&Board>> =
            Default::default();
        for b in &boards {
            by_box.entry(b.box_key).or_default().push(b);
        }
        let mut order: Vec<_> = by_box.into_iter().collect();
        order.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        println!("\n=== how a board repeats ({} distinct prints) ===", order.len());
        for (key, mut group) in order.into_iter().take(6) {
            group.sort_by(|a, b| a.along.partial_cmp(&b.along).unwrap());
            let mut gaps: Vec<f32> = group
                .windows(2)
                .filter(|w| (w[0].off < 0.0) == (w[1].off < 0.0))
                .map(|w| w[1].along - w[0].along)
                .filter(|g| *g > 0.05 && *g < 30.0)
                .collect();
            let mut span: Vec<f32> = group.iter().map(|b| b.span).collect();
            let mut tall: Vec<f32> = group.iter().map(|b| b.tall).collect();
            let mut off: Vec<f32> = group.iter().map(|b| b.off.abs()).collect();
            println!(
                "  uv {:>5.2}-{:<5.2} x {:>5.2}-{:<5.2}  n {:>4}  wide {:>4.2} m  tall {:>4.2} m  \
                 off {:>4.1} m  gap {:>4.2}/{:>4.2}/{:>4.2} m",
                key.0 as f32 / 20.0, key.1 as f32 / 20.0,
                key.2 as f32 / 20.0, key.3 as f32 / 20.0,
                group.len(),
                spread(&mut span).p50,
                spread(&mut tall).p50,
                spread(&mut off).p50,
                spread(&mut gaps).p10, spread(&mut gaps).p50, spread(&mut gaps).p90,
            );
        }
    }
}
