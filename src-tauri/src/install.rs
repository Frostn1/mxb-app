use crate::config::AppConfig;
use anyhow::Context;
use futures_util::StreamExt;
use obfstr::obfstr;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

// Same string as `shop_session::UA`, kept in one place.
use crate::shop_session::UA;
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

/// [`emit`], for the one caller outside this module.
///
/// [`crate::shop_fetch`] downloads through the browser rather than through `reqwest`, but it is
/// the same install to the user and has to drive the same progress bar — so it reports on this
/// event rather than inventing a second one.
pub(crate) fn emit_progress(
    app: &AppHandle,
    slug: &str,
    stage: &'static str,
    received: Option<u64>,
    total: Option<u64>,
) {
    emit(app, slug, stage, received, total);
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

/// A staging directory nobody else is using.
///
/// This used to be one path per process, wiped on entry — fine while installs were strictly
/// serial, fatal the moment two can be alive at once: a dropzone plan sits staged while the
/// user reviews it, and a second drop would delete the first one's files out from under it.
pub(crate) fn staging_dir(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "frost-{tag}-{}-{stamp:x}-{n}",
        std::process::id()
    ))
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

/// Download one mod and install it.
///
/// The staging directory is [`staging_dir`]'s, not a name derived from the slug: the retry
/// on a failed install starts a *second* run of the same slug, and a shared path meant the
/// newcomer's `remove_dir_all` deleted the files the first run was still copying — which
/// surfaced as a bare "os error 2" from the copy, on the install the user was retrying.
pub async fn download_and_place(
    app: &AppHandle,
    cfg: &AppConfig,
    client: &Client,
    slug: &str,
    direct_url: &str,
    subpath: &str,
    dest_folder: &str,
) -> anyhow::Result<()> {
    let work = staging_dir("dl");
    std::fs::create_dir_all(&work)?;

    let archive = download(app, client, slug, direct_url, &work).await?;
    extract_and_place(app, cfg, slug, &archive, &work, subpath, dest_folder)
}

/// Extract a downloaded archive and place it. `pub(crate)` for the shop, whose bytes come
/// through a WebView rather than `reqwest` but which finishes exactly the same way.
pub(crate) fn extract_and_place(
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

    // A ReShade preset lands beside ReShade itself, not in the mods tree, and is sorted by
    // file type rather than routed by folder — none of the placement planner below applies.
    if crate::reshade::is_reshade_subpath(subpath) {
        crate::reshade::install_extracted(&extracted, &cfg.reshade_dir())?;
        let _ = std::fs::remove_dir_all(work);
        emit(app, slug, "done", None, None);
        return Ok(());
    }

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
    let work = staging_dir("dl");
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

    let work = staging_dir("import");
    let _ = std::fs::remove_dir_all(&work);
    let extracted = work.join("extracted");
    std::fs::create_dir_all(&extracted)?;

    // A bare `.ini` isn't an archive — `extract_archive` would reject it — but it is the most
    // common way a preset arrives. Stage it as if it had been extracted.
    if crate::reshade::is_reshade_subpath(subpath) && crate::reshade::is_preset_file(src) {
        std::fs::copy(src, extracted.join(src.file_name().unwrap_or_default()))?;
    } else {
        extract_archive(src, &extracted)?;
    }

    if crate::reshade::is_reshade_subpath(subpath) {
        crate::reshade::install_extracted(&extracted, &cfg.reshade_dir())?;
        let _ = std::fs::remove_dir_all(&work);
        return Ok(());
    }

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
    if h.contains("proton") || u.contains("drive.proton.me") {
        // Proton Drive shares are end-to-end encrypted: the decryption key lives in the
        // URL *fragment*, which never reaches the server, and the file arrives as
        // OpenPGP-encrypted blocks. There is no direct link to resolve to — fetching the
        // URL returns the web app's HTML, which used to sail through here and fail much
        // later with "couldn't determine the archive type". Fail here instead, where we
        // can say what to do; the UI routes these to the manual download-and-pick flow.
        anyhow::bail!(
            "Proton Drive links are encrypted and can't be downloaded automatically — \
             download the file from Proton Drive, then use \"Choose file\" to install it."
        )
    } else if h.contains("mediafire") || u.contains("mediafire.com") {
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

    parse_mediafire_link(&html).ok_or_else(|| {
        anyhow::anyhow!(
            "Couldn't find the MediaFire download link — open the mod page to download it manually."
        )
    })
}

/// Pull the direct CDN link out of a MediaFire file page.
///
/// MediaFire keeps changing where it puts that link, and any one of these shapes can be
/// the only one on a given page — so try them all rather than betting on the current
/// layout. The base64 `data-scrambled-url` is the one that matters most: pages that carry
/// it leave the button's `href` as a placeholder, so a resolver that only reads `href`
/// finds nothing at all.
fn parse_mediafire_link(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);

    // 1. The scrambled attribute, wherever it hangs — the button today, something else
    //    tomorrow. It is plain base64 of the real URL.
    if let Ok(sel) = Selector::parse("[data-scrambled-url]") {
        for el in doc.select(&sel) {
            let scrambled = el.value().attr("data-scrambled-url").unwrap_or("");
            if let Some(u) = decode_scrambled(scrambled) {
                return Some(u);
            }
        }
    }

    // 2. The download button's own href. Matched through the parser rather than a regex
    //    so attribute order can't hide it — `href` before `aria-label` used to.
    for css in ["a#downloadButton[href]", "a[aria-label='Download file'][href]"] {
        if let Ok(sel) = Selector::parse(css) {
            if let Some(href) = doc.select(&sel).find_map(|el| el.value().attr("href")) {
                if let Some(u) = usable_link(href) {
                    return Some(u);
                }
            }
        }
    }

    // 3. Anywhere in the page source, including inside scripts: the CDN host is
    //    distinctive enough to match on its own. Un-escape the JSON slashes the scripts
    //    write (`https:\/\/download7…\/file.zip`) first, so one pattern covers both forms.
    let flat = html.replace("\\/", "/");
    let direct = Regex::new(r#"(?:https?:)?//download[0-9]+\.mediafire\.com/[^"'<>\\ ]+"#).unwrap();
    direct.find(&flat).and_then(|m| usable_link(m.as_str()))
}

fn decode_scrambled(value: &str) -> Option<String> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .ok()?;
    usable_link(std::str::from_utf8(&raw).ok()?)
}

