//! Anonymous usage counters.
//!
//! Which parts of this app people actually open has never been knowable. Release downloads
//! count downloads, and the handful of enrolled accounts describe the handful of enrolled
//! accounts — so every decision about what to build next, and what to stop carrying, has
//! been made from the loudest voice in Discord.
//!
//! ## What leaves the machine
//!
//! An install id — a random UUID this app generated for itself and keeps in its own config —
//! the app version, the OS, which title is active, a session count, minutes the app was
//! open, and counters for named events. That is the whole payload. No rider name, no GUID,
//! no account, no paths, no mod names: [`is_event_name`] is what makes it impossible for a
//! call site to smuggle one in, because a name carrying anything but `area.thing` is dropped
//! rather than sent.
//!
//! ## What stops it
//!
//! The player's own setting, first — off means nothing is buffered, not merely nothing sent.
//! Then [`DISABLE_ENV`] for a run. And debug builds are silent unless [`DEV_ENV`] says
//! otherwise, so working on the app doesn't quietly become a user of it.

/// A new anonymous install id.
///
/// Behind a feature so only the binary that owns the reporting pulls `uuid` in. A build
/// without it never mints one, and an empty id means "do not report" — which is the right
/// answer for a second app sharing this config: the id belongs to the install, and the
/// manager is what created it.
#[cfg(feature = "mint-install-id")]
fn mint_install_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(not(feature = "mint-install-id"))]
fn mint_install_id() -> String {
    String::new()
}

use crate::config::{self, AppConfig};
use crate::names::control_plane;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Set to `1` to send nothing for one run, whatever the config says.
pub const DISABLE_ENV: &str = "MXB_NO_ANALYTICS";

/// Set to `1` to let a debug build report. Off by default: a developer running the app forty
/// times a day would otherwise be the most active user it has.
pub const DEV_ENV: &str = "MXB_ANALYTICS_DEV";

/// How often the buffer is emptied.
///
/// Half-hourly, not the five minutes this shipped with. The app defaults to launching at
/// login and living in the tray, so the interval is not "per session" — it is per waking
/// hour of every machine that has the app. At five minutes that is ~288 requests a day from
/// an install nobody is even using, and on 2026-08-31 the control plane hit its daily
/// ceiling with a few hundred installs on it. Counters are counters: nothing here is worth
/// a request every five minutes, and a report that is late is worth exactly as much as one
/// that is prompt.
const FLUSH_EVERY: Duration = Duration::from_secs(30 * 60);

/// ...but the first report goes out a minute in.
///
/// Otherwise the app would only ever count sessions that lasted longer than a flush, and
/// "opened it, looked at one thing, closed it" — which is a great many of them — would be
/// invisible. The daily active number is exactly the number that bias would ruin. This is
/// what makes the long interval above affordable: the day's active install is recorded a
/// minute in, and everything after it is detail.
const FIRST_FLUSH: Duration = Duration::from_secs(60);

/// How long a quit waits for the last report. Short: nobody's shutdown should be held up by
/// a counter, and the next launch would carry the loss anyway.
const EXIT_GRACE: Duration = Duration::from_secs(3);

/// What a report got back, so the loop knows whether to keep trying.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// Sent, or there was nothing to send.
    Done,
    /// Kept, and worth trying again on the next tick.
    Retry,
    /// The endpoint is turning callers away. Every install would be hearing this at once,
    /// so the answer is to stop for the run rather than to knock again in half an hour.
    RateLimited,
}

/// Distinct names held at once — the same cap the endpoint enforces. Reaching it means a
/// call site is generating names rather than naming things, which is a bug on this side.
const MAX_EVENTS: usize = 64;

/// Longest a name may be, matching the endpoint. Anything near it is already wrong.
const MAX_NAME_LEN: usize = 48;

