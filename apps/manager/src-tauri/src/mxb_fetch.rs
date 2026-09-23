//! Fetching mxb-mods.com from inside a real browser, for the users Cloudflare refuses.
//!
//! [`crate::mxb_session`] exists because Cloudflare sometimes challenges our HTTP client, and
//! it answers that by earning a `cf_clearance` in a WebView and replaying the cookie. A
//! tester's log showed the limit of that approach: the challenge cleared in about a second,
//! the cookie was harvested and sent correctly, and Cloudflare served the interstitial to
//! reqwest anyway. A `cf_clearance` is bound to the TLS/HTTP2 fingerprint of the connection
//! that earned it, and rustls does not fingerprint like the WebView's Chrome — so the cookie
//! is not portable between them, and no amount of cookie work fixes it.
//!
//! What does fix it is not sending the request from Rust at all. This module keeps a hidden
//! WebView parked on the mxb-mods.com origin and runs `fetch()` inside it: same-origin, real
//! browser, real fingerprint, the site's own cookies. Cloudflare cannot tell it from a tab,
//! because it isn't one.
//!
//! When Cloudflare's check will not clear by itself — a managed challenge sometimes wants a
//! click, and a hidden window cannot be clicked — the same window is shown to the user, sized
//! and titled for it, and hidden again once the check is past. The check is always the user's
//! to complete: nothing here answers it. Closing that window ends the attempt for the session
//! (until the user asks again with Retry), so a refusal can never turn into a window that
//! keeps reopening.
//!
//! The window runs in the app's one WebView profile — Tauri puts it under the app's local data
//! folder (`%LOCALAPPDATA%\com.frost.mxbikes` on Windows) — so a `cf_clearance` the user earns
//! survives a restart, and [`inspect_on_startup`] goes straight to this transport while it
//! lasts.
//!
//! Only text comes back this way — the catalog JSON and mod-page HTML. Mod downloads resolve
//! to MediaFire/Drive/Mega and never touch this path, so no large binary is ever marshalled
//! through JavaScript.

use crate::mods::mxb::Fetched;
use crate::mods::Blocked;
use crate::mxb_session;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Listener, LogicalPosition, LogicalSize, Manager, WebviewUrl,
    WebviewWindowBuilder,
};

/// The window the fetches run in. Public because the window-event handler and the IPC guard
/// both have to recognise it — it is the one webview allowed to talk to us from a remote
/// origin, and the one that must never be parked in the tray.
pub const WINDOW: &str = "mxb-fetch";

/// The event the injected script emits its result on. Also public for the guard: this is the
/// only IPC the fetch window is permitted to perform.
pub const RESULT_EVENT: &str = "mxb-fetch:done";

/// How long a single fetch may take. Generous, because the very first one also pays for
/// Cloudflare's challenge clearing in the background.
const TIMEOUT: Duration = Duration::from_secs(45);

/// How long to wait for a page that is simply *loading* — not challenged — to be ready.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// How long Cloudflare's check gets to clear by itself in the hidden window before the user is
/// shown it. A non-interactive challenge passes in a second or two; one still up after this
/// is waiting for a person.
const REVEAL_AFTER: Duration = Duration::from_secs(5);

/// How long the shown check is left up for the user to complete.
const ASSIST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The size of the shown window: enough for the Turnstile widget and Cloudflare's text.
const ASSIST_SIZE: (f64, f64) = (520.0, 640.0);

/// Tells the main window what the check is doing, so it can say so — WebView2 has no way to
/// put our own line of text above a remote page. Payload: [`Verify`].
pub const VERIFY_EVENT: &str = "mods-verify";

/// Why this session has stopped asking, if it has. Read before every WebView request so a
/// dismissed check fails at once instead of rebuilding a window to be refused again.
/// `0` = still asking; otherwise a [`GiveUp`] discriminant. Cleared by [`reset`].
static GAVE_UP: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum GiveUp {
    /// The user closed the check window.
    Dismissed = 1,
    /// The check was shown and left for [`ASSIST_TIMEOUT`] without being completed.
    TimedOut = 2,
    /// The WebView could not be built at all — no WebView2, or a Wine prefix without one.
    Unavailable = 3,
}

impl GiveUp {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Dismissed),
            2 => Some(Self::TimedOut),
            3 => Some(Self::Unavailable),
            _ => None,
        }
    }

    /// What the mod page says. Each one points at the site itself, which the page offers as a
    /// button beside the error.
    fn message(self) -> &'static str {
        match self {
            Self::Dismissed => {
                "The mxb-mods.com check was closed before it finished, so mod pages can't load \
                 in the app right now. Open the mod on mxb-mods.com instead, or hit Retry to be \
                 shown the check again."
            }
            Self::TimedOut => {
                "The mxb-mods.com check wasn't completed in time. Hit Retry to be shown it \
                 again, or open the mod on mxb-mods.com instead."
            }
            Self::Unavailable => {
                "The app couldn't open a browser window for mxb-mods.com's check. Open the mod \
                 on mxb-mods.com instead."
            }
        }
    }
}

