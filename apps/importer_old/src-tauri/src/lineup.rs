//! Line up: a model swap made for one bike, moved onto another.
//!
//! The game hangs each part of a bike's model off its `.geom`: the chassis as it is, the steer
//! about the steering head (turned by the rake), the fork below the steer, the swingarm at its
//! pivot. A swap built for bike A therefore sits wrong on bike B wherever their mounts differ.
//! This moves each part so the joints that move in the game land on B's: the steer on B's
//! steering axis, the fork on B's front axle, the swingarm on B's pivot and rear axle. The
//! chassis is rigid and can't follow two different mounts at once, so it is fitted between
//! the steering head and the pivot (or to one of them), and what's left over is reported.
//!
//! A part is moved by its root node's matrix in the `.edf` (64 bytes per part), so the file is
//! otherwise byte for byte the one it was, and every lever, shock and child node keeps its own
//! pivot. `gfx.cfg` points move with their part; the shock's `link` (a point in the physics
//! rocker frame) moves with the rocker.
//!
//! Bike B is only ever read for its `.geom` numbers. A locked `.pkz` is read in memory, and none
//! of its files is written anywhere.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn len(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
/// About x, as `mxb_core::edf` turns the steer by the rake.
fn rot_x(p: V3, deg: f64) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}
fn mm(a: V3) -> [f64; 3] {
    [round1(a[0] * 1000.0), round1(a[1] * 1000.0), round1(a[2] * 1000.0)]
}
fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

// ------------------------------------------------------------------------------------- mounts

/// The `.geom` points a model's parts hang off, in the game's frame (+x right, +y up, +z ahead).
#[derive(Debug, Clone)]
pub struct Mounts {
    pub head: V3,
    pub pivot: V3,
    pub steer_joint: V3,
    pub rsusp_joint: V3,
    pub front_upper: V3,
    pub fwheel: V3,
    /// Middle of the chain-adjuster slot (`rwheel_min`/`rwheel_max`).
    pub rwheel: V3,
    /// Half the slot's length.
    pub rwheel_slack: f64,
    /// `rakeangle_min`, degrees back from vertical.
    pub rake: f64,
    pub shock_chassis: Option<V3>,
    pub shock_rocker: Option<V3>,
    /// Eye to eye, fully extended: `shock_minlength + rear_length`.
    pub shock_length: Option<f64>,
}

impl Mounts {
    pub fn parse(geom: &[u8]) -> Result<Mounts> {
        let g = mxb_core::edf::parse_geom(geom);
        let sc = mxb_core::edf::parse_geom_scalars(geom);
        let v = |k: &str| g.get(k).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]);
        let need = |k: &str| v(k).with_context(|| format!("the .geom has no {k}"));
        let lo = need("rwheel_min")?;
        let hi = v("rwheel_max").unwrap_or(lo);
        let s = |k: &str| sc.get(k).map(|x| *x as f64);
        Ok(Mounts {
            head: need("chassis_steer")?,
            pivot: need("chassis_rsusp_min")?,
            steer_joint: need("steer_joint")?,
            rsusp_joint: need("rsusp_joint")?,
            front_upper: need("front_upper")?,
            fwheel: need("fwheel")?,
            rwheel: scale(add(lo, hi), 0.5),
            rwheel_slack: len(sub(hi, lo)) * 0.5,
            rake: s("rakeangle_min").unwrap_or(0.0),
            shock_chassis: v("shock_chassis"),
            shock_rocker: v("shock_rocker"),
            shock_length: s("shock_minlength").zip(s("rear_length")).map(|(a, b)| a + b),
        })
    }

    /// The front axle relative to the steering joint, in the steer's own frame: y runs up the
    /// steering axis, z ahead of it.
    fn axle_in_steer(&self) -> V3 {
        sub(add(self.fwheel, self.front_upper), self.steer_joint)
    }
    fn front_axle(&self) -> V3 {
        add(rot_x(self.axle_in_steer(), -self.rake), self.head)
    }
    fn rear_arm(&self) -> V3 {
        sub(self.rwheel, self.rsusp_joint)
    }
    fn rear_axle(&self) -> V3 {
        add(self.rear_arm(), self.pivot)
    }
}

// ------------------------------------------------------------------------------------- plan

/// Which mount the chassis follows when the two bikes' frames differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Anchor {
    /// Halfway between the steering head and the swingarm pivot.
    #[default]
    Fit,
    Pivot,
    Head,
}

impl Anchor {
    pub fn parse(s: &str) -> Option<Anchor> {
        match s.to_ascii_lowercase().as_str() {
            "fit" => Some(Anchor::Fit),
            "pivot" => Some(Anchor::Pivot),
            "head" => Some(Anchor::Head),
            _ => None,
        }
    }
}

