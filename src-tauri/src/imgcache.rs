//! A disk cache for remote thumbnails, served to the webview over a custom URI scheme.
//!
//! Both catalogs put remote URLs straight into `<img src>`, so every scroll through a grid
//! re-fetched the same images across the internet. This puts a cache in front of that, for
//! mxb-mods.com and mxbikes-shop.com alike.
//!
//! Why a URI scheme rather than a command returning base64: the shop catalog is ~1300 items.
//! Data URIs would hold tens of megabytes of inflated strings in React state, cost an IPC
//! round-trip per image, bypass the webview's own image cache, and defeat `loading="lazy"`
//! outright — you have to fetch an image to build its `src`, so nothing can be deferred. A
//! scheme URL is a short string the webview resolves only when the image scrolls into view.
//!
//! The frontend never constructs these URLs by hand; see `cachedImage()` in
//! `src/lib/imgcache.ts`, which uses Tauri's `convertFileSrc` because the scheme resolves to
//! a different shape on Windows than on macOS and Linux.

use percent_encoding::percent_decode_str;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager, UriSchemeResponder};

pub const SCHEME: &str = "imgcache";

/// Bumped when the on-disk layout changes, so old entries are dropped rather than misread.
const CACHE_DIR: &str = "img-v1";

/// The stores' hosts. `mxbikes-shop.b-cdn.net` is that store's own Bunny pull zone, named in
/// full rather than as `b-cdn.net` — that suffix is shared by every Bunny customer, and
/// matching it would open the proxy to all of them. A few dozen product descriptions embed
/// images from it.
///
/// MXB Hub serves everything from its own origin: a sweep of 40 listings, thumbnails, srcsets
/// and description markup included, found no host but `shop.mxb-hub.com`. It is listed as the
/// registrable domain anyway, which covers the subdomain it actually uses and any sibling the
/// store adds later.
const SHOP_HOSTS: [&str; 3] = ["mxbikes-shop.com", "mxbikes-shop.b-cdn.net", "mxb-hub.com"];

/// Is `host` one we're willing to fetch from?
///
/// Not paranoia: the scheme handler will fetch whatever URL it's handed, so without this it
/// would be a general-purpose proxy that any injected markup could aim anywhere.
///
/// The catalogs come from the game table rather than a list written out here, because a list
/// written out here is a list that gets forgotten: gpb-mods.com wasn't on it, so every GP
/// Bikes thumbnail was refused and Browse showed a grid of blank cards. A third title's
/// catalog is now covered the moment its `GameProfile` exists.
fn is_allowed(host: &str) -> bool {
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    crate::game::Game::ALL
        .iter()
        .any(|g| under(g.profile().catalog.domain))
        || SHOP_HOSTS.iter().any(|h| under(h))
}

/// Roughly a few thousand thumbnails. Generous, because the whole point is not refetching.
const MAX_CACHE_BYTES: u64 = 256 * 1024 * 1024;
/// How far below the cap a prune goes, so pruning is occasional rather than constant.
const PRUNE_TO: f64 = 0.8;

/// Concurrent origin fetches. A fast scroll through the grid must not open a hundred
/// sockets — the same reasoning as `mods::mxb`'s rating concurrency.
const MAX_CONCURRENT_FETCHES: usize = 8;

/// Ceiling on a single download.
///
/// The whole body is held in memory to be sniffed and downscaled, and being on the allowlist
/// says only where a URL points, never how big what it points at is: an `<img>` on a mod page
/// can name whatever its author uploaded. Uncapped, one such link costs however large that
/// file happens to be — and [`MAX_CONCURRENT_FETCHES`] of them can be in flight at once, so
/// the grid decides how many times over. Generous for a card rendered around 300px wide: the
/// store's own product images, the largest thing normally seen here, are ~0.5 MB.
const MAX_FETCH_BYTES: usize = 32 * 1024 * 1024;

/// How long a failed URL is remembered, so a dead thumbnail isn't refetched on every pass.
const NEGATIVE_TTL: Duration = Duration::from_secs(5 * 60);

