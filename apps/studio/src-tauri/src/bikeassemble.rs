//! Putting the bike builder's parts together: where each one goes, and the frame the game
//! wants it in.
//!
//! Two frames meet here.
//!
//! - **Blender's**, which the parts are catalogued in: metres, Z up, the bike facing -Y.
//! - **The game's**: left-handed, +X right, +Y up, +Z forward. A point goes from one to the
//!   other through the FBX export (Blender +Y → FBX -Z, Z → Y) and the converter's X mirror,
//!   which together are `game = (-x, z, -y)`: [`to_game`] and [`to_blender`].
//!
//! A bike is placed by its `.geom`: the chassis as it is, the swingarm by its pivot, the
//! steer and the fork turned by the rake about the steering head (see `mxb_core::edf`'s
//! `assemble_bike`, which the viewer draws every installed bike with). The *template* is the
//! bike whose `.geom` a build is for. Its mount points become the anchors each role snaps
//! onto, and on the way out every part is carried back into the frame the `.geom` will place
//! it from. Studio's own placeholder bike is a template like any other; a real bike's folder
//! swaps in for it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::bikeparts::{Part, Role};

pub type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Turn `p` about `pivot` by `deg`, an XYZ Euler in degrees in Blender's frame — the same
/// matrix Blender's own `Euler((x, y, z), 'XYZ').to_matrix()` makes (Rz · Ry · Rx), so the
/// preview, the anchors here and the build's `assemble` job all turn a part the same way.
pub fn turn_about(p: V3, pivot: V3, deg: V3) -> V3 {
    if deg.iter().all(|d| *d == 0.0) {
        return p;
    }
    let [x, y, z] = sub(p, pivot);
    let (sx, cx) = deg[0].to_radians().sin_cos();
    let (sy, cy) = deg[1].to_radians().sin_cos();
    let (sz, cz) = deg[2].to_radians().sin_cos();
    // Rx, then Ry, then Rz.
    let (y, z) = (y * cx - z * sx, y * sx + z * cx);
    let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
    let (x, y) = (x * cz - y * sz, x * sz + y * cz);
    add([x, y, z], pivot)
}

/// Rotate about X by `deg`, as `mxb_core::edf` does (design frame, +Y up, +Z forward).
fn rot_x(p: V3, deg: f64) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

/// Blender (Z up, facing -Y) → the game (Y up, +Z forward, X mirrored).
pub fn to_game(b: V3) -> V3 {
    [-b[0], b[2], -b[1]]
}

/// The game → Blender.
pub fn to_blender(g: V3) -> V3 {
    [-g[0], -g[2], g[1]]
}

/// A bike's mount points as its `.geom` gives them, in the game's frame. Each is in the frame
/// of the part it belongs to: `chassis_steer` in the chassis', `steer_joint` in the steer's.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frames {
    /// `chassis_steer`: the steering head.
    pub head: V3,
    /// `chassis_rsusp_min`: the swingarm pivot.
    pub pivot: V3,
    pub steer_joint: V3,
    pub rsusp_joint: V3,
    pub front_upper: V3,
    /// `rakeangle_min`, degrees back from vertical.
    pub rake_deg: f64,
    /// `fwheel`, in the fork's frame.
    pub fwheel: V3,
    /// The middle of `rwheel_min`/`rwheel_max`, in the swingarm's frame.
    pub rwheel: V3,
}

/// Where the anchors a `.geom` doesn't name sit, from ones it does, in Blender's frame:
/// measured off a stock 450, and what the placeholder parts are built around.
const HANDLEBAR_FROM_HEAD: V3 = [0.0, 0.07, 0.17];
const PLATE_FROM_HEAD: V3 = [0.0, -0.11, 0.05];
const FOOTPEGS_FROM_PIVOT: V3 = [0.0, -0.04, -0.12];

impl Frames {
    /// Studio's placeholder bike, the same numbers `frost_make.py`'s `ANCHORS` builds its
    /// parts around: a stock-sized 450, 27° of rake, every joint at its part's origin.
    pub fn placeholder() -> Self {
        let head = to_game([0.0, -0.47, 0.93]);
        let pivot = to_game([0.0, 0.12, 0.52]);
        let rake_deg = 27.0;
        // fwheel is the front axle in the fork's own frame: turned back by the rake.
        let front_axle = to_game([0.0, -0.74, 0.365]);
        let fwheel = rot_x(sub(front_axle, head), rake_deg);
        let rwheel = sub(to_game([0.0, 0.74, 0.35]), pivot);
        Frames { head, pivot, steer_joint: [0.0; 3], rsusp_joint: [0.0; 3], front_upper: [0.0; 3], rake_deg, fwheel, rwheel }
    }

