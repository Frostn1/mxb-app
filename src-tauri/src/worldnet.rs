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
//!   ticket first. The public master answers nothing to a bare browse, so the ticket `LOGIN`
//!   is what returns a list; browse runs after it, for a private master and for the platforms
//!   where there is no Steam to ask.
//! - **Signing in**: `LOGIN` names the game (`mxbikes`/`0x1502`), the account (SteamID64, in
//!   decimal) and the ticket, whose length goes on the wire as a binary little-endian `i32`.
//!   The ticket is Steam's **encrypted app ticket** — `RequestEncryptedAppTicket` sealed with
//!   the app's key, which only PiBoSo can open — not a `GetAuthSessionTicket` blob; see
//!   [`steam_win::fetch_ticket`]. Every one of those is load-bearing, and the master answers a
//!   wrong one with silence or `Invalid Account` — see [`login_message`].
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
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The 32-byte ASCII key the exe builds on the stack and hands to the Blowfish schedule.
const KEY: &[u8; 32] = b"stIA1OatIev9evlABlaTroaSpletoAtr";
/// Field terminator and block pad, both `0x0A`.
const NL: u8 = 0x0A;
/// The exe's per-record struct is 0x1d8 bytes; the readable payload never approaches that, but
/// the guard keeps a hostile reply from steering the parser.
const MAX_REPLY: usize = 64 * 1024;
/// The exe's buffer for a record's event blob (`0x1400abdac`), and the cap on what we read.
const MAX_BLOB: usize = 300;
/// How long the whole paged sweep gets. The exe gives its own browse 30 s (`0x1402a7538`);
/// this is a tab the player is watching, so it ends sooner and shows what arrived.
const BROWSE_BUDGET: Duration = Duration::from_secs(12);
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

/// One decoded address, and whether we can hand it to the game's connect flag.
///
/// The game itself is dual-stack — every socket that matters is `AF_INET6` with `IPV6_V6ONLY`
/// cleared (`0x14028502e`) — so it does try IPv6. We can't: the game takes one address on its
/// command line and `gameproc::parse_server_address` refuses a bracketed literal. So an IPv6
/// address is shown but not offered, and [`best_address`] looks for an IPv4 route first.
struct Addr {
    text: String,
    joinable: bool,
}

impl Addr {
    fn v4(ip: &[u8], port: u16) -> Self {
        Addr {
            text: format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port),
            joinable: !is_private(ip) && ip[0] != 0,
        }
    }
}

/// Addresses nobody outside the server's own network can reach. The secondary address is
/// whatever `getaddrinfo` gave the server for its own hostname (`0x140284cc0`), so on a
/// home-hosted or containerised box it is an RFC1918 address.
fn is_private(ip: &[u8]) -> bool {
    matches!(ip, [10, ..] | [127, ..] | [169, 254, ..] | [192, 168, ..])
        || matches!(ip, [172, b, ..] if (16..32).contains(b))
}

/// Which address to offer for Join.
///
/// The game sends its connection request to *both* the public and the server-reported address
/// in the same pass and lets whichever answers win (`0x1402a342e`) — it never chooses. We only
/// get one argument, so we choose: the public address when the game's connect flag can take it,
/// otherwise a routable secondary. That is what makes an IPv6-registered server with an IPv4
/// interface joinable rather than a dead row.
fn best_address(public: Addr, secondary: Option<Addr>) -> (String, bool, String) {
    let lan = secondary.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    // Don't repeat the public address back as though it were extra detail.
    let lan = if lan == public.text { String::new() } else { lan };
    match secondary {
        Some(s) if !public.joinable && s.joinable => (s.text, true, String::new()),
        _ => (public.text, public.joinable, lan),
    }
}

/// Decode the exe's 19-byte packed address: `flag`, then the IP (network order), then the port
/// big-endian. `flag` 0 is IPv4 (IP at 1..5, port at 5..7), 1 is IPv6 (IP at 1..17, port
/// 17..19). Returns the `ip:port` string the game's connect flag takes.
fn decode_addr(b: &[u8]) -> Option<Addr> {
    match b.first()? {
        0 => {
            let ip = b.get(1..5)?;
            let port = u16::from_be_bytes([*b.get(5)?, *b.get(6)?]);
            Some(Addr::v4(ip, port))
        }
        1 => {
            let ip = b.get(1..17)?;
            let port = u16::from_be_bytes([*b.get(17)?, *b.get(18)?]);
            // `::ffff:a.b.c.d` is an IPv4 server the master happened to see through a
            // dual-stack socket. The address the game wants is the last four bytes.
            if ip[..10].iter().all(|&c| c == 0) && ip[10] == 0xFF && ip[11] == 0xFF {
                return Some(Addr::v4(&ip[12..16], port));
            }
            let seg: Vec<String> = ip.chunks(2).map(|c| format!("{:x}", u16::from_be_bytes([c[0], c[1]]))).collect();
            Some(Addr { text: format!("[{}]:{}", seg.join(":"), port), joinable: false })
        }
        _ => None,
    }
}

/// The command entry point: resolve the masters, ask each until one answers, return what the
/// tab shows. Runs the blocking socket work off the async runtime.
///
/// **The master is not always ours to ask.** Signing in to it spends the player's Steam
/// account, and MX Bikes spends the same account the moment it is running — so while the game
/// is up the app stays off the master entirely and rebuilds the list by asking the servers
/// themselves. `GETINFO` needs no account, no ticket and no challenge, so a refresh mid-session
/// costs nothing that the game is using. See [`crate::serverbook`] for where the addresses to
/// ask come from.
///
/// The same path is the fallback whenever the master fails for any other reason: a remembered
/// list, refreshed live from each server, beats an error message.
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
    let remembered = crate::serverbook::rows(&crate::serverbook::load(&app));
    let playing = crate::gameproc::is_game_running();

    let (mut list, from_master) = tauri::async_runtime::spawn_blocking(move || {
        if playing {
            log::info!(
                "[worldnet] MX Bikes is running — asking {} remembered server(s) directly and \
                 leaving the master login to the game",
                remembered.len()
            );
            return from_book(remembered).map(|l| (l, false));
        }
        match fetch(&masters, &rider, &install) {
            Ok(list) => Ok((list, true)),
            // A master that won't answer is exactly when a book of addresses earns its keep.
            Err(e) => match from_book(remembered) {
                Ok(list) => {
                    log::warn!("[worldnet] the master didn't answer ({e}); used the remembered list");
                    Ok((list, false))
                }
                Err(_) => Err(e),
            },
        }
    })
    .await
    .map_err(|e| format!("server-list task failed: {e}"))??;

    // Only a real sweep can teach the book an address it doesn't have; a probe of the book can
    // only ever confirm what taught it.
    if from_master {
        crate::serverbook::remember(&app, &list, crate::serverbook::now_millis());
    }

    let rules = crate::serverfilter::Rules::load(&crate::frostmod_manage::frostmod_dir(&app));
    let hidden = crate::serverfilter::mark(&rules, &mut list);
    log::info!("[worldnet] {} server(s), {hidden} hidden by the filter", list.len());
    Ok(list)
}

