import { useEffect, useMemo, useState } from "react";
import { Box, Loader2, X } from "lucide-react";
import { Dialog, DialogClose, DialogContent } from "../ui/dialog";
import { ModelViewer, type ViewerMode } from "./ModelViewer";
import {
  unpackPaint,
  loadBikeModel,
  previewModelSwap,
  loadRiderBodyModel,
  loadGearModel,
  loadStockGearModel,
  listGearPaints,
} from "../../api/mods";
import type { PaintTexture, BikeModel, EdfNode, RiderPart, GearPaints } from "../../types";
import { useT } from "../../i18n/context";

interface ViewerDialogProps {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title?: string;
  initialMode?: ViewerMode;
  paintPaths?: string[];
  modelSource?: string;
  /** A bike shown as one of its model swaps would leave it, rather than as it is on disk.
   *  Named by bike + variant because the swap is resolved backend-side, not by path. */
  modelSwap?: { bike: string; variant: string };
  gearSource?: string;
  gearPart?: RiderPart["part"];
  stockGearPart?: RiderPart["part"];
  /** Open on this paint / goggle paint rather than the first in the list — set when a
   *  paint was opened in 3D and the model shown is the one it belongs to. */
  initialPaint?: string;
  initialGoggles?: string;
}

const EMPTY_GEAR_PAINTS: GearPaints = {
  paints: [],
  goggles: [],
  hasStock: false,
  hasStockGoggles: false,
};

function paintLabel(path: string): string {
  const base = path.replace(/\\/g, "/").split("/").pop() ?? path;
  return base.replace(/\.pnt$/i, "");
}

/** The model's own look leads the picker, ahead of the paints it packs. */
function withStock(names: string[], hasStock: boolean): string[] {
  return hasStock ? ["Stock", ...names] : names;
}

/** Where `want` sits in a picker's list, or the first entry when it isn't in it — a paint
 *  the model doesn't carry (or a list that hasn't loaded) leaves the picker where it was. */
function indexOfName(names: string[], want?: string): number {
  if (!want) return 0;
  const i = names.findIndex((n) => n.toLowerCase() === want.toLowerCase());
  return i < 0 ? 0 : i;
}

