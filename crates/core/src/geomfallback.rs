//! Mount points for a bike whose own `.geom` can't be read: the public build's version.
//!
//! The full version (a sibling folder's `.geom`, or mounts read off the model's own shape) is
//! fbx3edf's and is not in this repository. Here a bike without a `.geom` is always assembled
//! with a typical MX bike's mounts, which places the parts within a few cm.

use std::path::Path;

/// Where the mounts a GFX model was assembled with came from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GeomSource {
    /// The bike's own `.geom`, loose or in a readable `.pkz`. Exact.
    Own,
    /// A loose `.geom` in a sibling folder for the same bike. `from` is that folder's name.
    Sibling { from: String },
    /// Read off the model's shape. Approximate.
    Shape { confidence: String, found: Vec<String> },
    /// A typical MX bike's mounts. Approximate.
    Generic,
}

/// A typical MX bike's mounts, written with zero joints.
pub const GENERIC: &str = "chassis_steer = 0, 0.9939, 0.342
chassis_rsusp_min = 0, 0.4577, -0.2012
rakeangle_min = 26.5
steer_joint = 0, 0, 0
rsusp_joint = 0, 0, 0
front_upper = 0, -0.4597, 0.0001
fwheel = 0, -0.333, 0.0664
rwheel_min = 0, -0.001, -0.5513
rwheel_max = 0, -0.0002, -0.5715
seat_height_ref = 0, 0.9419, -0.2322
";

/// The mounts [`crate::edf::assemble_bike`] can't do without.
const NEEDED: &[&str] = &["chassis_steer", "chassis_rsusp_min", "steer_joint", "rsusp_joint", "front_upper"];

/// Whether `bytes` is a `.geom` the parts can be assembled with.
pub fn has_mounts(bytes: &[u8]) -> bool {
    let g = crate::edf::parse_geom(bytes);
    NEEDED.iter().all(|k| g.contains_key(*k))
}

/// The mounts for a bike that has no `.geom` of its own: a typical bike's, in this build.
pub fn fallback_geom(_bike_dir: &Path, _files: &[(String, Vec<u8>)]) -> (Vec<u8>, GeomSource) {
    (GENERIC.as_bytes().to_vec(), GeomSource::Generic)
}

/// A sibling folder's `.geom`: never found in this build.
pub fn sibling_geom(_bike_dir: &Path) -> Option<(Vec<u8>, String)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_has_every_needed_mount() {
        assert!(has_mounts(GENERIC.as_bytes()));
        let (bytes, source) = fallback_geom(Path::new("."), &[]);
        assert_eq!(source, GeomSource::Generic);
        assert!(has_mounts(&bytes));
    }
}
