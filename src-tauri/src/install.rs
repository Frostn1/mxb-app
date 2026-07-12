//! Add-to-library pipeline: resolve a host-specific download URL, stream the
//! archive with progress events, extract it, and place the track files into
//! `<MX Bikes>/mods/tracks`.

use crate::config::AppConfig;
use futures_util::StreamExt;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";
const EMIT_EVERY_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    slug: String,
    stage: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    received: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn emit(app: &AppHandle, slug: &str, stage: &'static str, received: Option<u64>, total: Option<u64>) {
    let _ = app.emit(
        "install-progress",
        Progress {
            slug: slug.to_string(),
            stage,
            received,
            total,
            message: None,
        },
    );
}

pub async fn add_to_library(
    app: &AppHandle,
    cfg: &AppConfig,
    slug: &str,
    url: &str,
    host: &str,
    subpath: &str,
) -> anyhow::Result<()> {
    let client = Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(15))
        .cookie_store(true)
        .build()?;

    // 1. Resolve a directly-downloadable URL for the host.
    emit(app, slug, "resolving", None, None);
    let direct = resolve_direct_url(&client, url, host).await?;

    // Fresh working dir under the OS temp dir.
    let work = std::env::temp_dir().join(format!("frost-{}", sanitize(slug)));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    // 2. Download the archive.
    let archive = download(app, &client, slug, &direct, &work).await?;

    // 3. Extract it.
    emit(app, slug, "extracting", None, None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    extract_archive(&archive, &extracted)?;

    // 4. Place mod files into the game's folder for this mod type.
    emit(app, slug, "placing", None, None);
    let dest = crate::library::mods_subdir(&cfg.mods_path, subpath);
    place_mod_files(&extracted, &dest, slug)?;

    let _ = std::fs::remove_dir_all(&work);
    emit(app, slug, "done", None, None);
    Ok(())
}

// --- host resolution -------------------------------------------------------

async fn resolve_direct_url(client: &Client, url: &str, host: &str) -> anyhow::Result<String> {
    let h = host.to_lowercase();
    let u = url.to_lowercase();
    if h.contains("mediafire") || u.contains("mediafire.com") {
        resolve_mediafire(client, url).await
    } else if h.contains("drive.google") || u.contains("drive.google") {
        Ok(resolve_gdrive(url))
    } else if h.contains("mega") || u.contains("mega.nz") {
        anyhow::bail!("Mega links aren't supported yet — open the mod page to download it manually.")
    } else {
        // Assume a direct file link.
        Ok(url.to_string())
    }
}

async fn resolve_mediafire(client: &Client, url: &str) -> anyhow::Result<String> {
    let html = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let direct = Regex::new(r#"https?://download[0-9]+\.mediafire\.com/[^"'<>\\ ]+"#).unwrap();
    if let Some(m) = direct.find(&html) {
        return Ok(m.as_str().to_string());
    }
    let button = Regex::new(r#"id="downloadButton"[^>]*href="([^"]+)""#).unwrap();
    if let Some(c) = button.captures(&html) {
        return Ok(c[1].to_string());
    }
    anyhow::bail!("Couldn't find the MediaFire download link — open the mod page to download it manually.")
}

fn resolve_gdrive(url: &str) -> String {
    let by_path = Regex::new(r"/d/([A-Za-z0-9_-]+)").unwrap();
    let by_query = Regex::new(r"[?&]id=([A-Za-z0-9_-]+)").unwrap();
    let id = by_path
        .captures(url)
        .or_else(|| by_query.captures(url))
        .map(|c| c[1].to_string());
    match id {
        // usercontent is where the actual bytes live; large files still return a
        // virus-scan interstitial that `download` follows.
        Some(id) => {
            format!("https://drive.usercontent.google.com/download?id={id}&export=download")
        }
        None => url.to_string(),
    }
}

// --- download --------------------------------------------------------------

/// GET with a few retries for transient transport errors (flaky CDNs, resets).
async fn get_with_retry(client: &Client, url: &str) -> anyhow::Result<reqwest::Response> {
    const ATTEMPTS: u32 = 3;
    let mut last: Option<reqwest::Error> = None;
    for attempt in 1..=ATTEMPTS {
        match client.get(url).send().await {
            Ok(resp) => return Ok(resp.error_for_status()?),
            Err(e) => {
                last = Some(e);
                if attempt < ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(600 * attempt as u64)).await;
                }
            }
        }
    }
    Err(anyhow::Error::new(last.expect("had an error"))
        .context("could not reach the download host after 3 attempts"))
}

