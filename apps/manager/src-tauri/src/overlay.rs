//! MXB App's in-game overlay: Presets, Locker, Browse and Manage over the running game, so a
//! preset or swap can be changed without alt-tabbing. [`crate::gameproc::refresh_look`]
//! pushes a look change into the running game, so a preset picked here shows up at once.
//!
//! The window and its rules are core's ([`mxb_core::overlay`]). This holds its spec, the
//! shortcuts this app binds, and its end of the link to MXB Coach: MXB App always holds the
//! overlay key, and opens Coach's overlay through it.

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use mxb_core::overlay::{self as core, Spec};
use mxb_core::overlaylink::{App, Event, Link, Msg, PeerInfo};
pub use mxb_core::overlay::{dismiss, hide, on_focus_lost, OverlayState, LABEL};

use crate::config;

pub static SPEC: Spec = Spec {
    app: App::Manager,
    url: "index.html?overlay=1",
    title: "MXB App overlay",
    size: (1100.0, 720.0),
    min_size: (720.0, 480.0),
    tabs: &["presets", "locker", "browse", "manage"],
};

/// Show the overlay over the game, or put away whichever overlay is up.
pub fn toggle<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    core::toggle(app, &SPEC)
}

/// Point the global hotkey at the overlay, replacing whatever was registered before.
pub fn register<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> Result<(), String> {
    let result = bind(app, cfg);
    core::record_hotkey_result(&result);
    result
}

/// Rebind **every** global shortcut the app owns.
///
/// `unregister_all` is why this is one function rather than each feature registering its
/// own key: whoever called it last would otherwise wipe the others. Push-to-talk is bound
/// here for that reason. The overlay goes first, so a push-to-talk combo another app owns
/// costs the player their mic key and not their overlay as well.
fn bind<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> Result<(), String> {
    let _ = app.global_shortcut().unregister_all();

    let overlay_result = if cfg.overlay_enabled {
        core::bind_toggle(app, &SPEC, cfg)
    } else {
        log::info!("overlay hotkey disabled by config");
        Ok(())
    };

    // Bound even when the overlay is off — the two features are unrelated.
    let ptt_result = crate::voice::bind_ptt(app, cfg);

    overlay_result.and(ptt_result)
}

/// Current overlay settings plus what the game is doing right now.
pub fn state<R: Runtime>(app: &AppHandle<R>, cfg: &config::AppConfig) -> OverlayState {
    core::state(cfg, core::peer(app))
}

/// Start this app's end of the link to MXB Coach, and dial it if it's already up.
pub fn start_link(app: &AppHandle) {
    let Some(dir) = config::data_dir(app).map(|d| d.join("overlay")) else {
        return;
    };
    let handle = app.clone();
    let version = app.package_info().version.to_string();
    match Link::start(App::Manager, dir, &version, SPEC.tabs, move |link, event| {
        on_link(&handle, link, event)
    }) {
        Ok(link) => {
            let dial = link.clone();
            app.manage(link);
            std::thread::spawn(move || dial.dial());
        }
        Err(e) => log::warn!("overlay link not started: {e}"),
    }
}

/// Delete the presence file on the way out.
pub fn stop_link<R: Runtime>(app: &AppHandle<R>) {
    if let Some(link) = core::link(app) {
        link.stop();
    }
}

fn on_link(app: &AppHandle, link: &Link, event: Event) {
    if let Event::Msg(Msg::Rebind { .. }) = event {
        // Coach has let go of the key, or changed it in its Settings: bind from the config.
        let cfg = config::load(app).unwrap_or_default();
        let error = register(app, &cfg).err();
        link.send(&Msg::Rebind { error });
        return;
    }
    core::on_link_event(app, &SPEC, &event);
}

/// One of Coach's tabs was clicked in this overlay: Coach shows its own, here, on that tab.
#[tauri::command]
pub fn overlay_handoff(app: AppHandle, tab: String) -> Result<(), String> {
    core::handoff(&app, tab)
}

/// MXB Coach, while the two are linked.
#[tauri::command]
pub fn overlay_peer(app: AppHandle) -> Option<PeerInfo> {
    core::peer(&app)
}
