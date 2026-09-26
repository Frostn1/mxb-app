//! The bike builder's part library.
//!
//! A modder brings parts one file at a time; the library remembers each one so it is only
//! run through Blender once. Every part gets a folder of its own under the library root,
//!
//! ```text
//! <app data>/bike-parts/<id>/part.json   what the part is: role, attach empties, size
//!                           /thumb.png   a picture Blender rendered, for the tray
//!                           /part.glb    for the preview to come (phase D)
//! <app data>/bike-parts/slots.json       which part fills each role of the bike
//! ```
//!
//! and nothing is ever written beside the rider's own files. A part's id comes from its
//! path, so adding the same file again refreshes it rather than making a second copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// What a part is on the bike: the pieces MX Bikes puts a bike together from. Each one
/// hangs off a point on its parent, which is phase C's business; here it only decides
/// which slot a part can go in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Chassis,
    Steer,
    Fsusp,
    Rsusp,
    WheelF,
    WheelR,
    Levers,
    Pedals,
    /// Accessories that hang off the steer: made in the Part Maker, or brought like any part.
    Handguards,
    Plate,
}

impl Role {
    pub const ALL: [Role; 10] = [
        Role::Chassis,
        Role::Steer,
        Role::Fsusp,
        Role::Rsusp,
        Role::WheelF,
        Role::WheelR,
        Role::Levers,
        Role::Pedals,
        Role::Handguards,
        Role::Plate,
    ];
}

/// Name fragments that say which role a part plays, most specific first: "rear wheel"
/// must be decided before "wheel", "swingarm" before "arm". Matched against the file name
/// and the object names inside, lowercased with separators squeezed out.
const HINTS: &[(&str, Role)] = &[
    ("rearwheel", Role::WheelR),
    ("rwheel", Role::WheelR),
    ("wheelr", Role::WheelR),
    ("frontwheel", Role::WheelF),
    ("fwheel", Role::WheelF),
    ("wheelf", Role::WheelF),
    ("swingarm", Role::Rsusp),
    ("rsusp", Role::Rsusp),
    ("rearsusp", Role::Rsusp),
    ("shock", Role::Rsusp),
    ("linkage", Role::Rsusp),
    ("fsusp", Role::Fsusp),
    ("frontsusp", Role::Fsusp),
    ("fork", Role::Fsusp),
    ("handguard", Role::Handguards),
    ("brushguard", Role::Handguards),
    ("numberplate", Role::Plate),
    ("frontplate", Role::Plate),
    ("triple", Role::Steer),
    ("handlebar", Role::Steer),
    ("steer", Role::Steer),
    ("clutchlever", Role::Levers),
    ("brakelever", Role::Levers),
    ("gearlever", Role::Levers),
    ("shifter", Role::Levers),
    ("lever", Role::Levers),
    ("footpeg", Role::Pedals),
    ("pedal", Role::Pedals),
    ("peg", Role::Pedals),
    ("chassis", Role::Chassis),
    ("frame", Role::Chassis),
];

fn squeeze(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn first_hint(name: &str) -> Option<Role> {
    let s = squeeze(name);
    HINTS.iter().find(|(k, _)| s.contains(k)).map(|(_, r)| *r)
}

/// The role a part's names suggest, or `None` when they say nothing. The file name is
/// asked first, since a modder names the file for what it is and the objects inside are
/// often whatever the source model called them; then the heaviest hint among the objects —
/// weighted by each object's triangle count, not just counted once each, so four tiny lever
/// meshes in a whole-bike import can't outvote the one big chassis mesh sitting next to them
/// (a real case: a full bike FBX with `brake_lever`/`clutch_lever`/`gear_lever`/`rearbrake_lever`
/// all matching "lever" was guessed as levers over its one `chassis` object). A weight of zero
/// (an object with no triangle count, e.g. an empty) still counts for one, so a hint from a
/// non-mesh object isn't thrown away.
pub fn guess_role<'a>(file_stem: &str, objects: impl IntoIterator<Item = (&'a str, u64)>) -> Option<Role> {
    if let Some(r) = first_hint(file_stem) {
        return Some(r);
    }
    let mut votes: BTreeMap<Role, u64> = BTreeMap::new();
    for (name, weight) in objects {
        if let Some(r) = first_hint(name) {
            *votes.entry(r).or_default() += weight.max(1);
        }
    }
    // Ties go to the role listed first in `Role::ALL`, so the answer never depends on order.
    votes.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(r, _)| r)
}

