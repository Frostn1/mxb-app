//! The one boundary around anything the manager sends to the Trash.
//!
//! Paths arrive here in several shapes: the normal MX Bikes user folder, a relocated
//! `mods` tree, and the shadow tree beside either one.  The checks stay lexical so they
//! also protect a target that no longer exists, which is exactly when stale records are
//! most likely to name a parent folder by mistake.

use std::ffi::OsString;
use std::path::{Component, Path};

fn relative_parts(base: &Path, target: &Path) -> Option<Vec<OsString>> {
    let base: Vec<Component<'_>> = base.components().collect();
    let target: Vec<Component<'_>> = target.components().collect();
    if target.iter().any(|c| matches!(c, Component::ParentDir)) || target.len() < base.len() {
        return None;
    }
    let same = base.iter().zip(&target).all(|(a, b)| {
        #[cfg(target_os = "windows")]
        {
            a.as_os_str().to_string_lossy().eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
        }
        #[cfg(not(target_os = "windows"))]
        {
            a == b
        }
    });
    same.then(|| target[base.len()..].iter().map(|c| c.as_os_str().to_owned()).collect())
}

fn protected_under(root: &Path, target: &Path) -> bool {
    let Some(parts) = relative_parts(root, target) else {
        return false;
    };
    if parts.len() < 2 {
        return true;
    }
    parts[0].to_string_lossy().eq_ignore_ascii_case("rider") && parts.len() < 3
}

/// Refuse roots, content-category folders, escapes, and paths outside the configured tree.
pub fn check_target(mods_path: &str, target: &Path) -> anyhow::Result<()> {
    let root = Path::new(mods_path.trim());
    if root.as_os_str().is_empty() {
        anyhow::bail!("no MX Bikes folder configured");
    }
    if target.components().any(|c| matches!(c, Component::ParentDir)) {
        anyhow::bail!("refusing a path that climbs out of the MX Bikes folder");
    }
    if relative_parts(root, target).is_none() {
        anyhow::bail!("refusing to remove a path outside the MX Bikes folder");
    }

    let mods = crate::library::mods_root(mods_path);
    let shadow = crate::modstate::shadow_root(mods_path);
    if crate::docsdir::same_folder(root, target)
        || crate::docsdir::same_folder(&mods, target)
        || crate::docsdir::same_folder(&shadow, target)
        || protected_under(&mods, target)
        || protected_under(&shadow, target)
    {
        anyhow::bail!("refusing to remove a protected MX Bikes folder");
    }
    Ok(())
}

/// Check the target immediately before handing it to the operating system's Trash.
pub fn trash(mods_path: &str, target: &Path) -> anyhow::Result<crate::trashbin::TrashedAt> {
    check_target(mods_path, target)?;
    crate::trashbin::move_to_trash(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_every_root_and_escape_shape() {
        let root = Path::new("C:/Users/rider/Documents/PiBoSo/MX Bikes");
        let mods_path = root.to_string_lossy();
        for target in [
            root.to_path_buf(),
            root.join("mods"),
            root.join("mods/bikes"),
            root.join("mods/tracks"),
            root.join("mods/rider/helmets"),
            root.join("mxbapp_disabled"),
            root.join("mxbapp_disabled/bikes"),
            root.join("mods/bikes/../tracks/user.pkz"),
            Path::new("C:/somewhere-else/user.pkz").to_path_buf(),
        ] {
            assert!(check_target(&mods_path, &target).is_err(), "accepted {}", target.display());
        }
        assert!(check_target(&mods_path, &root.join("mods/bikes/User.pkz")).is_ok());
        assert!(check_target(&mods_path, &root.join("mods/rider/helmets/Airoh.pkz")).is_ok());
    }
}
