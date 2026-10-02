//! Paint sync v2: one room per server, and only the difference crosses the wire.
//!
//! The roster path ([`crate::paintsync::pull`]) asked "who is here and what are they wearing"
//! on a timer, and then hashed its way through the grid to find out what it already had. A
//! room turns that around. Joining (`POST /v1/paintsync/join`) says where this rider is and
//! what they wear, and comes back with the hashes of theirs the control plane lacks and
//! everyone else on the server. After that the room's WebSocket says when someone arrives,
//! changes their look, or leaves — so nothing is polled, and both directions carry only what
//! the other side is missing.
//!
//! **Identity is the hash.** MX Bikes picks a remote rider's paint by the file name that rider
//! chose, so two riders using one name for different artwork is a collision the game cannot
//! express, and renaming either file would mean the game never loads it. What can be done is
//! to keep every variant: each paint received lands in a store under the app data dir, keyed
//! by its SHA-256 and outside the game's folders, and the destination holds whichever variant
//! most of the grid wears ([`pick_variants`]). When the grid changes the file is swapped from
//! the store, with no download.
//!
//! Everything the roster path promised still holds: nothing but `.pnt` files are written,
//! only below the mods folder ([`safe_dest`]), only after their digest is checked, and never
//! over a file the player made ([`Manifest`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::paintsync::{
    self, control_plane, safe_dest, sha256_bytes, BikeLoadout, Manifest, PaintEntry, PullOutcome,
};
use crate::voice::gamesession;

/// How often presence is refreshed: a ping on the socket, or a fresh join when the socket is
/// down. Half the control plane's ten-minute presence window, so one lost beat is survivable.
pub const HEARTBEAT: Duration = Duration::from_secs(5 * 60);

/// The first wait before reopening a dropped room, doubled on each failure up to [`HEARTBEAT`].
const BACKOFF_MIN: Duration = Duration::from_secs(5);

/// How long a received paint, or a stored variant, outlives the last rider seen wearing it.
/// A week covers the riders someone races with every few days without the store becoming a
/// copy of everything ever seen on a grid.
pub const KEEP_FOR_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// The hash index is rebuilt at most this often while looking for a paint. Walking the mods
/// folder is stat calls, but it is every file the player owns.
const INDEX_REFRESH_EVERY: Duration = Duration::from_secs(60);

/// Whether the control plane has paint rooms: `0` not asked yet, `1` yes, `2` no.
///
/// Remembered for the life of the process so the launch-time roster pull knows whether a room
/// will cover it. A control plane that answered 404 once will not grow the route mid-session.
static ROOMS: AtomicU8 = AtomicU8::new(0);

/// Who is in the room right now, by rider name, folded. `None` while not in one. Read by the
/// Settings panel to name the riders on the grid who aren't sharing paints.
static ROOM_RIDERS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

fn fold_rider(name: &str) -> String {
    name.trim().to_lowercase()
}

fn set_room_riders(names: Option<Vec<String>>) {
    *ROOM_RIDERS.lock().unwrap_or_else(|e| e.into_inner()) = names;
}

/// The riders in the paint-sync room, folded; `None` when the app isn't in one.
pub fn room_rider_names() -> Option<Vec<String>> {
    ROOM_RIDERS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The riders on the game's own grid who aren't in the paint-sync room: they can't see this
/// player's paints and nobody here receives theirs. `me` and blank names are never listed.
///
/// Matched by rider name, folded, which is the one identity both sides carry: the grid from
/// FrostMod's session block, the room from each account's rider name.
pub fn not_sharing(grid: &[String], me: &str, room: &[String]) -> Vec<String> {
    let me = fold_rider(me);
    let mut out: Vec<String> = Vec::new();
    for name in grid {
        let f = fold_rider(name);
        if f.is_empty() || f == me || room.contains(&f) || out.iter().any(|o| fold_rider(o) == f) {
            continue;
        }
        out.push(name.trim().to_string());
    }
    out
}

/// The control plane answered a join with 404, so this run of the app syncs by roster.
pub fn rooms_unavailable() -> bool {
    ROOMS.load(Ordering::SeqCst) == 2
}

/// Set when the rider's own look changed, so the room re-joins with it. A join is what tells
/// everyone else on the server; `PUT /v1/loadouts` alone would not reach the room.
static LOOK_CHANGED: AtomicBool = AtomicBool::new(false);

pub fn note_look_changed() {
    LOOK_CHANGED.store(true, Ordering::SeqCst);
}

// ── Wire ─────────────────────────────────────────────────────────────────────

/// What this app knows about the server it is on. At least one field is set.
///
/// An app-launched join knows the address; a rider who picked the server in the game's own
/// browser gives us only the name FrostMod reads out of the session. The control plane turns
/// either into the one key it echoes back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ServerRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ServerRef {
    /// Whether two descriptions name the same server. An address settles it when both have
    /// one; otherwise the folded names do.
    pub fn same_server(&self, other: &ServerRef) -> bool {
        if let (Some(a), Some(b)) = (&self.address, &other.address) {
            return a.trim().eq_ignore_ascii_case(b.trim());
        }
        match (&self.name, &other.name) {
            (Some(a), Some(b)) => fold(a) == fold(b),
            _ => false,
        }
    }
}

/// A server name folded the way every rider's app folds it. See `voice::session::room_key`.
fn fold(name: &str) -> String {
    crate::voice::session::room_key(name)
}

/// Another rider in the room.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRider {
    pub rider_name: String,
    #[serde(default)]
    pub guid: Option<String>,
    /// When they joined, in ms. Breaks a tie between two variants of one file.
    #[serde(default)]
    pub joined_at: u64,
    #[serde(default)]
    pub paints: Vec<PaintEntry>,
}

impl RoomRider {
    /// The GUID where they have claimed one, their name otherwise — the same preference the
    /// roster path and the control plane group by.
    pub fn key(&self) -> String {
        rider_key(&self.rider_name, self.guid.as_deref())
    }
}

fn rider_key(name: &str, guid: Option<&str>) -> String {
    match guid.map(str::trim).filter(|g| !g.is_empty()) {
        Some(guid) => guid.to_string(),
        None => format!("name:{}", name.trim().to_lowercase()),
    }
}

/// `POST /v1/paintsync/join`, answered.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Joined {
    /// The key the control plane settled on. Echoed on every later call.
    pub server: String,
    /// Hashes from this rider's look the control plane does not hold. Upload these, nothing else.
    #[serde(default)]
    pub missing: Vec<String>,
    /// Everyone else on the server, never this rider.
    #[serde(default)]
    pub riders: Vec<RoomRider>,
    /// The room's path, `/v1/paintsync/room?server=…`.
    #[serde(default)]
    pub room: String,
}

pub enum JoinReply {
    Joined(Joined),
    /// 404: a control plane from before paint rooms. The roster path still works there.
    NotDeployed,
    /// 429, and how long it asked us to wait.
    RateLimited(Duration),
}

/// Say which server this rider is on, and — when `bikes` is given — what they are wearing.
///
/// Idempotent, so it doubles as the heartbeat while the room's socket is down. Leaving
/// `bikes` out keeps whatever look was stored last.
pub async fn join(
    token: &str,
    server: &ServerRef,
    bikes: Option<&[BikeLoadout]>,
) -> anyhow::Result<JoinReply> {
    let mut body = serde_json::json!({ "server": server });
    if let Some(bikes) = bikes {
        body["bikes"] = serde_json::to_value(bikes)?;
    }
    let resp = paintsync::client()?
        .post(format!("{}/v1/paintsync/join", control_plane()))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(JoinReply::NotDeployed);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Ok(JoinReply::RateLimited(retry_after(&resp).unwrap_or(HEARTBEAT)));
    }
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        crate::gate::note_refusal(status.as_u16(), Some(&detail));
        anyhow::bail!("the control plane refused the join ({status}): {detail}");
    }
    Ok(JoinReply::Joined(resp.json().await?))
}

fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Upload exactly the paints the join said were missing.
///
/// Each file is re-hashed as it is read: the look was hashed when the join was built, and a
/// paint rewritten since would be refused by the control plane anyway. Returns how many went up.
pub async fn upload_missing(
    token: &str,
    sources: &HashMap<String, PathBuf>,
    missing: &[String],
) -> usize {
    let Ok(http) = paintsync::client() else { return 0 };
    let mut uploaded = 0usize;
    for sha in missing {
        // Taken from the map built while hashing, so a hash is only ever answered with the
        // file that produced it — never a path chosen by the server.
        let Some(path) = sources.get(sha) else { continue };
        let Ok(bytes) = std::fs::read(path) else { continue };
        if sha256_bytes(&bytes) != *sha {
            log::debug!("[room] {} changed since it was hashed; not uploading", path.display());
            continue;
        }
        let resp = http
            .put(format!("{}/v1/paintsync/paints/{sha}", control_plane()))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes)
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => uploaded += 1,
            Ok(r) if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
                // The next join reports the rest as missing again.
                log::warn!("[room] upload rate limited; the rest waits for the next join");
                break;
            }
            Ok(r) => log::warn!("[room] uploading {sha} failed: {}", r.status()),
            Err(e) => log::warn!("[room] uploading {sha} failed: {e}"),
        }
    }
    uploaded
}

