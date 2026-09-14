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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

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

/// How long to let a focus loss settle before deciding the player clicked away.
///
/// Windows deactivates us *before* it settles the new foreground window, so an
/// immediate check still reads our own window and would never hide anything.
const BLUR_SETTLE: Duration = Duration::from_millis(150);

/// Counts the times the overlay has been put on screen.
///
/// A pending blur check reads this to tell "still the same showing" from "hidden and
/// summoned again while we were waiting" — otherwise a fast hotkey press right after a
/// click away would be answered by an overlay that closes itself 150ms later.
static SHOWINGS: AtomicU64 = AtomicU64::new(0);

/// What the Settings panel needs to describe the overlay's current state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayState {
    pub enabled: bool,
    pub hotkey: String,
    pub game_running: bool,
    /// A DirectX app is holding the screen exclusively right now.
    pub fullscreen_blocked: bool,
    /// Why the hotkey isn't live, when it isn't. `None` means the combo is registered
    /// — or that the overlay is switched off, which the player already knows.
    pub hotkey_error: Option<String>,
}

/// Why the last [`register`] call failed, if it did.
///
/// A hotkey that never bound has nothing to say at the moment it isn't pressed, and
/// startup only logs the failure (`main.rs`), so without this the player's evidence is
/// "I pressed it and nothing happened". Settings reads it and names the cause.
static HOTKEY_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn record_hotkey_result(result: &Result<(), String>) {
    if let Ok(mut slot) = HOTKEY_ERROR.lock() {
        *slot = result.as_ref().err().cloned();
    }
}

fn hotkey_error() -> Option<String> {
    HOTKEY_ERROR.lock().ok().and_then(|slot| slot.clone())
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
    SHOWINGS.fetch_add(1, Ordering::SeqCst);
    crate::usage::track("overlay.open");
    let _ = window.set_focus();
    Ok(())
}

/// What the desktop looked like once a focus loss had settled.
#[derive(Debug, Clone, Copy)]
struct Blur {
    /// The overlay is still on the same showing it was when focus left.
    same_showing: bool,
    /// It's still on the screen — something else may have put it away already.
    visible: bool,
    /// The player came back to it during the settle window.
    refocused: bool,
    /// Another process owns the foreground now.
    foreground_is_another_app: bool,
}

/// Should a blurred overlay take itself off the screen?
///
/// Kept separate from the timing and the window handles so the rules can be tested:
/// they're the whole reason this isn't just "hide on blur".
fn should_dismiss(blur: Blur) -> bool {
    blur.same_showing && blur.visible && !blur.refocused && blur.foreground_is_another_app
}

/// Put the overlay away when the player clicks back into the game.
///
/// Without this the window stays up unfocused, and an unfocused webview over a game
/// that's repainting the screen stops being drawn — leaving its frame sitting on top of
/// the game as an empty box that still swallows clicks. The window has to actually go.
///
/// Deliberately not fired on every blur: our own file picker (Browse → import) takes
/// focus off the overlay too, and closing the window out from under a dialog it opened
/// is worse than the bug. Hence the wait, and the check for *whose* window took over.
pub fn on_focus_lost<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let showing = SHOWINGS.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(BLUR_SETTLE);
        let blur = Blur {
            same_showing: SHOWINGS.load(Ordering::SeqCst) == showing,
            visible: window.is_visible().unwrap_or(false),
            refocused: window.is_focused().unwrap_or(false),
            foreground_is_another_app: gameproc::foreground_is_another_app(),
        };
        if !should_dismiss(blur) {
            return;
        }
        // `dismiss`, not `hide`: focus is already wherever the player sent it, and
        // dragging MX Bikes forward would fight the app they just switched to.
        if let Err(e) = dismiss(window.app_handle()) {
            log::warn!("overlay didn't close after losing focus: {e}");
        }
    });
}

