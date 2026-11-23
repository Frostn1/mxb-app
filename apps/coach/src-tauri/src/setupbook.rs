//! The setups Coach wrote, so a new save goes back onto Coach's own copy instead of numbering
//! another one.
//!
//! Coach never overwrote a setup, so every save took the next free name: "X (coach)",
//! "X (coach 2)", "X (coach 3)". The rider then rode "X (coach)", its name stripped back to
//! "X", and the next save became "(coach 2)". To the rider it was one setup, renamed every
//! session.
//!
//! Now each copy belongs to a lineage: the rider's setup it came from, or the track for one
//! Coach built from the game's default. A save goes back onto the lineage's latest copy when all
//! of these hold:
//! - Coach wrote that file. It's in this book.
//! - The file is unchanged since. Its SHA-256 still matches, so a copy the rider edited in the
//!   garage is theirs now.
//! - MX Bikes is closed. What the game does with a setup file it has open isn't known, and
//!   `default.ini` it certainly rewrites.
//!
//! Anything else gets a new numbered copy, as before, and that copy becomes the lineage's latest.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Entries past this are dropped oldest first. Far more than a rider makes, and the book is
/// read on every setup plan.
const MAX_ENTRIES: usize = 500;

/// A setup file Coach wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The file as it was written, in its real case: the key is lowercased for matching, but a
    /// case-sensitive filesystem needs the real name to open it.
    pub path: PathBuf,
    /// Hex SHA-256 of the bytes Coach wrote.
    pub sha256: String,
    /// `coach:<base>` or `fresh:<track>`: what the names are built from.
    pub lineage: String,
    /// Milliseconds since the epoch.
    pub written: u64,
}

/// Keyed by the file's path, lowercased: Windows paths don't care about case, and neither
/// should this.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    #[serde(default)]
    pub files: BTreeMap<String, Entry>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

pub fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Where a save goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Back onto this file, Coach's own copy, untouched since.
    Over(PathBuf),
    /// A new file under this name.
    New(String),
}

impl Target {
    pub fn name(&self) -> String {
        match self {
            Target::Over(p) => p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            Target::New(n) => n.clone(),
        }
    }
}