/// Tell the control plane this rider has left `server`.
pub async fn leave(token: &str, server: &str) -> anyhow::Result<()> {
    paintsync::client()?
        .post(format!("{}/v1/paintsync/leave", control_plane()))
        .bearer_auth(token)
        .json(&serde_json::json!({ "server": server }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// One paint's bytes, or `None` when the control plane no longer has it.
///
/// A 404 is ordinary — the store expires a paint a day after upload, and its owner re-uploads
/// it on their next join — so it skips that paint rather than failing the sync.
async fn fetch_paint(http: &reqwest::Client, token: &str, sha: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let resp = http
        .get(format!("{}/v1/paints/{sha}", control_plane()))
        .bearer_auth(token)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp = resp.error_for_status().inspect_err(crate::gate::note_error)?;
    Ok(Some(resp.bytes().await?.to_vec()))
}

// ── The room's socket ────────────────────────────────────────────────────────

/// What the room says.
#[derive(Debug)]
pub enum RoomEvent {
    /// Someone arrived, or changed their look.
    Joined(RoomRider),
    Left { rider_name: String, guid: Option<String> },
    /// The socket is gone; the session decides when to reopen it.
    Closed(String),
}

enum RoomCommand {
    Ping,
    Leave,
}

/// An open room. Blocking, on its own thread, like the voice room: a room is a handful of
/// frames an hour, so an async socket would be all cost.
pub struct Room {
    commands: Sender<RoomCommand>,
    events: Receiver<RoomEvent>,
}

impl Room {
    /// Open the room. Blocks for the handshake, so call it off the async runtime.
    pub fn open(token: &str, key: &str, room_path: &str) -> Result<Room, String> {
        use tungstenite::client::IntoClientRequest;
        let url = room_url(&control_plane(), key, room_path);
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| format!("bad paint room address: {e}"))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {token}").parse().map_err(|_| "bad token".to_string())?,
        );
        let (socket, _) =
            tungstenite::connect(request).map_err(|e| format!("couldn't open the paint room: {e}"))?;

        let (command_tx, command_rx) = std::sync::mpsc::channel::<RoomCommand>();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<RoomEvent>();
        std::thread::Builder::new()
            .name("paint-room".into())
            .spawn(move || pump(socket, command_rx, event_tx))
            .map_err(|e| format!("couldn't start the paint room thread: {e}"))?;
        Ok(Room { commands: command_tx, events: event_rx })
    }

    /// Everything the room has said since the last call. Never blocks.
    pub fn drain(&self) -> Vec<RoomEvent> {
        self.events.try_iter().collect()
    }

    pub fn ping(&self) {
        let _ = self.commands.send(RoomCommand::Ping);
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = self.commands.send(RoomCommand::Leave);
    }
}

/// The socket address for a room.
///
/// The path the join returned is used when it is the path it should be; anything else — an
/// absolute URL, another route — is rebuilt from the key, so a join reply can never point
/// the bearer token at somewhere other than the control plane.
fn room_url(base: &str, key: &str, room_path: &str) -> String {
    let scheme = if base.starts_with("http://") { "ws://" } else { "wss://" };
    let host = base.trim_end_matches('/').trim_start_matches("http://").trim_start_matches("https://");
    let path = if room_path.starts_with("/v1/paintsync/room?")
        && !room_path.contains("//")
        && !room_path.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        room_path.to_string()
    } else {
        let key = percent_encoding::utf8_percent_encode(key, percent_encoding::NON_ALPHANUMERIC);
        format!("/v1/paintsync/room?server={key}")
    };
    format!("{scheme}{host}{path}")
}

/// One frame from the room. Unknown types are ignored, so the control plane can add one.
fn parse_event(text: &str) -> Option<RoomEvent> {
    #[derive(Deserialize)]
    #[serde(tag = "t", rename_all = "lowercase")]
    enum Frame {
        Joined {
            rider: RoomRider,
        },
        Left {
            #[serde(rename = "riderName", default)]
            rider_name: String,
            #[serde(default)]
            guid: Option<String>,
        },
        #[serde(other)]
        Other,
    }
    match serde_json::from_str::<Frame>(text).ok()? {
        Frame::Joined { rider } => Some(RoomEvent::Joined(rider)),
        Frame::Left { rider_name, guid } => Some(RoomEvent::Left { rider_name, guid }),
        Frame::Other => None,
    }
}

fn pump(
    mut socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    commands: Receiver<RoomCommand>,
    events: Sender<RoomEvent>,
) {
    use tungstenite::Message;
    set_nonblocking(socket.get_ref());

    let mut reason = String::from("the paint room closed");
    'outer: loop {
        for command in commands.try_iter() {
            match command {
                RoomCommand::Ping => {
                    let ping = serde_json::json!({ "t": "ping" }).to_string();
                    if socket.send(Message::Text(ping)).is_err() {
                        reason = "lost the connection to the paint room".into();
                        break 'outer;
                    }
                }
                RoomCommand::Leave => break 'outer,
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Some(event) = parse_event(&text) {
                    if events.send(event).is_err() {
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                reason = format!("the paint room connection failed: {e}");
                break;
            }
        }
    }
    let _ = socket.close(None);
    let _ = socket.flush();
    let _ = events.send(RoomEvent::Closed(reason));
}

fn set_nonblocking(stream: &tungstenite::stream::MaybeTlsStream<std::net::TcpStream>) {
    use tungstenite::stream::MaybeTlsStream;
    let tcp = match stream {
        MaybeTlsStream::Plain(tcp) => tcp,
        MaybeTlsStream::Rustls(tls) => tls.get_ref(),
        _ => return,
    };
    let _ = tcp.set_nonblocking(true);
}

// ── Who is here ──────────────────────────────────────────────────────────────

/// The other riders in the room, as the join and the socket have described them.
#[derive(Debug, Default)]
pub struct Grid {
    riders: HashMap<String, RoomRider>,
}

/// What a rider's look amounts to, for telling a real change from a repeat.
fn look_of(rider: &RoomRider) -> Vec<(String, String)> {
    let mut look: Vec<(String, String)> = rider
        .paints
        .iter()
        .map(|p| (p.rel_dest.to_ascii_lowercase(), p.sha256.to_ascii_lowercase()))
        .collect();
    look.sort();
    look
}

impl Grid {
    /// Start over from a join's answer. Returns whether anything differs.
    pub fn replace(&mut self, riders: Vec<RoomRider>) -> bool {
        let mut next: HashMap<String, RoomRider> = HashMap::new();
        for rider in riders {
            next.insert(rider.key(), rider);
        }
        let changed = next.len() != self.riders.len()
            || next.iter().any(|(k, r)| self.riders.get(k).map(look_of) != Some(look_of(r)));
        self.riders = next;
        changed
    }

    /// Someone arrived, or changed their look. Returns whether anything differs.
    pub fn joined(&mut self, rider: RoomRider) -> bool {
        let key = rider.key();
        let changed = self.riders.get(&key).map(look_of) != Some(look_of(&rider));
        self.riders.insert(key, rider);
        changed
    }

    /// Someone left. Returns whether they were here.
    pub fn left(&mut self, rider_name: &str, guid: Option<&str>) -> bool {
        if self.riders.remove(&rider_key(rider_name, guid)).is_some() {
            return true;
        }
        // A rider known by GUID but announced by name alone, which a control plane that
        // lost track of the GUID could send.
        let name = rider_name.trim().to_lowercase();
        let before = self.riders.len();
        self.riders.retain(|_, r| r.rider_name.trim().to_lowercase() != name);
        self.riders.len() != before
    }

    pub fn clear(&mut self) {
        self.riders.clear();
    }

    pub fn riders(&self) -> impl Iterator<Item = &RoomRider> {
        self.riders.values()
    }
}

// ── Which variant a file holds ───────────────────────────────────────────────

/// The variant a destination should hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub rel_dest: String,
    pub sha256: String,
}

/// For every destination anyone wears, the variant to install there.
///
/// The variant worn by the most riders wins, because the game can show one file per name and
/// that is the choice that renders the most of the grid correctly. A tie goes to the variant
/// of whoever joined first: it is the one already on disk for everyone who was there, so a
/// later arrival doesn't flip everyone's file. A rider wearing one paint on two bikes counts
/// once.
pub fn pick_variants<'a>(riders: impl IntoIterator<Item = &'a RoomRider>) -> Vec<Pick> {
    struct Variant {
        rel_dest: String,
        riders: HashSet<String>,
        earliest: u64,
    }
    let mut by_dest: HashMap<String, HashMap<String, Variant>> = HashMap::new();
    for rider in riders {
        let key = rider.key();
        for paint in &rider.paints {
            let sha = paint.sha256.to_ascii_lowercase();
            let variant = by_dest
                .entry(paint.rel_dest.to_ascii_lowercase())
                .or_default()
                .entry(sha)
                .or_insert_with(|| Variant {
                    rel_dest: paint.rel_dest.clone(),
                    riders: HashSet::new(),
                    earliest: u64::MAX,
                });
            variant.riders.insert(key.clone());
            variant.earliest = variant.earliest.min(rider.joined_at);
        }
    }
    let mut picks: Vec<Pick> = by_dest
        .into_values()
        .filter_map(|variants| {
            variants
                .into_iter()
                .max_by(|(sha_a, a), (sha_b, b)| {
                    a.riders
                        .len()
                        .cmp(&b.riders.len())
                        .then(b.earliest.cmp(&a.earliest))
                        // Same count, same instant: any fixed order beats a hash map's.
                        .then(sha_b.cmp(sha_a))
                })
                .map(|(sha, v)| Pick { rel_dest: v.rel_dest, sha256: sha })
        })
        .collect();
    picks.sort_by(|a, b| a.rel_dest.cmp(&b.rel_dest));
    picks
}

