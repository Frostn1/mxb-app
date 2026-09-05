//! Who is this, when nothing on disk will say?
//!
//! A DLL that arrives through `LoadLibrary` is a file: it has a path, a name, a hash, and a
//! signature, and [`crate::procmods`] reports all four. A DLL that arrives by being written
//! into the game's address space and started by hand has none of them. It is a run of bytes
//! at an address, and the loader has never heard of it.
//!
//! It is still a PE image, though, because that is what the code was compiled as and what
//! its own loader stub needs it to be. So the headers are there in memory, and they carry
//! three things worth having:
//!
//!   * the **export directory's name** — what the DLL calls itself, which for anything built
//!     in the ordinary way is the file name it was linked as,
//!   * the **CodeView record's PDB path** — the debug file the linker expected to sit beside
//!     it, which is very often the project's own name and survives a rename of everything
//!     else,
//!   * the **build's shape** — timestamp, image size, and the section table — which is
//!     identical on every machine running that build and different on every other build.
//!
//! The third is what [`PeIdent::fingerprint`] is for. Hashing the region itself would not do:
//! a mapped image has had relocations applied against wherever it happened to land, so the
//! same payload hashes differently on two machines and the hash identifies nothing. The
//! header fields do not move.
//!
//! Only the PDB's file name is kept, never its directory. A real one reads
//! `C:\Users\<somebody>\source\repos\...` and the part before the last slash is a person's
//! name and a folder layout that is none of our business.
//!
//! Nothing here decides anything. It reads bytes and says what they claim to be, exactly as
//! [`crate::fileinfo`] does for a file on disk, and the judgement is the control plane's.

use serde::Serialize;

/// The most of a PDB path or an export name we will carry. Both are file names.
const MAX_NAME: usize = 96;

/// A cap on the section table. Real images have a handful; a number larger than this is a
/// corrupt header or a deliberate one, and either way not worth walking.
const MAX_SECTIONS: usize = 32;

/// What a mapped image says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeIdent {
    /// The name from the export directory, lowercased. Empty when there is no export table,
    /// which is ordinary for an executable and unusual for a DLL.
    pub name: String,
    /// The file name of the PDB the linker recorded, lowercased. Never its folder.
    pub pdb: String,
    /// The linker's timestamp. Part of the fingerprint; on its own it is a claim like any
    /// other, and a build can set it to whatever it likes.
    pub timestamp: u32,
    /// `SizeOfImage` — how much address space the image asked for.
    pub size_of_image: u32,
    /// Section names and virtual sizes, in header order.
    pub sections: Vec<Section>,
    /// The image is marked as a DLL rather than an executable.
    pub is_dll: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub name: String,
    pub size: u32,
}