fn gave_up() -> Option<GiveUp> {
    GiveUp::from_u8(GAVE_UP.load(Ordering::Acquire))
}

fn give_up(reason: GiveUp) -> anyhow::Error {
    GAVE_UP.store(reason as u8, Ordering::Release);
    stopped(reason)
}

fn stopped(reason: GiveUp) -> anyhow::Error {
    anyhow::Error::new(Blocked::new(None, reason.message()))
}

/// The user asked again (Retry). Whatever made this session stop asking, ask once more.
pub fn reset() {
    if let Some(reason) = GiveUp::from_u8(GAVE_UP.swap(0, Ordering::AcqRel)) {
        log::info!("{} check re-armed by the user after {reason:?}", mxb_session::site().domain);
    }
}

/// [`VERIFY_EVENT`]'s payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Verify {
    /// `shown`, `cleared`, `dismissed` or `timedOut`.
    state: &'static str,
    site: &'static str,
}

fn notify(app: &AppHandle, state: &'static str) {
    let payload = Verify {
        state,
        site: mxb_session::site().domain,
    };
    // To the main window only: the fetch window runs the remote site, which has no business
    // hearing about it.
    if let Err(e) = app.emit_to(crate::MAIN_WINDOW, VERIFY_EVENT, payload) {
        log::debug!("could not tell the main window the check is {state}: {e}");
    }
}

/// The browser is expensive even when hidden. Keep it through a short burst of catalog
/// requests, then release WebView2/WKWebView and recreate it lazily on the next request.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Set once from `setup`. The mods code is a set of free functions with no handle to thread
/// through — `search`, `detail` and `ratings` would all have to grow an `AppHandle`
/// parameter, along with everything between them and the command layer, to avoid this.
static APP: OnceLock<AppHandle> = OnceLock::new();

static READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
            // A concurrent request may still be using the shared browser successfully. Let the
            // last failing lease tear it down; every failed request reaches this check before
            // its lease decrements ACTIVE.
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
        .ok_or_else(|| anyhow::anyhow!("the WebView fetch bridge was never initialised"))
}

/// Requests still waiting on a reply, by id.
fn waiting() -> &'static Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Reply>>> {
    static WAITING: OnceLock<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Reply>>>> =
        OnceLock::new();
    WAITING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the injected script sends back.
///
/// `error` carries a network-level failure — a `fetch` that rejected rather than a request
/// that was answered — so those stay distinguishable from an HTTP error status.
#[derive(Debug, Deserialize)]
struct Reply {
    id: u64,
    #[serde(default)]
    status: u16,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    error: Option<String>,
}

/// Ids for requests in flight. Module-wide rather than per entry point: [`run`] and
/// [`read_page`] share one [`waiting`] map, and two counters would hand out the same id twice.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Route a reply to whoever is waiting for it.
///
/// An id we did not issue is dropped. That is the guard against the remote page inventing
/// results: mxb-mods.com script can emit whatever it likes on this event, and the worst it
/// achieves is a debug line.
fn deliver(reply: Reply) {
    let Some(tx) = lock(waiting()).remove(&reply.id) else {
        log::debug!("ignoring a WebView fetch result for unknown id {}", reply.id);
        return;
    };
    let _ = tx.send(reply);
}

fn listen_for_results(app: &AppHandle) {
    app.listen(RESULT_EVENT, |event| {
        match serde_json::from_str::<Reply>(event.payload()) {
            Ok(reply) => deliver(reply),
            // Never panic on a payload from a remote page.
            Err(e) => log::warn!("unreadable WebView fetch result: {e}"),
        }
    });
}

pub async fn get(url: &str, params: &[(&str, String)]) -> anyhow::Result<Fetched> {
    let full = with_query(url, params);
    run(&full, None).await
}

pub async fn post(url: &str, form: &[(&str, String)]) -> anyhow::Result<Fetched> {
    run(url, Some(&encode_form(form))).await
}

