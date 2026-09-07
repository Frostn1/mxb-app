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
//!   auth ticket first. The public master answers nothing to a bare browse, so the ticket
//!   `LOGIN` is what returns a list; browse runs after it, for a private master and for the
//!   platforms where there is no Steam to ask.
//! - **Signing in**: `LOGIN` names the game (`mxbikes`/`0x1502`), the account (SteamID64, in
//!   decimal) and the ticket, whose length goes on the wire as a binary little-endian `i32`.
//!   Every one of those is load-bearing, and the master says nothing at all when one is
//!   wrong — see [`login_message`].
//!
//! Set `MXB_WORLDNET_DEBUG=1` to log each decrypted reply as hex; the meaning of a couple of
//! the per-record bytes is inferred from the read sequence and confirmed against a live
//! capture, not from any spec.

use crate::WorldServer;
use blowfish::Blowfish;
use cipher::generic_array::GenericArray;
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};

/// The 32-byte ASCII key the exe builds on the stack and hands to the Blowfish schedule.
const KEY: &[u8; 32] = b"stIA1OatIev9evlABlaTroaSpletoAtr";
/// Field terminator and block pad, both `0x0A`.
const NL: u8 = 0x0A;
/// The exe's per-record struct is 0x1d8 bytes; the readable payload never approaches that, but
/// the guard keeps a hostile reply from steering the parser.
const MAX_REPLY: usize = 64 * 1024;
/// The client version the master gates on, written as `LOGIN`'s second field (`0x28` at
/// `0x1402a72e0`). Verified live: 39 is answered `Auth NO Old Version: please update`, 40 gets
/// past the gate. It tracks the game, so a build that bumps it breaks the list until we follow
/// — `MXB_WORLDNET_VERSION` is the stopgap.
const CLIENT_VERSION: i64 = 40;

/// The game's own identity, sent as `LOGIN`'s third and fourth fields. The exe hands
/// `("mxbikes", 0x1502)` to the setter at `0x1402a9c70` (from `0x140134078`) and the login
/// writes them back out verbatim — so a master that gates on which game is asking sees the
/// game, not us. GP Bikes will have its own pair.
const GAME_ID: &str = "mxbikes";
const GAME_NUMBER: i64 = 0x1502;

fn client_version() -> i64 {
    std::env::var("MXB_WORLDNET_VERSION")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(CLIENT_VERSION)
}

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
    /// A 4-byte little-endian int, no terminator. The exe writes the ticket length this way
    /// (`0x140283690`), not as text — get it wrong and everything after it is garbage.
    fn i32le(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
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
    let install = std::path::PathBuf::from(cfg.install_dir());
    tauri::async_runtime::spawn_blocking(move || fetch(&masters, &rider, &install))
        .await
        .map_err(|e| format!("server-list task failed: {e}"))?
}