/// How many *distinct* roles the objects' names hint at, ignoring the file name and ignoring
/// weight — three or more is a full bike (chassis, suspension, wheels…) brought in as one
/// file, not a single part, whatever role the file name or the vote above settled on. Kept
/// separate from [`guess_role`] because a rider who deliberately names a whole-bike proxy
/// "chassis" should still get that role; this only flags the file for a second look.
///
/// Callers must pass mesh names only, not empties: a single ordinary chassis brings attach
/// points named for what they snap to (`steer_axis`, `swingarm_pivot`…), which hint at
/// "steer" and "rsusp" as loudly as a real steer or swingarm mesh would — a well-modelled
/// single part would flag itself as a whole bike if those were counted in.
pub fn multi_part_hint<'a>(objects: impl IntoIterator<Item = &'a str>) -> bool {
    role_hints(objects).len() >= 3
}

/// How many mesh objects hinted at each role — the same per-name hints [`multi_part_hint`]
/// collapses to a yes/no, kept broken down instead so the tray can say *what* it found
/// ("chassis · steer · fsusp · rsusp · levers ×4") before the rider decides to split it or
/// use it whole as a base bike.
pub fn role_hints<'a>(objects: impl IntoIterator<Item = &'a str>) -> BTreeMap<Role, usize> {
    let mut hints: BTreeMap<Role, usize> = BTreeMap::new();
    for name in objects {
        if let Some(role) = first_hint(name) {
            *hints.entry(role).or_default() += 1;
        }
    }
    hints
}

/// An attach point a part brings with it, where it sits in Blender's world (Z up).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Empty {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    pub location: [f64; 3],
}

/// The sidecar: what the library knows of one part.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub id: String,
    /// The file name without its extension, as the tray shows it.
    pub name: String,
    /// The rider's file, read from and never written to.
    pub source: String,
    pub role: Option<Role>,
    /// Whether `role` is the library's guess rather than the rider's choice. A guess is
    /// made again when the part is refreshed; a choice is kept.
    #[serde(default)]
    pub role_guessed: bool,
    pub empties: Vec<Empty>,
    pub objects: usize,
    pub meshes: usize,
    pub tris: u64,
    pub bounds: Option<Bounds>,
    pub has_thumb: bool,
    pub has_glb: bool,
    /// The source's size and modified time when it was catalogued, so a changed file shows.
    pub stamp: String,
    /// Unix seconds.
    pub added: u64,
    /// The object names inside hint at three or more different roles — this file is probably
    /// a whole bike (or a big sub-assembly), not the one part `role` says. Studio still picks
    /// its best single guess so the part isn't left unusable, but the tray shows this so the
    /// rider knows to check it rather than trust it.
    #[serde(default)]
    pub multi_part_hint: bool,
    /// What [`multi_part_hint`] found, broken down: how many objects hinted at each role.
    /// Only meaningful alongside `multi_part_hint` — a part that doesn't trip it may still
    /// carry a hint or two, which isn't worth showing.
    #[serde(default)]
    pub role_hints: BTreeMap<Role, usize>,
    /// Texture files the part needs that couldn't be found.
    #[serde(default)]
    pub missing_textures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// A part's id: the first 16 hex of the SHA-256 of its path, case folded on Windows where
/// the file system is too.
pub fn part_id(source: &Path) -> String {
    part_id_tagged(source, None)
}

