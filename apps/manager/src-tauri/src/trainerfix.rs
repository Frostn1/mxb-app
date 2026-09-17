//! The trainer files that crash MX Bikes at track load, and the one byte range that fixes them.
//!
//! This is the single most common MX Bikes crash. In MXBMRP3's survey of 46,246 crash reports
//! it is 39% of them — 17,231 — well clear of anything else, and it presents three ways
//! depending on where the process's memory happened to land that launch: a crash to desktop, a
//! hang on the loading screen, or a Windows BEX64 report with no dump at all.
//!
//! ## What is wrong with the file
//!
//! A trainer (`.trn`) holds the player's reference lap for one track and bike. The record has a
//! fixed-width bike-name buffer at `0x62`, and the game writes the name plus its NUL terminator
//! **without clearing the rest of the buffer**, so whatever was on the stack at save time is
//! serialised into the slack. On the next load it reads that slack back and hands it to an
//! sprintf-family call as a string pointer, and the CRT walks it off the end of mapped memory —
//! `msvcr90.dll+0x36ede`, the same offset on every build because the fault is in the runtime.
//!
//! Only PiBoSo can fix the serialiser. The damage already written can be undone here.
//!
//! ## The repair
//!
//! Zero the slack: everything from the name's NUL terminator up to [`NAME_PAD_END`]. That puts
//! zeros exactly where every trainer that loads has zeros, needs no donor file, and leaves the
//! rest of the record — the lap itself — untouched.
//!
//! The bound is not a guess. A graft bisection against a matched good/bad pair narrowed the
//! fatal bytes to `0x7f..0x85`: copying a donor's `0x0..0x86` made a crashing file load, copying
//! `0x0..0x7f` did not. Clearing the slack alone was then confirmed in game. The analysis, the
//! bisection and the in-game test are all Thomas's (`thomas4f/mxbmrp3`, `crash_analysis/`);
//! this is his repair applied to the folder the app already manages.
//!
//! ## What this refuses to touch
//!
//! Everything it does not recognise, and it is strict about it: the file must open with `GHS\0`,
//! the name buffer must hold printable ASCII up to a NUL inside the buffer, and the file must be
//! long enough to contain the slack. Anything else is left exactly as it is — guessing where the
//! name ends means zeroing somebody's lap. A file whose slack is already clear is not rewritten
//! at all.
//!
//! Every file that is changed goes to the recycle bin first, so the repair is reversible by the
//! player without us having to be right.

use std::path::{Path, PathBuf};

/// Every trainer starts with this. Also the magic on the handle pool that crashes at
/// `mxbikes.exe+0x11d753` when one is closed twice — the same files, the other bug.
const MAGIC: &[u8; 4] = b"GHS\0";

/// Where the fixed-width bike-name buffer starts.
const NAME_AT: usize = 0x62;

/// The end of the slack that follows the name. See the module docs for how it was pinned.
const NAME_PAD_END: usize = 0x92;

/// What one trainer file is, as far as the repair is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Not a trainer, or not a layout we know. Left alone.
    Skip,
    /// A trainer whose slack is already zeroed. Left alone.
    Clean,
    /// A trainer carrying leftover memory after the name. `bytes` is how much.
    Dirty { name: String, from: usize, bytes: usize },
}

/// Read the name buffer: where the name ends, and how much rubbish follows it.
///
/// `None` means the layout is not the one we know, which is the signal to leave the file
/// alone rather than to guess at it.
fn name_pad(bytes: &[u8]) -> Option<(String, usize, usize)> {
    if bytes.len() < NAME_PAD_END {
        return None;
    }
    let mut nul = None;
    for i in NAME_AT..NAME_PAD_END {
        let c = bytes[i];
        if c == 0 {
            nul = Some(i);
            break;
        }
        // A bike name is plain ASCII. Anything else means we are not reading a name, and
        // whatever we are reading is not ours to clear.
        if !(0x20..=0x7e).contains(&c) {
            return None;
        }
    }
    // A name that fills the whole buffer has no slack, and no terminator to measure from.
    let nul = nul?;
    let from = nul + 1;
    let dirty = bytes[from..NAME_PAD_END].iter().filter(|b| **b != 0).count();
    let name = String::from_utf8_lossy(&bytes[NAME_AT..nul]).into_owned();
    Some((name, from, dirty))
}

