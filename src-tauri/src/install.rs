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
use tauri::{AppHandle, Emitter, Manager};

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

pub(crate) fn notify_frostmod(app: &AppHandle, slug: &str) {
    let outcome = crate::frostmod::signal_reload();
    let _ = app.emit(
        "frostmod-reload",
        FrostmodReload {
            slug: slug.to_string(),
            outcome,
        },
    );
}

pub(crate) fn build_client() -> anyhow::Result<Client> {
    Ok(Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(15))
        .cookie_store(true)
        .build()?)
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
    let client = build_client()?;

    // MEGA is end-to-end encrypted — no direct URL; use the fetch-and-decrypt path.
    let h = host.to_lowercase();
    let u = url.to_lowercase();
    if h.contains("mega") || u.contains("mega.nz") || u.contains("mega.co") {
        return download_mega_and_place(app, cfg, &client, slug, url, subpath, dest_folder).await;
    }

    emit(app, slug, "resolving", None, None);
    let direct = resolve_direct_url(&client, url, host).await?;

    download_and_place(app, cfg, &client, slug, &direct, subpath, dest_folder).await
}

pub async fn download_and_place(
    app: &AppHandle,
    cfg: &AppConfig,
    client: &Client,
    slug: &str,
    direct_url: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    let work = std::env::temp_dir().join(format!("frost-{}", sanitize(slug)));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;

    let archive = download(app, client, slug, direct_url, &work).await?;
    extract_and_place(app, cfg, slug, &archive, &work, subpath, dest_folder)
}

fn extract_and_place(
    app: &AppHandle,
    cfg: &AppConfig,
    slug: &str,
    archive: &Path,
    work: &Path,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    emit(app, slug, "extracting", None, None);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    extract_archive(archive, &extracted)?;

    emit(app, slug, "placing", None, None);
    let mods_dir = crate::library::mods_subdir(&cfg.mods_path, "mods");
    let type_folder = subpath.rsplit(['/', '\\']).next().unwrap_or("tracks");
    place_mod(&extracted, &mods_dir, type_folder, dest_folder, slug)?;

    // Record which bikes got a sound so the Library can tell them from stock (best-effort).
    if type_folder.eq_ignore_ascii_case("bikes") {
        let bikes = sound_bikes_in(&extracted);
        if !bikes.is_empty() {
            if let Ok(dir) = app.path().app_local_data_dir() {
                let _ = crate::soundmods::record(&dir, &bikes, slug);
            }
        }
    }

    let _ = std::fs::remove_dir_all(work);
    emit(app, slug, "done", None, None);

    notify_frostmod(app, slug);
    Ok(())
}

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

pub(crate) async fn download_mega(
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

    notify_frostmod(app, &slug);
    Ok(())
}

pub(crate) async fn resolve_direct_url(
    client: &Client,
    url: &str,
    host: &str,
) -> anyhow::Result<String> {
    let h = host.to_lowercase();
    let u = url.to_lowercase();
    if h.contains("mediafire") || u.contains("mediafire.com") {
        resolve_mediafire(client, url).await
    } else if h.contains("drive.google") || u.contains("drive.google") {
        // A folder link (…/drive/folders/ID) has no single file to fetch — look
        // inside it, find the mod archive, and download that file directly.
        if is_gdrive_folder(url) {
            resolve_gdrive_folder(client, url).await
        } else {
            Ok(resolve_gdrive(url))
        }
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
    // Fallback: match the download button's `aria-label="Download file"` href.
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
        // usercontent serves the bytes; large files still hit a virus-scan interstitial.
        Some(id) => {
            format!("https://drive.usercontent.google.com/download?id={id}&export=download")
        }
        None => url.to_string(),
    }
}

/// True when the link points at a whole Drive folder rather than a single file.
/// (`open?id=` is intentionally excluded — it's ambiguous and usually a file.)
fn is_gdrive_folder(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains("/folders/") || u.contains("/folderview")
}

fn gdrive_folder_id(url: &str) -> Option<String> {
    let by_path = Regex::new(r"/folders/([A-Za-z0-9_-]+)").unwrap();
    let by_query = Regex::new(r"[?&]id=([A-Za-z0-9_-]+)").unwrap();
    by_path
        .captures(url)
        .or_else(|| by_query.captures(url))
        .map(|c| c[1].to_string())
}

