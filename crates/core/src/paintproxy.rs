//! A painting proxy: a model's mesh cut down to a stand-in a painter can work on in Blender or
//! Substance, without the model itself ever leaving the creator. Any model: a bike, a helmet, a
//! pair of boots, a bar pad on its own.
//!
//! The UV layout is the one thing kept exactly — a paint made on the proxy has to land on the
//! real model — and everything that makes a mesh worth taking is dropped: most of the
//! triangles, the normals, the LODs (the viewer only reads level0), and sub-millimetre position
//! detail.
//!
//! The ratio is a constant on purpose. A slider would let anyone set it back to 100%.

use crate::edf::{EdfNode, Submesh};

/// Share of each part's triangles the proxy aims to keep.
pub const KEEP: f32 = 0.15;
/// Fewest triangles a part is cut to, so a small bracket doesn't collapse to nothing.
const MIN_TRIS: usize = 64;
/// Grid positions are rounded to, in model units (metres).
const QUANT: f32 = 0.001;

/// One mesh part of the proxy, compacted to the vertices it still uses.
pub struct ProxyPart {
    pub name: String,
    pub positions: Vec<f32>,
    /// Straight from the source mesh, never re-interpolated.
    pub uvs: Vec<f32>,
    /// Triangles per texture, in the order the part names them.
    pub groups: Vec<(Option<String>, Vec<u32>)>,
}

pub struct Proxy {
    pub parts: Vec<ProxyPart>,
    /// Triangles in the mesh the proxy was cut from.
    pub source_triangles: usize,
}

impl Proxy {
    pub fn triangles(&self) -> usize {
        self.parts.iter().flat_map(|p| &p.groups).map(|(_, i)| i.len() / 3).sum()
    }

    /// The proxy as meshes the webview already knows how to read, one submesh per texture —
    /// so the Designer can build the `.glb` from exactly what went into the `.obj`.
    pub fn to_nodes(&self) -> Vec<EdfNode> {
        self.parts
            .iter()
            .map(|p| {
                let mut indices = Vec::new();
                let mut submeshes = Vec::new();
                for (texture, idx) in &p.groups {
                    submeshes.push(Submesh {
                        name: texture.clone().unwrap_or_else(|| "untextured".into()),
                        tri_start: (indices.len() / 3) as u32,
                        tri_count: (idx.len() / 3) as u32,
                        texture: texture.clone(),
                        uv_tile: None,
                        mat: None,
                    });
                    indices.extend_from_slice(idx);
                }
                EdfNode {
                    name: p.name.clone(),
                    positions: p.positions.clone(),
                    uvs: p.uvs.clone(),
                    normals: Vec::new(),
                    indices,
                    submeshes,
                    texture: None,
                    placed: true,
                    materials: Vec::new(),
                }
            })
            .collect()
    }
}

/// The file stem a texture's template is written under — shared with the Designer, which writes
/// the PNGs this module's `.mtl` points at. Extension off, then anything a file name or an OBJ
/// statement can't hold becomes `_`.
pub fn template_stem(texture: &str) -> String {
    let base = match texture.rfind('.') {
        Some(i) if i > 0 => &texture[..i],
        _ => texture,
    };
    base.chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) || c.is_whitespace() { '_' } else { c })
        .collect()
}

