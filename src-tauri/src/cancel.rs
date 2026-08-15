//! Abort flags for in-flight installs, keyed by mod slug.
//!
//! Keyed by slug because that is already how an install identifies itself on the wire — the
//! `install-progress` event carries nothing else ([`crate::install`]) — and because the frontend
//! drains its queue strictly one at a time, so a slug is never ambiguous.
//!
//! The flag is looked up by slug at each check point rather than threaded through as a parameter.
//! That keeps [`crate::install::download`] signature-compatible with [`crate::shop_fetch::download`],
//! which is load-bearing: `shop_stage` swaps one for the other.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

fn flags() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static FLAGS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    FLAGS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A poisoned lock here would disable cancelling for the rest of the session, which is worse
/// than reading through the poison — nothing in the map is invariant-bearing.
fn map() -> std::sync::MutexGuard<'static, HashMap<String, Arc<AtomicBool>>> {
    flags().lock().unwrap_or_else(|e| e.into_inner())
}

/// The cancel flag for one install. `None` when nothing registered the slug — an install
/// started outside a [`Guard`] simply can't be cancelled, and says so by never firing.
#[derive(Clone, Default)]
pub(crate) struct Token(Option<Arc<AtomicBool>>);

impl Token {
    pub(crate) fn cancelled(&self) -> bool {
        self.0.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
    }

    /// `Err(`[`cancelled`]`)` once the user has asked to stop.
    pub(crate) fn check(&self) -> anyhow::Result<()> {
        if self.cancelled() {
            return Err(cancelled());
        }
        Ok(())
    }
}

/// Registers `slug` as cancellable and unregisters it on drop.
pub(crate) struct Guard(String);

impl Drop for Guard {
    fn drop(&mut self) {
        map().remove(&self.0);
    }
}

/// Make `slug` cancellable for as long as the returned guard lives.
pub(crate) fn begin(slug: &str) -> Guard {
    map().insert(slug.to_string(), Arc::new(AtomicBool::new(false)));
    Guard(slug.to_string())
}

/// The flag for `slug`, grabbed once so a hot loop can poll an atomic instead of a mutex.
pub(crate) fn token(slug: &str) -> Token {
    Token(map().get(slug).cloned())
}

/// Ask the install for `slug` to stop. `false` when there is nothing running under that slug.
pub(crate) fn request(slug: &str) -> bool {
    match map().get(slug) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// The error a cancelled transfer fails with. The frontend suppresses the failure toast for a
/// job it cancelled itself, so this text is a fallback rather than the usual path.
pub(crate) fn cancelled() -> anyhow::Error {
    anyhow::anyhow!("Download cancelled.")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The registry is process-global, so every test uses a slug of its own.

    #[test]
    fn unregistered_slug_never_fires() {
        assert!(!request("nobody-is-installing-this"));
        assert!(!token("nobody-is-installing-this").cancelled());
    }

    #[test]
    fn request_reaches_a_token_taken_before_it() {
        let _guard = begin("cancel-mid-transfer");
        // Taken up front, the way a download loop grabs it once and then polls.
        let held = token("cancel-mid-transfer");
        assert!(!held.cancelled());
        assert!(request("cancel-mid-transfer"));
        assert!(held.cancelled());
        assert!(held.check().is_err());
    }

    #[test]
    fn dropping_the_guard_unregisters() {
        {
            let _guard = begin("finished-install");
            assert!(request("finished-install"));
        }
        // The install is over; a late cancel must not linger and stop the next one.
        assert!(!request("finished-install"));
        assert!(!token("finished-install").cancelled());
    }

    #[test]
    fn a_fresh_run_starts_uncancelled() {
        let first = begin("retried-install");
        assert!(request("retried-install"));
        drop(first);

        let _second = begin("retried-install");
        assert!(!token("retried-install").cancelled());
    }
}
