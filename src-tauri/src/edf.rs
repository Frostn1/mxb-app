// EDF mesh format: "EDF\0" magic, global AABB, then SoA node blocks (72 B/vertex).
// Per-vertex: position f32[3] @ vs, uv0 f32[2] @ vs+vc*12 (stride 8), normal f32[3]
// @ vs+vc*44. Index block: u32 tri_count @ ic, u32[3]*tc indices @ ic+4 (plain
// triangle list, NOT ic+8), u32 submesh_count, then node name @ ic+8+tc*12 (anchor).

use serde::Serialize;

const STRIDE: usize = 72;
const HEADER_START: usize = 0x54;
const MAX_COUNT: usize = 3_000_000;

// tri_start/tri_count index the KEPT triangle list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Submesh {
    pub name: String,
    pub tri_start: u32,
    pub tri_count: u32,
    pub texture: Option<String>,
    // floor(u): which UV tile the group samples (sampled at u - tile). None when it straddles tiles.
    pub uv_tile: Option<i32>,
    // u32 at block_off - 4 = material index into the model's colour textures in FILE order.
    pub mat: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdfNode {
    pub name: String,
    pub positions: Vec<f32>, // 3 * vcount, local space
    pub uvs: Vec<f32>,       // 2 * vcount
    pub normals: Vec<f32>,   // 3 * vcount
    pub indices: Vec<u32>,   // 3 * kept triangles
    pub submeshes: Vec<Submesh>,
    pub texture: Option<String>, // node-wide texture, used when submeshes is empty
    // True once positions are in the part's .geom LOCAL frame rather than raw authored space.
    #[serde(skip)]
    pub placed: bool,
}

// Parse the .geom's `key = x, y, z` vector mount points (ignores non-vector lines).
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

// Parse the .geom's single-value keys (e.g. `rakeangle_min = 27.1`).
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

struct RawSub {
    name: String,
    tri_start: usize,
    tri_count: usize,
    block_off: usize, // offset of the six-u32 geometry block
    vert_start: usize,
    vert_count: usize,
    // Set for a skinned group's per-material range; None → material id read from block_off - 4.
    mat: Option<u32>,
}

// Rigid 4x4 placement matrix: row-major, translation in the 4th column.
type Mat4 = [f32; 16];

// Read a placement matrix at `o`, or None unless rigid: bottom row [0,0,0,1], orthonormal rows, |det|==1.
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

fn mat_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
    ]
}

// Rotation only (for normals — no translation).
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

fn finite_pos(b: &[u8], o: usize) -> bool {
    (0..3).all(|k| {
        let v = f32le(b, o + 4 * k);
        v.is_finite() && v.abs() < 200.0
    })
}

// Parse an .edf into its renderable mesh nodes (highest-detail LOD of each part).
pub fn parse(b: &[u8]) -> Vec<EdfNode> {
    parse_impl(b, &[])
}

// Parse keeping exactly the nodes the bike's .hrc declares as level0; empty slice
// falls back to level0_only's name heuristic.
pub fn parse_with_levels(b: &[u8], level0: &[String]) -> Vec<EdfNode> {
    parse_impl(b, level0)
}

