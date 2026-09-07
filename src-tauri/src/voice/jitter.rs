//! One buffer per talker, standing between the network and the speakers.
//!
//! Packets are sent every 20 ms and arrive whenever they arrive — early, late, out of order,
//! or not at all. Playback, meanwhile, asks for exactly one frame every 20 ms and cannot be
//! kept waiting: the audio callback is a realtime deadline, and missing it is a gap everyone
//! hears. This holds a small backlog so the common case of a slightly late packet is
//! invisible, and reports a loss when there is genuinely nothing to play.
//!
//! **Depth is the whole trade.** Every frame held is 20 ms added to how long it takes your
//! voice to reach someone. Racing is the case that cares — "left!" is worth nothing a beat
//! late — so this starts shallow and grows only when a talker's connection proves it needs
//! it, rather than picking a depth that is safe for everyone and slow for everyone.

use std::collections::BTreeMap;

/// Frames held before playback starts. Two is 40 ms — enough to absorb ordinary jitter on a
/// home connection without anyone noticing the delay.
pub const START_DEPTH: usize = 2;

/// The most we will ever hold. Past this, a connection is bad enough that the honest answer
/// is a dropout rather than a growing delay nobody asked for.
pub const MAX_DEPTH: usize = 10;

/// Frames a talker must arrive cleanly for before the buffer tries running shallower again.
const SHRINK_AFTER: u32 = 250; // ~5 seconds of talking

/// How far out of step a sequence number must be to count as a new stream rather than a late
/// packet. A talker who reconnects starts over at zero, and so does one whose app restarted.
const RESYNC_DISTANCE: u32 = 100;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stats {
    pub played: u64,
    /// Frames the sender sent that never arrived in time to be played.
    pub lost: u64,
    /// Frames that arrived after their slot had already been played and were thrown away.
    pub late: u64,
    /// Frames dropped because the buffer was already at `MAX_DEPTH`.
    pub overflowed: u64,
}

pub struct Jitter {
    frames: BTreeMap<u32, Vec<u8>>,
    /// The frame `pop` last handed out, kept alive for as long as the caller holds it.
    current: Vec<u8>,
    /// The sequence we will play next, or `None` before the first frame has been chosen.
    next: Option<u32>,
    depth: usize,
    clean_run: u32,
    pub stats: Stats,
}

/// What playback should do for this 20 ms slot.
#[derive(Debug, PartialEq)]
pub enum Play<'a> {
    /// Decode and play this packet.
    Frame(&'a [u8]),
    /// Nothing arrived: ask the decoder to invent one. Better than silence, which clicks.
    Conceal,
    /// This talker isn't talking. Not a loss — there is simply nothing to play.
    Idle,
}

impl Default for Jitter {
    fn default() -> Self {
        Jitter {
            frames: BTreeMap::new(),
            current: Vec::new(),
            next: None,
            depth: START_DEPTH,
            clean_run: 0,
            stats: Stats::default(),
        }
    }
}

impl Jitter {
    /// Take a packet off the network.
    pub fn push(&mut self, seq: u32, talk_start: bool, opus: &[u8]) {
        // A fresh burst of talking, or a talker whose numbering restarted: start over rather
        // than treating thousands of missing sequences as loss.
        if talk_start || self.is_out_of_range(seq) {
            self.frames.clear();
            self.next = None;
        }

        if let Some(next) = self.next {
            if seq < next {
                self.stats.late += 1;
                // A late frame is also evidence this buffer is running too shallow for this
                // talker — which is exactly when holding one more frame is worth its 20 ms.
                self.grow();
                return;
            }
        }

        if self.frames.len() >= MAX_DEPTH {
            self.stats.overflowed += 1;
            self.grow();
            // Drop the oldest rather than the newest: the newest is the one still worth
            // playing, and the oldest is nearly late already.
            if let Some(&oldest) = self.frames.keys().next() {
                self.frames.remove(&oldest);
            }
        }
        self.frames.insert(seq, opus.to_vec());
    }

    /// Hand playback the next 20 ms.
    pub fn pop(&mut self) -> Play<'_> {
        // Wait until there is a backlog worth playing from. Starting on the first packet to
        // arrive means the second one is already late.
        let Some(next) = self.next else {
            if self.frames.len() < self.depth {
                return Play::Idle;
            }
            let first = *self.frames.keys().next().expect("non-empty");
            self.next = Some(first);
            return self.take(first);
        };

        if self.frames.contains_key(&next) {
            self.clean_run += 1;
            if self.clean_run >= SHRINK_AFTER {
                self.shrink();
            }
            return self.take(next);
        }

