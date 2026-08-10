// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bikefiles;
mod bikeswap;
mod bundle;
mod cfg;
mod config;
mod cookie_session;
mod dropzone;
mod edf;
mod frostmod;
mod frostmod_manage;
mod game;
mod gameproc;
mod gearrepair;
mod imgcache;
mod install;
mod library;
mod linkwalk;
mod lru;
mod modelswap;
mod mods;
mod modstate;
mod modwatch;
mod profilewatch;
mod mxb_fetch;
mod mxb_session;
mod overlay;
mod paint;
mod paintstudio;
mod pkz;
#[cfg(sidecar)]
mod sidecar;
mod presets;
mod paintsync;
mod reshade;
mod servers;
mod shop_catalog_session;
mod shop_credentials;
mod shop_fetch;
mod shop_installed;
mod shop_session;
mod soundmods;
mod texstore;
mod upload;
mod vcruntime;

use config::AppConfig;
use frostmod::ReloadOutcome;
use frostmod_manage::{FrostmodProcess, FrostmodStatus, InstallReport};
use library::InstalledMod;
use modwatch::ModWatcher;
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
const MAIN_WINDOW: &str = "main";

/// The shop login WebView, opened on demand and closed once the session is captured.
const SHOP_LOGIN_WINDOW: &str = "shop-login";

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
        return Ok(reshade::status(&cfg.install_dir())
            .presets
            .into_iter()
            .map(|p| library::LibraryEntry {
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
    library::scan_library(&cfg.mods_path, &subpath, &sound_bikes, cfg.game()).map_err(|e| format!("{e:#}"))
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

/// Gear areas holding a model the game can't reach, because its files sit loose in the area
/// root instead of in a folder. See [`gearrepair`] for how that happened and what moves.
#[tauri::command]
async fn scan_gear_repairs(app: tauri::AppHandle) -> Result<Vec<gearrepair::GearRepair>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(gearrepair::plan(&cfg.mods_path))
    })
    .await
    .map_err(|e| format!("scan_gear_repairs task failed: {e}"))?
}

/// Gather one area's loose content into a folder of its own. Returns how many entries moved.
#[tauri::command]
async fn repair_gear_area(app: tauri::AppHandle, area: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        gearrepair::apply_one(&cfg.mods_path, &area).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("repair_gear_area task failed: {e}"))?
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
    /// Model swaps only (`None` for sound). `live_refresh` re-runs the *customization*
    /// loader, which reloads paints/gear but never the mesh — the model needs FrostMod
    /// to re-apply the bike. See `frostmod::signal_refresh_model`.
    model_refresh: Option<frostmod::CommandOutcome>,
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

/// Ask FrostMod to re-apply `bike` so a just-swapped model shows live. `None` when
/// instant refresh is off — the same switch that gates `live_refresh`, since both
/// reach into the running game.
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
    modelswap::apply_model_swap(&cfg.mods_path, &bike, &target).map_err(|e| format!("{e:#}"))?;
    // Make a bound sound travel with the model (case 2); independent sounds are left
    // untouched (case 1). Best-effort — the model swap itself already succeeded.
    if let Err(e) = soundmods::reconcile_after_model_swap(&cfg.mods_path, &bike, &prev, &target) {
        eprintln!("sound reconcile after model swap failed: {e:#}");
    }
    let content_reload = frostmod::signal_reload();
    // Ask FrostMod to re-apply the bike so the new model shows in the garage without a
    // class switch away-and-back. Only acts if `bike` is the selected one (decided
    // inside FrostMod, which is the only side that knows). Gated on the same
    // instant-refresh setting as the look refresh — both poke the live game.
    let model_refresh = model_refresh_cmd(&app, cfg.instant_refresh, &bike);
    Ok(SwapApplyOutcome {
        content_reload,
        game_running: gameproc::is_game_running(),
        live_refresh: live_refresh(cfg.instant_refresh),
        model_refresh,
    })
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
        })
    })
    .await
    .map_err(|e| format!("apply_sound_swap task failed: {e}"))?
}

/// The ReShade card's whole state, read fresh from the game folder — see [`reshade`].
#[tauri::command]
async fn reshade_status(app: tauri::AppHandle) -> Result<reshade::Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(reshade::status(&cfg.install_dir()))
    })
    .await
    .map_err(|e| format!("reshade_status task failed: {e}"))?
}

#[tauri::command]
async fn apply_reshade_preset(
    app: tauri::AppHandle,
    name: String,
) -> Result<ReshadeApplyOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        reshade::apply(&cfg.install_dir(), &name).map_err(|e| format!("{e:#}"))?;
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
        reshade::delete(&cfg.install_dir(), &name).map_err(|e| format!("{e:#}"))
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

#[tauri::command]
async fn get_pkz_meta(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_meta_blocking(app, path))
        .await
        .map_err(|e| format!("get_pkz_meta task failed: {e}"))?
}

fn get_pkz_meta_blocking(app: tauri::AppHandle, path: String) -> Result<pkz::PkzMeta, String> {
    pkz::read_meta_cached(&app, &path).map_err(|e| format!("{e:#}"))
}

/// Metadata for many mods at once, but only for the ones already cached — `None` marks
/// an entry the caller still has to request individually.
///
/// The Library asks for this first so a known collection paints in a single round trip
/// instead of one request (and one archive read) per card.
#[tauri::command]
async fn get_pkz_meta_cached(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<Option<pkz::PkzMeta>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| pkz::read_meta_if_cached(&app, p))
            .collect()
    })
    .await
    .map_err(|e| format!("get_pkz_meta_cached task failed: {e}"))
}

#[tauri::command]
async fn get_pkz_preview(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || get_pkz_preview_blocking(path))
        .await
        .map_err(|e| format!("get_pkz_preview task failed: {e}"))?
}

fn get_pkz_preview_blocking(path: String) -> Result<Option<String>, String> {
    pkz::read_preview(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
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

fn unpack_paint_blocking(path: String) -> Result<Vec<paint::PaintTexture>, String> {
    let t0 = std::time::Instant::now();
    // Path *and* mtime, as the bike cache does, so a paint re-saved under the same name misses.
    let key = bike_cache_key(&path);
    if let Some(t) = paint_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        log::info!("unpack_paint {path}: cache hit ({:?})", t0.elapsed());
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
        // texture store. The evicted paint's go with it; nothing else holds those tokens.
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
        .join("MXB App")
        .join("Paint Templates")
}

/// The texture names a destination's existing paints supply.
///
/// A paint binds by name: call a sheet `livery` and it lands on the bodywork that asked for
/// `livery`, call it `my_livery` and it lands nowhere. Nothing in a `.pnt` says which names
/// a model wants, but the paints already installed for that model do — read from their
/// headers, so this costs no pixels.
#[tauri::command]
async fn paint_studio_hints(app: tauri::AppHandle, rel: String) -> Result<Vec<String>, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let rel = rel.replace('\\', "/").trim_matches('/').to_string();
        let dir = library::mods_subdir(&cfg.mods_path, &format!("mods/{rel}"));
        let mut names: Vec<String> = Vec::new();
        fn add(names: &mut Vec<String>, bytes: &[u8]) {
            let Ok(found) = paint::texture_names_any(bytes) else { return };
            for n in found {
                if !n.is_empty() && !names.iter().any(|s| s.eq_ignore_ascii_case(&n)) {
                    names.push(n);
                }
            }
        }

        // A handful is plenty: paints for one model overwhelmingly supply the same names,
        // and this runs every time the destination changes.
        const SAMPLE: usize = 8;
        let mut seen = 0usize;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if seen >= SAMPLE
                    || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pnt"))
                {
                    continue;
                }
                if let Ok(bytes) = std::fs::read(&p) {
                    add(&mut names, &bytes);
                    seen += 1;
                }
            }
        }
        // Nothing installed loose: the model may be packed, and its own paints are the
        // same evidence. `<Model>.pkz` sits beside the `<Model>` folder this destination
        // lives in — for a bike as much as for a helmet.
        if names.is_empty() {
            if let (Some(sub), Some(model_dir)) = (dir.file_name(), dir.parent()) {
                let pkz = model_dir.with_extension("pkz");
                let tail = format!("/{}/", sub.to_string_lossy().to_ascii_lowercase());
                if pkz.is_file() {
                    let want = |n: &str| {
                        let n = n.replace('\\', "/").to_ascii_lowercase();
                        n.contains(&tail) && n.ends_with(".pnt")
                    };
                    let packed = pkz::read_selected(&pkz, want).unwrap_or_default();
                    for (_, bytes) in packed.iter().take(SAMPLE) {
                        add(&mut names, bytes);
                    }
                }
            }
        }
        names.sort_by_key(|n| n.to_lowercase());
        Ok(names)
    })
    .await
    .map_err(|e| format!("paint_studio_hints task failed: {e}"))?
}

/// Raw RGBA for a texture the viewer was handed a token for.
///
/// Returns an `ipc::Response`, which travels as `application/octet-stream` and lands in the
/// webview as an `ArrayBuffer` — the pixels are never encoded, base64'd, or parsed as JSON
/// on the way. The frontend feeds the buffer straight to a `THREE.DataTexture`. `async`
/// keeps the copy off the main thread, as with every other command here.
#[tauri::command]
async fn texture_bytes(token: String) -> tauri::ipc::Response {
    tauri::ipc::Response::new(texstore::bytes_or_missing(&token))
}

#[tauri::command]
async fn unpack_pkz(path: String, out_dir: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || unpack_pkz_blocking(path, out_dir))
        .await
        .map_err(|e| format!("unpack_pkz task failed: {e}"))?
}

fn unpack_pkz_blocking(path: String, out_dir: String) -> Result<Vec<String>, String> {
    pkz::extract(std::path::Path::new(&path), std::path::Path::new(&out_dir))
        .map_err(|e| format!("{e:#}"))
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikePaint {
    name: String,
    textures: Vec<paint::PaintTexture>,
    changes_preview: bool,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikeModel {
    nodes: Vec<edf::EdfNode>,
    paints: Vec<BikePaint>,
}

impl BikeModel {
    /// Every texture token the model holds, so evicting it can free the pixels too.
    fn tokens(&self) -> Vec<String> {
        self.paints
            .iter()
            .flat_map(|p| p.textures.iter().map(|t| t.token.clone()))
            .collect()
    }
}

/// Bikes are big (geometry plus every paint's pixels), so hold only the few most recent.
const BIKE_CACHE_CAP: usize = 3;

fn bike_cache() -> &'static std::sync::Mutex<lru::Lru<BikeModel>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<lru::Lru<BikeModel>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(BIKE_CACHE_CAP)))
}

fn bike_cache_key(source: &str) -> String {
    let mtime = std::fs::metadata(source)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{source}:{mtime}")
}

#[tauri::command]
async fn load_bike_model(source: String) -> Result<BikeModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_bike_model_blocking(source))
        .await
        .map_err(|e| format!("load_bike_model task failed: {e}"))?
}

fn load_bike_model_blocking(source: String) -> Result<BikeModel, String> {
    let t0 = std::time::Instant::now();
    let key = bike_cache_key(&source);
    if let Some(m) = bike_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        log::info!("load_bike_model {source}: cache hit ({:?})", t0.elapsed());
        return Ok(m);
    }

    let files = gather_bike_files(std::path::Path::new(&source)).map_err(|e| format!("{e:#}"))?;
    let installed = installed_paints(std::path::Path::new(&source));
    let t_read = t0.elapsed();

    let mut nodes = Vec::new();
    // Every mesh the bike ships, by file name — usually just `model.edf`, but a bike can
    // carry one per part. Which are actually used is decided by the `.hrc`s below.
    let mut edfs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut geom: Option<&Vec<u8>> = None;
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut tga_jobs: Vec<(String, &[u8])> = Vec::new();
    let mut pnt_jobs: Vec<(String, &[u8], bool)> = Vec::new();
    for (name, data) in &files {
        let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
        if bn.ends_with(".edf") {
            edfs.insert(bn.clone(), data);
        } else if bn.ends_with(".geom") {
            geom = Some(data);
        } else if bn.ends_with("gfx.cfg") {
            gfx_bytes = Some(data);
        } else if let Some(stem) = bn.strip_suffix(".hrc") {
            let stem = stem.rsplit("__").next().unwrap_or(stem);
            hrcs.insert(stem.to_string(), data);
        } else if let Some(stem) = bn.strip_suffix(".tga") {
            // Lowercased stem — the frontend matches textures case-insensitively.
            tga_jobs.push((stem.to_string(), data.as_slice()));
        } else if bn.ends_with(".pnt") {
            pnt_jobs.push((paint_display_name(&bn), data.as_slice(), true));
        }
    }

    let gfx = gfx_bytes.map(|b| cfg::parse_gfx(b)).unwrap_or_default();
    // Group each part's level0 node under the mesh its `.hrc` names. Bikes that point
    // every part at one `model.edf` collapse to a single group — the original path.
    let mut scenes: Vec<(String, Vec<String>)> = Vec::new();
    let mut node_part: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Fixed part order — `gfx` is a map, and node order must not shuffle between runs.
    for part in cfg::GFX_PARTS {
        let Some(gp) = gfx.get(part) else { continue };
        let Some(hrc_file) = gp.hrc.as_deref() else { continue };
        let stem = hrc_file.trim_end_matches(".hrc").trim_end_matches(".HRC");
        let Some(bytes) = hrcs.get(&stem.to_ascii_lowercase()) else {
            log::warn!("[viewer] gfx.cfg part '{part}' wants {hrc_file}, which the bike doesn't ship");
            continue;
        };
        let hrc = cfg::parse(bytes);
        let Some(node) = cfg::hrc_level0(&hrc, stem) else { continue };
        let scene = cfg::hrc_level0_scene(&hrc)
            .map(|s| s.replace('\\', "/"))
            .and_then(|s| s.rsplit('/').next().map(str::to_ascii_lowercase))
            .unwrap_or_else(|| "model.edf".to_string());
        node_part.insert(node.to_ascii_lowercase(), part.to_string());
        match scenes.iter_mut().find(|(f, _)| *f == scene) {
            Some((_, level0)) => level0.push(node),
            None => scenes.push((scene, vec![node])),
        }
    }

    // Parse each referenced mesh and bind its textures against *its own* bytes: a
    // submesh's material index selects from that file's texture pool, so a part must
    // never be bound through another file's pool.
    let mut used: Vec<&Vec<u8>> = Vec::new();
    for (file, level0) in &scenes {
        let Some(data) = edfs.get(file) else {
            log::warn!("[viewer] an .hrc wants {file}, which the bike doesn't ship");
            continue;
        };
        let mut part_nodes = edf::parse_with_levels(data, level0);
        bind_textures(&mut part_nodes, data, &gfx, &node_part);
        nodes.append(&mut part_nodes);
        used.push(data);
    }
    // No gfx.cfg/.hrc to go on (or none of it resolved) — fall back to the bike's base
    // mesh and let the parser's own level0 heuristic pick the parts.
    if nodes.is_empty() {
        if let Some(data) = base_edf(&edfs) {
            nodes = edf::parse_with_levels(data, &[]);
            bind_textures(&mut nodes, data, &gfx, &node_part);
            used.push(data);
        }
    }
    for (fname, data) in &installed {
        pnt_jobs.push((paint_display_name(fname), data.as_slice(), false));
    }
    if let Some(g) = geom {
        if !edf::assemble_bike(&mut nodes, g) {
            eprintln!("[viewer] .geom present but missing mount points — parts unassembled");
        }
    } else if !nodes.is_empty() {
        eprintln!("[viewer] no .geom alongside the mesh — parts unassembled");
    }
    edf::to_right_handed(&mut nodes);
    let t_parse = t0.elapsed();

    let mut base: Vec<paint::PaintTexture> = tga_jobs
        .par_iter()
        .filter_map(|(stem, data)| paint::decode_image(stem, data))
        .collect();
    // Textures embedded in the meshes actually shown. Parts often share a name (each
    // file embeds the plastics it needs), so keep the first of each.
    let mut seen: std::collections::HashSet<String> =
        base.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    for data in &used {
        for tex in paint::extract_edf_textures(data) {
            if seen.insert(tex.name.to_ascii_lowercase()) {
                base.push(tex);
            }
        }
    }
    let mut paints: Vec<(BikePaint, bool)> = pnt_jobs
        .par_iter()
        .filter_map(|(name, data, shipped)| {
            paint::decode_any(data).ok().map(|pnt| {
                (
                    BikePaint {
                        name: name.clone(),
                        textures: pnt.into_par_iter().map(paint::into_texture).collect(),
                        changes_preview: false, // resolved below, once bindings are known
                    },
                    *shipped,
                )
            })
        })
        .collect();
    let base_count = base.len();
    let t_textures = t0.elapsed();

    let bound: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| {
            n.texture
                .iter()
                .chain(n.submeshes.iter().filter_map(|s| s.texture.as_ref()))
        })
        .map(|t| t.to_ascii_lowercase())
        .collect();
    for (p, shipped) in &mut paints {
        p.changes_preview = *shipped
            || (!bound.is_empty()
                && p.textures
                    .iter()
                    .any(|t| bound.contains(&t.name.to_ascii_lowercase())));
        if !p.changes_preview {
            log::info!(
                "[viewer] paint '{}' won't move the preview: it ships {:?}, and the parts shown bind {:?}",
                p.name,
                p.textures.iter().map(|t| &t.name).collect::<Vec<_>>(),
                bound,
            );
        }
    }
    let mut paints: Vec<BikePaint> = paints.into_iter().map(|(p, _)| p).collect();

    for p in &mut paints {
        let own: std::collections::HashSet<String> =
            p.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        p.textures.extend(
            base.iter()
                .filter(|t| !own.contains(&t.name.to_ascii_lowercase()))
                .cloned(),
        );
    }
    if paints.is_empty() {
        paints.push(BikePaint {
            name: "Stock".into(),
            textures: base,
            changes_preview: true, // the model's own textures, by definition
        });
    }

    let distinct_tex: std::collections::HashSet<&str> = paints
        .iter()
        .flat_map(|p| p.textures.iter().map(|t| t.token.as_str()))
        .collect();
    log::info!(
        "load_bike_model {source}: {} paint(s) + {base_count} base tex | read {t_read:?}, parse {:?}, decode {:?}, total {:?} | {} distinct texture(s), {:.1} MB resident in the texture store",
        paints.len(),
        t_parse - t_read,
        t_textures - t_parse,
        t0.elapsed(),
        distinct_tex.len(),
        texstore::resident_bytes() as f64 / (1024.0 * 1024.0),
    );
    for p in &paints {
        let mut names: Vec<&str> = p.textures.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        log::info!("  paint '{}' textures: {}", p.name, names.join(", "));
    }
    for n in &nodes {
        let subs: Vec<String> = n
            .submeshes
            .iter()
            .map(|s| {
                format!(
                    "{}->{}{}",
                    s.name,
                    s.texture.as_deref().unwrap_or("(none)"),
                    match s.uv_tile {
                        Some(0) | None => String::new(),
                        Some(t) => format!("@tile{t}"),
                    }
                )
            })
            .collect();
        log::info!("  node '{}' placed={} {}", n.name, n.placed, subs.join(", "));
    }

    let model = BikeModel { nodes, paints };
    if let Ok(mut c) = bike_cache().lock() {
        // The evicted bike's pixels go with it — nothing else references them.
        if let Some(dropped) = c.insert(key, model.clone()) {
            texstore::release(&dropped.tokens());
        }
    }
    Ok(model)
}