/// A move in a part's own frame: turn about x by `deg`, then shift by `t` (metres).
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Xf {
    pub deg: f64,
    pub t: V3,
}

impl Xf {
    pub fn apply(&self, p: V3) -> V3 {
        add(rot_x(p, self.deg), self.t)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartMove {
    pub part: String,
    /// The shift, in millimetres.
    pub mm: [f64; 3],
    pub deg: f64,
}

/// One mount compared: where it is on each bike and how far apart, in millimetres.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub mount: String,
    pub from: String,
    pub to: String,
    /// How far apart, in `unit` (`mm`, or `°` for the rake).
    pub diff: f64,
    pub unit: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub anchor: Anchor,
    pub moves: Vec<PartMove>,
    pub mounts: Vec<Row>,
    /// What can't line up, in plain words.
    pub issues: Vec<String>,
    #[serde(skip)]
    pub parts: HashMap<&'static str, Xf>,
    #[serde(skip)]
    pub link: Option<V3>,
}

fn fmt_mm(a: V3) -> String {
    let m = mm(a);
    format!("{:.1}, {:.1}, {:.1}", m[0], m[1], m[2])
}

/// How each part moves to put a swap built on `a` onto `b`.
pub fn plan(a: &Mounts, b: &Mounts, anchor: Anchor) -> Plan {
    // Front: the fork goes where B's axle is. The steer keeps the fork in its clamps, so it
    // takes the fork's move across the steering axis (z) but not along it (y): along the axis
    // the steer stays on B's head and the fork slides, as it does in the game.
    let dax = sub(b.axle_in_steer(), a.axle_in_steer());
    let steer = Xf { deg: 0.0, t: add(sub(b.steer_joint, a.steer_joint), [0.0, 0.0, dax[2]]) };
    let fsusp = Xf { deg: 0.0, t: sub(b.fwheel, a.fwheel) };

    // Rear: the swingarm on B's pivot, turned so its axle points at B's.
    let ang = |v: V3| v[2].atan2(v[1]).to_degrees();
    let phi = ang(b.rear_arm()) - ang(a.rear_arm());
    let rsusp = Xf { deg: phi, t: sub(b.rsusp_joint, rot_x(a.rsusp_joint, phi)) };

    // Chassis: its head tube where the steer's stem now is, its pivot on B's.
    let stem = add(b.head, rot_x([0.0, 0.0, dax[2]], -b.rake));
    let dh = sub(stem, a.head);
    let dp = sub(b.pivot, a.pivot);
    let t = match anchor {
        Anchor::Fit => scale(add(dh, dp), 0.5),
        Anchor::Pivot => dp,
        Anchor::Head => dh,
    };
    let chassis = Xf { deg: 0.0, t };

    let link = a.shock_rocker.zip(b.shock_rocker).map(|(x, y)| sub(y, x));

    let row = |name: &str, x: V3, y: V3| Row {
        mount: name.into(),
        from: fmt_mm(x),
        to: fmt_mm(y),
        diff: round1(len(sub(y, x)) * 1000.0),
        unit: "mm",
    };
    let mut mounts = vec![
        row("Steering head", a.head, b.head),
        row("Swingarm pivot", a.pivot, b.pivot),
        row("Front axle", a.front_axle(), b.front_axle()),
        row("Rear axle", a.rear_axle(), b.rear_axle()),
    ];
    if let (Some(x), Some(y)) = (a.shock_chassis, b.shock_chassis) {
        mounts.push(row("Shock top", x, y));
    }
    if let (Some(x), Some(y)) = (a.shock_rocker, b.shock_rocker) {
        mounts.push(row("Shock bottom (rocker)", x, y));
    }
    mounts.push(Row {
        mount: "Rake".into(),
        from: format!("{:.1}°", a.rake),
        to: format!("{:.1}°", b.rake),
        diff: round1((b.rake - a.rake).abs()),
        unit: "°",
    });
    // Along the ground (z), as the mount spec measures it.
    let wb = |m: &Mounts| (m.front_axle()[2] - m.rear_axle()[2]).abs();
    mounts.push(Row {
        mount: "Wheelbase".into(),
        from: format!("{:.1}", wb(a) * 1000.0),
        to: format!("{:.1}", wb(b) * 1000.0),
        diff: round1((wb(b) - wb(a)).abs() * 1000.0),
        unit: "mm",
    });

    // What the model can't follow.
    let mut issues = Vec::new();
    let gap = |v: V3| round1(len(v) * 1000.0);
    let (head_gap, pivot_gap) = (gap(sub(dh, t)), gap(sub(dp, t)));
    let frame = round1((len(sub(b.head, b.pivot)) - len(sub(a.head, a.pivot))) * 1000.0);
    if head_gap >= 3.0 || pivot_gap >= 3.0 {
        issues.push(format!(
            "Frame: steering head to swingarm pivot is {:.1} mm {} on the new bike. The frame sits {head_gap:.1} mm off the steer and {pivot_gap:.1} mm off the swingarm.",
            frame.abs(),
            if frame < 0.0 { "shorter" } else { "longer" },
        ));
    }
    if dax[2].abs() >= 0.003 {
        issues.push(format!(
            "Fork offset: the front axle sits {:.1} mm further {} the steering axis. The bars and fork move with it, so the stem is that far off the head tube.",
            (dax[2] * 1000.0).abs(),
            if dax[2] > 0.0 { "ahead of" } else { "behind" },
        ));
    }
    if dax[1].abs() >= 0.003 {
        issues.push(format!(
            "Fork length: at rest the fork sits {:.1} mm {}.",
            (dax[1] * 1000.0).abs(),
            if dax[1] > 0.0 { "deeper (it bottoms out that much sooner)" } else { "longer" },
        ));
    }
    let drake = b.rake - a.rake;
    if drake.abs() >= 0.3 {
        issues.push(format!("Rake: the bars and fork lean {:.1}° {} than the frame's head tube.", drake.abs(), if drake > 0.0 { "more" } else { "less" }));
    }
    let darm = len(b.rear_arm()) - len(a.rear_arm());
    if darm.abs() >= 0.003 {
        let within = darm.abs() <= b.rwheel_slack;
        issues.push(format!(
            "Swingarm: the rear axle is {:.1} mm {}{}.",
            (darm * 1000.0).abs(),
            if darm > 0.0 { "further back" } else { "further forward" },
            if within { ", inside the chain adjuster slot" } else { ", past the chain adjuster slot" },
        ));
    }
    if let (Some(x), Some(y)) = (a.shock_chassis, b.shock_chassis) {
        let d = gap(sub(y, add(x, t)));
        if d >= 5.0 {
            issues.push(format!("Shock: the new bike's top shock mount is {d:.1} mm from the frame's."));
        }
    }
    if let (Some(x), Some(y)) = (a.shock_length, b.shock_length) {
        if y > x + 0.002 {
            issues.push(format!(
                "Shock length: the new bike's shock is {:.1} mm longer fully extended; check the shock reaches its linkage.",
                (y - x) * 1000.0
            ));
        }
    }

    let moves = [("chassis", chassis), ("steer", steer), ("fsusp", fsusp), ("rsusp", rsusp)]
        .iter()
        .map(|(p, x)| PartMove { part: p.to_string(), mm: mm(x.t), deg: (x.deg * 100.0).round() / 100.0 })
        .collect();
    let parts = HashMap::from([("chassis", chassis), ("steer", steer), ("fsusp", fsusp), ("rsusp", rsusp)]);
    Plan { anchor, moves, mounts, issues, parts, link }
}

// ------------------------------------------------------------------------------------- .edf

fn rd_u32(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).context("the .edf ends early")
}
fn rd_f32(b: &[u8], o: usize) -> f64 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as f64
}
fn wr_f32(b: &mut [u8], o: usize, v: f64) {
    b[o..o + 4].copy_from_slice(&(v as f32).to_le_bytes());
}

