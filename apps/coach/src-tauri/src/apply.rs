//! Writing the coach's changes into a rider's own setup file, with a way back.
//!
//! "Save a copy" leaves the rider's setup alone. This is the other choice, for a rider who
//! would rather the coach did the editing: the changes go into the setup they ride, and the
//! file as it was is kept beside it first, so one press undoes it.
//!
//! - **Backup.** `<name>.stp.coach-backup-<UTC stamp>` in the same folder. The game lists only
//!   `.stp` files, so a backup never turns up in the garage as a setup.
//! - **Swap.** The new bytes are written to a hidden file in the same folder and renamed over
//!   the setup, so the file is never half-written. The bytes the changes were worked out from
//!   are compared with what is on disk first: a setup saved in the garage a moment ago is not
//!   written over with changes worked out from the one before.
//! - **Undo.** The newest backup goes back over the setup and is used up, so pressing it again
//!   steps back to the one before.
//! - **Never with the game open.** The caller refuses while MX Bikes runs: the game keeps its
//!   garage in memory and writes it out on exit, which would undo the change and look like
//!   the coach had lied.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Between the setup's file name and the stamp.
pub const TAG: &str = ".coach-backup-";

/// `YYYYMMDD-HHMMSS` in UTC, so backups sort by name in the order they were made.
pub fn stamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn file_name(file: &Path) -> Option<String> {
    file.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Every backup of `file`, newest first.
pub fn backups(file: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(name)) = (file.parent(), file_name(file)) else { return Vec::new() };
    let prefix = format!("{name}{TAG}");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect();
    // The stamp sorts; a same-second suffix ("-2") sorts after it.
    found.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    found
}

/// A hidden file in `dir` nothing else has, created empty.
fn scratch(dir: &Path, tag: &str) -> Option<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    for n in 0..16u32 {
        let p = dir.join(format!(".coach-apply-{tag}-{}-{nanos}-{n}", std::process::id()));
        if std::fs::OpenOptions::new().write(true).create_new(true).open(&p).is_ok() {
            return Some(p);
        }
    }
    None
}

/// Put `bytes` in place of `file` without a moment where it is half-written.
fn swap_in(file: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = file.parent().ok_or("no folder")?;
    let tmp = scratch(dir, "new").ok_or("couldn't write beside the setup")?;
    let done = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, file));
    if let Err(e) = done {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(())
}

