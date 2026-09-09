//! A standing note of how much memory this process is holding.
//!
//! This exists because of an evening the app spent idle at tens of gigabytes. The log covered
//! the whole of it and had nothing to say: startup, four watcher pulses, then four silent
//! hours. Every cache in here has a ceiling and every loop is cheap, so working out what had
//! happened meant reading the code and guessing — and a guess is all it stayed.
//!
//! One number, written down regularly, turns the next one of those into an answer. Left alone
//! this is nearly silent: a line every quarter of an hour, and one the moment the figure
//! climbs. The subsystem totals ride along so a jump can be attributed rather than just
//! noticed.

use std::time::Duration;

/// How often to look. Cheap enough to be unnoticeable, often enough to catch a climb while it
/// is still climbing rather than after it has finished.
const SAMPLE_EVERY: Duration = Duration::from_secs(60);

/// Say something anyway every this many samples, so an ordinary session still leaves a
/// baseline for the next one to be read against.
const REPORT_EVERY: u32 = 15;

/// ...and say it straight away when the footprint gains this much between two samples.
const JUMP: u64 = 128 * 1024 * 1024;

/// Start the watcher. Call once, from `setup`.
pub fn start() {
    tauri::async_runtime::spawn(async move {
        let mut last: u64 = 0;
        let mut quiet: u32 = 0;
        loop {
            tokio::time::sleep(SAMPLE_EVERY).await;

            let Some(stats) = memory_stats::memory_stats() else {
                log::debug!("[mem] this platform won't say what the process is holding");
                return;
            };
            let now = stats.physical_mem as u64;
            let climbed = now.saturating_sub(last);
            quiet += 1;

            if climbed >= JUMP {
                log::warn!("[mem] {} — up {} in a minute. {}", mb(now), mb(climbed), holdings());
            } else if quiet >= REPORT_EVERY {
                log::info!("[mem] {}. {}", mb(now), holdings());
            }
            if climbed >= JUMP || quiet >= REPORT_EVERY {
                quiet = 0;
            }
            last = now;
        }
    });
}

/// What the parts of the app that hold memory on purpose are holding, so a figure that has
/// grown can be put next to the thing that grew it.
fn holdings() -> String {
    let (served, bytes) = crate::imgcache::traffic();
    format!(
        "Textures {}, thumbnails {} served ({})",
        mb(crate::texstore::resident_bytes() as u64),
        served,
        mb(bytes),
    )
}

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_reported_in_megabytes() {
        assert_eq!(mb(0), "0 MB");
        assert_eq!(mb(48 * 1024 * 1024), "48 MB");
        assert_eq!(mb(50 * 1024 * 1024 * 1024), "51200 MB", "the size that started this");
    }
}
