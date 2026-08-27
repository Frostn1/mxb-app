// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod antidebug;
mod bikefiles;
mod bikeswap;
mod bundle;
mod cancel;
mod cfg;
mod cloudfiles;
mod config;
mod cookie_session;
mod downloads;
mod dropzone;
mod edf;
mod fileshare;
mod frostmod;
mod frostmod_manage;
mod game;
mod gameproc;
mod gearrepair;
mod heightfield;
mod imgcache;
mod install;
mod ledger;
mod library;
mod linkwalk;
mod logs;
mod lru;
mod memwatch;
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
mod paintwatch;
mod pkz;
/// Linux only: the Proton prefix the game runs in, and how to put a Windows program in it.
#[cfg(target_os = "linux")]
mod proton;
#[cfg(sidecar)]
mod sidecar;
mod presets;
mod paintsync;
mod reshade;
mod servers;
mod sessionwatch;
mod shop_catalog_session;
mod shop_credentials;
mod shop_fetch;
mod shop_installed;
mod shop_session;
mod soundmods;
mod texstore;
mod track;
mod upload;
mod vcruntime;
mod voice;
mod winehost;

use config::AppConfig;
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
    /// Model swaps only (`None` for sound). `live_refresh` re-runs the *customization*
    /// loader, which reloads paints/gear but never the mesh — the model needs FrostMod
    /// to re-apply the bike. See `frostmod::signal_refresh_model`.
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
    // Ask FrostMod to re-apply the bike so the new model shows in the garage without a
    // class switch away-and-back. Only acts if `bike` is the selected one (decided
    // inside FrostMod, which is the only side that knows). Gated on the same
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

/// A track's metadata and contents. Cheap by construction — nothing is inflated — so the
/// track view can paint everything except the terrain immediately.
#[tauri::command]
async fn read_track_info(app: tauri::AppHandle, path: String) -> Result<track::TrackInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        track::read_info(&app, &path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("read_track_info task failed: {e}"))?
}

/// A plain-text account of what a track's terrain looks like to the reader.
///
/// Shown in the viewer when a track's terrain won't load. The height format is undocumented,
/// so a track that fails is evidence we don't otherwise have — and the player holding it is
/// rarely the person who can rebuild the app to investigate.
#[tauri::command]
async fn diagnose_track(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || track::diagnose(std::path::Path::new(&path)))
        .await
        .map_err(|e| format!("diagnose_track task failed: {e}"))
}

/// A track's terrain grid, at no more than `max_dim` samples on its longest edge.
///
/// Returned as raw bytes rather than JSON: a grid is a few hundred thousand floats, and
/// serialising that as a JSON array costs more than reading it out of the archive did. The
/// app reads the header described in [`track::BLOB_HEADER`] and takes the rest in place.
#[tauri::command]
async fn load_track_terrain(
    app: tauri::AppHandle,
    path: String,
    max_dim: u32,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let master = track::load_master(&app, &path).map_err(|e| format!("{e:#}"))?;
        Ok(tauri::ipc::Response::new(track::terrain_blob(
            &master, max_dim,
        )))
    })
    .await
    .map_err(|e| format!("load_track_terrain task failed: {e}"))?
}

/// A picture of a track's surfaces, to lay over its terrain.
///
/// Empty — not an error — when the track's height file carries no coverage masks. That track
/// draws on its relief alone, which is what every track did before this existed, so there is
/// nothing here worth failing a view over.
#[tauri::command]
async fn load_track_overview(
    app: tauri::AppHandle,
    path: String,
    max_dim: u32,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let blob = track::overview_blob(&app, std::path::Path::new(&path), max_dim)
            .unwrap_or_default();
        tauri::ipc::Response::new(blob)
    })
    .await
    .map_err(|e| format!("load_track_overview task failed: {e}"))
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
    if let (true, Some(model_dir)) = (main_paints, dir.parent()) {
        add(&mut names, mesh_texture_names(model_dir));
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
                meshes.extend(read_gear_file(&p));
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

/// Watch the paint files the 3D viewer is currently showing, replacing whatever it was
/// watching before. An empty list stops.
///
/// There is one viewer showing one paint, so this is a set-the-whole-thing call rather than
/// an add/remove pair: nothing can then leak a watch by forgetting to take one back.
#[tauri::command]
fn watch_paint_files(app: tauri::AppHandle, watcher: State<PaintWatcher>, paths: Vec<String>) {
    paintwatch::start(&app, &watcher, &paths);
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
    /// Where the `.pnt` sits on disk, for a paint installed loose in the bike's `paints`
    /// folder — the file the viewer watches so an edit re-dresses the model. `None` for a
    /// paint packed inside the archive: nothing rewrites one of those in place.
    path: Option<String>,
    textures: Vec<paint::PaintTexture>,
    changes_preview: bool,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BikeModel {
    nodes: Vec<edf::EdfNode>,
    paints: Vec<BikePaint>,
    /// The model's own textures — the look it ships with, before any paint replaces one.
    ///
    /// The same pixels the paints below already carry as fillers, said once on their own.
    /// Folded into a paint they're indistinguishable from that paint's own sheets, and the
    /// Designer's reference underlay needs the distinction: an OEM bike's stock `.pnt`
    /// replaces the wheels and the chain, so `plastics` is only ever in here.
    base: Vec<paint::PaintTexture>,
    /// The tyres mod the wheels came out of, or `None` when the bike drew none.
    ///
    /// What was *actually* fitted, not what was asked for: a pick that names nothing
    /// installed falls back to the bike's own, and the picker has to show that rather than
    /// claim a pack that isn't on screen.
    tyres: Option<String>,
    /// Whether the parts were placed into one frame by the bike's `.geom`.
    ///
    /// False means every node still sits in its own local frame, so a vertex's position says
    /// nothing about where it is on the bike. The Designer names the flank a sheet region
    /// paints from the sign of x, and that answer is only worth giving once this is true.
    assembled: bool,
    /// The joints this bike can be posed about, in the frame `nodes` came back in.
    ///
    /// `None` for a bike that wasn't assembled — there is nothing to pose a pile of parts
    /// that are each still in their own frame. See [`edf::BikeRig`] for why the viewer poses
    /// at all rather than drawing one settled stance.
    rig: Option<edf::BikeRig>,
}

impl BikeModel {
    /// Every texture token the model holds, so evicting it can free the pixels too.
    ///
    /// `base` as well as the paints, and not only because it is a field now: a base texture
    /// that *every* paint overrides is folded into none of them, and before this was dropped
    /// without ever being released. Duplicates are free — `texstore::release` removes by key.
    fn tokens(&self) -> Vec<String> {
        self.paints
            .iter()
            .flat_map(|p| p.textures.iter())
            .chain(self.base.iter())
            .map(|t| t.token.clone())
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

fn mtime_nanos(path: &std::path::Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Cache key for whatever lives at `source`: its path, when it was last written, and how
/// big it is.
///
/// Size is in there for the viewer's live reload. A paint being re-saved every few seconds
/// is the one caller that rewrites a file under the cache, and mtime alone would serve it
/// stale pixels on any filesystem whose timestamps are coarser than the gap between two
/// saves — FAT32 rounds to two seconds. A recompressed `.pnt` almost never comes back the
/// same length.
fn bike_cache_key(source: &str) -> String {
    let path = std::path::Path::new(source);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    format!("{source}:{}:{size}{}", mtime_nanos(path), packed_stamp(path))
}

/// The bike's packed archive, as a cache key fragment. It's a real input to every bike now
/// — the loose folder layers over it — and updating a bike replaces the `.pkz` without
/// necessarily touching the folder above it.
fn packed_stamp(bike_dir: &std::path::Path) -> String {
    if !bike_dir.is_dir() {
        return String::new(); // the source *is* the archive; its own mtime is already in the key
    }
    match packed_bike(bike_dir) {
        Some(pkz) => {
            let size = std::fs::metadata(&pkz).map(|m| m.len()).unwrap_or(0);
            format!("#z{}:{size}", mtime_nanos(&pkz))
        }
        None => String::new(),
    }
}

/// A swap preview is keyed by both folders it's built from — the same bike renders
/// differently per variant, and either side can change under us.
fn swap_cache_key(set: &modelswap::PreviewSet) -> String {
    // The livery list is part of what a preview shows, and an assignment edit changes it
    // without touching either folder — so key on the resolved paths, not just the mtimes.
    let paints: Vec<String> = set.paints.iter().map(|p| p.display().to_string()).collect();
    format!(
        "{}#{}:{}:{}:{}{}",
        set.bike_dir.display(),
        set.variant_dir.display(),
        mtime_nanos(&set.bike_dir),
        mtime_nanos(&set.variant_dir),
        paints.join(","),
        packed_stamp(&set.bike_dir),
    )
}

#[tauri::command]
async fn load_bike_model(source: String, tyres: Option<String>) -> Result<BikeModel, String> {
    tauri::async_runtime::spawn_blocking(move || load_bike_model_blocking(source, tyres))
        .await
        .map_err(|e| format!("load_bike_model task failed: {e}"))?
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
    let t0 = std::time::Instant::now();
    let cfg = config::load(&app).map_err(|e| format!("{e:#}"))?;
    let set =
        modelswap::preview_set(&cfg.mods_path, &bike, &variant).map_err(|e| format!("{e:#}"))?;
    let label = format!("{bike} · {variant}");
    let tyres = library::mods_subdir(&cfg.mods_path, "mods/tyres");
    let key = format!(
        "{}#p{:x}#t{:x}#w{}",
        swap_cache_key(&set),
        paints_stamp(&set.bike_dir),
        tyres_stamp(&tyres),
        pick.as_deref().unwrap_or(""),
    );
    if let Some(m) = bike_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        log::info!("preview_model_swap {label}: cache hit ({:?})", t0.elapsed());
        return Ok(m);
    }

    let files = gather_preview_files(&set).map_err(|e| format!("{e:#}"))?;
    let installed = paints_at(&set.paints);
    build_bike_model(&label, key, files, installed, Some(tyres), pick, t0)
}

/// A stamp over the loose paints beside a bike, for the cache key to carry.
///
/// The bike's own file can't see them. A `.pnt` is written into `<bike>/paints/`, which
/// leaves the `.pkz`'s mtime and size exactly as they were — and on a bike loaded from its
/// folder, writing a file inside `paints/` doesn't touch the folder above it either. So the
/// key matched, the cache answered, and the model handed back was the one read before the
/// paint existed: you saved, the bike didn't change, and nothing in the log looked wrong.
///
/// Name, length and mtime per `.pnt`, sorted so `read_dir` order can't shuffle the answer.
/// Covers all three ways the set can move — a paint added, removed, or re-saved in place.
fn paints_stamp(source: &std::path::Path) -> u64 {
    let folder = if source.is_dir() {
        source.to_path_buf()
    } else {
        source.with_extension("")
    };
    let mut rows: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(folder.join("paints")) {
        for e in entries.flatten() {
            let path = e.path();
            if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("pnt")) {
                continue;
            }
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            rows.push(format!(
                "{}:{len}:{}",
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                mtime_nanos(&path),
            ));
        }
    }
    rows.sort_unstable();
    fnv1a(&rows)
}

/// A stamp over the installed tyre mods, for the cache key to carry.
///
/// The hole [`paints_stamp`] fills, one folder out. A bike's wheels come from
/// `mods/tyres/<name>`, which nothing on the bike's own path can see: swapping that mod
/// changes what the viewer should draw while the bike's mtime, size and paints all stay
/// exactly as they were.
fn tyres_stamp(dir: &std::path::Path) -> u64 {
    let mut rows: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            rows.push(format!(
                "{}:{len}:{}",
                e.file_name().to_string_lossy(),
                mtime_nanos(&e.path()),
            ));
        }
    }
    rows.sort_unstable();
    fnv1a(&rows)
}

/// FNV-1a over the rows a stamp is built from. Not a security question — this only has to
/// change when the folder does.
fn fnv1a(rows: &[String]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for row in rows {
        for b in row.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    }
    h
}

fn load_bike_model_blocking(
    source: String,
    pick: Option<String>,
) -> Result<BikeModel, String> {
    let t0 = std::time::Instant::now();
    let tyres = tyres_dir_for(std::path::Path::new(&source));
    let key = format!(
        "{}#p{:x}#t{:x}#w{}",
        bike_cache_key(&source),
        paints_stamp(std::path::Path::new(&source)),
        tyres.as_deref().map(tyres_stamp).unwrap_or(0),
        pick.as_deref().unwrap_or(""),
    );
    if let Some(m) = bike_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        log::info!("load_bike_model {source}: cache hit ({:?})", t0.elapsed());
        return Ok(m);
    }

    let files = gather_bike_files(std::path::Path::new(&source)).map_err(|e| format!("{e:#}"))?;
    let installed = installed_paints(std::path::Path::new(&source));
    build_bike_model(&source, key, files, installed, tyres, pick, t0)
}

