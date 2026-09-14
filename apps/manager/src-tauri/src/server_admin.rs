//! Dedicated-server operator console — **parked, not registered**.
//!
//! These commands let a player create, publish, drive and destroy dedicated servers the control
//! plane runs for them (each authenticated with the account's `cp_token`). They are kept here,
//! out of the app's IPC surface, until there is a UI that uses them: none is wired into
//! `generate_handler!`, so nothing in a shipped build can invoke them, and `provision_server`
//! (which spins up billable infrastructure) and `cloud_servers` (which hands an agent token to
//! the webview) are not reachable from a compromised page.
//!
//! To bring the feature back, move these `#[tauri::command]` functions into `main.rs` (or
//! re-export them), add each to the `generate_handler!` list, and `.manage(CloudServers::default())`.
#![allow(dead_code)]

use tauri::Manager;

use crate::{claim_guid, config, paintsync, servers};

/// The dedicated servers this player administers.
#[tauri::command]
pub fn list_servers(app: tauri::AppHandle) -> Vec<servers::ServerRef> {
    config::load_or_detect(&app).unwrap_or_default().servers
}

/// Replace the saved server list. The UI owns add/edit/remove and sends the whole list,
/// which keeps ordering and identity in one place rather than split across three commands.
#[tauri::command]
pub fn save_servers(app: tauri::AppHandle, servers: Vec<servers::ServerRef>) -> Result<(), String> {
    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    cfg.servers = servers;
    config::save(&app, &cfg).map_err(|e| format!("{e:#}"))
}

/// Servers the control plane runs for this account, as last fetched.
///
/// Held in memory rather than saved to `config.json`, because the token in each one belongs
/// to a machine the control plane can re-issue at any time — and a credential this app never
/// writes to disk is one that cannot go stale there or be read out of it. Refreshed whenever
/// the Servers page asks, which is also whenever anything is about to act on one.
#[derive(Default)]
pub struct CloudServers(std::sync::Mutex<Vec<servers::ServerRef>>);

/// Look up one saved server by id, so the commands below take an id rather than having the
/// frontend hand back a token it was given.
fn server_by_id(app: &tauri::AppHandle, id: &str) -> Result<servers::ServerRef, String> {
    if let Some(saved) =
        config::load_or_detect(app).unwrap_or_default().servers.into_iter().find(|s| s.id == id)
    {
        return Ok(saved);
    }
    // Not one they paired by hand, so it's one the control plane launched for them. These
    // never reach the saved list — nothing on that box prints a pairing code anyone can read.
    app.state::<CloudServers>()
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| "That server isn't in your list any more.".to_string())
}

/// A server run for this account by the control plane.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudServer {
    id: String,
    name: String,
    region: String,
    /// `host:port` players connect to. Empty until the box has announced itself.
    address: String,
    #[serde(default)]
    agent_url: Option<String>,
    #[serde(default)]
    agent_token: Option<String>,
    #[serde(default)]
    instance_id: Option<String>,
    published: bool,
    created_at: u64,
    /// When the server was last seen with nobody on it, or `null` while someone is riding.
    #[serde(default)]
    idle_since: Option<u64>,
    /// Minutes of emptiness before it destroys itself.
    idle_minutes: u64,
    /// `pending` | `running` | `stopping` | `stopped` | `gone` | `self-hosted`.
    state: String,
    #[serde(default)]
    public_ip: Option<String>,
}

/// The servers the control plane is running for this account.
///
/// A provisioned box has no console and prints its pairing code to nobody, so this is the
/// only way its owner can ever obtain the token that drives it. Fetching it also refreshes
/// the in-memory list `server_by_id` falls back to, which is what makes Start, Stop and Set
/// track work on a machine the player has never touched.
#[tauri::command]
async fn cloud_servers(app: tauri::AppHandle) -> Result<Vec<CloudServer>, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        servers: Vec<CloudServer>,
    }
    let resp = reqwest::Client::new()
        .get(format!("{}/v1/servers/mine", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text));
    }
    let body: Resp = resp.json().await.map_err(|e| format!("{e}"))?;

    // Only the ones we can actually talk to become drivable; a box still booting has no
    // agent yet, and an entry with no token is one the control plane declined to hand over.
    *app.state::<CloudServers>().0.lock().unwrap() = body
        .servers
        .iter()
        .filter_map(|s| {
            Some(servers::ServerRef {
                id: s.id.clone(),
                name: s.name.clone(),
                url: s.agent_url.clone()?,
                token: s.agent_token.clone()?,
                registry_id: s.published.then(|| s.id.clone()).unwrap_or_default(),
            })
        })
        .collect();
    Ok(body.servers)
}

