//! Reading a track's scenery mesh out of its `.map`.
//!
//! The `.map` is not the riding surface — that comes from the `.trh`. It holds everything
//! standing on and around it: tents, hay bales, banner lines, fences, poles, vehicles, and
//! the landscape beyond the terrain square. The two are disjoint and share one world frame
//! in metres, so they compose without any fitting.
//!
//! ```text
//!  0x00  "MP2\0"
//!  0x04  u32 = 304 (constant on every map measured)
//!  0x08  u32 material_count
//!  0x0C  material_count x 56-byte material records
//!        u32 vc
//!        vc x 80 B, structure-of-arrays: pos @ +0 | uv0 @ +12*vc | normal @ +52*vc
//!        u32 tc
//!        tc x 3 u32 indices
//!        u32 node_count, then a tree of 44 B nodes
//!        ... the rest of the file is embedded textures — most of its bulk
//! ```
//!
//! The tree carries the draw calls. Each node is an AABB and five words; the fifth is a
//! group count, and zero means an inner node. A leaf's groups follow it inline, 24 bytes
//! each, and say which material paints which run of triangles. Sorted by material they
//! become the handful of draw calls the viewer needs.
//!
//! Textures sit after the tree in the same records an `.edf` uses — a name, its dimensions,
//! then raw-DEFLATE RGBA — with one difference that matters: a map's can be 16 or 32 pixels
//! across, which [`crate::edf::embedded_textures`] rejects, so this reads them itself.
//! Their names carry the only statement of what a surface *is*: `_c` is colour, and
//! **`_c_a` is colour with an alpha cut-out** — foliage, crowd, fencing. Drawn opaque those
//! read as solid slabs, so the suffix is what keeps a forest looking like a forest.
//!
//! Same shape as an `.edf` node, at 80 bytes per vertex rather than 72, so the normal sits
//! at byte 52 instead of 44. Reading it one byte out is not subtle: normal lengths are all
//! exactly 1.0 at the right offset and noise at any other, which is the check
//! [`parse`] makes before believing a file.

/// The magic that opens a `.map`.
const MAGIC: &[u8; 4] = b"MP2\0";

/// Where the material records start, and how big one is. Geometry follows the last of them.
const MATERIALS_AT: usize = 0x0C;
const MATERIAL_RECORD: usize = 56;

/// Bytes per vertex, and where each attribute's array begins within the block — every one
/// of them a count of vertices from the block's start, not a stride.
const STRIDE: usize = 80;
const UV_AT: usize = 12;
const NORMAL_AT: usize = 52;

/// Bytes per tree node, and per draw group inside a leaf.
const NODE: usize = 44;
const GROUP: usize = 24;

/// Texture record shape, shared with the `.edf`: the dimensions sit at one of two offsets
/// from the name, then a header, then the DEFLATE payload.
const TEX_W_FROM_NAME: [usize; 2] = [100, 104];

/// Where the payload starts, counted from the dimensions.
///
/// **Two shapes are in use**, differing by four bytes: one puts a size then eight zero bytes
/// before the payload, the other a size, a mip count and one pad word. Reading only the first
/// silently skips every record written the other way — and because a material's surface is
/// found by counting, one skipped record repaints every material after it. Both are tried,
/// and the payload itself settles which is right: a DEFLATE stream that ends cleanly on a
/// multiple of the sheet's own pixel count is not a coincidence.
const TEX_DATA_FROM_W: [usize; 2] = [40, 36];

/// Dimensions a texture may have. Wider at the small end than the `.edf` reader's, because a
/// map really does ship 16x16 and 32x32 surfaces and dropping them shifts every material
/// after them onto the wrong picture.
const TEX_SIZES: [u32; 9] = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

/// Sanity caps. A real map runs to about 900k vertices; these are loose enough not to reject
/// a bigger one and tight enough that a misread length can't ask for a gigabyte.
const MAX_VERTS: usize = 8_000_000;
const MAX_TRIS: usize = 8_000_000;
const MAX_MATERIALS: usize = 4096;

/// A run of triangles painted by one material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    pub material: u32,
    pub tri_start: u32,
    pub tri_count: u32,
}

/// One of a map's surfaces, inflated and reduced to something a viewer can hold.
#[derive(Clone, Debug)]
pub struct MapTexture {
    pub material: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Whether the name marks it as an alpha cut-out (`_c_a`). Drawn without an alpha test,
    /// a cut-out is a solid rectangle — which is most of what a track's vegetation is.
    pub alpha: bool,
    /// `width * height * 4`, RGBA.
    pub rgba: Vec<u8>,
}

/// A track's scenery, in world metres — the same frame the terrain grid is placed in.
#[derive(Clone, Debug, Default)]
pub struct MapMesh {
    /// `3 * vertex_count`, world metres.
    pub positions: Vec<f32>,
    /// `3 * vertex_count`, unit length.
    pub normals: Vec<f32>,
    /// `2 * vertex_count`.
    pub uvs: Vec<f32>,
    /// `3 * triangle_count`.
    pub indices: Vec<u32>,
    /// One run of triangles per material, after the index buffer has been sorted so each
    /// material's triangles sit together. A thousand scattered runs become a few dozen.
    pub groups: Vec<Group>,
    /// The connected pieces the scenery is made of — one per tent, trailer or foliage card.
    pub objects: Vec<MapObject>,
    /// Which piece each triangle belongs to, in the sorted order. What turns a ray hit into
    /// a thing you can pick.
    pub object_of_tri: Vec<u32>,
    /// How many materials the map declares. Nothing is drawn with them yet — it is the one
    /// honest measure of how much scenery a track carries, and zero means none at all.
    pub materials: u32,
}

impl MapMesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// World-space bounds, or a zero box when there's nothing in it.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in self.positions.chunks_exact(3) {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if lo.iter().any(|v| !v.is_finite()) {
            return ([0.0; 3], [0.0; 3]);
        }
        (lo, hi)
    }
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn f32le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Whether these bytes open like a `.map` at all.
pub fn is_map(b: &[u8]) -> bool {
    b.len() >= MATERIALS_AT + 4 && &b[0..4] == MAGIC
}

/// How many materials the map declares, without reading any geometry.
fn material_count(b: &[u8]) -> Option<usize> {
    let n = u32le(b, 8) as usize;
    (n <= MAX_MATERIALS).then_some(n)
}

/// Read the scenery mesh, or `None` when the file carries none.
///
/// A track with no scenery is ordinary, not a failure: the OEM drag strip declares zero
/// materials and holds 120 MB of pure texture with not one triangle behind it.
pub fn parse(b: &[u8]) -> Option<MapMesh> {
    if !is_map(b) {
        return None;
    }
    let materials = material_count(b)?;
    let head = MATERIALS_AT + materials * MATERIAL_RECORD;
    if head + 4 > b.len() {
        return None;
    }

    let vc = u32le(b, head) as usize;
    if !(8..=MAX_VERTS).contains(&vc) {
        return None;
    }
    let vs = head + 4;
    let block = vc.checked_mul(STRIDE)?;
    if vs.checked_add(block)?.checked_add(8)? > b.len() {
        return None;
    }

    // Every normal is unit length when the attribute offset is right, and nothing like it
    // when it isn't — the one cheap check that a block really is a vertex block.
    if !normals_are_unit(b, vs, vc) {
        return None;
    }

    let ic = vs + block;
    let tc = u32le(b, ic) as usize;
    if !(1..=MAX_TRIS).contains(&tc) {
        return None;
    }
    let idx_at = ic + 4;
    if idx_at.checked_add(tc.checked_mul(12)?)? > b.len() {
        return None;
    }

    let mut indices = Vec::with_capacity(tc * 3);
    for i in 0..tc * 3 {
        let v = u32le(b, idx_at + i * 4);
        // A single index past the end would walk the vertex arrays off their end in the
        // viewer, so the whole block is rejected rather than clamped.
        if v as usize >= vc {
            return None;
        }
        indices.push(v);
    }

    let mut positions = Vec::with_capacity(vc * 3);
    for i in 0..vc * 3 {
        let v = f32le(b, vs + i * 4);
        if !v.is_finite() {
            return None;
        }
        positions.push(v);
    }

    let normals_at = vs + vc * NORMAL_AT;
    let mut normals = Vec::with_capacity(vc * 3);
    for i in 0..vc * 3 {
        let v = f32le(b, normals_at + i * 4);
        normals.push(if v.is_finite() { v } else { 0.0 });
    }

    let uvs_at = vs + vc * UV_AT;
    let mut uvs = Vec::with_capacity(vc * 2);
    for i in 0..vc * 2 {
        let v = f32le(b, uvs_at + i * 4);
        // Tiling UVs run well outside 0..1, which is ordinary; only a non-finite one would
        // put a triangle's texture lookup somewhere undefined.
        uvs.push(if v.is_finite() { v } else { 0.0 });
    }

    let tree_at = ic + 4 + tc * 12;
    let groups = read_groups(b, tree_at, tc, materials);
    let (indices, groups) = sort_by_material(indices, &groups);

    // After sorting, so a piece's triangle run is contiguous in the buffer the viewer draws.
    let mut material_of_tri = vec![0u32; indices.len() / 3];
    for g in &groups {
        for t in g.tri_start..g.tri_start + g.tri_count {
            if let Some(slot) = material_of_tri.get_mut(t as usize) {
                *slot = g.material;
            }
        }
    }
    let (objects, object_of_tri) = split_into_objects(vc, &indices, &material_of_tri, &positions);

    Some(MapMesh {
        positions,
        normals,
        uvs,
        indices,
        groups,
        objects,
        object_of_tri,
        materials: materials as u32,
    })
}

/// Walk the tree after the index buffer and collect every leaf's draw groups.
///
/// Nodes are stored one after another; each is an AABB and five words, the last of which is
/// how many groups follow it inline. Zero marks an inner node, whose first two words are its
/// children. Nothing here follows the child links — the leaves are all that is wanted, and
/// they are all reachable by walking straight through.
fn read_groups(b: &[u8], at: usize, tri_count: usize, materials: usize) -> Vec<Group> {
    read_groups_to(b, at, tri_count, materials).0
}

/// As [`read_groups`], and also where the tree ends — which is where the surfaces begin.
fn read_groups_to(
    b: &[u8],
    at: usize,
    tri_count: usize,
    materials: usize,
) -> (Vec<Group>, Option<usize>) {
    let mut out = Vec::new();
    if at + 4 > b.len() {
        return (out, None);
    }
    let nodes = u32le(b, at) as usize;
    // Loose, but enough that a misread length can't spin for a billion iterations.
    if nodes > 4_000_000 {
        return (out, None);
    }
    let mut o = at + 4;
    for _ in 0..nodes {
        if o + NODE > b.len() {
            return (Vec::new(), None);
        }
        let ngroups = u32le(b, o + 24 + 16) as usize;
        o += NODE;
        if ngroups == 0 {
            continue;
        }
        if ngroups > 0xFFFF || o + ngroups * GROUP > b.len() {
            return (Vec::new(), None);
        }
        for g in 0..ngroups {
            let e = o + g * GROUP;
            // The leading word is a flag; the material and the ranges follow it.
            let material = u32le(b, e + 4);
            let tri_start = u32le(b, e + 8);
            let count = u32le(b, e + 12);
            if material as usize >= materials || tri_start as usize + count as usize > tri_count {
                return (Vec::new(), None);
            }
            out.push(Group {
                material,
                tri_start,
                tri_count: count,
            });
        }
        o += ngroups * GROUP;
    }
    (out, Some(o))
}

/// Where a map's surfaces begin, and how many entries it says are there.
///
/// The table sits straight after the node tree, so finding it means walking the tree — and
/// scanning from there rather than from the top of the file is what stops a texture-shaped
/// run of bytes inside the geometry being read as a surface.
fn texture_table(b: &[u8]) -> Option<(usize, usize)> {
    let materials = material_count(b)?;
    let head = MATERIALS_AT + materials * MATERIAL_RECORD;
    if head + 4 > b.len() {
        return None;
    }
    let vc = u32le(b, head) as usize;
    if !(8..=MAX_VERTS).contains(&vc) {
        return None;
    }
    let ic = head + 4 + vc.checked_mul(STRIDE)?;
    if ic + 4 > b.len() {
        return None;
    }
    let tc = u32le(b, ic) as usize;
    if !(1..=MAX_TRIS).contains(&tc) {
        return None;
    }
    let tree = ic + 4 + tc.checked_mul(12)?;
    let base = read_groups_to(b, tree, tc, materials).1?;
    if base + 4 > b.len() {
        return None;
    }
    Some((base + 4, u32le(b, base) as usize))
}

