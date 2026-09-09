//! The signed-in MX Bikes Shop session. Mechanics live in [`cookie_session`]; this module
//! is the shop's half — where its cookies live, and what "signed in" means.

use crate::cookie_session::{self, Cookies, Site};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

pub const SHOP_BASE: &str = "https://mxbikes-shop.com";

/// The User-Agent the plain-HTTP download clients browse under — see [`crate::install`], which
/// uses it for the mod hosts (MediaFire, Drive) that have nothing to do with this store.
///
/// It is **no longer forced on the login WebView**, which is the one thing it used to be for.
/// Real Chrome never sends a two-part version — [`crate::mxb_session::UA`] documents that shape
/// as a bot-filter signal in its own right — and on macOS the window wearing it is WKWebView,
/// so the claim contradicts every other thing a Cloudflare challenge measures. The window
/// introduces itself honestly now.
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// Only the cookie file and the domain matter now — no HTTP client is built from this any more,
/// so `ua` and `timeout` are inert. [`Site`] requires them.
const SITE: Site = Site {
    base: SHOP_BASE,
    domain: "mxbikes-shop.com",
    file: "shop_session.json",
    ua: UA,
    timeout: Duration::from_secs(120),
};

/// Whether a sign-in has been captured.
///
/// This used to hold a `reqwest` client built from the captured cookies, and that client is
/// gone: every signed-in page on the store is behind a Cloudflare managed challenge, an HTTP
/// client cannot clear one, and a `cf_clearance` earned in a WebView is not portable to it. The
/// pages are read by [`crate::shop_fetch`] out of a real browser, which keeps its own cookies —
/// so all this has to remember is *that* we signed in, not what with.
#[derive(Default)]
pub struct ShopSession(Mutex<bool>);

impl ShopSession {
    pub fn logged_in(&self) -> bool {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set(&self, on: bool) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = on;
    }
}

/// The cookies are still written to disk, and still only as the record that a sign-in happened:
/// the WebView persists its own across restarts, so this file is what lets the app know at
/// startup — without building a window — which half of the Shop tab to show. A file that has
/// outlived the browser's cookies surfaces as the "session expired" the purchases fetch already
/// reports.
pub fn set_session(app: &AppHandle, cookies: Cookies) -> anyhow::Result<()> {
    cookie_session::write(app, &SITE, &cookies)?;
    app.state::<ShopSession>().set(true);
    Ok(())
}

pub fn load_session(app: &AppHandle) {
    let Some(cookies) = cookie_session::read(app, &SITE) else {
        return;
    };
    if !is_authenticated(&cookies) {
        log::info!("a stored MX Bikes Shop session had no login cookie; ignoring it");
        return;
    }
    app.state::<ShopSession>().set(true);
    log::info!("restored MX Bikes Shop session ({} cookies)", cookies.len());
}

/// Forget the sign-in, without touching the browser's cookies.
///
/// For the case where the store itself has told us we are signed out — the purchases page came
/// back as the login form, or as Cloudflare's interstitial. Two things have to happen, and
/// neither used to:
///
///  - The stored marker goes. Otherwise `shop_status` keeps answering `true` from a file that
///    has outlived the browser's cookies, so every visit to the Purchases tab renders as signed
///    in, fails its first read, and drops to signed out — the "logged in for a second" flicker,
///    with no sign-in involved at all.
///  - The hidden window goes, so the next read starts from a fresh navigation instead of the
///    signed-out DOM that produced this.
///
/// What stays is the cookie jar. Those cookies are dead as a session, but the `cf_clearance`
/// among them is not, and dropping it makes the next sign-in pay for a fresh managed challenge
/// — the slowest and least reliable part of the whole flow. [`clear_session`] is the one that
/// clears cookies, because "Log out" has to mean the account is really let go of.
pub fn forget(app: &AppHandle) {
    cookie_session::remove(app, &SITE);
    app.state::<ShopSession>().set(false);
    crate::shop_fetch::close(app);
}

pub fn clear_session(app: &AppHandle) {
    cookie_session::remove(app, &SITE);
    app.state::<ShopSession>().set(false);

    // Dropping our own copy is no longer enough. The signed-in pages are read by a WebView
    // ([`crate::shop_fetch`]) holding the browser's own cookies, so a "Log out" that left those
    // in place would keep answering as the user who signed in — and the next sign-in would land
    // straight back in the same account.
    //
    // Close the window first, then clear. `clear_all_browsing_data` is app-wide rather than
    // per-origin, so it also drops mxb-mods.com's cookies; the only cost there is that
    // [`crate::mxb_fetch`] re-clears its Cloudflare check on next use.
    crate::shop_fetch::close(app);
    if let Some(main) = app.get_webview_window("main") {
        if let Err(e) = main.clear_all_browsing_data() {
            log::warn!("could not clear the WebView's cookies on sign-out: {e}");
        }
    }
}

/// Includes HttpOnly cookies (e.g. `wordpress_logged_in_*`) that `document.cookie` can't see.
pub fn cookies_from_window(window: &WebviewWindow) -> Cookies {
    cookie_session::cookies_from_window(window, &SITE)
}

/// The shop cookies the *browser* is holding, asked through whichever window is to hand.
///
/// The WebView cookie store is shared across the app's windows, so any of them can answer for
/// the shop — which is what lets the sign-in report what was in the jar before it clears it,
/// without needing a window on the shop to exist yet.
pub fn cookies_from_window_any(app: &AppHandle) -> Cookies {
    for label in ["main", crate::shop_fetch::WINDOW] {
        if let Some(win) = app.get_webview_window(label) {
            return cookies_from_window(&win);
        }
    }
    vec![]
}

pub fn is_authenticated(cookies: &[(String, String)]) -> bool {
    cookie_session::has_prefix(cookies, "wordpress_logged_in")
}

/// Which cookies the login window ended up with, by name.
///
/// Names only, never values: a `cf_clearance` is a bearer token and a log file is something
/// users paste into Discord. Same discipline as [`crate::mxb_session`]'s jar summary, and it
/// answers the one question a sign-in that never finished raises — did Cloudflare's challenge
/// ever clear (a `cf_clearance` is present) or did the window sit on the interstitial the
/// whole time (nothing but `__cf_bm`, or nothing at all).
pub fn cookie_names(cookies: &[(String, String)]) -> String {
    if cookies.is_empty() {
        return "none".to_string();
    }
    cookies
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_names_never_carry_values() {
        let cookies = vec![
            ("cf_clearance".to_string(), "supersecretvalue".to_string()),
            ("wordpress_logged_in_9f2".to_string(), "alsosecret".to_string()),
        ];
        let names = cookie_names(&cookies);
        assert_eq!(names, "cf_clearance, wordpress_logged_in_9f2");
        assert!(!names.contains("secret"), "{names}");
        assert_eq!(cookie_names(&[]), "none");
    }
}