/// Whether anything is being recorded at all.
///
/// An atomic rather than a config read: [`track`] is called from click handlers and from
/// hot-ish paths, and reading a file to answer "should I count this" would be the most
/// expensive thing about counting it.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// What has happened since the last successful report.
static BUFFER: Mutex<Buffer> = Mutex::new(Buffer::new());

struct Buffer {
    events: BTreeMap<String, u32>,
    sessions: u32,
    /// Whole minutes not yet accepted by the endpoint — including any a failed report is
    /// still carrying.
    minutes: u32,
    /// Start of the stretch of running time the minutes above have not yet counted.
    open_since: Option<Instant>,
    /// Said once when a call site overflows [`MAX_EVENTS`], so a bug is visible in the log
    /// without filling it.
    warned: bool,
}

impl Buffer {
    const fn new() -> Self {
        Self {
            events: BTreeMap::new(),
            sessions: 0,
            minutes: 0,
            open_since: None,
            warned: false,
        }
    }

    /// Move the running clock forward, returning the minutes now owed.
    fn take_minutes(&mut self, now: Instant) -> u32 {
        if let Some(since) = self.open_since {
            let whole = now.duration_since(since).as_secs() / 60;
            if whole > 0 {
                // Advance by whole minutes only, so the seconds either side of a flush are
                // carried rather than rounded away at every flush.
                self.open_since = Some(since + Duration::from_secs(whole * 60));
                self.minutes = self.minutes.saturating_add(whole as u32);
            }
        }
        self.minutes.min(MAX_MINUTES)
    }
}

/// The endpoint's own ceiling for one report. Clamped here too, so a machine that slept for
/// a week sends a number the endpoint will accept rather than one it drops.
const MAX_MINUTES: u32 = 1440;

/// Which binary is reporting.
///
/// Both apps share one config file (see [`crate::config::DATA_ID`]) and so share one install
/// id — which is right, because the id names a machine and two apps on one machine are not
/// two machines. But the control plane's rollups key on it: without this field the two would
/// overwrite each other's version and add up each other's minutes as though one app had been
/// open twice as long. So every report says which app it is.
pub const MANAGER: &str = "manager";
pub const STUDIO: &str = "studio";
pub const COACH: &str = "coach";

/// Set once, by whichever binary called [`start`].
static APP_ID: Mutex<&'static str> = Mutex::new(MANAGER);

fn app_id() -> &'static str {
    *APP_ID.lock().unwrap()
}

/// A report, exactly as `POST /v1/usage` expects it.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub install_id: String,
    /// [`MANAGER`] or [`STUDIO`].
    pub app: String,
    pub version: String,
    pub os: String,
    pub game: String,
    pub sessions: u32,
    pub minutes: u32,
    pub events: Vec<Event>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Event {
    pub name: String,
    pub count: u32,
}

/// Start counting, and report every few minutes.
///
/// Called once from `setup`. Mints the install id on first run and saves it — a fresh id
/// every launch would turn one player into a crowd.
pub fn start(app: &AppHandle, which: &'static str) {
    *APP_ID.lock().unwrap() = which;
    let mut cfg = config::load(app).unwrap_or_default();
    if !allowed(&cfg) {
        log::info!("[usage] anonymous stats are off for this run");
        return;
    }
    if cfg.install_id.trim().is_empty() {
        cfg.install_id = mint_install_id();
        if cfg.install_id.is_empty() {
            // A build without `mint-install-id` — the studio. The id names the machine and
            // the manager is what creates it, so a studio-only install reports nothing
            // rather than inventing a second identity for the same computer.
            log::info!("[usage] no install id on this machine — not counting this run");
            return;
        }
        if let Err(e) = config::save(app, &cfg) {
            // Without a saved id every launch would look like a new install, which is worse
            // than no numbers at all — so don't count this run.
            log::warn!("[usage] couldn't save an install id ({e:#}) — not counting this run");
            return;
        }
    }

    ENABLED.store(true, Ordering::Relaxed);
    {
        let mut buffer = BUFFER.lock().unwrap();
        buffer.sessions += 1;
        buffer.open_since = Some(Instant::now());
    }
    track("app.start");
    if cfg.seen_version.trim() != app.package_info().version.to_string() && !cfg.seen_version.trim().is_empty() {
        track("app.update");
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut delay = FIRST_FLUSH;
        loop {
            tokio::time::sleep(delay).await;
            flush(&handle).await;
            delay = FLUSH_EVERY;
        }
    });
}

