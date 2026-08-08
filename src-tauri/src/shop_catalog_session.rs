//! The catalog's half of [`crate::cookie_session`]: a jar, a User-Agent, and the cookies
//! Cloudflare hands out along the way.
//!
//! Separate from [`crate::shop_session`] even though both talk to mxbikes-shop.com, because
//! they are different conversations. That module holds a user's signed-in WordPress cookies
//! for downloading things they bought; this one is the anonymous catalog reader.
//!
//! Keeping them apart matters for one concrete reason: `shop_session::UA` is the two-part
//! `Chrome/126.0` form that [`crate::mxb_session`] documents as a bot-filter red flag, and it
//! can't be corrected — a `cf_clearance` is bound to the User-Agent that earned it, so
//! changing that constant would invalidate every stored login in the field. This module gets
//! the four-part UA and its own jar instead, and nobody has to choose.
//!
//! **There is deliberately no clearance handshake here.** One was written and then removed,
//! because it was measured and does not work: the WebView earned a `cf_clearance` in about a
//! second, and requests carrying that cookie were still refused — at the site root as well as
//! the API. A clearance is bound to the TLS fingerprint that earned it, so no HTTP client can
//! borrow one. [`crate::mxb_fetch`] is the shape that does work (move the request into the
//! browser rather than the cookie out of it); if the store ever withdraws the WAF exemption
//! that currently makes the catalog reachable, that is the pattern to copy — not this one.
//!
//! What remains is still worth having: `__cf_bm` and friends persist across runs and feed the
//! bot score, so a returning user arrives looking less like a stranger.

use crate::cookie_session::{self, Site};
use reqwest::cookie::Jar;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::AppHandle;

pub const BASE: &str = "https://mxbikes-shop.com";

/// The same four-part version [`crate::mxb_session::UA`] uses, and for the same reason: a
/// User-Agent no real browser would send is itself a signal to a bot filter.
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

/// Process-wide, because the catalog client is built once and must see cookies that arrive
/// later — `Jar` is internally synchronised, so filling it after the fact is visible to the
/// client already holding it.
pub fn jar() -> Arc<Jar> {
    static JAR: OnceLock<Arc<Jar>> = OnceLock::new();
    JAR.get_or_init(|| Arc::new(Jar::default())).clone()
}

pub fn client_builder() -> reqwest::ClientBuilder {
    cookie_session::client_builder(&SITE, jar())
}

/// Replay last run's cookies, so a returning user doesn't arrive at the bot filter cold.
/// Expired ones are harmless: they fail exactly like no cookie.
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
