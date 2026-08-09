import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpDown, RefreshCw, Search, Tag } from "lucide-react";
import type { ShopCategory, ShopMod, ShopSort, ShopStatus } from "../../types";
import {
  SHOP_CATALOG_UPDATED,
  SHOP_SORTS,
  shopCatalogCategories,
  shopCatalogRefresh,
  shopCatalogSearch,
  shopCatalogStatus,
} from "../../api/shop";
import ShopCard from "./ShopCard";
import ShopDetail from "./ShopDetail";
import { Button } from "@/Components/ui/button";
import { Skeleton } from "@/Components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/Components/ui/select";
import { cn } from "@/lib/utils";
import { useT } from "../../i18n/context";

/**
 * Browse the mxbikes-shop.com catalog.
 *
 * Mirrors `Browse`'s contract deliberately — 350 ms search debounce, a page-1 effect keyed
 * on the filters, append-on-load-more, eight skeletons while loading — so the two catalogs
 * behave identically even though they're separate pages.
 *
 * It owns `openId` itself rather than routing detail through `useModBrowsing`, which is
 * shared by `Dashboard` and `Overlay` and has no business knowing about the shop.
 *
 * Everything the grid needs is already in memory on the Rust side, so filtering and paging
 * are effectively instant; the only slow thing that can happen here is the very first fetch.
 */