/// Put the overlay over the game's window, wherever that is.
///
/// Tauri centres it on the primary monitor at build time, which is the wrong screen on
/// the multi-monitor rigs this crowd runs. Best-effort: with no game up (or off Windows)
/// the centred position stands.
fn center_over_game<R: Runtime>(window: &WebviewWindow<R>) {
    let Some(rect) = gameproc::game_window_rect() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let bounds = monitor_holding(window, rect);
    let (x, y) = centered_on(rect, (size.width, size.height), bounds);
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// A screen rectangle in physical pixels: `left, top, right, bottom`.
type Rect = (i32, i32, i32, i32);

/// Centre a `(width, height)` window on `rect`, kept inside `bounds` when there are any.
///
/// The clamp is the belt to [`gameproc::game_window_rect`]'s braces. That function is what
/// stops a minimized game handing us its (-32000, -32000) iconic rect, and this is what
/// makes sure no rect from anywhere can put the overlay somewhere the player can't see it
/// — an overlay off the edge of the desktop is indistinguishable from a hotkey that did
/// nothing, except that it also holds focus and takes it off the game.
fn centered_on(rect: Rect, size: (u32, u32), bounds: Option<Rect>) -> (i32, i32) {
    let (left, top, right, bottom) = rect;
    let (w, h) = (size.0 as i32, size.1 as i32);
    let x = left + (right - left - w) / 2;
    let y = top + (bottom - top - h) / 2;

    let Some((bl, bt, br, bb)) = bounds else {
        return (x, y);
    };
    // `.max(bl)` for a window larger than the screen it's on: there is no position that
    // fits, so pin it to the top-left and let it overhang the far edge rather than have
    // the clamp push its title bar off the near one.
    (x.clamp(bl, (br - w).max(bl)), y.clamp(bt, (bb - h).max(bt)))
}

/// The monitor `rect` sits on, as `left, top, right, bottom`.
///
/// Found by the rect's centre rather than by the window's own monitor: the whole point of
/// this path is to move the overlay to the screen the *game* is on, which on a sim rig is
/// routinely not the one Tauri built it over.
fn monitor_holding<R: Runtime>(window: &WebviewWindow<R>, rect: Rect) -> Option<Rect> {
    let (left, top, right, bottom) = rect;
    let (cx, cy) = ((left + right) / 2, (top + bottom) / 2);
    window.available_monitors().ok()?.into_iter().find_map(|m| {
        let (p, s) = (m.position(), m.size());
        let (l, t) = (p.x, p.y);
        let (r, b) = (l + s.width as i32, t + s.height as i32);
        (cx >= l && cx < r && cy >= t && cy < b).then_some((l, t, r, b))
    })
}

/// Take the overlay off the screen, leaving the foreground where it lands.
///
/// Separate from [`hide`] for the "Open full app" button: focus is on its way to the
/// main window, so pulling MX Bikes forward on the way out would only fight it.
pub fn dismiss<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.hide().map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}

/// Hide the overlay and return keyboard focus to MX Bikes.
pub fn hide<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    dismiss(app)?;
    // Order matters: the game can't take the foreground while our window still holds it.
    gameproc::focus_game();
    Ok(())
}

/// Point the global hotkey at the overlay, replacing whatever was registered before.
///
/// Unregisters everything first so a hotkey change can't leave the old combo live —
/// this app registers no other shortcuts, so the blanket call is the honest one.
pub fn register<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> Result<(), String> {
    let result = bind(app, cfg);
    record_hotkey_result(&result);
    result
}

/// Rebind **every** global shortcut the app owns.
///
/// `unregister_all` is why this is one function rather than each feature registering its
/// own key: whoever called it last would otherwise wipe the others. Push-to-talk is bound
/// here for that reason, even though it belongs to [`crate::voice`] in every other sense.
///
/// The overlay is bound first so that a push-to-talk combo another app owns costs the
/// player their mic key and not their overlay as well.
fn bind<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();

    let overlay_result = if cfg.overlay_enabled {
        bind_overlay(app, cfg)
    } else {
        log::info!("overlay hotkey disabled by config");
        Ok(())
    };

    // Bound even when the overlay is off — the two features are unrelated, and an early
    // return here would leave a player who hid the overlay with no working mic key.
    let ptt_result = crate::voice::bind_ptt(app, cfg);

    overlay_result.and(ptt_result)
}

