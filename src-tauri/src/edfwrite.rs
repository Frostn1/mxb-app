//! Writing an `.edf`, which is the only mesh format PiBoSo's compilers load.
//!
//! `terrained.exe` places objects from `scene<N>` blocks in a `.hmf`, and `.edf` is the one
//! extension it will open — its own string table says so. So a generated track can only stand
//! anything beside its riding line if we can write one, and nothing here could: [`crate::edf`]
//! is a reader that scans and resynchronises rather than walking a known structure.
//!
//! Reading `flagpoles.edf` from PiBoSo's example track against that reader's own constants
//! accounts for every byte of it, and the container turns out to be small:
//!
//! ```text
//! "EDF\0"
//! f32[6]                  bounding box over the placed geometry, min then max
//! u32                     material count
//!   per material, 56 B:   0 | six 1.0f | 0 | 0,0,0 | colour texture, 1-based | 0 | companion
//! u32                     vertex count, and the node starts here
//!   f32[vc*3] position | f32[vc*2] uv0 | f32[vc*2] uv1 | f32[vc*2] ones | f32[vc*2] ones
//!   f32[vc*3] normal   | f32[vc*4] tangent and its handedness            72 B a vertex
//! u32                     triangle count
//!   u32[tc*3]             a plain triangle list
//! u32                     submesh count, which is 1 whatever the group count is
//! char[104]               the node's name
//! f32[16]                 the node's placement matrix, row-major
//! 72 B                    zeros
//! u32 group count, u32 0
//!   per group, 24 B:      material | tri start | tri count | vert start | vert count | end
//! u32 0, f32[6]           the node's own bounds
//! u32                     texture count
//!   per texture:          char[104] name | u32 w | u32 h | 16 B | 0 | u32 size | 8 B zeros
//!                         | raw-DEFLATE RGBA, `size - 8` bytes
//! ```
//!
//! **A model of several materials is one node of several groups, not several nodes.**
//! `finish_gate.edf` settles it: four materials, four groups, one node called
//! `finishgate_proxy`, and its groups' triangle counts sum to the node's exactly. Written as
//! several nodes instead, TerrainEd faults on a null write and produces nothing — which is
//! how this was found, because a one-part model happens to be byte-identical either way.
//!
//! **One sheet per model.** TerrainEd accepts everything here with a single material —
//! boxes, cut-out cards, two groups sharing one sheet, two hundred copies in one node — and
//! faults on the *second material*, however the geometry is arranged. See
//! `which_ingredient_terrained_refuses`, which is the test that says so, case by case. The
//! reason is in the texture block: real files put a trailer between records whose length
//! varies with the record (8, 24 and 40 bytes in the two example models), and nothing here
//! knows what it says yet. So a model written here carries one sheet, and a thing with
//! several materials is several models — which is exactly what PiBoSo's own example track
//! does, shipping `finish_gate`, `flagpoles` and `standings_tower` as three `scene` blocks.
//!
//! Written this way a file reads back through [`crate::edf::parse`] with its positions, UVs
//! and normals intact. That is the first of three checks; the other two are that
//! `terrained.exe` bakes it and that [`crate::map`] finds the geometry in the `.map` it wrote.

#![allow(dead_code)]

use std::io::Write;

/// Bytes a name field occupies, in a node and in a texture record alike.
const NAME_LEN: usize = 104;
/// Bytes per material record.
const MAT_STRIDE: usize = 56;
/// Where the node's placement matrix sits, from the start of its name.
const NODE_MAT_OFF: usize = 104;
/// Zero words between the placement matrix and the group block.
const GROUP_GAP: usize = 72;

/// A mesh in the frame it will be placed in: metres, Y up, as the game holds them.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    /// `3 * vertex_count`.
    pub positions: Vec<f32>,
    /// `2 * vertex_count`.
    pub uvs: Vec<f32>,
    /// `3 * vertex_count`, unit length.
    pub normals: Vec<f32>,
    /// `3 * triangle_count`.
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Append another mesh, shifting its indices.
    pub fn append(&mut self, other: &Mesh) {
        let base = self.vertex_count() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.uvs.extend_from_slice(&other.uvs);
        self.normals.extend_from_slice(&other.normals);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    /// The box the geometry sits in, as `(min, max)`.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in self.positions.chunks_exact(3) {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        if !lo[0].is_finite() {
            return ([0.0; 3], [0.0; 3]);
        }
        (lo, hi)
    }
}

