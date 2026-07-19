//! Geometry extractor for MX Bikes **`.edf`** meshes (PiBoSo's compiled model
//! format), so the 3D viewer can render the real bike model. Reverse-engineered
//! from real bike/shadow meshes and validated by reconstructing clean geometry
//! (sane vertex/triangle counts + bounding box).
//!
//! Layout: `"EDF\0"` magic, a global AABB, then node geometry blocks. Each node
//! is **structure-of-arrays** with a 72-byte-per-vertex footprint:
//!
//! ```text
//! u32          vcount
//! f32[3]×vc    position   (local space)   @ vs + vc*0
//! f32[2]×vc    uv0                        @ vs + vc*12   (stride 8, ONE set)
//!              (24 bytes/vert unidentified: colour and/or uv1)
//! f32[3]×vc    normal                     @ vs + vc*44
//!              (16 bytes/vert unidentified: likely tangent xyz + sign)
//! u32          tri_count                  @ ic
//! u32[3]×tc    indices  (triangle list)   @ ic + 4
//! u32          submesh_count              @ ic + 4 + tc*12
//! char[]       node name                  @ ic + 8 + tc*12   (the anchor)
//! ```
//!
//! **Everything is a plain triangle list** — bikes, gear and the rider body alike.
//! A correct read yields exactly `tc` triangles with ZERO degenerates.
//!
//! There is no flag word between `tri_count` and the indices. The zero long read
//! there is `idx0`, which is 0 because every node's first triangle is `(0,1,2)`.
//! Reading indices from `ic+8` therefore looked correct — skipping `idx0` at the
//! front and consuming `submesh_count` at the back still landed the name anchor
//! exactly, so the block validated while every triangle was built from a shifted
//! window. That produced a scrambled-but-plausible surface, and in turn a strip
//! decoder, a degenerate-ratio heuristic and a UV-span streak filter that all
//! existed only to compensate for it. All three are gone.
//!
//! Blocks are found by validating each candidate fully (finite positions, in-range
//! indices, exact end alignment), resyncing one byte at a time otherwise.
//!
//! **Placement is per-submesh.** Vertices are authored in each submesh's own
//! space; the rigid 4×4 at `submesh_block - 148` (plus, for instanced sub-objects
//! like the bar-end grips, a parent 280 bytes further back) composed with the
//! node's orientation matrix at `name+104` puts a part into its `.geom` LOCAL
//! frame. [`assemble_bike`] then hangs the parts off the chassis using the bike's
//! `.geom` mount points. Proof: the placed chassis reproduces the header AABB.
//!
//! Submesh records are collected once for the whole file ([`collect_sub_cands`]) and
//! each node chains its own table out of that pool ([`detect_submeshes`]), because a
//! node's records are not always packed after its name — the Suzuki chassis' six
//! straddle a ~5 MB embedded-texture blob. A node that can't be reconciled falls back
//! to the historical bounded-window scan and renders unassembled rather than mangled.

use serde::Serialize;

const STRIDE: usize = 72;
const HEADER_START: usize = 0x54;
const MAX_COUNT: usize = 3_000_000;

/// A contiguous run of the node's kept triangles sharing one material/mesh group.
/// `tri_start`/`tri_count` index the KEPT triangle list, so a viewer can do
/// `geometry.addGroup(tri_start*3, tri_count*3, materialIndex)`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Submesh {
    /// Mesh-group name from the `.edf` (e.g. `frame.005`, `SHOCK250F`, `chain`).
    /// Textures bind to this by name via `gfx.cfg` / the paint.
    pub name: String,
    pub tri_start: u32,
    pub tri_count: u32,
    /// Name of the texture this group binds to, resolved in Rust (see
    /// [`crate::bind_textures`]) so the frontend never re-guesses from the name.
    /// `None` → no texture resolved; render it in a neutral colour.
    pub texture: Option<String>,
    /// Integer part of this group's `u`, i.e. which **UV tile** it samples. Nearly
    /// every group sits on tile 0; the Honda's exhaust is authored wholly on tile 1
    /// (u ∈ [1.001, 2.000]), which selects a *different* texture — sampled at
    /// `u - tile`. Verified by rendering: the exhaust reads as clean brushed metal
    /// from the model's `exhaust_22`, versus a smear of body graphics on tile 0.
    /// `None` when the group straddles tiles (small UV-wrap islands like `plate`).
    pub uv_tile: Option<i32>,
    /// **Material index** — the u32 stored immediately before this submesh's geometry
    /// block (`block_off - 4`). It indexes the model's **colour** textures in FILE order
    /// (normal/roughness maps skipped), which is exactly how the game assigns a texture
    /// to a part. Validated across Honda/KTM/Yamaha/Suzuki/TM: it reproduces the
    /// plate→`w_plate` and exhaust bindings that previously needed `gfx.cfg` + UV-tile
    /// hacks, AND splits the KTM's metals (`450f_metals`) from its plastics — which no
    /// heuristic could. [`crate::bind_textures`] resolves it to a texture name; `None`
    /// when the record has no room for it.
    pub mat: Option<u32>,
}

/// One decoded mesh node, ready to become a three.js `BufferGeometry`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdfNode {
    /// Node name from the trailing metadata (empty until that's wired).
    pub name: String,
    /// `3 * vcount` — vertex positions (local space).
    pub positions: Vec<f32>,
    /// `2 * vcount` — uv0 per vertex (empty if none).
    pub uvs: Vec<f32>,
    /// `3 * vcount` — normals per vertex (empty if none).
    pub normals: Vec<f32>,
    /// `3 * (kept triangles)` — u32 indices. A correct decode drops nothing; the
    /// degenerate check is kept only to guard a malformed export.
    pub indices: Vec<u32>,
    /// Material groups over the kept triangle list (empty if not resolved).
    pub submeshes: Vec<Submesh>,
    /// Texture for the node as a whole — used when `submeshes` is empty, i.e. the
    /// submesh table didn't resolve (see `detect_submeshes`) and there are no groups
    /// to bind individually. Resolved in Rust; `None` → neutral colour.
    pub texture: Option<String>,
    /// Whether the placement chain ran, i.e. `positions` are in the part's `.geom`
    /// LOCAL frame rather than raw authored space. False when the submesh table
    /// didn't cover every vertex (a second record layout we don't enumerate yet).
    /// [`assemble_bike`] must not hang an unplaced part off the chassis — it has no
    /// frame to hang from, so the `.geom` offsets fling it across the scene.
    #[serde(skip)]
    pub placed: bool,
}

/// Parse a bike's `.geom` (plain-text `key = x, y, z` physics config) into a map
/// of mount points, e.g. `chassis_steer`, `chassis_rsusp_min`, `steer_joint`,
/// `rsusp_joint`, `front_upper`, `fwheel`. Used to assemble the articulated parts
/// (fork/suspension/steering) onto the chassis. Ignores non-vector lines.
pub fn parse_geom(bytes: &[u8]) -> std::collections::HashMap<String, [f32; 3]> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim(); // strip comments
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let nums: Vec<f32> = val
            .split(',')
            .filter_map(|s| s.trim().parse::<f32>().ok())
            .collect();
        if nums.len() == 3 {
            out.insert(key.trim().to_ascii_lowercase(), [nums[0], nums[1], nums[2]]);
        }
    }
    out
}

/// Parse the `.geom`'s single-value keys (e.g. `rakeangle_min = 27.1`), which
/// [`parse_geom`] skips because it only keeps 3-vectors.
pub fn parse_geom_scalars(bytes: &[u8]) -> std::collections::HashMap<String, f32> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let nums: Vec<f32> = val
            .split(',')
            .filter_map(|s| s.trim().parse::<f32>().ok())
            .collect();
        if nums.len() == 1 {
            out.insert(key.trim().to_ascii_lowercase(), nums[0]);
        }
    }
    out
}

/// A submesh as found in the node's trailing metadata, before its triangles are
/// filtered and remapped into an [`EdfNode`]'s kept list.
struct RawSub {
    name: String,
    tri_start: usize,
    tri_count: usize,
    /// Offset of the six-u32 geometry block — anchors the placement matrix lookup.
    block_off: usize,
    vert_start: usize,
    vert_count: usize,
    /// Material index, when this sub is one RANGE of a single skinned group (the
    /// rider body: rider/gloves/face split across one group's contiguous ranges).
    /// `None` for the usual case, where the material id is read from `block_off - 4`.
    mat: Option<u32>,
}

/// A rigid 4×4 placement matrix: row-major, translation in the 4th **column**.
type Mat4 = [f32; 16];

