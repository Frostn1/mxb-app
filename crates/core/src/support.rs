//! The "support the developer" card: a rare, polite ask for a coffee.
//!
//! The apps are free and stay free. This card is the one place they mention that somebody pays
//! for them, and it is held to rules that keep it from turning into a nag:
//!
//! - it only appears straight after something went right (a mod installed, a track exported),
//!   or once every [`EVERY_LAUNCHES`] launches;
//! - never on the first launch, and never more than once every [`GAP`];
//! - "Remind me later" pushes it [`GAP`] away, "Don't show again" ends it for good;
//! - somebody known to be a supporter is never asked.
//!
//! The state is per app — the manager and the studio share one config file, and somebody who
//! has turned the card off in one has not said anything about the other. Like the survey, the
//! decision lives here rather than in the webview, which only draws the card.

use crate::config::{self, AppConfig, SupportState};
use crate::usage;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

/// Set to `1` to show nothing for one run, whatever the config says.
pub const DISABLE_ENV: &str = "MXB_NO_SUPPORT_PROMPT";

/// The shortest time between two cards, and how far "Remind me later" pushes the next one.
pub const GAP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How many launches since the last card before a launch alone is reason enough to show one.
pub const EVERY_LAUNCHES: u32 = 10;

/// How far "Support" pushes the next card. Somebody who just opened the page has most likely
/// either supported or decided not to; asking again a week later would be asking twice.
pub const AFTER_SUPPORT: Duration = Duration::from_secs(90 * 24 * 60 * 60);

/// How long the survey card keeps this one away, so the two never share a session's corner.
const AFTER_SURVEY: Duration = Duration::from_secs(60 * 60);

/// How long an app has to have been open before a launch alone may show the card. The card
/// must never be the first thing on screen.
const MIN_UPTIME: Duration = Duration::from_secs(3 * 60);

/// What prompted the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Something just finished well: an install, an export.
    Success,
    /// The app has been open a while; only counts every [`EVERY_LAUNCHES`] launches.
    Launch,
}

impl Trigger {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "success" => Some(Self::Success),
            "launch" => Some(Self::Launch),
            _ => None,
        }
    }
}

/// The whole schedule, with nothing but numbers in it.
///
/// `now` and the stamps are unix milliseconds; `survey_shown` is the survey card's last
/// appearance, which this one keeps clear of.
pub fn decide(state: &SupportState, trigger: Trigger, now: u64, survey_shown: u64) -> bool {
    if state.never || state.supporter {
        return false;
    }
    // `launches` counts this run too, so 1 is the first launch.
    if state.launches <= 1 {
        return false;
    }
    if now < state.snooze_until {
        return false;
    }
    if state.last_shown > 0 && now.saturating_sub(state.last_shown) < ms(GAP) {
        return false;
    }
    if survey_shown > 0 && now.saturating_sub(survey_shown) < ms(AFTER_SURVEY) {
        return false;
    }
    match trigger {
        Trigger::Success => true,
        Trigger::Launch => state.launches_since_shown >= EVERY_LAUNCHES,
    }
}

/// The card was put on screen.
pub fn mark_shown(state: &mut SupportState, now: u64) {
    state.last_shown = now;
    state.launches_since_shown = 0;
}

/// "Remind me later".
pub fn mark_later(state: &mut SupportState, now: u64) {
    state.snooze_until = now + ms(GAP);
}

/// "Support" — the page was opened.
pub fn mark_opened(state: &mut SupportState, now: u64) {
    state.snooze_until = now + ms(AFTER_SUPPORT);
}

/// "Don't show again".
pub fn mark_never(state: &mut SupportState) {
    state.never = true;
}

/// A new launch.
pub fn mark_launch(state: &mut SupportState) {
    state.launches = state.launches.saturating_add(1);
    state.launches_since_shown = state.launches_since_shown.saturating_add(1);
}

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Which app this is, and when it started.
struct State {
    app: &'static str,
    started: Instant,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn which() -> Option<&'static str> {
    STATE.lock().unwrap().as_ref().map(|s| s.app)
}

/// Whether this run may show the card at all.
fn allowed() -> bool {
    if std::env::var(DISABLE_ENV).map(|v| v == "1").unwrap_or(false) {
        return false;
    }
    // A developer running the app forty times a day would otherwise be asked for a coffee by
    // their own build.
    !(cfg!(debug_assertions) && !std::env::var(usage::DEV_ENV).map(|v| v == "1").unwrap_or(false))
}

/// Count this launch. Called once from `setup`, beside [`crate::survey::start`].
pub fn start(app: &AppHandle, which: &'static str) {
    *STATE.lock().unwrap() = Some(State { app: which, started: Instant::now() });
    update(app, mark_launch);
}

/// Load, change this app's block, save. A failed save is logged and otherwise ignored: nothing
/// about this card should ever surface as an error.
fn update(app: &AppHandle, change: impl FnOnce(&mut SupportState)) {
    let Some(app_id) = which() else { return };
    let Ok(mut cfg) = config::load(app) else { return };
    change(cfg.support.entry(app_id.to_string()).or_default());
    if let Err(e) = config::save(app, &cfg) {
        log::warn!("[support] couldn't save the support card's state ({e:#})");
    }
}

fn state_of(cfg: &AppConfig, app_id: &str) -> SupportState {
    cfg.support.get(app_id).cloned().unwrap_or_default()
}

