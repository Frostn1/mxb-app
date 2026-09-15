//! Whether it's the master server or it's you.
//!
//! MX Bikes answers a dead master server with `connection timeout` and nothing else. That
//! string is the same one you get for a firewall rule, a bad DNS server, a captive portal and
//! a router that needs power-cycling — so the game has told the player, in effect, that
//! something somewhere is wrong, and left them to guess which. What happens next is always the
//! same: they spend twenty minutes on their own machine, someone posts a troubleshooting list,
//! and by the time anybody works out the master was down for ten minutes it is already back.
//!
//! The app is in a position nothing else is. It talks to the master every time the Servers tab
//! opens, and so does every other install. So it does two things here:
//!
//! - **Reports** whether its own fetch worked, as one anonymous bit. `control-plane/src/masterstatus.ts`
//!   adds those up across installs.
//! - **Reads** what everyone else is seeing, which is the only thing that can actually answer
//!   "is it me". One machine failing knows nothing; one machine failing while twenty others
//!   are fine knows a great deal, and so does one failing alongside twenty others.
//!
//! Reporting rides on the existing anonymous-stats setting — off means nothing is sent.
//! Reading does not: somebody who shares none of their own data still gets told it isn't their
//! firewall, because the reason to withhold a report is privacy and the reason to read the
//! answer is that their game is broken.
//!
//! ## Why the reason is guessed from a string
//!
//! [`classify`] reads the error message. That is normally the wrong way round, and it is here
//! because the master-server protocol lives in the local-only `worldnet` module: a public build
//! doesn't have it, so the failure cannot be an enum this file matches on without moving the
//! protocol's vocabulary into the open tree. Everything it can't place becomes `error`, which
//! the control plane counts perfectly well — the reason is colour on the answer, never the
//! answer itself, and the answer is the count.

use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::config;
use crate::paintsync::control_plane;

/// How long any one check waits. Six of these run for a self-test the player is watching, so
/// the whole thing has to finish inside the time somebody will sit still for.
const CHECK_TIMEOUT: Duration = Duration::from_secs(6);

/// Why a fetch failed, in the vocabulary `control-plane/src/validate.ts` accepts. A closed list
/// on both sides: it is reported by every install and read back on a public page, so free text
/// here is free text that eventually carries somebody's address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    Timeout,
    Refused,
    Dns,
    Auth,
    Protocol,
    Offline,
    /// A build with no server browser in it. Not an outage, and the control plane drops it
    /// rather than counting it — see `readWindow` there.
    Unsupported,
    Error,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Reason::Timeout => "timeout",
            Reason::Refused => "refused",
            Reason::Dns => "dns",
            Reason::Auth => "auth",
            Reason::Protocol => "protocol",
            Reason::Offline => "offline",
            Reason::Unsupported => "unsupported",
            Reason::Error => "error",
        }
    }
}

/// Place a failure message in the closed list.
///
/// The arms are ordered by how *diagnostic* each phrase is, not by how likely the failure is,
/// because the real messages routinely satisfy two arms at once. `worldnet` says "the master
/// didn't answer the login" for a receive timeout and "the master server refused the login" for
/// a rejection: both name the login, only one is an auth failure, so the timeout wording is
/// tested first. The same goes for "login send failed", which is a socket that couldn't put a
/// packet on the wire and has nothing to do with logging in.
///
/// Nothing here is protocol knowledge — these are ordinary English phrases, and a message that
/// fits none of them becomes `error`, which the control plane counts exactly as well. The
/// reason is colour on the answer; the answer is the count.
pub fn classify(message: &str) -> Reason {
    let m = message.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| m.contains(n));

    if has(&["isn't included", "not included", "unsupported"]) {
        // Must come first: this is a build without the browser, and it is never an outage.
        Reason::Unsupported
    } else if has(&["dns", "resolve", "no such host", "name or service not known"]) {
        Reason::Dns
    } else if has(&["couldn't open a socket", "send failed", "no network", "network is down"]) {
        // A socket this machine couldn't open or couldn't send from. Local, and nothing to do
        // with whoever was being sent to.
        Reason::Offline
    } else if has(&["sent no servers", "parse", "malformed", "unexpected", "protocol", "garbage"]) {
        // Something answered and it wasn't what we asked for. Before the login arms, because
        // "accepted the login but sent no servers" is a protocol failure that names the login.
        Reason::Protocol
    } else if has(&["timed out", "timeout", "timing out", "answer", "stopped talking"]) {
        Reason::Timeout
    } else if has(&["login", "steam", "ticket", "auth", "unauthor", "forbidden"]) {
        Reason::Auth
    } else if has(&["refused", "reset", "unreachable"]) {
        Reason::Refused
    } else if has(&["offline"]) {
        Reason::Offline
    } else {
        Reason::Error
    }
}

