//! Servers hosted by mxbsecure. servers.mxbsecure.com mints a one-time claim code for a server
//! its owner runs there; this app swaps the code for a bearer at the control plane
//! (`POST /v1/hosted/claim`), keeps the bearer in the keychain like every other credential, and
//! from then on drives that one server through `/v1/hosted/servers/:id`.
//!
//! The control plane's address is fixed (debug builds may point elsewhere for `wrangler dev`),
//! and every path is built from a checked UUID, so nothing stored or linked can aim the bearer
//! at another host or route.

use crate::store::{self, Server, ServerKind};
use serde_json::{json, Value};

/// Where the control plane lives.
const CONTROL_PLANE: &str = "https://api.mxbsecure.com";

/// Debug builds only: a base URL to use instead, to try the flow against `wrangler dev`.
const CONTROL_PLANE_ENV: &str = "MXB_CONTROL_PLANE";

pub fn control_plane() -> String {
    if cfg!(debug_assertions) {
        if let Ok(base) = std::env::var(CONTROL_PLANE_ENV) {
            let base = base.trim().trim_end_matches('/');
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    CONTROL_PLANE.to_string()
}

/// One parameter out of a link's query string, undecoded: every value we accept is a plain
/// token, so a `%` means it isn't one.
fn link_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|pair| pair.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case(key)))
        .map(|(_, v)| v.trim())
}

/// The claim code in an `mxbservers://` link, or `None` if it isn't one of ours.
///
/// `mxbservers://hosted/claim?code=…` is the link the site opens; `mxbservers://connect?claim=…`
/// is the shape the control plane minted first, still accepted so an older link works.
pub fn parse_link(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("mxbservers://")?;
    let (route, query) = rest.split_once('?')?;
    let route = route.trim_end_matches('/');
    let code = if route.eq_ignore_ascii_case("hosted/claim") {
        link_param(query, "code")?
    } else if route.eq_ignore_ascii_case("connect") {
        link_param(query, "claim")?
    } else {
        return None;
    };
    store::valid_claim_code(code).then(|| code.to_string())
}

/// The control plane's error text for a failed call, or a plain fallback.
fn failure(code: u16, body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("mxbsecure answered HTTP {code}."))
}

async fn call(
    http: &reqwest::Client,
    method: reqwest::Method,
    path: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> Result<Value, String> {
    let mut request = http.request(method, format!("{}{path}", control_plane()));
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.header("Content-Type", "application/json").body(body.to_string());
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Couldn't reach mxbsecure: {e}"))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(failure(status, &text));
    }
    serde_json::from_str(&text).map_err(|_| "mxbsecure sent an answer this app can't read.".to_string())
}

/// What a claim hands back, checked: the bearer and the server it drives.
pub struct Claimed {
    pub token: String,
    pub hosted_id: String,
    pub name: String,
    pub address: Option<String>,
}

pub fn read_claim(body: &Value) -> Result<Claimed, String> {
    let token = body["token"].as_str().unwrap_or_default().trim();
    if !store::valid_agent_token(token) {
        return Err("mxbsecure sent no usable token.".into());
    }
    let server = &body["server"];
    let hosted_id = server["id"].as_str().unwrap_or_default();
    if !store::valid_hosted_id(hosted_id) {
        return Err("mxbsecure sent no usable server id.".into());
    }
    let name: String = server["name"].as_str().unwrap_or("Hosted server").trim().chars().take(64).collect();
    Ok(Claimed {
        token: token.to_string(),
        hosted_id: hosted_id.to_string(),
        name: if name.is_empty() { "Hosted server".into() } else { name },
        address: server["address"].as_str().map(str::to_string),
    })
}

/// Swap a claim code for a bearer and save the server, or refresh the one already linked to
/// the same hosted server (a second "Open in MSM" replaces its token rather than adding a copy).
pub async fn claim(http: &reqwest::Client, saved: &store::Store, code: &str) -> Result<Server, String> {
    let code = code.trim();
    if !store::valid_claim_code(code) {
        return Err("That code isn't valid.".into());
    }
    let body = call(http, reqwest::Method::POST, "/v1/hosted/claim", None, Some(json!({ "claim": code }))).await?;
    let claimed = read_claim(&body)?;
    let server = match saved.by_hosted_id(&claimed.hosted_id) {
        Some(existing) => Server {
            host: claimed.address.clone().unwrap_or(existing.host.clone()),
            ..existing
        },
        None => Server {
            id: store::new_id(),
            name: claimed.name.clone(),
            kind: ServerKind::Hosted,
            host: claimed.address.clone().unwrap_or_default(),
            agent_tls: true,
            ssh_port: 22,
            user: String::new(),
            key_path: None,
            observe_port: 9809,
            admin_port: None,
            log_path: String::new(),
            local: false,
            local_command: None,
            hosted_id: Some(claimed.hosted_id.clone()),
        },
    };
    store::validate(&server)?;
    store::set_token(&server.id, &claimed.token)?;
    saved.upsert(server.clone())?;
    Ok(server)
}