/// One connected piece of the scenery — a tent, a trailer, a single foliage card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapObject {
    /// Triangles in this piece, after the index buffer has been sorted by material.
    pub tri_start: u32,
    pub tri_count: u32,
    /// The material most of it is painted with.
    pub material: u32,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Drop the geometry that only makes sense with a picture on it.
///
/// A quarter to a third of a map is cut-out cards — foliage, crowd, netting — flat quads that
/// are a tree only once an alpha channel has cut the tree out of them. Drawn plain they are
/// standing sheets of paper, and there are thousands of them: they hide the track and read as
/// a fault rather than as a limitation.
///
/// So on a map whose surfaces can't be bound, they come out. A card is a triangle standing
/// near-vertical and tall enough not to be ground: measured against a track whose cut-outs
/// are known, that test catches them at 71 % against 11 % of everything else, which is the
/// separation that makes it worth doing at all.
///
/// Where the surfaces *do* bind, nothing is dropped — an alpha test draws them properly.
pub fn without_cards(mesh: &MapMesh) -> MapMesh {
    without_cards_where(mesh, |_| true)
}

/// The same, but only for materials the caller says have no sheet.
///
/// A map that binds most of its materials still leaves a few with nothing, and those are drawn
/// in flat grey. On a solid — a wall, a trailer — grey is merely dull; on a card it is a
/// standing sheet of paper the size of the thing it was meant to be. Indiana binds forty-seven
/// of its forty-nine materials and the two left over are thirteen per cent of its triangles,
/// which is where the grey slabs across the track came from.
pub fn without_cards_for(mesh: &MapMesh, unbound: impl Fn(u32) -> bool) -> MapMesh {
    without_cards_where(mesh, unbound)
}

fn without_cards_where(mesh: &MapMesh, drop_material: impl Fn(u32) -> bool) -> MapMesh {
    let tris = mesh.indices.len() / 3;
    // Which material each triangle belongs to, so a card can be judged by whether its own
    // material bound rather than by the mesh's average.
    let mut owner = vec![u32::MAX; tris];
    for g in &mesh.groups {
        for t in g.tri_start..g.tri_start + g.tri_count {
            if let Some(slot) = owner.get_mut(t as usize) {
                *slot = g.material;
            }
        }
    }
    let vert = |i: u32| {
        let o = i as usize * 3;
        [
            mesh.positions[o],
            mesh.positions[o + 1],
            mesh.positions[o + 2],
        ]
    };
    let mut keep = Vec::with_capacity(tris);
    for t in 0..tris {
        let p = [
            vert(mesh.indices[t * 3]),
            vert(mesh.indices[t * 3 + 1]),
            vert(mesh.indices[t * 3 + 2]),
        ];
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let upright = if len > 1e-9 { (n[1] / len).abs() } else { 1.0 };
        let high = p[0][1].max(p[1][1]).max(p[2][1]);
        let low = p[0][1].min(p[1][1]).min(p[2][1]);
        let card = upright < 0.35 && high - low > 0.8;
        keep.push(!(card && drop_material(owner[t])));
    }

    // Triangles are already sorted by material, so filtering keeps that order and the groups
    // only need recounting.
    let mut indices = Vec::with_capacity(mesh.indices.len());
    let mut groups: Vec<Group> = Vec::new();
    let mut material_of_tri: Vec<u32> = Vec::new();
    for g in &mesh.groups {
        let start = (indices.len() / 3) as u32;
        for t in g.tri_start..g.tri_start + g.tri_count {
            if !keep.get(t as usize).copied().unwrap_or(false) {
                continue;
            }
            let at = t as usize * 3;
            indices.extend_from_slice(&mesh.indices[at..at + 3]);
            material_of_tri.push(g.material);
        }
        let count = (indices.len() / 3) as u32 - start;
        if count > 0 {
            groups.push(Group {
                material: g.material,
                tri_start: start,
                tri_count: count,
            });
        }
    }

    let (objects, object_of_tri) = split_into_objects(
        mesh.vertex_count(),
        &indices,
        &material_of_tri,
        &mesh.positions,
    );
    MapMesh {
        positions: mesh.positions.clone(),
        normals: mesh.normals.clone(),
        uvs: mesh.uvs.clone(),
        indices,
        groups,
        objects,
        object_of_tri,
        materials: mesh.materials,
    }
}

/// Split the mesh into the pieces it is actually made of.
///
/// Triangles that share a vertex belong to the same thing: the exporter welds a tent to
/// itself and to nothing else, so union-find over the index buffer recovers the objects a
/// track was built from without guessing at distances. A published track comes apart into
/// ten thousand pieces this way, the largest of them five metres across — which is a banner,
/// not a blob.
///
/// This is the unit a designer needs: something to pick, hide, or move on its own.
fn split_into_objects(
    vertex_count: usize,
    indices: &[u32],
    material_of_tri: &[u32],
    positions: &[f32],
) -> (Vec<MapObject>, Vec<u32>) {
    let mut parent: Vec<u32> = (0..vertex_count as u32).collect();
    fn find(parent: &mut [u32], mut a: u32) -> u32 {
        while parent[a as usize] != a {
            // Halve the path as we go; a welded mesh makes long chains otherwise.
            parent[a as usize] = parent[parent[a as usize] as usize];
            a = parent[a as usize];
        }
        a
    }
    for t in indices.chunks_exact(3) {
        let r = find(&mut parent, t[0]);
        for &v in &t[1..] {
            let s = find(&mut parent, v);
            if s != r {
                parent[s as usize] = r;
            }
        }
    }

    // Number the roots in the order their first triangle appears, so the object list follows
    // the draw order rather than vertex numbering.
    let mut number: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let tris = indices.len() / 3;
    let mut of_tri = Vec::with_capacity(tris);
    let mut objects: Vec<MapObject> = Vec::new();
    for t in 0..tris {
        let root = find(&mut parent, indices[t * 3]);
        let id = *number.entry(root).or_insert_with(|| {
            objects.push(MapObject {
                tri_start: t as u32,
                tri_count: 0,
                material: material_of_tri.get(t).copied().unwrap_or(0),
                min: [f32::INFINITY; 3],
                max: [f32::NEG_INFINITY; 3],
            });
            objects.len() as u32 - 1
        });
        of_tri.push(id);
        let o = &mut objects[id as usize];
        o.tri_count += 1;
        for k in 0..3 {
            let v = indices[t * 3 + k] as usize * 3;
            for axis in 0..3 {
                let p = positions[v + axis];
                o.min[axis] = o.min[axis].min(p);
                o.max[axis] = o.max[axis].max(p);
            }
        }
    }
    (objects, of_tri)
}

/// Reorder triangles so each material's sit together, and return one group per material.
///
/// The tree hands back a thousand-odd runs in spatial order, which as draw calls would be a
/// thousand state changes for a few dozen surfaces. Sorting the index buffer once here costs
/// nothing at load and leaves the viewer one call per material.
fn sort_by_material(indices: Vec<u32>, groups: &[Group]) -> (Vec<u32>, Vec<Group>) {
    if groups.is_empty() {
        return (indices, Vec::new());
    }
    let mut order: Vec<&Group> = groups.iter().collect();
    order.sort_by_key(|g| (g.material, g.tri_start));

    let mut sorted = Vec::with_capacity(indices.len());
    let mut merged: Vec<Group> = Vec::new();
    for g in order {
        let from = g.tri_start as usize * 3;
        let to = from + g.tri_count as usize * 3;
        if to > indices.len() {
            continue;
        }
        let tri_start = (sorted.len() / 3) as u32;
        sorted.extend_from_slice(&indices[from..to]);
        match merged.last_mut() {
            Some(last) if last.material == g.material => last.tri_count += g.tri_count,
            _ => merged.push(Group {
                material: g.material,
                tri_start,
                tri_count: g.tri_count,
            }),
        }
    }
    (sorted, merged)
}

/// Sample normals across the block and ask whether they're unit vectors.
fn normals_are_unit(b: &[u8], vs: usize, vc: usize) -> bool {
    let at = vs + vc * NORMAL_AT;
    let picks = [0, 1, vc / 4, vc / 2, vc - 2, vc - 1];
    let mut seen = 0;
    for i in picks {
        if i >= vc {
            continue;
        }
        let o = at + i * 12;
        if o + 12 > b.len() {
            return false;
        }
        let (x, y, z) = (f32le(b, o), f32le(b, o + 4), f32le(b, o + 8));
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return false;
        }
        let len = (x * x + y * y + z * z).sqrt();
        if (len - 1.0).abs() > 0.02 {
            return false;
        }
        seen += 1;
    }
    seen > 0
}

// ---------------------------------------------------------------------------
// Surfaces
// ---------------------------------------------------------------------------

/// The longest edge a surface is kept at. A map ships 4096² sheets, and forty-eight of those
/// is a quarter of a gigabyte for a view that draws a whole track at once — while the alpha
/// cut-outs still need enough resolution to keep a tree looking like a tree.
pub const MAX_TEXTURE_DIM: u32 = 512;

/// A texture's name, read in place. `None` when the bytes aren't one.
fn tex_name(b: &[u8], o: usize) -> Option<String> {
    let mut e = o;
    while e < b.len() && e - o < 96 {
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
    (3..=95)
        .contains(&len)
        .then(|| String::from_utf8_lossy(&b[o..e]).into_owned())
}

/// Every colour surface in a map, in file order.
///
/// Every record in the table, in order, name included — no filtering by what it is called.
///
/// Order is the whole point. Nothing in a material record points at a texture — the *k*-th
/// colour map is material *k*'s — so a record skipped here silently repaints every material
/// after it, and one read that shouldn't be shifts the rest along by one.
///
/// The scan starts at the table rather than the top of the file, so a texture-shaped run of
/// bytes inside the geometry can't be mistaken for a surface, and it reads the 16- and
/// 32-pixel sheets the `.edf` scanner drops.
fn colour_records(b: &[u8], from: usize) -> Vec<(String, u32, u32, usize, usize)> {
    let mut out = Vec::new();
    let mut o = from;
    let min = TEX_W_FROM_NAME[1] + TEX_DATA_FROM_W[0] + 8;
    'scan: while o + min <= b.len() {
        // Only at a word boundary, or `2024_haybale` also matches at `haybale`.
        let starts = b[o].is_ascii_alphanumeric() || b[o] == b'_';
        let after = o > 0 && (b[o - 1].is_ascii_alphanumeric() || b[o - 1] == b'_');
        if !starts || after {
            o += 1;
            continue;
        }
        let Some(name) = tex_name(b, o) else {
            o += 1;
            continue;
        };
        for w_off in TEX_W_FROM_NAME {
            if name.len() >= w_off {
                continue; // the name has to terminate inside its own field
            }
            let w_at = o + w_off;
            if w_at + 8 > b.len() {
                continue;
            }
            let (w, h) = (u32le(b, w_at), u32le(b, w_at + 4));
            if !TEX_SIZES.contains(&w) || !TEX_SIZES.contains(&h) {
                continue;
            }
            for from_w in TEX_DATA_FROM_W {
                let data_off = w_at + from_w;
                if data_off >= b.len() {
                    continue;
                }
                let Some((total, used)) = stream_extent(b, data_off, w, h) else {
                    continue;
                };
                let _ = total;
                out.push((name, w, h, data_off, used));
                // Records don't overlap, so the payload is never worth scanning through —
                // walking it byte by byte is where a scan of a 400 MB map would spend its life.
                o = data_off + used;
                continue 'scan;
            }
        }
        o += 1;
    }
    out
}