/// Sweep for size every this many writes, plus once at startup.
const PRUNE_EVERY: u64 = 200;

// ───────────────────────────────── entry points ─────────────────────────────────

/// The scheme handler. Registered on the Builder in `main.rs`, so every window — including
/// the overlay — can use it.
pub fn handle(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        responder.respond(serve(&app, request.uri().to_string()).await);
    });
}

/// Images handed to the webview this session, and how many bytes that came to. Read by
/// [`crate::memwatch`], which is trying to answer "what was the app doing when it grew?" —
/// a grid being scrolled looks very different here from an app nobody is touching.
static SERVED: AtomicU64 = AtomicU64::new(0);
static SERVED_BYTES: AtomicU64 = AtomicU64::new(0);

/// `(images, bytes)` served since launch.
pub fn traffic() -> (u64, u64) {
    (SERVED.load(Ordering::Relaxed), SERVED_BYTES.load(Ordering::Relaxed))
}

/// Drop superseded cache generations, and bring the current one under its size cap. Spawned
/// at startup so it never sits in front of the first image.
pub fn start_maintenance(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(root) = app.path().app_cache_dir() {
            // Nothing will ever read these again.
            for old in ["img-cache"] {
                let _ = std::fs::remove_dir_all(root.join(old));
            }
        }
        prune(&app);
    });
}

// ───────────────────────────────── serving ─────────────────────────────────

async fn serve(app: &AppHandle, uri: String) -> http::Response<Vec<u8>> {
    let Some(url) = source_url(&uri) else {
        // A 403 rather than a 404: the request was understood and refused.
        return reply(403, "text/plain", b"blocked".to_vec());
    };
    let width = requested_width(&uri);

    match load(app, &url, width).await {
        Some((bytes, mime)) => {
            SERVED.fetch_add(1, Ordering::Relaxed);
            SERVED_BYTES.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            reply(200, mime, bytes)
        }
        // `ModCard`'s existing `onError` fallback fires on this, exactly as it does for a
        // dead remote URL — so a miss degrades to the placeholder icon, not a broken image.
        None => reply(404, "text/plain", b"not cached".to_vec()),
    }
}

fn reply(status: u16, mime: &str, body: Vec<u8>) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header("Content-Type", mime)
        // On Windows the scheme resolves to `http://imgcache.localhost`, a different origin
        // from the app's own. `<img>` doesn't need CORS, but a future `fetch` or canvas read
        // would hit an opaque wall without this.
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(body)
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

/// Recover the origin URL from the scheme request, and refuse anything off the allowlist.
///
/// `convertFileSrc` percent-encodes the whole URL into the path, and the scheme arrives as
/// `imgcache://localhost/<encoded>` (macOS, Linux) or `http://imgcache.localhost/<encoded>`
/// (Windows), so only the path is meaningful here.
fn source_url(uri: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(uri).ok()?;
    let encoded = parsed.path().trim_start_matches('/');
    if encoded.is_empty() {
        return None;
    }
    let decoded = percent_decode_str(encoded).decode_utf8().ok()?;

    let url = reqwest::Url::parse(&decoded).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    is_allowed(&host).then(|| url.to_string())
}

/// The `?w=` the caller asked for, if any.
///
/// `None` means "give me the original", which is what the detail gallery wants. The grid
/// asks for a width because the store has no thumbnail size — it serves 1000-1280px product
/// images for cards rendered around 300px wide, so caching the originals meant roughly
/// half a megabyte per card for pixels nobody sees.
fn requested_width(uri: &str) -> Option<u32> {
    let parsed = reqwest::Url::parse(uri).ok()?;
    let raw = parsed
        .query_pairs()
        .find(|(k, _)| k == "w")
        .map(|(_, v)| v.into_owned())?;
    // Bounded: the width is attacker-reachable in principle, and a giant value would ask
    // the encoder to allocate accordingly.
    raw.parse::<u32>().ok().filter(|w| (16..=4096).contains(w))
}

