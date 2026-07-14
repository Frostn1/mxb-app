import { useCallback, useEffect, useState } from "react";
import { Search, Download, X } from "lucide-react";
import { toast } from "sonner";
import {
  MOD_TYPES,
  SEARCH_PAGE_SIZE,
  normalizeModName,
  resolveQuickInstall,
  searchMods,
  type ModType,
} from "../../api/mods";
import type { ModSummary } from "../../types";
import { useInstall } from "../../Context/Install";
import ModCard from "./ModCard";
import { Segmented } from "@/Components/ui/segmented";
import { Button } from "@/Components/ui/button";
import { Skeleton } from "@/Components/ui/skeleton";
import { cn } from "@/lib/utils";

interface BrowseProps {
  modType: ModType;
  installedNames: Set<string>;
  onOpenMod: (slug: string) => void;
  onChangeType: (type: ModType) => void;
}

export default function Browse({
  modType,
  installedNames,
  onOpenMod,
  onChangeType,
}: BrowseProps) {
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [categoryId, setCategoryId] = useState(modType.categoryId);
  const [mods, setMods] = useState<ModSummary[]>([]);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [selected, setSelected] = useState<Map<string, ModSummary>>(new Map());
  const [bulkBusy, setBulkBusy] = useState(false);

  const { startInstall } = useInstall();
  const selectionActive = selected.size > 0;

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
  const quickInstall = useCallback(
    async (mod: ModSummary) => {
      try {
        const res = await resolveQuickInstall(mod.slug, modType);
        if (res.ok) {
          startInstall(res.params);
          toast.success(`Queued “${res.params.title}”`, {
            description: `Installing to ${res.params.destFolder || "root"}.`,
          });
        } else if (res.reason === "blocked") {
          toast.error(`“${res.title}” needs a browser download`, {
            description: `${res.host} blocks in-app downloads — open its page to finish.`,
          });
        } else {
          toast.error(`No download found for “${res.title}”`);
        }
      } catch (e) {
        toast.error(`Couldn't quick-install “${mod.title}”`, {
          description: String(e),
        });
      }
    },
    [modType, startInstall],
  );

  const bulkInstall = useCallback(async () => {
    const list = [...selected.values()];
    setBulkBusy(true);
    let queued = 0;
    const skipped: string[] = [];
    for (const mod of list) {
      try {
        const res = await resolveQuickInstall(mod.slug, modType);
        if (res.ok) {
          startInstall(res.params);
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
      toast.success(`Queued ${queued} mod${queued > 1 ? "s" : ""}`, {
        description: skipped.length
          ? `${skipped.length} skipped — browser-only host.`
          : "They'll install one after another.",
      });
    } else if (skipped.length) {
      toast.error("Couldn't quick-install the selection", {
        description: `All ${skipped.length} need a browser download.`,
      });
    }
  }, [selected, modType, startInstall, clearSelection]);

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

  // (Re)load the first page whenever the query or category changes.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setPage(1);
    searchMods(debounced, categoryId, 1)
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
  }, [debounced, categoryId, reloadKey]);

  const loadMore = useCallback(async () => {
    const next = page + 1;
    setLoadingMore(true);
    try {
      const res = await searchMods(debounced, categoryId, next);
      setMods((prev) => [...prev, ...res]);
      setHasMore(res.length >= SEARCH_PAGE_SIZE);
      setPage(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  }, [debounced, categoryId, page]);

  const isBike = modType.id === "bikes";

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none flex-col gap-4 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-3.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">Browse</h1>
          <Segmented
            value={modType.id}
            onChange={(id) => {
              const next = MOD_TYPES.find((t) => t.id === id);
              if (next) onChangeType(next);
            }}
            options={MOD_TYPES.map((t) => ({ value: t.id, label: t.label }))}
          />
          <div className="ml-auto flex w-[280px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
            <Search className="size-3.5 text-faint" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={`Search ${modType.label.toLowerCase()}…`}
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
                {c.label}
              </button>
            );
          })}
          <span className="ml-auto self-center text-[11.5px] text-faint">
            Sorted by newest
          </span>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <div className="flex flex-col items-center gap-3 py-20 text-center">
            <p className="text-[13px] text-destructive">
              Couldn&apos;t load mods: {error}
            </p>
            <Button variant="outline" size="sm" onClick={() => setReloadKey((k) => k + 1)}>
              Retry
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
            No {modType.label.toLowerCase()} found.
          </p>
        ) : (
          <>
            <div className="grid grid-cols-4 gap-3.5">
              {mods.map((m) => (
                <ModCard
                  key={m.id}
                  mod={m}
                  isBike={isBike}
                  installed={installedNames.has(normalizeModName(m.title))}
                  selected={selected.has(m.slug)}
                  selectionActive={selectionActive}
                  onOpen={() => onOpenMod(m.slug)}
                  onToggleSelect={() => toggleSelect(m)}
                  onQuickInstall={() => quickInstall(m)}
                />
              ))}
            </div>
            {hasMore && (
              <div className="flex justify-center pt-4">
                <Button variant="outline" onClick={loadMore} disabled={loadingMore}>
                  {loadingMore ? "Loading…" : "Load more"}
                </Button>
              </div>
            )}
          </>
        )}
      </div>

      {selectionActive && (
        <div className="flex flex-none items-center gap-3 border-t border-white/[0.08] bg-window px-7 py-3">
          <span className="text-[12.5px] font-semibold">
            {selected.size} selected
          </span>
          <Button size="sm" onClick={bulkInstall} disabled={bulkBusy}>
            <Download className="size-3.5" />
            {bulkBusy ? "Queuing…" : `Quick install ${selected.size}`}
          </Button>
          <Button size="sm" variant="outline" onClick={selectAll} disabled={bulkBusy}>
            Select all
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={clearSelection}
            disabled={bulkBusy}
            className="ml-auto"
          >
            <X className="size-3.5" /> Clear
          </Button>
        </div>
      )}
    </div>
  );
}
