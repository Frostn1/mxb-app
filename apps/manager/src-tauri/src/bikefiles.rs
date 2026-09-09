//! What the files inside a bike folder are, by name.
//!
//! A bike folder is not one thing. Some of its files are the **mesh** the bike renders
//! with (`model.edf` by convention, but a bike may split its mesh one-EDF-per-part —
//! `96cr250.edf`, `96cr250_st.edf`, … — named by its `.hrc`). Others are the bike's
//! **setup**: the `.hrc`s that say which mesh a part uses, plus `.cfg` and `.geom`.
//! Those belong to the bike itself, not to any one model, and a model swap must leave
//! them alone — moving them is what makes the game stop seeing the bike at all.
//!
//! Shared so the viewer (`gather_bike_files`) and the swapper (`modelswap`) agree.

fn base_name(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase()
}

fn has_ext(name: &str, ext: &str) -> bool {
    base_name(name).ends_with(ext)
}

/// A renderable mesh. Any `.edf` — `model.edf` is the convention, not a rule.
pub fn is_mesh(name: &str) -> bool {
    has_ext(name, ".edf")
}

/// True if the directory holds at least one mesh, i.e. it is a model set.
pub fn dir_has_mesh(dir: &std::path::Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().any(|e| {
            e.path().is_file()
                && e.file_name().to_str().is_some_and(is_mesh)
        }),
        Err(_) => false,
    }
}

/// The bike's own setup, shared by every model: the `.hrc`s naming each part's scene,
/// plus `.cfg` and `.geom`. Never part of a model set.
pub fn is_bike_setup(name: &str) -> bool {
    has_ext(name, ".hrc") || has_ext(name, ".cfg") || has_ext(name, ".geom")
}

/// Everything the viewer needs to read to draw a bike.
pub fn is_viewer_file(name: &str) -> bool {
    is_mesh(name) || is_bike_setup(name) || has_ext(name, ".tga") || has_ext(name, ".pnt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_is_any_edf() {
        assert!(is_mesh("model.edf"));
        assert!(is_mesh("96cr250.edf"));
        assert!(is_mesh("96cr250_st.edf"));
        assert!(is_mesh("MODEL.EDF"));
        assert!(is_mesh("bikes/honda/96cr250.edf"));
        assert!(!is_mesh("chassis.hrc"));
        assert!(!is_mesh("model.edf.bak"));
    }

    #[test]
    fn setup_files_are_not_model_files() {
        for f in ["chassis.hrc", "bike.cfg", "wheel.geom", "STEER.HRC"] {
            assert!(is_bike_setup(f), "{f}");
            assert!(!is_mesh(f), "{f}");
        }
        for f in ["model.edf", "paint.tga", "livery.pnt"] {
            assert!(!is_bike_setup(f), "{f}");
        }
    }

    #[test]
    fn viewer_file_covers_the_old_hardcoded_list() {
        for f in ["a.edf", "a.tga", "a.pnt", "a.geom", "a.cfg", "a.hrc"] {
            assert!(is_viewer_file(f), "{f}");
        }
        assert!(!is_viewer_file("readme.txt"));
        assert!(!is_viewer_file("engine.scl"));
    }
}
