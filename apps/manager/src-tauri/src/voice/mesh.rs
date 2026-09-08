//! The peers themselves: one socket, one connection per rider, no server in the middle.
//!
//! Every rider in a room holds a direct encrypted channel to every other rider. Audio never
//! reaches our infrastructure, which is what makes voice on a server we don't own — and
//! can't bill anyone for — free to provide. It is also why there is nothing for a server
//! operator to install: the only thing they host is the race.
//!
//! **One UDP socket serves every peer.** Each packet is offered to each connection in turn
//! and the one it belongs to claims it (`Rtc::accepts`) — str0m's own pattern. A socket per
//! peer would mean twenty NAT mappings to keep alive instead of one, and twenty chances for
//! a router to give up on us mid-race.
//!
//! **A peer that can't be reached goes quiet on its own.** Its connection fails, the rest of
//! the room is untouched, and the rider hears everyone else. Voice degrading to fewer voices
//! is survivable; voice failing because one person is behind an awkward router is not.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Instant;

use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

/// The label both ends use for the audio channel.
const CHANNEL_LABEL: &str = "voice";

/// Biggest datagram we will read. DTLS records carrying a 20 ms frame are a few hundred
/// bytes; this is headroom, not an expectation.
const MTU: usize = 2000;

/// One rider's connection.
struct Peer {
    rtc: Rtc,
    channel: Option<ChannelId>,
    /// Held between making an offer and the answer coming back.
    pending: Option<SdpPendingOffer>,
    connected: bool,
}

/// What the mesh noticed while being polled.
#[derive(Debug, PartialEq)]
pub enum MeshEvent {
    /// A frame arrived from this peer.
    Frame { peer_id: String, payload: Vec<u8> },
    /// The channel to this peer is open — they can hear us now.
    Open { peer_id: String },
    /// This peer is gone: the connection failed, or they left.
    Gone { peer_id: String },
}

pub struct Mesh {
    socket: UdpSocket,
    /// This socket's address on the local network.
    ///
    /// Not `socket.local_addr()`, which is `0.0.0.0:port` for a wildcard bind: str0m matches
    /// an arriving packet against the candidate it was addressed to, and an unspecified
    /// address matches nothing, so every packet would be dropped as belonging to no peer.
    local: SocketAddr,
    /// The addresses we tell other riders to aim at.
    candidates: Vec<Candidate>,
    peers: HashMap<String, Peer>,
    buf: Vec<u8>,
}

