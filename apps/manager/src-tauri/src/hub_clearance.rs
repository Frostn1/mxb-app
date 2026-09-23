//! Getting past MXB Hub's robot challenge, by running it in a real browser.
//!
//! `shop.mxb-hub.com` is on SiteGround, whose bot protection fires on **request rate** rather
//! than on anything about the client. That is why it is invisible to a handful of probes and
//! entirely reproducible the moment a grid asks for a page of twenty-four thumbnails: the
//! store starts answering every path — API and images alike — with a `202` carrying a "Robot
//! Challenge Screen", and keeps doing so until something solves it. Solving it means computing
//! a proof of work in a Web Worker and posting it back, so an HTTP client cannot; a browser
//! can, and gets a cookie for its trouble.
//!
//! So: park a window on the store, let it do that, and hand its cookies to the clients.
//!
//! Hidden first, and shown only if that fails. A headless window is the right default — the
//! user has no idea there was a question — but it is not guaranteed: the proof of work runs in
//! a Web Worker, and a window with no area and no visibility is exactly what a host throttles.
//! When the hidden pass comes back having earned nothing, the same page is opened where the
//! person can see it and finish the check themselves, which is the one thing that always
//! worked.
//!
//! Deliberately much smaller than [`crate::mxb_fetch`] and [`crate::shop_fetch`], which solve
//! the same shape of problem for the two Cloudflare-fronted sites. Those two have to *read
//! pages* out of the browser, so they inject a script, take a result back over IPC, and pay
//! for that with a capability file granting a remote origin the right to talk to us. Nothing
//! here does: the only thing taken from this window is its cookie jar, which Rust reads from
//! the outside. The page is never given a way to call the app — shown or hidden, both passes
//! are built the same way in that respect — so there is no IPC surface to get wrong.

