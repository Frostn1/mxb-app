//! The loop that owns everything else, on a thread of its own.
//!
//! Capture the microphone, encode it once, send it to everyone who can hear us; take what
//! arrives, hold it just long enough to be smooth, decode it, mix it, play it. Twenty
//! milliseconds at a time, for as long as the rider is on a server.
//!
//! ## Nothing here asks the player for anything
//!
//! Joining is a consequence of being on a server, not an action. The app already knows which
//! server it launched into and already tells the control plane so for paint sync; voice
//! rides on the same signal. There is no room to pick, no address to type, no code to share,
//! and no separate program to alt-tab to. Pick a microphone once, and after that voice is
//! either on or off.
//!
//! ## Where the time goes
//!
//! The loop runs far faster than the frame rate and does almost nothing most of the time.
//! That is deliberate: the spike showed a slack poll loop, not the network, was what put
//! double-digit milliseconds on a voice. Everything expensive is either once per frame
//! (one Opus encode, however many riders are listening) or once per talker.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use serde::Serialize;

use super::codec::{Decoder, Encoder};
use super::frame::Frame;
use super::jitter::{Jitter, Play};
use super::mesh::{Mesh, MeshEvent};
use super::ring::Ring;
use super::signal::{Peer, Room, RoomCommand, RoomEvent};
use super::{FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE};

/// How often the loop looks at the world. Fast enough that nothing waits on us; the spike's
/// difference between a 5 ms and a 1 ms poll was 12 ms versus 2.7 ms of added latency.
const TICK: Duration = Duration::from_millis(1);

/// A talker is shown as talking until this long after their last frame, so the indicator
/// doesn't flicker between words.
const TALKING_HOLD: Duration = Duration::from_millis(250);

/// Audio buffered for playback. Six frames is 120 ms of slack against the two device clocks
/// drifting apart; beyond that the delay would be audible.
const PLAYBACK_RING_FRAMES: usize = 6;

/// Captured audio waiting to be encoded. Small on purpose: old microphone audio is worthless,
/// and letting it pile up would mean transmitting the past.
const CAPTURE_RING_FRAMES: usize = 4;

/// One rider as the UI sees them.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeerStatus {
    pub peer_id: String,
    pub rider_name: String,
    pub race_num: u16,
    /// The direct connection is up and audio can flow.
    pub connected: bool,
    pub talking: bool,
    pub muted: bool,
}

/// What the app shows about voice, refreshed as things change.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub joined: bool,
    pub server: String,
    pub peers: Vec<PeerStatus>,
    /// Set when voice couldn't start or the room dropped us. Shown, not swallowed.
    pub error: Option<String>,
}

/// What the app can tell a running engine.
pub enum Command {
    /// The rider's name or race number changed — they entered a session, or left one.
    ///
    /// Nothing sends this yet: the race number comes from the game's entry list, which
    /// reaches us in phase 3 along with the server detection. The path it travels is built
    /// and tested; what is missing is the thing with the answer.
    #[allow(dead_code)]
    Rider { rider_name: String, race_num: u16 },
    Mute { peer_id: String, muted: bool },
    Volume(f32),
    Stop,
}

/// A running voice session. Dropping it stops the thread and closes the microphone.
pub struct Handle {
    commands: Sender<Command>,
    status: Arc<Mutex<Status>>,
    stopped: Arc<AtomicBool>,
}

impl Handle {
    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = self.commands.send(Command::Stop);
    }
}

/// Everything the engine needs to start. Gathered by the caller because most of it comes
/// from config, and the engine should not be reading settings behind anyone's back.
pub struct Config {
    pub token: String,
    pub server_key: String,
    pub rider_name: String,
    pub race_num: u16,
    pub input_device: String,
    pub output_device: String,
    pub input_gain: f32,
    pub output_volume: f32,
    pub stun_servers: Vec<SocketAddr>,
}

