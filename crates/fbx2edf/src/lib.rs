//! FBX to EDF for Frost's Studio.
//!
//! The converter is private and is not in this repository (see Cargo.toml). When its sources
//! have been synced in, they are this crate's root modules and [`host`] calls them. Without
//! them, [`host::available`] is false and every call in [`host`] refuses.

#[cfg(fbx2edf)]
include!(concat!(env!("OUT_DIR"), "/private_root.rs"));

pub mod host;
