import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import type { ComponentType } from "react";
import {
  Search,
  RefreshCw,
  MoreHorizontal,
  FolderInput,
  FolderOpen,
  Trash2,
  Plus,
  ChevronRight,
  Lock,
  Box,
  ListChecks,
  CheckCircle2,
  Circle,
  PackagePlus,
  FileArchive,
  Folder,
  Loader2,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import {
  installsOutsideMods,
  modTypesFor,
  scanLibrary,
  moveMod,
  revealInExplorer,
  uninstallMod,
  type ModType,
} from "../../api/mods";
import type { LibraryEntry, PkzMeta } from "../../types";
import { displayName, folderLabel, formatBytes, formatLength } from "../../lib/mods";
import { useT, type TFunc } from "../../i18n/context";
import { metaKey, peekMeta, primeMetaCache, requestMeta } from "../../lib/pkzMeta";
import {
  CATEGORY_LABEL,
  SECTION_LABEL,
  RIDER_SECTION_ORDER,
  categoryIcon,
} from "./categories";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import LibraryDetail from "./LibraryDetail";
import { ViewerDialog } from "../Viewer/ViewerDialog";
import { entryViewerProps } from "../Viewer/entryViewer";
import { useConfig } from "../../Context/Config";
import { useImport } from "../Dropzone/useImport";
import { Segmented } from "@/Components/ui/segmented";
import { Button } from "@/Components/ui/button";
import HelpHint from "@/Components/ui/help-hint";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/Components/ui/dropdown-menu";
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/Components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";
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

interface RowAction {
  key: string;
  icon: ComponentType<{ className?: string }>;
  label: string;
  onSelect: () => void;
  destructive?: boolean;
  separatorBefore?: boolean;
}

function LibraryCardBody({
  item,
  typeIcon: TypeIcon,
}: {
  item: LibraryEntry;
  typeIcon: LucideIcon;
}) {
  const t = useT();
  const cacheKey = metaKey(item);
  const [meta, setMeta] = useState<PkzMeta | null>(() => peekMeta(cacheKey) ?? null);
  // The thumbnail tile doubles as the visibility probe for the card.
  const [tile, setTile] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    const cached = peekMeta(cacheKey);
    if (cached) {
      setMeta(cached);
      return;
    }
    setMeta(null);
    if (!tile) return;

    // Reading metadata means opening the archive and decoding its preview, so only
    // the cards a player actually scrolls to are worth it. Asking for all of them the
    // moment the grid renders is what made a large library lock the machine up.
    let alive = true;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        io.disconnect();
        void requestMeta(item.path, cacheKey).then((m) => {
          if (alive && m) setMeta(m);
        });
      },
      // A little ahead of the viewport, so scrolling finds them already loaded.
      { rootMargin: "400px" },
    );
    io.observe(tile);
    return () => {
      alive = false;
      io.disconnect();
    };
  }, [item.path, cacheKey, tile]);

  const title = meta?.name?.trim() || displayName(item.name);
  const folder = item.name;
  const location = meta?.location?.trim();
  const parts: string[] = [];
  if (meta?.author) parts.push(t("library.byAuthor", { author: meta.author }));
  if (meta?.length) parts.push(formatLength(meta.length));
  if (item.size) parts.push(formatBytes(item.size));
  const categoryKey = CATEGORY_LABEL[item.category];
  const subtitle =
    parts.join(" · ") ||
    (categoryKey ? t(categoryKey) : folderLabel(item.folder));

  return (
    <>
      <div
        ref={setTile}
        className="relative grid h-12 w-[76px] flex-none place-items-center overflow-hidden rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/25"
      >
        {meta?.thumbnail ? (
          <img src={meta.thumbnail} alt="" className="h-full w-full object-cover" />
        ) : (
          <TypeIcon className="size-5" strokeWidth={1.5} />
        )}
        {meta?.locked && (
          <span
            className="absolute bottom-0.5 right-0.5 rounded bg-black/60 p-0.5 text-white/75"
            title={t("library.locked")}
          >
            <Lock className="size-3" />
          </span>
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        {/* The row truncates hard at this width, so the hover carries the full name and
            the folder it actually lives in — the id you need when matching paints. */}
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="w-fit max-w-full truncate text-[13px] font-semibold">{title}</span>
          </TooltipTrigger>
          <TooltipContent side="top" align="start" className="max-w-sm">
            <p className="font-semibold">{title}</p>
            {folder !== title && <p className="text-muted-foreground">{folder}</p>}
            {location && <p className="text-muted-foreground">{location}</p>}
          </TooltipContent>
        </Tooltip>
        <span className="truncate text-[11px] text-muted-foreground" title={subtitle}>
          {subtitle}
        </span>
      </div>
    </>
  );
}

interface Section {
  key: string;
  label: string;
  items: LibraryEntry[];
}

function buildSections(
  modType: ModType,
  entries: LibraryEntry[],
  search: string,
  t: TFunc,
): Section[] {
  const q = search.trim().toLowerCase();
  const filtered = q
    ? entries.filter(
        (e) =>
          e.name.toLowerCase().includes(q) ||
          e.folder.toLowerCase().includes(q) ||
          (e.parent ?? "").toLowerCase().includes(q),
      )
    : entries;

  if (modType.id === "rider") {
    const byCat = new Map<string, LibraryEntry[]>();
    for (const e of filtered) {
      const list = byCat.get(e.category) ?? [];
      list.push(e);
      byCat.set(e.category, list);
    }
    const order = RIDER_SECTION_ORDER as string[];
    return [...byCat.keys()]
      .sort((a, b) => {
        const ia = order.indexOf(a);
        const ib = order.indexOf(b);
        return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib);
      })
      .map((cat) => ({
        key: cat,
        label: SECTION_LABEL[cat] ? t(SECTION_LABEL[cat]) : cat,
        items: byCat.get(cat)!,
      }));
  }

  const shown =
    modType.id === "bikes"
      ? filtered.filter((e) => e.category !== "bikePaint" && e.category !== "bikeModelSwap")
      : filtered;
  const byFolder = new Map<string, LibraryEntry[]>();
  for (const e of shown) {
    const list = byFolder.get(e.folder) ?? [];
    list.push(e);
    byFolder.set(e.folder, list);
  }
  return [...byFolder.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([folder, items]) => ({ key: folder || "__root__", label: folderLabel(folder), items }));
}

