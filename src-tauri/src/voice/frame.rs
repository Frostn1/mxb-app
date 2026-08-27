//! What one 20 ms packet looks like on the wire.
//!
//! Twelve bytes of header on roughly sixty of Opus. It is deliberately small: at fifty
//! packets a second per talker, every byte here is 400 bits per second per listener.
//!
//! There is no authentication field and no encryption of our own. The data channel is
//! already DTLS — a peer we did not complete a handshake with cannot put a packet on it —
//! and a second layer would be one more thing to get wrong for no property we don't have.
//!
//! The `race_num` is the one field a receiver acts on beyond the audio: it is how a voice is
//! matched to a rider on the track, both to place them in the stereo field and to decide
//! they are not on the grid at all and should not be heard.

/// Bumped if the layout below ever changes. A receiver drops anything it doesn't know, so an
/// old app and a new one fall silent to each other rather than decoding garbage.
pub const VERSION: u8 = 1;

/// Version, flags, race number, sequence, timestamp.
pub const HEADER_BYTES: usize = 1 + 1 + 2 + 4 + 4;

/// A frame's worth of Opus, plus who sent it and when.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame<'a> {
    /// The sender's race number in the current session, or 0 before they have one.
    pub race_num: u16,
    /// Wraps at ~2.4 years of continuous talking; the jitter buffer only ever compares
    /// nearby values.
    pub seq: u32,
    /// Milliseconds since the sender's engine started. Only ever differenced against another
    /// timestamp from the same sender, so the clocks never have to agree.
    pub sent_ms: u32,
    /// True on the first frame after the mic opens, so a receiver can reset its buffer
    /// instead of treating a gap as loss.
    pub talk_start: bool,
    pub opus: &'a [u8],
}

impl Frame<'_> {
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.clear();
        out.reserve(HEADER_BYTES + self.opus.len());
        out.push(VERSION);
        out.push(if self.talk_start { 1 } else { 0 });
        out.extend_from_slice(&self.race_num.to_le_bytes());
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.extend_from_slice(&self.sent_ms.to_le_bytes());
        out.extend_from_slice(self.opus);
    }

    /// Parse a packet, or `None` if it isn't one of ours.
    ///
    /// Everything a peer sends arrives here, including whatever a peer running a different
    /// version — or nothing like our app at all — decides to write to the channel. A wrong
    /// answer is silence, never a panic: this runs on the engine thread, which is also the
    /// audio thread's supplier.
    pub fn decode(bytes: &[u8]) -> Option<Frame<'_>> {
        if bytes.len() <= HEADER_BYTES || bytes[0] != VERSION {
            return None;
        }
        Some(Frame {
            talk_start: bytes[1] & 1 == 1,
            race_num: u16::from_le_bytes([bytes[2], bytes[3]]),
            seq: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            sent_ms: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            opus: &bytes[HEADER_BYTES..],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut out = Vec::new();
        Frame { race_num: 42, seq: 7, sent_ms: 1234, talk_start: true, opus: &[9, 8, 7] }
            .encode(&mut out);
        out
    }

    #[test]
    fn round_trips() {
        let bytes = sample();
        let frame = Frame::decode(&bytes).expect("a frame");
        assert_eq!(frame.race_num, 42);
        assert_eq!(frame.seq, 7);
        assert_eq!(frame.sent_ms, 1234);
        assert!(frame.talk_start);
        assert_eq!(frame.opus, &[9, 8, 7]);
    }

    #[test]
    fn costs_twelve_bytes_over_the_audio() {
        assert_eq!(sample().len(), HEADER_BYTES + 3);
    }

    #[test]
    fn refuses_anything_that_isnt_one_of_ours() {
        assert!(Frame::decode(&[]).is_none());
        // Header but no audio: nothing to decode, and an empty Opus packet is not silence.
        assert!(Frame::decode(&[VERSION, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_none());
        let mut wrong_version = sample();
        wrong_version[0] = VERSION + 1;
        assert!(Frame::decode(&wrong_version).is_none());
    }

    #[test]
    fn a_short_read_never_panics() {
        // Every prefix of a valid frame, because a channel can hand us anything.
        let bytes = sample();
        for n in 0..bytes.len() {
            let _ = Frame::decode(&bytes[..n]);
        }
    }
}
