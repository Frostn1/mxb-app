//! The survey prompt: asking the player, rather than inferring from counters.
//!
//! [`crate::usage`] answers what people open. It has never been able to answer whether any of
//! it is any good — a feature with high reach is one people *find* — so every judgement about
//! whether something is working has come from whoever said so in Discord. This is the other
//! half: a small card, rarely, with a question on it.
//!
//! ## What leaves the machine
//!
//! The same install id the counters use — a random UUID this app minted for itself — plus the
//! app, its version, the OS and the active title, and then the answer: a poll id and a choice
//! id, both from a list the control plane wrote. Optionally the follow-up's chips, from the
//! same kind of list.
//!
//! And, when the player types one, a note. That is the one free-form thing anything in this
//! project sends, and it is worth being plain about rather than burying: the box is optional,
//! opt-in per answer, capped at [`MAX_NOTE_CHARS`] here and again at the endpoint, scrubbed of
//! addresses, links and user-folder paths on arrival, deleted long before the answer it came
//! with, and removable one at a time from the dashboard. A poll that has no use for prose
//! turns the box off and collects chips alone.
//!
//! ## What stops it
//!
//! Three things, in order. The player's own switch (`survey_enabled`), which is separate from
//! the counters' — being counted and being interrupted are different bargains. The counters'
//! switch, because an answer carries the install id and consenting to be asked must not be a
//! way around having said no to being counted. And [`DISABLE_ENV`], for one run.
//!
//! Debug builds are silent unless [`crate::usage::DEV_ENV`] says otherwise, for the reason the
//! counters are: a developer running the app forty times a day should not be the most opinionated
//! user it has.
//!
//! ## Why the questions come from the server
//!
//! Because a question is worth asking for three weeks. "Have you tried Race mode" baked into a
//! release needs one release to start asking and another to stop, with a month in between for
//! either to reach anybody — by which time nobody cares about the answer. The polls live in the
//! control plane's `survey_polls` table and are fetched here; nothing about them is trusted,
//! which is what [`Poll::usable`] is for.
//!
//! ## What is decided here, and what is decided in the webview
//!
//! Here: *whether* to ask, and *which* question. That is scheduling, it depends on state the
//! webview cannot see, and a webview that could ask for the prompt could ask for it in a loop.
//! There: how the question reads. The poll's text is handed over as the locale map it arrived
//! as, and the UI picks the language — it is the half that knows which one is on screen.

use crate::config::{self, AppConfig};
use crate::names::control_plane;
use crate::usage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

/// Set to `1` to ask nothing for one run, whatever the config says.
pub const DISABLE_ENV: &str = "MXB_NO_SURVEY";

/// How long a fresh install is left alone.
///
/// Somebody who installed the app twenty minutes ago has no opinion of it worth collecting,
/// and asking for one is the worst possible first impression of a mod manager: it has not yet
/// done the thing it exists to do. Measured from the first run that could have asked, which is
/// why [`SurveyState::first_run`](crate::config::SurveyState::first_run) is stamped rather
/// than inferred from anything else in the config.
const SETTLE_IN: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// How long an app has to have been open before it asks anything.
///
/// The prompt must never be the first thing on screen. Somebody who just double-clicked the
/// icon is on their way to doing something, and a card in the corner before the library has
/// even listed is an interruption of the task rather than a question about it.
const MIN_UPTIME: Duration = Duration::from_secs(5 * 60);

/// The ceiling on how often a prompt may appear at all: once a day, across every poll.
///
/// A ceiling, not a schedule. What actually decides whether there is anything to ask is the
/// poll's own `againDays` and whether this install has answered it — in ordinary running that
/// means a few times a year, not daily. This is here so that a batch of new questions written
/// in one afternoon cannot turn into four cards in one evening.
const ASK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// How long the prompt stays away after being dismissed.
const SNOOZE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Dismissals in a row before this is taken as an answer.
///
/// Waving the card away is the only way the prompt offers of saying "not interested", so three
/// of them is somebody saying it clearly. Continuing to ask after that is not research.
const GIVE_UP_AFTER: u32 = 3;

