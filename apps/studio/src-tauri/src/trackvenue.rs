//! The venue beyond the lap: a fenced paddock with a rig per team, the road to the pits,
//! and the sponsor wall behind the gate row.

#![allow(dead_code)]

use crate::edfwrite::{self, Mesh, Texture};
use crate::trackprog::TrackProgram;
use crate::tracksynth::Synth;

/// A donor's gate row from its `.rdf`: the row's middle and its heading, degrees.
pub fn donor_grid(path: &std::path::Path) -> Option<(f32, f32, f32)> {
    let names = crate::track::entry_names(path).ok()?;
    let rdf = names.iter().find(|n| n.to_ascii_lowercase().ends_with(".rdf"))?;
    let text = String::from_utf8_lossy(&crate::track::read_entry(path, rdf).ok()?).into_owned();
    let block = &text[text.find("starting_grid")?..];
    let val = |key: &str| -> Option<f32> {
        block.lines().find_map(|l| l.trim().strip_prefix(&format!("{key} = "))).and_then(|v| v.trim().parse().ok())
    };
    Some((val("posx")?, val("posz")?, val("angle")?))
}

/// The pieces of a donor's sponsor wall, by prop id and the sheet each wears.
pub const WALL_PARTS: [(&str, &str); 3] = [
    ("sponsor_wall", "start_backdrop_c"),
    ("sponsor_wall_frame", "main_track_objects_c"),
    ("sponsor_wall_tarp", "tarp_c"),
];

/// A donor's island triangles, as `lift` walks them: `tri_count` owned ones from `tri_start`.
fn island_tris(m: &crate::map::MapMesh, isl: usize) -> Vec<usize> {
    let o = &m.objects[isl];
    let (mut left, mut t, mut out) = (o.tri_count, o.tri_start as usize, Vec::new());
    while left > 0 && t < m.object_of_tri.len() {
        if m.object_of_tri[t] as usize == isl {
            out.push(t);
            left -= 1;
        }
        t += 1;
    }
    out
}

/// The sponsor wall behind a donor's gate row, lifted whole: the printed board, its frame and
/// the tarp behind it, one prop a sheet in one shared frame. Centred on the print, foot at the
/// frame's lowest point, `axis_ref` the gate heading, so it turns onto our gate like the arch.
pub fn lift_wall(d: &crate::trackprops::Donor, grid: (f32, f32, f32)) -> Vec<crate::trackprops::Prop> {
    let (gx, gz, ang) = grid;
    let h = ang.to_radians();
    let (fx, fz) = crate::trackprog::heading_vector(h);
    let (rx, rz) = crate::trackprog::right_vector(h);
    let frame = |x: f32, z: f32| ((x - gx) * fx + (z - gz) * fz, (x - gx) * rx + (z - gz) * rz);
    let m = &d.mesh;
    let sheet_of = |i: usize| d.sheets.get(m.objects[i].material as usize).map(|s| s.0.to_ascii_lowercase()).unwrap_or_default();
    let verts = |isl: usize| -> Vec<usize> {
        island_tris(m, isl).iter().flat_map(|&t| (0..3).map(move |k| m.indices[t * 3 + k] as usize)).collect()
    };
    // Extent in the gate frame: along, across, height.
    let extent = |vs: &[usize]| {
        let mut e = [f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for &v in vs {
            let (a, c) = frame(m.positions[v * 3], m.positions[v * 3 + 2]);
            let y = m.positions[v * 3 + 1];
            e = [e[0].min(a), e[1].max(a), e[2].min(c), e[3].max(c), e[4].min(y), e[5].max(y)];
        }
        e
    };
    // The print: the widest backdrop island standing behind the row.
    let Some((print, pe)) = (0..m.objects.len())
        .filter(|&i| sheet_of(i) == WALL_PARTS[0].1)
        .map(|i| (i, extent(&verts(i))))
        .filter(|(_, e)| (e[0] + e[1]) * 0.5 < 0.0 && (e[0] + e[1]) * 0.5 > -40.0 && ((e[2] + e[3]) * 0.5).abs() < 25.0)
        .max_by(|a, b| (a.1[3] - a.1[2]).total_cmp(&(b.1[3] - b.1[2])))
    else {
        return Vec::new();
    };
    if pe[3] - pe[2] < 15.0 {
        return Vec::new();
    }
    // Everything of the wall's sheets inside its slab.
    let (a0, a1, c0, c1) = (pe[0] - 3.0, pe[1] + 3.0, pe[2] - 1.5, pe[3] + 1.5);
    let mut parts: Vec<Vec<usize>> = vec![Vec::new(); WALL_PARTS.len()];
    for i in 0..m.objects.len() {
        let Some(k) = WALL_PARTS.iter().position(|(_, s)| *s == sheet_of(i)) else { continue };
        let o = &m.objects[i];
        let (ca, cc) = frame((o.min[0] + o.max[0]) * 0.5, (o.min[2] + o.max[2]) * 0.5);
        if !(a0 - 5.0..=a1 + 5.0).contains(&ca) || !(c0 - 5.0..=c1 + 5.0).contains(&cc) {
            continue;
        }
        let e = extent(&verts(i));
        if e[0] >= a0 && e[1] <= a1 && e[2] >= c0 && e[3] <= c1 && e[4] >= pe[4] - 4.0 {
            parts[k].push(i);
        }
    }
    let _ = print;
    let all: Vec<usize> = parts.iter().flatten().flat_map(|&i| verts(i)).collect();
    let foot = all.iter().map(|&v| m.positions[v * 3 + 1]).fold(f32::INFINITY, f32::min);
    let pv = verts(print);
    let (cx, cz) = (
        pv.iter().map(|&v| m.positions[v * 3]).sum::<f32>() / pv.len() as f32,
        pv.iter().map(|&v| m.positions[v * 3 + 2]).sum::<f32>() / pv.len() as f32,
    );
    let mut out = Vec::new();
    for (k, isls) in parts.iter().enumerate() {
        let mut mesh = Mesh::default();
        for &isl in isls {
            for t in island_tris(m, isl) {
                let base = mesh.vertex_count() as u32;
                for j in 0..3 {
                    let v = m.indices[t * 3 + j] as usize;
                    mesh.positions.extend_from_slice(&[m.positions[v * 3] - cx, m.positions[v * 3 + 1] - foot, m.positions[v * 3 + 2] - cz]);
                    mesh.normals.extend_from_slice(&m.normals[v * 3..v * 3 + 3]);
                    mesh.uvs.extend_from_slice(&m.uvs[v * 2..v * 2 + 2]);
                }
                mesh.indices.extend([base, base + 1, base + 2]);
            }
        }
        if mesh.triangle_count() == 0 {
            continue;
        }
        let (lo, hi) = mesh.bounds();
        let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
        out.push(crate::trackprops::Prop {
            id: WALL_PARTS[k].0.into(),
            sheet: WALL_PARTS[k].1.into(),
            class: crate::trackobjects::Class::Structure,
            height: hi[1] - lo[1].min(0.0),
            span: (hi[0] - lo[0]).max(hi[2] - lo[2]),
            reach,
            axis_ref: h,
            mesh,
        });
    }
    out
}

/// Each triangle cut in four, winding kept. The wall's print and tarp are one quad each, and
/// `trackscenery::build` writes no model under eight vertices.
fn fine(m: &Mesh) -> Mesh {
    let at = |i: u32| {
        let i = i as usize;
        (
            [m.positions[i * 3], m.positions[i * 3 + 1], m.positions[i * 3 + 2]],
            [m.uvs[i * 2], m.uvs[i * 2 + 1]],
            [m.normals[i * 3], m.normals[i * 3 + 1], m.normals[i * 3 + 2]],
        )
    };
    type V = ([f32; 3], [f32; 2], [f32; 3]);
    let mid = |a: V, b: V| -> V {
        (std::array::from_fn(|k| (a.0[k] + b.0[k]) * 0.5), std::array::from_fn(|k| (a.1[k] + b.1[k]) * 0.5), a.2)
    };
    let mut o = Mesh::default();
    for t in m.indices.chunks_exact(3) {
        let (a, b, c) = (at(t[0]), at(t[1]), at(t[2]));
        let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
        for tri in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            let base = o.vertex_count() as u32;
            for v in tri {
                o.positions.extend_from_slice(&v.0);
                o.uvs.extend_from_slice(&v.1);
                o.normals.extend_from_slice(&v.2);
            }
            o.indices.extend([base, base + 1, base + 2]);
        }
    }
    o
}

/// A mesh turned so its long axis runs along x, centred on its footprint.
fn aligned(m: &Mesh) -> Mesh {
    let width = |th: f32| {
        let (sn, cs) = th.sin_cos();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for v in m.positions.chunks_exact(3) {
            let d = -v[0] * sn + v[2] * cs;
            lo = lo.min(d);
            hi = hi.max(d);
        }
        hi - lo
    };
    let deg = (0..180).map(|d| d as f32).min_by(|a, b| width(a.to_radians()).total_cmp(&width(b.to_radians()))).unwrap_or(0.0);
    let x_len = |m: &Mesh| {
        let (lo, hi) = m.bounds();
        hi[0] - lo[0]
    };
    let (a, b) = (edfwrite::turned(m, deg), edfwrite::turned(m, -deg));
    let best = if x_len(&a) >= x_len(&b) { a } else { b };
    let (lo, hi) = best.bounds();
    edfwrite::moved(&best, [-(lo[0] + hi[0]) * 0.5, 0.0, -(lo[2] + hi[2]) * 0.5])
}

/// Pictures for judging by eye.
#[cfg(test)]
pub(crate) mod pic {
    use super::*;

    /// Rasterise a mesh into `img` through `proj` (screen x, screen y, depth; nearer is
    /// smaller), textured where a sheet is given, cut-outs dropped.
    pub fn raster(img: &mut image::RgbImage, zb: &mut [f32], m: &Mesh, tex: Option<(u32, u32, &[u8])>, flat: [u8; 3], proj: &dyn Fn(f32, f32, f32) -> (f32, f32, f32)) {
        let (w, h) = img.dimensions();
        for t in m.indices.chunks_exact(3) {
            let p: Vec<(f32, f32, f32)> = t.iter().map(|&i| {
                let i = i as usize;
                proj(m.positions[i * 3], m.positions[i * 3 + 1], m.positions[i * 3 + 2])
            }).collect();
            let uv: Vec<(f32, f32)> = t.iter().map(|&i| (m.uvs.get(i as usize * 2).copied().unwrap_or(0.0), m.uvs.get(i as usize * 2 + 1).copied().unwrap_or(0.0))).collect();
            let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-6 {
                continue;
            }
            let (x0, x1) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).max(0.0), p.iter().map(|q| q.0).fold(f32::MIN, f32::max).min(w as f32 - 1.0));
            let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).max(0.0), p.iter().map(|q| q.1).fold(f32::MIN, f32::max).min(h as f32 - 1.0));
            if x0 > x1 || y0 > y1 {
                continue;
            }
            for py in y0 as u32..=y1 as u32 {
                for px in x0 as u32..=x1 as u32 {
                    let (sx, sy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let e = |a: (f32, f32, f32), b: (f32, f32, f32)| (b.0 - a.0) * (sy - a.1) - (sx - a.0) * (b.1 - a.1);
                    let (l0, l1, l2) = (e(p[1], p[2]) / area, e(p[2], p[0]) / area, e(p[0], p[1]) / area);
                    if l0 < -0.01 || l1 < -0.01 || l2 < -0.01 {
                        continue;
                    }
                    let z = l0 * p[0].2 + l1 * p[1].2 + l2 * p[2].2;
                    let k = (py * w + px) as usize;
                    if z >= zb[k] {
                        continue;
                    }
                    let col = match tex {
                        Some((tw, th, px_)) if tw > 0 => {
                            let (u, v) = (l0 * uv[0].0 + l1 * uv[1].0 + l2 * uv[2].0, l0 * uv[0].1 + l1 * uv[1].1 + l2 * uv[2].1);
                            let x = (((u - u.floor()) * tw as f32) as u32).min(tw - 1);
                            let y = (((v - v.floor()) * th as f32) as u32).min(th - 1);
                            let i = ((y * tw + x) * 4) as usize;
                            if px_[i + 3] < 100 {
                                continue;
                            }
                            [px_[i], px_[i + 1], px_[i + 2]]
                        }
                        _ => flat,
                    };
                    zb[k] = z;
                    img.put_pixel(px, py, image::Rgb(col));
                }
            }
        }
    }

    /// A line of text on an RGB picture, pen at `(x, y)` on the baseline.
    pub fn label(img: &mut image::RgbImage, text: &str, size: f32, x: f32, y: f32, col: [u8; 3]) {
        use ab_glyph::{Font, ScaleFont};
        let Ok(font) = ab_glyph::FontRef::try_from_slice(FONT) else { return };
        let sf = font.as_scaled(size);
        let mut pen = x;
        let (w, h) = img.dimensions();
        for ch in text.chars() {
            let id = sf.glyph_id(ch);
            let g = id.with_scale_and_position(size, ab_glyph::point(pen, y));
            pen += sf.h_advance(id);
            let Some(og) = font.outline_glyph(g) else { continue };
            let b = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h && cov > 0.4 {
                    img.put_pixel(px as u32, py as u32, image::Rgb(col));
                }
            });
        }
    }
}