fn bind_textures(
    nodes: &mut [edf::EdfNode],
    edf_bytes: &[u8],
    gfx: &std::collections::HashMap<String, cfg::GfxPart>,
    node_part: &std::collections::HashMap<String, String>,
) {
    // A bike embeds every texture it draws, its stock livery included, so it declares
    // nothing beyond them.
    let colors = edf::declared_colors(edf_bytes, &[]);

    for n in nodes.iter_mut() {
        let part = node_part.get(&n.name.to_ascii_lowercase());
        let overrides = part.and_then(|p| gfx.get(p)).map(|p| &p.textures);
        if n.materials.is_empty() {
            log::warn!("[viewer] node '{}' has no material table — falling back", n.name);
        }
        // A material id is local to its node, so ask the node's own table.
        let material_texture = |mat: Option<u32>| -> Option<&String> {
            let slot = n.materials.get(mat? as usize).copied().flatten()?;
            colors.get(slot)
        };
        // A node with no submesh table draws on its first material.
        n.texture = material_texture(Some(0)).or_else(|| colors.first()).cloned();
        for sm in n.submeshes.iter_mut() {
            let group = sm.name.to_ascii_lowercase();
            // 1. An explicit gfx texture (animated chain, number plate) is authoritative.
            if let Some(tex) = overrides.and_then(|o| {
                o.get(&group)
                    .or_else(|| o.iter().find(|(g, _)| group.ends_with(&format!("_{g}"))).map(|(_, t)| t))
            }) {
                sm.texture = Some(tex.clone());
                continue;
            }
            // 2. The node's material table picks the colour texture this range was drawn on.
            if let Some(t) = material_texture(sm.mat) {
                sm.texture = Some(t.clone());
                continue;
            }
            // 3. No material recorded → leave unbound so it renders neutral grey, never smeared.
            sm.texture = None;
        }
    }
}

fn paint_display_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit('/')
        .next()
        .unwrap_or(file_name)
        .trim_end_matches(".pnt")
        .trim_end_matches(".PNT");
    let mut chars = stem.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Stock".into(),
    }
}

fn installed_paints(source: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let folder = if source.is_dir() {
        source.to_path_buf()
    } else {
        source.with_extension("")
    };
    let paints_dir = folder.join("paints");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&paints_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pnt")) {
                if let (Some(name), Ok(bytes)) =
                    (p.file_name().and_then(|n| n.to_str()), std::fs::read(&p))
                {
                    out.push((name.to_string(), bytes));
                }
            }
        }
    }
    out
}

// Any `.edf`, not just `model.edf`: a bike may ship one mesh per part, named by its
// `.hrc` (see `scene_files_for_parts`). Shadow meshes ride along unused. Shared with
// `modelswap` so the swapper and the viewer classify the same files the same way.
use bikefiles::is_viewer_file as wanted_bike_file;

/// The bike's main mesh when the `.hrc`s can't say which it is: `model.edf` by
/// convention, else the shortest non-shadow name — a per-part set like `96cr250.edf` /
/// `96cr250_fs.edf` / `96cr250_s.edf` (shadow) reduces to the chassis.
fn base_edf<'a>(
    edfs: &std::collections::HashMap<String, &'a Vec<u8>>,
) -> Option<&'a Vec<u8>> {
    if let Some(data) = edfs.get("model.edf") {
        return Some(data);
    }
    edfs.iter()
        .filter(|(name, _)| !name.ends_with("_s.edf"))
        .min_by_key(|(name, _)| (name.len(), name.to_string()))
        .or_else(|| edfs.iter().min_by_key(|(name, _)| (name.len(), name.to_string())))
        .map(|(_, data)| *data)
}

fn gather_bike_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::{bail, Context};
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("edf")) {
        let bytes = std::fs::read(p).with_context(|| format!("read {p:?}"))?;
        return Ok(vec![("model.edf".to_string(), bytes)]);
    }
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pkz")) {
        return pkz::read_selected(p, wanted_bike_file);
    }
    if p.is_dir() {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).map(str::to_string);
            if path.is_file() && name.as_deref().is_some_and(wanted_bike_file) {
                if let (Some(name), Ok(bytes)) = (name, std::fs::read(&path)) {
                    out.push((name, bytes));
                }
            }
        }
        // A mesh of any name will do — `model.edf` is the convention, not a rule.
        if out.iter().any(|(n, _)| n.to_ascii_lowercase().ends_with(".edf")) {
            return Ok(out);
        }
        let sibling = p.with_extension("pkz");
        if sibling.exists() {
            return pkz::read_selected(&sibling, wanted_bike_file);
        }
        bail!("no .edf mesh for bike folder {p:?}");
    }
    bail!("can't load a bike model from {p:?}")
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderPart {
    part: String,
    nodes: Vec<edf::EdfNode>,
    textures: Vec<paint::PaintTexture>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderModel {
    parts: Vec<RiderPart>,
}

#[tauri::command]
async fn load_rider_model(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<RiderModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_rider_model_blocking(app, loadout))
        .await
        .map_err(|e| format!("load_rider_model task failed: {e}"))?
}

fn load_rider_model_blocking(
    app: tauri::AppHandle,
    loadout: presets::Loadout,
) -> Result<RiderModel, String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let base = library::mods_subdir(&cfg.mods_path, "mods/rider");
    let mut parts = Vec::new();

    for spec in &GEAR {
        let (model, paint, goggles) = match spec.part {
            "helmet" => (
                loadout.helmet.as_str(),
                loadout.helmet_paint.as_str(),
                loadout.goggles_paint.as_str(),
            ),
            "boots" => (loadout.boots.as_str(), loadout.boots_paint.as_str(), ""),
            _ => (loadout.protection.as_str(), loadout.protection_paint.as_str(), ""),
        };
        if let Some(p) = load_gear(&cfg, &base, spec, model, paint, goggles, &loadout.rider) {
            parts.push(p);
        }
    }

    let suit = load_rider_paint(&cfg, &base, "suit", &loadout.rider, "paints", &loadout.suit_paint);
    let gloves =
        load_rider_paint(&cfg, &base, "gloves", &loadout.rider, "gloves", &loadout.gloves_paint);
    if !loadout.suit_paint.is_empty() && suit.is_none() {
        log::warn!("[rider] suit paint '{}' did not load for profile '{}'", loadout.suit_paint, loadout.rider);
    }
    if !loadout.gloves_paint.is_empty() && gloves.is_none() {
        log::warn!("[rider] glove paint '{}' did not load for profile '{}'", loadout.gloves_paint, loadout.rider);
    }
    let suit_texs = suit.as_ref().map(|s| s.textures.clone()).unwrap_or_default();
    let glove_texs = gloves.as_ref().map(|g| g.textures.clone()).unwrap_or_default();
    let mut body_texs = suit_texs;
    body_texs.extend(glove_texs);
    match load_rider_body(&cfg, &loadout.rider, body_texs) {
        Some(body) => parts.push(body),
        None => {
            if let Some(s) = suit {
                parts.push(s);
            }
            if let Some(g) = gloves {
                parts.push(g);
            }
        }
    }

    Ok(RiderModel { parts })
}

#[tauri::command]
async fn load_rider_body_model(
    app: tauri::AppHandle,
    profile: String,
) -> Result<Vec<edf::EdfNode>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(load_rider_body_nodes(&cfg, &profile).unwrap_or_default())
    })
    .await
    .map_err(|e| format!("load_rider_body_model task failed: {e}"))?
}

/// The textures a body mesh carries itself, memoised alongside the mesh.
///
/// These depend on the model and not on the loadout, but reaching them means reading the
/// whole `rider.edf` back — 67 MB for Rider+. The viewer reloads on every loadout change, so
/// without this a rider wearing a kit that leaves one slot bare re-reads the model each time
/// you touch a dropdown.
///
/// Only textures the viewer could actually draw are decoded. Skin renders as flat colour and
/// the `w_` planes render as nothing, so inflating and re-encoding them is time spent on
/// pixels no one will ever see — and on a rider body that decode costs more than parsing the
/// mesh does.
fn body_textures(src: &BodySource, profile: &str) -> Option<Vec<paint::PaintTexture>> {
    static C: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Vec<paint::PaintTexture>>>,
    > = std::sync::OnceLock::new();
    let cache = C.get_or_init(Default::default);
    let key = src.cache_key(profile);
    if let Some(t) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(t);
    }
    let drawn = |name: &str| !matches!(body_slot(Some(name)).as_str(), "hide" | "face");
    let texs = paint::extract_edf_textures_where(&src.read(profile)?, drawn);
    if let Ok(mut c) = cache.lock() {
        c.insert(key, texs.clone());
    }
    Some(texs)
}

fn load_rider_body(
    cfg: &config::AppConfig,
    profile: &str,
    mut textures: Vec<paint::PaintTexture>,
) -> Option<RiderPart> {
    let profile = rider_profile_or_stock(profile);
    let src = rider_body_source(cfg, profile)?;
    let nodes = rider_body_nodes(&src, profile)?;

    // Whatever the mesh asks for that no paint supplies, the model itself supplies: the
    // supermoto rider ships no `.pnt` at all and wears its baked textures, and a custom
    // model paints its own extra pieces into the mesh. Reading the file back costs real
    // time on a 60 MB body, so only a name actually missing pays for it.
    let supplied: std::collections::HashSet<String> =
        textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    let wanted: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| n.submeshes.iter().filter_map(|s| s.texture.as_deref()))
        .map(|t| t.to_ascii_lowercase())
        // `hide` draws nothing and `face` is bare skin — neither wants a texture.
        .filter(|t| t != "hide" && t != "face" && !supplied.contains(t))
        .collect();
    if !wanted.is_empty() {
        match body_textures(&src, profile) {
            Some(own) => textures.extend(
                own.into_iter().filter(|t| wanted.contains(&t.name.to_ascii_lowercase())),
            ),
            None => log::warn!("[rider] body '{profile}' could not be re-read for {wanted:?}"),
        }
    }

    log::info!(
        "[rider] body '{profile}' loaded from {src:?}: {} nodes, tex={:?}",
        nodes.len(),
        textures.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
    );
    Some(RiderPart {
        part: "body".into(),
        nodes,
        textures,
    })
}

/// Stand a rider body up.
///
/// Rider meshes don't agree on which axis is up. The stock motocross rider is authored Y-up;
/// the supermoto rider and Rider+ are Z-up and arrive lying on their back. The viewer anchors
/// every piece of gear to a fraction of the body's height, so a body on its side doesn't just
/// look wrong — it measures a quarter of a metre tall instead of a metre and a bit, and the
/// helmet and boots scale down to specks and sink into the torso.
///
/// A rider is a standing figure: its longest axis is its height. Where that's Z, the mesh is
/// authored in the other convention and takes that convention's one fixed rotation. Where
/// it's already Y, leave the mesh alone — guessing at a body that's already upright is how
/// the stock rider would get broken to fix a custom one.
///
/// The rotation is a half turn about Y on top of the quarter turn about X. Standing the body
/// up alone leaves it facing backwards, which the name and number planes give away: they sit
/// on a rider's back, and on the stock motocross rider — authored upright, so correct by
/// construction — they sit behind its centre. On the Z-up meshes a bare quarter turn puts
/// them in front. Both halves are needed together: `y = -z, z = -y` on its own mirrors the
/// mesh rather than turning it, which would swap the rider's left and right hands.
fn stand_body_upright(nodes: &mut [edf::EdfNode]) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for n in nodes.iter() {
        for v in n.positions.chunks_exact(3) {
            for a in 0..3 {
                lo[a] = lo[a].min(v[a]);
                hi[a] = hi[a].max(v[a]);
            }
        }
    }
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    if ext[2] <= ext[1] || ext[2] <= ext[0] {
        return;
    }
    for n in nodes.iter_mut() {
        for v in n.positions.chunks_exact_mut(3).chain(n.normals.chunks_exact_mut(3)) {
            let (x, y, z) = (v[0], v[1], v[2]);
            v[0] = -x;
            // Negated, not taken as-is: these meshes lie head-away, with the head at the
            // most negative Z, so this is what puts it at the top.
            v[1] = -z;
            v[2] = -y;
        }
    }
    log::info!("[rider] body was authored Z-up ({ext:?}); stood it upright");
}

/// Bind each body submesh to the texture the mesh itself says it wears.
///
/// A material index is not a slot: it counts into the model's own texture list, and that
/// list is written in the exporter's order. `default_mx` happens to put the suit first and
/// the gloves second; `default_sm` puts the face second and its gloves third; Rider+ puts
/// the gloves first and the suit last. So a fixed index→slot map is one model memorised —
/// it already swaps face and gloves on the supermoto rider, and on a custom model it smears
/// the glove texture across the whole body. Read the name the model was drawn against
/// instead, the same reading the bike and gear viewers take — through each node's own
/// material table, since an id counts into the table of the part that owns it and means
/// nothing outside it (see `bind_textures`).
fn bind_body_submeshes(nodes: &mut [edf::EdfNode], mesh: &[u8]) {
    let colors = edf::color_textures(mesh);
    if colors.is_empty() {
        // A mesh whose texture table doesn't parse tells us nothing; the stock layout is
        // still right for the model the app has always shown.
        return tag_body_materials(nodes);
    }
    bind_body_to_colors(nodes, &colors);
}

/// The binding itself, split out so a test can drive it without a mesh blob: reading a
/// material id through the wrong node's table is the failure worth pinning down, and it
/// needs two nodes whose tables disagree, not a parseable `.edf`.
fn bind_body_to_colors(nodes: &mut [edf::EdfNode], colors: &[edf::EmbeddedTexture]) {
    for node in nodes.iter_mut() {
        // Disjoint field borrows: the node's table is read while its submeshes are written.
        let materials = &node.materials;
        for sm in node.submeshes.iter_mut() {
            let emb = sm
                .mat
                .and_then(|m| materials.get(m as usize).copied().flatten())
                .and_then(|slot| colors.get(slot))
                .map(|t| t.name.as_str());
            sm.texture = Some(body_slot(emb));
        }
    }
}

/// The viewer slot an embedded texture name belongs to. The `w_` planes are decals the game
/// composites a rider's name and number onto and carry no look of their own, and skin must
/// never wear the kit. Everything else keeps its own name, so a paint replaces it by name
/// and a piece the paint doesn't cover falls back to the model's own texture.
fn body_slot(name: Option<&str>) -> String {
    let Some(n) = name else { return "rider".into() };
    let l = n.to_ascii_lowercase();
    if l.starts_with("w_") {
        return "hide".into();
    }
    if l.contains("face") {
        return "face".into();
    }
    l
}

fn tag_body_materials(nodes: &mut [edf::EdfNode]) {
    for n in nodes.iter_mut() {
        for sm in n.submeshes.iter_mut() {
            sm.texture = Some(
                match sm.mat {
                    Some(1) => "gloves",
                    Some(2) => "face",
                    Some(3) | Some(4) => "hide",
                    _ => "rider",
                }
                .into(),
            );
        }
    }
}

/// Rider bodies and helmets are small next to a bike, and a session cycles through a
/// handful of them, so this can hold more entries than the bike cache does.
const MESH_CACHE_CAP: usize = 12;

fn pkz_mesh_cache() -> &'static std::sync::Mutex<lru::Lru<Vec<edf::EdfNode>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<Vec<edf::EdfNode>>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP)))
}

fn keep_lod0(nodes: &mut Vec<edf::EdfNode>) {
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| n.name.is_empty() || seen.insert(n.name.clone()));
}

fn cached_mesh(key: &str) -> Option<Vec<edf::EdfNode>> {
    // `mut` because a hit marks the entry as the warm one — see `lru::Lru`.
    pkz_mesh_cache().lock().ok().and_then(|mut c| c.get(key).cloned())
}

/// Parse a mesh into viewer space and memoise it. `prepare` runs once, on the parse that
/// populates the cache, for work that depends on the file rather than on the loadout.
fn mesh_from_bytes(
    key: String,
    data: &[u8],
    // Gear is read with [`edf::parse_gear`] — see there for why the two readings differ.
    gear: bool,
    prepare: impl FnOnce(&mut Vec<edf::EdfNode>, &[u8]),
) -> Option<Vec<edf::EdfNode>> {
    let mut nodes = if gear { edf::parse_gear(data) } else { edf::parse(data) };
    edf::to_right_handed(&mut nodes);
    keep_lod0(&mut nodes);
    if nodes.is_empty() {
        return None;
    }
    prepare(&mut nodes, data);
    if let Ok(mut c) = pkz_mesh_cache().lock() {
        c.insert(key, nodes.clone());
    }
    Some(nodes)
}

/// A gear mesh out of a game archive, memoised. Gear-only: the two readings would otherwise
/// share a cache entry, and whichever asked first would settle how the other saw the file.
fn load_pkz_mesh(pkz: &std::path::Path, entry: &str) -> Option<Vec<edf::EdfNode>> {
    let key = format!("{}:{}#gear", bike_cache_key(&pkz.to_string_lossy()), entry);
    if let Some(n) = cached_mesh(&key) {
        return Some(n);
    }
    mesh_from_bytes(key, &read_pkz_entry(pkz, entry)?, true, |_, _| {})
}

/// The stock rider profiles the game itself ships. They're the fallback for a custom model
/// that brings a mesh but none of the kits meant to be worn on it.
const STOCK_RIDER_PROFILES: [&str; 2] = ["default_mx", "default_sm"];

fn rider_profile_or_stock(profile: &str) -> &str {
    if profile.is_empty() { STOCK_RIDER_PROFILES[0] } else { profile }
}

/// Where a rider profile's body mesh lives.
///
/// A rider model is a whole new `rider.edf`, not a texture — Rider+ and its variants install
/// as folders under `mods/rider/riders`. The game's own archive is the last place to look,
/// not the only one: reading only `rider.pkz` left a picked custom profile rendering no body
/// at all, just gear floating where the rider should be.
#[derive(Debug, Clone)]
enum BodySource {
    /// `mods/rider/riders/<profile>/rider.edf`, installed loose — the shape every rider
    /// model on mxb-mods ships.
    Loose(std::path::PathBuf),
    /// A profile packed as `<profile>.pkz`, or the game's own `rider.pkz`.
    Packed(std::path::PathBuf),
}

impl BodySource {
    fn cache_key(&self, profile: &str) -> String {
        match self {
            Self::Loose(p) => bike_cache_key(&p.to_string_lossy()),
            Self::Packed(p) => format!("{}:{profile}", bike_cache_key(&p.to_string_lossy())),
        }
    }

    /// The mesh bytes. A packed profile is read at the entry the game uses, then — for a
    /// repack that flattened the folder tree — by any `rider.edf` in the archive.
    fn read(&self, profile: &str) -> Option<Vec<u8>> {
        match self {
            Self::Loose(p) => std::fs::read(p).ok(),
            Self::Packed(p) => read_pkz_entry(p, &format!("rider/riders/{profile}/rider.edf"))
                .or_else(|| read_pkz_basename(p, "rider.edf")),
        }
    }
}

/// One named file out of an archive wherever it sits in the tree, for repacks that don't
/// keep the game's layout. Only the wanted entry is inflated.
fn read_pkz_basename(pkz: &std::path::Path, base: &str) -> Option<Vec<u8>> {
    let want = |n: &str| {
        n.replace('\\', "/")
            .rsplit('/')
            .next()
            .is_some_and(|b| b.eq_ignore_ascii_case(base))
    };
    pkz::read_selected(pkz, want).ok()?.into_iter().next().map(|(_, d)| d)
}

fn rider_body_source(cfg: &config::AppConfig, profile: &str) -> Option<BodySource> {
    let riders = library::mods_subdir(&cfg.mods_path, "mods/rider/riders");
    let loose = riders.join(profile).join("rider.edf");
    if loose.is_file() {
        return Some(BodySource::Loose(loose));
    }
    let packed = riders.join(format!("{profile}.pkz"));
    if packed.is_file() {
        return Some(BodySource::Packed(packed));
    }
    resolve_game_pkz(cfg, "rider.pkz").map(BodySource::Packed)
}

/// The body mesh, submeshes already bound to their textures. Binding depends on the model
/// and not on the loadout, so it happens on the parse that fills the cache — a paint change
/// must not re-read a 60 MB body.
fn rider_body_nodes(src: &BodySource, profile: &str) -> Option<Vec<edf::EdfNode>> {
    let key = src.cache_key(profile);
    if let Some(n) = cached_mesh(&key) {
        return Some(n);
    }
    mesh_from_bytes(key, &src.read(profile)?, false, |nodes, data| {
        bind_body_submeshes(nodes, data);
        stand_body_upright(nodes);
    })
}