/// How many destinations more than one variant wants.
fn contested(riders: &[&RoomRider]) -> usize {
    let mut seen: HashMap<String, HashSet<String>> = HashMap::new();
    for rider in riders {
        for paint in &rider.paints {
            seen.entry(paint.rel_dest.to_ascii_lowercase())
                .or_default()
                .insert(paint.sha256.to_ascii_lowercase());
        }
    }
    seen.values().filter(|v| v.len() > 1).count()
}

// ── What this machine already has ────────────────────────────────────────────

/// A lowercase hex SHA-256 — the only shape of string that becomes a file name in the store.
/// The hashes come from other riders, so anything else is refused rather than joined onto a path.
pub fn valid_sha(sha: &str) -> bool {
    sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Every mods folder the game can load from: the content root, and the install's own `mods`
/// when it is a different folder.
pub fn mods_roots(cfg: &AppConfig) -> Vec<PathBuf> {
    let mut roots = vec![crate::library::mods_root(&cfg.mods_path)];
    let install = cfg.install_dir();
    if !install.trim().is_empty() {
        let game_mods = crate::library::mods_root(&install);
        if game_mods.is_dir() && !roots.iter().any(|r| same_dir(r, &game_mods)) {
            roots.push(game_mods);
        }
    }
    roots
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Indexed {
    size: u64,
    mtime: u64,
    sha256: String,
}

/// The digest of every `.pnt` under the mods roots, cached by path, size and mtime.
///
/// This is what makes the download side a delta: a paint the player already has — bought,
/// downloaded by hand, or installed by an earlier sync — is found here and never fetched.
/// Only a file whose size or mtime moved is hashed again, so after the first walk a refresh
/// is stat calls.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HashIndex {
    files: HashMap<String, Indexed>,
    #[serde(skip)]
    dirty: bool,
    /// Files hashed by this instance, for the tests to tell a cache hit from a miss.
    #[serde(skip)]
    hashed: usize,
}

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some((meta.len(), mtime))
}

impl HashIndex {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self, path: &Path) {
        if !self.dirty {
            return;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string(&self) {
            Ok(text) => match std::fs::write(path, text) {
                Ok(()) => self.dirty = false,
                Err(e) => log::warn!("[room] couldn't write {}: {e}", path.display()),
            },
            Err(e) => log::warn!("[room] couldn't serialize the paint index: {e}"),
        }
    }

    /// The digest of the file at `path`, hashed only if it changed since it was last seen.
    pub fn sha_of(&mut self, path: &Path) -> Option<String> {
        let (size, mtime) = stamp(path)?;
        let key = path.to_string_lossy().into_owned();
        if let Some(hit) = self.files.get(&key) {
            if hit.size == size && hit.mtime == mtime {
                return Some(hit.sha256.clone());
            }
        }
        let sha = paintsync::sha256_file(path).ok()?;
        self.hashed += 1;
        self.files.insert(key, Indexed { size, mtime, sha256: sha.clone() });
        self.dirty = true;
        Some(sha)
    }

    /// Record a file this app just wrote, whose digest it already knows.
    ///
    /// Not left to [`Self::sha_of`]: a same-size paint swapped in within the filesystem's
    /// mtime resolution has the old stamp, and the cache would keep answering the old digest.
    pub fn record(&mut self, path: &Path, sha256: &str) {
        let Some((size, mtime)) = stamp(path) else { return };
        self.files.insert(path.to_string_lossy().into_owned(), Indexed { size, mtime, sha256: sha256.to_string() });
        self.dirty = true;
    }

    /// Walk every root, hashing what is new or changed and forgetting what has gone.
    pub fn refresh(&mut self, roots: &[PathBuf]) {
        let mut seen: HashSet<String> = HashSet::new();
        for root in roots {
            for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter().flatten() {
                if !entry.file_type().is_file() || !paintsync::is_paint(entry.path()) {
                    continue;
                }
                if self.sha_of(entry.path()).is_some() {
                    seen.insert(entry.path().to_string_lossy().into_owned());
                }
            }
        }
        let before = self.files.len();
        self.files.retain(|path, _| seen.contains(path));
        if self.files.len() != before {
            self.dirty = true;
        }
    }

    /// A file on disk holding `sha`, if the index knows one that hasn't changed since.
    pub fn find(&self, sha: &str) -> Option<PathBuf> {
        self.files.iter().find_map(|(path, e)| {
            let path = PathBuf::from(path);
            (e.sha256 == sha && stamp(&path) == Some((e.size, e.mtime))).then_some(path)
        })
    }
}

/// Every paint this machine has received, as `<sha>.pnt`, under the app data dir.
///
/// Outside the game's folders on purpose: nothing in here is loaded by the game, so holding
/// both variants of a contested file costs disk and nothing else. `used.json` records when
/// each hash was last worn by a rider in a room, which is what the week-long cleanup reads.
pub struct PaintStore {
    dir: PathBuf,
    used: HashMap<String, u64>,
    dirty: bool,
}

const USED_NAME: &str = "used.json";

impl PaintStore {
    pub fn open(dir: PathBuf) -> Self {
        let used = std::fs::read_to_string(dir.join(USED_NAME))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        PaintStore { dir, used, dirty: false }
    }

    /// Where `sha` lives in the store. `None` for anything that isn't a hash.
    pub fn path(&self, sha: &str) -> Option<PathBuf> {
        valid_sha(sha).then(|| self.dir.join(format!("{sha}.pnt")))
    }

    pub fn has(&self, sha: &str) -> bool {
        self.path(sha).is_some_and(|p| p.is_file())
    }

    /// Keep `bytes` as `sha`, refusing them unless they hash to it.
    pub fn put(&mut self, sha: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let Some(dest) = self.path(sha) else { anyhow::bail!("{sha:?} is not a paint hash") };
        if sha256_bytes(bytes) != sha {
            anyhow::bail!("digest mismatch for {sha}");
        }
        std::fs::create_dir_all(&self.dir)?;
        // Written aside and renamed, so a crash never leaves a truncated file under a name
        // that claims to be a verified hash.
        let tmp = self.dir.join(format!("{sha}.part"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &dest)?;
        Ok(())
    }

    /// Copy a file already on disk into the store as `sha`, if it really is that hash.
    pub fn adopt(&mut self, sha: &str, from: &Path) -> bool {
        if self.has(sha) {
            return true;
        }
        match std::fs::read(from) {
            Ok(bytes) => self.put(sha, &bytes).is_ok(),
            Err(_) => false,
        }
    }

    pub fn mark_used(&mut self, sha: &str, now: u64) {
        let sha = sha.to_ascii_lowercase();
        if !valid_sha(&sha) {
            return;
        }
        if self.used.get(&sha) != Some(&now) {
            self.used.insert(sha, now);
            self.dirty = true;
        }
    }

    pub fn last_used(&self, sha: &str) -> Option<u64> {
        self.used.get(sha).copied()
    }

    /// Delete stored variants nobody has worn for [`KEEP_FOR_MS`]. Returns how many went.
    ///
    /// A file with no record at all — an older store, or a record lost — starts its week now
    /// rather than being deleted on sight.
    pub fn prune(&mut self, now: u64) -> usize {
        let mut removed = 0usize;
        let on_disk: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().to_str()?.strip_suffix(".pnt").map(str::to_string))
                    .filter(|sha| valid_sha(sha))
                    .collect()
            })
            .unwrap_or_default();
        for sha in on_disk {
            match self.used.get(&sha) {
                None => self.mark_used(&sha, now),
                Some(&at) if now.saturating_sub(at) > KEEP_FOR_MS => {
                    if let Some(path) = self.path(&sha) {
                        if std::fs::remove_file(path).is_ok() {
                            removed += 1;
                        }
                    }
                }
                Some(_) => {}
            }
        }
        let before = self.used.len();
        self.used.retain(|_, at| now.saturating_sub(*at) <= KEEP_FOR_MS);
        if self.used.len() != before {
            self.dirty = true;
        }
        removed
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let Ok(text) = serde_json::to_string(&self.used) else { return };
        match std::fs::write(self.dir.join(USED_NAME), text) {
            Ok(()) => self.dirty = false,
            Err(e) => log::warn!("[room] couldn't record paint use: {e}"),
        }
    }
}

// ── Putting it on disk ───────────────────────────────────────────────────────

