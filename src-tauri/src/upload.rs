//! Anonymous file-host upload for the preset **full-share** bundle.
//!
//! The full share packages every asset a preset references into a `.zip`
//! ([`crate::bundle`]) and uploads it to a no-login host so a plain link can be
//! sent to a friend. The host lives behind the single [`upload_file`] entry point,
//! so swapping it (GoFile / 0x0.st / …) is a one-function change.
//!
//! Default host: **pixeldrain** — anonymous, large-file friendly, and its download
//! is a plain direct URL (`/api/file/<id>`) that the existing install downloader
//! streams with no extra resolver. Uploads are anonymous `PUT`s; the returned file
//! id is turned into that direct URL and embedded in the share code.

use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;

/// Where an uploaded bundle ended up: a direct-download URL, a host label, and the
/// uploaded size. Mirrors `presets::BundleRef` (built from this).
pub struct UploadRef {
    pub url: String,
    pub host: String,
    pub size: u64,
}

/// Human label for the default host (shown in the import dialog).
const HOST: &str = "pixeldrain";

/// Upload `file` to the default anonymous host, returning where it landed.
pub async fn upload_file(client: &Client, file: &Path) -> anyhow::Result<UploadRef> {
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let url = pixeldrain_upload(client, file).await?;
    Ok(UploadRef { url, host: HOST.to_string(), size })
}

#[derive(Deserialize)]
struct PixeldrainResp {
    id: Option<String>,
    /// Present on failures (`{ success:false, message:"…" }`).
    message: Option<String>,
}

/// pixeldrain.com — anonymous `PUT /api/file/{name}` returns `{ "id": "…" }`.
async fn pixeldrain_upload(client: &Client, file: &Path) -> anyhow::Result<String> {
    // Read the bundle into memory and PUT it anonymously. Bundles are usually
    // modest (paints/liveries + a few gear models); very large "everything"
    // bundles trade memory for simplicity here.
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "preset-bundle.zip".to_string());

    let resp = client
        .put(format!("https://pixeldrain.com/api/file/{name}"))
        .body(bytes)
        .send()
        .await
        .context("couldn't reach pixeldrain to upload the bundle")?;

    let status = resp.status();
    let parsed: PixeldrainResp = resp
        .json()
        .await
        .context("pixeldrain returned an unexpected response")?;

    match parsed.id {
        Some(id) if !id.is_empty() => {
            // Direct-download URL — the install downloader streams it as-is.
            Ok(format!("https://pixeldrain.com/api/file/{id}"))
        }
        _ => {
            let why = parsed.message.unwrap_or_else(|| format!("HTTP {status}"));
            anyhow::bail!("pixeldrain upload failed: {why}")
        }
    }
}