/// Normalise a link off the page, rejecting the placeholders MediaFire parks in `href`
/// (`#`, `javascript:void(0)`) that would otherwise be "found" and then fail to download.
fn usable_link(href: &str) -> Option<String> {
    let href = html_escape::decode_html_entities(href.trim()).into_owned();
    if href.starts_with("//") {
        Some(format!("https:{href}"))
    } else if href.starts_with("http://") || href.starts_with("https://") {
        Some(href)
    } else {
        None
    }
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
            format!(
                "{}?id={id}&export=download",
                obfstr!("https://drive.usercontent.google.com/download")
            )
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
        "{}/file/d/{}/view",
        obfstr!("https://drive.google.com"),
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
    let is_gdrive = url.contains("google");

    // Large Google Drive files return a virus-scan HTML page with a confirm form; submit it.
    if content_type(&resp).starts_with("text/html") && is_gdrive {
        let html = resp.text().await?;
        let (action, params) = parse_gdrive_confirm(&html).ok_or_else(|| {
            anyhow::anyhow!(gdrive_page_error(&html).unwrap_or_else(|| {
                "Google Drive returned an unexpected page — open the mod page to download it \
                 manually."
                    .to_string()
            }))
        })?;
        resp = client
            .get(&action)
            .query(&params)
            .send()
            .await?
            .error_for_status()?;
    }

    if content_type(&resp).starts_with("text/html") {
        // Passing the virus-scan form doesn't mean Drive will hand over the bytes — it
        // answers quota and permission refusals with another 200 HTML page.
        if is_gdrive {
            let html = resp.text().await.unwrap_or_default();
            if let Some(msg) = gdrive_page_error(&html) {
                anyhow::bail!(msg);
            }
        }
        anyhow::bail!(
            "The host returned a web page instead of a file — open the mod page to download it manually."
        );
    }

    let total = resp.content_length();
    let filename = filename_from(&resp, url);
    let path = dir.join(filename);
    // Resume against the URL that actually served the bytes, not the one we asked for:
    // for Drive that is the post-confirm URL, and for everyone else it is wherever the
    // redirect chain landed. Re-asking the original would start the dance over.
    let source = resp.url().clone();

    let mut file = File::create(&path)?;
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut next = Some(resp);
    let mut breaks: u32 = 0;
    // Always set before it is read: nothing reports a stall without first recording why.
    let mut last_err: Option<String>;

    loop {
        let resp = match next.take() {
            Some(r) => r,
            // Without a `Content-Length` there is nothing to resume *against*: reqwest
            // hands back decompressed bytes for an encoded body, and that count means
            // nothing to a `Range` header. Ask for the file from the top instead.
            None => match resume_request(client, &source, total.map(|_| received)).await {
                Ok(Resumed::Partial(r)) => r,
                Ok(Resumed::Restarted(r)) => {
                    // The host ignored `Range` and started the file over, so we have to
                    // as well — appending its second copy onto our first would corrupt
                    // the archive in a way only the extractor would notice.
                    file = File::create(&path)?;
                    received = 0;
                    last_emit = 0;
                    r
                }
                // 416: it has nothing left to give, so what we hold is the whole file.
                Ok(Resumed::Exhausted) => break,
                Err(e) => {
                    last_err = Some(format!("{e:#}"));
                    breaks += 1;
                    if breaks > RESUME_ATTEMPTS {
                        return Err(stalled(received, total, breaks, last_err));
                    }
                    tokio::time::sleep(Duration::from_millis(600 * breaks as u64)).await;
                    continue;
                }
            },
        };

        let end = stream_to_file(app, slug, resp, &mut file, &mut received, &mut last_emit, total)
            .await?;
        // A body can come up short without erroring — some hosts just close the socket
        // cleanly mid-file. Content-Length is what says whether we actually have it all.
        let short = total.is_some_and(|t| received < t);
        match end {
            BodyEnd::Complete if !short => break,
            BodyEnd::Complete => {
                last_err = Some(format!(
                    "the host closed the connection after {}",
                    crate::bundle::human_size(received)
                ))
            }
            BodyEnd::Broken(e) => last_err = Some(e.to_string()),
        }

        breaks += 1;
        if breaks > RESUME_ATTEMPTS {
            return Err(stalled(received, total, breaks, last_err));
        }
        // Hold the last reported byte count on screen; the bar picks up where it stalled.
        emit(app, slug, "downloading", Some(received), total);
        tokio::time::sleep(Duration::from_millis(600 * breaks as u64)).await;
    }

    file.flush()?;
    emit(app, slug, "downloading", Some(received), total);
    Ok(path)
}

/// How a response body ended.
enum BodyEnd {
    /// The stream ran to its end. Says nothing about whether that was the end of the
    /// *file* — check the byte count against `Content-Length` for that.
    Complete,
    /// The connection broke mid-body. Kept for the message we show if resuming fails too.
    Broken(reqwest::Error),
}

/// Times a broken download is picked back up before we give up on it.
///
/// Mod mirrors drop long transfers routinely — a 400 MB track over a home connection can
/// lose its socket more than once — and every byte re-fetched is a byte the user already
/// waited for, so this is deliberately more patient than [`get_with_retry`].
const RESUME_ATTEMPTS: u32 = 5;

async fn stream_to_file(
    app: &AppHandle,
    slug: &str,
    resp: reqwest::Response,
    file: &mut File,
    received: &mut u64,
    last_emit: &mut u64,
    total: Option<u64>,
) -> anyhow::Result<BodyEnd> {
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => {
                file.write_all(&chunk)?;
                *received += chunk.len() as u64;
                if *received - *last_emit >= EMIT_EVERY_BYTES {
                    *last_emit = *received;
                    emit(app, slug, "downloading", Some(*received), total);
                }
            }
            // Not fatal by itself — the caller asks for the rest with a `Range` request.
            Err(e) => return Ok(BodyEnd::Broken(e)),
        }
    }
    Ok(BodyEnd::Complete)
}

/// What came back when we asked for the rest of a file.
#[derive(Debug)]
enum Resumed {
    /// A `206` carrying the bytes from where we left off: append them.
    Partial(reqwest::Response),
    /// A `200` — `Range` was ignored and this is the file from the top again.
    Restarted(reqwest::Response),
    /// A `416` — there is nothing past the offset we asked from, so we already have it all.
    Exhausted,
}

/// Ask for the rest of a file from byte `from`, or for the whole file again when `from`
/// is `None`.
async fn resume_request(
    client: &Client,
    url: &reqwest::Url,
    from: Option<u64>,
) -> anyhow::Result<Resumed> {
    let mut req = client.get(url.clone());
    if let Some(from) = from {
        req = req.header(reqwest::header::RANGE, format!("bytes={from}-"));
    }
    let resp = req.send().await?;
    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(Resumed::Exhausted);
    }
    let resp = resp.error_for_status()?;
    if content_type(&resp).starts_with("text/html") {
        // An expired CDN link answers with a page, not the rest of the file. Writing that
        // into the archive would surface much later as "couldn't determine the archive type".
        anyhow::bail!("the host answered with a web page instead of the rest of the file");
    }
    if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Ok(Resumed::Restarted(resp));
    }
    // A 206 is only appendable if it starts exactly where we left off. A host that
    // answers from somewhere else would leave a hole in the file that nothing downstream
    // could detect until extraction failed on a corrupt archive.
    match (from, content_range_start(&resp)) {
        (Some(want), Some(got)) if got != want => {
            anyhow::bail!("the host resumed from byte {got} instead of {want}")
        }
        _ => Ok(Resumed::Partial(resp)),
    }
}

/// The first byte a `206` covers, from its `Content-Range: bytes <start>-<end>/<total>`.
fn content_range_start(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())?
        .trim()
        .strip_prefix("bytes ")?
        .split('-')
        .next()?
        .trim()
        .parse()
        .ok()
}

