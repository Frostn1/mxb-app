//! Knowing, before the user asks, whether a server's track can be downloaded.
//!
//! The server browser gets a track as an internal id - `Farm14`, `Highland-Mx`,
//! `2026_ARLMX_RD11_INDIANA_Pro` - and needs to say one of three things about it: installed,
//! downloadable from here, or not something we can get. The first is a disk scan. This module
//! is the second.
//!
//! ## Why an index, and why two halves
//!
//! Answering per-track over the network ([`crate::mods::mxb::search`] then a page fetch) is
//! what the app did before, and it can only run *after* the user clicks - so the browser can't
//! badge anything ahead of time, and every click pays a search. An index inverts that, but the
//! catalog only gives up half of what matching needs cheaply:
//!
//! * **Titles are cheap.** The tracks category is ~1,600 posts, and WP's 100-per-page ceiling
//!   turns the whole thing into 16 small requests. So every track's title and slug is known up
//!   front, refreshed daily. Exact title/slug matching resolves about a third of live servers.
//!
//! * **File names are not.** A mod's download links live only in its *rendered page*, one
//!   fetch each - and even then only MediaFire puts the real name in the URL; Drive and Mega
//!   use opaque ids. Roughly half of catalog links name their file.
//!
//! File names are worth the fetches because they catch exactly what titles cannot: a post
//! titled "Farm14 v0.1" ships `Farm14.pkz`, "Five-Two SMX" ships `SMX.pkz`, and "Rail MX"
//! ships `Motoland_MX_Park.pkz`. A version suffix or a rename defeats title matching outright,
//! and the file name is the author's own statement of what the track is called on disk - which
//! is precisely the id a server reports.
//!
//! ## The drip
//!
//! So enrichment is deliberate rather than exhaustive. Scraping all 1,600 pages on first run
//! would be ~1,600 requests per user against a Cloudflare-fronted site, to learn about tracks
//! nobody is hosting. Instead [`resolve`] queues the track ids live servers are actually
//! running that titles didn't resolve, and the drip behind it works the busiest first and stops at
//! [`MAX_PAGES_PER_SESSION`]. Results are permanent - a file name is a fact about a published
//! mod, not a cache of a changing value - so the index converges over a few sessions and
//! costs nothing thereafter.

use super::ModSummary;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// mxb-mods' "Tracks" category - the `tracks` entry of `MOD_TYPES` in `src/api/mods.ts`.
const TRACKS_CATEGORY: u32 = 22;

/// How long the title listing is served before a refresh is spawned behind it. Daily: new
/// tracks are published a few times a week, and a server running one the same day is the only
/// case a shorter window would help.
const LISTING_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A ceiling on the bulk crawl, so a category that grows (or a paging bug) can't spin. The
/// tracks category needs 16.
const MAX_LISTING_PAGES: u32 = 40;

/// Page fetches the drip will make in one app session, **across every round**.
///
/// A per-run cap is not enough, and the first live run proved it: finishing a round fires
/// `track-index://updated`, the UI re-resolves, the tracks that are still unmatched suggest
/// candidates that the round just made visible by reading their neighbours, and another run
/// starts. Five rounds of 49/12/10/9/1 spent 81 pages against a cap that read "60". The
/// budget has to span the cascade, so it lives in [`pages_read`] rather than in a loop bound.
///
/// What the cap doesn't spend this session it spends the next, because what the drip learns
/// is kept - so a low ceiling costs convergence time, never coverage.
const SESSION_PAGE_BUDGET: usize = 60;

/// Pages in flight at once. Low on purpose - this is background work nobody is waiting on,
/// and it shares a Cloudflare budget with the browsing the user *is* waiting on.
const ENRICH_CONCURRENCY: usize = 3;

/// Candidate posts considered per unresolved track. Beyond three the scorer is guessing, and
/// each extra candidate is a page fetch spent on a track we probably can't match anyway.
const CANDIDATES_PER_TRACK: usize = 3;

