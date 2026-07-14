import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import type { ComponentType } from "react";
import {
  Search,
  RefreshCw,
  MoreHorizontal,
  Mountain,
  Bike,
  FolderInput,
  FolderOpen,
  Trash2,
  Plus,
  ChevronRight,
} from "lucide-react";
import { toast } from "sonner";
import {
  MOD_TYPES,
  getInstalledMods,
  moveMod,
  revealInExplorer,
  uninstallMod,
  type ModType,
} from "../../api/mods";
import type { InstalledMod } from "../../types";
import { displayName, folderLabel } from "../../lib/mods";
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

interface LibraryProps {
  modType: ModType;
  onChangeType: (type: ModType) => void;
  refreshKey: number;
  /** Bump the dashboard's install version after a change (uninstall/move). */
  onChanged: () => void;
}

export default function Library({
  modType,
  onChangeType,
  refreshKey,
  onChanged,
}: LibraryProps) {
  const [items, setItems] = useState<InstalledMod[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [moveTarget, setMoveTarget] = useState<InstalledMod | null>(null);
  const [uninstallTarget, setUninstallTarget] = useState<InstalledMod | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setItems(await getInstalledMods(modType.installSubpath));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [modType]);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  const allFolders = useMemo(
    () => [...new Set(items.map((i) => i.folder))].sort((a, b) => a.localeCompare(b)),
    [items],
  );

  const groups = useMemo(() => {
    const q = search.trim().toLowerCase();
    const filtered = q
      ? items.filter(
          (i) =>
            i.name.toLowerCase().includes(q) || i.folder.toLowerCase().includes(q),
        )
      : items;
    const map = new Map<string, InstalledMod[]>();
    for (const it of filtered) {
      const list = map.get(it.folder) ?? [];
      list.push(it);
      map.set(it.folder, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [items, search]);

  const doMove = async (item: InstalledMod, toFolder: string) => {
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

  const doUninstall = async (item: InstalledMod) => {
    setBusy(true);
    setUninstallTarget(null);
    try {
      await uninstallMod(item.path, modType.installSubpath);
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

  // Single source of truth for a row's actions — rendered in both the 3-dot
  // dropdown and the right-click context menu so they can't drift apart.
  const rowActions = (item: InstalledMod): RowAction[] => [
    {
      key: "move",
      icon: FolderInput,
      label: "Move to folder…",
      onSelect: () => setMoveTarget(item),
    },
    {
      key: "reveal",
      icon: FolderOpen,
      label: "Show in Explorer",
      onSelect: () =>
        revealInExplorer(item.path).catch((e) =>
          toast.error("Couldn't open", { description: String(e) }),
        ),
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

  const TypeIcon = modType.id === "bikes" ? Bike : Mountain;

  return (
    <div className="flex h-full flex-col">
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
                  <span className="text-muted-foreground">{items.length}</span>
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
        ) : groups.length === 0 ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {items.length === 0
              ? `No ${modType.label.toLowerCase()} installed yet — head to Browse and add one.`
              : "No matches."}
          </p>
        ) : (
          <div className="flex flex-col gap-6">
            {groups.map(([folder, mods]) => (
              <section key={folder} className="flex flex-col gap-2.5">
                <div className="flex items-baseline gap-2">
                  <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
                    ▸ {folderLabel(folder)}
                  </span>
                  <span className="text-[11px] text-faint">{mods.length}</span>
                </div>
                <div className="grid grid-cols-3 gap-3">
                  {mods.map((item) => {
                    const actions = rowActions(item);
                    return (
                      <ContextMenu key={item.path}>
                        <ContextMenuTrigger asChild>
                          <div className="flex items-center gap-3 rounded-xl border border-white/[0.07] bg-card p-3 transition-colors hover:border-white/15">
                            <div className="grid h-12 w-[76px] flex-none place-items-center rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/25">
                              <TypeIcon className="size-5" strokeWidth={1.5} />
                            </div>
                            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                              <span
                                className="truncate text-[13px] font-semibold"
                                title={item.name}
                              >
                                {displayName(item.name)}
                              </span>
                              <span className="truncate text-[11px] text-muted-foreground">
                                {folderLabel(item.folder)}
                              </span>
                            </div>
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <button
                                  disabled={busy}
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
              The file is moved to the Recycle Bin — you can restore it from there.
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
  target: InstalledMod | null;
  folders: string[];
  modType: ModType;
  onClose: () => void;
  onMove: (item: InstalledMod, folder: string) => void;
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
