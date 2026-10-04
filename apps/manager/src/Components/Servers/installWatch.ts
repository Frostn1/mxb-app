/**
 * Following one Online-tab track install to its end, so a server's button can never be left
 * on a spinner that nothing will clear.
 *
 * The tab used to learn an install had ended only from a finished card in the install panel.
 * A job that vanished instead — cancelled, collapsed onto a duplicate, or retired before the
 * tab saw it — left the tile on "Installing" for good, and a transfer that simply stopped
 * moving did the same. Both now end here: a job seen running that is gone ends as `gone`,
 * and one whose stage and byte count have not moved for `stallMs` ends as `timeout`.
 */

/** The panel's view of a job, as much of it as the watch reads. */
export interface WatchedJob {
  stage: string;
  received?: number;
}

export interface InstallWatch {
  /** The job has been seen in a running stage, so a finished card is this job's and not a
   *  leftover from an earlier install of the same track. */
  seen: boolean;
  /** When the job's stage or byte count last moved — or when the watch began. */
  lastChange: number;
  /** `stage:received` at `lastChange`. */
  signature: string;
  /** The card already in the panel for this slug when the watch began — an earlier attempt's
   *  finished card, which says nothing about this one. A card is replaced, never mutated, so
   *  any other object is this attempt's. */
  stale?: WatchedJob;
}

export type WatchVerdict = "running" | "done" | "failed" | "gone" | "timeout";

/** No movement for this long ends the wait: the tile offers Join anyway, Retry and Pick. */
export const INSTALL_STALL_MS = 90_000;
/** Looking up a mod's download link is one page fetch; it gets a shorter leash. */
export const RESOLVE_TIMEOUT_MS = 30_000;

/** How long a job may be neither queued nor on a card before it counts as dropped — the
 *  queue's own state lands a render after the job is handed to it. */
export const GONE_GRACE_MS = 5_000;

const FINISHED = new Set(["done", "error", "review"]);
const LOCAL = new Set(["extracting", "placing"]);

export function startWatch(now: number, stale?: WatchedJob): InstallWatch {
  return { seen: false, lastChange: now, signature: "", stale };
}

/**
 * One step of the watch.
 *
 * `job` is the panel's card for this track's slug, if there is one; `queued` says the job is
 * still waiting its turn (or still resolving its link) in the install queue.
 */
export function stepWatch(
  w: InstallWatch,
  job: WatchedJob | undefined,
  queued: boolean,
  now: number,
  stallMs = INSTALL_STALL_MS,
): { watch: InstallWatch; verdict: WatchVerdict } {
  if (job && FINISHED.has(job.stage)) {
    if (w.seen || job !== w.stale) {
      // A pack goes to review: not an install the server can be joined on, and not a fault.
      const verdict = job.stage === "done" ? "done" : job.stage === "error" ? "failed" : "gone";
      return { watch: w, verdict };
    }
    // A finished card from an earlier attempt. Ours hasn't started; the stall clock still runs.
  }

  let next = w;
  if (job && LOCAL.has(job.stage)) {
    // The bytes are down; unpacking and placing are local work that sends no progress and
    // can't be interrupted safely. Not a stall, so never timed out.
    return {
      watch: { ...w, seen: true, lastChange: now, signature: job.stage },
      verdict: "running",
    };
  }
  if (job && !FINISHED.has(job.stage)) {
    const signature = `${job.stage}:${job.received ?? ""}`;
    next =
      signature !== w.signature || !w.seen
        ? { ...w, seen: true, lastChange: now, signature }
        : w;
  } else if (!job && !queued && (w.seen || now - w.lastChange >= GONE_GRACE_MS)) {
    // Ran, then left the panel without finishing: cancelled, or folded into another job. Or
    // never showed up at all — dropped from the queue before it started.
    return { watch: w, verdict: "gone" };
  }

  if (now - next.lastChange >= stallMs) return { watch: next, verdict: "timeout" };
  return { watch: next, verdict: "running" };
}

/** Rejects after `ms` with `"timeout"`, so a fetch that never answers can't hold a lane. */
export function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject("timeout"), ms);
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
