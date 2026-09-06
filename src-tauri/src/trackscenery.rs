//! What a generated track stands beside its riding line.
//!
//! `terrained.exe` places objects from `scene<N>` blocks in the `.hmf` and bakes their meshes
//! into the `.map`; the same blocks in the `.tht` give them collision. So this module has two
//! jobs: build the `.edf` models ([`crate::edfwrite`]) and decide where they go.
//!
//! Where they go is not invented. [`crate::trackobjects`] measured twelve published tracks and
//! every figure below is one of its numbers — the offset from the centreline, the gap round
//! the lap, the height. The single clearest of them is that **every track lines its riding
//! line with something about a metre tall, 7.5 m out, one every three metres**, and on most of
//! them it runs the whole lap. That is the edge a rider sees, and it is the first thing here.
//!
//! Everything of one kind is merged into **one node of one model**, placed once at the origin
//! in world metres. That is how published tracks do it — their `main_track_objects_c` is a
//! single baked mesh, which is why a `.map` comes apart into a hundred thousand islands and
//! not a hundred thousand draw calls — and it means a lap's worth of markers costs one
//! `scene` block rather than six hundred.

#![allow(dead_code)]

use crate::edfwrite::{self, Mesh, Part, Texture};
use crate::trackprog::TrackProgram;
use crate::tracksynth::Synth;

/// Metres out from the centreline for the line of edge markers. The corpus median is 7.5;
/// this keeps it just clear of the shoulder on a wide track.
const MARKER_OFF_M: f32 = 7.5;
/// Metres round the lap between markers. Corpus gap p50 is 3.1 m.
const MARKER_GAP_M: f32 = 3.0;
/// How tall a marker stands. Corpus p50 is 1.1 m.
const MARKER_H_M: f32 = 1.1;

/// The fence, at the corpus median of 19.2 m and 2.9 m tall.
const FENCE_OFF_M: f32 = 19.2;
const FENCE_H_M: f32 = 2.9;
/// One panel, so the run is built from repeats the way a real fence is.
const FENCE_PANEL_M: f32 = 2.5;

/// Bales guard what a rider would otherwise hit. Corpus offset runs 10.9–34.6; the near end
/// of that is where they do any good.
const BALE_OFF_M: f32 = 11.0;
const BALE_W_M: f32 = 1.2;
const BALE_H_M: f32 = 1.0;
const BALE_D_M: f32 = 0.8;

/// Trees stand back. Corpus p50 offset 36.8 m, p50 gap 18.3 m, p50 height 3.9 m — those are
/// the ones you ride past, not the backdrop ring beyond 60 m.
const TREE_FROM_M: f32 = 26.0;
const TREE_TO_M: f32 = 95.0;
const TREE_SPACING_M: f32 = 13.0;
const TREE_H_M: f32 = 6.5;

/// Deterministic noise, so a track built twice is the same track.
fn rnd(seed: u32, i: u32) -> f32 {
    let mut h = seed ^ i.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    (h % 100_000) as f32 / 100_000.0
}

/// One model placed in the world.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The `.edf` beside the `.hmf`.
    pub file: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// What a track's scenery amounts to: the files to write, and the blocks that place them.
