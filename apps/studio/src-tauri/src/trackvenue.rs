//! The venue beyond the lap: a fenced paddock with a rig per team, the road to the pits,
//! and the sponsor wall behind the gate row. A supercross lap gets [`stadium`] instead — a
//! wall round its floor and stands rising behind it, which is the venue that racing has.

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

/// Islands of a donor's map as one mesh, in a frame centred on `(cx, cz)` with `foot` at y 0,
/// donor world axes.
pub fn lift_islands(m: &crate::map::MapMesh, isls: &[usize], (cx, foot, cz): (f32, f32, f32)) -> Mesh {
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
    mesh
}

/// Islands of one sheet grouped where their boxes come within `gap` metres: a cab coupled to
/// its trailer touches it, and two rigs parked side by side stand apart.
pub fn touching(d: &crate::trackprops::Donor, sheet: &str, gap: f32) -> Vec<Vec<usize>> {
    let m = &d.mesh;
    let of = |i: usize| d.sheets.get(m.objects[i].material as usize).map(|s| s.0.to_ascii_lowercase()).unwrap_or_default();
    let isl: Vec<usize> = (0..m.objects.len()).filter(|&i| of(i) == sheet).collect();
    let mut up: Vec<usize> = (0..isl.len()).collect();
    fn root(up: &mut [usize], mut i: usize) -> usize {
        while up[i] != i {
            up[i] = up[up[i]];
            i = up[i];
        }
        i
    }
    for a in 0..isl.len() {
        for b in a + 1..isl.len() {
            let (p, q) = (&m.objects[isl[a]], &m.objects[isl[b]]);
            let g = (0..3).map(|k| (p.min[k] - q.max[k]).max(q.min[k] - p.max[k]).max(0.0)).fold(0.0f32, f32::max);
            if g <= gap {
                let (ra, rb) = (root(&mut up, a), root(&mut up, b));
                if ra != rb {
                    up[rb] = ra;
                }
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for k in 0..isl.len() {
        let r = root(&mut up, k);
        groups.entry(r).or_default().push(isl[k]);
    }
    groups.into_values().collect()
}

/// Pop-up roof colours, and which one each team in [`BRANDS`] puts up.
pub const POPUP_COLOURS: [(&str, [f32; 3]); 7] = [
    ("orange", [230.0, 110.0, 20.0]),
    ("red", [200.0, 30.0, 30.0]),
    ("blue", [25.0, 70.0, 190.0]),
    ("green", [40.0, 150.0, 45.0]),
    ("white", [230.0, 230.0, 230.0]),
    ("black", [25.0, 25.0, 28.0]),
    ("yellow", [240.0, 200.0, 20.0]),
];
pub const BRAND_POPUP: [&str; 8] = ["orange", "red", "blue", "green", "white", "red", "yellow", "black"];

fn prop_of(id: String, sheet: &str, class: crate::trackobjects::Class, mesh: Mesh) -> crate::trackprops::Prop {
    let (lo, hi) = mesh.bounds();
    let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
    crate::trackprops::Prop { id, sheet: sheet.into(), class, height: hi[1] - lo[1].min(0.0), span: (hi[0] - lo[0]).max(hi[2] - lo[2]), reach, axis_ref: 0.0, mesh }
}

/// A donor's whole team rigs: cab, trailer and the team's awning, touching pieces of
/// `semi_trailers_c`. Long along x, trailer to −z, awning to +z, centred, foot at y 0.
pub fn lift_team_rigs(d: &crate::trackprops::Donor) -> Vec<crate::trackprops::Prop> {
    let m = &d.mesh;
    let mut out: Vec<crate::trackprops::Prop> = Vec::new();
    for g in touching(d, "semi_trailers_c", 0.5) {
        let (lo, hi) = group_box(m, &g);
        let mut a = aligned(&lift_islands(m, &g, ((lo[0] + hi[0]) * 0.5, lo[1], (lo[2] + hi[2]) * 0.5)));
        let (b0, b1) = a.bounds();
        let (l, w, h) = (b1[0] - b0[0], b1[2] - b0[2], b1[1] - b0[1]);
        if !(25.0..=27.5).contains(&l) || !(12.5..=15.5).contains(&w) || !(4.2..=6.0).contains(&h) {
            continue;
        }
        // The trailer's wheels and body are most of what stands low; the awning is legs.
        let low: Vec<f32> = a.positions.chunks_exact(3).filter(|v| v[1] < 1.2).map(|v| v[2]).collect();
        if low.iter().sum::<f32>() / low.len().max(1) as f32 > 0.0 {
            a = edfwrite::turned(&a, 180.0);
        }
        // One model in many liveries: the livery is in the UVs.
        let key = |m: &Mesh| (m.triangle_count(), (m.uvs.iter().sum::<f32>() * 10.0).round() as i64);
        if out.iter().any(|p| key(&p.mesh) == key(&a)) {
            continue;
        }
        out.push(prop_of(format!("team_rig_{:02}", out.len()), "semi_trailers_c", crate::trackobjects::Class::Vehicle, a));
    }
    out
}

/// A donor's pop-ups, one a roof colour: the `easy_ups` roof and the frame of legs under it,
/// as two props sharing one frame, centred, foot at y 0.
pub fn lift_popups(d: &crate::trackprops::Donor, tex: &[crate::map::MapTexture]) -> Vec<crate::trackprops::Prop> {
    let m = &d.mesh;
    let sheet_of = |i: usize| d.sheets.get(m.objects[i].material as usize).map(|s| s.0.to_ascii_lowercase()).unwrap_or_default();
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for g in touching(d, "easy_ups_roof_c", 0.3) {
        let (lo, hi) = group_box(m, &g);
        let (dx, dz) = (hi[0] - lo[0], hi[2] - lo[2]);
        if !(3.0..=4.7).contains(&dx) || (dx - dz).abs() > 0.4 {
            continue;
        }
        let frame: Vec<usize> = (0..m.objects.len())
            .filter(|&i| {
                let o = &m.objects[i];
                sheet_of(i) == "main_track_objects_c" && o.min[0] >= lo[0] - 0.4 && o.max[0] <= hi[0] + 0.4 && o.min[2] >= lo[2] - 0.4 && o.max[2] <= hi[2] + 0.4 && o.max[1] <= hi[1] + 0.2 && o.min[1] >= lo[1] - 4.0
            })
            .collect();
        if frame.len() < 20 {
            continue;
        }
        let foot = frame.iter().map(|&i| m.objects[i].min[1]).fold(f32::INFINITY, f32::min);
        if !(2.8..=4.0).contains(&(hi[1] - foot)) {
            continue;
        }
        // The roof's colour, read through its own UVs.
        let Some(t) = tex.iter().find(|t| t.material == m.objects[g[0]].material) else { continue };
        let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
        for &isl in &g {
            for tri in island_tris(m, isl) {
                let (mut u, mut v) = (0.0, 0.0);
                for k in 0..3 {
                    let vi = m.indices[tri * 3 + k] as usize;
                    u += m.uvs[vi * 2] / 3.0;
                    v += m.uvs[vi * 2 + 1] / 3.0;
                }
                let x = (((u - u.floor()) * t.width as f32) as u32).min(t.width - 1);
                let y = (((v - v.floor()) * t.height as f32) as u32).min(t.height - 1);
                let i = ((y * t.width + x) * 4) as usize;
                for k in 0..3 {
                    sum[k] += t.rgba[i + k] as f32;
                }
                n += 1.0;
            }
        }
        let c = sum.map(|s| s / n.max(1.0));
        let (name, _) = POPUP_COLOURS
            .iter()
            .min_by(|a, b| {
                let e = |p: &[f32; 3]| (0..3).map(|k| (p[k] - c[k]).powi(2)).sum::<f32>();
                e(&a.1).total_cmp(&e(&b.1))
            })
            .unwrap();
        if seen.contains(name) {
            continue;
        }
        seen.push(name);
        let at = ((lo[0] + hi[0]) * 0.5, foot, (lo[2] + hi[2]) * 0.5);
        // Square to the axes by its roof, both parts by the same turn.
        let roof = lift_islands(m, &g, at);
        let area = |m: &Mesh| {
            let (a, b) = m.bounds();
            (b[0] - a[0]) * (b[2] - a[2])
        };
        let deg = (0..180).map(|d| d as f32 * 0.5).min_by(|a, b| area(&edfwrite::turned(&roof, *a)).total_cmp(&area(&edfwrite::turned(&roof, *b)))).unwrap_or(0.0);
        out.push(prop_of(format!("team_popup_{name}_roof"), "easy_ups_roof_c", crate::trackobjects::Class::Structure, edfwrite::turned(&roof, deg)));
        out.push(prop_of(format!("team_popup_{name}_frame"), "main_track_objects_c", crate::trackobjects::Class::Structure, edfwrite::turned(&lift_islands(m, &frame, at), deg)));
    }
    out
}

/// A group's box, over its islands.
pub fn group_box(m: &crate::map::MapMesh, g: &[usize]) -> ([f32; 3], [f32; 3]) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for &i in g {
        for k in 0..3 {
            lo[k] = lo[k].min(m.objects[i].min[k]);
            hi[k] = hi[k].max(m.objects[i].max[k]);
        }
    }
    (lo, hi)
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
const PAD_MARGIN_M: f32 = 2.0;
const SPOT_GAP_M: f32 = 5.0;
const AISLE_M: f32 = 10.0;
const GATE_M: f32 = 14.0;
/// How far every part of the paddock keeps from any leg of the lap, past the half-width.
const PAD_CLEAR_M: f32 = 8.0;
/// The most the ground may fall across the paddock.
const PAD_FALL_M: f32 = 3.0;
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
    /// Metres behind the gate row.
    pub back: f32,
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
            || self.road.windows(2).any(|s| seg_dist(s[0], s[1], (x, z)) <= ROAD_W_M * 0.5 + ROAD_FADE_M + 1.0 + m)
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

/// The paddock: four bays a side of its aisle, the gate in the middle of the near side. Fixed,
/// not sized to the library's models, so the `.rdf` and the scenery agree on every bay.
const BAY_W_M: f32 = 30.0;
const RIG_DEPTH_M: f32 = 15.0;
const FRONT_M: f32 = 6.0;
/// Five bikes a bay: forty, a full gate.
const SPAWNS_PER_BAY: usize = 5;
const SPAWN_GAP_M: f32 = 2.5;
/// How far the road's middle keeps from any leg of the lap past its half-width: its own half,
/// its soft edge and room to spare, so its paint never reaches the riding surface.
const ROAD_CLEAR_M: f32 = 7.0;
const ROAD_FADE_M: f32 = 2.5;
/// The road's route is found on a grid this fine.
const ROUTE_CELL_M: f32 = 2.0;
/// How far off the start pad the road runs until it meets the pad beside the gate row.
const ROAD_OFF_PAD_M: f32 = 4.0;
/// The room the wall is given behind the gate row, across and deep: it stands inside this.
const WALL_ENV_M: (f32, f32) = (52.0, 8.0);
/// The paddock floor is levelled, fading into the field over this; the road's bed over this.
const LEVEL_FADE_M: f32 = 5.0;
const BED_FADE_M: f32 = 3.0;
const PAINT_CELL_M: f32 = 0.25;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash01(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (z as u32).wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Smooth value noise, 0..1.
fn vnoise(x: f32, z: f32, seed: u32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (tx, tz) = (smooth(x - x0), smooth(z - z0));
    let (i, j) = (x0 as i32, z0 as i32);
    let a = hash01(i, j, seed) + (hash01(i + 1, j, seed) - hash01(i, j, seed)) * tx;
    let b = hash01(i, j + 1, seed) + (hash01(i + 1, j + 1, seed) - hash01(i, j + 1, seed)) * tx;
    a + (b - a) * tz
}

fn pad_half() -> (f32, f32) {
    ((4.0 * BAY_W_M + GATE_M + 2.0 * PAD_MARGIN_M) * 0.5, AISLE_M * 0.5 + FRONT_M + RIG_DEPTH_M + PAD_MARGIN_M)
}

/// Bay `i`'s middle along the paddock, and its row: +1 the far side, −1 the gate's.
fn bay(i: usize) -> (f32, f32) {
    let (hx, _) = pad_half();
    let k = i % 4;
    let x = if k < 2 { -hx + PAD_MARGIN_M + BAY_W_M * (k as f32 + 0.5) } else { hx - PAD_MARGIN_M - BAY_W_M * ((3 - k) as f32 + 0.5) };
    (x, if i < 4 { 1.0 } else { -1.0 })
}

/// Where the game spawns a rider: world position and heading (compass, radians), and the same
/// as the `.rdf` states it — metres round the lap, signed lateral, degrees off the lap's heading.
#[derive(Clone, Copy, Debug)]
pub struct Spawn {
    pub x: f32,
    pub z: f32,
    pub heading: f32,
    pub long: f32,
    pub lat: f32,
    pub angle: f32,
}

/// The venue's layout, from the lap alone: the same for the `.rdf`, the ground and the scenery.
pub struct Plan {
    pub paddock: Option<Rect>,
    /// From the aisle's middle, out through the gate, to the start pad beside the gate row.
    pub road: Vec<(f32, f32)>,
    pub aisle: Option<((f32, f32), (f32, f32))>,
    pub wall: Option<Wall>,
    pub spawns: Vec<Spawn>,
}

/// A world point as the `.rdf` states one: round the lap from its nearest station, signed
/// lateral, and a heading as degrees off the lap's there.
fn on_lap(st: &[crate::trackprog::Station], lap: f32, x: f32, z: f32, heading: f32) -> (f32, f32, f32) {
    // Against a station the spot stands square to, and of those the one where the lap is
    // straightest: a spot a hundred metres out swings two metres for every degree the lap turns
    // between one station and the next, so a `long` read a station off on a bend lands wrong.
    // Stated at the station itself, not projected past it, and clear of the lap's seam.
    // The point on the centreline square to the spot, between stations: a hundred metres off a
    // bend, the lines square to two neighbouring stations land metres apart, so snapping to one
    // puts the spot metres wrong. Of the legs near the nearest, the straightest wins: there a
    // `long` read a little off still lands where it should.
    let near = |q: &crate::trackprog::Station| (q.x - x).hypot(q.z - z);
    let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let at = |i: usize, t: f32| {
        let (a, b) = (&st[i], &st[i + 1]);
        (a.x + (b.x - a.x) * t, a.z + (b.z - a.z) * t, a.heading + wrap(b.heading - a.heading) * t, a.s + (b.s - a.s) * t)
    };
    let along = |i: usize, t: f32| {
        let (px, pz, h, _) = at(i, t);
        let (fx, fz) = crate::trackprog::heading_vector(h);
        (x - px) * fx + (z - pz) * fz
    };
    // Only as far out as the lap's nearest leg, give or take: past that it is another leg, and a
    // lateral hundreds of metres long is further than any published track states.
    let d0 = st.iter().map(near).fold(f32::INFINITY, f32::min);
    let mut best: Option<(f32, f32, usize, f32)> = None;
    for i in 0..st.len().saturating_sub(1) {
        let (a, b) = (&st[i], &st[i + 1]);
        if b.s - a.s < 0.05 || near(a) > d0 + 20.0 || along(i, 0.0) * along(i, 1.0) > 0.0 {
            continue;
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..30 {
            let m = (lo + hi) * 0.5;
            if along(i, lo) * along(i, m) <= 0.0 {
                hi = m;
            } else {
                lo = m;
            }
        }
        let key = (wrap(b.heading - a.heading).abs(), near(a));
        if best.is_none_or(|bb| key.partial_cmp(&(bb.0, bb.1)) == Some(std::cmp::Ordering::Less)) {
            best = Some((key.0, key.1, i, (lo + hi) * 0.5));
        }
    }
    let (px, pz, h, s) = match best {
        Some((_, _, i, t)) => at(i, t),
        None => {
            let q = st.iter().min_by(|a, b| near(a).total_cmp(&near(b))).unwrap();
            (q.x, q.z, q.heading, q.s)
        }
    };
    let (rx, rz) = crate::trackprog::right_vector(h);
    (s.rem_euclid(lap), (x - px) * rx + (z - pz) * rz, (heading.to_degrees() - h.to_degrees()).rem_euclid(360.0))
}

/// The old stalls beside the lap, for a track the paddock cannot reach.
fn lane_spawns(prog: &TrackProgram) -> Vec<Spawn> {
    let (lap, st) = (prog.lap_length(), prog.stations(0.5));
    lane(prog)
        .stalls
        .iter()
        .map(|&(long, lat)| {
            let q = st[((long.rem_euclid(lap) / 0.5) as usize).min(st.len() - 1)];
            let (rx, rz) = crate::trackprog::right_vector(q.heading);
            Spawn { x: q.x + rx * lat, z: q.z + rz * lat, heading: q.heading, long, lat, angle: 0.0 }
        })
        .collect()
}

/// Five bikes a bay, side by side in front of the team's awning, facing the aisle.
fn paddock_spawns(prog: &TrackProgram, area: &Rect) -> Vec<Spawn> {
    let (lap, st) = (prog.lap_length(), prog.stations(0.5));
    let mut out = Vec::new();
    for i in 0..BRANDS.len() {
        let (bx, row) = bay(i);
        let dir = (-row * area.z.0, -row * area.z.1);
        let heading = dir.0.atan2(dir.1);
        for j in 0..SPAWNS_PER_BAY {
            let (x, z) = area.at(bx - 12.0 + j as f32 * SPAWN_GAP_M, row * (AISLE_M * 0.5 + FRONT_M * 0.5));
            let (long, lat, angle) = on_lap(&st, lap, x, z, heading);
            out.push(Spawn { x, z, heading, long, lat, angle });
        }
    }
    out
}

/// The gate row: its middle station on the start straight, and the pad's half-width there.
fn gate_row(syn: &Synth) -> Option<(crate::trackprog::Station, f32)> {
    let spur = syn.spur.as_ref()?;
    let g = spur.stations[((spur.gate_at() / 0.5) as usize).min(spur.stations.len().saturating_sub(1))];
    Some((g, spur.at(spur.gate_at())))
}

/// The sponsor wall's room behind the gate row: the first clear of the pad, the lap and the plot.
fn wall_site(prog: &TrackProgram, syn: &Synth) -> Option<Wall> {
    let (g, _) = gate_row(syn)?;
    let half = prog.width * 0.5;
    let f = crate::trackprog::heading_vector(g.heading);
    let r = crate::trackprog::right_vector(g.heading);
    let mut back = WALL_BACK_M.0;
    while back <= WALL_BACK_M.1 {
        let foot = Rect { c: (g.x - f.0 * back, g.z - f.1 * back), x: r, z: f, hx: WALL_ENV_M.0 * 0.5, hz: WALL_ENV_M.1 * 0.5 };
        let ok = (0..=WALL_ENV_M.0 as i32).all(|i| {
            [-1.0f32, 0.0, 1.0].iter().all(|&j| {
                let (x, z) = foot.at(-foot.hx + i as f32, j * foot.hz);
                on_plot(prog, x, z, 2.0) && dist(syn, x, z) > half + 3.0 && syn.outside_the_start(x, z).is_none_or(|e| e > WALL_OFF_PAD_M)
            })
        });
        if ok {
            return Some(Wall { foot, faces: g.heading, lifted: false, back });
        }
        back += 0.5;
    }
    None
}

/// Where the road meets the start: beside each end of the gate row, a little behind it, and the
/// point on the pad's edge it runs on to.
fn gate_goals(syn: &Synth) -> Vec<((f32, f32), (f32, f32))> {
    let Some((g, ph)) = gate_row(syn) else { return Vec::new() };
    let f = crate::trackprog::heading_vector(g.heading);
    let r = crate::trackprog::right_vector(g.heading);
    [-1.0f32, 1.0]
        .iter()
        .map(|&s| {
            let at = |out: f32| (g.x + r.0 * s * out - f.0 * 2.0, g.z + r.1 * s * out - f.1 * 2.0);
            (at(ph + ROAD_OFF_PAD_M + 1.0), at(ph + 0.5))
        })
        .collect()
}

/// Where a road may run: on the plot, clear of the lap by [`ROAD_CLEAR_M`], off the pad, and
/// not under the wall.
fn road_ok(prog: &TrackProgram, syn: &Synth, wall: Option<&Wall>, x: f32, z: f32) -> bool {
    on_plot(prog, x, z, 3.0)
        && dist(syn, x, z) > prog.width * 0.5 + ROAD_CLEAR_M
        && syn.outside_the_start(x, z).is_none_or(|e| e > ROAD_OFF_PAD_M)
        && wall.is_none_or(|w| !w.foot.covers(x, z, 3.0))
}

/// Every cell's road distance to the gate row, and the way there.
struct Route {
    nx: usize,
    nz: usize,
    cost: Vec<f32>,
    next: Vec<u32>,
    goal: Vec<u8>,
}

impl Route {
    fn cell(&self, x: f32, z: f32) -> Option<usize> {
        let (i, j) = ((x / ROUTE_CELL_M).floor(), (z / ROUTE_CELL_M).floor());
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.nx && (j as usize) < self.nz).then(|| j as usize * self.nx + i as usize)
    }
    fn centre(&self, k: usize) -> (f32, f32) {
        (((k % self.nx) as f32 + 0.5) * ROUTE_CELL_M, ((k / self.nx) as f32 + 0.5) * ROUTE_CELL_M)
    }
    fn cost_at(&self, x: f32, z: f32) -> Option<f32> {
        self.cell(x, z).map(|k| self.cost[k]).filter(|c| c.is_finite())
    }
    /// The cells from `(x, z)` to the gate row, and which goal it reaches.
    fn path(&self, x: f32, z: f32) -> Option<(Vec<(f32, f32)>, usize)> {
        let mut k = self.cell(x, z)?;
        if !self.cost[k].is_finite() {
            return None;
        }
        let mut out = vec![(x, z)];
        while self.next[k] != u32::MAX {
            k = self.next[k] as usize;
            out.push(self.centre(k));
        }
        Some((out, self.goal[k] as usize))
    }
}

fn route(prog: &TrackProgram, syn: &Synth, wall: Option<&Wall>, goals: &[(f32, f32)]) -> Route {
    use std::cmp::Reverse;
    let (nx, nz) = ((prog.terrain.size_x / ROUTE_CELL_M).ceil() as usize, (prog.terrain.size_z / ROUTE_CELL_M).ceil() as usize);
    let mut r = Route { nx, nz, cost: vec![f32::INFINITY; nx * nz], next: vec![u32::MAX; nx * nz], goal: vec![0; nx * nz] };
    let open: Vec<bool> = (0..nx * nz)
        .map(|k| {
            let (x, z) = r.centre(k);
            road_ok(prog, syn, wall, x, z)
        })
        .collect();
    let mut heap = std::collections::BinaryHeap::new();
    for (gi, g) in goals.iter().enumerate() {
        for k in 0..nx * nz {
            let c = r.centre(k);
            if open[k] && (c.0 - g.0).hypot(c.1 - g.1) <= 3.0 {
                r.cost[k] = 0.0;
                r.goal[k] = gi as u8;
                heap.push((Reverse(0u64), k));
            }
        }
    }
    while let Some((Reverse(c), k)) = heap.pop() {
        let c = c as f32 / 1000.0;
        if c > r.cost[k] + 1e-3 {
            continue;
        }
        let (i, j) = ((k % nx) as isize, (k / nx) as isize);
        for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (a, b) = (i + di, j + dj);
            if a < 0 || b < 0 || a >= nx as isize || b >= nz as isize {
                continue;
            }
            let n = b as usize * nx + a as usize;
            if !open[n] {
                continue;
            }
            let step = if di != 0 && dj != 0 { std::f32::consts::SQRT_2 } else { 1.0 } * ROUTE_CELL_M;
            if c + step < r.cost[n] {
                r.cost[n] = c + step;
                r.next[n] = k as u32;
                r.goal[n] = r.goal[k];
                heap.push((Reverse(((c + step) * 1000.0) as u64), n));
            }
        }
    }
    r
}

/// A path cut down to the fewest straight runs that stay where a road may go.
fn pull(prog: &TrackProgram, syn: &Synth, wall: Option<&Wall>, pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let clear = |a: (f32, f32), b: (f32, f32)| {
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / 0.5).ceil().max(1.0) as usize;
        (0..=n).all(|k| {
            let t = k as f32 / n as f32;
            road_ok(prog, syn, wall, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        })
    };
    let mut out = vec![pts[0]];
    let mut i = 0;
    while i + 1 < pts.len() {
        let j = (i + 1..pts.len()).rev().find(|&j| clear(pts[i], pts[j])).unwrap_or(i + 1);
        out.push(pts[j]);
        i = j;
    }
    out
}

/// The natural ground on a coarse grid: how much a paddock site falls is judged on this, never
/// on the finished terrain, which `grade` levels.
struct Land {
    nx: usize,
    nz: usize,
    h: Vec<f32>,
}

impl Land {
    const STEP: f32 = 4.0;
    fn of(prog: &TrackProgram) -> Land {
        let l = crate::tracksynth::Landscape::of(prog);
        let (nx, nz) = ((prog.terrain.size_x / Self::STEP).ceil() as usize + 1, (prog.terrain.size_z / Self::STEP).ceil() as usize + 1);
        let h = (0..nx * nz).map(|k| l.at((k % nx) as f32 * Self::STEP, (k / nx) as f32 * Self::STEP)).collect();
        Land { nx, nz, h }
    }
    fn at(&self, x: f32, z: f32) -> f32 {
        let (gx, gz) = ((x / Self::STEP).clamp(0.0, (self.nx - 1) as f32), (z / Self::STEP).clamp(0.0, (self.nz - 1) as f32));
        let (i, j) = ((gx as usize).min(self.nx - 2), (gz as usize).min(self.nz - 2));
        let (tx, tz) = (gx - i as f32, gz - j as f32);
        let p = |a: usize, b: usize| self.h[b * self.nx + a];
        let a = p(i, j) + (p(i + 1, j) - p(i, j)) * tx;
        let b = p(i, j + 1) + (p(i + 1, j + 1) - p(i, j + 1)) * tx;
        a + (b - a) * tz
    }
}

/// Where the paddock goes: anywhere on the plot clear of the lap, the start and the wall, on
/// ground that falls little, with the shortest road from its gate to the gate row that never
/// crosses the lap. Returns the site, that road from just outside its gate, and which goal.
fn site(prog: &TrackProgram, syn: &Synth, wall: Option<&Wall>, route: &Route) -> Option<(Rect, Vec<(f32, f32)>, usize)> {
    let half = prog.width * 0.5;
    let (hx, hz) = pad_half();
    let land = Land::of(prog);
    let base = gate_row(syn).map_or(0.0, |(g, _)| g.heading);
    let mut cands: Vec<(f32, Rect)> = Vec::new();
    for b in [base, 0.0] {
        for k in 0..4 {
            let psi = b + k as f32 * std::f32::consts::FRAC_PI_2;
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
                    let gp = area.at(0.0, -hz - 2.0);
                    let Some(cost) = route.cost_at(gp.0, gp.1) else { continue };
                    let (mut lo, mut hi, mut ok) = (f32::INFINITY, f32::NEG_INFINITY, true);
                    'g: for i in 0..=((hx + 2.0) as i32) {
                        for j in 0..=((hz + 2.0) as i32) {
                            let (x, z) = area.at(-hx - 2.0 + i as f32 * 2.0, -hz - 2.0 + j as f32 * 2.0);
                            if !on_plot(prog, x, z, 2.0)
                                || dist(syn, x, z) <= half + PAD_CLEAR_M
                                || syn.outside_the_start(x, z).is_some_and(|e| e <= 6.0)
                                || wall.is_some_and(|w| w.foot.covers(x, z, 4.0))
                            {
                                ok = false;
                                break 'g;
                            }
                            let y = land.at(x, z);
                            (lo, hi) = (lo.min(y), hi.max(y));
                        }
                    }
                    if ok && hi - lo <= PAD_FALL_M {
                        cands.push((cost + 3.0 * (hi - lo), area));
                    }
                }
                cz += 4.0;
            }
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, area) in cands.into_iter().take(60) {
        let gp = area.at(0.0, -hz - 2.0);
        let Some((pts, goal)) = route.path(gp.0, gp.1) else { continue };
        // Out through the gate and away, never back through the paddock.
        if pts.iter().skip(1).any(|p| area.covers(p.0, p.1, 1.0)) {
            continue;
        }
        return Some((area, pull(prog, syn, wall, &pts), goal));
    }
    None
}

/// The venue's layout for a track.
pub fn plan(prog: &TrackProgram, syn: &Synth) -> Plan {
    let wall = wall_site(prog, syn);
    let goals = gate_goals(syn);
    let mut p = Plan { paddock: None, road: Vec::new(), aisle: None, wall, spawns: Vec::new() };
    if !goals.is_empty() {
        let pts: Vec<(f32, f32)> = goals.iter().map(|g| g.0).collect();
        let r = route(prog, syn, wall.as_ref(), &pts);
        if let Some((area, path, goal)) = site(prog, syn, wall.as_ref(), &r) {
            let (hx, hz) = pad_half();
            let mut road = vec![area.at(0.0, 0.0), area.at(0.0, -hz)];
            road.extend(path);
            // On to the pad's edge, unless that brings the paint near the lap.
            let end = goals[goal].1;
            if dist(syn, end.0, end.1) > prog.width * 0.5 + ROAD_W_M * 0.5 + ROAD_FADE_M + 1.0 {
                road.push(end);
            }
            p.aisle = Some((area.at(-hx + PAD_MARGIN_M, 0.0), area.at(hx - PAD_MARGIN_M, 0.0)));
            p.spawns = paddock_spawns(prog, &area);
            p.paddock = Some(area);
            p.road = road;
        }
    }
    if p.spawns.is_empty() {
        p.spawns = lane_spawns(prog);
    }
    p
}

/// Where the `.rdf` spawns riders, and where the stands go: the paddock's bays, or the old
/// stalls beside the lap on a track the paddock cannot reach.
pub fn spawns(prog: &TrackProgram, syn: &Synth) -> Vec<Spawn> {
    plan(prog, syn).spawns
}

/// The road and the paddock floor as paint, on a fine grid over where they are.
pub struct Paint {
    x0: f32,
    z0: f32,
    nx: usize,
    nz: usize,
    /// Coverage: the road, its two packed wheel tracks, the loose soil at its edges, the floor.
    road: Vec<f32>,
    lines: Vec<f32>,
    loose: Vec<f32>,
    floor: Vec<f32>,
}

impl Paint {
    pub fn at(&self, x: f32, z: f32) -> (f32, f32, f32, f32) {
        let (i, j) = (((x - self.x0) / PAINT_CELL_M).floor(), ((z - self.z0) / PAINT_CELL_M).floor());
        if i < 0.0 || j < 0.0 || i as usize >= self.nx || j as usize >= self.nz {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let k = j as usize * self.nx + i as usize;
        (self.road[k], self.lines[k], self.loose[k], self.floor[k])
    }
    /// Every cell the road paints, and how much.
    pub fn road_cells(&self) -> impl Iterator<Item = (f32, f32, f32)> + '_ {
        self.road.iter().enumerate().filter(|(_, r)| **r > 0.0).map(|(k, r)| {
            (self.x0 + ((k % self.nx) as f32 + 0.5) * PAINT_CELL_M, self.z0 + ((k / self.nx) as f32 + 0.5) * PAINT_CELL_M, *r)
        })
    }
}

/// Paint the plan's road and paddock floor: the track's own light soil with a torn, soft edge,
/// two packed wheel tracks down it, loose soil thrown to its sides, worked ground in the paddock.
pub fn paint(plan: &Plan) -> Option<Paint> {
    let area = plan.paddock?;
    let mut runs: Vec<((f32, f32), (f32, f32))> = plan.road.windows(2).map(|w| (w[0], w[1])).collect();
    runs.extend(plan.aisle);
    let reach = ROAD_W_M * 0.5 + ROAD_FADE_M + 1.0;
    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    let mut grow = |x: f32, z: f32, m: f32| {
        lo = (lo.0.min(x - m), lo.1.min(z - m));
        hi = (hi.0.max(x + m), hi.1.max(z + m));
    };
    for &(a, b) in &runs {
        grow(a.0, a.1, reach);
        grow(b.0, b.1, reach);
    }
    for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let (x, z) = area.at(a * area.hx, b * area.hz);
        grow(x, z, 3.0);
    }
    let (nx, nz) = (((hi.0 - lo.0) / PAINT_CELL_M).ceil() as usize + 1, ((hi.1 - lo.1) / PAINT_CELL_M).ceil() as usize + 1);
    let mut p = Paint { x0: lo.0, z0: lo.1, nx, nz, road: vec![0.0; nx * nz], lines: vec![0.0; nx * nz], loose: vec![0.0; nx * nz], floor: vec![0.0; nx * nz] };
    let centre = |i: usize, j: usize| (lo.0 + (i as f32 + 0.5) * PAINT_CELL_M, lo.1 + (j as f32 + 0.5) * PAINT_CELL_M);
    for &(a, b) in &runs {
        let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
        let (ux, uz) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let i0 = ((a.0.min(b.0) - reach - lo.0) / PAINT_CELL_M).max(0.0) as usize;
        let i1 = (((a.0.max(b.0) + reach - lo.0) / PAINT_CELL_M) as usize).min(nx - 1);
        let j0 = ((a.1.min(b.1) - reach - lo.1) / PAINT_CELL_M).max(0.0) as usize;
        let j1 = (((a.1.max(b.1) + reach - lo.1) / PAINT_CELL_M) as usize).min(nz - 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (x, z) = centre(i, j);
                let d = seg_dist(a, b, (x, z));
                if d > reach {
                    continue;
                }
                let lat = ux * (z - a.1) - uz * (x - a.0);
                // Torn, not drawn: the edge wanders a metre and more.
                let core = ROAD_W_M * 0.5 + (vnoise(x / 5.0, z / 5.0, 0x70AD) - 0.5) * 1.4 + (vnoise(x / 1.5, z / 1.5, 0x70AE) - 0.5) * 0.5;
                let k = j * nx + i;
                p.road[k] = p.road[k].max(smooth((core + ROAD_FADE_M - d) / ROAD_FADE_M));
                let inside = smooth((core - d) / 1.0);
                let line = (-((lat.abs() - 0.95) / 0.3).powi(2)).exp() * inside * (0.65 + 0.35 * vnoise(x / 2.0, z / 2.0, 0x71AF));
                p.lines[k] = p.lines[k].max(line);
                let edge = (((d - (core - 1.5)) / 2.5).clamp(0.0, 1.0) * std::f32::consts::PI).sin();
                p.loose[k] = p.loose[k].max(edge * vnoise(x / 3.0, z / 3.0, 0x72B0));
            }
        }
    }
    for j in 0..nz {
        for i in 0..nx {
            let (x, z) = centre(i, j);
            let (a, b) = area.local(x, z);
            let e = (a.abs() - area.hx).max(0.0).hypot((b.abs() - area.hz).max(0.0));
            if e < 2.0 {
                p.floor[j * nx + i] = smooth(1.0 - e / 2.0) * (0.5 + 0.5 * vnoise(x / 4.0, z / 4.0, 0x73B1));
            }
        }
    }
    Some(p)
}

