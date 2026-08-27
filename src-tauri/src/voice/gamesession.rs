//! What the game is doing, read out of FrostMod.
//!
//! The app cannot see inside MX Bikes. It doesn't know which server the player joined, who
//! else is on the grid, or where any of them are — and those are exactly the facts voice
//! chat is built out of. FrostMod is in the process and receives all three from the
//! sanctioned plugin API, so it publishes them into a shared block and this reads it.
//!
//! **The server name is the room key.** It is the only identifier every rider on a server
//! has: an address reaches only the ones whose app launched the game with `-directconnect`,
//! and anyone who picked the server from the game's own browser never sees one. Keying on
//! something half the grid cannot compute would put them in two rooms, each convinced it
//! was working.
//!
//! The layout below is a wire contract with `src/session.h` in the frostmod repository. The
//! two ship separately, so the block carries a version and this refuses one it doesn't know
//! rather than reading a field that has moved.

// Only Windows ever opens the mapping, so everywhere else the decoder is reached by the
// tests alone. It still compiles and is still tested there, which is the point: the layout
// is checked on the machine the work is done on, not only on the one that runs the game.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
use std::sync::Mutex;

/// Must match `frostmod::session::kVersion`.
const VERSION: u32 = 1;

/// Must match `frostmod::session::kMaxRiders`.
const MAX_RIDERS: usize = 64;

const RIDER_BYTES: usize = 56;
const RIDERS_AT: usize = 392;
const BLOCK_BYTES: usize = RIDERS_AT + RIDER_BYTES * MAX_RIDERS;

/// Field offsets, named rather than counted, because a wrong one reads plausible garbage.
const OFF_VERSION: usize = 0;
const OFF_SEQ: usize = 4;
const OFF_SERVER_NAME: usize = 8;
const OFF_TRACK_ID: usize = 72;
const OFF_GUID: usize = 176;
const OFF_RIDER_NAME: usize = 280;
const OFF_RACE_NUM: usize = 384;
const OFF_RIDER_COUNT: usize = 388;

/// One rider on the grid, as the game reports them.
#[derive(Debug, Clone, PartialEq)]
pub struct Rider {
    pub race_num: i32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Degrees from north, exactly as the game's SDK gives it. Converted where it is used,
    /// not in transit.
    pub yaw_deg: f32,
    pub crashed: bool,
    pub name: String,
}

/// A snapshot of the session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameSession {
    /// Empty when the game is not in an online session — testing, a replay, single player.
    pub server_name: String,
    pub track_id: String,
    /// Our own GUID. The plugin API exposes nobody else's.
    pub guid: String,
    pub rider_name: String,
    /// Our race number, or -1 before the grid exists.
    pub race_num: i32,
    pub riders: Vec<Rider>,
}

impl GameSession {
    /// Is this a session voice can put a room behind?
    ///
    /// A server name is the whole test: everything without one has nobody to talk to.
    pub fn on_a_server(&self) -> bool {
        !self.server_name.trim().is_empty()
    }

    /// Our race number, or 0 when we don't have one — the value the room takes for "not on
    /// the grid yet", where the block uses -1.
    pub fn race_num_for_room(&self) -> u16 {
        u16::try_from(self.race_num).unwrap_or(0)
    }
}

