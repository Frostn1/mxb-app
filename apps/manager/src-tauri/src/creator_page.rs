//! The creator's own mxb-mods.com page, shown *behind* the app while a mod is open or
//! installing — so the site keeps the ad impression the app would otherwise strip.
//!
//! MXB App browses the catalog through mxb-mods.com's REST API and installs straight from the
//! mirror, which means a player never loads the mod's own page. mxb-mods.com earns from the
//! ads on that page, so every install through the app is a view the creator used to get and
//! now doesn't. This window puts the real page back in front of a real person: a visible
//! WebView on the true origin, opened when a mod is viewed or installed, and pushed one layer
//! down so it doesn't take over the screen. Real browser, real fingerprint, the site's own
//! ads — the impression restored, not counterfeited. It is never parked hidden and never
//! auto-refreshed; a player who turns the setting off (Settings → General) stops opening it.
//!
//! It is display-only. It runs a remote origin and is granted no capability file, so — unlike
//! the mxb-fetch and shop-fetch windows — its page cannot reach a single app command at all
//! (capabilities target a window by label; nothing targets this one). Nothing is read back
//! out of it, and it only ever navigates to a catalog URL the app itself passed.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The window the creator's page runs in. One, reused: opening another mod navigates this
/// window rather than stacking a second one behind the app.
pub const WINDOW: &str = "creator-page";

/// Only the two catalogs belong here. A mod page links out to its mirror, to an ad network's
/// click-through and to whatever else the creator put on it; refusing anything but an
/// `https://` catalog page means the window can never be steered somewhere the app didn't
/// send it, however its DOM is scripted. The trailing slash pins the host — `mxb-mods.com/`
/// can't be spoofed by `mxb-mods.com.evil.test`.
fn is_catalog_url(url: &str) -> bool {
    const ALLOWED: [&str; 2] = ["https://mxb-mods.com/", "https://gpb-mods.com/"];
    ALLOWED.iter().any(|prefix| url.starts_with(prefix))
}

/// Push the main window back to the front, so the creator page sits behind it — visible, but
/// not in the way of what the player came to do. Best-effort: on a WM that ignores the
/// request the page is merely alongside the app rather than under it, which is still fine.
fn behind_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
        let _ = main.set_focus();
    }
}

/// Open (or, if it's already up, navigate) the creator's page for the mod being viewed or
/// installed. Fails only on a URL that isn't a catalog page; a build/navigate failure is
/// logged and swallowed, because this window supports the creator and must never be able to
/// block the player's own browsing or install.
#[tauri::command]
pub fn open_creator_page(app: AppHandle, url: String) -> Result<(), String> {
    if !is_catalog_url(&url) {
        return Err(format!(
            "refusing to open a non-catalog URL in the creator page: {url}"
        ));
    }
    let target = url
        .parse()
        .map_err(|e| format!("creator page URL didn't parse: {e}"))?;

    // Already open on a different mod: just point it at the new one. Reusing the window keeps
    // this to a single background page no matter how many mods a player clicks through.
    if let Some(win) = app.get_webview_window(WINDOW) {
        if let Err(e) = win.navigate(target) {
            log::warn!("couldn't navigate the creator page to {url}: {e:#}");
        }
        behind_main(&app);
        return Ok(());
    }

    // No `.user_agent()` override, for the reason the shop login window documents: forcing a
    // UA string that doesn't match the actual WebView is itself a signal to Cloudflare. Left
    // alone, the browser introduces itself honestly, which is also what a real ad impression
    // needs. `focused(false)` keeps the keyboard on the app.
    let mut builder = WebviewWindowBuilder::new(&app, WINDOW, WebviewUrl::External(target))
        .title("mxb-mods.com — supporting the creator")
        .inner_size(1100.0, 820.0)
        .focused(false);

    // Offset it down-right of the app so an edge always shows even when the app is maximised.
    // outer_position is physical pixels and the builder wants logical, so divide by the scale
    // factor — otherwise the offset doubles on a HiDPI display.
    if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
        if let (Ok(pos), Ok(scale)) = (main.outer_position(), main.scale_factor()) {
            builder = builder.position(pos.x as f64 / scale + 48.0, pos.y as f64 / scale + 48.0);
        }
    }

    if let Err(e) = builder.build() {
        // Not fatal: the mod still installs, the creator just doesn't get this view.
        log::warn!("couldn't open the creator page for {url}: {e:#}");
        return Ok(());
    }
    behind_main(&app);
    Ok(())
}

/// Close the creator page — called when the player leaves the mod. A no-op if it isn't open.
#[tauri::command]
pub fn close_creator_page(app: AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW) {
        let _ = win.close();
    }
}

#[cfg(test)]
mod tests {
    use super::is_catalog_url;

    #[test]
    fn accepts_both_catalog_hosts() {
        assert!(is_catalog_url("https://mxb-mods.com/tracks/some-track/"));
        assert!(is_catalog_url("https://gpb-mods.com/bikes/some-bike/"));
    }

    #[test]
    fn refuses_everything_else() {
        // A mirror, a look-alike host, plain http, and an off-site link a page might carry.
        assert!(!is_catalog_url("https://www.mediafire.com/file/abc"));
        assert!(!is_catalog_url("https://mxb-mods.com.evil.test/tracks/"));
        assert!(!is_catalog_url("http://mxb-mods.com/tracks/"));
        assert!(!is_catalog_url("https://mxbikes-shop.com/product/x"));
        assert!(!is_catalog_url(""));
    }
}