async fn download(
    app: &AppHandle,
    client: &Client,
    slug: &str,
    url: &str,
    dir: &Path,
) -> anyhow::Result<PathBuf> {
    let mut resp = get_with_retry(client, url).await?;

    // Large Google Drive files answer with a "virus scan warning" HTML page that
    // carries a confirm form; submit it to get the actual bytes.
    if content_type(&resp).starts_with("text/html") && url.contains("google") {
        let html = resp.text().await?;
        let (action, params) = parse_gdrive_confirm(&html).ok_or_else(|| {
            anyhow::anyhow!(
                "Google Drive returned an unexpected page — open the mod page to download it manually."
            )
        })?;
        resp = client
            .get(&action)
            .query(&params)
            .send()
            .await?
            .error_for_status()?;
    }

    if content_type(&resp).starts_with("text/html") {
        anyhow::bail!(
            "The host returned a web page instead of a file — open the mod page to download it manually."
        );
    }

    let total = resp.content_length();
    let filename = filename_from(&resp, url);
    let path = dir.join(filename);
    let mut file = File::create(&path)?;
    let mut stream = resp.bytes_stream();
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        received += chunk.len() as u64;
        if received - last_emit >= EMIT_EVERY_BYTES {
            last_emit = received;
            emit(app, slug, "downloading", Some(received), total);
        }
    }
    file.flush()?;
    emit(app, slug, "downloading", Some(received), total);
    Ok(path)
}

fn content_type(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase()
}

/// Parse Google Drive's virus-scan confirm form into (action, query params).
fn parse_gdrive_confirm(html: &str) -> Option<(String, Vec<(String, String)>)> {
    let doc = Html::parse_document(html);
    let form_sel = Selector::parse("form").ok()?;
    let input_sel = Selector::parse("input[name]").ok()?;

    let form = doc.select(&form_sel).next()?;
    let action = form.value().attr("action")?.to_string();
    let params: Vec<(String, String)> = form
        .select(&input_sel)
        .filter_map(|i| {
            let name = i.value().attr("name")?.to_string();
            let value = i.value().attr("value").unwrap_or("").to_string();
            Some((name, value))
        })
        .collect();

    (!params.is_empty()).then_some((action, params))
}

fn filename_from(resp: &reqwest::Response, url: &str) -> String {
    // Prefer the Content-Disposition filename.
    if let Some(cd) = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(c) = Regex::new(r#"filename\*?=(?:UTF-8''|")?([^";]+)"#)
            .unwrap()
            .captures(cd)
        {
            let name = c[1].trim().trim_matches('"');
            if !name.is_empty() {
                return sanitize(name);
            }
        }
    }
    // Otherwise the last path segment of the URL.
    let from_url = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    if from_url.is_empty() {
        "download.bin".to_string()
    } else {
        sanitize(&from_url)
    }
}

// --- extraction ------------------------------------------------------------

