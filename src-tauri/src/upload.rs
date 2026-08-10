use anyhow::Context;
use obfstr::obfstr;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use std::path::Path;

pub struct UploadRef {
    pub url: String,
    pub host: String,
    pub size: u64,
}

const HOST: &str = "catbox";

/// catbox refuses anything past this, so we stop before spending the upload.
const MAX_BYTES: u64 = 200 * 1024 * 1024;

pub async fn upload_file(client: &Client, file: &Path) -> anyhow::Result<UploadRef> {
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size > MAX_BYTES {
        anyhow::bail!(
            "This bundle is {:.0} MB — too large to share (the limit is {} MB). \
             Share the plain code instead.",
            size as f64 / (1024.0 * 1024.0),
            MAX_BYTES / (1024 * 1024)
        );
    }
    let url = catbox_upload(client, file).await?;
    Ok(UploadRef { url, host: HOST.to_string(), size })
}

/// catbox.moe — anonymous multipart POST whose response body *is* the direct download URL.
async fn catbox_upload(client: &Client, file: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "preset-bundle.zip".to_string());

    let form = Form::new().text("reqtype", "fileupload").part(
        "fileToUpload",
        Part::bytes(bytes).file_name(name).mime_str("application/zip")?,
    );

    let resp = client
        .post(obfstr!("https://catbox.moe/user/api.php"))
        .multipart(form)
        .send()
        .await
        .context("couldn't reach catbox to upload the bundle")?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("catbox returned an unreadable response")?;
    let body = body.trim();

    // Failures come back as plain prose, sometimes under a 200, so the URL is the only tell.
    if body.starts_with("https://") {
        Ok(body.to_string())
    } else if body.is_empty() {
        anyhow::bail!("catbox upload failed: HTTP {status}")
    } else {
        anyhow::bail!("catbox upload failed: {body}")
    }
}