/// Make every destination hold its picked variant, from the store or a file already on disk.
///
/// Swapping is this function run again with different picks: the file is ours (the manifest
/// says so and its bytes still match), so it is replaced from the store and nothing is
/// downloaded. The outgoing variant is kept in the store first, so swapping back is free too.
/// A file the player made is never touched, whichever variant would have won.
pub fn install_picks(
    mods_dir: &Path,
    picks: &[Pick],
    store: &mut PaintStore,
    index: &mut HashIndex,
    manifest: &mut Manifest,
) -> PullOutcome {
    let mut out = PullOutcome::default();
    for pick in picks {
        let Some(dest) = safe_dest(mods_dir, &pick.rel_dest) else {
            log::warn!("refusing paint destination {:?}", pick.rel_dest);
            out.rejected += 1;
            continue;
        };
        if dest.is_file() {
            let held = index.sha_of(&dest);
            if held.as_deref() == Some(pick.sha256.as_str()) {
                out.already_had += 1;
                manifest.claim(&pick.rel_dest, &pick.sha256);
                continue;
            }
            if !manifest.owns(&pick.rel_dest, held.as_deref()) {
                out.kept_yours += 1;
                continue;
            }
            if let Some(held) = held {
                store.adopt(&held, &dest);
            }
        }
        // Nothing to install from is a paint the control plane no longer had. Whatever the
        // file holds now stays until a later join brings the bytes.
        let source = store
            .path(&pick.sha256)
            .filter(|p| p.is_file())
            .or_else(|| index.find(&pick.sha256));
        let Some(source) = source else { continue };
        let Ok(bytes) = std::fs::read(&source) else { continue };
        if sha256_bytes(&bytes) != pick.sha256 {
            log::warn!("[room] {} no longer holds {}", source.display(), pick.sha256);
            out.rejected += 1;
            continue;
        }
        if let Some(parent) = dest.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        paintsync::note_sync_write(&dest);
        if let Err(e) = std::fs::write(&dest, &bytes) {
            log::warn!("[room] couldn't write {}: {e}", dest.display());
            continue;
        }
        index.record(&dest, &pick.sha256);
        manifest.claim(&pick.rel_dest, &pick.sha256);
        out.installed += 1;
    }
    out
}

/// Remove the received paints nobody has been seen wearing for [`KEEP_FOR_MS`].
///
/// Only files the manifest says the sync wrote and whose bytes are still the ones it wrote:
/// the player's own files are not in the manifest, and one they edited has stopped matching.
/// Returns how many were removed.
pub fn cleanup_received(
    mods_dir: &Path,
    store: &mut PaintStore,
    index: &mut HashIndex,
    manifest: &mut Manifest,
    now: u64,
) -> usize {
    let mut removed = 0usize;
    let entries: Vec<(String, String)> =
        manifest.installed.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (rel_dest, sha) in entries {
        match store.last_used(&sha) {
            // Installed before anything recorded use — the roster path, or an older store.
            // Its week starts now.
            None => {
                store.mark_used(&sha, now);
                continue;
            }
            Some(at) if now.saturating_sub(at) <= KEEP_FOR_MS => continue,
            Some(_) => {}
        }
        let Some(dest) = safe_dest(mods_dir, &rel_dest) else {
            manifest.forget(&rel_dest);
            continue;
        };
        let Some(dest) = dest
            .is_file()
            .then_some(dest)
            .or_else(|| paintsync::resolve_ignoring_case(mods_dir, &rel_dest))
        else {
            manifest.forget(&rel_dest);
            continue;
        };
        if index.sha_of(&dest).as_deref() != Some(sha.as_str()) {
            // The player has made it theirs since.
            continue;
        }
        paintsync::note_sync_write(&dest);
        if std::fs::remove_file(&dest).is_ok() {
            manifest.forget(&rel_dest);
            paintsync::prune_empty(mods_dir, dest.parent());
            removed += 1;
        }
    }
    removed
}

/// The store and the index, opened once per session.
pub struct Local {
    pub store: PaintStore,
    pub index: HashIndex,
    index_path: PathBuf,
    indexed_at: Option<Instant>,
}

impl Local {
    /// `dir` is the store's folder; the index sits beside the paints in it.
    pub fn open(dir: PathBuf) -> Self {
        let index_path = dir.join("index.json");
        Local {
            index: HashIndex::load(&index_path),
            store: PaintStore::open(dir),
            index_path,
            indexed_at: None,
        }
    }

    fn refresh_index(&mut self, roots: &[PathBuf]) {
        if self.indexed_at.is_some_and(|at| at.elapsed() < INDEX_REFRESH_EVERY) {
            return;
        }
        self.index.refresh(roots);
        self.indexed_at = Some(Instant::now());
    }

    fn save(&mut self) {
        self.store.save();
        self.index.save(&self.index_path);
    }
}

/// The minimum spacing between two paint downloads in one grid pass.
///
/// A new joiner can bring a dozen missing paints at once; fetched back to back they are a
/// burst on the control plane and on the player's own connection while a race might be
/// loading assets of its own. Downloading is never gated on pits or session state — unlike
/// applying, it costs nothing on the game's own thread — but it is paced so it never behaves
/// like a download manager set loose. See [`apply_grid`] for the gated half.
const DOWNLOAD_SPACING: Duration = Duration::from_millis(200);

/// Fetch every paint the grid wears that this machine lacks, into the store — never into the
/// mods folder. Safe to run any time, on any thread, whatever the player is doing in game:
/// nothing written here is where FrostMod looks, so it changes nothing the game can see.
///
/// Every variant worn in the room is fetched, not only the ones picked, so a rider leaving
/// swaps a file without a download later. Paced by [`DOWNLOAD_SPACING`] and done one at a
/// time, lowest priority, so a burst of joiners is a trickle here, not a spike.
pub async fn download_grid(
    cfg: &AppConfig,
    token: &str,
    local: &mut Local,
    riders: &[&RoomRider],
    now: u64,
) -> usize {
    let mods_dir = crate::library::mods_root(&cfg.mods_path);
    let roots = mods_roots(cfg);
    let manifest = Manifest::read(&mods_dir);

    let mut wanted: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for rider in riders {
        for paint in &rider.paints {
            let sha = paint.sha256.to_ascii_lowercase();
            if !valid_sha(&sha) || paint.size > paintsync::MAX_PAINT_BYTES {
                continue;
            }
            // Deduped by hash: an unchanged paint another rider already wore is never
            // re-requested, whatever destination it lands on.
            local.store.mark_used(&sha, now);
            let Some(dest) = safe_dest(&mods_dir, &paint.rel_dest) else { continue };
            if dest.is_file() {
                let held = local.index.sha_of(&dest);
                if held.as_deref() != Some(sha.as_str()) && !manifest.owns(&paint.rel_dest, held.as_deref()) {
                    continue;
                }
            }
            if seen.insert(sha.clone()) {
                wanted.push(sha);
            }
        }
    }

    let lacking = |local: &Local, sha: &String| !local.store.has(sha) && local.index.find(sha).is_none();
    let mut need: Vec<String> = wanted.iter().filter(|s| lacking(local, s)).cloned().collect();
    if !need.is_empty() {
        // The player may have the paint already, somewhere the index hasn't looked yet.
        local.refresh_index(&roots);
        need.retain(|s| lacking(local, s));
    }

    let mut fetched = 0usize;
    if !need.is_empty() {
        if let Ok(http) = paintsync::client() {
            let mut first = true;
            for sha in &need {
                // One at a time, spaced out: a bandwidth cap in everything but name, and it
                // keeps a dozen-paint join from landing as one burst of requests.
                if !first {
                    tokio::time::sleep(DOWNLOAD_SPACING).await;
                }
                first = false;
                match fetch_paint(&http, token, sha).await {
                    Ok(Some(bytes)) => {
                        // Verified inside `put`: unchecked bytes never reach the store.
                        if let Err(e) = local.store.put(sha, &bytes) {
                            log::warn!("[room] {e:#}");
                        } else {
                            fetched += 1;
                        }
                    }
                    Ok(None) => log::debug!("[room] {sha} has expired from the store; skipping"),
                    Err(e) => log::warn!("[room] fetching {sha} failed: {e:#}"),
                }
            }
        }
    }
    local.save();
    fetched
}

/// Put the grid's picked paints on disk where the game will load them, and clean up what
/// nobody wears any more.
///
/// Writing a file costs the game nothing: it reads its paint lists once, at boot, and never
/// rescans them on a join. What makes a staged paint show is a FrostMod refresh, and
/// [`signal_staged`] leaves the moment of that to FrostMod (the join's loading screen or the
/// pits, never while riding). So this runs as soon as the paints are downloaded - before the
/// game boots for an app-launched join, which needs no refresh at all.
pub fn apply_grid(cfg: &AppConfig, local: &mut Local, riders: &[&RoomRider], now: u64) -> PullOutcome {
    let mods_dir = crate::library::mods_root(&cfg.mods_path);
    let mut manifest = Manifest::read(&mods_dir);

    let picks = pick_variants(riders.iter().copied());
    let mut out = install_picks(&mods_dir, &picks, &mut local.store, &mut local.index, &mut manifest);
    out.riders = riders.len();
    out.conflicted = contested(riders);

    let removed = cleanup_received(&mods_dir, &mut local.store, &mut local.index, &mut manifest, now);
    let pruned = local.store.prune(now);
    if removed > 0 || pruned > 0 {
        log::info!("[room] cleaned up {removed} unworn paints and {pruned} stored variants");
    }
    manifest.write(&mods_dir);
    local.save();
    out
}

