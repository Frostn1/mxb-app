//! The shared core: the game's own file formats, and the pieces of the app that read them.
//!
//! Everything here is consumed by more than one binary, which is the only reason a module
//! belongs in this crate. Anything that serves one app — installing, launching, the mod
//! browser, the track compilers — stays with that app.
//!
//! Modules keep the names they had in the app, and the apps re-export them at their own
//! root (`pub(crate) use mxb_core::edf;`), so the several thousand `crate::edf::…` call
//! sites elsewhere kept resolving through the move unchanged.

pub mod bikefiles;
pub mod cfg;
pub mod edf;
pub mod gate;
pub mod heightfield;
pub mod linkwalk;
pub mod lru;
pub mod map;
pub mod names;
pub mod paint;
pub mod paintwatch;
pub mod pkz;
pub mod scenery;
pub mod texstore;
pub mod track;

/// The optional local-only module. Absent from the public tree; `build.rs` sets `cfg(sidecar)`
/// when the file is there and publishes that decision to dependent crates.
#[cfg(sidecar)]
pub mod sidecar;