interface LibraryProps {
  modType: ModType;
  onChangeType: (type: ModType) => void;
  refreshKey: number;
  onChanged: () => void;
}

export default function Library({
  modType,
  onChangeType,
  refreshKey,
  onChanged,
}: LibraryProps) {
  const t = useT();
  const { pickAndImport, staging } = useImport();
  const [entries, setEntries] = useState<LibraryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [detail, setDetail] = useState<LibraryEntry | null>(null);
  const [view3d, setView3d] = useState<LibraryEntry | null>(null);
  const [moveTarget, setMoveTarget] = useState<LibraryEntry | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<LibraryEntry | null>(null);
  // Multi-select: a "Select" mode turns cards into checkboxes for bulk actions.
  const [selectMode, setSelectMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkMoveOpen, setBulkMoveOpen] = useState(false);
  const [bulkUninstallOpen, setBulkUninstallOpen] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const scanned = await scanLibrary(modType.installSubpath);
      // Pull everything already known in one round trip, so a library that's been
      // opened before renders complete without touching a single archive.
      await primeMetaCache(scanned);
      setEntries(scanned);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [modType]);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  useEffect(() => setDetail(null), [modType]);
  // Leaving a type (or a rescan removing items) should never carry a stale selection.
  const exitSelect = useCallback(() => {
    setSelectMode(false);
    setSelected(new Set());
  }, []);
  useEffect(() => exitSelect(), [modType, exitSelect]);

  const allFolders = useMemo(
    () => [...new Set(entries.map((e) => e.folder))].sort((a, b) => a.localeCompare(b)),
    [entries],
  );

  const sections = useMemo(
    () => buildSections(modType, entries, search, t),
    [modType, entries, search, t],
  );

  const visibleItems = useMemo(() => sections.flatMap((s) => s.items), [sections]);
  const visibleCount = visibleItems.length;

  const toggleSelect = useCallback((path: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const allSelected =
    visibleItems.length > 0 && visibleItems.every((i) => selected.has(i.path));
  const selectAllOrNone = () =>
    setSelected(allSelected ? new Set() : new Set(visibleItems.map((i) => i.path)));

  const selectedEntries = useMemo(
    () => entries.filter((e) => selected.has(e.path)),
    [entries, selected],
  );
  // Move only applies to packaged `.pkz` items (folders stay put).
  const selectedMovable = useMemo(
    () => selectedEntries.filter((e) => e.kind === "pkz"),
    [selectedEntries],
  );

  const { bikePreview, game } = useConfig();
  // The Library is a view of the mods tree, so the one type that installs outside it —
  // ReShade presets, which live in the game's install folder — has no tab here. They're
  // managed in Settings, where their install status can be shown alongside them.
  const modTypes = modTypesFor(game.id).filter((mt) => !installsOutsideMods(mt));
  const view3dProps = view3d ? entryViewerProps(view3d, entries, bikePreview) : null;

  const doMove = async (item: LibraryEntry, toFolder: string) => {
    setBusy(true);
    setMoveTarget(null);
    try {
      await moveMod(item.path, toFolder, modType.installSubpath);
      await load();
      onChanged();
    } catch (e) {
      toast.error(t("library.moveFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const doUninstall = async (item: LibraryEntry) => {
    setBusy(true);
    setUninstallTarget(null);
    try {
      await uninstallMod(item.path, modType.installSubpath);
      if (detail?.path === item.path) setDetail(null);
      await load();
      onChanged();
      toast.success(t("library.uninstalledOne", { name: displayName(item.name) }), {
        description: t("library.movedToBin"),
      });
    } catch (e) {
      toast.error(t("library.uninstallFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const doBulkUninstall = async () => {
    setBusy(true);
    setBulkUninstallOpen(false);
    const targets = selectedEntries;
    let ok = 0;
    let fail = 0;
    for (const item of targets) {
      try {
        await uninstallMod(item.path, modType.installSubpath);
        ok++;
      } catch {
        fail++;
      }
    }
    if (detail && selected.has(detail.path)) setDetail(null);
    await load();
    onChanged();
    exitSelect();
    if (fail)
      toast.error(t("library.bulkUninstallPartial", { ok, fail }), {
        description: t("library.someNotRemoved"),
      });
    else
      toast.success(t("library.bulkUninstalled", { count: ok }), {
        description: t("library.movedToBin"),
      });
    setBusy(false);
  };

  const doBulkMove = async (toFolder: string) => {
    setBusy(true);
    setBulkMoveOpen(false);
    const targets = selectedMovable;
    let ok = 0;
    let fail = 0;
    for (const item of targets) {
      try {
        await moveMod(item.path, toFolder, modType.installSubpath);
        ok++;
      } catch {
        fail++;
      }
    }
    await load();
    onChanged();
    exitSelect();
    if (fail) toast.error(t("library.bulkMovePartial", { ok, fail }));
    else
      toast.success(
        t("library.bulkMoved", { count: ok, folder: folderLabel(toFolder) }),
      );
    setBusy(false);
  };

  const reveal = (item: LibraryEntry) =>
    revealInExplorer(item.path).catch((e) =>
      toast.error(t("library.openFailed"), { description: String(e) }),
    );

  const rowActions = (item: LibraryEntry): RowAction[] => [
    ...(item.kind === "pkz"
      ? [
          {
            key: "move",
            icon: FolderInput,
            label: t("library.moveToFolder"),
            onSelect: () => setMoveTarget(item),
          },
        ]
      : []),
    {
      key: "reveal",
      icon: FolderOpen,
      label: t("library.showInExplorer"),
      onSelect: () => reveal(item),
    },
    {
      key: "uninstall",
      icon: Trash2,
      label: t("library.uninstallAction"),
      destructive: true,
      separatorBefore: true,
      onSelect: () => setUninstallTarget(item),
    },
  ];

  return (
    <div className="flex h-full flex-col">
      {detail ? (
        <LibraryDetail
          entry={detail}
          entries={entries}
          modType={modType}
          onClose={() => setDetail(null)}
          onReveal={reveal}
          onUninstall={setUninstallTarget}
          onMove={setMoveTarget}
          onOpenEntry={setDetail}
        />
      ) : (
        <>
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">
            {t("nav.library")}
          </h1>
          <HelpHint title={t("nav.library")} description={t("library.help")} />
        </div>
        <Segmented
          value={modType.id}
          onChange={(id) => {
            const next = modTypes.find((mt) => mt.id === id);
            if (next) onChangeType(next);
          }}
          options={modTypes.map((mt) => ({
            value: mt.id,
            label: (
              <span className="flex items-center gap-1.5">
                {t(mt.label)}
                {mt.id === modType.id && (
                  <span className="text-muted-foreground">{visibleCount}</span>
                )}
              </span>
            ),
          }))}
        />
        <div className="ml-auto flex w-[240px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
          <Search className="size-3.5 text-faint" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("library.searchPlaceholder")}
            className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
          />
        </div>
        <Button
          variant={selectMode ? "default" : "outline"}
          size="sm"
          onClick={() => (selectMode ? exitSelect() : setSelectMode(true))}
          disabled={loading}
        >
          <ListChecks className="size-3.5" />{" "}
          {selectMode ? t("common.done") : t("common.select")}
        </Button>
        {/* The same install flow as dropping, for anyone the OS drop event never reaches —
            and a discoverable one for anyone who never thought to drag a file here. */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" disabled={staging}>
              {staging ? (
                <Loader2 className="size-3.5 animate-spin" />
              ) : (
                <PackagePlus className="size-3.5" />
              )}{" "}
              {staging ? t("import.staging") : t("import.action")}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={() => void pickAndImport("files")}>
              <FileArchive className="size-3.5" />
              {t("import.pickFiles")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void pickAndImport("folder")}>
              <Folder className="size-3.5" />
              {t("import.pickFolder")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button variant="outline" size="sm" onClick={load} disabled={loading || busy}>
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />{" "}
          {t("locker.rescan")}
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <p className="select-text py-16 text-center text-[13px] text-destructive">
            {error}
          </p>
        ) : loading ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {t("library.scanning")}
          </p>
        ) : sections.length === 0 ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {entries.length === 0
              ? t("library.empty", { type: t(modType.labelInline) })
              : t("library.noMatches")}
          </p>
        ) : (
          <div className="flex flex-col gap-6">
            {sections.map((section) => (
              <section key={section.key} className="flex flex-col gap-2.5">
                <div className="flex items-baseline gap-2">
                  <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
                    ▸ {section.label}
                  </span>
                  <span className="text-[11px] text-faint">{section.items.length}</span>
                </div>
                <div className="grid grid-cols-3 gap-3">
                  {section.items.map((item) => {
                    const actions = rowActions(item);
                    const Icon = categoryIcon(item.category);
                    const canView3d = entryViewerProps(item, entries, bikePreview) !== null;
                    const isSel = selected.has(item.path);
                    return (
                      <ContextMenu key={item.path}>
                        <ContextMenuTrigger asChild>
                          <div
                            role="button"
                            tabIndex={0}
                            onClick={() =>
                              selectMode ? toggleSelect(item.path) : setDetail(item)
                            }
                            onKeyDown={(e) =>
                              e.key === "Enter" &&
                              (selectMode ? toggleSelect(item.path) : setDetail(item))
                            }
                            className={cn(
                              "flex cursor-pointer items-center gap-3 rounded-xl border bg-card p-3 transition-colors",
                              isSel
                                ? "border-primary/60 bg-primary/[0.06]"
                                : "border-white/[0.07] hover:border-white/15",
                            )}
                          >
                            {selectMode && (
                              <span className="flex-none">
                                {isSel ? (
                                  <CheckCircle2 className="size-5 text-primary" />
                                ) : (
                                  <Circle className="size-5 text-faint" />
                                )}
                              </span>
                            )}
                            <LibraryCardBody item={item} typeIcon={Icon} />
                            {!selectMode && canView3d && (
                              <button
                                title={t("library.quick3d")}
                                aria-label={t("library.quick3d")}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setView3d(item);
                                }}
                                className="flex flex-none cursor-default items-center gap-1 whitespace-nowrap rounded-md px-1.5 py-1 text-[11px] font-semibold text-faint transition-colors hover:bg-foreground/[0.06] hover:text-primary"
                              >
                                <Box className="size-3.5" /> {t("library.quick3d")}
                              </button>
                            )}
                            {!selectMode && (
                              <DropdownMenu>
                                <DropdownMenuTrigger asChild>
                                  <button
                                    disabled={busy}
                                    onClick={(e) => e.stopPropagation()}
                                    className="flex-none cursor-default rounded-md px-1 text-faint transition-colors hover:text-foreground"
                                  >
                                    <MoreHorizontal className="size-4" />
                                  </button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="end">
                                  {actions.map((a) => (
                                    <Fragment key={a.key}>
                                      {a.separatorBefore && <DropdownMenuSeparator />}
                                      <DropdownMenuItem
                                        variant={a.destructive ? "destructive" : "default"}
                                        onSelect={a.onSelect}
                                      >
                                        <a.icon className="size-4" /> {a.label}
                                      </DropdownMenuItem>
                                    </Fragment>
                                  ))}
                                </DropdownMenuContent>
                              </DropdownMenu>
                            )}
                          </div>
                        </ContextMenuTrigger>
                        <ContextMenuContent>
                          {actions.map((a) => (
                            <Fragment key={a.key}>
                              {a.separatorBefore && <ContextMenuSeparator />}
                              <ContextMenuItem
                                variant={a.destructive ? "destructive" : "default"}
                                onSelect={a.onSelect}
                              >
                                <a.icon className="size-4" /> {a.label}
                              </ContextMenuItem>
                            </Fragment>
                          ))}
                        </ContextMenuContent>
                      </ContextMenu>
                    );
                  })}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>

      {selectMode && (
        <div className="flex flex-none items-center gap-3 border-t border-input bg-window/95 px-7 py-3">
          <button
            onClick={selectAllOrNone}
            disabled={visibleCount === 0}
            className="cursor-default text-[12px] font-semibold text-primary transition-colors hover:brightness-110 disabled:opacity-40"
          >
            {allSelected ? t("library.selectNone") : t("common.selectAll")}
          </button>
          <span className="text-[12.5px] text-muted-foreground">
            {t("browse.selectedCount", { count: selected.size })}
          </span>
          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={selectedMovable.length === 0 || busy}
              onClick={() => setBulkMoveOpen(true)}
            >
              <FolderInput className="size-3.5" /> {t("library.move")}
              {selectedMovable.length > 0 ? ` (${selectedMovable.length})` : ""}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={selected.size === 0 || busy}
              onClick={() => setBulkUninstallOpen(true)}
            >
              <Trash2 className="size-3.5" /> {t("library.uninstall")}
            </Button>
          </div>
        </div>
      )}
        </>
      )}

      <ViewerDialog
        open={Boolean(view3d)}
        onOpenChange={(o) => !o && setView3d(null)}
        title={view3d ? displayName(view3d.name) : undefined}
        initialMode={view3dProps?.mode}
        paintPaths={view3dProps?.paintPaths ?? []}
        modelSource={view3dProps?.modelSource}
        gearSource={view3dProps?.gearSource}
        gearPart={view3dProps?.gearPart}
        stockGearPart={view3dProps?.stockGearPart}
        initialPaint={view3dProps?.initialPaint}
        initialGoggles={view3dProps?.initialGoggles}
      />

      <MoveDialog
        open={Boolean(moveTarget)}
        title={t("library.moveDialogTitle")}
        subtitle={moveTarget ? displayName(moveTarget.name) : undefined}
        folders={allFolders}
        modType={modType}
        excludeFolder={moveTarget?.folder}
        onClose={() => setMoveTarget(null)}
        onPick={(f) => moveTarget && doMove(moveTarget, f)}
      />

      <MoveDialog
        open={bulkMoveOpen}
        title={t("library.moveCount", { count: selectedMovable.length })}
        subtitle={t("library.chooseDestination")}
        folders={allFolders}
        modType={modType}
        onClose={() => setBulkMoveOpen(false)}
        onPick={(f) => doBulkMove(f)}
      />

      <AlertDialog
        open={bulkUninstallOpen}
        onOpenChange={(o) => !o && setBulkUninstallOpen(false)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("library.confirmBulkUninstall", { count: selected.size })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("library.confirmBulkUninstallBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={doBulkUninstall}>
              {t("library.uninstallCount", { count: selected.size })}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={Boolean(uninstallTarget)}
        onOpenChange={(o) => !o && setUninstallTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("library.confirmUninstall", {
                name: uninstallTarget ? displayName(uninstallTarget.name) : "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("library.confirmUninstallBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => uninstallTarget && doUninstall(uninstallTarget)}
            >
              {t("library.uninstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

/**
 * Folder picker used for both single-item "Move to folder…" and bulk moves.
 * `excludeFolder` hides the item's current folder (single move only); `onPick`
 * receives the chosen (or newly typed) destination folder.
 */
function MoveDialog({
  open,
  title,
  subtitle,
  folders,
  modType,
  excludeFolder,
  onClose,
  onPick,
}: {
  open: boolean;
  title: string;
  subtitle?: string;
  folders: string[];
  modType: ModType;
  excludeFolder?: string;
  onClose: () => void;
  onPick: (folder: string) => void;
}) {
  const t = useT();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");

  useEffect(() => {
    if (open) {
      setCreating(false);
      setName("");
    }
  }, [open]);

  const options = ["", ...folders.filter((f) => f !== "")];

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle && <DialogDescription>{subtitle}</DialogDescription>}
        </DialogHeader>
        <div className="flex max-h-64 flex-col overflow-y-auto rounded-lg border border-input p-1.5">
          {options
            .filter((f) => f !== excludeFolder)
            .map((f) => (
              <button
                key={f || "__root__"}
                onClick={() => onPick(f)}
                className="flex cursor-default items-center gap-2 rounded-md px-3 py-2 text-left text-[12.5px] text-foreground/90 transition-colors hover:bg-foreground/[0.06]"
              >
                <ChevronRight className="size-3.5 text-faint" />
                {folderLabel(f)}
              </button>
            ))}
          <div className="mx-1.5 my-1 h-px bg-border" />
          {creating ? (
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && name.trim()) onPick(name.trim());
                if (e.key === "Escape") setCreating(false);
              }}
              placeholder={
                modType.id === "bikes"
                  ? "KTM450/paints"
                  : t("library.newFolderName")
              }
              className="rounded-md bg-transparent px-3 py-2 text-[12.5px] placeholder:text-faint focus:outline-none"
            />
          ) : (
            <button
              onClick={() => setCreating(true)}
              className="flex cursor-default items-center gap-1.5 rounded-md px-3 py-2 text-[12.5px] font-semibold text-primary hover:bg-foreground/[0.06]"
            >
              <Plus className="size-3.5" /> {t("library.newFolder")}
            </button>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          {creating && (
            <Button
              size="sm"
              disabled={!name.trim()}
              onClick={() => name.trim() && onPick(name.trim())}
            >
              {t("library.createAndMove")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