/// Lay the plan's paint into the ground masks: the riding soil over the road and, patchily,
/// the paddock; grass off both; the wheel tracks into the packed band; loose soil at the sides.
pub fn paint_ground(plan: &Plan, syn: &Synth, dirt: &mut [u8], ddim: usize, grass: &mut [u8], rut: &mut [u8], loose: &mut [u8], dim: usize) {
    let Some(p) = paint(plan) else { return };
    // A mask texel's ground, as `tracksynth`'s masks read it.
    let world = |x: usize, y: usize, n: usize| (((x * syn.gw / n).min(syn.gw - 1)) as f32 * syn.mps, ((y * syn.gh / n).min(syn.gh - 1)) as f32 * syn.mps);
    for y in 0..ddim {
        for x in 0..ddim {
            let (wx, wz) = world(x, y, ddim);
            let (r, _, _, f) = p.at(wx, wz);
            let v = (r.max(f * 0.6) * 255.0) as u8;
            let o = &mut dirt[y * ddim + x];
            *o = (*o).max(v);
        }
    }
    for y in 0..dim {
        for x in 0..dim {
            let (wx, wz) = world(x, y, dim);
            let (r, l, lo, f) = p.at(wx, wz);
            if r + l + lo + f <= 0.0 {
                continue;
            }
            let k = y * dim + x;
            grass[k] = (grass[k] as f32 * (1.0 - r.max(f * 0.85))) as u8;
            rut[k] = rut[k].max((l * 0.8 * 255.0) as u8);
            loose[k] = loose[k].max((lo * 0.75 * 255.0) as u8);
        }
    }
}

