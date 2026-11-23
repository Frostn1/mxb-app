/** The part of the debrief that is decided by data rather than by rendering, kept apart so it
 *  can be tested without a window. */

export interface PlanLap {
  path: string;
  num: number;
  timeMs: number;
  whole: boolean;
  invalid: boolean;
}

export type Missing = "noLaps" | "noWholeLap";

/** The laps a full debrief can be read from, and the fastest of them. With none, `missing`
 *  says why, so the screen can show every lap anyway instead of waiting for a read that will
 *  never come. */
export function planDebrief<L extends PlanLap>(laps: L[]): { laps: L[]; best: L | null; missing: Missing | null } {
  const valid = laps.filter((l) => l.whole && !l.invalid && l.timeMs > 0);
  const best = [...valid].sort((a, b) => a.timeMs - b.timeMs)[0] ?? null;
  return { laps: valid, best, missing: best ? null : laps.length === 0 ? "noLaps" : "noWholeLap" };
}

/** Reject if the promise has not settled in `ms`: a read that never answers is an error the
 *  rider can see, not an endless "Loading…". */
export function withTimeout<T>(p: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), ms);
    p.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}
