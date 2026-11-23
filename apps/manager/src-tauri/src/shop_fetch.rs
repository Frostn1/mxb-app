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
//!
//! When Cloudflare's check will not clear by itself, the window is shown for the user to
//! complete it and hidden again once it is past — the same treatment, and the same code
//! ([`crate::cf_check`]), as the mods window. Closing it fails the request with a plain message
//! until the user asks again (Retry, Refresh, or an install).

use crate::cf_check::{self, GiveUp};
use crate::shop_session::SHOP_BASE;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
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

/// How long a page that is simply *loading* — not challenged — gets to be ready. A challenge
/// is timed separately: [`cf_check`] shows it to the user if it doesn't clear by itself.
const READY_TIMEOUT: Duration = Duration::from_secs(45);

/// Tells the main window what the check is doing. Payload: `{ state, site }`.
pub const VERIFY_EVENT: &str = "shop-verify";

/// The check, as [`cf_check`] runs it for this window.
static CHECK_STATE: cf_check::State = cf_check::State::new();
static CHECK: cf_check::Site = cf_check::Site {
    window: WINDOW,
    domain,
    base,
    purpose: "load your purchases",
    event: VERIFY_EVENT,
    ready_timeout: READY_TIMEOUT,
    stopped,
    state: &CHECK_STATE,
};

const DOMAIN: &str = "mxbikes-shop.com";

fn domain() -> &'static str {
    DOMAIN
}

fn base() -> &'static str {
    SHOP_BASE
}

/// What the purchases page says once the user closed or ignored the check.
///
/// Never "sign in" or "session": the purchases view reads either as an expired login and drops
/// to signed-out, which would send the user to the wrong fix.
fn message(reason: GiveUp) -> &'static str {
    match reason {
        GiveUp::Dismissed => {
            "The mxbikes-shop.com check was closed before it finished, so your purchases \
             can't load. Try again to reopen it."
        }
        GiveUp::TimedOut => {
            "The mxbikes-shop.com check wasn't completed in time. Try again to reopen it."
        }
        GiveUp::Unavailable => {
            "The app couldn't open a browser window for mxbikes-shop.com's check."
        }
    }
}

fn stopped(reason: GiveUp) -> anyhow::Error {
    anyhow::anyhow!(message(reason))
}

/// The user asked again. Whatever made this session stop showing the check, show it once more.
///
/// Called only on an explicit action — Retry, Refresh, a sign-in, an install — never on the
/// purchases view's own first load, so a closed check never reopens by itself.
pub fn reset() {
    CHECK.reset();
}

/// How long to wait for a download to *start* — that is, for `on_download` to fire. A managed
/// challenge served in place of the file is the failure this catches: the window sits on an
/// interstitial and no download is ever requested.
const DOWNLOAD_START_TIMEOUT: Duration = Duration::from_secs(45);

/// How long a download may then take. Purchased tracks run to hundreds of megabytes.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 30);

/// Same cadence as [`crate::install`]'s streaming download, so the grid's progress bar behaves
/// the same whichever path a file came down.
const PROGRESS_EVERY: Duration = Duration::from_millis(400);

/// Keep the signed-in browser through a request burst, not for the process lifetime.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

static APP: OnceLock<AppHandle> = OnceLock::new();

static ACTIVE: AtomicU64 = AtomicU64::new(0);
static IDLE_GENERATION: AtomicU64 = AtomicU64::new(0);
static LIFECYCLE: Mutex<()> = Mutex::new(());

struct WindowLease {
    app: AppHandle,
}

impl WindowLease {
    fn acquire(app: &AppHandle) -> Self {
        let _guard = lock(&LIFECYCLE);
        ACTIVE.fetch_add(1, Ordering::AcqRel);
        IDLE_GENERATION.fetch_add(1, Ordering::AcqRel);
        Self { app: app.clone() }
    }
}