/// Bumped when [`Index`]'s shape changes, so an old file is dropped rather than misread.
const CACHE_DIR: &str = "track-index-v1";

// ───────────────────────────────── the index ─────────────────────────────────

/// One catalog track. `files` and `enriched_at` are filled in by the drip, not the listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub slug: String,
    pub title: String,
    pub link: String,
    pub date: String,
    /// File name stems the mod's downloads carry, e.g. `["Farm14"]`. Empty after enrichment
    /// means the page was read and named nothing - a Drive-only mod - which is different from
    /// never having been read; `enriched_at` is what tells those apart.
    #[serde(default)]
    pub files: Vec<String>,
    /// Unix ms of the page read, or `None` if the page has never been read.
    #[serde(default)]
    pub enriched_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub entries: Vec<Entry>,
    /// Unix ms of the listing crawl.
    pub fetched_at: i64,
}

impl Index {
    fn stale(&self) -> bool {
        let age = (now_ms() - self.fetched_at).max(0) as u64;
        Duration::from_millis(age) >= LISTING_TTL
    }
}

/// How a track id was matched to a catalog post. The UI shows both the same way; this is for
/// the log and for tests, where "matched by title" and "matched by file name" are the two
/// behaviours worth telling apart.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    /// The mod ships a file with this exact name.
    File,
    /// The post's title or slug is this exact name.
    Title,
}

/// A track the catalog has, as the browser needs to show it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub slug: String,
    pub title: String,
    pub link: String,
    pub via: Via,
}

/// What [`resolve`] found, plus whether the index is still filling itself in - the browser
/// uses `pending` to say "still looking" rather than "not available" for a track the drip
/// hasn't reached yet.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    /// Track id (exactly as asked for) → the catalog post that has it.
    pub found: HashMap<String, Hit>,
    /// Track ids the drip is still working through.
    pub pending: Vec<String>,
    /// Unix ms of the listing crawl, or `None` when the index has never loaded.
    pub fetched_at: Option<i64>,
}

// ───────────────────────────────── matching ─────────────────────────────────

/// Fold to the same case/separator-insensitive key `src/lib/trackContent.ts` folds to, so a
/// track matched here and a track matched against the disk agree about what "the same name"
/// means. `ZD - Prebe Backyard`, `zd_prebe_backyard` and `ZDPrebeBackyard` are one key.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The lookup tables, rebuilt whenever the index changes.
///
/// File names come first and win ties. A post that *ships* `Farm14.pkz` is a stronger claim
/// on the id `Farm14` than a post merely *titled* something that folds to it, and where the
/// two disagree the file is the one the server is actually running.
struct Lookup {
    by_file: HashMap<String, usize>,
    by_title: HashMap<String, usize>,
}

fn build_lookup(entries: &[Entry]) -> Lookup {
    let mut by_file = HashMap::new();
    let mut by_title = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        for f in &e.files {
            let k = norm(f);
            if !k.is_empty() {
                by_file.entry(k).or_insert(i);
            }
        }
        for k in [norm(&e.title), norm(&e.slug)] {
            if !k.is_empty() {
                by_title.entry(k).or_insert(i);
            }
        }
    }
    Lookup { by_file, by_title }
}

fn find(lookup: &Lookup, entries: &[Entry], track: &str) -> Option<Hit> {
    let key = norm(track);
    if key.is_empty() {
        return None;
    }
    let (i, via) = lookup
        .by_file
        .get(&key)
        .map(|i| (*i, Via::File))
        .or_else(|| lookup.by_title.get(&key).map(|i| (*i, Via::Title)))?;
    let e = entries.get(i)?;
    Some(Hit {
        slug: e.slug.clone(),
        title: e.title.clone(),
        link: e.link.clone(),
        via,
    })
}