/// The face every printed board here is set in.
const FONT: &[u8] = include_bytes!("../assets/fonts/BarlowCondensed-Bold.ttf");

type Kind = (String, Mesh, Texture, bool);

/// The teams, each with its board's ground and ink.
pub const BRANDS: [(&str, [u8; 3], [u8; 3]); 8] = [
    ("KTM", [240, 110, 0], [16, 16, 16]),
    ("HONDA", [204, 8, 20], [250, 250, 250]),
    ("YAMAHA", [10, 48, 160], [250, 250, 250]),
    ("KAWASAKI", [100, 190, 40], [16, 16, 16]),
    ("HUSQVARNA", [22, 40, 92], [250, 250, 250]),
    ("GASGAS", [200, 16, 46], [250, 250, 250]),
    ("SUZUKI", [250, 212, 0], [8, 40, 140]),
    ("TRIUMPH", [18, 18, 20], [250, 250, 250]),
];

/// The pit lane as `tracksynth::rdf` writes it: stall gap, how far past the edge, strip half-width.
const STALL_GAP_M: f32 = 5.0;
const LANE_OUT_M: f32 = 6.0;
const LANE_HALF_M: f32 = 4.0;

/// Paddock: margin inside the fence, gap between neighbouring spots, the aisle, the gate.
const PAD_MARGIN_M: f32 = 3.0;
const SPOT_GAP_M: f32 = 5.0;
const AISLE_M: f32 = 10.0;
const GATE_M: f32 = 12.0;
/// How far every part of the paddock keeps from any leg of the lap, past the half-width.
const PAD_CLEAR_M: f32 = 8.0;
/// The most the ground may fall across the paddock.
const PAD_FALL_M: f32 = 4.0;
/// Road widths: the approach, and the aisle between the rows.
const ROAD_W_M: f32 = 6.0;
const AISLE_ROAD_W_M: f32 = 7.0;
const ROAD_LIFT_M: f32 = 0.06;
/// A team board: size, and how high its bottom edge stands.
const BOARD_W_M: f32 = 4.0;
const BOARD_H_M: f32 = 2.0;
const BOARD_LIFT_M: f32 = 1.2;
/// The wall: how far behind the gate row it may stand, and its clearance off the start pad.
const WALL_BACK_M: (f32, f32) = (6.0, 45.0);
const WALL_OFF_PAD_M: f32 = 1.5;
/// Our own wall, when the library has none.
const OWN_WALL_M: (f32, f32, f32) = (48.0, 4.0, 1.0);

/// The pits: each stall's metres round the lap and signed lateral offset, which side they are
/// on (+1 the rider's right), and the lane's middle, metres from the centreline.
#[derive(Clone, Debug)]
pub struct Lane {
    pub stalls: Vec<(f32, f32)>,
    pub side: f32,
    pub lane: f32,
}

/// The stalls `tracksynth::rdf` spawns riders in (its `pit_lane` is private).
pub fn lane(prog: &TrackProgram) -> Lane {
    let side = prog.start_line().map(|l| -l.side).unwrap_or(-1.0);
    let run = prog.opening_straight().max(prog.lap_length() * 0.1);
    let from = 10.0f32.min(run * 0.1);
    let n = (((run - from) / STALL_GAP_M).floor() as usize).clamp(4, 16);
    let lane = prog.width * 0.5 + LANE_OUT_M;
    Lane { stalls: (0..n).map(|i| (from + i as f32 * STALL_GAP_M, side * lane)).collect(), side, lane }
}

/// A rectangle in the world: centre, unit axes, half extents.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub c: (f32, f32),
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub hx: f32,
    pub hz: f32,
}

impl Rect {
    pub fn at(&self, a: f32, b: f32) -> (f32, f32) {
        (self.c.0 + self.x.0 * a + self.z.0 * b, self.c.1 + self.x.1 * a + self.z.1 * b)
    }
    pub fn local(&self, x: f32, z: f32) -> (f32, f32) {
        let (dx, dz) = (x - self.c.0, z - self.c.1);
        (dx * self.x.0 + dz * self.x.1, dx * self.z.0 + dz * self.z.1)
    }
    pub fn covers(&self, x: f32, z: f32, m: f32) -> bool {
        let (a, b) = self.local(x, z);
        a.abs() <= self.hx + m && b.abs() <= self.hz + m
    }
}

/// One team's spot.
#[derive(Clone, Debug)]
pub struct Spot {
    pub brand: &'static str,
    /// The rig's footprint.
    pub rig: Rect,
    pub board: (f32, f32),
    pub tent: Option<(f32, f32)>,
}

/// The paddock: its fence line (`z` points away from the track), spots, fence panel centres.
#[derive(Clone, Debug)]
pub struct Paddock {
    pub area: Rect,
    pub spots: Vec<Spot>,
    pub fence: Vec<(f32, f32)>,
}

/// The sponsor wall: where it stands, which way its print faces (heading, radians), how wide.
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub foot: Rect,
    pub faces: f32,
    pub lifted: bool,
}

/// What the venue adds to a track.
pub struct Venue {
    pub kinds: Vec<Kind>,
    pub tally: Vec<(&'static str, usize)>,
    pub paddock: Option<Paddock>,
    /// The road from the paddock's aisle, out through its gate, to the pit lane.
    pub road: Vec<(f32, f32)>,
    pub wall: Option<Wall>,
}

impl Venue {
    /// Whether a point, grown by `m`, lands on anything the venue stands on.
    pub fn covers(&self, x: f32, z: f32, m: f32) -> bool {
        self.paddock.as_ref().is_some_and(|p| p.area.covers(x, z, m + 1.0))
            || self.wall.is_some_and(|w| w.foot.covers(x, z, m + 1.0))
            || self.road.windows(2).any(|s| seg_dist(s[0], s[1], (x, z)) <= ROAD_W_M * 0.5 + 1.0 + m)
    }
}

fn seg_dist(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dz) / (dx * dx + dz * dz).max(1e-6)).clamp(0.0, 1.0);
    (p.0 - a.0 - dx * t).hypot(p.1 - a.1 - dz * t)
}

fn ground(syn: &Synth, x: f32, z: f32) -> f32 {
    let (gx, gz) = (x / syn.mps, z / syn.mps);
    let (x0, z0) = (gx.floor() as isize, gz.floor() as isize);
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let at = |ix: isize, iz: isize| syn.heights[iz.clamp(0, syn.gh as isize - 1) as usize * syn.gw + ix.clamp(0, syn.gw as isize - 1) as usize];
    let (a, b, c, d) = (at(x0, z0), at(x0 + 1, z0), at(x0, z0 + 1), at(x0 + 1, z0 + 1));
    (a + (b - a) * fx) * (1.0 - fz) + (c + (d - c) * fx) * fz
}

/// Metres from the nearest leg of the lap's centreline.
fn dist(syn: &Synth, x: f32, z: f32) -> f32 {
    let gx = (x / syn.mps).round().clamp(0.0, (syn.gw - 1) as f32) as usize;
    let gz = (z / syn.mps).round().clamp(0.0, (syn.gh - 1) as f32) as usize;
    syn.dist[gz * syn.gw + gx]
}

fn on_plot(prog: &TrackProgram, x: f32, z: f32, m: f32) -> bool {
    x > m && z > m && x < prog.terrain.size_x - m && z < prog.terrain.size_z - m
}

/// Turn to send local +x along `(dx, dz)`: `turned` sends x to `right_vector(deg)`.
fn deg_along(d: (f32, f32)) -> f32 {
    (-d.1).atan2(d.0).to_degrees()
}

/// Turn to send local +z (a card's face) along `(nx, nz)`: `turned` sends z to `heading_vector(deg)`.
fn deg_facing(n: (f32, f32)) -> f32 {
    n.0.atan2(n.1).to_degrees()
}

/// A rigid piece turned by `deg` and stood at `(x, z)` on the lowest ground under it.
fn stand(m: &Mesh, x: f32, z: f32, deg: f32, syn: &Synth) -> Mesh {
    let t = edfwrite::turned(m, deg);
    let (lo, hi) = t.bounds();
    let mut foot = f32::INFINITY;
    for (a, b) in [(lo[0], lo[2]), (hi[0], lo[2]), (lo[0], hi[2]), (hi[0], hi[2]), ((lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5)] {
        foot = foot.min(ground(syn, x + a, z + b));
    }
    edfwrite::moved(&t, [x, foot - lo[1] - 0.05, z])
}

fn lib_tex(lib: &crate::trackprops::PropLibrary, sheet: &str) -> Option<Texture> {
    lib.sheets.iter().find(|s| s.0 == sheet).map(|(n, w, h, px)| Texture { name: n.clone(), width: *w, height: *h, rgba: px.clone() })
}

fn whole(p: &crate::trackprops::Prop) -> bool {
    p.reach <= p.span * std::f32::consts::FRAC_1_SQRT_2 + 0.5
}

/// Text centred on `(cx, cy)` in an RGBA sheet, as large as fits `(bw, bh)`.
fn print_text(px: &mut [u8], w: u32, h: u32, text: &str, (cx, cy): (f32, f32), (bw, bh): (f32, f32), ink: [u8; 3]) {
    use ab_glyph::{Font, ScaleFont};
    let Ok(font) = ab_glyph::FontRef::try_from_slice(FONT) else { return };
    let width = |size: f32| {
        let sf = font.as_scaled(size);
        text.chars().map(|c| sf.h_advance(sf.glyph_id(c))).sum::<f32>()
    };
    let size = (bh / 0.72).min(bw / width(1.0).max(1e-3));
    let sf = font.as_scaled(size);
    let mut pen = cx - width(size) * 0.5;
    let base = cy + size * 0.36;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        let g = id.with_scale_and_position(size, ab_glyph::point(pen, base));
        pen += sf.h_advance(id);
        let Some(og) = font.outline_glyph(g) else { continue };
        let b = og.px_bounds();
        og.draw(|gx, gy, cov| {
            let (x, y) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                return;
            }
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            let a = cov.clamp(0.0, 1.0);
            for k in 0..3 {
                px[i + k] = (px[i + k] as f32 + (ink[k] as f32 - px[i + k] as f32) * a) as u8;
            }
        });
    }
}

/// The team boards: one cell a brand, two across and four down, name in its ink on its ground.
const ATLAS_PX: u32 = 1024;
fn cell(i: usize) -> (f32, f32, f32, f32) {
    let (u0, v0) = ((i % 2) as f32 * 0.5, (i / 2) as f32 * 0.25);
    (u0, v0, 0.5, 0.25)
}

