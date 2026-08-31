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

use crate::config::{self, AppConfig};
use crate::paintsync::control_plane;
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

/// How often the buffer is emptied. Long enough that a session is a handful of requests,
/// short enough that a crash loses minutes rather than an evening.
const FLUSH_EVERY: Duration = Duration::from_secs(5 * 60);

/// ...but the first report goes out a minute in.
///
/// Otherwise the app would only ever count sessions that lasted longer than a flush, and
/// "opened it, looked at one thing, closed it" — which is a great many of them — would be
/// invisible. The daily active number is exactly the number that bias would ruin.
const FIRST_FLUSH: Duration = Duration::from_secs(60);

/// How long a quit waits for the last report. Short: nobody's shutdown should be held up by
/// a counter, and the next launch would carry the loss anyway.
const EXIT_GRACE: Duration = Duration::from_secs(3);

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
                // carried rather than rounded away every five minutes.
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

/// A report, exactly as `POST /v1/usage` expects it.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub install_id: String,
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
pub fn start(app: &AppHandle) {
    let mut cfg = config::load(app).unwrap_or_default();
    if !allowed(&cfg) {
        log::info!("[usage] anonymous stats are off for this run");
        return;
    }
    if cfg.install_id.trim().is_empty() {
        cfg.install_id = uuid::Uuid::new_v4().to_string();
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

/// Count one thing.
///
/// Deliberately infallible and silent: nothing in the app should be able to fail, slow down
/// or take a different path because of a counter. A name that isn't a name is dropped here,
/// where the mistake is, rather than travelling.
pub fn track(name: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
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
            fresh.install_id = uuid::Uuid::new_v4().to_string();
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
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let cfg = config::load(app).unwrap_or_default();
    // Re-read rather than trust the flag: the config can be edited by hand, and this is the
    // last point before anything leaves.
    if !allowed(&cfg) || cfg.install_id.trim().is_empty() {
        ENABLED.store(false, Ordering::Relaxed);
        return;
    }

    let Some(report) = take(&cfg, &app.package_info().version.to_string()) else {
        return;
    };
    let url = format!("{}/v1/usage", control_plane());
    let sent = match client() {
        Ok(client) => client.post(&url).json(&report).send().await,
        Err(e) => {
            log::debug!("[usage] no HTTP client: {e}");
            restore(report);
            return;
        }
    };
    match sent {
        Ok(res) if res.status().is_success() => {}
        Ok(res) => {
            // A 4xx means this build is sending something the endpoint won't take; retrying
            // it forever would never work, so drop it and leave the reason in the log.
            if res.status().is_client_error() {
                log::warn!("[usage] report refused ({}) — dropped", res.status());
            } else {
                restore(report);
            }
        }
        Err(e) => {
            log::debug!("[usage] report didn't send ({e}) — keeping it for the next try");
            restore(report);
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
        // report that times out is simply retried in five minutes.
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
            r#"{"installId":"6f1f2b6c-0f6d-4a5e-9f3a-2b7c4d5e6f70","version":"0.12.3","os":"PLATFORM","game":"mxb","sessions":1,"minutes":7,"events":[{"name":"view.browse","count":1}]}"#
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

    #[test]
    fn a_call_site_generating_names_is_capped() {
        let _guard = fresh();
        for i in 0..MAX_EVENTS + 20 {
            track(&format!("view.tab{i}"));
        }
        let report = take(&config_with_id(), "0.12.3").unwrap();
        assert_eq!(report.events.len(), MAX_EVENTS);
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
