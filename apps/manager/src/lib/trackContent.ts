/**
 * Deciding whether a server's track is already on disk.
 *
 * A server reports its track by internal id — the top folder inside the track's `.pkz`, e.g.
 * `2026_ARLSX_RD14` or `walnut` — and {@link installedTrackIds} returns exactly those ids for
 * everything installed. So the match is, at heart, an equality check; the only wrinkle is that
 * authors are inconsistent with case, spaces, dashes and underscores between the packaged folder
 * and how a host typed it, so we compare on a normalized form as well.
 */
import type { InstalledTrack } from "@frost/shared/api/mods";

/** Fold to a case/separator-insensitive key: lowercase, alphanumeric only. `"ZD - Meadow
 *  Valley MX"` and `"zd_meadowvalley_mx"` collapse together; `"2026_ARLSX_RD14"` keeps its
 *  digits and letters. */
function norm(id: string): string {
  return id.toLowerCase().replace(/[^a-z0-9]/g, "");
}

export interface TrackIndex {
  /** normalized id → the installed track (for reading its preview, etc.). */
  byId: Map<string, InstalledTrack>;
}

export function buildTrackIndex(installed: InstalledTrack[]): TrackIndex {
  const byId = new Map<string, InstalledTrack>();
  for (const t of installed) {
    const key = norm(t.id);
    if (key && !byId.has(key)) byId.set(key, t);
  }
  return { byId };
}

/** Whether a server's track content is present. `unknown` is for servers that name no track
 *  (practice/free-roam lobbies) — there is nothing to download and nothing missing. */
export type TrackContentState = "installed" | "missing" | "unknown";

export interface TrackMatch {
  state: TrackContentState;
  /** The installed track, when `state === "installed"`. */
  installed?: InstalledTrack;
}

export function matchTrack(index: TrackIndex, serverTrack: string): TrackMatch {
  const raw = serverTrack.trim();
  if (!raw) return { state: "unknown" };
  const hit = index.byId.get(norm(raw));
  return hit ? { state: "installed", installed: hit } : { state: "missing" };
}