/// The shortest title allowed to claim a track id it is merely *contained in*, and the
/// largest multiple of that title's length the id may be.
///
/// These guard one arm of [`candidates`] only - the "post title appears inside the track id"
/// case - and they exist because that arm is what made rounds 2-5 of the first live run spend
/// 32 pages to resolve 3 tracks. A league id like `2026arlmxrd11indianapro` contains
/// `indiana` and `pro`, so every generic short title in the catalog queued itself as a
/// candidate for tracks that were never going to match. Requiring the title to be both
/// substantial in itself and a real fraction of the id keeps the arm's actual wins -
/// `outpost` inside `outpostprepped`, `islandsx` inside `ghptracksislandsx` - and drops the
/// rest.
const MIN_CONTAINED_TITLE: usize = 5;
const MAX_CONTAINED_RATIO: usize = 3;

/// Posts worth reading a page for, when looking for `track`.
///
/// Purely local - it decides which pages to spend fetches on, never what matches. The rule is
/// containment either way round on the folded key, which is what the real misses look like:
/// `Farm14` inside `farm14v01` ("Farm14 v0.1"), `smx` inside `fivetwosmx` ("Five-Two SMX").
/// Shorter titles rank first, since a title that is barely longer than the id is far likelier
/// to be the same track than one that merely contains it somewhere.
fn candidates(entries: &[Entry], track: &str) -> Vec<usize> {
    let key = norm(track);
    // Two characters would match half the catalog and buy nothing.
    if key.len() < 3 {
        return Vec::new();
    }
    let mut hits: Vec<(usize, usize)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.enriched_at.is_none())
        .filter_map(|(i, e)| {
            let t = norm(&e.title);
            let s = norm(&e.slug);
            // The two productive arms: the post names the track with something appended (a
            // version, an author prefix), in its title or its slug.
            let widened = t.contains(&key) || s.contains(&key);
            // The narrow arm: the track id is the post's title plus a suffix the author
            // didn't publish under. Guarded - see the two constants above.
            let contained = key.contains(&t)
                && t.len() >= MIN_CONTAINED_TITLE
                && t.len() * MAX_CONTAINED_RATIO >= key.len();
            (widened || contained).then_some((i, t.len().abs_diff(key.len())))
        })
        .collect();
    hits.sort_by_key(|(_, d)| *d);
    hits.truncate(CANDIDATES_PER_TRACK);
    hits.into_iter().map(|(i, _)| i).collect()
}

// ───────────────────────────────── state ─────────────────────────────────

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn state() -> &'static Mutex<Option<Arc<Index>>> {
    static S: OnceLock<Mutex<Option<Arc<Index>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

/// Track ids the drip is working on right now, so the UI can say "still looking" and so a
/// second call doesn't queue the same work twice.
fn in_flight() -> &'static Mutex<HashSet<String>> {
    static F: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Track ids the drip has already been through this session and could not place.
///
/// The terminal state the index was missing. Without it "we haven't looked yet" and "we looked
/// and there is nothing" are the same answer, so the browser can only ever say "Looking…" -
/// and the queue re-reads the same dead ends every time the list refreshes.
///
/// Session-scoped on purpose: a restart is cheap and the catalog does gain posts. It is not
/// written to the cache, so tomorrow's run asks again.
fn settled() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Pages the drip has read this session, against [`SESSION_PAGE_BUDGET`].
fn pages_read() -> &'static std::sync::atomic::AtomicUsize {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    &N
}

/// How many pages a run may still read. Pure, so the accounting is testable without a
/// network or a Tauri handle.
fn remaining_budget(read: usize) -> usize {
    SESSION_PAGE_BUDGET.saturating_sub(read)
}

fn listing_lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn enrich_lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn current() -> Option<Arc<Index>> {
    lock(state()).clone()
}

fn install(index: Index) -> Arc<Index> {
    let arc = Arc::new(index);
    *lock(state()) = Some(arc.clone());
    arc
}

// ───────────────────────────────── disk ─────────────────────────────────

fn cache_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    Some(app.path().app_cache_dir().ok()?.join(CACHE_DIR))
}