pub fn build(nodes: &[EdfNode]) -> Proxy {
    let mut parts = Vec::new();
    let mut source_triangles = 0;
    for node in nodes {
        let vcount = node.positions.len() / 3;
        if node.indices.is_empty() || node.uvs.len() != vcount * 2 {
            continue;
        }
        if node.indices.iter().any(|&i| i as usize >= vcount) {
            continue;
        }
        let tris = node.indices.len() / 3;
        source_triangles += tris;

        // Weld corners that agree on position and UV. A mesh also splits a vertex for its
        // normals, and the simplifier sees only positions, so each such split reads to it as a
        // seam it must keep. Normals are dropped anyway; real UV seams still differ by UV.
        let mut welded: std::collections::HashMap<[u32; 5], u32> = std::collections::HashMap::new();
        let (mut pos, mut uv) = (Vec::new(), Vec::new());
        let mut weld = vec![0u32; vcount];
        for v in 0..vcount {
            let p = &node.positions[v * 3..v * 3 + 3];
            let t = &node.uvs[v * 2..v * 2 + 2];
            let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits(), t[0].to_bits(), t[1].to_bits()];
            weld[v] = *welded.entry(key).or_insert_with(|| {
                pos.extend_from_slice(p);
                uv.extend_from_slice(t);
                (pos.len() / 3 - 1) as u32
            });
        }
        let indices: Vec<u32> = node.indices.iter().map(|&i| weld[i as usize]).collect();
        let Ok(adapter) = meshopt::VertexDataAdapter::new(meshopt::typed_to_bytes(&pos), 12, 0)
        else {
            continue;
        };

        // Per texture, so the cut never moves a triangle from one sheet to another.
        let ranges: Vec<(Option<String>, usize, usize)> = if node.submeshes.is_empty() {
            vec![(node.texture.clone(), 0, tris)]
        } else {
            node.submeshes
                .iter()
                .map(|s| (s.texture.clone(), s.tri_start as usize, s.tri_count as usize))
                .collect()
        };

        let mut groups = Vec::new();
        for (texture, start, count) in ranges {
            let end = (start + count).min(tris);
            if start >= end {
                continue;
            }
            let src = &indices[start * 3..end * 3];
            let target = ((count as f32 * KEEP) as usize).max(MIN_TRIS).min(end - start) * 3;
            // No `Permissive`: meshopt then never collapses across a seam, and what comes back
            // indexes the original vertices — so every UV is the source's own value.
            let cut = meshopt::simplify(src, &adapter, target, 1.0, meshopt::SimplifyOptions::None, None);
            if !cut.is_empty() {
                groups.push((texture, cut));
            }
        }
        if groups.is_empty() {
            continue;
        }

        // Compact to the vertices still in use, in first-use order.
        let mut remap = vec![u32::MAX; pos.len() / 3];
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for (_, idx) in groups.iter_mut() {
            for i in idx.iter_mut() {
                let v = *i as usize;
                if remap[v] == u32::MAX {
                    remap[v] = (positions.len() / 3) as u32;
                    for k in 0..3 {
                        positions.push((pos[v * 3 + k] / QUANT).round() * QUANT);
                    }
                    uvs.extend_from_slice(&uv[v * 2..v * 2 + 2]);
                }
                *i = remap[v];
            }
        }
        parts.push(ProxyPart { name: node.name.clone(), positions, uvs, groups });
    }
    Proxy { parts, source_triangles }
}

/// The proxy as Wavefront OBJ and its material library, which names `<stem>_template.png`
/// per texture.
///
/// `v` is written as the game stores it. The game samples a sheet top-down and keeps its rows
/// upside-down from what a painter sees; OBJ counts `v` up from the bottom of the image. The
/// two flips cancel, so the templates — drawn the way painters see sheets — land right way up.
pub fn to_obj(proxy: &Proxy, mtl_file: &str) -> (String, String) {
    use std::fmt::Write;
    let mut obj = String::new();
    let _ = writeln!(obj, "# Painting proxy from Frost's Studio. A stand-in to paint on, not a game model.");
    let _ = writeln!(obj, "mtllib {mtl_file}");
    let mut materials: Vec<String> = Vec::new();
    let mut base = 1u32;
    for part in &proxy.parts {
        let _ = writeln!(obj, "o {}", template_stem(&part.name));
        for p in part.positions.chunks_exact(3) {
            let _ = writeln!(obj, "v {:.3} {:.3} {:.3}", p[0], p[1], p[2]);
        }
        for t in part.uvs.chunks_exact(2) {
            let _ = writeln!(obj, "vt {} {}", t[0], t[1]);
        }
        for (texture, idx) in &part.groups {
            let mat = texture.as_deref().map(template_stem).unwrap_or_else(|| "untextured".into());
            if !materials.contains(&mat) {
                materials.push(mat.clone());
            }
            let _ = writeln!(obj, "usemtl {mat}");
            for t in idx.chunks_exact(3) {
                let (a, b, c) = (t[0] + base, t[1] + base, t[2] + base);
                let _ = writeln!(obj, "f {a}/{a} {b}/{b} {c}/{c}");
            }
        }
        base += (part.positions.len() / 3) as u32;
    }

    let mut mtl = String::new();
    for mat in &materials {
        let _ = writeln!(mtl, "newmtl {mat}");
        let _ = writeln!(mtl, "Kd 1 1 1");
        if mat != "untextured" {
            let _ = writeln!(mtl, "map_Kd {mat}_template.png");
        }
        let _ = writeln!(mtl);
    }
    (obj, mtl)
}

