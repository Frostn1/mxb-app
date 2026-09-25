//! The startup gate, shared by every app in the lineup.
//!
//! MXB App, Frost's Studio and MXB Coach are one product and one company, so a rider who is
//! banned — or, when the deployment requires it, not signed in with Steam — is refused all of
//! them, not just the one that happens to hold secured content. This is the client side of
//! `GET /v1/app/gate`: each app calls [`enforce_marker`], spawns [`check`] and starts [`watch`]
//! first thing in its `setup`, and the server's one verdict decides what happens.
//!
//! Three verdicts:
//!  - `ok` — run;
//!  - `signin` — a Steam sign-in is required first. Honest: the app raises a wall (the
//!    `mxb-signin-required` event) and unlocks the moment Valve confirms the account. Never
//!    fatal — the person can complete it.
//!  - `unsupported` — a banned install. The app shows a mundane untruth ("this copy couldn't be
//!    verified") and closes. Disguised on purpose. See the control plane's `bans.ts` for why the
//!    app is lied to while the website is not.
//!
//! ## Offline, and in between launches
//!
//! The server also sends each verdict as an Ed25519-signed statement (`signed`, from the control
//! plane's `verdict.ts`) naming the account it is about and when it was issued. The last one is
//! kept in the folder every app in the lineup shares ([`crate::config::data_dir`]), so a block
//! given to MXB App holds in Studio and Coach too. The policy is the humane one:
//!
//!  - an install never told it is banned keeps working offline, exactly as it always has — no
//!    network, a timeout or an unreadable answer is never a reason to refuse anybody;
//!  - an install given a signed block stays blocked offline, and only a newer signed `ok` (or
//!    `signin`) for the same account lifts it. A stored "ok" cannot be forged into a lift, and a
//!    stored block cannot be carried onto another account, because the app holds only the half
//!    of the key that checks.
//!
//! The older per-app `gate.lock` marker is still read as a block signal, for installs blocked
//! before verdicts were signed. And a verdict is not only asked at launch: [`watch`] re-asks every
//! half hour, and [`note_refusal`] re-asks at once when any control-plane call comes back 403
//! with `code: "blocked"`, so a ban lands on a running app rather than on its next start.
//!
//! Living in `mxb-core` is the point: the gate, the account it needs (`account::ensure_token`)
//! and the Steam-link round trip all sit here once, so a new app in the lineup is secured by
//! calling three functions rather than by copying the machinery and letting it drift.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// Public half of the pair the control plane signs gate verdicts with (raw 32 bytes, base64url).
///
/// Empty in source: the release sets it to the public key printed by the control plane's
/// `scripts/verdict-keypair.ts`, whose private half lives only in the worker's
/// `MXB_VERDICT_SIGNING_KEY` secret. It is not a secret itself — it can only check a signature,
/// never make one. While it is empty no verdict verifies, nothing is kept, and the gate behaves
/// exactly as it did before verdicts were signed.
///
/// Rotating it is a release: a build verifies against the key it was built with, and treats a
/// verdict signed by any other as unsigned.
pub const VERDICT_PUBLIC_KEY: &str = "";

/// Signed-verdict format this build understands. A newer one is ignored rather than half-read.
const VERDICT_VERSION: u32 = 1;

/// How often a running app re-asks the gate. Long enough to be no load at all, short enough that
/// a ban reaches an app left open all evening.
const RECHECK_EVERY: Duration = Duration::from_secs(30 * 60);

/// The fewest seconds between two re-asks prompted by a refused call. A burst of 403s — paint
/// sync fanning out, say — is one question to the gate, not twenty.
const NUDGE_EVERY_SECS: u64 = 60;

/// How long a launch that already holds a block waits to hear it has been lifted, before refusing.
/// Only a blocked install ever waits; everybody else starts without touching the network.
const LIFT_WAIT: Duration = Duration::from_secs(8);

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

/// Where the older, per-app "stay blocked" marker lives. From before verdicts were signed, and
/// still written on a block and read at startup, so an install blocked by an earlier build stays
/// blocked; being a plain file anybody can delete is why it is no longer the only signal. `None`
/// only if there is no data dir to write into.
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