    /// A real bike's, from its `.geom`. `None` when it lacks the mounts a bike is placed by.
    pub fn from_geom(bytes: &[u8]) -> Option<Self> {
        let g = mxb_core::edf::parse_geom(bytes);
        let sc = mxb_core::edf::parse_geom_scalars(bytes);
        let v = |k: &str| g.get(k).map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]);
        let lo = v("rwheel_min")?;
        let hi = v("rwheel_max").unwrap_or(lo);
        Some(Frames {
            head: v("chassis_steer")?,
            pivot: v("chassis_rsusp_min")?,
            steer_joint: v("steer_joint")?,
            rsusp_joint: v("rsusp_joint")?,
            front_upper: v("front_upper")?,
            rake_deg: sc.get("rakeangle_min").copied().unwrap_or(0.0) as f64,
            fwheel: v("fwheel")?,
            rwheel: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0],
        })
    }

    /// These mounts as `.geom` lines: what the viewer places a bike by. Only the mounts, never
    /// a whole `.geom` (a bike needs its physics too), so it's for checking a build, not for
    /// shipping one.
    pub fn geom_mounts(&self) -> String {
        let v = |k: &str, p: V3| format!("{k} = {}, {}, {}
", p[0], p[1], p[2]);
        [
            v("chassis_steer", self.head),
            v("chassis_rsusp_min", self.pivot),
            format!("rakeangle_min = {}\n", self.rake_deg),
            v("steer_joint", self.steer_joint),
            v("front_upper", self.front_upper),
            v("fwheel", self.fwheel),
            v("rsusp_joint", self.rsusp_joint),
            v("rwheel_min", self.rwheel),
            v("rwheel_max", self.rwheel),
        ]
        .concat()
    }

    /// The signed angle `mxb_core::edf` turns the steer and fork by.
    fn rake(&self) -> f64 {
        -self.rake_deg
    }

    fn fork_origin(&self) -> V3 {
        add(rot_x(sub(self.front_upper, self.steer_joint), self.rake()), self.head)
    }

    /// Every anchor a role can snap onto, in Blender's frame.
    pub fn anchors(&self) -> BTreeMap<String, V3> {
        let head = to_blender(self.head);
        let pivot = to_blender(self.pivot);
        let front = add(rot_x(self.fwheel, self.rake()), self.fork_origin());
        let rear = add(self.rwheel, sub(self.pivot, self.rsusp_joint));
        [
            ("steer_axis", head),
            ("fork_clamp", to_blender(self.fork_origin())),
            ("swingarm_pivot", pivot),
            ("front_axle", to_blender(front)),
            ("rear_axle", to_blender(rear)),
            ("handlebar", add(head, HANDLEBAR_FROM_HEAD)),
            ("plate_mount", add(head, PLATE_FROM_HEAD)),
            ("footpegs", add(pivot, FOOTPEGS_FROM_PIVOT)),
        ]
        .into_iter()
        .map(|(k, p)| (k.to_string(), p))
        .collect()
    }

    /// The matrix (row-major, 4×4, Blender's frame) that carries a point of the assembled
    /// bike into the frame `group`'s `.geom` entry places it from. Inverts `assemble_bike`:
    ///
    /// - chassis: as it is;
    /// - rsusp: `world = local + pivot - rsusp_joint`;
    /// - steer: `world = rot(local - steer_joint) + head`;
    /// - fsusp: `world = rot(local) + fork_origin`.
    pub fn into_part(&self, group: Group) -> [[f64; 4]; 4] {
        // In the game's frame: local = R·(world - o) + k, R the inverse rake (or none).
        let (turn, o, k) = match group {
            Group::Chassis => (0.0, [0.0; 3], [0.0; 3]),
            Group::Rsusp => (0.0, self.pivot, self.rsusp_joint),
            Group::Steer => (-self.rake(), self.head, self.steer_joint),
            Group::Fsusp => (-self.rake(), self.fork_origin(), [0.0; 3]),
        };
        // As a function of a Blender point b: to_blender(R·(to_game(b) - o) + k). Every step
        // is affine, so its columns are the images of the axes and the origin.
        let f = |b: V3| to_blender(add(rot_x(sub(to_game(b), o), turn), k));
        let origin = f([0.0; 3]);
        let mut m = [[0.0; 4]; 4];
        for (j, axis) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].into_iter().enumerate() {
            let col = sub(f(axis), origin);
            for i in 0..3 {
                m[i][j] = col[i];
            }
        }
        for i in 0..3 {
            m[i][3] = origin[i];
        }
        m[3][3] = 1.0;
        m
    }
}

/// A game node's place on the bike: where its origin sits and which way its axes point, in
/// Blender's frame. A point a kit modelled at `p` in that node's own space sits at
/// `basis·p + origin` on the bike.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeFrame {
    pub origin: V3,
    /// Rows.
    pub basis: M3,
}

pub type M3 = [[f64; 3]; 3];

const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn mat_vec(m: &M3, v: V3) -> V3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Carry a point from one node frame to another.
fn carry(local_from: &NodeFrame, to: &NodeFrame, p: V3) -> V3 {
    let transpose: M3 = [0, 1, 2].map(|i| [0, 1, 2].map(|j| local_from.basis[j][i]));
    let local = mat_vec(&transpose, sub(p, local_from.origin));
    add(mat_vec(&to.basis, local), to.origin)
}

fn mat_mul(a: &M3, b: &M3) -> M3 {
    [0, 1, 2].map(|i| [0, 1, 2].map(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j]))
}

/// A turn about Blender's X by `deg`, as rows.
fn rot_x_m(deg: f64) -> M3 {
    let (s, c) = deg.to_radians().sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

/// Half a turn about Blender's Z: a bike facing +Y instead of -Y.
const ABOUT_FACE: M3 = [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]];

impl Frames {
    /// Where `group`'s node is on the assembled bike: [`Frames::into_part`] undone. A kit
    /// models its parts in exactly that node frame (Blender's axes, the node at the origin,
    /// the steer's own axis straight up), so this is where such a part goes on this template.
    pub fn node_frame(&self, group: Group) -> NodeFrame {
        let m = self.into_part(group);
        // A turn and a move (see `into_part`): undone by the turn's transpose.
        let basis: M3 = [0, 1, 2].map(|i| [0, 1, 2].map(|j| m[j][i]));
        let back = mat_vec(&basis, [m[0][3], m[1][3], m[2][3]]);
        NodeFrame { origin: [-back[0], -back[1], -back[2]], basis }
    }
}

/// The game node a kit's `mxb_node` names, when it's one of the four a bike is built from.
pub fn node_group(node: &str) -> Option<Group> {
    match node.trim().to_ascii_lowercase().as_str() {
        "chassis" => Some(Group::Chassis),
        "steer" => Some(Group::Steer),
        "fsusp" | "front_susp" => Some(Group::Fsusp),
        "rsusp" | "rear_susp" => Some(Group::Rsusp),
        _ => None,
    }
}

/// The base bike's own game nodes: the empties a bike exported from the game (or a Frost
/// Blend of one) carries for `chassis`, `steer`, `fsusp` and `rsusp`, and what they say about
/// the bike — which way it faces and its rake. Empty when the base brings none, and then the
/// template's `.geom` says where the nodes are.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BaseNodes {
    pub at: BTreeMap<Group, V3>,
    /// The base faces +Y (as the converter's own FBX export of a bike does), the other way
    /// from the templates and Studio's own parts.
    pub faces_plus_y: bool,
    /// Degrees back from vertical, read off the steer's (or fork's) node when it says.
    pub rake_deg: Option<f64>,
}

impl BaseNodes {
    /// Read off the base: its chassis part first, then the rest of the bike it was split from
    /// (parts in the same folder as the chassis' own file), never a kit part. The first empty
    /// named for a node wins.
    pub fn find(chassis: Option<&Part>, library: &[Part]) -> BaseNodes {
        let Some(chassis) = chassis else { return BaseNodes::default() };
        let folder = |p: &Part| {
            Path::new(&p.source).parent().map(|d| d.to_string_lossy().replace('\\', "/").to_ascii_lowercase())
        };
        let home = folder(chassis);
        let mut from: Vec<&Part> = vec![chassis];
        from.extend(
            library
                .iter()
                .filter(|p| p.id != chassis.id && p.node.is_none() && home.is_some() && folder(p) == home),
        );
        let mut at = BTreeMap::new();
        let mut axis: Option<V3> = None;
        for group in Group::ALL {
            let Some(e) = from.iter().find_map(|p| own_empty_full(p, group.name())) else { continue };
            at.insert(group, e.location);
            if axis.is_none() && matches!(group, Group::Steer | Group::Fsusp) {
                axis = e.rotation.as_ref().and_then(steer_axis);
            }
        }
        // The steer's axis leans back: that says the facing and the rake outright. Without it
        // (a part catalogued before Studio kept an empty's turn) the steer sits in front of the
        // swingarm (or of the chassis' middle), on whichever side the bike faces.
        let (faces_plus_y, rake_deg) = match axis {
            Some(a) => (a[1] < 0.0, Some(a[1].abs().atan2(a[2]).to_degrees())),
            None => {
                let front = at.get(&Group::Steer).or(at.get(&Group::Fsusp));
                let back = at.get(&Group::Rsusp).map(|p| p[1]).or_else(|| centre(chassis).map(|c| c[1]));
                match (front, back) {
                    (Some(f), Some(b)) => (f[1] > b, None),
                    _ => (false, None),
                }
            }
        };
        BaseNodes { at, faces_plus_y, rake_deg }
    }