/// Rebuild the list from remembered addresses alone, by asking each server about itself.
///
/// A row that doesn't answer is dropped rather than shown from memory: a remembered name beside
/// a rider count from last week is worse than an absent row, because it looks current. Rows the
/// game could never reach are dropped for the same reason — they can't be probed, so there is
/// nothing to say about them that is true right now.
fn from_book(mut rows: Vec<WorldServer>) -> Result<Vec<WorldServer>, String> {
    if rows.is_empty() {
        return Err("There are no remembered servers yet — open the browser once with MX Bikes \
                    closed and the list will keep working from then on."
            .into());
    }
    let budget = book_budget(rows.len());
    probe_within(&mut rows, budget);
    rows.retain(|s| s.ping_ms.is_some());
    if rows.is_empty() {
        return Err("None of the remembered servers answered.".into());
    }
    Ok(rows)
}

/// How long to wait on a whole book. The probe is one datagram out per server and the replies
/// come back in parallel, so this is about the slowest useful round trip plus room for a large
/// book's send loop — not about the number of servers.
fn book_budget(n: usize) -> Duration {
    (PROBE_BUDGET + Duration::from_millis(n as u64)).min(Duration::from_secs(10))
}

/// Ask one server about itself. What the detail panel opens with, so a row the list built
/// minutes ago isn't what a player reads before deciding to join.
pub async fn probe_server(address: String) -> Result<WorldServer, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut one = vec![WorldServer { address, joinable: true, ..Default::default() }];
        probe_within(&mut one, PROBE_BUDGET);
        match one.pop() {
            Some(s) if s.ping_ms.is_some() => Ok(s),
            _ => Err("That server didn't answer.".into()),
        }
    })
    .await
    .map_err(|e| format!("probe task failed: {e}"))?
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
    let ticket = signed_in(install);
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
                Ok(mut list) if !list.is_empty() => {
                    log::info!("[worldnet] {} server(s) from {master}", list.len());
                    probe(&mut list);
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
            Ok(mut list) if !list.is_empty() => {
                probe(&mut list);
                return Ok(list);
            }
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

    let debug = std::env::var("MXB_WORLDNET_DEBUG").is_ok();
    let mut servers = Vec::new();
    let mut buf = [0u8; 65535];
    let deadline = Instant::now() + BROWSE_BUDGET;

    // The master answers a page at a time — its send buffer is 1400 bytes, so a busy list
    // arrives in batches of roughly nine. Each `GETLIST` asks for the record after the last
    // one we hold, and the reply's second field is `"0"` while more remain. Asking once and
    // reading whatever turns up in the next few seconds is what capped the tab at one page.
    'pages: while Instant::now() < deadline {
        let want = servers.len() + 1;
        let msg = {
            let mut w = Writer::default();
            w.field("GETLIST").int(want as i64);
            w.finish()
        };
        sock.send_to(&encrypt(msg), target).map_err(|e| format!("send failed: {e}"))?;

        // One page per request: read until this batch lands, or the socket goes quiet.
        loop {
            if Instant::now() >= deadline {
                break 'pages;
            }
            let (n, from) = match sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(_) => break 'pages, // timeout: the master has stopped talking
            };
            if from != target || n == 0 || n > MAX_REPLY {
                continue;
            }
            let clear = decrypt(&buf[..n]);
            if debug {
                log::info!("[worldnet] {n}B reply from {from}: {}", hex(&clear));
            }
            let mut r = Reader::new(&clear);
            let tag = r.field();
            if tag != "LIST" && tag != "LIST2" {
                if debug {
                    log::info!("[worldnet] non-LIST reply tag {tag:?}");
                }
                continue;
            }
            match parse_list(&mut r, &mut servers, want) {
                // `"0"` means the master has more to send; anything else ended the list.
                Page::More => break,
                Page::Last => break 'pages,
                // A page that doesn't start where our list ends is dropped whole, exactly as
                // the exe does (`0x1402a6aab`) — re-ask rather than splice it in at the wrong
                // index and corrupt every row after it.
                Page::OutOfOrder => continue,
            }
        }
    }
    Ok(servers)
}

/// What a `LIST` page said about whether more of them are coming.
enum Page {
    More,
    Last,
    OutOfOrder,
}

/// Parse the records out of a `LIST` reply. Layout, per the exe's read sequence
/// (`0x1402a68d4`): an index field and a flag field, then repeating records terminated by an
/// empty name — `name`, the public address (19 B), a secondary/LAN address (19 B), three `u8`
/// counters, a `≤32`-byte string, an `i32`, and a `u16`-length blob.
fn parse_list(r: &mut Reader, out: &mut Vec<WorldServer>, want: usize) -> Page {
    // The index is 1-based and must name the record we asked for, or the exe throws the whole
    // datagram away (`0x1402a6aab`: `atoi(index) - 1 == count`). A page arriving out of order
    // would otherwise be spliced in at the wrong offset.
    if r.field().trim().parse::<usize>() != Ok(want) {
        return Page::OutOfOrder;
    }
    let more = r.field().trim() == "0";
    loop {
        let name = r.field();
        if name.is_empty() {
            break; // empty name terminates the batch
        }
        let public = r.raw(19).and_then(decode_addr);
        let secondary = r.raw(19).and_then(decode_addr);
        let players = r.u8();
        let max_players = r.u8();
        let passworded = r.u8();
        // Operator free text from `[connection] location` — "USA", "EU West". Never the track:
        // the exe's own browser doesn't read this field at all.
        let location = r.field();
        let rating = r.i32_le();
        let blob_len = r.i16_le().unwrap_or(0).max(0) as usize;
        // The cursor moves by the declared length even when the payload is longer than the
        // exe's 300-byte buffer (`0x140283370`); capping the *read* instead would desync every
        // record after it.
        let blob = r.raw(blob_len).map(|b| &b[..b.len().min(MAX_BLOB)]).unwrap_or_default().to_vec();

        let Some(public) = public else {
            // No usable address means nothing to show or join; skip but keep parsing.
            if players.is_none() {
                break;
            }
            continue;
        };
        let event = Event::parse(&blob);
        let (address, joinable, lan_address) = best_address(public, secondary);
        out.push(WorldServer {
            name: name.trim().to_string(),
            address,
            joinable,
            lan_address,
            players: players.unwrap_or(0) as u32,
            max_players: max_players.unwrap_or(0) as u32,
            passworded: passworded.unwrap_or(0) != 0,
            ping_ms: None,
            location: location.trim().to_string(),
            rating: rating_class(rating.unwrap_or(0)).to_string(),
            track: event.track,
            track_layout: event.layout,
            categories: event.categories,
            bikes: event.bikes,
            session: event.session,
            race_length: event.race_length,
            conditions: event.conditions,
            realistic_weather: event.realistic_weather,
            force_cockpit: event.force_cockpit,
            no_aids: event.no_aids,
            limited_tyre_sets: event.limited_tyre_sets,
            hidden: String::new(),
        });
    }
    if more {
        Page::More
    } else {
        Page::Last
    }
}

