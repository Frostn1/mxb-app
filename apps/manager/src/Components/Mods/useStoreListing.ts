import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { ModType } from "@frost/shared/api/mods";
import type { ShopCategory, ShopStatus } from "@frost/shared/types";
import {
  SHOP_CATALOG_UPDATED,
  shopCatalogCategories,
  shopCatalogRefresh,
  shopCatalogSearch,
  shopCatalogStatus,
} from "../../api/shop";
import { hubCategories, hubSearch } from "../../api/hub";
import {
  clearListings,
  listingKey,
  readListing,
  writeListing,
} from "../../lib/listingCache";
import {
  fromStoreMod,
  storeRootsFor,
  toHubSort,
  toShopSort,
  type MergedMod,
  type ModsSort,
  type StoreId,
} from "./sources";

interface StoreListingOpts {
  store: StoreId;
  /**
   * Nothing is fetched while this is false — and it is false whenever the source filter has
   * the store switched off. MXB Hub answers live over the network, so a catalogue nobody
   * asked to see must not be requested in the background.
   */
  enabled: boolean;
  /** The raw search box; debounced here, once, for both stores. */
  query: string;
  /** The left list's active type. Narrows the store to its matching top-level category. */
  modType: ModType;
  /** A sub-category the user picked underneath that type, or null for the whole type. */
  categoryId: number | null;
  sort: ModsSort;
}

export interface StoreListing {
  items: MergedMod[];
  categories: ShopCategory[];
  /** The store's own top-level category for the active type, when it has one. */
  root: ShopCategory | undefined;
  /** Every real store branch represented by the active app-level type. */
  roots: ShopCategory[];
  currency: string;
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  hasMore: boolean;
  loadMore: () => void;
  reload: () => void;
  /** mxbikes-shop only: how old its catalogue dump is, and a way to re-pull it. */
  status: ShopStatus | null;
  refresh: () => Promise<void>;
  refreshing: boolean;
}

const PAGE_ONE = 1;

/** What one store's filters answered last time, kept by `listingCache`. */
interface CachedPage {
  items: MergedMod[];
  hasMore: boolean;
  currency: string;
}

/**
 * One store's catalogue, in the merged grid's shape.
 *
 * `ShopCatalog` and `HubCatalog` were the same 250 lines twice — a 350 ms debounce, a page-1
 * effect keyed on the filters, append-on-load-more — with the fetch call and a staleness bar
 * as the only real differences. The Mods screen needs both at once for "All sources", so the
 * duplication is now this hook, called once per store.
 */
