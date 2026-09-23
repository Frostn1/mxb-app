//! Keeping the server list warm while nobody is looking at it.
//!
//! A sweep is a Steam sign-in, a master login and a datagram to every server that answers, and
//! until all of it lands the Servers tab has nothing of its own on it. Doing that work only
//! when somebody opens the tab meant the wait was always paid by the person waiting, and it
//! meant an install that never opened the tab contributed nothing — to the shared address book
//! that makes a fresh install's browser work, to the snapshot it paints while its own sweep
//! runs, or to the count that answers "is the master down or is it me".
//!
//! So the app sweeps on a beat while Online is visible, feeds what it finds to the control
//! plane, and the tab takes whatever the last one found. That pooled list is also the answer for
//! an app that cannot sweep at all — a machine with no MX Bikes on it, or a build without the
//! browser in it — which used to be an error message where the list should be and is now
//! somebody else's sweep from a minute ago, with its age on it.
//!
//! Three things make the beat affordable:
//!
//! - **While MX Bikes is running the master is never asked.** `worldnet::list_servers`
//!   already rebuilds the list by asking each remembered server about itself, because the
//!   Steam account the master login spends is the one the game is holding. A beat during a
//!   session is a handful of `GETINFO` datagrams and no account.
//! - **Nothing sweeps while nobody is there.** Leaving Online, minimising, or parking the main
//!   window pauses discovery. Returning to Online wakes it immediately instead of inheriting an
//!   idle sleep.
//! - **What a sweep *tells* the control plane is floored here, not on the beat.** The list is
//!   refreshed every couple of minutes; the addresses, the snapshot and the status report go at
//!   their own much slower rates, so running this everywhere costs the control plane about what
//!   the tab already cost it. See [`Told`].

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::masterstatus::{self, MasterOutcome};
use crate::{roster, serverbook, WorldServer};

/// How often the background sweep runs. Rider counts and sessions are what move between two of
/// these, and a couple of minutes is about how long a row stays true on a busy evening.
const BEAT: Duration = Duration::from_secs(120);

/// How long the first sweep waits after startup. Everything else the app does on launch —
/// scanning mods, reading the profile, seeding the book — matters more to somebody who just
/// opened it than a list they haven't asked for yet.
const SETTLE: Duration = Duration::from_secs(20);

/// A bounded sleep while discovery is paused. Navigation notifications normally wake it first;
/// the timeout is only a safety net if a frontend disappears without sending another update.
const IDLE_BEAT: Duration = Duration::from_secs(10 * 60);

/// The frontend starts on Online and reports every later navigation. The native watcher uses
/// this instead of treating any visible app window as a reason to sweep the network.
static SERVER_BROWSER_ACTIVE: AtomicBool = AtomicBool::new(true);
static ACTIVITY_CHANGED: Notify = Notify::const_new();

/// The widest the beat gets while sweeps keep failing. An outage is the one time every install
/// is failing at once, and it is the worst time for all of them to be retrying on the fast beat.
const MAX_BEAT: Duration = Duration::from_secs(10 * 60);

/// How fresh the last sweep has to be for the tab to take it as its own answer rather than
/// running one. Shorter than [`BEAT`], so a tab opened late in a beat still gets a sweep of its
/// own rather than a list about to be replaced.
const FRESH: Duration = Duration::from_secs(75);

/// The event an open Servers tab listens on, so a beat that lands while somebody is reading the
/// list updates it in place.
pub const EVENT: &str = "servers-swept";

/// How often a sweep may report to `masterstatus`. Half its ten-minute window, so every open
/// app is counted in every window without reporting several times into the same one.
const REPORT_EVERY: Duration = Duration::from_secs(5 * 60);

/// How often a sweep may offer the shared snapshot.
///
/// The pooled list is only as current as the last app that fed it, and apps that cannot sweep
/// have nothing else to look at — so this is deliberately close to the control plane's own
/// minute-wide gap rather than well above it. Anything offered inside that gap is turned down
/// cheaply, before the expensive half of the endpoint runs.
const SNAPSHOT_EVERY: Duration = Duration::from_secs(90);

/// How often a sweep may contribute addresses when the set of them hasn't changed. A changed
/// set always goes at once — that is a server that appeared or went away.
const ADDRESSES_EVERY: Duration = Duration::from_secs(15 * 60);