/// Level the paddock floor and grade the road's bed into the field round them, on the finished
/// terrain before anything stands on it. Never on the lap or the pad; the plan reads no heights,
/// so it comes out the same after this as before.
pub fn grade(prog: &TrackProgram, syn: &mut Synth) {
    let plan = plan(prog, syn);
    let Some(area) = plan.paddock else { return };
    let half = prog.width * 0.5;
    let (gw, gh, mps) = (syn.gw, syn.gh, syn.mps);
    let src = syn.heights.clone();
    let at = |x: f32, z: f32| src[((z / mps).round().clamp(0.0, (gh - 1) as f32) as usize) * gw + (x / mps).round().clamp(0.0, (gw - 1) as f32) as usize];
    let keep = |x: f32, z: f32| smooth((dist(syn, x, z) - half - 3.0) / 2.0) * syn.outside_the_start(x, z).map_or(1.0, |e| smooth(e / 2.0));
    let mut out = src.clone();
    // Every cell within `r` of a box, by index.
    let cells = |lo: (f32, f32), hi: (f32, f32)| {
        let (i0, i1) = (((lo.0 / mps).floor().max(0.0)) as usize, ((hi.0 / mps).ceil() as usize).min(gw - 1));
        let (j0, j1) = (((lo.1 / mps).floor().max(0.0)) as usize, ((hi.1 / mps).ceil() as usize).min(gh - 1));
        (i0, i1, j0, j1)
    };
    // The floor, level at its own mean.
    let (mut sum, mut n) = (0.0f32, 0.0f32);
    for i in 0..=(area.hx * 2.0) as i32 {
        for j in 0..=(area.hz * 2.0) as i32 {
            let (x, z) = area.at(-area.hx + i as f32, -area.hz + j as f32);
            sum += at(x, z);
            n += 1.0;
        }
    }
    let level = sum / n.max(1.0);
    let reach = area.hx.hypot(area.hz) + LEVEL_FADE_M;
    let (i0, i1, j0, j1) = cells((area.c.0 - reach, area.c.1 - reach), (area.c.0 + reach, area.c.1 + reach));
    for j in j0..=j1 {
        for i in i0..=i1 {
            let (x, z) = (i as f32 * mps, j as f32 * mps);
            let (a, b) = area.local(x, z);
            let e = (a.abs() - area.hx).max(0.0).hypot((b.abs() - area.hz).max(0.0));
            if e >= LEVEL_FADE_M {
                continue;
            }
            let w = smooth(1.0 - e / LEVEL_FADE_M) * keep(x, z);
            let k = j * gw + i;
            out[k] = src[k] + (level - src[k]) * w;
        }
    }
    // The road's bed: its own ground averaged along it, from the gate out.
    let mut pts: Vec<(f32, f32)> = Vec::new();
    for w in plan.road[1..].windows(2) {
        let len = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        let n = len.ceil().max(1.0) as usize;
        for k in 0..n {
            let t = k as f32 / n as f32;
            pts.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
        }
    }
    if let Some(l) = plan.road.last() {
        pts.push(*l);
    }
    if pts.len() < 2 {
        syn.heights = out;
        return;
    }
    let raw: Vec<f32> = pts
        .iter()
        .map(|&(x, z)| {
            let mut s = 0.0;
            for (dx, dz) in [(0.0, 0.0), (2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)] {
                s += out[((z + dz) / mps).round().clamp(0.0, (gh - 1) as f32) as usize * gw + ((x + dx) / mps).round().clamp(0.0, (gw - 1) as f32) as usize];
            }
            s / 5.0
        })
        .collect();
    let bed: Vec<f32> = (0..raw.len())
        .map(|k| {
            let (a, b) = (k.saturating_sub(8), (k + 8).min(raw.len() - 1));
            raw[a..=b].iter().sum::<f32>() / (b - a + 1) as f32
        })
        .collect();
    let mut weight = vec![0.0f32; 0];
    let mut target = vec![0.0f32; 0];
    let reach = ROAD_W_M * 0.5 + BED_FADE_M;
    let mut touched: std::collections::HashMap<usize, (f32, f32)> = Default::default();
    for s in 0..pts.len() - 1 {
        let (a, b) = (pts[s], pts[s + 1]);
        let (i0, i1, j0, j1) = cells((a.0.min(b.0) - reach, a.1.min(b.1) - reach), (a.0.max(b.0) + reach, a.1.max(b.1) + reach));
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (x, z) = (i as f32 * mps, j as f32 * mps);
                if area.covers(x, z, 0.0) {
                    continue;
                }
                let d = seg_dist(a, b, (x, z));
                if d > reach {
                    continue;
                }
                let w = smooth(1.0 - (d - ROAD_W_M * 0.5) / BED_FADE_M) * keep(x, z);
                let e = touched.entry(j * gw + i).or_insert((0.0, 0.0));
                if w > e.0 {
                    *e = (w, (bed[s] + bed[s + 1]) * 0.5);
                }
            }
        }
    }
    let _ = (&mut weight, &mut target);
    for (k, (w, t)) in touched {
        let d = ((t - out[k]) * 0.85 * w).clamp(-1.0, 1.0);
        out[k] += d;
    }
    syn.heights = out;
}