/// How long the prompt stays away after [`GIVE_UP_AFTER`].
const LONG_QUIET: Duration = Duration::from_secs(180 * 24 * 60 * 60);

/// How often the poll list is re-fetched while the app is open.
///
/// The app lives in the tray for days at a time, so "fetch once at startup" would mean a
/// question written on Tuesday reaching a machine that was last restarted on Sunday only when
/// it next restarts. Six hours is well inside the response's own cache lifetime.
const REFRESH_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The shortest gap between two fetches, however many things ask for one.
const MIN_BETWEEN_FETCHES: Duration = Duration::from_secs(60);

/// The longest note that will be sent. The endpoint caps it too; this stops it travelling.
pub const MAX_NOTE_CHARS: usize = 280;

/// The most chips one answer may carry, matching the endpoint.
pub const MAX_REASONS: usize = 8;

/// The standing question's id, and the answers the app draws for it.
///
/// A `mood` poll's three answers are not in its row: they are here, so the words the player
/// reads are the app's own translated strings rather than English served from a database. The
/// ids are the contract between the two — the endpoint groups on exactly these.
pub const MOOD_CHOICES: [&str; 3] = ["bad", "fine", "good"];

/// The follow-up chips the app ships translated, used by any poll that names none of its own.
pub const BUILT_IN_REASONS: [&str; 6] = ["crash", "slow", "confusing", "broken", "missing", "other"];

/// Text in whatever languages it was written in, keyed by locale. The UI picks; `en` is the
/// fallback and the endpoint refuses a poll without one.
pub type Localized = BTreeMap<String, String>;

/// One answer the player can tap, or one chip on the follow-up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PollChoice {
    pub id: String,
    #[serde(default)]
    pub label: Localized,
}

/// A question, as the control plane serves it.
///
/// Every field is defaulted, so a poll written by a newer control plane than this build knows
/// about still deserializes rather than taking the whole list down with it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Poll {
    pub id: String,
    /// `mood` — the standing question, whose three answers this app draws and translates.
    /// `choice` — anything else, which speaks whatever languages its own text was written in.
    pub kind: String,
    pub apps: Vec<String>,
    pub ask: Localized,
    pub choices: Vec<PollChoice>,
    /// Empty means [`BUILT_IN_REASONS`], which ship translated.
    pub reasons: Vec<PollChoice>,
    /// Choice ids that always get the follow-up.
    pub follow_up: Vec<String>,
    /// How often the follow-up comes after any other answer, 0–1.
    pub follow_up_chance: f64,
    /// Whether the optional note box is offered at all.
    pub note: bool,
    /// Days before the same install is asked again. 0 means once, ever.
    pub again_days: u32,
    /// Only ask builds at or past this. Empty asks everybody.
    pub min_version: String,
}

impl Poll {
    /// Is this something this app can actually put on screen?
    ///
    /// The control plane validates a poll when it is written, and this checks it again on
    /// arrival — not because the endpoint is distrusted, but because a build in the field
    /// outlives the deployment that served it, and a prompt with no question on it or with
    /// answers it cannot draw is worse than no prompt at all.
    pub fn usable(&self, app: &str, version: &str) -> bool {
        if self.id.is_empty() || !self.apps.iter().any(|a| a == app) {
            return false;
        }
        if !self.min_version.is_empty() && !at_least(version, &self.min_version) {
            return false;
        }
        match self.kind.as_str() {
            // The words are ours; the row only has to name it.
            "mood" => true,
            // Anything else has to bring a question and at least two answers, each with a
            // label in some language this UI can fall back through.
            "choice" => {
                !self.ask.is_empty()
                    && self.choices.len() >= 2
                    && self.choices.iter().all(|c| !c.id.is_empty() && !c.label.is_empty())
            }
            _ => false,
        }
    }

