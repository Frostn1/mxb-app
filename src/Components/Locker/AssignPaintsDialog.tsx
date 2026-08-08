import { useEffect, useMemo, useState } from "react";
import { Check, Loader2, Palette } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/Components/ui/dialog";
import { Button } from "@/Components/ui/button";
import { listBikeLiveries, setModelPaints } from "../../api/mods";
import type { BikeModels, PaintAssignOutcome } from "../../types";

/**
 * Assigns a bike's liveries to one model swap.
 *
 * The game gives a bike a single flat `paints/` folder and knows nothing about model
 * swaps, so every livery for every model shows up at once — most of them drawn for a mesh
 * that isn't on the bike. Ticking a livery here says it belongs to this model: it's the
 * only model that offers it, and while another model is active the file is moved out of
 * `paints/` so the game doesn't list it either. A livery left unticked by every model
 * belongs to none and stays on offer under all of them.
 */
export default function AssignPaintsDialog({
  open,
  onOpenChange,
  bike,
  model,
  models,
  onDone,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  bike: string;
  /** The variant being edited. */
  model: string;
  /** The bike's full scan — used to show which other model already claims a livery. */
  models: BikeModels;
  /** Called after a successful save so the Locker can refresh. */
  onDone?: () => void;
}) {
  const [liveries, setLiveries] = useState<string[] | null>(null);
  // Seeded once at mount — the caller keys this dialog by bike + model, so opening it for
  // another model remounts rather than reseeding mid-edit when a rescan lands.
  const [picked, setPicked] = useState<Set<string>>(
    () => new Set(models.variants.find((v) => v.name === model)?.paints ?? []),
  );
  const [saving, setSaving] = useState(false);

  // Which *other* model claims each livery, so ticking one shows what it's taken from.
  const claimedBy = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const v of models.variants) {
      if (v.name === model) continue;
      for (const p of v.paints) m.set(p.toLowerCase(), [...(m.get(p.toLowerCase()) ?? []), v.name]);
    }
    return m;
  }, [models, model]);

  useEffect(() => {
    let alive = true;
    listBikeLiveries(bike)
      .then((l) => alive && setLiveries(l))
      .catch((e) => {
        if (!alive) return;
        toast.error(String(e).replace(/^Error:\s*/, ""));
        setLiveries([]);
      });
    return () => {
      alive = false;
    };
  }, [bike]);

  const toggle = (name: string) =>
    setPicked((prev) => {
      const next = new Set(prev);
      if (!next.delete(name)) next.add(name);
      return next;
    });

  const save = async () => {
    setSaving(true);
    try {
      const r = await setModelPaints(bike, model, [...picked]);
      toast.success(summary(picked.size, model), { description: note(r) });
      onOpenChange(false);
      onDone?.();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !saving && onOpenChange(o)}>
      <DialogContent className="max-w-lg" showClose={!saving}>
        <DialogHeader>
          <DialogTitle>Liveries for “{model}”</DialogTitle>
          <DialogDescription>
            Tick the liveries drawn for this model. They&apos;re the only ones offered
            while it&apos;s active — and the ones belonging to another model are moved out
            of <span className="font-mono text-faint">paints/</span>, so MX Bikes stops
            listing them too. Leave a livery unticked everywhere and it stays available
            under every model.
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-72 min-h-24 overflow-y-auto rounded-lg border border-white/[0.07] bg-black/20">
          {liveries === null ? (
            <p className="py-8 text-center text-[12.5px] text-muted-foreground">
              Reading liveries…
            </p>
          ) : liveries.length === 0 ? (
            <p className="px-4 py-8 text-center text-[12.5px] text-muted-foreground">
              This bike has no liveries yet — install one and it&apos;ll show up here.
            </p>
          ) : (
            liveries.map((name) => {
              const on = picked.has(name);
              const others = claimedBy.get(name.toLowerCase()) ?? [];
              return (
                <button
                  key={name}
                  onClick={() => toggle(name)}
                  disabled={saving}
                  className={cn(
                    "flex w-full items-center gap-2.5 border-b border-white/[0.05] px-3.5 py-2 text-left transition-colors last:border-b-0",
                    on ? "bg-primary/[0.07]" : "hover:bg-white/[0.03]",
                    saving && "pointer-events-none opacity-60",
                  )}
                >
                  <span
                    className={cn(
                      "grid size-4 flex-none place-items-center rounded border",
                      on ? "border-primary bg-primary/20" : "border-white/15",
                    )}
                  >
                    {on && <Check className="size-3 text-primary" />}
                  </span>
                  <Palette className="size-3.5 flex-none text-foreground/30" strokeWidth={1.5} />
                  <span className="min-w-0 flex-1 truncate text-[12.5px]">{name}</span>
                  {others.length > 0 && (
                    <span
                      className="flex-none text-[10.5px] text-faint"
                      title={`Also assigned to ${others.join(", ")}`}
                    >
                      {others.join(", ")}
                    </span>
                  )}
                </button>
              );
            })
          )}
        </div>

        <DialogFooter className="flex-col-reverse gap-2 sm:flex-row sm:justify-between">
          <Button variant="ghost" size="sm" disabled={saving} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button size="sm" disabled={saving || liveries === null} onClick={() => void save()}>
            {saving && <Loader2 className="animate-spin" />}
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function summary(count: number, model: string): string {
  return count === 0
    ? `“${model}” no longer claims any livery.`
    : `${count} liver${count === 1 ? "y" : "ies"} assigned to “${model}”.`;
}

/** What the shelving actually managed — MX Bikes holds these files open while it runs. */
function note(r: PaintAssignOutcome): string {
  if (r.stuck > 0) {
    return `${r.stuck} livery file${r.stuck === 1 ? "" : "s"} couldn't be moved — close MX Bikes and hit Rescan, or they stay visible in-game.`;
  }
  return r.game_running
    ? "Reselect your profile in MX Bikes to see the new list."
    : "The game will show the new list next time it opens.";
}