/// The objects of a plain `.edf`: the main one at 4, then the `EXT` table's.
fn edf_objects(b: &[u8]) -> Result<Vec<(String, usize)>> {
    if b.len() < 8 || &b[..4] != b"EDF\0" {
        bail!("not a plain .edf");
    }
    let mut out = vec![(String::new(), 4usize)];
    if &b[b.len() - 8..b.len() - 4] == b"EXT\0" {
        let size = rd_u32(b, b.len() - 4)? as usize;
        let e = b.len().checked_sub(size).context("bad EXT table")?;
        let n = rd_u32(b, e + 4)? as usize;
        for i in 0..n {
            let at = e + 8 + 104 * i;
            let raw = b.get(at..at + 100).context("bad EXT table")?;
            let name = String::from_utf8_lossy(raw.split(|c| *c == 0).next().unwrap_or(&[])).to_string();
            out.push((name, rd_u32(b, at + 100)? as usize));
        }
    }
    Ok(out)
}

/// Offsets of the root nodes' matrices in the object at `at` (the layout `frost_import.py`
/// reads: bounds, materials, vertex lanes, triangles, nodes).
fn root_matrices(b: &[u8], at: usize) -> Result<Vec<usize>> {
    let mut o = at + 24;
    o += 4 + 56 * rd_u32(b, o)? as usize;
    o += 4 + 72 * rd_u32(b, o)? as usize;
    o += 4 + 12 * rd_u32(b, o)? as usize;
    let nodes = rd_u32(b, o)? as usize;
    o += 4;
    let mut roots = Vec::new();
    for _ in 0..nodes {
        let flags = rd_u32(b, o + 100)?;
        let parent = rd_u32(b, o + 232)?;
        let children = rd_u32(b, o + 236)? as usize;
        let mut p = o + 240 + 4 * children;
        let groups = rd_u32(b, p)? as usize;
        let groups_at = p + 8;
        p += 8 + 24 * groups + 4;
        if flags & 0x10000 != 0 {
            let verts: usize = (0..groups).map(|g| rd_u32(b, groups_at + 24 * g + 16).unwrap_or(0) as usize).sum();
            let keys = rd_u32(b, p + 4)? as usize;
            p += 8 + 4 * (keys * verts * 10 + keys);
        }
        p += 24;
        if parent == 0 {
            roots.push(o + 104);
        }
        if p > b.len() {
            bail!("the .edf ends inside a node");
        }
        o = p;
    }
    Ok(roots)
}

