//! The world-server browser: PiBoSo's master-server protocol, spoken from the app.
//!
//! LOCAL-ONLY. Gitignored and gated behind `cfg(worldnet)` (the file's presence flips the
//! cfg in `build.rs`), exactly like `sidecar`. The public tree has neither the file nor the
//! feature, so the master protocol — the cipher key, the framing, the Steam handshake — never
//! ships in the open source. The committed side is only the `list_master_servers` command and
//! the [`crate::WorldServer`] DTO, which carry no protocol detail.
//!
//! Everything here was reverse-engineered from `mxbikes.exe`:
//!
//! - **Transport** is UDP to the `[master] server` address in `mxbikes.ini` (default
//!   `master.mx-bikes.com:54200`, up to ten). Read by [`crate::config::master_servers`].
//! - **Cipher** is Blowfish (big-endian, the standard byte order — the exe byte-swaps each
//!   32-bit half before/after a block, which is the same thing) with the exe's literal 32-byte
//!   key. Each datagram is one message, padded to 8 bytes with `0x0A` and encrypted whole.
//! - **Framing**: a text tag then fields. Strings (and ints, written as `%d` text) are
//!   `0x0A`-terminated — the same byte as the pad, so a trailing pad reads as an empty field.
//!   Binary fields (addresses, the per-record counters) are little-endian where multi-byte.
//! - **Listing**: the game has a no-auth *browse* mode (core command `0x380` → state 3) that
//!   sends `GETLIST` with no `LOGIN`, and a *join* mode (state 2) that `LOGIN`s with a Steam
//!   auth ticket first. We try browse first — if the master answers, no Steam is needed — and
//!   fall back to the ticket `LOGIN` only when it doesn't.
//!
//! Set `MXB_WORLDNET_DEBUG=1` to log each decrypted reply as hex; the meaning of a couple of
//! the per-record bytes is inferred from the read sequence and confirmed against a live
//! capture, not from any spec.

use crate::WorldServer;
use blowfish::Blowfish;
use cipher::generic_array::GenericArray;
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// The 32-byte ASCII key the exe builds on the stack and hands to the Blowfish schedule.
const KEY: &[u8; 32] = b"stIA1OatIev9evlABlaTroaSpletoAtr";
/// Field terminator and block pad, both `0x0A`.
const NL: u8 = 0x0A;
/// The exe's per-record struct is 0x1d8 bytes; the readable payload never approaches that, but
/// the guard keeps a hostile reply from steering the parser.
const MAX_REPLY: usize = 64 * 1024;

fn blowfish() -> Blowfish {
    Blowfish::new_from_slice(KEY).expect("32-byte key is a valid Blowfish key")
}

/// Pad to a multiple of 8 with `0x0A`, then Blowfish-encrypt each block. The pad byte is the
/// field terminator, so it can't corrupt a message that already ended on one.
fn encrypt(mut buf: Vec<u8>) -> Vec<u8> {
    while buf.len() % 8 != 0 {
        buf.push(NL);
    }
    let bf = blowfish();
    for block in buf.chunks_exact_mut(8) {
        let mut ga = GenericArray::clone_from_slice(block);
        bf.encrypt_block(&mut ga);
        block.copy_from_slice(&ga);
    }
    buf
}

/// Decrypt whole 8-byte blocks; a short tail (there shouldn't be one) is dropped.
fn decrypt(buf: &[u8]) -> Vec<u8> {
    let bf = blowfish();
    let mut out = Vec::with_capacity(buf.len());
    for block in buf.chunks_exact(8) {
        let mut ga = GenericArray::clone_from_slice(block);
        bf.decrypt_block(&mut ga);
        out.extend_from_slice(&ga);
    }
    out
}

/// Builds a message body the way the exe's stream writer does.
#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    /// A string (or an int rendered as text): the bytes, then a `0x0A` terminator. Latin-1,
    /// which for ASCII tags and decimal numbers is just the bytes.
    fn field(&mut self, s: &str) -> &mut Self {
        self.0.extend(s.bytes());
        self.0.push(NL);
        self
    }
    fn int(&mut self, v: i64) -> &mut Self {
        self.field(&v.to_string())
    }
    /// Raw bytes with no terminator — for the Steam ticket blob.
    fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// Reads a decrypted message the way the exe's stream reader does. Every accessor is
