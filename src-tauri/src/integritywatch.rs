//! Keep watching while the session is live.
//!
//! [`crate::integrity`] can answer "is anything attached to the game?" once. That is not the
//! question a race asks. A cheat is loaded *during* a session — the player joins clean, gets
//! beaten, alt-tabs, and attaches something — and a check that ran at launch would have said
//! everything was fine ten minutes before it stopped being fine. So the scan repeats for as
//! long as the game is up.
//!
//! It also has to cover a game the app never launched. The mods folder is watched that way
//! for the same reason ([`crate::modwatch`]): plenty of people press Play in Steam. So this
//! is a standing watcher started at app launch, not something hung off the Play button — it
//! sits on a cheap "is the game up?" poll and only does real work while it is.
//!
//! Because it is already the one place that notices the game starting, it is also where
//! FrostMod is re-armed for a new session ([`crate::frostmod_manage::on_game_started`]).
//! That is a second job for one poll rather than a second poll, and it inherits the reason
//! this watcher is standing rather than hung off the Play button: the game is just as often
//! started from Steam. The re-arm sits *above* the `integrityWatch` gate — turning the
//! scanner off is not a request to stop FrostMod working.
//!
//! Two settings, deliberately separate, because they are two different decisions:
//!
//!   * `integrityWatch` — scan at all. On by default; it reads the game's own module list and
//!     the answer goes no further than this window.
//!   * `integrityReport` — publish the verdict so a server admin can see it. Off until it is
//!     chosen. See [`publish`] for exactly what leaves the machine, which is much less than
//!     what a scan sees.

use crate::config::AppConfig;
use crate::integrity::{self, Report, RuleSet, ScanContext, Severity, Verdict};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

/// How often to look while the game is running.
///
/// A cheat that attaches mid-session is caught within this window, so it wants to be short;
/// each pass walks a module list and — only when something is unaccounted for — hashes a
/// file, so it can't be free. Forty-five seconds matches the live paint sync, which is the
/// other thing running through a race.
const SCAN_EVERY: std::time::Duration = std::time::Duration::from_secs(45);

/// How often to ask whether the game has started. A process-table walk, and nothing else.
const IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(15);

/// Event name the UI listens on. Emitted only when the answer *changes*, so a settled
/// session is silent rather than pushing an identical report every 45 seconds.
pub const EVENT: &str = "integrity-report";

/// The last report, for anything that needs the current answer without waiting for a scan.
fn last() -> &'static Mutex<Report> {
    static LAST: OnceLock<Mutex<Report>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(Report::default()))
}

/// The rule list in force. Loaded from cache at startup and replaced by each refresh.
fn rules() -> &'static Mutex<RuleSet> {
    static RULES: OnceLock<Mutex<RuleSet>> = OnceLock::new();
    RULES.get_or_init(|| Mutex::new(RuleSet::bundled()))
}

pub fn current() -> Report {
    last().lock().map(|r| r.clone()).unwrap_or_default()
}

fn current_rules() -> RuleSet {
    rules().lock().map(|r| r.clone()).unwrap_or_else(|_| RuleSet::bundled())
}

/// What the app knows about this machine that the classifier needs.
///
/// The trusted roots are the folders whose contents are the player's own install: the game,
/// the FrostMod we put there, and ReShade if they use it. Everything else that ends up inside
/// the game process has to explain itself.
fn context(app: &AppHandle, cfg: &AppConfig) -> ScanContext {
    let mut roots = vec![crate::frostmod_manage::frostmod_dir(app)];
    let reshade = cfg.reshade_dir();
    if !reshade.trim().is_empty() {
        roots.push(PathBuf::from(reshade));
    }
    ScanContext::new(&PathBuf::from(cfg.install_dir()), &roots)
}

/// Scan once, remember the answer, and tell anyone who is listening.
///
/// Returns the report either way — including the [`Verdict::Unknown`] that means we could not
/// look, which is a real answer and not a failure to be swallowed.
pub fn scan_once(app: &AppHandle) -> Report {
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    if !cfg.integrity_watch {
        return Report::default();
    }
    let report = integrity::scan(&context(app, &cfg), &current_rules());
    remember(app, &cfg, report.clone());
    report
}