/// Inflate far enough to prove a record, returning `(pixel bytes, compressed bytes)`.
///
/// A record states a *compressed* length in a field whose offset is exactly what's in doubt,
/// so the stream is asked instead: it has to end cleanly, and on a whole number of bytes per
/// pixel. Bounded at 24 bytes per pixel, which covers the cube maps and rejects anything that
/// would inflate for ever.
fn stream_extent(b: &[u8], at: usize, w: u32, h: u32) -> Option<(usize, usize)> {
    use std::io::Read;
    let px = w as usize * h as usize;
    let cap = px * 24 + 64;
    let mut dec = flate2::bufread::DeflateDecoder::new(&b[at..]);
    let mut total = 0usize;
    let mut buf = [0u8; 1 << 16];
    loop {
        match dec.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total > cap {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    if px == 0 || total < px || total % px != 0 {
        return None;
    }
    let used = dec.total_in() as usize;
    (used > 0 && at + used <= b.len()).then_some((total, used))
}

/// Inflate one record to RGBA8, bounded by what its dimensions can hold.
fn inflate(b: &[u8], data_off: usize, data_len: usize, w: u32, h: u32) -> Option<Vec<u8>> {
    use std::io::Read;
    let expected = w as usize * h as usize * 4;
    let mut buf = Vec::with_capacity(expected);
    // Bounded: a record found by scanning could be a false positive, and one that inflates
    // to gigabytes is an ordinary Tuesday rather than a hostile file.
    Read::take(
        flate2::read::DeflateDecoder::new(&b[data_off..(data_off + data_len).min(b.len())]),
        expected as u64,
    )
    .read_to_end(&mut buf)
    .ok()?;
    (buf.len() == expected).then_some(buf)
}

/// Turn a sheet the right way up.
///
/// PiBoSo stores these bottom-up, the same way it stores a `.pnt`: a card's UVs put V zero at
/// the foot of the thing drawn, while the file's first row of pixels is its top. Left alone,
/// every tree in a track hangs from its canopy.
fn flip_rows(rgba: &mut [u8], w: u32, h: u32) {
    let stride = w as usize * 4;
    let (mut top, mut bottom) = (0usize, h as usize - 1);
    while top < bottom {
        for i in 0..stride {
            rgba.swap(top * stride + i, bottom * stride + i);
        }
        top += 1;
        bottom -= 1;
    }
}

/// Halve an RGBA image until it fits `max_dim`, averaging each block.
///
/// Colour is averaged *weighted by alpha*, which matters only for the cut-outs and matters a
/// lot there: the transparent half of a foliage sheet is usually black, and averaging it in
/// flatly drags every surviving leaf towards black a halving at a time. Alpha itself is
/// averaged plainly, so a shape thins out rather than developing holes.
fn reduce(mut rgba: Vec<u8>, mut w: u32, mut h: u32, max_dim: u32) -> (Vec<u8>, u32, u32) {
    while (w > max_dim || h > max_dim) && w >= 2 && h >= 2 {
        let (nw, nh) = (w / 2, h / 2);
        let mut out = vec![0u8; nw as usize * nh as usize * 4];
        for y in 0..nh as usize {
            for x in 0..nw as usize {
                let px =
                    |xx: usize, yy: usize, c: usize| rgba[(yy * w as usize + xx) * 4 + c] as u32;
                let (x0, y0) = (x * 2, y * 2);
                let corners = [(x0, y0), (x0 + 1, y0), (x0, y0 + 1), (x0 + 1, y0 + 1)];
                let alpha: u32 = corners.iter().map(|&(cx, cy)| px(cx, cy, 3)).sum();
                let o = (y * nw as usize + x) * 4;
                for c in 0..3 {
                    out[o + c] = if alpha > 0 {
                        let weighted: u32 = corners
                            .iter()
                            .map(|&(cx, cy)| px(cx, cy, c) * px(cx, cy, 3))
                            .sum();
                        (weighted / alpha) as u8
                    } else {
                        (corners.iter().map(|&(cx, cy)| px(cx, cy, c)).sum::<u32>() / 4) as u8
                    };
                }
                out[o + 3] = (alpha / 4) as u8;
            }
        }
        rgba = out;
        w = nw;
        h = nh;
    }
    (rgba, w, h)
}

/// How much of a sheet is see-through, as a fraction of its texels.
fn cutout_fraction(rgba: &[u8]) -> f32 {
    if rgba.len() < 4 {
        return 0.0;
    }
    let texels = rgba.len() / 4;
    let clear = rgba.chunks_exact(4).filter(|p| p[3] < 128).count();
    clear as f32 / texels as f32
}

/// Above this fraction of see-through texels a surface is treated as a cut-out.
///
/// Measured from the pixels rather than read off the name. `_c_a` is a convention some track
/// builders follow and others don't — one published track names every sheet plainly
/// (`CK_birch01`, `banner_fmf`) and would have had its whole treeline drawn as slabs. What a
/// surface *is* is in its alpha channel, and that is true of every track.
const CUTOUT_FRACTION: f32 = 0.02;

/// Whether a name marks a colour map under PiBoSo's own convention.
fn is_colour_name(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with("_c") || l.ends_with("_c_a")
}

/// Whether a record is a second map for the surface before it rather than a surface of its
/// own — a normal-and-specular sheet, an environment cube.
///
/// Every convention seen puts the secondary map's name on the colour map's stem: `bale1` then
/// `bale1_n`, `pitlane_c` then `pitlane_n_s`. One sheet is often shared by several materials,
/// so a name already used as a secondary stays one wherever it appears again.
fn is_secondary(
    name: &str,
    previous: Option<&str>,
    seen: &std::collections::HashSet<String>,
) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "env" || seen.contains(&lower) {
        return true;
    }
    const SUFFIXES: [&str; 5] = ["_n", "_s", "_n_s", "_nrm", "_spec"];
    let Some(prev) = previous else { return false };
    let prev = prev.to_ascii_lowercase();
    let stem = prev
        .strip_suffix("_c_a")
        .or_else(|| prev.strip_suffix("_c"))
        .unwrap_or(&prev);
    SUFFIXES
        .iter()
        .any(|suf| lower == format!("{stem}{suf}") || lower == format!("{prev}{suf}"))
}

/// Every surface record a map holds, as `(name, width, height)`, in table order.
///
/// A diagnostic: names and dimensions only, nothing inflated. What a map calls its sheets is
/// the whole of the binding problem, so being able to ask a track that question directly is
/// worth a function.
// Diagnostics the scenery tests read; nothing in the app calls them.
#[allow(dead_code)]
pub fn survey(b: &[u8]) -> Vec<(String, u32, u32)> {
    let Some((from, _)) = texture_table(b) else {
        return Vec::new();
    };
    colour_records(b, from)
        .into_iter()
        .map(|(n, w, h, ..)| (n, w, h))
        .collect()
}

/// The records that look like a material's own colour map, in order.
#[allow(dead_code)]
pub fn primaries(b: &[u8]) -> Vec<(String, u32, u32)> {
    let Some((from, _)) = texture_table(b) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut previous: Option<String> = None;
    let mut out = Vec::new();
    for (n, w, h, ..) in colour_records(b, from) {
        if is_secondary(&n, previous.as_deref(), &seen) {
            seen.insert(n.to_ascii_lowercase());
            continue;
        }
        previous = Some(n.clone());
        out.push((n, w, h));
    }
    out
}

/// The surfaces a map declares, named and sized but not inflated.
///
/// What the viewer needs to size its material slots before any pixels exist — and cheap,
/// because it stops at each record's header and steps over the payload.
pub fn declared(b: &[u8]) -> Vec<(String, u32, u32)> {
    let Some((from, count)) = texture_table(b) else {
        return Vec::new();
    };
    let all = colour_records(b, from);
    let named = all.iter().filter(|(n, ..)| is_colour_name(n)).count();
    if count == 0 || named * 4 < count * 3 {
        return Vec::new();
    }
    all.into_iter()
        .filter(|(n, ..)| is_colour_name(n))
        .take(count)
        .map(|(n, w, h, ..)| (n, w, h))
        .collect()
}

/// A sheet that looks like ground, for tiling over the terrain as detail.
///
/// The largest record whose name carries a ground word and which isn't a normal map. Order
/// and binding don't come into it: this asks only "is this ground", which a name answers well
/// enough, and a wrong answer costs grain rather than a wrong picture on a wrong object.
/// Words that mark a sheet as something drawn *on* a track rather than the ground under it.
/// A name is disqualified outright by one of these, however grounded the rest of it sounds —
/// Abydos ships `logo-dirtmaster`, which is a logo.
const NOT_GROUND: [&str; 31] = [
    "logo", "banner", "sign", "signage", "arch", "flag", "board", "billboard", "tent", "decal",
    "sticker", "poster", "sponsor", "truck", "bike", "wall", "fence", "gate", "tower", "screen",
    "sky", "cloud", "tree", "leaf", "bark", "bale", "tyre", "marker", "post", "cone", "arrow",
];

/// The words a texture name is made of. Separators vary by author — `soil_dark_c`,
/// `sand-dark`, `Dirt 2` — and digits never carry meaning, so everything but letters splits.
fn name_words(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    let chars: Vec<char> = name.chars().collect();
    let mut push = |w: &mut String| {
        if !w.is_empty() {
            out.push(std::mem::take(w));
        }
    };
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_ascii_alphabetic() {
            push(&mut word);
            continue;
        }
        // A capital starts a word too — `RR_TrackMarker` is a marker, and reading it as one
        // word `trackmarker` made it match `track` and pass as ground.
        let after_lower = i > 0 && chars[i - 1].is_ascii_lowercase();
        let before_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
        if c.is_ascii_uppercase() && (after_lower || before_lower) {
            push(&mut word);
        }
        word.push(c.to_ascii_lowercase());
    }
    push(&mut word);
    out
}

/// Whether the name says this record is a normal map. Written every way going: `Dirt2_n`,
/// `soil_white_n_s`, `sand-dark-normal`.
pub(crate) fn is_normal_name(name: &str) -> bool {
    name_words(name)
        .iter()
        .any(|w| matches!(w.as_str(), "n" | "nrm" | "norm" | "normal" | "nor"))
}

/// How well a name states one of the ground words, lower being better, or `None` if it states
/// none. Whole words rank ahead of every prefix match, which is what keeps `dirtmaster` from
/// outranking `dirt` while still letting `Dirtplane` count as dirt.
fn ground_rank(name: &str, words: &[&str]) -> Option<usize> {
    let parts = name_words(name);
    if parts.iter().any(|w| NOT_GROUND.contains(&w.as_str())) {
        return None;
    }
    if let Some(i) = words.iter().position(|g| parts.iter().any(|w| w == g)) {
        return Some(i);
    }
    words
        .iter()
        .position(|g| parts.iter().any(|w| w.starts_with(g)))
        .map(|i| words.len() + i)
}

/// The name with its trailing role words removed, so a colour sheet and its normal map can be
/// recognised as the same stem: `soil_dark_c` and `soil_dark_n_s` both reduce to `soil dark`.
fn name_stem(name: &str) -> Vec<String> {
    let mut parts = name_words(name);
    while parts
        .last()
        .is_some_and(|w| matches!(w.as_str(), "c" | "a" | "d" | "s" | "n" | "col" | "color" | "colour" | "diff" | "nrm" | "norm" | "normal" | "nor"))
    {
        parts.pop();
    }
    parts
}

pub fn ground_sheet(b: &[u8], words: &[&str]) -> Option<MapTexture> {
    let (from, _) = texture_table(b)?;
    // Ranked by which word matched, then by size — the word says what kind of ground it is,
    // and only among equals does the bigger sheet win.
    let mut best: Option<(usize, u64, String, u32, u32, usize, usize)> = None;
    for (name, w, h, off, len) in colour_records(b, from) {
        if is_normal_name(&name) || name.eq_ignore_ascii_case("env") {
            continue;
        }
        let Some(rank) = ground_rank(&name, words) else {
            continue;
        };
        let area = w as u64 * h as u64;
        let better = match &best {
            None => true,
            Some((r, a, ..)) => rank < *r || (rank == *r && area > *a),
        };
        if better {
            best = Some((rank, area, name, w, h, off, len));
        }
    }
    let (_, _, name, w, h, off, len) = best?;
    let mut rgba = inflate(b, off, len, w, h)?;
    let _ = &name;
    // A sheet that turns out to be a cut-out is no use as ground — its holes would punch
    // through the terrain.
    if cutout_fraction(&rgba) > CUTOUT_FRACTION {
        return None;
    }
    flip_rows(&mut rgba, w, h);
    // Smaller than a surface: this tiles, so it needs frequency rather than extent.
    let (rgba, w, h) = reduce(rgba, w, h, 512);
    Some(MapTexture {
        material: 0,
        name,
        width: w,
        height: h,
        alpha: false,
        rgba,
    })
}