/// Destroy a server the control plane runs, and stop paying for it.
#[tauri::command]
async fn destroy_cloud_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .delete(format!("{}/v1/servers/{id}", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    // Already gone is the state we wanted.
    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NOT_FOUND {
        let text = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text));
    }
    app.state::<CloudServers>().0.lock().unwrap().retain(|s| s.id != id);
    Ok(())
}

#[tauri::command]
async fn server_status(app: tauri::AppHandle, id: String) -> Result<serde_json::Value, String> {
    let server = server_by_id(&app, &id)?;
    let status = servers::status(&server).await?;
    claim_guid_from_roster(&app, &server).await;
    Ok(status)
}

/// Claim this player's own GUID the first time one of their servers sees them connect.
///
/// The GUID is the identity the roster keys on, and it used to be a 32-character field the
/// player had to find and type. They can't read it off their own machine — the game's
/// plugin API exposes it only for the local player, to a plugin, in-process — but the
/// dedicated server writes it next to their name on every connection, and the agent already
/// parses exactly that. So the app waits until it sees the name it enrolled under connected
/// to a server this player administers, and takes the GUID from there.
///
/// Runs off the status poll the Servers page already makes, and short-circuits the moment a
/// GUID is held, so it costs one extra request per poll only while still unclaimed.
async fn claim_guid_from_roster(app: &tauri::AppHandle, server: &servers::ServerRef) {
    let cfg = config::load_or_detect(app).unwrap_or_default();
    if !cfg.cp_guid.trim().is_empty() || cfg.cp_token.trim().is_empty() {
        return;
    }
    let rider = cfg.cp_rider_name.trim();
    if rider.is_empty() {
        return;
    }
    let Ok(players) = servers::players(server).await else { return };
    // Matched case-insensitively for the same reason the control plane's unique index is:
    // the player typed this name into the game and into the app on two separate occasions.
    let Some(me) = players
        .iter()
        .find(|p| p.name.trim().eq_ignore_ascii_case(rider) && !p.guid.trim().is_empty())
    else {
        return;
    };

    match claim_guid(app, &me.guid).await {
        Ok(()) => log::info!("[sync] claimed GUID {} for {rider}", me.guid),
        // First-come on the server side, so a rejection here is a real answer — someone
        // else holds it — not a transient failure worth retrying into a loop.
        Err(e) => log::warn!("[sync] couldn't claim GUID {} for {rider}: {e}", me.guid),
    }
}

#[tauri::command]
async fn server_tracks(app: tauri::AppHandle, id: String) -> Result<Vec<String>, String> {
    servers::tracks(&server_by_id(&app, &id)?).await
}