impl Book {
    pub fn load(path: &Path) -> Book {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn store(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// The entry for `file`, if Coach wrote it and `now_hash` (what's on disk) still matches.
    pub fn ours(&self, file: &Path, now_hash: Option<&str>) -> Option<&Entry> {
        let e = self.files.get(&key(file))?;
        (Some(e.sha256.as_str()) == now_hash).then_some(e)
    }

    /// The lineage's most recent copy in `dir`, whatever state it's in now.
    fn latest(&self, lineage: &str, dir: &Path) -> Option<PathBuf> {
        let dir_key = key(dir);
        self.files
            .iter()
            .filter(|(k, e)| e.lineage == lineage && Path::new(k.as_str()).parent().map(key).as_deref() == Some(dir_key.as_str()))
            .max_by_key(|(_, e)| e.written)
            .map(|(_, e)| e.path.clone())
    }

    /// Where a save of `lineage` into `dir` goes: back onto the latest copy when it's still
    /// Coach's and the game is closed, else the first of `names` nothing has taken.
    ///
    /// `on_disk` reads a file Coach wrote, returning its path and current fingerprint.
    pub fn target(
        &self,
        lineage: &str,
        dir: &Path,
        names: &[String],
        game_open: bool,
        on_disk: &dyn Fn(&Path) -> Option<(PathBuf, String)>,
        taken: &dyn Fn(&str) -> bool,
    ) -> Option<Target> {
        if !game_open {
            if let Some(latest) = self.latest(lineage, dir) {
                if let Some((real, hash)) = on_disk(&latest) {
                    if self.ours(&latest, Some(&hash)).is_some() {
                        return Some(Target::Over(real));
                    }
                }
            }
        }
        names.iter().find(|n| !taken(n)).cloned().map(Target::New)
    }

    /// Write down a file Coach just wrote, and drop what's gone or too old.
    pub fn record(&mut self, file: &Path, bytes: &[u8], lineage: &str, now: u64, exists: &dyn Fn(&Path) -> bool) {
        self.files.insert(
            key(file),
            Entry { path: file.to_path_buf(), sha256: fingerprint(bytes), lineage: lineage.to_string(), written: now },
        );
        self.files.retain(|_, e| exists(&e.path));
        while self.files.len() > MAX_ENTRIES {
            let oldest = self.files.iter().min_by_key(|(_, e)| e.written).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => self.files.remove(&k),
                None => break,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "c:/p/setups/indiana/kx450";

    fn names(base: &str) -> Vec<String> {
        crate::stp::coach_names(base).take(5).collect()
    }

    /// A fake disk: path (as the book keys it) -> bytes.
    struct Disk(BTreeMap<String, Vec<u8>>);
    impl Disk {
        fn on_disk(&self) -> impl Fn(&Path) -> Option<(PathBuf, String)> + '_ {
            move |p: &Path| self.0.get(&key(p)).map(|b| (p.to_path_buf(), fingerprint(b)))
        }
        fn taken(&self) -> impl Fn(&str) -> bool + '_ {
            move |n: &str| self.0.contains_key(&key(&Path::new(DIR).join(format!("{n}.stp"))))
        }
        fn exists(&self) -> impl Fn(&Path) -> bool + '_ {
            move |p: &Path| self.0.contains_key(&key(p))
        }
        fn put(&mut self, name: &str, bytes: &[u8]) -> PathBuf {
            let p = Path::new(DIR).join(format!("{name}.stp"));
            self.0.insert(key(&p), bytes.to_vec());
            p
        }
    }

    /// The bug: the second save of the same setup took a new number. Now it goes back onto the
    /// copy Coach wrote.
    #[test]
    fn a_second_save_goes_back_onto_coachs_own_copy() {
        let (mut book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        disk.put("Fast", b"rider's own");
        let lineage = "coach:Fast";

        let first = book.target(lineage, Path::new(DIR), &names("Fast"), false, &disk.on_disk(), &disk.taken()).unwrap();
        assert_eq!(first, Target::New("Fast (coach)".into()));
        let p = disk.put("Fast (coach)", b"v1");
        book.record(&p, b"v1", lineage, 1, &disk.exists());

        let second = book.target(lineage, Path::new(DIR), &names("Fast"), false, &disk.on_disk(), &disk.taken()).unwrap();
        assert_eq!(second.name(), "Fast (coach)", "same name, not \"(coach 2)\"");
        assert!(matches!(second, Target::Over(_)));
    }

    /// A copy the rider changed in the garage is theirs: never written over.
    #[test]
    fn a_copy_the_rider_edited_is_never_overwritten() {
        let (mut book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        let p = disk.put("Fast (coach)", b"v1");
        book.record(&p, b"v1", "coach:Fast", 1, &disk.exists());
        disk.put("Fast (coach)", b"edited in the garage");

        let t = book.target("coach:Fast", Path::new(DIR), &names("Fast"), false, &disk.on_disk(), &disk.taken()).unwrap();
        assert_eq!(t, Target::New("Fast (coach 2)".into()));
    }

    /// A file Coach didn't write, even one with a Coach-looking name, is left alone.
    #[test]
    fn a_file_coach_never_wrote_is_left_alone() {
        let (book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        disk.put("Fast (coach)", b"from an older Coach, or hand-made");
        let t = book.target("coach:Fast", Path::new(DIR), &names("Fast"), false, &disk.on_disk(), &disk.taken()).unwrap();
        assert_eq!(t, Target::New("Fast (coach 2)".into()));
    }

    /// With MX Bikes open, nothing is overwritten: a new copy, as before.
    #[test]
    fn with_the_game_open_a_new_copy_is_made() {
        let (mut book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        let p = disk.put("Fast (coach)", b"v1");
        book.record(&p, b"v1", "coach:Fast", 1, &disk.exists());
        let t = book.target("coach:Fast", Path::new(DIR), &names("Fast"), true, &disk.on_disk(), &disk.taken()).unwrap();
        assert_eq!(t, Target::New("Fast (coach 2)".into()));
    }

    /// Once a new copy is made (the old one was edited, say), it becomes the one saved back onto.
    #[test]
    fn the_latest_copy_is_the_one_carried_forward() {
        let (mut book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        let p1 = disk.put("Fast (coach)", b"v1");
        book.record(&p1, b"v1", "coach:Fast", 1, &disk.exists());
        disk.put("Fast (coach)", b"edited");
        let p2 = disk.put("Fast (coach 2)", b"v2");
        book.record(&p2, b"v2", "coach:Fast", 2, &disk.exists());
        let t = book.target("coach:Fast", Path::new(DIR), &names("Fast"), false, &disk.on_disk(), &disk.taken()).unwrap();
        assert_eq!(t.name(), "Fast (coach 2)");
        assert!(matches!(t, Target::Over(_)));
    }

    /// A lineage is per folder: the same setup name on another track is another copy.
    #[test]
    fn another_track_is_another_copy() {
        let (mut book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        let p = disk.put("Fast (coach)", b"v1");
        book.record(&p, b"v1", "coach:Fast", 1, &disk.exists());
        let other = Path::new("c:/p/setups/erzberg/kx450");
        let t = book.target("coach:Fast", other, &names("Fast"), false, &disk.on_disk(), &|_| false).unwrap();
        assert_eq!(t, Target::New("Fast (coach)".into()));
    }

    #[test]
    fn the_book_forgets_files_that_are_gone_and_survives_json() {
        let (mut book, mut disk) = (Book::default(), Disk(BTreeMap::new()));
        let p1 = disk.put("A (coach)", b"a");
        book.record(&p1, b"a", "coach:A", 1, &disk.exists());
        disk.0.clear();
        let p2 = disk.put("B (coach)", b"b");
        book.record(&p2, b"b", "coach:B", 2, &disk.exists());
        assert_eq!(book.files.len(), 1, "A's file is gone, so is its entry");
        let text = serde_json::to_string(&book).unwrap();
        assert_eq!(serde_json::from_str::<Book>(&text).unwrap(), book);
        assert_eq!(fingerprint(b"abc").len(), 64);
    }
}