fn load_rider_body_nodes(cfg: &config::AppConfig, profile: &str) -> Option<Vec<edf::EdfNode>> {
    let profile = rider_profile_or_stock(profile);
    rider_body_nodes(&rider_body_source(cfg, profile)?, profile)
}

fn resolve_game_pkz(cfg: &config::AppConfig, name: &str) -> Option<std::path::PathBuf> {
    let gp = cfg.game_path.trim();
    if !gp.is_empty() {
        let p = std::path::Path::new(gp).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    let p = std::path::Path::new(&cfg.mods_path).join(name);
    if p.exists() {
        return Some(p);
    }
    // Last resort for configs that predate game-path auto-detection: scan Steam now.
    let detected = config::detect_game_path(cfg.game())?;
    let p = std::path::Path::new(&detected).join(name);
    p.exists().then_some(p)
}

fn read_pkz_entry(pkz: &std::path::Path, entry: &str) -> Option<Vec<u8>> {
    let matches = |name: &str| name.replace('\\', "/").eq_ignore_ascii_case(entry);
    if pkz::is_plain_zip(pkz) {
        let file = std::fs::File::open(pkz).ok()?;
        let mut zip = zip::ZipArchive::new(file).ok()?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).ok()?;
            if matches(f.name()) {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut buf).ok()?;
                return Some(buf);
            }
        }
        return None;
    }
    pkz::read_all(pkz)
        .ok()?
        .into_iter()
        .find(|(n, _)| matches(n))
        .map(|(_, d)| d)
}

#[tauri::command]
async fn load_gear_model(
    path: String,
    part: String,
    paint: Option<String>,
    goggles: Option<String>,
    // Show the mesh's own textures instead of a `.pnt` — the stock look. Separate flags
    // because a helmet's goggles are picked independently of its shell.
    stock: Option<bool>,
    stock_goggles: Option<bool>,
) -> Result<RiderPart, String> {
    tauri::async_runtime::spawn_blocking(move || {
        load_gear_model_blocking(
            path,
            part,
            paint,
            goggles,
            stock.unwrap_or(false),
            stock_goggles.unwrap_or(false),
            // A library preview shows one mod on its own — nothing outside it to gather.
            Vec::new(),
        )
    })
    .await
    .map_err(|e| format!("load_gear_model task failed: {e}"))?
}

#[tauri::command]
async fn load_stock_gear_model(
    app: tauri::AppHandle,
    part: String,
    paint_path: Option<String>,
) -> Result<RiderPart, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        let spec = GEAR
            .iter()
            .find(|g| g.part == part)
            .ok_or_else(|| format!("no stock model for gear slot '{part}'"))?;
        let pkz = resolve_game_pkz(&cfg, "rider.pkz")
            .ok_or_else(|| "game path not set or rider.pkz not found".to_string())?;
        let folder = format!("rider/{}/{}", spec.pkz_kind, spec.default_name);
        // The entry is kept, not just the nodes: with no paint to show, the mesh's own
        // textures are the look, and they're read back out of the file it came from.
        let named = format!("{folder}/{}", spec.mesh);
        let (entry, nodes) = load_pkz_mesh(&pkz, &named)
            .map(|n| (named, n))
            .or_else(|| {
                let alt = stock_gear_entry(&pkz, &folder)?;
                let n = load_pkz_mesh(&pkz, &alt)?;
                Some((alt, n))
            })
            .ok_or_else(|| format!("stock {part} mesh not found in rider.pkz"))?;
        let textures = match paint_path.filter(|s| !s.is_empty()) {
            Some(p) => std::fs::read(&p)
                .ok()
                .and_then(|d| paint::decode_any(&d).ok())
                .map(|pnt| pnt.into_par_iter().map(paint::into_texture).collect())
                .unwrap_or_default(),
            // Nothing to preview → the stock look, which is what the mesh carries. Not the
            // first `.pnt` in the folder: that's a paint like any other, and picking it here
            // is what showed the stock helmet bronze everywhere else. Companion maps are
            // skipped — the viewer never draws a normal or roughness sheet, so inflating
            // one is time spent on pixels no one sees.
            None => read_pkz_entry(&pkz, &entry)
                .map(|d| paint::extract_edf_textures_where(&d, |n| !is_companion_map(n)))
                .unwrap_or_default(),
        };
        Ok(RiderPart {
            part: spec.part.into(),
            nodes,
            textures,
        })
    })
    .await
    .map_err(|e| format!("load_stock_gear_model task failed: {e}"))?
}

#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct GearPaints {
    paints: Vec<String>,
    goggles: Vec<String>,
    /// The mesh carries its own shell / goggle texture, so a "Stock" entry is worth
    /// offering next to the packed paints. Preview-only — never a loadout value, since
    /// the game names a `.pnt` there and has no word for "the model's own look".
    has_stock: bool,
    has_stock_goggles: bool,
}

impl GearPaints {
    /// Fold another source's paints into this one.
    ///
    /// A name that turns up twice is the same look twice — a paint pack installed loose
    /// beside the `.pkz` it was made for ships the same file names — so repeats are dropped
    /// rather than offered as two choices.
    fn absorb(&mut self, other: GearPaints) {
        let merge = |into: &mut Vec<String>, more: Vec<String>| {
            for n in more {
                if !into.iter().any(|have| have.eq_ignore_ascii_case(&n)) {
                    into.push(n);
                }
            }
        };
        merge(&mut self.paints, other.paints);
        merge(&mut self.goggles, other.goggles);
        self.has_stock |= other.has_stock;
        self.has_stock_goggles |= other.has_stock_goggles;
    }

    /// Alphabetical across the merged set — sources arrive sorted individually, which on its
    /// own would list one source after the other rather than one list of paints.
    fn sort(&mut self) {
        self.paints.sort_by_key(|s| s.to_lowercase());
        self.goggles.sort_by_key(|s| s.to_lowercase());
    }
}

fn gear_paints_at(path: &std::path::Path) -> Result<GearPaints, String> {
    let files = read_gear_files(path).map_err(|e| format!("{e:#}"))?;
    let names = |folder: &str| {
        let mut out: Vec<String> = files
            .iter()
            .filter_map(|(n, _)| gear_folder_paint_name(n, folder))
            .collect();
        out.sort_by_key(|s| s.to_lowercase());
        out.dedup();
        out
    };
    // Names only — decoding the pixels is the load path's job, and this runs per picker.
    // Resolved the same way the loader resolves it, so the picker can't offer a stock look
    // that comes off a different mesh than the one drawn — every mesh it draws, since a
    // two-piece set carries a texture per piece.
    let scenes = gear_scenes(&files);
    let mut meshes: Vec<&Vec<u8>> = scenes.iter().filter_map(|s| gear_file(&files, s)).collect();
    if meshes.is_empty() {
        meshes.extend(files.iter().find(|(n, _)| is_visible_gear_mesh(n)).map(|(_, d)| d));
    }
    let mut embedded: Vec<String> = Vec::new();
    for d in &meshes {
        for t in edf::embedded_textures(d) {
            if !embedded.iter().any(|h| h.eq_ignore_ascii_case(&t.name)) {
                embedded.push(t.name);
            }
        }
    }
    let supplied = |folder: &str| {
        paint_texture_names(
            files
                .iter()
                .filter(|(n, _)| gear_folder_paint_name(n, folder).is_some())
                .map(|(_, d)| d.as_slice()),
        )
    };
    Ok(GearPaints {
        has_stock: mesh_supplies_side(&embedded, &supplied("paints"), false),
        has_stock_goggles: mesh_supplies_side(&embedded, &supplied("goggles"), true),
        paints: names("paints"),
        goggles: names("goggles"),
    })
}

/// Whether a side has a stock look to offer: the mesh carries its own copy of the sheet that
/// side's paints replace.
///
/// Asking the paints, not the texture names, is what keeps a "Stock" entry off the Bell Moto 10 —
/// it embeds a tear-off film and a goggle, but not the shell sheet its paints supply, so picking
/// "Stock" there drew the helmet in a near-blank film. With nothing painted on a side at all, the
/// mesh's own look is the only one there is, so anything it carries for that side counts.
///
/// One function rather than two readings of the same question: it decides both whether the
/// library's picker offers "Stock" and whether an empty paint slot resolves to it
/// ([`load_gear_model_blocking`]). Those two have to agree — the rider tab rendering a helmet
/// the library's "Stock" entry says doesn't exist is the drift worth designing out.
fn mesh_supplies_side(embedded: &[String], side_paints: &[String], goggle_side: bool) -> bool {
    let mut usable = embedded.iter().filter(|n| !is_companion_map(n));
    if side_paints.is_empty() {
        return usable.any(|n| is_goggle_name(n) == goggle_side);
    }
    usable.any(|e| side_paints.iter().any(|p| p.eq_ignore_ascii_case(e)))
}

#[tauri::command]
async fn list_gear_paints(path: String) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || gear_paints_at(std::path::Path::new(&path)))
        .await
        .map_err(|e| format!("list_gear_paints task failed: {e}"))?
}

#[tauri::command]
async fn list_installed_gear_paints(
    app: tauri::AppHandle,
    part: String,
    model: String,
) -> Result<GearPaints, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if model.trim().is_empty() {
            return Ok(GearPaints::default());
        }
        let Some(spec) = GEAR.iter().find(|g| g.part == part) else {
            return Ok(GearPaints::default());
        };
        let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
        Ok(gear_paints_for(&cfg, spec, &model))
    })
    .await
    .map_err(|e| format!("list_installed_gear_paints task failed: {e}"))?
}

/// Every paint a gear model can be worn in, from every place one can live.
///
/// A model reaches the game three ways, and more than one is true at once more often than
/// not: an unpacked `<model>/` folder, a `<model>.pkz`, and — for the pieces the game itself
/// ships — a folder inside `rider.pkz`. A paint pack for a packaged mod installs loose in a
/// folder beside it, because nothing can write into the `.pkz`. So a picker that stopped at
/// the first source it found showed one of those sets and never the other.
fn gear_paints_for(cfg: &config::AppConfig, spec: &GearSpec, model: &str) -> GearPaints {
    let stem = model.trim_end_matches(".pkz");
    let rider = library::mods_subdir(&cfg.mods_path, "mods/rider");
    let mut out = GearPaints::default();
    for src in gear_sources(&rider, spec, stem) {
        if !src.exists() {
            continue;
        }
        match gear_paints_at(&src) {
            Ok(found) => out.absorb(found),
            // One unreadable source must not cost the others their paints.
            Err(e) => log::warn!("[rider] {} paints from {src:?}: {e}", spec.part),
        }
    }
    // The game's own copy of the piece. This is all a stock name — `default`, `full`, `neck`
    // — has instead of a folder, and a mod installed under a stock name gets both.
    if let Some(pkz) = resolve_game_pkz(cfg, "rider.pkz") {
        let folder = format!("rider/{}/{}", spec.pkz_kind, stem);
        out.absorb(GearPaints {
            paints: pkz_paint_names(&pkz, &folder, "paints"),
            goggles: pkz_paint_names(&pkz, &folder, "goggles"),
            // Whether the stock mesh has a look of its own is a question about the mesh, and
            // answering it would mean pulling that mesh out of a 100 MB archive every time a
            // picker opens. The installed sources above answer it where a "Stock" entry is
            // actually offered.
            ..GearPaints::default()
        });
    }
    out.sort();
    out
}

/// The `.pnt` names under `<folder>/<sub>/` inside a pkz, without reading a byte of pixels.
///
/// Names come out of the archive's own directory, which keeps this cheap enough to run every
/// time a picker opens — the game's `rider.pkz` is around 100 MB, and reading it to list a
/// dozen names would be felt. That shortcut is the same assumption [`read_pkz_first`] makes,
/// that the game's own archives are plain zips, with the general reader behind it for
/// anything else.
fn pkz_paint_names(pkz: &std::path::Path, folder: &str, sub: &str) -> Vec<String> {
    let prefix = format!("{folder}/{sub}/").to_ascii_lowercase();
    let stem = |name: &str| -> Option<String> {
        let n = name.replace('\\', "/");
        let lower = n.to_ascii_lowercase();
        if !lower.starts_with(&prefix) || !lower.ends_with(".pnt") {
            return None;
        }
        let base = n.rsplit('/').next()?;
        Some(base[..base.len() - ".pnt".len()].to_string())
    };
    if pkz::is_plain_zip(pkz) {
        let Ok(file) = std::fs::File::open(pkz) else {
            return Vec::new();
        };
        let Ok(zip) = zip::ZipArchive::new(file) else {
            return Vec::new();
        };
        return zip.file_names().filter_map(stem).collect();
    }
    pkz::read_selected(pkz, |n| stem(n).is_some())
        .map(|hits| hits.iter().filter_map(|(n, _)| stem(n)).collect())
        .unwrap_or_default()
}

fn gear_folder_paint_name(entry: &str, folder: &str) -> Option<String> {
    let n = entry.replace('\\', "/").to_ascii_lowercase();
    if !n.contains(&format!("/{folder}/")) && !n.starts_with(&format!("{folder}/")) {
        return None;
    }
    let base = entry.replace('\\', "/");
    let base = base.rsplit('/').next()?;
    let stem = base.strip_suffix(".pnt").or_else(|| base.strip_suffix(".PNT"))?;
    (!stem.is_empty()).then(|| stem.to_string())
}

/// One painted side of a gear item — the shell, or the goggles. A `.pnt` replaces the
/// mesh's textures by name, so a side is the names it supplies plus the colour texture a
/// piece falls back on when the mesh asks for one this side doesn't carry.
#[derive(Default)]
struct GearSide {
    names: Vec<String>,
    primary: Option<String>,
}

impl GearSide {
    fn new(names: Vec<String>) -> Self {
        let primary = names
            .iter()
            .find(|n| !is_companion_map(n))
            .or_else(|| names.first())
            .cloned();
        Self { names, primary }
    }

    /// This side's own name for a texture the mesh asks for, if it supplies it.
    fn supplies(&self, want: &str) -> Option<String> {
        self.names.iter().find(|n| n.eq_ignore_ascii_case(want)).cloned()
    }
}

fn load_gear_model_blocking(
    path: String,
    part: String,
    paint: Option<String>,
    goggles: Option<String>,
    stock: bool,
    stock_goggles: bool,
    // Paints that live outside the model — a rider profile's goggles, or `.pnt` files
    // dropped beside a `.pkz`. Named as a gear archive would carry them (`goggles/x.pnt`)
    // so they read exactly like the packed ones.
    extra: Vec<(String, Vec<u8>)>,
) -> Result<RiderPart, String> {
    let p = std::path::Path::new(&path);
    let mut files = read_gear_files(p).map_err(|e| format!("{e:#}"))?;
    // Appended, so a name the model itself packs still wins.
    files.extend(extra);
    let want = paint.filter(|s| !s.is_empty());
    let want_goggles = goggles.filter(|s| !s.is_empty());
    // Collect paint/goggle entries up front so we can prefer the requested one but always
    // fall back to the first available: a stale or unknown paint name must still show the
    // gear textured, never bare grey.
    let mut paints: Vec<(String, &Vec<u8>)> = Vec::new();
    let mut goggle_paints: Vec<(String, &Vec<u8>)> = Vec::new();
    for (name, data) in &files {
        if let Some(pname) = gear_folder_paint_name(name, "paints") {
            paints.push((pname, data));
        } else if let Some(gname) = gear_folder_paint_name(name, "goggles") {
            goggle_paints.push((gname, data));
        }
    }
    // Every `.edf` the item draws, in the mod's own words where it says so — kept as bytes
    // too, so a stock side can read each mesh's own textures back out of it.
    let mut meshes: Vec<&Vec<u8>> =
        gear_scenes(&files).iter().filter_map(|s| gear_file(&files, s)).collect();
    if meshes.is_empty() {
        // A mod that names a scene it doesn't ship still has a mesh in the folder; take it.
        meshes.extend(files.iter().find(|(n, _)| is_visible_gear_mesh(n)).map(|(_, d)| d));
    }
    // Parsed per mesh and kept that way until binding: a submesh's material id counts its own
    // mesh's texture list, so one mesh's materials must never be read against another's.
    let mut drawn: Vec<(&Vec<u8>, Vec<edf::EdfNode>)> = Vec::new();
    for d in &meshes {
        let mut nodes = edf::parse_gear(d);
        edf::to_right_handed(&mut nodes);
        keep_lod0(&mut nodes);
        if !nodes.is_empty() {
            drawn.push((d, nodes));
        }
    }
    if drawn.is_empty() {
        return Err(format!("no gear mesh found in {path}"));
    }
    // Which textures each side's paints supply — the names the mesh declares and leaves to a
    // `.pnt`, and so part of the slot order its materials count. Headers only, no pixels, and
    // wanted twice over: to decide whether a side has a stock look at all, and (rejoined
    // below) to bind the submeshes.
    let shell_declared = paint_texture_names(paints.iter().map(|(_, d)| d.as_slice()));
    let goggle_declared = paint_texture_names(goggle_paints.iter().map(|(_, d)| d.as_slice()));
    // Whether the meshes carry the shell sheet themselves. Reads headers, never pixels — but
    // it still walks each mesh's bytes looking for them, so it's a closure rather than a
    // value: the rider viewer reloads on every slot edit, and a load that names its paint has
    // already answered the question without asking.
    let mesh_carries_shell = || {
        let mut names: Vec<String> = Vec::new();
        for (d, _) in &drawn {
            for t in edf::embedded_textures(d) {
                if !names.iter().any(|h| h.eq_ignore_ascii_case(&t.name)) {
                    names.push(t.name);
                }
            }
        }
        mesh_supplies_side(&names, &shell_declared, false)
    };
    // With no `.pnt` to offer, the shell wears what the mesh already carries. Helmets and
    // boots nearly always ship a paint, so this reads as an edge case there — on the
    // protection slot it's the norm: a chain, a bib or a chest protector bakes its look into
    // the `.edf` and ships an empty `paints/` folder. Asked for a paint that doesn't exist,
    // the binder had nothing to hand each piece and the whole item came out bare grey.
    //
    // Naming no paint at all is that same answer arrived at from the other side: an empty
    // slot is the loadout saying "the model's own look", not "any paint will do", and letting
    // it fall through to `pick_gear_paint` dressed the rider in whichever paint the mod
    // happened to list first. Only where the mesh actually carries the shell sheet, though —
    // the Bell Moto 10 ships paints and embeds only a tear-off film, and forcing stock there
    // would draw the helmet near-blank instead. Same question the library's picker asks
    // before it offers "Stock", asked through the same function, so the two can't disagree.
    //
    // Only the shell. Unpainted goggles already have somewhere to go — they fall back to the
    // shell's texture, which is where a helmet that doesn't paint them apart drew them — and
    // a helmet whose shell paint repaints the goggles too would lose that to the mesh's own.
    let stock = stock || paints.is_empty() || (want.is_none() && mesh_carries_shell());
    // A stock side decodes nothing from `paints/` — the mesh already carries that texture.
    let main_pnt = (!stock)
        .then(|| pick_gear_paint(&paints, want.as_deref(), &part))
        .flatten()
        .unwrap_or_default();
    let goggle_pnt = (!stock_goggles)
        .then(|| pick_gear_paint(&goggle_paints, want_goggles.as_deref(), &format!("{part} goggle")))
        .flatten()
        .unwrap_or_default();
    let names_of = |texs: &[paint::PntTexture]| texs.iter().map(|t| t.name.clone()).collect();
    let mut main_side = GearSide::new(names_of(&main_pnt));
    let mut goggle_side = GearSide::new(names_of(&goggle_pnt));
    let mut out: Vec<paint::PaintTexture> =
        main_pnt.into_par_iter().chain(goggle_pnt).map(paint::into_texture).collect();
    // The look the model ships with, before any paint: the textures embedded in the meshes.
    if stock || stock_goggles {
        let mut embedded: Vec<paint::PaintTexture> = Vec::new();
        for (d, _) in &drawn {
            for t in paint::extract_edf_textures(d) {
                if !embedded.iter().any(|h| h.name.eq_ignore_ascii_case(&t.name)) {
                    embedded.push(t);
                }
            }
        }
        let (emb_goggle, emb_main): (Vec<String>, Vec<String>) =
            embedded.iter().map(|t| t.name.clone()).partition(|n| is_goggle_name(n));
        if stock {
            main_side = GearSide::new(emb_main);
            if main_side.primary.is_none() {
                log::warn!("[rider] {part} has no stock texture in its mesh — showing it bare");
            }
        }
        if stock_goggles {
            goggle_side = GearSide::new(emb_goggle);
        }
        // A paint reuses the mesh's texture names (that's how it replaces them), so with
        // one side stock and the other painted the two sets collide. Resolve it here: the
        // stock side's embedded texture wins, the painted side keeps its `.pnt`. The
        // frontend maps textures by name and would otherwise show whichever image
        // happened to finish loading last.
        let claimed: Vec<String> = [stock.then_some(&main_side), stock_goggles.then_some(&goggle_side)]
            .into_iter()
            .flatten()
            .flat_map(|s| s.names.iter())
            .map(|n| n.to_ascii_lowercase())
            .collect();
        out.retain(|t| !claimed.contains(&t.name.to_ascii_lowercase()));
        let taken: std::collections::HashSet<String> =
            out.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        out.extend(
            embedded
                .into_iter()
                .filter(|t| !taken.contains(&t.name.to_ascii_lowercase())),
        );
    }
    // Both sides' names as one list, for the binder — counted even when nothing is painted,
    // since a stock preview counts the same slots.
    let mut declared = shell_declared;
    for n in goggle_declared {
        if !declared.iter().any(|h| h.eq_ignore_ascii_case(&n)) {
            declared.push(n);
        }
    }
    for (d, nodes) in &mut drawn {
        bind_gear_submeshes(nodes, Some(d.as_slice()), &main_side, &goggle_side, &declared);
    }
    let nodes: Vec<edf::EdfNode> = drawn.into_iter().flat_map(|(_, n)| n).collect();
    // Pieces the paints don't cover keep the mesh's own texture — a tear-off film, a visor,
    // anything the author baked in and left out of the `.pnt`. The game draws those from the
    // mesh, so they have to travel with it; without them the Bell Moto 10's tear-off had
    // nothing to wear and took the helmet's paint across it. Decoded by name, so a mesh
    // holding several 4K sheets only pays for the ones actually on screen.
    let shipped: std::collections::HashSet<String> =
        out.iter().map(|t| t.name.to_ascii_lowercase()).collect();
    let worn: std::collections::HashSet<String> = nodes
        .iter()
        .flat_map(|n| n.texture.iter().chain(n.submeshes.iter().filter_map(|s| s.texture.as_ref())))
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !shipped.contains(t))
        .collect();
    for d in meshes.iter().filter(|_| !worn.is_empty()) {
        for t in paint::extract_edf_textures_where(d, |n| worn.contains(&n.to_ascii_lowercase())) {
            if !out.iter().any(|h| h.name.eq_ignore_ascii_case(&t.name)) {
                out.push(t);
            }
        }
    }
    log::info!(
        "[viewer] {part}: paint={want:?} goggles={want_goggles:?} stock={stock}/{stock_goggles} \
         -> shell {:?}, goggles {:?} ({} textures)",
        main_side.primary,
        goggle_side.primary,
        out.len(),
    );
    Ok(RiderPart { part, nodes, textures: out })
}

