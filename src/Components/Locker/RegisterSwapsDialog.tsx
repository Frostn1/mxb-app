import { useState } from "react";
import { Bike, Volume2, FolderInput, FolderPlus, Loader2 } from "lucide-react";
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
import type { LooseSwapBike, LooseSwapCandidate } from "../../types";

/** "2 model swaps and 1 sound mod" — omits a kind with zero, "sets" if somehow both are 0. */
function summarize(models: number, sounds: number): string {
  const parts: string[] = [];
  if (models) parts.push(`${models} model swap${models === 1 ? "" : "s"}`);
  if (sounds) parts.push(`${sounds} sound mod${sounds === 1 ? "" : "s"}`);
  return parts.join(" and ") || "0 sets";
}

/**
 * Prompts the user to *register* model- and sound-set folders found loose in their bike
 * dirs — either moving each into its library (`<Bike>/FrostMod Models/` for models,
 * `<Bike>/FrostMod Sounds/` for sounds, so the Locker picks them up) or just creating
 * those library folders and leaving the files put. Shared by the launch prompt (App) and
 * the Locker banner.
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

  const all = bikes.flatMap((b) => b.candidates);
  const models = all.filter((c) => c.kind === "model").length;
  const sounds = all.filter((c) => c.kind === "sound").length;

  const run = async (move: boolean) => {
    setBusy(move ? "move" : "folders");
    try {
      const r = await registerLooseSwaps(move);
      if (move) {
        toast.success(
          r.registered > 0
            ? `Registered ${r.registered} set${r.registered === 1 ? "" : "s"}.`
            : "Nothing was moved.",
          r.skipped > 0
            ? { description: `${r.skipped} skipped (name already in use).` }
            : undefined,
        );
      } else {
        toast.success(
          `Created the library folder${r.foldersCreated === 1 ? "" : "s"} for ${r.bikes} bike${r.bikes === 1 ? "" : "s"}.`,
          { description: "Your model / sound folders were left where they are." },
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
          <DialogTitle>Found {summarize(models, sounds)}</DialogTitle>
          <DialogDescription>
            These folders are sitting loose inside your bikes. Register them to move each
            into the right library —{" "}
            <span className="font-mono text-faint">FrostMod Models</span> for models,{" "}
            <span className="font-mono text-faint">FrostMod Sounds</span> for sounds — so
            they show up in the Locker.
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
                  <Chip key={`${c.kind}:${c.source}`} candidate={c} />
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
            <Button size="sm" disabled={!!busy} onClick={() => void run(true)}>
              {busy === "move" ? <Loader2 className="animate-spin" /> : <FolderInput />}
              Register &amp; move
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** One candidate: a model (bike icon) or sound (speaker icon) set, name + file count. */
function Chip({ candidate: c }: { candidate: LooseSwapCandidate }) {
  const Icon = c.kind === "sound" ? Volume2 : Bike;
  return (
    <span
      title={`${c.source} · ${c.kind}`}
      className="flex items-center gap-1.5 rounded-md bg-foreground/[0.06] px-2 py-0.5 text-[11px] text-foreground/80"
    >
      <Icon className="size-3 text-foreground/40" strokeWidth={1.5} />
      {c.name}
      <span className="text-faint">
        {c.fileCount} file{c.fileCount === 1 ? "" : "s"}
      </span>
    </span>
  );
}