// ── Telling the game ─────────────────────────────────────────────────────────

/// What to tell the game once paints are on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageSignal {
    /// Nothing: no new file, the game isn't up (it reads them at boot), or the FrostMod
    /// running can't be trusted to wait for the pits.
    Nothing,
    /// `paints_staged`: FrostMod (v0.43.0+) refreshes once on the join's loading screen or
    /// in the pits, never while riding, and repaints only riders missing a paint.
    PaintsStaged,
    /// An older FrostMod, and the rider is in the menus, off any server: its plain refresh
    /// has nobody on track to stall.
    LegacyRefresh,
}

/// The one rule for every paint sync install, whichever path installed it.
///
/// This replaces the live triggers - a refresh per room arrival, a full content reload after
/// every install, and the folder watcher's own refresh on top - which ran on track and made
/// riders lag out.
///
/// `on_server` is `true` when it cannot be read: an older FrostMod is only refreshed when it
/// is known to be safe.
pub fn stage_signal(installed: usize, game_running: bool, frostmod_gates: bool, on_server: bool) -> StageSignal {
    if installed == 0 || !game_running {
        return StageSignal::Nothing;
    }
    if frostmod_gates {
        return StageSignal::PaintsStaged;
    }
    if on_server {
        StageSignal::Nothing
    } else {
        StageSignal::LegacyRefresh
    }
}

/// Act on [`stage_signal`] for `installed` new paint files. At most one signal per install
/// batch; FrostMod coalesces anything closer together than that.
pub fn signal_staged(app: &tauri::AppHandle, installed: usize, on_server: bool) -> StageSignal {
    let tag = crate::frostmod_manage::installed_version(app);
    let signal = stage_signal(
        installed,
        crate::gameproc::is_game_running(),
        crate::frostmod::paints_staged_supported(tag.as_deref()),
        on_server,
    );
    match signal {
        StageSignal::Nothing => {
            if installed > 0 {
                log::info!(
                    "[room] {installed} paints staged; the game lists them when it next starts \
                     (nothing sent: game not up, or FrostMod {tag:?} can't wait for the pits)"
                );
            }
        }
        StageSignal::PaintsStaged => {
            log::info!(
                "[room] {installed} paints staged -> FrostMod ({:?}); it loads them on the loading \
                 screen or in the pits",
                crate::frostmod::signal_paints_staged()
            );
        }
        StageSignal::LegacyRefresh => {
            log::info!(
                "[room] {installed} paints staged in the menus -> refresh {:?}",
                crate::gameproc::refresh_look(tag.as_deref())
            );
        }
    }
    signal
}

// ── A session ────────────────────────────────────────────────────────────────

/// The joined server.
struct Current {
    server: ServerRef,
    /// The control plane's key; empty until a join has been answered.
    key: String,
    room_path: String,
    room: Option<Room>,
    last_beat: Instant,
    /// When a room that dropped, or a join that failed, is next tried.
    retry_at: Instant,
    backoff: Duration,
}

/// Whether the tick found paint rooms to use.
pub enum Tick {
    Live,
    /// The control plane has no paint rooms; the caller falls back to the roster.
    NotDeployed,
}

/// One game session's worth of paint rooms: join where the rider is, follow the room, leave.
#[derive(Default)]
pub struct Session {
    /// The address an app-launched join sent the game to.
    launched_to: Option<String>,
    /// The server name FrostMod reported while the game was at `launched_to`.
    paired_name: Option<String>,
    /// FrostMod has reported a server at least once this session.
    seen_server: bool,
    current: Option<Current>,
    grid: Grid,
    local: Option<Local>,
    /// Reads FrostMod's session block, to know whether the game is on a server. Held open
    /// across ticks: re-opening the mapping every pass is the one thing this reader was
    /// built to avoid.
    session_reader: gamesession::Reader,
}

impl Session {
    pub fn new(launched_to: Option<String>) -> Self {
        // Normalized, so `1.2.3.4` and `1.2.3.4:54210` are one server to the control plane.
        let launched_to =
            launched_to.map(|a| crate::gameproc::parse_server_address(&a).unwrap_or(a));
        Session { launched_to, ..Default::default() }
    }

    /// Whether the game is on a server, `true` when that can't be read (see [`stage_signal`]).
    fn on_server(&self) -> bool {
        self.session_reader.read().map(|s| s.on_a_server()).unwrap_or(true)
    }

    /// The room for where the rider is has answered. Until then the caller polls fast: the
    /// paints should be on disk while the game is still loading into the server.
    pub fn settled(&self) -> bool {
        self.current.as_ref().is_some_and(|c| !c.key.is_empty())
    }

    /// Where the rider is, from what we launched and what FrostMod reports.
    ///
    /// `reported` is the server name when FrostMod says the game is on one. An address only
    /// holds while the game is still on the server it took us to: FrostMod naming a different
    /// server means the rider moved in the game's own browser, and the address is stale.
    /// Once a server has been reported, reporting none means the rider left it.
    pub fn target(&mut self, reported: Option<&str>) -> Option<ServerRef> {
        let reported = reported.map(str::trim).filter(|n| !n.is_empty());
        match reported {
            Some(name) => {
                self.seen_server = true;
                if self.launched_to.is_some() {
                    match &self.paired_name {
                        None => self.paired_name = Some(name.to_string()),
                        Some(paired) if fold(paired) == fold(name) => {}
                        Some(_) => {
                            self.launched_to = None;
                            self.paired_name = None;
                        }
                    }
                }
                Some(ServerRef { address: self.launched_to.clone(), name: Some(name.to_string()) })
            }
            None if self.seen_server => {
                self.launched_to = None;
                self.paired_name = None;
                None
            }
            None => self
                .launched_to
                .clone()
                .map(|address| ServerRef { address: Some(address), name: None }),
        }
    }

    fn local(&mut self, app: &tauri::AppHandle) -> Option<&mut Local> {
        if self.local.is_none() {
            let dir = crate::config::data_dir(app)?.join("paintstore");
            self.local = Some(Local::open(dir));
        }
        self.local.as_mut()
    }

    /// One pass: follow the rider to the right server, then keep the room in step.
    pub async fn tick(
        &mut self,
        app: &tauri::AppHandle,
        cfg: &AppConfig,
        reported: Option<String>,
    ) -> Tick {
        let desired = self.target(reported.as_deref());
        if desired.is_none() && self.current.is_none() {
            // Not on a server and not headed to one: nothing to ask anyone.
            return Tick::Live;
        }
        let token = match crate::voice::signal::account(app, cfg).await {
            Ok(token) => token,
            Err(e) => {
                log::debug!("[room] no account yet: {e}");
                return Tick::Live;
            }
        };

        let same = match (&self.current, &desired) {
            (Some(cur), Some(want)) => cur.server.same_server(want),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.leave_current(&token).await;
            if let Some(server) = desired {
                self.current = Some(Current {
                    server,
                    key: String::new(),
                    room_path: String::new(),
                    room: None,
                    last_beat: Instant::now(),
                    retry_at: Instant::now(),
                    backoff: BACKOFF_MIN,
                });
                return self.join(app, cfg, &token, true).await;
            }
            return Tick::Live;
        }
        let Some(want) = desired else { return Tick::Live };
        if let Some(cur) = self.current.as_mut() {
            // The same server, described better — the name arriving for an address join.
            if cur.server != want && want.name.is_some() {
                cur.server = want;
            }
        }

        // A join that hasn't been answered yet — rate limited, or the network was down.
        if self.current.as_ref().is_some_and(|c| c.key.is_empty()) {
            if self.current.as_ref().is_some_and(|c| Instant::now() >= c.retry_at) {
                return self.join(app, cfg, &token, true).await;
            }
            return Tick::Live;
        }

        // What the room said.
        let events = self.current.as_ref().and_then(|c| c.room.as_ref()).map(Room::drain).unwrap_or_default();
        let mut changed = false;
        let mut closed = None;
        for event in events {
            match event {
                RoomEvent::Joined(rider) => {
                    // A rider who joins after us: their paints are downloaded and staged now,
                    // and FrostMod loads them at our next load point (the pits, or the next
                    // join) - never mid-ride.
                    changed |= self.grid.joined(rider);
                }
                RoomEvent::Left { rider_name, guid } => changed |= self.grid.left(&rider_name, guid.as_deref()),
                RoomEvent::Closed(why) => closed = Some(why),
            }
        }
        if let (Some(why), Some(cur)) = (closed, self.current.as_mut()) {
            log::info!("[room] {why}; reopening in {}s", cur.backoff.as_secs());
            cur.room = None;
            cur.retry_at = Instant::now() + cur.backoff;
            cur.backoff = (cur.backoff * 2).min(HEARTBEAT);
        }
        if changed {
            self.reconcile(app, cfg, &token).await;
        }

        // Reopen a dropped room, with backoff.
        if self.current.as_ref().is_some_and(|c| c.room.is_none() && Instant::now() >= c.retry_at) {
            self.open_room(&token).await;
        }

        // The heartbeat: a ping when the room is up, a fresh join when it isn't. A changed look
        // re-joins now, because the join is what tells the room.
        let look_changed = LOOK_CHANGED.swap(false, Ordering::SeqCst);
        let due = self.current.as_ref().is_some_and(|c| c.last_beat.elapsed() >= HEARTBEAT);
        if look_changed {
            return self.join(app, cfg, &token, true).await;
        }
        if due {
            let pinged = match self.current.as_mut() {
                Some(cur) => match &cur.room {
                    Some(room) => {
                        room.ping();
                        cur.last_beat = Instant::now();
                        true
                    }
                    None => false,
                },
                None => true,
            };
            if !pinged {
                return self.join(app, cfg, &token, false).await;
            }
        }
        Tick::Live
    }