/// Read a placement matrix at `o`, returning `None` unless it's a genuine rigid
/// transform. The node's trailing metadata is dense binary, so this validation is
/// what makes offset-based matrix lookup safe: bottom row exactly `[0,0,0,1]`,
/// orthonormal rows, and |det| == 1.
fn read_mat4(b: &[u8], o: usize) -> Option<Mat4> {
    if o + 64 > b.len() {
        return None;
    }
    let mut m = [0f32; 16];
    for (i, slot) in m.iter_mut().enumerate() {
        *slot = f32le(b, o + i * 4);
    }
    if !m.iter().all(|v| v.is_finite() && v.abs() < 10.0) {
        return None;
    }
    if m[12] != 0.0 || m[13] != 0.0 || m[14] != 0.0 || m[15] != 1.0 {
        return None;
    }
    let r = [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]];
    if r.iter().any(|row| (v_dot(*row, *row) - 1.0).abs() > 1e-3) {
        return None;
    }
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    if (det.abs() - 1.0).abs() > 1e-3 {
        return None;
    }
    Some(m)
}

/// Apply a [`Mat4`] to a point (uses the translation column).
fn mat_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
    ]
}

/// Apply only a [`Mat4`]'s rotation — for normals, which must not be translated.
fn mat_dir(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2],
    ]
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// A vertex position sample is plausible: finite and within a sane magnitude.
fn finite_pos(b: &[u8], o: usize) -> bool {
    (0..3).all(|k| {
        let v = f32le(b, o + 4 * k);
        v.is_finite() && v.abs() < 200.0
    })
}

/// Parse an `.edf` into its renderable mesh nodes (highest-detail LOD of each
/// part). Returns an empty vec if none decode.
///
/// A `.edf` is a multi-scene container: node0's geometry, then (for a bike) a
/// large embedded JPEG-texture blob, then the remaining part nodes back-to-back,
/// then a scene/LOD table. Nodes are enumerated by their **name anchor** — a
/// node's index buffer ends exactly on its name string — which steps cleanly over
/// the texture blob that would otherwise derail a positional scan.
pub fn parse(b: &[u8]) -> Vec<EdfNode> {
    parse_impl(b, &[])
}

/// Parse a bike, keeping exactly the nodes the bike's **`.hrc`** files declare as
/// `level0`. Prefer this over [`parse`]: a `.hrc` *states* the LOD lineup, so
/// there is nothing to infer.
///
/// `level0` is the empty slice → falls back to [`level0_only`]'s name heuristic
/// (for a loose `.edf` with no configs alongside it).
pub fn parse_with_levels(b: &[u8], level0: &[String]) -> Vec<EdfNode> {
    parse_impl(b, level0)
}

fn parse_impl(b: &[u8], level0: &[String]) -> Vec<EdfNode> {
    let n = b.len();
    if n < HEADER_START + 8 || &b[0..4] != b"EDF\0" {
        return Vec::new();
    }
    let mut nodes = Vec::new();
    // Submesh geometry blocks, collected ONCE for the whole file: a node's records
    // are not always within a bounded window of its name (the Suzuki chassis' six
    // straddle a ~5 MB embedded-texture blob), so each node chains its table against
    // this shared pool rather than re-scanning its own neighbourhood. Cheap: it is a
    // single linear pass keyed on the rare rigid-matrix bottom row.
    let cands = collect_sub_cands(b);
    let mut o = HEADER_START;

    while o + 8 <= n {
        let vc = u32le(b, o) as usize;
        // A candidate is accepted only if the whole block validates AND its index
        // buffer ends exactly on a plausible node name (the anchor).
        if (8..=MAX_COUNT).contains(&vc) && o + 4 + vc * STRIDE + 8 <= n {
            let vs = o + 4;
            let samples = [0usize, 1, 2, vc / 2, vc - 1];
            if samples.iter().all(|&i| finite_pos(b, vs + i * 12)) {
                let ic = vs + vc * STRIDE;
                let tc = u32le(b, ic) as usize;
                if (1..=MAX_COUNT).contains(&tc) && ic + 8 + tc * 12 <= n {
                    // Index block layout:
                    //   [tc][ tc*3 u32 indices ][u32 submesh_count][name…]
                    //        ^ ic+4              ^ ic+4+tc*12       ^ iend
                    //
                    // The indices start at ic+4 — immediately after the count. This
                    // was long read as ic+8, on the belief that ic+4 was a zero flag
                    // word. It is not: it is idx0, and it reads 0 because the first
                    // triangle of every node is (0,1,2). That off-by-one validated
                    // itself perfectly — skipping idx0 at the front and swallowing
                    // the trailing submesh_count at the back left `iend` landing
                    // exactly on the name anchor, so the block still "checked out"
                    // while every triangle was built from a shifted index window.
                    // The result was a connected-looking but scrambled surface, which
                    // is what the strip decoder, the degenerate-ratio heuristic and
                    // the UV-span streak filter all existed to paper over.
                    let idx_off = ic + 4;
                    let mut ok = true;
                    let mut raw = Vec::with_capacity(tc * 3);
                    for t in 0..tc * 3 {
                        let i = u32le(b, idx_off + t * 4);
                        if i as usize >= vc {
                            ok = false;
                            break;
                        }
                        raw.push(i);
                    }
                    // Name anchor: past the indices AND the trailing submesh_count.
                    let iend = ic + 8 + tc * 12;
                    if let (true, Some(name)) = (ok, plausible_name(b, iend)) {
                        nodes.push(read_node(b, &cands, vs, vc, raw, iend, tc, name));
                        o = iend; // jump past this block
                        continue;
                    }
                }
            }
        }
        // Resync one byte at a time: nodes after the embedded JPEG-texture blob
        // land at unaligned offsets, so a 4-step scan would miss them.
        o += 1;
    }
    if level0.is_empty() {
        return level0_only(nodes);
    }
    // The `.hrc` named the full-detail nodes outright — keep exactly those. A node
    // the `.hrc` lineup never mentions (e.g. a loose sub-scene) is dropped: every
    // renderable part of a bike is reachable from `gfx.cfg` → `.hrc` → `level0`.
    let want: std::collections::HashSet<String> =
        level0.iter().map(|n| n.to_ascii_lowercase()).collect();
    if !nodes.iter().any(|n| want.contains(&n.name.to_ascii_lowercase())) {
        // The `.hrc` lineup disagrees with the mesh (a half-applied model swap, say).
        // Better a heuristically-picked bike than no bike.
        log::warn!("edf: .hrc level0 {level0:?} matched no node — using the name heuristic");
        return level0_only(nodes);
    }
    nodes
        .into_iter()
        .filter(|n| want.contains(&n.name.to_ascii_lowercase()))
        .collect()
}