pub struct Scenery {
    /// `name.edf` against its bytes.
    pub files: Vec<(String, Vec<u8>)>,
    /// Placed in the `.hmf`, so drawn.
    pub drawn: Vec<Scene>,
    /// Placed in the `.tht` as well, so ridden into. Only what should stop a bike.
    pub solid: Vec<Scene>,
    /// What went where, for the log and for measuring the result back.
    pub tally: Vec<(&'static str, usize)>,
}

/// Height of the built ground at a world point, bilinear.
fn ground(syn: &Synth, x: f32, z: f32) -> f32 {
    let (gx, gz) = (x / syn.mps, z / syn.mps);
    let (x0, z0) = (gx.floor() as isize, gz.floor() as isize);
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let at = |ix: isize, iz: isize| -> f32 {
        let ix = ix.clamp(0, syn.gw as isize - 1) as usize;
        let iz = iz.clamp(0, syn.gh as isize - 1) as usize;
        syn.heights[iz * syn.gw + ix]
    };
    let (a, b) = (at(x0, z0), at(x0 + 1, z0));
    let (c, d) = (at(x0, z0 + 1), at(x0 + 1, z0 + 1));
    (a + (b - a) * fx) * (1.0 - fz) + (c + (d - c) * fx) * fz
}

/// How far a point is from the nearest part of the riding line — any part of it, not the
/// station it was placed from.
///
/// A lap folds back on itself. A tree put twenty-six metres to the side of one station lands
/// a metre from another, and measured only against its own station it looks correctly placed
/// right up until you ride into it. This is the check that catches that, and everything is
/// held to it.
fn clearance(coarse: &[crate::trackprog::Station], x: f32, z: f32) -> f32 {
    coarse
        .iter()
        .map(|st| (st.x - x).powi(2) + (st.z - z).powi(2))
        .fold(f32::INFINITY, f32::min)
        .sqrt()
}

/// The lowest ground under a footprint `r` metres across.
///
/// A card is placed by one point but stands on a width of ground, and on anything sloping the
/// far corner is the one that matters: sampling the centre alone buried a seven-metre tree
/// card a metre into a hillside. Taking the minimum leaves a gap under the high side, which
/// is what a real tree looks like anyway.
fn ground_min(syn: &Synth, x: f32, z: f32, r: f32) -> f32 {
    let mut lo = f32::INFINITY;
    for (dx, dz) in [(0.0, 0.0), (-r, 0.0), (r, 0.0), (0.0, -r), (0.0, r), (-r, -r), (r, r), (-r, r), (r, -r)] {
        lo = lo.min(ground(syn, x + dx, z + dz));
    }
    lo
}

/// The lowest and highest ground under a panel of length `len` centred on `(x, z)` and
/// running along `deg`.
///
/// A disc of samples is the wrong shape for a fence panel: it is a line, and what buries it
/// is the ground at its two ends. Sampled as a disc a panel across a two-metre step read as
/// level and sank a metre into the bank.
fn ground_span(syn: &Synth, x: f32, z: f32, deg: f32, len: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for k in 0..=8 {
        let t = (k as f32 / 8.0 - 0.5) * len;
        let g = ground(syn, x + c * t, z - s * t);
        lo = lo.min(g);
        hi = hi.max(g);
    }
    (lo, hi)
}

/// The lowest and highest ground under a rectangle `w` by `d`, turned to `deg`.
fn ground_foot(syn: &Synth, x: f32, z: f32, deg: f32, w: f32, d: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for a in -1..=1 {
        for b in -1..=1 {
            let (u, v) = (a as f32 * w * 0.5, b as f32 * d * 0.5);
            let g = ground(syn, x + c * u + s * v, z - s * u + c * v);
            lo = lo.min(g);
            hi = hi.max(g);
        }
    }
    (lo, hi)
}

/// Whether a world point is inside the terrain square, with a margin.
fn inside(prog: &TrackProgram, x: f32, z: f32, margin: f32) -> bool {
    x > margin
        && z > margin
        && x < prog.terrain.size_x - margin
        && z < prog.terrain.size_z - margin
}

// ---------------------------------------------------------------------------
// Sheets
// ---------------------------------------------------------------------------

fn sheet(name: &str, dim: u32, f: impl Fn(f32, f32) -> [u8; 4]) -> Texture {
    let mut rgba = Vec::with_capacity((dim * dim * 4) as usize);
    for y in 0..dim {
        for x in 0..dim {
            let (u, v) = (x as f32 / dim as f32, y as f32 / dim as f32);
            rgba.extend_from_slice(&f(u, v));
        }
    }
    Texture { name: name.into(), width: dim, height: dim, rgba }
}

fn grain(u: f32, v: f32, seed: u32, scale: f32) -> f32 {
    let i = ((u * scale) as u32) ^ (((v * scale) as u32) << 8);
    rnd(seed, i)
}

/// The marker panel: a coloured board on a short post, which is what a track's edge line
/// mostly is. Alpha cuts the board out of the sheet so the post reads as a post.
fn marker_sheet() -> Texture {
    sheet("marker_c_a", 64, |u, v| {
        // Bottom fifth is the post, the rest the board.
        if v > 0.8 {
            if (u - 0.5).abs() < 0.06 {
                [70, 70, 74, 255]
            } else {
                [0, 0, 0, 0]
            }
        } else if v > 0.06 && (0.04..0.96).contains(&u) {
            let stripe = ((v * 5.0) as u32) % 2 == 0;
            if stripe {
                [206, 70, 32, 255]
            } else {
                [236, 236, 232, 255]
            }
        } else {
            [0, 0, 0, 0]
        }
    })
}

/// Mesh fencing: a grid you can see through, which is the whole reason it is a cut-out.
fn fence_sheet() -> Texture {
    sheet("fence_c_a", 64, |u, v| {
        let line = |t: f32| (t * 16.0).fract() < 0.22;
        // Posts down the sides and a rail top and bottom keep the panel readable at distance.
        let frame = v < 0.05 || v > 0.95 || u < 0.03 || u > 0.97;
        if frame {
            [120, 122, 126, 255]
        } else if line(u) || line(v) {
            [150, 152, 156, 210]
        } else {
            [0, 0, 0, 0]
        }
    })
}

fn bale_sheet() -> Texture {
    sheet("bale_c", 64, |u, v| {
        let g = grain(u, v, 0x51A7, 32.0);
        let band = (v * 3.0).fract() < 0.12;
        let base = if band { [180, 160, 60] } else { [214, 196, 96] };
        [
            (base[0] as f32 * (0.86 + 0.2 * g)) as u8,
            (base[1] as f32 * (0.86 + 0.2 * g)) as u8,
            (base[2] as f32 * (0.86 + 0.2 * g)) as u8,
            255,
        ]
    })
}

/// Foliage, cut out of its sheet. A canopy that fills the quad reads as a green slab from
/// every angle; the cut-out is what makes it a tree.
fn leaf_sheet() -> Texture {
    sheet("leaf_c_a", 128, |u, v| {
        // A rough crown: dense in the middle, ragged at the edge, with a trunk below it.
        let (cx, cy) = (u - 0.5, v - 0.38);
        let r = (cx * cx + cy * cy * 1.35).sqrt();
        let ragged = 0.36 + 0.09 * (grain(u, v, 0x2C41, 22.0) - 0.5) * 2.0;
        if v > 0.72 {
            if (u - 0.5).abs() < 0.035 {
                let g = grain(u, v, 0x77B1, 40.0);
                [(74.0 + 26.0 * g) as u8, (58.0 + 18.0 * g) as u8, (44.0 + 14.0 * g) as u8, 255]
            } else {
                [0, 0, 0, 0]
            }
        } else if r < ragged {
            let g = grain(u, v, 0x3D19, 26.0);
            let shade = 0.62 + 0.38 * g;
            [
                (58.0 * shade) as u8,
                (104.0 * shade) as u8,
                (46.0 * shade) as u8,
                255,
            ]
        } else {
            [0, 0, 0, 0]
        }
    })
}

fn gate_sheet() -> Texture {
    sheet("gate_c", 64, |u, v| {
        let g = grain(u, v, 0x9E11, 24.0);
        if v < 0.28 {
            [(30.0 + 30.0 * g) as u8, (60.0 + 40.0 * g) as u8, (120.0 + 40.0 * g) as u8, 255]
        } else {
            [(190.0 + 40.0 * g) as u8, (190.0 + 40.0 * g) as u8, (186.0 + 40.0 * g) as u8, 255]
        }
    })
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// Build a track's scenery: the models, and where they stand.
pub fn build(prog: &TrackProgram, syn: &Synth) -> Scenery {
    let seed = prog.terrain.relief.seed;
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    let stations = prog.stations(0.5);
    // Every candidate is measured against the whole lap, not against the station it came
    // from. Two metres a step is finer than anything being placed is wide.
    let coarse = prog.stations(2.0);
    let at = |s: f32| -> crate::trackprog::Station {
        let i = ((s / 0.5) as usize).min(stations.len().saturating_sub(1));
        stations[i]
    };

    let mut markers = Mesh::default();
    let mut fence = Mesh::default();
    let mut bales = Mesh::default();
    let mut trees = Mesh::default();
    let mut gate = Mesh::default();
    let mut tally = Vec::new();

    // 1. The edge line. Both sides, the whole lap. This is the thing every track has.
    let marker_off = (half + 1.0).max(MARKER_OFF_M);
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        for side in [-1.0f32, 1.0] {
            let (x, z) = (st.x + rx * marker_off * side, st.z + rz * marker_off * side);
            // Its own station is `marker_off` away, so anything nearer than that is another
            // part of the lap and the marker would be standing on the track.
            if !inside(prog, x, z, 2.0) || clearance(&coarse, x, z) < marker_off - 1.0 {
                continue;
            }
            // A hand-planted line is not a ruler: a little jitter either way, and the boards
            // face across the track rather than all one way.
            let jitter = (rnd(seed ^ 0x11, n as u32) - 0.5) * 0.4;
            let card = edfwrite::card(0.5, MARKER_H_M * (0.9 + 0.2 * rnd(seed ^ 0x12, n as u32)));
            let deg = st.heading.to_degrees() + 90.0 + jitter * 20.0;
            markers.append(&edfwrite::moved(
                &edfwrite::turned(&card, deg),
                [x, ground(syn, x, z) - 0.05, z],
            ));
            n += 1;
        }
        s += MARKER_GAP_M;
    }
    tally.push(("markers", n));

    // 2. The fence, further out and continuous.
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        for side in [-1.0f32, 1.0] {
            let off = FENCE_OFF_M + (rnd(seed ^ 0x21, n as u32) - 0.5) * 2.0;
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            if !inside(prog, x, z, 4.0) || clearance(&coarse, x, z) < half + 4.0 {
                continue;
            }
            let deg = st.heading.to_degrees() + 90.0;
            let (lo, hi) = ground_span(syn, x, z, deg, FENCE_PANEL_M);
            // A fence is not built across a step. Where the ground under a panel moves more
            // than this the run simply has a gap, which is what happens on a real one.
            if hi - lo > 1.0 {
                continue;
            }
            // Set on the mean of its two ends rather than on either. A panel is a flat board
            // and the ground is not: put it on the low end and the high end buries it, put it
            // on the high end and the low end leaves it in the air. Halfway is the bottom
            // rail dipping in and out of the dirt, which is what a fence on rolling ground
            // actually does.
            let panel = edfwrite::card(FENCE_PANEL_M, FENCE_H_M);
            fence.append(&edfwrite::moved(
                &edfwrite::turned(&panel, deg),
                [x, (lo + hi) * 0.5 - 0.05, z],
            ));
            n += 1;
        }
        s += FENCE_PANEL_M;
    }
    tally.push(("fence panels", n));

