//! MXB Coach's in-game overlay: tips, setup, live cues and the HUD over the game.
//!
//! Alone, Coach holds the overlay hotkey. Beside MXB App it registers none: MXB App holds the
//! one key for both, and the two overlays hand the screen to each other over
//! [`mxb_core::overlaylink`]. The window and its rules are core's.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use mxb_core::config;
use mxb_core::gamewindow;
use mxb_core::overlay::{self as core, OverlayState, Spec};
use mxb_core::overlaylink::{self, App, Event, Link, Msg, PeerInfo};
pub use mxb_core::overlay::{on_focus_lost, LABEL};

pub static SPEC: Spec = Spec {
    app: App::Coach,
    url: "index.html?overlay=1",
    title: "MXB Coach overlay",
    size: (560.0, 680.0),
    min_size: (420.0, 480.0),
    tabs: &["tips", "setup", "cues", "hud"],
};

/// An MXB App from before the link holds the key without telling anyone.
const MANAGER_EXE: &str = "MXB App.exe";
/// How often to look for MXB App: to link, or to notice an old one.
const POLL: Duration = Duration::from_secs(5);

/// Why Coach isn't holding the key, or `None` when it is. See [`OverlayState::deferred`].
static DEFERRED: Mutex<Option<&'static str>> = Mutex::new(None);
/// Whether [`DEFERRED`] has been acted on yet, so the first decision always binds.
static APPLIED: AtomicBool = AtomicBool::new(false);
/// What MXB App last said about binding the key, while it holds it for both.
static PEER_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn deferred() -> Option<&'static str> {
    DEFERRED.lock().ok().and_then(|d| *d)
}

/// Who should hold the key right now.
fn decide(app: &AppHandle) -> Option<&'static str> {
    let Some(link) = core::link(app) else {
        // Coach's own link never started — a firewall or AV blocking the loopback listener
        // is the usual cause. Without it there is no way to tell a current MXB App from one
        // that predates the link, and the guess below would have Coach let the key go to an
        // app that is already holding it for itself: the overlay then never opens at all,
        // and Settings blames "an older MXB App" on a perfectly current one.
        //
        // So hold it. If an old MXB App really does own the combo the bind fails and says
        // which, and a named failure beats a shortcut that silently does nothing.
        return None;
    };
    if link.peer().is_some() {
        return Some("linked");
    }
    if let Some(p) = link.mismatch() {
        return Some(if p.proto > overlaylink::PROTO { "updateCoach" } else { "updateManager" });
    }
    // Running, and not linked: an MXB App that predates the link, holding the key itself.
    gamewindow::process_running(MANAGER_EXE).then_some("oldManager")
}

/// Hold the key or let it go, when that changed (or `force`).
fn apply(app: &AppHandle, next: Option<&'static str>, force: bool) -> Result<(), String> {
    {
        let Ok(mut cur) = DEFERRED.lock() else {
            return Ok(());
        };
        let first = !APPLIED.swap(true, Ordering::SeqCst);
        if !first && !force && *cur == next {
            return Ok(());
        }
        *cur = next;
    }
    let _ = app.global_shortcut().unregister_all();
    let result = match next {
        None => {
            let cfg = config::load(app).unwrap_or_default();
            if cfg.overlay_enabled {
                core::bind_toggle(app, &SPEC, &cfg)
            } else {
                Ok(())
            }
        }
        Some(why) => {
            log::info!("overlay hotkey left to MXB App ({why})");
            Ok(())
        }
    };
    core::record_hotkey_result(&result);
    result
}

/// Start the link and the watch for MXB App.
pub fn start(app: &AppHandle) {
    if let Some(dir) = config::data_dir(app).map(|d| d.join("overlay")) {
        let handle = app.clone();
        let version = app.package_info().version.to_string();
        match Link::start(App::Coach, dir, &version, SPEC.tabs, move |link, event| {
            on_link(&handle, link, event)
        }) {
            Ok(link) => {
                app.manage(link);
            }
            Err(e) => log::warn!("overlay link not started: {e}"),
        }
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        if let Some(link) = core::link(&handle) {
            link.dial();
        }
        let _ = apply(&handle, decide(&handle), false);
        std::thread::sleep(POLL);
    });
}

/// Delete the presence file on the way out.
pub fn stop(app: &AppHandle) {
    if let Some(link) = core::link(app) {
        link.stop();
    }
}