fn extract_archive(archive: &Path, dest: &Path) -> anyhow::Result<()> {
    match detect_ext(archive)?.as_str() {
        "zip" => {
            let file = File::open(archive)?;
            zip::ZipArchive::new(file)?.extract(dest)?;
        }
        "7z" => {
            sevenz_rust::decompress_file(archive, dest)
                .map_err(|e| anyhow::anyhow!("7z extraction failed: {e}"))?;
        }
        "rar" => extract_rar(archive, dest)?,
        "pkz" => {
            // Already a track file; carry it through to the place step.
            let name = archive.file_name().unwrap_or_default();
            std::fs::copy(archive, dest.join(name))?;
        }
        other => anyhow::bail!("Unsupported archive type: .{other}"),
    }
    Ok(())
}

/// Extract a RAR archive, preserving entry paths under `dest`.
fn extract_rar(archive: &Path, dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut open = unrar::Archive::new(archive)
        .open_for_processing()
        .map_err(|e| anyhow::anyhow!("failed to open RAR: {e}"))?;
    while let Some(header) = open
        .read_header()
        .map_err(|e| anyhow::anyhow!("RAR read error: {e}"))?
    {
        open = if header.entry().is_file() {
            header
                .extract_with_base(dest)
                .map_err(|e| anyhow::anyhow!("RAR extract error: {e}"))?
        } else {
            header
                .skip()
                .map_err(|e| anyhow::anyhow!("RAR skip error: {e}"))?
        };
    }
    Ok(())
}

fn detect_ext(archive: &Path) -> anyhow::Result<String> {
    if let Some(ext) = archive
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
    {
        if ["zip", "7z", "rar", "pkz"].contains(&ext.as_str()) {
            return Ok(ext);
        }
    }
    // Sniff magic bytes when the name has no useful extension.
    let mut buf = [0u8; 8];
    let n = File::open(archive)?.read(&mut buf)?;
    let magic = &buf[..n];
    if magic.starts_with(b"PK\x03\x04") || magic.starts_with(b"PK\x05\x06") {
        return Ok("zip".to_string());
    }
    if magic.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Ok("7z".to_string());
    }
    if magic.starts_with(b"Rar!") {
        return Ok("rar".to_string());
    }
    anyhow::bail!("Could not determine the archive type of the downloaded file.")
}

// --- placement -------------------------------------------------------------

/// Copy `.pkz` mod files into `dest`. If none are found, copy the whole
/// extracted tree into `dest/<slug>` as a fallback.
fn place_mod_files(extracted: &Path, dest: &Path, slug: &str) -> anyhow::Result<usize> {
    let pkz: Vec<PathBuf> = WalkDir::new(extracted)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("pkz"))
                .unwrap_or(false)
        })
        .collect();

    std::fs::create_dir_all(dest)?;

    if !pkz.is_empty() {
        for p in &pkz {
            if let Some(name) = p.file_name() {
                std::fs::copy(p, dest.join(name))?;
            }
        }
        return Ok(pkz.len());
    }

    copy_dir_all(extracted, &dest.join(sanitize(slug)))?;
    Ok(0)
}

fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Strip path separators and other unsafe characters from a file/dir name.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdrive_id_from_share_url() {
        let out = resolve_gdrive("https://drive.google.com/file/d/ABC123_xyz/view?usp=sharing");
        assert!(out.contains("id=ABC123_xyz"));
        assert!(out.contains("export=download"));
        assert!(out.contains("drive.usercontent.google.com"));
    }

    #[test]
    fn parses_gdrive_virus_scan_form() {
        // Mirrors the real "Virus scan warning" interstitial markup.
        let html = r#"<!DOCTYPE html><html><head><title>Google Drive - Virus scan warning</title></head>
            <body><form id="download-form" action="https://drive.usercontent.google.com/download" method="get">
              <input type="hidden" name="id" value="1GfLnMrUXqOaBzn61">
              <input type="hidden" name="export" value="download">
              <input type="hidden" name="confirm" value="t">
              <input type="hidden" name="uuid" value="2b32fee2-d9c8-48a0-be9a-51d4b1dea839">
            </form></body></html>"#;
        let (action, params) = parse_gdrive_confirm(html).expect("form should parse");
        assert_eq!(action, "https://drive.usercontent.google.com/download");
        assert!(params.iter().any(|(k, v)| k == "confirm" && v == "t"));
        assert!(params
            .iter()
            .any(|(k, v)| k == "uuid" && v == "2b32fee2-d9c8-48a0-be9a-51d4b1dea839"));
    }

    #[test]
    fn sanitize_strips_separators() {
        assert_eq!(sanitize("a/b\\c:d"), "a_b_c_d");
    }

    /// Live check that the Google Drive large-file confirm flow resolves to an
    /// actual file (headers only — no full download). Ignored by default.
    #[test]
    #[ignore = "hits live Google Drive"]
    fn live_gdrive_resolves_to_file() {
        tauri::async_runtime::block_on(async {
            let client = Client::builder()
                .user_agent(UA)
                .cookie_store(true)
                .build()
                .unwrap();
            let url = resolve_gdrive(
                "https://drive.google.com/file/d/1GfLnMrUXqOaBzn61RZo1gIGoGaytM030/view",
            );
            let mut resp = client.get(&url).send().await.unwrap().error_for_status().unwrap();
            if content_type(&resp).starts_with("text/html") {
                let html = resp.text().await.unwrap();
                let (action, params) =
                    parse_gdrive_confirm(&html).expect("confirm form should parse");
                resp = client
                    .get(&action)
                    .query(&params)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap();
            }
            let ct = content_type(&resp);
            let cd = resp
                .headers()
                .get(reqwest::header::CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            println!(
                "GDRIVE final: content-type='{}' content-disposition='{}' len={:?}",
                ct,
                cd,
                resp.content_length()
            );
            assert!(!ct.starts_with("text/html"), "expected a file, got HTML");
        });
    }

    #[test]
    fn detect_ext_sniffs_magic_bytes() -> anyhow::Result<()> {
        let base = std::env::temp_dir().join(format!("frost-magic-{}", std::process::id()));
        std::fs::create_dir_all(&base)?;

        // No/unknown extension → sniff the leading bytes.
        let rar = base.join("download.bin");
        std::fs::write(&rar, b"Rar!\x1a\x07\x01\x00")?;
        assert_eq!(detect_ext(&rar)?, "rar");

        let zip = base.join("blob");
        std::fs::write(&zip, b"PK\x03\x04rest")?;
        assert_eq!(detect_ext(&zip)?, "zip");

        // A real extension wins without sniffing.
        let named = base.join("track.7z");
        std::fs::write(&named, b"not really 7z")?;
        assert_eq!(detect_ext(&named)?, "7z");

        let _ = std::fs::remove_dir_all(&base);
        Ok(())
    }

    /// End-to-end for the local half of the pipeline: a zip containing a nested
    /// `.pkz` extracts and the `.pkz` lands directly in the tracks dir.
    #[test]
    fn extract_and_place_zip_with_pkz() -> anyhow::Result<()> {
        let base = std::env::temp_dir().join(format!("frost-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base)?;

        // Build a zip: SomeTrack/track.pkz + a readme to ignore.
        let zip_path = base.join("mod.zip");
        {
            let file = std::fs::File::create(&zip_path)?;
            let mut w = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("SomeTrack/track.pkz", opts)?;
            std::io::Write::write_all(&mut w, b"PKZDATA")?;
            w.start_file("SomeTrack/readme.txt", opts)?;
            std::io::Write::write_all(&mut w, b"hello")?;
            w.finish()?;
        }

        let extracted = base.join("extracted");
        std::fs::create_dir_all(&extracted)?;
        extract_archive(&zip_path, &extracted)?;

        let dest = base.join("tracks");
        let placed = place_mod_files(&extracted, &dest, "some-track")?;

        assert_eq!(placed, 1);
        assert!(dest.join("track.pkz").exists());
        assert!(!dest.join("readme.txt").exists());

        let _ = std::fs::remove_dir_all(&base);
        Ok(())
    }
}
