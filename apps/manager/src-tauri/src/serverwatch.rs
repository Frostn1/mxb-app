//! Keeping the server list warm while nobody is looking at it.
//!
//! A sweep is a Steam sign-in, a master login and a datagram to every server that answers, and
//! until all of it lands the Servers tab has nothing of its own on it. Doing that work only
//! when somebody opens the tab meant the wait was always paid by the person waiting, and it
//! meant an install that never opened the tab contributed nothing — to the shared address book
//! that makes a fresh install's browser work, to the snapshot it paints while its own sweep
//! runs, or to the count that answers "is the master down or is it me".
//!
//! So the app sweeps on a beat for as long as it is open, and the tab takes whatever the last
//! one found. Two things make that affordable:
//!
//! - **While MX Bikes is running the master is never asked.** `worldnet::list_servers`
//!   already rebuilds the list by asking each remembered server about itself, because the
//!   Steam account the master login spends is the one the game is holding. A beat during a
//!   session is a handful of `GETINFO` datagrams and no account.
//! - **Nothing sweeps hard while nobody is there.** The main window parks in the tray rather
//!   than ending the app, and a tray-parked app drops to [`IDLE_BEAT`] — enough to keep feeding
//!   the shared book and the outage count, without reading the master every couple of minutes
//!   for a list nobody is looking at.
//! - **What a sweep *tells* the control plane is floored here, not on the beat.** The list is
//!   refreshed every couple of minutes; the addresses, the snapshot and the status report go at
//!   their own much slower rates, so running this everywhere costs the control plane about what
//!   the tab already cost it. See [`Told`].

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::masterstatus::{self, MasterOutcome};
use crate::{roster, serverbook, WorldServer};

/// How often the background sweep runs. Rider counts and sessions are what move between two of
/// these, and a couple of minutes is about how long a row stays true on a busy evening.
const BEAT: Duration = Duration::from_secs(120);

/// How long the first sweep waits after startup. Everything else the app does on launch —
/// scanning mods, reading the profile, seeding the book — matters more to somebody who just
/// opened it than a list they haven't asked for yet.
const SETTLE: Duration = Duration::from_secs(20);

/// The beat while the app is parked in the tray with no window on screen.
///
/// Nobody is reading a list then, so the only thing a sweep is still good for is the shared
/// book, the snapshot and the outage count — and all three have floors of their own measured in
/// minutes. Reading the master also spends a moment of the player's Steam account, which is
/// worth doing six times an hour for somebody looking at the app and not for somebody who left
/// it in the tray this morning.
const IDLE_BEAT: Duration = Duration::from_secs(10 * 60);

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

/// How often a sweep may offer the shared snapshot. The control plane keeps one snapshot and
/// refuses another inside a minute, so a faster offer is a request spent to be turned down.
const SNAPSHOT_EVERY: Duration = Duration::from_secs(5 * 60);

/// How often a sweep may contribute addresses when the set of them hasn't changed. A changed
/// set always goes at once — that is a server that appeared or went away.
const ADDRESSES_EVERY: Duration = Duration::from_secs(15 * 60);

/// The last sweep, and when it landed.
struct Warm {
    at: Instant,
    list: Vec<WorldServer>,
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

/// What the tab asks for: the last sweep when it is still true, and a fresh one otherwise.
///
/// A failure is handed back as a failure even with a warm list in hand. The tab has its own
/// remembered list to paint and a connection check to offer under the error, and quietly
/// serving a list from three minutes ago would take both away from somebody whose network has
/// just gone — which is the one moment the tab is trying to be honest about.
pub async fn list(app: AppHandle) -> Result<Vec<WorldServer>, String> {
    if let Some(list) = warm() {
        return Ok(list);
    }
    sweep(app).await
}

/// The last sweep, if it is fresh enough to answer with.
fn warm() -> Option<Vec<WorldServer>> {
    let held = WARM.lock().unwrap_or_else(|p| p.into_inner());
    let warm = held.as_ref()?;
    (warm.at.elapsed() < FRESH).then(|| warm.list.clone())
}

/// Sweep now: read the list, keep it, tell the control plane what the floors allow, and wake
/// whoever is looking at the tab.
pub async fn sweep(app: AppHandle) -> Result<Vec<WorldServer>, String> {
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
            remember(&app, list);
        }
        Err(e) => report(&app, &MasterOutcome::Failed(e.clone())),
    }

    out.map(|(list, _)| list)
}

/// Keep the list for the next asker, and hand it to a tab that is already open.
///
/// Emitted whatever the outcome was: a list rebuilt from the book with `GETINFO` is as true as
/// one from the master, and during a session it is the only kind there is.
fn remember(app: &AppHandle, list: &[WorldServer]) {
    {
        let mut held = WARM.lock().unwrap_or_else(|p| p.into_inner());
        *held = Some(Warm { at: Instant::now(), list: list.to_vec() });
    }
    if let Err(e) = app.emit(EVENT, list) {
        log::debug!("[serverwatch] couldn't announce the sweep: {e}");
    }
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

/// Whether anybody could be looking at the app.
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

/// Start sweeping, for as long as the app is open.
///
/// The beat widens to [`IDLE_BEAT`] whenever the app is parked in the tray: nobody is reading a
/// list then, and a sweep of the master spends a moment of the player's Steam account.
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
            let wait = match sweep(app.clone()).await {
                Ok(list) => {
                    log::info!("[serverwatch] swept: {} server(s)", list.len());
                    backoff = BEAT;
                    if watching(&app) { BEAT } else { IDLE_BEAT }
                }
                Err(e) => {
                    log::debug!("[serverwatch] sweep failed: {e}");
                    backoff = (backoff * 2).min(MAX_BEAT);
                    backoff.max(if watching(&app) { BEAT } else { IDLE_BEAT })
                }
            };
            tokio::time::sleep(wait).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(address: &str) -> WorldServer {
        WorldServer { address: address.into(), joinable: true, ..Default::default() }
    }

    /// The tab and the beat share one warm list, and it ages out.
    #[test]
    fn warm_is_only_offered_while_it_is_fresh() {
        let mut held = WARM.lock().unwrap();
        *held = Some(Warm { at: Instant::now(), list: vec![server("1.2.3.4:54200")] });
        drop(held);
        assert_eq!(warm().map(|l| l.len()), Some(1));

        let stale = Instant::now()
            .checked_sub(FRESH + Duration::from_secs(1))
            .expect("the clock has been up longer than FRESH");
        let mut held = WARM.lock().unwrap();
        *held = Some(Warm { at: stale, list: vec![server("1.2.3.4:54200")] });
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

    /// The beat must not widen without bound, and must come back to the fast one.
    #[test]
    fn the_beat_backs_off_and_recovers() {
        let mut wait = BEAT;
        for _ in 0..10 {
            wait = (wait * 2).min(MAX_BEAT);
        }
        assert_eq!(wait, MAX_BEAT);
        assert!(MAX_BEAT > BEAT);
    }
}