    /// Does an answer of `choice` get the follow-up?
    ///
    /// Always for the answers the poll names — somebody who has just said it is going badly is
    /// the one person worth asking why — and otherwise on a coin weighted by `followUpChance`.
    /// Not always, deliberately: a second question every single time is what turns one tap into
    /// a form, and a quarter of the answers is plenty to read a pattern off.
    fn wants_follow_up(&self, choice: &str) -> bool {
        if self.follow_up.iter().any(|c| c == choice) {
            return true;
        }
        self.follow_up_chance > 0.0 && coin() < self.follow_up_chance
    }
}

/// Is `version` at or past `least`? A build that cannot parse its own version asks anyway —
/// missing a question is a smaller failure than a whole app going quiet over a parse.
fn at_least(version: &str, least: &str) -> bool {
    match (semver::Version::parse(version), semver::Version::parse(least)) {
        (Ok(have), Ok(want)) => have >= want,
        _ => true,
    }
}

/// A uniform number in `[0, 1)`, from the OS.
///
/// `getrandom` rather than a time-derived value: two apps on one machine start within
/// milliseconds of each other, and a coin flipped from the clock would land the same way in
/// both. It is a coin toss, not a key, so a failure to read entropy is a "no" rather than a
/// panic.
fn coin() -> f64 {
    let mut bytes = [0u8; 8];
    if getrandom::getrandom(&mut bytes).is_err() {
        return 1.0;
    }
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// What the webview is handed when there is something to ask.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub poll: Poll,
    /// The answer ids for a `mood` poll, so the UI never has to know them by heart. Empty for
    /// any other kind, whose answers are in the poll itself.
    pub mood_choices: Vec<String>,
    /// The chip ids to draw when the poll names none of its own.
    pub built_in_reasons: Vec<String>,
}

/// What an answer got back: whether there is a second question.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Answered {
    pub follow_up: bool,
}

/// Everything this module keeps for the life of the run.
struct State {
    /// Which binary is asking — [`usage::MANAGER`] and friends.
    app: &'static str,
    /// When this run started, for [`MIN_UPTIME`].
    started: Instant,
    /// The polls last fetched, and when.
    polls: Vec<Poll>,
    fetched: Option<Instant>,
    /// The poll [`due`] last handed over, so an answer can be matched to the question that was
    /// actually on screen rather than to whatever the webview claims it was answering.
    asked: Option<Poll>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Start asking, and keep the question list fresh.
///
/// Called once from `setup`, beside [`usage::start`]. The loop is spawned whatever the switch
/// says, and every pass through it asks again — a switch that is off means nothing is fetched
/// and nothing is shown, and a switch thrown mid-run means exactly that from the next pass
/// rather than from the next launch.
pub fn start(app: &AppHandle, which: &'static str) {
    *STATE.lock().unwrap() = Some(State {
        app: which,
        started: Instant::now(),
        polls: Vec::new(),
        fetched: None,
        asked: None,
    });
    if !allowed(&config::load(app).unwrap_or_default()) {
        // Set up anyway, and let [`refresh`] and [`due`] find the switch off every time they
        // look. Bailing out here would mean the switch in Settings did nothing until the next
        // launch, which is not what a switch is — and nothing is fetched or asked meanwhile,
        // because both of those re-read the config rather than trusting a flag set now.
        log::info!("[survey] the survey prompt is off — nothing will be fetched or asked");
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&handle).await;
            tokio::time::sleep(REFRESH_EVERY).await;
        }
    });
}

/// Whether this run may ask anything at all.
pub fn allowed(cfg: &AppConfig) -> bool {
    if std::env::var(DISABLE_ENV).map(|v| v == "1").unwrap_or(false) {
        return false;
    }
    if cfg!(debug_assertions) && !std::env::var(usage::DEV_ENV).map(|v| v == "1").unwrap_or(false) {
        return false;
    }
    // Both switches, and the id. An answer carries the same install id the counters use, so
    // being asked cannot be a way round having said no to being counted — and an install with
    // no id (a studio-only machine, see `usage::start`) has nothing to attach an answer to.
    cfg.analytics_enabled && cfg.survey_enabled && !cfg.install_id.trim().is_empty()
}