export default function ShopCatalog() {
  const t = useT();

  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [categoryId, setCategoryId] = useState<number | null>(null);
  const [sort, setSort] = useState<ShopSort>("recentlyUpdated");
  const [onSaleOnly, setOnSaleOnly] = useState(false);

  const [categories, setCategories] = useState<ShopCategory[]>([]);
  const [items, setItems] = useState<ShopMod[]>([]);
  const [currency, setCurrency] = useState("USD");
  const [status, setStatus] = useState<ShopStatus | null>(null);
  const [staleDismissed, setStaleDismissed] = useState(false);

  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const [openId, setOpenId] = useState<number | null>(null);

  // Debounce the search box so a fast typist doesn't re-run the scan on every keystroke.
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 350);
    return () => clearTimeout(timer);
  }, [query]);

  // (Re)load page 1 whenever a filter changes.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setPage(1);
    shopCatalogSearch(debounced, categoryId, 1, sort, onSaleOnly)
      .then((res) => {
        if (cancelled) return;
        setItems(res.items);
        setHasMore(res.hasMore);
        setCurrency(res.currency);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [debounced, categoryId, sort, onSaleOnly, reloadKey]);

  // Categories come from the dump, so they arrive with the first successful fetch. A failure
  // is silent: the pill row just stays at "All", and the grid still works.
  useEffect(() => {
    let cancelled = false;
    shopCatalogCategories()
      .then((res) => !cancelled && setCategories(res))
      .catch(() => {});
    shopCatalogStatus()
      .then((s) => !cancelled && setStatus(s))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [reloadKey]);

  // The backend refreshes in the background when its copy goes stale, and says so when a
  // newer catalog lands. Re-running the search is what makes prices correct themselves
  // without the user touching anything.
  useEffect(() => {
    const pending = listen<ShopStatus>(SHOP_CATALOG_UPDATED, (event) => {
      setStatus(event.payload);
      setStaleDismissed(false);
      setReloadKey((k) => k + 1);
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const loadMore = useCallback(async () => {
    const next = page + 1;
    setLoadingMore(true);
    try {
      const res = await shopCatalogSearch(debounced, categoryId, next, sort, onSaleOnly);
      setItems((prev) => [...prev, ...res.items]);
      setHasMore(res.hasMore);
      setPage(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  }, [debounced, categoryId, sort, onSaleOnly, page]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      setStatus(await shopCatalogRefresh());
      setStaleDismissed(false);
      setReloadKey((k) => k + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, []);

  if (openId !== null) {
    return (
      <ShopDetail
        id={openId}
        currency={currency}
        onBack={() => setOpenId(null)}
      />
    );
  }

  // Only top-level categories in the first row; the children of whatever is selected go in a
  // second row, so a deep tree doesn't turn into a wall of pills.
  const roots = categories.filter((c) => c.depth === 0);
  const selected = categories.find((c) => c.id === categoryId);
  const branchRoot = selected ? (selected.depth === 0 ? selected.id : selected.parent) : null;
  const children = branchRoot === null ? [] : categories.filter((c) => c.parent === branchRoot);

  const showStale = status?.stale && !staleDismissed;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* No title of its own — `Shop` owns the heading and the tab strip above this. */}
      <header className="flex flex-none flex-col gap-4 px-7 pb-3.5">
        <div className="flex items-center gap-3.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void refresh()}
            disabled={refreshing}
            className="flex-none"
          >
            <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
            {refreshing ? t("shopCatalog.refreshing") : t("shopCatalog.refresh")}
          </Button>
          <div className="ml-auto flex w-[280px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
            <Search className="size-3.5 text-faint" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("shopCatalog.searchPlaceholder")}
              className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
            />
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <CategoryPill
            label={t("shopCatalog.allCategories")}
            on={categoryId === null}
            onClick={() => setCategoryId(null)}
          />
          {roots.map((c) => (
            <CategoryPill
              key={c.id}
              label={c.name}
              count={c.count}
              on={categoryId === c.id || selected?.parent === c.id}
              onClick={() => setCategoryId(c.id)}
            />
          ))}
          <div className="ml-auto flex items-center gap-2 self-center">
            <button
              onClick={() => setOnSaleOnly((v) => !v)}
              className={cn(
                "flex cursor-default items-center gap-1.5 rounded-full px-3 py-[5px] text-[12px] font-medium transition-colors",
                onSaleOnly
                  ? "bg-emerald-500 font-semibold text-black"
                  : "border border-input text-muted-foreground hover:text-foreground",
              )}
            >
              <Tag className="size-3" />
              {t("shopCatalog.onSaleOnly")}
            </button>
            <ArrowUpDown className="size-3.5 text-faint" />
            <Select value={sort} onValueChange={(v) => setSort(v as ShopSort)}>
              <SelectTrigger className="h-8 w-[210px] bg-card">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {SHOP_SORTS.map((s) => (
                  <SelectItem key={s.value} value={s.value}>
                    {t(s.label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        {children.length > 0 && (
          <div className="-mt-1 flex flex-wrap items-center gap-2">
            {children.map((c) => (
              <CategoryPill
                key={c.id}
                label={c.name}
                count={c.count}
                small
                on={categoryId === c.id}
                onClick={() => setCategoryId(c.id)}
              />
            ))}
          </div>
        )}

        {showStale && (
          <div
            className={cn(
              "flex items-center gap-3 rounded-lg border px-3.5 py-2 text-[12.5px]",
              status?.veryStale
                ? "border-amber-500/40 bg-amber-500/10 text-amber-200"
                : "border-input bg-card text-muted-foreground",
            )}
          >
            <span className="min-w-0 flex-1">
              {status?.veryStale
                ? t("shopCatalog.staleHard", { when: agoLabel(status.fetchedAt, t) })
                : t("shopCatalog.stale", { when: agoLabel(status?.fetchedAt ?? null, t) })}
            </span>
            <Button variant="outline" size="sm" onClick={() => void refresh()}>
              {t("shopCatalog.refresh")}
            </Button>
            {/* A catalog days out of date isn't something to let someone wave away —
                the prices on screen may simply be wrong. */}
            {!status?.veryStale && (
              <button
                onClick={() => setStaleDismissed(true)}
                className="cursor-default text-faint hover:text-foreground"
              >
                {t("common.dismiss")}
              </button>
            )}
          </div>
        )}
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
            <p className="text-[13px] font-semibold text-destructive">
              {t("shopCatalog.loadFailed")}
            </p>
            <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
              {error.replace(/^Error:\s*/, "")}
            </p>
            <Button variant="outline" size="sm" onClick={() => setReloadKey((k) => k + 1)}>
              {t("common.retry")}
            </Button>
          </div>
        ) : loading ? (
          <div className="grid grid-cols-4 gap-3.5">
            {/* Square, to match the cards that replace them — a different shape here makes
                the whole grid jump the moment results land. */}
            {Array.from({ length: 8 }).map((_, i) => (
              <Skeleton key={i} className="aspect-square rounded-xl" />
            ))}
          </div>
        ) : items.length === 0 ? (
          <p className="py-20 text-center text-[13px] text-muted-foreground">
            {t("shopCatalog.empty")}
          </p>
        ) : (
          <>
            <div className="grid grid-cols-4 gap-3.5">
              {items.map((m) => (
                <ShopCard
                  key={m.id}
                  mod={m}
                  currency={currency}
                  onOpen={() => setOpenId(m.id)}
                />
              ))}
            </div>
            {hasMore && (
              <div className="flex justify-center pt-4">
                <Button variant="outline" onClick={loadMore} disabled={loadingMore}>
                  {loadingMore ? t("common.loading") : t("shopCatalog.loadMore")}
                </Button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function CategoryPill({
  label,
  count,
  on,
  small,
  onClick,
}: {
  label: string;
  count?: number;
  on: boolean;
  small?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "cursor-default rounded-full transition-colors",
        small ? "px-3 py-[3px] text-[11.5px]" : "px-3.5 py-[5px] text-[12px]",
        "font-medium",
        on
          ? "bg-foreground font-semibold text-background"
          : "border border-input text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
      {count !== undefined && count > 0 && (
        <span className={cn("ml-1.5", on ? "text-background/60" : "text-faint")}>
          {count}
        </span>
      )}
    </button>
  );
}

/** "3 days ago" / "2 hours ago" / "just now", for the staleness bar. */
function agoLabel(fetchedAt: number | null, t: ReturnType<typeof useT>): string {
  if (fetchedAt === null) return t("shopCatalog.agoUnknown");
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - fetchedAt);
  const days = Math.floor(seconds / 86_400);
  if (days >= 1) return t("shopCatalog.agoDays", { count: days });
  const hours = Math.floor(seconds / 3_600);
  if (hours >= 1) return t("shopCatalog.agoHours", { count: hours });
  const minutes = Math.floor(seconds / 60);
  if (minutes >= 1) return t("shopCatalog.agoMinutes", { count: minutes });
  return t("shopCatalog.agoJustNow");
}
