//! The shared server book.
//!
//! ## What this finishes
//!
//! [`crate::serverbook`] already lets the app survive a dead master server, and the mechanism
//! is worth restating because this is only its missing half. The master is the sole source of
//! *discovery* — the only thing that can tell you a server exists — but it is the source of
//! nothing else: a server answers `GETINFO` to whoever asks, with no account, no ticket and no
//! challenge, and that reply carries the name, the riders, the seats and the whole event blob.
//! So the app keeps every address it has been told about and, when the master won't answer,
//! rebuilds the entire list by asking the servers themselves.
//!
//! That works, and it works for the wrong people. The book is per-install and starts empty, so
//! it is worth nothing to a fresh install and nothing to anybody who had not opened the Servers
//! tab before the outage began — which is the whole population that shows up in the Discord
//! asking what happened. Pooling the book is what gets the fallback there before the outage
//! does.
//!
//! ## What is sent, and what isn't
//!
//! Addresses. `host:port` of public game servers, which the app has just read out of the game's
//! own public server browser, and nothing else — no names, no rider, no install id, no account.
//! The control plane keys corroboration on a day-salted digest of the caller's address that it
//! computes for itself (see `control-plane/src/roster.ts`), so this end sends nothing that
//! identifies anybody and nothing that could be traced back between days.
//!
//! That is also why this is not behind the anonymous-stats setting the outage probe rides on.
//! That setting governs telling us about *you* — which features you open, how long the app is
//! up. This tells us about public infrastructure the player just looked at, and gating it would
//! mean the people who most value their privacy are the ones whose Servers tab stays broken.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::paintsync::control_plane;
use crate::serverbook;
use crate::WorldServer;

/// Long enough for a cold Worker, short enough that nothing here delays a tab.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct Report<'a> {
    addresses: Vec<&'a str>,
}

#[derive(Deserialize)]
struct Roster {
    #[serde(default)]
    addresses: Vec<String>,
}

/// Contribute the addresses of a sweep that really came from the master.
///
/// Only ever called with a list the master itself produced, which is the property that makes
/// the shared book safe to serve: the control plane holds an address back until distinct
/// networks have independently seen it there. Feeding it a list rebuilt from our own book would
/// be circular — it would corroborate addresses using the very copies it handed out — so the
/// caller passes the master's answer and nothing else.
///
/// Spawned and forgotten. This hangs off something the player is waiting on, and a contribution
/// that doesn't send is simply one that doesn't send; the next sweep sends another.
pub fn contribute(servers: &[WorldServer]) {
    let addresses: Vec<String> = servers
        .iter()
        .filter(|s| s.joinable && !s.address.trim().is_empty())
        .map(|s| s.address.trim().to_string())
        .collect();
    if addresses.is_empty() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let body = Report { addresses: addresses.iter().map(String::as_str).collect() };
        let sent = match client() {
            Ok(client) => {
                client
                    .post(format!("{}/v1/roster", control_plane()))
                    .json(&body)
                    .send()
                    .await
            }
            Err(e) => {
                log::debug!("[roster] no HTTP client: {e}");
                return;
            }
        };
        match sent {
            Ok(res) if res.status().is_success() => {
                log::debug!("[roster] contributed {} address(es)", body.addresses.len());
            }
            Ok(res) => log::debug!("[roster] contribution refused ({})", res.status()),
            Err(e) => log::debug!("[roster] contribution didn't send: {e}"),
        }
    });
}

/// The rows a snapshot carries. A subset of [`WorldServer`] on purpose.
///
/// No ping: it is a measurement of the round trip from *one* machine, and serving somebody
/// else's would be a number about a network the reader isn't on. No `hidden`: the filtering is
/// the app's own judgement and it makes it again on whatever it draws. Nothing else is left out
/// for any reason but that the tab doesn't draw it.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapshotRow {
    pub address: String,
    pub name: String,
    pub players: u32,
    pub max_players: u32,
    pub track: String,
    pub track_layout: String,
    pub location: String,
    pub session: String,
    pub conditions: String,
    pub categories: Vec<String>,
    pub passworded: bool,
    pub joinable: bool,
}

