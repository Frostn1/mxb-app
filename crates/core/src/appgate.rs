//! The startup gate, shared by every app in the lineup.
//!
//! MXB App, Frost's Studio and MXB Coach are one product and one company, so a rider who is
//! banned — or, when the deployment requires it, not signed in with Steam — is refused all of
//! them, not just the one that happens to hold secured content. This is the client side of
//! `GET /v1/app/gate`: each app calls [`enforce_marker`] and spawns [`check`] first thing in its
//! `setup`, and the server's one verdict decides what happens.
//!
//! Three verdicts:
//!  - `ok` — run;
//!  - `signin` — a Steam sign-in is required first. Honest: the app raises a wall (the
//!    `mxb-signin-required` event) and unlocks the moment Valve confirms the account. Never
//!    fatal — the person can complete it.
//!  - `unsupported` — a banned install. The app shows a mundane untruth ("this copy couldn't be
//!    verified") and closes. Disguised on purpose, and it is written to a marker so a blocked
//!    install stays blocked even offline. See the control plane's `bans.ts` for why the app is
//!    lied to while the website is not.
//!
//! Living in `mxb-core` is the point: the gate, the account it needs (`account::ensure_token`)
//! and the Steam-link round trip all sit here once, so a new app in the lineup is secured by
//! calling three functions rather than by copying the machinery and letting it drift.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::account;
use crate::names::control_plane;

/// Shown when the server sent an empty message, or a stale marker holds none. The server's own
/// text wins when present; this only has to read as a mundane failure, never as a ban.
const FALLBACK_BLOCK: &str =
    "This copy couldn't be verified. It may be out of date or damaged — reinstall the latest version from mxbsecure.com.";

/// Shown behind the sign-in wall when the server sent no message of its own.
const FALLBACK_SIGNIN: &str = "Sign in with Steam to continue.";

/// Every request here is one the sign-in wall is waiting on, and `reqwest` has no timeout of its
/// own: a connection that opens and then says nothing hangs for as long as the OS allows. On the
/// wall that is not a slow request, it is a button that never comes back — the command never
/// resolves, so the frontend never leaves the state it entered to make the call. `account.rs`
/// already builds its client this way; this is the rest of the flow catching up.
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// A client that always gives up eventually. Falls back to the default client if the builder
/// fails, which keeps a timeout from being the thing that stops the gate working at all.
fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(HTTP_TIMEOUT).build().unwrap_or_default()
}

/// The verdict `GET /v1/app/gate` returns.
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Verdict {
    /// This install may run.
    ///
    /// `steam` is whether Valve has confirmed the account — a different question from whether it
    /// may run, because with the requirement off an unlinked install is also `Ok`. Defaulted, so
    /// a control plane older than the field reads as "not confirmed" rather than failing to
    /// deserialize and taking the whole gate down with it.
    Ok {
        #[serde(default)]
        steam: bool,
    },
    /// A Steam sign-in is required before it may. Honest, and never fatal.
    Signin {
        #[serde(default)]
        message: String,
    },
    /// It may not. `message` is a mundane untruth, never the word "ban".
    Unsupported {
        #[serde(default)]
        message: String,
    },
}

/// What the frontend is told about the sign-in wall — put up, or taken down.
#[derive(Clone, serde::Serialize)]
struct SigninRequired {
    required: bool,
    message: String,
}

/// What the gate said about this install's Steam sign-in, for the usage counters to report.
///
/// Three states, and `UNKNOWN` is the one that matters: the gate answers once at startup and can
/// fail to (offline, no token yet), while the counters flush every half hour regardless. Reading
/// "we have not been told" as "not signed in" would put every unreachable start in the `no`
/// bucket and make the adoption figure a graph of the network. See `0041_usage_steam.sql`.
const STEAM_UNKNOWN: u8 = 0;
const STEAM_NO: u8 = 1;
const STEAM_YES: u8 = 2;
static STEAM: AtomicU8 = AtomicU8::new(STEAM_UNKNOWN);