/// The part an `.edf` object belongs to: the main object is the chassis, the others are named.
fn part_of(object: &str) -> Option<&'static str> {
    let n = object.to_ascii_lowercase();
    if n.is_empty() || n.starts_with("chassis") {
        Some("chassis")
    } else {
        ["steer", "fsusp", "rsusp"].into_iter().find(|p| n.starts_with(p))
    }
}

/// Move every part of a plain `.edf` in place. Returns, per object, the part it took.
pub fn move_edf(b: &mut [u8], parts: &HashMap<&'static str, Xf>) -> Result<Vec<(String, Option<&'static str>)>> {
    let mut done = Vec::new();
    for (name, at) in edf_objects(b)? {
        let part = part_of(&name);
        let Some(x) = part.and_then(|p| parts.get(p)) else {
            done.push((name, None));
            continue;
        };
        // Root matrices: row-major, translation in the 4th column. M' = X · M.
        for m in root_matrices(b, at)? {
            let el = |r: usize, c: usize| m + (r * 4 + c) * 4;
            for c in 0..4 {
                let col = [rd_f32(b, el(0, c)), rd_f32(b, el(1, c)), rd_f32(b, el(2, c))];
                let v = if c == 3 { x.apply(col) } else { rot_x(col, x.deg) };
                for r in 0..3 {
                    wr_f32(b, el(r, c), v[r]);
                }
            }
        }
        // The object's bounds follow (the main object's are the file's header).
        let lo = [rd_f32(b, at), rd_f32(b, at + 4), rd_f32(b, at + 8)];
        let hi = [rd_f32(b, at + 12), rd_f32(b, at + 16), rd_f32(b, at + 20)];
        if (0..3).all(|k| lo[k] <= hi[k]) {
            let (mut nlo, mut nhi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for i in 0..8 {
                let c = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
                let p = x.apply(c);
                for k in 0..3 {
                    nlo[k] = nlo[k].min(p[k]);
                    nhi[k] = nhi[k].max(p[k]);
                }
            }
            for k in 0..3 {
                wr_f32(b, at + 4 * k, nlo[k]);
                wr_f32(b, at + 12 + 4 * k, nhi[k]);
            }
        }
        done.push((name, part));
    }
    Ok(done)
}

// ------------------------------------------------------------------------------------- gfx.cfg

fn num(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

/// Rewrite a swap's `gfx.cfg` for the new bike, keeping its text: the grips move with the steer,
/// the exhaust with the chassis, the shock's `link` with the rocker. The chain block is left as
/// written. Returns the paths that changed.
pub fn move_gfx(text: &str, p: &Plan) -> (String, Vec<String>) {
    use mxb_core::gfxedit::Doc;
    let mut doc = Doc::parse(text);
    let mut changed = Vec::new();
    let mut shift = |doc: &mut Doc, path: &str, f: &dyn Fn(V3) -> V3| {
        let get = |k: &str| doc.get(&format!("{path}/{k}")).and_then(|v| v.trim().parse::<f64>().ok());
        let (Some(x), Some(y), Some(z)) = (get("x"), get("y"), get("z")) else { return };
        let v = f([x, y, z]);
        for (k, n) in ["x", "y", "z"].iter().zip(v) {
            let key = format!("{path}/{k}");
            let old = doc.get(&key).unwrap_or("").trim().to_string();
            if old.parse::<f64>().ok().map(num) != Some(num(n)) && doc.set(&key, &num(n)).is_ok() {
                changed.push(key);
            }
        }
    };
    let steer = p.parts["steer"];
    let chassis = p.parts["chassis"];
    for root in ["", "cockpit/"] {
        for grip in ["leftgrip", "rightgrip"] {
            shift(&mut doc, &format!("{root}steer/{grip}/pos"), &|v| steer.apply(v));
        }
        shift(&mut doc, &format!("{root}chassis/exhaust/pos"), &|v| chassis.apply(v));
        if let Some(d) = p.link {
            shift(&mut doc, &format!("{root}chassis/shock/link"), &|v| add(v, d));
        }
    }
    (doc.text().to_string(), changed)
}

// ------------------------------------------------------------------------------------- bikes

pub const LIB_DIR: &str = "FrostMod Models";
const MANIFEST: &str = "_files.txt";
const ACTIVE: &str = "_active.txt";

/// A bike's `.geom`: loose in its folder, else in its `.pkz` (read in memory).
pub fn bike_geom(bike: &Path) -> Result<(String, Vec<u8>)> {
    let is_geom = |n: &str| n.to_ascii_lowercase().ends_with(".geom");
    if bike.is_dir() {
        if let Ok(rd) = std::fs::read_dir(bike) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if is_geom(&n) {
                    return Ok((n, std::fs::read(e.path())?));
                }
            }
        }
        let pkz = bike.with_extension("pkz");
        let pkz = if pkz.is_file() { pkz } else { PathBuf::from(format!("{}.pkz", bike.display())) };
        if pkz.is_file() {
            return bike_geom(&pkz);
        }
        bail!("no .geom for {}", bike.display());
    }
    let files = mxb_core::pkz::read_selected(bike, is_geom).with_context(|| format!("can't open {}", bike.display()))?;
    let (n, b) = files.into_iter().next().with_context(|| format!("no .geom in {}", bike.display()))?;
    Ok((n.rsplit('/').next().unwrap_or(&n).to_string(), b))
}

