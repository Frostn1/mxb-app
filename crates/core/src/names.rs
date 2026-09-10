//! Small shared facts that have no better home.
//!
//! Each of these was a private helper inside a module that serves one app, and each is
//! needed by code that serves both. They are here rather than duplicated so the two apps
//! cannot end up disagreeing about where the control plane is, or about which characters
//! are safe in a filename.

/// Where the control plane lives. A constant rather than a setting: pointing the app at
/// another host would let anything served there write files into the mods folder.
pub const CONTROL_PLANE: &str = "https://mxb-control-plane.aui-svi.workers.dev";

/// Set to a base URL to talk to a control plane other than the real one. **Debug builds
/// only** — see [`control_plane`].
pub const CONTROL_PLANE_ENV: &str = "MXB_CONTROL_PLANE";

/// The control plane's base URL, honouring the debug-only override.
///
/// The override exists so the loop can be exercised against `wrangler dev` without pointing
/// a test run at the live accounts, and `cfg!(debug_assertions)` is what keeps it out of
/// anything anyone is handed.
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