fn bind_overlay<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> Result<(), String> {
    let combo = hotkey_of(cfg).to_string();
    let shortcut = parse_hotkey(&combo)?;
    app.global_shortcut()
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
        // A disabled overlay has no binding by design — reporting that as a fault
        // would put a warning under a switch the player just turned off.
        hotkey_error: cfg.overlay_enabled.then(hotkey_error).flatten(),
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

    /// One test, not three: `HOTKEY_ERROR` is process-wide, so separate tests touching
    /// it would race each other under the default parallel runner.
    #[test]
    fn a_failed_binding_is_remembered_and_reported() {
        let cfg = config::AppConfig::default();

        record_hotkey_result(&Err("another app already has it".into()));
        assert_eq!(
            state(&cfg).hotkey_error.as_deref(),
            Some("another app already has it"),
            "a hotkey that never bound has to say so somewhere the player can look",
        );

        let mut off = cfg.clone();
        off.overlay_enabled = false;
        assert!(
            state(&off).hotkey_error.is_none(),
            "an overlay switched off isn't broken, so it doesn't get a warning",
        );

        record_hotkey_result(&Ok(()));
        assert!(state(&cfg).hotkey_error.is_none(), "a later success clears it");
    }

    /// A hand-edited config holding junk should surface a message naming the junk,
    /// not panic or silently disable the overlay.
    #[test]
    fn an_unparseable_hotkey_says_what_was_wrong() {
        let err = parse_hotkey("Ctrl+++").expect_err("nonsense doesn't parse");
        assert!(err.contains("Ctrl+++"), "names the bad combo: {err}");
    }

    /// A 1920x1080 primary monitor at the origin.
    const PRIMARY: Rect = (0, 0, 1920, 1080);
    /// What the overlay is sized to.
    const OVERLAY: (u32, u32) = (WIDTH as u32, HEIGHT as u32);

    #[test]
    fn the_overlay_lands_in_the_middle_of_the_game_window() {
        // A 1600x900 game window centred on the primary monitor.
        let game = (160, 90, 1760, 990);
        assert_eq!(
            centered_on(game, OVERLAY, Some(PRIMARY)),
            (410, 180),
            "centred on the game, not on the monitor",
        );
    }

    /// The bug this whole path had: a minimized game reports the iconic rect, and an
    /// overlay centred on it is placed ~32,000px off the left of the desktop — shown,
    /// focused, and invisible, which is indistinguishable from a hotkey that did nothing.
    ///
    /// `game_window_rect` now refuses to return that rect at all. This is the second line
    /// of defence, and it is the one that holds for *any* rect we haven't thought of.
    #[test]
    fn an_iconic_rect_cannot_put_the_overlay_off_the_desktop() {
        let iconic = (-32000, -32000, -31840, -31970);
        let (x, y) = centered_on(iconic, OVERLAY, Some(PRIMARY));
        assert_eq!((x, y), (0, 0), "pulled back onto the monitor");

        let unclamped = centered_on(iconic, OVERLAY, None);
        assert!(
            unclamped.0 < -32000,
            "and without bounds it really would go off-screen: {unclamped:?}",
        );
    }

    /// The reason this positions by the game's monitor rather than the primary one. A
    /// screen to the left of primary has negative coordinates, and a clamp that assumed
    /// otherwise would drag the overlay back onto the wrong display — breaking exactly
    /// the multi-monitor case the feature exists for.
    #[test]
    fn a_monitor_left_of_primary_keeps_its_negative_coordinates() {
        let left_screen = (-1920, 0, 0, 1080);
        let game = (-1920, 0, 0, 1080); // fullscreen on that monitor
        let (x, y) = centered_on(game, OVERLAY, Some(left_screen));
        assert_eq!((x, y), (-1510, 180));
        assert!(x < 0, "stayed on the left-hand screen");
    }

    /// A window taller than the screen has no position that fits. Pin it to the top-left
    /// so its controls are reachable, rather than letting the clamp push them off.
    #[test]
    fn an_overlay_bigger_than_the_screen_is_pinned_not_pushed() {
        let small_screen = (0, 0, 800, 600);
        let game = (0, 0, 800, 600);
        assert_eq!(centered_on(game, OVERLAY, Some(small_screen)), (0, 0));
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

    /// "Open full app" routes through `dismiss` before the main window is raised, and
    /// it too can fire with no overlay window built yet.
    #[test]
    fn dismissing_a_never_opened_overlay_is_harmless() {
        let app = mock_app();
        dismiss(app.handle()).expect("nothing to dismiss is not a failure");
        assert!(app.get_webview_window(LABEL).is_none());
    }

    /// A blur with nothing else going on: the player clicked back into the game.
    fn clicked_away() -> Blur {
        Blur {
            same_showing: true,
            visible: true,
            refocused: false,
            foreground_is_another_app: true,
        }
    }

    #[test]
    fn clicking_back_into_the_game_closes_the_overlay() {
        assert!(
            should_dismiss(clicked_away()),
            "an overlay left up unfocused is the empty box over the game",
        );
    }

    /// Browse's import picker is a window of *our* process, and it deactivates the
    /// overlay exactly like a click into the game does. Closing the overlay while its
    /// own dialog is up would strand the player in a file picker with nothing behind it.
    #[test]
    fn our_own_file_picker_does_not_close_it() {
        let blur = Blur {
            foreground_is_another_app: false,
            ..clicked_away()
        };
        assert!(!should_dismiss(blur));
    }

    #[test]
    fn coming_back_during_the_settle_keeps_it_open() {
        let blur = Blur {
            refocused: true,
            ..clicked_away()
        };
        assert!(!should_dismiss(blur));
    }

    /// The hotkey, Esc and the close button all hide the window themselves, and each
    /// one blurs it on the way out. The pending check must not fire into that.
    #[test]
    fn an_already_hidden_overlay_is_left_alone() {
        let blur = Blur {
            visible: false,
            ..clicked_away()
        };
        assert!(!should_dismiss(blur));
    }

    /// Click into the game, then hit the hotkey before the settle is up: the overlay
    /// the player just summoned must not be closed by the previous showing's blur.
    #[test]
    fn a_resummon_during_the_settle_survives() {
        let blur = Blur {
            same_showing: false,
            ..clicked_away()
        };
        assert!(!should_dismiss(blur));
    }
}