/// The normal map that goes with a ground sheet, if the track ships one.
///
/// Named on the sheet's own stem — `dirt` and `dirt_n`, `soil_dark_c` and `soil_dark_n_s`.
/// Tiled with the colour it belongs to, it is what gives close ground relief instead of a
/// flat picture of relief.
pub fn ground_normal(b: &[u8], colour_name: &str, words: &[&str]) -> Option<MapTexture> {
    let stem = name_stem(colour_name);
    let (from, _) = texture_table(b)?;

    // The sheet's own partner first, then any ground's — tracks often ship no partner for the
    // sheet that won (Millville has `soil_dark_c` but its normals are `grass_n_s` and
    // `soil_white_n_s`), and for a detail layer any ground's relief will do.
    let mut candidates: Vec<(usize, (String, u32, u32, usize, usize))> = colour_records(b, from)
        .into_iter()
        .filter(|(n, ..)| is_normal_name(n))
        .filter_map(|r| {
            if name_stem(&r.0) == stem {
                Some((0, r))
            } else {
                ground_rank(&r.0, words).map(|k| (k + 1, r))
            }
        })
        .collect();
    candidates.sort_by_key(|(rank, _)| *rank);

    // Each is only a candidate until its pixels agree: a tangent-space normal map sits around
    // (128, 128, 255), and anything else here is a colour sheet named like one — lighting the
    // ground by it would be lighting it with noise. Trying them in turn rather than testing
    // only the first is what keeps one mis-named sheet from costing a track its relief.
    for (_, (name, w, h, off, len)) in candidates {
        let Some(mut rgba) = inflate(b, off, len, w, h) else {
            continue;
        };
        let texels = rgba.len() / 4;
        if texels == 0 {
            continue;
        }
        let mut sum = [0u64; 3];
        for p in rgba.chunks_exact(4) {
            for k in 0..3 {
                sum[k] += p[k] as u64;
            }
        }
        let mean = [
            sum[0] as f32 / texels as f32,
            sum[1] as f32 / texels as f32,
            sum[2] as f32 / texels as f32,
        ];
        if mean[2] < 200.0 || (mean[0] - 128.0).abs() > 40.0 || (mean[1] - 128.0).abs() > 40.0 {
            continue;
        }
        flip_rows(&mut rgba, w, h);
        let (rgba, w, h) = reduce(rgba, w, h, 512);
        return Some(MapTexture {
            material: 1,
            name,
            width: w,
            height: h,
            alpha: false,
            rgba,
        });
    }
    None
}

/// The biggest picture inside a model file, reduced for the viewer./// The biggest picture inside a model file, reduced for the viewer.
///
/// A track's sky and backdrop keep their image inside the `.edf` rather than in the map's
/// table, and those records use the same two header shapes — so they are read with the same
/// scanner rather than the `.edf`'s own, which knows only one and finds the wrong sheet.
pub fn largest_picture(b: &[u8]) -> Option<MapTexture> {
    let (name, w, h, off, len) = colour_records(b, 0)
        .into_iter()
        .max_by_key(|(_, w, h, ..)| *w as u64 * *h as u64)?;
    let rgba = inflate(b, off, len, w, h)?;
    Some(reduced_texture(&name, w, h, rgba))
}

/// Turn already-inflated RGBA into a surface the viewer can hold: right way up, and small
/// enough to keep. Used for the sky and backdrop, which come out of an `.edf` rather than the
/// map's own table.
pub fn reduced_texture(name: &str, w: u32, h: u32, mut rgba: Vec<u8>) -> MapTexture {
    let alpha = cutout_fraction(&rgba) > CUTOUT_FRACTION;
    flip_rows(&mut rgba, w, h);
    let (rgba, w, h) = reduce(rgba, w, h, MAX_TEXTURE_DIM);
    MapTexture {
        material: 0,
        name: name.to_string(),
        width: w,
        height: h,
        alpha,
        rgba,
    }
}

/// A map's colour surfaces, one per material, inflated and reduced.
///
/// Nothing in a material record points at a texture, so the binding is positional: the *k*-th
/// colour map paints material *k*. That only holds when the colour maps can be told apart
/// from the sheets beside them, and the one reliable marker is PiBoSo's own naming — `_c`
/// (or `_c_a`) for colour, `_n_s` for the normal-and-specular map next to it.
///
/// **A map that doesn't use those suffixes gets no surfaces here, and draws plain.** Its
/// table holds more records than materials — one published track has 55 for 49, the last
/// seven being the terrain's own `dirt`/`grass`/`gravel` — and which of them line up is not
/// something the file has yet been made to say. Guessing puts a blue tent on a boundary wall,
/// which is worse than the honest grey: measured against the geometry, the obvious reading is
/// off by two on that track and nothing in the format explains why.
/// Push a cut-out's colour outwards into the texels its alpha throws away.
///
/// A foliage sheet stores black in every transparent texel — there is no reason for an artist
/// to paint what will never be drawn. That is fine at full size, where an alpha test either
/// keeps a texel or discards it, and wrong at every size below it: a mipmap averages colour
/// and alpha separately, so a leaf texel next to three transparent ones comes out a quarter of
/// its brightness. Indiana's `leafs_twigs_QP_c_a` averages to luma 26, and its trees read green
/// close up and black across the paddock — which is exactly what the mip chain does to them.
///
/// So the colour is grown outwards before the sheet is ever reduced: a transparent texel takes
/// the average of whatever opaque neighbours it has, and the next pass treats that as opaque in
/// turn. Alpha is untouched, so the cut-out still cuts out — only the colour behind the cut
/// changes, from black to more of the leaf.
fn bleed_cutout(rgba: &mut [u8], w: u32, h: u32) {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 {
        return;
    }
    // Which texels carry colour worth spreading. Only the ones the alpha test would actually
    // keep: a half-transparent texel on the edge of a leaf is mostly the black behind it, and
    // seeding from those spreads the very colour this is meant to get rid of — it left
    // Indiana's foliage at luma 40 against leaves of 75. Grows by one texel a pass.
    let mut solid: Vec<bool> = rgba.chunks(4).map(|c| c[3] >= 128).collect();
    // Until the sheet is full. It runs on the *reduced* sheet — a few hundred texels a side —
    // so filling it outright costs less than the eight passes this used to make on the
    // original, which on a 4096-square foliage atlas moved the average by one level.
    for _ in 0..w.max(h) {
        let mut filled = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if solid[i] {
                    continue;
                }
                let (mut sum, mut n) = ([0u32; 3], 0u32);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if !solid[j] {
                        continue;
                    }
                    for k in 0..3 {
                        sum[k] += rgba[j * 4 + k] as u32;
                    }
                    n += 1;
                }
                if n > 0 {
                    filled.push((i, [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]));
                }
            }
        }
        if filled.is_empty() {
            break;
        }
        for (i, rgb) in filled {
            rgba[i * 4..i * 4 + 3].copy_from_slice(&rgb);
            solid[i] = true;
        }
    }
}

/// Whether a map's surfaces can be bound to its materials at all.
///
/// The same test [`textures`] makes, without inflating a pixel — a map carries hundreds of
/// megabytes of sheets and this has to be answerable before deciding what to draw.
///
/// It matters because of what happens when the answer is no. A material with no sheet is drawn
/// in flat grey, and a third of a map is cut-out cards — foliage, crowd, netting — so a track
/// that binds nothing renders as a forest of grey slabs standing over the ground. Measured:
/// Briarcliff names one sheet by the convention out of eighty-five materials and SFDR none at
/// all, so every triangle of both is untextured.
pub fn binds(b: &[u8]) -> bool {
    let Some((from, count)) = texture_table(b) else {
        return false;
    };
    if count == 0 {
        return false;
    }
    let named = colour_records(b, from)
        .iter()
        .filter(|(n, ..)| is_colour_name(n))
        .count();
    named * 4 >= count * 3
}

/// How many materials a map will actually bind a sheet to.
///
/// [`textures`] hands back one entry per `_c`-named record, capped at the material count, so
/// every material at or past this has no sheet and is drawn in flat grey.
pub fn bound_count(b: &[u8]) -> usize {
    let Some((from, count)) = texture_table(b) else {
        return 0;
    };
    if !binds(b) {
        return 0;
    }
    colour_records(b, from)
        .iter()
        .filter(|(n, ..)| is_colour_name(n))
        .count()
        .min(count)
}

