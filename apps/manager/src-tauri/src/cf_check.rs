//! Cloudflare's managed check, for the hidden WebViews that fetch from behind it.
//!
//! [`crate::mxb_fetch`] (mxb-mods.com) and [`crate::shop_fetch`] (mxbikes-shop.com) each park a
//! hidden window on a site that serves Cloudflare's managed challenge. Most of the time the
//! check clears by itself in the hidden window. Sometimes it wants a click, and a hidden window
//! cannot be clicked: it waits out its timeout and the request fails, every time.
//!
//! So the window waits [`REVEAL_AFTER`] for the check to clear by itself, then shows itself to
//! the user, sized and titled for it, and hides again once the check is past. The check is
//! always the user's to complete: nothing here answers it. Closing the window, or leaving it
//! for [`ASSIST_TIMEOUT`], ends the attempt and latches until the site's module re-arms it with
//! [`Site::reset`], so a refusal can never turn into a window that keeps reopening.
//!
//! Everything here is per [`Site`]: its window, its domain, what the main window is told, and
//! what error a request gets once the session has stopped asking.

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};

/// How long Cloudflare's check gets to clear by itself in the hidden window before the user is
/// shown it. A non-interactive challenge passes in a second or two; one still up after this
/// is waiting for a person.
pub const REVEAL_AFTER: Duration = Duration::from_secs(5);

/// How long the shown check is left up for the user to complete.
pub const ASSIST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The size of the shown window: enough for the Turnstile widget and Cloudflare's text.
pub const ASSIST_SIZE: (f64, f64) = (520.0, 640.0);

/// Why a session has stopped asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GiveUp {
    /// The user closed the check window.
    Dismissed = 1,
    /// The check was shown and left for [`ASSIST_TIMEOUT`] without being completed.
    TimedOut = 2,
    /// The WebView could not be built at all — no WebView2, or a Wine prefix without one.
    Unavailable = 3,
}

impl GiveUp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Dismissed),
            2 => Some(Self::TimedOut),
            3 => Some(Self::Unavailable),
            _ => None,
        }
    }
}

/// A site's mutable half. One per site, in a `static`.
pub struct State {
    /// `0` = still asking; otherwise a [`GiveUp`] discriminant. Read before every request so a
    /// dismissed check fails at once instead of rebuilding a window to be refused again.
    gave_up: AtomicU8,
    /// Whether the window that is up has been seen past the check. Belongs to that window: a
    /// stale `true` would let the *next* window be handed out before its own check cleared.
    ready: AtomicBool,
    /// One shown check at a time: a second request arriving mid-check waits here, and finds the
    /// answer the first one got instead of opening anything.
    asking: tokio::sync::Mutex<()>,
    /// What the last probe saw, for the log lines that say how a check ended.
    seen: Mutex<String>,
}

impl State {
    pub const fn new() -> Self {
        Self {
            gave_up: AtomicU8::new(0),
            ready: AtomicBool::new(false),
            asking: tokio::sync::Mutex::const_new(()),
            seen: Mutex::new(String::new()),
        }
    }
}

/// One site behind Cloudflare, and the window that fetches from it.
pub struct Site {
    /// The window's label.
    pub window: &'static str,
    /// `mxb-mods.com`. A function because the mods site switches with the active game.
    pub domain: fn() -> &'static str,
    /// `https://mxb-mods.com`. The window counts as past the check only on this origin.
    pub base: fn() -> &'static str,
    /// What completing the check is for, ending the shown window's title: "load mod pages".
    pub purpose: &'static str,
    /// The event the main window is told on. Payload: `{ state, site }`.
    pub event: &'static str,
    /// How long a page that is simply *loading* — not challenged — gets to be ready.
    pub ready_timeout: Duration,
    /// The error a request gets once the session has stopped asking.
    pub stopped: fn(GiveUp) -> anyhow::Error,
    pub state: &'static State,
}

/// [`Site::event`]'s payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Verify {
    /// `shown`, `cleared`, `dismissed` or `timedOut`.
    state: &'static str,
    site: &'static str,
}