fn read_cache(app: &tauri::AppHandle) -> Option<Index> {
    let text = std::fs::read_to_string(cache_dir(app)?.join("index.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(app: &tauri::AppHandle, index: &Index) {
    let Some(dir) = cache_dir(app) else { return };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("could not create the track-index cache dir: {e}");
        return;
    }
    match serde_json::to_vec(index) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(dir.join("index.json"), bytes) {
                log::warn!("could not cache the track index: {e}");
            }
        }
        Err(e) => log::warn!("could not encode the track index: {e}"),
    }
}

// ───────────────────────────────── the listing crawl ─────────────────────────────────

/// Page the whole tracks category, keeping what enrichment has already learned.
///
/// A refresh must not undo the drip: entries are matched by post id, so a track whose page was
/// read months ago keeps its file names through every listing refresh and is never fetched
/// again. Posts that leave the catalog fall out with the listing they came from.
async fn crawl_listing(app: &tauri::AppHandle) -> anyhow::Result<Arc<Index>> {
    let _guard = listing_lock().lock().await;
    // Another caller may have finished the crawl while we waited for the lock.
    if let Some(idx) = current() {
        if !idx.stale() {
            return Ok(idx);
        }
    }

    let known: HashMap<u64, (Vec<String>, Option<i64>)> = current()
        .map(|idx| {
            idx.entries
                .iter()
                .filter(|e| e.enriched_at.is_some())
                .map(|e| (e.id, (e.files.clone(), e.enriched_at)))
                .collect()
        })
        .unwrap_or_default();

    let mut entries: Vec<Entry> = Vec::new();
    for page in 1..=MAX_LISTING_PAGES {
        let posts = super::mxb::catalog_page(TRACKS_CATEGORY, page).await?;
        if posts.is_empty() {
            break;
        }
        let short = posts.len() < 100;
        entries.extend(posts.into_iter().map(|p| entry_from(p, &known)));
        if short {
            break;
        }
    }
    if entries.is_empty() {
        anyhow::bail!("the tracks catalog came back empty");
    }
    log::info!("track index: {} tracks listed", entries.len());

    let index = Index { entries, fetched_at: now_ms() };
    write_cache(app, &index);
    Ok(install(index))
}

fn entry_from(p: ModSummary, known: &HashMap<u64, (Vec<String>, Option<i64>)>) -> Entry {
    let (files, enriched_at) = known.get(&p.id).cloned().unwrap_or_default();
    Entry {
        id: p.id,
        slug: p.slug,
        title: p.title,
        link: p.link,
        date: p.date,
        files,
        enriched_at,
    }
}

/// The index to answer from, loading disk on first use and crawling only when there is
/// nothing at all. A stale index is served as-is with a refresh spawned behind it.
async fn ensure_loaded(app: &tauri::AppHandle) -> anyhow::Result<Arc<Index>> {
    if let Some(idx) = current() {
        if idx.stale() {
            spawn_listing_refresh(app);
        }
        return Ok(idx);
    }
    if let Some(cached) = read_cache(app) {
        if !cached.entries.is_empty() {
            let arc = install(cached);
            if arc.stale() {
                spawn_listing_refresh(app);
            }
            return Ok(arc);
        }
    }
    crawl_listing(app).await
}

fn spawn_listing_refresh(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crawl_listing(&app).await {
            log::warn!("track index: listing refresh failed: {e:#}");
        }
    });
}

// ───────────────────────────────── the drip ─────────────────────────────────

/// Read one mod page and record what it ships. Failures are recorded too - as an enrichment
/// with no files - so a page that 404s or is Cloudflare-blocked isn't retried on every open.
async fn enrich_one(entry: &Entry) -> (u64, Vec<String>) {
    let files = match super::mxb::downloads_at(&entry.link).await {
        Ok(downloads) => downloads
            .iter()
            .filter_map(|d| super::mxb::download_file_name(&d.url))
            .collect(),
        Err(e) => {
            log::info!("track index: couldn't read {} ({e:#})", entry.slug);
            Vec::new()
        }
    };
    (entry.id, files)
}