/// The message for a download that kept breaking off. Says how far it got, so a transfer
/// dying at the same byte every time reads differently from one dying at random.
fn stalled(received: u64, total: Option<u64>, breaks: u32, last_err: Option<String>) -> anyhow::Error {
    let got = match total {
        Some(t) => format!(
            "{} of {}",
            crate::bundle::human_size(received),
            crate::bundle::human_size(t)
        ),
        None => crate::bundle::human_size(received),
    };
    let because = last_err
        .map(|e| format!(" ({e})"))
        .unwrap_or_default();
    anyhow::anyhow!(
        "The download kept breaking off — got {got} before the connection dropped, and {breaks} \
         attempts to pick it back up failed too{because}. The download host is struggling; try \
         again in a few minutes, or open the mod page to download it manually."
    )
}

fn content_type(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase()
}

/// Drive serves its refusals as ordinary 200 HTML pages whose `<title>` names the
/// reason ("Google Drive - Quota exceeded"). Translate the ones we recognise into
/// advice that's actually true — the catch-all "download it manually" is wrong for a
/// quota block, where a browser hits exactly the same wall.
fn gdrive_page_error(html: &str) -> Option<String> {
    let title = page_title(html)?.to_lowercase();
    let msg = if title.contains("quota") {
        "Google Drive has hit this file's download limit — too many people grabbed it recently. \
         Downloading it in a browser will fail the same way. Open the file in Drive, use \"Make a \
         copy\" to save it to your own Drive, then download your copy — or try again in a day."
    } else if title.contains("access denied") || title.contains("permission") {
        "This Google Drive file isn't shared publicly, so it can't be downloaded — the uploader \
         needs to fix its sharing settings."
    } else if title.contains("not found") {
        "This Google Drive file no longer exists — the uploader probably removed it. Check the mod \
         page for an updated link."
    } else {
        return None;
    };
    Some(msg.to_string())
}

fn page_title(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("title").ok()?;
    let title = doc.select(&sel).next()?.text().collect::<String>();
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
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
            // `sevenz-rust` joins entry names to the destination without filtering `..`,
            // so a hostile `.7z` can write outside `dest`. `zip` filters internally and
            // the native unrar side is unverified — sweep both rather than trust them.
            purge_escapees(archive, dest)?;
        }
        "rar" => {
            extract_rar(archive, dest)?;
            purge_escapees(archive, dest)?;
        }
        "pkz" | "pnt" => {
            // Already installable (.pkz/.pnt) — carry it through unchanged.
            let name = archive.file_name().unwrap_or_default();
            std::fs::copy(archive, dest.join(name))?;
        }
        other => anyhow::bail!("Unsupported archive type: .{other}"),
    }
    Ok(())
}

/// What a dropped path turned out to be once staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StagedKind {
    /// An archive that was unpacked into the staging directory.
    Archive,
    /// A single file carried through as-is — a `.pkz`, a `.pnt`, or something we don't
    /// recognise but the user still wants placed somewhere.
    Loose,
    /// A folder the user dropped. Staged by reference: nothing is copied until commit.
    Directory,
}

/// Stage one dropped path for classification.
///
/// Deliberately more permissive than [`extract_archive`]: the download path must reject a
/// mystery file (an HTML error page is not a mod), but a user who drags a bare `.edf` in has
/// told us they mean it. Unrecognised files become [`StagedKind::Loose`] and the classifier
/// then refuses to guess a destination for them.
pub(crate) fn stage_input(src: &Path, dest: &Path) -> anyhow::Result<StagedKind> {
    if src.is_dir() {
        return Ok(StagedKind::Directory);
    }
    if !src.is_file() {
        anyhow::bail!("not found: {}", src.display());
    }
    std::fs::create_dir_all(dest)?;
    match detect_ext(src) {
        Ok(ext) if matches!(ext.as_str(), "zip" | "7z" | "rar") => {
            extract_archive(src, dest)?;
            Ok(StagedKind::Archive)
        }
        // `.pkz`/`.pnt` are recognised but not containers, and anything else the user
        // dropped is carried through for the classifier to ask about.
        _ => {
            let name = src.file_name().unwrap_or_default();
            std::fs::copy(src, dest.join(name))?;
            Ok(StagedKind::Loose)
        }
    }
}

