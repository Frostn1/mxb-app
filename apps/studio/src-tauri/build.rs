fn main() {
    // No sidecar detection here: `sidecar.rs` lives in mxb-core, which owns that decision
    // and publishes it. Nothing in this app is gated on it yet — `sidecar_lock`, which is,
    // arrives with the Protect tab.
    tauri_build::build()
}