    // 3. Bales where a rider leaves the track fastest: the outside of every corner.
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        if st.curvature.abs() > 1.0 / 45.0 {
            // Outside of the turn is away from the way it bends.
            let side = -st.curvature.signum();
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let off = BALE_OFF_M + (rnd(seed ^ 0x31, n as u32) - 0.5) * 2.0;
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            if inside(prog, x, z, 3.0) && clearance(&coarse, x, z) > half + 2.0 {
                let deg = st.heading.to_degrees() + 12.0 * (rnd(seed ^ 0x32, n as u32) - 0.5);
                // A bale is a flat-bottomed box and the outside of a corner is the most
                // sloped ground on the track. Where its own footprint is too far off level
                // nobody would have stacked one there, so it isn't stacked there.
                let (lo, hi) = ground_foot(syn, x, z, deg, BALE_W_M, BALE_D_M);
                if hi - lo > 0.8 {
                    s += 4.0;
                    continue;
                }
                bales.append(&edfwrite::moved(
                    &edfwrite::turned(&edfwrite::cuboid(BALE_W_M, BALE_H_M, BALE_D_M), deg),
                    [x, (lo + hi) * 0.5, z],
                ));
                n += 1;
            }
        }
        s += 4.0;
    }
    tally.push(("bales", n));

    // 4. Trees, out past the fence, thinned so they don't line up with the lap.
    let mut n = 0usize;
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        for k in 0..3u32 {
            let i = (n as u32) * 7 + k;
            if rnd(seed ^ 0x41, i) > 0.45 {
                continue;
            }
            let side = if rnd(seed ^ 0x42, i) < 0.5 { -1.0 } else { 1.0 };
            let off = TREE_FROM_M + (TREE_TO_M - TREE_FROM_M) * rnd(seed ^ 0x43, i);
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            if !inside(prog, x, z, 6.0) || clearance(&coarse, x, z) < TREE_FROM_M - 6.0 {
                continue;
            }
            // Nothing where the track is, and nothing on top of another tree.
            if placed
                .iter()
                .any(|(px, pz)| (px - x).powi(2) + (pz - z).powi(2) < TREE_SPACING_M.powi(2))
            {
                continue;
            }
            placed.push((x, z));
            let h = TREE_H_M * (0.7 + 0.6 * rnd(seed ^ 0x44, i));
            let w = h * 0.75;
            let tree = edfwrite::crossed(w, h, 3);
            trees.append(&edfwrite::moved(
                &edfwrite::turned(&tree, 360.0 * rnd(seed ^ 0x45, i)),
                [x, ground_min(syn, x, z, w * 0.5) - 0.1, z],
            ));
            n += 1;
        }
        s += TREE_SPACING_M;
    }
    tally.push(("trees", n));

    // 5. The start structure, over the gate row.
    let st = at(2.0);
    let (rx, rz) = crate::trackprog::right_vector(st.heading);
    let (fx, fz) = crate::trackprog::heading_vector(st.heading);
    // One foot for the whole structure: a gantry with a post at each end is level, and
    // levelling it on the lower ground is what keeps the other post out of the air.
    // Each post stands on its own ground — level them together and one is buried and the
    // other in the air — and the beam clears the higher of the two.
    let mut highest = f32::NEG_INFINITY;
    for side in [-1.0f32, 1.0] {
        let (x, z) = (st.x + rx * (half + 3.0) * side, st.z + rz * (half + 3.0) * side);
        let foot = ground(syn, x, z);
        highest = highest.max(foot);
        gate.append(&edfwrite::moved(&edfwrite::cuboid(0.5, 5.0, 0.5), [x, foot, z]));
    }
    let span = (half + 3.0) * 2.0;
    let beam = edfwrite::turned(&edfwrite::cuboid(span, 1.2, 0.3), st.heading.to_degrees() + 90.0);
    gate.append(&edfwrite::moved(&beam, [st.x, highest + 4.6, st.z]));
    let _ = (fx, fz);
    tally.push(("gate", 1));

    // One model, one node a kind. Empty kinds are dropped so nothing writes a node with no
    // geometry in it.
    let kinds: Vec<(&str, Mesh, usize)> = vec![
        ("markers", markers, 0),
        ("fence", fence, 1),
        ("bales", bales, 2),
        ("trees", trees, 3),
        ("gate", gate, 4),
    ];
    let sheets = vec![
        marker_sheet(),
        fence_sheet(),
        bale_sheet(),
        leaf_sheet(),
        gate_sheet(),
    ];
    let parts: Vec<Part> = kinds
        .into_iter()
        .filter(|(_, m, _)| m.vertex_count() >= 8)
        .map(|(name, mesh, texture)| Part { name: name.into(), mesh, texture })
        .collect();

    let bytes = edfwrite::write(&parts, &sheets);
    let at_origin = Scene {
        file: "scenery.edf".into(),
        pos: [0.0, 0.0, 0.0],
        rot: [0.0, 0.0, 0.0],
    };

    // Collision is a second model holding only what should stop a bike: the bales and the
    // gate posts. Foliage and the marker line are cards — solid, they would be invisible
    // walls, and a rider who clips a track-edge board should ride on.
    let solid_parts: Vec<Part> = parts
        .iter()
        .filter(|p| p.name == "bales" || p.name == "gate")
        .cloned()
        .collect();
    let mut files = vec![("scenery.edf".to_string(), bytes)];
    let mut solid = Vec::new();
    if !solid_parts.is_empty() {
        files.push((
            "solids.edf".to_string(),
            edfwrite::write(&solid_parts, &sheets),
        ));
        solid.push(Scene {
            file: "solids.edf".into(),
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0],
        });
    }

    Scenery { files, drawn: vec![at_origin], solid, tally }
}