/// Whether this run may report at all.
pub fn allowed(cfg: &AppConfig) -> bool {
    if std::env::var(DISABLE_ENV).map(|v| v == "1").unwrap_or(false) {
        return false;
    }
    if cfg!(debug_assertions) && !std::env::var(DEV_ENV).map(|v| v == "1").unwrap_or(false) {
        return false;
    }
    cfg.analytics_enabled
}

/// Every name this project may report, as a closed list.
///
/// [`is_event_name`] is the privacy guarantee — it is what makes a path or a rider name
/// impossible to send. This is the *integrity* one, and it exists because `track_event` is not
/// a private door. Paid plugins are third-party modules mounted into the app's own window, and
/// the plugin host hands each of them a raw `invoke`, so anything the webview can count, a
/// plugin can count too. Without a closed list one could write rows of its own choosing into
/// everybody's numbers — and, worse, fill [`MAX_EVENTS`] with names it invented, after which
/// [`track`] drops every *new* real name until the next flush. A counter a third party can
/// silence is not a counter.
///
/// It is also why the cap is now unreachable in ordinary running: this list is deliberately
/// shorter than [`MAX_EVENTS`], so the "more than 64 buffered" warning means what it says
/// rather than describing a busy afternoon.
///
/// Adding a name here is the deliberate half of adding a call site. The control plane keeps the
/// same list (`control-plane/src/usage.ts`) and its `usage.test.ts` reads this file to prove the
/// two have not drifted — which is what keeps the dashboard's "Never touched" panel honest,
/// because a name missing from that list can never be reported as missing.
pub const KNOWN_EVENTS: &[&str] = &[
    // Lifecycle, from every app.
    "app.start",
    "app.update",
    // The manager's pages. One name per tab; `view.plugin` is every plugin panel together,
    // because naming each one would be unbounded cardinality.
    "view.browse",
    "view.library",
    "view.downloads",
    "view.locker",
    "view.presets",
    "view.manage",
    "view.shop",
    "view.hub",
    "view.servers",
    "view.ranked",
    "view.studio",
    "view.plugin",
    "view.settings",
    // What the manager is for.
    "mod.detail",
    "mod.install",
    "mod.download",
    "game.launch",
    "preset.apply",
    "preset.save",
    "paint.publish",
    "voice.join",
    "server.join",
    "overlay.open",
    "frostmod.install",
    "drop.import",
    // Frost's Studio: its tools, and what they make.
    "view.studio.designer",
    "view.studio.paints",
    "view.studio.rider",
    "view.studio.pose",
    "view.studio.track",
    "view.studio.diagnose",
    "view.studio.settings",
    "track.generate",
    "track.settings",
    "track.build.install",
    "paint.save",
    // MXB Coach. It shares `view.settings` above with the manager — it is the same page, and
    // the `app` column is what tells them apart. The studio counts its own as
    // `view.studio.settings`, because there it is one tool among six rather than the shell.
    "view.sessions",
    "coach.session.open",
    "coach.review",
];

/// Whether [`KNOWN_EVENTS`] holds this name.
pub fn is_known_event(name: &str) -> bool {
    KNOWN_EVENTS.contains(&name)
}

