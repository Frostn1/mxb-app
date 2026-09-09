//! Lifting a published track's objects into a prop library our generator can place.
//!
//! A `.map` carries no instancing. `terrained.exe` bakes every `scene<N>` block into one
//! world-space mesh, so a tent that stood twenty times is twenty copies of its triangles and
//! nothing anywhere says they were ever the same model. Indiana's 665,313 triangles weld into
//! 176,317 islands this way, and its only surviving instance table is ten flags in a `.scr`.
//!
//! Recovering a library is three steps, and this module is each of them:
//!
//! 1. **Split.** [`map::parse`] union-finds the index buffer into islands already. An island
//!    is a fragment, not a model — the exporter cuts a banner run into one quad a metre — so
//!    islands are clustered into objects the way [`crate::trackobjects`] clusters them.
//! 2. **Fold.** Copies of one model differ by where they stand and which way they face, so
//!    the signature that finds them has to ignore both: a yaw-invariant profile of the shape.
//!    Yaw comes back afterwards, by fitting.
//! 3. **Record.** A placement in world metres is worthless on a different lap, so each copy
//!    is stored against the donor's own centreline — a fraction round the lap, a signed
//!    lateral offset, a yaw relative to the heading there, and a height above the ground.
//!    Those four replay onto a lap of a different length and shape.
//!
//! What this deliberately does *not* lift is the line-like classes — banners, hoardings, the
//! backdrop line, fences, edge stakes. Indiana cuts those into ~1 m panels that each follow
//! the curve, so they fold into thousands of near-identical props at a thousand yaws, and
//! replaying them as props would be a worse hoarding than the one
//! [`crate::trackscenery`] already sweeps. They are runs, and a run wants a rule and a
//! measurement, not a model. See [`LINE_LIKE`].

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::edfwrite::Mesh;
use crate::map;
use crate::trackobjects::{classify, Class};

/// How close two islands of the same class have to be to count as one object, metres.
///
/// The same 3 m [`crate::trackobjects`] clusters by, and for the same reason: a tree is
/// crossed cards that share no vertex, and it is one tree.
const LINK_M: f32 = 3.0;

/// Where a track's furniture ends and its backdrop begins, metres from the centreline.
///
/// Indiana puts 120 of its 133 trees beyond this and 13 inside it. The two populations are
/// different things and want different placement, so they are lifted apart.
const NEAR_M: f32 = 60.0;

/// Classes that are runs rather than models, and are measured instead of lifted.
///
/// A banner here is a metre of printed board out of a hoarding that covers 97.7% of the lap.
/// Lifted as props they would be ~9,000 models; measured they are four numbers.
const LINE_LIKE: [Class; 3] = [Class::Banner, Class::Fence, Class::Crowd];

/// Quantum for the shape signature, metres. Coarse enough that two copies of one model agree
/// through the exporter's float noise, fine enough that a bale and a tent never collide.
const SIG_Q: f32 = 0.05;

/// A distinct model, recovered from however many copies of it were baked into the map.
#[derive(Clone, Debug)]
pub struct Prop {
    /// Stable across runs: the sheet, the class and a hash of the shape.
    pub id: String,
    /// The sheet it is painted with, lower-cased, as the `.map` names it.
    pub sheet: String,
    pub class: Class,
    /// Local frame: X and Z centred on the footprint, Y zero at the foot. This is the frame a
    /// `scene<N>` block's `pos` places, so it has to be the frame the `.edf` is written in.
    pub mesh: Mesh,
    /// How tall it stands, metres.
    pub height: f32,
    /// Longest footprint dimension, metres.
    pub span: f32,
    /// The principal axis the first copy stood at, radians. Every later copy's yaw is measured
    /// against this, so the prop's own mesh is the zero and nothing depends on world north.
    pub axis_ref: f32,
}