/// Store a report, emit it if it says something new, and publish it if that's turned on.
///
/// "Something new" is the verdict plus the findings — not the timestamp, which changes on
/// every pass. Without that test a quiet session would emit an identical report every 45
/// seconds and the UI would flash a banner each time.
fn remember(app: &AppHandle, cfg: &AppConfig, report: Report) {
    let changed = match last().lock() {
        Ok(mut slot) => {
            let changed = slot.verdict != report.verdict || slot.findings != report.findings;
            *slot = report.clone();
            changed
        }
        // A poisoned lock means a previous scan panicked. Emit rather than go quiet: the one
        // thing this must not do is stop reporting without saying so.
        Err(_) => true,
    };
    if !changed {
        return;
    }
    if report.verdict >= Verdict::Suspect {
        log::warn!(
            "[integrity] {:?}: {}",
            report.verdict,
            report.findings.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    let _ = app.emit(EVENT, &report);
    if cfg.integrity_report {
        publish_soon(cfg, &report);
    }
}

/// What the control plane is told.
///
/// Deliberately not the report: a scan sees every DLL loaded on the machine, and that is
/// nobody else's business. A [`Severity::Suspect`] finding is by definition "software we do
/// not recognise", which on somebody's work laptop could be anything — so its *name* never
/// leaves, only the fact that the count is non-zero. Rule-matched detections do carry their
/// name and label, because at that point it is a known cheat and naming it is the point.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attestation {
    verdict: Verdict,
    /// Which rule list produced the verdict. An admin looking at a Clean report needs to
    /// know it was checked against something current.
    rules_version: u32,
    /// How many unrecognised-but-unnamed things were loaded.
    suspect_count: usize,
    /// The named detections, worst first.
    flagged: Vec<FlaggedItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FlaggedItem {
    name: String,
    label: String,
    sha256: String,
}

/// Send an attestation to the control plane. Best-effort and fire-and-forget: a failed
/// publish costs one missing row in an admin's list for a minute.
fn publish_soon(cfg: &AppConfig, report: &Report) {
    let token = cfg.cp_token.trim().to_string();
    if token.is_empty() {
        return;
    }
    let body = Attestation {
        verdict: report.verdict,
        rules_version: report.rules_version,
        suspect_count: report.findings.iter().filter(|f| f.severity == Severity::Suspect).count(),
        flagged: report
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Flagged)
            .map(|f| FlaggedItem {
                name: f.name.clone(),
                label: f.label.clone(),
                sha256: f.sha256.clone(),
            })
            .collect(),
    };
    tauri::async_runtime::spawn(async move {
        if let Err(e) = publish(&token, &body).await {
            log::debug!("[integrity] couldn't publish the verdict: {e:#}");
        }
    });
}

async fn publish(token: &str, body: &Attestation) -> anyhow::Result<()> {
    let res = reqwest::Client::new()
        .put(format!("{}/v1/integrity", crate::paintsync::control_plane()))
        .bearer_auth(token)
        .json(body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;
    if !res.status().is_success() {
        anyhow::bail!("control plane said {}", res.status());
    }
    Ok(())
}

/// Start the standing watcher. Call once, from `setup`.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Load whatever list is cached before the first scan, so a machine that starts
        // offline still matches against the last one it saw rather than an empty set.
        if let Ok(mut slot) = rules().lock() {
            *slot = integrity::cached_rules(&app);
        }
        refresh_rules(&app).await;

        // The game process, tracked regardless of any setting — `scanned` below is the
        // scanner's own view, which the `integrityWatch` switch is allowed to reset.
        let mut was_running = false;
        let mut scanned = false;
        // A handle on the current session, held so that how it ended is still readable
        // once the process is gone. See [`crate::gameproc::GameSession`].
        let mut session: Option<crate::gameproc::GameSession> = None;
        loop {
            let cfg = crate::config::load_or_detect(&app).unwrap_or_default();

            let running = crate::gameproc::is_game_running();
            let started = running && !was_running;
            was_running = running;

            // Checked every pass, not only when the poll says the game is gone: the handle
            // is what knows the process ended, and it knows it exactly.
            if let Some(open) = session.take() {
                session = open.report_if_ended();
            }

            // Above the `integrityWatch` gate deliberately: re-arming FrostMod has nothing
            // to do with cheat scanning, and someone who turned the scanner off has not
            // asked for FrostMod to stop working.
            if started {
                crate::frostmod_manage::on_game_started(&app, &cfg);
                session = crate::gameproc::GameSession::open();
                // The mods folder is read during the load screen, so a placeholder that
                // isn't really on disk becomes a crash there. Ask now, while there is
                // still a log line to attach the answer to.
                crate::cloudfiles::warn_if_dehydrated(&app, &cfg);
            }

            if !cfg.integrity_watch {
                // Turned off mid-session: forget the last answer so the UI shows "not
                // watching" rather than a stale verdict from an hour ago.
                if let Ok(mut slot) = last().lock() {
                    *slot = Report::default();
                }
                scanned = false;
                tokio::time::sleep(IDLE_POLL).await;
                continue;
            }

            if running && !scanned {
                // A new session is the natural moment to pick up rules published since the
                // app started — someone may have been playing for eight hours.
                refresh_rules(&app).await;
            }
            if !running && scanned {
                // The session is over, so the verdict is about a game that no longer exists.
                if let Ok(mut slot) = last().lock() {
                    *slot = Report::default();
                }
                let _ = app.emit(EVENT, Report::default());
            }
            scanned = running;

            if running {
                scan_once(&app);
                tokio::time::sleep(SCAN_EVERY).await;
            } else {
                tokio::time::sleep(IDLE_POLL).await;
            }
        }
    });
}