/// [`part_id`], with a tag folded in — for a part that shares its source file with others,
/// which a plain path-derived id can't tell apart. A whole-bike split (`Library::add_tagged`)
/// is the only caller with a tag: each of its groups comes from the *same* file, so without
/// this they'd all hash to one id and overwrite each other in the library.
pub fn part_id_tagged(source: &Path, tag: Option<&str>) -> String {
    let mut s = source.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    if let Some(t) = tag {
        s.push('#');
        s.push_str(t);
    }
    let digest = Sha256::digest(s.as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Size and modified time of a file, as one string that changes when the file does.
pub fn file_stamp(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return String::new();
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}:{mtime}", meta.len())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// An id is 16 hex and nothing else, so one handed in from the UI can't point outside the library.
fn check_id(id: &str) -> anyhow::Result<()> {
    if id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        anyhow::bail!("not a part id: {id:?}")
    }
}

/// The library under one root folder.
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn part_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Every part the library holds, newest first. A folder whose sidecar won't read is
    /// skipped, not an error: one bad part must not empty the tray.
    pub fn list(&self) -> Vec<Part> {
        let mut parts: Vec<Part> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let bytes = std::fs::read(e.path().join("part.json")).ok()?;
                serde_json::from_slice::<Part>(&bytes).ok()
            })
            .collect();
        parts.sort_by(|a, b| b.added.cmp(&a.added).then(a.name.cmp(&b.name)));
        parts
    }

    pub fn get(&self, id: &str) -> anyhow::Result<Part> {
        check_id(id)?;
        let bytes = std::fs::read(self.part_dir(id).join("part.json"))
            .map_err(|_| anyhow::anyhow!("that part isn't in the library any more"))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn save(&self, part: &Part) -> anyhow::Result<()> {
        let dir = self.part_dir(&part.id);
        std::fs::create_dir_all(&dir)?;
        write_atomic(&dir.join("part.json"), &serde_json::to_vec_pretty(part)?)
    }

    /// Put a catalogued part in the library. `answer` is what `frost_bike.py`'s `catalog`
    /// op wrote; its thumbnail and GLB, if any, are moved in from the job's folder.
    ///
    /// `stamp` is the source's [`file_stamp`] from before Blender read it, so a file saved
    /// while the job ran still shows as changed. Re-adding a part keeps a role the rider
    /// chose and its place in the list.
    pub fn add(&self, source: &Path, answer: &serde_json::Value, stamp: String) -> anyhow::Result<Part> {
        self.add_as(source, answer, stamp, None)
    }

    /// [`Library::add`], with the role said rather than guessed: Studio's own parts (the
    /// placeholder bike, the Part Maker's) know what they are.
    pub fn add_as(
        &self,
        source: &Path,
        answer: &serde_json::Value,
        stamp: String,
        known: Option<Role>,
    ) -> anyhow::Result<Part> {
        let id = part_id(source);
        let before = self.get(&id).ok();
        let objects = answer["objects"].as_array().cloned().unwrap_or_default();
        let weighted: Vec<(&str, u64)> = objects
            .iter()
            .filter_map(|o| Some((o["name"].as_str()?, o["tris"].as_u64().unwrap_or(0))))
            .collect();
        let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (role, role_guessed) = match &before {
            _ if known.is_some() => (known, false),
            Some(p) if !p.role_guessed => (p.role, false),
            _ => (guess_role(&stem, weighted.iter().copied()), true),
        };
        // Mesh names only, for the whole-bike check: an ordinary chassis brings attach
        // empties named for what they snap to (`steer_axis`, `swingarm_pivot`…), and those
        // hint at "steer" and "rsusp" just as loudly as a real steer or swingarm mesh would.
        // Counted in, a single well-modelled chassis part would flag itself as a whole bike.
        let mesh_names: Vec<&str> =
            objects.iter().filter(|o| o["type"] == "MESH").filter_map(|o| o["name"].as_str()).collect();
        let hints = role_hints(mesh_names.iter().copied());
        self.finish(id, stem, source, role, role_guessed, hints, answer, stamp, before)
    }

    /// A group `frost_bike.py`'s "split" op cut a whole bike into — `role` is whatever
    /// Blender's own per-object grouping decided for it (`None` for its leftover
    /// "unassigned" bucket), taken as given rather than run back through [`guess_role`]'s
    /// file-wide vote, which is exactly what splitting a whole bike is trying to get away
    /// from. `tag` (the group's slug — "chassis", "part-2"…) becomes part of its id: every
    /// group in a split shares one source file, so the plain path-derived id would collide
    /// between them.
    /// `source` is this group's own exported FBX (`op_split` writes one per group), not the
    /// whole-bike file it was cut from: a later build imports a part's `source` fresh, and
    /// that has to be just this group's geometry. `origin_name` is the whole bike's own
    /// name, kept for the tray's sake — "KTMRM — chassis" reads better than the group's own
    /// file stem, which is the tag again.
    pub fn add_split_group(
        &self,
        origin_name: &str,
        source: &Path,
        tag: &str,
        role: Option<Role>,
        answer: &serde_json::Value,
        stamp: String,
    ) -> anyhow::Result<Part> {
        let id = part_id_tagged(source, Some(tag));
        let before = self.get(&id).ok();
        // Not run through `role_hints`: a split group is already as split as Blender's
        // grouping could make it, so flagging it again would just repeat the same warning
        // the split was the answer to.
        self.finish(id, format!("{origin_name} — {tag}"), source, role, false, BTreeMap::new(), answer, stamp, before)
    }

    /// The tail [`add_as`] and [`add_split_group`] share: stage the new thumbnail/GLB in,
    /// build the sidecar, commit both together, and drop the part from any slot it no
    /// longer fits now that its role has (possibly) changed.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        id: String,
        name: String,
        source: &Path,
        role: Option<Role>,
        role_guessed: bool,
        role_hints: BTreeMap<Role, usize>,
        answer: &serde_json::Value,
        stamp: String,
        before: Option<Part>,
    ) -> anyhow::Result<Part> {
        let dir = self.part_dir(&id);
        std::fs::create_dir_all(&dir)?;
        let objects = answer["objects"].as_array().cloned().unwrap_or_default();

        let mut missing_textures: Vec<String> = objects
            .iter()
            .filter_map(|o| o["missingTextures"].as_array())
            .flatten()
            .filter_map(|texture| texture.as_str().map(str::to_owned))
            .collect();
        missing_textures.sort();
        missing_textures.dedup();

        // Both new files are staged beside the old ones before either is replaced, so a copy
        // that fails leaves the part as it was and says so, rather than losing its picture.
        let mut staged = Vec::new();
        for (key, into) in [("thumb", "thumb.png"), ("glb", "part.glb")] {
            if let Some(from) = answer[key].as_str() {
                let next = dir.join(format!("{into}.new"));
                if let Err(e) = move_file(Path::new(from), &next) {
                    for (n, _) in &staged {
                        let _ = std::fs::remove_file(n);
                    }
                    let _ = std::fs::remove_file(&next);
                    anyhow::bail!("couldn't keep the part's {into}: {e}");
                }
                staged.push((next, dir.join(into)));
            }
        }
        let has_thumb = answer["thumb"].is_string();
        let has_glb = answer["glb"].is_string();

        let part = Part {
            id: id.clone(),
            name,
            source: source.to_string_lossy().into_owned(),
            role,
            role_guessed,
            empties: serde_json::from_value(answer["empties"].clone()).unwrap_or_default(),
            objects: objects.len(),
            meshes: objects.iter().filter(|o| o["type"] == "MESH").count(),
            tris: answer["tris"].as_u64().unwrap_or(0),
            bounds: serde_json::from_value(answer["bounds"].clone()).ok(),
            has_thumb,
            has_glb,
            stamp,
            added: before.as_ref().map(|p| p.added).unwrap_or_else(now_secs),
            multi_part_hint: role_hints.len() >= 3,
            role_hints,
            missing_textures,
        };
        self.commit(&part, staged)?;
        // A fresh guess can say something else: then the part leaves the slot it no longer fits.
        if before.is_some_and(|p| p.role != part.role) {
            let mut slots = self.slots();
            slots.retain(|_, v| *v != id);
            self.save_slots(&slots)?;
        }
        Ok(part)
    }

    /// Swap a part's files for a new generation all together: the old thumbnail and GLB
    /// step aside, the staged ones and the sidecar go in, and on any failure the old ones
    /// come back. A part never ends up with one generation's picture and another's sidecar.
    fn commit(&self, part: &Part, staged: Vec<(PathBuf, PathBuf)>) -> anyhow::Result<()> {
        let dir = self.part_dir(&part.id);
        let assets = ["thumb.png", "part.glb"].map(|n| dir.join(n));
        let aside = |p: &Path| p.with_extension(format!("{}.old", p.extension().unwrap_or_default().to_string_lossy()));
        let mut moved = Vec::new();
        let result = (|| -> anyhow::Result<()> {
            for a in &assets {
                if a.exists() {
                    std::fs::rename(a, aside(a))?;
                    moved.push(a.clone());
                }
            }
            for (next, target) in &staged {
                std::fs::rename(next, target)?;
            }
            self.save(part)
        })();
        match result {
            Ok(()) => {
                for a in &moved {
                    let _ = std::fs::remove_file(aside(a));
                }
                Ok(())
            }
            Err(e) => {
                for a in &assets {
                    let _ = std::fs::remove_file(a);
                }
                for a in &moved {
                    let _ = std::fs::rename(aside(a), a);
                }
                for (next, _) in &staged {
                    let _ = std::fs::remove_file(next);
                }
                Err(e)
            }
        }
    }

    /// The rider says what a part is; `None` clears it. A slot the part filled under its
    /// old role is emptied, since it no longer fits there.
    pub fn set_role(&self, id: &str, role: Option<Role>) -> anyhow::Result<Part> {
        let mut part = self.get(id)?;
        if part.role != role {
            let mut slots = self.slots();
            slots.retain(|_, v| v != id);
            self.save_slots(&slots)?;
        }
        part.role = role;
        part.role_guessed = false;
        self.save(&part)?;
        Ok(part)
    }

    /// Take a part out of the library. The rider's file is not touched.
    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        check_id(id)?;
        let mut slots = self.slots();
        if slots.values().any(|v| v == id) {
            slots.retain(|_, v| v != id);
            self.save_slots(&slots)?;
        }
        let dir = self.part_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Which part fills each role. A slot naming a part that has gone is left out.
    pub fn slots(&self) -> BTreeMap<Role, String> {
        let mut slots: BTreeMap<Role, String> = std::fs::read(self.root.join("slots.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        slots.retain(|_, id| check_id(id).is_ok() && self.part_dir(id).join("part.json").is_file());
        slots
    }

    fn save_slots(&self, slots: &BTreeMap<Role, String>) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        write_atomic(&self.root.join("slots.json"), &serde_json::to_vec_pretty(slots)?)
    }

    /// Put a part in a role's slot, or empty the slot with `None`. The part has to be one
    /// of that role: a wheel can't be the chassis.
    pub fn set_slot(&self, role: Role, id: Option<&str>) -> anyhow::Result<BTreeMap<Role, String>> {
        let mut slots = self.slots();
        match id {
            Some(id) => {
                let part = self.get(id)?;
                if part.role != Some(role) {
                    anyhow::bail!("{} isn't a {:?} part", part.name, role);
                }
                slots.insert(role, id.to_string());
            }
            None => {
                slots.remove(&role);
            }
        }
        self.save_slots(&slots)?;
        Ok(slots)
    }
}