// ---------------------------------------------------------------------------------------
// Signed verdicts: the one kept in the shared folder, and the rules for replacing it.
// ---------------------------------------------------------------------------------------

/// The `signed` field of a gate answer: the payload as the exact string that was signed, and an
/// Ed25519 signature over its UTF-8 bytes, base64url without padding. Kept on disk as it came.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedVerdict {
    pub payload: String,
    pub sig: String,
}

/// What a verified signed verdict says.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VerdictPayload {
    pub v: u32,
    /// `ok`, `signin` or `unsupported` — the same three words as the unsigned verdict.
    pub status: String,
    /// The account it is about. A verdict for one account never decides anything for another.
    pub account: String,
    /// SHA-256 of the bearer token the verdict was fetched with, lowercase hex — see
    /// [`token_digest`]. Signed, so a kept verdict cannot be re-pointed at another account by
    /// editing the file. The app never learns its account id except from a verdict, so this is
    /// how a launch knows a kept verdict is about the account it is signed in as.
    pub token: Option<String>,
    #[serde(rename = "steamId")]
    pub steam_id: Option<String>,
    pub guid: Option<String>,
    /// Milliseconds since epoch, by the server's clock — the only clock the ordering trusts.
    #[serde(rename = "issuedAt")]
    pub issued_at: i64,
}

impl VerdictPayload {
    fn blocks(&self) -> bool {
        self.status == "unsupported"
    }

    /// Whether this verdict was fetched with the token whose digest is given.
    fn for_token(&self, token_digest: &str) -> bool {
        self.token.as_deref() == Some(token_digest)
    }
}

