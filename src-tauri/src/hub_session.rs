//! The signed-in MXB Hub session — cookies for `shop.mxb-hub.com`, and the client that uses
//! them.
//!
//! The shape is [`crate::shop_session`]'s, with one large simplification: **MXB Hub is not
//! behind Cloudflare** (measured: `server: nginx`, no `cf-ray`, no interstitial on
//! `/my-account/`). That is what lets this keep a real `reqwest` client built from the
//! captured cookies, which mxbikes-shop.com had to give up — there, every signed-in path is a
//! managed challenge an HTTP client cannot clear, so its pages are read out of a parked
//! WebView instead ([`crate::shop_fetch`]). None of that machinery is needed here: a WebView
//! is opened once, for the user to type their password into, and everything after that is
//! ordinary HTTP.

use crate::cookie_session::{self, Cookies, Site};
use reqwest::cookie::Jar;
use reqwest::Client;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

pub const HUB_BASE: &str = "https://shop.mxb-hub.com";

/// The store is one host — `mxbhub.com` and `mxb-hub.com` both redirect here — so the cookie
/// domain is the full subdomain rather than the registrable one. Scoping it wider would put
/// the session cookie on requests to any other `mxb-hub.com` host we ever add.
pub const HUB_SITE: Site = Site {
    base: HUB_BASE,
    domain: "shop.mxb-hub.com",
    file: "hub_session.json",
    ua: crate::shop_session::UA,
    // Purchased tracks run to hundreds of megabytes; `install::download` streams with this
    // client, so the ceiling has to cover a whole transfer rather than a page load.
    timeout: Duration::from_secs(60 * 30),
};

/// The captured session: the cookies, and the client that carries them.
///
/// Holding the built client rather than rebuilding per call is what keeps the connection pool
/// (and so the TLS handshake) shared between the downloads page read and the file transfer
/// that follows it.
#[derive(Default)]
pub struct HubSession(Mutex<Option<Client>>);

impl HubSession {
    pub fn client(&self) -> Option<Client> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn logged_in(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    fn set(&self, client: Option<Client>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = client;
    }
}

fn build_client(cookies: &Cookies) -> anyhow::Result<Client> {
    let jar = Arc::new(Jar::default());
    cookie_session::fill(&jar, &HUB_SITE, cookies)?;
    Ok(cookie_session::client_builder(&HUB_SITE, jar).build()?)
}

/// Rebuild the signed-in client with a clearance folded in.
///
/// The account half is challenged by the same filter as the catalog, and the two keep separate
/// jars — so a clearance earned for browsing has to be handed over explicitly or every
/// purchases read after one goes on failing. Cookies captured from the WebView are a superset:
/// the login cookie and the clearance arrive together.
pub fn adopt_clearance(app: &AppHandle, cookies: &Cookies) {
    let state = app.state::<HubSession>();
    if !state.logged_in() {
        return;
    }
    let Some(stored) = cookie_session::read(app, &HUB_SITE) else {
        return;
    };
    let mut merged = stored;
    for (name, value) in cookies {
        if let Some(slot) = merged.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value.clone();
        } else {
            merged.push((name.clone(), value.clone()));
        }
    }
    match build_client(&merged) {
        Ok(client) => {
            let _ = cookie_session::write(app, &HUB_SITE, &merged);
            state.set(Some(client));
            log::info!("folded an MXB Hub clearance into the signed-in session");
        }
        Err(e) => log::warn!("could not rebuild the MXB Hub session with a clearance: {e:#}"),
    }
}

pub fn set_session(app: &AppHandle, cookies: Cookies) -> anyhow::Result<()> {
    let client = build_client(&cookies)?;
    cookie_session::write(app, &HUB_SITE, &cookies)?;
    app.state::<HubSession>().set(Some(client));
    Ok(())
}

