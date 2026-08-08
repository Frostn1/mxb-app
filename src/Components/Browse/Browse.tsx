import { useCallback, useEffect, useRef, useState } from "react";
import { Search, Download, X, ArrowUpDown } from "lucide-react";
import { toast } from "sonner";
import {
  MOD_SORTS,
  SEARCH_PAGE_SIZE,
  getModRatings,
  isLiveryContext,
  resolveQuickInstall,
  searchMods,
  type ModSort,
  type ModType,
} from "../../api/mods";
import type { InstalledIndex } from "../../lib/installedMatch";
import type { ModRating, ModSummary } from "../../types";
import { useInstall } from "../../Context/Install";
import { useT } from "../../i18n/context";
import ModCard from "./ModCard";
import { Segmented } from "@/Components/ui/segmented";
import { Button } from "@/Components/ui/button";
import HelpHint from "@/Components/ui/help-hint";
import { Skeleton } from "@/Components/ui/skeleton";
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from "@/Components/ui/select";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@/Components/ui/alert-dialog";
import { cn } from "@/lib/utils";

interface BrowseProps {
  modType: ModType;
  /** The active game's browse tree — the catalogs differ per title. */
  modTypes: ModType[];
  installed: InstalledIndex;
  onOpenMod: (slug: string, categoryId: number) => void;
  onChangeType: (type: ModType) => void;
}