fn v_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn v_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn v_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Rotate about X by `deg` (design frame: +Y up, +Z forward).
fn rot_x(p: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

/// Assemble a bike's parts onto its chassis using the `.geom` mount points, then
/// centre the result on the origin for the viewer.
///
/// [`parse`] leaves each part in its own `.geom` LOCAL frame; this hangs them off
/// the chassis (which is already design space). Each articulated part rotates about
/// its own joint and lands on the chassis' matching mount:
/// `rsusp` pivots at `rsusp_joint`→`chassis_rsusp_min`, `steer` rakes back by
/// `rakeangle_min` about `steer_joint`→`chassis_steer`, and `fsusp` (the fork
/// lowers) hangs off the steering axis at `front_upper`, i.e. fully extended —
/// the bike is posed on its stand, with no rider sag.
///
/// Returns false (leaving `nodes` untouched) if the `.geom` lacks the mounts.
pub fn assemble_bike(nodes: &mut [EdfNode], geom_bytes: &[u8]) -> bool {
    let g = parse_geom(geom_bytes);
    let sc = parse_geom_scalars(geom_bytes);
    let (Some(&head), Some(&pivot), Some(&steer_joint), Some(&rsusp_joint), Some(&front_upper)) = (
        g.get("chassis_steer"),
        g.get("chassis_rsusp_min"),
        g.get("steer_joint"),
        g.get("rsusp_joint"),
        g.get("front_upper"),
    ) else {
        return false;
    };
    // Rake tilts the steering head back, i.e. toward -Z (front is +Z).
    let rake = -sc.get("rakeangle_min").copied().unwrap_or(0.0);
    let place_steer = |p: [f32; 3]| v_add(rot_x(v_sub(p, steer_joint), rake), head);
    let fork_origin = place_steer(front_upper);

    for n in nodes.iter_mut() {
        // An unplaced part is still in raw authored space, so the `.geom` mounts
        // don't apply — offsetting it just throws it across the scene (the KTM 450's
        // fork and steering floating off on their own).
        if !n.placed {
            continue;
        }
        // Match the part by prefix: names carry a displacement tag (`chassis450f`).
        let name = n.name.to_ascii_lowercase();
        let (rot, off) = if name.starts_with("chassis") {
            continue; // root body: already in design space
        } else if name.starts_with("rsusp") {
            (0.0, v_sub(pivot, rsusp_joint))
        } else if name.starts_with("steer") {
            (rake, v_sub(head, rot_x(steer_joint, rake)))
        } else if name.starts_with("fsusp") {
            (rake, fork_origin)
        } else {
            continue;
        };
        for p in n.positions.chunks_exact_mut(3) {
            let v = v_add(rot_x([p[0], p[1], p[2]], rot), off);
            p.copy_from_slice(&v);
        }
        for d in n.normals.chunks_exact_mut(3) {
            let v = rot_x([d[0], d[1], d[2]], rot);
            d.copy_from_slice(&v);
        }
    }

    // Design space puts y=0 at the ground, so the assembled bike sits well off the
    // origin. Centre it, since the viewer orbits [0,0,0].
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for n in nodes.iter() {
        for p in n.positions.chunks_exact(3) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    if lo[0] > hi[0] {
        return true;
    }
    let c = [
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    ];
    for n in nodes.iter_mut() {
        for p in n.positions.chunks_exact_mut(3) {
            for k in 0..3 {
                p[k] -= c[k];
            }
        }
    }
    true
}

/// A node name at `o`: a printable run of length 2–31 starting with a letter and
/// made of name-safe characters. `None` (→ not a node boundary) otherwise; this
/// is what keeps the JPEG-texture blob from producing false node matches.
fn plausible_name(b: &[u8], o: usize) -> Option<String> {
    if o >= b.len() || !b[o].is_ascii_alphabetic() {
        return None;
    }
    let mut e = o;
    while e < b.len() && e - o < 32 {
        let c = b[e];
        if c == 0 {
            break;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-')) {
            return None;
        }
        e += 1;
    }
    let len = e - o;
    if (2..=31).contains(&len) {
        Some(String::from_utf8_lossy(&b[o..e]).into_owned())
    } else {
        None
    }
}



/// `steer`). Group by that base and keep the node with the most vertices (level0).
fn level0_only(nodes: Vec<EdfNode>) -> Vec<EdfNode> {
    use std::collections::HashMap;
    let base = |name: &str| -> String {
        let bytes = name.as_bytes();
        match bytes.iter().position(|c| c.is_ascii_digit()) {
            // Tagged name: strip a `b`/`c` immediately before the first digit.
            Some(d) if d >= 1 && (bytes[d - 1] == b'b' || bytes[d - 1] == b'c') => {
                let mut s = name.to_string();
                s.remove(d - 1);
                s
            }
            // Untagged name: strip a trailing `b`/`c` LOD suffix.
            None if bytes.len() > 1 && matches!(bytes[bytes.len() - 1], b'b' | b'c') => {
                name[..name.len() - 1].to_string()
            }
            _ => name.to_string(),
        }
    };
    // Level0 is the node whose name IS the base (`chassis450f`); the LOD variants
    // carry the `b`/`c` tag (`chassisb450f`). Prefer that exact-name match, and only
    // fall back to "most triangles" when no untagged node exists. Ranking purely by
    // triangle count is fragile — it made the KTM 450 flip to its LOD-B chassis
    // (which has no submesh table, so it can't be placed) on a small filter change.
    let mut best: HashMap<String, usize> = HashMap::new();
    for (i, nd) in nodes.iter().enumerate() {
        let k = base(&nd.name);
        let is_level0 = nd.name == k;
        let better = match best.get(&k) {
            None => true,
            Some(&j) => {
                let prev_level0 = nodes[j].name == k;
                match (is_level0, prev_level0) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => nd.indices.len() > nodes[j].indices.len(),
                }
            }
        };
        if better {
            best.insert(k, i);
        }
    }
    let keep: std::collections::HashSet<usize> = best.into_values().collect();
    nodes
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep.contains(i))
        .map(|(_, nd)| nd)
        .collect()
}

/// Which UV tile a submesh's vertices sit on: `floor(u)`, if every vertex in
/// `[vert_start, vert_start+vert_count)` agrees. `None` when they straddle tiles.
///
/// Nearly every group is tile 0. The Honda's exhaust (`Cylinder.003`) is authored
/// wholly on tile 1 — u ∈ [1.001, 2.000], a clean 0–1 range shifted by exactly +1 —
/// which selects the node's *second* texture. Small groups like `plate` straddle
/// tiles -1/0/1 in a handful of vertices; that's UV **wrap** on a repeating island,
/// not tiling, so they report `None` and fall through to the normal binding.
fn uv_tile(uvs: &[f32], vert_start: usize, vert_count: usize) -> Option<i32> {
    let hi = (vert_start + vert_count).min(uvs.len() / 2);
    if vert_start >= hi {
        return None;
    }
    let mut tile: Option<i32> = None;
    for i in vert_start..hi {
        let t = uvs[i * 2].floor() as i32;
        match tile {
            None => tile = Some(t),
            Some(prev) if prev != t => return None,
            _ => {}
        }
    }
    tile
}

/// One texture packed inside a `model.edf`, as the model itself names it.
///
/// This is the pool a bike's textures actually bind to. The names are the model
/// author's (`2021crf`, `exhaust_22`, `w_plate` on the Honda; `plastics`,
/// `450f_metals` on the KTM 450) — **not** a convention. A `.pnt` paint supplies
/// textures by name too, and a paint texture replaces the embedded one of the SAME
/// name; that is the whole mechanism by which a paint changes a bike, and it's why
/// the KTM/TM models literally embed a texture called `plastics` while the Honda
/// does not.
#[derive(Debug, Clone)]
pub struct EmbeddedTexture {
    /// The model's own name for it, e.g. `2021crf` / `plastics` / `w_plate`.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Byte offset of the raw-DEFLATE RGBA payload.
    pub data_off: usize,
    /// Compressed byte length.
    pub data_len: usize,
}

/// Byte layout of an embedded texture record, relative to its `width` field:
/// `… | width u32 | height u32 | md5[16] | u32 | data_size u32 | pad[8] | data`.
/// `data_size` counts the 8 padding bytes, so the payload is `data_size - 8` long.
/// (Identical to a `.pnt`'s image record bar one extra u32.)
const TEX_SIZE_FROM_W: usize = 28;
const TEX_PAD_FROM_W: usize = 32;
const TEX_DATA_FROM_W: usize = 40;
const TEX_PAD_LEN: usize = 8;
/// Distance from the name's first character to `width`. The name lives in a
/// fixed-size field, but it starts at **either** `width - 100` or `width - 104`
/// depending on the record — both occur inside a single Honda `model.edf`
/// (`2021crf` and `w_plate` are the former, `2023crf_n` the latter). Rather than
/// pick one and hope, probe both and keep whichever validates; the wrong offset
/// reads `height` (or a md5 byte) as `width` and fails the checks below.
const TEX_W_FROM_NAME: [usize; 2] = [100, 104];

/// A null-terminated embedded-texture name at `o`: 2–39 name-safe characters.
/// Unlike [`plausible_name`] (node names) these may lead with a digit.
fn tex_name(b: &[u8], o: usize) -> Option<String> {
    let mut e = o;
    while e < b.len() && e - o < 40 {
        let c = b[e];
        if c == 0 {
            break;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-')) {
            return None;
        }
        e += 1;
    }
    let len = e - o;
    (2..=39).contains(&len).then(|| String::from_utf8_lossy(&b[o..e]).into_owned())
}

/// Enumerate every texture packed into a `model.edf`, in file order.
///
/// Anchored on the texture's **name**, then validated by shape: power-of-two
/// dimensions, eight zero bytes of padding, and a payload that fits the file.
/// Specific enough to walk a 70 MB mesh cleanly — the Honda yields exactly its
/// `2021crf / 2023crf_n / w_plate / exhaust_22 / exhaust_23_n`, the KTM 450 its
/// `450f_metals / plastics / plastics_n / w_plate`.
///
/// The `_r` (roughness) maps are skipped: they're stored in a compression this
/// doesn't decode, and the viewer has no use for them.
pub fn embedded_textures(b: &[u8]) -> Vec<EmbeddedTexture> {
    const SIZES: [u32; 7] = [64, 128, 256, 512, 1024, 2048, 4096];
    let mut out = Vec::new();
    let mut o = 0usize;
    'scan: while o + TEX_W_FROM_NAME[1] + TEX_DATA_FROM_W <= b.len() {
        // A name starts a record only where it starts a *word*: this is what stops
        // `2021crf` from also matching at its own `crf` (whose w/h would then be
        // read 4 bytes late). Texture names may lead with a digit — `2021crf`,
        // `450f_metals` — so this can't reuse `plausible_name`'s letter-first rule.
        if !b[o].is_ascii_alphanumeric() || (o > 0 && b[o - 1].is_ascii_alphanumeric()) {
            o += 1;
            continue;
        }
        let Some(name) = tex_name(b, o) else {
            o += 1;
            continue;
        };
        for w_off in TEX_W_FROM_NAME {
            if name.len() >= w_off {
                continue; // the name must terminate inside its own field
            }
            let w_at = o + w_off;
            let (w, h) = (u32le(b, w_at), u32le(b, w_at + 4));
            let size = u32le(b, w_at + TEX_SIZE_FROM_W) as usize;
            let pad = w_at + TEX_PAD_FROM_W;
            if !SIZES.contains(&w) || !SIZES.contains(&h) || size <= TEX_PAD_LEN {
                continue;
            }
            let (data_off, data_len) = (w_at + TEX_DATA_FROM_W, size - TEX_PAD_LEN);
            if pad + TEX_PAD_LEN > b.len()
                || b[pad..pad + TEX_PAD_LEN] != [0u8; TEX_PAD_LEN]
                || data_off + data_len > b.len()
            {
                continue;
            }
            out.push(EmbeddedTexture {
                name,
                width: w,
                height: h,
                data_off,
                data_len,
            });
            o = data_off + data_len; // records don't overlap — skip the payload
            continue 'scan;
        }
        o += 1;
    }
    out
}