/// What this install reports about its sign-in: `"yes"`, `"no"` or `"unknown"`.
///
/// Named the way the wire names it so there is nothing to translate at the call site — the one
/// caller is `usage::take`, and a second spelling of these three words is a bug waiting to
/// happen.
pub fn steam_state() -> &'static str {
    match STEAM.load(Ordering::Relaxed) {
        STEAM_YES => "yes",
        STEAM_NO => "no",
        _ => "unknown",
    }
}

/// The last verdict this run reached, kept so a webview can ask for it.
///
/// [`check`] is spawned from each app's `setup`, which runs *before* the webview exists, and a
/// Tauri event goes only to the listeners attached at the instant it is emitted — there is no
/// buffer and no replay. The gate's round trip is a couple of hundred milliseconds; mounting the
/// frontend on a cold start is frequently slower. So the verdict was routinely emitted into an
/// empty room and the sign-in wall never appeared at all — on an install that had just been told
/// it must sign in with Steam. Remembering it costs nothing and makes the handshake one the
/// webview can complete from its side.
fn last_verdict() -> &'static Mutex<Option<SigninRequired>> {
    static LAST: OnceLock<Mutex<Option<SigninRequired>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

/// Emit a verdict and remember it. Every announcement goes through here, so what is remembered
/// cannot drift from what was sent.
fn announce(app: &AppHandle, verdict: SigninRequired) {
    if let Ok(mut slot) = last_verdict().lock() {
        *slot = Some(verdict.clone());
    }
    let _ = app.emit("mxb-signin-required", verdict);
}

/// Re-send the verdict this run already reached, for a webview that mounted too late to hear it.
///
/// Does nothing when there is none yet, which is the point: either [`check`] is still in flight —
/// and will emit to the listener that now exists — or it ended without a verdict (offline, no
/// token), which already means "don't know" and never a wall. Deliberately *not* a second
/// [`check`]: two running at once on an install with no token yet would both claim a device
/// account, and the one that lost the race to the config file would have minted an orphan.
pub async fn replay_verdict(app: AppHandle) {
    let known = last_verdict().lock().ok().and_then(|slot| slot.clone());
    if let Some(verdict) = known {
        let _ = app.emit("mxb-signin-required", verdict);
    }
}

/// Where the "stay blocked, even offline" marker lives. `None` only if there is no data dir to
/// write into, the same situation the rest of the app's local state cannot survive.
fn marker_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("gate.lock"))
}

/// The message a marker holds, or `None` when this install is not marked blocked. An empty or
/// unreadable marker still counts as blocked — its presence is the signal, the text a nicety.
pub fn blocked(app: &AppHandle) -> Option<String> {
    let path = marker_path(app)?;
    if !path.exists() {
        return None;
    }
    let msg = std::fs::read_to_string(&path).unwrap_or_default();
    Some(if msg.trim().is_empty() { FALLBACK_BLOCK.to_string() } else { msg })
}

fn mark(app: &AppHandle, message: &str) {
    if let Some(path) = marker_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, if message.trim().is_empty() { FALLBACK_BLOCK } else { message });
    }
}