/// Count one thing.
///
/// Deliberately infallible and silent: nothing in the app should be able to fail, slow down
/// or take a different path because of a counter. A name that isn't a name is dropped here,
/// where the mistake is, rather than travelling.
pub fn track(name: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    // Before the shape check, and without a debug assertion: an unknown name is what a plugin
    // sends, so it is refused rather than treated as a bug in this codebase. A first-party typo
    // lands here too, and shows up as the name it meant to be sitting in "Never touched".
    if !is_known_event(name) {
        log::warn!("[usage] refusing to count {name:?} — not a name this app reports");
        return;
    }
    if !is_event_name(name) {
        log::warn!("[usage] refusing to count {name:?} — not an event name");
        debug_assert!(false, "not an event name: {name}");
        return;
    }
    let mut buffer = BUFFER.lock().unwrap();
    if !buffer.events.contains_key(name) && buffer.events.len() >= MAX_EVENTS {
        if !buffer.warned {
            buffer.warned = true;
            log::warn!("[usage] more than {MAX_EVENTS} distinct events buffered — dropping {name}");
        }
        return;
    }
    *buffer.events.entry(name.to_string()).or_insert(0) += 1;
}

/// Turn reporting on or off from Settings.
///
/// Turning it off empties the buffer: the point of the switch is that nothing further leaves
/// the machine, and a buffer that survived it would send one last report after the player
/// said not to.
pub fn set_enabled(app: &AppHandle, on: bool, cfg: &AppConfig) {
    if on && allowed(cfg) {
        ENABLED.store(true, Ordering::Relaxed);
        let mut buffer = BUFFER.lock().unwrap();
        if buffer.open_since.is_none() {
            buffer.open_since = Some(Instant::now());
            buffer.sessions += 1;
        }
        drop(buffer);
        log::info!("[usage] anonymous stats on");
        // A config that has never had an id gets one now rather than at the next launch.
        if cfg.install_id.trim().is_empty() {
            let mut fresh = cfg.clone();
            fresh.install_id = mint_install_id();
            let _ = config::save(app, &fresh);
        }
        return;
    }
    ENABLED.store(false, Ordering::Relaxed);
    let mut buffer = BUFFER.lock().unwrap();
    *buffer = Buffer::new();
    log::info!("[usage] anonymous stats off");
}

/// Send what has been counted, and keep it if the send fails.
///
/// Nothing is queued to disk. A report is a handful of counters whose worst case is a slightly
/// low number for one day, and a spool file would be a record of somebody's activity sitting
/// on their machine for the sake of that.
pub async fn flush(app: &AppHandle) {
    if report(app).await == Outcome::RateLimited {
        // Not a pause: reporting is done for this run. A counter is not worth being part of
        // the reason a service stays over its limit, and tomorrow's launch counts this
        // install again anyway.
        log::warn!("[usage] the endpoint is rate limiting — not reporting again this run");
        ENABLED.store(false, Ordering::Relaxed);
        BUFFER.lock().unwrap().events.clear();
    }
}

/// The header a signed report carries, matching the control plane's `SIGNATURE_HEADER`.
const SIGNATURE_HEADER: &str = "X-MXB-Usage";

/// The build key, or `None` in a build that was not given one.
///
/// `option_env!` rather than `env!`: the public repo builds without it, and must keep doing so.
/// A build with no key sends no signature, which the endpoint accepts until a deployment turns
/// [`MXB_USAGE_REQUIRE_SIGNATURE`](../../../control-plane/src/env.d.ts) on.
const BUILD_KEY: Option<&str> = option_env!("MXB_USAGE_KEY");

/// Sign a report body, when this build can.
///
/// The key is compiled into something anyone can download, so this is not authentication and
/// is not meant to be: it is the difference between "anyone with a terminal can move these
/// numbers" and "someone would have to reverse a binary first". What it actually defends is the
/// cheap case — a script, or a web page quietly posting from its visitors' addresses, where the
/// per-address cap buys nothing because every visitor brings a fresh one.
///
/// The timestamp is what bounds replay; see the endpoint for how far out it may be.
fn signature(body: &str) -> Option<String> {
    signature_with(BUILD_KEY, body)
}