#[derive(Serialize)]
struct Probe<'a> {
    #[serde(rename = "installId")]
    install_id: &'a str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

/// Tell the control plane how the last server-list fetch went.
///
/// Spawned and forgotten by design: this is a bit of telemetry attached to something the player
/// is waiting on, and it must never be able to slow the Servers tab down or make it fail. A
/// report that doesn't send is simply a report that doesn't send — the next refresh sends
/// another one, and the window is ten minutes wide.
pub fn report(app: &AppHandle, outcome: Result<(), &str>) {
    let cfg = match config::load(app) {
        Ok(cfg) => cfg,
        Err(_) => return,
    };
    // The same gate, and the same re-read-before-sending discipline, as the usage counters.
    if !mxb_core::usage::allowed(&cfg) || cfg.install_id.trim().is_empty() {
        return;
    }
    let install_id = cfg.install_id.trim().to_string();
    let reason = outcome.err().map(|message| classify(message));

    // An unsupported build has nothing to say about the master and says nothing: reporting it
    // would spend a request to tell the control plane something it explicitly discards.
    if reason == Some(Reason::Unsupported) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let body = Probe {
            install_id: &install_id,
            ok: reason.is_none(),
            reason: reason.map(Reason::as_str),
        };
        let sent = match client() {
            Ok(client) => {
                client
                    .post(format!("{}/v1/master-status", control_plane()))
                    .json(&body)
                    .send()
                    .await
            }
            Err(e) => {
                log::debug!("[status] no HTTP client: {e}");
                return;
            }
        };
        if let Err(e) = sent {
            log::debug!("[status] probe didn't send: {e}");
        }
    });
}

/// What the control plane says everyone is seeing. Mirrors `StatusBody` there; extra fields it
/// grows are ignored rather than fatal, so an older app keeps working against a newer Worker.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterStatus {
    /// `up`, `degraded`, `down` or `unknown`.
    pub state: String,
    /// The sentence, already written, in English. Rendered as a detail line beneath the app's
    /// own translated heading: it carries the numbers, and it is the same wording the status
    /// page and a Discord bot show, which is the point of composing it in one place.
    pub summary: String,
    pub master: MasterCounts,
    pub window_minutes: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterCounts {
    pub installs: u32,
    pub failing: u32,
    #[serde(default)]
    pub failing_for_minutes: Option<u32>,
}

/// Ask the control plane what everyone else is seeing.
///
/// Every failure here is `None` rather than an error. The caller is a banner explaining another
/// failure; a banner that replaces "the master isn't answering" with "couldn't reach the thing
/// that would have told you whether the master is answering" has made the player's afternoon
/// worse, not better.
pub async fn fetch(cfg_timeout: Duration) -> Option<MasterStatus> {
    let client = client_with(cfg_timeout).ok()?;
    let res = client
        .get(format!("{}/v1/status", control_plane()))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    res.json::<MasterStatus>().await.ok()
}

/// One line of the self-test.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// Which check this is. The frontend has a translated label per id; nothing here is shown
    /// to the player as it stands.
    pub id: &'static str,
    /// `ok`, `fail`, or `skip` for a check this build or this install can't run.
    pub state: &'static str,
    /// The technical half — a host name, an error, a count. Shown small, beneath the label,
    /// and deliberately not translated: it is for pasting into Discord.
    pub detail: String,
}

