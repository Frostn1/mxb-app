import { useEffect, type RefObject } from "react";

/**
 * Whether a grid that has not filled its scroller should ask for another page.
 *
 * A page is 24 cards. On a maximised or high-resolution window that is two or three rows in
 * a viewport that holds six, so the scroller has nothing to scroll and the button under the
 * grid is the only thing left to press. Anything that is loading, failed, or out of pages
 * stands down, so a failing page can never turn this into a retry loop.
 */
export function needsMoreToFill(opts: {
  scrollHeight: number;
  clientHeight: number;
  hasMore: boolean;
  busy: boolean;
  blocked: boolean;
}): boolean {
  if (!opts.hasMore || opts.busy || opts.blocked) return false;
  if (opts.clientHeight <= 0) return false;
  // Within one card row of the bottom counts as unfilled: the next page is wanted before the
  // user reaches the end, not after.
  return opts.scrollHeight <= opts.clientHeight + 1;
}

/**
 * Keeps loading pages until the scroller overflows, and again whenever the window is resized
 * (entering fullscreen is a resize).
 */
export function useFillViewport(
  scroller: RefObject<HTMLElement | null>,
  opts: {
    /** Changes whenever the grid gains or loses cards. */
    itemCount: number;
    hasMore: boolean;
    busy: boolean;
    blocked: boolean;
    loadMore: () => void;
  },
) {
  const { itemCount, hasMore, busy, blocked, loadMore } = opts;
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const check = () => {
      if (
        needsMoreToFill({
          scrollHeight: el.scrollHeight,
          clientHeight: el.clientHeight,
          hasMore,
          busy,
          blocked,
        })
      ) {
        loadMore();
      }
    };
    check();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  }, [scroller, itemCount, hasMore, busy, blocked, loadMore]);
}
