//! One leased job, start to finish: a post page (and the pictures it asks for), or a file.
//!
//! Files are streamed to a temporary file on disk while they are hashed, never held in RAM,
//! then PUT to R2 from disk with their length known: R2's presigned PUT needs a
//! Content-Length, and hashing first means a file the mirror already holds is never uploaded.

use crate::api::{self, Api, Failure, Job, Uploaded, WantedImage};
use crate::mediafire;
use crate::pacing::{retry_after, Pacer};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::{header, Client, Response, StatusCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// What a browser sends, near enough: these sites vary what they serve on it.
pub const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36";
const PAGE_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
/// A post page is ~150 KB; the control plane takes up to 4 MB.
const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
/// A listing page is 50 posts with their content; the control plane takes up to 8 MB.
const MAX_LIST_BYTES: usize = 8 * 1024 * 1024;

pub struct Ctx {
    pub api: Api,
    /// Pages, pictures and share pages: browser-like, with cookies and compression.
    pub web: Client,
    /// File bodies: no transparent decompression, so the bytes counted are the bytes stored.
    pub files: Client,
    /// PUTs to R2.
    pub put: Client,
    pub pacer: Pacer,
    pub tmp: PathBuf,
}

/// How a job ended, for the log.
#[derive(Debug)]
pub enum Outcome {
    Done(String),
    Failed(String),
    Deferred(Duration),
}

/// An error that carries what to tell the control plane.
#[derive(Debug)]
struct Fail(Failure);
impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.error)
    }
}
impl std::error::Error for Fail {}

fn fail(error: impl Into<String>, permanent: bool) -> anyhow::Error {
    Fail(Failure {
        error: error.into(),
        permanent,
        ..Default::default()
    })
    .into()
}

/// Turn whatever went wrong into the report the control plane takes.
fn failure_of(err: &anyhow::Error) -> Failure {
    match err.downcast_ref::<Fail>() {
        Some(f) => f.0.clone(),
        None => Failure {
            error: format!("{err:#}").chars().take(280).collect(),
            ..Default::default()
        },
    }
}