/// Inflate an embedded texture to RGBA8 (`width * height * 4` bytes), or `None` if
/// it doesn't decode (the `_r` roughness maps use a format we don't read).
pub fn inflate_texture(b: &[u8], t: &EmbeddedTexture) -> Option<Vec<u8>> {
    use std::io::Read;
    let expected = (t.width as usize) * (t.height as usize) * 4;
    let mut buf = Vec::with_capacity(expected);
    flate2::read::DeflateDecoder::new(&b[t.data_off..t.data_off + t.data_len])
        .read_to_end(&mut buf)
        .ok()?;
    (buf.len() >= expected).then(|| {
        buf.truncate(expected);
        buf
    })
}

/// Extract a validated block's attribute arrays (dropping degenerate triangles)
/// and its submesh groups (remapped to the kept-triangle list).
fn read_node(
    b: &[u8],
    cands: &[SubCand],
    vs: usize,
    vc: usize,
    raw_idx: Vec<u32>,
    iend: usize,
    raw_tris: usize,
    name: String,
) -> EdfNode {
    // Positions: contiguous vc*3 f32.
    let mut positions = Vec::with_capacity(vc * 3);
    for i in 0..vc * 3 {
        positions.push(f32le(b, vs + i * 4));
    }
    // SoA sub-arrays: positions(3f) | **uv(2f)** | … | normal(3f). The `uv` block
    // is a SINGLE 2-float set → **stride 8**. (It was previously read as two sets
    // at stride 16, which samples every OTHER vertex's UV and scrambles the paint
    // across the whole model.)
    let uv_base = vs + vc * 12;
    let normal_base = vs + vc * 44;
    let mut uvs = Vec::with_capacity(vc * 2);
    let mut normals = Vec::with_capacity(vc * 3);
    for i in 0..vc {
        uvs.push(f32le(b, uv_base + i * 8));
        uvs.push(f32le(b, uv_base + i * 8 + 4));
        normals.push(f32le(b, normal_base + i * 12));
        normals.push(f32le(b, normal_base + i * 12 + 4));
        normals.push(f32le(b, normal_base + i * 12 + 8));
    }


    // Every node is a plain triangle **list** — bikes, gear and the rider body
    // alike. There is no strip encoding in this format; the "strips" were an
    // artifact of reading indices from ic+8 instead of ic+4 (see `parse_impl`).
    // A correct read yields EXACTLY `tc` triangles with ZERO degenerate triples,
    // which is what proved the offset: stock helmet 4120/4120, TLD SE4 6318/6318.

    // Raw submesh table (each entry a raw tri range) from the trailing metadata.
    let mut raw_subs = detect_submeshes(b, cands, iend, raw_tris, vc);
    // A **skinned mesh** (the rider body) is a SINGLE group covering the whole node,
    // whose internal contiguous ranges are distinct MATERIALS (rider/gloves/face/…),
    // not vertex segments of one material. `read_sub_group` merges those ranges (right
    // for a bike part), collapsing them to one submesh. Split them back out — carrying
    // each range's material index — so each material can bind its own texture (this is
    // what lets glove paint land on the hands). Bike nodes are covered by *multiple*
    // groups, or single-range groups, so they never hit this and stay untouched.
    if raw_subs.len() == 1 && raw_subs[0].tri_count == raw_tris {
        if let Some(ranges) = read_sub_group_ranges(b, raw_subs[0].block_off, raw_tris, vc) {
            if ranges.len() > 1 {
                let (name, block_off) = (raw_subs[0].name.clone(), raw_subs[0].block_off);
                raw_subs = ranges
                    .into_iter()
                    .enumerate()
                    .map(|(i, (ts, tc, vs2, vc2))| RawSub {
                        name: name.clone(),
                        tri_start: ts,
                        tri_count: tc,
                        block_off,
                        vert_start: vs2,
                        vert_count: vc2,
                        mat: Some(i as u32),
                    })
                    .collect();
            }
        }
    }
    // Tiles the node when the submesh triangle COUNTS sum to the raw total.
    let covers = !raw_subs.is_empty() && raw_subs.iter().map(|s| s.tri_count).sum::<usize>() == raw_tris;

    // Place the geometry. Vertices are authored per submesh in their own space, so
    // each submesh carries its own rigid transform; composed with the node's
    // orientation matrix this yields the part's `.geom` LOCAL frame (design space
    // for `chassis`, the root body). Without this, parts land scattered — the
    // exhaust sinks below ground and the fork/steering float away from the bike.
    //
    // A vertex the submesh table doesn't list can't be placed — it stays in raw
    // authored space, in a different frame from its neighbours. Mixing the two
    // stretches triangles across the model, so `placed_vert` tracks coverage and
    // any triangle touching an unplaced vertex is dropped below. Tables are usually
    // complete; some bikes use a second record layout we don't enumerate yet and
    // come up a little short (KTM 450: 85 of 21126 triangles).
    //
    // `iend` is the node's name offset (the index buffer ends exactly there).
    let mut placed_vert = vec![false; vc];
    let mut placed = false;
    // Dev switch (`MXB_NO_PLACE=1`) to render raw authored space — used to tell a
    // placement bug apart from a decode bug.
    let skip_place = std::env::var_os("MXB_NO_PLACE").is_some();
    if let (false, false, Some(node_mat)) = (
        skip_place,
        raw_subs.is_empty(),
        read_mat4(b, iend + NODE_MAT_OFF),
    ) {
        placed = true;
        for s in &raw_subs {
            let chain = submesh_transform(b, iend, s.block_off);
            let hi_v = (s.vert_start + s.vert_count).min(vc);
            for i in s.vert_start..hi_v {
                placed_vert[i] = true;
                let (p, n) = (i * 3, i * 3);
                let mut pos = [positions[p], positions[p + 1], positions[p + 2]];
                let mut nrm = [normals[n], normals[n + 1], normals[n + 2]];
                for m in &chain {
                    pos = mat_point(m, pos);
                    nrm = mat_dir(m, nrm);
                }
                pos = mat_point(&node_mat, pos);
                nrm = mat_dir(&node_mat, nrm);
                positions[p..p + 3].copy_from_slice(&pos);
                normals[n..n + 3].copy_from_slice(&nrm);
            }
        }
    }

    // Drop only collapsed (degenerate) triangles.
    //
    // There used to be an extra "long AND thin" sliver rule here, to kill the
    // "strings" seen running through a bike. Those were never slivers: they are the
    // FENDERS — long, flat panel triangles with tightly-clustered indices and ~0.16m
    // edges. They only looked like strings because the parts were mis-placed and
    // overlapping; per-submesh placement fixed that and made the rule pure damage.
    // It dropped just ~4% of triangles, but they were the *large panel* ones, so it
    // punched exactly the holes that made the bodywork look shredded (proven by
    // rendering with and without: identical geometry, solid vs. shot full of holes).
    // A correct decode has no degenerates, but keep the collapse check: it costs
    // nothing and keeps a malformed export from emitting zero-area triangles.
    //
    // The UV-span streak filter that used to live here is gone. The "lines" it
    // chased were triangles built across a shifted index window, smearing the
    // atlas — a symptom of the ic+8 read, not a property of the content. With the
    // indices read correctly there are no streaks to filter, and the rule was
    // deleting real geometry.
    let is_drop = |t: &[u32]| {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            return true;
        }
        // Placed and unplaced vertices live in different frames — never span them.
        if placed && t.iter().any(|&i| !placed_vert[i as usize]) {
            return true;
        }
        false
    };
    let mut indices = Vec::with_capacity(raw_idx.len());
    let mut submeshes = Vec::new();
    if covers {
        let mut kept_start = 0u32;
        for s in &raw_subs {
            let mut kept = 0u32;
            for t in s.tri_start..s.tri_start + s.tri_count {
                let tri = &raw_idx[t * 3..t * 3 + 3];
                if !is_drop(tri) {
                    indices.extend_from_slice(tri);
                    kept += 1;
                }
            }
            if kept > 0 {
                submeshes.push(Submesh {
                    name: s.name.clone(),
                    tri_start: kept_start,
                    tri_count: kept,
                    texture: None,
                    uv_tile: uv_tile(&uvs, s.vert_start, s.vert_count),
                    // A split skinned range carries its own material index; otherwise
                    // it sits at `block_off - 4` (see `Submesh::mat`).
                    mat: s.mat.or_else(|| s.block_off.checked_sub(4).map(|o| u32le(b, o))),
                });
                kept_start += kept;
            }
        }
    } else {
        // No submesh table (or an incomplete one): decode the node as one list.
        for t in raw_idx.chunks_exact(3) {
            if !is_drop(t) {
                indices.extend_from_slice(t);
            }
        }
    }

    EdfNode {
        name,
        positions,
        uvs,
        normals,
        indices,
        submeshes,
        texture: None,
        placed,
    }
}

