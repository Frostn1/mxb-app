//! MediaFire share links into something to download: the MXB App's resolver
//! (`apps/manager/src-tauri/src/install.rs`) and the mirror Worker's port of it
//! (`control-plane/src/mirrorhosts.ts`), on regexes so the box needs no HTML parser.
//!
//! A file share resolves to its CDN link: the page first (the API has been the one refusing
//! anonymous callers), the API second. A folder share is listed through the public API,
//! sub-folders and all, and handed back to the control plane as parts.

use base64::Engine;
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

/// How deep a folder share is walked, and how many files it may hold: the app's limits.
pub const FOLDER_MAX_DEPTH: usize = 4;
pub const FOLDER_MAX_FILES: usize = 500;
/// 100 entries a chunk; a stop for a `more_chunks` that never says no.
pub const MAX_FOLDER_CHUNKS: u32 = 10;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid pattern"))
}

pub fn is_mediafire(url: &str) -> bool {
    host_of(url).is_some_and(|h| h == "mediafire.com" || h.ends_with(".mediafire.com"))
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .split('@')
        .next_back()?
        .split(':')
        .next()?;
    Some(host.to_ascii_lowercase())
}

pub fn api_url(path: &str, query: &str) -> String {
    format!("https://www.mediafire.com/api/1.5/{path}?{query}&response_format=json")
}

/// The quick key of a file share. 11 or 15 characters, in the path or bare in the query.
pub fn quick_key(url: &str) -> Option<String> {
    static BY_PATH: OnceLock<Regex> = OnceLock::new();
    static BY_QUERY: OnceLock<Regex> = OnceLock::new();
    re(
        &BY_PATH,
        r"(?i)/(?:file|file_premium|download|view)/([a-z0-9]{11}(?:[a-z0-9]{4})?)",
    )
    .captures(url)
    .or_else(|| {
        re(
            &BY_QUERY,
            r"(?i)[?&]([a-z0-9]{11}(?:[a-z0-9]{4})?)(?:[&#]|$)",
        )
        .captures(url)
    })
    .map(|c| c[1].to_string())
}

/// The folder key: `…/folder/<key>/<name>`, or `?sharekey=`.
pub fn folder_key(url: &str) -> Option<String> {
    static BY_PATH: OnceLock<Regex> = OnceLock::new();
    static BY_QUERY: OnceLock<Regex> = OnceLock::new();
    re(&BY_PATH, r"(?i)/folder/([a-z0-9]+)")
        .captures(url)
        .or_else(|| re(&BY_QUERY, r"(?i)[?&]sharekey=([a-z0-9]+)").captures(url))
        .map(|c| c[1].to_string())
}

/// A CDN link that already serves bytes, not a share page.
pub fn is_direct(url: &str) -> bool {
    static DIRECT: OnceLock<Regex> = OnceLock::new();
    re(&DIRECT, r"(?i)^https?://download[0-9]*\.mediafire\.com/").is_match(url)
}

fn decode_html(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// A link off the page, or None for the placeholders MediaFire parks in `href` (`#`,
/// `javascript:void(0)`).
fn usable_link(href: &str) -> Option<String> {
    let h = decode_html(href.trim());
    if let Some(rest) = h.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    (h.starts_with("http://") || h.starts_with("https://")).then_some(h)
}

fn decode_scrambled(value: &str) -> Option<String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .ok()?;
    usable_link(std::str::from_utf8(&raw).ok()?)
}

