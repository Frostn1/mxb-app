//! Talking to `mxb-agent` on a dedicated server host.
//!
//! The app manages a server by calling its agent, never by calling a cloud provider — a
//! desktop app that shipped provider credentials could create infrastructure on any
//! machine it ran on. What lives here is a bearer token for one server the player already
//! administers, which is the smallest thing that can do the job.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A dedicated server the player administers, as stored in the app config.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerRef {
    /// Stable handle for the UI; the agent never sees it.
    pub id: String,
    /// Display label, free text.
    pub name: String,
    /// Base URL of the agent, e.g. `http://203.0.113.10:8787`.
    pub url: String,
    /// Bearer token from the host's `agent.json`.
    pub token: String,
}

/// What `mxb-agent` prints at startup for the operator to paste in.
///
/// Adding a server was three fields — address, token, name — two of which the agent already
/// knows and one of which its `.ini` holds. This is the other end of that: the agent emits
/// them as one string, and the app unpacks it instead of asking.
///
/// Duplicated rather than shared with the agent because the two ship as separate binaries
/// on separate machines and update independently. The format is a wire contract, so it is
/// pinned by tests on both sides instead of by a common type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pairing {
    pub url: String,
    pub token: String,
}

/// Marks the string as an agent's, so a mis-paste gets a straight answer rather than a
/// base64 error.
const PAIRING_PREFIX: &str = "mxb-agent:";

pub fn parse_pairing(blob: &str) -> Result<Pairing, String> {
    use base64::Engine;

    let body = blob
        .trim()
        .strip_prefix(PAIRING_PREFIX)
        .ok_or("That doesn't look like a pairing code — copy the whole line the agent printed.")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|_| "That pairing code looks damaged — copy the whole line again.".to_string())?;
    let pairing: Pairing = serde_json::from_slice(&bytes)
        .map_err(|_| "That pairing code looks damaged — copy the whole line again.".to_string())?;
    if pairing.token.trim().is_empty() {
        return Err("That pairing code has no token in it.".into());
    }
    // Run it through the same check every request uses, so a bad URL is rejected at paste
    // time rather than becoming a row that can never load.
    endpoint(&pairing.url, "/status")?;
    Ok(Pairing { url: pairing.url.trim().trim_end_matches('/').to_string(), token: pairing.token })
}

/// Requests are short-lived on purpose: every endpoint either reads state or restarts a
/// process, so a slow one means the host is unreachable rather than busy. `PUT /config`
/// restarts the game, so it gets the longer budget.
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// The lifecycle verbs the app may invoke, as an enum rather than a string so that a
/// caller cannot turn a typo — or a crafted argument — into some other agent endpoint.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Start,
    Stop,
    Restart,
}

impl Action {
    fn path(self) -> &'static str {
        match self {
            Action::Start => "/start",
            Action::Stop => "/stop",
            Action::Restart => "/restart",
        }
    }
}

/// Join the configured base URL with an agent path.
///
/// Rejects anything that isn't plain http/https, and trims a trailing slash so that a base
/// entered as `http://host:8787/` doesn't produce a doubled separator the agent would 404.
pub fn endpoint(base: &str, path: &str) -> Result<String, String> {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("This server has no agent address set.".into());
    }
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(format!(
            "\"{base}\" isn't a valid agent address — it should start with http:// or https://."
        ));
    }
    // A base carrying its own query or fragment would silently swallow the path.
    if base.contains('?') || base.contains('#') {
        return Err(format!("\"{base}\" isn't a valid agent address."));
    }
    Ok(format!("{base}{path}"))
}