/// What the self-test concluded, which is the only part most people will read.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTest {
    /// `upstream` (it's the master, not you), `local` (it's this machine), `fine` (nothing is
    /// wrong right now) or `unknown` (not enough to say).
    pub verdict: &'static str,
    pub checks: Vec<Check>,
    /// The control plane's own sentence, when it had one.
    pub summary: Option<String>,
}

/// Run every check this machine can run, and say what they add up to.
///
/// The order matters: it walks outwards from the machine, so the first thing that fails is the
/// nearest thing to the player, and somebody reading the list top to bottom stops at the line
/// that is actually their problem. The master fetch itself is passed in rather than done here,
/// because it lives behind `cfg(worldnet)` and this module is in the public tree.
pub async fn self_test(app: AppHandle, master: Result<usize, String>) -> SelfTest {
    let mut checks = Vec::new();

    let hosts = {
        let cfg = config::load(&app).unwrap_or_default();
        config::master_servers(&cfg)
    };
    let host = hosts.first().cloned().unwrap_or_else(|| config::DEFAULT_MASTER.to_string());

    // 1. Is there any internet at all? Asked first because every answer below it is meaningless
    //    without one, and because "your wifi is off" is a real and very common cause of exactly
    //    the timeout that brought somebody here.
    let online = reachable_control_plane().await;
    checks.push(Check {
        id: "internet",
        state: if online { "ok" } else { "fail" },
        detail: if online {
            "api.mxbsecure.com".into()
        } else {
            "api.mxbsecure.com didn't answer".into()
        },
    });

    // 2. Does the master's name resolve? A DNS server that has stopped answering looks exactly
    //    like an outage from inside the game, and is the single most common cause that really is
    //    the player's to fix.
    let dns = resolve(&host).await;
    checks.push(match &dns {
        Ok(addr) => Check { id: "dns", state: "ok", detail: format!("{host} → {addr}") },
        Err(e) => Check { id: "dns", state: "fail", detail: format!("{host}: {e}") },
    });

    // 3. Can this machine send UDP at all? The master is spoken to over UDP, and a firewall or a
    //    VPN that drops outbound datagrams silently produces the timeout and nothing else. A
    //    send that succeeds proves the socket was allowed out, not that anything received it —
    //    the detail line says so, because a check that overstates what it proves is worse than
    //    no check.
    let udp = dns.as_ref().ok().map(|addr| udp_send(addr));
    checks.push(match udp {
        Some(Ok(())) => Check { id: "udp", state: "ok", detail: "outbound UDP allowed".into() },
        Some(Err(e)) => Check { id: "udp", state: "fail", detail: e },
        None => Check { id: "udp", state: "skip", detail: "needs the address first".into() },
    });

    // 4. Our own fetch — the thing the player actually noticed.
    checks.push(match &master {
        Ok(count) => Check { id: "master", state: "ok", detail: format!("{count} servers listed") },
        Err(e) if classify(e) == Reason::Unsupported => {
            Check { id: "master", state: "skip", detail: e.clone() }
        }
        Err(e) => Check { id: "master", state: "fail", detail: e.clone() },
    });

    // 5. Everyone else. Last because it is the only one that can overturn the rest: a machine
    //    whose own checks all passed and whose fetch still failed has learned nothing until it
    //    knows whether twenty other people are failing too.
    let status = fetch(CHECK_TIMEOUT).await;
    checks.push(match &status {
        Some(s) => Check {
            id: "others",
            state: if s.state == "down" || s.state == "degraded" { "fail" } else { "ok" },
            detail: format!(
                "{} of {} apps failing in the last {} min",
                s.master.failing, s.master.installs, s.window_minutes
            ),
        },
        None => Check { id: "others", state: "skip", detail: "couldn't ask".into() },
    });

    SelfTest {
        verdict: verdict(&master, online, &status),
        summary: status.as_ref().map(|s| s.summary.clone()),
        checks,
    }
}

