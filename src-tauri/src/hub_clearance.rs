//! Getting past MXB Hub's robot challenge, by running it in a real browser.
//!
//! `shop.mxb-hub.com` is on SiteGround, whose bot protection fires on **request rate** rather
//! than on anything about the client. That is why it is invisible to a handful of probes and
//! entirely reproducible the moment a grid asks for a page of twenty-four thumbnails: the
//! store starts answering every path — API and images alike — with a `202` carrying a "Robot
//! Challenge Screen", and keeps doing so until something solves it. Solving it means computing
//! a proof of work in a Web Worker and posting it back, so an HTTP client cannot; a browser
//! can, and gets a cookie for its trouble.
//!
//! So: park a hidden window on the store, let it do that, and hand its cookies to the clients.
//!
//! Deliberately much smaller than [`crate::mxb_fetch`] and [`crate::shop_fetch`], which solve
//! the same shape of problem for the two Cloudflare-fronted sites. Those two have to *read
//! pages* out of the browser, so they inject a script, take a result back over IPC, and pay
//! for that with a capability file granting a remote origin the right to talk to us. Nothing
//! here does: the only thing taken from this window is its cookie jar, which Rust reads from
//! the outside. The page is never given a way to call the app, so there is no IPC surface to
//! get wrong.

use crate::hub_session::{HUB_BASE, HUB_SITE};
use crate::{cookie_session, mods};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The window's label. Transient — it must be destroyed on close, never parked in the tray,
/// or its label stays registered and the next handshake silently cannot build one.
pub const WINDOW: &str = "hub-clearance";

/// How long the browser gets to run the challenge. The proof of work is a second or two on a
/// modern machine; the rest is page load, and being generous costs nothing when it succeeds.
const SOLVE_TIMEOUT: Duration = Duration::from_secs(40);

const POLL: Duration = Duration::from_millis(500);

/// Unix seconds of the last successful handshake, or 0.
///
/// Not a "we are cleared" flag: the clearance can lapse at any time and only the store knows.
/// It is a throttle. Without it, a page whose twenty-four thumbnails are all being refused
/// would queue twenty-four handshakes, which is both pointless and precisely the request storm
/// that got us challenged.
static LAST: AtomicU64 = AtomicU64::new(0);

/// Don't hand out a second handshake within this of the last one succeeding.
const REUSE_WITHIN: u64 = 20;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Run the challenge in a hidden window and give the result to both hub clients.
///
/// One at a time, and cheap to call again: a caller that lost the race for the lock finds the
/// work already done and returns without opening anything.
pub async fn earn(app: &AppHandle) -> anyhow::Result<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    if now().saturating_sub(LAST.load(Ordering::Relaxed)) < REUSE_WITHIN {
        log::info!("an MXB Hub clearance was just earned; reusing it");
        return Ok(());
    }

    // A window left over from a previous attempt is on a page that has already been decided
    // one way or the other, so it is dropped rather than reused: the point is a fresh
    // navigation, which is what re-serves the challenge and lets the browser answer it.
    close(app);

    let url: tauri::Url = HUB_BASE.parse()?;
    log::info!("opening the hidden MXB Hub window to answer the robot challenge");
    let window = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title("MXB Hub")
        // Never shown, and never given a way to talk to the app — see the module comment.
        .visible(false)
        .inner_size(1.0, 1.0)
        .position(-32000.0, -32000.0)
        .skip_taskbar(true)
        .decorations(false)
        .focused(false)
        .build()?;

    let deadline = std::time::Instant::now() + SOLVE_TIMEOUT;
    let mut last_seen = Vec::new();
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(POLL).await;

        let cookies = cookie_session::cookies_from_window(&window, &HUB_SITE);
        if cookies.is_empty() {
            continue;
        }
        // What "solved" means is asked of the store, not guessed from a cookie name. The
        // challenge sets more than one cookie and renames them between SiteGround versions, so
        // matching on a name is a check that silently stops working; a request that comes back
        // as the thing we asked for cannot.
        if cookies != last_seen {
            last_seen = cookies.clone();
            mods::hub::adopt_clearance(&cookies)?;
            if probe().await {
                crate::hub_session::adopt_clearance(app, &cookies);
                LAST.store(now(), Ordering::Relaxed);
                log::info!(
                    "MXB Hub clearance earned ({})",
                    crate::hub_session::cookie_names(&cookies)
                );
                close(app);
                return Ok(());
            }
        }
    }

    close(app);
    log::warn!(
        "the MXB Hub challenge was not answered within {}s (cookies: {})",
        SOLVE_TIMEOUT.as_secs(),
        crate::hub_session::cookie_names(&last_seen)
    );
    anyhow::bail!("MXB Hub is still asking the app to prove it isn't a robot. Try again shortly.")
}

/// The cheapest request the store answers, used only to ask "are we still challenged?".
///
/// One product, and no parsing: all that matters is whether what came back is the challenge.
async fn probe() -> bool {
    let Ok(client) = mods::hub::client() else {
        return false;
    };
    match client
        .get(format!("{HUB_BASE}/wp-json/wc/store/v1/products?per_page=1"))
        .send()
        .await
    {
        Ok(resp) => !mods::hub::challenged(&resp) && resp.status().is_success(),
        Err(_) => false,
    }
}

pub fn close(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW) {
        let _ = win.close();
    }
}
