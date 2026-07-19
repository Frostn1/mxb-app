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
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import {
  MOD_TYPES,
  scanLibrary,
  getPkzMeta,
  moveMod,
  revealInExplorer,
  uninstallMod,
  type ModType,
} from "../../api/mods";
import type { LibraryEntry, PkzMeta } from "../../types";
import { displayName, folderLabel, formatBytes, formatLength } from "../../lib/mods";
import {
  CATEGORY_LABEL,
  SECTION_LABEL,
  RIDER_SECTION_ORDER,
  categoryIcon,
} from "./categories";
import LibraryDetail from "./LibraryDetail";
import { ViewerDialog } from "../Viewer/ViewerDialog";
import { entryViewerProps } from "../Viewer/entryViewer";
import { Segmented } from "@/Components/ui/segmented";
import { Button } from "@/Components/ui/button";
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

const metaCache = new Map<string, PkzMeta>();

function LibraryCardBody({
  item,
  typeIcon: TypeIcon,
}: {
  item: LibraryEntry;
  typeIcon: LucideIcon;
}) {
  const cacheKey = `${item.path}:${item.size}`;
  const [meta, setMeta] = useState<PkzMeta | null>(
    () => metaCache.get(cacheKey) ?? null,
  );

  useEffect(() => {
    const cached = metaCache.get(cacheKey);
    if (cached) {
      setMeta(cached);
      return;
    }
    let alive = true;
    setMeta(null);
    getPkzMeta(item.path)
      .then((m) => {
        metaCache.set(cacheKey, m);
        if (alive) setMeta(m);
      })
      .catch(() => {
        /* leave the icon/size fallback in place */
      });
    return () => {
      alive = false;
    };
  }, [item.path, cacheKey]);

  const title = meta?.name?.trim() || displayName(item.name);
  const parts: string[] = [];
  if (meta?.author) parts.push(`by ${meta.author}`);
  if (meta?.length) parts.push(formatLength(meta.length));
  if (item.size) parts.push(formatBytes(item.size));
  const subtitle = parts.join(" · ") || CATEGORY_LABEL[item.category] || folderLabel(item.folder);

  return (
    <>
      <div className="relative grid h-12 w-[76px] flex-none place-items-center overflow-hidden rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/25">
        {meta?.thumbnail ? (
          <img src={meta.thumbnail} alt="" className="h-full w-full object-cover" />
        ) : (
          <TypeIcon className="size-5" strokeWidth={1.5} />
        )}
        {meta?.locked && (
          <span
            className="absolute bottom-0.5 right-0.5 rounded bg-black/60 p-0.5 text-white/75"
            title="Locked — contents can't be read"
          >
            <Lock className="size-3" />
          </span>
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span
          className="truncate text-[13px] font-semibold"
          title={meta?.location?.trim() || item.name}
        >
          {title}
        </span>
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
        label: SECTION_LABEL[cat] ?? cat,
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
  const [entries, setEntries] = useState<LibraryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [detail, setDetail] = useState<LibraryEntry | null>(null);
  const [view3d, setView3d] = useState<LibraryEntry | null>(null);
  const [moveTarget, setMoveTarget] = useState<LibraryEntry | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<LibraryEntry | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setEntries(await scanLibrary(modType.installSubpath));
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

  const allFolders = useMemo(
    () => [...new Set(entries.map((e) => e.folder))].sort((a, b) => a.localeCompare(b)),
    [entries],
  );

  const sections = useMemo(
    () => buildSections(modType, entries, search),
    [modType, entries, search],
  );

  const visibleCount = useMemo(
    () => sections.reduce((n, s) => n + s.items.length, 0),
    [sections],
  );

  const view3dProps = view3d ? entryViewerProps(view3d, entries) : null;

  const doMove = async (item: LibraryEntry, toFolder: string) => {
    setBusy(true);
    setMoveTarget(null);
    try {
      await moveMod(item.path, toFolder, modType.installSubpath);
      await load();
      onChanged();
    } catch (e) {
      toast.error("Couldn't move mod", { description: String(e) });
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
      toast.success(`${displayName(item.name)} uninstalled`, {
        description: "Moved to the Recycle Bin.",
      });
    } catch (e) {
      toast.error("Couldn't uninstall", { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const reveal = (item: LibraryEntry) =>
    revealInExplorer(item.path).catch((e) =>
      toast.error("Couldn't open", { description: String(e) }),
    );

  const rowActions = (item: LibraryEntry): RowAction[] => [
    ...(item.kind === "pkz"
      ? [
          {
            key: "move",
            icon: FolderInput,
            label: "Move to folder…",
            onSelect: () => setMoveTarget(item),
          },
        ]
      : []),
    {
      key: "reveal",
      icon: FolderOpen,
      label: "Show in Explorer",
      onSelect: () => reveal(item),
    },
    {
      key: "uninstall",
      icon: Trash2,
      label: "Uninstall…",
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
        <h1 className="text-[21px] font-bold tracking-[-0.2px]">Library</h1>
        <Segmented
          value={modType.id}
          onChange={(id) => {
            const next = MOD_TYPES.find((t) => t.id === id);
            if (next) onChangeType(next);
          }}
          options={MOD_TYPES.map((t) => ({
            value: t.id,
            label: (
              <span className="flex items-center gap-1.5">
                {t.label}
                {t.id === modType.id && (
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
            placeholder="Search installed…"
            className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
          />
        </div>
        <Button variant="outline" size="sm" onClick={load} disabled={loading || busy}>
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} /> Rescan
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <p className="select-text py-16 text-center text-[13px] text-destructive">
            {error}
          </p>
        ) : loading ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            Scanning your library…
          </p>
        ) : sections.length === 0 ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {entries.length === 0
              ? `No ${modType.label.toLowerCase()} installed yet — head to Browse and add one.`
              : "No matches."}
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
                    const canView3d = entryViewerProps(item, entries) !== null;
                    return (
                      <ContextMenu key={item.path}>
                        <ContextMenuTrigger asChild>
                          <div
                            role="button"
                            tabIndex={0}
                            onClick={() => setDetail(item)}
                            onKeyDown={(e) => e.key === "Enter" && setDetail(item)}
                            className="flex cursor-pointer items-center gap-3 rounded-xl border border-white/[0.07] bg-card p-3 transition-colors hover:border-white/15"
                          >
                            <LibraryCardBody item={item} typeIcon={Icon} />
                            {canView3d && (
                              <button
                                title="Quick 3D view"
                                aria-label="Quick 3D view"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setView3d(item);
                                }}
                                className="flex-none cursor-default rounded-md p-1 text-faint transition-colors hover:bg-foreground/[0.06] hover:text-primary"
                              >
                                <Box className="size-4" />
                              </button>
                            )}
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
      />

      <MoveDialog
        target={moveTarget}
        folders={allFolders}
        modType={modType}
        onClose={() => setMoveTarget(null)}
        onMove={doMove}
      />

      <AlertDialog
        open={Boolean(uninstallTarget)}
        onOpenChange={(o) => !o && setUninstallTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Uninstall {uninstallTarget && displayName(uninstallTarget.name)}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              The item is moved to the Recycle Bin — you can restore it from there.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => uninstallTarget && doUninstall(uninstallTarget)}
            >
              Uninstall
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function MoveDialog({
  target,
  folders,
  modType,
  onClose,
  onMove,
}: {
  target: LibraryEntry | null;
  folders: string[];
  modType: ModType;
  onClose: () => void;
  onMove: (item: LibraryEntry, folder: string) => void;
}) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");

  useEffect(() => {
    if (target) {
      setCreating(false);
      setName("");
    }
  }, [target]);

  const options = ["", ...folders.filter((f) => f !== "")];

  return (
    <Dialog open={Boolean(target)} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Move to folder</DialogTitle>
          <DialogDescription>
            {target && displayName(target.name)}
          </DialogDescription>
        </DialogHeader>
        <div className="flex max-h-64 flex-col overflow-y-auto rounded-lg border border-input p-1.5">
          {options
            .filter((f) => f !== target?.folder)
            .map((f) => (
              <button
                key={f || "__root__"}
                onClick={() => target && onMove(target, f)}
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
                if (e.key === "Enter" && name.trim() && target)
                  onMove(target, name.trim());
                if (e.key === "Escape") setCreating(false);
              }}
              placeholder={
                modType.id === "bikes" ? "KTM450/paints" : "New folder name"
              }
              className="rounded-md bg-transparent px-3 py-2 text-[12.5px] placeholder:text-faint focus:outline-none"
            />
          ) : (
            <button
              onClick={() => setCreating(true)}
              className="flex cursor-default items-center gap-1.5 rounded-md px-3 py-2 text-[12.5px] font-semibold text-primary hover:bg-foreground/[0.06]"
            >
              <Plus className="size-3.5" /> New folder…
            </button>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>
            Cancel
          </Button>
          {creating && (
            <Button
              size="sm"
              disabled={!name.trim()}
              onClick={() => target && name.trim() && onMove(target, name.trim())}
            >
              Create &amp; move
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