/// Turn a bike's files into the viewer's model: resolve each part's mesh through the
/// `.hrc`s, bind its textures, decode the paints. Shared by a bike loaded from disk and a
/// model-swap preview assembled in memory — `label` only names it in the log.
fn build_bike_model(
    label: &str,
    key: String,
    files: Vec<(String, Vec<u8>)>,
    // The loose paints beside the bike, as `installed_paints` answers them.
    installed: Vec<(String, String, Vec<u8>)>,
    // Where the tyre mods live, for the wheels this bike wears. `None` skips them.
    tyres_dir: Option<std::path::PathBuf>,
    // The tyre pack the player picked, if any. Blank/absent → the one the bike names.
    tyres_pick: Option<String>,
    t0: std::time::Instant,
) -> Result<BikeModel, String> {
    let t_read = t0.elapsed();

    let mut nodes = Vec::new();
    // Every mesh the bike ships, by file name — usually just `model.edf`, but a bike can
    // carry one per part. Which are actually used is decided by the `.hrc`s below.
    let mut edfs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut geom: Option<&Vec<u8>> = None;
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut tga_jobs: Vec<(String, &[u8])> = Vec::new();
    // (display name, bytes, shipped-in-the-archive, path on disk if it has one)
    let mut pnt_jobs: Vec<(String, &[u8], bool, Option<&str>)> = Vec::new();
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
            pnt_jobs.push((paint_display_name(&bn), data.as_slice(), true, None));
        }
    }

    let gfx = gfx_bytes.map(|b| cfg::parse_gfx(b)).unwrap_or_default();
    // Read before `used` borrows anything, so the wheel meshes outlive the borrows taken of
    // them below. `edfs` is only consulted so an unreadable bike doesn't pay for a tyre
    // archive it will never draw.
    let tyre_set = match (tyres_dir, gfx_bytes, geom) {
        (Some(dir), Some(bytes), Some(g)) if !edfs.is_empty() => {
            if edf::wheel_axles(g).is_some() {
                gather_tyre_files(&dir, bytes, tyres_pick.as_deref())
            } else {
                log::warn!("[viewer] {label}: the .geom names no axles — no wheels");
                None
            }
        }
        _ => None,
    };
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
    // Wheels last, and only onto a bike that arrived: a mesh that didn't read has to go on
    // reading as "none of this bike arrived", not as a pair of wheels hanging in the air.
    let mut tyres = None;
    if let Some(set) = tyre_set.as_ref().filter(|_| !nodes.is_empty()) {
        let (mut wheels, meshes) = wheel_nodes(&set.files);
        if wheels.is_empty() {
            log::warn!("[viewer] {label}: tyres '{}' hold no readable wheel mesh", set.name);
        } else {
            tyres = Some(set.name.clone());
        }
        nodes.append(&mut wheels);
        used.extend(meshes);
    }
    for (fname, path, data) in &installed {
        pnt_jobs.push((paint_display_name(fname), data.as_slice(), false, Some(path)));
    }
    // Whether the parts ended up in one frame. Logged rather than printed: it decides what the
    // Designer may say about a sheet's flanks, so "was this bike assembled?" has to be
    // answerable from the log file after the fact, not only from a terminal nobody kept.
    let mut rig = match geom {
        Some(g) => {
            let rig = edf::assemble_bike(&mut nodes, g);
            if rig.is_none() {
                log::warn!("[viewer] {label}: .geom present but missing mount points — parts unassembled");
            }
            rig
        }
        None => {
            if !nodes.is_empty() {
                log::warn!("[viewer] {label}: no .geom alongside the mesh — parts unassembled");
            }
            None
        }
    };
    let assembled = rig.is_some();
    edf::to_right_handed(&mut nodes);
    // The rig names points on the mesh, so it goes through the same mirror the mesh does.
    if let Some(r) = rig.as_mut() {
        r.to_right_handed();
    }
    // Nothing to draw. Returning a model with no nodes is worse than failing: the viewer reads
    // it as a successful load and puts its stand-in bike on screen, which reads as "this is your
    // bike" rather than "none of this bike arrived". A cloud-synced archive that hasn't been
    // downloaded lands here — every entry reads short, so the `.edf` never appears.
    if nodes.is_empty() {
        return Err(format!(
            "{label} holds no readable mesh — if the file is cloud-synced, it may not be fully downloaded yet"
        ));
    }
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
        .filter_map(|(name, data, shipped, path)| {
            paint::decode_any(data).ok().map(|pnt| {
                (
                    BikePaint {
                        name: name.clone(),
                        path: path.map(str::to_string),
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

    // Kept before the folding below, which is where the model's own look stops being
    // telling apart from a paint's. Names and tokens only — no pixels are copied.
    let model_base = base.clone();

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
            // The mesh's own textures, which live inside it rather than in a `.pnt`.
            path: None,
            textures: base,
            changes_preview: true, // the model's own textures, by definition
        });
    }

    let distinct_tex: std::collections::HashSet<&str> = paints
        .iter()
        .flat_map(|p| p.textures.iter().map(|t| t.token.as_str()))
        .collect();
    // The phase split, on stdout, for the `bike_load_timing` diagnostic — `log` has no
    // subscriber under `cargo test`, and this is the breakdown that says where to optimise.
    if std::env::var_os("MXB_PHASE_TIMES").is_some() {
        println!(
            "  parse mesh           {:>9.2?}\n  decode paints        {:>9.2?}  ({} paint(s), {base_count} base tex)",
            t_parse - t_read,
            t_textures - t_parse,
            paints.len(),
        );
    }
    log::info!(
        "load_bike_model {label}: {} paint(s) + {base_count} base tex | read {t_read:?}, parse {:?}, decode {:?}, total {:?} | {} distinct texture(s), {:.1} MB resident in the texture store",
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

    let model = BikeModel { nodes, paints, base: model_base, tyres, assembled, rig };
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
    // Which list this mesh's material indices count. A mesh whose materials never use the
    // second texture slot is read exactly as it always was; only one that does — a mod
    // shipping companion maps, unreadable until now — gets the companion-aware list.
    let colors = if edf::uses_companion_slots(edf_bytes) {
        edf::bike_material_slots(edf_bytes)
    } else {
        edf::declared_colors(edf_bytes, &[])
    };

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

/// The loose `.pnt`s installed beside a bike, as (file name, full path, bytes).
///
/// The path rides along because these are the only paints that can change under the viewer:
/// they are ordinary files a painter re-saves, where the ones inside the archive are not.
/// Read an already-resolved livery list. A preview's liveries come from
/// `modelswap::PreviewSet`, which knows the shelf — reading `paints/` directly would show
/// whatever the model *currently* on the bike offers, not the one being previewed.
fn paints_at(paths: &[std::path::PathBuf]) -> Vec<(String, String, Vec<u8>)> {
    paths
        .iter()
        .filter_map(|p| {
            let name = p.file_name().and_then(|n| n.to_str())?.to_string();
            let full = p.to_str()?.to_string();
            Some((name, full, std::fs::read(p).ok()?))
        })
        .collect()
}

fn installed_paints(source: &std::path::Path) -> Vec<(String, String, Vec<u8>)> {
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
                if let (Some(name), Some(full), Ok(bytes)) = (
                    p.file_name().and_then(|n| n.to_str()),
                    p.to_str(),
                    std::fs::read(&p),
                ) {
                    out.push((name.to_string(), full.to_string(), bytes));
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

/// The `tyres` folder beside a bike, where the wheels it wears come from.
///
/// A bike source is `<mods>/bikes/<Bike>` or `<mods>/bikes/<Bike>.pkz`, so the sibling
/// folder is two levels up. Derived rather than configured: `load_bike_model` is handed a
/// path and nothing else, and that path already says where the mods tree is.
fn tyres_dir_for(source: &std::path::Path) -> Option<std::path::PathBuf> {
    // Resolved, not joined: under Proton the tree is case-sensitive and a `Tyres` folder is
    // a different path from `tyres`.
    Some(library::resolve_child(source.parent()?.parent()?, "tyres"))
}

/// Whether `mods/tyres/<name>` is installed, as a folder or as the `.pkz` beside it.
fn tyres_mod_exists(tyres_dir: &std::path::Path, name: &str) -> bool {
    library::resolve_child(tyres_dir, name).is_dir()
        || library::resolve_child(tyres_dir, &format!("{name}.pkz")).is_file()
}

/// A tyres mod, opened: the name it goes by and the files a wheel resolves through.
struct TyreSet {
    name: String,
    files: Vec<(String, Vec<u8>)>,
}

/// Open the tyres mod a bike will wear — its own `gfx.cfg`, an `.hrc` per wheel, and the
/// meshes those name.
///
/// A bike ships no wheel of its own. Its `gfx.cfg` ends with one line — `tyres = oem_mx` —
/// and `mods/tyres/oem_mx`, a folder or the `.pkz` beside it, is where the mesh actually
/// lives. `pick` substitutes that name so a bike can be *seen* on another pack; nothing on
/// disk moves and the bike's own `gfx.cfg` still reads as the game will read it.
///
/// `None` when there is no line, no mod, or nothing readable in one — the bike the viewer
/// drew before wheels, not a failure.
fn gather_tyre_files(
    tyres_dir: &std::path::Path,
    gfx_bytes: &[u8],
    // The pack the player picked, if they picked one. Blank or absent → the bike's own.
    pick: Option<&str>,
) -> Option<TyreSet> {
    let root = cfg::parse(gfx_bytes);
    let own = root.get("tyres").map(str::trim).filter(|n| library::is_simple_name(n));
    // A pick that names nothing installed falls back to the bike's own rather than taking
    // the wheels away: the picker is a way to look at a bike, not a way to break it.
    let name = match pick.map(str::trim).filter(|n| !n.is_empty()) {
        Some(p) if library::is_simple_name(p) && tyres_mod_exists(tyres_dir, p) => p,
        Some(p) => {
            log::warn!("[viewer] tyres '{p}' isn't installed — falling back to the bike's own");
            own?
        }
        None => own?,
    }
    .to_string();

    // The `.tyre` parameter files, the previews and the shadow meshes all sit beside these
    // and none of them are drawn — read only what a wheel is resolved through.
    let want = |n: &str| {
        let n = n.rsplit(['/', '\\']).next().unwrap_or(n).to_ascii_lowercase();
        n.ends_with(".edf") || n.ends_with(".hrc") || n.ends_with(".cfg")
    };

    let dir = library::resolve_child(tyres_dir, &name);
    let mut loose = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let path = e.path();
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if path.is_file() && want(fname) {
                if let Ok(bytes) = std::fs::read(&path) {
                    loose.push((fname.to_string(), bytes));
                }
            }
        }
    }
    if !loose.is_empty() {
        return Some(TyreSet { name, files: loose });
    }

    let pkz = library::resolve_child(tyres_dir, &format!("{name}.pkz"));
    if !pkz.is_file() {
        log::warn!("[viewer] tyres '{name}' isn't installed — no wheels");
        return None;
    }
    match pkz::read_selected(&pkz, want) {
        Ok(files) => Some(TyreSet { name, files }),
        Err(e) => {
            log::warn!("[viewer] tyres '{name}' wouldn't read: {e:#} — no wheels");
            None
        }
    }
}

/// The wheel nodes out of a tyres mod, textured and ready for the `.geom` to mount.
///
/// Same shape as a bike's own parts — `gfx.cfg` names an `.hrc` per wheel, and the `.hrc`'s
/// level0 names both the node and the mesh it lives in — so the bike's own resolution reads
/// it unchanged. Returns the nodes and the meshes they came out of, which the caller needs
/// in order to lift the wheel textures out of them.
fn wheel_nodes(files: &[(String, Vec<u8>)]) -> (Vec<edf::EdfNode>, Vec<&Vec<u8>>) {
    let mut edfs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut hrcs: std::collections::HashMap<String, &Vec<u8>> = std::collections::HashMap::new();
    let mut gfx_bytes: Option<&Vec<u8>> = None;
    for (name, data) in files {
        let bn = name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase();
        if bn.ends_with(".edf") {
            edfs.insert(bn, data);
        } else if let Some(stem) = bn.strip_suffix(".hrc") {
            hrcs.insert(stem.to_string(), data);
        } else if bn.ends_with("gfx.cfg") {
            gfx_bytes = Some(data);
        }
    }
    let Some(gfx_bytes) = gfx_bytes else {
        log::warn!("[viewer] the tyres mod ships no gfx.cfg — no wheels");
        return (Vec::new(), Vec::new());
    };
    let gfx = cfg::parse(gfx_bytes);

    let mut nodes = Vec::new();
    let mut used: Vec<&Vec<u8>> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    // Front then rear, fixed: node order must not shuffle between runs.
    for part in ["front_wheel", "rear_wheel"] {
        let Some(hrc_file) = gfx
            .block(part)
            .and_then(|p| p.block("model"))
            .and_then(|m| m.get("file"))
        else {
            continue;
        };
        let stem = hrc_file
            .trim_end_matches(".hrc")
            .trim_end_matches(".HRC")
            .to_ascii_lowercase();
        let Some(bytes) = hrcs.get(&stem) else {
            log::warn!("[viewer] tyres '{part}' wants {hrc_file}, which the mod doesn't ship");
            continue;
        };
        let hrc = cfg::parse(bytes);
        let Some(node) = cfg::hrc_level0(&hrc, &stem) else { continue };
        let scene = cfg::hrc_level0_scene(&hrc)
            .map(|s| s.replace('\\', "/"))
            .and_then(|s| s.rsplit('/').next().map(str::to_ascii_lowercase))
            .unwrap_or_else(|| "model.edf".to_string());
        let Some(data) = edfs.get(&scene) else {
            log::warn!("[viewer] a tyres .hrc wants {scene}, which the mod doesn't ship");
            continue;
        };
        let mut part_nodes = edf::parse_with_levels(data, &[node]);
        // No gfx overrides: a wheel binds straight off its own material table.
        bind_textures(
            &mut part_nodes,
            data,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        drop_chain(&mut part_nodes);
        nodes.append(&mut part_nodes);
        if !seen.contains(&scene) {
            seen.push(scene);
            used.push(data);
        }
    }
    (nodes, used)
}

fn is_chain(sm: &edf::Submesh) -> bool {
    sm.texture.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("chain"))
}

/// Take the chain off the wheels — the one thing the wheel mesh carries that the viewer
/// can't draw.
///
/// It ships as a straight template strip that the game bends onto the sprockets from the
/// `pos`/`engine`/`ratio` the *bike's* `gfx.cfg` gives, geometry we don't build. Drawn where
/// it sits it is a bar standing 0.7 m out of the rear wheel.
///
/// A node that was nothing but chain goes entirely, since one left with no groups at all is
/// drawn whole on a single texture rather than not at all.
fn drop_chain(nodes: &mut Vec<edf::EdfNode>) {
    nodes.retain_mut(|n| {
        // No submesh table: a whole-node binding, and not ours to judge.
        if n.submeshes.is_empty() || !n.submeshes.iter().any(is_chain) {
            return true;
        }
        n.submeshes.retain(|sm| !is_chain(sm));
        if n.submeshes.is_empty() {
            return false;
        }
        compact_to_submeshes(n);
        true
    });
}

/// Rebuild a node around the submeshes it has left, so what it no longer draws stops
/// counting for anything else either.
///
/// Dropping a submesh on its own leaves its triangles — and their vertices — in the buffers.
/// Nothing draws them, but everything that *measures* the model still sees them, and the
/// chain's 0.7 m of template was enough to move where the viewer centres the bike and how
/// far `SideBySide` drops it onto the ground.
fn compact_to_submeshes(n: &mut edf::EdfNode) {
    let old_idx = std::mem::take(&mut n.indices);
    let old_pos = std::mem::take(&mut n.positions);
    let old_uv = std::mem::take(&mut n.uvs);
    let old_nrm = std::mem::take(&mut n.normals);
    let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut indices: Vec<u32> = Vec::with_capacity(old_idx.len());
    let mut tri_start = 0u32;

    for sm in n.submeshes.iter_mut() {
        let from = (sm.tri_start as usize).saturating_mul(3);
        let to = from.saturating_add((sm.tri_count as usize).saturating_mul(3));
        let range = old_idx.get(from..to).unwrap_or(&[]);
        for &v in range {
            let slot = match remap.get(&v) {
                Some(&slot) => slot,
                None => {
                    let slot = (n.positions.len() / 3) as u32;
                    let o = v as usize;
                    n.positions
                        .extend_from_slice(old_pos.get(o * 3..o * 3 + 3).unwrap_or(&[0.0; 3]));
                    if !old_uv.is_empty() {
                        n.uvs.extend_from_slice(old_uv.get(o * 2..o * 2 + 2).unwrap_or(&[0.0; 2]));
                    }
                    if !old_nrm.is_empty() {
                        n.normals
                            .extend_from_slice(old_nrm.get(o * 3..o * 3 + 3).unwrap_or(&[0.0; 3]));
                    }
                    remap.insert(v, slot);
                    slot
                }
            };
            indices.push(slot);
        }
        sm.tri_start = tri_start;
        sm.tri_count = (range.len() / 3) as u32;
        tri_start += sm.tri_count;
    }
    n.indices = indices;
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
        let mut loose = Vec::new();
        for entry in std::fs::read_dir(p).with_context(|| format!("read dir {p:?}"))? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).map(str::to_string);
            if path.is_file() && name.as_deref().is_some_and(wanted_bike_file) {
                if let (Some(name), Ok(bytes)) = (name, std::fs::read(&path)) {
                    loose.push((name, bytes));
                }
            }
        }
        // Packed first, loose over it. A folder holding only a swapped-in mesh still draws
        // with the `.geom`, `gfx.cfg` and stock paint that never left the archive; taking
        // the loose files alone left every part stacked at the origin and untextured.
        let mut out = packed_layer(p);
        overlay_files(&mut out, loose);
        // A mesh of any name will do — `model.edf` is the convention, not a rule.
        if !out.iter().any(|(n, _)| bikefiles::is_mesh(n)) {
            bail!("no .edf mesh for bike folder {p:?}");
        }
        return Ok(out);
    }
    bail!("can't load a bike model from {p:?}")
}

/// Add `incoming` to `files`, replacing any entry of the same name — later wins, which is
/// how the game reads a bike too: loose files layer over the packed archive, and a swap's
/// files layer over the loose ones.
fn overlay_files(files: &mut Vec<(String, Vec<u8>)>, incoming: Vec<(String, Vec<u8>)>) {
    for (name, data) in incoming {
        let bn = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_ascii_lowercase();
        match files
            .iter_mut()
            .find(|(n, _)| n.rsplit(['/', '\\']).next().unwrap_or(n).eq_ignore_ascii_case(&bn))
        {
            Some(slot) => *slot = (name, data),
            None => files.push((name, data)),
        }
    }
}

/// Read the named files out of `dir`, keeping only what the viewer draws with.
fn read_named(dir: &std::path::Path, names: &[String]) -> Vec<(String, Vec<u8>)> {
    names
        .iter()
        .filter(|n| wanted_bike_file(n))
        .filter_map(|n| std::fs::read(dir.join(n)).ok().map(|b| (n.clone(), b)))
        .collect()
}

/// The bike's packed model, either inside the folder or as its `<Bike>.pkz` sibling —
/// both layouts exist, and it's the fallback a Stock preview shows.
fn packed_bike(bike_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if let Ok(rd) = std::fs::read_dir(bike_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkz")) {
                return Some(p);
            }
        }
    }
    let sibling = library::sibling_pkz(bike_dir);
    sibling.exists().then_some(sibling)
}

