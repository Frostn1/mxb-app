import type { RefArgs } from "@/api/coach";

/**
 * What a lap is compared with, and remembering it per track.
 *
 * One string carries the choice everywhere — the review page, the live cues and the in-game
 * HUD all read the same one, so the gap on the HUD is against the lap the rider picked:
 *
 *   `best`              the fastest lap of theirs on this track
 *   `ideal`             their own best sections added up, which was never ridden whole
 *   `alone`             nothing: the lap on its own
 *   `<recording>::<n>`  one particular lap, theirs or an imported one
 */
export type Reference = string;

export const BEST: Reference = "best";
export const IDEAL: Reference = "ideal";
export const ALONE: Reference = "alone";

const key = (trackId: string) => `coach-reference-${trackId}`;

/** The reference the rider last picked on this track, so they don't pick it again every time. */
export function rememberedRef(trackId: string, fallback: Reference = BEST): Reference {
  try {
    return localStorage.getItem(key(trackId)) || fallback;
  } catch {
    return fallback;
  }
}

export function rememberRef(trackId: string, choice: Reference) {
  try {
    localStorage.setItem(key(trackId), choice);
  } catch {
    /* no storage: the choice lasts as long as the app is open */
  }
}

/** One lap of one recording, as the picker writes it. */
export const lapRef = (path: string, lap: number): Reference => `${path}::${lap}`;

/** The choice as the backend takes it. */
export function refArgs(choice: Reference): RefArgs {
  if (choice === ALONE) return { solo: true };
  if (choice === IDEAL) return { ideal: true };
  const cut = choice.lastIndexOf("::");
  if (cut > 0) return { refPath: choice.slice(0, cut), refLap: Number(choice.slice(cut + 2)) };
  return {};
}
