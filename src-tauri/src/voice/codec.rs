//! Opus, at the one format voice ever uses: 48 kHz, mono, 20 ms.
//!
//! Thin wrappers, and they exist for two reasons. The buffers are owned here rather than
//! allocated per frame — fifty allocations a second per talker is not a cost worth paying on
//! the thread that also feeds the speakers — and the errors become strings at this boundary,
//! so nothing downstream carries an `opus::Error` it can only log.

use super::{BITRATE, FRAME_SAMPLES, SAMPLE_RATE};

/// The largest an Opus frame can be at our settings, with room to spare.
const MAX_PACKET: usize = 512;

pub struct Encoder {
    inner: opus::Encoder,
    out: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Result<Self, String> {
        // `Voip` rather than `Audio`: it biases Opus toward speech intelligibility over
        // musical fidelity, which is the trade every rider on this channel wants.
        let mut inner = opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip)
            .map_err(|e| format!("Couldn't start the voice encoder: {e}"))?;
        inner
            .set_bitrate(opus::Bitrate::Bits(BITRATE))
            .map_err(|e| format!("Couldn't set the voice bitrate: {e}"))?;
        // Tell Opus to expect loss, so it spends a little bitrate on making a dropped frame
        // recoverable rather than assuming a perfect link. These are riders on home
        // connections, often on wifi, sometimes on a phone hotspot at a track.
        let _ = inner.set_inband_fec(true);
        let _ = inner.set_packet_loss_perc(5);
        Ok(Encoder { inner, out: vec![0; MAX_PACKET] })
    }

    /// Encode exactly one frame. The slice is valid until the next call.
    pub fn encode(&mut self, pcm: &[i16]) -> Result<&[u8], String> {
        debug_assert_eq!(pcm.len(), FRAME_SAMPLES);
        let n = self
            .inner
            .encode(pcm, &mut self.out)
            .map_err(|e| format!("Couldn't encode a voice frame: {e}"))?;
        Ok(&self.out[..n])
    }
}

pub struct Decoder {
    inner: opus::Decoder,
    out: Vec<i16>,
}

impl Decoder {
    pub fn new() -> Result<Self, String> {
        Ok(Decoder {
            inner: opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono)
                .map_err(|e| format!("Couldn't start the voice decoder: {e}"))?,
            out: vec![0; FRAME_SAMPLES],
        })
    }

    /// Decode one packet.
    pub fn decode(&mut self, packet: &[u8]) -> Result<&[i16], String> {
        let n = self
            .inner
            .decode(packet, &mut self.out, false)
            .map_err(|e| format!("Couldn't decode a voice frame: {e}"))?;
        Ok(&self.out[..n])
    }

    /// Invent the frame that didn't arrive.
    ///
    /// Opus reconstructs from what it already decoded, which turns a dropped packet into a
    /// brief smear instead of a click. The alternative — a frame of zeroes — is the sound
    /// people describe as "choppy", and it is much more noticeable than this.
    pub fn conceal(&mut self) -> Result<&[i16], String> {
        let n = self
            .inner
            .decode(&[], &mut self.out, false)
            .map_err(|e| format!("Couldn't conceal a lost voice frame: {e}"))?;
        Ok(&self.out[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frame: usize) -> Vec<i16> {
        let base = frame * FRAME_SAMPLES;
        (0..FRAME_SAMPLES)
            .map(|i| {
                let t = (base + i) as f32 / SAMPLE_RATE as f32;
                ((t * 440.0 * std::f32::consts::TAU).sin() * 0.25 * i16::MAX as f32) as i16
            })
            .collect()
    }

    fn rms(pcm: &[i16]) -> f32 {
        let sum: f64 = pcm.iter().map(|s| (*s as f64).powi(2)).sum();
        (sum / pcm.len().max(1) as f64).sqrt() as f32 / i16::MAX as f32
    }

    #[test]
    fn a_frame_survives_the_round_trip() {
        let mut encoder = Encoder::new().expect("encoder");
        let mut decoder = Decoder::new().expect("decoder");

        // Warm up: the first frames of any Opus stream are the codec finding its feet.
        let mut decoded = Vec::new();
        for i in 0..10 {
            let packet = encoder.encode(&tone(i)).expect("encode").to_vec();
            decoded = decoder.decode(&packet).expect("decode").to_vec();
        }

        assert_eq!(decoded.len(), FRAME_SAMPLES);
        // Same energy in as out is the whole claim: a tone went in and a tone came back.
        assert!((rms(&decoded) - rms(&tone(0))).abs() < 0.03, "rms {}", rms(&decoded));
    }

    #[test]
    fn a_frame_fits_in_a_packet_worth_paying_for() {
        let mut encoder = Encoder::new().expect("encoder");
        for i in 0..10 {
            let packet = encoder.encode(&tone(i)).expect("encode");
            // 24 kbps at 50 packets a second is 60 bytes. Anything near MAX_PACKET would
            // mean the bitrate setting silently didn't take.
            assert!(packet.len() < 120, "frame {i} was {} bytes", packet.len());
        }
    }

    #[test]
    fn concealment_produces_a_frame_rather_than_an_error() {
        let mut encoder = Encoder::new().expect("encoder");
        let mut decoder = Decoder::new().expect("decoder");
        for i in 0..5 {
            let packet = encoder.encode(&tone(i)).expect("encode").to_vec();
            decoder.decode(&packet).expect("decode");
        }
        assert_eq!(decoder.conceal().expect("conceal").len(), FRAME_SAMPLES);
    }
}
