use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::{Component, Path};

const ZIP_LOCAL_MAGIC: [u8; 4] = *b"PK\x03\x04";
const ZIP_EMPTY_MAGIC: [u8; 4] = *b"PK\x05\x06";

/// True only for an ordinary ZIP package. Secured/opaque containers deliberately return false.
pub fn is_plain_zip(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map(|()| matches!(magic, ZIP_LOCAL_MAGIC | ZIP_EMPTY_MAGIC))
        .unwrap_or(false)
}

/// Enumerate normalized file names without inflating payloads.
pub fn entry_names(path: &Path) -> Result<Vec<String>> {
    ensure_plain_path(path)?;
    let file = File::open(path).with_context(|| format!("open {path:?}"))?;
    names_from_archive(zip::ZipArchive::new(file).with_context(|| format!("open zip {path:?}"))?)
}

/// Enumerate normalized file names from an in-memory ordinary ZIP.
pub fn entry_names_bytes(bytes: &[u8]) -> Result<Vec<String>> {
    if !is_plain_zip_bytes(bytes) {
        bail!("unsupported opaque package");
    }
    names_from_archive(zip::ZipArchive::new(Cursor::new(bytes)).context("open in-memory zip")?)
}

/// Read selected files from an ordinary ZIP, bounding every inflated entry.
pub fn read_selected(
    path: &Path,
    max_entry_bytes: u64,
    keep: impl Fn(&str) -> bool + Copy,
) -> Result<Vec<(String, Vec<u8>)>> {
    ensure_plain_path(path)?;
    let file = File::open(path).with_context(|| format!("open {path:?}"))?;
    selected_from_archive(
        zip::ZipArchive::new(file).with_context(|| format!("open zip {path:?}"))?,
        max_entry_bytes,
        keep,
    )
}

/// The in-memory equivalent used for a server package nested inside a distributable package.
pub fn read_selected_bytes(
    bytes: &[u8],
    max_entry_bytes: u64,
    keep: impl Fn(&str) -> bool + Copy,
) -> Result<Vec<(String, Vec<u8>)>> {
    if !is_plain_zip_bytes(bytes) {
        bail!("unsupported opaque package");
    }
    selected_from_archive(
        zip::ZipArchive::new(Cursor::new(bytes)).context("open in-memory zip")?,
        max_entry_bytes,
        keep,
    )
}

fn ensure_plain_path(path: &Path) -> Result<()> {
    if !is_plain_zip(path) {
        bail!("unsupported opaque package: {path:?}");
    }
    Ok(())
}

fn is_plain_zip_bytes(bytes: &[u8]) -> bool {
    bytes
        .get(..4)
        .is_some_and(|magic| magic == ZIP_LOCAL_MAGIC || magic == ZIP_EMPTY_MAGIC)
}

fn names_from_archive<R: Read + Seek>(mut archive: zip::ZipArchive<R>) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let mut folded = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if !entry.is_file() {
            continue;
        }
        let name = safe_name(entry.name())?;
        if !folded.insert(name.to_ascii_lowercase()) {
            bail!("archive contains a case-colliding duplicate entry: {name}");
        }
        names.push(name);
    }
    Ok(names)
}

fn selected_from_archive<R: Read + Seek>(
    mut archive: zip::ZipArchive<R>,
    max_entry_bytes: u64,
    keep: impl Fn(&str) -> bool + Copy,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut folded = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if !entry.is_file() {
            continue;
        }
        let name = safe_name(entry.name())?;
        if !folded.insert(name.to_ascii_lowercase()) {
            bail!("archive contains a case-colliding duplicate entry: {name}");
        }
        if !keep(&name) {
            continue;
        }
        if entry.size() > max_entry_bytes {
            bail!(
                "archive entry {name:?} is {} bytes, over the {max_entry_bytes}-byte limit",
                entry.size()
            );
        }
        let capacity = usize::try_from(entry.size()).context("entry is too large for this host")?;
        let mut bytes = Vec::with_capacity(capacity);
        entry
            .by_ref()
            .take(max_entry_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max_entry_bytes {
            bail!("archive entry {name:?} inflated beyond its declared limit");
        }
        out.push((name, bytes));
    }
    Ok(out)
}

fn safe_name(raw: &str) -> Result<String> {
    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains('\0') {
        bail!("unsafe archive entry name: {raw:?}");
    }
    let path = Path::new(&normalized);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("unsafe archive entry name: {raw:?}");
    }
    if normalized
        .split('/')
        .any(|part| part.is_empty() || part == ".")
    {
        bail!("unsafe archive entry name: {raw:?}");
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn selected_reads_normalize_names_and_verify_payloads() {
        let bytes = package(&[
            ("Track\\Track.trh", b"terrain"),
            ("Track/readme.txt", b"no"),
        ]);
        let selected = read_selected_bytes(&bytes, 32, |name| name.ends_with(".trh")).unwrap();
        assert_eq!(
            selected,
            [("Track/Track.trh".to_string(), b"terrain".to_vec())]
        );
    }

    #[test]
    fn opaque_unsafe_duplicate_and_oversized_inputs_fail_closed() {
        assert!(read_selected_bytes(b"KCOL", 32, |_| true).is_err());
        assert!(read_selected_bytes(&package(&[("../escape", b"x")]), 32, |_| true).is_err());
        assert!(read_selected_bytes(
            &package(&[("Track/A.trh", b"x"), ("track/a.TRH", b"y")]),
            32,
            |_| true
        )
        .is_err());
        assert!(read_selected_bytes(&package(&[("Track/a.trh", b"large")]), 4, |_| true).is_err());
    }
}