use crate::hub_session::{HUB_BASE, HUB_SITE};
use crate::{cookie_session, mods};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserResponse {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
enum BrowserResult {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "done")]
    Done {
        status: u16,
        headers: std::collections::HashMap<String, String>,
        body: String,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

/// The window's label. Transient — it must be destroyed on close, never parked in the tray,
/// or its label stays registered and the next handshake silently cannot build one.
pub const WINDOW: &str = "hub-clearance";

/// Open the URL that actually triggered the challenge. SiteGround redirects back here after
/// solving its proof of work, which also makes the verification useful even when protection
/// is enabled for the API but not the storefront homepage.
fn challenge_url() -> String {
    format!("{HUB_BASE}/wp-json/wc/store/v1/products?per_page=1")
}

/// How long the hidden window gets to run the challenge by itself. The proof of work is a
/// second or two on a modern machine; the rest is page load, and being generous costs nothing
/// when it succeeds.
const SOLVE_TIMEOUT: Duration = Duration::from_secs(40);

/// How long the *visible* window then gets, once the user has been asked to finish the check.
/// The sign-in window's budget, for the same reason: a person needs time a script does not.
const ASSIST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// A single probe's ceiling.
///
/// Bounded here and not only by the client, because the solve loop tests its deadline between
/// probes: one request left to run long overruns the whole budget, however short the budget.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

const POLL: Duration = Duration::from_millis(500);
const MAX_BROWSER_BODY: usize = 8 * 1024 * 1024;

/// A refused browser fetch is not permission to start the same request on every poll. Give the
/// challenge page time to make progress, and cap the entire pass even when it never does.
const BROWSER_RETRY_BACKOFF: Duration = Duration::from_secs(10);
const MAX_BROWSER_ATTEMPTS: u32 = 32;

/// How long to let the page settle before asking the store anything. Probing instantly only
/// spends a request confirming what we already know — we are here because we were refused.
const FIRST_PROBE: Duration = Duration::from_secs(3);

/// And how often after that. Deliberately unhurried: this runs while a page is refusing us,
/// and hammering the thing that rate-limited us is how we got here.
const PROBE_EVERY: Duration = Duration::from_secs(4);

/// Unix seconds of the last successful handshake, or 0.
///
/// Not a "we are cleared" flag: the clearance can lapse at any time and only the store knows.
/// It is a throttle. Without it, a page whose twenty-four thumbnails are all being refused
/// would queue twenty-four handshakes, which is both pointless and precisely the request storm
/// that got us challenged.
static LAST: AtomicU64 = AtomicU64::new(0);

/// Don't hand out a second handshake within this of the last one succeeding.
const REUSE_WITHIN: u64 = 20;

/// Unix seconds of the last handshake that came back empty-handed, or 0.
///
/// [`LAST`] throttles the happy path; this throttles the unhappy one, and it is the one that
/// was missing. Opening the Hub asks for a search, two category reads and the purchases at
/// once — all four are refused together, all four call in here, and the lock serialises them.
/// With only a success remembered, each in turn paid the full solve budget over again: one
/// refusal became four windows and four times the wait, which is what the logs of a challenged
/// session are full of. A failure this recent means the store is still refusing *now*, and the
/// three callers queued behind the first have nothing to add by asking again.
static LAST_FAILURE: AtomicU64 = AtomicU64::new(0);

/// How long a failure stands before it is worth opening another window.
///
/// Short, because it is there to collapse one burst rather than to stop anybody retrying: the
/// four callers arrive within the same second, and a person who reaches for Retry has by then
/// watched a window come and go.
const RETRY_AFTER_FAILURE: u64 = 10;

/// What the caller is told when the check is still unanswered. One wording for both ways of
/// getting here, so the view reads the same whether this attempt opened a window or stood on a
/// refusal a moment old.
const STILL_REFUSED: &str =
    "MXB Hub is still asking the app to prove it isn't a robot. It opens a window for you to \
     finish that check — complete it there, then try again.";

/// A WebView's user agent is part of the challenge identity, but only macOS needs an override.
///
/// WKWebView's default string does not travel with the HTTP client, so it uses the matching
/// macOS identity from [`HUB_SITE`]. WebView2 must remain untouched: overriding only its
/// `User-Agent` to claim Chrome leaves its Edge client-hint headers intact, which SiteGround
/// rejects as an inconsistent browser and returns 403 after the visible fallback opens.
fn browser_user_agent() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        Some(HUB_SITE.ua)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Answer the challenge and give the result to both hub clients.
///
/// Two passes. The first is the one nobody sees: a hidden window runs the proof of work and the
/// user is never told there was a question. When that comes back empty-handed the check is
/// handed to the person sitting there — the same page, in a window they can see and click,
/// which is what a browser was always able to do and a headless one sometimes cannot.
///
/// One at a time, and cheap to call again: a caller that lost the race for the lock finds the
/// work already done — or just failed — and returns without opening anything.
pub async fn earn(app: &AppHandle) -> anyhow::Result<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;
    let _cleanup = WindowCleanup(app.clone());

    if now().saturating_sub(LAST.load(Ordering::Relaxed)) < REUSE_WITHIN {
        log::info!("an MXB Hub clearance was just earned; reusing it");
        return Ok(());
    }
    // The other three callers of a refused page load arrive here while the first one's windows
    // are still closing. They would each reopen the lot to be told the same thing.
    if now().saturating_sub(LAST_FAILURE.load(Ordering::Relaxed)) < RETRY_AFTER_FAILURE {
        log::info!("an MXB Hub handshake just failed; not opening another window for this one");
        anyhow::bail!(STILL_REFUSED);
    }

    let outcome = async {
        // A window left over from a previous attempt is on a page that has already been decided
        // one way or the other, so it is dropped rather than reused: the point is a fresh
        // navigation, which is what re-serves the challenge and lets the browser answer it.
        close_and_settle(app).await;
        if attempt(app, Mode::Hidden).await? {
            return anyhow::Ok(true);
        }
        log::info!("the hidden window could not answer the MXB Hub check — asking the user to");
        close_and_settle(app).await;
        attempt(app, Mode::Visible).await
    }
    .await;

    match outcome {
        Ok(true) => Ok(()),
        Ok(false) => {
            LAST_FAILURE.store(now(), Ordering::Relaxed);
            anyhow::bail!(STILL_REFUSED)
        }
        // A window that could not even be built is still a failure to throttle: without this,
        // whatever stopped it would be retried by every caller in the burst.
        Err(e) => {
            LAST_FAILURE.store(now(), Ordering::Relaxed);
            Err(e)
        }
    }
}

