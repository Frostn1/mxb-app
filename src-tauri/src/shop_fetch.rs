//! The signed-in half of mxbikes-shop.com, done from inside a real browser.
//!
//! Every path the purchases flow touches is behind a Cloudflare *managed* challenge, which is
//! cleared by running its JavaScript — so `reqwest` can't, and a `cf_clearance` earned in a
//! WebView isn't portable to it (see [`crate::mxb_fetch`], which solved the same problem for
//! the mods catalogs). The catalog's exemption doesn't generalise: Cloudflare skips the one
//! `/frostmod-mods.json` path, and `X-Frostmod-Key` is an origin check, not a bypass.
//!
//! So a hidden WebView stays parked on the shop, signed in, and does the work: the purchases
//! page is read out of the DOM (a `fetch()` to a challenged URL just returns the interstitial —
//! only a navigation runs the challenge), and a purchased file is downloaded by the browser
//! itself, with the destination redirected into our staging directory so no bytes go through
//! JavaScript.

use crate::shop_session::SHOP_BASE;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

/// The window everything here runs in. Public because the window-event handler and the IPC
/// guard both have to recognise it — it runs a remote origin, so it is one of the two webviews
/// allowed to talk to us from one, and it must never be parked in the tray.
pub const WINDOW: &str = "shop-fetch";

/// The event the injected script returns page HTML on. Public for the same reason: this is the
/// only IPC the window is permitted to perform.
pub const RESULT_EVENT: &str = "shop-fetch:done";

/// The purchases page. The window lives here — it is both what we read and what earns the
/// clearance every other request then rides on.
const DOWNLOADS_PATH: &str = "/all-my-downloads/";

/// How long the window gets to load and clear Cloudflare. Generous: the first navigation of a
/// session pays for the challenge, and a managed one is not always quick.
const READY_TIMEOUT: Duration = Duration::from_secs(45);

/// How long to wait for a download to *start* — that is, for `on_download` to fire. A managed
/// challenge served in place of the file is the failure this catches: the window sits on an
/// interstitial and no download is ever requested.
const DOWNLOAD_START_TIMEOUT: Duration = Duration::from_secs(45);

/// How long a download may then take. Purchased tracks run to hundreds of megabytes.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 30);

/// Same cadence as [`crate::install`]'s streaming download, so the grid's progress bar behaves
/// the same whichever path a file came down.
const PROGRESS_EVERY: Duration = Duration::from_millis(400);

static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    listen_for_results(app);
}

fn app() -> anyhow::Result<&'static AppHandle> {
    APP.get()
        .ok_or_else(|| anyhow::anyhow!("the shop WebView bridge was never initialised"))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ─────────────────────────────── reading the page ───────────────────────────────

/// What the injected script sends back.
#[derive(Debug, Deserialize)]
struct Reply {
    id: u64,
    #[serde(default)]
    html: String,
}

type Waiting = Mutex<std::collections::HashMap<u64, tokio::sync::oneshot::Sender<Reply>>>;

fn waiting() -> &'static Waiting {
    static WAITING: OnceLock<Waiting> = OnceLock::new();
    WAITING.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// An id we did not issue is dropped. That is the guard against the remote page inventing
/// results: script on the shop can emit whatever it likes on this event, and the worst it
/// achieves is a debug line.
fn deliver(reply: Reply) {
    let Some(tx) = lock(waiting()).remove(&reply.id) else {
        log::debug!("ignoring a shop WebView result for unknown id {}", reply.id);
        return;
    };
    let _ = tx.send(reply);
}

fn listen_for_results(app: &AppHandle) {
    app.listen(RESULT_EVENT, |event| {
        match serde_json::from_str::<Reply>(event.payload()) {
            Ok(reply) => deliver(reply),
            // Never panic on a payload from a remote page.
            Err(e) => log::warn!("unreadable shop WebView result: {e}"),
        }
    });
}

/// The purchases page's HTML, read from the window that is already displaying it.
///
/// `reload` re-navigates first, which is what "Refresh" has to mean — the DOM otherwise stays
/// on whatever was fetched when the window opened.
pub async fn downloads_html(reload: bool) -> anyhow::Result<String> {
    let app = app()?;
    let window = ensure_window(app).await?;

    if reload {
        let url = format!("{SHOP_BASE}{DOWNLOADS_PATH}");
        window.navigate(url.parse()?)?;
        wait_until_ready(&window).await?;
    }

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = tokio::sync::oneshot::channel();
    lock(waiting()).insert(id, tx);

    if let Err(e) = window.eval(read_script(id)) {
        lock(waiting()).remove(&id);
        return Err(anyhow::anyhow!("could not read the shop page: {e}"));
    }

    let reply = match tokio::time::timeout(Duration::from_secs(15), rx).await {
        Ok(Ok(reply)) => reply,
        _ => {
            lock(waiting()).remove(&id);
            return Err(anyhow::anyhow!("the shop page did not answer in time"));
        }
    };
    Ok(reply.html)
}

