//! Where the frontends' log lines land, shared so every app trims and routes them alike.
//!
//! Each app registers its own `log_client` Tauri command (a command has to be per binary), but
//! the body is one thing — cap the size and pick the level — so it lives here rather than in
//! three copies that could drift on the cap or the tag.

/// Record a webview log line at `level` ("error" / "warn" / anything else → info).
///
/// A log line is not a transport for arbitrary payloads: the message is trimmed rather than
/// rejected, because a truncated fact still reads and a dropped one is a support thread that
/// goes nowhere.
pub fn record(level: &str, message: &str) {
    let msg: String = message.chars().take(2000).collect();
    match level {
        "error" => log::error!("[webview] {msg}"),
        "warn" => log::warn!("[webview] {msg}"),
        _ => log::info!("[webview] {msg}"),
    }
}
