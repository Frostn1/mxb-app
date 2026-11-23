//! mxb-mods.com's official read-only API, through our control plane.
//!
//! mxb-mods.com asked us to stop scraping it: failed star ratings (admin-ajax `load_results`,
//! 400) and the removed popular-posts plugin (404) were being retried in tight loops, and
//! Wordfence throttled our users for it. The site gave us an API instead. Its key may not ship
//! in the app, and its limit (120 requests a minute) is shared by every user, so the app never
//! talks to it: it asks `{control_plane}/v1/mxbmods/...`, which holds the key, caches the
//! answers and passes the upstream JSON through unchanged. Nothing secret lives here.
//!
//! Used only while mxb-mods.com is the active catalog ([`active`]). gpb-mods.com has no API
//! and keeps its site paths in [`super::mxb`].
//!
//! The rules, from the contract with the control plane:
//! - one attempt per call, never a retry on the spot;
//! - a failure parks the whole client ([`Gate`]): 30 s, doubling, capped at 10 min, or the
//!   server's `Retry-After` when that is longer; while parked nothing is sent at all;
//! - failures are cached too, so a ratings call that failed is not repeated per card, per
//!   render or per listing.

use super::{ModRating, ModSort, ModSummary};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Per request. The control plane answers from its cache almost always; a slow answer is a
/// failure worth backing off from, not one worth waiting on.
const TIMEOUT: Duration = Duration::from_secs(8);

/// What one listing page asks for, the same 24 the site's own pages and the UI's "is there
/// more?" check use.
pub const PER_PAGE: u32 = 24;

/// The control plane accepts at most this many ids per ratings call.
pub const RATINGS_BATCH: usize = 100;

/// How long a rating (or the absence of one) is trusted. Same as the control plane's own.
pub const RATING_TTL: Duration = Duration::from_secs(10 * 60);

/// The longest search text the control plane accepts.
const MAX_SEARCH_CHARS: usize = 100;

/// The first cooldown after a failure, and the most it doubles up to.
pub const FIRST_STEP: Duration = Duration::from_secs(30);
pub const MAX_STEP: Duration = Duration::from_secs(10 * 60);

/// Shown when the popular listing can't be had. There is no fallback for it: the site's own
/// popular-posts endpoint was removed, and asking it anyway is what got us throttled.
pub const POPULAR_UNAVAILABLE: &str =
    "The popular list isn't available right now. Try again in a minute.";

/// Exponential back-off as plain state, so it can be tested with a fake clock.
///
/// `fail` parks until `now + step`, doubling `step` each consecutive failure (30 s, 60 s,
/// 120 s ... capped at [`MAX_STEP`]). A `Retry-After` longer than the step wins. `succeed`
/// resets it. While parked, [`Gate::open`] is false and callers send nothing.
#[derive(Debug, Clone)]
pub struct Gate {
    until: Option<Instant>,
    step: Duration,
    first: Duration,
    max: Duration,
}

impl Default for Gate {
    fn default() -> Self {
        Self::new(FIRST_STEP, MAX_STEP)
    }
}

impl Gate {
    pub const fn new(first: Duration, max: Duration) -> Self {
        Self {
            until: None,
            step: Duration::ZERO,
            first,
            max,
        }
    }

    /// May a request go out at `now`?
    pub fn open(&self, now: Instant) -> bool {
        self.until.is_none_or(|u| now >= u)
    }

    /// When the gate opens again, if it is closed at `now`.
    pub fn until(&self, now: Instant) -> Option<Instant> {
        self.until.filter(|u| *u > now)
    }

    /// The step the last failure parked for.
    #[cfg(test)]
    pub fn step(&self) -> Duration {
        self.step
    }

    /// A failure at `now`. Returns how long the gate is now closed for.
    pub fn fail(&mut self, now: Instant, retry_after: Option<Duration>) -> Duration {
        self.step = if self.step.is_zero() {
            self.first
        } else {
            (self.step * 2).min(self.max)
        };
        let wait = retry_after.map_or(self.step, |ra| ra.max(self.step));
        self.until = Some(now + wait);
        wait
    }

    pub fn succeed(&mut self) {
        self.step = Duration::ZERO;
        self.until = None;
    }
}