/// Turn the prompt on or off from Settings.
pub fn set_enabled(on: bool) {
    if !on {
        // Forget what was going to be asked, so a prompt already decided on cannot be answered
        // after the switch was thrown.
        if let Some(state) = STATE.lock().unwrap().as_mut() {
            state.asked = None;
        }
    }
    log::info!("[survey] the survey prompt is {}", if on { "on" } else { "off" });
}

/// Fetch the live questions.
async fn refresh(app: &AppHandle) {
    let Some((which, fetched)) = ({
        let guard = STATE.lock().unwrap();
        guard.as_ref().map(|state| (state.app, state.fetched))
    }) else {
        return;
    };
    // The loop paces itself, but the switch in Settings kicks one of these too, so a flurry of
    // flips would otherwise be a flurry of requests.
    if fetched.is_some_and(|at| at.elapsed() < MIN_BETWEEN_FETCHES) {
        return;
    }
    // Re-read rather than trust the flag: the switch may have been thrown since startup, and
    // this is the last point before anything is asked for.
    if !allowed(&config::load(app).unwrap_or_default()) {
        return;
    }

    let url = format!("{}/v1/survey/polls?app={}", control_plane(), which);
    let fetched = match client() {
        Ok(client) => client.get(&url).send().await,
        Err(e) => {
            log::debug!("[survey] no HTTP client: {e}");
            return;
        }
    };
    let polls = match fetched {
        Ok(res) if res.status().is_success() => match res.json::<PollList>().await {
            Ok(list) => list.polls,
            Err(e) => {
                log::debug!("[survey] couldn't read the question list ({e})");
                return;
            }
        },
        Ok(res) => {
            log::debug!("[survey] the question list came back {}", res.status());
            return;
        }
        Err(e) => {
            log::debug!("[survey] couldn't fetch the question list ({e})");
            return;
        }
    };

    if let Some(state) = STATE.lock().unwrap().as_mut() {
        state.polls = polls;
        state.fetched = Some(Instant::now());
    }
}

#[derive(Deserialize)]
struct PollList {
    #[serde(default)]
    polls: Vec<Poll>,
}

/// Is there something to ask right now?
///
/// Called by the webview on a timer. Everything it can answer "no" with is checked here rather
/// than there, because the webview cannot see the config, cannot see how long the app has been
/// open, and — being where the plugins are mounted — is not the right place to decide how often
/// the player is interrupted.
pub fn due(app: &AppHandle) -> Option<Prompt> {
    let cfg = config::load(app).ok()?;
    if !allowed(&cfg) {
        return None;
    }
    // Only the uptime needs the lock at this point, and holding it across a config write below
    // would be holding it across a file write.
    let running = STATE.lock().unwrap().as_ref()?.started.elapsed();
    if running < MIN_UPTIME {
        return None;
    }

    let now = now_ms();
    let survey = &cfg.survey;
    if survey.first_run == 0 {
        // The settling-in period is measured from the first time an app could have asked, so it
        // is stamped here rather than at startup: an install that had the switch off for a
        // month and has just turned it on has not been asked anything either, and should not be
        // asked the moment it says yes. Nothing is asked on the pass that stamps it.
        stamp_first_run(app, &cfg);
        return None;
    }
    if now.saturating_sub(survey.first_run) < SETTLE_IN.as_millis() as u64 {
        return None;
    }
    if now < survey.quiet_until {
        return None;
    }
    if survey.last_shown > 0 && now.saturating_sub(survey.last_shown) < ASK_EVERY.as_millis() as u64 {
        return None;
    }

    let version = app.package_info().version.to_string();
    let mut guard = STATE.lock().unwrap();
    let state = guard.as_mut()?;
    let poll = state
        .polls
        .iter()
        .find(|poll| poll.usable(state.app, &version) && is_due(poll, survey, now))?
        .clone();

    // Remembered, so an answer can be matched against the question that was actually put up.
    state.asked = Some(poll.clone());
    Some(Prompt {
        poll,
        mood_choices: MOOD_CHOICES.iter().map(|s| s.to_string()).collect(),
        built_in_reasons: BUILT_IN_REASONS.iter().map(|s| s.to_string()).collect(),
    })
}