/// bounds-checked and returns `None` past the end, so a truncated or crafted reply stops the
/// parse instead of panicking.
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, p: 0 }
    }
    fn done(&self) -> bool {
        self.p >= self.b.len()
    }
    /// A field up to the next `0x0A` (or a NUL, which the game also treats as a terminator).
    /// Consumes the terminator.
    fn field(&mut self) -> String {
        let mut out = Vec::new();
        while self.p < self.b.len() {
            let c = self.b[self.p];
            self.p += 1;
            if c == NL || c == 0 {
                break;
            }
            out.push(c);
        }
        out.iter().map(|&b| b as char).collect()
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.p)?;
        self.p += 1;
        Some(v)
    }
    fn i16_le(&mut self) -> Option<i16> {
        let s = self.b.get(self.p..self.p + 2)?;
        self.p += 2;
        Some(i16::from_le_bytes([s[0], s[1]]))
    }
    fn i32_le(&mut self) -> Option<i32> {
        let s = self.b.get(self.p..self.p + 4)?;
        self.p += 4;
        Some(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn raw(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.p..self.p + n)?;
        self.p += n;
        Some(s)
    }
}

/// Decode the exe's 19-byte packed address: `flag`, then the IP (network order), then the port
/// big-endian. `flag` 0 is IPv4 (IP at 1..5, port at 5..7), 1 is IPv6 (IP at 1..17, port
/// 17..19). Returns the `ip:port` string the game's connect flag takes.
fn decode_addr(b: &[u8]) -> Option<String> {
    match b.first()? {
        0 => {
            let ip = b.get(1..5)?;
            let port = u16::from_be_bytes([*b.get(5)?, *b.get(6)?]);
            Some(format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port))
        }
        1 => {
            let ip = b.get(1..17)?;
            let seg: Vec<String> = ip.chunks(2).map(|c| format!("{:x}", u16::from_be_bytes([c[0], c[1]]))).collect();
            let port = u16::from_be_bytes([*b.get(17)?, *b.get(18)?]);
            Some(format!("[{}]:{}", seg.join(":"), port))
        }
        _ => None,
    }
}

/// The command entry point: resolve the masters, ask each until one answers, return what the
/// tab shows. Runs the blocking socket work off the async runtime.
pub async fn list_servers(app: tauri::AppHandle) -> Result<Vec<WorldServer>, String> {
    let cfg = crate::config::load_or_detect(&app).unwrap_or_default();
    let masters = crate::config::master_servers(&cfg);
    // Any display name works for the browse path; the login path only needs one to fill the
    // field beside the Steam ticket, which is what's actually checked.
    let rider = if cfg.cp_rider_name.trim().is_empty() {
        "MXBapp".to_string()
    } else {
        cfg.cp_rider_name.trim().to_string()
    };
    tauri::async_runtime::spawn_blocking(move || fetch(&masters, &rider))
        .await
        .map_err(|e| format!("server-list task failed: {e}"))?
}

/// Try each master: browse (no auth) first, then a Steam-ticket `LOGIN` fallback.
fn fetch(masters: &[String], rider: &str) -> Result<Vec<WorldServer>, String> {
    if masters.is_empty() {
        return Err("No master server is configured.".into());
    }
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("couldn't open a socket: {e}"))?;
    sock.set_read_timeout(Some(Duration::from_secs(3))).ok();

    let mut last_err = String::new();
    for master in masters {
        let target = match resolve(master) {
            Ok(t) => t,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        // No-auth browse first — most servers list this way and it needs no Steam.
        match query(&sock, target, None) {
            Ok(list) if !list.is_empty() => return Ok(list),
            Ok(_) => {}
            Err(e) => last_err = e,
        }
        // Fall back to the authenticated path the game uses to join.
        match steam_ticket() {
            Ok(ticket) => match query(&sock, target, Some((rider, &ticket))) {
                Ok(list) if !list.is_empty() => return Ok(list),
                Ok(_) => last_err = "The master server accepted the login but sent no servers.".into(),
                Err(e) => last_err = e,
            },
            Err(e) => {
                if last_err.is_empty() {
                    last_err = e;
                }
            }
        }
    }
    Err(if last_err.is_empty() {
        "The master server didn't answer.".into()
    } else {
        last_err
    })
}