/// Convert a decoded model from the game's **left-handed** frame to the
/// **right-handed** one three.js renders in, by negating X on positions and
/// normals.
///
/// **This is the root cause of both the mirrored graphics and the "holes".**
/// MX Bikes is a DirectX engine, and DirectX is left-handed. The viewer fed those
/// coordinates straight to three.js (right-handed), which mirrors the model. That
/// single mistake produced two separate-looking bugs:
///
/// 1. **Mirrored artwork** — "HONDA" on the seat and "CRF"/"450R" on the shrouds
///    render back-to-front. Note this is *not* a `flipY` problem and can't be fixed
///    with one: a texture flip moves the UV islands (setting `flipY = true` drags
///    the dark engine-metals region of the atlas onto the bodywork), whereas the
///    islands were landing correctly all along — only their *contents* were
///    mirrored, which is what a mirrored mesh does.
/// 2. **Dark facets / "holes"** — a mirror inverts triangle orientation, so against
///    the model's own normals **100.0%** of the Honda's `chassis` and `fsusp`
///    triangles read as back-facing (gear agrees: boots 100.0%, the TLD SE4 helmet
///    99.9%). Culling was never the culprit — the viewer renders `DoubleSide`, so
///    nothing ever vanished — but `DoubleSide` lighting does
///    `normal *= gl_FrontFacing ? 1.0 : -1.0`, so every normal was negated and the
///    whole bike was lit from the inside. The geometry was complete the entire time.
///
/// Negating X fixes both at once, and the winding then agrees with the normals with
/// no re-winding needed. (Proven by rendering the real Honda: the text reads
/// correctly and the black facets resolve into solid red bodywork.)
///
/// Applied **after** [`assemble_bike`], deliberately: the `.geom` mount points and
/// rake rotations are authored in the game's own frame, and mirroring X inverts a
/// rotation about X — so the assembly math must run first, in native coordinates.
pub fn to_right_handed(nodes: &mut [EdfNode]) {
    for n in nodes.iter_mut() {
        for p in n.positions.chunks_exact_mut(3) {
            p[0] = -p[0];
        }
        for d in n.normals.chunks_exact_mut(3) {
            d[0] = -d[0];
        }
    }
}

/// Read a printable-ASCII C-string starting at `o` (node/submesh names).
fn read_cname(b: &[u8], o: usize) -> String {
    let mut e = o;
    while e < b.len() && (32..127).contains(&b[e]) {
        e += 1;
    }
    String::from_utf8_lossy(&b[o..e]).into_owned()
}

/// Scan the node's trailing metadata for submesh geometry blocks and chain them by
/// contiguity into `(name, raw_tri_start, raw_tri_count)`. A block is six u32:
/// `tri_start, tri_count, vert_start, vert_count, vert_count, vert_start` (the last
/// two duplicate the middle pair — the signature). The name sits at `block - 252`.
/// Read one submesh group at `o` → `(tri_start, tri_count, vert_start, vert_count)`.
///
/// A group is a run of 4-word `(tri_start, tri_count, vert_start, vert_count)`
/// ranges, each followed by a 2-word pair, laid out `[range][pair][range][pair]…`
/// (6 words / 24 bytes per step). The pair terminates the group when it reads
/// `(cumulative vert_count, the group's FIRST vert_start)`; anything else (in
/// practice `(0, 1)`) means another range follows.
///
/// The old reader only ever accepted the single-range case, where that terminator
/// degenerates to `(vert_count, vert_start)` — which looked like a "mirrored pair"
/// and got tested for as if it were one. It isn't a mirror, it's a running total,
/// and a multi-range group therefore failed to parse at all. Since the FIRST group
/// of the Yamaha's `fsusp`/`rsusp` is multi-range, the table couldn't even start at
/// `tri_start == 0`, came back empty, and the part was left unplaced — the broken
/// alignment. Proof this is the real layout, not a fitted guess: the Yamaha's
/// `fsusp` reads `(0,1520,0,1038)` + `(1520,1470,1038,1535)` → terminator
/// `(2573, 0)` = 1038+1535 ✓ and first vert_start ✓; the group ends at tri 2990 and
/// vert 2573, which is exactly where the next record begins, and the two groups sum
/// to 3374 tris / 2798 verts — the node's totals, on the nose, on both axes.
///
/// The ranges within a group are contiguous in both tri and vert space; that's
/// required anyway (a submesh renders as one `tri_start..tri_start+tri_count`
/// span), and it doubles as validation.
fn read_sub_group(b: &[u8], o: usize, tot_tris: usize, tot_verts: usize) -> Option<(usize, usize, usize, usize)> {
    let tri_start = u32le(b, o) as usize;
    let first_vs = u32le(b, o + 8) as usize;
    let (mut tri_total, mut vc_total) = (0usize, 0usize);
    let mut k = o;
    // A handful of ranges at most; the bound just stops a garbage read running away.
    for _ in 0..64 {
        if k + 24 > b.len() {
            return None;
        }
        let a = u32le(b, k) as usize;
        let cnt = u32le(b, k + 4) as usize;
        let vstart = u32le(b, k + 8) as usize;
        let vcnt = u32le(b, k + 12) as usize;
        // Each range must continue the previous one, and stay inside the node.
        if cnt == 0
            || vcnt == 0
            || a != tri_start + tri_total
            || vstart != first_vs + vc_total
            || a + cnt > tot_tris
            || vstart + vcnt > tot_verts
        {
            return None;
        }
        tri_total += cnt;
        vc_total += vcnt;
        // The pair: running vert total + the group's first vert_start ends it.
        if u32le(b, k + 16) as usize == vc_total && u32le(b, k + 20) as usize == first_vs {
            return Some((tri_start, tri_total, first_vs, vc_total));
        }
        k += 24;
    }
    None
}

/// Like [`read_sub_group`] but keeps each range separately rather than merging them
/// into one span: `[(tri_start, tri_count, vert_start, vert_count), …]`. Used to
/// split a skinned mesh's single group (the rider body) into its per-material ranges
/// (rider/gloves/face/…), which merge would otherwise collapse to one submesh.
fn read_sub_group_ranges(
    b: &[u8],
    o: usize,
    tot_tris: usize,
    tot_verts: usize,
) -> Option<Vec<(usize, usize, usize, usize)>> {
    let tri_start = u32le(b, o) as usize;
    let first_vs = u32le(b, o + 8) as usize;
    let (mut tri_total, mut vc_total) = (0usize, 0usize);
    let mut ranges = Vec::new();
    let mut k = o;
    for _ in 0..64 {
        if k + 24 > b.len() {
            return None;
        }
        let a = u32le(b, k) as usize;
        let cnt = u32le(b, k + 4) as usize;
        let vstart = u32le(b, k + 8) as usize;
        let vcnt = u32le(b, k + 12) as usize;
        if cnt == 0
            || vcnt == 0
            || a != tri_start + tri_total
            || vstart != first_vs + vc_total
            || a + cnt > tot_tris
            || vstart + vcnt > tot_verts
        {
            return None;
        }
        ranges.push((a, cnt, vstart, vcnt));
        tri_total += cnt;
        vc_total += vcnt;
        if u32le(b, k + 16) as usize == vc_total && u32le(b, k + 20) as usize == first_vs {
            return Some(ranges);
        }
        k += 24;
    }
    None
}

/// A submesh geometry block found anywhere in the file, anchored by a valid rigid
/// placement matrix at `block_off - 148`. Collected once per parse (see
/// [`collect_sub_cands`]) so every node can chain its table against the shared pool.
struct SubCand {
    /// Offset of the six-u32 geometry block (what a [`RawSub`] records as `block_off`).
    block_off: usize,
    tri_start: usize,
    tri_count: usize,
    vert_start: usize,
    vert_count: usize,
}