    /// Join the current server, upload what it lacks, and bring the grid in.
    async fn join(&mut self, app: &tauri::AppHandle, cfg: &AppConfig, token: &str, with_look: bool) -> Tick {
        let Some(server) = self.current.as_ref().map(|c| c.server.clone()) else { return Tick::Live };
        if with_look {
            // This join carries the look, so a change noted before it is already covered.
            LOOK_CHANGED.store(false, Ordering::SeqCst);
        }
        let profile = crate::sync_profile(cfg);
        let mut look = match (&profile, with_look) {
            (Some(p), true) => paintsync::local_look(cfg, p).ok(),
            _ => None,
        };
        let bikes = look.as_ref().map(|l| l.bikes.as_slice());
        let reply = join(token, &server, bikes).await;

        let Some(cur) = self.current.as_mut() else { return Tick::Live };
        cur.last_beat = Instant::now();
        let joined = match reply {
            Ok(JoinReply::Joined(joined)) => joined,
            Ok(JoinReply::NotDeployed) => {
                ROOMS.store(2, Ordering::SeqCst);
                self.current = None;
                return Tick::NotDeployed;
            }
            Ok(JoinReply::RateLimited(wait)) => {
                log::warn!("[room] join rate limited; retrying in {}s", wait.as_secs());
                cur.retry_at = Instant::now() + wait;
                return Tick::Live;
            }
            Err(e) => {
                log::warn!("[room] joining failed: {e:#}");
                cur.retry_at = Instant::now() + cur.backoff;
                cur.backoff = (cur.backoff * 2).min(HEARTBEAT);
                return Tick::Live;
            }
        };
        ROOMS.store(1, Ordering::SeqCst);
        if cur.key != joined.server {
            log::info!("[room] on {}", joined.server);
        }
        cur.key = joined.server;
        cur.room_path = joined.room;

        if !joined.missing.is_empty() {
            if look.is_none() {
                look = profile.as_deref().and_then(|p| paintsync::local_look(cfg, p).ok());
            }
            if let Some(look) = &look {
                let uploaded = upload_missing(token, &look.sources, &joined.missing).await;
                log::info!("[room] uploaded {uploaded} of {} missing paints", joined.missing.len());
            }
        }

        let changed = self.grid.replace(joined.riders);
        if changed || self.current.as_ref().is_some_and(|c| c.room.is_none()) {
            self.reconcile(app, cfg, token).await;
        }
        if self.current.as_ref().is_some_and(|c| c.room.is_none()) {
            self.open_room(token).await;
        }
        Tick::Live
    }

    async fn open_room(&mut self, token: &str) {
        let Some(cur) = self.current.as_mut() else { return };
        let (token, key, path) = (token.to_string(), cur.key.clone(), cur.room_path.clone());
        let opened = tauri::async_runtime::spawn_blocking(move || Room::open(&token, &key, &path)).await;
        match opened {
            Ok(Ok(room)) => {
                cur.room = Some(room);
                cur.backoff = BACKOFF_MIN;
            }
            Ok(Err(e)) => {
                log::info!("[room] {e}; retrying in {}s", cur.backoff.as_secs());
                cur.retry_at = Instant::now() + cur.backoff;
                cur.backoff = (cur.backoff * 2).min(HEARTBEAT);
            }
            Err(e) => log::warn!("[room] couldn't open the room: {e}"),
        }
    }

    /// Download the grid's paints, put them on disk, and tell FrostMod they are staged.
    ///
    /// Neither half costs the game a frame: it never rescans paints by itself, so a file on
    /// disk is inert until FrostMod refreshes, and FrostMod decides when that is - once on the
    /// join's loading screen, or in the pits, never while riding ([`signal_staged`]). On an
    /// app-launched join this all happens before the game has booted, and the boot scan
    /// lists the paints with no refresh at all.
    async fn reconcile(&mut self, app: &tauri::AppHandle, cfg: &AppConfig, token: &str) {
        let now = crate::now_ms();
        let grid = std::mem::take(&mut self.grid);
        set_room_riders(Some(grid.riders().map(|r| fold_rider(&r.rider_name)).collect()));
        let riders: Vec<&RoomRider> = grid.riders().collect();

        let Some(local) = self.local(app) else {
            drop(riders);
            self.grid = grid;
            return;
        };
        download_grid(cfg, token, local, &riders, now).await;

        let Some(local) = self.local.as_mut() else {
            drop(riders);
            self.grid = grid;
            return;
        };
        let out = apply_grid(cfg, local, &riders, now);
        drop(riders);
        self.grid = grid;

        log::info!(
            "[room] {} riders, {} paints installed, {} already held, {} kept as yours, {} contested",
            out.riders,
            out.installed,
            out.already_had,
            out.kept_yours,
            out.conflicted
        );
        let mut saved = crate::config::load_or_detect(app).unwrap_or_default();
        saved.sync.pulled_at = now;
        saved.sync.pulled_riders = out.riders;
        saved.sync.kept_yours = out.kept_yours;
        saved.sync.conflicted = out.conflicted;
        if let Err(e) = crate::config::save(app, &saved) {
            log::warn!("[room] couldn't record the sync: {e:#}");
        }
        crate::emit_sync(app, crate::SyncEvent::pulled(&out));
        let on_server = self.on_server();
        signal_staged(app, out.installed, on_server);
    }

    async fn leave_current(&mut self, token: &str) {
        let Some(cur) = self.current.take() else { return };
        set_room_riders(None);
        // Dropping the room closes its socket.
        drop(cur.room);
        self.grid.clear();
        if cur.key.is_empty() {
            return;
        }
        match leave(token, &cur.key).await {
            Ok(()) => log::info!("[room] left {}", cur.key),
            Err(e) => log::debug!("[room] leaving {} failed: {e:#}", cur.key),
        }
    }