/// The bike folder a path names: `<bikes>/X.pkz` or `<bikes>/X` → `<bikes>/X`.
pub fn bike_dir(p: &Path) -> PathBuf {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz")) {
        p.with_extension("")
    } else {
        p.to_path_buf()
    }
}

/// A swap's files: a variant folder's own, or a bike folder's active set (its manifest).
pub struct Swap {
    pub name: String,
    /// The bike it was built for.
    pub bike: PathBuf,
    pub files: Vec<PathBuf>,
}

fn is_model_file(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    l == "gfx.cfg" || [".edf", ".hrc", ".tga", ".dds", ".png", ".jpg", ".shd"].iter().any(|e| l.ends_with(e))
}

pub fn find_swap(dir: &Path) -> Result<Swap> {
    let list = |d: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(d)
            .map(|rd| rd.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        v.sort();
        v
    };
    let manifest = |d: &Path| -> Option<Vec<String>> {
        let t = std::fs::read_to_string(d.join(MANIFEST)).ok()?;
        let v: Vec<String> = t.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
        (!v.is_empty()).then_some(v)
    };
    let in_lib = dir.parent().and_then(|p| p.file_name()).is_some_and(|n| n.eq_ignore_ascii_case(LIB_DIR));
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if in_lib {
        let bike = dir.parent().and_then(Path::parent).context("no bike folder")?.to_path_buf();
        let active = std::fs::read_to_string(dir.parent().unwrap().join(ACTIVE)).unwrap_or_default();
        // The active set is loose at the bike root; its folder holds only the manifest.
        let (from, names) = if active.trim().eq_ignore_ascii_case(&name) {
            (bike.clone(), manifest(dir).unwrap_or_default())
        } else {
            (dir.to_path_buf(), list(dir).into_iter().filter(|n| is_model_file(n)).collect())
        };
        let files: Vec<PathBuf> = names.iter().map(|n| from.join(n)).filter(|p| p.is_file()).collect();
        if !files.iter().any(|f| f.file_name().is_some_and(|n| n.eq_ignore_ascii_case("model.edf"))) {
            bail!("'{name}' has no model.edf");
        }
        return Ok(Swap { name, bike, files });
    }
    // A bike folder: its active swap, else every model file at its root.
    let active = std::fs::read_to_string(dir.join(LIB_DIR).join(ACTIVE)).unwrap_or_default().trim().to_string();
    if !active.is_empty() && dir.join(LIB_DIR).join(&active).is_dir() {
        return find_swap(&dir.join(LIB_DIR).join(&active));
    }
    let files: Vec<PathBuf> = list(dir).into_iter().filter(|n| is_model_file(n)).map(|n| dir.join(n)).collect();
    if !files.iter().any(|f| f.file_name().is_some_and(|n| n.eq_ignore_ascii_case("model.edf"))) {
        bail!("no model.edf in {}", dir.display());
    }
    Ok(Swap { name, bike: dir.to_path_buf(), files })
}