impl Site {
    pub fn gave_up(&self) -> Option<GiveUp> {
        GiveUp::from_u8(self.state.gave_up.load(Ordering::Acquire))
    }

    /// Stop asking for this session, and the error that says why.
    pub fn give_up(&self, reason: GiveUp) -> anyhow::Error {
        self.state.gave_up.store(reason as u8, Ordering::Release);
        (self.stopped)(reason)
    }

    /// `Err` if this session has stopped asking.
    pub fn check_asking(&self) -> anyhow::Result<()> {
        match self.gave_up() {
            Some(reason) => Err((self.stopped)(reason)),
            None => Ok(()),
        }
    }

    /// The user asked again. Whatever made this session stop asking, ask once more.
    pub fn reset(&self) {
        if let Some(reason) = GiveUp::from_u8(self.state.gave_up.swap(0, Ordering::AcqRel)) {
            log::info!("{} check re-armed by the user after {reason:?}", (self.domain)());
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state.ready.load(Ordering::Acquire)
    }

    pub fn set_ready(&self, ready: bool) {
        self.state.ready.store(ready, Ordering::Release);
    }

    /// Tell the main window what the check is doing, so it can say so — WebView2 has no way to
    /// put our own line of text above a remote page.
    fn notify(&self, app: &AppHandle, state: &'static str) {
        let payload = Verify {
            state,
            site: (self.domain)(),
        };
        // To the main window only: the fetch window runs the remote site, which has no business
        // hearing about it.
        if let Err(e) = app.emit_to(crate::MAIN_WINDOW, self.event, payload) {
            log::debug!("could not tell the main window the check is {state}: {e}");
        }
    }

    /// Read the page's state straight out of the WebView. See [`probe_script`].
    pub async fn probe(&self, window: &WebviewWindow) -> Probe {
        let reply = eval_json::<ProbeReply>(window, &probe_script(), Duration::from_secs(1)).await;
        *self.state.seen.lock().unwrap_or_else(|e| e.into_inner()) = describe(reply.as_ref());
        classify(reply, (self.base)())
    }

    /// What the last probe saw, and the names (never the values) of the cookies the window
    /// holds for the site. Logged whenever a shown check ends, so a report says which page the
    /// window was on and whether it held a clearance and a session.
    async fn seen(&self, window: Option<&WebviewWindow>) -> String {
        let page = self.state.seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(window) = window.cloned() else {
            return page;
        };
        let Ok(url) = (self.base)().parse::<tauri::Url>() else {
            return page;
        };
        // Off the async workers: on Windows this waits on the UI thread.
        let read = tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(url)).await;
        let cookies = match read {
            Ok(Ok(cookies)) => cookie_names(cookies.iter().map(|c| c.name())),
            _ => "unreadable".to_string(),
        };
        format!("{page}; cookies: {cookies}")
    }

