//! The catalog's half of [`crate::cookie_session`], plus the WebView handshake that fills it.
//!
//! Separate from [`crate::shop_session`] even though both talk to mxbikes-shop.com, because
//! they are different conversations with different requirements. That module holds a user's
//! signed-in WordPress cookies for downloading things they bought; this one holds nothing but
//! a Cloudflare `cf_clearance` for reading a public JSON document.
//!
//! Keeping them apart matters for one concrete reason: `shop_session::UA` is the two-part
//! `Chrome/126.0` form that [`crate::mxb_session`] documents as a bot-filter red flag, and it
//! can't be corrected — a `cf_clearance` is bound to the User-Agent that earned it, so
//! changing that constant would invalidate every stored login in the field. This module gets
//! the four-part UA and its own jar instead, and nobody has to choose.
//!
//! A caveat worth knowing: the catalog endpoint currently answers with Cloudflare's *firewall*
//! page rather than a challenge, and a firewall rule is not something a clearance cookie can
//! satisfy. The handshake is here because it costs one window to try and it fixes the
//! challenge case; it is not a promise that every refusal is clearable.

use crate::cookie_session::{self, Cookies, Site};
use reqwest::cookie::Jar;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const BASE: &str = "https://mxbikes-shop.com";

/// The same four-part version [`crate::mxb_session::UA`] uses, and for the same reason: a
/// User-Agent no real browser would send is itself a signal to a bot filter. The handshake
/// WebView below is built with this constant, so it cannot drift from the HTTP client's.
pub const UA: &str = crate::mxb_session::UA;

pub const SITE: Site = Site {
    base: BASE,
    domain: "mxbikes-shop.com",
    file: "shop_catalog_session.json",
    ua: UA,
    // The catalog is one large document rather than many small calls, so it gets more room
    // than a REST page would.
    timeout: Duration::from_secs(60),
};

const WINDOW: &str = "shop-catalog-clearance";

/// Half of [`crate::mxb_session`]'s budget, deliberately.
///
/// Over there a challenge is genuinely likely, so waiting two minutes for a slow one is worth
/// it. Here the challenge sits on the catalog *path*, which a browser can't reach without the
/// API header — so the WebView lands on the site root, is usually not challenged at all, and
/// there is nothing to wait for. Sixty seconds still covers a real challenge on the root
/// without leaving a pointless window in someone's face for two minutes.
///
/// The proper fix is on the store's side: exempting the catalog path from the challenge
/// removes this path entirely. See the module docs.
const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(500);

/// Process-wide, because the catalog client is built once and must see cookies that arrive
/// long afterwards — `Jar` is internally synchronised, so filling it after the fact is
/// visible to the client already holding it.
pub fn jar() -> Arc<Jar> {
    static JAR: OnceLock<Arc<Jar>> = OnceLock::new();
    JAR.get_or_init(|| Arc::new(Jar::default())).clone()
}

pub fn client_builder() -> reqwest::ClientBuilder {
    cookie_session::client_builder(&SITE, jar())
}

/// Bumped whenever fresh cookies land, so a caller that queued behind another handshake can
/// tell it has something new to retry with instead of opening a second window.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn store(app: &AppHandle, cookies: Cookies) {
    if let Err(e) = cookie_session::fill(&jar(), &SITE, &cookies) {
        log::warn!("could not add mxbikes-shop.com cookies to the catalog jar: {e:#}");
        return;
    }
    if let Err(e) = cookie_session::write(app, &SITE, &cookies) {
        // Not fatal — the jar is filled, so this session works; only the next starts cold.
        log::warn!("could not persist mxbikes-shop.com catalog cookies: {e:#}");
    }
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Replay last run's clearance so a user blocked yesterday isn't blocked again on launch.
/// An expired cookie is harmless: it fails exactly like no cookie, and the refusal path
/// mints a new one.
pub fn load(app: &AppHandle) {
    let Some(cookies) = cookie_session::read(app, &SITE) else {
        return;
    };
    if let Err(e) = cookie_session::fill(&jar(), &SITE, &cookies) {
        log::warn!("failed to restore mxbikes-shop.com catalog cookies: {e:#}");
        return;
    }
    log::info!("restored mxbikes-shop.com catalog cookies ({})", cookies.len());
}

/// Open a real browser at the store, wait for Cloudflare to hand it a `cf_clearance`, and
/// keep the cookie. `true` if we came away with one.
///
/// Serialised, so two commands refused at the same moment don't open two windows.
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
            log::error!("mxbikes-shop.com base URL is not a URL: {e}");
            return false;
        }
    };
    let window = match WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title("Checking with mxbikes-shop.com…")
        .user_agent(UA)
        .inner_size(480.0, 560.0)
        .build()
    {
        Ok(window) => window,
        Err(e) => {
            log::error!("could not open the mxbikes-shop.com check window: {e:#}");
            return false;
        }
    };

    let attempts = WAIT.as_millis() / POLL.as_millis();
    let mut last: Cookies = vec![];
    for _ in 0..attempts {
        tokio::time::sleep(POLL).await;
        let Some(win) = app.get_webview_window(WINDOW) else {
            log::info!("mxbikes-shop.com check window closed before it cleared");
            return false;
        };
        last = cookie_session::cookies_from_window(&win, &SITE);
        if cookie_session::has(&last, "cf_clearance") {
            store(app, last);
            let _ = win.close();
            log::info!("earned a cf_clearance for mxbikes-shop.com");
            return true;
        }
    }

    let _ = window.close();
    // No clearance, but `__cf_bm` and friends still feed the bot score, so keep what we got.
    if !last.is_empty() {
        store(app, last);
    }
    log::warn!("mxbikes-shop.com never issued a cf_clearance within {WAIT:?}");
    false
}
