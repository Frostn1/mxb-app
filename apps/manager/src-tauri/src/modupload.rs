//! Uploading a rider's own mod to the mxbsecure catalogue.
//!
//! The control plane (`control-plane/src/uploads.ts`) opens a session and hands back presigned
//! R2 multipart part URLs; the bytes go from here straight to R2, a part at a time, read from
//! disk in Rust. The webview never holds the file.
//!
//!   POST   /v1/uploads                 uploads.ts:222  open: metadata, size, SHA-256 in; part URLs out
//!   GET    /v1/uploads/<id>            uploads.ts:289  state; for an open one, parts held + fresh URLs
//!   POST   /v1/uploads/<id>/complete   uploads.ts:310  part ETags in; queued for checking
//!   DELETE /v1/uploads/<id>            uploads.ts:367  abandon
//!   GET    /v1/me/mods                 uploads.ts:378  the rider's mods, open uploads, quota
//!   PATCH  /v1/assets/<uuid>           uploads.ts      edit title/description/bike/visibility
//!   PUT    /v1/assets/<uuid>/thumb     uploads.ts      set the mod's picture
//!   DELETE /v1/assets/<uuid>           uploads.ts      delete
//!
//! A mod is named by its public UUID (`control-plane/src/modids.ts`), never a number.
//!
//! A session is written to `mod-uploads.json` as soon as it is opened and after every part, so
//! a pause or an app restart resumes from what R2 already holds (`GET /v1/uploads/<id>` lists
//! it). After completion the upload is quarantined and checked by the mxb-mirror worker
//! (`uploadcheck.ts` `verifyUpload`); we poll until it is `live` or `rejected`.

use anyhow::Context;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

/// `uploads.ts:31` `MAX_UPLOAD_BYTES`.
pub const MAX_UPLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// `modscan.ts:31` `MAX_PNT_BYTES`: a `.pnt` has its own, smaller limit.
pub const MAX_PNT_BYTES: u64 = 64 * 1024 * 1024;
/// The file kinds the control plane takes (`uploads.ts:41` `KINDS`).
pub const KINDS: [&str; 3] = ["pkz", "zip", "pnt"];

/// Where the progress events go.
pub const EVENT: &str = "mod-upload";
const STORE: &str = "mod-uploads.json";
/// How often a checking upload is asked about.
const POLL: Duration = Duration::from_secs(5);
/// How often the progress bar is told.
const TICK: Duration = Duration::from_millis(250);

/// How parts go out. A struct so the tests can make the retry wait instant.
#[derive(Clone, Debug)]
pub struct Tuning {
    /// Parts in flight at once.
    pub concurrency: usize,
    /// Tries per part before the upload stops (it can be resumed).
    pub attempts: u32,
    /// Grows with each retry.
    pub retry_wait: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self { concurrency: 3, attempts: 4, retry_wait: Duration::from_secs(2) }
    }
}

// ───────────────────────────── part plan ─────────────────────────────

/// One part: its 1-based number, where it starts and how long it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub part: u32,
    pub offset: u64,
    pub len: u64,
}

/// The parts a file of `size` is cut into, the same count as the control plane's `partCount`
/// (`uploads.ts:196`): `max(1, ceil(size / part_size))`.
pub fn part_plan(size: u64, part_size: u64) -> Vec<Span> {
    let part_size = part_size.max(1);
    let n = size.div_ceil(part_size).max(1);
    (0..n)
        .map(|i| {
            let offset = i * part_size;
            Span { part: i as u32 + 1, offset, len: part_size.min(size.saturating_sub(offset)) }
        })
        .collect()
}

/// The parts still to send: every part of the plan without an ETag.
pub fn missing(plan: &[Span], etags: &BTreeMap<u32, String>) -> Vec<Span> {
    plan.iter().copied().filter(|s| !etags.contains_key(&s.part)).collect()
}

/// `kindOf` (`uploads.ts:91`): the extension, if it is one the control plane takes.
pub fn kind_of(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    KINDS.iter().copied().find(|k| *k == ext)
}

/// The size limit for a kind.
pub fn limit_for(kind: &str) -> u64 {
    if kind == "pnt" {
        MAX_PNT_BYTES
    } else {
        MAX_UPLOAD_BYTES
    }
}

// ───────────────────────────── API shapes ─────────────────────────────

/// What the rider fills in. Field names are the control plane's (`uploads.ts:97` `parseOpen`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadMeta {
    pub title: String,
    #[serde(rename = "type")]
    pub mod_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub bike: String,
    pub visibility: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// A new version of this mod of yours (its public id); none for a new mod.
    #[serde(default, deserialize_with = "public_id")]
    pub asset_id: Option<String>,
    /// A picture to set on the mod once the upload is in. Optional.
    #[serde(default)]
    pub thumb_path: Option<String>,
}

/// A mod's public id, or none. Jobs saved by an older build hold a number there, which no
/// longer names anything: read as none rather than failing to load the saved list.
fn public_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(v.and_then(|v| v.as_str().map(str::to_string)))
}

/// The largest picture the control plane takes (`mirror.ts` `MAX_THUMB_BYTES`).
pub const MAX_THUMB_BYTES: u64 = 2 * 1024 * 1024;

/// The picture's content type by its first bytes, as the control plane checks it.
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [_, _, _, _, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f' | b's', ..] => Some("image/avif"),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PartUrl {
    pub part: u32,
    pub url: String,
}

/// `POST /v1/uploads` 201 (`uploads.ts:272`).
#[derive(Clone, Debug, Deserialize)]
pub struct Opened {
    pub id: String,
    pub part_size: u64,
    pub parts: Vec<PartUrl>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HeldPart {
    pub part: u32,
    pub etag: String,
}

/// `GET /v1/uploads/<id>` (`uploads.ts:292`). `parts` and `uploaded` only while open.
#[derive(Clone, Debug, Deserialize)]
pub struct Status {
    pub state: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, deserialize_with = "public_id")]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub uploaded: Option<Vec<HeldPart>>,
    #[serde(default)]
    pub parts: Option<Vec<PartUrl>>,
}

/// `POST /v1/uploads/<id>/complete` 202 (`uploads.ts:353`).
#[derive(Clone, Debug, Deserialize)]
pub struct Completed {
    pub state: String,
    #[serde(default, deserialize_with = "public_id")]
    pub asset_id: Option<String>,
}

/// One of the rider's mods (`uploads.ts:380`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyMod {
    /// The public id (a UUID).
    pub id: String,
    pub title: String,
    #[serde(rename(deserialize = "type"))]
    pub mod_type: String,
    pub visibility: String,
    pub state: String,
    pub modified: String,
    /// A version has been published.
    #[serde(default)]
    pub live: bool,
    /// The picture on the CDN, if it has one.
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub reports: i64,
}

/// An upload still open, being checked, or rejected (`uploads.ts:387`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyUpload {
    pub id: String,
    #[serde(default, alias = "asset_id", deserialize_with = "public_id")]
    pub asset_id: Option<String>,
    pub filename: String,
    pub size: u64,
    pub state: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(alias = "created_at")]
    pub created_at: i64,
}

