//! The in-game overlay: a frameless, always-on-top window summoned by a global hotkey
//! so presets and swaps can be changed without alt-tabbing out of MX Bikes.
//!
//! It's a second webview onto the same bundle (`index.html?overlay=1`), which is what
//! lets it reuse the Presets / Locker / Browse UI verbatim. The payoff is the live-apply
//! path that already exists: [`crate::gameproc::refresh_look`] pushes a look change into
//! the running game, so a preset picked here shows up without leaving the session.
//!
//! What it deliberately does *not* do is draw inside the game's swapchain. Exclusive
//! fullscreen therefore covers it — see [`fullscreen_hint`].

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::config::{self, DEFAULT_OVERLAY_HOTKEY};
use crate::gameproc;

/// Window label. Also the label the overlay capability grants permissions to.
pub const LABEL: &str = "overlay";

/// Frontend entry point — `main.tsx` branches on the `overlay` query flag.
const URL: &str = "index.html?overlay=1";

const WIDTH: f64 = 1100.0;
const HEIGHT: f64 = 720.0;

/// Emitted to the main window when the overlay is summoned but the game owns the
/// screen exclusively, so the player finds out why nothing appeared.
const FULLSCREEN_EVENT: &str = "overlay-fullscreen-blocked";

/// What the Settings panel needs to describe the overlay's current state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayState {
    pub enabled: bool,
    pub hotkey: String,
    pub game_running: bool,
    /// A DirectX app is holding the screen exclusively right now.
    pub fullscreen_blocked: bool,
}

/// The configured combo, falling back to the default when the setting is blank.
pub fn hotkey_of(cfg: &config::AppConfig) -> &str {
    match cfg.overlay_hotkey.trim() {
        "" => DEFAULT_OVERLAY_HOTKEY,
        combo => combo,
    }
}

/// Parse a Tauri accelerator string, naming the bad input on failure.
///
/// The Settings field builds these from real key events, but a hand-edited config can
/// still hold nonsense — and an unparseable combo must not take the overlay down with it.
pub fn parse_hotkey(combo: &str) -> Result<Shortcut, String> {
    combo
        .parse::<Shortcut>()
        .map_err(|_| format!("\"{combo}\" isn't a shortcut we can register."))
}

/// Get-or-create the overlay window, hidden.
///
/// Created lazily on first use rather than at startup: players who never press the
/// hotkey shouldn't pay for a second webview.
fn ensure_window<R: Runtime>(app: &AppHandle<R>) -> Result<WebviewWindow<R>, String> {
    if let Some(w) = app.get_webview_window(LABEL) {
        return Ok(w);
    }
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(URL.into()))
        .title("MXB App overlay")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(720.0, 480.0)
        .decorations(false)
        .always_on_top(true)
        // Keep it out of the taskbar and alt-tab: it's a HUD, not a second app window.
        .skip_taskbar(true)
        .visible(false)
        .center();

    // Transparency is what makes this read as an overlay rather than a floating window.
    // Tauri gates the setter behind `macos-private-api`, which we don't take on for a
    // dev-only platform — on macOS the overlay is simply an opaque panel.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true);

    builder
        .build()
        .map_err(|e| format!("Couldn't create the overlay window: {e:#}"))
}

/// Tell the main window the overlay is being covered by an exclusive-fullscreen game.
///
/// Deliberately advisory — we show the overlay anyway. The detection can only be a
/// heuristic, and a wrong guess must not be what stops the overlay from opening; the
/// player sees this the moment they alt-tab, which is what they'd do anyway.
fn fullscreen_hint<R: Runtime>(app: &AppHandle<R>) {
    log::info!("overlay summoned while a D3D app owns the screen exclusively");
    let _ = app.emit(FULLSCREEN_EVENT, ());
}

/// Show the overlay over the game (or hide it and hand focus back).
pub fn toggle<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let window = ensure_window(app)?;
    if window.is_visible().unwrap_or(false) {
        return hide(app);
    }
    if gameproc::is_exclusive_fullscreen() {
        fullscreen_hint(app);
    }
    center_over_game(&window);
    window.show().map_err(|e| format!("{e:#}"))?;
    let _ = window.set_focus();
    Ok(())
}