    /// Where a part modelled in `group`'s node space goes on this base, `template` filling in
    /// what the base doesn't say.
    pub fn frame(&self, template: &Frames, group: Group) -> NodeFrame {
        let on_template = template.node_frame(group);
        if self.at.is_empty() {
            return on_template;
        }
        let rake = self.rake_deg.unwrap_or(template.rake_deg);
        let mut basis = match group {
            Group::Steer | Group::Fsusp => rot_x_m(-rake),
            Group::Chassis | Group::Rsusp => IDENTITY,
        };
        if self.faces_plus_y {
            basis = mat_mul(&ABOUT_FACE, &basis);
        }
        NodeFrame { origin: self.at.get(&group).copied().unwrap_or(on_template.origin), basis }
    }
}

/// The node an anchor keeps its place against when it's carried onto a base bike. The fork's
/// two go with the steering head, not the base's `fsusp` node: where that node sits along the
/// fork differs between the placeholder (at the head) and a game export (partway down the
/// legs), while the head-to-axle distance is a fork length on any bike.
fn anchor_group(name: &str) -> Group {
    match name {
        "steer_axis" | "handlebar" | "plate_mount" | "fork_clamp" | "front_axle" => Group::Steer,
        "swingarm_pivot" | "rear_axle" => Group::Rsusp,
        "footpegs" => Group::Chassis,
        _ => Group::Chassis,
    }
}

/// The template's anchors carried onto the base bike's own node frames.
pub fn base_anchors(template: &Frames, nodes: &BaseNodes) -> BTreeMap<String, V3> {
    let anchors = template.anchors();
    if nodes.at.is_empty() {
        return anchors;
    }
    anchors
        .into_iter()
        .map(|(name, p)| {
            let group = anchor_group(&name);
            let p = carry(&template.node_frame(group), &nodes.frame(template, group), p);
            (name, p)
        })
        .collect()
}

/// The steering axis an empty's turn carries: whichever of its axes is closest to upright,
/// pointed up. Read this way rather than trusting one particular axis, since the game's own
/// export and a hand-made Blend turn their node empties by different conventions; the axis
/// they share is the steering axis, near enough vertical. `None` when nothing's within 45° of
/// upright.
fn steer_axis(rows: &M3) -> Option<V3> {
    let col = |j: usize| [rows[0][j], rows[1][j], rows[2][j]];
    let best = (0..3).map(col).max_by(|a, b| a[2].abs().total_cmp(&b[2].abs()))?;
    let len = (best[0] * best[0] + best[1] * best[1] + best[2] * best[2]).sqrt();
    if len < 1e-9 || best[2].abs() / len < 45f64.to_radians().cos() {
        return None;
    }
    let s = best[2].signum() / len;
    Some([best[0] * s, best[1] * s, best[2] * s])
}

/// The four parts the game's `.hrc` files name, which every role is built into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Chassis,
    Steer,
    Fsusp,
    Rsusp,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Chassis, Group::Steer, Group::Fsusp, Group::Rsusp];

    pub fn name(self) -> &'static str {
        match self {
            Group::Chassis => "chassis",
            Group::Steer => "steer",
            Group::Fsusp => "fsusp",
            Group::Rsusp => "rsusp",
        }
    }

    /// How far the part's root empty is turned about Blender's X, in degrees.
    ///
    /// The converter doesn't keep a part root's rotation: it writes the part's own axes
    /// instead, as `fbx2edf.exe` does (`0 180 0` for chassis and rsusp, `-90 180 0` for steer
    /// and fsusp, Rz·Ry·Rx), and keeps everything under the root where it is relative to it.
    /// A root turned by this much is one the converter's rotation stands in for exactly, so
    /// the meshes come out where the build put them. Checked by a round trip of a stock bike
    /// through Blender and the converter: every part's bounds match its model.edf.
    pub fn root_turn(self) -> f64 {
        match self {
            Group::Chassis | Group::Rsusp => 90.0,
            Group::Steer | Group::Fsusp => 180.0,
        }
    }
}

/// Where a role hangs, which game part it's built into, and the anchors it gives the roles
/// below it. `None` for the group: the wheels, which the game takes from a tyres mod rather
/// than the bike, so they're in the preview and not in the build.
pub fn role_rule(role: Role) -> (Option<&'static str>, Option<Group>, &'static [&'static str]) {
    match role {
        Role::Chassis => (None, Some(Group::Chassis), &["steer_axis", "swingarm_pivot", "footpegs"]),
        Role::Steer => (Some("steer_axis"), Some(Group::Steer), &["fork_clamp", "handlebar", "plate_mount"]),
        Role::Fsusp => (Some("fork_clamp"), Some(Group::Fsusp), &["front_axle"]),
        Role::Rsusp => (Some("swingarm_pivot"), Some(Group::Rsusp), &["rear_axle"]),
        Role::WheelF => (Some("front_axle"), None, &[]),
        Role::WheelR => (Some("rear_axle"), None, &[]),
        Role::Levers => (Some("handlebar"), Some(Group::Steer), &[]),
        Role::Pedals => (Some("footpegs"), Some(Group::Chassis), &[]),
        Role::Handguards => (Some("handlebar"), Some(Group::Steer), &[]),
        Role::Plate => (Some("plate_mount"), Some(Group::Steer), &[]),
    }
}

/// The template a build is for.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TemplateSource {
    /// Studio's placeholder bike: every part lands, nothing is rideable.
    Placeholder,
    /// An installed bike's folder or `.pkz`: its `.geom` places the parts and its setup files
    /// come with the build.
    Bike { path: String },
}

pub struct Template {
    pub source: TemplateSource,
    pub name: String,
    pub frames: Frames,
    /// The template's own files (setup, textures), when it's a real bike.
    pub files: Vec<(String, Vec<u8>)>,
}