        // Nothing for this slot. If there is nothing at all, the talker stopped; if there is
        // something later, this frame was lost and the show must go on.
        if self.frames.is_empty() {
            self.next = None;
            self.clean_run = 0;
            return Play::Idle;
        }
        self.stats.lost += 1;
        self.clean_run = 0;
        self.grow();
        self.next = Some(next.wrapping_add(1));
        Play::Conceal
    }

    /// How many frames are waiting — a talker's connection quality, in one number.
    #[allow(dead_code)]
    pub fn queued(&self) -> usize {
        self.frames.len()
    }

    fn take(&mut self, seq: u32) -> Play<'_> {
        self.next = Some(seq.wrapping_add(1));
        match self.frames.remove(&seq) {
            Some(opus) => {
                self.stats.played += 1;
                self.current = opus;
                Play::Frame(&self.current)
            }
            None => Play::Conceal,
        }
    }

    fn is_out_of_range(&self, seq: u32) -> bool {
        match self.next {
            Some(next) => seq.abs_diff(next) > RESYNC_DISTANCE,
            None => false,
        }
    }

    fn grow(&mut self) {
        self.depth = (self.depth + 1).min(MAX_DEPTH);
    }

    fn shrink(&mut self) {
        self.depth = self.depth.saturating_sub(1).max(START_DEPTH);
        self.clean_run = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `n` frames in order, starting at `from`.
    fn fill(j: &mut Jitter, from: u32, n: u32) {
        for seq in from..from + n {
            j.push(seq, seq == from && from == 0, &[seq as u8]);
        }
    }

    fn played(play: Play<'_>) -> Option<u8> {
        match play {
            Play::Frame(f) => Some(f[0]),
            _ => None,
        }
    }

    #[test]
    fn waits_for_a_backlog_before_it_starts() {
        let mut j = Jitter::default();
        j.push(0, true, &[0]);
        assert_eq!(j.pop(), Play::Idle, "one frame is not a backlog");
        j.push(1, false, &[1]);
        assert_eq!(played(j.pop()), Some(0), "two frames is");
    }

    #[test]
    fn plays_in_order_even_when_they_arrive_out_of_it() {
        let mut j = Jitter::default();
        j.push(1, false, &[1]);
        j.push(0, false, &[0]);
        assert_eq!(played(j.pop()), Some(0));
        assert_eq!(played(j.pop()), Some(1));
    }

    #[test]
    fn conceals_a_frame_that_never_came() {
        let mut j = Jitter::default();
        fill(&mut j, 0, 2);
        j.push(3, false, &[3]); // 2 is missing
        assert_eq!(played(j.pop()), Some(0));
        assert_eq!(played(j.pop()), Some(1));
        assert_eq!(j.pop(), Play::Conceal);
        assert_eq!(played(j.pop()), Some(3));
        assert_eq!(j.stats.lost, 1);
    }

    #[test]
    fn goes_idle_when_the_talker_stops_rather_than_concealing_forever() {
        let mut j = Jitter::default();
        fill(&mut j, 0, 2);
        j.pop();
        j.pop();
        assert_eq!(j.pop(), Play::Idle);
        assert_eq!(j.stats.lost, 0, "a rider who stopped talking has lost nothing");
    }

    #[test]
    fn throws_away_a_frame_whose_moment_has_passed() {
        let mut j = Jitter::default();
        fill(&mut j, 0, 3);
        j.pop();
        j.pop();
        j.push(0, false, &[0]); // arrives long after it was played
        assert_eq!(j.stats.late, 1);
        assert_eq!(played(j.pop()), Some(2), "the late frame did not displace the next one");
    }

    #[test]
    fn holds_more_for_a_talker_whose_packets_keep_arriving_late() {
        let mut j = Jitter::default();
        let start = j.depth;
        fill(&mut j, 0, 3);
        j.pop();
        j.pop();
        j.push(0, false, &[0]);
        assert!(j.depth > start, "a late frame should buy some slack");
    }

    #[test]
    fn never_holds_more_than_the_cap() {
        let mut j = Jitter::default();
        for seq in 0..MAX_DEPTH as u32 * 3 {
            j.push(seq, false, &[seq as u8]);
        }
        assert!(j.queued() <= MAX_DEPTH);
        assert!(j.stats.overflowed > 0);
    }

    #[test]
    fn starts_over_when_a_talker_reconnects() {
        let mut j = Jitter::default();
        fill(&mut j, 0, 3);
        j.pop();
        // Their app restarted: sequence numbers begin again, nowhere near where we were.
        j.push(0, true, &[99]);
        j.push(1, false, &[98]);
        assert_eq!(played(j.pop()), Some(99), "the new stream plays, not the old backlog");
    }

    #[test]
    fn treats_a_wildly_different_sequence_as_a_new_stream() {
        let mut j = Jitter::default();
        fill(&mut j, 0, 3);
        j.pop();
        j.push(RESYNC_DISTANCE + 500, false, &[7]);
        j.push(RESYNC_DISTANCE + 501, false, &[8]);
        assert_eq!(played(j.pop()), Some(7));
    }
}