/// `QUOTA` (`uploads.ts:35`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub open_sessions: u64,
    pub uploads_per_day: u64,
    pub bytes_per_day: u64,
    pub storage_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyMods {
    pub mods: Vec<MyMod>,
    pub uploads: Vec<MyUpload>,
    pub quota: Quota,
}

/// What the owner may change (`uploads.ts:395` `editMod`). Absent fields are left alone.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bike: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
}

/// What the edit form fills itself from (`modapi.ts:243`). `bike` is the asset's `; ` list.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModDetails {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub bike: Vec<String>,
}

/// A refusal, with what the frontend needs to say why.
///
/// `signin`: no account token, or the control plane doesn't know it. `blocked`: the estate
/// gate's refusal (`index.ts:457`, `bans.ts` `appBlocked`), carrying its own message.
/// Anything else is the control plane's own `error` text.
#[derive(Debug)]
pub enum ApiError {
    SignIn,
    Blocked(String),
    Refused(u16, String),
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::SignIn => write!(f, "signin"),
            ApiError::Blocked(m) => write!(f, "blocked:{m}"),
            ApiError::Refused(_, m) => write!(f, "{m}"),
            ApiError::Network(m) => write!(f, "network:{m}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// The control plane, as the app's account sees it. `base` is a parameter so tests can point
/// it at a local stand-in.
#[derive(Clone)]
pub struct Api {
    pub client: Client,
    pub base: String,
    pub token: String,
}

impl Api {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base.trim_end_matches('/'))
    }

    async fn call<T: serde::de::DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T, ApiError> {
        if self.token.trim().is_empty() {
            return Err(ApiError::SignIn);
        }
        let resp = req
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if status.is_success() {
            return serde_json::from_str(&text).map_err(|e| ApiError::Network(format!("bad response: {e}")));
        }
        let body: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let message = body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| status.to_string());
        if status.as_u16() == 401 {
            return Err(ApiError::SignIn);
        }
        if status.as_u16() == 403 && body.get("code").and_then(|c| c.as_str()) == Some("blocked") {
            crate::gate::note_refusal(403, Some(&text));
            return Err(ApiError::Blocked(message));
        }
        Err(ApiError::Refused(status.as_u16(), message))
    }

    pub async fn open(&self, filename: &str, size: u64, sha256: &str, meta: &UploadMeta) -> Result<Opened, ApiError> {
        let mut body = serde_json::json!({
            "filename": filename,
            "size": size,
            "sha256": sha256,
            "title": meta.title,
            "type": meta.mod_type,
            "description": meta.description,
            "bike": meta.bike,
            "visibility": meta.visibility,
        });
        if let Some(v) = meta.version.as_deref().filter(|v| !v.trim().is_empty()) {
            body["version"] = v.trim().into();
        }
        if let Some(n) = meta.notes.as_deref().filter(|n| !n.trim().is_empty()) {
            body["notes"] = n.trim().into();
        }
        if let Some(id) = meta.asset_id.as_deref() {
            body["asset_id"] = id.into();
        }
        self.call(self.client.post(self.url("/v1/uploads")).json(&body)).await
    }

    pub async fn status(&self, id: &str) -> Result<Status, ApiError> {
        self.call(self.client.get(self.url(&format!("/v1/uploads/{id}")))).await
    }

    pub async fn complete(&self, id: &str, etags: &BTreeMap<u32, String>) -> Result<Completed, ApiError> {
        let parts: Vec<_> = etags.iter().map(|(p, e)| serde_json::json!({ "part": p, "etag": e })).collect();
        self.call(
            self.client
                .post(self.url(&format!("/v1/uploads/{id}/complete")))
                .json(&serde_json::json!({ "parts": parts })),
        )
        .await
    }

    pub async fn abort(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.call(self.client.delete(self.url(&format!("/v1/uploads/{id}")))).await
    }

    pub async fn my_mods(&self) -> Result<MyMods, ApiError> {
        self.call(self.client.get(self.url("/v1/me/mods"))).await
    }

    pub async fn edit(&self, asset_id: &str, edit: &ModEdit) -> Result<serde_json::Value, ApiError> {
        self.call(self.client.patch(self.url(&format!("/v1/assets/{asset_id}"))).json(edit)).await
    }

    /// The public page's description and bikes (`modapi.ts:181` `getAsset`), to fill the edit
    /// form. Only a mod with a live version answers.
    pub async fn details(&self, asset_id: &str) -> Result<ModDetails, ApiError> {
        self.call(self.client.get(self.url(&format!("/v1/assets/{asset_id}")))).await
    }

    pub async fn delete(&self, asset_id: &str) -> Result<serde_json::Value, ApiError> {
        self.call(self.client.delete(self.url(&format!("/v1/assets/{asset_id}")))).await
    }

    /// Set the mod's picture: the image file's bytes as the body (`uploads.ts` `setThumb`).
    pub async fn set_thumb(&self, asset_id: &str, bytes: Vec<u8>) -> Result<serde_json::Value, ApiError> {
        let Some(kind) = image_type(&bytes) else {
            return Err(ApiError::Refused(415, "the picture must be a JPEG, PNG, WebP, GIF or AVIF".into()));
        };
        if bytes.len() as u64 > MAX_THUMB_BYTES {
            return Err(ApiError::Refused(413, "the picture is larger than 2 MB".into()));
        }
        let req = self.client.put(self.url(&format!("/v1/assets/{asset_id}/thumb"))).header("content-type", kind).body(bytes);
        self.call(req).await
    }
}

// ───────────────────────────── sending parts ─────────────────────────────

/// How a run of parts stopped short.
#[derive(Debug)]
pub enum PartError {
    /// The rider paused it.
    Paused,
    /// R2 refused a presigned URL (they last six hours, `uploads.ts:34`). Fresh ones come from
    /// `GET /v1/uploads/<id>`.
    Expired,
    Failed(anyhow::Error),
}

/// Bytes on their way: parts stored plus how far into each in-flight part the body has got.
#[derive(Default)]
pub struct Meter {
    stored: AtomicU64,
    inflight: Mutex<HashMap<u32, Arc<AtomicU64>>>,
}

impl Meter {
    pub fn with_stored(stored: u64) -> Self {
        Self { stored: AtomicU64::new(stored), ..Default::default() }
    }

    pub fn sent(&self) -> u64 {
        let inflight: u64 = self.inflight.lock().unwrap().values().map(|c| c.load(Relaxed)).sum();
        self.stored.load(Relaxed) + inflight
    }

    fn begin(&self, part: u32) -> Arc<AtomicU64> {
        let c = Arc::new(AtomicU64::new(0));
        self.inflight.lock().unwrap().insert(part, c.clone());
        c
    }

    fn end(&self, part: u32, stored: Option<u64>) {
        self.inflight.lock().unwrap().remove(&part);
        if let Some(n) = stored {
            self.stored.fetch_add(n, Relaxed);
        }
    }
}

