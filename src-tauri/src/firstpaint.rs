//! Whether the main window has ever painted, and the rescue for when it hasn't.
//!
//! Every window control is drawn by the frontend (`decorations: false`), so a webview that
//! never produces a first frame leaves an OS window with no way to close it.
//!
//! Two signals, because they are two different facts. [`loaded`] — the document parsed —
//! is what puts the window on screen. [`mark`] — a frame actually reached the glass — is
//! what says the title bar in it is real, and it can only arrive *after* the window is
//! visible, since a hidden webview doesn't composite and never runs its frame callbacks.
//! A window that goes quiet between the two is what [`arm`] rescues.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// How long the frontend gets before the window is rescued. A healthy start paints in a
/// second or two; this is sized for a cold boot with the disk still thrashing.
const GRACE: Duration = Duration::from_secs(12);

/// Whether the window is missing a title bar until the frontend draws one. macOS keeps a
/// native one at all times (`tauri.macos.conf.json`), so there is nothing to add there —
/// and taking it away again afterwards would break the window the rescue is meant to save.
const NEEDS_A_TITLE_BAR: bool = !cfg!(target_os = "macos");

/// Never cleared once set: a webview that painted and later broke still left our title bar
/// on screen, which is a different problem from this one.
static PAINTED: AtomicBool = AtomicBool::new(false);

/// Set while the window is wearing the native title bar it was rescued with.
static DECORATED: AtomicBool = AtomicBool::new(false);

pub fn painted() -> bool {
    PAINTED.load(Ordering::SeqCst)
}

/// The document finished loading — put the window on screen, still undecorated because the
/// frontend's own title bar is a frame away. Reloads land here again and are ignored.
pub fn loaded(app: &AppHandle) {
    if painted() {
        return;
    }
    let Some(w) = app.get_webview_window(crate::MAIN_WINDOW) else {
        return;
    };
    let _ = w.show();
    let _ = w.set_focus();
}

/// A frame reached the glass. Reloads land here again and are ignored.
pub fn mark(app: &AppHandle) {
    if PAINTED.swap(true, Ordering::SeqCst) {
        return;
    }
    log::info!("[startup] the main window painted");
    // It arrived late, after the watchdog had already dressed the window in a native title
    // bar. Take that back off now there is one drawn in the page again.
    if DECORATED.swap(false, Ordering::SeqCst) {
        if let Some(w) = app.get_webview_window(crate::MAIN_WINDOW) {
            let _ = w.set_decorations(false);
        }
    }
}

/// Give the window a native title bar, because nothing has drawn one in it.
///
/// Called from every path that puts the window on screen before it has painted — the
/// watchdog below, the tray, a second launch — so none of them can produce a window the
/// player has no way to close.
pub fn decorate_unpainted(window: &tauri::WebviewWindow) {
    if !NEEDS_A_TITLE_BAR || painted() || DECORATED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = window.set_decorations(true);
}

/// Start the watchdog. Call once, right after the main window is built.
///
/// On its own OS thread rather than the async runtime: it has to survive a startup that
/// has gone wrong, and a runtime that isn't scheduling is one of the ways it can go wrong.
pub fn arm(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(GRACE);
        if painted() {
            return;
        }
        log::warn!(
            "[startup] the main window hasn't painted after {}s — showing it with a native \
             title bar so it can be closed",
            GRACE.as_secs()
        );
        crate::show_main(&app);
    });
}