/// Read a protected Hub URL in the browser identity that answered the challenge.
///
/// This is the Windows fallback. WebView2 must keep its native Edge identity (overriding only
/// its UA leaves Edge client hints behind), and consequently its clearance must not be replayed
/// by the Chrome-shaped reqwest client. The request stays in WebView2 and its result comes back
/// through the native execute-script callback. The remote page receives no Tauri IPC capability.
pub async fn get(app: &AppHandle, url: &str) -> anyhow::Result<BrowserResponse> {
    if !browser_target_allowed(url) {
        anyhow::bail!("refusing to read a non-Hub URL in the Hub browser");
    }
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;
    let _cleanup = WindowCleanup(app.clone());

    let outcome = async {
        close_and_settle(app).await;
        if let Some(response) = browser_attempt(app, url, Mode::Hidden).await? {
            return Ok(response);
        }
        close_and_settle(app).await;
        browser_attempt(app, url, Mode::Visible)
            .await?
            .ok_or_else(|| anyhow::anyhow!(STILL_REFUSED))
    }
    .await;
    close_and_settle(app).await;
    outcome
}

fn browser_target_allowed(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some(HUB_SITE.domain)
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
    })
}

struct WindowCleanup(AppHandle);

impl Drop for WindowCleanup {
    fn drop(&mut self) {
        close(&self.0);
    }
}

async fn browser_attempt(
    app: &AppHandle,
    target: &str,
    mode: Mode,
) -> anyhow::Result<Option<BrowserResponse>> {
    let url: tauri::Url = challenge_url().parse()?;
    let mut builder = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title(if mode.visible() {
            "MXB Hub — please finish the robot check"
        } else {
            "MXB Hub"
        })
        .visible(mode.visible())
        .decorations(mode.visible())
        .focused(mode.visible())
        .skip_taskbar(!mode.visible());
    builder = if mode.visible() {
        builder.inner_size(520.0, 760.0).center()
    } else {
        builder.inner_size(1024.0, 768.0).position(-32000.0, -32000.0)
    };
    let window = builder.build()?;
    let _cleanup = WindowCleanup(app.clone());

    let target = serde_json::to_string(target)?;
    let script = browser_request_script(&target);

    let deadline = std::time::Instant::now() + mode.budget();
    tokio::time::sleep(FIRST_PROBE).await;
    while std::time::Instant::now() < deadline {
        if mode.visible() && app.get_webview_window(WINDOW).is_none() {
            return Ok(None);
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        window.eval_with_callback(script.clone(), move |value| {
            if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = tx.send(value);
            }
        })?;
        let raw = match tokio::time::timeout(PROBE_TIMEOUT, rx).await {
            Ok(Ok(value)) => value,
            _ => {
                tokio::time::sleep(POLL).await;
                continue;
            }
        };
        let inner = serde_json::from_str::<String>(&raw).unwrap_or(raw);
        match serde_json::from_str::<BrowserResult>(&inner) {
            Ok(BrowserResult::Done {
                status,
                headers,
                body,
            }) => {
                if !browser_response_challenged(status, &headers) {
                    return Ok(Some(BrowserResponse {
                        status,
                        headers,
                        body,
                    }));
                }
                // The answer is terminal for this pass. Replace it with a tiny pending marker
                // so the 500 ms readiness poll does not keep serializing and parsing the same
                // (potentially multi-megabyte) challenge body for five minutes.
                let _ = window.eval(&format!(
                    "if (window.__mxbAppHubFetch) window.__mxbAppHubFetch = {{ state: 'error', target: window.__mxbAppHubFetch.target, attempts: window.__mxbAppHubFetch.attempts, message: 'challenge not finished', retryAt: Date.now() + {} }};",
                    browser_retry_backoff_ms()
                ));
            }
            Ok(BrowserResult::Error { message }) => {
                log::debug!("MXB Hub browser request is not ready: {message}");
            }
            Ok(BrowserResult::Pending) | Err(_) => {}
        }
        tokio::time::sleep(POLL).await;
    }
    Ok(None)
}