/// Create a server: the control plane launches a machine for it.
///
/// The app never talks to AWS. A desktop binary can be unpacked, so a cloud credential
/// inside one would let anyone create infrastructure in our account — the control plane
/// holds the key and this asks it nicely, authenticated as this player.
#[tauri::command]
async fn provision_server(app: tauri::AppHandle, name: String) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .post(format!("{}/v1/provision", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .json(&serde_json::json!({ "name": name.trim() }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;

    let ok = resp.status().is_success();
    let text = resp.text().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !ok {
        return Err(body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(text));
    }
    Ok(body)
}

/// What's running, and therefore what's being paid for.
///
/// Read from EC2 rather than from anyone's records, because that is the number that turns
/// into a bill.
#[tauri::command]
async fn fleet_state(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .get(format!("{}/v1/fleet", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    let ok = resp.status().is_success();
    let text = resp.text().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !ok {
        return Err(body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(text));
    }
    Ok(body)
}

/// Put a server the player runs into the public list, so other people can find it.
///
/// Everything the control plane needs is already known to the agent, so nothing here is
/// asked of the operator: the game address is the agent's own host joined to the port it
/// reports, and the name comes from the server's `.ini`. What the player supplies is the
/// decision to publish, and a region — which is the one fact no machine can infer.
///
/// The agent URL is sent so the control plane can check the box actually answers before
/// advertising it. That check is why an unreachable home server doesn't end up as a row in
/// everyone's join picker that nobody can connect to.
#[tauri::command]
async fn publish_server(
    app: tauri::AppHandle,
    id: String,
    region: String,
) -> Result<serde_json::Value, String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let server = server_by_id(&app, &id)?;
    let status = servers::status(&server).await?;

    let port = status
        .get("port")
        .and_then(|p| p.as_u64())
        .ok_or("The agent didn't say which port the server runs on.")?;
    let host = servers::host_of(&server.url)?;
    let name = status
        .get("server")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(&server.name)
        .to_string();

    let resp = reqwest::Client::new()
        .post(format!("{}/v1/servers", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .json(&serde_json::json!({
            "name": name,
            "region": region,
            "address": format!("{host}:{port}"),
            "agentUrl": server.url,
        }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;

    let status_code = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !status_code.is_success() {
        return Err(body
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
            .unwrap_or(text));
    }

    // Remember the registry id: it is the only handle that can withdraw this row later, and
    // the control plane will never hand it out a second time.
    if let Some(registry_id) = body.get("id").and_then(|v| v.as_str()) {
        let mut cfg = config::load_or_detect(&app).unwrap_or_default();
        if let Some(saved) = cfg.servers.iter_mut().find(|s| s.id == id) {
            saved.registry_id = registry_id.to_string();
            config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
        }
    }
    Ok(body)
}

/// Take a server back out of the public list.
#[tauri::command]
async fn unpublish_server(app: tauri::AppHandle, registry_id: String) -> Result<(), String> {
    let cfg = config::load_or_detect(&app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first.".into());
    }
    let resp = reqwest::Client::new()
        .delete(format!("{}/v1/servers/{registry_id}", paintsync::control_plane()))
        .bearer_auth(&cfg.cp_token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;
    // A row already gone from the control plane is the state we wanted; clearing our end
    // regardless keeps a 404 from stranding the local entry as permanently "published".
    let gone = resp.status() == reqwest::StatusCode::NOT_FOUND;
    if !resp.status().is_success() && !gone {
        let text = resp.text().await.unwrap_or_default();
        return Err(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or(text));
    }

    let mut cfg = config::load_or_detect(&app).unwrap_or_default();
    if let Some(saved) = cfg.servers.iter_mut().find(|s| s.registry_id == registry_id) {
        saved.registry_id.clear();
        config::save(&app, &cfg).map_err(|e| format!("{e:#}"))?;
    }
    Ok(())
}

/// Unpack the one-line code `mxb-agent` prints, so adding a server is a paste rather than
/// an address, a token and a name typed in by hand.
#[tauri::command]
fn parse_pairing(blob: String) -> Result<servers::Pairing, String> {
    servers::parse_pairing(&blob)
}

/// Ask an agent to name itself, before it's saved to the list.
///
/// Lets the add form fill the server's name in from its `.ini` rather than having the
/// operator retype something the host already knows, and doubles as the check that the
/// address and token are right — a typo shows up here instead of as a dead row.
#[tauri::command]
async fn server_probe(url: String, token: String) -> Result<serde_json::Value, String> {
    let probe = servers::ServerRef {
        url: url.trim().to_string(),
        token: token.trim().to_string(),
        ..Default::default()
    };
    servers::status(&probe).await
}

#[tauri::command]
async fn server_action(
    app: tauri::AppHandle,
    id: String,
    action: servers::Action,
) -> Result<serde_json::Value, String> {
    servers::act(&server_by_id(&app, &id)?, action).await
}

#[tauri::command]
async fn server_set_config(
    app: tauri::AppHandle,
    id: String,
    patch: serde_json::Value,
) -> Result<serde_json::Value, String> {
    servers::set_config(&server_by_id(&app, &id)?, patch).await
}