/// Collect every matrix-anchored submesh geometry block in the file, in ONE linear
/// pass. A block qualifies when a rigid 4×4 sits at `block_off - 148` — the anchor
/// that makes offset lookup safe in dense binary — and the six-u32 group at
/// `block_off` parses. Ranges are validated against generous bounds here; each node
/// re-checks records against its own totals when it chains (see [`detect_submeshes`]).
///
/// The scan is keyed on the matrix's bottom row, which a rigid transform fixes at
/// exactly `[0,0,0,1]`: find those 16 bytes (twelve zeros then `1.0f`) as a cheap
/// pre-filter, then fully validate. That row sits at `matrix_base + 48`, so the block
/// is a further 100 bytes on. Matrices are NOT 4-aligned in general (a node's name
/// ends the index buffer at an arbitrary offset), so the scan steps one byte at a time.
fn collect_sub_cands(b: &[u8]) -> Vec<SubCand> {
    let mut out = Vec::new();
    if b.len() < 16 {
        return out;
    }
    let end = b.len() - 16;
    let mut p = 0usize;
    while p <= end {
        // Fast reject: the bottom row's last word must be 1.0f, and the preceding
        // three words zero. Cheap enough to run at every byte of a 70 MB file.
        if u32le(b, p + 12) == 0x3F80_0000 && b[p..p + 12].iter().all(|&x| x == 0) {
            if let Some(mb) = p.checked_sub(48) {
                if read_mat4(b, mb).is_some() {
                    let o = mb + SUB_MAT_BACK;
                    if let Some((ts, tc, vs, vc)) = read_sub_group(b, o, MAX_COUNT, MAX_COUNT) {
                        out.push(SubCand {
                            block_off: o,
                            tri_start: ts,
                            tri_count: tc,
                            vert_start: vs,
                            vert_count: vc,
                        });
                    }
                }
            }
        }
        p += 1;
    }
    out
}

/// Assemble a node's submesh table by chaining the shared candidate pool, falling
/// back to the historical bounded-window scan if the chain can't be reconciled.
///
/// A node's records are laid out `[range 0][range 1]…` contiguous in BOTH triangle
/// and vertex space, but they are not always packed together after the node's name:
/// the Suzuki RM-Z450 chassis' first record (`SPRING`) sits just past its index
/// buffer, while the other five are ~5 MB later, on the far side of the embedded
/// texture blob. The old fixed ~200 KB window saw only `SPRING` (9 096 of 48 310
/// triangles) and dropped the other 81 % as unplaced — the "weird half paint".
///
/// So instead: chain by exact `(tri_start, vert_start)` contiguity across the whole
/// file, and accept the result only if it reconciles to BOTH the node's triangle and
/// vertex totals exactly. If it doesn't, fall back to today's window scan so a bike
/// the chain can't resolve is no worse off than before.
fn detect_submeshes(
    b: &[u8],
    cands: &[SubCand],
    iend: usize,
    tot_tris: usize,
    tot_verts: usize,
) -> Vec<RawSub> {
    if let Some(chained) = chain_submeshes(b, cands, iend, tot_tris, tot_verts) {
        return chained;
    }
    detect_submeshes_window(b, iend, tot_tris, tot_verts)
}

/// Chain the shared candidate pool into this node's submesh table, or `None` if it
/// doesn't reconcile to both totals exactly.
///
/// Every node's first submesh starts at `(tri_start 0, vert_start 0)`, and many
/// records across the file share that (each node and each LOD begins there), so the
/// chain is seeded with the `(0, 0)` record NEAREST AFTER this node's name — a node's
/// own metadata follows its name. From there it extends greedily by exact
/// `(tri_start, vert_start)` match; ties (near-impossible with both coordinates
/// pinned) go to the record nearest the previous one. A record from another node or
/// LOD can pass the per-node bounds check, but the exact contiguity chain and the
/// exact reconciliation reject it — which is also why the four other OEM bikes come
/// out byte-identical to the old window scan.
fn chain_submeshes(
    b: &[u8],
    cands: &[SubCand],
    iend: usize,
    tot_tris: usize,
    tot_verts: usize,
) -> Option<Vec<RawSub>> {
    let in_bounds =
        |c: &SubCand| c.tri_start + c.tri_count <= tot_tris && c.vert_start + c.vert_count <= tot_verts;
    let start = cands
        .iter()
        .filter(|c| c.tri_start == 0 && c.vert_start == 0 && c.block_off >= iend && in_bounds(c))
        .min_by_key(|c| c.block_off - iend)?;
    let mut out = Vec::new();
    let (mut run_t, mut run_v) = (0usize, 0usize);
    let mut prev_off = start.block_off;
    while run_t < tot_tris {
        let next = cands
            .iter()
            .filter(|c| c.tri_start == run_t && c.vert_start == run_v && in_bounds(c))
            .min_by_key(|c| c.block_off.abs_diff(prev_off))?;
        let name = if next.block_off >= 252 {
            read_cname(b, next.block_off - 252)
        } else {
            String::new()
        };
        out.push(RawSub {
            name,
            tri_start: run_t,
            tri_count: next.tri_count,
            block_off: next.block_off,
            vert_start: next.vert_start,
            vert_count: next.vert_count,
            mat: None,
        });
        run_t += next.tri_count;
        run_v = next.vert_start + next.vert_count;
        prev_off = next.block_off;
    }
    // Reconcile to BOTH totals exactly, or reject (→ window fallback).
    (run_t == tot_tris && run_v == tot_verts).then_some(out)
}

/// The pre-`SubCand` bounded scan: search a fixed ~200 KB window from the node's name
/// for matrix-anchored records and chain them by contiguity. Retained as the fallback
/// for [`detect_submeshes`] — a bike whose records the whole-file chain can't reconcile
/// is left exactly where it was before the chain existed.
fn detect_submeshes_window(b: &[u8], iend: usize, tot_tris: usize, tot_verts: usize) -> Vec<RawSub> {
    use std::collections::HashMap;
    let window = 200_000usize.min(b.len().saturating_sub(iend));
    // Candidate blocks, indexed by tri_start.
    let mut cand: HashMap<usize, Vec<(usize, usize, usize, usize)>> = HashMap::new(); // tri_start -> [(off, tri_count, vert_start, vert_count)]
    let mut i = 0usize;
    while i + 24 <= window {
        let o = iend + i;
        // Require the submesh's own placement matrix at `o - 148`. This is what
        // makes the scan safe in dense binary, and it replaces the range-shape
        // checks that used to carry that load alone: a rigid 4×4 (orthonormal,
        // |det|=1, last row exactly (0,0,0,1)) does not occur by chance, whereas
        // plausible-looking counts do — constantly. The Yamaha's `rsusp` window
        // yields 42 shape-valid candidates and exactly ONE with a matrix; the other
        // 41 are runs of consecutive small integers inside a neighbouring node's
        // data that happen to look like ranges. Filtering on the matrix is both
        // stricter and, unlike the shape test, correct for multi-range groups.
        if o >= SUB_MAT_BACK && read_mat4(b, o - SUB_MAT_BACK).is_some() {
            if let Some((a, cnt, vstart, vcnt)) = read_sub_group(b, o, tot_tris, tot_verts) {
                cand.entry(a).or_default().push((o, cnt, vstart, vcnt));
            }
        }
        i += 4;
    }

    let mut out = Vec::new();
    let (mut run_t, mut run_v) = (0usize, 0usize);
    while run_t < tot_tris {
        let Some(opts) = cand.get(&run_t) else { break };
        // Prefer the block whose vert_start matches our running vertex total.
        let pick = opts
            .iter()
            .find(|(_, _, vstart, _)| *vstart == run_v)
            .or_else(|| opts.first());
        let Some(&(o, cnt, vstart, vcnt)) = pick else { break };
        let name = if o >= 252 { read_cname(b, o - 252) } else { String::new() };
        out.push(RawSub {
            name,
            tri_start: run_t,
            tri_count: cnt,
            block_off: o,
            vert_start: vstart,
            vert_count: vcnt,
            mat: None,
        });
        run_t += cnt;
        run_v = vstart + vcnt;
    }
    out
}

/// Byte offsets, relative to a submesh's geometry block, of its placement matrix
/// and of that matrix's parent (see [`submesh_transform`]).
const SUB_MAT_BACK: usize = 148;
const SUB_MAT_PARENT_STEP: usize = 280;
/// The node's own orientation matrix occupies `name+104 .. name+168`. The parent
/// walk must stop before it: it's applied separately, and letting the walk reach
/// it applies the orientation twice (which silently flips the swingarm forward).
const NODE_MAT_OFF: usize = 104;
const NODE_MAT_END: usize = 168;

