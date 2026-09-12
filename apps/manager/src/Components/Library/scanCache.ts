/**
 * The last scan of each mod folder, kept across mounts.
 *
 * Leaving the Library unmounts it, so every return walked the tree again — and that walk
 * measures each mod folder file by file to put a size on the card, which is minutes of
 * disk on a big collection and a spinner in front of a list that hadn't changed. Clicking
 * between the Library's own tabs did it too, once per tab.
 *
 * So a scan is remembered. A visit inside [`TTL_MS`] shows it and touches nothing; a later
 * one shows it *and* rescans behind it, replacing the list when the answer arrives. The app's
 * own installs and moves drop the cache outright, since those know the tree changed, and so
 * does the mods-folder watcher when something is dropped in by hand.
 *
 * `primeMetaCache` keeps archive metadata the same way, for the same reason.
 */

/** How long a scan is trusted before a visit refreshes it in the background. Long enough
 *  that flicking between tabs costs nothing, short enough that a mod dropped into the
 *  folder by hand turns up on the next look rather than the next launch. */
const TTL_MS = 30_000;

const scans = new Map<string, { value: unknown; at: number }>();

/** The remembered scan, and whether it is young enough to skip refreshing. */
export function cachedScan<T>(key: string): { value: T; fresh: boolean } | null {
  const hit = scans.get(key);
  if (!hit) return null;
  return { value: hit.value as T, fresh: Date.now() - hit.at < TTL_MS };
}

export function putScan<T>(key: string, value: T): void {
  scans.set(key, { value, at: Date.now() });
}

/** Forget everything: something changed the tree, so no scan speaks for it any more. */
export function dropScans(): void {
  scans.clear();
}