/// Refuse an archive that wrote outside its own staging directory.
///
/// Deleting the strays is not enough on its own — by the time we notice, the bytes are
/// already on disk — so this also fails the whole install rather than proceeding with a
/// half-extracted tree that a user would then be invited to commit.
fn purge_escapees(archive: &Path, dest: &Path) -> anyhow::Result<()> {
    let root = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
    let mut escaped = Vec::new();
    for entry in walkdir::WalkDir::new(dest).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        // Compare canonically so a symlink planted by the archive can't point out either.
        let real = match p.canonicalize() {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !real.starts_with(&root) {
            escaped.push(real);
        }
    }
    if escaped.is_empty() {
        return Ok(());
    }
    for p in &escaped {
        let _ = if p.is_dir() {
            std::fs::remove_dir_all(p)
        } else {
            std::fs::remove_file(p)
        };
    }
    anyhow::bail!(
        "{} tried to write {} file(s) outside the staging folder and was rejected",
        archive.file_name().unwrap_or_default().to_string_lossy(),
        escaped.len()
    )
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

pub(crate) fn detect_ext(archive: &Path) -> anyhow::Result<String> {
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

/// Content categories that live directly under `mods/`, across every title the app
/// drives — see [`crate::game::ALL_MODS_DIRS`] for why this is a union rather than the
/// active game's own list. Re-exported because the dropzone reports on it too.
pub(crate) use crate::game::ALL_MODS_DIRS as CATEGORY_DIRS;

/// Which rule in [`plan_placement`] decided the destination.
///
/// The dropzone renders this as the reason it shows the user ("found engine.scl + sfx.cfg"),
/// so the explanation and the routing can never disagree — they come from the same match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteRule {
    /// The archive carries a whole `mods/` tree; the caller's category is ignored.
    ModsTree,
    /// The archive root holds `bikes/`, `tracks/`, … — merged category by category.
    CategoryDirs,
    /// A `<Bike>/paints/…` bundle: bike content whatever the caller asked for.
    PaintsBundle,
    /// `<Bike>/{engine.scl,sfx.cfg}` — a packaged sound set.
    SoundBundle,
    /// Loose `engine.scl`+`sfx.cfg` at the root, destined for a bike the caller picked.
    LooseSound,
    /// Nothing self-describing was found; the caller's category decides.
    Typed,
}

/// What a placement will actually do, decided but not yet performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Merge one tree into one destination.
    Merge { src: PathBuf, dst: PathBuf },
    /// Merge several trees, each into its own destination (the category dirs).
    MergeEach { pairs: Vec<(PathBuf, PathBuf)> },
    /// The `place_plain` rules: `.pkz`-at-root wins, junk is dropped, loose files may
    /// get wrapped in a folder of their own.
    Plain {
        src: PathBuf,
        dst: PathBuf,
        slug: String,
        wrap_loose: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    pub rule: RouteRule,
    pub placement: Placement,
}

/// Decide where `extracted` belongs — without touching the filesystem.
///
/// Split out of [`place_mod`] so a preview and the write that follows it are produced by the
/// same code: the dropzone shows the user what [`writes_for`] enumerates, then commits by
/// applying the very same [`Placement`].
pub(crate) fn plan_placement(
    extracted: &Path,
    mods_dir: &Path,
    type_folder: &str,
    dest_folder: &str,
    slug: &str,
) -> Route {
    let route = |rule, placement| Route { rule, placement };

    // Check the extracted root FIRST so a `<Bike>/paints/` bundle isn't unwrapped to bare `paints/`.
    let unwrapped = unwrap_wrapper(extracted);
    let candidates: Vec<&Path> = if unwrapped == extracted {
        vec![extracted]
    } else {
        vec![extracted, unwrapped.as_path()]
    };

    for base in &candidates {
        if let Some(m) = child_dir(base, "mods") {
            return route(
                RouteRule::ModsTree,
                Placement::Merge {
                    src: m,
                    dst: mods_dir.to_path_buf(),
                },
            );
        }
    }

    for base in &candidates {
        let cats: Vec<PathBuf> = CATEGORY_DIRS
            .iter()
            .filter_map(|c| child_dir(base, c))
            .collect();
        if !cats.is_empty() {
            let pairs = cats
                .into_iter()
                .map(|c| {
                    let dst = mods_dir.join(c.file_name().unwrap_or_default());
                    (c, dst)
                })
                .collect();
            return route(RouteRule::CategoryDirs, Placement::MergeEach { pairs });
        }
    }

    // A `<Bike>/paints/…` bundle is bike content → route to `mods/bikes` regardless of
    // the caller's default type. Rider paints are exempt (kept under `mods/rider` below).
    if !type_folder.eq_ignore_ascii_case("rider") {
        for base in &candidates {
            if contains_paints_bundle(base) {
                return route(
                    RouteRule::PaintsBundle,
                    Placement::Merge {
                        src: base.to_path_buf(),
                        dst: mods_dir.join("bikes"),
                    },
                );
            }
        }
    }

    // Sound bundle (`engine.scl`+`sfx.cfg`): bike content that belongs at the bike root,
    // NEVER inside `paints/` → route to `mods/bikes` and drop any trailing `paints` segment.
    for base in &candidates {
        if contains_sound_bundle(base) {
            // `<Bike>/{engine.scl,sfx.cfg}` — merge the bike folder(s) as-is.
            return route(
                RouteRule::SoundBundle,
                Placement::Merge {
                    src: base.to_path_buf(),
                    dst: mods_dir.join("bikes"),
                },
            );
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
            return route(
                RouteRule::LooseSound,
                Placement::Plain {
                    src: base.to_path_buf(),
                    dst: dir,
                    slug: slug.to_string(),
                    wrap_loose: false,
                },
            );
        }
    }

    // Plain placement into the type folder, honoring the chosen destination sub-folder.
    let mut type_dir = mods_dir.join(type_folder);
    let segs: Vec<&str> = dest_folder.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    for seg in &segs {
        type_dir.push(sanitize(seg));
    }
    if let Some(name) = gear_model_folder(type_folder, &segs, extracted, &unwrapped, slug) {
        type_dir.push(name);
    }

    // A bike livery only loads from `<Bike>/paints/`. The picker offers a bike's root as a
    // destination too — that's where sounds and model swaps go — so a paint chosen there
    // would land one folder high and never show up in game. Drop it into `paints/` instead:
    // a loose `.pnt` at a bike root does nothing whatever, so this can only help.
    if type_folder.eq_ignore_ascii_case("bikes")
        && !segs.is_empty()
        && !segs[segs.len() - 1].eq_ignore_ascii_case("paints")
        && is_loose_paint_drop(&unwrapped)
    {
        type_dir.push("paints");
    }
    // Extracted tracks need their own folder; loose bike paints don't.
    let wrap_loose = type_folder.eq_ignore_ascii_case("tracks");
    route(
        RouteRule::Typed,
        Placement::Plain {
            src: unwrapped,
            dst: type_dir,
            slug: slug.to_string(),
            wrap_loose,
        },
    )
}

/// The folder a whole gear model needs under its area, or `None` when this placement isn't
/// installing one.
///
/// A gear model *is* a folder: the game loads `helmets/<Model>/helmet.edf`, and every picker
/// lists the folders an area contains. But the destination offered for a new model is the
/// bare area (`helmets`), and [`unwrap_wrapper`] has by then stripped the mod's own folder —
/// which is exactly the shape a packaged `.pkz` extracts to. Without this the model's files
/// landed loose in `mods/rider/helmets`, where nothing lists them and nothing can load them,
/// and the area picked up `paints`/`goggles` as if those were models of their own.
///
/// Named as the mod named itself, because that name is what the pickers will show. A mod
/// that ships its files bare has only the slug to go on.
///
/// Two placements are deliberately left alone. A paint drop names the model it belongs to
/// (`helmets/<Model>/paints`), so it is more than one segment and merges as before. And an
/// archive packed area-first — a `helmets/` folder holding the models — is already in the
/// shape the destination expects; wrapping it would bury the models a level down.
fn gear_model_folder(
    type_folder: &str,
    segs: &[&str],
    extracted: &Path,
    unwrapped: &Path,
    slug: &str,
) -> Option<String> {
    if !type_folder.eq_ignore_ascii_case("rider") {
        return None;
    }
    let [area] = segs else { return None };
    if !crate::game::is_rider_model_area(area) {
        return None;
    }
    let own = (unwrapped != extracted)
        .then(|| unwrapped.file_name()?.to_str())
        .flatten()
        .filter(|n| !crate::game::is_rider_model_area(n));
    let name = sanitize(own.unwrap_or(slug));
    (!name.trim().is_empty()).then_some(name)
}

/// What a placement does with a file that is already sitting at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnConflict {
    /// Overwrite it. Re-installing a mod is how a player updates it.
    Overwrite,
    /// Keep what's on disk. A preset bundle carries whole asset folders — the sender's
    /// helmet mesh rides along with the one paint they meant to share — so an import must
    /// fill in what's missing and never trade the receiver's copy for the sender's.
    Keep,
}

pub(crate) fn place_mod(
    extracted: &Path,
    mods_dir: &Path,
    type_folder: &str,
    dest_folder: &str,
    slug: &str,
) -> anyhow::Result<usize> {
    place_mod_with(extracted, mods_dir, type_folder, dest_folder, slug, OnConflict::Overwrite)
}

pub(crate) fn place_mod_with(
    extracted: &Path,
    mods_dir: &Path,
    type_folder: &str,
    dest_folder: &str,
    slug: &str,
    on_conflict: OnConflict,
) -> anyhow::Result<usize> {
    let route = plan_placement(extracted, mods_dir, type_folder, dest_folder, slug);
    apply(&route.placement, on_conflict).inspect_err(|e| {
        // The one place every install — download, import, drop — funnels through, so one
        // log line here covers all three. Without it a failed install left no trace at all
        // beyond a toast the player had already dismissed by the time they reported it.
        log::error!(
            "installing {slug} into {type_folder}/{dest_folder} ({:?}) failed: {e:#}",
            route.rule
        );
    })
}