impl PeIdent {
    /// A stable identifier for this build, as lowercase hex.
    ///
    /// Over the header fields only, and deliberately: they are the same bytes in every copy
    /// of a build, on every machine, whatever address it was mapped at. Two clients that
    /// report the same fingerprint are running the same payload, and that is the whole
    /// question — it is what lets one rule name something nobody has a file for.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.name.as_bytes());
        hasher.update([0]);
        hasher.update(self.pdb.as_bytes());
        hasher.update([0]);
        hasher.update(self.timestamp.to_le_bytes());
        hasher.update(self.size_of_image.to_le_bytes());
        hasher.update([u8::from(self.is_dll)]);
        for section in &self.sections {
            hasher.update(section.name.as_bytes());
            hasher.update([0]);
            hasher.update(section.size.to_le_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Is there anything here worth reporting? A header we could read nothing out of is not
    /// evidence of anything, and a fingerprint over five zeroes would be the same on every
    /// machine in the world.
    pub fn is_useful(&self) -> bool {
        !self.name.is_empty() || !self.pdb.is_empty() || !self.sections.is_empty()
    }
}

/// Read a mapped PE image through `read`, which answers with the bytes at an RVA.
///
/// `read(rva, len)` is relative to the image base, because that is what the headers are in
/// terms of and it keeps every address in this file a small number. It may answer with fewer
/// bytes than asked for, or with nothing; every read here is treated as allowed to fail,
/// since the thing being parsed is a header in another process that may be a lie, a
/// half-written image, or unmapped by the time we reach it.
///
/// `None` when the bytes are not a PE image at all.
pub fn identify(read: &dyn Fn(u64, usize) -> Option<Vec<u8>>) -> Option<PeIdent> {
    let dos = read(0, 0x40)?;
    if dos.len() < 0x40 || &dos[0..2] != b"MZ" {
        return None;
    }
    let pe_off = u32::from_le_bytes(dos[0x3c..0x40].try_into().ok()?) as u64;
    // A PE header further into the image than this is not one. The bound is what keeps a
    // made-up `e_lfanew` from turning into a read at an arbitrary offset.
    if pe_off < 0x40 || pe_off > 0x1000 {
        return None;
    }

    // Signature, file header, and the whole optional header in one read.
    let head = read(pe_off, 24 + 240)?;
    if head.len() < 24 || &head[0..4] != b"PE\0\0" {
        return None;
    }
    let sections_count = u16::from_le_bytes(head[6..8].try_into().ok()?) as usize;
    let timestamp = u32::from_le_bytes(head[8..12].try_into().ok()?);
    let opt_size = u16::from_le_bytes(head[20..22].try_into().ok()?) as usize;
    let characteristics = u16::from_le_bytes(head[22..24].try_into().ok()?);
    let opt = head.get(24..)?;
    if opt.len() < 68 {
        return None;
    }
    let magic = u16::from_le_bytes(opt[0..2].try_into().ok()?);
    // 0x10b is PE32, 0x20b is PE32+. Anything else is not an image we can read, and a ROM
    // image (0x107) is not something that turns up in a game process.
    let dirs_at = match magic {
        0x10b => 96,
        0x20b => 112,
        _ => return None,
    };
    let size_of_image = u32::from_le_bytes(opt[56..60].try_into().ok()?);
    let is_dll = characteristics & 0x2000 != 0;

    let mut ident = PeIdent {
        timestamp,
        size_of_image,
        is_dll,
        ..PeIdent::default()
    };

    // Section table: immediately after the optional header, however long the header said it
    // was. Read from the image's own claim rather than from the magic, so a header padded
    // out to an unusual length is still walked correctly.
    let table_at = pe_off + 24 + opt_size as u64;
    let wanted = sections_count.min(MAX_SECTIONS);
    if wanted > 0 {
        if let Some(table) = read(table_at, wanted * 40) {
            for entry in table.chunks_exact(40).take(wanted) {
                ident.sections.push(Section {
                    name: ascii_name(&entry[0..8]),
                    size: u32::from_le_bytes(entry[8..12].try_into().unwrap_or_default()),
                });
            }
        }
    }

    let dir = |index: usize| -> Option<(u64, u32)> {
        let at = dirs_at + index * 8;
        let bytes = opt.get(at..at + 8)?;
        let rva = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let size = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        (rva > 0).then_some((rva as u64, size))
    };

    if let Some((rva, _)) = dir(0) {
        ident.name = export_name(read, rva).unwrap_or_default();
    }
    if let Some((rva, size)) = dir(6) {
        ident.pdb = pdb_name(read, rva, size).unwrap_or_default();
    }
    Some(ident)
}

/// The name out of the export directory: a `Name` RVA at offset 12, pointing at a C string.
fn export_name(read: &dyn Fn(u64, usize) -> Option<Vec<u8>>, rva: u64) -> Option<String> {
    let dir = read(rva, 16)?;
    let name_rva = u32::from_le_bytes(dir.get(12..16)?.try_into().ok()?) as u64;
    if name_rva == 0 {
        return None;
    }
    let text = read(name_rva, MAX_NAME)?;
    Some(file_name(&cstr(&text)))
}

/// The PDB path out of the debug directory's CodeView record.
///
/// The directory is an array of 28-byte entries; type 2 is CodeView, and its data starts
/// `RSDS`, a GUID and an age, with the path as a C string after them.
fn pdb_name(
    read: &dyn Fn(u64, usize) -> Option<Vec<u8>>,
    rva: u64,
    size: u32,
) -> Option<String> {
    let count = (size as usize / 28).min(16);
    let table = read(rva, count * 28)?;
    for entry in table.chunks_exact(28).take(count) {
        if u32::from_le_bytes(entry[12..16].try_into().ok()?) != 2 {
            continue;
        }
        let data_rva = u32::from_le_bytes(entry[20..24].try_into().ok()?) as u64;
        if data_rva == 0 {
            continue;
        }
        let record = read(data_rva, 24 + MAX_NAME)?;
        if record.len() < 24 || &record[0..4] != b"RSDS" {
            continue;
        }
        let name = file_name(&cstr(&record[24..]));
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// A NUL-terminated string out of a byte run, as far as it goes.
fn cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// A fixed-width ASCII field — a section name — trimmed of its padding.
fn ascii_name(bytes: &[u8]) -> String {
    cstr(bytes).trim().to_ascii_lowercase()
}

/// The last component of a path, lowercased, and only if it looks like a file name.
///
/// This is where a PDB path stops being a path: everything before the last separator is
/// discarded, and what is left has to be shaped like the file names the control plane
/// accepts or it is dropped. A name that arrives shaped like something else is not worth
/// having a special case for.
fn file_name(path: &str) -> String {
    let tail = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let name = tail.trim().to_ascii_lowercase();
    if name.is_empty() || name.len() > MAX_NAME || !name.contains('.') {
        return String::new();
    }
    if name.starts_with('.') || name.ends_with('.') {
        return String::new();
    }
    let shaped = name
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'+' | b'(' | b')' | b'-'));
    if shaped {
        name
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PE_OFF: usize = 0x80;
    const OPT_OFF: usize = PE_OFF + 24;
    const OPT_SIZE: usize = 240;
    const EXPORT_RVA: usize = 0x1000;
    const EXPORT_NAME_RVA: usize = 0x1020;
    const DEBUG_RVA: usize = 0x1100;
    const CODEVIEW_RVA: usize = 0x1140;

    /// A PE32+ image the shape a mapped one has: headers at the front, an export directory
    /// and a CodeView record further in, at their own RVAs.
    fn image(export: &str, pdb: &str) -> Vec<u8> {
        let mut img = vec![0u8; 0x2000];
        img[0..2].copy_from_slice(b"MZ");
        img[0x3c..0x40].copy_from_slice(&(PE_OFF as u32).to_le_bytes());

        img[PE_OFF..PE_OFF + 4].copy_from_slice(b"PE\0\0");
        img[PE_OFF + 4..PE_OFF + 6].copy_from_slice(&0x8664u16.to_le_bytes()); // machine
        img[PE_OFF + 6..PE_OFF + 8].copy_from_slice(&2u16.to_le_bytes()); // sections
        img[PE_OFF + 8..PE_OFF + 12].copy_from_slice(&0x6512_3456u32.to_le_bytes()); // stamp
        img[PE_OFF + 20..PE_OFF + 22].copy_from_slice(&(OPT_SIZE as u16).to_le_bytes());
        img[PE_OFF + 22..PE_OFF + 24].copy_from_slice(&0x2000u16.to_le_bytes()); // is a DLL

        img[OPT_OFF..OPT_OFF + 2].copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
        img[OPT_OFF + 56..OPT_OFF + 60].copy_from_slice(&0x0002_0000u32.to_le_bytes());
        let dirs = OPT_OFF + 112;
        if !export.is_empty() {
            img[dirs..dirs + 4].copy_from_slice(&(EXPORT_RVA as u32).to_le_bytes());
            img[dirs + 4..dirs + 8].copy_from_slice(&64u32.to_le_bytes());
        }
        if !pdb.is_empty() {
            img[dirs + 48..dirs + 52].copy_from_slice(&(DEBUG_RVA as u32).to_le_bytes());
            img[dirs + 52..dirs + 56].copy_from_slice(&28u32.to_le_bytes());
        }

        let table = OPT_OFF + OPT_SIZE;
        img[table..table + 5].copy_from_slice(b".text");
        img[table + 8..table + 12].copy_from_slice(&0x1234u32.to_le_bytes());
        img[table + 40..table + 46].copy_from_slice(b".rdata");
        img[table + 48..table + 52].copy_from_slice(&0x0567u32.to_le_bytes());

        if !export.is_empty() {
            img[EXPORT_RVA + 12..EXPORT_RVA + 16]
                .copy_from_slice(&(EXPORT_NAME_RVA as u32).to_le_bytes());
            img[EXPORT_NAME_RVA..EXPORT_NAME_RVA + export.len()]
                .copy_from_slice(export.as_bytes());
        }
        if !pdb.is_empty() {
            img[DEBUG_RVA + 12..DEBUG_RVA + 16].copy_from_slice(&2u32.to_le_bytes());
            img[DEBUG_RVA + 20..DEBUG_RVA + 24]
                .copy_from_slice(&(CODEVIEW_RVA as u32).to_le_bytes());
            img[CODEVIEW_RVA..CODEVIEW_RVA + 4].copy_from_slice(b"RSDS");
            img[CODEVIEW_RVA + 24..CODEVIEW_RVA + 24 + pdb.len()].copy_from_slice(pdb.as_bytes());
        }
        img
    }

    fn reader(img: Vec<u8>) -> impl Fn(u64, usize) -> Option<Vec<u8>> {
        move |rva, len| {
            let at = rva as usize;
            if at >= img.len() {
                return None;
            }
            let end = (at + len).min(img.len());
            Some(img[at..end].to_vec())
        }
    }

    #[test]
    fn a_mapped_image_names_itself() {
        let ident = identify(&reader(image(
            "kaizo.dll",
            r"C:\Users\somebody\source\repos\Kaizo\x64\Release\Kaizo.pdb",
        )))
        .unwrap();
        assert_eq!(ident.name, "kaizo.dll");
        assert_eq!(ident.pdb, "kaizo.pdb");
        assert!(ident.is_dll);
        assert_eq!(ident.timestamp, 0x6512_3456);
        assert_eq!(ident.size_of_image, 0x0002_0000);
    }

    #[test]
    fn a_pdb_path_never_carries_the_folder_it_was_built_in() {
        let ident = identify(&reader(image(
            "x.dll",
            r"C:\Users\a-real-persons-name\src\thing.pdb",
        )))
        .unwrap();
        assert_eq!(ident.pdb, "thing.pdb");
        assert!(!ident.pdb.contains('\\'), "{:?}", ident.pdb);
        assert!(!ident.pdb.contains("real"), "{:?}", ident.pdb);
    }

    #[test]
    fn the_section_table_is_read_in_header_order() {
        let ident = identify(&reader(image("a.dll", ""))).unwrap();
        let names: Vec<&str> = ident.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec![".text", ".rdata"]);
        assert_eq!(ident.sections[0].size, 0x1234);
        assert_eq!(ident.sections[1].size, 0x0567);
    }

    #[test]
    fn bytes_that_are_not_an_image_are_not_one() {
        let junk = vec![0xccu8; 0x2000];
        assert!(identify(&reader(junk)).is_none());
    }

    #[test]
    fn a_header_pointing_off_into_nowhere_is_refused() {
        let mut img = image("a.dll", "");
        img[0x3c..0x40].copy_from_slice(&0x00ff_ffffu32.to_le_bytes());
        assert!(identify(&reader(img)).is_none());
    }

    #[test]
    fn a_truncated_read_costs_a_field_and_not_the_answer() {
        // Everything past the section table is unreadable — the shape of a region that was
        // unmapped, or shrank, between the walk and the read.
        let img = image("kaizo.dll", r"C:\x\kaizo.pdb");
        let ident = identify(&|rva, len| {
            if rva >= 0x1000 {
                return None;
            }
            let at = rva as usize;
            let end = (at + len).min(img.len());
            Some(img[at..end].to_vec())
        })
        .unwrap();
        assert_eq!(ident.name, "");
        assert_eq!(ident.pdb, "");
        assert_eq!(ident.sections.len(), 2);
        assert!(ident.is_useful());
    }

    #[test]
    fn the_fingerprint_is_the_build_and_not_the_address_it_landed_at() {
        // The same build read twice — the only thing a relocation would change is content,
        // and the fingerprint is over the header fields, which do not move.
        let a = identify(&reader(image("kaizo.dll", r"C:\a\kaizo.pdb"))).unwrap();
        let b = identify(&reader(image("kaizo.dll", r"D:\somewhere-else\kaizo.pdb"))).unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn a_different_build_fingerprints_differently() {
        let a = identify(&reader(image("kaizo.dll", r"C:\a\kaizo.pdb"))).unwrap();
        let mut img = image("kaizo.dll", r"C:\a\kaizo.pdb");
        img[PE_OFF + 8..PE_OFF + 12].copy_from_slice(&0x7000_0000u32.to_le_bytes());
        let b = identify(&reader(img)).unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn nothing_recovered_is_nothing_worth_reporting() {
        let mut img = vec![0u8; 0x2000];
        img[0..2].copy_from_slice(b"MZ");
        img[0x3c..0x40].copy_from_slice(&(PE_OFF as u32).to_le_bytes());
        img[PE_OFF..PE_OFF + 4].copy_from_slice(b"PE\0\0");
        img[PE_OFF + 20..PE_OFF + 22].copy_from_slice(&(OPT_SIZE as u16).to_le_bytes());
        img[OPT_OFF..OPT_OFF + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        let ident = identify(&reader(img)).unwrap();
        assert!(!ident.is_useful());
    }
}