/// A rendered page, read from the window after *navigating* to it.
///
/// [`get`] cannot serve this, and the difference is the whole reason a mod page could be
/// refused while the catalog browsed fine. A `fetch()` of a challenged URL is answered with
/// the interstitial — only a navigation runs the check that clears it — and no `fetch` may
/// claim `sec-fetch-dest: document`, so it never looks like the page view it stands in for.
/// mxb-mods.com guards its rendered pages far more tightly than its JSON API.
///
/// The status comes from the navigation timing entry, which WebView2 exposes and WKWebView
/// does not; where it is missing, a document we were handed at all counts as a 200. That is
/// what keeps a refusal reported as one rather than as a page that parsed to nothing.
pub async fn read_page(url: &str) -> anyhow::Result<Fetched> {
    if let Some(reason) = gave_up() {
        return Err(stopped(reason));
    }
    let app = app()?;
    let _lease = WindowLease::acquire(app);
    let mut cleanup = WindowFailureCleanup::new(app);
    let window = ensure_window(app).await?;

    // Which document is being replaced. `navigate` returns immediately and the old page stays
    // loaded — and past its own check — until the new one arrives, so a readiness test that
    // could not tell them apart would pass at once and read the page we just left.
    let previous = probe(&window).await.origin;
    window.navigate(url.parse()?)?;
    clear(app, &window, previous).await?;

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = tokio::sync::oneshot::channel();
    lock(waiting()).insert(id, tx);

    let started = std::time::Instant::now();
    if let Err(e) = window.eval(read_script(id)) {
        lock(waiting()).remove(&id);
        return Err(anyhow::anyhow!("could not read the mxb-mods.com page: {e}"));
    }

    let reply = match tokio::time::timeout(TIMEOUT, rx).await {
        Ok(Ok(reply)) => reply,
        Ok(Err(_)) => return Err(anyhow::anyhow!("the WebView read was cancelled")),
        Err(_) => {
            lock(waiting()).remove(&id);
            return Err(anyhow::anyhow!(
                "the WebView did not hand back {url} within {TIMEOUT:?}"
            ));
        }
    };

    log::debug!(
        "webview PAGE {} -> {} ({} bytes) in {:?}",
        url.strip_prefix(mxb_session::base()).unwrap_or(url),
        reply.status,
        reply.body.len(),
        started.elapsed()
    );

    let fetched = Fetched {
        status: reply.status,
        // A navigation's response headers are not visible to script, so the refusal log will
        // say `cf-ray=-` for this transport. The body carries the block reason regardless.
        headers: reply.headers,
        body: reply.body,
        url: if reply.url.is_empty() {
            url.to_string()
        } else {
            reply.url
        },
    };
    cleanup.disarm();
    Ok(fetched)
}

/// `application/x-www-form-urlencoded`, matching what `reqwest`'s `.form()` sends, so the
/// two transports look identical to the site.
fn encode_form(form: &[(&str, String)]) -> String {
    form.iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn with_query(url: &str, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return url.to_string();
    }
    let q = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{q}")
}

/// Percent-encoding for the characters that actually turn up in these queries — search
/// terms, slugs and numbers. Deliberately conservative: anything not unreserved is escaped.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn run(url: &str, body: Option<&str>) -> anyhow::Result<Fetched> {
    if let Some(reason) = gave_up() {
        return Err(stopped(reason));
    }
    let app = app()?;
    let _lease = WindowLease::acquire(app);
    let mut cleanup = WindowFailureCleanup::new(app);
    let window = ensure_window(app).await?;

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = tokio::sync::oneshot::channel();
    lock(waiting()).insert(id, tx);

    let started = std::time::Instant::now();
    if let Err(e) = window.eval(script(id, url, body)) {
        lock(waiting()).remove(&id);
        return Err(anyhow::anyhow!("could not run the fetch in the WebView: {e}"));
    }

    let reply = match tokio::time::timeout(TIMEOUT, rx).await {
        Ok(Ok(reply)) => reply,
        // The sender is only dropped if the map is cleared out from under us.
        Ok(Err(_)) => {
            return Err(anyhow::anyhow!("the WebView fetch was cancelled"));
        }
        Err(_) => {
            lock(waiting()).remove(&id);
            return Err(anyhow::anyhow!(
                "the WebView did not answer within {TIMEOUT:?} — {url}"
            ));
        }
    };

    if let Some(error) = reply.error {
        return Err(anyhow::anyhow!("the WebView could not reach {url}: {error}"));
    }

    log::debug!(
        "webview GET {} -> {} in {:?}",
        url.strip_prefix(mxb_session::base()).unwrap_or(url),
        reply.status,
        started.elapsed()
    );

    let fetched = Fetched {
        status: reply.status,
        headers: reply.headers,
        body: reply.body,
        url: if reply.url.is_empty() {
            url.to_string()
        } else {
            reply.url
        },
    };
    cleanup.disarm();
    Ok(fetched)
}

/// The script run inside the page.
///
/// `credentials: "include"` so the site's own cookies ride along, and header names are
/// lowercased to match what [`Fetched`] callers look up. `withGlobalTauri` is off, so the
/// result goes back through the IPC primitive rather than `window.__TAURI__`.
fn script(id: u64, url: &str, body: Option<&str>) -> String {
    let init = match body {
        Some(body) => format!(
            "{{method:'POST',credentials:'include',\
             headers:{{'content-type':'application/x-www-form-urlencoded'}},body:{}}}",
            js_string(body)
        ),
        None => "{credentials:'include'}".to_string(),
    };
    format!(
        r#"(function(){{
  var send = function(p) {{
    p.id = {id};
    // The object itself, not a JSON string of it — Tauri encodes the payload, and
    // stringifying first would land a quoted string where a struct is expected.
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {{
      event: {event}, payload: p
    }});
  }};
  try {{
    fetch({url}, {init}).then(function(r) {{
      return r.text().then(function(t) {{
        var h = {{}};
        r.headers.forEach(function(v, k) {{ h[k.toLowerCase()] = v; }});
        send({{ status: r.status, headers: h, body: t, url: r.url }});
      }});
    }}).catch(function(e) {{ send({{ error: String(e) }}); }});
  }} catch (e) {{ send({{ error: String(e) }}); }}
}})();"#,
        event = js_string(RESULT_EVENT),
        url = js_string(url),
    )
}