impl Template {
    pub fn load(source: &TemplateSource) -> anyhow::Result<Template> {
        match source {
            TemplateSource::Placeholder => Ok(Template {
                source: source.clone(),
                name: "placeholder".into(),
                frames: Frames::placeholder(),
                files: Vec::new(),
            }),
            TemplateSource::Bike { path } => {
                let p = Path::new(path);
                let files = mxb_core::viewer::gather_bike_files(p).map_err(|e| {
                    // The bike picker already keeps a protected `.pkz` from being chosen in
                    // the first place (`bike_template_readable`); this is the fallback for
                    // whatever reaches here anyway — "Choose a bike…" pointed straight at
                    // one, say — so the rider gets a reason instead of Blender's own
                    // "unsupported .pkz" wording, which reads like a bug report, not an
                    // answer.
                    let msg = format!("{e:#}");
                    if msg.contains("unsupported .pkz") || msg.contains("secured content") {
                        anyhow::anyhow!(
                            "{} is protected content this build can't read (OEM/stock bikes need the game itself, or a build with that support)",
                            p.display()
                        )
                    } else {
                        e
                    }
                })?;
                let geom = files
                    .iter()
                    .find(|(n, _)| n.to_ascii_lowercase().ends_with(".geom"))
                    .ok_or_else(|| anyhow::anyhow!("{} has no .geom, so it can't place parts", p.display()))?;
                let frames = Frames::from_geom(&geom.1)
                    .ok_or_else(|| anyhow::anyhow!("{} lacks the mount points a bike is placed by", geom.0))?;
                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                Ok(Template { source: source.clone(), name, frames, files })
            }
        }
    }
}

/// One part, placed.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub role: Role,
    pub part_id: String,
    /// Added to every point of the part, in Blender's frame. Includes the nudge.
    pub offset: V3,
    pub nudge: V3,
    /// The rider's turn of the part, an XYZ Euler in degrees (Blender's frame), applied after
    /// `offset` about `pivot`. Zero for a part nobody turned.
    pub rotation: V3,
    /// What the part turns about, in Blender's frame: its joint (where it meets its anchor,
    /// nudge included), else its middle, else its origin.
    pub pivot: V3,
    /// The anchor it hangs from, and where that is.
    pub mount: Option<String>,
    pub at: Option<V3>,
    /// How the part's own end of the joint was found: "empty" (it has one by the anchor's
    /// name), "centre" (a wheel's middle), or "as modelled" (it was built in place).
    pub by: &'static str,
    pub group: Option<Group>,
    /// How a kit part, modelled in its game node's own space, is turned onto the bike before
    /// `offset` (rows). `None` for every other part, which is used as it's turned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<M3>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assembly {
    pub placed: Vec<Placed>,
    /// Every anchor where it ended up, after the parts that carry it were placed.
    pub anchors: BTreeMap<String, V3>,
}

/// The part's own empty for `anchor`: its exact name, or Blender's duplicate of it
/// (`steer_axis.001`), ignoring case.
fn own_empty(part: &Part, anchor: &str) -> Option<V3> {
    part.empties
        .iter()
        .find(|e| {
            let n = e.name.to_ascii_lowercase();
            n == anchor || n.strip_prefix(anchor).is_some_and(|rest| rest.starts_with('.'))
        })
        .map(|e| e.location)
}

/// [`own_empty`], the whole empty.
fn own_empty_full<'a>(part: &'a Part, anchor: &str) -> Option<&'a crate::bikeparts::Empty> {
    part.empties.iter().find(|e| {
        let n = e.name.to_ascii_lowercase();
        n == anchor || n.strip_prefix(anchor).is_some_and(|rest| rest.starts_with('.'))
    })
}

fn centre(part: &Part) -> Option<V3> {
    part.bounds.as_ref().map(|b| {
        [(b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0, (b.min[2] + b.max[2]) / 2.0]
    })
}

/// Put the slotted parts on the template, parents first.
///
/// A part meets its anchor with the empty it carries by that anchor's name. A wheel without
/// one is taken by its middle. Anything else without one is taken to have been built in place,
/// on a bike like the template, and moves only as far as its anchor has. The anchors a part
/// provides follow it: from its own empties if it has them, else carried by that same move.
/// A nudge moves a part and everything hanging from it.
pub fn place(template: &Frames, slots: &BTreeMap<Role, &Part>, nudges: &BTreeMap<Role, V3>) -> Assembly {
    place_turned(template, slots, nudges, &BTreeMap::new())
}

/// `place`, with the rider's turns: a turned part carries the anchors it provides round with
/// it, so what hangs from it follows to where it now meets it (moved, not turned itself).
pub fn place_turned(
    template: &Frames,
    slots: &BTreeMap<Role, &Part>,
    nudges: &BTreeMap<Role, V3>,
    rotations: &BTreeMap<Role, V3>,
) -> Assembly {
    place_on(template, &BaseNodes::default(), slots, nudges, rotations)
}

/// `place_turned`, on a base bike whose own game nodes are known: a kit part (one modelled in
/// a game node's space, see `Part::node`) snaps its origin onto the base's matching node,
/// turned the way that node is, rather than sitting where it was modelled — which for a fork
/// is round the origin, under the bike.
pub fn place_on(
    template: &Frames,
    nodes: &BaseNodes,
    slots: &BTreeMap<Role, &Part>,
    nudges: &BTreeMap<Role, V3>,
    rotations: &BTreeMap<Role, V3>,
) -> Assembly {
    let base = base_anchors(template, nodes);
    let mut anchors = base.clone();
    let mut placed = Vec::new();
    for role in Role::ALL {
        let Some(part) = slots.get(&role) else { continue };
        let (mount, group, provides) = role_rule(role);
        let nudge = nudges.get(&role).copied().unwrap_or([0.0; 3]);
        let at = mount.and_then(|m| anchors.get(m).copied());
        // How far this role's anchor moved from the template's, before the nudge.
        let moved = match (mount, at) {
            (Some(m), Some(at)) => sub(at, base[m]),
            _ => [0.0; 3],
        };
        let (offset, by) = match (mount, at) {
            (Some(m), Some(at)) => match own_empty(part, m) {
                Some(own) => (sub(at, own), "empty"),
                None if matches!(role, Role::WheelF | Role::WheelR) && centre(part).is_some() => {
                    (sub(at, centre(part).unwrap()), "centre")
                }
                None => (moved, "as modelled"),
            },
            _ => ([0.0; 3], "as modelled"),
        };
        let kit_group = part.node.as_deref().and_then(node_group);
        let kit = kit_group.map(|g| nodes.frame(template, g));
        let (offset, by) = match kit {
            Some(f) => (f.origin, "node"),
            None => (offset, by),
        };
        let offset = add(offset, nudge);
        let shift = add(moved, nudge);
        let rotation = rotations.get(&role).copied().unwrap_or([0.0; 3]);
        let pivot = match (kit, at) {
            // A kit part turns about its node, where it meets the bike.
            (Some(_), _) => offset,
            (None, Some(at)) => add(at, nudge),
            (None, None) => centre(part).map(|c| add(c, offset)).unwrap_or(offset),
        };
        for name in provides {
            let p = match (kit, kit_group) {
                // A kit part's empties are in its node's own space, as its mesh is: turned onto
                // the bike with it. An anchor it doesn't carry keeps its place in the template's
                // node frame and goes where that node now is — on a base facing the other way
                // or raked differently, that's not the template's own spot.
                (Some(f), Some(g)) => {
                    match own_empty(part, name) {
                        Some(own) => add(mat_vec(&f.basis, own), offset),
                        None => add(
                            carry(&template.node_frame(g), &f, template.anchors()[*name]),
                            nudge,
                        ),
                    }
                }
                _ => match own_empty(part, name) {
                    Some(own) => add(own, offset),
                    None => add(base[*name], shift),
                },
            };
            anchors.insert(name.to_string(), turn_about(p, pivot, rotation));
        }
        placed.push(Placed {
            role,
            part_id: part.id.clone(),
            offset,
            nudge,
            rotation,
            pivot,
            mount: mount.map(str::to_string),
            at,
            by,
            group,
            basis: kit.map(|f| f.basis),
        });
    }
    Assembly { placed, anchors }
}

/// What the builder keeps between sessions beyond the slots: the template, and each role's nudge.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSettings {
    #[serde(default)]
    pub template: Option<TemplateSource>,
    #[serde(default)]
    pub nudges: BTreeMap<Role, V3>,
    /// Each turned role's turn, XYZ degrees in Blender's frame. See `Placed::rotation`.
    #[serde(default)]
    pub rotations: BTreeMap<Role, V3>,
    /// The name the build's folder and files take.
    #[serde(default)]
    pub name: String,
}