// ───────────────────────────────── cache ─────────────────────────────────

fn cache_root(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_cache_dir().ok()?.join(CACHE_DIR))
}

/// `<root>/<first two hex chars>/<hash>.img`, so no single directory holds thousands of
/// entries. `DefaultHasher` matches `pkz.rs` and needs no dependency; its instability across
/// Rust versions is harmless here, since a changed hash only misses the cache.
///
/// The width is part of the key, so the grid's downscaled copy and the detail view's
/// original are separate entries rather than one overwriting the other.
fn entry_path(app: &AppHandle, url: &str, width: Option<u32>) -> Option<PathBuf> {
    let hash = format!("{:016x}", key_of(url, width));
    Some(cache_root(app)?.join(&hash[..2]).join(format!("{hash}.img")))
}

fn key_of(url: &str, width: Option<u32>) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    width.hash(&mut hasher);
    hasher.finish()
}

async fn load(app: &AppHandle, url: &str, width: Option<u32>) -> Option<(Vec<u8>, &'static str)> {
    let path = entry_path(app, url, width)?;

    if let Some(hit) = read_entry(&path) {
        return Some(hit);
    }
    if recently_failed(url, width) {
        return None;
    }

    // Single-flight: the first caller for a URL fetches, the rest wait and then read what it
    // wrote. Without this, a grid painting twenty copies of one placeholder would fetch it
    // twenty times.
    let gate = inflight_gate(url, width);
    let _turn = gate.lock().await;
    if let Some(hit) = read_entry(&path) {
        return Some(hit);
    }

    let bytes = {
        let _permit = fetch_semaphore().acquire().await.ok()?;
        fetch(url).await
    };

    let Some(bytes) = bytes else {
        remember_failure(url, width);
        return None;
    };

    // Sniffing decides whether this is cacheable at all, so an origin that answers a dead
    // thumbnail with an HTML error page — or a Cloudflare block page — never gets stored as
    // an "image".
    let Some(mime) = sniff(&bytes) else {
        log::debug!("imgcache: {url} did not answer with a recognisable image");
        remember_failure(url, width);
        return None;
    };

    // Only the resized copy is stored. The original is what we just downloaded and it is far
    // larger than anything the grid renders; keeping both would double the cache for pixels
    // that are never drawn.
    let (bytes, mime) = match width {
        Some(w) => downscale(bytes, mime, w).await,
        None => (bytes, mime),
    };

    write_entry(&path, &bytes);
    maybe_prune(app);
    Some((bytes, mime))
}

/// Shrink an image to fit `width`, off the async runtime.
///
/// Falls back to the original bytes on any failure — an undecodable format, an encoder
/// error, or an image already smaller than asked for. A slightly-too-large thumbnail is a
/// far better outcome than a missing one, so nothing here is allowed to lose the image.
async fn downscale(bytes: Vec<u8>, mime: &'static str, width: u32) -> (Vec<u8>, &'static str) {
    // Animation doesn't survive a resize, and `image` would hand back only the first frame.
    if mime == "image/gif" {
        return (bytes, mime);
    }
    tokio::task::spawn_blocking(move || resize_blocking(&bytes, width).unwrap_or((bytes, mime)))
        .await
        .unwrap_or_else(|e| {
            log::warn!("imgcache: resize task failed: {e}");
            (Vec::new(), mime)
        })
}

