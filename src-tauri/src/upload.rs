use anyhow::Context;
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;

pub struct UploadRef {
    pub url: String,
    pub host: String,
    pub size: u64,
}

const HOST: &str = "pixeldrain";

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