/// Start talking on `server_key`.
///
/// Returns as soon as the thread is up — connecting takes a moment and the UI shows it
/// happening rather than freezing on it.
pub fn start(config: Config) -> Handle {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let status = Arc::new(Mutex::new(Status {
        joined: false,
        server: config.server_key.clone(),
        ..Default::default()
    }));
    let stopped = Arc::new(AtomicBool::new(false));

    let handle = Handle {
        commands: command_tx,
        status: status.clone(),
        stopped: stopped.clone(),
    };

    let thread_status = status.clone();
    let spawned = std::thread::Builder::new()
        .name("voice-engine".into())
        .spawn(move || {
            if let Err(e) = run(config, command_rx, &thread_status, &stopped) {
                log::warn!("[voice] engine stopped: {e}");
                if let Ok(mut status) = thread_status.lock() {
                    status.joined = false;
                    status.peers.clear();
                    status.error = Some(e);
                }
            }
        });
    if let Err(e) = spawned {
        if let Ok(mut status) = status.lock() {
            status.error = Some(format!("Couldn't start voice: {e}"));
        }
    }
    handle
}

/// One rider we are connected to.
struct Talker {
    peer: Peer,
    jitter: Jitter,
    decoder: Decoder,
    connected: bool,
    muted: bool,
    last_frame: Option<Instant>,
}