pub fn brand_sheet() -> Texture {
    let n = ATLAS_PX;
    let mut px = vec![0u8; (n * n * 4) as usize];
    for (i, (name, ground, ink)) in BRANDS.iter().enumerate() {
        let (u0, v0, uw, vh) = cell(i);
        let (x0, y0, cw, ch) = ((u0 * n as f32) as u32, (v0 * n as f32) as u32, (uw * n as f32) as u32, (vh * n as f32) as u32);
        for y in y0..y0 + ch {
            for x in x0..x0 + cw {
                let (fu, fv) = ((x - x0) as f32 / cw as f32, (y - y0) as f32 / ch as f32);
                let hem = fu < 0.03 || fu > 0.97 || fv < 0.06 || fv > 0.94;
                // A stripe of ink under the name, the way a team board carries its colours.
                let stripe = (0.78..0.84).contains(&fv);
                let c = if hem { ground.map(|v| (v as f32 * 0.6) as u8) } else if stripe { *ink } else { *ground };
                let i = ((y * n + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        print_text(&mut px, n, n, name, (x0 as f32 + cw as f32 * 0.5, y0 as f32 + ch as f32 * 0.45), (cw as f32 * 0.84, ch as f32 * 0.5), *ink);
    }
    Texture { name: "paddock_brands_c".into(), width: n, height: n, rgba: px }
}

/// Map a mesh's 0..1 UVs into a window of a sheet.
fn into_window(m: &Mesh, (u0, v0, uw, vh): (f32, f32, f32, f32)) -> Mesh {
    let mut o = m.clone();
    for uv in o.uvs.chunks_exact_mut(2) {
        uv[0] = u0 + uw * (0.01 + 0.98 * uv[0]);
        uv[1] = v0 + vh * (0.01 + 0.98 * uv[1]);
    }
    o
}

/// All UVs on one texel of a window's hem.
fn on_hem(m: &Mesh, (u0, v0, _, vh): (f32, f32, f32, f32)) -> Mesh {
    let mut o = m.clone();
    for uv in o.uvs.chunks_exact_mut(2) {
        uv[0] = u0 + 0.004;
        uv[1] = v0 + vh * 0.5;
    }
    o
}

/// A team board on two posts, printed both sides, facing +z, in brand `i`'s cell.
fn board_mesh(i: usize) -> Mesh {
    let c = cell(i);
    let face = edfwrite::moved(&into_window(&edfwrite::card(BOARD_W_M, BOARD_H_M), c), [0.0, BOARD_LIFT_M, 0.0]);
    let mut m = edfwrite::printed_both_sides(&face);
    for x in [-1.0f32, 1.0] {
        let post = on_hem(&edfwrite::cuboid(0.1, BOARD_LIFT_M + BOARD_H_M, 0.1), c);
        m.append(&edfwrite::moved(&post, [x * (BOARD_W_M * 0.5 - 0.2), 0.0, -0.08]));
    }
    m
}

/// A team's pop-up canopy in brand `i`'s colour: four legs, a valance, a pitched roof, seen
/// from above and below. The library's `tent_sides` are walls with no roof.
const CANOPY_M: (f32, f32, f32) = (4.5, 2.4, 3.1);
fn canopy_mesh(i: usize) -> Mesh {
    let (w, eave, apex) = CANOPY_M;
    let (u0, v0, uw, vh) = cell(i);
    // A ground texel clear of the name, and the hem's darker one for the legs.
    let (gu, gv) = (u0 + uw * 0.12, v0 + vh * 0.14);
    let hem = (u0, v0, uw, vh);
    let h = w * 0.5;
    let mut m = Mesh::default();
    let tri = |m: &mut Mesh, p: [[f32; 3]; 3]| {
        let base = m.vertex_count() as u32;
        let (a, b, c) = (p[0], p[1], p[2]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
        for q in p {
            m.positions.extend_from_slice(&q);
            m.normals.extend_from_slice(&[n[0] / l, n[1] / l, n[2] / l]);
            m.uvs.extend_from_slice(&[gu, gv]);
        }
        m.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 1]);
    };
    let c = [[-h, eave, -h], [h, eave, -h], [h, eave, h], [-h, eave, h]];
    for k in 0..4 {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        tri(&mut m, [a, b, [0.0, apex, 0.0]]);
        // The valance under each eave.
        let (a2, b2) = ([a[0], eave - 0.3, a[2]], [b[0], eave - 0.3, b[2]]);
        tri(&mut m, [a, b, b2]);
        tri(&mut m, [a, b2, a2]);
    }
    for (x, z) in [(-h, -h), (h, -h), (h, h), (-h, h)] {
        let leg = on_hem(&edfwrite::cuboid(0.06, eave, 0.06), hem);
        m.append(&edfwrite::moved(&leg, [x * 0.97, 0.0, z * 0.97]));
    }
    m
}

/// Our own box trailer in brand `i`'s colours, long in x.
fn own_rig(i: usize) -> Mesh {
    into_window(&edfwrite::cuboid(15.0, 4.0, 2.55), cell(i))
}

/// Packed dirt, from the ground sheet, with two wheel tracks down it: `u` across, `v` along.
pub fn road_sheet() -> Texture {
    let n = 256u32;
    let img = image::load_from_memory(include_bytes!("../assets/ground/soil_light_c.jpg"))
        .map(|i| image::imageops::resize(&i.to_rgba8(), n, n, image::imageops::FilterType::Triangle))
        .ok();
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / n as f32;
            let base = img.as_ref().map(|i| i.get_pixel(x, y).0).unwrap_or([150, 120, 90, 255]);
            let rut = [0.3f32, 0.7].iter().map(|c| (-((u - c) / 0.06).powi(2)).exp()).sum::<f32>();
            let k = 0.92 - 0.22 * rut.min(1.0);
            px.extend_from_slice(&[(base[0] as f32 * k) as u8, (base[1] as f32 * k) as u8, (base[2] as f32 * k) as u8, 255]);
        }
    }
    Texture { name: "paddock_road_c".into(), width: n, height: n, rgba: px }
}

/// A net fence panel, when the library has no barrier: a cut-out grid in a frame.
fn net_sheet() -> Texture {
    let n = 64u32;
    let mut px = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let c = if v < 0.05 || v > 0.95 || u < 0.03 || u > 0.97 {
                [110, 112, 116, 255]
            } else if (u * 16.0).fract() < 0.2 || (v * 12.0).fract() < 0.2 {
                [30, 90, 40, 230]
            } else {
                [0, 0, 0, 0]
            };
            px.extend_from_slice(&c);
        }
    }
    Texture { name: "paddock_net_c_a".into(), width: n, height: n, rgba: px }
}

/// Our own sponsor wall's print: every team's board across it, a plain band at the foot.
pub fn own_wall_sheet() -> Texture {
    let (w, h) = (2048u32, 256u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let band = (h as f32 * 0.8) as u32;
    let cells = BRANDS.len() as u32 * 2;
    let cw = w / cells;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let c = if y >= band {
                [90, 92, 96]
            } else {
                let (_, g, _) = BRANDS[(x / cw) as usize % BRANDS.len()];
                if (x % cw) < 4 || y < 6 || y + 6 > band { [12, 12, 14] } else { g }
            };
            px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    for k in 0..cells {
        let (name, _, ink) = BRANDS[k as usize % BRANDS.len()];
        print_text(&mut px, w, h, name, ((k * cw) as f32 + cw as f32 * 0.5, band as f32 * 0.5), (cw as f32 * 0.8, band as f32 * 0.5), ink);
    }
    Texture { name: "sponsor_wall_c".into(), width: w, height: h, rgba: px }
}

/// A draped strip along a polyline, `w` wide, three vertices across, both windings; no quad
/// with a corner where `skip` says.
fn strip(pts: &[(f32, f32)], w: f32, syn: &Synth, skip: &dyn Fn(f32, f32) -> bool) -> Mesh {
    let mut line = vec![pts[0]];
    for s in pts.windows(2) {
        let len = (s[1].0 - s[0].0).hypot(s[1].1 - s[0].1);
        let n = (len / 1.0).ceil().max(1.0) as usize;
        for k in 1..=n {
            let t = k as f32 / n as f32;
            line.push((s[0].0 + (s[1].0 - s[0].0) * t, s[0].1 + (s[1].1 - s[0].1) * t));
        }
    }
    let mut m = Mesh::default();
    let side = |i: usize| {
        let (a, b) = (line[i.saturating_sub(1)], line[(i + 1).min(line.len() - 1)]);
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let l = dx.hypot(dz).max(1e-6);
        (-dz / l, dx / l)
    };
    for i in 0..line.len() - 1 {
        let corners = [i, i + 1].into_iter().flat_map(|j| {
            let (nx, nz) = side(j);
            [-1.0f32, 1.0].map(|k| (line[j].0 + nx * k * w * 0.5, line[j].1 + nz * k * w * 0.5))
        });
        if corners.chain([line[i]]).any(|(x, z)| skip(x, z)) {
            continue;
        }
        let base = m.vertex_count() as u32;
        let v0 = (i % 4) as f32 * 0.25;
        for (j, v) in [(i, v0), (i + 1, v0 + 0.25)] {
            let (nx, nz) = side(j);
            for k in 0..3 {
                let t = (k as f32 - 1.0) * w * 0.5;
                let (x, z) = (line[j].0 + nx * t, line[j].1 + nz * t);
                m.positions.extend_from_slice(&[x, ground(syn, x, z) + ROAD_LIFT_M, z]);
                m.normals.extend_from_slice(&[0.0, 1.0, 0.0]);
                m.uvs.extend_from_slice(&[k as f32 * 0.5, v]);
            }
        }
        for k in 0..2u32 {
            let (a, b, c, d) = (base + k, base + k + 1, base + 3 + k + 1, base + 3 + k);
            m.indices.extend_from_slice(&[a, b, c, a, c, d, a, c, b, a, d, c]);
        }
    }
    m
}

/// The paddock's layout, before it is placed: rig size, tent size, extents.
struct Layout {
    rig_len: f32,
    rig_w: f32,
    tent: f32,
    len: f32,
    depth: f32,
}

impl Layout {
    fn of(rig_len: f32, rig_w: f32, tent: f32) -> Layout {
        let row = PAD_MARGIN_M + rig_w + 1.5 + tent + 2.0;
        let pitch = rig_len + SPOT_GAP_M;
        Layout { rig_len, rig_w, tent, len: 4.0 * pitch + GATE_M + 2.0 * PAD_MARGIN_M, depth: 2.0 * row + AISLE_M }
    }
    /// Spot `k`'s middle along the paddock, two a side of the gate gap.
    fn x(&self, k: usize) -> f32 {
        let pitch = self.rig_len + SPOT_GAP_M;
        let half = self.len * 0.5 - PAD_MARGIN_M;
        if k < 2 { -half + pitch * (k as f32 + 0.5) } else { half - pitch * (3 - k) as f32 - pitch * 0.5 }
    }
}

/// Where the paddock goes, and the road from its gate to the lane: anywhere on the plot clear
/// of every leg of the lap and the start, on ground that falls little, as near the pits as it
/// can be. The road never crosses the start; it crosses the lap only where the lane is boxed in
/// by it, and the fewer metres of track it crosses the better. Returns the crossings too.
fn site(prog: &TrackProgram, syn: &Synth, pits: &Lane, lay: &Layout) -> Option<(Rect, Vec<(f32, f32)>, usize)> {
    let lap = prog.lap_length();
    let half = prog.width * 0.5;
    let st = prog.stations(0.5);
    let at = |s: f32| st[((s.rem_euclid(lap) / 0.5) as usize).min(st.len() - 1)];
    // Where the road may end: the lane's outer edge beside each stall.
    let ends: Vec<(f32, f32)> = pits
        .stalls
        .iter()
        .map(|&(long, _)| {
            let p = at(long);
            let pr = crate::trackprog::right_vector(p.heading);
            let reach = (pits.lane + LANE_HALF_M - 1.0) * pits.side;
            (p.x + pr.0 * reach, p.z + pr.1 * reach)
        })
        .collect();
    let (hx, hz) = (lay.len * 0.5, lay.depth * 0.5);
    const GROW: f32 = 2.0;
    let fall = |area: &Rect| -> Option<f32> {
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for i in 0..=((hx + GROW) as i32) {
            for j in 0..=((hz + GROW) as i32) {
                let (x, z) = area.at(-hx - GROW + i as f32 * 2.0, -hz - GROW + j as f32 * 2.0);
                if !on_plot(prog, x, z, 2.0) || dist(syn, x, z) <= half + PAD_CLEAR_M || syn.outside_the_start(x, z).is_some_and(|e| e <= 4.0) {
                    return None;
                }
                let y = ground(syn, x, z);
                (lo, hi) = (lo.min(y), hi.max(y));
            }
        }
        (hi - lo <= PAD_FALL_M).then_some(hi - lo)
    };
    // A straight road from the gate: its length and metres on the track, or `None` if it
    // leaves the plot, touches the start or runs back through the paddock.
    let road = |area: &Rect, gate: (f32, f32), end: (f32, f32)| -> Option<(f32, usize, usize)> {
        let len = (end.0 - gate.0).hypot(end.1 - gate.1);
        let n = len.ceil().max(1.0) as usize;
        let (mut on, mut runs, mut was) = (0usize, 0usize, false);
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let (x, z) = (gate.0 + (end.0 - gate.0) * t, gate.1 + (end.1 - gate.1) * t);
            if !on_plot(prog, x, z, 2.0) || syn.outside_the_start(x, z).is_some_and(|e| e <= 1.5) || (t * len > 1.0 && area.covers(x, z, 0.0)) {
                return None;
            }
            let track = dist(syn, x, z) < half + 2.0;
            on += track as usize;
            runs += (track && !was) as usize;
            was = track;
        }
        Some((len, on, runs))
    };
    let q = at((pits.stalls[0].0 + pits.stalls[pits.stalls.len() - 1].0) * 0.5);
    let mut best: Option<(f32, Rect, Vec<(f32, f32)>, usize)> = None;
    for base in [q.heading, 0.0] {
        for k in 0..4 {
            let psi = base + k as f32 * std::f32::consts::FRAC_PI_2;
            let (x, z) = (crate::trackprog::heading_vector(psi), crate::trackprog::right_vector(psi));
            let mut cz = hz;
            while cz < prog.terrain.size_z - hz {
                let mut cx = hz;
                while cx < prog.terrain.size_x - hz {
                    let c = (cx, cz);
                    cx += 4.0;
                    if dist(syn, c.0, c.1) <= half + PAD_CLEAR_M + hz {
                        continue;
                    }
                    let area = Rect { c, x, z, hx, hz };
                    let gate = area.at(0.0, -hz);
                    let near = ends.iter().map(|e| (e.0 - gate.0).hypot(e.1 - gate.1)).fold(f32::INFINITY, f32::min);
                    if best.as_ref().is_some_and(|b| b.0 <= near) {
                        continue;
                    }
                    let Some(f) = fall(&area) else { continue };
                    for &end in &ends {
                        let Some((len, on, runs)) = road(&area, gate, end) else { continue };
                        let score = len + 3.0 * f + 25.0 * on as f32 + 80.0 * runs as f32;
                        if best.as_ref().is_none_or(|b| score < b.0) {
                            best = Some((score, area, vec![area.at(0.0, -AISLE_M * 0.5), gate, end], runs));
                        }
                    }
                }
                cz += 4.0;
            }
        }
    }
    best.map(|b| (b.1, b.2, b.3))
}

