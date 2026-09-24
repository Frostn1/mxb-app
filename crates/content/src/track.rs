use crate::{
    entry_names, read_selected, read_selected_bytes, trh_descriptor, RdfBootstrap, TrhDescriptor,
};
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const WORLD_BLOCK_SIDE: u32 = 64;
const MAX_NESTED_PACKAGE: u64 = 512 * 1024 * 1024;
const MAX_TERRAIN: u64 = 256 * 1024 * 1024;
const MAX_RDF: u64 = 4 * 1024 * 1024;

#[derive(Debug)]
enum Source {
    File(PathBuf),
    Memory(Vec<u8>),
}

/// A server-version track package resolved far enough to build capture-free bootstrap data.
#[derive(Debug)]
pub struct TrackPackage {
    source: Source,
    pub id: String,
    pub entries: Vec<String>,
    pub ini_entry: String,
    pub rdf_entry: String,
    pub trh_entry: String,
    pub terrain: TrhDescriptor,
}

impl TrackPackage {
    /// Open a server package directly, or the single `*server.pkz` nested in a distribution.
    pub fn open(path: &Path) -> Result<Self> {
        let names = entry_names(path)?;
        if has_track_triplet(&names) {
            return Self::from_source(Source::File(path.to_path_buf()), names);
        }

        let nested: Vec<_> = names
            .iter()
            .filter(|name| name.to_ascii_lowercase().ends_with("server.pkz"))
            .cloned()
            .collect();
        if nested.len() != 1 {
            bail!(
                "package contains no direct server track and {} nested server packages",
                nested.len()
            );
        }
        let wanted = &nested[0];
        let (_, bytes) = read_selected(path, MAX_NESTED_PACKAGE, |name| {
            name.eq_ignore_ascii_case(wanted)
        })?
        .into_iter()
        .next()
        .context("nested server package disappeared while reading")?;
        let names = crate::pkz::entry_names_bytes(&bytes)?;
        Self::from_source(Source::Memory(bytes), names)
    }

    pub fn world_block_grid(&self) -> (u32, u32) {
        (
            self.terrain.width.div_ceil(WORLD_BLOCK_SIDE),
            self.terrain.height.div_ceil(WORLD_BLOCK_SIDE),
        )
    }

    /// Read and strictly parse the race metadata used by a fresh server bootstrap.
    pub fn rdf_bootstrap(&self) -> Result<RdfBootstrap> {
        let rdf = self
            .read_selected(MAX_RDF, |name| name.eq_ignore_ascii_case(&self.rdf_entry))?
            .into_iter()
            .next()
            .context("RDF entry disappeared while reading")?
            .1;
        let text = std::str::from_utf8(&rdf).context("track RDF is not UTF-8")?;
        RdfBootstrap::parse(text).context("track has invalid bootstrap RDF data")
    }

    /// Read selected entries from the already-resolved server package.
    pub fn read_selected(
        &self,
        max_entry_bytes: u64,
        keep: impl Fn(&str) -> bool + Copy,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        match &self.source {
            Source::File(path) => read_selected(path, max_entry_bytes, keep),
            Source::Memory(bytes) => read_selected_bytes(bytes, max_entry_bytes, keep),
        }
    }

    fn from_source(source: Source, names: Vec<String>) -> Result<Self> {
        if !has_track_triplet(&names) {
            bail!("server package must contain .ini, .rdf, and .trh track entries");
        }
        let roots: BTreeSet<_> = names
            .iter()
            .filter_map(|name| name.split_once('/').map(|(root, _)| root.to_string()))
            .collect();
        if roots.len() != 1 || names.iter().any(|name| !name.contains('/')) {
            bail!("server package must contain exactly one top-level track folder");
        }
        let id = roots.into_iter().next().expect("one root");
        // Community packages can carry helper files (generator settings, alternate layouts)
        // beside the track's own `<folder>/<folder>.<ext>` entry, which is the one to use.
        let unique = |extension: &str| -> Result<String> {
            let matches: Vec<_> = names
                .iter()
                .filter(|name| extension_of(name).eq_ignore_ascii_case(extension))
                .cloned()
                .collect();
            if matches.len() == 1 {
                return Ok(matches[0].clone());
            }
            let canonical = format!("{id}/{id}.{extension}");
            let named: Vec<_> = matches
                .iter()
                .filter(|name| name.eq_ignore_ascii_case(&canonical))
                .collect();
            if named.len() != 1 {
                bail!(
                    "server package has {} .{extension} entries and none is exactly {canonical:?}",
                    matches.len()
                );
            }
            Ok(named[0].clone())
        };
        let ini_entry = unique("ini")?;
        let rdf_entry = unique("rdf")?;
        let trh_entry = unique("trh")?;
        let terrain_bytes = match &source {
            Source::File(path) => read_selected(path, MAX_TERRAIN, |name| {
                name.eq_ignore_ascii_case(&trh_entry)
            })?,
            Source::Memory(bytes) => read_selected_bytes(bytes, MAX_TERRAIN, |name| {
                name.eq_ignore_ascii_case(&trh_entry)
            })?,
        }
        .into_iter()
        .next()
        .context("TRH entry disappeared while reading")?
        .1;
        let terrain = trh_descriptor(&terrain_bytes).context("track has an invalid TRH")?;
        Ok(Self {
            source,
            id,
            entries: names,
            ini_entry,
            rdf_entry,
            trh_entry,
            terrain,
        })
    }
}