fn resolve(addr: &str) -> Result<SocketAddr, String> {
    addr.to_socket_addrs()
        .map_err(|e| format!("couldn't resolve {addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("{addr} resolved to no address"))
}

/// One master conversation: optional `LOGIN`, then `GETLIST`, draining every `LIST` datagram
/// that arrives before the socket times out. The master may split the list across packets, so
/// we accumulate rather than assume one reply.
fn query(sock: &UdpSocket, target: SocketAddr, auth: Option<(&str, &[u8])>) -> Result<Vec<WorldServer>, String> {
    if let Some((rider, ticket)) = auth {
        login(sock, target, rider, ticket)?;
    }

    // GETLIST carries the count-so-far + 1 (a 1-based "next index I want"); a fresh fetch is 1.
    let msg = {
        let mut w = Writer::default();
        w.field("GETLIST").int(1);
        w.finish()
    };
    sock.send_to(&encrypt(msg), target).map_err(|e| format!("send failed: {e}"))?;

    let debug = std::env::var("MXB_WORLDNET_DEBUG").is_ok();
    let mut servers = Vec::new();
    let mut buf = [0u8; 65535];
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        let (n, from) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(_) => break, // timeout: no more datagrams
        };
        if from != target || n == 0 || n > MAX_REPLY {
            continue;
        }
        let clear = decrypt(&buf[..n]);
        if debug {
            log::info!("[worldnet] {n}B reply from {from}: {}", hex(&clear));
        }
        let mut r = Reader::new(&clear);
        match r.field().as_str() {
            "LIST" | "LIST2" => parse_list(&mut r, &mut servers),
            other => {
                if debug {
                    log::info!("[worldnet] non-LIST reply tag {other:?}");
                }
            }
        }
    }
    Ok(servers)
}

/// Parse the records out of a `LIST` reply. Layout, per the exe's read sequence
/// (`0x1402a68d4`): an index field and a flag field, then repeating records terminated by an
/// empty name — `name`, the public address (19 B), a secondary/LAN address (19 B), three `u8`
/// counters, a `≤32`-byte string, an `i32`, and a `u16`-length blob.
fn parse_list(r: &mut Reader, out: &mut Vec<WorldServer>) {
    let _index = r.field();
    let _flag = r.field();
    loop {
        let name = r.field();
        if name.is_empty() {
            break; // empty name terminates the batch
        }
        let public = r.raw(19).and_then(decode_addr);
        let _secondary = r.raw(19); // LAN address, not shown
        let a = r.u8();
        let b = r.u8();
        let c = r.u8();
        let track = r.field();
        let _extra_id = r.i32_le();
        let blob_len = r.i16_le().unwrap_or(0).max(0) as usize;
        let _blob = r.raw(blob_len.min(300));

        let Some(address) = public else {
            // No usable address means nothing to show or join; skip but keep parsing.
            if a.is_none() {
                break;
            }
            continue;
        };
        out.push(WorldServer {
            name: name.trim().to_string(),
            address,
            // Byte meanings are inferred from the read order; confirmed via MXB_WORLDNET_DEBUG.
            players: a.unwrap_or(0) as u32,
            max_players: b.unwrap_or(0) as u32,
            passworded: c.unwrap_or(0) != 0,
            ping_ms: None,
            track: track.trim().to_string(),
            region: String::new(),
        });
    }
}