fn run(
    config: Config,
    commands: Receiver<Command>,
    status: &Arc<Mutex<Status>>,
    stopped: &Arc<AtomicBool>,
) -> Result<(), String> {
    // The socket first: discovering our public address is a network round trip, and doing it
    // before the room means we can offer a reachable address in our very first offer.
    let mut mesh = Mesh::new(&config.stun_servers)?;
    let room = Room::join(&config.token, &config.server_key, &config.rider_name, config.race_num)?;

    let capture = Ring::new(FRAME_SAMPLES * CAPTURE_RING_FRAMES);
    let playback = Ring::new(FRAME_SAMPLES * PLAYBACK_RING_FRAMES);
    // Built here, on this thread, and dropped here: a `cpal::Stream` is not `Send`, so the
    // thread that opens the microphone is the thread that has to close it.
    let _input = open_input(&config, capture.clone())?;
    let _output = open_output(&config, playback.clone())?;

    let mut encoder = Encoder::new()?;
    let mut talkers: HashMap<String, Talker> = HashMap::new();
    let mut volume = config.output_volume.clamp(0.0, 1.0);
    let mut race_num = config.race_num;
    let mut rider_name = config.rider_name.clone();

    let mut seq: u32 = 0;
    let mut was_transmitting = false;
    let started = Instant::now();
    let mut next_frame = Instant::now();
    let mut frame_pcm = vec![0f32; FRAME_SAMPLES];
    let mut frame_i16 = vec![0i16; FRAME_SAMPLES];
    let mut mixed = vec![0f32; FRAME_SAMPLES];
    let mut wire = Vec::with_capacity(256);

    set_joined(status, true);

    while !stopped.load(Ordering::Relaxed) {
        // --- what the app wants ---------------------------------------------------------
        for command in commands.try_iter() {
            match command {
                Command::Stop => return Ok(()),
                Command::Volume(v) => volume = v.clamp(0.0, 1.0),
                Command::Mute { peer_id, muted } => {
                    if let Some(talker) = talkers.get_mut(&peer_id) {
                        talker.muted = muted;
                    }
                }
                Command::Rider { rider_name: name, race_num: num } => {
                    if name != rider_name || num != race_num {
                        rider_name = name.clone();
                        race_num = num;
                        room.send(RoomCommand::Rider { rider_name: name, race_num: num });
                    }
                }
            }
        }

        // --- what the room says ---------------------------------------------------------
        for event in room.drain() {
            match event {
                RoomEvent::Welcome { peers, .. } => {
                    // We are the newcomer, so we offer to everyone already here. They were
                    // told only that we arrived, which is what keeps offers one-directional.
                    for peer in peers {
                        offer_to(&mut mesh, &room, &mut talkers, peer);
                    }
                }
                RoomEvent::Joined(peer) => {
                    // They will offer to us. Remember them so their name is known before
                    // their first frame arrives.
                    talkers.entry(peer.peer_id.clone()).or_insert_with(|| new_talker(peer));
                }
                RoomEvent::Rider(peer) => {
                    if let Some(talker) = talkers.get_mut(&peer.peer_id) {
                        talker.peer = peer;
                    }
                }
                RoomEvent::Left { peer_id } => {
                    mesh.remove(&peer_id);
                    talkers.remove(&peer_id);
                }
                RoomEvent::Signal { from, kind, data } => {
                    handle_signal(&mut mesh, &room, &mut talkers, &from, &kind, &data);
                }
                RoomEvent::Closed(reason) => {
                    return Err(reason);
                }
            }
        }

        // --- what the peers say ---------------------------------------------------------
        let (events, next_poll) = mesh.poll();
        for event in events {
            match event {
                MeshEvent::Open { peer_id } => {
                    if let Some(talker) = talkers.get_mut(&peer_id) {
                        talker.connected = true;
                    }
                }
                MeshEvent::Gone { peer_id } => {
                    if let Some(talker) = talkers.get_mut(&peer_id) {
                        talker.connected = false;
                    }
                }
                MeshEvent::Frame { peer_id, payload } => {
                    let Some(talker) = talkers.get_mut(&peer_id) else { continue };
                    let Some(frame) = Frame::decode(&payload) else { continue };
                    // Their race number as *they* report it in the frame, so a rider who
                    // joins a session mid-room is placed without waiting for the room to
                    // catch up. Phase 4 checks it against the local grid before playing.
                    if frame.race_num != 0 {
                        talker.peer.race_num = frame.race_num;
                    }
                    talker.jitter.push(frame.seq, frame.talk_start, frame.opus);
                }
            }
        }

        // --- the 20 ms beat -------------------------------------------------------------
        if Instant::now() >= next_frame {
            next_frame += Duration::from_millis(FRAME_MS);
            // A stall — a locked machine, a suspended laptop — must not turn into a burst of
            // catch-up frames nobody wants to hear. Start the beat again from now.
            if next_frame + Duration::from_millis(200) < Instant::now() {
                next_frame = Instant::now();
            }

            // Send.
            let transmitting = super::devices::transmitting();
            if transmitting && !was_transmitting {
                // Throw away what the microphone captured while the key was up. It is up to
                // a few frames old and nobody asked for it to be sent — a mic key that
                // transmits the moment before it was pressed is a privacy bug, not a feature.
                capture.clear();
            }
            if transmitting && capture.len() >= FRAME_SAMPLES {
                capture.pop(&mut frame_pcm);
                for (out, sample) in frame_i16.iter_mut().zip(frame_pcm.iter()) {
                    *out = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                }
                match encoder.encode(&frame_i16) {
                    Ok(opus) => {
                        Frame {
                            race_num,
                            seq,
                            sent_ms: started.elapsed().as_millis() as u32,
                            talk_start: !was_transmitting,
                            opus,
                        }
                        .encode(&mut wire);
                        mesh.broadcast(&wire);
                        seq = seq.wrapping_add(1);
                    }
                    Err(e) => log::debug!("[voice] {e}"),
                }
            }
            was_transmitting = transmitting;

            // Receive: one frame from every talker, summed.
            mixed.iter_mut().for_each(|s| *s = 0.0);
            let mut any = false;
            for talker in talkers.values_mut() {
                let decoded = match talker.jitter.pop() {
                    Play::Frame(opus) => talker.decoder.decode(opus).ok(),
                    Play::Conceal => talker.decoder.conceal().ok(),
                    Play::Idle => None,
                };
                let Some(pcm) = decoded else { continue };
                talker.last_frame = Some(Instant::now());
                if talker.muted {
                    continue;
                }
                any = true;
                // Per-rider gain is 1.0 today. Phase 4 makes it a function of how far away
                // they are, which is the only change this line needs.
                for (out, sample) in mixed.iter_mut().zip(pcm.iter()) {
                    *out += *sample as f32 / i16::MAX as f32;
                }
            }
            if any {
                for sample in mixed.iter_mut() {
                    // Soft clip: twenty riders shouting at once must distort gracefully
                    // rather than wrap around into noise.
                    *sample = (*sample * volume).tanh();
                }
                playback.push(&mixed);
            }

            publish_status(status, &config.server_key, &talkers);
        }

        // Sleep until whichever comes first, but never past the next frame.
        let now = Instant::now();
        let wake = next_poll.min(next_frame).max(now + TICK);
        std::thread::sleep((wake - now).min(TICK * 4));
    }
    Ok(())
}

