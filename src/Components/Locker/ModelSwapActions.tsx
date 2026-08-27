import { useEffect, useState } from "react";
import { FolderInput, MoreHorizontal, Trash2 } from "lucide-react";
import { toast } from "sonner";
import {
  bikeFolders,
  deleteModelSwap,
  modelSwapLiveries,
  moveModelSwap,
} from "../../api/mods";
import type { ModelVariant } from "../../types";
import { useT } from "../../i18n/context";
import { cn } from "@/lib/utils";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/Components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";
import { Button } from "@/Components/ui/button";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/Components/ui/select";

/**
 * Move / delete for one model set, shared by the Locker and the Library so the two can never
 * disagree about what a variant offers.
 *
 * Both actions are refused for the **active** model: its files are loose at the bike root
 * rather than in its folder, so moving or deleting the folder would take the bike's live model
 * out from under it. Stock isn't a folder at all. The backend refuses both again — this only
 * decides what to grey out.
 */
export function ModelSwapActions({
  bike,
  variant,
  onChanged,
  className,
}: {
  bike: string;
  variant: ModelVariant;
  /** Something moved or went to the Trash — rescan. */
  onChanged: () => void;
  className?: string;
}) {
  const t = useT();
  const [moveOpen, setMoveOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const isStock = variant.name.toLowerCase() === "stock";
  const locked = variant.active || isStock;
  const why = variant.active
    ? t("swapActions.activeFirst")
    : isStock
      ? t("swapActions.stockHasNoFiles")
      : undefined;

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            onClick={(e) => e.stopPropagation()}
            title={why ?? t("swapActions.menu")}
            className={cn(
              "flex-none cursor-default rounded-md px-1 text-faint transition-colors hover:text-foreground",
              className,
            )}
          >
            <MoreHorizontal className="size-4" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
          <DropdownMenuItem disabled={locked} onSelect={() => setMoveOpen(true)}>
            <FolderInput className="size-4" /> {t("swapActions.move")}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            disabled={locked}
            onSelect={() => setDeleteOpen(true)}
          >
            <Trash2 className="size-4" /> {t("swapActions.delete")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <MoveDialog
        open={moveOpen}
        onOpenChange={setMoveOpen}
        bike={bike}
        variant={variant.name}
        onDone={onChanged}
      />
      <DeleteDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        bike={bike}
        variant={variant}
        onDone={onChanged}
      />
    </>
  );
}

function MoveDialog({
  open,
  onOpenChange,
  bike,
  variant,
  onDone,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  bike: string;
  variant: string;
  onDone: () => void;
}) {
  const t = useT();
  const [bikes, setBikes] = useState<string[]>([]);
  const [target, setTarget] = useState("");
  const [liveries, setLiveries] = useState<string[]>([]);
  // Which liveries travel. Empty by default on purpose: a `.pnt` is cut for one bike's UV
  // layout, so carrying one is a deliberate choice, not the safe default.
  const [carry, setCarry] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setTarget("");
    setCarry(new Set());
    void bikeFolders()
      .then((all) => setBikes(all.filter((b) => b.toLowerCase() !== bike.toLowerCase())))
      .catch(() => setBikes([]));
    void modelSwapLiveries(bike, variant).then(setLiveries).catch(() => setLiveries([]));
  }, [open, bike, variant]);

  const go = async () => {
    setBusy(true);
    try {
      await moveModelSwap(bike, variant, target, [...carry]);
      toast.success(t("swapActions.moved", { name: variant, bike: target }));
      onOpenChange(false);
      onDone();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("swapActions.moveTitle", { name: variant })}</DialogTitle>
          <DialogDescription>{t("swapActions.moveBlurb")}</DialogDescription>
        </DialogHeader>

        <Select value={target} onValueChange={setTarget}>
          <SelectTrigger>
            <SelectValue placeholder={t("swapActions.pickBike")} />
          </SelectTrigger>
          <SelectContent>
            {bikes.map((b) => (
              <SelectItem key={b} value={b}>
                {b}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {liveries.length > 0 && (
          <div className="flex flex-col gap-1.5">
            <p className="text-[12px] font-semibold">{t("swapActions.liveriesTitle")}</p>
            {/* Off by default, and said plainly: a paint drawn for one bike rarely fits
                another, and leaving it behind loses nothing. */}
            <p className="text-[11px] text-muted-foreground">
              {t("swapActions.liveriesBlurb")}
            </p>
            <div className="mt-1 flex max-h-40 flex-col gap-1 overflow-y-auto">
              {liveries.map((l) => (
                <label
                  key={l}
                  className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-[12px] hover:bg-foreground/[0.04]"
                >
                  <input
                    type="checkbox"
                    checked={carry.has(l)}
                    onChange={() =>
                      setCarry((prev) => {
                        const next = new Set(prev);
                        if (!next.delete(l)) next.add(l);
                        return next;
                      })
                    }
                  />
                  <span className="truncate">{l}</span>
                </label>
              ))}
            </div>
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button disabled={!target || busy} onClick={() => void go()}>
            {t("swapActions.moveConfirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DeleteDialog({
  open,
  onOpenChange,
  bike,
  variant,
  onDone,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  bike: string;
  variant: ModelVariant;
  onDone: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);

  const go = async () => {
    setBusy(true);
    try {
      await deleteModelSwap(bike, variant.name);
      toast.success(t("swapActions.deleted", { name: variant.name }));
      onOpenChange(false);
      onDone();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("swapActions.deleteTitle", { name: variant.name })}</DialogTitle>
          {/* Says where it goes and what survives — a model set can be hundreds of MB the
              player may not be able to download again. */}
          <DialogDescription>
            {t("swapActions.deleteBlurb", { count: variant.fileCount })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button variant="destructive" disabled={busy} onClick={() => void go()}>
            {t("swapActions.deleteConfirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