/// `Retry-After` in seconds (the only form the control plane and the site send).
pub fn retry_after(header: Option<&str>) -> Option<Duration> {
    header
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// Is mxb-mods.com the catalog being browsed? The API covers only that site.
pub fn active() -> bool {
    crate::mxb_session::base() == crate::mxb_session::MXB_SITE.base
}

/// The session's client, aimed at the real control plane.
pub fn api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| Api::new(&format!("{}/v1/mxbmods", crate::paintsync::control_plane())))
}

/// A cached rating: `None` is "no rating", which is cached just as firmly as a score.
type RatingEntry = (Option<ModRating>, Instant);

pub struct Api {
    base: String,
    client: Result<reqwest::Client, String>,
    gate: Mutex<Gate>,
    ratings: Mutex<HashMap<u64, RatingEntry>>,
    /// One ratings call at a time, so cards asking together share one answer instead of each
    /// sending the same ids.
    ratings_flight: tokio::sync::Mutex<()>,
    /// Category id to name, fetched once per session.
    categories: Mutex<Option<HashMap<u64, String>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Api {
    /// `base` is the `/v1/mxbmods` root, no trailing slash. Injectable so the tests can aim it
    /// at a local server.
    pub fn new(base: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("MXB-App/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string());
        Self {
            base: base.trim_end_matches('/').to_string(),
            client,
            gate: Mutex::new(Gate::default()),
            ratings: Mutex::new(HashMap::new()),
            ratings_flight: tokio::sync::Mutex::new(()),
            categories: Mutex::new(None),
        }
    }

    fn fail(&self, why: &str, retry: Option<Duration>) {
        let wait = lock(&self.gate).fail(Instant::now(), retry);
        log::warn!("mxb-mods API via the control plane failed ({why}); not asking again for {wait:?}");
    }

    /// One GET, one attempt. `Ok(None)` is a 404 (the API's "no such mod"), which is an
    /// answer, not a failure.
    async fn get(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<Option<Value>> {
        if let Some(until) = lock(&self.gate).until(Instant::now()) {
            anyhow::bail!(
                "the mod catalogue is cooling down after a failed request ({}s left)",
                until.saturating_duration_since(Instant::now()).as_secs()
            );
        }
        let client = self.client.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
        let url = format!("{}{}", self.base, path);
        let resp = match client.get(&url).query(query).send().await {
            Ok(r) => r,
            Err(e) => {
                self.fail(&format!("{path}: {e}"), None);
                return Err(anyhow::anyhow!("couldn't reach the mod catalogue: {e}"));
            }
        };
        let status = resp.status().as_u16();
        if status == 404 {
            lock(&self.gate).succeed();
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            let retry = match status {
                429 | 503 => retry_after(
                    resp.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                ),
                _ => None,
            };
            self.fail(&format!("{path} -> {status}"), retry);
            anyhow::bail!("the mod catalogue answered {status}");
        }
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => {
                self.fail(&format!("{path}: body: {e}"), None);
                anyhow::bail!("the mod catalogue's answer was cut off");
            }
        };
        match serde_json::from_str::<Value>(&body) {
            Ok(v) => {
                lock(&self.gate).succeed();
                Ok(Some(v))
            }
            Err(e) => {
                self.fail(&format!("{path}: not JSON: {e}"), None);
                anyhow::bail!("the mod catalogue sent back something that isn't JSON")
            }
        }
    }

    /// A body that parsed as JSON but not into the shape asked for is a failure too.
    fn shape_error(&self, what: &str) -> anyhow::Error {
        self.fail(&format!("{what}: unexpected shape"), None);
        anyhow::anyhow!("the mod catalogue's {what} wasn't in the expected shape")
    }

    /// One page of a category, newest or oldest first, optionally searched.
    pub async fn mods(
        &self,
        query: &str,
        category_id: u32,
        sort: ModSort,
        page: u32,
    ) -> anyhow::Result<Vec<ModSummary>> {
        let order = if matches!(sort, ModSort::Oldest) { "asc" } else { "desc" };
        let mut q: Vec<(&str, String)> = vec![
            ("category", category_id.to_string()),
            ("orderby", "date".to_string()),
            ("order", order.to_string()),
            ("page", page.clamp(1, 500).to_string()),
            ("per_page", PER_PAGE.to_string()),
        ];
        let search: String = query.trim().chars().take(MAX_SEARCH_CHARS).collect();
        if !search.is_empty() {
            q.push(("search", search));
        }
        let Some(v) = self.get("/mods", &q).await? else {
            return Ok(Vec::new());
        };
        let items = parse_list(&v, category_id).ok_or_else(|| self.shape_error("listing"))?;
        self.seed_ratings(&v);
        Ok(items)
    }

    /// One page of a category ranked by views over `range` (`all`, `30d`, `7d`, `24h`).
    pub async fn popular(&self, range: &str, category_id: u32, page: u32) -> anyhow::Result<Vec<ModSummary>> {
        let q: Vec<(&str, String)> = vec![
            ("range", range.to_string()),
            ("category", category_id.to_string()),
            ("page", page.clamp(1, 500).to_string()),
            ("per_page", PER_PAGE.to_string()),
        ];
        let Some(v) = self.get("/popular", &q).await? else {
            return Ok(Vec::new());
        };
        let items = parse_list(&v, category_id).ok_or_else(|| self.shape_error("popular list"))?;
        self.seed_ratings(&v);
        Ok(items)
    }

    /// The full record for a slug, or `None` when there is no such mod. A slug the control
    /// plane would refuse (it validates them) cannot be a WordPress slug either, so it is
    /// answered here without a request; a 400 would otherwise park the client.
    pub async fn by_slug(&self, slug: &str) -> anyhow::Result<Option<Value>> {
        let slug = slug.trim().to_lowercase();
        if !valid_slug(&slug) {
            return Ok(None);
        }
        let v = self.get(&format!("/mods/slug/{slug}"), &[]).await?;
        match v {
            Some(v) if v.get("id").and_then(Value::as_u64).is_some() => Ok(Some(v)),
            Some(_) => Err(self.shape_error("mod record")),
            None => Ok(None),
        }
    }

    /// Category names by id, fetched once and kept for the session.
    pub async fn categories(&self) -> anyhow::Result<HashMap<u64, String>> {
        if let Some(map) = lock(&self.categories).as_ref() {
            return Ok(map.clone());
        }
        let v = self
            .get("/categories", &[])
            .await?
            .ok_or_else(|| anyhow::anyhow!("the mod catalogue has no category list"))?;
        let map = parse_categories(&v).ok_or_else(|| self.shape_error("category list"))?;
        *lock(&self.categories) = Some(map.clone());
        Ok(map)
    }

    /// A listing already carries every mod's score: keep them, so the ratings pass that
    /// follows a listing is answered without a request.
    fn seed_ratings(&self, listing: &Value) {
        let expires = Instant::now() + RATING_TTL;
        let mut cache = lock(&self.ratings);
        for (id, r) in listing_ratings(listing) {
            cache.insert(id, (r, expires));
        }
    }

    /// Scores for these ids. Never an error: a rating decorates a card.
    ///
    /// Batched, one call per [`RATINGS_BATCH`] ids that aren't already cached. An id the
    /// answer leaves out is cached as "no rating" for [`RATING_TTL`]. If a call fails, every
    /// id still wanted is cached as "no rating" until the gate opens again, so other cards and
    /// re-renders asking for the same (or different) ids cost nothing.
    pub async fn ratings(&self, ids: &[u64]) -> HashMap<u64, ModRating> {
        let _one_at_a_time = self.ratings_flight.lock().await;
        let mut out = HashMap::new();
        let mut missing = Vec::new();
        {
            let cache = lock(&self.ratings);
            let now = Instant::now();
            for &id in ids {
                match cache.get(&id) {
                    Some((r, expires)) if now < *expires => {
                        if let Some(r) = r {
                            out.insert(id, *r);
                        }
                    }
                    _ => missing.push(id),
                }
            }
        }
        missing.sort_unstable();
        missing.dedup();

        let mut chunks = missing.chunks(RATINGS_BATCH);
        while let Some(chunk) = chunks.next() {
            let joined = chunk.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
            let answer = match self.get("/ratings", &[("ids", joined)]).await {
                Ok(Some(v)) => match parse_ratings(&v) {
                    Some(map) => Ok(map),
                    None => Err(self.shape_error("ratings")),
                },
                Ok(None) => Ok(HashMap::new()),
                Err(e) => Err(e),
            };
            match answer {
                Ok(map) => {
                    let expires = Instant::now() + RATING_TTL;
                    let mut cache = lock(&self.ratings);
                    for &id in chunk {
                        let r = map.get(&id).copied();
                        cache.insert(id, (r, expires));
                        if let Some(r) = r {
                            out.insert(id, r);
                        }
                    }
                }
                Err(e) => {
                    log::debug!("ratings unavailable: {e:#}");
                    let now = Instant::now();
                    let expires = lock(&self.gate).until(now).unwrap_or(now + FIRST_STEP);
                    let mut cache = lock(&self.ratings);
                    for &id in chunk.iter().chain(chunks.by_ref().flatten()) {
                        cache.insert(id, (None, expires));
                    }
                    break;
                }
            }
        }
        out
    }
}

/// What the control plane accepts as a slug: `^[a-z0-9%-]{1,200}$`.
pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 200
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'%' || b == b'-')
}