/// The same, with the key passed in, so both ways of having no key can be tested from any build.
fn signature_with(key: Option<&str>, body: &str) -> Option<String> {
    // An *empty* key is the shape a misconfigured release takes — a CI secret referenced but
    // never set arrives as "" rather than as absent, and `option_env!` reports that as `Some`.
    // Signing with it would produce a MAC the endpoint refuses, so a deployment that had turned
    // the requirement on would drop every report and say nothing at all. Unset and blank mean
    // the same thing here: do not sign, and let the endpoint decide what to do about that.
    let key = key.filter(|k| !k.is_empty())?;
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    sign_with(key, seconds, body)
}

/// The header value itself, with the key and the clock passed in so it can be tested.
///
/// The construction — `v1.<seconds>.<body>`, MAC'd, rendered as lower-case hex, presented as
/// `v1 <seconds> <mac>` — is written twice, here and in `control-plane/src/usage.ts`, with
/// nothing but agreement holding the two together. Both sides test the same vector.
fn sign_with(key: &str, seconds: u64, body: &str) -> Option<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = <Hmac<Sha256>>::new_from_slice(key.as_bytes()).ok()?;
    mac.update(format!("v1.{seconds}.{body}").as_bytes());
    let hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Some(format!("v1 {seconds} {hex}"))
}

async fn report(app: &AppHandle) -> Outcome {
    if !ENABLED.load(Ordering::Relaxed) {
        return Outcome::Done;
    }
    let cfg = config::load(app).unwrap_or_default();
    // Re-read rather than trust the flag: the config can be edited by hand, and this is the
    // last point before anything leaves.
    if !allowed(&cfg) || cfg.install_id.trim().is_empty() {
        ENABLED.store(false, Ordering::Relaxed);
        return Outcome::Done;
    }

    let Some(payload) = take(&cfg, &app.package_info().version.to_string()) else {
        return Outcome::Done;
    };
    let url = format!("{}/v1/usage", control_plane());
    // Serialized once, here, rather than by `.json()`: the signature is over the exact bytes
    // that travel, so anything that re-serializes between signing and sending would produce a
    // body the endpoint checks against a MAC for a different one.
    let raw = match serde_json::to_string(&payload) {
        Ok(raw) => raw,
        Err(e) => {
            // Nothing a retry fixes — the payload is the same shape every time.
            log::warn!("[usage] couldn't serialize a report ({e}) — dropped");
            return Outcome::Done;
        }
    };
    let sent = match client() {
        Ok(client) => {
            // Signed before the body is handed over, so the bytes that were MAC'd are the bytes
            // that go — and there is no second copy of the report kept alive to manage that.
            let signed = signature(&raw);
            let mut request = client
                .post(&url)
                .header("content-type", "application/json")
                .body(raw);
            if let Some(header) = signed {
                request = request.header(SIGNATURE_HEADER, header);
            }
            request.send().await
        }
        Err(e) => {
            log::debug!("[usage] no HTTP client: {e}");
            restore(payload);
            return Outcome::Retry;
        }
    };
    match sent {
        Ok(res) if res.status().is_success() => Outcome::Done,
        Ok(res) if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
            // Either the endpoint's own per-address cap or the platform's daily one. Both
            // mean the same thing to a client: stop asking.
            Outcome::RateLimited
        }
        Ok(res) => {
            // Any other 4xx means this build is sending something the endpoint won't take;
            // retrying it forever would never work, so drop it and leave the reason.
            if res.status().is_client_error() {
                log::warn!("[usage] report refused ({}) — dropped", res.status());
                Outcome::Done
            } else {
                restore(payload);
                Outcome::Retry
            }
        }
        Err(e) => {
            log::debug!("[usage] report didn't send ({e}) — keeping it for the next try");
            restore(payload);
            Outcome::Retry
        }
    }
}

