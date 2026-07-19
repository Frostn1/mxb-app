use reqwest::cookie::Jar;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

pub const SHOP_BASE: &str = "https://mxbikes-shop.com";

/// Must match the login WebView's User-Agent, or a Cloudflare `cf_clearance` cookie minted there breaks when replayed.
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredSession {
    cookies: Vec<(String, String)>,
}

#[derive(Default)]
pub struct ShopSession {
    client: Mutex<Option<Client>>,
}

impl ShopSession {
    pub fn logged_in(&self) -> bool {
        self.client.lock().unwrap().is_some()
    }

    pub fn client(&self) -> Option<Client> {
        self.client.lock().unwrap().clone()
    }

    fn set_client(&self, client: Option<Client>) {
        *self.client.lock().unwrap() = client;
    }
}

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

pub fn clear_session(app: &AppHandle) {
    let _ = std::fs::remove_file(session_path(app));
    app.state::<ShopSession>().set_client(None);
}

/// Includes HttpOnly cookies (e.g. `wordpress_logged_in_*`) that `document.cookie` can't see.
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

pub fn is_authenticated(cookies: &[(String, String)]) -> bool {
    cookies
        .iter()
        .any(|(name, _)| name.starts_with("wordpress_logged_in"))
}