fn unmark(app: &AppHandle) {
    if let Some(path) = marker_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// The name to put on the dialog — this app's own product name, so Studio doesn't say "MXB App".
fn app_name(app: &AppHandle) -> String {
    let n = app.package_info().name.clone();
    if n.trim().is_empty() { "MXB App".to_string() } else { n }
}

/// Show the block message and end the process. Must run on the main thread — dialogs do on
/// macOS — which is true of the `enforce_marker` call, and arranged with `run_on_main_thread`
/// for the async one.
pub fn deny(app: &AppHandle, message: &str) -> ! {
    log::warn!("[gate] this installation is blocked; refusing to start");
    app.dialog()
        .message(message)
        .kind(MessageDialogKind::Error)
        .title(app_name(app))
        .blocking_show();
    app.exit(1);
    std::process::exit(1);
}

/// Refuse instantly if a previous run was told to. Called at the very top of `setup`, before the
/// window is built, so a blocked install never flashes a usable window and never needs the
/// network to enforce a block it has already been given. A no-op for everyone else.
pub fn enforce_marker(app: &AppHandle) {
    if let Some(message) = blocked(app) {
        deny(app, &message);
    }
}

/// Ask the server whether this install may run, and act on the answer.
///
/// Spawned in the background from `setup` so it never delays a legitimate launch. `ok` clears any
/// stale marker and lowers the sign-in wall; `signin` raises the wall (never fatal); `unsupported`
/// marks and tears the app down. Any network or auth error does nothing — a marker already
/// written stands, and an install that was never blocked keeps running.
pub async fn check(app: AppHandle) {
    let token = match account::ensure_token(&app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("[gate] no verdict this run ({e})");
            return;
        }
    };

    let resp = match http()
        .get(format!("{}/v1/app/gate", control_plane()))
        .bearer_auth(&token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::info!("[gate] no verdict this run ({e})");
            return;
        }
    };
    if !resp.status().is_success() {
        log::info!("[gate] service answered {}", resp.status());
        return;
    }
    let verdict: Verdict = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            log::info!("[gate] unreadable verdict ({e})");
            return;
        }
    };

    match verdict {
        Verdict::Ok { steam } => {
            STEAM.store(if steam { STEAM_YES } else { STEAM_NO }, Ordering::Relaxed);
            unmark(&app);
            announce(&app, SigninRequired { required: false, message: String::new() });
        }
        Verdict::Signin { message } => {
            // A sign-in wall is only raised for an account Valve has not confirmed, so this
            // verdict is itself the answer — and it is the one state where "not signed in" is
            // known rather than merely untold.
            STEAM.store(STEAM_NO, Ordering::Relaxed);
            let message = if message.trim().is_empty() { FALLBACK_SIGNIN.to_string() } else { message };
            log::info!("[gate] a Steam sign-in is required before this install may run");
            announce(&app, SigninRequired { required: true, message });
        }
        Verdict::Unsupported { message } => {
            let message = if message.trim().is_empty() { FALLBACK_BLOCK.to_string() } else { message };
            mark(&app, &message);
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || deny(&handle, &message));
        }
    }
}

// ---------------------------------------------------------------------------------------
// The Steam-link round trip the sign-in wall drives — shared so every app's wall is the same.
// ---------------------------------------------------------------------------------------

/// Ask the control plane for a Steam OpenID sign-in URL. The frontend opens it; the browser half
/// lands on `/v1/steam/return`, which sets `steam_id` and pins the derived GUID.
pub async fn steam_link_start(app: &AppHandle) -> Result<String, String> {
    let token = account::ensure_token(app).await?;
    let resp = http()
        .post(format!("{}/v1/steam/login", control_plane()))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("couldn't reach the service: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("the service refused the sign-in ({})", resp.status()));
    }
    #[derive(Deserialize)]
    struct Login {
        url: String,
    }
    let login: Login = resp.json().await.map_err(|e| format!("bad response: {e}"))?;
    Ok(login.url)
}

/// The Steam ID this account is linked to now, or `None` if not linked yet. The wall polls this
/// after opening the browser, then calls [`check`] again to have the gate lower the wall.
pub async fn steam_link_status(app: &AppHandle) -> Result<Option<String>, String> {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    let token = cfg.cp_token.trim().to_string();
    if token.is_empty() {
        return Ok(None);
    }
    let resp = http()
        .get(format!("{}/v1/entitlements", control_plane()))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| format!("couldn't reach the service: {e}"))?;
    // Shown to the person when the wall gives up, so it has to read as a sentence rather than
    // as a log line.
    if !resp.status().is_success() {
        return Err(format!("couldn't check the sign-in ({})", resp.status()));
    }
    #[derive(Deserialize)]
    struct Ent {
        #[serde(rename = "steamId")]
        steam_id: Option<String>,
    }
    let ent: Ent = resp.json().await.map_err(|e| format!("bad response: {e}"))?;
    Ok(ent.steam_id)
}