/// Put the overlay over the game's window, wherever that is.
///
/// Tauri centres it on the primary monitor at build time, which is the wrong screen on
/// the multi-monitor rigs this crowd runs. Best-effort: with no game up (or off Windows)
/// the centred position stands.
fn center_over_game<R: Runtime>(window: &WebviewWindow<R>) {
    let Some((left, top, right, bottom)) = gameproc::game_window_rect() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let x = left + (right - left - size.width as i32) / 2;
    let y = top + (bottom - top - size.height as i32) / 2;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Hide the overlay and return keyboard focus to MX Bikes.
pub fn hide<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.hide().map_err(|e| format!("{e:#}"))?;
    }
    // Order matters: the game can't take the foreground while our window still holds it.
    gameproc::focus_game();
    Ok(())
}

/// Point the global hotkey at the overlay, replacing whatever was registered before.
///
/// Unregisters everything first so a hotkey change can't leave the old combo live —
/// this app registers no other shortcuts, so the blanket call is the honest one.
pub fn register<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    if !cfg.overlay_enabled {
        log::info!("overlay hotkey disabled by config");
        return Ok(());
    }

    let combo = hotkey_of(cfg).to_string();
    let shortcut = parse_hotkey(&combo)?;
    shortcuts
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            // Fires on both press and release; acting on both would toggle twice.
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if let Err(e) = toggle(app) {
                log::error!("overlay toggle failed: {e}");
            }
        })
        .map_err(|e| {
            format!("Couldn't register {combo} — another app is probably using it. ({e})")
        })?;
    log::info!("overlay hotkey registered: {combo}");
    Ok(())
}

/// Current overlay settings plus what the game is doing right now.
pub fn state(cfg: &config::AppConfig) -> OverlayState {
    OverlayState {
        enabled: cfg.overlay_enabled,
        hotkey: hotkey_of(cfg).to_string(),
        game_running: gameproc::is_game_running(),
        fullscreen_blocked: gameproc::is_exclusive_fullscreen(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_hotkey_falls_back_to_the_default() {
        let mut cfg = config::AppConfig::default();
        cfg.overlay_hotkey = "   ".into();
        assert_eq!(hotkey_of(&cfg), DEFAULT_OVERLAY_HOTKEY);
    }

    #[test]
    fn a_configured_hotkey_wins() {
        let mut cfg = config::AppConfig::default();
        cfg.overlay_hotkey = "Alt+F1".into();
        assert_eq!(hotkey_of(&cfg), "Alt+F1");
    }

    #[test]
    fn the_default_hotkey_is_registrable() {
        parse_hotkey(DEFAULT_OVERLAY_HOTKEY).expect("the shipped default must parse");
    }

    /// A hand-edited config holding junk should surface a message naming the junk,
    /// not panic or silently disable the overlay.
    #[test]
    fn an_unparseable_hotkey_says_what_was_wrong() {
        let err = parse_hotkey("Ctrl+++").expect_err("nonsense doesn't parse");
        assert!(err.contains("Ctrl+++"), "names the bad combo: {err}");
    }

    /// Tauri's mock runtime — real window bookkeeping, no display, no webview.
    fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app builds")
    }

    // The mock runtime reports every window as visible, so show/hide state isn't
    // something these can check — that's the Windows manual pass. What they do pin down
    // is the part that has actually gone wrong in second-window features: building the
    // thing twice.

    #[test]
    fn summoning_twice_reuses_the_one_window() {
        let app = mock_app();
        let first = ensure_window(app.handle()).expect("the overlay window is creatable");
        let second = ensure_window(app.handle()).expect("second call reuses the window");
        assert_eq!(first.label(), second.label());
        assert_eq!(
            app.webview_windows().len(),
            1,
            "a second summon must not stack another webview behind the first"
        );
    }

    #[test]
    fn toggling_never_builds_a_second_overlay() {
        let app = mock_app();
        toggle(app.handle()).expect("first toggle");
        toggle(app.handle()).expect("second toggle");
        toggle(app.handle()).expect("third toggle");
        assert_eq!(app.webview_windows().len(), 1);
    }

    /// Esc and the close button both route here, and they can fire before the window
    /// has ever been built (e.g. a stale frontend). That must not error.
    #[test]
    fn hiding_a_never_opened_overlay_is_harmless() {
        let app = mock_app();
        hide(app.handle()).expect("nothing to hide is not a failure");
        assert!(app.get_webview_window(LABEL).is_none());
    }
}