/// Resolves once `cancel` reads true. Never, if the sender is gone.
async fn cancelled(mut cancel: tokio::sync::watch::Receiver<bool>) {
    loop {
        if *cancel.borrow_and_update() {
            return;
        }
        if cancel.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Send `spans` of `file` to their URLs, `tuning.concurrency` at a time. Each part's ETag is
/// handed to `on_part` as soon as R2 takes it, so the caller can persist it.
pub async fn send_parts(
    client: &Client,
    file: &Path,
    spans: &[Span],
    urls: &HashMap<u32, String>,
    tuning: &Tuning,
    meter: &Meter,
    cancel: tokio::sync::watch::Receiver<bool>,
    on_part: &(dyn Fn(u32, String) + Sync),
) -> Result<(), PartError> {
    // By value: a closure over `&Span` makes the future not `Send` for every lifetime.
    let jobs = spans.to_vec().into_iter().map(|span: Span| {
        let cancel = cancel.clone();
        async move {
            let url = urls
                .get(&span.part)
                .ok_or_else(|| PartError::Failed(anyhow::anyhow!("no URL for part {}", span.part)))?;
            if *cancel.borrow() {
                return Err(PartError::Paused);
            }
            let path = file.to_path_buf();
            let bytes = tokio::task::spawn_blocking(move || read_span(&path, span))
                .await
                .map_err(|e| PartError::Failed(anyhow::anyhow!("reading the file failed: {e}")))?
                .context("reading the file")
                .map_err(PartError::Failed)?;
            let counter = meter.begin(span.part);
            let put = put_part(client, url, Arc::new(bytes), counter, tuning, span.part);
            let r = match futures_util::future::select(std::pin::pin!(put), std::pin::pin!(cancelled(cancel))).await {
                futures_util::future::Either::Left((r, _)) => r,
                futures_util::future::Either::Right(_) => Err(PartError::Paused),
            };
            meter.end(span.part, r.as_ref().ok().map(|_| span.len));
            r.map(|etag| (span.part, etag))
        }
    });
    let mut stream = futures_util::stream::iter(jobs).buffer_unordered(tuning.concurrency.max(1));
    while let Some(r) = stream.next().await {
        let (part, etag) = r?;
        on_part(part, etag);
    }
    Ok(())
}

fn read_span(path: &Path, span: Span) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(span.offset))?;
    let mut buf = Vec::with_capacity(span.len as usize);
    f.take(span.len).read_to_end(&mut buf)?;
    if buf.len() as u64 != span.len {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the file is shorter than it was"));
    }
    Ok(buf)
}

/// PUT one part, retrying a dropped connection, a 5xx, 408 or 429. Returns R2's ETag as sent,
/// quotes included, which is how `ListParts` reports it too (`uploads.ts:191`).
async fn put_part(
    client: &Client,
    url: &str,
    bytes: Arc<Vec<u8>>,
    counter: Arc<AtomicU64>,
    tuning: &Tuning,
    part: u32,
) -> Result<String, PartError> {
    let attempts = tuning.attempts.max(1);
    let mut last = String::new();
    for attempt in 1..=attempts {
        let sent = client
            .put(url)
            .header(reqwest::header::CONTENT_LENGTH, bytes.len())
            .body(counted_body(bytes.clone(), counter.clone()))
            .send()
            .await;
        match sent {
            Ok(resp) if resp.status().is_success() => {
                let etag = resp
                    .headers()
                    .get(reqwest::header::ETAG)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                return etag.ok_or_else(|| PartError::Failed(anyhow::anyhow!("R2 sent no ETag for part {part}")));
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if status == reqwest::StatusCode::FORBIDDEN {
                    return Err(PartError::Expired);
                }
                let busy = status.is_server_error()
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || status == reqwest::StatusCode::REQUEST_TIMEOUT;
                if !busy {
                    return Err(PartError::Failed(anyhow::anyhow!("part {part} was refused (HTTP {status}): {}", body.trim())));
                }
                last = format!("HTTP {status}");
            }
            Err(e) => last = e.to_string(),
        }
        if attempt < attempts {
            tokio::time::sleep(tuning.retry_wait * attempt).await;
        }
    }
    Err(PartError::Failed(anyhow::anyhow!("part {part} failed after {attempts} tries: {last}")))
}

/// `bytes` as a body that records how far the connection has got. A retry starts at zero.
fn counted_body(bytes: Arc<Vec<u8>>, sent: Arc<AtomicU64>) -> reqwest::Body {
    const CHUNK: usize = 64 * 1024;
    sent.store(0, Relaxed);
    let len = bytes.len();
    let chunks = (0..len).step_by(CHUNK).map(move |at| {
        let end = (at + CHUNK).min(len);
        sent.store(end as u64, Relaxed);
        Ok::<_, std::io::Error>(bytes[at..end].to_vec())
    });
    reqwest::Body::wrap_stream(futures_util::stream::iter(chunks))
}

/// SHA-256 of a file, streamed, reporting bytes read.
pub fn sha256_file(path: &Path, on_read: &dyn Fn(u64)) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
        on_read(total);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

// ───────────────────────────── one upload, end to end ─────────────────────────────

/// Where an upload stands, as the screen shows it.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Hashing,
    Uploading,
    Paused,
    Completing,
    Checking,
    Live,
    Rejected,
    Failed,
}

/// An upload this app started, persisted across restarts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// Ours, stable from the first click.
    pub key: String,
    /// The control plane's session id, once opened.
    #[serde(default)]
    pub upload_id: Option<String>,
    pub path: PathBuf,
    pub filename: String,
    pub size: u64,
    /// The file's modified time when it was hashed: a resume of a changed file is refused.
    #[serde(default)]
    pub modified: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub part_size: u64,
    #[serde(default)]
    pub etags: BTreeMap<u32, String>,
    pub meta: UploadMeta,
    pub phase: Phase,
    #[serde(default)]
    pub sent: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, deserialize_with = "public_id")]
    pub asset_id: Option<String>,
    pub started_at: u64,
}

impl Job {
    fn stored_bytes(&self) -> u64 {
        let plan = part_plan(self.size, self.part_size);
        plan.iter().filter(|s| self.etags.contains_key(&s.part)).map(|s| s.len).sum()
    }
}

/// What the driver tells its owner: a changed job to save and show.
pub trait Sink: Sync {
    fn update(&self, job: &Job);
}

/// Open the session if there isn't one, send what R2 doesn't hold, complete it. Leaves the job
/// `Checking`, `Paused`, `Rejected` (refused at completion) or `Failed`.
pub async fn drive(
    api: &Api,
    job: &mut Job,
    tuning: &Tuning,
    cancel: tokio::sync::watch::Receiver<bool>,
    sink: &dyn Sink,
) {
    if let Err(e) = drive_inner(api, job, tuning, cancel, sink).await {
        match e {
            Stop::Paused => job.phase = Phase::Paused,
            Stop::Rejected(m) => {
                job.phase = Phase::Rejected;
                job.error = Some(m);
            }
            Stop::Failed(m) => {
                job.phase = Phase::Failed;
                job.error = Some(m);
            }
        }
        sink.update(job);
    }
}

enum Stop {
    Paused,
    Rejected(String),
    Failed(String),
}

impl From<ApiError> for Stop {
    fn from(e: ApiError) -> Self {
        Stop::Failed(e.to_string())
    }
}

