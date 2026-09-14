//! A painting proxy: a bike's mesh cut down to a stand-in a painter can work on in Blender or
//! Substance, without the model itself ever leaving the creator.
//!
//! The UV layout is the one thing kept exactly — a paint made on the proxy has to land on the
//! real bike — and everything that makes a mesh worth taking is dropped: most of the triangles,
//! the normals, the LODs (the viewer only reads level0), and sub-millimetre position detail.
//!
//! The ratio is a constant on purpose. A slider would let anyone set it back to 100%.

use crate::edf::EdfNode;

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
/// per texture. `v` of the UV is flipped: OBJ counts it up from the bottom of the image, the
/// game down from the top.
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
            let _ = writeln!(obj, "vt {} {}", t[0], 1.0 - t[1]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edf::Submesh;

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
        let mut node = seamed_sheet(20);
        let tris = (node.indices.len() / 3) as u32;
        node.submeshes = vec![
            Submesh { name: "a".into(), tri_start: 0, tri_count: tris / 2, texture: Some("plastics.tga".into()), uv_tile: Some(0), mat: None },
            Submesh { name: "b".into(), tri_start: tris / 2, tri_count: tris - tris / 2, texture: Some("frame decals.png".into()), uv_tile: Some(0), mat: None },
        ];
        let proxy = build(&[node]);
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
    fn template_stems() {
        assert_eq!(template_stem("plastics.tga"), "plastics");
        assert_eq!(template_stem("my sheet:2.png"), "my_sheet_2");
        assert_eq!(template_stem(".hidden"), ".hidden");
        assert_eq!(template_stem("noext"), "noext");
    }

    /// A whole installed bike, down the path the Studio's export takes:
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

    /// A real bike part: `MXB_EDF=<model.edf plaintext> cargo test -p mxb-core real_edf -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_edf() {
        let path = std::env::var("MXB_EDF").expect("MXB_EDF");
        let bytes = std::fs::read(path).unwrap();
        let proxy = build(&crate::edf::parse(&bytes));
        println!("{} -> {} triangles", proxy.source_triangles, proxy.triangles());
        assert!(proxy.triangles() > 0);
        // `MXB_PROXY_OUT=<dir>` keeps the result, to look at.
        if let Ok(out) = std::env::var("MXB_PROXY_OUT") {
            let (obj, mtl) = to_obj(&proxy, "proxy.mtl");
            std::fs::write(format!("{out}/proxy.obj"), obj).unwrap();
            std::fs::write(format!("{out}/proxy.mtl"), mtl).unwrap();
        }
    }
}