    /// Wait until the window shows a page past Cloudflare's check — and, when `replacing` is
    /// set, not the page a navigation was just asked to leave.
    ///
    /// `replacing` is that document's `performance.timeOrigin`. Without it the check answers
    /// about whatever is still on screen, which straight after a `navigate` is the previous
    /// page: complete, past its own check, and wrong.
    ///
    /// Hidden first. A page that is merely loading gets [`Site::ready_timeout`]; one sitting on
    /// the check gets [`REVEAL_AFTER`] to clear by itself, and is then shown to the user.
    pub async fn clear(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        replacing: Option<f64>,
    ) -> anyhow::Result<()> {
        let started = Instant::now();
        let mut challenged: Option<Instant> = None;
        let mut last = "no answer from the page yet";
        loop {
            let probe = self.probe(window).await;
            if probe.is_past(replacing) {
                if let Some(since) = challenged {
                    log::info!(
                        "{} check cleared by itself in the hidden window after {:?}",
                        (self.domain)(),
                        since.elapsed()
                    );
                }
                return Ok(());
            }
            match probe.page {
                Page::Challenge => {
                    let since = *challenged.get_or_insert_with(Instant::now);
                    if since.elapsed() >= REVEAL_AFTER {
                        return self
                            .ask_user(app, window, || async move {
                                Ok(self.probe(window).await.is_past(replacing))
                            })
                            .await;
                    }
                }
                Page::Ready => last = "the page it was told to leave is still loaded",
                Page::Loading => last = "still loading",
            }
            if challenged.is_none() && started.elapsed() >= self.ready_timeout {
                return Err(anyhow::anyhow!(
                    "the hidden {} window didn't finish loading in {:?} ({last})",
                    (self.domain)(),
                    self.ready_timeout
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Show the check to the user, and wait until `done` says it is past or the user closes it.
    ///
    /// `done` is "the page is past the check" for a read. For a download it is "the download
    /// started", because a file never becomes a page. An `Err` from it (a cancel) hides the
    /// window and ends the wait.
    pub async fn ask_user<F, Fut>(
        &self,
        app: &AppHandle,
        window: &WebviewWindow,
        mut done: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = anyhow::Result<bool>>,
    {
        let _guard = self.state.asking.lock().await;
        self.check_asking()?;
        if done().await? {
            return Ok(());
        }

        let domain = (self.domain)();
        log::info!(
            "{domain} check did not clear by itself within {REVEAL_AFTER:?} — showing it to the \
             user (saw {})",
            self.seen(Some(window)).await
        );
        self.reveal(window);
        self.notify(app, "shown");

        let shown = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            // Closed by the user. The window is transient (never parked in the tray), so a close
            // is a destroy and the label is gone.
            if app.get_webview_window(self.window).is_none() {
                self.set_ready(false);
                log::info!(
                    "user closed the {domain} check after {:?} — not asking again until they \
                     ask for it (last saw {})",
                    shown.elapsed(),
                    self.seen(None).await
                );
                self.notify(app, "dismissed");
                return Err(self.give_up(GiveUp::Dismissed));
            }
            match done().await {
                Ok(true) => {
                    log::info!(
                        "{domain} check cleared by the user after {:?} (saw {})",
                        shown.elapsed(),
                        self.seen(Some(window)).await
                    );
                    self.conceal(window);
                    self.notify(app, "cleared");
                    return Ok(());
                }
                Ok(false) => {}
                Err(e) => {
                    self.conceal(window);
                    return Err(e);
                }
            }
            if shown.elapsed() >= ASSIST_TIMEOUT {
                log::info!(
                    "{domain} check was shown for {ASSIST_TIMEOUT:?} and not completed — timed \
                     out; not asking again until the user asks for it"
                );
                self.conceal(window);
                self.notify(app, "timedOut");
                return Err(self.give_up(GiveUp::TimedOut));
            }
        }
    }

    /// The shown window's title.
    pub fn title(&self) -> String {
        format!(
            "Verifying access to {} — complete the check below to {}",
            (self.domain)(),
            self.purpose
        )
    }

    /// Undo everything that keeps the window out of sight, and put it in front of the user.
    fn reveal(&self, window: &WebviewWindow) {
        let title = self.title();
        // Each step on its own: a window manager that refuses one (Wine's is the likeliest)
        // should not stop the rest from putting the window where it can be seen.
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
                log::warn!("showing the {} check window: {step} failed: {e}", (self.domain)());
            }
        }
    }

    /// Back out of sight, the way the window was built, for the requests still to come.
    fn conceal(&self, window: &WebviewWindow) {
        let _ = window.hide();
        let _ = window.set_skip_taskbar(true);
        let _ = window.set_decorations(false);
        let _ = window.set_size(LogicalSize::new(1.0, 1.0));
        let _ = window.set_position(LogicalPosition::new(-32000.0, -32000.0));
        let _ = window.set_title((self.domain)());
    }
}

/// What the page in the window is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Loaded, script and IPC available, and not Cloudflare's check.
    Ready,
    /// Sitting on Cloudflare's check.
    Challenge,
    /// Anything else: mid-navigation, not answering, or a page without our IPC.
    Loading,
}

#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub page: Page,
    /// The document's `performance.timeOrigin`, minted fresh per document — how a caller that
    /// has just navigated tells the page it asked for from the one it asked to leave.
    pub origin: Option<f64>,
    /// The document is on the site's own origin.
    pub on_site: bool,
}

impl Probe {
    pub fn is_past(&self, replacing: Option<f64>) -> bool {
        self.page == Page::Ready && self.on_site && (replacing.is_none() || self.origin != replacing)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeReply {
    ipc: bool,
    complete: bool,
    /// Shaped like Cloudflare's interstitial. See [`probe_script`].
    challenge: bool,
    /// Carries the site's own content, which the interstitial never does.
    #[serde(default)]
    site_page: bool,
    /// `window._cf_chl_opt` exists at all. Logged, never decided on (see [`probe_script`]).
    #[serde(default)]
    chl_opt: bool,
    #[serde(default)]
    path: String,
    #[serde(default)]
    title: String,
    origin: f64,
    /// `location.origin`.
    #[serde(default)]
    site: String,
}

/// One line for the log: where the window was and every signal the probe read there.
fn describe(reply: Option<&ProbeReply>) -> String {
    let Some(r) = reply else {
        return "no answer from the page".to_string();
    };
    let title: String = r.title.chars().take(80).collect();
    format!(
        "{}{} {title:?}: interstitial={} sitePage={} _cf_chl_opt={} complete={} ipc={}",
        r.site, r.path, r.challenge, r.site_page, r.chl_opt, r.complete, r.ipc
    )
}

/// Cookie names, sorted, for the log. Names only: the values are the user's session.
fn cookie_names<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<&str> = names.collect();
    if names.is_empty() {
        return "none".to_string();
    }
    names.sort_unstable();
    names.dedup();
    names.join(", ")
}

fn classify(reply: Option<ProbeReply>, base: &str) -> Probe {
    let Some(r) = reply else {
        return Probe {
            page: Page::Loading,
            origin: None,
            on_site: false,
        };
    };
    // The site's own page is never the check, whatever Cloudflare's scripts left on it.
    let page = if r.challenge && !r.site_page {
        Page::Challenge
    } else if r.complete && r.ipc {
        Page::Ready
    } else {
        Page::Loading
    };
    Probe {
        page,
        origin: Some(r.origin),
        on_site: same_site(&r.site, base),
    }
}

/// Whether the origin the window reports (`location.origin`) is the site's.
pub fn same_site(origin: &str, base: &str) -> bool {
    origin.trim_end_matches('/').eq_ignore_ascii_case(base.trim_end_matches('/'))
}

/// The probe, as its own function so a test can hold it to what it has to say.
///
/// Asked with `eval_with_callback` rather than over our IPC, because the question that matters
/// most — *is this the check?* — has to be answerable on the check itself. Our IPC's presence is
/// still one of the answers: the fetches depend on it, and if a Tauri upgrade ever renames
/// `__TAURI_INTERNALS__` this reports "loading" for good rather than declaring the page ready.
///
/// "On the check" is asked as a `window._cf_chl_opt` carrying the interstitial's own fields
/// (`cType`, `cRay`), or a "Just a moment…" title. Neither a bare `_cf_chl_opt` nor
/// `challenge-platform` in the HTML will do, and both fail the same quiet way: Cloudflare's
/// precursor script, which it injects into ordinary pages, defines `_cf_chl_opt` without those
/// fields, and its JS detections put `challenge-platform` in the HTML. Keyed on either, a page
/// the user is already past reads as the check for good — which is what mxbikes-shop.com's
/// All My Downloads did: the shown check sat on the signed-in page until the user closed it,
/// and the close failed the load.
///
/// `sitePage` is the positive answer: anything from the site's own `/wp-content/` (both sites
/// are WordPress) is a page the site served, and the interstitial carries none of it.
fn probe_script() -> String {
    "(function(){ try { var o = window._cf_chl_opt; return { \
       ipc: !!window.__TAURI_INTERNALS__, \
       complete: document.readyState === 'complete', \
       challenge: !!(o && (o.cType || o.cRay)) \
         || /^just a moment/i.test(document.title || ''), \
       sitePage: !!document.querySelector('[src*=\"/wp-content/\"],[href*=\"/wp-content/\"]'), \
       chlOpt: typeof o !== 'undefined', path: location.pathname, title: document.title || '', \
       origin: performance.timeOrigin, site: location.origin }; } catch (e) { return null; } })()"
        .to_string()
}

/// Evaluate `js` in `window` and deserialize what it returns. `None` for anything that did not
/// come back as the expected shape in time — a page mid-navigation answers `null`, or nothing.
pub async fn eval_json<T: serde::de::DeserializeOwned + Send + 'static>(
    window: &WebviewWindow,
    js: &str,
    within: Duration,
) -> Option<T> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    window
        .eval_with_callback(js, move |json| {
            if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = tx.send(json);
            }
        })
        .ok()?;
    let json = tokio::time::timeout(within, rx).await.ok()?.ok()?;
    serde_json::from_str(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://example-shop.com";

    fn reply(ipc: bool, complete: bool, challenge: bool, origin: f64) -> Option<ProbeReply> {
        Some(ProbeReply {
            ipc,
            complete,
            challenge,
            site_page: false,
            chl_opt: challenge,
            path: "/".to_string(),
            title: String::new(),
            origin,
            site: BASE.to_string(),
        })
    }

    static TEST_STATE: State = State::new();
    static TEST_SITE: Site = Site {
        window: "test-fetch",
        domain: || "example-shop.com",
        base: || BASE,
        purpose: "load your purchases",
        event: "test-verify",
        ready_timeout: Duration::from_secs(30),
        stopped: |reason| anyhow::anyhow!("stopped: {reason:?}"),
        state: &TEST_STATE,
    };

    /// The bug the mods window shipped with: the probe answered on Cloudflare's interstitial,
    /// because Tauri injects its IPC there too. Every question below has to be asked, or the
    /// window is declared ready while still on "Just a moment…" and every request is refused.
    #[test]
    fn the_probe_asks_every_question() {
        let js = probe_script();
        assert!(js.contains("_cf_chl_opt"), "{js}");
        assert!(js.contains("o.cType || o.cRay"), "{js}");
        // The bare global is what Cloudflare's precursor script defines on ordinary pages.
        assert!(!js.contains("typeof window._cf_chl_opt"), "{js}");
        assert!(js.contains("/wp-content/"), "{js}");
        assert!(js.contains("just a moment"), "{js}");
        assert!(js.contains("readyState === 'complete'"), "{js}");
        assert!(js.contains("__TAURI_INTERNALS__"), "{js}");
        // Cloudflare injects `challenge-platform` into ordinary pages as well, so keying on it
        // would mean the window is never ready at all.
        assert!(!js.contains("challenge-platform"), "{js}");
        assert!(js.contains("performance.timeOrigin"), "{js}");
        assert!(js.contains("location.origin"), "{js}");
    }

    /// The challenge wins over everything else — Tauri's IPC is injected into it and it
    /// finishes loading like any page — and nothing that failed to answer is ever ready.
    #[test]
    fn a_challenge_is_never_ready() {
        assert_eq!(classify(reply(true, true, true, 1.0), BASE).page, Page::Challenge);
        assert_eq!(classify(reply(true, false, true, 1.0), BASE).page, Page::Challenge);
        assert_eq!(classify(reply(true, true, false, 1.0), BASE).page, Page::Ready);
        assert_eq!(classify(reply(true, false, false, 1.0), BASE).page, Page::Loading);
        assert_eq!(classify(reply(false, true, false, 1.0), BASE).page, Page::Loading);
        assert_eq!(classify(None, BASE).page, Page::Loading);
    }

    /// mxbikes-shop.com's All My Downloads, signed in, after the user passed the shown check:
    /// Cloudflare's precursor script had defined `_cf_chl_opt` on it. The page's own content
    /// settles it, so the check is past and the window hides itself.
    #[test]
    fn the_sites_own_page_is_past_whatever_cloudflare_left_on_it() {
        let mut downloads = reply(true, true, true, 1.0).unwrap();
        downloads.site_page = true;
        let p = classify(Some(downloads), BASE);
        assert_eq!(p.page, Page::Ready);
        assert!(p.is_past(None));
        // Still loading is still loading, and a reply without the field reads as before.
        let mut loading = reply(true, false, true, 1.0).unwrap();
        loading.site_page = true;
        assert_eq!(classify(Some(loading), BASE).page, Page::Loading);
        let bare: ProbeReply = serde_json::from_str(
            r#"{"ipc":true,"complete":true,"challenge":true,"origin":1,"site":"https://example-shop.com"}"#,
        )
        .unwrap();
        assert_eq!(classify(Some(bare), BASE).page, Page::Challenge);
    }

    /// A report has to say which page the window was on and what the probe made of it, and the
    /// cookies by name only.
    #[test]
    fn the_log_line_says_what_the_window_saw() {
        let mut r = reply(true, true, false, 1.0).unwrap();
        r.path = "/all-my-downloads/".into();
        r.title = "All My Downloads - MX Bikes Shop".into();
        r.site_page = true;
        r.chl_opt = true;
        let line = describe(Some(&r));
        assert!(line.starts_with("https://example-shop.com/all-my-downloads/"), "{line}");
        assert!(line.contains(r#""All My Downloads - MX Bikes Shop""#), "{line}");
        assert!(line.contains("interstitial=false sitePage=true _cf_chl_opt=true"), "{line}");
        assert_eq!(describe(None), "no answer from the page");
        assert_eq!(
            cookie_names(["wordpress_logged_in_x", "cf_clearance", "cf_clearance"].into_iter()),
            "cf_clearance, wordpress_logged_in_x"
        );
        assert_eq!(cookie_names(std::iter::empty()), "none");
    }

    /// Straight after a navigation the old page is still loaded and past its check. It must
    /// not count as the new one.
    #[test]
    fn the_page_being_replaced_does_not_count() {
        let old = classify(reply(true, true, false, 100.0), BASE);
        assert!(old.is_past(None));
        assert!(!old.is_past(Some(100.0)));
        assert!(classify(reply(true, true, false, 200.0), BASE).is_past(Some(100.0)));
        assert!(!classify(reply(true, true, true, 200.0), BASE).is_past(Some(100.0)));
    }

    /// A document anywhere but the site's origin is never "past", however complete it looks.
    #[test]
    fn a_page_off_the_site_is_not_ready() {
        let mut off = reply(true, true, false, 1.0).unwrap();
        off.site = "https://example.org".to_string();
        let p = classify(Some(off), BASE);
        assert!(!p.on_site);
        assert!(!p.is_past(None));
        assert!(same_site("https://mxb-mods.com/", "https://mxb-mods.com"));
        assert!(!same_site("https://www.mxb-mods.com", "https://mxb-mods.com"));
        assert!(!same_site("", "https://mxb-mods.com"));
    }

    /// A closed check stops the session asking, and only a reset starts it again.
    #[test]
    fn a_dismissed_check_fails_fast_until_reset() {
        TEST_SITE.reset();
        assert_eq!(TEST_SITE.gave_up(), None);
        assert!(TEST_SITE.check_asking().is_ok());
        let err = TEST_SITE.give_up(GiveUp::Dismissed);
        assert!(err.to_string().contains("Dismissed"), "{err}");
        assert_eq!(TEST_SITE.gave_up(), Some(GiveUp::Dismissed));
        let again = TEST_SITE.check_asking().unwrap_err();
        assert!(again.to_string().contains("Dismissed"), "{again}");
        TEST_SITE.reset();
        assert_eq!(TEST_SITE.gave_up(), None);
        for reason in [GiveUp::Dismissed, GiveUp::TimedOut, GiveUp::Unavailable] {
            assert_eq!(GiveUp::from_u8(reason as u8), Some(reason));
        }
        assert_eq!(GiveUp::from_u8(0), None);
    }

    /// The shown window says which site it is checking, and what for.
    #[test]
    fn the_shown_title_names_the_site_and_purpose() {
        let title = TEST_SITE.title();
        assert!(title.starts_with("Verifying access to example-shop.com"), "{title}");
        assert!(title.ends_with("to load your purchases"), "{title}");
    }

    #[test]
    fn the_shown_window_fits_the_widget() {
        assert!(ASSIST_SIZE.0 >= 400.0 && ASSIST_SIZE.1 >= 500.0);
        assert!(REVEAL_AFTER < ASSIST_TIMEOUT);
    }
}