/// The `scene<N>` blocks, in the form TerrainEd reads them.
pub fn blocks(scenes: &[Scene]) -> String {
    let mut s = String::new();
    for (i, sc) in scenes.iter().enumerate() {
        s.push_str(&format!(
            "\nscene{i}\n{{\n\tname = {}\n\tpos\n\t{{\n\t\tx = {:.3}\n\t\ty = {:.3}\n\t\tz = {:.3}\n\t}}\n\
             \trot\n\t{{\n\t\tx = {:.3}\n\t\ty = {:.3}\n\t\tz = {:.3}\n\t}}\n}}\n",
            sc.file, sc.pos[0], sc.pos[1], sc.pos[2], sc.rot[0], sc.rot[1], sc.rot[2],
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> (TrackProgram, Synth) {
        let p: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        let s = crate::tracksynth::synthesise(&p).unwrap();
        (p, s)
    }

    #[test]
    fn a_lap_gets_an_edge_line_the_whole_way_round() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let markers = sc.tally.iter().find(|(k, _)| *k == "markers").unwrap().1;
        // Both sides, one every three metres — the corpus figure.
        let want = (p.lap_length() / MARKER_GAP_M * 2.0) as usize;
        assert!(
            markers > want * 8 / 10,
            "{markers} markers for a {:.0} m lap, expected about {want}",
            p.lap_length()
        );
    }

    #[test]
    fn nothing_stands_on_the_riding_line() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let bytes = &sc.files[0].1;
        let nodes = crate::edf::parse_world(bytes);
        assert!(!nodes.is_empty(), "the model has nodes");

        // Every vertex, against the corridor the track was built with. The edge line sits
        // just outside the shoulder; nothing may sit inside the riding surface itself.
        let stations = p.stations(1.0);
        let half = p.width * 0.5;
        // Anything a rider's head would pass under may cross the line — the start gantry is
        // meant to. What matters is that nothing is in the way at riding height.
        let ride_h = 3.0;
        let mut worst = f32::INFINITY;
        for n in &nodes {
            for v in n.positions.chunks_exact(3) {
                let g = ground(&s, v[0], v[2]);
                if v[1] - g > ride_h {
                    continue;
                }
                let d = stations
                    .iter()
                    .map(|st| ((st.x - v[0]).powi(2) + (st.z - v[2]).powi(2)).sqrt())
                    .fold(f32::INFINITY, f32::min);
                worst = worst.min(d);
            }
        }
        assert!(
            worst > half,
            "something stands {worst:.1} m from the centreline, inside a {half:.1} m half-width"
        );
    }

    #[test]
    fn everything_stands_on_the_ground_it_was_placed_on() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let nodes = crate::edf::parse_world(&sc.files[0].1);

        // How far a thing's foot may sit under the surface, by kind. Not one number: a
        // seven-metre foliage card is placed on the lowest ground it covers precisely so it
        // never floats, and its far corner going under a rise is what that costs — and what
        // you want, because a tree hovering over a bank is the thing you would notice. A
        // fence panel or a gate post is built, flat-footed and man-high, and one sunk half a
        // metre reads as broken.
        let allowed = |name: &str| if name == "trees" { 1.5f32 } else { 0.6 };

        let mut worst: (f32, String, [f32; 3]) = (0.0, String::new(), [0.0; 3]);
        for n in &nodes {
            let limit = allowed(&n.name);
            for v in n.positions.chunks_exact(3) {
                let under = ground(&s, v[0], v[2]) - v[1];
                if under > limit && under - limit > worst.0 - allowed(&worst.1) {
                    worst = (under, n.name.clone(), [v[0], v[1], v[2]]);
                }
            }
        }
        assert!(
            worst.1.is_empty(),
            "{} is buried {:.2} m at {:?}, over its {:.1} m allowance",
            worst.1, worst.0, worst.2, allowed(&worst.1)
        );
    }

    #[test]
    fn the_blocks_read_the_way_terrained_writes_them() {
        let s = blocks(&[Scene {
            file: "scenery.edf".into(),
            pos: [1.0, 2.0, 3.0],
            rot: [0.0, 90.0, 0.0],
        }]);
        assert!(s.contains("scene0"));
        assert!(s.contains("name = scenery.edf"));
        assert!(s.contains("x = 1.000") && s.contains("y = 2.000") && s.contains("z = 3.000"));
        assert!(s.contains("y = 90.000"));
    }

    #[test]
    fn collision_is_only_what_should_stop_a_bike() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        assert_eq!(sc.solid.len(), 1, "one collision model");
        let names: Vec<String> = crate::edf::parse_world(&sc.files[1].1)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert!(names.iter().any(|n| n == "bales"), "{names:?}");
        assert!(!names.iter().any(|n| n == "trees"), "foliage cards are not walls: {names:?}");
    }
}

