import { useEffect, useMemo, useState } from "react";
import { Check, Loader2, Palette, Search } from "lucide-react";
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
import { Input } from "@/Components/ui/input";
import { useT } from "@/i18n/context";
import { listBikeLiveries, setModelPaints } from "../../api/mods";
import type { BikeModels, SwapApplyOutcome } from "../../types";

/**
 * Assigns a bike's liveries to one model swap.
 *
 * The game gives a bike a single flat `paints/` folder and knows nothing about model
 * swaps, so every livery for every model shows up at once — most of them drawn for a mesh
 * that isn't on the bike. Ticking a livery here says it belongs to this model: it's the
 * only model that offers it, and while another model is active the file is moved out of
 * `paints/` so the game doesn't list it either. A livery left unticked by every model
 * belongs to none and stays on offer under all of them.
 *
 * A livery may be ticked under more than one model. Ownership is a record rather than a
 * folder, so that costs no second copy of the file.
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
  const t = useT();
  const [liveries, setLiveries] = useState<string[] | null>(null);
  // Seeded once at mount — the caller keys this dialog by bike + model, so opening it for
  // another model remounts rather than reseeding mid-edit when a rescan lands.
  const [picked, setPicked] = useState<Set<string>>(
    () => new Set(models.variants.find((v) => v.name === model)?.paints ?? []),
  );
  const [filter, setFilter] = useState("");
  const [saving, setSaving] = useState(false);

  // Which *other* model claims each livery, so ticking one shows what it shares with.
  const claimedBy = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const v of models.variants) {
      if (v.name === model) continue;
      for (const p of v.paints) {
        const k = p.toLowerCase();
        m.set(k, [...(m.get(k) ?? []), v.name]);
      }
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

  // A bike can carry thirty liveries, and the whole complaint is about long lists — so
  // the picker that fixes it can't be a long list you scroll twice.
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (liveries ?? []).filter((l) => !q || l.toLowerCase().includes(q));
  }, [liveries, filter]);

  const allShownPicked = shown.length > 0 && shown.every((l) => picked.has(l));

  const toggle = (name: string) =>
    setPicked((prev) => {
      const next = new Set(prev);
      if (!next.delete(name)) next.add(name);
      return next;
    });

  /** Tick or clear everything the filter is currently showing, not the whole bike. */
  const toggleShown = () =>
    setPicked((prev) => {
      const next = new Set(prev);
      for (const l of shown) {
        if (allShownPicked) next.delete(l);
        else next.add(l);
      }
      return next;
    });

  const save = async () => {
    setSaving(true);
    try {
      const r = await setModelPaints(bike, model, [...picked]);
      toast.success(t("locker.paintsSaved", { count: picked.size, model }), {
        description: note(r, t),
      });
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
          <DialogTitle>{t("locker.paintsTitle", { model })}</DialogTitle>
          <DialogDescription>{t("locker.paintsBlurb")}</DialogDescription>
        </DialogHeader>

        {liveries !== null && liveries.length > 0 && (
          <div className="flex items-center gap-2">
            <div className="relative min-w-0 flex-1">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-foreground/30" />
              <Input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder={t("locker.paintsFilter")}
                disabled={saving}
                className="h-8 pl-8 text-[12.5px]"
              />
            </div>
            <Button
              variant="ghost"
              size="sm"
              disabled={saving || shown.length === 0}
              onClick={toggleShown}
            >
              {allShownPicked ? t("locker.paintsClearAll") : t("locker.paintsSelectAll")}
            </Button>
          </div>
        )}

        <div className="max-h-72 min-h-24 overflow-y-auto rounded-lg border border-white/[0.07] bg-black/20">
          {liveries === null ? (
            <p className="py-8 text-center text-[12.5px] text-muted-foreground">
              {t("locker.paintsLoading")}
            </p>
          ) : liveries.length === 0 ? (
            <p className="px-4 py-8 text-center text-[12.5px] text-muted-foreground">
              {t("locker.paintsNone")}
            </p>
          ) : shown.length === 0 ? (
            <p className="px-4 py-8 text-center text-[12.5px] text-muted-foreground">
              {t("locker.paintsNoMatch")}
            </p>
          ) : (
            shown.map((name) => {
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
                      title={t("locker.paintsAlsoOn", { models: others.join(", ") })}
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
            {t("common.cancel")}
          </Button>
          <Button size="sm" disabled={saving || liveries === null} onClick={() => void save()}>
            {saving && <Loader2 className="animate-spin" />}
            {t("common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** What the shelving actually managed — MX Bikes holds these files open while it runs. */
function note(r: SwapApplyOutcome, t: ReturnType<typeof useT>): string {
  if (r.paints_stuck > 0) return t("locker.paintsStuck", { count: r.paints_stuck });
  return r.game_running ? t("locker.paintsReselect") : t("locker.paintsNextLaunch");
}
