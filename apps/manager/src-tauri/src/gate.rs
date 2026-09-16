//! The startup gate: may this installation run at all?
//!
//! MXB App, Studio, Coach and FrostMod are one product, and a rider banned for unlocking
//! protected content and passing it around is banned from the lot — including the desktop app
//! in front of them, not only its online features. This is what stops the app from opening.
//!
//! ## Why it lies
//!
//! The control plane knows the refusal is a ban; the person is told their copy "couldn't be
//! verified" and to reinstall. That is deliberate, and it is the opposite of what the website
//! does (there a creator is told plainly, because that is where an appeal starts). The app is
//! not a place to argue — it is a place a content thief is trying to keep using, and:
//!
//!  * "banned" only tells them to make another account;
//!  * the honest message we show a creator is exactly the next-step coaching a thief would act
//!    on;
//!  * the reinstall the disguise names cannot help, because a ban follows the GUID, the Steam
//!    login and the install — never the files — so a pirate who trusts the message burns their
//!    afternoon on a fix that was never going to work.
//!
//! The verdict, and its wording, come from the server (`GET /v1/app/gate`), so a ban can be
//! lifted without shipping a new build.
//!
//! ## Why it survives going offline
//!
//! A block is written to a marker file the moment the server reports one, and the marker is
//! read *before the window is built* on every later launch. So pulling the network cable after
//! the first block does not get the app back — a blocked install stays blocked offline. A clean
//! install writes no marker and is never delayed or stopped, and a genuine network failure with
//! no marker present fails open, because we cannot tell a ban from a dead connection and must
//! not lock out a paying rider whose wifi dropped.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// The fallback shown if the server sent an empty message, or a stale marker holds none. Kept
/// in step with the control plane's `APP_BLOCK_MESSAGE`; the server's own text wins when present.
const FALLBACK: &str =
    "This copy couldn't be verified. It may be out of date or damaged — reinstall the latest version from mxbsecure.com.";

/// The verdict the server returns from `GET /v1/app/gate`.
#[derive(serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Verdict {
    /// This install may run.
    Ok,
    /// It may not. `message` is what to show — a mundane untruth, never the word "ban".
    Unsupported {
        #[serde(default)]
        message: String,
    },
}

/// Where the "stay blocked, even offline" marker lives. `None` only if there is no data dir to
/// write into, which is the same situation the rest of the app's local state cannot survive.
fn marker_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("gate.lock"))
}

/// The message a marker holds, or `None` when this install is not marked blocked. An empty or
/// unreadable marker still counts as blocked — its presence is the signal, the text is a nicety.
pub fn blocked(app: &AppHandle) -> Option<String> {
    let path = marker_path(app)?;
    if !path.exists() {
        return None;
    }
    let msg = std::fs::read_to_string(&path).unwrap_or_default();
    Some(if msg.trim().is_empty() { FALLBACK.to_string() } else { msg })
}

fn mark(app: &AppHandle, message: &str) {
    if let Some(path) = marker_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, if message.trim().is_empty() { FALLBACK } else { message });
    }
}

fn unmark(app: &AppHandle) {
    if let Some(path) = marker_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// Show the message and end the process. Must run on the main thread — dialogs do on macOS —
/// which is true of the setup call, and arranged with `run_on_main_thread` for the async one.
pub fn deny(app: &AppHandle, message: &str) -> ! {
    log::warn!("[gate] this installation is blocked; refusing to start");
    app.dialog()
        .message(message)
        .kind(MessageDialogKind::Error)
        .title("MXB App")
        .blocking_show();
    app.exit(1);
    // `app.exit` schedules the loop to stop but returns; nothing below should run.
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
/// Spawned in the background from `setup` so it never delays a legitimate launch. On a block it
/// writes the marker (so the next launch is instant and offline-proof) and then tears the app
/// down on the main thread with the message. On `ok` it clears any stale marker — this is how a
/// lifted ban lets the app back in. On any network or auth error it does nothing: a marker
/// already written stands, and an install that was never blocked keeps running.
pub async fn check(app: AppHandle) {
    let cfg = crate::config::load_or_detect(&app).unwrap_or_default();
    // The same account voice and unlock use. Claims a self-serve one if there is none, so a
    // fresh install is gated too. An error here is "couldn't reach the service" — fail open.
    let token = match crate::voice::signal::account(&app, &cfg).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("[gate] no verdict this run ({e})");
            return;
        }
    };

    let resp = match reqwest::Client::new()
        .get(format!("{}/v1/app/gate", crate::paintsync::control_plane()))
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
        // An unreachable or erroring gate is not a block: only an explicit `unsupported` is.
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
        Verdict::Ok => unmark(&app),
        Verdict::Unsupported { message } => {
            let message = if message.trim().is_empty() { FALLBACK.to_string() } else { message };
            mark(&app, &message);
            let handle = app.clone();
            // Dialogs and process teardown belong on the main thread.
            let _ = app.run_on_main_thread(move || deny(&handle, &message));
        }
    }
}
