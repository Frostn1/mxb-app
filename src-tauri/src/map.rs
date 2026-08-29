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
    if !all.iter().any(|(n, ..)| is_colour_name(n)) {
        return Vec::new();
    }
    all.into_iter()
        .filter(|(n, ..)| is_colour_name(n))
        .take(count)
        .map(|(n, w, h, ..)| (n, w, h))
        .collect()
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
pub fn textures(b: &[u8], max_dim: u32) -> Vec<MapTexture> {
    let Some((from, count)) = texture_table(b) else {
        return Vec::new();
    };
    let all = colour_records(b, from);
    if !all.iter().any(|(n, ..)| is_colour_name(n)) {
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
            let (rgba, w, h) = reduce(rgba, w, h, max_dim.max(1));
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
pub const SCENERY_HEADER: usize = 48;

/// Bytes per entry in the blob's texture table.
pub const TEXTURE_ENTRY: usize = 20;

/// Pack a mesh and its surfaces for the IPC channel.
///
/// ```text
///  0  "FSCN"
///  4  u16 version, u16 flags
///  8  u32 vertex_count, u32 index_count, u32 group_count, u32 texture_count
/// 24  f32[6] world bounds
/// 48  positions  vc*12 | normals vc*12 | uvs vc*8 | indices ic*4
///     groups     gc*12  (material, tri_start, tri_count)
///     pieces     (ic/3)*4  which separable piece each triangle belongs to
///     textures   tc*20  (material, width, height, flags, byte_len), then the pixels
/// ```
///
/// Raw bytes because this is a few hundred thousand triangles and a couple of dozen
/// surfaces; as JSON numbers it would cost more to parse than the archive read that
/// produced it.
pub fn scenery_blob(mesh: &MapMesh, textures: &[MapTexture]) -> Vec<u8> {
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
    // Top half carries how many separable pieces the mesh came apart into; the low half is
    // the surface count. Both fit, and it keeps the header the size the app already reads.
    let packed = (textures.len() as u32 & 0xFFFF) | ((mesh.objects.len().min(0xFFFF) as u32) << 16);
    out.extend_from_slice(&packed.to_le_bytes());
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

/// Bytes before the surface table. Four-byte aligned, same reasoning as [`SCENERY_HEADER`].
pub const SURFACES_HEADER: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

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
        // One word carries both counts: surfaces low, separable pieces high.
        assert_eq!(u32le(&blob, 20) & 0xFFFF, 1, "one surface");
        assert_eq!(
            u32le(&blob, 20) >> 16,
            m.objects.len() as u32,
            "and the piece count"
        );
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
}
