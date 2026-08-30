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
    // Material id, LOCAL to the owning node — look it up in that node's `materials`, never
    // in the model's colour list directly. Range i reads it at block_off + 24*i - 4.
    pub mat: Option<u32>,
}

/// A mesh's bulk arrays, base64'd rather than written out as JSON numbers.
///
/// Tauri hands a command's return value to the webview as JSON, and a float spelled out as
/// text costs about fifteen characters and a parse. A real bike is a few hundred thousand of
/// them: measured at 5.9 MB of text for a small one, 12 ms to encode here and ~20 ms for the
/// webview to `JSON.parse` — and 147 ms of parsing alone for a mesh eight times the size,
/// which an ordinary detailed bike or a gear mesh reaches.
///
/// Base64 of the raw little-endian bytes is 4/3 of the binary size instead of ~10x, and the
/// webview adopts it into a typed array with one decode rather than parsing every number.
/// `src/api/mods.ts` does that decode, so nothing downstream sees a string.
mod mesh_b64 {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::Serializer;

    pub fn f32s<S: Serializer>(v: &[f32], s: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::with_capacity(v.len() * 4);
        for f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        s.serialize_str(&STANDARD.encode(&bytes))
    }

    pub fn u32s<S: Serializer>(v: &[u32], s: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::with_capacity(v.len() * 4);
        for n in v {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        s.serialize_str(&STANDARD.encode(&bytes))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdfNode {
    pub name: String,
    #[serde(serialize_with = "mesh_b64::f32s")]
    pub positions: Vec<f32>, // 3 * vcount, local space
    #[serde(serialize_with = "mesh_b64::f32s")]
    pub uvs: Vec<f32>, // 2 * vcount
    #[serde(serialize_with = "mesh_b64::f32s")]
    pub normals: Vec<f32>, // 3 * vcount
    #[serde(serialize_with = "mesh_b64::u32s")]
    pub indices: Vec<u32>, // 3 * kept triangles
    pub submeshes: Vec<Submesh>,
    pub texture: Option<String>, // node-wide texture, used when submeshes is empty
    // True once positions are in the part's .geom LOCAL frame rather than raw authored space.
    #[serde(skip)]
    pub placed: bool,
    // This node's OWN material table: local material id -> position in the model's colour
    // textures, None where the material is untextured. Every node carries its own, so the
    // same id means different textures in different parts of one mesh.
    #[serde(skip)]
    pub materials: Vec<Option<usize>>,
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

/// How far from the origin a vertex may sit and still look like one.
///
/// A bike is two metres and a rider less, so a tight bound is most of what tells a real
/// vertex block from a run of bytes that happens to parse as floats.
const MODEL_EXTENT: f32 = 200.0;

/// The same, for models built at track scale.
///
/// A track's sky dome reaches 1.5 km from its centre and its backdrop 750 m — every vertex in
/// them fails the model bound, so under it a whole sky reads as no geometry at all.
const WORLD_EXTENT: f32 = 100_000.0;

fn finite_pos(b: &[u8], o: usize, extent: f32) -> bool {
    (0..3).all(|k| {
        let v = f32le(b, o + 4 * k);
        v.is_finite() && v.abs() < extent
    })
}

/// The file's own bounds, as `(min, max)` in authored space — the six floats behind the
/// magic. Written by the exporter over the *placed* mesh, so it is the one statement in the
/// file about where the geometry ends up, and the way to tell a placement that ran twice
/// from one that ran once. Covers every node and LOD in the file.
pub fn header_aabb(b: &[u8]) -> Option<([f32; 3], [f32; 3])> {
    if b.len() < HEADER_START || &b[0..4] != b"EDF\0" {
        return None;
    }
    let f = |i: usize| f32le(b, 4 + i * 4);
    let (lo, hi) = ([f(0), f(1), f(2)], [f(3), f(4), f(5)]);
    (0..3)
        .all(|k| lo[k].is_finite() && hi[k].is_finite() && lo[k] <= hi[k])
        .then_some((lo, hi))
}

/// Whether these bytes are an `.edf` at all. A file that arrives sealed, damaged or half
/// downloaded fails here, before any of the parse below is worth attempting.
pub fn is_edf(b: &[u8]) -> bool {
    b.len() >= HEADER_START + 8 && &b[0..4] == b"EDF\0"
}

// Parse an .edf into its renderable mesh nodes (highest-detail LOD of each part).
pub fn parse(b: &[u8]) -> Vec<EdfNode> {
    parse_impl(b, &[], false, MODEL_EXTENT)
}

/// Parse a model built at track scale — a sky dome, a backdrop, a grandstand.
///
/// Identical to [`parse`] but for the bound a vertex has to sit inside. Everything a track
/// ships is placed in world metres, so the tight bound that protects a bike parse throws the
/// whole model away.
pub fn parse_world(b: &[u8]) -> Vec<EdfNode> {
    parse_impl(b, &[], false, WORLD_EXTENT)
}

/// Parse a gear mesh: as [`parse`], but a node whose geometry is a single group takes its
/// orientation once rather than twice — see [`submesh_transform`].
///
/// Every gear mesh checked lands exactly on the bounds its own header states this way,
/// including the chains that make up most of the protection slot and which were otherwise
/// drawn 35 cm below the rider. Bikes and rider bodies keep the older reading: it moves their
/// fork and swingarm too, and re-checking a whole bike's assembly against these bounds is
/// its own piece of work.
pub fn parse_gear(b: &[u8]) -> Vec<EdfNode> {
    parse_impl(b, &[], true, MODEL_EXTENT)
}

// Parse keeping exactly the nodes the bike's .hrc declares as level0; empty slice
// falls back to level0_only's name heuristic.
pub fn parse_with_levels(b: &[u8], level0: &[String]) -> Vec<EdfNode> {
    parse_impl(b, level0, false, MODEL_EXTENT)
}

fn parse_impl(b: &[u8], level0: &[String], node_matrix_once: bool, extent: f32) -> Vec<EdfNode> {
    let n = b.len();
    if !is_edf(b) {
        return Vec::new();
    }
    let mut nodes = Vec::new();
    let cands = collect_sub_cands(b);
    // Only needed to bound a material's texture index, so count them once.
    let textures = embedded_textures(b).len();
    let mut o = HEADER_START;

    while o + 8 <= n {
        let vc = u32le(b, o) as usize;
        if (8..=MAX_COUNT).contains(&vc) && o + 4 + vc * STRIDE + 8 <= n {
            let vs = o + 4;
            let samples = [0usize, 1, 2, vc / 2, vc - 1];
            if samples.iter().all(|&i| finite_pos(b, vs + i * 12, extent)) {
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
                        // `o` is the node's vertex-count word — its material table ends there.
                        let mats = node_material_table(b, o, textures);
                        nodes.push(read_node(
                            b,
                            &cands,
                            vs,
                            vc,
                            raw,
                            iend,
                            tc,
                            name,
                            mats,
                            node_matrix_once,
                        ));
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
    if !nodes
        .iter()
        .any(|n| want.contains(&n.name.to_ascii_lowercase()))
    {
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

// Where every part of a bike hangs off, resolved from the .geom once.
struct Mounts {
    // Rake tilts the steering head back, i.e. toward -Z (front is +Z). Degrees, as rot_x takes.
    rake: f32,
    head: [f32; 3],
    pivot: [f32; 3],
    steer_joint: [f32; 3],
    rsusp_joint: [f32; 3],
    fork_origin: [f32; 3],
}

fn mounts(
    g: &std::collections::HashMap<String, [f32; 3]>,
    sc: &std::collections::HashMap<String, f32>,
) -> Option<Mounts> {
    let head = *g.get("chassis_steer")?;
    let pivot = *g.get("chassis_rsusp_min")?;
    let steer_joint = *g.get("steer_joint")?;
    let rsusp_joint = *g.get("rsusp_joint")?;
    let front_upper = *g.get("front_upper")?;
    let rake = -sc.get("rakeangle_min").copied().unwrap_or(0.0);
    let fork_origin = v_add(rot_x(v_sub(front_upper, steer_joint), rake), head);
    Some(Mounts {
        rake,
        head,
        pivot,
        steer_joint,
        rsusp_joint,
        fork_origin,
    })
}

impl Mounts {
    /// (front, rear) axle, in the assembled bike's frame.
    ///
    /// Front is the fork's `fwheel` point carried down the raked fork. Rear is the swingarm's,
    /// taken at the midpoint of the chain-adjuster range the .geom gives as `rwheel_min`/
    /// `rwheel_max`. `None` when the .geom names neither — a bike we can still assemble but
    /// can't say where the wheels ride on.
    fn axles(
        &self,
        g: &std::collections::HashMap<String, [f32; 3]>,
    ) -> Option<([f32; 3], [f32; 3])> {
        let fwheel = *g.get("fwheel")?;
        let lo = *g.get("rwheel_min")?;
        let hi = g.get("rwheel_max").copied().unwrap_or(lo);
        let rear = [
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        ];
        Some((
            v_add(rot_x(fwheel, self.rake), self.fork_origin),
            v_add(rear, v_sub(self.pivot, self.rsusp_joint)),
        ))
    }
}

/// The joints an assembled bike can be posed about, in the frame its vertices come back in.
///
/// The .geom places the parts in the frame the bike was *authored* in, and says nothing about
/// where the suspension rides at rest — there is no travel in the file, only a setup range
/// (`chassis_rsusp_min`/`_max`, tenths of a millimetre apart) and the chain-adjuster slot
/// (`rwheel_min`/`_max`). Ride height falls out of the physics, which the viewer doesn't run.
/// So the viewer is handed the joints instead and lets the user move them: the swingarm turns
/// about `pivot`, the fork slides along the axis `rake` tilts through `steer_head`, and the
/// axles say where the wheels ride so a pose can be solved for level.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BikeRig {
    /// Swingarm pivot.
    pub pivot: [f32; 3],
    /// A point on the steering axis (the head itself).
    pub steer_head: [f32; 3],
    /// Rake, in degrees, tilting the steering axis back from vertical.
    pub rake: f32,
    pub front_axle: Option<[f32; 3]>,
    pub rear_axle: Option<[f32; 3]>,
    /// Where a rider sits, from the .geom's `seat_height_ref`. `None` when it names none.
    ///
    /// A setup reference rather than a point on the mesh, but it is the bike's own statement
    /// of where the seat is and it is written in the same frame as every mount above — which
    /// is what lets the viewer stand a rider on it instead of guessing at a height.
    pub seat: Option<[f32; 3]>,
}

impl BikeRig {
    /// Shift every point by the same amount the vertices were shifted by when the assembled
    /// bike was centred, so the rig and the mesh stay in one frame.
    fn recentre(&mut self, c: [f32; 3]) {
        self.pivot = v_sub(self.pivot, c);
        self.steer_head = v_sub(self.steer_head, c);
        self.front_axle = self.front_axle.map(|p| v_sub(p, c));
        self.rear_axle = self.rear_axle.map(|p| v_sub(p, c));
        self.seat = self.seat.map(|p| v_sub(p, c));
    }

    /// Follow the mesh into three.js' frame — see [`to_right_handed`], which must have been
    /// run on the nodes for this to be the right thing to do.
    ///
    /// Points mirror; `rake` doesn't. The mirror negates x alone, and both the fork axis
    /// (x = 0) and the rotation about it act purely on y/z, so the angle carries over as it
    /// stands and reads as a three.js `rotation.x` of the same sign.
    pub fn to_right_handed(&mut self) {
        self.pivot[0] = -self.pivot[0];
        self.steer_head[0] = -self.steer_head[0];
        if let Some(p) = self.front_axle.as_mut() {
            p[0] = -p[0];
        }
        if let Some(p) = self.rear_axle.as_mut() {
            p[0] = -p[0];
        }
        if let Some(p) = self.seat.as_mut() {
            p[0] = -p[0];
        }
    }
}

// Assemble a bike's parts onto its chassis via the .geom mount points, then centre
// on the origin. `None` (nodes untouched) if the .geom lacks the mounts.
/// Where a bike's axles land, for a caller deciding whether it has anywhere to hang wheels.
///
/// The same two points [`BikeRig`] carries, asked for before the bike is assembled: a bike
/// ships no wheel of its own — the mesh comes from the tyres mod its `gfx.cfg` names — and
/// reading that mod is only worth doing once there is somewhere to put what's in it. `None`
/// when the .geom is missing a mount, where the wheels are better left off than dropped on
/// the origin.
pub fn wheel_axles(geom_bytes: &[u8]) -> Option<([f32; 3], [f32; 3])> {
    let g = parse_geom(geom_bytes);
    mounts(&g, &parse_geom_scalars(geom_bytes))?.axles(&g)
}

pub fn assemble_bike(nodes: &mut [EdfNode], geom_bytes: &[u8]) -> Option<BikeRig> {
    let g = parse_geom(geom_bytes);
    let sc = parse_geom_scalars(geom_bytes);
    let Some(m) = mounts(&g, &sc) else {
        return None;
    };
    let (head, pivot, steer_joint, rsusp_joint, rake) =
        (m.head, m.pivot, m.steer_joint, m.rsusp_joint, m.rake);
    let fork_origin = m.fork_origin;
    let axles = m.axles(&g);
    let mut rig = BikeRig {
        pivot,
        steer_head: head,
        rake,
        front_axle: axles.map(|(f, _)| f),
        rear_axle: axles.map(|(_, r)| r),
        // In the chassis' own frame, like `chassis_steer` and `chassis_rsusp_min` beside it,
        // so it needs no carrying down a fork or a swingarm the way the axles do.
        seat: g.get("seat_height_ref").copied(),
    };

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
        // A wheel is authored about its own axle, and how far it has spun is arbitrary —
        // so it only ever wants moving, never turning.
        } else if name.starts_with("fwheel") {
            match axles {
                Some((front, _)) => (0.0, front),
                None => continue,
            }
        } else if name.starts_with("rwheel") {
            match axles {
                Some((_, rear)) => (0.0, rear),
                None => continue,
            }
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
        return Some(rig);
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
    rig.recentre(c);
    Some(rig)
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
        // A colon is a namespace prefix an exporter left on — a track's sky dome comes out of
        // Maya as `tmp1:dome`, and rejecting the name threw the whole sky away.
        if !(c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b':')) {
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

// A node's OWN material table sits immediately before its vertex block, with no padding:
// | u32 count | count records |, each 56 bytes holding six shading terms in (0,1] and, at
// +44, a ONE-BASED index into the model's COLOUR textures (0 = untextured).
//
// There is no global table. What looks like one at 0x1c is simply the FIRST node's, because
// node 0's vertex block starts right where it ends — reading it as the whole model's put the
// blank number-plate overlay over other parts' bodywork. Material ids are LOCAL to their
// node, so the same id means different textures in different parts of one mesh: the Alta's
// chassis reads material 3 as its battery pack where its steering reads 3 as something else,
// and the Husqvarna TC 250 and KTM 125 SX share a swingarm yet embed their textures in
// different orders, which only a per-node table can express.
const MAT_STRIDE: usize = 56;
const MAT_TEX_FROM_REC: usize = 44;
const MAX_MATERIALS: usize = 64;

// A 56-byte material record: two bracketing 0.0 floats around six shading terms, a run of
// zeroed words, and the one-based texture index. Strict on purpose — the table is found by
// walking backwards, so a loose test would latch onto compressed texture bytes.
//
// `textures` is every embedded texture, companions included, because a material may name
// one: the Bell Moto 10's fourth material points at `Racecraft_n`, its goggle's normal map.
// Bounding the index by the colour list instead threw that whole table away, and with it
// the evidence every *other* piece of that helmet is bound from.
fn valid_material_record(b: &[u8], o: usize, textures: usize) -> bool {
    if o + MAT_STRIDE > b.len() {
        return false;
    }
    let w = |i: usize| u32le(b, o + i * 4);
    let f = |i: usize| f32le(b, o + i * 4);
    if w(0) != 0 || w(7) != 0 {
        return false;
    }
    if (1..7).any(|k| !(f(k) > 0.0 && f(k) <= 1.0)) {
        return false;
    }
    if w(8) != 0 || w(9) != 0 || w(10) != 0 || w(12) != 0 {
        return false;
    }
    // w11 is the colour texture (1-based, 0 for none); w13 the companion beside it, likewise
    // 0 where there is none. PiBoSo's own bikes leave w13 zero, so it read as padding — but a
    // mod that ships `_n`/`_s`/`_r` sheets fills it in, and demanding zero threw out the whole
    // table with it, leaving every submesh unbound and the bike grey.
    w(11) as usize <= textures && w(13) as usize <= textures
}

/// One node's material table: LOCAL material id -> position in the model's declared colour
/// textures (see [`declared_colors`]), None where that material carries no texture. Empty
/// when the node has no readable table (shadow meshes, which are never painted).
///
/// `node_start` is the offset of the node's `u32` vertex count; the table ends exactly there.
/// `textures` is how many textures the model embeds — only a bound on the index, since the
/// slot a material names may be a texture the model declares without embedding.
pub fn node_material_table(b: &[u8], node_start: usize, textures: usize) -> Vec<Option<usize>> {
    for count in 1..=MAX_MATERIALS {
        let Some(o) = node_start.checked_sub(4 + MAT_STRIDE * count) else {
            break;
        };
        if u32le(b, o) as usize != count {
            continue;
        }
        if (0..count).all(|k| valid_material_record(b, o + 4 + MAT_STRIDE * k, textures)) {
            return (0..count)
                .map(|k| {
                    let one = u32le(b, o + 4 + MAT_STRIDE * k + MAT_TEX_FROM_REC) as usize;
                    one.checked_sub(1)
                })
                .collect();
        }
    }
    Vec::new()
}

/// The COLOUR textures a model declares, by name, in file order — the list material indices
/// count. Companion maps are left out: the TLD SE4 helmet embeds `TLDSE4`, `TLDSE4_n`,
/// `TLDSE4goggle`, `TLDSE4goggle_n` and binds its goggles with material 2, which is only the
/// goggle sheet if the `_n` between them doesn't take a slot.
///
/// `external` names textures the model draws but does *not* embed — a paint supplies their
/// pixels. They hold a slot all the same, and it is the slot's *position* that matters: the
/// Bell Moto 10 ships no shell texture in its mesh, only the name of the one its paints
/// supply, written ahead of the three it does embed. Counting the embedded three alone slid
/// every material down one, and its goggles came out wearing the helmet's paint.
///
/// A name is only taken as a declaration where the model writes it in the clear — outside
/// any texture payload, and terminated — so a name that happens to occur inside compressed
/// pixels can't invent a slot.
pub fn declared_colors(b: &[u8], external: &[String]) -> Vec<String> {
    let embedded = embedded_textures(b);
    let mut slots: Vec<(usize, String)> = embedded
        .iter()
        .filter(|t| !is_companion_texture(&t.name))
        .map(|t| (t.data_off, t.name.clone()))
        .collect();
    for name in external {
        if name.is_empty()
            || embedded.iter().any(|t| t.name.eq_ignore_ascii_case(name))
            || is_companion_texture(name)
        {
            continue;
        }
        if let Some(at) = declaration_offset(b, name, &embedded) {
            slots.push((at, name.clone()));
        }
    }
    slots.sort_by_key(|(at, _)| *at);
    slots.into_iter().map(|(_, name)| name).collect()
}

/// Where the model writes `name` as a texture it draws but doesn't embed, if it does.
fn declaration_offset(b: &[u8], name: &str, embedded: &[EmbeddedTexture]) -> Option<usize> {
    let needle = name.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = b[from..].windows(needle.len()).position(|w| w == needle) {
        let at = from + rel;
        let terminated = b.get(at + needle.len()) == Some(&0);
        // The declaration is a word followed by the name, so the byte in front is that
        // word's zero high byte — a much narrower target than "not a letter".
        let starts_clean = at > 0 && b[at - 1] == 0;
        let in_pixels = embedded
            .iter()
            .any(|t| at >= t.data_off && at < t.data_off + t.data_len);
        if terminated && starts_clean && !in_pixels {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// Companion maps ride alongside a colour texture and are never the look itself — MX
/// Bikes names them `_n` normal, `_s` specular, `_r` reflection.
///
/// Mods baked in Substance or Blender keep those maps under the exporter's own names
/// instead (`Vest_Normal` beside `Vest_BaseColor`). Counting one of those as a colour
/// texture doesn't just add a stray entry: material indices count this list, so every
/// texture after it slides onto the wrong mesh — which is how the Tactical Vest came out
/// wearing its pouch's normal map.
pub fn is_companion_texture(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    // MX Bikes' own single-letter convention, plus the exporter names.
    n.ends_with("_n") || n.ends_with("_s") || n.ends_with("_r") || is_exporter_companion(&n)
}

/// Companion maps under the names Substance and Blender give them, shared by every filter
/// that has to tell a map from the look. Colour channels (`_basecolor`, `_diffuse`,
/// `_albedo`) are deliberately absent — those ARE the look.
pub fn is_exporter_companion(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "_normal",
        "_nrm",
        "_roughness",
        "_metallic",
        "_metalness",
        "_specular",
        "_glossiness",
        "_ao",
        "_ambientocclusion",
        "_bump",
        "_height",
        "_displacement",
        "_opacity",
    ]
    .iter()
    .any(|suffix| n.ends_with(suffix))
}

/// A model's COLOUR textures, in file order. Material indices count these — including the
/// gfx-referenced ones (chain, `w_plate`) — so dropping any shifts every later material
/// onto the wrong texture, which is why the filter lives in one place.
pub fn color_textures(b: &[u8]) -> Vec<EmbeddedTexture> {
    embedded_textures(b)
        .into_iter()
        .filter(|t| !is_companion_texture(&t.name))
        .collect()
}

// Record layout from `width`: | width u32 | height u32 | md5[16] | u32 | data_size u32 | pad[8] | data |
// data_size counts the 8 pad bytes, so payload = data_size - 8.
const TEX_SIZE_FROM_W: usize = 28;
const TEX_PAD_FROM_W: usize = 32;
const TEX_DATA_FROM_W: usize = 40;
const TEX_PAD_LEN: usize = 8;
// Name's first char to `width`: either -100 or -104 depending on the record; probe both.
const TEX_W_FROM_NAME: [usize; 2] = [100, 104];

// A null-terminated embedded-texture name at `o`: 1-39 name-safe chars (may lead with a digit).
//
// One character counts: the Bell Moto 10 "O" pack names its goggle sheet `O`, and skipping it
// didn't just lose that texture — material indices count this list, so every slot after it
// slid, and the helmet came out wearing its goggle's paint. The record's shape (both
// dimensions from the fixed size set, eight zero pad bytes, a payload that fits the file) is
// what rules out a stray letter, not the length of the name.
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
    (1..=39)
        .contains(&len)
        .then(|| String::from_utf8_lossy(&b[o..e]).into_owned())
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
    // Bounded, because `data_len` is a *compressed* length and nothing in the record says what
    // it expands to. Reading the stream whole let anything that inflates to gigabytes do
    // exactly that before the size check below threw it away — and the records these come from
    // are found by scanning for a byte pattern, so a false positive is not a hostile file, it
    // is Tuesday. Everything past `expected` was truncated anyway.
    std::io::Read::take(
        flate2::read::DeflateDecoder::new(&b[t.data_off..t.data_off + t.data_len]),
        expected as u64,
    )
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
    materials: Vec<Option<usize>>,
    // See `submesh_transform`: whether a one-group node takes its orientation once.
    node_matrix_once: bool,
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

    // A group can carry SEVERAL materials as contiguous ranges — a fork leg and the
    // plastic guard strapped to it, a triple clamp and the front fender, a skinned rider
    // body and its kit. Merging those into one submesh makes every range wear the first
    // range's texture (the KX250's front fender comes out in bare metal), so split them
    // back out and let each bind its own. Ranges number upward from the group's material.
    let raw_subs: Vec<RawSub> = detect_submeshes(b, cands, iend, raw_tris, vc)
        .into_iter()
        .flat_map(|s| {
            let ranges =
                read_sub_group_ranges(b, s.block_off, raw_tris, vc).filter(|r| r.len() > 1);
            let Some(ranges) = ranges else { return vec![s] };
            // Each range names its own material in the word just BEFORE its 24-byte entry:
            // range 0 takes the word at `block_off - 4`, and every later range takes the
            // second word of the previous entry's tail. They do NOT simply count upward from
            // the group's first material — assuming they did put the swingarm's guard on the
            // metals sheet and its body on the plastics one, exactly swapped.
            ranges
                .into_iter()
                .enumerate()
                .map(|(i, (ts, tc, vs2, vc2))| RawSub {
                    name: s.name.clone(),
                    tri_start: ts,
                    tri_count: tc,
                    block_off: s.block_off,
                    vert_start: vs2,
                    vert_count: vc2,
                    mat: (s.block_off + 24 * i).checked_sub(4).map(|o| u32le(b, o)),
                })
                .collect()
        })
        .collect();
    // Covers the node when the submesh triangle counts sum to the raw total.
    let covers =
        !raw_subs.is_empty() && raw_subs.iter().map(|s| s.tri_count).sum::<usize>() == raw_tris;

    // Place the geometry: each submesh's own transform composed with the node
    // orientation matrix (at iend, the name offset) yields its .geom LOCAL frame.
    // Vertices not listed in the table stay unplaced; placed_vert tracks coverage so
    // a triangle spanning both frames is dropped below.
    let mut placed_vert = vec![false; vc];
    let mut placed = false;
    let skip_place = std::env::var_os("MXB_NO_PLACE").is_some(); // dev: render raw authored space
    if let (false, Some(node_mat)) = (skip_place, read_mat4(b, iend + NODE_MAT_OFF)) {
        placed = true;
        // A node with no submesh table is one piece in one frame, so the node matrix alone
        // places it — over every vertex, there being no per-group ranges to walk.
        //
        // Skipping those used to be harmless-looking, because a one-piece mesh usually
        // *is* authored where it belongs and its matrix is identity: the game's own helmet
        // reads the same either way. It isn't always. A one-piece helmet authored in a
        // rotated frame carries the rotation in that matrix, and left unapplied it renders
        // lying on its side. The `.edf` header's own AABB is the check — see
        // `placed_chassis_matches_header_aabb` and the gear diagnostic: placed geometry
        // has to land where the file says the model sits.
        let ranges: Vec<(std::ops::Range<usize>, Vec<Mat4>)> = if raw_subs.is_empty() {
            vec![(0..vc, Vec::new())]
        } else {
            raw_subs
                .iter()
                .map(|s| {
                    let hi_v = (s.vert_start + s.vert_count).min(vc);
                    (
                        s.vert_start..hi_v,
                        submesh_transform(b, iend, s.block_off, node_matrix_once),
                    )
                })
                .collect()
        };
        for (verts, chain) in ranges {
            for i in verts {
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
                    mat: s
                        .mat
                        .or_else(|| s.block_off.checked_sub(4).map(|o| u32le(b, o))),
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
        materials,
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
fn read_sub_group(
    b: &[u8],
    o: usize,
    tot_tris: usize,
    tot_verts: usize,
) -> Option<(usize, usize, usize, usize)> {
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
    let in_bounds = |c: &SubCand| {
        c.tri_start + c.tri_count <= tot_tris && c.vert_start + c.vert_count <= tot_verts
    };
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
fn detect_submeshes_window(
    b: &[u8],
    iend: usize,
    tot_tris: usize,
    tot_verts: usize,
) -> Vec<RawSub> {
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
        let Some(&(o, cnt, vstart, vcnt)) = pick else {
            break;
        };
        let name = if o >= 252 {
            read_cname(b, o - 252)
        } else {
            String::new()
        };
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
fn submesh_transform(
    b: &[u8],
    name_off: usize,
    block_off: usize,
    node_matrix_once: bool,
) -> Vec<Mat4> {
    let mut chain = Vec::new();
    let Some(base) = block_off.checked_sub(SUB_MAT_BACK) else {
        return chain;
    };
    // A node's first geometry group has no matrix of its own: its block sits at name+252, so
    // `base` lands back inside the node matrix at name+104, and reading it there applies that
    // orientation a second time. Invisible where it's identity — every mesh the game itself
    // ships — and 35 cm out of place on a chain exported from Blender, which is most of the
    // protection slot. The parent walk below already stops at this same boundary.
    //
    // Gear only ([`parse_gear`]): the same correction moves a bike's fork and swingarm, which
    // is right by their own headers but needs the whole assembly re-checked against it.
    if node_matrix_once && base < name_off + NODE_MAT_END {
        return chain;
    }
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

// ── The rider's rig ───────────────────────────────────────────────────────────

/// One bone of a rider's rig.
///
/// A rider `.edf` carries a skeleton alongside its mesh — 98 bones on every model the game
/// ships or a modder builds on, named `riderRIG_Pelvis`, `riderRIG_LeftElbow` and so on. The
/// game references those names itself, in each rider's `gfx.cfg`, to hang the helmet off the
/// head and the boots off the knees.
///
/// Only the bones the mesh actually binds to are returned: 64 of the 98 on the riders the game
/// ships. The rest are markers — every `_end` tip, and the ankles and toes, which belong to the
/// boots rather than the body and so carry no region of this mesh.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bone {
    pub name: String,
    /// Index into the same list. `None` for the root, and only for the root.
    pub parent: Option<usize>,
    /// Bone space → model space, at rest.
    pub bind: Mat4,
    /// Model space → bone space, at rest. Stored in the file; `bind` is derived from it.
    pub inv_bind: Mat4,
    /// The slice of the mesh this bone covers, **in bone space** — so it must be taken through
    /// `bind` before it means anything in the model's frame.
    pub aabb_lo: [f32; 3],
    pub aabb_hi: [f32; 3],
}

impl Bone {
    /// Where the bone sits at rest, in model space.
    pub fn origin(&self) -> [f32; 3] {
        [self.bind[3], self.bind[7], self.bind[11]]
    }
}

// A record runs `[marker][matrix ×1 or ×2][index words][AABB]`, and the name of the bone it
// belongs to sits *before* it — the name that trails a record is the next bone's. The marker
// says how many matrices follow; with two, the second is the inverse bind.
//
// The index words are variable-length and not all zero at the end — `Rider+` ends every run
// with three zero words, `Rider+RolledUp` puts a count in the last of them — so the record is
// closed by finding the box instead: 24 bytes that read as a real AABB, immediately followed
// by a name.
const BONE_ONE_MATRIX: u32 = 0x1000;
const BONE_AABB_BACK: usize = 24;
// The longest index-word run seen is the root's, at 22 words.
const BONE_FIELDS_MAX: usize = 200;

struct BoneBlock {
    /// The name that follows this record — which belongs to the *next* bone, not this one.
    next_name: String,
    inv_bind: Option<Mat4>,
    aabb_lo: [f32; 3],
    aabb_hi: [f32; 3],
    name_end: usize,
}

/// Read the rig out of a rider `.edf`. Bikes and gear carry none, and give an empty list.
///
/// Bones come back in file order, which is depth-first from the root.
pub fn parse_skeleton(b: &[u8]) -> Vec<Bone> {
    if b.len() < HEADER_START || &b[0..4] != b"EDF\0" {
        return Vec::new();
    }
    let mut blocks: Vec<BoneBlock> = Vec::new();
    let mut o = 0usize;
    while o + 16 <= b.len() {
        match read_block(b, o) {
            Some(bl) => {
                o = bl.name_end;
                blocks.push(bl);
            }
            // A name is variable-length, so the next record starts at no fixed step from
            // this one — walk a byte at a time. The marker word turns nearly every offset
            // away immediately.
            None => o += 1,
        }
    }
    // Pair each name with the record that follows it, and keep the bones that bind.
    //
    // Stop at the first name that comes round again. The game's own riders store the whole rig
    // once per level of detail — `default_mx` and `default_sm` both hold three copies of the
    // same 64 bones, each with its own boxes — and the first copy is the one that goes with the
    // LOD0 mesh the viewer draws. Reading past it gave every bone two namesakes to hang off,
    // and the tree built from the names below closed into a cycle.
    let mut names = Vec::new();
    let mut binds = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for pair in blocks.windows(2) {
        let (Some(inv_bind), name) = (pair[1].inv_bind, &pair[0].next_name) else {
            continue;
        };
        if !seen.insert(name.clone()) {
            break;
        }
        names.push(name.clone());
        binds.push((inv_bind, pair[1].aabb_lo, pair[1].aabb_hi));
    }
    let parents = rig_parents(&names);
    names
        .into_iter()
        .zip(binds)
        .zip(parents)
        .map(|((name, (inv_bind, aabb_lo, aabb_hi)), parent)| Bone {
            name,
            parent,
            bind: rigid_inverse(&inv_bind),
            inv_bind,
            aabb_lo,
            aabb_hi,
        })
        .collect()
}

/// One record starting at `o`, or `None` if that isn't one.
///
/// Anchored on the marker word and the matrix that follows it, then read forward to the three
/// zero words, the AABB and the name — the only combination that pins down a variable-length
/// index run without guessing where it ends.
fn read_block(b: &[u8], o: usize) -> Option<BoneBlock> {
    let marker = u32le(b, o);
    let two = match marker {
        BONE_ONE_MATRIX => false,
        0x1800 => true,
        _ => return None,
    };
    let first = o + 4;
    // The first matrix is the bone's own placement relative to its parent. It is never read —
    // the inverse bind that follows already says where the bone is in the model — and it is
    // deliberately *not* required to be rigid: `Rider+RolledUp` scales some of its bones, and
    // insisting on a rigid one there threw away the whole rig.
    if !affine_at(b, first) {
        return None;
    }
    let inv_bind = if two {
        Some(read_mat4(b, first + 64)?)
    } else {
        None
    };
    let fields = first + if two { 128 } else { 64 };
    // Step over the index words: the name is the first thing that reads as one with three zero
    // words and a finite AABB in front of it.
    let mut step = 0usize;
    while step <= BONE_FIELDS_MAX {
        let name_off = fields + step + BONE_AABB_BACK;
        if name_off >= b.len() {
            return None;
        }
        if let Some(bone) = read_tail(b, name_off, inv_bind) {
            return Some(bone);
        }
        step += 4;
    }
    None
}

/// The `[AABB][name]` that closes a record, read backwards from the name.
fn read_tail(b: &[u8], name_off: usize, inv_bind: Option<Mat4>) -> Option<BoneBlock> {
    let aabb = name_off.checked_sub(BONE_AABB_BACK)?;
    let mut corner = [0f32; 6];
    for (i, slot) in corner.iter_mut().enumerate() {
        *slot = f32le(b, aabb + i * 4);
        if !slot.is_finite() || slot.abs() > 10.0 {
            return None;
        }
    }
    // A box, not just six floats: the low corner is low on every axis. Index words that happen
    // to read as small floats almost never do this, and a bone with no region reads all zeros,
    // which passes.
    if (0..3).any(|i| corner[i] > corner[i + 3]) {
        return None;
    }
    let name = bone_name(b, name_off)?;
    Some(BoneBlock {
        name_end: name_off + name.len(),
        next_name: name,
        inv_bind,
        aabb_lo: [corner[0], corner[1], corner[2]],
        aabb_hi: [corner[3], corner[4], corner[5]],
    })
}

/// Is there a placement matrix at `o` — finite, sane in size, bottom row `0, 0, 0, 1`?
///
/// Weaker than [`read_mat4`] on purpose: it admits a matrix that scales as well as turns.
fn affine_at(b: &[u8], o: usize) -> bool {
    if o + 64 > b.len() {
        return false;
    }
    (0..16).all(|i| {
        let v = f32le(b, o + i * 4);
        v.is_finite() && v.abs() < 100.0
    }) && (0..3).all(|i| f32le(b, o + 48 + i * 4) == 0.0)
        && f32le(b, o + 60) == 1.0
}

/// A bone's name, NUL-terminated.
///
/// Not [`plausible_name`]: that caps at 31 characters, which is right for the mesh-node names
/// it was written for and two characters short of `riderRIG_RightShoulderTwist1_end`. Reading
/// the rig with that cap silently drops the two bones whose *records* those names close.
fn bone_name(b: &[u8], o: usize) -> Option<String> {
    if o >= b.len() || !b[o].is_ascii_alphabetic() {
        return None;
    }
    let mut e = o;
    while e < b.len() && e - o < 64 {
        let c = b[e];
        if c == 0 {
            break;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-')) {
            return None;
        }
        e += 1;
    }
    (2..=63)
        .contains(&(e - o))
        .then(|| String::from_utf8_lossy(&b[o..e]).into_owned())
}

/// Rebuild the bone tree from the names.
///
/// The rig is the game's own: every rider model carries the same bones under the same names,
/// and `gfx.cfg` spells several of them out, so the naming is a contract rather than a habit.
/// A bone whose named parent isn't in the list — the ankles and the `_end` markers are dropped
/// before this runs — climbs to the nearest named ancestor that is. Anything the rules don't
/// recognise falls back to the bone listed before it.
///
/// Only a bone earlier in the list is ever taken as a parent. The file lists a rig depth-first,
/// so that holds of every real one, and insisting on it is what keeps the result a tree: a bone
/// inside a cycle hangs off no root, and nothing ever works out where it is.
fn rig_parents(names: &[String]) -> Vec<Option<usize>> {
    let stems: Vec<String> = names
        .iter()
        .map(|n| bone_stem(n).to_ascii_lowercase())
        .collect();
    let index: std::collections::HashMap<&str, usize> = stems
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    (0..names.len())
        .map(|i| {
            if i == 0 {
                return None;
            }
            // Climb past ancestors this model doesn't bind to.
            let named = named_parent(&stems[i]);
            let known = named.is_some();
            let mut want = named;
            for _ in 0..8 {
                match want {
                    None => break,
                    Some(ref w) => match index.get(w.as_str()) {
                        Some(&p) if p < i => return Some(p),
                        _ => want = named_parent(w),
                    },
                }
            }
            // The rig names this joint's parent and this model binds none of that chain —
            // `default_mx_c` binds the limbs and no spine at all. Then it is a root of its
            // own, not whatever happens to sit before it in the file: hanging the right leg
            // off the left one's twist bone makes one hip drag the whole body behind it.
            // A stem the naming rules don't recognise is somebody's own bone, and the file's
            // depth-first order is still the best guess there.
            if known {
                None
            } else {
                Some(i - 1)
            }
        })
        .collect()
}

/// The part of a bone's name that describes the joint: `riderRIG_LeftKnee` → `LeftKnee`.
fn bone_stem(name: &str) -> &str {
    match name.to_ascii_lowercase().find("rig_") {
        Some(at) => &name[at + 4..],
        None => name,
    }
}

/// The name of the bone this one hangs off, by the rig's own naming.
fn named_parent(stem: &str) -> Option<String> {
    // `LeftToe_end` hangs off `LeftToe`, and so does every other tip marker.
    if let Some(base) = stem.strip_suffix("_end") {
        return Some(base.to_string());
    }
    // `LeftHipTwist1` and `LeftKneeTwist` both hang off the joint they twist about.
    if let Some(at) = stem.find("twist") {
        return Some(stem[..at].to_string());
    }
    let (side, joint) = match stem.strip_prefix("left") {
        Some(rest) => ("left", rest),
        None => match stem.strip_prefix("right") {
            Some(rest) => ("right", rest),
            None => ("", stem),
        },
    };
    // The links the rig's own numbering doesn't spell out.
    let fixed = match joint {
        "root" => return None,
        "pelvis" => "root",
        "spine1" => "pelvis",
        "neck1" => "spine4",
        "head" => "neck2",
        "armour" => "spine4",
        "collar" => "neck1",
        "shoulder" => return Some(format!("{side}collar")),
        "elbow" => return Some(format!("{side}shoulder")),
        "wrist" => return Some(format!("{side}elbow")),
        "hip" => "pelvis",
        "knee" => return Some(format!("{side}hip")),
        "ankle" => return Some(format!("{side}knee")),
        "toe" => return Some(format!("{side}ankle")),
        _ => "",
    };
    if !fixed.is_empty() {
        return Some(fixed.to_string());
    }
    // Everything numbered counts down its own chain: `Spine3` → `Spine2`, `LeftIndex2` →
    // `LeftIndex1`. The first link of a finger hangs off the wrist.
    let digit = joint.chars().last().filter(char::is_ascii_digit)?;
    let base = &joint[..joint.len() - 1];
    if digit == '1' {
        return matches!(
            base,
            "thumb" | "index" | "middle" | "ring" | "pink" | "pinky"
        )
        .then(|| format!("{side}wrist"));
    }
    let prev = (digit as u8 - 1) as char;
    Some(format!("{side}{base}{prev}"))
}

/// Put every bone through the same orthogonal transform, in place.
///
/// Used to bring a rig into the frame the viewer draws its mesh in. Both turns the mesh takes
/// go through here — the mirror into right-handed space and, for a Z-up model, the turn that
/// stands it up — and the rig must take exactly the same ones, or the skeleton ends up
/// mirrored, or lying beside a standing body. A mirror is allowed: `r` only has to be
/// orthogonal, not a rotation, and the inverse bind is rebuilt from the result either way.
///
/// The bone-space boxes are left alone: they are already in each bone's own frame, which moves
/// with it.
pub fn transform_skeleton(bones: &mut [Bone], r: [[f32; 3]; 3]) {
    for bone in bones.iter_mut() {
        let m = bone.bind;
        let mut out = [0f32; 16];
        for i in 0..3 {
            for j in 0..4 {
                out[i * 4 + j] = (0..3).map(|k| r[i][k] * m[k * 4 + j]).sum();
            }
        }
        out[15] = 1.0;
        bone.bind = out;
        bone.inv_bind = rigid_inverse(&out);
    }
}

/// How many bones one vertex may be shared between. Four is what every engine that skins on
/// the GPU settles on, and what three.js takes.
pub const SKIN_BONES_PER_VERTEX: usize = 4;

/// The mesh's binding to its rig: four bones and four weights for every vertex, in the same
/// order as the node's `positions`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    /// `4 * vertex count` indices into the bone list.
    pub indices: Vec<u16>,
    /// `4 * vertex count` weights, each vertex's four summing to 1.
    pub weights: Vec<f32>,
}

/// Work out which bones move which vertices.
///
/// The file doesn't say. A rider `.edf` stores its rig and its mesh and nothing that joins
/// them: there is no weight anywhere in the 72 bytes a vertex occupies, and no table beside
/// the rig either — the game rebuilds the binding at load, and so must we.
///
/// What the file *does* give is a box per bone, in that bone's own space, covering the part of
/// the mesh it is responsible for. Those boxes cover the body — barely a vertex in a thousand
/// falls outside all of them — but they overlap far too much to name a bone on their own: nine
/// vertices in ten sit inside three or more. So they are used as a filter rather than an
/// answer, and the choice among the bones that claim a vertex is made on distance: to the limb
/// each bone actually swings, which is the run from that bone to its children, so that the
/// thigh follows the hip and the shin follows the knee.
pub fn skin_mesh(nodes: &[EdfNode], bones: &[Bone]) -> Skin {
    let verts: usize = nodes.iter().map(|n| n.positions.len() / 3).sum();
    let mut skin = Skin {
        indices: vec![0; verts * SKIN_BONES_PER_VERTEX],
        weights: vec![0.0; verts * SKIN_BONES_PER_VERTEX],
    };
    if bones.is_empty() || verts == 0 {
        // Nothing to bind to: hang the whole mesh off bone zero so it still draws.
        for i in 0..verts {
            skin.weights[i * SKIN_BONES_PER_VERTEX] = 1.0;
        }
        return skin;
    }
    let limbs = limb_segments(bones);
    let mut v = 0usize;
    for node in nodes {
        for p in node.positions.chunks_exact(3) {
            let point = [p[0], p[1], p[2]];
            weigh_vertex(&point, bones, &limbs, &mut skin, v);
            v += 1;
        }
    }
    skin
}

/// The run of flesh each bone swings: from the bone to each of its children, or the bone
/// itself where it has none.
fn limb_segments(bones: &[Bone]) -> Vec<Vec<([f32; 3], [f32; 3])>> {
    let mut out: Vec<Vec<([f32; 3], [f32; 3])>> = vec![Vec::new(); bones.len()];
    for bone in bones.iter() {
        if let Some(p) = bone.parent {
            out[p].push((bones[p].origin(), bone.origin()));
        }
    }
    for (i, segs) in out.iter_mut().enumerate() {
        if segs.is_empty() {
            let o = bones[i].origin();
            segs.push((o, o));
        }
    }
    out
}

/// Pick this vertex's four bones and their shares.
fn weigh_vertex(
    point: &[f32; 3],
    bones: &[Bone],
    limbs: &[Vec<([f32; 3], [f32; 3])>],
    skin: &mut Skin,
    v: usize,
) {
    // Closeness, not distance, so the sums below stay well behaved where a vertex sits exactly
    // on a bone. 1 mm is finer than any rider mesh resolves.
    const EPS: f32 = 1e-3;
    let mut best: Vec<(usize, f32)> = Vec::new();
    let mut fallback = (0usize, f32::MAX);
    for (i, bone) in bones.iter().enumerate() {
        let d = limbs[i]
            .iter()
            .map(|(a, b)| point_to_segment(point, a, b))
            .fold(f32::MAX, f32::min);
        if d < fallback.1 {
            fallback = (i, d);
        }
        if !claims(bone, point) {
            continue;
        }
        best.push((i, 1.0 / (d + EPS).powi(2)));
    }
    // A vertex no box claims — a thousandth of them — goes to the nearest limb outright.
    if best.is_empty() {
        best.push((fallback.0, 1.0));
    }
    best.sort_by(|a, b| b.1.total_cmp(&a.1));
    best.truncate(SKIN_BONES_PER_VERTEX);
    let total: f32 = best.iter().map(|(_, w)| w).sum();
    let base = v * SKIN_BONES_PER_VERTEX;
    for (slot, (i, w)) in best.iter().enumerate() {
        skin.indices[base + slot] = *i as u16;
        skin.weights[base + slot] = w / total;
    }
}

/// Does this bone's own box cover the point? The box is in bone space, so the point goes
/// through the inverse bind first.
fn claims(bone: &Bone, point: &[f32; 3]) -> bool {
    if bone.aabb_lo == bone.aabb_hi {
        return false; // a bone with no region of its own claims nothing
    }
    let m = &bone.inv_bind;
    (0..3).all(|a| {
        let p =
            m[a * 4] * point[0] + m[a * 4 + 1] * point[1] + m[a * 4 + 2] * point[2] + m[a * 4 + 3];
        p >= bone.aabb_lo[a] && p <= bone.aabb_hi[a]
    })
}

fn point_to_segment(p: &[f32; 3], a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0)
    };
    let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t, ap[2] - ab[2] * t];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The inverse of a rigid placement: transpose the rotation, and turn the translation back
/// through it.
fn rigid_inverse(m: &Mat4) -> Mat4 {
    let t = [m[3], m[7], m[11]];
    let mut out = [0f32; 16];
    for i in 0..3 {
        for j in 0..3 {
            out[i * 4 + j] = m[j * 4 + i];
        }
        out[i * 4 + 3] = -(0..3).map(|k| m[k * 4 + i] * t[k]).sum::<f32>();
    }
    out[15] = 1.0;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mesh arrays cross as base64 of their raw bytes, and the webview reads them back as
    /// typed arrays. A silent mismatch here would not fail anything — it would draw a bike
    /// made of noise — so the round trip is asserted rather than assumed.
    #[test]
    fn mesh_arrays_survive_the_base64_round_trip() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let node = EdfNode {
            name: "part".into(),
            // Values a naive encoder gets wrong: negatives, fractions, and a payload length
            // that isn't a multiple of three.
            positions: vec![0.0, -1.5, 2.25, f32::MIN_POSITIVE, -0.1, 1e6],
            uvs: vec![0.0, 1.0, 0.5, 0.25],
            normals: vec![0.0, 1.0, 0.0],
            indices: vec![0, 1, 2, u32::MAX],
            submeshes: Vec::new(),
            texture: None,
            placed: false,
            materials: Vec::new(),
        };

        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&node).unwrap()).unwrap();

        let floats = |key: &str| -> Vec<f32> {
            let b = STANDARD.decode(v[key].as_str().unwrap()).unwrap();
            b.chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect()
        };
        assert_eq!(floats("positions"), node.positions);
        assert_eq!(floats("uvs"), node.uvs);
        assert_eq!(floats("normals"), node.normals);

        let idx: Vec<u32> = STANDARD
            .decode(v["indices"].as_str().unwrap())
            .unwrap()
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(idx, node.indices);
    }

    /// A record that claims to be a small texture but whose payload inflates to far more than
    /// that. Everything past `width * height * 4` was always thrown away — the bug was that it
    /// was decoded first, so what the file said it expanded to decided how much memory this
    /// took. 64 MB here stands in for the gigabytes a real one could ask for.
    #[test]
    fn a_payload_that_expands_past_its_texture_is_not_inflated_whole() {
        use flate2::write::DeflateEncoder;
        use std::io::Write;

        let mut enc = DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&vec![0u8; 64 * 1024 * 1024]).unwrap();
        let payload = enc.finish().unwrap();
        assert!(
            payload.len() < 1024 * 1024,
            "the point is that it compresses hard"
        );

        let tex = EmbeddedTexture {
            name: "bomb".into(),
            width: 64,
            height: 64,
            data_off: 0,
            data_len: payload.len(),
        };
        let out = inflate_texture(&payload, &tex).expect("still decodes");
        assert_eq!(
            out.len(),
            64 * 64 * 4,
            "bounded by what the record claims to be"
        );
    }

    // Material indices count the colour textures, so a map counted among them slides every
    // later texture onto the wrong mesh — the Tactical Vest wore its pouch's normal map.
    #[test]
    fn exporter_named_maps_are_companions_not_colour() {
        for name in [
            "Vest_Normal",
            "chest_Roughness",
            "brace_AO",
            "pouch_metallic",
            "shell_n",
        ] {
            assert!(is_companion_texture(name), "'{name}' is a map");
        }
        // The look itself, however it's spelled.
        for name in [
            "Vest_BaseColor",
            "chest_diffuse",
            "CK_A1",
            "bake1",
            "aphair",
        ] {
            assert!(!is_companion_texture(name), "'{name}' is the look");
        }
    }

    // Build a material table of `tex` one-based texture indices, ending at `node_start`.
    fn material_table_bytes(tex: &[u32], node_start: usize) -> Vec<u8> {
        let mut b = vec![0u8; node_start + 4];
        let o = node_start - 4 - MAT_STRIDE * tex.len();
        b[o..o + 4].copy_from_slice(&(tex.len() as u32).to_le_bytes());
        for (k, one_based) in tex.iter().enumerate() {
            let r = o + 4 + MAT_STRIDE * k;
            for s in 1..7 {
                b[r + s * 4..r + s * 4 + 4].copy_from_slice(&1.0f32.to_le_bytes());
            }
            b[r + MAT_TEX_FROM_REC..r + MAT_TEX_FROM_REC + 4]
                .copy_from_slice(&one_based.to_le_bytes());
        }
        b
    }

    #[test]
    fn a_nodes_material_table_is_read_back_off_its_vertex_count() {
        let b = material_table_bytes(&[3, 1, 0], 512);
        // One-based into the declared colours, and 0 means the material carries no texture.
        assert_eq!(
            node_material_table(&b, 512, 3),
            vec![Some(2), Some(0), None]
        );
    }

    /// A model declares more texture slots than it embeds — the Bell Moto 10 leaves its
    /// shell sheet to a `.pnt` and names it, then embeds three more. The table is still this
    /// node's, and throwing it away costs every piece in the mesh its binding.
    #[test]
    fn a_material_may_name_a_slot_past_the_embedded_textures() {
        let b = material_table_bytes(&[1, 4], 512);
        assert_eq!(node_material_table(&b, 512, 4), vec![Some(0), Some(3)]);
    }

    #[test]
    fn a_material_table_is_refused_when_it_overruns_the_textures() {
        // Index 4 with only 3 textures in the file isn't this node's table — better no
        // table than a part bound from a misread one.
        let b = material_table_bytes(&[4], 512);
        assert!(node_material_table(&b, 512, 3).is_empty());
        assert_eq!(node_material_table(&b, 512, 4), vec![Some(3)]);
    }

    #[test]
    fn each_node_keeps_its_own_material_table() {
        // Two nodes, two tables: the same local id must resolve differently.
        let a = material_table_bytes(&[1, 2], 512);
        let c = material_table_bytes(&[2, 1], 512);
        assert_eq!(node_material_table(&a, 512, 2), vec![Some(0), Some(1)]);
        assert_eq!(node_material_table(&c, 512, 2), vec![Some(1), Some(0)]);
    }

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
        eprintln!(
            "overall bbox lo={lo:?} hi={hi:?}  size={:?}",
            [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
        );
        for n in &nodes {
            eprintln!(
                "  node '{}'  verts={}  submeshes={}",
                n.name,
                n.positions.len() / 3,
                n.submeshes.len()
            );
            for sm in &n.submeshes {
                eprintln!(
                    "      submesh '{}'  tris={}  tex={:?}",
                    sm.name, sm.tri_count, sm.texture
                );
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
        assert_eq!(
            read_sub_group(&b, 0, 46184, 35689),
            Some((0, 31846, 0, 24904))
        );
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
            SubCand {
                block_off: 100,
                tri_start: 0,
                tri_count: 9096,
                vert_start: 0,
                vert_count: 6816,
            },
            SubCand {
                block_off: 200,
                tri_start: 9096,
                tri_count: 39214,
                vert_start: 6816,
                vert_count: 28502,
            },
        ];
        let subs = chain_submeshes(&b, &cands, 0, 48310, 35318).expect("chain reconciles");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].tri_start, 0);
        assert_eq!(subs[1].tri_start, 9096);
        assert_eq!(subs.iter().map(|s| s.tri_count).sum::<usize>(), 48310);
        assert_eq!(
            subs.last().unwrap().vert_start + subs.last().unwrap().vert_count,
            35318
        );
    }

    #[test]
    fn rejects_unreconcilable_chain() {
        let b = [0u8; 8];
        let cands = vec![
            SubCand {
                block_off: 100,
                tri_start: 0,
                tri_count: 9096,
                vert_start: 0,
                vert_count: 6816,
            },
            // vert_start doesn't continue 6816 → the chain can't reach it.
            SubCand {
                block_off: 200,
                tri_start: 9096,
                tri_count: 39214,
                vert_start: 9999,
                vert_count: 28502,
            },
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
        assert!(
            !ch.submeshes.is_empty(),
            "chassis must have a submesh table"
        );
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

    /// The mount points of a real bike (MX1OEM_1996_Honda_CR250), trimmed to the lines the
    /// rig is built from. Kept verbatim so the numbers below can be checked against the
    /// bike's published spec rather than against this test.
    const CR250_GEOM: &[u8] = b"type = bike\n\
chassis_steer = 0, 0.9591, 0.3317\n\
chassis_rsusp_min = 0, 0.401, -0.219\n\
rakeangle_min = 27.2\n\
steer_joint = 0, 0.0025, -0.0229\n\
front_upper = 0, -0.4032, -0.0026\n\
fwheel = 0, -0.2046, 0.0147\n\
rsusp_joint = 0, 0.0004, 0.0558\n\
rwheel_min = 0, 0.0118, -0.4985\n\
rwheel_max = 0, 0.0122, -0.5359\n\
seat_height_ref = 0, 0.9115, -0.1674\n";

    /// A node of loose points, already in its part's local frame — the shape `assemble_bike`
    /// mounts. What comes back is where those points landed.
    fn mount_node(name: &str, points: &[[f32; 3]]) -> EdfNode {
        EdfNode {
            name: name.into(),
            positions: points.iter().flatten().copied().collect(),
            uvs: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            submeshes: Vec::new(),
            texture: None,
            placed: true,
            materials: Vec::new(),
        }
    }

    fn vertex(n: &EdfNode, i: usize) -> [f32; 3] {
        [
            n.positions[i * 3],
            n.positions[i * 3 + 1],
            n.positions[i * 3 + 2],
        ]
    }

    fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < tol)
    }

    /// The axles are what a stance is solved against, and nothing in the mesh says where they
    /// are — only the .geom does. Checked against the real bike: a 1996 CR250R's wheelbase is
    /// about 1450 mm.
    #[test]
    fn rig_axles_land_a_real_wheelbase_apart() {
        let mut nodes = [mount_node("chassis", &[[0.0, 0.0, 0.0], [0.0, 1.0, 0.5]])];
        let rig = assemble_bike(&mut nodes, CR250_GEOM).expect("the .geom has every mount");
        let (front, rear) = (rig.front_axle.expect("front"), rig.rear_axle.expect("rear"));
        let wheelbase = front[2] - rear[2];
        assert!(
            (wheelbase - 1.434).abs() < 0.03,
            "wheelbase {wheelbase} m is not a 250's"
        );
        // Both axles on the bike's centreline, and within a wheel's worth of the same height —
        // a rig that had the rear swung out would pass the wheelbase check and nothing else.
        assert!(front[0].abs() < 1e-4 && rear[0].abs() < 1e-4);
        assert!(
            (front[1] - rear[1]).abs() < 0.05,
            "axles {front:?} {rear:?}"
        );
    }

    /// The rig names points *on the mesh*, so it has to survive the centring the mesh gets:
    /// a vertex sitting exactly on the swingarm pivot must still be on `rig.pivot` afterwards.
    /// Off by the centring shift, every pose would swing about a point in mid-air.
    #[test]
    fn rig_lands_in_the_frame_the_mesh_came_back_in() {
        let g = parse_geom(CR250_GEOM);
        // The swingarm's own joint: assembly puts this vertex on the chassis' pivot.
        let joint = g["rsusp_joint"];
        let mut nodes = [
            mount_node("chassis", &[[0.0, 0.0, 0.0], [0.0, 1.2, 0.9]]),
            mount_node("rsusp", &[joint]),
        ];
        let rig = assemble_bike(&mut nodes, CR250_GEOM).expect("assembled");
        assert!(
            close(vertex(&nodes[1], 0), rig.pivot, 1e-4),
            "pivot {:?} left the mesh's {:?}",
            rig.pivot,
            vertex(&nodes[1], 0)
        );
    }

    /// Where a rider sits. The .geom is the only thing that says — nothing in the mesh marks
    /// a seat — and it is written in the chassis' own frame, so a chassis vertex sitting on
    /// `seat_height_ref` has to come back sitting on `rig.seat`. Off by the centring shift,
    /// a rider stood on it would float somewhere over the bike.
    #[test]
    fn the_seat_lands_where_the_geom_puts_it() {
        let g = parse_geom(CR250_GEOM);
        let seat = g["seat_height_ref"];
        let mut nodes = [mount_node(
            "chassis",
            &[[0.0, 0.0, 0.0], [0.0, 1.2, 0.9], seat],
        )];
        let rig = assemble_bike(&mut nodes, CR250_GEOM).expect("assembled");
        let at = rig.seat.expect("the .geom names a seat");
        assert!(
            close(vertex(&nodes[0], 2), at, 1e-4),
            "seat {at:?} left the mesh"
        );
        // And it is a seat: above both axles, and between them rather than out past a wheel.
        let (front, rear) = (rig.front_axle.expect("front"), rig.rear_axle.expect("rear"));
        assert!(
            at[1] > front[1] + 0.3 && at[1] > rear[1] + 0.3,
            "seat {at:?} is not above the axles"
        );
        assert!(
            at[2] < front[2] && at[2] > rear[2],
            "seat {at:?} is not between the wheels"
        );
    }

    /// A .geom with no seat line leaves it unset rather than dropping a rider on the origin.
    #[test]
    fn a_geom_with_no_seat_says_so() {
        let trimmed: Vec<u8> = CR250_GEOM
            .split(|b| *b == b'\n')
            .filter(|l| !l.starts_with(b"seat_height_ref"))
            .flat_map(|l| l.iter().copied().chain(std::iter::once(b'\n')))
            .collect();
        let mut nodes = [mount_node("chassis", &[[0.0, 0.0, 0.0], [0.0, 1.2, 0.9]])];
        assert_eq!(
            assemble_bike(&mut nodes, &trimmed).expect("assembled").seat,
            None
        );
    }

    /// …and the same again through the mirror into three.js' frame.
    #[test]
    fn rig_follows_the_mesh_into_the_right_handed_frame() {
        let g = parse_geom(CR250_GEOM);
        let joint = g["rsusp_joint"];
        let mut nodes = [
            // Off the centreline, so a mirror that did nothing would be caught.
            mount_node("chassis", &[[0.3, 0.0, 0.0], [-0.1, 1.2, 0.9]]),
            mount_node("rsusp", &[[joint[0] + 0.25, joint[1], joint[2]]]),
        ];
        let mut rig = assemble_bike(&mut nodes, CR250_GEOM).expect("assembled");
        let before = rig.pivot;
        to_right_handed(&mut nodes);
        rig.to_right_handed();
        assert!(
            (rig.pivot[0] + before[0]).abs() < 1e-6,
            "x should have flipped"
        );
        assert_eq!(rig.pivot[1], before[1]);
        // The swingarm vertex sits 0.25 m off the pivot either side of the mirror.
        let d = vertex(&nodes[1], 0)[0] - rig.pivot[0];
        assert!(
            (d + 0.25).abs() < 1e-4,
            "mesh and rig disagree after mirroring: {d}"
        );
    }

    /// The mount points of a real bike (MX1OEM_2023_Honda_CRF450R), trimmed to the lines
    /// the wheels are hung off. Kept verbatim so the numbers below can be checked against
    /// the bike's published spec rather than against this test.
    const CRF450R_GEOM: &[u8] = b"type = bike\n\
chassis_steer = 0, 0.9935, 0.2982\n\
chassis_rsusp_min = 0, 0.4599, -0.2118\n\
rakeangle_min = 27.1\n\
steer_joint = 0, 0.0412, -0.0372\n\
front_upper = 0, -0.4048, -0.0153\n\
fwheel = 0, -0.2468, 0.0481\n\
rsusp_joint = 0, -0.0042, 0.0399\n\
rwheel_min = 0, -0.0011, -0.5282\n\
rwheel_max = 0, -0.001, -0.5683\n";

    fn wheel_node(name: &str) -> EdfNode {
        EdfNode {
            name: name.into(),
            // One vertex on the axle: what comes back is where the axle landed.
            positions: vec![0.0, 0.0, 0.0],
            uvs: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            submeshes: Vec::new(),
            texture: None,
            placed: true,
            materials: Vec::new(),
        }
    }

    /// The wheels aren't in the bike's mesh, so the `.geom` is the only thing that says
    /// where they go. Checked against the real bike: a 2023 CRF450R's wheelbase is 1481 mm.
    #[test]
    fn wheel_axles_land_a_real_wheelbase_apart() {
        let (front, rear) = wheel_axles(CRF450R_GEOM).expect("the mounts are all there");
        assert!(
            (front[1] - 0.4086).abs() < 5e-3,
            "front axle height: {front:?}"
        );
        assert!(
            (front[2] - 0.6761).abs() < 5e-3,
            "front axle reach: {front:?}"
        );
        assert!(
            (rear[1] - 0.4631).abs() < 5e-3,
            "rear axle height: {rear:?}"
        );
        assert!((rear[2] + 0.7999).abs() < 5e-3, "rear axle reach: {rear:?}");
        let wheelbase = front[2] - rear[2];
        assert!(
            (wheelbase - 1.481).abs() < 0.02,
            "wheelbase {wheelbase} is nowhere near the real 1.481 m",
        );
        // Both on the bike's centreline, and the front ahead of the rear.
        assert!(front[0].abs() < 1e-6 && rear[0].abs() < 1e-6);
        assert!(front[2] > rear[2]);
    }

    /// The rear axle sits at the midpoint of the chain-adjuster range, which is what the
    /// `.geom`'s own collision notes tell a modder to use. Taking `rwheel_min` alone put
    /// the wheel 20 mm forward of where the bike is measured.
    #[test]
    fn rear_axle_splits_the_chain_adjuster_range() {
        let (_, rear) = wheel_axles(CRF450R_GEOM).unwrap();
        let g = parse_geom(CRF450R_GEOM);
        let mid = (g["rwheel_min"][2] + g["rwheel_max"][2]) * 0.5;
        let off = g["chassis_rsusp_min"][2] - g["rsusp_joint"][2];
        assert!((rear[2] - (mid + off)).abs() < 1e-6, "rear: {rear:?}");
    }

    #[test]
    fn assemble_puts_the_wheels_on_their_axles() {
        let (front, rear) = wheel_axles(CRF450R_GEOM).unwrap();
        // A chassis vertex at the origin, so the centring pass has a third point to work
        // with and the two wheels keep their real separation.
        let mut nodes = vec![
            wheel_node("chassis"),
            wheel_node("fwheel"),
            wheel_node("rwheela"),
        ];
        assert!(assemble_bike(&mut nodes, CRF450R_GEOM).is_some());
        // Everything is shifted by the same centring offset, so compare the gap.
        let at = |i: usize| {
            [
                nodes[i].positions[0],
                nodes[i].positions[1],
                nodes[i].positions[2],
            ]
        };
        let (f, r) = (at(1), at(2));
        for k in 0..3 {
            assert!(
                ((f[k] - r[k]) - (front[k] - rear[k])).abs() < 1e-5,
                "axle {k}: got {:?}, want {:?}",
                (f[k] - r[k]),
                (front[k] - rear[k]),
            );
        }
    }

    /// A `.geom` with no `fwheel`/`rwheel` lines has nowhere to hang a wheel. The bike's
    /// own parts must still assemble — the wheels are the only thing that goes missing.
    #[test]
    fn a_geom_without_wheel_mounts_still_assembles_the_bike() {
        let geom: Vec<u8> = CRF450R_GEOM
            .split(|b| *b == b'\n')
            .filter(|line| !line.starts_with(b"fwheel") && !line.starts_with(b"rwheel"))
            .map(|line| [line, b"\n"].concat())
            .collect::<Vec<_>>()
            .concat();
        assert!(wheel_axles(&geom).is_none());
        let mut nodes = vec![
            wheel_node("chassis"),
            wheel_node("steer"),
            wheel_node("fwheel"),
        ];
        assert!(
            assemble_bike(&mut nodes, &geom).is_some(),
            "the bike still assembles"
        );
        // The steering head moved; the wheel, having no mount, did not.
        assert_ne!(nodes[1].positions, nodes[0].positions, "steer was placed");
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
        if let Some(rs) = nodes
            .iter()
            .find(|n| n.name.to_ascii_lowercase().starts_with("rsusp"))
        {
            let (mut zlo, mut zhi) = (f32::MAX, f32::MIN);
            for p in rs.positions.chunks_exact(3) {
                zlo = zlo.min(p[2]);
                zhi = zhi.max(p[2]);
            }
            eprintln!("rsusp local z [{zlo}, {zhi}]");
            assert!(
                zlo < -0.4,
                "swingarm should reach ~-0.57 rearward, got {zlo}"
            );
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
            eprintln!(
                "tex '{}' {}x{} data@{} len={}",
                t.name, t.width, t.height, t.data_off, t.data_len
            );
        }
        assert!(!texs.is_empty(), "found the model's textures");
        // Every record must inflate to width*height*4 RGBA bytes (_r maps exempt).
        for t in texs.iter().filter(|t| !t.name.ends_with("_r")) {
            let rgba = inflate_texture(&bytes, t).unwrap_or_else(|| panic!("inflate '{}'", t.name));
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
                eprintln!(
                    "    submesh '{}' tri[{}..{})",
                    s.name,
                    s.tri_start,
                    s.tri_start + s.tri_count
                );
            }
            assert!(verts >= 8 && tris >= 1);
        }
    }

    // ── The rig ───────────────────────────────────────────────────────────────

    /// One record: `[marker][matrix ×1 or ×2][index words][0, 0, 0][AABB][name]`. The name that
    /// closes a record belongs to the *next* bone, which is why the builder takes it that way.
    fn bone_block(
        next_name: &str,
        inv_bind: Option<[f32; 16]>,
        words: &[u32],
        lo: [f32; 3],
        hi: [f32; 3],
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(
            &(if inv_bind.is_some() {
                0x1800u32
            } else {
                0x1000
            })
            .to_le_bytes(),
        );
        let local = placed(0.0, 0.0, 0.0);
        for m in std::iter::once(local).chain(inv_bind) {
            for f in m {
                v.extend_from_slice(&f.to_le_bytes());
            }
        }
        for w in words.iter().chain(&[0, 0, 0]) {
            v.extend_from_slice(&w.to_le_bytes());
        }
        for f in lo.iter().chain(&hi) {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(next_name.as_bytes());
        v.push(0);
        v
    }

    fn placed(x: f32, y: f32, z: f32) -> [f32; 16] {
        [
            1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, z, 0.0, 0.0, 0.0, 1.0,
        ]
    }

    /// A bone at `(x, y, z)` stores the transform that takes the model *into* its frame.
    fn bind_at(x: f32, y: f32, z: f32) -> [f32; 16] {
        placed(-x, -y, -z)
    }

    fn rig_file(blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut v = b"EDF\0".to_vec();
        v.resize(HEADER_START, 0);
        for b in blocks {
            v.extend_from_slice(b);
        }
        v
    }

    #[test]
    fn reads_the_rig() {
        // Index-word runs of different lengths, because the real file has both. The first
        // record belongs to the mesh node, so it carries no bind of its own.
        let bytes = rig_file(&[
            bone_block("riderRIG_Root", None, &[0, 1, 1], [0.0; 3], [0.0; 3]),
            bone_block(
                "riderRIG_Pelvis",
                Some(bind_at(0.0, 0.0, 0.0)),
                &[1, 1, 2],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_LeftHip",
                Some(bind_at(0.0, 0.0, -0.887)),
                &[2, 2, 3, 14],
                [-0.11, -0.14, -0.10],
                [0.14, 0.14, 0.13],
            ),
            bone_block(
                "riderRIG_LeftKnee",
                Some(bind_at(0.085, 0.0, -0.864)),
                &[3, 1, 4],
                [-0.13, -0.13, -0.16],
                [0.24, 0.10, 0.16],
            ),
        ]);
        let rig = parse_skeleton(&bytes);
        let names: Vec<&str> = rig.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            ["riderRIG_Root", "riderRIG_Pelvis", "riderRIG_LeftHip"]
        );
        assert_eq!(rig[0].parent, None, "only the root is parentless");
        assert_eq!(rig[1].parent, Some(0), "the pelvis hangs off the root");
        assert_eq!(rig[2].parent, Some(1), "and the hip off the pelvis");
        // The bind is derived from the stored inverse, so a bone reports where it really is.
        let hip = rig[2].origin();
        assert!(
            (hip[2] + 0.864).abs() < 1e-5 && (hip[0] - 0.085).abs() < 1e-5,
            "{hip:?}"
        );
        assert_eq!(rig[1].origin(), [0.0, 0.0, -0.887]);
    }

    #[test]
    fn the_rig_is_read_once_however_many_copies_the_file_holds() {
        // The game's riders store the whole rig once per level of detail, back to back and with
        // slightly different boxes. Reading past the first copy gave every bone a namesake to
        // hang off and closed the tree into a cycle: a bone inside one reaches no root, so
        // nothing works out where it is and the body collapses into the origin.
        let copy = |lo: [f32; 3], hi: [f32; 3]| {
            vec![
                bone_block("riderRIG_Pelvis", None, &[0], [0.0; 3], [0.0; 3]),
                bone_block(
                    "riderRIG_LeftHip",
                    Some(bind_at(0.0, 0.9, 0.0)),
                    &[1],
                    lo,
                    hi,
                ),
                bone_block(
                    "riderRIG_LeftKnee",
                    Some(bind_at(0.1, 0.5, 0.0)),
                    &[2],
                    lo,
                    hi,
                ),
                bone_block(
                    "riderRIG_Spine1",
                    Some(bind_at(0.1, 0.2, 0.0)),
                    &[3],
                    lo,
                    hi,
                ),
            ]
        };
        let mut blocks = copy([-0.1, -0.1, -0.1], [0.1, 0.1, 0.1]);
        blocks.extend(copy([-0.2, -0.2, -0.2], [0.2, 0.2, 0.2]));
        let rig = parse_skeleton(&rig_file(&blocks));
        assert_eq!(
            rig.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["riderRIG_Pelvis", "riderRIG_LeftHip", "riderRIG_LeftKnee"],
            "a second copy of the rig is not more bones"
        );
        // The copy that survives is the first — the one that goes with the LOD0 mesh.
        assert_eq!(rig[1].aabb_hi, [0.1, 0.1, 0.1]);
    }

    #[test]
    fn a_bone_hangs_off_one_the_file_has_already_listed() {
        // Depth-first order means a parent always precedes its child, so a name-matched parent
        // that comes later is a misread — and taking it would be a cycle.
        let bytes = rig_file(&[
            bone_block("riderRIG_Pelvis", None, &[0], [0.0; 3], [0.0; 3]),
            bone_block(
                "riderRIG_LeftKnee",
                Some(bind_at(0.0, 0.9, 0.0)),
                &[1],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_LeftHip",
                Some(bind_at(0.1, 0.5, 0.0)),
                &[2],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_Head",
                Some(bind_at(0.1, 0.2, 0.0)),
                &[3],
                [0.0; 3],
                [0.0; 3],
            ),
        ]);
        let rig = parse_skeleton(&bytes);
        assert_eq!(rig[0].parent, None, "only the first bone is parentless");
        for (i, b) in rig.iter().enumerate() {
            assert!(
                b.parent.is_none_or(|p| p < i),
                "{} hangs off a later bone",
                b.name
            );
        }
    }

    #[test]
    fn a_limb_whose_chain_is_unbound_is_its_own_root() {
        // `default_mx_c` binds the arms and legs and no spine at all. Every chain root then
        // names a parent that isn't there, and hanging it off the bone before it in the file
        // built a fake tree — the right leg off the left leg's twist, the arms off the right
        // leg — where turning one hip dragged the whole body behind it.
        let at = |x: f32, z: f32| Some(bind_at(x, 0.0, z));
        let bytes = rig_file(&[
            bone_block("riderRIG_LeftHip", None, &[0], [0.0; 3], [0.0; 3]),
            bone_block(
                "riderRIG_LeftKnee",
                at(0.085, -0.864),
                &[1],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_LeftHipTwist2",
                at(0.138, -0.503),
                &[2],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_RightHip",
                at(0.113, -0.678),
                &[3],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_RightKnee",
                at(-0.085, -0.864),
                &[4],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_LeftCollar",
                at(-0.138, -0.503),
                &[5],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_LeftShoulder",
                at(-0.019, -1.368),
                &[6],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_Spare",
                at(-0.183, -1.311),
                &[7],
                [0.0; 3],
                [0.0; 3],
            ),
        ]);
        let rig = parse_skeleton(&bytes);
        let at_name = |n: &str| rig.iter().position(|b| b.name == n).expect(n);
        let parent = |n: &str| rig[at_name(n)].parent.map(|p| rig[p].name.clone());
        assert_eq!(
            parent("riderRIG_LeftKnee").as_deref(),
            Some("riderRIG_LeftHip")
        );
        assert_eq!(
            parent("riderRIG_RightKnee").as_deref(),
            Some("riderRIG_RightHip")
        );
        assert_eq!(
            parent("riderRIG_LeftShoulder").as_deref(),
            Some("riderRIG_LeftCollar")
        );
        // The three the model gives no ancestor for stand on their own.
        assert_eq!(parent("riderRIG_LeftHip"), None);
        assert_eq!(
            parent("riderRIG_RightHip"),
            None,
            "a leg does not hang off the other leg"
        );
        assert_eq!(parent("riderRIG_LeftCollar"), None, "nor an arm off a leg");
    }

    #[test]
    fn a_bone_nobody_names_still_hangs_off_the_one_before_it() {
        // A mod rig with a bone of its own invention: the file is depth-first, so the bone
        // before it is the best guess there is, and that is still what it gets.
        let bytes = rig_file(&[
            bone_block("riderRIG_Pelvis", None, &[0], [0.0; 3], [0.0; 3]),
            bone_block(
                "riderRIG_Cape",
                Some(bind_at(0.0, 0.0, -0.887)),
                &[1],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_Spare",
                Some(bind_at(0.0, 0.1, -1.2)),
                &[2],
                [0.0; 3],
                [0.0; 3],
            ),
        ]);
        let rig = parse_skeleton(&bytes);
        assert_eq!(rig[1].name, "riderRIG_Cape");
        assert_eq!(rig[1].parent, Some(0));
    }

    #[test]
    fn a_bone_that_binds_nothing_is_left_out() {
        // The ankles and every `_end` marker carry a local matrix and no inverse bind: they
        // belong to the boots, not to this mesh, so they are not bones the body can be posed by.
        let bytes = rig_file(&[
            bone_block("riderRIG_Pelvis", None, &[0], [0.0; 3], [0.0; 3]),
            bone_block(
                "riderRIG_LeftAnkle",
                Some(bind_at(0.0, 0.0, -0.887)),
                &[1],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block("riderRIG_LeftToe", None, &[2], [0.0; 3], [0.0; 3]),
            bone_block("riderRIG_Head", None, &[3], [0.0; 3], [0.0; 3]),
        ]);
        let rig = parse_skeleton(&bytes);
        assert_eq!(
            rig.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["riderRIG_Pelvis"],
            "only the bone whose record carries an inverse bind survives"
        );
    }

    #[test]
    fn a_bike_has_no_rig() {
        // Vertex and index soup must not read as bone records.
        let mut v = b"EDF\0".to_vec();
        v.resize(HEADER_START, 0);
        for i in 0..4000u32 {
            v.extend_from_slice(&(i as f32 * 0.001).to_le_bytes());
        }
        assert!(parse_skeleton(&v).is_empty());
        assert!(parse_skeleton(b"not an edf").is_empty());
    }

    #[test]
    fn the_rig_naming_is_the_hierarchy() {
        let p = |s: &str| super::named_parent(s);
        assert_eq!(p("root"), None);
        assert_eq!(p("pelvis").as_deref(), Some("root"));
        // Numbered chains count themselves down.
        assert_eq!(p("spine3").as_deref(), Some("spine2"));
        assert_eq!(p("spine1").as_deref(), Some("pelvis"));
        assert_eq!(p("neck1").as_deref(), Some("spine4"));
        assert_eq!(p("head").as_deref(), Some("neck2"));
        // Arms, both sides, down to the fingers.
        assert_eq!(p("leftshoulder").as_deref(), Some("leftcollar"));
        assert_eq!(p("leftelbow").as_deref(), Some("leftshoulder"));
        assert_eq!(p("leftwrist").as_deref(), Some("leftelbow"));
        assert_eq!(p("rightindex1").as_deref(), Some("rightwrist"));
        assert_eq!(p("rightindex3").as_deref(), Some("rightindex2"));
        // Legs.
        assert_eq!(p("lefthip").as_deref(), Some("pelvis"));
        assert_eq!(p("leftknee").as_deref(), Some("lefthip"));
        assert_eq!(p("leftankle").as_deref(), Some("leftknee"));
        assert_eq!(p("lefttoe").as_deref(), Some("leftankle"));
        // Markers and twist bones hang off what they mark and what they twist about.
        assert_eq!(p("lefttoe_end").as_deref(), Some("lefttoe"));
        assert_eq!(p("leftkneetwist").as_deref(), Some("leftknee"));
        assert_eq!(p("lefthiptwist1").as_deref(), Some("lefthip"));
        assert_eq!(p("leftshouldertwist2").as_deref(), Some("leftshoulder"));
        assert_eq!(p("leftkneetwist_end").as_deref(), Some("leftkneetwist"));
        // The body armour hangs off the upper spine, which is where `gfx.cfg` puts the neck brace.
        assert_eq!(p("armour").as_deref(), Some("spine4"));
    }

    #[test]
    fn standing_the_rig_up_turns_it_with_the_mesh() {
        // The same turn `stand_body_upright` gives a Z-up body: x = -x, y = -z, z = -y.
        let r = [[-1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]];
        let mut rig = parse_skeleton(&rig_file(&[
            bone_block("riderRIG_Root", None, &[0], [0.0; 3], [0.0; 3]),
            bone_block(
                "riderRIG_Pelvis",
                Some(bind_at(0.0, 0.0, 0.0)),
                &[1],
                [0.0; 3],
                [0.0; 3],
            ),
            bone_block(
                "riderRIG_Head",
                Some(bind_at(0.0, 0.0, -0.9)),
                &[2],
                [0.0; 3],
                [0.0; 3],
            ),
        ]));
        transform_skeleton(&mut rig, r);
        let o = rig[1].origin();
        assert!(
            (o[1] - 0.9).abs() < 1e-5,
            "the pelvis stands up at y=0.9, got {o:?}"
        );
        assert!(
            o[0].abs() < 1e-5 && o[2].abs() < 1e-5,
            "and nowhere else: {o:?}"
        );
        // The inverse was rebuilt from the turned bind, not carried over.
        let back = super::rigid_inverse(&rig[1].bind);
        assert!(rig[1]
            .inv_bind
            .iter()
            .zip(back)
            .all(|(a, b)| (a - b).abs() < 1e-5));
    }

    /// Investigation aid: print a rider's rig.
    /// MXB_EDF_FILE=~/…/rider.edf cargo test rig_dump -- --ignored --nocapture
    #[test]
    #[ignore]
    fn rig_dump() {
        let path = std::env::var("MXB_EDF_FILE").expect("set MXB_EDF_FILE");
        let bytes = std::fs::read(&path).expect("read edf");
        let rig = parse_skeleton(&bytes);
        eprintln!("bones: {}", rig.len());
        for (i, b) in rig.iter().enumerate() {
            let p = b.parent.map(|p| rig[p].name.as_str()).unwrap_or("—");
            let o = b.origin();
            eprintln!(
                "{i:3} {:32} parent={:28} at ({:7.3},{:7.3},{:7.3})",
                b.name, p, o[0], o[1], o[2]
            );
        }
    }

    /// The real rig, against the model on disk.
    ///
    /// Checks it three ways, because a skeleton that parses is not the same as a skeleton that
    /// is *right*: the tree the game's own `gfx.cfg` implies, limb lengths that belong to a
    /// person, and every joint landing inside the body it is supposed to move.
    #[test]
    #[ignore]
    fn real_rider_rig() {
        let Ok(path) = std::env::var("MXB_EDF_FILE") else {
            eprintln!("set MXB_EDF_FILE to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read edf");
        let rig = parse_skeleton(&bytes);
        let nodes = parse(&bytes);
        eprintln!("{}: {} bones bind of the rig's 98", path, rig.len());
        assert!(
            rig.len() >= 40,
            "a rider binds most of its rig, got {}",
            rig.len()
        );
        assert_eq!(rig[0].name, "riderRIG_Root");
        assert_eq!(rig[0].parent, None);
        assert!(
            rig[1..].iter().all(|b| b.parent.is_some()),
            "only the root is parentless"
        );

        let at = |n: &str| {
            rig.iter()
                .position(|b| b.name == n)
                .unwrap_or_else(|| panic!("no {n}"))
        };
        let ancestors = |n: &str| {
            let (mut out, mut k) = (Vec::new(), rig[at(n)].parent);
            while let Some(i) = k {
                assert!(out.len() < rig.len(), "{n} loops");
                out.push(rig[i].name.clone());
                k = rig[i].parent;
            }
            out
        };
        // `lefthand { refobj = LeftWrist; endeffector = LeftElbow; root = LeftShoulder }`.
        let arm = ancestors("riderRIG_LeftWrist");
        assert!(arm.contains(&"riderRIG_LeftElbow".to_string()), "{arm:?}");
        assert!(
            arm.contains(&"riderRIG_LeftShoulder".to_string()),
            "{arm:?}"
        );
        for b in &rig[1..] {
            let up = ancestors(&b.name);
            assert_eq!(
                up.last().map(String::as_str),
                Some("riderRIG_Root"),
                "{} → {up:?}",
                b.name
            );
        }

        // Limbs the length a person's are.
        let span = |a: &str, c: &str| {
            let (x, y) = (rig[at(a)].origin(), rig[at(c)].origin());
            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
        };
        for (a, c, lo, hi) in [
            ("riderRIG_LeftHip", "riderRIG_LeftKnee", 0.30, 0.45),
            ("riderRIG_LeftShoulder", "riderRIG_LeftElbow", 0.18, 0.32),
            ("riderRIG_LeftElbow", "riderRIG_LeftWrist", 0.18, 0.32),
        ] {
            let d = span(a, c);
            assert!((lo..hi).contains(&d), "{a} → {c} is {d:.3} m");
        }
        // Left and right are mirrors of each other.
        for (l, r) in [
            ("riderRIG_LeftHip", "riderRIG_RightHip"),
            ("riderRIG_LeftElbow", "riderRIG_RightElbow"),
        ] {
            let (a, b) = (rig[at(l)].origin(), rig[at(r)].origin());
            assert!(
                (a[0] + b[0]).abs() < 1e-3,
                "{l}/{r} are not mirrored: {a:?} {b:?}"
            );
        }

        // Every joint inside the body it moves.
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for n in &nodes {
            for v in n.positions.chunks_exact(3) {
                for a in 0..3 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
        }
        for b in &rig {
            let o = b.origin();
            for a in 0..3 {
                assert!(
                    o[a] >= lo[a] - 0.02 && o[a] <= hi[a] + 0.02,
                    "{} sits outside the mesh on axis {a}: {o:?} vs {lo:?}..{hi:?}",
                    b.name
                );
            }
        }
    }

    // ── Skinning ──────────────────────────────────────────────────────────────

    fn boxed(name: &str, at: [f32; 3], half: f32) -> Vec<u8> {
        bone_block(
            name,
            Some(bind_at(at[0], at[1], at[2])),
            &[0],
            [-half, -half, -half],
            [half, half, half],
        )
    }

    fn node_of(points: &[[f32; 3]]) -> EdfNode {
        EdfNode {
            name: "body".into(),
            positions: points.iter().flatten().copied().collect(),
            uvs: vec![0.0; points.len() * 2],
            normals: vec![0.0; points.len() * 3],
            indices: vec![],
            submeshes: vec![],
            texture: None,
            placed: false,
            materials: vec![],
        }
    }

    /// A two-link arm along X: shoulder at the origin, elbow at 1, wrist at 2. A name leads
    /// its record, so each block carries the *next* bone's name — see `bone_block`.
    fn arm_rig() -> Vec<Bone> {
        let rig = parse_skeleton(&rig_file(&[
            bone_block("riderRIG_LeftShoulder", None, &[0], [0.0; 3], [0.0; 3]),
            boxed("riderRIG_LeftElbow", [0.0, 0.0, 0.0], 1.2),
            boxed("riderRIG_LeftWrist", [1.0, 0.0, 0.0], 1.2),
            boxed("riderRIG_LeftThumb1", [2.0, 0.0, 0.0], 1.2),
        ]));
        assert_eq!(
            rig.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            [
                "riderRIG_LeftShoulder",
                "riderRIG_LeftElbow",
                "riderRIG_LeftWrist"
            ]
        );
        assert_eq!(rig[0].origin(), [0.0, 0.0, 0.0]);
        assert_eq!(rig[1].origin(), [1.0, 0.0, 0.0]);
        rig
    }

    #[test]
    fn every_vertex_is_bound_and_its_weights_add_up() {
        let rig = arm_rig();
        let nodes = [node_of(&[
            [0.2, 0.0, 0.0],
            [0.9, 0.1, 0.0],
            [1.5, 0.0, 0.0],
            [40.0, 40.0, 40.0],
        ])];
        let skin = skin_mesh(&nodes, &rig);
        assert_eq!(skin.indices.len(), 4 * SKIN_BONES_PER_VERTEX);
        for v in 0..4 {
            let w: f32 = skin.weights[v * SKIN_BONES_PER_VERTEX..][..SKIN_BONES_PER_VERTEX]
                .iter()
                .sum();
            assert!((w - 1.0).abs() < 1e-5, "vertex {v} weights sum to {w}");
            assert!(
                skin.indices[v * SKIN_BONES_PER_VERTEX..][..SKIN_BONES_PER_VERTEX]
                    .iter()
                    .all(|&i| (i as usize) < rig.len()),
                "vertex {v} names a bone that isn't there"
            );
        }
    }

    #[test]
    fn a_vertex_follows_the_limb_it_sits_on() {
        let rig = arm_rig();
        let shoulder = rig
            .iter()
            .position(|b| b.name.ends_with("Shoulder"))
            .unwrap();
        let elbow = rig.iter().position(|b| b.name.ends_with("Elbow")).unwrap();
        // Upper arm, forearm, and a point right on the elbow.
        let nodes = [node_of(&[
            [0.5, 0.0, 0.0],
            [1.5, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ])];
        let skin = skin_mesh(&nodes, &rig);
        let heaviest = |v: usize| {
            let s = &skin.weights[v * SKIN_BONES_PER_VERTEX..][..SKIN_BONES_PER_VERTEX];
            let at = s
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            skin.indices[v * SKIN_BONES_PER_VERTEX + at] as usize
        };
        assert_eq!(
            heaviest(0),
            shoulder,
            "the upper arm swings from the shoulder"
        );
        assert_eq!(heaviest(1), elbow, "the forearm swings from the elbow");
        // At the joint the vertex is shared rather than snapped to one side.
        let at_joint = &skin.weights[2 * SKIN_BONES_PER_VERTEX..][..SKIN_BONES_PER_VERTEX];
        assert!(
            at_joint.iter().filter(|w| **w > 0.05).count() >= 2,
            "{at_joint:?}"
        );
    }

    #[test]
    fn a_far_vertex_still_lands_on_a_limb() {
        // No box claims it, so it goes to the nearest limb outright rather than nowhere: a
        // vertex bound to nothing would sit still while the rest of the body moved.
        let rig = arm_rig();
        let nodes = [node_of(&[[9.0, 9.0, 0.0]])];
        let skin = skin_mesh(&nodes, &rig);
        assert_eq!(skin.weights[0], 1.0);
        assert!(skin.weights[1..SKIN_BONES_PER_VERTEX]
            .iter()
            .all(|w| *w == 0.0));
        assert!((skin.indices[0] as usize) < rig.len());
    }

    #[test]
    fn a_mesh_with_no_rig_still_draws() {
        let nodes = [node_of(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]])];
        let skin = skin_mesh(&nodes, &[]);
        assert_eq!(skin.weights[0], 1.0);
        assert_eq!(skin.weights[SKIN_BONES_PER_VERTEX], 1.0);
    }

    /// The real body against its real rig.
    #[test]
    #[ignore]
    fn real_rider_skin() {
        let Ok(path) = std::env::var("MXB_EDF_FILE") else {
            eprintln!("set MXB_EDF_FILE to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read edf");
        let rig = parse_skeleton(&bytes);
        let nodes = parse(&bytes);
        let skin = skin_mesh(&nodes, &rig);
        let verts: usize = nodes.iter().map(|n| n.positions.len() / 3).sum();
        assert_eq!(skin.weights.len(), verts * SKIN_BONES_PER_VERTEX);

        // The rig has to be a tree, and each bone in it once: the file holds a copy per level
        // of detail, and reading two of them gives every bone a namesake to hang off. That
        // closes a cycle, and a bone inside one hangs off no root — so nothing works out where
        // it is and the whole body draws folded into the origin.
        let names: std::collections::HashSet<&str> = rig.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names.len(), rig.len(), "the rig is in here more than once");
        // A model that binds the whole rig has one root; one that binds only its limbs has a
        // root per chain (see `a_limb_whose_chain_is_unbound_is_its_own_root`). What must hold
        // either way is that every bone reaches a root without going round.
        assert!(rig.iter().any(|b| b.parent.is_none()), "at least one root");
        for (i, b) in rig.iter().enumerate() {
            assert!(
                b.parent.is_none_or(|p| p < i),
                "{} hangs off a later bone",
                b.name
            );
        }

        let mut used = std::collections::HashSet::new();
        let mut shared = 0usize;
        for v in 0..verts {
            let w = &skin.weights[v * SKIN_BONES_PER_VERTEX..][..SKIN_BONES_PER_VERTEX];
            let total: f32 = w.iter().sum();
            assert!((total - 1.0).abs() < 1e-4, "vertex {v} sums to {total}");
            if w.iter().filter(|x| **x > 0.05).count() > 1 {
                shared += 1;
            }
            for slot in 0..SKIN_BONES_PER_VERTEX {
                if w[slot] > 0.0 {
                    used.insert(skin.indices[v * SKIN_BONES_PER_VERTEX + slot]);
                }
            }
        }
        eprintln!(
            "{verts} vertices, {} of the {} bones used, {shared} shared between bones",
            used.len(),
            rig.len()
        );
        // A skin that puts everything on one bone, or leaves limbs unbound, is not a skin.
        assert!(
            used.len() > rig.len() / 2,
            "only {} bones move anything",
            used.len()
        );
        assert!(
            shared * 4 > verts,
            "hardly any vertex is shared — the seams will tear"
        );

        // Left stays left: no vertex on one side of the body may be pulled by the other's arm.
        // One run of x across every node, in the order the skin was built in.
        let xs: Vec<f32> = nodes
            .iter()
            .flat_map(|n| n.positions.chunks_exact(3).map(|v| v[0]))
            .collect();
        let at = |n: &str| rig.iter().position(|b| b.name == n);
        if let (Some(l), Some(r)) = (at("riderRIG_LeftWrist"), at("riderRIG_RightWrist")) {
            for (side, other) in [(l, r), (r, l)] {
                let reach: Vec<f32> = (0..verts)
                    .filter(|v| {
                        (0..SKIN_BONES_PER_VERTEX).any(|s| {
                            skin.indices[v * SKIN_BONES_PER_VERTEX + s] as usize == side
                                && skin.weights[v * SKIN_BONES_PER_VERTEX + s] > 0.2
                        })
                    })
                    .map(|v| xs[v])
                    .collect();
                if reach.is_empty() {
                    continue;
                }
                let x = rig[side].origin()[0];
                let wrong = reach.iter().filter(|v| v.signum() != x.signum()).count();
                assert!(
                    wrong * 20 < reach.len().max(20),
                    "{} pulls {wrong} of {} vertices from the other side",
                    rig[side].name,
                    reach.len()
                );
                let _ = other;
            }
        }
    }
}

/// Does this mesh's material tables use the second texture slot?
///
/// The discriminator between the two index spaces below. PiBoSo's own bikes leave `w13` zero
/// on every record; a mod that ships companion maps fills it in. Meshes that don't use it keep
/// the reading they've always had, so this can only ever change a mesh that was unreadable
/// before.
pub fn uses_companion_slots(b: &[u8]) -> bool {
    let textures = embedded_textures(b).len();
    for (_, start) in node_starts(b) {
        for count in 1..=MAX_MATERIALS {
            let Some(o) = start.checked_sub(4 + MAT_STRIDE * count) else {
                break;
            };
            if u32le(b, o) as usize != count {
                continue;
            }
            if (0..count).all(|k| valid_material_record(b, o + 4 + MAT_STRIDE * k, textures)) {
                if (0..count).any(|k| u32le(b, o + 4 + MAT_STRIDE * k + 52) != 0) {
                    return true;
                }
                break;
            }
        }
    }
    false
}

/// `(name, offset of its vertex-count word)` for every node the scanner accepts — the same
/// walk `parse_impl` does, without building the meshes.
fn node_starts(b: &[u8]) -> Vec<(String, usize)> {
    let n = b.len();
    let mut out = Vec::new();
    if n < HEADER_START + 8 || &b[0..4] != b"EDF\0" {
        return out;
    }
    let mut o = HEADER_START;
    while o + 8 <= n {
        let vc = u32le(b, o) as usize;
        if (8..=MAX_COUNT).contains(&vc) && o + 4 + vc * STRIDE + 8 <= n {
            let vs = o + 4;
            if [0usize, 1, 2, vc / 2, vc - 1]
                .iter()
                .all(|&i| finite_pos(b, vs + i * 12, MODEL_EXTENT))
            {
                let ic = vs + vc * STRIDE;
                let tc = u32le(b, ic) as usize;
                if (1..=MAX_COUNT).contains(&tc) && ic + 8 + tc * 12 <= n {
                    let ok = (0..tc * 3).all(|t| (u32le(b, ic + 4 + t * 4) as usize) < vc);
                    let iend = ic + 8 + tc * 12;
                    if let (true, Some(name)) = (ok, plausible_name(b, iend)) {
                        out.push((name, o));
                        o = iend;
                        continue;
                    }
                }
            }
        }
        o += 1;
    }
    out
}

/// The texture list a **bike** material's index counts, for a mesh that uses companion slots.
///
/// Not [`declared_colors`], which keeps one slot per colour sheet. That is right for gear and
/// right for any mesh whose materials leave `w13` zero. A mod bike is different: it declares
/// companion maps and, crucially, sheets it never embeds at all, and those hold slots too.
/// Counting only the colour sheets slid every material down by a growing amount, so a silencer
/// came out wearing the tank's paint and most parts bound to nothing.
///
/// The rule, checked against twelve parts of the KTM 450's swap mesh whose names name their own
/// sheet (`LUXON LMM` -> `luxlmm`, `Polar + Mount` -> `polarm`, `levers` -> `arclever`): walk
/// the declarations in file order; each family contributes its colour sheet, then its `_r` if
/// one is declared. `_n` and `_s` never take a slot, and a family with no colour sheet of its
/// own contributes nothing.
pub fn bike_material_slots(b: &[u8]) -> Vec<String> {
    let declared = declared_names(b);
    let with_colour: std::collections::HashSet<String> = declared
        .iter()
        .filter(|n| family_stem(n) == n.to_ascii_lowercase())
        .map(|n| n.to_ascii_lowercase())
        .collect();
    let mut out: Vec<String> = Vec::new();
    for name in &declared {
        if !with_colour.contains(&family_stem(name)) {
            continue;
        }
        let takes_a_slot = name.to_ascii_lowercase() == family_stem(name)
            || family_stem(name) != name.to_ascii_lowercase()
                && name.to_ascii_lowercase().ends_with("_r");
        if takes_a_slot && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push(name.clone());
        }
    }
    out
}

fn family_stem(n: &str) -> String {
    for suf in ["_n", "_s", "_r"] {
        if let Some(base) = n.strip_suffix(suf) {
            return base.to_ascii_lowercase();
        }
    }
    n.to_ascii_lowercase()
}

/// Every texture name the model writes in the clear, in file order — embedded or not.
///
/// A name only counts where it sits outside every texture payload, preceded by the zero high
/// byte of the word in front of it and terminated by a zero, as [`declaration_offset`]
/// requires — so a string inside compressed pixels can't invent a slot. The file is still full
/// of short delimited scraps that look like names, so a candidate also has to be corroborated:
/// embedded, or one of several in its family, or written like a sheet name by hand. Without
/// that an eight-character scrap took a slot and every part after it bound one sheet late.
fn declared_names(b: &[u8]) -> Vec<String> {
    let embedded = embedded_textures(b);
    let ok_char = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let mut hits: Vec<(usize, String)> = Vec::new();
    for t in &embedded {
        let at = declaration_offset(b, &t.name, &embedded).unwrap_or(t.data_off);
        hits.push((at, t.name.clone()));
    }
    let mut i = 1usize;
    while i + 3 < b.len() {
        if b[i - 1] != 0 || !ok_char(b[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < b.len() && ok_char(b[j]) && j - i < 64 {
            j += 1;
        }
        if !(5..=63).contains(&(j - i)) || b.get(j) != Some(&0) {
            i += 1;
            continue;
        }
        let name = String::from_utf8_lossy(&b[i..j]).to_string();
        let in_pixels = embedded
            .iter()
            .any(|t| i >= t.data_off && i < t.data_off + t.data_len);
        if !in_pixels && name.chars().any(|c| c.is_ascii_alphabetic()) && !is_part_name(&name) {
            hits.push((i, name));
        }
        i = j + 1;
    }
    hits.sort_by_key(|(at, _)| *at);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let names: Vec<String> = hits
        .into_iter()
        .filter(|(_, n)| seen.insert(n.to_ascii_lowercase()))
        .map(|(_, n)| n)
        .collect();

    let embedded_stems: std::collections::HashSet<String> =
        embedded.iter().map(|t| family_stem(&t.name)).collect();
    let mut family: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for n in &names {
        *family.entry(family_stem(n)).or_default() += 1;
    }
    names
        .into_iter()
        .filter(|n| {
            let stem = family_stem(n);
            embedded_stems.contains(&stem)
                || family.get(&stem).copied().unwrap_or(0) > 1
                || (stem == n.to_ascii_lowercase()
                    && !n.chars().any(|c| c.is_ascii_uppercase())
                    && n.len() >= 5)
        })
        .collect()
}

/// The `.hrc` part names, which share the declaration encoding and are never textures.
fn is_part_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "chassis" | "steer" | "fsusp" | "rsusp" | "swingarm" | "fwheel" | "rwheel" | "rwheela"
    )
}