/// Restore a sign-in captured in an earlier run, if the file still holds a login cookie.
///
/// Whether the *server* still honours it is not knowable without asking, and asking at startup
/// would put a network call on the launch path. A dead cookie surfaces on the first read as the
/// login form, which [`crate::mods::hubaccount`] turns into "sign in again".
pub fn load_session(app: &AppHandle) {
    let Some(cookies) = cookie_session::read(app, &HUB_SITE) else {
        return;
    };
    if !is_authenticated(&cookies) {
        log::info!("a stored MXB Hub session had no login cookie; ignoring it");
        return;
    }
    match build_client(&cookies) {
        Ok(client) => {
            app.state::<HubSession>().set(Some(client));
            log::info!("restored MXB Hub session ({} cookies)", cookies.len());
        }
        Err(e) => log::warn!("could not rebuild the stored MXB Hub session: {e:#}"),
    }
}

/// Drop the session because the *store* said it is over — the downloads page came back as the
/// login form. Nothing is revoked, because there is nothing left to revoke.
pub fn forget(app: &AppHandle) {
    cookie_session::remove(app, &HUB_SITE);
    app.state::<HubSession>().set(None);
}

/// "Log out" as the user means it: end the session on the server too.
///
/// Deliberately **not** `clear_all_browsing_data`, which is what the shop's sign-out uses. That
/// call is app-wide — it would take the mxbikes-shop.com session down with it, and signing out
/// of one store must not sign you out of the other. Calling WooCommerce's own logout URL from
/// our client kills the session token instead, which makes the copy of the cookie still sitting
/// in the WebView's jar inert: the next sign-in lands on the login form, not on the account.
pub async fn clear_session(app: &AppHandle) {
    let client = app.state::<HubSession>().client();
    cookie_session::remove(app, &HUB_SITE);
    app.state::<HubSession>().set(None);

    let Some(client) = client else { return };
    match crate::mods::hubaccount::logout(&client).await {
        Ok(true) => log::info!("signed out of MXB Hub on the server"),
        // The account page didn't offer a logout link — already signed out, most likely.
        Ok(false) => log::info!("MXB Hub had no logout link to follow; dropped the session"),
        Err(e) => log::warn!("could not sign out of MXB Hub on the server: {e:#}"),
    }
}

pub fn cookies_from_window(window: &WebviewWindow) -> Cookies {
    cookie_session::cookies_from_window(window, &HUB_SITE)
}

pub fn is_authenticated(cookies: &[(String, String)]) -> bool {
    cookie_session::has_prefix(cookies, "wordpress_logged_in")
}

/// Which cookies a sign-in ended up with, by name — never by value. Same discipline as
/// [`crate::shop_session::cookie_names`]: a session cookie is a bearer token and a log file is
/// something users paste into Discord.
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
    fn a_wordpress_login_cookie_is_what_counts_as_signed_in() {
        assert!(is_authenticated(&[(
            "wordpress_logged_in_9f2".into(),
            "x".into()
        )]));
        // WooCommerce sets these for anyone who touches a cart, signed in or not.
        assert!(!is_authenticated(&[
            ("woocommerce_items_in_cart".into(), "1".into()),
            ("wp_woocommerce_session_9f2".into(), "x".into()),
        ]));
    }

    #[test]
    fn cookie_names_never_carry_values() {
        let names = cookie_names(&[("wordpress_logged_in_9f2".into(), "supersecret".into())]);
        assert_eq!(names, "wordpress_logged_in_9f2");
        assert_eq!(cookie_names(&[]), "none");
    }

    /// The hub's cookies must never ride along on a request to either of the other two
    /// WordPress sites the app talks to.
    #[test]
    fn the_jar_is_scoped_to_the_hub() {
        use reqwest::cookie::CookieStore;

        let jar = Jar::default();
        cookie_session::fill(&jar, &HUB_SITE, &[("wordpress_logged_in_9f2".into(), "x".into())])
            .unwrap();

        assert!(jar
            .cookies(&"https://shop.mxb-hub.com/my-account/downloads/".parse().unwrap())
            .is_some());
        for other in [
            "https://mxbikes-shop.com/all-my-downloads/",
            "https://mxb-mods.com/wp-json",
        ] {
            assert!(
                jar.cookies(&other.parse().unwrap()).is_none(),
                "hub cookies leaked to {other}"
            );
        }
    }
}
