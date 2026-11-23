use super::mxbmods_api::{self, Gate};
use super::{DownloadOption, ModDetail, ModRating, ModSort, ModSource, ModSummary};
use crate::mxb_session;
use futures_util::StreamExt;
use obfstr::obfstr;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PER_PAGE: &str = "24";

/// The mods catalog for whichever game is active.
///
/// mxb-mods.com and gpb-mods.com are the same WordPress build by the same author — same
/// REST shape, same popular-posts plugin, same ratings endpoint — so one implementation
/// serves both and only the host varies (see [`mxb_session::base`]). Category ids differ
/// between them and are supplied by the caller.
pub struct WpModsSource;

impl ModSource for WpModsSource {
    async fn search(
        &self,
        query: &str,
        category_id: u32,
        page: u32,
        sort: ModSort,
    ) -> anyhow::Result<Vec<ModSummary>> {
        search(query, category_id, page, sort).await
    }

    async fn detail(&self, slug: &str) -> anyhow::Result<ModDetail> {
        detail(slug).await
    }
}

/// One client for the whole session, not one per call.
///
/// Two reasons beyond the obvious. It holds [`mxb_session::jar`] — shared with the WebView
/// handshake, so a `cf_clearance` earned there is picked up by this client without
/// rebuilding it — and it reuses connections, so a user typing in the search box costs one
/// TLS handshake rather than one per keystroke, which is the sort of traffic shape that
/// gets a client rate-limited in the first place.
fn client() -> anyhow::Result<&'static Client> {
    static CLIENT: std::sync::OnceLock<Result<Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| build_client().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}


fn build_client() -> anyhow::Result<Client> {
    use reqwest::header::{HeaderMap, HeaderValue};
    // What Chrome actually sends on a same-origin `fetch`. reqwest sends almost none of
    // these on its own, so a request claiming to be Chrome didn't look like one.
    let mut headers = HeaderMap::new();
    for (k, v) in [
        ("accept", "application/json, text/plain, */*"),
        ("accept-language", "en-US,en;q=0.9"),
        ("sec-fetch-site", "same-origin"),
        ("sec-fetch-mode", "cors"),
        ("sec-fetch-dest", "empty"),
        ("referer", mxb_session::base()),
    ] {
        headers.insert(k, HeaderValue::from_static(v));
    }
    // Jar and timeouts come from `mxb_session`. The User-Agent is set again on every request
    // (see `get`), because this client can be built before the WebView has said what its real
    // one is.
    Ok(mxb_session::client_builder().default_headers(headers).build()?)
}

/// Statuses worth trying again *on the spot*: 429 is rate limiting and 503 is usually an
/// interstitial going up, both of which pass on their own.
///
/// 403 is deliberately not here. It is a bot-score refusal, and a second identical request
/// from the same fingerprint on the same connection gets the same answer — retrying it just
/// failed three times more slowly. It is handled a level up instead, by earning a
/// `cf_clearance` in a real browser and trying once more with something actually different.
fn worth_retrying(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 503)
}

/// One response, read into memory, independent of which transport produced it.
///
/// The reqwest client and the WebView bridge return the same shape so everything downstream
/// — the parsers, the `X-WP-Total` paging, the refusal diagnostics — is transport-agnostic.
/// Owned rather than borrowed because the WebView hands back a decoded body, not a stream;
/// that is affordable here because mxb-mods.com only ever serves us JSON and HTML. Actual
/// mod downloads go to MediaFire/Drive/Mega and never come through this path.
pub struct Fetched {
    pub status: u16,
    /// Lowercased header names. Same-origin `fetch` exposes every header, so this is as
    /// complete over the bridge as it is over reqwest.
    pub headers: HashMap<String, String>,
    pub body: String,
    /// The URL that actually answered, for the refusal log.
    pub url: String,
}

impl Fetched {
    fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    fn json<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
        serde_json::from_str(&self.body).map_err(|e| not_json_error(&self.body, &e))
    }
}

/// Which transport this session uses for mxb-mods.com.
///
/// Starts on the HTTP client, which is faster and is what nearly every user needs. A
/// Cloudflare refusal latches it to the WebView for the rest of the session — see
/// [`use_webview`]. Never latches back: a client that has been refused once on a given IP
/// and fingerprint will be refused again, and flapping between transports would just
/// reintroduce the failure on every request.
static WEBVIEW_TRANSPORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Route every mxb-mods.com request through the WebView from here on. `why` goes in the log.
pub fn use_webview(why: &str) {
    if !WEBVIEW_TRANSPORT.swap(true, std::sync::atomic::Ordering::Relaxed) {
        log::info!(
            "switching {} traffic to the WebView for the rest of this session ({why})",
            mxb_session::site().domain
        );
    }
}

/// `MXB_FORCE_WEBVIEW=1` starts on the bridge instead of falling back to it. The only way to
/// exercise this path on a machine Cloudflare is happy with, and a support switch for a user
/// who is blocked from the first request.
fn forced_to_webview() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| {
        let on = matches!(
            std::env::var("MXB_FORCE_WEBVIEW").unwrap_or_default().as_str(),
            "1" | "true" | "yes"
        );
        if on {
            log::info!("MXB_FORCE_WEBVIEW is set — all mxb-mods.com traffic goes via the WebView");
        }
        on
    })
}

/// Whether requests are going through the WebView right now. [`crate::with_clearance`] asks
/// before each attempt: a refusal the WebView itself got is not one another WebView attempt
/// can fix.
pub fn on_webview() -> bool {
    forced_to_webview() || WEBVIEW_TRANSPORT.load(std::sync::atomic::Ordering::Relaxed)
}

/// `GET` with at most one retry over the transient blocks above, then a site-wide cooldown
/// (see [`send`]). Transport errors get the one retry too.
async fn get_with_retry(url: &str, params: &[(&str, String)]) -> anyhow::Result<Fetched> {
    get(url, params, Want::Api).await
}

/// `GET` a rendered page, asked for the way a browser asks for one.
///
/// Its own entry point because Cloudflare guards the rendered pages far more tightly than the
/// JSON API — a user whose catalog browses fine can still be refused the moment they open a
/// single mod, which is the whole shape of this failure.
async fn get_page(url: &str) -> anyhow::Result<Fetched> {
    get(url, &[], Want::Page).await
}

/// Which shape of request this is, and therefore what it has to look like on the wire.
#[derive(Clone, Copy)]
enum Want {
    /// The WP REST API. Script asks for these in a browser, and [`client`]'s default headers
    /// already say exactly that.
    Api,
    /// A rendered page. A browser *navigates* to these — see [`page_headers`] for the HTTP
    /// client and [`crate::mxb_fetch::read_page`] for the WebView.
    Page,
}