#[derive(Serialize)]
struct SnapshotReport {
    servers: Vec<SnapshotRow>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Snapshot {
    as_of: u64,
    servers: Vec<SnapshotRow>,
}

impl From<&WorldServer> for SnapshotRow {
    fn from(s: &WorldServer) -> Self {
        SnapshotRow {
            address: s.address.trim().to_string(),
            name: s.name.clone(),
            players: s.players,
            max_players: s.max_players,
            track: s.track.clone(),
            track_layout: s.track_layout.clone(),
            location: s.location.clone(),
            session: s.session.clone(),
            conditions: s.conditions.clone(),
            categories: s.categories.clone(),
            passworded: s.passworded,
            joinable: s.joinable,
        }
    }
}

impl From<SnapshotRow> for WorldServer {
    fn from(r: SnapshotRow) -> Self {
        WorldServer {
            address: r.address,
            name: r.name,
            players: r.players,
            max_players: r.max_players,
            track: r.track,
            track_layout: r.track_layout,
            location: r.location,
            session: r.session,
            conditions: r.conditions,
            categories: r.categories,
            passworded: r.passworded,
            joinable: r.joinable,
            ..Default::default()
        }
    }
}

/// Contribute what the sweep found, so somebody else's tab opens with a list in it.
///
/// Called with the same list [`contribute`] gets and under the same rule — a master sweep and
/// nothing rebuilt from a book — for a reason worth stating again: a snapshot built from the
/// shared one would be this list quoting itself, ageing a little more each round.
///
/// Spawned and forgotten, and the control plane takes at most one of these a minute from
/// everybody, so a contribution that isn't needed costs one request and is answered as fine.
pub fn contribute_snapshot(servers: &[WorldServer]) {
    let rows: Vec<SnapshotRow> = servers
        .iter()
        .filter(|s| !s.address.trim().is_empty() && !s.name.trim().is_empty())
        .map(SnapshotRow::from)
        .collect();
    if rows.is_empty() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let count = rows.len();
        let Ok(client) = client() else { return };
        match client
            .post(format!("{}/v1/roster/snapshot", control_plane()))
            .json(&SnapshotReport { servers: rows })
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                log::debug!("[roster] contributed a snapshot of {count} server(s)");
            }
            Ok(res) => log::debug!("[roster] snapshot refused ({})", res.status()),
            Err(e) => log::debug!("[roster] snapshot didn't send: {e}"),
        }
    });
}

/// The shared snapshot: rows, and the moment they were true. `None` for any failure at all.
///
/// Read only when this install has no last sweep of its own to paint — a fresh install, or one
/// whose book has aged out — so the answer is either a tab that fills at once or the spinner it
/// would have had anyway.
pub async fn snapshot() -> Option<(Vec<WorldServer>, u64)> {
    snapshot_from(&control_plane()).await
}

/// The same, against a given control plane. Split out so a test can point it at a stand-in
/// without touching the process-wide override, which every other caller reads too.
async fn snapshot_from(base: &str) -> Option<(Vec<WorldServer>, u64)> {
    let client = client().ok()?;
    let res = client.get(format!("{base}/v1/roster/snapshot")).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }
    let snapshot = res.json::<Snapshot>().await.ok()?;
    if snapshot.servers.is_empty() || snapshot.as_of == 0 {
        return None;
    }
    let rows: Vec<WorldServer> = snapshot.servers.into_iter().map(WorldServer::from).collect();
    log::info!("[roster] painted {} server(s) from the shared snapshot", rows.len());
    Some((rows, snapshot.as_of))
}

/// Fill the local book from the shared one. Returns how many addresses it added.
///
/// Additive by construction — see [`serverbook::seed`] — so this can run whenever without ever
/// costing the book something it already knew. Every failure is a quiet zero: the caller is
/// either starting the app or already handling a failed list, and neither is improved by a
/// second error about the thing that was meant to help with the first.
pub async fn seed(app: &AppHandle) -> usize {
    let Some(addresses) = fetch().await else {
        return 0;
    };
    if addresses.is_empty() {
        return 0;
    }

    let before = serverbook::load(app);
    let known = before.len();
    let seeded = serverbook::seed(before, &addresses, serverbook::now_millis());
    let added = seeded.len().saturating_sub(known);
    if added == 0 {
        return 0;
    }
    serverbook::write(app, &seeded);
    log::info!("[roster] seeded {added} address(es) into the server book");
    added
}

