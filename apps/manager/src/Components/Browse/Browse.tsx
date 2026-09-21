import { useLayoutEffect, useRef } from "react";
import { Download, X } from "lucide-react";
import { type ModSort, type ModType } from "@frost/shared/api/mods";
import type { InstalledIndex } from "../../lib/installedMatch";
import type { ModListing } from "../../lib/useModListing";
import { useT } from "@/i18n";
import ModCard from "./ModCard";
import FeaturedMod from "./FeaturedMod";
import { useQuickInstall } from "./useQuickInstall";
import { SearchBox } from "@frost/shared/Components/ui/search-box";
import { Button } from "@frost/shared/Components/ui/button";
import { ContextBarLeft, ContextBarRight, ContextTab } from "../Shell/ContextBar";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import { Skeleton } from "@frost/shared/Components/ui/skeleton";
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from "@frost/shared/Components/ui/select";

interface BrowseProps {
  modType: ModType;
  /** The active game's browse tree — the catalogs differ per title. */
  modTypes: ModType[];
  /**
   * Filters, fetched pages and scroll offset. Owned by `useModBrowsing` above this
   * component, which is what lets it survive a trip into a mod's detail page — see
   * `useModListing`.
   */
  listing: ModListing;
  installed: InstalledIndex;
  onOpenMod: (slug: string, categoryId: number) => void;
  onChangeType: (type: ModType) => void;
  /**
   * Whether this grid fills the context bar itself.
   *
   * False inside the Mods screen, which owns one search box, one sort and one source filter
   * for three catalogues — two components portalling into the same slot would stack two of
   * each. True everywhere else (the in-game overlay), where nothing else is competing for it.
   */
  chrome?: boolean;
}

export default function Browse({
  modType,
  modTypes,
  listing,
  installed,
  onOpenMod,
  onChangeType,
  chrome = true,
}: BrowseProps) {
  const t = useT();
  const {
    query,
    setQuery,
    categoryId,
    setCategoryId,
    setSort,
    sortOptions,
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
  } = listing;

  const selectionActive = selected.size > 0;

  // The grid scroller. Its offset is kept in `listing` rather than here, so opening a mod
  // and coming back — which unmounts this component — lands on the same row of cards.
  const grid = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    // Before paint, so the lazy thumbnails load for the row we're actually on and there
    // is no jump from the top.
    if (grid.current) grid.current.scrollTop = scrollTop.current;
  }, [scrollTop]);

  const { quickInstall, quickInstallMany, isInstalled, dialog } = useQuickInstall(
    modType,
    categoryId,
    installed,
  );

  const bulkInstall = () => quickInstallMany([...selected.values()], clearSelection);

  const isBike = modType.id === "bikes";

  // "Newest, with a picture" is the only claim the catalog actually supports, so the
  // banner appears on the default sort with nothing filtered and stands down otherwise.
  const featured =
    !query.trim() &&
    (categoryId === null || categoryId === modType.categoryId) &&
    activeSort === sortOptions[0]?.value
      ? mods.find((m) => m.image)
      : undefined;

  return (
    <div className="flex h-full flex-col">
      {/* The type tabs and the search/sort controls live in the shell's context bar. The
          rail already says MODS, so the page does not repeat it as a heading — that
          stacked title-then-tabs-then-filters column is what read as a dashboard. */}
      {chrome && (
        <>
          <ContextBarLeft>
            {modTypes.map((mt) => (
              <ContextTab
                key={mt.id}
                active={mt.id === modType.id}
                onSelect={() => onChangeType(mt)}
              >
                {t(mt.label)}
              </ContextTab>
            ))}
          </ContextBarLeft>

          <ContextBarRight>
            <SearchBox
              value={query}
              onChange={setQuery}
              placeholder={t("browse.searchPlaceholder", { type: t(modType.labelInline) })}
              className="w-[210px]"
            />
            {/* The category filter was a row of pills of its own. Three bands of chrome
                before the first mod is what this redesign set out to remove, so it folds in
                here beside the sort it belongs with. */}
            <Select
              value={String(categoryId ?? modType.categoryId)}
              onValueChange={(v) => setCategoryId(Number(v))}
            >
              <SelectTrigger className="h-7 w-[164px] bg-card text-[12px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {modType.categories.map((c) => (
                  <SelectItem key={c.id} value={String(c.id)}>
                    {t(c.label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={activeSort} onValueChange={(v) => setSort(v as ModSort)}>
              {/* Wide enough for the longest translated label ("Popolari questa
                  settimana") rather than the English one. */}
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
            <HelpHint title={t("nav.mods")} description={t("browse.help")} />
          </ContextBarRight>
        </>
      )}

      <div
        ref={grid}
        onScroll={(e) => (scrollTop.current = e.currentTarget.scrollTop)}
        className="min-h-0 flex-1 overflow-y-auto px-7 pb-6"
      >
        {error ? (
          <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
            <p className="text-[13px] font-semibold text-destructive">
              {t("browse.loadFailed")}
            </p>
            {/* The backend now explains blocks in plain words; show that on its own line
                rather than glued to the heading, and keep it selectable for bug reports. */}
            <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
              {error.replace(/^Error:\s*/, "")}
            </p>
            <Button variant="outline" size="sm" onClick={reload}>
              {t("common.retry")}
            </Button>
          </div>
        ) : loading ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(178px,1fr))] gap-3.5">
            {Array.from({ length: 8 }).map((_, i) => (
              <Skeleton key={i} className="aspect-[4/3] rounded-xl" />
            ))}
          </div>
        ) : mods.length === 0 ? (
          <p className="py-20 text-center text-[13px] text-muted-foreground">
            {t("browse.empty", { type: t(modType.labelInline) })}
          </p>
        ) : (
          <>
            {featured && (
              <FeaturedMod
                mod={featured}
                rating={ratings.get(featured.id)}
                installed={isInstalled(featured)}
                onOpen={() => onOpenMod(featured.slug, categoryId ?? modType.categoryId)}
                onInstall={() => quickInstall(featured)}
              />
            )}
            <div className="grid grid-cols-[repeat(auto-fill,minmax(178px,1fr))] gap-3.5">
              {mods.filter((m) => m !== featured).map((m) => (
                <ModCard
                  key={m.id}
                  mod={m}
                  rating={ratings.get(m.id)}
                  isBike={isBike}
                  installed={isInstalled(m)}
                  selected={selected.has(m.slug)}
                  selectionActive={selectionActive}
                  onOpen={() => onOpenMod(m.slug, categoryId)}
                  onToggleSelect={() => toggleSelect(m)}
                  onQuickInstall={() => quickInstall(m)}
                />
              ))}
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

      {selectionActive && (
        <div className="flex flex-none items-center gap-3 border-t border-white/[0.08] bg-window px-7 py-3">
          <span className="text-[12.5px] font-semibold">
            {t("browse.selectedCount", { count: selected.size })}
          </span>
          <Button size="sm" onClick={bulkInstall}>
            <Download className="size-3.5" />
            {t("browse.quickInstallCount", { count: selected.size })}
          </Button>
          <Button size="sm" variant="outline" onClick={selectAll}>
            {t("common.selectAll")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={clearSelection}
            className="ml-auto"
          >
            <X className="size-3.5" /> {t("common.clear")}
          </Button>
        </div>
      )}

      {dialog}
    </div>
  );
}