/// A texture to embed, RGBA8.
#[derive(Clone, Debug)]
pub struct Texture {
    /// The name the material binds by. A `_c_a` suffix marks it an alpha cut-out, which is
    /// what makes a foliage card a tree rather than a standing sheet of paper.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// `width * height * 4`.
    pub rgba: Vec<u8>,
}

/// One drawn part: a mesh and the texture it wears.
#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub mesh: Mesh,
    /// Index into the model's textures.
    pub texture: usize,
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_name(out: &mut Vec<u8>, name: &str) {
    let mut field = [0u8; NAME_LEN];
    for (i, b) in name.bytes().take(NAME_LEN - 1).enumerate() {
        field[i] = b;
    }
    out.extend_from_slice(&field);
}

/// Per-vertex tangents, from the UVs where they say anything and from the normal where they
/// don't. The fourth component is handedness, which every vertex of the example model writes
/// as 1.
fn tangents(mesh: &Mesh) -> Vec<[f32; 4]> {
    let vc = mesh.vertex_count();
    let mut acc = vec![[0.0f32; 3]; vc];
    for t in mesh.indices.chunks_exact(3) {
        let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if i0 >= vc || i1 >= vc || i2 >= vc {
            continue;
        }
        let p = |i: usize| {
            [
                mesh.positions[i * 3],
                mesh.positions[i * 3 + 1],
                mesh.positions[i * 3 + 2],
            ]
        };
        let uv = |i: usize| [mesh.uvs[i * 2], mesh.uvs[i * 2 + 1]];
        let (p0, p1, p2) = (p(i0), p(i1), p(i2));
        let (t0, t1, t2) = (uv(i0), uv(i1), uv(i2));
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let (du1, dv1) = (t1[0] - t0[0], t1[1] - t0[1]);
        let (du2, dv2) = (t2[0] - t0[0], t2[1] - t0[1]);
        let det = du1 * dv2 - du2 * dv1;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let tan = [
            (e1[0] * dv2 - e2[0] * dv1) * r,
            (e1[1] * dv2 - e2[1] * dv1) * r,
            (e1[2] * dv2 - e2[2] * dv1) * r,
        ];
        for &i in &[i0, i1, i2] {
            for k in 0..3 {
                acc[i][k] += tan[k];
            }
        }
    }
    (0..vc)
        .map(|i| {
            let n = [
                mesh.normals[i * 3],
                mesh.normals[i * 3 + 1],
                mesh.normals[i * 3 + 2],
            ];
            let mut t = acc[i];
            // Gram-Schmidt against the normal, so the frame is orthogonal.
            let d = t[0] * n[0] + t[1] * n[1] + t[2] * n[2];
            for k in 0..3 {
                t[k] -= n[k] * d;
            }
            let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
            if len > 1e-6 {
                [t[0] / len, t[1] / len, t[2] / len, 1.0]
            } else {
                // A degenerate UV frame still needs a tangent: any vector across the normal.
                let a = if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
                let c = [
                    a[1] * n[2] - a[2] * n[1],
                    a[2] * n[0] - a[0] * n[2],
                    a[0] * n[1] - a[1] * n[0],
                ];
                let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1e-6);
                [c[0] / l, c[1] / l, c[2] / l, 1.0]
            }
        })
        .collect()
}

/// Write a model: its parts, and the textures they wear.
///
/// The parts become **groups of one node**, in the order given, each binding the texture it
/// names. A part's `name` is for our own bookkeeping — a group has no name field in the file;
/// only the node does, and it takes `name`.
pub fn write(name: &str, parts: &[Part], textures: &[Texture]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"EDF\0");

    // One combined mesh, and where each part landed in it.
    let mut mesh = Mesh::default();
    let mut ranges: Vec<Range> = Vec::with_capacity(parts.len());
    for p in parts {
        let r = Range {
            material: ranges.len() as u32,
            tri_start: mesh.triangle_count() as u32,
            tri_count: p.mesh.triangle_count() as u32,
            vert_start: mesh.vertex_count() as u32,
            vert_count: p.mesh.vertex_count() as u32,
        };
        mesh.append(&p.mesh);
        ranges.push(r);
    }

    // The bounds are written over the placed geometry, and the node matrix below is identity,
    // so the combined mesh is it.
    let (lo, hi) = mesh.bounds();
    for v in lo.iter().chain(hi.iter()) {
        put_f32(&mut out, *v);
    }

    // One material per part. `texture` is 1-based in the file, zero meaning none.
    put_u32(&mut out, parts.len() as u32);
    for p in parts {
        let start = out.len();
        put_u32(&mut out, 0);
        for _ in 0..6 {
            put_f32(&mut out, 1.0);
        }
        for _ in 0..4 {
            put_u32(&mut out, 0);
        }
        put_u32(&mut out, p.texture as u32 + 1);
        put_u32(&mut out, 0);
        put_u32(&mut out, 0);
        debug_assert_eq!(out.len() - start, MAT_STRIDE);
    }

    write_node(&mut out, name, &mesh, &ranges);

    // One count for the whole model, then the records back to back. `finish_gate.edf` is
    // what says this is a count rather than a per-node word: it writes 4 here and carries
    // four sheets, where the one-sheet models write 1.
    put_u32(&mut out, textures.len() as u32);
    for t in textures {
        write_texture(&mut out, t);
    }
    out
}