/// A list to draw, and the moment it was true.
///
/// `as_of` and `source` are the whole contract. A list is either this app's own sweep, seconds
/// old, or the pooled one somebody else's app read a minute ago — and the tab says which rather
/// than passing the second off as the first.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedServers {
    pub servers: Vec<WorldServer>,
    /// Milliseconds since the epoch. Zero when there was nothing to give.
    pub as_of: u64,
    /// `""` for this app's own sweep, `"local"` for its last one out of the book, `"shared"`
    /// for the pooled snapshot. The tab draws an age beside the two that aren't live.
    pub source: String,
}

/// Where a list came from, in the vocabulary the tab reads.
pub const SOURCE_LIVE: &str = "";
pub const SOURCE_LOCAL: &str = "local";
pub const SOURCE_SHARED: &str = "shared";

/// The last list, and when this app got hold of it.
struct Warm {
    at: Instant,
    list: CachedServers,
}

static WARM: Mutex<Option<Warm>> = Mutex::new(None);

/// What has already been said to the control plane, so a beat doesn't say it again.
///
/// The floors live here rather than in [`roster`] or [`masterstatus`] because they are about
/// *this* caller: a sweep somebody asked for by pressing Refresh is the same call, and the
/// reason not to send is the same reason either way.
#[derive(Default)]
struct Told {
    /// Whether the last report that actually went said "ok", and when it went.
    report: Option<(bool, Instant)>,
    /// When a snapshot was last offered.
    snapshot: Option<Instant>,
    /// The addresses last contributed, and when.
    addresses: Option<(BTreeSet<String>, Instant)>,
}

static TOLD: Mutex<Option<Told>> = Mutex::new(None);

/// One sweep at a time, app-wide.
///
/// Two overlapping sweeps each sign in to Steam and each log in to the master, and the master
/// answers the second one no better for it. The tab held this as a ref of its own, which
/// couldn't see the beat; holding it here means the beat and the tab take turns, and a tab that
/// opens mid-beat waits for that sweep and then finds it warm rather than starting a second.
fn gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// The master-server list, or the reason there isn't one.
///
/// All the work lives in the local-only `worldnet` module. Without it — the public tree, or a
/// build that never had the file — the tab still exists but says the browser isn't included,
/// rather than failing opaquely.
pub async fn master_list(app: AppHandle) -> Result<(Vec<WorldServer>, MasterOutcome), String> {
    #[cfg(worldnet)]
    {
        crate::worldnet::list_servers(app).await
    }
    #[cfg(not(worldnet))]
    {
        let _ = app;
        Err("The server browser isn't included in this build.".into())
    }
}

/// What the tab asks for: the last list when it is still true, and a fresh one otherwise.
pub async fn list(app: AppHandle) -> Result<CachedServers, String> {
    if let Some(list) = warm() {
        return Ok(list);
    }
    refresh(app).await
}

/// The last list, if it is fresh enough to answer with — without asking for a new one.
///
/// What the instant-paint command takes: it runs while the tab is drawing, so it must never be
/// the thing that starts a sweep.
pub fn warm_list() -> Option<CachedServers> {
    warm()
}

/// The last list, if it is fresh enough to answer with.
fn warm() -> Option<CachedServers> {
    let held = WARM.lock().unwrap_or_else(|p| p.into_inner());
    let warm = held.as_ref()?;
    (warm.at.elapsed() < FRESH).then(|| warm.list.clone())
}

/// A sweep of our own, and the pooled list when there can't be one.
///
/// The fallback is what makes the tab work on a machine that has never run MX Bikes: no Steam,
/// no ticket, no master login, and — until now — no list either. It is somebody else's sweep,
/// so it arrives labelled `shared` with the moment it was true, and the tab draws that age
/// rather than passing it off as live.
///
/// The failure is still handed back when the pool has nothing either. The tab has a connection
/// check to offer under that error, and it is the one moment worth being blunt about.
pub async fn refresh(app: AppHandle) -> Result<CachedServers, String> {
    match got(app).await {
        Got::Mine(list) | Got::Shared(list, _) => Ok(list),
        Got::Nothing(why) => Err(why),
    }
}

/// What a refresh came back with, and — when it isn't ours — what stopped it being ours.
///
/// The beat needs that reason and the tab doesn't: a master that was asked and didn't answer is
/// worth backing off from, and a build that was never able to ask isn't.
enum Got {
    Mine(CachedServers),
    Shared(CachedServers, String),
    Nothing(String),
}

