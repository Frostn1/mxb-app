import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCw } from "lucide-react";
import type { ModType } from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { SearchBox } from "@frost/shared/Components/ui/search-box";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { Skeleton } from "@frost/shared/Components/ui/skeleton";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import type { ModListing } from "../../lib/useModListing";
import type { InstalledIndex } from "../../lib/installedMatch";
import { shopCatalogAvailable } from "../../api/shop";
import { hubDetail } from "../../api/hub";
import Browse from "../Browse/Browse";
import { useQuickInstall } from "../Browse/useQuickInstall";
import ShopDetail from "../Shop/ShopDetail";
import MyDownloads from "../Shop/MyDownloads";
import HubPurchases from "../Hub/HubPurchases";
import { ContextBarLeft, ContextBarRight } from "../Shell/ContextBar";
import type { ModsView } from "../Shell/nav";
import ModsCard from "./ModsCard";
import SourceFilter from "./SourceFilter";
import TypeList, { type TypeListCategory } from "./TypeList";
import { useStoreListing } from "./useStoreListing";
import {
  MODS_SORTS,
  fromModSummary,
  sortMerged,
  storeRootsFor,
  toModSort,
  type MergedMod,
  type ModSource,
  type ModsSort,
  type StoreId,
} from "./sources";

const GROUPED_CATEGORY_ID = -1;

interface ModsProps {
  /** `browse`, `hub` or `shop` — an old deep link still lands on the source it named. */
  view: ModsView;
  modType: ModType;
  modTypes: ModType[];
  listing: ModListing;
  installed: InstalledIndex;
  /** Bumped after any install, so the purchases grids re-scan their "Installed" badges. */
  refreshKey: number;
  onOpenMod: (slug: string, categoryId: number) => void;
  onChangeType: (type: ModType) => void;
}

/** Which source a view id means, so `shop` and `hub` keep resolving after the fold. */
function sourceForView(view: ModsView): ModSource {
  return view === "hub" ? "hub" : view === "shop" ? "shop" : "mods";
}

/**
 * Find a mod and install it — from mxb-mods, MXB Hub or the shop, in one screen.
 *
 * These were three rail items for one errand, and `Shop` and `Hub` opened with the same
 * comment about being "a catalog with two tabs". Folding them into a tab row would have made
 * it worse: rail, then source tabs, then type tabs, then a category menu, four levels deep
 * before the first mod. So the decisions are split by kind instead. What you are after —
 * type, then category — goes down the left edge, because it is a place you stay. Where it
 * comes from is a filter on the bar, because it changes what is already on screen.
 */