/// A rig lifted off the donor's slope keeps the slope: its wheels touch a tilted plane, and
/// stood on our level pad one end floated. Sheared level on the plane its lowest points make.
fn level(m: &Mesh) -> Mesh {
    let mut low: std::collections::HashMap<(i32, i32), [f32; 3]> = Default::default();
    for v in m.positions.chunks_exact(3) {
        let e = low.entry((v[0].floor() as i32, v[2].floor() as i32)).or_insert([v[0], v[1], v[2]]);
        if v[1] < e[1] {
            *e = [v[0], v[1], v[2]];
        }
    }
    // y = a x + b z + c through `pts`, least squares.
    let fit = |pts: &[[f32; 3]]| -> Option<(f32, f32, f32)> {
        let (mut s, mut n) = ([0.0f64; 9], 0.0f64);
        for p in pts {
            let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
            s = [s[0] + x * x, s[1] + x * z, s[2] + x, s[3] + z * z, s[4] + z, s[5] + x * y, s[6] + z * y, s[7] + y, 0.0];
            n += 1.0;
        }
        let m = [[s[0], s[1], s[2]], [s[1], s[3], s[4]], [s[2], s[4], n]];
        let r = [s[5], s[6], s[7]];
        let det = |m: [[f64; 3]; 3]| {
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        };
        let d = det(m);
        if d.abs() < 1e-9 {
            return None;
        }
        let col = |k: usize| {
            let mut c = m;
            for i in 0..3 {
                c[i][k] = r[i];
            }
            det(c) / d
        };
        Some((col(0) as f32, col(1) as f32, col(2) as f32))
    };
    let mut pts: Vec<[f32; 3]> = low.into_values().collect();
    let Some(mut plane) = fit(&pts) else { return m.clone() };
    // Down onto the lower envelope: keep what lies at or under the plane, and fit again.
    for _ in 0..8 {
        let under: Vec<[f32; 3]> = pts.iter().copied().filter(|p| p[1] <= plane.0 * p[0] + plane.1 * p[2] + plane.2 + 0.02).collect();
        if under.len() < 8 || under.len() == pts.len() {
            break;
        }
        pts = under;
        match fit(&pts) {
            Some(f) => plane = f,
            None => break,
        }
    }
    // More than a few degrees is not a slope it was parked on.
    if plane.0.hypot(plane.1) > 0.08 {
        return m.clone();
    }
    let mut out = m.clone();
    for v in out.positions.chunks_exact_mut(3) {
        v[1] -= plane.0 * v[0] + plane.1 * v[2];
    }
    out
}

/// A group of rigid parts turned by `deg` and stood together at `(x, z)` on the lowest ground
/// under all of them.
fn stand_parts(parts: &[&Mesh], x: f32, z: f32, deg: f32, syn: &Synth) -> Vec<Mesh> {
    let t: Vec<Mesh> = parts.iter().map(|m| edfwrite::turned(m, deg)).collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for m in &t {
        let (a, b) = m.bounds();
        for k in 0..3 {
            lo[k] = lo[k].min(a[k]);
            hi[k] = hi[k].max(b[k]);
        }
    }
    let mut foot = f32::INFINITY;
    for (a, b) in [(lo[0], lo[2]), (hi[0], lo[2]), (lo[0], hi[2]), (hi[0], hi[2]), ((lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5)] {
        foot = foot.min(ground(syn, x + a, z + b));
    }
    t.iter().map(|m| edfwrite::moved(m, [x, foot - lo[1] - 0.05, z])).collect()
}

/// The paddock's scenery: in each bay a whole team rig backing onto the fence, the team's pop-up
/// in its colour and its board at the aisle; the fence round it all.
fn dress_paddock(prog: &TrackProgram, syn: &Synth, lib: Option<&crate::trackprops::PropLibrary>, plan: &Plan, v: &mut Venue) {
    use crate::trackobjects::Class;
    let _ = prog;
    let Some(area) = plan.paddock else {
        v.tally.push(("paddock", 0));
        return;
    };
    let (hx, hz, f, o) = (area.hx, area.hz, area.x, area.z);
    let lib_sheet = |sheet: &str| lib.and_then(|l| lib_tex(l, sheet));
    // Whole team rigs; else a loose semi; else our own boxes.
    let team: Vec<&crate::trackprops::Prop> = lib
        .map(|l| l.props.iter().filter(|p| p.id.starts_with("team_rig_") && lib_tex(l, &p.sheet).is_some()).collect())
        .unwrap_or_default();
    let semis: Vec<(&crate::trackprops::Prop, Mesh)> = match (lib, team.is_empty()) {
        (Some(l), true) => l
            .props
            .iter()
            .filter(|p| p.class == Class::Vehicle && whole(p) && (3.8..=5.4).contains(&p.height) && lib_tex(l, &p.sheet).is_some())
            .filter_map(|p| {
                let a = aligned(&p.mesh);
                let (lo, hi) = a.bounds();
                ((17.0..=21.0).contains(&(hi[0] - lo[0])) && (2.0..=4.5).contains(&(hi[2] - lo[2]))).then_some((p, a))
            })
            .collect(),
        _ => Vec::new(),
    };
    let popup = |c: &str| -> Option<(&crate::trackprops::Prop, &crate::trackprops::Prop)> {
        let l = lib?;
        let r = l.props.iter().find(|p| p.id == format!("team_popup_{c}_roof"))?;
        let fr = l.props.iter().find(|p| p.id == format!("team_popup_{c}_frame"))?;
        (lib_tex(l, &r.sheet).is_some() && lib_tex(l, &fr.sheet).is_some()).then_some((r, fr))
    };
    let (mut rigs, mut trucks, mut roofs, mut frames, mut canopies, mut boards) =
        (Mesh::default(), Mesh::default(), Mesh::default(), Mesh::default(), Mesh::default(), Mesh::default());
    let (mut rig_sheet, mut roof_sheet, mut frame_sheet) = (None, None, None);
    let mut spots = Vec::new();
    for (i, (brand, ..)) in BRANDS.iter().enumerate() {
        let (bx, row) = bay(i);
        let to_aisle = (-row * o.0, -row * o.1);
        let deg = deg_facing(to_aisle);
        let (len, dep, (rx, rz)) = if !team.is_empty() {
            let p = team[i * team.len() / BRANDS.len()];
            let (lo, hi) = p.mesh.bounds();
            let at = area.at(bx, row * (hz - PAD_MARGIN_M - RIG_DEPTH_M * 0.5));
            rigs.append(&stand(&level(&p.mesh), at.0, at.1, deg, syn));
            rig_sheet = Some(p.sheet.clone());
            (hi[0] - lo[0], hi[2] - lo[2], at)
        } else if !semis.is_empty() {
            let (p, m) = &semis[i * semis.len() / BRANDS.len()];
            let (lo, hi) = m.bounds();
            let at = area.at(bx, row * (hz - PAD_MARGIN_M - (hi[2] - lo[2]) * 0.5 - 0.5));
            rigs.append(&stand(&level(m), at.0, at.1, deg, syn));
            rig_sheet = Some(p.sheet.clone());
            (hi[0] - lo[0], hi[2] - lo[2], at)
        } else {
            let at = area.at(bx, row * (hz - PAD_MARGIN_M - 2.0));
            trucks.append(&stand(&own_rig(i), at.0, at.1, deg, syn));
            (15.0, 2.55, at)
        };
        let (px, pz) = area.at(bx + 6.5, row * (AISLE_M * 0.5 + FRONT_M * 0.5));
        match popup(BRAND_POPUP[i]) {
            Some((r, fr)) => {
                let parts = stand_parts(&[&r.mesh, &fr.mesh], px, pz, deg, syn);
                roofs.append(&parts[0]);
                frames.append(&parts[1]);
                roof_sheet = Some(r.sheet.clone());
                frame_sheet = Some(fr.sheet.clone());
            }
            None => canopies.append(&stand(&canopy_mesh(i), px, pz, deg, syn)),
        }
        let (bxw, bzw) = area.at(bx + 12.0, row * (AISLE_M * 0.5 + 0.8));
        boards.append(&stand(&board_mesh(i), bxw, bzw, deg_facing(to_aisle), syn));
        spots.push(Spot { brand, rig: Rect { c: (rx, rz), x: f, z: o, hx: len * 0.5, hz: dep * 0.5 }, board: (bxw, bzw), tent: Some((px, pz)) });
    }

    // The fence: panel after panel round the edge, a gap for the gate in the near side.
    let barrier = lib.and_then(|l| l.props.iter().find(|p| p.id == "edge_barrier").filter(|p| lib_tex(l, &p.sheet).is_some()));
    let post = lib.and_then(|l| l.props.iter().find(|p| p.id == "edge_post").filter(|p| lib_tex(l, &p.sheet).is_some()));
    let step = barrier.map_or(3.0, |p| p.span.clamp(1.5, 4.0));
    let (mut fence, mut posts, mut centres) = (Mesh::default(), Mesh::default(), Vec::new());
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

    let n_popups = if roofs.vertex_count() > 0 || canopies.vertex_count() > 0 { BRANDS.len() } else { 0 };
    if let Some(t) = rig_sheet.and_then(|s| lib_sheet(&s)) {
        v.kinds.push(("paddock_rigs".into(), rigs, t, true));
    }
    if trucks.vertex_count() > 0 {
        v.kinds.push(("paddock_trucks".into(), trucks, brand_sheet(), true));
    }
    if let Some(t) = roof_sheet.and_then(|s| lib_sheet(&s)) {
        v.kinds.push(("paddock_popup_roofs".into(), roofs, t, true));
    }
    if let Some(t) = frame_sheet.and_then(|s| lib_sheet(&s)) {
        v.kinds.push(("paddock_popup_frames".into(), frames, t, true));
    }
    if canopies.vertex_count() > 0 {
        v.kinds.push(("paddock_canopies".into(), canopies, brand_sheet(), true));
    }
    v.kinds.push(("paddock_boards".into(), boards, brand_sheet(), true));
    match barrier.and_then(|p| lib_sheet(&p.sheet)) {
        Some(t) => v.kinds.push(("paddock_fence".into(), fence, t, true)),
        None => v.kinds.push(("paddock_fence".into(), fence, net_sheet(), true)),
    }
    if let Some(t) = post.and_then(|p| lib_sheet(&p.sheet)) {
        v.kinds.push(("paddock_posts".into(), posts, t, true));
    }
    let road_len: f32 = plan.road.windows(2).map(|s| (s[1].0 - s[0].0).hypot(s[1].1 - s[0].1)).sum();
    v.tally.extend([
        ("paddock", 1),
        ("paddock rigs", spots.len()),
        ("paddock popups", n_popups),
        ("paddock boards", BRANDS.len()),
        ("paddock fence panels", centres.len()),
        ("paddock spawns", plan.spawns.len()),
        ("paddock road m", road_len.round() as usize),
    ]);
    v.paddock = Some(Paddock { area, spots, fence: centres });
}

/// The sponsor wall in its room behind the gate row, facing the row: the library's lifted one
/// when it has it, our own printed board when it does not.
fn dress_wall(syn: &Synth, lib: Option<&crate::trackprops::PropLibrary>, plan: &Plan, v: &mut Venue) {
    let Some(w) = plan.wall else {
        v.tally.push(("sponsor wall", 0));
        return;
    };
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
                let m = edfwrite::turned(&p.mesh, (w.faces - p.axis_ref).to_degrees());
                (p.id.clone(), if m.vertex_count() < 64 { fine(&fine(&m)) } else { m }, t.clone())
            })
            .collect()
    } else {
        let (ww, h, lift) = OWN_WALL_M;
        let mut m = edfwrite::moved(&into_window(&edfwrite::card(ww, h), (0.0, 0.0, 1.0, 0.8)), [0.0, lift, 0.0]);
        let grey = (0.0, 0.85, 1.0, 0.1);
        let back = edfwrite::turned(&edfwrite::moved(&on_hem(&edfwrite::card(ww, h), grey), [0.0, lift, 0.0]), 180.0);
        m.append(&edfwrite::moved(&back, [0.0, 0.0, -0.05]));
        for k in 0..9 {
            let post = on_hem(&edfwrite::cuboid(0.2, lift + h + 0.2, 0.2), grey);
            m.append(&edfwrite::moved(&post, [(k as f32 / 8.0 - 0.5) * (ww - 0.4), 0.0, -0.2]));
        }
        vec![("sponsor_wall".into(), edfwrite::turned(&m, w.faces.to_degrees()), own_wall_sheet())]
    };
    // On the lowest ground along it: its legs reach into the higher.
    let fall = (0..=20)
        .map(|i| {
            let (x, z) = w.foot.at(-w.foot.hx + i as f32 * w.foot.hx / 10.0, 0.0);
            ground(syn, x, z)
        })
        .fold(f32::INFINITY, f32::min);
    for (name, m, t) in meshes {
        v.kinds.push((name, edfwrite::moved(&m, [w.foot.c.0, fall - 0.1, w.foot.c.1]), t, true));
    }
    v.tally.push(("sponsor wall", 1));
    v.tally.push(("sponsor wall back m", w.back.round() as usize));
    v.wall = Some(Wall { lifted, ..w });
}