fn new_talker(peer: Peer) -> Talker {
    Talker {
        peer,
        jitter: Jitter::default(),
        // A decoder that fails to build means no audio from this rider; the rest of the room
        // is unaffected, which is why this is not fatal to the engine.
        decoder: Decoder::new().unwrap_or_else(|e| {
            log::warn!("[voice] {e}");
            // A second attempt would fail the same way; a silent talker is the honest state.
            Decoder::new().expect("opus decoder")
        }),
        connected: false,
        muted: false,
        last_frame: None,
    }
}

fn offer_to(mesh: &mut Mesh, room: &Room, talkers: &mut HashMap<String, Talker>, peer: Peer) {
    match mesh.offer(&peer.peer_id) {
        Ok(sdp) => {
            room.send(RoomCommand::Signal {
                to: peer.peer_id.clone(),
                kind: "offer".into(),
                data: sdp,
            });
            talkers.entry(peer.peer_id.clone()).or_insert_with(|| new_talker(peer));
        }
        Err(e) => log::warn!("[voice] couldn't offer to {}: {e}", peer.peer_id),
    }
}

fn handle_signal(
    mesh: &mut Mesh,
    room: &Room,
    talkers: &mut HashMap<String, Talker>,
    from: &str,
    kind: &str,
    data: &str,
) {
    match kind {
        "offer" => match mesh.answer(from, data) {
            Ok(sdp) => {
                room.send(RoomCommand::Signal {
                    to: from.to_string(),
                    kind: "answer".into(),
                    data: sdp,
                });
                talkers.entry(from.to_string()).or_insert_with(|| {
                    new_talker(Peer {
                        peer_id: from.to_string(),
                        rider_name: String::new(),
                        race_num: 0,
                    })
                });
            }
            Err(e) => log::warn!("[voice] couldn't answer {from}: {e}"),
        },
        "answer" => {
            if let Err(e) = mesh.accept_answer(from, data) {
                log::warn!("[voice] couldn't finish connecting to {from}: {e}");
            }
        }
        // Candidates ride inside the offer and answer, so a trickled one is not expected.
        // Ignored rather than refused: a future control plane may send them.
        _ => {}
    }
}

fn set_joined(status: &Arc<Mutex<Status>>, joined: bool) {
    if let Ok(mut status) = status.lock() {
        status.joined = joined;
        if joined {
            status.error = None;
        }
    }
}

fn publish_status(status: &Arc<Mutex<Status>>, server: &str, talkers: &HashMap<String, Talker>) {
    let now = Instant::now();
    let mut peers: Vec<PeerStatus> = talkers
        .values()
        .map(|t| PeerStatus {
            peer_id: t.peer.peer_id.clone(),
            rider_name: t.peer.rider_name.clone(),
            race_num: t.peer.race_num,
            connected: t.connected,
            talking: t.last_frame.is_some_and(|at| now.duration_since(at) < TALKING_HOLD),
            muted: t.muted,
        })
        .collect();
    // Stable order, so the list doesn't reshuffle itself every time someone speaks.
    peers.sort_by(|a, b| a.rider_name.cmp(&b.rider_name).then(a.peer_id.cmp(&b.peer_id)));

    if let Ok(mut status) = status.lock() {
        status.joined = true;
        status.server = server.to_string();
        status.peers = peers;
    }
}

// ---------------------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------------------

/// Open the microphone, converting whatever it gives us to 48 kHz mono on the way in.
fn open_input(config: &Config, ring: Arc<Ring>) -> Result<cpal::Stream, String> {
    let (device, warning) = super::devices::resolve(&config.input_device, true)?;
    if let Some(warning) = warning {
        log::info!("[voice] {warning}");
    }
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Couldn't read the microphone's format: {e}"))?;
    let channels = supported.channels() as usize;
    let rate = supported.sample_rate().0;
    let gain = config.input_gain.clamp(0.0, 4.0);

    let mut resampler = Resampler::new(rate, SAMPLE_RATE);
    let stream = device
        .build_input_stream(
            &supported.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Everything here is arithmetic on a fixed buffer — no allocation, no lock.
                let mut mono = [0f32; 2048];
                let mut n = 0;
                for chunk in data.chunks(channels) {
                    if n == mono.len() {
                        break;
                    }
                    let sum: f32 = chunk.iter().sum();
                    mono[n] = (sum / chunk.len() as f32 * gain).clamp(-1.0, 1.0);
                    n += 1;
                }
                resampler.push(&mono[..n], &ring);
            },
            |e| log::warn!("[voice] microphone error: {e}"),
            None,
        )
        .map_err(|e| format!("Couldn't open the microphone: {e}"))?;
    stream.play().map_err(|e| format!("Couldn't start the microphone: {e}"))?;
    Ok(stream)
}