impl Ctx {
    /// One GET, paced per site. A 403/429 backs the site off and becomes a deferred failure.
    async fn get(&self, client: &Client, url: &str, accept: &str) -> Result<Response> {
        if let Err(left) = self.pacer.turn(url).await {
            return Err(deferred(left));
        }
        let res = client
            .get(url)
            .header(header::ACCEPT, accept)
            .header(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .send()
            .await?;
        let status = res.status();
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            let after = retry_after(
                res.headers()
                    .get(header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
            );
            let wait = self.pacer.refused(url, after).await;
            eprintln!("{{\"msg\":\"site refused us; backing off\",\"status\":{},\"url\":{:?},\"secs\":{}}}", status.as_u16(), url, wait.as_secs());
            return Err(Fail(Failure {
                error: format!("site answered {}", status.as_u16()),
                status: Some(status.as_u16()),
                retry_after: Some(wait),
                deferred: true,
                ..Default::default()
            })
            .into());
        }
        self.pacer.answered(url).await;
        Ok(res)
    }

    pub async fn run(&self, job: &Job) -> Outcome {
        // A site we are backing off: hand the job straight back rather than sit on its lease.
        if let Some(left) = self.pacer.blocked(&job.url).await {
            let f = Failure {
                error: "backing off this site".into(),
                status: Some(429),
                retry_after: Some(left),
                deferred: true,
                ..Default::default()
            };
            let _ = self.api.failed(&job.id, &f).await;
            return Outcome::Deferred(left);
        }
        let result = match job.kind.as_str() {
            "page" => self.page(job).await,
            "file" => self.file(job).await,
            "list" => self.list(job).await,
            other => Err(fail(format!("unknown job kind {other}"), true)),
        };
        match result {
            Ok(note) => Outcome::Done(note),
            Err(err) => {
                // The control plane turned a call down (the lease ran out, locked content):
                // it already knows; there is nothing to report.
                if let Some(status) = api::refused_status(&err) {
                    return Outcome::Failed(format!("control plane said {status}: {err:#}"));
                }
                let f = failure_of(&err);
                if let Err(e) = self.api.failed(&job.id, &f).await {
                    return Outcome::Failed(format!(
                        "{} (and reporting it failed: {e:#})",
                        f.error
                    ));
                }
                if f.deferred {
                    Outcome::Deferred(f.retry_after.unwrap_or_default())
                } else {
                    Outcome::Failed(f.error)
                }
            }
        }
    }

    // ───────────────────────────── pages ─────────────────────────────

    async fn page(&self, job: &Job) -> Result<String> {
        let res = self.get(&self.web, &job.url, PAGE_ACCEPT).await?;
        let status = res.status();
        if !status.is_success() {
            return Err(Fail(Failure {
                error: format!("page answered {}", status.as_u16()),
                status: Some(status.as_u16()),
                permanent: status == StatusCode::NOT_FOUND || status == StatusCode::GONE,
                ..Default::default()
            })
            .into());
        }
        let html = read_capped(res, MAX_HTML_BYTES)
            .await?
            .ok_or_else(|| fail("larger than a post page", false))?;
        let html = String::from_utf8_lossy(&html).into_owned();
        let wanted = self.api.page_html(&job.id, &html).await?;
        if wanted.is_empty() {
            return Ok("page read".into());
        }
        let mut uploaded = Vec::new();
        for w in &wanted {
            match self.picture(job, w).await {
                Ok(Some(up)) => uploaded.push(up),
                Ok(None) => {}
                Err(e) => eprintln!(
                    "{{\"msg\":\"picture skipped\",\"src\":{:?},\"error\":{:?}}}",
                    w.src,
                    format!("{e:#}")
                ),
            }
        }
        let answer: Value = self.api.page_done(&job.id, &uploaded).await?;
        let rejected = answer["rejected"].as_array().map(|a| a.len()).unwrap_or(0);
        Ok(format!(
            "page read, {} of {} pictures ({} rejected)",
            uploaded.len(),
            wanted.len(),
            rejected
        ))
    }

    /// One picture: fetched (paced), checked to be a picture by its first bytes, uploaded.
    async fn picture(&self, job: &Job, w: &WantedImage) -> Result<Option<Uploaded>> {
        let res = self
            .get(
                &self.web,
                &w.src,
                "image/avif,image/webp,image/png,image/jpeg,image/gif",
            )
            .await?;
        if !res.status().is_success() {
            return Ok(None);
        }
        let Some(bytes) = read_capped(res, w.max_bytes as usize).await? else {
            return Ok(None);
        };
        let Some(content_type) = sniff_image(&bytes) else {
            return Ok(None);
        };
        let up = Uploaded {
            sha256: hex(&Sha256::digest(&bytes)),
            size: bytes.len() as u64,
            content_type: content_type.into(),
            filename: url_file_name(&w.src),
            src: Some(w.src.clone()),
            purpose: Some(w.purpose.clone()),
        };
        let target = self.api.upload_url(&job.id, &up).await?;
        if !target.have {
            let url = target
                .url
                .as_deref()
                .ok_or_else(|| anyhow!("no upload URL"))?;
            let mut req = self
                .put
                .put(url)
                .header(header::CONTENT_LENGTH, bytes.len());
            for (k, v) in &target.headers {
                req = req.header(k.as_str(), v.as_str());
            }
            let res = req.body(bytes).send().await?;
            if !res.status().is_success() {
                return Err(anyhow!("R2 answered {} to the PUT", res.status().as_u16()));
            }
        }
        Ok(Some(up))
    }

    // ───────────────────────────── discovery ─────────────────────────────

    /// A WordPress REST request (category tree, listing, id sweep): its status and body go back
    /// as they came, a 400 past the end of the listing included. The control plane parses it.
    async fn list(&self, job: &Job) -> Result<String> {
        let res = self.get(&self.web, &job.url, "application/json").await?;
        let status = res.status().as_u16();
        let body = read_capped(res, MAX_LIST_BYTES)
            .await?
            .ok_or_else(|| fail("larger than a listing", false))?;
        self.api
            .list_result(&job.id, status, &String::from_utf8_lossy(&body))
            .await?;
        Ok(format!("listing answered {status}"))
    }

    // ───────────────────────────── files ─────────────────────────────

    async fn file(&self, job: &Job) -> Result<String> {
        let folder_allowed = job.folder_allowed.unwrap_or(false);
        let url = if mediafire::is_mediafire(&job.url) {
            if let Some(key) = mediafire::folder_key(&job.url) {
                if !folder_allowed {
                    return Err(fail("a folder inside a listed folder", true));
                }
                let files = self.mediafire_folder(&key).await?;
                if files.is_empty() {
                    return Err(fail("MediaFire: the folder is empty", true));
                }
                let n = files.len();
                self.api
                    .folder(&job.id, &mediafire::folder_name(&job.url), &files)
                    .await?;
                return Ok(format!("folder listed, {n} files"));
            }
            self.mediafire_link(&job.url).await?
        } else {
            job.url.clone()
        };
        self.download_and_upload(job, &url).await
    }

    /// Page first, API second: the order the app measured as the working one.
    async fn mediafire_link(&self, url: &str) -> Result<String> {
        if mediafire::is_direct(url) {
            return Ok(url.to_string());
        }
        let res = self.get(&self.web, url, PAGE_ACCEPT).await?;
        let status = res.status();
        if status == StatusCode::NOT_FOUND || status == StatusCode::GONE {
            return Err(fail(format!("MediaFire: {}", status.as_u16()), true));
        }
        if !status.is_success() {
            return Err(Fail(Failure {
                error: format!("MediaFire answered {}", status.as_u16()),
                status: Some(status.as_u16()),
                ..Default::default()
            })
            .into());
        }
        let html = res.text().await?;
        if let Some(direct) = mediafire::parse_link(&html) {
            return Ok(direct);
        }
        if let Some(key) = mediafire::quick_key(url) {
            let api = mediafire::api_url(
                "file/get_links.php",
                &format!("quick_key={key}&link_type=direct_download"),
            );
            if let Some(response) = self.mediafire_api(&api).await? {
                if let Some((msg, permanent)) = mediafire::api_error(&response) {
                    return Err(fail(msg, permanent));
                }
                if let Some(direct) = mediafire::api_direct_link(&response) {
                    return Ok(direct);
                }
            }
        }
        Err(match mediafire::refusal(&html) {
            Some(r) => Fail(Failure {
                error: r.message.into(),
                permanent: r.permanent,
                retry_after: r.retry_after_ms.map(Duration::from_millis),
                ..Default::default()
            })
            .into(),
            None => fail("MediaFire: no download link on the page", false),
        })
    }

    /// One API call's `response` object; None when it didn't come back as JSON.
    async fn mediafire_api(&self, url: &str) -> Result<Option<Value>> {
        let res = self.get(&self.web, url, "application/json").await?;
        let text = res.text().await.unwrap_or_default();
        Ok(serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("response").cloned()))
    }