/// The direct CDN link on a MediaFire file page, wherever this month's layout put it.
pub fn parse_link(html: &str) -> Option<String> {
    static ATTR: OnceLock<Regex> = OnceLock::new();
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    static TAG: OnceLock<Regex> = OnceLock::new();
    static BUTTON_ID: OnceLock<Regex> = OnceLock::new();
    static BUTTON_LABEL: OnceLock<Regex> = OnceLock::new();
    static HREF: OnceLock<Regex> = OnceLock::new();
    static CDN: OnceLock<Regex> = OnceLock::new();

    // 1. The base64 `data-scrambled-url`, on whatever element carries it.
    for c in re(&ATTR, r#"(?i)data-scrambled-url\s*=\s*"([^"]*)""#).captures_iter(html) {
        if let Some(u) = decode_scrambled(&c[1]) {
            return Some(u);
        }
    }
    // 1b. The same base64, assigned in a script.
    for c in re(
        &SCRIPT,
        r#"(?i)scrambled[_-]?url["'\s:=]+([A-Za-z0-9+/=]{24,})"#,
    )
    .captures_iter(html)
    {
        if let Some(u) = decode_scrambled(&c[1]) {
            return Some(u);
        }
    }
    // 2. The download button's own href, whatever the attribute order.
    for tag in re(&TAG, r"(?i)<a\b[^>]*>").find_iter(html) {
        let tag = tag.as_str();
        let button = re(&BUTTON_ID, r#"(?i)\bid\s*=\s*"downloadButton""#).is_match(tag)
            || re(
                &BUTTON_LABEL,
                r#"(?i)aria-label\s*=\s*['"]Download file['"]"#,
            )
            .is_match(tag);
        if !button {
            continue;
        }
        if let Some(u) = re(&HREF, r#"(?i)\bhref\s*=\s*"([^"]*)""#)
            .captures(tag)
            .and_then(|c| usable_link(&c[1]))
        {
            return Some(u);
        }
    }
    // 3. Anywhere in the source, JSON-escaped slashes undone first.
    let flat = html.replace("\\/", "/");
    re(
        &CDN,
        r#"(?i)(?:https?:)?//download[0-9]*\.mediafire\.com/[^"'<>\\ ]+"#,
    )
    .find(&flat)
    .and_then(|m| usable_link(m.as_str()))
}

/// Why MediaFire won't hand a file over, from an API message or the page's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub message: &'static str,
    /// Not worth asking again.
    pub permanent: bool,
    /// Ask again no sooner than this.
    pub retry_after_ms: Option<u64>,
}

pub fn refusal(text: &str) -> Option<Refusal> {
    let t = text.to_lowercase();
    let has = |s: &str| t.contains(s);
    if has("invalid or deleted file")
        || has("has been removed")
        || has("has been deleted")
        || has("unknown or invalid quickkey")
        || has("file not found")
    {
        return Some(Refusal {
            message: "MediaFire: the file no longer exists",
            permanent: true,
            retry_after_ms: None,
        });
    }
    if has("enter password") || has("password to access") {
        return Some(Refusal {
            message: "MediaFire: password-protected",
            permanent: true,
            retry_after_ms: None,
        });
    }
    if has("violation of our terms") || has("dangerous file") {
        return Some(Refusal {
            message: "MediaFire: blocked by MediaFire",
            permanent: true,
            retry_after_ms: None,
        });
    }
    if has("bandwidth limit") || has("daily download limit") {
        return Some(Refusal {
            message: "MediaFire: download limit reached",
            permanent: false,
            retry_after_ms: Some(6 * 3600 * 1000),
        });
    }
    None
}

/// The API's `response` object said no: the refusal, or its own message.
pub fn api_error(response: &Value) -> Option<(String, bool)> {
    if !response["result"]
        .as_str()
        .is_some_and(|r| r.eq_ignore_ascii_case("error"))
    {
        return None;
    }
    let msg = response["message"].as_str().unwrap_or_default();
    Some(match refusal(msg) {
        Some(r) => (r.message.to_string(), r.permanent),
        None => (format!("MediaFire refused ({msg})"), false),
    })
}

/// `file/get_links.php`'s direct link, if it gave one that really is a CDN link.
pub fn api_direct_link(response: &Value) -> Option<String> {
    response["links"]
        .as_array()
        .and_then(|l| l.first())
        .and_then(|l| l["direct_download"].as_str())
        .and_then(usable_link)
        .filter(|u| is_direct(u))
}

/// One file in a folder share.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RemoteFile {
    pub rel: String,
    pub url: String,
    pub size: Option<u64>,
}

/// One chunk of `folder/get_content.php`: its items and whether more chunks follow.
pub fn folder_chunk(response: &Value, content_type: &str) -> (Vec<Value>, bool) {
    let content = &response["folder_content"];
    let items = content[content_type]
        .as_array()
        .cloned()
        .unwrap_or_default();
    (items, content["more_chunks"].as_str() == Some("yes"))
}

/// A file entry from a listing: its name and its own share page, the route that yields a CDN link.
pub fn listed_file(item: &Value, prefix: &str) -> Option<RemoteFile> {
    let name = item["filename"]
        .as_str()
        .map(str::trim)
        .filter(|n| usable_name(n))?;
    let url = item["links"]["normal_download"].as_str()?;
    let size = item["size"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| item["size"].as_u64());
    Some(RemoteFile {
        rel: format!("{prefix}{name}"),
        url: url.to_string(),
        size,
    })
}

