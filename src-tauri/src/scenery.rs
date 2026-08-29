//! What stands on a track, as opposed to the ground it stands on.
//!
//! Three sources, all in the same world frame as the terrain grid — metres, with the grid's
//! own corner at the origin:
//!
//! * The `.map` carries the baked scenery as one mesh — tents, bales, banners, fences, and
//!   the landscape beyond the terrain square. This is nearly all of it. See [`crate::map`].
//! * The `.scr` places a handful of loose props by name, each a `.edf` shipped beside it.
//!   Flags, wind turbines, an orbiting aeroplane. Their meshes are folded into the same
//!   mesh, so the viewer draws one thing.
//! * `marshals.cfg`, the `.tsc` and the `.ssc` pin fixtures to points with no mesh in the
//!   track at all — marshal posts, TV cameras, crowd sound. Those come back as
//!   [`Placement`]s for the viewer to mark.
//!
//! That the frame really is shared is not an assumption: `marshals.cfg` states each post's
//! height, and sampling the terrain grid underneath reproduces all of them to within 0.1 m.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::cfg::CfgNode;
use crate::map::{self, Group, MapMesh, MapTexture};

/// Cache generation. Bump when the blob layout or the parse changes shape.
// v2: the mesh gained UVs and material groups, and the surfaces travel with it.
// v3: the mesh and the surfaces are cached apart, so the first can be served without the
// second having been decoded at all.
const MESH_CACHE: &str = "track-scenery-v3";
const SURFACE_CACHE: &str = "track-surfaces-v3";

/// How many decoded scenery meshes to keep. Smaller than the terrain's: one of these is
/// about 30 MB, against 16 for a terrain master.
const CACHE_KEEP: usize = 4;

/// A point on the track where something stands that the track ships no mesh for.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// `prop`, `marshal`, `camera` or `sound`. A key, not prose — the app translates it.
    pub kind: String,
    /// The `.edf` for a prop, the `.wav` for a sound, otherwise the name the track gave it.
    pub name: String,
    /// World metres, in the game's own left-handed frame — the viewer mirrors X to draw it,
    /// exactly as it does the terrain.
    pub pos: [f32; 3],
    /// Degrees, where the file states one: a marshal's `long`, a camera's `rot`.
    pub heading: Option<f32>,
}

/// What a track's scenery amounts to, minus the geometry itself.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SceneryInfo {
    /// Materials the `.map` declares. Zero means the track carries no scenery at all.
    pub materials: u32,
    pub vertex_count: u32,
    pub triangle_count: u32,
    /// `.scr` props whose mesh was found and folded in.
    pub props: u32,
    /// `.scr` props naming a `.edf` the track doesn't ship. Surfaced rather than swallowed:
    /// it's the difference between a track with no props and one whose props went missing.
    pub props_missing: u32,
    /// The `.map` entry this came out of, empty when the track has none.
    pub entry: String,
    /// Connected pieces the scenery comes apart into — one per tent, trailer or foliage card.
    #[serde(default)]
    pub objects: u32,
    /// One run of triangles per material: `[material, tri_start, tri_count]`.
    #[serde(default)]
    pub groups: Vec<[u32; 3]>,
    /// The surfaces, in the order their pixels are stored.
    #[serde(default)]
    pub textures: Vec<TextureInfo>,
}

/// A surface's shape, kept apart from its pixels so the descriptor stays small.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextureInfo {
    pub material: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// An alpha cut-out — foliage, crowd, fencing. Drawn without an alpha test it is a slab.
    pub alpha: bool,
}

/// A track's scenery, decoded.
#[derive(Clone, Debug, Default)]
pub struct Scenery {
    pub info: SceneryInfo,
    pub mesh: MapMesh,
    pub textures: Vec<MapTexture>,
}

// ---------------------------------------------------------------------------
// Picking the files out of a track
// ---------------------------------------------------------------------------

/// The stem a track's own files are named after — `Millville.pkz` holds `Millville.map`.
fn track_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Entries with the given extension, the one named after the track first.
///
/// A track can ship several — a pack with an MX, an SX and a supercross layout ships a `.scr`
/// each — and the one matching the archive's own name is the track being looked at.
fn entries_with_ext(names: &[String], ext: &str, stem: &str) -> Vec<String> {
    let mut out: Vec<String> = names
        .iter()
        .filter(|n| {
            n.rsplit('.')
                .next()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        })
        .cloned()
        .collect();
    out.sort_by_key(|n| {
        let base = n.rsplit('/').next().unwrap_or(n).to_ascii_lowercase();
        let matches = base.strip_suffix(&format!(".{ext}")).unwrap_or(&base) == stem;
        (!matches, base.clone())
    });
    out
}

// ---------------------------------------------------------------------------
// The cfg-shaped placement files
// ---------------------------------------------------------------------------

/// `x, y, z` on one line, as the `.scr`, `.ssc` and `.tsc` write a position.
fn vec3_csv(s: &str) -> Option<[f32; 3]> {
    let mut it = s.split(',').map(|p| p.trim().parse::<f32>());
    let v = [it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?];
    v.iter().all(|f| f.is_finite()).then_some(v)
}

/// `pos { x = .. y = .. z = .. }`, as `marshals.cfg` writes one.
fn vec3_block(node: &CfgNode) -> Option<[f32; 3]> {
    let p = node.block("pos")?;
    let f = |k: &str| p.get(k)?.trim().parse::<f32>().ok();
    let v = [f("x")?, f("y")?, f("z")?];
    v.iter().all(|f| f.is_finite()).then_some(v)
}

/// Child blocks named `prefix0`, `prefix1`, … in index order.
///
/// Read by scanning what's there rather than counting up from the file's own `num…` key:
/// the two disagree in the wild, and the blocks are the ones that carry positions.
fn numbered<'a>(node: &'a CfgNode, prefix: &str) -> Vec<&'a CfgNode> {
    let mut found: Vec<(u32, &CfgNode)> = node
        .blocks
        .iter()
        .filter_map(|(k, v)| {
            let n = k.strip_prefix(prefix)?;
            (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                .then(|| n.parse().ok().map(|i| (i, v)))?
        })
        .collect();
    found.sort_by_key(|(i, _)| *i);
    found.into_iter().map(|(_, v)| v).collect()
}