/// The paddock: fence, a rig, a tent and a board per team, the road in, and its kinds.
fn paddock(prog: &TrackProgram, syn: &Synth, lib: Option<&crate::trackprops::PropLibrary>, v: &mut Venue) {
    use crate::trackobjects::Class;
    let pits = lane(prog);
    // Rigs: the big single semis, else box trucks; our own boxes without a library.
    let pool = |lo: f32, hi: f32, h: (f32, f32)| -> Vec<(&crate::trackprops::Prop, Mesh)> {
        let Some(lib) = lib else { return Vec::new() };
        let mut v: Vec<_> = lib
            .props
            .iter()
            .filter(|p| p.class == Class::Vehicle && whole(p) && (h.0..=h.1).contains(&p.height) && lib.sheets.iter().any(|s| s.0 == p.sheet))
            .filter_map(|p| {
                let a = aligned(&p.mesh);
                let (l, w) = { let (lo, hi) = a.bounds(); (hi[0] - lo[0], hi[2] - lo[2]) };
                ((lo..=hi).contains(&l) && (2.0..=4.5).contains(&w)).then_some((p, a))
            })
            .collect();
        v.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        v
    };
    let mut rigs = pool(17.0, 21.0, (3.8, 5.4));
    if rigs.is_empty() {
        rigs = pool(9.0, 16.0, (2.8, 5.0));
    }
    let picked: Vec<Option<&(&crate::trackprops::Prop, Mesh)>> =
        (0..BRANDS.len()).map(|i| (!rigs.is_empty()).then(|| &rigs[i * rigs.len() / BRANDS.len()])).collect();
    let dims = |m: &Mesh| { let (lo, hi) = m.bounds(); (hi[0] - lo[0], hi[2] - lo[2]) };
    let (rig_len, rig_w) = picked.iter().fold((0.0f32, 0.0f32), |acc, r| {
        let (l, w) = r.map_or((15.0, 2.55), |r| dims(&r.1));
        (acc.0.max(l), acc.1.max(w))
    });
    let lay = Layout::of(rig_len, rig_w, CANOPY_M.0);
    let Some((area, road, crossings)) = site(prog, syn, &pits, &lay) else {
        v.tally.push(("paddock", 0));
        return;
    };

    let (f, o) = (area.x, area.z);
    let (mut rig_mesh, mut own_rigs, mut tent_mesh, mut boards) = (Mesh::default(), Mesh::default(), Mesh::default(), Mesh::default());
    let mut spots = Vec::new();
    for (i, (brand, ..)) in BRANDS.iter().enumerate() {
        // Far row first, then the near one; each rig backs onto the fence, facing the aisle.
        let (k, row) = (i % 4, if i < 4 { 1.0f32 } else { -1.0 });
        let x = lay.x(k);
        let back = lay.depth * 0.5 - PAD_MARGIN_M;
        let (rx, rz) = area.at(x, row * (back - lay.rig_w * 0.5));
        let deg = deg_along(f);
        match picked[i] {
            Some((_, m)) => rig_mesh.append(&stand(m, rx, rz, deg, syn)),
            None => own_rigs.append(&stand(&own_rig(i), rx, rz, deg, syn)),
        }
        let front = back - lay.rig_w - 1.5;
        let (tx, tz) = area.at(x + lay.rig_len * 0.18, row * (front - lay.tent * 0.5));
        tent_mesh.append(&stand(&canopy_mesh(i), tx, tz, deg, syn));
        let tent = Some((tx, tz));
        let (bx, bz) = area.at(x - lay.rig_len * 0.25, row * (AISLE_M * 0.5 + 0.8));
        // Faces the aisle.
        boards.append(&stand(&board_mesh(i), bx, bz, deg_facing((-o.0 * row, -o.1 * row)), syn));
        spots.push(Spot {
            brand,
            rig: Rect { c: (rx, rz), x: f, z: o, hx: lay.rig_len * 0.5, hz: lay.rig_w * 0.5 },
            board: (bx, bz),
            tent,
        });
    }

    // The fence: panel after panel round the edge, a gap for the gate in the near side.
    let barrier = lib.and_then(|l| l.props.iter().find(|p| p.id == "edge_barrier").filter(|p| lib_tex(l, &p.sheet).is_some()));
    let post = lib.and_then(|l| l.props.iter().find(|p| p.id == "edge_post").filter(|p| lib_tex(l, &p.sheet).is_some()));
    let step = barrier.map_or(3.0, |p| p.span.clamp(1.5, 4.0));
    let (mut fence, mut posts, mut centres) = (Mesh::default(), Mesh::default(), Vec::new());
    let (hx, hz) = (area.hx, area.hz);
    for (a, b) in [((-hx, -hz), (hx, -hz)), ((hx, -hz), (hx, hz)), ((hx, hz), (-hx, hz)), ((-hx, hz), (-hx, -hz))] {
        let (pa, pb) = (area.at(a.0, a.1), area.at(b.0, b.1));
        let len = (pb.0 - pa.0).hypot(pb.1 - pa.1);
        let n = (len / step).round().max(1.0) as usize;
        let d = ((pb.0 - pa.0) / len, (pb.1 - pa.1) / len);
        let heading = d.0.atan2(d.1);
        for k in 0..n {
            let t0 = k as f32 * len / n as f32;
            let tc = t0 + len / n as f32 * 0.5;
            let (cx, cz) = (pa.0 + d.0 * tc, pa.1 + d.1 * tc);
            let (lx, lz) = area.local(cx, cz);
            if lz < -hz + 0.5 && lx.abs() < GATE_M * 0.5 {
                continue;
            }
            centres.push((cx, cz));
            match barrier {
                Some(p) => fence.append(&stand(&p.mesh, cx, cz, (heading - p.axis_ref).to_degrees(), syn)),
                None => fence.append(&stand(&edfwrite::cuboid(0.04, 1.8, len / n as f32), cx, cz, heading.to_degrees(), syn)),
            }
            if let Some(p) = post {
                let (qx, qz) = (pa.0 + d.0 * t0, pa.1 + d.1 * t0);
                posts.append(&stand(&p.mesh, qx, qz, (heading - p.axis_ref).to_degrees(), syn));
            }
        }
    }

    // The road: in along the aisle, then out through the gate to the lane.
    // Not drawn on the riding surface or the start: it meets the track edge either side.
    let half = prog.width * 0.5;
    let off = |x: f32, z: f32| dist(syn, x, z) < half + 1.5 || syn.outside_the_start(x, z).is_some_and(|e| e < 0.5);
    let aisle = strip(&[area.at(-hx + PAD_MARGIN_M, 0.0), area.at(hx - PAD_MARGIN_M, 0.0)], AISLE_ROAD_W_M, syn, &off);
    let mut road_mesh = strip(&road, ROAD_W_M, syn, &off);
    road_mesh.append(&aisle);
    let road_len: f32 = road.windows(2).map(|s| (s[1].0 - s[0].0).hypot(s[1].1 - s[0].1)).sum();

    let n_rigs = spots.len();
    let n_tents = spots.iter().filter(|s| s.tent.is_some()).count();
    let lib_sheet = |sheet: &str| lib.and_then(|l| lib_tex(l, sheet));
    if let Some((p, _)) = picked.iter().flatten().next() {
        if let Some(t) = lib_sheet(&p.sheet) {
            v.kinds.push(("paddock_rigs".into(), rig_mesh, t, true));
        }
    }
    if own_rigs.vertex_count() > 0 {
        v.kinds.push(("paddock_trucks".into(), own_rigs, brand_sheet(), true));
    }
    v.kinds.push(("paddock_canopies".into(), tent_mesh, brand_sheet(), true));
    v.kinds.push(("paddock_boards".into(), boards, brand_sheet(), true));
    match barrier.and_then(|p| lib_sheet(&p.sheet)) {
        Some(t) => v.kinds.push(("paddock_fence".into(), fence, t, true)),
        None => v.kinds.push(("paddock_fence".into(), fence, net_sheet(), true)),
    }
    if let Some(t) = post.and_then(|p| lib_sheet(&p.sheet)) {
        v.kinds.push(("paddock_posts".into(), posts, t, true));
    }
    v.kinds.push(("paddock_road".into(), road_mesh, road_sheet(), false));
    v.tally.extend([
        ("paddock", 1),
        ("paddock rigs", n_rigs),
        ("paddock canopies", n_tents),
        ("paddock boards", BRANDS.len()),
        ("paddock fence panels", centres.len()),
        ("paddock road m", road_len.round() as usize),
        ("paddock road crossings", crossings),
    ]);
    v.paddock = Some(Paddock { area, spots, fence: centres });
    v.road = road;
}