/// Read the document rather than `fetch` it.
///
/// `mxb_fetch` runs `fetch()` because it needs arbitrary URLs. Here there is exactly one page
/// and the window is on it, and that distinction matters: `fetch()` to a challenged URL returns
/// the interstitial, since only a navigation runs the challenge that clears it.
fn read_script(id: u64) -> String {
    format!(
        r#"(function(){{
  try {{
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {{
      event: {event},
      payload: {{ id: {id}, html: document.documentElement.outerHTML }}
    }});
  }} catch (e) {{}}
}})();"#,
        event = js_string(RESULT_EVENT),
    )
}

/// A JS string literal, for the one constant we interpolate.
fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' => out.push_str("\\u003C"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ──────────────────────────────── downloading a file ────────────────────────────────

/// One download in flight. Only one runs at a time — the window can only navigate to one URL.
#[derive(Default)]
struct Download {
    /// Where we told the browser to put the file, from the `Requested` hook.
    path: Option<PathBuf>,
    /// Set by `Finished`. `None` while the download is still running.
    outcome: Option<bool>,
}

fn download_state() -> &'static Mutex<Download> {
    static STATE: OnceLock<Mutex<Download>> = OnceLock::new();
    STATE.get_or_init(Mutex::default)
}

/// Where a download in progress should be put. Set before navigating, read by the hook.
fn download_dir() -> &'static Mutex<Option<PathBuf>> {
    static DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    DIR.get_or_init(|| Mutex::new(None))
}

/// Serialises downloads, since they all share the one window.
fn download_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Download a purchased file into `dir`, and return where it landed.
///
/// Signature-compatible with [`crate::install::download`] on purpose: `shop_stage` swaps one
/// for the other and nothing downstream of the bytes — `dropzone::plan`, the review sheet,
/// `commit_drop` — moves at all.
pub async fn download(app: &AppHandle, slug: &str, url: &str, dir: &Path) -> anyhow::Result<PathBuf> {
    let _guard = download_lock().lock().await;
    let window = ensure_window(app).await?;

    *lock(download_dir()) = Some(dir.to_path_buf());
    *lock(download_state()) = Download::default();

    crate::install::emit_progress(app, slug, "downloading", Some(0), None);
    window.navigate(url.parse()?)?;

    let path = wait_for_start(app, slug).await?;
    wait_for_finish(app, slug, &path).await?;

    // Put the window back where it belongs, so the next read finds the purchases page rather
    // than whatever the download navigation left behind.
    let back = format!("{SHOP_BASE}{DOWNLOADS_PATH}");
    if let Ok(url) = back.parse() {
        let _ = window.navigate(url);
    }

    Ok(path)
}

/// Wait for `on_download` to fire.
///
/// The failure this exists for: if Cloudflare answers the file URL with a challenge instead of
/// the file, the window quietly renders an interstitial and no download is ever requested. That
/// has to become a real error rather than a hang.
async fn wait_for_start(app: &AppHandle, slug: &str) -> anyhow::Result<PathBuf> {
    let deadline = std::time::Instant::now() + DOWNLOAD_START_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(path) = lock(download_state()).path.clone() {
            return Ok(path);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    crate::install::emit_progress(app, slug, "downloading", None, None);
    Err(anyhow::anyhow!(
        "MX Bikes Shop never started the download — Cloudflare is most likely challenging it. \
         Try signing out and in again."
    ))
}

/// Wait for the download to finish, reporting progress from the file itself.
///
/// `on_download` reports a start and a finish and nothing in between, so the size on disk is
/// the only progress signal there is. No total comes with it, so the grid shows a rising byte
/// count rather than a percentage — worse than the streaming path, and much better than a bar
/// that sits still for several hundred megabytes.
async fn wait_for_finish(app: &AppHandle, slug: &str, path: &Path) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + DOWNLOAD_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Some(success) = lock(download_state()).outcome {
            if !success {
                anyhow::bail!("the download failed — the store or the connection ended it early");
            }
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if size == 0 {
                anyhow::bail!("the download finished but wrote nothing");
            }
            crate::install::emit_progress(app, slug, "downloading", Some(size), None);
            return Ok(());
        }
        if let Ok(meta) = std::fs::metadata(path) {
            crate::install::emit_progress(app, slug, "downloading", Some(meta.len()), None);
        }
        tokio::time::sleep(PROGRESS_EVERY).await;
    }
    Err(anyhow::anyhow!("the download did not finish within 30 minutes"))
}