fn resize_blocking(bytes: &[u8], width: u32) -> Option<(Vec<u8>, &'static str)> {
    use std::io::Cursor;

    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    let img = reader.decode().ok()?;

    // Never upscale: a store image smaller than the grid asks for is already the right size,
    // and blowing it up would cost bytes to look worse.
    if img.width() <= width {
        return None;
    }
    let scaled = img.thumbnail(width, u32::MAX);

    // Transparency has to survive — a flattened PNG logo would gain a black box — so an
    // image with alpha only ever gets PNG.
    if scaled.color().has_alpha() {
        let mut png = Vec::new();
        scaled
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        return (png.len() < bytes.len()).then_some((png, "image/png"));
    }

    // Otherwise try both and keep whichever is smaller. Photographs compress far better as
    // JPEG, but flat artwork — logos, plain-background product shots, the kind of thing a
    // mod store is full of — is often smaller as PNG, and picking JPEG blindly would make
    // those *bigger* and then get discarded by the check below, leaving the full-size
    // original in place. Trying both is one extra encode, once, off the async runtime.
    let rgb = image::DynamicImage::ImageRgb8(scaled.to_rgb8());
    let mut jpeg = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .ok()?;
    let mut png = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;

    let (out, mime) = if jpeg.len() <= png.len() {
        (jpeg, "image/jpeg")
    } else {
        (png, "image/png")
    };

    // If the "optimised" copy still came out bigger, keep the original.
    (out.len() < bytes.len()).then_some((out, mime))
}

fn read_entry(path: &Path) -> Option<(Vec<u8>, &'static str)> {
    let bytes = std::fs::read(path).ok()?;
    let mime = sniff(&bytes)?;
    touch(path);
    Some((bytes, mime))
}

fn write_entry(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    // Write beside, then rename, so a killed process can't leave a half-image that would
    // then be served — and sniffed — as a truncated file.
    let tmp = path.with_extension("part");
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// mtime doubles as the LRU stamp. Only restamp entries that are already a day old:
/// rewriting the timestamp on every scroll would be pure write amplification.
fn touch(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let stale = meta
        .modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .map(|age| age > Duration::from_secs(24 * 60 * 60))
        .unwrap_or(false);
    if stale {
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
            let _ = file.set_modified(SystemTime::now());
        }
    }
}

// ───────────────────────────────── fetching ─────────────────────────────────

/// Images are fetched with the same session as the catalog they belong to, so a
/// `cf_clearance` earned for a site applies to its images too.
///
/// Which site that is comes from the URL's host, not from the active game: the two catalogs
/// keep separate jars precisely because a clearance is scoped to the host that minted it, so
/// serving a gpb-mods.com thumbnail through mxb-mods.com's session — or, as it did before,
/// through the store's — sends a cookie that can only fail.
fn client_for(url: &str) -> Option<&'static reqwest::Client> {
    static MXB: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    static GPB: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    static SHOP: OnceLock<Option<reqwest::Client>> = OnceLock::new();

    let host = reqwest::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));

    let catalog = |slot: &'static OnceLock<Option<reqwest::Client>>, site| {
        slot.get_or_init(|| crate::mxb_session::client_builder_for(site).build().ok())
            .as_ref()
    };
    if under(crate::mxb_session::MXB_SITE.domain) {
        catalog(&MXB, &crate::mxb_session::MXB_SITE)
    } else if under(crate::mxb_session::GPB_SITE.domain) {
        catalog(&GPB, &crate::mxb_session::GPB_SITE)
    } else {
        SHOP.get_or_init(|| crate::shop_catalog_session::client_builder().build().ok())
            .as_ref()
    }
}

async fn fetch(url: &str) -> Option<Vec<u8>> {
    use futures_util::StreamExt;

    let resp = client_for(url)?
        .get(url)
        .header("accept", "image/avif,image/webp,image/*,*/*;q=0.8")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }

    // Refuse an oversized body before a byte of it is read, when the origin says how big it
    // is...
    if let Some(len) = resp.content_length() {
        if len > MAX_FETCH_BYTES as u64 {
            log::warn!("imgcache: {url} declares {len} bytes, past the cap — not fetching it");
            return None;
        }
    }
    // ...and again as it arrives, because that header is optional and can be wrong. Streaming
    // rather than `bytes()` also stops the body being held twice over: once whole, once copied
    // into a `Vec`.
    let mut body: Vec<u8> = Vec::new();
    let mut chunks = resp.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        if !push_capped(&mut body, &chunk.ok()?) {
            log::warn!("imgcache: {url} went past the cap mid-download — dropped");
            return None;
        }
    }
    Some(body)
}