/// The bike's packed layer, or nothing at all.
///
/// An archive that won't read — a locked one, or a stub iCloud has evicted — must not take
/// down a bike whose loose folder can still be drawn. Callers bail later if what's left
/// holds no mesh.
fn packed_layer(bike_dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let Some(pkz) = packed_bike(bike_dir) else { return Vec::new() };
    match pkz::read_selected(&pkz, wanted_bike_file) {
        Ok(files) => files,
        Err(e) => {
            log::warn!("[viewer] couldn't read {pkz:?} ({e:#}) — drawing the loose files alone");
            Vec::new()
        }
    }
}

/// The bytes behind a `PreviewSet`: the packed bike, with the loose files that stay laid
/// over it and the variant's over those. Stock parks every loose mesh and so draws the
/// packed model itself; every other variant draws its own mesh on the same foundation.
fn gather_preview_files(
    set: &modelswap::PreviewSet,
) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use anyhow::bail;
    // Packed, then the loose root, then the variant — the game's own order. The archive is
    // never skipped: a swap ships a mesh and little else, so the bike's `.geom`, `gfx.cfg`
    // and `.hrc`s have nowhere else to come from.
    let mut out = packed_layer(&set.bike_dir);
    overlay_files(&mut out, read_named(&set.bike_dir, &set.root_keep));
    overlay_files(&mut out, read_named(&set.variant_dir, &set.variant_files));
    if !out.iter().any(|(n, _)| bikefiles::is_mesh(n)) {
        bail!("this model has no mesh to show — the bike would have no model at all");
    }
    Ok(out)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RiderPart {
    part: String,
    nodes: Vec<edf::EdfNode>,
    textures: Vec<paint::PaintTexture>,
    /// The body's rig, in the frame `nodes` came back in. Only the body has one — gear is
    /// rigid and hangs off a bone rather than carrying any.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skeleton: Vec<edf::Bone>,
    /// Which bones move which vertices. Empty unless `skeleton` is filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    skin: Option<edf::Skin>,
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
    let skeleton = body_rig(&src, profile);
    let skin = (!skeleton.is_empty()).then(|| body_skin(&src, profile, &nodes, &skeleton));
    Some(RiderPart {
        part: "body".into(),
        nodes,
        textures,
        skeleton,
        skin,
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
/// The turn a Z-up body takes: a half turn about Y on top of a quarter turn about X.
/// Named here because the rig has to take exactly the same one — see [`body_rig`].
const BODY_STAND_UP: [[f32; 3]; 3] =
    [[-1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]];

/// Is this body authored Z-up — lying on its back, longest axis in Z?
fn body_is_z_up(ext: [f32; 3]) -> bool {
    ext[2] > ext[1] && ext[2] > ext[0]
}

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
    if !body_is_z_up(ext) {
        return;
    }
    for n in nodes.iter_mut() {
        for v in n.positions.chunks_exact_mut(3).chain(n.normals.chunks_exact_mut(3)) {
            let (x, y, z) = (v[0], v[1], v[2]);
            let r = BODY_STAND_UP;
            // Negated, not taken as-is: these meshes lie head-away, with the head at the
            // most negative Z, so this is what puts it at the top.
            v[0] = r[0][0] * x + r[0][1] * y + r[0][2] * z;
            v[1] = r[1][0] * x + r[1][1] * y + r[1][2] * z;
            v[2] = r[2][0] * x + r[2][1] * y + r[2][2] * z;
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

/// Rigs are tiny — 65 bones of two matrices each — so this holds more than the mesh cache.
fn rig_cache() -> &'static std::sync::Mutex<lru::Lru<Vec<edf::Bone>>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<Vec<edf::Bone>>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP * 2)))
}

/// A rider body's rig, in the same frame the viewer gets its mesh in, memoised.
///
/// The mesh takes two turns on the way out of the file — [`edf::to_right_handed`] mirrors X,
/// then [`stand_body_upright`] stands a Z-up body up — and the rig has to take both, or the
/// skeleton ends up mirrored or lying beside a standing body. Whether the second one applies
/// is decided from the rig's own extents rather than the mesh's: both are authored in the
/// same frame, so they agree, and asking the rig costs nothing where asking the mesh would
/// mean parsing 67 MB a second time.
fn body_rig(src: &BodySource, profile: &str) -> Vec<edf::Bone> {
    let key = format!("rig:{}", src.cache_key(profile));
    if let Some(r) = rig_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        return r;
    }
    let mut rig = src.read(profile).map(|b| edf::parse_skeleton(&b)).unwrap_or_default();
    if !rig.is_empty() {
        edf::transform_skeleton(&mut rig, [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for b in rig.iter() {
            let o = b.origin();
            for a in 0..3 {
                lo[a] = lo[a].min(o[a]);
                hi[a] = hi[a].max(o[a]);
            }
        }
        if body_is_z_up([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) {
            edf::transform_skeleton(&mut rig, BODY_STAND_UP);
        }
        log::info!("[rider] body '{profile}' rig: {} bones", rig.len());
    }
    if let Ok(mut c) = rig_cache().lock() {
        c.insert(key, rig.clone());
    }
    rig
}

/// The body's binding to its rig, memoised. Working it out is quick — a third of a million
/// point-to-segment distances — but it depends only on the model, and a loadout change must
/// not pay for it again.
fn body_skin(
    src: &BodySource,
    profile: &str,
    nodes: &[edf::EdfNode],
    rig: &[edf::Bone],
) -> edf::Skin {
    let key = format!("skin:{}", src.cache_key(profile));
    if let Some(s) = skin_cache().lock().ok().and_then(|mut c| c.get(&key).cloned()) {
        return s;
    }
    let skin = edf::skin_mesh(nodes, rig);
    if let Ok(mut c) = skin_cache().lock() {
        c.insert(key, skin.clone());
    }
    skin
}

fn skin_cache() -> &'static std::sync::Mutex<lru::Lru<edf::Skin>> {
    static C: std::sync::OnceLock<std::sync::Mutex<lru::Lru<edf::Skin>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(lru::Lru::new(MESH_CACHE_CAP)))
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
            skeleton: Vec::new(),
        skin: None,
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
    Ok(RiderPart { part, nodes, textures: out, skeleton: Vec::new(), skin: None })
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
        skeleton: Vec::new(),
        skin: None,
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
        skeleton: Vec::new(),
        skin: None,
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
    let _cancel = cancel::begin(&slug);
    install::add_to_library(&app, &cfg, &slug, &url, &host, &subpath, &dest_folder)
        .await
        .map_err(|e| format!("{e:#}"))
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

/// Where MXB App's own logs are, where the game's are, and what's currently in each.
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

/// What to do with the login item at startup.
#[derive(Debug, PartialEq, Eq)]
enum Autostart {
    Leave,
    Enable,
    /// It is there, but it was written for a binary this build no longer has.
    Rebind,
    Disable,
}

/// Reconcile the login item with the setting.
///
/// `stale` is the case that isn't obvious: the entry holds the executable's absolute path, so
/// renaming the binary leaves every existing one pointing at a file that is gone — while
/// `is_enabled` still answers yes, because all it looks for is the entry. Left alone, the app
/// simply stops starting at login and nothing ever says why.
fn autostart_action(wanted: bool, enabled: bool, stale: bool) -> Autostart {
    match (wanted, enabled) {
        (true, false) => Autostart::Enable,
        (true, true) if stale => Autostart::Rebind,
        (false, true) => Autostart::Disable,
        _ => Autostart::Leave,
    }
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
        // Both directions, because Play is the last moment before either one matters: the
        // grid needs everyone else's paints on disk, and everyone else needs ours.
        publish_paints_soon(&app, &cfg, None);
        live_sync_session(&app, None);
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
    if watches_looks(&cfg) {
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

/// Is a look change worth noticing? Paint sync publishes them, and the look watcher rebuilds
/// on them — either one is reason enough to watch `profile.ini`.
fn watches_looks(cfg: &AppConfig) -> bool {
    cfg.experimental_enabled() || can_refresh_live_look()
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

/// How often to re-check the grid while a session is running.
///
/// A rider who joins after you did is invisible until the next pull, so this is the gap
/// between someone arriving and their paint appearing. Short enough not to matter in a race,
/// long enough that an unchanged roster — the overwhelmingly common answer — costs nothing.
const LIVE_SYNC_EVERY: std::time::Duration = std::time::Duration::from_secs(45);

/// How long to wait for the game to show up before giving up on a session.
///
/// MX Bikes takes a while to appear in the process list, and on a platform where we cannot
/// see processes at all it never will. Either way, stopping is right: the launch pull has
/// already run.
const LIVE_SYNC_STARTUP_GRACE: std::time::Duration = std::time::Duration::from_secs(120);

/// A session's worth of syncing, so a rider who arrives after you do still renders.
///
/// The pull used to happen once, at launch. That made the whole thing lopsided: whoever
/// joined last saw everybody, and whoever was there first never saw anyone who turned up
/// afterwards — they had pulled before those riders existed.
///
/// Runs until the game exits. Each pass also re-reports presence, which is what keeps this
/// rider in other people's rosters; the control plane forgets anyone who goes quiet.
fn live_sync_session(app: &tauri::AppHandle, address: Option<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        let mut seen_running = false;
        loop {
            tokio::time::sleep(LIVE_SYNC_EVERY).await;

            let cfg = config::load_or_detect(&app).unwrap_or_default();
            if !cfg.experimental_enabled() || cfg.cp_token.trim().is_empty() {
                return;
            }
            // The game is the session. Once it has been seen and then gone, so are we.
            if gameproc::is_game_running() {
                seen_running = true;
            } else if seen_running || started.elapsed() > LIVE_SYNC_STARTUP_GRACE {
                log::info!("[sync] session over, stopping the live sync");
                return;
            }

            match pull_rosters(&app, address.clone()).await {
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

    // Say where we are before asking who else is here: the roster is scoped by presence, so
    // reporting first is what puts this rider into everyone else's grid too.
    for key in &keys {
        if let Err(e) = paintsync::report_presence(&cfg.cp_token, key).await {
            log::debug!("[sync] couldn't report presence on {key}: {e:#}");
        }
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
        live_sync_session(&app, Some(address.clone()));
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

/// Note a finished download — installed or failed. Called from the two places every install
/// passes through (`Context/Install` and `Context/DropReview`), which is why nothing in the
/// download paths themselves has to know history exists.
#[tauri::command]
fn record_download(
    app: tauri::AppHandle,
    entry: downloads::NewDownload,
) -> Result<Option<downloads::DownloadRecord>, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| format!("{e:#}"))?;
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

/// Read a share code without downloading anything — the import dialog's preview.
#[tauri::command]
fn file_share_preview(text: String) -> Result<fileshare::FileShare, String> {
    fileshare::decode(&text).map_err(|e| format!("{e:#}"))
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

/// What the session looks like, read once so the choice below is a pure function of it —
/// the only way to test any of this from a machine that isn't the one it's for.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct GraphicsEnv {
    /// The compositor's socket — set on any Wayland session, gamescope included.
    wayland: bool,
    /// An X server is reachable, real or XWayland.
    x_server: bool,
    /// `MXB_SAFE_GRAPHICS=1`, for a white screen the defaults didn't cure.
    safe_mode: bool,
}

impl GraphicsEnv {
    fn read() -> Self {
        let set = |key: &str| std::env::var_os(key).is_some_and(|v| !v.is_empty());
        Self {
            wayland: set("WAYLAND_DISPLAY"),
            x_server: set("DISPLAY"),
            safe_mode: std::env::var("MXB_SAFE_GRAPHICS").unwrap_or_default() == "1",
        }
    }
}

/// The environment WebKitGTK should start under: the renderer every session falls back to,
/// and the knobs `MXB_SAFE_GRAPHICS` turns when a window still won't paint.
fn webview_env_defaults(env: GraphicsEnv) -> Vec<(&'static str, &'static str)> {
    // DMA-BUF asks the WebKit our AppImage carries from Ubuntu 22.04 to negotiate buffers
    // with whatever Mesa the host ships; where that fails it fails silently, painting
    // nothing. The shared-memory fallback costs a copy per frame — imperceptible on a UI
    // of mostly static lists — and paints everywhere.
    let mut vars = vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1")];

    // The other fault — WebKitGTK 2.46+ aborting because it can't create an EGL display —
    // was the bundled libwayland, and it is fixed where it was made: the AppImage no longer
    // carries those libraries at all (scripts/appimage-drop-bundled-wayland.sh). Nothing
    // here could have fixed it, and the version that tried never even ran: an AppImage's
    // AppRun hook exports `GDK_BACKEND=x11` itself, before this process starts, so the
    // default below was already set by the time we looked.
    //
    // What's left is the hand-operated fallback: an X server is a second graphics stack to
    // land on when a machine still won't paint. Guarded on there being one — forcing the
    // backend without it trades a white screen for no window at all.
    if env.safe_mode && env.wayland && env.x_server {
        vars.push(("GDK_BACKEND", "x11"));
    }

    // Asked for by hand, once the above wasn't enough: take the GPU out of it entirely.
    if env.safe_mode {
        vars.push(("WEBKIT_DISABLE_COMPOSITING_MODE", "1"));
        vars.push(("LIBGL_ALWAYS_SOFTWARE", "1"));
    }

    vars
}

/// Defaults, not overrides — anything already set explicitly wins, so a machine whose driver
/// stack handles the fast paths can ask for them back with `GDK_BACKEND=wayland`.
///
/// Has to run before the first window is built, since WebKit reads these when it spawns the
/// web process, and before any other thread exists — being `main`'s first statement gives
/// both.
fn prepare_webview_env() {
    if !cfg!(target_os = "linux") {
        return;
    }
    for (key, value) in webview_env_defaults(GraphicsEnv::read()) {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
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
        .manage(voice::Monitor::default())
        .manage(voice::session::Session::default())
        .setup(|app| {
            log::info!("MXB App {} starting", env!("CARGO_PKG_VERSION"));

            // The main window is `"create": false` in tauri.conf.json so it is built here
            // rather than by Tauri's own startup loop, which is the only way to decide the
            // drag-drop handler per run: it can only be turned off while the window is
            // being built. Everything else about the window still comes from the config,
            // the macOS overrides in `tauri.macos.conf.json` included — and that file
            // replaces this array wholesale, so it has to repeat `"create": false` or
            // Tauri opens `main` itself and the build below aborts on the duplicate.
            let drag_drop = drag_drop_enabled();
            log::info!("wine={} drag-drop-handler={}", under_wine(), drag_drop);
            for window_config in app
                .config()
                .app
                .windows
                .iter()
                .filter(|w| w.label == MAIN_WINDOW)
            {
                let mut builder =
                    tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?;
                if !drag_drop {
                    builder = builder.disable_drag_drop_handler();
                }
                builder.build()?;
            }
            // Cloudflare scores the User-Agent alongside the IP, and a cf_clearance is bound
            // to the UA that earned it — a log about a block should say which one was used.
            log::info!("{} user-agent: {}", mxb_session::site().domain, mxb_session::UA);
            // A blank webview leaves nothing else behind to diagnose from, so record the
            // session this run started under and every knob `prepare_webview_env` settled on.
            if cfg!(target_os = "linux") {
                let on = |key: &str| std::env::var(key).unwrap_or_default() == "1";
                log::info!(
                    "webview env: appimage={} wayland={} x_server={} gdk_backend={} \
                     dmabuf_disabled={} compositing_disabled={} software_gl={}",
                    std::env::var_os("APPIMAGE").is_some(),
                    std::env::var_os("WAYLAND_DISPLAY").is_some(),
                    std::env::var_os("DISPLAY").is_some(),
                    std::env::var("GDK_BACKEND").unwrap_or_else(|_| "default".into()),
                    on("WEBKIT_DISABLE_DMABUF_RENDERER"),
                    on("WEBKIT_DISABLE_COMPOSITING_MODE"),
                    on("LIBGL_ALWAYS_SOFTWARE"),
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
                let stale = cfg.autostart_binding_rev < config::AUTOSTART_BINDING_REV;
                match autostart_action(
                    cfg.launch_at_startup,
                    manager.is_enabled().unwrap_or(false),
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
                    Autostart::Leave => {}
                }
                if stale {
                    cfg.autostart_binding_rev = config::AUTOSTART_BINDING_REV;
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
                // The publish no-ops unless the experimental features are on and an account
                // exists; the watching is also what keeps the look watcher pointed at the
                // right files, which has nothing to do with sync.
                if cfg.experimental_enabled() {
                    publish_paints_soon(handle, &cfg, None);
                }
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
            // Voice follows the rider onto whatever server they join, and off it again.
            // There is nothing to press: the supervisor is the whole of "joining a room".
            voice::session::start(handle);
            shop_session::load_session(handle);
            shop_catalog_session::load(handle);
            mxb_session::load(handle);
            imgcache::start_maintenance(handle);
            memwatch::start();
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
            read_track_info,
            load_track_terrain,
            load_track_overview,
            diagnose_track,
            unpack_paint,
            texture_bytes,
            watch_paint_files,
            unpack_pkz,
            load_bike_model,
            preview_model_swap,
            load_rider_model,
            load_rider_body_model,
            load_gear_model,
            load_stock_gear_model,
            list_gear_paints,
            list_installed_gear_paints,
            paint_studio_load,
            paint_studio_pixels,
            paint_studio_stage,
            photo_save,
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
            set_launch_at_startup,
            set_auto_run_frostmod,
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
            set_voice_enabled,
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
            preset_bundle_stats,
            preset_bundle_create,
            preset_bundle_import,
            file_share_plan,
            file_share_create,
            file_share_preview,
            file_share_import,
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
mod webview_env_tests {
    use super::*;
    use std::collections::HashMap;

    fn defaults(env: GraphicsEnv) -> HashMap<&'static str, &'static str> {
        webview_env_defaults(env).into_iter().collect()
    }

    /// SteamOS, both modes: Desktop is Plasma Wayland and gamescope is a compositor of its
    /// own, and each runs an XWayland the app can land on instead.
    const STEAMOS: GraphicsEnv = GraphicsEnv {
        wayland: true,
        x_server: true,
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
mod autostart_tests {
    use super::*;

    #[test]
    fn the_setting_is_honoured_when_the_binding_is_current() {
        assert_eq!(autostart_action(true, false, false), Autostart::Enable);
        assert_eq!(autostart_action(true, true, false), Autostart::Leave);
        assert_eq!(autostart_action(false, true, false), Autostart::Disable);
        assert_eq!(autostart_action(false, false, false), Autostart::Leave);
    }

    /// The rename bug: the entry exists, so nothing looks wrong, but it names a binary that
    /// is gone. Without this the app quietly stops starting at login for everyone upgrading.
    #[test]
    fn an_entry_written_for_the_old_binary_is_rewritten() {
        assert_eq!(autostart_action(true, true, true), Autostart::Rebind);
    }

    /// Whoever turned it off gets it off, however old their entry is — a stale binding is a
    /// reason to rewrite the entry, never to bring one back.
    #[test]
    fn a_stale_binding_never_revives_a_disabled_login_item() {
        assert_eq!(autostart_action(false, true, true), Autostart::Disable);
        assert_eq!(autostart_action(false, false, true), Autostart::Leave);
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

    /// How the viewer sees a bike, as a comparable shape: which parts resolved and what
    /// each submesh is bound to. Node order is fixed by `GFX_PARTS`, so this is stable.
    fn shape(m: &super::BikeModel) -> Vec<(String, Vec<(String, Option<String>)>)> {
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

    /// Write a plain-zip `.pkz` — `pkz::read_selected` reads those natively, so the packed
    /// half of a bike can be fixtured without the sidecar.
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

    /// The fault behind a swap that renders white with every part stacked at the origin: a
    /// model set is a mesh and little else, so the bike's `.geom`, `gfx.cfg`, `.hrc`s and
    /// stock paint have nowhere to come from but the archive — which the preview used to
    /// skip the moment the variant brought a mesh.
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
        let variant = bikes.join(bike).join(super::modelswap::LIB_DIR).join("Factory");
        std::fs::create_dir_all(&variant).unwrap();
        std::fs::write(variant.join("model.edf"), b"swap mesh").unwrap();

        let set = super::modelswap::preview_set(mp, bike, "Factory").expect("preview set");
        let files = super::gather_preview_files(&set).expect("preview files");

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

    /// The same law on the ordinary load path: an extracted bike whose folder holds only a
    /// mesh still draws with the setup and paint left behind in its archive.
    #[test]
    fn a_loose_bike_still_reads_its_packed_setup() {
        let root: PathBuf =
            std::env::temp_dir().join(format!("frost-loose-packed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bikes = root.join("bikes");
        std::fs::create_dir_all(bikes.join("KTM450")).unwrap();
        write_pkz(
            &bikes.join("KTM450.pkz"),
            &[
                ("KTM450/model.edf", b"packed mesh"),
                ("KTM450/gfx.cfg", b"packed gfx"),
                ("KTM450/wheel.geom", b"packed geom"),
            ],
        );
        std::fs::write(bikes.join("KTM450").join("model.edf"), b"loose mesh").unwrap();

        let files = super::gather_bike_files(&bikes.join("KTM450")).expect("gather");
        assert_eq!(named(&files, "model.edf"), Some(&b"loose mesh"[..]), "loose wins");
        assert!(named(&files, "gfx.cfg").is_some(), "packed gfx.cfg comes through");
        assert!(named(&files, "wheel.geom").is_some(), "packed .geom comes through");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A mod mesh that ships companion maps used to render entirely grey: its material
    /// tables were thrown out because `w13` was assumed to be padding, and even read back they
    /// index a list `declared_colors` doesn't build — one that counts the sheets the mesh
    /// declares but never embeds. `polarm` is the proof: nothing embeds it, and it is the only
    /// candidate for the `Polar + Mount` submesh.
    ///
    /// Pinned against parts whose names name their own sheet, so a wrong index space can't
    /// pass. Needs the real tree — no synthetic `.edf` exercises this.
    ///
    /// MXB_REAL_BIKES=~/Documents/PiBoSo/"MX Bikes" \
    ///   cargo test a_companion_shipping_mesh_binds_every_part -- --ignored --nocapture
    #[test]
    #[ignore]
    fn a_companion_shipping_mesh_binds_every_part() {
        let Ok(src_root) = std::env::var("MXB_REAL_BIKES") else {
            eprintln!("set MXB_REAL_BIKES to run");
            return;
        };
        let dir = Path::new(&src_root)
            .join("mods")
            .join("bikes")
            .join("MX1OEM_2023_KTM_450_SX-F");
        if !crate::bikefiles::dir_has_mesh(&dir) {
            eprintln!("no extracted mesh at {dir:?} — skipping");
            return;
        }
        let m = super::load_bike_model_blocking(dir.to_string_lossy().to_string(), None)
            .expect("the bike loads");
        let bound: Vec<(String, String)> = m
            .nodes
            .iter()
            .flat_map(|n| n.submeshes.iter().map(|s| {
                (s.name.clone(), s.texture.clone().unwrap_or_default())
            }))
            .collect();
        for (part, sheet) in [
            ("LUXON LMM.001", "luxlmm"),
            ("pedale_low", "HHpedal"),
            ("pedale_low.002", "HHshifter"),
            ("tank_low", "rmxtank"),
            ("L master cyl.002", "asv"),
            ("ODI Grips+bar end", "ODIGRIPBAREND"),
            ("Polar + Mount", "polarm"),
            ("levers", "arclever"),
        ] {
            let got = bound.iter().find(|(n, _)| n == part).map(|(_, t)| t.as_str());
            assert_eq!(got, Some(sheet), "{part} should wear {sheet}");
        }
        assert!(
            bound.iter().all(|(_, t)| !t.is_empty()),
            "every submesh should be bound: {:?}",
            bound.iter().filter(|(_, t)| t.is_empty()).collect::<Vec<_>>(),
        );
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

        let set = super::modelswap::preview_set(mp, &bike, "Factory").expect("preview set");
        eprintln!("keeps {:?} + brings {:?}", set.root_keep, set.variant_files);
        let files = super::gather_preview_files(&set).expect("preview files");
        let previewed = super::build_bike_model(
            "preview",
            "preview-test".into(),
            files,
            super::installed_paints(&set.bike_dir),
            // What `load_bike_model_blocking` derives for the bike it's compared against.
            Some(crate::library::mods_subdir(mp, "mods/tyres")),
            None,
            std::time::Instant::now(),
        )
        .expect("preview builds");
        assert!(!previewed.nodes.is_empty(), "the preview drew nothing");

        super::modelswap::apply_model_swap(mp, &bike, "Factory").expect("swap applies");
        let applied = super::load_bike_model_blocking(dst.to_string_lossy().to_string(), None)
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

        let set = super::modelswap::preview_set(mp, &bike, "Stock").expect("preview set");
        eprintln!("keeps {:?}", set.root_keep);
        assert!(
            !set.root_keep.iter().any(|f| crate::bikefiles::is_mesh(f)),
            "Stock must park every loose mesh",
        );
        let files = super::gather_preview_files(&set).expect("preview files");
        let m = super::build_bike_model(
            "stock preview",
            "stock-preview-test".into(),
            files,
            super::installed_paints(&set.bike_dir),
            Some(crate::library::mods_subdir(mp, "mods/tyres")),
            None,
            std::time::Instant::now(),
        )
        .expect("stock preview builds");
        assert!(!m.nodes.is_empty(), "the packed model didn't come through");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn tex(name: &str, token: &str) -> crate::paint::PaintTexture {
        crate::paint::PaintTexture {
            name: name.into(),
            width: 4,
            height: 4,
            token: token.into(),
        }
    }

    /// Evicting a bike has to free every blob it put in the store.
    ///
    /// The one that gets away is a model texture every paint overrides: it is folded into
    /// none of them, so walking the paints alone never names it and its pixels outlive the
    /// bike by the whole session. Cheap to leak, since it's the biggest sheets — `plastics`
    /// on a bike whose paints all replace it — that leak.
    #[test]
    fn eviction_frees_the_models_own_textures_too() {
        let model = super::BikeModel {
            nodes: Vec::new(),
            paints: vec![super::BikePaint {
                name: "Red".into(),
                path: None,
                // Its own `plastics`, so the model's never reaches it.
                textures: vec![tex("plastics", "t-paint"), tex("wheel", "t-shared")],
                changes_preview: true,
            }],
            base: vec![tex("plastics", "t-own"), tex("wheel", "t-shared")],
            tyres: None,
            assembled: true,
            rig: None,
        };
        let tokens = model.tokens();
        assert!(tokens.contains(&"t-own".to_string()), "the overridden one is still released");
        assert!(tokens.contains(&"t-paint".to_string()));
        // Named twice over — the paints borrowed it — and that's fine: `release` removes by
        // key, so the second pass over a token is a no-op rather than a double free.
        assert_eq!(tokens.iter().filter(|t| *t == "t-shared").count(), 2);
    }

    /// A paint installed loose beside a bike has to come back with the file it lives in —
    /// that path is the only thing the viewer can watch, and without it a re-saved livery
    /// goes unnoticed until the dialog is closed and re-opened.
    #[test]
    fn an_installed_paint_names_the_file_it_came_from() {
        let root = std::env::temp_dir().join(format!("frost-installed-paints-{}", std::process::id()));
        let paints = root.join("KTM 450").join("paints");
        std::fs::create_dir_all(&paints).expect("make the bike's paints folder");
        std::fs::write(paints.join("Frost.pnt"), b"not really a paint").expect("write a paint");
        // Whatever else is parked in there is not a paint and must not be offered as one.
        std::fs::write(paints.join("notes.txt"), b"x").expect("write the noise");

        let found = super::installed_paints(&root.join("KTM 450"));
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(found.len(), 1, "only the .pnt counts");
        let (name, path, bytes) = &found[0];
        assert_eq!(name, "Frost.pnt");
        assert_eq!(std::path::Path::new(path), paints.join("Frost.pnt"));
        assert_eq!(bytes, b"not really a paint");
    }

    /// A paint re-saved inside one timestamp tick still misses the cache.
    ///
    /// The viewer's live reload re-decodes a file the moment it changes, which can be twice
    /// in the same second — and a filesystem with coarse timestamps (FAT32 rounds to two)
    /// would hand back the previous decode, i.e. the painter's *last* attempt. The mtime is
    /// pinned here so the size is the only thing left to tell the two apart.
    #[test]
    fn a_paint_resaved_within_a_timestamp_tick_is_not_served_stale() {
        let dir = std::env::temp_dir().join(format!("frost-cache-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("make the folder");
        let paint = dir.join("Frost.pnt");
        let pinned = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let stamp = |bytes: &[u8]| {
            std::fs::write(&paint, bytes).expect("save the paint");
            let f = std::fs::File::options().write(true).open(&paint).expect("reopen");
            f.set_times(std::fs::FileTimes::new().set_modified(pinned))
                .expect("pin the mtime");
            super::bike_cache_key(&paint.to_string_lossy())
        };

        let first = stamp(b"the first attempt");
        let second = stamp(b"the second attempt, recompressed");
        let identical = stamp(b"the second attempt, recompressed");
        let _ = std::fs::remove_dir_all(&dir);

        assert_ne!(first, second, "a re-saved paint must miss its cached decode");
        assert_eq!(identical, second, "and an unchanged one must still hit it");
    }

    /// The same, reached through the `.pkz` beside the folder — how a packaged bike is
    /// installed, and the path a painter's own liveries actually sit next to.
    #[test]
    fn paints_are_found_beside_a_packaged_bike_too() {
        let root = std::env::temp_dir().join(format!("frost-packaged-paints-{}", std::process::id()));
        let paints = root.join("KTM 450").join("paints");
        std::fs::create_dir_all(&paints).expect("make the bike's paints folder");
        std::fs::write(paints.join("Frost.pnt"), b"paint").expect("write a paint");

        // The source the viewer is given is the archive, not the folder next to it.
        let found = super::installed_paints(&root.join("KTM 450.pkz"));
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(found.len(), 1);
        assert_eq!(std::path::Path::new(&found[0].1), paints.join("Frost.pnt"));
    }

    fn sub(name: &str, texture: Option<&str>) -> crate::edf::Submesh {
        crate::edf::Submesh {
            name: name.into(),
            tri_start: 0,
            tri_count: 1,
            texture: texture.map(str::to_string),
            uv_tile: None,
            mat: None,
        }
    }

    fn node(name: &str, subs: Vec<crate::edf::Submesh>) -> crate::edf::EdfNode {
        crate::edf::EdfNode {
            name: name.into(),
            positions: vec![0.0; 3],
            uvs: Vec::new(),
            normals: Vec::new(),
            indices: vec![0, 0, 0],
            submeshes: subs,
            texture: None,
            placed: true,
            materials: Vec::new(),
        }
    }

    /// The rear wheel arrives with the chain in it, and the chain is a template strip the
    /// game bends — a metre-long bar if it's drawn where it sits. Everything else on the
    /// wheel has to survive.
    #[test]
    fn the_chain_comes_off_the_rear_wheel() {
        let mut nodes = vec![
            node("fwheel", vec![sub("thefwheel", Some("wheel")), sub("thefwheel", Some("fgeomax"))]),
            node(
                "rwheela",
                vec![
                    sub("thechain", Some("chain")),
                    sub("therwheela", Some("wheel")),
                    sub("therwheela", Some("sprocket")),
                    sub("therwheela", Some("rgeomax")),
                ],
            ),
        ];
        super::drop_chain(&mut nodes);
        assert_eq!(nodes.len(), 2, "both wheels stay");
        assert_eq!(nodes[0].submeshes.len(), 2, "the front wheel is untouched");
        let rear: Vec<&str> =
            nodes[1].submeshes.iter().filter_map(|s| s.texture.as_deref()).collect();
        assert_eq!(rear, ["wheel", "sprocket", "rgeomax"], "rim, sprocket and tyre stay");
    }

    /// The bug the first cut of this had: the submesh went, its vertices stayed, and the
    /// chain's 0.7 m of template still counted towards the bounds everything else is
    /// measured against — where the viewer centres the bike, where `SideBySide` stands it.
    #[test]
    fn the_chains_vertices_go_with_it() {
        let mut n = node(
            "rwheela",
            vec![sub("therwheela", Some("wheel")), sub("thechain", Some("chain"))],
        );
        // Two triangles: the wheel's on the axle, the chain's a long way above it.
        n.positions = vec![0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.7, 0.0];
        n.uvs = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.5, 0.5];
        n.indices = vec![0, 1, 2, 1, 2, 3];
        n.submeshes[0].tri_start = 0;
        n.submeshes[1].tri_start = 1;
        let mut nodes = vec![n];

        super::drop_chain(&mut nodes);
        let n = &nodes[0];
        assert_eq!(n.submeshes.len(), 1);
        assert_eq!(n.submeshes[0].texture.as_deref(), Some("wheel"));
        assert_eq!(n.submeshes[0].tri_start, 0, "the survivor is renumbered from zero");
        assert_eq!(n.submeshes[0].tri_count, 1);
        assert_eq!(n.indices, vec![0, 1, 2], "vertices remapped onto what's left");
        assert_eq!(n.positions.len(), 9, "the chain's lone vertex is gone");
        assert_eq!(n.uvs.len(), 6, "uvs are compacted alongside");
        let top = n.positions.chunks_exact(3).map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!(top < 0.2, "nothing left standing 0.7 m up: {top}");
    }

    /// A node with nothing but chain in it has to go entirely: left with no groups, the
    /// frontend draws the whole node on one texture rather than nothing at all.
    #[test]
    fn a_node_that_is_only_chain_is_dropped() {
        let mut nodes = vec![
            node("chain", vec![sub("thechain", Some("CHAIN"))]),
            // No submesh table at all — a whole-node binding, and not ours to judge.
            node("rwheela", Vec::new()),
        ];
        super::drop_chain(&mut nodes);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "rwheela");
    }

    /// The bike source is `<mods>/bikes/<Bike>`; the tyres sit beside `bikes`, not inside it.
    #[test]
    fn tyres_sit_beside_the_bikes_folder() {
        let dir = super::tyres_dir_for(Path::new("/games/mods/bikes/MX1OEM_2023_Honda_CRF450R"));
        assert_eq!(dir.as_deref(), Some(Path::new("/games/mods/tyres")));
        let packed = super::tyres_dir_for(Path::new("/games/mods/bikes/Some_Bike.pkz"));
        assert_eq!(packed.as_deref(), Some(Path::new("/games/mods/tyres")));
    }

    fn tyres_tmp(tag: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("frost-tyres-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Lay down a minimal but real tyres mod under `<root>/<name>`.
    fn write_tyres_mod(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, body) in [
            ("gfx.cfg", "front_wheel\n{\n\tmodel\n\t{\n\t\tfile = fwheel.hrc\n\t}\n}\n"),
            ("fwheel.hrc", "level0\n{\n\tscene = model.edf\n}\n"),
            ("model.edf", "EDF\0"),
            // Beside them and never drawn — must not be read.
            ("OEM_MXf_is80100-21.tyre", "params"),
            ("preview.tga", "pixels"),
        ] {
            std::fs::write(dir.join(file), body).unwrap();
        }
    }

    fn tyre_file_names(set: &super::TyreSet) -> Vec<&str> {
        let mut names: Vec<&str> = set.files.iter().map(|(n, _)| n.as_str()).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn tyre_files_come_from_the_mod_the_bike_names() {
        let root = tyres_tmp("loose");
        write_tyres_mod(&root, "oem_mx");

        let set = super::gather_tyre_files(&root, b"tyres = oem_mx\n", None).expect("found");
        assert_eq!(set.name, "oem_mx");
        assert_eq!(tyre_file_names(&set), ["fwheel.hrc", "gfx.cfg", "model.edf"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole point of the picker: a bike names one pack, and picking another has to
    /// beat it — without anything on disk being renamed.
    #[test]
    fn a_picked_pack_beats_the_one_the_bike_names() {
        let root = tyres_tmp("pick");
        write_tyres_mod(&root, "oem_mx");
        write_tyres_mod(&root, "p_mx");

        let own = super::gather_tyre_files(&root, b"tyres = oem_mx\n", None).unwrap();
        assert_eq!(own.name, "oem_mx");
        let picked = super::gather_tyre_files(&root, b"tyres = oem_mx\n", Some("p_mx")).unwrap();
        assert_eq!(picked.name, "p_mx", "the pick wins");
        // Blank is "no pick", not "a pack called nothing".
        let blank = super::gather_tyre_files(&root, b"tyres = oem_mx\n", Some("  ")).unwrap();
        assert_eq!(blank.name, "oem_mx");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A pick that names nothing installed must not cost the bike its wheels — it falls back
    /// to the pack the bike itself names.
    #[test]
    fn a_pick_that_isnt_installed_falls_back_to_the_bikes_own() {
        let root = tyres_tmp("fallback");
        write_tyres_mod(&root, "oem_mx");

        for pick in ["uninstalled_pack", "../bikes", "sub/dir"] {
            let set = super::gather_tyre_files(&root, b"tyres = oem_mx\n", Some(pick))
                .unwrap_or_else(|| panic!("still wheels for pick {pick:?}"));
            assert_eq!(set.name, "oem_mx", "pick {pick:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Every way a bike can end up with no wheels. None of them is an error: that is the
    /// bike the viewer drew before wheels existed.
    #[test]
    fn no_tyres_mod_means_no_wheels_and_no_fuss() {
        let root = tyres_tmp("empty");
        let none = |gfx: &[u8], pick: Option<&str>| {
            super::gather_tyre_files(&root, gfx, pick).is_none()
        };
        assert!(none(b"tyres = oem_mx\n", None), "not installed");
        assert!(none(b"chassis\n{\n}\n", None), "no tyres line");
        // The name is read out of a mod's own file, so it never gets to walk out of `tyres/`.
        assert!(none(b"tyres = ../bikes\n", None), "traversal");
        // A pick can't rescue a bike that names nothing installed either.
        assert!(none(b"chassis\n{\n}\n", Some("p_mx")), "pick, but nothing installed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Every base texture a bike decodes, with its source size and what it cost.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> cargo test bike_texture_costs -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_texture_costs() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");

        println!("\n  source            decode    src px      -> stored");
        let mut total = std::time::Duration::ZERO;
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if let Some(stem) = bn.strip_suffix(".tga") {
                let t = std::time::Instant::now();
                let tex = super::paint::decode_image(stem, data);
                let d = t.elapsed();
                total += d;
                if let Some(tex) = tex {
                    println!("  {stem:<16}{d:>9.2?}  {:>6}KB  -> {}x{}",
                             data.len() / 1024, tex.width, tex.height);
                }
            } else if bn.ends_with(".edf") {
                let t = std::time::Instant::now();
                let texs = super::paint::extract_edf_textures(data);
                let d = t.elapsed();
                total += d;
                println!("  {bn:<16}{d:>9.2?}  {:>6}KB  -> {} embedded texture(s)",
                         data.len() / 1024, texs.len());
                for tex in &texs {
                    println!("      {:<12}            -> {}x{}", tex.name, tex.width, tex.height);
                }
            }
        }
        println!("\n  base textures total {total:.2?}");

        // The other half of the parse phase: the geometry in the same files.
        let mut mesh = std::time::Duration::ZERO;
        for (name, data) in &files {
            let bn = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
            if !bn.ends_with(".edf") {
                continue;
            }
            let t = std::time::Instant::now();
            let nodes = super::edf::parse(data);
            let d = t.elapsed();
            mesh += d;
            println!("  parse {bn:<20}{d:>9.2?}  -> {} node(s)", nodes.len());
        }
        println!("  mesh parse total    {mesh:.2?}\n");
    }

    /// Where a bike view's time goes, uncached.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> cargo test bike_load_timing -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_load_timing() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("\n  {path}  ({:.1} MB)", size as f64 / 1e6);

        // The archive read, split out from everything downstream of it.
        let t = std::time::Instant::now();
        let files = super::gather_bike_files(std::path::Path::new(&path)).expect("gather");
        let read = t.elapsed();
        let bytes: usize = files.iter().map(|(_, d)| d.len()).sum();
        drop(files);

        // Cold: the cache is what a second open gets, and it is not what anyone complains about.
        let t = std::time::Instant::now();
        let m = super::load_bike_model_blocking(path.clone(), None).expect("load bike");
        let cold = t.elapsed();
        println!("  read archive         {read:>9.2?}  ({:.1} MB inflated)", bytes as f64 / 1e6);

        let t = std::time::Instant::now();
        let _ = super::load_bike_model_blocking(path, None).expect("load bike");
        let warm = t.elapsed();

        let t = std::time::Instant::now();
        let json = serde_json::to_string(&m.nodes).unwrap();
        let encode = t.elapsed();

        let sheets: usize = m.paints.iter().map(|p| p.textures.len()).sum();
        println!("  load, cold           {cold:>9.2?}");
        println!("  load, cached         {warm:>9.2?}");
        println!("  mesh -> JSON         {encode:>9.2?}  ({:.1} MB of text to the webview)",
                 json.len() as f64 / 1e6);
        println!("  {} paint(s), {sheets} sheet(s) — pixels stay in the texture store\n",
                 m.paints.len());
    }

    /// What the mesh costs to hand the webview.
    ///
    /// `MXB_REAL_PKZ=<bike.pkz> cargo test bike_mesh_payload -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_mesh_payload() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let m = super::load_bike_model_blocking(path, None).expect("load bike");

        let verts: usize = m.nodes.iter().map(|n| n.positions.len() / 3).sum();
        let tris: usize = m.nodes.iter().map(|n| n.indices.len() / 3).sum();
        let floats: usize = m
            .nodes
            .iter()
            .map(|n| n.positions.len() + n.uvs.len() + n.normals.len())
            .sum();
        let ints: usize = m.nodes.iter().map(|n| n.indices.len()).sum();

        let t = std::time::Instant::now();
        let json = serde_json::to_string(&m.nodes).unwrap();
        let encode = t.elapsed();

        // What the same numbers weigh as raw little-endian, which is what a binary channel
        // would carry and what the webview can adopt without parsing.
        let binary = floats * 4 + ints * 4;

        println!("\n  {} nodes, {verts} vertices, {tris} triangles", m.nodes.len());
        println!("  JSON   {:>9.1} MB  encoded in {encode:.2?}", json.len() as f64 / 1e6);
        println!("  binary {:>9.1} MB", binary as f64 / 1e6);
        println!("  ratio  {:>9.1}x\n", json.len() as f64 / binary as f64);
    }

    #[test]
    #[ignore = "needs a real bike — set MXB_REAL_PKZ"]
    fn bike_model_from_pkz() {
        let Ok(path) = std::env::var("MXB_REAL_PKZ") else {
            eprintln!("set MXB_REAL_PKZ to run");
            return;
        };
        let m = super::load_bike_model_blocking(path, std::env::var("MXB_TYRES").ok()).expect("load bike");
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
        let mut own: Vec<&str> = m.base.iter().map(|t| t.name.as_str()).collect();
        own.sort_unstable();
        eprintln!("the model's own textures: {}", own.join(", "));
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
        // The Designer's stock underlay reads exactly this list, and it is wanted most where no
        // `.pnt` can answer: an OEM bike's stock paint replaces the wheels and the chain, so its
        // `plastics` is only ever embedded in the mesh. Not asserted by that name — a mod bike
        // calls its sheets whatever it likes, and a bike whose paints supply everything has a
        // shorter list than this one — only that the list survived being folded into the paints
        // above, which is the way it would be lost.
        assert!(!m.base.is_empty(), "the model's own textures outlive the fold into the paints");
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
            // A one-piece item has no submesh table at all and wears its texture on the node
            // — `bind_gear_submeshes` says so explicitly. Counting only submeshes read that
            // as "nothing was bound" on exactly the meshes worth checking.
            let pieces: Vec<(&str, Option<&String>)> = if n.submeshes.is_empty() {
                vec![(n.name.as_str(), n.texture.as_ref())]
            } else {
                n.submeshes.iter().map(|s| (s.name.as_str(), s.texture.as_ref())).collect()
            };
            for (name, tex) in pieces {
                let t = tex.expect("stock piece bound to a texture");
                eprintln!("stock piece {name:<10} -> {t}");
                assert!(embedded.contains(&t.to_ascii_lowercase()), "'{t}' is embedded in the mesh");
                bound += 1;
            }
        }
        assert!(bound > 0, "stock bound at least one piece");

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

    /// Investigation aid: a rider's rig as JSON, in the frame the viewer draws it in.
    ///
    /// The two turns `body_rig` puts a rig through are what make the difference between this
    /// and `edf::tests::rig_dump`, and they are what the front end's ready-made moves are
    /// stated against — so this is the shape to check those against.
    ///
    /// `MXB_EDF_FILE=…/rider.edf cargo test rig_json -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rig_json() {
        let Ok(path) = std::env::var("MXB_EDF_FILE") else {
            eprintln!("set MXB_EDF_FILE to run");
            return;
        };
        let bytes = std::fs::read(&path).expect("read edf");
        let mut rig = super::edf::parse_skeleton(&bytes);
        super::edf::transform_skeleton(&mut rig, [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for b in rig.iter() {
            let o = b.origin();
            for a in 0..3 {
                lo[a] = lo[a].min(o[a]);
                hi[a] = hi[a].max(o[a]);
            }
        }
        if super::body_is_z_up([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]) {
            super::edf::transform_skeleton(&mut rig, super::BODY_STAND_UP);
        }
        println!("{}", serde_json::to_string(&rig).expect("serialise"));
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
        let model = super::load_bike_model_blocking(path.clone(), None).expect("load bike");
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

    /// The whole round trip for a package a shop install buried: unloadable where it sits,
    /// loadable once the repair has raised it. Run against a real locked helmet, because the
    /// point is that the file coming up is the same file the viewer then decodes.
    ///
    /// `MXB_REAL_GEAR` must name a `.pkz`. Linked rather than copied, so the player's own
    /// 42 MB helmet is neither duplicated nor moved — the repair renames the link.
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
        let buried_load = super::load_gear_model_blocking(
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

        let plans = crate::gearrepair::plan(root.to_str().unwrap());
        assert_eq!(plans.len(), 1, "the burial is found");
        let moved = crate::gearrepair::apply_one(root.to_str().unwrap(), &plans[0].id).unwrap();
        assert_eq!(moved, 1);

        let raised = helmets.join(&name);
        assert!(raised.is_file(), "the package is in the area root");
        let part = super::load_gear_model_blocking(
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
