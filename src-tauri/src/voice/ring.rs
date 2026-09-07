//! A lock-free handoff between the engine thread and the audio callback.
//!
//! The callback that feeds the speakers runs on a realtime thread with a hard deadline. It
//! may not allocate and it may not take a lock — if it blocks on a mutex the engine happens
//! to be holding, the result is a gap in the audio, which is exactly the failure voice chat
//! is judged on.
//!
//! So the two sides share a fixed buffer and two counters. One thread only ever writes, the
//! other only ever reads, and neither can make the other wait.
//!
//! **Underrun is silence, overrun is dropped audio, and both are survivable.** A ring that
//! blocked to avoid either would trade a moment of imperfect sound for a stall in the
//! process that produces all of it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A single-producer, single-consumer float ring.
pub struct Ring {
    /// Fixed at construction and never resized, so neither side ever allocates.
    buf: Vec<std::cell::UnsafeCell<f32>>,
    write: AtomicUsize,
    read: AtomicUsize,
    /// Counts samples the consumer asked for and didn't get — the number that says "this
    /// machine can't keep up" rather than "the network is bad".
    underruns: AtomicUsize,
}

// SAFETY: the cells are only ever touched through `push`/`pop`, which are documented as
// single-producer / single-consumer. The atomics order every write before the read that can
// observe it.
unsafe impl Sync for Ring {}
unsafe impl Send for Ring {}

impl Ring {
    /// A ring holding `capacity` samples. One slot is always left empty, which is what makes
    /// "full" and "empty" distinguishable without a third counter.
    pub fn new(capacity: usize) -> Arc<Ring> {
        Arc::new(Ring {
            buf: (0..capacity + 1).map(|_| std::cell::UnsafeCell::new(0.0)).collect(),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            underruns: AtomicUsize::new(0),
        })
    }

    /// Samples waiting to be played.
    pub fn len(&self) -> usize {
        let write = self.write.load(Ordering::Acquire);
        let read = self.read.load(Ordering::Acquire);
        write.wrapping_sub(read) % self.buf.len()
    }

    /// Samples that had to be silence. "This machine can't keep up", as distinct from
    /// "the network is bad" — worth having separate when someone reports choppy audio.
    #[allow(dead_code)]
    pub fn underruns(&self) -> usize {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Producer side. Returns how many samples were taken; the rest were dropped because the
    /// consumer has fallen behind.
    pub fn push(&self, samples: &[f32]) -> usize {
        let mut write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let mut written = 0;

        for &sample in samples {
            let next = (write + 1) % self.buf.len();
            if next == read {
                break; // full
            }
            // SAFETY: `write` is only advanced by this thread, and the consumer never reads
            // a slot until the store below is published by the release ordering.
            unsafe { *self.buf[write].get() = sample };
            write = next;
            written += 1;
        }

        self.write.store(write, Ordering::Release);
        written
    }

    /// Consumer side. Fills `out` completely, padding with silence when there isn't enough.
    pub fn pop(&self, out: &mut [f32]) {
        let write = self.write.load(Ordering::Acquire);
        let mut read = self.read.load(Ordering::Relaxed);
        let mut filled = 0;

        for slot in out.iter_mut() {
            if read == write {
                *slot = 0.0;
                filled += 1;
                continue;
            }
            // SAFETY: `read` only advances here, and this slot was published by the producer.
            *slot = unsafe { *self.buf[read].get() };
            read = (read + 1) % self.buf.len();
        }

        self.read.store(read, Ordering::Release);
        if filled > 0 {
            self.underruns.fetch_add(filled, Ordering::Relaxed);
        }
    }

    /// Throw away everything waiting. Used when playback restarts, where old audio would be
    /// heard as a burst of the past.
    pub fn clear(&self) {
        self.read.store(self.write.load(Ordering::Acquire), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn what_goes_in_comes_out() {
        let ring = Ring::new(16);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0; 3];
        ring.pop(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0]);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn an_empty_ring_plays_silence_rather_than_stalling() {
        let ring = Ring::new(16);
        let mut out = [9.0; 4];
        ring.pop(&mut out);
        assert_eq!(out, [0.0; 4]);
        assert_eq!(ring.underruns(), 4, "silence is counted, not hidden");
    }

    #[test]
    fn a_full_ring_drops_rather_than_blocking() {
        let ring = Ring::new(4);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0, 4.0]), 4);
        assert_eq!(ring.push(&[5.0]), 0, "nothing more fits");
        let mut out = [0.0; 4];
        ring.pop(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0], "the audio already queued is intact");
    }

    #[test]
    fn wraps_around_without_losing_anything() {
        let ring = Ring::new(4);
        let mut out = [0.0; 3];
        for round in 0..10 {
            let base = round as f32 * 3.0;
            assert_eq!(ring.push(&[base, base + 1.0, base + 2.0]), 3);
            ring.pop(&mut out);
            assert_eq!(out, [base, base + 1.0, base + 2.0], "round {round}");
        }
    }

    #[test]
    fn clearing_drops_the_backlog() {
        let ring = Ring::new(16);
        ring.push(&[1.0, 2.0, 3.0]);
        ring.clear();
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn survives_a_real_producer_and_consumer_running_at_once() {
        // The property: every sample the producer wrote comes out exactly once, in a run
        // where the two threads are genuinely racing. A torn read would show up as a value
        // that is neither the producer's nor silence.
        let ring = Ring::new(1024);
        let done = Arc::new(AtomicBool::new(false));

        let producer = {
            let ring = ring.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                let mut sent = 0usize;
                while sent < 100_000 {
                    sent += ring.push(&[1.0; 64]);
                    std::thread::yield_now();
                }
                done.store(true, Ordering::Release);
                sent
            })
        };

        // Drain until the producer has finished *and* the ring is empty — stopping on a
        // sample count instead would wedge the producer against a full ring, which is
        // exactly the deadlock this test had when it was first written.
        let mut out = [0.0; 32];
        let mut received = 0usize;
        while !done.load(Ordering::Acquire) || ring.len() > 0 {
            ring.pop(&mut out);
            assert!(out.iter().all(|&s| s == 1.0 || s == 0.0), "a sample was torn");
            received += out.iter().filter(|&&s| s == 1.0).count();
        }

        let sent = producer.join().expect("producer");
        assert_eq!(received, sent, "{sent} samples went in, {received} came out");
    }
}