/// What to do with these bytes.
pub fn inspect(bytes: &[u8]) -> Verdict {
    if bytes.len() < NAME_PAD_END || !bytes.starts_with(MAGIC) {
        return Verdict::Skip;
    }
    match name_pad(bytes) {
        None => Verdict::Skip,
        Some((_, _, 0)) => Verdict::Clean,
        Some((name, from, bytes_dirty)) => Verdict::Dirty { name, from, bytes: bytes_dirty },
    }
}

/// The repaired bytes, or `None` when there is nothing to repair.
///
/// Only the slack changes. The lap, the header and everything past `NAME_PAD_END` are copied
/// through untouched — this must never be the reason somebody loses a reference lap.
pub fn repair(bytes: &[u8]) -> Option<Vec<u8>> {
    let Verdict::Dirty { from, .. } = inspect(bytes) else {
        return None;
    };
    let mut out = bytes.to_vec();
    out[from..NAME_PAD_END].fill(0);
    Some(out)
}

/// One file the repair changed.
#[derive(Debug, Clone)]
pub struct Repaired {
    pub path: PathBuf,
    /// The bike name the record carries, for the log line.
    pub name: String,
    /// How many bytes of leftover memory were cleared.
    pub bytes: usize,
}

fn is_trainer_file(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".trn")
}

/// Every `trainers` folder under `profiles_dir`, one per profile.
fn trainer_dirs(profiles_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(profiles_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| p.join("trainers"))
        .filter(|p| p.is_dir())
        .collect()
}

/// Repair every damaged trainer under `profiles_dir`, oldest first, and say which.
///
/// Safe to run repeatedly: a repaired file inspects as `Clean` on the next pass and is not
/// touched again. Does nothing at all in the common case, which is a folder of clean files.
///
/// `backup` is called with a file about to be changed, before it is changed. It is passed in
/// rather than called directly so the tests can exercise the walk without a recycle bin.
pub fn repair_all(profiles_dir: &Path, backup: &dyn Fn(&Path)) -> Vec<Repaired> {
    let mut fixed = Vec::new();
    for dir in trainer_dirs(profiles_dir) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_trainer_file(&entry.file_name().to_string_lossy()) {
                continue;
            }
            // A trainer is a few hundred kilobytes at most. Something far larger under this
            // name is not one, and is not going to be read into memory to find out.
            match entry.metadata() {
                Ok(m) if m.len() <= 8 * 1024 * 1024 => {}
                _ => continue,
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Verdict::Dirty { name, bytes: dirty, .. } = inspect(&bytes) else { continue };
            let Some(repaired) = repair(&bytes) else { continue };

            // Order matters, because the backup is a *move* to the recycle bin and not a
            // copy. Write the repaired file beside the original first, so a disk that is
            // full or a folder that is read-only costs nothing: the player's file has not
            // been touched at that point. Only then send the original to the bin, and only
            // then put the repaired one in its place.
            let staged = path.with_extension("trn.repairing");
            if let Err(e) = std::fs::write(&staged, &repaired) {
                log::warn!("trainers: could not stage a repair for {}: {e}", path.display());
                continue;
            }
            backup(&path);
            if let Err(e) = std::fs::rename(&staged, &path) {
                // The original is in the recycle bin and the repair is beside it under
                // `.trn.repairing`. Say both, because this is the one path where a player
                // has to do something themselves.
                log::error!(
                    "trainers: repaired {} but could not put it back ({e}). The original is \
                     in the recycle bin and the repaired copy is at {}.",
                    path.display(),
                    staged.display()
                );
                continue;
            }
            log::info!(
                "trainers: cleared {dirty} byte(s) of leftover memory after the name '{name}' \
                 in {}",
                path.display()
            );
            fixed.push(Repaired { path, name, bytes: dirty });
        }
    }
    fixed
}