/// Check a signed verdict against a named key and return what it says.
///
/// Every failure is the same answer — "not a verdict we can keep" — and the caller treats it as
/// if the server had sent no signature at all, which is how every verdict was read before this.
pub fn verify_verdict_with(signed: &SignedVerdict, public_key_b64: &str) -> Result<VerdictPayload, String> {
    let key_bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(public_key_b64.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("no verdict key is built in")?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "the built-in verdict key is invalid")?;
    let sig: [u8; 64] = URL_SAFE_NO_PAD
        .decode(signed.sig.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("the verdict signature is malformed")?;
    key.verify_strict(signed.payload.as_bytes(), &Signature::from_bytes(&sig))
        .map_err(|_| "the verdict was not signed by us")?;
    let payload: VerdictPayload =
        serde_json::from_str(&signed.payload).map_err(|e| format!("unreadable verdict ({e})"))?;
    if payload.v != VERDICT_VERSION {
        return Err(format!("verdict version {} is not one this build reads", payload.v));
    }
    Ok(payload)
}

/// [`verify_verdict_with`] against the key this build ships with.
pub fn verify_verdict(signed: &SignedVerdict) -> Result<VerdictPayload, String> {
    verify_verdict_with(signed, VERDICT_PUBLIC_KEY)
}

/// SHA-256 of a token, lowercase hex — the control plane's own `hashToken`, so the digest the
/// server signs and the one a launch computes from its config are the same string.
pub fn token_digest(token: &str) -> String {
    Sha256::digest(token.trim().as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a kept verdict refuses a launch holding the token with this digest: it verifies, it is
/// a block, and it is about this token's account. Another account's block is ignored, never obeyed.
pub fn stored_blocks(stored: &SignedVerdict, token_digest: &str, public_key_b64: &str) -> bool {
    verify_verdict_with(stored, public_key_b64).is_ok_and(|p| p.blocks() && p.for_token(token_digest))
}

/// Whether a freshly verified verdict should replace the one kept.
///
/// - Nothing kept, or nothing that still verifies: keep the new one.
/// - The same account: only a strictly newer verdict replaces — so a replayed old `ok` cannot lift
///   a block, and a block is lifted only by a later `ok` or `signin` for that account.
/// - Another account: a kept block is not the new account's to lift, so it stays unless the new
///   verdict is a block too; anything else is simply superseded.
pub fn supersedes(kept: Option<&VerdictPayload>, incoming: &VerdictPayload) -> bool {
    match kept {
        None => true,
        Some(k) if k.account == incoming.account => incoming.issued_at > k.issued_at,
        Some(k) => !k.blocks() || incoming.blocks(),
    }
}

/// What [`keep_if_newer`] did with a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// It superseded what was there and is now the kept verdict.
    Stored,
    /// The kept verdict is a newer one for the same account: this answer was overtaken in flight
    /// and must not be acted on either.
    Overtaken,
    /// It was not kept — another account's block stands, or the write failed — but it is not
    /// stale, so the running app may still act on it.
    NotKept,
}

/// Where the kept verdict lives: the folder every app in the lineup shares, beside the config
/// that holds the token it was fetched with. Not `app_local_data_dir`, which is per app.
fn verdict_path(app: &AppHandle) -> Option<PathBuf> {
    crate::config::data_dir(app).map(|d| d.join("gate-verdict.json"))
}

/// The kept verdict, or `None` when there is none or it cannot be read. Unreadable is "none": a
/// damaged file is not evidence of anything.
pub fn read_stored(path: &Path) -> Option<SignedVerdict> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Keep `incoming` if it supersedes what is there.
///
/// Three apps share the file, so the read, the comparison and the write happen under an exclusive
/// lock on a sibling file: without it two apps could both pass the comparison against the same
/// old verdict and the older answer could land last, undoing a lift or a block. The write goes to
/// a temporary name of this process's own and is moved in, so a crash never leaves half a verdict.
pub fn keep_if_newer(path: &Path, incoming_signed: &SignedVerdict, incoming: &VerdictPayload, public_key_b64: &str) -> Kept {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Held until the end of this function. If the lock cannot be had at all (a filesystem without
    // locks), carry on unlocked: a rare race is better than never keeping a verdict.
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path.with_extension("lock"));
    let _guard = lock.as_ref().ok().filter(|f| f.lock().is_ok());

    let kept = read_stored(path).and_then(|s| verify_verdict_with(&s, public_key_b64).ok());
    if !supersedes(kept.as_ref(), incoming) {
        let overtaken = kept.is_some_and(|k| k.account == incoming.account && k.issued_at > incoming.issued_at);
        return if overtaken { Kept::Overtaken } else { Kept::NotKept };
    }
    let Ok(text) = serde_json::to_string(incoming_signed) else { return Kept::NotKept };
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    match std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)) {
        Ok(()) => Kept::Stored,
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            Kept::NotKept
        }
    }
}