async fn send(
    server: &ServerRef,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let url = endpoint(&server.url, path)?;
    if server.token.trim().is_empty() {
        return Err("This server has no agent token set.".into());
    }
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("couldn't build the HTTP client: {e}"))?;

    let mut req = client.request(method, &url).bearer_auth(&server.token);
    if let Some(json) = &body {
        req = req.json(json);
    }
    let resp = req.send().await.map_err(|e| {
        // The common failure by far is an unreachable host, and reqwest's own wording for
        // that is noise to a player. Name the address instead.
        if e.is_timeout() || e.is_connect() {
            format!("Couldn't reach the server agent at {}.", server.url)
        } else {
            format!("{e}")
        }
    })?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("The agent rejected the token for this server.".into());
    }
    if !status.is_success() {
        // The agent reports failures as {"error": "..."}; fall back to the raw body.
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_else(|| text.clone());
        return Err(format!("The agent returned {status}: {detail}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("The agent sent a response we couldn't read: {e}"))
}

pub async fn status(server: &ServerRef) -> Result<serde_json::Value, String> {
    send(server, reqwest::Method::GET, "/status", None, READ_TIMEOUT).await
}

pub async fn act(server: &ServerRef, action: Action) -> Result<serde_json::Value, String> {
    send(server, reqwest::Method::POST, action.path(), None, WRITE_TIMEOUT).await
}

/// Who is connected right now, as the host's own log reports them.
pub async fn players(server: &ServerRef) -> Result<Vec<Player>, String> {
    let value = send(server, reqwest::Method::GET, "/players", None, READ_TIMEOUT).await?;
    let list = value.get("players").cloned().unwrap_or_default();
    serde_json::from_value(list).map_err(|e| format!("The agent sent a roster we couldn't read: {e}"))
}

/// A connected rider, as the agent reads them out of the dedicated server's log.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: i64,
    pub name: String,
    pub guid: String,
}

/// The tracks the host has installed — the values its `.ini` will actually accept.
pub async fn tracks(server: &ServerRef) -> Result<Vec<String>, String> {
    let value = send(server, reqwest::Method::GET, "/tracks", None, READ_TIMEOUT).await?;
    let list = value.get("tracks").cloned().unwrap_or_default();
    serde_json::from_value(list)
        .map_err(|e| format!("The agent sent a track list we couldn't read: {e}"))
}

/// Change server settings. The agent restarts the game, since it reads its `.ini` only at
/// startup, so this shares the write timeout.
pub async fn set_config(
    server: &ServerRef,
    patch: serde_json::Value,
) -> Result<serde_json::Value, String> {
    send(server, reqwest::Method::PUT, "/config", Some(patch), WRITE_TIMEOUT).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent's own encoding, reproduced here so the test pins the wire format rather
    /// than whatever this crate happens to produce.
    fn blob(json: &str) -> String {
        use base64::Engine;
        format!("mxb-agent:{}", base64::engine::general_purpose::STANDARD.encode(json))
    }

    #[test]
    fn unpacks_a_pairing_code_the_agent_printed() {
        let got =
            parse_pairing(&blob(r#"{"url":"http://203.0.113.10:8787","token":"s3cret"}"#)).unwrap();
        assert_eq!(got.url, "http://203.0.113.10:8787");
        assert_eq!(got.token, "s3cret");
    }

    #[test]
    fn tolerates_the_whitespace_a_terminal_copy_brings_along() {
        let raw = blob(r#"{"url":"http://host:8787/","token":"t"}"#);
        let got = parse_pairing(&format!("\n  {raw}  \n")).unwrap();
        // The trailing slash is dropped here so it can't double the separator later.
        assert_eq!(got.url, "http://host:8787");
    }

    #[test]
    fn refuses_a_pairing_code_carrying_an_address_we_would_never_call() {
        // The URL becomes a request target, so the scheme check has to happen at paste
        // time — not once it's already saved in the list.
        for bad in [
            r#"{"url":"file:///etc/passwd","token":"t"}"#,
            r#"{"url":"ftp://host","token":"t"}"#,
            r#"{"url":"","token":"t"}"#,
        ] {
            assert!(parse_pairing(&blob(bad)).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn refuses_a_pairing_code_with_no_token() {
        assert!(parse_pairing(&blob(r#"{"url":"http://host:8787","token":"  "}"#)).is_err());
    }

    #[test]
    fn refuses_things_that_are_not_pairing_codes_at_all() {
        for bad in ["", "http://203.0.113.10:8787", "mxb-agent:!!!", &blob(r#"{"hi":"there"}"#)] {
            assert!(parse_pairing(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn builds_an_endpoint_from_a_plain_base() {
        assert_eq!(endpoint("http://203.0.113.10:8787", "/status").unwrap(), "http://203.0.113.10:8787/status");
    }

    #[test]
    fn tolerates_a_trailing_slash_and_surrounding_space() {
        // Pasting an address usually brings one or both along.
        assert_eq!(endpoint("  http://host:8787/  ", "/status").unwrap(), "http://host:8787/status");
    }

    #[test]
    fn rejects_schemes_that_are_not_http() {
        for bad in ["file:///etc/passwd", "ftp://host", "host:8787", ""] {
            assert!(endpoint(bad, "/status").is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn rejects_a_base_that_would_swallow_the_path() {
        // "http://host/?x=1" + "/status" produces a URL whose path is not /status.
        assert!(endpoint("http://host:8787/?x=1", "/status").is_err());
        assert!(endpoint("http://host:8787/#frag", "/status").is_err());
    }

    #[test]
    fn actions_map_to_their_own_endpoints() {
        assert_eq!(Action::Start.path(), "/start");
        assert_eq!(Action::Stop.path(), "/stop");
        assert_eq!(Action::Restart.path(), "/restart");
    }

    #[tokio::test]
    async fn a_blank_token_fails_before_any_request_goes_out() {
        let server = ServerRef {
            url: "http://127.0.0.1:1".into(),
            token: "  ".into(),
            ..Default::default()
        };
        let err = status(&server).await.unwrap_err();
        assert!(err.contains("token"), "unhelpful error: {err}");
    }
}
