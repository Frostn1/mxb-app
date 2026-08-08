//! The mxb-mods.com side of [`cookie_session`], plus the WebView handshake that fills it.
//!
//! Browsing is anonymous — there is nothing to sign into here. The only cookie that matters
//! is Cloudflare's `cf_clearance`, which is earned by a real browser passing a challenge.
//!
//! Why we need one at all: a reqwest client can copy Chrome's headers exactly and still be
//! told apart by its TLS and HTTP/2 fingerprint, which no header can disguise. Cloudflare
//! scores that fingerprint against the caller's IP reputation, so the same binary sails
//! through on one connection and is refused on another — the shape of the report this
//! exists to fix. We ship a real browser, so when the site refuses the HTTP client we open
//! one, let it clear, and take the cookie.

use crate::cookie_session::{self, Cookies, Site};
use reqwest::cookie::Jar;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const BASE: &str = "https://mxb-mods.com";

/// A full four-part version, unlike the `Chrome/126.0` form [`crate::shop_session::UA`]
/// uses — real Chrome never sends a two-part version, and a UA no browser would emit is
/// itself a signal to a bot filter. The handshake WebView is built with this same constant:
/// a `cf_clearance` is bound to the User-Agent that earned it, so the two must not drift.
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.6778.140 Safari/537.36";

const SITE: Site = Site {
    base: BASE,
    domain: "mxb-mods.com",
    file: "mxb_session.json",
    ua: UA,
    timeout: Duration::from_secs(30),
};

const WINDOW: &str = "mxb-clearance";

/// How long we leave the window up. It normally closes in a few seconds — a managed
/// challenge clears on its own — but the budget has to cover one that wants a click.
const WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(500);

/// Process-wide, because the mods client is built once and must see cookies that arrive
/// long afterwards. `Jar` is internally synchronised, so filling it after the fact is
/// visible to the client already holding it.
pub fn jar() -> Arc<Jar> {
    static JAR: OnceLock<Arc<Jar>> = OnceLock::new();
    JAR.get_or_init(|| Arc::new(Jar::default())).clone()
}

/// The mods client is built from this so its User-Agent, jar and timeouts cannot drift from
/// the handshake WebView's. Callers add their own default headers.
pub fn client_builder() -> reqwest::ClientBuilder {
    cookie_session::client_builder(&SITE, jar())
}

/// Bumped whenever fresh cookies land, so a caller that waited behind another handshake can
/// tell it now has something new to retry with instead of opening a second window.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn store(app: &AppHandle, cookies: Cookies) {
    if let Err(e) = cookie_session::fill(&jar(), &SITE, &cookies) {
        log::warn!("could not add mxb-mods.com cookies to the jar: {e:#}");
        return;
    }
    if let Err(e) = cookie_session::write(app, &SITE, &cookies) {
        // Not fatal: the jar is filled, so this session works — only the next one arrives cold.
        log::warn!("could not persist mxb-mods.com cookies: {e:#}");
    }
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Replay last run's clearance, so a user who was blocked yesterday isn't blocked again on
/// launch. An expired cookie is harmless — it fails exactly like no cookie, and the 403
/// path mints a new one.
pub fn load(app: &AppHandle) {
    let Some(cookies) = cookie_session::read(app, &SITE) else {
        return;
    };
    if let Err(e) = cookie_session::fill(&jar(), &SITE, &cookies) {
        log::warn!("failed to restore mxb-mods.com cookies: {e:#}");
        return;
    }
    log::info!("restored mxb-mods.com cookies ({})", cookies.len());
}

/// Open a real browser at mxb-mods.com, wait for Cloudflare to hand it a `cf_clearance`,
/// and keep the cookie. `true` if we came away with one.
///
/// Serialised: a 403 on search and a 403 on a detail page arriving together must not open
/// two windows. The loser of the lock checks whether the winner already succeeded.
pub async fn handshake(app: &AppHandle) -> bool {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    let before = GENERATION.load(Ordering::Relaxed);
    let _guard = LOCK.lock().await;
    if GENERATION.load(Ordering::Relaxed) != before {
        return true; // another caller earned one while we waited
    }

    if let Some(existing) = app.get_webview_window(WINDOW) {
        let _ = existing.close();
    }

    let url = match BASE.parse() {
        Ok(url) => url,
        Err(e) => {
            log::error!("mxb-mods.com base URL is not a URL: {e}");
            return false;
        }
    };
    let window = match WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title("Checking with mxb-mods.com…")
        .user_agent(UA)
        .inner_size(480.0, 560.0)
        .build()
    {
        Ok(window) => window,
        Err(e) => {
            log::error!("could not open the mxb-mods.com check window: {e:#}");
            return false;
        }
    };

    let deadline = WAIT.as_millis() / POLL.as_millis();
    let mut last: Cookies = vec![];
    for _ in 0..deadline {
        tokio::time::sleep(POLL).await;
        let Some(win) = app.get_webview_window(WINDOW) else {
            log::info!("mxb-mods.com check window closed before it cleared");
            return false;
        };
        last = cookie_session::cookies_from_window(&win, &SITE);
        if cookie_session::has(&last, "cf_clearance") {
            store(app, last);
            let _ = win.close();
            log::info!("earned a cf_clearance for mxb-mods.com");
            return true;
        }
    }

    let _ = window.close();
    // No clearance, but `__cf_bm` and friends still feed the bot score, so keep what we got.
    if !last.is_empty() {
        store(app, last);
    }
    // Worth reading in a user's log, because it distinguishes the two ways this can fail.
    // No clearance means the WebView was never challenged, so there was no challenge for us
    // to pass and the refusal is aimed at the HTTP client specifically — the case that
    // needs the request to be made from inside the WebView rather than merely wearing its
    // cookie. A clearance that we *did* get and that still 403s is the other case.
    log::warn!("mxb-mods.com never issued a cf_clearance within {WAIT:?}");
    false
}