export default function Mods({
  view,
  modType,
  modTypes,
  listing,
  installed,
  refreshKey,
  onOpenMod,
  onChangeType,
}: ModsProps) {
  const t = useT();
  const { game } = useConfig();
  const storesAllowed = Boolean(game.caps.shop);

  const [source, setSource] = useState<ModSource>(() => sourceForView(view));
  const [panel, setPanel] = useState<"catalog" | "purchases">("catalog");
  const [sort, setSort] = useState<ModsSort>("newest");
  // A sub-category under the store's own tree, or null for the whole type.
  const [storeCategoryId, setStoreCategoryId] = useState<number | null>(null);
  const [open, setOpen] = useState<{ store: StoreId; id: number } | null>(null);
  // A compile-time fact on the Rust side, so it's asked once and can't change under us.
  // `null` is "not asked yet", and nothing settles the source filter until it is known —
  // otherwise a deep link to the shop would be bounced off it a tick before the answer lands.
  const [shopAvailable, setShopAvailable] = useState<boolean | null>(
    storesAllowed ? null : false,
  );

  useEffect(() => {
    if (!storesAllowed) return;
    let cancelled = false;
    shopCatalogAvailable()
      .then((ok) => !cancelled && setShopAvailable(ok))
      .catch(() => !cancelled && setShopAvailable(false));
    return () => {
      cancelled = true;
    };
  }, [storesAllowed]);

  // A deep link names a source; following one also closes whatever detail page was open.
  useEffect(() => {
    setSource(sourceForView(view));
    setPanel("catalog");
    setOpen(null);
  }, [view]);

  // Only the sources this build and this title can actually answer for. A shop with no
  // credential is not offered at all — an empty segment that can never fill is worse than a
  // shorter control.
  const canHub = storesAllowed;
  const canShop = storesAllowed && shopAvailable === true;

  const sources = useMemo<ModSource[]>(() => {
    const stores: ModSource[] = [];
    if (canHub) stores.push("hub");
    if (canShop) stores.push("shop");
    return stores.length ? ["all", "mods", ...stores] : ["mods"];
  }, [canHub, canShop]);

  // The shop going away under us — a build without it, or a title that doesn't sell — must
  // not strand the screen on a source that answers nothing.
  useEffect(() => {
    if (shopAvailable === null) return;
    if (!sources.includes(source)) {
      setSource("mods");
      setPanel("catalog");
    }
  }, [sources, source, shopAvailable]);

  const modsOn = source === "mods" || source === "all";
  const hubOn = canHub && (source === "hub" || source === "all");
  const shopOn = canShop && (source === "shop" || source === "all");
  const store: StoreId | null =
    source === "hub" && canHub ? "hub" : source === "shop" && canShop ? "shop" : null;
  const browsing = panel === "catalog";

  // One order control for three vocabularies: only the orders at least one live source can
  // honour are offered, and a pick that stops being offered — "Oldest" is mxb-mods' alone —
  // falls back rather than silently ordering by something else.
  const sortOptions = MODS_SORTS.filter((s) =>
    source === "all"
      ? s.sources.some((src) => (src === "mods" ? modsOn : src === "hub" ? hubOn : shopOn))
      : s.sources.includes(source),
  );
  const activeSort = sortOptions.some((s) => s.value === sort) ? sort : "newest";

  // `useModListing` keeps its own vocabulary, so the pick is translated down into it rather
  // than duplicated beside it.
  const setListingSort = listing.setSort;
  useEffect(() => setListingSort(toModSort(activeSort)), [setListingSort, activeSort]);

  // Switching type or source starts the store filter over: the trees are unrelated.
  useEffect(() => setStoreCategoryId(null), [modType, source]);

  const hub = useStoreListing({
    store: "hub",
    enabled: hubOn && browsing,
    query: listing.query,
    modType,
    categoryId: source === "hub" ? storeCategoryId : null,
    sort: activeSort,
  });
  const shop = useStoreListing({
    store: "shop",
    enabled: shopOn && browsing,
    query: listing.query,
    modType,
    categoryId: source === "shop" ? storeCategoryId : null,
    sort: activeSort,
  });
  const active = store === "hub" ? hub : store === "shop" ? shop : null;

  const { quickInstall, isInstalled, dialog } = useQuickInstall(
    modType,
    listing.categoryId,
    installed,
  );

  /* ── the left column ─────────────────────────────────────────────────────────────── */

  // Counts come from the stores' own category trees, which arrive with the catalogue. There
  // is deliberately none for mxb-mods: its API publishes no per-category total, and counting
  // would mean four searches against a third-party site nobody asked for.
  const counts = useMemo(() => {
    const map = new Map<string, number>();
    if (!active) return map;
    for (const mt of modTypes) {
      const roots = storeRootsFor(store!, mt, active.categories);
      // Separate branches can contain the same product, so summing them would overstate the
      // group. A count is optional; omit it until the grouped search response supplies one.
      if (roots.length === 1) map.set(mt.id, roots[0].count);
    }
    return map;
  }, [active, modTypes, store]);

  const categories = useMemo<TypeListCategory[]>(() => {
    if (!active) {
      return modType.categories.map((c) => ({ id: c.id, label: t(c.label) }));
    }
    const { root, roots } = active;
    if (!root) return [];
    if (roots.length > 1) {
      return [
        { id: GROUPED_CATEGORY_ID, label: t("shopCatalog.allCategories") },
        ...roots.map((category) => ({ id: category.id, label: category.name })),
      ];
    }
    const children = active.categories.filter((c) => c.parent === root.id);
    return [
      { id: root.id, label: t("shopCatalog.allCategories") },
      ...children.map((c) => ({ id: c.id, label: c.name })),
    ];
  }, [active, modType, t]);

  const activeCategoryId = active
    ? (storeCategoryId ?? (active.roots.length > 1 ? GROUPED_CATEGORY_ID : active.root?.id) ?? null)
    : listing.categoryId;

  const selectCategory = useCallback(
    (id: number) => {
      if (store) {
        const allId = active && active.roots.length > 1 ? GROUPED_CATEGORY_ID : active?.root?.id;
        setStoreCategoryId(id === allId ? null : id);
      } else listing.setCategoryId(id);
    },
    [store, active, listing],
  );

  const selectType = useCallback(
    (mt: ModType) => {
      setPanel("catalog");
      onChangeType(mt);
    },
    [onChangeType],
  );

  /* ── the grid ────────────────────────────────────────────────────────────────────── */

  const items = useMemo<MergedMod[]>(() => {
    if (source === "mods") return [];
    const out: MergedMod[] = [];
    if (modsOn) out.push(...listing.mods.map(fromModSummary));
    if (hubOn) out.push(...hub.items);
    if (shopOn) out.push(...shop.items);
    return source === "all" ? sortMerged(out, activeSort) : out;
  }, [source, modsOn, hubOn, shopOn, listing.mods, hub.items, shop.items, activeSort]);

  const busy =
    (modsOn && listing.loading) || (hubOn && hub.loading) || (shopOn && shop.loading);
  const errors = [
    modsOn ? listing.error : null,
    hubOn ? hub.error : null,
    shopOn ? shop.error : null,
  ].filter((e): e is string => Boolean(e));
  const enabledCount = [modsOn, hubOn, shopOn].filter(Boolean).length;
  // One store being down must not blank a merged grid the other two filled.
  const allFailed = errors.length > 0 && errors.length === enabledCount;
  const hasMore =
    (modsOn && listing.hasMore) || (hubOn && hub.hasMore) || (shopOn && shop.hasMore);
  const loadingMore =
    (modsOn && listing.loadingMore) || (hubOn && hub.loadingMore) || (shopOn && shop.loadingMore);

  const loadMore = useCallback(() => {
    if (modsOn && listing.hasMore) listing.loadMore();
    if (hubOn && hub.hasMore) hub.loadMore();
    if (shopOn && shop.hasMore) shop.loadMore();
  }, [modsOn, hubOn, shopOn, listing, hub, shop]);

  const retry = useCallback(() => {
    if (modsOn) listing.reload();
    if (hubOn) hub.reload();
    if (shopOn) shop.reload();
  }, [modsOn, hubOn, shopOn, listing, hub, shop]);

  const openItem = useCallback(
    (item: MergedMod) => {
      if (item.source === "mods" && item.mod) {
        onOpenMod(item.mod.slug, listing.categoryId ?? modType.categoryId);
      } else if (item.source !== "mods") {
        setOpen({ store: item.source, id: item.id });
      }
    },
    [onOpenMod, listing.categoryId, modType.categoryId],
  );

  // A store's detail page takes the whole screen, the way it did when the store was its own
  // rail item — the left column is a filter on a grid that isn't on screen any more.
  if (open) {
    return (
      <ShopDetail
        id={open.id}
        currency={open.store === "hub" ? hub.currency : shop.currency}
        load={open.store === "hub" ? hubDetail : undefined}
        onBack={() => setOpen(null)}
      />
    );
  }

  const showStale = shopOn && shop.status?.stale;

  return (
    <div className="flex h-full min-h-0">
      <TypeList
        modTypes={modTypes}
        modType={modType}
        counts={counts}
        categories={browsing ? categories : []}
        categoryId={activeCategoryId}
        onChangeType={selectType}
        onChangeCategory={selectCategory}
        purchases={
          store
            ? { active: !browsing, onSelect: () => setPanel("purchases") }
            : undefined
        }
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {/* Where it comes from reads before what you are looking at, so it sits at the head
            of the bar rather than trailing the search and sort that act on the result. */}
        <ContextBarLeft>
          <SourceFilter options={sources} value={source} onChange={setSource} />
        </ContextBarLeft>

        <ContextBarRight>
          {browsing && (
            <>
              <SearchBox
          value={listing.query}
          onChange={listing.setQuery}
          placeholder={t("mods.searchPlaceholder")}
          className="w-[210px]"
        />
              <Select value={activeSort} onValueChange={(v) => setSort(v as ModsSort)}>
                <SelectTrigger className="h-7 w-[196px] bg-card text-[12px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {sortOptions.map((s) => (
                    <SelectItem key={s.value} value={s.value}>
                      {t(s.label)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </>
          )}
          <HelpHint title={t("nav.mods")} description={t("mods.help")} />
        </ContextBarRight>

        {!browsing && store && (
          <div className="flex min-h-0 flex-1 flex-col">
            {store === "shop" ? (
              <MyDownloads refreshKey={refreshKey} />
            ) : (
              <HubPurchases refreshKey={refreshKey} />
            )}
          </div>
        )}

        {browsing && source === "mods" && (
          <div className="min-h-0 flex-1">
            <Browse
              chrome={false}
              modType={modType}
              modTypes={modTypes}
              listing={listing}
              installed={installed}
              onOpenMod={onOpenMod}
              onChangeType={onChangeType}
            />
          </div>
        )}

        {browsing && source !== "mods" && (
          <div className="flex min-h-0 flex-1 flex-col">
            {showStale && (
              <div
                className={cn(
                  "mx-7 mb-3 flex items-center gap-3 rounded-lg border px-3.5 py-2 text-[12.5px]",
                  shop.status?.veryStale
                    ? "border-warning/40 bg-warning/10 text-warning"
                    : "border-input bg-card text-muted-foreground",
                )}
              >
                <span className="min-w-0 flex-1">{t("mods.shopStale")}</span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void shop.refresh()}
                  disabled={shop.refreshing}
                >
                  <RefreshCw className={cn("size-3.5", shop.refreshing && "animate-spin")} />
                  {shop.refreshing ? t("shopCatalog.refreshing") : t("shopCatalog.refresh")}
                </Button>
              </div>
            )}

            <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
              {allFailed ? (
                <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
                  <p className="text-[13px] font-semibold text-destructive">
                    {t("browse.loadFailed")}
                  </p>
                  <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
                    {errors[0].replace(/^Error:\s*/, "")}
                  </p>
                  <Button variant="outline" size="sm" onClick={retry}>
                    {t("common.retry")}
                  </Button>
                </div>
              ) : items.length === 0 && busy ? (
                <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3.5">
                  {Array.from({ length: 8 }).map((_, i) => (
                    <Skeleton key={i} className="aspect-[4/3] rounded-xl" />
                  ))}
                </div>
              ) : items.length === 0 ? (
                <p className="py-20 text-center text-[13px] text-muted-foreground">
                  {t("mods.empty")}
                </p>
              ) : (
                <>
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3.5">
                    {items.map((item) => {
                      const mod = item.mod;
                      return (
                      <ModsCard
                        key={item.key}
                        item={item}
                        currency={item.source === "hub" ? hub.currency : shop.currency}
                        installed={mod ? isInstalled(mod) : false}
                        onOpen={() => openItem(item)}
                        onQuickInstall={mod ? () => quickInstall(mod) : undefined}
                      />
                      );
                    })}
                  </div>
                  {hasMore && (
                    <div className="flex justify-center pt-4">
                      <Button variant="outline" onClick={loadMore} disabled={loadingMore}>
                        {loadingMore ? t("common.loading") : t("browse.loadMore")}
                      </Button>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        )}
      </div>

      {dialog}
    </div>
  );
}