/// A sub-folder entry: its key and the prefix its files go under.
pub fn listed_folder(item: &Value, prefix: &str) -> Option<(String, String)> {
    let key = item["folderkey"].as_str()?;
    let name = item["name"]
        .as_str()
        .map(str::trim)
        .filter(|n| usable_name(n))?;
    Some((key.to_string(), format!("{prefix}{}/", sanitize(name))))
}

/// A name that can be a path segment: not empty, not `.`/`..`.
pub fn usable_name(n: &str) -> bool {
    !n.is_empty() && n != "." && n != ".." && n.len() <= 255
}

pub fn sanitize(n: &str) -> String {
    n.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// The share's own name, after the key in `…/folder/<key>/<Name>`; "mod" when it has none.
pub fn folder_name(url: &str) -> String {
    static NAME: OnceLock<Regex> = OnceLock::new();
    re(&NAME, r"(?i)/folder/[a-z0-9]+/([^/?#]+)")
        .captures(url)
        .map(|c| percent_decode(&c[1]).replace(['_', '+'], " "))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "mod".to_string())
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The button's href is a placeholder; the real link is only there base64-scrambled.
    #[test]
    fn link_comes_out_of_the_scrambled_attribute() {
        let html = r#"<a id="downloadButton" class="input popsok" aria-label="Download file"
               href="javascript:void(0)"
               data-scrambled-url="aHR0cHM6Ly9kb3dubG9hZDIyMDIubWVkaWFmaXJlLmNvbS9hYmMvdHJhY2sucGt6">Download</a>"#;
        assert_eq!(
            parse_link(html).as_deref(),
            Some("https://download2202.mediafire.com/abc/track.pkz")
        );
    }

    #[test]
    fn link_survives_attribute_order() {
        let html = r#"<a href="https://download1234.mediafire.com/xyz/bike.zip"
                         aria-label="Download file" id="downloadButton">Download</a>"#;
        assert_eq!(
            parse_link(html).as_deref(),
            Some("https://download1234.mediafire.com/xyz/bike.zip")
        );
    }

    #[test]
    fn link_from_page_source() {
        let relative =
            r#"<a id="downloadButton" href="//download99.mediafire.com/q/track.rar">go</a>"#;
        assert_eq!(
            parse_link(relative).as_deref(),
            Some("https://download99.mediafire.com/q/track.rar")
        );
        let escaped =
            r#"<script>var u = "https:\/\/download7.mediafire.com\/k\/paint.pnt";</script>"#;
        assert_eq!(
            parse_link(escaped).as_deref(),
            Some("https://download7.mediafire.com/k/paint.pnt")
        );
    }

    #[test]
    fn placeholder_href_is_not_a_link() {
        assert!(parse_link(r##"<a id="downloadButton" href="#">Download</a>"##).is_none());
    }

    #[test]
    fn link_from_scrambled_script_variable() {
        let html = r#"<script>var scrambledUrl = "aHR0cHM6Ly9kb3dubG9hZDIyMDIubWVkaWFmaXJlLmNvbS9hYmMvdHJhY2sucGt6";</script>"#;
        assert_eq!(
            parse_link(html).as_deref(),
            Some("https://download2202.mediafire.com/abc/track.pkz")
        );
    }

    #[test]
    fn link_from_unnumbered_cdn_host() {
        let html = r#"<script>u="https://download.mediafire.com/ab/cd/bike.zip"</script>"#;
        assert_eq!(
            parse_link(html).as_deref(),
            Some("https://download.mediafire.com/ab/cd/bike.zip")
        );
    }

    #[test]
    fn entities_in_an_href_are_decoded() {
        let html = r#"<a id="downloadButton" href="https://download9.mediafire.com/a/b.zip?x=1&amp;y=2">go</a>"#;
        assert_eq!(
            parse_link(html).as_deref(),
            Some("https://download9.mediafire.com/a/b.zip?x=1&y=2")
        );
    }

    #[test]
    fn quick_key_from_every_share_shape() {
        for url in [
            "https://www.mediafire.com/file/bqmw1tdd7yq3qzr/I40_MX.pkz/file",
            "https://www.mediafire.com/file/bqmw1tdd7yq3qzr/I40_MX.pkz",
            "https://www.mediafire.com/file_premium/bqmw1tdd7yq3qzr/I40_MX.pkz/file",
            "https://www.mediafire.com/download/bqmw1tdd7yq3qzr/I40_MX.pkz",
            "https://www.mediafire.com/view/bqmw1tdd7yq3qzr/I40_MX.pkz/file",
            "http://www.mediafire.com/?bqmw1tdd7yq3qzr",
        ] {
            assert_eq!(quick_key(url).as_deref(), Some("bqmw1tdd7yq3qzr"), "{url}");
        }
        assert_eq!(
            quick_key("https://www.mediafire.com/file/a1b2c3d4e5f/track.rar").as_deref(),
            Some("a1b2c3d4e5f")
        );
    }

    #[test]
    fn folder_links_are_recognised() {
        assert_eq!(
            folder_key("https://www.mediafire.com/folder/9dhrz4bkzcnzo/I40").as_deref(),
            Some("9dhrz4bkzcnzo")
        );
        assert_eq!(
            folder_key("https://www.mediafire.com/?sharekey=abc123def").as_deref(),
            Some("abc123def")
        );
        assert!(
            folder_key("https://www.mediafire.com/file/bqmw1tdd7yq3qzr/I40.pkz/file").is_none()
        );
        assert_eq!(
            folder_name("https://www.mediafire.com/folder/9dhrz4bkzcnzo/I40_MX%20Pack"),
            "I40 MX Pack"
        );
        assert_eq!(
            folder_name("https://www.mediafire.com/folder/9dhrz4bkzcnzo"),
            "mod"
        );
    }

    #[test]
    fn direct_links_need_no_resolving() {
        assert!(is_direct(
            "https://download2202.mediafire.com/abc/track.pkz"
        ));
        assert!(is_direct("https://download.mediafire.com/abc/track.pkz"));
        assert!(!is_direct(
            "https://www.mediafire.com/file/bqmw1tdd7yq3qzr/I40.pkz/file"
        ));
        assert!(is_mediafire(
            "https://download2202.mediafire.com/abc/track.pkz"
        ));
        assert!(is_mediafire("https://www.mediafire.com/file/x"));
        assert!(!is_mediafire("https://notmediafire.com/file/x"));
    }

    #[test]
    fn refusals_are_recognised() {
        let gone = json!({ "result": "Error", "message": "Unknown or Invalid QuickKey" });
        assert_eq!(
            api_error(&gone),
            Some(("MediaFire: the file no longer exists".into(), true))
        );
        assert!(api_error(&json!({ "result": "Success", "links": [] })).is_none());
        let odd =
            api_error(&json!({ "result": "Error", "message": "Rate limit exceeded" })).unwrap();
        assert!(odd.0.contains("Rate limit exceeded") && !odd.1);
        assert!(
            refusal("<p>Invalid or Deleted File.</p>")
                .unwrap()
                .permanent
        );
        assert_eq!(
            refusal("daily download limit reached")
                .unwrap()
                .retry_after_ms,
            Some(6 * 3600 * 1000)
        );
        assert!(refusal("<html><body>Download this file</body></html>").is_none());
    }

    #[test]
    fn api_direct_link_must_be_a_cdn_link() {
        let ok =
            json!({ "links": [{ "direct_download": "https://download5.mediafire.com/a/b.pkz" }] });
        assert_eq!(
            api_direct_link(&ok).as_deref(),
            Some("https://download5.mediafire.com/a/b.pkz")
        );
        let page = json!({ "links": [{ "direct_download": "https://www.mediafire.com/file/x/b.pkz/file" }] });
        assert!(api_direct_link(&page).is_none());
    }

    #[test]
    fn folder_listing_entries() {
        let response = json!({ "folder_content": {
            "files": [
                { "filename": "I40 MX.pkz", "size": "1234", "links": { "normal_download": "https://www.mediafire.com/file/aaaaaaaaaaa/I40_MX.pkz/file" } },
                { "filename": "..", "links": { "normal_download": "https://x" } },
                { "filename": "no-link.txt" }
            ],
            "more_chunks": "yes"
        }});
        let (items, more) = folder_chunk(&response, "files");
        assert!(more);
        let files: Vec<_> = items
            .iter()
            .filter_map(|i| listed_file(i, "sub/"))
            .collect();
        assert_eq!(
            files,
            vec![RemoteFile {
                rel: "sub/I40 MX.pkz".into(),
                url: "https://www.mediafire.com/file/aaaaaaaaaaa/I40_MX.pkz/file".into(),
                size: Some(1234),
            }]
        );
        let sub = json!({ "folderkey": "k1", "name": "Server: files" });
        assert_eq!(
            listed_folder(&sub, ""),
            Some(("k1".into(), "Server_ files/".into()))
        );
        assert_eq!(folder_chunk(&json!({}), "folders"), (vec![], false));
    }
}