async fn got(app: AppHandle) -> Got {
    let failed = match sweep(app.clone()).await {
        Ok(list) => return Got::Mine(list),
        Err(e) => e,
    };
    match roster::snapshot().await {
        Some((servers, as_of)) => {
            let list = CachedServers { servers, as_of, source: SOURCE_SHARED.into() };
            remember(&app, list.clone());
            Got::Shared(list, failed)
        }
        None => Got::Nothing(failed),
    }
}

/// Sweep now: read the list, keep it, tell the control plane what the floors allow, and wake
/// whoever is looking at the tab.
pub async fn sweep(app: AppHandle) -> Result<CachedServers, String> {
    // Held across the whole sweep, so a tab opening mid-beat waits rather than racing it. The
    // freshness check runs again inside the gate: by the time a waiter gets in, the sweep it
    // was waiting on has usually answered its question.
    let _held = gate().lock().await;
    if let Some(list) = warm() {
        return Ok(list);
    }

    let mut out = master_list(app.clone()).await;

    // A fresh install has an empty address book, so the fallback the rest of this depends on
    // has nothing to fall back to — which makes it useless to precisely the people an outage
    // hits hardest, the ones who never got to open this tab on a good day. Fill it from the
    // shared book and ask again. Only ever on an empty book, so this is once in an install's
    // life and nobody pays the second attempt twice.
    if out.is_err() && serverbook::load(&app).is_empty() && roster::seed(&app).await > 0 {
        out = master_list(app.clone()).await;
    }

    match &out {
        // The outcome, never the list: a list rebuilt from the book is what a *failed* master
        // looks like from here, and reporting it as an answer would have every install with a
        // warm book calling an outage `ok`. See `masterstatus::MasterOutcome`.
        Ok((list, outcome)) => {
            report(&app, outcome);
            // Only a real sweep is worth contributing. A list rebuilt from our own book would
            // corroborate the shared book using the copies it handed out — see `roster`.
            if *outcome == MasterOutcome::Answered {
                contribute(list);
            }
        }
        Err(e) => report(&app, &MasterOutcome::Failed(e.clone())),
    }

    out.map(|(servers, _)| {
        let list = CachedServers {
            servers,
            as_of: serverbook::now_millis(),
            source: SOURCE_LIVE.into(),
        };
        remember(&app, list.clone());
        list
    })
}

/// Keep the list for the next asker, and hand it to a tab that is already open.
///
/// Emitted whatever the outcome was: a list rebuilt from the book with `GETINFO` is as true as
/// one from the master, and during a session it is the only kind there is.
fn remember(app: &AppHandle, list: CachedServers) {
    if let Err(e) = app.emit(EVENT, &list) {
        log::debug!("[serverwatch] couldn't announce the sweep: {e}");
    }
    let mut held = WARM.lock().unwrap_or_else(|p| p.into_inner());
    *held = Some(Warm { at: Instant::now(), list });
}

/// Report to `masterstatus`, but not on every beat.
///
/// A changed answer always goes at once — the whole point of the count is how many installs are
/// failing *now*, and an install that has just started failing is the news. An unchanged one
/// goes at [`REPORT_EVERY`], which keeps every open app inside every ten-minute window without
/// filling it with the same install several times over.
fn report(app: &AppHandle, outcome: &MasterOutcome) {
    let ok = match masterstatus::what_to_say(outcome) {
        // Nothing to say, and nothing to remember saying: a session with the game open must not
        // leave a stale "last report" that silences the first real failure after it.
        masterstatus::Say::Nothing => return,
        masterstatus::Say::Answered => true,
        masterstatus::Say::Failed(_) => false,
    };

    let now = Instant::now();
    {
        let mut held = TOLD.lock().unwrap_or_else(|p| p.into_inner());
        let told = held.get_or_insert_with(Told::default);
        if !report_due(told.report, ok, now) {
            return;
        }
        told.report = Some((ok, now));
    }
    masterstatus::report(app, outcome);
}

/// Whether this answer is worth a report: a changed one always, an unchanged one on the floor.
fn report_due(last: Option<(bool, Instant)>, ok: bool, now: Instant) -> bool {
    match last {
        Some((was, at)) => was != ok || now.saturating_duration_since(at) >= REPORT_EVERY,
        None => true,
    }
}