/// The API's dates are UTC with a `Z`. Cards get the same shape the mirror and the site's own
/// listing gave them: no zone suffix.
fn date(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(|d| d.trim_end_matches('Z').to_string())
        .unwrap_or_default()
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

/// A COMPACT (or full) record as the app's card. `None` without an id and a slug: the app keys
/// everything by them.
pub fn summary(v: &Value, category_id: u32) -> Option<ModSummary> {
    let id = v.get("id")?.as_u64()?;
    let slug = text(v, "slug")?.to_string();
    Some(ModSummary {
        id,
        slug,
        title: v.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
        link: v.get("link").and_then(Value::as_str).unwrap_or("").to_string(),
        date: date(v, "date"),
        image: text(v, "thumbnail")
            .or_else(|| text(v, "image"))
            .map(str::to_string),
        category_id,
        author: v
            .get("author")
            .and_then(|a| text(a, "name"))
            .map(|n| n.trim().to_string()),
    })
}

/// `{"items":[...]}` (both `/mods` and `/popular`) as cards. `None` when there is no `items`
/// array at all; individual unreadable rows are skipped.
pub fn parse_list(v: &Value, category_id: u32) -> Option<Vec<ModSummary>> {
    let items = v.get("items")?.as_array()?;
    Some(items.iter().filter_map(|i| summary(i, category_id)).collect())
}

fn rating_of(v: &Value) -> Option<ModRating> {
    let average = v.get("average").and_then(Value::as_f64)?;
    let count = v.get("count").and_then(Value::as_u64)?;
    if !average.is_finite() {
        return None;
    }
    Some(ModRating {
        average: average as f32,
        count: count.min(u32::MAX as u64) as u32,
    })
}

/// `{"ratings":{"54857":{"average":3.9,"count":64}}}`. Ids without votes are absent.
pub fn parse_ratings(v: &Value) -> Option<HashMap<u64, ModRating>> {
    let map = v.get("ratings")?.as_object()?;
    Some(
        map.iter()
            .filter_map(|(id, r)| Some((id.parse::<u64>().ok()?, rating_of(r)?)))
            .filter(|(_, r)| r.count > 0)
            .collect(),
    )
}

/// The rating each listed mod carries, `None` for one nobody has voted on.
pub fn listing_ratings(v: &Value) -> Vec<(u64, Option<ModRating>)> {
    v.get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    let id = i.get("id")?.as_u64()?;
                    let r = i.get("rating").and_then(rating_of).filter(|r| r.count > 0);
                    Some((id, r))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `{"items":[{"id":29,"name":"Bikes",...}]}` as id to name.
pub fn parse_categories(v: &Value) -> Option<HashMap<u64, String>> {
    let items = v.get("items")?.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|c| Some((c.get("id")?.as_u64()?, text(c, "name")?.to_string())))
            .collect(),
    )
}