/// The authenticated login the game uses before joining: `LOGIN`, a sequence number, the
/// identity fields, mode 1 (Steam), the rider name, an empty password, the Steam id string,
/// then the ticket length and the ticket bytes. Succeeds when the master replies `AUTH … OK`.
fn login(sock: &UdpSocket, target: SocketAddr, rider: &str, ticket: &[u8]) -> Result<(), String> {
    let mut w = Writer::default();
    w.field("LOGIN")
        .int(0) // sequence
        .field(rider) // identity string
        .int(0) // identity number
        .int(1) // mode 1 = Steam
        .field(rider) // name
        .field("") // password (none)
        .field(rider) // steam id string (best-effort; the ticket is what's checked)
        .int(ticket.len() as i64)
        .raw(ticket);
    sock.send_to(&encrypt(w.finish()), target).map_err(|e| format!("login send failed: {e}"))?;

    let mut buf = [0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf).map_err(|_| "the master didn't answer the login".to_string())?;
    let clear = decrypt(&buf[..n.min(MAX_REPLY)]);
    let mut r = Reader::new(&clear);
    if r.field() == "AUTH" && r.field().eq_ignore_ascii_case("OK") {
        Ok(())
    } else {
        Err("The master server rejected the login.".into())
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("")
}

// --- Steam auth ticket ------------------------------------------------------------------
//
// A genuine `GetAuthSessionTicket` blob, obtained by loading Steam's flat C API from
// `steam_api64.dll` at runtime — no build-time linkage, matching how `steamid.rs` reads Steam
// without the SDK. Windows only; unverifiable off Windows, so elsewhere it's a clear error and
// the browse path is what runs.
#[cfg(windows)]
fn steam_ticket() -> Result<Vec<u8>, String> {
    steam_win::auth_ticket()
}

#[cfg(not(windows))]
fn steam_ticket() -> Result<Vec<u8>, String> {
    Err("A Steam login is only available on Windows; the browser used the public list instead.".into())
}

#[cfg(windows)]
mod steam_win {
    use libloading::{Library, Symbol};
    use std::ffi::c_void;

    /// MX Bikes' Steam AppID, so the ticket authenticates as this game.
    const APPID: &str = "655500";

    type InitFn = unsafe extern "C" fn() -> bool;
    type ShutdownFn = unsafe extern "C" fn();
    type RunCallbacksFn = unsafe extern "C" fn();
    type SteamUserFn = unsafe extern "C" fn() -> *mut c_void;
    // GetAuthSessionTicket(self, buf, cbMax, *pcbTicket, *pSteamNetworkingIdentity) -> handle
    type GetTicketFn = unsafe extern "C" fn(*mut c_void, *mut u8, i32, *mut u32, *const c_void) -> u32;

    /// Find `steam_api64.dll`: beside the game if we can, else let the loader search the
    /// process's DLL path (Steam puts it there for a running client).
    fn load() -> Result<Library, String> {
        unsafe { Library::new("steam_api64.dll") }.map_err(|e| format!("steam_api64.dll not available: {e}"))
    }

    pub fn auth_ticket() -> Result<Vec<u8>, String> {
        // Init as the game's AppID. The env vars are how the flat API learns which app it is
        // when the process wasn't launched by Steam.
        std::env::set_var("SteamAppId", APPID);
        std::env::set_var("SteamGameId", APPID);

        let lib = load()?;
        unsafe {
            let init: Symbol<InitFn> = lib.get(b"SteamAPI_Init\0").map_err(|e| e.to_string())?;
            if !init() {
                return Err("Steam isn't running, or this account doesn't own MX Bikes.".into());
            }
            let shutdown: Symbol<ShutdownFn> = lib.get(b"SteamAPI_Shutdown\0").map_err(|e| e.to_string())?;
            let run: Symbol<RunCallbacksFn> = lib.get(b"SteamAPI_RunCallbacks\0").map_err(|e| e.to_string())?;
            // Accessor version is SDK-pinned; verify against the shipped steam_api64.dll on a
            // Windows tester if a future SDK renames it.
            let user_fn: Symbol<SteamUserFn> =
                lib.get(b"SteamAPI_SteamUser_v023\0").map_err(|_| "Steam user interface unavailable".to_string())?;
            let get_ticket: Symbol<GetTicketFn> = lib
                .get(b"SteamAPI_ISteamUser_GetAuthSessionTicket\0")
                .map_err(|e| e.to_string())?;

            let user = user_fn();
            if user.is_null() {
                shutdown();
                return Err("Steam user interface unavailable.".into());
            }
            let mut ticket = vec![0u8; 1024];
            let mut written: u32 = 0;
            let _handle = get_ticket(user, ticket.as_mut_ptr(), ticket.len() as i32, &mut written, std::ptr::null());
            // The ticket only validates after Steam's backend acknowledges it; pump callbacks
            // briefly so it's usable by the time we send it.
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
            while std::time::Instant::now() < deadline {
                run();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            shutdown();
            if written == 0 {
                return Err("Steam returned an empty auth ticket.".into());
            }
            ticket.truncate(written as usize);
            Ok(ticket)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blowfish, over the raw bytes, must match the published test vectors — that's what pins
    /// our reading of the exe's byte-swapping to the standard cipher.
    #[test]
    fn blowfish_matches_the_standard_vectors() {
        fn enc(key: &[u8], pt: [u8; 8]) -> [u8; 8] {
            let bf = Blowfish::new_from_slice(key).unwrap();
            let mut ga = GenericArray::clone_from_slice(&pt);
            bf.encrypt_block(&mut ga);
            ga.into()
        }
        assert_eq!(
            enc(&[0u8; 8], [0u8; 8]),
            [0x4E, 0xF9, 0x97, 0x45, 0x61, 0x98, 0xDD, 0x78]
        );
        assert_eq!(
            enc(&[0xFF; 8], [0xFF; 8]),
            [0x51, 0x86, 0x6F, 0xD5, 0xB8, 0x5E, 0xCB, 0x8A]
        );
    }

    #[test]
    fn encrypt_then_decrypt_round_trips_with_the_game_key() {
        let body = Writer::default().field("GETLIST").int(1).finish();
        let round = decrypt(&encrypt(body.clone()));
        // Decryption yields the padded body; the meaningful prefix is intact.
        assert!(round.starts_with(&body));
    }

    #[test]
    fn addresses_decode_to_ip_port() {
        // IPv4 203.0.113.10:54210, port big-endian (0xD3C2).
        let mut v = vec![0u8; 19];
        v[1..5].copy_from_slice(&[203, 0, 113, 10]);
        v[5] = 0xD3;
        v[6] = 0xC2;
        assert_eq!(decode_addr(&v).as_deref(), Some("203.0.113.10:54210"));
    }

    #[test]
    fn parses_a_synthetic_list_reply() {
        // Build one the way the master does: index, flag, one record, empty-name terminator.
        let mut addr = vec![0u8; 19];
        addr[1..5].copy_from_slice(&[10, 0, 0, 5]);
        addr[5] = 0xD3;
        addr[6] = 0xC2; // 54210

        let mut w = Writer::default();
        w.field("LIST").int(1).int(0); // tag, index, flag
        w.field("Frost's Server"); // name
        w.raw(&addr); // public
        w.raw(&vec![0u8; 19]); // secondary
        w.raw(&[7, 20, 1]); // players=7, max=20, passworded=1
        w.field("mmx_supercross"); // track
        w.raw(&0i32.to_le_bytes()); // extra id
        w.raw(&0i16.to_le_bytes()); // blob length 0
        w.field(""); // terminating empty name
        let body = w.finish();

        let clear = decrypt(&encrypt(body));
        let mut r = Reader::new(&clear);
        assert_eq!(r.field(), "LIST");
        let mut out = Vec::new();
        parse_list(&mut r, &mut out);
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.name, "Frost's Server");
        assert_eq!(s.address, "10.0.0.5:54210");
        assert_eq!(s.players, 7);
        assert_eq!(s.max_players, 20);
        assert!(s.passworded);
        assert_eq!(s.track, "mmx_supercross");
    }
}