/// Decode a block, or `None` if it is mid-write, the wrong version, or too short.
///
/// A failure here is ordinary and means "ask again", never "something is wrong": the writer
/// is a game thread updating the block every frame, and catching it mid-update is expected.
pub fn decode(bytes: &[u8]) -> Option<GameSession> {
    if bytes.len() < BLOCK_BYTES {
        return None;
    }
    if u32(bytes, OFF_VERSION) != VERSION {
        return None;
    }
    // Odd means a write is in flight. The caller reads the counter again afterwards; this
    // is the cheap half of the check.
    if u32(bytes, OFF_SEQ) & 1 == 1 {
        return None;
    }

    let count = i32(bytes, OFF_RIDER_COUNT).clamp(0, MAX_RIDERS as i32) as usize;
    let riders = (0..count)
        .map(|i| {
            let at = RIDERS_AT + i * RIDER_BYTES;
            Rider {
                race_num: i32(bytes, at),
                x: f32(bytes, at + 4),
                y: f32(bytes, at + 8),
                z: f32(bytes, at + 12),
                yaw_deg: f32(bytes, at + 16),
                crashed: i32(bytes, at + 20) != 0,
                name: text(bytes, at + 24, 32),
            }
        })
        .collect();

    Some(GameSession {
        server_name: text(bytes, OFF_SERVER_NAME, 64),
        track_id: text(bytes, OFF_TRACK_ID, 104),
        guid: text(bytes, OFF_GUID, 104),
        rider_name: text(bytes, OFF_RIDER_NAME, 104),
        race_num: i32(bytes, OFF_RACE_NUM),
        riders,
    })
}

/// The sequence counter, so a caller can confirm nothing moved while it was copying.
pub fn sequence(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= BLOCK_BYTES).then(|| u32(bytes, OFF_SEQ))
}

fn u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn i32(bytes: &[u8], at: usize) -> i32 {
    u32(bytes, at) as i32
}

fn f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_bits(u32(bytes, at))
}

/// A fixed-width C string. Stops at the first NUL, and never trusts the field to hold one.
fn text(bytes: &[u8], at: usize, len: usize) -> String {
    let field = &bytes[at..at + len];
    let end = field.iter().position(|&b| b == 0).unwrap_or(len);
    // Lossy rather than strict: a rider name is display text, and one odd byte in it must
    // not cost us the whole session.
    String::from_utf8_lossy(&field[..end]).trim().to_string()
}

// ---------------------------------------------------------------------------------------
// The mapping
// ---------------------------------------------------------------------------------------

/// Reads the block FrostMod publishes.
///
/// Holds the mapping open once found: this is polled every few seconds now and will be
/// polled at frame rate for proximity, and re-opening it each time would be the expensive
/// part of a cheap operation.
#[derive(Default)]
pub struct Reader {
    #[cfg(windows)]
    view: Mutex<Option<MappedBlock>>,
}

impl Reader {
    /// The current session, or `None` when FrostMod isn't publishing one.
    ///
    /// `None` covers a rider who hasn't got FrostMod running, a game that isn't started, and
    /// a block caught mid-write. None of those is an error worth reporting — voice simply
    /// isn't joined yet.
    #[cfg(windows)]
    pub fn read(&self) -> Option<GameSession> {
        let mut guard = self.view.lock().ok()?;
        if guard.is_none() {
            *guard = MappedBlock::open();
        }
        let mapped = guard.as_ref()?;

        // Seqlock: copy, then check the counter didn't move. Two attempts is plenty — the
        // writer holds the block for a memcpy and the caller can afford to come back.
        for _ in 0..2 {
            let before = mapped.sequence();
            if before.is_some_and(|s| s & 1 == 0) {
                let copy = mapped.copy();
                if mapped.sequence() == before {
                    if let Some(session) = decode(&copy) {
                        return Some(session);
                    }
                }
            }
        }
        // The block is there but unreadable — a version we don't know, or a writer we keep
        // catching. Drop the mapping so a FrostMod that restarts with a new one is found.
        *guard = None;
        None
    }

    /// No block outside Windows: the game only runs there, and under Proton the app is a
    /// native process outside the prefix FrostMod lives in, so the name would not resolve
    /// even if it did.
    #[cfg(not(windows))]
    pub fn read(&self) -> Option<GameSession> {
        None
    }
}

#[cfg(windows)]
struct MappedBlock {
    handle: *mut std::ffi::c_void,
    view: *const u8,
}

// SAFETY: the pointers are a file mapping and a read-only view of it, both valid for as long
// as this value lives, and nothing here mutates through them.
#[cfg(windows)]
unsafe impl Send for MappedBlock {}

#[cfg(windows)]
mod ffi {
    use std::ffi::c_void;