/// Resolve a Drive *folder* link to a single downloadable file URL. Mod folders
/// bundle the track archive alongside sub-folders (server files, unpacked track);
/// we scrape the folder listing and pick the archive.
async fn resolve_gdrive_folder(client: &Client, url: &str) -> anyhow::Result<String> {
    let folder_id = gdrive_folder_id(url).ok_or_else(|| {
        anyhow::anyhow!(
            "Couldn't read the Google Drive folder id — open the mod page to download it manually."
        )
    })?;
    let html = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let files = parse_gdrive_folder(&html, &folder_id);
    if files.is_empty() {
        anyhow::bail!(
            "This Google Drive folder has no downloadable file — open the mod page to download it manually."
        );
    }
    let chosen = pick_folder_archive(&files).ok_or_else(|| {
        anyhow::anyhow!(
            "Couldn't tell which file in the Google Drive folder is the mod — open the mod page to download it manually."
        )
    })?;
    Ok(resolve_gdrive(&format!(
        "https://drive.google.com/file/d/{}/view",
        chosen.id
    )))
}

/// A file entry scraped from a Drive folder listing.
struct GDriveFile {
    id: String,
    name: String,
    mime: String,
}

/// Extract `[fileId,[parentId],name,mime]` tuples the folder page embeds in its
/// bootstrap data. Sub-folders (mime `application/vnd.google-apps.folder`) stay in
/// the list so the caller can skip them explicitly.
fn parse_gdrive_folder(html: &str, folder_id: &str) -> Vec<GDriveFile> {
    // The listing lives in an escaped JS blob (\x5b = '[', \x22 = '"'); normalize it.
    let text = html
        .replace(r"\x5b", "[")
        .replace(r"\x5d", "]")
        .replace(r"\x22", "\"")
        .replace(r"\/", "/");
    let pat = Regex::new(&format!(
        r#""([A-Za-z0-9_-]{{20,}})",\["{}"\],"((?:[^"\\]|\\.)*?)","([^"]+)""#,
        regex::escape(folder_id)
    ))
    .unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for c in pat.captures_iter(&text) {
        let id = c[1].to_string();
        if seen.insert(id.clone()) {
            out.push(GDriveFile {
                id,
                name: c[2].to_string(),
                mime: c[3].to_string(),
            });
        }
    }
    out
}

/// Choose the mod archive from a folder's files: skip sub-folders, prefer a known
/// archive extension, and fall back to the sole remaining file when unambiguous.
fn pick_folder_archive(files: &[GDriveFile]) -> Option<&GDriveFile> {
    const ARCHIVE_EXT: [&str; 5] = [".pkz", ".zip", ".rar", ".7z", ".pnt"];
    let candidates: Vec<&GDriveFile> = files
        .iter()
        .filter(|f| f.mime != "application/vnd.google-apps.folder")
        .collect();
    let is_archive = |f: &GDriveFile| {
        let n = f.name.to_lowercase();
        ARCHIVE_EXT.iter().any(|ext| n.ends_with(ext))
    };
    candidates
        .iter()
        .find(|f| is_archive(f))
        .or_else(|| (candidates.len() == 1).then(|| &candidates[0]))
        .copied()
}

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

