//! How big a mod's download is, asked of the host before anything is downloaded.
//!
//! No catalogue publishes a size, so the only honest source is the file's host. The link is
//! resolved the way an install resolves it, then asked with `HEAD`; hosts that refuse `HEAD`
//! are asked for the first byte with a `Range` request, whose `Content-Range` carries the
//! total. A host that will not say — a web page where a file should be, a chunked response,
//! an encrypted share — yields `None`, and the interface shows nothing rather than a guess.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::install;

/// The whole probe, link resolution included. A size is a nicety on a page that is already
/// usable, so it gives up quickly.
const PROBE_BUDGET: Duration = Duration::from_secs(10);

/// The total size in bytes out of a `Content-Range` value such as `bytes 0-0/52428800`.
/// `*` (unknown total) and anything malformed give `None`.
pub fn content_range_total(value: &str) -> Option<u64> {
    let total = value.trim().strip_prefix("bytes")?.rsplit('/').next()?.trim();
    total.parse::<u64>().ok().filter(|n| *n > 0)
}

/// The file size a response states, if it states one about a *file*.
///
/// - A web page is not the file: hosts answer a refusal or a confirm form with an ordinary
///   200 HTML page, and its length says nothing about the download.
/// - `206` carries the total in `Content-Range`; its `Content-Length` is the slice.
/// - Zero is not a size: some hosts answer `HEAD` with `content-length: 0` for a file they
///   hold fine.
pub fn size_from_response(
    status: u16,
    content_length: Option<u64>,
    content_range: Option<&str>,
    content_type: Option<&str>,
) -> Option<u64> {
    if content_type.is_some_and(|c| c.trim().to_ascii_lowercase().starts_with("text/html")) {
        return None;
    }
    match status {
        206 => content_range.and_then(content_range_total),
        200..=299 => content_length.filter(|n| *n > 0),
        _ => None,
    }
}

fn cache() -> &'static Mutex<HashMap<String, u64>> {
    static CACHE: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn header(resp: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

async fn ask(client: &reqwest::Client, url: &str) -> Option<u64> {
    use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE};
    if let Ok(resp) = client.head(url).send().await {
        let status = resp.status().as_u16();
        let found = size_from_response(
            status,
            resp.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok()),
            header(&resp, CONTENT_RANGE).as_deref(),
            header(&resp, CONTENT_TYPE).as_deref(),
        );
        if found.is_some() {
            return found;
        }
    }
    // HEAD refused or silent: take the first byte and read the total off the answer. The body
    // is dropped unread, so at most one byte of it crosses the wire.
    let resp = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
        .ok()?;
    let status = resp.status().as_u16();
    size_from_response(
        status,
        resp.content_length(),
        header(&resp, CONTENT_RANGE).as_deref(),
        header(&resp, CONTENT_TYPE).as_deref(),
    )
}

/// The size of the file behind a mod's download link, or `None` when the host will not say.
pub async fn probe(url: &str, host: &str) -> Option<u64> {
    let scheme = url.trim_start().to_ascii_lowercase();
    if !(scheme.starts_with("https://") || scheme.starts_with("http://")) {
        return None;
    }
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(url).copied()) {
        return Some(hit);
    }
    let size = tokio::time::timeout(PROBE_BUDGET, async {
        let client = install::build_client().ok()?;
        // Folder shares and encrypted shares have no single file to measure.
        let direct = match install::resolve_share(&client, url, host).await.ok()? {
            install::Resolved::File(u) => u,
            install::Resolved::Folder { .. } => return None,
        };
        ask(&client, &direct).await
    })
    .await
    .ok()
    .flatten()?;
    if let Ok(mut c) = cache().lock() {
        c.insert(url.to_string(), size);
    }
    Some(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_total_reads_the_whole_size() {
        assert_eq!(content_range_total("bytes 0-0/52428800"), Some(52_428_800));
        assert_eq!(content_range_total("bytes */1024"), Some(1024));
        assert_eq!(content_range_total("bytes 0-0/*"), None);
        assert_eq!(content_range_total("items 0-0/5"), None);
        assert_eq!(content_range_total("bytes 0-0/0"), None);
        assert_eq!(content_range_total(""), None);
    }

    #[test]
    fn a_file_response_gives_its_length() {
        assert_eq!(
            size_from_response(200, Some(1_000), None, Some("application/zip")),
            Some(1_000)
        );
        assert_eq!(size_from_response(200, Some(1_000), None, None), Some(1_000));
    }

    #[test]
    fn a_partial_response_gives_the_total_not_the_slice() {
        assert_eq!(
            size_from_response(206, Some(1), Some("bytes 0-0/9000"), Some("application/zip")),
            Some(9_000)
        );
        assert_eq!(size_from_response(206, Some(1), None, None), None);
    }

    #[test]
    fn a_web_page_a_zero_or_an_error_is_no_size() {
        assert_eq!(
            size_from_response(200, Some(40_000), None, Some("text/html; charset=utf-8")),
            None
        );
        assert_eq!(size_from_response(200, Some(0), None, None), None);
        assert_eq!(size_from_response(200, None, None, None), None);
        assert_eq!(size_from_response(403, Some(10), None, None), None);
    }
}