/// The token this install holds in the shared config, read straight from the file.
///
/// Not `config::load`, which may migrate and rewrite the config — the gate runs before the app
/// has decided anything, and reading one field is all it needs.
fn current_token(app: &AppHandle) -> Option<String> {
    let text = std::fs::read_to_string(crate::config::config_path(app)).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let token = doc.get("cpToken")?.as_str()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Whether the kept, signed verdict refuses this launch.
fn stored_block(app: &AppHandle) -> bool {
    let (Some(path), Some(token)) = (verdict_path(app), current_token(app)) else { return false };
    read_stored(&path).is_some_and(|s| stored_blocks(&s, &token_digest(&token), VERDICT_PUBLIC_KEY))
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

/// Refuse at once if a previous run was told to. Called at the very top of `setup`, before the
/// window is built, so a blocked install never flashes a usable window and never needs the
/// network to enforce a block it has already been given. A no-op, with no network, for everyone
/// else.
///
/// A block here is either the signed verdict every app shares, for the account this install is
/// signed in as, or the older per-app `gate.lock`. Before refusing, a blocked install asks the
/// gate once, for at most [`LIFT_WAIT`]: that is how a lifted ban gets back in — otherwise the
/// block would refuse every launch before any check could hear the lift. Offline, the ask fails
/// and the block stands.
pub fn enforce_marker(app: &AppHandle) {
    if blocked(app).is_none() && !stored_block(app) {
        return;
    }

    let handle = app.clone();
    let answer = tauri::async_runtime::block_on(async move {
        tokio::time::timeout(LIFT_WAIT, ask(&handle)).await.ok().flatten()
    });
    if let Some(answer) = answer {
        match answer.verdict {
            Verdict::Unsupported { message } => {
                let message = if message.trim().is_empty() { FALLBACK_BLOCK.to_string() } else { message };
                if !answer.signed {
                    mark(app, &message);
                }
                deny(app, &message);
            }
            // The server says this install may run. The per-app marker was only ever as good as
            // the unsigned answer that wrote it, so any fresh answer lifts it, as `check` always
            // did; a signed block needs a newer signed lift, which `ask` has kept if one came.
            _ => unmark(app),
        }
    }
    if let Some(message) = blocked(app) {
        deny(app, &message);
    }
    if stored_block(app) {
        deny(app, FALLBACK_BLOCK);
    }
}

/// A verdict worth acting on, and whether it came signed and verified.
struct Answer {
    verdict: Verdict,
    /// The signed form verified and is now the kept verdict: it speaks for this block, so the
    /// per-app marker — which knows no account and cannot be lifted from another app — is not
    /// written.
    signed: bool,
}

/// One round trip to the gate: the verdict, with its signed form kept when it verifies and
/// supersedes what is kept. `None` for any network, auth or parse failure — "don't know" — and
/// for an answer that was overtaken before it arrived: one older than the verdict already kept
/// for this account, or fetched for a token the config no longer holds.
async fn ask(app: &AppHandle) -> Option<Answer> {
    remember(app);
    let token = match account::ensure_token(app).await {
        Ok(t) => t,
        Err(e) => {
            log::info!("[gate] no verdict this run ({e})");
            return None;
        }
    };

    // The machine's one-way hash rides along (`device.rs`), so a ban follows the PC to a fresh
    // account. Never the machine id itself; absent when the OS will not say.
    let resp = match crate::device::with_device(
        http().get(format!("{}/v1/app/gate", control_plane())).bearer_auth(&token),
    )
    .send()
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::info!("[gate] no verdict this run ({e})");
            return None;
        }
    };
    if !resp.status().is_success() {
        log::info!("[gate] service answered {}", resp.status());
        return None;
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            log::info!("[gate] unreadable verdict ({e})");
            return None;
        }
    };
    let verdict = match Verdict::deserialize(&body) {
        Ok(v) => v,
        Err(e) => {
            log::info!("[gate] unreadable verdict ({e})");
            return None;
        }
    };

    // Signed in as somebody else since the request left — another app claimed or replaced the
    // token. This answer is about an account that is no longer this install's.
    if current_token(app).is_some_and(|now| now != token.trim()) {
        log::info!("[gate] the account changed while asking; answer set aside");
        return None;
    }

    // The signed form, kept when it verifies, is about this token, and says the same as the plain
    // answer. One that does not verify — no key built in, a key rotated since this build — is
    // simply not kept: the plain verdict is still acted on, as it always was.
    let mut signed = false;
    if let Some(sv) = body.get("signed").and_then(|v| SignedVerdict::deserialize(v).ok()) {
        match verify_verdict(&sv) {
            Ok(p) if p.status == verdict.status() && p.for_token(&token_digest(&token)) => {
                let kept = verdict_path(app).map(|path| keep_if_newer(&path, &sv, &p, VERDICT_PUBLIC_KEY));
                if kept == Some(Kept::Overtaken) {
                    log::info!("[gate] a newer verdict is already kept; answer set aside");
                    return None;
                }
                // Only a verdict that is actually on disk speaks for a block. One that could not
                // be written falls back to the per-app marker, so it still holds next launch.
                signed = kept == Some(Kept::Stored);
            }
            Ok(_) => log::warn!("[gate] the signed verdict disagrees with the plain one; not kept"),
            Err(e) => log::debug!("[gate] signed verdict not kept ({e})"),
        }
    }
    Some(Answer { verdict, signed })
}