fn hosted_path(server: &Server, tail: &str) -> Result<String, String> {
    match (&server.kind, &server.hosted_id) {
        (ServerKind::Hosted, Some(id)) if store::valid_hosted_id(id) => Ok(format!("/v1/hosted/servers/{id}{tail}")),
        _ => Err("That isn't a hosted server.".into()),
    }
}

fn bearer(server: &Server) -> Result<String, String> {
    store::token(&server.id).ok_or_else(|| "This server is no longer linked. Open it again from servers.mxbsecure.com.".to_string())
}

pub async fn get(http: &reqwest::Client, server: &Server) -> Result<Value, String> {
    let path = hosted_path(server, "")?;
    call(http, reqwest::Method::GET, &path, Some(&bearer(server)?), None).await
}

/// Only the fields the hosted settings route takes are passed on.
pub fn settings_body(input: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for key in ["track", "bikeSet", "maxRiders"] {
        if let Some(v) = input.get(key) {
            if !v.is_null() {
                out.insert(key.to_string(), v.clone());
            }
        }
    }
    Value::Object(out)
}

pub async fn settings(http: &reqwest::Client, server: &Server, input: &Value) -> Result<Value, String> {
    let path = hosted_path(server, "/settings")?;
    call(http, reqwest::Method::PUT, &path, Some(&bearer(server)?), Some(settings_body(input))).await
}

pub async fn restart(http: &reqwest::Client, server: &Server) -> Result<Value, String> {
    let path = hosted_path(server, "/restart")?;
    call(http, reqwest::Method::POST, &path, Some(&bearer(server)?), None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "AbCdEfGhIjKlMnOp_-123456";

    #[test]
    fn reads_the_claim_link_and_the_older_connect_link() {
        assert_eq!(parse_link(&format!("mxbservers://hosted/claim?code={CODE}")).as_deref(), Some(CODE));
        assert_eq!(parse_link(&format!("mxbservers://hosted/claim/?code={CODE}")).as_deref(), Some(CODE));
        assert_eq!(parse_link(&format!("mxbservers://connect?claim={CODE}")).as_deref(), Some(CODE));
        assert_eq!(parse_link(&format!("mxbservers://hosted/claim?ref=site&code={CODE}")).as_deref(), Some(CODE));
    }

    #[test]
    fn ignores_anything_else() {
        for bad in [
            format!("mxb://hosted/claim?code={CODE}"),
            format!("mxbservers://hosted/other?code={CODE}"),
            "mxbservers://hosted/claim?code=short".to_string(),
            "mxbservers://hosted/claim?code=".to_string(),
            "mxbservers://hosted/claim".to_string(),
            format!("mxbservers://hosted/claim?code={CODE}%26x"),
            format!("mxbservers://connect?code={CODE}"),
        ] {
            assert!(parse_link(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_claim_answer_is_checked_before_anything_is_saved() {
        let good = json!({ "token": "t0k3n-abcdefghijklmnop", "server": { "id": "0f8fad5b-d9cb-469f-a165-70867728950e", "name": "Friday night", "address": "51.81.10.18:54210" } });
        let claimed = read_claim(&good).unwrap();
        assert_eq!(claimed.name, "Friday night");
        assert_eq!(claimed.address.as_deref(), Some("51.81.10.18:54210"));
        assert!(read_claim(&json!({ "token": "", "server": good["server"] })).is_err());
        assert!(read_claim(&json!({ "token": "x\ny", "server": good["server"] })).is_err());
        assert!(read_claim(&json!({ "token": "abc", "server": { "id": "../admin" } })).is_err());
    }

    #[test]
    fn hosted_routes_are_built_from_the_checked_id_only() {
        let server = Server {
            id: "h".into(),
            name: "x".into(),
            kind: ServerKind::Hosted,
            host: String::new(),
            agent_tls: true,
            ssh_port: 22,
            user: String::new(),
            key_path: None,
            observe_port: 9809,
            admin_port: None,
            log_path: String::new(),
            local: false,
            local_command: None,
            hosted_id: Some("0f8fad5b-d9cb-469f-a165-70867728950e".into()),
        };
        assert_eq!(hosted_path(&server, "/restart").unwrap(), "/v1/hosted/servers/0f8fad5b-d9cb-469f-a165-70867728950e/restart");
        let bad = Server { hosted_id: Some("../../v1/web/admin/hosting".into()), ..server.clone() };
        assert!(hosted_path(&bad, "").is_err());
        let native = Server { kind: ServerKind::Native, ..server };
        assert!(hosted_path(&native, "").is_err());
    }

    #[test]
    fn settings_pass_only_the_known_fields() {
        let body = settings_body(&json!({ "track": "club", "maxRiders": 8, "bikeSet": null, "steamId": "x" }));
        assert_eq!(body, json!({ "track": "club", "maxRiders": 8 }));
    }

    #[test]
    fn the_control_plane_error_is_what_the_user_sees() {
        assert_eq!(failure(404, r#"{"error":"That link has expired."}"#), "That link has expired.");
        assert_eq!(failure(502, "<html>"), "mxbsecure answered HTTP 502.");
    }
}
