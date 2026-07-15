//! mxbikes-shop.com authenticated session.
//!
//! The paid shop (WordPress + Easy Digital Downloads) sits behind Cloudflare and
//! only serves a logged-in user the files they've purchased. We never handle the
//! user's password: they sign in through a real WebView window (see `shop_login`
//! in `main.rs`), and we capture the resulting session cookies here to drive an
//! authenticated `reqwest` client for listing ("All My Downloads") and
//! downloading. Cookies are persisted so the session survives app restarts.

use reqwest::cookie::Jar;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

pub const SHOP_BASE: &str = "https://mxbikes-shop.com";

/// Must match the User-Agent set on the login WebView so a Cloudflare
/// `cf_clearance` cookie minted there stays valid when replayed from `reqwest`
/// (Cloudflare binds that cookie to the UA + client IP).
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredSession {
    /// Raw (name, value) cookie pairs captured from the login WebView.
    cookies: Vec<(String, String)>,
}

/// Shared authenticated shop client, held as Tauri managed state.
#[derive(Default)]
pub struct ShopSession {
    client: Mutex<Option<Client>>,
}

impl ShopSession {
    /// Whether we currently hold a session.
    pub fn logged_in(&self) -> bool {
        self.client.lock().unwrap().is_some()
    }

    /// A clone of the authenticated client, if signed in. `reqwest::Client` is
    /// cheap to clone (reference-counted internally) and shares the cookie jar.
    pub fn client(&self) -> Option<Client> {
        self.client.lock().unwrap().clone()
    }

    fn set_client(&self, client: Option<Client>) {
        *self.client.lock().unwrap() = client;
    }
}

/// Build an authenticated client whose cookie jar is seeded with the captured
/// session cookies, scoped to the shop domain.
fn build_client(cookies: &[(String, String)]) -> anyhow::Result<Client> {
    let jar = Arc::new(Jar::default());
    let url: Url = SHOP_BASE.parse()?;
    for (name, value) in cookies {
        jar.add_cookie_str(
            &format!("{name}={value}; Domain=mxbikes-shop.com; Path=/"),
            &url,
        );
    }
    Ok(Client::builder()
        .user_agent(UA)
        .cookie_provider(jar)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()?)
}

fn session_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data dir")
        .join("shop_session.json")
}

/// Persist captured cookies and (re)build the authenticated client in state.
pub fn set_session(app: &AppHandle, cookies: Vec<(String, String)>) -> anyhow::Result<()> {
    let client = build_client(&cookies)?;
    let path = session_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&StoredSession { cookies })?)?;
    app.state::<ShopSession>().set_client(Some(client));
    Ok(())
}

/// Load a previously-saved session on startup (best-effort; ignores errors).
pub fn load_session(app: &AppHandle) {
    let path = session_path(app);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(stored) = serde_json::from_str::<StoredSession>(&text) else {
        return;
    };
    if stored.cookies.is_empty() {
        return;
    }
    match build_client(&stored.cookies) {
        Ok(client) => {
            app.state::<ShopSession>().set_client(Some(client));
            log::info!("restored MX Bikes Shop session ({} cookies)", stored.cookies.len());
        }
        Err(e) => log::warn!("failed to restore shop session: {e:#}"),
    }
}

/// Forget the session (sign out): drop the client and delete the stored cookies.
pub fn clear_session(app: &AppHandle) {
    let _ = std::fs::remove_file(session_path(app));
    app.state::<ShopSession>().set_client(None);
}

/// Read the shop cookies currently held by a login WebView window (includes
/// HttpOnly cookies like `wordpress_logged_in_*`, which `document.cookie` can't).
pub fn cookies_from_window(window: &WebviewWindow) -> Vec<(String, String)> {
    let Ok(url) = SHOP_BASE.parse::<Url>() else {
        return vec![];
    };
    match window.cookies_for_url(url) {
        Ok(list) => list
            .into_iter()
            .map(|c| (c.name().to_string(), c.value().to_string()))
            .collect(),
        Err(_) => vec![],
    }
}

/// Do the captured cookies indicate a logged-in WordPress session?
pub fn is_authenticated(cookies: &[(String, String)]) -> bool {
    cookies
        .iter()
        .any(|(name, _)| name.starts_with("wordpress_logged_in"))
}