/// One group of a node: which material paints which run of triangles.
#[derive(Clone, Copy, Debug)]
struct Range {
    material: u32,
    tri_start: u32,
    tri_count: u32,
    vert_start: u32,
    vert_count: u32,
}

fn write_node(out: &mut Vec<u8>, name: &str, mesh: &Mesh, ranges: &[Range]) {
    let vc = mesh.vertex_count();
    let tc = mesh.triangle_count();
    let tans = tangents(mesh);

    put_u32(out, vc as u32);
    for v in &mesh.positions {
        put_f32(out, *v);
    }
    for v in &mesh.uvs {
        put_f32(out, *v);
    }
    // uv1 is zero on every node measured, and the two lanes after it are a constant (1, 1).
    for _ in 0..vc * 2 {
        put_f32(out, 0.0);
    }
    for _ in 0..vc * 4 {
        put_f32(out, 1.0);
    }
    for v in &mesh.normals {
        put_f32(out, *v);
    }
    for t in &tans {
        for v in t {
            put_f32(out, *v);
        }
    }

    put_u32(out, tc as u32);
    for i in &mesh.indices {
        put_u32(out, *i);
    }
    // One, whatever the group count is — `finish_gate.edf` writes 1 here and carries four
    // groups.
    put_u32(out, 1);

    let name_at = out.len();
    put_name(out, name);
    debug_assert_eq!(out.len() - name_at, NODE_MAT_OFF);

    // Placed where it was authored.
    for row in 0..4 {
        for col in 0..4 {
            put_f32(out, if row == col { 1.0 } else { 0.0 });
        }
    }
    for _ in 0..GROUP_GAP / 4 {
        put_u32(out, 0);
    }

    put_u32(out, ranges.len() as u32);
    put_u32(out, 0);
    for (i, r) in ranges.iter().enumerate() {
        put_u32(out, r.material);
        put_u32(out, r.tri_start);
        put_u32(out, r.tri_count);
        put_u32(out, r.vert_start);
        put_u32(out, r.vert_count);
        // Zero on every group but the last, which carries the vertex the node ends at. Both
        // worked examples do this and nothing here needs to know why.
        let last = i + 1 == ranges.len();
        put_u32(out, if last { r.vert_start + r.vert_count } else { 0 });
    }
    put_u32(out, 0);
    let (lo, hi) = mesh.bounds();
    for v in lo.iter().chain(hi.iter()) {
        put_f32(out, *v);
    }
}

fn write_texture(out: &mut Vec<u8>, t: &Texture) {
    put_name(out, &t.name);
    put_u32(out, t.width);
    put_u32(out, t.height);
    // Four words the exporter fills with something of its own and nothing reads, then a zero.
    for _ in 0..5 {
        put_u32(out, 0);
    }
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = enc.write_all(&t.rgba);
    let data = enc.finish().unwrap_or_default();
    put_u32(out, data.len() as u32 + 8);
    put_u32(out, 0);
    put_u32(out, 0);
    out.extend_from_slice(&data);
}