/// What the checks add up to.
///
/// Two rules, in this order. A fetch of our own that worked settles it: whatever is true for
/// everyone else, this machine has the server list in its hands, and telling somebody their
/// connection is broken while they are looking at a list of servers would be absurd. Failing
/// that, a widespread failure outranks everything local — when the master is down every machine
/// in the world looks broken from the inside, and "check your firewall" is precisely the advice
/// that wastes an afternoon. Only once both are ruled out is a local failure worth naming.
fn verdict(master: &Result<usize, String>, online: bool, status: &Option<MasterStatus>) -> &'static str {
    if master.is_ok() {
        return "fine";
    }
    let widespread = matches!(status.as_ref().map(|s| s.state.as_str()), Some("down") | Some("degraded"));
    if widespread {
        return "upstream";
    }
    // A build with no browser has not observed anything; saying "it's your machine" on the
    // strength of a fetch that never happened would be a guess wearing a verdict's clothes.
    if master.as_ref().err().map(|e| classify(e)) == Some(Reason::Unsupported) {
        return "unknown";
    }
    if !online {
        return "local";
    }
    // Our fetch failed, the internet is up, and the crowd is either fine or too quiet to say.
    // Both of those mean something here rather than out there — but only when the crowd was
    // loud enough to be believed.
    match status.as_ref().map(|s| s.state.as_str()) {
        Some("up") => "local",
        _ => "unknown",
    }
}

/// Does the control plane answer? Stands in for "is this machine on the internet", and is a
/// better stand-in than pinging something famous: it is the host the app already depends on, so
/// a false alarm here is a real problem either way.
async fn reachable_control_plane() -> bool {
    let Ok(client) = client_with(CHECK_TIMEOUT) else {
        return false;
    };
    matches!(
        client.get(format!("{}/health", control_plane())).send().await,
        Ok(res) if res.status().is_success()
    )
}

/// Resolve `host:port` to its first address.
///
/// On a blocking pool: `ToSocketAddrs` is the platform resolver and will sit on a dead DNS
/// server for as long as the system says to, which is exactly the case being tested for.
async fn resolve(host: &str) -> Result<String, String> {
    let host = host.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let with_port = if host.contains(':') { host.clone() } else { format!("{host}:54200") };
        match with_port.to_socket_addrs() {
            Ok(mut addrs) => addrs
                .next()
                .map(|a| a.to_string())
                .ok_or_else(|| "resolved to no addresses".to_string()),
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

/// Bind a UDP socket and send one datagram towards the master.
///
/// Deliberately not the game's protocol — this module is public and the protocol is not, and
/// the question here is narrower anyway: was this machine *allowed* to put a datagram on the
/// wire. A blocked outbound UDP rule fails at `send_to` with a clear error, and that failure is
/// a real and unguessable cause of the timeout. Nothing is read back, so nothing here can be
/// mistaken for proof the master answered.
fn udp_send(addr: &str) -> Result<(), String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("couldn't open a UDP socket: {e}"))?;
    socket
        .set_write_timeout(Some(CHECK_TIMEOUT))
        .map_err(|e| e.to_string())?;
    // A single zero byte. The master will make nothing of it and is meant not to: an
    // unsolicited datagram from an unknown port is something every server on the internet
    // receives constantly, and it is the send that is being tested, not the reply.
    socket
        .send_to(&[0u8], addr)
        .map(|_| ())
        .map_err(|e| format!("outbound UDP blocked: {e}"))
}

fn client() -> anyhow::Result<reqwest::Client> {
    client_with(Duration::from_secs(10))
}