impl Drop for WindowLease {
    fn drop(&mut self) {
        let _guard = lock(&LIFECYCLE);
        if ACTIVE.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let generation = IDLE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(IDLE_TIMEOUT).await;
            let _guard = lock(&LIFECYCLE);
            if idle_expired(
                ACTIVE.load(Ordering::Acquire),
                IDLE_GENERATION.load(Ordering::Acquire),
                generation,
            ) {
                destroy_window(&app);
            }
        });
    }
}

struct WindowFailureCleanup {
    app: AppHandle,
    armed: bool,
}

impl WindowFailureCleanup {
    fn new(app: &AppHandle) -> Self {
        Self { app: app.clone(), armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for WindowFailureCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _guard = lock(&LIFECYCLE);
            // Do not destroy the shared signed-in browser underneath another request that is
            // still succeeding. The last failed lease performs the cleanup.
            if failure_cleanup_allowed(ACTIVE.load(Ordering::Acquire)) {
                destroy_window(&self.app);
            }
        }
    }
}

fn idle_expired(active: u64, current_generation: u64, scheduled_generation: u64) -> bool {
    active == 0 && current_generation == scheduled_generation
}

fn failure_cleanup_allowed(active: u64) -> bool {
    active == 1
}

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
/// on whatever was fetched when the window opened. It is also what a *fresh sign-in* means: the
/// window that is already up was loaded by whoever was signed in before, and reading it again
/// would answer for the old session — or, when there was none, hand back the login form that
/// EDD serves a signed-out visitor. That is the reload's real job, not just the button's.
pub async fn downloads_html(reload: bool) -> anyhow::Result<String> {
    // `reload` is always the user asking — Retry, Refresh, a sign-in — so it re-arms a check
    // they closed. The view's own first load is not, and fails fast instead.
    if reload {
        reset();
    }
    CHECK.check_asking()?;
    let app = app()?;
    let _lease = WindowLease::acquire(app);
    let mut cleanup = WindowFailureCleanup::new(app);
    // A window that was *just built* has already loaded the purchases page, freshly, as itself:
    // there is no stale DOM for a reload to replace, and re-navigating would only make the user
    // wait through a second load and a second challenge. This is the common case after a
    // sign-in now, since signing in drops the old window.
    let (window, freshly_built) = ensure_window(app).await?;

    if reload && !freshly_built {
        // Which document is being replaced. `navigate` returns immediately and the *old* page
        // stays live — and `readyState === 'complete'` — for as long as the new one takes to
        // arrive, so a readiness check that couldn't tell them apart would pass at once and
        // read the page we just navigated away from. `Err` here (still on a challenge, script
        // not reachable) simply leaves the wait below unconstrained.
        let previous = CHECK.probe(&window).await.origin;
        let url = format!("{SHOP_BASE}{DOWNLOADS_PATH}");
        window.navigate(url.parse()?)?;
        CHECK.clear(app, &window, previous).await?;
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
    cleanup.disarm();
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
    // An install is the user asking, so it re-arms a check they closed earlier.
    reset();
    let _guard = download_lock().lock().await;
    let _lease = WindowLease::acquire(app);
    let mut cleanup = WindowFailureCleanup::new(app);
    let (window, _) = ensure_window(app).await?;

    *lock(download_dir()) = Some(dir.to_path_buf());
    *lock(download_state()) = Download::default();
    // Only while this download runs. Left behind, it sent the next file the user clicked on the
    // shown page into a staging directory that was already gone.
    struct Requested;
    impl Drop for Requested {
        fn drop(&mut self) {
            *lock(download_dir()) = None;
        }
    }
    let _requested = Requested;

    crate::install::emit_progress(app, slug, "downloading", Some(0), None);
    window.navigate(url.parse()?)?;

    let outcome = async {
        let path = wait_for_start(app, &window, slug).await?;
        wait_for_finish(app, slug, &path).await?;
        anyhow::Ok(path)
    }
    .await;

    // Put the window back where it belongs, so the next read finds the purchases page rather
    // than whatever the download navigation left behind. On the way out of a *cancel* this is
    // also the only lever we have on the transfer itself: `DownloadEvent` carries no abort
    // handle, so navigating away is as close to stopping it as the WebView gets — and the
    // caller's `remove_dir_all` over the staging directory has always been best-effort.
    let back = format!("{SHOP_BASE}{DOWNLOADS_PATH}");
    if let Ok(url) = back.parse() {
        let _ = window.navigate(url);
    }

    if outcome.is_ok() {
        cleanup.disarm();
    }
    outcome
}

/// Wait for `on_download` to fire.
///
/// If Cloudflare answers the file URL with a challenge instead of the file, the window renders
/// an interstitial and no download is requested. A challenge still up after
/// [`cf_check::REVEAL_AFTER`] is shown to the user; once they pass it, Cloudflare reloads the
/// URL and the download starts. Anything else that never starts becomes a real error rather
/// than a hang.
async fn wait_for_start(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    slug: &str,
) -> anyhow::Result<PathBuf> {
    let cancel = crate::cancel::token(slug);
    let started = || lock(download_state()).path.clone();
    let mut deadline = Instant::now() + DOWNLOAD_START_TIMEOUT;
    let mut challenged: Option<Instant> = None;
    while Instant::now() < deadline {
        cancel.check()?;
        if let Some(path) = started() {
            return Ok(path);
        }
        if CHECK.probe(window).await.page == cf_check::Page::Challenge {
            let since = *challenged.get_or_insert_with(Instant::now);
            if since.elapsed() >= cf_check::REVEAL_AFTER {
                CHECK
                    .ask_user(app, window, || async {
                        cancel.check()?;
                        // The download starting is the usual end. A real page on the shop is
                        // the other one: the check passed, and the URL answered with a page.
                        Ok(started().is_some() || CHECK.probe(window).await.is_past(None))
                    })
                    .await?;
                // The user's time on the check is not the store's time to start the file.
                challenged = None;
                deadline = Instant::now() + DOWNLOAD_START_TIMEOUT;
            }
        } else {
            challenged = None;
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
    let cancel = crate::cancel::token(slug);
    let deadline = std::time::Instant::now() + DOWNLOAD_TIMEOUT;
    while std::time::Instant::now() < deadline {
        cancel.check()?;
        if let Some(success) = lock(download_state()).outcome {
            if !success {
                anyhow::bail!("the download failed — the store or the connection ended it early");
            }
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if size == 0 {
                anyhow::bail!("the download finished but wrote nothing");
            }
            crate::install::emit_progress_done(app, slug, "downloading", Some(size), None);
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
            let name = destination
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "purchase.zip".to_string());
            let Some(dir) = lock(download_dir()).clone() else {
                // Not one of ours: the user clicked a file on the page while the check window
                // was up. It used to be refused, which looked to them like a dead button.
                let Some(target) = unrequested_target(dirs_next::download_dir(), &name) else {
                    log::warn!("the shop window asked to download {url} with no Downloads folder");
                    return false;
                };
                log::info!("shop download the user started -> {}", target.display());
                *destination = target.clone();
                lock(unrequested()).insert(url.to_string(), target);
                return true;
            };
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
            if let Some(target) = lock(unrequested()).remove(url.as_str()) {
                saved(target, success);
                return true;
            }
            lock(download_state()).outcome = Some(success);
            true
        }
        _ => true,
    }
}

/// Tells the main window a file the user clicked on the shop page has landed. Payload:
/// [`Saved`]. The main window offers to install it, through the same review a drop gets.
pub const SAVED_EVENT: &str = "shop-download-saved";

#[derive(Debug, Clone, serde::Serialize)]
struct Saved {
    path: String,
    name: String,
    success: bool,
}

/// Files the user started on the shown page, by URL, until they finish.
fn unrequested() -> &'static Mutex<std::collections::HashMap<String, PathBuf>> {
    static MAP: OnceLock<Mutex<std::collections::HashMap<String, PathBuf>>> = OnceLock::new();
    MAP.get_or_init(Mutex::default)
}

/// Where a file the user clicked goes: their Downloads folder, under the name the shop gave
/// it, never over a file already there (`Track (1).pkz`, `Track (2).pkz`, …).
fn unrequested_target(downloads: Option<PathBuf>, suggested: &str) -> Option<PathBuf> {
    let dir = downloads?;
    let name = sanitize(suggested);
    let first = dir.join(&name);
    if !first.exists() {
        return Some(first);
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    (1..1000)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
}

fn saved(target: PathBuf, success: bool) {
    let Ok(app) = app() else { return };
    let payload = Saved {
        name: target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: target.to_string_lossy().into_owned(),
        success,
    };
    // The main window only: the shop window runs the remote site.
    if let Err(e) = tauri::Emitter::emit_to(app, crate::MAIN_WINDOW, SAVED_EVENT, payload) {
        log::warn!("could not tell the main window a shop download landed: {e}");
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
///
/// The flag says whether this call is what built it — which is the same question as "is the
/// page it is showing freshly loaded, or left over from before", and so decides whether a
/// caller asking to reload has anything to reload away from.
async fn ensure_window(app: &AppHandle) -> anyhow::Result<(tauri::WebviewWindow, bool)> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    if let Some(existing) = app.get_webview_window(WINDOW) {
        // Whether the window that is up has been seen past the check. Kept by [`cf_check`] so
        // [`close`] can clear it: a stale `true` would let the *next* window be handed out
        // before its own check had cleared.
        if CHECK.is_ready() {
            return Ok((existing, false));
        }
        CHECK.clear(app, &existing, None).await?;
        CHECK.set_ready(true);
        return Ok((existing, false));
    }

    let url: tauri::Url = format!("{SHOP_BASE}{DOWNLOADS_PATH}").parse()?;
    log::info!("opening the hidden mxbikes-shop.com window");
    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title(DOMAIN)
        .on_download(|_webview, event| on_download(event))
        // Never needed, and under Wine the drag-drop handler is what faults in `ole32`.
        .disable_drag_drop_handler()
        // Hidden until a check needs a person — see `cf_check`, which undoes all of the below
        // while it is shown. `visible` is the *window*'s flag; the webview keeps its own, so
        // the page is never treated as backgrounded.
        .visible(false)
        .inner_size(1.0, 1.0)
        .position(-32000.0, -32000.0)
        .skip_taskbar(true)
        .decorations(false)
        .focused(false)
        .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => {
            log::warn!("WebView unavailable for {DOMAIN} ({e}) — the check can't be shown in-app");
            return Err(CHECK.give_up(GiveUp::Unavailable));
        }
    };

    if let Err(error) = CHECK.clear(app, &window, None).await {
        close(app);
        return Err(error);
    }
    CHECK.set_ready(true);
    Ok((window, true))
}

/// Drop the window, so the next use rebuilds it from a fresh navigation.
///
/// Signing *out* has to do this, or a window that is still signed in keeps answering. Signing
/// *in* has to do it just as much, and that is the less obvious half: the window is a browser
/// parked on a page, and the page it is parked on was loaded by whoever was signed in before —
/// or, if the last read failed, is the login form EDD serves a signed-out visitor. Left up, it
/// answers the very next read with that same stale DOM, so a sign-in that genuinely succeeded
/// is reported back as "your session expired" and the app drops straight to signed-out again.
pub fn close(app: &AppHandle) {
    let _guard = lock(&LIFECYCLE);
    IDLE_GENERATION.fetch_add(1, Ordering::AcqRel);
    destroy_window(app);
}

fn destroy_window(app: &AppHandle) {
    // Cleared before the close, not after: the ready flag describes the window that is going
    // away, and a `true` left behind would let the next one be handed out before its check
    // cleared.
    CHECK.set_ready(false);
    if let Some(win) = app.get_webview_window(WINDOW) {
        let _ = win.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_cleanup_only_owns_the_generation_it_scheduled() {
        assert!(idle_expired(0, 11, 11));
        assert!(!idle_expired(1, 11, 11), "an overlapping request keeps the browser alive");
        assert!(!idle_expired(0, 12, 11), "new activity cancels the older timer");
        assert!(IDLE_TIMEOUT <= Duration::from_secs(60));
        assert!(failure_cleanup_allowed(1));
        assert!(!failure_cleanup_allowed(2));
    }

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

    /// RShep clicked his purchases on the shown page and got nothing: with no install waiting,
    /// the download was refused. It lands in Downloads now, without overwriting anything there
    /// and without a name that can leave the folder.
    #[test]
    fn a_file_the_user_clicked_goes_to_downloads() {
        let dir = std::env::temp_dir().join(format!("frost-test-shop-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let first = unrequested_target(Some(dir.clone()), "Track PRO.pkz").unwrap();
        assert_eq!(first, dir.join("Track PRO.pkz"));
        std::fs::write(&first, b"x").unwrap();
        let second = unrequested_target(Some(dir.clone()), "Track PRO.pkz").unwrap();
        assert_eq!(second, dir.join("Track PRO (1).pkz"));
        std::fs::write(&second, b"x").unwrap();
        assert_eq!(
            unrequested_target(Some(dir.clone()), "Track PRO.pkz").unwrap(),
            dir.join("Track PRO (2).pkz")
        );

        let hostile = unrequested_target(Some(dir.clone()), "../../evil.pkz").unwrap();
        assert_eq!(hostile.parent(), Some(dir.as_path()));
        assert_eq!(unrequested_target(None, "Track.pkz"), None);
        let _ = std::fs::remove_dir_all(&dir);
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

    /// A reply is parsed from a remote page, so every field it may omit has to have a default.
    #[test]
    fn a_reply_may_omit_everything_but_its_id() {
        let bare: Reply = serde_json::from_str(r#"{"id":0}"#).unwrap();
        assert_eq!(bare.html, "");

        let read: Reply = serde_json::from_str(r#"{"id":3,"html":"<html></html>"}"#).unwrap();
        assert_eq!(read.html, "<html></html>");
    }

    /// The check is shown the way the mods one is: same size, same title shape, naming this
    /// site and what it is for.
    #[test]
    fn the_shown_check_names_the_shop() {
        assert_eq!(
            CHECK.title(),
            "Verifying access to mxbikes-shop.com — complete the check below to load your purchases"
        );
        assert!(cf_check::same_site(&format!("{SHOP_BASE}/"), (CHECK.base)()));
        assert_eq!(CHECK.window, WINDOW);
        assert_ne!(VERIFY_EVENT, RESULT_EVENT);
    }

    /// Closing the check fails fast with a plain message until the user asks again — and that
    /// message must not read as a lost login, or the purchases view drops to signed-out (it
    /// matches `/sign in|session/i`) and sends the user to the wrong fix.
    #[test]
    fn a_closed_check_says_so_and_waits_for_the_user() {
        reset();
        assert!(CHECK.check_asking().is_ok());
        let err = CHECK.give_up(GiveUp::Dismissed);
        assert!(err.to_string().contains("closed"), "{err}");
        assert!(CHECK.check_asking().is_err(), "a closed check must not reopen by itself");
        reset();
        assert!(CHECK.check_asking().is_ok(), "an explicit retry re-arms it");

        for reason in [GiveUp::Dismissed, GiveUp::TimedOut, GiveUp::Unavailable] {
            let text = message(reason).to_ascii_lowercase();
            assert!(text.contains("mxbikes-shop.com"), "{text}");
            assert!(!text.contains("sign in") && !text.contains("session"), "{text}");
        }
    }

    #[test]
    fn js_strings_escape_what_would_break_out() {
        assert_eq!(js_string(r#"a"b"#), r#""a\"b""#);
        assert!(!js_string("</script>").contains('<'));
    }
}
