import { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Plus, Check, X, Search } from "lucide-react";
import { Dialog, DialogContent } from "@/Components/ui/dialog";
import { Button } from "@/Components/ui/button";
import { Badge } from "@/Components/ui/badge";
import { cn } from "@/lib/utils";
import {
  isBlockedDownload,
  sortMirrors,
  type DestOption,
  type ModType,
} from "../../api/mods";
import type { DownloadOption, ModDetail as Detail } from "../../types";

export interface InstallChoice {
  destFolder: string;
  mirror: DownloadOption;
}

interface InstallDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  detail: Detail;
  modType: ModType;
  destOptions: DestOption[];
  /** Ranked "probable" destination values (best first) — e.g. the matched bike. */
  suggestions: string[];
  /** folder value → number of mods currently installed there. */
  folderCounts: Map<string, number>;
  /** Preselected destination (remembered per category). */
  initialFolder: string;
  onConfirm: (choice: InstallChoice) => void;
}

/** Ordered, de-duped playable mirrors with the "official" default first. */
function useMirrors(detail: Detail): DownloadOption[] {
  return useMemo(() => sortMirrors(detail), [detail]);
}

export default function InstallDialog({
  open,
  onOpenChange,
  detail,
  modType,
  destOptions,
  suggestions,
  folderCounts,
  initialFolder,
  onConfirm,
}: InstallDialogProps) {
  const mirrors = useMirrors(detail);
  const [folder, setFolder] = useState(initialFolder);
  const [folderOpen, setFolderOpen] = useState(false);
  const [folderSearch, setFolderSearch] = useState("");
  const [creating, setCreating] = useState(false);
  const [newFolder, setNewFolder] = useState("");
  const [mirrorIdx, setMirrorIdx] = useState(0);
  const [mirrorsOpen, setMirrorsOpen] = useState(false);

  // Reset transient state each time the dialog opens.
  useEffect(() => {
    if (open) {
      setFolder(initialFolder);
      setFolderOpen(false);
      setFolderSearch("");
      setCreating(false);
      setNewFolder("");
      setMirrorIdx(0);
      setMirrorsOpen(false);
    }
  }, [open, initialFolder]);

  const folderLabel = useMemo(() => {
    if (creating && newFolder.trim()) return newFolder.trim();
    return destOptions.find((o) => o.value === folder)?.label ?? folder ?? "(root)";
  }, [creating, newFolder, destOptions, folder]);

  // Probable destinations (ranked) resolved to options, best first.
  const suggestedOptions = useMemo(() => {
    const byValue = new Map(destOptions.map((o) => [o.value, o]));
    return suggestions
      .map((v) => byValue.get(v))
      .filter((o): o is DestOption => Boolean(o));
  }, [suggestions, destOptions]);

  // Command-style filter over every destination.
  const filteredOptions = useMemo(() => {
    const q = folderSearch.trim().toLowerCase();
    if (!q) return destOptions;
    return destOptions.filter((o) => o.label.toLowerCase().includes(q));
  }, [folderSearch, destOptions]);

  const suggestedValues = useMemo(
    () => new Set(suggestedOptions.map((o) => o.value)),
    [suggestedOptions],
  );

  const selectedMirror = mirrors[mirrorIdx];
  const thumb = detail.images[0];
  const subtitleType = modType.id === "bikes" ? "Bike" : "Track";

  const commitNewFolder = () => {
    const v = newFolder.trim();
    if (!v) return;
    setFolder(v);
    setCreating(false);
    setFolderOpen(false);
  };

  const chooseFolder = (value: string) => {
    setFolder(value);
    setCreating(false);
    setFolderOpen(false);
    setFolderSearch("");
  };

  const renderRow = (o: DestOption) => {
    const on = !creating && o.value === folder;
    const count = folderCounts.get(o.value) ?? 0;
    return (
      <button
        key={o.value || "__root__"}
        onClick={() => chooseFolder(o.value)}
        className={cn(
          "flex w-full cursor-default items-center justify-between gap-2 rounded-md px-3 py-2 text-[12.5px] transition-colors",
          on
            ? "bg-accent font-semibold text-accent-foreground"
            : "text-foreground/90 hover:bg-foreground/[0.06]",
        )}
      >
        <span className="min-w-0 flex-1 truncate text-left">{o.label}</span>
        <span className="flex flex-none items-center gap-2 text-[11px] text-faint">
          <span>{count} mods</span>
          {on && <Check className="size-3.5 text-primary" />}
        </span>
      </button>
    );
  };

  const confirm = () => {
    if (!selectedMirror) return;
    const destFolder = creating && newFolder.trim() ? newFolder.trim() : folder;
    onConfirm({ destFolder, mirror: selectedMirror });
  };

  const shownMirrors = mirrorsOpen ? mirrors : mirrors.slice(0, 1);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showClose={false}
        className="max-w-[460px] gap-0 overflow-hidden rounded-2xl p-0"
      >
        {/* header */}
        <div className="flex items-center gap-3 border-b border-white/[0.07] px-[18px] pb-3.5 pt-4">
          <div
            className="h-[34px] w-[52px] flex-none rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a] bg-cover bg-center"
            style={thumb ? { backgroundImage: `url(${thumb})` } : undefined}
          />
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[14px] font-bold">{detail.title}</span>
            <span className="text-[11.5px] text-muted-foreground">
              {subtitleType}
              {detail.version ? ` · ${detail.version}` : ""}
            </span>
          </div>
          <button
            onClick={() => onOpenChange(false)}
            className="cursor-default text-faint transition-colors hover:text-foreground"
          >
            <X className="size-4" />
          </button>
        </div>

        {/* body */}
        <div className="flex flex-col gap-4 px-[18px] py-4">
          {/* destination */}
          <section className="flex flex-col gap-2">
            <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
              Install to
            </span>
            <button
              onClick={() => setFolderOpen((v) => !v)}
              className="flex cursor-default items-center gap-2.5 rounded-[9px] border border-input bg-background px-3 py-2.5"
            >
              <ChevronRight className="size-3.5 flex-none text-primary" />
              <span className="flex-1 truncate text-left font-mono text-[12px] text-muted-foreground">
                {modType.installSubpath.replace(/\//g, "\\")}\
                <b className="text-foreground">{folderLabel}</b>
              </span>
              <span className="flex flex-none items-center gap-1 text-[11px] text-muted-foreground">
                Change <ChevronDown className="size-3" />
              </span>
            </button>

            {folderOpen && (
              <div className="flex flex-col overflow-hidden rounded-[10px] border border-input bg-popover shadow-[0_12px_32px_rgba(0,0,0,0.5)]">
                {/* command-style search */}
                <div className="flex items-center gap-2 border-b border-border px-3 py-2">
                  <Search className="size-3.5 flex-none text-faint" />
                  <input
                    autoFocus
                    value={folderSearch}
                    onChange={(e) => setFolderSearch(e.target.value)}
                    placeholder={
                      modType.id === "bikes" ? "Search bikes…" : "Search folders…"
                    }
                    className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
                  />
                </div>

                {/* scrollable results */}
                <div className="max-h-[240px] overflow-y-auto p-1.5">
                  {!folderSearch && suggestedOptions.length > 0 && (
                    <>
                      <div className="px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-faint">
                        Probably
                      </div>
                      {suggestedOptions.map(renderRow)}
                      <div className="mx-1.5 my-1 h-px bg-border" />
                      <div className="px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-faint">
                        All folders
                      </div>
                    </>
                  )}
                  {(folderSearch
                    ? filteredOptions
                    : destOptions.filter((o) => !suggestedValues.has(o.value))
                  ).map(renderRow)}
                  {folderSearch && filteredOptions.length === 0 && (
                    <div className="px-3 py-4 text-center text-[12px] text-muted-foreground">
                      No folder matches — create it below.
                    </div>
                  )}
                </div>

                {/* new folder, pinned */}
                <div className="border-t border-border p-1.5">
                  {creating ? (
                    <input
                      autoFocus
                      value={newFolder}
                      onChange={(e) => setNewFolder(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") commitNewFolder();
                        if (e.key === "Escape") setCreating(false);
                      }}
                      onBlur={commitNewFolder}
                      placeholder={
                        modType.id === "bikes" ? "KTM450/paints" : "New folder name"
                      }
                      className="w-full rounded-md bg-transparent px-3 py-2 text-[12.5px] text-foreground placeholder:text-faint focus:outline-none"
                    />
                  ) : (
                    <button
                      onClick={() => {
                        setCreating(true);
                        setNewFolder(folderSearch);
                      }}
                      className="flex w-full cursor-default items-center gap-1.5 rounded-md px-3 py-2 text-[12.5px] font-semibold text-primary hover:bg-foreground/[0.06]"
                    >
                      <Plus className="size-3.5" /> New folder…
                    </button>
                  )}
                </div>
              </div>
            )}
            <span className="text-[11px] text-faint">
              Remembered for {modType.label}
            </span>
          </section>

          {/* mirrors */}
          {mirrors.length > 0 && (
            <section className="flex flex-col gap-2">
              <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
                Download from
              </span>
              <div className="flex flex-col gap-1.5">
                {shownMirrors.map((m) => {
                  const idx = mirrors.indexOf(m);
                  const on = idx === mirrorIdx;
                  const blocked = isBlockedDownload(m);
                  return (
                    <button
                      key={`${m.url}-${idx}`}
                      onClick={() => setMirrorIdx(idx)}
                      className={cn(
                        "flex cursor-default items-center gap-[11px] rounded-[9px] border bg-background px-3 py-2.5 text-left transition-colors",
                        on ? "border-primary/50" : "border-input hover:border-white/20",
                      )}
                    >
                      <span
                        className={cn(
                          "size-[15px] flex-none rounded-full",
                          on
                            ? "border-4 border-primary"
                            : "border-[1.5px] border-foreground/25",
                        )}
                      />
                      <span className="flex flex-1 flex-col">
                        <span className="text-[12.5px] font-semibold">{m.host}</span>
                        <span className="text-[11px] text-muted-foreground">
                          {blocked
                            ? "Opens in browser — MXB App finishes the install"
                            : m.isDefault
                              ? "Direct · fastest"
                              : "Direct"}
                        </span>
                      </span>
                      {blocked ? (
                        <Badge variant="warning" className="flex-none">
                          Browser
                        </Badge>
                      ) : m.isDefault ? (
                        <Badge variant="success" className="flex-none border-primary/35 text-primary">
                          Default
                        </Badge>
                      ) : null}
                    </button>
                  );
                })}
                {!mirrorsOpen && mirrors.length > 1 && (
                  <button
                    onClick={() => setMirrorsOpen(true)}
                    className="flex cursor-default items-center gap-1 self-start px-1 text-[11px] text-muted-foreground hover:text-foreground"
                  >
                    {mirrors.length - 1} more mirror{mirrors.length - 1 > 1 ? "s" : ""}
                    <ChevronDown className="size-3" />
                  </button>
                )}
              </div>
              {mirrors.length > 1 && (
                <span className="text-[11px] text-faint">
                  All mirrors contain the same file. If one fails, try the next.
                </span>
              )}
            </section>
          )}
        </div>

        {/* footer */}
        <div className="flex gap-2.5 border-t border-white/[0.07] px-[18px] py-3.5">
          <Button
            className="min-w-0 flex-1"
            onClick={confirm}
            disabled={!selectedMirror}
          >
            <span className="truncate">Install to {folderLabel}</span>
          </Button>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