export function ViewerDialog({
  open,
  onOpenChange,
  title,
  paintPaths = [],
  modelSource,
  modelSwap,
  gearSource,
  gearPart,
  stockGearPart,
  initialPaint,
  initialGoggles,
}: ViewerDialogProps) {
  const t = useT();
  // A bike model → bike; gear/rider paint → rider. No user switch.
  const isBike = !!modelSource || !!modelSwap;
  const mode: ViewerMode = isBike ? "bike" : "rider";
  // Null until the player picks: the selection falls back to `initialPaint`, and the
  // names it matches against only arrive once the model (or its archive) has loaded.
  // Deriving it rather than storing it is what lets a late-arriving list still land on
  // the right paint without ever overriding a pick made in the meantime.
  const [paintPick, setPaintPick] = useState<number | null>(null);
  const [gogglesPick, setGogglesPick] = useState<number | null>(null);
  const [model, setModel] = useState<BikeModel | null>(null);
  const [loadingModel, setLoadingModel] = useState(false);
  // Gear-paint path (no bike model): textures unpacked straight from a `.pnt`.
  const [gearTextures, setGearTextures] = useState<PaintTexture[] | null>(null);
  const [loadingPaint, setLoadingPaint] = useState(false);
  const [bodyNodes, setBodyNodes] = useState<EdfNode[] | null>(null);
  const [gear, setGear] = useState<RiderPart | null>(null);
  const [gearPaints, setGearPaints] = useState<GearPaints>(EMPTY_GEAR_PAINTS);
  const [err, setErr] = useState<string | null>(null);
  // Why the bike itself wouldn't load — a swap preview can be refused (an incomplete set,
  // a bike with nothing behind it), and that reason is worth showing.
  const [modelErr, setModelErr] = useState<string | null>(null);

  const nodes = model?.nodes ?? null;
  const paints = model?.paints ?? [];

  // The names behind each picker, in order — the labels shown are decorated versions of
  // these, and matching happens here so a decoration can never break the match.
  const paintNames = isBike
    ? paints.map((p) => p.name)
    : gearSource
      ? withStock(gearPaints.paints, gearPaints.hasStock)
      : paintPaths.map(paintLabel);
  const goggleNames = gearSource
    ? withStock(gearPaints.goggles, gearPaints.hasStockGoggles)
    : [];
  const paintIdx = paintPick ?? indexOfName(paintNames, initialPaint);
  const gogglesIdx = gogglesPick ?? indexOfName(goggleNames, initialGoggles);

  // Load the real bike geometry + its paints once per open (cached backend-side). A swap
  // preview takes the same shape — it's the same bike, assembled from a different set.
  const swapBike = modelSwap?.bike;
  const swapVariant = modelSwap?.variant;
  useEffect(() => {
    if (!open) {
      setModel(null);
      return;
    }
    const load =
      swapBike && swapVariant
        ? previewModelSwap(swapBike, swapVariant)
        : modelSource
          ? loadBikeModel(modelSource)
          : null;
    if (!load) {
      setModel(null);
      return;
    }
    let alive = true;
    setLoadingModel(true);
    setModelErr(null);
    load
      .then((m) => alive && setModel(m))
      .catch((e) => {
        if (alive) {
          setModelErr(String(e).replace(/^Error:\s*/, ""));
          setModel(null);
        }
      })
      .finally(() => alive && setLoadingModel(false));
    return () => {
      alive = false;
    };
  }, [open, modelSource, swapBike, swapVariant]);

  // Drop any pick each time it opens, so the next thing shown starts from its own paint
  // rather than the index left behind by the last one.
  useEffect(() => {
    if (open) {
      setPaintPick(null);
      setGogglesPick(null);
    }
  }, [open]);

  // Gear-paint preview: decode the selected `.pnt` (only when there's no bike).
  const gearPath = !isBike ? paintPaths[paintIdx] : undefined;
  useEffect(() => {
    if (!open || !gearPath) {
      setGearTextures(null);
      return;
    }
    let alive = true;
    setLoadingPaint(true);
    setErr(null);
    unpackPaint(gearPath)
      .then((t) => alive && setGearTextures(t))
      .catch((e) => {
        if (alive) {
          setErr(String(e).replace(/^Error:\s*/, ""));
          setGearTextures(null);
        }
      })
      .finally(() => alive && setLoadingPaint(false));
    return () => {
      alive = false;
    };
  }, [open, gearPath]);

  // Rider outfit → load the real player body from the game's `rider.pkz` for this profile.
  const riderProfile = useMemo(() => {
    const m = (gearPath ?? "").replace(/\\/g, "/").match(/riders\/([^/]+)\//i);
    return m?.[1] ?? "default_mx";
  }, [gearPath]);
  useEffect(() => {
    // A gear preview shows the piece itself — no rider body behind it.
    if (!open || isBike || gearSource || stockGearPart) {
      setBodyNodes(null);
      return;
    }
    let alive = true;
    loadRiderBodyModel(riderProfile)
      .then((n) => alive && setBodyNodes(n.length ? n : null))
      .catch(() => alive && setBodyNodes(null));
    return () => {
      alive = false;
    };
  }, [open, isBike, riderProfile, gearSource, stockGearPart]);

  // The textures the mesh should wear: the selected bike paint, or the gear paint.
  const activeTextures = useMemo<PaintTexture[]>(
    () => (isBike ? paints[paintIdx]?.textures ?? [] : gearTextures ?? []),
    [isBike, paints, paintIdx, gearTextures],
  );

  // Gear ships paints inside the archive — read them out for the picker.
  useEffect(() => {
    if (!open || !gearSource) {
      setGearPaints(EMPTY_GEAR_PAINTS);
      return;
    }
    let alive = true;
    listGearPaints(gearSource)
      .then((p) => alive && setGearPaints(p))
      .catch(() => alive && setGearPaints(EMPTY_GEAR_PAINTS));
    return () => {
      alive = false;
    };
  }, [open, gearSource]);

  // A gear model's own textures are a choice of their own next to its packed paints, so
  // "Stock" leads each list when the mesh carries one — and shifts the packed names by one.
  const gearStock = gearPaints.hasStock && paintIdx === 0;
  const gearStockGoggles = gearPaints.hasStockGoggles && gogglesIdx === 0;

  // The gear item to show: an installed gear model, or the stock model for a loose paint.
  const gearPaint = gearPaints.paints[paintIdx - (gearPaints.hasStock ? 1 : 0)];
  const gogglePaint = gearPaints.goggles[gogglesIdx - (gearPaints.hasStockGoggles ? 1 : 0)];
  useEffect(() => {
    if (!open || isBike) {
      setGear(null);
      return;
    }
    let load: Promise<RiderPart> | null = null;
    if (gearSource && gearPart) {
      load = loadGearModel(
        gearSource,
        gearPart,
        gearPaint,
        gogglePaint,
        gearStock,
        gearStockGoggles,
      );
    } else if (stockGearPart && gearPath) {
      load = loadStockGearModel(stockGearPart, gearPath);
    }
    if (!load) {
      setGear(null);
      return;
    }
    let alive = true;
    setLoadingPaint(true);
    load
      .then((g) => alive && setGear(g.nodes.length ? g : null))
      .catch(() => alive && setGear(null))
      .finally(() => alive && setLoadingPaint(false));
    return () => {
      alive = false;
    };
  }, [
    open,
    isBike,
    gearSource,
    gearPart,
    gearPaint,
    gogglePaint,
    gearStock,
    gearStockGoggles,
    stockGearPart,
    gearPath,
  ]);

  // Rider parts for the viewer: the real body (skinned when previewing a paint) plus any gear.
  const riderParts = useMemo<RiderPart[] | null>(() => {
    const out: RiderPart[] = [];
    if (bodyNodes) {
      // A gear preview shouldn't smear the gear's paint over the body.
      out.push({
        part: "body",
        nodes: bodyNodes,
        textures: gear ? [] : activeTextures,
      });
    }
    if (gear) out.push(gear);
    return out.length ? out : null;
  }, [bodyNodes, activeTextures, gear]);

  // A single representative texture for the placeholder stand-in.
  const byName = (names: string[]) =>
    activeTextures.find((t) => names.includes(t.name.toLowerCase())) ?? null;
  const standInTex =
    mode === "bike"
      ? byName(["livery", "bike_parts"]) ?? null
      : byName(["rider", "suit", "helmet", "gloves", "boots"]) ?? null;

  // Paint dropdown labels: a bike's paints carry a note when they can't move the preview.
  const paintOptions = isBike
    ? paintNames.map((n, i) => (paints[i]?.changesPreview ? n : `${n} — no change here`))
    : paintNames;
  // A helmet's goggles carry their own paint set (lens/strap) — a second picker.
  const goggleOptions = goggleNames;
  const paintNoChange = isBike && paints[paintIdx]?.changesPreview === false;

  const loading = loadingModel || loadingPaint;
  // A bike that loaded but yielded no geometry (older split-`.edf` bikes), or one that
  // wouldn't load at all — show a message, not a fake.
  const bikeFailed =
    isBike && !loadingModel && (!!modelErr || (!!model && !model.nodes.length));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* The close button lives in the header row (below) so it lines up with the
          paint pickers instead of floating over them at a fixed offset. */}
      <DialogContent
        showClose={false}
        className="flex h-[85vh] w-[92vw] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
      >
        <div className="flex flex-none items-center justify-between gap-3 border-b border-border px-4 py-2.5">
          <div className="flex min-w-0 items-center gap-2 text-sm font-medium">
            <Box className="h-4 w-4 flex-none text-muted-foreground" />
            <span className="truncate">{title ?? t("viewer.preview3d")}</span>
          </div>
          <div className="flex flex-none items-center gap-2">
            {paintOptions.length > 0 && (
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                {t("viewer.paint")}
                <select
                  value={paintIdx}
                  onChange={(e) => setPaintPick(Number(e.target.value))}
                  className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground"
                >
                  {paintOptions.map((name, i) => (
                    <option key={`${name}-${i}`} value={i}>
                      {name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {goggleOptions.length > 0 && (
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                {t("category.goggles")}
                <select
                  value={gogglesIdx}
                  onChange={(e) => setGogglesPick(Number(e.target.value))}
                  className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground"
                >
                  {goggleOptions.map((name, i) => (
                    <option key={`${name}-${i}`} value={i}>
                      {name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <DialogClose className="rounded-md p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100 focus:outline-none">
              <X className="size-4" />
              <span className="sr-only">{t("common.close")}</span>
            </DialogClose>
          </div>
        </div>

        <div className="relative min-h-0 flex-1">
          <ModelViewer
            mode={mode}
            texture={standInTex ?? null}
            textures={activeTextures}
            nodes={nodes}
            riderParts={riderParts}
            loading={loading}
            noStandIn={isBike}
            className="absolute inset-0"
          />
          {stockGearPart && gear && !loading && (
            <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center p-3">
              <span className="rounded-md bg-black/70 px-3 py-1.5 text-center text-xs text-white/90">
                {t("viewer.stockGearNote", { part: stockGearPart })}
              </span>
            </div>
          )}
          {paintNoChange && !loading && (
            <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center p-3">
              <span className="rounded-md bg-black/70 px-3 py-1.5 text-center text-xs text-white/90">
                {t("viewer.paintNoChange")}
              </span>
            </div>
          )}
          {bikeFailed && (
            <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-1 text-center">
              <span className="text-sm font-medium text-foreground">
                Can&apos;t load bike model
              </span>
              <span className="max-w-md px-6 text-xs text-muted-foreground">
                {modelErr ??
                  "This bike's 3D model isn't in a format the viewer supports yet."}
              </span>
            </div>
          )}
          {loading && (
            <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/40">
              <Loader2 className="h-6 w-6 animate-spin text-white/80" />
              <span className="text-sm text-white/80">
                {loadingModel ? t("viewer.loadingModel") : t("viewer.loadingPaint")}
              </span>
              {/* Indeterminate bar so a slow decode/transfer never looks hung. */}
              <div className="h-1 w-52 overflow-hidden rounded-full bg-white/15">
                <div
                  className="h-full w-1/3 rounded-full bg-primary"
                  style={{ animation: "mxbLoadSlide 1.1s ease-in-out infinite" }}
                />
              </div>
              <style>{`@keyframes mxbLoadSlide{0%{transform:translateX(-110%)}100%{transform:translateX(320%)}}`}</style>
            </div>
          )}
          {!loading && err && (
            <div className="pointer-events-none absolute left-3 top-3 rounded-md bg-black/55 px-2 py-1 text-xs text-white/85">
              {t("viewer.noPaintPreview", { err })}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