#[cfg(test)]
mod built {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn u32b(v: usize) -> [u8; 4] {
        (v as u32).to_le_bytes()
    }

    /// Build the demo track with its objects, compile it, package it, and dump what came out
    /// in a form a renderer can draw.
    ///
    /// ```text
    /// FROST_BUILD=/tmp/objtrack FROST_TOOLS=~/Downloads/mxb-trackbuild/tools \
    /// FROST_PREFIX=~/Downloads/mxb-trackbuild/prefix FROST_WINE=".../wine" \
    /// FROST_INSTALL="~/Documents/MX Bikes/mods/tracks" \
    ///   cargo test -- --ignored --nocapture builds_a_track_with_objects
    /// ```
    #[test]
    #[ignore = "writes and compiles a track — set FROST_BUILD"]
    fn builds_a_track_with_objects() {
        let dir = PathBuf::from(std::env::var("FROST_BUILD").expect("set FROST_BUILD"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p: TrackProgram = serde_json::from_str(crate::trackprog::EXAMPLE).unwrap();
        let syn = crate::tracksynth::synthesise(&p).unwrap();
        let slug = crate::tracksynth::write_source(&p, &syn, &dir).unwrap();
        println!("{}: {:.0} m lap, {} files", p.name, p.lap_length(), slug.len());

        let sc = build(&p, &syn);
        for (k, n) in &sc.tally {
            println!("  {k:<14} {n}");
        }
        for (name, bytes) in &sc.files {
            println!("  {name:<14} {} KB", bytes.len() / 1024);
        }

        let tools = PathBuf::from(std::env::var("FROST_TOOLS").expect("set FROST_TOOLS"));
        let t = crate::trackbuild::find(&tools).expect("compilers under FROST_TOOLS");
        let stem = dir
            .join("track.hmf")
            .exists()
            .then(|| slug.iter().find_map(|f| f.strip_suffix(".map").map(|s| s.to_string())))
            .flatten()
            .unwrap_or_default();
        let name = stem.split('/').next_back().unwrap_or("track").to_string();
        println!("slug: {name}");

        let map_rel = format!("{name}/{name}.map");
        let trh_rel = format!("{name}/{name}.trh");
        std::fs::create_dir_all(dir.join(&name)).ok();
        for (label, args) in [
            ("map", vec!["track.hmf", map_rel.as_str(), "params.ini"]),
            ("trh", vec!["track.tht", trh_rel.as_str(), "trh_params.ini"]),
        ] {
            let out = run(&t.terrained, &args, &dir);
            println!("--- {label} ---\n{out}");
        }
        if let Some(tracked) = &t.tracked {
            let out = run(
                tracked,
                &["-merge", &trh_rel, "cl", "track.tcl", "sa", "track_start.tcl"],
                &dir,
            );
            println!("--- centreline ---\n{out}");
        }

        let map_path = dir.join(&map_rel);
        assert!(map_path.is_file(), "no {map_rel} was written");
        let mb = std::fs::read(&map_path).unwrap();
        let mesh = crate::map::parse(&mb).expect("the compiled .map parses");
        let sheets = crate::map::declared(&mb);
        println!(
            "\ncompiled: {} materials, {} vertices, {} triangles, {} islands\nsheets: {:?}",
            mesh.materials,
            mesh.vertex_count(),
            mesh.triangle_count(),
            mesh.objects.len(),
            sheets.iter().map(|(n, ..)| n).collect::<Vec<_>>()
        );
        assert!(mesh.triangle_count() > 1000, "the scenery didn't reach the map");

        // Package, and install where the game lists it.
        let pkz = dir.join(format!("{name}.pkz"));
        let size = crate::trackbuild::package(&dir, &name, &pkz).unwrap();
        println!("packaged {} KB", size / 1024);
        if let Ok(to) = std::env::var("FROST_INSTALL") {
            let at = crate::trackbuild::install(&pkz, Path::new(&to)).unwrap();
            println!("installed {at:?}");
        }

        dump(&dir.join("scene.bin"), &p, &syn, &mesh, &mb);
        println!("dumped {:?}", dir.join("scene.bin"));
    }

    /// Everything a renderer needs, in one file: the ground, the riding line, the scenery
    /// mesh and the sheets it wears. Written so the picture is of what was *compiled*, not of
    /// what we meant to compile.
    fn dump(
        to: &Path,
        p: &TrackProgram,
        syn: &Synth,
        mesh: &crate::map::MapMesh,
        map_bytes: &[u8],
    ) {
        let mut f = std::fs::File::create(to).unwrap();
        let mut w = |b: &[u8]| f.write_all(b).unwrap();
        w(b"SDMP");
        w(&u32b(syn.gw));
        w(&u32b(syn.gh));
        w(&syn.mps.to_le_bytes());
        w(&p.terrain.size_x.to_le_bytes());
        for v in &syn.heights {
            w(&v.to_le_bytes());
        }
        for c in &syn.corridor {
            w(&[*c as u8]);
        }

        // Sheets, reduced so the dump stays small — enough to tell a leaf from a fence.
        const DIM: u32 = 64;
        let textures = crate::map::textures(map_bytes, 256);
        let count = mesh.materials.max(1) as usize;
        w(&u32b(count));
        for m in 0..count {
            let t = textures.iter().find(|t| t.material == m as u32);
            w(&u32b(DIM as usize));
            for y in 0..DIM {
                for x in 0..DIM {
                    let px = match t {
                        Some(t) if t.width > 0 && t.height > 0 => {
                            let sx = (x * t.width / DIM).min(t.width - 1) as usize;
                            let sy = (y * t.height / DIM).min(t.height - 1) as usize;
                            let i = (sy * t.width as usize + sx) * 4;
                            [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2], t.rgba[i + 3]]
                        }
                        _ => [170, 170, 170, 255],
                    };
                    w(&px);
                }
            }
        }

        w(&u32b(mesh.vertex_count()));
        for v in &mesh.positions {
            w(&v.to_le_bytes());
        }
        for v in &mesh.uvs {
            w(&v.to_le_bytes());
        }
        // One material per triangle, from the group runs.
        let tris = mesh.triangle_count();
        let mut mat = vec![0u32; tris];
        for g in &mesh.groups {
            for t in g.tri_start as usize..(g.tri_start + g.tri_count) as usize {
                if t < tris {
                    mat[t] = g.material;
                }
            }
        }
        w(&u32b(tris));
        for i in &mesh.indices {
            w(&i.to_le_bytes());
        }
        for m in &mat {
            w(&m.to_le_bytes());
        }
    }

    fn run(exe: &Path, args: &[&str], dir: &Path) -> String {
        let mut cmd = if cfg!(target_os = "windows") {
            std::process::Command::new(exe)
        } else {
            let mut c = std::process::Command::new(
                std::env::var("FROST_WINE").expect("set FROST_WINE"),
            );
            c.env("WINEPREFIX", std::env::var("FROST_PREFIX").expect("set FROST_PREFIX"));
            c.env("WINEDEBUG", "-all");
            c.arg(exe);
            c
        };
        cmd.args(args).current_dir(dir);
        let out = cmd.output().expect("running the compiler");
        format!(
            "exit {:?}\n{}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }
}
