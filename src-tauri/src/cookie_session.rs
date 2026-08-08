//! Persisted cookie jars for the Cloudflare-fronted WordPress sites the app talks to.
//!
//! `shop_session` and `mxb_session` want the same mechanics and differ only in the domain,
//! the file they persist to, and how patient a request may be: cookies minted by a real
//! WebView, replayed by a reqwest client whose User-Agent matches that WebView, and kept on
//! disk so a restart doesn't arrive at the bot filter cold.

use reqwest::cookie::Jar;
use reqwest::{Client, ClientBuilder, Url};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

/// One of the sites we keep a jar for.
///
/// `ua` is not cosmetic: Cloudflare ties a `cf_clearance` to the User-Agent that earned it,
/// so the WebView we harvest from and the client that replays the cookie have to agree, or
/// the cookie fails exactly as if we'd sent none.
pub struct Site {
    pub base: &'static str,
    pub domain: &'static str,
    pub file: &'static str,
    pub ua: &'static str,
    pub timeout: Duration,
}

pub type Cookies = Vec<(String, String)>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Stored {
    cookies: Cookies,
}

/// A client, once a site has one. Both sites hold theirs in Tauri managed state.
#[derive(Default)]
pub struct Store {
    client: Mutex<Option<Client>>,
}

impl Store {
    pub fn client(&self) -> Option<Client> {
        self.client.lock().unwrap().clone()
    }

    pub fn is_set(&self) -> bool {
        self.client.lock().unwrap().is_some()
    }

    pub fn set(&self, client: Option<Client>) {
        *self.client.lock().unwrap() = client;
    }
}

/// Add `cookies` to `jar` as if the site had set them.
pub fn fill(jar: &Jar, site: &Site, cookies: &[(String, String)]) -> anyhow::Result<()> {
    let url: Url = site.base.parse()?;
    for (name, value) in cookies {
        jar.add_cookie_str(
            &format!("{name}={value}; Domain={}; Path=/", site.domain),
            &url,
        );
    }
    Ok(())
}

/// A builder rather than a `Client`, so a caller can add its own default headers before
/// building. Everything common to both sites is already applied.
pub fn client_builder(site: &Site, jar: Arc<Jar>) -> ClientBuilder {
    Client::builder()
        .user_agent(site.ua)
        .cookie_provider(jar)
        .connect_timeout(Duration::from_secs(15))
        .timeout(site.timeout)
}

fn path(app: &AppHandle, site: &Site) -> anyhow::Result<std::path::PathBuf> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow::anyhow!("could not resolve app local data dir: {e}"))?
        .join(site.file))
}

pub fn write(app: &AppHandle, site: &Site, cookies: &Cookies) -> anyhow::Result<()> {
    let path = path(app, site)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&Stored {
            cookies: cookies.clone(),
        })?,
    )?;
    Ok(())
}

/// `None` when there is nothing usable on disk — missing, unreadable, or empty.
pub fn read(app: &AppHandle, site: &Site) -> Option<Cookies> {
    let text = std::fs::read_to_string(path(app, site).ok()?).ok()?;
    let stored: Stored = serde_json::from_str(&text).ok()?;
    (!stored.cookies.is_empty()).then_some(stored.cookies)
}

pub fn remove(app: &AppHandle, site: &Site) {
    if let Ok(path) = path(app, site) {
        let _ = std::fs::remove_file(path);
    }
}

/// Includes HttpOnly cookies — `wordpress_logged_in_*` and `cf_clearance` alike — which
/// `document.cookie` cannot see.
pub fn cookies_from_window(window: &WebviewWindow, site: &Site) -> Cookies {
    let Ok(url) = site.base.parse::<Url>() else {
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

pub fn has(cookies: &[(String, String)], name: &str) -> bool {
    cookies.iter().any(|(n, _)| n == name)
}

pub fn has_prefix(cookies: &[(String, String)], prefix: &str) -> bool {
    cookies.iter().any(|(n, _)| n.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: Site = Site {
        base: "https://example.com",
        domain: "example.com",
        file: "example.json",
        ua: "test-ua",
        timeout: Duration::from_secs(30),
    };

    /// The jar is what the client actually sends, so "did `fill` work" is really "does the
    /// cookie come back out for a URL on that site".
    #[test]
    fn fill_puts_cookies_where_the_client_will_find_them() {
        use reqwest::cookie::CookieStore;

        let jar = Jar::default();
        fill(
            &jar,
            &SITE,
            &[("cf_clearance".into(), "abc123".into())],
        )
        .unwrap();

        let header = jar
            .cookies(&"https://example.com/wp-json/wp/v2/posts".parse().unwrap())
            .expect("jar should hand out a cookie header");
        assert!(header.to_str().unwrap().contains("cf_clearance=abc123"));
    }

    /// A cookie for one site must never ride along on a request to the other.
    #[test]
    fn fill_scopes_cookies_to_the_site() {
        use reqwest::cookie::CookieStore;

        let jar = Jar::default();
        fill(&jar, &SITE, &[("cf_clearance".into(), "abc123".into())]).unwrap();

        assert!(jar
            .cookies(&"https://mxb-mods.com/wp-json".parse().unwrap())
            .is_none());
    }

    #[test]
    fn has_and_has_prefix_distinguish_exact_from_family() {
        let cookies = vec![
            ("cf_clearance".to_string(), "x".to_string()),
            ("wordpress_logged_in_9f2".to_string(), "y".to_string()),
        ];
        assert!(has(&cookies, "cf_clearance"));
        assert!(!has(&cookies, "wordpress_logged_in"));
        assert!(has_prefix(&cookies, "wordpress_logged_in"));
    }
}