/// Where one copy stood, in a frame that survives being replayed onto a different lap.
#[derive(Clone, Copy, Debug)]
pub struct Instance {
    /// Index into [`PropLibrary::props`].
    pub prop: usize,
    /// How far round the donor lap it stood, as a fraction of the lap. Replays by fraction
    /// rather than by metres, because our lap is not the donor's length.
    pub along: f32,
    /// Lateral offset from the centreline, metres. Signed: positive is right of travel, which
    /// is the sense `trackprog::right_vector` uses everywhere else in the pipeline.
    pub offset: f32,
    /// Which way it faced, radians, relative to the lap heading at that station. Relative, so
    /// a tent beside a corner still faces the track once the corner is somewhere else.
    pub yaw: f32,
    /// Height of its foot above the donor's terrain, metres. Almost always ~0; a gantry beam
    /// and a hanging banner are why it is carried rather than assumed.
    pub lift: f32,
    /// Whether it stood inside [`NEAR_M`] — trackside furniture rather than backdrop.
    pub near: bool,
}

/// What one class amounts to as a run, for the classes that are swept rather than placed.
#[derive(Clone, Copy, Debug)]
pub struct RunStats {
    pub class: Class,
    /// Pieces measured.
    pub pieces: usize,
    /// Median distance from the centreline, metres.
    pub offset_m: f32,
    /// Median height, metres.
    pub height_m: f32,
    /// Median gap between one piece and the next along the lap, metres.
    pub gap_m: f32,
    /// Fraction of the lap that has one of these within [`NEAR_M`].
    pub covered: f32,
}

/// A donor track's objects, ready to replay.
pub struct PropLibrary {
    pub donor: String,
    /// The donor's lap length, metres. Instances are fractions of it.
    pub donor_lap_m: f32,
    pub props: Vec<Prop>,
    pub instances: Vec<Instance>,
    /// The line-like classes, measured rather than lifted.
    pub runs: Vec<RunStats>,
    /// Sheet name against the RGBA the props wear, at the donor's own resolution.
    pub sheets: Vec<(String, u32, u32, Vec<u8>)>,
}

impl PropLibrary {
    /// Props that were actually used by an instance, and the instances that used them.
    pub fn tally(&self) -> Vec<(String, usize)> {
        let mut n: HashMap<usize, usize> = HashMap::new();
        for i in &self.instances {
            *n.entry(i.prop).or_default() += 1;
        }
        let mut out: Vec<(String, usize)> = n
            .into_iter()
            .map(|(p, c)| (self.props[p].id.clone(), c))
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1));
        out
    }
}

/// A donor track, opened far enough to measure: its lap and its scenery.
pub struct Donor {
    pub stem: String,
    pub lap: crate::trackline::Lap,
    pub mesh: map::MapMesh,
    /// Material index against the sheet it binds, as [`map::declared`] resolves it.
    pub sheets: Vec<(String, u32, u32)>,
    /// `declared` where the sheet table bound properly, `fallback` where it refused.
    pub binding: &'static str,
    /// The `.map` bytes, kept so the sheets can be inflated on demand.
    pub map_bytes: Vec<u8>,
}

/// Open a track's lap and scenery together.
///
/// Split out of [`crate::trackobjects::survey`], which did exactly this before anything else
/// needed it, so the two cannot drift apart on which `.map` they read or how they bind it.
pub fn open(path: &Path) -> Result<Donor> {
    let names = crate::track::entry_names(path)?;
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();

    // The lap, from the height file's trailing block.
    let hf = crate::track::heightfield_entries(&names)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no heightfield in this track"))?;
    let hb = crate::track::read_entry(path, &hf)?;
    let layout = crate::heightfield::probe(&hb, None)
        .ok_or_else(|| anyhow!("{hf} doesn't read as a terrain grid"))?;
    let block_at =
        layout.offset + layout.width as usize * layout.height as usize * layout.sample.size();
    let lap = crate::trackline::read(hb.get(block_at..).unwrap_or(&[]))
        .ok_or_else(|| anyhow!("{hf} carries no centreline"))?;

    // The scenery. A track's own `.map` wins over any other it happens to carry.
    let mut chosen: Option<(map::MapMesh, Vec<(String, u32, u32)>, &'static str, Vec<u8>)> = None;
    for name in names.iter().filter(|n| {
        Path::new(n)
            .extension()
            .map(|e| e.eq_ignore_ascii_case("map"))
            .unwrap_or(false)
    }) {
        let Ok(bytes) = crate::track::read_entry(path, name) else {
            continue;
        };
        if !map::is_map(&bytes) {
            continue;
        }
        let Some(m) = map::parse(&bytes) else { continue };
        let named = Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase() == stem)
            .unwrap_or(false);
        if chosen.is_none() || named {
            let declared = map::declared(&bytes);
            let binding = if declared.is_empty() { "fallback" } else { "declared" };
            let sheets = if declared.is_empty() {
                map::primaries(&bytes)
            } else {
                declared
            };
            chosen = Some((m, sheets, binding, bytes));
        }
        if named {
            break;
        }
    }
    let (mesh, sheets, binding, map_bytes) =
        chosen.ok_or_else(|| anyhow!("no readable .map in {path:?}"))?;

    Ok(Donor {
        stem,
        lap,
        mesh,
        sheets,
        binding,
        map_bytes,
    })
}

