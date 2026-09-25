import { useEffect, useRef } from "react";
import { coachSessions, coachWriteCues, onSessionsChanged } from "@/api/coach";
import { AMOUNTS, CUE_AMOUNT_KEY, CUE_LEVEL_KEY, LEVELS, remembered } from "@/lib/cues";

/** The least time between two automatic re-picks: a re-pick is a full review of a lap, and the
 *  recorder's watcher fires whenever the recording grows, not once a lap. */
const GAP_MS = 45_000;

/**
 * Keeps the cue sheet current while Coach is open, wherever the rider happens to be in it.
 *
 * This used to live inside the Live cues panel, which meant it only ran while that panel was on
 * screen — the "In game" tab of an open lap, or the overlay's cues tab. Riding with Coach on any
 * other tab, or minimised, re-picked the calls exactly zero times, so the recorder loaded the
 * same sheet session after session and the rider heard the same cues forever. That is the whole
 * complaint about the cues never changing, and no amount of work in the picker could reach it.
 *
 * Mounted once at the root, it follows the newest lap on whatever the rider is riding now.
 */
export default function CueKeeper() {
  const last = useRef(0);
  const busy = useRef(false);
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    // A change inside the gap is put off until the gap ends rather than dropped. Dropped, a lap
    // finished within 45 s of the last pick waited for whatever the recorder wrote next, and
    // the first sheet of a new track (the one that starts the in-game coaching) landed a lap late.
    let later: ReturnType<typeof setTimeout> | undefined;
    const run = async () => {
      const now = Date.now();
      const wait = GAP_MS - (now - last.current);
      if (busy.current || wait > 0) {
        if (later === undefined && alive) {
          later = setTimeout(() => {
            later = undefined;
            void run();
          }, Math.max(wait, 1_000));
        }
        return;
      }
      last.current = now;
      busy.current = true;
      try {
        // The newest session is the one being ridden. `latest` then coaches its newest lap
        // rather than whichever one an open review happens to be showing.
        const sessions = await coachSessions();
        const newest = sessions[0];
        if (!newest) return;
        const level = remembered(CUE_LEVEL_KEY, LEVELS, "intermediate");
        const amount = remembered(CUE_AMOUNT_KEY, AMOUNTS, "normal");
        await coachWriteCues(newest.path, 1, level, amount, {}, true);
      } catch {
        // Nothing to say: a session with no whole lap in it yet is the normal case out on
        // track, and a toast for it would fire every time the rider left the gate.
      } finally {
        busy.current = false;
      }
    };
    void onSessionsChanged(() => void run()).then((stop) => {
      if (alive) off = stop;
      else stop();
    });
    return () => {
      alive = false;
      if (later !== undefined) clearTimeout(later);
      off?.();
    };
  }, []);
  return null;
}