/// Write through a temporary file and rename, so a crash mid-write leaves the old file.
fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Rename when the job folder is on the same drive as the library, copy when it isn't.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `guess_role` with no per-object weight: every hint counts once, as when nothing is
    /// known of an object's size (an `.obj`'s vertex-only names, or a test that isn't about
    /// weighting at all).
    fn unweighted<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<(&'a str, u64)> {
        names.into_iter().map(|n| (n, 1)).collect()
    }

    #[test]
    fn roles_are_guessed_from_names() {
        assert_eq!(guess_role("KTM_rear_wheel", []), Some(Role::WheelR));
        assert_eq!(guess_role("front-wheel", []), Some(Role::WheelF));
        assert_eq!(guess_role("Swingarm v2", []), Some(Role::Rsusp));
        assert_eq!(guess_role("forks", []), Some(Role::Fsusp));
        assert_eq!(guess_role("handlebars_renthal", []), Some(Role::Steer));
        assert_eq!(guess_role("clutch lever", []), Some(Role::Levers));
        assert_eq!(guess_role("footpegs", []), Some(Role::Pedals));
        assert_eq!(guess_role("frame_450", []), Some(Role::Chassis));
        assert_eq!(guess_role("KTM handguards", []), Some(Role::Handguards));
        assert_eq!(guess_role("front number plate", []), Some(Role::Plate));
        // The file says nothing: the objects vote.
        assert_eq!(guess_role("part01", unweighted(["fork_l", "fork_r", "axle"])), Some(Role::Fsusp));
        assert_eq!(guess_role("part01", unweighted(["Cube", "Cube.001"])), None);
        // The file name wins over what's inside.
        assert_eq!(guess_role("chassis", unweighted(["fork_l", "fork_r"])), Some(Role::Chassis));
        // A tie is settled the same way every time.
        assert_eq!(guess_role("x", unweighted(["fork", "frame"])), Some(Role::Chassis));
        assert_eq!(guess_role("x", unweighted(["frame", "fork"])), Some(Role::Chassis));
    }

    #[test]
    fn a_heavier_object_outvotes_a_crowd_of_light_ones() {
        // The real case: a full bike FBX with one big `chassis` mesh and four small lever
        // meshes used to be guessed "levers" on a flat count of 4 to 1. Weighted by size, the
        // chassis wins.
        let objects = [("chassis", 40_000), ("brake_lever", 300), ("clutch_lever", 280), ("gear_lever", 260), ("rearbrake_lever", 240)];
        assert_eq!(guess_role("KTMRM", objects), Some(Role::Chassis));
        // Without the weighting it would have been the crowd: kept as a regression marker.
        assert_eq!(guess_role("KTMRM", unweighted(objects.iter().map(|(n, _)| *n))), Some(Role::Levers));
    }

    #[test]
    fn three_or_more_roles_hint_at_a_whole_bike() {
        assert!(multi_part_hint(["chassis", "steer", "fsusp", "brake_lever"]));
        assert!(!multi_part_hint(["fork_l", "fork_r", "axle"]), "one role, however many objects");
        assert!(!multi_part_hint(["Cube", "Cube.001"]), "no hints at all");
    }

    #[test]
    fn role_hints_break_down_what_multi_part_hint_only_flags() {
        let hints = role_hints(["chassis", "steer", "brake_lever", "clutch_lever"]);
        assert_eq!(hints.get(&Role::Chassis), Some(&1));
        assert_eq!(hints.get(&Role::Steer), Some(&1));
        assert_eq!(hints.get(&Role::Levers), Some(&2), "both levers count towards the one role");
        assert_eq!(hints.len(), 3);
        assert!(role_hints(["Cube", "Cube.001"]).is_empty());
    }

    #[test]
    fn roles_read_and_write_the_way_the_ui_names_them() {
        assert_eq!(serde_json::to_string(&Role::WheelF).unwrap(), "\"wheel_f\"");
        assert_eq!(serde_json::from_str::<Role>("\"rsusp\"").unwrap(), Role::Rsusp);
    }

    #[test]
    fn ids_are_stable_and_safe() {
        let a = part_id(Path::new(r"C:\Parts\Fork.blend"));
        assert_eq!(a.len(), 16);
        assert_eq!(a, part_id(Path::new(r"C:\Parts\Fork.blend")));
        assert_ne!(a, part_id(Path::new(r"C:\Parts\Frame.blend")));
        if cfg!(windows) {
            assert_eq!(a, part_id(Path::new(r"c:/parts/fork.BLEND")));
        }
        assert!(check_id(&a).is_ok());
        assert!(check_id("../../etc").is_err());
        assert!(check_id("").is_err());
    }

    fn tmp_lib(tag: &str) -> (PathBuf, Library) {
        let root = std::env::temp_dir().join(format!("frost-bikeparts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        (root.clone(), Library::new(root.join("lib")))
    }

    fn answer(work: &Path, objs: &[(&str, &str)]) -> serde_json::Value {
        std::fs::create_dir_all(work).unwrap();
        std::fs::write(work.join("t.png"), b"png").unwrap();
        serde_json::json!({
            "objects": objs.iter().map(|(n, t)| serde_json::json!({"name": n, "type": t})).collect::<Vec<_>>(),
            "empties": [{"name": "axle", "parent": null, "location": [0.0, 0.7, 0.3]}],
            "bounds": {"min": [0.0, 0.0, 0.0], "max": [0.1, 0.8, 0.6]},
            "tris": 1200,
            "thumb": work.join("t.png"),
        })
    }

    #[test]
    fn an_ordinary_chassis_does_not_flag_itself_as_a_whole_bike() {
        // The chassis' own attach empties are named for what they snap to — "steer_axis",
        // "swingarm_pivot" — which hint at "steer" and "rsusp" just as loudly as a real steer
        // or swingarm mesh would. `multi_part_hint` must look at the mesh only, or a single,
        // correctly modelled chassis part flags itself as a whole bike on its own empties.
        let (root, lib) = tmp_lib("chassis-empties");
        let src = root.join("chassis.fbx");
        std::fs::write(&src, b"fbx").unwrap();
        let objs = [("chassis", "MESH"), ("steer_axis", "EMPTY"), ("swingarm_pivot", "EMPTY")];
        let p = lib.add(&src, &answer(&root.join("job"), &objs), file_stamp(&src)).unwrap();
        assert_eq!(p.role, Some(Role::Chassis));
        assert!(!p.multi_part_hint);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn split_groups_from_one_file_get_distinct_ids_and_names() {
        let (root, lib) = tmp_lib("split");
        let src = root.join("KTMRM.fbx");
        std::fs::write(&src, b"fbx").unwrap();

        let chassis = lib
            .add_split_group(
                "KTMRM",
                &src,
                "chassis",
                Some(Role::Chassis),
                &answer(&root.join("j1"), &[("chassis", "MESH")]),
                file_stamp(&src),
            )
            .unwrap();
        let levers = lib
            .add_split_group(
                "KTMRM",
                &src,
                "levers",
                Some(Role::Levers),
                &answer(&root.join("j2"), &[("brake_lever", "MESH")]),
                file_stamp(&src),
            )
            .unwrap();
        let leftover = lib
            .add_split_group(
                "KTMRM",
                &src,
                "part-3",
                None,
                &answer(&root.join("j3"), &[("Cube", "MESH")]),
                file_stamp(&src),
            )
            .unwrap();

        // Same source file, three distinct ids: the plain path-derived id would have
        // collided every one of these into the same part.
        assert_ne!(chassis.id, levers.id);
        assert_ne!(levers.id, leftover.id);
        assert_eq!(chassis.name, "KTMRM — chassis");
        assert_eq!((chassis.role, chassis.role_guessed), (Some(Role::Chassis), false));
        assert_eq!((leftover.role, leftover.role_guessed), (None, false));
        // A split group isn't re-flagged as a whole bike itself.
        assert!(!chassis.multi_part_hint && !levers.multi_part_hint && !leftover.multi_part_hint);
        assert_eq!(lib.list().len(), 3, "three parts in the tray, not one");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_part_is_added_refreshed_and_removed() {
        let (root, lib) = tmp_lib("add");
        let src = root.join("my fork.fbx");
        std::fs::write(&src, b"fbx").unwrap();

        let p = lib.add(&src, &answer(&root.join("job1"), &[("fork_l", "MESH"), ("axle", "EMPTY")]), file_stamp(&src)).unwrap();
        assert_eq!(p.name, "my fork");
        assert_eq!((p.role, p.role_guessed), (Some(Role::Fsusp), true));
        assert_eq!((p.objects, p.meshes, p.tris), (2, 1, 1200));
        assert_eq!(p.empties[0].name, "axle");
        assert!(p.has_thumb && lib.part_dir(&p.id).join("thumb.png").is_file());
        assert!(!root.join("job1").join("t.png").exists(), "the thumbnail moved in");
        assert!(!p.has_glb);
        assert_eq!(lib.list(), vec![p.clone()]);

        // The rider says it's the steer; a refresh keeps that and the id.
        lib.set_role(&p.id, Some(Role::Steer)).unwrap();
        let again = lib.add(&src, &answer(&root.join("job2"), &[("fork_l", "MESH")]), file_stamp(&src)).unwrap();
        assert_eq!(again.id, p.id);
        assert_eq!((again.role, again.role_guessed), (Some(Role::Steer), false));
        assert_eq!(again.added, p.added);
        assert_eq!(lib.list().len(), 1, "one part, not two");
        let dir = lib.part_dir(&p.id);
        let names = |d: &Path| {
            let mut n: Vec<String> = std::fs::read_dir(d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            n.sort();
            n
        };
        assert_eq!(names(&dir), ["part.json", "thumb.png"], "no staged or set-aside files left");

        // A refresh whose picture didn't render drops the old one rather than keep a stale picture.
        let bare = serde_json::json!({ "objects": [], "empties": [], "bounds": null, "tris": 0 });
        let third = lib.add(&src, &bare, file_stamp(&src)).unwrap();
        assert!(!third.has_thumb);
        assert_eq!(names(&dir), ["part.json"]);

        lib.remove(&p.id).unwrap();
        assert!(lib.list().is_empty());
        assert!(src.is_file(), "the rider's file stays");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn slots_take_only_parts_of_their_role() {
        let (root, lib) = tmp_lib("slots");
        let fork_src = root.join("fork.obj");
        let frame_src = root.join("frame.obj");
        std::fs::write(&fork_src, b"o").unwrap();
        std::fs::write(&frame_src, b"o").unwrap();
        let fork = lib.add(&fork_src, &answer(&root.join("j1"), &[]), file_stamp(&fork_src)).unwrap();
        let frame = lib.add(&frame_src, &answer(&root.join("j2"), &[]), file_stamp(&frame_src)).unwrap();

        assert!(lib.set_slot(Role::Chassis, Some(&fork.id)).is_err(), "a fork isn't a chassis");
        lib.set_slot(Role::Chassis, Some(&frame.id)).unwrap();
        lib.set_slot(Role::Fsusp, Some(&fork.id)).unwrap();
        assert_eq!(lib.slots().len(), 2);

        // Changing a part's role takes it out of the slot it no longer fits.
        lib.set_role(&fork.id, Some(Role::Steer)).unwrap();
        assert_eq!(lib.slots().get(&Role::Fsusp), None);
        // Removing a part empties its slot.
        lib.remove(&frame.id).unwrap();
        assert!(lib.slots().is_empty());
        lib.set_slot(Role::Steer, Some(&fork.id)).unwrap();
        lib.set_slot(Role::Steer, None).unwrap();
        assert!(lib.slots().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_broken_sidecar_does_not_empty_the_tray() {
        let (root, lib) = tmp_lib("broken");
        let src = root.join("peg.obj");
        std::fs::write(&src, b"o").unwrap();
        lib.add(&src, &answer(&root.join("j"), &[]), file_stamp(&src)).unwrap();
        let bad = root.join("lib").join("0123456789abcdef");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("part.json"), b"{not json").unwrap();
        assert_eq!(lib.list().len(), 1);
        assert!(lib.get("0123456789abcdef").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