    /// Leave whatever server this session is on. For the game closing, or sync turning off.
    pub async fn end(&mut self, app: &tauri::AppHandle, cfg: &AppConfig) {
        if self.current.is_none() {
            return;
        }
        match crate::voice::signal::account(app, cfg).await {
            Ok(token) => self.leave_current(&token).await,
            Err(_) => {
                self.current = None;
                self.grid.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(n: u8) -> String {
        sha256_bytes(&[n])
    }

    fn paint(rel_dest: &str, sha: &str) -> PaintEntry {
        PaintEntry {
            slot: "paint".into(),
            file_name: rel_dest.rsplit('/').next().unwrap().into(),
            sha256: sha.into(),
            size: 1,
            rel_dest: rel_dest.into(),
        }
    }

    fn rider(name: &str, joined_at: u64, paints: Vec<PaintEntry>) -> RoomRider {
        RoomRider { rider_name: name.into(), guid: None, joined_at, paints }
    }

    const RED: &str = "bikes/KTM450/paints/Red.pnt";

    /// No install while riding can make the game refresh: an up-to-date FrostMod is told the
    /// paints are staged and waits for the loading screen or the pits itself; an older one is
    /// told nothing at all while on a server.
    #[test]
    fn staged_paints_never_ask_an_ungated_frostmod_to_refresh_on_a_server() {
        // FrostMod v0.43.0+: always the gated verb, whatever the game is doing.
        assert_eq!(stage_signal(3, true, true, true), StageSignal::PaintsStaged);
        assert_eq!(stage_signal(3, true, true, false), StageSignal::PaintsStaged);
        // Older FrostMod: nothing on a server (or when we can't tell), its refresh in the menus.
        assert_eq!(stage_signal(3, true, false, true), StageSignal::Nothing);
        assert_eq!(stage_signal(3, true, false, false), StageSignal::LegacyRefresh);
        // Nothing new, or no game yet (the boot scan reads them): nothing to say.
        assert_eq!(stage_signal(0, true, true, false), StageSignal::Nothing);
        assert_eq!(stage_signal(5, false, true, false), StageSignal::Nothing);
        assert_eq!(stage_signal(5, false, false, false), StageSignal::Nothing);
    }

    /// The old flow could fire up to three refreshes per sync. Now one install batch is one
    /// signal at most, and a batch that installed nothing is none.
    #[test]
    fn one_install_batch_is_at_most_one_signal() {
        let batches = [0usize, 4, 0, 0, 1];
        let signals = batches
            .iter()
            .filter(|&&n| stage_signal(n, true, true, true) != StageSignal::Nothing)
            .count();
        assert_eq!(signals, 2);
    }

    #[test]
    fn a_fresh_session_polls_fast_until_its_room_answers() {
        // No room yet: the caller polls fast so the paints land while the game loads.
        let session = Session::new(Some("1.2.3.4:54210".into()));
        assert!(!session.settled());
    }

    #[test]
    fn the_variant_most_riders_wear_wins() {
        let (a, b) = (sha(1), sha(2));
        let riders = [
            rider("Ann", 1, vec![paint(RED, &a)]),
            rider("Bob", 2, vec![paint(RED, &b)]),
            rider("Cat", 3, vec![paint(RED, &b)]),
        ];
        assert_eq!(pick_variants(&riders), vec![Pick { rel_dest: RED.into(), sha256: b }]);
    }

    #[test]
    fn a_tie_goes_to_whoever_joined_first() {
        let (a, b) = (sha(1), sha(2));
        let riders = [rider("Late", 20, vec![paint(RED, &b)]), rider("Early", 10, vec![paint(RED, &a)])];
        assert_eq!(pick_variants(&riders)[0].sha256, a);
        // Order in the list is not what decides it.
        let riders = [rider("Early", 10, vec![paint(RED, &a)]), rider("Late", 20, vec![paint(RED, &b)])];
        assert_eq!(pick_variants(&riders)[0].sha256, a);
    }

    #[test]
    fn one_rider_wearing_a_paint_twice_counts_once() {
        let (a, b) = (sha(1), sha(2));
        let riders = [
            // Same dest on two bikes' worth of loadout: still one rider.
            rider("Ann", 5, vec![paint(RED, &a), paint(RED, &a)]),
            rider("Bob", 1, vec![paint(RED, &b)]),
        ];
        assert_eq!(pick_variants(&riders)[0].sha256, b, "1 v 1, and Bob was first");
    }

    #[test]
    fn destinations_match_whatever_case_they_arrive_in() {
        let (a, b) = (sha(1), sha(2));
        let riders = [
            rider("Ann", 1, vec![paint("bikes/ktm450/paints/red.pnt", &a)]),
            rider("Bob", 2, vec![paint(RED, &b)]),
            rider("Cat", 3, vec![paint(RED, &b)]),
        ];
        let picks = pick_variants(&riders);
        assert_eq!(picks.len(), 1, "one file on disk, one pick: {picks:?}");
        assert_eq!(picks[0].sha256, b);
    }

    #[test]
    fn a_rider_is_counted_by_guid_before_name() {
        let a = rider("Frost", 1, vec![]);
        let mut b = rider("Frost", 2, vec![]);
        b.guid = Some("GUID-B".into());
        assert_ne!(a.key(), b.key());
    }

    /// A mods tree and a store in a temp folder.
    fn scratch(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("mxb-room-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mods = root.join("mods");
        std::fs::create_dir_all(mods.join("bikes")).unwrap();
        (mods, root.join("paintstore"))
    }

    fn pick(sha: &str) -> Vec<Pick> {
        vec![Pick { rel_dest: RED.into(), sha256: sha.into() }]
    }

    #[test]
    fn a_grid_change_swaps_the_file_from_the_store() {
        let (mods, store_dir) = scratch("swap");
        let (red_a, red_b) = (b"red, by Ann".to_vec(), b"red, by Bob".to_vec());
        let (a, b) = (sha256_bytes(&red_a), sha256_bytes(&red_b));
        let mut store = PaintStore::open(store_dir);
        store.put(&a, &red_a).unwrap();
        store.put(&b, &red_b).unwrap();
        let mut index = HashIndex::default();
        let mut manifest = Manifest::default();
        let dest = mods.join(RED);

        let out = install_picks(&mods, &pick(&a), &mut store, &mut index, &mut manifest);
        assert_eq!(out.installed, 1);
        assert_eq!(std::fs::read(&dest).unwrap(), red_a);

        // Bob's side takes the majority: the same file flips, from the store.
        let out = install_picks(&mods, &pick(&b), &mut store, &mut index, &mut manifest);
        assert_eq!((out.installed, out.kept_yours), (1, 0));
        assert_eq!(std::fs::read(&dest).unwrap(), red_b);

        // With Ann's variant gone from the store there is nothing to swap back to. That is
        // not an error: the file stays as it is until a join brings the bytes.
        std::fs::remove_file(store.path(&a).unwrap()).unwrap();
        let out = install_picks(&mods, &pick(&a), &mut store, &mut index, &mut manifest);
        assert_eq!((out.installed, out.rejected), (0, 0));
        assert_eq!(std::fs::read(&dest).unwrap(), red_b);
    }

    #[test]
    fn the_outgoing_variant_is_kept_so_swapping_back_needs_no_download() {
        let (mods, store_dir) = scratch("keep");
        let (red_a, red_b) = (b"red, by Ann".to_vec(), b"red, by Bob".to_vec());
        let (a, b) = (sha256_bytes(&red_a), sha256_bytes(&red_b));
        // Ann's paint was installed by the roster path, so it is on disk and in the manifest
        // but was never in the store.
        let dest = safe_dest(&mods, RED).unwrap();
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, &red_a).unwrap();
        let mut manifest = Manifest::default();
        manifest.claim(RED, &a);
        let mut store = PaintStore::open(store_dir);
        store.put(&b, &red_b).unwrap();
        let mut index = HashIndex::default();

        install_picks(&mods, &pick(&b), &mut store, &mut index, &mut manifest);
        assert!(store.has(&a), "the variant swapped out went into the store");
        let out = install_picks(&mods, &pick(&a), &mut store, &mut index, &mut manifest);
        assert_eq!(out.installed, 1);
        assert_eq!(std::fs::read(&dest).unwrap(), red_a);
    }

    #[test]
    fn the_players_own_file_is_never_swapped() {
        let (mods, store_dir) = scratch("yours");
        let theirs = b"someone else's red".to_vec();
        let t = sha256_bytes(&theirs);
        let dest = safe_dest(&mods, RED).unwrap();
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, b"my own red").unwrap();
        let mut store = PaintStore::open(store_dir);
        store.put(&t, &theirs).unwrap();
        let (mut index, mut manifest) = (HashIndex::default(), Manifest::default());

        let out = install_picks(&mods, &pick(&t), &mut store, &mut index, &mut manifest);
        assert_eq!((out.installed, out.kept_yours), (0, 1));
        assert_eq!(std::fs::read(&dest).unwrap(), b"my own red");
    }

    #[test]
    fn a_destination_outside_the_mods_folder_is_refused() {
        let (mods, store_dir) = scratch("escape");
        let mut store = PaintStore::open(store_dir);
        let (mut index, mut manifest) = (HashIndex::default(), Manifest::default());
        let picks = vec![Pick { rel_dest: "../../evil.pnt".into(), sha256: sha(1) }];
        let out = install_picks(&mods, &picks, &mut store, &mut index, &mut manifest);
        assert_eq!(out.rejected, 1);
    }

    #[test]
    fn the_store_takes_only_verified_hashes_as_file_names() {
        let (_, store_dir) = scratch("names");
        let mut store = PaintStore::open(store_dir);
        // A hash comes from another rider; this is the one place it becomes a path.
        for bad in ["../../x", "ABCDEF", &"g".repeat(64), &sha(1).to_uppercase()] {
            assert!(store.path(bad).is_none(), "{bad:?}");
        }
        assert!(store.put(&sha(1), b"not the bytes of sha(1)").is_err());
        assert!(!store.has(&sha(1)));
        assert!(store.put(&sha(1), &[1]).is_ok());
        assert!(store.has(&sha(1)));
    }

    #[test]
    fn the_index_hashes_a_file_once_until_it_changes() {
        let (mods, _) = scratch("index");
        let file = mods.join("bikes/KTM450/paints/Mine.pnt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"mine").unwrap();
        let mut index = HashIndex::default();
        index.refresh(&[mods.clone()]);
        index.refresh(&[mods.clone()]);
        assert_eq!(index.hashed, 1, "the second walk must be stat calls only");
        assert_eq!(index.find(&sha256_bytes(b"mine")), Some(file.clone()));

        std::fs::write(&file, b"repainted, and longer").unwrap();
        index.refresh(&[mods.clone()]);
        assert_eq!(index.hashed, 2);
        assert!(index.find(&sha256_bytes(b"mine")).is_none());

        std::fs::remove_file(&file).unwrap();
        index.refresh(&[mods]);
        assert!(index.files.is_empty(), "a deleted file is forgotten");
    }

    const DAY: u64 = 24 * 60 * 60 * 1000;

    #[test]
    fn a_received_paint_nobody_has_worn_for_a_week_is_removed() {
        let (mods, store_dir) = scratch("cleanup");
        let mut store = PaintStore::open(store_dir);
        let mut index = HashIndex::default();
        let mut manifest = Manifest::default();
        let now = 100 * DAY;

        let stale = b"stale".to_vec();
        let fresh = b"fresh".to_vec();
        let edited = b"as installed".to_vec();
        for (rel, bytes, used) in [
            ("bikes/A/paints/Stale.pnt", &stale, now - 8 * DAY),
            ("bikes/A/paints/Fresh.pnt", &fresh, now - DAY),
            ("bikes/A/paints/Edited.pnt", &edited, now - 8 * DAY),
        ] {
            let dest = safe_dest(&mods, rel).unwrap();
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(&dest, bytes).unwrap();
            manifest.claim(rel, &sha256_bytes(bytes));
            store.mark_used(&sha256_bytes(bytes), used);
        }
        std::fs::write(mods.join("bikes/A/paints/Edited.pnt"), b"repainted by hand").unwrap();
        // The player's own file, never in the manifest.
        std::fs::write(mods.join("bikes/A/paints/Mine.pnt"), b"mine").unwrap();

        let removed = cleanup_received(&mods, &mut store, &mut index, &mut manifest, now);
        assert_eq!(removed, 1);
        assert!(!mods.join("bikes/A/paints/Stale.pnt").exists());
        assert!(mods.join("bikes/A/paints/Fresh.pnt").exists());
        assert!(mods.join("bikes/A/paints/Edited.pnt").exists(), "edited since: theirs now");
        assert!(mods.join("bikes/A/paints/Mine.pnt").exists());
    }

    #[test]
    fn a_stored_variant_unused_for_a_week_is_pruned() {
        let (_, store_dir) = scratch("prune");
        let mut store = PaintStore::open(store_dir);
        let now = 100 * DAY;
        store.put(&sha(1), &[1]).unwrap();
        store.put(&sha(2), &[2]).unwrap();
        store.put(&sha(3), &[3]).unwrap();
        store.mark_used(&sha(1), now - 8 * DAY);
        store.mark_used(&sha(2), now - DAY);
        // sha(3) has no record: its week starts now.
        assert_eq!(store.prune(now), 1);
        assert!(!store.has(&sha(1)));
        assert!(store.has(&sha(2)) && store.has(&sha(3)));
        assert_eq!(store.last_used(&sha(3)), Some(now));
    }

    #[test]
    fn an_address_holds_until_the_game_names_a_different_server() {
        let mut s = Session::new(Some("1.2.3.4:54210".into()));
        // Launched, FrostMod hasn't reported yet: the address is all we have.
        assert_eq!(s.target(None), Some(ServerRef { address: Some("1.2.3.4:54210".into()), name: None }));
        // The game arrives and names it: both are sent.
        let both = s.target(Some("Frost EU")).unwrap();
        assert_eq!(both.address.as_deref(), Some("1.2.3.4:54210"));
        assert!(both.same_server(&ServerRef { address: Some("1.2.3.4:54210".into()), name: None }));
        // The rider picks another server in the game's browser: the address is stale.
        let moved = s.target(Some("Someone Else")).unwrap();
        assert_eq!(moved, ServerRef { address: None, name: Some("Someone Else".into()) });
        assert!(!moved.same_server(&both));
        // And back in the menus means they left.
        assert_eq!(s.target(None), None);
    }

    #[test]
    fn a_name_only_join_follows_the_game() {
        let mut s = Session::new(None);
        assert_eq!(s.target(None), None, "nothing launched, nothing reported");
        let a = s.target(Some("  Frost   EU ")).unwrap();
        assert!(a.same_server(&ServerRef { address: None, name: Some("frost eu".into()) }));
    }

    #[test]
    fn the_room_path_cannot_send_the_token_elsewhere() {
        let base = "https://cp.example";
        assert_eq!(
            room_url(base, "addr:1.2.3.4:54210", "/v1/paintsync/room?server=addr%3A1.2.3.4%3A54210"),
            "wss://cp.example/v1/paintsync/room?server=addr%3A1.2.3.4%3A54210"
        );
        for bad in ["https://evil.example/x", "//evil.example/v1/paintsync/room?x", "/v1/other", ""] {
            assert_eq!(
                room_url(base, "name:frost eu", bad),
                "wss://cp.example/v1/paintsync/room?server=name%3Afrost%20eu",
                "{bad:?}"
            );
        }
        assert!(room_url("http://127.0.0.1:8799", "k", "").starts_with("ws://127.0.0.1:8799/"));
    }

    #[test]
    fn room_frames_parse_and_unknown_ones_are_ignored() {
        let joined = r#"{"t":"joined","rider":{"riderName":"Bob","guid":null,"joinedAt":5,"paints":[]}}"#;
        assert!(matches!(parse_event(joined), Some(RoomEvent::Joined(r)) if r.rider_name == "Bob" && r.joined_at == 5));
        let left = r#"{"t":"left","riderName":"Bob","guid":"G"}"#;
        assert!(matches!(parse_event(left), Some(RoomEvent::Left { guid: Some(g), .. }) if g == "G"));
        assert!(parse_event(r#"{"t":"pong"}"#).is_none());
        assert!(parse_event(r#"{"t":"something-new"}"#).is_none());
        assert!(parse_event("not json").is_none());
    }

    /// Everyone on the grid but me and the room's riders, once each, by folded name.
    #[test]
    fn the_grid_riders_outside_the_room_are_named() {
        let grid = vec!["Frost".to_string(), "CaptiveDuck".into(), "  soggy ".into(), "".into(), "captiveduck".into()];
        let room = vec!["soggy".to_string()];
        assert_eq!(not_sharing(&grid, "frost", &room), vec!["CaptiveDuck".to_string()]);
        assert!(not_sharing(&grid, "Frost", &["captiveduck".into(), "soggy".into()]).is_empty());
    }

    #[test]
    fn the_grid_notices_arrivals_changes_and_departures() {
        let mut grid = Grid::default();
        assert!(grid.joined(rider("Bob", 1, vec![paint(RED, &sha(1))])));
        assert!(!grid.joined(rider("Bob", 1, vec![paint(RED, &sha(1))])), "a repeat is no change");
        assert!(grid.joined(rider("Bob", 1, vec![paint(RED, &sha(2))])), "a new look is");
        assert!(grid.left("bob", None));
        assert!(!grid.left("bob", None));
        assert!(grid.replace(vec![rider("Ann", 1, vec![])]));
        assert!(!grid.replace(vec![rider("Ann", 1, vec![])]));
    }
}

/// End-to-end against a running control plane, `#[ignore]`d like `paintsync::live_sync`:
///
/// ```sh
/// MXB_CONTROL_PLANE=http://127.0.0.1:8799 cargo test paint_room_live -- --ignored --nocapture
/// ```
#[cfg(test)]
mod paint_room_live {
    use super::*;

    async fn sign_up(name: &str) -> String {
        #[derive(Deserialize)]
        struct Claimed {
            token: String,
        }
        let claimed: Claimed = paintsync::client()
            .unwrap()
            .post(format!("{}/v1/account", control_plane()))
            .json(&serde_json::json!({ "riderName": name }))
            .send()
            .await
            .expect("the control plane must be reachable")
            .error_for_status()
            .expect("a self-serve sign-up must be accepted")
            .json()
            .await
            .unwrap();
        claimed.token
    }

    /// Ann joins wearing a paint the control plane has never seen, is told it is missing,
    /// uploads it, and Bob — joining the same server by name — installs it.
    #[tokio::test]
    #[ignore = "needs a running control plane"]
    async fn a_rider_joining_receives_the_paint_of_one_already_there() {
        let ann = sign_up("Ann").await;
        let bob = sign_up("Bob").await;
        let server = ServerRef { address: None, name: Some(format!("Room Test {}", std::process::id())) };

        let root = std::env::temp_dir().join(format!("mxb-room-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("ann.pnt");
        std::fs::create_dir_all(&root).unwrap();
        let bytes = format!("ann's paint {:?}", std::time::SystemTime::now()).into_bytes();
        std::fs::write(&src, &bytes).unwrap();
        let sha = sha256_bytes(&bytes);
        let rel = "bikes/YZ450F/paints/Ann.pnt";
        let look = vec![BikeLoadout {
            bike_id: "YZ450F".into(),
            paints: vec![PaintEntry {
                slot: "paint".into(),
                file_name: "Ann.pnt".into(),
                sha256: sha.clone(),
                size: bytes.len() as u64,
                rel_dest: rel.into(),
            }],
        }];

        let JoinReply::Joined(joined) = join(&ann, &server, Some(&look)).await.unwrap() else {
            panic!("paint rooms must be deployed");
        };
        assert_eq!(joined.missing, vec![sha.clone()], "a new paint is reported missing");
        let sources = HashMap::from([(sha.clone(), src)]);
        assert_eq!(upload_missing(&ann, &sources, &joined.missing).await, 1);

        let JoinReply::Joined(seen) = join(&bob, &server, Some(&[])).await.unwrap() else { unreachable!() };
        assert_eq!(seen.server, joined.server, "both land on one key");
        let riders: Vec<&RoomRider> = seen.riders.iter().collect();
        assert_eq!(riders.len(), 1);

        let cfg = AppConfig { mods_path: root.to_string_lossy().into_owned(), ..Default::default() };
        let mut local = Local::open(root.join("store"));
        let now = crate::now_ms();
        let fetched = download_grid(&cfg, &bob, &mut local, &riders, now).await;
        assert_eq!(fetched, 1, "the missing paint should have been downloaded into the store");
        let out = apply_grid(&cfg, &mut local, &riders, now);
        assert_eq!(out.installed, 1, "{out:?}");
        assert_eq!(std::fs::read(root.join("mods").join(rel)).unwrap(), bytes);

        leave(&ann, &joined.server).await.unwrap();
        leave(&bob, &seen.server).await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