/// The props a `.scr` places, as `(placement, rotation in degrees)`.
fn read_scr(bytes: &[u8]) -> Vec<(Placement, [f32; 3])> {
    let root = crate::cfg::parse(bytes);
    numbered(&root, "obj")
        .into_iter()
        .filter_map(|o| {
            let name = o.get("name")?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let pos = vec3_csv(o.get("pos")?)?;
            let rot = o.get("rot").and_then(vec3_csv).unwrap_or([0.0; 3]);
            Some((
                Placement {
                    kind: "prop".into(),
                    name,
                    pos,
                    heading: Some(rot[1]),
                },
                rot,
            ))
        })
        .collect()
}

/// Marshal posts and the flagman. `long` is the heading the marshal faces.
fn read_marshals(bytes: &[u8]) -> Vec<Placement> {
    let root = crate::cfg::parse(bytes);
    let mut out = Vec::new();
    let mut named: Vec<(&String, &CfgNode)> = root
        .blocks
        .iter()
        .filter(|(k, _)| k.as_str() == "flagman" || k.starts_with("marshal"))
        .collect();
    named.sort_by(|a, b| a.0.cmp(b.0));
    for (name, node) in named {
        if let Some(pos) = vec3_block(node) {
            out.push(Placement {
                kind: "marshal".into(),
                name: name.clone(),
                pos,
                heading: node.get("long").and_then(|v| v.trim().parse().ok()),
            });
        }
    }
    out
}

/// The TV cameras a `.tsc` sets up, across every camera set in it.
fn read_cameras(bytes: &[u8]) -> Vec<Placement> {
    let root = crate::cfg::parse(bytes);
    let mut out = Vec::new();
    for (n, set) in numbered(&root, "camset").into_iter().enumerate() {
        let set_name = set.get("name").unwrap_or("").trim().to_string();
        for (i, cam) in numbered(set, "camera").into_iter().enumerate() {
            let Some(pos) = cam.get("pos").and_then(vec3_csv) else {
                continue;
            };
            let label = if set_name.is_empty() {
                format!("camset{n} · camera{i}")
            } else {
                format!("{set_name} · camera{i}")
            };
            out.push(Placement {
                kind: "camera".into(),
                name: label,
                pos,
                heading: cam.get("rot").and_then(|v| v.trim().parse().ok()),
            });
        }
    }
    out
}

/// Crowd and ambience sources from the `.ssc`.
fn read_sounds(bytes: &[u8]) -> Vec<Placement> {
    let root = crate::cfg::parse(bytes);
    numbered(&root, "source")
        .into_iter()
        .filter_map(|s| {
            let pos = s.get("pos").and_then(vec3_csv)?;
            Some(Placement {
                kind: "sound".into(),
                name: s.get("data").unwrap_or("").trim().to_string(),
                pos,
                heading: None,
            })
        })
        .collect()
}