/// One file out of a gear folder or archive, by base name.
fn gear_file<'a>(files: &'a [(String, Vec<u8>)], want: &str) -> Option<&'a Vec<u8>> {
    files
        .iter()
        .find(|(n, _)| n.rsplit('/').next().unwrap_or(n).eq_ignore_ascii_case(want))
        .map(|(_, d)| d)
}

/// Every `.edf` a gear item draws, in the order it declares them.
///
/// Follow `gfx.cfg` → `<piece>.hrc` → `level0 { scene }`, the same chain the game walks and
/// the bike loader already uses. Worth following on gear because a gear mesh is named for the
/// piece rather than the slot — `neckbrace.edf`, `pickaxe.edf`, `protection.edf` all turn up
/// in the protection folder — so which `.edf` is *the* one is the mod's answer to give, not
/// ours to guess from filenames.
///
/// A gear item is not always one mesh: the stock `full` protection declares an `armour` and a
/// `neckbrace`, each its own `.edf`, and drawing whichever block came first left the rider
/// wearing half the item — a different half from one run to the next, since the blocks are
/// keyed by name rather than kept in file order.
fn gear_scenes(files: &[(String, Vec<u8>)]) -> Vec<String> {
    let Some(gfx) = gear_file(files, "gfx.cfg").map(|d| cfg::parse(d)) else {
        return Vec::new();
    };
    // Three spellings are in use and all three are the item: `model = x.hrc` at the top level
    // (helmets), `<piece> { model = x.hrc }` (protection), `<piece> { model { file = x.hrc } }`
    // (boots). `cockpit` is the first-person mesh and `shadow` the blob cast under the rider —
    // neither is ever drawn on the model.
    let mut hrcs: Vec<String> = Vec::new();
    let mut push = |name: Option<&str>| {
        if let Some(n) = name.filter(|n| !n.is_empty()) {
            if !hrcs.iter().any(|h| h.eq_ignore_ascii_case(n)) {
                hrcs.push(n.to_string());
            }
        }
    };
    push(gfx.get("model"));
    let mut blocks: Vec<&String> = gfx.blocks.keys().collect();
    blocks.sort();
    for name in blocks {
        if matches!(name.as_str(), "cockpit" | "shadow") {
            continue;
        }
        let b = &gfx.blocks[name];
        push(b.get("model").or_else(|| b.block("model").and_then(|m| m.get("file"))));
    }
    let mut scenes: Vec<String> = Vec::new();
    for hrc in &hrcs {
        let Some(scene) = gear_file(files, hrc)
            .map(|d| cfg::parse(d))
            .and_then(|c| cfg::hrc_level0_scene(&c))
        else {
            continue;
        };
        // Two `.hrc`s can name one mesh — the boots' left and right both point at `boots.edf`,
        // which already holds both feet — so a repeat is one piece, not two.
        if !scenes.iter().any(|s| s.eq_ignore_ascii_case(&scene)) {
            scenes.push(scene);
        }
    }
    scenes
}

/// The gear file carrying the visible mesh — not the `_s` shadow or the `c_` cockpit variant.
fn is_visible_gear_mesh(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    base.ends_with(".edf") && !base.ends_with("_s.edf") && !base.starts_with("c_")
}

/// Normal (`_n`) and reflection (`_r`) maps ride alongside a colour texture and are never
/// the look itself. Mirrors the filter in `paint::extract_edf_textures`, and shares the
/// exporter-spelled names with [`edf::is_companion_texture`] so the two can't drift.
/// `_s` is left out on purpose: the mesh-side filter reads it as MX Bikes' specular map,
/// but a `.pnt` may legitimately name a texture that way.
fn is_companion_map(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with("_n") || n.ends_with("_r") || edf::is_exporter_companion(&n)
}

/// Goggles (and their lens) are the one gear part painted separately from the shell —
/// the same test decides which submesh wears which texture, and which embedded texture
/// is the stock goggle.
fn is_goggle_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("goggle") || n.contains("lens")
}

/// Pick a gear paint by name, else the first available so the piece is always textured —
/// a stale or unknown name must never leave the gear bare grey. `what` labels the side in
/// the log, since a miss is exactly how a picked paint ends up looking like no change.
fn pick_gear_paint(
    paints: &[(String, &Vec<u8>)],
    want: Option<&str>,
    what: &str,
) -> Option<Vec<paint::PntTexture>> {
    let hit = want.and_then(|w| paints.iter().find(|(n, _)| n.eq_ignore_ascii_case(w)));
    if let (Some(w), None, false) = (want, hit, paints.is_empty()) {
        log::warn!("[rider] {what} paint '{w}' not found; used first of {} available", paints.len());
    }
    paint::decode_any(hit.or_else(|| paints.first())?.1).ok()
}

/// Every texture name a set of `.pnt` files supplies, deduplicated. Headers only — no pixels
/// are inflated, so this stays cheap enough to run on every paint an item ships.
fn paint_texture_names<'a>(paints: impl Iterator<Item = &'a [u8]>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for data in paints {
        for name in paint::texture_names_any(data).unwrap_or_default() {
            if !out.iter().any(|n: &String| n.eq_ignore_ascii_case(&name)) {
                out.push(name);
            }
        }
    }
    out
}

/// Which side of the item a piece belongs to, given the texture the mesh draws it from.
///
/// The paints decide it. A `.pnt` replaces textures *by name*, so the side that supplies
/// the name a piece is drawn from is the side that piece is on — the mod's own answer,
/// stated in its own files. Only when neither side claims the name (or the mesh names none)
/// does the piece's spelling get a say.
///
/// It has to be this way round, because a helmet names its goggle group after the goggle it
/// ships: the Bell Moto 10's goggle submeshes are called `Armega.001` and drawn from
/// `Racecraft`, neither of which reads as "goggles" — so on names alone the whole goggle
/// went out wearing the helmet's paint.
fn on_goggle_side(emb: Option<&str>, spelled_goggle: bool, main: &GearSide, goggle: &GearSide) -> bool {
    match emb {
        Some(e) if goggle.supplies(e).is_some() => true,
        Some(e) if main.supplies(e).is_some() => false,
        _ => spelled_goggle,
    }
}

/// Bind each submesh (or single-material node) to the texture it should wear: goggles take
/// the goggle paint, everything else the shell paint.
///
/// The mesh settles which texture a piece was drawn against — a submesh's material names it,
/// the same reading the bike viewer uses — and [`on_goggle_side`] settles whose texture that
/// is. A paint replaces textures by name, so a side wearing one it doesn't supply falls back
/// to its primary; unmatched → `None`, so the frontend renders neutral grey rather than
/// smearing another part's texture over it.
fn bind_gear_submeshes(
    nodes: &mut [edf::EdfNode],
    mesh: Option<&[u8]>,
    main: &GearSide,
    goggle: &GearSide,
    // Every texture name the item's own paints supply — see `paint_texture_names`.
    declared: &[String],
) {
    let colors = mesh.map(|d| edf::declared_colors(d, declared)).unwrap_or_default();
    // Names the mesh carries pixels for, so a piece no paint covers can keep its own look.
    let embedded: Vec<String> = mesh
        .map(|d| edf::embedded_textures(d).into_iter().map(|t| t.name).collect())
        .unwrap_or_default();
    let carried = |want: &str| embedded.iter().any(|n| n.eq_ignore_ascii_case(want));
    // What this side puts on a piece the mesh draws from `emb`.
    let wear = |emb: Option<&str>, goggles_here: bool| -> Option<String> {
        let (side, other) = if goggles_here { (goggle, main) } else { (main, goggle) };
        emb.and_then(|e| side.supplies(e))
            // No paint replaces this one, but the mesh has it: that IS the piece's look, and
            // it beats stretching the side's paint over something it was never drawn for.
            .or_else(|| emb.filter(|e| carried(e)).map(str::to_string))
            .or_else(|| side.primary.clone())
            // A helmet whose goggles aren't painted apart still shows them — in the shell's
            // texture, which is where they were drawn.
            .or_else(|| goggles_here.then(|| other.primary.clone()).flatten())
    };
    for node in nodes.iter_mut() {
        // A material id is local to its node — resolve it through that node's own table.
        let material_texture = |mat: Option<u32>| -> Option<&str> {
            let slot = node.materials.get(mat? as usize).copied().flatten()?;
            colors.get(slot).map(String::as_str)
        };
        let node_goggle = is_goggle_name(&node.name);
        if node.submeshes.is_empty() {
            // No submesh table means no material index to look up. Take a texture from this
            // side of the mesh, so a goggle node isn't handed the shell's.
            let emb = colors
                .iter()
                .map(String::as_str)
                .find(|n| on_goggle_side(Some(n), is_goggle_name(n), main, goggle) == node_goggle);
            node.texture = wear(emb, node_goggle);
            continue;
        }
        for sm in &mut node.submeshes {
            let emb = material_texture(sm.mat);
            let spelled = node_goggle || is_goggle_name(&sm.name) || emb.is_some_and(is_goggle_name);
            sm.texture = wear(emb, on_goggle_side(emb, spelled, main, goggle));
        }
    }
}

/// One loose file out of an unpacked gear folder, in the form the rest of the loader reads.
///
/// A mod that ships as a plain folder may still seal its individual files the way a `.pkz`
/// seals its entries — the Tactical Vest on mxb-mods does, and read raw its `.edf` isn't a
/// mesh at all, so the whole item failed with "no gear mesh found". Anything already plain
/// passes straight through.
fn read_gear_file(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    Some(pkz::read_sidecar_blob(&bytes).unwrap_or(bytes))
}

/// Every file a gear item is made of — from its folder *and* from the `.pkz` beside it.
///
/// A packed helmet is one file, but a paint installed for it later is a loose `.pnt` in a
/// folder of the same name next to it: that's where the game looks, and it's where this
/// app's paint studio writes one. Reading only whichever of the two the caller resolved
/// meant a folder holding nothing but paints hid the archive entirely — the picker listed
/// the new paint alone and the preview lost the mesh it belongs to. So both are read, the
/// folder's copy winning a name clash because it's the one installed last.
fn read_gear_files(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let stem = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = stem.strip_suffix(".pkz").unwrap_or(&stem);
    let (dir, pkz) = match p.parent() {
        Some(parent) => (parent.join(stem), parent.join(format!("{stem}.pkz"))),
        None => return read_gear_source(p),
    };
    if !dir.is_dir() || !pkz.is_file() {
        return read_gear_source(p);
    }
    // Neither side is allowed to take the other down with it: a `.pkz` that won't open is a
    // reason to show the folder's paints on their own, not to fail the whole item.
    let mut out = read_gear_source(&dir).unwrap_or_default();
    let mut have: Vec<String> = out.iter().map(|(n, _)| gear_entry_key(n)).collect();
    for (name, bytes) in read_gear_source(&pkz).unwrap_or_default() {
        let key = gear_entry_key(&name);
        if !have.contains(&key) {
            have.push(key);
            out.push((name, bytes));
        }
    }
    // Both empty says nothing about *why*; let the caller's own path report it.
    if out.is_empty() {
        return read_gear_source(p);
    }
    Ok(out)
}

/// What two spellings of the same gear file have in common: `helmets/Foo/paints/red.pnt`
/// from an archive and `paints/red.pnt` from the folder beside it are one entry, while
/// `paints/red.pnt` and `goggles/red.pnt` stay two.
fn gear_entry_key(name: &str) -> String {
    let n = name.replace('\\', "/").to_ascii_lowercase();
    let base = n.rsplit('/').next().unwrap_or(&n).to_string();
    match n.rsplit('/').nth(1) {
        Some(folder @ ("paints" | "goggles")) => format!("{folder}/{base}"),
        _ => base,
    }
}

fn read_gear_source(p: &std::path::Path) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::Context;
    if p.is_dir() {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            if path.is_file() {
                if let (Some(name), Some(bytes)) =
                    (path.file_name().and_then(|n| n.to_str()), read_gear_file(&path))
                {
                    out.push((name.to_string(), bytes));
                }
            }
        }
        for sub in ["paints", "goggles"] {
            if let Ok(rd) = std::fs::read_dir(p.join(sub)) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if let (Some(name), Some(bytes)) =
                        (path.file_name().and_then(|n| n.to_str()), read_gear_file(&path))
                    {
                        out.push((format!("{sub}/{name}"), bytes));
                    }
                }
            }
        }
        return Ok(out);
    }
    // A sealed file stays sealed when it's zipped up: pack the Tactical Vest's folder into a
    // `.pkz` — which is what anyone tidying a mods folder does — and every entry comes back
    // as a blob rather than the mesh and paints it holds. Unwrap them the same way the loose
    // files above are unwrapped; anything already plain passes straight through.
    Ok(pkz::read_all(p)?
        .into_iter()
        .map(|(n, d)| {
            let d = pkz::read_sidecar_blob(&d).unwrap_or(d);
            (n, d)
        })
        .collect())
}

struct GearSpec {
    part: &'static str,
    /// Folders under `mods/rider` this slot's models live in. The first is the game's own —
    /// where a new install goes — and any after it are read for what earlier versions of this
    /// app wrote somewhere else. See [`game::PROTECTION_AREAS`].
    mods_kind: &'static [&'static str],
    pkz_kind: &'static str,
    mesh: &'static str,
    default_name: &'static str,
}

const GEAR: [GearSpec; 3] = [
    GearSpec { part: "helmet", mods_kind: &["helmets"], pkz_kind: "helmets", mesh: "helmet.edf", default_name: "default" },
    GearSpec { part: "boots", mods_kind: &["boots"], pkz_kind: "boots", mesh: "boots.edf", default_name: "default" },
    GearSpec { part: "protection", mods_kind: game::PROTECTION_AREAS, pkz_kind: "protections", mesh: "armour.edf", default_name: "full" },
];

/// Everywhere a gear model of this slot could be installed, in the order they're preferred:
/// each of the slot's folders as an unpacked `<model>/` and as a packed `<model>.pkz`.
fn gear_sources(rider: &std::path::Path, spec: &GearSpec, stem: &str) -> Vec<std::path::PathBuf> {
    spec.mods_kind
        .iter()
        .flat_map(|kind| {
            let dir = rider.join(kind);
            [dir.join(stem), dir.join(format!("{stem}.pkz"))]
        })
        .collect()
}