/// Append `chunk`, unless doing so would take `body` past [`MAX_FETCH_BYTES`].
///
/// Returns whether it was appended. The check is against the total rather than the chunk, so
/// a body arriving in small pieces can't walk past the ceiling one piece at a time.
fn push_capped(body: &mut Vec<u8>, chunk: &[u8]) -> bool {
    if body.len().saturating_add(chunk.len()) > MAX_FETCH_BYTES {
        return false;
    }
    body.extend_from_slice(chunk);
    true
}

fn fetch_semaphore() -> &'static tokio::sync::Semaphore {
    static SEM: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SEM.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_FETCHES))
}

fn inflight_gate(url: &str, width: Option<u32>) -> Arc<tokio::sync::Mutex<()>> {
    static GATES: OnceLock<Mutex<HashMap<u64, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let gates = GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = lock(gates);
    // Bounded, so a long session can't accumulate a gate per image ever seen.
    if gates.len() > 4096 {
        gates.clear();
    }
    gates.entry(key_of(url, width)).or_default().clone()
}

fn negative() -> &'static Mutex<HashMap<u64, SystemTime>> {
    static NEG: OnceLock<Mutex<HashMap<u64, SystemTime>>> = OnceLock::new();
    NEG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn recently_failed(url: &str, width: Option<u32>) -> bool {
    let key = key_of(url, width);
    let mut map = lock(negative());
    match map.get(&key) {
        Some(at) if at.elapsed().map(|e| e < NEGATIVE_TTL).unwrap_or(false) => true,
        Some(_) => {
            map.remove(&key);
            false
        }
        None => false,
    }
}

fn remember_failure(url: &str, width: Option<u32>) {
    let mut map = lock(negative());
    if map.len() > 4096 {
        map.clear();
    }
    map.insert(key_of(url, width), SystemTime::now());
}

/// Poison-tolerant: a panic in one request must not disable every thumbnail afterwards.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────────────── sniffing ─────────────────────────────────

/// The content type, from the bytes rather than the origin's `Content-Type` header.
///
/// Deciding from the bytes is both more robust — misconfigured origins mislabel images all
/// the time — and the check that keeps non-images out of the cache entirely.
///
/// SVG is deliberately absent. It is a scripting-capable format, and the app renders in a
/// webview with no CSP; an SVG "thumbnail" from a remote catalog is not something to serve
/// back to ourselves.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.starts_with(PNG) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    // AVIF and friends: an ISO-BMFF box whose brand says it's an image.
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if brand == b"avif" || brand == b"avis" {
            return Some("image/avif");
        }
    }
    None
}

// ───────────────────────────────── eviction ─────────────────────────────────

fn maybe_prune(app: &AppHandle) {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    if WRITES.fetch_add(1, Ordering::Relaxed) % PRUNE_EVERY != PRUNE_EVERY - 1 {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { prune(&app) });
}