/// Every `(source file, destination file)` a placement would produce, in order.
///
/// This is the single enumeration behind both the dropzone's preview and [`apply`]'s copy
/// loop, which is what makes "what the review sheet promised" and "what landed on disk"
/// the same list by construction rather than by careful maintenance.
pub(crate) fn writes_for(placement: &Placement) -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    match placement {
        Placement::Merge { src, dst } => walk_merge(src, dst, &mut out),
        Placement::MergeEach { pairs } => {
            for (src, dst) in pairs {
                walk_merge(src, dst, &mut out);
            }
        }
        Placement::Plain {
            src,
            dst,
            slug,
            wrap_loose,
        } => walk_plain(src, dst, slug, *wrap_loose, &mut out),
    }
    out
}

/// The directories a placement needs to exist even when it writes no files into them.
///
/// `place_plain` used to `create_dir_all` its target up front, and a track that extracts to
/// an empty folder still wants that folder; enumerating writes alone would silently drop it.
fn roots_for(placement: &Placement) -> Vec<PathBuf> {
    match placement {
        Placement::Merge { dst, .. } => vec![dst.clone()],
        Placement::MergeEach { pairs } => pairs.iter().map(|(_, dst)| dst.clone()).collect(),
        Placement::Plain { dst, .. } => vec![dst.clone()],
    }
}

/// Perform a placement, naming the file that failed.
///
/// The errors here reach the user verbatim, and an unadorned `io::Error` is close to
/// useless in a bug report: "os error 2" says a path was missing without saying which one,
/// or even whether it was the source or the destination. Every step says what it was doing
/// to what, and a failure is logged as well as returned — the toast is transient, the log
/// is what a player can send back.
fn apply(placement: &Placement, on_conflict: OnConflict) -> anyhow::Result<usize> {
    for root in roots_for(placement) {
        std::fs::create_dir_all(&root)
            .with_context(|| format!("creating {}", root.display()))?;
    }
    let writes = writes_for(placement);
    let mut written = 0;
    let mut kept = 0;
    for (src, dst) in &writes {
        // The skip sits here rather than in `writes_for` so the enumeration stays the one
        // description of what a placement covers — the dropzone's preview still lists every
        // file, whatever this install then decides to leave alone.
        if on_conflict == OnConflict::Keep && dst.exists() {
            kept += 1;
            continue;
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        copy_staged(src, dst)?;
        written += 1;
    }
    if kept > 0 {
        log::info!("kept {kept} file(s) already on disk, wrote {written}");
    }
    Ok(written)
}

/// How long [`copy_staged`] keeps waiting out a copy that still looks transient.
///
/// Two seconds all told: long enough for a scanner to finish with a freshly unpacked file,
/// short enough that a genuinely doomed install still fails while the user is watching.
const COPY_ATTEMPTS: u32 = 13;
const COPY_RETRY_WAIT: Duration = Duration::from_millis(150);

/// Copy one staged file into place, waiting out a failure that may not be permanent.
///
/// Every source here was listed by [`writes_for`]'s `read_dir` moments earlier, so a copy
/// that reports the file as missing is describing something that happened *since* — and on
/// Windows that is nearly always a real-time virus scanner reacting to a mod file appearing
/// in `%TEMP%`. Track `.pkz`es are a standing false positive: the scanner opens the new file
/// exclusively (or quarantines it outright) in the gap between the walk and the copy, and the
/// install died with a bare "The system cannot find the file specified. (os error 2)" naming
/// a file that had been there a moment earlier. Most such holds are released within a moment
/// — [`crate::frostmod_manage`] already waits one out on its own binary — so wait here too
/// rather than fail a whole install over it.
fn copy_staged(src: &Path, dst: &Path) -> anyhow::Result<()> {
    let mut waited = false;
    for attempt in 1..=COPY_ATTEMPTS {
        let err = match std::fs::copy(src, dst) {
            Ok(_) => {
                if waited {
                    // Worth a line: it says the install only survived because of the wait,
                    // which is the signal that a player's scanner needs an exclusion.
                    log::warn!(
                        "copying {} succeeded on attempt {attempt} — something was holding it",
                        src.display()
                    );
                }
                return Ok(());
            }
            Err(e) => e,
        };
        if attempt == COPY_ATTEMPTS || !worth_retrying(dst, &err) {
            return Err(copy_failure(src, dst, err));
        }
        waited = true;
        std::thread::sleep(COPY_RETRY_WAIT);
    }
    unreachable!("the loop returns on its last attempt")
}

/// Whether a failed copy could plausibly succeed if we tried again in a moment.
///
/// Waiting is only ever worth it for a *transient* holder. A folder sitting where the file
/// has to land is not one — it fails identically forever — so that case fails immediately
/// rather than making the user watch the retries run out.
fn worth_retrying(dst: &Path, e: &std::io::Error) -> bool {
    if dst.is_dir() {
        return false;
    }
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
    ) || matches!(
        e.raw_os_error(),
        // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION: someone else has the file open.
        // Rust has no stable `ErrorKind` for either, so match the Windows codes directly.
        Some(32) | Some(33)
    )
}

/// Turn a failed copy into something a player can act on.
///
/// A staged file that has gone missing is the one case where the raw error actively
/// misleads: it reads as if the *mod* were broken, when the bytes downloaded fine and
/// something on the machine took them away afterwards. Name the culprit and the folder to
/// exclude, because "os error 2" leaves a player with nowhere to go.
fn copy_failure(src: &Path, dst: &Path, e: std::io::Error) -> anyhow::Error {
    if e.kind() == std::io::ErrorKind::NotFound && !src.exists() {
        let name = src.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let folder = src
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| src.display().to_string());
        return anyhow::anyhow!(
            "{name} vanished from the staging folder part-way through the install. The \
             download itself worked — something deleted or quarantined the file before it \
             could be copied into place, which is almost always antivirus (mod .pkz files \
             are a common false positive) or a temp-folder cleaner. Add an exclusion for \
             {folder} and install it again."
        );
    }
    anyhow::Error::new(e).context(format!("copying {} to {}", src.display(), dst.display()))
}

fn walk_merge(src: &Path, dst: &Path, out: &mut Vec<(PathBuf, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(src) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    // `read_dir` order is filesystem-dependent; sort so a preview and its commit list the
    // same files in the same order (and so tests are reproducible).
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let target = dst.join(entry.file_name());
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_merge(&entry.path(), &target, out),
            Ok(t) if t.is_file() => out.push((entry.path(), target)),
            // Neither a file nor a directory: a symlink, a junction, or a link whose target
            // has gone away. Handing one of these to `fs::copy` fails the *entire* install
            // over a single entry — a dangling link with "os error 2", a junction with a
            // permission error — so leave it out and say so. Following it is not the
            // alternative: this walk runs over freshly extracted archives too, where a link
            // is a stranger's idea of where our files should go (see `linkwalk`).
            Ok(_) => log::warn!("skipping {} — not a regular file", entry.path().display()),
            Err(e) => log::warn!("skipping {} — {e}", entry.path().display()),
        }
    }
}