export function useStoreListing({
  store,
  enabled,
  query,
  modType,
  categoryId,
  sort,
}: StoreListingOpts): StoreListing {
  const [debounced, setDebounced] = useState(query.trim());
  const [categories, setCategories] = useState<ShopCategory[]>([]);
  // Whether the category fetch has settled — the item fetch waits for it, so the first page
  // is already narrowed to the active type rather than being pulled twice.
  const [catsSettled, setCatsSettled] = useState(false);
  const [items, setItems] = useState<MergedMod[]>([]);
  const [currency, setCurrency] = useState("USD");
  const [page, setPage] = useState(PAGE_ONE);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [status, setStatus] = useState<ShopStatus | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  // Set by `reload` and `refresh`, so an explicit ask goes to the store rather than
  // repainting what is already on screen.
  const skipCache = useRef(false);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 350);
    return () => clearTimeout(timer);
  }, [query]);

  // Categories, once the store is switched on. A failure here is quiet: the left list falls
  // back to the type's own categories and the grid still works, unnarrowed.
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    const catsKey = listingKey(["categories", store]);
    // A store's tree is the same tree every time it is switched on, and the item fetch waits
    // on it — so painting the last one is what turns a second visit into no wait at all.
    const cached = skipCache.current ? undefined : readListing<ShopCategory[]>(catsKey);
    if (cached) {
      setCategories(cached.value);
      setCatsSettled(true);
      if (cached.fresh) return;
    }
    const load = store === "shop" ? shopCatalogCategories : hubCategories;
    load()
      .then((res) => {
        writeListing(catsKey, res);
        if (!cancelled) setCategories(res);
      })
      .catch(() => {})
      .finally(() => !cancelled && setCatsSettled(true));
    return () => {
      cancelled = true;
    };
  }, [store, enabled, reloadKey]);

  const roots = useMemo(
    () => storeRootsFor(store, modType, categories),
    [store, modType, categories],
  );
  const root = roots[0];
  const effectiveCategory = categoryId ?? root?.id ?? null;
  const hubCategoryIds = categoryId === null ? roots.map((category) => category.id) : [categoryId];
  const unsupported = store === "hub" && categories.length > 0 && roots.length === 0;
  const categoryKey = store === "hub" ? hubCategoryIds.join(",") : effectiveCategory;

  const fetchPage = useCallback(
    (next: number) =>
      store === "shop"
        ? shopCatalogSearch(debounced, effectiveCategory, next, toShopSort(sort), false)
        : hubSearch(debounced, hubCategoryIds, next, toHubSort(sort), false),
    [store, debounced, effectiveCategory, hubCategoryIds.join(","), sort],
  );

  // (Re)load page 1 whenever a filter changes — from the last answer first, so coming back
  // to a tab shows its mods rather than a block of skeletons.
  useEffect(() => {
    if (!enabled || !catsSettled) return;
    if (unsupported) {
      setItems([]);
      setHasMore(false);
      setLoading(false);
      return;
    }
    let cancelled = false;
    const key = listingKey([store, debounced, categoryKey, sort]);
    const cached = skipCache.current ? undefined : readListing<CachedPage>(key);
    skipCache.current = false;
    setError(null);
    setPage(PAGE_ONE);

    if (cached) {
      setItems(cached.value.items);
      setHasMore(cached.value.hasMore);
      setCurrency(cached.value.currency);
      setLoading(false);
      // Asked a moment ago; MXB Hub in particular answers a burst of requests by deciding
      // we are a robot, so the one we can skip is worth skipping.
      if (cached.fresh) return;
    } else {
      setLoading(true);
    }

    fetchPage(PAGE_ONE)
      .then((res) => {
        const items = res.items.map((m) => fromStoreMod(store, m));
        writeListing<CachedPage>(key, { items, hasMore: res.hasMore, currency: res.currency });
        if (cancelled) return;
        setItems(items);
        setHasMore(res.hasMore);
        setCurrency(res.currency);
      })
      // A refresh that fails behind a grid that is already up leaves it up.
      .catch((e) => !cancelled && !cached && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [
    enabled,
    catsSettled,
    unsupported,
    fetchPage,
    store,
    debounced,
    categoryKey,
    sort,
    reloadKey,
  ]);

  // The shop's catalogue is a dump the backend refreshes in the background; the hub's is
  // live, so it has neither a staleness bar nor anything to say here.
  useEffect(() => {
    if (store !== "shop" || !enabled) return;
    let cancelled = false;
    shopCatalogStatus()
      .then((s) => !cancelled && setStatus(s))
      .catch(() => {});
    const pending = listen<ShopStatus>(SHOP_CATALOG_UPDATED, (event) => {
      setStatus(event.payload);
      // A fresh dump landed, so what we were holding is the previous catalogue.
      clearListings();
      skipCache.current = true;
      setReloadKey((k) => k + 1);
    });
    return () => {
      cancelled = true;
      void pending.then((unlisten) => unlisten());
    };
  }, [store, enabled, reloadKey]);

  const loadMore = useCallback(() => {
    const next = page + 1;
    setLoadingMore(true);
    void fetchPage(next)
      .then((res) => {
        setItems((prev) => [...prev, ...res.items.map((m) => fromStoreMod(store, m))]);
        setHasMore(res.hasMore);
        setPage(next);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoadingMore(false));
  }, [fetchPage, page, store]);

  const reload = useCallback(() => {
    // Nothing stored is worth painting once the player has asked for it again.
    clearListings();
    skipCache.current = true;
    setReloadKey((k) => k + 1);
  }, []);

  const refresh = useCallback(async () => {
    if (store !== "shop") return;
    setRefreshing(true);
    try {
      setStatus(await shopCatalogRefresh());
      clearListings();
      skipCache.current = true;
      setReloadKey((k) => k + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, [store]);

  return {
    items: enabled ? items : [],
    categories,
    root,
    roots,
    currency,
    loading: enabled && (loading || !catsSettled),
    loadingMore: enabled && loadingMore,
    error: enabled ? error : null,
    hasMore: enabled && hasMore,
    loadMore,
    reload,
    status,
    refresh,
    refreshing,
  };
}
