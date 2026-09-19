import { useSyncExternalStore } from "react";
import type { TrackGuess } from "@frost/shared/api/mods";

/**
 * What each track id turned out to be, for the life of the app.
 *
 * Identifying a track costs up to four catalogue lookups, and the same track is on a dozen
 * servers and comes round again on every rotation — so one answer per track per run.
 *
 * It lives here rather than inside the detail pane because the list wants it too: the pane
 * was the only thing that knew Fort Red has a picture on mxb-mods, so the row beside it sat
 * empty while the hero above it was fine. Anything that learns an answer puts it here, and
 * everything that draws a track reads from here.
 */
const GUESSES = new Map<string, TrackGuess>();
const listeners = new Set<() => void>();
let version = 0;

export const guessFor = (track: string): TrackGuess | undefined => GUESSES.get(track);

export function rememberGuess(track: string, guess: TrackGuess) {
  GUESSES.set(track, guess);
  version += 1;
  for (const l of listeners) l();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** The picture a guess carries, best first: the installed track's own, then the shop's. */
export const guessPicture = (track: string): string => {
  const g = GUESSES.get(track);
  return g ? g.preview || g.productImage || "" : "";
};

/** Re-renders whatever draws a track when any guess lands. */
export function useTrackGuesses(): number {
  return useSyncExternalStore(
    subscribe,
    () => version,
    () => version,
  );
}
