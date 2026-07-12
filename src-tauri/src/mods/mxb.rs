//! mxb-mods.com catalog source.
//!
//! mxb-mods.com is a WordPress site behind Cloudflare. A browser-like
//! User-Agent gets clean JSON from the WP REST API; the generic default UA is
//! 403'd. Two channels are used:
//!   * search / listing / images  -> WP REST API (`/wp-json/wp/v2/posts`)
//!   * canonical download link     -> the post's rendered HTML page, where the
//!     theme injects `div.download-container` blocks (not present in REST).

use super::{DownloadOption, ModDetail, ModSource, ModSummary};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;

const BASE: &str = "https://mxb-mods.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";
const PER_PAGE: &str = "24";

/// The mxb-mods.com implementation of [`ModSource`].
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

fn build_client() -> anyhow::Result<Client> {
    Ok(Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

pub async fn search(
    query: &str,
    category_id: u32,
    page: u32,
) -> anyhow::Result<Vec<ModSummary>> {
    let client = build_client()?;
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

    let resp = client.get(&url).query(&params).send().await?;
    // WP returns 400 (rest_post_invalid_page_number) once you page past the end.
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        return Ok(vec![]);
    }
    let resp = resp.error_for_status()?;
    let posts: Vec<Value> = resp.json().await?;

    Ok(posts
        .iter()
        .filter_map(|p| summary_from_post(p, category_id))
        .collect())
}

pub async fn detail(slug: &str) -> anyhow::Result<ModDetail> {
    let client = build_client()?;

    // 1. Post metadata + description via the REST API.
    let url = format!("{BASE}/wp-json/wp/v2/posts");
    let params = vec![
        ("slug", slug.to_string()),
        ("_embed", "wp:featuredmedia".to_string()),
    ];
    let resp = client
        .get(&url)
        .query(&params)
        .send()
        .await?
        .error_for_status()?;
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

    // 2. Download links + version from the rendered page HTML.
    let (downloads, version) = match client.get(&link).send().await {
        Ok(r) => {
            let html = r.text().await.unwrap_or_default();
            (parse_downloads(&html), parse_version(&html))
        }
        Err(_) => (Vec::new(), None),
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
        downloads,
    })
}

// --- parsing helpers -------------------------------------------------------

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
        let href = el.select(&a_sel).next().and_then(|a| a.value().attr("href"));
        let Some(url) = href else { continue };

        let host = el
            .select(&filename)
            .next()
            .map(|f| f.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| host_from_url(url));

        out.push(DownloadOption {
            url: url.to_string(),
            host: host.clone(),
            is_default,
            label: host,
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
        assert_eq!(downloads[0].host, "drive.google.com");
        assert!(downloads[0].url.contains("drive.google.com/file/d/ABC123"));
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

    /// Live end-to-end check against mxb-mods.com. Ignored by default (network);
    /// run with `cargo test -- --ignored`.
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