pub fn textures(b: &[u8], max_dim: u32) -> Vec<MapTexture> {
    let Some((from, count)) = texture_table(b) else {
        return Vec::new();
    };
    let all = colour_records(b, from);
    // The convention has to cover the map, not merely appear in it. One `_c` among fifty
    // plainly-named sheets is a coincidence, and binding on it paints a single material and
    // leaves the rest grey — which reads as broken rather than as undecided.
    let named = all.iter().filter(|(n, ..)| is_colour_name(n)).count();
    if count == 0 || named * 4 < count * 3 {
        return Vec::new();
    }

    all.into_iter()
        .filter(|(n, ..)| is_colour_name(n))
        .take(count)
        .enumerate()
        .filter_map(|(i, (name, w, h, off, len))| {
            let mut rgba = inflate(b, off, len, w, h)?;
            // Read from the pixels, not the name: `_c_a` is the convention, but the alpha
            // channel is the fact, and a sheet that is half see-through is a cut-out whatever
            // it is called.
            let alpha = cutout_fraction(&rgba) > CUTOUT_FRACTION;
            flip_rows(&mut rgba, w, h);
            let (mut rgba, w, h) = reduce(rgba, w, h, max_dim.max(1));
            // After the reduce, and so before the only mips left to make are the GPU's.
            // `reduce` already weights colour by alpha, so an opaque texel keeps its leaf
            // green all the way down; what it cannot do is invent a colour for a block that
            // is transparent throughout, and those come out black. Filling them is what stops
            // the GPU averaging a tree toward black as it shrinks.
            if alpha {
                bleed_cutout(&mut rgba, w, h);
            }
            Some(MapTexture {
                material: i as u32,
                name,
                width: w,
                height: h,
                alpha,
                rgba,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The wire format the viewer reads
// ---------------------------------------------------------------------------

/// Bytes before the vertex data in a scenery blob. Four-byte aligned so the app can adopt
/// every array as a typed-array view rather than copying it.
pub const SCENERY_HEADER: usize = 56;

/// Bytes per entry in the blob's texture table.
pub const TEXTURE_ENTRY: usize = 20;

/// Pack a mesh and its surfaces for the IPC channel.
///
/// ```text
///  0  "FSCN"
///  4  u16 version, u16 flags
///  8  u32 vertex_count, u32 index_count, u32 group_count, u32 texture_count
/// 24  u32 piece_count, u32 reserved
/// 32  f32[6] world bounds
/// 56  positions  vc*12 | normals vc*12 | uvs vc*8 | indices ic*4
///     groups     gc*12  (material, tri_start, tri_count)
///     pieces     (ic/3)*4  which separable piece each triangle belongs to
///     textures   tc*20  (material, width, height, flags, byte_len), then the pixels
/// ```
///
/// Raw bytes because this is a few hundred thousand triangles and a couple of dozen
/// surfaces; as JSON numbers it would cost more to parse than the archive read that
/// produced it.
pub fn scenery_blob(mesh: &MapMesh, textures: &[MapTexture]) -> Vec<u8> {
    // The reader walks the sections back to back and works out where the surfaces start from
    // the triangle count, so a mesh carrying fewer piece ids than triangles doesn't lose the
    // pieces — it loses the whole scenery. Appending a placed prop without numbering it was
    // exactly that.
    debug_assert_eq!(
        mesh.object_of_tri.len(),
        mesh.indices.len() / 3,
        "one piece id per triangle"
    );
    let (lo, hi) = mesh.bounds();
    let vc = mesh.vertex_count() as u32;
    let ic = mesh.indices.len() as u32;
    let pixels: usize = textures.iter().map(|t| t.rgba.len()).sum();

    let mut out = Vec::with_capacity(
        SCENERY_HEADER
            + mesh.positions.len() * 4
            + mesh.normals.len() * 4
            + mesh.uvs.len() * 4
            + mesh.indices.len() * 4
            + mesh.groups.len() * 12
            + textures.len() * TEXTURE_ENTRY
            + pixels,
    );
    out.extend_from_slice(b"FSCN");
    out.extend_from_slice(&2u16.to_le_bytes()); // version
    out.extend_from_slice(&0u16.to_le_bytes()); // flags, none yet
    out.extend_from_slice(&vc.to_le_bytes());
    out.extend_from_slice(&ic.to_le_bytes());
    out.extend_from_slice(&(mesh.groups.len() as u32).to_le_bytes());
    out.extend_from_slice(&(textures.len() as u32).to_le_bytes());
    // A word of its own: a dense track has more than sixty-five thousand pieces, and a count
    // that saturates is a number that lies.
    out.extend_from_slice(&(mesh.objects.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for v in lo.iter().chain(hi.iter()) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in mesh
        .positions
        .iter()
        .chain(mesh.normals.iter())
        .chain(mesh.uvs.iter())
    {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in &mesh.indices {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for g in &mesh.groups {
        out.extend_from_slice(&g.material.to_le_bytes());
        out.extend_from_slice(&g.tri_start.to_le_bytes());
        out.extend_from_slice(&g.tri_count.to_le_bytes());
    }
    // One id per triangle. A ray hit gives the viewer a face; this turns that face into the
    // thing it belongs to, which is what makes a tent something you can point at.
    for id in &mesh.object_of_tri {
        out.extend_from_slice(&id.to_le_bytes());
    }
    for t in textures {
        out.extend_from_slice(&t.material.to_le_bytes());
        out.extend_from_slice(&t.width.to_le_bytes());
        out.extend_from_slice(&t.height.to_le_bytes());
        // Bit 0: the surface is an alpha cut-out and has to be drawn with an alpha test.
        out.extend_from_slice(&u32::from(t.alpha).to_le_bytes());
        out.extend_from_slice(&(t.rgba.len() as u32).to_le_bytes());
    }
    for t in textures {
        out.extend_from_slice(&t.rgba);
    }
    out
}

/// Pack a track's surfaces on their own, for the pass that follows the mesh.
///
/// ```text
///  0  "FSRF"
///  4  u16 version, u16 flags
///  8  u32 surface_count
/// 12  u32 reserved (keeps the table four-byte aligned)
/// 16  surface_count x 20  (material, width, height, flags, byte_len), then the pixels
/// ```
pub fn surfaces_blob(textures: &[MapTexture]) -> Vec<u8> {
    let pixels: usize = textures.iter().map(|t| t.rgba.len()).sum();
    let mut out = Vec::with_capacity(SURFACES_HEADER + textures.len() * TEXTURE_ENTRY + pixels);
    out.extend_from_slice(b"FSRF");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(textures.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for t in textures {
        out.extend_from_slice(&t.material.to_le_bytes());
        out.extend_from_slice(&t.width.to_le_bytes());
        out.extend_from_slice(&t.height.to_le_bytes());
        out.extend_from_slice(&u32::from(t.alpha).to_le_bytes());
        out.extend_from_slice(&(t.rgba.len() as u32).to_le_bytes());
    }
    for t in textures {
        out.extend_from_slice(&t.rgba);
    }
    out
}


// ---------------------------------------------------------------------------
// The ground the game draws
// ---------------------------------------------------------------------------

/// One painted layer of a track's ground.
///
/// This is what the game puts under the bike, and it is not what the `.trh` coverage masks
/// describe — those are the *physics* surfaces, a friction table keyed by
/// `asphalt/grass/sand/kerb/soil/concrete`, and on a published track they are all but unused:
/// Indiana paints one 256×256 patch of `concrete` across a 2049² grid and nothing else. Drawn
/// from those, a real track is a flat brown slab.
///
/// The paint lives here instead: a stack of layers, each a tiling sheet with its own mask,
/// composited in order. The first covers everything and carries no mask; the rest cut into it.
#[derive(Clone, Debug)]
pub struct GroundLayer {
    /// The sheet, reduced for tiling rather than for extent.
    pub sheet: MapTexture,
    /// How many times the sheet repeats across the ground, per axis. Read rather than
    /// assumed: Indiana's base is 200 and its dark soil 180, which over 525 m is about
    /// 2.6 and 2.9 metres a tile — close enough to the viewer's old hardcoded 4 m to look
    /// plausible and far enough to read as the wrong ground.
    pub tile_u: f32,
    pub tile_v: f32,
    /// Coverage, one byte a texel, laid across the whole ground. `None` on the base layer.
    ///
    /// Coarse on purpose — Indiana's are 256² against a 2049² heightfield — which is why the
    /// whole stack is a few hundred kilobytes rather than the hundreds of megabytes the sheets
    /// themselves come to.
    pub mask: Option<GroundMask>,
}

#[derive(Clone, Debug)]
pub struct GroundMask {
    pub width: u32,
    pub height: u32,
    /// `width * height`, one byte a texel.
    pub coverage: Vec<u8>,
}

/// How far a layer's sheet is reduced. It tiles, so it needs frequency rather than extent —
/// the same reasoning as [`ground_sheet`], and it keeps a six-layer stack inside a few
/// megabytes rather than the ninety a map's own 1024² sheets would cost.
const LAYER_SHEET_DIM: u32 = 256;

/// A sheet record, in either of the two shapes the format uses.
///
/// A material's colour sheet and the normal map hanging off it are the same record with two
/// words' difference — the normal carries neither the zero before its width nor the zero
/// before its length:
///
/// ```text
/// colour  name(100)  u32 0  w  h  hash(16)  u32 0  len  8 B  data
/// normal  name(100)         w  h  hash(16)         len  8 B  data
/// ```
///
/// Both are tried and the record has to prove itself: a NUL-padded ASCII name, power-of-two
/// dimensions, and a length that stays inside the file. Reading the normal's length field
/// where the colour's sits finds a zero that is not there, which is what made every normal map
/// unreadable — and with it, every material after the first in a map's surface table.
fn layer_sheet_at(b: &[u8], o: usize) -> Option<(String, u32, u32, usize, usize)> {
    if o + 148 > b.len() {
        return None;
    }
    let name = b.get(o..o + 100)?;
    let end = name.iter().position(|c| *c == 0)?;
    // Printable rather than graphic: a sheet may be named `CK_KeLLz Tree ATLAS 2 8K`, and
    // rejecting the space threw away SFDR's whole surface table at its very first record.
    // The first character still has to be graphic, so a field of blanks is not a name.
    if end == 0
        || !name[0].is_ascii_graphic()
        || !name[..end].iter().all(|c| (0x20..=0x7e).contains(c))
    {
        return None;
    }
    if !name[end..].iter().all(|c| *c == 0) {
        return None;
    }
    let pow2 = |v: u32| (4..=8192).contains(&v) && v.is_power_of_two();
    // `(where the width sits, where the length sits)`, colour first.
    for (w_off, len_off) in [(104usize, 132usize), (100, 124)] {
        if w_off == 104 {
            // The colour's two spacer words: one before the width, one where sub-records
            // would be counted. The normal has neither, which is how they are told apart.
            if u32le(b, o + 100) != 0 || o + 132 > b.len() || u32le(b, o + 128) != 0 {
                continue;
            }
        }
        let (w, h) = (u32le(b, o + w_off), u32le(b, o + w_off + 4));
        if !pow2(w) || !pow2(h) {
            continue;
        }
        if o + len_off + 12 > b.len() {
            continue;
        }
        // The length counts the eight bytes that follow it.
        let len = u32le(b, o + len_off) as usize;
        if len < 8 {
            continue;
        }
        let data = o + len_off + 12;
        if data + len - 8 > b.len() {
            continue;
        }
        return Some((
            String::from_utf8_lossy(&name[..end]).into_owned(),
            w,
            h,
            data,
            data + len - 8,
        ));
    }
    None
}

/// A mask record: `u32 w, u32 h, u32 len`, eight bytes, then DEFLATE over one byte a texel.
///
/// Proved by inflating rather than by its header alone — it has to come out at exactly `w * h`
/// bytes. Nothing else in a map does that, so this cannot pick up a sheet or a run of geometry.
fn layer_mask_at(b: &[u8], o: usize) -> Option<(GroundMask, usize)> {
    if o + 20 > b.len() {
        return None;
    }
    let (w, h, len) = (u32le(b, o), u32le(b, o + 4), u32le(b, o + 8) as usize);
    let ok = |v: u32| (16..=8192).contains(&v) && v.is_power_of_two();
    if !ok(w) || !ok(h) || len < 16 {
        return None;
    }
    let data = o + 20;
    let take = len - 8;
    if data + take > b.len() {
        return None;
    }
    let want = w as usize * h as usize;
    if take > want {
        return None;
    }
    let mut out = Vec::with_capacity(want);
    let mut dec = flate2::bufread::DeflateDecoder::new(&b[data..data + take]);
    use std::io::Read;
    let mut buf = [0u8; 1 << 16];
    loop {
        match dec.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.len() > want {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
    if out.len() != want {
        return None;
    }
    // Bottom-up, like every other sheet the compilers write — see the flip in [`ground_sheet`].
    // Left as it lies, a mask puts the riding line on the wrong side of the track.
    let row = w as usize;
    for y in 0..(h as usize) / 2 {
        let (top, bottom) = (y * row, (h as usize - 1 - y) * row);
        for x in 0..row {
            out.swap(top + x, bottom + x);
        }
    }
    Some((
        GroundMask {
            width: w,
            height: h,
            coverage: out,
        },
        data + take,
    ))
}

/// Whether a name marks a normal map rather than a colour sheet.
///
/// Wider than [`is_normal_name`], which splits on separators and so reads `dirtnorm` and
/// `grassnorm` — Smokey Pines' two — as words of their own. A normal map drawn as ground is a
/// lilac sheet, so the test errs toward calling one.
fn is_layer_normal(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    // The environment cube every layer carries beside its sheets. Drawn as ground it is a
    // sky reflection tiled across the dirt.
    n == "env"
        || is_normal_name(&n)
        || n.ends_with("norm")
        || n.ends_with("_n")
        || n.ends_with("_n_s")
        || n.ends_with("nrm")
}

/// How many bytes of material a layer states before its sheet: fourteen floats.
const LAYER_MATERIAL: usize = 56;

/// How far past a sheet the walk will look for the trailer that closes its layer.
///
/// Far enough to step over a normal map's payload — a 1024² sheet is a megabyte or two — and
/// bounded so a walk that has lost the stack gives up rather than reading the rest of the file.
const TRAILER_REACH: usize = 8 << 20;

/// What closes a layer: how its sheet tiles, and the mask that cuts it into the one below.
struct Trailer {
    tile_u: f32,
    tile_v: f32,
    mask: Option<GroundMask>,
    /// Where the next layer's material record begins.
    next: usize,
}

/// Find the trailer that follows a layer's sheet.
///
/// Searched for rather than read at a fixed offset, because what sits between the sheet and
/// the trailer is a normal map whose own header is a variable run of words — two samples were
/// not enough to pin it, and a walk that assumes one is wrong on the tracks that carry the
/// other. The trailer proves itself instead: two plausible tiling floats, a flag that is zero
/// or one, and — where it is one — a mask that inflates to exactly its stated size.
fn trailer_after(b: &[u8], from: usize, limit: usize) -> Option<Trailer> {
    let sane = |v: f32| v.is_finite() && (0.01..=100_000.0).contains(&v);
    let mut o = from;
    while o + 20 < limit.min(b.len()) {
        let (tu, tv) = (f32le(b, o), f32le(b, o + 4));
        if sane(tu) && sane(tv) {
            match u32le(b, o + 8) {
                // A masked layer: the mask is proof enough on its own.
                1 => {
                    if let Some((mask, after)) = layer_mask_at(b, o + 12) {
                        return Some(Trailer {
                            tile_u: tu,
                            tile_v: tv,
                            mask: Some(mask),
                            next: after + 4,
                        });
                    }
                }
                // The base layer carries no mask, so there is nothing to prove it by except
                // what comes next: the following layer's material and sheet have to parse.
                0 => {
                    let next = o + 16;
                    if next + 60 < b.len()
                        && (1..=4).contains(&u32le(b, next + LAYER_MATERIAL))
                        && layer_sheet_at(b, next + LAYER_MATERIAL + 4).is_some()
                    {
                        return Some(Trailer {
                            tile_u: tu,
                            tile_v: tv,
                            mask: None,
                            next,
                        });
                    }
                }
                _ => {}
            }
        }
        o += 4;
    }
    None
}

/// Walk the layer stack from `start`, which is a layer's material record.
///
/// `decode` is false while the stack's phase is still being searched for: the walk is the same
/// but no sheet is inflated, because hundreds of candidate starts each decoding a megabyte of
/// pixels to be thrown away is most of a minute of work.
fn march_layers(b: &[u8], start: usize, decode: bool) -> Vec<GroundLayer> {
    let mut out = Vec::new();
    let mut o = start;
    while out.len() < 64 && o + 60 < b.len() {
        if !(1..=4).contains(&u32le(b, o + LAYER_MATERIAL)) {
            break;
        }
        let Some((name, w, h, data, end)) = layer_sheet_at(b, o + LAYER_MATERIAL + 4) else {
            break;
        };
        let Some(t) = trailer_after(b, end, end + TRAILER_REACH) else {
            break;
        };
        // A normal map opens no layer of its own; it hangs off the colour sheet above it.
        if !is_layer_normal(&name) {
            let sheet = if decode {
                match inflate(b, data, end - data, w, h) {
                    Some(mut rgba) => {
                        flip_rows(&mut rgba, w, h);
                        let (rgba, rw, rh) = reduce(rgba, w, h, LAYER_SHEET_DIM);
                        Some((rgba, rw, rh))
                    }
                    None => None,
                }
            } else {
                Some((Vec::new(), w, h))
            };
            if let Some((rgba, rw, rh)) = sheet {
                out.push(GroundLayer {
                    sheet: MapTexture {
                        material: out.len() as u32,
                        name,
                        width: rw,
                        height: rh,
                        alpha: false,
                        rgba,
                    },
                    tile_u: t.tile_u,
                    tile_v: t.tile_v,
                    mask: t.mask,
                });
            }
        }
        o = t.next;
    }
    out
}

/// The ground a track is painted with, layer by layer.
///
/// This is what the game puts under the bike. It is a march through the records rather than a
/// scan for them, because the stack only means anything in order — but a march has to start in
/// phase, and nothing in the file announces where the stack begins. So the phase is *found*:
/// every sheet record in the map is tried as the stack's first, and the one that walks furthest
/// wins. A march that starts in the wrong place fails within a layer or two, because a layer
/// only closes on a mask that inflates to exactly its stated size.
///
/// Sheets are not inflated while the phase is being searched for — only the winning march
/// decodes anything. A map carries well over a hundred textures that are banners and foliage.
pub fn ground_layers(b: &[u8]) -> Vec<GroundLayer> {
    // Every sheet record is a candidate first layer; its material record sits 60 bytes back.
    let mut starts: Vec<usize> = Vec::new();
    let mut o = 0usize;
    while o + 160 < b.len() {
        if b[o].is_ascii_graphic()
            && b[o + 99] == 0
            && o >= LAYER_MATERIAL + 4
            && (1..=4).contains(&u32le(b, o - 4))
            && layer_sheet_at(b, o).is_some()
        {
            starts.push(o - LAYER_MATERIAL - 4);
        }
        o += 4;
    }

    // The march that reads the most layers is the stack. Ties go to the earliest, which is the
    // one that starts at the base rather than part way down it.
    let mut best: Option<(usize, usize)> = None; // (layers, start)
    for s in starts {
        let n = march_layers(b, s, false).len();
        if n > best.map_or(0, |(m, _)| m) {
            best = Some((n, s));
        }
    }
    match best {
        Some((_, s)) => march_layers(b, s, true),
        None => Vec::new(),
    }
}

/// The two tiling floats a layer keeps between the end of its sheet and the next record.
///
/// Scanned for rather than read at a fixed offset, because what sits between them and the
/// sheet is a normal map of unknown length. They are the pair immediately before a flag word.
fn tiling_before(b: &[u8], from: usize, to: usize) -> Option<(f32, f32)> {
    let mut o = from;
    let mut found = None;
    while o + 12 <= to.min(b.len()) {
        let flag = u32le(b, o + 8);
        if flag <= 1 {
            let (u, v) = (f32le(b, o), f32le(b, o + 4));
            if (0.01..=100_000.0).contains(&u) && (0.01..=100_000.0).contains(&v) {
                found = Some((u, v));
            }
        }
        o += 4;
    }
    found
}

/// The ground stack, packed for the front end.
///
/// One table of fixed-width entries and then the pixels, so the whole stack crosses the bridge
/// as one buffer rather than as a JSON object per layer with a megabyte of numbers in it.
pub fn ground_layers_blob(layers: &[GroundLayer]) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        GROUND_LAYERS_HEADER
            + layers.len() * GROUND_LAYER_ENTRY
            + layers
                .iter()
                .map(|l| l.sheet.rgba.len() + l.mask.as_ref().map_or(0, |m| m.coverage.len()))
                .sum::<usize>(),
    );
    out.extend_from_slice(b"FGLY");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(layers.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for l in layers {
        out.extend_from_slice(&l.sheet.width.to_le_bytes());
        out.extend_from_slice(&l.sheet.height.to_le_bytes());
        out.extend_from_slice(&(l.sheet.rgba.len() as u32).to_le_bytes());
        out.extend_from_slice(&l.tile_u.to_le_bytes());
        out.extend_from_slice(&l.tile_v.to_le_bytes());
        let (mw, mh, ml) = l
            .mask
            .as_ref()
            .map_or((0, 0, 0), |m| (m.width, m.height, m.coverage.len() as u32));
        out.extend_from_slice(&mw.to_le_bytes());
        out.extend_from_slice(&mh.to_le_bytes());
        out.extend_from_slice(&ml.to_le_bytes());
    }
    for l in layers {
        out.extend_from_slice(&l.sheet.rgba);
    }
    for l in layers {
        if let Some(m) = &l.mask {
            out.extend_from_slice(&m.coverage);
        }
    }
    out
}

/// Bytes before the layer table.
pub const GROUND_LAYERS_HEADER: usize = 16;
/// Bytes per layer in that table.
pub const GROUND_LAYER_ENTRY: usize = 32;

/// Bytes before the surface table. Four-byte aligned, same reasoning as [`SCENERY_HEADER`].
pub const SURFACES_HEADER: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a layer stack the way a compiled map holds one, so the walk can be tested
    /// without a three-hundred-megabyte track on disk.
    fn synth_stack(layers: &[(&str, f32, Option<u32>)]) -> Vec<u8> {
        use flate2::{write::DeflateEncoder, Compression};
        use std::io::Write;
        let mut b = vec![0u8; 64]; // something in front, so offset zero is never the answer
        for (name, tile, mask) in layers {
            b.extend_from_slice(&[0u8; LAYER_MATERIAL]);
            b.extend_from_slice(&1u32.to_le_bytes()); // one sheet
            let mut nm = vec![0u8; 100];
            nm[..name.len()].copy_from_slice(name.as_bytes());
            b.extend_from_slice(&nm);
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&8u32.to_le_bytes()); // 8x8 sheet
            b.extend_from_slice(&8u32.to_le_bytes());
            b.extend_from_slice(&[0u8; 16]); // hash
            b.extend_from_slice(&0u32.to_le_bytes()); // no sub-records
            let px = vec![128u8; 8 * 8 * 4];
            let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
            e.write_all(&px).unwrap();
            let z = e.finish().unwrap();
            b.extend_from_slice(&((z.len() + 8) as u32).to_le_bytes());
            b.extend_from_slice(&[0u8; 8]);
            b.extend_from_slice(&z);
            b.extend_from_slice(&0u32.to_le_bytes()); // no secondary map
            b.extend_from_slice(&tile.to_le_bytes());
            b.extend_from_slice(&tile.to_le_bytes());
            match mask {
                Some(d) => {
                    b.extend_from_slice(&1u32.to_le_bytes());
                    b.extend_from_slice(&d.to_le_bytes());
                    b.extend_from_slice(&d.to_le_bytes());
                    let cov = vec![200u8; (*d as usize) * (*d as usize)];
                    let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
                    e.write_all(&cov).unwrap();
                    let z = e.finish().unwrap();
                    b.extend_from_slice(&((z.len() + 8) as u32).to_le_bytes());
                    b.extend_from_slice(&[0u8; 8]);
                    b.extend_from_slice(&z);
                }
                None => b.extend_from_slice(&0u32.to_le_bytes()),
            }
            b.extend_from_slice(&0u32.to_le_bytes()); // the word that closes a layer
        }
        b.extend_from_slice(&[0u8; 256]);
        b
    }

    #[test]
    fn reads_a_layer_stack_in_order() {
        let b = synth_stack(&[
            ("base_c", 200.0, None),
            ("line_c", 180.0, Some(64)),
            ("grass_c", 150.0, Some(32)),
        ]);
        let ls = ground_layers(&b);
        assert_eq!(
            ls.iter().map(|l| l.sheet.name.as_str()).collect::<Vec<_>>(),
            ["base_c", "line_c", "grass_c"],
            "the stack has to come back in the order it is painted"
        );
        // The base covers everything, so it states no mask; the rest cut into it.
        assert!(ls[0].mask.is_none());
        assert_eq!(ls[1].mask.as_ref().unwrap().width, 64);
        assert_eq!(ls[2].mask.as_ref().unwrap().width, 32);
        // Tiling is read, not assumed — the viewer used to hardcode four metres for every one.
        assert_eq!(ls[0].tile_u, 200.0);
        assert_eq!(ls[1].tile_u, 180.0);
        assert_eq!(ls[2].tile_u, 150.0);
        assert_eq!(ls[1].mask.as_ref().unwrap().coverage.len(), 64 * 64);
    }

    /// A normal map hangs off the colour sheet above it and is not a layer of its own. Drawn
    /// as ground it is a lilac sheet.
    #[test]
    fn a_normal_map_opens_no_layer() {
        let b = synth_stack(&[
            ("base_c", 200.0, None),
            ("line_n_s", 180.0, Some(32)),
            ("grassnorm", 150.0, Some(32)),
        ]);
        let names: Vec<_> = ground_layers(&b)
            .iter()
            .map(|l| l.sheet.name.clone())
            .collect();
        assert_eq!(names, ["base_c"], "only the colour sheet is ground");
    }

    /// A normal map is the colour record minus two words — no zero before its width, none
    /// before its length. Reading it at the colour's offsets finds a zero that is not there,
    /// which made every normal map in a map unreadable and stopped the walk at the first
    /// layer that carried one.
    #[test]
    fn a_normal_map_record_is_read_at_its_own_offsets() {
        use flate2::{write::DeflateEncoder, Compression};
        use std::io::Write;
        let px = vec![7u8; 8 * 8 * 4];
        let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
        e.write_all(&px).unwrap();
        let z = e.finish().unwrap();

        let build = |colour: bool| {
            let mut b = vec![0u8; 100];
            b[..6].copy_from_slice(b"sand_n");
            if colour {
                b.extend_from_slice(&0u32.to_le_bytes());
            }
            b.extend_from_slice(&8u32.to_le_bytes());
            b.extend_from_slice(&8u32.to_le_bytes());
            b.extend_from_slice(&[0u8; 16]);
            if colour {
                b.extend_from_slice(&0u32.to_le_bytes());
            }
            b.extend_from_slice(&((z.len() + 8) as u32).to_le_bytes());
            b.extend_from_slice(&[0u8; 8]);
            b.extend_from_slice(&z);
            b.extend_from_slice(&[0u8; 32]);
            b
        };

        for colour in [true, false] {
            let b = build(colour);
            let got = layer_sheet_at(&b, 0);
            let (name, w, h, data, end) = got.unwrap_or_else(|| {
                panic!("the {} shape did not parse", if colour { "colour" } else { "normal" })
            });
            assert_eq!(name, "sand_n");
            assert_eq!((w, h), (8, 8));
            assert_eq!(end - data, z.len(), "the payload has to end where the stream does");
        }
    }

    /// The environment cube sits beside a layer's sheets. Drawn as ground it is a sky
    /// reflection tiled across the dirt.
    #[test]
    fn an_env_cube_opens_no_layer() {
        assert!(is_layer_normal("env"));
        assert!(is_layer_normal("ENV"));
        assert!(!is_layer_normal("gravel_c"));
    }

    /// A card whose material bound keeps its place; one whose material did not is a standing
    /// sheet of paper the size of the thing it was meant to be, and comes out.
    #[test]
    fn only_the_unbound_cards_come_out() {
        // Two upright cards, one per material, plus a floor so the mesh is not all cards.
        let mut mesh = MapMesh::default();
        let mut push_card = |m: &mut MapMesh, x: f32| {
            let base = m.vertex_count() as u32;
            for (dx, y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 2.0)] {
                m.positions.extend_from_slice(&[x + dx, y, 0.0]);
                m.normals.extend_from_slice(&[0.0, 0.0, 1.0]);
                m.uvs.extend_from_slice(&[0.0, 0.0]);
            }
            m.indices.extend_from_slice(&[base, base + 1, base + 2]);
        };
        push_card(&mut mesh, 0.0);
        push_card(&mut mesh, 10.0);
        mesh.groups = vec![
            Group { material: 0, tri_start: 0, tri_count: 1 },
            Group { material: 7, tri_start: 1, tri_count: 1 },
        ];

        // Material 7 has no sheet; material 0 has one.
        let kept = without_cards_for(&mesh, |m| m >= 1);
        assert_eq!(
            kept.indices.len() / 3,
            1,
            "the bound material's card stays and the unbound one goes"
        );
        assert_eq!(kept.groups.iter().map(|g| g.material).collect::<Vec<_>>(), [0]);

        // And with everything bound, nothing is dropped at all.
        let all = without_cards_for(&mesh, |_| false);
        assert_eq!(all.indices.len() / 3, 2);
    }

    /// A foliage sheet is mostly transparent, and what is stored behind the cut is black. A
    /// mipmap averages colour and alpha separately, so shrinking a tree blends its leaves with
    /// that black and the tree goes dark as it recedes — which is what Indiana's did.
    #[test]
    fn a_cut_out_keeps_its_colour_when_it_shrinks() {
        // A quarter of the texels are opaque green; the rest are transparent black.
        let (w, h) = (16u32, 16u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = (y * w as usize + x) * 4;
                if (x + y) % 4 == 0 {
                    rgba[i..i + 4].copy_from_slice(&[40, 160, 40, 255]);
                }
            }
        }
        let mean = |px: &[u8]| -> u32 {
            let n = (px.len() / 4) as u32;
            px.chunks(4)
                .map(|c| (c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000)
                .sum::<u32>()
                / n.max(1)
        };
        let before = mean(&rgba);
        bleed_cutout(&mut rgba, w, h);
        let after = mean(&rgba);

        // The leaf colour itself, which is what every mip should converge on.
        let leaf = (40 * 299 + 160 * 587 + 40 * 114) / 1000;
        assert!(
            before < leaf / 2,
            "the sheet has to start dark for this to be testing anything ({before} vs {leaf})"
        );
        assert!(
            after > leaf * 3 / 4,
            "after bleeding, a mip should land near the leaf colour, not the black behind it \
             ({after} against a leaf of {leaf})"
        );
        // The cut-out still cuts out: alpha is untouched.
        assert!(rgba.chunks(4).any(|c| c[3] == 0), "alpha must not be filled in");
        assert!(rgba.chunks(4).any(|c| c[3] == 255));
    }

    /// Nothing in a map announces where the ground begins, so the walk finds its own phase.
    /// A map with no stack in it at all must come back empty rather than reading its scenery
    /// as ground — a banner tiled across the terrain is worse than no change at all.
    #[test]
    fn a_map_with_no_ground_reads_none() {
        assert!(ground_layers(&[0u8; 4096]).is_empty());
        let noise: Vec<u8> = (0..40_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        assert!(ground_layers(&noise).is_empty());
    }

    /// Ground words used to be looked for anywhere in a name, which is how Abydos's
    /// `logo-dirtmaster` came to be its dirt and I40's `banner_lucasoil` its soil. Both are
    /// near-flat pictures, so the detail layer they won did nothing at all.
    #[test]
    fn a_logo_is_not_the_ground_it_is_named_after() {
        let words = ["dirt", "soil", "sand"];
        assert_eq!(ground_rank("logo-dirtmaster", &words), None);
        assert_eq!(ground_rank("banner_lucasoil", &words), None);
        assert_eq!(ground_rank("RR_TrackMarker", &["track"]), None);
    }

    #[test]
    fn a_whole_word_outranks_a_word_it_only_starts() {
        let words = ["dirt", "sand"];
        let whole = ground_rank("dry_dirt", &words).expect("dirt");
        let prefix = ground_rank("Dirtplane", &words).expect("still dirt");
        assert!(whole < prefix, "{whole} should beat {prefix}");
        // And a later word said outright still loses to an earlier one said at all.
        assert!(ground_rank("sand-dark", &words).expect("sand") > whole);
    }

    #[test]
    fn a_name_is_read_however_its_author_wrote_it() {
        assert_eq!(name_words("soil_dark_c"), ["soil", "dark", "c"]);
        assert_eq!(name_words("sand-dark-normal"), ["sand", "dark", "normal"]);
        assert_eq!(name_words("Dirt_2"), ["dirt"]);
    }

    #[test]
    fn a_normal_map_says_so_in_any_of_the_ways_it_is_written() {
        for n in ["Dirt2_n", "soil_white_n_s", "sand-dark-normal", "track-norm"] {
            assert!(is_normal_name(n), "{n} is a normal map");
        }
        for n in ["grass", "sand-new-2", "dirtyconcrete", "banner_fmf"] {
            assert!(!is_normal_name(n), "{n} is a picture");
        }
    }

    /// What lets a sheet find its own partner rather than any old normal map.
    #[test]
    fn a_sheet_and_its_normal_reduce_to_the_same_stem() {
        assert_eq!(name_stem("soil_dark_c"), name_stem("soil_dark_n_s"));
        assert_eq!(name_stem("sand-dark"), name_stem("sand-dark-normal"));
        assert_eq!(name_stem("dirt"), name_stem("dirt_n"));
        assert_ne!(name_stem("soil_dark_c"), name_stem("soil_white_n_s"));
    }

    /// Build a `.map` byte-for-byte the way a real one is laid out.
    fn synth(materials: usize, verts: usize, tris: usize) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&304u32.to_le_bytes());
        b.extend_from_slice(&(materials as u32).to_le_bytes());
        b.extend_from_slice(&vec![0u8; materials * MATERIAL_RECORD]);
        b.extend_from_slice(&(verts as u32).to_le_bytes());

        // The vertex block is structure-of-arrays: every attribute is one run of `verts`
        // elements, so the block is filled column by column rather than vertex by vertex.
        let mut block = vec![0u8; verts * STRIDE];
        for i in 0..verts {
            let p = i * 12;
            for (k, v) in [i as f32, (i * 2) as f32, (i * 3) as f32]
                .iter()
                .enumerate()
            {
                block[p + k * 4..p + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            // Unit normal, which is what proves the attribute offset.
            let n = verts * NORMAL_AT + i * 12;
            for (k, v) in [0.0f32, 1.0, 0.0].iter().enumerate() {
                block[n + k * 4..n + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(&block);

        b.extend_from_slice(&(tris as u32).to_le_bytes());
        for t in 0..tris {
            for k in 0..3 {
                let v = ((t + k) % verts) as u32;
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(&0u32.to_le_bytes()); // bvh count
        b
    }

    #[test]
    fn reads_a_map() {
        let bytes = synth(48, 64, 20);
        let m = parse(&bytes).expect("a well-formed map should read");
        assert_eq!(m.vertex_count(), 64);
        assert_eq!(m.triangle_count(), 20);
        assert_eq!(m.materials, 48);
        // Positions come back in the order they were written, which pins the column layout.
        assert_eq!(&m.positions[0..3], &[0.0, 0.0, 0.0]);
        assert_eq!(&m.positions[3..6], &[1.0, 2.0, 3.0]);
        assert_eq!(&m.normals[3..6], &[0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_track_with_no_scenery_is_not_a_failure() {
        // The OEM drag strip's shape: zero materials, nothing behind them.
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&304u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&vec![0u8; 4096]);
        assert!(is_map(&b));
        assert!(parse(&b).is_none());
    }

    #[test]
    fn the_normal_offset_is_load_bearing() {
        // Shift the whole vertex block by one attribute slot and the normals stop being
        // unit vectors, which is exactly the misread the check exists to catch.
        let mut bytes = synth(2, 64, 10);
        let head = MATERIALS_AT + 2 * MATERIAL_RECORD;
        let vs = head + 4;
        let at = vs + 64 * NORMAL_AT;
        for k in 0..12 {
            bytes[at + k] = 0;
        }
        assert!(
            parse(&bytes).is_none(),
            "a zero normal is not a unit normal"
        );
    }

    #[test]
    fn an_index_past_the_end_is_rejected() {
        let mut bytes = synth(1, 32, 4);
        let head = MATERIALS_AT + MATERIAL_RECORD;
        let ic = head + 4 + 32 * STRIDE;
        bytes[ic + 4..ic + 8].copy_from_slice(&999u32.to_le_bytes());
        assert!(parse(&bytes).is_none());
    }

    /// Does the `.edf` texture scanner read a `.map`'s textures too?
    ///
    /// ```text
    /// FROST_MAP="…/Millville.map" \
    ///   cargo test --bin mxb-app -- --ignored --nocapture map_textures_via_edf_scanner
    /// ```
    #[test]
    #[ignore = "needs a real .map — set FROST_MAP"]
    fn map_textures_via_edf_scanner() {
        let path = std::env::var("FROST_MAP").expect("set FROST_MAP");
        let bytes = std::fs::read(&path).expect("read the map");
        let t0 = std::time::Instant::now();
        let texs = crate::edf::embedded_textures(&bytes);
        println!("{} textures in {:?}", texs.len(), t0.elapsed());
        let diffuse: Vec<_> = texs
            .iter()
            .filter(|t| t.name.ends_with("_c") || t.name.ends_with("_c_a"))
            .collect();
        println!("  diffuse: {}", diffuse.len());
        for (i, t) in diffuse.iter().enumerate().take(50) {
            println!(
                "   {i:<3} {:<34} {}x{} {}",
                t.name,
                t.width,
                t.height,
                if t.name.ends_with("_c_a") {
                    "ALPHA"
                } else {
                    ""
                }
            );
        }
        if let Some(t) = diffuse.first() {
            let px = crate::edf::inflate_texture(&bytes, t).expect("inflate the first diffuse");
            println!(
                "  first inflates to {} bytes (expected {})",
                px.len(),
                t.width as usize * t.height as usize * 4
            );
        }
    }

    #[test]
    fn not_a_map_at_all() {
        assert!(parse(b"EDF\0nonsense").is_none());
        assert!(parse(&[]).is_none());
    }

    #[test]
    fn blob_round_trips_its_header() {
        let m = parse(&synth(3, 40, 12)).unwrap();
        let tex = vec![MapTexture {
            material: 0,
            name: "tent_c".into(),
            width: 2,
            height: 2,
            alpha: false,
            rgba: vec![7u8; 16],
        }];
        let blob = scenery_blob(&m, &tex);
        assert_eq!(&blob[0..4], b"FSCN");
        assert_eq!(u32le(&blob, 4) & 0xFFFF, 2, "version 2");
        assert_eq!(u32le(&blob, 8), 40, "vertex count");
        assert_eq!(u32le(&blob, 12), 36, "index count");
        assert_eq!(u32le(&blob, 20), 1, "one surface");
        // Its own word, so a dense track's count doesn't saturate.
        assert_eq!(u32le(&blob, 24), m.objects.len() as u32, "the piece count");
        let groups = u32le(&blob, 16) as usize;
        assert_eq!(
            blob.len(),
            SCENERY_HEADER
                + 40 * 12   // positions
                + 40 * 12   // normals
                + 40 * 8    // uvs
                + 36 * 4    // indices
                + groups * 12
                + 12 * 4    // one piece id per triangle
                + TEXTURE_ENTRY
                + 16,
            "every section back to back, in the order the header declares"
        );
        // The pixels are last, so the tail is exactly what went in.
        assert_eq!(&blob[blob.len() - 16..], &[7u8; 16]);
    }

    #[test]
    fn cut_out_cards_come_out_and_the_ground_stays() {
        // A flat ground quad and a standing card. Without a picture on it the card is a sheet
        // of paper, so it goes; the ground it stands on does not.
        let positions = vec![
            0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 4.0, // ground, lying flat
            1.0, 0.0, 1.0, 1.0, 3.0, 1.0, 2.0, 3.0, 1.0, // a card, standing three metres
        ];
        let mesh = MapMesh {
            positions,
            normals: vec![0.0; 18],
            uvs: vec![0.0; 12],
            indices: vec![0, 1, 2, 3, 4, 5],
            groups: vec![Group {
                material: 0,
                tri_start: 0,
                tri_count: 2,
            }],
            objects: Vec::new(),
            object_of_tri: Vec::new(),
            materials: 1,
        };
        let out = without_cards(&mesh);
        assert_eq!(out.triangle_count(), 1, "the card goes, the ground stays");
        assert_eq!(&out.indices[..], &[0, 1, 2]);
        assert_eq!(out.groups.len(), 1);
        assert_eq!(
            out.groups[0].tri_count, 1,
            "the group is recounted, not left stale"
        );
        assert_eq!(out.objects.len(), 1, "and the pieces are found again");
    }

    #[test]
    fn a_mesh_comes_apart_into_the_things_it_is_made_of() {
        // Two triangles sharing an edge, and a third off on its own: two pieces, not three.
        let positions = vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, // a welded quad
            9.0, 0.0, 9.0, 10.0, 0.0, 9.0, 9.0, 0.0, 10.0, // and something else
        ];
        let indices = vec![0, 1, 2, 1, 3, 2, 4, 5, 6];
        let mats = vec![0, 0, 1];
        let (objects, of_tri) = split_into_objects(7, &indices, &mats, &positions);
        assert_eq!(objects.len(), 2, "the shared edge holds the quad together");
        assert_eq!(of_tri, vec![0, 0, 1]);
        assert_eq!(objects[0].tri_count, 2);
        assert_eq!(objects[1].tri_count, 1);
        assert_eq!(objects[1].material, 1);
        // Bounds are the piece's own, not the whole mesh's.
        assert_eq!(objects[1].min, [9.0, 0.0, 9.0]);
        assert_eq!(objects[0].max, [1.0, 0.0, 1.0]);
    }

    #[test]
    fn groups_are_sorted_and_merged_by_material() {
        // Two runs of one material either side of another: the sort has to bring the pair
        // together, and the merge leave one group per material rather than three.
        let raw = vec![
            Group {
                material: 2,
                tri_start: 0,
                tri_count: 3,
            },
            Group {
                material: 5,
                tri_start: 3,
                tri_count: 2,
            },
            Group {
                material: 2,
                tri_start: 5,
                tri_count: 4,
            },
        ];
        let indices: Vec<u32> = (0..27).collect();
        let (sorted, merged) = sort_by_material(indices, &raw);
        assert_eq!(merged.len(), 2, "one group per material");
        assert_eq!(
            merged[0],
            Group {
                material: 2,
                tri_start: 0,
                tri_count: 7
            }
        );
        assert_eq!(
            merged[1],
            Group {
                material: 5,
                tri_start: 7,
                tri_count: 2
            }
        );
        assert_eq!(sorted.len(), 27, "no triangle is lost");
        // Material 2's second run must follow its first, not stay where it was.
        assert_eq!(&sorted[0..9], &[0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(
            &sorted[9..21],
            &[15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26]
        );
        assert_eq!(&sorted[21..27], &[9, 10, 11, 12, 13, 14]);
    }

    #[test]
    fn a_reduced_surface_keeps_its_average() {
        // Fully opaque, so the alpha weighting is a plain mean and the arithmetic is visible.
        let rgba = vec![
            0, 0, 0, 255, 100, 100, 100, 255, 200, 200, 200, 255, 40, 40, 40, 255,
        ];
        let (out, w, h) = reduce(rgba, 2, 2, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, vec![85, 85, 85, 255]);
    }

    #[test]
    fn reducing_a_cutout_does_not_drag_it_towards_black() {
        // One bright leaf against three transparent black texels — the shape of a foliage
        // sheet. A flat average would return a quarter-bright pixel; weighting by alpha keeps
        // the leaf's own colour and lets alpha alone carry how much of it survived.
        let rgba = vec![
            200, 220, 180, 255, // the leaf
            0, 0, 0, 0, //
            0, 0, 0, 0, //
            0, 0, 0, 0,
        ];
        let (out, _, _) = reduce(rgba, 2, 2, 1);
        assert_eq!(&out[0..3], &[200, 220, 180], "the leaf keeps its colour");
        assert_eq!(out[3], 63, "and alpha says a quarter of it is there");
    }

    #[test]
    fn sheets_are_turned_the_right_way_up() {
        // Two rows, distinguishable: the file's first row is the picture's top, and a card's
        // V zero is its foot, so the rows have to swap or every tree hangs upside down.
        let mut rgba = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        flip_rows(&mut rgba, 2, 2);
        assert_eq!(
            rgba,
            vec![9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    /// Point this at a real `.map` — unpacked, not the archive — to see what reads out of it:
    ///
    /// ```text
    /// FROST_MAP="…/Millville.map" \
    ///   cargo test --bin mxb-app -- --ignored --nocapture read_a_real_map
    /// ```
    ///
    /// Checks the two things a transcription bug would break: that every normal is unit
    /// length, and that the mesh lands in the world rather than around the origin.
    #[test]
    #[ignore = "needs a real .map — set FROST_MAP"]
    fn read_a_real_map() {
        let path = std::env::var("FROST_MAP").expect("set FROST_MAP to an unpacked .map");
        let bytes = std::fs::read(&path).expect("read the map");
        println!("{path}: {} bytes", bytes.len());

        let Some(m) = parse(&bytes) else {
            println!(
                "no scenery in this map (materials = {:?})",
                material_count(&bytes)
            );
            return;
        };
        let (lo, hi) = m.bounds();
        println!(
            "  {} materials, {} verts, {} tris",
            m.materials,
            m.vertex_count(),
            m.triangle_count()
        );
        println!(
            "  x[{:.1}, {:.1}] y[{:.1}, {:.1}] z[{:.1}, {:.1}]",
            lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );

        let mut worst: f32 = 0.0;
        for n in m.normals.chunks_exact(3) {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            worst = worst.max((len - 1.0).abs());
        }
        println!("  worst normal length error: {worst:.6}");
        assert!(
            worst < 0.01,
            "normals must be unit — the attribute offset is wrong otherwise"
        );
        assert!(m.indices.iter().all(|i| (*i as usize) < m.vertex_count()));
        assert!(hi[0] - lo[0] > 1.0, "a real map spans real ground");
    }

    /// What a published track's ground sheets actually are: their names, their sizes, and the
    /// statistics that decide whether ground reads as ground.
    ///
    /// The terrain's own textures are the last records in a `.map`'s table — the `dirt`,
    /// `grass` and `gravel` a track paints its ground with — and until now nothing had ever
    /// looked at them. Ours were designed against Indiana's *loose sheets* on disk, which is
    /// a different thing from what the compiler bakes in.
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn ground_sheets() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let entry = names
            .iter()
            .find(|n| n.to_ascii_lowercase().ends_with(".map"))
            .expect("a .map")
            .clone();
        let bytes = crate::track::read_entry(path, &entry).unwrap();
        let sheets = textures(&bytes, 512);
        println!("{}  —  {} sheets", path.file_stem().unwrap().to_string_lossy(), sheets.len());
        for t in &sheets {
            if t.rgba.is_empty() {
                continue;
            }
            let n = (t.rgba.len() / 4) as f32;
            let mut sum = [0.0f32; 3];
            for p in t.rgba.chunks_exact(4) {
                for c in 0..3 {
                    sum[c] += p[c] as f32;
                }
            }
            let mean = [sum[0] / n, sum[1] / n, sum[2] / n];
            // Spread of luminance: how much grain the sheet carries, which is what makes
            // ground read as ground rather than as a flat colour.
            let mut var = 0.0f32;
            let lum = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            let mean_l = 0.299 * mean[0] + 0.587 * mean[1] + 0.114 * mean[2];
            for p in t.rgba.chunks_exact(4) {
                let d = lum(p) - mean_l;
                var += d * d;
            }
            let sd = (var / n).sqrt();
            // And how much of that grain is fine rather than blotchy: the mean absolute
            // difference between neighbouring pixels.
            let w = t.width as usize;
            let mut grad = 0.0f32;
            let mut k = 0.0f32;
            for y in 0..t.height as usize {
                for x in 1..w {
                    let a = &t.rgba[(y * w + x) * 4..];
                    let b = &t.rgba[(y * w + x - 1) * 4..];
                    grad += (lum(a) - lum(b)).abs();
                    k += 1.0;
                }
            }
            println!(
                "  {:<28} {:>4}x{:<4} mean ({:>5.1},{:>5.1},{:>5.1}) luma {:>5.1}  \
                 spread {:>5.1}  grain {:>4.2}",
                t.name, t.width, t.height, mean[0], mean[1], mean[2], mean_l, sd, grad / k.max(1.0)
            );
        }
    }

    /// Every record in a `.map`'s texture table, unfiltered — what the file carries, before
    /// any judgement about which of them bind to geometry.
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn map_survey() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let entry = names
            .iter()
            .find(|n| n.to_ascii_lowercase().ends_with(".map"))
            .expect("a .map")
            .clone();
        let bytes = crate::track::read_entry(path, &entry).unwrap();
        let all = survey(&bytes);
        println!("{} — {} records in the texture table", entry, all.len());
        for (n, w, h) in &all {
            println!("    {n:<32} {w}x{h}");
        }
    }

    /// How much of a track's ground is *mesh* rather than painted heightmap.
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn map_mesh_extent() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let entry = names
            .iter()
            .find(|n| n.to_ascii_lowercase().ends_with(".map"))
            .expect("a .map")
            .clone();
        let bytes = crate::track::read_entry(path, &entry).unwrap();
        let Some(m) = parse(&bytes) else {
            println!("no mesh");
            return;
        };
        let n = m.positions.len() / 3;
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in m.positions.chunks_exact(3) {
            for c in 0..3 {
                lo[c] = lo[c].min(v[c]);
                hi[c] = hi[c].max(v[c]);
            }
        }
        // How flat it is: scenery stands up, ground lies down.
        let mut low = 0usize;
        for v in m.positions.chunks_exact(3) {
            if v[1] - lo[1] < (hi[1] - lo[1]) * 0.15 {
                low += 1;
            }
        }
        println!(
            "{}: {n} vertices, {} triangles, {} groups — spans {:.0} x {:.0} m, {:.0} m tall; \
             {:.0}% of its vertices lie in the bottom sixth of that",
            entry,
            m.indices.len() / 3,
            m.groups.len(),
            hi[0] - lo[0],
            hi[2] - lo[2],
            hi[1] - lo[1],
            low as f32 * 100.0 / n as f32,
        );
    }

    /// Per-group extent and flatness — is any of a track's mesh a ground carpet, or is it all
    /// scenery standing up beside the track?
    ///
    /// The question matters because "Indiana's map carries 665,000 triangles and ours 18,000"
    /// admits two readings, and they call for completely different work: a textured ground
    /// laid over the terrain is an architecture we do not have, while tents and trucks are
    /// scenery we already place.
    #[test]
    #[ignore = "needs a track — set FROST_TRACK"]
    fn map_groups() {
        let var = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::Path::new(&var);
        let names = crate::track::entry_names(path).unwrap();
        let entry =
            names.iter().find(|n| n.to_ascii_lowercase().ends_with(".map")).unwrap().clone();
        let bytes = crate::track::read_entry(path, &entry).unwrap();
        let Some(m) = parse(&bytes) else {
            println!("no mesh");
            return;
        };
        let sheets = textures(&bytes, 64);
        let mut rows: Vec<(f32, String, usize, f32, f32, f32)> = Vec::new();
        for g in &m.groups {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            let (mut up, mut tris) = (0usize, 0usize);
            let from = g.tri_start as usize * 3;
            let to = ((g.tri_start + g.tri_count) as usize * 3).min(m.indices.len());
            if from >= to {
                continue;
            }
            for t in m.indices[from..to].chunks_exact(3) {
                tris += 1;
                let p: Vec<[f32; 3]> = t
                    .iter()
                    .map(|&i| {
                        let i = i as usize * 3;
                        [m.positions[i], m.positions[i + 1], m.positions[i + 2]]
                    })
                    .collect();
                for q in &p {
                    for c in 0..3 {
                        lo[c] = lo[c].min(q[c]);
                        hi[c] = hi[c].max(q[c]);
                    }
                }
                // A ground triangle faces up; a wall or a card does not.
                let (a, b, c) = (p[0], p[1], p[2]);
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                if (n[1] / len).abs() > 0.85 {
                    up += 1;
                }
            }
            let name = sheets
                .iter()
                .find(|t| t.material == g.material)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| format!("material {}", g.material));
            let flat = up as f32 * 100.0 / tris as f32;
            let area = (hi[0] - lo[0]) * (hi[2] - lo[2]);
            rows.push((area * flat, name, tris, hi[0] - lo[0], hi[2] - lo[2], flat));
        }
        rows.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("{entry}");
        println!(
            "  {:<28} {:>8} {:>9} {:>9} {:>8}",
            "sheet", "tris", "span x", "span z", "flat %"
        );
        for (_, name, tris, sx, sz, flat) in rows.iter() {
            println!("  {name:<28} {tris:>8} {sx:>8.0}m {sz:>8.0}m {flat:>7.0}%");
        }
    }
}