fn has_track_triplet(names: &[String]) -> bool {
    ["ini", "rdf", "trh"].into_iter().all(|wanted| {
        names
            .iter()
            .any(|name| extension_of(name).eq_ignore_ascii_case(wanted))
    })
}

fn extension_of(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn trh(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TRH\0");
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.resize(12 + width as usize * height as usize * 2, 0);
        bytes
    }

    fn package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn write_temp(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(bytes).unwrap();
        file
    }

    #[test]
    fn opens_direct_and_nested_server_packages_with_the_same_reader() {
        let terrain = trh(65, 33);
        let server = package(&[
            ("Track/Track.ini", b"ini"),
            ("Track/Track.rdf", b"rdf"),
            ("Track/Track.trh", &terrain),
        ]);
        let direct = write_temp(&server);
        let direct = TrackPackage::open(direct.path()).unwrap();
        assert_eq!(direct.id, "Track");
        assert_eq!(direct.world_block_grid(), (2, 1));

        let outer = package(&[("Track/Trackserver.pkz", &server)]);
        let outer = write_temp(&outer);
        let nested = TrackPackage::open(outer.path()).unwrap();
        assert_eq!(nested.id, direct.id);
        assert_eq!(nested.world_block_grid(), direct.world_block_grid());
    }

    #[test]
    fn prefers_the_folder_named_entry_when_helpers_share_an_extension() {
        let file = write_temp(&package(&[
            ("Track/generator.ini", b"ini"),
            ("Track/track.INI", b"ini"),
            ("Track/Alternate.rdf", b"rdf"),
            ("Track/Track.rdf", b"rdf"),
            ("Track/Track.trh", &trh(32, 32)),
        ]));
        let track = TrackPackage::open(file.path()).unwrap();
        assert_eq!(track.ini_entry, "Track/track.INI");
        assert_eq!(track.rdf_entry, "Track/Track.rdf");
        assert_eq!(track.trh_entry, "Track/Track.trh");
    }

    #[test]
    fn ties_need_one_exact_folder_named_entry() {
        let file = write_temp(&package(&[
            ("Track/generator.ini", b"ini"),
            ("Track/Other.ini", b"ini"),
            ("Track/Track.rdf", b"rdf"),
            ("Track/Track.trh", &trh(32, 32)),
        ]));
        let error = TrackPackage::open(file.path()).unwrap_err();
        assert!(format!("{error:#}").contains("2 .ini entries"));

        let nested = write_temp(&package(&[
            ("Track/Track.ini", b"ini"),
            ("Track/Track.rdf", b"rdf"),
            ("Track/Track.trh", &trh(32, 32)),
            ("Track/sub/Track.trh", &trh(32, 32)),
        ]));
        let track = TrackPackage::open(nested.path()).unwrap();
        assert_eq!(track.trh_entry, "Track/Track.trh");
    }

    #[test]
    fn rejects_missing_parts_multiple_roots_and_ambiguous_nested_packages() {
        let missing = write_temp(&package(&[("Track/Track.trh", &trh(32, 32))]));
        assert!(TrackPackage::open(missing.path()).is_err());

        let split = write_temp(&package(&[
            ("A/A.ini", b"ini"),
            ("B/B.rdf", b"rdf"),
            ("A/A.trh", &trh(32, 32)),
        ]));
        assert!(TrackPackage::open(split.path()).is_err());

        let server = package(&[
            ("Track/Track.ini", b"ini"),
            ("Track/Track.rdf", b"rdf"),
            ("Track/Track.trh", &trh(32, 32)),
        ]);
        let ambiguous = write_temp(&package(&[
            ("A/Aserver.pkz", &server),
            ("B/Bserver.pkz", &server),
        ]));
        assert!(TrackPackage::open(ambiguous.path()).is_err());
    }
}
