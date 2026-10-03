//! Unpacking zip, 7z and rar archives without letting an entry land outside the staging
//! folder.
//!
//! Archives reach the app from download links and share codes, so the names inside them are
//! untrusted. Every entry name is checked *before* anything is written: it is rebuilt one
//! component at a time, and an absolute path, a drive or UNC prefix, a `..`, a NUL byte, an
//! alternate data stream or a reserved device name rejects the whole archive. Link entries
//! are refused rather than resolved, and nothing is ever written through a link or reparse
//! point that already sits in the staging folder.
//!
//! [`crate::install`]'s post-extraction sweep still runs afterwards as a second line of
//! defence, but it cannot be the guard: it only looks inside the staging folder, and a file
//! written beside it is never seen.
//!
//! No Tauri types in here, so the checks can be exercised on their own.

use anyhow::{anyhow, Result};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

/// Why an entry name can't be used, for the message the user sees.
type Why = &'static str;

/// Turn an archive entry name into a path relative to the staging folder.
///
/// `Ok(None)` is a name that names the staging folder itself (`./`, say) — harmless for a
/// directory entry, and refused for a file by [`plan`].
pub(crate) fn entry_rel_path(raw: &str) -> Result<Option<PathBuf>, Why> {
    if raw.contains('\0') {
        return Err("NUL byte in the name");
    }
    // Archives written on Windows use `\`; treat both as separators on every platform so a
    // name can't hide a `..` behind the "wrong" one.
    let name = raw.replace('\\', "/");
    if name.starts_with('/') {
        return Err("absolute path");
    }
    let mut out = PathBuf::new();
    for part in name.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err("parent-directory component"),
            p => {
                check_component(p)?;
                out.push(p);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Ok(None);
    }
    // Whatever this platform's `Path` makes of the result, it must be plain names only.
    if !out.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("not a plain relative path");
    }
    Ok(Some(out))
}

/// One path component, already free of separators.
fn check_component(p: &str) -> Result<(), Why> {
    // `C:`, `C:x` and `name:stream` all hinge on the colon.
    if p.contains(':') {
        return Err("drive letter or alternate data stream");
    }
    if p.chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err("character Windows can't store in a file name");
    }
    // Windows drops a trailing dot or space, so `... ` and friends would not mean what they say.
    if p.ends_with('.') || p.ends_with(' ') {
        return Err("trailing dot or space");
    }
    let stem = p
        .split('.')
        .next()
        .unwrap_or(p)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.chars().count() == 4
            && stem[3..]
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '¹' | '²' | '³')));
    if reserved {
        return Err("reserved Windows device name");
    }
    Ok(())
}

/// The relative path an entry will be written to, or the error that rejects the archive.
fn plan(archive: &Path, raw: &str, is_dir: bool) -> Result<Option<PathBuf>> {
    match entry_rel_path(raw) {
        Ok(Some(rel)) => Ok(Some(rel)),
        Ok(None) if is_dir => Ok(None),
        Ok(None) => Err(rejected(archive, raw, "empty file name")),
        Err(why) => Err(rejected(archive, raw, why)),
    }
}

fn rejected(archive: &Path, raw: &str, why: &str) -> anyhow::Error {
    anyhow!(
        "{} was rejected: entry {raw:?} is not safe to unpack ({why}). Nothing from it was installed.",
        archive.file_name().unwrap_or_default().to_string_lossy()
    )
}

/// A link or reparse point — anything that could redirect a write elsewhere.
fn is_link(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Windows attributes as 7-Zip and RAR record them: the reparse-point bit, or — for an
/// entry archived on a Unix host — the file mode carried in the high half (7z) or the whole
/// word (rar).
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

/// Whether a 7z entry is a link. 7-Zip sets bit 15 to say the high 16 bits are a Unix mode.
pub(crate) fn sevenz_attr_is_link(has_attrs: bool, attrs: u32) -> bool {
    has_attrs
        && (attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (attrs & 0x8000 != 0 && (attrs >> 16) & S_IFMT == S_IFLNK))
}

/// Whether a rar entry is a link, from the attributes the unrar library reports: Windows
/// attributes for an entry archived on Windows, the Unix mode for one archived on Unix.
pub(crate) fn rar_attr_is_link(attrs: u32) -> bool {
    attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 || attrs & S_IFMT == S_IFLNK
}

/// `canonicalize` hands back `\\?\C:\…` on Windows. Every path here is built from the root,
/// so drop the verbatim prefix from an ordinary drive path: the native rar library is given
/// these paths, and a plain one is what it expects.
fn plain_path(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            let b = rest.as_bytes();
            if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
                return PathBuf::from(rest);
            }
        }
    }
    p
}

