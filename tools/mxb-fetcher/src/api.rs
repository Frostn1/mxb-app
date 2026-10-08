//! The control plane's fetcher routes (`control-plane/src/mirrorfetcher.ts`), as typed calls.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub folder_allowed: Option<bool>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Lease {
    jobs: Vec<Job>,
}

/// A picture a page wants before it is finished.
#[derive(Debug, Clone, Deserialize)]
pub struct WantedImage {
    pub src: String,
    pub purpose: String,
    pub max_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct PageAnswer {
    #[serde(default)]
    images: Vec<WantedImage>,
}

/// An upload opened on the control plane, or word that it holds those bytes already.
#[derive(Debug, Deserialize)]
pub struct UploadStart {
    pub have: bool,
    #[serde(default)]
    pub upload: Option<String>,
    /// Every part but the last is exactly this long.
    #[serde(default)]
    pub part_bytes: Option<u64>,
}

/// What we uploaded, as `done` takes it.
#[derive(Debug, Clone, Serialize)]
pub struct Uploaded {
    pub sha256: String,
    pub size: u64,
    pub content_type: String,
    pub filename: String,
    /// Pictures only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

/// A failure to report: why, the HTTP status behind it, and whether to bother again.
#[derive(Debug, Clone, Default)]
pub struct Failure {
    pub error: String,
    pub status: Option<u16>,
    pub permanent: bool,
    pub retry_after: Option<Duration>,
    /// Not the job's fault (we are backing a site off): no attempt is counted.
    pub deferred: bool,
}

pub struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
}

/// An answer the control plane gave with an error status, kept so callers can tell 409/422 apart.
#[derive(Debug)]
pub struct Refused {
    pub status: u16,
    pub body: Value,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "control plane answered {}: {}", self.status, self.body)
    }
}
impl std::error::Error for Refused {}

impl Api {
    pub fn new(base: &str, token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("mxb-fetcher/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        })
    }

    async fn post(&self, action: &str, body: Value) -> Result<Value> {
        let res = self
            .http
            .post(format!("{}/v1/mirror/fetcher/{action}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {action}"))?;
        let status = res.status().as_u16();
        let value: Value = res.json().await.unwrap_or(Value::Null);
        if !(200..300).contains(&status) {
            return Err(Refused {
                status,
                body: value,
            }
            .into());
        }
        Ok(value)
    }

    pub async fn lease(&self, max: usize) -> Result<Vec<Job>> {
        let v = self.post("lease", json!({ "max": max })).await?;
        Ok(serde_json::from_value::<Lease>(v)?.jobs)
    }

    /// Hand a page's HTML in; the answer lists the pictures it still wants.
    pub async fn page_html(&self, job: &str, html: &str) -> Result<Vec<WantedImage>> {
        let v = self
            .post("result", json!({ "job": job, "html": html }))
            .await?;
        Ok(serde_json::from_value::<PageAnswer>(v)?.images)
    }

    pub async fn folder(
        &self,
        job: &str,
        name: &str,
        files: &[crate::mediafire::RemoteFile],
    ) -> Result<()> {
        self.post(
            "result",
            json!({ "job": job, "folder": { "name": name, "files": files } }),
        )
        .await
        .map(|_| ())
    }

    /// A discovery request's answer, status and body as the site gave them.
    pub async fn list_result(&self, job: &str, status: u16, body: &str) -> Result<()> {
        self.post(
            "result",
            json!({ "job": job, "status": status, "body": body }),
        )
        .await
        .map(|_| ())
    }

    pub async fn too_big(&self, job: &str) -> Result<()> {
        self.post("result", json!({ "job": job, "too_big": true }))
            .await
            .map(|_| ())
    }

    pub async fn failed(&self, job: &str, f: &Failure) -> Result<()> {
        let mut body = json!({ "job": job, "error": f.error, "permanent": f.permanent, "deferred": f.deferred });
        if let Some(s) = f.status {
            body["status"] = json!(s);
        }
        if let Some(r) = f.retry_after {
            body["retry_after_ms"] = json!(r.as_millis() as u64);
        }
        self.post("result", body).await.map(|_| ())
    }

    /// `upload/start`: by SHA-256, so a blob the mirror holds is never sent again.
    pub async fn upload_start(&self, job: &str, up: &Uploaded) -> Result<UploadStart> {
        let mut body = serde_json::to_value(up)?;
        body["job"] = json!(job);
        Ok(serde_json::from_value(
            self.post("upload/start", body).await?,
        )?)
    }

    /// One part's raw bytes. Parts go in order; the control plane hashes them as they pass.
    pub async fn upload_part(&self, upload: &str, n: u32, bytes: Vec<u8>) -> Result<()> {
        let res = self
            .http
            .put(format!("{}/v1/mirror/fetcher/upload/part", self.base))
            .query(&[("upload", upload), ("n", &n.to_string())])
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            // 32 MiB on a slow uplink takes a while; the client's default is for small calls.
            .timeout(Duration::from_secs(600))
            .body(bytes)
            .send()
            .await
            .with_context(|| format!("PUT upload part {n}"))?;
        let status = res.status().as_u16();
        if !(200..300).contains(&status) {
            let body = res.json().await.unwrap_or(Value::Null);
            return Err(Refused { status, body }.into());
        }
        Ok(())
    }

    pub async fn upload_complete(&self, upload: &str) -> Result<()> {
        self.post("upload/complete", json!({ "upload": upload }))
            .await
            .map(|_| ())
    }

    pub async fn upload_abort(&self, upload: &str) -> Result<()> {
        self.post("upload/abort", json!({ "upload": upload }))
            .await
            .map(|_| ())
    }
    pub async fn file_done(&self, job: &str, up: &Uploaded) -> Result<()> {
        let mut body = serde_json::to_value(up)?;
        body["job"] = json!(job);
        self.post("done", body).await.map(|_| ())
    }

    pub async fn page_done(&self, job: &str, images: &[Uploaded]) -> Result<Value> {
        self.post("done", json!({ "job": job, "images": images }))
            .await
    }
}

/// The status of a control-plane refusal inside an error chain, if that is what it was.
pub fn refused_status(err: &anyhow::Error) -> Option<u16> {
    err.downcast_ref::<Refused>().map(|r| r.status)
}

pub fn require(v: Option<String>, what: &str) -> Result<String> {
    v.filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("{what} is not set"))
}
