import { useCallback, useEffect, useState } from "react";
import { RefreshCw, Search, Tag } from "lucide-react";
import type { HubCategory, HubMod, HubSort } from "../../types";
import { HUB_SORTS, hubCategories, hubDetail, hubSearch } from "../../api/hub";
import ShopCard from "../Shop/ShopCard";
import ShopDetail from "../Shop/ShopDetail";
import CategoryPill from "../Shop/CategoryPill";
import { ContextBarRight } from "../Shell/ContextBar";
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
 * Browse the MXB Hub catalog.
 *
 * Mirrors `ShopCatalog`'s contract deliberately — 350 ms search debounce, a page-1 effect keyed
 * on the filters, append-on-load-more, eight skeletons while loading — so all three catalogs in
 * the app behave the same. The cards and the detail page are literally the shop's: a Hub
 * listing is a `ShopMod` with a slug on it.
 *
 * Where it differs is underneath. The shop's catalog is a background-refreshed dump, so that
 * view carries a staleness bar, a "generated" timestamp and a Refresh that re-fetches the whole
 * thing. This one queries the store live on every filter change, so none of that exists: what
 * is on screen is what the store said a moment ago, and Refresh just asks again.
 */
export default function HubCatalog() {
  const t = useT();

  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [categoryId, setCategoryId] = useState<number | null>(null);
  const [sort, setSort] = useState<HubSort>("newest");
  const [onSaleOnly, setOnSaleOnly] = useState(false);

  const [categories, setCategories] = useState<HubCategory[]>([]);
  const [items, setItems] = useState<HubMod[]>([]);
  const [currency, setCurrency] = useState("USD");
  const [total, setTotal] = useState(0);

  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const [openId, setOpenId] = useState<number | null>(null);

  // Debounce the search box so a fast typist doesn't fire a request per keystroke — this
  // catalog goes to the network on every change, where the shop's answers from memory.
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
    hubSearch(debounced, categoryId, 1, sort, onSaleOnly)
      .then((res) => {
        if (cancelled) return;
        setItems(res.items);
        setHasMore(res.hasMore);
        setCurrency(res.currency);
        setTotal(res.total);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [debounced, categoryId, sort, onSaleOnly, reloadKey]);

  // A failure here is silent: the pill row stays at "All" and the grid still works.
  useEffect(() => {
    let cancelled = false;
    hubCategories()
      .then((res) => !cancelled && setCategories(res))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [reloadKey]);

  const loadMore = useCallback(async () => {
    const next = page + 1;
    setLoadingMore(true);
    try {
      const res = await hubSearch(debounced, categoryId, next, sort, onSaleOnly);
      setItems((prev) => [...prev, ...res.items]);
      setHasMore(res.hasMore);
      setPage(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  }, [debounced, categoryId, sort, onSaleOnly, page]);

  if (openId !== null) {
    return (
      <ShopDetail
        id={openId}
        currency={currency}
        load={hubDetail}
        onBack={() => setOpenId(null)}
      />
    );
  }

  // Only top-level categories in the first row; the children of whatever is selected go in a
  // second row, so a 99-category tree doesn't turn into a wall of pills.
  const roots = categories.filter((c) => c.depth === 0);
  const selected = categories.find((c) => c.id === categoryId);
  const branchRoot = selected ? (selected.depth === 0 ? selected.id : selected.parent) : null;
  const children = branchRoot === null ? [] : categories.filter((c) => c.parent === branchRoot);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* No title of its own — `Hub` owns the heading and the tab strip above this. */}
      {/* No title of its own — `Hub` owns the heading and the tab strip above this. */}
      <ContextBarRight>
        {!loading && !error && (
          <span className="tabular-figures text-[12.5px] text-muted-foreground">
            {t("hub.count", { count: total })}
          </span>
        )}
        <div className="flex h-7 w-[220px] items-center gap-2 border border-input bg-card px-2.5">
          <Search className="size-3.5 text-faint" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("hub.searchPlaceholder")}
            className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
          />
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setReloadKey((k) => k + 1)}
          disabled={loading}
        >
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
          {t("shopCatalog.refresh")}
        </Button>
        <Select
          value={categoryId === null ? "all" : String(categoryId)}
          onValueChange={(v) => setCategoryId(v === "all" ? null : Number(v))}
        >
          <SelectTrigger className="h-7 w-[170px] bg-card text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("shopCatalog.allCategories")}</SelectItem>
            {roots.map((c) => (
              <SelectItem key={c.id} value={String(c.id)}>
                {c.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select value={sort} onValueChange={(v) => setSort(v as HubSort)}>
          <SelectTrigger className="h-7 w-[190px] bg-card text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {HUB_SORTS.map((s) => (
              <SelectItem key={s.value} value={s.value}>
                {t(s.label)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          variant={onSaleOnly ? "secondary" : "outline"}
          size="sm"
          onClick={() => setOnSaleOnly((v) => !v)}
        >
          <Tag className="size-3" />
          {t("shopCatalog.onSaleOnly")}
        </Button>
      </ContextBarRight>

        {children.length > 0 && (
          <div className="flex flex-none flex-wrap items-center gap-2 px-7 pb-3 pt-3">
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