fn walk_plain(
    base: &Path,
    type_dir: &Path,
    slug: &str,
    wrap_loose: bool,
    out: &mut Vec<(PathBuf, PathBuf)>,
) {
    let Ok(rd) = std::fs::read_dir(base) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
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
        walk_merge(base, &target, out);
        return;
    }

    // A `.pkz` is the complete, installable package. When one sits at the root,
    // sibling folders are almost always extras the archive bundles alongside it —
    // the dedicated-"server" build and the unpacked track source. Install ONLY the
    // `.pkz` file(s) so those extras don't get dumped into the game folder.
    if has_pkz {
        for p in files.iter().filter(|p| has_ext(p, "pkz")) {
            let name = p.file_name().unwrap_or_default();
            out.push((p.clone(), type_dir.join(name)));
        }
        return;
    }

    for p in &files {
        let name = p.file_name().unwrap_or_default();
        if is_junk(&name.to_string_lossy()) {
            continue;
        }
        out.push((p.clone(), type_dir.join(name)));
    }
    for d in &dirs {
        let name = d.file_name().unwrap_or_default();
        walk_merge(d, &type_dir.join(name), out);
    }
}

pub(crate) fn unwrap_wrapper(dir: &Path) -> PathBuf {
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

pub(crate) fn child_dir(parent: &Path, name: &str) -> Option<PathBuf> {
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

pub(crate) fn dir_has_sound_markers(dir: &Path) -> bool {
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

pub(crate) fn contains_sound_bundle(base: &Path) -> bool {
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

/// Bare paints, ready to drop into a `paints/` folder: at least one `.pnt` sitting loose in
/// `base`, and no `.pkz`. The `.pkz` matters — it's a packaged bike or model set, which
/// belongs at the bike's root, and an archive that ships both is the package, not the paint.
fn is_loose_paint_drop(base: &Path) -> bool {
    let mut has_paint = false;
    let Ok(rd) = std::fs::read_dir(base) else {
        return false;
    };
    for e in rd.filter_map(|e| e.ok()) {
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = e.path();
        if has_ext(&path, "pkz") {
            return false;
        }
        has_paint |= has_ext(&path, "pnt");
    }
    has_paint
}

pub(crate) fn contains_paints_bundle(base: &Path) -> bool {
    std::fs::read_dir(base)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .any(|d| child_dir(&d.path(), "paints").is_some())
        })
        .unwrap_or(false)
}

pub(crate) fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

pub(crate) fn is_junk(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("readme")
        || n.ends_with(".txt")
        || n.ends_with(".url")
        || n.ends_with(".nfo")
        || n.ends_with(".md")
}

pub(crate) fn sanitize(name: &str) -> String {
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

    /// The shape that broke installs: the button's `href` is a placeholder and the real
    /// link is only there base64-scrambled, so reading `href` alone finds nothing.
    #[test]
    fn mediafire_link_comes_out_of_the_scrambled_attribute() {
        let html = r#"
          <html><body>
            <a id="downloadButton" class="input popsok" aria-label="Download file"
               href="javascript:void(0)"
               data-scrambled-url="aHR0cHM6Ly9kb3dubG9hZDIyMDIubWVkaWFmaXJlLmNvbS9hYmMvdHJhY2sucGt6">
               Download</a>
          </body></html>"#;
        assert_eq!(
            parse_mediafire_link(html).as_deref(),
            Some("https://download2202.mediafire.com/abc/track.pkz")
        );
    }

    /// `href` before `aria-label` — the old regex demanded the other order and missed this.
    #[test]
    fn mediafire_link_survives_attribute_order() {
        let html = r#"<a href="https://download1234.mediafire.com/xyz/bike.zip"
                         aria-label="Download file" id="downloadButton">Download</a>"#;
        assert_eq!(
            parse_mediafire_link(html).as_deref(),
            Some("https://download1234.mediafire.com/xyz/bike.zip")
        );
    }

    /// Protocol-relative and JSON-escaped forms both appear in the page's scripts.
    #[test]
    fn mediafire_link_from_page_source() {
        let relative = r#"<a id="downloadButton" href="//download99.mediafire.com/q/track.rar">go</a>"#;
        assert_eq!(
            parse_mediafire_link(relative).as_deref(),
            Some("https://download99.mediafire.com/q/track.rar")
        );

        let escaped = r#"<script>var u = "https:\/\/download7.mediafire.com\/k\/paint.pnt";</script>"#;
        assert_eq!(
            parse_mediafire_link(escaped).as_deref(),
            Some("https://download7.mediafire.com/k/paint.pnt")
        );
    }

    /// A page with nothing usable must say so rather than hand back `#` and fail later.
    #[test]
    fn mediafire_placeholder_href_is_not_a_link() {
        let html = r##"<a id="downloadButton" href="#">Download</a>"##;
        assert!(parse_mediafire_link(html).is_none());
    }

    /// Answer one request with a canned response, then drop the connection.
    fn serve_once(response: &'static [u8]) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                // Drain the request first — writing to a socket whose peer is still
                // sending can reset the connection before the response lands.
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf);
                let _ = sock.write_all(response);
                let _ = sock.flush();
            }
        });
        format!("http://127.0.0.1:{port}/mod.zip")
    }

    async fn resume_against(response: &'static [u8]) -> anyhow::Result<Resumed> {
        let client = build_client().unwrap();
        let url = reqwest::Url::parse(&serve_once(response)).unwrap();
        resume_request(&client, &url, Some(4)).await
    }

    /// A host that honours `Range` hands back the tail, and we append it.
    #[tokio::test]
    async fn resume_takes_a_206_as_the_rest_of_the_file() {
        let resp = resume_against(
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 4-7/8\r\nContent-Type: application/zip\r\n\r\ntail",
        )
        .await
        .expect("206 resumes");
        assert!(matches!(resp, Resumed::Partial(_)));
    }

    /// A host that ignores `Range` sends the file from the top — appending that onto what
    /// we already hold would produce an archive that only fails at extraction.
    #[tokio::test]
    async fn resume_takes_a_200_as_a_restart() {
        let resp = resume_against(
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nContent-Type: application/zip\r\n\r\nwholefil",
        )
        .await
        .expect("200 is a restart, not a failure");
        assert!(matches!(resp, Resumed::Restarted(_)));
    }

    /// 416 means there is nothing past our offset — we already have the whole file.
    #[tokio::test]
    async fn resume_takes_a_416_as_already_complete() {
        let resp = resume_against(
            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\r\n",
        )
        .await
        .expect("416 is not an error here");
        assert!(matches!(resp, Resumed::Exhausted));
    }

    /// A 206 that starts somewhere other than where we left off would leave a hole in the
    /// file — refuse it rather than write an archive that only fails at extraction.
    #[tokio::test]
    async fn resume_refuses_a_206_from_the_wrong_offset() {
        let err = resume_against(
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 6-9/10\r\nContent-Type: application/zip\r\n\r\ntail",
        )
        .await
        .expect_err("a 206 from byte 6 does not continue byte 4");
        assert!(format!("{err:#}").contains("instead of 4"), "{err:#}");
    }

    /// An expired CDN link answers with a page. That must not get appended to the archive.
    #[tokio::test]
    async fn resume_refuses_a_web_page() {
        let err = resume_against(
            b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nContent-Type: text/html\r\n\r\n<html>gone</h",
        )
        .await
        .expect_err("HTML is not the rest of the file");
        assert!(format!("{err:#}").contains("web page"), "{err:#}");
    }

    /// A Proton Drive share is end-to-end encrypted, so there is no direct URL to hand
    /// back. It has to fail *here*, with an explanation — fetching the link returns the
    /// web app's HTML, which previously sailed through and died much later at extraction
    /// with "couldn't determine the archive type", which tells the user nothing.
    #[tokio::test]
    async fn proton_drive_fails_with_an_explanation_not_a_download() {
        let client = build_client().unwrap();
        let err = resolve_direct_url(
            &client,
            "https://drive.proton.me/urls/ABC123XYZ#SomeKey",
            "Proton Drive",
        )
        .await
        .expect_err("an encrypted share can't resolve to a direct link");

        let msg = format!("{err:#}");
        assert!(msg.contains("encrypted"), "says why: {msg}");
        assert!(msg.contains("Choose file"), "says what to do instead: {msg}");
    }

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
    fn names_the_reason_drive_refused_the_file() {
        // Verbatim shape of the page Drive returns after the virus-scan confirm when a
        // file has been downloaded too often (seen on Flow Series #1 FlowiCompound).
        let quota = r#"<!DOCTYPE html><html><head><title>Google Drive - Quota exceeded</title></head>
            <body><p class="uc-error-caption">Sorry, you can't view or download this file at this time.</p></body></html>"#;
        let msg = gdrive_page_error(quota).expect("quota page should be recognised");
        assert!(msg.contains("download limit"), "{msg}");
        assert!(msg.contains("Make a copy"), "{msg}");

        let denied = r#"<html><head><title>Google Drive - Access Denied</title></head><body></body></html>"#;
        assert!(gdrive_page_error(denied)
            .expect("denied page should be recognised")
            .contains("shared publicly"));

        // The virus-scan interstitial is handled by the confirm form, not by this.
        let scan = r#"<html><head><title>Google Drive - Virus scan warning</title></head><body></body></html>"#;
        assert!(gdrive_page_error(scan).is_none());
        assert!(gdrive_page_error("<html><body>no title</body></html>").is_none());
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

    /// `zip` filters `..` out of entry names itself, so a zip-slip archive should extract
    /// harmlessly *inside* the staging directory rather than escaping it. This pins that
    /// behaviour: if a future `zip` upgrade regressed it, the sweep still has to catch it.
    #[test]
    fn a_zip_that_climbs_out_stays_inside_the_staging_folder() -> anyhow::Result<()> {
        let base = place_tmp("zipslip");
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside)?;

        let archive = base.join("evil.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&archive)?);
            w.start_file::<_, ()>("../outside/pwned.txt", zip::write::SimpleFileOptions::default())?;
            w.write_all(b"nope")?;
            w.finish()?;
        }

        let dest = base.join("staged");
        std::fs::create_dir_all(&dest)?;
        let _ = extract_archive(&archive, &dest);

        assert!(
            !outside.join("pwned.txt").exists(),
            "an archive escaped the staging folder"
        );
        let _ = std::fs::remove_dir_all(&base);
        Ok(())
    }

    /// The sweep itself, exercised directly: a file planted outside the staging directory
    /// must be removed and the extraction reported as failed rather than silently accepted.
    #[test]
    fn an_escaped_file_fails_the_extraction_and_is_deleted() -> anyhow::Result<()> {
        let base = place_tmp("escape");
        let dest = base.join("staged");
        std::fs::create_dir_all(&dest)?;
        touch(&dest.join("fine.txt"));

        // A symlink is the escape route a `..`-filtering extractor still leaves open.
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside)?;
        touch(&outside.join("secret.txt"));
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, dest.join("link"))?;
        #[cfg(not(unix))]
        return Ok(());

        #[cfg(unix)]
        {
            let err = purge_escapees(Path::new("evil.7z"), &dest).unwrap_err();
            assert!(err.to_string().contains("outside"), "{err}");
            assert!(dest.join("fine.txt").exists(), "innocent files kept");
            let _ = std::fs::remove_dir_all(&base);
            Ok(())
        }
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

    /// Re-installing a mod is how a player updates it, so the default has to keep clobbering.
    /// Only [`OnConflict::Keep`] — the preset-bundle import — steps around what's on disk.
    #[test]
    fn reinstalling_overwrites_by_default() {
        let root = place_tmp("overwrite");
        let ex = root.join("ex");
        std::fs::create_dir_all(&ex).unwrap();
        std::fs::write(ex.join("track.pkz"), b"new").unwrap();

        let mods = root.join("mods");
        std::fs::create_dir_all(mods.join("tracks")).unwrap();
        std::fs::write(mods.join("tracks/track.pkz"), b"old").unwrap();

        let placed = place_mod(&ex, &mods, "tracks", "", "slug").unwrap();

        assert_eq!(placed, 1);
        assert_eq!(std::fs::read(mods.join("tracks/track.pkz")).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&root);
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

    /// A packaged helmet extracts to a single `<Model>/` folder, which `unwrap_wrapper`
    /// strips — and the destination offered for a new model is the bare area. Together
    /// those scattered the mesh and its `paints/` straight into `mods/rider/helmets`,
    /// where the game can't load it and the picker lists `paints` as a helmet.
    #[test]
    fn a_new_gear_model_keeps_a_folder_of_its_own() {
        let root = place_tmp("gear-model-folder");
        let ex = root.join("ex");
        touch(&ex.join("Astars_SM10_EKS/gfx.cfg"));
        touch(&ex.join("Astars_SM10_EKS/helmet.edf"));
        touch(&ex.join("Astars_SM10_EKS/paints/Red.pnt"));
        touch(&ex.join("Astars_SM10_EKS/goggles/Smoke.pnt"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "rider", "helmets", "astars-sm10").unwrap();

        let model = mods.join("rider/helmets/Astars_SM10_EKS");
        assert!(model.join("helmet.edf").exists(), "the mesh is under the model's own name");
        assert!(model.join("gfx.cfg").exists());
        assert!(model.join("paints/Red.pnt").exists(), "paints travel with the model");
        assert!(model.join("goggles/Smoke.pnt").exists());
        assert!(
            !mods.join("rider/helmets/helmet.edf").exists(),
            "nothing is left loose in the area folder"
        );
        assert!(
            !mods.join("rider/helmets/paints").exists(),
            "`paints` must not become a sibling of the models — the picker reads it as one"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Nothing to take the name from, so the slug is what's left. Still a folder: loose
    /// files in the area root are unloadable however they got there.
    #[test]
    fn a_bare_gear_model_is_named_from_the_slug() {
        let root = place_tmp("gear-model-slug");
        let ex = root.join("ex");
        touch(&ex.join("gfx.cfg"));
        touch(&ex.join("boots.edf"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "rider", "boots", "tech-10").unwrap();
        assert!(mods.join("rider/boots/tech-10/boots.edf").exists());
        assert!(!mods.join("rider/boots/boots.edf").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Packed area-first: the archive's `helmets/` already *is* the destination, so its
    /// children are the models. Wrapping that would bury every one of them a level down.
    #[test]
    fn an_area_first_archive_is_not_wrapped_again() {
        let root = place_tmp("gear-area-first");
        let ex = root.join("ex");
        touch(&ex.join("helmets/Fox V3/gfx.cfg"));
        touch(&ex.join("helmets/Fox V3/helmet.edf"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "rider", "helmets", "fox-v3").unwrap();
        assert!(mods.join("rider/helmets/Fox V3/helmet.edf").exists());
        assert!(!mods.join("rider/helmets/helmets").exists(), "no doubled area folder");
        assert!(!mods.join("rider/helmets/fox-v3").exists(), "no slug folder over the models");
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
    fn loose_livery_chosen_against_a_bike_root_lands_in_paints() {
        // The picker offers `<Bike>` as well as `<Bike>/paints` (sounds and model swaps need
        // the root), and `resolveInitialFolder` will preselect the root when that's what was
        // remembered. A `.pnt` left there is invisible to the game.
        let root = place_tmp("livery-bike-root");
        let ex = root.join("ex");
        touch(&ex.join("cool.pnt"));
        touch(&ex.join("preview.jpg")); // paints ship a thumbnail; it travels along
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "MX1OEM_2023_KTM_450_SX-F", "cool-livery").unwrap();
        assert!(mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt")
            .exists());
        assert!(!mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/cool.pnt")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bike_package_chosen_against_a_bike_root_stays_at_the_root() {
        // A `.pkz` is a bike or model set — the root is exactly where it belongs.
        let root = place_tmp("pkz-bike-root");
        let ex = root.join("ex");
        touch(&ex.join("FrostMod Models.pkz"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "MX1OEM_2023_KTM_450_SX-F", "swap").unwrap();
        assert!(mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/FrostMod Models.pkz")
            .exists());
        assert!(!mods.join("bikes/MX1OEM_2023_KTM_450_SX-F/paints").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn paints_destination_is_not_doubled_up() {
        let root = place_tmp("livery-already-paints");
        let ex = root.join("ex");
        touch(&ex.join("cool.pnt"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "MX1OEM_2023_KTM_450_SX-F/paints", "slug").unwrap();
        assert!(mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/paints/cool.pnt")
            .exists());
        assert!(!mods
            .join("bikes/MX1OEM_2023_KTM_450_SX-F/paints/paints")
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loose_paint_at_the_bikes_root_is_left_alone() {
        // No bike was chosen, so there's no `paints` folder to redirect into — inventing
        // `bikes/paints` would be just as dead and harder to find.
        let root = place_tmp("livery-no-bike");
        let ex = root.join("ex");
        touch(&ex.join("cool.pnt"));
        let mods = root.join("mods");
        place_mod(&ex, &mods, "bikes", "", "slug").unwrap();
        assert!(mods.join("bikes/cool.pnt").exists());
        assert!(!mods.join("bikes/paints").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two installs alive at once must not share a staging folder.
    ///
    /// The download path used to name its work folder after the slug and wipe it on entry,
    /// so retrying a failed install — a second run of the *same* slug — deleted the first
    /// run's extracted files while it was still copying them. The player saw the copy fail
    /// on a file that had been there a moment earlier ("os error 2").
    #[test]
    fn staging_dirs_never_collide() {
        let a = staging_dir("dl");
        let b = staging_dir("dl");
        assert_ne!(a, b, "a second install would wipe the first one's files");
    }

    /// An install that fails must say which file it died on. "os error 2" on its own is
    /// unactionable in a bug report — it names neither the path nor which end it was.
    #[test]
    fn a_failed_copy_names_both_paths() {
        let root = place_tmp("copy-err");
        let ex = root.join("ex");
        touch(&ex.join("cool.pnt"));
        // A directory sitting where the file has to land: `copy` cannot overwrite it.
        let mods = root.join("mods");
        std::fs::create_dir_all(mods.join("bikes/cool.pnt")).unwrap();

        let err = place_mod(&ex, &mods, "bikes", "", "slug").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("copying"), "{msg}");
        assert!(msg.contains("cool.pnt"), "{msg}");
        assert!(msg.contains(&mods.display().to_string()), "{msg}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A staged file that disappears mid-install is not a broken mod, and the raw error
    /// says the opposite: "cannot find the file specified" reads as if the download had
    /// failed. Name the file and the folder to exclude instead.
    #[test]
    fn a_vanished_staged_file_blames_the_right_thing() {
        let root = place_tmp("vanished");
        let staging = root.join("ex");
        std::fs::create_dir_all(&staging).unwrap();
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).unwrap();

        // The state `apply` finds itself in once a scanner has taken the file away: the
        // walk listed it, the copy can no longer see it.
        let err = copy_staged(&staging.join("Scottsdale.pkz"), &mods.join("Scottsdale.pkz"))
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("Scottsdale.pkz"), "{msg}");
        assert!(msg.contains(&staging.display().to_string()), "{msg}");
        assert!(msg.to_lowercase().contains("antivirus"), "{msg}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A scanner holding a just-unpacked file lets go within a moment. Waiting it out is
    /// the difference between an install that works and one that fails on a file the user
    /// can plainly see on disk.
    #[test]
    fn a_copy_waits_out_whatever_is_holding_the_file() {
        let root = place_tmp("copy-retry");
        let staging = root.join("ex");
        std::fs::create_dir_all(&staging).unwrap();
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let src = staging.join("Scottsdale.pkz");
        let dst = mods.join("Scottsdale.pkz");

        // Stands in for the scanner releasing the file: unreadable at the first attempt,
        // there well inside the retry window.
        let late = src.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            std::fs::write(&late, b"track").unwrap();
        });

        copy_staged(&src, &dst).expect("the copy should have waited");
        writer.join().unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"track");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Waiting is for holders that let go. A folder sitting where the file has to land
    /// never will, so that failure must not sit through the whole retry window.
    #[test]
    fn a_hopeless_copy_gives_up_immediately() {
        let root = place_tmp("copy-hopeless");
        let staging = root.join("ex");
        touch(&staging.join("cool.pnt"));
        let dst = root.join("mods/cool.pnt");
        std::fs::create_dir_all(&dst).unwrap();

        let start = std::time::Instant::now();
        assert!(copy_staged(&staging.join("cool.pnt"), &dst).is_err());
        assert!(
            start.elapsed() < COPY_RETRY_WAIT,
            "a directory in the way was retried"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A link whose target is gone is one entry, not a reason to fail the whole install.
    /// `fs::copy` follows it into nothing and reports the same "os error 2" a missing
    /// source does, which used to take the other files down with it.
    #[cfg(unix)]
    #[test]
    fn a_dangling_link_is_skipped_rather_than_fatal() {
        let root = place_tmp("dangling");
        let ex = root.join("ex");
        touch(&ex.join("real.pnt"));
        std::os::unix::fs::symlink(ex.join("gone.pnt"), ex.join("broken.pnt")).unwrap();

        let mods = root.join("mods");
        let n = place_mod(&ex, &mods, "bikes", "KTM/paints", "slug").unwrap();
        assert_eq!(n, 1, "only the real file is installed");
        assert!(mods.join("bikes/KTM/paints/real.pnt").exists());
        assert!(!mods.join("bikes/KTM/paints/broken.pnt").exists());
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