/// Contribute to the shared book and the shared snapshot, at their own rates.
///
/// Addresses barely move — a list of the same servers as ten minutes ago teaches the control
/// plane nothing it can act on — so an unchanged set waits. The snapshot is the live half and
/// does move, but the control plane keeps only the latest one and refuses another inside a
/// minute, so offering it faster than that spends a request to be turned down.
fn contribute(list: &[WorldServer]) {
    let now = Instant::now();
    let addresses: BTreeSet<String> = list
        .iter()
        .filter(|s| s.joinable && !s.address.trim().is_empty())
        .map(|s| s.address.trim().to_string())
        .collect();

    let (send_addresses, send_snapshot) = {
        let mut held = TOLD.lock().unwrap_or_else(|p| p.into_inner());
        let told = held.get_or_insert_with(Told::default);

        let send_addresses = addresses_due(told.addresses.as_ref(), &addresses, now);
        if send_addresses {
            told.addresses = Some((addresses, now));
        }

        let send_snapshot =
            told.snapshot.is_none_or(|at| now.saturating_duration_since(at) >= SNAPSHOT_EVERY);
        if send_snapshot {
            told.snapshot = Some(now);
        }
        (send_addresses, send_snapshot)
    };

    if send_addresses {
        roster::contribute(list);
    }
    if send_snapshot {
        // The same list with its live half attached, for whoever opens the tab next with
        // nothing of their own to paint.
        roster::contribute_snapshot(list);
    }
}

/// Whether the addresses are worth contributing: a set that changed always, an unchanged one
/// on the floor. A server that appeared or went away is the only thing the shared book can act
/// on, so it never waits.
fn addresses_due(
    last: Option<&(BTreeSet<String>, Instant)>,
    next: &BTreeSet<String>,
    now: Instant,
) -> bool {
    match last {
        Some((was, at)) => was != next || now.saturating_duration_since(*at) >= ADDRESSES_EVERY,
        None => true,
    }
}

/// The beat to use after a sweep that didn't come back with a list of our own.
///
/// A build with no browser in it never put a packet on the wire, so widening after one protects
/// nothing and only makes the pooled list it falls back to staler — it keeps the fast beat.
/// Every other failure is a master that was asked and didn't answer, and an outage is the one
/// time every install is failing at once, so those widen.
fn widened(backoff: Duration, failure: &str) -> Duration {
    if masterstatus::classify(failure) == masterstatus::Reason::Unsupported {
        BEAT
    } else {
        (backoff * 2).min(MAX_BEAT)
    }
}

/// Whether anybody could be looking at the app./// Whether anybody could be looking at the app.
///
/// The main window parks in the tray on close rather than ending the app, so "open" covers a
/// process that has had no window on screen since breakfast. A hidden or minimised window is
/// nobody watching; anything else — including a window behind the game — is.
///
/// Missing windows read as watching: the app is starting, or this is a build that doesn't park,
/// and the fast beat is the safer of the two guesses.
fn watching(app: &AppHandle) -> bool {
    use tauri::Manager;
    let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) else {
        return true;
    };
    let visible = window.is_visible().unwrap_or(true);
    let minimised = window.is_minimized().unwrap_or(false);
    visible && !minimised
}

fn should_sweep(window_visible: bool, server_browser_active: bool) -> bool {
    window_visible && server_browser_active
}

/// Change whether the Online screen is visible and wake the watcher so returning to it does
/// not inherit the ten-minute idle sleep used by every other tab.
pub fn set_active(active: bool) {
    if SERVER_BROWSER_ACTIVE.swap(active, Ordering::Relaxed) != active {
        ACTIVITY_CHANGED.notify_one();
    }
}