fn browser_request_script(target: &str) -> String {
    format!(
        r#"(() => {{
          const target = {target};
          const key = "__mxbAppHubFetch";
          const attempts = Number(window[key]?.attempts || 0);
          if (!window[key] || window[key].target !== target ||
              ((window[key].state === "done" || window[key].state === "error") &&
               Number(window[key].retryAt || 0) <= Date.now() &&
               attempts < {MAX_BROWSER_ATTEMPTS})) {{
            window[key] = {{ state: "pending", target, attempts: attempts + 1 }};
            (async () => {{
              try {{
                const response = await fetch(target, {{ credentials: "include", cache: "no-store" }});
                const declared = Number(response.headers.get("content-length") || "0");
                if (declared > {MAX_BROWSER_BODY}) throw new Error("response body is too large");
                const reader = response.body && response.body.getReader();
                const decoder = new TextDecoder();
                let body = "";
                let bytes = 0;
                if (reader) {{
                  for (;;) {{
                    const part = await reader.read();
                    if (part.done) break;
                    bytes += part.value.byteLength;
                    if (bytes > {MAX_BROWSER_BODY}) {{
                      await reader.cancel();
                      throw new Error("response body is too large");
                    }}
                    body += decoder.decode(part.value, {{ stream: true }});
                  }}
                  body += decoder.decode();
                }} else {{
                  body = await response.text();
                  if (new TextEncoder().encode(body).byteLength > {MAX_BROWSER_BODY})
                    throw new Error("response body is too large");
                }}
                const headers = {{}};
                response.headers.forEach((value, name) => headers[name.toLowerCase()] = value);
                window[key] = {{ state: "done", target, attempts: window[key].attempts,
                                 status: response.status, headers, body,
                                 retryAt: Date.now() + {retry_ms} }};
              }} catch (error) {{
                window[key] = {{ state: "error", target, attempts: window[key].attempts,
                                 message: String(error),
                                 retryAt: Date.now() + {retry_ms} }};
              }}
            }})();
          }}
          return JSON.stringify(window[key]);
        }})()"#,
        retry_ms = browser_retry_backoff_ms()
    )
}

fn browser_response_challenged(
    status: u16,
    headers: &std::collections::HashMap<String, String>,
) -> bool {
    headers.contains_key("sg-captcha")
        || (status == 202
            && headers
                .get("content-type")
                .is_some_and(|v| v.starts_with("text/html")))
        || status == 403
}

#[cfg(test)]
fn browser_restart_allowed(elapsed: Duration) -> bool {
    elapsed.as_millis() >= browser_retry_backoff_ms()
}

fn browser_retry_backoff_ms() -> u128 {
    BROWSER_RETRY_BACKOFF.as_millis()
}

/// Whose problem the challenge is on this pass.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// The page answers it alone, off-screen, and the user never learns there was one.
    Hidden,
    /// The user answers it, in a window built to be read and clicked.
    Visible,
}

impl Mode {
    fn visible(self) -> bool {
        self == Mode::Visible
    }

    fn budget(self) -> Duration {
        match self {
            Mode::Hidden => SOLVE_TIMEOUT,
            Mode::Visible => ASSIST_TIMEOUT,
        }
    }
}

