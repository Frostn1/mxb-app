import type { ModSummary } from "@frost/shared/types";

/**
 * Append one page of a listing to what is on screen, by post id.
 *
 * A ranked listing (popular) can shift between two page requests, so page 2 may repeat a
 * mod page 1 already showed. Two cards with one id are two React children with one key:
 * React drops one, the grid stops growing, and `useFillViewport` keeps asking for "more" to
 * fill a viewport that never fills, which is a request loop. So repeats are dropped here, and
 * a page that added nothing new ends the paging.
 */
export function appendPage(
  prev: ModSummary[],
  page: ModSummary[],
  pageSize: number,
): { mods: ModSummary[]; hasMore: boolean } {
  const seen = new Set(prev.map((m) => m.id));
  const fresh = page.filter((m) => {
    if (seen.has(m.id)) return false;
    seen.add(m.id);
    return true;
  });
  return {
    mods: fresh.length ? [...prev, ...fresh] : prev,
    hasMore: fresh.length > 0 && page.length >= pageSize,
  };
}