/// Read the document the window is sitting on, rather than `fetch`ing it — see [`read_page`].
fn read_script(id: u64) -> String {
    format!(
        r#"(function(){{
  try {{
    var nav = performance.getEntriesByType('navigation')[0];
    window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {{
      event: {event},
      payload: {{ id: {id}, status: (nav && nav.responseStatus) || 200,
                  body: document.documentElement.outerHTML, url: location.href }}
    }});
  }} catch (e) {{}}
}})();"#,
        event = js_string(RESULT_EVENT),
    )
}

/// A JS string literal. The URLs here are ours, but they carry user-typed search terms, and
/// a quote or backslash reaching `eval` unescaped would break the script — or worse.
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
            // `</script` inside a string literal would still end a script block.
            '<' => out.push_str("\\u003C"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Build the hidden window on first use, and have it past Cloudflare's check before returning.
///
/// Serialised, so two requests arriving together don't both try to build it.
async fn ensure_window(app: &AppHandle) -> anyhow::Result<tauri::WebviewWindow> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    if let Some(existing) = app.get_webview_window(WINDOW) {
        if READY.load(Ordering::Relaxed) {
            return Ok(existing);
        }
        clear(app, &existing, None).await?;
        READY.store(true, Ordering::Relaxed);
        return Ok(existing);
    }

    let url: tauri::Url = mxb_session::base().parse()?;
    log::info!("opening the hidden {} fetch window", mxb_session::site().domain);
    let built = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title(mxb_session::site().domain)
        // No `.user_agent()`. This used to claim a pinned Chrome build, and on WebView2 that
        // only replaced the header: the `Sec-CH-UA` client hints kept announcing the Edge that
        // is really installed, so every request contradicted itself — the same inconsistency
        // `hub_clearance` found SiteGround refusing. The WebView introduces itself honestly,
        // and `mxb_session::ua` borrows that for the HTTP client rather than the reverse.
        //
        // Never needed, and under Wine the drag-drop handler is what faults in `ole32` — the
        // reason the main window turns it off there too.
        .disable_drag_drop_handler()
        // Hidden until a check needs a person — see `reveal`. `visible` here is the
        // *window*'s flag; the webview inside keeps its own, which Tauri leaves on, and
        // WebView2 reads page visibility from that one, so nothing backgrounds the page.
        .visible(false)
        // A second line of defence while hidden: if anything ever does show this window
        // un-asked, it has one pixel to do it in, well off-screen. `reveal` undoes all four.
        .inner_size(1.0, 1.0)
        .position(-32000.0, -32000.0)
        .skip_taskbar(true)
        .decorations(false)
        .focused(false)
        .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => {
            log::warn!(
                "WebView unavailable for {} ({e}) — the check can't be shown in-app; mod \
                 pages will offer the site link instead",
                mxb_session::site().domain
            );
            return Err(give_up(GiveUp::Unavailable));
        }
    };

    if let Err(error) = clear(app, &window, None).await {
        destroy_window(app);
        return Err(error);
    }
    READY.store(true, Ordering::Relaxed);
    Ok(window)
}

fn destroy_window(app: &AppHandle) {
    READY.store(false, Ordering::Release);
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.destroy();
    }
}