/// The headers a browser sends when it navigates to a page, replacing the JSON-fetch defaults
/// [`build_client`] sets.
///
/// Those defaults describe a same-origin `fetch` for JSON, which is what every REST call is.
/// Sending them for a rendered page asks Cloudflare to believe a script wanted an HTML
/// document — a shape no browser produces, on the one path its bot rules actually guard.
fn page_headers() -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    for (k, v) in [
        (
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,\
             image/apng,*/*;q=0.8",
        ),
        ("sec-fetch-mode", "navigate"),
        ("sec-fetch-dest", "document"),
        // A page reached by clicking a link on the site, which is what this stands in for.
        ("sec-fetch-site", "same-origin"),
        ("sec-fetch-user", "?1"),
        ("upgrade-insecure-requests", "1"),
    ] {
        headers.insert(k, HeaderValue::from_static(v));
    }
    headers
}

/// A site-wide cooldown after the site told us to back off (429/503), so a throttled user
/// stops sending anything at all for a while instead of every card, page and keystroke
/// trying again. Same [`Gate`] as the API client, separate instance per site.
pub(crate) struct SiteCooldown {
    gate: Gate,
    /// The status that parked it, for the message a parked request fails with.
    status: u16,
}

impl SiteCooldown {
    pub(crate) const fn new() -> Self {
        Self {
            gate: Gate::new(mxbmods_api::FIRST_STEP, mxbmods_api::MAX_STEP),
            status: 503,
        }
    }
}

fn site_cooldown() -> &'static Mutex<SiteCooldown> {
    static MXB: Mutex<SiteCooldown> = Mutex::new(SiteCooldown::new());
    static GPB: Mutex<SiteCooldown> = Mutex::new(SiteCooldown::new());
    if mxb_session::site().domain == mxb_session::GPB_SITE.domain {
        &GPB
    } else {
        &MXB
    }
}

/// The error a request fails fast with while the site is cooling down.
fn cooling_down(cooldown: &Mutex<SiteCooldown>) -> Option<anyhow::Error> {
    let c = lock(cooldown);
    c.gate
        .until(Instant::now())
        .map(|_| blocked_error(c.status, None))
}

fn note_throttled(cooldown: &Mutex<SiteCooldown>, status: u16, retry: Option<Duration>) {
    let mut c = lock(cooldown);
    let wait = c.gate.fail(Instant::now(), retry);
    c.status = status;
    log::warn!(
        "{} answered {status}; sending it nothing for {wait:?}",
        mxb_session::site().domain
    );
}

fn note_ok(cooldown: &Mutex<SiteCooldown>) {
    lock(cooldown).gate.succeed();
}

async fn get(url: &str, params: &[(&str, String)], want: Want) -> anyhow::Result<Fetched> {
    let cooldown = site_cooldown();
    if on_webview() {
        // No retry here: the bridge is a real browser, so a 429/503 it gets is one the site
        // means. It still parks the site like any other.
        if let Some(e) = cooling_down(cooldown) {
            return Err(e);
        }
        let fetched = match want {
            Want::Api => crate::mxb_fetch::get(url, params).await?,
            Want::Page => crate::mxb_fetch::read_page(url).await?,
        };
        if matches!(fetched.status, 429 | 503) {
            let retry = mxbmods_api::retry_after(fetched.header("retry-after"));
            note_throttled(cooldown, fetched.status, retry);
        } else if fetched.is_success() {
            note_ok(cooldown);
        }
        return Ok(fetched);
    }
    send(client()?, cooldown, url, params, want).await
}

/// One request over the HTTP client: at most two attempts, one second apart, and only for a
/// transient status or a transport error. A `Retry-After` longer than that pause is obeyed
/// rather than raced, and a 429/503 that survives parks the whole site ([`SiteCooldown`]):
/// until it opens, every request fails fast without being sent. Nothing here loops.
async fn send(
    client: &Client,
    cooldown: &Mutex<SiteCooldown>,
    url: &str,
    params: &[(&str, String)],
    want: Want,
) -> anyhow::Result<Fetched> {
    const ATTEMPTS: u32 = 2;
    const PAUSE: Duration = Duration::from_secs(1);
    for attempt in 1..=ATTEMPTS {
        if let Some(e) = cooling_down(cooldown) {
            return Err(e);
        }
        let started = Instant::now();
        let req = client
            .get(url)
            .query(params)
            .header(reqwest::header::USER_AGENT, mxb_session::ua());
        let req = match want {
            Want::Api => req,
            Want::Page => req.headers(page_headers()),
        };
        match req.send().await {
            Ok(resp) if !worth_retrying(resp.status()) => {
                // Debug, not info: search runs on every keystroke, and a line per keystroke
                // would bury the one failure worth reading. `MXB_LOG=debug` turns it on.
                let fetched = into_fetched(resp).await;
                if fetched.is_success() {
                    note_ok(cooldown);
                }
                log::debug!(
                    "GET {} -> {} in {:?}",
                    path_of(url),
                    fetched.status,
                    started.elapsed()
                );
                return Ok(fetched);
            }
            Ok(resp) => {
                let status = resp.status().as_u16();
                let fetched = into_fetched(resp).await;
                let retry = mxbmods_api::retry_after(fetched.header("retry-after"));
                if attempt == ATTEMPTS || retry.is_some_and(|r| r > PAUSE) {
                    note_throttled(cooldown, status, retry);
                    return Ok(fetched);
                }
                log::warn!(
                    "GET {} -> {status} (attempt {attempt}/{ATTEMPTS}), trying once more in {PAUSE:?}",
                    path_of(url)
                );
            }
            Err(e) => {
                if attempt == ATTEMPTS {
                    log::warn!("GET {} failed after {ATTEMPTS} attempts: {e}", path_of(url));
                    return Err(e.into());
                }
                log::warn!(
                    "GET {} failed (attempt {attempt}/{ATTEMPTS}): {e}",
                    path_of(url)
                );
            }
        }
        tokio::time::sleep(PAUSE).await;
    }
    Err(anyhow::anyhow!("request failed"))
}

/// Read a reqwest response into the transport-agnostic shape. A body that won't decode
/// becomes an empty one rather than an error — the status and headers are still worth
/// having, and that is exactly the case the refusal diagnostics exist to report.
async fn into_fetched(resp: reqwest::Response) -> Fetched {
    let status = resp.status().as_u16();
    let url = resp.url().to_string();
    let headers = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_string(), v.to_string())))
        .collect();
    Fetched {
        status,
        headers,
        body: resp.text().await.unwrap_or_default(),
        url,
    }
}

/// The path part of a URL, for logs. The full URL is mostly a constant prefix plus
/// percent-encoded query params, and the path is the bit that distinguishes the two
/// endpoints Cloudflare treats differently — the WP REST API and the rendered mod page.
fn path_of(url: &str) -> &str {
    url.strip_prefix(mxb_session::base()).unwrap_or(url)
}

/// Cloudflare refusals, shared with [`super::shop_catalog`] — both catalogs sit behind it,
/// and `main.rs`'s clearance retry downcasts to one type for both. Defined in [`super`] and
/// re-exported here so `mods::mxb::Blocked` still names it.
pub use super::Blocked;

/// Turn a blocked response into something a person can act on. The raw reqwest `Display`
/// ("HTTP status client error (403 Forbidden) for url (…)") told users nothing and shipped
/// them a URL with percent-encoded query params.
///
/// The 403 wording assumes the handshake above it has already been tried and failed, since
/// that is the only way this message reaches a user.
///
/// `ray` is Cloudflare's `cf-ray` id for the refused request. It goes on the end because it
/// is the one token that identifies this specific block on Cloudflare's side — a screenshot
/// carrying it can be diagnosed without asking anyone to find their log file.
fn blocked_error(status: u16, ray: Option<&str>) -> anyhow::Error {
    let message = match status {
        403 => "mxb-mods.com refused the request (403). Hit Retry to be shown its check, or \
                open the mod on mxb-mods.com in your browser."
            .to_string(),
        429 => "mxb-mods.com is rate-limiting us (429). Give it a minute, then hit Retry."
            .to_string(),
        503 => "mxb-mods.com is unavailable right now (503) — it may be behind a Cloudflare \
                check. Try again shortly."
            .to_string(),
        _ => format!("mxb-mods.com returned {status}"),
    };
    let message = match ray {
        Some(ray) if !ray.is_empty() && ray != "-" => format!("{message}\n\nRef: {ray}"),
        _ => message,
    };
    anyhow::Error::new(Blocked {
        status: Some(status),
        message,
    })
}

/// Everything about a refused response that is worth having in a user's log, written once,
/// then turned into the error the UI shows.
///
/// Takes the whole `Response` rather than its status because the interesting parts are the
/// bits [`blocked_error`] never saw: the `cf-ray` that identifies the block, `cf-mitigated`
/// which says *how* Cloudflare decided, the cookies we actually sent, and the error body —
/// Cloudflare puts the block reason in there ("Sorry, you have been blocked", `Error 1015`),
/// and we were dropping it on the floor.
fn refusal(what: &str, resp: &Fetched) -> anyhow::Error {
    // Just the path: the rest is a constant prefix plus percent-encoded query noise.
    let path = resp
        .url
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map_or_else(|| resp.url.clone(), |(_, p)| format!("/{p}"));
    let path = path.split('?').next().unwrap_or(&path).to_string();
    let ray = resp.header("cf-ray").unwrap_or("-").to_string();
    log::warn!(
        "{}",
        refusal_line(
            what,
            &path,
            resp.status,
            &resp.headers,
            &mxb_session::jar_summary(),
            &resp.body,
        )
    );
    blocked_error(resp.status, Some(&ray))
}

/// The line itself, built separately from the logging so a test can pin what a user ends up
/// pasting into Discord — one line, every field a diagnosis needs, no cookie values.
fn refusal_line(
    what: &str,
    path: &str,
    status: u16,
    headers: &HashMap<String, String>,
    jar: &str,
    body: &str,
) -> String {
    let h = |name: &str| headers.get(name).map_or("-", String::as_str);
    format!(
        "mxb-mods refused {what} ({path}, {status}) — cf-ray={} cf-mitigated={} server={} \
         retry-after={}; jar: {jar}; body: {}",
        h("cf-ray"),
        h("cf-mitigated"),
        h("server"),
        h("retry-after"),
        snippet(body)
    )
}

/// A response body cut down to something that belongs on one log line: whitespace collapsed
/// (Cloudflare's block pages are mostly indentation) and truncated.
fn snippet(body: &str) -> String {
    const MAX: usize = 500;
    if body.is_empty() {
        return "(empty)".to_string();
    }
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX {
        return format!("\"{flat}\"");
    }
    let cut: String = flat.chars().take(MAX).collect();
    format!("\"{cut}…\" ({} bytes total)", body.len())
}

/// A 200 whose body is not the JSON we asked for. Without this the user saw serde's
/// "expected value at line 1 column 1", which names nothing they can act on. A Cloudflare
/// page is reported as the check it is; anything else (a firewall, antivirus or captive-portal
/// page standing in for the site) says so and points at Retry.
fn not_json_error(body: &str, err: &serde_json::Error) -> anyhow::Error {
    if let Some(marker) = challenge_marker(body) {
        return challenge_error(marker, body.len());
    }
    log::warn!(
        "mxb-mods answered 200 with a body that is not JSON ({err}): {}",
        snippet(body)
    );
    anyhow::anyhow!(
        "mxb-mods.com sent back something that isn't a mod listing. A firewall, antivirus, VPN          or a network login page may be intercepting the connection. Check your connection,          then hit Retry."
    )
}

/// Whether a 400 from the listing means "past the last page". WordPress answers
/// `rest_post_invalid_page_number` that way, but only for a page after the first: page 1 of
/// an empty category is a 200 with `[]`, so a 400 there is a real refusal that must reach the
/// user rather than render as an empty grid.
fn is_end_of_listing(status: u16, page: &Page) -> bool {
    status == 400
        && match page {
            Page::Number(n) => *n > 1,
            Page::Offset { skip, .. } => *skip > 0,
        }
}

/// The interstitial-as-a-200 case. Same handling as a 403 — it is the same refusal, just
/// dressed as a success.
fn challenge_error(marker: &str, html_len: usize) -> anyhow::Error {
    log::warn!(
        "mxb-mods served a Cloudflare interstitial as a 200 (matched '{marker}', {html_len} bytes); \
         jar: {}",
        mxb_session::jar_summary()
    );
    anyhow::Error::new(Blocked {
        status: None,
        message: "mxb-mods.com served a Cloudflare check instead of the mod page. Hit Retry \
                  to be shown the check, or open the mod on mxb-mods.com in your browser."
            .to_string(),
    })
}

/// A Cloudflare interstitial served with a 200, which would otherwise parse as an empty
/// page — the quiet failure behind "No download link was found on this page".
///
/// Deliberately *not* keyed on `challenge-platform`: Cloudflare injects that script into
/// ordinary pages too (verified — it appears once in a normal 212 KB mod page that parses
/// fine), so matching it would condemn every mod page. The markers below only appear on
/// the interstitial itself.
///
/// Returns *which* marker matched, so the log says why we called a 200 a challenge — the next
/// time Cloudflare reworks its interstitial, that is the difference between "the markers are
/// stale" and "the page really was a challenge".
fn challenge_marker(html: &str) -> Option<&'static str> {
    if html.contains("cf-browser-verification") {
        Some("cf-browser-verification")
    } else if html.contains("cf_chl_opt") {
        Some("cf_chl_opt")
    } else if html.to_ascii_lowercase().contains("<title>just a moment") {
        Some("<title>just a moment")
    } else {
        None
    }
}

/// A post asked for by its slug.
///
/// The site's search is a relevance ranker over titles and bodies, and it does not always
/// reach a short name: `fort red` comes back as TheBackForty, Silver Rock and The Redman's,
/// while https://mxb-mods.com/fort-red/ exists and is exactly the post wanted. A server's
/// track id is the folder name inside the `.pkz` and reads like a slug already, so asking
/// WordPress for that slug is both cheaper than a search and right more often.
pub async fn by_slug(slug: &str, category_id: u32) -> anyhow::Result<Option<ModSummary>> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Ok(None);
    }
    // The official API first (mxb-mods only). "No such mod" is its answer and stands; only a
    // failure sends the lookup to the site, once.
    if mxbmods_api::active() {
        match mxbmods_api::api().by_slug(slug).await {
            Ok(None) => return Ok(None),
            Ok(Some(full)) => return Ok(slug_hit(&full, category_id)),
            Err(e) => log::info!("slug lookup via the mxb-mods API failed ({e:#}); asking the site"),
        }
    }
    by_slug_site(slug, category_id).await
}

/// A full API record as the answer to a slug lookup scoped to `category_id`: only when the
/// mod is filed under that category, which is what `categories=` did on wp/v2.
fn slug_hit(full: &Value, category_id: u32) -> Option<ModSummary> {
    mxbmods_api::category_ids(full)
        .contains(&u64::from(category_id))
        .then(|| mxbmods_api::summary(full, category_id))
        .flatten()
}

async fn by_slug_site(slug: &str, category_id: u32) -> anyhow::Result<Option<ModSummary>> {
    let url = format!("{}{}", mxb_session::base(), obfstr!("/wp-json/wp/v2/posts"));
    let params = vec![
        ("slug", slug.to_string()),
        ("categories", category_id.to_string()),
        ("_embed", "author,wp:featuredmedia".to_string()),
    ];
    let resp = get_with_retry(&url, &params).await?;
    let v: Value = resp.json()?;
    Ok(v.as_array()
        .and_then(|posts| posts.first())
        .and_then(|p| summary_from_post(p, category_id)))
}

pub async fn search(
    query: &str,
    category_id: u32,
    page: u32,
    sort: ModSort,
) -> anyhow::Result<Vec<ModSummary>> {
    let q = query.trim();
    // The popular listings come from a plugin endpoint that has no search of its own, so
    // a typed query always means the plain catalog. The UI hides those options while the
    // box has text in it; this is the backstop.
    let sort = if q.is_empty() || sort.popular_range().is_none() {
        sort
    } else {
        ModSort::Newest
    };
    let page = page.max(1);

    if mxbmods_api::active() {
        let api = mxbmods_api::api();
        // Popular has no fallback: the site's popular-posts endpoint is gone, and asking it
        // anyway is the traffic mxb-mods.com asked us to stop.
        if let Some(range) = sort.api_range() {
            return api.popular(range, category_id, page).await.map_err(|e| {
                log::warn!("popular listing unavailable: {e:#}");
                anyhow::anyhow!(mxbmods_api::POPULAR_UNAVAILABLE)
            });
        }
        match api.mods(q, category_id, sort, page).await {
            Ok(items) => return Ok(items),
            // Once, to the site, which has its own cooldown.
            Err(e) => log::info!("listing via the mxb-mods API failed ({e:#}); asking the site"),
        }
    }
    search_site(q, category_id, page, sort).await
}

/// The site's own listing, which already falls back to the WebView when the HTTP client is refused.
async fn search_site(
    q: &str,
    category_id: u32,
    page: u32,
    sort: ModSort,
) -> anyhow::Result<Vec<ModSummary>> {
    let listing_fut = async {
        match sort {
            ModSort::Newest => listing(q, category_id, Page::Number(page)).await,
            ModSort::Oldest => oldest(q, category_id, page).await,
            // `popular_range` is Some for every remaining variant.
            _ => popular(category_id, page, sort.popular_range().unwrap_or("all")).await,
        }
    };

    // The site publishes no author through its API — see `warm_feed_authors`. A date-ordered
    // listing asks the feed for them *while* the listing is being fetched rather than after
    // it: the feed pages depend only on which page was asked for, and waiting for them one
    // after another behind the listing is what made "Load more" slow. Not for mxb-mods,
    // whose official API names authors: this path is only its fallback, and the feed would be
    // two more requests to a site that is already failing us.
    let items = if matches!(sort, ModSort::Newest) && !mxbmods_api::active() {
        let (items, ()) = futures_util::join!(listing_fut, warm_feed_authors(category_id, q, page));
        items
    } else {
        listing_fut.await
    };

    // Every listing gets whatever the feed has already taught us.
    let mut items = items?;
    apply_known_authors(&mut items);
    Ok(items)
}

/// Which slice of a listing to ask for. WP accepts either, and `Offset` is what makes
/// oldest-first possible — see [`oldest`].
enum Page {
    Number(u32),
    Offset { skip: u32, take: u32 },
}

/// One page of the catalog in the site's own order (newest first, near enough).
async fn listing(q: &str, category_id: u32, page: Page) -> anyhow::Result<Vec<ModSummary>> {
    let (resp, _) = listing_response(q, category_id, &page).await?;
    // WP returns 400 (rest_post_invalid_page_number) once you page past the end.
    if is_end_of_listing(resp.status, &page) {
        log::info!(
            "catalog returned 400 — treating as the end of the listing: {}",
            snippet(&resp.body)
        );
        return Ok(vec![]);
    }
    if !resp.is_success() {
        return Err(refusal("the catalog listing", &resp));
    }
    let posts: Vec<Value> = resp.json()?;
    Ok(posts
        .iter()
        .filter_map(|p| summary_from_post(p, category_id))
        .collect())
}

/// The raw response plus the total post count WP reports in `X-WP-Total`.
async fn listing_response(
    q: &str,
    category_id: u32,
    page: &Page,
) -> anyhow::Result<(Fetched, Option<u32>)> {
    let url = format!("{}{}", mxb_session::base(), obfstr!("/wp-json/wp/v2/posts"));
    let mut params: Vec<(&str, String)> = vec![
        ("categories", category_id.to_string()),
        // `author` rides along so a card can carry a byline; it costs no extra request.
        ("_embed", "author,wp:featuredmedia".to_string()),
    ];
    match page {
        Page::Number(n) => {
            params.push(("page", n.to_string()));
            params.push(("per_page", PER_PAGE.to_string()));
        }
        Page::Offset { skip, take } => {
            params.push(("offset", skip.to_string()));
            params.push(("per_page", take.to_string()));
        }
    }
    if !q.is_empty() {
        params.push(("search", q.to_string()));
    }
    let resp = get_with_retry(&url, &params).await?;
    let total = resp.header("x-wp-total").and_then(|v| v.parse::<u32>().ok());
    Ok((resp, total))
}

/// Oldest first.
///
/// `orderby=date&order=asc` is ignored by the site, but `offset` is honoured — so we walk
/// the same fixed listing backwards: page 1 is the last `PER_PAGE` posts, reversed. The
/// final page is the short one, which is what the caller's "is there more?" check keys on.
async fn oldest(q: &str, category_id: u32, page: u32) -> anyhow::Result<Vec<ModSummary>> {
    let per_page: u32 = PER_PAGE.parse().unwrap_or(24);
    let total = total_count(q, category_id).await?;
    let taken = (page - 1).saturating_mul(per_page);
    if taken >= total {
        return Ok(vec![]);
    }
    // How far from the end this page starts; the last one is short when the count doesn't
    // divide evenly, and saturating at 0 is exactly that case.
    let remaining = total - taken;
    let take = remaining.min(per_page);
    let skip = remaining - take;

    let mut posts = listing(q, category_id, Page::Offset { skip, take }).await?;
    posts.reverse();
    Ok(posts)
}

/// How long a listing's post count is trusted. New mods appear a few times a day, and
/// being one behind only shifts the oldest page by a slot.
const TOTAL_TTL: Duration = Duration::from_secs(300);

fn total_cache() -> &'static Mutex<HashMap<(u32, String), (u32, Instant)>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<(u32, String), (u32, Instant)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How many posts a category (optionally narrowed by a search) holds, from `X-WP-Total`.
/// Cached, because every "load more" under the oldest sort needs it again.
async fn total_count(q: &str, category_id: u32) -> anyhow::Result<u32> {
    let key = (category_id, q.to_string());
    if let Some((total, at)) = lock(total_cache()).get(&key) {
        if at.elapsed() < TOTAL_TTL {
            return Ok(*total);
        }
    }
    // One post is enough to read the header off; the body is discarded.
    let (resp, total) = listing_response(q, category_id, &Page::Offset { skip: 0, take: 1 }).await?;
    if !resp.is_success() {
        return Err(refusal("the catalog post count", &resp));
    }
    let total = total.ok_or_else(|| anyhow::anyhow!("the catalog didn't report a post count"))?;
    lock(total_cache()).insert(key, (total, Instant::now()));
    Ok(total)
}

/// One page of a category ranked by views, from the site's popular-posts plugin.
///
/// It hands back ordinary post objects, so the same parser reads them — the only
/// differences are `limit`/`offset` instead of `page`, and that paging past the end
/// returns an empty list rather than a 400.
async fn popular(category_id: u32, page: u32, range: &str) -> anyhow::Result<Vec<ModSummary>> {
    // gpb-mods.com only. mxb-mods.com removed the plugin; its popular list is the API's.
    if mxbmods_api::active() {
        anyhow::bail!(mxbmods_api::POPULAR_UNAVAILABLE);
    }
    let url = format!(
        "{}{}",
        mxb_session::base(),
        obfstr!("/wp-json/wordpress-popular-posts/v1/popular-posts")
    );
    let per_page: u32 = PER_PAGE.parse().unwrap_or(24);
    let params: Vec<(&str, String)> = vec![
        ("taxonomy", "category".to_string()),
        ("term_id", category_id.to_string()),
        ("range", range.to_string()),
        ("order_by", "views".to_string()),
        ("limit", per_page.to_string()),
        ("offset", (page.saturating_sub(1) * per_page).to_string()),
        ("_embed", "author,wp:featuredmedia".to_string()),
    ];

    let resp = get_with_retry(&url, &params).await?;
    if !resp.is_success() {
        return Err(refusal("the popular listing", &resp));
    }
    let posts: Vec<Value> = resp.json()?;
    Ok(posts
        .iter()
        .filter_map(|p| summary_from_post(p, category_id))
        .collect())
}

/// Everything a mod page shows that is not read off the rendered page.
struct PostMeta {
    id: u64,
    title: String,
    link: String,
    date: String,
    description_html: String,
    images: Vec<String>,
    categories: Vec<String>,
    /// The API's author, used when the page carries no byline.
    author: Option<String>,
}

/// The metadata from the official API (mxb-mods only). `Ok(None)` when it has no such mod,
/// and an error when it couldn't answer; both send the caller to the site's own REST API.
async fn detail_meta_api(slug: &str) -> anyhow::Result<Option<PostMeta>> {
    let api = mxbmods_api::api();
    let Some(full) = api.by_slug(slug).await? else {
        return Ok(None);
    };
    let m = mxbmods_api::parse_meta(&full).ok_or_else(|| anyhow::anyhow!("unreadable mod record"))?;
    // Names, not ids: the install picker ranks destinations by them. Cached for the session.
    let names = api.categories().await?;
    let mut images = m.images.clone();
    if images.is_empty() {
        images.extend(extract_images(&m.content_html));
    }
    if images.is_empty() {
        images.extend(m.image.clone());
    }
    dedup(&mut images);
    let mut categories: Vec<String> = m
        .category_ids
        .iter()
        .filter_map(|id| names.get(id).cloned())
        .collect();
    dedup(&mut categories);
    Ok(Some(PostMeta {
        id: m.id,
        title: m.title,
        link: m.link,
        date: m.date,
        description_html: description_from(&m.content_html),
        images,
        categories,
        author: m.author,
    }))
}

pub async fn detail(slug: &str) -> anyhow::Result<ModDetail> {
    // 1. Metadata: the official API where there is one, the site's REST API otherwise (or
    //    when the API couldn't answer).
    let api_meta = if mxbmods_api::active() {
        match detail_meta_api(slug).await {
            Ok(Some(m)) => Some(m),
            Ok(None) => {
                log::info!("the mxb-mods API has no '{slug}'; asking the site");
                None
            }
            Err(e) => {
                log::info!("mod metadata via the mxb-mods API failed ({e:#}); asking the site");
                None
            }
        }
    } else {
        None
    };
    let meta = match api_meta {
        Some(m) => m,
        None => detail_meta_site(slug).await?,
    };
    let PostMeta {
        id,
        title,
        link,
        date,
        description_html,
        mut images,
        categories,
        author: api_author,
    } = meta;

    // 2. Download links + version from the rendered page HTML. A block here used to be
    // swallowed: a 403 is `Ok(resp)`, so its error body went to the parsers and produced
    // zero downloads — the page then said "No download link was found on this page"
    // rather than "we couldn't read the page". Surface it instead.
    let resp = get_page(&link).await?;
    if !resp.is_success() {
        return Err(refusal("the mod page", &resp));
    }
    let html = resp.body;
    let (downloads, version) = (parse_downloads(&html), parse_version(&html));
    let author = parse_author(&html);
    // A post whose only picture is a featured image the REST API won't hand over
    // (`rest_forbidden`) still names it on the page, so the gallery isn't left empty.
    if images.is_empty() {
        images.extend(parse_og_image(&html));
    }
    // Only call it a challenge when the page also yielded nothing, so a marker that turns
    // up in a page that actually parsed can never turn a working mod into an error.
    if downloads.is_empty() {
        if let Some(marker) = challenge_marker(&html) {
            return Err(challenge_error(marker, html.len()));
        }
        log::info!(
            "no downloads parsed from '{slug}' ({} bytes) and it is not a challenge page",
            html.len()
        );
    }

    let (author, author_url) = match author {
        Some((name, url)) => (Some(name), url),
        None => (api_author, None),
    };
    Ok(ModDetail {
        id,
        slug: slug.to_string(),
        title,
        link,
        date,
        description_html,
        images,
        version,
        author,
        author_url,
        downloads,
        categories,
    })
}

/// The metadata from the site's own REST API (gpb-mods, and mxb-mods' fallback).
async fn detail_meta_site(slug: &str) -> anyhow::Result<PostMeta> {
    let url = format!("{}{}", mxb_session::base(), obfstr!("/wp-json/wp/v2/posts"));
    // `wp:term` rides along in the same request — the categories it brings back are what
    // tell the install picker which bike a livery is for.
    let params = vec![
        ("slug", slug.to_string()),
        ("_embed", "wp:featuredmedia,wp:term".to_string()),
    ];
    let resp = get_with_retry(&url, &params).await?;
    if !resp.is_success() {
        return Err(refusal("mod metadata", &resp));
    }
    let posts: Vec<Value> = resp.json()?;
    let post = posts
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("mod not found: {slug}"))?;

    let id = post.get("id").and_then(Value::as_u64).unwrap_or(0);
    let title = decode_entities(rendered(&post, "title"));
    let link = post
        .get("link")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let date = post
        .get("date")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let content = rendered(&post, "content").to_string();

    let mut images = Vec::new();
    if let Some(feat) = featured_image(&post) {
        images.push(feat);
    }
    images.extend(extract_images(&content));
    dedup(&mut images);

    let description_html = description_from(&content);

    Ok(PostMeta {
        id,
        title,
        link,
        date,
        description_html,
        images,
        categories: term_names(&post),
        author: None,
    })
}

/// Scores for a batch of post ids. Never an error: a rating decorates a card, so a blocked
/// or slow request must never surface as a browsing error.
///
/// mxb-mods.com: one batched call per hundred ids to its official API, cached, failures
/// included — see [`mxbmods_api::Api::ratings`]. gpb-mods.com: its admin-ajax endpoint, see
/// [`site_ratings`].
pub async fn ratings(ids: &[u64]) -> HashMap<u64, ModRating> {
    if mxbmods_api::active() {
        mxbmods_api::api().ratings(ids).await
    } else {
        site_ratings(ids).await
    }
}

/// How long a fetched rating is trusted. Votes trickle in over days, so a stale score is
/// harmless — but re-browsing a category shouldn't re-ask the site for the same 24 posts.
const RATING_TTL: Duration = Duration::from_secs(10 * 60);

/// How long a rating that failed is left alone: the same id is not asked again for this long.
const RATING_FAILURE_TTL: Duration = Duration::from_secs(10 * 60);

/// The site's ratings cost one request each, so a page of results is a burst. Two at a time,
/// and the batch stops at the first failure.
const RATING_CONCURRENCY: usize = 2;

/// Per id: the score (or `None` for one that failed) and when the entry expires.
fn site_rating_cache() -> &'static Mutex<HashMap<u64, (Option<ModRating>, Instant)>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<u64, (Option<ModRating>, Instant)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Parked after a failed rating, so the rest of the batch (and the next one) send nothing.
fn site_ratings_gate() -> &'static Mutex<Gate> {
    static GATE: Mutex<Gate> = Mutex::new(Gate::new(mxbmods_api::FIRST_STEP, mxbmods_api::MAX_STEP));
    &GATE
}

/// gpb-mods.com's scores, one admin-ajax request per id, two at a time.
///
/// A failure parks [`site_ratings_gate`]: requests not yet sent are dropped, and the id that
/// failed is not asked again for [`RATING_FAILURE_TTL`]. Nothing is retried on the spot.
async fn site_ratings(ids: &[u64]) -> HashMap<u64, ModRating> {
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    {
        let cache = lock(site_rating_cache());
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
    if missing.is_empty() || !lock(site_ratings_gate()).open(Instant::now()) {
        return out;
    }

    let fetched: Vec<(u64, Option<ModRating>)> = futures_util::stream::iter(missing.into_iter().map(|id| async move {
        if !lock(site_ratings_gate()).open(Instant::now()) {
            return None;
        }
        match rating(id).await {
            Ok(r) => {
                lock(site_ratings_gate()).succeed();
                Some((id, Some(r)))
            }
            Err(e) => {
                let wait = lock(site_ratings_gate()).fail(Instant::now(), None);
                log::info!("rating for {id} failed ({e:#}); no more ratings for {wait:?}");
                Some((id, None))
            }
        }
    }))
    .buffer_unordered(RATING_CONCURRENCY)
    .filter_map(|r| async move { r })
    .collect()
    .await;

    {
        let mut cache = lock(site_rating_cache());
        let now = Instant::now();
        for (id, r) in &fetched {
            let ttl = if r.is_some() { RATING_TTL } else { RATING_FAILURE_TTL };
            cache.insert(*id, (*r, now + ttl));
            if let Some(r) = r {
                out.insert(*id, *r);
            }
        }
    }
    out
}

/// A poisoned rating cache is not worth taking the app down for — the worst case is one
/// stale entry written by a thread that panicked mid-insert.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The rating plugin has no REST route; its front end reads scores from this admin-ajax
/// action, which answers unauthenticated with `{voteCount, avgRating}`. gpb-mods.com only:
/// mxb-mods.com's scores come from its API.
async fn rating(id: u64) -> anyhow::Result<ModRating> {
    let cooldown = site_cooldown();
    if let Some(e) = cooling_down(cooldown) {
        return Err(e);
    }
    let url = format!("{}{}", mxb_session::base(), obfstr!("/wp-admin/admin-ajax.php"));
    let form = [
        ("action", "load_results".to_string()),
        ("postID", id.to_string()),
    ];
    // Ratings go over whichever transport the session settled on too — otherwise a blocked
    // user gets their mods back but every card silently loses its stars.
    let resp = if on_webview() {
        crate::mxb_fetch::post(&url, &form).await?
    } else {
        let req = client()?
            .post(&url)
            .header(reqwest::header::USER_AGENT, mxb_session::ua())
            .form(&form);
        into_fetched(req.send().await?).await
    };
    if matches!(resp.status, 429 | 503) {
        note_throttled(cooldown, resp.status, mxbmods_api::retry_after(resp.header("retry-after")));
    }
    if !resp.is_success() {
        anyhow::bail!("{}", resp.status);
    }
    let v: Value = resp.json()?;
    let average = number(v.get("avgRating")).unwrap_or(0.0);
    Ok(ModRating {
        // A NaN would reach the UI as `null` and break the stars.
        average: if average.is_finite() { average as f32 } else { 0.0 },
        count: number(v.get("voteCount")).unwrap_or(0.0).max(0.0) as u32,
    })
}

/// WordPress plugins are casual about JSON types — the same field comes back as a number
/// on one post and a string on another.
fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn summary_from_post(p: &Value, category_id: u32) -> Option<ModSummary> {
    let id = p.get("id")?.as_u64()?;
    let slug = p.get("slug")?.as_str()?.to_string();
    Some(ModSummary {
        id,
        slug,
        title: decode_entities(rendered(p, "title")),
        link: p.get("link").and_then(Value::as_str).unwrap_or("").to_string(),
        date: p.get("date").and_then(Value::as_str).unwrap_or("").to_string(),
        image: card_image(p),
        category_id,
        author: embedded_author(p),
    })
}

/// The card's picture: the featured image, else the first picture in the post.
///
/// Some posts' featured media answers `rest_forbidden` inside `_embed`, so the card had no
/// picture at all while the detail page, which also reads the content, showed a full gallery.
fn card_image(p: &Value) -> Option<String> {
    featured_image(p).or_else(|| {
        extract_images(rendered(p, "content"))
            .into_iter()
            .find(|u| u.starts_with("https://") && is_image_url(u))
    })
}

/// Post link to the name the site credits it to, learned from the feed and kept for the
/// session. Authors do not change, and a mod seen once should keep its byline while the
/// catalog is browsed around it.
static FEED_AUTHORS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, String>>> =
    std::sync::OnceLock::new();

/// Posts the feed hands back per page. WordPress's own default, and not the same as
/// [`PER_PAGE`] — which is why a listing needs two feed pages to cover itself.
const FEED_PER_PAGE: u32 = 20;

/// Feed pages that have already been read into [`FEED_AUTHORS`], so paging back over a
/// listing (or a cached refresh) costs no request at all.
static FEED_PAGES_DONE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

/// How long the feed may take before the listing is returned without bylines. They decorate
/// a card; a slow feed must not hold the cards back.
const FEED_BUDGET: Duration = Duration::from_secs(4);

/// The feed pages (first, last) that hold the posts of listing page `page`.
fn feed_pages_for(page: u32, per: u32, feed_per: u32) -> (u32, u32) {
    let first = page.saturating_sub(1) * per;
    let from = first / feed_per + 1;
    let to = (first + per).div_ceil(feed_per);
    (from, to.max(from))
}

/// Give each item the byline the feed has taught us, if any.
fn apply_known_authors(items: &mut [ModSummary]) {
    let cache = FEED_AUTHORS.get_or_init(Default::default);
    if let Ok(map) = cache.lock() {
        for m in items.iter_mut() {
            if m.author.is_none() {
                m.author = map.get(&m.link).cloned();
            }
        }
    }
}

/// Learn the bylines the API refuses to give.
///
/// mxb-mods.com has closed everything that would name an author: `_embed=author` answers
/// `rest_user_invalid_id`, the users endpoint is shut, `class_list` carries no author class,
/// and `/?author=<id>` is a 500. What it does still publish is its RSS feed, and every item
/// in that carries a `dc:creator`.
///
/// So the feed is the source: one or two requests per page of results rather than one per
/// card, matched on the post's own permalink, and remembered. The pages are requested
/// together and within [`FEED_BUDGET`], and this runs beside the listing request — see
/// [`search`]. Best effort throughout: a feed that fails or is slow leaves the cards as they
/// were before this existed.
async fn warm_feed_authors(category_id: u32, q: &str, page: u32) {
    let cache = FEED_AUTHORS.get_or_init(Default::default);
    let done = FEED_PAGES_DONE.get_or_init(Default::default);

    // Which feed pages hold the posts this listing is showing.
    let per: u32 = PER_PAGE.parse().unwrap_or(24);
    let (from, to) = feed_pages_for(page, per, FEED_PER_PAGE);

    let fetches = (from..=to).filter_map(|m| {
        let key = format!("{category_id}|{q}|{m}");
        if done.lock().map(|d| d.contains(&key)).unwrap_or(false) {
            return None;
        }
        let mut params: Vec<(&str, String)> = vec![("paged", m.to_string())];
        if category_id != 0 {
            params.push(("cat", category_id.to_string()));
        }
        if !q.is_empty() {
            params.push(("s", q.to_string()));
        }
        // Built into the URL rather than passed alongside it: the webview path reads a page
        // by URL alone and would drop them.
        let base = format!("{}{}", mxb_session::base(), obfstr!("/feed/"));
        let url = reqwest::Url::parse_with_params(&base, &params).ok()?;
        Some(async move {
            let resp = get_page(url.as_str()).await.ok()?;
            match feed_page_outcome(resp.status) {
                FeedPage::Items => Some((key, feed_creators(&resp.body))),
                FeedPage::PastTheEnd => Some((key, Vec::new())),
                FeedPage::Failed => None,
            }
        })
    });

    let all = futures_util::future::join_all(fetches);
    let Ok(results) = tokio::time::timeout(FEED_BUDGET, all).await else {
        return;
    };
    for (key, creators) in results.into_iter().flatten() {
        if let Ok(mut map) = cache.lock() {
            for (link, name) in creators {
                map.insert(link, name);
            }
        }
        if let Ok(mut d) = done.lock() {
            d.insert(key);
        }
    }
}

/// What a feed page's status says.
#[derive(Debug, PartialEq, Eq)]
enum FeedPage {
    Items,
    /// WordPress answers a feed page beyond the last one with a 404. That is the end of the
    /// feed, not a failure: remember it, or every visit to the last listing page asks again.
    PastTheEnd,
    Failed,
}

fn feed_page_outcome(status: u16) -> FeedPage {
    match status {
        200..=299 => FeedPage::Items,
        404 => FeedPage::PastTheEnd,
        _ => FeedPage::Failed,
    }
}

/// `(permalink, creator)` for each item of an RSS feed.
///
/// Read with a scanner rather than an XML parser: the shape is two known tags inside
/// `<item>`, and the crate that reads the site's HTML is a *HTML* parser, which would mangle
/// this. Anything it does not recognise is skipped rather than guessed at.
fn feed_creators(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in xml.split("<item>").skip(1) {
        let item = item.split("</item>").next().unwrap_or_default();
        let tag = |name: &str| -> Option<String> {
            let open = format!("<{name}>");
            let body = item.split(&open).nth(1)?.split(&format!("</{name}>")).next()?;
            let body = body
                .trim()
                .trim_start_matches("<![CDATA[")
                .trim_end_matches("]]>")
                .trim();
            (!body.is_empty()).then(|| decode_entities(body))
        };
        if let (Some(link), Some(name)) = (tag("link"), tag("dc:creator")) {
            out.push((link, name));
        }
    }
    out
}

/// The post author's display name, from `_embed=author`.
///
/// Optional on purpose. WordPress answers the embed with an error object rather than a user
/// when the site keeps its author list private, and a catalog that stops naming anyone is no
/// reason for a listing to fail — the card simply doesn't show a byline.
fn embedded_author(p: &Value) -> Option<String> {
    let name = p
        .get("_embedded")?
        .get("author")?
        .as_array()?
        .first()?
        .get("name")?
        .as_str()?;
    let name = decode_entities(name).trim().to_string();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod feed_tests {
    use super::{feed_creators, feed_page_outcome, feed_pages_for, FeedPage};

    #[test]
    fn a_feed_404_is_the_end_not_a_failure() {
        assert_eq!(feed_page_outcome(200), FeedPage::Items);
        assert_eq!(feed_page_outcome(404), FeedPage::PastTheEnd);
        assert_eq!(feed_page_outcome(403), FeedPage::Failed);
        assert_eq!(feed_page_outcome(503), FeedPage::Failed);
    }

    #[test]
    fn feed_pages_cover_a_listing_page() {
        // 24 per listing page over a 20-per-page feed: page 1 is feed pages 1-2, page 2 is
        // posts 24..48 so feed pages 2-3, page 3 is 48..72 so feed pages 3-4.
        assert_eq!(feed_pages_for(1, 24, 20), (1, 2));
        assert_eq!(feed_pages_for(2, 24, 20), (2, 3));
        assert_eq!(feed_pages_for(3, 24, 20), (3, 4));
        // Page 0 must not underflow.
        assert_eq!(feed_pages_for(0, 24, 20), (1, 2));
    }

    /// The shape the site's own feed comes in, down to the CDATA and the entity.
    const FEED: &str = r#"<rss><channel>
<item>
  <title><![CDATA[LATROBE SPEEDWAY RD.1]]></title>
  <link>https://mxb-mods.com/latrobe-speedway-rd-1/</link>
  <dc:creator><![CDATA[S33VERSMX]]></dc:creator>
</item>
<item>
  <title><![CDATA[ID Worx &#8211; Factoryburn]]></title>
  <link>https://mxb-mods.com/id-worx-factoryburn/</link>
  <dc:creator><![CDATA[dryftzz]]></dc:creator>
</item>
<item>
  <title><![CDATA[No byline on this one]]></title>
  <link>https://mxb-mods.com/anon/</link>
</item>
</channel></rss>"#;

    #[test]
    fn creators_come_out_paired_with_their_post() {
        let got = feed_creators(FEED);
        assert_eq!(
            got,
            vec![
                ("https://mxb-mods.com/latrobe-speedway-rd-1/".to_string(), "S33VERSMX".to_string()),
                ("https://mxb-mods.com/id-worx-factoryburn/".to_string(), "dryftzz".to_string()),
            ],
            "an item with no creator is skipped rather than guessed at"
        );
    }

    #[test]
    fn nothing_that_isnt_a_feed_yields_anything() {
        assert!(feed_creators("").is_empty());
        assert!(feed_creators("<html><body>refused</body></html>").is_empty());
        // A truncated item — a cut-off response — must not panic or half-read.
        assert!(feed_creators("<item><link>https://x/</link>").is_empty());
    }
}

/// `post[field]["rendered"]` as a &str.
fn rendered<'a>(post: &'a Value, field: &str) -> &'a str {
    post.get(field)
        .and_then(|v| v.get("rendered"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn featured_image(p: &Value) -> Option<String> {
    p.get("_embedded")?
        .get("wp:featuredmedia")?
        .as_array()?
        .first()?
        .get("source_url")?
        .as_str()
        .map(str::to_string)
}

/// Every term name `_embed=wp:term` brought back, flattened across taxonomies.
///
/// The site files a mod under one category per bike it fits ("2023 KTM 450 SX-F OEM") on top
/// of the browse categories ("Liveries") and the manufacturer ("KTM"), so the caller gets a
/// mixed bag and decides what's distinctive. Order follows the site's own.
fn term_names(post: &Value) -> Vec<String> {
    let mut out: Vec<String> = post
        .get("_embedded")
        .and_then(|e| e.get("wp:term"))
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .filter_map(Value::as_array)
                .flatten()
                .filter_map(|t| t.get("name").and_then(Value::as_str))
                .map(decode_entities)
                .collect()
        })
        .unwrap_or_default();
    dedup(&mut out);
    out
}

fn decode_entities(s: &str) -> String {
    html_escape::decode_html_entities(s).into_owned()
}

fn is_image_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_lowercase();
    [".jpg", ".jpeg", ".png", ".webp", ".gif"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// Prefer full-res links (`<a href="…full.webp">`), falling back to `<img src>`.
pub(crate) fn extract_images(content_html: &str) -> Vec<String> {
    let doc = Html::parse_fragment(content_html);
    let a_sel = Selector::parse("a[href]").unwrap();
    let img_sel = Selector::parse("img[src]").unwrap();

    let mut out: Vec<String> = doc
        .select(&a_sel)
        .filter_map(|el| el.value().attr("href"))
        .filter(|h| is_image_url(h))
        .map(str::to_string)
        .collect();

    if out.is_empty() {
        out = doc
            .select(&img_sel)
            .filter_map(|el| el.value().attr("src"))
            .map(str::to_string)
            .collect();
    }
    out
}

/// A post body as the mod page shows it: images out (they're in the gallery), then the same
/// allowlist sanitiser the shop uses. The body comes from mxb-mods.com and is rendered as
/// HTML in the main window, so it is never handed over unsanitised.
pub(crate) fn description_from(content: &str) -> String {
    super::shop_catalog::sanitize_html(&strip_images(content)).unwrap_or_default()
}

/// Remove image markup from the description (images are shown in the gallery).
pub(crate) fn strip_images(html: &str) -> String {
    let a_img = Regex::new(r"(?is)<a\b[^>]*>\s*<img\b[^>]*>\s*</a>").unwrap();
    let img = Regex::new(r"(?is)<img\b[^>]*>").unwrap();
    let s = a_img.replace_all(html, "");
    img.replace_all(&s, "").into_owned()
}

/// The file name a download URL ends in — where an author who didn't label the block
/// often still says what the file is (`…/Track_Server.zip`).
///
/// MediaFire and Google Drive hang a routing segment off the end of the path
/// (`…/track.zip/file`, `…/view`), so those are stepped over to reach the real name.
fn url_file_name(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let mut segs = path.trim_end_matches('/').rsplit('/').filter(|s| !s.is_empty());
    let last = segs.next().unwrap_or("");
    if ["file", "view", "download"].iter().any(|s| last.eq_ignore_ascii_case(s)) {
        segs.next().unwrap_or(last)
    } else {
        last
    }
}

/// Does this text call something a server build?
///
/// The word has to stand on its own: a track named "Observer Hill" contains the letters
/// but isn't a server file, and mistaking one for the other hides the only download a mod
/// has. Letters are what separate them rather than `\b`, because `_` is a word character
/// to a regex and `Ironman_2024_Server.pkz` is how these files are actually named.
fn mentions_server(text: &str) -> bool {
    Regex::new(r"(?i)(?:^|[^a-z])servers?(?:[^a-z]|$)")
        .unwrap()
        .is_match(text)
}

/// Parse the theme's `div.download-container` blocks into download options.
fn parse_downloads(html: &str) -> Vec<DownloadOption> {
    let doc = Html::parse_document(html);
    let container = Selector::parse("div.download-container").unwrap();
    let a_sel = Selector::parse("a[href]").unwrap();
    let filename = Selector::parse("div.filename").unwrap();

    let mut out = Vec::new();
    for el in doc.select(&container) {
        let classes: Vec<&str> = el.value().classes().collect();
        let site_default = classes
            .iter()
            .any(|c| c.eq_ignore_ascii_case("container-default"));
        let is_recommended = classes
            .iter()
            .any(|c| c.eq_ignore_ascii_case("container-recommended"));
        let href = el.select(&a_sel).next().and_then(|a| a.value().attr("href"));
        let Some(url) = href else { continue };

        // Dedicated-server builds are labelled "server" in the block — or, when the author
        // only said it in the file's name, nowhere but the link. Both are read, because a
        // server build that reaches the app unflagged is one the picker can preselect.
        let is_server =
            mentions_server(&el.text().collect::<String>()) || mentions_server(url_file_name(url));

        // The shown origin must reflect the ACTUAL link — authors often type a
        // mirror nickname (e.g. "GoWithTheFlow") in `div.filename`, which is not
        // the host. Derive the host from the URL; keep the author's text as the
        // label (used for per-bike sound matching).
        let filename_text = el
            .select(&filename)
            .next()
            .map(|f| f.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());
        let host = friendly_host(url);
        let label = filename_text.unwrap_or_else(|| host.clone());
        // The theme distinguishes "Recommended" mirrors from ordinary mirrors, but the
        // old parser only understood "Default". Project OEM exposes an information page
        // as its first Default and the actual full-pack mirrors as Recommended, so the app
        // dutifully tried to unpack HTML. Treat both flags as recommendations while
        // demoting OEM's known information-page handoff below the real file mirrors.
        let oem_information_page = host_from_url(url).eq_ignore_ascii_case("oem.mxb-mods.com")
            && label.to_lowercase().contains("information");
        let is_default = (site_default || is_recommended) && !oem_information_page;

        out.push(DownloadOption {
            url: url.to_string(),
            host,
            is_default,
            is_server,
            label,
        });
    }

    // The author's default file first, and dedicated-server builds after everything else —
    // whatever the page's own order was, a server build is never the file someone browsing
    // for something to ride is after.
    out.sort_by_key(|d| (d.is_server, !d.is_default));
    out
}

fn parse_version(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("p.betas").ok()?;
    let text = doc.select(&sel).next()?.text().collect::<String>();

    let re = Regex::new(r"(?i)beta\s*[0-9]+(\.[0-9]+)*").unwrap();
    if let Some(m) = re.find(&text) {
        let mut v = m.as_str().to_string();
        if let Some(first) = v.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        return Some(v);
    }
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The byline the theme prints above the post title, and the profile page it links to.
///
/// mxb-mods and gpb-mods run the same theme: `<p class="post-date">posted by
/// <a href="…/author/<slug>/"><b id="authorName">Name</b></a>`. Scoped to the post header
/// because every comment below the page names an author too, and the first `/author/` link
/// in the document is only the byline by luck of layout.
/// The page's `og:image`, when it is an https picture.
fn parse_og_image(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"meta[property="og:image"]"#).ok()?;
    doc.select(&sel)
        .filter_map(|el| el.value().attr("content"))
        .map(str::trim)
        .find(|u| u.starts_with("https://") && is_image_url(u))
        .map(str::to_string)
}

fn parse_author(html: &str) -> Option<(String, Option<String>)> {
    let doc = Html::parse_document(html);

    if let Ok(sel) = Selector::parse(r#".post-header a[href*="/author/"]"#) {
        if let Some(el) = doc.select(&sel).next() {
            let name = el.text().collect::<String>();
            let name = name.trim();
            if !name.is_empty() {
                return Some((name.to_string(), el.value().attr("href").map(str::to_string)));
            }
        }
    }

    // The name without its link — the theme's own id for it, which still stands when the
    // profile isn't linkable.
    let sel = Selector::parse("#authorName").ok()?;
    let name = doc.select(&sel).next()?.text().collect::<String>();
    let name = name.trim();
    (!name.is_empty()).then(|| (name.to_string(), None))
}

fn host_from_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// A friendly, accurate origin name derived from the download URL's host — so the
/// UI shows the real mirror (Google Drive / MediaFire / MEGA …) regardless of
/// whatever label the mod author typed on the page.
fn friendly_host(url: &str) -> String {
    let host = host_from_url(url).to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let has = |needle: &str| host.contains(needle);
    if has("drive.google") || has("drive.usercontent.google") || has("docs.google") {
        "Google Drive".to_string()
    } else if has("mediafire") {
        "MediaFire".to_string()
    } else if has("mega.nz") || has("mega.co") {
        "MEGA".to_string()
    } else if has("dropbox") {
        "Dropbox".to_string()
    } else if has("sharemods") {
        "ShareMods".to_string()
    } else if has("pixeldrain") {
        "Pixeldrain".to_string()
    } else if has("drive.proton.me") || has("proton.me") {
        "Proton Drive".to_string()
    } else if has("1drv.ms") || has("onedrive") {
        "OneDrive".to_string()
    } else if host.is_empty() {
        "Download".to_string()
    } else {
        host.to_string()
    }
}

fn dedup(v: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(x.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A post body is site-authored HTML rendered in the main window. Whatever a post holds,
    /// what reaches the webview carries no script.
    #[test]
    fn a_post_body_is_sanitised_before_the_webview_sees_it() {
        let body = r#"<p>Great track</p><svg onload="alert(1)"></svg>
<details open ontoggle="alert(2)"><summary>x</summary></details>
<a href="javascript:alert(3)">bad</a><a href="https://example.com/dl">good</a>
<img src="https://example.com/a.png" onerror="alert(4)"><script>alert(5)</script>"#;
        let html = description_from(body);
        let lower = html.to_ascii_lowercase();
        for bad in ["<svg", "<script", "javascript:", "onload", "ontoggle", "onerror", "alert("] {
            assert!(!lower.contains(bad), "{bad} survived: {html}");
        }
        assert!(html.contains("Great track"), "{html}");
        assert!(html.contains(r#"href="https://example.com/dl""#), "{html}");
        assert!(!html.contains("<img"), "images still go to the gallery: {html}");
    }

    /// The rendered pages are guarded far more tightly than the JSON API, and asking for one
    /// with the client's JSON-fetch defaults describes a request no browser makes: a script
    /// wanting an HTML document. Every default this overrides is one that said so.
    #[test]
    fn a_page_is_asked_for_the_way_a_browser_navigates() {
        let h = page_headers();
        assert_eq!(h.get("sec-fetch-dest").unwrap(), "document");
        assert_eq!(h.get("sec-fetch-mode").unwrap(), "navigate");
        assert!(h.get("accept").unwrap().to_str().unwrap().starts_with("text/html"));
        // A continued string literal is easy to leave a newline in, and a header value with
        // one in it is rejected outright — which would fail only on the blocked user's machine.
        for (_, v) in h.iter() {
            let v = v.to_str().unwrap();
            assert!(!v.contains('\n') && !v.contains("  "), "{v:?}");
        }
    }

    #[test]
    fn reads_every_embedded_term_name() {
        // Shape of `_embed=wp:term` on a real livery post: one group per taxonomy, the
        // second empty because the site files nothing under tags.
        let post: Value = serde_json::from_str(
            r#"{"_embedded":{"wp:term":[[
                 {"taxonomy":"category","name":"2023 KTM 450 SX-F OEM"},
                 {"taxonomy":"category","name":"Liveries"},
                 {"taxonomy":"category","name":"KTM"},
                 {"taxonomy":"category","name":"Ren&#038;s Bikes"}
               ],[]]}}"#,
        )
        .unwrap();
        assert_eq!(
            term_names(&post),
            vec![
                "2023 KTM 450 SX-F OEM",
                "Liveries",
                "KTM",
                // Entities decoded, same as the title.
                "Ren&s Bikes",
            ]
        );

        // A post fetched without `_embed`, or one with no terms, must not blow up.
        assert!(term_names(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn reads_the_byline_from_the_post_header() {
        // mxb-mods: byline and date in one paragraph.
        let mxb = r#"
            <div class="post-header">
              <p class="post-date">posted by
                <a href="https://mxb-mods.com/author/kenziesaunders/"><b id="authorName">Macks Tracks</b></a>
                on Aug. 29, 2026</p>
              <h1 class="post-title">Highland-Mx</h1>
            </div>"#;
        assert_eq!(
            parse_author(mxb),
            Some((
                "Macks Tracks".to_string(),
                Some("https://mxb-mods.com/author/kenziesaunders/".to_string())
            ))
        );

        // gpb-mods: same theme, date split into its own paragraph.
        let gpb = r#"
            <div class="post-header"><p class="post-date">Aug. 21, 2026</p>
              <p class="post-date">posted by
                <a href="https://gpb-mods.com/author/kalat/"><b id="authorName">Kalat le Nul</b></a></p>
            </div>"#;
        assert_eq!(
            parse_author(gpb).unwrap().0,
            "Kalat le Nul",
            "the same parse has to serve both catalogs"
        );
    }

    #[test]
    fn a_commenter_is_never_mistaken_for_the_byline() {
        // Discussion sits outside `.post-header` and names an author on every comment. A
        // document-wide search for the first `/author/` link would credit the mod to
        // whoever happened to be rendered first.
        let html = r#"
            <div class="wpd-comment">
              <a href="https://mxb-mods.com/author/somecommenter/">SomeCommenter</a>
            </div>
            <div class="post-header">
              <p class="post-date">posted by
                <a href="https://mxb-mods.com/author/dr-phdeez/"><b id="authorName">Dr.PhDeez</b></a></p>
            </div>"#;
        assert_eq!(parse_author(html).unwrap().0, "Dr.PhDeez");

        // No byline at all — a page must still parse rather than inventing one.
        assert_eq!(parse_author("<div class=\"post-header\"></div>"), None);
    }

    #[test]
    fn parses_default_download_container() {
        let html = r#"
            <div id="link1" class="download-container container-default">
              <div class="filename"><i class="fas fa-globe"></i> drive.google.com</div>
              <a href="https://drive.google.com/file/d/ABC123/view?usp=sharing">Download</a>
            </div>
            <div id="link2" class="download-container container-mirror">
              <div class="filename"><i class="fas fa-globe"></i> mediafire.com</div>
              <a href="https://www.mediafire.com/file/xyz/track.zip/file">Download</a>
            </div>
        "#;
        let downloads = parse_downloads(html);
        assert_eq!(downloads.len(), 2);
        assert!(downloads[0].is_default);
        // Origin is derived from the URL, shown as a friendly host name.
        assert_eq!(downloads[0].host, "Google Drive");
        assert_eq!(downloads[1].host, "MediaFire");
        assert!(downloads[0].url.contains("drive.google.com/file/d/ABC123"));
    }

    #[test]
    fn oem_information_page_does_not_beat_recommended_pack_mirrors() {
        // This is the significant shape of the live OEM listing. The first row is marked
        // Default by the site, but it is only release notes; the installable full pack is
        // marked Recommended. Selecting the first row used to feed HTML into the archive
        // downloader and fail with "host returned a web page instead of a file".
        let html = r#"
            <div class="download-container container-default">
              <div class="filename">Informations about the pack</div>
              <a href="https://oem.mxb-mods.com/mx-oem-v0-19-1-update/">Direct download</a>
            </div>
            <div class="download-container container-recommended">
              <div class="filename">FULL PACK v0.19.1 - MEDIAFIRE</div>
              <a href="https://www.mediafire.com/file/abc/MX_OEM_v0.19.1.zip/file">Download</a>
            </div>
            <div class="download-container container-default">
              <div class="filename">FULL PACK v0.19.1 - GOOGLE DRIVE</div>
              <a href="https://drive.google.com/file/d/ABC123/view">Download</a>
            </div>
        "#;

        let downloads = parse_downloads(html);
        assert_eq!(downloads.len(), 3);
        assert_eq!(downloads[0].host, "MediaFire");
        assert!(downloads[0].is_default);
        assert_eq!(downloads[1].host, "Google Drive");
        assert_eq!(downloads[2].host, "oem.mxb-mods.com");
        assert!(!downloads[2].is_default);
    }

    #[test]
    fn host_reflects_url_not_author_label() {
        // Author typed a mirror nickname in `div.filename` — the shown origin must
        // still be the real host from the link, with the nickname kept as label.
        let html = r#"
            <div class="download-container container-default">
              <div class="filename"><i class="fas fa-globe"></i> GoWithTheFlow</div>
              <a href="https://www.mediafire.com/file/abc/GoWithTheFlow.zip/file">Download</a>
            </div>
        "#;
        let downloads = parse_downloads(html);
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].host, "MediaFire");
        assert_eq!(downloads[0].label, "GoWithTheFlow");
    }

    #[test]
    fn reads_the_pages_og_image() {
        let html = r#"<html><head>
            <meta property="og:image" content="https://mxb-mods.com/wp-content/uploads/2026/10/655500_40.webp" />
            <meta property="og:image:width" content="1920" /></head><body></body></html>"#;
        assert_eq!(
            parse_og_image(html).as_deref(),
            Some("https://mxb-mods.com/wp-content/uploads/2026/10/655500_40.webp")
        );
        assert_eq!(parse_og_image("<html><head></head></html>"), None);
    }

    #[test]
    fn a_forbidden_featured_image_falls_back_to_the_posts_first_picture() {
        let post: Value = serde_json::from_str(
            r#"{"id":1,"slug":"reed-creek","title":{"rendered":"Reed Creek MX"},
                "featured_media":192044,
                "content":{"rendered":"<p><a href=\"https://mxb-mods.com/a.webp\"><img src=\"https://mxb-mods.com/a-1280x669.webp\"></a></p>"},
                "_embedded":{"wp:featuredmedia":[{"code":"rest_forbidden"}]}}"#,
        )
        .unwrap();
        let summary = summary_from_post(&post, 22).unwrap();
        assert_eq!(summary.image.as_deref(), Some("https://mxb-mods.com/a.webp"));
    }

    #[test]
    fn reads_the_posts_author() {
        let post: Value = serde_json::from_str(
            r#"{"id":1,"slug":"a-track","title":{"rendered":"A Track"},
                "_embedded":{"author":[{"name":"Ren&#038;s Bikes"}]}}"#,
        )
        .unwrap();
        let summary = summary_from_post(&post, 22).unwrap();
        // Entities decoded, same as the title.
        assert_eq!(summary.author.as_deref(), Some("Ren&s Bikes"));

        // A site that keeps its author list private answers the embed with an error object
        // instead of a user. That's a card without a byline, not a failed listing.
        let hidden: Value = serde_json::from_str(
            r#"{"id":2,"slug":"b-track","title":{"rendered":"B Track"},
                "_embedded":{"author":[{"code":"rest_user_cannot_view"}]}}"#,
        )
        .unwrap();
        assert_eq!(summary_from_post(&hidden, 22).unwrap().author, None);

        // No `_embed` at all, and a blank name, are the same non-answer.
        let bare: Value =
            serde_json::from_str(r#"{"id":3,"slug":"c","title":{"rendered":"C"}}"#).unwrap();
        assert_eq!(summary_from_post(&bare, 22).unwrap().author, None);
        let blank: Value = serde_json::from_str(
            r#"{"id":4,"slug":"d","title":{"rendered":"D"},"_embedded":{"author":[{"name":"  "}]}}"#,
        )
        .unwrap();
        assert_eq!(summary_from_post(&blank, 22).unwrap().author, None);
    }

    #[test]
    fn flags_server_builds_and_sorts_them_last() {
        // The author marked the server build in the block's text, and made it the page's
        // default — which is exactly how a server file used to end up preselected.
        let html = r#"
            <div class="download-container container-default">
              <div class="filename">Dedicated Server files</div>
              <a href="https://www.mediafire.com/file/abc/track_srv.zip/file">Download</a>
            </div>
            <div class="download-container container-mirror">
              <div class="filename">mediafire.com</div>
              <a href="https://www.mediafire.com/file/xyz/track.zip/file">Download</a>
            </div>
        "#;
        let downloads = parse_downloads(html);
        assert_eq!(downloads.len(), 2);
        // Playable first, despite the server build carrying the page's "default" flag.
        assert!(!downloads[0].is_server);
        assert!(downloads[1].is_server);
        assert!(downloads[1].is_default);
    }

    #[test]
    fn reads_the_server_label_out_of_the_link() {
        // Nothing in the block says "server"; only the file it points at does.
        let html = r#"
            <div class="download-container container-default">
              <div class="filename">MediaFire</div>
              <a href="https://www.mediafire.com/file/abc/Ironman_2024_Server.pkz/file">Download</a>
            </div>
        "#;
        assert!(parse_downloads(html)[0].is_server);
    }

    #[test]
    fn a_track_named_observer_is_not_a_server_build() {
        // Substring matching flagged this one, which left the mod looking undownloadable.
        let html = r#"
            <div class="download-container container-default">
              <div class="filename">Observer Hill</div>
              <a href="https://www.mediafire.com/file/abc/ObserverHill.zip/file">Download</a>
            </div>
        "#;
        assert!(!parse_downloads(html)[0].is_server);
    }

    #[test]
    fn file_name_skips_the_hosts_routing_segment() {
        assert_eq!(
            url_file_name("https://www.mediafire.com/file/abc/track_server.zip/file"),
            "track_server.zip",
        );
        assert_eq!(
            url_file_name("https://drive.google.com/file/d/ABC123/view?usp=sharing"),
            "ABC123",
        );
        assert_eq!(url_file_name("https://x.com/downloads/pack.7z"), "pack.7z");
    }

    #[test]
    fn parses_beta_version() {
        let html = r#"<p class="betas">Made for <b>Beta 19</b>. </p>"#;
        assert_eq!(parse_version(html).as_deref(), Some("Beta 19"));
    }

    #[test]
    fn decodes_title_entities() {
        assert_eq!(decode_entities("Rock &#038; Roll &#8211; MX"), "Rock & Roll – MX");
        // Hex entities too — flag emoji are common in track titles, and left raw they
        // fold `x1f1ee` into the "already installed" comparison key (issue #26).
        assert_eq!(
            decode_entities("ARDAN318 &#8211; Sirkuit Goro assalam &#x1f1ee;&#x1f1e9;"),
            "ARDAN318 – Sirkuit Goro assalam 🇮🇩",
        );
    }

    #[test]
    fn image_url_detection() {
        assert!(is_image_url("https://x/y.webp"));
        assert!(is_image_url("https://x/y.JPG?v=2"));
        assert!(!is_image_url("https://x/y.html"));
    }

    /// Live end-to-end check against mxb-mods.com (ignored by default; network).
    #[test]
    #[ignore = "hits the live mxb-mods.com API"]
    fn live_search_and_detail() {
        tauri::async_runtime::block_on(async {
            let results = search("supercross", 22, 1, ModSort::Newest)
                .await
                .expect("search failed");
            assert!(!results.is_empty(), "expected some track results");
            let first = &results[0];
            assert!(!first.title.is_empty());

            let detail = detail(&first.slug).await.expect("detail failed");
            assert_eq!(detail.slug, first.slug);
            assert!(!detail.title.is_empty());
            println!(
                "LIVE: '{}' images={} version={:?} downloads={:?}",
                detail.title,
                detail.images.len(),
                detail.version,
                detail
                    .downloads
                    .iter()
                    .map(|d| format!("{}{}", d.host, if d.is_default { "*" } else { "" }))
                    .collect::<Vec<_>>()
            );
        });
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;

    #[test]
    fn a_400_ends_a_listing_only_past_the_first_page() {
        assert!(is_end_of_listing(400, &Page::Number(2)));
        assert!(!is_end_of_listing(400, &Page::Number(1)));
        assert!(is_end_of_listing(400, &Page::Offset { skip: 24, take: 24 }));
        assert!(!is_end_of_listing(400, &Page::Offset { skip: 0, take: 24 }));
        assert!(!is_end_of_listing(500, &Page::Number(2)));
    }

    #[test]
    fn an_html_body_where_json_was_expected_is_a_readable_error() {
        let bad = |body: &str| {
            let fetched = Fetched {
                status: 200,
                headers: HashMap::new(),
                body: body.to_string(),
                url: String::new(),
            };
            fetched.json::<Vec<Value>>().unwrap_err()
        };
        let captive = bad("<html><body>Please sign in to the network</body></html>");
        assert!(captive.downcast_ref::<Blocked>().is_none());
        assert!(captive.to_string().contains("firewall"));
        assert!(!captive.to_string().contains("expected value"));
        let cf = bad("<title>Just a moment...</title>");
        assert!(cf.downcast_ref::<Blocked>().is_some());
    }

    #[test]
    fn retries_only_the_transient_blocks() {
        for code in [429u16, 503] {
            assert!(worth_retrying(reqwest::StatusCode::from_u16(code).unwrap()), "{code}");
        }
        for code in [200u16, 400, 404, 500] {
            assert!(!worth_retrying(reqwest::StatusCode::from_u16(code).unwrap()), "{code}");
        }
    }

    /// The point of the clearance work: a 403 is a bot-score refusal, and repeating the
    /// same request from the same fingerprint only fails slower. It goes up to the
    /// handshake instead.
    #[test]
    fn a_403_is_not_retried_in_place() {
        assert!(!worth_retrying(reqwest::StatusCode::FORBIDDEN));
    }

    /// Which refusals the command layer should answer with a browser window. Getting this
    /// wrong either pops a window at someone who is merely rate-limited, or fails to open
    /// one for the person this whole change exists for.
    #[test]
    fn only_challenges_are_worth_a_handshake() {
        let blocked = |code: u16| Blocked {
            status: Some(code),
            message: String::new(),
        };
        assert!(blocked(403).clearable());
        assert!(!blocked(429).clearable());
        assert!(!blocked(503).clearable());
        // The interstitial-as-a-200 — the same refusal wearing a success code.
        assert!(matches!(
            challenge_error("cf_chl_opt", 4096).downcast_ref::<Blocked>(),
            Some(b) if b.clearable()
        ));
    }

    /// `with_clearance` finds this by downcast, so a 403 has to survive the trip through
    /// `anyhow` as a `Blocked` and not collapse into a bare message.
    #[test]
    fn a_blocked_error_stays_downcastable() {
        let err = blocked_error(403, None);
        let blocked = err
            .downcast_ref::<Blocked>()
            .expect("the command layer downcasts to decide whether to run the handshake");
        assert_eq!(blocked.status, Some(403));
        // `{:#}` is what the command layer sends the UI; it must still read as the message.
        assert!(format!("{err:#}").contains("403"));
    }

    #[test]
    fn spots_a_cloudflare_interstitial() {
        assert!(challenge_marker("<title>Just a moment...</title>").is_some());
        assert!(
            challenge_marker(r#"<form id="challenge-form" class="cf-browser-verification">"#)
                .is_some()
        );
        assert!(challenge_marker("window._cf_chl_opt = {};").is_some());
    }

    #[test]
    fn a_normal_page_is_not_a_challenge() {
        // Cloudflare injects this script into ordinary pages; a real 212 KB mod page that
        // parses fine contains it. Matching on it would break every mod detail view.
        let real = r#"<title>MXB App - MXB-Mods.com</title>
            <div class="download-container"><a href="https://x/f.pkz">Default</a></div>
            <script src="/cdn-cgi/challenge-platform/h/b/scripts/jsd/main.js"></script>"#;
        assert_eq!(challenge_marker(real), None);
        assert!(!parse_downloads(real).is_empty(), "and it still parses");
    }

    #[test]
    fn blocked_errors_say_what_to_do() {
        let msg = blocked_error(403, None).to_string();
        assert!(msg.contains("403") && msg.contains("Retry"), "{msg}");
        // The old text leaked a percent-encoded URL at the user; make sure we don't.
        assert!(!msg.contains("wp-json"), "{msg}");
        // Nothing to reference, so nothing dangling on the end.
        assert!(!msg.contains("Ref:"), "{msg}");
    }

    /// The ray id is the whole reason it is on screen: it identifies this block on
    /// Cloudflare's side, so it has to survive into the text the UI renders.
    #[test]
    fn a_ray_id_rides_along_on_the_message() {
        let err = blocked_error(403, Some("8f2a1c9d4e5b6a07"));
        let msg = format!("{err:#}");
        assert!(msg.contains("Ref: 8f2a1c9d4e5b6a07"), "{msg}");
        assert!(msg.contains("403") && msg.contains("Retry"), "{msg}");
        // An absent header reads as "-" upstream; that must not reach the user as a ref.
        assert!(!blocked_error(403, Some(""))
            .to_string()
            .contains("Ref:"));
    }

    fn refused(status: u16, headers: &[(&str, &str)], body: &str) -> Fetched {
        Fetched {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: body.to_string(),
            url: "https://mxb-mods.com/wp-json/wp/v2/posts?categories=22".to_string(),
        }
    }

    /// A real 403 can't be provoked locally — it's scored on IP and TLS fingerprint — so
    /// the response is synthesised. What's under test is that the diagnostic path reads the
    /// headers, still produces a `Blocked` the command layer can act on, and carries the ray.
    #[test]
    fn a_refused_response_becomes_a_blocked_error_with_its_ray() {
        let resp = refused(
            403,
            &[
                ("cf-ray", "8f2a1c9d4e5b6a07-LHR"),
                ("cf-mitigated", "challenge"),
                ("server", "cloudflare"),
            ],
            "Sorry, you have been blocked",
        );
        let err = refusal("the catalog listing", &resp);

        let blocked = err
            .downcast_ref::<Blocked>()
            .expect("a refusal must stay actionable by the command layer");
        assert_eq!(blocked.status, Some(403));
        assert!(
            blocked.clearable(),
            "a 403 should still send the command layer to the WebView"
        );
        assert!(format!("{err:#}").contains("Ref: 8f2a1c9d4e5b6a07-LHR"));
    }

    /// The refusal log names the endpoint, not the whole URL with its query string — the
    /// catalog API and the rendered mod page sit behind different Cloudflare rules, and
    /// that distinction is the first thing to read off a report.
    #[test]
    fn the_refused_path_is_reported_without_query_noise() {
        let err = refusal("the catalog listing", &refused(403, &[], ""));
        // Nothing to reference, so no dangling Ref.
        assert!(!format!("{err:#}").contains("Ref:"));
    }

    /// What a user actually pastes into Discord. Everything a diagnosis needs has to be on
    /// it, and none of it may be a cookie value.
    ///
    /// `cargo test the_refusal_line -- --nocapture` prints it.
    #[test]
    fn the_refusal_line_carries_a_whole_diagnosis() {
        let headers: HashMap<String, String> = [
            ("cf-ray", "8f2a1c9d4e5b6a07-LHR"),
            ("cf-mitigated", "challenge"),
            ("server", "cloudflare"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        let line = refusal_line(
            "the catalog listing",
            "/wp-json/wp/v2/posts",
            403,
            &headers,
            "__cf_bm (no cf_clearance)",
            "<html>\n  <h1>Sorry, you have been blocked</h1>\n</html>",
        );
        eprintln!("{line}");

        for expected in [
            "the catalog listing",   // which call failed
            "/wp-json/wp/v2/posts",  // which endpoint — REST and the mod page differ
            "403",
            "cf-ray=8f2a1c9d4e5b6a07-LHR", // identifies the block on Cloudflare's side
            "cf-mitigated=challenge",      // how Cloudflare decided
            "no cf_clearance",             // did we even have a cookie to send
            "you have been blocked",       // Cloudflare's own reason, from the body
            "retry-after=-",               // absent headers read as '-', never as a panic
        ] {
            assert!(line.contains(expected), "missing {expected:?} in: {line}");
        }
        assert!(!line.contains('\n'), "a log line must stay one line: {line}");
    }

    /// Named markers, so the log says *why* a 200 was called a challenge.
    #[test]
    fn the_challenge_marker_says_which_one_matched() {
        assert_eq!(
            challenge_marker("<title>Just a moment...</title>"),
            Some("<title>just a moment")
        );
        assert_eq!(
            challenge_marker(r#"<div class="cf-browser-verification">"#),
            Some("cf-browser-verification")
        );
        assert_eq!(challenge_marker("window._cf_chl_opt = {};"), Some("cf_chl_opt"));
        assert_eq!(challenge_marker("<h1>MXB App</h1>"), None);
    }

    /// The old loop tried a 503 three times per call, and every card and keystroke made its
    /// own call. Now: twice, then the whole site is parked and the next call sends nothing.
    #[tokio::test]
    async fn a_503_is_tried_at_most_twice_then_the_cooldown_sends_nothing() {
        let (base, seen) = super::mxbmods_api::mock::serve(std::sync::Arc::new(|_| {
            (503, vec![], "busy".to_string())
        }));
        let cooldown = Mutex::new(SiteCooldown::new());
        let client = reqwest::Client::new();
        let url = format!("{base}/wp-json/wp/v2/posts");

        let first = send(&client, &cooldown, &url, &[], Want::Api).await.unwrap();
        assert_eq!(first.status, 503);
        assert_eq!(seen.lock().unwrap().len(), 2, "one retry, no more");

        for _ in 0..5 {
            let err = send(&client, &cooldown, &url, &[], Want::Api).await.err().expect("parked");
            let blocked = err.downcast_ref::<Blocked>().expect("the existing blocked message");
            assert_eq!(blocked.status, Some(503));
            assert!(!blocked.clearable(), "a cooldown never opens the WebView check");
        }
        assert_eq!(seen.lock().unwrap().len(), 2, "parked: nothing more was sent");
    }

    /// A `Retry-After` longer than the one-second pause is obeyed, not raced.
    #[tokio::test]
    async fn a_429_with_retry_after_is_not_retried_on_the_spot() {
        let (base, seen) = super::mxbmods_api::mock::serve(std::sync::Arc::new(|_| {
            (429, vec![("Retry-After".to_string(), "120".to_string())], String::new())
        }));
        let cooldown = Mutex::new(SiteCooldown::new());
        let client = reqwest::Client::new();
        let url = format!("{base}/wp-admin/admin-ajax.php");

        assert_eq!(send(&client, &cooldown, &url, &[], Want::Api).await.unwrap().status, 429);
        assert_eq!(seen.lock().unwrap().len(), 1);
        let err = send(&client, &cooldown, &url, &[], Want::Api).await.err().expect("parked");
        assert_eq!(err.downcast_ref::<Blocked>().unwrap().status, Some(429));
        let left = lock(&cooldown).gate.until(Instant::now()).unwrap();
        assert!(left.saturating_duration_since(Instant::now()) > Duration::from_secs(100));
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_slug_hit_must_be_filed_under_the_category() {
        let full: Value = serde_json::from_str(
            r#"{"id":7,"slug":"fort-red","title":"Fort Red","link":"https://mxb-mods.com/fort-red/",
                "date":"2026-01-02T03:04:05Z","author":{"name":"someone"},"categories":[22,40]}"#,
        )
        .unwrap();
        let hit = slug_hit(&full, 22).unwrap();
        assert_eq!((hit.id, hit.title.as_str(), hit.category_id), (7, "Fort Red", 22));
        assert!(slug_hit(&full, 29).is_none(), "a livery is not a track");
    }

    /// Log lines go on one line, and a Cloudflare block page is mostly indentation.
    #[test]
    fn snippets_collapse_and_truncate() {
        assert_eq!(snippet(""), "(empty)");
        assert_eq!(snippet("  a\n\n   b  "), "\"a b\"");

        let long = "x".repeat(900);
        let cut = snippet(&long);
        assert!(cut.contains('…') && cut.contains("900 bytes total"), "{cut}");
        assert!(cut.chars().count() < 600, "still fits on a log line");
    }

    /// Live check against mxb-mods.com — the client this ships, not an approximation.
    /// `cargo test live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_search_and_detail() {
        let mods = search("", 22, 1, ModSort::Newest).await.expect("search works");
        eprintln!("search returned {} tracks", mods.len());
        assert!(!mods.is_empty(), "the tracks category should not be empty");

        let d = detail(&mods[0].slug).await.expect("detail works");
        eprintln!(
            "detail '{}': {} downloads, version {:?}, by {:?} ({:?})",
            d.title, d.downloads.len(), d.version, d.author, d.author_url
        );
        assert!(!d.title.is_empty());
        // The byline is scraped off the rendered page, not the REST API — the site answers
        // `_embed=author` with an empty user, so nothing else would notice a theme change.
        assert!(d.author.is_some(), "every post on the catalog carries a byline");
    }

    /// The ReShade Presets category the Settings card sends people to is real, populated,
    /// and its posts carry downloads like any other mod.
    ///
    /// It is the one browse category whose id isn't shared with a folder in the mods tree,
    /// so nothing else would notice if the site renumbered it — the tab would just come back
    /// empty. `cargo test live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_reshade_category_has_presets() {
        const RESHADE: u32 = 174;
        let mods = search("", RESHADE, 1, ModSort::Newest)
            .await
            .expect("search works");
        eprintln!("ReShade category returned {} presets", mods.len());
        assert!(!mods.is_empty(), "category {RESHADE} should not be empty");

        let d = detail(&mods[0].slug).await.expect("detail works");
        eprintln!("'{}': {} downloads", d.title, d.downloads.len());
        assert!(
            !d.downloads.is_empty(),
            "a preset with no download can't be installed",
        );
    }

    /// Every sort the UI offers really is accepted by the catalog, and each one changes
    /// the order it hands back. `cargo test live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_search_sorts() {
        let newest = search("", 22, 1, ModSort::Newest).await.expect("newest works");
        assert!(!newest.is_empty(), "the tracks category should not be empty");

        // Every sort must actually move the listing. The site ignores `orderby`, so a
        // sort that quietly did nothing is the exact failure mode worth catching.
        for sort in [
            ModSort::Oldest,
            ModSort::PopularAll,
            ModSort::PopularMonth,
            ModSort::PopularWeek,
        ] {
            let mods = search("", 22, 1, sort)
                .await
                .unwrap_or_else(|e| panic!("{sort:?} failed: {e:#}"));
            assert!(!mods.is_empty(), "{sort:?} returned nothing");
            assert_ne!(
                mods[0].id, newest[0].id,
                "{sort:?} returned the same listing as newest — is it being ignored?"
            );
            eprintln!("{sort:?}: first is '{}' ({})", mods[0].title, mods[0].date);
        }

        // Oldest walks the listing backwards, so its first page really should be the
        // catalog's earliest posts — years behind whatever is on the newest page.
        let oldest = search("", 22, 1, ModSort::Oldest).await.expect("oldest works");
        assert!(
            oldest[0].date < newest[0].date,
            "oldest ({}) should predate newest ({})",
            oldest[0].date,
            newest[0].date
        );
        // ...and it pages without repeating itself.
        let oldest_p2 = search("", 22, 2, ModSort::Oldest).await.expect("oldest pages");
        assert!(!oldest_p2.is_empty(), "oldest page 2 was empty");
        assert!(
            oldest_p2.iter().all(|m| !oldest.iter().any(|o| o.id == m.id)),
            "oldest page 2 repeats page 1"
        );

        // A popular sort can't carry a search term, so a query has to fall back to the
        // catalog rather than silently returning the unfiltered top-viewed list.
        let searched = search("supercross", 22, 1, ModSort::PopularAll)
            .await
            .expect("popular + query falls back");
        let plain = search("supercross", 22, 1, ModSort::Newest)
            .await
            .expect("newest + query works");
        assert_eq!(
            searched.first().map(|m| m.id),
            plain.first().map(|m| m.id),
            "a query under a popular sort should behave like newest"
        );
    }

    /// The headers really do go out — proves the fix is on the wire, not just in source.
    #[tokio::test]
    #[ignore]
    async fn live_sends_browser_headers() {
        let body: serde_json::Value = client()
            .unwrap()
            .get("https://httpbin.org/headers")
            .header(reqwest::header::USER_AGENT, mxb_session::ua())
            .send()
            .await
            .expect("reachable")
            .json()
            .await
            .expect("json");
        let h = &body["headers"];
        eprintln!("{}", serde_json::to_string_pretty(h).unwrap());
        assert!(h["User-Agent"].as_str().unwrap().contains("Chrome/"));
        assert!(h["Accept-Encoding"].as_str().unwrap().contains("gzip"));
        assert!(h["Accept-Language"].as_str().is_some());
        assert!(h["Sec-Fetch-Mode"].as_str().is_some());
    }
}
