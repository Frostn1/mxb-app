//! Voice chat: talk to the riders around you, on any server, with nothing to set up.
//!
//! That last part is the whole design constraint. The player installs the app — which many
//! already have — picks a microphone, and is talking. No second program to download, no
//! account to create by hand, no port to forward, and **nothing whatsoever for the server
//! operator to install**. A server has voice because riders with the app turned up on it.
//!
//! ## How the pieces fit
//!
//! ```text
//!   game ──► frostmod ──► who is on the grid, and where          (proximity, phase 4)
//!                              │
//!   control plane ◄── join(serverKey) ── engine ──► peers ══ Opus over DTLS ══► other apps
//!    a room per server,                    │
//!    signalling only                       └─► capture · encode · jitter · mix · playback
//! ```
//!
//! - [`devices`] — the microphone, the speakers, the meter, the push-to-talk key. Everything
//!   the player actually configures, and the only part of voice with a settings page.
//! - [`frame`] — what one 20 ms packet looks like on the wire.
//! - [`codec`] — Opus in and out, at the one format we ever use.
//! - [`jitter`] — per-sender reordering and loss concealment, because the network is not a
//!   pipe and a dropped frame must not become a click.
//! - [`signal`] — the control-plane end: claim an account if this app doesn't have one, join
//!   the room for the server we're on, and relay ICE.
//! - [`stun`] — what this socket looks like from outside, without which two riders behind
//!   home routers can describe themselves to each other and still never connect.
//! - [`ring`] — the lock-free handoff to the audio callback, which may not wait for anything.
//! - [`mesh`] — the peers themselves: one UDP socket, one `Rtc` per peer, demultiplexed by
//!   remote address.
//! - [`engine`] — the loop that owns all of the above and runs on its own thread.
//! - [`session`] — the supervisor that joins and leaves as the rider moves between servers,
//!   so none of this is ever something they have to do.
//!
//! ## Why a thread and not the async runtime
//!
//! The spike measured the transport adding essentially nothing to end-to-end latency, and
//! the poll loop adding all of it: a 5 ms sleep between polls cost 12 ms, a 1 ms sleep cost
//! 2.7 ms. Voice is a hard 20 ms cadence with a realtime audio callback at each end, so it
//! gets a dedicated thread that is never behind something else's work.

pub mod codec;
pub mod devices;
pub mod engine;
pub mod frame;
pub mod jitter;
pub mod mesh;
pub mod ring;
pub mod session;
pub mod signal;
pub mod stun;

// The settings page and the hotkey binding are wired to these names; re-exported so moving
// the device code into a submodule stayed invisible to the rest of the app.
pub use devices::{bind_ptt, devices, test_output, Devices, Monitor};

/// Opus's native rate, and what every device is resampled to. Also the only rate at which
/// 20 ms is a whole number of samples for every Opus frame size.
pub const SAMPLE_RATE: u32 = 48_000;

/// One packet of audio. 20 ms is the WebRTC default for a reason: short enough that a lost
/// frame is inaudible, long enough that per-packet overhead isn't most of the bandwidth.
pub const FRAME_MS: u64 = 20;

/// Samples in one frame, mono.
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize / 1000) * FRAME_MS as usize;

/// What we ask Opus for. Speech at 24 kbps is comfortably intelligible, and twenty riders
/// on push-to-talk with a proximity cull never approach a link's limit.
pub const BITRATE: i32 = 24_000;
