//! Crash reports the game left behind, on their way to the control plane.
//!
//! MX Bikes closes to desktop on its own: landing an overjump, hitting an object, clicking go
//! to track, and more often on a busy server. None of that is the app's doing, and all of it
//! lands on players who are running the app when it happens.
//!
//! FrostMod is inside the game process, so it is the one thing that can say where the game
//! died. When it does, FrostMod writes `frostmod-crash-<stamp>.json` beside its log: the
//! faulting module and offset, the call stack, which track and server, how many riders were in
//! the session, and what happened just before. This module finds those files and posts them.
//!
//! **Why the game writes a file instead of the app sending it live.** At the moment of the
//! crash the game has seconds at best, the network stack is the last thing to trust, and the
//! app may not even be running. A file survives all three. The app picks it up whenever it
//! next looks, which is at startup and when a session ends.
//!
//! **What is not sent.** The minidump beside it. That is megabytes and a copy of process
//! memory, and it stays on the machine unless someone asks for it by name through Send logs.
//! This sends the small JSON and nothing else.
//!
//! A sent report is renamed to `.sent` rather than deleted: the player keeps their own copy,
//! a bug in this module cannot destroy evidence, and the rename is what stops a second send.
//! If the rename fails the control plane deduplicates anyway — the crash's own timestamp and
//! site are a unique key over there.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::AppConfig;

/// The most reports one pass sends. A machine that crashed thirty times while the app was
/// closed has thirty files and one story; the newest handful tell it.
const MAX_PER_PASS: usize = 10;

/// Refuse anything absurd. A report is a couple of kilobytes; one this big is not one of ours.
const MAX_REPORT_BYTES: u64 = 512 * 1024;

fn is_report(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("frostmod-crash-") && lower.ends_with(".json")
}