/// Work through `wanted` - track ids the index couldn't resolve - reading pages for the
/// candidates each one suggests, busiest track first.
///
/// `wanted` arrives already ordered by how many servers are running each track, so the drip
/// spends its budget where it will change what the most people see.
async fn drip(app: tauri::AppHandle, wanted: Vec<String>) {
    // Whatever [`round`] does - finish, give up early, or panic - these ids come back and are
    // marked settled (see [`Claim`]), and the browser is told to ask again. A round that
    // learned nothing still has to say so: "Looking…" is a state the UI cannot leave on its
    // own, because the only thing that moves it is this event.
    let claim = Claim(wanted);
    round(&app, &claim.0).await;
    drop(claim);

    use tauri::Emitter;
    let _ = app.emit("track-index://updated", ());
}

/// One pass of the drip. Every early return here is a round that learned nothing, which
/// [`drip`] is responsible for reporting.
async fn round(app: &tauri::AppHandle, wanted: &[String]) {
    let app = app.clone();
    let _guard = enrich_lock().lock().await;
    let Ok(index) = ensure_loaded(&app).await else { return };

    use std::sync::atomic::Ordering;
    let budget = remaining_budget(pages_read().load(Ordering::Relaxed));
    if budget == 0 {
        log::info!(
            "track index: session page budget ({SESSION_PAGE_BUDGET}) spent;              {} tracks left for the next session",
            wanted.len()
        );
        return;
    }

    // Pick the pages to read, dropping duplicates: two tracks often point at the same post.
    let mut targets: Vec<Entry> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    'pick: for track in wanted {
        for i in candidates(&index.entries, track) {
            let e = &index.entries[i];
            if seen.insert(e.id) {
                targets.push(e.clone());
            }
            if targets.len() >= budget {
                break 'pick;
            }
        }
    }
    if targets.is_empty() {
        return;
    }
    // Claimed before the fetches, not after: another round can start the moment this one
    // emits its event, and a budget that only counted finished work would let them overlap
    // past the ceiling.
    pages_read().fetch_add(targets.len(), Ordering::Relaxed);
    log::info!(
        "track index: reading {} pages for {} unresolved tracks ({} of {} budget spent)",
        targets.len(),
        wanted.len(),
        pages_read().load(Ordering::Relaxed),
        SESSION_PAGE_BUDGET
    );

    let mut learned: HashMap<u64, Vec<String>> = HashMap::new();
    for chunk in targets.chunks(ENRICH_CONCURRENCY) {
        let results = futures_util::future::join_all(chunk.iter().map(enrich_one)).await;
        for (id, files) in results {
            learned.insert(id, files);
        }
    }

    // Fold the results into the index as it stands *now* - a listing refresh may have landed
    // while we were fetching, and rebuilding from the copy we started with would lose it.
    let base = current().unwrap_or(index);
    let mut next = Index {
        entries: base.entries.clone(),
        fetched_at: base.fetched_at,
    };
    let stamp = now_ms();
    let mut named = 0usize;
    for e in &mut next.entries {
        if let Some(files) = learned.remove(&e.id) {
            named += usize::from(!files.is_empty());
            e.files = files;
            e.enriched_at = Some(stamp);
        }
    }
    write_cache(&app, &next);
    install(next);
    log::info!("track index: {named} of {} pages named their files", targets.len());
}