// ------------------------------------------------------------------------------------- run

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub from: String,
    pub to: String,
    pub plan: Plan,
    /// The new variant folder (none on a dry run).
    pub out: Option<String>,
    pub files: Vec<String>,
    pub gfx_changed: Vec<String>,
    pub notes: Vec<String>,
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Line `swap` (a variant or bike folder) up for `target` (a bike's `.pkz` or folder) and, unless
/// `dry`, install it as `<target>/FrostMod Models/<name>/`, the way MXB App keeps model swaps.
/// Nothing that exists is overwritten.
pub fn line_up(swap: &Path, target: &Path, name: Option<&str>, anchor: Anchor, dry: bool) -> Result<Outcome> {
    let s = find_swap(swap)?;
    let (_, ga) = bike_geom(&s.bike).context("the swap's own bike")?;
    let (_, gb) = bike_geom(target).context("the new bike")?;
    let p = plan(&Mounts::parse(&ga)?, &Mounts::parse(&gb)?, anchor);
    let to_bike = bike_dir(target);
    let mut o = Outcome {
        from: file_name(&s.bike),
        to: file_name(&to_bike),
        plan: p,
        out: None,
        files: Vec::new(),
        gfx_changed: Vec::new(),
        notes: Vec::new(),
    };
    if dry {
        return Ok(o);
    }
    let name = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&s.name).to_string();
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
        bail!("'{name}' can't be a folder name");
    }
    let dest = to_bike.join(LIB_DIR).join(&name);
    if dest.exists() {
        bail!("{} already has a model called '{name}'", o.to);
    }
    // Built in this app's scratch folder, then moved in whole: a half-written variant never
    // shows up in MXB App, and a failed run leaves nothing in the mods folder.
    let tmp = std::env::temp_dir().join("frost-importer").join(format!("lineup-{}", std::process::id()));
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    std::fs::create_dir_all(&tmp)?;
    let res = (|| -> Result<()> {
        for f in &s.files {
            let n = file_name(f);
            let mut bytes = std::fs::read(f).with_context(|| format!("reading {n}"))?;
            if n.eq_ignore_ascii_case("gfx.cfg") {
                let (text, utf8) = mxb_core::gfxedit::decode(&bytes);
                let (text, changed) = move_gfx(&text, &o.plan);
                o.gfx_changed = changed;
                bytes = mxb_core::gfxedit::encode(&text, utf8);
            } else if n.to_ascii_lowercase().ends_with(".edf") {
                if bytes.starts_with(b"EDF\0") {
                    for (obj, part) in move_edf(&mut bytes, &o.plan.parts).with_context(|| format!("moving {n}"))? {
                        if part.is_none() {
                            o.notes.push(format!("{n}: object '{obj}' isn't a bike part, left where it is"));
                        }
                    }
                } else {
                    o.notes.push(format!("{n} is locked, copied as it is"));
                }
            }
            std::fs::write(tmp.join(&n), &bytes).with_context(|| format!("writing {n}"))?;
            o.files.push(n);
        }
        std::fs::write(tmp.join(MANIFEST), format!("{}
", o.files.join("
")))?;
        std::fs::create_dir_all(dest.parent().unwrap())?;
        if std::fs::rename(&tmp, &dest).is_err() {
            // Another drive: copy across.
            std::fs::create_dir_all(&dest)?;
            for n in o.files.iter().map(String::as_str).chain([MANIFEST]) {
                std::fs::copy(tmp.join(n), dest.join(n)).with_context(|| format!("copying {n}"))?;
            }
        }
        Ok(())
    })();
    // This app's own scratch folder, never one of the user's.
    let _ = std::fs::remove_dir_all(&tmp);
    res?;
    o.out = Some(dest.to_string_lossy().to_string());
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KTM: &str = "chassis_steer = 0, 1.0130, 0.3570\nchassis_rsusp_min = 0, 0.4835, -0.1957\nsteer_joint = 0, 0, 0.0020\n\
rsusp_joint = 0, 0.0285, -0.0159\nfront_upper = 0, -0.4573, 0.0240\nfwheel = 0, -0.3391, 0.0375\n\
rwheel_min = 0, 0.0331, -0.5758\nrwheel_max = 0, 0.0331, -0.6162\nrakeangle_min = 26.1\n\
shock_chassis = 0, 0.7881, -0.1400\nshock_rocker = 0, 0.0743, 0.0418\nshock_minlength = 0.337\nrear_length = 0.129\n";
    const YZ: &str = "chassis_steer = 0, 1.0061, 0.3377\nchassis_rsusp_min = 0, 0.4808, -0.1766\nsteer_joint = 0, -0.0851, 0\n\
rsusp_joint = 0, 0.0006, -0.0007\nfront_upper = 0, -0.5002, 0.0216\nfwheel = 0, -0.3344, 0.0631\n\
rwheel_min = 0, 0.0202, -0.5652\nrwheel_max = 0, 0.0202, -0.6062\nrakeangle_min = 26.9\n\
shock_chassis = 0, 0.7706, -0.1578\nshock_rocker = 0, 0.0544, 0.0309\nshock_minlength = 0.324\nrear_length = 0.125\n";

    fn close(a: V3, b: V3) -> bool {
        len(sub(a, b)) < 1e-6
    }

    /// World position of a point in a part's frame, as the game places it (edf::assemble_bike).
    fn world(m: &Mounts, part: &str, v: V3) -> V3 {
        match part {
            "chassis" => v,
            "steer" => add(rot_x(sub(v, m.steer_joint), -m.rake), m.head),
            "fsusp" => add(rot_x(sub(add(v, m.front_upper), m.steer_joint), -m.rake), m.head),
            _ => add(sub(v, m.rsusp_joint), m.pivot),
        }
    }

    #[test]
    fn the_moving_joints_land_on_the_new_bikes() {
        let (a, b) = (Mounts::parse(KTM.as_bytes()).unwrap(), Mounts::parse(YZ.as_bytes()).unwrap());
        let p = plan(&a, &b, Anchor::Fit);
        // The fork's axle on B's axle.
        assert!(close(world(&b, "fsusp", p.parts["fsusp"].apply(a.fwheel)), b.front_axle()));
        // The swingarm on B's pivot, its axle on the line to B's.
        assert!(close(world(&b, "rsusp", p.parts["rsusp"].apply(a.rsusp_joint)), b.pivot));
        let axle = sub(world(&b, "rsusp", p.parts["rsusp"].apply(a.rwheel)), b.pivot);
        let want = b.rear_arm();
        assert!((axle[1] * want[2] - axle[2] * want[1]).abs() < 1e-9);
        // The steer on B's steering axis, shifted across it with the fork.
        let j = sub(world(&b, "steer", p.parts["steer"].apply(a.steer_joint)), b.head);
        let across = rot_x(j, b.rake);
        assert!(across[1].abs() < 1e-9 && across[2] > 0.02);
        // The fork stays in the clamps: across the axis, fork and steer move the same.
        let (f, st) = (p.parts["fsusp"].t, p.parts["steer"].t);
        let fork = add(sub(f, sub(a.front_upper, a.steer_joint)), sub(b.front_upper, b.steer_joint));
        let stem = add(sub(st, b.steer_joint), a.steer_joint);
        assert!((fork[2] - stem[2]).abs() < 1e-9);
        // Fit splits the frame's gap evenly.
        assert!(p.issues.iter().any(|i| i.starts_with("Frame:")));
    }

    #[test]
    fn a_bike_onto_itself_moves_nothing() {
        let a = Mounts::parse(KTM.as_bytes()).unwrap();
        let p = plan(&a, &a, Anchor::Fit);
        for x in p.parts.values() {
            assert!(len(x.t) < 1e-12 && x.deg.abs() < 1e-12);
        }
        assert!(p.issues.is_empty(), "{:?}", p.issues);
    }

    #[test]
    fn gfx_points_move_with_their_part_and_keep_the_text() {
        let (a, b) = (Mounts::parse(KTM.as_bytes()).unwrap(), Mounts::parse(YZ.as_bytes()).unwrap());
        let p = plan(&a, &b, Anchor::Pivot);
        let text = "steer\n{\n\tleftgrip\n\t{\n\t\ttype = 1\n\t\tpos\n\t\t{\n\t\t\tx = -0.33\n\t\t\ty = 0.25\n\t\t\tz = -0.0235\n\t\t}\n\t}\n}\n\
chassis\n{\n\tchain\n\t{\n\t\tengine\n\t\t{\n\t\t\tx = -0.083\n\t\t\ty = 0.487\n\t\t\tz = -0.094\n\t\t}\n\t}\n\tshock\n\t{\n\t\tlink\n\t\t{\n\t\t\tx = 0\n\t\t\ty = 0.048\n\t\t\tz = 0.066\n\t\t}\n\t}\n}\n";
        let (out, changed) = move_gfx(text, &p);
        let d = mxb_core::gfxedit::Doc::parse(&out);
        let g = steer_grip(&p, [-0.33, 0.25, -0.0235]);
        assert_eq!(d.get("steer/leftgrip/pos/y").unwrap().trim(), num(g[1]));
        assert_eq!(d.get("chassis/shock/link/y").unwrap().trim(), num(0.048 + 0.0544 - 0.0743));
        // The chain is left as written.
        assert_eq!(d.get("chassis/chain/engine/y").unwrap().trim(), "0.487");
        assert!(changed.iter().all(|c| !c.contains("chain")));
        assert!(out.contains("\t\ttype = 1\n"));
    }

    fn steer_grip(p: &Plan, v: V3) -> V3 {
        p.parts["steer"].apply(v)
    }

    #[test]
    fn edf_root_matrices_and_bounds_move() {
        // A minimal EDF: main object, 0 materials/verts/tris, one root node with identity.
        let mut b = b"EDF\0".to_vec();
        for v in [-1.0f32, 0.0, -1.0, 1.0, 1.0, 1.0] {
            b.extend(v.to_le_bytes());
        }
        b.extend([0u8; 12]); // materials, vertices, triangles
        b.extend(1u32.to_le_bytes()); // nodes
        let mut name = [0u8; 100];
        name[..7].copy_from_slice(b"chassis");
        b.extend(name);
        b.extend(0x1000u32.to_le_bytes());
        for i in 0..16 {
            b.extend((if i % 5 == 0 { 1.0f32 } else { 0.0 }).to_le_bytes());
        }
        b.extend([0u8; 64]);
        b.extend([0u8; 4 * 4]); // parent, children, groups, pad
        b.extend([0u8; 4 + 24]); // first vertex, bounds
        b.extend(0u32.to_le_bytes()); // textures
        let parts = HashMap::from([("chassis", Xf { deg: 0.0, t: [0.0, 0.01, -0.02] })]);
        let done = move_edf(&mut b, &parts).unwrap();
        assert_eq!(done, vec![(String::new(), Some("chassis"))]);
        let m = 4 + 24 + 12 + 4 + 104;
        assert!((rd_f32(&b, m + 7 * 4) - 0.01).abs() < 1e-7);
        assert!((rd_f32(&b, m + 11 * 4) + 0.02).abs() < 1e-7);
        assert!((rd_f32(&b, 4 + 4) - 0.01).abs() < 1e-7);
    }

    /// The shared mount spec's fixtures (mxbsecure-web `src/lib/mounts.fixtures.json`, branch
    /// feat/build-autoplace): `MXB_MOUNT_FIXTURES=<path> cargo test -- --ignored mount_spec`.
    #[test]
    #[ignore]
    fn mount_spec_fixtures() {
        let path = std::env::var("MXB_MOUNT_FIXTURES").expect("MXB_MOUNT_FIXTURES");
        let f: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let tol = f["tolerance_m"].as_f64().unwrap();
        let v3 = |v: &serde_json::Value| -> V3 { [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()] };
        let mut n = 0;
        for (name, g) in f["geoms"].as_object().unwrap() {
            let text = g["geom"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect::<Vec<_>>().join("
");
            let m = Mounts::parse(text.as_bytes()).unwrap();
            let e = &g["expect"];
            let near = |what: &str, got: V3, want: V3| assert!(len(sub(got, want)) <= tol, "{name} {what}: {got:?} vs {want:?}");
            assert!((-m.rake - e["rake"].as_f64().unwrap()).abs() < 1e-6, "{name} rake");
            near("head", m.head, v3(&e["head"]));
            near("pivot", m.pivot, v3(&e["pivot"]));
            near("front axle", m.front_axle(), v3(&e["front_axle"]));
            near("rear axle", m.rear_axle(), v3(&e["rear_axle"]));
            let wb = (m.front_axle()[2] - m.rear_axle()[2]).abs();
            assert!((wb - e["wheelbase"].as_f64().unwrap()).abs() <= tol, "{name} wheelbase {wb}");
            for p in e["places"].as_array().unwrap() {
                let part = p["part"].as_str().unwrap();
                near(part, world(&m, part, v3(&p["local"])), v3(&p["world"]));
            }
            n += 1;
        }
        assert!(n > 0);
    }
}