/// The venue for a track: the paddock and the sponsor wall. The road is paint (`paint_ground`).
pub fn build(prog: &TrackProgram, syn: &Synth, lib: Option<&crate::trackprops::PropLibrary>) -> Venue {
    let plan = plan(prog, syn);
    let mut v = Venue { kinds: Vec::new(), tally: Vec::new(), paddock: None, road: plan.road.clone(), wall: None };
    dress_paddock(prog, syn, lib, &plan, &mut v);
    dress_wall(syn, lib, &plan, &mut v);
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

/// The stadium a supercross lap is laid inside: the wall round its floor, and its height and
/// thickness. Low enough to see the racing over, high enough to stop a bike.
const SX_WALL_M: (f32, f32) = (1.3, 0.3);
/// How far outside the lap's own extent the wall stands, and how near any leg of the lap
/// anything here may come. The floor is sized off the lap, not fitted to its shape: a stadium
/// is a bowl the track is built in, not a fence that follows it.
const SX_OUT_M: f32 = 12.0;
const SX_CLEAR_M: f32 = 6.0;
/// The stands behind the wall: how many tiers, and how deep and how much higher each is than
/// the one below it.
const SX_TIERS: usize = 6;
const SX_TIER_M: (f32, f32) = (2.5, 1.0);
/// The most boxes a kind here is allowed. A cuboid is 24 vertices, so this holds a mesh under
/// `trackscenery`'s 48,000 split and well under the 65,535 a model may draw at all; the steps
/// are stretched to suit a big plot rather than the count let run.
const SX_MAX_BOXES: usize = 1_800;

/// Boxes laid nose to tail round the rectangle `(x0, z0, x1, z1)`, each `size` (tall, deep) and
/// about `step` long, stood on the ground under it. A piece that would fall off the plot or
/// come within `SX_CLEAR_M` of a leg of the lap is left out: a closed ring is never worth a
/// wall across the riding surface.
fn sx_ring(prog: &TrackProgram, syn: &Synth, r: (f32, f32, f32, f32), step: f32, size: (f32, f32), into: &mut Mesh) -> usize {
    let (x0, z0, x1, z1) = r;
    let corners = [(x0, z0), (x1, z0), (x1, z1), (x0, z1)];
    let mut laid = 0;
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len < step {
            continue;
        }
        let n = (len / step).round().max(1.0) as usize;
        let run = len / n as f32;
        let d = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let deg = deg_along(d);
        for i in 0..n {
            let t = (i as f32 + 0.5) * run;
            let (cx, cz) = (a.0 + d.0 * t, a.1 + d.1 * t);
            // Every corner of the piece, not its middle alone: what the ring promises is that no
            // part of a box it lays comes within `SX_CLEAR_M` of a leg of the lap, and a corner
            // stands half a run along and half a depth across from the centre it is stood on.
            let along = (d.0 * (run + 0.1) * 0.5, d.1 * (run + 0.1) * 0.5);
            let across = (-d.1 * size.1 * 0.5, d.0 * size.1 * 0.5);
            let clear = [-1.0f32, 0.0, 1.0].iter().all(|&e| {
                [-1.0f32, 1.0].iter().all(|&f| {
                    let (x, z) = (cx + along.0 * e + across.0 * f, cz + along.1 * e + across.1 * f);
                    on_plot(prog, x, z, 1.0) && dist(syn, x, z) > prog.width * 0.5 + SX_CLEAR_M
                })
            });
            if !clear {
                continue;
            }
            // A touch longer than its run, so the ring reads as one wall and not as a row.
            into.append(&stand(&edfwrite::cuboid(run + 0.1, size.0, size.1), cx, cz, deg, syn));
            laid += 1;
        }
    }
    laid
}

/// The floor wall's print: a dark board with a bright rail along its top. A cuboid's side face
/// puts v 0 at its top edge, so the rail is drawn at the top of the sheet.
fn stadium_wall_sheet() -> Texture {
    let n = 64u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let c = if v < 0.16 {
                [20, 70, 170]
            } else if v < 0.2 || (u * 4.0).fract() < 0.03 {
                [16, 16, 18]
            } else {
                let k = 0.9 + 0.1 * vnoise(u * 12.0, v * 12.0, 0x5D01);
                [(52.0 * k) as u8, (54.0 * k) as u8, (60.0 * k) as u8]
            };
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Texture { name: "stadium_wall_c".into(), width: n, height: n, rgba: px }
}

/// The stands: poured concrete, a darker line along the nose of each step, which is the top
/// edge of every tier's box and so v 0 again.
fn stadium_stand_sheet() -> Texture {
    let n = 64u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let k = if v < 0.08 { 0.62 } else { 0.9 + 0.12 * vnoise(u * 9.0, v * 9.0, 0x5D02) };
            let c = [(148.0 * k) as u8, (146.0 * k) as u8, (140.0 * k) as u8];
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Texture { name: "stadium_stand_c".into(), width: n, height: n, rgba: px }
}

