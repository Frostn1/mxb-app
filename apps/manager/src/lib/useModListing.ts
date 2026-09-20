import { useCallback, useEffect, useRef, useState } from "react";
import {
  MOD_SORTS,
  SEARCH_PAGE_SIZE,
  getModRatings,
  searchMods,
  type ModSort,
  type ModType,
} from "@frost/shared/api/mods";
import type { ModRating, ModSummary } from "@frost/shared/types";
import { clearListings, listingKey, readListing, writeListing } from "./listingCache";

/** What one set of filters answered last time, kept by `listingCache`. */
interface CachedPage {
  mods: ModSummary[];
  hasMore: boolean;
}

/**
 * What the browse grid is showing: the filters, the pages fetched under them, and where
 * the user was scrolled to.
 *
 * It lives here rather than inside `Browse` because `Browse` is unmounted whenever a mod
 * detail page opens — it and `ModDetail` are the two arms of one ternary in both the
 * Dashboard and the overlay. State held in the component would be thrown away on the way
 * in and refetched from page 1 on the way back, losing the search, the category, the sort,
 * every "load more" page and the scroll offset. Held here, by a hook that `useModBrowsing`
 * calls *above* that swap, nothing unmounts: coming back re-renders the grid that is
 * already in memory, with no refetch at all.
 */
export function useModListing(modType: ModType) {
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [categoryId, setCategoryId] = useState(modType.categoryId);
  const [sort, setSort] = useState<ModSort>("newest");
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [ratings, setRatings] = useState<Map<number, ModRating>>(new Map());
  // Ids we've already asked about, so a mod the site had no answer for isn't re-requested
  // on every render. Kept across a listing switch rather than cleared with it: the same mod
  // turns up under several of them, and its score is already on screen.
  const askedForRatings = useRef<Set<number>>(new Set());
  // Set by `reload`, so the retry button goes to the catalog rather than repainting the
  // cached answer the player just told us was wrong.
  const skipCache = useRef(false);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [selected, setSelected] = useState<Map<string, ModSummary>>(new Map());
  // Where the grid was scrolled to, for `Browse` to put back on its next mount. A ref
  // rather than state: it changes on every scroll frame and nothing renders from it.
  const scrollTop = useRef(0);

  // The popular listings are ranked by views by a part of the site that takes no search
  // term, so they step aside while the box has text in it and the order falls back to
  // newest — the control always names the order actually on screen. The pick itself is
  // kept, so clearing the search puts it back.
  const sortOptions = MOD_SORTS.filter((s) => !s.noSearch || !debounced);
  const activeSort = sortOptions.some((s) => s.value === sort) ? sort : "newest";

  // The filters that decide what the first page holds. A switch between two of these is
  // what the cache is for: the grid is painted from the last answer before the request that
  // refreshes it is even sent.
  const key = listingKey([modType.id, debounced, categoryId, activeSort]);

  // Reset the category filter (and any selection) when the mod type changes —
  // selection + quick-install resolve against the current type's folders.
  useEffect(() => {
    setCategoryId(modType.categoryId);
    setSelected(new Map());
    scrollTop.current = 0;
  }, [modType]);

  const toggleSelect = useCallback((mod: ModSummary) => {
    setSelected((prev) => {
      const next = new Map(prev);
      if (next.has(mod.slug)) next.delete(mod.slug);
      else next.set(mod.slug, mod);
      return next;
    });
  }, []);

  const clearSelection = useCallback(() => setSelected(new Map()), []);

  const selectAll = useCallback(() => {
    setSelected((prev) => {
      const next = new Map(prev);
      for (const m of mods) next.set(m.slug, m);
      return next;
    });
  }, [mods]);

  // Debounce the search input so we don't hammer the API on every keystroke.
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 350);
    return () => clearTimeout(timer);
  }, [query]);

  // (Re)load the first page whenever the query, category or sort changes — from the last
  // answer first, where there is one, so the grid is never replaced by skeletons for a
  // listing we already have.
  useEffect(() => {
    let cancelled = false;
    const cached = skipCache.current ? undefined : readListing<CachedPage>(key);
    skipCache.current = false;
    setError(null);
    setPage(1);
    // A different listing starts at the top; the offset we were holding belongs to the
    // old one.
    scrollTop.current = 0;

    if (cached) {
      setMods(cached.value.mods);
      setHasMore(cached.value.hasMore);
      setLoading(false);
      // Asked a moment ago, so asking again would only cost the catalog a request to be
      // told the same thing.
      if (cached.fresh) return;
    } else {
      setLoading(true);
    }

    searchMods(debounced, categoryId, 1, activeSort)
      .then((res) => {
        writeListing<CachedPage>(key, { mods: res, hasMore: res.length >= SEARCH_PAGE_SIZE });
        if (cancelled) return;
        setMods(res);
        setHasMore(res.length >= SEARCH_PAGE_SIZE);
      })
      // A refresh that fails behind a grid that is already up leaves it up: the player is
      // looking at real mods, and replacing them with an error would be a worse answer than
      // the slightly older one on screen.
      .catch((e) => !cancelled && !cached && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [key, debounced, categoryId, activeSort, reloadKey]);

  // Scores aren't part of the search response — they come in a second pass keyed by post
  // id, for whatever is on screen. Never awaited by the grid: cards paint immediately and
  // stars appear a moment later, and a failure just leaves them off.
  useEffect(() => {
    const wanted = mods.map((m) => m.id).filter((id) => !askedForRatings.current.has(id));
    if (wanted.length === 0) return;
    for (const id of wanted) askedForRatings.current.add(id);
    let cancelled = false;
    getModRatings(wanted)
      .then((res) => {
        if (cancelled) return;
        setRatings((prev) => {
          const next = new Map(prev);
          for (const [id, rating] of Object.entries(res)) next.set(Number(id), rating);
          return next;
        });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [mods]);

  const loadMore = useCallback(async () => {
    const next = page + 1;
    setLoadingMore(true);
    try {
      const res = await searchMods(debounced, categoryId, next, activeSort);
      setMods((prev) => [...prev, ...res]);
      setHasMore(res.length >= SEARCH_PAGE_SIZE);
      setPage(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  }, [debounced, categoryId, activeSort, page]);

  const reload = useCallback(() => {
    // Nothing stored is worth painting once the player has asked for it again.
    clearListings();
    skipCache.current = true;
    askedForRatings.current = new Set();
    setReloadKey((k) => k + 1);
  }, []);

  return {
    query,
    setQuery,
    categoryId,
    setCategoryId,
    setSort,
    /** The orders offered right now: the popular ones drop out during a search. */
    sortOptions,
    /** The order on screen, which is `newest` when the pick isn't currently offered. */
    activeSort,
    mods,
    ratings,
    hasMore,
    loading,
    loadingMore,
    error,
    reload,
    loadMore,
    selected,
    toggleSelect,
    clearSelection,
    selectAll,
    scrollTop,
  };
}

export type ModListing = ReturnType<typeof useModListing>;
