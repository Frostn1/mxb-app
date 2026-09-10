import { useEffect, useMemo, useRef, useState } from "react";
import {
  onTrackIndexUpdated,
  resolveServerTracks,
  type MasterServer,
  type TrackHit,
} from "@frost/shared/api/mods";
import { matchTrack, type TrackIndex } from "./trackContent";

/**
 * Which of the browser's missing tracks can actually be downloaded - worked out up front,
 * for every server on screen at once.
 *
 * The old flow could only answer this *after* a click: each "get this track" button ran its
 * own three-catalog search, so the browser had no way to distinguish a track one click away
 * from one nobody has published. This asks the backend's index instead ({@link
 * resolveServerTracks}), in a single call covering every missing track in the list, so the
 * card can say "Install" or stay quiet before the user commits to anything.
 *
 * Two things it is careful about:
 *
 * * **Order is priority.** Ids go out ranked by how many servers are running them, because
 *   whatever the index can't answer becomes its background queue - so the tracks the most
 *   people are looking at are the ones it reads pages for first.
 * * **It re-asks, but only when there is a reason to.** The queue fills in behind us, so
 *   `track-index://updated` triggers another resolve; the set of ids is otherwise compared by
 *   value, so a refreshed server list with the same tracks (which is most refreshes) doesn't.
 */
export interface TrackCatalog {
  /** Track id → the catalog post that has it. */
  found: Map<string, TrackHit>;
  /** Track ids the backend is still reading candidate pages for. */
  pending: Set<string>;
}

const EMPTY: TrackCatalog = { found: new Map(), pending: new Set() };

export function useTrackCatalog(
  servers: MasterServer[] | null,
  trackIndex: TrackIndex,
): TrackCatalog {
  const [catalog, setCatalog] = useState<TrackCatalog>(EMPTY);
  // Bumped by `track-index://updated` to re-run the resolve with the same ids.
  const [round, setRound] = useState(0);

  // The missing tracks, most-hosted first. Joined into a string so the effect below compares
  // by value: a refresh that returns the same tracks with different player counts is the
  // common case, and re-resolving on every one of those would be pure noise.
  const wantedKey = useMemo(() => {
    const counts = new Map<string, number>();
    for (const s of servers ?? []) {
      const id = s.track.trim();
      if (!id) continue;
      if (matchTrack(trackIndex, id).state !== "missing") continue;
      counts.set(id, (counts.get(id) ?? 0) + 1);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([id]) => id)
      .join("\n");
  }, [servers, trackIndex]);

  // A token so a resolve started before a re-render can't apply over a newer one.
  const runId = useRef(0);

  useEffect(() => {
    if (!wantedKey) {
      setCatalog(EMPTY);
      return;
    }
    const id = ++runId.current;
    resolveServerTracks(wantedKey.split("\n"))
      .then((res) => {
        if (runId.current !== id) return;
        setCatalog({
          found: new Map(Object.entries(res.found)),
          pending: new Set(res.pending),
        });
      })
      // A failed resolve leaves every track looking un-gettable, which is the same thing the
      // browser showed before this existed - the per-track search is still there behind the
      // button. Not worth a toast.
      .catch(() => {
        if (runId.current === id) setCatalog(EMPTY);
      });
  }, [wantedKey, round]);

  useEffect(() => {
    const un = onTrackIndexUpdated(() => setRound((r) => r + 1));
    return () => {
      void un.then((off) => off()).catch(() => {});
    };
  }, []);

  return catalog;
}
