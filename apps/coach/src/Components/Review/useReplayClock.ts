import { useCallback, useEffect, useRef, useState } from "react";

/**
 * A clock over a recorded run, in the run's own seconds.
 *
 * Playback is real time against `performance.now()` rather than a frame counter, so a corner
 * takes as long to watch as it took to ride and a dropped frame costs no time. The recording is
 * 50 Hz and the screen is not, so the two were never going to line up anyway.
 */
export type Clock = {
  /** Where the playhead is, seconds from the start of the run. */
  at: number;
  playing: boolean;
  play: () => void;
  pause: () => void;
  toggle: () => void;
  /** Put the playhead somewhere, in seconds. Stops playback: a hand on the scrub means look. */
  seek: (t: number) => void;
};

/** How much slower than life to play. A corner at speed is over before it can be read. */
const RATE = 0.55;

export function useReplayClock(length: number, autoplay = true): Clock {
  const [at, setAt] = useState(0);
  const [playing, setPlaying] = useState(autoplay && length > 0);
  const frame = useRef<number | null>(null);
  // The playhead the rAF loop reads. State alone would make every frame depend on the last
  // render having landed, and the loop would crawl on a busy step.
  const head = useRef(0);

  const stop = useCallback(() => {
    if (frame.current != null) cancelAnimationFrame(frame.current);
    frame.current = null;
  }, []);

  const seek = useCallback(
    (t: number) => {
      const to = Math.max(0, Math.min(length, t));
      head.current = to;
      setAt(to);
      setPlaying(false);
      stop();
    },
    [length, stop],
  );

  // Restarting from the end is what a rider means by play on a run they have just watched.
  const play = useCallback(() => {
    if (head.current >= length - 0.01) {
      head.current = 0;
      setAt(0);
    }
    setPlaying(true);
  }, [length]);

  const pause = useCallback(() => setPlaying(false), []);
  const toggle = useCallback(() => (playing ? pause() : play()), [playing, pause, play]);

  useEffect(() => {
    if (!playing || length <= 0) return;
    let last = performance.now();
    const step = (now: number) => {
      const on = head.current + ((now - last) / 1000) * RATE;
      last = now;
      if (on >= length) {
        head.current = length;
        setAt(length);
        setPlaying(false);
        frame.current = null;
        return;
      }
      head.current = on;
      setAt(on);
      frame.current = requestAnimationFrame(step);
    };
    frame.current = requestAnimationFrame(step);
    return stop;
  }, [playing, length, stop]);

  // A new run is a new clock. Without this, stepping to the next corner would drop the playhead
  // in the middle of a run it does not belong to.
  useEffect(() => {
    head.current = 0;
    setAt(0);
    setPlaying(autoplay && length > 0);
  }, [length, autoplay]);

  return { at, playing, play, pause, toggle, seek };
}

/**
 * The frame at a moment, and how far past it we are.
 *
 * Returns the index below the time and the 0-to-1 share to the next one, so a caller can read
 * between two samples instead of stepping. At 50 Hz on a 120 Hz screen, stepping shows the same
 * sample twice and the movement stutters at exactly the speeds worth watching.
 */
export function frameAt(times: number[], at: number): { i: number; mix: number } {
  if (times.length === 0) return { i: 0, mix: 0 };
  // Binary search: a corner is a few hundred samples and this runs every frame.
  let [lo, hi] = [0, times.length - 1];
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (times[mid] <= at) lo = mid;
    else hi = mid - 1;
  }
  const next = times[lo + 1];
  if (next == null) return { i: lo, mix: 0 };
  const span = next - times[lo];
  return { i: lo, mix: span > 0 ? Math.max(0, Math.min(1, (at - times[lo]) / span)) : 0 };
}

/** Read a channel between two samples. */
export function lerp(a: number, b: number, mix: number): number {
  return a + (b - a) * mix;
}