    extern "system" {
        pub fn OpenFileMappingA(access: u32, inherit: i32, name: *const u8) -> *mut c_void;
        pub fn MapViewOfFile(
            mapping: *mut c_void,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        pub fn UnmapViewOfFile(base: *const c_void) -> i32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub const FILE_MAP_READ: u32 = 0x0004;
}

#[cfg(windows)]
impl MappedBlock {
    /// Must match `frostmod::session::kMappingName`.
    const NAME: &'static [u8] = b"Local\\FrostModSession\0";

    fn open() -> Option<MappedBlock> {
        // SAFETY: a NUL-terminated name; a null return just means FrostMod isn't running.
        let handle = unsafe { ffi::OpenFileMappingA(ffi::FILE_MAP_READ, 0, Self::NAME.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        // SAFETY: `handle` is a mapping we just opened; a null view is a failure we handle.
        let view = unsafe { ffi::MapViewOfFile(handle, ffi::FILE_MAP_READ, 0, 0, BLOCK_BYTES) };
        if view.is_null() {
            // SAFETY: closing a handle we own and are about to drop.
            unsafe { ffi::CloseHandle(handle) };
            return None;
        }
        Some(MappedBlock { handle, view: view as *const u8 })
    }

    fn sequence(&self) -> Option<u32> {
        sequence(&self.copy_header())
    }

    /// The whole block. Copied rather than read in place, so the seqlock check afterwards
    /// is checking something that cannot change under us.
    fn copy(&self) -> Vec<u8> {
        // SAFETY: the view is at least BLOCK_BYTES — the mapping was created that size by
        // FrostMod and requested that size here — and is valid for this object's lifetime.
        unsafe { std::slice::from_raw_parts(self.view, BLOCK_BYTES).to_vec() }
    }

    /// Just enough for the counter, so the cheap check stays cheap.
    fn copy_header(&self) -> Vec<u8> {
        let mut header = vec![0u8; BLOCK_BYTES];
        // SAFETY: as `copy`, and only the first bytes are read by `sequence`.
        unsafe {
            std::ptr::copy_nonoverlapping(self.view, header.as_mut_ptr(), 8);
        }
        header
    }
}

#[cfg(windows)]
impl Drop for MappedBlock {
    fn drop(&mut self) {
        // SAFETY: both were produced by the matching calls in `open` and are dropped once.
        unsafe {
            ffi::UnmapViewOfFile(self.view as *const std::ffi::c_void);
            ffi::CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a block the way `src/session.h` lays one out. Written by hand rather than by
    /// mirroring a Rust struct: the point is to check this decoder against the C layout,
    /// and a shared definition would agree with itself no matter what frostmod does.
    struct BlockBuilder(Vec<u8>);

    impl BlockBuilder {
        fn new() -> BlockBuilder {
            let mut bytes = vec![0u8; BLOCK_BYTES];
            bytes[OFF_VERSION..OFF_VERSION + 4].copy_from_slice(&VERSION.to_le_bytes());
            bytes[OFF_RACE_NUM..OFF_RACE_NUM + 4].copy_from_slice(&(-1i32).to_le_bytes());
            BlockBuilder(bytes)
        }
        fn text(mut self, at: usize, len: usize, value: &str) -> Self {
            let value = value.as_bytes();
            let n = value.len().min(len - 1);
            self.0[at..at + n].copy_from_slice(&value[..n]);
            self
        }
        fn i32(mut self, at: usize, value: i32) -> Self {
            self.0[at..at + 4].copy_from_slice(&value.to_le_bytes());
            self
        }
        fn f32(mut self, at: usize, value: f32) -> Self {
            self.0[at..at + 4].copy_from_slice(&value.to_le_bytes());
            self
        }
        fn rider(self, index: usize, race_num: i32, x: f32, yaw: f32, name: &str) -> Self {
            let at = RIDERS_AT + index * RIDER_BYTES;
            self.i32(at, race_num)
                .f32(at + 4, x)
                .f32(at + 16, yaw)
                .text(at + 24, 32, name)
        }
        fn build(self) -> Vec<u8> {
            self.0
        }
    }

    fn a_session() -> Vec<u8> {
        BlockBuilder::new()
            .text(OFF_SERVER_NAME, 64, "Frost Racing EU")
            .text(OFF_TRACK_ID, 104, "practice_track")
            .text(OFF_GUID, 104, "abc-123")
            .text(OFF_RIDER_NAME, 104, "Frost")
            .i32(OFF_RACE_NUM, 7)
            .i32(OFF_RIDER_COUNT, 2)
            .rider(0, 7, 1.5, 90.0, "Frost")
            .rider(1, 22, -3.0, 180.0, "Ryan")
            .build()
    }

    #[test]
    fn reads_a_session_the_way_frostmod_writes_one() {
        let session = decode(&a_session()).expect("a session");
        assert_eq!(session.server_name, "Frost Racing EU");
        assert_eq!(session.track_id, "practice_track");
        assert_eq!(session.guid, "abc-123");
        assert_eq!(session.rider_name, "Frost");
        assert_eq!(session.race_num, 7);
        assert_eq!(session.riders.len(), 2);
        assert_eq!(session.riders[1].race_num, 22);
        assert_eq!(session.riders[1].x, -3.0);
        assert_eq!(session.riders[1].yaw_deg, 180.0);
        assert_eq!(session.riders[1].name, "Ryan");
    }

    #[test]
    fn a_server_name_is_what_makes_it_joinable() {
        let session = decode(&a_session()).expect("a session");
        assert!(session.on_a_server());
        // Testing, replays and single player leave it empty — nobody to talk to.
        let alone = decode(&BlockBuilder::new().build()).expect("a block");
        assert!(!alone.on_a_server());
    }

    #[test]
    fn refuses_a_block_that_is_being_written() {
        let mut bytes = a_session();
        bytes[OFF_SEQ] = 1;
        assert_eq!(decode(&bytes), None);
    }

    #[test]
    fn refuses_a_layout_it_does_not_know() {
        let mut bytes = a_session();
        bytes[OFF_VERSION] = (VERSION + 1) as u8;
        assert_eq!(decode(&bytes), None, "a newer frostmod must not be misread");
    }

    #[test]
    fn a_short_or_empty_block_is_none_rather_than_a_panic() {
        let bytes = a_session();
        for n in [0, 1, 8, RIDERS_AT, BLOCK_BYTES - 1] {
            assert_eq!(decode(&bytes[..n]), None, "{n} bytes should not decode");
        }
    }

    #[test]
    fn a_rider_count_beyond_the_table_is_clamped_not_trusted() {
        // The block is written by another process; a count of 4 billion must not become a
        // read past the end of the mapping.
        let bytes = BlockBuilder::new().i32(OFF_RIDER_COUNT, i32::MAX).build();
        assert_eq!(decode(&bytes).expect("a session").riders.len(), MAX_RIDERS);
        let negative = BlockBuilder::new().i32(OFF_RIDER_COUNT, -5).build();
        assert!(negative.len() == BLOCK_BYTES && decode(&negative).expect("a session").riders.is_empty());
    }

    #[test]
    fn a_field_with_no_terminator_stops_at_the_field() {
        let bytes = BlockBuilder::new()
            .text(OFF_SERVER_NAME, 65, &"x".repeat(64))
            .build();
        assert_eq!(decode(&bytes).expect("a session").server_name.len(), 64);
    }

    #[test]
    fn no_race_number_yet_reads_as_none_of_the_grid() {
        // -1 in the block is "no grid"; the room speaks 0.
        let session = decode(&BlockBuilder::new().build()).expect("a session");
        assert_eq!(session.race_num, -1);
        assert_eq!(session.race_num_for_room(), 0);
    }

    #[test]
    fn the_layout_matches_the_one_frostmod_asserts() {
        // These four numbers are static_asserted in src/session.h. If either side moves,
        // this is the test that says so before a rider is heard from the wrong place.
        assert_eq!(RIDER_BYTES, 56);
        assert_eq!(BLOCK_BYTES, 3976);
        assert_eq!(OFF_SERVER_NAME, 8);
        assert_eq!(RIDERS_AT, 392);
    }
}