/// Every unsent report in the FrostMod folder, newest first.
///
/// Newest first because the send is capped: if a player has been crashing all week, the
/// reports worth having are the ones from the build they are running now.
pub fn pending(frostmod_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(frostmod_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_report(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() > MAX_REPORT_BYTES {
            continue;
        }
        found.push((meta.modified().unwrap_or(std::time::UNIX_EPOCH), entry.path()));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// Whether anything is waiting to be sent, without reading any of it. What the UI asks.
pub fn any_pending(frostmod_dir: &Path) -> bool {
    !pending(frostmod_dir).is_empty()
}

/// Mark one as sent. The file stays; only its name changes.
fn mark_sent(path: &Path) {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".sent");
    let _ = std::fs::rename(path, path.with_file_name(name));
}

/// Post one report, with the few things only the app knows added to it.
///
/// FrostMod knows what faulted; it does not know which app version is installed, which
/// account this is, or which game build the exe on disk is. Those are merged in here rather
/// than plumbed into the DLL, where they would be three more things to keep in step.
async fn send(token: &str, app_version: &str, guid: &str, build: &str, body: &str) -> anyhow::Result<()> {
    // The file is the client's own JSON. It is re-parsed rather than concatenated so a
    // truncated report — the game died mid-write, which is exactly when this file is written —
    // is caught here instead of becoming a 400 the player never sees.
    let mut report: serde_json::Value = serde_json::from_str(body)?;
    let Some(object) = report.as_object_mut() else {
        anyhow::bail!("a crash report has to be an object");
    };
    object.insert("appVersion".into(), serde_json::Value::String(app_version.to_string()));
    if !guid.is_empty() {
        object.insert("guid".into(), serde_json::Value::String(guid.to_string()));
    }
    if !build.is_empty() {
        object.insert("build".into(), serde_json::Value::String(build.to_string()));
    }

    let res = reqwest::Client::new()
        .put(format!("{}/v1/diagnostics/crash", crate::paintsync::control_plane()))
        .bearer_auth(token)
        .json(&report)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    // Which refusals are final, and which are worth keeping the file for.
    //
    // This is the whole difference between collecting crashes and quietly binning them. A
    // 400 means the report is malformed and sending it again next week changes nothing, so
    // it is done with. Everything else is about the moment, not the report: a 404 is an
    // endpoint that has not been deployed yet, a 401 is a token that will be refreshed, a
    // 429 is a retry by definition, and a 5xx is somebody else's bad day. Treating any of
    // those as final would throw away exactly the reports that arrive around a release,
    // which are the ones worth having.
    let status = res.status();
    if status.is_success() || status == reqwest::StatusCode::BAD_REQUEST {
        return Ok(());
    }
    anyhow::bail!("control plane said {status}")
}

/// Send whatever is waiting. Safe to call on every session end and at startup.
///
/// Silent by design, apart from the log: a player whose game just crashed does not need the
/// app to have an opinion about it as well. The one thing they are asked is whether to send
/// the dump, and that is a separate question the UI puts to them once.
pub async fn flush(app_version: &str, cfg: &AppConfig, frostmod_dir: &Path, build: &str) {
    let token = cfg.cp_token.trim();
    if token.is_empty() {
        return; // not enrolled: there is nowhere for a report to go
    }
    let waiting = pending(frostmod_dir);
    if waiting.is_empty() {
        return;
    }
    log::info!("crash reports: {} waiting to send", waiting.len());

    for path in waiting.into_iter().take(MAX_PER_PASS) {
        let Ok(body) = std::fs::read_to_string(&path) else { continue };
        match send(token, app_version, cfg.cp_guid.trim(), build, &body).await {
            Ok(()) => {
                mark_sent(&path);
                log::info!("crash reports: sent {}", path.display());
            }
            Err(e) => {
                // Left on disk on purpose. The next pass tries again, and the control plane
                // will not count it twice if this one actually landed.
                log::warn!("crash reports: {} not sent ({e})", path.display());
                break; // one failure means the network, not the file; stop for now
            }
        }
    }
}


/// A minidump on this machine that nobody has been asked about yet.
///
/// The JSON report goes on its own, because it is small and carries nothing but addresses.
/// The dump is the other half: megabytes of process memory, which is the difference between
/// knowing where the game died and being able to take it apart. That one is a question, and
/// it is asked once. A `.offered` marker beside it is the record of having asked, so a
/// player who said no is not asked again about the same crash.
pub fn dumps_waiting(frostmod_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(frostmod_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with("frostmod-crash-") || !name.ends_with(".dmp") {
            continue;
        }
        let path = entry.path();
        if offered_marker(&path).exists() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        found.push((meta.modified().unwrap_or(std::time::UNIX_EPOCH), path));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, p)| p).collect()
}

fn offered_marker(dump: &Path) -> PathBuf {
    let mut name = dump.file_name().unwrap_or_default().to_os_string();
    name.push(".offered");
    dump.with_file_name(name)
}

/// Record that the player has been asked about every dump currently waiting.
///
/// Called whether they said yes or no: the question was put, and asking again about the
/// same crash is nagging. The dump itself is left alone either way — it is the player's
/// file, and FrostMod prunes to the newest three on its own.
pub fn mark_dumps_offered(frostmod_dir: &Path) {
    for dump in dumps_waiting(frostmod_dir) {
        let _ = std::fs::write(offered_marker(&dump), b"asked\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Same shape as the other module tests here: a named folder under the OS temp dir,
    /// cleared first, so a run that died halfway does not poison the next one.
    fn dir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("frost-crash-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn finds_reports_and_ignores_everything_else() {
        let d = dir("finds");
        fs::write(d.join("frostmod-crash-20260916-145929.json"), "{}").unwrap();
        fs::write(d.join("frostmod.log"), "not a report").unwrap();
        fs::write(d.join("frostmod.exe"), "not a report").unwrap();
        // Already sent: the rename is what stops a second send.
        fs::write(d.join("frostmod-crash-20260915-101010.json.sent"), "{}").unwrap();

        let found = pending(&d);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("frostmod-crash-20260916-145929.json"));
    }

    #[test]
    fn sends_the_newest_first() {
        let d = dir("order");
        // A player who crashed all week has a folder of these, and the send is capped. The
        // ones worth having are from the build they are running now.
        for name in ["frostmod-crash-20260910-090000.json", "frostmod-crash-20260916-145929.json"] {
            fs::write(d.join(name), "{}").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let found = pending(&d);
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("frostmod-crash-20260916-145929.json"));
    }

    #[test]
    fn skips_anything_too_big_to_be_one_of_ours() {
        let d = dir("toobig");
        let big = vec![b'x'; (MAX_REPORT_BYTES + 1) as usize];
        fs::write(d.join("frostmod-crash-20260916-145929.json"), big).unwrap();
        assert!(pending(&d).is_empty());
    }

    #[test]
    fn a_sent_report_is_renamed_not_deleted() {
        let d = dir("renamed");
        let path = d.join("frostmod-crash-20260916-145929.json");
        fs::write(&path, "{}").unwrap();
        mark_sent(&path);
        // The player keeps their copy, and a bug here cannot destroy evidence.
        assert!(!path.exists());
        assert!(d.join("frostmod-crash-20260916-145929.json.sent").exists());
        assert!(pending(&d).is_empty());
    }

    #[test]
    fn asks_about_a_dump_once() {
        let d = dir("offered");
        fs::write(d.join("frostmod-crash-20260916-145929.dmp"), "MDMP").unwrap();
        assert_eq!(dumps_waiting(&d).len(), 1);

        // Asked. Whether they said yes or no, asking again about the same crash is nagging.
        mark_dumps_offered(&d);
        assert!(dumps_waiting(&d).is_empty());
        // And the dump is still theirs.
        assert!(d.join("frostmod-crash-20260916-145929.dmp").exists());
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        assert!(pending(Path::new("/nowhere/at/all")).is_empty());
        assert!(!any_pending(Path::new("/nowhere/at/all")));
    }
}