impl Mesh {
    /// Bind the socket voice will use and work out how it looks from the outside.
    ///
    /// `stun_servers` may be empty, and discovery may fail — both leave us with only local
    /// candidates, which still connects two riders on the same network. It is a smaller
    /// feature, not a broken one.
    pub fn new(stun_servers: &[SocketAddr]) -> Result<Mesh, String> {
        let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))
            .map_err(|e| format!("Couldn't open a socket for voice: {e}"))?;
        let port = socket
            .local_addr()
            .map_err(|e| format!("Couldn't read the voice socket's port: {e}"))?
            .port();

        let local = primary_local_ip()
            .map(|ip| SocketAddr::from((ip, port)))
            .ok_or("Couldn't work out this machine's address for voice.")?;

        let mut candidates = Vec::new();
        {
            let host = local;
            if let Ok(candidate) = Candidate::host(host, "udp") {
                candidates.push(candidate);
            }
            // Discovery has to happen from this socket, before it goes non-blocking: the
            // mapping a router opens belongs to the socket, not to the machine.
            if let Some(public) = super::stun::public_address(&socket, stun_servers) {
                match Candidate::server_reflexive(public, host, "udp") {
                    Ok(candidate) => {
                        log::info!("[voice] this machine is {public} from outside");
                        candidates.push(candidate);
                    }
                    Err(e) => log::warn!("[voice] couldn't use the discovered address: {e}"),
                }
            } else {
                log::warn!("[voice] no STUN server answered; only riders on this network are reachable");
            }
        }
        if candidates.is_empty() {
            return Err("Couldn't work out this machine's address for voice.".into());
        }

        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Couldn't configure the voice socket: {e}"))?;

        Ok(Mesh { socket, local, candidates, peers: HashMap::new(), buf: vec![0; MTU] })
    }

    /// Used by the tests, and by the status line when there is one.
    #[allow(dead_code)]
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    #[allow(dead_code)]
    pub fn is_open(&self, peer_id: &str) -> bool {
        self.peers.get(peer_id).is_some_and(|p| p.channel.is_some())
    }

    /// Start a connection to a rider who was already in the room when we arrived.
    ///
    /// Only the newcomer ever offers. That rule comes from the room — it hands a joiner the
    /// list of who is already there, and tells everyone else only that someone arrived — and
    /// it is what stops two riders offering to each other at the same moment.
    pub fn offer(&mut self, peer_id: &str) -> Result<String, String> {
        let mut rtc = self.new_rtc();
        let mut api = rtc.sdp_api();
        api.add_channel_with_config(ChannelConfig {
            label: CHANNEL_LABEL.into(),
            // Voice is worthless late. Never retransmit, and never hold a frame back waiting
            // for one that was dropped.
            ordered: false,
            reliability: Reliability::MaxRetransmits { retransmits: 0 },
            ..Default::default()
        });
        let (offer, pending) = api.apply().ok_or("nothing to offer")?;
        let sdp = serde_json::to_string(&offer).map_err(|e| format!("Couldn't write an offer: {e}"))?;

        self.peers.insert(
            peer_id.to_string(),
            Peer { rtc, channel: None, pending: Some(pending), connected: false },
        );
        Ok(sdp)
    }

    /// Answer a rider who just arrived and offered to us.
    pub fn answer(&mut self, peer_id: &str, offer_sdp: &str) -> Result<String, String> {
        let offer: SdpOffer =
            serde_json::from_str(offer_sdp).map_err(|e| format!("Couldn't read their offer: {e}"))?;
        let mut rtc = self.new_rtc();
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("Couldn't accept their offer: {e}"))?;
        let sdp = serde_json::to_string(&answer).map_err(|e| format!("Couldn't write an answer: {e}"))?;

        self.peers.insert(
            peer_id.to_string(),
            Peer { rtc, channel: None, pending: None, connected: false },
        );
        Ok(sdp)
    }

    /// Take the answer to an offer we made.
    pub fn accept_answer(&mut self, peer_id: &str, answer_sdp: &str) -> Result<(), String> {
        let peer = self.peers.get_mut(peer_id).ok_or("no such peer")?;
        let pending = peer.pending.take().ok_or("we weren't waiting for an answer")?;
        let answer: SdpAnswer =
            serde_json::from_str(answer_sdp).map_err(|e| format!("Couldn't read their answer: {e}"))?;
        peer.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|e| format!("Couldn't complete the connection: {e}"))
    }

    /// Forget a rider — they left the room, or we are shutting down.
    pub fn remove(&mut self, peer_id: &str) {
        if let Some(mut peer) = self.peers.remove(peer_id) {
            peer.rtc.disconnect();
        }
    }

    /// Send one frame to every rider whose channel is open.
    ///
    /// Encoded once by the caller and written N times: Opus is the expensive part and it
    /// does not depend on who is listening.
    pub fn broadcast(&mut self, payload: &[u8]) {
        for (peer_id, peer) in self.peers.iter_mut() {
            let Some(channel) = peer.channel else { continue };
            if let Some(mut writer) = peer.rtc.channel(channel) {
                if let Err(e) = writer.write(true, payload) {
                    log::debug!("[voice] couldn't send to {peer_id}: {e}");
                }
            }
        }
    }

    /// Send one frame to a single rider — the proximity cull's entry point.
    ///
    /// Unused until phase 4, which is the point: culling by distance is the change that
    /// makes an open mic affordable at a full grid, and this is the whole of what it needs.
    #[allow(dead_code)]
    pub fn send_to(&mut self, peer_id: &str, payload: &[u8]) {
        let Some(peer) = self.peers.get_mut(peer_id) else { return };
        let Some(channel) = peer.channel else { return };
        if let Some(mut writer) = peer.rtc.channel(channel) {
            let _ = writer.write(true, payload);
        }
    }

    /// Read whatever has arrived, drive every connection forward, and report what happened.
    ///
    /// Returns the events and the instant the mesh next wants attention. The caller may sleep
    /// until then, but never longer — a connection that isn't polled stalls.
    pub fn poll(&mut self) -> (Vec<MeshEvent>, Instant) {
        let mut events = Vec::new();

        // Everything on the wire, handed to whichever connection claims it. The buffer is
        // moved out for the loop so a packet can be dispatched while it is borrowed.
        let mut buf = std::mem::take(&mut self.buf);
        while let Ok((n, source)) = self.socket.recv_from(&mut buf) {
            self.dispatch(&buf[..n], source);
        }
        self.buf = buf;

        let now = Instant::now();
        let mut next = now + std::time::Duration::from_millis(50);
        let mut dead = Vec::new();

        for (peer_id, peer) in self.peers.iter_mut() {
            loop {
                match peer.rtc.poll_output() {
                    Ok(Output::Timeout(at)) => {
                        next = next.min(at);
                        break;
                    }
                    Ok(Output::Transmit(t)) => {
                        let _ = self.socket.send_to(&t.contents, t.destination);
                    }
                    Ok(Output::Event(event)) => match event {
                        Event::ChannelOpen(id, label) if label == CHANNEL_LABEL => {
                            peer.channel = Some(id);
                            peer.connected = true;
                            events.push(MeshEvent::Open { peer_id: peer_id.clone() });
                        }
                        Event::ChannelData(data) => {
                            events.push(MeshEvent::Frame {
                                peer_id: peer_id.clone(),
                                payload: data.data,
                            });
                        }
                        Event::ChannelClose(_) => {
                            peer.channel = None;
                        }
                        Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                            dead.push(peer_id.clone());
                        }
                        _ => {}
                    },
                    Err(e) => {
                        log::debug!("[voice] connection to {peer_id} failed: {e}");
                        dead.push(peer_id.clone());
                        break;
                    }
                }
            }
            if !peer.rtc.is_alive() {
                dead.push(peer_id.clone());
            }
            let _ = peer.rtc.handle_input(Input::Timeout(now));
        }

        for peer_id in dead {
            if self.peers.remove(&peer_id).is_some() {
                events.push(MeshEvent::Gone { peer_id });
            }
        }

        (events, next)
    }

    /// Hand one datagram to the connection it belongs to.
    ///
    /// Anything nobody claims is dropped without comment. That is ordinary during a
    /// handshake — a peer's first packet can beat its answer to us — and it is also what
    /// happens to a stray packet from anywhere else, which is the behaviour we want.
    fn dispatch(&mut self, bytes: &[u8], source: SocketAddr) {
        for peer in self.peers.values_mut() {
            let Ok(contents) = Receive::new(Protocol::Udp, source, self.local, bytes) else {
                return;
            };
            let input = Input::Receive(Instant::now(), contents);
            if peer.rtc.accepts(&input) {
                let _ = peer.rtc.handle_input(input);
                return;
            }
        }
    }

    fn new_rtc(&self) -> Rtc {
        let mut rtc = Rtc::new(Instant::now());
        for candidate in &self.candidates {
            rtc.add_local_candidate(candidate.clone());
        }
        rtc
    }
}