/// The meshes the Designer sends: whatever model it has on screen, packed flat.
///
/// Little-endian throughout. `u32` node count; per node a name, a texture, `u32` vertex count,
/// the positions (3 × f32 each) and UVs (2 × f32 each), `u32` index count and the indices, then
/// `u32` submesh count and per submesh a name, a texture, and `u32` first triangle and count.
/// A string is a `u32` byte length and UTF-8, with `u32::MAX` for none.
pub fn decode_nodes(bytes: &[u8]) -> Option<Vec<EdfNode>> {
    struct Cur<'a>(&'a [u8]);
    impl Cur<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> {
            if n > self.0.len() {
                return None;
            }
            let (a, b) = self.0.split_at(n);
            self.0 = b;
            Some(a)
        }
        fn u32(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
        fn str(&mut self) -> Option<Option<String>> {
            match self.u32()? {
                u32::MAX => Some(None),
                n => Some(Some(String::from_utf8_lossy(self.take(n as usize)?).into_owned())),
            }
        }
        fn f32s(&mut self, n: usize) -> Option<Vec<f32>> {
            let b = self.take(n.checked_mul(4)?)?;
            Some(b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
        }
        fn u32s(&mut self, n: usize) -> Option<Vec<u32>> {
            let b = self.take(n.checked_mul(4)?)?;
            Some(b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
        }
    }

    let mut c = Cur(bytes);
    let count = c.u32()? as usize;
    let mut nodes = Vec::new();
    for _ in 0..count {
        let name = c.str()?.unwrap_or_default();
        let texture = c.str()?;
        let vcount = c.u32()? as usize;
        let positions = c.f32s(vcount.checked_mul(3)?)?;
        let uvs = c.f32s(vcount.checked_mul(2)?)?;
        let icount = c.u32()? as usize;
        let indices = c.u32s(icount)?;
        let subs = c.u32()? as usize;
        let mut submeshes = Vec::new();
        for _ in 0..subs {
            let name = c.str()?.unwrap_or_default();
            let texture = c.str()?;
            let tri_start = c.u32()?;
            let tri_count = c.u32()?;
            submeshes.push(Submesh { name, tri_start, tri_count, texture, uv_tile: None, mat: None });
        }
        nodes.push(EdfNode {
            name,
            positions,
            uvs,
            normals: Vec::new(),
            indices,
            submeshes,
            texture,
            placed: true,
            materials: Vec::new(),
        });
    }
    c.0.is_empty().then_some(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A curved `n`×`n` sheet with a UV seam down the middle: the two halves share positions
    /// along the seam but not vertices, the way a real unwrap splits an island.
    fn seamed_sheet(n: usize) -> EdfNode {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();
        let half = n / 2;
        let mut grid = |c0: usize, c1: usize, u0: f32| {
            let first = (positions.len() / 3) as u32;
            let w = c1 - c0 + 1;
            for r in 0..=n {
                for c in c0..=c1 {
                    let (x, z) = (c as f32 / n as f32, r as f32 / n as f32);
                    positions.extend_from_slice(&[x, (x * 3.0).sin() * 0.2 + (z * 2.0).cos() * 0.1, z]);
                    uvs.extend_from_slice(&[u0 + (c - c0) as f32 / n as f32 * 0.5, z]);
                }
            }
            for r in 0..n as u32 {
                for c in 0..(w - 1) as u32 {
                    let a = first + r * w as u32 + c;
                    let b = a + w as u32;
                    indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
                }
            }
        };
        grid(0, half, 0.0);
        grid(half, n, 0.5);
        EdfNode {
            name: "shroud".into(),
            normals: vec![0.0; positions.len()],
            positions,
            uvs,
            indices,
            submeshes: Vec::new(),
            texture: Some("plastics.tga".into()),
            placed: true,
            materials: Vec::new(),
        }
    }

    fn two_sheets(n: usize) -> EdfNode {
        let mut node = seamed_sheet(n);
        let tris = (node.indices.len() / 3) as u32;
        node.submeshes = vec![
            Submesh { name: "a".into(), tri_start: 0, tri_count: tris / 2, texture: Some("plastics.tga".into()), uv_tile: Some(0), mat: None },
            Submesh { name: "b".into(), tri_start: tris / 2, tri_count: tris - tris / 2, texture: Some("frame decals.png".into()), uv_tile: Some(0), mat: None },
        ];
        node
    }

    /// The Designer's side of [`decode_nodes`], for the round trip.
    fn encode(nodes: &[EdfNode]) -> Vec<u8> {
        let mut b = Vec::new();
        let u = |b: &mut Vec<u8>, n: u32| b.extend_from_slice(&n.to_le_bytes());
        let s = |b: &mut Vec<u8>, v: &Option<String>| match v {
            Some(v) => {
                b.extend_from_slice(&(v.len() as u32).to_le_bytes());
                b.extend_from_slice(v.as_bytes());
            }
            None => b.extend_from_slice(&u32::MAX.to_le_bytes()),
        };
        u(&mut b, nodes.len() as u32);
        for n in nodes {
            s(&mut b, &Some(n.name.clone()));
            s(&mut b, &n.texture);
            u(&mut b, (n.positions.len() / 3) as u32);
            n.positions.iter().chain(&n.uvs).for_each(|f| b.extend_from_slice(&f.to_le_bytes()));
            u(&mut b, n.indices.len() as u32);
            n.indices.iter().for_each(|&i| u(&mut b, i));
            u(&mut b, n.submeshes.len() as u32);
            for sm in &n.submeshes {
                s(&mut b, &Some(sm.name.clone()));
                s(&mut b, &sm.texture);
                u(&mut b, sm.tri_start);
                u(&mut b, sm.tri_count);
            }
        }
        b
    }

    #[test]
    fn keeps_a_fraction_of_the_triangles() {
        let node = seamed_sheet(60);
        let before = node.indices.len() / 3;
        let proxy = build(&[node]);
        assert_eq!(proxy.source_triangles, before);
        let after = proxy.triangles();
        assert!(after > 0);
        assert!(after as f32 <= before as f32 * KEEP, "{after} of {before}");
    }

    #[test]
    fn every_uv_is_the_sources_own() {
        let node = seamed_sheet(40);
        let pairs: std::collections::HashSet<[u32; 5]> = (0..node.positions.len() / 3)
            .map(|v| {
                let q = |x: f32| ((x / QUANT).round() * QUANT).to_bits();
                [
                    q(node.positions[v * 3]),
                    q(node.positions[v * 3 + 1]),
                    q(node.positions[v * 3 + 2]),
                    node.uvs[v * 2].to_bits(),
                    node.uvs[v * 2 + 1].to_bits(),
                ]
            })
            .collect();
        let proxy = build(&[node]);
        let p = &proxy.parts[0];
        for v in 0..p.positions.len() / 3 {
            let key = [
                p.positions[v * 3].to_bits(),
                p.positions[v * 3 + 1].to_bits(),
                p.positions[v * 3 + 2].to_bits(),
                p.uvs[v * 2].to_bits(),
                p.uvs[v * 2 + 1].to_bits(),
            ];
            assert!(pairs.contains(&key), "vertex {v} isn't one of the source's");
        }
    }

    #[test]
    fn positions_sit_on_the_millimetre_grid() {
        let proxy = build(&[seamed_sheet(30)]);
        for &x in &proxy.parts[0].positions {
            let steps = x / QUANT;
            assert!((steps - steps.round()).abs() < 1e-2, "{x}");
        }
    }

    #[test]
    fn a_small_part_keeps_its_floor() {
        let node = seamed_sheet(4);
        let before = node.indices.len() / 3;
        let proxy = build(&[node]);
        assert!(proxy.triangles() > 0 && proxy.triangles() <= before);
    }

    #[test]
    fn obj_is_consistent_and_names_one_template_per_texture() {
        let proxy = build(&[two_sheets(20)]);
        let (obj, mtl) = to_obj(&proxy, "bike_proxy.mtl");
        let v = obj.lines().filter(|l| l.starts_with("v ")).count();
        let vt = obj.lines().filter(|l| l.starts_with("vt ")).count();
        assert_eq!(v, vt);
        for l in obj.lines().filter(|l| l.starts_with("f ")) {
            for corner in l[2..].split(' ') {
                let i: usize = corner.split('/').next().unwrap().parse().unwrap();
                assert!(i >= 1 && i <= v, "{l}");
            }
        }
        assert!(mtl.contains("map_Kd plastics_template.png"));
        assert!(mtl.contains("map_Kd frame_decals_template.png"));
        assert_eq!(obj.lines().filter(|l| l.starts_with("usemtl ")).count(), 2);
    }

    #[test]
    fn obj_writes_v_as_stored() {
        let proxy = build(&[seamed_sheet(10)]);
        let (obj, _) = to_obj(&proxy, "p.mtl");
        let first = obj.lines().find(|l| l.starts_with("vt ")).unwrap();
        let p = &proxy.parts[0];
        assert_eq!(first, format!("vt {} {}", p.uvs[0], p.uvs[1]));
    }

    #[test]
    fn nodes_round_trip_through_the_wire() {
        let nodes = vec![two_sheets(8), seamed_sheet(4)];
        let back = decode_nodes(&encode(&nodes)).expect("decodes");
        assert_eq!(back.len(), 2);
        for (a, b) in nodes.iter().zip(&back) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.texture, b.texture);
            assert_eq!(a.positions, b.positions);
            assert_eq!(a.uvs, b.uvs);
            assert_eq!(a.indices, b.indices);
            assert_eq!(a.submeshes.len(), b.submeshes.len());
            for (x, y) in a.submeshes.iter().zip(&b.submeshes) {
                assert_eq!((x.tri_start, x.tri_count, &x.texture), (y.tri_start, y.tri_count, &y.texture));
            }
        }
    }

    #[test]
    fn a_truncated_or_padded_payload_is_refused() {
        let bytes = encode(&[seamed_sheet(4)]);
        assert!(decode_nodes(&bytes[..bytes.len() - 1]).is_none());
        let mut long = bytes.clone();
        long.push(0);
        assert!(decode_nodes(&long).is_none());
        assert!(decode_nodes(&[0xff, 0xff, 0xff, 0x7f]).is_none());
    }

    #[test]
    fn to_nodes_keeps_one_submesh_per_texture() {
        let proxy = build(&[two_sheets(20)]);
        let nodes = proxy.to_nodes();
        let n = &nodes[0];
        assert_eq!(n.submeshes.len(), 2);
        let total: u32 = n.submeshes.iter().map(|s| s.tri_count).sum();
        assert_eq!(total as usize, n.indices.len() / 3);
        assert_eq!(n.submeshes[1].tri_start, n.submeshes[0].tri_count);
        assert_eq!(n.submeshes[0].texture.as_deref(), Some("plastics.tga"));
        assert_eq!(n.uvs.len() / 2, n.positions.len() / 3);
    }

    #[test]
    fn template_stems() {
        assert_eq!(template_stem("plastics.tga"), "plastics");
        assert_eq!(template_stem("my sheet:2.png"), "my_sheet_2");
        assert_eq!(template_stem(".hidden"), ".hidden");
        assert_eq!(template_stem("noext"), "noext");
    }

    /// A whole installed bike, down the path the Designer takes:
    /// `MXB_REAL_BIKE=<bike folder or .pkz> cargo test -p mxb-core real_bike -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_bike() {
        let path = std::env::var("MXB_REAL_BIKE").expect("MXB_REAL_BIKE");
        let model = crate::viewer::load_bike_model_blocking(path, None).expect("load bike");
        let own = &model.nodes[..model.nodes.len() - model.wheels];
        let proxy = build(own);
        let (obj, mtl) = to_obj(&proxy, "proxy.mtl");
        println!(
            "{} nodes ({} wheels), {} -> {} triangles",
            model.nodes.len(),
            model.wheels,
            proxy.source_triangles,
            proxy.triangles()
        );
        println!("{mtl}");
        assert!(proxy.triangles() > 0);
        assert!(mtl.contains("map_Kd"), "no textured material");
        if let Ok(out) = std::env::var("MXB_PROXY_OUT") {
            std::fs::write(format!("{out}/bike_proxy.obj"), obj).unwrap();
        }
    }

    /// Any model file, down the loose-file loader:
    /// `MXB_MODEL_FILE=<.edf, .pkz or folder> cargo test -p mxb-core real_model_file -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_model_file() {
        let path = std::env::var("MXB_MODEL_FILE").expect("MXB_MODEL_FILE");
        let model = crate::viewer::load_model_file_blocking(&path).expect("load model");
        let proxy = build(&model.nodes);
        let (obj, mtl) = to_obj(&proxy, "proxy.mtl");
        if let Ok(out) = std::env::var("MXB_PROXY_OUT") {
            std::fs::write(format!("{out}/file_proxy.obj"), obj).unwrap();
        }
        println!(
            "{} nodes, {} textures, {} -> {} triangles",
            model.nodes.len(),
            model.textures.len(),
            proxy.source_triangles,
            proxy.triangles()
        );
        println!("{mtl}");
        assert!(proxy.triangles() > 0);
    }
}