/// Oldest-first by mtime until the cache is back under [`PRUNE_TO`] of its cap.
fn prune(app: &AppHandle) {
    let Some(root) = cache_root(app) else { return };
    if !root.exists() {
        return;
    }

    let mut entries: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(&root)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        total += meta.len();
        entries.push((entry.into_path(), meta.len(), modified));
    }

    if total <= MAX_CACHE_BYTES {
        return;
    }

    entries.sort_by_key(|(_, _, modified)| *modified);
    let target = (MAX_CACHE_BYTES as f64 * PRUNE_TO) as u64;
    let mut removed = 0u64;
    for (path, size, _) in entries {
        if total <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            removed += 1;
        }
    }
    log::info!("imgcache: pruned {removed} entries, now ~{} MB", total / 1_048_576);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(url: &str) -> String {
        // What `convertFileSrc` produces on macOS and Linux.
        format!(
            "imgcache://localhost/{}",
            percent_encoding::utf8_percent_encode(url, percent_encoding::NON_ALPHANUMERIC)
        )
    }

    #[test]
    fn a_body_within_the_cap_is_kept() {
        let mut body = Vec::new();
        assert!(push_capped(&mut body, &[1, 2, 3]));
        assert!(push_capped(&mut body, &[4, 5]));
        assert_eq!(body, [1, 2, 3, 4, 5]);
    }

    /// The bug: the cap has to be on the running total. Checking each chunk on its own lets a
    /// body of any size through as long as it arrives in small enough pieces — which is how
    /// bodies arrive.
    #[test]
    fn small_chunks_cannot_walk_past_the_cap() {
        let mut body = vec![0u8; MAX_FETCH_BYTES - 1];
        assert!(push_capped(&mut body, &[1]), "the last byte that fits");
        assert_eq!(body.len(), MAX_FETCH_BYTES);
        assert!(!push_capped(&mut body, &[1]), "one past the cap");
        assert_eq!(body.len(), MAX_FETCH_BYTES, "a refused chunk leaves nothing behind");
    }

    #[test]
    fn a_single_oversized_chunk_is_refused_whole() {
        let mut body = Vec::new();
        assert!(!push_capped(&mut body, &vec![0u8; MAX_FETCH_BYTES + 1]));
        assert!(body.is_empty());
    }

    #[test]
    fn accepts_the_catalog_origins_and_their_cdns() {
        for url in [
            "https://mxb-mods.com/wp-content/uploads/a.jpg",
            "https://gpb-mods.com/wp-content/uploads/a.jpg",
            "https://mxbikes-shop.com/img/1.png",
            "https://cdn.mxbikes-shop.com/img/1.png",
            // The store's Bunny pull zone — where some product descriptions embed from.
            "https://mxbikes-shop.b-cdn.net/wp-content/uploads/2024/05/a.jpg",
            // MXB Hub serves its product images from its own origin.
            "https://shop.mxb-hub.com/wp-content/uploads/2026/08/a.png",
            "https://mxb-hub.com/wp-content/uploads/2026/08/a.png",
        ] {
            assert_eq!(source_url(&encoded(url)).as_deref(), Some(url), "{url}");
        }
    }

    /// A lookalike host must not ride in on a store's name — the handler fetches whatever it
    /// is handed, and product descriptions are markup written by other people.
    #[test]
    fn a_lookalike_store_host_is_refused() {
        for url in [
            "https://mxb-hub.com.evil.com/x.png",
            "https://evil-mxb-hub.com/x.png",
            "https://mxb-hub.co/x.png",
        ] {
            assert!(source_url(&encoded(url)).is_none(), "{url} must be refused");
        }
    }

    /// Every catalog the app can browse has to be fetchable, or that title's Browse is a
    /// grid of blank cards — which is exactly what shipped for GP Bikes. Driven off the game
    /// table so adding a title can't quietly leave its thumbnails refused.
    #[test]
    fn every_titles_catalog_is_fetchable() {
        for game in crate::game::Game::ALL {
            let url = format!(
                "https://{}/wp-content/uploads/2026/01/thumb.jpg",
                game.profile().catalog.domain,
            );
            assert_eq!(
                source_url(&encoded(&url)).as_deref(),
                Some(url.as_str()),
                "{} browses {} — its images must load",
                game.id(),
                game.profile().catalog.domain,
            );
        }
    }

    /// End-to-end against the live catalog: take a real thumbnail off gpb-mods.com's API,
    /// put it through the same three steps a `<img src="imgcache://…">` does — allowlist,
    /// session-picked client, fetch — and check what comes back is a real image the cache
    /// would serve. The unit tests above prove the allowlist; only this proves a GP Bikes
    /// card actually gets pixels.
    ///
    /// Ignored by default because it hits the network. Run it with
    /// `cargo test gp_bikes_thumbnails_really_load -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "hits live gpb-mods.com"]
    async fn gp_bikes_thumbnails_really_load() {
        let api = "https://gpb-mods.com/wp-json/wp/v2/posts\
                   ?categories=18&per_page=1&_embed=wp:featuredmedia";
        let posts: serde_json::Value = reqwest::get(api).await.unwrap().json().await.unwrap();
        let origin = posts[0]["_embedded"]["wp:featuredmedia"][0]["source_url"]
            .as_str()
            .expect("the catalog gives its posts a featured image")
            .to_string();

        let url = source_url(&encoded(&origin)).expect("a GP thumbnail must not be refused");
        let bytes = fetch(&url).await.expect("and it must actually fetch");
        let mime = sniff(&bytes).expect("what comes back has to be an image");
        assert!(bytes.len() > 1024, "{mime} of only {} bytes", bytes.len());
        println!("{origin}\n  -> {mime}, {} bytes", bytes.len());
    }

    /// The pull zone is allowed by its full name. `b-cdn.net` is Bunny's shared domain, so
    /// letting the suffix through would hand the proxy to every other Bunny customer.
    #[test]
    fn another_bunny_customer_is_not_covered_by_the_pull_zone() {
        for url in [
            "https://someone-else.b-cdn.net/x.png",
            "https://b-cdn.net/x.png",
            "https://evil-mxbikes-shop.b-cdn.net/x.png",
        ] {
            assert!(source_url(&encoded(url)).is_none(), "{url} must be refused");
        }
    }

    #[test]
    fn refuses_anything_that_would_make_it_an_open_proxy() {
        for url in [
            "https://evil.example.com/x.png",
            "http://mxb-mods.com/x.png",
            "file:///etc/passwd",
            // A lookalike must not pass the suffix check.
            "https://notmxb-mods.com/x.png",
            "https://mxb-mods.com.evil.example/x.png",
        ] {
            assert!(source_url(&encoded(url)).is_none(), "{url} must be refused");
        }
    }

    /// Windows resolves the scheme to a different shape; the handler must read both.
    #[test]
    fn reads_the_windows_form_of_the_scheme_url() {
        let url = "https://mxb-mods.com/a.jpg";
        let windows = format!(
            "http://imgcache.localhost/{}",
            percent_encoding::utf8_percent_encode(url, percent_encoding::NON_ALPHANUMERIC)
        );
        assert_eq!(source_url(&windows).as_deref(), Some(url));
    }

    #[test]
    fn an_empty_path_is_refused_rather_than_fetched() {
        assert!(source_url("imgcache://localhost/").is_none());
        assert!(source_url("imgcache://localhost").is_none());
    }

    #[test]
    fn sniffs_the_raster_formats_a_catalog_serves() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]), Some("image/png"));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff(b"GIF89a....."), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"\0\0\0\x20ftypavif"), Some("image/avif"));
    }

    #[test]
    fn refuses_to_cache_things_that_are_not_images() {
        assert_eq!(sniff(b"<!DOCTYPE html><html>"), None, "an HTML error page");
        assert_eq!(
            sniff(b"<html><title>Attention Required! | Cloudflare</title>"),
            None,
            "a Cloudflare block page must never be stored as a thumbnail"
        );
        assert_eq!(sniff(br#"<svg xmlns="http://www.w3.org/2000/svg">"#), None, "SVG can script");
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><svg>"), None);
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"RIFF"), None, "truncated, not a webp");
    }

    #[test]
    fn the_negative_cache_remembers_then_forgets() {
        let url = "https://mxb-mods.com/gone.jpg";
        assert!(!recently_failed(url, None));
        remember_failure(url, None);
        assert!(recently_failed(url, None));

        // A different width is a different entry — one dead size must not blacklist another.
        assert!(!recently_failed(url, Some(600)));

        // Backdate past the TTL and it should be retried rather than refused forever.
        lock(negative()).insert(
            key_of(url, None),
            SystemTime::now() - NEGATIVE_TTL - Duration::from_secs(1),
        );
        assert!(!recently_failed(url, None));
    }

    #[test]
    fn the_requested_width_is_read_and_bounded() {
        let base = "imgcache://localhost/https%3A%2F%2Fmxb-mods.com%2Fa.jpg";
        assert_eq!(requested_width(base), None, "no ?w= means the original");
        assert_eq!(requested_width(&format!("{base}?w=600")), Some(600));
        assert_eq!(requested_width(&format!("{base}?w=0")), None, "too small");
        assert_eq!(requested_width(&format!("{base}?w=99999")), None, "too large");
        assert_eq!(requested_width(&format!("{base}?w=abc")), None);
    }

    #[test]
    fn a_width_makes_a_distinct_cache_entry() {
        let url = "https://mxb-mods.com/a.jpg";
        assert_ne!(key_of(url, None), key_of(url, Some(600)));
        assert_eq!(key_of(url, Some(600)), key_of(url, Some(600)));
    }

    /// The whole point: a 1200px catalog image must come back much smaller for a card.
    #[test]
    fn downscaling_a_large_image_shrinks_it() {
        use std::io::Cursor;
        let big = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(1200, 1200, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        }));
        let mut original = Vec::new();
        big.write_to(&mut Cursor::new(&mut original), image::ImageFormat::Png)
            .unwrap();

        let (out, _mime) = resize_blocking(&original, 600).expect("should resize");
        assert!(out.len() < original.len(), "resized should be smaller");

        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 600);
    }

    /// A photographic image should land on JPEG, which is where the big saving comes from.
    #[test]
    fn a_photographic_image_encodes_as_jpeg() {
        use std::io::Cursor;
        // A smooth gradient, i.e. the sort of thing JPEG is built for — unlike the
        // sharp-edged synthetic pattern above, which PNG wins.
        let photo = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(1200, 1200, |x, y| {
            image::Rgb([
                (x * 255 / 1200) as u8,
                (y * 255 / 1200) as u8,
                ((x + y) * 255 / 2400) as u8,
            ])
        }));
        let mut original = Vec::new();
        photo
            .write_to(&mut Cursor::new(&mut original), image::ImageFormat::Png)
            .unwrap();

        let (out, mime) = resize_blocking(&original, 600).expect("should resize");
        assert_eq!(mime, "image/jpeg");
        assert!(out.len() < original.len());
    }

    /// Flat artwork is often smaller as PNG. Encoding it blindly as JPEG would make it
    /// *bigger*, which the size guard would then reject — leaving the full-size original
    /// in place and defeating the whole point.
    #[test]
    fn flat_artwork_is_kept_as_png_when_that_is_smaller() {
        use std::io::Cursor;
        let flat = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(1200, 1200, |x, _| {
            if x < 600 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        }));
        let mut original = Vec::new();
        flat.write_to(&mut Cursor::new(&mut original), image::ImageFormat::Png)
            .unwrap();

        let (out, mime) = resize_blocking(&original, 600).expect("should resize");
        assert_eq!(mime, "image/png", "two flat blocks compress far better as PNG");
        assert!(out.len() < original.len());
    }

    #[test]
    fn transparency_survives_the_resize() {
        use std::io::Cursor;
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(900, 900, |x, _| {
            image::Rgba([255, 0, 0, (x % 256) as u8])
        }));
        let mut original = Vec::new();
        img.write_to(&mut Cursor::new(&mut original), image::ImageFormat::Png)
            .unwrap();

        if let Some((out, mime)) = resize_blocking(&original, 300) {
            assert_eq!(mime, "image/png", "flattening would put a black box behind a logo");
            assert!(image::load_from_memory(&out).unwrap().color().has_alpha());
        }
    }

    #[test]
    fn an_image_already_small_enough_is_left_alone() {
        use std::io::Cursor;
        let small = image::DynamicImage::ImageRgb8(image::RgbImage::new(200, 200));
        let mut bytes = Vec::new();
        small
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        assert!(
            resize_blocking(&bytes, 600).is_none(),
            "upscaling costs bytes to look worse"
        );
    }

    #[test]
    fn undecodable_bytes_fall_back_rather_than_losing_the_image() {
        assert!(resize_blocking(b"not an image at all", 600).is_none());
    }
}
