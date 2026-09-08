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
//! Only text comes back this way — the catalog JSON and mod-page HTML. Mod downloads resolve
//! to MediaFire/Drive/Mega and never touch this path, so no large binary is ever marshalled
//! through JavaScript.

use crate::mods::mxb::Fetched;
use crate::mxb_session;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

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

/// How long to wait for the page to be ready to run script after the window is built.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Set once from `setup`. The mods code is a set of free functions with no handle to thread
/// through — `search`, `detail` and `ratings` would all have to grow an `AppHandle`
/// parameter, along with everything between them and the command layer, to avoid this.
static APP: OnceLock<AppHandle> = OnceLock::new();

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
    /// The document's `performance.timeOrigin`, minted fresh per document. It is how
    /// [`probe`] tells the page a navigation asked for from the one it asked to leave.
    #[serde(default)]
    origin: f64,
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
    let app = app()?;
    let window = ensure_window(app).await?;

    // Which document is being replaced. `navigate` returns immediately and the old page stays
    // loaded — and past its own check — until the new one arrives, so a readiness test that
    // could not tell them apart would pass at once and read the page we just left.
    let previous = probe(&window).await.ok();
    window.navigate(url.parse()?)?;
    wait_until_ready_replacing(&window, previous).await?;

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

    Ok(Fetched {
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
    })
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
    let app = app()?;
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

    Ok(Fetched {
        status: reply.status,
        headers: reply.headers,
        body: reply.body,
        url: if reply.url.is_empty() {
            url.to_string()
        } else {
            reply.url
        },
    })
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

/// Build the hidden window on first use and leave it up for the session.
///
/// Serialised, so two requests arriving together don't both try to build it.
async fn ensure_window(app: &AppHandle) -> anyhow::Result<tauri::WebviewWindow> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    // Readiness is tracked separately from existence: a window that was built but never
    // came good must not be handed out as if it were working, or every fetch through it
    // times out 45 seconds at a time.
    static READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    if let Some(existing) = app.get_webview_window(WINDOW) {
        if READY.load(Ordering::Relaxed) {
            return Ok(existing);
        }
        wait_until_ready(&existing).await?;
        READY.store(true, Ordering::Relaxed);
        return Ok(existing);
    }

    let url: tauri::Url = mxb_session::base().parse()?;
    log::info!("opening the hidden mxb-mods.com fetch window");
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title("mxb-mods.com")
        .user_agent(mxb_session::UA)
        // Never shown. This was first built visible-but-off-screen, on the worry that a
        // fully hidden window would have its timers throttled and read as the site being
        // slow. But off-screen only takes it off the desktop — it stays a window the
        // system lists and can surface, and `skip_taskbar` below does nothing on macOS, so
        // it turns up in the Window menu and Mission Control with no titlebar to dismiss it
        // by. Nor was it the trade-off it looked like: `visible` here is the *window*'s
        // flag, and the webview inside keeps its own, which Tauri leaves on. WebView2 reads
        // page visibility from that one rather than from the host window, so the page still
        // counts as visible and nothing backgrounds it.
        .visible(false)
        // Kept as a second line of defence: if anything ever does show this window, it has
        // one pixel to do it in, well off-screen.
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

/// Wait for the page to be able to run our script, and to be past Cloudflare's check.
async fn wait_until_ready(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
    wait_until_ready_replacing(window, None).await
}

/// [`wait_until_ready`], for a page a navigation has just been asked for.
///
/// `replacing` is the document that has to be *gone* before the window counts as ready,
/// identified by its `performance.timeOrigin`. Without it the check answers about whatever is
/// still on screen, which straight after a `navigate` is the previous page: complete, past its
/// own check, and wrong.
async fn wait_until_ready_replacing(
    window: &tauri::WebviewWindow,
    replacing: Option<f64>,
) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + READY_TIMEOUT;
    let mut last: Option<String> = None;
    while std::time::Instant::now() < deadline {
        match probe(window).await {
            Ok(origin) if Some(origin) != replacing => return Ok(()),
            Ok(_) => last = Some("the page it was told to leave is still loaded".to_string()),
            Err(e) => last = Some(e.to_string()),
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(anyhow::anyhow!(
        "the hidden mxb-mods.com window never became ready — Cloudflare's check did not clear{}",
        last.map(|e| format!(" ({e})")).unwrap_or_default()
    ))
}

/// One round-trip that proves script can run and reach us, *and* that we are past the check.
///
/// This used to ask only whether `__TAURI_INTERNALS__` existed. Tauri injects that into
/// Cloudflare's interstitial as readily as into the real page, so the window was declared
/// ready while still sitting on "Just a moment…" — and every request made from there was
/// refused, which is precisely the "its check window didn't clear it either" a blocked user
/// was shown. [`crate::shop_fetch`] has always asked the fuller question; this is that.
///
/// "Past the check" is asked as `window._cf_chl_opt`, which the interstitial defines and an
/// ordinary page does not. The obvious test — looking for `challenge-platform` in the HTML —
/// is wrong, and quietly so: Cloudflare injects that script into ordinary pages too when JS
/// detections are on, so the window would never be declared ready at all.
///
/// Doubles as the check that `__TAURI_INTERNALS__` still exists — if a Tauri upgrade renames
/// it, this fails here with a clear message instead of every fetch timing out.
///
/// Answers with the document's `performance.timeOrigin`, so a caller that has just navigated
/// can tell the page it asked for from the one it asked to leave.
async fn probe(window: &tauri::WebviewWindow) -> anyhow::Result<f64> {
    const PROBE_ID: u64 = 0;
    let (tx, rx) = tokio::sync::oneshot::channel();
    lock(waiting()).insert(PROBE_ID, tx);
    if let Err(e) = window.eval(probe_script()) {
        lock(waiting()).remove(&PROBE_ID);
        return Err(anyhow::anyhow!("{e}"));
    }
    match tokio::time::timeout(Duration::from_millis(400), rx).await {
        Ok(Ok(reply)) => Ok(reply.origin),
        _ => {
            lock(waiting()).remove(&PROBE_ID);
            Err(anyhow::anyhow!("still on the check, or not ready yet"))
        }
    }
}

/// The probe, as its own function so a test can hold it to what it has to say.
fn probe_script() -> String {
    format!(
        "if (window.__TAURI_INTERNALS__ && document.readyState === 'complete' \
         && typeof window._cf_chl_opt === 'undefined' \
         && !/^just a moment/i.test(document.title || '')) {{ \
         window.__TAURI_INTERNALS__.invoke\
         ('plugin:event|emit', {{ event: {event}, \
         payload: {{id:0, origin: performance.timeOrigin}} }}); }}",
        event = js_string(RESULT_EVENT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_probe_refuses_to_answer_on_the_challenge() {
        let js = probe_script();
        assert!(js.contains("_cf_chl_opt"), "{js}");
        assert!(js.contains("just a moment"), "{js}");
        assert!(js.contains("readyState === 'complete'"), "{js}");
        // The naive test: Cloudflare injects `challenge-platform` into ordinary pages as
        // well, so keying on it would mean the window is never ready at all.
        assert!(!js.contains("challenge-platform"), "{js}");
        // Without a per-document token a caller that just navigated cannot tell the page it
        // asked for from the one it asked to leave.
        assert!(js.contains("performance.timeOrigin"), "{js}");
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
        // Zero is the probe's, and must never be handed to a real request.
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
            origin: 0.0,
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
            origin: 0.0,
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