/// Put a server on the shared book deliberately, as its own operator.
///
/// The one path onto that list that needs no corroborating, because the account behind it is
/// the corroboration — see `claimRoster` in `control-plane/src/roster.ts`. It exists for the
/// server that two strangers will never happen to report: a new one nobody has found yet, or a
/// private one that was never in the game's master list to be seen in.
///
/// The address goes through [`gameproc::parse_server_address`] first, which is the same
/// normalisation the Join flow uses. That matters beyond tidiness: the shared book is keyed by
/// the exact string, so registering `203.0.113.10` and joining `203.0.113.10:54210` must not
/// end up as two different servers. It also fills in the default port, which is what most
/// people will leave off.
pub async fn register_mine(app: &AppHandle, address: &str) -> Result<String, String> {
    let address = crate::gameproc::parse_server_address(address).map_err(|e| format!("{e:#}"))?;
    let cfg = crate::config::load_or_detect(app).unwrap_or_default();
    if cfg.cp_token.trim().is_empty() {
        return Err("Enroll with an invite code first — Settings, then Account.".into());
    }

    let client = client().map_err(|e| format!("Couldn't build an HTTP client: {e}"))?;
    let resp = client
        .post(format!("{}/v1/roster/mine", control_plane()))
        .bearer_auth(cfg.cp_token.trim())
        .json(&serde_json::json!({ "address": address }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the control plane: {e}"))?;

    if resp.status().is_success() {
        log::info!("[roster] registered {address} on the shared book");
        return Ok(address);
    }
    // The one refusal worth rewording. The endpoint is invite-gated on purpose — a self-serve
    // account anyone can mint with one request would walk straight past the two-network bar
    // that makes the anonymous path safe — but most people running the app have never claimed
    // an invite, so "that needs an invite" is what the majority of clicks would get, and it
    // tells them nothing they can act on. The useful half is the second sentence: almost
    // everyone who lands here did not need this in the first place.
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err("Adding a server by hand needs an invited account. You probably don't need \
                    it: a server the game lists is remembered automatically, and this is only \
                    for one that never appears there."
            .into());
    }
    // Otherwise the control plane's own wording: it knows why it refused and this side would
    // only be guessing.
    let detail = resp.text().await.unwrap_or_default();
    Err(serde_json::from_str::<serde_json::Value>(&detail)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or(detail))
}

/// Ask the control plane for the shared book. `None` for any failure at all.
async fn fetch() -> Option<Vec<String>> {
    let client = client().ok()?;
    let res = client
        .get(format!("{}/v1/roster", control_plane()))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    Some(res.json::<Roster>().await.ok()?.addresses)
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(TIMEOUT).build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A stand-in control plane that answers one request with `body` and hangs up.
    fn serve(body: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else { return };
            // Read the request first. Replying to a peer that is still sending — and then
            // closing on unread bytes — resets the connection, and the client sees a send
            // error rather than the answer.
            read_request(&mut sock);
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(reply.as_bytes());
            let _ = sock.flush();
        });
        format!("http://127.0.0.1:{port}")
    }

    /// Read up to the end of the request headers. A GET carries no body, so that is all of it.
    fn read_request(sock: &mut std::net::TcpStream) {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            match sock.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
    }

    /// The pooled list is the whole of what a machine with no MX Bikes on it can show, so the
    /// shape the Worker sends has to arrive as rows the Servers tab can draw — name, address,
    /// riders and what's running, not an empty `WorldServer`.
    ///
    #[tokio::test]
    async fn the_pooled_snapshot_arrives_as_rows_the_tab_can_draw() {
        let body = r#"{"asOf":1758000000000,"count":2,"servers":[
            {"address":"45.129.56.133:5341","name":"Frost EU #1","players":7,"maxPlayers":20,
             "track":"Indiana","trackLayout":"","location":"EU West","session":"Practice",
             "conditions":"Sunny","categories":["MX2"],"passworded":false,"joinable":true},
            {"address":"18.185.94.143:54210","name":"Frost US","players":0,"maxPlayers":16,
             "track":"Duna","trackLayout":"Short","location":"USA","session":"Race 1",
             "conditions":"Cloudy","categories":[],"passworded":true,"joinable":true}]}"#;
        let base = serve(body);
        let (rows, as_of) =
            snapshot_from(&base).await.expect("the pooled list should have come back");

        assert_eq!(as_of, 1_758_000_000_000);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Frost EU #1");
        assert_eq!(rows[0].address, "45.129.56.133:5341");
        assert_eq!((rows[0].players, rows[0].max_players), (7, 20));
        assert_eq!(rows[0].track, "Indiana");
        assert_eq!(rows[0].location, "EU West");
        assert!(rows[0].joinable && !rows[0].passworded);
        assert!(rows[1].passworded, "a locked server has to still read as locked");
        // Never measured for somebody else's sweep: the tab sorts unmeasured rows last rather
        // than showing a ping this machine did not take.
        assert!(rows[0].ping_ms.is_none());
    }

    /// An empty pool is not a list. The caller has to see `None` so the tab shows the failure
    /// and its connection check rather than an empty browser that looks like nobody is online.
    #[tokio::test]
    async fn an_empty_pool_is_not_a_list() {
        let base = serve(r#"{"asOf":0,"count":0,"servers":[]}"#);
        assert!(snapshot_from(&base).await.is_none());
    }
}
