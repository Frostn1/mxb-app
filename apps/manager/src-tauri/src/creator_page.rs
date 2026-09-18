//! The creator's own mxb-mods.com page, shown beside the app (or behind it, if the player
//! prefers) while a mod is open or installing — so the site keeps the ad impression the app
//! would otherwise strip.
//!
//! MXB App browses the catalog through mxb-mods.com's REST API and installs straight from the
//! mirror, which means a player never loads the mod's own page. mxb-mods.com earns from the
//! ads on that page, so every install through the app is a view the creator used to get and
//! now doesn't. This window puts the real page back in front of a real person: a visible
//! WebView on the true origin, opened when a mod is viewed or installed. Beside the app by
//! default, so the whole page is on screen — which is what an ad network actually counts as a
//! view; a player who'd rather it stayed out of the way can tuck it behind the app instead,
//! at the cost of that revenue. Real browser, real fingerprint, the site's own ads — the
//! impression restored, not counterfeited. It is never parked hidden and never auto-refreshed;
//! a player who turns the setting off (Settings → General) stops opening it.
//!
//! It is display-only. It runs a remote origin and is granted no capability file, so — unlike
//! the mxb-fetch and shop-fetch windows — its page cannot reach a single app command at all
//! (capabilities target a window by label; nothing targets this one). Nothing is read back
//! out of it, and it only ever navigates to a catalog URL the app itself passed.

use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

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

/// Keep the keyboard on the app after the creator page opens. Beside the app that just means
/// the player keeps typing where they were; behind it, focusing the app is also what pushes
/// the page under it. Best-effort — a WM that ignores the request leaves the page alongside,
/// which is still fine.
fn keep_app_focused(app: &AppHandle) {
    if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
        let _ = main.set_focus();
    }
}

/// Where to put the creator page, in logical pixels, relative to the main window.
///
/// Beside (the default): just off the app's right edge, so the whole page — ads and all — is
/// on screen, which is what an ad network actually counts as a view. Behind: overlapping the
/// app, down and right a little so an edge still shows; [`keep_app_focused`] then drops it
/// under the app. outer_position/outer_size are physical pixels and the window API wants
/// logical, so both are divided by the scale factor — otherwise the offset doubles on HiDPI.
fn target_pos(app: &AppHandle, behind: bool) -> Option<(f64, f64)> {
    let main = app.get_webview_window(crate::MAIN_WINDOW)?;
    let pos = main.outer_position().ok()?;
    let scale = main.scale_factor().ok()?;
    let x0 = pos.x as f64 / scale;
    let y0 = pos.y as f64 / scale;
    if behind {
        Some((x0 + 48.0, y0 + 48.0))
    } else {
        let width = main
            .outer_size()
            .ok()
            .map_or(0.0, |s| s.width as f64 / scale);
        Some((x0 + width + 24.0, y0))
    }
}

/// Open (or, if it's already up, navigate) the creator's page for the mod being viewed or
/// installed. Fails only on a URL that isn't a catalog page; a build/navigate failure is
/// logged and swallowed, because this window supports the creator and must never be able to
/// block the player's own browsing or install.
#[tauri::command]
pub fn open_creator_page(app: AppHandle, url: String, placement: String) -> Result<(), String> {
    if !is_catalog_url(&url) {
        return Err(format!(
            "refusing to open a non-catalog URL in the creator page: {url}"
        ));
    }
    let target = url
        .parse()
        .map_err(|e| format!("creator page URL didn't parse: {e}"))?;
    // Anything but the explicit "behind" is beside the app — the default, and where the page
    // is actually visible.
    let behind = placement == "behind";

    // Already open on a different mod: point it at the new one and re-apply placement (the
    // player may have changed the setting since). Reusing the window keeps this to a single
    // background page no matter how many mods a player clicks through.
    if let Some(win) = app.get_webview_window(WINDOW) {
        if let Err(e) = win.navigate(target) {
            log::warn!("couldn't navigate the creator page to {url}: {e:#}");
        }
        if let Some((x, y)) = target_pos(&app, behind) {
            let _ = win.set_position(LogicalPosition::new(x, y));
        }
        keep_app_focused(&app);
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

    if let Some((x, y)) = target_pos(&app, behind) {
        builder = builder.position(x, y);
    }

    if let Err(e) = builder.build() {
        // Not fatal: the mod still installs, the creator just doesn't get this view.
        log::warn!("couldn't open the creator page for {url}: {e:#}");
        return Ok(());
    }
    keep_app_focused(&app);
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