    async fn mediafire_folder(&self, key: &str) -> Result<Vec<mediafire::RemoteFile>> {
        let mut out = Vec::new();
        // An explicit stack rather than recursion: (folder key, path prefix, depth).
        let mut todo = vec![(key.to_string(), String::new(), 0usize)];
        while let Some((key, prefix, depth)) = todo.pop() {
            if depth > mediafire::FOLDER_MAX_DEPTH || out.len() >= mediafire::FOLDER_MAX_FILES {
                continue;
            }
            for item in self.mediafire_content(&key, "files").await? {
                if out.len() >= mediafire::FOLDER_MAX_FILES {
                    break;
                }
                out.extend(mediafire::listed_file(&item, &prefix));
            }
            for item in self.mediafire_content(&key, "folders").await? {
                if let Some((sub, sub_prefix)) = mediafire::listed_folder(&item, &prefix) {
                    todo.push((sub, sub_prefix, depth + 1));
                }
            }
        }
        Ok(out)
    }

    async fn mediafire_content(&self, key: &str, kind: &str) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for chunk in 1..=mediafire::MAX_FOLDER_CHUNKS {
            let url = mediafire::api_url(
                "folder/get_content.php",
                &format!("folder_key={key}&content_type={kind}&chunk={chunk}&chunk_size=100"),
            );
            let response = self
                .mediafire_api(&url)
                .await?
                .ok_or_else(|| fail("MediaFire: unreadable folder listing", false))?;
            if let Some((msg, _)) = mediafire::api_error(&response) {
                return Err(fail(msg, true));
            }
            let (items, more) = mediafire::folder_chunk(&response, kind);
            out.extend(items);
            if !more {
                break;
            }
        }
        Ok(out)
    }

    async fn download_and_upload(&self, job: &Job, url: &str) -> Result<String> {
        let max = job.max_bytes.unwrap_or(4 * 1024 * 1024 * 1024);
        let res = self.get(&self.files, url, "*/*").await?;
        let status = res.status();
        if status == StatusCode::NOT_FOUND || status == StatusCode::GONE {
            return Err(fail(
                format!("the file is gone ({})", status.as_u16()),
                true,
            ));
        }
        if status.is_server_error() {
            let after = retry_after(
                res.headers()
                    .get(header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
            );
            return Err(Fail(Failure {
                error: format!("host answered {}", status.as_u16()),
                status: Some(status.as_u16()),
                retry_after: after,
                ..Default::default()
            })
            .into());
        }
        if !status.is_success() {
            return Err(fail(format!("host answered {}", status.as_u16()), false));
        }
        let content_type = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| {
                v.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "application/octet-stream".into());
        if content_type == "text/html" {
            let html = res.text().await.unwrap_or_default();
            return Err(
                match mediafire::refusal(&html).filter(|_| mediafire::is_mediafire(url)) {
                    Some(r) => fail(r.message, r.permanent),
                    None => fail(
                        "a web page instead of a file",
                        !mediafire::is_mediafire(url),
                    ),
                },
            );
        }
        let declared = res.content_length();
        if declared.is_some_and(|n| n > max) {
            self.api.too_big(&job.id).await?;
            return Ok("too big for the fetcher".into());
        }
        let filename = job
            .filename
            .clone()
            .unwrap_or_else(|| filename_from(&res, url));

        let tmp = self.tmp.join(format!("{}.part", job.id.replace(':', "-")));
        let spooled = spool(res, &tmp, max).await;
        let result = async {
            let Some((sha256, size)) = spooled? else {
                self.api.too_big(&job.id).await?;
                return Ok("too big for the fetcher".to_string());
            };
            if let Some(n) = declared.filter(|n| *n != size) {
                return Err(fail(format!("short body: {size} of {n} bytes"), false));
            }
            if size == 0 {
                return Err(fail("empty file", true));
            }
            let up = Uploaded {
                sha256,
                size,
                content_type,
                filename,
                src: None,
                purpose: None,
            };
            let target = self.api.upload_url(&job.id, &up).await?;
            if !target.have {
                let url = target
                    .url
                    .as_deref()
                    .ok_or_else(|| anyhow!("no upload URL"))?;
                put_file(&self.put, url, &target.headers, &tmp, size).await?;
            }
            self.api.file_done(&job.id, &up).await?;
            Ok(format!(
                "{} ({} bytes{})",
                up.filename,
                size,
                if target.have { ", already held" } else { "" }
            ))
        }
        .await;
        let _ = tokio::fs::remove_file(&tmp).await;
        result
    }
}