/// This machine's address on its own network.
///
/// There is no portable way to ask "which interface would you use", so we ask the routing
/// table the way everything does: point a UDP socket at a public address — which sends
/// nothing and needs nothing to be reachable — and read back the local address the kernel
/// chose for it.
fn primary_local_ip() -> Option<Ipv4Addr> {
    let probe = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).ok()?;
    probe.connect("1.1.1.1:80").ok()?;
    match probe.local_addr().ok()? {
        SocketAddr::V4(addr) => Some(*addr.ip()),
        SocketAddr::V6(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two meshes on this machine, connected to each other with no signalling server: the
    /// offer and answer are handed across directly, which is all the room ever does.
    fn connected_pair() -> (Mesh, Mesh) {
        let mut alice = Mesh::new(&[]).expect("alice");
        let mut bob = Mesh::new(&[]).expect("bob");
        let offer = alice.offer("bob").expect("offer");
        let answer = bob.answer("alice", &offer).expect("answer");
        alice.accept_answer("bob", &answer).expect("accept");
        (alice, bob)
    }

    /// Drive both until something happens or the deadline passes.
    fn pump(alice: &mut Mesh, bob: &mut Mesh, deadline: std::time::Duration) -> Vec<MeshEvent> {
        let until = Instant::now() + deadline;
        let mut seen = Vec::new();
        while Instant::now() < until {
            let (a, _) = alice.poll();
            let (b, _) = bob.poll();
            seen.extend(a);
            seen.extend(b);
            if seen.iter().any(|e| matches!(e, MeshEvent::Frame { .. })) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        seen
    }

    #[test]
    fn a_local_address_is_always_available() {
        // Without this there are no candidates at all and voice cannot start, so it is worth
        // asserting rather than assuming.
        assert!(primary_local_ip().is_some());
    }

    #[test]
    fn two_riders_connect_and_one_frame_reaches_the_other() {
        let (mut alice, mut bob) = connected_pair();

        // Both channels open, then Alice talks.
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while Instant::now() < until && !(alice.is_open("bob") && bob.is_open("alice")) {
            alice.poll();
            bob.poll();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(alice.is_open("bob"), "the channel never opened");

        alice.broadcast(b"hello from alice");
        let events = pump(&mut alice, &mut bob, std::time::Duration::from_secs(2));
        assert!(
            events.iter().any(|e| matches!(e, MeshEvent::Frame { peer_id, payload }
                if peer_id == "alice" && payload == b"hello from alice")),
            "the frame never arrived: {events:?}"
        );
    }

    #[test]
    fn forgetting_a_peer_leaves_the_rest_of_the_room_alone() {
        let (mut alice, _bob) = connected_pair();
        alice.offer("carol").expect("offer");
        assert_eq!(alice.peer_count(), 2);
        alice.remove("carol");
        assert_eq!(alice.peer_count(), 1);
        assert!(alice.peers.contains_key("bob"));
    }

    #[test]
    fn nonsense_in_place_of_an_offer_is_an_error_not_a_panic() {
        let mut mesh = Mesh::new(&[]).expect("mesh");
        assert!(mesh.answer("someone", "not an sdp").is_err());
        assert!(mesh.accept_answer("nobody", "{}").is_err());
    }
}