async fn drive_inner(
    api: &Api,
    job: &mut Job,
    tuning: &Tuning,
    cancel: tokio::sync::watch::Receiver<bool>,
    sink: &dyn Sink,
) -> Result<(), Stop> {
    job.error = None;
    let urls: Vec<PartUrl> = match job.upload_id.clone() {
        None => {
            let o = api.open(&job.filename, job.size, &job.sha256, &job.meta).await?;
            job.upload_id = Some(o.id);
            job.part_size = o.part_size;
            job.etags.clear();
            o.parts
        }
        Some(id) => {
            let s = api.status(&id).await?;
            if let Some(stop) = settled(job, &s) {
                return stop;
            }
            if s.state != "open" {
                // Completed earlier and now being checked.
                job.phase = Phase::Checking;
                sink.update(job);
                return Ok(());
            }
            // R2's own list is the truth about what it holds.
            job.etags = s.uploaded.unwrap_or_default().into_iter().map(|p| (p.part, p.etag)).collect();
            s.parts.unwrap_or_default()
        }
    };
    job.phase = Phase::Uploading;
    job.sent = job.stored_bytes();
    sink.update(job);

    let id = job.upload_id.clone().unwrap_or_default();
    let mut urls: HashMap<u32, String> = urls.into_iter().map(|p| (p.part, p.url)).collect();
    let mut refreshed = false;
    loop {
        let plan = part_plan(job.size, job.part_size);
        let todo = missing(&plan, &job.etags);
        if todo.is_empty() {
            break;
        }
        let meter = Meter::with_stored(job.stored_bytes());
        let shared = Mutex::new(job.clone());
        let on_part = |part: u32, etag: String| {
            let mut j = shared.lock().unwrap();
            j.etags.insert(part, etag);
            j.sent = meter.sent();
            sink.update(&j);
        };
        let send = send_parts(&api.client, &job.path, &todo, &urls, tuning, &meter, cancel.clone(), &on_part);
        let ticker = async {
            loop {
                tokio::time::sleep(TICK).await;
                let mut j = shared.lock().unwrap();
                j.sent = meter.sent();
                sink.update(&j);
            }
        };
        let r = match futures_util::future::select(std::pin::pin!(send), std::pin::pin!(ticker)).await {
            futures_util::future::Either::Left((r, _)) => r,
            futures_util::future::Either::Right(_) => unreachable!("the ticker never stops"),
        };
        *job = shared.into_inner().unwrap();
        job.sent = job.stored_bytes();
        match r {
            Ok(()) => {}
            Err(PartError::Paused) => return Err(Stop::Paused),
            Err(PartError::Expired) if !refreshed => {
                refreshed = true;
                let s = api.status(&id).await?;
                if let Some(stop) = settled(job, &s) {
                    return stop;
                }
                job.etags = s.uploaded.unwrap_or_default().into_iter().map(|p| (p.part, p.etag)).collect();
                urls = s.parts.unwrap_or_default().into_iter().map(|p| (p.part, p.url)).collect();
            }
            Err(PartError::Expired) => return Err(Stop::Failed("the upload links expired".into())),
            Err(PartError::Failed(e)) => return Err(Stop::Failed(format!("{e:#}"))),
        }
    }

    job.phase = Phase::Completing;
    job.sent = job.size;
    sink.update(job);
    match api.complete(&id, &job.etags).await {
        Ok(c) => {
            job.asset_id = c.asset_id.or(job.asset_id.take());
            job.phase = if c.state == "live" { Phase::Live } else { Phase::Checking };
            sink.update(job);
            send_thumb(api, job).await;
            Ok(())
        }
        // The control plane rejects a completion whose parts don't add up (`uploads.ts:336`).
        Err(ApiError::Refused(400, m)) => Err(Stop::Rejected(m)),
        Err(e) => Err(e.into()),
    }
}

/// The picture the rider picked, onto the mod the upload made. A picture that can't be read or
/// is refused doesn't fail the upload: the rider can set one later from My mods.
async fn send_thumb(api: &Api, job: &Job) {
    let (Some(path), Some(id)) = (job.meta.thumb_path.as_deref(), job.asset_id.as_deref()) else { return };
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            if let Err(e) = api.set_thumb(id, bytes).await {
                log::warn!("[mods] the picture for {id} was not set: {e}");
            }
        }
        Err(e) => log::warn!("[mods] couldn't read the picture {path}: {e}"),
    }
}

/// A session that is over: expired, aborted, or already decided.
fn settled(job: &mut Job, s: &Status) -> Option<Result<(), Stop>> {
    job.asset_id = s.asset_id.clone().or(job.asset_id.take());
    match s.state.as_str() {
        "live" => {
            job.phase = Phase::Live;
            Some(Ok(()))
        }
        "rejected" => Some(Err(Stop::Rejected(s.error.clone().unwrap_or_default()))),
        "expired" => Some(Err(Stop::Failed("the upload expired".into()))),
        "aborted" => Some(Err(Stop::Failed("the upload was cancelled".into()))),
        _ => None,
    }
}

/// Ask about a quarantined upload until the check decides. Returns true when it did.
pub async fn poll_once(api: &Api, job: &mut Job) -> bool {
    let Some(id) = job.upload_id.clone() else { return true };
    match api.status(&id).await {
        Ok(s) => {
            job.asset_id = s.asset_id.clone().or(job.asset_id.take());
            match s.state.as_str() {
                "live" => {
                    job.phase = Phase::Live;
                    job.error = None;
                    true
                }
                "rejected" | "expired" | "aborted" => {
                    job.phase = Phase::Rejected;
                    job.error = s.error.or(Some(s.state));
                    true
                }
                _ => false,
            }
        }
        Err(ApiError::Refused(404, _)) => {
            job.phase = Phase::Failed;
            job.error = Some("no such upload".into());
            true
        }
        Err(_) => false,
    }
}

// ───────────────────────────── the app's side ─────────────────────────────

struct Running {
    cancel: tokio::sync::watch::Sender<bool>,
}

fn running() -> &'static Mutex<HashMap<String, Running>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, Running>>> = std::sync::OnceLock::new();
    R.get_or_init(Default::default)
}

fn jobs_cell(app: &tauri::AppHandle) -> &'static Mutex<Vec<Job>> {
    static J: std::sync::OnceLock<Mutex<Vec<Job>>> = std::sync::OnceLock::new();
    J.get_or_init(|| {
        let mut jobs: Vec<Job> = store_path(app)
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        // Nothing runs across a restart: an upload that was sending is paused until resumed.
        for j in &mut jobs {
            if matches!(j.phase, Phase::Hashing | Phase::Uploading | Phase::Completing) {
                j.phase = if j.upload_id.is_some() { Phase::Paused } else { Phase::Failed };
            }
        }
        Mutex::new(jobs)
    })
}

fn store_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    Ok(app.path().app_data_dir().map_err(|e| format!("no data directory: {e}"))?.join(STORE))
}

