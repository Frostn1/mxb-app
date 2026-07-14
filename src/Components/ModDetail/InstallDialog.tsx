import { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Plus, Check, X } from "lucide-react";
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
  folderCounts,
  initialFolder,
  onConfirm,
}: InstallDialogProps) {
  const mirrors = useMirrors(detail);
  const [folder, setFolder] = useState(initialFolder);
  const [folderOpen, setFolderOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [newFolder, setNewFolder] = useState("");
  const [mirrorIdx, setMirrorIdx] = useState(0);
  const [mirrorsOpen, setMirrorsOpen] = useState(false);

  // Reset transient state each time the dialog opens.
  useEffect(() => {
    if (open) {
      setFolder(initialFolder);
      setFolderOpen(false);
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
              <div className="flex flex-col rounded-[10px] border border-input bg-popover p-1.5 shadow-[0_12px_32px_rgba(0,0,0,0.5)]">
                {destOptions.map((o) => {
                  const on = !creating && o.value === folder;
                  const count = folderCounts.get(o.value) ?? 0;
                  return (
                    <button
                      key={o.value}
                      onClick={() => {
                        setFolder(o.value);
                        setCreating(false);
                        setFolderOpen(false);
                      }}
                      className={cn(
                        "flex cursor-default items-center justify-between rounded-md px-3 py-2 text-[12.5px] transition-colors",
                        on
                          ? "bg-accent font-semibold text-accent-foreground"
                          : "text-foreground/90 hover:bg-foreground/[0.06]",
                      )}
                    >
                      <span className="truncate">{o.label}</span>
                      <span className="flex flex-none items-center gap-2 text-[11px] text-faint">
                        <span>{count} mods</span>
                        {on && <Check className="size-3.5 text-primary" />}
                      </span>
                    </button>
                  );
                })}
                <div className="mx-1.5 my-1 h-px bg-border" />
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
                    className="rounded-md bg-transparent px-3 py-2 text-[12.5px] text-foreground placeholder:text-faint focus:outline-none"
                  />
                ) : (
                  <button
                    onClick={() => {
                      setCreating(true);
                      setNewFolder("");
                    }}
                    className="flex cursor-default items-center gap-1.5 rounded-md px-3 py-2 text-[12.5px] font-semibold text-primary hover:bg-foreground/[0.06]"
                  >
                    <Plus className="size-3.5" /> New folder…
                  </button>
                )}
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
          <Button className="flex-1" onClick={confirm} disabled={!selectedMirror}>
            Install to {folderLabel}
          </Button>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