/// Empty the buffer into a report, or `None` when there is nothing worth a request.
fn take(cfg: &AppConfig, version: &str) -> Option<Report> {
    let mut buffer = BUFFER.lock().unwrap();
    let minutes = buffer.take_minutes(Instant::now());
    if buffer.events.is_empty() && buffer.sessions == 0 && minutes == 0 {
        return None;
    }
    let events = std::mem::take(&mut buffer.events)
        .into_iter()
        .map(|(name, count)| Event { name, count })
        .collect();
    let sessions = std::mem::take(&mut buffer.sessions);
    buffer.minutes = 0;
    buffer.warned = false;

    Some(Report {
        install_id: cfg.install_id.trim().to_string(),
        app: app_id().to_string(),
        version: version.to_string(),
        os: platform().to_string(),
        game: cfg.active_game.id().to_string(),
        sessions,
        minutes,
        events,
    })
}

/// Put a failed report back, folding it into whatever has been counted since.
fn restore(report: Report) {
    let mut buffer = BUFFER.lock().unwrap();
    buffer.sessions = buffer.sessions.saturating_add(report.sessions);
    buffer.minutes = buffer.minutes.saturating_add(report.minutes).min(MAX_MINUTES);
    for event in report.events {
        if !buffer.events.contains_key(&event.name) && buffer.events.len() >= MAX_EVENTS {
            continue;
        }
        *buffer.events.entry(event.name).or_insert(0) += event.count;
    }
}

/// What the endpoint calls this platform.
pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// `area.thing`, up to four segments, lower-case throughout.
///
/// The shape is the privacy guarantee, not a style rule: a path, a rider name, a mod title or
/// an address cannot survive it, so a call site that tried to pass one counts nothing instead
/// of sending it.
pub fn is_event_name(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return false;
    }
    let mut segments = name.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    let mut chars = first.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return false;
    }
    let mut count = 0;
    for segment in segments {
        count += 1;
        if count > 3 || segment.is_empty() {
            return false;
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return false;
        }
    }
    count >= 1
}