/// Event the UI listens on when files were repaired.
pub const EVENT: &str = "trainers-repaired";

/// What the player is told, once, after a repair.
#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// How many files were repaired.
    pub count: usize,
    /// A few bike names, so the notice is concrete rather than a number.
    pub examples: Vec<String>,
}

/// Repair the trainers under this config's profiles folder, and tell the UI if any changed.
///
/// Called when the game is **not** running: at app start, and when a session ends. The
/// damage is written when the game saves a trainer, which is when you finish riding a
/// track, so the moment after a session is exactly when a fresh one appears — and the
/// moment before the next launch is when it would have crashed.
///
/// Silent when there is nothing to do, which is the common case.
pub fn repair_and_report(app: &tauri::AppHandle, cfg: &crate::config::AppConfig) {
    let dir = cfg.profiles_dir();
    if !dir.is_dir() {
        return;
    }
    // To the recycle bin, not overwritten in place. The player can put any of these back
    // without us, which is the only reason it is defensible to edit their files unasked.
    let fixed = repair_all(&dir, &|path: &Path| {
        if let Err(e) = crate::trashbin::move_to_trash(path) {
            log::warn!("trainers: could not back up {} ({e:#})", path.display());
        }
    });
    if fixed.is_empty() {
        return;
    }
    // Named, not counted: a bundle sent weeks later is the only record of what was changed
    // in somebody's profile, and "three files" is not a record.
    let cleared: usize = fixed.iter().map(|f| f.bytes).sum();
    log::info!(
        "trainers: repaired {} file(s) ({cleared} byte(s) of leftover memory in total) that \
         could have crashed the game at track load: {}",
        fixed.len(),
        fixed
            .iter()
            .map(|f| f.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut examples: Vec<String> = fixed.iter().map(|f| f.name.clone()).collect();
    examples.sort();
    examples.dedup();
    examples.truncate(3);
    use tauri::Emitter;
    let _ = app.emit(EVENT, &Report { count: fixed.len(), examples });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A trainer record as the game writes one: magic, a name at 0x62, and a lap after the
    /// slack. `slack` is what follows the name's terminator.
    fn trainer(name: &str, slack: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[..4].copy_from_slice(MAGIC);
        // Something in the header, so a test can prove the repair does not touch it.
        b[0x10..0x14].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        b[NAME_AT..NAME_AT + name.len()].copy_from_slice(name.as_bytes());
        let from = NAME_AT + name.len() + 1;
        b[from..from + slack.len()].copy_from_slice(slack);
        // The lap itself, past the slack.
        for (i, slot) in b[NAME_PAD_END..].iter_mut().enumerate() {
            *slot = (i % 251) as u8;
        }
        b
    }

    #[test]
    fn a_clean_trainer_is_left_alone() {
        let b = trainer("KTM 250 SX-F", &[]);
        assert_eq!(inspect(&b), Verdict::Clean);
        assert!(repair(&b).is_none(), "nothing to do means nothing is written");
    }

    #[test]
    fn leftover_memory_after_the_name_is_the_damage() {
        // The shape the bisection found: a leaked pointer sitting in the slack.
        let b = trainer("Husqvarna FC 250", &[0x30, 0x2F, 0x1A, 0x00, 0x00, 0x7F, 0x00, 0x00]);
        match inspect(&b) {
            Verdict::Dirty { ref name, bytes, .. } => {
                assert_eq!(name, "Husqvarna FC 250");
                assert_eq!(bytes, 4, "only the non-zero bytes count as leftovers");
            }
            other => panic!("expected dirty, got {other:?}"),
        }
    }

    #[test]
    fn the_repair_clears_the_slack_and_nothing_else() {
        let b = trainer("Yamaha YZ250F", &[0xAA; 12]);
        let fixed = repair(&b).expect("a dirty trainer is repairable");

        assert_eq!(fixed.len(), b.len(), "the file keeps its length");
        assert_eq!(&fixed[..NAME_AT], &b[..NAME_AT], "the header is untouched");
        assert_eq!(
            &fixed[NAME_PAD_END..],
            &b[NAME_PAD_END..],
            "the lap past the slack is untouched — this must never cost somebody their lap"
        );
        let name_end = NAME_AT + "Yamaha YZ250F".len();
        assert_eq!(&fixed[NAME_AT..name_end], b"Yamaha YZ250F", "the name survives");
        assert!(
            fixed[name_end..NAME_PAD_END].iter().all(|b| *b == 0),
            "everything from the terminator to the pad end is zero"
        );
        // And it settles: repairing twice changes nothing the second time.
        assert_eq!(inspect(&fixed), Verdict::Clean);
        assert!(repair(&fixed).is_none());
    }

    #[test]
    fn anything_that_is_not_a_trainer_is_skipped() {
        assert_eq!(inspect(b""), Verdict::Skip);
        assert_eq!(inspect(&[0u8; 0x400]), Verdict::Skip, "no magic, no repair");

        // Right magic, too short to hold the slack.
        let mut short = vec![0u8; 0x40];
        short[..4].copy_from_slice(MAGIC);
        assert_eq!(inspect(&short), Verdict::Skip);
    }

    #[test]
    fn a_name_buffer_we_cannot_read_is_left_alone() {
        // Not printable ASCII where the name should be: we are not looking at what we think
        // we are, and clearing bytes on that basis would be destroying data.
        let mut b = trainer("KTM 350", &[0xFF; 4]);
        b[NAME_AT + 2] = 0x01;
        assert_eq!(inspect(&b), Verdict::Skip);

        // A name with no terminator inside the buffer has no slack to measure.
        let mut full = trainer("x", &[]);
        for i in NAME_AT..NAME_PAD_END {
            full[i] = b'A';
        }
        assert_eq!(inspect(&full), Verdict::Skip);
    }

    fn dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("frost-trn-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn walks_every_profile_and_repairs_only_what_is_damaged() {
        let root = dir("walk");
        for profile in ["Frost", "Second Rider"] {
            let trainers = root.join(profile).join("trainers");
            fs::create_dir_all(&trainers).unwrap();
            fs::write(trainers.join("damaged.trn"), trainer("KTM 250 SX-F", &[0xAB; 6])).unwrap();
            fs::write(trainers.join("fine.trn"), trainer("GasGas MC 250F", &[])).unwrap();
            // Not ours: a different extension, and a file that only looks like a trainer.
            fs::write(trainers.join("notes.txt"), b"not a trainer").unwrap();
            fs::write(trainers.join("stranger.trn"), b"not a trainer either").unwrap();
        }

        let backed_up = std::cell::RefCell::new(Vec::new());
        let fixed = repair_all(&root, &|p: &Path| backed_up.borrow_mut().push(p.to_path_buf()));

        assert_eq!(fixed.len(), 2, "one damaged file per profile");
        assert!(fixed.iter().all(|f| f.name == "KTM 250 SX-F"));
        assert_eq!(backed_up.borrow().len(), 2, "every file changed was backed up first");

        // The file on disk is the repaired one, and nothing is left staged.
        for profile in ["Frost", "Second Rider"] {
            let t = root.join(profile).join("trainers");
            let on_disk = fs::read(t.join("damaged.trn")).unwrap();
            assert_eq!(inspect(&on_disk), Verdict::Clean, "the repair reached the disk");
            assert!(!t.join("damaged.trn.repairing").exists(), "nothing left staged");
        }

        // The clean one was not rewritten, and a second pass finds nothing.
        assert!(repair_all(&root, &|_: &Path| panic!("nothing left to back up")).is_empty());
    }

    #[test]
    fn a_missing_profiles_folder_is_not_an_error() {
        assert!(repair_all(Path::new("/nowhere/at/all"), &|_: &Path| {}).is_empty());
    }
}
