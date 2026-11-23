//! Track uploads that outlive the screen that started them. The transfer runs in a background
//! task owned by the backend, not by any React component: switching tabs, servers or pages
//! changes nothing. Progress goes out as `upload-update` events, and the frontend can always
//! ask for the current list (`upload_list`) when a view mounts. Only an explicit cancel, or the
//! app exiting, stops one.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UploadStatus {
    Checking,
    Uploading,
    Retrying,
    Installing,
    Done,
    Error,
    Cancelled,
}

impl UploadStatus {
    pub fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Error | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadInfo {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub path: String,
    pub file_name: String,
    pub bytes: u64,
    pub sent: u64,
    /// Bytes per second over the last moments of the transfer.
    pub speed: f64,
    pub attempt: u32,
    pub status: UploadStatus,
    pub error: Option<String>,
}

struct Entry {
    info: UploadInfo,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Uploads {
    items: Mutex<HashMap<String, Entry>>,
}

impl Uploads {
    pub fn add(&self, info: UploadInfo) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut items) = self.items.lock() {
            items.insert(
                info.id.clone(),
                Entry {
                    info,
                    cancel: Arc::clone(&cancel),
                },
            );
        }
        cancel
    }

    /// Change one upload and return its new state. A finished upload stays finished.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut UploadInfo)) -> Option<UploadInfo> {
        let mut items = self.items.lock().ok()?;
        let entry = items.get_mut(id)?;
        if entry.info.status.finished() {
            return None;
        }
        change(&mut entry.info);
        Some(entry.info.clone())
    }

    pub fn list(&self) -> Vec<UploadInfo> {
        let mut all: Vec<UploadInfo> = self
            .items
            .lock()
            .map(|items| items.values().map(|e| e.info.clone()).collect())
            .unwrap_or_default();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }

    /// Ask a running upload to stop. The task notices, kills ssh and reports `Cancelled`.
    pub fn cancel(&self, id: &str) -> bool {
        match self.items.lock() {
            Ok(items) => match items.get(id) {
                Some(entry) if !entry.info.status.finished() => {
                    entry.cancel.store(true, Ordering::SeqCst);
                    true
                }
                _ => false,
            },
            Err(_) => false,
        }
    }

    /// Forget a finished upload (the user dismissed it).
    pub fn dismiss(&self, id: &str) {
        if let Ok(mut items) = self.items.lock() {
            if items.get(id).is_some_and(|e| e.info.status.finished()) {
                items.remove(id);
            }
        }
    }

    pub fn active(&self) -> usize {
        self.items
            .lock()
            .map(|items| items.values().filter(|e| !e.info.status.finished()).count())
            .unwrap_or(0)
    }

    /// Is an upload of this file to this server already running?
    pub fn running(&self, server_id: &str, path: &str) -> Option<UploadInfo> {
        self.items.lock().ok()?.values().find_map(|e| {
            (e.info.server_id == server_id && e.info.path == path && !e.info.status.finished())
                .then(|| e.info.clone())
        })
    }
}

/// Bytes per second, smoothed over roughly the last second so the number is readable.
pub struct SpeedMeter {
    window_start: Instant,
    window_bytes: u64,
    speed: f64,
}

impl SpeedMeter {
    pub fn new() -> Self {
        Self {
            window_start: Instant::now(),
            window_bytes: 0,
            speed: 0.0,
        }
    }

    /// Record the total bytes sent so far; returns the current speed.
    pub fn record(&mut self, total: u64, now: Instant) -> f64 {
        let elapsed = now.duration_since(self.window_start).as_secs_f64();
        if elapsed >= 0.5 {
            let rate = total.saturating_sub(self.window_bytes) as f64 / elapsed;
            self.speed = if self.speed == 0.0 {
                rate
            } else {
                self.speed * 0.5 + rate * 0.5
            };
            self.window_start = now;
            self.window_bytes = total;
        }
        self.speed
    }
}

/// Is this ssh failure worth another attempt (the network blinked) rather than a permanent
/// one (bad key, full disk)?
pub fn transient(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    !m.contains("permission denied")
        && [
            "connection reset",
            "connection closed",
            "connection timed out",
            "broken pipe",
            "connection refused",
            "timed out",
            "network is unreachable",
            "could not resolve",
            "did not finish",
            "kex_exchange_identification",
        ]
        .iter()
        .any(|needle| m.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn info(id: &str) -> UploadInfo {
        UploadInfo {
            id: id.into(),
            server_id: "s1".into(),
            server_name: "Prod".into(),
            path: "C:/t.pkz".into(),
            file_name: "t.pkz".into(),
            bytes: 100,
            sent: 0,
            speed: 0.0,
            attempt: 1,
            status: UploadStatus::Uploading,
            error: None,
        }
    }

    #[test]
    fn cancel_flags_a_running_upload_and_finished_ones_stay_finished() {
        let uploads = Uploads::default();
        let flag = uploads.add(info("a"));
        assert_eq!(uploads.active(), 1);
        assert!(uploads.cancel("a"));
        assert!(flag.load(Ordering::SeqCst));
        uploads.update("a", |i| i.status = UploadStatus::Cancelled);
        assert_eq!(uploads.active(), 0);
        assert!(!uploads.cancel("a"));
        // A late progress update cannot revive it.
        assert!(uploads.update("a", |i| i.sent = 50).is_none());
        assert_eq!(uploads.list()[0].sent, 0);
        uploads.dismiss("a");
        assert!(uploads.list().is_empty());
    }

    #[test]
    fn running_uploads_are_not_dismissed_and_are_found_by_server_and_file() {
        let uploads = Uploads::default();
        uploads.add(info("a"));
        uploads.dismiss("a");
        assert_eq!(uploads.list().len(), 1);
        assert!(uploads.running("s1", "C:/t.pkz").is_some());
        assert!(uploads.running("s2", "C:/t.pkz").is_none());
    }

    #[test]
    fn speed_is_bytes_over_the_window() {
        let mut meter = SpeedMeter::new();
        let start = meter.window_start;
        assert_eq!(meter.record(1000, start + Duration::from_millis(100)), 0.0);
        let speed = meter.record(1_000_000, start + Duration::from_secs(1));
        assert!((speed - 1_000_000.0).abs() < 1.0);
    }

    #[test]
    fn network_blips_are_retried_and_auth_failures_are_not() {
        assert!(transient("client_loop: send disconnect: Broken pipe"));
        assert!(transient("ssh: connect to host x port 22: Connection timed out"));
        assert!(!transient("user@host: Permission denied (publickey)."));
        assert!(!transient("cannot write /tmp/x: No space left on device"));
    }
}