/// The track ids one drip round has claimed, released on drop and marked [`settled`].
///
/// Deliberately a guard rather than a call at the end of the round. Every early return in
/// [`round`] is a path where nothing was learned - the listing wouldn't load, the session's
/// page budget was already spent, no candidate pages to read - and a claim left behind on one
/// of those is **permanent**: [`resolve`] goes on reporting that track as pending, so the
/// browser shows "Looking…" and never stops, *and* the same claim filters it out of the queue,
/// so nothing ever looks again. It is stuck in both directions at once.
///
/// The path that actually did it was `ensure_loaded` failing, which is an ordinary Tuesday for
/// a Cloudflare-fronted site. Dropping instead of clearing also covers a panic mid-round.
///
/// Settling them is the other half, and it is what makes the notification in [`drip`] safe:
/// without it, the re-resolve that event triggers would queue the same unanswerable ids, drip
/// again, learn nothing again, and notify again - forever. A track the drip has already been
/// through is not asked about twice.
struct Claim(Vec<String>);

impl Drop for Claim {
    fn drop(&mut self) {
        let mut f = lock(in_flight());
        let mut s = lock(settled());
        for w in &self.0 {
            f.remove(w);
            s.insert(w.clone());
        }
    }
}

// ───────────────────────────────── public API ─────────────────────────────────