/// What the detail page takes from a full record. Downloads, the version and the byline are
/// not in the API; they still come from the rendered page.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub id: u64,
    pub title: String,
    pub link: String,
    pub date: String,
    pub content_html: String,
    /// Full-size pictures, in the post's order.
    pub images: Vec<String>,
    /// The card picture, for a post whose gallery is otherwise empty.
    pub image: Option<String>,
    pub author: Option<String>,
    pub category_ids: Vec<u64>,
}

pub fn parse_meta(v: &Value) -> Option<Meta> {
    let summary = summary(v, 0)?;
    Some(Meta {
        id: summary.id,
        title: summary.title,
        link: summary.link,
        date: summary.date,
        content_html: v.get("content_html").and_then(Value::as_str).unwrap_or("").to_string(),
        images: v
            .get("images")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        image: text(v, "image").or_else(|| text(v, "thumbnail")).map(str::to_string),
        author: summary.author,
        category_ids: category_ids(v),
    })
}

pub fn category_ids(v: &Value) -> Vec<u64> {
    v.get("categories")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

impl ModSort {
    /// The control plane's popular `range` for this sort, `None` for the plain listing.
    pub fn api_range(self) -> Option<&'static str> {
        match self {
            ModSort::PopularAll => Some("all"),
            ModSort::PopularMonth => Some("30d"),
            ModSort::PopularWeek => Some("7d"),
            ModSort::Newest | ModSort::Oldest => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! A tiny HTTP/1.1 server on 127.0.0.1 for the tests: records every request line and
    //! answers from a closure. No network beyond the loopback.

    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    pub type Reply = (u16, Vec<(String, String)>, String);
    pub type Handler = Arc<dyn Fn(&str) -> Reply + Send + Sync>;

    /// Returns the base URL and the log of request targets (`/path?query`).
    pub fn serve(handler: Handler) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for sock in listener.incoming() {
                let Ok(mut sock) = sock else { return };
                let (handler, log) = (handler.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 8192];
                    loop {
                        let Ok(n) = sock.read(&mut chunk) else { return };
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let head = String::from_utf8_lossy(&buf).to_string();
                    let target = head
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("")
                        .to_string();
                    log.lock().unwrap().push(target.clone());
                    let (status, headers, body) = handler(&target);
                    let mut out = format!(
                        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                        body.len()
                    );
                    for (k, v) in headers {
                        out.push_str(&format!("{k}: {v}\r\n"));
                    }
                    out.push_str("\r\n");
                    out.push_str(&body);
                    let _ = sock.write_all(out.as_bytes());
                    let _ = sock.flush();
                });
            }
        });
        (base, seen)
    }

    pub fn ok(body: &str) -> Reply {
        (200, vec![("Content-Type".into(), "application/json".into())], body.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{ok, serve, Reply};
    use super::*;
    use std::sync::Arc;

    // Samples straight from the contract (live answers, 2026-11-07).
    const COMPACT_LIST: &str = r#"{"page":1,"per_page":2,"total":12694,"total_pages":6347,"items":[
      {"id":196902,"slug":"the-sand-track-mx-2","title":"the sand track MX","link":"https:\/\/mxb-mods.com\/the-sand-track-mx-2\/",
       "date":"2026-11-08T02:20:38Z","modified":"2026-11-08T02:20:38Z","author":{"id":68091,"name":"kdub_403","url":"https:\/\/mxb-mods.com\/user\/kdub_403\/"},
       "categories":[22],"game_versions":["beta-21"],"image":"https:\/\/mxb-mods.com\/wp-content\/uploads\/2026\/11\/img-1730-1-320x257.webp",
       "thumbnail":"https:\/\/mxb-mods.com\/wp-content\/uploads\/2026\/11\/img-1730-1-320x257.webp","excerpt":"a fun flowy track","rating":{"average":0,"count":0},"views":10},
      {"id":196895,"slug":"motomade-honda-for-eyezaya-swap","title":"MotoMade Honda for Eyezaya Swap","link":"https:\/\/mxb-mods.com\/motomade-honda-for-eyezaya-swap\/",
       "date":"2026-11-08T02:12:13Z","author":{"id":49886,"name":"kr611"},"categories":[29,37],"game_versions":[],
       "image":"https:\/\/mxb-mods.com\/wp-content\/uploads\/2026\/11\/neon-1280x720.webp","thumbnail":"https:\/\/mxb-mods.com\/wp-content\/uploads\/2026\/11\/neon-640x360.webp",
       "rating":{"average":0,"count":0},"views":22}]}"#;

    const POPULAR: &str = r#"{"range":"7d","items":[
      {"id":54857,"slug":"oem-bike-pack","title":"MX OEM Bike Pack - v0.19.1","link":"https:\/\/mxb-mods.com\/oem-bike-pack\/",
       "date":"2025-01-09T18:00:07Z","author":{"id":15,"name":"insane"},"categories":[29,45],
       "image":"https:\/\/mxb-mods.com\/a-768x768.webp","thumbnail":"https:\/\/mxb-mods.com\/a-640x640.webp",
       "rating":{"average":3.9,"count":64},"views":2177103,"views_in_range":12507},
      {"id":195566,"slug":"graphics-overhaul-2","title":"Graphics Overhaul","link":"https:\/\/mxb-mods.com\/graphics-overhaul-2\/",
       "date":"2026-11-03T02:38:31Z","author":{"id":68582,"name":"Finner24"},"categories":[609,40],
       "image":"https:\/\/mxb-mods.com\/b-1280x720.webp","thumbnail":"",
       "rating":{"average":4.7,"count":13},"views":7870,"views_in_range":7870},
      {"slug":"no-id","title":"broken row"}]}"#;

    const FULL: &str = r#"{"id":196902,"slug":"the-sand-track-mx-2","title":"the sand track MX","link":"https:\/\/mxb-mods.com\/the-sand-track-mx-2\/",
      "date":"2026-11-08T02:20:38Z","modified":"2026-11-08T02:20:38Z","author":{"id":68091,"name":"kdub_403"},"categories":[22],
      "image":"https:\/\/mxb-mods.com\/img-320x257.webp","thumbnail":"https:\/\/mxb-mods.com\/img-320x257.webp",
      "rating":{"average":0,"count":0},"views":10,"password_protected":false,
      "content_html":"<p>a fun flowy track<\/p>\n<p>great for beginers<\/p>",
      "images":["https:\/\/mxb-mods.com\/img-1730-1.jpeg","https:\/\/mxb-mods.com\/unnamed.jpg"],"tags":[]}"#;

    const RATINGS: &str =
        r#"{"ratings":{"54857":{"average":3.9,"count":64},"195566":{"average":4.7,"count":13}}}"#;

    fn json(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn a_compact_listing_becomes_cards() {
        let cards = parse_list(&json(COMPACT_LIST), 22).unwrap();
        assert_eq!(cards.len(), 2);
        let c = &cards[0];
        assert_eq!((c.id, c.slug.as_str()), (196902, "the-sand-track-mx-2"));
        assert_eq!(c.title, "the sand track MX");
        assert_eq!(c.link, "https://mxb-mods.com/the-sand-track-mx-2/");
        // Same shape the mirror and the site gave the UI: no zone suffix.
        assert_eq!(c.date, "2026-11-08T02:20:38");
        assert_eq!(c.author.as_deref(), Some("kdub_403"));
        assert_eq!(c.category_id, 22);
        // The thumbnail, which is the smaller of the two.
        assert_eq!(cards[1].image.as_deref(), Some("https://mxb-mods.com/wp-content/uploads/2026/11/neon-640x360.webp"));
        // Nobody has voted: seeded as "no rating", not as five empty stars.
        let seeded: Vec<(u64, bool)> = listing_ratings(&json(COMPACT_LIST)).into_iter().map(|(id, r)| (id, r.is_some())).collect();
        assert_eq!(seeded, vec![(196902, false), (196895, false)]);
    }

    #[test]
    fn a_popular_page_becomes_cards_and_seeds_ratings() {
        let v = json(POPULAR);
        let cards = parse_list(&v, 29).unwrap();
        assert_eq!(cards.iter().map(|c| c.id).collect::<Vec<_>>(), [54857, 195566], "a row without an id is skipped");
        // An empty thumbnail falls back to the image.
        assert_eq!(cards[1].image.as_deref(), Some("https://mxb-mods.com/b-1280x720.webp"));
        let seeded = listing_ratings(&v);
        assert_eq!(seeded[0].0, 54857);
        assert_eq!(seeded[0].1.map(|r| r.count), Some(64));
        assert!(parse_list(&json(r#"{"error":"x"}"#), 1).is_none(), "no items array is a shape error");
    }

    #[test]
    fn a_full_record_gives_the_detail_metadata() {
        let m = parse_meta(&json(FULL)).unwrap();
        assert_eq!(m.id, 196902);
        assert_eq!(m.date, "2026-11-08T02:20:38");
        assert_eq!(m.images, ["https://mxb-mods.com/img-1730-1.jpeg", "https://mxb-mods.com/unnamed.jpg"]);
        assert!(m.content_html.starts_with("<p>a fun flowy track</p>"));
        assert_eq!(m.category_ids, [22]);
        assert_eq!(m.author.as_deref(), Some("kdub_403"));
    }

    #[test]
    fn ratings_and_categories_parse() {
        let r = parse_ratings(&json(RATINGS)).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[&54857].count, 64);
        assert!((r[&195566].average - 4.7).abs() < 1e-6);
        assert!(parse_ratings(&json(r#"{"ratings":{}}"#)).unwrap().is_empty());
        assert!(parse_ratings(&json(r#"[]"#)).is_none());

        let c = parse_categories(&json(
            r#"{"items":[{"id":29,"slug":"bikes","name":"Bikes","parent":0},{"id":37,"name":"Liveries"},{"name":"no id"}]}"#,
        ))
        .unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[&37], "Liveries");
    }

    #[test]
    fn slugs_the_control_plane_would_refuse_are_never_sent() {
        assert!(valid_slug("fort-red"));
        assert!(valid_slug("caf%c3%a9-mx"));
        assert!(!valid_slug("fort_red"));
        assert!(!valid_slug("Fort-Red"));
        assert!(!valid_slug(""));
        assert!(!valid_slug(&"a".repeat(201)));
    }

    #[test]
    fn the_gate_doubles_caps_and_resets() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        assert!(g.open(t0));
        assert_eq!(g.fail(t0, None), Duration::from_secs(30));
        assert!(!g.open(t0 + Duration::from_secs(29)));
        assert!(g.open(t0 + Duration::from_secs(30)));
        assert_eq!(g.fail(t0, None), Duration::from_secs(60));
        assert_eq!(g.fail(t0, None), Duration::from_secs(120));
        for _ in 0..10 {
            g.fail(t0, None);
        }
        assert_eq!(g.step(), MAX_STEP, "capped at ten minutes");
        assert_eq!(g.fail(t0, None), MAX_STEP);
        g.succeed();
        assert!(g.open(t0));
        assert_eq!(g.fail(t0, None), Duration::from_secs(30), "success starts the ladder over");
    }

    #[test]
    fn retry_after_wins_when_it_is_longer() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        assert_eq!(g.fail(t0, Some(Duration::from_secs(120))), Duration::from_secs(120));
        assert!(!g.open(t0 + Duration::from_secs(119)));
        assert!(g.open(t0 + Duration::from_secs(120)));
        // A Retry-After shorter than the step does not shorten it.
        assert_eq!(g.fail(t0, Some(Duration::from_secs(5))), Duration::from_secs(60));
        assert_eq!(retry_after(Some(" 120 ")), Some(Duration::from_secs(120)));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after(None), None);
    }

    fn api_with(reply: impl Fn(&str) -> Reply + Send + Sync + 'static) -> (Api, Arc<Mutex<Vec<String>>>) {
        let (base, seen) = serve(Arc::new(reply));
        (Api::new(&format!("{base}/v1/mxbmods")), seen)
    }

    fn count(seen: &Arc<Mutex<Vec<String>>>, prefix: &str) -> usize {
        seen.lock().unwrap().iter().filter(|t| t.starts_with(prefix)).count()
    }

    /// THE regression: a failing ratings call used to be retried per card and per render,
    /// hundreds of times a minute. Twenty asks, for the same ids and for others, are one
    /// request.
    #[tokio::test]
    async fn a_failing_ratings_call_is_not_retried_in_a_loop() {
        let (api, seen) = api_with(|_| (500, vec![], "{}".to_string()));
        for i in 0..20u64 {
            let ids: Vec<u64> = if i % 2 == 0 { vec![1, 2, 3] } else { vec![100 + i, 200 + i] };
            assert!(api.ratings(&ids).await.is_empty());
        }
        assert_eq!(count(&seen, "/v1/mxbmods/ratings"), 1, "{:?}", seen.lock().unwrap());
    }

    #[tokio::test]
    async fn a_rate_limited_ratings_call_waits_out_retry_after() {
        let (api, seen) = api_with(|_| {
            (429, vec![("Retry-After".to_string(), "120".to_string())], r#"{"error":"slow down","retry_after":120}"#.to_string())
        });
        for i in 0..20u64 {
            api.ratings(&[i, i + 1000]).await;
        }
        assert_eq!(count(&seen, "/v1/mxbmods/ratings"), 1);
        let until = lock(&api.gate).until(Instant::now()).expect("parked");
        assert!(until.saturating_duration_since(Instant::now()) > Duration::from_secs(100), "Retry-After: 120 honoured");
        // Every other call through the same client is parked too, without a request.
        assert!(api.mods("", 22, ModSort::Newest, 1).await.is_err());
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ratings_go_out_in_batches_of_a_hundred_and_are_cached() {
        let (api, seen) = api_with(|target| {
            // Answer the first id of each batch; the rest have "no votes".
            let first = target.split("ids=").nth(1).and_then(|s| s.split("%2C").next()).unwrap_or("0").to_string();
            ok(&format!(r#"{{"ratings":{{"{first}":{{"average":4.5,"count":2}}}}}}"#))
        });
        let ids: Vec<u64> = (1..=130).collect();
        let got = api.ratings(&ids).await;
        assert_eq!(count(&seen, "/v1/mxbmods/ratings"), 2, "130 ids are two calls");
        assert_eq!(got.len(), 2);
        assert_eq!(got[&1].count, 2);
        assert_eq!(got[&101].count, 2);

        // Asked again: all cached, the scores and the "no rating"s alike.
        let again = api.ratings(&ids).await;
        assert_eq!(again.len(), 2);
        assert_eq!(count(&seen, "/v1/mxbmods/ratings"), 2);

        // Five new ids among old ones: one call, for just the new ones.
        let mut mixed: Vec<u64> = (126..=135).collect();
        mixed.push(1);
        api.ratings(&mixed).await;
        let log = seen.lock().unwrap().clone();
        assert_eq!(log.len(), 3);
        assert!(log[2].ends_with("ids=131%2C132%2C133%2C134%2C135"), "{}", log[2]);
    }

    #[tokio::test]
    async fn a_listing_answers_the_ratings_pass_that_follows_it() {
        let (api, seen) = api_with(|target| {
            if target.starts_with("/v1/mxbmods/popular") {
                ok(POPULAR)
            } else {
                ok(RATINGS)
            }
        });
        let cards = api.popular("7d", 29, 1).await.unwrap();
        assert_eq!(cards.len(), 2);
        let q = seen.lock().unwrap()[0].clone();
        assert!(q.contains("range=7d") && q.contains("category=29") && q.contains("per_page=24"), "{q}");
        let r = api.ratings(&[54857, 195566]).await;
        assert_eq!(r.len(), 2);
        assert_eq!(seen.lock().unwrap().len(), 1, "the listing's own scores were enough");
    }

    #[tokio::test]
    async fn newest_and_oldest_ask_for_date_order() {
        let (api, seen) = api_with(|_| ok(COMPACT_LIST));
        api.mods("  sand  ", 22, ModSort::Newest, 2).await.unwrap();
        api.mods("", 22, ModSort::Oldest, 1).await.unwrap();
        let log = seen.lock().unwrap().clone();
        assert!(log[0].contains("orderby=date") && log[0].contains("order=desc") && log[0].contains("page=2"), "{}", log[0]);
        assert!(log[0].contains("search=sand"), "{}", log[0]);
        assert!(log[1].contains("order=asc") && !log[1].contains("search="), "{}", log[1]);
    }

    #[tokio::test]
    async fn an_unknown_slug_is_an_answer_not_a_failure() {
        let (api, seen) = api_with(|target| {
            if target.ends_with("/the-sand-track-mx-2") {
                ok(FULL)
            } else {
                (404, vec![], r#"{"code":"not_found"}"#.to_string())
            }
        });
        assert!(api.by_slug("nope").await.unwrap().is_none());
        // Not parked by the 404.
        let full = api.by_slug("the-sand-track-mx-2").await.unwrap().unwrap();
        assert_eq!(category_ids(&full), [22]);
        // Refused locally, no request.
        assert!(api.by_slug("fort_red").await.unwrap().is_none());
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn categories_are_fetched_once() {
        let (api, seen) = api_with(|_| ok(r#"{"items":[{"id":22,"name":"Tracks"}]}"#));
        assert_eq!(api.categories().await.unwrap()[&22], "Tracks");
        assert_eq!(api.categories().await.unwrap()[&22], "Tracks");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_body_that_isnt_the_shape_parks_the_client() {
        let (api, seen) = api_with(|_| ok("<html>not json</html>"));
        assert!(api.popular("all", 22, 1).await.is_err());
        assert!(api.popular("all", 22, 1).await.is_err());
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