/// The venue a supercross track gets instead of the paddock and the sponsor wall: the wall round
/// the stadium floor and the tiered stands rising behind it. Built where `trackscenery` would
/// otherwise call [`build`], so a stadium lap is never given a field's paddock.
pub fn stadium(prog: &TrackProgram, syn: &Synth) -> Venue {
    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for q in prog.stations(2.0) {
        lo = (lo.0.min(q.x), lo.1.min(q.z));
        hi = (hi.0.max(q.x), hi.1.max(q.z));
    }
    // The floor: the lap's extent grown by the clearance, and never off the plot. Growing the
    // box is what keeps the wall off the track — every side of it stands `SX_OUT_M` out from the
    // furthest the lap reaches that way.
    let grown = |out: f32| {
        (
            (lo.0 - out).max(4.0),
            (lo.1 - out).max(4.0),
            (hi.0 + out).min(prog.terrain.size_x - 4.0),
            (hi.1 + out).min(prog.terrain.size_z - 4.0),
        )
    };
    let wall_rect = grown(SX_OUT_M);
    let perim = 2.0 * ((wall_rect.2 - wall_rect.0) + (wall_rect.3 - wall_rect.1));
    let mut v = Venue { kinds: Vec::new(), tally: Vec::new(), paddock: None, road: Vec::new(), wall: None };

    // Steps sized off the ring's own length, so a big plot stretches the pieces rather than
    // multiplying them past what one model may hold.
    let mut wall = Mesh::default();
    let step = (perim / SX_MAX_BOXES as f32).max(4.0);
    let pieces = sx_ring(prog, syn, wall_rect, step, SX_WALL_M, &mut wall);

    let (deep, rise) = SX_TIER_M;
    let mut tiers = Mesh::default();
    let mut boxes = 0;
    let step = (perim * SX_TIERS as f32 / SX_MAX_BOXES as f32).max(8.0);
    for k in 0..SX_TIERS {
        // Each tier stepped back from the one below and standing that much higher: seen from
        // the floor it is a staircase, which is all a plain grandstand is.
        let out = SX_OUT_M + SX_WALL_M.1 + deep * (k as f32 + 0.5);
        boxes += sx_ring(prog, syn, grown(out), step, (SX_WALL_M.0 + rise * (k + 1) as f32, deep), &mut tiers);
    }

    debug_assert!(wall.vertex_count() < 65_536 && tiers.vertex_count() < 65_536, "a stadium kind past what a model may draw");
    if pieces > 0 {
        v.kinds.push(("stadium_wall".into(), wall, stadium_wall_sheet(), true));
    }
    if boxes > 0 {
        v.kinds.push(("stadium_tiers".into(), tiers, stadium_stand_sheet(), true));
    }
    v.tally.extend([("stadium wall", pieces), ("stadium tiers", boxes)]);
    v
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

    /// A supercross lap, built the way the app builds one. The stadium is judged on this rather
    /// than on Northgate: an outdoor lap all but fills its own plot, so a floor grown round it
    /// would clamp against the plot's edge and the ring would come out in pieces.
    fn sx_track() -> &'static (TrackProgram, Synth) {
        static S: std::sync::OnceLock<(TrackProgram, Synth)> = std::sync::OnceLock::new();
        S.get_or_init(|| {
            let knobs = crate::tracklayout::LayoutKnobs::for_discipline(crate::trackprog::Discipline::Sx);
            let mut p = (0..40).find_map(|i| crate::tracklayout::draw_with(200 + i, &knobs)).expect("a supercross lap");
            p.terrain.surface = serde_json::from_str("\"soil\"").unwrap();
            let p = crate::tracksynth::with_fitted_budget(&p).unwrap();
            let s = crate::tracksynth::synthesise(&p).unwrap();
            (p, s)
        })
    }

    fn plan_ng() -> &'static Plan {
        static P: std::sync::OnceLock<Plan> = std::sync::OnceLock::new();
        P.get_or_init(|| {
            let (p, s) = northgate();
            plan(p, s)
        })
    }

    fn prop(id: &str, sheet: &str, class: Class, mesh: Mesh, axis_ref: f32) -> Prop {
        let (lo, hi) = mesh.bounds();
        let reach = mesh.positions.chunks_exact(3).map(|v| v[0].hypot(v[2])).fold(0.0f32, f32::max);
        Prop { id: id.into(), sheet: sheet.into(), class, height: hi[1] - lo[1], span: (hi[0] - lo[0]).max(hi[2] - lo[2]), reach, axis_ref, mesh }
    }

    /// A library with what the venue asks for: team rigs, pop-ups, a barrier and post, a wall.
    fn library() -> PropLibrary {
        let wall = edfwrite::moved(&edfwrite::card(50.0, 4.0), [0.0, 1.0, 0.0]);
        let mut props = vec![
            prop("team_rig_00", "semi_trailers_c", Class::Vehicle, edfwrite::cuboid(26.0, 5.2, 13.0), 0.0),
            prop("team_rig_01", "semi_trailers_c", Class::Vehicle, edfwrite::cuboid(26.2, 5.3, 13.2), 0.0),
            prop("edge_barrier", "ck_fence_c_a", Class::Structure, edfwrite::cuboid(0.05, 1.36, 3.0), 0.0),
            prop("edge_post", "main_track_objects_c", Class::Structure, edfwrite::cuboid(0.05, 1.46, 0.05), 0.0),
            prop("sponsor_wall", "start_backdrop_c", Class::Structure, wall, 0.0),
            prop("sponsor_wall_frame", "main_track_objects_c", Class::Structure, edfwrite::moved(&edfwrite::cuboid(50.0, 5.8, 0.4), [0.0, 0.0, -0.5]), 0.0),
        ];
        for (c, _) in POPUP_COLOURS {
            props.push(prop(&format!("team_popup_{c}_roof"), "easy_ups_roof_c", Class::Structure, edfwrite::moved(&edfwrite::cuboid(4.0, 0.4, 4.0), [0.0, 2.7, 0.0]), 0.0));
            let mut legs = Mesh::default();
            for (x, z) in [(-1.9, -1.9), (1.9, -1.9), (1.9, 1.9), (-1.9, 1.9)] {
                legs.append(&edfwrite::moved(&edfwrite::cuboid(0.05, 2.7, 0.05), [x, 0.0, z]));
            }
            props.push(prop(&format!("team_popup_{c}_frame"), "main_track_objects_c", Class::Structure, legs, 0.0));
        }
        let sheets = ["semi_trailers_c", "easy_ups_roof_c", "ck_fence_c_a", "main_track_objects_c", "start_backdrop_c"]
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

    /// The centreline's point and heading at `long` round the lap, between stations, the way
    /// a `long`/`lat` in the `.rdf` is read back.
    fn at_long(st: &[crate::trackprog::Station], long: f32) -> (f32, f32, f32) {
        let k = st.partition_point(|q| q.s <= long).clamp(1, st.len() - 1);
        let (a, b) = (&st[k - 1], &st[k]);
        let t = ((long - a.s) / (b.s - a.s).max(1e-6)).clamp(0.0, 1.0);
        let turn = (b.heading - a.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        (a.x + (b.x - a.x) * t, a.z + (b.z - a.z) * t, a.heading + turn * t)
    }

    /// Every point along a polyline, `step` metres apart.
    fn along(pts: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for w in pts.windows(2) {
            let n = ((w[1].0 - w[0].0).hypot(w[1].1 - w[0].1) / step).ceil().max(1.0) as usize;
            for k in 0..=n {
                let t = k as f32 / n as f32;
                out.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
            }
        }
        out
    }

    /// A written track's files, once.
    fn written() -> &'static std::path::PathBuf {
        static D: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        D.get_or_init(|| {
            let (p, s) = northgate();
            let dir = std::env::temp_dir().join(format!("mxb-venue-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            crate::tracksynth::write_source(p, s, &dir).unwrap();
            dir
        })
    }

    /// A mask's coverage at a world point, read back out of its `.tga`.
    fn mask_at(tga: &[u8], p: &TrackProgram, x: f32, z: f32) -> u8 {
        let w = u16::from_le_bytes([tga[12], tga[13]]) as usize;
        let (px, py) = (((x / p.terrain.size_x) * w as f32) as usize, ((z / p.terrain.size_z) * w as f32) as usize);
        tga[18 + (py.min(w - 1) * w + px.min(w - 1)) * 4 + 3]
    }

    /// The `.rdf` spawns riders in the paddock's bays: every stall, read back the way the game
    /// places one, lands inside the fence on one of the plan's spots, facing the aisle.
    #[test]
    fn the_rdf_spawns_riders_in_the_paddock() {
        let (p, _) = northgate();
        let pl = plan_ng();
        let area = pl.paddock.expect("Northgate gets a paddock");
        let dir = written();
        let rdf = std::fs::read_dir(dir.join(crate::tracksynth::slug(&p.name)))
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| e.path().extension().is_some_and(|x| x == "rdf"))
            .expect("an .rdf");
        let txt = std::fs::read_to_string(rdf.path()).unwrap().replace('\r', "");
        let block = &txt[txt.find("pit_lane").unwrap()..txt.find("pit_board").unwrap()];
        let num = |k: &str| block.lines().filter_map(|l| l.trim().strip_prefix(k).map(|v| v.parse::<f32>().unwrap())).collect::<Vec<_>>();
        let (longs, lats, angles) = (num("long = "), num("lat = "), num("angle = "));
        assert_eq!(longs.len(), BRANDS.len() * SPAWNS_PER_BAY, "one stall a bike");
        assert!(block.contains(&format!("numstalls = {}", longs.len())));
        let st = p.stations(0.5);
        for ((long, lat), angle) in longs.iter().zip(&lats).zip(&angles) {
            // The centreline's point at that distance round, as the game finds it.
            let (qx, qz, qh) = at_long(&st, *long);
            let (rx, rz) = crate::trackprog::right_vector(qh);
            let (x, z) = (qx + rx * lat, qz + rz * lat);
            assert!(area.covers(x, z, -1.0), "a stall at {long:.1}/{lat:.1} lands at ({x:.0}, {z:.0}), outside the paddock");
            let sp = pl.spawns.iter().min_by(|a, b| (a.x - x).hypot(a.z - z).total_cmp(&(b.x - x).hypot(b.z - z))).unwrap();
            assert!((sp.x - x).hypot(sp.z - z) < 0.8, "a stall {:.1} m from its spot", (sp.x - x).hypot(sp.z - z));
            let facing = (angle + qh.to_degrees() - sp.heading.to_degrees()).rem_euclid(360.0);
            assert!(facing.min(360.0 - facing) < 1.0, "a stall faces {facing:.0}° off its spot");
        }
    }

    /// A fence all the way round but the gate, and nothing of the paddock near the track.
    #[test]
    fn the_paddock_is_enclosed_and_clear_of_the_track() {
        let (p, s) = northgate();
        let v = venue();
        let pad = v.paddock.as_ref().expect("Northgate gets a paddock");
        let a = pad.area;
        assert!(a.hx * 2.0 >= 100.0 && a.hz * 2.0 >= 40.0, "{:.0} x {:.0} m is not a big paddock", a.hx * 2.0, a.hz * 2.0);
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
        for pre in ["paddock_rigs", "paddock_popup_roofs", "paddock_popup_frames", "paddock_boards", "paddock_fence", "paddock_posts"] {
            let mut n = 0;
            for q in verts(v, pre) {
                n += 1;
                assert!(dist(s, q[0], q[2]) > half + PAD_CLEAR_M - 1.0, "{pre} {:.1} m from the centreline", dist(s, q[0], q[2]));
                assert!(s.outside_the_start(q[0], q[2]).is_none_or(|e| e > 2.0), "{pre} on the start");
                assert!(a.covers(q[0], q[2], 1.0), "{pre} outside the fence");
            }
            assert!(n > 0, "no {pre}");
        }
    }

    /// Eight teams, each a whole rig backing onto the fence, its pop-up and its board, apart.
    #[test]
    fn a_rig_lifted_off_a_slope_stands_level() {
        let at = |c: Mesh, x: f32, y: f32| {
            let (lo, hi) = c.bounds();
            edfwrite::moved(&c, [x - (lo[0] + hi[0]) * 0.5, y - lo[1], -(lo[2] + hi[2]) * 0.5])
        };
        // A trailer on four wheels, and an awning's thin leg standing off its side.
        let mut rig = at(edfwrite::cuboid(26.0, 3.0, 3.0), 0.0, 1.0);
        for x in [-11.0, -9.5, 9.5, 11.0] {
            rig.append(&at(edfwrite::cuboid(1.0, 1.0, 3.0), x, 0.0));
        }
        rig.append(&at(edfwrite::cuboid(0.1, 3.5, 0.1), 3.0, 0.4));
        // Parked on the donor's 1.7° slope, along its length and a little across.
        rig.positions.chunks_exact_mut(3).for_each(|v| v[1] += 0.03 * v[0] + 0.01 * v[2]);
        let flat = level(&rig);
        let feet: Vec<f32> = flat
            .positions
            .chunks_exact(3)
            .filter(|v| v[0].abs() > 9.0 && v[1] < flat.bounds().0[1] + 0.3)
            .map(|v| v[0])
            .collect();
        assert!(feet.iter().any(|&x| x < -9.0) && feet.iter().any(|&x| x > 9.0), "a wheel end still off the ground: {feet:?}");
        let (lo, hi) = flat.bounds();
        assert!(hi[1] - lo[1] < 4.0 + 0.05, "still tilted: {:.2} m tall", hi[1] - lo[1]);
    }

    #[test]
    fn a_rig_and_a_popup_per_brand() {
        let v = venue();
        let pad = v.paddock.as_ref().expect("a paddock");
        let names: Vec<&str> = pad.spots.iter().map(|s| s.brand).collect();
        assert_eq!(names, BRANDS.iter().map(|b| b.0).collect::<Vec<_>>());
        for (i, a) in pad.spots.iter().enumerate() {
            let (_, lz) = pad.area.local(a.rig.c.0, a.rig.c.1);
            assert!(pad.area.covers(a.rig.c.0, a.rig.c.1, -a.rig.hz), "{}'s rig outside", a.brand);
            assert!(lz.abs() > pad.area.hz * 0.5, "{}'s rig is not backed onto the fence", a.brand);
            assert!(a.tent.is_some(), "{} has no pop-up", a.brand);
            for b in &pad.spots[i + 1..] {
                let (dx, dz) = pad.area.local(b.rig.c.0, b.rig.c.1);
                let (ex, ez) = pad.area.local(a.rig.c.0, a.rig.c.1);
                assert!((dx - ex).abs() > a.rig.hx * 2.0 || (dz - ez).abs() > a.rig.hz * 2.0, "{} and {} overlap", a.brand, b.brand);
            }
        }
        for k in ["paddock rigs", "paddock popups", "paddock boards"] {
            assert_eq!(v.tally.iter().find(|t| t.0 == k).map(|t| t.1), Some(8), "{k}");
        }
        // Each team's pop-up is the library's whole one — roof and legs — not our drawn canopy.
        assert!(v.kinds.iter().all(|k| k.0 != "paddock_canopies"));
        let roofs = v.kinds.iter().find(|k| k.0 == "paddock_popup_roofs").expect("pop-up roofs");
        assert_eq!(roofs.1.vertex_count(), 8 * 24);
        // Every board prints its own name: the cells differ.
        let sheet = brand_sheet();
        let mut seen = std::collections::HashSet::new();
        for i in 0..BRANDS.len() {
            let (u0, v0, uw, vh) = cell(i);
            let (x0, y0) = ((u0 * 1024.0) as u32, (v0 * 1024.0) as u32);
            let ink = BRANDS[i].2;
            let mut sig = Vec::new();
            for y in (y0 + 20..y0 + (vh * 1024.0) as u32 - 20).step_by(3) {
                for x in (x0 + 20..x0 + (uw * 1024.0) as u32 - 20).step_by(3) {
                    let k = ((y * 1024 + x) * 4) as usize;
                    sig.push(sheet.rgba[k..k + 3] == ink);
                }
            }
            assert!(sig.iter().filter(|b| **b).count() > 200, "{}'s board prints nothing", BRANDS[i].0);
            assert!(seen.insert(sig), "{}'s board is another's", BRANDS[i].0);
        }
    }

    /// The road never comes near the lap: its line keeps clear, and none of its paint, soft edge
    /// and all, lands on the riding surface.
    #[test]
    fn the_road_never_touches_the_lap() {
        let (p, s) = northgate();
        let pl = plan_ng();
        let half = p.width * 0.5;
        assert!(pl.road.len() >= 3, "no road");
        for (x, z) in along(&pl.road, 0.5) {
            assert!(dist(s, x, z) > half + ROAD_W_M * 0.5 + ROAD_FADE_M + 0.9, "the road runs {:.1} m from the centreline", dist(s, x, z));
        }
        let paint = paint(pl).expect("paint");
        let mut n = 0;
        for (x, z, r) in paint.road_cells() {
            if r > 0.02 {
                n += 1;
                assert!(dist(s, x, z) > half + 0.5, "road paint {:.1} m from the centreline", dist(s, x, z));
            }
        }
        assert!(n > 1000, "hardly any road painted");
    }

    /// The road runs from the paddock's aisle, out through its gate, to the start pad beside
    /// the gate row, where the riders line up.
    #[test]
    fn the_road_reaches_the_gate_area() {
        let (_, s) = northgate();
        let pl = plan_ng();
        let area = pl.paddock.expect("a paddock");
        let (start, end) = (pl.road[0], *pl.road.last().unwrap());
        assert!(area.covers(start.0, start.1, -1.0), "the road starts outside the paddock");
        let cross = pl.road.windows(2).find_map(|w| {
            let (a, b) = (area.local(w[0].0, w[0].1), area.local(w[1].0, w[1].1));
            let e = -area.hz;
            let dz = b.1 - a.1;
            ((a.1 - e) * (b.1 - e) <= 0.0).then(|| if dz.abs() < 1e-6 { a.0 } else { a.0 + (b.0 - a.0) * (e - a.1) / dz })
        });
        let x = cross.expect("the road never leaves the paddock");
        assert!(x.abs() + ROAD_W_M * 0.5 <= GATE_M * 0.5 + 0.1, "the road meets the fence {x:.1} m off the gate");
        let (g, ph) = gate_row(s).expect("a gate row");
        let (fx, fz) = crate::trackprog::heading_vector(g.heading);
        let (rx, rz) = crate::trackprog::right_vector(g.heading);
        let (dx, dz) = (end.0 - g.x, end.1 - g.z);
        let (a, c) = (dx * fx + dz * fz, dx * rx + dz * rz);
        assert!(a.abs() < 10.0, "the road ends {a:.1} m along from the gate row");
        assert!((c.abs() - ph).abs() < ROAD_OFF_PAD_M + 3.0, "the road ends {c:.1} m across, the row reaches {ph:.1}");
        assert!(s.outside_the_start(end.0, end.1).is_some_and(|e| e < ROAD_OFF_PAD_M + 2.0), "the road stops short of the pad");
    }

    /// Painted into the ground, not laid on it: the riding soil over the road, grass off it, a
    /// soft edge; the paddock floor level and the road's bed without steps.
    #[test]
    fn the_road_is_painted_and_graded() {
        let (p, s) = northgate();
        let pl = plan_ng();
        let dir = written();
        let dirt = std::fs::read(dir.join("mask_dirt.tga")).unwrap();
        let grass = std::fs::read(dir.join("mask_grass.tga")).unwrap();
        let pts = along(&pl.road[1..], 1.0);
        let mut soft = 0;
        for k in [pts.len() / 4, pts.len() / 2, pts.len() * 3 / 4] {
            let (x, z) = pts[k];
            assert!(mask_at(&dirt, p, x, z) >= 200, "the road is not the riding soil at ({x:.0}, {z:.0})");
            assert!(mask_at(&grass, p, x, z) <= 60, "grass on the road at ({x:.0}, {z:.0})");
            let (a, b) = (pts[k.saturating_sub(2)], pts[(k + 2).min(pts.len() - 1)]);
            let l = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
            let (nx, nz) = (-(b.1 - a.1) / l, (b.0 - a.0) / l);
            for o in 0..40 {
                let d = ROAD_W_M * 0.5 - 1.0 + o as f32 * 0.15;
                let v = mask_at(&dirt, p, x + nx * d, z + nz * d);
                soft += (30..=225).contains(&v) as usize;
            }
        }
        assert!(soft >= 6, "the road's edge is hard: {soft} blended texels across it");
        let area = pl.paddock.unwrap();
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for i in 0..=((area.hx - 1.0) * 2.0) as i32 {
            for j in 0..=((area.hz - 1.0) * 2.0) as i32 {
                let (x, z) = area.at(-area.hx + 1.0 + i as f32, -area.hz + 1.0 + j as f32);
                let h = ground(s, x, z);
                (lo, hi) = (lo.min(h), hi.max(h));
            }
        }
        assert!(hi - lo < 0.2, "the paddock floor falls {:.2} m", hi - lo);
        let hs: Vec<f32> = pts.iter().map(|&(x, z)| ground(s, x, z)).collect();
        let step = hs.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(step < 0.35, "a {step:.2} m step in the road's bed");
    }

    /// The wall stands behind the gate row, square to it, printed side to the gates, off the pad.
    #[test]
    fn the_wall_stands_behind_the_gates() {
        let (p, s) = northgate();
        let v = venue();
        let w = v.wall.expect("a sponsor wall");
        assert!(w.lifted, "the library's wall was not used");
        let (g, _) = gate_row(s).expect("a gate row");
        let (fx, fz) = crate::trackprog::heading_vector(g.heading);
        let (rx, rz) = crate::trackprog::right_vector(g.heading);
        let (dx, dz) = (w.foot.c.0 - g.x, w.foot.c.1 - g.z);
        let (a, across) = (dx * fx + dz * fz, dx * rx + dz * rz);
        assert!(a < -WALL_BACK_M.0 + 0.5 && a > -WALL_BACK_M.1 - 5.0, "the wall is {a:.1} m along from the gates");
        assert!(across.abs() < 3.0, "the wall is {across:.1} m off the gates' middle");
        assert!((w.foot.x.0 * fx + w.foot.x.1 * fz).abs() < 0.05, "the wall is not square to the row");
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
        // And the road keeps out from under it.
        for (x, z) in along(&plan_ng().road, 0.5) {
            assert!(!w.foot.covers(x, z, 1.0), "the road runs under the wall");
        }
    }

    /// Every venue model is one `trackscenery::build` writes: eight vertices or more, whole.
    #[test]
    fn every_venue_model_is_written() {
        let (p, s) = northgate();
        let mut lib = library();
        let w = lib.props.iter_mut().find(|q| q.id == "sponsor_wall").unwrap();
        w.mesh = edfwrite::moved(&edfwrite::card(50.0, 4.0), [0.0, 1.0, 0.0]);
        let v = build(p, s, Some(&lib));
        for (name, m, t, _) in &v.kinds {
            assert!(m.vertex_count() >= 8, "{name} has {} vertices and would not be written", m.vertex_count());
            assert!(m.indices.iter().all(|&i| (i as usize) < m.vertex_count()), "{name} indexes past its vertices");
            assert_eq!(m.uvs.len(), m.vertex_count() * 2, "{name}'s uvs");
            assert_eq!(m.normals.len(), m.vertex_count() * 3, "{name}'s normals");
            assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize, "{name}'s sheet");
        }
    }

    /// Without a library the paddock still has its trucks, canopies, fence and boards, and the
    /// wall is ours.
    #[test]
    fn without_a_library_the_venue_draws_its_own() {
        let (p, s) = northgate();
        let v = build(p, s, None);
        assert!(v.paddock.is_some());
        assert!(v.kinds.iter().any(|k| k.0 == "paddock_trucks" && k.1.triangle_count() >= 8 * 12));
        assert!(v.kinds.iter().any(|k| k.0 == "paddock_canopies"));
        assert!(v.kinds.iter().any(|k| k.0 == "paddock_fence" && k.2.name == "paddock_net_c_a"));
        let w = v.wall.expect("our own wall");
        assert!(!w.lifted);
        assert!(v.kinds.iter().any(|k| k.0 == "sponsor_wall" && k.2.name == "sponsor_wall_c"));
    }

    /// The stadium encloses the floor without ever standing on it: every vertex of the wall and
    /// of the stands keeps its clearance off the centreline and stays on the plot. And each kind
    /// is one model the game will draw — a mesh past 65,535 vertices silently does not.
    #[test]
    fn the_stadium_wall_never_stands_on_the_riding_surface() {
        let (p, s) = sx_track();
        let v = stadium(p, s);
        let half = p.width * 0.5;
        assert!(v.paddock.is_none() && v.road.is_empty() && v.wall.is_none(), "a stadium has no paddock");
        for (name, m, t, solid) in &v.kinds {
            assert!(*solid, "{name} is not solid");
            assert!(m.vertex_count() < 65_536, "{name} has {} vertices and would not draw", m.vertex_count());
            assert_eq!(t.rgba.len(), (t.width * t.height * 4) as usize, "{name}'s sheet");
            for q in m.positions.chunks_exact(3) {
                let d = dist(s, q[0], q[2]);
                // `sx_ring` reads its clearance at the corners, so all that is left to allow for
                // is the grid the distance field is sampled on.
                assert!(d > half + SX_CLEAR_M - 1.5, "{name} stands {d:.1} m from the centreline");
                assert!(on_plot(p, q[0], q[2], 0.0), "{name} stands off the plot");
            }
        }
        let n = |k: &str| v.tally.iter().find(|t| t.0 == k).map(|t| t.1).unwrap_or(0);
        assert!(n("stadium wall") > 40, "{} wall pieces is not a ring", n("stadium wall"));
        assert!(n("stadium tiers") > 40, "{} tier boxes is not a grandstand", n("stadium tiers"));
        let tiers = v.kinds.iter().find(|k| k.0 == "stadium_tiers").expect("the stands");
        let (lo, hi) = tiers.1.bounds();
        assert!(hi[1] - lo[1] > SX_TIER_M.1 * SX_TIERS as f32, "the stands rise {:.1} m", hi[1] - lo[1]);
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

    /// Northgate from above: the paddock, its painted road to the gate row, and the start.
    ///
    /// ```text
    /// FROST_PROPS=library13.fpl FROST_VENUE_PNG=venue.png cargo test --bins -- --ignored --nocapture draw_the_venue
    /// ```
    #[test]
    #[ignore = "draws the venue — set FROST_PROPS and FROST_VENUE_PNG"]
    fn draw_the_venue() {
        let path = std::env::var("FROST_VENUE_PNG").expect("set FROST_VENUE_PNG");
        let (p, s) = northgate();
        let sc = crate::trackscenery::build(p, s);
        let pl = plan(p, s);
        let lib = crate::trackprops::load();
        let v = build(p, s, lib.as_ref());
        let pad = v.paddock.as_ref().expect("a paddock");
        let paint = paint(&pl).unwrap();
        let half = p.width * 0.5;
        let (g, ph) = gate_row(s).expect("a gate row");
        let mut pts: Vec<(f32, f32)> = pl.road.clone();
        for (a, b) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            pts.push(pad.area.at(a * (pad.area.hx + 10.0), b * (pad.area.hz + 10.0)));
        }
        let (fx, fz) = crate::trackprog::heading_vector(g.heading);
        let (rx, rz) = crate::trackprog::right_vector(g.heading);
        for (a, c) in [(-25.0f32, -ph - 12.0), (-25.0, ph + 12.0), (20.0, -ph - 12.0), (20.0, ph + 12.0)] {
            pts.push((g.x + fx * a + rx * c, g.z + fz * a + rz * c));
        }
        let (x0, x1) = (pts.iter().map(|q| q.0).fold(f32::MAX, f32::min), pts.iter().map(|q| q.0).fold(f32::MIN, f32::max));
        let (z0, z1) = (pts.iter().map(|q| q.1).fold(f32::MAX, f32::min), pts.iter().map(|q| q.1).fold(f32::MIN, f32::max));
        let ppm = (2400.0 / (x1 - x0).max(z1 - z0)).min(8.0);
        let (w, h) = (((x1 - x0) * ppm) as u32, ((z1 - z0) * ppm) as u32);
        let mut img = image::RgbImage::new(w, h);
        let mix = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] { std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t.clamp(0.0, 1.0)) };
        for py in 0..h {
            for px in 0..w {
                let (x, z) = (x0 + px as f32 / ppm, z1 - py as f32 / ppm);
                let c = if dist(s, x, z) < half {
                    [150.0, 110.0, 70.0]
                } else if s.outside_the_start(x, z).is_some_and(|e| e < 0.0) {
                    [170.0, 125.0, 80.0]
                } else {
                    let (r, l, lo, f) = paint.at(x, z);
                    let c = mix([72.0, 108.0, 58.0], [178.0, 140.0, 104.0], r.max(f * 0.6));
                    let c = mix(c, [118.0, 88.0, 60.0], l * 0.8);
                    mix(c, [198.0, 164.0, 124.0], lo * 0.5)
                };
                img.put_pixel(px, py, image::Rgb([c[0] as u8, c[1] as u8, c[2] as u8]));
            }
        }
        let mut zb = vec![f32::MAX; (w * h) as usize];
        let proj = |x: f32, y: f32, z: f32| ((x - x0) * ppm, (z1 - z) * ppm, -y);
        for (m, t) in &sc.models {
            let (lo, hi) = m.bounds();
            if hi[0] < x0 || lo[0] > x1 || hi[2] < z0 || lo[2] > z1 {
                continue;
            }
            let tex = (t.width > 0).then(|| (t.width, t.height, t.rgba.as_slice()));
            pic::raster(&mut img, &mut zb, m, tex, [255, 0, 255], &proj);
        }
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
        // The spawns: a ring and a tick the way the bike faces.
        for sp in &pl.spawns {
            let (hx, hz) = crate::trackprog::heading_vector(sp.heading);
            for a in 0..32 {
                let t = a as f32 / 32.0 * std::f32::consts::TAU;
                dot(&mut img, sp.x + t.cos() * 0.8, sp.z + t.sin() * 0.8, 0, [255, 255, 255]);
            }
            for k in 0..6 {
                dot(&mut img, sp.x + hx * k as f32 * 0.25, sp.z + hz * k as f32 * 0.25, 0, [255, 255, 255]);
            }
        }
        if let Some(wl) = v.wall {
            let (x, z) = wl.foot.at(-6.0, -7.0);
            pic::label(&mut img, "SPONSOR WALL", 22.0, (x - x0) * ppm, (z1 - z) * ppm, [255, 255, 255]);
        }
        pic::label(&mut img, "GATES", 22.0, (g.x - x0) * ppm - 25.0, (z1 - g.z) * ppm, [255, 255, 255]);
        for sp in &pad.spots {
            pic::label(&mut img, sp.brand, 20.0, (sp.board.0 - x0) * ppm - 20.0, (z1 - sp.board.1) * ppm, [255, 255, 255]);
        }
        img.save(&path).unwrap();
        println!("VENUE picture {path}: {w}x{h} at {ppm:.1} px/m");
        println!("tally {:?}", sc.tally.iter().filter(|(k, _)| k.starts_with("paddock") || k.starts_with("sponsor") || k.starts_with("venue") || k.starts_with("pit")).collect::<Vec<_>>());
        println!("paddock {:.0} x {:.0} m, road {} points, {} spawns", pad.area.hx * 2.0, pad.area.hz * 2.0, pl.road.len(), pl.spawns.len());
        let lats: Vec<f32> = pl.spawns.iter().map(|s| s.lat).collect();
        println!("spawn lat {:.1}..{:.1}, long {:.1}..{:.1}", lats.iter().cloned().fold(f32::MAX, f32::min), lats.iter().cloned().fold(f32::MIN, f32::max), pl.spawns.iter().map(|s| s.long).fold(f32::MAX, f32::min), pl.spawns.iter().map(|s| s.long).fold(f32::MIN, f32::max));
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

    /// Indiana's whole rigs (touching `semi_trailers_c` pieces) and pop-ups (an `easy_ups` roof
    /// and whatever stands under it), numbered in a contact sheet, side and top.
    ///
    /// ```text
    /// FROST_TRACK=indiana.pkz FROST_OUT=dir cargo test --bins -- --ignored --nocapture donor_rigs_and_canopies
    /// ```
    #[test]
    #[ignore = "reads a donor — set FROST_TRACK and FROST_OUT"]
    fn donor_rigs_and_canopies() {
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        let d = crate::trackprops::open(&std::path::PathBuf::from(std::env::var("FROST_TRACK").expect("set FROST_TRACK"))).unwrap();
        let m = &d.mesh;
        let tex = crate::map::textures(&d.map_bytes, 1024);
        let sheet_of = |i: usize| d.sheets.get(m.objects[i].material as usize).map(|s| s.0.to_ascii_lowercase()).unwrap_or_default();
        let mut picks: Vec<(String, Vec<(u32, Mesh)>)> = Vec::new();
        for g in touching(&d, "semi_trailers_c", 0.5) {
            let (lo, hi) = group_box(m, &g);
            let c = ((lo[0] + hi[0]) * 0.5, lo[1], (lo[2] + hi[2]) * 0.5);
            let mesh = aligned(&lift_islands(m, &g, c));
            let (a, b) = mesh.bounds();
            let (l, w, h) = (b[0] - a[0], b[2] - a[2], b[1] - a[1]);
            if (12.0..=26.0).contains(&l) && w <= 5.0 && h >= 2.5 {
                picks.push((format!("rig {l:.1}x{w:.1}x{h:.1} {} isl", g.len()), vec![(m.objects[g[0]].material, mesh)]));
            }
        }
        for g in touching(&d, "easy_ups_roof_c", 0.3) {
            let (lo, hi) = group_box(m, &g);
            // Everything standing inside the roof's footprint and under it.
            let under: Vec<usize> = (0..m.objects.len())
                .filter(|&i| {
                    let o = &m.objects[i];
                    !g.contains(&i) && o.min[0] >= lo[0] - 0.4 && o.max[0] <= hi[0] + 0.4 && o.min[2] >= lo[2] - 0.4 && o.max[2] <= hi[2] + 0.4 && o.max[1] <= hi[1] + 0.2 && o.min[1] >= lo[1] - 4.0
                })
                .collect();
            let foot = under.iter().map(|&i| m.objects[i].min[1]).fold(lo[1], f32::min);
            let c = ((lo[0] + hi[0]) * 0.5, foot, (lo[2] + hi[2]) * 0.5);
            let mut parts = vec![(m.objects[g[0]].material, lift_islands(m, &g, c))];
            let mut sheets: std::collections::BTreeMap<u32, Vec<usize>> = Default::default();
            for &i in &under {
                sheets.entry(m.objects[i].material).or_default().push(i);
            }
            let names: Vec<String> = sheets.values().map(|v| format!("{}x{}", sheet_of(v[0]), v.len())).collect();
            for (mat, isl) in sheets {
                parts.push((mat, lift_islands(m, &isl, c)));
            }
            picks.push((format!("popup {:.1}x{:.1} h{:.1} {}", hi[0] - lo[0], hi[2] - lo[2], hi[1] - foot, names.join(",")), parts));
        }
        let (cw, ch, ppm, cols) = (360u32, 220u32, 12.0f32, 5u32);
        let rows = (picks.len() as u32).div_ceil(cols).max(1);
        let mut img = image::RgbImage::from_pixel(cw * cols, ch * rows, image::Rgb([60, 70, 80]));
        let mut zb = vec![f32::MAX; (cw * cols * ch * rows) as usize];
        for (n, (label, parts)) in picks.iter().enumerate() {
            let (ox, oy) = ((n as u32 % cols * cw) as f32, (n as u32 / cols * ch) as f32);
            for (mat, mesh) in parts {
                let t = tex.iter().find(|t| t.material == *mat).map(|t| (t.width, t.height, t.rgba.as_slice()));
                pic::raster(&mut img, &mut zb, mesh, t, [255, 0, 255], &|x, y, z| (ox + 180.0 + x * ppm, oy + 110.0 - y * ppm, z));
                pic::raster(&mut img, &mut zb, mesh, t, [255, 0, 255], &|x, y, z| (ox + 180.0 + x * ppm, oy + 160.0 + z * ppm, -y));
            }
            pic::label(&mut img, &format!("{n} {label}"), 14.0, ox + 4.0, oy + 14.0, [255, 255, 255]);
            println!("{n:3} {label}");
        }
        img.save(out.join("donor_rigs_popups.png")).unwrap();
    }

    /// Indiana's trailers and what stands against their ends: which sheet a cab is on.
    #[test]
    #[ignore = "reads a donor — set FROST_TRACK and FROST_OUT"]
    fn donor_rig_parts() {
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        let d = crate::trackprops::open(&std::path::PathBuf::from(std::env::var("FROST_TRACK").expect("set FROST_TRACK"))).unwrap();
        let m = &d.mesh;
        let tex = crate::map::textures(&d.map_bytes, 1024);
        let sheet_of = |i: usize| d.sheets.get(m.objects[i].material as usize).map(|s| s.0.to_ascii_lowercase()).unwrap_or_default();
        let mut picks: Vec<(String, Vec<(u32, Mesh)>)> = Vec::new();
        for g in touching(&d, "semi_trailers_c", 0.5) {
            let (lo, hi) = group_box(m, &g);
            let (l, w, h) = ((hi[0] - lo[0]).max(hi[2] - lo[2]), (hi[0] - lo[0]).min(hi[2] - lo[2]), hi[1] - lo[1]);
            if l < 8.0 || h < 2.5 {
                continue;
            }
            // Pieces of any other sheet standing within 1.5 m of the group's box.
            let near: Vec<usize> = (0..m.objects.len())
                .filter(|&i| {
                    let o = &m.objects[i];
                    !g.contains(&i) && (o.max[1] - o.min[1]) > 0.3 && (0..3).all(|k| o.min[k] <= hi[k] + 1.5 && o.max[k] >= lo[k] - 1.5)
                })
                .collect();
            let mut by: std::collections::BTreeMap<String, (u32, Vec<usize>)> = Default::default();
            for &i in &near {
                by.entry(sheet_of(i)).or_insert((m.objects[i].material, vec![])).1.push(i);
            }
            let c = ((lo[0] + hi[0]) * 0.5, lo[1], (lo[2] + hi[2]) * 0.5);
            let mut parts = vec![(m.objects[g[0]].material, lift_islands(m, &g, c))];
            let mut label = format!("{l:.1}x{w:.1}x{h:.1} {}isl", g.len());
            for (s, (mat, isl)) in &by {
                let (a, b) = group_box(m, isl);
                label.push_str(&format!(" {s}:{}({:.1}x{:.1}h{:.1})", isl.len(), b[0] - a[0], b[2] - a[2], b[1] - a[1]));
                parts.push((*mat, lift_islands(m, isl, c)));
            }
            picks.push((label, parts));
        }
        let (cw, ch, ppm, cols) = (420u32, 260u32, 12.0f32, 4u32);
        let rows = (picks.len() as u32).div_ceil(cols).max(1);
        let mut img = image::RgbImage::from_pixel(cw * cols, ch * rows, image::Rgb([60, 70, 80]));
        let mut zb = vec![f32::MAX; (cw * cols * ch * rows) as usize];
        for (n, (label, parts)) in picks.iter().enumerate() {
            let (ox, oy) = ((n as u32 % cols * cw) as f32, (n as u32 / cols * ch) as f32);
            for (mat, mesh) in parts {
                let t = tex.iter().find(|t| t.material == *mat).map(|t| (t.width, t.height, t.rgba.as_slice()));
                pic::raster(&mut img, &mut zb, mesh, t, [255, 0, 255], &|x, y, z| (ox + 210.0 + x * ppm, oy + 120.0 - y * ppm, z));
                pic::raster(&mut img, &mut zb, mesh, t, [255, 0, 255], &|x, y, z| (ox + 210.0 + x * ppm, oy + 190.0 + z * ppm, -y));
            }
            pic::label(&mut img, &format!("{n}"), 16.0, ox + 4.0, oy + 16.0, [255, 255, 255]);
            println!("{n:3} {label}");
        }
        img.save(out.join("donor_rig_parts.png")).unwrap();
    }

    /// Each donor's `.rdf` pit block: how its spawn stalls are written, and how far off the lap.
    #[test]
    #[ignore = "reads donor tracks — set FROST_DONORS"]
    fn donor_pit_blocks() {
        for path in std::env::var("FROST_DONORS").expect("set FROST_DONORS").split(',').filter(|p| !p.is_empty()) {
            let path = std::path::PathBuf::from(path);
            let names = crate::track::entry_names(&path).unwrap();
            let Some(rdf) = names.iter().find(|n| n.to_ascii_lowercase().ends_with(".rdf")) else { continue };
            let text = String::from_utf8_lossy(&crate::track::read_entry(&path, rdf).unwrap()).replace('\r', "");
            let Some(a) = text.find("pit_lane") else { continue };
            let block = &text[a..text[a..].find("pit_board").map_or(text.len(), |b| a + b)];
            let head: Vec<&str> = block.lines().map(str::trim).filter(|l| l.starts_with("num") || l.starts_with("start") && !l.starts_with("start_stall")).collect();
            let nums = |k: &str| block.lines().filter_map(|l| l.trim().strip_prefix(k).and_then(|v| v.parse::<f32>().ok())).collect::<Vec<_>>();
            let (longs, lats, angs) = (nums("long = "), nums("lat = "), nums("angle = "));
            let lat_max = lats.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            println!("\n{}: {head:?}", path.file_stem().unwrap().to_string_lossy());
            println!("  {} stalls, long {:?}..{:?}, lat {:?}..{:?} (max |lat| {lat_max:.1}), angles {:?}", longs.len(), longs.first(), longs.last(), lats.iter().cloned().fold(f32::MAX, f32::min), lats.iter().cloned().fold(f32::MIN, f32::max), angs.iter().take(4).collect::<Vec<_>>());
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

    /// Add a donor's whole team rigs and one pop-up a colour to a baked library, and draw them.
    ///
    /// ```text
    /// FROST_BASE=library12.fpl FROST_TRACK=indiana.pkz FROST_BAKE=library13.fpl FROST_OUT=dir \
    ///   cargo test --bins -- --ignored --nocapture bake_the_paddock_in
    /// ```
    #[test]
    #[ignore = "bakes a library — set FROST_BASE, FROST_TRACK, FROST_BAKE and FROST_OUT"]
    fn bake_the_paddock_in() {
        let base = std::fs::read(std::env::var("FROST_BASE").expect("set FROST_BASE")).unwrap();
        let mut lib = crate::trackprops::PropLibrary::decode(&base).expect("the base reads");
        let path = std::path::PathBuf::from(std::env::var("FROST_TRACK").expect("set FROST_TRACK"));
        let d = crate::trackprops::open(&path).expect("opens");
        // They wear the library's own copies of the donor's sheets, so it has to be that donor.
        assert_eq!(d.stem, lib.donor, "the rigs must come from the library's own donor");
        lib.props.retain(|p| !p.id.starts_with("team_"));
        let tex = crate::map::textures(&d.map_bytes, 1024);
        let rigs = lift_team_rigs(&d);
        let popups = lift_popups(&d, &tex);
        println!("{} team rigs, {} pop-up parts", rigs.len(), popups.len());
        let added: Vec<crate::trackprops::Prop> = rigs.into_iter().chain(popups).collect();
        // Drawn, side and top, before they go in.
        let out = std::path::PathBuf::from(std::env::var("FROST_OUT").expect("set FROST_OUT"));
        let groups: Vec<Vec<&crate::trackprops::Prop>> = {
            let mut g: Vec<Vec<&crate::trackprops::Prop>> = Vec::new();
            for p in &added {
                let key = p.id.trim_end_matches("_roof").trim_end_matches("_frame");
                match g.iter_mut().find(|v| v[0].id.trim_end_matches("_roof").trim_end_matches("_frame") == key) {
                    Some(v) => v.push(p),
                    None => g.push(vec![p]),
                }
            }
            g
        };
        let (cw, ch, ppm, cols) = (440u32, 300u32, 14.0f32, 4u32);
        let rows = (groups.len() as u32).div_ceil(cols).max(1);
        let mut img = image::RgbImage::from_pixel(cw * cols, ch * rows, image::Rgb([70, 80, 90]));
        let mut zb = vec![f32::MAX; (cw * cols * ch * rows) as usize];
        for (n, g) in groups.iter().enumerate() {
            let (ox, oy) = ((n as u32 % cols * cw) as f32, (n as u32 / cols * ch) as f32);
            for p in g {
                let t = lib.sheets.iter().find(|s| s.0 == p.sheet).map(|s| (s.1, s.2, s.3.as_slice()));
                pic::raster(&mut img, &mut zb, &p.mesh, t, [255, 0, 255], &|x, y, z| (ox + 220.0 + x * ppm, oy + 110.0 - y * ppm, z));
                pic::raster(&mut img, &mut zb, &p.mesh, t, [255, 0, 255], &|x, y, z| (ox + 220.0 + x * ppm, oy + 215.0 + z * ppm, -y));
            }
            let (lo, hi) = g.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |acc, p| {
                let (a, b) = p.mesh.bounds();
                (std::array::from_fn(|k| acc.0[k].min(a[k])), std::array::from_fn(|k| acc.1[k].max(b[k])))
            });
            let name = g[0].id.trim_end_matches("_roof").trim_end_matches("_frame");
            pic::label(&mut img, &format!("{name} {:.1}x{:.1}x{:.1}", hi[0] - lo[0], hi[2] - lo[2], hi[1] - lo[1]), 16.0, ox + 6.0, oy + 18.0, [255, 255, 255]);
            println!("  {name}: {:.1} x {:.1} x {:.1} m, {} parts", hi[0] - lo[0], hi[2] - lo[2], hi[1] - lo[1], g.len());
        }
        img.save(out.join("paddock_picks.png")).unwrap();
        lib.props.extend(added);
        let bake = std::env::var("FROST_BAKE").expect("set FROST_BAKE");
        let bytes = lib.encode();
        std::fs::write(&bake, &bytes).unwrap();
        let back = crate::trackprops::PropLibrary::decode(&std::fs::read(&bake).unwrap()).expect("reads back");
        assert_eq!(back.props.len(), lib.props.len());
        println!("{bake}: {} props, {} sheets, {:.1} MB", back.props.len(), back.sheets.len(), bytes.len() as f32 / 1_048_576.0);
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
