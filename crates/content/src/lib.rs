//! Headless, fail-closed readers for the game content formats shared by `mxb-core` and
//! dedicated-server tooling.
//!
//! This crate intentionally knows nothing about Tauri, image decoding, secured-content
//! sidecars, or account state. It accepts ordinary ZIP-backed packages only. Opaque packages
//! remain unsupported unless the caller has already obtained a lawful plain representation.

pub mod manifest;
pub mod pkz;
pub mod rdf;
pub mod track;
pub mod trh;

pub use manifest::{aggregate_checksums, checksum_bytes};
pub use pkz::{entry_names, is_plain_zip, read_selected, read_selected_bytes};
pub use rdf::{
    PitBoard, PitLane, RdfBootstrap, Stall, StartingGrid, ThirtySecondsBoard, TimingLine,
    MAX_STALLS,
};
pub use track::{TrackPackage, WORLD_BLOCK_SIDE};
pub use trh::{
    beta21e_main_centreline_pose, beta21e_manifest_checks as beta21e_trh_manifest_checks,
    descriptor as trh_descriptor, Beta21eTrhManifestChecks, CentrelinePose, TrhDescriptor,
};