pub(crate) async fn download(
    app: &AppHandle,
    client: &Client,
    slug: &str,
    url: &str,
    dir: &Path,
) -> anyhow::Result<PathBuf> {
    let mut resp = get_with_retry(client, url).await?;

    // Large Google Drive files return a virus-scan HTML page with a confirm form; submit it.
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

pub(crate) fn extract_archive(archive: &Path, dest: &Path) -> anyhow::Result<()> {
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
        "pkz" | "pnt" => {
            // Already installable (.pkz/.pnt) — carry it through unchanged.
            let name = archive.file_name().unwrap_or_default();
            std::fs::copy(archive, dest.join(name))?;
        }
        other => anyhow::bail!("Unsupported archive type: .{other}"),
    }
    Ok(())
}

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
        if ["zip", "7z", "rar", "pkz", "pnt"].contains(&ext.as_str()) {
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

/// MX Bikes content categories that live directly under `mods/`.
const CATEGORY_DIRS: [&str; 5] = ["bikes", "tracks", "rider", "tyres", "misc"];

pub(crate) fn place_mod(
    extracted: &Path,
    mods_dir: &Path,
    type_folder: &str,
    dest_folder: &str,
    slug: &str,
) -> anyhow::Result<usize> {
    // Check the extracted root FIRST so a `<Bike>/paints/` bundle isn't unwrapped to bare `paints/`.
    let unwrapped = unwrap_wrapper(extracted);
    let candidates: Vec<&Path> = if unwrapped == extracted {
        vec![extracted]
    } else {
        vec![extracted, unwrapped.as_path()]
    };

    for base in &candidates {
        if let Some(m) = child_dir(base, "mods") {
            return merge_tree(&m, mods_dir);
        }
    }

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

    // A `<Bike>/paints/…` bundle is bike content → route to `mods/bikes` regardless of
    // the caller's default type. Rider paints are exempt (kept under `mods/rider` below).
    if !type_folder.eq_ignore_ascii_case("rider") {
        for base in &candidates {
            if contains_paints_bundle(base) {
                return merge_tree(base, &mods_dir.join("bikes"));
            }
        }
    }

    // Sound bundle (`engine.scl`+`sfx.cfg`): bike content that belongs at the bike root,
    // NEVER inside `paints/` → route to `mods/bikes` and drop any trailing `paints` segment.
    for base in &candidates {
        if contains_sound_bundle(base) {
            // `<Bike>/{engine.scl,sfx.cfg}` — merge the bike folder(s) as-is.
            return merge_tree(base, &mods_dir.join("bikes"));
        }
        if dir_has_sound_markers(base) {
            // Loose `engine.scl`+`sfx.cfg` — drop into the chosen bike's root.
            let mut dir = mods_dir.join("bikes");
            for seg in dest_folder.split(['/', '\\']).filter(|s| !s.is_empty()) {
                if seg.eq_ignore_ascii_case("paints") {
                    continue;
                }
                dir.push(sanitize(seg));
            }
            return place_plain(base, &dir, slug, false);
        }
    }

    // Plain placement into the type folder, honoring the chosen destination sub-folder.
    let mut type_dir = mods_dir.join(type_folder);
    for seg in dest_folder.split(['/', '\\']).filter(|s| !s.is_empty()) {
        type_dir.push(sanitize(seg));
    }
    // Extracted tracks need their own folder; loose bike paints don't.
    let wrap_loose = type_folder.eq_ignore_ascii_case("tracks");
    place_plain(&unwrapped, &type_dir, slug, wrap_loose)
}

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

/// Both present in a folder = a sound mod.
const SOUND_MARKERS: [&str; 2] = ["engine.scl", "sfx.cfg"];

fn dir_has_sound_markers(dir: &Path) -> bool {
    let mut found = [false; SOUND_MARKERS.len()];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                let name = e.file_name();
                let name = name.to_string_lossy();
                for (i, m) in SOUND_MARKERS.iter().enumerate() {
                    if name.eq_ignore_ascii_case(m) {
                        found[i] = true;
                    }
                }
            }
        }
    }
    found.iter().all(|&f| f)
}

fn contains_sound_bundle(base: &Path) -> bool {
    std::fs::read_dir(base)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .any(|d| dir_has_sound_markers(&d.path()))
        })
        .unwrap_or(false)
}

pub fn sound_bikes_in(extracted: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(extracted)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let p = entry.path();
        if !dir_has_sound_markers(p) {
            continue;
        }
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if name.eq_ignore_ascii_case("sounds") {
                continue;
            }
            let name = name.to_string();
            if !out.iter().any(|n: &String| n.eq_ignore_ascii_case(&name)) {
                out.push(name);
            }
        }
    }
    out
}

