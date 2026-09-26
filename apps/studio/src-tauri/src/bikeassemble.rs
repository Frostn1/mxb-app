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
    /// The anchor it hangs from, and where that is.
    pub mount: Option<String>,
    pub at: Option<V3>,
    /// How the part's own end of the joint was found: "empty" (it has one by the anchor's
    /// name), "centre" (a wheel's middle), or "as modelled" (it was built in place).
    pub by: &'static str,
    pub group: Option<Group>,
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
    let base = template.anchors();
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
        let offset = add(offset, nudge);
        let shift = add(moved, nudge);
        for name in provides {
            let p = match own_empty(part, name) {
                Some(own) => add(own, offset),
                None => add(base[*name], shift),
            };
            anchors.insert(name.to_string(), p);
        }
        placed.push(Placed {
            role,
            part_id: part.id.clone(),
            offset,
            nudge,
            mount: mount.map(str::to_string),
            at,
            by,
            group,
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
            empties: empties.iter().map(|(n, p)| Empty { name: n.to_string(), parent: None, location: *p }).collect(),
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
    fn names_and_files() {
        assert_eq!(folder_name("KTM 450 SX-F!"), "ktm_450_sx-f");
        assert_eq!(folder_name("  "), "frost_bike");
        assert!(hrc(Group::Steer).contains("name = steer"));
        assert!(default_gfx("oem_mx").contains("tyres = oem_mx"));
    }
}