/// Every fixture a track pins to a point, without touching the `.map`.
///
/// Cheap on purpose: these files are a few kilobytes each, so the viewer can mark them while
/// the scenery mesh — which is most of a gigabyte of archive away — is still being read.
pub fn read_placements(path: &str) -> Result<Vec<Placement>> {
    let p = Path::new(path);
    let names = crate::track::entry_names(p)?;
    let stem = track_stem(p);
    let mut out = Vec::new();

    let mut take = |ext: &str, f: &dyn Fn(&[u8]) -> Vec<Placement>| {
        for entry in entries_with_ext(&names, ext, &stem) {
            if let Ok(bytes) = crate::track::read_entry(p, &entry) {
                let found = f(&bytes);
                if !found.is_empty() {
                    out.extend(found);
                    // One layout's worth. A pack ships a `.scr` per layout, and merging them
                    // would stand every pack's props on whichever track is open.
                    break;
                }
            }
        }
    };

    take("scr", &|b| {
        read_scr(b).into_iter().map(|(p, _)| p).collect()
    });
    take("tsc", &read_cameras);
    take("ssc", &read_sounds);

    for entry in &names {
        if entry
            .rsplit('/')
            .next()
            .is_some_and(|b| b.eq_ignore_ascii_case("marshals.cfg"))
        {
            if let Ok(bytes) = crate::track::read_entry(p, entry) {
                out.extend(read_marshals(&bytes));
            }
            break;
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Folding the `.scr` props into the scenery
// ---------------------------------------------------------------------------

/// Rotate by `deg` (X, then Y, then Z) and translate to `pos`, in the game's own frame.
fn place(mesh: &mut MapMesh, node_pos: &[f32], node_nrm: &[f32], deg: [f32; 3], pos: [f32; 3]) {
    let (sx, cx) = deg[0].to_radians().sin_cos();
    let (sy, cy) = deg[1].to_radians().sin_cos();
    let (sz, cz) = deg[2].to_radians().sin_cos();
    // R = Rz * Ry * Rx, written out rather than composed to keep this a single pass.
    let m = [
        cz * cy,
        cz * sy * sx - sz * cx,
        cz * sy * cx + sz * sx,
        sz * cy,
        sz * sy * sx + cz * cx,
        sz * sy * cx - cz * sx,
        -sy,
        cy * sx,
        cy * cx,
    ];
    let rot = |v: &[f32]| {
        [
            m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
            m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
            m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
        ]
    };
    for p in node_pos.chunks_exact(3) {
        let r = rot(p);
        mesh.positions
            .extend_from_slice(&[r[0] + pos[0], r[1] + pos[1], r[2] + pos[2]]);
    }
    for n in node_nrm.chunks_exact(3) {
        mesh.normals.extend_from_slice(&rot(n));
    }
}

/// Fold every `.scr` prop whose `.edf` the track ships into `mesh`.
///
/// Returns `(placed, missing)`. A prop naming a mesh that isn't there is worth counting: a
/// track that lost its flags looks the same as one that never had any.
fn bake_props(path: &Path, names: &[String], stem: &str, mesh: &mut MapMesh) -> (u32, u32) {
    let Some(entry) = entries_with_ext(names, "scr", stem).into_iter().find(|e| {
        crate::track::read_entry(path, e)
            .map(|b| !read_scr(&b).is_empty())
            .unwrap_or(false)
    }) else {
        return (0, 0);
    };
    let Ok(bytes) = crate::track::read_entry(path, &entry) else {
        return (0, 0);
    };

    let prop_material = mesh.materials;
    let (mut placed, mut missing) = (0, 0);
    for (p, rot) in read_scr(&bytes) {
        // The `.scr` names a bare file; the archive may hold it in a folder.
        let found = names.iter().find(|n| {
            n.rsplit('/')
                .next()
                .is_some_and(|b| b.eq_ignore_ascii_case(&p.name))
        });
        let Some(edf_entry) = found else {
            missing += 1;
            continue;
        };
        let Ok(edf_bytes) = crate::track::read_entry(path, edf_entry) else {
            missing += 1;
            continue;
        };
        // Left in the game's frame: the viewer mirrors X once, over everything at once.
        let nodes = crate::edf::parse(&edf_bytes);
        if nodes.is_empty() {
            missing += 1;
            continue;
        }
        for node in &nodes {
            if node.positions.is_empty() || node.indices.is_empty() {
                continue;
            }
            let base = mesh.vertex_count() as u32;
            let normals = if node.normals.len() == node.positions.len() {
                node.normals.clone()
            } else {
                vec![0.0; node.positions.len()]
            };
            place(mesh, &node.positions, &normals, rot, p.pos);
            // A prop's own sheets aren't read, so it gets a material past the map's last —
            // one with no surface behind it, which the viewer draws plain.
            let verts = node.positions.len() / 3;
            if node.uvs.len() == verts * 2 {
                mesh.uvs.extend_from_slice(&node.uvs);
            } else {
                mesh.uvs.extend(std::iter::repeat(0.0).take(verts * 2));
            }
            let tri_start = (mesh.indices.len() / 3) as u32;
            mesh.indices.extend(node.indices.iter().map(|i| i + base));
            let tri_count = (mesh.indices.len() / 3) as u32 - tri_start;
            match mesh.groups.last_mut() {
                Some(last) if last.material == prop_material => last.tri_count += tri_count,
                _ => mesh.groups.push(Group {
                    material: prop_material,
                    tri_start,
                    tri_count,
                }),
            }
        }
        placed += 1;
    }
    (placed, missing)
}

// ---------------------------------------------------------------------------
// The whole thing, cached
// ---------------------------------------------------------------------------

/// The last `.map` read, kept whole so the surfaces pass doesn't repeat it.
///
/// Pulling a map out of a track is nearly all of what loading one costs — measured across a
/// library, between half a second and twenty-eight, against milliseconds to parse what comes
/// out. The viewer asks for the mesh and then the surfaces, so without this the archive is
/// read twice for one look at a track.
///
/// One entry, and it holds a few hundred megabytes, so it is dropped as soon as the surfaces
/// have been taken from it rather than kept against a second viewing — that is what the disk
/// cache is for.
fn map_bytes_cache() -> &'static std::sync::Mutex<Option<(String, Vec<u8>, String)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<(String, Vec<u8>, String)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

fn remember_map(key: &str, entry: &str, bytes: &[u8]) {
    if let Ok(mut c) = map_bytes_cache().lock() {
        *c = Some((key.to_string(), bytes.to_vec(), entry.to_string()));
    }
}

/// Take the held bytes if they belong to this track, clearing the slot.
fn take_map(key: &str) -> Option<(Vec<u8>, String)> {
    let mut c = map_bytes_cache().lock().ok()?;
    match c.as_ref() {
        Some((k, ..)) if k == key => c.take().map(|(_, b, e)| (b, e)),
        _ => None,
    }
}

/// Decode a track's scenery, going to the archive only when nothing nearer has it.
///
/// The mesh and the surfaces are fetched separately on purpose. A track's shape is a few
/// hundred thousand triangles that parse in milliseconds once the archive read is done; its
/// surfaces are hundreds of megabytes to inflate and reduce. Waiting for the second before
/// showing the first is most of a second of blank canvas for no reason, so the viewer asks
/// for the mesh, draws it, and asks for the surfaces after.
pub fn load(app: &tauri::AppHandle, path: &str) -> Result<Scenery> {
    let key = cache_key(path)?;
    if let Some(hit) = cache_file(app, &key, MESH_CACHE).and_then(|f| read_cache(&f)) {
        return Ok(hit);
    }
    let scenery = decode_with_key(Path::new(path), false, Some(&key))?;
    if let Some(f) = cache_file(app, &key, MESH_CACHE) {
        write_cache(&f, &scenery);
        prune_cache(app, MESH_CACHE);
    }
    Ok(scenery)
}

/// A track's surfaces, cached apart from its mesh.
pub fn load_surfaces(app: &tauri::AppHandle, path: &str) -> Result<Vec<MapTexture>> {
    let key = cache_key(path)?;
    if let Some(hit) = cache_file(app, &key, SURFACE_CACHE).and_then(|f| read_surface_cache(&f)) {
        return Ok(hit);
    }
    let scenery = decode_with_key(Path::new(path), true, Some(&key))?;
    if let Some(f) = cache_file(app, &key, SURFACE_CACHE) {
        write_surface_cache(&f, &scenery.textures);
        prune_cache(app, SURFACE_CACHE);
    }
    Ok(scenery.textures)
}

/// The expensive path. A track's `.map` is the largest file it ships — most of it textures
/// this doesn't read — which is exactly why the result is worth caching.
fn decode(path: &Path, want_surfaces: bool) -> Result<Scenery> {
    decode_with_key(path, want_surfaces, None)
}

fn decode_with_key(path: &Path, want_surfaces: bool, key: Option<&str>) -> Result<Scenery> {
    let names = crate::track::entry_names(path)?;
    let stem = track_stem(path);

    let mut mesh = MapMesh::default();
    let mut textures: Vec<MapTexture> = Vec::new();
    let mut info = SceneryInfo::default();

    // The surfaces pass follows the mesh pass on the same track, so the bytes are usually
    // still in hand. Taking them clears the slot: they are big, and one pass over a track
    // is all they are for.
    let held = key.and_then(take_map);
    for entry in entries_with_ext(&names, "map", &stem) {
        let bytes = match &held {
            Some((b, e)) if *e == entry => b.clone(),
            _ => match crate::track::read_entry(path, &entry) {
                Ok(b) => b,
                Err(_) => continue,
            },
        };
        if !map::is_map(&bytes) {
            continue;
        }
        match map::parse(&bytes) {
            Some(m) => {
                info.materials = m.materials;
                info.entry = entry.clone();
                mesh = m;
                if want_surfaces {
                    textures = map::textures(&bytes, map::MAX_TEXTURE_DIM);
                } else {
                    // Hold the bytes for the surfaces pass that is about to follow.
                    if let Some(k) = key {
                        remember_map(k, &entry, &bytes);
                    }
                    // Named only, so the viewer knows how many to expect and can size its
                    // material slots before a single pixel has been inflated.
                    info.textures = map::declared(&bytes)
                        .into_iter()
                        .enumerate()
                        .map(|(i, (name, width, height))| TextureInfo {
                            material: i as u32,
                            name,
                            width,
                            height,
                            alpha: false,
                        })
                        .collect();
                }
            }
            // A `.map` that declares no materials carries no scenery — an ordinary track,
            // not a failed read. Keep the entry so the app can say it looked.
            None => {
                info.entry = entry.clone();
            }
        }
        break;
    }

    let (placed, missing) = bake_props(path, &names, &stem, &mut mesh);
    info.props = placed;
    info.props_missing = missing;
    info.vertex_count = mesh.vertex_count() as u32;
    info.triangle_count = mesh.triangle_count() as u32;
    info.objects = mesh.objects.len() as u32;
    info.groups = mesh
        .groups
        .iter()
        .map(|g| [g.material, g.tri_start, g.tri_count])
        .collect();
    if want_surfaces {
        info.textures = textures
            .iter()
            .map(|t| TextureInfo {
                material: t.material,
                name: t.name.clone(),
                width: t.width,
                height: t.height,
                alpha: t.alpha,
            })
            .collect();
    }

    if mesh.is_empty() && info.entry.is_empty() {
        bail!("no .map in {path:?}");
    }
    Ok(Scenery {
        info,
        mesh,
        textures,
    })
}

/// Pack a track's mesh for the viewer. See [`map::scenery_blob`].
pub fn blob(scenery: &Scenery) -> Vec<u8> {
    map::scenery_blob(&scenery.mesh, &scenery.textures)
}

/// Pack a track's surfaces on their own.
pub fn surfaces_blob(textures: &[MapTexture]) -> Vec<u8> {
    map::surfaces_blob(textures)
}

/// On disk: the surface descriptors as JSON, then their pixels raw.
fn write_surface_cache(file: &Path, textures: &[MapTexture]) {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let head: Vec<TextureInfo> = textures
        .iter()
        .map(|t| TextureInfo {
            material: t.material,
            name: t.name.clone(),
            width: t.width,
            height: t.height,
            alpha: t.alpha,
        })
        .collect();
    let Ok(head) = serde_json::to_vec(&head) else {
        return;
    };
    let mut out = Vec::with_capacity(4 + head.len());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    for t in textures {
        out.extend_from_slice(&t.rgba);
    }
    let _ = std::fs::write(file, out);
}

fn read_surface_cache(file: &Path) -> Option<Vec<MapTexture>> {
    let bytes = std::fs::read(file).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    let head_len = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    let at = 4usize.checked_add(head_len)?;
    if bytes.len() < at {
        return None;
    }
    let head: Vec<TextureInfo> = serde_json::from_slice(&bytes[4..at]).ok()?;
    let want: usize = head
        .iter()
        .map(|t| t.width as usize * t.height as usize * 4)
        .sum();
    let body = &bytes[at..];
    if body.len() != want {
        return None;
    }
    let mut out = Vec::with_capacity(head.len());
    let mut o = 0usize;
    for t in head {
        let n = t.width as usize * t.height as usize * 4;
        out.push(MapTexture {
            material: t.material,
            name: t.name,
            width: t.width,
            height: t.height,
            alpha: t.alpha,
            rgba: body[o..o + n].to_vec(),
        });
        o += n;
    }
    Some(out)
}

fn cache_key(path: &str) -> Result<String> {
    let m = std::fs::metadata(path)?;
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(format!("{name}:{}:{mtime}", m.len()))
}

fn cache_file(app: &tauri::AppHandle, key: &str, dir_name: &str) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_cache_dir().ok()?.join(dir_name);
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Some(dir.join(format!("{safe}.bin")))
}

/// On disk: the descriptor as JSON, then the mesh arrays and the surface pixels raw.
///
/// The descriptor carries the group and surface tables — a few dozen entries each, small
/// enough that JSON costs nothing — while everything counted in megabytes stays bytes.
fn write_cache(file: &Path, s: &Scenery) {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(head) = serde_json::to_vec(&s.info) else {
        return;
    };
    let pixels: usize = s.textures.iter().map(|t| t.rgba.len()).sum();
    let mut out = Vec::with_capacity(4 + head.len() + s.mesh.positions.len() * 8 + pixels);
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    for v in s
        .mesh
        .positions
        .iter()
        .chain(s.mesh.normals.iter())
        .chain(s.mesh.uvs.iter())
    {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in &s.mesh.indices {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for t in &s.textures {
        out.extend_from_slice(&t.rgba);
    }
    let _ = std::fs::write(file, out);
}

fn read_cache(file: &Path) -> Option<Scenery> {
    let bytes = std::fs::read(file).ok()?;
    if bytes.len() < 4 {
        return None;
    }
    let head_len = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    let at = 4usize.checked_add(head_len)?;
    if bytes.len() < at {
        return None;
    }
    let info: SceneryInfo = serde_json::from_slice(&bytes[4..at]).ok()?;

    let vc = info.vertex_count as usize;
    let ic = info.triangle_count as usize * 3;
    let pixels: usize = info
        .textures
        .iter()
        .map(|t| t.width as usize * t.height as usize * 4)
        .sum();
    // Positions, normals, UVs, indices, then every surface's pixels.
    let want = vc * 12 + vc * 12 + vc * 8 + ic * 4 + pixels;
    let body = &bytes[at..];
    if body.len() != want {
        return None;
    }
    let f = |b: &[u8]| -> Vec<f32> {
        b.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };
    let mut o = 0usize;
    let positions = f(&body[o..o + vc * 12]);
    o += vc * 12;
    let normals = f(&body[o..o + vc * 12]);
    o += vc * 12;
    let uvs = f(&body[o..o + vc * 8]);
    o += vc * 8;
    let indices: Vec<u32> = body[o..o + ic * 4]
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    o += ic * 4;

    let mut textures = Vec::with_capacity(info.textures.len());
    for t in &info.textures {
        let n = t.width as usize * t.height as usize * 4;
        textures.push(MapTexture {
            material: t.material,
            name: t.name.clone(),
            width: t.width,
            height: t.height,
            alpha: t.alpha,
            rgba: body[o..o + n].to_vec(),
        });
        o += n;
    }

    let groups = info
        .groups
        .iter()
        .map(|g| Group {
            material: g[0],
            tri_start: g[1],
            tri_count: g[2],
        })
        .collect();

    Some(Scenery {
        mesh: MapMesh {
            positions,
            normals,
            uvs,
            indices,
            groups,
            // Pieces are recomputed on demand rather than cached: the cache holds what is
            // expensive to fetch, and this is a linear pass over an index buffer already in
            // hand.
            objects: Vec::new(),
            object_of_tri: Vec::new(),
            materials: info.materials,
        },
        info,
        textures,
    })
}

fn prune_cache(app: &tauri::AppHandle, dir_name: &str) {
    use tauri::Manager;
    let Ok(base) = app.path().app_cache_dir() else {
        return;
    };
    let Ok(rd) = std::fs::read_dir(base.join(dir_name)) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let modified = e.metadata().ok()?.modified().ok()?;
            p.is_file().then_some((modified, p))
        })
        .collect();
    if files.len() <= CACHE_KEEP {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    for (_, p) in files.iter().take(files.len() - CACHE_KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCR: &[u8] = br#"
obj0
{
	flag = 1
	name = usa_flag.edf
	paint =
	data = usa_flag.cfg
	pos = 271.41, 59.2, 149.31
}
obj1
{
 name = windturbine_rotor.edf
 rot_axis = 0, 0, 1
 pos = -412, 108.71, -381
 rot = 0, 90, 0
}
"#;

    const MARSHALS: &[u8] = br#"
flagman
{
	pos
	{
		x = 452.08
		y = 11.71
		z = 133.84
	}
	long = 0.28
}
marshal1
{
	pos
	{
		x = 456.47
		y = 8.35
		z = 220.48
	}
	long = 88.39
}
"#;

    const TSC: &[u8] = br#"
numcamset = 1
camset0
{
	name = TV_Cameras
	numcameras = 2
	camera0
	{
		type = 0
		pos = 423.7, 9.43, 233.75
		fov = 60
		rot = 35.3
	}
	camera1
	{
		pos = 318.54, 17.4, 282.13
		rot = 112.1
	}
}
"#;

    const SSC: &[u8] = br#"
numsources = 2
source0
{
	data = noisy_crowd2.wav
	pos = 355,10,430
	mindistance = 50
}
source1
{
	data = mellow_crowd.wav
	pos = 388,10,329
}
"#;

    #[test]
    fn reads_scr_props() {
        let objs = read_scr(SCR);
        assert_eq!(objs.len(), 2);
        assert_eq!(objs[0].0.name, "usa_flag.edf");
        assert_eq!(objs[0].0.pos, [271.41, 59.2, 149.31]);
        // Negative coordinates are ordinary: background props stand outside the terrain.
        assert_eq!(objs[1].0.pos, [-412.0, 108.71, -381.0]);
        assert_eq!(objs[1].1, [0.0, 90.0, 0.0]);
    }

    #[test]
    fn reads_marshal_posts() {
        let m = read_marshals(MARSHALS);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "flagman");
        // The heights that prove the frame: these are the figures the terrain grid
        // reproduces underneath each post.
        assert_eq!(m[0].pos, [452.08, 11.71, 133.84]);
        assert_eq!(m[0].heading, Some(0.28));
        assert_eq!(m[1].pos, [456.47, 8.35, 220.48]);
    }

    #[test]
    fn reads_cameras_across_a_set() {
        let c = read_cameras(TSC);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "TV_Cameras · camera0");
        assert_eq!(c[0].pos, [423.7, 9.43, 233.75]);
        assert_eq!(c[0].heading, Some(35.3));
        assert_eq!(c[1].pos, [318.54, 17.4, 282.13]);
    }

    #[test]
    fn reads_sound_sources() {
        let s = read_sounds(SSC);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "noisy_crowd2.wav");
        assert_eq!(s[0].pos, [355.0, 10.0, 430.0]);
        assert_eq!(s[1].heading, None);
    }

    /// The oracle for the whole feature: put a real track's marshal posts on its own terrain
    /// and see whether they land on the ground.
    ///
    /// ```text
    /// FROST_TRACK="…/Millville.pkz" \
    ///   cargo test --bin mxb-app -- --ignored --nocapture placements_stand_on_the_terrain
    /// ```
    ///
    /// `marshals.cfg` states each post's height, and the terrain grid states the ground's.
    /// Nothing makes those agree except reading both correctly, so this is what proves the
    /// world frame is shared — and it is the same frame the scenery mesh arrives in.
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK"]
    fn placements_stand_on_the_terrain() {
        let path = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track .pkz/folder");
        let p = Path::new(&path);

        let names = crate::track::entry_names(p).expect("read the track");
        let trh = names
            .iter()
            .find(|n| n.to_ascii_lowercase().ends_with(".trh"))
            .expect("the track needs a heightfield");
        let bytes = crate::track::read_entry(p, trh).expect("read the heightfield");
        let layout = crate::heightfield::probe(&bytes, None).expect("the heightfield must read");
        let (gw, gh, grid) = crate::heightfield::read_grid(&bytes, &layout, 4096);

        // Metres of ground per sample, so world metres can be turned into grid indices.
        let mps = layout
            .metres_per_sample
            .expect("the track must state its footprint");
        let size_x = mps * (layout.width.max(2) - 1) as f32;
        let size_z = mps * (layout.height.max(2) - 1) as f32;

        let marshals: Vec<Placement> = read_placements(&path)
            .expect("read placements")
            .into_iter()
            .filter(|p| p.kind == "marshal")
            .collect();
        assert!(!marshals.is_empty(), "this track states no marshal posts");

        let mut worst: f32 = 0.0;
        for m in &marshals {
            let gx = ((m.pos[0] / size_x) * (gw - 1) as f32).round() as i64;
            let gz = ((m.pos[2] / size_z) * (gh - 1) as f32).round() as i64;
            let gx = gx.clamp(0, gw as i64 - 1) as usize;
            let gz = gz.clamp(0, gh as i64 - 1) as usize;
            let ground = grid[gz * gw as usize + gx];
            let delta = m.pos[1] - ground;
            println!(
                "  {:10} stated y={:7.2}  terrain={:7.2}  Δ{:+.2}",
                m.name, m.pos[1], ground, delta
            );
            worst = worst.max(delta.abs());
        }
        // A post stands on the ground, so the only slack here is the grid's own sampling.
        assert!(
            worst < 2.0,
            "marshal posts are {worst:.2} m off the terrain — the frames disagree"
        );
    }

    /// Walk a folder of tracks and report what each one gives up, and how fast:
    ///
    /// ```text
    /// FROST_TRACKS="…/mods/tracks" \
    ///   cargo test --bin mxb-app -- --ignored --nocapture survey_every_track
    /// ```
    ///
    /// The two timings are the point of the split: `mesh` is what the viewer waits for
    /// before it can draw anything, `surfaces` is what it no longer waits for.
    #[test]
    #[ignore = "needs a folder of tracks — set FROST_TRACKS"]
    fn survey_every_track() {
        let root = std::env::var("FROST_TRACKS").expect("set FROST_TRACKS");
        let mut found: Vec<PathBuf> = Vec::new();
        for depth in crate::linkwalk::walk_depth(Path::new(&root), 3)
            .into_iter()
            .flatten()
        {
            let p = depth.path().to_path_buf();
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz")) {
                found.push(p);
            }
        }
        found.sort();
        println!(
            "{:<38} {:>7} {:>9} {:>8} {:>8} {:>9} {:>10}",
            "track", "mats", "tris", "props", "surf", "mesh ms", "surf ms"
        );
        let (mut ok, mut painted) = (0, 0);
        for p in &found {
            let name: String = p
                .file_stem()
                .map(|s| s.to_string_lossy().chars().take(36).collect())
                .unwrap_or_default();
            let key = p.to_string_lossy().into_owned();
            let t0 = std::time::Instant::now();
            let mesh = decode_with_key(p, false, Some(&key));
            let mesh_ms = t0.elapsed().as_millis();
            let Ok(m) = mesh else {
                println!("{name:<38} {:>7}", "—");
                continue;
            };
            ok += 1;
            let t1 = std::time::Instant::now();
            let surf = decode_with_key(p, true, Some(&key))
                .map(|s| s.textures.len())
                .unwrap_or(0);
            let surf_ms = t1.elapsed().as_millis();
            if surf > 0 {
                painted += 1;
            }
            println!(
                "{name:<38} {:>7} {:>9} {:>8} {:>8} {:>9} {:>10}",
                m.info.materials, m.info.triangle_count, m.info.props, surf, mesh_ms, surf_ms
            );
        }
        println!(
            "\n{ok} of {} tracks decoded, {painted} with surfaces bound",
            found.len()
        );
    }

    /// A stable colour per piece. Scattered around the wheel by an odd multiplier so that
    /// pieces numbered next to each other — which are usually next to each other in space —
    /// come out in unrelated hues rather than a gradient.
    fn piece_hue(id: u32) -> [f32; 3] {
        let h = (id.wrapping_mul(2_654_435_761) % 3600) as f32 / 3600.0 * 6.0;
        let (s, v) = (0.62, 0.95);
        let i = h.floor() as i32 % 6;
        let f = h - h.floor();
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
        match i {
            0 => [v, t, p],
            1 => [q, v, p],
            2 => [p, v, t],
            3 => [p, q, v],
            4 => [t, p, v],
            _ => [v, p, q],
        }
    }

    /// Draw a real track's scenery to a PNG, so a decode can be looked at rather than
    /// argued about:
    ///
    /// ```text
    /// FROST_TRACK="…/Farm14.pkz" FROST_PNG=/tmp/farm14.png \
    ///   cargo test --bin mxb-app -- --ignored --nocapture render_a_real_track
    /// ```
    ///
    /// `FROST_COLOR=pieces` gives every separable piece its own hue, which is the only way to
    /// see whether the mesh really came apart into the things it is made of. `FROST_CROP=x,z,r`
    /// frames a square of the track in metres instead of the whole site.
    ///
    /// Deliberately in-process: a sealed track's contents are never written anywhere except
    /// as pixels.
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK and FROST_PNG"]
    fn render_a_real_track() {
        let path = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let out = std::env::var("FROST_PNG").unwrap_or_else(|_| "/tmp/track.png".into());
        let s = decode(Path::new(&path), true).expect("decode the scenery");
        let (lo, hi) = s.mesh.bounds();
        println!(
            "{} verts, {} tris, bbox x[{:.0},{:.0}] y[{:.0},{:.0}] z[{:.0},{:.0}]",
            s.info.vertex_count, s.info.triangle_count, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );

        const W: usize = 1200;
        const H: usize = 1000;
        let by_piece = std::env::var("FROST_COLOR").as_deref() == Ok("pieces");
        let crop: Option<(f32, f32, f32)> = std::env::var("FROST_CROP").ok().and_then(|v| {
            let n: Vec<f32> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            (n.len() == 3).then(|| (n[0], n[1], n[2]))
        });
        // Frame the whole thing, seen from above and to one side — the viewer's own angle.
        let (mid, span) = match crop {
            Some((cx, cz, r)) => ([cx, (lo[1] + hi[1]) / 2.0, cz], r * 2.0),
            None => (
                [
                    (lo[0] + hi[0]) / 2.0,
                    (lo[1] + hi[1]) / 2.0,
                    (lo[2] + hi[2]) / 2.0,
                ],
                (hi[0] - lo[0]).max(hi[2] - lo[2]).max(1.0),
            ),
        };
        let eye = [
            mid[0] - span * 0.55,
            mid[1] + span * 0.75,
            mid[2] + span * 1.05,
        ];
        let norm = |v: [f32; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let fwd = norm([mid[0] - eye[0], mid[1] - eye[1], mid[2] - eye[2]]);
        let right = norm(cross(fwd, [0.0, 1.0, 0.0]));
        let up = cross(right, fwd);
        let focal = 1.0 / (45.0f32.to_radians() / 2.0).tan();

        let project = |p: [f32; 3]| -> [f32; 3] {
            let rel = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
            let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            let z = dot(rel, fwd);
            if z <= 0.01 {
                return [f32::NAN, f32::NAN, z];
            }
            [
                dot(rel, right) / z * focal * (W as f32 / 2.0) + W as f32 / 2.0,
                -dot(rel, up) / z * focal * (H as f32 / 2.0) + H as f32 / 2.0,
                z,
            ]
        };

        let light = norm([0.5, 0.8, 0.35]);
        let mut depth = vec![f32::INFINITY; W * H];
        let mut pixels = vec![18u8; W * H * 3];

        let vert = |i: u32| {
            let o = i as usize * 3;
            [
                s.mesh.positions[o],
                s.mesh.positions[o + 1],
                s.mesh.positions[o + 2],
            ]
        };
        for (ti, t) in s.mesh.indices.chunks_exact(3).enumerate() {
            let p = [vert(t[0]), vert(t[1]), vert(t[2])];
            if let Some((cx, cz, r)) = crop {
                let c = [
                    (p[0][0] + p[1][0] + p[2][0]) / 3.0,
                    0.0,
                    (p[0][2] + p[1][2] + p[2][2]) / 3.0,
                ];
                if (c[0] - cx).abs() > r || (c[2] - cz).abs() > r {
                    continue;
                }
            }
            let sp = [project(p[0]), project(p[1]), project(p[2])];
            if sp.iter().any(|v| !v[0].is_finite()) {
                continue;
            }
            // Face normal, so the picture doesn't depend on the stored normals being read right.
            let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
            let n = norm(cross(e1, e2));
            let shade = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2])
                .abs()
                .clamp(0.0, 1.0)
                * 0.75
                + 0.25;

            let (minx, maxx) = (
                sp.iter().fold(f32::MAX, |a, v| a.min(v[0])).max(0.0) as usize,
                (sp.iter()
                    .fold(f32::MIN, |a, v| a.max(v[0]))
                    .min(W as f32 - 1.0))
                .max(0.0) as usize,
            );
            let (miny, maxy) = (
                sp.iter().fold(f32::MAX, |a, v| a.min(v[1])).max(0.0) as usize,
                (sp.iter()
                    .fold(f32::MIN, |a, v| a.max(v[1]))
                    .min(H as f32 - 1.0))
                .max(0.0) as usize,
            );
            if maxx < minx || maxy < miny {
                continue;
            }
            let area = (sp[1][0] - sp[0][0]) * (sp[2][1] - sp[0][1])
                - (sp[2][0] - sp[0][0]) * (sp[1][1] - sp[0][1]);
            if area.abs() < 1e-9 {
                continue;
            }
            for y in miny..=maxy {
                for x in minx..=maxx {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((sp[1][0] - px) * (sp[2][1] - py)
                        - (sp[2][0] - px) * (sp[1][1] - py))
                        / area;
                    let w1 = ((sp[2][0] - px) * (sp[0][1] - py)
                        - (sp[0][0] - px) * (sp[2][1] - py))
                        / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * sp[0][2] + w1 * sp[1][2] + w2 * sp[2][2];
                    let at = y * W + x;
                    if z >= depth[at] {
                        continue;
                    }
                    depth[at] = z;
                    // Height above the mesh's floor, so relief reads as well as facing.
                    let base = if by_piece {
                        piece_hue(s.mesh.object_of_tri.get(ti).copied().unwrap_or(0))
                    } else {
                        let h = (w0 * p[0][1] + w1 * p[1][1] + w2 * p[2][1] - lo[1])
                            / (hi[1] - lo[1]).max(1.0);
                        [0.52 + 0.35 * h, 0.45 + 0.32 * h, 0.34 + 0.28 * h]
                    };
                    for c in 0..3 {
                        pixels[at * 3 + c] = ((base[c] * shade).clamp(0.0, 1.0) * 255.0) as u8;
                    }
                }
            }
        }

        image::save_buffer(&out, &pixels, W as u32, H as u32, image::ColorType::Rgb8)
            .expect("write the png");
        println!("wrote {out}");
    }

    /// The whole decode against a real track — the `.map` mesh with the `.scr` props folded in.
    ///
    /// ```text
    /// FROST_TRACK="…/Millville" \
    ///   cargo test --bin mxb-app -- --ignored --nocapture decode_a_real_track
    /// ```
    #[test]
    #[ignore = "needs a real track — set FROST_TRACK"]
    fn decode_a_real_track() {
        let path = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track .pkz/folder");
        let s = decode(Path::new(&path), true).expect("decode the scenery");
        let (lo, hi) = s.mesh.bounds();
        println!("  from {}", s.info.entry);
        println!(
            "  {} materials, {} verts, {} tris",
            s.info.materials, s.info.vertex_count, s.info.triangle_count
        );
        println!(
            "  {} props folded in, {} missing a mesh",
            s.info.props, s.info.props_missing
        );
        let packed = blob(&s);
        println!(
            "  {} draw groups, {} surfaces",
            s.mesh.groups.len(),
            s.textures.len()
        );
        let alpha = s.textures.iter().filter(|t| t.alpha).count();
        println!("  {alpha} of them alpha cut-outs");
        let obj = &s.mesh.objects;
        if !obj.is_empty() {
            let mut sizes: Vec<u32> = obj.iter().map(|o| o.tri_count).collect();
            sizes.sort_unstable();
            let biggest = obj.iter().max_by_key(|o| o.tri_count).unwrap();
            println!(
                "  {} separable pieces (median {} tris, largest {} tris, {:.1}x{:.1}x{:.1} m)",
                obj.len(),
                sizes[sizes.len() / 2],
                biggest.tri_count,
                biggest.max[0] - biggest.min[0],
                biggest.max[1] - biggest.min[1],
                biggest.max[2] - biggest.min[2],
            );
        }
        if s.textures.is_empty() && !s.info.entry.is_empty() {
            // Unbound: say what the map actually calls its sheets, which is the whole of why.
            if let Ok(bytes) = crate::track::read_entry(Path::new(&path), &s.info.entry) {
                let survey = crate::map::survey(&bytes);
                println!("  {} surface records, unbound:", survey.len());
                let primaries = crate::map::primaries(&bytes);
                println!(
                    "  {} of them primary (materials: {})",
                    primaries.len(),
                    s.info.materials
                );
                for (n, w, h) in survey.iter().take(12) {
                    println!("    {n:<34} {w}x{h}");
                }
            }
        }
        for t in s.textures.iter().take(60) {
            println!(
                "    mat{:<3} {:<34} {}x{} {}",
                t.material,
                t.name,
                t.width,
                t.height,
                if t.alpha { "ALPHA" } else { "" }
            );
        }
        let px: usize = s.textures.iter().map(|t| t.rgba.len()).sum();
        println!(
            "  surface pixels {:.1} MB, blob {:.1} MB",
            px as f64 / 1e6,
            packed.len() as f64 / 1e6
        );
        println!(
            "  x[{:.1}, {:.1}] y[{:.1}, {:.1}] z[{:.1}, {:.1}]",
            lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );
        // The blob is what the viewer actually gets, so check its size adds up rather than
        // trusting the counts alone.
        let pixels: usize = s.textures.iter().map(|t| t.rgba.len()).sum();
        assert_eq!(
            packed.len(),
            crate::map::SCENERY_HEADER
                + s.info.vertex_count as usize * 32
                + s.info.triangle_count as usize * 12
                + s.mesh.groups.len() * 12
                + s.textures.len() * crate::map::TEXTURE_ENTRY
                + pixels
        );
        assert!(s
            .mesh
            .indices
            .iter()
            .all(|i| (*i as usize) < s.mesh.vertex_count()));
    }

    #[test]
    fn numbered_blocks_come_back_in_order() {
        // Ten and above must not sort next to one — the format has no padding.
        let mut root = CfgNode::default();
        for i in [0u32, 1, 2, 9, 10, 11] {
            root.blocks.insert(format!("obj{i}"), CfgNode::default());
        }
        assert_eq!(numbered(&root, "obj").len(), 6);
        let mut root = CfgNode::default();
        for i in [11u32, 2, 0] {
            let mut n = CfgNode::default();
            n.values.insert("pos".into(), format!("{i}, 0, 0"));
            root.blocks.insert(format!("obj{i}"), n);
        }
        let got: Vec<f32> = numbered(&root, "obj")
            .iter()
            .filter_map(|n| n.get("pos").and_then(vec3_csv))
            .map(|p| p[0])
            .collect();
        assert_eq!(got, vec![0.0, 2.0, 11.0]);
    }

    #[test]
    fn the_tracks_own_file_comes_first() {
        let names = vec![
            "TPC-2/SX.scr".to_string(),
            "TPC-2/MX.scr".to_string(),
            "TPC-2/TPC-2.scr".to_string(),
        ];
        let picked = entries_with_ext(&names, "scr", "tpc-2");
        assert_eq!(picked[0], "TPC-2/TPC-2.scr");
    }

    #[test]
    fn a_prop_lands_where_the_scr_puts_it() {
        // One triangle at the origin, rotated a quarter turn and moved into the world.
        let mut mesh = MapMesh::default();
        let pos = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let nrm = vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];
        place(&mut mesh, &pos, &nrm, [0.0, 90.0, 0.0], [10.0, 5.0, -3.0]);
        // +X spun about Y by 90° goes to -Z, then the whole thing translates.
        assert!((mesh.positions[0] - 10.0).abs() < 1e-4);
        assert!((mesh.positions[2] - -4.0).abs() < 1e-4);
        // The second vertex sat on the origin, so it lands exactly on the stated position.
        assert!((mesh.positions[3] - 10.0).abs() < 1e-4);
        assert!((mesh.positions[4] - 5.0).abs() < 1e-4);
        assert!((mesh.positions[5] - -3.0).abs() < 1e-4);
        // Up stays up: a yaw doesn't tip a normal over.
        assert!((mesh.normals[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_malformed_placement_is_skipped_not_fatal() {
        let bad = br#"
obj0
{
	name = ok.edf
	pos = 1, 2, 3
}
obj1
{
	name = nopos.edf
}
obj2
{
	pos = 4, 5, 6
}
obj3
{
	name = short.edf
	pos = 7, 8
}
"#;
        let objs = read_scr(bad);
        assert_eq!(objs.len(), 1, "only the complete object survives");
        assert_eq!(objs[0].0.name, "ok.edf");
    }
}