/// Record when this machine first had an app that could ask.
fn stamp_first_run(app: &AppHandle, cfg: &AppConfig) {
    let mut fresh = cfg.clone();
    fresh.survey.first_run = now_ms();
    if let Err(e) = config::save(app, &fresh) {
        log::warn!("[survey] couldn't stamp the first run ({e:#})");
    }
}

/// Has this install answered `poll` recently enough to be left alone about it?
fn is_due(poll: &Poll, survey: &config::SurveyState, now: u64) -> bool {
    match survey.answered.get(&poll.id) {
        None => true,
        // Once, ever.
        Some(_) if poll.again_days == 0 => false,
        Some(&at) => now.saturating_sub(at) >= poll.again_days as u64 * 24 * 60 * 60 * 1000,
    }
}

/// The prompt was put on screen. Starts the once-a-day clock whether or not it is answered.
///
/// Separate from [`due`] on purpose: `due` is polled, and a webview that reloaded between the
/// two would otherwise have burned a day's allowance on a card nobody saw.
pub fn shown(app: &AppHandle) {
    let Ok(mut cfg) = config::load(app) else { return };
    cfg.survey.last_shown = now_ms();
    if let Err(e) = config::save(app, &cfg) {
        log::warn!("[survey] couldn't record that the prompt was shown ({e:#})");
    }
    usage::track("survey.shown");
}

/// The player answered.
///
/// Sends immediately rather than waiting for the follow-up, and that is the important part: the
/// rating is the answer, and somebody who taps "fine" and then closes the card has answered the
/// question. The follow-up's chips arrive as a second call, which the endpoint folds onto the
/// same row.
///
/// Infallible from the caller's side. Nothing about a survey should be able to fail in a way
/// the player has to deal with — a send that does not land is an answer lost, which is a cost
/// worth paying over an error dialog about a question nobody asked for.
pub async fn answer(
    app: &AppHandle,
    poll_id: &str,
    choice: &str,
    reasons: Vec<String>,
    note: Option<String>,
) -> Answered {
    let asked = STATE.lock().unwrap().as_ref().and_then(|s| s.asked.clone());
    // Matched against the poll this run actually put on screen. The webview is where paid
    // plugins are mounted, so "answer whatever id you like" is a door into everybody's numbers
    // — the same reason `usage::track` refuses a name it did not ship with.
    let Some(poll) = asked.filter(|p| p.id == poll_id) else {
        log::warn!("[survey] refusing an answer to {poll_id:?} — not the question that was asked");
        return Answered { follow_up: false };
    };

    let Ok(cfg) = config::load(app) else {
        return Answered { follow_up: false };
    };
    if !allowed(&cfg) {
        return Answered { follow_up: false };
    }
    if !answerable(&poll, choice) {
        log::warn!("[survey] refusing {choice:?} — not one of {poll_id}'s answers");
        return Answered { follow_up: false };
    }

    // Recorded before the send, and whether or not it lands: the point of the record is that
    // this install is not asked again, and a dropped request is not a reason to ask twice.
    let mut fresh = cfg.clone();
    fresh.survey.answered.insert(poll.id.clone(), now_ms());
    fresh.survey.dismissals = 0;
    if let Err(e) = config::save(app, &fresh) {
        log::warn!("[survey] couldn't record an answer ({e:#})");
    }
    usage::track("survey.answer");

    let payload = Answer {
        install_id: cfg.install_id.trim().to_string(),
        app: STATE.lock().unwrap().as_ref().map(|s| s.app).unwrap_or(usage::MANAGER).to_string(),
        version: app.package_info().version.to_string(),
        os: usage::platform().to_string(),
        game: cfg.active_game.id().to_string(),
        poll_id: poll.id.clone(),
        choice: choice.to_string(),
        reasons: clean_reasons(&poll, reasons),
        // A poll that turned the box off never sends one, whatever arrives from the webview.
        note: if poll.note { clean_note(note) } else { None },
    };
    send(payload).await;

    Answered { follow_up: poll.wants_follow_up(choice) }
}

