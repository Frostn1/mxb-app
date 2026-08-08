use super::{DownloadOption, ModDetail, ModSource, ModSummary};
use crate::mxb_session;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;

const BASE: &str = mxb_session::BASE;
const PER_PAGE: &str = "24";

pub struct MxbModsSource;

impl ModSource for MxbModsSource {
    async fn search(
        &self,
        query: &str,
        category_id: u32,
        page: u32,
    ) -> anyhow::Result<Vec<ModSummary>> {
        search(query, category_id, page).await
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
        ("referer", BASE),
    ] {
        headers.insert(k, HeaderValue::from_static(v));
    }
    // UA, jar and timeouts come from `mxb_session` so the client and the handshake WebView
    // cannot drift apart — a `cf_clearance` is bound to the UA that earned it.
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

/// `GET` with backoff over the transient blocks above. Transport errors retry too, which
/// is what `install::get_with_retry` already does for download hosts.
async fn get_with_retry(
    url: &str,
    params: &[(&str, String)],
) -> anyhow::Result<reqwest::Response> {
    const ATTEMPTS: u32 = 3;
    let client = client()?;
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=ATTEMPTS {
        match client.get(url).query(params).send().await {
            Ok(resp) if !worth_retrying(resp.status()) => return Ok(resp),
            Ok(resp) => {
                let status = resp.status();
                if attempt == ATTEMPTS {
                    return Ok(resp);
                }
                last_err = Some(anyhow::anyhow!("{status}"));
            }
            Err(e) => {
                if attempt == ATTEMPTS {
                    return Err(e.into());
                }
                last_err = Some(e.into());
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("request failed")))
}

/// Cloudflare refused us, as opposed to the site being down or the parse failing.
///
/// A type rather than a message because the command layer has to *act* on it — run the
/// WebView handshake and retry — and matching on error strings to decide that would break
/// the first time someone reworded a sentence.
#[derive(Debug, Clone)]
pub struct Blocked {
    /// The refusing status, or `None` for an interstitial served as a 200.
    pub status: Option<u16>,
    message: String,
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Blocked {}

impl Blocked {
    /// True when a `cf_clearance` could plausibly fix it — i.e. we were challenged, rather
    /// than rate-limited or handed a server error.
    pub fn clearable(&self) -> bool {
        matches!(self.status, None | Some(403))
    }
}

/// Turn a blocked response into something a person can act on. The raw reqwest `Display`
/// ("HTTP status client error (403 Forbidden) for url (…)") told users nothing and shipped
/// them a URL with percent-encoded query params.
///
/// The 403 wording assumes the handshake above it has already been tried and failed, since
/// that is the only way this message reaches a user.
fn blocked_error(status: reqwest::StatusCode) -> anyhow::Error {
    let message = match status.as_u16() {
        403 => "mxb-mods.com refused the request (403), and its check window didn't clear \
                it either. Open mxb-mods.com in your normal browser to confirm the site \
                loads for you, then hit Retry."
            .to_string(),
        429 => "mxb-mods.com is rate-limiting us (429). Give it a minute, then hit Retry."
            .to_string(),
        503 => "mxb-mods.com is unavailable right now (503) — it may be behind a Cloudflare \
                check. Try again shortly."
            .to_string(),
        _ => format!("mxb-mods.com returned {status}"),
    };
    anyhow::Error::new(Blocked {
        status: Some(status.as_u16()),
        message,
    })
}

/// The interstitial-as-a-200 case. Same handling as a 403 — it is the same refusal, just
/// dressed as a success.
fn challenge_error() -> anyhow::Error {
    anyhow::Error::new(Blocked {
        status: None,
        message: "mxb-mods.com served a Cloudflare check instead of the mod page, and its \
                  check window didn't clear it. Open mxb-mods.com in your normal browser, \
                  then hit Retry."
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
fn is_challenge(html: &str) -> bool {
    html.contains("cf-browser-verification")
        || html.contains("cf_chl_opt")
        || html.to_ascii_lowercase().contains("<title>just a moment")
}

pub async fn search(
    query: &str,
    category_id: u32,
    page: u32,
) -> anyhow::Result<Vec<ModSummary>> {
    let url = format!("{BASE}/wp-json/wp/v2/posts");

    let mut params: Vec<(&str, String)> = vec![
        ("categories", category_id.to_string()),
        ("page", page.to_string()),
        ("per_page", PER_PAGE.to_string()),
        ("orderby", "date".to_string()),
        ("_embed", "wp:featuredmedia".to_string()),
    ];
    let q = query.trim();
    if !q.is_empty() {
        params.push(("search", q.to_string()));
    }

    let resp = get_with_retry(&url, &params).await?;
    // WP returns 400 (rest_post_invalid_page_number) once you page past the end.
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        return Ok(vec![]);
    }
    if !resp.status().is_success() {
        return Err(blocked_error(resp.status()));
    }
    let posts: Vec<Value> = resp.json().await?;

    Ok(posts
        .iter()
        .filter_map(|p| summary_from_post(p, category_id))
        .collect())
}

pub async fn detail(slug: &str) -> anyhow::Result<ModDetail> {
    // 1. Post metadata + description via the REST API.
    let url = format!("{BASE}/wp-json/wp/v2/posts");
    let params = vec![
        ("slug", slug.to_string()),
        ("_embed", "wp:featuredmedia".to_string()),
    ];
    let resp = get_with_retry(&url, &params).await?;
    if !resp.status().is_success() {
        return Err(blocked_error(resp.status()));
    }
    let posts: Vec<Value> = resp.json().await?;
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

    let description_html = strip_images(&content);

    // 2. Download links + version from the rendered page HTML. A block here used to be
    // swallowed: a 403 is `Ok(resp)`, so its error body went to the parsers and produced
    // zero downloads — the page then said "No download link was found on this page"
    // rather than "we couldn't read the page". Surface it instead.
    let resp = get_with_retry(&link, &[]).await?;
    if !resp.status().is_success() {
        return Err(blocked_error(resp.status()));
    }
    let html = resp.text().await.unwrap_or_default();
    let (downloads, version) = (parse_downloads(&html), parse_version(&html));
    // Only call it a challenge when the page also yielded nothing, so a marker that turns
    // up in a page that actually parsed can never turn a working mod into an error.
    if downloads.is_empty() && is_challenge(&html) {
        return Err(challenge_error());
    }

    Ok(ModDetail {
        id,
        slug: slug.to_string(),
        title,
        link,
        date,
        description_html,
        images,
        version,
        downloads,
    })
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
        image: featured_image(p),
        category_id,
    })
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
fn extract_images(content_html: &str) -> Vec<String> {
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

/// Remove image markup from the description (images are shown in the gallery).
fn strip_images(html: &str) -> String {
    let a_img = Regex::new(r"(?is)<a\b[^>]*>\s*<img\b[^>]*>\s*</a>").unwrap();
    let img = Regex::new(r"(?is)<img\b[^>]*>").unwrap();
    let s = a_img.replace_all(html, "");
    img.replace_all(&s, "").into_owned()
}

/// Parse the theme's `div.download-container` blocks into download options.
fn parse_downloads(html: &str) -> Vec<DownloadOption> {
    let doc = Html::parse_document(html);
    let container = Selector::parse("div.download-container").unwrap();
    let a_sel = Selector::parse("a[href]").unwrap();
    let filename = Selector::parse("div.filename").unwrap();

    let mut out = Vec::new();
    for el in doc.select(&container) {
        let is_default = el
            .value()
            .classes()
            .any(|c| c.eq_ignore_ascii_case("container-default"));
        // Dedicated-server builds are labelled "server" somewhere in the block.
        let is_server = el.text().collect::<String>().to_lowercase().contains("server");
        let href = el.select(&a_sel).next().and_then(|a| a.value().attr("href"));
        let Some(url) = href else { continue };

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

        out.push(DownloadOption {
            url: url.to_string(),
            host,
            is_default,
            is_server,
            label,
        });
    }

    // Show the author's default file first.
    out.sort_by_key(|d| !d.is_default);
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
    fn parses_beta_version() {
        let html = r#"<p class="betas">Made for <b>Beta 19</b>. </p>"#;
        assert_eq!(parse_version(html).as_deref(), Some("Beta 19"));
    }

    #[test]
    fn decodes_title_entities() {
        assert_eq!(decode_entities("Rock &#038; Roll &#8211; MX"), "Rock & Roll – MX");
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
            let results = search("supercross", 22, 1).await.expect("search failed");
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
            challenge_error().downcast_ref::<Blocked>(),
            Some(b) if b.clearable()
        ));
    }

    /// `with_clearance` finds this by downcast, so a 403 has to survive the trip through
    /// `anyhow` as a `Blocked` and not collapse into a bare message.
    #[test]
    fn a_blocked_error_stays_downcastable() {
        let err = blocked_error(reqwest::StatusCode::FORBIDDEN);
        let blocked = err
            .downcast_ref::<Blocked>()
            .expect("the command layer downcasts to decide whether to run the handshake");
        assert_eq!(blocked.status, Some(403));
        // `{:#}` is what the command layer sends the UI; it must still read as the message.
        assert!(format!("{err:#}").contains("403"));
    }

    #[test]
    fn spots_a_cloudflare_interstitial() {
        assert!(is_challenge("<title>Just a moment...</title>"));
        assert!(is_challenge(r#"<form id="challenge-form" class="cf-browser-verification">"#));
        assert!(is_challenge("window._cf_chl_opt = {};"));
    }

    #[test]
    fn a_normal_page_is_not_a_challenge() {
        // Cloudflare injects this script into ordinary pages; a real 212 KB mod page that
        // parses fine contains it. Matching on it would break every mod detail view.
        let real = r#"<title>MXB App - MXB-Mods.com</title>
            <div class="download-container"><a href="https://x/f.pkz">Default</a></div>
            <script src="/cdn-cgi/challenge-platform/h/b/scripts/jsd/main.js"></script>"#;
        assert!(!is_challenge(real));
        assert!(!parse_downloads(real).is_empty(), "and it still parses");
    }

    #[test]
    fn blocked_errors_say_what_to_do() {
        let msg = blocked_error(reqwest::StatusCode::FORBIDDEN).to_string();
        assert!(msg.contains("403") && msg.contains("Retry"), "{msg}");
        // The old text leaked a percent-encoded URL at the user; make sure we don't.
        assert!(!msg.contains("wp-json"), "{msg}");
    }

    /// Live check against mxb-mods.com — the client this ships, not an approximation.
    /// `cargo test live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_search_and_detail() {
        let mods = search("", 22, 1).await.expect("search works");
        eprintln!("search returned {} tracks", mods.len());
        assert!(!mods.is_empty(), "the tracks category should not be empty");

        let d = detail(&mods[0].slug).await.expect("detail works");
        eprintln!("detail '{}': {} downloads, version {:?}", d.title, d.downloads.len(), d.version);
        assert!(!d.title.is_empty());
    }

    /// The headers really do go out — proves the fix is on the wire, not just in source.
    #[tokio::test]
    #[ignore]
    async fn live_sends_browser_headers() {
        let body: serde_json::Value = client()
            .unwrap()
            .get("https://httpbin.org/headers")
            .send()
            .await
            .expect("reachable")
            .json()
            .await
            .expect("json");
        let h = &body["headers"];
        eprintln!("{}", serde_json::to_string_pretty(h).unwrap());
        assert!(h["User-Agent"].as_str().unwrap().contains("Chrome/131.0.6778.140"));
        assert!(h["Accept-Encoding"].as_str().unwrap().contains("gzip"));
        assert!(h["Accept-Language"].as_str().is_some());
        assert!(h["Sec-Fetch-Mode"].as_str().is_some());
    }
}