/// The `on_download` hook, installed on the window.
///
/// Tauri hands us the destination the browser picked, and that is the *only* place the real
/// filename exists: an Easy Digital Downloads URL is `/?edd_action=file_download&…` with no
/// name in the path, so the extension that `extract_archive` dispatches on comes from
/// `Content-Disposition` by way of this suggestion. So the name is kept and only the directory
/// is changed.
fn on_download(event: tauri::webview::DownloadEvent<'_>) -> bool {
    match event {
        tauri::webview::DownloadEvent::Requested { url, destination } => {
            let Some(dir) = lock(download_dir()).clone() else {
                log::warn!("the shop window asked to download {url} with nowhere to put it");
                return false;
            };
            let name = destination
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "purchase.zip".to_string());
            let target = dir.join(sanitize(&name));
            log::info!("shop download starting -> {}", target.display());
            *destination = target.clone();
            lock(download_state()).path = Some(target);
            true
        }
        tauri::webview::DownloadEvent::Finished { url, path, success } => {
            // On macOS `path` is always `None` by platform limitation, so what we assigned in
            // `Requested` is the only record of where the file went. `success` is what this
            // event is actually for.
            log::info!("shop download finished (success={success}, path={path:?}) for {url}");
            lock(download_state()).outcome = Some(success);
            true
        }
        _ => true,
    }
}

/// The filename comes from a remote header, so it must not be able to climb out of the staging
/// directory or name a path at all.
fn sanitize(name: &str) -> String {
    let name = name.trim().trim_matches('.');
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '\0' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return "purchase.zip".to_string();
    }
    cleaned
}

// ──────────────────────────────────── the window ────────────────────────────────────

/// Build the hidden window on first use and leave it up for the session.
async fn ensure_window(app: &AppHandle) -> anyhow::Result<tauri::WebviewWindow> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    static READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    if let Some(existing) = app.get_webview_window(WINDOW) {
        if READY.load(Ordering::Relaxed) {
            return Ok(existing);
        }
        wait_until_ready(&existing).await?;
        READY.store(true, Ordering::Relaxed);
        return Ok(existing);
    }

    let url: tauri::Url = format!("{SHOP_BASE}{DOWNLOADS_PATH}").parse()?;
    log::info!("opening the hidden mxbikes-shop.com window");
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title("mxbikes-shop.com")
        .on_download(|_webview, event| on_download(event))
        // Never shown, for the reasons [`crate::mxb_fetch`] sets out at length: off-screen
        // still leaves a window the system lists, and the webview keeps its own visibility
        // flag, so the page is never treated as backgrounded.
        .visible(false)
        .inner_size(1.0, 1.0)
        .position(-32000.0, -32000.0)
        .skip_taskbar(true)
        .decorations(false)
        .focused(false)
        .build()?;

    wait_until_ready(&window).await?;
    READY.store(true, Ordering::Relaxed);
    Ok(window)
}

