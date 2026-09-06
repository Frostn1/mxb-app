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

/// The line of stakes that marks the edge of the riding line, metres out. The corpus median
/// for what lines a track is 7.5; this keeps it just clear of the shoulder on a wide track.
const STAKE_OFF_M: f32 = 7.5;
/// Metres round the lap between stakes.
///
/// Not the corpus's 3.1. That figure is the gap between *islands*, and an island is one quad
/// of a baked mesh rather than one object — so a banner made of four quads counted as four
/// banners three metres apart. Read literally it put 1272 boards round a lap and the track
/// looked fenced in by its own edge marking.
const STAKE_GAP_M: f32 = 6.0;
const STAKE_H_M: f32 = 1.15;
/// A stake is a stake: thin enough that what you see is a line of points, not a wall.
const STAKE_W_M: f32 = 0.07;

/// Banners are the big printed panels, and there are few of them.
const BANNER_OFF_M: f32 = 9.5;
const BANNER_GAP_M: f32 = 55.0;
const BANNER_W_M: f32 = 4.0;
const BANNER_H_M: f32 = 1.2;
/// How far off the ground the panel is slung.
const BANNER_LIFT_M: f32 = 0.35;

/// The fence, at the corpus median of 19.2 m and 2.9 m tall.
const FENCE_OFF_M: f32 = 19.2;
const FENCE_H_M: f32 = 2.9;
/// One panel. Wide, because a fence built from narrow boards reads as a crowd of boards.
const FENCE_PANEL_M: f32 = 4.0;

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

/// Which way to turn a mesh so its own X axis runs along a heading.
///
/// [`edfwrite::turned`] sends local X to `right_vector(deg)` and a card's normal to
/// `heading_vector(deg)`. So a thing whose length should run *along* the track — a fence
/// panel, a bale, a board facing the rider — turns by `heading + 90`, and a thing that should
/// span *across* it — the gantry beam — turns by `heading`. Getting that backwards is what
/// laid the finish gantry along the track instead of over it, and stood every bale end-on.
fn along(heading_rad: f32) -> f32 {
    heading_rad.to_degrees() + 90.0
}

fn across(heading_rad: f32) -> f32 {
    heading_rad.to_degrees()
}

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

/// A stake: bare timber with the top painted, which is what marks the edge of every
/// motocross track there is.
fn stake_sheet() -> Texture {
    sheet("stake_c", 32, |u, v| {
        let g = grain(u, v * 0.25, 0x33A1, 26.0);
        if v < 0.34 {
            // The painted top, so a line of them reads at speed.
            let s = 0.88 + 0.12 * g;
            [(238.0 * s) as u8, (238.0 * s) as u8, (232.0 * s) as u8, 255]
        } else {
            let s = 0.80 + 0.30 * g;
            [(166.0 * s) as u8, (132.0 * s) as u8, (88.0 * s) as u8, 255]
        }
    })
}