/// Which of `tracks` the catalog has, and which the drip is still working on.
///
/// `tracks` should be the ids the browser is actually showing as missing, busiest first: what
/// doesn't resolve here becomes the drip's queue, so the order is also the priority.
pub async fn resolve(app: &tauri::AppHandle, tracks: Vec<String>) -> anyhow::Result<Resolution> {
    let index = ensure_loaded(app).await?;
    let lookup = build_lookup(&index.entries);

    let mut found = HashMap::new();
    let mut unresolved = Vec::new();
    for t in tracks {
        match find(&lookup, &index.entries, &t) {
            Some(hit) => {
                found.insert(t, hit);
            }
            None => unresolved.push(t),
        }
    }

    // Queue whatever is left, minus anything already being worked on and anything the drip has
    // already been through without finding - see [`settled`].
    let queue: Vec<String> = {
        let mut f = lock(in_flight());
        let s = lock(settled());
        unresolved
            .iter()
            .filter(|t| !t.trim().is_empty())
            .filter(|t| !s.contains(*t))
            .filter(|t| f.insert((*t).clone()))
            .cloned()
            .collect()
    };
    if !queue.is_empty() {
        let app = app.clone();
        let queued = queue.clone();
        tauri::async_runtime::spawn(drip(app, queued));
    }

    // Only the tracks *this* caller asked about, and only the ones actually being worked on.
    // Returning the whole in-flight set would report another view's queue as this one's, and
    // would keep reporting ids the loop above deliberately dropped for being blank.
    let pending = {
        let f = lock(in_flight());
        unresolved.into_iter().filter(|t| f.contains(t)).collect()
    };
    Ok(Resolution {
        found,
        pending,
        fetched_at: Some(index.fetched_at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, title: &str, slug: &str, files: &[&str]) -> Entry {
        Entry {
            id,
            slug: slug.to_string(),
            title: title.to_string(),
            link: format!("https://mxb-mods.com/{slug}/"),
            date: String::new(),
            files: files.iter().map(|s| s.to_string()).collect(),
            enriched_at: files.is_empty().then_some(None).unwrap_or(Some(1)),
        }
    }

    fn fixture() -> Vec<Entry> {
        vec![
            entry(1, "Farm14 v0.1", "farm14-v0-1", &["Farm14"]),
            entry(2, "Highland-Mx", "highland-mx", &[]),
            entry(3, "Rail MX", "rail-mx", &["Motoland_MX_Park"]),
            entry(4, "Five-Two SMX", "five-two-smx", &["SMX"]),
        ]
    }

    #[test]
    fn a_version_suffix_is_matched_by_file_name_not_title() {
        let e = fixture();
        let hit = find(&build_lookup(&e), &e, "Farm14").expect("Farm14");
        assert_eq!(hit.slug, "farm14-v0-1");
        assert_eq!(hit.via, Via::File);
    }

    #[test]
    fn a_renamed_track_is_matched_by_file_name() {
        let e = fixture();
        let hit = find(&build_lookup(&e), &e, "Motoland_MX_Park").expect("Motoland");
        assert_eq!(hit.slug, "rail-mx");
    }

    #[test]
    fn titles_still_match_when_no_file_name_was_learned() {
        let e = fixture();
        let hit = find(&build_lookup(&e), &e, "Highland-Mx").expect("Highland");
        assert_eq!(hit.via, Via::Title);
    }

    /// The two sides fold separators and case the same way `trackContent.ts` does.
    #[test]
    fn matching_ignores_case_and_separators() {
        let e = fixture();
        let l = build_lookup(&e);
        assert!(find(&l, &e, "highland mx").is_some());
        assert!(find(&l, &e, "HIGHLAND_MX").is_some());
        // Separators fold on the file-name side too, so a host who typed the id with a space
        // still lands on the mod that ships `Farm14.pkz`.
        assert_eq!(find(&l, &e, "farm 14").map(|h| h.slug), Some("farm14-v0-1".into()));
        // Folding is not fuzzing: a different id stays unmatched.
        assert!(find(&l, &e, "farm 15").is_none());
    }

    #[test]
    fn unknown_and_empty_track_ids_match_nothing() {
        let e = fixture();
        let l = build_lookup(&e);
        assert!(find(&l, &e, "2026_ARLMX_RD11_INDIANA_Pro").is_none());
        assert!(find(&l, &e, "").is_none());
        assert!(find(&l, &e, "   ").is_none());
    }

    /// The drip's targets: only pages never read, ranked by how close the title is.
    #[test]
    fn candidates_prefer_the_closest_unread_title() {
        let entries = vec![
            entry(1, "Ironman", "ironman", &[]),
            entry(2, "Ironman 2024 Remastered Edition", "ironman-2024-r", &[]),
            entry(3, "Ironman v2", "ironman-v2", &[]),
        ];
        let picked: Vec<&str> = candidates(&entries, "Ironman")
            .into_iter()
            .map(|i| entries[i].slug.as_str())
            .collect();
        assert_eq!(picked[0], "ironman");
        assert_eq!(picked[1], "ironman-v2");
    }

    #[test]
    fn candidates_skip_pages_already_read() {
        let entries = vec![entry(1, "Farm14 v0.1", "farm14-v0-1", &["Farm14"])];
        assert!(candidates(&entries, "Farm14").is_empty());
    }

    /// A two-character id would otherwise pull in most of the catalog for nothing.
    #[test]
    fn candidates_ignore_ids_too_short_to_discriminate() {
        let entries = vec![entry(1, "LL Compound", "ll-compound", &[])];
        assert!(candidates(&entries, "LL").is_empty());
    }

    /// The narrow arm's wins: a track id that is a published title plus a suffix.
    #[test]
    fn candidates_keep_a_title_the_id_is_built_on() {
        let entries = vec![
            entry(1, "Outpost", "outpost", &[]),
            entry(2, "island sx", "island-sx", &[]),
        ];
        assert_eq!(candidates(&entries, "Outpost - Prepped"), vec![0]);
        assert_eq!(candidates(&entries, "GHPTRACKS_island sx"), vec![1]);
    }

    /// What rounds 2-5 of the first live run spent 32 pages on. A league id contains plenty
    /// of short generic words, and none of those posts is the track.
    #[test]
    fn candidates_drop_generic_titles_swallowed_by_a_long_id() {
        let entries = vec![
            entry(1, "Indiana", "indiana", &[]),
            entry(2, "Pro", "pro", &[]),
            entry(3, "MX", "mx", &[]),
        ];
        assert!(candidates(&entries, "2026_ARLMX_RD11_INDIANA_Pro").is_empty());
    }

    /// Tightening one arm must not touch the other two - these are the round-one wins.
    #[test]
    fn candidates_still_widen_on_title_and_slug() {
        let entries = vec![
            entry(1, "Farm14 v0.1", "farm14-v0-1", &[]),
            entry(2, "Rocky Mountain NP", "rocky-mountain-the-colorado-trail", &[]),
        ];
        assert_eq!(candidates(&entries, "Farm14"), vec![0]);
        assert_eq!(candidates(&entries, "The Colorado Trail"), vec![1]);
    }

    /// The budget spans the whole cascade, which a per-run bound did not.
    #[test]
    fn the_page_budget_is_spent_across_rounds_not_reset_by_them() {
        assert_eq!(remaining_budget(0), SESSION_PAGE_BUDGET);
        assert_eq!(remaining_budget(49), SESSION_PAGE_BUDGET - 49);
        // The five rounds that actually ran were 49 + 12 + 10 + 9 + 1 = 81, which a per-run
        // cap of 60 allowed. Round two now runs down to nothing and round three is refused.
        assert_eq!(remaining_budget(49 + 12), 0);
        assert_eq!(remaining_budget(81), 0);
        // And it never wraps, however far over a resumed session starts.
        assert_eq!(remaining_budget(usize::MAX), 0);
    }

    #[test]
    fn a_listing_refresh_keeps_what_enrichment_learned() {
        let known: HashMap<u64, (Vec<String>, Option<i64>)> =
            [(1, (vec!["Farm14".to_string()], Some(99)))].into_iter().collect();
        let fresh = ModSummary {
            id: 1,
            slug: "farm14-v0-1".into(),
            title: "Farm14 v0.2".into(),
            link: "https://mxb-mods.com/farm14-v0-1/".into(),
            date: String::new(),
            image: None,
            category_id: TRACKS_CATEGORY,
            author: None,
        };
        let e = entry_from(fresh, &known);
        assert_eq!(e.title, "Farm14 v0.2");
        assert_eq!(e.files, vec!["Farm14".to_string()]);
        assert_eq!(e.enriched_at, Some(99));
    }

    #[test]
    fn a_post_new_to_the_listing_starts_unenriched() {
        let e = entry_from(
            ModSummary {
                id: 7,
                slug: "new-track".into(),
                title: "New Track".into(),
                link: String::new(),
                date: String::new(),
                image: None,
                category_id: TRACKS_CATEGORY,
                author: None,
            },
            &HashMap::new(),
        );
        assert!(e.files.is_empty());
        assert_eq!(e.enriched_at, None);
    }

    /// A file name beats a title that folds to the same key.
    #[test]
    fn file_names_outrank_titles() {
        let entries = vec![
            entry(1, "SMX", "smx-the-other-one", &[]),
            entry(2, "Five-Two SMX", "five-two-smx", &["SMX"]),
        ];
        let hit = find(&build_lookup(&entries), &entries, "SMX").expect("SMX");
        assert_eq!(hit.slug, "five-two-smx");
        assert_eq!(hit.via, Via::File);
    }

    /// The claim has to come back on *every* exit from a drip round, not just the one that
    /// finished its work. A track left claimed is reported pending forever and requeued never,
    /// so the browser sits on "Looking…" for the rest of the session.
    #[test]
    fn a_claim_is_released_on_an_early_return() {
        let ids = vec!["leaky-track".to_string()];
        lock(in_flight()).extend(ids.iter().cloned());

        // Stands in for `ensure_loaded` failing: the guard is built, the round gives up.
        {
            let _claim = Claim(ids.clone());
        }

        assert!(!lock(in_flight()).contains("leaky-track"));
        // …and it is not asked again, which is what keeps the notification from looping.
        assert!(lock(settled()).contains("leaky-track"));
    }

    /// And on a panic, which is the other way a round can stop without reaching its end.
    #[test]
    fn a_claim_is_released_on_a_panic() {
        let ids = vec!["panicky-track".to_string()];
        lock(in_flight()).extend(ids.iter().cloned());

        let caught = std::panic::catch_unwind(|| {
            let _claim = Claim(ids.clone());
            panic!("mid-round");
        });

        assert!(caught.is_err());
        assert!(!lock(in_flight()).contains("panicky-track"));
    }
}