/// Resolve a submesh's full local transform chain, outermost-first.
///
/// Each submesh's own rigid 4×4 sits at `block_off - 148`; instanced sub-objects
/// (bar-end grips, footpegs) add a parent matrix a further 280 bytes back. Composed
/// with the node's orientation at `name+104`, this places the submesh in its
/// part's `.geom` LOCAL frame — for `chassis` (the root body) that IS design space.
fn submesh_transform(b: &[u8], name_off: usize, block_off: usize) -> Vec<Mat4> {
    let mut chain = Vec::new();
    let Some(base) = block_off.checked_sub(SUB_MAT_BACK) else {
        return chain;
    };
    let Some(m) = read_mat4(b, base) else {
        return chain;
    };
    chain.push(m);
    let mut k = base;
    while let Some(p) = k.checked_sub(SUB_MAT_PARENT_STEP) {
        if p < name_off + NODE_MAT_END {
            break;
        }
        let Some(pm) = read_mat4(b, p) else { break };
        chain.push(pm);
        k = p;
    }
    // Already innermost-first: the submesh's own matrix applies before its parent's,
    // so callers fold in order. (Reversing puts the bar-end grips at y≈1.53 instead
    // of on the handlebars.)
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Investigation aid: parse an `.edf` and print its overall vertex bounds + node
    /// names — the reference frame for validating decoded bone world positions.
    /// `MXB_EDF_FILE=/tmp/rider.edf cargo test edf_bounds -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn edf_bounds() {
        let path = std::env::var("MXB_EDF_FILE").expect("set MXB_EDF_FILE");
        let bytes = std::fs::read(&path).expect("read edf");
        let nodes = parse(&bytes);
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for n in &nodes {
            for c in n.positions.chunks_exact(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(c[k]);
                    hi[k] = hi[k].max(c[k]);
                }
            }
        }
        eprintln!("nodes: {}", nodes.len());
        eprintln!("overall bbox lo={lo:?} hi={hi:?}  size={:?}", [hi[0]-lo[0], hi[1]-lo[1], hi[2]-lo[2]]);
        for n in &nodes {
            eprintln!("  node '{}'  verts={}  submeshes={}", n.name, n.positions.len() / 3, n.submeshes.len());
            for sm in &n.submeshes {
                eprintln!("      submesh '{}'  tris={}  tex={:?}", sm.name, sm.tri_count, sm.texture);
            }
        }
    }

    /// Build a submesh-group record: `[range][pair][range][pair]…`.
    fn group_bytes(ranges: &[(u32, u32, u32, u32)], pairs: &[(u32, u32)]) -> Vec<u8> {
        let mut v = Vec::new();
        for (r, p) in ranges.iter().zip(pairs) {
            for w in [r.0, r.1, r.2, r.3, p.0, p.1] {
                v.extend_from_slice(&w.to_le_bytes());
            }
        }
        v
    }

    /// A single-range group ends on `(vert_count, vert_start)` — the case the old
    /// reader hard-coded as a "mirrored pair".
    #[test]
    fn reads_single_range_submesh_group() {
        // The real Honda chassis' first group: tris 0..31846, verts 0..24904.
        let b = group_bytes(&[(0, 31846, 0, 24904)], &[(24904, 0)]);
        assert_eq!(read_sub_group(&b, 0, 46184, 35689), Some((0, 31846, 0, 24904)));
    }

    /// A multi-range group: the terminator is `(cumulative vert_count, first
    /// vert_start)`, so it only *looks* like a mirror when there's one range. The
    /// old reader rejected this outright, which left the Yamaha's fork and swingarm
    /// with an empty table and therefore unplaced.
    ///
    /// These are the real bytes of the Yamaha YZ450F's `fsusp` first group.
    #[test]
    fn reads_multi_range_submesh_group() {
        let b = group_bytes(
            &[(0, 1520, 0, 1038), (1520, 1470, 1038, 1535)],
            &[(0, 1), (2573, 0)], // (0,1) continues; (1038+1535, first vs) ends
        );
        // Whole group: tris 0..2990, verts 0..2573 — which is exactly where the
        // node's next record begins, and 2990+384 == the node's 3374 total.
        assert_eq!(read_sub_group(&b, 0, 3374, 2798), Some((0, 2990, 0, 2573)));
    }

    /// Ranges within a group must chain in BOTH tri and vert space; a table that
    /// doesn't is not understood, and half-placing a node is far worse than not
    /// placing it (it mixes two frames in one mesh).
    #[test]
    fn rejects_non_contiguous_submesh_group() {
        let b = group_bytes(
            &[(0, 1520, 0, 1038), (9999, 1470, 1038, 1535)], // tri gap
            &[(0, 1), (2573, 0)],
        );
        assert_eq!(read_sub_group(&b, 0, 30000, 30000), None);
    }

    /// A group that never terminates (garbage) must not run away.
    #[test]
    fn rejects_unterminated_submesh_group() {
        let mut ranges = Vec::new();
        let mut pairs = Vec::new();
        for i in 0..80u32 {
            ranges.push((i, 1, i, 1));
            pairs.push((0, 1)); // always "continue" — never ends
        }
        let b = group_bytes(&ranges, &pairs);
        assert_eq!(read_sub_group(&b, 0, 10_000, 10_000), None);
    }

    /// The chain must span records that are NOT adjacent in the file. The Suzuki
    /// chassis' first submesh sits right after its name while the rest are ~5 MB
    /// later (past the embedded texture blob); a bounded window sees only the first.
    /// Chaining by exact `(tri_start, vert_start)` across the whole pool stitches
    /// them and reconciles to the node's real totals (the Suzuki chassis'
    /// 48 310 tris / 35 318 verts, here condensed to two records).
    #[test]
    fn chains_records_split_across_a_gap() {
        let b = [0u8; 8]; // block_off < 252 → names skipped, buffer unused
        let cands = vec![
            SubCand { block_off: 100, tri_start: 0, tri_count: 9096, vert_start: 0, vert_count: 6816 },
            SubCand { block_off: 200, tri_start: 9096, tri_count: 39214, vert_start: 6816, vert_count: 28502 },
        ];
        let subs = chain_submeshes(&b, &cands, 0, 48310, 35318).expect("chain reconciles");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].tri_start, 0);
        assert_eq!(subs[1].tri_start, 9096);
        assert_eq!(subs.iter().map(|s| s.tri_count).sum::<usize>(), 48310);
        assert_eq!(subs.last().unwrap().vert_start + subs.last().unwrap().vert_count, 35318);
    }

    /// A chain that can't be reconciled to BOTH totals must be rejected (so the
    /// caller falls back to the window scan) rather than half-cover the node — a
    /// break in vertex contiguity here leaves the second record unreachable.
    #[test]
    fn rejects_unreconcilable_chain() {
        let b = [0u8; 8];
        let cands = vec![
            SubCand { block_off: 100, tri_start: 0, tri_count: 9096, vert_start: 0, vert_count: 6816 },
            // vert_start doesn't continue 6816 → the chain can't reach it.
            SubCand { block_off: 200, tri_start: 9096, tri_count: 39214, vert_start: 9999, vert_count: 28502 },
        ];
        assert!(chain_submeshes(&b, &cands, 0, 48310, 35318).is_none());
    }

    /// End-to-end guard on a real bike: the chassis' submesh table must cover EVERY
    /// one of its triangles. This is what the Suzuki RM-Z450 failed before — a
    /// windowed scan found only its first record (`SPRING`, 9 096 of 48 310 tris) and
    /// the other 81 % rendered as dropped, unplaced geometry (the "weird half paint").
    /// Passes for every OEM bike, since a covered chassis has a submesh table whose
    /// counts sum to its kept-triangle count (a correct decode drops zero).
    /// `MXB_REAL_EDF=…/suzuki model.edf cargo test -- --ignored chassis_submeshes_cover`
    #[test]
    #[ignore]
    fn chassis_submeshes_cover_all_triangles() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            eprintln!("set MXB_REAL_EDF to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read real edf");
        let nodes = parse(&bytes);
        let ch = nodes
            .iter()
            .find(|n| n.name.to_ascii_lowercase().starts_with("chassis"))
            .expect("chassis node");
        let covered: u32 = ch.submeshes.iter().map(|s| s.tri_count).sum();
        eprintln!(
            "chassis '{}' placed={} kept_tris={} covered_by_submeshes={} ({} groups)",
            ch.name,
            ch.placed,
            ch.indices.len() / 3,
            covered,
            ch.submeshes.len()
        );
        assert!(ch.placed, "chassis must be placed");
        assert!(!ch.submeshes.is_empty(), "chassis must have a submesh table");
        assert_eq!(
            covered as usize,
            ch.indices.len() / 3,
            "submesh groups must cover every kept chassis triangle"
        );
    }

    #[test]
    fn parses_geom_mount_points() {
        let g = b"type = bike\nchassis_steer = 0, 0.9935, 0.2982\n; a comment\nsteer_joint = 0, 0.0412, -0.0372\nrsusp_type = Linkage\nchain_pitch = 0.0159\n";
        let m = parse_geom(g);
        assert_eq!(m.get("chassis_steer"), Some(&[0.0, 0.9935, 0.2982]));
        assert_eq!(m.get("steer_joint"), Some(&[0.0, 0.0412, -0.0372]));
        assert!(!m.contains_key("rsusp_type")); // non-vector line ignored
        assert!(!m.contains_key("chain_pitch")); // single scalar ignored
    }

    #[test]
    fn rejects_non_edf() {
        assert!(parse(b"not an edf file, definitely not, no way at all........").is_empty());
    }

    /// Build a one-node EDF (vc >= 8, the parser's minimum) with the given
    /// triangle indices; positions are a simple ramp so samples read as finite.
    fn synth_edf(vc: usize, tris: &[[u32; 3]]) -> Vec<u8> {
        let mut b = vec![0u8; HEADER_START];
        b[0..4].copy_from_slice(b"EDF\0");
        b.extend_from_slice(&(vc as u32).to_le_bytes());
        let mut attrs = vec![0u8; vc * STRIDE];
        // positions occupy the first vc*12 bytes (SoA position sub-array). Use
        // spread-out, non-collinear points so triangles have real area (a ramp
        // would read as zero-area slivers and be filtered out).
        let pts: [[f32; 3]; 8] = [
            [0.0, 0.0, 0.0],
            [0.3, 0.0, 0.0],
            [0.0, 0.3, 0.0],
            [0.0, 0.0, 0.3],
            [0.3, 0.0, 0.3],
            [0.0, 0.3, 0.3],
            [0.2, 0.1, 0.15],
            [0.1, 0.2, 0.05],
        ];
        for i in 0..vc {
            for k in 0..3 {
                let o = (i * 3 + k) * 4;
                attrs[o..o + 4].copy_from_slice(&pts[i % 8][k].to_le_bytes());
            }
        }
        b.extend_from_slice(&attrs);
        // Index block, real layout: [tri_count][tri_count*3 indices][submesh_count]
        // then the node name. Note there is NO padding word between the count and
        // idx0 — this fixture used to write one, which is exactly the mistake the
        // parser made (idx0 is 0 in real files because the first triangle is
        // (0,1,2), so a stray zero word here looks indistinguishable from it).
        b.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for t in tris {
            for i in t {
                b.extend_from_slice(&i.to_le_bytes());
            }
        }
        b.extend_from_slice(&1u32.to_le_bytes()); // submesh_count
        // The parser anchors on a node name right after the index buffer.
        b.extend_from_slice(b"testnode\0");
        b
    }

    #[test]
    fn parses_a_synthetic_soa72_node() {
        let b = synth_edf(8, &[[0, 1, 2], [3, 4, 5]]);
        let nodes = parse(&b);
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert_eq!(node.positions.len(), 24); // 8 verts * 3
        assert_eq!(node.uvs.len(), 16); // 8 verts * 2
        assert_eq!(node.normals.len(), 24); // 8 verts * 3
        // Indices decode EXACTLY as authored: a plain triangle list read from
        // ic+4. No interpretation, no dropped or invented triangles.
        assert_eq!(node.indices, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn drops_degenerate_triangles() {
        // Second triangle is degenerate (a == b) and must be dropped.
        let b = synth_edf(8, &[[0, 1, 2], [1, 1, 2]]);
        let nodes = parse(&b);
        assert_eq!(nodes[0].indices, vec![0, 1, 2], "degenerate dropped");
    }

    /// The chassis is the root body, so placing it must reproduce the `.edf`
    /// header's own design-space AABB (file+4). That AABB is authored independently
    /// of the node/submesh matrices, making this a real end-to-end check on the
    /// placement chain rather than a restatement of it.
    ///
    /// It also pins the parent-chain stop: the walk back from `rec-148` in 280-byte
    /// steps must not reach the node's orientation matrix at +104, or that matrix
    /// is applied twice and the swingarm silently flips to point forward.
    /// `MXB_REAL_EDF=…/honda model.edf cargo test -- --ignored placed_chassis`
    #[test]
    #[ignore]
    fn placed_chassis_matches_header_aabb() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            eprintln!("set MXB_REAL_EDF to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read real edf");
        let aabb: Vec<f32> = (0..6).map(|i| f32le(&bytes, 4 + i * 4)).collect();
        let nodes = parse(&bytes);
        let ch = nodes
            .iter()
            .find(|n| n.name.to_ascii_lowercase().starts_with("chassis"))
            .expect("chassis node");
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in ch.positions.chunks_exact(3) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        eprintln!("header aabb {aabb:?}\nplaced lo {lo:?} hi {hi:?}");
        // The chassis node carries stray sub-parts (exhaust, footpegs) that sit
        // inside the hull, so its floor can be above the AABB's; every other bound
        // must land on it. Honda reads 5/6 exact with min-y ~0.02 high.
        for k in 0..3 {
            assert!(
                (hi[k] - aabb[3 + k]).abs() < 0.02,
                "axis {k} max {} vs header {}",
                hi[k],
                aabb[3 + k]
            );
            assert!(
                lo[k] >= aabb[k] - 0.02,
                "axis {k} min {} below header {}",
                lo[k],
                aabb[k]
            );
        }
        // Swingarm must run rearward (-Z) from its pivot, not forward.
        if let Some(rs) = nodes.iter().find(|n| n.name.to_ascii_lowercase().starts_with("rsusp")) {
            let (mut zlo, mut zhi) = (f32::MAX, f32::MIN);
            for p in rs.positions.chunks_exact(3) {
                zlo = zlo.min(p[2]);
                zhi = zhi.max(p[2]);
            }
            eprintln!("rsusp local z [{zlo}, {zhi}]");
            assert!(zlo < -0.4, "swingarm should reach ~-0.57 rearward, got {zlo}");
            assert!(zhi < 0.2, "swingarm should not extend forward, got {zhi}");
        }
    }

    /// The model's own texture pool, from a real mesh. Names are the model
    /// author's: the Honda reads `2021crf, 2023crf_r, 2023crf_n, w_plate,
    /// exhaust_22, exhaust_23_r, exhaust_23_n`; the KTM 450 `450f_metals,
    /// 450f_metals_n, plastics, plastics_n, w_plate`. The KTM naming its body
    /// texture `plastics` — and the Honda NOT — is why a `.pnt` paint's `plastics`
    /// changes one and not the other.
    /// `MXB_REAL_EDF=…/model.edf cargo test -- --ignored embedded_textures`
    #[test]
    #[ignore]
    fn embedded_textures_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            eprintln!("set MXB_REAL_EDF to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read real edf");
        let texs = embedded_textures(&bytes);
        for t in &texs {
            eprintln!("tex '{}' {}x{} data@{} len={}", t.name, t.width, t.height, t.data_off, t.data_len);
        }
        assert!(!texs.is_empty(), "found the model's textures");
        // Every record must actually inflate to width*height*4 RGBA bytes — that is
        // what makes the shape-based scan a proof rather than a guess. (`_r`
        // roughness maps use a format we don't decode, so they're exempt.)
        for t in texs.iter().filter(|t| !t.name.ends_with("_r")) {
            let rgba = inflate_texture(&bytes, t)
                .unwrap_or_else(|| panic!("inflate '{}'", t.name));
            assert_eq!(rgba.len(), (t.width as usize) * (t.height as usize) * 4);
        }
        // Names must come through whole — a mis-set field offset silently truncates
        // them (`2021crf` read as `crf`), which would then match no paint texture.
        assert!(
            texs.iter().all(|t| t.name.len() >= 3),
            "names are not truncated: {:?}",
            texs.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
    }

    /// Local-only proof against a real bike mesh: set `MXB_REAL_EDF` to a real
    /// `model.edf` path and run `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn parses_real_edf_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            eprintln!("set MXB_REAL_EDF to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read real edf");
        let nodes = parse(&bytes);
        assert!(!nodes.is_empty(), "recovered at least one mesh node");
        // `MXB_OBJ=<file>` dumps the decoded mesh so it can be rendered/inspected
        // exactly as the viewer receives it.
        if let Ok(obj) = std::env::var("MXB_OBJ") {
            let mut s = String::new();
            let mut base = 1usize;
            for nd in &nodes {
                for p in nd.positions.chunks_exact(3) {
                    s.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
                }
                for t in nd.indices.chunks_exact(3) {
                    s.push_str(&format!(
                        "f {} {} {}\n",
                        base + t[0] as usize,
                        base + t[1] as usize,
                        base + t[2] as usize
                    ));
                }
                base += nd.positions.len() / 3;
            }
            std::fs::write(&obj, s).expect("write obj");
            eprintln!("wrote {obj}");
        }
        for n in &nodes {
            let verts = n.positions.len() / 3;
            let tris = n.indices.len() / 3;
            // Basic bbox for a sanity eyeball.
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for p in n.positions.chunks_exact(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            eprintln!(
                "node '{}' verts={verts} tris={tris} uv={} nrm={} submeshes={} bbox=[{:.2},{:.2},{:.2}]..[{:.2},{:.2},{:.2}]",
                n.name,
                !n.uvs.is_empty(),
                !n.normals.is_empty(),
                n.submeshes.len(),
                lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
            );
            for s in &n.submeshes {
                eprintln!("    submesh '{}' tri[{}..{})", s.name, s.tri_start, s.tri_start + s.tri_count);
            }
            assert!(verts >= 8 && tris >= 1);
        }
    }
}