/// One object: the islands that make it up, and the box they stand in.
struct Object {
    class: Class,
    sheet: String,
    islands: Vec<usize>,
    min: [f32; 3],
    max: [f32; 3],
}

/// Cluster a map's islands into objects, the way [`crate::trackobjects`] does.
fn cluster(mesh: &map::MapMesh, sheet_of: &dyn Fn(u32) -> String) -> Vec<Object> {
    let n = mesh.objects.len();
    let mut uf: Vec<usize> = (0..n).collect();
    fn find(uf: &mut Vec<usize>, mut i: usize) -> usize {
        while uf[i] != i {
            uf[i] = uf[uf[i]];
            i = uf[i];
        }
        i
    }

    let class: Vec<Class> = mesh
        .objects
        .iter()
        .map(|o| classify(&sheet_of(o.material)))
        .collect();

    // Grid-hash so this stays linear in the island count rather than quadratic.
    let cell = LINK_M;
    let mut grid: HashMap<(i32, i32, u8), Vec<usize>> = HashMap::new();
    for (i, o) in mesh.objects.iter().enumerate() {
        if !class[i].is_object() {
            continue;
        }
        let cx = (o.min[0] + o.max[0]) * 0.5;
        let cz = (o.min[2] + o.max[2]) * 0.5;
        grid.entry((
            (cx / cell).floor() as i32,
            (cz / cell).floor() as i32,
            class[i] as u8,
        ))
        .or_default()
        .push(i);
    }
    for (&(gx, gz, k), here) in &grid {
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(there) = grid.get(&(gx + dx, gz + dz, k)) else {
                    continue;
                };
                for &a in here {
                    for &b in there {
                        if a >= b {
                            continue;
                        }
                        // Centres within LINK_M, exactly as `trackobjects` links them.
                        //
                        // Not box proximity, which is what this did first: a map carries
                        // ground decals hundreds of metres across, and a box that big is
                        // within LINK_M of everything, so a single chain swallowed half the
                        // track and 2,404 objects came out as 438.
                        let (oa, ob) = (&mesh.objects[a], &mesh.objects[b]);
                        let dx = (oa.min[0] + oa.max[0]) * 0.5 - (ob.min[0] + ob.max[0]) * 0.5;
                        let dz = (oa.min[2] + oa.max[2]) * 0.5 - (ob.min[2] + ob.max[2]) * 0.5;
                        if dx * dx + dz * dz <= LINK_M * LINK_M {
                            let (ra, rb) = (find(&mut uf, a), find(&mut uf, b));
                            if ra != rb {
                                uf[rb] = ra;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut by_root: HashMap<usize, Object> = HashMap::new();
    for i in 0..n {
        if !class[i].is_object() {
            continue;
        }
        let r = find(&mut uf, i);
        let o = &mesh.objects[i];
        let e = by_root.entry(r).or_insert_with(|| Object {
            class: class[i],
            sheet: sheet_of(o.material),
            islands: Vec::new(),
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        });
        e.islands.push(i);
        for k in 0..3 {
            e.min[k] = e.min[k].min(o.min[k]);
            e.max[k] = e.max[k].max(o.max[k]);
        }
    }
    by_root.into_values().collect()
}

/// Pull one object's triangles out of the map mesh, in a local frame.
///
/// X and Z are centred on the footprint and Y is zeroed at the foot, because that is the
/// frame a `scene<N>` block's `pos` places a model in. The returned angle is the object's
/// principal axis in the donor's world, which is what yaw is recovered against.
fn lift(mesh: &map::MapMesh, obj: &Object) -> (Mesh, [f32; 3], f32) {
    let cx = (obj.min[0] + obj.max[0]) * 0.5;
    let cz = (obj.min[2] + obj.max[2]) * 0.5;
    let foot = obj.min[1];

    let mut out = Mesh::default();
    let mut remap: HashMap<u32, u32> = HashMap::new();
    for &isl in &obj.islands {
        let o = &mesh.objects[isl];
        for t in o.tri_start..o.tri_start + o.tri_count {
            for k in 0..3 {
                let vi = mesh.indices[t as usize * 3 + k];
                let next = remap.len() as u32;
                let slot = *remap.entry(vi).or_insert(next);
                if slot == next {
                    let v = vi as usize;
                    out.positions.push(mesh.positions[v * 3] - cx);
                    out.positions.push(mesh.positions[v * 3 + 1] - foot);
                    out.positions.push(mesh.positions[v * 3 + 2] - cz);
                    out.uvs.push(mesh.uvs[v * 2]);
                    out.uvs.push(mesh.uvs[v * 2 + 1]);
                    out.normals.push(mesh.normals[v * 3]);
                    out.normals.push(mesh.normals[v * 3 + 1]);
                    out.normals.push(mesh.normals[v * 3 + 2]);
                }
                out.indices.push(slot);
            }
        }
    }
    let axis = principal_axis(&out);
    (out, [cx, foot, cz], axis)
}

/// The angle of an object's long axis in the XZ plane, radians.
///
/// The eigenvector of the 2x2 XZ covariance, in closed form. Ambiguous by pi — a tent's long
/// axis does not say which end is the door — which is why [`fold`] resolves the flip by
/// trying both and keeping the better fit rather than trusting this alone.
fn principal_axis(m: &Mesh) -> f32 {
    let n = m.vertex_count();
    if n < 2 {
        return 0.0;
    }
    let (mut mx, mut mz) = (0.0f64, 0.0f64);
    for v in m.positions.chunks_exact(3) {
        mx += v[0] as f64;
        mz += v[2] as f64;
    }
    mx /= n as f64;
    mz /= n as f64;
    let (mut sxx, mut szz, mut sxz) = (0.0f64, 0.0f64, 0.0f64);
    for v in m.positions.chunks_exact(3) {
        let (dx, dz) = (v[0] as f64 - mx, v[2] as f64 - mz);
        sxx += dx * dx;
        szz += dz * dz;
        sxz += dx * dz;
    }
    // A shape with no elongation has no meaningful axis; call it zero rather than noise.
    if (sxx - szz).abs() < 1e-9 && sxz.abs() < 1e-9 {
        return 0.0;
    }
    // Negated, because the pipeline's angles are compass-style — `heading_vector` is
    // (sin, cos) and `edfwrite::turned` spins the other way to the atan2 this formula uses.
    // An axis measured in one convention and applied in the other turns every prop the wrong
    // way, and a symmetric test prop will not show it.
    -(0.5 * (2.0 * sxz).atan2(sxx - szz)) as f32
}

/// A yaw-invariant fingerprint of a shape.
///
/// Rotation about Y is exactly what separates two copies of one model, so the signature is
/// built from quantities a yaw cannot change: how many triangles it has, how tall it is, and
/// the sorted profile of every vertex's distance from the centre against its height. Two
/// copies of a tent agree; a tent and a trailer do not.
fn signature(m: &Mesh, sheet: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let n = m.vertex_count();
    let (mut mx, mut mz) = (0.0f32, 0.0f32);
    for v in m.positions.chunks_exact(3) {
        mx += v[0];
        mz += v[2];
    }
    if n > 0 {
        mx /= n as f32;
        mz /= n as f32;
    }
    let mut profile: Vec<(i32, i32)> = m
        .positions
        .chunks_exact(3)
        .map(|v| {
            let r = ((v[0] - mx).powi(2) + (v[2] - mz).powi(2)).sqrt();
            ((r / SIG_Q).round() as i32, (v[1] / SIG_Q).round() as i32)
        })
        .collect();
    profile.sort_unstable();

    let mut h = DefaultHasher::new();
    sheet.hash(&mut h);
    m.triangle_count().hash(&mut h);
    profile.hash(&mut h);
    h.finish()
}

/// How well one shape matches another once turned by `dyaw`, as a mean vertex distance.
///
/// Vertices are compared in sorted order rather than paired, which is enough to tell a good
/// yaw from a bad one on shapes that are not rotationally symmetric — and on ones that are,
/// any yaw is right by definition.
fn fit_error(a: &Mesh, b: &Mesh, dyaw: f32) -> f32 {
    let (s, c) = dyaw.sin_cos();
    let key = |p: &[f32]| -> (i32, i32, i32) {
        (
            (p[0] / SIG_Q).round() as i32,
            (p[1] / SIG_Q).round() as i32,
            (p[2] / SIG_Q).round() as i32,
        )
    };
    let mut av: Vec<(i32, i32, i32)> = a
        .positions
        .chunks_exact(3)
        .map(|p| key(&[p[0] * c - p[2] * s, p[1], p[0] * s + p[2] * c]))
        .collect();
    let mut bv: Vec<(i32, i32, i32)> = b.positions.chunks_exact(3).map(key).collect();
    if av.len() != bv.len() || av.is_empty() {
        return f32::INFINITY;
    }
    av.sort_unstable();
    bv.sort_unstable();
    let mut acc = 0.0f64;
    for (p, q) in av.iter().zip(&bv) {
        let d = ((p.0 - q.0).pow(2) + (p.1 - q.1).pow(2) + (p.2 - q.2).pow(2)) as f64;
        acc += d.sqrt();
    }
    (acc / av.len() as f64) as f32 * SIG_Q
}

/// The nearest station on a lap to a world point.
///
/// Returns how far round the lap it sits, the signed lateral offset, and the heading there.
/// Signed the way the rest of the pipeline signs it: positive is right of travel.
fn nearest(stations: &[crate::trackline::Station], x: f32, z: f32) -> (f32, f32, f32) {
    let mut best = (0.0f32, f32::INFINITY, 0.0f32, 0.0f32);
    for p in stations {
        let (dx, dz) = (x - p.x, z - p.z);
        let d2 = dx * dx + dz * dz;
        if d2 < best.1 {
            let (rx, rz) = crate::trackprog::right_vector(p.heading);
            best = (p.at, d2, dx * rx + dz * rz, p.heading);
        }
    }
    (best.0, best.2, best.3)
}

/// Lift a donor track's discrete objects into a prop library, and measure its runs.
///
/// `keep` decides which classes are lifted as props; everything in [`LINE_LIKE`] is measured
/// as a run instead however `keep` is set, because a metre of hoarding is not a model.
pub fn extract(donor: &Donor, keep: &[Class]) -> PropLibrary {
    let sheets_tbl = donor.sheets.clone();
    let sheet_of = move |m: u32| -> String {
        sheets_tbl
            .get(m as usize)
            .map(|(n, ..)| n.to_ascii_lowercase())
            .unwrap_or_default()
    };
    let objects = cluster(&donor.mesh, &sheet_of);
    let stations = donor.lap.stations(1.0);
    let lap_m = donor.lap.length.max(1.0);

    // Runs first: the line-like classes, measured rather than lifted.
    let mut runs = Vec::new();
    for &class in &LINE_LIKE {
        let mut offs: Vec<f32> = Vec::new();
        let mut tall: Vec<f32> = Vec::new();
        let mut alongs: Vec<f32> = Vec::new();
        for o in objects.iter().filter(|o| o.class == class) {
            let cx = (o.min[0] + o.max[0]) * 0.5;
            let cz = (o.min[2] + o.max[2]) * 0.5;
            let (at, off, _) = nearest(&stations, cx, cz);
            if off.abs() > NEAR_M {
                continue;
            }
            offs.push(off.abs());
            tall.push(o.max[1] - o.min[1]);
            alongs.push(at);
        }
        if offs.is_empty() {
            continue;
        }
        alongs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let gaps: Vec<f32> = alongs.windows(2).map(|w| w[1] - w[0]).collect();
        // How much of the lap has one of these near it: metres of lap within 30 m of a piece.
        let mut hit = vec![false; (lap_m as usize / 10).max(1)];
        for a in &alongs {
            let b = (*a / 10.0) as usize;
            if b < hit.len() {
                hit[b] = true;
            }
        }
        let covered = hit.iter().filter(|h| **h).count() as f32 / hit.len() as f32;
        runs.push(RunStats {
            class,
            pieces: offs.len(),
            offset_m: median(&mut offs),
            height_m: median(&mut tall),
            gap_m: median(&mut gaps.clone()),
            covered,
        });
    }

    // Then the props: lift each object, fold the repeats, and record where each copy stood.
    let mut props: Vec<Prop> = Vec::new();
    let mut by_sig: HashMap<u64, usize> = HashMap::new();
    let mut instances: Vec<Instance> = Vec::new();

    for o in objects.iter().filter(|o| keep.contains(&o.class)) {
        let (mesh, at, axis) = lift(&donor.mesh, o);
        if mesh.triangle_count() == 0 {
            continue;
        }
        let sig = signature(&mesh, &o.sheet);

        // A new shape becomes a prop in its own frame; a repeat resolves to the one already
        // held, and the yaw between them is what gets recorded.
        let (idx, dyaw) = match by_sig.get(&sig) {
            Some(&i) => {
                let want = axis - props[i].axis_ref;
                // The principal axis is ambiguous by pi. Try both and keep the better fit.
                let a = fit_error(&mesh, &props[i].mesh, -want);
                let b = fit_error(&mesh, &props[i].mesh, -(want + std::f32::consts::PI));
                (i, if b < a { want + std::f32::consts::PI } else { want })
            }
            None => {
                let height = o.max[1] - o.min[1];
                let span = (o.max[0] - o.min[0]).max(o.max[2] - o.min[2]);
                props.push(Prop {
                    id: format!("{}_{:04x}", short(&o.sheet), sig & 0xffff),
                    sheet: o.sheet.clone(),
                    class: o.class,
                    mesh,
                    height,
                    span,
                    axis_ref: axis,
                });
                by_sig.insert(sig, props.len() - 1);
                (props.len() - 1, 0.0)
            }
        };

        let (along, offset, heading) = nearest(&stations, at[0], at[2]);
        instances.push(Instance {
            prop: idx,
            along: (along / lap_m).clamp(0.0, 1.0),
            offset,
            yaw: wrap_pi(dyaw - heading),
            lift: 0.0,
            near: offset.abs() <= NEAR_M,
        });
    }

    PropLibrary {
        donor: donor.stem.clone(),
        donor_lap_m: lap_m,
        props,
        instances,
        runs,
        sheets: Vec::new(),
    }
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

fn wrap_pi(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut x = a % tau;
    if x > std::f32::consts::PI {
        x -= tau;
    }
    if x < -std::f32::consts::PI {
        x += tau;
    }
    x
}

/// A sheet name cut down to something that reads in a filename.
fn short(sheet: &str) -> String {
    let s = sheet
        .trim_end_matches("_c_a")
        .trim_end_matches("_c")
        .trim_start_matches("ck_");
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Inflate just the sheets the library's props wear, at or below `max_dim`.
///
/// Two things worth stating, because both are silent when wrong.
///
/// **Orientation.** [`map::textures`] flips rows on the way out, because a viewer wants row 0
/// at the top. A lifted prop keeps the donor's own UVs, which address the sheet the way the
/// `.map` stored it, so the flip has to be undone or every wordmark ships upside-down — the
/// same trap [`crate::tracksynth`]'s generated sheets hit from the other side.
///
/// **Size.** Indiana's venue sheets are 72 MB inflated and seven of them are most of it:
/// `semi_trailers_c` is 4096² for two trailers. `max_dim` is the knob that makes a library
/// shippable, and it is applied here rather than later so the full-size RGBA never has to be
/// held for every sheet at once.
pub fn sheets_for(donor: &Donor, lib: &mut PropLibrary, max_dim: u32) {
    let want: std::collections::HashSet<String> =
        lib.props.iter().map(|p| p.sheet.to_ascii_lowercase()).collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in map::textures(&donor.map_bytes, max_dim) {
        let name = t.name.to_ascii_lowercase();
        if !want.contains(&name) || !seen.insert(name.clone()) {
            continue;
        }
        let mut rgba = t.rgba;
        flip_rows(&mut rgba, t.width, t.height);
        lib.sheets.push((name, t.width, t.height, rgba));
    }
}

/// Turn an RGBA image upside-down, in place.
fn flip_rows(rgba: &mut [u8], w: u32, h: u32) {
    let stride = w as usize * 4;
    if stride == 0 || rgba.len() < stride * h as usize {
        return;
    }
    for y in 0..(h as usize / 2) {
        let (a, b) = (y * stride, (h as usize - 1 - y) * stride);
        for i in 0..stride {
            rgba.swap(a + i, b + i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classes worth lifting as models: the venue, not the runs and not the ground.
    fn venue() -> Vec<Class> {
        vec![
            Class::Structure,
            Class::Vehicle,
            Class::Tree,
            Class::Bale,
            Class::Pole,
        ]
    }

    /// A yaw the fold cannot see is a yaw the replay cannot restore.
    ///
    /// Two copies of one shape at different angles must fold to one prop, and the angle
    /// between them must come back out. Built here rather than measured off a track, so the
    /// answer is known rather than plausible.
    #[test]
    fn a_turned_copy_folds_and_its_angle_comes_back() {
        let base = crate::edfwrite::cuboid(4.0, 2.0, 1.0);
        for deg in [0.0f32, 17.0, 90.0, 143.0] {
            let turned = crate::edfwrite::turned(&base, deg);
            assert_eq!(
                signature(&base, "tent_c"),
                signature(&turned, "tent_c"),
                "a copy turned {deg}° must sign the same as the one it was turned from"
            );
            // The angle between their principal axes is the yaw, up to the pi the axis
            // cannot resolve — which is the ambiguity `extract` settles by fitting.
            let want = deg.to_radians();
            let got = principal_axis(&turned) - principal_axis(&base);
            let err = wrap_pi(got - want).abs().min(wrap_pi(got - want + std::f32::consts::PI).abs());
            assert!(err < 0.02, "{deg}°: recovered {got}, wanted {want}, err {err}");
        }
    }

    /// A different shape must not fold into the same prop, or the library loses models.
    #[test]
    fn different_shapes_do_not_fold_together() {
        let a = crate::edfwrite::cuboid(4.0, 2.0, 1.0);
        let b = crate::edfwrite::cuboid(4.0, 3.0, 1.0);
        assert_ne!(signature(&a, "tent_c"), signature(&b, "tent_c"));
        // And the same shape on a different sheet is a different prop: it wears a different
        // picture, so it cannot share an `.edf`.
        assert_ne!(signature(&a, "tent_c"), signature(&a, "trailer_c"));
    }

    /// Lift Indiana and report what came out.
    ///
    /// ```text
    /// FROST_TRACK=~/Projects/pkz/tracks/2024_ARLMX_RD11_INDIANA_PRO.pkz \
    ///   cargo test -- --ignored --nocapture lift_a_donor
    /// ```
    #[test]
    #[ignore]
    fn lift_a_donor() {
        let track = std::env::var("FROST_TRACK").expect("set FROST_TRACK to a track archive");
        let path = std::path::PathBuf::from(shellexpand(&track));
        let donor = open(&path).expect("opens");
        eprintln!(
            "{}: lap {:.0} m, {} islands, binding {}",
            donor.stem,
            donor.lap.length,
            donor.mesh.objects.len(),
            donor.binding
        );

        let lib = extract(&donor, &venue());
        eprintln!(
            "\n{} props from {} instances ({} near)",
            lib.props.len(),
            lib.instances.len(),
            lib.instances.iter().filter(|i| i.near).count()
        );

        eprintln!("\nruns (measured, not lifted):");
        for r in &lib.runs {
            eprintln!(
                "  {:10} {:6} pieces  off {:5.1} m  tall {:4.2} m  gap {:4.1} m  {:.0}% of lap",
                r.class.key(),
                r.pieces,
                r.offset_m,
                r.height_m,
                r.gap_m,
                r.covered * 100.0
            );
        }

        // What the library weighs, which is the whole question about shipping it.
        let tris: usize = lib.props.iter().map(|p| p.mesh.triangle_count()).sum();
        let verts: usize = lib.props.iter().map(|p| p.mesh.vertex_count()).sum();
        eprintln!(
            "geometry: {tris} tris, {verts} verts -> {:.1} MB of .edf",
            (verts * 72 + tris * 12) as f32 / 1_048_576.0
        );

        eprintln!("\nprops by copies:");
        let mut by_class: HashMap<&str, (usize, usize)> = HashMap::new();
        for (id, n) in lib.tally().iter().take(30) {
            let p = lib.props.iter().find(|p| &p.id == id).unwrap();
            eprintln!(
                "  {:4} x {:28} {:9} {:5.1} m tall {:5.1} m span  {} tris",
                n,
                id,
                p.class.key(),
                p.height,
                p.span,
                p.mesh.triangle_count()
            );
        }
        for (i, p) in lib.props.iter().enumerate() {
            let n = lib.instances.iter().filter(|x| x.prop == i).count();
            let e = by_class.entry(p.class.key()).or_default();
            e.0 += 1;
            e.1 += n;
        }
        eprintln!("\nby class: props / instances");
        let mut ks: Vec<_> = by_class.into_iter().collect();
        ks.sort();
        for (k, (props, inst)) in ks {
            eprintln!("  {k:10} {props:4} props  {inst:5} instances");
        }

        assert!(!lib.props.is_empty(), "a published track has objects");
    }

    fn shellexpand(s: &str) -> String {
        match s.strip_prefix("~/") {
            Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
            None => s.to_string(),
        }
    }
}

#[cfg(test)]
mod folddiag {
    use super::*;

    /// Does anything actually repeat, and at which level?
    ///
    /// The first cut of this module folded whole clusters and got 438 shapes out of 450
    /// copies — no reuse at all, because a cluster is an assembly and assemblies vary. This
    /// asks the question at the island level instead, which is the unit the exporter emits.
    #[test]
    #[ignore]
    fn what_repeats() {
        let track = std::env::var("FROST_TRACK").expect("set FROST_TRACK");
        let path = std::path::PathBuf::from(track);
        let donor = open(&path).expect("opens");
        let tbl = donor.sheets.clone();
        let sheet_of = |m: u32| -> String {
            tbl.get(m as usize).map(|(n, ..)| n.to_ascii_lowercase()).unwrap_or_default()
        };

        let mut by_sig: HashMap<u64, (usize, Class, String, usize)> = HashMap::new();
        let mut considered = 0usize;
        for (i, o) in donor.mesh.objects.iter().enumerate() {
            let sheet = sheet_of(o.material);
            let class = classify(&sheet);
            if !class.is_object() || LINE_LIKE.contains(&class) {
                continue;
            }
            considered += 1;
            let one = Object {
                class,
                sheet: sheet.clone(),
                islands: vec![i],
                min: o.min,
                max: o.max,
            };
            let (mesh, _, _) = lift(&donor.mesh, &one);
            if mesh.triangle_count() == 0 {
                continue;
            }
            let e = by_sig
                .entry(signature(&mesh, &sheet))
                .or_insert((0, class, sheet, mesh.triangle_count()));
            e.0 += 1;
        }
        let mut v: Vec<_> = by_sig.into_values().collect();
        v.sort_by(|a, b| b.0.cmp(&a.0));
        let total: usize = v.iter().map(|x| x.0).sum();
        eprintln!(
            "\n{considered} islands considered -> {} distinct shapes, {total} copies",
            v.len()
        );
        eprintln!("reuse factor {:.1}x\n", total as f32 / v.len().max(1) as f32);
        for (n, class, sheet, tris) in v.iter().take(25) {
            eprintln!("  {n:6} x {:9} {:32} {tris} tris", class.key(), sheet);
        }
    }
}