/// Back up `file`, then replace it with `new`. `was` is the bytes `new` was worked out from.
/// Returns the backup's path. On any failure the setup is as it was.
pub fn commit(file: &Path, was: &[u8], new: &[u8], stamp: &str) -> Result<PathBuf, String> {
    let now = std::fs::read(file).map_err(|e| e.to_string())?;
    if now != was {
        return Err("changed while the coach was working on it".into());
    }
    let name = file_name(file).ok_or("no file name")?;
    let dir = file.parent().ok_or("no folder")?;
    // Reserved with create_new, so an older backup is never overwritten.
    let mut backup = None;
    for n in 1..100 {
        let tail = if n == 1 { String::new() } else { format!("-{n}") };
        let p = dir.join(format!("{name}{TAG}{stamp}{tail}"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(mut f) => {
                if let Err(e) = std::io::Write::write_all(&mut f, was) {
                    drop(f);
                    let _ = std::fs::remove_file(&p);
                    return Err(e.to_string());
                }
                backup = Some(p);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    let backup = backup.ok_or("too many backups in one second")?;
    if let Err(e) = swap_in(file, new) {
        let _ = std::fs::remove_file(&backup);
        return Err(e);
    }
    Ok(backup)
}

/// Put the newest backup back over `file` and use it up. Returns the backup's path.
pub fn restore(file: &Path) -> Result<PathBuf, String> {
    let newest = backups(file).into_iter().next().ok_or("there's no backup of this setup")?;
    let bytes = std::fs::read(&newest).map_err(|e| e.to_string())?;
    swap_in(file, &bytes)?;
    let _ = std::fs::remove_file(&newest);
    Ok(newest)
}

/// Where a setup sits, for naming it to the rider: the profile folder, then the place under
/// `setups` (a track, or `common`), layout included.
pub fn describe(file: &Path) -> (String, String) {
    let parts: Vec<String> = file.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let Some(at) = parts.iter().rposition(|p| p.eq_ignore_ascii_case("setups")) else { return (String::new(), String::new()) };
    let profile = at.checked_sub(1).map(|i| parts[i].clone()).unwrap_or_default();
    // Between `setups` and the file: <place>[/<layout>]/<bike>. The bike is last; it says nothing new.
    let between = &parts[at + 1..parts.len().saturating_sub(1)];
    let place = between[..between.len().saturating_sub(1)].join(" / ");
    (profile, place)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("coach-apply-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_stamp_is_a_utc_date_that_sorts() {
        assert_eq!(stamp(UNIX_EPOCH), "19700101-000000");
        assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(1_794_000_000)), "20261106-212000");
        // Leap day.
        assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(1_709_210_096)), "20240229-123456");
    }

    #[test]
    fn apply_keeps_the_original_beside_it_and_undo_puts_it_back() {
        let d = dir("roundtrip");
        let f = d.join("race.stp");
        std::fs::write(&f, b"before").unwrap();
        let b = commit(&f, b"before", b"after", "20261112-101500").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"after");
        assert_eq!(std::fs::read(&b).unwrap(), b"before");
        assert_eq!(b.file_name().unwrap().to_string_lossy(), "race.stp.coach-backup-20261112-101500");
        assert_eq!(backups(&f), vec![b.clone()]);
        // Nothing else was left in the folder, and the backup isn't an .stp the garage lists.
        let left: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left.len(), 2, "{left:?}");
        assert!(b.extension().is_none_or(|e| !e.eq_ignore_ascii_case("stp")));

        restore(&f).unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"before");
        assert!(backups(&f).is_empty(), "the backup is used up");
        assert!(restore(&f).is_err(), "nothing left to restore");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn two_applies_in_one_second_keep_both_backups_and_undo_steps_back_one_at_a_time() {
        let d = dir("twice");
        let f = d.join("race.stp");
        std::fs::write(&f, b"v0").unwrap();
        commit(&f, b"v0", b"v1", "20261112-101500").unwrap();
        commit(&f, b"v1", b"v2", "20261112-101500").unwrap();
        assert_eq!(backups(&f).len(), 2);
        restore(&f).unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"v1");
        restore(&f).unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"v0");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The rider saved in the garage after the plan was read: their save is not written over.
    #[test]
    fn a_setup_that_changed_since_it_was_read_is_left_alone() {
        let d = dir("raced");
        let f = d.join("race.stp");
        std::fs::write(&f, b"saved in the garage").unwrap();
        assert!(commit(&f, b"what the plan was made from", b"after", "20261112-101500").is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"saved in the garage");
        assert!(backups(&f).is_empty(), "no stray backup");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_backup_belongs_to_its_own_setup() {
        let d = dir("own");
        let (a, b) = (d.join("a.stp"), d.join("a (coach).stp"));
        std::fs::write(&a, b"1").unwrap();
        std::fs::write(&b, b"1").unwrap();
        commit(&a, b"1", b"2", "20261112-101500").unwrap();
        assert_eq!(backups(&a).len(), 1);
        assert!(backups(&b).is_empty(), "'a (coach).stp' is not a's backup");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_setup_is_described_by_its_profile_and_place() {
        let f = Path::new("C:/g/profiles/Terp/setups/indiana/kx450/race.stp");
        assert_eq!(describe(f), ("Terp".into(), "indiana".into()));
        let g = Path::new("C:/g/profiles/Second/setups/common/kx450/race.stp");
        assert_eq!(describe(g), ("Second".into(), "common".into()));
        let h = Path::new("C:/g/profiles/Terp/setups/hangtown/Pro/kx450/race.stp");
        assert_eq!(describe(h), ("Terp".into(), "hangtown / Pro".into()));
    }
}