/// Should the card go up now?
pub fn due(app: &AppHandle, trigger: Trigger) -> bool {
    if !allowed() {
        return false;
    }
    let (app_id, running) = match STATE.lock().unwrap().as_ref() {
        Some(s) => (s.app, s.started.elapsed()),
        None => return false,
    };
    if trigger == Trigger::Launch && running < MIN_UPTIME {
        return false;
    }
    let Ok(cfg) = config::load(app) else { return false };
    let mut state = state_of(&cfg, app_id);
    state.supporter |= crate::supporter::is_supporter(&cfg);
    decide(&state, trigger, now_ms(), cfg.survey.last_shown)
}

// ── The commands the webview calls ──────────────────────────────────────────────────────

/// Is now a good moment? `trigger` is `"success"` or `"launch"`.
#[tauri::command]
pub fn support_due(app: tauri::AppHandle, trigger: String) -> bool {
    Trigger::parse(&trigger).is_some_and(|t| due(&app, t))
}

/// The card is on screen.
#[tauri::command]
pub fn support_shown(app: tauri::AppHandle) {
    update(&app, |s| mark_shown(s, now_ms()));
}

/// "Remind me later".
#[tauri::command]
pub fn support_later(app: tauri::AppHandle) {
    update(&app, |s| mark_later(s, now_ms()));
}

/// "Support" was pressed and the page opened.
#[tauri::command]
pub fn support_opened(app: tauri::AppHandle) {
    update(&app, |s| mark_opened(s, now_ms()));
}

/// "Don't show again".
#[tauri::command]
pub fn support_never(app: tauri::AppHandle) {
    update(&app, mark_never);
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60 * 1000;
    /// Some fixed "now", far from zero so subtraction never wraps.
    const NOW: u64 = 1_800_000_000_000;

    /// An install on its second launch that has never seen the card.
    fn second_launch() -> SupportState {
        let mut s = SupportState::default();
        mark_launch(&mut s);
        mark_launch(&mut s);
        s
    }

    #[test]
    fn never_on_the_first_launch() {
        let mut s = SupportState::default();
        mark_launch(&mut s);
        assert!(!decide(&s, Trigger::Success, NOW, 0));
        assert!(!decide(&s, Trigger::Launch, NOW, 0));
    }

    #[test]
    fn a_success_after_the_first_launch_shows_it() {
        assert!(decide(&second_launch(), Trigger::Success, NOW, 0));
    }

    #[test]
    fn a_launch_alone_needs_ten_launches_since_the_last_card() {
        let mut s = SupportState::default();
        for _ in 0..9 {
            mark_launch(&mut s);
        }
        assert!(!decide(&s, Trigger::Launch, NOW, 0));
        mark_launch(&mut s);
        assert!(decide(&s, Trigger::Launch, NOW, 0));

        // Showing it starts the count again.
        mark_shown(&mut s, NOW - 8 * DAY);
        mark_launch(&mut s);
        assert!(!decide(&s, Trigger::Launch, NOW, 0));
    }

    #[test]
    fn at_most_once_every_seven_days() {
        let mut s = second_launch();
        mark_shown(&mut s, NOW);
        assert!(!decide(&s, Trigger::Success, NOW + DAY, 0));
        assert!(!decide(&s, Trigger::Success, NOW + 7 * DAY - 1, 0));
        assert!(decide(&s, Trigger::Success, NOW + 7 * DAY, 0));
    }

    #[test]
    fn remind_me_later_pushes_it_seven_days() {
        let mut s = second_launch();
        mark_later(&mut s, NOW);
        assert!(!decide(&s, Trigger::Success, NOW + 6 * DAY, 0));
        assert!(decide(&s, Trigger::Success, NOW + 7 * DAY, 0));
    }

    #[test]
    fn dont_show_again_is_for_good() {
        let mut s = second_launch();
        mark_never(&mut s);
        for _ in 0..50 {
            mark_launch(&mut s);
        }
        assert!(!decide(&s, Trigger::Success, NOW + 3650 * DAY, 0));
        assert!(!decide(&s, Trigger::Launch, NOW + 3650 * DAY, 0));
    }

    #[test]
    fn a_known_supporter_is_never_asked() {
        let mut s = second_launch();
        s.supporter = true;
        assert!(!decide(&s, Trigger::Success, NOW, 0));
    }

    #[test]
    fn opening_the_page_keeps_it_away_for_a_long_while() {
        let mut s = second_launch();
        mark_opened(&mut s, NOW);
        assert!(!decide(&s, Trigger::Success, NOW + 30 * DAY, 0));
        assert!(decide(&s, Trigger::Success, NOW + 90 * DAY, 0));
    }

    #[test]
    fn never_right_after_the_survey_card() {
        let s = second_launch();
        assert!(!decide(&s, Trigger::Success, NOW, NOW - 10 * 60 * 1000));
        assert!(decide(&s, Trigger::Success, NOW, NOW - 2 * 60 * 60 * 1000));
    }

    #[test]
    fn an_unknown_trigger_is_ignored() {
        assert_eq!(Trigger::parse("success"), Some(Trigger::Success));
        assert_eq!(Trigger::parse("launch"), Some(Trigger::Launch));
        assert_eq!(Trigger::parse("now"), None);
    }

    #[test]
    fn a_config_without_the_block_still_reads() {
        let cfg: AppConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.support.is_empty());
        let s: SupportState = serde_json::from_str(r#"{"launches":3}"#).unwrap();
        assert_eq!(s.launches, 3);
        assert!(!s.never);
    }
}