impl Verdict {
    /// The wire word for this verdict, as the signed payload spells it.
    fn status(&self) -> &'static str {
        match self {
            Verdict::Ok { .. } => "ok",
            Verdict::Signin { .. } => "signin",
            Verdict::Unsupported { .. } => "unsupported",
        }
    }
}

/// Ask the server whether this install may run, and act on the answer.
///
/// Spawned in the background from `setup` so it never delays a legitimate launch. `ok` clears any
/// stale per-app marker and lowers the sign-in wall; `signin` raises the wall (never fatal);
/// `unsupported` tears the app down. Any network or auth error does nothing — a block already
/// kept stands, and an install that was never blocked keeps running.
///
/// One at a time: a second call waits for the first rather than racing it, so an older answer can
/// never land after a newer one in the same process.
pub async fn check(app: AppHandle) {
    {
        let _one = in_flight().lock().await;
        PENDING.store(false, Ordering::Release);
        run(&app).await;
    }
    if PENDING.swap(false, Ordering::AcqRel) {
        background_check(app).await;
    }
}

/// The body of [`check`], for a caller already holding [`in_flight`].
async fn run(app: &AppHandle) {
    let Some(answer) = ask(app).await else { return };

    match answer.verdict {
        Verdict::Ok { steam } => {
            STEAM.store(if steam { STEAM_YES } else { STEAM_NO }, Ordering::Relaxed);
            unmark(app);
            announce(app, SigninRequired { required: false, message: String::new() });
        }
        Verdict::Signin { message } => {
            // A sign-in wall is only raised for an account Valve has not confirmed, so this
            // verdict is itself the answer — and it is the one state where "not signed in" is
            // known rather than merely untold.
            STEAM.store(STEAM_NO, Ordering::Relaxed);
            let message = if message.trim().is_empty() { FALLBACK_SIGNIN.to_string() } else { message };
            log::info!("[gate] a Steam sign-in is required before this install may run");
            announce(app, SigninRequired { required: true, message });
        }
        Verdict::Unsupported { message } => {
            let message = if message.trim().is_empty() { FALLBACK_BLOCK.to_string() } else { message };
            // An unsigned block is remembered the old way, per app; a signed one is already kept,
            // for this account, where every app reads it.
            if !answer.signed {
                mark(app, &message);
            }
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || deny(&handle, &message));
        }
    }
}

// ---------------------------------------------------------------------------------------
// Asking again while the app runs: every half hour, and at once when a call is refused.
// ---------------------------------------------------------------------------------------

/// The handle the background re-checks run against, set by the first [`watch`] or [`check`]. Held
/// here so a refused call in a module with no handle of its own can still say so.
fn app_handle() -> &'static OnceLock<AppHandle> {
    static APP: OnceLock<AppHandle> = OnceLock::new();
    &APP
}

fn remember(app: &AppHandle) {
    let _ = app_handle().set(app.clone());
}

/// Held for the length of a check. Two at once on an install with no token yet would both claim
/// a device account, and two answers could land in either order.
fn in_flight() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// A re-check was asked for while one was running. The running one may have left before the
/// reason to ask again arrived — a refusal seen mid-check — so it runs once more when it ends.
static PENDING: AtomicBool = AtomicBool::new(false);

/// A re-check nobody is waiting on. When one is already running it is not skipped but queued:
/// the running check runs once more when it finishes.
async fn background_check(app: AppHandle) {
    loop {
        let Ok(one) = in_flight().try_lock() else {
            PENDING.store(true, Ordering::Release);
            // The running check may have finished, and read the flag, between the failed lock and
            // the store. If the lock is free now, nobody is left to drain the flag: take the turn.
            // Otherwise the holder reads it after it lets go, so the re-run is not lost.
            if in_flight().try_lock().is_ok() {
                continue;
            }
            return;
        };
        PENDING.store(false, Ordering::Release);
        run(&app).await;
        drop(one);
        if !PENDING.swap(false, Ordering::AcqRel) {
            return;
        }
    }
}

/// Re-ask the gate every [`RECHECK_EVERY`] for as long as the app runs. Call once from `setup`,
/// beside the startup [`check`]; the first re-ask is one interval after launch.
pub fn watch(app: &AppHandle) {
    remember(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(RECHECK_EVERY).await;
            background_check(app.clone()).await;
        }
    });
}