fn client_with(timeout: Duration) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(timeout).build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_the_browser_is_never_an_outage() {
        // The one classification that must not be wrong: counted as a failure it would peg the
        // control plane's ratio high for ever on every install that cannot even ask.
        assert_eq!(
            classify("The server browser isn't included in this build."),
            Reason::Unsupported
        );
    }

    #[test]
    fn the_common_failures_land_where_they_should() {
        assert_eq!(classify("operation timed out"), Reason::Timeout);
        assert_eq!(classify("Connection refused (os error 111)"), Reason::Refused);
        assert_eq!(classify("failed to lookup address: no such host"), Reason::Dns);
        assert_eq!(classify("couldn't get a Steam auth ticket"), Reason::Auth);
        assert_eq!(classify("unexpected reply from the master"), Reason::Protocol);
    }

    /// The messages `worldnet` actually produces. It is not in the public tree, so this is the
    /// only place the two vocabularies are held against each other — and three of these fit
    /// more than one arm, which is what the ordering is for.
    #[test]
    fn the_real_messages_land_where_they_should() {
        assert_eq!(classify("couldn't resolve master.mx-bikes.com:54200: no such host"), Reason::Dns);
        assert_eq!(classify("master.mx-bikes.com:54200 resolved to no address"), Reason::Dns);
        assert_eq!(classify("couldn't open a socket: permission denied"), Reason::Offline);
        // Names the login, and is a socket that couldn't send.
        assert_eq!(classify("login send failed: network is unreachable"), Reason::Offline);
        // Names the login, and is a receive timeout.
        assert_eq!(classify("the master didn't answer the login"), Reason::Timeout);
        assert_eq!(classify("None of the remembered servers answered."), Reason::Timeout);
        // Names the login, and is an auth rejection — the arm the two above must not steal.
        assert_eq!(classify("The master server refused the login: banned."), Reason::Auth);
        assert_eq!(classify("The master server rejected the login."), Reason::Auth);
        // Names the login, and is neither: something answered with nothing in it.
        assert_eq!(
            classify("The master server accepted the login but sent no servers."),
            Reason::Protocol
        );
    }

    #[test]
    fn a_message_naming_both_a_host_and_a_timeout_reads_as_dns() {
        // Ordering, on purpose: a resolver failure that mentions a timeout is still a resolver
        // failure, and it is the one the player can do something about.
        assert_eq!(
            classify("failed to resolve master.mx-bikes.com: timed out"),
            Reason::Dns
        );
    }

    #[test]
    fn anything_unplaceable_is_error_rather_than_a_guess() {
        assert_eq!(classify("something went wrong"), Reason::Error);
        assert_eq!(classify(""), Reason::Error);
    }

    #[test]
    fn a_widespread_outage_outranks_every_local_check() {
        // Telling somebody to look at their firewall while the master is down is exactly the
        // advice that wastes their afternoon.
        let down = Some(MasterStatus {
            state: "down".into(),
            summary: "…".into(),
            master: MasterCounts { installs: 25, failing: 23, failing_for_minutes: Some(9) },
            window_minutes: 10,
        });
        assert_eq!(verdict(&Err("timed out".into()), false, &down), "upstream");
    }

    #[test]
    fn our_failure_alone_while_everyone_else_is_fine_is_ours() {
        let up = Some(MasterStatus {
            state: "up".into(),
            summary: "…".into(),
            master: MasterCounts { installs: 25, failing: 1, failing_for_minutes: None },
            window_minutes: 10,
        });
        assert_eq!(verdict(&Err("timed out".into()), true, &up), "local");
    }

    #[test]
    fn a_quiet_crowd_yields_unknown_rather_than_blaming_the_player() {
        let quiet = Some(MasterStatus {
            state: "unknown".into(),
            summary: "…".into(),
            master: MasterCounts { installs: 1, failing: 1, failing_for_minutes: None },
            window_minutes: 10,
        });
        assert_eq!(verdict(&Err("timed out".into()), true, &quiet), "unknown");
        assert_eq!(verdict(&Err("timed out".into()), true, &None), "unknown");
    }

    #[test]
    fn a_working_fetch_is_fine_whatever_else_is_unreachable() {
        assert_eq!(verdict(&Ok(42), true, &None), "fine");
    }

    #[test]
    fn a_list_we_loaded_ourselves_beats_an_outage_everyone_else_is_having() {
        // Somebody looking at a list of servers must not be told their connection is down,
        // however bad everyone else's ten minutes has been.
        let down = Some(MasterStatus {
            state: "down".into(),
            summary: "…".into(),
            master: MasterCounts { installs: 25, failing: 23, failing_for_minutes: Some(9) },
            window_minutes: 10,
        });
        assert_eq!(verdict(&Ok(42), true, &down), "fine");
    }

    #[test]
    fn a_build_without_the_browser_concludes_nothing() {
        let e = Err("The server browser isn't included in this build.".to_string());
        assert_eq!(verdict(&e, true, &None), "unknown");
    }
}
