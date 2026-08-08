//! The one-line blob that adds this server to the app.
//!
//! Pairing used to be three fields typed into the app by hand: an address, a token copied
//! out of `agent.json`, and a name the operator invented. Two of those the agent already
//! knows, and the third it can read from the server's own config — so it prints all of it
//! once, as a single string, and the app takes a paste instead of a form.
//!
//! The blob is base64 so it survives a copy out of a terminal in one piece: a raw JSON
//! object wraps across lines, invites a half-selection, and looks like something to edit
//! rather than something to paste whole.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr, UdpSocket};

/// Marks the string as ours, so a mis-paste fails with "that isn't a pairing code" rather
/// than as a confusing base64 or JSON error.
pub const PREFIX: &str = "mxb-agent:";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pairing {
    /// Base URL the app should call, e.g. `http://203.0.113.10:8787`.
    pub url: String,
    pub token: String,
}

impl Pairing {
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!("{PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(json))
    }

    pub fn decode(blob: &str) -> Result<Self, String> {
        let body = blob
            .trim()
            .strip_prefix(PREFIX)
            .ok_or("that doesn't look like a pairing code")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body.trim())
            .map_err(|_| "that pairing code is damaged".to_string())?;
        let pairing: Pairing =
            serde_json::from_slice(&bytes).map_err(|_| "that pairing code is damaged".to_string())?;
        if pairing.url.trim().is_empty() || pairing.token.trim().is_empty() {
            return Err("that pairing code is incomplete".into());
        }
        Ok(pairing)
    }
}

/// The URL an app on another machine should use to reach this agent.
///
/// `listen` is usually `0.0.0.0:8787`, which is a bind address and not something anyone can
/// connect to, so the host part has to be discovered. `configured` wins when the operator
/// set one — behind NAT or a reverse proxy, the address the agent can see is not the
/// address the app must use, and only a human knows that.
pub fn public_url(listen: &str, configured: Option<&str>) -> String {
    if let Some(url) = configured.map(str::trim).filter(|u| !u.is_empty()) {
        return url.trim_end_matches('/').to_string();
    }
    let port = listen.rsplit_once(':').map(|(_, p)| p).unwrap_or("8787");
    let host = match listen.rsplit_once(':').map(|(h, _)| h) {
        // A wildcard bind tells us nothing about how to reach the box.
        Some("0.0.0.0") | Some("") | Some("[::]") | Some("::") | None => primary_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        Some(host) => host.to_string(),
    };
    format!("http://{host}:{port}")
}

/// The address of the interface this box would use to reach the outside world.
///
/// Connecting a UDP socket sends nothing — it only makes the kernel pick a route and bind a
/// local address — so this names the primary interface without traffic, without a DNS
/// lookup, and without waiting on a host that may be firewalled off.
fn primary_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("198.51.100.1:80".parse::<SocketAddr>().ok()?).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Pairing {
        Pairing { url: "http://203.0.113.10:8787".into(), token: "s3cret-token".into() }
    }

    #[test]
    fn a_blob_survives_a_round_trip() {
        assert_eq!(Pairing::decode(&sample().encode()).unwrap(), sample());
    }

    #[test]
    fn tolerates_the_whitespace_a_copy_paste_picks_up() {
        let blob = format!("  {}\n", sample().encode());
        assert_eq!(Pairing::decode(&blob).unwrap(), sample());
    }

    #[test]
    fn refuses_anything_that_is_not_a_pairing_code() {
        // Each of these is a plausible mis-paste, and each has to say so rather than
        // producing a half-filled server row.
        for bad in [
            "http://203.0.113.10:8787",
            "s3cret-token",
            "",
            "mxb-agent:not-base64!!",
            // Valid base64, but not a pairing object.
            "mxb-agent:eyJoaSI6InRoZXJlIn0=",
        ] {
            assert!(Pairing::decode(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn refuses_a_blob_missing_half_of_itself() {
        let empty_token = Pairing { url: "http://x:1".into(), token: String::new() };
        assert!(Pairing::decode(&empty_token.encode()).is_err());
    }

    #[test]
    fn a_configured_url_wins_over_anything_discovered() {
        // The NAT case: what the box sees itself as is not what the app must call.
        assert_eq!(
            public_url("0.0.0.0:8787", Some("https://mx.example.com")),
            "https://mx.example.com"
        );
        // A trailing slash would double the separator when a path is appended.
        assert_eq!(public_url("0.0.0.0:8787", Some("http://host:9/")), "http://host:9");
    }

    #[test]
    fn keeps_an_explicit_bind_host_and_its_port() {
        assert_eq!(public_url("203.0.113.10:9000", None), "http://203.0.113.10:9000");
    }

    #[test]
    fn a_wildcard_bind_keeps_its_port() {
        // The host is whatever this machine's primary interface is, so only the port is
        // predictable — but it must be carried over rather than defaulted.
        assert!(public_url("0.0.0.0:9123", None).ends_with(":9123"));
        assert!(public_url("0.0.0.0:9123", None).starts_with("http://"));
    }

    #[test]
    fn an_empty_configured_url_is_ignored_rather_than_used() {
        assert!(public_url("203.0.113.10:8787", Some("   ")).contains("203.0.113.10"));
    }
}