/// Whether a control-plane answer is the refusal a block produces: a 403 whose body carries
/// `code: "blocked"`. Any other 403 — not entitled, needs an invite — is not.
pub fn is_block_refusal(status: u16, body: &str) -> bool {
    status == 403
        && serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .is_some_and(|v| v.get("code").and_then(|c| c.as_str()) == Some("blocked"))
}

/// Tell the gate a control-plane call was refused. Call it with the status and body of a failed
/// control-plane response; when it is a block refusal the gate is re-asked at once (at most once
/// a minute) rather than at the next half-hourly check.
///
/// `body` is `None` for a caller that never read one (an `error_for_status` path): a bare 403
/// there is taken as worth a re-ask, and the gate — not the guess — decides what it meant.
pub fn note_refusal(status: u16, body: Option<&str>) {
    let worth_asking = match body {
        Some(body) => is_block_refusal(status, body),
        None => status == 403,
    };
    if !worth_asking {
        return;
    }
    let Some(app) = app_handle().get() else { return };
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let last = LAST.load(Ordering::Relaxed);
    if now.saturating_sub(last) < NUDGE_EVERY_SECS
        || LAST.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_err()
    {
        return;
    }
    log::info!("[gate] a call was refused; asking the gate again now");
    tauri::async_runtime::spawn(background_check(app.clone()));
}

