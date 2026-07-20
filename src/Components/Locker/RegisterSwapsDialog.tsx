import { useState } from "react";
import { Bike, FolderInput, FolderPlus, Loader2 } from "lucide-react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";
import { Button } from "@/Components/ui/button";
import { registerLooseSwaps } from "../../api/mods";
import type { LooseSwapBike } from "../../types";

/**
 * Prompts the user to *register* model-set folders found loose in their bike dirs —
 * either moving each into `<Bike>/FrostMod Models/` (so the Model Swaps page picks them
 * up) or just creating the `FrostMod Models/` folder and leaving the files put. Shared by
 * the launch prompt (App) and the Model Swaps banner (Locker).
 */
export default function RegisterSwapsDialog({
  open,
  onOpenChange,
  bikes,
  onDone,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  bikes: LooseSwapBike[];
  /** Called after a successful register so callers can refresh their view. */
  onDone?: () => void;
}) {
  // Which action is running ("move" | "folders"), so we can spin the right button.
  const [busy, setBusy] = useState<"move" | "folders" | null>(null);

  const total = bikes.reduce((n, b) => n + b.candidates.length, 0);

  const run = async (move: boolean) => {
    setBusy(move ? "move" : "folders");
    try {
      const r = await registerLooseSwaps(move);
      if (move) {
        toast.success(
          r.registered > 0
            ? `Registered ${r.registered} model swap${r.registered === 1 ? "" : "s"}.`
            : "No model swaps were moved.",
          r.skipped > 0
            ? { description: `${r.skipped} skipped (name already in use).` }
            : undefined,
        );
      } else {
        toast.success(
          `Created the FrostMod Models folder for ${r.bikes} bike${r.bikes === 1 ? "" : "s"}.`,
          { description: "Your model folders were left where they are." },
        );
      }
      onOpenChange(false);
      onDone?.();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !busy && onOpenChange(o)}>
      <DialogContent className="max-w-lg" showClose={!busy}>
        <DialogHeader>
          <DialogTitle>Found {total} model swap{total === 1 ? "" : "s"}</DialogTitle>
          <DialogDescription>
            These model folders are sitting loose inside your bikes. Register them to move
            each into the bike’s{" "}
            <span className="font-mono text-faint">FrostMod Models</span> folder so they
            show up under Model Swaps.
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-64 overflow-y-auto rounded-lg border border-white/[0.07] bg-black/20">
          {bikes.map((b) => (
            <div
              key={b.bike}
              className="border-b border-white/[0.05] px-3.5 py-2.5 last:border-b-0"
            >
              <div className="flex items-center gap-2 text-[12.5px] font-semibold">
                <Bike className="size-3.5 text-foreground/40" strokeWidth={1.5} />
                <span className="truncate">{b.bike}</span>
              </div>
              <div className="mt-1.5 flex flex-wrap gap-1.5 pl-[22px]">
                {b.candidates.map((c) => (
                  <span
                    key={c.source}
                    title={c.source}
                    className="rounded-md bg-foreground/[0.06] px-2 py-0.5 text-[11px] text-foreground/80"
                  >
                    {c.name}
                    <span className="ml-1 text-faint">
                      {c.fileCount} file{c.fileCount === 1 ? "" : "s"}
                    </span>
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>

        <DialogFooter className="flex-col-reverse gap-2 sm:flex-row sm:justify-between">
          <Button
            variant="ghost"
            size="sm"
            disabled={!!busy}
            onClick={() => onOpenChange(false)}
          >
            Later
          </Button>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Button
              variant="outline"
              size="sm"
              disabled={!!busy}
              onClick={() => void run(false)}
            >
              {busy === "folders" ? (
                <Loader2 className="animate-spin" />
              ) : (
                <FolderPlus />
              )}
              Just create folders
            </Button>
            <Button
              size="sm"
              disabled={!!busy}
              onClick={() => void run(true)}
            >
              {busy === "move" ? <Loader2 className="animate-spin" /> : <FolderInput />}
              Register &amp; move
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