/// The staging folder, resolved once, with writes that never pass through a link.
pub(crate) struct Staging {
    root: PathBuf,
}

impl Staging {
    pub(crate) fn new(dest: &Path) -> Result<Self> {
        std::fs::create_dir_all(dest)?;
        Ok(Self {
            root: plain_path(dest.canonicalize()?),
        })
    }

    /// Create `rel` under the root one component at a time, refusing to step through a link
    /// or onto a file. Returns the full path.
    pub(crate) fn dir(&self, rel: &Path) -> Result<PathBuf> {
        let mut cur = self.root.clone();
        for c in rel.components() {
            let Component::Normal(part) = c else {
                anyhow::bail!("refusing to unpack into {}", rel.display());
            };
            cur.push(part);
            match std::fs::symlink_metadata(&cur) {
                Ok(m) if is_link(&m) => {
                    anyhow::bail!("refusing to unpack through a link at {}", cur.display())
                }
                Ok(m) if m.is_dir() => {}
                Ok(_) => anyhow::bail!(
                    "the archive names {} as both a file and a folder",
                    cur.display()
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    match std::fs::create_dir(&cur) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        debug_assert!(cur.starts_with(&self.root));
        Ok(cur)
    }

    /// Where `rel` will be written, with its folders made and any earlier copy removed — so a
    /// later entry of the same name replaces the file rather than writing through whatever
    /// that name now points at.
    pub(crate) fn prepare_file(&self, rel: &Path) -> Result<PathBuf> {
        let name = rel
            .file_name()
            .ok_or_else(|| anyhow!("no file name in {}", rel.display()))?;
        let parent = self.dir(rel.parent().unwrap_or(Path::new("")))?;
        let path = parent.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(m) if is_link(&m) => {
                anyhow::bail!("refusing to write through a link at {}", path.display())
            }
            Ok(m) if m.is_dir() => anyhow::bail!(
                "the archive names {} as both a folder and a file",
                path.display()
            ),
            Ok(_) => std::fs::remove_file(&path)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(path)
    }

    pub(crate) fn create_file(&self, rel: &Path) -> Result<File> {
        let path = self.prepare_file(rel)?;
        Ok(OpenOptions::new().write(true).create_new(true).open(&path)?)
    }

    /// After a native extractor wrote `path`: it must be a plain file, not a link it made.
    fn verify_plain(&self, path: &Path) -> Result<()> {
        let m = std::fs::symlink_metadata(path)?;
        if is_link(&m) {
            let _ = std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path));
            anyhow::bail!("the archive tried to create a link at {}", path.display());
        }
        Ok(())
    }
}

/// Unpack a zip from anything that reads like one. `label` names the archive in errors.
pub(crate) fn extract_zip<R: Read + Seek>(reader: R, dest: &Path, label: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(reader)?;
    let mut planned = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i)?;
        let name = e.name().to_string();
        if e.is_symlink() {
            return Err(rejected(label, &name, "links are not allowed"));
        }
        let is_dir = e.is_dir() || name.ends_with('\\');
        planned.push((plan(label, &name, is_dir)?, is_dir));
    }
    let staging = Staging::new(dest)?;
    for (i, (rel, is_dir)) in planned.into_iter().enumerate() {
        let Some(rel) = rel else { continue };
        if is_dir {
            staging.dir(&rel)?;
            continue;
        }
        let mut out = staging.create_file(&rel)?;
        let mut entry = zip.by_index(i)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

/// Unpack a 7z, checking every entry in its header before decoding any of them.
pub(crate) fn extract_7z(archive: &Path, dest: &Path) -> Result<()> {
    let open = File::open(archive)?;
    let len = open.metadata()?.len();
    extract_7z_from(open, len, dest, archive)
}

pub(crate) fn extract_7z_from<R: Read + Seek>(
    reader: R,
    len: u64,
    dest: &Path,
    label: &Path,
) -> Result<()> {
    let mut seven = sevenz_rust::SevenZReader::new(reader, len, sevenz_rust::Password::empty())
        .map_err(|e| anyhow!("7z extraction failed: {e}"))?;
    for e in &seven.archive().files {
        if sevenz_attr_is_link(e.has_windows_attributes, e.windows_attributes) {
            return Err(rejected(label, e.name(), "links are not allowed"));
        }
        plan(label, e.name(), e.is_directory())?;
    }
    let staging = Staging::new(dest)?;
    let mut failure: Option<anyhow::Error> = None;
    seven
        .for_each_entries(|entry, data| {
            // The reader moves on to the next block even after a `false`, so keep refusing.
            if failure.is_some() {
                return Ok(false);
            }
            let step = (|| -> Result<()> {
                if entry.is_anti_item() {
                    return Ok(());
                }
                let Some(rel) = plan(label, entry.name(), entry.is_directory())? else {
                    return Ok(());
                };
                if entry.is_directory() {
                    staging.dir(&rel)?;
                } else {
                    let mut out = staging.create_file(&rel)?;
                    std::io::copy(data, &mut out)?;
                }
                Ok(())
            })();
            match step {
                Ok(()) => Ok(true),
                Err(e) => {
                    failure = Some(e);
                    Ok(false)
                }
            }
        })
        .map_err(|e| anyhow!("7z extraction failed: {e}"))?;
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Unpack a rar: list and check every header first, then extract each file to the exact path
/// that was checked, never letting the native library pick the name.
pub(crate) fn extract_rar(archive: &Path, dest: &Path) -> Result<()> {
    let listing = unrar::Archive::new(archive)
        .open_for_listing()
        .map_err(|e| anyhow!("failed to open RAR: {e}"))?;
    for header in listing {
        let h = header.map_err(|e| anyhow!("RAR read error: {e}"))?;
        let name = h.filename.to_string_lossy();
        if rar_attr_is_link(h.file_attr) {
            return Err(rejected(archive, &name, "links are not allowed"));
        }
        plan(archive, &name, h.is_directory())?;
    }

    let staging = Staging::new(dest)?;
    let mut open = unrar::Archive::new(archive)
        .open_for_processing()
        .map_err(|e| anyhow!("failed to open RAR: {e}"))?;
    while let Some(header) = open
        .read_header()
        .map_err(|e| anyhow!("RAR read error: {e}"))?
    {
        let entry = header.entry();
        let name = entry.filename.to_string_lossy().into_owned();
        let is_dir = entry.is_directory();
        if rar_attr_is_link(entry.file_attr) {
            return Err(rejected(archive, &name, "links are not allowed"));
        }
        let rel = plan(archive, &name, is_dir)?;
        open = match rel {
            Some(rel) if !is_dir => {
                let path = staging.prepare_file(&rel)?;
                let next = header
                    .extract_to(&path)
                    .map_err(|e| anyhow!("RAR extract error: {e}"))?;
                staging.verify_plain(&path)?;
                next
            }
            other => {
                if let Some(rel) = other {
                    staging.dir(&rel)?;
                }
                header
                    .skip()
                    .map_err(|e| anyhow!("RAR skip error: {e}"))?
            }
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn files_under(dir: &Path) -> Vec<PathBuf> {
        walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| !e.file_type().is_dir())
            .map(|e| e.path().to_path_buf())
            .collect()
    }

    /// Names that must never be written. Each is paired with a benign entry *before* it, so a
    /// pass also proves nothing was written ahead of the check.
    fn hostile_names(tag: &str) -> Vec<String> {
        vec![
            format!("../{tag}.txt"),
            format!("..\\{tag}.txt"),
            format!("ok/../../{tag}.txt"),
            format!("ok\\..\\..\\{tag}.txt"),
            format!("/{tag}.txt"),
            format!("\\{tag}.txt"),
            format!("C:\\{tag}.txt"),
            format!("C:/{tag}.txt"),
            format!("C:{tag}.txt"),
            format!("\\\\server\\share\\{tag}.txt"),
            format!("//?/C:/{tag}.txt"),
            format!("ok/{tag}.txt:stream"),
            format!("ok/{tag}\0.txt"),
            "ok/CON".to_string(),
            "ok/nul.txt".to_string(),
            "ok/COM1.pkz".to_string(),
            "ok/...".to_string(),
        ]
    }

    #[test]
    fn plain_names_pass_and_normalise() {
        assert_eq!(
            entry_rel_path("Track/track.pkz").unwrap(),
            Some(PathBuf::from("Track").join("track.pkz"))
        );
        assert_eq!(
            entry_rel_path("Track\\paints\\a.pnt").unwrap(),
            Some(PathBuf::from("Track").join("paints").join("a.pnt"))
        );
        assert_eq!(
            entry_rel_path("./a/./b.txt").unwrap(),
            Some(PathBuf::from("a").join("b.txt"))
        );
        assert_eq!(entry_rel_path("./").unwrap(), None);
        // Dots inside a name are fine; only a whole `..` component climbs.
        assert!(entry_rel_path("a..b/c...d.txt").unwrap().is_some());
        assert!(entry_rel_path("console.cfg").unwrap().is_some());
        assert!(entry_rel_path("COM10.txt").unwrap().is_some());
    }

    #[test]
    fn hostile_names_are_refused() {
        for n in hostile_names("x") {
            assert!(entry_rel_path(&n).is_err(), "{n:?} must be refused");
        }
    }

    #[test]
    fn link_attributes_are_recognised() {
        let unix_link = 0x8000 | (0o120777 << 16);
        let unix_file = 0x8000 | (0o100644 << 16);
        assert!(sevenz_attr_is_link(true, unix_link));
        assert!(sevenz_attr_is_link(true, 0x400 | 0x20));
        assert!(!sevenz_attr_is_link(true, unix_file));
        assert!(!sevenz_attr_is_link(true, 0x20));
        assert!(!sevenz_attr_is_link(false, unix_link));
        assert!(rar_attr_is_link(0o120777));
        assert!(rar_attr_is_link(0x400));
        assert!(!rar_attr_is_link(0o100644));
        assert!(!rar_attr_is_link(0x20));
    }

    // ---- zip ----

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn zip_with_a_hostile_entry_is_rejected_whole() {
        let base = tmp("zip-hostile");
        let tag = format!("frost-guard-zip-{}", std::process::id());
        for (i, bad) in hostile_names(&tag).iter().enumerate() {
            let dest = base.join(format!("stage{i}")).join("staged");
            let bytes = zip_with(&[("ok/first.txt", b"fine"), (bad, b"nope")]);
            let err = extract_zip(Cursor::new(bytes), &dest, Path::new("evil.zip"))
                .expect_err(&format!("{bad:?} must be rejected"));
            assert!(err.to_string().contains("rejected"), "{err:#}");
            assert!(files_under(&base).is_empty(), "{bad:?} wrote something");
        }
        assert!(!std::env::temp_dir().join(format!("{tag}.txt")).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn zip_with_a_link_entry_is_rejected() {
        let base = tmp("zip-link");
        let dest = base.join("staged");
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        w.add_symlink("link", "../outside", opts).unwrap();
        w.start_file("link/pwned.txt", opts).unwrap();
        w.write_all(b"nope").unwrap();
        let bytes = w.finish().unwrap().into_inner();

        let err = extract_zip(Cursor::new(bytes), &dest, Path::new("evil.zip")).unwrap_err();
        assert!(err.to_string().contains("links are not allowed"), "{err:#}");
        assert!(files_under(&base).is_empty());
        assert!(!base.join("outside").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn benign_zip_still_extracts() {
        let base = tmp("zip-ok");
        let dest = base.join("staged");
        let bytes = zip_with(&[
            ("Track/track.pkz", b"PKZ"),
            ("Track\\paints\\a.pnt", b"PNT"),
            ("readme.txt", b"hi"),
        ]);
        extract_zip(Cursor::new(bytes), &dest, Path::new("ok.zip")).unwrap();
        assert_eq!(std::fs::read(dest.join("Track/track.pkz")).unwrap(), b"PKZ");
        assert_eq!(std::fs::read(dest.join("Track/paints/a.pnt")).unwrap(), b"PNT");
        assert_eq!(std::fs::read(dest.join("readme.txt")).unwrap(), b"hi");
        let _ = std::fs::remove_dir_all(&base);
    }

    // ---- 7z ----

    struct Entry<'a> {
        name: &'a str,
        data: &'a [u8],
        dir: bool,
        attrs: Option<u32>,
    }

    fn file<'a>(name: &'a str, data: &'a [u8]) -> Entry<'a> {
        Entry { name, data, dir: false, attrs: None }
    }

    fn sevenz_with(entries: &[Entry]) -> Vec<u8> {
        let mut w = sevenz_rust::SevenZWriter::new(Cursor::new(Vec::new())).unwrap();
        for e in entries {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = e.name.to_string();
            entry.is_directory = e.dir;
            entry.has_stream = !e.dir;
            if let Some(a) = e.attrs {
                entry.has_windows_attributes = true;
                entry.windows_attributes = a;
            }
            let reader = (!e.dir).then_some(e.data);
            w.push_archive_entry(entry, reader).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn extract_7z_bytes(bytes: Vec<u8>, dest: &Path) -> Result<()> {
        let len = bytes.len() as u64;
        extract_7z_from(Cursor::new(bytes), len, dest, Path::new("evil.7z"))
    }

    #[test]
    fn sevenz_with_a_hostile_entry_is_rejected_whole() {
        let base = tmp("7z-hostile");
        let tag = format!("frost-guard-7z-{}", std::process::id());
        // 7z stores names NUL-terminated, so a NUL can't be smuggled into one.
        for (i, bad) in hostile_names(&tag).iter().filter(|n| !n.contains('\0')).enumerate() {
            let dest = base.join(format!("stage{i}")).join("staged");
            let bytes = sevenz_with(&[file("ok/first.txt", b"fine"), file(bad, b"nope")]);
            let err = extract_7z_bytes(bytes, &dest).expect_err(&format!("{bad:?} must be rejected"));
            assert!(err.to_string().contains("rejected"), "{err:#}");
            assert!(files_under(&base).is_empty(), "{bad:?} wrote something");
            // The `..` cases would have landed right beside the staging folder.
            assert!(!dest.parent().unwrap().join(format!("{tag}.txt")).exists());
        }
        assert!(!std::env::temp_dir().join(format!("{tag}.txt")).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sevenz_with_a_link_entry_is_rejected() {
        let base = tmp("7z-link");
        let dest = base.join("staged");
        let bytes = sevenz_with(&[
            Entry {
                name: "link",
                data: b"../outside",
                dir: false,
                attrs: Some(0x8000 | (0o120777 << 16)),
            },
            file("readme.txt", b"hi"),
        ]);
        let err = extract_7z_bytes(bytes, &dest).unwrap_err();
        assert!(err.to_string().contains("links are not allowed"), "{err:#}");
        assert!(files_under(&base).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn benign_7z_still_extracts() {
        let base = tmp("7z-ok");
        let dest = base.join("staged");
        let bytes = sevenz_with(&[
            Entry { name: "Track", data: b"", dir: true, attrs: None },
            file("Track/track.pkz", b"PKZ"),
            file("Track\\paints\\a.pnt", b"PNT"),
            file("empty.txt", b""),
        ]);
        extract_7z_bytes(bytes, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("Track/track.pkz")).unwrap(), b"PKZ");
        assert_eq!(std::fs::read(dest.join("Track/paints/a.pnt")).unwrap(), b"PNT");
        assert!(dest.join("empty.txt").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    // ---- rar ----
    //
    // Nothing in the dependency tree writes rar, so the tests build a RAR 4 archive by hand:
    // stored (uncompressed) entries are a marker, a main header, one header per entry
    // followed by its bytes, and an end block — each header guarded by a CRC.

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    /// One header: its CRC16 (the low half of a CRC32 over everything after it), then `body`.
    fn rar_block(body: &[u8]) -> Vec<u8> {
        let mut out = ((crc32(body) & 0xFFFF) as u16).to_le_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    struct RarEntry<'a> {
        name: &'a str,
        data: &'a [u8],
        /// 2 = Windows, 3 = Unix: says how `attr` is read.
        host_os: u8,
        attr: u32,
        dir: bool,
    }

    fn rar_file<'a>(name: &'a str, data: &'a [u8]) -> RarEntry<'a> {
        RarEntry { name, data, host_os: 2, attr: 0x20, dir: false }
    }

    fn rar_with(entries: &[RarEntry]) -> Vec<u8> {
        let mut out = b"Rar!\x1a\x07\x00".to_vec();
        // Main header: type 0x73, no flags, 13 bytes, six reserved bytes.
        let mut main = vec![0x73, 0, 0, 13, 0];
        main.extend_from_slice(&[0; 6]);
        out.extend(rar_block(&main));
        for e in entries {
            let name = e.name.as_bytes();
            let data: &[u8] = if e.dir { b"" } else { e.data };
            // 0x8000: a data area follows. 0x00E0: the "dictionary" bits that mean directory.
            let flags: u16 = 0x8000 | if e.dir { 0x00E0 } else { 0 };
            let size = (32 + name.len()) as u16;
            let mut h = vec![0x74];
            h.extend_from_slice(&flags.to_le_bytes());
            h.extend_from_slice(&size.to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes()); // packed
            h.extend_from_slice(&(data.len() as u32).to_le_bytes()); // unpacked
            h.push(e.host_os);
            h.extend_from_slice(&crc32(data).to_le_bytes());
            h.extend_from_slice(&0x0021_0000u32.to_le_bytes()); // 1980-01-01, DOS format
            h.push(20); // version needed: 2.0
            h.push(0x30); // method: store
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&e.attr.to_le_bytes());
            h.extend_from_slice(name);
            out.extend(rar_block(&h));
            out.extend_from_slice(data);
        }
        out.extend(rar_block(&[0x7B, 0x00, 0x40, 7, 0]));
        out
    }

    fn extract_rar_bytes(bytes: &[u8], base: &Path, dest: &Path) -> Result<()> {
        let archive = base.join("archive.rar");
        std::fs::write(&archive, bytes).unwrap();
        let r = extract_rar(&archive, dest);
        std::fs::remove_file(&archive).unwrap();
        r
    }

    #[test]
    fn benign_rar_still_extracts() {
        let base = tmp("rar-ok");
        let dest = base.join("staged");
        let bytes = rar_with(&[
            RarEntry { name: "Track", data: b"", host_os: 2, attr: 0x10, dir: true },
            rar_file("Track\\track.pkz", b"PKZ"),
            rar_file("Track\\paints\\a.pnt", b"PNT"),
            rar_file("readme.txt", b"hi"),
        ]);
        extract_rar_bytes(&bytes, &base, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("Track/track.pkz")).unwrap(), b"PKZ");
        assert_eq!(std::fs::read(dest.join("Track/paints/a.pnt")).unwrap(), b"PNT");
        assert_eq!(std::fs::read(dest.join("readme.txt")).unwrap(), b"hi");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rar_with_a_hostile_entry_is_rejected_whole() {
        let base = tmp("rar-hostile");
        let tag = format!("frost-guard-rar-{}", std::process::id());
        for (i, bad) in hostile_names(&tag).iter().filter(|n| !n.contains('\0')).enumerate() {
            let stage = base.join(format!("stage{i}"));
            let dest = stage.join("staged");
            let bytes = rar_with(&[rar_file("ok\\first.txt", b"fine"), rar_file(bad, b"nope")]);
            match extract_rar_bytes(&bytes, &base, &dest) {
                Err(err) => {
                    assert!(err.to_string().contains("rejected"), "{bad:?}: {err:#}");
                    assert!(files_under(&stage).is_empty(), "{bad:?} wrote something");
                }
                // The unrar library already rewrites a drive or UNC prefix and a stream colon
                // in the name it reports, leaving a plain relative path — which then lands
                // inside `dest`.
                Ok(()) => {
                    assert!(
                        bad.contains(':') || bad.starts_with("\\\\"),
                        "{bad:?} must be rejected"
                    );
                    let written = files_under(&stage);
                    assert!(!written.is_empty());
                    assert!(written.iter().all(|p| p.starts_with(&dest)), "{written:?}");
                }
            }
            assert!(!dest.parent().unwrap().join(format!("{tag}.txt")).exists());
        }
        assert!(!std::env::temp_dir().join(format!("{tag}.txt")).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rar_with_a_link_entry_is_rejected() {
        let base = tmp("rar-link");
        let dest = base.join("staged");
        let bytes = rar_with(&[
            RarEntry { name: "link", data: b"../outside", host_os: 3, attr: 0o120777, dir: false },
            rar_file("readme.txt", b"hi"),
        ]);
        let err = extract_rar_bytes(&bytes, &base, &dest).unwrap_err();
        assert!(err.to_string().contains("links are not allowed"), "{err:#}");
        assert!(files_under(&base).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A link already sitting in the staging folder is never written through.
    #[cfg(unix)]
    #[test]
    fn an_existing_link_in_staging_is_not_followed() {
        let base = tmp("7z-prelink");
        let dest = base.join("staged");
        let outside = base.join("outside");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, dest.join("ok")).unwrap();
        let bytes = sevenz_with(&[file("ok/pwned.txt", b"nope")]);
        assert!(extract_7z_bytes(bytes, &dest).is_err());
        assert!(!outside.join("pwned.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