/// Is `choice` one of the answers this poll actually offers?
fn answerable(poll: &Poll, choice: &str) -> bool {
    if poll.kind == "mood" {
        return MOOD_CHOICES.contains(&choice);
    }
    poll.choices.iter().any(|c| c.id == choice)
}

/// The chips, held to the list the question offered.
fn clean_reasons(poll: &Poll, reasons: Vec<String>) -> Vec<String> {
    let offered: Vec<&str> = if poll.reasons.is_empty() {
        BUILT_IN_REASONS.to_vec()
    } else {
        poll.reasons.iter().map(|r| r.id.as_str()).collect()
    };
    let mut clean = Vec::new();
    for reason in reasons {
        if clean.len() >= MAX_REASONS {
            break;
        }
        if offered.contains(&reason.as_str()) && !clean.contains(&reason) {
            clean.push(reason);
        }
    }
    clean
}

/// The note, as it will travel: one line, capped, or nothing.
///
/// The endpoint scrubs and caps it again — that is the check that counts, because it is the one
/// a modified client cannot skip. This one keeps a note the player did not mean to send from
/// leaving the machine at all, which is the only place that can be prevented.
fn clean_note(note: Option<String>) -> Option<String> {
    let note = note?;
    let line: String = note
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(MAX_NOTE_CHARS).collect())
}

/// The player waved the card away.
///
/// Three in a row and the prompt stops asking for half a year. Dismissing is the only way this
/// card offers of saying "not interested", so it is taken as an answer rather than as silence.
pub fn dismiss(app: &AppHandle) {
    let Ok(mut cfg) = config::load(app) else { return };
    cfg.survey.dismissals = cfg.survey.dismissals.saturating_add(1);
    let quiet = if cfg.survey.dismissals >= GIVE_UP_AFTER { LONG_QUIET } else { SNOOZE };
    cfg.survey.quiet_until = now_ms() + quiet.as_millis() as u64;
    if let Err(e) = config::save(app, &cfg) {
        log::warn!("[survey] couldn't record a dismissal ({e:#})");
    }
    if let Some(state) = STATE.lock().unwrap().as_mut() {
        state.asked = None;
    }
    usage::track("survey.dismiss");
}

/// An answer, exactly as `POST /v1/survey` expects it.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Answer {
    install_id: String,
    app: String,
    version: String,
    os: String,
    game: String,
    poll_id: String,
    choice: String,
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