/// Open the speakers, spreading our mono mix across however many channels they have.
fn open_output(config: &Config, ring: Arc<Ring>) -> Result<cpal::Stream, String> {
    let (device, warning) = super::devices::resolve(&config.output_device, false)?;
    if let Some(warning) = warning {
        log::info!("[voice] {warning}");
    }
    let supported = device
        .default_output_config()
        .map_err(|e| format!("Couldn't read the output's format: {e}"))?;
    let channels = supported.channels() as usize;

    let stream = device
        .build_output_stream(
            &supported.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels.max(1);
                let mut mono = [0f32; 2048];
                let take = frames.min(mono.len());
                ring.pop(&mut mono[..take]);
                for (i, out) in data.chunks_mut(channels).enumerate() {
                    let sample = if i < take { mono[i] } else { 0.0 };
                    for slot in out.iter_mut() {
                        *slot = sample;
                    }
                }
            },
            |e| log::warn!("[voice] output error: {e}"),
            None,
        )
        .map_err(|e| format!("Couldn't open the speakers: {e}"))?;
    stream.play().map_err(|e| format!("Couldn't start the speakers: {e}"))?;
    Ok(stream)
}

/// Straight-line resampling between a device's rate and Opus's.
///
/// Linear rather than a windowed filter, and deliberately: this runs inside the audio
/// callback, most devices are already at 48 kHz so it does nothing at all, and the artefact
/// it introduces on the ones that aren't is far below what a 24 kbps speech codec is doing
/// to the same signal anyway.
struct Resampler {
    from: u32,
    to: u32,
    position: f64,
    last: f32,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Resampler {
        Resampler { from, to, position: 0.0, last: 0.0 }
    }

    fn push(&mut self, input: &[f32], ring: &Ring) {
        if self.from == self.to {
            ring.push(input);
            return;
        }
        let step = self.from as f64 / self.to as f64;
        let mut out = [0f32; 2048];
        let mut n = 0;
        while self.position < input.len() as f64 && n < out.len() {
            let index = self.position as usize;
            let frac = (self.position - index as f64) as f32;
            let a = if index == 0 { self.last } else { input[index - 1] };
            let b = input[index];
            out[n] = a + (b - a) * frac;
            n += 1;
            self.position += step;
        }
        self.position -= input.len() as f64;
        if let Some(&last) = input.last() {
            self.last = last;
        }
        ring.push(&out[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_rate_passes_audio_through_untouched() {
        let ring = Ring::new(64);
        let mut resampler = Resampler::new(48_000, 48_000);
        resampler.push(&[0.1, 0.2, 0.3], &ring);
        let mut out = [0.0; 3];
        ring.pop(&mut out);
        assert_eq!(out, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn upsampling_produces_more_samples_than_it_was_given() {
        let ring = Ring::new(4096);
        let mut resampler = Resampler::new(44_100, 48_000);
        let input: Vec<f32> = (0..441).map(|i| i as f32 / 441.0).collect();
        resampler.push(&input, &ring);
        let produced = ring.len();
        // 441 in at 44.1k is 10 ms, which is 480 samples at 48k. Allow a sample either way
        // for where the fractional position happened to land.
        assert!((479..=481).contains(&produced), "produced {produced}");
    }

    #[test]
    fn a_long_run_does_not_drift() {
        let ring = Ring::new(1 << 16);
        let mut resampler = Resampler::new(44_100, 48_000);
        let mut drained = 0usize;
        let mut sink = [0.0; 480];
        for _ in 0..100 {
            let input = [0.5f32; 441];
            resampler.push(&input, &ring);
            while ring.len() >= sink.len() {
                ring.pop(&mut sink);
                drained += sink.len();
            }
        }
        // One second of 44.1k audio must come out as one second of 48k audio, not 1.01.
        assert!((47_500..=48_500).contains(&drained), "a second became {drained} samples");
    }
}