fn contains_paints_bundle(base: &Path) -> bool {
    std::fs::read_dir(base)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .any(|d| child_dir(&d.path(), "paints").is_some())
        })
        .unwrap_or(false)
}

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

    // Loose files, no sub-folders: wrap in their own folder, or drop straight in.
    if !has_pkz && dirs.is_empty() && non_junk_files > 0 {
        let target = if wrap_loose {
            type_dir.join(sanitize(slug))
        } else {
            type_dir.to_path_buf()
        };
        return merge_tree(base, &target);
    }

    // A `.pkz` is the complete, installable package. When one sits at the root,
    // sibling folders are almost always extras the archive bundles alongside it —
    // the dedicated-"server" build and the unpacked track source. Install ONLY the
    // `.pkz` file(s) so those extras don't get dumped into the game folder.
    if has_pkz {
        let mut n = 0;
        for p in files.iter().filter(|p| has_ext(p, "pkz")) {
            let name = p.file_name().unwrap_or_default();
            std::fs::copy(p, type_dir.join(name))?;
            n += 1;
        }
        return Ok(n);
    }

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
    fn detects_gdrive_folder_links() {
        assert!(is_gdrive_folder(
            "https://drive.google.com/drive/folders/1vYkgITTCU8hXhu1yBgfsLhyvXfnlG2Ln"
        ));
        assert!(!is_gdrive_folder(
            "https://drive.google.com/file/d/ABC123/view"
        ));
    }

    #[test]
    fn parses_gdrive_folder_listing_and_picks_archive() {
        // Mirrors the escaped bootstrap blob a public folder page embeds.
        let folder = "1vYkgITTCU8hXhu1yBgfsLhyvXfnlG2Ln";
        let html = format!(
            r#"junk \x5b\x221YKsASoNQ498qvk0CF3XEN9rOnIkkCLaR\x22,\x5b\x22{f}\x22\x5d,\x22I40 MX server\x22,\x22application/vnd.google-apps.folder\x22\x5d more \x5b\x221pymPFNcJ3h6iegZZhz2GBGQ4JBMxm2OY\x22,\x5b\x22{f}\x22\x5d,\x22I40 MX.pkz\x22,\x22application/x-zip\x22\x5d tail"#,
            f = folder
        );
        let files = parse_gdrive_folder(&html, folder);
        assert_eq!(files.len(), 2);
        let chosen = pick_folder_archive(&files).expect("should pick the archive");
        assert_eq!(chosen.name, "I40 MX.pkz");
        assert_eq!(chosen.id, "1pymPFNcJ3h6iegZZhz2GBGQ4JBMxm2OY");
    }

    #[test]
    fn sanitize_strips_separators() {
        assert_eq!(sanitize("a/b\\c:d"), "a_b_c_d");
    }

    #[test]
    fn extract_passes_through_bare_pnt() {
        let dir = std::env::temp_dir().join(format!("frost-test-pnt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = dir.join("Cool Livery.pnt");
        let dest = dir.join("extracted");
        std::fs::create_dir_all(&dest).unwrap();
        // Real .pnt files start with the "PNT\0" magic — anything but a known archive.
        std::fs::write(&src, b"PNT\0some paint bytes").unwrap();

        extract_archive(&src, &dest).expect("bare .pnt should extract (copy) through");
        assert!(dest.join("Cool Livery.pnt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

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

        let rar = base.join("download.bin");
        std::fs::write(&rar, b"Rar!\x1a\x07\x01\x00")?;
        assert_eq!(detect_ext(&rar)?, "rar");

        let zip = base.join("blob");
        std::fs::write(&zip, b"PK\x03\x04rest")?;
        assert_eq!(detect_ext(&zip)?, "zip");

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
    fn pkz_alongside_server_and_source_folders_installs_only_pkz() {
        // Mirrors the I40 MX bundle: the client `.pkz` plus a dedicated-server
        // folder and the unpacked track source. Only the `.pkz` should install.
        let root = place_tmp("bundle-pkz");
        let ex = root.join("ex");
        touch(&ex.join("I40 MX.pkz"));
        touch(&ex.join("I40 MX server/server.cfg"));
        touch(&ex.join("I40 MX server/I40 MX.pkz"));
        touch(&ex.join("I40 MX!/track.trk"));
        touch(&ex.join("I40 MX!/textures/asphalt.tga"));
        let mods = root.join("mods");
        let placed = place_mod(&ex, &mods, "tracks", "", "i40-mx").unwrap();

        assert_eq!(placed, 1);
        assert!(mods.join("tracks/I40 MX.pkz").exists());
        assert!(!mods.join("tracks/I40 MX server").exists());
        assert!(!mods.join("tracks/I40 MX!").exists());
        let _ = std::fs::remove_dir_all(&root);
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
    fn livery_bundle_routes_to_bikes_even_with_tracks_default() {
        let root = place_tmp("livery-tracks-default");
        let ex = root.join("ex");
        touch(&ex.join("MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "tracks", "", "slug").unwrap();
        assert!(mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt")
            .exists());
        assert!(!mods.join("tracks/MX1OEM_2023_KTM_450_SX-F").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn places_rider_kit_into_profile_paints() {
        let root = place_tmp("rider-kit");
        let ex = root.join("ex");
        touch(&ex.join("2026 ASTARS TECHSTAR UNITY.pnt")); // loose outfit paint
        let mods = root.join("mods");
        place_mod(&ex, &mods, "rider", "riders/default_mx/paints", "kit").unwrap();
        assert!(mods
            .join("rider/riders/default_mx/paints/2026 ASTARS TECHSTAR UNITY.pnt")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rider_paint_bundle_not_routed_to_bikes() {
        let root = place_tmp("rider-bundle");
        let ex = root.join("ex");
        touch(&ex.join("default_mx/paints/kit.pnt")); // <profile>/paints bundle
        let mods = root.join("mods");
        place_mod(&ex, &mods, "rider", "riders/default_mx/paints", "kit").unwrap();
        assert!(mods.join("rider/riders/default_mx/paints/kit.pnt").exists());
        assert!(!mods.join("bikes/default_mx").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn helmet_paint_bundle_stays_in_rider() {
        let root = place_tmp("helmet-paint");
        let ex = root.join("ex");
        touch(&ex.join("Fox V3/paints/red.pnt"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "rider", "helmets/Fox V3/paints", "paint").unwrap();
        assert!(mods.join("rider/helmets/Fox V3/paints/red.pnt").exists());
        assert!(!mods.join("bikes/Fox V3").exists());
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

    #[test]
    fn packaged_sound_mod_merges_to_bike_root() {
        let root = place_tmp("sound-packaged");
        let ex = root.join("ex");
        let bike = "ASS KTM250-0.1/mods/bikes/MX2OEM_2023_KTM_250_SX-F";
        touch(&ex.join(format!("{bike}/engine.scl")));
        touch(&ex.join(format!("{bike}/sfx.cfg")));
        touch(&ex.join("ASS KTM250-0.1/mods/bikes/sounds/idle.wav"));
        let mods = root.join("mods");
        // Picker may pass `<Bike>/paints`; a self-structured archive ignores it.
        place_mod(&ex, &mods, "bikes", "MX2OEM_2023_KTM_250_SX-F/paints", "slug").unwrap();
        assert!(mods
            .join("bikes/MX2OEM_2023_KTM_250_SX-F/engine.scl")
            .exists());
        assert!(mods.join("bikes/MX2OEM_2023_KTM_250_SX-F/sfx.cfg").exists());
        assert!(mods.join("bikes/sounds/idle.wav").exists());
        // Never inside a paints folder.
        assert!(!mods.join("bikes/MX2OEM_2023_KTM_250_SX-F/paints").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nested_sound_bundle_routes_to_bikes() {
        let root = place_tmp("sound-nested");
        let ex = root.join("ex");
        touch(&ex.join("MX2OEM_2023_KTM_250_SX-F/engine.scl"));
        touch(&ex.join("MX2OEM_2023_KTM_250_SX-F/sfx.cfg"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "", "slug").unwrap();
        assert!(mods
            .join("bikes/MX2OEM_2023_KTM_250_SX-F/engine.scl")
            .exists());
        assert!(mods.join("bikes/MX2OEM_2023_KTM_250_SX-F/sfx.cfg").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loose_sound_files_never_land_in_paints() {
        let root = place_tmp("sound-loose");
        let ex = root.join("ex");
        touch(&ex.join("engine.scl"));
        touch(&ex.join("sfx.cfg"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "MX2OEM_2023_KTM_250_SX-F/paints", "slug").unwrap();
        assert!(mods
            .join("bikes/MX2OEM_2023_KTM_250_SX-F/engine.scl")
            .exists());
        assert!(!mods
            .join("bikes/MX2OEM_2023_KTM_250_SX-F/paints/engine.scl")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