/// Send one answer. Nothing is retried and nothing is spooled: see [`answer`].
async fn send(payload: Answer) {
    let url = format!("{}/v1/survey", control_plane());
    // Serialized once, here, so the bytes that are signed are the bytes that travel — the same
    // reason `usage::report` does it this way.
    let raw = match serde_json::to_string(&payload) {
        Ok(raw) => raw,
        Err(e) => {
            log::warn!("[survey] couldn't serialize an answer ({e}) — dropped");
            return;
        }
    };
    let Ok(client) = client() else { return };
    let mut request = client
        .post(&url)
        .header("content-type", "application/json")
        .body(raw.clone());
    if let Some(header) = usage::signature(&raw) {
        request = request.header(usage::SIGNATURE_HEADER, header);
    }
    match request.send().await {
        Ok(res) if res.status().is_success() => {}
        Ok(res) => log::debug!("[survey] an answer came back {}", res.status()),
        Err(e) => log::debug!("[survey] an answer didn't send ({e})"),
    }
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── The commands the webview calls ──────────────────────────────────────────────────────
//
// Defined here rather than three times in three `main.rs` files, and registered by each app
// the way the viewer's and the track view's commands already are. One prompt, one schedule,
// one set of rules about what may be answered — a copy per app is three chances for one of
// them to drift into asking somebody every morning.

/// Is there a question to put on screen?
///
/// Polled by the shell. Returns nothing almost every time, which is the intended shape: the
/// decision lives on this side because it depends on the config, on how long the app has been
/// open and on what this install has already been asked — none of which the webview can see,
/// and all of which a webview asking in a loop could otherwise ignore.
#[tauri::command]
pub fn survey_due(app: tauri::AppHandle) -> Option<Prompt> {
    due(&app)
}

/// The card is on screen. Starts the once-a-day clock whether or not it is answered.
#[tauri::command]
pub fn survey_shown(app: tauri::AppHandle) {
    shown(&app);
}

/// The player tapped an answer; the reply says whether to ask the follow-up.
///
/// Called twice for an answer that gets one: once with the choice alone, and again with the
/// chips and the note. The endpoint folds the second onto the first, so a player who closes
/// the card at the follow-up has still answered the question.
#[tauri::command]
pub async fn survey_answer(
    app: tauri::AppHandle,
    poll_id: String,
    choice: String,
    reasons: Vec<String>,
    note: Option<String>,
) -> Answered {
    answer(&app, &poll_id, &choice, reasons, note).await
}

/// The player waved the card away.
#[tauri::command]
pub fn survey_dismiss(app: tauri::AppHandle) {
    dismiss(&app);
}

/// The switch in Settings.
///
/// Saves first and only then tells this module, so a save that failed can never leave the app
/// asking questions the player has said no to — the same order `set_analytics_enabled` uses.
#[tauri::command]
pub fn set_survey_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app).unwrap_or_default();
    cfg.survey_enabled = enabled;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    set_enabled(enabled);
    if enabled {
        // The list is refreshed six-hourly, and somebody who has just turned this on should not
        // wait that long for it — the fetch debounces itself, so this cannot become a way to
        // hammer the endpoint with a switch.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move { refresh(&handle).await });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SurveyState;

    fn mood() -> Poll {
        Poll {
            id: "mood".into(),
            kind: "mood".into(),
            apps: vec!["manager".into(), "studio".into(), "coach".into()],
            follow_up: vec!["bad".into()],
            follow_up_chance: 0.0,
            note: true,
            again_days: 45,
            ..Poll::default()
        }
    }

    fn choice_poll() -> Poll {
        Poll {
            id: "race-mode".into(),
            kind: "choice".into(),
            apps: vec!["manager".into()],
            ask: [("en".to_string(), "Tried Race mode?".to_string())].into_iter().collect(),
            choices: vec![
                PollChoice { id: "yes".into(), label: [("en".to_string(), "Yes".to_string())].into_iter().collect() },
                PollChoice { id: "no".into(), label: [("en".to_string(), "Not yet".to_string())].into_iter().collect() },
            ],
            note: true,
            ..Poll::default()
        }
    }

    #[test]
    fn a_mood_poll_needs_no_text_because_the_app_has_its_own() {
        assert!(mood().usable("manager", "0.15.1"));
    }

    #[test]
    fn a_question_with_nothing_to_ask_is_not_drawn() {
        let mut poll = choice_poll();
        poll.ask.clear();
        assert!(!poll.usable("manager", "0.15.1"));
    }

    #[test]
    fn a_question_with_one_answer_is_not_a_question() {
        let mut poll = choice_poll();
        poll.choices.truncate(1);
        assert!(!poll.usable("manager", "0.15.1"));
    }

    #[test]
    fn a_kind_this_build_has_never_heard_of_is_left_alone() {
        let mut poll = choice_poll();
        poll.kind = "slider".into();
        assert!(!poll.usable("manager", "0.15.1"));
    }

    #[test]
    fn a_question_for_another_app_is_not_asked_here() {
        assert!(!choice_poll().usable("coach", "0.15.1"));
    }

    #[test]
    fn a_question_about_something_this_build_has_not_got_is_skipped() {
        let mut poll = choice_poll();
        poll.min_version = "0.16.0".into();
        assert!(!poll.usable("manager", "0.15.1"));
        assert!(poll.usable("manager", "0.16.0"));
        assert!(poll.usable("manager", "1.0.0"));
    }

    #[test]
    fn a_version_neither_side_can_parse_asks_anyway() {
        let mut poll = choice_poll();
        poll.min_version = "whenever".into();
        assert!(poll.usable("manager", "0.15.1"));
    }

    #[test]
    fn the_answers_are_the_ones_the_question_offered() {
        assert!(answerable(&mood(), "bad"));
        assert!(!answerable(&mood(), "awful"));
        assert!(answerable(&choice_poll(), "yes"));
        assert!(!answerable(&choice_poll(), "bad"));
    }

    #[test]
    fn a_chip_the_question_never_offered_is_dropped() {
        let poll = mood();
        // No reasons of its own, so the built-in list is what it offers.
        let clean = clean_reasons(&poll, vec!["crash".into(), "smells".into(), "slow".into()]);
        assert_eq!(clean, vec!["crash".to_string(), "slow".to_string()]);
    }

    #[test]
    fn the_same_chip_twice_is_counted_once() {
        let clean = clean_reasons(&mood(), vec!["crash".into(), "crash".into()]);
        assert_eq!(clean, vec!["crash".to_string()]);
    }

    #[test]
    fn a_bad_answer_always_gets_the_follow_up_and_a_good_one_does_not_by_chance_alone() {
        let poll = mood();
        assert!(poll.wants_follow_up("bad"));
        // `follow_up_chance` is zero in the fixture, so this is the named list alone.
        assert!(!poll.wants_follow_up("good"));
    }

    #[test]
    fn a_certain_chance_asks_everyone() {
        let mut poll = mood();
        poll.follow_up_chance = 1.0;
        assert!(poll.wants_follow_up("good"));
    }

    #[test]
    fn a_note_is_one_line_and_capped() {
        assert_eq!(clean_note(Some("one\ntwo\t three ".into())), Some("one two three".to_string()));
        assert_eq!(clean_note(Some("   ".into())), None);
        assert_eq!(clean_note(None), None);
        let long = clean_note(Some("x".repeat(MAX_NOTE_CHARS * 2))).unwrap();
        assert_eq!(long.chars().count(), MAX_NOTE_CHARS);
    }

    #[test]
    fn a_question_answered_once_is_not_asked_again() {
        let mut survey = SurveyState::default();
        let now = 1_700_000_000_000;
        survey.answered.insert("race-mode".into(), now - 1000);
        assert!(!is_due(&choice_poll(), &survey, now));
    }

    #[test]
    fn a_question_that_comes_round_waits_its_turn() {
        let poll = mood();
        let now = 1_700_000_000_000u64;
        let day = 24 * 60 * 60 * 1000u64;
        let mut survey = SurveyState::default();

        survey.answered.insert("mood".into(), now - 10 * day);
        assert!(!is_due(&poll, &survey, now));

        survey.answered.insert("mood".into(), now - 46 * day);
        assert!(is_due(&poll, &survey, now));
    }

    #[test]
    fn a_question_never_answered_is_due() {
        assert!(is_due(&mood(), &SurveyState::default(), 1_700_000_000_000));
    }

    #[test]
    fn a_coin_lands_inside_the_unit_interval() {
        for _ in 0..64 {
            let flip = coin();
            assert!((0.0..=1.0).contains(&flip), "{flip} is not a probability");
        }
    }

    #[test]
    fn a_poll_from_a_newer_control_plane_still_deserializes() {
        let json = r#"{"id":"x","kind":"choice","apps":["manager"],"ask":{"en":"?"},
            "choices":[{"id":"a","label":{"en":"A"}},{"id":"b","label":{"en":"B"}}],
            "somethingNewer":42}"#;
        let poll: Poll = serde_json::from_str(json).expect("a newer field is not a reason to fail");
        assert!(poll.usable("manager", "0.15.1"));
    }
}