/// The server's rating gate, as the browser renders it (`0x1400abbf6`). 0 is "no requirement".
fn rating_class(v: i32) -> &'static str {
    match v {
        1 => "D",
        2 => "C",
        3 => "B",
        4 => "A",
        _ => "",
    }
}

/// What the server is actually running, decoded from the record's blob.
///
/// The blob is opaque to the master: the game builds it (`0x140073147`) and hands it over to be
/// forwarded verbatim, and the browser parses it back at `0x1400abdac`. It is where the *real*
/// track lives — the `location` field above is what we used to show instead. Strings inside it
/// are NUL-terminated, unlike the master protocol's own `0x0A`-terminated fields.
///
/// Everything here is best-effort: a short or unfamiliar blob leaves the rest blank rather than
/// failing the row, because one odd server must not empty the tab.
#[derive(Default)]
struct Event {
    track: String,
    layout: String,
    categories: Vec<String>,
    bikes: Vec<String>,
    session: String,
    race_length: String,
    conditions: String,
    realistic_weather: bool,
    force_cockpit: bool,
    no_aids: bool,
    limited_tyre_sets: bool,
}

impl Event {
    fn parse(blob: &[u8]) -> Event {
        let mut e = Event::default();
        if blob.is_empty() {
            return e;
        }
        let mut r = Reader::new(blob);
        let _flag = r.u8();
        e.track = r.field().trim().to_string();
        e.layout = r.field().trim().to_string();
        e.categories = split_list(&r.field());
        e.bikes = split_list(&r.field());

        // The session block's shape depends on the event type, so an unknown type means we
        // stop rather than read the rule flags from the wrong offset.
        let kind = r.u8().unwrap_or(0);
        let phase = match kind {
            1 => {
                let p = r.u8().unwrap_or(0);
                if p == 0 { "Waiting" } else { "Open practice" }.to_string()
            }
            2 => {
                let _ = r.raw(5); // five flags the browser doesn't surface
                let value = r.u8().unwrap_or(0);
                let unit = r.u8().unwrap_or(0);
                let extra = r.u8().unwrap_or(0);
                e.race_length = race_length(value, unit, extra);
                RACE_PHASES.get(r.u8().unwrap_or(0) as usize).unwrap_or(&"").to_string()
            }
            4 => CUP_PHASES.get(r.u8().unwrap_or(0) as usize).unwrap_or(&"").to_string(),
            0 | 3 => String::new(),
            _ => return e,
        };
        e.session = phase;
        e.realistic_weather = r.u8().unwrap_or(0) != 0;
        e.conditions = match r.u8().unwrap_or(0) {
            0 => "Sunny",
            1 => "Cloudy",
            2 => "Rainy",
            _ => "",
        }
        .to_string();
        e.force_cockpit = r.u8().unwrap_or(0) != 0;
        e.no_aids = r.u8().unwrap_or(0) != 0;
        e.limited_tyre_sets = r.u8().unwrap_or(0) != 0;
        e
    }
}

/// Race phases, in the order the exe's string table lists them (`CC_Waiting` then
/// `CC_Practice` … `CC_Race2` at `0x108052`).
const RACE_PHASES: [&str; 8] = [
    "Waiting",
    "Practice",
    "Pre-qualify",
    "Qualify practice",
    "Qualify",
    "Warmup",
    "Race 1",
    "Race 2",
];
/// The knockout format's phases (`CC_Round` … `CC_Final`).
const CUP_PHASES: [&str; 6] = ["Waiting", "Practice", "Round", "Quarter-finals", "Semi-finals", "Final"];

/// Race length, formatted as the browser does (`0x1400ac5cd`).
fn race_length(value: u8, unit: u8, extra_laps: u8) -> String {
    match unit {
        1 => format!("{value}m + {extra_laps}"),
        2 => format!("{value}L"),
        _ => format!("{value}%"),
    }
}

/// The blob's category and bike lists are `/`-separated; an empty one means "anything".
fn split_list(s: &str) -> Vec<String> {
    s.split('/').map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect()
}

// --- Asking the servers themselves ---------------------------------------------------------

/// `GETINFO`, the connectionless query id (`0x1402a0ca0` case 0).
const GETINFO: i32 = 0;
/// `SERVERINFO`, the reply's leading id. Note it carries no `-1` prefix of its own.
const SERVERINFO: i32 = 1;
/// Marks a datagram as connectionless; a server reads the first `i32` and anything else is
/// taken for a live connection id (`0x1402a1441`).
const CONNECTIONLESS: i32 = -1;
/// How long the whole probe sweep waits for answers.
const PROBE_BUDGET: Duration = Duration::from_secs(3);

/// Ask every server about itself, and time the reply.
///
/// `GETINFO` needs no challenge, no password and no Steam ticket — the server answers whoever
/// asks, provided the version matches and it has more than one slot (`0x1402a0cea`). The
/// timestamp we send comes back untouched, which is exactly how the game measures ping
/// (`0x1402a7f88`); the master never sends one.
///
/// Best-effort throughout: a server that stays quiet keeps the row the master gave it and
/// simply shows no ping. Live answers win over the master's copy, which can be a minute stale.
fn probe(servers: &mut [WorldServer]) {
    probe_within(servers, PROBE_BUDGET)
}

