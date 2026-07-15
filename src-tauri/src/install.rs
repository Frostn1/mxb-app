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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrostmodReload {
    slug: String,
    outcome: crate::frostmod::ReloadOutcome,
}

/// Best-effort: ask a running FrostMod to live-reload the game's content now that
/// a new mod has landed, and tell the UI whether it worked. Never fails the
/// install — if FrostMod isn't running the game just picks the mod up on its
/// next launch.
fn notify_frostmod(app: &AppHandle, slug: &str) {
    let outcome = crate::frostmod::signal_reload();
    let _ = app.emit(
        "frostmod-reload",
        FrostmodReload {
            slug: slug.to_string(),
            outcome,
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub async fn add_to_library(
    app: &AppHandle,
    cfg: &AppConfig,
    slug: &str,
    url: &str,
    host: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    let client = Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(15))
        .cookie_store(true)
        .build()?;

    // MEGA is end-to-end encrypted: there's no plain "direct URL" to hand to the
    // generic downloader, so it gets its own fetch-and-decrypt path.
    let h = host.to_lowercase();
    let u = url.to_lowercase();
    if h.contains("mega") || u.contains("mega.nz") || u.contains("mega.co") {
        return download_mega_and_place(app, cfg, &client, slug, url, subpath, dest_folder).await;
    }

    // 1. Resolve a directly-downloadable URL for the host.
    emit(app, slug, "resolving", None, None);
    let direct = resolve_direct_url(&client, url, host).await?;

    // 2-4. Download, extract, and place (shared with the paid shop source).
    download_and_place(app, cfg, &client, slug, &direct, subpath, dest_folder).await
}

/// Download an already-resolved direct URL, extract the archive, and place the
/// mod into the game folder. Shared by the free catalog (public hosts) and the
/// paid shop, which supplies its own authenticated client and a direct EDD file
/// URL. Emits the same `install-progress` stages so the UI is source-agnostic.
pub async fn download_and_place(
    app: &AppHandle,
    cfg: &AppConfig,
    client: &Client,
    slug: &str,
    direct_url: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    // Fresh working dir under the OS temp dir.
    let work = std::env::temp_dir().join(format!("frost-{}", sanitize(slug)));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    // Download the archive.
    let archive = download(app, client, slug, direct_url, &work).await?;

    // Extract + place (shared with the MEGA path).
    extract_and_place(app, cfg, slug, &archive, &work, subpath, dest_folder)
}

/// Extract a downloaded archive and place its mod files into the game folder,
/// then clean up the work dir and nudge FrostMod. Shared by every download
/// source (public hosts, the paid shop, and MEGA) so the tail is identical.
fn extract_and_place(
    app: &AppHandle,
    cfg: &AppConfig,
    slug: &str,
    archive: &Path,
    work: &Path,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    // Extract it.
    emit(app, slug, "extracting", None, None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    extract_archive(archive, &extracted)?;

    // Place mod files into the game's folder for this mod type.
    emit(app, slug, "placing", None, None);
    let mods_dir = crate::library::mods_subdir(&cfg.mods_path, "mods");
    let type_folder = subpath.rsplit(['/', '\\']).next().unwrap_or("tracks");
    place_mod(&extracted, &mods_dir, type_folder, dest_folder, slug)?;

    let _ = std::fs::remove_dir_all(work);
    emit(app, slug, "done", None, None);

    // New mod is in place — nudge FrostMod to reload it live if it's running.
    notify_frostmod(app, slug);
    Ok(())
}

/// Download a MEGA public file link (fetch node metadata, decrypt the stream)
/// and place it like any other install. MEGA is end-to-end encrypted, so the
/// generic `resolve_direct_url` → `download` path can't be used — this fetches
/// and decrypts in-app via the pure-Rust `mega` crate.
async fn download_mega_and_place(
    app: &AppHandle,
    cfg: &AppConfig,
    client: &Client,
    slug: &str,
    url: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    let work = std::env::temp_dir().join(format!("frost-{}", sanitize(slug)));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let archive = download_mega(app, client, slug, url, &work).await?;
    extract_and_place(app, cfg, slug, &archive, &work, subpath, dest_folder)
}

/// Fetch + decrypt a MEGA public file link into `dir`, emitting the same
/// `downloading` progress stages as the generic HTTP downloader.
async fn download_mega(
    app: &AppHandle,
    http_client: &Client,
    slug: &str,
    url: &str,
    dir: &Path,
) -> anyhow::Result<PathBuf> {
    emit(app, slug, "resolving", None, None);

    let mega = mega::Client::builder()
        .build(http_client.clone())
        .map_err(|e| anyhow::anyhow!("MEGA client init failed: {e}"))?;

    let nodes = mega.fetch_public_nodes(url).await.map_err(|e| {
        anyhow::anyhow!("Couldn't read the MEGA link — it may be invalid or removed ({e}).")
    })?;

    // We only install single-file links; folder links fall back to the browser.
    let node = nodes
        .roots()
        .find(|n| n.kind().is_file())
        .ok_or_else(|| {
            anyhow::anyhow!("This MEGA link is a folder — open the mod page to download it manually.")
        })?;

    let total = Some(node.size());
    let path = dir.join(sanitize(node.name()));
    let file = File::create(&path)?;

    emit(app, slug, "downloading", Some(0), total);
    let writer = MegaProgressWriter {
        file,
        app,
        slug,
        total,
        received: 0,
        last_emit: 0,
    };
    mega.download_node(node, writer)
        .await
        .map_err(|e| anyhow::anyhow!("MEGA download failed: {e}"))?;
    emit(app, slug, "downloading", total, total);

    Ok(path)
}

/// A `futures` async writer that streams decrypted MEGA bytes to a file while
/// emitting throttled `downloading` progress events (mirrors the HTTP path).
struct MegaProgressWriter<'a> {
    file: File,
    app: &'a AppHandle,
    slug: &'a str,
    total: Option<u64>,
    received: u64,
    last_emit: u64,
}

impl futures_util::io::AsyncWrite for MegaProgressWriter<'_> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let n = this.file.write(buf)?;
        this.received += n as u64;
        if this.received - this.last_emit >= EMIT_EVERY_BYTES {
            this.last_emit = this.received;
            emit(this.app, this.slug, "downloading", Some(this.received), this.total);
        }
        std::task::Poll::Ready(Ok(n))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(self.get_mut().file.flush())
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

/// Import an already-downloaded archive or `.pkz` from disk. Used for hosts that
/// block in-app downloads (e.g. MediaFire): the user downloads via the browser,
/// then imports the file here and it's extracted/placed like a normal install.
pub fn import_file(
    app: &AppHandle,
    cfg: &AppConfig,
    file_path: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    let src = Path::new(file_path);
    if !src.is_file() {
        anyhow::bail!("file not found: {file_path}");
    }

    let work = std::env::temp_dir().join(format!("frost-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;

    extract_archive(src, &extracted)?;
    let mods_dir = crate::library::mods_subdir(&cfg.mods_path, "mods");
    let type_folder = subpath.rsplit(['/', '\\']).next().unwrap_or("tracks");
    let slug = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "import".to_string());
    place_mod(&extracted, &mods_dir, type_folder, dest_folder, &slug)?;

    let _ = std::fs::remove_dir_all(&work);

    // Imported mod is in place — nudge FrostMod to reload it live if it's running.
    notify_frostmod(app, &slug);
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

    // The direct CDN link is usually present verbatim in the page.
    let direct = Regex::new(r#"https?://download[0-9]+\.mediafire\.com/[^"'<>\\ ]+"#).unwrap();
    if let Some(m) = direct.find(&html) {
        return Ok(m.as_str().to_string());
    }
    // Fallback: the download button. MediaFire's current markup is a
    // `<a aria-label="Download file" href="…">` inside `#download_link` (the
    // old `id="downloadButton"` no longer exists), so match the aria-label.
    let button = Regex::new(r#"aria-label="Download file"[^>]*href="([^"]+)""#).unwrap();
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

/// MX Bikes content categories that live directly under `mods/`.
const CATEGORY_DIRS: [&str; 5] = ["bikes", "tracks", "rider", "tyres", "misc"];

/// Place an extracted mod into the game's `mods/` folder **preserving the
/// archive's folder structure** (never flattening), so liveries land in
/// `mods/bikes/<Bike>/paints/`, extracted tracks keep their folder, etc.
///
/// - `mods_dir` is `<MX Bikes>/mods`.
/// - `type_folder` is the default bucket for this mod type (`tracks` / `bikes`)
///   when the archive doesn't carry its own structure.
///
/// Returns the number of files placed.
fn place_mod(
    extracted: &Path,
    mods_dir: &Path,
    type_folder: &str,
    dest_folder: &str,
    slug: &str,
) -> anyhow::Result<usize> {
    // Look for recognized structure at the extracted root AND one wrapper level
    // down (archives are often wrapped in a `ModName/` folder). Check the root
    // FIRST so a `<Bike>/paints/` bundle isn't unwrapped into a bare `paints/`.
    let unwrapped = unwrap_wrapper(extracted);
    let candidates: Vec<&Path> = if unwrapped == extracted {
        vec![extracted]
    } else {
        vec![extracted, unwrapped.as_path()]
    };

    // 1. Archive already contains a `mods/` tree -> merge it in.
    for base in &candidates {
        if let Some(m) = child_dir(base, "mods") {
            return merge_tree(&m, mods_dir);
        }
    }

    // 2. Archive has top-level category folders (bikes/tracks/rider/...).
    for base in &candidates {
        let cats: Vec<PathBuf> = CATEGORY_DIRS
            .iter()
            .filter_map(|c| child_dir(base, c))
            .collect();
        if !cats.is_empty() {
            let mut n = 0;
            for c in &cats {
                let name = c.file_name().unwrap_or_default();
                n += merge_tree(c, &mods_dir.join(name))?;
            }
            return Ok(n);
        }
    }

    // 3. Bike-livery bundle: a `<BikeName>/paints/…` structure -> mods/bikes.
    if type_folder.eq_ignore_ascii_case("bikes") {
        for base in &candidates {
            if contains_paints_bundle(base) {
                return merge_tree(base, &mods_dir.join("bikes"));
            }
        }
    }

    // 4. Plain placement into the mod type's folder, honoring the user's chosen
    //    destination sub-folder (e.g. a track folder, or `<Bike>/paints` for a
    //    loose livery). Self-structured archives above ignore it.
    let mut type_dir = mods_dir.join(type_folder);
    for seg in dest_folder.split(['/', '\\']).filter(|s| !s.is_empty()) {
        type_dir.push(sanitize(seg));
    }
    // Extracted tracks need their own folder; loose bike paints don't.
    let wrap_loose = type_folder.eq_ignore_ascii_case("tracks");
    place_plain(&unwrapped, &type_dir, slug, wrap_loose)
}

/// Descend through redundant single-folder wrappers (e.g. `archive/ModName/...`).
fn unwrap_wrapper(dir: &Path) -> PathBuf {
    let mut cur = dir.to_path_buf();
    loop {
        let entries: Vec<_> = match std::fs::read_dir(&cur) {
            Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
            Err(_) => return cur,
        };
        let dirs: Vec<_> = entries
            .iter()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();
        let only_junk_files = entries
            .iter()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .all(|f| is_junk(&f.file_name().to_string_lossy()));
        if dirs.len() == 1 && only_junk_files {
            cur = dirs[0].path();
        } else {
            return cur;
        }
    }
}

fn child_dir(parent: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| {
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && e.file_name().to_string_lossy().eq_ignore_ascii_case(name)
        })
        .map(|e| e.path())
}

/// True when a child folder itself holds a `paints` folder (i.e. `<Bike>/paints`).
fn contains_paints_bundle(base: &Path) -> bool {
    std::fs::read_dir(base)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .any(|d| child_dir(&d.path(), "paints").is_some())
        })
        .unwrap_or(false)
}

/// Place content that carries no MX Bikes structure into `type_dir`, preserving
/// any sub-folders. When `wrap_loose` is set, loose files (no `.pkz`, no
/// sub-folders) are wrapped in their own `<slug>` folder (an extracted track
/// needs one); when unset they go straight into `type_dir` (e.g. a loose livery
/// dropped into a chosen `paints` folder).
fn place_plain(
    base: &Path,
    type_dir: &Path,
    slug: &str,
    wrap_loose: bool,
) -> anyhow::Result<usize> {
    std::fs::create_dir_all(type_dir)?;
    let entries: Vec<_> = std::fs::read_dir(base)?.filter_map(|e| e.ok()).collect();
    let dirs: Vec<PathBuf> = entries
        .iter()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    let files: Vec<PathBuf> = entries
        .iter()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .collect();

    let has_pkz = files.iter().any(|p| has_ext(p, "pkz"));
    let non_junk_files = files
        .iter()
        .filter(|p| !is_junk(&p.file_name().unwrap_or_default().to_string_lossy()))
        .count();

    // Loose files with no sub-folders: wrap in their own folder (extracted track)
    // or drop straight in (a livery placed into a chosen paints folder).
    if !has_pkz && dirs.is_empty() && non_junk_files > 0 {
        let target = if wrap_loose {
            type_dir.join(sanitize(slug))
        } else {
            type_dir.to_path_buf()
        };
        return merge_tree(base, &target);
    }

    // Otherwise copy top-level files (skipping junk) and merge sub-folders as-is.
    let mut n = 0;
    for p in &files {
        let name = p.file_name().unwrap_or_default();
        if is_junk(&name.to_string_lossy()) {
            continue;
        }
        std::fs::copy(p, type_dir.join(name))?;
        n += 1;
    }
    for d in &dirs {
        let name = d.file_name().unwrap_or_default();
        n += merge_tree(d, &type_dir.join(name))?;
    }
    Ok(n)
}

/// Recursively copy `src` into `dst`, merging into existing folders.
fn merge_tree(src: &Path, dst: &Path) -> anyhow::Result<usize> {
    std::fs::create_dir_all(dst)?;
    let mut n = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            n += merge_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
            n += 1;
        }
    }
    Ok(n)
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn is_junk(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("readme")
        || n.ends_with(".txt")
        || n.ends_with(".url")
        || n.ends_with(".nfo")
        || n.ends_with(".md")
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

    /// Live test: can we actually fetch a real MediaFire-hosted track? Tries
    /// both TLS backends against the CDN. Ignored by default.
    #[test]
    #[ignore = "hits live MediaFire CDN"]
    fn live_mediafire_download() {
        tauri::async_runtime::block_on(async {
            let page_client = Client::builder().user_agent(UA).build().unwrap();
            let page = page_client
                .get("https://mxb-mods.com/mosca-mx/")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            let mf = Regex::new(r#"https://www\.mediafire\.com/file/[^"']+"#)
                .unwrap()
                .find(&page)
                .map(|m| m.as_str().to_string())
                .expect("mediafire link on page");
            let direct = resolve_mediafire(&page_client, &mf)
                .await
                .expect("resolve mediafire");
            println!("direct host: {}", &direct[..48.min(direct.len())]);

            let client = Client::builder()
                .user_agent(UA)
                .use_rustls_tls()
                .build()
                .unwrap();
            match client
                .get(&direct)
                .header("Range", "bytes=0-102399")
                .send()
                .await
            {
                Ok(r) => {
                    let status = r.status();
                    match r.bytes().await {
                        Ok(b) => println!(
                            "[rustls] status={status} bytes={} magic={:?}",
                            b.len(),
                            &b[..4.min(b.len())]
                        ),
                        Err(e) => println!("[rustls] body error: {e}"),
                    }
                }
                Err(e) => println!("[rustls] send error: {e:#}"),
            }
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

    fn place_tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-place-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// End-to-end: a zip containing `SomeTrack/track.pkz` + a readme lands the
    /// `.pkz` in `mods/tracks` and drops the readme.
    #[test]
    fn extract_and_place_zip_with_pkz() -> anyhow::Result<()> {
        let base = place_tmp("zip");
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

        let mods = base.join("mods");
        let placed = place_mod(&extracted, &mods, "tracks", "", "some-track")?;

        assert_eq!(placed, 1);
        assert!(mods.join("tracks/track.pkz").exists());
        assert!(!mods.join("tracks/readme.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
        Ok(())
    }

    #[test]
    fn places_plain_pkz_into_type_folder() {
        let root = place_tmp("plain");
        let ex = root.join("ex");
        touch(&ex.join("track.pkz"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "", "slug").unwrap();
        assert!(mods.join("tracks/track.pkz").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn places_bike_livery_into_bike_paints() {
        let root = place_tmp("livery");
        let ex = root.join("ex");
        touch(&ex.join("MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "", "slug").unwrap();
        assert!(mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merges_full_mods_tree() {
        let root = place_tmp("modstree");
        let ex = root.join("ex");
        touch(&ex.join("mods/bikes/KTM.pkz"));
        touch(&ex.join("mods/tracks/T.pkz"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "", "slug").unwrap();
        assert!(mods.join("bikes/KTM.pkz").exists());
        assert!(mods.join("tracks/T.pkz").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merges_top_level_category_folders() {
        let root = place_tmp("cats");
        let ex = root.join("ex");
        touch(&ex.join("bikes/Y.pkz"));
        touch(&ex.join("tracks/X.pkz"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "", "slug").unwrap();
        assert!(mods.join("bikes/Y.pkz").exists());
        assert!(mods.join("tracks/X.pkz").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wraps_loose_extracted_track_files() {
        let root = place_tmp("loose");
        let ex = root.join("ex");
        touch(&ex.join("round3.cfg"));
        touch(&ex.join("round3.map"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "", "MyTrack").unwrap();
        assert!(mods.join("tracks/MyTrack/round3.cfg").exists());
        assert!(mods.join("tracks/MyTrack/round3.map").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plain_pkz_honors_chosen_dest_folder() {
        let root = place_tmp("dest");
        let ex = root.join("ex");
        touch(&ex.join("track.pkz"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "Supercross/Round 1", "slug").unwrap();
        assert!(mods.join("tracks/Supercross/Round 1/track.pkz").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loose_livery_goes_into_chosen_bike_paints() {
        let root = place_tmp("loose-livery");
        let ex = root.join("ex");
        touch(&ex.join("cool.pnt")); // loose paint, no bike folder
        let mods = root.join("mods");
        place_mod(
            &ex,
            &mods,
            "bikes",
            "MX1OEM_2023_KTM_450_SX-F/paints",
            "cool-livery",
        )
        .unwrap();
        assert!(mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unwraps_single_wrapper_folder() {
        let root = place_tmp("wrap");
        let ex = root.join("ex");
        touch(&ex.join("Downloaded Mod/track.pkz"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "", "slug").unwrap();
        assert!(mods.join("tracks/track.pkz").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