fn load_gear(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    spec: &GearSpec,
    model: &str,
    paint: &str,
    goggles: &str,
    profile: &str,
) -> Option<RiderPart> {
    let stem = model.trim_end_matches(".pkz");
    let sources = gear_sources(base, spec, stem);
    // A goggle paint routinely ships apart from the helmet it's worn with — under the
    // rider profile, or loose beside a `.pkz` the loader can't write into. Gather those so
    // a name the picker offered always resolves to a paint instead of falling back to
    // whichever goggle happened to be packed first.
    let mut extra: Vec<(String, Vec<u8>)> = Vec::new();
    if !goggles.is_empty() {
        if !model.is_empty() {
            for src in &sources {
                extra.extend(loose_paints(&src.join("goggles"), "goggles"));
            }
        }
        if !profile.is_empty() {
            extra.extend(loose_paints(&base.join("riders").join(profile).join("goggles"), "goggles"));
        }
    }

    if !model.is_empty() {
        for (i, src) in sources.iter().enumerate() {
            if !src.exists() {
                continue;
            }
            // The same model can be installed more than one way at once — a paint pack for a
            // packaged mod has nowhere to go but a folder beside it — and only one of them is
            // opened here. Carry the named paint over from the others, so a name the picker
            // offered can't quietly render as whichever paint happened to be first.
            let mut extra = extra.clone();
            for other in sources.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| p) {
                if other.exists() {
                    extra.extend(gear_paint_from(other, "paints", paint));
                    extra.extend(gear_paint_from(other, "goggles", goggles));
                }
            }
            match load_gear_model_blocking(
                src.to_string_lossy().into_owned(),
                spec.part.to_string(),
                Some(paint.to_string()),
                Some(goggles.to_string()),
                // The rider wears what the loadout names; "stock" is a preview-only choice.
                false,
                false,
                extra,
            ) {
                Ok(part) => {
                    log::info!("[rider] {} '{model}' loaded: {} nodes", spec.part, part.nodes.len());
                    return Some(part);
                }
                // Don't silently fall through to stock: a chosen model that fails to parse is a
                // real problem the client's log should show, not a bare head with no trace.
                Err(e) => log::warn!("[rider] {} '{model}' from {src:?} failed: {e}", spec.part),
            }
        }
    }
    // Stock / "free" gear: mesh and paints ship separately in the game pkz, so bind
    // submeshes here too (installed gear is bound in load_gear_model_blocking).
    let name = if model.is_empty() { spec.default_name } else { model };
    let pkz = resolve_game_pkz(cfg, "rider.pkz")?;
    let folder = format!("rider/{}/{}", spec.pkz_kind, name);
    // What the folder says it draws, asked the same way an installed mod is asked. `full`
    // declares two pieces — the chest protector and the neck brace worn with it — so taking
    // only the slot's usual mesh name dressed the rider in half the item.
    let mut drawn: Vec<(String, Vec<edf::EdfNode>)> = gear_scenes(&pkz_gear_cfgs(&pkz, &folder))
        .iter()
        .map(|s| format!("{folder}/{s}"))
        .filter_map(|e| load_pkz_mesh(&pkz, &e).map(|n| (e, n)))
        .collect();
    if drawn.is_empty() {
        let named = format!("{folder}/{}", spec.mesh);
        drawn.push(match load_pkz_mesh(&pkz, &named) {
            Some(n) => (named, n),
            None => {
                let alt = stock_gear_entry(&pkz, &folder)?;
                let n = load_pkz_mesh(&pkz, &alt)?;
                (alt, n)
            }
        });
    }
    let shell_texs = load_pkz_paint(&pkz, &folder, "paints", paint);
    // A stock folder can ship no paint at all — `rider/protections/{full,neck}` carry none —
    // and then the mesh's own textures are the look, exactly as they are for an installed mod
    // that bakes it in. Without this the whole piece came out bare grey.
    let stock_shell = shell_texs.is_empty();
    let mut main_side = GearSide::new(shell_texs.iter().map(|t| t.name.clone()).collect());
    // Stock gear paints its goggles apart from the shell just as an installed helmet does:
    // from the rider profile's own folder where it has one, else from `rider.pkz`.
    let goggle_texs: Vec<paint::PaintTexture> = if goggles.is_empty() {
        Vec::new()
    } else {
        loose_paint_named(&extra, "goggles", goggles)
            .unwrap_or_else(|| load_pkz_paint(&pkz, &folder, "goggles", goggles))
    };
    let goggle_side = GearSide::new(goggle_texs.iter().map(|t| t.name.clone()).collect());
    if !goggles.is_empty() && goggle_side.primary.is_none() {
        log::warn!("[rider] goggle paint '{goggles}' not found for stock {}", spec.part);
    }
    let mut textures: Vec<paint::PaintTexture> =
        shell_texs.into_iter().chain(goggle_texs).collect();
    // The mesh's own materials are what the binder reads to tell one piece from the next, so
    // every source that needs them pays for the archive read — the nodes themselves stay
    // cached. Skipped only when a paint covers the shell and nothing is goggled.
    let need_mesh = stock_shell || !goggles.is_empty();
    let bytes: Vec<Option<Vec<u8>>> = drawn
        .iter()
        .map(|(e, _)| need_mesh.then(|| read_pkz_entry(&pkz, e)).flatten())
        .collect();
    if stock_shell {
        let mut embedded: Vec<paint::PaintTexture> = Vec::new();
        for d in bytes.iter().flatten() {
            for t in paint::extract_edf_textures(d) {
                if !embedded.iter().any(|h| h.name.eq_ignore_ascii_case(&t.name)) {
                    embedded.push(t);
                }
            }
        }
        main_side = GearSide::new(
            embedded.iter().map(|t| t.name.clone()).filter(|n| !is_goggle_name(n)).collect(),
        );
        if main_side.primary.is_none() {
            log::warn!("[rider] stock {} '{name}' has no texture of its own", spec.part);
        }
        // The goggle side keeps its own `.pnt`; a paint reuses the mesh's names, so only what
        // it doesn't supply comes off the mesh.
        let taken: std::collections::HashSet<String> =
            textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        textures
            .extend(embedded.into_iter().filter(|t| !taken.contains(&t.name.to_ascii_lowercase())));
    }
    // The stock model's own paints are what it declares — both sides' names are already in
    // hand here, decoded from `rider.pkz` above.
    let declared: Vec<String> =
        main_side.names.iter().chain(goggle_side.names.iter()).cloned().collect();
    let mut nodes = Vec::new();
    for ((_, mut n), d) in drawn.into_iter().zip(&bytes) {
        bind_gear_submeshes(&mut n, d.as_deref(), &main_side, &goggle_side, &declared);
        nodes.extend(n);
    }
    log::info!(
        "[rider] {} stock '{name}' loaded: {} nodes, tex={:?} goggles={:?}",
        spec.part,
        nodes.len(),
        main_side.primary,
        goggle_side.primary,
    );
    Some(RiderPart {
        part: spec.part.into(),
        nodes,
        textures,
    })
}

/// The small text files a stock gear folder ships — `gfx.cfg` and the `.hrc`s it names —
/// read out of the game archive in the shape [`gear_scenes`] reads an installed mod's folder.
/// A few hundred bytes each, against a 100 MB archive, so ask before guessing at mesh names.
fn pkz_gear_cfgs(pkz: &std::path::Path, folder: &str) -> Vec<(String, Vec<u8>)> {
    let prefix = format!("{}/", folder.replace('\\', "/").to_ascii_lowercase());
    pkz::read_selected(pkz, |n| {
        let n = n.replace('\\', "/").to_ascii_lowercase();
        n.starts_with(&prefix) && (n.ends_with(".cfg") || n.ends_with(".hrc"))
    })
    .unwrap_or_default()
}

/// The `.edf` a stock gear folder in `rider.pkz` actually carries, for the folders that
/// don't answer to the slot's usual name.
///
/// Protection is where this bites: the slot expects `armour.edf`, which is the chest
/// protector's name — the neck brace beside it is its own mesh, and asking for a name that
/// isn't there left the slot silently empty rather than wrong, which is harder to notice.
/// Only reached once the expected name has already missed.
fn stock_gear_entry(pkz: &std::path::Path, folder: &str) -> Option<String> {
    let prefix = format!("{}/", folder.replace('\\', "/").to_ascii_lowercase());
    let mut found: Vec<String> = pkz::read_selected(pkz, |n| {
        let n = n.replace('\\', "/").to_ascii_lowercase();
        n.starts_with(&prefix) && is_visible_gear_mesh(&n)
    })
    .ok()?
    .into_iter()
    .map(|(n, _)| n.replace('\\', "/"))
    .collect();
    // Deterministic rather than archive-order, so the same folder always resolves the same.
    found.sort_by_key(|n| n.to_ascii_lowercase());
    let hit = found.into_iter().next()?;
    log::info!("[rider] stock '{folder}' doesn't carry the slot's mesh; using '{hit}'");
    Some(hit)
}

/// One named `.pnt` out of a gear source — an unpacked folder or a `.pkz`, whichever it is —
/// named the way a gear archive carries it so the loader reads it like any packed paint.
///
/// Just the one paint: a helmet folder holds dozens, and this runs to cover the *other*
/// install of a model the loader already has open, so reading them all would be paying a
/// hundred megabytes for a file it needs one of. No name wanted, or a source that doesn't
/// carry it → nothing, and the loader falls back exactly as it did before.
fn gear_paint_from(src: &std::path::Path, sub: &str, want: &str) -> Vec<(String, Vec<u8>)> {
    if want.is_empty() {
        return Vec::new();
    }
    let entry = format!("{sub}/{want}.pnt");
    if src.is_dir() {
        return std::fs::read(src.join(sub).join(format!("{want}.pnt")))
            .map(|d| vec![(entry, d)])
            .unwrap_or_default();
    }
    pkz::read_selected(src, |n| {
        gear_folder_paint_name(n, sub).is_some_and(|p| p.eq_ignore_ascii_case(want))
    })
    .map(|hits| hits.into_iter().map(|(_, d)| (entry.clone(), d)).collect())
    .unwrap_or_default()
}

/// Loose `.pnt` files in a folder, named as a gear archive would carry them so the loader
/// reads them exactly like packed ones. Missing folder → nothing, which is the norm.
fn loose_paints(dir: &std::path::Path, folder: &str) -> Vec<(String, Vec<u8>)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pnt")))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            Some((format!("{folder}/{name}"), std::fs::read(&p).ok()?))
        })
        .collect()
}

/// Decode one named paint out of a gathered set — no fallback: a miss means the caller
/// should look elsewhere, not that some other paint will do.
fn loose_paint_named(
    files: &[(String, Vec<u8>)],
    folder: &str,
    want: &str,
) -> Option<Vec<paint::PaintTexture>> {
    let hit = files.iter().find(|(n, _)| {
        gear_folder_paint_name(n, folder).is_some_and(|p| p.eq_ignore_ascii_case(want))
    })?;
    Some(paint::decode_any(&hit.1).ok()?.into_par_iter().map(paint::into_texture).collect())
}

/// A paint the game itself ships, from `<folder>/<sub>/<paint>.pnt` — `sub` being `paints`
/// for the piece's own look or `goggles` for the goggles worn with it.
///
/// A name that misses falls back to the first paint in the folder, so a stale name still shows
/// the piece textured rather than bare grey. *No name* is a different answer and must not reach
/// that fallback: an empty slot is the loadout saying "the model's own look", and dressing it in
/// whichever `.pnt` sorts first is how the stock helmet came out bronze — `black_yellow` leads
/// `rider/helmets/default/paints/`, while the mesh's own sheet is white. Nothing here, and the
/// caller falls through to the mesh's embedded textures, which is what stock means.
fn load_pkz_paint(
    pkz: &std::path::Path,
    folder: &str,
    sub: &str,
    paint: &str,
) -> Vec<paint::PaintTexture> {
    if paint.is_empty() {
        return Vec::new();
    }
    read_pkz_entry(pkz, &format!("{folder}/{sub}/{paint}.pnt"))
        .or_else(|| read_pkz_first(pkz, &format!("{folder}/{sub}/"), ".pnt"))
        .and_then(|d| paint::decode_any(&d).ok())
        .map(|p| p.into_par_iter().map(paint::into_texture).collect())
        .unwrap_or_default()
}

fn read_pkz_first(pkz: &std::path::Path, prefix: &str, ext: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(pkz).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut hit = None;
    for i in 0..zip.len() {
        let f = zip.by_index(i).ok()?;
        let n = f.name().replace('\\', "/");
        if n.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())
            && n.to_ascii_lowercase().ends_with(ext)
        {
            hit = Some(i);
            break;
        }
    }
    let mut f = zip.by_index(hit?).ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut f, &mut buf).ok()?;
    Some(buf)
}

fn load_rider_paint(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    part: &str,
    profile: &str,
    sub: &str,
    paint: &str,
) -> Option<RiderPart> {
    if paint.is_empty() {
        return None;
    }
    // With no profile picked the body mesh already falls back to the stock rider
    // (`load_rider_body_nodes`); do the same here so a chosen suit/glove paint still
    // resolves instead of silently dropping off the preview.
    let profile = rider_profile_or_stock(profile);
    let data = read_rider_paint_file(cfg, base, profile, sub, paint)?;
    let textures: Vec<_> =
        paint::decode_any(&data).ok()?.into_par_iter().map(paint::into_texture).collect();
    if textures.is_empty() {
        return None;
    }
    Some(RiderPart {
        part: part.into(),
        nodes: Vec::new(),
        textures,
    })
}

/// A kit or glove paint for `profile`, by exact name.
///
/// A rider model isn't a wardrobe. Rider+ ships its `paints` and `gloves` folders empty on
/// purpose — the kits already installed under the stock profile are meant to work on it —
/// so looking only inside the chosen profile drops every paint the picker offered. Look in
/// the profile's own folder, then inside its archive where it's packed, then under the
/// stock profiles.
///
/// Exact name at every step, and never the first paint in a folder: reaching past the
/// chosen profile is only safe while the name still means the same paint.
fn read_rider_paint_file(
    cfg: &config::AppConfig,
    base: &std::path::Path,
    profile: &str,
    sub: &str,
    paint: &str,
) -> Option<Vec<u8>> {
    let riders = base.join("riders");
    let game = resolve_game_pkz(cfg, "rider.pkz");
    let candidates =
        std::iter::once(profile).chain(STOCK_RIDER_PROFILES.into_iter().filter(|s| *s != profile));

    for from in candidates {
        // Installed loose, then packed as `<profile>.pkz`, then — for a stock profile, whose
        // kits ship inside the game archive and never touch the disk — the game's own copy.
        let hit = read_paint_file(&riders.join(from).join(sub), paint)
            .or_else(|| {
                let packed = riders.join(format!("{from}.pkz"));
                packed.is_file().then(|| read_pkz_paint_named(&packed, sub, paint)).flatten()
            })
            .or_else(|| {
                let pkz = game.as_ref()?;
                read_pkz_entry(pkz, &format!("rider/riders/{from}/{sub}/{paint}.pnt"))
            });
        if let Some(d) = hit {
            if from != profile {
                log::info!("[rider] {sub} '{paint}' for '{profile}' came from '{from}'");
            }
            return Some(d);
        }
    }
    None
}

/// A named paint out of an archive, matched on its folder as well as its name — `red.pnt`
/// under `gloves` is not the `red.pnt` under `paints`.
fn read_pkz_paint_named(pkz: &std::path::Path, sub: &str, paint: &str) -> Option<Vec<u8>> {
    let tail = format!("/{}/{}.pnt", sub.to_ascii_lowercase(), paint.to_ascii_lowercase());
    let want = |n: &str| n.replace('\\', "/").to_ascii_lowercase().ends_with(&tail);
    pkz::read_selected(pkz, want).ok()?.into_iter().next().map(|(_, d)| d)
}

fn read_paint_file(dir: &std::path::Path, paint: &str) -> Option<Vec<u8>> {
    if !paint.is_empty() {
        return std::fs::read(dir.join(format!("{paint}.pnt"))).ok();
    }
    let first = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pnt")))?;
    std::fs::read(first).ok()
}

#[tauri::command]
async fn add_to_library(
    app: tauri::AppHandle,
    slug: String,
    url: String,
    host: String,
    subpath: String,
    dest_folder: String,
) -> Result<(), String> {
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    install::add_to_library(&app, &cfg, &slug, &url, &host, &subpath, &dest_folder)
        .await
        .map_err(|e| format!("{e:#}"))
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
        library::uninstall_mod(&cfg.mods_path, &from_path, &subpath).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("uninstall_mod task failed: {e}"))?
}