/// [`probe`], with the budget named by the caller. A whole address book takes longer to hear
/// back from than the tail of one master sweep, and a single server takes no time at all.
fn probe_within(servers: &mut [WorldServer], budget: Duration) {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return;
    };
    sock.set_read_timeout(Some(Duration::from_millis(250))).ok();

    let mut waiting: std::collections::HashMap<SocketAddr, usize> = std::collections::HashMap::new();
    for (i, s) in servers.iter().enumerate() {
        // An unreachable address can't be asked, and asking costs a datagram each.
        if !s.joinable {
            continue;
        }
        if let Ok(addr) = s.address.parse::<SocketAddr>() {
            waiting.insert(addr, i);
        }
    }

    let start = Instant::now();
    let stamp = || start.elapsed().as_millis() as i32;
    for addr in waiting.keys() {
        let mut w = Writer::default();
        w.i32le(CONNECTIONLESS).i32le(GETINFO).i32le(client_version() as i32).i32le(stamp());
        sock.send_to(&encrypt(w.finish()), addr).ok();
    }

    let deadline = start + budget;
    let mut buf = [0u8; 4096];
    while !waiting.is_empty() && Instant::now() < deadline {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue; // a tick with nothing on it; keep waiting until the budget is out
        };
        let Some(i) = waiting.remove(&from) else {
            continue;
        };
        if let Some(info) = parse_serverinfo(&decrypt(&buf[..n.min(MAX_REPLY)])) {
            let s = &mut servers[i];
            s.ping_ms = Some(stamp().saturating_sub(info.echo).max(0) as u32);
            // The server's own name over the master's forwarded copy — an operator who renamed
            // it half an hour ago shows as renamed, and a row probed with no name at all (the
            // detail panel's single-server refresh) gets one.
            if !info.name.is_empty() {
                s.name = info.name;
            }
            s.players = info.players as u32;
            s.max_players = info.max_players as u32;
            s.passworded = info.passworded;
            // The server's own blob is fresher than the master's forwarded copy — a session
            // that has rolled over to the next race shows the race, not the warmup.
            if !info.event.track.is_empty() {
                let e = info.event;
                s.track = e.track;
                s.track_layout = e.layout;
                s.categories = e.categories;
                s.bikes = e.bikes;
                s.session = e.session;
                s.race_length = e.race_length;
                s.conditions = e.conditions;
                s.realistic_weather = e.realistic_weather;
                s.force_cockpit = e.force_cockpit;
                s.no_aids = e.no_aids;
                s.limited_tyre_sets = e.limited_tyre_sets;
            }
        }
    }
}

/// A `SERVERINFO` reply, as the client reads it back (`0x1402a7ead`).
struct ServerInfo {
    echo: i32,
    name: String,
    players: u8,
    max_players: u8,
    passworded: bool,
    event: Event,
}