/// A flat card standing upright, `w` across and `h` tall, centred on the origin and facing
/// +z. The primitive nearly everything on a track is made of.
pub fn card(w: f32, h: f32) -> Mesh {
    let (hw, y) = (w * 0.5, h);
    Mesh {
        positions: vec![-hw, 0.0, 0.0, hw, 0.0, 0.0, hw, y, 0.0, -hw, y, 0.0],
        uvs: vec![0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// `n` cards through the same upright axis, evenly spread — the shape a tree, a bush or a
/// crowd line is in every published track: flat quads whose sheet cuts the silhouette out.
///
/// Two is the usual answer and three reads better from a low camera, which is where a rider
/// is. Also the smallest honest node: a lone card is four vertices, and
/// [`crate::edf::parse`] cannot see a node that small.
pub fn crossed(w: f32, h: f32, n: usize) -> Mesh {
    let mut mesh = Mesh::default();
    for i in 0..n.max(2) {
        let deg = 180.0 * i as f32 / n.max(2) as f32;
        mesh.append(&turned(&card(w, h), deg));
    }
    mesh
}

/// Rotate a mesh about Y, degrees.
pub fn turned(mesh: &Mesh, deg: f32) -> Mesh {
    let (s, c) = deg.to_radians().sin_cos();
    let spin = |v: &[f32]| vec![v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c];
    Mesh {
        positions: mesh.positions.chunks_exact(3).flat_map(spin).collect(),
        normals: mesh.normals.chunks_exact(3).flat_map(spin).collect(),
        uvs: mesh.uvs.clone(),
        indices: mesh.indices.clone(),
    }
}

/// Move a mesh.
pub fn moved(mesh: &Mesh, by: [f32; 3]) -> Mesh {
    Mesh {
        positions: mesh
            .positions
            .chunks_exact(3)
            .flat_map(|v| vec![v[0] + by[0], v[1] + by[1], v[2] + by[2]])
            .collect(),
        ..mesh.clone()
    }
}

/// A box with its base on y = 0, `w` by `d` and `h` tall, centred on the origin in x and z.
pub fn cuboid(w: f32, h: f32, d: f32) -> Mesh {
    let (hw, hd) = (w * 0.5, d * 0.5);
    let mut mesh = Mesh::default();
    // Six faces, each its own four vertices so every one carries its own normal.
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let half = [hw, h * 0.5, hd];
    for (n, u, v) in faces {
        let centre = [n[0] * half[0], h * 0.5 + n[1] * half[1], n[2] * half[2]];
        let eu = [u[0] * half[0], u[1] * half[1], u[2] * half[2]];
        let ev = [v[0] * half[0], v[1] * half[1], v[2] * half[2]];
        let base = mesh.vertex_count() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            for k in 0..3 {
                mesh.positions
                    .push(centre[k] + eu[k] * su as f32 + ev[k] * sv as f32);
            }
            mesh.normals.extend_from_slice(&n);
        }
        mesh.uvs
            .extend_from_slice(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat sheet, so the check is on the container rather than on the picture.
    fn sheet(name: &str, dim: u32, rgba: [u8; 4]) -> Texture {
        Texture {
            name: name.to_string(),
            width: dim,
            height: dim,
            rgba: rgba
                .iter()
                .copied()
                .cycle()
                .take((dim * dim * 4) as usize)
                .collect(),
        }
    }

    #[test]
    fn a_written_edf_reads_back_through_our_own_parser() {
        let mesh = cuboid(2.0, 3.0, 1.0);
        let (vc, tc) = (mesh.vertex_count(), mesh.triangle_count());
        let bytes = write(
            "probe",
            &[Part { name: "post".into(), mesh: mesh.clone(), texture: 0 }],
            &[sheet("post_c", 64, [200, 180, 60, 255])],
        );

        assert!(crate::edf::is_edf(&bytes));
        let nodes = crate::edf::parse_world(&bytes);
        assert_eq!(nodes.len(), 1, "one part, one node");
        let n = &nodes[0];
        assert_eq!(n.name, "probe", "the node takes the model's name");
        assert_eq!(n.positions.len(), vc * 3);
        assert_eq!(n.normals.len(), vc * 3);
        assert_eq!(n.uvs.len(), vc * 2);
        assert_eq!(n.indices.len() / 3, tc);

        // The geometry has to come back where it was written, not merely in the right shape.
        for (a, b) in n.positions.iter().zip(mesh.positions.iter()) {
            assert!((a - b).abs() < 1e-4, "{a} != {b}");
        }
    }

    #[test]
    fn the_header_states_the_bounds_the_geometry_actually_has() {
        // The one statement the file makes about where the model sits, and the reader
        // checks placed geometry against it — see `edf::header_aabb`.
        let mesh = moved(&cuboid(2.0, 3.0, 1.0), [5.0, 0.0, -4.0]);
        let bytes = write(
            "probe",
            &[Part { name: "post".into(), mesh: mesh.clone(), texture: 0 }],
            &[sheet("post_c", 64, [1, 2, 3, 255])],
        );
        let (lo, hi) = crate::edf::header_aabb(&bytes).expect("header states bounds");
        let (mlo, mhi) = mesh.bounds();
        for k in 0..3 {
            assert!((lo[k] - mlo[k]).abs() < 1e-4, "min {k}: {} != {}", lo[k], mlo[k]);
            assert!((hi[k] - mhi[k]).abs() < 1e-4, "max {k}: {} != {}", hi[k], mhi[k]);
        }
    }

    #[test]
    fn the_texture_comes_back_out_of_the_file() {
        let tex = sheet("bark_c_a", 64, [10, 120, 30, 255]);
        let bytes = write(
            "probe",
            &[Part { name: "trunk".into(), mesh: card(1.0, 4.0), texture: 0 }],
            &[tex.clone()],
        );
        let found = crate::edf::embedded_textures(&bytes);
        assert_eq!(found.len(), 1, "one texture in, one out");
        assert_eq!(found[0].name, "bark_c_a");
        assert_eq!((found[0].width, found[0].height), (64, 64));
        let rgba = crate::edf::inflate_texture(&bytes, &found[0]).expect("inflates");
        assert_eq!(rgba, tex.rgba, "the pixels survive the round trip");
    }

    #[test]
    fn several_parts_are_one_node_of_several_groups() {
        let bytes = write(
            "tree",
            &[
                Part { name: "trunk".into(), mesh: cuboid(0.4, 4.0, 0.4), texture: 0 },
                Part { name: "canopy".into(), mesh: crossed(5.0, 5.0, 2), texture: 1 },
            ],
            &[sheet("bark_c", 64, [90, 60, 40, 255]), sheet("leaf_c_a", 64, [40, 110, 40, 128])],
        );
        // One node. Written as two, TerrainEd faults on a null write — see the module note.
        let nodes = crate::edf::parse_world(&bytes);
        assert_eq!(nodes.len(), 1, "several materials are one node, not several");
        assert_eq!(nodes[0].name, "tree");

        // Both sheets travel, and the node's material table names them in order.
        let found = crate::edf::embedded_textures(&bytes);
        assert_eq!(found.len(), 2);
        let table = crate::edf::node_material_table(&bytes, 0x1c + 4 + MAT_STRIDE * 2, found.len());
        assert_eq!(table, vec![Some(0), Some(1)], "two materials, two sheets, in order");

        // And the groups split the triangles between them the way the parts were given.
        let (trunk_t, canopy_t) = (cuboid(0.4, 4.0, 0.4).triangle_count(), crossed(5.0, 5.0, 2).triangle_count());
        assert_eq!(nodes[0].indices.len() / 3, trunk_t + canopy_t);
    }

    #[test]
    fn a_node_under_eight_vertices_is_invisible_to_the_reader() {
        // Not a fault in what is written — it is [`crate::edf::parse`]'s own floor, and it
        // exists because that parser finds nodes by scanning for a plausible vertex count in
        // a file it cannot walk. A lone card is four vertices and lands under it. Everything
        // this module builds crosses cards or boxes them, so nothing real sits below eight,
        // but a bare `card` node would vanish and it is better to have said so.
        let bytes = write(
            "lone",
            &[Part { name: "lone".into(), mesh: card(1.0, 1.0), texture: 0 }],
            &[sheet("x_c", 64, [1, 1, 1, 255])],
        );
        assert!(crate::edf::parse_world(&bytes).is_empty());
        assert_eq!(crate::edf::embedded_textures(&bytes).len(), 1, "the sheet is still there");
    }

    #[test]
    fn a_card_stands_up_and_faces_the_way_it_was_turned() {
        let m = turned(&card(2.0, 2.0), 90.0);
        // Facing +z turned a quarter is facing +x, and it still stands from y = 0 to y = 2.
        assert!((m.normals[0] - 1.0).abs() < 1e-5, "{:?}", &m.normals[..3]);
        let (lo, hi) = m.bounds();
        assert!((lo[1] - 0.0).abs() < 1e-5 && (hi[1] - 2.0).abs() < 1e-5);
    }
}

#[cfg(test)]
mod compiles {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A sheet with something to look at, so a compiled map can be told apart from a blank.
    fn checks(name: &str, dim: u32, a: [u8; 4], b: [u8; 4]) -> Texture {
        let mut rgba = Vec::with_capacity((dim * dim * 4) as usize);
        for y in 0..dim {
            for x in 0..dim {
                let c = if (x / 8 + y / 8) % 2 == 0 { a } else { b };
                rgba.extend_from_slice(&c);
            }
        }
        Texture { name: name.into(), width: dim, height: dim, rgba }
    }

    /// The one check that matters: `terrained.exe` opens what we wrote, and the geometry
    /// comes back out of the `.map` it compiled, where the `scene` block asked for it.
    ///
    /// ```text
    /// FROST_TOOLS=~/Downloads/mxb-trackbuild/tools \
    /// FROST_PREFIX=~/Downloads/mxb-trackbuild/prefix \
    /// FROST_WINE="~/Downloads/mxb-trackbuild/Wine Devel.app/Contents/Resources/wine/bin/wine" \
    /// FROST_SCENE=/tmp/edftest \
    ///   cargo test -- --ignored --nocapture terrained_bakes_what_we_wrote
    /// ```
    ///
    /// `FROST_SCENE` is a folder holding a `track.hmf` whose `scene0` names `probe.edf`, a
    /// flat `heightmap.raw` and one layer sheet — the smallest thing TerrainEd will compile.
    #[test]
    #[ignore = "needs PiBoSo's compilers and a Wine prefix"]
    fn terrained_bakes_what_we_wrote() {
        let dir = PathBuf::from(std::env::var("FROST_SCENE").expect("set FROST_SCENE"));
        let tools = PathBuf::from(std::env::var("FROST_TOOLS").expect("set FROST_TOOLS"));
        let terrained = crate::trackbuild::find(&tools)
            .expect("terrained.exe under FROST_TOOLS")
            .terrained;

        // A post you could not mistake for terrain: two metres square, four tall, standing
        // where the scene block says and nowhere else.
        let (w, h) = (2.0f32, 4.0f32);
        let bytes = write(
            "probe",
            &[Part { name: "probe".into(), mesh: cuboid(w, h, w), texture: 0 }],
            &[checks("probe_c", 64, [220, 40, 40, 255], [250, 250, 250, 255])],
        );
        std::fs::write(dir.join("probe.edf"), &bytes).unwrap();
        println!("probe.edf: {} bytes", bytes.len());

        let map = "out/probe.map";
        let _ = std::fs::remove_file(dir.join(map));
        let out = run(&terrained, &["track.hmf", map, "params.ini"], &dir);
        println!("--- terrained ---\n{out}\n---");

        let produced = dir.join(map);
        assert!(produced.is_file(), "terrained wrote no {map}");
        let mb = std::fs::read(&produced).unwrap();
        let mesh = crate::map::parse(&mb).expect("the .map parses");
        println!(
            "{} materials, {} vertices, {} triangles, {} islands",
            mesh.materials,
            mesh.vertex_count(),
            mesh.triangle_count(),
            mesh.objects.len()
        );

        // The scene block put it at (40, 3, 50). Terrain in this scene is flat at y = 0 and
        // covers 128 m square, so anything standing between 3 and 7 metres up is ours.
        let ours: Vec<[f32; 3]> = mesh
            .positions
            .chunks_exact(3)
            .filter(|v| v[1] > 2.5)
            .map(|v| [v[0], v[1], v[2]])
            .collect();
        assert!(!ours.is_empty(), "nothing in the map stands above the ground");
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for v in &ours {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        println!("what stands above the ground: {lo:?} .. {hi:?}");
        assert!((hi[1] - (3.0 + h)).abs() < 0.5, "top should be at {}, is {}", 3.0 + h, hi[1]);
        assert!((hi[0] - lo[0] - w).abs() < 0.5, "should be {w} m across, is {}", hi[0] - lo[0]);
        // And the texture travelled with it.
        let names: Vec<String> = crate::map::survey(&mb).into_iter().map(|(n, ..)| n).collect();
        println!("sheets in the map: {names:?}");
        assert!(
            names.iter().any(|n| n.eq_ignore_ascii_case("probe_c")),
            "the sheet the model embedded is not in the map: {names:?}"
        );
    }

    /// The same, but a model of several parts wearing several sheets — which is what a
    /// track's scenery is, and which the single-part proof above does not cover.
    #[test]
    #[ignore = "needs PiBoSo's compilers and a Wine prefix"]
    fn terrained_bakes_a_model_of_several_parts() {
        let dir = PathBuf::from(std::env::var("FROST_SCENE").expect("set FROST_SCENE"));
        let tools = PathBuf::from(std::env::var("FROST_TOOLS").expect("set FROST_TOOLS"));
        let terrained = crate::trackbuild::find(&tools)
            .expect("terrained.exe under FROST_TOOLS")
            .terrained;

        let parts = vec![
            Part { name: "posts".into(), mesh: cuboid(1.0, 4.0, 1.0), texture: 0 },
            Part { name: "boards".into(), mesh: moved(&crossed(3.0, 2.0, 2), [6.0, 0.0, 0.0]), texture: 1 },
            Part { name: "canopy".into(), mesh: moved(&crossed(4.0, 5.0, 3), [-6.0, 0.0, 0.0]), texture: 2 },
        ];
        let sheets = vec![
            checks("posts_c", 64, [200, 60, 60, 255], [240, 240, 240, 255]),
            checks("boards_c_a", 64, [40, 90, 200, 255], [255, 255, 255, 0]),
            checks("canopy_c_a", 64, [40, 150, 60, 255], [255, 255, 255, 0]),
        ];
        let bytes = write("probe", &parts, &sheets);
        std::fs::write(dir.join("probe.edf"), &bytes).unwrap();
        println!("probe.edf: {} bytes, {} parts, {} sheets", bytes.len(), parts.len(), sheets.len());

        let map = "out/multi.map";
        let _ = std::fs::remove_file(dir.join(map));
        let out = run(&terrained, &["track.hmf", map, "params.ini"], &dir);
        println!("--- terrained ---\n{out}\n---");

        assert!(dir.join(map).is_file(), "terrained wrote no {map}");
        let mb = std::fs::read(dir.join(map)).unwrap();
        let mesh = crate::map::parse(&mb).expect("the .map parses");
        let names: Vec<String> = crate::map::survey(&mb).into_iter().map(|(n, ..)| n).collect();
        println!(
            "{} materials, {} triangles, sheets {names:?}",
            mesh.materials,
            mesh.triangle_count()
        );
        for want in ["posts_c", "boards_c_a", "canopy_c_a"] {
            assert!(names.iter().any(|n| n.eq_ignore_ascii_case(want)), "{want} missing: {names:?}");
        }
    }

    /// Which ingredient TerrainEd refuses, one at a time.
    ///
    /// A control it is known to accept, then one change each: a second group, a cut-out
    /// sheet, cards instead of boxes. Cheap because the scene is 129 samples square; the
    /// answer is which rows say `ok`.
    #[test]
    #[ignore = "needs PiBoSo's compilers and a Wine prefix"]
    fn which_ingredient_terrained_refuses() {
        let dir = PathBuf::from(std::env::var("FROST_SCENE").expect("set FROST_SCENE"));
        let tools = PathBuf::from(std::env::var("FROST_TOOLS").expect("set FROST_TOOLS"));
        let terrained = crate::trackbuild::find(&tools).expect("terrained").terrained;

        let opaque = |n: &str| checks(n, 64, [210, 60, 50, 255], [240, 240, 240, 255]);
        let cutout = |n: &str| checks(n, 64, [60, 150, 70, 255], [0, 0, 0, 0]);

        let cases: Vec<(&str, Vec<Part>, Vec<Texture>)> = vec![
            ("1 box, opaque (control)",
             vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 }],
             vec![opaque("a_c")]),
            ("2 boxes, 2 sheets",
             vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 },
                  Part { name: "b".into(), mesh: moved(&cuboid(2.0, 3.0, 2.0), [6.0, 0.0, 0.0]), texture: 1 }],
             vec![opaque("a_c"), opaque("b_c")]),
            ("1 box, cut-out sheet",
             vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 }],
             vec![cutout("a_c_a")]),
            ("1 set of crossed cards",
             vec![Part { name: "a".into(), mesh: crossed(4.0, 5.0, 3), texture: 0 }],
             vec![cutout("a_c_a")]),
            ("3 parts, mixed",
             vec![Part { name: "a".into(), mesh: cuboid(1.0, 4.0, 1.0), texture: 0 },
                  Part { name: "b".into(), mesh: moved(&crossed(3.0, 2.0, 2), [6.0, 0.0, 0.0]), texture: 1 },
                  Part { name: "c".into(), mesh: moved(&crossed(4.0, 5.0, 3), [-6.0, 0.0, 0.0]), texture: 2 }],
             vec![opaque("a_c"), cutout("b_c_a"), cutout("c_c_a")]),
        ];

        let mut verdict = Vec::new();
        for (i, (label, parts, sheets)) in cases.iter().enumerate() {
            let bytes = write("probe", parts, sheets);
            std::fs::write(dir.join("probe.edf"), &bytes).unwrap();
            let map = format!("out/case{i}.map");
            let _ = std::fs::remove_file(dir.join(&map));
            let out = run(&terrained, &["track.hmf", &map, "params.ini"], &dir);
            let ok = dir.join(&map).is_file();
            let tris = ok
                .then(|| std::fs::read(dir.join(&map)).ok())
                .flatten()
                .and_then(|b| crate::map::parse(&b))
                .map(|m| m.triangle_count())
                .unwrap_or(0);
            println!(
                "{:<26} {:>10}  {:>5} tris  ({} B edf)  {}",
                label,
                if ok { "ok" } else { "REFUSED" },
                tris,
                bytes.len(),
                out.lines().next().unwrap_or("")
            );
            verdict.push((label, ok));
        }
        println!();
        for (l, ok) in &verdict {
            println!("  {} {l}", if *ok { "ok " } else { "NO " });
        }
        assert!(verdict[0].1, "the control has to compile or nothing here means anything");
    }

    /// Write one probe model to `FROST_SCENE/probe.edf` and stop. Lets the compiler be driven
    /// from outside, one case at a time, so a crash costs one run instead of the whole set.
    #[test]
    #[ignore = "writes a probe model — set FROST_SCENE and FROST_CASE"]
    fn write_probe_case() {
        let dir = PathBuf::from(std::env::var("FROST_SCENE").expect("set FROST_SCENE"));
        let case: usize = std::env::var("FROST_CASE").unwrap_or_default().parse().unwrap_or(0);
        let opaque = |n: &str| checks(n, 64, [210, 60, 50, 255], [240, 240, 240, 255]);
        let cutout = |n: &str| checks(n, 64, [60, 150, 70, 255], [0, 0, 0, 0]);
        let (label, parts, sheets): (&str, Vec<Part>, Vec<Texture>) = match case {
            0 => ("1 box, opaque (control)",
                  vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 }],
                  vec![opaque("a_c")]),
            1 => ("2 boxes, 2 sheets",
                  vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 },
                       Part { name: "b".into(), mesh: moved(&cuboid(2.0, 3.0, 2.0), [6.0, 0.0, 0.0]), texture: 1 }],
                  vec![opaque("a_c"), opaque("b_c")]),
            2 => ("1 box, cut-out sheet",
                  vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 }],
                  vec![cutout("a_c_a")]),
            3 => ("1 set of crossed cards",
                  vec![Part { name: "a".into(), mesh: crossed(4.0, 5.0, 3), texture: 0 }],
                  vec![cutout("a_c_a")]),
            4 => ("2 boxes, 1 shared sheet",
                  vec![Part { name: "a".into(), mesh: cuboid(2.0, 4.0, 2.0), texture: 0 },
                       Part { name: "b".into(), mesh: moved(&cuboid(2.0, 3.0, 2.0), [6.0, 0.0, 0.0]), texture: 0 }],
                  vec![opaque("a_c")]),
            5 => ("1 box, 200 copies",
                  vec![Part { name: "a".into(), mesh: {
                      let mut m = Mesh::default();
                      for i in 0..200 {
                          let (x, z) = ((i % 20) as f32 * 3.0, (i / 20) as f32 * 3.0);
                          m.append(&moved(&cuboid(1.0, 2.0, 1.0), [20.0 + x, 0.0, 20.0 + z]));
                      }
                      m
                  }, texture: 0 }],
                  vec![opaque("a_c")]),
            _ => ("3 parts, mixed",
                  vec![Part { name: "a".into(), mesh: cuboid(1.0, 4.0, 1.0), texture: 0 },
                       Part { name: "b".into(), mesh: moved(&crossed(3.0, 2.0, 2), [6.0, 0.0, 0.0]), texture: 1 },
                       Part { name: "c".into(), mesh: moved(&crossed(4.0, 5.0, 3), [-6.0, 0.0, 0.0]), texture: 2 }],
                  vec![opaque("a_c"), cutout("b_c_a"), cutout("c_c_a")]),
        };
        let bytes = write("probe", &parts, &sheets);
        std::fs::write(dir.join("probe.edf"), &bytes).unwrap();
        println!("case {case}: {label} — {} parts, {} sheets, {} B", parts.len(), sheets.len(), bytes.len());
    }

    fn run(exe: &Path, args: &[&str], dir: &Path) -> String {
        let mut cmd = if cfg!(target_os = "windows") {
            std::process::Command::new(exe)
        } else {
            let wine = std::env::var("FROST_WINE").expect("set FROST_WINE");
            let mut c = std::process::Command::new(wine);
            c.env(
                "WINEPREFIX",
                std::env::var("FROST_PREFIX").expect("set FROST_PREFIX"),
            );
            c.env("WINEDEBUG", "-all");
            c.arg(exe);
            c
        };
        cmd.args(args).current_dir(dir);
        let out = cmd.output().expect("running terrained");
        format!(
            "exit {:?}\n{}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }
}
