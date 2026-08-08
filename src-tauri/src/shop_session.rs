//! The signed-in MX Bikes Shop session. Mechanics live in [`cookie_session`]; this module
//! is the shop's half — where its cookies live, and what "signed in" means.

use crate::cookie_session::{self, Cookies, Site, Store};
use reqwest::cookie::Jar;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

pub const SHOP_BASE: &str = "https://mxbikes-shop.com";

/// Must match the login WebView's User-Agent, or a Cloudflare `cf_clearance` cookie minted
/// there breaks when replayed.
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// Downloads run through this client, hence the patient timeout.
const SITE: Site = Site {
    base: SHOP_BASE,
    domain: "mxbikes-shop.com",
    file: "shop_session.json",
    ua: UA,
    timeout: Duration::from_secs(120),
};

#[derive(Default)]
pub struct ShopSession(Store);

impl ShopSession {
    pub fn logged_in(&self) -> bool {
        self.0.is_set()
    }

    pub fn client(&self) -> Option<Client> {
        self.0.client()
    }
}

fn build_client(cookies: &[(String, String)]) -> anyhow::Result<Client> {
    let jar = Arc::new(Jar::default());
    cookie_session::fill(&jar, &SITE, cookies)?;
    Ok(cookie_session::client_builder(&SITE, jar).build()?)
}

pub fn set_session(app: &AppHandle, cookies: Cookies) -> anyhow::Result<()> {
    let client = build_client(&cookies)?;
    cookie_session::write(app, &SITE, &cookies)?;
    app.state::<ShopSession>().0.set(Some(client));
    Ok(())
}

pub fn load_session(app: &AppHandle) {
    let Some(cookies) = cookie_session::read(app, &SITE) else {
        return;
    };
    match build_client(&cookies) {
        Ok(client) => {
            app.state::<ShopSession>().0.set(Some(client));
            log::info!("restored MX Bikes Shop session ({} cookies)", cookies.len());
        }
        Err(e) => log::warn!("failed to restore shop session: {e:#}"),
    }
}

pub fn clear_session(app: &AppHandle) {
    cookie_session::remove(app, &SITE);
    app.state::<ShopSession>().0.set(None);
}

/// Includes HttpOnly cookies (e.g. `wordpress_logged_in_*`) that `document.cookie` can't see.
pub fn cookies_from_window(window: &WebviewWindow) -> Cookies {
    cookie_session::cookies_from_window(window, &SITE)
}

pub fn is_authenticated(cookies: &[(String, String)]) -> bool {
    cookie_session::has_prefix(cookies, "wordpress_logged_in")
}