#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    library::reveal_in_explorer(&path).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn set_game_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.game_path = path;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
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
    // The watcher is pinned to a folder that just moved; re-point it, and publish in case
    // the new folder's look differs from what the old one last sent.
    if cfg.experimental_enabled() {
        let profiles = app.state::<ProfileWatcher>();
        profilewatch::start(&app, &profiles, &cfg.profiles_dir());
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

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
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

/// Start MX Bikes from the Play button in the sidebar.
#[tauri::command]
fn launch_game(app: tauri::AppHandle) -> Result<gameproc::LaunchOutcome, String> {
    // `load_or_detect`, not `load`: a missing config file shouldn't turn Play into an
    // error when the install is sitting exactly where the detector looks.
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let outcome = gameproc::launch(&cfg).map_err(|e| format!("{e:#}"))?;
    if matches!(outcome, gameproc::LaunchOutcome::Launched) {
        // Both directions, because Play is the last moment before either one matters: the
        // grid needs everyone else's paints on disk, and everyone else needs ours.
        publish_paints_soon(&app, &cfg, None);
        // No address to aim at — they'll pick from the in-game browser — so this covers
        // the whole registry.
        sync_paints_soon(&app, None);
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

/// Whether the unfinished multiplayer features should be shown, and whether this build is
/// a pre-release. The frontend gates the Servers tab and the paint-sync UI on the first and
/// badges the version with the second.
#[tauri::command]
fn experimental_state(app: tauri::AppHandle) -> serde_json::Value {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    let version = release_version(app.package_info().version.to_string());
    serde_json::json!({
        "enabled": cfg.experimental_enabled(),
        // Set by the env var rather than the setting, so the UI can explain why the toggle
        // looks stuck on.
        "forcedByEnv": std::env::var(config::EXPERIMENTAL_ENV).map(|v| v == "1").unwrap_or(false),
        "version": version,
        // A semver pre-release suffix (`0.8.0-beta.1`) is what the release workflow uses to
        // mark a build as a pre-release, so it is also what makes this build a beta.
        "prerelease": version.contains('-'),
        "enrolled": !cfg.cp_token.trim().is_empty(),
        "riderName": cfg.cp_rider_name,
        "guid": cfg.cp_guid,
        // What paint sync last managed, so the panel can say so on a cold start rather than
        // showing nothing until something happens to run.
        "sync": cfg.sync,
        // Whether there is a profile to publish at all. A rider name that matches no profile
        // on disk publishes nothing, silently, and that is worth saying out loud.
        "profile": sync_profile(&cfg),
    })
}

/// Turn the experimental features on or off.
#[tauri::command]
fn set_experimental(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.experimental = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;

    // Bring paint sync up — or take it down — with the switch that owns it.
    //
    // Both of these are otherwise only decided in `.setup`, so turning the feature on left
    // the profile watcher stopped until the next restart: the app would keep publishing on
    // an apply or a launch, but a look changed in the game's garage went unnoticed for the
    // whole session. That is the session a player has just enrolled in, which makes it the
    // worst one to be quietly missing.
    let watcher = app.state::<ProfileWatcher>();
    if cfg.experimental_enabled() {
        profilewatch::start(&app, &watcher, &cfg.profiles_dir());
        // And publish once now, since nothing has been watching until this moment.
        publish_paints_soon(&app, &cfg, None);
    } else {
        profilewatch::stop(&watcher);
    }
    Ok(())
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

/// Register `guid` against this account and remember it locally.
///
/// Shared by the manual field and the automatic claim off a server roster, so both go
/// through the same validation and land in the same place.
async fn claim_guid(app: &tauri::AppHandle, guid: &str) -> Result<(), String> {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .put(format!("{}/v1/me/guid", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .json(&serde_json::json!({ "guid": guid.trim() }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if !resp.status().is_success() {
        let detail = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&detail)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(detail));
    }

    // Re-read rather than reusing the config above: the round trip is long enough for
    // something else to have written it.
    let mut cfg = config::load_or_detect(app).unwrap_or_default();
    cfg.cp_guid = guid.trim().to_string();
    config::save(app, &cfg).map_err(|e| format!("{e:#}"))
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
    if !cfg.experimental_enabled() || cfg.cp_token.trim().is_empty() {
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
        if !cfg.experimental_enabled() || cfg.cp_token.trim().is_empty() {
            return;
        }
        let Some(profile) = profile.or_else(|| sync_profile(&cfg)) else {
            return;
        };
        let known = cfg.sync.published_digest.clone();
        emit_sync(&app, SyncEvent::phase("publishing"));
        match paintsync::publish_all(&cfg, &cfg.cp_token, &profile, Some(known.as_str())).await {
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
        if !cfg.experimental_enabled() {
            return;
        }
        emit_sync(&app, SyncEvent::phase("pulling"));
        match pull_rosters(&app, address).await {
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

/// Pull the rosters for wherever the player is, or could be, riding.
///
/// `address` narrows this to a single server when we know where they're headed. Without
/// one it covers the whole registry, which is the best available answer when the server is
/// chosen from the in-game browser and never passes through us.
async fn pull_rosters(
    app: &tauri::AppHandle,
    address: Option<String>,
) -> Result<paintsync::PullOutcome, String> {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    // An unreachable registry doesn't have to sink a targeted sync — the address is a
    // usable key on its own — but with no address there's nothing left to aim at.
    let registry = match paintsync::registry(Some(&cfg.cp_token)).await {
        Ok(list) => list,
        Err(e) => {
            log::warn!("[sync] couldn't read the server registry: {e:#}");
            Vec::new()
        }
    };

    let keys: Vec<String> = match &address {
        Some(addr) => vec![paintsync::server_key_for(&registry, addr)],
        None => registry.iter().map(|s| s.id.clone()).collect(),
    };
    if keys.is_empty() {
        return Err("No servers to sync with yet.".into());
    }

    let outcome = paintsync::pull(&cfg, &cfg.cp_token, &keys)
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
    // mods folder.
    let _ = frostmod::signal_reload();
    Ok(outcome)
}

/// Install every other rider's paints, so the grid renders correctly.
///
/// The app does this on its own at launch; this is the manual retry for when that ran
/// before a rider had published, or failed on a flaky connection.
#[tauri::command]
async fn sync_paints(app: tauri::AppHandle) -> Result<paintsync::PullOutcome, String> {
    emit_sync(&app, SyncEvent::phase("pulling"));
    match pull_rosters(&app, None).await {
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
        publish_paints_soon(&app, &cfg, None);
        // We know exactly where they're going, so this syncs that server alone.
        sync_paints_soon(&app, Some(address));
    }
    Ok(outcome)
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
#[tauri::command]
async fn frostmod_install_runtime(
    app: tauri::AppHandle,
    runtime: vcruntime::Runtime,
) -> Result<vcruntime::InstallOutcome, String> {
    vcruntime::install(&app, runtime)
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
        move || install::extract_and_place(&app, &cfg, &slug, &archive, &work, &subpath, &dest_folder)
            .map(|()| dest_folder)
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
    presets::save_preset(&presets_dir(&app)?, preset).map_err(|e| format!("{e:#}"))
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

/// WebKitGTK renders through DMA-BUF by default, which asks the WebKit our AppImage carries
/// from Ubuntu 22.04 to negotiate buffers with whatever Mesa/EGL the host happens to ship.
/// On SteamOS that negotiation fails silently: the window appears, the web process never
/// paints, and the user is left looking at a white rectangle with nothing on stdout to say
/// why. The shared-memory fallback costs a copy per frame — imperceptible on a UI that is
/// mostly static lists — and paints everywhere.
///
/// A default, not an override: an explicit `WEBKIT_DISABLE_DMABUF_RENDERER=0` still wins, so
/// a machine whose driver stack handles the fast path can ask for it back.
///
/// Has to run before the first window is built, since WebKit reads this when it spawns the
/// web process, and before any other thread exists — being `main`'s first statement gives
/// both.
fn prepare_webview_env() {
    if cfg!(target_os = "linux") && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

fn main() {
    prepare_webview_env();

    let builder = tauri::Builder::default();

    // One app, one process. Closing the window parks MXB App in the tray rather than
    // quitting it, so without this a second launch doesn't reveal the copy already
    // running — it builds a whole new one: another window, another tray icon, another
    // FrostMod, another mod watcher. Five launches in a day left five of everything, and
    // only the tray overflow to clean them up from.
    //
    // Registered before every other plugin: the guard's setup hook is what kills the
    // second process, and it should do so before anything else has started work that
    // would then need unwinding. `show_main` is the same path the tray's "Show MXB App"
    // takes, so relaunching behaves exactly like clicking the tray icon.
    //
    // Release builds only, for the same reason close-to-tray is (see `CloseRequested`
    // below): a `tauri dev` run must still start while the installed MXB App is sitting
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
        .manage(CloudServers::default())
        .manage(shop_session::ShopSession::default())
        .setup(|app| {
            log::info!("MXB App {} starting", env!("CARGO_PKG_VERSION"));
            // Cloudflare scores the User-Agent alongside the IP, and a cf_clearance is bound
            // to the UA that earned it — a log about a block should say which one was used.
            log::info!("{} user-agent: {}", mxb_session::site().domain, mxb_session::UA);
            // A blank webview leaves nothing else behind to diagnose from, so record which
            // renderer path this run took. See `prepare_webview_env`.
            if cfg!(target_os = "linux") {
                log::info!(
                    "webview dmabuf renderer disabled: {}",
                    std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").unwrap_or_default() == "1"
                );
            }
            if let Ok(dir) = app.path().app_local_data_dir() {
                log::info!("data dir (config/session/frostmod): {}", dir.display());
            }
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

            let show = MenuItem::with_id(app, "show", "Show MXB App", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("MXB App")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => {
                        frostmod_manage::stop(&app.state::<FrostmodProcess>());
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
                let enabled = manager.is_enabled().unwrap_or(false);
                if cfg.launch_at_startup && !enabled {
                    let _ = manager.enable();
                } else if !cfg.launch_at_startup && enabled {
                    let _ = manager.disable();
                }
                if cfg.auto_run_frostmod && frostmod_manage::is_installed(handle) {
                    let state = handle.state::<FrostmodProcess>();
                    let _ = frostmod_manage::start(handle, &state);
                }
                if cfg.watch_mods_reload {
                    let watcher = handle.state::<ModWatcher>();
                    modwatch::start(handle, &watcher, &cfg.mods_path);
                }
                // Paint sync, both directions, from the moment the app opens:
                //  * publish, because the look may have changed in the game's garage while
                //    the app was shut, and nothing would ever have noticed;
                //  * watch, so the same change during this session is noticed as it happens.
                // Both no-op unless the experimental features are on and an account exists.
                if cfg.experimental_enabled() {
                    publish_paints_soon(handle, &cfg, None);
                    let profiles = handle.state::<ProfileWatcher>();
                    profilewatch::start(handle, &profiles, &cfg.profiles_dir());
                }
                // A combo another app already owns shouldn't stop the app from starting
                // — Settings reports the state and lets the player pick another.
                if let Err(e) = overlay::register(handle, &cfg) {
                    log::warn!("overlay hotkey not registered: {e}");
                }
            } else {
                log::info!("no MX Bikes folder found — showing first-run setup");
            }
            shop_session::load_session(handle);
            shop_catalog_session::load(handle);
            mxb_session::load(handle);
            imgcache::start_maintenance(handle);
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
                // Never on Linux: the tray runs through libayatana-appindicator, which
                // doesn't deliver click events to Tauri and isn't present at all on a
                // stock GNOME desktop. Hiding there can strand the window with no way
                // back, so closing closes.
                let tray_can_restore = cfg!(not(target_os = "linux"));
                if cfg.run_in_background && tray_can_restore && !cfg!(debug_assertions) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        // Wrapped rather than passed straight in: `ipc_allowed` closes the hole that giving
        // the hidden mxb-mods.com window a capability would otherwise open. See its doc.
        .invoke_handler({
            // The macro is generic over the runtime; naming `Wry` here is what lets the
            // wrapper below infer what it is wrapping.
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
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
            get_pkz_meta_cached,
            get_pkz_meta,
            get_pkz_preview,
            unpack_paint,
            texture_bytes,
            unpack_pkz,
            load_bike_model,
            load_rider_model,
            load_rider_body_model,
            load_gear_model,
            load_stock_gear_model,
            list_gear_paints,
            list_installed_gear_paints,
            paint_studio_load,
            paint_studio_target,
            paint_studio_save,
            paint_studio_extract,
            paint_studio_hints,
            scan_rider_targets,
            scan_gear_repairs,
            repair_gear_area,
            scan_bike_targets,
            scan_model_swaps,
            apply_model_swap,
            scan_sound_swaps,
            apply_sound_swap,
            bind_sound,
            unbind_sound,
            reshade_status,
            apply_reshade_preset,
            delete_reshade_preset,
            detect_loose_swaps,
            register_loose_swaps,
            detect_orphaned_setup,
            repair_orphaned_setup,
            add_to_library,
            import_file,
            plan_drop,
            repreview_drop,
            commit_drop,
            cancel_drop,
            move_mod,
            uninstall_mod,
            reveal_in_explorer,
            set_game_path,
            set_mods_path,
            set_intro_seen,
            set_seen_version,
            set_profiles_path,
            detect_game_path,
            count_profiles_in,
            get_mods_root,
            set_run_in_background,
            set_launch_at_startup,
            set_auto_run_frostmod,
            set_instant_refresh,
            overlay_toggle,
            overlay_hide,
            overlay_open_main,
            overlay_state,
            set_overlay_enabled,
            set_overlay_hotkey,
            set_watch_mods_reload,
            frostmod_reload,
            frostmod_running,
            garage_scan_bikes,
            garage_swap_bike,
            frostmod_status,
            frostmod_install,
            frostmod_install_runtime,
            frostmod_start,
            frostmod_stop,
            launch_game,
            join_server,
            experimental_state,
            set_experimental,
            enroll_account,
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
            shop_catalog_available,
            shop_catalog_status,
            shop_catalog_categories,
            shop_catalog_search,
            shop_catalog_detail,
            shop_catalog_refresh,
            presets_list_profiles,
            presets_list_bikes,
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
            preset_bundle_stats,
            preset_bundle_create,
            preset_bundle_import,
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
        for label in [MAIN_WINDOW, overlay::LABEL, SHOP_LOGIN_WINDOW] {
            for command in ["create_config", "install_mod", "plugin:event|emit"] {
                assert!(ipc_allowed(label, command), "{label} / {command}");
            }
        }
    }
}

#[cfg(test)]
mod gear_bind_tests {
    use super::{bind_gear_submeshes, edf, GearSide};

    fn submesh(name: &str) -> edf::Submesh {
        edf::Submesh {
            name: name.into(),
            tri_start: 0,
            tri_count: 1,
            texture: None,
            uv_tile: None,
            mat: None,
        }
    }

    fn node(name: &str, subs: &[&str]) -> edf::EdfNode {
        edf::EdfNode {
            name: name.into(),
            positions: Vec::new(),
            uvs: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            submeshes: subs.iter().map(|s| submesh(s)).collect(),
            texture: None,
            placed: false,
            materials: Vec::new(),
        }
    }

    fn side(names: &[&str]) -> GearSide {
        GearSide::new(names.iter().map(|s| s.to_string()).collect())
    }

    fn bound(nodes: &[edf::EdfNode]) -> Vec<Option<String>> {
        nodes
            .iter()
            .flat_map(|n| {
                if n.submeshes.is_empty() {
                    vec![n.texture.clone()]
                } else {
                    n.submeshes.iter().map(|s| s.texture.clone()).collect()
                }
            })
            .collect()
    }

    // Without the mesh's materials to read, the names are all there is to go on.
    #[test]
    fn a_named_goggle_submesh_wears_the_goggle_paint() {
        let mut nodes = vec![node("helmet", &["shell", "goggle", "lens"])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &side(&["smoke"]), &[]);
        assert_eq!(
            bound(&nodes),
            [Some("hjc".into()), Some("smoke".into()), Some("smoke".into())],
        );
    }

    // The goggles of many helmets are a node of their own, with no submesh table at all —
    // the case that had every goggle paint land on the shell instead.
    #[test]
    fn a_goggle_node_without_submeshes_wears_the_goggle_paint() {
        let mut nodes = vec![node("helmet", &[]), node("goggles", &[])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &side(&["smoke"]), &[]);
        assert_eq!(bound(&nodes), [Some("hjc".into()), Some("smoke".into())]);
    }

    // A goggle group inside a goggle node needn't repeat the word.
    #[test]
    fn a_submesh_inherits_its_node() {
        let mut nodes = vec![node("goggles", &["strap", "glass"])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &side(&["smoke"]), &[]);
        assert_eq!(bound(&nodes), [Some("smoke".into()), Some("smoke".into())]);
    }

    // A helmet that draws its goggles into the shell atlas still shows them.
    #[test]
    fn unpainted_goggles_fall_back_to_the_shell() {
        let mut nodes = vec![node("helmet", &["shell", "goggle"])];
        bind_gear_submeshes(&mut nodes, None, &side(&["hjc"]), &GearSide::default(), &[]);
        assert_eq!(bound(&nodes), [Some("hjc".into()), Some("hjc".into())]);
    }

    // The shell never borrows the goggle paint the other way round — that's the smear.
    #[test]
    fn an_unpainted_shell_stays_bare() {
        let mut nodes = vec![node("helmet", &["shell", "goggle"])];
        bind_gear_submeshes(&mut nodes, None, &GearSide::default(), &side(&["smoke"]), &[]);
        assert_eq!(bound(&nodes), [None, Some("smoke".into())]);
    }

    // A paint replaces the mesh's textures by name, so where it supplies the exact name a
    // piece asks for, that beats the side's primary.
    #[test]
    fn a_side_supplies_the_name_the_mesh_asks_for() {
        let s = side(&["shell_n", "shell", "strap"]);
        assert_eq!(s.primary.as_deref(), Some("shell")); // never the companion map
        assert_eq!(s.supplies("STRAP").as_deref(), Some("strap")); // case-insensitive
        assert_eq!(s.supplies("visor"), None);
    }

    // A paint baked out of Substance names its maps the exporter's way, and one of those
    // taken for the look leaves the piece wearing a normal map.
    #[test]
    fn an_exporter_named_map_is_never_the_primary() {
        let s = side(&["Vest_Normal", "Vest_BaseColor"]);
        assert_eq!(s.primary.as_deref(), Some("Vest_BaseColor"));
    }
}

/// A packed gear item and a folder of the same name are one item, not two — the case the
/// paint studio creates every time it installs a paint for a `.pkz` helmet.
#[cfg(test)]
mod gear_source_tests {
    use super::{gear_entry_key, read_gear_files};

    #[test]
    fn one_entry_however_it_is_spelled() {
        assert_eq!(gear_entry_key("helmets/Foo/paints/red.pnt"), gear_entry_key("paints/red.pnt"));
        assert_eq!(gear_entry_key("helmets/Foo/helmet.edf"), gear_entry_key("helmet.edf"));
        assert_ne!(gear_entry_key("paints/red.pnt"), gear_entry_key("goggles/red.pnt"));
    }

    #[test]
    fn a_folder_of_paints_beside_a_pkz_reads_as_both() {
        let root = std::env::temp_dir().join(format!("frost-gear-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Helmet");
        std::fs::create_dir_all(dir.join("paints")).unwrap();
        std::fs::write(dir.join("paints").join("mine.pnt"), b"PNT\0mine").unwrap();
        // Not a real archive — `pkz::read_all` returning nothing is enough to prove the
        // folder's paint isn't lost, and a genuine `.pkz` is exercised by the pkz tests.
        std::fs::write(root.join("Helmet.pkz"), b"not really an archive").unwrap();

        for from in [dir.clone(), root.join("Helmet.pkz")] {
            let files = read_gear_files(&from).unwrap_or_default();
            assert!(
                files.iter().any(|(n, _)| n.ends_with("mine.pnt")),
                "the loose paint has to be visible whichever side the caller resolved ({from:?})"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod gear_scene_tests {
    use super::gear_scenes;

    fn files(entries: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
        entries.iter().map(|(n, d)| (n.to_string(), d.as_bytes().to_vec())).collect()
    }

    // A protection folder names its mesh for the piece, not the slot, so the loader has to
    // ask rather than guess: `neckbrace.edf` sits beside the shadow mesh it must not pick.
    #[test]
    fn a_mod_names_its_own_mesh() {
        let f = files(&[
            ("gfx.cfg", "neckbrace\n{\n\tmodel = neckbrace.hrc\n}\n"),
            ("neckbrace.hrc", "level0\n{\n\tscene = neckbrace.edf\n\tswitch = 0\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["neckbrace.edf"]);
    }

    // The block in `gfx.cfg` is named for the piece, and authors don't agree on the word —
    // `armour` and `neckbrace` both turn up on protection mods that ship `protection.edf`.
    #[test]
    fn the_block_name_doesnt_matter() {
        let f = files(&[
            ("gfx.cfg", "armour\n{\n\tmodel = protection.hrc\n}\n"),
            ("protection.hrc", "level0\n{\n\tscene = protection.edf\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["protection.edf"]);
    }

    // The stock `full` protection: two pieces worn together, each its own mesh. Taking one
    // block's mesh dressed the rider in half the item.
    #[test]
    fn a_two_piece_set_draws_both_pieces() {
        let f = files(&[
            (
                "gfx.cfg",
                "neckbrace\n{\n\tmodel = neckbrace.hrc\n}\n\narmour\n{\n\tmodel = armour.hrc\n}\n",
            ),
            ("armour.hrc", "level0\n{\n\tscene = armour.edf\n}\n"),
            ("neckbrace.hrc", "level0\n{\n\tscene = neckbrace.edf\n}\n"),
        ]);
        // Sorted by block name, so the same folder always draws in the same order.
        assert_eq!(gear_scenes(&f), ["armour.edf", "neckbrace.edf"]);
    }

    // Boots declare a left and a right, both pointing at the one mesh that holds both feet.
    // A repeat is one piece, not two — and the nested `model { file = … }` is the boots'
    // own spelling, which nothing else uses.
    #[test]
    fn two_blocks_naming_one_mesh_are_one_piece() {
        let f = files(&[
            (
                "gfx.cfg",
                "left\n{\n\tmodel\n\t{\n\t\tfile = left_boot.hrc\n\t}\n}\nright\n{\n\tmodel\n\t{\n\t\tfile = right_boot.hrc\n\t}\n}\n",
            ),
            ("left_boot.hrc", "level0\n{\n\tscene = boots.edf\n}\n"),
            ("right_boot.hrc", "level0\n{\n\tscene = boots.edf\n\tname = righta\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["boots.edf"]);
    }

    // A helmet names its mesh at the top level and its first-person mesh in `cockpit`. The
    // cockpit one is never drawn on the model, and picking it up put a headless shell on the
    // rider.
    #[test]
    fn the_cockpit_mesh_is_not_the_item() {
        let f = files(&[
            ("gfx.cfg", "model = helmet.hrc\nshadow = helmet_s.edf\n\ncockpit\n{\n\tmodel = c_helmet.edf\n}\n"),
            ("helmet.hrc", "level0\n{\n\tscene = helmet.edf\n}\n"),
        ]);
        assert_eq!(gear_scenes(&f), ["helmet.edf"]);
    }

    // Most gear ships nothing to read, and that's not an error — the loader falls back to
    // scanning the folder for a mesh.
    #[test]
    fn a_mod_that_says_nothing_answers_nothing() {
        assert!(gear_scenes(&files(&[("protection.edf", "EDF\0")])).is_empty());
    }
}

#[cfg(test)]
mod gear_paint_merge_tests {
    use super::GearPaints;

    fn paints(names: &[&str]) -> GearPaints {
        GearPaints {
            paints: names.iter().map(|s| s.to_string()).collect(),
            ..GearPaints::default()
        }
    }

    // The whole point: a model installed as a `.pkz` *and* as a folder of extra paints
    // offers both sets. Taking the first source is what showed one and hid the other.
    #[test]
    fn sources_add_up() {
        let mut all = paints(&["Black", "Red"]);
        all.absorb(paints(&["Purple White", "RDS Leopard"]));
        all.sort();
        assert_eq!(all.paints, ["Black", "Purple White", "RDS Leopard", "Red"]);
    }

    // A paint pack ships the same file names as the mod it was made for, so the same look
    // arrives twice. Twice in a dropdown is a bug, not a choice.
    #[test]
    fn a_repeat_is_the_same_look_twice() {
        let mut all = paints(&["Black", "Red"]);
        all.absorb(paints(&["black", "Flo"]));
        all.sort();
        assert_eq!(all.paints, ["Black", "Flo", "Red"]);
    }

    // Each source sorts its own names; merged, they have to sort as one list rather than
    // as one source appended to the next.
    #[test]
    fn the_merged_list_sorts_as_one() {
        let mut all = paints(&["Alpha", "Zulu"]);
        all.absorb(paints(&["Bravo"]));
        all.sort();
        assert_eq!(all.paints, ["Alpha", "Bravo", "Zulu"]);
    }

    // A "Stock" entry is worth offering as soon as any source's mesh carries its own look.
    #[test]
    fn stock_carries_across() {
        let mut all = GearPaints::default();
        all.absorb(GearPaints { has_stock: true, ..GearPaints::default() });
        assert!(all.has_stock);
        all.absorb(GearPaints::default());
        assert!(all.has_stock, "a later source without one doesn't take it away");
    }
}

/// Whether a side has a stock look at all — the question behind both the library's "Stock"
/// entry and what an empty paint slot resolves to on the rider. They read the same function,
/// so these cases pin down both at once.
#[cfg(test)]
mod stock_side_tests {
    use super::mesh_supplies_side;

    fn v(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // The ordinary helmet: the mesh carries the sheet its paints replace, so there is a stock
    // look to show and an empty slot means it.
    #[test]
    fn the_mesh_carrying_the_painted_sheet_is_a_stock_look() {
        assert!(mesh_supplies_side(&v(&["helmet", "visor"]), &v(&["helmet"]), false));
    }

    // Case is the mod author's business, not ours — a `.pnt` replaces by name regardless.
    #[test]
    fn the_match_ignores_case() {
        assert!(mesh_supplies_side(&v(&["Helmet"]), &v(&["helmet"]), false));
    }

    // The Bell Moto 10: it embeds a tear-off film and a goggle, but the shell sheet comes from
    // its paints. Calling that a stock look drew the helmet near-blank — and now it would also
    // make an empty slot render bare, which is worse than the first-paint fallback it keeps.
    #[test]
    fn a_mesh_that_leaves_the_shell_to_its_paints_has_no_stock_look() {
        assert!(!mesh_supplies_side(&v(&["tearoff", "Racecraft"]), &v(&["airoh_shell"]), false));
    }

    // Normal and roughness maps are never the look. A mesh carrying only companions carries
    // nothing to show.
    #[test]
    fn companion_maps_dont_count_as_a_look() {
        assert!(!mesh_supplies_side(&v(&["boots_n", "boots_r"]), &v(&[]), false));
        assert!(!mesh_supplies_side(&v(&["helmet_n"]), &v(&["helmet_n"]), false));
    }

    // Nothing painted on a side at all — the stock protection slot — so whatever the mesh
    // carries for that side is the only look there is.
    #[test]
    fn with_no_paints_the_mesh_is_the_only_look() {
        assert!(mesh_supplies_side(&v(&["armor"]), &v(&[]), false));
    }

    // ...and the sides don't borrow from each other: an embedded goggle is not a shell look.
    #[test]
    fn an_unpainted_side_only_counts_its_own_sheets() {
        let embedded = v(&["goggles"]);
        assert!(mesh_supplies_side(&embedded, &v(&[]), true), "the goggle side has one");
        assert!(!mesh_supplies_side(&embedded, &v(&[]), false), "the shell does not");
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
mod viewer_tests {
    #[test]
    #[ignore]
    fn bike_model_from_pkz() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let m = super::load_bike_model_blocking(path).expect("load bike");
        for n in &m.nodes {
            // Where the part ended up. Printed because placement is the half of this that a
            // texture listing can't show: a part bound to the right sheet and hung in the
            // wrong place reads as correct here otherwise.
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for v in n.positions.chunks_exact(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            eprintln!(
                "node '{}' placed={} x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                n.name, n.placed, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2],
            );
            for s in &n.submeshes {
                eprintln!(
                    "   {:<16} -> {:<12} tile={:?}",
                    s.name,
                    s.texture.as_deref().unwrap_or("(none)"),
                    s.uv_tile
                );
            }
        }
        for p in &m.paints {
            let mut names: Vec<&str> = p.textures.iter().map(|t| t.name.as_str()).collect();
            names.sort_unstable();
            eprintln!(
                "paint '{}' changes_preview={}: {}",
                p.name,
                p.changes_preview,
                names.join(", ")
            );
        }
        assert!(!m.nodes.is_empty(), "decoded the mesh");
        let have: std::collections::HashSet<String> = m.paints[0]
            .textures
            .iter()
            .map(|t| t.name.to_ascii_lowercase())
            .collect();
        for n in &m.nodes {
            for s in &n.submeshes {
                if let Some(t) = &s.texture {
                    assert!(have.contains(&t.to_ascii_lowercase()), "'{t}' is available");
                }
            }
        }
    }


    #[test]
    #[ignore]
    fn gear_model_from_pkz() {
        let Ok(path) = std::env::var("MXB_REAL_GEAR") else {
            eprintln!("set MXB_REAL_GEAR to run");
            return;
        };
        let files = super::read_gear_files(std::path::Path::new(&path)).expect("read gear");
        let paints: Vec<String> = files
            .iter()
            .filter_map(|(n, _)| super::gear_folder_paint_name(n, "paints"))
            .collect();
        let goggles: Vec<String> = files
            .iter()
            .filter_map(|(n, _)| super::gear_folder_paint_name(n, "goggles"))
            .collect();
        eprintln!("paints ({}): {:?}", paints.len(), &paints[..paints.len().min(4)]);
        eprintln!("goggles ({}): {:?}", goggles.len(), &goggles[..goggles.len().min(4)]);

        // What the mesh itself draws each piece from — the reading the binder goes by, and
        // the one worth eyeballing when a helmet's goggles come out wearing the shell.
        if let Some((_, d)) = files.iter().find(|(n, _)| super::is_visible_gear_mesh(n)) {
            // The slots as the loader counts them: what the mesh embeds, plus the sheets it
            // leaves to a `.pnt`, in the order the model names them.
            let declared = super::paint_texture_names(
                files
                    .iter()
                    .filter(|(n, _)| {
                        super::gear_folder_paint_name(n, "paints").is_some()
                            || super::gear_folder_paint_name(n, "goggles").is_some()
                    })
                    .map(|(_, d)| d.as_slice()),
            );
            let colors = super::edf::declared_colors(d, &declared);
            eprintln!("mesh colour textures: {colors:?}");
            // Per piece, the texture the model was drawn against. This is the evidence the
            // binder decides on, so when a piece ends up wearing the wrong side it says
            // whether the reading was wrong or the choice made from it was.
            for n in &super::edf::parse(d) {
                eprintln!("mats    {:<28} {:?}", n.name, n.materials);
                for sm in &n.submeshes {
                    let emb = sm
                        .mat
                        .and_then(|m| n.materials.get(m as usize).copied().flatten())
                        .and_then(|slot| colors.get(slot))
                        .map(String::as_str);
                    // The triangle count tells the pieces apart when their names don't: a
                    // helmet shell dwarfs its lens, and both dwarf a tear-off film.
                    eprintln!(
                        "drawn   {:<28} mat={:?} tris={:<6} -> {}",
                        format!("{}/{}", n.name, sm.name),
                        sm.mat,
                        sm.tri_count,
                        emb.unwrap_or("(none)"),
                    );
                }
            }
        }
        // Where each mesh sits in its own frame, against the bounds the file states for
        // itself. Protection is authored in the rider's own space, so these say whether a
        // piece is a chest-wide vest or a thin chain — the difference a one-size fit erases —
        // and disagreeing with the header is how a placement that ran twice shows up.
        let mut scenes: Vec<&Vec<u8>> =
            super::gear_scenes(&files).iter().filter_map(|s| super::gear_file(&files, s)).collect();
        if scenes.is_empty() {
            scenes.extend(files.iter().find(|(n, _)| super::is_visible_gear_mesh(n)).map(|(_, d)| d));
        }
        for d in scenes {
            let mut nodes = super::edf::parse_gear(d);
            super::edf::to_right_handed(&mut nodes);
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for n in &nodes {
                let (mut nlo, mut nhi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
                for v in n.positions.chunks_exact(3) {
                    for k in 0..3 {
                        nlo[k] = nlo[k].min(v[k]);
                        nhi[k] = nhi[k].max(v[k]);
                    }
                }
                eprintln!(
                    "bounds  {:<16} x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                    n.name, nlo[0], nhi[0], nlo[1], nhi[1], nlo[2], nhi[2],
                );
                for k in 0..3 {
                    lo[k] = lo[k].min(nlo[k]);
                    hi[k] = hi[k].max(nhi[k]);
                }
            }
            let Some((hlo, hhi)) = super::edf::header_aabb(d) else { continue };
            // The header is written in authored space, so it takes the same X flip the
            // vertices got — and the flip swaps which end is the minimum.
            let (hlo, hhi) = ([-hhi[0], hlo[1], hlo[2]], [-hlo[0], hhi[1], hhi[2]]);
            eprintln!(
                "header                   x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]",
                hlo[0], hhi[0], hlo[1], hhi[1], hlo[2], hhi[2],
            );
            // Only the highest LOD is kept and a dropped one can reach a millimetre further,
            // so this is a "same place, same size" check, not an equality.
            for k in 0..3 {
                let slack = 0.02 + 0.1 * (hhi[k] - hlo[k]);
                assert!(
                    (lo[k] - hlo[k]).abs() <= slack && (hi[k] - hhi[k]).abs() <= slack,
                    "axis {k}: mesh [{:.3},{:.3}] is not where the file says it is \
                     ([{:.3},{:.3}]) — placement",
                    lo[k], hi[k], hlo[k], hhi[k],
                );
            }
        }
        // The names each side's first paint supplies — where each can actually land.
        let supplied = |folder: &str| -> std::collections::HashSet<String> {
            files
                .iter()
                .find(|(n, _)| super::gear_folder_paint_name(n, folder).is_some())
                .and_then(|(_, d)| super::paint::decode_any(d).ok())
                .map(|p| p.iter().map(|t| t.name.to_ascii_lowercase()).collect())
                .unwrap_or_default()
        };

        // The slot this item is worn in. Gear behaves differently per slot — protection has
        // no goggle side, and is the slot where a paintless mod is the norm rather than the
        // exception — so the run is worth naming honestly.
        let slot = std::env::var("MXB_REAL_GEAR_PART").unwrap_or_else(|_| "helmet".into());
        let part = super::load_gear_model_blocking(path.clone(), slot.clone(), None, None, false, false, Vec::new())
            .expect("load gear");
        // MXB_DUMP_OBJ=<file> writes the loaded geometry in viewer-input space, so a
        // silhouette can be drawn outside the test — the only way to settle which way a
        // gear frame points without the game in front of you.
        if let Ok(out) = std::env::var("MXB_DUMP_OBJ") {
            let mut s = String::new();
            for n in &part.nodes {
                s.push_str(&format!("o {}\n", n.name));
                for v in n.positions.chunks_exact(3) {
                    s.push_str(&format!("v {} {} {}\n", v[0], v[1], v[2]));
                }
            }
            std::fs::write(&out, s).expect("write obj");
            eprintln!("wrote {out}");
        }
        let have: std::collections::HashSet<String> =
            part.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        // An item with no paint and nothing baked into its mesh has no look to wear — the
        // Minecraft pickaxe on mxb-mods ships exactly that. Bare grey is the honest answer
        // there, and the only case where an unbound piece isn't a bug.
        let has_look = !paints.is_empty() || !have.is_empty();
        // Which texture each piece ended up wearing, by node so a goggle node that carries
        // no submeshes of its own shows up too.
        let mut worn: Vec<(String, String)> = Vec::new();
        let mut bare = 0usize;
        for n in &part.nodes {
            let pieces: Vec<(String, &Option<String>)> = if n.submeshes.is_empty() {
                vec![(n.name.clone(), &n.texture)]
            } else {
                n.submeshes
                    .iter()
                    .map(|s| (format!("{}/{}", n.name, s.name), &s.texture))
                    .collect()
            };
            for (label, tex) in pieces {
                match tex {
                    Some(t) => {
                        eprintln!("worn    {label:<28} -> {t}");
                        worn.push((label, t.clone()));
                    }
                    None => {
                        eprintln!("worn    {label:<28} -> (bare)");
                        bare += 1;
                    }
                }
            }
        }
        assert!(
            !has_look || bare == 0,
            "{bare} piece(s) left bare though the item ships a look",
        );
        for (_, t) in &worn {
            assert!(have.contains(&t.to_ascii_lowercase()), "'{t}' is shipped");
        }
        let (shell_names, goggle_names) = (supplied("paints"), supplied("goggles"));
        eprintln!("shell paint supplies {shell_names:?}, goggle paint {goggle_names:?}");
        // A helmet that ships goggle paints has somewhere for them to land — asked by name,
        // not by what a piece is called, since that's exactly what a mesh needn't spell out.
        // Skipped where the two sides share an atlas: then "whose texture is this" has no
        // answer to check.
        if !goggle_names.is_empty() && goggle_names.is_disjoint(&shell_names) {
            assert!(
                worn.iter().any(|(_, t)| goggle_names.contains(&t.to_ascii_lowercase())),
                "a piece wears the goggle paint: {worn:?}",
            );
            assert!(
                worn.iter().any(|(_, t)| !goggle_names.contains(&t.to_ascii_lowercase())),
                "the shell keeps its own texture: {worn:?}",
            );
        }

        // Stock: the same mesh wearing the textures embedded in it, not a packed `.pnt`.
        let listed = super::gear_paints_at(std::path::Path::new(&path)).expect("list paints");
        eprintln!("has_stock={} goggles={}", listed.has_stock, listed.has_stock_goggles);
        if !listed.has_stock {
            eprintln!("this piece embeds no textures — no stock entry to check");
            return;
        }
        let stock =
            super::load_gear_model_blocking(
                path,
                slot,
                None,
                None,
                true,
                listed.has_stock_goggles,
                Vec::new(),
            )
            .expect("load stock gear");
        let embedded: std::collections::HashSet<String> =
            stock.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        assert!(
            !paints.iter().any(|p| embedded.contains(&p.to_ascii_lowercase())),
            "a stock preview decodes no packed paint",
        );
        let mut bound = 0;
        for n in &stock.nodes {
            for s in &n.submeshes {
                let t = s.texture.as_ref().expect("stock submesh bound to a texture");
                eprintln!("stock submesh {:<10} -> {t}", s.name);
                assert!(embedded.contains(&t.to_ascii_lowercase()), "'{t}' is embedded in the mesh");
                bound += 1;
            }
        }
        assert!(bound > 0, "stock bound at least one submesh");

        // Mixed: stock shell, painted goggles. A paint reuses the mesh's texture names, so
        // this is where the two sets would collide and the viewer would pick at random.
        if let Some(g) = goggles.first() {
            let mixed = super::load_gear_model_blocking(
                std::env::var("MXB_REAL_GEAR").unwrap(),
                "helmet".into(),
                None,
                Some(g.clone()),
                true,
                false,
                Vec::new(),
            )
            .expect("load mixed gear");
            let mut names: Vec<String> =
                mixed.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
            names.sort_unstable();
            let total = names.len();
            names.dedup();
            assert_eq!(names.len(), total, "one texture per name, so binding is unambiguous");
            for n in &mixed.nodes {
                for s in &n.submeshes {
                    let t = s.texture.as_ref().expect("mixed submesh bound to a texture");
                    assert!(names.contains(&t.to_ascii_lowercase()), "'{t}' is available");
                }
            }
        }
    }

    /// The picker's paint list against a real install: whatever any single source can
    /// supply for a model, the merged list offers.
    ///
    /// `MXB_MODS=~/Documents/PiBoSo/MX\ Bikes MXB_GEAR_PART=boots \
    ///   MXB_GEAR_MODEL='Fox Instinct 2.0 by Aeffertz' \
    ///   cargo test gear_paints_merge_every_source -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gear_paints_merge_every_source() {
        let Ok(mods) = std::env::var("MXB_MODS") else {
            eprintln!("set MXB_MODS to run");
            return;
        };
        let part = std::env::var("MXB_GEAR_PART").unwrap_or_else(|_| "boots".into());
        let Ok(model) = std::env::var("MXB_GEAR_MODEL") else {
            eprintln!("set MXB_GEAR_MODEL to run");
            return;
        };
        let cfg = crate::config::AppConfig { mods_path: mods.clone(), ..Default::default() };
        let spec = super::GEAR.iter().find(|g| g.part == part).expect("a gear slot");

        let merged = super::gear_paints_for(&cfg, spec, &model);
        eprintln!("merged paints ({}): {:?}", merged.paints.len(), merged.paints);
        eprintln!("merged goggles ({}): {:?}", merged.goggles.len(), merged.goggles);

        let has = |set: &[String], want: &str| set.iter().any(|n| n.eq_ignore_ascii_case(want));

        // Every source, asked on its own. Each one's paints have to survive the merge —
        // taking the first source is exactly what dropped the others.
        let rider = std::path::Path::new(&mods).join("mods").join("rider");
        let stem = model.trim_end_matches(".pkz");
        let mut sources = 0;
        for src in super::gear_sources(&rider, spec, stem) {
            if !src.exists() {
                continue;
            }
            sources += 1;
            let alone = super::gear_paints_at(&src).expect("list one source");
            eprintln!("  {src:?}: {:?}", alone.paints);
            for p in &alone.paints {
                assert!(has(&merged.paints, p), "'{p}' from {src:?} survives the merge");
            }
            for g in &alone.goggles {
                assert!(has(&merged.goggles, g), "goggle '{g}' from {src:?} survives the merge");
            }
        }

        // The game's own copy, which is all a stock name has and which nothing listed before.
        if let Some(pkz) = super::resolve_game_pkz(&cfg, "rider.pkz") {
            let folder = format!("rider/{}/{}", spec.pkz_kind, stem);
            let stock = super::pkz_paint_names(&pkz, &folder, "paints");
            eprintln!("  rider.pkz {folder}: {stock:?}");
            if !stock.is_empty() {
                sources += 1;
            }
            for p in &stock {
                assert!(has(&merged.paints, p), "stock '{p}' survives the merge");
            }
        }
        assert!(sources > 0, "'{model}' resolved to at least one source");
        // Nothing is offered twice — a paint pack installed beside the mod it was made for
        // ships the same names, and the same look twice is not two choices.
        let mut seen: Vec<String> = merged.paints.iter().map(|s| s.to_lowercase()).collect();
        let total = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), total, "no paint is offered twice");
    }

    #[test]
    fn body_slot_reads_the_name_not_the_index() {
        // The `w_` planes are decals the game composites a name and number onto.
        for decal in ["w_name", "w_number", "w_plate", "W_Name"] {
            assert_eq!(super::body_slot(Some(decal)), "hide");
        }
        // Skin, whatever the model calls it, must not wear the kit.
        for skin in ["face_parts", "face", "Face_Parts", "rider_face"] {
            assert_eq!(super::body_slot(Some(skin)), "face");
        }
        // Everything else keeps its own name, so a paint replaces it by name and a piece no
        // paint covers falls back to the model's own texture.
        for (name, slot) in [
            ("rider", "rider"),
            ("gloves", "gloves"),
            ("rider_sm", "rider_sm"),
            ("glovessm", "glovessm"),
            ("Braces", "braces"),
        ] {
            assert_eq!(super::body_slot(Some(name)), slot);
        }
        // A material that names no texture still has to render as something.
        assert_eq!(super::body_slot(None), "rider");
    }

    /// A material id is LOCAL to the node that owns it, so the same id must resolve to
    /// different textures in different parts of one mesh.
    ///
    /// This is the invariant the per-part material-table fix established, and the one the
    /// rider binder lost: it was still resolving ids through a whole-model reading, which
    /// is what put one part's texture on another's geometry.
    #[test]
    fn a_body_part_resolves_its_material_through_its_own_table() {
        fn tex(name: &str) -> super::edf::EmbeddedTexture {
            super::edf::EmbeddedTexture {
                name: name.into(),
                width: 4,
                height: 4,
                data_off: 0,
                data_len: 0,
            }
        }
        fn sm(mat: u32) -> super::edf::Submesh {
            super::edf::Submesh {
                name: "range".into(),
                tri_start: 0,
                tri_count: 1,
                texture: None,
                uv_tile: None,
                mat: Some(mat),
            }
        }
        fn node(materials: Vec<Option<usize>>, mats: &[u32]) -> super::edf::EdfNode {
            super::edf::EdfNode {
                name: "part".into(),
                positions: Vec::new(),
                uvs: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                submeshes: mats.iter().map(|m| sm(*m)).collect(),
                texture: None,
                placed: false,
                materials,
            }
        }

        let colors = [tex("rider"), tex("gloves"), tex("w_number")];
        // Both parts draw on local id 0 — and mean different textures by it.
        let mut nodes = vec![
            node(vec![Some(0)], &[0]),           // body  -> rider
            node(vec![Some(1)], &[0]),           // hands -> gloves
            node(vec![Some(2), None], &[0, 1]),  // plate -> hidden decal, then untextured
        ];
        super::bind_body_to_colors(&mut nodes, &colors);

        assert_eq!(nodes[0].submeshes[0].texture.as_deref(), Some("rider"));
        assert_eq!(
            nodes[1].submeshes[0].texture.as_deref(),
            Some("gloves"),
            "id 0 read through the first node's table would smear the suit onto the hands"
        );
        assert_eq!(nodes[2].submeshes[0].texture.as_deref(), Some("hide"));
        // An untextured material, and an id past the end of the table, both still render.
        assert_eq!(nodes[2].submeshes[1].texture.as_deref(), Some("rider"));
    }

    /// The bug this replaced: material indices count into the model's own texture list, and
    /// no two rider models write that list in the same order.
    ///
    /// `MXB_REAL_BODY=<rider.edf>[,<rider.edf>…] cargo test rider_body_binding -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rider_body_binding_from_env() {
        let Ok(paths) = std::env::var("MXB_REAL_BODY") else {
            eprintln!("set MXB_REAL_BODY to run");
            return;
        };
        for path in paths.split(',').filter(|p| !p.is_empty()) {
            let bytes = std::fs::read(path).expect("read rider.edf");
            let order: Vec<String> =
                crate::edf::color_textures(&bytes).iter().map(|t| t.name.clone()).collect();
            let mut nodes = crate::edf::parse(&bytes);
            crate::edf::to_right_handed(&mut nodes);
            super::keep_lod0(&mut nodes);
            super::bind_body_submeshes(&mut nodes, &bytes);

            super::stand_body_upright(&mut nodes);

            let bounds = |ns: &[crate::edf::EdfNode], only_face: bool| {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for n in ns {
                    for sm in &n.submeshes {
                        if only_face && sm.texture.as_deref() != Some("face") {
                            continue;
                        }
                        for i in &n.indices[sm.tri_start as usize * 3
                            ..(sm.tri_start + sm.tri_count) as usize * 3]
                        {
                            let v = &n.positions[*i as usize * 3..*i as usize * 3 + 3];
                            for a in 0..3 {
                                lo[a] = lo[a].min(v[a]);
                                hi[a] = hi[a].max(v[a]);
                            }
                        }
                    }
                }
                (lo, hi)
            };
            let (lo, hi) = bounds(&nodes, false);
            let (h, d) = (hi[1] - lo[1], hi[2] - lo[2]);
            eprintln!("  upright bounds x={:.3} y={h:.3} z={d:.3}", hi[0] - lo[0]);

            // A rider is a standing figure, so height is its longest axis. The viewer scales
            // and anchors every piece of gear off this — a body on its side buries the
            // helmet and boots in the torso at a fifth of their size.
            assert!(h > hi[0] - lo[0] && h > d, "the body stands up");
            // And it stands the right way up. Height alone can't tell a rider from one
            // hanging upside down, so check the skin: the head is the highest thing on a
            // rider. Its top, not its bottom — Rider+'s skin texture also covers the bare
            // wrists of its rolled-sleeve variants, which reach well down the body.
            let (_, fhi) = bounds(&nodes, true);
            eprintln!("  skin tops out at {:.3} of {:.3}", fhi[1], hi[1]);

            // Which way does it face? Report the Z centroid of each slot relative to the
            // body's own centre, plus the head alone (the top eighth of the skin, so
            // Rider+'s bare wrists don't drag the number toward the bars).
            let cz = (lo[2] + hi[2]) / 2.0;
            let mut per: std::collections::BTreeMap<String, (f64, f64, usize)> = Default::default();
            for n in &nodes {
                for sm in &n.submeshes {
                    let slot = sm.texture.clone().unwrap_or_default();
                    for i in &n.indices
                        [sm.tri_start as usize * 3..(sm.tri_start + sm.tri_count) as usize * 3]
                    {
                        let v = &n.positions[*i as usize * 3..*i as usize * 3 + 3];
                        let e = per.entry(slot.clone()).or_default();
                        e.0 += (v[2] - cz) as f64;
                        e.2 += 1;
                        if slot == "face" && v[1] > hi[1] - 0.125 * h {
                            let hd = per.entry("face(head)".into()).or_default();
                            hd.0 += (v[2] - cz) as f64;
                            hd.2 += 1;
                        }
                    }
                }
            }
            for (slot, (sum, _, n)) in &per {
                eprintln!("  {slot:>12} z-centroid {:+.4} ({n} verts)", sum / *n as f64);
            }
            let centroid = |slot: &str| per.get(slot).map(|(s, _, n)| s / *n as f64);

            // And it faces the right way. The viewer nudges the helmet and boots forward in
            // +Z, so a rider turned around wears its gear through its own back.
            //
            // The name and number planes are the tell: they go on a rider's back. Where the
            // model has none, fall back to the head, which leans forward over the bars —
            // a weaker signal, so it only decides when the strong one is absent.
            match centroid("hide") {
                Some(back) => assert!(back < 0.0, "the name and number sit on the back ({back:+.4})"),
                None => {
                    let head = centroid("face(head)").expect("a rider has a head");
                    assert!(head > 0.0, "the head leans forward ({head:+.4})");
                }
            }
            assert!(
                fhi[1] > lo[1] + 0.9 * h,
                "the head is at the top (skin tops at {:.3}, body {:.3}..{:.3})",
                fhi[1],
                lo[1],
                hi[1],
            );

            let mut slots: Vec<String> = nodes
                .iter()
                .flat_map(|n| n.submeshes.iter().filter_map(|s| s.texture.clone()))
                .collect();
            slots.sort_unstable();
            slots.dedup();
            eprintln!("{path}\n  blob order: {order:?}\n  bound slots: {slots:?}");

            assert!(!slots.is_empty(), "every body binds its submeshes to something");
            // Whatever the model calls its suit and gloves, the binding must name the
            // textures the model actually carries — never a slot borrowed from another model.
            for s in &slots {
                assert!(
                    s == "hide"
                        || s == "face"
                        || order.iter().any(|t| t.eq_ignore_ascii_case(s)),
                    "'{s}' is a texture this model carries",
                );
            }
            // Skin is its own slot on every rider model shipped so far; catching its loss
            // is what tells us a mesh stopped being read and an index map crept back in.
            assert!(slots.iter().any(|s| s == "face"), "the face binds to bare skin");
        }
    }

    /// The whole rider-body path against a real install: a custom model resolves out of
    /// `mods/rider/riders`, binds, and wears a kit that only the stock profile owns.
    ///
    /// `MXB_MODS=~/Documents/PiBoSo/MX\ Bikes MXB_PROFILE=Rider+ cargo test rider_body_end_to_end -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rider_body_end_to_end() {
        let Ok(mods) = std::env::var("MXB_MODS") else {
            eprintln!("set MXB_MODS to run");
            return;
        };
        let profile = std::env::var("MXB_PROFILE").unwrap_or_else(|_| "Rider+".into());
        let cfg = crate::config::AppConfig { mods_path: mods.clone(), ..Default::default() };
        let base = std::path::Path::new(&mods).join("mods").join("rider");

        // A model nobody can pick is a model nobody can wear. The scan reports what's on
        // disk; the two stock riders live in `rider.pkz` and the picker adds them itself.
        let targets = crate::library::scan_rider_targets(&mods);
        eprintln!("profiles: {:?}", targets.profiles);
        assert!(
            targets.profiles.iter().any(|p| *p == profile)
                || super::STOCK_RIDER_PROFILES.contains(&profile.as_str()),
            "'{profile}' is offered",
        );

        let src = super::rider_body_source(&cfg, &profile).expect("a body source");
        eprintln!("{profile}: {src:?}");

        {
            let t = std::time::Instant::now();
            let data = src.read(&profile).expect("read mesh");
            eprintln!("  read {} MB in {:?}", data.len() / 1_000_000, t.elapsed());
            let t = std::time::Instant::now();
            let mut n = crate::edf::parse(&data);
            eprintln!("  parse {} nodes in {:?}", n.len(), t.elapsed());
            let t = std::time::Instant::now();
            crate::edf::to_right_handed(&mut n);
            super::keep_lod0(&mut n);
            eprintln!("  handedness + lod0 in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            super::bind_body_submeshes(&mut n, &data);
            eprintln!("  bind in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            super::stand_body_upright(&mut n);
            eprintln!("  stand in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            let texs = super::body_textures(&src, &profile).expect("textures");
            eprintln!(
                "  extract {:?} in {:?}",
                texs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
                t.elapsed(),
            );
        }
        // A profile folder that carries a mesh is a model, and beats the game archive. One
        // that only carries paints — which is what installing a kit under `default_mx`
        // leaves behind — is not, and must still fall through to the stock body.
        let installed = base.join("riders").join(&profile).join("rider.edf").is_file();
        assert_eq!(
            matches!(src, super::BodySource::Loose(_)),
            installed,
            "an installed mesh wins, a paints-only folder doesn't",
        );

        let part = super::load_rider_body(&cfg, &profile, Vec::new()).expect("a body part");
        let slots: std::collections::BTreeSet<&str> = part
            .nodes
            .iter()
            .flat_map(|n| n.submeshes.iter().filter_map(|s| s.texture.as_deref()))
            .collect();
        let texs: std::collections::BTreeSet<String> =
            part.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
        eprintln!("  slots={slots:?}\n  textures={texs:?}");
        // With no paint chosen, every slot that draws something is dressed by the model.
        for s in slots.iter().filter(|s| **s != "hide" && **s != "face") {
            assert!(texs.contains(*s), "'{s}' is supplied by the mesh when no paint is");
        }

        // Rider+ ships `paints` empty on purpose — the kit still has to resolve.
        if let Some(kit) = std::env::var("MXB_KIT").ok().filter(|k| !k.is_empty()) {
            let found = super::read_rider_paint_file(&cfg, &base, &profile, "paints", &kit);
            assert!(found.is_some(), "kit '{kit}' resolves for '{profile}'");
            eprintln!("  kit '{kit}' resolved ({} bytes)", found.unwrap().len());
        }
    }

    #[test]
    #[ignore]
    fn lod0_dedup_from_env() {
        let Ok(path) = std::env::var("MXB_REAL_EDF") else {
            eprintln!("set MXB_REAL_EDF to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read edf");
        let mut nodes = crate::edf::parse(&bytes);
        let before = nodes.len();
        super::keep_lod0(&mut nodes);
        for n in &nodes {
            eprintln!("kept node '{}' tris={}", n.name, n.indices.len() / 3);
        }
        let mut names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "no duplicate node names survive");
        eprintln!("{before} nodes -> {} after LOD dedup", nodes.len());
    }

    /// Write a bike's raw `.edf` meshes out so the binary layout can be studied directly.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> MXB_EDF_OUT=<dir> \
    ///   cargo test extract_bike_edfs -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn extract_bike_edfs() {
        let (Ok(path), Ok(out)) = (std::env::var("MXB_REAL_PKZ"), std::env::var("MXB_EDF_OUT"))
        else {
            eprintln!("set MXB_REAL_PKZ and MXB_EDF_OUT to run");
            return;
        };
        let stem = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        std::fs::create_dir_all(&out).expect("create out dir");
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if !bn.ends_with(".edf") {
                continue;
            }
            let dst = std::path::Path::new(&out).join(format!("{stem}__{bn}"));
            std::fs::write(&dst, data).expect("write edf");
            println!("wrote {} ({} bytes)", dst.display(), data.len());
        }
    }

    /// Dump, as one JSON object, everything that decides which texture each part of a
    /// bike wears: the mesh's colour list, what each reading of a material index claims,
    /// which reading the geometry settled on, and what the viewer finally bound.
    ///
    /// `MXB_REAL_PKZ='…/mods/bikes/MX2OEM_2023_Kawasaki_KX250.pkz' \
    ///   cargo test audit_bike_bindings -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn audit_bike_bindings() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let bike = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        // What the viewer renders today.
        let model = super::load_bike_model_blocking(path.clone()).expect("load bike");
        // Keyed on the triangle range too: one group name can appear twice in a node,
        // once per material, and those two are exactly the interesting case.
        let bound: std::collections::HashMap<(String, String, u32), Option<String>> = model
            .nodes
            .iter()
            .flat_map(|n| {
                n.submeshes.iter().map(move |s| {
                    (
                        (n.name.to_ascii_lowercase(), s.name.to_ascii_lowercase(), s.tri_start),
                        s.texture.clone(),
                    )
                })
            })
            .collect();

        // The same meshes again, raw, so both readings of every material can be shown
        // side by side with the fit that chose between them.
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");
        let mut out = String::new();
        out.push_str(&format!("{{\"bike\":\"{}\",\"meshes\":[", esc(&bike)));
        let mut first_mesh = true;
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if !bn.ends_with(".edf") {
                continue;
            }
            let nodes = crate::edf::parse_with_levels(data, &[]);
            if nodes.is_empty() {
                continue;
            }
            let color = crate::edf::color_textures(data);
            if color.is_empty() {
                continue; // a shadow mesh carries no colour textures and is never rendered
            }
            let colors: Vec<String> =
                color.iter().map(|t| format!("\"{}\"", esc(&t.name))).collect();
            if !first_mesh {
                out.push(',');
            }
            first_mesh = false;
            out.push_str(&format!(
                "{{\"file\":\"{}\",\"colors\":[{}],\"nodes\":[",
                esc(&bn),
                colors.join(",")
            ));
            for (i, n) in nodes.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // The node's OWN material table: local id -> colour texture.
                let table: Vec<String> = n
                    .materials
                    .iter()
                    .map(|slot| match slot.and_then(|s| color.get(s)) {
                        Some(t) => format!("\"{}\"", esc(&t.name)),
                        None => "null".into(),
                    })
                    .collect();
                out.push_str(&format!(
                    "{{\"node\":\"{}\",\"materials\":[{}],\"submeshes\":[",
                    esc(&n.name),
                    table.join(",")
                ));
                for (j, s) in n.submeshes.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    let key =
                        (n.name.to_ascii_lowercase(), s.name.to_ascii_lowercase(), s.tri_start);
                    let rendered = bound
                        .get(&key)
                        .cloned()
                        .flatten()
                        .map(|t| format!("\"{}\"", esc(&t)))
                        .unwrap_or_else(|| "null".into());
                    out.push_str(&format!(
                        "{{\"group\":\"{}\",\"mat\":{},\"tris\":{},\"rendered\":{}}}",
                        esc(&s.name),
                        s.mat.map(|m| m.to_string()).unwrap_or_else(|| "null".into()),
                        s.tri_count,
                        rendered,
                    ));
                }
                out.push_str("]}");
            }
            out.push_str("]}");
        }
        let paints: Vec<String> = model
            .paints
            .iter()
            .map(|p| {
                let mut t: Vec<String> =
                    p.textures.iter().map(|t| format!("\"{}\"", esc(&t.name))).collect();
                t.sort_unstable();
                format!("{{\"paint\":\"{}\",\"textures\":[{}]}}", esc(&p.name), t.join(","))
            })
            .collect();
        out.push_str(&format!("],\"paints\":[{}]}}", paints.join(",")));
        println!("AUDIT {out}");
    }

    /// The average colour of a stored texture, and how far off neutral it is.
    ///
    /// Neutrality rather than "is it white" on purpose: it holds whichever way round the
    /// channels are stored, so this can't pass or fail for the wrong reason if that ever
    /// moves. A white or grey sheet has its three channels together; a painted one doesn't.
    fn avg_and_spread(tex: &crate::paint::PaintTexture) -> ([u32; 3], u32) {
        let px = crate::texstore::get(&tex.token).expect("token resolves");
        let mut sum = [0u64; 3];
        let mut n = 0u64;
        for p in px.chunks_exact(4) {
            if p[3] < 128 {
                continue; // fully transparent regions are not the look
            }
            for k in 0..3 {
                sum[k] += p[k] as u64;
            }
            n += 1;
        }
        assert!(n > 0, "'{}' has no opaque pixels", tex.name);
        let avg = [(sum[0] / n) as u32, (sum[1] / n) as u32, (sum[2] / n) as u32];
        (avg, avg.iter().max().unwrap() - avg.iter().min().unwrap())
    }

    /// The reported bug, pinned to the archive it came from: the rider tab drew the stock
    /// helmet bronze while the library's "Stock" entry drew it white.
    ///
    /// Both halves matter and they pull in opposite directions — an empty name must reach no
    /// paint at all, while a *stale* name must still reach one, or gear whose paint was
    /// renamed comes out bare grey. A single assertion would let the fix overshoot.
    #[test]
    #[ignore]
    fn an_unnamed_stock_paint_is_the_mesh_not_the_first_pnt() {
        let Ok(pkz) = std::env::var("MXB_RIDER_PKZ") else {
            eprintln!("set MXB_RIDER_PKZ to the game's rider.pkz to run");
            return;
        };
        let pkz = std::path::Path::new(&pkz);
        let folder = "rider/helmets/default";

        // No name → no paint, so the caller falls through to the mesh's own textures.
        assert!(
            super::load_pkz_paint(pkz, folder, "paints", "").is_empty(),
            "an empty slot must not resolve to a paint",
        );
        // A name that misses → still a paint, so a renamed livery shows textured.
        assert!(
            !super::load_pkz_paint(pkz, folder, "paints", "no_such_paint_ea7b").is_empty(),
            "a stale name must still fall back to a paint",
        );

        // And what the two answers actually look like. The mesh's own sheet is the white one
        // the library shows; the paint that used to stand in for it is not.
        let mesh = super::read_pkz_entry(pkz, &format!("{folder}/helmet.edf")).expect("helmet.edf");
        let stock = crate::paint::extract_edf_textures_where(&mesh, |n| n.eq_ignore_ascii_case("helmet"));
        let stock = stock.first().expect("the mesh carries its own 'helmet' sheet");
        let (stock_avg, stock_spread) = avg_and_spread(stock);

        let first = super::load_pkz_paint(pkz, folder, "paints", "black_yellow");
        let first = first.iter().find(|t| t.name.eq_ignore_ascii_case("helmet"));
        let first = first.expect("black_yellow paints 'helmet'");
        let (first_avg, first_spread) = avg_and_spread(first);

        eprintln!("stock mesh sheet  avg={stock_avg:?} spread={stock_spread}");
        eprintln!("black_yellow.pnt  avg={first_avg:?} spread={first_spread}");
        assert!(stock_spread < 12, "the stock helmet is neutral (white/grey), got {stock_avg:?}");
        assert!(
            first_spread > 40,
            "black_yellow is a colour, not a neutral — if this fails the fixture archive \
             changed and the test above proves less than it looks like ({first_avg:?})",
        );
    }

    /// The invariant behind the fix, for whatever gear is on this machine: an empty paint
    /// slot renders exactly what the library's "Stock" entry renders — and where there is no
    /// stock look to render, it still renders *something* rather than going bare.
    ///
    /// Stated as an equality between the two code paths rather than as an expected texture
    /// name, because the point is that they agree, not what they agree on.
    #[test]
    #[ignore]
    fn an_empty_slot_renders_what_the_library_calls_stock() {
        let Ok(path) = std::env::var("MXB_REAL_GEAR") else {
            eprintln!("set MXB_REAL_GEAR to an installed gear folder/.pkz to run");
            return;
        };
        let slot = std::env::var("MXB_REAL_GEAR_PART").unwrap_or_else(|_| "helmet".into());
        let names = |p: super::RiderPart| {
            let mut v: Vec<String> = p.textures.iter().map(|t| t.name.to_ascii_lowercase()).collect();
            v.sort_unstable();
            v
        };
        let load = |paint: Option<&str>, stock: bool| {
            names(
                super::load_gear_model_blocking(
                    path.clone(),
                    slot.clone(),
                    paint.map(str::to_string),
                    None,
                    stock,
                    false,
                    Vec::new(),
                )
                .expect("load gear"),
            )
        };

        let offers_stock = super::gear_paints_at(std::path::Path::new(&path))
            .expect("read gear paints")
            .has_stock;
        // The rider tab's empty slot, and the library's picker on "Stock".
        let empty = load(Some(""), false);
        eprintln!("has_stock={offers_stock} empty slot -> {empty:?}");
        assert!(!empty.is_empty(), "an empty slot must never render untextured");
        if offers_stock {
            assert_eq!(empty, load(None, true), "empty slot != the library's Stock");
        } else {
            // No stock look to fall back on, so the first-paint fallback still stands —
            // forcing stock here is what would draw the Bell Moto 10 in a near-blank film.
            assert_eq!(empty, load(None, false), "kept the first-paint fallback");
        }
    }
}

