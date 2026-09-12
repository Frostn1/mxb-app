//! Decoded texture pixels, held here instead of being shipped as text.
//!
//! Every texture the viewer shows used to travel to the webview as a
//! `data:image/png;base64,…` string: a PNG encode per texture (seconds of every core on a
//! bike with a handful of 1024² liveries), a 37% base64 tax on top, and a multi-megabyte
//! string that then lived in the Rust cache, the JSON payload and the JS heap at once.
//!
//! Instead the raw RGBA stays in this store and the frontend gets a short token. It fetches
//! the pixels over binary IPC (`texture_bytes`) and hands them straight to a
//! `THREE.DataTexture`, so nothing is ever encoded or stringified.
//!
//! Ownership: `to_texture` is called once per texture per source load, so a token belongs
//! to exactly one `bike_cache` entry — no refcounting. That entry calls [`release`] when it
//! leaves the cache, which means evicted *or* replaced in place: a bike built twice under
//! one key displaces the first build, and its pixels are freed by nothing else. Paths with no cache of their own (a gear `.pnt` re-decoded on every pick)
//! rely on [`CAP_BYTES`] reaping the oldest blobs.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Ceiling on resident pixels. Comfortably above the working set — a livery at full size is
/// 67 MB a sheet, and the bike and paint caches together hold a few hundred MB of them — so
/// the eviction below only ever reaps blobs whose owner is long gone.
const CAP_BYTES: usize = 1024 * 1024 * 1024;

/// Stand-in for a token that has been evicted: the same grey an untextured part wears, so a
/// stale reference reads as "no texture" rather than throwing in the viewer.
const MISSING_RGBA: [u8; 4] = [0xb7, 0xbc, 0xc4, 0xff];

/// Row-major RGBA8, `width * height * 4` bytes, top-left origin. Dimensions are not kept
/// here — the `PaintTexture` the frontend already holds is their single source of truth.
type Blob = Vec<u8>;

type Inflate = Arc<dyn Fn() -> Option<Blob> + Send + Sync>;

/// A texture's pixels, held — or, see [`put_lazy`], made each time they are fetched.
enum Entry {
    Ready(Arc<Blob>),
    Lazy { inflate: Inflate, bytes: usize },
}

impl Entry {
    fn bytes(&self) -> usize {
        match self {
            Entry::Ready(b) => b.len(),
            Entry::Lazy { bytes, .. } => *bytes,
        }
    }
}

#[derive(Default)]
struct Store {
    blobs: HashMap<String, Entry>,
    /// Insertion order, for the cap below.
    order: VecDeque<String>,
    bytes: usize,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store::default()))
}

fn next_token() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("t{}", N.fetch_add(1, Ordering::Relaxed))
}

/// Take ownership of a texture's pixels and return the token that names them.
pub fn put(rgba: Vec<u8>) -> String {
    insert(Entry::Ready(Arc::new(rgba)))
}

/// A token whose pixels `inflate` makes on every fetch, holding only what it captures —
/// `bytes` of it, for the cap. For a paint the viewer may never show.
pub fn put_lazy(bytes: usize, inflate: impl Fn() -> Option<Blob> + Send + Sync + 'static) -> String {
    insert(Entry::Lazy { inflate: Arc::new(inflate), bytes })
}

fn insert(entry: Entry) -> String {
    let token = next_token();
    let Ok(mut s) = store().lock() else {
        return token; // poisoned — the token resolves to grey, which beats panicking
    };
    s.bytes += entry.bytes();
    s.blobs.insert(token.clone(), entry);
    s.order.push_back(token.clone());

    while s.bytes > CAP_BYTES {
        let Some(oldest) = s.order.pop_front() else {
            break;
        };
        if let Some(b) = s.blobs.remove(&oldest) {
            s.bytes -= b.bytes();
            log::warn!(
                "texture store over {CAP_BYTES} bytes — dropped {oldest}; a viewer still \
                 holding it will show grey"
            );
        }
    }
    token
}

pub fn get(token: &str) -> Option<Arc<Blob>> {
    let inflate = match store().lock().ok()?.blobs.get(token)? {
        Entry::Ready(b) => return Some(b.clone()),
        Entry::Lazy { inflate, .. } => inflate.clone(),
    };
    // Outside the lock, so inflating a 4096² sheet doesn't stall every other fetch.
    inflate().map(Arc::new)
}

/// Whether every one of `tokens` still names pixels — false once the cap has reaped any.
pub fn all_resident(tokens: &[String]) -> bool {
    store().lock().is_ok_and(|s| tokens.iter().all(|t| s.blobs.contains_key(t)))
}

/// Drop the pixels behind `tokens`, called when whatever owned them is evicted.
pub fn release(tokens: &[String]) {
    let Ok(mut guard) = store().lock() else {
        return;
    };
    let s = &mut *guard;
    for t in tokens {
        if let Some(b) = s.blobs.remove(t) {
            s.bytes -= b.bytes();
        }
    }
    let blobs = &s.blobs;
    s.order.retain(|t| blobs.contains_key(t));
}

/// Pixels for `token`, or a single grey pixel if it has been evicted. The frontend treats a
/// buffer that doesn't match the texture's declared size as "no texture" and renders grey.
pub fn bytes_or_missing(token: &str) -> Vec<u8> {
    match get(token) {
        // A lazy sheet's pixels are ours alone — move them rather than copy 67 MB.
        Some(b) => Arc::try_unwrap(b).unwrap_or_else(|b| b.as_ref().clone()),
        None => {
            log::warn!("texture {token} is no longer resident — serving grey");
            MISSING_RGBA.to_vec()
        }
    }
}

/// Resident pixel bytes, for the load-timing log.
pub fn resident_bytes() -> usize {
    store().lock().map(|s| s.bytes).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_pixels() {
        let px = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let token = put(px.clone());
        assert_eq!(*get(&token).expect("token resolves"), px);
        assert_eq!(bytes_or_missing(&token), px);
    }

    #[test]
    fn lazy_tokens_inflate_on_fetch_and_count_what_they_hold() {
        let before = resident_bytes();
        let token = put_lazy(10, || Some(vec![1u8, 2, 3, 4]));
        assert_eq!(resident_bytes(), before + 10);
        assert_eq!(bytes_or_missing(&token), vec![1u8, 2, 3, 4]);
        release(std::slice::from_ref(&token));
        assert_eq!(resident_bytes(), before);
    }

    #[test]
    fn residency_notices_a_reaped_token() {
        let (a, b) = (put(vec![1u8; 4]), put(vec![2u8; 4]));
        assert!(all_resident(&[a.clone(), b.clone()]));
        release(std::slice::from_ref(&b));
        assert!(!all_resident(&[a, b]));
    }

    #[test]
    fn released_tokens_serve_grey() {
        let token = put(vec![9u8; 4]);
        release(std::slice::from_ref(&token));
        assert!(get(&token).is_none());
        assert_eq!(bytes_or_missing(&token), MISSING_RGBA.to_vec());
    }

    #[test]
    fn releasing_keeps_the_byte_count_honest() {
        let before = resident_bytes();
        let token = put(vec![7u8; 4096]);
        assert_eq!(resident_bytes(), before + 4096);
        release(&[token]);
        assert_eq!(resident_bytes(), before);
    }

    #[test]
    fn tokens_are_unique() {
        let a = put(vec![0u8; 4]);
        let b = put(vec![0u8; 4]);
        assert_ne!(a, b, "each put names its own pixels");
    }
}