/// Wait for the page to be able to run our script.
///
/// Two things have to be true: the IPC primitive has to be injected, and Cloudflare's managed
/// challenge — which this window is served on first navigation, exactly as the login window
/// is — has to have cleared. Polling a one-line `eval` covers both, since the challenge page is
/// replaced by the real one when it passes.
async fn wait_until_ready(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + READY_TIMEOUT;
    let mut last: Option<String> = None;
    while std::time::Instant::now() < deadline {
        match probe(window).await {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(anyhow::anyhow!(
        "the MX Bikes Shop page never became ready — Cloudflare's check did not clear{}",
        last.map(|e| format!(" ({e})")).unwrap_or_default()
    ))
}

/// One round-trip that proves script can run and reach us, *and* that we are past the
/// challenge — answering only when both are true is what makes this a readiness check rather
/// than a liveness one.
///
/// "Past the challenge" is asked as `window._cf_chl_opt`, which the interstitial defines and an
/// ordinary page does not. The obvious test — looking for `challenge-platform` in the HTML — is
/// wrong, and quietly so: Cloudflare injects its `challenge-platform/scripts/jsd` script into
/// *ordinary* pages as well when JS detections are on, so that string is no evidence of a
/// challenge and the window would never be declared ready.
async fn probe(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    const PROBE_ID: u64 = 0;
    let (tx, rx) = tokio::sync::oneshot::channel();
    lock(waiting()).insert(PROBE_ID, tx);
    let js = format!(
        "if (window.__TAURI_INTERNALS__ && document.readyState === 'complete' \
         && typeof window._cf_chl_opt === 'undefined' \
         && !/^just a moment/i.test(document.title || '')) {{ \
         window.__TAURI_INTERNALS__.invoke\
         ('plugin:event|emit', {{ event: {event}, payload: {{id:0}} }}); }}",
        event = js_string(RESULT_EVENT)
    );
    if let Err(e) = window.eval(&js) {
        lock(waiting()).remove(&PROBE_ID);
        return Err(anyhow::anyhow!("{e}"));
    }
    match tokio::time::timeout(Duration::from_millis(400), rx).await {
        Ok(Ok(_)) => Ok(()),
        _ => {
            lock(waiting()).remove(&PROBE_ID);
            Err(anyhow::anyhow!("still on the challenge, or not ready yet"))
        }
    }
}

/// Drop the window, so the next use rebuilds it. Signing out has to do this, or a window that
/// is still signed in keeps answering.
pub fn close(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW) {
        let _ = win.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name comes from a remote `Content-Disposition`. The property that matters is not
    /// what it gets rewritten *to* — that is cosmetic — but that whatever comes out is a single
    /// plain filename, so joining it to the staging directory cannot land anywhere else.
    #[test]
    fn a_hostile_filename_cannot_escape_the_staging_directory() {
        use std::path::{Component, Path};

        for hostile in [
            "../../etc/passwd",
            "/absolute.zip",
            "a\\b.zip",
            "..",
            ".",
            "",
            "   ",
            "evil\r\nname.zip",
            "C:\\Windows\\System32\\evil.dll",
            "\0.zip",
        ] {
            let safe = sanitize(hostile);
            let path = Path::new(&safe);
            assert!(!safe.is_empty(), "{hostile:?} sanitized to nothing");
            assert!(
                !safe.contains('/') && !safe.contains('\\'),
                "{hostile:?} -> {safe:?} still carries a separator"
            );
            assert!(
                !safe.contains('\0') && !safe.contains('\n') && !safe.contains('\r'),
                "{hostile:?} -> {safe:?} still carries a control character"
            );
            let mut parts = path.components();
            assert!(
                matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none(),
                "{hostile:?} -> {safe:?} is not a single plain filename"
            );
            // The invariant stated the way it is actually relied on.
            assert_eq!(
                Path::new("/tmp/staging").join(&safe).parent(),
                Some(Path::new("/tmp/staging")),
                "{hostile:?} -> {safe:?} escaped the staging directory"
            );
        }
    }

    /// The ordinary case has to survive untouched — the extension is what `extract_archive`
    /// dispatches on, so losing it would break every install.
    #[test]
    fn an_ordinary_filename_is_left_alone() {
        assert_eq!(sanitize("Sunshine MX PRO.pkz"), "Sunshine MX PRO.pkz");
        assert_eq!(sanitize("track-v1.2.zip"), "track-v1.2.zip");
    }

    #[test]
    fn the_read_script_carries_the_id_and_event() {
        let js = read_script(7);
        assert!(js.contains("id: 7"), "{js}");
        assert!(js.contains(RESULT_EVENT), "{js}");
        assert!(js.contains("outerHTML"), "{js}");
        // Tauri encodes the payload itself; stringifying first lands a quoted string where the
        // listener expects a struct, and every reply is dropped as unreadable.
        assert!(!js.contains("JSON.stringify"), "{js}");
    }

    /// The remote page can emit anything on this event. A result for an id we never issued has
    /// to be dropped, not delivered to whoever happens to be waiting.
    #[test]
    fn a_reply_for_an_unknown_id_is_dropped() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        lock(waiting()).insert(4242, tx);

        deliver(Reply { id: 4243, html: "injected".into() });
        assert!(rx.try_recv().is_err(), "a reply for another id must not resolve this request");

        deliver(Reply { id: 4242, html: "ours".into() });
        assert_eq!(rx.try_recv().unwrap().html, "ours");
    }

    #[test]
    fn js_strings_escape_what_would_break_out() {
        assert_eq!(js_string(r#"a"b"#), r#""a\"b""#);
        assert!(!js_string("</script>").contains('<'));
    }
}