/// [`note_refusal`] for a `reqwest` error from `error_for_status`, which carries the status and
/// nothing else.
pub fn note_error(err: &reqwest::Error) {
    if let Some(status) = err.status() {
        note_refusal(status.as_u16(), None);
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
        let status = resp.status();
        note_refusal(status.as_u16(), Some(&resp.text().await.unwrap_or_default()));
        return Err(format!("the service refused the sign-in ({status})"));
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
        let status = resp.status();
        note_refusal(status.as_u16(), Some(&resp.text().await.unwrap_or_default()));
        return Err(format!("couldn't check the sign-in ({status})"));
    }
    #[derive(Deserialize)]
    struct Ent {
        #[serde(rename = "steamId")]
        steam_id: Option<String>,
    }
    let ent: Ent = resp.json().await.map_err(|e| format!("bad response: {e}"))?;
    Ok(ent.steam_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// A pair made here, for the tests only. The shipped key is a different one.
    fn pair(seed: u8) -> (SigningKey, String) {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let public = URL_SAFE_NO_PAD.encode(key.verifying_key().to_bytes());
        (key, public)
    }

    /// A verdict for `account`, fetched with `token`, the way the control plane words one.
    fn sign(key: &SigningKey, status: &str, account: &str, token: &str, issued_at: i64) -> SignedVerdict {
        let payload = format!(
            r#"{{"v":1,"status":"{status}","account":"{account}","token":"{}","steamId":null,"guid":"FF0110000111111111","issuedAt":{issued_at}}}"#,
            token_digest(token)
        );
        let sig = URL_SAFE_NO_PAD.encode(key.sign(payload.as_bytes()).to_bytes());
        SignedVerdict { payload, sig }
    }

    fn keep(path: &Path, signed: &SignedVerdict, public: &str) -> Kept {
        let p = verify_verdict_with(signed, public).unwrap();
        keep_if_newer(path, signed, &p, public)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxb-appgate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("gate-verdict.json")
    }

    /// A verdict the control plane's own `signPayload` produced, in TypeScript, under a throwaway
    /// pair — pasted verbatim. The one seam neither side can check alone: the exact bytes signed,
    /// base64url without padding, field names in camelCase, the token digest as `hashToken` makes it.
    const TS_PUBLIC_KEY: &str = "6UPKDEVIrh4oaY0czLmDznbPNuH_8KtzkuT_GJaLQ-c";
    const TS_PAYLOAD: &str = r#"{"v":1,"status":"unsupported","account":"acc_test","token":"4c5dc9b7708905f77f5e5d16316b5dfb425e68cb326dcd55a860e90a7707031e","steamId":"76561198000000042","guid":"FF0110000111111111","issuedAt":1800000000000}"#;
    const TS_SIG: &str = "N43S6cB0pacXpcJiy_sX5P5T7UTp_mmjeKHXr1wUx46-wfMRP8Bju5ioTmkSNIeIVktLfQomLtKsOgq5eTKqAA";

    #[test]
    fn verifies_a_verdict_the_typescript_worker_actually_signed() {
        let signed = SignedVerdict { payload: TS_PAYLOAD.into(), sig: TS_SIG.into() };
        let p = verify_verdict_with(&signed, TS_PUBLIC_KEY).expect("the worker's own verdict must verify");
        assert_eq!(p.v, 1);
        assert_eq!(p.status, "unsupported");
        assert_eq!(p.account, "acc_test");
        // The worker's `hashToken` and this crate's `token_digest` must agree on the same token.
        assert!(p.for_token(&token_digest("test-token")));
        assert_eq!(p.steam_id.as_deref(), Some("76561198000000042"));
        assert_eq!(p.guid.as_deref(), Some("FF0110000111111111"));
        assert_eq!(p.issued_at, 1_800_000_000_000);
    }

    #[test]
    fn a_good_signature_verifies_and_a_bad_or_tampered_one_does_not() {
        let (key, public) = pair(1);
        let signed = sign(&key, "ok", "acc_a", "token-a", 10);
        assert_eq!(verify_verdict_with(&signed, &public).unwrap().status, "ok");

        // Tampered: the payload says something it was not signed saying.
        let tampered = SignedVerdict { payload: signed.payload.replace("\"ok\"", "\"unsupported\""), ..signed.clone() };
        assert!(verify_verdict_with(&tampered, &public).is_err());

        // Signed by somebody else's key.
        let (_, other) = pair(2);
        assert!(verify_verdict_with(&signed, &other).is_err());

        // Garbage where the signature goes.
        let bad = SignedVerdict { sig: "not-a-signature".into(), ..signed.clone() };
        assert!(verify_verdict_with(&bad, &public).is_err());

        // A build with no key in it keeps nothing, rather than trusting everything.
        assert!(verify_verdict_with(&signed, "").is_err());
    }

    #[test]
    fn a_verdict_of_an_unknown_version_is_not_read() {
        let (key, public) = pair(3);
        let payload = r#"{"v":2,"status":"ok","account":"acc_a","token":null,"steamId":null,"guid":null,"issuedAt":1}"#.to_string();
        let sig = URL_SAFE_NO_PAD.encode(key.sign(payload.as_bytes()).to_bytes());
        assert!(verify_verdict_with(&SignedVerdict { payload, sig }, &public).is_err());
    }

    #[test]
    fn a_newer_ok_lifts_a_kept_block_and_an_older_one_does_not() {
        let (key, public) = pair(4);
        let path = scratch("precedence");
        let tok = token_digest("token-a");

        assert_eq!(keep(&path, &sign(&key, "unsupported", "acc_a", "token-a", 1_000), &public), Kept::Stored);
        assert!(stored_blocks(&read_stored(&path).unwrap(), &tok, &public));

        // An older ok — a replayed one, or one overtaken in flight — changes nothing, and is
        // reported as overtaken so the running app does not act on it either.
        assert_eq!(keep(&path, &sign(&key, "ok", "acc_a", "token-a", 999), &public), Kept::Overtaken);
        assert!(stored_blocks(&read_stored(&path).unwrap(), &tok, &public));

        // So does one issued at the same instant: strictly newer only.
        assert_eq!(keep(&path, &sign(&key, "ok", "acc_a", "token-a", 1_000), &public), Kept::NotKept);
        assert!(stored_blocks(&read_stored(&path).unwrap(), &tok, &public));

        // A newer signin lifts it just as an ok does — neither is a block.
        assert_eq!(keep(&path, &sign(&key, "signin", "acc_a", "token-a", 1_001), &public), Kept::Stored);
        assert!(!stored_blocks(&read_stored(&path).unwrap(), &tok, &public));
    }

    #[test]
    fn another_accounts_verdict_neither_blocks_nor_lifts() {
        let (key, public) = pair(5);
        let path = scratch("accounts");
        let tok_a = token_digest("token-a");
        let tok_b = token_digest("token-b");

        assert_eq!(keep(&path, &sign(&key, "unsupported", "acc_a", "token-a", 1_000), &public), Kept::Stored);

        // A launch holding another account's token is not refused by account A's block.
        assert!(!stored_blocks(&read_stored(&path).unwrap(), &tok_b, &public));

        // Nor does account B's ok, however new, lift account A's block — and it is not stale, so
        // B's running app may still act on it.
        assert_eq!(keep(&path, &sign(&key, "ok", "acc_b", "token-b", 5_000), &public), Kept::NotKept);
        assert!(stored_blocks(&read_stored(&path).unwrap(), &tok_a, &public));

        // A block for the new account does replace it: that is the one the current launch is about.
        assert_eq!(keep(&path, &sign(&key, "unsupported", "acc_b", "token-b", 5_001), &public), Kept::Stored);
        assert!(stored_blocks(&read_stored(&path).unwrap(), &tok_b, &public));
        assert!(!stored_blocks(&read_stored(&path).unwrap(), &tok_a, &public));
    }

    #[test]
    fn a_kept_ok_never_blocks_and_a_forged_block_is_ignored() {
        let (key, public) = pair(6);
        let tok = token_digest("token-a");
        assert!(!stored_blocks(&sign(&key, "ok", "acc_a", "token-a", 1), &tok, &public));

        // A block written by hand, or signed by anybody else, is not a block.
        let (forger, _) = pair(7);
        assert!(!stored_blocks(&sign(&forger, "unsupported", "acc_a", "token-a", 1), &tok, &public));
    }

    #[test]
    fn the_account_binding_is_inside_the_signature() {
        // Re-pointing a kept block at another token means editing the signed payload, which
        // breaks the signature — so a block cannot be moved onto an account, nor moved off one.
        let (key, public) = pair(8);
        let block = sign(&key, "unsupported", "acc_a", "token-a", 1);
        let moved = SignedVerdict {
            payload: block.payload.replace(&token_digest("token-a"), &token_digest("token-b")),
            ..block.clone()
        };
        assert!(!stored_blocks(&moved, &token_digest("token-b"), &public));
        assert!(!stored_blocks(&moved, &token_digest("token-a"), &public));
        assert!(stored_blocks(&block, &token_digest("token-a"), &public));
    }

    #[test]
    fn supersedes_follows_the_rules_it_states() {
        let v = |status: &str, account: &str, at: i64| VerdictPayload {
            v: 1,
            status: status.into(),
            account: account.into(),
            token: None,
            steam_id: None,
            guid: None,
            issued_at: at,
        };
        assert!(supersedes(None, &v("ok", "a", 1)));
        assert!(supersedes(Some(&v("ok", "a", 1)), &v("unsupported", "a", 2)));
        assert!(!supersedes(Some(&v("ok", "a", 2)), &v("unsupported", "a", 1)));
        assert!(supersedes(Some(&v("ok", "a", 9)), &v("ok", "b", 1)));
        assert!(!supersedes(Some(&v("unsupported", "a", 1)), &v("ok", "b", 9)));
    }

    #[test]
    fn only_a_block_refusal_asks_the_gate_again() {
        assert!(is_block_refusal(403, r#"{"error":"This copy couldn't be verified.","code":"blocked"}"#));
        assert!(!is_block_refusal(403, r#"{"error":"that needs an invite"}"#));
        assert!(!is_block_refusal(401, r#"{"code":"blocked"}"#));
        assert!(!is_block_refusal(403, "not json"));
    }

    #[test]
    fn the_token_digest_is_stable_and_ignores_surrounding_space() {
        assert_eq!(token_digest("abc"), token_digest("  abc\n"));
        assert_eq!(token_digest("abc").len(), 64);
        assert_ne!(token_digest("abc"), token_digest("abd"));
    }
}