/// One pass at the challenge, in a window of its own.
///
/// `Ok(false)` is "the store is still refusing us" — the ordinary outcome of a pass that ran
/// out of budget, or of a visible window the user closed. Only something that stopped the pass
/// from happening at all is an `Err`.
async fn attempt(app: &AppHandle, mode: Mode) -> anyhow::Result<bool> {
    let url: tauri::Url = challenge_url().parse()?;
    log::info!(
        "opening the {} MXB Hub window to answer the robot challenge",
        if mode.visible() { "visible" } else { "hidden" }
    );

    let builder = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title(if mode.visible() {
            "MXB Hub — please finish the robot check"
        } else {
            "MXB Hub"
        })
        // Never given a way to talk to the app, shown or not — see the module comment.
        .visible(mode.visible())
        .decorations(mode.visible())
        .focused(mode.visible())
        .skip_taskbar(!mode.visible());
    let builder = match browser_user_agent() {
        Some(user_agent) => builder.user_agent(user_agent),
        None => builder,
    };

    let builder = if mode.visible() {
        // The sign-in window's shape, because it is the same kind of thing: a store page the
        // user has to deal with by hand.
        builder.inner_size(520.0, 760.0).center()
    } else {
        // A viewport with real area, and **not** the 1x1 this used to open. The challenge
        // computes its proof of work in a Web Worker, and a window with no area is the shape a
        // host throttles hardest — which is what a session that never earns a single new
        // cookie, and dies on the solve budget every time, looks like from the outside.
        // Off-screen, so nothing flashes if a platform shows it regardless.
        builder.inner_size(1024.0, 768.0).position(-32000.0, -32000.0)
    };

    let window = builder.build().inspect_err(|e| {
        // Mute until now: the caller turns this into "still refused", which reads as the store
        // saying no when in fact the app never opened anything.
        log::error!("the MXB Hub challenge window could not be built: {e}");
    })?;

    let deadline = std::time::Instant::now() + mode.budget();
    let mut last_seen: Vec<(String, String)> = Vec::new();
    let mut next_probe = std::time::Instant::now() + FIRST_PROBE;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(POLL).await;

        // A window the user was asked to finish is one they can also close. That is a decision,
        // not a budget to sit out — but it is not automatically a refusal either. The page may
        // have come up clean, with nothing to click, and closing it is then the reasonable
        // thing to do. The probe below runs on a timer, so the window is easily gone before
        // the next one was due; ask the store once more before calling this a failure.
        if mode.visible() && app.get_webview_window(WINDOW).is_none() {
            if probe().await {
                crate::hub_session::adopt_clearance(app, &last_seen);
                LAST.store(now(), Ordering::Relaxed);
                log::info!(
                    "the MXB Hub check window was closed and the store is letting us in ({})",
                    crate::hub_session::cookie_names(&last_seen)
                );
                return Ok(true);
            }
            log::info!("the MXB Hub check window was closed before the check was finished");
            return Ok(false);
        }

        let cookies = cookie_session::cookies_from_window(&window, &HUB_SITE);
        let changed = cookies != last_seen;
        if changed {
            log::debug!(
                "MXB Hub window cookies now: {}",
                crate::hub_session::cookie_names(&cookies)
            );
            last_seen = cookies.clone();
            if !cookies.is_empty() {
                mods::hub::adopt_clearance(&cookies)?;
            }
        }
        // Probed on a timer as well as on a cookie, because the clearance need not arrive as
        // one. SiteGround is just as free to stop refusing this *address* once its script has
        // run, and a loop that only looks when a cookie moves would sit out the whole timeout
        // next to a store that had already let us back in.
        if std::time::Instant::now() < next_probe && !changed {
            continue;
        }
        next_probe = std::time::Instant::now() + PROBE_EVERY;

        // What "solved" means is asked of the store, not guessed from a cookie name. The
        // challenge sets more than one cookie and renames them between SiteGround versions, so
        // matching on a name is a check that silently stops working; a request that comes back
        // as the thing we asked for cannot.
        if probe().await {
            crate::hub_session::adopt_clearance(app, &cookies);
            LAST.store(now(), Ordering::Relaxed);
            log::info!(
                "MXB Hub clearance earned ({})",
                crate::hub_session::cookie_names(&cookies)
            );
            close(app);
            return Ok(true);
        }
    }

    close(app);
    log::warn!(
        "the MXB Hub challenge was not answered within {}s in the {} window (cookies: {})",
        mode.budget().as_secs(),
        if mode.visible() { "visible" } else { "hidden" },
        crate::hub_session::cookie_names(&last_seen)
    );
    Ok(false)
}

/// The cheapest request the store answers, used only to ask "are we still challenged?".
///
/// One product, and no parsing: all that matters is whether what came back is the challenge.
async fn probe() -> bool {
    let Ok(client) = mods::hub::client() else {
        return false;
    };
    let request = client
        .get(challenge_url())
        .send();
    match tokio::time::timeout(PROBE_TIMEOUT, request).await {
        Ok(Ok(resp)) => !mods::hub::challenged(&resp) && resp.status().is_success(),
        // A probe that timed out and a probe that was refused mean the same thing here: not
        // yet. The loop's own deadline decides when that stops being worth asking.
        _ => false,
    }
}

pub fn close(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW) {
        // `close()` only *asks* the window to go, and the label stays taken until it actually
        // does. The next pass then fails to build with "a window with label … already
        // exists" — which is how the visible window came to never open at all: the hidden one
        // timed out, the app told the user to finish the check in a window that was never
        // there, and the error went nowhere because a build failure is an `Err` this function
        // never sees. `destroy()` tears it down rather than asking.
        let _ = win.destroy();
    }
}

