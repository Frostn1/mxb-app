//! Walking content folders a player has stitched together with links.
//!
//! A mods tree is not always a plain tree. One set of paints shared across six rider
//! models is six `…/riders/<model>/paints` entries pointing at a single folder that lives
//! somewhere else — a directory junction on Windows, a symlink on Linux and macOS. Both
//! are *links*, and a link is exactly what a directory entry reports itself as:
//! `DirEntry::file_type().is_dir()` is **false** for a junction, and `WalkDir` won't
//! descend through one unless it's told to. So every scanner that asked the directory
//! *entry* what it was walked straight past the shared folder, and its paints were
//! invisible in the app while the game loaded them perfectly well.
//!
//! Asking the *path* instead — `Path::is_dir`, `read_dir` — follows the link and gets the
//! answer the player expects. The helpers here are the following-side versions of the
//! walks the content scanners do, so the app sees the same tree the game does.
//!
//! Deliberately scoped to content already sitting on the player's disk. Freshly extracted
//! archives stay on the *non*-following side (see `install::purge_escapees`): a link
//! inside a downloaded `.zip` is a stranger's idea of where our files should go, and
//! following one is how an archive writes outside the folder it was extracted into.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A recursive walk over the player's own content that descends through directory links.
///
/// Cycles are walkdir's problem rather than ours: with `follow_links` on, a link that
/// points at one of its own ancestors is reported as an `Err` for that entry instead of
/// being followed round forever — and every caller already drops walk errors. Entry types
/// come from the link's *target*, so `file_type().is_dir()` finally answers the question
/// callers were actually asking.
pub fn walk(dir: &Path) -> WalkDir {
    WalkDir::new(dir).follow_links(true)
}

/// [`walk`], stopping `max_depth` levels below `dir`.
pub fn walk_depth(dir: &Path, max_depth: usize) -> WalkDir {
    walk(dir).max_depth(max_depth)
}

/// Whether `path` is itself a link: a symlink, or a junction on Windows.
///
/// `symlink_metadata` is the point — plain `metadata` resolves the link and would answer
/// about the target.
pub fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// The immediate children of `dir` that are directories, links to directories included.
/// Sorted, so a scan of the same folder always comes out in the same order.
pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    children(dir, |p| p.is_dir())
}

/// The immediate children of `dir` that are files, links to files included.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    children(dir, |p| p.is_file())
}

fn children(dir: &Path, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| keep(p))
        .collect();
    out.sort();
    out
}

/// Directory links under `root`, as `(link, target)` pairs — the shape a caller needs to
/// treat the far end as if it sat where the link does.
///
/// Bounded twice over, because this runs against a folder whose size is the player's
/// business: `max_depth` keeps it out of a track's interior (the links that matter are
/// shallow — `bikes/<bike>/paints`, `rider/riders/<model>/paints`), and `cap` stops a
/// pathological tree from turning into thousands of watch registrations.
///
/// Links pointing back inside `root` are left out: whatever is at the far end is already
/// part of the tree being walked, so treating it as external would double-count it.
pub fn dir_links(root: &Path, max_depth: usize, cap: usize) -> Vec<(PathBuf, PathBuf)> {
    let root_real = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut out = Vec::new();
    // Not `walk`: this one is looking *for* the links, so it must see them rather than
    // transparently step through them.
    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let link = entry.path();
        if !is_link(link) {
            continue;
        }
        // A link to a file, or one whose target has since gone away.
        if !link.is_dir() {
            continue;
        }
        let Ok(target) = std::fs::canonicalize(link) else {
            continue;
        };
        if target.starts_with(&root_real) {
            continue;
        }
        out.push((link.to_path_buf(), target));
        if out.len() >= cap {
            log::warn!(
                "more than {cap} directory links under {} — ignoring the rest",
                root.display()
            );
            break;
        }
    }
    out
}

/// Copy `src` to `dst`, materialising directory links into real folders.
///
/// The point of the copy decides this: what's being built is a bundle for someone else's
/// machine, where the far end of a junction doesn't exist. Copying the link itself (or,
/// as the entry-type version did, handing a directory to `fs::copy` and failing the whole
/// bundle) ships a preset with a hole in it.
///
/// Self-referential links are the hazard that comes with following them, so the chain of
/// directories currently open is tracked and one that reappears is skipped rather than
/// descended into again.
pub fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut open = HashSet::new();
    copy_tree_inner(src, dst, &mut open)
}

