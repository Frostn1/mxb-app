import { useSyncExternalStore } from "react";
import type { TrackGuess } from "@frost/shared/api/mods";
import { BoundedCache } from "@/lib/boundedCache";

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
const GUESSES = new BoundedCache<TrackGuess>(512, 32 * 1024 * 1024);
const listeners = new Set<() => void>();
let version = 0;

export const guessFor = (track: string): TrackGuess | undefined => GUESSES.get(track);

export function rememberGuess(track: string, guess: TrackGuess) {
  const bytes =
    2 *
    (track.length +
      guess.id.length +
      guess.installed.length +
      guess.source.length +
      guess.productName.length +
      guess.productUrl.length +
      guess.preview.length +
      guess.productImage.length);
  GUESSES.set(track, track, guess, bytes);
  version += 1;
  for (const l of listeners) l();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/**
 * The picture a guess carries.
 *
 * A track the player has shows its own artwork or nothing at all. A catalogue photo is only
 * ever offered for a track they don't have, because the match behind it is a fold of one name
 * onto another — and a stranger's mod wearing the same name is worse than a blank tile.
 */
export const guessPicture = (track: string): string => {
  const g = GUESSES.get(track);
  if (!g) return "";
  return g.installed ? g.preview : g.preview || g.productImage || "";
};

/** Re-renders whatever draws a track when any guess lands. */
export function useTrackGuesses(): number {
  return useSyncExternalStore(
    subscribe,
    () => version,
    () => version,
  );
}

/**
 * Identify the tracks on a list of servers, a couple at a time.
 *
 * Nobody should have to click a server to find out what it is running. The list arrives with
 * ids and nothing else, so this walks it once and fills in the name, the page and the picture
 * behind the player.
 *
 * Kept cheap on purpose. A track already answered in this run is skipped outright, and the
 * backend answers everything else from its own book on disk — so the only thing that ever
 * reaches mxb-mods or a shop is a track this install has genuinely never seen, once, ever.
 * Two at a time so a sixty-server sweep doesn't arrive as sixty simultaneous requests.
 */
const LANES = 2;

export interface TrackToIdentify {
  id: string;
  /** The server name often carries the pack title the internal track id omits. */
  hint?: string;
}

export async function warmTracks(
  tracks: TrackToIdentify[],
  identify: (track: string, hint?: string) => Promise<TrackGuess>,
  keepGoing: () => boolean,
) {
  // One id can be on several servers. Keep its first useful title hint while still resolving
  // it only once; a rotation's friendly server name is the only place some Shop packs publish
  // the product title.
  const byId = new Map<string, string | undefined>();
  for (const { id, hint } of tracks) {
    if (id && !byId.has(id)) byId.set(id, hint);
  }
  const queue = [...byId.keys()].filter((t) => !GUESSES.has(t) && !PENDING.has(t));
  // Claimed up front: a second sweep starting mid-walk must not ask for the same track again.
  for (const t of queue) PENDING.add(t);
  const lane = async () => {
    for (let t = queue.shift(); t; t = queue.shift()) {
      if (!keepGoing()) {
        PENDING.delete(t);
        continue;
      }
      try {
        rememberGuess(t, await identify(t, byId.get(t)));
      } catch {
        // A track we couldn't identify is one without a picture, not a broken list.
      } finally {
        PENDING.delete(t);
      }
    }
  };
  await Promise.all(Array.from({ length: LANES }, lane));
}

/** Tracks a walk has claimed but not yet answered. */
const PENDING = new Set<string>();