fn deferred(left: Duration) -> anyhow::Error {
    Fail(Failure {
        error: "backing off this site".into(),
        status: Some(429),
        retry_after: Some(left),
        deferred: true,
        ..Default::default()
    })
    .into()
}

/// The body into `path`, hashed as it passes. None once it passes `max` bytes (and the rest is
/// never downloaded). Memory stays at one network chunk.
async fn spool(res: Response, path: &PathBuf, max: u64) -> Result<Option<(String, u64)>> {
    let mut out = tokio::fs::File::create(path).await?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut body = res.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        size += chunk.len() as u64;
        if size > max {
            return Ok(None);
        }
        hash.update(&chunk);
        out.write_all(&chunk).await?;
    }
    out.flush().await?;
    Ok(Some((hex(&hash.finalize()), size)))
}

/// A PUT streamed from disk, with the headers the presigned URL was signed with.
async fn put_file(
    client: &Client,
    url: &str,
    headers: &std::collections::HashMap<String, String>,
    path: &PathBuf,
    size: u64,
) -> Result<()> {
    let file = tokio::fs::File::open(path).await?;
    let stream = tokio_util::io::ReaderStream::with_capacity(file, 256 * 1024);
    let mut req = client.put(url).header(header::CONTENT_LENGTH, size);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let res = req.body(reqwest::Body::wrap_stream(stream)).send().await?;
    if !res.status().is_success() {
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        return Err(anyhow!(
            "R2 answered {status} to the PUT: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    Ok(())
}

/// A response body, or None once it passes `cap` bytes.
async fn read_capped(res: Response, cap: usize) -> Result<Option<Vec<u8>>> {
    if res.content_length().is_some_and(|n| n as usize > cap) {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut body = res.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        if out.len() + chunk.len() > cap {
            return Ok(None);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Some(out))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A picture type by its first bytes, as the Worker sniffs it. Never SVG.
pub fn sniff_image(b: &[u8]) -> Option<&'static str> {
    let at = |i: usize, s: &[u8]| b.len() >= i + s.len() && &b[i..i + s.len()] == s;
    if at(0, &[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if at(0, &[0x89]) && at(1, b"PNG") {
        Some("image/png")
    } else if at(0, b"RIFF") && at(8, b"WEBP") {
        Some("image/webp")
    } else if at(0, b"GIF8") {
        Some("image/gif")
    } else if at(4, b"ftypavif") || at(4, b"ftypavis") {
        Some("image/avif")
    } else {
        None
    }
}

/// The last path segment of a URL, stepping over MediaFire's `/file` and Drive's `/view`.
pub fn url_file_name(url: &str) -> String {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    let mut segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut last = segs.pop().unwrap_or("");
    if ["file", "view", "download"]
        .iter()
        .any(|w| last.eq_ignore_ascii_case(w))
    {
        last = segs.pop().unwrap_or(last);
    }
    let name = mediafire::sanitize(&mediafire::percent_decode(last));
    if name.is_empty() || name.contains("://") {
        "download.bin".into()
    } else {
        name
    }
}

/// The file's own name: Content-Disposition, else the URL it finally came from.
fn filename_from(res: &Response, url: &str) -> String {
    let cd = res
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(name) = disposition_name(cd) {
        return name;
    }
    let from = res.url().as_str();
    let name = url_file_name(if from.is_empty() { url } else { from });
    if name.contains('.') {
        name
    } else {
        url_file_name(url)
    }
}

pub fn disposition_name(cd: &str) -> Option<String> {
    let lower = cd.to_ascii_lowercase();
    let star = lower.find("filename*=").map(|i| {
        let v = cd[i + 10..]
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        let v = v
            .strip_prefix("UTF-8''")
            .or_else(|| v.strip_prefix("utf-8''"))
            .unwrap_or(v);
        mediafire::percent_decode(v)
    });
    let plain = || {
        let i = lower.find("filename=")?;
        Some(
            cd[i + 9..]
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string(),
        )
    };
    let raw = star.or_else(plain)?;
    let name = mediafire::sanitize(raw.trim());
    mediafire::usable_name(&name).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_pictures_and_nothing_else() {
        assert_eq!(sniff_image(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
        assert_eq!(sniff_image(b"RIFF\x08\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"GIF89a"), Some("image/gif"));
        assert_eq!(sniff_image(b"\0\0\0\x1cftypavif"), Some("image/avif"));
        assert_eq!(sniff_image(b"<svg onload=alert(1)>"), None);
        assert_eq!(sniff_image(b""), None);
    }

    #[test]
    fn names_from_urls_and_headers() {
        assert_eq!(
            url_file_name("https://www.mediafire.com/file/abc/I40_MX.pkz/file"),
            "I40_MX.pkz"
        );
        assert_eq!(
            url_file_name("https://download9.mediafire.com/x/y/Red%20Bull.pnt?dl=1"),
            "Red Bull.pnt"
        );
        assert_eq!(
            disposition_name("attachment; filename=\"Track v2.pkz\""),
            Some("Track v2.pkz".into())
        );
        assert_eq!(
            disposition_name("attachment; filename*=UTF-8''Caf%C3%A9.zip"),
            Some("Café.zip".into())
        );
        assert_eq!(
            disposition_name("attachment; filename=\"a/../b.zip\""),
            Some("a_.._b.zip".into())
        );
        assert_eq!(disposition_name("inline"), None);
    }

    #[test]
    fn hashes_are_lowercase_hex() {
        assert_eq!(
            hex(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