/// The sponsor wall behind the gate row, facing it, clear of the start pad: the library's
/// lifted one when it has it, our own printed board when it does not.
fn wall(prog: &TrackProgram, syn: &Synth, lib: Option<&crate::trackprops::PropLibrary>, v: &mut Venue) {
    let Some(spur) = &syn.spur else {
        v.tally.push(("sponsor wall", 0));
        return;
    };
    let half = prog.width * 0.5;
    let gi = ((spur.gate_at() / 0.5) as usize).min(spur.stations.len().saturating_sub(1));
    let g = spur.stations[gi];
    let f = crate::trackprog::heading_vector(g.heading);
    let r = crate::trackprog::right_vector(g.heading);
    let parts: Vec<(&crate::trackprops::Prop, Texture)> = lib
        .map(|l| {
            WALL_PARTS
                .iter()
                .filter_map(|(id, _)| l.props.iter().find(|p| p.id == *id))
                .filter_map(|p| lib_tex(l, &p.sheet).map(|t| (p, t)))
                .collect()
        })
        .unwrap_or_default();
    let lifted = parts.iter().any(|(p, _)| p.id == WALL_PARTS[0].0);
    // Turned onto our gate the way the donor's stood behind its own; ours faces +z.
    let meshes: Vec<(String, Mesh, Texture)> = if lifted {
        parts
            .iter()
            .map(|(p, t)| {
                let m = edfwrite::turned(&p.mesh, (g.heading - p.axis_ref).to_degrees());
                (p.id.clone(), if m.vertex_count() < 64 { fine(&fine(&m)) } else { m }, t.clone())
            })
            .collect()
    } else {
        let (w, h, lift) = OWN_WALL_M;
        let face = edfwrite::moved(&into_window(&edfwrite::card(w, h), (0.0, 0.0, 1.0, 0.8)), [0.0, lift, 0.0]);
        let mut m = face;
        let grey = (0.0, 0.85, 1.0, 0.1);
        let back = edfwrite::turned(&edfwrite::moved(&on_hem(&edfwrite::card(w, h), grey), [0.0, lift, 0.0]), 180.0);
        m.append(&edfwrite::moved(&back, [0.0, 0.0, -0.05]));
        for k in 0..9 {
            let post = on_hem(&edfwrite::cuboid(0.2, lift + h + 0.2, 0.2), grey);
            m.append(&edfwrite::moved(&post, [(k as f32 / 8.0 - 0.5) * (w - 0.4), 0.0, -0.2]));
        }
        vec![("sponsor_wall".into(), edfwrite::turned(&m, g.heading.to_degrees()), own_wall_sheet())]
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (_, m, _) in &meshes {
        for p in m.positions.chunks_exact(3) {
            let (a, c) = (p[0] * f.0 + p[2] * f.1, p[0] * r.0 + p[2] * r.1);
            (lo, hi) = ([lo[0].min(a), lo[1].min(c)], [hi[0].max(a), hi[1].max(c)]);
        }
    }
    let (mut back, mut placed) = (WALL_BACK_M.0, None);
    while back <= WALL_BACK_M.1 {
        let (cx, cz) = (g.x - f.0 * back, g.z - f.1 * back);
        let foot = Rect { c: (cx + f.0 * (lo[0] + hi[0]) * 0.5 + r.0 * (lo[1] + hi[1]) * 0.5, cz + f.1 * (lo[0] + hi[0]) * 0.5 + r.1 * (lo[1] + hi[1]) * 0.5), x: r, z: f, hx: (hi[1] - lo[1]) * 0.5, hz: (hi[0] - lo[0]) * 0.5 };
        let n = (foot.hx * 2.0) as i32;
        let ok = (0..=n).all(|i| {
            [-1.0f32, 0.0, 1.0].iter().all(|&j| {
                let (x, z) = foot.at(-foot.hx + i as f32 * foot.hx * 2.0 / n as f32, j * foot.hz);
                on_plot(prog, x, z, 2.0) && dist(syn, x, z) > half + 3.0 && syn.outside_the_start(x, z).is_none_or(|e| e > WALL_OFF_PAD_M)
            })
        });
        if ok {
            placed = Some((cx, cz, foot));
            break;
        }
        back += 0.5;
    }
    let Some((cx, cz, foot)) = placed else {
        v.tally.push(("sponsor wall", 0));
        return;
    };
    // On the lowest ground along it: its legs reach into the higher.
    let fall = (0..=20)
        .map(|i| {
            let (x, z) = foot.at(-foot.hx + i as f32 * foot.hx / 10.0, 0.0);
            ground(syn, x, z)
        })
        .fold(f32::INFINITY, f32::min);
    for (name, m, t) in meshes {
        v.kinds.push((name, edfwrite::moved(&m, [cx, fall - 0.1, cz]), t, true));
    }
    v.tally.push(("sponsor wall", 1));
    v.tally.push(("sponsor wall back m", back.round() as usize));
    v.wall = Some(Wall { foot, faces: g.heading, lifted });
}

/// The venue for a track: the paddock and its road, and the sponsor wall.
pub fn build(prog: &TrackProgram, syn: &Synth, lib: Option<&crate::trackprops::PropLibrary>) -> Venue {
    let mut v = Venue { kinds: Vec::new(), tally: Vec::new(), paddock: None, road: Vec::new(), wall: None };
    paddock(prog, syn, lib, &mut v);
    wall(prog, syn, lib, &mut v);
    v
}

/// Drop every separate piece of `kinds` whose middle, grown by its own size, lands on the
/// venue: a tree in the paddock, a lifted tent on the road. The ground bank stays whole.
fn clear(kinds: &mut [Kind], v: &Venue) -> usize {
    let mut dropped = 0;
    for (name, m, ..) in kinds.iter_mut() {
        // The ground bank, the stalls' stands, and what spans the track stay whole.
        let kept = name == "backdrop" || name == "pit_stands" || name == "gate" || name == "finish_arch";
        if kept || m.vertex_count() == 0 {
            continue;
        }
        let n = m.vertex_count();
        let mut up: Vec<usize> = (0..n).collect();
        fn root(up: &mut [usize], mut i: usize) -> usize {
            while up[i] != i {
                up[i] = up[up[i]];
                i = up[i];
            }
            i
        }
        for t in m.indices.chunks_exact(3) {
            let a = root(&mut up, t[0] as usize);
            for &j in &t[1..] {
                let b = root(&mut up, j as usize);
                if a != b {
                    up[b] = a;
                }
            }
        }
        let mut bx: std::collections::HashMap<usize, [f32; 4]> = Default::default();
        for i in 0..n {
            let r = root(&mut up, i);
            let (x, z) = (m.positions[i * 3], m.positions[i * 3 + 2]);
            let e = bx.entry(r).or_insert([f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
            *e = [e[0].min(x), e[1].min(z), e[2].max(x), e[3].max(z)];
        }
        let gone: std::collections::HashSet<usize> = bx
            .iter()
            .filter(|(_, b)| {
                let r = ((b[2] - b[0]).hypot(b[3] - b[1]) * 0.5).min(6.0);
                v.covers((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5, r)
            })
            .map(|(k, _)| *k)
            .collect();
        if gone.is_empty() {
            continue;
        }
        dropped += gone.len();
        let mut out = Mesh::default();
        let mut remap: std::collections::HashMap<u32, u32> = Default::default();
        for t in m.indices.chunks_exact(3) {
            if gone.contains(&root(&mut up, t[0] as usize)) {
                continue;
            }
            for &i in t {
                let j = *remap.entry(i).or_insert_with(|| {
                    let k = i as usize;
                    out.positions.extend_from_slice(&m.positions[k * 3..k * 3 + 3]);
                    out.normals.extend_from_slice(&m.normals[k * 3..k * 3 + 3]);
                    out.uvs.extend_from_slice(&m.uvs[k * 2..k * 2 + 2]);
                    (out.positions.len() / 3 - 1) as u32
                });
                out.indices.push(j);
            }
        }
        *m = out;
    }
    dropped
}

/// The hook `trackscenery::build` calls: clears what stood where the venue goes, then adds it.
pub fn dress(
    prog: &TrackProgram,
    syn: &Synth,
    lib: Option<&crate::trackprops::PropLibrary>,
    kinds: &mut Vec<Kind>,
    tally: &mut Vec<(&'static str, usize)>,
) {
    let v = build(prog, syn, lib);
    tally.push(("venue cleared", clear(kinds, &v)));
    tally.extend(v.tally);
    kinds.extend(v.kinds);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trackobjects::Class;
    use crate::trackprops::{Prop, PropLibrary};

    /// Northgate, built the way the app builds it, once for every test here.
    fn northgate() -> &'static (TrackProgram, Synth) {
        static N: std::sync::OnceLock<(TrackProgram, Synth)> = std::sync::OnceLock::new();
        N.get_or_init(|| {
            let mut p = match crate::tracklayout::search(103, 1) { Ok(m) => m.program, Err(v) => v[0].program.clone() };
            p.terrain.surface = serde_json::from_str("\"soil\"").unwrap();
            let p = crate::tracksynth::with_fitted_budget(&p).unwrap();
            let s = crate::tracksynth::synthesise(&p).unwrap();
            (p, s)
        })
    }

    fn prop(id: &str, sheet: &str, class: Class, mesh: Mesh, axis_ref: f32) -> Prop {
        let (lo, hi) = mesh.bounds();
        let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
        Prop { id: id.into(), sheet: sheet.into(), class, height: hi[1] - lo[1], span: (hi[0] - lo[0]).max(hi[2] - lo[2]), reach, axis_ref, mesh }
    }

    /// A library with what the venue asks for: semis, pop-ups, a barrier and post, a wall.
    fn library() -> PropLibrary {
        let wall = edfwrite::moved(&edfwrite::card(50.0, 4.0), [0.0, 1.0, 0.0]);
        let props = vec![
            prop("semi_a", "semi_trailers_c", Class::Vehicle, edfwrite::cuboid(19.7, 4.6, 3.2), 0.0),
            prop("semi_b", "semi_trailers_c", Class::Vehicle, edfwrite::cuboid(19.6, 4.8, 3.0), 0.0),
            prop("tent", "tent_sides_c", Class::Structure, edfwrite::cuboid(4.5, 2.8, 4.5), 0.0),
            prop("edge_barrier", "ck_fence_c_a", Class::Structure, edfwrite::cuboid(0.05, 1.36, 3.0), 0.0),
            prop("edge_post", "main_track_objects_c", Class::Structure, edfwrite::cuboid(0.05, 1.46, 0.05), 0.0),
            prop("sponsor_wall", "start_backdrop_c", Class::Structure, wall, 0.0),
            prop("sponsor_wall_frame", "main_track_objects_c", Class::Structure, edfwrite::moved(&edfwrite::cuboid(50.0, 5.8, 0.4), [0.0, 0.0, -0.5]), 0.0),
        ];
        let sheets = ["semi_trailers_c", "tent_sides_c", "ck_fence_c_a", "main_track_objects_c", "start_backdrop_c"]
            .iter()
            .map(|n| (n.to_string(), 2, 2, vec![200u8; 16]))
            .collect();
        PropLibrary { donor: "t".into(), donor_lap_m: 1000.0, props, instances: vec![], runs: vec![], sheets }
    }

    fn venue() -> &'static Venue {
        static V: std::sync::OnceLock<Venue> = std::sync::OnceLock::new();
        V.get_or_init(|| {
            let (p, s) = northgate();
            build(p, s, Some(&library()))
        })
    }

    fn verts<'a>(v: &'a Venue, prefix: &'a str) -> impl Iterator<Item = [f32; 3]> + 'a {
        v.kinds.iter().filter(move |k| k.0.starts_with(prefix)).flat_map(|k| k.1.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]))
    }

    /// The stalls here are the ones the `.rdf` spawns riders at.
    #[test]
    fn the_lane_is_the_rdf_s() {
        let (p, s) = northgate();
        let dir = std::env::temp_dir().join(format!("mxb-venue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let wrote = crate::tracksynth::write_source(p, s, &dir).unwrap();
        let rdf = wrote.iter().find(|f| f.ends_with(".rdf")).expect("an .rdf");
        let txt = std::fs::read_to_string(dir.join(rdf)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let block = &txt[txt.find("pit_lane").unwrap()..txt.find("pit_board").unwrap()];
        let num = |k: &str| block.lines().filter_map(|l| l.trim().strip_prefix(k).map(|v| v.parse::<f32>().unwrap())).collect::<Vec<_>>();
        let (longs, lats) = (num("long = "), num("lat = "));
        let l = lane(p);
        assert_eq!(longs.len(), l.stalls.len());
        for ((a, b), &(sa, sb)) in longs.iter().zip(&lats).zip(&l.stalls) {
            assert!((a - sa).abs() < 0.01 && (b - sb).abs() < 0.01, "{a}/{b} in the .rdf, {sa}/{sb} here");
        }
    }

    /// A fence all the way round but the gate, and nothing of the paddock near the track.
    #[test]
    fn the_paddock_is_enclosed_and_clear_of_the_track() {
        let (p, s) = northgate();
        let v = venue();
        let pad = v.paddock.as_ref().expect("Northgate gets a paddock");
        let a = pad.area;
        assert!(a.hx * 2.0 >= 80.0 && a.hz * 2.0 >= 30.0, "{:.0} x {:.0} m is not a big paddock", a.hx * 2.0, a.hz * 2.0);
        // Every metre of the edge has a panel within reach, except the gate.
        let per = [((-a.hx, -a.hz), (a.hx, -a.hz)), ((a.hx, -a.hz), (a.hx, a.hz)), ((a.hx, a.hz), (-a.hx, a.hz)), ((-a.hx, a.hz), (-a.hx, -a.hz))];
        let mut gap = 0.0f32;
        for (s0, s1) in per {
            let len = (s1.0 - s0.0).hypot(s1.1 - s0.1);
            for k in 0..len as usize {
                let t = k as f32 / len;
                let (lx, lz) = (s0.0 + (s1.0 - s0.0) * t, s0.1 + (s1.1 - s0.1) * t);
                let (x, z) = a.at(lx, lz);
                if !pad.fence.iter().any(|c| (c.0 - x).hypot(c.1 - z) < 2.2) {
                    assert!(lz < -a.hz + 0.5 && lx.abs() <= GATE_M * 0.5 + 1.0, "no fence at ({lx:.0}, {lz:.0})");
                    gap += 1.0;
                }
            }
        }
        assert!(gap >= ROAD_W_M, "the gate is {gap} m, narrower than the road");
        let half = p.width * 0.5;
        for pre in ["paddock_rigs", "paddock_canopies", "paddock_boards", "paddock_fence", "paddock_posts"] {
            let mut n = 0;
            for q in verts(v, pre) {
                n += 1;
                assert!(dist(s, q[0], q[2]) > half + PAD_CLEAR_M - 1.0, "{pre} {:.1} m from the centreline", dist(s, q[0], q[2]));
                assert!(s.outside_the_start(q[0], q[2]).is_none_or(|e| e > 2.0), "{pre} on the start");
                assert!(a.covers(q[0], q[2], 1.0), "{pre} outside the fence");
                assert!((q[1] - ground(s, q[0], q[2])).abs() < 8.0, "{pre} off the ground");
            }
            assert!(n > 0, "no {pre}");
        }
    }

    /// Eight teams, each a rig, a tent and a board, and no two rigs in each other.
    #[test]
    fn a_rig_per_brand() {
        let v = venue();
        let pad = v.paddock.as_ref().expect("a paddock");
        let names: Vec<&str> = pad.spots.iter().map(|s| s.brand).collect();
        assert_eq!(names, BRANDS.iter().map(|b| b.0).collect::<Vec<_>>());
        for (i, a) in pad.spots.iter().enumerate() {
            assert!(pad.area.covers(a.rig.c.0, a.rig.c.1, -a.rig.hz), "{}'s rig outside", a.brand);
            assert!(a.tent.is_some(), "{} has no tent", a.brand);
            for b in &pad.spots[i + 1..] {
                let (dx, dz) = pad.area.local(b.rig.c.0, b.rig.c.1);
                let (ex, ez) = pad.area.local(a.rig.c.0, a.rig.c.1);
                assert!((dx - ex).abs() > a.rig.hx * 2.0 || (dz - ez).abs() > a.rig.hz * 2.0, "{} and {} overlap", a.brand, b.brand);
            }
        }
        assert_eq!(v.tally.iter().find(|t| t.0 == "paddock rigs").map(|t| t.1), Some(8));
        // Every board prints its own name: the cells differ.
        let sheet = brand_sheet();
        let mut seen = std::collections::HashSet::new();
        for i in 0..BRANDS.len() {
            let (u0, v0, uw, vh) = cell(i);
            let (x0, y0) = ((u0 * 1024.0) as u32, (v0 * 1024.0) as u32);
            let ink = BRANDS[i].2;
            let mut inked = 0;
            let mut sig = Vec::new();
            for y in (y0 + 20..y0 + (vh * 1024.0) as u32 - 20).step_by(3) {
                for x in (x0 + 20..x0 + (uw * 1024.0) as u32 - 20).step_by(3) {
                    let k = ((y * 1024 + x) * 4) as usize;
                    let is = sheet.rgba[k..k + 3] == ink;
                    inked += is as usize;
                    sig.push(is);
                }
            }
            assert!(inked > 200, "{}'s board prints nothing", BRANDS[i].0);
            assert!(seen.insert(sig), "{}'s board is another's", BRANDS[i].0);
        }
    }

    /// The road runs from inside the paddock, out through its gate, onto the pit lane.
    #[test]
    fn the_road_connects_the_paddock_to_the_pit_lane() {
        let (p, s) = northgate();
        let v = venue();
        let pad = v.paddock.as_ref().expect("a paddock");
        let (start, end) = (v.road[0], *v.road.last().unwrap());
        assert!(pad.area.covers(start.0, start.1, 0.0), "the road starts outside the paddock");
        // Through the gate: where it crosses the near fence line it is inside the gap.
        let cross = v.road.windows(2).find_map(|w| {
            let (a, b) = (pad.area.local(w[0].0, w[0].1), pad.area.local(w[1].0, w[1].1));
            let e = -pad.area.hz;
            let dz = b.1 - a.1;
            ((a.1 - e) * (b.1 - e) <= 0.0).then(|| if dz.abs() < 1e-6 { a.0 } else { a.0 + (b.0 - a.0) * (e - a.1) / dz })
        });
        let x = cross.expect("the road never leaves the paddock");
        assert!(x.abs() + ROAD_W_M * 0.5 <= GATE_M * 0.5 + 0.1, "the road meets the fence {x:.1} m off the gate");
        // Ends on the lane's strip, beside a stall.
        let l = lane(p);
        let (lap, st) = (p.lap_length(), p.stations(0.5));
        let on_lane = l.stalls.iter().any(|&(long, _)| {
            let q = st[((long / 0.5) as usize).min(st.len() - 1)];
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let out = ((end.0 - q.x) * rx + (end.1 - q.z) * rz) * l.side;
            let along = (end.0 - q.x) * q.heading.sin() + (end.1 - q.z) * q.heading.cos();
            (out - l.lane).abs() <= LANE_HALF_M && along.abs() <= STALL_GAP_M
        });
        assert!(on_lane, "the road ends off the pit lane");
        let _ = lap;
        // Never drawn on the track or the start, and across the lap at most once.
        let half = p.width * 0.5;
        let mut n = 0;
        for q in verts(v, "paddock_road") {
            n += 1;
            assert!(dist(s, q[0], q[2]) > half + 1.0, "road {:.1} m from the centreline", dist(s, q[0], q[2]));
            assert!(s.outside_the_start(q[0], q[2]).is_none_or(|e| e > 0.0), "road on the start");
        }
        assert!(n > 100, "hardly any road drawn");
        let crossings = v.tally.iter().find(|t| t.0 == "paddock road crossings").map(|t| t.1).unwrap();
        assert!(crossings <= 1, "the road crosses the lap {crossings} times");
    }

    /// The wall stands behind the gate row, square to it, printed side to the gates, off the pad.
    #[test]
    fn the_wall_stands_behind_the_gates() {
        let (p, s) = northgate();
        let v = venue();
        let w = v.wall.expect("a sponsor wall");
        assert!(w.lifted, "the library's wall was not used");
        let spur = s.spur.as_ref().expect("a start straight");
        let g = spur.stations[((spur.gate_at() / 0.5) as usize).min(spur.stations.len() - 1)];
        let (fx, fz) = crate::trackprog::heading_vector(g.heading);
        let (rx, rz) = crate::trackprog::right_vector(g.heading);
        let (dx, dz) = (w.foot.c.0 - g.x, w.foot.c.1 - g.z);
        let (along, across) = (dx * fx + dz * fz, dx * rx + dz * rz);
        assert!(along < -WALL_BACK_M.0 + 0.5 && along > -WALL_BACK_M.1 - 5.0, "the wall is {along:.1} m along from the gates");
        assert!(across.abs() < 3.0, "the wall is {across:.1} m off the gates' middle");
        assert!((w.foot.x.0 * fx + w.foot.x.1 * fz).abs() < 0.05, "the wall is not square to the row");
        assert!(w.foot.hx * 2.0 >= 40.0, "{:.0} m is not a wall", w.foot.hx * 2.0);
        let print = v.kinds.iter().find(|k| k.0 == "sponsor_wall").expect("the print");
        let n = print.1.normals.chunks_exact(3).fold((0.0, 0.0), |a, q| (a.0 + q[0], a.1 + q[2]));
        assert!(n.0 * fx + n.1 * fz > 0.0, "the print faces away from the gates");
        let half = p.width * 0.5;
        for k in v.kinds.iter().filter(|k| k.0.starts_with("sponsor_wall")) {
            for q in k.1.positions.chunks_exact(3) {
                assert!(s.outside_the_start(q[0], q[2]).is_none_or(|e| e > WALL_OFF_PAD_M - 0.5), "{} on the start pad", k.0);
                assert!(dist(s, q[0], q[2]) > half + 2.0, "{} on the track", k.0);
            }
        }
    }

    /// Every venue model is one `trackscenery::build` writes: eight vertices or more, whole.
    #[test]
    fn every_venue_model_is_written() {
        let (p, s) = northgate();
        // The lifted wall's print and tarp are single quads in a real library.
        let mut lib = library();
        for id in ["sponsor_wall"] {
            let w = lib.props.iter_mut().find(|q| q.id == id).unwrap();
            w.mesh = edfwrite::moved(&edfwrite::card(50.0, 4.0), [0.0, 1.0, 0.0]);
        }
        let v = build(p, s, Some(&lib));
        for (name, m, t, _) in &v.kinds {
            assert!(m.vertex_count() >= 8, "{name} has {} vertices and would not be written", m.vertex_count());
            assert!(m.indices.iter().all(|&i| (i as usize) < m.vertex_count()), "{name} indexes past its vertices");
            assert_eq!(m.uvs.len(), m.vertex_count() * 2, "{name}'s uvs");
            assert_eq!(m.normals.len(), m.vertex_count() * 3, "{name}'s normals");
            assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize, "{name}'s sheet");
        }
    }

    /// Without a library the paddock still has its trucks, fence and boards, and the wall is ours.
    #[test]
    fn without_a_library_the_venue_draws_its_own() {
        let (p, s) = northgate();
        let v = build(p, s, None);
        assert!(v.paddock.is_some());
        assert!(v.kinds.iter().any(|k| k.0 == "paddock_trucks" && k.1.triangle_count() >= 8 * 12));
        assert!(v.kinds.iter().any(|k| k.0 == "paddock_fence" && k.2.name == "paddock_net_c_a"));
        let w = v.wall.expect("our own wall");
        assert!(!w.lifted);
        assert!(v.kinds.iter().any(|k| k.0 == "sponsor_wall" && k.2.name == "sponsor_wall_c"));
    }

    /// A tree in the paddock goes; one out in the field stays.
    #[test]
    fn what_stood_in_the_paddock_is_cleared() {
        let v = venue();
        let pad = v.paddock.as_ref().unwrap();
        let (ix, iz) = pad.area.at(10.0, 3.0);
        let (ox, oz) = pad.area.at(0.0, pad.area.hz + 40.0);
        let mut m = edfwrite::moved(&edfwrite::crossed(4.0, 8.0, 2), [ix, 0.0, iz]);
        m.append(&edfwrite::moved(&edfwrite::crossed(4.0, 8.0, 2), [ox, 0.0, oz]));
        let tex = Texture { name: "t".into(), width: 1, height: 1, rgba: vec![0; 4] };
        let mut kinds = vec![("trees_x".to_string(), m, tex.clone(), true), ("backdrop".to_string(), edfwrite::moved(&edfwrite::card(2.0, 2.0), [ix, 0.0, iz]), tex, false)];
        assert_eq!(clear(&mut kinds, v), 2, "both cards of the one tree go");
        assert!(kinds[0].1.positions.chunks_exact(3).all(|q| (q[0] - ox).hypot(q[2] - oz) < 3.0));
        assert_eq!(kinds[1].1.vertex_count(), 4, "the bank is never cut");
    }

    /// Why a paddock does or doesn't fit: rejections by reason, and a clearance map.
    #[test]
    #[ignore = "diagnostic — set FROST_OUT"]
    fn why_no_paddock() {
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        let (p, s) = northgate();
        let l = lane(p);
        let lay = Layout::of(19.7, 3.2, 4.5);
        println!("paddock {:.0} x {:.0}, lane side {} at {:.1}, stalls {:?}..{:?}, plot {}x{}", lay.len, lay.depth, l.side, l.lane, l.stalls[0], l.stalls.last(), p.terrain.size_x, p.terrain.size_z);
        let half = p.width * 0.5;
        let (lap, st) = (p.lap_length(), p.stations(0.5));
        let at = |x: f32| st[((x.rem_euclid(lap) / 0.5) as usize).min(st.len() - 1)];
        let q = at((l.stalls[0].0 + l.stalls.last().unwrap().0) * 0.5);
        let f = crate::trackprog::heading_vector(q.heading);
        let r = crate::trackprog::right_vector(q.heading);
        let o = (r.0 * l.side, r.1 * l.side);
        let mut why = std::collections::BTreeMap::<&str, usize>::new();
        for gi in 0..40 {
            let g = 6.0 + gi as f32 * 4.0;
            for ai in 0..61 {
                let a = ((ai + 1) / 2) as f32 * 5.0 * if ai % 2 == 0 { 1.0 } else { -1.0 };
                let d = l.lane + LANE_HALF_M + g + lay.depth * 0.5;
                let area = Rect { c: (q.x + f.0 * a + o.0 * d, q.z + f.1 * a + o.1 * d), x: f, z: o, hx: lay.len * 0.5, hz: lay.depth * 0.5 };
                let mut reason = "";
                let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
                'g: for i in 0..=((lay.len + 6.0) / 2.0).ceil() as i32 {
                    for j in 0..=((lay.depth + 6.0) / 2.0).ceil() as i32 {
                        let (x, z) = area.at(-lay.len * 0.5 - 3.0 + i as f32 * 2.0, -lay.depth * 0.5 - 3.0 + j as f32 * 2.0);
                        if !on_plot(p, x, z, 3.0) { reason = "off plot"; break 'g; }
                        if dist(s, x, z) <= half + PAD_CLEAR_M { reason = "near track"; break 'g; }
                        if s.outside_the_start(x, z).is_some_and(|e| e <= 4.0) { reason = "on start"; break 'g; }
                        let y = ground(s, x, z);
                        (lo, hi) = (lo.min(y), hi.max(y));
                    }
                }
                if reason.is_empty() && hi - lo > PAD_FALL_M { reason = "falls"; }
                if reason.is_empty() { reason = "fits (road unchecked)"; println!("  fits at a {a} g {g}, fall {:.1}", hi - lo); }
                *why.entry(reason).or_default() += 1;
            }
        }
        println!("{why:?}");
        let (w, h) = (p.terrain.size_x as u32, p.terrain.size_z as u32);
        let mut img = image::RgbImage::new(w, h);
        let (glo, ghi) = s.heights.iter().fold((f32::MAX, f32::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
        for py in 0..h {
            for px in 0..w {
                let (x, z) = (px as f32 + 0.5, (h - py) as f32 - 0.5);
                let shade = ((ground(s, x, z) - glo) / (ghi - glo).max(1.0) * 80.0) as u8;
                let c = if dist(s, x, z) < half { [150, 110, 70] } else if s.outside_the_start(x, z).is_some_and(|e| e < 0.0) { [190, 140, 90] } else if dist(s, x, z) > half + PAD_CLEAR_M && s.outside_the_start(x, z).is_none_or(|e| e > 4.0) { [60 + shade, 150, 60] } else { [30, 70, 30] };
                img.put_pixel(px, py, image::Rgb(c));
            }
        }
        for &(long, lat) in &l.stalls {
            let q = at(long);
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let (x, z) = (q.x + rx * lat, q.z + rz * lat);
            if x >= 0.0 && z >= 0.0 && (x as u32) < w && (z as u32) < h {
                img.put_pixel(x as u32, h - 1 - z as u32, image::Rgb([255, 255, 255]));
            }
        }
        img.save(out.join("northgate_room.png")).unwrap();
    }

    /// Northgate from above round the pits, the paddock, the road and the start, textured.
    ///
    /// ```text
    /// FROST_PROPS=library12.fpl FROST_VENUE_PNG=venue.png cargo test --bins -- --ignored --nocapture draw_the_venue
    /// ```
    #[test]
    #[ignore = "draws the venue — set FROST_PROPS and FROST_VENUE_PNG"]
    fn draw_the_venue() {
        let path = std::env::var("FROST_VENUE_PNG").expect("set FROST_VENUE_PNG");
        let (p, s) = northgate();
        let sc = crate::trackscenery::build(p, s);
        let lib = crate::trackprops::load();
        let v = build(p, s, lib.as_ref());
        let pad = v.paddock.as_ref().expect("a paddock");
        let half = p.width * 0.5;
        let l = lane(p);
        let st = p.stations(0.5);
        let mut pts: Vec<(f32, f32)> = v.road.clone();
        for (a, b) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            pts.push(pad.area.at(a * (pad.area.hx + 10.0), b * (pad.area.hz + 10.0)));
        }
        if let Some(w) = v.wall {
            pts.push(w.foot.at(-w.foot.hx - 10.0, -20.0));
            pts.push(w.foot.at(w.foot.hx + 10.0, 60.0));
        }
        for &(long, lat) in &l.stalls {
            let q = st[((long / 0.5) as usize).min(st.len() - 1)];
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            pts.push((q.x - rx * lat, q.z - rz * lat));
        }
        let (x0, x1) = (pts.iter().map(|q| q.0).fold(f32::MAX, f32::min), pts.iter().map(|q| q.0).fold(f32::MIN, f32::max));
        let (z0, z1) = (pts.iter().map(|q| q.1).fold(f32::MAX, f32::min), pts.iter().map(|q| q.1).fold(f32::MIN, f32::max));
        let ppm = (2400.0 / (x1 - x0).max(z1 - z0)).min(8.0);
        let (w, h) = (((x1 - x0) * ppm) as u32, ((z1 - z0) * ppm) as u32);
        let mut img = image::RgbImage::new(w, h);
        for py in 0..h {
            for px in 0..w {
                let (x, z) = (x0 + px as f32 / ppm, z1 - py as f32 / ppm);
                let pit = l.stalls.iter().any(|&(long, _)| {
                    let q = st[((long / 0.5) as usize).min(st.len() - 1)];
                    let (rx, rz) = crate::trackprog::right_vector(q.heading);
                    let out = ((x - q.x) * rx + (z - q.z) * rz) * l.side;
                    let along = (x - q.x) * q.heading.sin() + (z - q.z) * q.heading.cos();
                    (out - l.lane).abs() <= LANE_HALF_M && along.abs() <= STALL_GAP_M * 0.5
                });
                let c = if dist(s, x, z) < half { [150, 110, 70] } else if s.outside_the_start(x, z).is_some_and(|e| e < 0.0) { [170, 125, 80] } else if pit { [205, 190, 150] } else { [72, 108, 58] };
                img.put_pixel(px, py, image::Rgb(c));
            }
        }
        let mut zb = vec![f32::MAX; (w * h) as usize];
        let proj = |x: f32, y: f32, z: f32| ((x - x0) * ppm, (z1 - z) * ppm, -y);
        // The road first, so what stands on it draws over it.
        let mut order: Vec<&(Mesh, Texture)> = sc.models.iter().collect();
        order.sort_by_key(|m| m.1.name != "paddock_road_c");
        for (m, t) in order {
            let (lo, hi) = m.bounds();
            if hi[0] < x0 || lo[0] > x1 || hi[2] < z0 || lo[2] > z1 {
                continue;
            }
            let tex = (t.width > 0).then(|| (t.width, t.height, t.rgba.as_slice()));
            pic::raster(&mut img, &mut zb, m, tex, [255, 0, 255], &proj);
        }
        for &(long, lat) in &l.stalls {
            let q = st[((long / 0.5) as usize).min(st.len() - 1)];
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            let (cx, cy) = ((q.x + rx * lat - x0) * ppm, (z1 - q.z - rz * lat) * ppm);
            for a in 0..64 {
                let t = a as f32 / 64.0 * std::f32::consts::TAU;
                let (px, py) = (cx + t.cos() * 1.2 * ppm, cy + t.sin() * 1.2 * ppm);
                if px >= 0.0 && py >= 0.0 && (px as u32) < w && (py as u32) < h {
                    img.put_pixel(px as u32, py as u32, image::Rgb([255, 255, 255]));
                }
            }
        }
        // Thin from above, so drawn over: the fence runs, the wall's line, each team's name.
        let dot = |img: &mut image::RgbImage, x: f32, z: f32, r: i32, c: [u8; 3]| {
            let (px, py) = (((x - x0) * ppm) as i32, ((z1 - z) * ppm) as i32);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (a, b) = (px + dx, py + dy);
                    if a >= 0 && b >= 0 && (a as u32) < w && (b as u32) < h {
                        img.put_pixel(a as u32, b as u32, image::Rgb(c));
                    }
                }
            }
        };
        for &(cx, cz) in &pad.fence {
            let (lx, lz) = pad.area.local(cx, cz);
            let d = if (lz.abs() - pad.area.hz).abs() < 0.5 && lx.abs() < pad.area.hx - 0.5 { pad.area.x } else { pad.area.z };
            for k in -6..=6 {
                dot(&mut img, cx + d.0 * k as f32 * 0.25, cz + d.1 * k as f32 * 0.25, 1, [40, 40, 44]);
            }
        }
        if let Some(wl) = v.wall {
            for k in 0..=(wl.foot.hx * 8.0) as i32 {
                let (x, z) = wl.foot.at(-wl.foot.hx + k as f32 * 0.25, 0.0);
                dot(&mut img, x, z, 2, [20, 20, 20]);
            }
            let (x, z) = wl.foot.at(-6.0, -6.0);
            pic::label(&mut img, "SPONSOR WALL", 22.0, (x - x0) * ppm, (z1 - z) * ppm, [255, 255, 255]);
        }
        for sp in &pad.spots {
            pic::label(&mut img, sp.brand, 20.0, (sp.board.0 - x0) * ppm - 20.0, (z1 - sp.board.1) * ppm, [255, 255, 255]);
        }
        img.save(&path).unwrap();
        println!("VENUE picture {path}: {w}x{h} at {ppm:.1} px/m");
        // And the wall as the gate row sees it.
        if let (Some(wl), Some(spur)) = (v.wall, s.spur.as_ref()) {
            let g = spur.stations[((spur.gate_at() / 0.5) as usize).min(spur.stations.len() - 1)];
            let (fx, fz) = crate::trackprog::heading_vector(g.heading);
            let (rx, rz) = crate::trackprog::right_vector(g.heading);
            let base = ground(s, wl.foot.c.0, wl.foot.c.1);
            let (ew, eh, eppm) = (70.0f32, 12.0f32, 16.0f32);
            let (iw, ih) = ((ew * eppm) as u32, (eh * eppm) as u32);
            let proj = |x: f32, y: f32, z: f32| {
                let (dx, dz) = (x - g.x, z - g.z);
                let (a, c) = (dx * fx + dz * fz, dx * rx + dz * rz);
                if a >= 2.0 {
                    return (-1e6, -1e6, f32::MAX);
                }
                ((ew * 0.5 - c) * eppm, ih as f32 - (y - base + 2.0) * eppm, -a)
            };
            // Everything the track places there, and the venue's own pieces alone.
            let venue_models: Vec<(Mesh, Texture)> = v.kinds.iter().map(|k| (k.1.clone(), k.2.clone())).collect();
            for (suffix, models) in [("_wall.png", &sc.models), ("_wall_alone.png", &venue_models)] {
                let mut e = image::RgbImage::from_pixel(iw, ih, image::Rgb([150, 185, 215]));
                let mut zb = vec![f32::MAX; (iw * ih) as usize];
                for (m, t) in models {
                    let (lo, hi) = m.bounds();
                    if !wl.foot.covers((lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5, 80.0) {
                        continue;
                    }
                    let tex = (t.width > 0).then(|| (t.width, t.height, t.rgba.as_slice()));
                    pic::raster(&mut e, &mut zb, m, tex, [255, 0, 255], &proj);
                }
                let wall_path = path.replace(".png", suffix);
                e.save(&wall_path).unwrap();
                println!("WALL picture {wall_path}");
            }
            // Everything the track stands in the wall's footprint, and how far from the row.
            for (m, t) in &sc.models {
                let near: Vec<f32> = m
                    .positions
                    .chunks_exact(3)
                    .filter(|q| wl.foot.covers(q[0], q[2], 1.0))
                    .map(|q| (q[0] - g.x) * fx + (q[2] - g.z) * fz)
                    .collect();
                if !near.is_empty() {
                    let (lo, hi) = near.iter().fold((f32::MAX, f32::MIN), |a, &x| (a.0.min(x), a.1.max(x)));
                    println!("  in the wall's footprint: {:24} {:6} of {:6} verts, {lo:6.2}..{hi:6.2} m from the row", t.name, near.len(), m.vertex_count());
                }
            }
            // Which part stands nearest the gates: the print should.
            for k in v.kinds.iter().filter(|k| k.0.starts_with("sponsor_wall")) {
                let n = k.1.vertex_count().max(1) as f32;
                let a = k.1.positions.chunks_exact(3).map(|q| (q[0] - g.x) * fx + (q[2] - g.z) * fz).sum::<f32>() / n;
                let nf = k.1.normals.chunks_exact(3).map(|q| q[0] * fx + q[2] * fz).sum::<f32>() / n;
                println!("  {:20} mean {a:6.2} m from the gate row, normal toward the gates {nf:5.2}", k.0);
            }
        }
        println!("tally {:?}", sc.tally.iter().filter(|(k, _)| k.starts_with("paddock") || k.starts_with("sponsor") || k.starts_with("venue") || k.starts_with("pit")).collect::<Vec<_>>());
        println!("paddock {:.0} x {:.0} m", pad.area.hx * 2.0, pad.area.hz * 2.0);
        for sp in &pad.spots {
            println!("  {:10} rig at ({:.0}, {:.0})", sp.brand, sp.rig.c.0, sp.rig.c.1);
        }
    }
}

#[cfg(test)]
mod probe {
    use super::*;

    fn sheet_of<'a>(lib: &'a crate::trackprops::PropLibrary, name: &str) -> Option<(u32, u32, &'a [u8])> {
        lib.sheets.iter().find(|s| s.0 == name).map(|s| (s.1, s.2, s.3.as_slice()))
    }

    /// Lift each donor's sponsor wall and draw it as seen from its gate row.
    ///
    /// ```text
    /// FROST_DONORS=a.pkz,b.pkz FROST_OUT=dir cargo test --bins -- --ignored --nocapture lift_the_walls
    /// ```
    #[test]
    #[ignore = "reads donor tracks — set FROST_DONORS and FROST_OUT"]
    fn lift_the_walls() {
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        std::fs::create_dir_all(&out).unwrap();
        for path in std::env::var("FROST_DONORS").expect("set FROST_DONORS").split(',').filter(|p| !p.is_empty()) {
            let path = std::path::PathBuf::from(path);
            let d = crate::trackprops::open(&path).expect("opens");
            let grid = donor_grid(&path).expect("a grid");
            let props = lift_wall(&d, grid);
            let mut lib = crate::trackprops::PropLibrary { donor: d.stem.clone(), donor_lap_m: 1.0, props, instances: vec![], runs: vec![], sheets: vec![] };
            crate::trackprops::sheets_for(&d, &mut lib, 2048);
            println!("\n{}: {} wall parts", d.stem, lib.props.len());
            for p in &lib.props {
                let (lo, hi) = p.mesh.bounds();
                let t = sheet_of(&lib, &p.sheet).map(|t| (t.0, t.1));
                println!("  {:20} {:22} {:5} tris  {:5.1} x {:5.1} m, {:4.1} m tall, y {:5.2}..{:5.2}  sheet {:?}", p.id, p.sheet, p.mesh.triangle_count(), hi[0] - lo[0], hi[2] - lo[2], p.height, lo[1], hi[1], t);
            }
            // From the gate row, looking back at it: across flipped, 16 px a metre.
            let (fx, fz) = crate::trackprog::heading_vector(grid.2.to_radians());
            let (rx, rz) = crate::trackprog::right_vector(grid.2.to_radians());
            let ppm = 16.0;
            let (w, h) = (60 * 16u32, 9 * 16u32);
            for (label, dir) in [("front", 1.0f32), ("rear", -1.0)] {
                let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([120, 160, 200]));
                let mut zb = vec![f32::MAX; (w * h) as usize];
                for p in &lib.props {
                    let proj = |x: f32, y: f32, z: f32| {
                        let (a, c) = (x * fx + z * fz, x * rx + z * rz);
                        ((30.0 - c * dir) * ppm, h as f32 - (y + 0.5) * ppm, a * dir)
                    };
                    pic::raster(&mut img, &mut zb, &p.mesh, sheet_of(&lib, &p.sheet), [255, 0, 255], &proj);
                }
                img.save(out.join(format!("wall_{}_{label}.png", d.stem))).unwrap();
            }
        }
    }

    /// Rigs and tents in the library, side and top, numbered, to pick the team trucks by eye.
    #[test]
    #[ignore = "reads a library — set FROST_PROPS and FROST_OUT"]
    fn rig_contact_sheet() {
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        let lib = crate::trackprops::load().expect("a library");
        let whole = |p: &crate::trackprops::Prop| p.reach <= p.span * std::f32::consts::FRAC_1_SQRT_2 + 0.5;
        let mut picks: Vec<(usize, Mesh)> = lib
            .props
            .iter()
            .enumerate()
            .filter(|(_, p)| whole(p))
            .filter_map(|(i, p)| {
                let a = aligned(&p.mesh);
                let (lo, hi) = a.bounds();
                let (l, wd) = (hi[0] - lo[0], hi[2] - lo[2]);
                let rig = p.class == crate::trackobjects::Class::Vehicle && (5.5..=21.0).contains(&l) && (1.8..=5.5).contains(&wd) && (2.4..=5.6).contains(&p.height);
                let tent = ["tent_sides", "easy_ups", "big_tent"].iter().any(|s| p.sheet.starts_with(s)) && (2.5..=10.0).contains(&l);
                (rig || tent).then_some((i, a))
            })
            .collect();
        picks.sort_by(|a, b| lib.props[a.0].sheet.cmp(&lib.props[b.0].sheet).then((b.1.bounds().1[0]).total_cmp(&a.1.bounds().1[0])));
        let (cw, ch, ppm, cols) = (320u32, 200u32, 13.0f32, 6u32);
        let rows = (picks.len() as u32).div_ceil(cols);
        let mut img = image::RgbImage::from_pixel(cw * cols, ch * rows, image::Rgb([235, 235, 235]));
        let mut zb = vec![f32::MAX; (cw * cols * ch * rows) as usize];
        for (n, (i, m)) in picks.iter().enumerate() {
            let p = &lib.props[*i];
            let (ox, oy) = ((n as u32 % cols * cw) as f32, (n as u32 / cols * ch) as f32);
            let side = |x: f32, y: f32, z: f32| (ox + 160.0 + x * ppm, oy + 100.0 - y * ppm, z);
            let top = |x: f32, y: f32, z: f32| (ox + 160.0 + x * ppm, oy + 150.0 + z * ppm, -y);
            pic::raster(&mut img, &mut zb, m, sheet_of(&lib, &p.sheet), [255, 0, 255], &side);
            pic::raster(&mut img, &mut zb, m, sheet_of(&lib, &p.sheet), [255, 0, 255], &top);
            let (lo, hi) = m.bounds();
            pic::label(&mut img, &format!("{n} {} {:.1}x{:.1}x{:.1}", p.id, hi[0] - lo[0], hi[2] - lo[2], p.height), 15.0, ox + 4.0, oy + 16.0, [0, 0, 0]);
            println!("{n:3} {:28} {:5.1} x {:4.1} x {:4.1} tris {}", p.id, hi[0] - lo[0], hi[2] - lo[2], p.height, p.mesh.triangle_count());
        }
        img.save(out.join("rigs.png")).unwrap();
    }

    /// Islands near a donor's gate row, drawn from above and from the gates looking back, and
    /// the big flat clusters among them listed.
    ///
    /// ```text
    /// FROST_DONORS=a.pkz,b.pkz FROST_OUT=dir cargo test --bins -- --ignored --nocapture find_sponsor_walls
    /// ```
    #[test]
    #[ignore = "reads donor tracks — set FROST_DONORS and FROST_OUT"]
    fn find_sponsor_walls() {
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        std::fs::create_dir_all(&out).unwrap();
        for path in std::env::var("FROST_DONORS").expect("set FROST_DONORS").split(',').filter(|p| !p.is_empty()) {
            let path = std::path::PathBuf::from(path);
            let d = crate::trackprops::open(&path).expect("opens");
            let Some((gx, gz, ang)) = donor_grid(&path) else {
                println!("{}: no starting_grid", d.stem);
                continue;
            };
            let (fx, fz) = crate::trackprog::heading_vector(ang.to_radians());
            let (rx, rz) = crate::trackprog::right_vector(ang.to_radians());
            let frame = |x: f32, z: f32| ((x - gx) * fx + (z - gz) * fz, (x - gx) * rx + (z - gz) * rz);
            let base = d.ground.as_ref().map_or(0.0, |g| g.at(gx, gz));
            let sheet_of = |m: u32| d.sheets.get(m as usize).map(|s| s.0.to_ascii_lowercase()).unwrap_or_default();
            let m = &d.mesh;
            const R: f32 = 90.0;
            // Islands whose centre is in the box.
            let near: Vec<usize> = (0..m.objects.len())
                .filter(|&i| {
                    let o = &m.objects[i];
                    let (a, c) = frame((o.min[0] + o.max[0]) * 0.5, (o.min[2] + o.max[2]) * 0.5);
                    a.abs() < R && c.abs() < R && (o.max[0] - o.min[0]).max(o.max[2] - o.min[2]) < 120.0
                })
                .collect();
            // Same-sheet islands whose boxes touch are one cluster.
            let mut up: Vec<usize> = (0..near.len()).collect();
            fn root(up: &mut [usize], mut i: usize) -> usize {
                while up[i] != i {
                    up[i] = up[up[i]];
                    i = up[i];
                }
                i
            }
            for a in 0..near.len() {
                for b in a + 1..near.len() {
                    let (p, q) = (&m.objects[near[a]], &m.objects[near[b]]);
                    if p.material != q.material {
                        continue;
                    }
                    let gap = (0..3).map(|k| (p.min[k] - q.max[k]).max(q.min[k] - p.max[k]).max(0.0)).fold(0.0f32, f32::max);
                    if gap < 1.0 {
                        let (ra, rb) = (root(&mut up, a), root(&mut up, b));
                        if ra != rb {
                            up[rb] = ra;
                        }
                    }
                }
            }
            let mut groups: std::collections::HashMap<usize, Vec<usize>> = Default::default();
            for k in 0..near.len() {
                let r = root(&mut up, k);
                groups.entry(r).or_default().push(near[k]);
            }
            let mut rows = Vec::new();
            for isl in groups.values() {
                let (mut a0, mut a1, mut c0, mut c1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for &i in isl {
                    let o = &m.objects[i];
                    for (x, z) in [(o.min[0], o.min[2]), (o.max[0], o.min[2]), (o.min[0], o.max[2]), (o.max[0], o.max[2])] {
                        let (a, c) = frame(x, z);
                        (a0, a1, c0, c1) = (a0.min(a), a1.max(a), c0.min(c), c1.max(c));
                    }
                    (y0, y1) = (y0.min(o.min[1]), y1.max(o.max[1]));
                }
                let wide = (a1 - a0).max(c1 - c0);
                let thin = (a1 - a0).min(c1 - c0);
                if wide >= 10.0 && y1 - y0 >= 2.5 && thin < wide * 0.35 {
                    rows.push((wide, thin, y1 - y0, (a0 + a1) * 0.5, (c0 + c1) * 0.5, y0 - base, sheet_of(m.objects[isl[0]].material), isl.len()));
                }
            }
            rows.sort_by(|a, b| b.0.total_cmp(&a.0));
            println!("\n{}: grid ({gx:.1}, {gz:.1}) angle {ang:.1}, ground {base:.1}", d.stem);
            for r in rows.iter().take(40) {
                println!(
                    "  {:5.1} m wide {:4.1} deep {:4.1} tall  along {:6.1} across {:6.1} foot {:5.1}  {:28} {} islands",
                    r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7
                );
            }

            // Pictures: from above, and from the gate row looking back.
            let tex = crate::map::textures(&d.map_bytes, 512);
            let tex_of = |mat: u32| tex.iter().find(|t| t.material == mat);
            let sample = |mat: u32, u: f32, v: f32| -> [u8; 3] {
                match tex_of(mat) {
                    Some(t) if t.width > 0 => {
                        let x = (((u - u.floor()) * t.width as f32) as u32).min(t.width - 1);
                        let y = (((v - v.floor()) * t.height as f32) as u32).min(t.height - 1);
                        let i = ((y * t.width + x) * 4) as usize;
                        [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2]]
                    }
                    _ => [255, 0, 255],
                }
            };
            let tris: Vec<usize> = {
                let set: std::collections::HashSet<u32> = near.iter().map(|&i| i as u32).collect();
                (0..m.object_of_tri.len()).filter(|&t| set.contains(&m.object_of_tri[t])).collect()
            };
            let vert = |i: u32| {
                let i = i as usize;
                let (a, c) = frame(m.positions[i * 3], m.positions[i * 3 + 2]);
                (a, c, m.positions[i * 3 + 1] - base, m.uvs[i * 2], m.uvs[i * 2 + 1])
            };
            for (label, ppm, w, h) in [("plan", 4.0f32, (R * 2.0 * 4.0) as u32, (R * 2.0 * 4.0) as u32), ("back", 8.0, (R * 2.0 * 8.0) as u32, 40 * 8), ("ahead", 8.0, (R * 2.0 * 8.0) as u32, 40 * 8)] {
                let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([90, 120, 80]));
                let mut zb = vec![f32::MAX; (w * h) as usize];
                for &t in &tris {
                    let mat = m.objects[m.object_of_tri[t] as usize].material;
                    let v: Vec<_> = (0..3).map(|k| vert(m.indices[t * 3 + k])).collect();
                    // Screen x, screen y, depth (smaller is nearer).
                    let p: Vec<(f32, f32, f32)> = v
                        .iter()
                        .map(|&(a, c, y, _, _)| match label {
                            "plan" => ((c + R) * ppm, (R - a) * ppm, -y),
                            // Looking back from the row: -along ahead of the eye, across flipped.
                            "back" => ((R - c) * ppm, h as f32 - (y + 8.0) * ppm, if a < 0.0 { -a } else { f32::MAX }),
                            _ => ((c + R) * ppm, h as f32 - (y + 8.0) * ppm, if a > 0.0 { a } else { f32::MAX }),
                        })
                        .collect();
                    if p.iter().any(|q| q.2 == f32::MAX) {
                        continue;
                    }
                    let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
                    if area.abs() < 1e-6 {
                        continue;
                    }
                    let (x0, x1) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).max(0.0), p.iter().map(|q| q.0).fold(f32::MIN, f32::max).min(w as f32 - 1.0));
                    let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).max(0.0), p.iter().map(|q| q.1).fold(f32::MIN, f32::max).min(h as f32 - 1.0));
                    if x0 > x1 || y0 > y1 {
                        continue;
                    }
                    for py in y0 as u32..=y1 as u32 {
                        for px in x0 as u32..=x1 as u32 {
                            let (sx, sy) = (px as f32 + 0.5, py as f32 + 0.5);
                            let e = |a: (f32, f32, f32), b: (f32, f32, f32)| (b.0 - a.0) * (sy - a.1) - (sx - a.0) * (b.1 - a.1);
                            let (l0, l1, l2) = (e(p[1], p[2]) / area, e(p[2], p[0]) / area, e(p[0], p[1]) / area);
                            if l0 < 0.0 || l1 < 0.0 || l2 < 0.0 {
                                continue;
                            }
                            let z = l0 * p[0].2 + l1 * p[1].2 + l2 * p[2].2;
                            let k = (py * w + px) as usize;
                            if z >= zb[k] {
                                continue;
                            }
                            zb[k] = z;
                            let (u, vv) = (l0 * v[0].3 + l1 * v[1].3 + l2 * v[2].3, l0 * v[0].4 + l1 * v[1].4 + l2 * v[2].4);
                            img.put_pixel(px, py, image::Rgb(sample(mat, u, vv)));
                        }
                    }
                }
                if label == "plan" {
                    // The gate row, red, and a stub pointing the way the riders go.
                    for c in -30..=30 {
                        let (px, py) = (((c as f32 + R) * ppm) as u32, (R * ppm) as u32);
                        img.put_pixel(px.min(w - 1), py, image::Rgb([255, 0, 0]));
                    }
                    for a in 0..20 {
                        img.put_pixel((R * ppm) as u32, ((R - a as f32) * ppm) as u32, image::Rgb([255, 0, 0]));
                    }
                }
                img.save(out.join(format!("{}_{label}.png", d.stem))).unwrap();
            }
        }
    }

    /// What the library offers a paddock: vehicles, tents, fences.
    #[test]
    #[ignore = "reads a library — set FROST_PROPS"]
    fn list_paddock_props() {
        let lib = crate::trackprops::load().expect("a library");
        let mut v: Vec<&crate::trackprops::Prop> = lib
            .props
            .iter()
            .filter(|p| {
                p.class == crate::trackobjects::Class::Vehicle
                    || ["tent", "easy_up", "awning", "canopy", "fence", "barrier", "edge_"].iter().any(|w| p.sheet.contains(w) || p.id.contains(w))
            })
            .collect();
        v.sort_by(|a, b| b.span.total_cmp(&a.span));
        for p in v {
            let (lo, hi) = p.mesh.bounds();
            let n = lib.instances.iter().filter(|i| std::ptr::eq(&lib.props[i.prop], p)).count();
            println!(
                "{:34} {:26} {:9} h {:5.2} span {:5.2} reach {:5.2} box {:5.1}x{:5.1} tris {:6} inst {n}",
                p.id, p.sheet, p.class.key(), p.height, p.span, p.reach, hi[0] - lo[0], hi[2] - lo[2], p.mesh.triangle_count()
            );
        }
        for (n, w, h, _) in &lib.sheets {
            println!("sheet {n} {w}x{h}");
        }
    }

    /// Add a donor's sponsor wall to a baked library and write it out as a new one.
    ///
    /// ```text
    /// FROST_BASE=library11.fpl FROST_TRACK=indiana.pkz FROST_BAKE=library12.fpl \
    ///   cargo test --bins -- --ignored --nocapture bake_the_wall_in
    /// ```
    #[test]
    #[ignore = "bakes a library — set FROST_BASE, FROST_TRACK and FROST_BAKE"]
    fn bake_the_wall_in() {
        let base = std::fs::read(std::env::var("FROST_BASE").expect("set FROST_BASE")).unwrap();
        let mut lib = crate::trackprops::PropLibrary::decode(&base).expect("the base reads");
        let path = std::path::PathBuf::from(std::env::var("FROST_TRACK").expect("set FROST_TRACK"));
        let d = crate::trackprops::open(&path).expect("opens");
        // The wall's frame wears the library's own copy of its sheet, so it has to be that donor.
        assert_eq!(d.stem, lib.donor, "the wall must come from the library's own donor");
        lib.props.retain(|p| !WALL_PARTS.iter().any(|(id, _)| p.id == *id));
        let wall = lift_wall(&d, donor_grid(&path).expect("a grid"));
        assert!(!wall.is_empty(), "no sponsor wall behind {}'s gate row", d.stem);
        for p in &wall {
            println!("  {}: {:.1} m across, {:.1} m tall, {} tris on {}", p.id, p.span, p.height, p.mesh.triangle_count(), p.sheet);
        }
        lib.props.extend(wall);
        crate::trackprops::sheets_for(&d, &mut lib, 2048);
        let out = std::env::var("FROST_BAKE").expect("set FROST_BAKE");
        let bytes = lib.encode();
        std::fs::write(&out, &bytes).unwrap();
        let back = crate::trackprops::PropLibrary::decode(&std::fs::read(&out).unwrap()).expect("reads back");
        assert_eq!(back.props.len(), lib.props.len());
        assert!(WALL_PARTS.iter().all(|(id, sheet)| !back.props.iter().any(|p| p.id == *id) || back.sheets.iter().any(|s| s.0 == *sheet)));
        println!("{out}: {} props, {} sheets, {:.1} MB", back.props.len(), back.sheets.len(), bytes.len() as f32 / 1_048_576.0);
    }
}