fn save(app: &tauri::AppHandle, jobs: &[Job]) {
    let Ok(path) = store_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Only what can be resumed or still needs showing; hashing jobs have nothing to resume.
    let keep: Vec<&Job> = jobs.iter().filter(|j| j.phase != Phase::Hashing).collect();
    if let Ok(bytes) = serde_json::to_vec_pretty(&keep) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

struct AppSink {
    app: tauri::AppHandle,
}

impl Sink for AppSink {
    fn update(&self, job: &Job) {
        let cell = jobs_cell(&self.app);
        let mut jobs = cell.lock().unwrap();
        // Progress ticks are frequent; the file is written only when something durable moved.
        let durable = match jobs.iter_mut().find(|j| j.key == job.key) {
            Some(j) => {
                let moved = j.phase != job.phase || j.etags.len() != job.etags.len() || j.upload_id != job.upload_id;
                *j = job.clone();
                moved
            }
            None => {
                jobs.push(job.clone());
                true
            }
        };
        if durable {
            save(&self.app, &jobs);
        }
        drop(jobs);
        let _ = self.app.emit(EVENT, job);
    }
}

fn api(app: &tauri::AppHandle) -> Result<Api, String> {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err(ApiError::SignIn.to_string());
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Api { client, base: crate::paintsync::control_plane(), token: cfg.cp_token })
}

fn modified_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Run `job` (hash first if it hasn't been), then watch its check. One task per job.
fn spawn(app: tauri::AppHandle, mut job: Job) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let mut r = running().lock().unwrap();
        if r.contains_key(&job.key) {
            return;
        }
        r.insert(job.key.clone(), Running { cancel: tx });
    }
    tauri::async_runtime::spawn(async move {
        let sink = AppSink { app: app.clone() };
        let key = job.key.clone();
        let finished = async {
            let api = match api(&app) {
                Ok(a) => a,
                Err(e) => {
                    job.phase = Phase::Failed;
                    job.error = Some(e);
                    sink.update(&job);
                    return;
                }
            };
            if job.sha256.is_empty() {
                job.phase = Phase::Hashing;
                sink.update(&job);
                let path = job.path.clone();
                let app2 = app.clone();
                let snapshot = job.clone();
                let hashed = tauri::async_runtime::spawn_blocking(move || {
                    let last = AtomicU64::new(0);
                    sha256_file(&path, &|n| {
                        // A tick per 64 MiB is plenty for a bar.
                        if n - last.load(Relaxed) >= 64 * 1024 * 1024 {
                            last.store(n, Relaxed);
                            let mut j = snapshot.clone();
                            j.sent = n;
                            let _ = app2.emit(EVENT, &j);
                        }
                    })
                })
                .await;
                match hashed {
                    Ok(Ok(sha)) => job.sha256 = sha,
                    Ok(Err(e)) => {
                        job.phase = Phase::Failed;
                        job.error = Some(format!("couldn't read the file: {e}"));
                        sink.update(&job);
                        return;
                    }
                    Err(e) => {
                        job.phase = Phase::Failed;
                        job.error = Some(e.to_string());
                        sink.update(&job);
                        return;
                    }
                }
                if *rx.borrow() {
                    // Paused before anything was opened: nothing to resume, so it starts over.
                    job.sha256.clear();
                    job.phase = Phase::Failed;
                    job.error = Some("cancelled".into());
                    sink.update(&job);
                    return;
                }
            }
            if job.phase != Phase::Checking {
                drive(&api, &mut job, &Tuning::default(), rx.clone(), &sink).await;
            }
            while job.phase == Phase::Checking {
                tokio::time::sleep(POLL).await;
                if poll_once(&api, &mut job).await {
                    sink.update(&job);
                }
            }
        };
        finished.await;
        running().lock().unwrap().remove(&key);
    });
}

/// A file the rider picked, before anything is sent: its name, size and kind.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub kind: Option<String>,
}

#[tauri::command]
pub async fn mod_upload_inspect(path: String) -> Result<FileInfo, String> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("couldn't read the file: {e}"))?;
    if !meta.is_file() {
        return Err("not a file".into());
    }
    let filename = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(FileInfo { kind: kind_of(&filename).map(str::to_string), path, filename, size: meta.len() })
}

/// Start an upload. Checks the file again here: the screen's checks are for the rider, these
/// are the ones that hold.
#[tauri::command]
pub async fn mod_upload_start(app: tauri::AppHandle, path: String, meta: UploadMeta) -> Result<Job, String> {
    api(&app)?;
    let info = mod_upload_inspect(path.clone()).await?;
    let kind = info.kind.as_deref().ok_or("only .pkz, .zip and .pnt files can be uploaded")?;
    if info.size == 0 || info.size > limit_for(kind) {
        return Err("the file is too large".into());
    }
    let job = Job {
        key: uuid::Uuid::new_v4().simple().to_string(),
        upload_id: None,
        modified: modified_secs(Path::new(&path)),
        path: PathBuf::from(path),
        filename: info.filename,
        size: info.size,
        sha256: String::new(),
        part_size: 0,
        etags: BTreeMap::new(),
        meta,
        phase: Phase::Hashing,
        sent: 0,
        error: None,
        asset_id: None,
        started_at: now_ms(),
    };
    jobs_cell(&app).lock().unwrap().push(job.clone());
    spawn(app, job.clone());
    Ok(job)
}

/// Every upload this app knows about. Starts watching any that are being checked.
#[tauri::command]
pub async fn mod_upload_jobs(app: tauri::AppHandle) -> Result<Vec<Job>, String> {
    let jobs = jobs_cell(&app).lock().unwrap().clone();
    for j in &jobs {
        if j.phase == Phase::Checking {
            spawn(app.clone(), j.clone());
        }
    }
    Ok(jobs)
}

#[tauri::command]
pub async fn mod_upload_pause(key: String) -> Result<(), String> {
    if let Some(r) = running().lock().unwrap().get(&key) {
        let _ = r.cancel.send(true);
    }
    Ok(())
}

#[tauri::command]
pub async fn mod_upload_resume(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let job = jobs_cell(&app)
        .lock()
        .unwrap()
        .iter()
        .find(|j| j.key == key)
        .cloned()
        .ok_or("no such upload")?;
    if !matches!(job.phase, Phase::Paused | Phase::Failed) {
        return Ok(());
    }
    let same = std::fs::metadata(&job.path).map(|m| m.len() == job.size).unwrap_or(false)
        && modified_secs(&job.path) == job.modified;
    if !same {
        return Err("the file has changed or moved since it was picked".into());
    }
    spawn(app, job);
    Ok(())
}

/// Cancel an upload: stop it, abandon the session, forget it.
#[tauri::command]
pub async fn mod_upload_cancel(app: tauri::AppHandle, key: String) -> Result<(), String> {
    mod_upload_pause(key.clone()).await?;
    let job = {
        let mut jobs = jobs_cell(&app).lock().unwrap();
        let at = jobs.iter().position(|j| j.key == key);
        let job = at.map(|i| jobs.remove(i));
        save(&app, &jobs);
        job
    };
    if let (Some(job), Ok(api)) = (job, api(&app)) {
        if let (Some(id), Phase::Paused | Phase::Uploading | Phase::Failed) = (job.upload_id, job.phase) {
            let _ = api.abort(&id).await;
        }
    }
    Ok(())
}