/// Pull the live rule list, keeping the current one if the fetch fails.
pub async fn refresh_rules(app: &AppHandle) {
    match integrity::refresh_rules(app).await {
        Ok(fresh) => {
            if let Ok(mut slot) = rules().lock() {
                *slot = fresh;
            }
        }
        Err(e) => log::debug!("[integrity] couldn't refresh the rule list: {e:#}"),
    }
}

/// One rider's verdict, as an admin sees it.
#[derive(Debug, Clone, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiderIntegrity {
    pub rider_name: String,
    #[serde(default)]
    pub guid: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub rules_version: u32,
    #[serde(default)]
    pub suspect_count: usize,
    #[serde(default)]
    pub flagged: Vec<FlaggedSeen>,
    /// Unix ms of the client's last attestation. What tells an admin the difference between
    /// "reported clean" and "hasn't reported since they joined".
    #[serde(default)]
    pub reported_at: u64,
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedSeen {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub sha256: String,
}

/// Everyone on `server_id` who has published a verdict.
///
/// An admin's read, and it is worth being clear about what it is not: an absence here is not
/// innocence and not guilt. A rider shows up only if their app is running, watching, and set
/// to report — so the list answers "who has attested", and the people missing from it are
/// exactly the ones this can say nothing about.
pub async fn server_reports(token: &str, server_id: &str) -> anyhow::Result<Vec<RiderIntegrity>> {
    #[derive(serde::Deserialize)]
    struct Body {
        riders: Vec<RiderIntegrity>,
    }
    let body: Body = reqwest::Client::new()
        .get(format!("{}/v1/integrity", crate::paintsync::control_plane()))
        .query(&[("server", server_id)])
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(body.riders)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::{Finding, Kind, Reason};

    fn finding(name: &str, severity: Severity) -> Finding {
        Finding {
            kind: Kind::Module,
            name: name.into(),
            path: format!("C:\\Users\\rider\\Secret Work Tools\\{name}"),
            reason: if severity == Severity::Flagged {
                Reason::RuleName
            } else {
                Reason::ForeignModule
            },
            severity,
            sha256: "d34d".into(),
            label: if severity == Severity::Flagged { "Kaizo trainer".into() } else { String::new() },
        }
    }

    fn attestation(findings: Vec<Finding>) -> Attestation {
        let report = Report {
            verdict: Verdict::Flagged,
            findings,
            scanned_at: 1,
            rules_version: 9,
            modules_scanned: 120,
            game_running: true,
        };
        Attestation {
            verdict: report.verdict,
            rules_version: report.rules_version,
            suspect_count: report
                .findings
                .iter()
                .filter(|f| f.severity == Severity::Suspect)
                .count(),
            flagged: report
                .findings
                .iter()
                .filter(|f| f.severity == Severity::Flagged)
                .map(|f| FlaggedItem {
                    name: f.name.clone(),
                    label: f.label.clone(),
                    sha256: f.sha256.clone(),
                })
                .collect(),
        }
    }

    /// The privacy line this feature stands on. A Suspect finding is "software we don't
    /// recognise", which is not evidence of anything and is nobody else's business — it is
    /// counted, never named. Anything that changes this test is changing what the app tells
    /// a stranger about its user's machine.
    #[test]
    fn an_attestation_counts_suspects_without_naming_them() {
        let sent = attestation(vec![
            finding("kaizo.dll", Severity::Flagged),
            finding("employer_vpn_hook.dll", Severity::Suspect),
            finding("some_other_overlay.dll", Severity::Suspect),
        ]);
        let json = serde_json::to_string(&sent).unwrap();

        assert_eq!(sent.suspect_count, 2);
        assert!(!json.contains("employer_vpn_hook"), "a suspect module was named: {json}");
        assert!(!json.contains("some_other_overlay"), "a suspect module was named: {json}");
        assert!(!json.contains("Secret Work Tools"), "a path leaked: {json}");
        // The known cheat is named, which is the whole point of reporting.
        assert!(json.contains("kaizo.dll"));
        assert!(json.contains("Kaizo trainer"));
    }

    /// A clean machine sends a clean attestation and nothing else — no empty finding
    /// objects, no module list.
    #[test]
    fn a_clean_attestation_carries_nothing_about_the_machine() {
        let sent = attestation(vec![]);
        assert_eq!(sent.suspect_count, 0);
        assert!(sent.flagged.is_empty());
        let json = serde_json::to_string(&sent).unwrap();
        assert!(!json.contains(".dll"), "{json}");
    }

    /// The wire shape the control plane parses, in camelCase like every other endpoint.
    #[test]
    fn an_attestation_serializes_in_camel_case() {
        let json = serde_json::to_string(&attestation(vec![finding("k.dll", Severity::Flagged)]))
            .unwrap();
        assert!(json.contains("\"rulesVersion\":9"), "{json}");
        assert!(json.contains("\"suspectCount\":0"), "{json}");
        assert!(json.contains("\"verdict\":\"flagged\""), "{json}");
    }
}