fn copy_tree_inner(src: &Path, dst: &Path, open: &mut HashSet<PathBuf>) -> std::io::Result<()> {
    // The identity of a folder is where it really is, not the path we reached it by —
    // otherwise a two-link cycle looks like two different folders every time round.
    let id = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    if !open.insert(id.clone()) {
        log::warn!("skipping {} — a link points back into a folder already being copied", src.display());
        return Ok(());
    }

    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        // `from.is_dir()`, not `entry.file_type()`: a junction is a directory to everyone
        // except the directory entry describing it.
        if from.is_dir() {
            copy_tree_inner(&from, &to, open)?;
        } else if from.is_file() {
            std::fs::copy(&from, &to)?;
        }
        // Anything else — a broken link, a device node — is not content and is left out.
    }

    open.remove(&id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-platform in principle, unix-only in practice: making a link on Windows needs
    /// either developer mode or `mklink /J`, neither of which a test run can assume.
    #[cfg(unix)]
    fn link_dir(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).expect("make a directory link");
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-linkwalk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// The reported bug in miniature: one shared folder, several models pointing at it.
    #[cfg(unix)]
    #[test]
    fn walks_into_a_linked_folder() {
        let root = tmp("walk");
        let shared = root.join("Shared Paints");
        touch(&shared.join("Frost.pnt"));

        let tree = root.join("mods/rider/riders");
        std::fs::create_dir_all(tree.join("Male")).unwrap();
        std::fs::create_dir_all(tree.join("Female")).unwrap();
        link_dir(&shared, &tree.join("Male/paints"));
        link_dir(&shared, &tree.join("Female/paints"));

        let found: Vec<String> = walk(&tree)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.path().strip_prefix(&tree).ok().map(|p| p.to_string_lossy().into_owned()))
            .collect();
        assert!(found.contains(&"Male/paints/Frost.pnt".to_string()), "{found:?}");
        assert!(found.contains(&"Female/paints/Frost.pnt".to_string()), "{found:?}");

        // And the plain walk is what missed them — the regression this guards.
        let plain = WalkDir::new(&tree)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count();
        assert_eq!(plain, 0, "a non-following walk sees nothing behind the link");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A link pointing at one of its own ancestors must end the walk, not run forever.
    #[cfg(unix)]
    #[test]
    fn a_link_that_points_at_its_own_parent_terminates() {
        let root = tmp("cycle");
        let tree = root.join("bikes/KTM");
        touch(&tree.join("model.edf"));
        link_dir(&root.join("bikes"), &tree.join("loop"));

        let files = walk(&root.join("bikes"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count();
        assert!(files >= 1, "the real file is still found");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn subdirs_and_files_see_through_links() {
        let root = tmp("children");
        let elsewhere = root.join("elsewhere");
        touch(&elsewhere.join("Red.pnt"));
        let here = root.join("here");
        std::fs::create_dir_all(&here).unwrap();
        link_dir(&elsewhere, &here.join("paints"));
        link_dir(&elsewhere.join("Red.pnt"), &here.join("Blue.pnt"));

        assert_eq!(subdirs(&here), vec![here.join("paints")]);
        assert_eq!(files(&here), vec![here.join("Blue.pnt")]);
        assert!(is_link(&here.join("paints")));
        assert!(!is_link(&elsewhere));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn dir_links_reports_the_far_end_and_ignores_links_that_stay_inside() {
        let root = tmp("links");
        let outside = root.join("outside");
        touch(&outside.join("Frost.pnt"));
        let tree = root.join("mods");
        std::fs::create_dir_all(tree.join("bikes/KTM")).unwrap();
        link_dir(&outside, &tree.join("bikes/KTM/paints"));
        // A link that lands back inside the tree is already being walked.
        link_dir(&tree.join("bikes/KTM"), &tree.join("bikes/Copy"));

        let links = dir_links(&tree, 4, 16);
        assert_eq!(links.len(), 1, "{links:?}");
        assert_eq!(links[0].0, tree.join("bikes/KTM/paints"));
        assert_eq!(links[0].1, std::fs::canonicalize(&outside).unwrap());

        // Too deep to reach with a shallower budget.
        assert!(dir_links(&tree, 2, 16).is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn copy_tree_materialises_linked_folders() {
        let root = tmp("copy");
        let shared = root.join("shared");
        touch(&shared.join("Frost.pnt"));
        let src = root.join("src");
        touch(&src.join("model.edf"));
        link_dir(&shared, &src.join("paints"));

        let dst = root.join("dst");
        copy_tree(&src, &dst).expect("copy");
        assert!(dst.join("model.edf").is_file());
        assert!(dst.join("paints/Frost.pnt").is_file(), "the link's contents came along");
        assert!(!is_link(&dst.join("paints")), "and arrived as a real folder");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Following links means a cycle is reachable; the copy must stop rather than fill the
    /// disk with an ever-deeper tree.
    #[cfg(unix)]
    #[test]
    fn copy_tree_stops_at_a_cycle() {
        let root = tmp("copy-cycle");
        let src = root.join("src");
        touch(&src.join("a.txt"));
        link_dir(&src, &src.join("self"));

        let dst = root.join("dst");
        copy_tree(&src, &dst).expect("copy");
        assert!(dst.join("a.txt").is_file(), "the real content is copied");
        assert!(!dst.join("self").exists(), "the folder pointing back at itself is skipped");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn children_of_a_missing_folder_are_empty_not_an_error() {
        let root = tmp("missing");
        assert!(subdirs(&root.join("nope")).is_empty());
        assert!(files(&root.join("nope")).is_empty());
        assert!(!is_link(&root.join("nope")));
        assert!(dir_links(&root.join("nope"), 4, 16).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
