//! One-shot capture to arm state-invariant baselines from a real machine.
//!
//! Reading a clean-install baseline off the running game needs two things a normal report does
//! not carry: the build's fingerprint (so a baseline is filed against the right build), and the
//! raw bytes of a candidate region (so the baseline can be taken at all). This logs both, once
//! per game session, to the app log — the one place a tester can copy a line out of.
//!
//! It is deliberately steerable without a rebuild: the fingerprint always logs, and setting
//! `MXB_CAP_RVA` (+ `MXB_CAP_LEN`) makes it also log that region's bytes. So one build can be
//! pointed at any static address by changing an environment variable, rather than shipping a
//! bespoke build per region.
//!
//! The bytes are base64'd, not printed raw: enough to keep a region's contents from being read
//! straight out of a pasted log at a glance, and trivially decoded on our end. It is a capture
//! aid, not a secret — nothing here is a security boundary.

use std::sync::atomic::{AtomicBool, Ordering};

/// Logged once per process. Set only after a fingerprint is actually read, so a tick that runs
/// before the game is up retries on the next one rather than giving up for the session.
static DONE: AtomicBool = AtomicBool::new(false);

/// Read the running game's fingerprint — and, if asked, one region's bytes — and log them once.
///
/// Called from [`crate::procmods::tick`] ahead of everything else, so it runs whether or not
/// the machine is enrolled: a tester capturing a baseline has no reason to have a token.
pub fn dump_once() {
    if DONE.load(Ordering::SeqCst) {
        return;
    }
    let Some(probe) = crate::gameproc::GameProbe::open() else {
        return;
    };
    let Some(base) = probe.module_ranges().first().map(|m| m.base) else {
        return;
    };
    let fp = match crate::peident::identify(&|rva, len| probe.read(base.saturating_add(rva), len)) {
        Some(ident) if ident.is_useful() => ident.fingerprint(),
        // No readable header yet — the game may still be starting. Try again next tick.
        _ => return,
    };
    DONE.store(true, Ordering::SeqCst);
    log::warn!("[statecap] build={fp}");

    // Optional region capture, steered by the environment so one build can read any address.
    let (Some(rva), Some(len)) = (env_num("MXB_CAP_RVA"), env_num("MXB_CAP_LEN")) else {
        return;
    };
    if len == 0 || len > 64 * 1024 {
        log::warn!("[statecap] rva={rva:#x} len={len} rejected (len must be 1..=65536)");
        return;
    }
    match probe.read(base.saturating_add(rva), len as usize) {
        Some(bytes) => {
            use base64::Engine as _;
            use sha2::{Digest as _, Sha256};
            let sha = format!("{:x}", Sha256::new().chain_update(&bytes).finalize());
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            log::warn!(
                "[statecap] rva={rva:#x} len={got} sha={sha} b64={b64}",
                got = bytes.len()
            );
        }
        None => log::warn!("[statecap] rva={rva:#x} len={len} unreadable"),
    }
}

/// Parse an environment number as hex (`0x...`) or decimal, or `None` if unset or unparseable.
fn env_num(key: &str) -> Option<u64> {
    let raw = std::env::var(key).ok()?;
    let s = raw.trim();
    let parsed = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .map(|hex| u64::from_str_radix(hex, 16))
        .unwrap_or_else(|| s.parse::<u64>());
    parsed.ok()
}