/// Close the window and wait for its label to come free.
///
/// Even `destroy()` unwinds on the main thread, so a builder that runs in the same breath can
/// still land on a label that is on its way out. A short wait costs nothing next to a pass
/// that is about to sit out a 40 second budget.
async fn close_and_settle(app: &AppHandle) {
    close(app);
    for _ in 0..40 {
        if app.get_webview_window(WINDOW).is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    log::warn!("the MXB Hub window did not go away; the next pass may fail to open");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two passes must not be interchangeable. A hidden window is given only as long as a
    /// script plausibly needs; the visible one has to outlast a person reading the page,
    /// finding the checkbox and waiting for the store to catch up.
    #[test]
    fn the_user_gets_longer_than_the_script_did() {
        assert!(
            Mode::Visible.budget() > Mode::Hidden.budget(),
            "a person ({:?}) cannot be given less time than the headless pass ({:?})",
            Mode::Visible.budget(),
            Mode::Hidden.budget()
        );
        assert!(
            Mode::Hidden.budget() >= Duration::from_secs(30),
            "the proof of work needs room to run before the user is interrupted over it"
        );
    }

    /// Only the visible pass may be seen. The hidden one exists so that the ordinary case
    /// costs the user nothing, and a window that shows up unasked spends exactly that.
    #[test]
    fn only_the_pass_meant_to_be_seen_is_visible() {
        assert!(Mode::Visible.visible());
        assert!(!Mode::Hidden.visible());
    }

    /// A probe has to be able to end well inside the budget it is spent from, or the deadline
    /// it is checked against means nothing — the loop only tests the clock between probes.
    #[test]
    fn a_probe_cannot_outlast_the_pass_it_runs_in() {
        assert!(
            PROBE_TIMEOUT < Mode::Hidden.budget(),
            "one probe ({PROBE_TIMEOUT:?}) could spend the whole hidden budget ({:?})",
            Mode::Hidden.budget()
        );
    }

    /// The throttle exists to collapse the burst that one refused page load produces — a
    /// search, two category reads and the purchases, all inside the same second. It must not
    /// grow into something that swallows a person's retry.
    #[test]
    fn a_failure_is_remembered_long_enough_to_collapse_one_burst() {
        assert!(RETRY_AFTER_FAILURE > 0, "a failure nobody remembers is reopened four times");
        assert!(
            RETRY_AFTER_FAILURE < Mode::Hidden.budget().as_secs(),
            "standing on a stale refusal for longer than an attempt takes is just a slower no"
        );
    }

    #[test]
    fn clearance_window_uses_only_the_platform_safe_identity() {
        #[cfg(target_os = "macos")]
        assert_eq!(browser_user_agent(), Some(HUB_SITE.ua));

        #[cfg(not(target_os = "macos"))]
        assert_eq!(browser_user_agent(), None);
    }

    #[test]
    fn browser_transport_never_becomes_an_open_proxy() {
        assert!(browser_target_allowed(
            "https://shop.mxb-hub.com/wp-json/wc/store/v1/products?per_page=1"
        ));
        for refused in [
            "http://shop.mxb-hub.com/wp-json",
            "https://mxb-hub.com/wp-json",
            "https://shop.mxb-hub.com.evil.example/wp-json",
            "https://shop.mxb-hub.com:444/wp-json",
            "https://user:pass@shop.mxb-hub.com/wp-json",
            "https://example.com/",
            "not a url",
        ] {
            assert!(!browser_target_allowed(refused), "accepted {refused}");
        }
    }

    #[test]
    fn browser_transport_recognises_every_hub_refusal_shape() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("content-type".into(), "text/html; charset=UTF-8".into());
        assert!(browser_response_challenged(202, &headers));
        assert!(browser_response_challenged(
            403,
            &std::collections::HashMap::new()
        ));
        headers.insert("sg-captcha".into(), "1".into());
        assert!(browser_response_challenged(200, &headers));
        assert!(!browser_response_challenged(
            200,
            &std::collections::HashMap::new()
        ));
    }

    #[test]
    fn browser_response_cap_is_small_enough_for_control_plane_data() {
        assert_eq!(MAX_BROWSER_BODY, 8 * 1024 * 1024);
        assert!(MAX_BROWSER_BODY < 32 * 1024 * 1024);
    }
    #[test]
    fn failed_browser_fetches_do_not_restart_on_the_poll_cadence() {
        assert!(!browser_restart_allowed(POLL));
        assert!(!browser_restart_allowed(BROWSER_RETRY_BACKOFF - Duration::from_millis(1)));
        assert!(browser_restart_allowed(BROWSER_RETRY_BACKOFF));
        assert!(BROWSER_RETRY_BACKOFF >= Duration::from_secs(5));
        assert!(
            BROWSER_RETRY_BACKOFF * MAX_BROWSER_ATTEMPTS >= ASSIST_TIMEOUT,
            "the visible challenge must remain retryable for its whole budget"
        );
        let script = browser_request_script("\"https://shop.mxb-hub.com/test\"");
        assert!(script.contains("attempts < 32"));
        let script = browser_request_script(r#""https://shop.mxb-hub.com/test""#);
        assert!(script.contains("retryAt"), "{script}");
        assert!(
            script.contains(&browser_retry_backoff_ms().to_string()),
            "{script}"
        );
        assert!(!script.contains("delete window"), "{script}");
    }
}