fn on_link(app: &AppHandle, link: &Link, event: Event) {
    match &event {
        Event::Up(_) => {
            // Let go first, then tell MXB App it can bind.
            let _ = apply(app, Some("linked"), false);
            link.send(&Msg::Rebind { error: None });
        }
        Event::Down => {
            if let Ok(mut e) = PEER_ERROR.lock() {
                *e = None;
            }
            // MXB App went (or crashed): the key is ours again at once.
            let _ = apply(app, None, false);
        }
        Event::Msg(Msg::Rebind { error }) => {
            if let Ok(mut e) = PEER_ERROR.lock() {
                *e = error.clone();
            }
        }
        _ => {}
    }
    core::on_link_event(app, &SPEC, &event);
}

/// A setting changed: MXB App rebinds when it holds the key, else Coach does.
fn rebind(app: &AppHandle) -> Result<(), String> {
    match deferred() {
        Some("linked") => {
            if let Some(link) = core::link(app) {
                link.send(&Msg::Rebind { error: None });
            }
            Ok(())
        }
        Some(_) => Ok(()),
        None => apply(app, None, true),
    }
}

/// Coach never writes `config.json` from nothing: a file holding only these keys would make
/// MXB App think it had been set up. It writes into one that exists or can be detected.
fn patch(app: &AppHandle, key: &str, value: serde_json::Value) -> Result<(), String> {
    if !config::exists(app) && config::load_or_detect(app).is_none() {
        return Err("MXB Coach couldn't find the game's folder, so it has nowhere to keep this setting.".into());
    }
    let mut keys = serde_json::Map::new();
    keys.insert(key.to_string(), value);
    config::patch_json(app, keys).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn overlay_toggle(app: AppHandle) -> Result<(), String> {
    core::toggle(&app, &SPEC)
}

#[tauri::command]
pub fn overlay_hide(app: AppHandle) -> Result<(), String> {
    core::hide(&app)
}

#[tauri::command]
pub fn overlay_state(app: AppHandle) -> OverlayState {
    let cfg = config::load(&app).unwrap_or_default();
    let mut state = core::state(&cfg, core::peer(&app));
    let why = deferred();
    state.deferred = why.map(str::to_string);
    // No link means no sharing with MXB App, whatever else is true. Coach keeps the key in
    // that case, so this is a note about the two apps, not about the shortcut.
    state.link_down = core::link(&app).is_none();
    if why.is_some() {
        // Not ours to bind: only MXB App's own report, while linked, says anything.
        let peer_error = PEER_ERROR.lock().ok().and_then(|e| e.clone());
        state.hotkey_error = (why == Some("linked") && cfg.overlay_enabled).then_some(peer_error).flatten();
    }
    state
}

#[tauri::command]
pub fn set_overlay_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    patch(&app, "overlayEnabled", serde_json::json!(enabled))?;
    if !enabled {
        let _ = core::hide(&app);
    }
    rebind(&app)
}

/// Rebind the overlay hotkey. Alone, it registers before saving, so a combo another app owns
/// leaves the working one in place. Linked, MXB App binds it and reports back.
#[tauri::command]
pub fn set_overlay_hotkey(app: AppHandle, hotkey: String) -> Result<(), String> {
    core::parse_hotkey(&hotkey)?;
    if deferred().is_none() {
        let previous = config::load(&app).unwrap_or_default();
        let mut cfg = previous.clone();
        cfg.overlay_hotkey = hotkey.clone();
        if cfg.overlay_enabled {
            let _ = app.global_shortcut().unregister_all();
            if let Err(e) = core::bind_toggle(&app, &SPEC, &cfg) {
                let _ = app.global_shortcut().unregister_all();
                let _ = core::bind_toggle(&app, &SPEC, &previous);
                return Err(e);
            }
            core::record_hotkey_result(&Ok(()));
        }
        return patch(&app, "overlayHotkey", serde_json::json!(hotkey));
    }
    patch(&app, "overlayHotkey", serde_json::json!(hotkey))?;
    rebind(&app)
}

/// "Open full app": put the overlay away and bring Coach's own window forward.
#[tauri::command]
pub fn overlay_open_main(app: AppHandle) -> Result<(), String> {
    core::dismiss(&app)?;
    show_main(&app);
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// One of MXB App's tabs was clicked in this overlay: MXB App shows its own, here.
#[tauri::command]
pub fn overlay_handoff(app: AppHandle, tab: String) -> Result<(), String> {
    core::handoff(&app, tab)
}

/// MXB App, while the two are linked.
#[tauri::command]
pub fn overlay_peer(app: AppHandle) -> Option<PeerInfo> {
    core::peer(&app)
}