/// Send what's left on the way out, and don't hold the shutdown up over it.
///
/// Called from the paths that really are quitting — the tray's Quit and a close that isn't
/// parking in the tray. Everything else is covered by the timer.
pub fn flush_on_exit(app: &AppHandle) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::block_on(async move {
        let _ = tokio::time::timeout(EXIT_GRACE, flush(&handle)).await;
    });
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // Short on purpose: a counter is never worth holding a connection open for, and a
        // report that times out is simply retried at the next flush.
        .timeout(Duration::from_secs(15))
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_an_area_and_a_thing() {
        assert!(is_event_name("app.start"));
        assert!(is_event_name("view.studio.designer"));
        assert!(is_event_name("mod.install"));
        assert!(is_event_name("track.generate"));
    }

    #[test]
    fn nothing_identifying_can_be_a_name() {
        // The cases that matter: every one of these is something a careless call site might
        // pass, and every one of them is refused before it reaches the buffer.
        assert!(!is_event_name("mod.install:C:/Users/ryan/mods"));
        assert!(!is_event_name("rider.Ryan Sipes"));
        assert!(!is_event_name("server.203.0.113.10:54210"));
        assert!(!is_event_name("Mod.Install"));
        assert!(!is_event_name("paint.publish "));
    }

    #[test]
    fn a_name_needs_both_halves() {
        assert!(!is_event_name("start"));
        assert!(!is_event_name("app."));
        assert!(!is_event_name(".start"));
        assert!(!is_event_name(""));
        assert!(!is_event_name("a.b.c.d.e"));
        assert!(!is_event_name(&"a.".repeat(40)));
    }

    /// The buffer is process-wide, so the tests that use it run one after another under this
    /// guard rather than in parallel — two of them interleaving would count each other's
    /// events.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        *BUFFER.lock().unwrap() = Buffer::new();
        ENABLED.store(true, Ordering::Relaxed);
        guard
    }

    fn config_with_id() -> AppConfig {
        AppConfig {
            install_id: "6f1f2b6c-0f6d-4a5e-9f3a-2b7c4d5e6f70".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn repeats_add_up_rather_than_becoming_rows() {
        let _guard = fresh();
        track("view.browse");
        track("view.browse");
        track("view.settings");

        let report = take(&config_with_id(), "0.12.3").expect("something was counted");
        assert_eq!(
            report.events,
            vec![
                Event { name: "view.browse".into(), count: 2 },
                Event { name: "view.settings".into(), count: 1 },
            ]
        );
        assert_eq!(report.install_id, "6f1f2b6c-0f6d-4a5e-9f3a-2b7c4d5e6f70");
        assert_eq!(report.game, "mxb");
    }

    /// The wire contract, spelled out.
    ///
    /// Both halves of this feature are written here and in `control-plane/src/usage.ts`, and
    /// the only thing holding them together is the JSON — a renamed field would be a silent
    /// 400 in the field and nothing anywhere else.
    #[test]
    fn the_payload_is_what_the_endpoint_asks_for() {
        let _guard = fresh();
        track("view.browse");
        let mut report = take(&config_with_id(), "0.12.3").unwrap();
        report.minutes = 7;
        report.sessions = 1;

        assert_eq!(
            serde_json::to_string(&report).unwrap(),
            r#"{"installId":"6f1f2b6c-0f6d-4a5e-9f3a-2b7c4d5e6f70","app":"manager","version":"0.12.3","os":"PLATFORM","game":"mxb","sessions":1,"minutes":7,"events":[{"name":"view.browse","count":1}]}"#
                .replace("PLATFORM", platform())
        );
    }

    #[test]
    fn counting_nothing_makes_no_request() {
        let _guard = fresh();
        assert!(take(&config_with_id(), "0.12.3").is_none());
    }

    #[test]
    fn a_disabled_run_buffers_nothing() {
        let _guard = fresh();
        ENABLED.store(false, Ordering::Relaxed);
        track("view.browse");
        ENABLED.store(true, Ordering::Relaxed);

        assert!(take(&config_with_id(), "0.12.3").is_none());
    }

    #[test]
    fn a_failed_report_is_folded_back_in() {
        let _guard = fresh();
        track("view.browse");
        let report = take(&config_with_id(), "0.12.3").unwrap();
        restore(report);
        track("view.browse");

        let again = take(&config_with_id(), "0.12.3").unwrap();
        assert_eq!(again.events, vec![Event { name: "view.browse".into(), count: 2 }]);
    }

    /// The plugin case, which is the reason [`KNOWN_EVENTS`] exists.
    ///
    /// A third-party module holds the same `invoke` the app's own webview does, so this is the
    /// only thing standing between somebody else's code and everybody's numbers.
    #[test]
    fn a_name_this_app_does_not_report_is_refused() {
        let _guard = fresh();
        track("view.browse");
        // Well-formed, plausible, and not ours.
        track("replaycam.export");
        track("view.tab7");

        let report = take(&config_with_id(), "0.12.3").expect("something was counted");
        assert_eq!(report.events, vec![Event { name: "view.browse".into(), count: 1 }]);
    }

    /// ...which is also what makes the buffer cap unreachable by anything but a bug.
    ///
    /// [`track`] drops *new* names once the buffer is full, so a vocabulary that could fill it
    /// would let a busy session silence the names that had not been counted yet.
    #[test]
    fn the_vocabulary_cannot_fill_the_buffer() {
        assert!(
            KNOWN_EVENTS.len() < MAX_EVENTS,
            "{} names against a cap of {MAX_EVENTS}",
            KNOWN_EVENTS.len()
        );
    }

    #[test]
    fn every_name_this_app_reports_is_a_name() {
        for name in KNOWN_EVENTS {
            assert!(is_event_name(name), "{name} is in the list but is not an event name");
        }
        let mut sorted = KNOWN_EVENTS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), KNOWN_EVENTS.len(), "a name is in the list twice");
    }

    /// The cap itself still holds where it can still be reached: a report that failed and is
    /// being folded back in comes from the wire, not from [`track`].
    #[test]
    fn a_restored_report_cannot_overflow_the_buffer() {
        let _guard = fresh();
        let events = (0..MAX_EVENTS + 20)
            .map(|i| Event { name: format!("view.tab{i}"), count: 1 })
            .collect();
        restore(Report {
            install_id: String::new(),
            app: MANAGER.to_string(),
            version: String::new(),
            os: String::new(),
            game: String::new(),
            sessions: 0,
            minutes: 0,
            events,
        });
        assert_eq!(BUFFER.lock().unwrap().events.len(), MAX_EVENTS);
    }

    #[test]
    fn minutes_count_whole_ones_and_carry_the_rest() {
        let mut buffer = Buffer::new();
        let start = Instant::now();
        buffer.open_since = Some(start);

        // Ninety seconds is one whole minute; the thirty left over are carried, not lost.
        assert_eq!(buffer.take_minutes(start + Duration::from_secs(90)), 1);
        assert_eq!(buffer.take_minutes(start + Duration::from_secs(110)), 1);
        // ...so the second minute lands at 120, not at 150.
        assert_eq!(buffer.take_minutes(start + Duration::from_secs(120)), 2);

        // What a successful report leaves behind: the clock keeps running, the total resets.
        buffer.minutes = 0;
        assert_eq!(buffer.take_minutes(start + Duration::from_secs(179)), 0);
        assert_eq!(buffer.take_minutes(start + Duration::from_secs(181)), 1);
    }

    #[test]
    fn a_machine_that_slept_reports_a_day_at_most() {
        let mut buffer = Buffer::new();
        let start = Instant::now();
        buffer.open_since = Some(start);
        assert_eq!(buffer.take_minutes(start + Duration::from_secs(86_400 * 5)), MAX_MINUTES);
    }

    /// The vector `control-plane/src/usage.test.ts` also checks.
    ///
    /// Two implementations of one construction, in two languages, joined by nothing but this
    /// number. A build whose signature the endpoint rejects reports nothing at all once a
    /// deployment requires one, and the only sign of it would be numbers quietly going flat.
    #[test]
    fn a_signature_is_built_the_way_the_endpoint_rebuilds_it() {
        assert_eq!(
            sign_with("a-build-key", 1_700_000_000, r#"{"installId":"x"}"#).unwrap(),
            "v1 1700000000 78626f503fcdc861e76c223f96c36373bdd848c8e9aeacca28ee27d41e40db86",
        );
    }

    /// Both ways of having no key, and the one way of having one.
    ///
    /// The blank case is the one worth a test: a CI secret referenced but never set arrives as
    /// `Some("")`, the MAC construction would happily use it, and the endpoint would refuse
    /// every report from that build — silently, if the deployment had turned the requirement on.
    #[test]
    fn nothing_is_signed_without_a_key_worth_the_name() {
        assert!(signature_with(None, "{}").is_none());
        assert!(signature_with(Some(""), "{}").is_none());
        assert!(signature_with(Some("a-build-key"), "{}").is_some());
    }

    #[test]
    fn the_env_switch_wins_over_the_setting() {
        // Environment variables are process-wide, so the two tests that set them take the
        // same guard as the buffer tests rather than racing each other.
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = AppConfig { analytics_enabled: true, ..Default::default() };
        // Only meaningful in a release build; a debug build is already silent, which the
        // second assertion is what checks.
        std::env::set_var(DISABLE_ENV, "1");
        assert!(!allowed(&cfg));
        std::env::remove_var(DISABLE_ENV);
        assert_eq!(allowed(&cfg), !cfg!(debug_assertions));
    }

    #[test]
    fn the_setting_is_the_last_word() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = AppConfig { analytics_enabled: false, ..Default::default() };
        std::env::set_var(DEV_ENV, "1");
        assert!(!allowed(&cfg));
        std::env::remove_var(DEV_ENV);
    }
}