/// A printed banner panel, cut out of its sheet so the sling above it reads as rope.
fn banner_sheet() -> Texture {
    sheet("banner_c_a", 128, |u, v| {
        if v < 0.12 {
            // The tie line along the top.
            return if (u * 40.0).fract() < 0.55 { [40, 40, 44, 255] } else { [0, 0, 0, 0] };
        }
        let g = grain(u, v, 0x5C2D, 30.0);
        // A block of colour with a lighter bar through it — enough that it reads as printed
        // rather than as a slab, without pretending to be anyone's logo.
        let base = if (0.34..0.62).contains(&v) {
            [238.0, 238.0, 234.0]
        } else if u < 0.5 {
            [196.0, 62.0, 40.0]
        } else {
            [34.0, 78.0, 150.0]
        };
        let s = 0.92 + 0.10 * g;
        [(base[0] * s) as u8, (base[1] * s) as u8, (base[2] * s) as u8, 255]
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

/// A tree's sheet, in two halves: bark on the left, foliage on the right.
///
/// The tree is solid geometry now rather than crossed cards, so nothing here is cut out — a
/// canopy built as a shape does not need an alpha channel to stop being a slab.
fn tree_sheet() -> Texture {
    sheet("tree_c", 128, |u, v| {
        if u < 0.5 {
            // Bark: vertical grain, because that is the one thing that makes a trunk read.
            let streak = grain(u * 6.0, v, 0x77B1, 30.0);
            let s = 0.72 + 0.46 * streak;
            [(96.0 * s) as u8, (72.0 * s) as u8, (52.0 * s) as u8, 255]
        } else {
            let g = grain(u, v, 0x3D19, 34.0);
            let h = grain(u, v, 0x1A55, 9.0);
            let s = 0.58 + 0.34 * g + 0.16 * h;
            [(62.0 * s) as u8, (112.0 * s) as u8, (50.0 * s) as u8, 255]
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

/// A tree with a shape rather than a picture of one.
///
/// Crossed cards are what published tracks use and they read fine at a distance, but close up
/// they are two flat pictures and you can see them turn as you ride past. This is a tapered
/// trunk and a canopy of stacked rings — about forty triangles, solid from every angle, and
/// no alpha channel needed because a shape does not have to be cut out of anything.
///
/// UVs put the trunk in the left half of the sheet and the foliage in the right; see
/// [`tree_sheet`].
fn tree_mesh(h: f32, seed: u32, i: u32) -> Mesh {
    const SIDES: usize = 7;
    let mut m = Mesh::default();
    let trunk_h = h * 0.42;
    let r_base = (h * 0.045).max(0.06);

    let ring = |mesh: &mut Mesh, y: f32, r: f32, u: f32, v: f32, wobble: f32, k: u32| -> u32 {
        let start = mesh.vertex_count() as u32;
        for s in 0..SIDES {
            let a = std::f32::consts::TAU * s as f32 / SIDES as f32;
            let rr = r * (1.0 - wobble * 0.5 + wobble * rnd(seed ^ 0xB1, k * 31 + s as u32));
            let (sx, sz) = (a.sin() * rr, a.cos() * rr);
            mesh.positions.extend_from_slice(&[sx, y, sz]);
            // Unit length, and not merely pointing the right way: `map::parse` rejects a
            // whole `.map` whose normals aren't unit, because that is its one cheap check
            // that a block really is a vertex block. Tilted outward-and-up by eye and left
            // un-normalised, these came out 1.031 long and TerrainEd's output — which was
            // otherwise perfect — would not read back at all.
            let (ox, oy, oz) = (sx, 0.25 * rr.max(0.01), sz);
            let l = (ox * ox + oy * oy + oz * oz).sqrt().max(1e-4);
            mesh.normals.extend_from_slice(&[ox / l, oy / l, oz / l]);
            mesh.uvs.extend_from_slice(&[u + 0.18 * (s as f32 / SIDES as f32), v]);
        }
        start
    };
    // Wound the way `cuboid` is — see its note. Outward faces come after the reverse.
    let mut band = |mesh: &mut Mesh, lo: u32, hi: u32| {
        for s in 0..SIDES as u32 {
            let n = (s + 1) % SIDES as u32;
            mesh.indices.extend_from_slice(&[lo + s, lo + n, hi + s]);
            mesh.indices.extend_from_slice(&[hi + s, lo + n, hi + n]);
        }
    };

    // Trunk: two rings, tapering.
    let t0 = ring(&mut m, 0.0, r_base, 0.04, 0.95, 0.15, i * 3);
    let t1 = ring(&mut m, trunk_h, r_base * 0.72, 0.04, 0.05, 0.15, i * 3 + 1);
    band(&mut m, t0, t1);

    // Canopy: three rings and a cap, widest a third of the way up.
    let cw = h * 0.34;
    let c0 = ring(&mut m, trunk_h * 0.82, cw * 0.55, 0.55, 0.96, 0.30, i * 3 + 2);
    let c1 = ring(&mut m, trunk_h + (h - trunk_h) * 0.30, cw, 0.55, 0.62, 0.30, i * 5);
    let c2 = ring(&mut m, trunk_h + (h - trunk_h) * 0.68, cw * 0.74, 0.55, 0.30, 0.30, i * 5 + 1);
    band(&mut m, c0, c1);
    band(&mut m, c1, c2);
    let apex = m.vertex_count() as u32;
    m.positions.extend_from_slice(&[0.0, h, 0.0]);
    m.normals.extend_from_slice(&[0.0, 1.0, 0.0]);
    m.uvs.extend_from_slice(&[0.72, 0.04]);
    for s in 0..SIDES as u32 {
        let n = (s + 1) % SIDES as u32;
        m.indices.extend_from_slice(&[c2 + s, c2 + n, apex]);
    }
    m
}

/// A banner: a printed panel slung between two stakes.
fn banner_mesh() -> Mesh {
    let mut m = edfwrite::double_sided(&edfwrite::moved(
        &edfwrite::card(BANNER_W_M, BANNER_H_M),
        [0.0, BANNER_LIFT_M, 0.0],
    ));
    for side in [-1.0f32, 1.0] {
        m.append(&edfwrite::moved(
            &edfwrite::cuboid(0.08, BANNER_LIFT_M + BANNER_H_M, 0.08),
            [side * BANNER_W_M * 0.5, 0.0, 0.0],
        ));
    }
    m
}

/// Build a track's scenery: the models, and where they stand.
pub fn build(prog: &TrackProgram, syn: &Synth) -> Scenery {
    let seed = prog.terrain.relief.seed;
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    let stations = prog.stations(0.5);
    let at = |s: f32| -> crate::trackprog::Station {
        let i = ((s / 0.5) as usize).min(stations.len().saturating_sub(1));
        stations[i]
    };
    // Every candidate is measured against the whole lap, not against the station it came
    // from. Two metres a step is finer than anything being placed is wide.
    let coarse = prog.stations(2.0);

    let mut stakes = Mesh::default();
    let mut banners = Mesh::default();
    let mut fence = Mesh::default();
    let mut bales = Mesh::default();
    let mut trees = Mesh::default();
    let mut gate = Mesh::default();
    let mut tally = Vec::new();

    // 1. The edge line: bare stakes with a painted top, which is how a motocross track is
    // marked. Thin, so what you see down the track is a line of points rather than a wall.
    let stake_off = (half + 1.0).max(STAKE_OFF_M);
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        for side in [-1.0f32, 1.0] {
            let (x, z) = (st.x + rx * stake_off * side, st.z + rz * stake_off * side);
            if !inside(prog, x, z, 2.0) || clearance(&coarse, x, z) < stake_off - 1.0 {
                continue;
            }
            let key = (s / STAKE_GAP_M) as u32 * 2 + (side > 0.0) as u32;
            let h = STAKE_H_M * (0.92 + 0.16 * rnd(seed ^ 0x12, key));
            let lean = (rnd(seed ^ 0x13, key) - 0.5) * 16.0;
            stakes.append(&edfwrite::moved(
                &edfwrite::turned(&edfwrite::cuboid(STAKE_W_M, h, STAKE_W_M), lean),
                [x, ground(syn, x, z) - 0.03, z],
            ));
            n += 1;
        }
        s += STAKE_GAP_M;
    }
    tally.push(("stakes", n));

    // 2. Banners: few, big, and facing the rider.
    let mut n = 0usize;
    let mut s = BANNER_GAP_M * 0.5;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        let key = (s / BANNER_GAP_M) as u32;
        let side = if rnd(seed ^ 0x51, key) < 0.5 { -1.0f32 } else { 1.0 };
        let off = BANNER_OFF_M.max(half + 2.5);
        let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
        if inside(prog, x, z, 3.0) && clearance(&coarse, x, z) > off - 1.0 {
            let (lo, hi) = ground_span(syn, x, z, along(st.heading), BANNER_W_M);
            if hi - lo < 1.0 {
                banners.append(&edfwrite::moved(
                    &edfwrite::turned(&banner_mesh(), along(st.heading)),
                    [x, (lo + hi) * 0.5 - 0.05, z],
                ));
                n += 1;
            }
        }
        s += BANNER_GAP_M;
    }
    tally.push(("banners", n));

    // 3. The fence, walked along its *own* line rather than along the centreline.
    //
    // Stepping the centreline and offsetting each step spaces panels by the centreline's arc
    // length, and the offset line's is longer on the outside of a corner and shorter on the
    // inside — so the run gapped through every turn one way and piled up the other. Walking
    // the offset polyline and laying a panel every panel-length of *it* is what makes a fence
    // a fence.
    let mut n = 0usize;
    let mut fence_at: Vec<(f32, f32)> = Vec::new();
    for side in [-1.0f32, 1.0] {
        let line: Vec<(f32, f32)> = stations
            .iter()
            .map(|st| {
                let (rx, rz) = crate::trackprog::right_vector(st.heading);
                let off = FENCE_OFF_M + 1.2 * (rnd(seed ^ 0x21, (st.s * 0.05) as u32) - 0.5);
                (st.x + rx * off * side, st.z + rz * off * side)
            })
            .collect();
        let mut run = 0.0f32;
        let mut last = line[0];
        for w in line.windows(2) {
            run += ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            if run < FENCE_PANEL_M {
                continue;
            }
            run = 0.0;
            let (x, z) = ((last.0 + w[1].0) * 0.5, (last.1 + w[1].1) * 0.5);
            let deg = (w[1].0 - last.0).atan2(w[1].1 - last.1).to_degrees() + 90.0;
            last = w[1];
            if !inside(prog, x, z, 4.0) || clearance(&coarse, x, z) < half + 4.0 {
                continue;
            }
            let (lo, hi) = ground_span(syn, x, z, deg, FENCE_PANEL_M);
            if hi - lo > 1.2 {
                continue;
            }
            // Where the lap folds back the two sides' fences meet, and a panel from one run
            // lands inside a panel from the other. Spacing along a run does not catch that
            // because the runs are walked separately.
            let too_near = (FENCE_PANEL_M * 0.55).powi(2);
            if fence_at
                .iter()
                .any(|(px, pz)| (px - x).powi(2) + (pz - z).powi(2) < too_near)
            {
                continue;
            }
            fence_at.push((x, z));
            // Double-sided: a fence you can only see from behind is not a fence.
            let panel = edfwrite::double_sided(&edfwrite::card(FENCE_PANEL_M, FENCE_H_M));
            fence.append(&edfwrite::moved(
                &edfwrite::turned(&panel, deg),
                [x, (lo + hi) * 0.5 - 0.05, z],
            ));
            n += 1;
        }
    }
    tally.push(("fence panels", n));

    // 4. Bales where a rider leaves the track fastest: the outside of every corner. Their
    // long side runs along the track edge, which is what `along` is for.
    let mut n = 0usize;
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        if st.curvature.abs() > 1.0 / 45.0 {
            let side = -st.curvature.signum();
            let (rx, rz) = crate::trackprog::right_vector(st.heading);
            let key = (s / 4.0) as u32;
            let off = BALE_OFF_M + (rnd(seed ^ 0x31, key) - 0.5) * 2.0;
            let (x, z) = (st.x + rx * off * side, st.z + rz * off * side);
            if inside(prog, x, z, 3.0) && clearance(&coarse, x, z) > half + 2.0 {
                let deg = along(st.heading) + 10.0 * (rnd(seed ^ 0x32, key) - 0.5);
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

    // 5. Trees, out past the fence, thinned so they don't line up with the lap.
    let mut n = 0usize;
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut s = 0.0f32;
    while s < lap {
        let st = at(s);
        let (rx, rz) = crate::trackprog::right_vector(st.heading);
        // Keyed on where round the lap we are, not on how many have been placed. Keyed on
        // the count, every station drew the same three numbers — because the count only
        // moves when a tree is accepted — so every station after the first proposed trees in
        // the same three spots and the spacing rule threw them all away. One tree on the
        // whole track, and it looked like the spacing rule was too strict.
        let step = (s / TREE_SPACING_M) as u32;
        for k in 0..3u32 {
            let i = step * 7 + k;
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
            trees.append(&edfwrite::moved(
                &edfwrite::turned(&tree_mesh(h, seed, i), 360.0 * rnd(seed ^ 0x45, i)),
                [x, ground_min(syn, x, z, h * 0.2) - 0.05, z],
            ));
            n += 1;
        }
        s += TREE_SPACING_M;
    }
    tally.push(("trees", n));

    // 6. The start gantry. Each post stands on its own ground — level them together and one
    // is buried and the other in the air — and the beam clears the higher of the two.
    //
    // The beam spans *across* the track, so it turns by `across`. Turned by `along` it lay
    // down the track instead of over it, which is the ninety degrees you could see.
    let st = at(2.0);
    let (rx, rz) = crate::trackprog::right_vector(st.heading);
    let mut highest = f32::NEG_INFINITY;
    for side in [-1.0f32, 1.0] {
        let (x, z) = (st.x + rx * (half + 3.0) * side, st.z + rz * (half + 3.0) * side);
        let foot = ground(syn, x, z);
        highest = highest.max(foot);
        gate.append(&edfwrite::moved(&edfwrite::cuboid(0.5, 5.0, 0.5), [x, foot, z]));
    }
    let span = (half + 3.0) * 2.0;
    let beam = edfwrite::turned(&edfwrite::cuboid(span, 1.2, 0.3), across(st.heading));
    gate.append(&edfwrite::moved(&beam, [st.x, highest + 4.6, st.z]));
    tally.push(("gate", 1));

    // One model a kind, each with its own single sheet. Not one model of several materials:
    // TerrainEd faults on the second material in a model whatever the geometry — see
    // `edfwrite`'s note and the case-by-case test behind it. PiBoSo's own example track is
    // built the same way, three `scene` blocks for three objects.
    let kinds: Vec<(&str, Mesh, Texture, bool)> = vec![
        ("stakes", stakes, stake_sheet(), false),
        ("banners", banners, banner_sheet(), true),
        ("fence", fence, fence_sheet(), false),
        ("bales", bales, bale_sheet(), true),
        ("trees", trees, tree_sheet(), true),
        ("gate", gate, gate_sheet(), true),
    ];

    let mut files = Vec::new();
    let mut drawn = Vec::new();
    let mut solid = Vec::new();
    for (name, mesh, sheet, is_solid) in kinds {
        if mesh.vertex_count() < 8 {
            continue;
        }
        let file = format!("{name}.edf");
        let bytes = edfwrite::write(name, &[Part { name: name.into(), mesh, texture: 0 }], &[sheet]);
        files.push((file.clone(), bytes));
        let at = Scene { file, pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] };
        // Collision only for what should stop a bike. A stake snaps and the fence is behind
        // the run-off, so neither is a wall; a tree, a bale and the gantry are.
        if is_solid {
            solid.push(at.clone());
        }
        drawn.push(at);
    }

    Scenery { files, drawn, solid, tally }
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
        let stakes = sc.tally.iter().find(|(k, _)| *k == "stakes").unwrap().1;
        let want = (p.lap_length() / STAKE_GAP_M * 2.0) as usize;
        assert!(
            stakes > want * 8 / 10,
            "{stakes} stakes for a {:.0} m lap, expected about {want}",
            p.lap_length()
        );
    }

    #[test]
    fn banners_are_few_and_big() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let banners = sc.tally.iter().find(|(k, _)| *k == "banners").unwrap().1;
        let stakes = sc.tally.iter().find(|(k, _)| *k == "stakes").unwrap().1;
        // A banner is a thing you notice, not the edge marking. If the two counts are the
        // same order the track is fenced in by its own advertising, which is what the first
        // pass looked like.
        assert!(banners < stakes / 8, "{banners} banners against {stakes} stakes");
        assert!(banners > 10, "only {banners} banners on a {:.0} m lap", p.lap_length());
    }

    #[test]
    fn a_lap_gets_a_treeline_and_not_one_tree() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let trees = sc.tally.iter().find(|(k, _)| *k == "trees").unwrap().1;
        // Corpus p50 is 118 near trees per km over the tracks that have them, and the loop is
        // deliberately thinner than that. What this catches is the failure that actually
        // happened: randomness keyed on the placed count, so every station drew the same
        // numbers and the whole lap ended up with one tree.
        let per_km = trees as f32 / (p.lap_length() / 1000.0);
        assert!(per_km > 20.0, "{trees} trees is {per_km:.0}/km over {:.0} m", p.lap_length());
    }

    #[test]
    fn nothing_stands_on_the_riding_line() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let nodes: Vec<crate::edf::EdfNode> = sc
            .files
            .iter()
            .flat_map(|(_, b)| crate::edf::parse_world(b))
            .collect();
        assert!(nodes.len() >= 4, "a kind a model: {}", nodes.len());

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
    fn every_normal_is_unit_length() {
        // `map::parse` refuses a map whose normals are not unit — it is how it tells a vertex
        // block from noise — so a model that ships a normal of length 1.03 makes TerrainEd's
        // output unreadable even though TerrainEd compiled it perfectly.
        let (p, s) = demo();
        let sc = build(&p, &s);
        for (name, bytes) in &sc.files {
            for n in crate::edf::parse_world(bytes) {
                for v in n.normals.chunks_exact(3) {
                    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    assert!(
                        (l - 1.0).abs() < 1e-3,
                        "{name}/{} has a normal {l:.4} long: {v:?}",
                        n.name
                    );
                }
            }
        }
    }

    #[test]
    fn collision_is_only_what_should_stop_a_bike() {
        let (p, s) = demo();
        let sc = build(&p, &s);
        let named = |v: &[Scene]| -> Vec<String> { v.iter().map(|s| s.file.clone()).collect() };
        let drawn = named(&sc.drawn);
        let solid = named(&sc.solid);
        for want in ["stakes.edf", "banners.edf", "fence.edf", "bales.edf", "trees.edf", "gate.edf"] {
            assert!(drawn.contains(&want.to_string()), "{want} not drawn: {drawn:?}");
        }
        // A tree, a bale and the gantry stop a bike.
        for want in ["bales.edf", "trees.edf", "gate.edf", "banners.edf"] {
            assert!(solid.contains(&want.to_string()), "{want} should be solid: {solid:?}");
        }
        // A stake snaps rather than stopping you, and the fence run has gaps where the ground
        // steps — as collision that is a wall with holes in it, which is worse than no wall.
        assert!(!solid.contains(&"stakes.edf".to_string()), "{solid:?}");
        assert!(!solid.contains(&"fence.edf".to_string()), "{solid:?}");
        // And every placed model is one the export actually writes.
        for f in solid.iter().chain(drawn.iter()) {
            assert!(sc.files.iter().any(|(n, _)| n == f), "{f} is placed but never written");
        }
    }

    #[test]
    fn the_blocks_read_the_way_terrained_writes_them() {
        let s = blocks(&[
            Scene { file: "trees.edf".into(), pos: [1.0, 2.0, 3.0], rot: [0.0, 90.0, 0.0] },
            Scene { file: "bales.edf".into(), pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] },
        ]);
        assert!(s.contains("scene0") && s.contains("scene1"), "{s}");
        assert!(s.contains("name = trees.edf") && s.contains("name = bales.edf"));
        assert!(s.contains("x = 1.000") && s.contains("y = 2.000") && s.contains("z = 3.000"));
        assert!(s.contains("y = 90.000"));
    }

    #[test]
    fn a_fence_run_is_spaced_along_its_own_line() {
        // Stepping the centreline and offsetting each step spaces panels by the *centreline's*
        // arc length, so a run gaps through every corner one way and piles up the other. Walk
        // the offset line instead and consecutive panels are a panel-length apart wherever
        // they are, corner or straight.
        let (p, s) = demo();
        let sc = build(&p, &s);
        let (_, bytes) = sc.files.iter().find(|(n, _)| n == "fence.edf").expect("a fence");
        let nodes = crate::edf::parse_world(bytes);
        let mut centres: Vec<[f32; 2]> = Vec::new();
        for n in &nodes {
            // Each panel is 8 vertices: a card and its double. Take each panel's own centre.
            for chunk in n.positions.chunks_exact(24) {
                let (mut x, mut z) = (0.0f32, 0.0f32);
                for v in chunk.chunks_exact(3) {
                    x += v[0] / 8.0;
                    z += v[2] / 8.0;
                }
                centres.push([x, z]);
            }
        }
        assert!(centres.len() > 100, "only {} panels", centres.len());
        // Neighbours within a run sit about a panel apart; nothing sits on top of anything.
        let mut overlapping = 0;
        for i in 0..centres.len() {
            for j in i + 1..centres.len() {
                let d = (centres[i][0] - centres[j][0]).powi(2)
                    + (centres[i][1] - centres[j][1]).powi(2);
                if d < (FENCE_PANEL_M * 0.55).powi(2) {
                    overlapping += 1;
                }
            }
        }
        assert_eq!(overlapping, 0, "{overlapping} fence panels sit on top of each other");
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
        let name = crate::tracksynth::slug(&p.name);
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