export default function Browse({
  modType,
  modTypes,
  installed,
  onOpenMod,
  onChangeType,
}: BrowseProps) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [categoryId, setCategoryId] = useState(modType.categoryId);
  const [sort, setSort] = useState<ModSort>("newest");
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [ratings, setRatings] = useState<Map<number, ModRating>>(new Map());
  // Ids we've already asked about, so a mod the site had no answer for isn't re-requested
  // on every render. Cleared when the listing is rebuilt (the Rust side caches, so asking
  // again after a category switch costs nothing).
  const askedForRatings = useRef<Set<number>>(new Set());
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [selected, setSelected] = useState<Map<string, ModSummary>>(new Map());
  const [bulkBusy, setBulkBusy] = useState(false);
  // A pending reinstall the user must confirm (they already have these mods).
  const [reinstall, setReinstall] = useState<
    { kind: "single"; mod: ModSummary } | { kind: "bulk"; mods: ModSummary[] } | null
  >(null);

  const { startInstall } = useInstall();
  const selectionActive = selected.size > 0;

  // The popular listings are ranked by views by a part of the site that takes no search
  // term, so they step aside while the box has text in it and the order falls back to
  // newest — the control always names the order actually on screen. The pick itself is
  // kept, so clearing the search puts it back.
  const sortOptions = MOD_SORTS.filter((s) => !s.noSearch || !debounced);
  const activeSort = sortOptions.some((s) => s.value === sort) ? sort : "newest";

  const isInstalled = useCallback(
    (mod: ModSummary) => installed.has(mod.title),
    [installed],
  );

  // Reset the category filter (and any selection) when the mod type changes —
  // selection + quick-install resolve against the current type's folders.
  useEffect(() => {
    setCategoryId(modType.categoryId);
    setSelected(new Map());
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

  // Silent quick-install: resolve the mirror + folder, then enqueue.
  const doQuickInstall = useCallback(
    async (mod: ModSummary) => {
      try {
        const res = await resolveQuickInstall(
          mod.slug,
          modType,
          isLiveryContext(modType, categoryId),
        );
        if (res.ok) {
          startInstall({ ...res.params, categoryId });
          toast.success(t("browse.queued", { title: res.params.title }), {
            description: t("browse.queuedDesc", {
              folder: res.params.destFolder || t("browse.rootFolder"),
            }),
          });
        } else if (res.reason === "blocked") {
          toast.error(t("browse.needsBrowser", { title: res.title }), {
            description: t("browse.needsBrowserDesc", { host: res.host ?? "" }),
          });
        } else {
          toast.error(t("browse.noDownload", { title: res.title }));
        }
      } catch (e) {
        toast.error(t("browse.quickInstallFailed", { title: mod.title }), {
          description: String(e),
        });
      }
    },
    [modType, categoryId, startInstall, t],
  );

  // Guard: if the mod is already installed, confirm before overwriting.
  const quickInstall = useCallback(
    (mod: ModSummary) => {
      if (isInstalled(mod)) setReinstall({ kind: "single", mod });
      else void doQuickInstall(mod);
    },
    [isInstalled, doQuickInstall],
  );

  const doBulkInstall = useCallback(
    async (list: ModSummary[]) => {
      setBulkBusy(true);
      let queued = 0;
      const skipped: string[] = [];
      for (const mod of list) {
        try {
          const res = await resolveQuickInstall(
            mod.slug,
            modType,
            isLiveryContext(modType, categoryId),
          );
          if (res.ok) {
            startInstall({ ...res.params, categoryId });
            queued++;
          } else {
            skipped.push(res.title);
          }
        } catch {
          skipped.push(mod.title);
        }
      }
      setBulkBusy(false);
      clearSelection();
      if (queued > 0) {
        toast.success(t("browse.queuedBulk", { count: queued }), {
          description: skipped.length
            ? t("browse.queuedBulkSkipped", { count: skipped.length })
            : t("browse.queuedBulkDesc"),
        });
      } else if (skipped.length) {
        toast.error(t("browse.bulkFailed"), {
          description: t("browse.bulkFailedDesc", { count: skipped.length }),
        });
      }
    },
    [modType, categoryId, startInstall, clearSelection, t],
  );

  const bulkInstall = useCallback(() => {
    const list = [...selected.values()];
    const already = list.filter(isInstalled);
    if (already.length) setReinstall({ kind: "bulk", mods: list });
    else void doBulkInstall(list);
  }, [selected, isInstalled, doBulkInstall]);

  const confirmReinstall = useCallback(() => {
    const pending = reinstall;
    setReinstall(null);
    if (!pending) return;
    if (pending.kind === "single") void doQuickInstall(pending.mod);
    else void doBulkInstall(pending.mods);
  }, [reinstall, doQuickInstall, doBulkInstall]);

  const selectAll = useCallback(() => {
    setSelected((prev) => {
      const next = new Map(prev);
      for (const m of mods) next.set(m.slug, m);
      return next;
    });
  }, [mods]);

  // Debounce the search input so we don't hammer the API on every keystroke.
  useEffect(() => {
    const t = setTimeout(() => setDebounced(query.trim()), 350);
    return () => clearTimeout(t);
  }, [query]);

  // (Re)load the first page whenever the query, category or sort changes.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setPage(1);
    askedForRatings.current = new Set();
    searchMods(debounced, categoryId, 1, activeSort)
      .then((res) => {
        if (cancelled) return;
        setMods(res);
        setHasMore(res.length >= SEARCH_PAGE_SIZE);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [debounced, categoryId, activeSort, reloadKey]);

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

  const isBike = modType.id === "bikes";

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none flex-col gap-4 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-3.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">
            {t("nav.browse")}
          </h1>
          <HelpHint
            title={t("nav.browse")}
            description={t("browse.help")}
          />
          <Segmented
            value={modType.id}
            onChange={(id) => {
              const next = modTypes.find((mt) => mt.id === id);
              if (next) onChangeType(next);
            }}
            options={modTypes.map((mt) => ({
              value: mt.id,
              label: t(mt.label),
            }))}
          />
          <div className="ml-auto flex w-[280px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
            <Search className="size-3.5 text-faint" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("browse.searchPlaceholder", {
                type: t(modType.labelInline),
              })}
              className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
            />
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {modType.categories.map((c) => {
            const on = c.id === categoryId;
            return (
              <button
                key={c.id}
                onClick={() => setCategoryId(c.id)}
                className={cn(
                  "cursor-default rounded-full px-3.5 py-[5px] text-[12px] font-medium transition-colors",
                  on
                    ? "bg-foreground font-semibold text-background"
                    : "border border-input text-muted-foreground hover:text-foreground",
                )}
              >
                {t(c.label)}
              </button>
            );
          })}
          <div className="ml-auto flex items-center gap-2 self-center">
            <ArrowUpDown className="size-3.5 text-faint" />
            <Select value={activeSort} onValueChange={(v) => setSort(v as ModSort)}>
              {/* Wide enough for the longest translated label ("Popolari questa
                  settimana") rather than the English one. */}
              <SelectTrigger className="h-8 w-[210px] bg-card">
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
          </div>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
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
            <Button variant="outline" size="sm" onClick={() => setReloadKey((k) => k + 1)}>
              {t("common.retry")}
            </Button>
          </div>
        ) : loading ? (
          <div className="grid grid-cols-4 gap-3.5">
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
            <div className="grid grid-cols-4 gap-3.5">
              {mods.map((m) => (
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
          <Button size="sm" onClick={bulkInstall} disabled={bulkBusy}>
            <Download className="size-3.5" />
            {bulkBusy
              ? t("browse.queuing")
              : t("browse.quickInstallCount", { count: selected.size })}
          </Button>
          <Button size="sm" variant="outline" onClick={selectAll} disabled={bulkBusy}>
            {t("common.selectAll")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={clearSelection}
            disabled={bulkBusy}
            className="ml-auto"
          >
            <X className="size-3.5" /> {t("common.clear")}
          </Button>
        </div>
      )}

      <AlertDialog open={Boolean(reinstall)} onOpenChange={(o) => !o && setReinstall(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {reinstall?.kind === "single"
                ? t("browse.reinstallOne", { title: reinstall.mod.title })
                : t("browse.reinstallMany")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {reinstall?.kind === "single"
                ? t("browse.reinstallOneBody")
                : t("browse.reinstallManyBody", {
                    installed: reinstall?.mods.filter(isInstalled).length ?? 0,
                    total: reinstall?.mods.length ?? 0,
                  })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={confirmReinstall}>
              {reinstall?.kind === "single"
                ? t("browse.reinstall")
                : t("browse.reinstallAll")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