fn parse_impl(b: &[u8], level0: &[String]) -> Vec<EdfNode> {
    let n = b.len();
    if n < HEADER_START + 8 || &b[0..4] != b"EDF\0" {
        return Vec::new();
    }
    let mut nodes = Vec::new();
    let cands = collect_sub_cands(b);
    let mut o = HEADER_START;

    while o + 8 <= n {
        let vc = u32le(b, o) as usize;
        if (8..=MAX_COUNT).contains(&vc) && o + 4 + vc * STRIDE + 8 <= n {
            let vs = o + 4;
            let samples = [0usize, 1, 2, vc / 2, vc - 1];
            if samples.iter().all(|&i| finite_pos(b, vs + i * 12)) {
                let ic = vs + vc * STRIDE;
                let tc = u32le(b, ic) as usize;
                if (1..=MAX_COUNT).contains(&tc) && ic + 8 + tc * 12 <= n {
                    // Index block: [tc][ tc*3 u32 indices @ ic+4 ][u32 submesh_count @ ic+4+tc*12][name]
                    // indices start at ic+4 (idx0), NOT ic+8.
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
                    // Name anchor @ ic+8+tc*12 (past the indices and submesh_count).
                    let iend = ic + 8 + tc * 12;
                    if let (true, Some(name)) = (ok, plausible_name(b, iend)) {
                        nodes.push(read_node(b, &cands, vs, vc, raw, iend, tc, name));
                        o = iend; // jump past this block
                        continue;
                    }
                }
            }
        }
        // Resync one byte at a time: nodes after the texture blob land unaligned.
        o += 1;
    }
    if level0.is_empty() {
        return level0_only(nodes);
    }
    let want: std::collections::HashSet<String> =
        level0.iter().map(|n| n.to_ascii_lowercase()).collect();
    if !nodes.iter().any(|n| want.contains(&n.name.to_ascii_lowercase())) {
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

// Rotate about X by `deg` (design frame: +Y up, +Z forward).
fn rot_x(p: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

// Assemble a bike's parts onto its chassis via the .geom mount points, then centre
// on the origin. Returns false (nodes untouched) if the .geom lacks the mounts.
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
        // An unplaced part is still in raw authored space — the .geom mounts don't apply.
        if !n.placed {
            continue;
        }
        // Match the part by prefix (names carry a displacement tag, e.g. `chassis450f`).
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

    // Centre the assembled bike on the origin (the viewer orbits [0,0,0]).
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

// A node name at `o`: 2-31 name-safe chars starting with a letter, else None.
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
// Group LOD variants by base name, keeping level0 (the untagged node).
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
    // Level0 is the node whose name IS the base; prefer that exact match, fall back to most triangles.
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

// floor(u) if every vertex in [vert_start, vert_start+vert_count) agrees, else None.
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

#[derive(Debug, Clone)]
pub struct EmbeddedTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub data_off: usize, // byte offset of the raw-DEFLATE RGBA payload
    pub data_len: usize, // compressed byte length
}

// Record layout from `width`: | width u32 | height u32 | md5[16] | u32 | data_size u32 | pad[8] | data |
// data_size counts the 8 pad bytes, so payload = data_size - 8.
const TEX_SIZE_FROM_W: usize = 28;
const TEX_PAD_FROM_W: usize = 32;
const TEX_DATA_FROM_W: usize = 40;
const TEX_PAD_LEN: usize = 8;
// Name's first char to `width`: either -100 or -104 depending on the record; probe both.
const TEX_W_FROM_NAME: [usize; 2] = [100, 104];

// A null-terminated embedded-texture name at `o`: 2-39 name-safe chars (may lead with a digit).
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

// Enumerate every texture in a model.edf, in file order. Anchored on the name, then
// validated by shape (power-of-two dims, 8 zero pad bytes, payload fits the file).
pub fn embedded_textures(b: &[u8]) -> Vec<EmbeddedTexture> {
    const SIZES: [u32; 7] = [64, 128, 256, 512, 1024, 2048, 4096];
    let mut out = Vec::new();
    let mut o = 0usize;
    'scan: while o + TEX_W_FROM_NAME[1] + TEX_DATA_FROM_W <= b.len() {
        // A name starts a record only at a word boundary (else `2021crf` also matches at `crf`).
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

// Inflate an embedded texture to RGBA8 (width * height * 4 bytes), or None if it doesn't decode.
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

// Extract a block's attribute arrays and submesh groups (remapped to kept triangles).
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
    // SoA: positions @ vs (3f) | uv @ vs+vc*12 (2f, SINGLE set → stride 8) | normal @ vs+vc*44 (3f).
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

    let mut raw_subs = detect_submeshes(b, cands, iend, raw_tris, vc);
    // A skinned mesh (rider body) is ONE group whose contiguous ranges are distinct
    // materials; split it back out so each material can bind its own texture.
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
    // Covers the node when the submesh triangle counts sum to the raw total.
    let covers = !raw_subs.is_empty() && raw_subs.iter().map(|s| s.tri_count).sum::<usize>() == raw_tris;

    // Place the geometry: each submesh's own transform composed with the node
    // orientation matrix (at iend, the name offset) yields its .geom LOCAL frame.
    // Vertices not listed in the table stay unplaced; placed_vert tracks coverage so
    // a triangle spanning both frames is dropped below.
    let mut placed_vert = vec![false; vc];
    let mut placed = false;
    let skip_place = std::env::var_os("MXB_NO_PLACE").is_some(); // dev: render raw authored space
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
                    // Split skinned range carries its own mat; else read u32 at block_off - 4.
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

// Convert from the game's left-handed frame (DirectX) to three.js's right-handed
// one by negating X on positions and normals. Must run AFTER assemble_bike, whose
// rake/mount math is authored in the game's own frame.
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

fn read_cname(b: &[u8], o: usize) -> String {
    let mut e = o;
    while e < b.len() && (32..127).contains(&b[e]) {
        e += 1;
    }
    String::from_utf8_lossy(&b[o..e]).into_owned()
}

// Read one submesh group at `o` → (tri_start, tri_count, vert_start, vert_count).
// Layout: [range][pair][range][pair]... at 24 bytes/step, where range is 4 u32
// (tri_start, tri_count, vert_start, vert_count) and pair is 2 u32. The pair ends
// the group when it reads (cumulative vert_count, group's FIRST vert_start); anything
// else (in practice (0,1)) means another range follows. Name sits at block - 252.
fn read_sub_group(b: &[u8], o: usize, tot_tris: usize, tot_verts: usize) -> Option<(usize, usize, usize, usize)> {
    let tri_start = u32le(b, o) as usize;
    let first_vs = u32le(b, o + 8) as usize;
    let (mut tri_total, mut vc_total) = (0usize, 0usize);
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
        tri_total += cnt;
        vc_total += vcnt;
        // Terminator pair: (running vert total, group's first vert_start).
        if u32le(b, k + 16) as usize == vc_total && u32le(b, k + 20) as usize == first_vs {
            return Some((tri_start, tri_total, first_vs, vc_total));
        }
        k += 24;
    }
    None
}

// Like read_sub_group but keeps each range separate (to split a skinned mesh's group
// into per-material ranges rather than merging them into one span).
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

// A submesh geometry block, anchored by a rigid placement matrix at block_off - 148.
struct SubCand {
    block_off: usize, // offset of the six-u32 geometry block
    tri_start: usize,
    tri_count: usize,
    vert_start: usize,
    vert_count: usize,
}

// Collect every matrix-anchored submesh block in the file in one linear pass. Keyed
// on the matrix bottom row [0,0,0,1] (twelve zeros then 1.0f at matrix_base+48, block
// a further 100 bytes on); matrices are not 4-aligned, so scan one byte at a time.
fn collect_sub_cands(b: &[u8]) -> Vec<SubCand> {
    let mut out = Vec::new();
    if b.len() < 16 {
        return out;
    }
    let end = b.len() - 16;
    let mut p = 0usize;
    while p <= end {
        // Fast reject: bottom row's last word == 1.0f (0x3F800000), preceding three zero.
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

// Build a node's submesh table by chaining the shared candidate pool, falling back to
// the bounded-window scan if the chain can't reconcile to both totals.
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

// Chain the candidate pool into this node's table, seeded with the (0,0) record
// nearest after the node's name, extending by exact (tri_start, vert_start) match.
// Returns None unless it reconciles to both totals exactly.
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

// Fallback for detect_submeshes: scan a fixed ~200 KB window from the node's name for
// matrix-anchored records and chain them by contiguity.
fn detect_submeshes_window(b: &[u8], iend: usize, tot_tris: usize, tot_verts: usize) -> Vec<RawSub> {
    use std::collections::HashMap;
    let window = 200_000usize.min(b.len().saturating_sub(iend));
    // Candidate blocks, indexed by tri_start.
    let mut cand: HashMap<usize, Vec<(usize, usize, usize, usize)>> = HashMap::new(); // tri_start -> [(off, tri_count, vert_start, vert_count)]
    let mut i = 0usize;
    while i + 24 <= window {
        let o = iend + i;
        // Require the submesh's own placement matrix at o - 148 (block_off - SUB_MAT_BACK).
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

// Submesh matrix at block_off - 148; its parent a further 280 bytes back.
const SUB_MAT_BACK: usize = 148;
const SUB_MAT_PARENT_STEP: usize = 280;
// Node orientation matrix occupies name+104 .. name+168; the parent walk must stop
// before it, or the orientation is applied twice (flips the swingarm forward).
const NODE_MAT_OFF: usize = 104;
const NODE_MAT_END: usize = 168;

// Resolve a submesh's full local transform chain, innermost-first.
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
    // Innermost-first: the submesh's own matrix applies before its parent's; callers fold in order.
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    // Investigation aid: print an .edf's overall vertex bounds + node names.
    // MXB_EDF_FILE=/tmp/rider.edf cargo test edf_bounds -- --ignored --nocapture
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

    // Build a submesh-group record: [range][pair][range][pair]...
    fn group_bytes(ranges: &[(u32, u32, u32, u32)], pairs: &[(u32, u32)]) -> Vec<u8> {
        let mut v = Vec::new();
        for (r, p) in ranges.iter().zip(pairs) {
            for w in [r.0, r.1, r.2, r.3, p.0, p.1] {
                v.extend_from_slice(&w.to_le_bytes());
            }
        }
        v
    }

    #[test]
    fn reads_single_range_submesh_group() {
        // The real Honda chassis' first group: tris 0..31846, verts 0..24904.
        let b = group_bytes(&[(0, 31846, 0, 24904)], &[(24904, 0)]);
        assert_eq!(read_sub_group(&b, 0, 46184, 35689), Some((0, 31846, 0, 24904)));
    }

    // Real bytes of the Yamaha YZ450F's `fsusp` first group (a multi-range group).
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

    #[test]
    fn rejects_non_contiguous_submesh_group() {
        let b = group_bytes(
            &[(0, 1520, 0, 1038), (9999, 1470, 1038, 1535)], // tri gap
            &[(0, 1), (2573, 0)],
        );
        assert_eq!(read_sub_group(&b, 0, 30000, 30000), None);
    }

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

    // The chain must span records that are NOT adjacent in the file (Suzuki chassis:
    // first submesh right after its name, the rest ~5 MB past the texture blob).
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

    // The chassis' submesh table must cover every one of its triangles.
    // MXB_REAL_EDF=…/suzuki model.edf cargo test -- --ignored chassis_submeshes_cover
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

    // Build a one-node EDF (vc >= 8, the parser's minimum) with the given triangles.
    fn synth_edf(vc: usize, tris: &[[u32; 3]]) -> Vec<u8> {
        let mut b = vec![0u8; HEADER_START];
        b[0..4].copy_from_slice(b"EDF\0");
        b.extend_from_slice(&(vc as u32).to_le_bytes());
        let mut attrs = vec![0u8; vc * STRIDE];
        // positions occupy the first vc*12 bytes; spread out so triangles have real area.
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
        // Index block: [tri_count][tri_count*3 indices][submesh_count][name]. NO padding
        // word between count and idx0.
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
        // Indices decode exactly as authored (plain triangle list read from ic+4).
        assert_eq!(node.indices, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn drops_degenerate_triangles() {
        // Second triangle is degenerate (a == b) and must be dropped.
        let b = synth_edf(8, &[[0, 1, 2], [1, 1, 2]]);
        let nodes = parse(&b);
        assert_eq!(nodes[0].indices, vec![0, 1, 2], "degenerate dropped");
    }

    // Placing the chassis (root body) must reproduce the .edf header's AABB (file+4).
    // MXB_REAL_EDF=…/honda model.edf cargo test -- --ignored placed_chassis
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
        // Stray sub-parts sit inside the hull, so the floor can be above the AABB's;
        // every other bound must land on it.
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

    // The model's own texture pool, from a real mesh.
    // MXB_REAL_EDF=…/model.edf cargo test -- --ignored embedded_textures
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
        // Every record must inflate to width*height*4 RGBA bytes (_r maps exempt).
        for t in texs.iter().filter(|t| !t.name.ends_with("_r")) {
            let rgba = inflate_texture(&bytes, t)
                .unwrap_or_else(|| panic!("inflate '{}'", t.name));
            assert_eq!(rgba.len(), (t.width as usize) * (t.height as usize) * 4);
        }
        // Names must come through whole (a mis-set field offset truncates them).
        assert!(
            texs.iter().all(|t| t.name.len() >= 3),
            "names are not truncated: {:?}",
            texs.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
    }

    // Local-only proof against a real bike mesh (set MXB_REAL_EDF, run with --ignored).
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
        // MXB_OBJ=<file> dumps the decoded mesh as the viewer receives it.
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