/// Parse `SERVERINFO`. The field order is *not* the master's: this message writes the seat cap
/// before the rider count (`0x1402a0d6d`), where `LIST` writes them the other way round — so
/// the two parsers stay apart however similar the records look.
fn parse_serverinfo(clear: &[u8]) -> Option<ServerInfo> {
    let mut r = Reader::new(clear);
    if r.i32_le()? != SERVERINFO {
        return None;
    }
    let echo = r.i32_le()?;
    let name = r.field().trim().to_string();
    let max_players = r.u8()?;
    let players = r.u8()?;
    let passworded = r.u8()? != 0;
    let _announced_track = r.field(); // start-time copy of `location`; the blob is the truth
    let _id = r.i32_le();
    let blob_len = r.i16_le().unwrap_or(0).max(0) as usize;
    let blob = r.raw(blob_len).map(|b| &b[..b.len().min(MAX_BLOB)]).unwrap_or_default();
    Some(ServerInfo { echo, name, players, max_players, passworded, event: Event::parse(blob) })
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
///
/// The ticket itself has to be the right kind of ticket: an encrypted app ticket, which the
/// master opens with the app's encryption key. A `GetAuthSessionTicket` blob is the same
/// length and the same shape on the wire, gets the same `LOGIN` accepted as far as parsing,
/// and is answered `Auth NO Invalid Account`.
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

// --- Steam app ticket -------------------------------------------------------------------
//
// The encrypted app ticket the master signs a player in with, and the SteamID64 it belongs
// to, obtained by loading Steam's flat C API from `steam_api64.dll` at runtime — no
// build-time linkage, matching how `steamid.rs` reads Steam without the SDK. Windows only;
// unverifiable off Windows, so elsewhere it's a clear error and the browse path is what runs.

/// What a Steam sign-in needs: the account, and the ticket that proves it.
#[derive(Clone)]
struct SteamAuth {
    steam_id: u64,
    ticket: Vec<u8>,
}

/// How long one sign-in is reused before Steam is asked for another. Steam mints at most one
/// ticket a minute and hands the same one back in between, so asking more often buys nothing.
const TICKET_TTL: Duration = Duration::from_secs(10 * 60);

/// The sign-in this run is using.
static SIGN_IN: Mutex<Option<(Instant, SteamAuth)>> = Mutex::new(None);

/// The ticket to sign in with — this run's, until it ages out.
///
/// Asking Steam twice in one process does not work: `SteamAPI_Init` after a `SteamAPI_Shutdown`
/// comes back with no `ISteamUser`, so the second refresh of a session used to fail with
/// "Steam user interface unavailable." and empty the tab. The ticket is a sealed blob that
/// outlives the API that issued it, so keep it rather than ask again.
fn signed_in(install: &Path) -> Result<SteamAuth, String> {
    let mut held = SIGN_IN.lock().unwrap_or_else(|p| p.into_inner());
    reuse_or_fetch(&mut held, Instant::now(), || steam_ticket(install))
}

/// Reuse a ticket younger than [`TICKET_TTL`]; otherwise ask for a fresh one, and fall back to
/// the one in hand when Steam won't issue another.
fn reuse_or_fetch(
    held: &mut Option<(Instant, SteamAuth)>,
    now: Instant,
    fresh: impl FnOnce() -> Result<SteamAuth, String>,
) -> Result<SteamAuth, String> {
    if let Some((issued, auth)) = held.as_ref() {
        if now.saturating_duration_since(*issued) < TICKET_TTL {
            return Ok(auth.clone());
        }
    }
    match fresh() {
        Ok(auth) => {
            *held = Some((now, auth.clone()));
            Ok(auth)
        }
        Err(e) => match held.as_ref() {
            Some((_, auth)) => {
                log::info!("[worldnet] Steam wouldn't issue another ticket ({e}); reusing this run's");
                Ok(auth.clone())
            }
            None => Err(e),
        },
    }
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
    use std::ptr::null_mut;
    use std::time::{Duration, Instant};

    /// MX Bikes' Steam AppID, so the ticket authenticates as this game.
    const APPID: &str = "655500";
    const DLL: &str = "steam_api64.dll";

    /// The four bytes the game has Steam seal into the ticket: the constant written at
    /// `0x1401308b6` and handed to `RequestEncryptedAppTicket` at `0x1401308de`. They ride
    /// inside the encrypted blob, so the master reads them back out of it — send the game's.
    const TICKET_USER_DATA: [u8; 4] = 0x4d12_447du32.to_le_bytes();
    /// `k_nSteamEncryptedAppTicketSizeMax`, and the size of the game's own ticket buffer.
    const MAX_TICKET: i32 = 1024;
    /// `EncryptedAppTicketResponse_t::k_iCallback` — `k_iSteamUserCallbacks` (100) + 54.
    const TICKET_RESPONSE: i32 = 154;
    /// `k_EResultOK`.
    const RESULT_OK: i32 = 1;
    /// How long Steam gets to come back with a ticket. The game pumps callbacks in a 10 ms
    /// loop with no deadline at all (`0x140130940`); a server refresh has to end.
    const TICKET_WAIT: Duration = Duration::from_secs(10);

    type InitFn = unsafe extern "C" fn() -> bool;
    type ShutdownFn = unsafe extern "C" fn();
    type RunCallbacksFn = unsafe extern "C" fn();
    type IfaceFn = unsafe extern "C" fn() -> *mut c_void;
    type CreateInterfaceFn = unsafe extern "C" fn(*const u8) -> *mut c_void;
    type HandleFn = unsafe extern "C" fn() -> i32;
    // GetISteamUser(client, hUser, hPipe, version) -> ISteamUser*
    type GetIUserFn = unsafe extern "C" fn(*mut c_void, i32, i32, *const u8) -> *mut c_void;
    // GetISteamUtils(client, hPipe, version) -> ISteamUtils* — no user handle, unlike the rest.
    type GetIUtilsFn = unsafe extern "C" fn(*mut c_void, i32, *const u8) -> *mut c_void;
    // RequestEncryptedAppTicket(self, pDataToInclude, cbDataToInclude) -> SteamAPICall_t
    type RequestTicketFn = unsafe extern "C" fn(*mut c_void, *const u8, i32) -> u64;
    // GetEncryptedAppTicket(self, buf, cbMaxTicket, *pcbTicket) -> bool
    type GetTicketFn = unsafe extern "C" fn(*mut c_void, *mut u8, i32, *mut u32) -> bool;
    // GetAPICallResult(self, call, buf, cubCallback, iCallbackExpected, *pbFailed) -> bool
    type CallResultFn = unsafe extern "C" fn(*mut c_void, u64, *mut c_void, i32, i32, *mut bool) -> bool;
    // GetSteamID(self) -> CSteamID. A `CSteamID` is one 8-byte value and comes back in RAX
    // either way, so the same signature reads an old DLL and a new one.
    type GetSteamIdFn = unsafe extern "C" fn(*mut c_void) -> u64;

    /// Every `ISteamUser` accessor Valve has shipped, newest first. A DLL from before they
    /// existed is handled by [`via_client`].
    const ACCESSORS: [&[u8]; 5] = [
        b"SteamAPI_SteamUser_v023\0",
        b"SteamAPI_SteamUser_v022\0",
        b"SteamAPI_SteamUser_v021\0",
        b"SteamAPI_SteamUser_v020\0",
        b"SteamAPI_SteamUser\0",
    ];
    /// The same for `ISteamUtils`, which only reports on the request; missing it costs the
    /// reason for a refusal, not the ticket.
    const UTILS_ACCESSORS: [&[u8]; 4] = [
        b"SteamAPI_SteamUtils_v010\0",
        b"SteamAPI_SteamUtils_v009\0",
        b"SteamAPI_SteamUtils_v008\0",
        b"SteamAPI_SteamUtils\0",
    ];
    /// Interface versions for the old route, the game's own first (`SteamClient017` /
    /// `SteamUser019` / `SteamUtils009` at `0x140130288`). Steam still serves an old version
    /// to a new DLL.
    const CLIENTS: [&[u8]; 4] = [b"SteamClient017\0", b"SteamClient020\0", b"SteamClient021\0", b"SteamClient022\0"];
    const USERS: [&[u8]; 5] = [b"SteamUser019\0", b"SteamUser020\0", b"SteamUser021\0", b"SteamUser022\0", b"SteamUser023\0"];
    const UTILS: [&[u8]; 3] = [b"SteamUtils009\0", b"SteamUtils010\0", b"SteamUtils008\0"];

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

    /// The interfaces a sign-in needs: `ISteamUser` asks for the ticket, `ISteamUtils` says
    /// how the request Steam went away to answer came back. Utils may be null — then the wait
    /// simply polls for the ticket, and a refusal reads as a timeout.
    struct Ifaces {
        user: *mut c_void,
        utils: *mut c_void,
    }

    /// The first accessor in the list that the DLL exports and that hands back an interface.
    unsafe fn by_accessor(lib: &Library, names: &[&[u8]]) -> *mut c_void {
        names
            .iter()
            .find_map(|name| lib.get::<IfaceFn>(name).ok().map(|f| f()))
            .filter(|p| !p.is_null())
            .unwrap_or(null_mut())
    }

    /// The old route, for a DLL that predates the accessors: create the client interface and
    /// ask it, exactly as the game does — all exported functions, no vtables.
    unsafe fn via_client(lib: &Library) -> Option<(*mut c_void, *mut c_void)> {
        let create: Symbol<CreateInterfaceFn> = lib.get(b"SteamInternal_CreateInterface\0").ok()?;
        let get_user: Symbol<GetIUserFn> = lib.get(b"SteamAPI_ISteamClient_GetISteamUser\0").ok()?;
        let get_utils = lib.get::<GetIUtilsFn>(b"SteamAPI_ISteamClient_GetISteamUtils\0").ok();
        let h_user: Symbol<HandleFn> = lib.get(b"SteamAPI_GetHSteamUser\0").ok()?;
        let h_pipe: Symbol<HandleFn> = lib.get(b"SteamAPI_GetHSteamPipe\0").ok()?;
        let (user_handle, pipe) = (h_user(), h_pipe());
        for client_ver in CLIENTS {
            let client = create(client_ver.as_ptr());
            if client.is_null() {
                continue;
            }
            let user = USERS
                .iter()
                .map(|v| get_user(client, user_handle, pipe, v.as_ptr()))
                .find(|p| !p.is_null());
            let utils = get_utils
                .as_ref()
                .and_then(|f| UTILS.iter().map(|v| f(client, pipe, v.as_ptr())).find(|p| !p.is_null()));
            if let Some(user) = user {
                return Some((user, utils.unwrap_or(null_mut())));
            }
        }
        None
    }

    /// Newer DLLs hand the interfaces over directly; the game ships one old enough to predate
    /// that (it asks Steam for `SteamUser019`), so fall back to the client for whichever the
    /// accessors didn't produce.
    unsafe fn interfaces(lib: &Library) -> Option<Ifaces> {
        let mut user = by_accessor(lib, &ACCESSORS);
        let mut utils = by_accessor(lib, &UTILS_ACCESSORS);
        if user.is_null() || utils.is_null() {
            if let Some((u, ut)) = via_client(lib) {
                if user.is_null() {
                    user = u;
                }
                if utils.is_null() {
                    utils = ut;
                }
            }
        }
        (!user.is_null()).then_some(Ifaces { user, utils })
    }

    /// Whether the ticket request has been answered, and with what — `None` while Steam is
    /// still thinking, or when this DLL has no `ISteamUtils` to ask.
    unsafe fn answered(lib: &Library, ifaces: &Ifaces, call: u64) -> Option<i32> {
        if ifaces.utils.is_null() || call == 0 {
            return None;
        }
        let get: Symbol<CallResultFn> = lib.get(b"SteamAPI_ISteamUtils_GetAPICallResult\0").ok()?;
        // `EncryptedAppTicketResponse_t` is one `EResult`, and nothing else.
        let mut result: i32 = 0;
        let mut failed = false;
        let done = get(
            ifaces.utils,
            call,
            &mut result as *mut i32 as *mut c_void,
            std::mem::size_of::<i32>() as i32,
            TICKET_RESPONSE,
            &mut failed,
        );
        // An IO failure is an answer too: nothing further is coming for this call.
        done.then_some(if failed { 0 } else { result })
    }

    /// Ask Steam for the encrypted app ticket and wait for it, the way the game does at
    /// `0x140130860`: request it with the game's four bytes of user data, pump callbacks, then
    /// collect. Steam holds the issued ticket for us, so a request it declines to repeat —
    /// they're rate-limited to one a minute — still collects the one already in hand; that's
    /// why the collect comes before the verdict is judged.
    unsafe fn fetch_ticket(lib: &Library, ifaces: &Ifaces) -> Result<Vec<u8>, String> {
        let request: Symbol<RequestTicketFn> = lib
            .get(b"SteamAPI_ISteamUser_RequestEncryptedAppTicket\0")
            .map_err(|e| format!("this {DLL} can't request an app ticket: {}", why(&e)))?;
        let collect: Symbol<GetTicketFn> = lib
            .get(b"SteamAPI_ISteamUser_GetEncryptedAppTicket\0")
            .map_err(|e| format!("this {DLL} has no app-ticket call: {}", why(&e)))?;
        let run: Symbol<RunCallbacksFn> = lib.get(b"SteamAPI_RunCallbacks\0").map_err(|e| why(&e))?;

        let call = request(ifaces.user, TICKET_USER_DATA.as_ptr(), TICKET_USER_DATA.len() as i32);
        let mut ticket = vec![0u8; MAX_TICKET as usize];
        let mut written: u32 = 0;
        let mut verdict: Option<i32> = None;
        let deadline = Instant::now() + TICKET_WAIT;
        loop {
            run();
            if collect(ifaces.user, ticket.as_mut_ptr(), MAX_TICKET, &mut written) && written > 0 {
                ticket.truncate(written as usize);
                return Ok(ticket);
            }
            // One more pass after the answer lands, then give up: the ticket is there the
            // moment the call result is, and nothing else will bring it.
            if verdict.is_some() {
                break;
            }
            verdict = answered(lib, ifaces, call);
            if verdict.is_none() {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Err(match verdict {
            Some(RESULT_OK) | None => "Steam didn't answer the app-ticket request in time.".to_string(),
            Some(0) => "Steam couldn't be reached for an app ticket.".to_string(),
            Some(e) => format!("Steam refused an app ticket for MX Bikes (EResult {e})."),
        })
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
            let Some(ifaces) = interfaces(&lib) else {
                shutdown();
                return Err("Steam user interface unavailable.".into());
            };
            // The master is told which account the ticket belongs to, so ask the interface
            // that issued it rather than guessing from a name.
            let steam_id = lib
                .get::<GetSteamIdFn>(b"SteamAPI_ISteamUser_GetSteamID\0")
                .ok()
                .map(|f| f(ifaces.user))
                .unwrap_or(0);
            let ticket = fetch_ticket(&lib, &ifaces);
            // The ticket is a sealed blob, not a live session: it outlives the API we got it
            // from, so shutting down here costs nothing the master needs.
            shutdown();
            // The ticket is the harder thing to get and the more useful thing to report, so
            // its failure is the one that speaks.
            let ticket = ticket?;
            if steam_id == 0 {
                return Err("Steam didn't say which account is signed in.".into());
            }
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
        let a = decode_addr(&v).unwrap();
        assert_eq!(a.text, "203.0.113.10:54210");
        assert!(a.joinable);
    }

    /// An IPv4 server the master saw through a dual-stack socket comes back flagged as IPv6
    /// with the real address in the last four bytes. Rendering that as `[::ffff:…]` produced a
    /// row whose Join `gameproc::parse_server_address` refuses outright.
    #[test]
    fn an_ipv4_mapped_address_unwraps_to_the_address_the_game_can_reach() {
        let mut v = vec![0u8; 19];
        v[0] = 1; // IPv6
        v[11] = 0xFF;
        v[12] = 0xFF;
        v[13..17].copy_from_slice(&[203, 0, 113, 10]);
        v[17] = 0xD3;
        v[18] = 0xC2;
        let a = decode_addr(&v).unwrap();
        assert_eq!(a.text, "203.0.113.10:54210");
        assert!(a.joinable);
    }

    /// A real IPv6 server is shown but not offered — the game takes one address on its command
    /// line and the connect flag won't take a bracketed one.
    #[test]
    fn a_real_ipv6_server_is_not_joinable() {
        let mut v = vec![0u8; 19];
        v[0] = 1;
        v[1..3].copy_from_slice(&[0x2a, 0x01]);
        v[17] = 0xD3;
        v[18] = 0xC2;
        let a = decode_addr(&v).unwrap();
        assert!(a.text.starts_with("[2a01:"), "{}", a.text);
        assert!(!a.joinable);
    }

    fn addr(text: &str, joinable: bool) -> Addr {
        Addr { text: text.to_string(), joinable }
    }

    /// A server that registered over IPv6 but has a routable IPv4 interface is joinable via
    /// the second address — which is what the game does anyway, by asking both at once.
    #[test]
    fn an_ipv6_row_falls_back_to_its_routable_second_address() {
        let (address, joinable, lan) =
            best_address(addr("[2a01::1]:54210", false), Some(addr("203.0.113.10:54210", true)));
        assert_eq!(address, "203.0.113.10:54210");
        assert!(joinable);
        assert!(lan.is_empty(), "the address we're joining isn't also side detail");
    }

    /// The server-reported address is whatever its own hostname resolved to, so it is often a
    /// LAN address. That can't rescue an IPv6 row, and the row must stay honest about it.
    #[test]
    fn a_lan_second_address_cannot_rescue_an_ipv6_row() {
        let (address, joinable, lan) =
            best_address(addr("[2a01::1]:54210", false), Some(addr("192.168.1.20:54210", false)));
        assert_eq!(address, "[2a01::1]:54210");
        assert!(!joinable);
        assert_eq!(lan, "192.168.1.20:54210", "still worth showing as detail");
    }

    /// A server whose two addresses agree shouldn't report the same thing twice.
    #[test]
    fn a_duplicate_second_address_is_not_shown() {
        let (_, _, lan) = best_address(addr("203.0.113.10:54210", true), Some(addr("203.0.113.10:54210", true)));
        assert!(lan.is_empty());
    }

    /// The event blob a server publishes about itself — where the real track lives.
    fn race_blob() -> Vec<u8> {
        let mut b = vec![0u8]; // leading flag
        b.extend(b"mmx_supercross\0");
        b.extend(b"Night\0");
        b.extend(b"MX1/MX2\0");
        b.extend(b"\0"); // no bike restriction
        b.push(2); // event type 2: a race
        b.extend([0, 0, 0, 0, 0]); // five flags the browser doesn't surface
        b.extend([30, 1, 2]); // 30 minutes + 2 laps
        b.push(6); // phase 6 = Race 1
        b.extend([1, 2, 1, 1, 0]); // realistic weather, rainy, cockpit, no aids, tyres free
        b
    }

    /// One record, built the way the master does.
    fn list_page(index: i64, more: bool, name: &str, blob: &[u8]) -> Vec<u8> {
        let mut addr = vec![0u8; 19];
        addr[1..5].copy_from_slice(&[203, 0, 113, 10]);
        addr[5] = 0xD3;
        addr[6] = 0xC2; // 54210

        let mut w = Writer::default();
        w.field("LIST").int(index).field(if more { "0" } else { "1" });
        w.field(name);
        w.raw(&addr); // public
        w.raw(&vec![0u8; 19]); // secondary
        w.raw(&[7, 20, 1]); // players=7, max=20, passworded=1
        w.field("USA"); // location — the field we used to render as the track
        w.raw(&3i32.to_le_bytes()); // rating class B
        w.raw(&(blob.len() as i16).to_le_bytes());
        w.raw(blob);
        w.field(""); // terminating empty name
        w.finish()
    }

    fn parse_page(body: Vec<u8>, out: &mut Vec<WorldServer>, want: usize) -> Page {
        let clear = decrypt(&encrypt(body));
        let mut r = Reader::new(&clear);
        assert_eq!(r.field(), "LIST");
        parse_list(&mut r, out, want)
    }

    #[test]
    fn parses_a_synthetic_list_reply() {
        let mut out = Vec::new();
        parse_page(list_page(1, false, "Frost's Server", &race_blob()), &mut out, 1);
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.name, "Frost's Server");
        assert_eq!(s.address, "203.0.113.10:54210");
        assert!(s.joinable);
        assert_eq!(s.players, 7);
        assert_eq!(s.max_players, 20);
        assert!(s.passworded);

        // The two fields that were being conflated.
        assert_eq!(s.location, "USA");
        assert_eq!(s.track, "mmx_supercross");
        assert_eq!(s.track_layout, "Night");
        assert_eq!(s.rating, "B");

        assert_eq!(s.categories, ["MX1", "MX2"]);
        assert!(s.bikes.is_empty(), "an empty list means any bike");
        assert_eq!(s.session, "Race 1");
        assert_eq!(s.race_length, "30m + 2");
        assert_eq!(s.conditions, "Rainy");
        assert!(s.realistic_weather && s.force_cockpit && s.no_aids);
        assert!(!s.limited_tyre_sets);
    }

    /// The bug behind "only 9 servers worldwide": the master pages, and asking once got page
    /// one. `"0"` in the second field means more are coming.
    #[test]
    fn a_page_says_whether_more_follow() {
        let mut out = Vec::new();
        assert!(matches!(parse_page(list_page(1, true, "One", &[]), &mut out, 1), Page::More));
        assert!(matches!(parse_page(list_page(2, false, "Two", &[]), &mut out, 2), Page::Last));
        assert_eq!(out.len(), 2);
    }

    /// A page that doesn't start where our list ends is thrown away whole, as the exe does —
    /// splicing it in would put every following row at the wrong index.
    #[test]
    fn a_page_arriving_out_of_order_is_dropped() {
        let mut out = Vec::new();
        assert!(matches!(parse_page(list_page(7, true, "Late", &[]), &mut out, 1), Page::OutOfOrder));
        assert!(out.is_empty());
    }

    /// A blob from a build we don't recognise must cost that server its detail, not its row.
    #[test]
    fn an_unknown_event_type_leaves_the_row_standing() {
        let mut blob = vec![0u8];
        blob.extend(b"someTrack\0\0\0\0");
        blob.push(9); // an event type with no known shape
        blob.extend([1, 1, 1, 1, 1]);

        let mut out = Vec::new();
        parse_page(list_page(1, false, "Odd", &blob), &mut out, 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].track, "someTrack");
        assert_eq!(out[0].session, "", "an unreadable session is blank, not a guess");
        assert!(!out[0].no_aids, "flags past an unknown block are never read");
    }

    /// `SERVERINFO` writes the seat cap before the rider count; `LIST` writes them the other
    /// way round. Sharing one parser between them would swap the two on every row.
    #[test]
    fn serverinfo_puts_the_seat_cap_first() {
        let mut w = Writer::default();
        w.i32le(SERVERINFO).i32le(1234);
        w.raw(b"Frost's Server\0");
        w.raw(&[20, 7, 1]); // max=20, players=7, passworded
        w.raw(b"USA\0");
        w.raw(&0i32.to_le_bytes());
        let blob = race_blob();
        w.raw(&(blob.len() as i16).to_le_bytes()).raw(&blob);

        let info = parse_serverinfo(&w.finish()).expect("a well-formed SERVERINFO");
        assert_eq!(info.echo, 1234);
        assert_eq!(info.max_players, 20);
        assert_eq!(info.players, 7);
        assert!(info.passworded);
        assert_eq!(info.event.track, "mmx_supercross");
    }

    /// The 16-byte request is two whole Blowfish blocks, and the version gate is exact: a
    /// server compares it to 40 and answers nothing otherwise.
    #[test]
    fn getinfo_is_sixteen_bytes_of_connectionless_request() {
        let mut w = Writer::default();
        w.i32le(CONNECTIONLESS).i32le(GETINFO).i32le(client_version() as i32).i32le(99);
        let body = w.finish();
        assert_eq!(body.len(), 16);
        assert_eq!(encrypt(body.clone()).len(), 16, "no padding block is added");

        let mut r = Reader::new(&body);
        assert_eq!(r.i32_le(), Some(-1));
        assert_eq!(r.i32_le(), Some(0));
        assert_eq!(r.i32_le(), Some(40));
    }

    fn auth(id: u64) -> SteamAuth {
        SteamAuth { steam_id: id, ticket: vec![1, 2, 3] }
    }

    /// The second refresh of a session must not ask Steam again — asking is what failed:
    /// `SteamAPI_Init` after a shutdown has no `ISteamUser`, and the tab went empty.
    #[test]
    fn a_second_refresh_reuses_this_run_s_sign_in() {
        let t0 = Instant::now();
        let mut held = None;
        reuse_or_fetch(&mut held, t0, || Ok(auth(7))).unwrap();

        let mut asked = false;
        let again = reuse_or_fetch(&mut held, t0 + Duration::from_secs(30), || {
            asked = true;
            Err("Steam user interface unavailable.".into())
        })
        .unwrap();
        assert!(!asked, "Steam was asked a second time");
        assert_eq!(again.steam_id, 7);
    }

    /// Past the TTL it asks again — and a refusal still leaves a working list, not an error.
    #[test]
    fn a_refusal_falls_back_to_the_ticket_in_hand() {
        let t0 = Instant::now();
        let mut held = None;
        reuse_or_fetch(&mut held, t0, || Ok(auth(7))).unwrap();

        let stale = t0 + TICKET_TTL + Duration::from_secs(1);
        let out = reuse_or_fetch(&mut held, stale, || Err("Steam user interface unavailable.".into())).unwrap();
        assert_eq!(out.steam_id, 7);

        // A fresh one, when Steam does answer, replaces it.
        let out = reuse_or_fetch(&mut held, stale, || Ok(auth(9))).unwrap();
        assert_eq!(out.steam_id, 9);
    }

    /// The probe-only refresh, end to end against a server that actually answers.
    ///
    /// This is the path the tab runs on while MX Bikes is up, so it is worth exercising for
    /// real rather than by parsing a handmade buffer: a socket, the request the game sends,
    /// the cipher in both directions, and the reply parsed back into a row. A regression in
    /// any one of those is the difference between a live list and an empty tab.
    #[test]
    fn a_remembered_server_is_refreshed_by_asking_it() {
        // Stand in for a dedicated server: read one GETINFO, answer one SERVERINFO.
        let server = UdpSocket::bind("127.0.0.1:0").expect("a loopback socket");
        let addr = server.local_addr().unwrap();
        server.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let listening = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let (n, from) = server.recv_from(&mut buf).expect("the probe's request");
            let asked = decrypt(&buf[..n]);

            // What the game sends, and what a server checks before answering at all.
            let mut r = Reader::new(&asked);
            assert_eq!(r.i32_le(), Some(CONNECTIONLESS));
            assert_eq!(r.i32_le(), Some(GETINFO));
            assert_eq!(r.i32_le(), Some(client_version() as i32));
            let stamp = r.i32_le().expect("a client timestamp to echo");

            let mut w = Writer::default();
            w.i32le(SERVERINFO).i32le(stamp);
            w.raw(b"Frost's Server\0");
            w.raw(&[20, 7, 1]); // max, then players, then passworded — this message's order
            w.raw(b"USA\0");
            w.raw(&0i32.to_le_bytes());
            let blob = race_blob();
            w.raw(&(blob.len() as i16).to_le_bytes()).raw(&blob);
            server.send_to(&encrypt(w.finish()), from).ok();
        });

        // A row as the address book hands one over: an address, and nothing else worth trusting.
        let mut rows = vec![WorldServer {
            name: "stale name".into(),
            address: addr.to_string(),
            joinable: true,
            ..Default::default()
        }];
        probe_within(&mut rows, Duration::from_secs(5));
        listening.join().expect("the stand-in server");

        let s = &rows[0];
        assert!(s.ping_ms.is_some(), "a server that answered has a measured ping");
        assert_eq!(s.players, 7);
        assert_eq!(s.max_players, 20);
        assert!(s.passworded);
        assert_eq!(s.name, "Frost's Server", "the server's own name replaces the remembered one");
        assert_eq!(s.track, "mmx_supercross", "the event blob is read from the live reply");
    }

    /// A remembered address that has gone quiet must not be listed. Showing a name beside a
    /// rider count from last week reads as current, which is worse than an absent row.
    #[test]
    fn a_server_that_doesnt_answer_is_dropped_from_a_book_only_refresh() {
        // Port 1 on loopback: nothing is listening, and nothing will start.
        let err = from_book(vec![WorldServer {
            name: "gone".into(),
            address: "127.0.0.1:1".into(),
            joinable: true,
            ..Default::default()
        }])
        .unwrap_err();
        assert!(err.contains("answered"), "unhelpful: {err}");
    }

    /// With no book at all the tab has to say what would fill it, not just fail.
    #[test]
    fn an_empty_book_explains_itself() {
        let err = from_book(vec![]).unwrap_err();
        assert!(err.contains("MX Bikes closed"), "unhelpful: {err}");
    }

    /// The budget grows with the book but stays bounded — a refresh is something a player is
    /// watching, and a large book must not turn it into a minute of spinner.
    #[test]
    fn the_probe_budget_is_bounded() {
        assert!(book_budget(0) >= PROBE_BUDGET);
        assert!(book_budget(10_000) <= Duration::from_secs(10));
        assert!(book_budget(500) > book_budget(5), "a bigger book waits longer");
    }

    /// With nothing in hand there is nothing to fall back to, and the Steam reason is what
    /// the tab should say.
    #[test]
    fn the_first_failure_still_speaks() {
        let mut held = None;
        let e = reuse_or_fetch(&mut held, Instant::now(), || Err("Steam isn't running.".into()))
            .map(|_| ())
            .unwrap_err();
        assert_eq!(e, "Steam isn't running.");
    }
}