/// Wait until the window shows a page past Cloudflare's check — and, when `replacing` is set,
/// not the page a navigation was just asked to leave.
///
/// `replacing` is that document's `performance.timeOrigin`. Without it the check answers
/// about whatever is still on screen, which straight after a `navigate` is the previous page:
/// complete, past its own check, and wrong.
///
/// Hidden first. A page that is merely loading gets [`READY_TIMEOUT`]; one sitting on the
/// check gets [`REVEAL_AFTER`] to clear by itself, and is then shown to the user.
async fn clear(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    replacing: Option<f64>,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut challenged: Option<Instant> = None;
    let mut last = "no answer from the page yet";
    loop {
        let probe = probe(window).await;
        if probe.is_past(replacing) {
            if let Some(since) = challenged {
                log::info!(
                    "{} check cleared by itself in the hidden window after {:?}",
                    mxb_session::site().domain,
                    since.elapsed()
                );
            }
            return Ok(());
        }
        match probe.page {
            Page::Challenge => {
                let since = *challenged.get_or_insert_with(Instant::now);
                if since.elapsed() >= REVEAL_AFTER {
                    return ask_user(app, window, replacing).await;
                }
            }
            Page::Ready => last = "the page it was told to leave is still loaded",
            Page::Loading => last = "still loading",
        }
        if challenged.is_none() && started.elapsed() >= READY_TIMEOUT {
            return Err(anyhow::anyhow!(
                "the hidden {} window didn't finish loading in {READY_TIMEOUT:?} ({last})",
                mxb_session::site().domain
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Show the check to the user, and wait for them to complete it or close it.
///
/// One at a time: a second request arriving mid-check waits here, and finds the answer the
/// first one got instead of opening anything.
async fn ask_user(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    replacing: Option<f64>,
) -> anyhow::Result<()> {
    static ASKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = ASKING.lock().await;
    if let Some(reason) = gave_up() {
        return Err(stopped(reason));
    }
    if probe(window).await.is_past(replacing) {
        return Ok(());
    }

    let domain = mxb_session::site().domain;
    log::info!(
        "{domain} check did not clear by itself within {REVEAL_AFTER:?} — showing it to the user"
    );
    reveal(window);
    notify(app, "shown");

    let shown = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Closed by the user. The window is transient (never parked in the tray), so a close
        // is a destroy and the label is gone.
        if app.get_webview_window(WINDOW).is_none() {
            READY.store(false, Ordering::Release);
            log::info!(
                "user closed the {domain} check after {:?} — not asking again this session \
                 unless they hit Retry",
                shown.elapsed()
            );
            notify(app, "dismissed");
            return Err(give_up(GiveUp::Dismissed));
        }
        if probe(window).await.is_past(replacing) {
            log::info!("{domain} check cleared by the user after {:?}", shown.elapsed());
            conceal(window);
            notify(app, "cleared");
            return Ok(());
        }
        if shown.elapsed() >= ASSIST_TIMEOUT {
            log::info!(
                "{domain} check was shown for {ASSIST_TIMEOUT:?} and not completed — timed out; \
                 not asking again this session unless the user hits Retry"
            );
            conceal(window);
            notify(app, "timedOut");
            return Err(give_up(GiveUp::TimedOut));
        }
    }
}

/// Undo everything that keeps the window out of sight, and put it in front of the user.
fn reveal(window: &tauri::WebviewWindow) {
    let title = format!(
        "Verifying access to {} — complete the check below to load mod pages",
        mxb_session::site().domain
    );
    // Each step on its own: a window manager that refuses one (Wine's is the likeliest) should
    // not stop the rest from putting the window where it can be seen.
    let steps: [(&str, tauri::Result<()>); 7] = [
        ("title", window.set_title(&title)),
        ("decorations", window.set_decorations(true)),
        ("taskbar", window.set_skip_taskbar(false)),
        ("size", window.set_size(LogicalSize::new(ASSIST_SIZE.0, ASSIST_SIZE.1))),
        ("center", window.center()),
        ("show", window.show()),
        ("focus", window.set_focus()),
    ];
    for (step, result) in steps {
        if let Err(e) = result {
            log::warn!("showing the check window: {step} failed: {e}");
        }
    }
}

/// Back out of sight, the way [`ensure_window`] built it, for the requests still to come.
fn conceal(window: &tauri::WebviewWindow) {
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_decorations(false);
    let _ = window.set_size(LogicalSize::new(1.0, 1.0));
    let _ = window.set_position(LogicalPosition::new(-32000.0, -32000.0));
    let _ = window.set_title(mxb_session::site().domain);
}

/// What the page in the window is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    /// Loaded, script and IPC available, and not Cloudflare's check.
    Ready,
    /// Sitting on Cloudflare's check.
    Challenge,
    /// Anything else: mid-navigation, not answering, or a page without our IPC.
    Loading,
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    page: Page,
    /// The document's `performance.timeOrigin`, minted fresh per document — how a caller that
    /// has just navigated tells the page it asked for from the one it asked to leave.
    origin: Option<f64>,
}

impl Probe {
    fn is_past(&self, replacing: Option<f64>) -> bool {
        self.page == Page::Ready && (replacing.is_none() || self.origin != replacing)
    }
}

#[derive(Deserialize)]
struct ProbeReply {
    ipc: bool,
    complete: bool,
    challenge: bool,
    origin: f64,
}

/// Read the page's state straight out of the WebView.
///
/// Asked with `eval_with_callback` rather than over our IPC, because the question that matters
/// most — *is this the check?* — has to be answerable on the check itself. Our IPC's presence is
/// still one of the answers: the fetches depend on it, and if a Tauri upgrade ever renames
/// `__TAURI_INTERNALS__` this reports "loading" for good rather than declaring the page ready.
async fn probe(window: &tauri::WebviewWindow) -> Probe {
    let reply = eval_json::<ProbeReply>(window, &probe_script(), Duration::from_secs(1)).await;
    classify(reply)
}

fn classify(reply: Option<ProbeReply>) -> Probe {
    let Some(r) = reply else {
        return Probe {
            page: Page::Loading,
            origin: None,
        };
    };
    let page = if r.challenge {
        Page::Challenge
    } else if r.complete && r.ipc {
        Page::Ready
    } else {
        Page::Loading
    };
    Probe {
        page,
        origin: Some(r.origin),
    }
}

/// The probe, as its own function so a test can hold it to what it has to say.
///
/// "On the check" is asked as `window._cf_chl_opt`, which the interstitial defines and an
/// ordinary page does not, or a "Just a moment…" title. The obvious test — looking for
/// `challenge-platform` in the HTML — is wrong, and quietly so: Cloudflare injects that script
/// into ordinary pages too when JS detections are on, so the window would never be ready.
fn probe_script() -> String {
    "(function(){ try { return { \
       ipc: !!window.__TAURI_INTERNALS__, \
       complete: document.readyState === 'complete', \
       challenge: typeof window._cf_chl_opt !== 'undefined' \
         || /^just a moment/i.test(document.title || ''), \
       origin: performance.timeOrigin }; } catch (e) { return null; } })()"
        .to_string()
}

/// Evaluate `js` in `window` and deserialize what it returns. `None` for anything that did not
/// come back as the expected shape in time — a page mid-navigation answers `null`, or nothing.
async fn eval_json<T: serde::de::DeserializeOwned + Send + 'static>(
    window: &tauri::WebviewWindow,
    js: &str,
    within: Duration,
) -> Option<T> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    window
        .eval_with_callback(js, move |json| {
            if let Some(tx) = lock(&tx).take() {
                let _ = tx.send(json);
            }
        })
        .ok()?;
    let json = tokio::time::timeout(within, rx).await.ok()?.ok()?;
    serde_json::from_str(&json).ok()
}

/// Once at startup: read the WebView's real user agent, and whether an earlier session left a
/// `cf_clearance` in the profile.
///
/// The UA is logged and handed to [`mxb_session::ua`]. The clearance is logged by *presence
/// and expiry only* — its value is a bearer token, and logs get pasted into Discord — and if
/// one is still valid this session starts on the WebView transport rather than walking into
/// the same refusal first.
pub fn inspect_on_startup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) else {
            log::warn!("no main window to read the WebView's user-agent from");
            return;
        };

        let mut ua = None;
        for _ in 0..40 {
            ua = eval_json::<String>(&main, "navigator.userAgent", Duration::from_secs(1)).await;
            if ua.as_deref().is_some_and(|ua| !ua.is_empty()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        match ua {
            Some(ua) if mxb_session::set_webview_ua(&ua) => {
                log::info!("webview user-agent: {ua} (the mods HTTP client uses it too)")
            }
            Some(ua) => log::info!(
                "webview user-agent: {ua} (not Chromium — the mods HTTP client keeps {})",
                mxb_session::FALLBACK_UA
            ),
            None => log::warn!(
                "couldn't read the WebView's user-agent — the mods HTTP client uses {}",
                mxb_session::FALLBACK_UA
            ),
        }

        let domain = mxb_session::site().domain;
        if let Ok(dir) = app.path().app_local_data_dir() {
            log::info!("webview profile: {}", dir.display());
        }
        let Ok(url) = mxb_session::base().parse::<tauri::Url>() else {
            return;
        };
        // Off the async workers: on Windows this waits on the UI thread.
        let read = tauri::async_runtime::spawn_blocking(move || main.cookies_for_url(url)).await;
        let cookies = match read {
            Ok(Ok(cookies)) => cookies,
            Ok(Err(e)) => {
                log::info!("couldn't read {domain} cookies from the WebView: {e}");
                return;
            }
            Err(e) => {
                log::info!("couldn't read {domain} cookies from the WebView: {e}");
                return;
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let found = clearance(
            cookies
                .iter()
                .map(|c| (c.name(), c.expires_datetime().map(|t| t.unix_timestamp()))),
            now,
        );
        match found {
            Clearance::Missing => log::info!("{domain} cf_clearance: none in the WebView profile"),
            Clearance::Expired => {
                log::info!("{domain} cf_clearance: present but expired")
            }
            Clearance::Valid { expires_in } => {
                log::info!(
                    "{domain} cf_clearance: present, {}",
                    expires_in.map_or_else(
                        || "no expiry".to_string(),
                        |s| format!("expires in {}h{:02}m", s / 3600, (s % 3600) / 60)
                    )
                );
                crate::mods::mxb::use_webview("a cf_clearance from an earlier session is still valid");
            }
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
enum Clearance {
    Missing,
    Expired,
    /// Seconds left, when the cookie says.
    Valid { expires_in: Option<u64> },
}

/// Whether `cookies` — `(name, expiry as unix seconds)` — hold a live `cf_clearance`.
fn clearance<'a>(cookies: impl Iterator<Item = (&'a str, Option<i64>)>, now: i64) -> Clearance {
    let mut found = Clearance::Missing;
    for (name, expires) in cookies {
        if name != "cf_clearance" {
            continue;
        }
        match expires {
            None => return Clearance::Valid { expires_in: None },
            Some(at) if at > now => {
                return Clearance::Valid {
                    expires_in: Some((at - now) as u64),
                }
            }
            Some(_) => found = Clearance::Expired,
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_cleanup_only_owns_the_generation_it_scheduled() {
        assert!(idle_expired(0, 7, 7));
        assert!(!idle_expired(1, 7, 7), "an overlapping request keeps the browser alive");
        assert!(!idle_expired(0, 8, 7), "new activity cancels the older timer");
        assert!(IDLE_TIMEOUT <= Duration::from_secs(60));
        assert!(failure_cleanup_allowed(1));
        assert!(!failure_cleanup_allowed(2));
    }

    /// A search term with a quote in it reaches `eval` — it must not be able to close the
    /// string literal it sits in.
    #[test]
    fn js_strings_escape_what_would_break_out() {
        assert_eq!(js_string(r#"a"b"#), r#""a\"b""#);
        assert_eq!(js_string(r"a\b"), r#""a\\b""#);
        assert_eq!(js_string("a\nb"), r#""a\nb""#);
        // A closing script tag inside a literal still ends a script block in HTML parsing.
        assert!(!js_string("</script>").contains('<'));
    }

    #[test]
    fn the_script_carries_the_id_url_and_event() {
        let js = script(42, "https://mxb-mods.com/wp-json/wp/v2/posts?page=2", None);
        assert!(js.contains("p.id = 42;"), "{js}");
        assert!(js.contains("wp-json/wp/v2/posts?page=2"), "{js}");
        assert!(js.contains(RESULT_EVENT), "{js}");
        assert!(js.contains("credentials:'include'"), "{js}");
        assert!(!js.contains("method:'POST'"), "a GET must not claim to be a POST");
        // Tauri encodes the payload itself. Stringifying first lands a quoted string where
        // the listener expects a struct, and every reply is dropped as unreadable — which
        // is exactly what the first run of this bridge did.
        assert!(
            !js.contains("JSON.stringify"),
            "the payload must be the object, not a JSON string of it: {js}"
        );
    }

    /// The bug this module shipped with: the probe answered on Cloudflare's interstitial,
    /// because Tauri injects its IPC there too. Every question below has to be asked, or the
    /// window is declared ready while still on "Just a moment…" and every request is refused.
    #[test]
    fn the_probe_asks_every_question() {
        let js = probe_script();
        assert!(js.contains("_cf_chl_opt"), "{js}");
        assert!(js.contains("just a moment"), "{js}");
        assert!(js.contains("readyState === 'complete'"), "{js}");
        assert!(js.contains("__TAURI_INTERNALS__"), "{js}");
        // The naive test: Cloudflare injects `challenge-platform` into ordinary pages as
        // well, so keying on it would mean the window is never ready at all.
        assert!(!js.contains("challenge-platform"), "{js}");
        // Without a per-document token a caller that just navigated cannot tell the page it
        // asked for from the one it asked to leave.
        assert!(js.contains("performance.timeOrigin"), "{js}");
    }

    fn reply(ipc: bool, complete: bool, challenge: bool, origin: f64) -> Option<ProbeReply> {
        Some(ProbeReply {
            ipc,
            complete,
            challenge,
            origin,
        })
    }

    /// The challenge wins over everything else — Tauri's IPC is injected into it and it
    /// finishes loading like any page — and nothing that failed to answer is ever ready.
    #[test]
    fn a_challenge_is_never_ready() {
        assert_eq!(classify(reply(true, true, true, 1.0)).page, Page::Challenge);
        assert_eq!(classify(reply(true, false, true, 1.0)).page, Page::Challenge);
        assert_eq!(classify(reply(true, true, false, 1.0)).page, Page::Ready);
        assert_eq!(classify(reply(true, false, false, 1.0)).page, Page::Loading);
        assert_eq!(classify(reply(false, true, false, 1.0)).page, Page::Loading);
        assert_eq!(classify(None).page, Page::Loading);
    }

    /// Straight after a navigation the old page is still loaded and past its check. It must
    /// not count as the new one.
    #[test]
    fn the_page_being_replaced_does_not_count() {
        let old = classify(reply(true, true, false, 100.0));
        assert!(old.is_past(None));
        assert!(!old.is_past(Some(100.0)));
        assert!(classify(reply(true, true, false, 200.0)).is_past(Some(100.0)));
        assert!(!classify(reply(true, true, true, 200.0)).is_past(Some(100.0)));
    }

    /// A dismissed check stops the session asking, and Retry is what starts it again. The
    /// message is a `Blocked`, so the mod page renders it as a refusal with its site link.
    #[test]
    fn a_dismissed_check_fails_fast_until_reset() {
        reset();
        assert_eq!(gave_up(), None);
        let err = give_up(GiveUp::Dismissed);
        assert_eq!(gave_up(), Some(GiveUp::Dismissed));
        assert!(err.downcast_ref::<Blocked>().is_some());
        assert!(err.to_string().contains("Retry"), "{err}");
        reset();
        assert_eq!(gave_up(), None);
        for reason in [GiveUp::Dismissed, GiveUp::TimedOut, GiveUp::Unavailable] {
            assert_eq!(GiveUp::from_u8(reason as u8), Some(reason));
            assert!(reason.message().contains("mxb-mods.com"));
        }
    }

    /// Presence and expiry are all that is read — and an expired clearance is not a reason to
    /// start on the WebView.
    #[test]
    fn a_clearance_is_judged_by_name_and_expiry() {
        let now = 1_000_000;
        assert_eq!(clearance([("__cf_bm", Some(now + 60))].into_iter(), now), Clearance::Missing);
        assert_eq!(
            clearance([("cf_clearance", Some(now - 1))].into_iter(), now),
            Clearance::Expired
        );
        assert_eq!(
            clearance([("cf_clearance", Some(now + 3600))].into_iter(), now),
            Clearance::Valid {
                expires_in: Some(3600)
            }
        );
        assert_eq!(
            clearance([("cf_clearance", None)].into_iter(), now),
            Clearance::Valid { expires_in: None }
        );
    }

    /// A page is read off the document, never fetched — a `fetch()` of a challenged URL is
    /// answered with the interstitial, and cannot claim `sec-fetch-dest: document` either.
    #[test]
    fn a_page_is_read_from_the_document_not_fetched() {
        let js = read_script(11);
        assert!(js.contains("document.documentElement.outerHTML"), "{js}");
        assert!(!js.contains("fetch("), "{js}");
        assert!(js.contains("id: 11"), "{js}");
        // Where the engine exposes the navigation's status, a refusal stays a refusal rather
        // than becoming a page that parsed to nothing.
        assert!(js.contains("responseStatus"), "{js}");
    }

    /// `run` and `read_page` share one `waiting()` map, so they must share one counter — two
    /// would issue the same id twice and let one reply resolve the other's request.
    #[test]
    fn request_ids_are_issued_from_one_counter() {
        let a = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let b = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(a, b);
        assert!(a > 0 && b > 0);
    }

    #[test]
    fn a_post_carries_its_form_body() {
        let js = script(7, "https://mxb-mods.com/wp-admin/admin-ajax.php", Some("a=1&b=2"));
        assert!(js.contains("method:'POST'"), "{js}");
        assert!(js.contains("a=1&b=2"), "{js}");
        assert!(js.contains("x-www-form-urlencoded"), "{js}");
    }

    #[test]
    fn queries_are_encoded_the_way_the_site_expects() {
        let params = [("search", "supercross 26".to_string()), ("page", "2".to_string())];
        let url = with_query("https://mxb-mods.com/wp-json/wp/v2/posts", &params);
        assert_eq!(
            url,
            "https://mxb-mods.com/wp-json/wp/v2/posts?search=supercross%2026&page=2"
        );
        // An existing query string is appended to, not replaced.
        assert!(with_query("https://x/y?a=1", &params).contains("?a=1&search="));
        assert_eq!(with_query("https://x/y", &[]), "https://x/y");
    }

    #[test]
    fn form_encoding_matches_reqwests() {
        let form = [
            ("action", "load_results".to_string()),
            ("postID", "1234".to_string()),
        ];
        assert_eq!(encode_form(&form), "action=load_results&postID=1234");
    }

    /// The remote page can emit anything it likes on this event. A result for an id we never
    /// issued has to be dropped, not delivered to whoever happens to be waiting.
    #[test]
    fn a_reply_for_an_unknown_id_is_dropped() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        lock(waiting()).insert(9001, tx);

        deliver(Reply {
            id: 9002,
            status: 200,
            headers: HashMap::new(),
            body: "injected".into(),
            url: String::new(),
            error: None,
        });
        assert!(
            rx.try_recv().is_err(),
            "a reply for another id must not resolve this request"
        );

        deliver(Reply {
            id: 9001,
            status: 200,
            headers: HashMap::new(),
            body: "ours".into(),
            url: String::new(),
            error: None,
        });
        assert_eq!(rx.try_recv().unwrap().body, "ours");
    }

    /// Malformed JSON from the page must not panic the listener.
    #[test]
    fn an_unreadable_payload_is_survivable() {
        assert!(serde_json::from_str::<Reply>("not json").is_err());
        // Missing optional fields are fine; only the id is required.
        let r: Reply = serde_json::from_str(r#"{"id":3}"#).unwrap();
        assert_eq!(r.id, 3);
        assert_eq!(r.status, 0);
    }
}