impl BuildSettings {
    pub fn file(root: &Path) -> PathBuf {
        root.join("build.json")
    }

    pub fn load(root: &Path) -> BuildSettings {
        std::fs::read(Self::file(root))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(root)?;
        let tmp = Self::file(root).with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, Self::file(root))?;
        Ok(())
    }

    pub fn template(&self) -> TemplateSource {
        self.template.clone().unwrap_or(TemplateSource::Placeholder)
    }
}

/// A build's folder name: the name given, or "frost_bike", kept to what a file system and the
/// game's mod list both take.
pub fn folder_name(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "frost_bike".into()
    } else {
        s.chars().take(40).collect()
    }
}

/// The `.hrc` for one part: a single level, from the build's own `model.edf`.
pub fn hrc(group: Group) -> String {
    format!("level0\n{{\n\tscene = model.edf\n\tname = {}\n\tswitch = 0\n}}\n", group.name())
}

/// The `gfx.cfg` a build gets when its template brings none.
pub fn default_gfx(tyres: &str) -> String {
    format!(
        "chassis\n{{\n\tmodel\n\t{{\n\t\tfile = chassis.hrc\n\t}}\n\tshadow\n\t{{\n\t\tfile = model_shadow.edf\n\t}}\n\trearbrakepedal\n\t{{\n\t\tname = rearbrake_lever\n\t\taxis = x\n\t\tmaxrot = 10\n\t}}\n}}\n\
         steer\n{{\n\tmodel\n\t{{\n\t\tfile = steer.hrc\n\t}}\n}}\n\
         front_susp\n{{\n\tmodel\n\t{{\n\t\tfile = fsusp.hrc\n\t}}\n}}\n\
         rear_susp\n{{\n\tmodel\n\t{{\n\t\tfile = rsusp.hrc\n\t}}\n}}\n\
         tyres = {tyres}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bikeparts::{Bounds, Empty};

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    fn apply(m: &[[f64; 4]; 4], p: V3) -> V3 {
        let mut o = [0.0; 3];
        for i in 0..3 {
            o[i] = m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3];
        }
        o
    }

    #[test]
    fn frames_round_trip() {
        let b = [0.1, -0.5, 0.9];
        assert!(close(to_blender(to_game(b)), b));
        // The bike's front is -Y in Blender and +Z in the game; up is Z and Y.
        assert!(close(to_game([0.0, -1.0, 0.0]), [0.0, 0.0, 1.0]));
        assert!(close(to_game([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn the_placeholder_anchors_are_the_ones_its_parts_are_built_around() {
        let a = Frames::placeholder().anchors();
        assert!(close(a["steer_axis"], [0.0, -0.47, 0.93]));
        assert!(close(a["fork_clamp"], [0.0, -0.47, 0.93]));
        assert!(close(a["swingarm_pivot"], [0.0, 0.12, 0.52]));
        assert!(close(a["front_axle"], [0.0, -0.74, 0.365]), "{:?}", a["front_axle"]);
        assert!(close(a["rear_axle"], [0.0, 0.74, 0.35]));
        assert!(close(a["handlebar"], [0.0, -0.40, 1.10]));
        assert!(close(a["footpegs"], [0.0, 0.08, 0.40]));
        assert!(close(a["plate_mount"], [0.0, -0.58, 0.98]));
    }

    #[test]
    fn root_turns_differ_as_the_converters_part_axes_do() {
        // The converter's part axes, in the game's frame: degrees, applied X then Y then Z.
        fn axes(g: Group) -> V3 {
            match g {
                Group::Chassis | Group::Rsusp => [0.0, 180.0, 0.0],
                Group::Steer | Group::Fsusp => [-90.0, 180.0, 0.0],
            }
        }
        fn rot_y(p: V3, deg: f64) -> V3 {
            let (s, c) = deg.to_radians().sin_cos();
            [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
        }
        fn rot_z(p: V3, deg: f64) -> V3 {
            let (s, c) = deg.to_radians().sin_cos();
            [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
        }
        let turn = |p: V3, a: V3| rot_z(rot_y(rot_x(p, a[0]), a[1]), a[2]);
        let unturn = |p: V3, a: V3| rot_x(rot_y(rot_z(p, -a[2]), -a[1]), -a[0]);
        // Turning from one part's root to another's in Blender is the same turn as from one
        // part's axes to the other's in the game: so one root put right puts them all right.
        for a in Group::ALL {
            for b in Group::ALL {
                for v in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
                    let in_blender = rot_x(rot_x(v, -b.root_turn()), a.root_turn());
                    let in_game = to_blender(turn(unturn(to_game(v), axes(b)), axes(a)));
                    assert!(close(in_blender, in_game), "{a:?} from {b:?}: {in_blender:?} vs {in_game:?}");
                }
            }
        }
        // Where they start is pinned by the round trip of a stock bike (see `root_turn`).
        assert_eq!(Group::Chassis.root_turn(), 90.0);
        assert_eq!(Group::Steer.root_turn(), 180.0);
    }

    /// The CR250 `.geom` the core's own tests use: a real bike's mounts.
    const CR250: &str = "chassis_steer = 0, 0.9591, 0.3317\nchassis_rsusp_min = 0, 0.401, -0.219\nrakeangle_min = 27.2\n\
        steer_joint = 0, 0.0025, -0.0229\nfront_upper = 0, -0.4032, -0.0026\nfwheel = 0, -0.2046, 0.0147\n\
        rsusp_joint = 0, 0.0004, 0.0558\nrwheel_min = 0, 0.0118, -0.4985\nrwheel_max = 0, 0.0122, -0.5359\n";

    #[test]
    fn a_real_geom_gives_anchors_and_part_frames() {
        let f = Frames::from_geom(CR250.as_bytes()).expect("the CR250's mounts");
        assert_eq!(f.rake_deg as f32, 27.2);
        let a = f.anchors();
        // The head is in front of the pivot and above it; the axles are ahead of and behind them.
        assert!(a["steer_axis"][1] < a["swingarm_pivot"][1] && a["steer_axis"][2] > a["swingarm_pivot"][2]);
        assert!(a["front_axle"][1] < a["steer_axis"][1] && a["rear_axle"][1] > a["swingarm_pivot"][1]);

        // Carried into its own frame, the steer's anchor lands on its steer_joint, and the
        // swingarm's pivot on its rsusp_joint: exactly what the .geom will put back.
        let steer = f.into_part(Group::Steer);
        assert!(close(apply(&steer, a["steer_axis"]), to_blender(f.steer_joint)));
        let rsusp = f.into_part(Group::Rsusp);
        assert!(close(apply(&rsusp, a["swingarm_pivot"]), to_blender(f.rsusp_joint)));
        let fsusp = f.into_part(Group::Fsusp);
        assert!(close(apply(&fsusp, a["front_axle"]), to_blender(f.fwheel)));
        assert!(close(apply(&f.into_part(Group::Chassis), a["footpegs"]), a["footpegs"]));
        // The steer's frame is turned, not mirrored or stretched.
        let col = |j: usize| [steer[0][j], steer[1][j], steer[2][j]];
        for j in 0..3 {
            let c = col(j);
            assert!(((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() - 1.0).abs() < 1e-9);
        }
        assert!(Frames::from_geom(b"chassis_steer = 0, 1, 0\n").is_none());
    }

    fn part(id: &str, empties: &[(&str, V3)], bounds: Option<(V3, V3)>) -> Part {
        Part {
            id: id.into(),
            name: id.into(),
            source: String::new(),
            role: None,
            role_guessed: false,
            empties: empties.iter().map(|(n, p)| Empty { name: n.to_string(), parent: None, location: *p, rotation: None }).collect(),
            objects: 1,
            meshes: 1,
            tris: 0,
            bounds: bounds.map(|(min, max)| Bounds { min, max }),
            has_thumb: false,
            has_glb: false,
            stamp: String::new(),
            added: 0,
            multi_part_hint: false,
            role_hints: Default::default(),
            missing_textures: Vec::new(),
            lod: None,
            split_by: None,
            split_checked: false,
            node: None,
        }
    }

    #[test]
    fn parts_snap_by_their_empties_and_follow_their_parents() {
        let f = Frames::placeholder();
        let a = f.anchors();
        // A chassis that carries its own steer_axis 2 cm lower than the template's.
        let chassis = part("c", &[("steer_axis", add(a["steer_axis"], [0.0, 0.0, -0.02]))], None);
        // A steer modelled at the origin, with its mount there and a handlebar above it.
        let steer = part("s", &[("steer_axis.001", [0.0; 3]), ("handlebar", [0.0, 0.07, 0.17])], None);
        // A wheel at the origin with no empties: taken by its middle.
        let wheel = part("w", &[], Some(([-0.05, -0.36, -0.36], [0.05, 0.36, 0.36])));
        // Levers built in place, no empties.
        let levers = part("l", &[], None);
        let slots: BTreeMap<Role, &Part> =
            [(Role::Chassis, &chassis), (Role::Steer, &steer), (Role::WheelF, &wheel), (Role::Levers, &levers)].into_iter().collect();

        let got = place(&f, &slots, &BTreeMap::new());
        let by = |r: Role| got.placed.iter().find(|p| p.role == r).unwrap();
        assert_eq!(by(Role::Chassis).offset, [0.0; 3]);
        assert!(close(got.anchors["steer_axis"], add(a["steer_axis"], [0.0, 0.0, -0.02])));
        // The steer lands on the chassis' own head, 2 cm below the template's.
        assert_eq!(by(Role::Steer).by, "empty");
        assert!(close(by(Role::Steer).offset, got.anchors["steer_axis"]));
        assert!(close(got.anchors["handlebar"], add(got.anchors["steer_axis"], [0.0, 0.07, 0.17])));
        // The wheel's middle is on the template's axle.
        assert_eq!(by(Role::WheelF).by, "centre");
        assert!(close(by(Role::WheelF).offset, a["front_axle"]));
        // The levers, built in place, move as far as the handlebar did: down 2 cm.
        assert_eq!(by(Role::Levers).by, "as modelled");
        assert!(close(by(Role::Levers).offset, [0.0, 0.0, -0.02]));

        // A nudge moves the part and what hangs from it.
        let nudges: BTreeMap<Role, V3> = [(Role::Chassis, [0.0, 0.01, 0.0])].into_iter().collect();
        let got = place(&f, &slots, &nudges);
        let by = |r: Role| got.placed.iter().find(|p| p.role == r).unwrap();
        assert!(close(by(Role::Chassis).offset, [0.0, 0.01, 0.0]));
        assert!(close(by(Role::Levers).offset, [0.0, 0.01, -0.02]));
        // The wheel hangs off the fork, which isn't there: it stays on the template's axle.
        assert!(close(by(Role::WheelF).offset, a["front_axle"]));
    }

    #[test]
    fn a_turn_swings_the_part_and_what_hangs_from_it_about_its_joint() {
        // Blender's own Euler XYZ: a quarter turn about Z takes +X to +Y.
        assert!(close(turn_about([1.0, 0.0, 0.0], [0.0; 3], [0.0, 0.0, 90.0]), [0.0, 1.0, 0.0]));
        // X first, then Z: +Y goes to +Z (X), and +Z stays put under Z.
        assert!(close(turn_about([0.0, 1.0, 0.0], [0.0; 3], [90.0, 0.0, 90.0]), [0.0, 0.0, 1.0]));
        assert!(close(turn_about([2.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 90.0]), [1.0, 1.0, 0.0]));

        let f = Frames::placeholder();
        let a = f.anchors();
        let chassis = part("c", &[], None);
        let steer = part("s", &[("steer_axis", [0.0; 3]), ("handlebar", [0.0, 0.0, 0.2])], None);
        let slots: BTreeMap<Role, &Part> = [(Role::Chassis, &chassis), (Role::Steer, &steer)].into_iter().collect();
        let turns: BTreeMap<Role, V3> = [(Role::Steer, [0.0, 0.0, 90.0])].into_iter().collect();
        let got = place_turned(&f, &slots, &BTreeMap::new(), &turns);
        let steer_placed = got.placed.iter().find(|p| p.role == Role::Steer).unwrap();
        assert_eq!(steer_placed.rotation, [0.0, 0.0, 90.0]);
        // It turns about the joint it hangs from, which doesn't move.
        assert!(close(steer_placed.pivot, a["steer_axis"]));
        // Straight above the joint, a turn about Z leaves the handlebar where it was...
        assert!(close(got.anchors["handlebar"], add(a["steer_axis"], [0.0, 0.0, 0.2])));
        // ...while one about X swings it forward with the part.
        let turns: BTreeMap<Role, V3> = [(Role::Steer, [90.0, 0.0, 0.0])].into_iter().collect();
        let got = place_turned(&f, &slots, &BTreeMap::new(), &turns);
        assert!(close(got.anchors["handlebar"], add(a["steer_axis"], [0.0, -0.2, 0.0])));
        // Untouched parts carry no turn.
        assert_eq!(got.placed.iter().find(|p| p.role == Role::Chassis).unwrap().rotation, [0.0; 3]);
    }

    /// Where a kit point lands: `basis·p + offset`.
    fn on_bike(p: &Placed, l: V3) -> V3 {
        add(mat_vec(&p.basis.unwrap_or(IDENTITY), l), p.offset)
    }

    #[test]
    fn a_kit_part_snaps_onto_its_node_on_the_template() {
        let f = Frames::from_geom(CR250.as_bytes()).unwrap();
        let chassis = part("c", &[], None);
        let mut steer = part("s", &[], None);
        steer.node = Some("steer".into());
        let mut fork = part("f", &[], None);
        fork.node = Some("fsusp".into());
        let slots: BTreeMap<Role, &Part> =
            [(Role::Chassis, &chassis), (Role::Steer, &steer), (Role::Fsusp, &fork)].into_iter().collect();
        let got = place(&f, &slots, &BTreeMap::new());
        let by = |r: Role| got.placed.iter().find(|p| p.role == r).unwrap();
        assert_eq!(by(Role::Steer).by, "node");
        assert!(by(Role::Chassis).basis.is_none(), "an ordinary part isn't turned");
        // The kit's node origin is where the .geom puts that node: carried back into the
        // part's own frame it's the origin again (the fork) or the steer's joint offset.
        let into = |g: Group, p: V3| apply(&f.into_part(g), p);
        assert!(close(into(Group::Fsusp, on_bike(by(Role::Fsusp), [0.0; 3])), [0.0; 3]));
        assert!(close(into(Group::Steer, on_bike(by(Role::Steer), [0.0; 3])), [0.0; 3]));
        // And any kit point goes back to itself: the snap is exactly the .geom's placement.
        let l = [0.1, -0.04, -0.6];
        assert!(close(into(Group::Steer, on_bike(by(Role::Steer), l)), l));
        // The steer's own axis (kit +Z) leans back by the rake: towards +Y, the rear.
        let up = sub(on_bike(by(Role::Steer), [0.0, 0.0, 1.0]), on_bike(by(Role::Steer), [0.0; 3]));
        assert!((up[1].atan2(up[2]).to_degrees() - 27.2).abs() < 1e-6, "{up:?}");
        // It turns about its node.
        assert!(close(by(Role::Steer).pivot, by(Role::Steer).offset));
        // A nudge moves it, as any part.
        let nudges: BTreeMap<Role, V3> = [(Role::Fsusp, [0.0, 0.0, 0.01])].into_iter().collect();
        let moved = place(&f, &slots, &nudges);
        let fork_at = |a: &Assembly| a.placed.iter().find(|p| p.role == Role::Fsusp).unwrap().offset;
        assert!(close(sub(fork_at(&moved), fork_at(&got)), [0.0, 0.0, 0.01]));
    }

    #[test]
    fn a_kit_forks_axle_turns_with_the_fork() {
        let f = Frames::from_geom(CR250.as_bytes()).unwrap();
        let chassis = part("c", &[], None);
        // A store fork, modelled in fsusp's node space: its axle 0.8 m down its own legs.
        let mut fork = part("f", &[("front_axle", [0.0, 0.0, -0.8])], None);
        fork.node = Some("fsusp".into());
        let mut bare = part("b", &[], None);
        bare.node = Some("fsusp".into());
        let slots: BTreeMap<Role, &Part> = [(Role::Chassis, &chassis), (Role::Fsusp, &fork)].into_iter().collect();
        let got = place(&f, &slots, &BTreeMap::new());
        let p = got.placed.iter().find(|p| p.role == Role::Fsusp).unwrap();
        // The axle is where the fork's own point is on the bike: raked with the legs, not
        // straight under the fork's origin.
        assert!(close(got.anchors["front_axle"], on_bike(p, [0.0, 0.0, -0.8])), "{:?}", got.anchors["front_axle"]);
        // A kit fork with no axle empty on the template: the template's axle, unchanged.
        let slots: BTreeMap<Role, &Part> = [(Role::Chassis, &chassis), (Role::Fsusp, &bare)].into_iter().collect();
        assert!(close(place(&f, &slots, &BTreeMap::new()).anchors["front_axle"], f.anchors()["front_axle"]));
    }

    #[test]
    fn a_kit_fork_on_a_bike_facing_the_other_way_carries_the_axle_round() {
        let f = Frames::placeholder();
        let mut chassis = part("c", &[], None);
        chassis.source = r"C:\split\ktm\chassis.fbx".into();
        let turn = mat_mul(&[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]], &rows_rx(-26.5));
        chassis.empties = vec![
            crate::bikeparts::Empty { name: "fsusp".into(), parent: None, location: [0.0, 0.40, 0.95], rotation: Some(turn) },
            crate::bikeparts::Empty { name: "rsusp".into(), parent: None, location: [0.0, -0.197, 0.45], rotation: None },
        ];
        let mut fork = part("f", &[], None);
        fork.node = Some("fsusp".into());
        let nodes = BaseNodes::find(Some(&chassis), &[]);
        assert!(nodes.faces_plus_y);
        let slots: BTreeMap<Role, &Part> = [(Role::Chassis, &chassis), (Role::Fsusp, &fork)].into_iter().collect();
        let got = place_on(&f, &nodes, &slots, &BTreeMap::new(), &BTreeMap::new());
        // This bike's front is +Y: the front wheel goes there, ahead of the fork, not at the
        // template's axle behind the swingarm.
        assert!(got.anchors["front_axle"][1] > 0.40, "{:?}", got.anchors["front_axle"]);
        assert!(got.anchors["front_axle"][2] < 0.95);
    }

    fn rows_rx(deg: f64) -> M3 {
        rot_x_m(deg)
    }

    #[test]
    fn a_kit_part_snaps_onto_the_base_bikes_own_node() {
        let f = Frames::placeholder();
        // A bike exported from the game, the way the converter writes it: facing +Y, its
        // nodes' empties turned Ry(180)·Rx(-26.5) (the KTMRM's numbers).
        let turn = mat_mul(&[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]], &rows_rx(-26.5));
        let empty = |n: &str, at: V3, rot: Option<M3>| crate::bikeparts::Empty {
            name: n.into(),
            parent: None,
            location: at,
            rotation: rot,
        };
        let mut chassis = part("c", &[], Some(([-0.25, -0.97, 0.3], [0.25, 0.44, 1.15])));
        chassis.source = r"C:\split\ktm\chassis.fbx".into();
        chassis.empties = vec![empty("chassis", [0.0; 3], None)];
        let mut stock_steer = part("ss", &[], None);
        stock_steer.source = r"C:\split\ktm\steer.fbx".into();
        stock_steer.empties = vec![empty("steer", [0.0, 0.3445, 0.9953], Some(turn))];
        let mut stock_rsusp = part("sr", &[], None);
        stock_rsusp.source = r"C:\split\ktm\rsusp.fbx".into();
        stock_rsusp.empties = vec![empty("rsusp", [0.0, -0.197, 0.45], None)];
        // Somebody else's part with a `steer` empty: not this bike, never read.
        let mut other = part("o", &[], None);
        other.source = r"C:\elsewhere\steer.fbx".into();
        other.empties = vec![empty("steer", [9.0, 9.0, 9.0], None)];
        let library = vec![chassis.clone(), stock_steer.clone(), stock_rsusp.clone(), other];

        let nodes = BaseNodes::find(Some(&chassis), &library);
        assert!(nodes.faces_plus_y);
        assert!((nodes.rake_deg.unwrap() - 26.5).abs() < 1e-6);
        assert!(close(nodes.at[&Group::Steer], [0.0, 0.3445, 0.9953]));

        let mut kit = part("k", &[], None);
        kit.node = Some("steer".into());
        let slots: BTreeMap<Role, &Part> = [(Role::Chassis, &chassis), (Role::Steer, &kit)].into_iter().collect();
        let got = place_on(&f, &nodes, &slots, &BTreeMap::new(), &BTreeMap::new());
        let s = got.placed.iter().find(|p| p.role == Role::Steer).unwrap();
        // Its origin on the stock steer's node...
        assert!(close(on_bike(s, [0.0; 3]), [0.0, 0.3445, 0.9953]));
        // ...its axis leaning back towards -Y, this bike's rear...
        let up = sub(on_bike(s, [0.0, 0.0, 1.0]), s.offset);
        assert!(up[1] < 0.0 && up[2] > 0.0 && (up[1].abs().atan2(up[2]).to_degrees() - 26.5).abs() < 1e-6, "{up:?}");
        // ...its left (kit +X) on the bike's left, which facing +Y is -X.
        assert!(sub(on_bike(s, [1.0, 0.0, 0.0]), s.offset)[0] < -0.99);
        // The same from the empty's turn as from the hand-made Blend's convention (Rx(-27.8)
        // facing -Y): either way the steering axis is read, not the empty's own axes.
        let blend = BaseNodes::find(
            Some(&{
                let mut c = chassis.clone();
                c.empties = vec![empty("steer", [0.0, -0.339, 1.003], Some(rows_rx(-27.8)))];
                c
            }),
            &[],
        );
        assert!(!blend.faces_plus_y);
        assert!((blend.rake_deg.unwrap() - 27.8).abs() < 1e-6);
        // Without the empty's turn (catalogued before Studio kept it), the facing comes from
        // where the steer sits against the swingarm, and the rake from the template.
        let mut old = stock_steer.clone();
        old.empties[0].rotation = None;
        let nodes = BaseNodes::find(Some(&chassis), &[old, stock_rsusp]);
        assert!(nodes.faces_plus_y && nodes.rake_deg.is_none());
        let fr = nodes.frame(&f, Group::Steer);
        assert!(close(fr.origin, [0.0, 0.3445, 0.9953]));
        let up = mat_vec(&fr.basis, [0.0, 0.0, 1.0]);
        assert!((up[1].abs().atan2(up[2]).to_degrees() - f.rake_deg).abs() < 1e-6 && up[1] < 0.0);
        // No base nodes at all: the template's.
        assert_eq!(BaseNodes::find(None, &[]).frame(&f, Group::Fsusp), f.node_frame(Group::Fsusp));
    }

    #[test]
    fn base_anchors_follow_a_game_exported_bikes_nodes() {
        let f = Frames::placeholder();
        let turn = mat_mul(&[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]], &rows_rx(-26.5));
        let empty = |name: &str, location: V3, rotation: Option<M3>| crate::bikeparts::Empty {
            name: name.into(),
            parent: None,
            location,
            rotation,
        };
        let mut chassis = part("c", &[], Some(([-0.25, -0.97, 0.3], [0.25, 0.44, 1.15])));
        chassis.empties = vec![
            empty("chassis", [0.0; 3], None),
            empty("steer", [0.0, 0.3445, 0.9953], Some(turn)),
            empty("fsusp", [0.0, 0.42, 0.92], Some(turn)),
            empty("rsusp", [0.0, -0.197, 0.45], None),
        ];
        let nodes = BaseNodes::find(Some(&chassis), &[]);
        let slots: BTreeMap<Role, &Part> = [(Role::Chassis, &chassis)].into_iter().collect();
        let got = place_on(&f, &nodes, &slots, &BTreeMap::new(), &BTreeMap::new());
        let a = &got.anchors;

        assert!(a["front_axle"][1] > a["steer_axis"][1]);
        assert!(a["steer_axis"][1] > a["swingarm_pivot"][1]);
        assert!(a["swingarm_pivot"][1] > a["rear_axle"][1]);
        assert!(close(a["steer_axis"], [0.0, 0.3445, 0.9953]));
        assert!(close(a["swingarm_pivot"], [0.0, -0.197, 0.45]));
        assert!(a["handlebar"][2] > a["steer_axis"][2]);
        assert!((a["handlebar"][1] - a["steer_axis"][1]).abs() < 0.25);
        // A fork's length below the head, wheel-radius high: not on the ground.
        assert!(a["front_axle"][2] > 0.25 && a["front_axle"][2] < a["steer_axis"][2] - 0.4, "{:?}", a["front_axle"]);
    }

    #[test]
    fn base_anchors_without_base_nodes_are_unchanged() {
        let f = Frames::placeholder();
        assert_eq!(base_anchors(&f, &BaseNodes::default()), f.anchors());
    }

    #[test]
    fn names_and_files() {
        assert_eq!(folder_name("KTM 450 SX-F!"), "ktm_450_sx-f");
        assert_eq!(folder_name("  "), "frost_bike");
        assert!(hrc(Group::Steer).contains("name = steer"));
        assert!(default_gfx("oem_mx").contains("tyres = oem_mx"));
    }
}
