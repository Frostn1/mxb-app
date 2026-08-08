//! The mxb-mods.com side of [`cookie_session`]: the identity the HTTP client browses under.
//!
//! Browsing is anonymous — there is nothing to sign into here. The only cookie that ever
//! mattered is Cloudflare's `cf_clearance`, and this module used to open a WebView, let the
//! challenge clear, and replay the cookie it earned into the reqwest client.
//!
//! That no longer exists, because a tester's log proved it cannot work: the challenge cleared
//! in about a second, the cookie was sent correctly, and Cloudflare served the interstitial to
//! reqwest anyway. A `cf_clearance` is bound to the TLS/HTTP2 fingerprint of the connection
//! that earned it, and rustls does not fingerprint like the WebView's Chrome — so the cookie
//! is not portable between them. The answer is to move the *request* into the browser instead;
//! see [`crate::mxb_fetch`].
//!
//! What remains is the jar the client browses with — ordinary `Set-Cookie` handling, plus
//! anything an older version left on disk — and the UA the bridge is built with, kept here so
//! the two cannot drift.

use crate::cookie_session::{self, Site};
use reqwest::cookie::Jar;
use reqwest::Url;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::AppHandle;

pub const BASE: &str = "https://mxb-mods.com";

/// A full four-part version, unlike the `Chrome/126.0` form [`crate::shop_session::UA`]
/// uses — real Chrome never sends a two-part version, and a UA no browser would emit is
/// itself a signal to a bot filter. [`crate::mxb_fetch`]'s WebView is built with this same
/// constant, so the HTTP client and the browser present as the same visitor.
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.6778.140 Safari/537.36";

const SITE: Site = Site {
    base: BASE,
    domain: "mxb-mods.com",
    file: "mxb_session.json",
    ua: UA,
    timeout: Duration::from_secs(30),
};

/// Process-wide, because the mods client is built once and must see cookies that arrive
/// long afterwards. `Jar` is internally synchronised, so filling it after the fact is
/// visible to the client already holding it.
pub fn jar() -> Arc<Jar> {
    static JAR: OnceLock<Arc<Jar>> = OnceLock::new();
    JAR.get_or_init(|| Arc::new(Jar::default())).clone()
}

/// The mods client is built from this so its User-Agent, jar and timeouts cannot drift from
/// the fetch WebView's. Callers add their own default headers.
pub fn client_builder() -> reqwest::ClientBuilder {
    cookie_session::client_builder(&SITE, jar())
}

/// Which cookies the client would actually send to mxb-mods.com, by name.
///
/// Names only, never values: a `cf_clearance` is a bearer token, and a log file is something
/// users paste into Discord. The names alone answer the question a 403 raises — did we send a
/// clearance and get refused anyway, or did we never have one to send.
pub fn jar_summary() -> String {
    match BASE.parse() {
        Ok(url) => summarize(&jar(), &url),
        Err(_) => "unknown".to_string(),
    }
}

/// Split out from [`jar_summary`] so it can be tested against a fresh jar rather than the
/// process-wide one.
fn summarize(jar: &Jar, url: &Url) -> String {
    use reqwest::cookie::CookieStore;

    let Some(header) = jar.cookies(url) else {
        return "none".to_string();
    };
    let Ok(header) = header.to_str() else {
        return "unreadable".to_string();
    };
    let names: Vec<&str> = header
        .split(';')
        .filter_map(|c| c.split('=').next())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return "none".to_string();
    }
    let cleared = if names.contains(&"cf_clearance") {
        ""
    } else {
        " (no cf_clearance)"
    };
    format!("{}{cleared}", names.join(", "))
}

/// Replay whatever an earlier version of the app left on disk. Nothing writes this file any
/// more — the handshake that used to fill it is gone — but an existing clearance still costs
/// nothing to send, and an expired one fails exactly like no cookie.
pub fn load(app: &AppHandle) {
    let Some(cookies) = cookie_session::read(app, &SITE) else {
        return;
    };
    if let Err(e) = cookie_session::fill(&jar(), &SITE, &cookies) {
        log::warn!("failed to restore mxb-mods.com cookies: {e:#}");
        return;
    }
    // Whether a clearance came back matters more than how many cookies did — it is the first
    // thing to check against a refusal line further down the log.
    log::info!(
        "restored mxb-mods.com cookies ({}, {})",
        cookies.len(),
        if cookie_session::has(&cookies, "cf_clearance") {
            "including a cf_clearance"
        } else {
            "no cf_clearance"
        }
    );
}


#[cfg(test)]
mod tests {
    use super::*;

    fn url() -> Url {
        BASE.parse().unwrap()
    }

    /// The whole point of the summary: a user's log should say which cookies went out
    /// without handing anyone the clearance itself, which is a bearer token.
    #[test]
    fn summarize_reports_names_and_never_values() {
        let jar = Jar::default();
        cookie_session::fill(
            &jar,
            &SITE,
            &[("cf_clearance".into(), "supersecretvalue".into())],
        )
        .unwrap();

        let summary = summarize(&jar, &url());
        assert!(summary.contains("cf_clearance"), "{summary}");
        assert!(!summary.contains("supersecretvalue"), "{summary}");
    }

    /// The distinction a 403 hangs on — we sent a clearance and were refused anyway, or we
    /// never had one. The summary has to make that readable at a glance.
    #[test]
    fn summarize_calls_out_a_missing_clearance() {
        let jar = Jar::default();
        cookie_session::fill(&jar, &SITE, &[("__cf_bm".into(), "x".into())]).unwrap();
        assert!(summarize(&jar, &url()).contains("no cf_clearance"));

        cookie_session::fill(&jar, &SITE, &[("cf_clearance".into(), "y".into())]).unwrap();
        assert!(!summarize(&jar, &url()).contains("no cf_clearance"));
    }

    #[test]
    fn summarize_says_none_for_an_empty_jar() {
        assert_eq!(summarize(&Jar::default(), &url()), "none");
    }
}