/// Start sweeping whenever Online is the visible screen.
///
/// Other tabs and a parked/minimised window wait without touching the master. A return to Online
/// wakes that wait immediately.
///
/// Failures widen it further rather than stopping it. The common failure is the master being down,
/// which is exactly when every install is failing at once, and thousands of apps retrying on the
/// fast beat is the one thing this could do that the tab never did.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        let mut backoff = BEAT;
        loop {
            let active = should_sweep(
                watching(&app),
                SERVER_BROWSER_ACTIVE.load(Ordering::Relaxed),
            );
            if !active {
                let _ = tokio::time::timeout(IDLE_BEAT, ACTIVITY_CHANGED.notified()).await;
                continue;
            }
            // The backoff follows our own sweep, not what ended up on screen: a beat that
            // finished on the pooled list is still a beat whose master didn't answer.
            match got(app.clone()).await {
                Got::Mine(list) => {
                    log::info!("[serverwatch] swept {} server(s)", list.servers.len());
                    backoff = BEAT;
                }
                Got::Shared(list, why) => {
                    log::info!(
                        "[serverwatch] {} server(s) from the shared snapshot ({why})",
                        list.servers.len()
                    );
                    backoff = widened(backoff, &why);
                }
                Got::Nothing(why) => {
                    log::debug!("[serverwatch] no list: {why}");
                    backoff = widened(backoff, &why);
                }
            }
            let _ = tokio::time::timeout(BEAT.max(backoff), ACTIVITY_CHANGED.notified()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeps_only_for_a_visible_online_tab() {
        assert!(should_sweep(true, true));
        assert!(!should_sweep(true, false));
        assert!(!should_sweep(false, true));
    }

    fn one_server() -> CachedServers {
        CachedServers {
            servers: vec![WorldServer {
                address: "1.2.3.4:54200".into(),
                joinable: true,
                ..Default::default()
            }],
            as_of: 1,
            source: SOURCE_LIVE.into(),
        }
    }

    /// The tab and the beat share one warm list, and it ages out.
    #[test]
    fn warm_is_only_offered_while_it_is_fresh() {
        let mut held = WARM.lock().unwrap();
        *held = Some(Warm { at: Instant::now(), list: one_server() });
        drop(held);
        assert_eq!(warm().map(|l| l.servers.len()), Some(1));

        let stale = Instant::now()
            .checked_sub(FRESH + Duration::from_secs(1))
            .expect("the clock has been up longer than FRESH");
        let mut held = WARM.lock().unwrap();
        *held = Some(Warm { at: stale, list: one_server() });
        drop(held);
        assert!(warm().is_none(), "a sweep older than FRESH is not an answer");

        *WARM.lock().unwrap() = None;
    }

    fn ago(d: Duration) -> Instant {
        Instant::now().checked_sub(d).expect("the clock has been up longer than that")
    }

    /// An install that has just started failing is the news the count exists for, so a changed
    /// answer never waits for the floor.
    #[test]
    fn a_changed_answer_is_reported_at_once() {
        let now = Instant::now();
        assert!(report_due(None, true, now), "the first sweep of a run always reports");
        assert!(report_due(Some((true, ago(Duration::from_secs(5)))), false, now));
        assert!(report_due(Some((false, ago(Duration::from_secs(5)))), true, now));
    }

    /// The same answer on every beat would put one install into a ten-minute window five times.
    #[test]
    fn the_same_answer_waits_for_the_floor() {
        let now = Instant::now();
        assert!(!report_due(Some((true, ago(Duration::from_secs(60)))), true, now));
        assert!(report_due(Some((true, ago(REPORT_EVERY + Duration::from_secs(1)))), true, now));
    }

    /// A server that appeared or went away is the only thing the shared book can act on.
    #[test]
    fn changed_addresses_go_at_once_and_unchanged_ones_wait() {
        let now = Instant::now();
        let one: BTreeSet<String> = ["1.2.3.4:54200".to_string()].into();
        let two: BTreeSet<String> =
            ["1.2.3.4:54200".to_string(), "5.6.7.8:54200".to_string()].into();

        assert!(addresses_due(None, &one, now));
        assert!(addresses_due(Some(&(one.clone(), ago(Duration::from_secs(30)))), &two, now));
        assert!(!addresses_due(Some(&(one.clone(), ago(Duration::from_secs(30)))), &one, now));
        assert!(addresses_due(
            Some(&(one.clone(), ago(ADDRESSES_EVERY + Duration::from_secs(1)))),
            &one,
            now
        ));
    }

    /// A master that keeps not answering is asked less and less often, but never unboundedly so.
    #[test]
    fn a_failing_master_widens_the_beat_to_a_limit() {
        let mut wait = BEAT;
        for _ in 0..10 {
            wait = widened(wait, "the master didn't answer the login");
        }
        assert_eq!(wait, MAX_BEAT);
        assert!(MAX_BEAT > BEAT);
    }

    /// A build with no browser in it never asked anyone anything, so there is nothing to be
    /// gentle with — and widening would only stale the pooled list it falls back to.
    #[test]
    fn a_build_without_the_browser_keeps_the_fast_beat() {
        let widened_once = widened(BEAT, "The server browser isn't included in this build.");
        assert_eq!(widened_once, BEAT);
        assert_eq!(widened(MAX_BEAT, "The server browser isn't included in this build."), BEAT);
    }
}