/// Try each master: the Steam-ticket `LOGIN` the game uses, then a no-auth browse.
///
/// The public master ignores a `GETLIST` that never logged in — verified live against
/// `master.mx-bikes.com`, which answers nothing at all to a bare browse — so the ticket is what
/// actually returns a list. Browse still runs after it: it costs one datagram, it's the only
/// path off Windows, and a private master may well answer it.
fn fetch(masters: &[String], rider: &str, install: &Path) -> Result<Vec<WorldServer>, String> {
    if masters.is_empty() {
        return Err("No master server is configured.".into());
    }
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("couldn't open a socket: {e}"))?;
    sock.set_read_timeout(Some(Duration::from_secs(3))).ok();

    // One ticket for the whole sweep — each one costs a Steam init and a callback pump.
    let ticket = steam_ticket(install);
    match &ticket {
        Ok(t) => log::info!("[worldnet] Steam ticket: {} bytes for {}", t.ticket.len(), t.steam_id),
        Err(e) => log::info!("[worldnet] no Steam ticket, browsing instead: {e}"),
    }
    let mut last_err = String::new();
    for master in masters {
        let target = match resolve(master) {
            Ok(t) => t,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        match &ticket {
            Ok(steam) => match query(&sock, target, Some((rider, steam))) {
                Ok(list) if !list.is_empty() => {
                    log::info!("[worldnet] {} server(s) from {master}", list.len());
                    return Ok(list);
                }
                Ok(_) => last_err = "The master server accepted the login but sent no servers.".into(),
                Err(e) => last_err = e,
            },
            Err(e) => {
                if last_err.is_empty() {
                    last_err = e.clone();
                }
            }
        }
        match query(&sock, target, None) {
            Ok(list) if !list.is_empty() => return Ok(list),
            Ok(_) => {}
            Err(e) => last_err = e,
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
fn query(sock: &UdpSocket, target: SocketAddr, auth: Option<(&str, &SteamAuth)>) -> Result<Vec<WorldServer>, String> {
    if let Some((rider, steam)) = auth {
        login(sock, target, rider, steam)?;
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

/// The authenticated login the game uses before joining. Succeeds when the master replies
/// `AUTH … OK`; the body it sends is [`login_message`].
fn login(sock: &UdpSocket, target: SocketAddr, rider: &str, auth: &SteamAuth) -> Result<(), String> {
    let body = login_message(rider, &auth.steam_id.to_string(), &auth.ticket);
    sock.send_to(&encrypt(body), target)
        .map_err(|e| format!("login send failed: {e}"))?;

    let mut buf = [0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf).map_err(|_| "the master didn't answer the login".to_string())?;
    auth_verdict(&decrypt(&buf[..n.min(MAX_REPLY)]))
}

/// The `LOGIN` body, in the exe's field order (built at `0x1402a72a8`).
///
/// The last three fields are the ones the master actually gates a Steam sign-in on: who is
/// asking ([`GAME_ID`]/[`GAME_NUMBER`]), which account (`steam_id`, decimal — the master
/// `sscanf`s it as `%llud` at `0x140023250`), and the ticket. The length before the ticket is
/// a **binary** little-endian `i32`, not text: written as text it reads as a length in the
/// hundreds of millions, the master's read of the ticket runs off the end of the datagram, and
/// it drops the packet without a word — which is exactly what "didn't answer the login" was.
fn login_message(rider: &str, steam_id: &str, ticket: &[u8]) -> Vec<u8> {
    let mut w = Writer::default();
    w.field("LOGIN")
        .int(client_version()) // client version — the master refuses anything older
        .field(GAME_ID) // which game is asking
        .int(GAME_NUMBER) // and its number, beside it
        .int(1) // mode 1 = Steam
        .field(rider) // name
        .field("") // password (none)
        .field(steam_id) // the account the ticket belongs to
        .i32le(ticket.len() as i32)
        .raw(ticket);
    w.finish()
}

/// Read the master's answer to a `LOGIN`: `Auth` — mixed case on the wire, so compare loosely —
/// then OK/NO, then a human reason when it refuses ("Old Version: please update", "Invalid
/// Key"). Passing that reason through is the difference between a fixable report and a shrug.
fn auth_verdict(clear: &[u8]) -> Result<(), String> {
    let mut r = Reader::new(clear);
    let (tag, verdict, reason) = (r.field(), r.field(), r.field());
    if tag.eq_ignore_ascii_case("AUTH") && verdict.eq_ignore_ascii_case("OK") {
        Ok(())
    } else if !reason.trim().is_empty() {
        Err(format!("The master server refused the login: {}.", reason.trim()))
    } else {
        Err("The master server rejected the login.".into())
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("")
}

// --- Steam auth ticket ------------------------------------------------------------------
//
// A genuine `GetAuthSessionTicket` blob and the SteamID64 it was issued to, obtained by
// loading Steam's flat C API from `steam_api64.dll` at runtime — no build-time linkage,
// matching how `steamid.rs` reads Steam without the SDK. Windows only; unverifiable off
// Windows, so elsewhere it's a clear error and the browse path is what runs.

/// What a Steam sign-in needs: the account, and the ticket that proves it.
struct SteamAuth {
    steam_id: u64,
    ticket: Vec<u8>,
}

#[cfg(windows)]
fn steam_ticket(install: &Path) -> Result<SteamAuth, String> {
    steam_win::auth_ticket(install)
}

#[cfg(not(windows))]
fn steam_ticket(_install: &Path) -> Result<SteamAuth, String> {
    Err("A Steam login is only available on Windows; the browser used the public list instead.".into())
}

#[cfg(windows)]
mod steam_win {
    use libloading::os::windows::{Library as WinLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
    use libloading::{Library, Symbol};
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};

    /// MX Bikes' Steam AppID, so the ticket authenticates as this game.
    const APPID: &str = "655500";
    const DLL: &str = "steam_api64.dll";

    type InitFn = unsafe extern "C" fn() -> bool;
    type ShutdownFn = unsafe extern "C" fn();
    type RunCallbacksFn = unsafe extern "C" fn();
    type SteamUserFn = unsafe extern "C" fn() -> *mut c_void;
    type CreateInterfaceFn = unsafe extern "C" fn(*const u8) -> *mut c_void;
    type HandleFn = unsafe extern "C" fn() -> i32;
    // GetISteamUser(client, hUser, hPipe, version) -> ISteamUser*
    type GetIUserFn = unsafe extern "C" fn(*mut c_void, i32, i32, *const u8) -> *mut c_void;
    // GetAuthSessionTicket(self, buf, cbMax, *pcbTicket, *pSteamNetworkingIdentity) -> handle.
    // The identity argument arrived in a later SDK; on the Microsoft x64 ABI an older DLL just
    // ignores the extra register, so one signature covers both.
    type GetTicketFn = unsafe extern "C" fn(*mut c_void, *mut u8, i32, *mut u32, *const c_void) -> u32;
    // GetSteamID(self) -> CSteamID. A `CSteamID` is one 8-byte value and comes back in RAX
    // either way, so the same signature reads an old DLL and a new one.
    type GetSteamIdFn = unsafe extern "C" fn(*mut c_void) -> u64;

    /// Every `ISteamUser` accessor Valve has shipped, newest first. A DLL from before they
    /// existed is handled by [`user_via_client`].
    const ACCESSORS: [&[u8]; 5] = [
        b"SteamAPI_SteamUser_v023\0",
        b"SteamAPI_SteamUser_v022\0",
        b"SteamAPI_SteamUser_v021\0",
        b"SteamAPI_SteamUser_v020\0",
        b"SteamAPI_SteamUser\0",
    ];
    /// Interface versions for the old route, the game's own first (`SteamClient017` /
    /// `SteamUser019` at `0x140130288`). Steam still serves an old version to a new DLL.
    const CLIENTS: [&[u8]; 4] = [b"SteamClient017\0", b"SteamClient020\0", b"SteamClient021\0", b"SteamClient022\0"];
    const USERS: [&[u8]; 5] = [b"SteamUser019\0", b"SteamUser020\0", b"SteamUser021\0", b"SteamUser022\0", b"SteamUser023\0"];

    /// libloading's `Display` is just "LoadLibraryExW failed"; the OS reason — module not found
    /// vs. a bad image — is in the source, and that reason is the whole diagnosis.
    fn why(e: &libloading::Error) -> String {
        match std::error::Error::source(e) {
            Some(src) => format!("{e}: {src}"),
            None => e.to_string(),
        }
    }

    /// The dirs that can hold the DLL: the game's install first, then ours.
    ///
    /// Nothing puts `steam_api64.dll` on the app's search path — we don't ship it and Steam
    /// didn't launch us — so a bare `LoadLibrary` fails with "module not found" on every
    /// machine. The copy that exists is the one beside `mxbikes.exe`, and
    /// `LOAD_WITH_ALTERED_SEARCH_PATH` lets it resolve its own dependencies from there.
    fn candidates(install: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if !install.as_os_str().is_empty() {
            dirs.push(install.to_path_buf());
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                dirs.push(dir.to_path_buf());
            }
        }
        dirs
    }

    fn load(install: &Path) -> Result<Library, String> {
        let mut tried: Vec<String> = Vec::new();
        for dir in candidates(install) {
            let path = dir.join(DLL);
            if !path.is_file() {
                tried.push(format!("{} (not there)", path.display()));
                continue;
            }
            match unsafe { WinLibrary::load_with_flags(&path, LOAD_WITH_ALTERED_SEARCH_PATH) } {
                Ok(lib) => return Ok(Library::from(lib)),
                Err(e) => tried.push(format!("{}: {}", path.display(), why(&e))),
            }
        }
        match unsafe { Library::new(DLL) } {
            Ok(lib) => Ok(lib),
            Err(e) => {
                tried.push(format!("the loader's search path: {}", why(&e)));
                Err(format!("{DLL} not available — tried {}", tried.join("; ")))
            }
        }
    }

    /// The old route to `ISteamUser`, for a DLL that predates the accessors: create the client
    /// interface and ask it, exactly as the game does — all exported functions, no vtables.
    unsafe fn user_via_client(lib: &Library) -> Option<*mut c_void> {
        let create: Symbol<CreateInterfaceFn> = lib.get(b"SteamInternal_CreateInterface\0").ok()?;
        let get_user: Symbol<GetIUserFn> = lib.get(b"SteamAPI_ISteamClient_GetISteamUser\0").ok()?;
        let h_user: Symbol<HandleFn> = lib.get(b"SteamAPI_GetHSteamUser\0").ok()?;
        let h_pipe: Symbol<HandleFn> = lib.get(b"SteamAPI_GetHSteamPipe\0").ok()?;
        let (user, pipe) = (h_user(), h_pipe());
        for client_ver in CLIENTS {
            let client = create(client_ver.as_ptr());
            if client.is_null() {
                continue;
            }
            for user_ver in USERS {
                let iface = get_user(client, user, pipe, user_ver.as_ptr());
                if !iface.is_null() {
                    return Some(iface);
                }
            }
        }
        None
    }

    /// Sets `SteamAppId`/`SteamGameId` for as long as it lives, then puts them back.
    ///
    /// They're how the flat API learns which app it is when Steam didn't launch us, so they
    /// have to be set across `SteamAPI_Init`. Leaving them set afterwards would say the app
    /// *is* MX Bikes to everything it goes on to touch — including a `steam://rungameid`
    /// launch — so the claim ends with the call that needed it.
    struct AppIdVars {
        prior: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl AppIdVars {
        fn claim() -> Self {
            let prior = ["SteamAppId", "SteamGameId"]
                .into_iter()
                .map(|k| {
                    let was = std::env::var_os(k);
                    std::env::set_var(k, APPID);
                    (k, was)
                })
                .collect();
            AppIdVars { prior }
        }
    }

    impl Drop for AppIdVars {
        fn drop(&mut self) {
            for (key, was) in &self.prior {
                match was {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    pub fn auth_ticket(install: &Path) -> Result<super::SteamAuth, String> {
        // Held for the whole call: every early return below puts the environment back.
        let _appid = AppIdVars::claim();

        let lib = load(install)?;
        unsafe {
            let init: Symbol<InitFn> = lib.get(b"SteamAPI_Init\0").map_err(|e| why(&e))?;
            if !init() {
                return Err("Steam isn't running, or this account doesn't own MX Bikes.".into());
            }
            let shutdown: Symbol<ShutdownFn> = lib.get(b"SteamAPI_Shutdown\0").map_err(|e| why(&e))?;
            let run: Symbol<RunCallbacksFn> = lib.get(b"SteamAPI_RunCallbacks\0").map_err(|e| why(&e))?;
            // Newer DLLs hand the interface over directly; the game ships one old enough to
            // predate that (it asks Steam for `SteamUser019`), so fall back to the client.
            let user = ACCESSORS
                .iter()
                .find_map(|name| lib.get::<SteamUserFn>(name).ok().map(|f| f()))
                .filter(|u| !u.is_null())
                .or_else(|| user_via_client(&lib));
            let Some(user) = user else {
                shutdown();
                return Err("Steam user interface unavailable.".into());
            };
            let get_ticket: Symbol<GetTicketFn> = lib
                .get(b"SteamAPI_ISteamUser_GetAuthSessionTicket\0")
                .map_err(|e| {
                    shutdown();
                    format!("this {DLL} has no auth-ticket call: {}", why(&e))
                })?;

            // The master is told which account the ticket belongs to, so ask the interface
            // that issued it rather than guessing from a name.
            let steam_id = lib
                .get::<GetSteamIdFn>(b"SteamAPI_ISteamUser_GetSteamID\0")
                .ok()
                .map(|f| f(user))
                .unwrap_or(0);

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
            if steam_id == 0 {
                return Err("Steam didn't say which account is signed in.".into());
            }
            ticket.truncate(written as usize);
            Ok(super::SteamAuth { steam_id, ticket })
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
            // Annotated: with the block handed straight to `encrypt_block`, nothing else
            // pins the cipher's byte order, and `Blowfish` is generic over it.
            let bf: Blowfish = Blowfish::new_from_slice(key).unwrap();
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
        // `field` and `int` hand back a borrow, so the writer has to outlive the chain
        // before `finish` can take it by value.
        let mut w = Writer::default();
        w.field("GETLIST").int(1);
        let body = w.finish();
        let round = decrypt(&encrypt(body.clone()));
        // Decryption yields the padded body; the meaningful prefix is intact.
        assert!(round.starts_with(&body));
    }

    /// The master gates on the version field: 39 is answered "Old Version: please update",
    /// 40 gets past it (probed live). Losing this again costs the whole server list.
    #[test]
    fn login_carries_the_client_version() {
        let body = login_message("Frost", "76561199164505734", b"ticket");
        let mut r = Reader::new(&body);
        assert_eq!(r.field(), "LOGIN");
        assert_eq!(r.field(), CLIENT_VERSION.to_string());
    }

    /// Every field of `LOGIN`, in the order the exe writes them (`0x1402a72a8`). The master
    /// answers nothing at all when this is wrong, so the whole thing is pinned, not just the
    /// parts we happened to get right.
    #[test]
    fn login_matches_the_exes_field_order() {
        let ticket: Vec<u8> = (0u8..=255).collect(); // 256 bytes, like a real one
        let body = login_message("Frost", "76561199164505734", &ticket);

        let mut r = Reader::new(&body);
        assert_eq!(r.field(), "LOGIN");
        assert_eq!(r.field(), CLIENT_VERSION.to_string());
        assert_eq!(r.field(), "mxbikes"); // the game, not the rider
        assert_eq!(r.field(), "5378"); // 0x1502, beside it
        assert_eq!(r.field(), "1"); // mode 1 = Steam
        assert_eq!(r.field(), "Frost"); // name
        assert_eq!(r.field(), ""); // password
        assert_eq!(r.field(), "76561199164505734"); // the account, decimal

        // The length is four raw little-endian bytes — not "256\n", which is what silently
        // cost us every login.
        assert_eq!(r.raw(4), Some(&256i32.to_le_bytes()[..]));
        assert_eq!(r.raw(ticket.len()), Some(&ticket[..]));
        assert!(r.done(), "nothing follows the ticket");
    }

    /// A ticket length written as text lands inside the ASCII range, so a reader that expects
    /// four binary bytes gets a plausible-looking number in the hundreds of millions — no
    /// error, no reply, just a master reading a ticket that isn't there.
    #[test]
    fn a_text_length_would_read_as_a_wild_one() {
        let text = b"234\n"; // what `.int(234)` used to emit, exactly four bytes
        assert_eq!(i32::from_le_bytes(*text), 171_193_138);
    }

    #[test]
    fn a_refusal_carries_the_masters_own_reason() {
        // Exactly what master.mx-bikes.com sends an out-of-date client, casing included.
        let refused = b"Auth\nNO\nOld Version: please update\n";
        let err = auth_verdict(refused).unwrap_err();
        assert!(err.contains("Old Version: please update"), "{err}");
        assert!(auth_verdict(b"Auth\nOK\n").is_ok());
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