/// Drop a finished upload from the list.
#[tauri::command]
pub async fn mod_upload_dismiss(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let mut jobs = jobs_cell(&app).lock().unwrap();
    jobs.retain(|j| j.key != key || !matches!(j.phase, Phase::Live | Phase::Rejected | Phase::Failed));
    save(&app, &jobs);
    Ok(())
}

#[tauri::command]
pub async fn my_mods(app: tauri::AppHandle) -> Result<MyMods, String> {
    api(&app)?.my_mods().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mod_edit(app: tauri::AppHandle, asset_id: String, edit: ModEdit) -> Result<(), String> {
    api(&app)?.edit(&asset_id, &edit).await.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mod_details(app: tauri::AppHandle, asset_id: String) -> Result<ModDetails, String> {
    api(&app)?.details(&asset_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mod_delete(app: tauri::AppHandle, asset_id: String) -> Result<(), String> {
    api(&app)?.delete(&asset_id).await.map(|_| ()).map_err(|e| e.to_string())
}

/// Set a mod's picture from a file the rider picked.
#[tauri::command]
pub async fn mod_set_thumb(app: tauri::AppHandle, asset_id: String, path: String) -> Result<(), String> {
    let len = tokio::fs::metadata(&path).await.map_err(|e| e.to_string())?.len();
    if len > MAX_THUMB_BYTES {
        return Err("the picture is larger than 2 MB".into());
    }
    let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    api(&app)?.set_thumb(&asset_id, bytes).await.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicUsize;

    // ── a stand-in control plane and R2 ──

    #[derive(Clone, Debug)]
    struct Seen {
        method: String,
        path: String,
        body: Vec<u8>,
    }

    type Reply = (u16, Vec<(&'static str, String)>, String);
    type Handler = Arc<dyn Fn(&Seen) -> Reply + Send + Sync>;

    /// Answers each request with `handler`, one connection per request, and records them.
    fn serve(handler: Handler) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for sock in listener.incoming() {
                let Ok(mut sock) = sock else { return };
                let (handler, log) = (handler.clone(), log.clone());
                std::thread::spawn(move || {
                    let Some(req) = read_request(&mut sock) else { return };
                    log.lock().unwrap().push(req.clone());
                    let (status, headers, body) = handler(&req);
                    let mut out = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
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

    fn read_request(sock: &mut std::net::TcpStream) -> Option<Seen> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 65536];
        loop {
            let n = sock.read(&mut chunk).ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
            let Some(head) = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4) else { continue };
            let text = String::from_utf8_lossy(&buf[..head]).to_string();
            let len = text
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                .unwrap_or(0);
            if buf.len() >= head + len {
                let first = text.lines().next().unwrap_or_default().to_string();
                let mut it = first.split(' ');
                return Some(Seen {
                    method: it.next().unwrap_or_default().to_string(),
                    path: it.next().unwrap_or_default().to_string(),
                    body: buf[head..head + len].to_vec(),
                });
            }
        }
    }

    fn json(status: u16, v: serde_json::Value) -> Reply {
        (status, vec![("Content-Type", "application/json".into())], v.to_string())
    }

    fn quick() -> Tuning {
        Tuning { concurrency: 3, attempts: 3, retry_wait: Duration::from_millis(1) }
    }

    fn scratch_file(tag: &str, size: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxb-modupload-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mod.pkz");
        let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    struct Nothing;
    impl Sink for Nothing {
        fn update(&self, _: &Job) {}
    }

    struct Record(Mutex<Vec<Job>>);
    impl Sink for Record {
        fn update(&self, job: &Job) {
            self.0.lock().unwrap().push(job.clone());
        }
    }

    fn job_for(path: &Path, size: u64) -> Job {
        Job {
            key: "k".into(),
            upload_id: None,
            path: path.to_path_buf(),
            filename: "mod.pkz".into(),
            size,
            modified: 0,
            sha256: "a".repeat(64),
            part_size: 0,
            etags: BTreeMap::new(),
            meta: UploadMeta {
                title: "Test track".into(),
                mod_type: "tracks".into(),
                visibility: "public".into(),
                ..Default::default()
            },
            phase: Phase::Hashing,
            sent: 0,
            error: None,
            asset_id: None,
            started_at: 0,
        }
    }

    fn no_cancel() -> tokio::sync::watch::Receiver<bool> {
        let (tx, rx) = tokio::sync::watch::channel(false);
        std::mem::forget(tx);
        rx
    }

    // ── part sizing ──

    #[test]
    fn parts_match_the_control_planes_count() {
        let mib = 1024 * 1024;
        let part = 32 * mib;
        assert_eq!(part_plan(1, part), vec![Span { part: 1, offset: 0, len: 1 }]);
        assert_eq!(part_plan(part, part).len(), 1);
        let p = part_plan(part + 1, part);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1], Span { part: 2, offset: part, len: 1 });
        let max = part_plan(MAX_UPLOAD_BYTES, part);
        assert_eq!(max.len(), 64);
        assert_eq!(max.iter().map(|s| s.len).sum::<u64>(), MAX_UPLOAD_BYTES);
        assert!(max.iter().all(|s| s.len == part));
        // Part numbers are 1-based and contiguous, as S3 wants.
        assert!(max.iter().enumerate().all(|(i, s)| s.part == i as u32 + 1));
    }

    #[test]
    fn missing_skips_parts_already_held() {
        let plan = part_plan(100, 30);
        let mut held = BTreeMap::new();
        held.insert(2, "\"b\"".to_string());
        let todo: Vec<u32> = missing(&plan, &held).iter().map(|s| s.part).collect();
        assert_eq!(todo, vec![1, 3, 4]);
    }

    #[test]
    fn kinds_and_limits() {
        assert_eq!(kind_of("Track.PKZ"), Some("pkz"));
        assert_eq!(kind_of("paint.pnt"), Some("pnt"));
        assert_eq!(kind_of("thing.rar"), None);
        assert_eq!(kind_of("noext"), None);
        assert_eq!(limit_for("pnt"), MAX_PNT_BYTES);
        assert_eq!(limit_for("zip"), MAX_UPLOAD_BYTES);
    }

    // ── sending parts: retry ──

    /// A 500 and a dropped 503 are retried; the ETag of the try that took is the one kept.
    #[tokio::test]
    async fn a_busy_part_is_retried() {
        let tries = Arc::new(AtomicUsize::new(0));
        let t = tries.clone();
        let (base, seen) = serve(Arc::new(move |_req: &Seen| match t.fetch_add(1, Relaxed) {
            0 => (500, vec![], "busy".into()),
            1 => (503, vec![], "busy".into()),
            _ => (200, vec![("ETag", "\"good\"".into())], String::new()),
        }));
        let path = scratch_file("retry", 10);
        let spans = part_plan(10, 10);
        let urls = HashMap::from([(1, format!("{base}/part1"))]);
        let got = Mutex::new(Vec::new());
        let meter = Meter::default();
        send_parts(&Client::new(), &path, &spans, &urls, &quick(), &meter, no_cancel(), &|p, e| {
            got.lock().unwrap().push((p, e))
        })
        .await
        .unwrap();
        assert_eq!(*got.lock().unwrap(), vec![(1, "\"good\"".to_string())]);
        assert_eq!(seen.lock().unwrap().len(), 3);
        assert!(seen.lock().unwrap().iter().all(|s| s.method == "PUT" && s.body.len() == 10));
        assert_eq!(meter.sent(), 10);
    }

    /// A part that keeps failing stops the run after `attempts` tries.
    #[tokio::test]
    async fn retries_give_up() {
        let (base, seen) = serve(Arc::new(|_: &Seen| (500, vec![], "down".into())));
        let path = scratch_file("giveup", 10);
        let urls = HashMap::from([(1, format!("{base}/p"))]);
        let r = send_parts(&Client::new(), &path, &part_plan(10, 10), &urls, &quick(), &Meter::default(), no_cancel(), &|_, _| {}).await;
        assert!(matches!(r, Err(PartError::Failed(_))));
        assert_eq!(seen.lock().unwrap().len(), 3);
    }

    /// A 4xx other than 403 is final: sending the same bytes again says nothing new.
    #[tokio::test]
    async fn a_refused_part_is_not_retried() {
        let (base, seen) = serve(Arc::new(|_: &Seen| (400, vec![], "bad".into())));
        let path = scratch_file("refused", 10);
        let urls = HashMap::from([(1, format!("{base}/p"))]);
        let r = send_parts(&Client::new(), &path, &part_plan(10, 10), &urls, &quick(), &Meter::default(), no_cancel(), &|_, _| {}).await;
        assert!(matches!(r, Err(PartError::Failed(_))));
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    /// Every part goes, each with exactly its own bytes, in parallel.
    #[tokio::test]
    async fn parts_carry_their_own_slice() {
        let (base, seen) = serve(Arc::new(|req: &Seen| (200, vec![("ETag", format!("\"{}\"", req.path.trim_start_matches('/')))], String::new())));
        let path = scratch_file("slices", 25);
        let spans = part_plan(25, 10);
        let urls: HashMap<u32, String> = spans.iter().map(|s| (s.part, format!("{base}/{}", s.part))).collect();
        let got = Mutex::new(BTreeMap::new());
        send_parts(&Client::new(), &path, &spans, &urls, &quick(), &Meter::default(), no_cancel(), &|p, e| {
            got.lock().unwrap().insert(p, e);
        })
        .await
        .unwrap();
        assert_eq!(got.lock().unwrap().len(), 3);
        let file = std::fs::read(&path).unwrap();
        for s in seen.lock().unwrap().iter() {
            let part: usize = s.path.trim_start_matches('/').parse().unwrap();
            let span = spans[part - 1];
            assert_eq!(s.body, file[span.offset as usize..(span.offset + span.len) as usize]);
        }
    }

    /// Pausing stops the run without counting the part as sent.
    #[tokio::test]
    async fn pause_stops_the_run() {
        let (base, _) = serve(Arc::new(|_: &Seen| (200, vec![("ETag", "\"x\"".into())], String::new())));
        let path = scratch_file("pause", 10);
        let urls = HashMap::from([(1, format!("{base}/p"))]);
        let (tx, rx) = tokio::sync::watch::channel(true);
        let r = send_parts(&Client::new(), &path, &part_plan(10, 10), &urls, &quick(), &Meter::default(), rx, &|_, _| {}).await;
        drop(tx);
        assert!(matches!(r, Err(PartError::Paused)));
    }

    // ── end to end against the control plane's shapes ──

    /// A stand-in for `uploads.ts`: `holds` are parts R2 already has for a resumed session.
    fn control_plane(part_size: u64, parts: u32, holds: Vec<u32>, fail_puts: usize) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let base = Arc::new(Mutex::new(String::new()));
        let b = base.clone();
        let fails = Arc::new(AtomicUsize::new(fail_puts));
        let (url, seen) = serve(Arc::new(move |req: &Seen| {
            let me = b.lock().unwrap().clone();
            let id = "0123456789abcdef0123456789abcdef";
            let urls = |nums: Vec<u32>| -> Vec<serde_json::Value> {
                nums.into_iter().map(|n| serde_json::json!({ "part": n, "url": format!("{me}/r2/{n}") })).collect()
            };
            match (req.method.as_str(), req.path.as_str()) {
                ("POST", "/v1/uploads") => json(201, serde_json::json!({
                    "id": id, "state": "open", "part_size": part_size, "parts": urls((1..=parts).collect()),
                    "expires_at": "2026-10-09T00:00:00.000Z"
                })),
                ("GET", p) if p == format!("/v1/uploads/{id}") => {
                    let missing: Vec<u32> = (1..=parts).filter(|n| !holds.contains(n)).collect();
                    let uploaded: Vec<_> = holds.iter().map(|n| serde_json::json!({ "part": n, "etag": format!("\"held{n}\""), "size": part_size })).collect();
                    json(200, serde_json::json!({
                        "id": id, "state": "open", "error": null, "asset_id": null,
                        "part_size": part_size, "uploaded": uploaded, "parts": urls(missing)
                    }))
                }
                ("POST", p) if p == format!("/v1/uploads/{id}/complete") => {
                    json(202, serde_json::json!({ "id": id, "state": "verifying", "asset_id": "3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c" }))
                }
                ("PUT", p) if p.starts_with("/r2/") => {
                    if fails.load(Relaxed) > 0 {
                        fails.fetch_sub(1, Relaxed);
                        return (502, vec![], String::new());
                    }
                    (200, vec![("ETag", format!("\"e{}\"", &p[4..]))], String::new())
                }
                _ => json(404, serde_json::json!({ "error": "no such endpoint" })),
            }
        }));
        *base.lock().unwrap() = url.clone();
        (url, seen)
    }

    fn api_at(base: &str) -> Api {
        Api { client: Client::new(), base: base.into(), token: "test-token".into() }
    }

    #[tokio::test]
    async fn a_fresh_upload_opens_sends_and_completes() {
        let (base, seen) = control_plane(10, 3, vec![], 1);
        let path = scratch_file("fresh", 25);
        let mut job = job_for(&path, 25);
        let sink = Record(Mutex::new(Vec::new()));
        drive(&api_at(&base), &mut job, &quick(), no_cancel(), &sink).await;
        assert_eq!(job.phase, Phase::Checking, "{:?}", job.error);
        assert_eq!(job.asset_id.as_deref(), Some("3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c"));
        assert_eq!(job.etags.len(), 3);
        let seen = seen.lock().unwrap();
        let open: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
        assert_eq!(open["size"], 25);
        assert_eq!(open["type"], "tracks");
        assert_eq!(open["visibility"], "public");
        assert!(open.get("asset_id").is_none());
        let done = seen.iter().find(|s| s.path.ends_with("/complete")).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&done.body).unwrap();
        assert_eq!(body["parts"], serde_json::json!([
            { "part": 1, "etag": "\"e1\"" }, { "part": 2, "etag": "\"e2\"" }, { "part": 3, "etag": "\"e3\"" }
        ]));
        // The session was handed to the sink before any part went, so a crash can resume it.
        assert!(sink.0.lock().unwrap().iter().any(|j| j.upload_id.is_some() && j.etags.is_empty()));
    }

    /// After a restart: the session exists, R2 holds part 2, and only 1 and 3 are sent.
    #[tokio::test]
    async fn a_resume_sends_only_what_r2_lacks() {
        let (base, seen) = control_plane(10, 3, vec![2], 0);
        let path = scratch_file("resume", 25);
        let mut job = job_for(&path, 25);
        job.upload_id = Some("0123456789abcdef0123456789abcdef".into());
        job.part_size = 10;
        job.phase = Phase::Paused;
        drive(&api_at(&base), &mut job, &quick(), no_cancel(), &Nothing).await;
        assert_eq!(job.phase, Phase::Checking, "{:?}", job.error);
        let seen = seen.lock().unwrap();
        let puts: Vec<&str> = seen.iter().filter(|s| s.method == "PUT").map(|s| s.path.as_str()).collect();
        assert_eq!(puts.len(), 2);
        assert!(!puts.contains(&"/r2/2"));
        assert!(!seen.iter().any(|s| s.method == "POST" && s.path == "/v1/uploads"), "no second session");
        let done = seen.iter().find(|s| s.path.ends_with("/complete")).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&done.body).unwrap();
        assert_eq!(body["parts"][1], serde_json::json!({ "part": 2, "etag": "\"held2\"" }));
    }

    /// A paused run keeps its session and the parts it finished; nothing is completed.
    #[tokio::test]
    async fn a_paused_upload_is_kept_for_later() {
        let (base, seen) = control_plane(10, 3, vec![], 0);
        let path = scratch_file("paused", 25);
        let mut job = job_for(&path, 25);
        let (tx, rx) = tokio::sync::watch::channel(true);
        drive(&api_at(&base), &mut job, &quick(), rx, &Nothing).await;
        drop(tx);
        assert_eq!(job.phase, Phase::Paused);
        assert!(job.upload_id.is_some());
        assert!(!seen.lock().unwrap().iter().any(|s| s.path.ends_with("/complete")));
    }

    /// The control plane's refusal is what the rider is told, word for word.
    #[tokio::test]
    async fn a_refused_session_says_why() {
        let (base, _) = serve(Arc::new(|_: &Seen| json(429, serde_json::json!({ "error": "at most 20 uploads a day" }))));
        let path = scratch_file("quota", 5);
        let mut job = job_for(&path, 5);
        drive(&api_at(&base), &mut job, &quick(), no_cancel(), &Nothing).await;
        assert_eq!(job.phase, Phase::Failed);
        assert_eq!(job.error.as_deref(), Some("at most 20 uploads a day"));
    }

    #[tokio::test]
    async fn blocked_and_signin_are_told_apart() {
        let (base, _) = serve(Arc::new(|req: &Seen| {
            if req.path == "/v1/me/mods" {
                json(403, serde_json::json!({ "error": "This copy couldn't be verified.", "code": "blocked" }))
            } else {
                json(401, serde_json::json!({ "error": "unauthorized" }))
            }
        }));
        let api = api_at(&base);
        assert_eq!(api.my_mods().await.unwrap_err().to_string(), "blocked:This copy couldn't be verified.");
        assert_eq!(api.status("x").await.unwrap_err().to_string(), "signin");
        let none = Api { token: String::new(), ..api };
        assert!(matches!(none.my_mods().await, Err(ApiError::SignIn)));
    }

    #[tokio::test]
    async fn checking_ends_live_or_rejected() {
        let state = Arc::new(Mutex::new("checking"));
        let s = state.clone();
        let (base, _) = serve(Arc::new(move |_: &Seen| {
            let st = *s.lock().unwrap();
            let error = if st == "rejected" { "an executable inside" } else { "" };
            json(200, serde_json::json!({ "id": "x", "state": st, "error": error, "asset_id": "9d1e2f3a-4b5c-4d6e-8f70-8192a3b4c5d6" }))
        }));
        let api = api_at(&base);
        let mut job = job_for(Path::new("x"), 1);
        job.upload_id = Some("x".into());
        job.phase = Phase::Checking;
        assert!(!poll_once(&api, &mut job).await);
        *state.lock().unwrap() = "live";
        assert!(poll_once(&api, &mut job).await);
        assert_eq!((job.phase, job.asset_id.as_deref()), (Phase::Live, Some("9d1e2f3a-4b5c-4d6e-8f70-8192a3b4c5d6")));
        *state.lock().unwrap() = "rejected";
        job.phase = Phase::Checking;
        assert!(poll_once(&api, &mut job).await);
        assert_eq!(job.phase, Phase::Rejected);
        assert_eq!(job.error.as_deref(), Some("an executable inside"));
    }

    #[test]
    fn my_mods_reads_the_control_planes_shape() {
        let body = serde_json::json!({
            "mods": [{ "id": "3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c", "title": "Track", "type": "tracks", "visibility": "unlisted", "state": "active",
                       "modified": "2026-10-07T00:00:00.000Z", "live": true, "thumb": null, "reports": 1 }],
            "uploads": [{ "id": "ab", "asset_id": "3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c", "filename": "t.pkz", "size": 10,
                          "state": "rejected", "error": "not a pkz archive", "created_at": 1 }],
            "quota": { "openSessions": 3, "uploadsPerDay": 20, "bytesPerDay": 10737418240u64, "storageBytes": 26843545600u64 }
        });
        let m: MyMods = serde_json::from_value(body).unwrap();
        assert_eq!(m.mods[0].mod_type, "tracks");
        assert_eq!(m.mods[0].id, "3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c");
        assert_eq!(m.uploads[0].asset_id.as_deref(), Some("3f2b6c1e-8d4a-4b7f-9c2e-1a5d7e9f0b3c"));
        assert_eq!(m.uploads[0].error.as_deref(), Some("not a pkz archive"));
        assert_eq!(m.quota.uploads_per_day, 20);
        // And out to the webview in camelCase.
        let out = serde_json::to_value(&m).unwrap();
        assert_eq!(out["mods"][0]["modType"], "tracks");
        assert_eq!(out["quota"]["storageBytes"], 26843545600u64);
    }

    #[test]
    fn a_saved_job_from_before_public_ids_still_loads() {
        let meta: UploadMeta = serde_json::from_value(serde_json::json!({
            "title": "t", "type": "tracks", "visibility": "public", "assetId": 42
        }))
        .unwrap();
        assert_eq!(meta.asset_id, None);
        assert_eq!(meta.thumb_path, None);
    }

    #[test]
    fn knows_a_picture_by_its_bytes() {
        assert_eq!(image_type(b"RIFF\x08\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(image_type(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(image_type(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(image_type(b"<html>"), None);
    }

    #[test]
    fn hashing_matches_sha2() {
        let path = scratch_file("hash", 3_000_000);
        let want: String = Sha256::digest(std::fs::read(&path).unwrap()).iter().map(|b| format!("{b:02x}")).collect();
        let last = AtomicU64::new(0);
        assert_eq!(sha256_file(&path, &|n| last.store(n, Relaxed)).unwrap(), want);
        assert_eq!(last.load(Relaxed), 3_000_000);
    }
}
