// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod antidebug;
pub(crate) use mxb_core::bikefiles;
mod bikeswap;
mod bundle;
mod cancel;
pub(crate) use mxb_core::cfg;
pub(crate) use mxb_core::cloudfiles;
pub(crate) use mxb_core::viewer;
pub(crate) use mxb_core::config;
mod cookie_session;
mod downloads;
mod dropzone;
pub(crate) use mxb_core::edf;
mod edfwrite;
mod feel;
mod fileshare;
mod firstpaint;
mod frostmod;
mod frostmod_manage;
pub(crate) use mxb_core::game;
mod fileinfo;
mod gameproc;
pub(crate) use mxb_core::gate;
mod gearrepair;
pub(crate) use mxb_core::heightfield;
mod hub_clearance;
mod hub_session;
mod identity;
mod imgcache;
mod install;
mod ledger;
pub(crate) use mxb_core::library;
mod liveshare;
pub(crate) use mxb_core::linkwalk;
mod logs;
pub(crate) use mxb_core::lru;
pub(crate) use mxb_core::map;
mod memwatch;
mod modelswap;
mod mods;
mod modstate;
mod modwatch;
mod profilewatch;
mod mxb_fetch;
mod mxb_session;
mod overlay;
pub(crate) use mxb_core::paint;
mod paintstudio;
pub(crate) use mxb_core::paintwatch;
mod peident;
pub(crate) use mxb_core::pkz;
/// Paid plugins: what this install may run, and how it proves it offline.
mod plugins;
/// What the running game has loaded, reported for diagnostics.
mod procmods;
/// Linux only: the Proton prefix the game runs in, and how to put a Windows program in it.
#[cfg(target_os = "linux")]
pub(crate) use mxb_core::proton;
#[cfg(sidecar)]
pub(crate) use mxb_core::sidecar;
#[cfg(sidecar)]
mod sidecar_lock;
/// The world-server browser: speaks the master-server protocol to list live servers.
/// Local-only, like [`sidecar`] — the public tree neither has the file nor the feature.
#[cfg(worldnet)]
mod worldnet;
#[cfg(mxbsecure)]
mod mxbsecure;
mod steamid;
mod secure_launch;

#[cfg(all(test, mxbsecure))]
mod offline_flow_test {
    // The whole offline story on a real file: lock, provision (seal to a Steam ID), then
    // open offline with that identity — and prove a different Steam account gets nothing.
    #[test]
    fn provision_then_open_offline_binds_to_the_steam_id() {
        let dir = std::env::temp_dir().join(format!("mxb-offline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // A fake Steam loginusers.vdf, pointed at by the env override the reader honours.
        let vdf = dir.join("loginusers.vdf");
        let write_vdf = |id: &str| {
            std::fs::write(
                &vdf,
                format!("\"users\"\n{{\n\t\"{id}\"\n\t{{\n\t\t\"MostRecent\" \"1\"\n\t}}\n}}\n"),
            )
            .unwrap();
        };
        std::env::set_var("MXB_STEAM_LOGINUSERS", &vdf);

        // Lock a real file.
        let plaintext = vec![9u8; 130_000];
        let src = dir.join("track.pkz");
        std::fs::write(&src, &plaintext).unwrap();
        let locked = crate::mxbsecure::lock(&plaintext, "trk_x", "k1");
        let blob_path = dir.join("track.pkz.mxbsecure");
        std::fs::write(&blob_path, &locked.blob).unwrap();

        // Provision: seal the key to the (fake) live Steam ID, store the .mxbkey.
        write_vdf("76561198000000001");
        let steam = crate::steamid::current_steam_id64().expect("steam id");
        assert_eq!(steam, "76561198000000001");
        let sealed = crate::mxbsecure::seal_key_to_identity(&locked.content_key, &steam, "");
        std::fs::write(dir.join("track.pkz.mxbsecure.mxbkey"), &sealed).unwrap();

        // Open offline as the same account: unseal, decrypt, compare.
        let key = crate::mxbsecure::unseal_key(&sealed, &crate::steamid::current_steam_id64().unwrap(), "")
            .expect("unseals for the same account");
        let opened = crate::mxbsecure::open(&locked.blob, &key).unwrap();
        assert_eq!(opened, plaintext, "offline open matches the original");

        // A different Steam account (a copy on a friend's machine) gets nothing.
        write_vdf("76561198000000999");
        let other = crate::steamid::current_steam_id64().unwrap();
        assert_eq!(other, "76561198000000999");
        assert!(
            crate::mxbsecure::unseal_key(&sealed, &other, "").is_none(),
            "another account must not unseal it"
        );

        std::env::remove_var("MXB_STEAM_LOGINUSERS");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
pub(crate) use mxb_core::presets;
mod paintsync;
mod ranked;
mod reshade;
pub(crate) use mxb_core::scenery;
mod serverbook;
mod serverfilter;
mod servers;
mod sessionwatch;
mod shop_catalog_session;
mod shop_credentials;
mod shop_fetch;
mod shop_installed;
mod shop_session;
mod soundmods;
pub(crate) use mxb_core::texstore;
pub(crate) use mxb_core::track;
mod trackbuild;
mod trackline;
mod tracklayout;
mod trackllm;
mod trackobjects;
mod trackprog;
mod trackprops;
mod trackscenery;
mod trackshot;
mod trackspeed;
mod trackstats;
mod tracksynth;
mod upload;
pub(crate) use mxb_core::usage;
mod vcruntime;
mod voice;
pub(crate) use mxb_core::winehost;

use config::AppConfig;
use mxb_core::viewer::{BikeModel, PreviewSet};
use frostmod::ReloadOutcome;
use frostmod_manage::{FrostmodProcess, FrostmodStatus, InstallReport};
use library::InstalledMod;
use modwatch::ModWatcher;
use paintwatch::{LookWatcher, PaintWatcher};
// Decoding a paint's textures is per-texture CPU work over no shared state, and every path
// that does it wants the same treatment — so this sits here rather than in one function.
use rayon::prelude::*;
use profilewatch::ProfileWatcher;
use mods::mxb::WpModsSource;
use mods::{ModDetail, ModRating, ModSort, ModSource, ModSummary};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// The app window, as opposed to the transient ones the app opens alongside it (the
/// overlay, the mxb-mods.com clearance check, the shop login). `tauri.conf.json` declares
/// it without an explicit label, which is Tauri's default of `main`.
pub(crate) const MAIN_WINDOW: &str = "main";

/// The shop login WebView, opened on demand and closed once the session is captured.
const SHOP_LOGIN_WINDOW: &str = "shop-login";
/// The MXB Hub sign-in window. Transient like the shop's: it is the user typing a password
/// into the store's own page, and it closes the moment the cookie appears.
const HUB_LOGIN_WINDOW: &str = "hub-login";

/// Whether closing this window should park it in the tray rather than destroy it.
///
/// Only the main window. The transient ones are owned by the code that opened them, and
/// hiding one instead of closing it keeps its label registered for the life of the
/// process — the next attempt to build it then fails with "a webview with label `…`
/// already exists" and never opens again. A tester hit exactly that on the mxb-mods.com
/// clearance window: the first Cloudflare handshake worked, and every Retry afterwards
/// silently did nothing.
fn parks_in_tray(label: &str) -> bool {
    label == MAIN_WINDOW
}

/// Whether closing the main window should park it in the tray instead of ending the app.
///
/// `painted` is the one that isn't a preference: a window that never painted has no close
/// button in it, so parking it leaves the player nothing — the process stays alive holding
/// the single-instance guard, and the next launch hands the same dead window straight back.
///
/// Never parks on Linux: the tray runs through libayatana-appindicator, which doesn't
/// deliver click events to Tauri and isn't present at all on a stock GNOME desktop, so
/// hiding there can strand the window with no way back. Never in a dev build either, or a
/// `tauri dev` run would linger in the tray and block the next one.
fn parks_on_close(painted: bool, run_in_background: bool) -> bool {
    let tray_can_restore = cfg!(not(target_os = "linux"));
    painted && run_in_background && tray_can_restore && !cfg!(debug_assertions)
}

/// Whether a window may make this IPC call.
///
/// Exists for the two windows that run a *remote* origin. [`mxb_fetch`] parks a hidden webview
/// on mxb-mods.com and hands catalog fetches back; [`shop_fetch`] parks one on
/// mxbikes-shop.com and hands the signed-in purchases page back. Giving each a capability is
/// what lets its page speak to us at all. But a capability grants IPC in general, and the
/// commands registered with `generate_handler!` are not covered by the permission ACL — so
/// without this, script on either site could call `create_config`, `install_mod` or anything
/// else the app exposes.
///
/// So both get an allowlist of exactly one call: emitting their result event. Every other
/// window is unaffected and keeps whatever its own capability file grants.
fn ipc_allowed(label: &str, command: &str) -> bool {
    if label != mxb_fetch::WINDOW && label != shop_fetch::WINDOW {
        return true;
    }
    command == "plugin:event|emit"
}

/// Whether the app is ready to use. Falls back to auto-detection when the config file
/// is missing, so the setup screen only appears when the MX Bikes folder genuinely
/// can't be found — not every time the saved config goes astray.
#[tauri::command]
fn is_configured(app: tauri::AppHandle) -> bool {
    config::load_or_detect(&app).is_some()
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> AppConfig {
    config::load(&app).unwrap_or_default()
}

#[tauri::command]
fn create_config(
    app: tauri::AppHandle,
    watcher: State<ModWatcher>,
    config: AppConfig,
) -> Result<bool, String> {
    let mut cfg = config::finalize(config);
    // Detection came up empty and the user didn't pick a folder, so there is nothing to
    // save. Say so instead of writing a config with no folder in it: the setup screen
    // only reappears when `modsPath` is blank, so a silent save would bounce the user
    // straight back to the same screen with no explanation of what went wrong.
    if cfg.mods_path.trim().is_empty() {
        return Err(format!(
            "Couldn't find your {} folder automatically — choose it manually.",
            cfg.game().display
        ));
    }
    // Setup only sends the folders, so carry over first-run state from any config
    // that's already there — rewriting it would replay the intro and the tour.
    match config::load(&app) {
        Ok(prev) => {
            cfg.welcome_seen |= prev.welcome_seen;
            cfg.tour_done |= prev.tour_done;
            cfg.seen_version = prev.seen_version;
        }
        // Nothing came before: this install is new, and nothing in the version someone
        // just installed is news to them. Stamping it here is what keeps the release
        // showcase to upgrades — a first run gets the intro and the tour instead.
        Err(_) => cfg.seen_version = app.package_info().version.to_string(),
    }
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Begin watching straight away so a fresh setup doesn't need a restart before
    // manual downloads reload the game.
    if cfg.watch_mods_reload {
        modwatch::start(&app, &watcher, &cfg.mods_path);
    }
    Ok(true)
}

/// Run an mxb-mods.com call; if Cloudflare refuses it, run it again from inside a real
/// browser and keep using that transport for the rest of the session.
///
/// This used to earn a `cf_clearance` in a WebView and replay the cookie through the HTTP
/// client. A tester's log showed why that can't work: the challenge cleared in about a
/// second, the cookie was sent correctly, and Cloudflare served the interstitial to reqwest
/// anyway — a clearance is bound to the TLS fingerprint that earned it. So instead of moving
/// the cookie to the request, we move the request to the browser. See [`mxb_fetch`].
///
/// Once, not a loop: the second attempt is on a different transport, so if that is refused
/// too, trying a third time changes nothing. Only refusals a browser could plausibly satisfy
/// get this treatment — a 429 wants patience, not another request.
async fn with_clearance<T, F, Fut>(
    _app: &tauri::AppHandle,
    what: &str,
    op: F,
) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let err = match op().await {
        Ok(value) => return Ok(value),
        Err(err) => err,
    };
    match err.downcast_ref::<mods::mxb::Blocked>() {
        Some(blocked) if blocked.clearable() => {
            log::info!(
                "{what} blocked ({}) — retrying it from inside the WebView",
                blocked
                    .status
                    .map_or_else(|| "interstitial".to_string(), |s| s.to_string())
            );
        }
        // Not clearable, or not a block at all — a parse failure, a timeout, a 429.
        _ => {
            log::warn!("{what} failed and a browser wouldn't help: {err:#}");
            return Err(format!("{err:#}"));
        }
    }
    // Latches for the session: once this client's fingerprint has been refused on this
    // network, every later request would be refused the same way, so there is nothing to
    // gain from trying the HTTP client again first.
    mods::mxb::use_webview();
    match op().await {
        Ok(value) => {
            log::info!("{what} succeeded through the WebView");
            Ok(value)
        }
        // Report the browser's failure, not the original 403 — if the site is refusing a
        // real browser too, "open mxb-mods.com and hit Retry" is the wrong advice.
        Err(e) => {
            log::warn!("{what} failed through the WebView too: {e:#}");
            Err(format!("{e:#}"))
        }
    }
}

#[tauri::command]
async fn search_mods(
    app: tauri::AppHandle,
    query: String,
    category_id: u32,
    page: u32,
    sort: ModSort,
) -> Result<Vec<ModSummary>, String> {
    with_clearance(&app, "search", || {
        WpModsSource.search(&query, category_id, page, sort)
    })
    .await
}

/// Community scores for the mods currently on screen, keyed by post id. Ids the site
/// wouldn't answer for are left out rather than erroring — the cards just show no stars.
#[tauri::command]
async fn get_mod_ratings(ids: Vec<u64>) -> std::collections::HashMap<u64, ModRating> {
    mods::mxb::ratings(&ids).await
}

#[tauri::command]
async fn get_mod_detail(app: tauri::AppHandle, slug: String) -> Result<ModDetail, String> {
    with_clearance(&app, "mod detail", || WpModsSource.detail(&slug)).await
}

// ───────────────────────────── mxbikes-shop catalog ─────────────────────────────
//
// Browsing only. Nothing here installs or buys — the frontend opens the product page in
// the user's own browser. See `mods::shop_catalog`.

/// Whether this build has a shop credential at all. False hides the Shop tab entirely,
/// which is what forks and credential-less CI builds get.
#[tauri::command]
fn shop_catalog_available() -> bool {
    mods::shop_catalog::available()
}

/// Cheap and synchronous — it reports on what's already loaded and never fetches, so the
/// UI can poll it without cost.
#[tauri::command]
fn shop_catalog_status(app: tauri::AppHandle) -> mods::shop_catalog::ShopStatus {
    mods::shop_catalog::status(&app)
}

#[tauri::command]
async fn shop_catalog_categories(
    app: tauri::AppHandle,
) -> Result<Vec<mods::shop_catalog::ShopCategory>, String> {
    mods::shop_catalog::categories(&app)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn shop_catalog_search(
    app: tauri::AppHandle,
    query: String,
    category_id: Option<u64>,
    page: u32,
    sort: mods::shop_catalog::ShopSort,
    on_sale_only: bool,
) -> Result<mods::shop_catalog::ShopPage, String> {
    mods::shop_catalog::search(&app, &query, category_id, page, sort, on_sale_only)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn shop_catalog_detail(
    app: tauri::AppHandle,
    id: u64,
) -> Result<mods::shop_catalog::ShopModDetail, String> {
    mods::shop_catalog::detail(&app, id)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Ignores the cache age and any `ETag` we hold — "Refresh" has to mean refresh, not
/// "ask politely and accept a 304".
#[tauri::command]
async fn shop_catalog_refresh(
    app: tauri::AppHandle,
) -> Result<mods::shop_catalog::ShopStatus, String> {
    mods::shop_catalog::force_refresh(&app)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn get_installed_mods(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<InstalledMod>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    library::scan_mods(&cfg.mods_path, &subpath).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn scan_library(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<library::LibraryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || scan_library_blocking(app, subpath))
        .await
        .map_err(|e| format!("scan_library task failed: {e}"))?
}

fn scan_library_blocking(
    app: tauri::AppHandle,
    subpath: String,
) -> Result<Vec<library::LibraryEntry>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    // ReShade presets aren't in the mods tree, so the scanner below would look in a folder
    // that doesn't exist and report every preset as not installed. Browse compares these
    // names against catalog titles to draw its "Installed" badge, so it has to be the real
    // list. See `reshade::status`.
    if reshade::is_reshade_subpath(&subpath) {
        return Ok(reshade::status(&cfg.reshade_dir())
            .presets
            .into_iter()
            .map(|p| library::LibraryEntry {
                modified: std::fs::metadata(&p.path)
                    .map(|m| library::mtime_ms(&m))
                    .unwrap_or(0),
                name: p.name,
                path: p.path,
                folder: String::new(),
                size: 0,
                kind: "loose".into(),
                category: "reshade".into(),
                parent: None,
            })
            .collect());
    }
    let sound_bikes = sound_bikes_of(&app);
    // Looking at the library is the moment its record of what used to be there most needs to
    // be current. Detached and rate-limited: the scan the user is waiting on never pays for it.
    if ledger_due() {
        ledger_reconcile_detached(&app);
    }
    library::scan_library(&cfg.mods_path, &subpath, &sound_bikes, cfg.game()).map_err(|e| format!("{e:#}"))
}

/// Rate-limit for the Library-scan trigger. Switching tabs fires a scan each time, and
/// walking the whole tree once per tab would be work nobody asked for.
const LEDGER_MIN_GAP: std::time::Duration = std::time::Duration::from_secs(30);

/// Whether enough time has passed since the last Library-triggered reconcile. Claims the slot
/// when it answers yes, so two scans racing only produce one pass.
fn ledger_due() -> bool {
    use std::sync::Mutex;
    static LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    if last.is_some_and(|t| now.duration_since(t) < LEDGER_MIN_GAP) {
        return false;
    }
    *last = Some(now);
    true
}

#[tauri::command]
async fn scan_rider_targets(app: tauri::AppHandle) -> Result<library::RiderTargets, String> {
    tauri::async_runtime::spawn_blocking(move || scan_rider_targets_blocking(app))
        .await
        .map_err(|e| format!("scan_rider_targets task failed: {e}"))?
}

fn scan_rider_targets_blocking(app: tauri::AppHandle) -> Result<library::RiderTargets, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(library::scan_rider_targets(&cfg.mods_path))
}

/// Gear models the game can't reach where they are: files loose in an area root, or a package
/// buried a folder deep. See [`gearrepair`] for how each happened and what moves.
#[tauri::command]
async fn scan_gear_repairs(app: tauri::AppHandle) -> Result<Vec<gearrepair::GearRepair>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(gearrepair::plan(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("scan_gear_repairs task failed: {e}"))?
}

/// Carry out one repair, by the `id` its plan carries. Returns how many entries moved.
#[tauri::command]
async fn repair_gear(app: tauri::AppHandle, id: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        gearrepair::apply_one(&cfg.mods_path, &id).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("repair_gear task failed: {e}"))?
}

#[tauri::command]
async fn scan_bike_targets(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || scan_bike_targets_blocking(app))
        .await
        .map_err(|e| format!("scan_bike_targets task failed: {e}"))?
}

fn scan_bike_targets_blocking(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(library::scan_bike_targets(&cfg.mods_path, &cfg.profiles_dir()))
}

#[tauri::command]
async fn scan_model_swaps(app: tauri::AppHandle) -> Result<Vec<modelswap::BikeModels>, String> {
    tauri::async_runtime::spawn_blocking(move || scan_model_swaps_blocking(app))
        .await
        .map_err(|e| format!("scan_model_swaps task failed: {e}"))?
}

fn scan_model_swaps_blocking(app: tauri::AppHandle) -> Result<Vec<modelswap::BikeModels>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(modelswap::scan_model_swaps(&cfg.mods_path))
}

/// Outcome of a Locker model/sound swap — mirrors `PresetApplyOutcome` so the UI can
/// report the same "refreshed live in-game" feedback the presets flow gives.
#[derive(serde::Serialize)]
struct SwapApplyOutcome {
    content_reload: ReloadOutcome,
    game_running: bool,
    live_refresh: gameproc::LiveRefresh,
    /// Model swaps only (`None` for sound). Nothing here makes the mesh appear live —
    /// `live_refresh` re-runs the *customization* loader (paints/gear, never the mesh)
    /// and the verb below is only a notice. See `frostmod::signal_refresh_model`.
    model_refresh: Option<frostmod::CommandOutcome>,
    /// Liveries the swap couldn't move into or out of `paints/`, because MX Bikes holds
    /// bike files open while it runs. Zero on every other path. See
    /// `modelswap::reconcile_paints`.
    paints_stuck: usize,
}

/// Outcome of switching ReShade preset.
///
/// Much thinner than [`SwapApplyOutcome`] because switching a preset touches nothing this app
/// owns: no content to reload, no loader to re-run. ReShade picks the file up itself, so the
/// only thing the UI can't work out on its own is whether a session is already running and
/// therefore whether the player sees it now or next launch.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReshadeApplyOutcome {
    game_running: bool,
}

/// Re-run the game's look loader live if instant refresh is enabled, else report it off.
fn live_refresh(enabled: bool) -> gameproc::LiveRefresh {
    if enabled {
        gameproc::refresh_look()
    } else {
        gameproc::LiveRefresh::Disabled
    }
}

/// Shortest gap between two unattended look refreshes.
///
/// Every refresh is a thread started inside the running game, and the watcher that drives
/// them fires without anyone asking. A painter saving repeatedly, or a sync pull landing
/// half a grid's paints, would otherwise queue one call per event; this collapses that
/// burst into one. Sized to outlast a save the debounce didn't already fold together,
/// while still being imperceptible to someone waiting to see their paint.
const LIVE_LOOK_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(2);

/// When the last unattended refresh went out. Not shared with the apply paths — a refresh
/// the player asked for by clicking is never worth withholding.
static LAST_LIVE_LOOK: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);

/// Has the cooldown passed? Records the attempt when it has, so two callers racing here
/// produce one refresh.
fn live_look_cooldown_passed() -> bool {
    let now = std::time::Instant::now();
    let mut last = LAST_LIVE_LOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *last {
        if now.duration_since(at) < LIVE_LOOK_COOLDOWN {
            return false;
        }
    }
    *last = Some(now);
    true
}

/// Can a look change reach the running game at all?
///
/// Two things have to hold, and both are fixed for the life of the process. The title needs
/// a loader offset ([`game::Caps::instant_refresh`]), and the call that uses it is Windows'
/// alone — under Wine or Proton the game is a Windows binary but we are not the one that can
/// start a thread in it. Asked before watching as well as before firing, so a platform that
/// could never act on a save doesn't hold OS watch handles waiting for one.
fn can_refresh_live_look() -> bool {
    cfg!(windows) && game::active().caps.instant_refresh
}

/// Push a look that changed on disk into the running game.
///
/// The trigger nobody clicked: a `.pnt` rewritten under the player's feet, or paints pulled
/// from the control plane mid-session. Everything else about it is the apply paths' refresh
/// — the same loader call, gated on the same Instant refresh setting, because that setting
/// already means "put look changes into the live game" and this is another way one arrives.
///
/// Silent when there is nothing to do: no game, no setting, or a title whose loader we don't
/// have an offset for. Only a real attempt is logged, so the log answers "did it fire, and
/// what did the game say" — which is the question a first Windows run has to settle.
fn refresh_live_look(app: &tauri::AppHandle) {
    if !can_refresh_live_look() {
        return;
    }
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if !cfg.instant_refresh || !gameproc::is_game_running() {
        return;
    }
    if !live_look_cooldown_passed() {
        log::debug!("[look] a refresh went out moments ago; folding this one into it");
        return;
    }
    log::info!("[look] refreshing the live game: {:?}", gameproc::refresh_look());
}

/// The `.pnt` files the game is wearing right now — the bike's own paint and font, and every
/// piece of gear on the rider.
///
/// Read through the same resolver an upload uses, so a paint packed in a `.pkz`, sitting
/// loose beside it, or living under the rider profile is found the same way here as
/// everywhere else. [`bundle::plan_detailed`] rather than `plan`, for the reason Manage
/// needs it too: `plan` collapses a gear paint into the model folder that contains it, and a
/// folder is not a file to watch. Only the *active* bike — the others aren't on screen, and
/// re-running the game's loader for a paint nobody can see is a thread started for nothing.
///
/// Empty whenever the look can't be read — no profile, no bike, an unreadable `profile.ini`.
/// That stops the watcher rather than failing anything; the next `profile.ini` write rebuilds
/// it.
fn worn_paints(cfg: &AppConfig) -> Vec<String> {
    let profiles_dir = cfg.profiles_dir();
    let Some(profile) = sync_profile(cfg) else {
        return Vec::new();
    };
    let Some(bike) = presets::active_bike(&profiles_dir, &profile) else {
        return Vec::new();
    };
    let Ok(loadout) = presets::read_loadout(&profiles_dir, &profile, &bike) else {
        return Vec::new();
    };
    let Ok(plan) = bundle::plan_detailed(cfg, &loadout, Some(&bike)) else {
        return Vec::new();
    };
    plan.assets
        .iter()
        .filter(|a| !a.is_dir && paintsync::is_paint(std::path::Path::new(&a.abs_path)))
        .map(|a| a.abs_path.clone())
        .collect()
}

/// Point the look watcher at whatever the rider is wearing now, replacing what it watched
/// before. Called from every path that can change the answer, and cheap enough to be: one
/// `profile.ini` parse and one library walk.
fn watch_worn_paints(app: &tauri::AppHandle) {
    if !can_refresh_live_look() {
        return;
    }
    let cfg = config::load_or_detect(app).unwrap_or_default();
    let paths = worn_paints(&cfg);
    let handle = app.clone();
    paintwatch::start_with(
        &app.state::<LookWatcher>().0,
        "look watcher",
        &paths,
        move |_changed| refresh_live_look(&handle),
    );
}

/// Tell FrostMod that `bike`'s model changed, so it can say so in-game. `None` when
/// instant refresh is off — the same switch that gates `live_refresh`, since both
/// reach into the running game.
///
/// It does not make the model appear: FrostMod v0.9.11 removed the live re-apply (it
/// crashed the game), so the player still switches bike category away and back — that
/// is what re-reads the model; reselecting the same bike does not. All this buys is
/// the in-game notice.
///
/// The tag our installer recorded decides whether the command goes out at all. It used
/// to be sent unconditionally and only the *wording* adjusted afterwards, because the
/// worst an old FrostMod did was log an unknown verb and drop it. That is no longer the
/// worst: FrostMod v0.9.9 acts on the verb by replaying a bike-apply call it captured
/// earlier, which corrupts the game's bike state and crashes it to desktop at the next
/// bike the player picks by hand. So the check moved *before* the send — nothing is
/// written to the command file and no event is pulsed for a build we don't trust.
/// See `frostmod::MODEL_REFRESH_MIN_VERSION`.
fn model_refresh_cmd(
    app: &tauri::AppHandle,
    enabled: bool,
    bike: &str,
) -> Option<frostmod::CommandOutcome> {
    if !enabled {
        return None;
    }
    let tag = frostmod_manage::installed_version(app);
    if !frostmod::model_refresh_is_safe(tag.as_deref()) {
        return Some(frostmod::CommandOutcome::Withheld);
    }
    Some(frostmod::signal_refresh_model(bike))
}

/// The bike folders a model set could be moved to.
#[tauri::command]
async fn bike_folders(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(modelswap::bike_folders(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("bike_folders task failed: {e}"))?
}

/// The liveries a model owns outright — what a move offers to take with it.
#[tauri::command]
async fn model_swap_liveries(
    app: tauri::AppHandle,
    bike: String,
    variant: String,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(modelswap::liveries_owned_by(&cfg.mods_path, &bike, &variant))
    })
    .await
    .map_err(|e| format!("model_swap_liveries task failed: {e}"))?
}

#[tauri::command]
async fn move_model_swap(
    app: tauri::AppHandle,
    bike: String,
    variant: String,
    to_bike: String,
    carry: Vec<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        modelswap::move_model_swap(&cfg.mods_path, &bike, &variant, &to_bike, &carry)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("move_model_swap task failed: {e}"))?
}

#[tauri::command]
async fn delete_model_swap(
    app: tauri::AppHandle,
    bike: String,
    variant: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        modelswap::delete_model_swap(&cfg.mods_path, &bike, &variant)
            .map(|_| ())
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("delete_model_swap task failed: {e}"))?
}

#[tauri::command]
async fn apply_model_swap(
    app: tauri::AppHandle,
    bike: String,
    target: String,
) -> Result<SwapApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || apply_model_swap_blocking(app, bike, target))
        .await
        .map_err(|e| format!("apply_model_swap task failed: {e}"))?
}

fn apply_model_swap_blocking(
    app: tauri::AppHandle,
    bike: String,
    target: String,
) -> Result<SwapApplyOutcome, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let prev = modelswap::current_active(&cfg.mods_path, &bike);
    let paints_stuck = modelswap::apply_model_swap_reporting(&cfg.mods_path, &bike, &target)
        .map_err(|e| format!("{e:#}"))?;
    // Make a bound sound travel with the model (case 2); independent sounds are left
    // untouched (case 1). Best-effort — the model swap itself already succeeded.
    if let Err(e) = soundmods::reconcile_after_model_swap(&cfg.mods_path, &bike, &prev, &target) {
        eprintln!("sound reconcile after model swap failed: {e:#}");
    }
    let content_reload = frostmod::signal_reload();
    // Tell FrostMod the model changed so it prompts the player in-game. Gated on the same
    // instant-refresh setting as the look refresh — both poke the live game.
    let model_refresh = model_refresh_cmd(&app, cfg.instant_refresh, &bike);
    // A different model can resolve a slot to a different file, so the look watcher has to
    // follow the swap — nothing writes `profile.ini` here for it to notice on its own.
    watch_worn_paints(&app);
    Ok(SwapApplyOutcome {
        content_reload,
        game_running: gameproc::is_game_running(),
        live_refresh: live_refresh(cfg.instant_refresh),
        model_refresh,
        paints_stuck,
    })
}

/// Every livery the bike has, wherever it currently sits — the loose `paints/` folder and
/// the shelf both — so the assignment picker lists a livery it has already shelved.
///
/// Reconciles first, which is what adopts liveries stranded inside a model-swap folder: a
/// livery the picker can't see is one nobody can assign, and adoption is the only thing
/// that moves them somewhere the picker looks. Deliberately here and not in `scan_*` —
/// this is a single bike the user has just opened the picker for, where a scan runs over
/// the whole tree on every refresh and has no business moving files.
#[tauri::command]
async fn list_bike_liveries(app: tauri::AppHandle, bike: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        modelswap::reconcile_paints(&cfg.mods_path, &bike);
        Ok(modelswap::bike_liveries(&cfg.mods_path, &bike))
    })
    .await
    .map_err(|e| format!("list_bike_liveries task failed: {e}"))?
}

/// Set which liveries a model swap owns, then move the folder to match.
#[tauri::command]
async fn set_model_paints(
    app: tauri::AppHandle,
    bike: String,
    model: String,
    paints: Vec<String>,
) -> Result<SwapApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let paints_stuck = modelswap::set_model_paints(&cfg.mods_path, &bike, &model, &paints)
            .map_err(|e| format!("{e:#}"))?;
        // Liveries moved in or out of `paints/`, which is exactly what the customization
        // loader reads — same refresh the Locker's swaps ask for.
        let content_reload = frostmod::signal_reload();
        watch_worn_paints(&app);
        Ok(SwapApplyOutcome {
            content_reload,
            game_running: gameproc::is_game_running(),
            live_refresh: live_refresh(cfg.instant_refresh),
            model_refresh: None, // the mesh didn't change, only which liveries sit beside it
            paints_stuck,
        })
    })
    .await
    .map_err(|e| format!("set_model_paints task failed: {e}"))?
}

#[tauri::command]
async fn scan_sound_swaps(app: tauri::AppHandle) -> Result<Vec<soundmods::BikeSounds>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(soundmods::scan_sound_swaps(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("scan_sound_swaps task failed: {e}"))?
}

#[tauri::command]
async fn apply_sound_swap(
    app: tauri::AppHandle,
    bike: String,
    target: String,
) -> Result<SwapApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        soundmods::apply_sound_swap(&cfg.mods_path, &bike, &target).map_err(|e| format!("{e:#}"))?;
        let content_reload = frostmod::signal_reload();
        Ok(SwapApplyOutcome {
            content_reload,
            game_running: gameproc::is_game_running(),
            live_refresh: live_refresh(cfg.instant_refresh),
            model_refresh: None, // a sound swap doesn't touch the model
            paints_stuck: 0,     // nor the liveries
        })
    })
    .await
    .map_err(|e| format!("apply_sound_swap task failed: {e}"))?
}

/// The ReShade card's whole state, read fresh from the folder ReShade lives in — see
/// [`reshade`].
#[tauri::command]
async fn reshade_status(app: tauri::AppHandle) -> Result<reshade::Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let mut status = reshade::status(&cfg.reshade_dir());
        // Only this side knows where that folder came from, and the card has to say so
        // before it offers to hand the folder back to the game's install dir.
        status.custom = !cfg.reshade_path.trim().is_empty();
        Ok(status)
    })
    .await
    .map_err(|e| format!("reshade_status task failed: {e}"))?
}

/// Point the ReShade card at a folder of the player's choosing. An empty string clears the
/// override, back to the game's install dir.
///
/// The pick is taken as given rather than validated: a folder with no ReShade in it is a
/// perfectly ordinary thing to land on mid-setup, and `reshade_status` says so plainly on
/// the very next read. Refusing it would leave the player with a dialog and no way to see
/// what the app thinks is wrong.
#[tauri::command]
fn set_reshade_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.reshade_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn apply_reshade_preset(
    app: tauri::AppHandle,
    name: String,
) -> Result<ReshadeApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        reshade::apply(&cfg.reshade_dir(), &name).map_err(|e| format!("{e:#}"))?;
        // Unlike a content swap there is nothing to signal: ReShade owns its own config and
        // FrostMod has no part in it. All the UI needs is whether the player will see this
        // now or on the next launch.
        Ok(ReshadeApplyOutcome {
            game_running: gameproc::is_game_running(),
        })
    })
    .await
    .map_err(|e| format!("apply_reshade_preset task failed: {e}"))?
}

#[tauri::command]
async fn delete_reshade_preset(app: tauri::AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        reshade::delete(&cfg.reshade_dir(), &name).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("delete_reshade_preset task failed: {e}"))?
}

#[tauri::command]
async fn bind_sound(app: tauri::AppHandle, bike: String, model: String, sound: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        soundmods::bind_sound(&cfg.mods_path, &bike, &model, &sound).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("bind_sound task failed: {e}"))?
}

#[tauri::command]
async fn unbind_sound(app: tauri::AppHandle, bike: String, model: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        soundmods::unbind_sound(&cfg.mods_path, &bike, &model).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("unbind_sound task failed: {e}"))?
}

#[tauri::command]
async fn detect_loose_swaps(app: tauri::AppHandle) -> Result<Vec<modelswap::LooseSwapBike>, String> {
    tauri::async_runtime::spawn_blocking(move || detect_loose_swaps_blocking(app))
        .await
        .map_err(|e| format!("detect_loose_swaps task failed: {e}"))?
}

fn detect_loose_swaps_blocking(app: tauri::AppHandle) -> Result<Vec<modelswap::LooseSwapBike>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(modelswap::detect_loose_swaps(&cfg.mods_path))
}

#[tauri::command]
async fn register_loose_swaps(
    app: tauri::AppHandle,
    move_files: bool,
) -> Result<modelswap::RegisterReport, String> {
    tauri::async_runtime::spawn_blocking(move || register_loose_swaps_blocking(app, move_files))
        .await
        .map_err(|e| format!("register_loose_swaps task failed: {e}"))?
}

fn register_loose_swaps_blocking(
    app: tauri::AppHandle,
    move_files: bool,
) -> Result<modelswap::RegisterReport, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    modelswap::register_loose_swaps(&cfg.mods_path, move_files).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn detect_orphaned_setup(
    app: tauri::AppHandle,
) -> Result<Vec<modelswap::OrphanedSetup>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(modelswap::detect_orphaned_setup(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("detect_orphaned_setup task failed: {e}"))?
}

#[tauri::command]
async fn repair_orphaned_setup(app: tauri::AppHandle, bike: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        modelswap::repair_orphaned_setup(&cfg.mods_path, &bike).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("repair_orphaned_setup task failed: {e}"))?
}



// ---------------------------------------------------------------------------
// Generating a track
// ---------------------------------------------------------------------------

/// What a generated track measures, so the studio can show it rather than assert it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackPreview {
    /// The `.pkz` written for the viewer. Terrain and surfaces only — no graphics, no
    /// scenery — so it previews and does not play.
    path: String,
    name: String,
    lap_m: f32,
    width_m: f32,
    features: usize,
    closure_m: f32,
    /// Height used against the budget it was given. A track using a tenth of its budget is
    /// quantising ten times coarser than it needs to.
    used_m: f32,
    budget_m: f32,
    /// The same measurements `trackstats` takes of published tracks.
    measured_width_m: f32,
    measured_length_m: f32,
    lips: usize,
    lips_per_km: f32,
    slope_p99_deg: f32,
    relief_p90_m: f32,
}

fn track_program(value: serde_json::Value) -> Result<trackprog::TrackProgram, String> {
    serde_json::from_value(value).map_err(|e| format!("that isn't a track program: {e}"))
}

/// Ask the control plane for a track program, and keep asking until it measures like a track.
///
/// The key lives there, not here. Everything that comes back is synthesised and measured
/// before this returns — see `trackllm` — so a program reaching the studio has already been
/// built once.
#[tauri::command]
async fn generate_track(app: tauri::AppHandle, brief: String) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let base = paintsync::control_plane();
    // A debug build pointed at a local control plane is someone testing this, and a local
    // `wrangler dev` has no accounts to enroll with. Anywhere else, the token is what says
    // whose Anthropic spend this is.
    let local = cfg!(debug_assertions) && !base.starts_with("https://");
    if cfg.cp_token.trim().is_empty() && !local {
        return Err(
            "Track generation goes through your MXB account — enroll with an invite code in \
             Settings first. To test against a local control plane, run `wrangler dev` in \
             control-plane/ with ANTHROPIC_API_KEY in .dev.vars and start the app with \
             MXB_CONTROL_PLANE=http://localhost:8787."
                .into(),
        );
    }
    let ask = trackllm::ControlPlane {
        base,
        token: cfg.cp_token.clone(),
    };
    // Four attempts: one to write it, one to fix the numbers, one for the thing the fix
    // broke, and one more because they are cheap now. Three was set when this called Opus at
    // $5/$25 per MTok; it calls Haiku at $1/$5, most of what used to come back wrong is
    // repaired without asking, and an attempt costs a fraction of a cent and twenty seconds.
    let prog = trackllm::generate(brief.trim(), &ask, 4)
        .await
        .map_err(|e| format!("{e:#}"))?;
    usage::track("track.generate");
    serde_json::to_value(&prog).map_err(|e| e.to_string())
}

/// A track to start from, without asking anyone for one.
///
/// The studio's first screen used to be a prompt and nothing else, which is a bad place to
/// start from when the model isn't configured — and a worse one when you just want to change
/// two jumps on something that already works.
#[tauri::command]
async fn base_track_program() -> Result<serde_json::Value, String> {
    serde_json::from_str::<trackprog::TrackProgram>(trackprog::EXAMPLE)
        .and_then(|p| serde_json::to_value(&p))
        .map_err(|e| format!("the built-in track didn't load: {e}"))
}

/// A whole track from a number, with no model in it.
///
/// The shape of a lap is geometry and geometry is checkable — it either closes, stays off
/// itself and carries the corners a published track carries, or it does not. That half needs
/// no model, and asking one for it costs a round trip and a key. So this walks a lap out of
/// the ground against the numbers in `scripts/track-survey.py`'s corpus and hands back a
/// programme the studio can edit like any other.
///
/// A seed is a *lap*, not an attempt: [`tracklayout::draw`] already retries the walk six times
/// inside one seed, and a seed it still paints itself in on is skipped rather than reported —
/// which is why this takes a seed and searches forward from it. Roughly nine seeds in ten
/// give a lap on the first try.
#[tauri::command]
async fn random_track_program(seed: Option<u64>) -> Result<serde_json::Value, String> {
    let from = seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
    });
    let prog = tauri::async_runtime::spawn_blocking(move || {
        (0..24u64).find_map(|i| tracklayout::draw(from.wrapping_add(i)))
    })
    .await
    .map_err(|e| format!("random_track_program task failed: {e}"))?
    .ok_or_else(|| "the walk painted itself in on every seed it tried".to_string())?;
    serde_json::to_value(&prog).map_err(|e| e.to_string())
}

/// A lap with nothing on it: somewhere to start from scratch.
///
/// Answered from the type, not from the source text, exactly as the base track is. The
/// literal leaves out every field that has a default — `blend`, `elevation`, the ground's
/// `wear` — and handing those absences to the studio put an undefined into a number field
/// the moment a blank track loaded.
#[tauri::command]
async fn blank_track_program() -> Result<serde_json::Value, String> {
    serde_json::from_str::<trackprog::TrackProgram>(trackprog::BLANK)
        .and_then(|p| serde_json::to_value(&p))
        .map_err(|e| format!("the blank track didn't load: {e}"))
}

/// Give a programme a height budget that fits it.
///
/// The budget only exists because samples are quantised against it, and there is no reason a
/// person should be told to guess a number the synthesiser already knows.
#[tauri::command]
async fn fit_track_budget(program: serde_json::Value) -> Result<serde_json::Value, String> {
    let prog = track_program(program)?;
    let fitted = tauri::async_runtime::spawn_blocking(move || {
        tracksynth::with_fitted_budget(&prog).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("fit_track_budget task failed: {e}"))??;
    serde_json::to_value(&fitted).map_err(|e| e.to_string())
}

/// Bring an open lap back to its start.
#[tauri::command]
async fn close_track_lap(program: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut prog = track_program(program)?;
    // A turn no tighter than the lap's own tightest, so the join doesn't need a corner
    // sharper than anything already on the track.
    let radius = prog
        .segments
        .iter()
        .filter_map(|s| match s {
            trackprog::Segment::Arc { radius, .. } => Some(radius.abs()),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    let radius = if radius.is_finite() { radius } else { 25.0 };
    match prog.closing_segments(radius) {
        Some(add) => prog.segments.extend(add),
        None => return Err("The lap already meets itself.".into()),
    }
    serde_json::to_value(&prog).map_err(|e| e.to_string())
}

/// Everything wrong with a program, without asking anyone. The studio calls this as edits are
/// made, so a hand-edited track is held to the same corpus a generated one is.
#[tauri::command]
async fn check_track(program: serde_json::Value) -> Result<trackllm::Review, String> {
    let prog = track_program(program)?;
    tauri::async_runtime::spawn_blocking(move || trackllm::review(&prog))
        .await
        .map_err(|e| format!("check_track task failed: {e}"))
}

/// Build a program into terrain and write it where the track viewer can open it.
#[tauri::command]
async fn preview_track(
    app: tauri::AppHandle,
    program: serde_json::Value,
) -> Result<TrackPreview, String> {
    let prog = track_program(program)?;
    tauri::async_runtime::spawn_blocking(move || {
        let syn = tracksynth::synthesise(&prog).map_err(|e| format!("{e:#}"))?;
        let dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("no cache directory: {e}"))?
            .join("track-preview");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
        // One file, overwritten. A studio session generates many tracks and none of them are
        // worth keeping until someone installs one.
        let path = dir.join("preview.pkz");
        tracksynth::write_pkz(&prog, &syn, &path, true).map_err(|e| format!("{e:#}"))?;

        let c = trackstats::measure("synth", &syn.corridor, &syn.heights, syn.gw, syn.gh, syn.mps);
        Ok(TrackPreview {
            path: path.to_string_lossy().into_owned(),
            name: prog.name.clone(),
            lap_m: prog.lap_length(),
            width_m: prog.width,
            features: prog.features.len(),
            closure_m: prog.closure_error(),
            used_m: syn.used_m,
            budget_m: syn.budget_m,
            measured_width_m: c.width_from_mean_m,
            measured_length_m: c.length_m,
            lips: c.lips,
            lips_per_km: c.lips_per_km,
            slope_p99_deg: c.slope_deg.p99,
            relief_p90_m: c.feature_relief_m.p90,
        })
    })
    .await
    .map_err(|e| format!("preview_track task failed: {e}"))?
}

/// Where an installed track goes: the mods tree's `tracks` folder.
///
/// Not `game_path` — that is the folder with the executable in it, which the game never
/// reads content from, and which is empty on a machine that only has the mods folder
/// configured. And `mods` is resolved rather than joined on: a player who relocated the
/// tree with `mxbikes.ini` has `mods_path` already pointing at it.
fn track_install_dir(cfg: &AppConfig) -> Result<std::path::PathBuf, String> {
    if cfg.mods_path.trim().is_empty() {
        return Err(format!(
            "No {} folder is configured yet — set it in Settings.",
            cfg.game().display
        ));
    }
    Ok(library::mods_subdir(&cfg.mods_path, "mods/tracks"))
}

/// Write the folder TerrainEd compiles: the heightmap, the masks, and every config file.
#[tauri::command]
async fn export_track_source(
    program: serde_json::Value,
    dir: String,
) -> Result<Vec<String>, String> {
    let prog = track_program(program)?;
    tauri::async_runtime::spawn_blocking(move || {
        let syn = tracksynth::synthesise(&prog).map_err(|e| format!("{e:#}"))?;
        tracksynth::write_source(&prog, &syn, std::path::Path::new(&dir))
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("export_track_source task failed: {e}"))?
}


/// Whether the app can compile a track here, and what with.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackToolsStatus {
    /// The folder someone pointed at, if they have.
    path: String,
    /// Whether `terrained.exe` was actually found in it.
    found: bool,
    /// Whether `tracked.exe` was too — without it the terrain still builds, it just has no
    /// centreline yet.
    has_tracked: bool,
}

#[tauri::command]
async fn track_tools_status(app: tauri::AppHandle) -> Result<TrackToolsStatus, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let path = cfg.track_tools_path.clone();
    let tools = (!path.trim().is_empty())
        .then(|| trackbuild::find(std::path::Path::new(&path)))
        .flatten();
    Ok(TrackToolsStatus {
        path,
        found: tools.is_some(),
        has_tracked: tools.map(|t| t.tracked.is_some()).unwrap_or(false),
    })
}

/// Remember where PiBoSo's track editing tools live.
#[tauri::command]
async fn set_track_tools(app: tauri::AppHandle, dir: String) -> Result<TrackToolsStatus, String> {
    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.track_tools_path = dir.trim().to_string();
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    track_tools_status(app).await
}

/// Where PiBoSo's track tools live once the app has fetched them.
fn track_tools_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))?
        .join("track-tools"))
}

/// Fetch PiBoSo's track tools, so nobody has to leave the app to find them.
///
/// They are a public download and not ours to ship, so the app gets them on request rather
/// than carrying them. This was the last step of building a track that needed a browser.
#[tauri::command]
async fn download_track_tools(app: tauri::AppHandle) -> Result<TrackToolsStatus, String> {
    const URL: &str = "https://www.kartracing-pro.com/downloads/tt.zip";
    let dir = track_tools_dir(&app)?;

    let bytes = reqwest::Client::new()
        .get(URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't reach {URL}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("the download stopped early: {e}"))?;

    let extracted = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("that download isn't a zip: {e}"))?;
        // Flat: the archive nests each tool in its own folder, and `find` looks one level
        // down anyway — but the fonts `tracked.exe` reads sit beside it, so keep the shape.
        zip.extract(&dir).map_err(|e| format!("{e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("download_track_tools task failed: {e}"))?;
    extracted?;

    let dir = track_tools_dir(&app)?;
    if trackbuild::find(&dir).is_none() {
        return Err("the download arrived but there's no terrained.exe in it".into());
    }
    set_track_tools(app, dir.to_string_lossy().into_owned()).await
}

/// The compilers, fetched if this machine hasn't got them yet.
///
/// A track is only a track once `terrained.exe` has been over it — there is no second way to
/// produce a `.map` the game will ride. So the download belongs to the build rather than to a
/// step someone has to know to take first.
async fn ensure_track_tools(app: &tauri::AppHandle) -> Result<String, String> {
    let at = config::load_or_detect(app)
        .unwrap_or_default()
        .track_tools_path;
    if !at.trim().is_empty() && trackbuild::find(std::path::Path::new(&at)).is_some() {
        return Ok(at);
    }
    let got = download_track_tools(app.clone()).await?;
    if !got.found {
        return Err("PiBoSo's track tools downloaded but there's no terrained.exe in them".into());
    }
    Ok(got.path)
}

/// How far a track build has got.
///
/// A build is minutes of work with nothing to look at, so it reports where it is rather than
/// only what it produced. Keyed by slug: the studio's bar belongs to one track, and a second
/// build must not drive the first one's.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildProgress {
    slug: String,
    #[serde(flatten)]
    at: trackbuild::Progress,
}

/// The event a build reports itself on.
const BUILD_EVENT: &str = "track-build-progress";

/// Everything a build produced, and where it ended up.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildResult {
    /// Each compiler run, in order.
    steps: Vec<trackbuild::Step>,
    /// The folder the source and the compiled files are in.
    dir: String,
    /// The archive, once every step has succeeded.
    pkz: Option<String>,
    /// Where it was installed, when it was asked for and worked.
    installed: Option<String>,
}

/// Export a track, run the compilers over it, and put the result where the game reads it.
///
/// The whole way, because a folder of source is homework and a compiled folder is still
/// homework — a track you can ride is a `.pkz` in the mods tree. The compilers are PiBoSo's
/// and Windows-only; on macOS they run through the same Wine prefix the game does.
#[tauri::command]
async fn build_track(
    app: tauri::AppHandle,
    program: serde_json::Value,
    dir: Option<String>,
    install: bool,
) -> Result<BuildResult, String> {
    let prog = track_program(program)?;
    let tools_at = ensure_track_tools(&app).await?;
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let slug = tracksynth::slug(&prog.name);
    // Somewhere of its own when nobody picked a folder, so building is one press.
    let root = match dir {
        Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d),
        _ => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("no data directory: {e}"))?
            .join("track-builds")
            .join(&slug),
    };
    let tracks = install.then(|| track_install_dir(&cfg)).transpose()?;

    tauri::async_runtime::spawn_blocking(move || {
        let tools = trackbuild::find(std::path::Path::new(&tools_at))
            .ok_or("There's no terrained.exe in that folder.".to_string())?;
        let mut plan = trackbuild::Plan::new(
            prog.terrain.samples,
            tools.tracked.is_some(),
            tracks.is_some(),
        );
        let slug_for_events = slug.clone();
        let say = |at: trackbuild::Progress| {
            let _ = app.emit(
                BUILD_EVENT,
                BuildProgress { slug: slug_for_events.clone(), at },
            );
        };

        say(plan.start("synthesising"));
        let syn = tracksynth::synthesise(&prog).map_err(|e| format!("{e:#}"))?;

        say(plan.start("writing"));
        std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        tracksynth::write_source(&prog, &syn, &root).map_err(|e| format!("{e:#}"))?;

        let steps = trackbuild::compile(&tools, &root, &slug, &cfg.game_path, &mut |phase| {
            say(plan.start(phase))
        })
        .map_err(|e| format!("{e:#}"))?;

        let mut out = BuildResult {
            dir: root.to_string_lossy().into_owned(),
            pkz: None,
            installed: None,
            steps,
        };
        // Only a build that got all the way through is worth packaging: a `.pkz` missing its
        // `.map` is a track the game lists and then refuses to load.
        if out.steps.iter().all(|s| s.ok) {
            say(plan.start("packaging"));
            let pkz = root.join(format!("{slug}.pkz"));
            trackbuild::package(&root, &slug, &pkz).map_err(|e| format!("{e:#}"))?;
            out.pkz = Some(pkz.to_string_lossy().into_owned());
            if let Some(tracks) = tracks {
                say(plan.start("installing"));
                let at = trackbuild::install(&pkz, &tracks).map_err(|e| format!("{e:#}"))?;
                usage::track("track.build.install");
                out.installed = Some(at.to_string_lossy().into_owned());
            }
        }
        // Closes the last phase, so what it cost is remembered and the next build is paced
        // by this machine rather than by the one the defaults were measured on.
        plan.finish();
        Ok(out)
    })
    .await
    .map_err(|e| format!("build_track task failed: {e}"))?
}


/// The models a track ships that a prop can be placed by name.
#[tauri::command]
async fn read_track_placeable(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scenery::placeable(&path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("read_track_placeable task failed: {e}"))?
}

/// One prop's mesh, so it can be drawn where it is about to go.
#[tauri::command]
async fn load_track_prop(
    path: String,
    name: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || match scenery::prop_mesh(&path, &name) {
        Ok(m) => tauri::ipc::Response::new(map::scenery_blob(&m, &[])),
        Err(e) => {
            log::debug!("[scenery] prop {name}: {e:#}");
            tauri::ipc::Response::new(Vec::new())
        }
    })
    .await
    .map_err(|e| format!("load_track_prop task failed: {e}"))
}

/// Bake a track you own into a prop library the generator can place.
///
/// A generated track otherwise stands on a kit we author ourselves. A library lifts a real
/// venue's objects — its tents, trailers, buildings, poles and trees — and replays them
/// against our own centreline, so a generated lap gets a paddock rather than a field.
///
/// It reads a track archive already installed and writes one file into the app's own data
/// folder. Baking takes about ten seconds and only has to happen once.
#[tauri::command]
async fn bake_prop_library(path: String, sheet_max: Option<u32>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use trackobjects::Class;
        let donor = trackprops::open(std::path::Path::new(&path))
            .map_err(|e| format!("{path}: {e:#}"))?;
        let mut lib = trackprops::extract(
            &donor,
            &[Class::Structure, Class::Vehicle, Class::Tree, Class::Bale, Class::Pole],
        );
        if lib.props.is_empty() {
            return Err(format!("{} carries no objects to lift", donor.stem));
        }
        trackprops::sheets_for(&donor, &mut lib, sheet_max.unwrap_or(1024).clamp(64, 4096));

        let out = dirs_next::data_local_dir()
            .ok_or("no app data folder")?
            .join(APP_IDENTIFIER)
            .join("props");
        std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        let file = out.join("library.fpl");
        let bytes = lib.encode();
        std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
        log::info!(
            "[props] baked {} props / {} instances from {} -> {:.1} MB",
            lib.props.len(),
            lib.instances.len(),
            donor.stem,
            bytes.len() as f32 / 1_048_576.0
        );
        Ok(format!(
            "{} props from {} placed objects, {:.1} MB",
            lib.props.len(),
            lib.instances.len(),
            bytes.len() as f32 / 1_048_576.0
        ))
    })
    .await
    .map_err(|e| format!("bake_prop_library task failed: {e}"))?
}

/// Save a track's props to a `.scr` the game will load.
///
/// The `.scr` is the one part of a track that states where a thing goes in plain text, so it
/// is where anything placed in the app has to end up. Writes only where it is told, never
/// inside an archive, and refuses to replace a file unless asked — a track's own `.scr` is
/// the record of however long someone spent placing things.
#[tauri::command]
async fn save_track_props(
    target: String,
    props: Vec<scenery::Placement>,
    overwrite: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        scenery::save_scr(&target, &props, overwrite).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("save_track_props task failed: {e}"))?
}

#[tauri::command]
async fn unpack_paint(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_paint_blocking(path))
        .await
        .map_err(|e| format!("unpack_paint task failed: {e}"))?
}

/// Paints decoded for the viewer, so re-opening one doesn't inflate it a second time.
///
/// The picker re-runs this on every selection change and on every re-open, and a gear paint is
/// tens of megabytes of DEFLATE — the pixels behind an entry, on the other hand, are small,
/// because each is downscaled to 1024² before it is stored.
const PAINT_CACHE_CAP: usize = 4;

fn paint_cache() -> &'static std::sync::Mutex<lru::Lru<Vec<paint::PaintTexture>>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<lru::Lru<Vec<paint::PaintTexture>>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(PAINT_CACHE_CAP)))
}

/// As [`cached_bike`], for the paints: looked up and released without holding the lock.
fn cached_paint(key: &str) -> Option<Vec<paint::PaintTexture>> {
    paint_cache().lock().ok().and_then(|mut c| c.get(key).cloned())
}

fn unpack_paint_blocking(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    let t0 = std::time::Instant::now();
    // Path *and* mtime, as the bike cache does, so a paint re-saved under the same name misses.
    let key = viewer::bike_cache_key(&path);
    if let Some(t) = cached_paint(&key) {
        log::info!("unpack_paint {path}: cache hit ({:?})", t0.elapsed());
        return Ok(t);
    }
    let _gate = gate::enter(&key);
    if let Some(t) = cached_paint(&key) {
        log::info!("unpack_paint {path}: cache hit, waited ({:?})", t0.elapsed());
        return Ok(t);
    }

    let textures = paint::unpack_file(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))?;
    log::info!(
        "unpack_paint {path}: {} texture(s) in {:?} | {:.1} MB resident in the texture store",
        textures.len(),
        t0.elapsed(),
        texstore::resident_bytes() as f64 / (1024.0 * 1024.0),
    );
    if let Ok(mut c) = paint_cache().lock() {
        // Cloning an entry copies names, sizes and tokens — never pixels, which stay in the
        // texture store. The displaced paint's go with it; nothing else holds those tokens.
        if let Some(dropped) = c.insert(key, textures.clone()) {
            let tokens: Vec<String> = dropped.iter().map(|t| t.token.clone()).collect();
            texstore::release(&tokens);
        }
    }
    Ok(textures)
}

// ── Paint studio ────────────────────────────────────────────────────────────────────
//
// A `.pnt` is a packed container no image editor can write, so a livery drawn in GIMP has
// always needed somebody else's converter before the game would load it. These commands are
// both halves of that: images in (`paint_studio_save`), sheets out as editable TGA
// templates (`paint_studio_extract`), and the texture names a destination expects
// (`paint_studio_hints`) so a new paint binds to the same parts as the ones already there.

/// Where a built paint is written.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PaintDest {
    /// Under the game's `mods` folder — `bikes/<Bike>/paints`,
    /// `rider/helmets/<Helmet>/paints`, `rider/riders/<Profile>/gloves`…
    Mods { rel: String },
    /// A folder the player picked themselves, for a paint they mean to share rather than
    /// install.
    Folder { path: String },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedPaint {
    path: String,
    textures: Vec<String>,
    bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PaintTarget {
    path: String,
    exists: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PaintTemplate {
    dir: String,
    files: Vec<String>,
    textures: Vec<String>,
}

/// Read source images for the studio — the pixels land in the texture store, so the UI
/// previews them through exactly the same path as a decoded paint's.
#[tauri::command]
async fn paint_studio_load(paths: Vec<String>) -> Result<Vec<paintstudio::StudioImage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| paintstudio::inspect(std::path::Path::new(p)).map_err(|e| format!("{e:#}")))
            .collect()
    })
    .await
    .map_err(|e| format!("paint_studio_load task failed: {e}"))?
}

/// Read one image at its full size, for the Designer to composite with.
///
/// Separate from `paint_studio_load` because that one answers "describe this file" with a
/// thumbnail, and the editor needs the pixels themselves — see `paintstudio::pixels`.
#[tauri::command]
async fn paint_studio_pixels(path: String) -> Result<paint::PaintTexture, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paintstudio::pixels(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("paint_studio_pixels task failed: {e}"))?
}

/// Stage a composited sheet, returning the file `paint_studio_save` should pack.
///
/// Takes the PNG as a raw request body rather than an argument: a 4096² sheet is megabytes,
/// and JSON would send it as a list of numbers. The sheet's texture name rides in a header
/// for the same reason — the body has to be the bytes and nothing else.
///
/// One staging directory per call. The caller saves immediately after staging every sheet, so
/// these are short-lived; they sit in the OS temp dir either way, which is where an editor
/// that's closed mid-flight should leave its scratch files.
#[tauri::command]
async fn paint_studio_stage(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("paint_studio_stage expects the sheet's PNG bytes as the request body".into());
    };
    let name = request
        .headers()
        .get("x-sheet-name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let png = png.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = install::staging_dir("paint");
        paintstudio::stage_sheet(&dir, &name, &png)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("paint_studio_stage task failed: {e}"))?
}

/// Write a photo of the 3D preview to a path the user picked in a save dialog.
///
/// Raw body and a header, for the same reason [`paint_studio_stage`] takes one: a 4K frame is
/// megabytes and JSON would send it as a list of numbers. The path is percent-encoded, because
/// a header has to be ASCII and a Windows user's pictures folder is under their name.
///
/// Nothing is resolved or relocated here — the dialog already asked, and the file goes exactly
/// where it said. A `.png` is enforced so a typed name can't quietly write PNG bytes to
/// something that isn't one.
#[tauri::command]
async fn photo_save(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(png) = request.body() else {
        return Err("photo_save expects the PNG bytes as the request body".into());
    };
    let raw = request
        .headers()
        .get("x-dest")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let dest = percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned();
    if dest.is_empty() {
        return Err("photo_save needs a destination".into());
    }
    let png = png.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut path = std::path::PathBuf::from(&dest);
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            path.set_extension("png");
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{dir:?}: {e}"))?;
        }
        std::fs::write(&path, &png).map_err(|e| format!("{path:?}: {e}"))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("photo_save task failed: {e}"))?
}

/// How big a `.psd` this will open. A 4096² sheet with a couple of dozen layers is well
/// inside this; the cap exists so a mistyped path at a 4 GB video doesn't try to cross the
/// IPC channel as one allocation.
const PSD_LIMIT: u64 = 512 * 1024 * 1024;

/// The bytes of a `.psd`, for the Designer to take apart in the webview.
///
/// Parsing happens up there rather than here, because that is where the pixels have to end
/// up: a layer becomes an `ImageBitmap` on a canvas, and a Rust-side decode would only mean
/// re-encoding every layer to cross back. So this is the whole of the backend's part —
/// hand over the file.
///
/// Restricted to the two Photoshop extensions on purpose. Nothing else has any business
/// being read wholesale into the webview, and a command that would do it for any path is a
/// wider door than this feature needs.
#[tauri::command]
async fn psd_read(path: String) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::PathBuf::from(&path);
        let ok = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("psd") || e.eq_ignore_ascii_case("psb"));
        if !ok {
            return Err(format!("{path:?} is not a .psd"));
        }
        let len = std::fs::metadata(&path).map_err(|e| format!("{path:?}: {e}"))?.len();
        if len > PSD_LIMIT {
            return Err(format!("{path:?} is {} MB — too large to open", len / (1024 * 1024)));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{path:?}: {e}"))?;
        Ok(tauri::ipc::Response::new(bytes))
    })
    .await
    .map_err(|e| format!("psd_read task failed: {e}"))?
}

/// Write one sheet's `.psd` to a path the user picked.
///
/// Same shape as [`photo_save`], and for the same reason: a 4096² document with its layers
/// still separate runs to tens of megabytes, so the file is the request body and the
/// destination rides in a percent-encoded header.
///
/// Nothing is resolved or relocated — the dialog already asked. The extension is enforced so
/// a typed name can't leave PSD bytes in a file Photoshop won't offer to open.
#[tauri::command]
async fn psd_save(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(psd) = request.body() else {
        return Err("psd_save expects the PSD bytes as the request body".into());
    };
    let raw = request
        .headers()
        .get("x-dest")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let dest = percent_encoding::percent_decode_str(raw).decode_utf8_lossy().into_owned();
    if dest.is_empty() {
        return Err("psd_save needs a destination".into());
    }
    let psd = psd.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut path = std::path::PathBuf::from(&dest);
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd")) {
            path.set_extension("psd");
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{dir:?}: {e}"))?;
        }
        std::fs::write(&path, &psd).map_err(|e| format!("{path:?}: {e}"))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("psd_save task failed: {e}"))?
}

/// The file a save would write, resolved but not written — so the UI can ask before
/// replacing a paint that's already there.
#[tauri::command]
fn paint_studio_target(
    app: tauri::AppHandle,
    file_name: String,
    dest: PaintDest,
) -> Result<PaintTarget, String> {
    let path = resolve_paint_dest(&app, &file_name, &dest)?;
    Ok(PaintTarget { exists: path.is_file(), path: path.to_string_lossy().into_owned() })
}

/// `<dir>/<name>.pnt` for a destination, refusing anything that isn't a paint sitting where
/// a paint belongs.
///
/// The `Mods` arm goes through [`paintsync::safe_dest`] — the same check that vets a
/// destination sent by another player over paint sync. Nothing here is remote, but the rule
/// it enforces (a relative path, no traversal, at least two segments deep, ending in
/// `.pnt`) is exactly the rule a paint destination has to satisfy, and one boundary with
/// tests beats a second one written from memory.
fn resolve_paint_dest(
    app: &tauri::AppHandle,
    file_name: &str,
    dest: &PaintDest,
) -> Result<std::path::PathBuf, String> {
    let stem = install::sanitize(file_name.trim())
        .trim()
        .trim_end_matches('.')
        .trim_end_matches(".pnt")
        .trim()
        .to_string();
    if stem.is_empty() {
        return Err("Name this paint before saving it.".into());
    }
    let file = format!("{stem}.pnt");
    match dest {
        PaintDest::Mods { rel } => {
            let cfg = config::load(app).map_err(|e| format!("{e:#}"))?;
            let mods_dir = library::mods_subdir(&cfg.mods_path, "mods");
            let rel = rel.replace('\\', "/");
            let rel = rel.trim_matches('/');
            paintsync::safe_dest(&mods_dir, &format!("{rel}/{file}"))
                .ok_or_else(|| format!("'{rel}' isn't a folder a paint can be installed into"))
        }
        PaintDest::Folder { path } => {
            let dir = std::path::PathBuf::from(path);
            if !dir.is_dir() {
                return Err(format!("{} isn't a folder", dir.display()));
            }
            Ok(dir.join(file))
        }
    }
}

/// Build a `.pnt` from the chosen images and write it.
#[tauri::command]
async fn paint_studio_save(
    app: tauri::AppHandle,
    name: String,
    file_name: String,
    textures: Vec<paintstudio::BuildTexture>,
    dest: PaintDest,
    overwrite: bool,
) -> Result<SavedPaint, String> {
    let target = resolve_paint_dest(&app, &file_name, &dest)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !overwrite && target.exists() {
            return Err(format!("{} is already there.", target.display()));
        }
        // The paint's own name is the one the game shows; an empty one falls back to the
        // file name, which is what every paint on disk is picked by anyway.
        let title = if name.trim().is_empty() {
            target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            name.trim().to_string()
        };
        let bytes = paintstudio::build(&title, &textures).map_err(|e| format!("{e:#}"))?;
        let names = paint::texture_names(&bytes).map_err(|e| format!("{e:#}"))?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        std::fs::write(&target, &bytes)
            .map_err(|e| format!("write {}: {e}", target.display()))?;
        log::info!(
            "[paint studio] wrote {} ({} bytes, textures: {})",
            target.display(),
            bytes.len(),
            names.join(", ")
        );
        usage::track("paint.save");
        Ok(SavedPaint {
            path: target.to_string_lossy().into_owned(),
            textures: names,
            bytes: bytes.len() as u64,
        })
    })
    .await
    .map_err(|e| format!("paint_studio_save task failed: {e}"))?
}

/// Write a paint's sheets out as `.tga` files to edit — the way to start from a livery
/// that already fits the model instead of from a blank sheet.
#[tauri::command]
async fn paint_studio_extract(
    app: tauri::AppHandle,
    path: String,
    dest: Option<String>,
) -> Result<PaintTemplate, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let src = std::path::PathBuf::from(&path);
        let stem = src
            .file_stem()
            .map(|s| install::sanitize(&s.to_string_lossy()))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "paint".to_string());
        let dir = match dest {
            Some(d) => std::path::PathBuf::from(d).join(&stem),
            None => templates_root(&app).join(&stem),
        };
        let bytes = std::fs::read(&src).map_err(|e| format!("read {}: {e}", src.display()))?;
        let files = paintstudio::extract(&bytes, &dir).map_err(|e| format!("{e:#}"))?;
        let textures = files
            .iter()
            .filter_map(|f| f.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect();
        Ok(PaintTemplate {
            dir: dir.to_string_lossy().into_owned(),
            files: files.iter().map(|f| f.to_string_lossy().into_owned()).collect(),
            textures,
        })
    })
    .await
    .map_err(|e| format!("paint_studio_extract task failed: {e}"))?
}

/// Where templates go when the player doesn't pick somewhere: their Documents folder, not
/// the mods folder — the game scans that, and a folder of loose sheets isn't a mod.
fn templates_root(app: &tauri::AppHandle) -> std::path::PathBuf {
    dirs_next::document_dir()
        .or_else(|| app.path().app_data_dir().ok())
        .unwrap_or_else(std::env::temp_dir)
        // NOT renamed with the product: this folder already exists on players' machines
        // and holds templates they exported. Moving it orphans them.
        .join("MXB App")
        .join("Paint Templates")
}

/// The texture names a destination can paint.
///
/// A paint binds by name: call a sheet `livery` and it lands on the bodywork that asked for
/// `livery`, call it `my_livery` and it lands nowhere. Two sources answer it, and both are
/// needed. The paints already installed name what *they* replace — read from their headers,
/// so that half costs no pixels. The model's own mesh names everything it draws, which is
/// the half a paint can't supply: the OEM bikes ship a stock paint that replaces the wheels
/// and the chain and nothing else, so on a stock Husqvarna the sheets on offer were `chain`,
/// `wheel`, `wheels` — and `plastics`, the one anybody opens the Designer for, was missing.
#[tauri::command]
async fn paint_studio_hints(app: tauri::AppHandle, rel: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let rel = rel.replace('\\', "/").trim_matches('/').to_string();
        Ok(paint_hints(&library::mods_subdir(&cfg.mods_path, &format!("mods/{rel}"))))
    })
    .await
    .map_err(|e| format!("paint_studio_hints task failed: {e}"))?
}

/// How many paints are read for their names. A handful is plenty: paints for one model
/// overwhelmingly supply the same names, and this runs every time the destination changes.
const PAINT_SAMPLE: usize = 8;

/// The texture names of the `.pnt` files sitting loose in `dir`, sampled.
fn loose_paint_names(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if seen >= PAINT_SAMPLE || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pnt")) {
            continue;
        }
        // Seeked, not read: a bike's paints are tens of megabytes each and the names are
        // in their headers. Reading eight of them whole put nineteen seconds between
        // picking a model and being told what it wants.
        if let Ok(found) = paint::texture_names_at(&p) {
            out.extend(found);
            seen += 1;
        }
    }
    out
}

/// [`paint_studio_hints`] for a destination folder that's already been resolved.
fn paint_hints(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    fn add(names: &mut Vec<String>, found: Vec<String>) {
        for n in found {
            if !n.is_empty() && !names.iter().any(|s| s.eq_ignore_ascii_case(&n)) {
                names.push(n);
            }
        }
    }

    add(&mut names, loose_paint_names(dir));
    // Nothing installed loose: the model may be packed, and its own paints are the
    // same evidence. `<Model>.pkz` sits beside the `<Model>` folder this destination
    // lives in — for a bike as much as for a helmet.
    if names.is_empty() {
        if let (Some(sub), Some(model_dir)) = (dir.file_name(), dir.parent()) {
            let pkz = library::sibling_pkz(model_dir);
            let tail = format!("/{}/", sub.to_string_lossy().to_ascii_lowercase());
            if pkz.is_file() {
                let want = |n: &str| {
                    let n = n.replace('\\', "/").to_ascii_lowercase();
                    n.contains(&tail) && n.ends_with(".pnt")
                };
                let packed = pkz::read_selected(&pkz, want).unwrap_or_default();
                for (_, bytes) in packed.iter().take(PAINT_SAMPLE) {
                    add(&mut names, paint::texture_names_any(bytes).unwrap_or_default());
                }
            }
        }
    }
    // A rider profile that ships its folders empty wears the stock profile's kits.
    //
    // `Rider+` and `Rider+RolledUp` do exactly that on purpose — the kits installed under
    // `default_mx` are meant to work on them, which is why `read_rider_paint_file` reaches
    // there to render one. The names are the same names, so the hints have to reach there
    // too, or painting for one of those profiles starts with nothing to call a sheet. It
    // also spares the walk over the profile's own mesh, which for `Rider+` is 67 MB of
    // rider read to learn what nine installed kits already say.
    if names.is_empty() {
        if let Some((sub, riders)) = dir.file_name().zip(dir.parent().and_then(|p| p.parent())) {
            if riders.file_name().is_some_and(|n| n.eq_ignore_ascii_case(game::RIDERS_DIR)) {
                for stock in game::active().rider.stock_profiles {
                    add(&mut names, loose_paint_names(&riders.join(stock).join(&sub)));
                    if !names.is_empty() {
                        break;
                    }
                }
            }
        }
    }
    // What the model itself draws, whether or not a paint has ever replaced it.
    //
    // Only for the model's own `paints` folder. A mesh names every texture on the item
    // without saying which of them belong to the goggles hanging off it — the paints are
    // the only thing that says that (see `on_goggle_side`) — and offering a helmet's shell
    // sheet to somebody painting its goggles would put the shell in the wrong file.
    let main_paints = dir.file_name().is_some_and(|s| s.eq_ignore_ascii_case("paints"));
    let mesh = match (main_paints, dir.parent()) {
        (true, Some(model_dir)) => mesh_texture_names(model_dir),
        _ => Vec::new(),
    };
    add(&mut names, mesh.clone());

    // Drop a name that is another paint's misspelling of one the model actually binds.
    //
    // A `.pnt` supplies textures by name, so a sheet the model never asks for changes
    // nothing — and these names come from paints as much as from the mesh, misspellings and
    // all. The KTM 250 SX-F binds `plastics_n`; a paint installed beside it calls its own
    // sheet `plastics-n`, and the two sat next to each other in the list, one character
    // apart, with the dead one first. Painting it is work that cannot reach the bike.
    //
    // Only a name that collides with a bound one is dropped, and only by separator or case.
    // A paint is free to ship sheets the mesh never mentions — `tyres` and `wheel` come off
    // the wheels rather than the bike — and those are left alone.
    if !mesh.is_empty() {
        let key = |s: &str| s.to_ascii_lowercase().replace('-', "_");
        let bound: std::collections::HashSet<String> = mesh.iter().map(|n| key(n)).collect();
        names.retain(|n| {
            mesh.iter().any(|m| m.eq_ignore_ascii_case(n)) || !bound.contains(&key(n))
        });
    }
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Every texture a model's own mesh carries, by name.
///
/// Read from the mesh's texture records — names and dimensions, never the pixels beside
/// them — so a 54 MB bike is answered by a walk over its bytes rather than by inflating it.
/// A model that ships as a folder is read from there, and one that ships packed from the
/// `<Model>.pkz` beside it; a sealed file is unwrapped the way the viewer unwraps it.
fn mesh_texture_names(model_dir: &std::path::Path) -> Vec<String> {
    let mut meshes: Vec<Vec<u8>> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(model_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(bikefiles::is_mesh)
            {
                // Reading one back from iCloud or OneDrive costs minutes, and this is a
                // convenience: a rider profile whose two 67 MB meshes had been evicted put
                // 84 seconds between picking it and seeing any sheet names. The preview
                // fetches the model when it actually draws it — that wait buys a picture.
                if cloudfiles::is_placeholder(&p) {
                    log::info!("[paint studio] skipping evicted mesh {}", p.display());
                    continue;
                }
                meshes.extend(viewer::read_gear_file(&p));
            }
        }
    }
    if meshes.is_empty() {
        let pkz = library::sibling_pkz(model_dir);
        if pkz.is_file() && !cloudfiles::is_placeholder(&pkz) {
            for (_, d) in pkz::read_selected(&pkz, bikefiles::is_mesh).unwrap_or_default() {
                meshes.push(pkz::read_sidecar_blob(&d).unwrap_or(d));
            }
        }
    }
    let mut names: Vec<String> = Vec::new();
    for mesh in &meshes {
        for t in edf::embedded_textures(mesh) {
            if !names.iter().any(|s| s.eq_ignore_ascii_case(&t.name)) {
                names.push(t.name);
            }
        }
    }
    names
}
/// Draw `bike` as the model-swap variant `variant` would leave it, without applying the
/// swap. The file set is assembled in memory (see `gather_preview_files`) — nothing on
/// disk moves, so this is safe to run with the game open.
#[tauri::command]
async fn preview_model_swap(
    app: tauri::AppHandle,
    bike: String,
    variant: String,
    tyres: Option<String>,
) -> Result<BikeModel, String> {
    tauri::async_runtime::spawn_blocking(move || {
        preview_model_swap_blocking(app, bike, variant, tyres)
    })
    .await
    .map_err(|e| format!("preview_model_swap task failed: {e}"))?
}

fn preview_model_swap_blocking(
    app: tauri::AppHandle,
    bike: String,
    variant: String,
    pick: Option<String>,
) -> Result<BikeModel, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    // Working out which files a variant would park is swap business and stays here; turning
    // the answer into a model is the viewer's, and does not need to know what a swap is.
    let set =
        modelswap::preview_set(&cfg.mods_path, &bike, &variant).map_err(|e| format!("{e:#}"))?;
    let label = format!("{bike} · {variant}");
    let tyres = library::mods_subdir(&cfg.mods_path, "mods/tyres");
    viewer::load_preview_blocking(&set, &label, tyres, pick)
}

/// Download a mod and install it.
///
/// Answers `null` for the ordinary case — one mod, downloaded and placed. A download that
/// turns out to be a *pack* (several mods in one self-describing tree, like the OEM bike
/// pack's 54 bikes and its tyre set) answers with a plan instead, and nothing has been
/// written yet: the caller puts it up for review and commits it through `commit_drop`,
/// exactly as it would a dropped file. Until then the plan owns the staged bytes, and
/// `cancel_drop` is what frees them.
#[tauri::command]
async fn add_to_library(
    app: tauri::AppHandle,
    slug: String,
    url: String,
    host: String,
    subpath: String,
    dest_folder: String,
) -> Result<Option<dropzone::DropPlan>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let _cancel = cancel::begin(&slug);
    let placed = install::add_to_library(&app, &cfg, &slug, &url, &host, &subpath, &dest_folder)
        .await
        .map(install::Placed::review)
        .map_err(|e| format!("{e:#}"))?;
    usage::track("mod.install");
    Ok(placed)
}

/// Stop the install running under `slug`. `false` when nothing is running under it — the
/// frontend drops queued items itself and only reaches for this on the one in flight.
///
/// Only the transfer is interruptible: once the bytes are down and extraction has started
/// there is nothing safe to stop, so the flag is polled by the download loops alone.
#[tauri::command]
fn cancel_install(slug: String) -> bool {
    cancel::request(&slug)
}

#[tauri::command]
async fn import_file(
    app: tauri::AppHandle,
    path: String,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        install::import_file(&app, &cfg, &path, &subpath, &dest_folder).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("import_file task failed: {e}"))?
}

/// Stage and classify dropped paths. Reads only — nothing is installed until `commit_drop`.
#[tauri::command]
async fn plan_drop(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<dropzone::DropPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        dropzone::plan(&cfg.mods_path, &paths).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("plan_drop task failed: {e}"))?
}

/// Re-cost one row after the user picked a different destination.
#[tauri::command]
async fn repreview_drop(
    app: tauri::AppHandle,
    plan_id: String,
    item_id: String,
    subpath: String,
    dest_folder: String,
) -> Result<DropPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let (file_count, bytes, collisions) =
            dropzone::repreview(&cfg.mods_path, &plan_id, &item_id, &subpath, &dest_folder)
                .map_err(|e| format!("{e:#}"))?;
        Ok(DropPreview {
            file_count,
            bytes,
            collisions,
        })
    })
    .await
    .map_err(|e| format!("repreview_drop task failed: {e}"))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DropPreview {
    file_count: usize,
    bytes: u64,
    collisions: Vec<String>,
}

/// Install the reviewed rows.
#[tauri::command]
async fn commit_drop(
    app: tauri::AppHandle,
    plan_id: String,
    items: Vec<dropzone::CommitItem>,
) -> Result<dropzone::CommitOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || commit_plan(&app, &plan_id, &items))
        .await
        .map_err(|e| format!("commit_drop task failed: {e}"))?
}

/// Place a staged plan and do everything that has to follow it.
///
/// Shared by the review sheet's `commit_drop` and the shop's one-click install, so the
/// bookkeeping after a commit can't drift between them — a plan committed but never released
/// leaks its staging directory for the life of the process.
fn commit_plan(
    app: &tauri::AppHandle,
    plan_id: &str,
    items: &[dropzone::CommitItem],
) -> Result<dropzone::CommitOutcome, String> {
    let cfg = config::load(app).map_err(|e| format!("{e:#}"))?;
    let outcome = dropzone::commit(&cfg.mods_path, plan_id, items).map_err(|e| format!("{e:#}"))?;

    // Record which bikes gained a sound set, so the Library can tell them from stock.
    let ok: Vec<String> = outcome.installed.iter().map(|i| i.id.clone()).collect();
    let bikes = dropzone::sound_bikes(plan_id, &ok);
    if !bikes.is_empty() {
        if let Ok(dir) = app.path().app_local_data_dir() {
            let _ = soundmods::record(&dir, &bikes, "drop");
        }
    }

    dropzone::cancel(plan_id);

    // One signal for the whole drop: `notify_frostmod` also emits `frostmod-reload`,
    // which every library scanner listens to — firing it per item would re-run them all
    // N times for a single user action.
    if !outcome.installed.is_empty() {
        usage::track("drop.import");
        install::notify_frostmod(app, "drop");
    }
    Ok(outcome)
}

/// Discard a plan the user dismissed, deleting anything staged for it.
#[tauri::command]
async fn cancel_drop(plan_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || dropzone::cancel(&plan_id))
        .await
        .map_err(|e| format!("cancel_drop task failed: {e}"))
}

#[tauri::command]
async fn move_mod(
    app: tauri::AppHandle,
    from_path: String,
    to_folder: String,
    subpath: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        library::move_mod(&cfg.mods_path, &from_path, &to_folder, &subpath)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("move_mod task failed: {e}"))?
}

#[tauri::command]
async fn uninstall_mod(app: tauri::AppHandle, from_path: String, subpath: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let landed =
            library::uninstall_mod(&cfg.mods_path, &from_path, &subpath).map_err(|e| format!("{e:#}"))?;
        // Remember where the Trash put it, while we still know: that is what makes the
        // ledger row able to offer Restore rather than only a name to go hunting with.
        ledger_note_trashed(&app, &cfg, &from_path, landed);
        Ok(())
    })
    .await
    .map_err(|e| format!("uninstall_mod task failed: {e}"))?
}

#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

/// Put a line from the webview into the app's own log file.
///
/// `tauri_plugin_log` writes what Rust logs; nothing the frontend prints to its console
/// reaches the file a player sends us. Facts only the webview knows — which GPU its WebGL
/// context landed on, say — would otherwise be invisible in exactly the report that needs
/// them.
#[tauri::command]
fn log_client(level: String, message: String) {
    // A log line is not a transport for arbitrary payloads. Trim rather than reject: a
    // truncated fact still reads, and a dropped one is a support thread that goes nowhere.
    let msg: String = message.chars().take(2000).collect();
    match level.as_str() {
        "error" => log::error!("[webview] {msg}"),
        "warn" => log::warn!("[webview] {msg}"),
        _ => log::info!("[webview] {msg}"),
    }
}

/// Where Frost's Mod Manager's own logs are, where the game's are, and what's currently in each.
///
/// Read fresh on every call rather than cached: the whole reason someone opens this is
/// that something just went wrong, and a stale "no logs found" would send them looking in
/// the wrong place.
#[tauri::command]
fn logs_info(app: tauri::AppHandle) -> logs::LogsInfo {
    let cfg = config::load(&app).unwrap_or_default();
    logs::info(&app_log_dir(&app), &frostmod_manage::frostmod_dir(&app), &cfg)
}

/// Open the folder one of the log sets lives in, newest file selected where the OS can do
/// that. `which` is `"app"`, `"frostmod"` or `"game"`.
#[tauri::command]
fn open_logs_folder(app: tauri::AppHandle, which: String) -> Result<(), String> {
    let info = logs_info(app);
    let group = match which.as_str() {
        "game" => &info.game,
        "frostmod" => &info.frostmod,
        _ => &info.app,
    };
    logs::open_location(group).map_err(|e| format!("{e:#}"))
}

/// Zip every set of logs to `dest` — a path the user just picked in a save dialog.
///
/// Blocking work (reads the whole of every log), so it goes off the UI thread: an app log
/// that has been growing for a month would otherwise freeze Settings while it's read.
#[tauri::command]
async fn export_logs(app: tauri::AppHandle, dest: String) -> Result<logs::ExportResult, String> {
    let version = app.package_info().version.to_string();
    let log_dir = app_log_dir(&app);
    let frostmod_dir = frostmod_manage::frostmod_dir(&app);
    let frostmod_version = frostmod_manage::installed_version(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).unwrap_or_default();
        let info = logs::info(&log_dir, &frostmod_dir, &cfg);
        let summary = logs::summary(&version, frostmod_version.as_deref(), &cfg, &info);
        logs::export(std::path::Path::new(&dest), &info, &summary).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("export_logs task failed: {e}"))?
}

/// Zip every set of logs and upload it, handing back the direct link.
///
/// The same archive `export_logs` writes to disk, taken one step further: what a bug
/// report needs is a link, and asking a player to find a save dialog, then a file, then an
/// upload box is where "send me your logs" usually stalls. The upload is the one the
/// Library's file share uses, so the ceiling and the slicing are already understood.
#[tauri::command]
async fn share_logs(app: tauri::AppHandle) -> Result<logs::ShareResult, String> {
    let version = app.package_info().version.to_string();
    let log_dir = app_log_dir(&app);
    let frostmod_dir = frostmod_manage::frostmod_dir(&app);
    let cfg = config::load(&app).unwrap_or_default();
    let info = logs::info(&log_dir, &frostmod_dir, &cfg);
    let summary =
        logs::summary(&version, frostmod_manage::installed_version(&app).as_deref(), &cfg, &info);
    logs::share(&app, &info, &summary).await.map_err(|e| format!("{e:#}"))
}

/// Where `tauri_plugin_log`'s `LogDir` target writes. Empty when the path can't be
/// resolved at all, which [`logs::info`] reports as a folder that isn't there — the same
/// as any other missing folder, rather than a special failure mode of its own.
fn app_log_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path().app_log_dir().unwrap_or_default()
}

#[tauri::command]
fn set_game_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.game_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// macOS: pick the Wine binary that starts the game. Blank hands it back to auto-detection.
#[tauri::command]
fn set_wine_runner(app: tauri::AppHandle, path: String) -> Result<winehost::HostInfo, String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.wine_runner = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    Ok(winehost::describe(&cfg.wine_runner))
}

/// macOS: what the app found to launch the game with, and which bottles it can see.
///
/// Reported rather than assumed — a player whose bottle we can't see needs to know that
/// before they press Play, not after.
#[tauri::command]
fn wine_host_info(app: tauri::AppHandle) -> winehost::HostInfo {
    let cfg = config::load(&app).unwrap_or_default();
    winehost::describe(&cfg.wine_runner)
}

/// The titles this build can drive, with their per-game capabilities. Static data —
/// the switcher and the feature gating both read it.
#[tauri::command]
fn list_games() -> Vec<game::GameInfo> {
    game::all_info()
}

/// Switch which game the app is driving.
///
/// The outgoing game's folders are parked and the incoming one's restored; a game being
/// opened for the first time has none saved, so `finalize` auto-detects them the same
/// way first-run setup does. Returns the resulting config so the UI can go straight to
/// the setup screen when detection came up empty.
///
/// Async for the same reason as `set_mods_path`: detection scans Steam libraries and the
/// watcher restart tears down a thread, neither of which belongs on the UI thread.
#[tauri::command]
async fn set_active_game(
    app: tauri::AppHandle,
    watcher: State<'_, ModWatcher>,
    frostmod_state: State<'_, FrostmodProcess>,
    game: game::Game,
) -> Result<AppConfig, String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    if !cfg.switch_game(game) {
        return Ok(cfg);
    }
    let cfg = config::finalize(cfg);
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    log::info!("switched to {} ({})", cfg.game().display, cfg.mods_path);
    // Point the watcher at the new game's folder — otherwise it keeps reporting changes
    // in the game we just left.
    if cfg.watch_mods_reload {
        modwatch::start(&app, &watcher, &cfg.mods_path);
    }
    // FrostMod reads `--game` and `--mods` once, at launch, and `start` no-ops while one
    // is already running — so without this a switch leaves FrostMod waiting for the game
    // we just left while the status pill still reads "running". `force_stop_exe` because
    // the running one may not be ours to `stop`: a hand-launched frostmod.exe claims the
    // same named event, and that is exactly how this was first reported.
    if frostmod::is_running() {
        frostmod_manage::stop(&frostmod_state);
        frostmod_manage::force_stop_exe();
        if let Err(e) = frostmod_manage::start(&app, &frostmod_state) {
            log::warn!("could not restart FrostMod for {}: {e:#}", cfg.game().display);
        }
    }
    Ok(cfg)
}

/// Point the app at a different mods folder; an empty string re-runs detection.
/// Only the folder changes — unlike a full `create_config`, the rest of the settings
/// (startup, tray, FrostMod, first-run state) are left alone.
///
/// Async so the switch runs off the UI thread: `finalize` can scan Steam libraries and
/// restarting the watcher tears down its background thread, neither of which should be
/// able to lock up the window.
#[tauri::command]
async fn set_mods_path(
    app: tauri::AppHandle,
    watcher: State<'_, ModWatcher>,
    path: String,
) -> Result<String, String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.mods_path = path;
    let cfg = config::finalize(cfg);
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    if cfg.watch_mods_reload {
        modwatch::start(&app, &watcher, &cfg.mods_path);
    }
    // The folder actually adopted, which isn't always the one picked: detection fills a
    // blank, and a pick of the `mods` folder resolves to the game folder above it. Settings
    // says which it took, so a corrected pick is visible rather than silently different.
    Ok(cfg.mods_path)
}

/// Remember that the intro slideshow / guided tour is done. No-ops before the config
/// exists — writing one there would leave the app "configured" with no folder set;
/// the webview flag covers that short window instead.
#[tauri::command]
fn set_intro_seen(app: tauri::AppHandle, welcome: bool, tour: bool) -> Result<(), String> {
    if !config::exists(&app) {
        return Ok(());
    }
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.welcome_seen |= welcome;
    cfg.tour_done |= tour;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Remember that the release showcase for `version` has been seen, so it doesn't come
/// back on the next launch. No-ops before the config exists, like `set_intro_seen`.
#[tauri::command]
fn set_seen_version(app: tauri::AppHandle, version: String) -> Result<(), String> {
    if !config::exists(&app) {
        return Ok(());
    }
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.seen_version = version;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Override the PiBoSo `profiles` folder for the split-folder edge case. An empty
/// string clears the override, falling back to `<mods_path>/profiles`.
#[tauri::command]
fn set_profiles_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.profiles_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Both watchers are pinned to a folder that just moved; re-point them, and publish in
    // case the new folder's look differs from what the old one last sent.
    if watches_looks(&cfg) {
        let profiles = app.state::<ProfileWatcher>();
        profilewatch::start(&app, &profiles, &cfg.profiles_dir());
        watch_worn_paints(&app);
        publish_paints_soon(&app, &cfg, None);
    }
    Ok(())
}

/// Scan Steam for a game's install folder. `None` if not found.
///
/// `game` names which title to look for. Setup passes it explicitly because on a first
/// run the user has picked a game but nothing is saved yet — and persisting the pick just
/// to make detection work would write a config before setup finishes, which would then
/// look like an upgrade rather than a fresh install. Omitted, it means the active game.
#[tauri::command]
fn detect_game_path(app: tauri::AppHandle, game: Option<game::Game>) -> Option<String> {
    let profile = match game {
        Some(g) => g.profile(),
        None => config::load(&app).unwrap_or_default().game(),
    };
    config::detect_game_path(profile)
}

/// How many profiles (subdirs with a `profile.ini`) live under `path` — lets the
/// UI warn when a picked profiles folder has none.
#[tauri::command]
fn count_profiles_in(path: String) -> usize {
    presets::list_profiles(std::path::Path::new(&path)).len()
}

/// The folder the app actually reads content out of, and whether it's there.
///
/// `modsPath` alone doesn't answer this any more: it may be the game's user folder, whose
/// `mods` child is the real root, or a relocated tree that *is* the root. Which one it
/// landed on is the first thing to check when the library comes up empty, so Settings
/// shows it rather than leaving the player to infer it from a path that reads fine.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModsRootInfo {
    path: String,
    exists: bool,
    /// The root is `modsPath` itself rather than its `mods` child — i.e. a relocated tree.
    relocated: bool,
}

#[tauri::command]
fn get_mods_root(app: tauri::AppHandle) -> ModsRootInfo {
    let cfg = config::load(&app).unwrap_or_default();
    let root = library::mods_root(&cfg.mods_path);
    ModsRootInfo {
        exists: root.is_dir(),
        relocated: !cfg.mods_path.trim().is_empty()
            && root == std::path::Path::new(cfg.mods_path.trim()),
        path: root.to_string_lossy().into_owned(),
    }
}

/// Whether this build can decode real bike geometry (the optional local module is
/// compiled in). Public builds without it return `false`, so the UI hides the bike
/// 3D preview instead of showing a broken/empty one.
#[tauri::command]
fn bike_preview_available() -> bool {
    cfg!(sidecar)
}

/// Whether this build can produce protected copies of a creator's files. Same shape as
/// [`bike_preview_available`]: the optional local module carries the format, so a build
/// without it hides the tool rather than offering one that can't do anything.
#[tauri::command]
fn content_lock_available() -> bool {
    cfg!(sidecar)
}

/// Whether this build can lock content with mxbsecure — the packer is the gitignored
/// `mxbsecure` sidecar, so a public build reports false and the Secure tab stays hidden.
#[tauri::command]
fn content_secure_available() -> bool {
    cfg!(mxbsecure)
}

/// Turn the experimental mxbsecure tab on or off.
#[tauri::command]
fn set_mxbsecure_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.mxbsecure_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// The result of locking a file: where the blob landed, and the content key to keep. The key
/// is returned once, for the operator to store server-side; it is never written into the blob.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureLockOutcome {
    blob_path: String,
    asset_id: String,
    key_id: String,
    key: String,
    plain_bytes: u64,
    blob_bytes: u64,
}

/// Lock a file into a `.mxbsecure` blob under a fresh content key.
///
/// `src` is read and never modified. The blob is written beside it (or into `out_dir` when
/// given) as `<name>.mxbsecure`. Reads the whole file into memory — a creator's asset, not a
/// stream — which is fine for the sizes involved and keeps the packer simple.
#[tauri::command]
async fn mxbsecure_lock(
    src: String,
    out_dir: Option<String>,
) -> Result<SecureLockOutcome, String> {
    #[cfg(mxbsecure)]
    {
        use std::path::PathBuf;
        let src_path = PathBuf::from(&src);
        let name = src_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| "not a file".to_string())?;
        let out = match out_dir {
            Some(d) => PathBuf::from(d).join(format!("{name}.mxbsecure")),
            None => src_path.with_file_name(format!("{name}.mxbsecure")),
        };
        // A stable-ish asset id from the file name plus a random suffix, so two locks of two
        // files don't collide. A real registration mints this; here it just labels the blob.
        let mut rnd = [0u8; 6];
        getrandom::getrandom(&mut rnd).map_err(|e| e.to_string())?;
        let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
        let asset_id = format!("{}-{suffix}", sanitize_asset_id(&name));

        let plaintext = tokio::fs::read(&src_path).await.map_err(|e| format!("read {src}: {e}"))?;
        let locked = mxbsecure::lock(&plaintext, &asset_id, "k1");
        tokio::fs::write(&out, &locked.blob).await.map_err(|e| format!("write blob: {e}"))?;

        Ok(SecureLockOutcome {
            blob_path: out.to_string_lossy().to_string(),
            asset_id: locked.asset_id,
            key_id: locked.key_id,
            key: mxbsecure::hex_key(&locked.content_key),
            plain_bytes: plaintext.len() as u64,
            blob_bytes: locked.blob.len() as u64,
        })
    }
    #[cfg(not(mxbsecure))]
    {
        let _ = (src, out_dir);
        Err("this build can't lock content with mxbsecure".into())
    }
}

/// Verify a locked blob opens back to a plaintext identical to `original`.
///
/// This is the "can it unlock it" check the Secure tab runs after a lock: decrypt the blob
/// with the key and compare byte-for-byte to the source. Proves the format round-trips on
/// this machine, independently of the in-game DLL.
#[tauri::command]
async fn mxbsecure_verify(
    blob_path: String,
    key: String,
    original: String,
) -> Result<bool, String> {
    #[cfg(mxbsecure)]
    {
        let content_key = mxbsecure::key_from_hex(&key).ok_or("the key isn't 32 bytes of hex")?;
        let blob = tokio::fs::read(&blob_path).await.map_err(|e| format!("read blob: {e}"))?;
        let opened = mxbsecure::open(&blob, &content_key).map_err(|e| format!("{e}"))?;
        let original = tokio::fs::read(&original).await.map_err(|e| format!("read original: {e}"))?;
        Ok(opened == original)
    }
    #[cfg(not(mxbsecure))]
    {
        let _ = (blob_path, key, original);
        Err("this build can't open mxbsecure content".into())
    }
}

/// The Steam account currently signed in on this machine, for binding a key to.
#[tauri::command]
fn secure_steam_id() -> Option<String> {
    steamid::current_steam_id64()
}

/// What generating a protected copy produced. The two files a buyer needs, side by side.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureGenerateOutcome {
    /// The name the game will list and open (the original file's name, e.g. `FarmSX.pkz`).
    game_name: String,
    /// The encrypted blob: `<original>.mxbsecure`.
    blob_path: String,
    /// The key sealed to the buyer's Steam ID: `<original>.mxbsecure.mxbkey`.
    mxbkey_path: String,
    /// The Steam ID it was sealed to.
    steam_id: String,
    plain_bytes: u64,
}

/// Generate a protected copy of a track for a **specific Steam ID**, leaving the original
/// untouched.
///
/// This is the creator's action. Given a track and the buyer's 17-digit Steam ID, it writes two
/// files beside the original:
///
/// - `<track>.mxbsecure` — the encrypted blob.
/// - `<track>.mxbsecure.mxbkey` — the content key sealed to that Steam ID.
///
/// Both are needed, next to each other, to load; the key opens only on the machine signed into
/// that Steam account. The original `.pkz` is never modified — the creator keeps their master,
/// and hands the buyer only the two generated files.
#[tauri::command]
async fn mxbsecure_generate(
    track_path: String,
    steam_id: String,
) -> Result<SecureGenerateOutcome, String> {
    #[cfg(mxbsecure)]
    {
        use std::path::Path;

        let steam_id = steam_id.trim().to_string();
        if steam_id.len() != 17 || !steam_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("that isn't a Steam ID — it should be 17 digits (a SteamID64)".into());
        }

        let plaintext = tokio::fs::read(&track_path)
            .await
            .map_err(|e| format!("read {track_path}: {e}"))?;
        // Refuse a file that is already one of ours, so a double-encrypt can't seal ciphertext.
        if plaintext.starts_with(b"MXBSEC") {
            return Err("that file is already protected".into());
        }
        let name = Path::new(&track_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or("not a file")?;

        let mut rnd = [0u8; 6];
        getrandom::getrandom(&mut rnd).map_err(|e| e.to_string())?;
        let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
        let asset_id = format!("{}-{suffix}", sanitize_asset_id(&name));

        let locked = mxbsecure::lock(&plaintext, &asset_id, "k1");
        let sealed = mxbsecure::seal_key_to_identity(&locked.content_key, &steam_id, "");

        // `<track>.mxbsecure` and `<track>.mxbsecure.mxbkey`, beside the original.
        let blob_path = format!("{track_path}.mxbsecure");
        let mxbkey_path = format!("{blob_path}.mxbkey");
        // Blob is written to a temp and renamed, so a crash mid-write leaves no half file.
        let tmp = format!("{blob_path}.writing");
        tokio::fs::write(&tmp, &locked.blob).await.map_err(|e| format!("write blob: {e}"))?;
        tokio::fs::rename(&tmp, &blob_path).await.map_err(|e| format!("finish blob: {e}"))?;
        tokio::fs::write(&mxbkey_path, &sealed).await.map_err(|e| format!("write .mxbkey: {e}"))?;

        Ok(SecureGenerateOutcome {
            game_name: name,
            blob_path,
            mxbkey_path,
            steam_id,
            plain_bytes: plaintext.len() as u64,
        })
    }
    #[cfg(not(mxbsecure))]
    {
        let _ = (track_path, steam_id);
        Err("this build can't generate protected content".into())
    }
}

/// What provisioning a key produced.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureProvisionOutcome {
    mxbkey_path: String,
    steam_id: String,
}

/// Provision a content key for offline play: seal it to the live Steam ID and store it as a
/// `.mxbkey` beside the blob. Called once, after the server has released the key (here the
/// Lock tab supplies it). From then on the key opens offline for this account only.
#[tauri::command]
async fn mxbsecure_provision(
    app: tauri::AppHandle,
    blob_path: String,
    key: String,
) -> Result<SecureProvisionOutcome, String> {
    #[cfg(mxbsecure)]
    {
        let steam_id = steamid::current_steam_id64()
            .ok_or("couldn't read your Steam ID — is Steam installed and signed in?")?;
        let content_key = mxbsecure::key_from_hex(&key).ok_or("the key isn't 32 bytes of hex")?;
        let sealed = mxbsecure::seal_key_to_identity(&content_key, &steam_id, "");
        let out = std::path::PathBuf::from(format!("{blob_path}.mxbkey"));
        tokio::fs::write(&out, &sealed).await.map_err(|e| format!("write .mxbkey: {e}"))?;

        // Remember the mapping so the app can arm this asset — write the manifest and inject
        // the DLL — the next time the game starts. The game name is the blob's own name with
        // the `.mxbsecure` suffix removed: the file the engine will ask for.
        let game_name = std::path::Path::new(&blob_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .map(|n| n.strip_suffix(".mxbsecure").unwrap_or(&n).to_string())
            .unwrap_or_default();
        if let Err(e) = secure_launch::record_asset(
            &app,
            secure_launch::SecureAsset {
                game_name,
                blob_path: blob_path.clone(),
                mxbkey_path: out.to_string_lossy().to_string(),
            },
        ) {
            log::warn!("[secure] couldn't record the provisioned asset: {e}");
        }

        Ok(SecureProvisionOutcome {
            mxbkey_path: out.to_string_lossy().to_string(),
            steam_id,
        })
    }
    #[cfg(not(mxbsecure))]
    {
        let _ = (app, blob_path, key);
        Err("this build can't provision mxbsecure content".into())
    }
}

/// Open a blob offline using its provisioned `.mxbkey`: read the live Steam ID, unseal the
/// key, decrypt, and confirm it matches `original`. This is the offline "does it still
/// unlock for me, with no server" proof — a different account gets nothing.
#[tauri::command]
async fn mxbsecure_open_offline(
    blob_path: String,
    original: String,
) -> Result<bool, String> {
    #[cfg(mxbsecure)]
    {
        let steam_id = steamid::current_steam_id64()
            .ok_or("couldn't read your Steam ID — is Steam installed and signed in?")?;
        let mxbkey_path = format!("{blob_path}.mxbkey");
        let sealed = tokio::fs::read(&mxbkey_path).await.map_err(|e| format!("read .mxbkey: {e}"))?;
        let key = mxbsecure::unseal_key(&sealed, &steam_id, "")
            .ok_or("this key isn't sealed to your Steam account")?;
        let blob = tokio::fs::read(&blob_path).await.map_err(|e| format!("read blob: {e}"))?;
        let opened = mxbsecure::open(&blob, &key).map_err(|e| format!("{e}"))?;
        let original = tokio::fs::read(&original).await.map_err(|e| format!("read original: {e}"))?;
        Ok(opened == original)
    }
    #[cfg(not(mxbsecure))]
    {
        let _ = (blob_path, original);
        Err("this build can't open mxbsecure content".into())
    }
}

/// Keep an asset id to the characters a header and a URL are both happy with.
#[cfg(mxbsecure)]
fn sanitize_asset_id(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() { "asset".to_string() } else { trimmed.to_string() }
}

/// What a run over `paths` would touch — every file under the selection, with the ones it
/// would leave alone flagged and why. Folders are walked; a file is taken as itself.
#[tauri::command]
async fn content_lock_plan(paths: Vec<String>) -> Result<serde_json::Value, String> {
    #[cfg(sidecar)]
    {
        let roots: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        let items = tauri::async_runtime::spawn_blocking(move || sidecar_lock::plan(&roots))
            .await
            .map_err(|e| format!("content_lock_plan task failed: {e}"))?
            .map_err(|e| format!("{e:#}"))?;
        return serde_json::to_value(items).map_err(|e| e.to_string());
    }
    #[cfg(not(sidecar))]
    {
        let _ = paths;
        Err("this build can't lock content".into())
    }
}

/// Write a copy of every file in `paths`, locked to each GUID in `guids`, under
/// `out_dir/<GUID>/`. Reports progress on `content-lock://progress`.
///
/// The sources are only ever read. A creator's plaintext is the one thing they can't get
/// back, so the tool that hands out locked copies is not also the tool that could eat the
/// original.
#[tauri::command]
async fn content_lock_run(
    app: tauri::AppHandle,
    paths: Vec<String>,
    guids: Vec<String>,
    out_dir: String,
) -> Result<serde_json::Value, String> {
    #[cfg(sidecar)]
    {
        let roots: Vec<std::path::PathBuf> =
            paths.into_iter().map(std::path::PathBuf::from).collect();
        let out = std::path::PathBuf::from(out_dir);
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            sidecar_lock::run(&app, &roots, &guids, &out)
        })
        .await
        .map_err(|e| format!("content_lock_run task failed: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
        usage::track("content.protect");
        return serde_json::to_value(outcome).map_err(|e| e.to_string());
    }
    #[cfg(not(sidecar))]
    {
        let _ = (app, paths, guids, out_dir);
        Err("this build can't lock content".into())
    }
}

/// This player's own MX Bikes GUID, read out of the running game.
///
/// `None` is the ordinary answer — the game isn't running, or hasn't reached Steam sign-in
/// yet. See [`gameproc::local_guid`] for why it is never a guess.
#[tauri::command]
fn local_guid() -> Option<String> {
    gameproc::local_guid()
}

/// The OS we're running on — `"windows"`, `"macos"`, `"linux"`.
///
/// The frontend used to infer this from `navigator.userAgent`, which can tell a Mac from
/// everything else and nothing more. Features that only exist on Windows (FrostMod, the
/// live in-game refresh) need to know the difference between Windows and Linux, so it
/// comes from the backend rather than adding `plugin-os` and a capability for one string.
#[tauri::command]
fn app_platform() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
fn set_run_in_background(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.run_in_background = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Count something the UI did.
///
/// The webview is where pages and most features are, so it needs a way in — but not a way
/// to invent the payload: it sends a name and nothing else, and a name that isn't one is
/// dropped by [`usage::track`] rather than stored.
#[tauri::command]
fn track_event(name: String) {
    usage::track(&name);
}

/// The one switch that stops anonymous usage counts.
///
/// Saves first and only then tells [`usage`], so a save that failed can never leave the app
/// counting things the player has said no to.
#[tauri::command]
fn set_analytics_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.analytics_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    usage::set_enabled(&app, enabled, &cfg);
    Ok(())
}

#[tauri::command]
fn set_launch_at_startup(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.launch_at_startup = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())
}

/// What to do with the login item at startup.
#[derive(Debug, PartialEq, Eq)]
enum Autostart {
    Leave,
    Enable,
    /// It is there, but it was written for a binary this build no longer has.
    Rebind,
    Disable,
    /// Windows' own Startup list has the app switched off. Take that as the answer and
    /// turn the setting off to match, instead of writing over it.
    Adopt,
}

/// Reconcile the login item with the setting.
///
/// `stale` is the case that isn't obvious: the entry holds the executable's absolute path, so
/// renaming the binary leaves every existing one pointing at a file that is gone — while
/// `is_enabled` still answers yes, because all it looks for is the entry. Left alone, the app
/// simply stops starting at login and nothing ever says why.
///
/// `vetoed` is the other one, and it outranks everything: see [`startup_vetoed`].
fn autostart_action(wanted: bool, enabled: bool, vetoed: bool, stale: bool) -> Autostart {
    match (wanted, enabled) {
        (true, _) if vetoed => Autostart::Adopt,
        (true, false) => Autostart::Enable,
        (true, true) if stale => Autostart::Rebind,
        (false, true) => Autostart::Disable,
        _ => Autostart::Leave,
    }
}

/// Has the player switched the app off in Windows' own list of startup apps?
///
/// Task Manager → Startup apps (and Settings → Apps → Startup) does not remove the `Run`
/// entry. It leaves it where it is and stamps a flag beside it under `StartupApproved\Run`:
/// twelve bytes whose tail is all zeros for on, and carries the time it was switched off for
/// off. `auto-launch` folds that flag into `is_enabled()`, so a veto there reads back exactly
/// like "there is no entry" — and the reconcile above answered that by calling `enable()`,
/// which rewrites both the entry *and* the flag. Every launch quietly undid the choice, and
/// an update, which restarts the app, is where people noticed.
///
/// The value name is the app's own name, which is what `tauri-plugin-autostart` passes
/// `auto-launch` as the entry name.
#[cfg(windows)]
fn startup_vetoed(app_name: &str) -> bool {
    use std::os::raw::c_void;

    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001
    const RRF_RT_REG_BINARY: u32 = 0x0000_0008;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hkey: isize,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            typ: *mut u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let subkey =
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
    let value = wide(app_name);
    let mut buf = [0u8; 32];
    let mut len = buf.len() as u32;
    // SAFETY: a read-only registry query into a fixed stack buffer, length passed in bytes.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        )
    };
    // No flag at all is the normal case — nobody has been through that screen.
    if rc != 0 {
        return false;
    }
    startup_flag_is_veto(&buf[..(len as usize).min(buf.len())])
}

#[cfg(not(windows))]
fn startup_vetoed(_app_name: &str) -> bool {
    false
}

/// The product name this app shipped under up to v0.13.x.
///
/// Kept as a literal rather than read from anywhere: it names things already written to a
/// user's machine, so it must not follow `productName` when that changes again.
const LEGACY_APP_NAME: &str = "MXB App";

/// Delete the login item a previous product name left behind.
///
/// `tauri-plugin-autostart` names the `Run` value after `package_info().name`, which is
/// `productName`. A rename therefore does not move that value — it writes a second one and
/// leaves the first pointing into an install folder the new installer no longer owns, so the
/// player gets two startup entries, one of them dead. [`Autostart::Rebind`] cannot help: it
/// only fires when the item is already enabled under the *current* name, which right after a
/// rename it never is.
///
/// The `StartupApproved\Run` flag goes with it, so a stale row does not sit in Task
/// Manager's Startup list naming a program that is no longer installed.
#[cfg(windows)]
fn delete_legacy_login_item(app_name: &str) {
    const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegDeleteKeyValueW(hkey: isize, subkey: *const u16, value: *const u16) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let value = wide(app_name);
    for subkey in [
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run",
    ] {
        let sk = wide(subkey);
        // SAFETY: deletes one named value from a fixed HKCU subkey. Both strings are
        // NUL-terminated and outlive the call; a value that isn't there returns non-zero
        // and is the normal case on every launch after the first.
        let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sk.as_ptr(), value.as_ptr()) };
        if rc == 0 {
            log::info!("removed the stale `{app_name}` login item from HKCU\\{subkey}");
        }
    }
}

#[cfg(not(windows))]
fn delete_legacy_login_item(_app_name: &str) {}

/// Read one `StartupApproved\Run` flag: enabled carries a zero tail, disabled carries the
/// FILETIME it was switched off. Anything too short to hold one is not a veto.
#[cfg_attr(not(windows), allow(dead_code))] // read only on Windows; the tests run anywhere
fn startup_flag_is_veto(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && !bytes.iter().rev().take(8).all(|b| *b == 0)
}

/// The frontend has drawn its first frame — see [`firstpaint`].
#[tauri::command]
fn window_painted(app: tauri::AppHandle) {
    firstpaint::mark(&app);
}

pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        // Every path in — the tray, a second launch, the watchdog — can land here before
        // the webview has painted, and an undecorated window with no frontend in it has
        // nothing to close it by.
        firstpaint::decorate_unpainted(&w);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn frostmod_reload() -> ReloadOutcome {
    frostmod::signal_reload()
}

#[tauri::command]
fn frostmod_running() -> bool {
    frostmod::is_running()
}

/// Whether FrostMod actually got into the running game — and what to do when it didn't.
///
/// `frostmod_running` only says the launcher is up, which is what made an elevated game so
/// confusing to be on the wrong side of: the app said FrostMod was running, and the game
/// had no pill in it. See [`frostmod::attachment`].
#[tauri::command]
fn frostmod_attachment() -> frostmod::Attachment {
    frostmod::attachment()
}

/// Start MX Bikes from the Play button in the sidebar.
#[tauri::command]
fn launch_game(app: tauri::AppHandle) -> Result<gameproc::LaunchOutcome, String> {
    // `load_or_detect`, not `load`: a missing config file shouldn't turn Play into an
    // error when the install is sitting exactly where the detector looks.
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let outcome = gameproc::launch(&cfg).map_err(|e| format!("{e:#}"))?;
    if matches!(outcome, gameproc::LaunchOutcome::Launched) {
        usage::track("game.launch");
        // Both directions, because Play is the last moment before either one matters: the
        // grid needs everyone else's paints on disk, and everyone else needs ours. No
        // address to aim at — they'll pick from the in-game browser — so the pre-pull
        // covers the whole registry until the game says which server it really landed on.
        sync_on_game_started(&app, &cfg);
    }
    Ok(outcome)
}

/// The version to *show*, which is the release tag when this build came from one.
///
/// `tauri.conf.json` carries a plain `x.y.z` that the release workflow never rewrites, so a
/// build cut from `v0.8.0-beta.1` packages itself as `0.8.0` — indistinguishable from the
/// full release that follows. The tag is baked in by `build.rs` (`MXB_RELEASE_TAG`) and is
/// the only place the pre-release suffix survives.
///
/// Deliberately scoped to what the UI displays: the updater and the release showcase compare
/// against `package_info().version` and must keep doing so.
///
/// A tag is only believed when it looks like a version, so a stray `MXB_RELEASE_TAG=main`
/// can't put a branch name in the About box.
fn release_version(packaged: String) -> String {
    pick_release_version(option_env!("MXB_RELEASE_TAG"), packaged)
}

/// The rule behind [`release_version`], split out because `option_env!` resolves at compile
/// time — testing it in place would mean a rebuild per case.
fn pick_release_version(tag: Option<&str>, packaged: String) -> String {
    tag.map(str::trim)
        .map(|tag| tag.strip_prefix('v').unwrap_or(tag))
        .filter(|tag| tag.starts_with(|c: char| c.is_ascii_digit()))
        .map(str::to_string)
        .unwrap_or(packaged)
}

/// Paint sync's state, and whether this build is a pre-release.
///
/// Named for a toggle that no longer exists: it used to answer "should the unfinished
/// multiplayer surface be shown". Server creation is out of the app for now, so what is
/// left is what paint sync has published and pulled, plus the version the About page
/// badges. Kept under the old name because six call sites read it and none of them cares
/// what it is called.
#[tauri::command]
fn experimental_state(app: tauri::AppHandle) -> serde_json::Value {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let version = release_version(app.package_info().version.to_string());
    serde_json::json!({
        "version": version,
        // A semver pre-release suffix (`0.8.0-beta.1`) is what the release workflow uses to
        // mark a build as a pre-release, so it is also what makes this build a beta.
        "prerelease": version.contains('-'),
        // An account, however it was come by. Since paint sync claims one on its own the
        // first time it runs, this stopped meaning "typed in an invite code" and now means
        // "the control plane knows who this is" — which is what every caller wanted.
        "enrolled": !cfg.cp_token.trim().is_empty(),
        "paintSyncEnabled": cfg.paint_sync_enabled,
        "riderName": cfg.cp_rider_name,
        "guid": cfg.cp_guid,
        // What paint sync last managed, so the panel can say so on a cold start rather than
        // showing nothing until something happens to run.
        "sync": cfg.sync,
        // Whether there is a profile to publish at all. A rider name that matches no profile
        // on disk publishes nothing, silently, and that is worth saying out loud.
        "profile": sync_profile(&cfg),
        // Other people's paints currently sitting in the mods folder. The sync writes them
        // and, until it was given something to press, nothing ever took them out again.
        "syncedPaints": paintsync::installed_count(&cfg),
    })
}

/// Trade an invite code for a control-plane account, and remember the token.
#[tauri::command]
async fn enroll_account(
    app: tauri::AppHandle,
    code: String,
    rider_name: String,
) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Resp {
        token: String,
        #[serde(rename = "riderName")]
        rider_name: String,
    }
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/enroll", paintsync::control_plane()))
        .json(&serde_json::json!({ "code": code, "riderName": rider_name }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if !resp.status().is_success() {
        let detail = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&detail)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(detail);
        return Err(msg);
    }
    let body: Resp = resp.json().await.map_err(|e| format!("{e}"))?;

    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.cp_token = body.token;
    cfg.cp_rider_name = body.rider_name.clone();
    // A new account has published nothing, whatever this machine last sent under an older
    // one. Clearing the digest is what stops the first publish being skipped as unchanged.
    cfg.sync = config::SyncState::default();
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;

    // Enrolling and then pressing Play was the commonest way to end up in a roster wearing
    // nothing: publishing only ever ran off a preset apply, so a player who never opened the
    // Locker published nothing at all. This is that gap closed at its source.
    publish_paints_soon(&app, &cfg, None);
    Ok(body.rider_name)
}

/// Claim this player's MX Bikes GUID.
///
/// The GUID is the stable identity: a rider name is free text that changes between sessions
/// and two people can pick the same one, while the dedicated server logs a GUID with every
/// connection. Claiming is first-come on the server side.
#[tauri::command]
async fn set_guid(app: tauri::AppHandle, guid: String) -> Result<(), String> {
    claim_guid(&app, &guid).await
}

/// Register `guid` against this account and remember it locally. See [`identity`].
async fn claim_guid(app: &tauri::AppHandle, guid: &str) -> Result<(), String> {
    identity::claim_guid(app, guid).await
}

/// Publish this rider's paints so everyone else on the server can see them.
///
/// The app does this on its own whenever the look changes; this is the button for when a
/// player wants to know it happened, or to force it after a failure. `force` drops the
/// digest so an identical look is sent anyway — otherwise pressing it after a successful
/// publish would look like it did nothing.
#[tauri::command]
async fn publish_paints(
    app: tauri::AppHandle,
    profile: Option<String>,
    force: bool,
) -> Result<paintsync::PublishOutcome, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let profile = match profile.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => sync_profile(&cfg).ok_or("No MX Bikes profile to publish.")?,
    };
    let known = (!force).then(|| cfg.sync.published_digest.clone());
    let outcome = paintsync::publish_all(&cfg, &cfg.cp_token, &profile, known.as_deref())
        .await
        .map_err(|e| format!("{e:#}"))?;
    remember_publish(&app, &outcome);
    emit_sync(&app, SyncEvent::published(&outcome));
    Ok(outcome)
}

/// Which profile paint sync speaks for.
///
/// The name enrolled with, when it matches a profile on disk — that is the identity other
/// players' apps key on. Otherwise the only profile there is, which is the overwhelmingly
/// common shape and saves asking a question with one possible answer.
fn sync_profile(cfg: &AppConfig) -> Option<String> {
    let profiles = presets::list_profiles(&cfg.profiles_dir());
    let enrolled = cfg.cp_rider_name.trim();
    if !enrolled.is_empty() {
        if let Some(hit) = profiles.iter().find(|p| p.eq_ignore_ascii_case(enrolled)) {
            return Some(hit.clone());
        }
    }
    (profiles.len() == 1).then(|| profiles[0].clone()).or_else(|| profiles.first().cloned())
}

/// Record what a publish achieved, so a cold start can still answer "is my look out there?".
fn remember_publish(app: &tauri::AppHandle, outcome: &paintsync::PublishOutcome) {
    // Re-read immediately before writing: the publish took a round trip, and `config::save`
    // rewrites the whole file.
    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    cfg.sync.published_digest = outcome.digest.clone();
    cfg.sync.published_at = now_ms();
    cfg.sync.published_bikes = outcome.bikes;
    cfg.sync.published_paints = outcome.published;
    if let Err(e) = config::save(app, &cfg) {
        log::warn!("[sync] couldn't record the publish: {e:#}");
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The event the frontend listens on to open enrollment with the code already filled in.
const DEEP_LINK_ENROLL_EVENT: &str = "deep-link-enroll";

/// The invite code out of an `mxb://enroll?code=…` link.
///
/// Parsed by hand rather than with a URL crate because only one shape is accepted and the
/// value goes straight into a form: anything that isn't the enroll route, or carries a code
/// that isn't a plain token, is dropped rather than guessed at. A deep link is reachable by
/// any page the player visits, so this is untrusted input and treated as such.
fn enroll_code_from_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix("mxb://")?;
    // `mxb://enroll?code=X` — the host is the route. A trailing slash is what some launchers
    // add, so it's tolerated rather than made to fail.
    let (route, query) = rest.split_once('?')?;
    let route = route.trim_end_matches('/');
    // Both spellings, because the link is written by a human handing out an invite and the
    // two are a genuine trap. Accepting one costs a comparison; rejecting it costs someone
    // an invite that silently does nothing.
    if !route.eq_ignore_ascii_case("enroll") && !route.eq_ignore_ascii_case("enroll") {
        return None;
    }
    let code = query.split('&').find_map(|pair| pair.strip_prefix("code="))?.trim();
    // Invite codes are opaque tokens. Anything with punctuation or spacing in it is either
    // percent-encoding we don't want to guess at or an attempt to smuggle something else.
    if code.is_empty()
        || code.len() > 128
        || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(code.to_string())
}

/// Bring the window up and hand the frontend the code from an `mxb://enroll` link.
///
/// The link only ever *prefills* the field — enrolling still needs the player to press the
/// button. A URL a website can open must not be able to spend an invite on its own.
fn handle_deep_link(app: &tauri::AppHandle, urls: &[String]) {
    let Some(code) = urls.iter().find_map(|u| enroll_code_from_link(u)) else {
        log::warn!("[deep-link] ignored {urls:?} — not an enroll link");
        return;
    };
    show_main(app);
    if let Err(e) = app.emit(DEEP_LINK_ENROLL_EVENT, code) {
        log::warn!("[deep-link] couldn't hand the code to the UI: {e}");
    }
}

/// Coalesces a burst of look changes into a single publish.
///
/// Bumped on every request; a waiting task whose generation has moved on drops out rather
/// than uploading a look that has already been replaced.
static PUBLISH_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How long to wait for the player to stop changing their look before publishing it.
/// Cycling through presets in the Locker is one publish, not one per click.
const PUBLISH_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(1500);

/// The event the frontend listens on to follow a background publish or pull.
const SYNC_EVENT: &str = "paint-sync";

/// What paint sync is doing, for the UI to show.
///
/// Publishing and pulling both happen in spawned tasks off actions the player didn't ask
/// for directly — an apply, a launch, a file changing under us. Without this they are
/// entirely invisible: the only report was a log line, so "is this working?" had no answer
/// anywhere on screen.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncEvent {
    /// `publishing` | `published` | `pulling` | `pulled` | `failed`
    phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    publish: Option<paintsync::PublishOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pull: Option<paintsync::PullOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl SyncEvent {
    fn phase(phase: &'static str) -> Self {
        SyncEvent { phase, publish: None, pull: None, error: None }
    }
    fn published(outcome: &paintsync::PublishOutcome) -> Self {
        SyncEvent { publish: Some(outcome.clone()), ..SyncEvent::phase("published") }
    }
    fn pulled(outcome: &paintsync::PullOutcome) -> Self {
        SyncEvent { pull: Some(outcome.clone()), ..SyncEvent::phase("pulled") }
    }
    fn failed(error: impl std::fmt::Display) -> Self {
        SyncEvent { error: Some(error.to_string()), ..SyncEvent::phase("failed") }
    }
}

/// Publish because something outside the app changed the look on disk.
///
/// The watcher runs on the notify thread with no config in hand, so this reads it and hands
/// off to the ordinary debounced path. Separate from `publish_paints_soon` only because that
/// one takes the config its caller already has.
pub fn publish_look_now(app: &tauri::AppHandle) {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    publish_paints_soon(app, &cfg, None);
}

/// Is a look change worth noticing? Paint sync publishes them, and the look watcher rebuilds
/// on them — either one is reason enough to watch `profile.ini`.
fn watches_looks(cfg: &AppConfig) -> bool {
    cfg.paint_sync_enabled || can_refresh_live_look()
}

/// The rider is wearing something different: re-point the look watcher at the new files, and
/// publish. One entry point rather than two calls at every site, because forgetting the
/// re-point leaves the watcher holding the paints of a look nobody is in any more.
pub fn look_changed(app: &tauri::AppHandle) {
    watch_worn_paints(app);
    publish_look_now(app);
}

fn emit_sync(app: &tauri::AppHandle, event: SyncEvent) {
    if let Err(e) = app.emit(SYNC_EVENT, event) {
        log::warn!("[sync] couldn't tell the UI: {e}");
    }
}

/// Publish the current look in the background, once the player has stopped changing it.
///
/// Called from every path that could have changed what this rider is wearing — an apply,
/// enrolling, starting up, launching the game, and the profile watcher. It does not try to
/// work out whether the look really changed: `publish_all` hashes it and sends nothing when
/// the digest matches, so calling this too often costs one local hash and no request.
///
/// Publishes the whole profile rather than one bike. Which bike a rider takes out is decided
/// in the game, so publishing only the one the app last touched is why a rider could look
/// right on one bike and default on the next.
///
/// Best-effort on purpose: publishing is a side errand of an action that has already
/// succeeded on disk, so a failure here logs and is dropped rather than surfacing as an
/// error on the apply the player actually asked for.
fn publish_paints_soon(app: &tauri::AppHandle, cfg: &AppConfig, profile: Option<&str>) {
    if !cfg.paint_sync_enabled {
        return;
    }
    let generation = PUBLISH_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let app = app.clone();
    let profile = profile.map(str::to_string);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PUBLISH_DEBOUNCE).await;
        // A later change superseded this one; that request owns the publish.
        if PUBLISH_GEN.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return;
        }
        // Re-read rather than reusing the captured config: the debounce is long enough for
        // the player to have enrolled, or re-enrolled, since the change that queued this.
        let cfg = config::load_or_detect(&app).unwrap_or_default();
        if !cfg.paint_sync_enabled {
            return;
        }
        let Some(profile) = profile.or_else(|| sync_profile(&cfg)) else {
            return;
        };
        // Claimed here rather than required: a rider with no invite still has to be able to
        // publish, or the grid beside them stays default no matter what they wear.
        let token = match voice::signal::account(&app, &cfg).await {
            Ok(token) => token,
            Err(e) => {
                log::warn!("[sync] no account to publish with: {e}");
                return;
            }
        };
        let known = cfg.sync.published_digest.clone();
        emit_sync(&app, SyncEvent::phase("publishing"));
        match paintsync::publish_all(&cfg, &token, &profile, Some(known.as_str())).await {
            Ok(o) => {
                if o.unchanged {
                    log::debug!("[sync] {profile} is already published as {}", o.digest);
                } else {
                    log::info!(
                        "[sync] published {} paints across {} bikes for {profile}, {} uploaded",
                        o.published,
                        o.bikes,
                        o.uploaded
                    );
                    remember_publish(&app, &o);
                    usage::track("paint.publish");
                }
                emit_sync(&app, SyncEvent::published(&o));
            }
            Err(e) => {
                log::warn!("[sync] publishing {profile} failed: {e:#}");
                emit_sync(&app, SyncEvent::failed(format!("{e:#}")));
            }
        }
    });
}

/// Install everyone else's paints in the background, ahead of a session.
///
/// `address` is where the player is headed when we know it — joining by address — and
/// `None` when they pressed Play and will pick a server from the in-game browser. In that
/// second case there is nothing to resolve, so this syncs every server in the registry:
/// a superset of wherever they end up, which is the point.
///
/// Fired at launch rather than on a button because the paints have to be on disk *before*
/// the game reads them. The game loads a rider's look when they appear on track, so a sync
/// that happens after the grid forms is a sync that changes nothing this session.
fn sync_paints_soon(app: &tauri::AppHandle, address: Option<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let cfg = config::load_or_detect(&app).unwrap_or_default();
        if !cfg.paint_sync_enabled {
            return;
        }
        emit_sync(&app, SyncEvent::phase("pulling"));
        // Nobody asked for this one — it fires when the game starts, before the rider has
        // joined anything. With no server to aim at there is nothing worth reading.
        match pull_rosters(&app, address, Sweep::Never).await {
            Ok(o) => {
                log::info!(
                    "[sync] {} riders, {} paints installed, {} already held, {} kept as yours, \
                     {} clashed, {} refused",
                    o.riders,
                    o.installed,
                    o.already_had,
                    o.kept_yours,
                    o.conflicted,
                    o.rejected
                );
                emit_sync(&app, SyncEvent::pulled(&o));
            }
            Err(e) => {
                log::warn!("[sync] automatic sync failed: {e}");
                emit_sync(&app, SyncEvent::failed(&e));
            }
        }
    });
}

/// The fallback heartbeat while a session is running.
///
/// Not the gap between a rider arriving and their paint appearing — `GRID_POLL` below
/// watches the entry list and pulls within seconds of a new name, which is the case that
/// matters in a race. This only covers what the grid cannot announce: a rider who was
/// already there and has since changed their look, and keeping our own presence inside the
/// endpoint's ten-minute window.
///
/// Three minutes rather than the 45 seconds it ran at. At 45s every player in a session was
/// a request a minute and a half, and with paint sync on by default that was most of what
/// took the worker past its daily ceiling on 2026-08-31. Nothing was bought for it: an
/// unchanged roster is the overwhelmingly common answer, and the arrival path already
/// covers the change anyone would notice.
const LIVE_SYNC_EVERY: std::time::Duration = std::time::Duration::from_secs(180);

/// How often to read the grid out of FrostMod.
///
/// A shared-memory read and a set lookup, so this is close to free — it is the *pull* that
/// costs, and that still only happens when the grid changed or the heartbeat came due. Five
/// seconds is the gap between a rider appearing on track and their paint being fetched.
const GRID_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// How long to wait for the game to show up before giving up on a session.
///
/// MX Bikes takes a while to appear in the process list, and on a platform where we cannot
/// see processes at all it never will. Either way, stopping is right: the launch pull has
/// already run.
const LIVE_SYNC_STARTUP_GRACE: std::time::Duration = std::time::Duration::from_secs(120);

/// Paint sync for a session that has just begun, whoever started it.
///
/// The Play button knows the moment it launches the game, but Steam and the desktop
/// shortcut don't tell us anything — and a rider who starts the game the way they always
/// have is exactly the rider this feature is for. So the session watcher calls this too,
/// and the pieces are written to be asked twice: publishing is deduplicated by digest, the
/// pull installs only what is missing, and the live loop refuses to start a second copy of
/// itself.
pub(crate) fn sync_on_game_started(app: &tauri::AppHandle, cfg: &AppConfig) {
    publish_paints_soon(app, cfg, None);
    live_sync_session(app, None);
    sync_paints_soon(app, None);
}

/// Whether a live sync loop is already running, so the paths that all want one — the Play
/// button, a join by address, and the watcher that notices a game started from Steam — get
/// exactly one between them rather than one each.
static LIVE_SYNC_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Clears [`LIVE_SYNC_RUNNING`] when dropped, however the loop it guards ended.
struct LiveSyncGuard;

impl Drop for LiveSyncGuard {
    fn drop(&mut self) {
        LIVE_SYNC_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A session's worth of syncing, so a rider who arrives after you do still renders.
///
/// The pull used to happen once, at launch. That made the whole thing lopsided: whoever
/// joined last saw everybody, and whoever was there first never saw anyone who turned up
/// afterwards — they had pulled before those riders existed.
///
/// Runs until the game exits. Each pass also re-reports presence, which is what keeps this
/// rider in other people's rosters; the control plane forgets anyone who goes quiet.
fn live_sync_session(app: &tauri::AppHandle, address: Option<String>) {
    if LIVE_SYNC_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Cleared however the loop ends, or the first session would be the last one this
        // run of the app ever syncs.
        let _running = LiveSyncGuard;
        let started = std::time::Instant::now();
        let mut seen_running = false;
        let mut grid = GridWatch::default();
        let mut last_pull = std::time::Instant::now();
        loop {
            tokio::time::sleep(GRID_POLL).await;

            let cfg = config::load_or_detect(&app).unwrap_or_default();
            if !cfg.paint_sync_enabled {
                return;
            }
            // The game is the session. Once it has been seen and then gone, so are we.
            if gameproc::is_game_running() {
                seen_running = true;
            } else if seen_running || started.elapsed() > LIVE_SYNC_STARTUP_GRACE {
                log::info!("[sync] session over, stopping the live sync");
                return;
            }

            // Someone new on the grid is the reason to pull; the heartbeat is the fallback
            // for everything the grid can't tell us — a rider who was already there when we
            // joined and has since changed their look, and keeping our own presence fresh so
            // we stay in everyone else's roster.
            let arrivals = match live_session().filter(|s| s.on_a_server()) {
                Some(session) => {
                    let key = voice::session::room_key(&session.server_name);
                    grid.arrivals(&key, session.riders.iter().map(|r| r.name.as_str()))
                }
                None => 0,
            };
            let due = last_pull.elapsed() >= LIVE_SYNC_EVERY;
            if arrivals == 0 && !due {
                continue;
            }
            if arrivals > 0 {
                log::info!("[sync] {arrivals} rider(s) joined the grid; pulling now");
            }
            last_pull = std::time::Instant::now();

            match pull_rosters(&app, address.clone(), Sweep::Never).await {
                // Only say so when something actually arrived: an unchanged grid is the
                // common case and does not need announcing every 45 seconds.
                Ok(o) if o.installed > 0 => {
                    log::info!("[sync] {} new paints mid-session", o.installed);
                    emit_sync(&app, SyncEvent::pulled(&o));
                    // The files are on disk but the game read its grid when it built it.
                    // Same loader call a save on disk gets, for the same reason: a rider
                    // who is already out there shouldn't have to rejoin to stop seeing
                    // default liveries.
                    refresh_live_look(&app);
                }
                Ok(_) => {}
                Err(e) => log::debug!("[sync] live pull failed: {e}"),
            }
        }
    });
}

/// The server the game is on right now, as the key every rider on it computes.
///
/// The roster has to be scoped to one grid, and for most of the game's life we had no way to
/// name that grid: an address only reaches the riders whose app launched the game with
/// `-directconnect`, and anyone who picked the server from the in-game browser never has
/// one. So paint sync fell back to the registry — our own servers — and a rider on a
/// community server synced with nobody.
///
/// FrostMod is inside the process and `EventInit` hands it the server *name*, which every
/// rider on that server sees identically. Voice already rooms on it. Sharing the key is what
/// makes both features work on servers whose operator has installed nothing and knows
/// nothing about us.
///
/// `None` when FrostMod isn't running, the game isn't up, or the rider is in the menus, in a
/// replay, or testing alone — none of which is a grid to sync with.
fn live_server_key() -> Option<String> {
    let session = live_session()?;
    session
        .on_a_server()
        .then(|| voice::session::room_key(&session.server_name))
}

/// The session FrostMod is publishing, if any.
///
/// One reader for the whole app, held open across calls: opening the mapping is the
/// expensive part and the block is read every few seconds.
fn live_session() -> Option<voice::gamesession::GameSession> {
    static GAME: std::sync::OnceLock<voice::gamesession::Reader> = std::sync::OnceLock::new();
    GAME.get_or_init(voice::gamesession::Reader::default).read()
}

/// What the running game says this player is called, for [`identity::claim_from_game`].
///
/// `None` when there is no session to read — no FrostMod, or one that has not reached
/// `EventInit` yet — which is not the same as a session that reports empty fields.
pub fn seen_identity() -> Option<identity::SeenIdentity> {
    live_session().map(|s| identity::SeenIdentity {
        guid: s.guid,
        rider_name: s.rider_name,
    })
}

/// Who is on the grid, and who has turned up since we last looked.
///
/// A rider who joins after you is the case paint sync is worst at: they were not on the
/// roster when you pulled, so they render in default livery until something pulls again.
/// Waiting out the heartbeat means up to [`LIVE_SYNC_EVERY`] of a wrong-looking grid, and
/// the rider who joined is the one person guaranteed to be looking at it.
///
/// So the grid itself is the trigger. The game knows the entry list, FrostMod publishes it,
/// and a name that wasn't there last time is a reason to pull now.
#[derive(Default)]
struct GridWatch {
    /// The server these names belong to. A different one is a different grid, so the names
    /// carried over from the last server mean nothing and are dropped.
    server: String,
    seen: std::collections::HashSet<String>,
}

impl GridWatch {
    /// How many riders on `grid` we hadn't seen on `server` before.
    ///
    /// Folded like a room key, because the entry list is what the game reports and one
    /// rider must not read as two because their name arrived spaced differently.
    fn arrivals<'a>(&mut self, server: &str, grid: impl Iterator<Item = &'a str>) -> usize {
        if self.server != server {
            self.server = server.to_string();
            self.seen.clear();
        }
        grid.map(|name| name.trim().to_lowercase())
            .filter(|name| !name.is_empty())
            .filter(|name| self.seen.insert(name.clone()))
            .count()
    }
}

/// Pull the rosters for wherever the player is, or could be, riding.
///
/// Once the game is on a server, that server is the answer and `address` is ignored — see
/// [`live_server_key`]. Before then there is nothing to read, so `address` narrows this to
/// where they're headed when we launched them at one, and to the whole registry when they
/// pressed Play and will pick from the game's own browser.
/// Whether a pull with nowhere to aim may fall back to every server on the registry.
///
/// It is a whole-platform sweep — one roster read per registered server, each one returning
/// the paints of everybody on it — so it is only ever right when a person asked for it. On
/// 2026-09-01 the automatic paths doing this took the account past D1's daily row ceiling and
/// every endpoint that reads the database answered 500 for the rest of the day.
///
/// The same reasoning already applies to the write half: reporting presence for every key was
/// removed from this function for being untrue. Reading every roster is the other half of the
/// same mistake, and it is the expensive half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sweep {
    /// A person pressed something. One sweep, now.
    Allowed,
    /// Nobody asked. With no server and no address there is nothing to sync *for* — the
    /// grid this feature exists to render does not exist yet.
    Never,
}

/// Which servers to ask about, once it is settled that the rider is not on one.
///
/// An address is a server, whoever gave it to us. Without one, the registry is every server
/// there is, and asking all of them is a thing only a person may set off.
fn sync_targets(address_key: Option<String>, registry_ids: &[String], sweep: Sweep) -> Vec<String> {
    match address_key {
        Some(key) => vec![key],
        None => match sweep {
            Sweep::Allowed => registry_ids.to_vec(),
            Sweep::Never => Vec::new(),
        },
    }
}

async fn pull_rosters(
    app: &tauri::AppHandle,
    address: Option<String>,
    sweep: Sweep,
) -> Result<paintsync::PullOutcome, String> {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    let token = voice::signal::account(app, &cfg).await?;

    // Where the rider actually is beats everything else we could guess. The name FrostMod
    // reads out of the running game is the one key every rider on that server can compute,
    // so it is the only one that works on a server we have nothing to do with — which is
    // most of them.
    let live = live_server_key();
    // Nothing to aim at and no sweep allowed: stop before the registry is even fetched. That
    // read is not free either — it was 20,869 calls the day the database hit its ceiling, and
    // every one of them was a prelude to reading a roster we had no business reading.
    if live.is_none() && address.is_none() && sweep == Sweep::Never {
        return Err("No servers to sync with yet.".into());
    }
    let keys: Vec<String> = if let Some(key) = live {
        vec![key]
    } else {
        // Not on a server yet. An unreachable registry doesn't have to sink a targeted sync
        // — the address is a usable key on its own — but with no address there's nothing
        // left to aim at.
        let registry = match paintsync::registry(Some(&token)).await {
            Ok(list) => list,
            Err(e) => {
                log::warn!("[sync] couldn't read the server registry: {e:#}");
                Vec::new()
            }
        };
        let ids: Vec<String> = registry.iter().map(|s| s.id.clone()).collect();
        sync_targets(address.as_deref().map(|a| paintsync::server_key_for(&registry, a)), &ids, sweep)
    };
    if keys.is_empty() {
        return Err("No servers to sync with yet.".into());
    }

    // Where we are rides along with the request that asks who else is here — the roster
    // records it and then scopes itself by it, so this is one request rather than two.
    //
    // Only the server the rider is actually on. Reporting presence for every key used to
    // happen here, which on a registry sweep meant claiming to be on every server at once:
    // untrue, and one write per server for the privilege.
    let here = live_server_key();
    let outcome = paintsync::pull(&cfg, &token, &keys, here.as_deref())
        .await
        .map_err(|e| format!("{e:#}"))?;
    // Re-read immediately before writing: the pull took a round trip, and `config::save`
    // rewrites the whole file.
    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    cfg.sync.pulled_at = now_ms();
    cfg.sync.pulled_riders = outcome.riders;
    cfg.sync.kept_yours = outcome.kept_yours;
    cfg.sync.conflicted = outcome.conflicted;
    if let Err(e) = config::save(app, &cfg) {
        log::warn!("[sync] couldn't record the pull: {e:#}");
    }
    // Anything newly on disk is invisible to a running game until the loader re-reads the
    // mods folder — but only *newly*. This used to fire on every pull, and a pull that
    // installed nothing is the overwhelmingly common one: the grid is unchanged, everyone's
    // paints are already here, and there is nothing for the game to re-read. Asking it to
    // rescan the mods folder anyway, every time a rider joined, is work done to a process
    // that is mid-race.
    if outcome.installed > 0 {
        let _ = frostmod::signal_reload();
    }
    Ok(outcome)
}

/// Install every other rider's paints, so the grid renders correctly.
///
/// The app does this on its own at launch; this is the manual retry for when that ran
/// before a rider had published, or failed on a flaky connection.
#[tauri::command]
async fn sync_paints(app: tauri::AppHandle) -> Result<paintsync::PullOutcome, String> {
    emit_sync(&app, SyncEvent::phase("pulling"));
    // The one place a sweep is right: a person pressed Sync, and if they are not on a server
    // the whole registry is the only answer to "whose paints do you mean".
    match pull_rosters(&app, None, Sweep::Allowed).await {
        Ok(o) => {
            emit_sync(&app, SyncEvent::pulled(&o));
            Ok(o)
        }
        Err(e) => {
            emit_sync(&app, SyncEvent::failed(&e));
            Err(e)
        }
    }
}

/// The servers the control plane knows about — what the join picker offers instead of
/// asking a player to find and type an IP address.
///
/// Works without an account. Gating it on enrollment meant the people most in need of the
/// list — the ones who have never joined a server and have no address to type — were the
/// only ones who couldn't see it.
#[tauri::command]
async fn cp_servers(app: tauri::AppHandle) -> Result<Vec<paintsync::RegisteredServer>, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let token = Some(cfg.cp_token.as_str()).filter(|t| !t.trim().is_empty());
    paintsync::registry(token).await.map_err(|e| format!("{e:#}"))
}

/// The dedicated servers this player administers.
#[tauri::command]
fn list_servers(app: tauri::AppHandle) -> Vec<servers::ServerRef> {
    config::load_or_detect(&app).unwrap_or_default().servers
}

/// Replace the saved server list. The UI owns add/edit/remove and sends the whole list,
/// which keeps ordering and identity in one place rather than split across three commands.
#[tauri::command]
fn save_servers(app: tauri::AppHandle, servers: Vec<servers::ServerRef>) -> Result<(), String> {
    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.servers = servers;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Look up one saved server by id, so the commands below take an id rather than having the
/// frontend hand back a token it was given.
/// Servers the control plane runs for this account, as last fetched.
///
/// Held in memory rather than saved to `config.json`, because the token in each one belongs
/// to a machine the control plane can re-issue at any time — and a credential this app never
/// writes to disk is one that cannot go stale there or be read out of it. Refreshed whenever
/// the Servers page asks, which is also whenever anything is about to act on one.
#[derive(Default)]
struct CloudServers(std::sync::Mutex<Vec<servers::ServerRef>>);

fn server_by_id(app: &tauri::AppHandle, id: &str) -> Result<servers::ServerRef, String> {
    if let Some(saved) =
        config::load_or_detect(app).unwrap_or_default().servers.into_iter().find(|s| s.id == id)
    {
        return Ok(saved);
    }
    // Not one they paired by hand, so it's one the control plane launched for them. These
    // never reach the saved list — nothing on that box prints a pairing code anyone can read.
    app.state::<CloudServers>()
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| "That server isn't in your list any more.".to_string())
}

/// A server run for this account by the control plane.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudServer {
    id: String,
    name: String,
    region: String,
    /// `host:port` players connect to. Empty until the box has announced itself.
    address: String,
    #[serde(default)]
    agent_url: Option<String>,
    #[serde(default)]
    agent_token: Option<String>,
    #[serde(default)]
    instance_id: Option<String>,
    published: bool,
    created_at: u64,
    /// When the server was last seen with nobody on it, or `null` while someone is riding.
    #[serde(default)]
    idle_since: Option<u64>,
    /// Minutes of emptiness before it destroys itself.
    idle_minutes: u64,
    /// `pending` | `running` | `stopping` | `stopped` | `gone` | `self-hosted`.
    state: String,
    #[serde(default)]
    public_ip: Option<String>,
}

/// The servers the control plane is running for this account.
///
/// A provisioned box has no console and prints its pairing code to nobody, so this is the
/// only way its owner can ever obtain the token that drives it. Fetching it also refreshes
/// the in-memory list `server_by_id` falls back to, which is what makes Start, Stop and Set
/// track work on a machine the player has never touched.
#[tauri::command]
async fn cloud_servers(app: tauri::AppHandle) -> Result<Vec<CloudServer>, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        servers: Vec<CloudServer>,
    }
    let resp = reqwest::Client::new()
        .get(format!("{}/v1/servers/mine", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text));
    }
    let body: Resp = resp.json().await.map_err(|e| format!("{e}"))?;

    // Only the ones we can actually talk to become drivable; a box still booting has no
    // agent yet, and an entry with no token is one the control plane declined to hand over.
    *app.state::<CloudServers>().0.lock().unwrap() = body
        .servers
        .iter()
        .filter_map(|s| {
            Some(servers::ServerRef {
                id: s.id.clone(),
                name: s.name.clone(),
                url: s.agent_url.clone()?,
                token: s.agent_token.clone()?,
                registry_id: s.published.then(|| s.id.clone()).unwrap_or_default(),
            })
        })
        .collect();
    Ok(body.servers)
}

/// Destroy a server the control plane runs, and stop paying for it.
#[tauri::command]
async fn destroy_cloud_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .delete(format!("{}/v1/servers/{id}", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    // Already gone is the state we wanted.
    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NOT_FOUND {
        let text = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text));
    }
    app.state::<CloudServers>().0.lock().unwrap().retain(|s| s.id != id);
    Ok(())
}

#[tauri::command]
async fn server_status(app: tauri::AppHandle, id: String) -> Result<serde_json::Value, String> {
    let server = server_by_id(&app, &id)?;
    let status = servers::status(&server).await?;
    claim_guid_from_roster(&app, &server).await;
    Ok(status)
}

/// Claim this player's own GUID the first time one of their servers sees them connect.
///
/// The GUID is the identity the roster keys on, and it used to be a 32-character field the
/// player had to find and type. They can't read it off their own machine — the game's
/// plugin API exposes it only for the local player, to a plugin, in-process — but the
/// dedicated server writes it next to their name on every connection, and the agent already
/// parses exactly that. So the app waits until it sees the name it enrolled under connected
/// to a server this player administers, and takes the GUID from there.
///
/// Runs off the status poll the Servers page already makes, and short-circuits the moment a
/// GUID is held, so it costs one extra request per poll only while still unclaimed.
async fn claim_guid_from_roster(app: &tauri::AppHandle, server: &servers::ServerRef) {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if !cfg.cp_guid.trim().is_empty() || cfg.cp_token.trim().is_empty() {
        return;
    }
    let rider = cfg.cp_rider_name.trim();
    if rider.is_empty() {
        return;
    }
    let Ok(players) = servers::players(server).await else { return };
    // Matched case-insensitively for the same reason the control plane's unique index is:
    // the player typed this name into the game and into the app on two separate occasions.
    let Some(me) = players
        .iter()
        .find(|p| p.name.trim().eq_ignore_ascii_case(rider) && !p.guid.trim().is_empty())
    else {
        return;
    };

    match claim_guid(app, &me.guid).await {
        Ok(()) => log::info!("[sync] claimed GUID {} for {rider}", me.guid),
        // First-come on the server side, so a rejection here is a real answer — someone
        // else holds it — not a transient failure worth retrying into a loop.
        Err(e) => log::warn!("[sync] couldn't claim GUID {} for {rider}: {e}", me.guid),
    }
}

#[tauri::command]
async fn server_tracks(app: tauri::AppHandle, id: String) -> Result<Vec<String>, String> {
    servers::tracks(&server_by_id(&app, &id)?).await
}

/// Create a server: the control plane launches a machine for it.
///
/// The app never talks to AWS. A desktop binary can be unpacked, so a cloud credential
/// inside one would let anyone create infrastructure in our account — the control plane
/// holds the key and this asks it nicely, authenticated as this player.
#[tauri::command]
async fn provision_server(app: tauri::AppHandle, name: String) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/provision", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .json(&serde_json::json!({ "name": name.trim() }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;

    let ok = resp.status().is_success();
    let text = resp.text().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !ok {
        return Err(body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(text));
    }
    Ok(body)
}

/// What's running, and therefore what's being paid for.
///
/// Read from EC2 rather than from anyone's records, because that is the number that turns
/// into a bill.
#[tauri::command]
async fn fleet_state(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .get(format!("{}/v1/fleet", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    let ok = resp.status().is_success();
    let text = resp.text().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !ok {
        return Err(body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(text));
    }
    Ok(body)
}

/// Put a server the player runs into the public list, so other people can find it.
///
/// Everything the control plane needs is already known to the agent, so nothing here is
/// asked of the operator: the game address is the agent's own host joined to the port it
/// reports, and the name comes from the server's `.ini`. What the player supplies is the
/// decision to publish, and a region — which is the one fact no machine can infer.
///
/// The agent URL is sent so the control plane can check the box actually answers before
/// advertising it. That check is why an unreachable home server doesn't end up as a row in
/// everyone's join picker that nobody can connect to.
#[tauri::command]
async fn publish_server(
    app: tauri::AppHandle,
    id: String,
    region: String,
) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let server = server_by_id(&app, &id)?;
    let status = servers::status(&server).await?;

    let port = status
        .get("port")
        .and_then(|p| p.as_u64())
        .ok_or("The agent didn't say which port the server runs on.")?;
    let host = servers::host_of(&server.url)?;
    let name = status
        .get("server")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(&server.name)
        .to_string();

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/servers", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .json(&serde_json::json!({
            "name": name,
            "region": region,
            "address": format!("{host}:{port}"),
            "agentUrl": server.url,
        }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;

    let status_code = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !status_code.is_success() {
        return Err(body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(text));
    }

    // Remember the registry id: it is the only handle that can withdraw this row later, and
    // the control plane will never hand it out a second time.
    if let Some(registry_id) = body.get("id").and_then(|v| v.as_str()) {
        let mut cfg = config::load_or_detect(&app).unwrap_or_default();
        if let Some(saved) = cfg.servers.iter_mut().find(|s| s.id == id) {
            saved.registry_id = registry_id.to_string();
            config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
        }
    }
    Ok(body)
}

/// Take a server back out of the public list.
#[tauri::command]
async fn unpublish_server(app: tauri::AppHandle, registry_id: String) -> Result<(), String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .delete(format!("{}/v1/servers/{registry_id}", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    // A row already gone from the control plane is the state we wanted; clearing our end
    // regardless keeps a 404 from stranding the local entry as permanently "published".
    let gone = resp.status() == reqwest::StatusCode::NOT_FOUND;
    if !resp.status().is_success() && !gone {
        let text = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text));
    }

    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    if let Some(saved) = cfg.servers.iter_mut().find(|s| s.registry_id == registry_id) {
        saved.registry_id.clear();
        config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}

/// Unpack the one-line code `mxb-agent` prints, so adding a server is a paste rather than
/// an address, a token and a name typed in by hand.
#[tauri::command]
fn parse_pairing(blob: String) -> Result<servers::Pairing, String> {
    servers::parse_pairing(&blob)
}

/// Ask an agent to name itself, before it's saved to the list.
///
/// Lets the add form fill the server's name in from its `.ini` rather than having the
/// operator retype something the host already knows, and doubles as the check that the
/// address and token are right — a typo shows up here instead of as a dead row.
#[tauri::command]
async fn server_probe(url: String, token: String) -> Result<serde_json::Value, String> {
    let probe = servers::ServerRef {
        url: url.trim().to_string(),
        token: token.trim().to_string(),
        ..Default::default()
    };
    servers::status(&probe).await
}

#[tauri::command]
async fn server_action(
    app: tauri::AppHandle,
    id: String,
    action: servers::Action,
) -> Result<serde_json::Value, String> {
    servers::act(&server_by_id(&app, &id)?, action).await
}

#[tauri::command]
async fn server_set_config(
    app: tauri::AppHandle,
    id: String,
    patch: serde_json::Value,
) -> Result<serde_json::Value, String> {
    servers::set_config(&server_by_id(&app, &id)?, patch).await
}

/// Start MX Bikes and connect it straight to a server.
///
/// The game reads the connect flag only at startup, so this reports `already_running`
/// rather than trying to steer a copy that's already up.
#[tauri::command]
fn join_server(app: tauri::AppHandle, address: String) -> Result<gameproc::LaunchOutcome, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let outcome = gameproc::join(&cfg, &address).map_err(|e| format!("{e:#}"))?;
    if matches!(outcome, gameproc::LaunchOutcome::Launched) {
        usage::track("server.join");
        publish_paints_soon(&app, &cfg, None);
        live_sync_session(&app, Some(address.clone()));
        // We know exactly where they're going, so this syncs that server alone.
        sync_paints_soon(&app, Some(address));
    }
    Ok(outcome)
}

/// One live server as the Servers tab shows it. Filled by the local-only `worldnet` module
/// from the master-server list; a superset of [`paintsync::RegisteredServer`] so the tab's
/// Join button reuses [`join_server`]. The struct carries no protocol detail — that all lives
/// behind `cfg(worldnet)` — so it stays in the public tree and the command compiles either way.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldServer {
    /// Display name the operator gave the server.
    pub name: String,
    /// `ip:port`, the form the game's connect flag and [`join_server`] take.
    pub address: String,
    /// Whether the game can reach that address at all. False for a genuine IPv6 server: the
    /// client's own socket is `AF_INET`, so the row is worth showing but its Join is not.
    pub joinable: bool,
    /// The address the server reports for itself, which the master forwards untouched. Usually
    /// its LAN address, so it is shown as detail rather than offered as somewhere to connect.
    pub lan_address: String,
    /// Riders currently connected, and the seat cap.
    pub players: u32,
    pub max_players: u32,
    /// Round-trip to the server in milliseconds, or `None` if it wasn't measured. The master
    /// never sends this — it is timed against the server's own `GETINFO` reply.
    pub ping_ms: Option<u32>,
    /// Whether a password is required to join.
    pub passworded: bool,
    /// The operator's free-text `[connection] location` — "USA", "EU West". Not the track.
    pub location: String,
    /// The licence class the server requires: "D", "C", "B", "A", or empty for none.
    pub rating: String,
    /// What the server is running, from the event blob the game publishes about itself.
    pub track: String,
    pub track_layout: String,
    /// Bike categories and models the server allows; empty means anything.
    pub categories: Vec<String>,
    pub bikes: Vec<String>,
    /// The session in progress — "Practice", "Race 1", "Waiting" — and its length.
    pub session: String,
    pub race_length: String,
    /// "Sunny", "Cloudy", "Rainy", and whether weather evolves during the session.
    pub conditions: String,
    pub realistic_weather: bool,
    /// The three rules a rider notices before joining.
    pub force_cockpit: bool,
    pub no_aids: bool,
    pub limited_tyre_sets: bool,
    /// Why the spam filter would hide this row, or empty to show it. The row is sent either
    /// way: the tab shows a count of what was hidden and can reveal it, and someone who thinks
    /// a rule is wrong has to be able to see what it caught. See [`serverfilter`].
    pub hidden: String,
}

/// Who the app can name on a server, and where the names came from.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRiders {
    pub riders: Vec<String>,
    /// `"session"` when it is the game's own roster for the server you are on — every rider,
    /// named by the server. `"app"` when it is the riders whose own apps reported themselves
    /// there, which is a subset and has to be labelled as one.
    pub source: String,
}

/// The riders on a server, as far as anything can honestly say.
///
/// MX Bikes will not tell a stranger who is on a server. `GETINFO` answers with a rider count
/// and a seat count and nothing else, and the roster message is only ever built inside a live
/// session — so a full list for a server you are not on does not exist to be fetched.
///
/// That leaves two real answers, and the panel says which one it is showing. If you are on the
/// server, FrostMod is in the session and hands over the actual grid. Otherwise the control
/// plane knows where each rider's *app* said it was, which names the players who run Frost's Mod Manager
/// and nobody else.
#[tauri::command]
async fn server_riders(
    app: tauri::AppHandle,
    address: String,
    name: String,
) -> Result<ServerRiders, String> {
    // The game's own roster, when this is the server under us. `room_key` folds both sides so
    // capitalisation or a stray space can't make a server fail to match itself.
    if let Some(session) = live_session() {
        if session.on_a_server()
            && voice::session::room_key(&session.server_name) == voice::session::room_key(&name)
        {
            let riders = session.riders.into_iter().map(|r| r.name).collect();
            return Ok(ServerRiders { riders, source: "session".into() });
        }
    }

    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let token = Some(cfg.cp_token.as_str()).filter(|t| !t.trim().is_empty());

    // Both key forms, because presence is recorded under whichever one the reporting app had:
    // the address for a rider who joined through the app, the folded server name for one whose
    // session FrostMod detected. See `paintsync::who_is_on`.
    let mut keys = Vec::new();
    if !address.trim().is_empty() {
        let registry = paintsync::registry(token).await.unwrap_or_default();
        keys.push(paintsync::server_key_for(&registry, &address));
    }
    let named = voice::session::room_key(&name);
    if !named.is_empty() && !keys.contains(&named) {
        keys.push(named);
    }
    if keys.is_empty() {
        return Ok(ServerRiders::default());
    }

    let riders = paintsync::who_is_on(token, &keys).await.map_err(|e| format!("{e:#}"))?;
    Ok(ServerRiders { riders, source: "app".into() })
}

/// One row of the server browser, as much of it as naming a server takes.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRow {
    pub name: String,
    pub address: String,
}

/// How many riders on each of these servers are running paint sync, by address.
///
/// The detail panel asks [`server_riders`] who is on one server; a browser row only needs to
/// know whether joining would put the player among people they can actually sync with, and
/// asking that server-by-server would be one request per row. This is the whole list in one
/// request — plus the registry, which is what turns an address into the key a rider who
/// joined through the app was recorded under.
///
/// A row with nobody on it is absent rather than zero: the badge is a thing that is there or
/// isn't, and a map of mostly-zeroes is a bigger answer than the question deserves.
///
/// Best-effort throughout. A control plane that can't be reached means no badges, which is
/// the same as the browser has always looked — never an error over the list itself.
#[tauri::command]
async fn servers_with_paint_sync(
    app: tauri::AppHandle,
    servers: Vec<ServerRow>,
) -> Result<std::collections::HashMap<String, u32>, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let token = Some(cfg.cp_token.as_str()).filter(|t| !t.trim().is_empty());
    let counts = paintsync::presence_counts(token).await.map_err(|e| format!("{e:#}"))?;
    if counts.is_empty() {
        return Ok(Default::default());
    }
    // Only worth fetching once there is something to map onto, and only for the address key:
    // a server nobody registered is keyed by its own address either way.
    let registry = paintsync::registry(token).await.unwrap_or_default();

    Ok(match_presence(&counts, &registry, &servers))
}

/// Resolve each browser row to the number of synced riders standing on it.
///
/// Split out from the command because this is the whole of the thinking: a server is recorded
/// under two different keys depending on how the reporting app found out where it was, so a
/// row has to be looked up twice and the answers reconciled.
///
/// **The larger of the two, never the sum.** Both keys can hold the same rider — their app
/// reported the address, the rider beside them reported the folded name — and nothing here can
/// tell a second rider from the same one counted twice. Adding them would show four riders on
/// a server holding two, which is worse than showing two on a server holding three.
fn match_presence(
    counts: &std::collections::HashMap<String, u32>,
    registry: &[paintsync::RegisteredServer],
    servers: &[ServerRow],
) -> std::collections::HashMap<String, u32> {
    let mut present = std::collections::HashMap::new();
    for row in servers {
        let by_address = counts.get(&paintsync::server_key_for(registry, &row.address));
        let by_name = counts.get(&voice::session::room_key(&row.name));
        let riders = by_address.into_iter().chain(by_name).copied().max().unwrap_or(0);
        // Absent rather than zero: the badge is a thing that is there or isn't.
        if riders > 0 {
            present.insert(row.address.clone(), riders);
        }
    }
    present
}

/// What track a server is running, matched against what the player has and what they could get.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackGuess {
    /// The internal id the server published, e.g. `mmx_supercross`.
    pub id: String,
    /// The installed track's file or folder name, empty when it isn't installed.
    pub installed: String,
    /// The installed track's own preview image, as a data URL.
    pub preview: String,
    /// Where a copy could come from when it isn't installed: `"shop"`, `"hub"`, or empty.
    pub source: String,
    pub product_id: u64,
    pub product_name: String,
    pub product_url: String,
    pub product_image: String,
    /// False when the name only resembles the track rather than matching it. The panel says
    /// "we think" for these, because an internal id is not a product title and a fold of one
    /// onto the other is a guess however good it looks.
    pub exact: bool,
}

/// Work out which track a server means.
///
/// The server publishes an internal id — `mmx_supercross` — and nothing else. That is not a
/// product title, not a folder name, and not something a player can search for, which is why
/// the tab has always shown it raw and left everyone to guess.
///
/// Three answers in order of how much they are worth: the track is installed and the panel can
/// show its own preview; there is an exact catalogue match and the panel can offer it; the name
/// merely resembles something, which is offered as a guess and labelled as one.
#[tauri::command]
async fn guess_server_track(app: tauri::AppHandle, track: String) -> Result<TrackGuess, String> {
    let id = track.trim().to_string();
    let mut guess = TrackGuess { id: id.clone(), ..Default::default() };
    if id.is_empty() {
        return Ok(guess);
    }

    // Installed wins outright: nothing to buy, and the track's own artwork beats a shop photo.
    if let Ok(entries) = scan_library(app.clone(), "tracks".into()).await {
        let want = fold_name(&id);
        if let Some(hit) = entries.iter().find(|e| fold_name(&e.name) == want) {
            guess.installed = hit.name.clone();
            guess.exact = true;
            guess.preview = pkz::read_preview(std::path::Path::new(&hit.path))
                .ok()
                .flatten()
                .unwrap_or_default();
            return Ok(guess);
        }
    }

    // The shop's own matcher, which is the fold the Browse grid already trusts to decide
    // whether something is installed — reused here rather than a second opinion about names.
    if let Ok(hits) = mods::shop_catalog::match_products(&app, &[id.clone()]).await {
        if let Some(hit) = hits.into_iter().flatten().next() {
            return Ok(shop_guess(guess, hit, true));
        }
    }
    // Nothing matched outright, so fall back to searching for it. The id is snake_case and a
    // product title is not, so the underscores become spaces before either catalogue sees it.
    let words = id.replace('_', " ");
    if let Ok(page) =
        mods::shop_catalog::search(&app, &words, None, 1, mods::shop_catalog::ShopSort::default(), false).await
    {
        if let Some(hit) = page.items.into_iter().next() {
            return Ok(shop_guess(guess, hit, false));
        }
    }

    // Nothing in the shop; the hub is the other half of where tracks come from. Asked directly
    // rather than through `with_hub_clearance`: that answers the robot challenge by opening a
    // browser, and this runs from opening a panel. A browser window appearing because someone
    // clicked a server row would be an ambush, so a challenge here simply means no guess.
    if let Ok(page) = mods::hub::search(&words, None, 1, mods::hub::HubSort::default(), false).await {
        if let Some(hit) = page.items.into_iter().next() {
            guess.source = "hub".into();
            guess.product_id = hit.id;
            guess.product_name = hit.title;
            guess.product_url = hit.url.unwrap_or_default();
            guess.product_image = hit.image.unwrap_or_default();
        }
    }
    Ok(guess)
}

fn shop_guess(mut guess: TrackGuess, hit: mods::shop_catalog::ShopMod, exact: bool) -> TrackGuess {
    guess.source = "shop".into();
    guess.product_id = hit.id;
    guess.product_name = hit.title;
    guess.product_url = hit.url.unwrap_or_default();
    guess.product_image = hit.image.unwrap_or_default();
    guess.exact = exact;
    guess
}

/// Lowercase and reduce everything that isn't alphanumeric to a single space, so an internal
/// id (`mmx_supercross`) and a folder or product name ("MMX Supercross") fold together. The
/// same rule the shop catalogue matches on, so the two agree about what counts as the same
/// name.
fn fold_name(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Which GUID the Ranked tab will ask about, and where it came from.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RankedIdentity {
    /// Empty when we have nothing to go on and the player has to type one.
    guid: String,
    /// `"steam"` when it was derived from the signed-in Steam account, `"manual"` when the
    /// player typed it. The tab says which, because a wrong-but-plausible GUID would
    /// otherwise show somebody else's season with no clue why.
    source: String,
}

/// The GUID this machine's profile lives under.
///
/// A Steam copy needs no setup: MX Bikes' GUID is `FF` + the SteamID64, and Steam records the
/// signed-in account on disk. A copy bought direct from PiBoSo has a stand-alone GUID that
/// only mxb-ranked knows, so that one is typed in and kept in config — which is the same
/// field used to point the tab at a friend.
#[tauri::command]
fn ranked_identity(app: tauri::AppHandle) -> RankedIdentity {
    let manual = config::load_or_detect(&app).unwrap_or_default().ranked_guid;
    if let Some(guid) = ranked::normalise_guid(&manual) {
        return RankedIdentity { guid, source: "manual".into() };
    }
    match ranked::local_guid() {
        Some(guid) => RankedIdentity { guid, source: "steam".into() },
        None => RankedIdentity::default(),
    }
}

/// One rider's rank, season standings and last 50 races, read off mxb-ranked.com.
///
/// `guid` names whose — omit it for this machine's own. There is no sign-in: the profile is
/// public and server-rendered, so this is one request and no account. See [`ranked`].
#[tauri::command]
async fn ranked_profile(
    app: tauri::AppHandle,
    guid: Option<String>,
) -> Result<ranked::RankedProfile, String> {
    let asked = guid.unwrap_or_default();
    let guid = match ranked::normalise_guid(&asked) {
        Some(g) => g,
        // Not "invalid": an empty argument is the tab asking for the player's own.
        None if asked.trim().is_empty() => ranked_identity(app).guid,
        None => return Err(format!("{asked} isn't an MX Bikes GUID")),
    };
    if guid.is_empty() {
        return Err("No MX Bikes GUID — sign into Steam, or enter your GUID.".into());
    }
    ranked::fetch(&guid).await
}

/// Remember a hand-entered MXB Ranked GUID, or clear it with an empty string.
///
/// Only players whose copy didn't come from Steam need this — everyone else's GUID is derived
/// — so a value that isn't a GUID is refused here rather than silently showing an empty
/// profile for ever.
#[tauri::command]
fn set_ranked_guid(app: tauri::AppHandle, guid: String) -> Result<(), String> {
    let cleaned = if guid.trim().is_empty() {
        String::new()
    } else {
        ranked::normalise_guid(&guid).ok_or_else(|| format!("{guid} isn't an MX Bikes GUID"))?
    };
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.ranked_guid = cleaned;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// The live MX Bikes server list, as the game's WORLD browser sees it.
///
/// All the work — the master-server protocol, the Steam auth ticket, the parsing — lives in
/// the local-only `worldnet` module. Without it (the public tree, or a build that never had
/// the file) the tab still exists but says the browser isn't included, rather than failing
/// opaquely.
#[tauri::command]
async fn list_master_servers(app: tauri::AppHandle) -> Result<Vec<WorldServer>, String> {
    #[cfg(worldnet)]
    {
        worldnet::list_servers(app).await
    }
    #[cfg(not(worldnet))]
    {
        let _ = app;
        Err("The server browser isn't included in this build.".into())
    }
}

/// Ask one server about itself, right now.
///
/// The detail panel used to format whatever the list happened to hold, which on a busy evening
/// is minutes old — the rider count, the session and the track are all things that move while
/// somebody reads the row. `GETINFO` costs one datagram and no account, so the panel asks.
#[tauri::command]
async fn probe_server(address: String) -> Result<WorldServer, String> {
    #[cfg(worldnet)]
    {
        worldnet::probe_server(address).await
    }
    #[cfg(not(worldnet))]
    {
        let _ = address;
        Err("The server browser isn't included in this build.".into())
    }
}

/// Is MX Bikes running? Polled by the sidebar so Play can show the live state.
#[tauri::command]
fn game_running() -> bool {
    gameproc::is_game_running()
}

/// Installed bikes with their class, for the garage bike-switch UI. The frontend
/// filters this to the current race's class before offering a swap.
#[tauri::command]
async fn garage_scan_bikes(app: tauri::AppHandle) -> Result<Vec<bikeswap::BikeIdentity>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(bikeswap::scan_installed_bikes(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("garage_scan_bikes task failed: {e}"))?
}

/// Ask FrostMod to swap the active bike (offline, in-garage). FrostMod enforces the
/// offline/in-garage guard; this only sends the request.
#[tauri::command]
fn garage_swap_bike(bike_id: String) -> frostmod::CommandOutcome {
    frostmod::signal_swap_bike(&bike_id)
}

#[tauri::command]
async fn frostmod_status(app: tauri::AppHandle) -> FrostmodStatus {
    frostmod_manage::status(&app).await
}

#[tauri::command]
async fn frostmod_install(
    app: tauri::AppHandle,
    state: State<'_, FrostmodProcess>,
) -> Result<InstallReport, String> {
    let was_running = frostmod::is_running();
    let was_installed = frostmod_manage::is_installed(&app);
    frostmod_manage::stop(&state);
    frostmod_manage::force_stop_exe();

    let report = frostmod_manage::install(&app)
        .await
        .map_err(|e| format!("{e:#}"))?;

    usage::track("frostmod.install");
    if was_running || !was_installed {
        let _ = frostmod_manage::start(&app, &state);
    }
    Ok(report)
}

/// Install a Visual C++ runtime `frostmod_status` reported missing.
///
/// Raises a UAC prompt — Microsoft's redistributables require admin, and only the shell
/// can ask. A declined prompt comes back as `cancelled`, not an error, so the UI can fall
/// back to handing over the download link instead of reading as broken.
/// Install every Visual C++ runtime this machine is short of, and sweep the game folder for
/// the loose `msvcr90.dll` older builds of this app left there.
///
/// Deliberately not gated on `frostmod_status` having reported anything missing. The
/// machine this exists for reported everything present and still couldn't start the game,
/// so a repair reachable only from the warning bar would never have run there.
#[tauri::command]
async fn frostmod_repair_runtimes(app: tauri::AppHandle) -> vcruntime::RepairReport {
    vcruntime::repair(&app, game_dir_for_runtimes(&app).as_deref()).await
}

/// Move a loose `msvcr90.dll` beside the game exe out of the loader's way.
///
/// The player-consented counterpart to the sweep, which only ever deletes a copy this app
/// made. Reachable only once `frostmod_status` has reported a `foreign` or `locked` stray,
/// because that report is what put the file in front of them to agree to.
#[tauri::command]
async fn frostmod_clear_stray_msvcr90(app: tauri::AppHandle) -> Result<String, String> {
    let Some(dir) = game_dir_for_runtimes(&app) else {
        return Err("No game folder is set, so there's nowhere to look.".into());
    };
    vcruntime::disable_stray_msvcr90(&dir)
        .map(|p| p.display().to_string())
        .map_err(|e| format!("{e:#}"))
}

/// Where the active title is installed, or `None` when we don't know.
///
/// `install_dir` hands back an empty string for "unset", which must not become the path
/// `""` — every runtime path that touches the game folder needs the same guard.
fn game_dir_for_runtimes(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    config::load(app)
        .ok()
        .map(|c| c.install_dir())
        .filter(|d| !d.trim().is_empty())
        .map(std::path::PathBuf::from)
}

/// Microsoft's download page links for every runtime, so the UI can always offer the
/// manual route — the backstop for a declined UAC prompt or a PC that can't reach
/// `aka.ms`.
#[tauri::command]
fn runtime_downloads() -> Vec<(vcruntime::Runtime, &'static str, &'static str)> {
    vcruntime::Runtime::ALL
        .into_iter()
        .map(|r| (r, r.label(), r.url()))
        .collect()
}

#[tauri::command]
async fn frostmod_install_runtime(
    app: tauri::AppHandle,
    runtime: vcruntime::Runtime,
) -> Result<vcruntime::InstallOutcome, String> {
    // The game folder is part of the VC90 answer — a private assembly can sit there — so
    // the post-install re-check has to be asked the same question the banner was.
    let game_dir = game_dir_for_runtimes(&app);
    vcruntime::install(&app, runtime, game_dir.as_deref())
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn frostmod_start(app: tauri::AppHandle, state: State<FrostmodProcess>) -> Result<bool, String> {
    frostmod_manage::start(&app, &state).map_err(|e| format!("{e:#}"))
}

/// Stop FrostMod now, whoever started it — ours to kill or not. `false` means it's still
/// running (elevated, or another user's), which the UI reports rather than papering over.
///
/// Async for the same reason as `set_mods_path`: a sync command runs on the UI thread, and
/// this one waits out the moment between the kill and the process actually going.
#[tauri::command]
async fn frostmod_stop(state: State<'_, FrostmodProcess>) -> Result<bool, String> {
    Ok(frostmod_manage::stop_running(&state))
}

#[tauri::command]
fn set_auto_run_frostmod(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.auto_run_frostmod = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Extra flags for `frostmod.exe`, stored as typed. Not validated here: FrostMod ignores a
/// flag it doesn't know, so an unknown one costs nothing, while checking against a list this
/// app carries would reject flags a newer FrostMod does understand.
#[tauri::command]
fn set_frostmod_args(app: tauri::AppHandle, args: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.frostmod_args = args.trim().to_string();
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_instant_refresh(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.instant_refresh = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Show or hide the in-game overlay. Also reachable from its global hotkey.
#[tauri::command]
fn overlay_toggle(app: tauri::AppHandle) -> Result<(), String> {
    overlay::toggle(&app)
}

/// Dismiss the overlay (its close button and Esc) and hand focus back to the game.
#[tauri::command]
fn overlay_hide(app: tauri::AppHandle) -> Result<(), String> {
    overlay::hide(&app)
}

/// The overlay's "Open full app" button: put the overlay away and bring the main
/// window forward. Deliberately not `overlay::hide` — that hands focus back to MX
/// Bikes, which is the opposite of what someone leaving for the full app wants.
#[tauri::command]
fn overlay_open_main(app: tauri::AppHandle) -> Result<(), String> {
    overlay::dismiss(&app)?;
    show_main(&app);
    Ok(())
}

#[tauri::command]
fn overlay_state(app: tauri::AppHandle) -> overlay::OverlayState {
    overlay::state(&config::load(&app).unwrap_or_default())
}

#[tauri::command]
fn set_overlay_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.overlay_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Turning it off should also take the overlay off the screen, not just stop the
    // hotkey from re-summoning it.
    if !enabled {
        let _ = overlay::hide(&app);
    }
    overlay::register(&app, &cfg)
}

/// Every microphone and speaker the machine currently offers.
///
/// Not cached: the point is to notice the headset plugged in after the app launched.
#[tauri::command]
fn voice_devices() -> voice::Devices {
    voice::devices()
}

/// Turn voice chat on or off. Rebinds shortcuts, since push-to-talk only exists while it's on.
#[tauri::command]
fn set_voice_enabled(
    app: tauri::AppHandle,
    monitor: State<voice::Monitor>,
    session: State<voice::session::Session>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.voice_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Turning voice off must close the microphone, not just stop transmitting from it — and
    // it must do so now. The supervisor would notice within a few seconds, which is the
    // wrong answer to "I turned it off, is my mic still open?".
    if !enabled {
        monitor.stop();
        session.leave();
    }
    overlay::register(&app, &cfg)
}

/// Turn paint sync on or off.
///
/// Off stops both halves at once — nothing of this rider's look goes up, and nothing anyone
/// else published comes down. Paints already installed stay installed: they are files the
/// player now owns, and deleting a grid's worth of liveries because a switch moved would be
/// a worse surprise than leaving them.
#[tauri::command]
fn set_paint_sync_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.paint_sync_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Turning it back on shouldn't wait for the next thing that happens to change a look.
    if enabled {
        publish_paints_soon(&app, &cfg, None);
    }
    Ok(())
}

/// Take back the paints the sync installed.
///
/// Turning the sync off deliberately leaves them: they are files the player now has, and a
/// switch that wipes a grid's worth of liveries is a worse surprise than one that doesn't.
/// This is the other half of that — the way to actually get the folder back — and it only
/// touches what the manifest says we wrote and nobody has edited since.
#[tauri::command]
fn remove_synced_paints(app: tauri::AppHandle) -> Result<paintsync::RemoveOutcome, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let out = paintsync::remove_installed(&cfg);
    // A running game is holding paints that are no longer there; a rescan is what makes it
    // fall back to the default liveries rather than keep drawing them.
    if out.removed > 0 {
        let _ = frostmod::signal_reload();
    }
    Ok(out)
}

/// Pick the tyre pack the 3D previews fit. A blank name means "whatever the bike names".
#[tauri::command]
fn set_preview_tyres(app: tauri::AppHandle, tyres: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.preview_tyres = tyres;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Pick the microphone. A blank name means "follow the system default".
#[tauri::command]
fn set_voice_input_device(app: tauri::AppHandle, device: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.voice_input_device = device;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Pick where other riders come out. A blank name means "follow the system default".
#[tauri::command]
fn set_voice_output_device(app: tauri::AppHandle, device: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.voice_output_device = device;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Rebind push-to-talk. Registers before saving, so a combo another app owns leaves the
/// working one in place — same contract as the overlay hotkey.
#[tauri::command]
fn set_voice_ptt_hotkey(app: tauri::AppHandle, hotkey: String) -> Result<(), String> {
    let previous = config::load(&app).unwrap_or_default();
    let mut cfg = previous.clone();
    cfg.voice_ptt_hotkey = hotkey;
    if let Err(e) = overlay::register(&app, &cfg) {
        let _ = overlay::register(&app, &previous);
        return Err(e);
    }
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Switch between push-to-talk and toggle. Rebinds, since the two modes differ only in
/// which key edges the handler acts on.
#[tauri::command]
fn set_voice_toggle_to_talk(app: tauri::AppHandle, toggle: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.voice_toggle_to_talk = toggle;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    overlay::register(&app, &cfg)
}

/// Set mic gain and playback volume together — they're one slider pair in the UI, and
/// saving them separately would write the config file twice for one drag.
#[tauri::command]
fn set_voice_levels(
    app: tauri::AppHandle,
    session: State<voice::session::Session>,
    input_gain: f32,
    output_volume: f32,
) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.voice_input_gain = input_gain.clamp(0.0, 4.0);
    cfg.voice_output_volume = output_volume.clamp(0.0, 1.0);
    // Straight through to a running session as well as to disk: dragging the volume slider
    // while people are talking should change what you hear, not what you hear next time.
    session.send(voice::engine::Command::Volume(cfg.voice_output_volume));
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Hear riders from where they are on the track, or flat.
///
/// Takes effect on the next session rather than mid-race: the engine reads it when it joins,
/// and a rider is far more likely to be flipping this while parked in the menus than while
/// racing. The setting is what persists; the switch is not a live mixer control.
#[tauri::command]
fn set_voice_proximity(app: tauri::AppHandle, proximity: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.voice_proximity = proximity;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Who is in voice on this server right now, and who is talking.
///
/// The panel also gets this pushed as a `voice-status` event; this is for the first paint,
/// before anything has changed.
#[tauri::command]
fn voice_status(session: State<voice::session::Session>) -> voice::engine::Status {
    session.status()
}

/// Silence one rider, for as long as this session lasts.
#[tauri::command]
fn voice_mute(session: State<voice::session::Session>, peer_id: String, muted: bool) {
    session.send(voice::engine::Command::Mute { peer_id, muted });
}

/// Open the mic and start reporting its level as `voice-input-level`.
///
/// Returns a warning string when the saved device is gone and we fell back to the default
/// — the unplugged-headset case, which must be visible rather than silently mute.
#[tauri::command]
fn voice_meter_start(
    app: tauri::AppHandle,
    monitor: State<voice::Monitor>,
) -> Result<Option<String>, String> {
    let cfg = config::load(&app).unwrap_or_default();
    monitor.start(app.clone(), &cfg.voice_input_device, cfg.voice_input_gain)
}

/// Close the mic. Idempotent — the settings page calls it on unmount.
#[tauri::command]
fn voice_meter_stop(monitor: State<voice::Monitor>) {
    monitor.stop();
}

/// Play a short tone on the configured output, so the player can confirm which headset
/// voice will come out of before they're on a grid with twenty people.
#[tauri::command]
fn voice_test_output(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let cfg = config::load(&app).unwrap_or_default();
    voice::test_output(&cfg.voice_output_device, cfg.voice_output_volume)
}

/// Rebind the overlay hotkey. Validates and registers before saving, so a combo that
/// another app already owns leaves the working one in place.
#[tauri::command]
fn set_overlay_hotkey(app: tauri::AppHandle, hotkey: String) -> Result<(), String> {
    let previous = config::load(&app).unwrap_or_default();
    let mut cfg = previous.clone();
    cfg.overlay_hotkey = hotkey;
    if let Err(e) = overlay::register(&app, &cfg) {
        // Put the old binding back — a rejected combo must not leave the player with
        // no way to open the overlay.
        let _ = overlay::register(&app, &previous);
        return Err(e);
    }
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_watch_mods_reload(
    app: tauri::AppHandle,
    state: State<ModWatcher>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.watch_mods_reload = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    // Start/stop the watcher live so the toggle takes effect without a restart.
    if enabled {
        modwatch::start(&app, &state, &cfg.mods_path);
    } else {
        modwatch::stop(&state);
    }
    Ok(())
}

/// Turn injecting `mxbsecure.dll` into the running game on or off.
///
/// Takes effect on the next game session: the watcher decides once per run, so a change made
/// mid-session doesn't reach into a game that is already up.
#[tauri::command]
fn set_secure_content_inject(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.secure_content_inject = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn shop_login(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(SHOP_LOGIN_WINDOW) {
        let _ = w.set_focus();
        return Ok(());
    }

    // Lands on `/robots.txt`, not `/all-my-downloads/`: every HTML path here is behind a
    // managed challenge, so the old redirect walked straight into a second one the moment
    // credentials were accepted. Static files aren't gated (measured: 200, no `cf-mitigated`).
    let target = format!(
        "{base}/wp-login.php?redirect_to={base}%2Frobots.txt",
        base = shop_session::SHOP_BASE
    );
    // A sign-in starts from a clean browser. An expired or mismatched `cf_clearance` is worse
    // than none — it's presented, rejected, and the challenge re-served, which is the loop.
    // Coarse on purpose: Tauri has no per-origin cookie delete, so mxb-mods.com re-clears its
    // own check on next use.
    let stale = shop_session::cookies_from_window_any(&app);
    log::info!(
        "opening the shop login window at {target} (clearing first; jar held: {})",
        shop_session::cookie_names(&stale)
    );
    // The hidden purchases window goes first, for the same reason sign-*out* drops it: it is a
    // browser parked on a page belonging to the session about to be thrown away. Clearing the
    // cookies out from under a window that stays up leaves it displaying the old DOM, and that
    // DOM is what the read straight after this sign-in would return — the login form, read back
    // as "your session expired", which signs the user out a moment after signing them in.
    shop_fetch::close(&app);
    if let Some(main) = app.get_webview_window(MAIN_WINDOW) {
        if let Err(e) = main.clear_all_browsing_data() {
            log::warn!("could not clear stale cookies before sign-in: {e}");
        }
    }
    let url = tauri::WebviewUrl::External(target.parse().map_err(|e| format!("{e}"))?);
    // No `.user_agent()` override: it claimed `Chrome/126.0` on Windows while actually being
    // WKWebView on macOS. Left alone, the WebView introduces itself honestly.
    let window = tauri::WebviewWindowBuilder::new(&app, SHOP_LOGIN_WINDOW, url)
        .title("Sign in to MX Bikes Shop")
        .inner_size(520.0, 760.0)
        .build()
        .map_err(|e| format!("{e:#}"))?;
    let _ = window;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // ~5 minutes at 500ms intervals, then give up (user can retry).
        let mut last_seen = Vec::new();
        for _ in 0..600u32 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(win) = app.get_webview_window(SHOP_LOGIN_WINDOW) else {
                // Closed by hand. That is a cancel rather than a failure, so it gets a log
                // line and no error toast — but it does get a log line, because "the user gave
                // up" and "the app stopped watching" are otherwise indistinguishable later.
                log::info!(
                    "the shop login window was closed before sign-in finished (cookies: {})",
                    shop_session::cookie_names(&last_seen)
                );
                return;
            };
            let cookies = shop_session::cookies_from_window(&win);
            if !shop_session::is_authenticated(&cookies) {
                last_seen = cookies;
                continue;
            }
            let ok = match shop_session::set_session(&app, cookies) {
                Ok(()) => {
                    log::info!("captured MX Bikes Shop session");
                    true
                }
                Err(e) => {
                    log::error!("failed to save shop session: {e:#}");
                    false
                }
            };
            let _ = app.emit("shop-auth", ok);
            let _ = win.close();
            return;
        }

        // Five minutes on the page and never signed in.
        //
        // This used to end here in silence — no event, no log line — which is what makes a
        // sign-in that cannot get past Cloudflare's challenge look like an app that has simply
        // hung. The store fronts every path with a *managed* challenge, and an embedded WebView
        // is exactly the visitor it is least willing to clear, so this is a real outcome and
        // not a corner case. The cookie names say which it was: a `cf_clearance` means the
        // challenge cleared and the sign-in itself never completed; no clearance means the
        // window never got off the interstitial.
        log::warn!(
            "shop sign-in did not complete within 5 minutes (cookies: {})",
            shop_session::cookie_names(&last_seen)
        );
        let _ = app.emit("shop-auth", false);
        // Closed rather than left up: nothing is watching it any more, so a sign-in finished
        // afterwards would go unnoticed. Retry reopens it.
        if let Some(win) = app.get_webview_window(SHOP_LOGIN_WINDOW) {
            let _ = win.close();
        }
    });
    Ok(())
}

#[tauri::command]
fn shop_status(state: State<shop_session::ShopSession>) -> bool {
    state.logged_in()
}

#[tauri::command]
fn shop_logout(app: tauri::AppHandle) {
    shop_session::clear_session(&app);
}

#[tauri::command]
async fn shop_my_downloads(
    app: tauri::AppHandle,
    state: State<'_, shop_session::ShopSession>,
    reload: Option<bool>,
) -> Result<Vec<mods::mxbshop::ShopItem>, String> {
    // The captured cookies are what "signed in" means now; the `reqwest` client they also build
    // is no longer what reads this page, because Cloudflare will not let it.
    if !state.logged_in() {
        return Err("Not signed in to MX Bikes Shop.".to_string());
    }
    // The hidden window keeps the page it loaded, so Refresh has to say so or it re-reads the
    // same DOM. Absent (first load) means the window is being built and navigates anyway.
    mods::mxbshop::fetch_my_downloads(&app, reload.unwrap_or(false))
        .await
        .map_err(|e| format!("{e:#}"))
}

/// The catalog entry for each purchased product name, positionally, so the purchases grid can
/// show real artwork instead of a grey placeholder.
#[tauri::command]
async fn shop_match_catalog(
    app: tauri::AppHandle,
    names: Vec<String>,
) -> Result<Vec<Option<mods::shop_catalog::ShopMod>>, String> {
    mods::shop_catalog::match_products(&app, &names)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Download a purchased file and install it, to a destination the caller already chose.
///
/// The shop half of [`install::download_and_place`]: the bytes come through
/// [`shop_fetch::download`] because the store's file URLs sit behind Cloudflare's managed
/// challenge, and everything after that is the same extract-and-place every other install uses.
///
/// `subpath`/`dest_folder` arrive from the same dialog Browse uses, so nothing here guesses.
#[tauri::command]
async fn shop_install(
    app: tauri::AppHandle,
    state: State<'_, shop_session::ShopSession>,
    item: mods::mxbshop::ShopItem,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    if !state.logged_in() {
        return Err("Not signed in to MX Bikes Shop.".to_string());
    }
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    // Asked before the download: a track archive is several hundred megabytes to waste.
    if cfg.mods_path.trim().is_empty() {
        return Err("No MX Bikes folder is configured yet.".to_string());
    }

    let work = install::staging_dir("shop");
    std::fs::create_dir_all(&work).map_err(|e| format!("{e:#}"))?;

    let _cancel = cancel::begin(&item.slug);
    let archive = match shop_fetch::download(&app, &item.slug, &item.download_url, &work).await {
        Ok(path) => path,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            return Err(format!("{e:#}"));
        }
    };

    // What identifies this purchase on disk afterwards. Both forms, because a `.pkz` is placed
    // under its own file name while an archive that extracts lands in a folder named for its
    // stem. Deliberately *not* the chosen destination folder: that is shared by everything
    // filed there, so matching on it would badge every other mod in the same folder.
    let names: Vec<String> = [archive.file_name(), archive.file_stem()]
        .into_iter()
        .flatten()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

    let placed = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        let cfg = cfg.clone();
        let slug = item.slug.clone();
        let work = work.clone();
        move || install::extract_and_place(
            &app,
            &cfg,
            &slug,
            &archive,
            &work,
            &subpath,
            &dest_folder,
            install::Packs::PlaceWhole,
        )
        .map(|_| dest_folder)
    })
    .await
    .map_err(|e| format!("shop_install task failed: {e}"))?;

    let _ = std::fs::remove_dir_all(&work);
    let dest_folder = placed.map_err(|e| format!("{e:#}"))?;

    // One place records what a purchase installed, so the badge is written on every path.
    let _ = dest_folder;
    if let Ok(dir) = app.path().app_local_data_dir() {
        if let Err(e) = shop_installed::record(&dir, &item.product, &names) {
            log::warn!("could not record what {} installed: {e:#}", item.product);
        }
    }
    Ok(())
}

// ─────────────────────────────────── MXB Hub ───────────────────────────────────
//
// shop.mxb-hub.com — the community marketplace `mxbhub.com` redirects to. Two halves, like
// the shop: a public catalog anyone can browse, and the files this account owns.
//
// It is markedly simpler than the shop's equivalent because the store is not behind
// Cloudflare (measured 2026-08-30: `server: nginx`, no `cf-ray`, no interstitial on
// `/my-account/`). So there is no clearance to earn, no parked WebView reading pages out of
// the DOM, and no `with_clearance` wrapper: `reqwest` talks to it directly, browsing needs no
// credential at all, and the only window ever opened is the sign-in one.

/// Run a hub read, and if the store answers with its robot challenge, solve it and try again.
///
/// The sibling of [`with_clearance`] above, and the difference is where the browser comes in.
/// mxb-mods.com's fix is to move the *request* into a WebView and keep it there for the
/// session, because Cloudflare judges the client. SiteGround judges the request rate and hands
/// out a cookie once its script has run — so the browser is needed exactly once, and every
/// request after it is an ordinary one again.
async fn with_hub_clearance<T, F, Fut>(
    app: &tauri::AppHandle,
    what: &str,
    op: F,
) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let err = match op().await {
        Ok(value) => return Ok(value),
        Err(err) => err,
    };
    if err.downcast_ref::<mods::Blocked>().is_none() {
        log::warn!("{what} failed and a browser wouldn't help: {err:#}");
        return Err(format!("{err:#}"));
    }
    log::info!("{what} hit the MXB Hub robot challenge — answering it in a browser");
    if let Err(e) = hub_clearance::earn(app).await {
        return Err(format!("{e:#}"));
    }
    op().await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn hub_search(
    app: tauri::AppHandle,
    query: String,
    category_id: Option<u64>,
    page: u32,
    sort: mods::hub::HubSort,
    on_sale_only: bool,
) -> Result<mods::hub::HubPage, String> {
    with_hub_clearance(&app, "hub search", || {
        mods::hub::search(&query, category_id, page, sort, on_sale_only)
    })
    .await
}

#[tauri::command]
async fn hub_categories(
    app: tauri::AppHandle,
) -> Result<Vec<mods::hub::HubCategory>, String> {
    with_hub_clearance(&app, "hub categories", mods::hub::categories).await
}

#[tauri::command]
async fn hub_detail(
    app: tauri::AppHandle,
    id: u64,
) -> Result<mods::hub::HubModDetail, String> {
    with_hub_clearance(&app, "hub detail", || mods::hub::detail(id)).await
}

#[tauri::command]
fn hub_status(state: State<hub_session::HubSession>) -> bool {
    state.logged_in()
}

#[tauri::command]
async fn hub_logout(app: tauri::AppHandle) {
    hub_session::clear_session(&app).await;
}

/// Open the store's own sign-in page and wait for the login cookie to appear.
///
/// The password is typed into WooCommerce's page, in a window of its own — the app never sees
/// it, and never asks for it. What we take is the cookie the store sets afterwards.
///
/// Two things the shop's version has to do are deliberately absent. It clears the whole
/// WebView's cookies first, because a stale `cf_clearance` there is worse than none; there is
/// no Cloudflare here, and clearing is app-wide, so doing it would sign the user out of the
/// *other* store on their way into this one. And it lands on `/robots.txt` to dodge a second
/// challenge; this one can land on the account page, which is also the page that proves the
/// sign-in worked.
#[tauri::command]
async fn hub_login(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(HUB_LOGIN_WINDOW) {
        let _ = w.set_focus();
        return Ok(());
    }

    // The account page, plainly. It is both the login form when signed out and the proof of a
    // sign-in when not, so there is nothing to redirect to and no parameter worth inventing.
    let target = format!("{base}/my-account/", base = hub_session::HUB_BASE);
    let url = tauri::WebviewUrl::External(target.parse().map_err(|e| format!("{e}"))?);
    let window = tauri::WebviewWindowBuilder::new(&app, HUB_LOGIN_WINDOW, url)
        .title("Sign in to MXB Hub")
        .inner_size(520.0, 760.0)
        .build()
        .map_err(|e| format!("{e:#}"))?;
    let _ = window;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // ~5 minutes at 500 ms. Long enough for a password reset mid-flow; the user can retry.
        let mut last_seen = Vec::new();
        for _ in 0..600u32 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let Some(win) = app.get_webview_window(HUB_LOGIN_WINDOW) else {
                // Closed by hand — a cancel, not a failure. Logged all the same, because "the
                // user gave up" and "the app stopped watching" are indistinguishable later.
                log::info!(
                    "the MXB Hub login window was closed before sign-in finished (cookies: {})",
                    hub_session::cookie_names(&last_seen)
                );
                return;
            };
            let cookies = hub_session::cookies_from_window(&win);
            if !hub_session::is_authenticated(&cookies) {
                last_seen = cookies;
                continue;
            }
            let ok = match hub_session::set_session(&app, cookies) {
                Ok(()) => {
                    log::info!("captured MXB Hub session");
                    true
                }
                Err(e) => {
                    log::error!("failed to save the MXB Hub session: {e:#}");
                    false
                }
            };
            let _ = app.emit("hub-auth", ok);
            let _ = win.close();
            return;
        }

        log::warn!(
            "MXB Hub sign-in did not complete within 5 minutes (cookies: {})",
            hub_session::cookie_names(&last_seen)
        );
        let _ = app.emit("hub-auth", false);
        // Closed rather than left up: nothing is watching it any more, so a sign-in finished
        // afterwards would go unnoticed. Retry reopens it.
        if let Some(win) = app.get_webview_window(HUB_LOGIN_WINDOW) {
            let _ = win.close();
        }
    });
    Ok(())
}

/// What this account owns, with its catalog entries alongside.
///
/// One command rather than the shop's two. A Hub download row links to its product page, so
/// the catalog lookup is a single request keyed on exact slugs — where the shop has to fold
/// product *names* together and match approximately, which is why its match is a separate
/// call the grid makes after the fact. Doing both here means the grid renders once, complete,
/// instead of popping in twice.
///
/// The listings are positional: `listings[i]` is `items[i]`'s catalog entry, or `null` where
/// the product has since been unlisted. Best-effort — a catalog that won't answer costs the
/// cards their artwork, not the list.
#[tauri::command]
async fn hub_my_downloads(
    app: tauri::AppHandle,
    state: State<'_, hub_session::HubSession>,
) -> Result<HubDownloads, String> {
    if !state.logged_in() {
        return Err("Not signed in to MXB Hub.".to_string());
    }
    let mut items = with_hub_clearance(&app, "hub purchases", || {
        // Re-read the client each attempt: answering the challenge rebuilds the signed-in one
        // with the clearance folded in, and the stale handle would just be challenged again.
        let client = state.client();
        async {
            let client = client.ok_or_else(|| anyhow::anyhow!("Not signed in to MXB Hub."))?;
            mods::hubaccount::fetch_my_downloads(&app, &client).await
        }
    })
    .await?;

    let listings = match mods::hubaccount::match_products(&items).await {
        Ok(found) => found,
        Err(e) => {
            log::warn!("could not match MXB Hub purchases to the catalog: {e:#}");
            vec![None; items.len()]
        }
    };
    // Mirrored onto the rows themselves as well, because a purchase is handed to the install
    // queue on its own and has to still know what it is once the listing is out of scope.
    for (item, found) in items.iter_mut().zip(&listings) {
        let Some(found) = found else { continue };
        item.image = found.image.clone();
        item.author = found.author.clone();
        item.category_id = found.category_ids.first().copied().unwrap_or(0) as u32;
    }

    Ok(HubDownloads { items, listings })
}

/// The purchases page and the catalog, joined, in one answer.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HubDownloads {
    items: Vec<mods::hubaccount::HubItem>,
    /// Positional against `items`.
    listings: Vec<Option<mods::hub::HubMod>>,
}

/// Download a file this account owns and install it, to a destination the caller already chose.
///
/// The whole body after the client lookup is [`install`]'s — `download` streams with progress,
/// resume and cancellation, and `extract_and_place` does what every other install does. That
/// reuse is the point of the store having no Cloudflare in front of it: the shop needed a
/// WebView download path ([`shop_fetch::download`]) precisely because its file URLs are
/// challenged, and none of that is needed here.
#[tauri::command]
async fn hub_install(
    app: tauri::AppHandle,
    state: State<'_, hub_session::HubSession>,
    item: mods::hubaccount::HubItem,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    let Some(session) = state.client() else {
        return Err("Not signed in to MXB Hub.".to_string());
    };
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    // Asked before the download: a track archive is several hundred megabytes to waste.
    if cfg.mods_path.trim().is_empty() {
        return Err("No MX Bikes folder is configured yet.".to_string());
    }

    let work = install::staging_dir("hub");
    std::fs::create_dir_all(&work).map_err(|e| format!("{e:#}"))?;

    let _cancel = cancel::begin(&item.slug);

    // Where the bytes come from decides both the client and whether the link needs resolving.
    //
    // A store-issued `?download_file=` URL is authorised by the user's session, so it is
    // fetched with it and is already a file. A handed-off link — MediaFire and friends, which
    // WooCommerce allows for any product and MXB Hub uses for a number of its free mods — is
    // fetched with the ordinary download client instead: sending the user's store cookies to a
    // third party would be wrong whichever way it turned out. It also has to be resolved
    // first, because a MediaFire *folder* is a web page listing files, not a file.
    let fetch = async {
        if item.external {
            let client = install::build_download_client()?;
            install::emit_resolving(&app, &item.slug);
            let direct = install::resolve_direct_url(&client, &item.download_url, &item.host)
                .await?;
            install::download(&app, &client, &item.slug, &direct, &work).await
        } else {
            // A dead cookie doesn't 401 here — WooCommerce answers a link it won't honour with
            // an HTML page, which `install::download` reports as "a web page instead of a
            // file". Left as it is rather than translated: guessing "session expired" from a
            // content type would sign the user out on a server hiccup.
            install::download(&app, &session, &item.slug, &item.download_url, &work).await
        }
    };
    let archive = match fetch.await {
        Ok(path) => path,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            return Err(format!("{e:#}"));
        }
    };

    // What identifies this purchase on disk afterwards. Both forms, because a `.pkz` is placed
    // under its own file name while an archive that extracts lands in a folder named for its
    // stem.
    let names: Vec<String> = [archive.file_name(), archive.file_stem()]
        .into_iter()
        .flatten()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();

    let placed = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        let cfg = cfg.clone();
        let slug = item.slug.clone();
        let work = work.clone();
        let subpath = subpath.clone();
        let dest_folder = dest_folder.clone();
        move || {
            install::extract_and_place(
                &app,
                &cfg,
                &slug,
                &archive,
                &work,
                &subpath,
                &dest_folder,
                install::Packs::PlaceWhole,
            )
            .map(|_| ())
        }
    })
    .await
    .map_err(|e| format!("hub_install task failed: {e}"))?;

    let _ = std::fs::remove_dir_all(&work);
    placed.map_err(|e| format!("{e:#}"))?;

    // Same record the shop's installs write, so both stores' grids get their badge from one
    // place — it is a claim about a product name and the folders it put on disk, and nothing
    // in it is specific to which store sold the thing.
    if let Ok(dir) = app.path().app_local_data_dir() {
        if let Err(e) = shop_installed::record(&dir, &item.product, &names) {
            log::warn!("could not record what {} installed: {e:#}", item.product);
        }
    }
    Ok(())
}

/// Which purchased products have a recorded install, and the folders they claim.
///
/// The claim is not checked against disk here — the purchases grid already scans the library
/// for its badges, so it does the intersecting and this stays a cheap read.
#[tauri::command]
fn shop_installed_map(
    app: tauri::AppHandle,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    Ok(shop_installed::recorded(&dir))
}

/// Note a finished download — installed or failed. Called from the two places every install
/// passes through (`Context/Install` and `Context/DropReview`), which is why nothing in the
/// download paths themselves has to know history exists.
#[tauri::command]
fn record_download(
    app: tauri::AppHandle,
    entry: downloads::NewDownload,
) -> Result<Option<downloads::DownloadRecord>, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    usage::track("mod.download");
    downloads::record(&dir, entry).map_err(|e| format!("{e:#}"))
}

/// Everything downloaded, newest first.
#[tauri::command]
fn download_history(app: tauri::AppHandle) -> Result<Vec<downloads::DownloadRecord>, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    Ok(downloads::history(&dir))
}

#[tauri::command]
fn forget_download(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    downloads::forget(&dir, &id).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn clear_download_history(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    downloads::clear(&dir).map_err(|e| format!("{e:#}"))
}

// ===========================================================================
// Library ledger — what the mods tree used to hold
// ===========================================================================

/// What the ledger counts as a mod: everything Manage governs, plus bike liveries.
///
/// Manage leaves liveries out because it has no reason to move them. The ledger has every
/// reason to remember them — a livery you deleted is exactly as hard to name months later as
/// a track you deleted.
fn ledger_candidate(e: &library::LibraryEntry) -> bool {
    modstate::is_candidate(e) || e.category == "bikePaint"
}

/// Fold the current state of the mods tree into the ledger.
///
/// Blocking: it walks the content folders and the shadow tree. Every caller runs it off the
/// UI thread.
fn ledger_reconcile_blocking(app: &tauri::AppHandle) {
    let Ok(dir) = app.path().app_local_data_dir() else {
        return;
    };
    let Ok(cfg) = config::load(app) else {
        return;
    };
    if cfg.mods_path.trim().is_empty() {
        return;
    }
    let game = cfg.active_game.id().to_string();

    let scanned = modstate::scan_with(&cfg, &sound_bikes_of(app), ledger_candidate);
    // Whether the tree was there to be read at all. Without this an unplugged drive and an
    // emptied library are the same observation, and only one of them means anything.
    let tree_ok = library::mods_root(&cfg.mods_path).is_dir();

    let mut store = ledger::load(&dir, &game);
    ledger::reconcile_store(&mut store, &cfg.mods_path, &scanned, tree_ok, ledger::now_ms());
    let pruned = ledger::prune(&dir, &game, &mut store, ledger::now_ms());
    if pruned > 0 {
        log::info!("ledger: pruned {pruned} row(s) gone longer than the keep window");
    }
    if let Err(e) = ledger::save(&dir, &game, &store) {
        log::warn!("ledger: could not save: {e:#}");
    }
}

/// Reconcile without making the caller wait. Used by the triggers that fire during normal
/// use, where the ledger being a moment behind costs nothing.
pub fn ledger_reconcile_detached(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || ledger_reconcile_blocking(&app));
}

/// How many archives one capture pass may open to backfill a snapshot.
///
/// Most snapshots cost nothing — the Library warms the metadata cache on every load, so the
/// data is already on disk. This covers the rest: a library that predates the ledger has no
/// cached metadata for mods the player never scrolled past, and without a bounded inflating
/// pass those rows would record a name and no picture forever. Small, so a capture never
/// becomes something the user waits on; repeated, so it finishes across a few visits.
const LEDGER_BACKFILL_PER_PASS: usize = 12;

/// Take the snapshot — title, author, location, length, thumbnail — for installed mods whose
/// row hasn't got one yet.
///
/// This is the only chance: once the files are gone, so is any way to learn what they were.
#[tauri::command]
async fn ledger_capture(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(dir) = app.path().app_local_data_dir() else {
            return;
        };
        let Ok(cfg) = config::load(&app) else {
            return;
        };
        let game = cfg.active_game.id().to_string();
        let mut store = ledger::load(&dir, &game);

        // Only mods still on disk can be snapshotted, and only `.pkz` carries metadata to
        // read — an extracted track folder or a loose `.pnt` has none.
        let todo: Vec<String> = store
            .entries
            .values()
            .filter(|e| e.state == ledger::PRESENT && e.needs_snapshot() && !e.is_dir)
            .filter(|e| e.name.to_ascii_lowercase().ends_with(".pkz"))
            .map(|e| e.key.clone())
            .collect();
        if todo.is_empty() {
            return;
        }

        let mut inflated = 0usize;
        let mut captured = 0usize;
        let mut skipped = 0usize;
        for key in todo {
            let Some(rel) = store.entries.get(&key).map(|e| e.rel.clone()) else {
                continue;
            };
            let path = library::mods_subdir(&cfg.mods_path, &rel);
            if !path.is_file() {
                continue;
            }
            // A mod whose bytes are off in iCloud or OneDrive reads as an empty archive, and
            // recording *that* as the snapshot would be worse than having none: the row would
            // be marked done and never looked at again, so a mod that is merely offloaded
            // today would lose its name and picture permanently. Leave it for a pass when the
            // file is actually here. Attributes only — this never triggers a download.
            if cloudfiles::is_placeholder(&path) {
                skipped += 1;
                continue;
            }
            let path = path.to_string_lossy().into_owned();

            // Free first: whatever the Library already warmed costs a single file read.
            let meta = match pkz::read_meta_if_cached(&app, &path) {
                Some(m) => Some(m),
                None if inflated < LEDGER_BACKFILL_PER_PASS => {
                    inflated += 1;
                    pkz::read_meta_cached(&app, &path).ok()
                }
                None => None,
            };
            let (Some(meta), Some(entry)) = (meta, store.entries.get_mut(&key)) else {
                continue;
            };
            ledger::apply_snapshot(&dir, &game, entry, &meta, ledger::now_ms());
            captured += 1;
        }

        if skipped > 0 {
            log::info!("ledger: {skipped} mod(s) left for later — offloaded to the cloud");
        }
        if captured > 0 {
            log::info!("ledger: captured {captured} snapshot(s), {inflated} by opening the archive");
            if let Err(e) = ledger::save(&dir, &game, &store) {
                log::warn!("ledger: could not save snapshots: {e:#}");
            }
        }
    })
    .await
    .map_err(|e| format!("ledger_capture task failed: {e}"))
}

/// Mods under `subpath` that the tree no longer holds — deleted, or parked by Manage.
///
/// Only the missing ones: what is installed is already in the caller's scan, and inflating a
/// thumbnail for a mod the Library can see for itself is work for nothing.
#[tauri::command]
async fn library_ledger(app: tauri::AppHandle, subpath: String) -> Result<Vec<ledger::LedgerRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let game = cfg.active_game.id().to_string();
        let prefix = format!("{}/", subpath.trim_end_matches('/').to_lowercase());

        let store = ledger::load(&dir, &game);
        let missing = store
            .entries
            .values()
            .filter(|e| e.state != ledger::PRESENT && e.key.starts_with(&prefix))
            .cloned();
        Ok(ledger::rows(&dir, &game, missing))
    })
    .await
    .map_err(|e| format!("library_ledger task failed: {e}"))?
}

/// Record where the Trash put a mod the app just uninstalled.
///
/// Best-effort throughout: losing the Restore option is a smaller harm than failing an
/// uninstall that has already happened.
fn ledger_note_trashed(
    app: &tauri::AppHandle,
    cfg: &config::AppConfig,
    from_path: &str,
    landed: library::TrashedAt,
) {
    let Ok(dir) = app.path().app_local_data_dir() else {
        return;
    };
    let root = library::mods_root(&cfg.mods_path);
    let Ok(rel) = std::path::Path::new(from_path).strip_prefix(&root) else {
        return;
    };
    let rel = format!("mods/{}", rel.to_string_lossy().replace('\\', "/"));
    ledger::note_trashed(&dir, cfg.active_game.id(), &rel, landed);
}

/// Put a mod the app deleted back where it came from.
#[tauri::command]
async fn restore_ledger_entry(app: tauri::AppHandle, key: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let game = cfg.active_game.id().to_string();

        let store = ledger::load(&dir, &game);
        let entry = store
            .entries
            .get(&key.to_lowercase())
            .ok_or_else(|| "no such entry".to_string())?;
        let original = library::mods_subdir(&cfg.mods_path, &entry.rel);

        library::restore_from_trash(&original, entry.trashed_at.as_deref())
            .map_err(|e| format!("{e:#}"))?;

        // Straight back to the truth rather than patching the row by hand: the mod is on disk
        // again, and a scan is what says so.
        ledger_reconcile_blocking(&app);
        Ok(())
    })
    .await
    .map_err(|e| format!("restore_ledger_entry task failed: {e}"))?
}

#[tauri::command]
fn forget_ledger_entry(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    ledger::forget(&dir, cfg.active_game.id(), &key).map_err(|e| format!("{e:#}"))
}

/// Forget everything no longer installed. What is still on disk stays — the next pass would
/// only write it straight back.
#[tauri::command]
fn clear_ledger(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    ledger::clear_gone(&dir, cfg.active_game.id()).map_err(|e| format!("{e:#}"))
}

fn presets_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("{e:#}"))
}

/// The player's profiles, plus the folder they were read from and whether it exists —
/// so an empty Presets tab can say *which* folder came up empty instead of leaving the
/// player to guess that a path is involved at all.
#[tauri::command]
fn presets_list_profiles(app: tauri::AppHandle) -> Result<presets::ProfilesScan, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    Ok(presets::scan_profiles(&cfg.profiles_dir()))
}

#[tauri::command]
fn presets_list_bikes(app: tauri::AppHandle, profile: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    presets::list_bikes(&cfg.profiles_dir(), &profile).map_err(|e| format!("{e:#}"))
}

/// Drop a bike from a profile — its saved loadout in every section, and the active-bike
/// pointer if it was the one.
///
/// The bike picker is a view of `profile.ini`, not of the mods folder, so a bike whose mod
/// was deleted long ago still sits in the list with nothing in the Library to uninstall.
#[tauri::command]
fn presets_forget_bike(
    app: tauri::AppHandle,
    profile: String,
    bikeid: String,
) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let dir = cfg.profiles_dir();
    presets::forget_bike(&dir, &profile, &bikeid).map_err(|e| format!("{e:#}"))?;
    presets::list_bikes(&dir, &profile).map_err(|e| format!("{e:#}"))
}

/// Which cosmetic slots this profile actually has, in `profile.ini` order.
///
/// The two games don't offer the same ones — GP Bikes has no goggles, boots or
/// protection — so the editor asks rather than rendering a fixed MX Bikes list with rows
/// that would do nothing.
#[tauri::command]
fn presets_slots(app: tauri::AppHandle, profile: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    presets::slots_for(&cfg.profiles_dir(), &profile).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_read_loadout(
    app: tauri::AppHandle,
    profile: String,
    bikeid: String,
) -> Result<presets::Loadout, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let mut loadout =
        presets::read_loadout(&cfg.profiles_dir(), &profile, &bikeid).map_err(|e| format!("{e:#}"))?;
    let active = modelswap::current_active(&cfg.mods_path, &bikeid);
    if !active.eq_ignore_ascii_case(modelswap::ORIGINAL_LABEL) {
        loadout.model_swap = active;
    }
    Ok(loadout)
}

#[derive(serde::Serialize)]
struct PresetApplyOutcome {
    content_reload: ReloadOutcome,
    game_running: bool,
    live_refresh: gameproc::LiveRefresh,
    /// Set only when the preset actually performed a model swap — see the note on
    /// `SwapApplyOutcome::model_refresh`.
    model_refresh: Option<frostmod::CommandOutcome>,
}

#[tauri::command]
fn presets_apply(
    app: tauri::AppHandle,
    profile: String,
    bikeid: String,
    loadout: presets::Loadout,
    make_active: bool,
) -> Result<PresetApplyOutcome, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    apply_loadout_now(&app, &cfg, &profile, &bikeid, &loadout, make_active)
}

/// Write a loadout into `profile.ini`, perform its model swap if it asks for one, and tell
/// a running game to pick it all up.
///
/// Shared by `presets_apply` and the Manage tab's race apply, which does this *and* takes
/// every mod the preset doesn't need out of the game's way — one action, not two.
fn apply_loadout_now(
    app: &tauri::AppHandle,
    cfg: &AppConfig,
    profile: &str,
    bikeid: &str,
    loadout: &presets::Loadout,
    make_active: bool,
) -> Result<PresetApplyOutcome, String> {
    presets::apply_loadout(&cfg.profiles_dir(), profile, bikeid, loadout, make_active)
        .map_err(|e| format!("{e:#}"))?;
    let want = loadout.model_swap.trim();
    let mut model_refresh = None;
    if !want.is_empty() && !want.eq_ignore_ascii_case(&modelswap::current_active(&cfg.mods_path, bikeid))
    {
        modelswap::apply_model_swap(&cfg.mods_path, bikeid, want)
            .map_err(|e| format!("Cosmetics applied, but the model swap failed: {e:#}"))?;
        // Same reason as the Locker path: the look loader won't reload the mesh.
        model_refresh = model_refresh_cmd(app, cfg.instant_refresh, bikeid);
    }
    let content_reload = frostmod::signal_reload();
    usage::track("preset.apply");
    // The look on disk just changed, so what the control plane holds for this rider is now
    // stale. Queued rather than awaited — this function is the synchronous apply path.
    publish_paints_soon(app, cfg, Some(profile));
    Ok(PresetApplyOutcome {
        content_reload,
        game_running: gameproc::is_game_running(),
        live_refresh: live_refresh(cfg.instant_refresh),
        model_refresh,
    })
}

#[tauri::command]
fn presets_list(app: tauri::AppHandle) -> Result<Vec<presets::Preset>, String> {
    Ok(presets::load_presets(&presets_dir(&app)?))
}

#[tauri::command]
fn presets_save(app: tauri::AppHandle, preset: presets::Preset) -> Result<(), String> {
    presets::save_preset(&presets_dir(&app)?, preset).map_err(|e| format!("{e:#}"))?;
    usage::track("preset.save");
    Ok(())
}

#[tauri::command]
fn presets_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    presets::delete_preset(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_export(app: tauri::AppHandle, name: String) -> Result<String, String> {
    presets::export_code(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_decode(text: String) -> Result<presets::Preset, String> {
    presets::decode_code(&text).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn presets_import(app: tauri::AppHandle, text: String) -> Result<presets::Preset, String> {
    presets::import_code(&presets_dir(&app)?, &text).map_err(|e| format!("{e:#}"))
}

// ---------------------------------------------------------------------------
// Feel presets — the settings half of a profile.
// ---------------------------------------------------------------------------

#[tauri::command]
fn feel_list(app: tauri::AppHandle) -> Result<Vec<feel::Feel>, String> {
    Ok(feel::load_feels(&presets_dir(&app)?))
}

/// Read what the profile is set to right now, ready to be named and saved.
#[tauri::command]
fn feel_capture(app: tauri::AppHandle, profile: String) -> Result<feel::Feel, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    feel::capture(&cfg.profiles_dir(), &profile).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn feel_save(app: tauri::AppHandle, feel: feel::Feel) -> Result<(), String> {
    feel::save_feel(&presets_dir(&app)?, feel).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn feel_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    feel::delete_feel(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

/// Write a saved feel back into a profile.
///
/// Refused while the game is up. MX Bikes reads both files once at startup and writes them
/// back when the Options screen closes, so an apply mid-session is invisible until the game
/// overwrites it — the rider would see nothing change and then lose the preset.
#[tauri::command]
fn feel_apply(
    app: tauri::AppHandle,
    profile: String,
    name: String,
) -> Result<feel::ApplyReport, String> {
    if gameproc::is_game_running() {
        return Err("Close MX Bikes first — it rewrites your settings when it exits.".into());
    }
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let saved = feel::find_feel(&presets_dir(&app)?, &name)
        .ok_or_else(|| format!("no feel preset named '{name}'"))?;
    if saved.is_empty() {
        return Err(format!("'{name}' has no settings saved in it."));
    }
    feel::apply(&cfg.profiles_dir(), &profile, &saved).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn feel_export(app: tauri::AppHandle, name: String) -> Result<String, String> {
    feel::export_code(&presets_dir(&app)?, &name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn feel_decode(text: String) -> Result<feel::Feel, String> {
    feel::decode_code(&text).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn feel_import(app: tauri::AppHandle, text: String) -> Result<feel::Feel, String> {
    feel::import_code(&presets_dir(&app)?, &text).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn preset_bundle_stats(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<bundle::BundlePlan, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    bundle::plan(&cfg, &loadout).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn preset_bundle_create(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let dir = presets_dir(&app)?;
    bundle::create(&app, &cfg, &dir, &name)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn preset_bundle_import(
    app: tauri::AppHandle,
    text: String,
) -> Result<presets::Preset, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let dir = presets_dir(&app)?;
    bundle::import(&app, &cfg, &dir, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// What sharing these picked files would carry, and what it would leave out. Nothing is
/// packed or uploaded — this is the dialog's preview.
#[tauri::command]
async fn file_share_plan(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<fileshare::SharePlan, String> {
    // Off the UI thread: sizing a picked folder walks it, and a track folder is thousands
    // of files.
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(fileshare::plan(&cfg, &paths))
    })
    .await
    .map_err(|e| format!("file_share_plan task failed: {e}"))?
}

/// Pack the picked files, upload them, and hand back the `MXBS1-` code.
#[tauri::command]
async fn file_share_create(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<String, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    fileshare::create(&app, &cfg, &paths)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Read a share code without downloading anything — the import dialog's preview. Says
/// which of its files the importer already has, since an import overwrites them.
#[tauri::command]
fn file_share_preview(
    app: tauri::AppHandle,
    text: String,
) -> Result<fileshare::SharePreview, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    fileshare::preview(&cfg, &text).map_err(|e| format!("{e:#}"))
}

/// Download a share code's files and install them where they came from.
#[tauri::command]
async fn file_share_import(
    app: tauri::AppHandle,
    text: String,
) -> Result<fileshare::FileShare, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    fileshare::import(&app, &cfg, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Publish picked files under a live code, or push a new version to one already published.
///
/// `code` names an existing share to update and `None` mints a new one. Unauthenticated all
/// the way down — there is no account, no enrollment and nothing to sign into; the update
/// key that comes back from a first publish is written into the config and never shown.
#[tauri::command]
async fn live_share_publish(
    app: tauri::AppHandle,
    paths: Vec<String>,
    name: Option<String>,
    code: Option<String>,
) -> Result<liveshare::LiveShareInfo, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::publish(
        &app,
        &cfg,
        &paths,
        name.as_deref().unwrap_or_default(),
        code.as_deref(),
    )
    .await
    .map_err(|e| format!("{e:#}"))
}

/// Read a live code without installing anything — the import dialog's preview.
#[tauri::command]
async fn live_share_preview(
    app: tauri::AppHandle,
    text: String,
) -> Result<fileshare::SharePreview, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::preview(&cfg, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Follow a live code and install what it points at now.
#[tauri::command]
async fn live_share_subscribe(
    app: tauri::AppHandle,
    text: String,
) -> Result<fileshare::FileShare, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::subscribe(&app, &cfg, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Install the version a followed code points at now.
#[tauri::command]
async fn live_share_sync(
    app: tauri::AppHandle,
    text: String,
) -> Result<fileshare::FileShare, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::sync(&app, &cfg, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Ask the control plane what version every followed code is on, and hand back the list.
#[tauri::command]
async fn live_share_check(
    app: tauri::AppHandle,
) -> Result<Vec<liveshare::LiveShareInfo>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::check(&app, &cfg).await.map_err(|e| format!("{e:#}"))
}

/// Every live code this machine publishes or follows. Local only — no request is made.
#[tauri::command]
fn live_share_list(app: tauri::AppHandle) -> Result<Vec<liveshare::LiveShareInfo>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::list(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Stop following a code. The files it installed stay where they are.
#[tauri::command]
fn live_share_forget(app: tauri::AppHandle, text: String) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::forget(&app, &cfg, &text).map_err(|e| format!("{e:#}"))
}

/// Install new versions of a followed code as soon as they appear, or stop doing that.
#[tauri::command]
fn live_share_set_auto(app: tauri::AppHandle, text: String, auto: bool) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::set_auto(&app, &cfg, &text, auto).map_err(|e| format!("{e:#}"))
}

/// The code plus its update key, for moving a published share to another machine.
///
/// Deliberately its own command rather than a field on `live_share_list`: this string lets
/// whoever holds it replace the track for everyone following the code, so it is fetched
/// when the player asks for it and never rendered beside the code they hand out.
#[tauri::command]
fn live_share_owner_code(app: tauri::AppHandle, code: String) -> Result<String, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let want = liveshare::normalise(&code).ok_or("that isn't a live share code")?;
    cfg.published_shares
        .iter()
        .find(|s| liveshare::normalise(&s.code).as_deref() == Some(want.as_str()))
        .map(liveshare::owner_code)
        .ok_or_else(|| "this machine didn't publish that code".to_string())
}

/// Take over a share published on another machine, from its owner code.
#[tauri::command]
async fn live_share_adopt(
    app: tauri::AppHandle,
    text: String,
) -> Result<liveshare::LiveShareInfo, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    liveshare::adopt(&app, &cfg, &text)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Every mod Manage can act on, enabled and disabled alike.
#[tauri::command]
async fn mods_state_scan(app: tauri::AppHandle) -> Result<Vec<modstate::ModEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let sound_bikes = sound_bikes_of(&app);
        Ok(modstate::scan(&cfg, &sound_bikes))
    })
    .await
    .map_err(|e| format!("mods_state_scan task failed: {e}"))?
}

/// Sound-swap bookkeeping the bike scan needs to tell a sound folder from a bike folder.
fn sound_bikes_of(app: &tauri::AppHandle) -> Vec<String> {
    app.path()
        .app_local_data_dir()
        .map(|d| soundmods::known_bikes(&d))
        .unwrap_or_default()
}

fn preset_by_name(app: &tauri::AppHandle, name: &str) -> Result<presets::Preset, String> {
    presets::find_preset(&presets_dir(app)?, name)
        .ok_or_else(|| format!("no preset named '{name}'"))
}

/// What racing this preset would enable and disable — the numbers the confirm dialog shows,
/// worked out before anything moves.
#[tauri::command]
async fn mods_state_plan(
    app: tauri::AppHandle,
    name: String,
) -> Result<modstate::StatePlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let preset = preset_by_name(&app, &name)?;
        Ok(modstate::plan(&cfg, &preset, &sound_bikes_of(&app)))
    })
    .await
    .map_err(|e| format!("mods_state_plan task failed: {e}"))?
}

/// Outcome of a Manage operation: what moved, and what the game was told about it.
#[derive(serde::Serialize)]
struct ModsStateOutcome {
    #[serde(flatten)]
    state: modstate::StateOutcome,
    content_reload: ReloadOutcome,
    game_running: bool,
    /// Present only on a race apply, when a preset's cosmetics went in alongside the
    /// content shuffle.
    look: Option<PresetApplyOutcome>,
}

/// Run a bulk file shuffle with the mods watcher parked.
///
/// Moving hundreds of archives is hundreds of filesystem events, and the watcher would
/// answer them with its own reload on top of the one we send deliberately. Stop it, move,
/// start it again — the folder it watches hasn't changed, only its contents.
fn with_watcher_parked<T>(
    app: &tauri::AppHandle,
    cfg: &AppConfig,
    op: impl FnOnce() -> T,
) -> T {
    let watcher = app.state::<ModWatcher>();
    modwatch::stop(&watcher);
    let out = op();
    if cfg.watch_mods_reload {
        modwatch::start(app, &watcher, &cfg.mods_path);
    }
    out
}

fn finish_state_op(
    state: modstate::StateOutcome,
    look: Option<PresetApplyOutcome>,
) -> ModsStateOutcome {
    // One deliberate reload for the whole batch, the same signal a preset apply sends.
    let content_reload = if state.touched() > 0 {
        frostmod::signal_reload()
    } else {
        ReloadOutcome::NotRunning
    };
    ModsStateOutcome {
        state,
        content_reload,
        game_running: gameproc::is_game_running(),
        look,
    }
}

/// Enable or disable a hand-picked set of mods.
#[tauri::command]
async fn mods_state_set(
    app: tauri::AppHandle,
    rels: Vec<String>,
    enabled: bool,
) -> Result<ModsStateOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let out = with_watcher_parked(&app, &cfg, || modstate::set_many(&cfg, &rels, enabled));
        Ok(finish_state_op(out, None))
    })
    .await
    .map_err(|e| format!("mods_state_set task failed: {e}"))?
}

/// Race mode: put on the preset's look and leave the game with only the content it needs.
///
/// `profile`/`bikeid` are optional — blank means "just do the content", for a preset used
/// purely as a content list.
#[tauri::command]
async fn mods_state_apply(
    app: tauri::AppHandle,
    name: String,
    profile: String,
    bikeid: String,
) -> Result<ModsStateOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let preset = preset_by_name(&app, &name)?;

        // Cosmetics first: they're the part that can fail loudly (a missing profile), and
        // failing before anything has moved leaves the library untouched.
        let look = if profile.trim().is_empty() || bikeid.trim().is_empty() {
            None
        } else {
            Some(apply_loadout_now(
                &app,
                &cfg,
                profile.trim(),
                bikeid.trim(),
                &preset.loadout,
                true,
            )?)
        };

        let plan = modstate::plan(&cfg, &preset, &sound_bikes_of(&app));
        let out = with_watcher_parked(&app, &cfg, || modstate::apply(&cfg, &plan));
        Ok(finish_state_op(out, look))
    })
    .await
    .map_err(|e| format!("mods_state_apply task failed: {e}"))?
}

/// Send mods to the recycle bin, enabled or parked.
#[tauri::command]
async fn mods_state_delete(
    app: tauri::AppHandle,
    rels: Vec<String>,
) -> Result<ModsStateOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let out = with_watcher_parked(&app, &cfg, || modstate::delete_many(&cfg, &rels));
        Ok(finish_state_op(out, None))
    })
    .await
    .map_err(|e| format!("mods_state_delete task failed: {e}"))?
}

/// Put everything back the way it was.
#[tauri::command]
async fn mods_state_restore_all(app: tauri::AppHandle) -> Result<ModsStateOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let out = with_watcher_parked(&app, &cfg, || modstate::restore_all(&cfg));
        Ok(finish_state_op(out, None))
    })
    .await
    .map_err(|e| format!("mods_state_restore_all task failed: {e}"))?
}

/// Normally `Info`. `MXB_LOG=debug` turns on the per-request traces that would otherwise
/// write a line per keystroke in the search box — the switch to flip when chasing a
/// Cloudflare block on a machine that can reproduce one.
fn log_level() -> log::LevelFilter {
    match std::env::var("MXB_LOG").unwrap_or_default().to_lowercase().as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        _ => log::LevelFilter::Info,
    }
}

/// The values of `GDK_BACKEND` this code has to tell apart. Anything else — `broadway`, or a
/// comma-separated list of fallbacks — is somebody being deliberate, and is left alone.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
enum Backend {
    #[default]
    Unset,
    X11,
    Wayland,
    Other,
}

/// What the session looks like, read once so the choice below is a pure function of it —
/// the only way to test any of this from a machine that isn't the one it's for.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct GraphicsEnv {
    /// The compositor's socket — set on any Wayland session, gamescope included.
    wayland: bool,
    /// An X server is reachable, real or XWayland.
    x_server: bool,
    /// What `GDK_BACKEND` already says, whether the session exported it or a player did.
    /// The EGL correction below keys off the backend GTK will *actually* use, which is this
    /// when it's set and our own choice when it isn't.
    gdk_backend: Backend,
    /// `EGL_PLATFORM=wayland`, which SteamOS exports and everything it launches inherits.
    egl_wayland: bool,
    /// `MXB_SAFE_GRAPHICS=1`, or a previous run that died before it painted anything.
    safe_mode: bool,
}

impl GraphicsEnv {
    fn read(safe_mode: bool) -> Self {
        let set = |key: &str| std::env::var_os(key).is_some_and(|v| !v.is_empty());
        let var = |key: &str| std::env::var(key).unwrap_or_default().trim().to_lowercase();
        Self {
            wayland: set("WAYLAND_DISPLAY"),
            x_server: set("DISPLAY"),
            gdk_backend: match var("GDK_BACKEND").as_str() {
                "" => Backend::Unset,
                "x11" => Backend::X11,
                "wayland" => Backend::Wayland,
                _ => Backend::Other,
            },
            egl_wayland: var("EGL_PLATFORM") == "wayland",
            safe_mode: safe_mode || std::env::var("MXB_SAFE_GRAPHICS").unwrap_or_default() == "1",
        }
    }
}

/// Whether a variable may overwrite one that is already set.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Force {
    /// A default. An explicit choice already in the environment wins.
    IfUnset,
    /// A correction. Set even over an existing value, because what is there cannot work.
    Always,
}

/// The environment WebKitGTK should start under. Several separate faults all end as a white
/// window or an abort, and each has its own knob here.
fn webview_env_defaults(env: GraphicsEnv) -> Vec<(&'static str, &'static str, Force)> {
    // DMA-BUF asks the WebKit our AppImage carries from Ubuntu 22.04 to negotiate buffers
    // with whatever Mesa the host ships; where that fails it fails silently, painting
    // nothing. The shared-memory fallback costs a copy per frame — imperceptible on a UI
    // of mostly static lists — and paints everywhere.
    let mut vars = vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1", Force::IfUnset)];

    // The other fault — WebKitGTK 2.46+ aborting because it can't create an EGL display —
    // was the bundled libwayland, and it is fixed where it was made: the AppImage no longer
    // carries those libraries at all (scripts/appimage-drop-bundled-wayland.sh). An AppImage
    // also exports `GDK_BACKEND=x11` from its own AppRun hook, before this process starts,
    // so on the machine this was written for the backend is already spoken for by the time
    // we look — which is why the correction below keys off the backend GTK will actually
    // use rather than off our own choice.
    //
    // What's left here is the hand-operated fallback: an X server is a second graphics stack
    // to land on when a machine still won't paint. Guarded on there being one — forcing the
    // backend without it trades a white screen for no window at all — and on nothing having
    // chosen already, so an explicit `GDK_BACKEND=wayland` still wins.
    let choose_x11 =
        env.gdk_backend == Backend::Unset && env.safe_mode && env.wayland && env.x_server;
    if choose_x11 {
        vars.push(("GDK_BACKEND", "x11", Force::IfUnset));
    }

    // Putting GTK on X11 was supposed to be the end of that abort, and on a Steam Deck it
    // wasn't: `GDK_BACKEND=x11` still died on `EGL_BAD_PARAMETER` before the window existed.
    // The missing half is that SteamOS exports `EGL_PLATFORM=wayland`, and that variable —
    // not the GDK backend — is what Mesa reads to decide which platform the *default* EGL
    // display belongs to, the display named in the message WebKit aborts on. Left as it
    // came, it hands an X11 session to the Wayland platform, the one pairing that cannot
    // work. So the two are made to agree.
    //
    // Unsetting it isn't enough: with `WAYLAND_DISPLAY` still in the environment, Mesa's own
    // detection picks Wayland straight back. And this one is set over whatever is there,
    // rather than only when unset, because what's there *is* the fault — a value inherited
    // from a session that knows nothing about which backend we went on to choose.
    let on_x11 = choose_x11
        || env.gdk_backend == Backend::X11
        || (env.gdk_backend == Backend::Unset && !env.wayland && env.x_server);
    if on_x11 && env.egl_wayland {
        vars.push(("EGL_PLATFORM", "x11", Force::Always));
    }

    // Last resort, and where a run lands after one that aborted: take the GPU out of it
    // entirely. Software rasterisation is also what gets past a host DRI driver too new for
    // the Mesa loader we bundle, which is the other way that EGL display creation fails.
    if env.safe_mode {
        vars.push(("WEBKIT_DISABLE_COMPOSITING_MODE", "1", Force::IfUnset));
        vars.push(("LIBGL_ALWAYS_SOFTWARE", "1", Force::IfUnset));
    }

    vars
}

/// Defaults, not overrides — anything already set explicitly wins, so a machine whose driver
/// stack handles the fast paths can ask for them back with `GDK_BACKEND=wayland`. The one
/// exception is marked [`Force::Always`] above, and is there to resolve a contradiction
/// rather than to express a preference.
///
/// Has to run before the first window is built, since WebKit reads these when it spawns the
/// web process, and before any other thread exists — being `main`'s first statement gives
/// both.
fn prepare_webview_env(safe_mode: bool) {
    let env = GraphicsEnv::read(safe_mode);
    let vars = if cfg!(target_os = "linux") {
        webview_env_defaults(env)
    } else if cfg!(windows) {
        webview2_env_defaults(env)
    } else {
        Vec::new()
    };
    for (key, value, force) in vars {
        if force == Force::Always || std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
}

/// Ordinary defaults: whatever [`webview_env_defaults`] settles on for the session.
const TIER_DEFAULT: u8 = 0;
/// Every knob at once, GPU included. The last thing there is to try, so nothing escalates
/// past it.
const TIER_SAFE: u8 = 1;

/// The bundle identifier, from `tauri.conf.json`. Duplicated because the record below is
/// read before Tauri exists to be asked.
const APP_IDENTIFIER: &str = "com.frost.mxbikes";

/// How far the last run got.
///
/// A failed EGL display is not an error anyone can catch: WebKitGTK prints one line and
/// aborts the process, before the window is on screen and with nothing for the app to
/// handle. So the tier is written down on the way in and marked again once the page has
/// actually loaded, and a run that finds the previous attempt still unmarked knows it died
/// getting there — and starts one tier safer.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct GraphicsAttempt {
    tier: u8,
    /// Set once the webview has loaded a page, which is proof the web process survived.
    painted: bool,
    /// The build that tried it. A new version gets to try the fast path again rather than
    /// inheriting a verdict about libraries it may no longer be built against.
    version: String,
}

/// Where to start, given what the last run left behind.
fn next_graphics_tier(last: Option<&GraphicsAttempt>, version: &str) -> u8 {
    match last {
        // Nothing recorded, or a record from a build that isn't this one.
        None => TIER_DEFAULT,
        Some(a) if a.version != version => TIER_DEFAULT,
        // It painted. Start where it worked — including at tier 0, which is the common case
        // and costs the machine nothing.
        Some(a) if a.painted => a.tier.min(TIER_SAFE),
        // It didn't. Whatever we tried, try less of it.
        Some(a) => a.tier.saturating_add(1).min(TIER_SAFE),
    }
}

/// Rebuilds the `$XDG_DATA_HOME/<identifier>` that `app.path().app_local_data_dir()` returns
/// on Linux, since the read happens before there is an app handle to ask.
fn graphics_attempt_path() -> Option<std::path::PathBuf> {
    dirs_next::data_local_dir().map(|d| d.join(APP_IDENTIFIER).join("graphics-attempt.json"))
}

fn read_graphics_attempt() -> Option<GraphicsAttempt> {
    let raw = std::fs::read_to_string(graphics_attempt_path()?).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Best-effort throughout: a machine that can't write this file should still start, it just
/// won't remember. Logged rather than surfaced — it changes nothing the player can act on.
fn write_graphics_attempt(tier: u8, painted: bool) {
    let Some(path) = graphics_attempt_path() else {
        return;
    };
    let record = GraphicsAttempt {
        tier,
        painted,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let written = path
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| serde_json::to_string(&record).map_err(std::io::Error::other))
        .and_then(|json| std::fs::write(&path, json));
    if let Err(e) = written {
        log::warn!("[graphics] couldn't record the launch attempt: {e}");
    }
}

/// The tier this run started under, so the page-load hook can mark the right one good.
static GRAPHICS_TIER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(TIER_DEFAULT);

/// Decide the tier and apply it. Only the decision is remembered here — the record isn't
/// written until the window is about to be built, because a second launch exits inside the
/// single-instance guard before that point, and a marker it left behind would read as a
/// crash and drop the *next* real launch into safe graphics for nothing.
fn begin_graphics_attempt() {
    // The record is a Linux story: only there does a failed EGL display abort the process
    // before anything can catch it. The environment itself is settled on every platform,
    // because WebView2 has a safe-graphics lever of its own.
    let tier = if cfg!(target_os = "linux") {
        // `MXB_SAFE_GRAPHICS=0` is the way back out: it ignores the record, so a machine
        // pinned to software rendering by one bad launch — a kill during startup looks
        // exactly like an abort from here — can be put back on the fast path without
        // hunting for a file.
        match std::env::var("MXB_SAFE_GRAPHICS").unwrap_or_default().as_str() {
            "0" => TIER_DEFAULT,
            "1" => TIER_SAFE,
            _ => next_graphics_tier(read_graphics_attempt().as_ref(), env!("CARGO_PKG_VERSION")),
        }
    } else {
        TIER_DEFAULT
    };
    GRAPHICS_TIER.store(tier, std::sync::atomic::Ordering::Relaxed);
    prepare_webview_env(tier >= TIER_SAFE);
}

/// The environment WebView2 should start under.
///
/// Nothing by default — WebView2 paints on virtually every Windows machine. `MXB_SAFE_GRAPHICS=1`
/// takes the GPU out of it, which is the lever to pull for a window that came up black and
/// stayed that way: a first frame that never arrives is the GPU path failing.
fn webview2_env_defaults(env: GraphicsEnv) -> Vec<(&'static str, &'static str, Force)> {
    if !env.safe_mode {
        return Vec::new();
    }
    vec![(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--disable-gpu",
        Force::IfUnset,
    )]
}

/// Whether this process is running under Wine — CrossOver, Whisky, Kegworks or plain Wine.
///
/// `ntdll` exports `wine_get_version` under Wine and never on real Windows, which is the
/// check Wine documents for programs that need to tell. It matters here because Wine's
/// `ole32` faults inside `RegisterDragDrop`: the app came up as a transparent window and
/// died in `ole32` from its own window procedure, with WebView2 already running.
#[cfg(windows)]
fn under_wine() -> bool {
    use std::os::raw::{c_char, c_void};

    extern "system" {
        fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    // SAFETY: both take a NUL-terminated name and return null rather than failing. `ntdll`
    // is always already loaded, so this never brings a library in.
    unsafe {
        let ntdll = GetModuleHandleA(b"ntdll.dll\0".as_ptr() as *const c_char);
        !ntdll.is_null()
            && !GetProcAddress(ntdll, b"wine_get_version\0".as_ptr() as *const c_char).is_null()
    }
}

#[cfg(not(windows))]
fn under_wine() -> bool {
    false
}

/// Whether to register the OS drag-drop handler on the main window.
///
/// Under Wine it is what crashes the app, so it comes off there and stays on everywhere
/// else — a Windows player keeps the dropzone. `MXB_DRAG_DROP` forces the answer either way
/// (`0` off, `1` on) so one build can be tried both ways.
fn drag_drop_enabled() -> bool {
    match std::env::var("MXB_DRAG_DROP").ok().as_deref() {
        Some("0") => false,
        Some("1") => true,
        _ => !under_wine(),
    }
}

fn main() {
    // Before anything else: in a release build, refuse to run under a debugger. A live
    // debugger attached to the process defeats the static hardening the release profile
    // pays for (stripped symbols, fat LTO, no debug info), so this is the runtime half of
    // it. No-op in debug builds, so `tauri dev` and the tests stay debuggable. See
    // `antidebug`.
    antidebug::guard();

    begin_graphics_attempt();

    let builder = tauri::Builder::default();

    // One app, one process. Closing the window parks Frost's Mod Manager in the tray rather than
    // quitting it, so without this a second launch doesn't reveal the copy already
    // running — it builds a whole new one: another window, another tray icon, another
    // FrostMod, another mod watcher. Five launches in a day left five of everything, and
    // only the tray overflow to clean them up from.
    //
    // Registered before every other plugin: the guard's setup hook is what kills the
    // second process, and it should do so before anything else has started work that
    // would then need unwinding. `show_main` is the same path the tray's "Show Frost's Mod Manager"
    // takes, so relaunching behaves exactly like clicking the tray icon.
    //
    // Release builds only, for the same reason close-to-tray is (see `CloseRequested`
    // below): a `tauri dev` run must still start while the installed Frost's Mod Manager is sitting
    // in the tray, otherwise it would silently exit and just re-show the shipped app.
    //
    // The updater's restart is safe against this by construction, and it's worth knowing
    // why, because it looks like it shouldn't be: a restart spawns the replacement before
    // this process is gone, so a guard still held would make the new app mistake itself
    // for a second copy and quit — an update that leaves nothing running. It doesn't,
    // because `relaunch()` goes through `request_restart()`, and Tauri hands plugins
    // `RunEvent::Exit` — where this one releases the guard — before it spawns anything.
    // A restart that skipped the event loop would not be safe.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        log::info!("another instance was launched — showing the window already running");
        show_main(app);
    }));

    builder
        // Thumbnails for both catalogs, served from a disk cache instead of refetched on
        // every scroll. Registered here rather than per-window so the overlay — which
        // renders the same `ModCard` — gets it too.
        //
        // Asynchronous, so a cache miss that has to reach the origin never blocks the
        // webview's protocol thread.
        //
        // A URI scheme is not a permission subject, so no capability file changes with this.
        // If a CSP is ever enabled in `tauri.conf.json` (currently `null`), it must allow
        // `img-src imgcache: http://imgcache.localhost` or every thumbnail goes blank.
        .register_asynchronous_uri_scheme_protocol(imgcache::SCHEME, imgcache::handle)
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log_level())
                // Local time, not UTC. FrostMod's log is stamped in local time, and a
                // support thread that has to hold a timezone offset in its head while
                // reading the two side by side gets read wrong.
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: None,
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        // `mxb://` links, so an invite can be handed out as something to click rather than
        // a code to transcribe. See `handle_deep_link`.
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        // Copying happens minutes after the click that started it — a share code is
        // born when the upload lands — so the web clipboard's focus and gesture rules
        // rule it out. This writes from the process instead.
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        // The overlay's hotkey has to fire while MX Bikes holds keyboard focus.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(FrostmodProcess::default())
        .manage(ModWatcher::default())
        .manage(ProfileWatcher::default())
        .manage(PaintWatcher::default())
        .manage(LookWatcher::default())
        .manage(CloudServers::default())
        .manage(shop_session::ShopSession::default())
        .manage(hub_session::HubSession::default())
        .manage(voice::Monitor::default())
        .manage(voice::session::Session::default())
        .setup(|app| {
            log::info!("Frost's Mod Manager {} starting", env!("CARGO_PKG_VERSION"));

            // The main window is `"create": false` in tauri.conf.json so it is built here
            // rather than by Tauri's own startup loop, which is the only way to decide the
            // drag-drop handler per run: it can only be turned off while the window is
            // being built. Everything else about the window still comes from the config,
            // the macOS overrides in `tauri.macos.conf.json` included — and that file
            // replaces this array wholesale, so it has to repeat `"create": false` or
            // Tauri opens `main` itself and the build below aborts on the duplicate.
            let drag_drop = drag_drop_enabled();
            log::info!("wine={} drag-drop-handler={}", under_wine(), drag_drop);
            // The single-instance guard has had its say by now, so this process is the one
            // that will actually build a window: safe to claim the attempt. On Linux the
            // next statement is where a broken EGL stack takes the whole process down.
            let tier = GRAPHICS_TIER.load(std::sync::atomic::Ordering::Relaxed);
            if cfg!(target_os = "linux") {
                write_graphics_attempt(tier, false);
            }
            for window_config in app
                .config()
                .app
                .windows
                .iter()
                .filter(|w| w.label == MAIN_WINDOW)
            {
                let mut builder =
                    tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?
                        // Also what puts the window on screen: it is hidden until the
                        // document is there to show, and a hidden webview never composites
                        // — so waiting for a painted frame here would wait forever.
                        .on_page_load(|window, payload| {
                            log::info!("[startup] main window {:?}", payload.event());
                            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                                firstpaint::loaded(window.app_handle());
                            }
                        });
                if !drag_drop {
                    builder = builder.disable_drag_drop_handler();
                }
                // Proof of life for the tier above. A load event can only come from a web
                // process that started, and a broken EGL stack takes that process down
                // before it ever reaches one — so the first event, rather than the finished
                // one, is both enough to clear the tier and the harder of the two to miss.
                // Fires again on every navigation, and only the first is news.
                if cfg!(target_os = "linux") {
                    let marked = std::sync::atomic::AtomicBool::new(false);
                    builder = builder.on_page_load(move |_, _| {
                        if !marked.swap(true, std::sync::atomic::Ordering::Relaxed) {
                            write_graphics_attempt(tier, true);
                        }
                    });
                }
                builder.build()?;
            }
            // The window is built hidden and revealed by `window_painted`; this is what
            // rescues it when that never arrives.
            firstpaint::arm(app.handle());
            // Cloudflare scores the User-Agent alongside the IP, and a cf_clearance is bound
            // to the UA that earned it — a log about a block should say which one was used.
            log::info!("{} user-agent: {}", mxb_session::site().domain, mxb_session::UA);
            // A blank webview leaves nothing else behind to diagnose from, so record the
            // session this run started under and every knob `prepare_webview_env` settled on.
            if cfg!(target_os = "linux") {
                let on = |key: &str| std::env::var(key).unwrap_or_default() == "1";
                log::info!(
                    "webview env: appimage={} wayland={} x_server={} gdk_backend={} \
                     egl_platform={} dmabuf_disabled={} compositing_disabled={} \
                     software_gl={} tier={}",
                    std::env::var_os("APPIMAGE").is_some(),
                    std::env::var_os("WAYLAND_DISPLAY").is_some(),
                    std::env::var_os("DISPLAY").is_some(),
                    std::env::var("GDK_BACKEND").unwrap_or_else(|_| "default".into()),
                    std::env::var("EGL_PLATFORM").unwrap_or_else(|_| "default".into()),
                    on("WEBKIT_DISABLE_DMABUF_RENDERER"),
                    on("WEBKIT_DISABLE_COMPOSITING_MODE"),
                    on("LIBGL_ALWAYS_SOFTWARE"),
                    tier,
                );
            }
            if cfg!(windows) {
                log::info!(
                    "webview env: webview2_args={:?}",
                    std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default(),
                );
            }
            if let Ok(dir) = app.path().app_local_data_dir() {
                log::info!("data dir (config/session/frostmod): {}", dir.display());
            }
            // Linux and macOS drive FrostMod by leaving a file in its folder — the one
            // thing this side of the Wine prefix can reach. Told once here, because the
            // senders are called from watchers that hold no handle to resolve a data dir
            // with.
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            frostmod::set_command_dir(frostmod_manage::frostmod_dir(app.handle()));
            if let Ok(dir) = app.path().app_log_dir() {
                log::info!("log dir: {}", dir.display());
            }

            {
                use tauri_plugin_deep_link::DeepLinkExt;
                // Windows and Linux bind the scheme at runtime rather than at install, so a
                // build run from a folder — or a dev build — still answers `mxb://`. macOS
                // takes it from the bundle's Info.plist and has no runtime equivalent.
                #[cfg(any(windows, target_os = "linux"))]
                if let Err(e) = app.deep_link().register_all() {
                    // Not fatal: everything except the link still works, and on a locked-down
                    // machine this is the one part that can legitimately be refused.
                    log::warn!("[deep-link] couldn't register the mxb:// scheme: {e}");
                }
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    let urls: Vec<String> = event.urls().iter().map(|u| u.to_string()).collect();
                    handle_deep_link(&handle, &urls);
                });
            }

            let show = MenuItem::with_id(app, "show", "Show Frost's Mod Manager", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Frost's Mod Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => {
                        frostmod_manage::stop(&app.state::<FrostmodProcess>());
                        usage::flush_on_exit(app);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            let handle = app.handle();
            log::info!(
                "config: {} ({})",
                config::config_path(handle).display(),
                if config::exists(handle) { "found" } else { "missing" },
            );
            // `load_or_detect` rebuilds a missing/unreadable config from the standard
            // MX Bikes folder, so a lost config no longer means a trip through setup.
            if let Some(mut cfg) = config::load_or_detect(handle) {
                // Auto-detect the active game's install on launch for configs that
                // never got one (created before detection existed, or when the
                // game wasn't installed yet). Only fills a blank — never overrides
                // a manual pick — and persists it so the 3D rider preview works.
                if cfg.game_path.trim().is_empty() {
                    if let Some(gp) = config::detect_game_path(cfg.game()) {
                        log::info!("auto-detected {} install: {gp}", cfg.game().display);
                        cfg.game_path = gp;
                        let _ = config::save(handle, &cfg);
                    }
                }
                let manager = handle.autolaunch();
                let stale = cfg.autostart_binding_rev < config::AUTOSTART_BINDING_REV;
                // Every call below is keyed on the *current* product name, so none of them
                // can see the entry the rename orphaned. Clear it before they run — but read
                // its veto first: a player who switched the app off in Task Manager did so
                // under the old name, and the new name carries no flag yet. Without carrying
                // it over, the reconcile below sees "wanted, not enabled, not vetoed" and
                // quietly switches it back on, which is the regression `Autostart::Adopt`
                // exists to prevent.
                let legacy_vetoed = if stale {
                    let vetoed = startup_vetoed(LEGACY_APP_NAME);
                    delete_legacy_login_item(LEGACY_APP_NAME);
                    vetoed
                } else {
                    false
                };
                let mut cfg_dirty = false;
                match autostart_action(
                    cfg.launch_at_startup,
                    manager.is_enabled().unwrap_or(false),
                    startup_vetoed(&handle.package_info().name) || legacy_vetoed,
                    stale,
                ) {
                    Autostart::Enable => {
                        let _ = manager.enable();
                    }
                    Autostart::Rebind => {
                        log::info!("re-binding the login item to this build's binary");
                        let _ = manager.disable();
                        let _ = manager.enable();
                    }
                    Autostart::Disable => {
                        let _ = manager.disable();
                    }
                    Autostart::Adopt => {
                        log::info!(
                            "Windows' startup list has the app switched off — turning \
                             Launch at startup off to match"
                        );
                        cfg.launch_at_startup = false;
                        cfg_dirty = true;
                    }
                    Autostart::Leave => {}
                }
                if stale {
                    cfg.autostart_binding_rev = config::AUTOSTART_BINDING_REV;
                    cfg_dirty = true;
                }
                if cfg_dirty {
                    let _ = config::save(handle, &cfg);
                }
                if cfg.auto_run_frostmod && frostmod_manage::is_installed(handle) {
                    let state = handle.state::<FrostmodProcess>();
                    // Not `let _ =`: a FrostMod that refused to start at launch is the
                    // reason half the "FrostMod isn't working" reports exist, and it used
                    // to leave nothing behind in the log to say so.
                    if let Err(e) = frostmod_manage::start(handle, &state) {
                        log::warn!("FrostMod didn't start at launch: {e:#}");
                    }
                }
                if cfg.watch_mods_reload {
                    let watcher = handle.state::<ModWatcher>();
                    modwatch::start(handle, &watcher, &cfg.mods_path);
                }
                // Catch up on whatever changed while the app was shut. The folder watcher
                // above only sees changes from here on, and it is a setting the player can
                // turn off — so without this pass, mods deleted between sessions would never
                // be noticed at all.
                ledger_reconcile_detached(handle);
                // Paint sync, both directions, from the moment the app opens:
                //  * publish, because the look may have changed in the game's garage while
                //    the app was shut, and nothing would ever have noticed;
                //  * watch, so the same change during this session is noticed as it happens.
                // The publish no-ops when paint sync is turned off; the watching is also
                // what keeps the look watcher pointed at the right files, which has nothing
                // to do with sync.
                publish_paints_soon(handle, &cfg, None);
                if watches_looks(&cfg) {
                    let profiles = handle.state::<ProfileWatcher>();
                    profilewatch::start(handle, &profiles, &cfg.profiles_dir());
                }
                // And watch the paints the rider is wearing, so saving one over the top
                // while the game runs reaches the game.
                watch_worn_paints(handle);
                // A combo another app already owns shouldn't stop the app from starting
                // — Settings reports the state and lets the player pick another.
                if let Err(e) = overlay::register(handle, &cfg) {
                    log::warn!("overlay hotkey not registered: {e}");
                }
            } else {
                log::info!("no MX Bikes folder found — showing first-run setup");
            }
            // Notice the game starting (Steam or Play button) to re-arm FrostMod for the
            // session and check the mods folder is really on disk.
            sessionwatch::start(handle);
            secure_launch::watch(handle);
            // Voice follows the rider onto whatever server they join, and off it again.
            // There is nothing to press: the supervisor is the whole of "joining a room".
            voice::session::start(handle);
            shop_session::load_session(handle);
            hub_session::load_session(handle);
            shop_catalog_session::load(handle);
            mxb_session::load(handle);
            imgcache::start_maintenance(handle);
            memwatch::start();
            // Anonymous counters. Started last of the startup tasks and after the config
            // work above, because the install id it mints is saved into that same config.
            usage::start(handle);
            // Only registers the result listener and stashes the handle — the hidden window
            // isn't built until something is actually refused.
            mxb_fetch::init(handle);
            // Same again for the shop's signed-in half. Nothing opens until the purchases tab
            // is actually used, so a user who never signs in never pays for the window.
            shop_fetch::init(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The overlay is a HUD over a running game, so clicking back into the game
            // has to put it away — an unfocused webview stops repainting and leaves an
            // empty frame over the game that still eats clicks.
            if let WindowEvent::Focused(false) = event {
                if window.label() == overlay::LABEL {
                    overlay::on_focus_lost(window.app_handle());
                    return;
                }
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing the overlay (Alt+F4, its own button) parks it rather than
                // destroying it, so the next hotkey press doesn't rebuild the webview.
                if window.label() == overlay::LABEL {
                    api.prevent_close();
                    let _ = overlay::hide(window.app_handle());
                    return;
                }
                // Everything else — the clearance check, the shop login — closes for real.
                if !parks_in_tray(window.label()) {
                    return;
                }
                let cfg = config::load(window.app_handle()).unwrap_or_default();
                let painted = firstpaint::painted();
                if parks_on_close(painted, cfg.run_in_background) {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                // Closing for real: this is the last chance to report the session, and a
                // short one would otherwise never be counted at all.
                usage::flush_on_exit(window.app_handle());
                if !painted {
                    log::warn!(
                        "[startup] closing a main window that never painted — quitting \
                         rather than parking it in the tray"
                    );
                    frostmod_manage::stop(&window.app_handle().state::<FrostmodProcess>());
                }
            }
        })
        // Wrapped rather than passed straight in: `ipc_allowed` closes the hole that giving
        // the hidden mxb-mods.com window a capability would otherwise open. See its doc.
        .invoke_handler({
            // The macro is generic over the runtime; naming `Wry` here is what lets the
            // wrapper below infer what it is wrapping.
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
            window_painted,
            is_configured,
            get_config,
            create_config,
            bike_preview_available,
            app_platform,
            search_mods,
            get_mod_detail,
            get_mod_ratings,
            get_installed_mods,
            scan_library,
            mxb_core::trackview::get_pkz_meta_cached,
            mxb_core::trackview::get_pkz_meta,
            mxb_core::trackview::get_pkz_preview,
            mxb_core::trackview::read_track_info,
            mxb_core::trackview::load_track_terrain,
            generate_track,
            check_track,
            base_track_program,
            blank_track_program,
            random_track_program,
            close_track_lap,
            fit_track_budget,
            preview_track,
            export_track_source,
            track_tools_status,
            set_track_tools,
            download_track_tools,
            build_track,
            mxb_core::trackview::load_track_overview,
            mxb_core::trackview::load_track_scenery,
            mxb_core::trackview::load_track_surfaces,
            mxb_core::trackview::read_track_placements,
            save_track_props,
            bake_prop_library,
            read_track_placeable,
            load_track_prop,
            mxb_core::trackview::load_track_backdrop,
            mxb_core::trackview::load_track_ground,
            mxb_core::trackview::load_track_ground_layers,
            mxb_core::trackview::diagnose_track,
            unpack_paint,
            mxb_core::viewer::texture_bytes,
            mxb_core::viewer::watch_paint_files,
            mxb_core::viewer::unpack_pkz,
            content_lock_available,
            content_lock_plan,
            content_lock_run,
            content_secure_available,
            set_mxbsecure_enabled,
            mxbsecure_lock,
            mxbsecure_verify,
            secure_steam_id,
            mxbsecure_provision,
            mxbsecure_open_offline,
            mxbsecure_generate,
            local_guid,
            mxb_core::viewer::load_bike_model,
            preview_model_swap,
            mxb_core::viewer::load_rider_model,
            mxb_core::viewer::load_rider_body_model,
            mxb_core::viewer::load_gear_model,
            mxb_core::viewer::load_stock_gear_model,
            mxb_core::viewer::list_gear_paints,
            mxb_core::viewer::list_installed_gear_paints,
            paint_studio_load,
            paint_studio_pixels,
            paint_studio_stage,
            photo_save,
            psd_read,
            psd_save,
            paint_studio_target,
            paint_studio_save,
            paint_studio_extract,
            paint_studio_hints,
            scan_rider_targets,
            scan_gear_repairs,
            repair_gear,
            scan_bike_targets,
            scan_model_swaps,
            apply_model_swap,
            bike_folders,
            model_swap_liveries,
            move_model_swap,
            delete_model_swap,
            list_bike_liveries,
            set_model_paints,
            scan_sound_swaps,
            apply_sound_swap,
            bind_sound,
            unbind_sound,
            reshade_status,
            set_reshade_path,
            apply_reshade_preset,
            delete_reshade_preset,
            detect_loose_swaps,
            register_loose_swaps,
            detect_orphaned_setup,
            repair_orphaned_setup,
            add_to_library,
            cancel_install,
            import_file,
            plan_drop,
            repreview_drop,
            commit_drop,
            cancel_drop,
            move_mod,
            uninstall_mod,
            reveal_in_explorer,
            log_client,
            logs_info,
            share_logs,
            open_logs_folder,
            export_logs,
            set_game_path,
            set_wine_runner,
            wine_host_info,
            set_mods_path,
            set_intro_seen,
            set_seen_version,
            set_profiles_path,
            detect_game_path,
            count_profiles_in,
            get_mods_root,
            set_run_in_background,
            set_analytics_enabled,
            track_event,
            set_launch_at_startup,
            set_auto_run_frostmod,
            set_frostmod_args,
            set_instant_refresh,
            overlay_toggle,
            overlay_hide,
            overlay_open_main,
            overlay_state,
            set_overlay_enabled,
            set_overlay_hotkey,
            voice_devices,
            voice_status,
            voice_mute,
            set_voice_proximity,
            set_voice_enabled,
            set_paint_sync_enabled,
            remove_synced_paints,
            set_preview_tyres,
            set_voice_input_device,
            set_voice_output_device,
            set_voice_ptt_hotkey,
            set_voice_levels,
            set_voice_toggle_to_talk,
            voice_meter_start,
            voice_meter_stop,
            voice_test_output,
            set_watch_mods_reload,
            set_secure_content_inject,
            frostmod_reload,
            frostmod_running,
            frostmod_attachment,
            garage_scan_bikes,
            garage_swap_bike,
            frostmod_status,
            frostmod_install,
            frostmod_install_runtime,
            frostmod_repair_runtimes,
            frostmod_clear_stray_msvcr90,
            runtime_downloads,
            frostmod_start,
            frostmod_stop,
            launch_game,
            join_server,
            list_master_servers,
            probe_server,
            server_riders,
            servers_with_paint_sync,
            guess_server_track,
            ranked_identity,
            ranked_profile,
            set_ranked_guid,
            experimental_state,
            enroll_account,
            // Paid plugins: the catalogue, redeeming a key, and getting a bundle on disk.
            plugins::plugin_list,
            plugins::plugin_redeem,
            plugins::plugin_install,
            plugins::plugin_remove,
            plugins::plugin_runtime,
            plugins::plugin_read_file,
            plugins::plugin_write_file,
            plugins::plugin_list_dir,
            plugins::plugin_delete_file,
            plugins::plugin_install_payload,
            set_guid,
            publish_paints,
            sync_paints,
            list_servers,
            save_servers,
            cp_servers,
            server_status,
            server_tracks,
            server_probe,
            publish_server,
            unpublish_server,
            provision_server,
            fleet_state,
            cloud_servers,
            destroy_cloud_server,
            parse_pairing,
            server_action,
            server_set_config,
            game_running,
            shop_login,
            shop_status,
            shop_logout,
            shop_my_downloads,
            shop_match_catalog,
            shop_install,
            shop_installed_map,
            hub_search,
            hub_categories,
            hub_detail,
            hub_login,
            hub_status,
            hub_logout,
            hub_my_downloads,
            hub_install,
            record_download,
            download_history,
            forget_download,
            clear_download_history,
            library_ledger,
            ledger_capture,
            forget_ledger_entry,
            restore_ledger_entry,
            clear_ledger,
            shop_catalog_available,
            shop_catalog_status,
            shop_catalog_categories,
            shop_catalog_search,
            shop_catalog_detail,
            shop_catalog_refresh,
            presets_list_profiles,
            presets_list_bikes,
            presets_forget_bike,
            presets_read_loadout,
            presets_slots,
            list_games,
            set_active_game,
            presets_apply,
            presets_list,
            presets_save,
            presets_delete,
            presets_export,
            presets_decode,
            presets_import,
            feel_list,
            feel_capture,
            feel_save,
            feel_delete,
            feel_apply,
            feel_export,
            feel_decode,
            feel_import,
            preset_bundle_stats,
            preset_bundle_create,
            preset_bundle_import,
            file_share_plan,
            file_share_create,
            file_share_preview,
            file_share_import,
            live_share_publish,
            live_share_preview,
            live_share_subscribe,
            live_share_sync,
            live_share_check,
            live_share_list,
            live_share_forget,
            live_share_set_auto,
            live_share_owner_code,
            live_share_adopt,
            mods_state_scan,
            mods_state_plan,
            mods_state_set,
            mods_state_delete,
            mods_state_apply,
                mods_state_restore_all
            ];
            move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
                let (label, command) = (
                    invoke.message.webview().label().to_string(),
                    invoke.message.command().to_string(),
                );
                if !ipc_allowed(&label, &command) {
                    log::warn!("refused IPC '{command}' from the '{label}' window");
                    invoke.resolver.reject("not permitted from this window");
                    return true;
                }
                handler(invoke)
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod build_progress_tests {
    use super::*;

    /// The studio reads these five names off the event. `#[serde(flatten)]` is the only thing
    /// holding the phase's own fields at the top level, and nothing about that is checked by
    /// the compiler — get it wrong and the bar simply never moves.
    #[test]
    fn a_build_reports_one_flat_object() {
        let at = trackbuild::Plan::new(2049, true, true).start("map");
        let json = serde_json::to_value(BuildProgress {
            slug: "corpus_national".into(),
            at,
        })
        .unwrap();
        let obj = json.as_object().expect("an object");
        let mut names: Vec<&str> = obj.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["expect", "from", "phase", "slug", "to"]);
        assert_eq!(obj["phase"], "map");
        assert_eq!(obj["slug"], "corpus_national");
        assert!(obj["to"].as_f64().unwrap() > obj["from"].as_f64().unwrap());
    }
}

#[cfg(test)]
mod webview_env_tests {
    use super::*;
    use std::collections::HashMap;

    fn defaults(env: GraphicsEnv) -> HashMap<&'static str, &'static str> {
        webview_env_defaults(env).into_iter().map(|(k, v, _)| (k, v)).collect()
    }

    /// Whether a variable is set over one the session already exported, or only into a gap.
    fn force(env: GraphicsEnv, key: &str) -> Option<Force> {
        webview_env_defaults(env)
            .into_iter()
            .find(|(k, _, _)| *k == key)
            .map(|(_, _, f)| f)
    }

    /// SteamOS, both modes: Desktop is Plasma Wayland and gamescope is a compositor of its
    /// own, and each runs an XWayland the app can land on instead.
    const STEAMOS: GraphicsEnv = GraphicsEnv {
        wayland: true,
        x_server: true,
        gdk_backend: Backend::Unset,
        egl_wayland: true,
        safe_mode: false,
    };

    /// The cheap fix for the silent no-paint fault, and it costs a machine nothing that
    /// works already — so it goes on everywhere rather than being guessed at.
    #[test]
    fn the_shared_memory_renderer_is_always_the_default() {
        for env in [
            GraphicsEnv::default(),
            STEAMOS,
            GraphicsEnv { wayland: false, ..STEAMOS },
            GraphicsEnv { safe_mode: true, ..STEAMOS },
        ] {
            assert_eq!(
                defaults(env).get("WEBKIT_DISABLE_DMABUF_RENDERER"),
                Some(&"1"),
                "{env:?} should have fallen back to shared memory",
            );
        }
    }

    /// Nothing here forces a backend of its own accord any more. The EGL abort this used to
    /// answer for was the AppImage's bundled libwayland, taken out of the bundle itself; an
    /// AppImage is on XWayland regardless, because its AppRun hook says so before this
    /// process starts.
    #[test]
    fn a_wayland_session_is_left_on_its_own_backend() {
        assert_eq!(defaults(STEAMOS).get("GDK_BACKEND"), None);
    }

    /// Without an X server to fall back to, forcing the backend trades a white screen for
    /// no window at all.
    #[test]
    fn wayland_with_no_x_server_is_left_alone() {
        let no_xwayland = GraphicsEnv { x_server: false, ..STEAMOS };
        assert_eq!(defaults(no_xwayland).get("GDK_BACKEND"), None);

        let safe = GraphicsEnv { safe_mode: true, ..no_xwayland };
        assert_eq!(defaults(safe).get("GDK_BACKEND"), None);
    }

    /// GTK already picks X11 there; saying so again would only be noise in the log — and
    /// safe mode has nothing to move the session *to*.
    #[test]
    fn a_plain_x11_session_needs_no_override() {
        let x11_only = GraphicsEnv { wayland: false, ..STEAMOS };
        assert_eq!(defaults(x11_only).get("GDK_BACKEND"), None);

        let safe = GraphicsEnv { safe_mode: true, ..x11_only };
        assert_eq!(defaults(safe).get("GDK_BACKEND"), None);
    }

    /// The escape hatch to hand someone whose screen is still white: every knob at once.
    #[test]
    fn safe_mode_takes_the_gpu_out_of_it() {
        let vars = defaults(GraphicsEnv { safe_mode: true, ..STEAMOS });
        assert_eq!(vars.get("GDK_BACKEND"), Some(&"x11"));
        assert_eq!(vars.get("WEBKIT_DISABLE_COMPOSITING_MODE"), Some(&"1"));
        assert_eq!(vars.get("LIBGL_ALWAYS_SOFTWARE"), Some(&"1"));
    }

    /// Nothing is imposed on a desktop that was never broken.
    #[test]
    fn an_ordinary_session_gets_only_the_renderer_default() {
        let vars = defaults(GraphicsEnv::default());
        assert_eq!(vars.len(), 1);
        assert!(vars.contains_key("WEBKIT_DISABLE_DMABUF_RENDERER"));
    }

    /// The bug the XWayland fix didn't reach: a Deck aborts on `EGL_BAD_PARAMETER` under
    /// `GDK_BACKEND=x11` because the session's `EGL_PLATFORM=wayland` still points Mesa at
    /// the wrong platform for the default display. The two have to name the same thing —
    /// including when the move to X11 is ours, which is safe mode.
    #[test]
    fn egl_follows_the_backend_onto_xwayland() {
        let safe = GraphicsEnv { safe_mode: true, ..STEAMOS };
        assert_eq!(defaults(safe).get("GDK_BACKEND"), Some(&"x11"));
        assert_eq!(defaults(safe).get("EGL_PLATFORM"), Some(&"x11"));
    }

    /// The value came from a session that knew nothing about the backend we went on to pick,
    /// so this is the one variable set over what's already there rather than into a gap.
    #[test]
    fn the_egl_platform_correction_overrides_the_session() {
        let safe = GraphicsEnv { safe_mode: true, ..STEAMOS };
        assert_eq!(force(safe, "EGL_PLATFORM"), Some(Force::Always));
        assert_eq!(force(safe, "GDK_BACKEND"), Some(Force::IfUnset));
        assert_eq!(
            force(safe, "WEBKIT_DISABLE_DMABUF_RENDERER"),
            Some(Force::IfUnset),
        );
    }

    /// How the Deck report arrived: the player had already exported `GDK_BACKEND=x11`, so we
    /// added nothing and the contradiction went uncorrected. Their backend, our correction.
    #[test]
    fn a_hand_forced_x11_backend_still_gets_the_correction() {
        let by_hand = GraphicsEnv { gdk_backend: Backend::X11, ..STEAMOS };
        assert_eq!(defaults(by_hand).get("GDK_BACKEND"), None);
        assert_eq!(defaults(by_hand).get("EGL_PLATFORM"), Some(&"x11"));
    }

    /// A stale `EGL_PLATFORM` breaks a session with no compositor at all just as thoroughly.
    #[test]
    fn a_plain_x11_session_gets_the_correction_too() {
        let x11_only = GraphicsEnv { wayland: false, ..STEAMOS };
        assert_eq!(defaults(x11_only).get("EGL_PLATFORM"), Some(&"x11"));
    }

    /// Staying on Wayland means `EGL_PLATFORM=wayland` is right, and an installed build has
    /// no reason to leave. Correcting it here would break the machine instead of fixing it.
    #[test]
    fn a_session_left_on_wayland_keeps_its_egl_platform() {
        assert_eq!(defaults(STEAMOS).get("EGL_PLATFORM"), None);

        let asked_for_wayland = GraphicsEnv { gdk_backend: Backend::Wayland, ..STEAMOS };
        assert_eq!(defaults(asked_for_wayland).get("EGL_PLATFORM"), None);

        let no_xwayland = GraphicsEnv { x_server: false, ..STEAMOS };
        assert_eq!(defaults(no_xwayland).get("EGL_PLATFORM"), None);
    }

    /// Nothing to reconcile: a session that never named a platform leaves Mesa to work it
    /// out from the display it's handed, which is what we'd be asking for anyway.
    #[test]
    fn an_unset_egl_platform_is_left_unset() {
        let quiet = GraphicsEnv { egl_wayland: false, safe_mode: true, ..STEAMOS };
        assert_eq!(quiet.gdk_backend, Backend::Unset);
        assert_eq!(defaults(quiet).get("GDK_BACKEND"), Some(&"x11"));
        assert_eq!(defaults(quiet).get("EGL_PLATFORM"), None);
    }

    /// WebView2 paints on virtually every Windows machine, and forcing software rendering
    /// on all of them to cure the few would be a bad trade.
    #[test]
    fn windows_is_left_alone_unless_safe_graphics_is_asked_for() {
        assert!(webview2_env_defaults(GraphicsEnv::default()).is_empty());
    }

    /// The lever support has to pull for a window that came up black and stayed black.
    #[test]
    fn safe_graphics_takes_the_gpu_out_of_webview2() {
        let vars: HashMap<_, _> = webview2_env_defaults(GraphicsEnv {
            safe_mode: true,
            ..GraphicsEnv::default()
        })
        .into_iter()
        .map(|(k, v, _)| (k, v))
        .collect();
        assert_eq!(
            vars.get("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"),
            Some(&"--disable-gpu"),
        );
    }
}

#[cfg(test)]
mod graphics_tier_tests {
    use super::*;

    fn attempt(tier: u8, painted: bool, version: &str) -> GraphicsAttempt {
        GraphicsAttempt { tier, painted, version: version.into() }
    }

    /// The overwhelmingly common case, and it must not cost anything: no history, fast path.
    #[test]
    fn a_first_run_takes_the_ordinary_defaults() {
        assert_eq!(next_graphics_tier(None, "0.9.2"), TIER_DEFAULT);
    }

    /// The bug this exists for. WebKitGTK prints one line and aborts before the window is on
    /// screen — there is no error to catch, so the only evidence is an attempt never marked.
    #[test]
    fn a_launch_that_died_before_painting_escalates() {
        let died = attempt(TIER_DEFAULT, false, "0.9.2");
        assert_eq!(next_graphics_tier(Some(&died), "0.9.2"), TIER_SAFE);
    }

    /// Once it works, it keeps working the same way — otherwise every launch would abort
    /// once on the way to the tier that paints.
    #[test]
    fn a_tier_that_painted_is_where_the_next_run_starts() {
        for tier in [TIER_DEFAULT, TIER_SAFE] {
            let good = attempt(tier, true, "0.9.2");
            assert_eq!(next_graphics_tier(Some(&good), "0.9.2"), tier);
        }
    }

    /// Safe graphics is the last thing there is to try. Aborting there too means the fault is
    /// somewhere this can't reach, and climbing further would only invent tiers.
    #[test]
    fn nothing_escalates_past_safe_graphics() {
        let died = attempt(TIER_SAFE, false, "0.9.2");
        assert_eq!(next_graphics_tier(Some(&died), "0.9.2"), TIER_SAFE);
    }

    /// A verdict is about one build against one set of libraries. An update — of the app or,
    /// by way of it, the WebKit an AppImage carries — earns a fresh try at the fast path.
    #[test]
    fn a_new_build_retries_the_fast_path() {
        let pinned = attempt(TIER_SAFE, true, "0.9.1");
        assert_eq!(next_graphics_tier(Some(&pinned), "0.9.2"), TIER_DEFAULT);

        let died = attempt(TIER_DEFAULT, false, "0.9.1");
        assert_eq!(next_graphics_tier(Some(&died), "0.9.2"), TIER_DEFAULT);
    }

    /// A record written by a build that knew of tiers this one doesn't shouldn't strand the
    /// app above the top of its own ladder.
    #[test]
    fn a_tier_from_the_future_is_clamped() {
        let ahead = attempt(9, true, "0.9.2");
        assert_eq!(next_graphics_tier(Some(&ahead), "0.9.2"), TIER_SAFE);
    }

    /// It travels as JSON through the app-data folder, and a half-written or hand-edited file
    /// has to read as "no history" rather than take the app down on the way past.
    #[test]
    fn a_record_survives_a_round_trip() {
        let record = attempt(TIER_SAFE, true, "0.9.2");
        let json = serde_json::to_string(&record).expect("serialises");
        assert_eq!(
            serde_json::from_str::<GraphicsAttempt>(&json).expect("parses"),
            record,
        );
        assert!(serde_json::from_str::<GraphicsAttempt>("{ not json").is_err());
    }
}

/// The window that never painted is the one these are for: it has no close button drawn
/// in it, so whatever the player does next has to actually get rid of it.
#[cfg(test)]
mod close_behaviour_tests {
    use super::*;

    /// The whole bug: Alt+F4 on a black window parked it in the tray, the process stayed
    /// alive holding the single-instance guard, and reopening the app called `show_main`
    /// and handed the same dead window back. Task Manager was the only way out.
    #[test]
    fn a_window_that_never_painted_never_parks_in_the_tray() {
        assert!(!parks_on_close(false, true));
        assert!(!parks_on_close(false, false));
    }

    /// And the setting still means what it says for a window that works.
    #[test]
    fn a_painted_window_still_honours_run_in_background() {
        // Release builds on Windows and macOS park; this test binary is neither, so the
        // assertion is on the platform-and-build gate agreeing with itself rather than on
        // a fixed answer.
        let parks = cfg!(not(target_os = "linux")) && !cfg!(debug_assertions);
        assert_eq!(parks_on_close(true, true), parks);
        assert!(!parks_on_close(true, false), "turning it off always closes for real");
    }
}

#[cfg(test)]
mod release_version_tests {
    use super::*;

    /// The bug this exists for: a build cut from `v0.8.0-beta.1` packages itself as `0.8.0`,
    /// so the Beta badge — which keys off a pre-release suffix — never fired on any beta.
    #[test]
    fn a_release_tag_carries_its_prerelease_suffix() {
        assert_eq!(
            pick_release_version(Some("v0.8.0-beta.1"), "0.8.0".into()),
            "0.8.0-beta.1",
        );
    }

    /// Local builds, and the `workflow_dispatch` runs that pass an empty tag.
    #[test]
    fn no_tag_leaves_the_packaged_version_alone() {
        assert_eq!(pick_release_version(None, "0.8.0".into()), "0.8.0");
        assert_eq!(pick_release_version(Some("  "), "0.8.0".into()), "0.8.0");
    }

    /// `github.ref_name` is a *branch* outside a tag push. Believing it would put "main" in
    /// the About box, which reads as a broken build rather than a misconfigured workflow.
    #[test]
    fn a_tag_that_isnt_a_version_is_ignored() {
        for junk in ["main", "chore/harden-release-binary", "vNext"] {
            assert_eq!(
                pick_release_version(Some(junk), "0.8.0".into()),
                "0.8.0",
                "{junk} should not have been believed",
            );
        }
    }

    /// The `v` is a tag convention, not part of the version — the UI adds its own.
    #[test]
    fn the_tags_v_prefix_is_optional() {
        assert_eq!(pick_release_version(Some("0.9.0"), "0.8.0".into()), "0.9.0");
        assert_eq!(pick_release_version(Some("v0.9.0"), "0.8.0".into()), "0.9.0");
    }
}

#[cfg(test)]
mod sync_target_tests {
    use super::*;

    fn registry() -> Vec<String> {
        vec!["alpha".to_string(), "bravo".to_string(), "charlie".to_string()]
    }

    /// The bug that emptied the database: every automatic pull that ran before the rider had
    /// joined anything asked every server on the platform for its full roster — and each of
    /// those answers is the paints of everyone on that server.
    #[test]
    fn an_automatic_pull_with_nowhere_to_aim_asks_nobody() {
        assert!(sync_targets(None, &registry(), Sweep::Never).is_empty());
    }

    /// The manual button still works the way it reads: a person asked whose paints they are
    /// missing, and with no server of their own the registry is the only answer there is.
    #[test]
    fn a_person_pressing_sync_may_still_ask_everyone() {
        assert_eq!(sync_targets(None, &registry(), Sweep::Allowed), registry());
    }

    /// An address is a server. Whoever supplied it, and whatever the sweep rule says, that is
    /// the one to ask — a targeted sync is never the expensive case.
    #[test]
    fn an_address_is_asked_about_whatever_the_sweep_rule_says() {
        let one = vec!["bravo".to_string()];
        assert_eq!(sync_targets(Some("bravo".into()), &registry(), Sweep::Never), one);
        assert_eq!(sync_targets(Some("bravo".into()), &registry(), Sweep::Allowed), one);
    }

    /// An empty registry is not a reason to sweep something else — it is simply nothing.
    #[test]
    fn an_empty_registry_asks_nobody_either_way() {
        assert!(sync_targets(None, &[], Sweep::Allowed).is_empty());
        assert!(sync_targets(None, &[], Sweep::Never).is_empty());
    }
}

#[cfg(test)]
mod autostart_tests {
    use super::*;

    #[test]
    fn the_setting_is_honoured_when_the_binding_is_current() {
        assert_eq!(autostart_action(true, false, false, false), Autostart::Enable);
        assert_eq!(autostart_action(true, true, false, false), Autostart::Leave);
        assert_eq!(autostart_action(false, true, false, false), Autostart::Disable);
        assert_eq!(autostart_action(false, false, false, false), Autostart::Leave);
    }

    /// The rename bug: the entry exists, so nothing looks wrong, but it names a binary that
    /// is gone. Without this the app quietly stops starting at login for everyone upgrading.
    #[test]
    fn an_entry_written_for_the_old_binary_is_rewritten() {
        assert_eq!(autostart_action(true, true, false, true), Autostart::Rebind);
    }

    /// Whoever turned it off gets it off, however old their entry is — a stale binding is a
    /// reason to rewrite the entry, never to bring one back.
    #[test]
    fn a_stale_binding_never_revives_a_disabled_login_item() {
        assert_eq!(autostart_action(false, true, false, true), Autostart::Disable);
        assert_eq!(autostart_action(false, false, false, true), Autostart::Leave);
    }

    /// The report: turned off in Task Manager, back on after the next update. A veto there
    /// reads as "not enabled", so every one of these used to come out `Enable` — and
    /// `enable()` rewrites the very flag that was the player saying no.
    #[test]
    fn windows_own_startup_list_wins_over_the_setting() {
        assert_eq!(autostart_action(true, false, true, false), Autostart::Adopt);
        assert_eq!(autostart_action(true, false, true, true), Autostart::Adopt);
        // Belt and braces: a veto standing next to an entry that still reads as enabled.
        assert_eq!(autostart_action(true, true, true, false), Autostart::Adopt);
    }

    /// A veto is a reason to stop turning it on, never a reason to turn it on. With the
    /// setting already off there is nothing left to reconcile.
    #[test]
    fn a_veto_leaves_an_already_off_setting_alone() {
        assert_eq!(autostart_action(false, false, true, false), Autostart::Leave);
        assert_eq!(autostart_action(false, false, true, true), Autostart::Leave);
    }

    /// The twelve bytes Windows writes: `02 00…` while the app is allowed to start, `03 00`
    /// plus the FILETIME it was switched off once it isn't.
    #[test]
    fn the_startup_flag_is_read_off_its_tail() {
        assert!(!startup_flag_is_veto(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(startup_flag_is_veto(&[3, 0, 0, 0, 0x1e, 0x2b, 0x9c, 0x5f, 0x7a, 0x0d, 0xdc, 0x01]));
        // Too short to carry a timestamp, so there is nothing there that says "off".
        assert!(!startup_flag_is_veto(&[]));
        assert!(!startup_flag_is_veto(&[3, 0, 0, 0]));
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    /// The regression a tester's log caught: the clearance window was being parked in the
    /// tray on close, which left its label registered, so every handshake after the first
    /// failed to build a window and Retry silently did nothing for the rest of the session.
    ///
    /// Only the main window may park. Every transient window has to close for real.
    #[test]
    fn only_the_main_window_parks_in_the_tray() {
        assert!(parks_in_tray(MAIN_WINDOW));

        for transient in [
            mxb_fetch::WINDOW,
            shop_fetch::WINDOW,
            SHOP_LOGIN_WINDOW,
            HUB_LOGIN_WINDOW,
            hub_clearance::WINDOW,
            overlay::LABEL, // handled earlier by its own branch, but never by this one
        ] {
            assert!(
                !parks_in_tray(transient),
                "{transient} must be destroyed on close, not hidden — a stranded label \
                 makes it unopenable for the life of the process"
            );
        }
    }

    /// The security boundary. Both fetch windows run a *remote* origin — mxb-mods.com's own
    /// page in one, mxbikes-shop.com's signed-in page in the other — and their capabilities
    /// grant IPC, which `generate_handler!` commands are not gated by. So each gets exactly one
    /// call and nothing else; if this test ever goes green on a second command, script on those
    /// sites can drive that command.
    ///
    /// The shop window is the sharper case of the two: it is signed in as the user, and
    /// `shop_install` writes files.
    #[test]
    fn the_remote_fetch_windows_may_only_emit_their_result() {
        for remote in [mxb_fetch::WINDOW, shop_fetch::WINDOW] {
            assert!(ipc_allowed(remote, "plugin:event|emit"), "{remote}");

            for forbidden in [
                "create_config",
                "install_mod",
                "mods_state_delete",
                "get_config",
                "shop_install",
                "shop_logout",
                "commit_drop",
                "record_download",
                "clear_download_history",
                "plugin:shell|open",
                "plugin:dialog|open",
                "plugin:event|listen",
                "plugin:process|restart",
            ] {
                assert!(
                    !ipc_allowed(remote, forbidden),
                    "script on {remote}'s remote origin must not be able to call {forbidden}"
                );
            }
        }
    }

    /// ...and the guard must not touch anything else. The app's own windows keep whatever
    /// their capability files grant.
    #[test]
    fn the_apps_own_windows_are_unaffected_by_the_guard() {
        for label in [MAIN_WINDOW, overlay::LABEL, SHOP_LOGIN_WINDOW, HUB_LOGIN_WINDOW] {
            for command in ["create_config", "install_mod", "plugin:event|emit"] {
                assert!(ipc_allowed(label, command), "{label} / {command}");
            }
        }
    }
}

#[cfg(test)]
mod mesh_texture_tests {
    use super::{mesh_texture_names, paint_hints};

    /// The smallest thing `edf::embedded_textures` reads as a texture record: a
    /// null-terminated name, then the fields it validates by shape at a fixed offset.
    fn mesh_naming(texture: &str) -> Vec<u8> {
        const W_FROM_NAME: usize = 100;
        let mut b = vec![0u8; W_FROM_NAME];
        b[..texture.len()].copy_from_slice(texture.as_bytes());
        b.extend_from_slice(&64u32.to_le_bytes()); // width, from the fixed size set
        b.extend_from_slice(&64u32.to_le_bytes()); // height
        b.extend_from_slice(&[0u8; 16]); // digest
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&12u32.to_le_bytes()); // payload, counting the pad
        b.extend_from_slice(&[0u8; 8]); // pad
        b.extend_from_slice(&[1u8; 4]); // payload
        b
    }

    #[test]
    fn a_models_own_textures_come_from_its_mesh() {
        let root = std::env::temp_dir().join(format!("frost-mesh-tex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("MyBike");
        std::fs::create_dir_all(&dir).unwrap();
        // No paint here at all — the mesh is the only thing that can name `plastics`.
        std::fs::write(dir.join("model.edf"), mesh_naming("plastics")).unwrap();
        assert_eq!(mesh_texture_names(&dir), vec!["plastics".to_string()]);

        std::fs::create_dir_all(root.join("Bare")).unwrap();
        assert!(mesh_texture_names(&root.join("Bare")).is_empty(), "no mesh, nothing to say");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A sheet name that only exists because another paint misspelt one the model binds is
    /// not offered. The KTM 250 SX-F binds `plastics_n`; a paint beside it ships
    /// `plastics-n`, which the game asks for on no part of the bike, so painting it is work
    /// that cannot show up. Names the mesh never mentions at all are somebody else's sheet
    /// and are left alone.
    #[test]
    fn a_paints_misspelling_of_a_bound_sheet_is_not_offered() {
        let root = std::env::temp_dir().join(format!("frost-dead-sheet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bike = root.join("Bike");
        std::fs::create_dir_all(bike.join("paints")).unwrap();
        let mut mesh = mesh_naming("plastics");
        mesh.extend_from_slice(&mesh_naming("plastics_n"));
        std::fs::write(bike.join("model.edf"), mesh).unwrap();
        std::fs::write(
            bike.join("paints").join("Someone.pnt"),
            super::paint::encode(
                "Someone",
                &[
                    super::paint::PntTexture {
                        name: "plastics-n".into(),
                        width: 2,
                        height: 2,
                        rgba: vec![0; 16],
                    },
                    super::paint::PntTexture {
                        name: "tyres".into(),
                        width: 2,
                        height: 2,
                        rgba: vec![0; 16],
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();

        let names = paint_hints(&bike.join("paints"));
        assert!(names.iter().any(|n| n == "plastics_n"), "the bound spelling stays: {names:?}");
        assert!(!names.iter().any(|n| n == "plastics-n"), "the dead one goes: {names:?}");
        assert!(names.iter().any(|n| n == "tyres"), "a sheet of its own is not ours to drop: {names:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A model's own textures are offered where a paint for it goes, and nowhere else: the
    /// goggles beside it are a different file, painted from a different sheet.
    #[test]
    fn only_the_models_own_paints_folder_is_offered_the_mesh() {
        let root = std::env::temp_dir().join(format!("frost-mesh-hints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let helmet = root.join("Helmet");
        std::fs::create_dir_all(helmet.join("paints")).unwrap();
        std::fs::create_dir_all(helmet.join("goggles")).unwrap();
        std::fs::write(helmet.join("helmet.edf"), mesh_naming("shell")).unwrap();

        assert_eq!(paint_hints(&helmet.join("paints")), vec!["shell".to_string()]);
        assert!(paint_hints(&helmet.join("goggles")).is_empty(), "the shell is not a goggle sheet");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A packed model whose name carries a version number: `Fox Instinct 2.0 by Aeffertz`
    /// has no folder on disk at all, only the archive beside where one would be, and the
    /// dot in the name is not an extension to be replaced. Getting that wrong asked for
    /// `Fox Instinct 2.pkz`, and the Designer offered no sheet names for the boots — so
    /// nothing suggested `fox`, and a sheet named anything else paints nothing.
    #[test]
    fn a_packed_model_with_a_dot_in_its_name_still_names_its_sheets() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("frost-dotted-pkz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let packed = root.join("Fox Instinct 2.0 by Aeffertz.pkz");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&packed).unwrap());
            w.start_file::<_, ()>("boots.edf", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(&mesh_naming("fox")).unwrap();
            w.finish().unwrap();
        }

        // The destination the picker aims at: a `paints` folder under a model folder that
        // was never unpacked, which is where a paint for a packed mod has to go.
        let dest = root.join("Fox Instinct 2.0 by Aeffertz").join("paints");
        assert_eq!(paint_hints(&dest), vec!["fox".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `Rider+` and `Rider+RolledUp` ship `paints/` and `gloves/` empty on purpose: the kits
    /// installed under the stock profile are the ones meant to be worn on them, which is
    /// what `read_rider_paint_file` already does when it renders one. So the sheet names
    /// have to come from there too — otherwise painting a kit or a pair of gloves for such
    /// a profile starts with nothing to call the sheet, and a sheet named by guesswork
    /// binds to nothing.
    #[test]
    fn a_profile_that_ships_no_paints_borrows_the_stock_ones() {
        let root = std::env::temp_dir().join(format!("frost-stock-kit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let riders = root.join("riders");
        let mine = riders.join("Rider+");
        std::fs::create_dir_all(mine.join("paints")).unwrap();
        std::fs::create_dir_all(mine.join("gloves")).unwrap();

        let pnt = |name: &str| {
            crate::paint::encode(
                "Stock",
                &[crate::paint::PntTexture {
                    name: name.to_string(),
                    width: 4,
                    height: 4,
                    rgba: vec![0u8; 4 * 4 * 4],
                }],
            )
            .unwrap()
        };
        let stock = riders.join("default_mx");
        std::fs::create_dir_all(stock.join("paints")).unwrap();
        std::fs::create_dir_all(stock.join("gloves")).unwrap();
        std::fs::write(stock.join("paints").join("Kit.pnt"), pnt("rider")).unwrap();
        std::fs::write(stock.join("gloves").join("Gloves.pnt"), pnt("gloves")).unwrap();

        assert_eq!(paint_hints(&mine.join("paints")), vec!["rider".to_string()]);
        assert_eq!(
            paint_hints(&mine.join("gloves")),
            vec!["gloves".to_string()],
            "a gloves folder is never offered the mesh's names, so this is its only source",
        );
        // A profile with kits of its own is answered by those, not by the stock ones.
        std::fs::write(mine.join("paints").join("Mine.pnt"), pnt("rider_mine")).unwrap();
        assert_eq!(paint_hints(&mine.join("paints")), vec!["rider_mine".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole hint list for a real destination:
    /// `MXB_PAINT_DEST='…/mods/bikes/MX1OEM_2023_Husqvarna_FC_450/paints' \
    ///   cargo test paint_hints_from_env -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paint_hints_from_env() {
        let Ok(dir) = std::env::var("MXB_PAINT_DEST") else {
            eprintln!("set MXB_PAINT_DEST to run");
            return;
        };
        let t = std::time::Instant::now();
        let names = paint_hints(std::path::Path::new(&dir));
        eprintln!("{dir} expects {names:?} ({:?})", t.elapsed());
        assert!(!names.is_empty(), "a destination with a model behind it expects something");
    }
}

#[cfg(test)]
mod deep_link_tests {
    use super::enroll_code_from_link;

    #[test]
    fn reads_the_code_out_of_an_enroll_link() {
        assert_eq!(enroll_code_from_link("mxb://enroll?code=ABC-123").as_deref(), Some("ABC-123"));
        // A launcher that adds a trailing slash to the host must still work.
        assert_eq!(enroll_code_from_link("mxb://enroll/?code=xyz_9").as_deref(), Some("xyz_9"));
    }

    #[test]
    fn accepts_the_british_spelling_too() {
        // Whoever writes the invite link is a person, and the two spellings are a trap.
        assert_eq!(enroll_code_from_link("mxb://enroll?code=A1").as_deref(), Some("A1"));
    }

    #[test]
    fn finds_the_code_among_other_parameters() {
        assert_eq!(enroll_code_from_link("mxb://enroll?ref=discord&code=A1").as_deref(), Some("A1"));
    }

    #[test]
    fn ignores_links_that_are_not_the_enroll_route() {
        // Any page the player visits can open one of these, so anything unrecognised has
        // to be dropped rather than interpreted.
        for bad in [
            "mxb://install?code=A1",
            "mxb://enroll",
            "https://example.com/enroll?code=A1",
            "mxb://",
            "",
        ] {
            assert!(enroll_code_from_link(bad).is_none(), "{bad:?} must be ignored");
        }
    }

    #[test]
    fn refuses_a_code_that_is_not_a_plain_token() {
        // These would each go straight into the enroll field; none is a real invite code.
        for bad in [
            "mxb://enroll?code=",
            "mxb://enroll?code=a b",
            "mxb://enroll?code=../../etc",
            "mxb://enroll?code=%2E%2E",
            "mxb://enroll?code=<script>",
        ] {
            assert!(enroll_code_from_link(bad).is_none(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn refuses_an_absurdly_long_code() {
        let long = format!("mxb://enroll?code={}", "a".repeat(200));
        assert!(enroll_code_from_link(&long).is_none());
    }
}

#[cfg(test)]
mod grid_watch_tests {
    use super::GridWatch;

    /// The whole point: a rider who wasn't there a moment ago is a reason to pull.
    #[test]
    fn a_rider_who_joins_later_is_an_arrival() {
        let mut w = GridWatch::default();
        assert_eq!(w.arrivals("silver mx", ["Frost", "Nate"].into_iter()), 2, "the first look is all arrivals");
        assert_eq!(w.arrivals("silver mx", ["Frost", "Nate"].into_iter()), 0, "an unchanged grid pulls nothing");
        assert_eq!(w.arrivals("silver mx", ["Frost", "Nate", "Alice"].into_iter()), 1, "the newcomer is the trigger");
        assert_eq!(w.arrivals("silver mx", ["Frost", "Nate", "Alice"].into_iter()), 0);
    }

    /// A rider who leaves and comes back must not pull again — their paint is already here,
    /// and a grid where someone is rejoining repeatedly would pull on a loop.
    #[test]
    fn someone_returning_is_not_a_new_arrival() {
        let mut w = GridWatch::default();
        w.arrivals("silver mx", ["Frost", "Nate"].into_iter());
        assert_eq!(w.arrivals("silver mx", ["Frost"].into_iter()), 0, "Nate left");
        assert_eq!(w.arrivals("silver mx", ["Frost", "Nate"].into_iter()), 0, "and came back");
    }

    /// One rider must not read as two because the game spaced or capitalised their name
    /// differently between two reads of the block.
    #[test]
    fn a_name_is_folded_before_it_counts() {
        let mut w = GridWatch::default();
        assert_eq!(w.arrivals("silver mx", ["Frost"].into_iter()), 1);
        assert_eq!(w.arrivals("silver mx", ["  frost  ", "FROST"].into_iter()), 0);
    }

    /// An empty slot in the entry list is not a rider.
    #[test]
    fn blank_names_are_not_riders() {
        let mut w = GridWatch::default();
        assert_eq!(w.arrivals("silver mx", ["", "   ", "Frost"].into_iter()), 1);
    }

    /// Changing server is a new grid. Carrying the last one's names over would mean the
    /// riders on the new server were silently treated as already pulled for.
    #[test]
    fn another_server_starts_the_grid_again() {
        let mut w = GridWatch::default();
        assert_eq!(w.arrivals("silver mx", ["Frost", "Nate"].into_iter()), 2);
        assert_eq!(w.arrivals("other server", ["Frost", "Nate"].into_iter()), 2, "same names, new grid");
        assert_eq!(w.arrivals("other server", ["Frost", "Nate"].into_iter()), 0);
    }
}

#[cfg(test)]
mod live_look_tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("frost-look-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    /// A profile wearing a bike paint and a helmet paint, plus a helmet model and a tyre
    /// set that are emphatically not paints.
    fn fixture(root: &Path) -> AppConfig {
        touch(&root.join("mods/bikes/KTM450/paints/RedBud.pnt"));
        touch(&root.join("mods/bikes/KTM450/paints/Southwick.pnt")); // owned, not worn
        touch(&root.join("mods/rider/helmets/AGV/AGV.pkz"));
        touch(&root.join("mods/rider/helmets/AGV/paints/Blue.pnt"));
        touch(&root.join("mods/tyres/oem_mx.pkz"));
        touch(&root.join("profiles/Frost/profile.ini"));
        std::fs::write(
            root.join("profiles/Frost/profile.ini"),
            "[info]\nbikeid = KTM450\n\n\
             [paint]\nKTM450 = RedBud\n\n\
             [helmet]\nKTM450 = AGV\n\n\
             [helmet_paint]\nKTM450 = Blue\n\n\
             [tyres]\nKTM450 = oem_mx\n",
        )
        .unwrap();
        AppConfig {
            mods_path: root.to_string_lossy().into_owned(),
            profiles_path: root.join("profiles").to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    /// The set the watcher is pointed at: every `.pnt` the active bike is wearing, and
    /// nothing else. A helmet's mesh and a tyre archive are resolved by the same plan and
    /// must not become watches — re-running the game's loader can't change a mesh, so a
    /// watch on one is a thread started for nothing.
    #[test]
    fn only_the_paints_the_active_bike_is_wearing_are_watched() {
        let root = tmp("worn");
        let cfg = fixture(&root);

        let mut names: Vec<String> = worn_paints(&cfg)
            .iter()
            .map(|p| Path::new(p).file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();

        assert_eq!(names, vec!["Blue.pnt".to_string(), "RedBud.pnt".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A profile that names a paint nobody installed leaves nothing to watch, rather than
    /// leaving the watcher pointed at the last look the rider was in.
    #[test]
    fn a_look_that_resolves_to_nothing_watches_nothing() {
        let root = tmp("empty");
        let cfg = fixture(&root);
        std::fs::write(
            root.join("profiles/Frost/profile.ini"),
            "[info]\nbikeid = KTM450\n\n[paint]\nKTM450 = A Paint Nobody Has\n",
        )
        .unwrap();

        assert!(worn_paints(&cfg).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// No profile, no bike, no `profile.ini` — every one of these is an ordinary state for
    /// a fresh install to be in, and none of them may panic on the way to an empty answer.
    #[test]
    fn an_unreadable_look_is_an_empty_answer_not_a_panic() {
        let root = tmp("unreadable");
        std::fs::create_dir_all(root.join("profiles")).unwrap();
        let cfg = AppConfig {
            mods_path: root.to_string_lossy().into_owned(),
            profiles_path: root.join("profiles").to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert!(worn_paints(&cfg).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The cooldown is what keeps a burst of saves — or half a grid's paints landing at
    /// once — from becoming a queue of threads started inside the running game.
    #[test]
    fn a_burst_gets_one_refresh() {
        assert!(live_look_cooldown_passed(), "the first one always goes");
        for _ in 0..5 {
            assert!(!live_look_cooldown_passed(), "the rest fold into it");
        }
    }
}

#[cfg(test)]
mod track_install_tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("frost-trackdest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_track_installs_into_the_mods_tree_not_the_install_dir() {
        let user = tmp("user");
        std::fs::create_dir_all(user.join("mods").join("tracks")).unwrap();
        let cfg = AppConfig {
            mods_path: user.to_string_lossy().into_owned(),
            // Deliberately set, and deliberately not where the track goes.
            game_path: "/somewhere/else/MX Bikes".into(),
            ..Default::default()
        };
        assert_eq!(
            track_install_dir(&cfg).unwrap(),
            user.join("mods").join("tracks")
        );
        let _ = std::fs::remove_dir_all(&user);
    }

    /// `mxbikes.ini` lets a player point the game at `C:\mods`, and then `mods_path` *is*
    /// the tree — joining `mods` on by hand would write to `C:\mods\mods\tracks`.
    #[test]
    fn a_relocated_tree_is_the_mods_folder_itself() {
        let tree = tmp("tree");
        for d in ["bikes", "tracks", "rider"] {
            std::fs::create_dir_all(tree.join(d)).unwrap();
        }
        let cfg = AppConfig {
            mods_path: tree.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert_eq!(track_install_dir(&cfg).unwrap(), tree.join("tracks"));
        let _ = std::fs::remove_dir_all(&tree);
    }

    /// With no folder configured the old code joined onto an empty `game_path` and wrote
    /// `mods/tracks` relative to the working directory — a track installed into thin air.
    #[test]
    fn no_configured_folder_is_an_error_not_a_relative_path() {
        let cfg = AppConfig { mods_path: String::new(), ..Default::default() };
        let err = track_install_dir(&cfg).unwrap_err();
        assert!(err.contains("configured"), "{err}");
    }
}

/// Two cases that span both crates: the viewer builds the model, but resolving a model swap
/// and repairing a buried gear package are the manager's business. They live here rather
/// than in `mxb_core::viewer`'s own tests because that crate cannot see these modules.
#[cfg(test)]
mod viewer_crossing_tests {
    use crate::modelswap;
    use crate::gearrepair;
    use mxb_core::viewer::*;
    use std::path::{Path, PathBuf};

    fn copy_tree(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap().flatten() {
            let (from, to) = (e.path(), dst.join(e.file_name()));
            if from.is_dir() {
                copy_tree(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }

    fn shape(m: &BikeModel) -> Vec<(String, Vec<(String, Option<String>)>)> {
        m.nodes
            .iter()
            .map(|n| {
                (
                    n.name.clone(),
                    n.submeshes.iter().map(|s| (s.name.clone(), s.texture.clone())).collect(),
                )
            })
            .collect()
    }

    fn write_pkz(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    fn named<'a>(files: &'a [(String, Vec<u8>)], base: &str) -> Option<&'a [u8]> {
        files
            .iter()
            .find(|(n, _)| {
                n.rsplit('/').next().unwrap_or(n).eq_ignore_ascii_case(base)
            })
            .map(|(_, d)| d.as_slice())
    }

    #[test]
    fn a_swap_preview_keeps_the_packed_bike_under_it() {
        let root: PathBuf =
            std::env::temp_dir().join(format!("frost-packed-under-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mp = root.to_str().unwrap();
        let bike = "MX1OEM_2023_KTM_450_SX-F";
        let bikes = crate::library::mods_subdir(mp, "mods/bikes");
        std::fs::create_dir_all(bikes.join(bike)).unwrap();
        write_pkz(
            &bikes.join(format!("{bike}.pkz")),
            &[
                (&format!("{bike}/model.edf"), b"packed mesh"),
                (&format!("{bike}/gfx.cfg"), b"packed gfx"),
                (&format!("{bike}/chassis.hrc"), b"packed hrc"),
                (&format!("{bike}/{bike}.geom"), b"packed geom"),
                (&format!("{bike}/paints/stock.pnt"), b"packed paint"),
            ],
        );
        let variant = bikes.join(bike).join(modelswap::LIB_DIR).join("Factory");
        std::fs::create_dir_all(&variant).unwrap();
        std::fs::write(variant.join("model.edf"), b"swap mesh").unwrap();

        let set = modelswap::preview_set(mp, bike, "Factory").expect("preview set");
        let files = mxb_core::viewer::gather_preview_files(&set).expect("preview files");

        // The bike comes through underneath...
        for base in ["gfx.cfg", "chassis.hrc", &format!("{bike}.geom"), "stock.pnt"] {
            assert!(named(&files, base).is_some(), "{base} missing from {:?}",
                files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>());
        }
        // ...and the swap's mesh, not the packed one, is what gets drawn.
        assert_eq!(named(&files, "model.edf"), Some(&b"swap mesh"[..]));
        assert_eq!(
            files.iter().filter(|(n, _)| crate::bikefiles::is_mesh(n)).count(),
            1,
            "the packed mesh must be replaced, not added alongside",
        );
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    #[ignore]
    fn a_buried_package_loads_once_the_repair_has_raised_it() {
        let Ok(path) = std::env::var("MXB_REAL_GEAR") else {
            eprintln!("set MXB_REAL_GEAR to an installed gear .pkz to run");
            return;
        };
        let src = std::path::Path::new(&path);
        assert!(src.is_file(), "MXB_REAL_GEAR must be a .pkz file for this one");
        let name = src.file_name().unwrap().to_string_lossy().into_owned();

        let root = std::env::temp_dir().join(format!("frost-buried-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let helmets = root.join("mods/rider/helmets");
        let buried = helmets.join("shop-44");
        std::fs::create_dir_all(&buried).unwrap();
        std::fs::hard_link(src, buried.join(&name))
            .or_else(|_| std::fs::copy(src, buried.join(&name)).map(|_| ()))
            .expect("stage the package");

        // Where the install put it: the folder is what the picker offers, and it has no mesh.
        let buried_load = mxb_core::viewer::load_gear_model_blocking(
            buried.to_string_lossy().into_owned(),
            "helmet".into(),
            None,
            None,
            false,
            false,
            Vec::new(),
        );
        let err = buried_load.err().expect("a buried package must not load");
        assert!(err.contains("no gear mesh found"), "unexpected error: {err}");

        let plans = gearrepair::plan(root.to_str().unwrap());
        assert_eq!(plans.len(), 1, "the burial is found");
        let moved = gearrepair::apply_one(root.to_str().unwrap(), &plans[0].id).unwrap();
        assert_eq!(moved, 1);

        let raised = helmets.join(&name);
        assert!(raised.is_file(), "the package is in the area root");
        let part = mxb_core::viewer::load_gear_model_blocking(
            raised.to_string_lossy().into_owned(),
            "helmet".into(),
            None,
            None,
            false,
            false,
            Vec::new(),
        )
        .expect("load the raised package");
        eprintln!(
            "{name} -> {} node(s), textures {:?}",
            part.nodes.len(),
            part.textures.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        assert!(!part.nodes.is_empty(), "it draws something");
        assert!(!part.textures.is_empty(), "and it is textured");
        let _ = std::fs::remove_dir_all(&root);
    }
    /// The contract behind the Locker's 3D preview: what it shows is what applying the
    /// swap would give you. Run against a **real** bike, because the in-memory overlay has
    /// to pick up the `.hrc`s, `.geom` and textures the root keeps while the mesh comes
    /// from the variant folder — a synthetic bike can't exercise that chain.
    ///
    /// MXB_REAL_BIKES=~/Projects/PiBoSo/"MX Bikes" \
    ///   cargo test preview_matches_the_applied_swap -- --ignored --nocapture
    #[test]
    #[ignore]
    fn preview_matches_the_applied_swap() {
        let Ok(src_root) = std::env::var("MXB_REAL_BIKES") else {
            eprintln!("set MXB_REAL_BIKES to the MX Bikes folder to run");
            return;
        };
        let src_bikes = Path::new(&src_root).join("mods").join("bikes");
        let Some((bike, src_dir)) = std::fs::read_dir(&src_bikes)
            .expect("read bikes")
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
            .find(|(_, p)| p.is_dir() && crate::bikefiles::dir_has_mesh(p))
        else {
            eprintln!("no extracted bike with a mesh found");
            return;
        };
        eprintln!("using real bike: {bike}");

        let root: PathBuf =
            std::env::temp_dir().join(format!("frost-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mp = root.to_str().unwrap();
        let dst = crate::library::mods_subdir(mp, "mods/bikes").join(&bike);
        copy_tree(&src_dir, &dst);

        // A realistic swap set: the bike's own mesh under a variant name, nothing else.
        // The preview must find everything else at the root.
        let mesh = std::fs::read_dir(&dst)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .find(|f| crate::bikefiles::is_mesh(f))
            .expect("mesh");
        let variant = dst.join("FrostMod Models").join("Factory");
        std::fs::create_dir_all(&variant).unwrap();
        std::fs::copy(dst.join(&mesh), variant.join(&mesh)).unwrap();

        let set = modelswap::preview_set(mp, &bike, "Factory").expect("preview set");
        eprintln!("keeps {:?} + brings {:?}", set.root_keep, set.variant_files);
        let files = mxb_core::viewer::gather_preview_files(&set).expect("preview files");
        let previewed = mxb_core::viewer::build_bike_model(
            "preview",
            "preview-test".into(),
            files,
            mxb_core::viewer::installed_paints(&set.bike_dir),
            // What `load_bike_model_blocking` derives for the bike it's compared against.
            Some(crate::library::mods_subdir(mp, "mods/tyres")),
            None,
            std::time::Instant::now(),
        )
        .expect("preview builds");
        assert!(!previewed.nodes.is_empty(), "the preview drew nothing");

        modelswap::apply_model_swap(mp, &bike, "Factory").expect("swap applies");
        let applied = mxb_core::viewer::load_bike_model_blocking(dst.to_string_lossy().to_string(), None)
            .expect("the swapped bike loads");

        assert_eq!(shape(&previewed), shape(&applied), "preview differs from the real swap");
        assert_eq!(
            previewed.paints.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            applied.paints.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "the preview offers different paints",
        );
        let _ = std::fs::remove_dir_all(&root);
    }
    /// Stock parks every loose override, so there'd be nothing to draw if the packed model
    /// didn't come through underneath. Needs a real `.pkz` — that's the whole mechanism.
    ///
    /// MXB_REAL_BIKES=~/Projects/PiBoSo/"MX Bikes" \
    ///   cargo test preview_of_stock_shows_the_packed_model -- --ignored --nocapture
    #[test]
    #[ignore]
    fn preview_of_stock_shows_the_packed_model() {
        let Ok(src_root) = std::env::var("MXB_REAL_BIKES") else {
            eprintln!("set MXB_REAL_BIKES to the MX Bikes folder to run");
            return;
        };
        let src_bikes = Path::new(&src_root).join("mods").join("bikes");
        // A bike that is both extracted *and* packed — the loose files hide a packed model.
        let Some((bike, src_dir)) = std::fs::read_dir(&src_bikes)
            .expect("read bikes")
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
            .find(|(_, p)| {
                p.is_dir()
                    && crate::bikefiles::dir_has_mesh(p)
                    && crate::library::sibling_pkz(p).exists()
            })
        else {
            eprintln!("no bike with both a loose mesh and a .pkz found");
            return;
        };
        eprintln!("using real bike: {bike}");

        let root: PathBuf =
            std::env::temp_dir().join(format!("frost-preview-stock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mp = root.to_str().unwrap();
        let bikes = crate::library::mods_subdir(mp, "mods/bikes");
        copy_tree(&src_dir, &bikes.join(&bike));
        std::fs::copy(crate::library::sibling_pkz(&src_dir), bikes.join(format!("{bike}.pkz")))
            .unwrap();

        let set = modelswap::preview_set(mp, &bike, "Stock").expect("preview set");
        eprintln!("keeps {:?}", set.root_keep);
        assert!(
            !set.root_keep.iter().any(|f| crate::bikefiles::is_mesh(f)),
            "Stock must park every loose mesh",
        );
        let files = mxb_core::viewer::gather_preview_files(&set).expect("preview files");
        let m = mxb_core::viewer::build_bike_model(
            "stock preview",
            "stock-preview-test".into(),
            files,
            mxb_core::viewer::installed_paints(&set.bike_dir),
            Some(crate::library::mods_subdir(mp, "mods/tyres")),
            None,
            std::time::Instant::now(),
        )
        .expect("stock preview builds");
        assert!(!m.nodes.is_empty(), "the packed model didn't come through");
        let _ = std::fs::remove_dir_all(&root);
    }
}


/// The server browser's paint-sync badge.
///
/// Presence is keyed two ways for one server, so every one of these is about a row being
/// looked up twice and the two answers reconciled into one number.
#[cfg(test)]
mod paint_sync_badge_tests {
    use super::*;

    fn counts(pairs: &[(&str, u32)]) -> std::collections::HashMap<String, u32> {
        pairs.iter().map(|(k, n)| ((*k).to_string(), *n)).collect()
    }

    fn row(name: &str, address: &str) -> ServerRow {
        ServerRow { name: name.into(), address: address.into() }
    }

    /// A community server nobody registered: its riders are recorded under the folded name,
    /// because that is the only key every rider in the session can compute.
    #[test]
    fn a_server_is_found_by_its_folded_name() {
        let counts = counts(&[("mxb hub public", 4)]);
        let got = match_presence(&counts, &[], &[row("MXB  Hub   Public", "1.2.3.4:54210")]);
        assert_eq!(got.get("1.2.3.4:54210"), Some(&4), "{got:?}");
    }

    /// A rider who joined through the app reports the address, not the name — so a row whose
    /// name we have never seen still has to resolve.
    #[test]
    fn a_server_is_found_by_its_address() {
        let counts = counts(&[("1.2.3.4:54210", 2)]);
        let got = match_presence(&counts, &[], &[row("Something Else Entirely", "1.2.3.4:54210")]);
        assert_eq!(got.get("1.2.3.4:54210"), Some(&2), "{got:?}");
    }

    /// Both keys holding riders is the ordinary case in a mixed session, and the two sets
    /// overlap by an unknowable amount. Summing them would put four riders on a server that
    /// might hold two.
    #[test]
    fn two_keys_are_the_larger_not_the_sum() {
        let counts = counts(&[("1.2.3.4:54210", 2), ("night league", 3)]);
        let got = match_presence(&counts, &[], &[row("Night League", "1.2.3.4:54210")]);
        assert_eq!(got.get("1.2.3.4:54210"), Some(&3), "{got:?}");
    }

    /// A registered server's riders are recorded under its registry id, which is neither its
    /// address nor its name — that is what the registry is fetched for.
    #[test]
    fn a_registered_server_resolves_through_its_id() {
        let registry = vec![paintsync::RegisteredServer {
            id: "mxb-eu-1".into(),
            name: "MXB EU 1".into(),
            region: "eu".into(),
            address: "5.6.7.8:54210".into(),
        }];
        let counts = counts(&[("mxb-eu-1", 6)]);
        let got = match_presence(&counts, &registry, &[row("MXB EU 1", "5.6.7.8:54210")]);
        assert_eq!(got.get("5.6.7.8:54210"), Some(&6), "{got:?}");
    }

    /// An empty entry is not a badge that says nothing, it is no badge. The frontend keys on
    /// presence in the map, so a zero here would draw one.
    #[test]
    fn a_server_with_nobody_on_it_is_absent() {
        let counts = counts(&[("somewhere else", 3)]);
        let got = match_presence(&counts, &[], &[row("Quiet Server", "9.9.9.9:54210")]);
        assert!(got.is_empty(), "{got:?}");
    }
}
