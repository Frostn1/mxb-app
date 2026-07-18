import { useEffect, useMemo, useState } from "react";
import { Box, Loader2 } from "lucide-react";
import { Dialog, DialogContent } from "../ui/dialog";
import { ModelViewer, type ViewerMode } from "./ModelViewer";
import {
  unpackPaint,
  loadBikeModel,
  loadRiderBodyModel,
  loadGearModel,
  loadStockGearModel,
  listGearPaints,
} from "../../api/mods";
import type { PaintTexture, BikeModel, EdfNode, RiderPart, GearPaints } from "../../types";

interface ViewerDialogProps {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title?: string;
  initialMode?: ViewerMode;
  /** Candidate `.pnt` paint files to offer (gear-paint preview, no bike model). */
  paintPaths?: string[];
  /** Bike folder / `.pkz` / `.edf` path to load real geometry + paints from. */
  modelSource?: string;
  /** An installed gear item (folder or `.pkz`) to show on the rider, and which
   * slot it fills — lets a helmet/boots mod be previewed straight from the
   * Library, no game profile needed. */
  gearSource?: string;
  gearPart?: RiderPart["part"];
  /** For a loose gear **paint** whose own model may not be installed: the slot to
   * preview it on the game's **stock** model (`boots`/`helmet`/`protection`). The
   * `.pnt` to apply is `paintPaths[paintIdx]`. */
  stockGearPart?: RiderPart["part"];
}

/** Strip folder + `.pnt` for a readable paint label. */
function paintLabel(path: string): string {
  const base = path.replace(/\\/g, "/").split("/").pop() ?? path;
  return base.replace(/\.pnt$/i, "");
}

/**
 * Full-screen 3D viewer for a single library item — a bike (with its selectable
 * paints) or a piece of gear. Works without a game profile, so it's usable on any
 * platform for any item in the library.
 */
export function ViewerDialog({
  open,
  onOpenChange,
  title,
  paintPaths = [],
  modelSource,
  gearSource,
  gearPart,
  stockGearPart,
}: ViewerDialogProps) {
  // The content decides the view: a bike model → bike; gear/rider paint → rider.
  // There's no user switch — a rider outfit is never shown as a bike.
  const isBike = !!modelSource;
  const mode: ViewerMode = isBike ? "bike" : "rider";
  const [paintIdx, setPaintIdx] = useState(0);
  const [model, setModel] = useState<BikeModel | null>(null);
  const [loadingModel, setLoadingModel] = useState(false);
  // Gear-paint path (no bike model): textures unpacked straight from a `.pnt`.
  const [gearTextures, setGearTextures] = useState<PaintTexture[] | null>(null);
  const [loadingPaint, setLoadingPaint] = useState(false);
  const [bodyNodes, setBodyNodes] = useState<EdfNode[] | null>(null);
  const [gear, setGear] = useState<RiderPart | null>(null);
  const [gearPaints, setGearPaints] = useState<GearPaints>({ paints: [], goggles: [] });
  const [gogglesIdx, setGogglesIdx] = useState(0);
  const [err, setErr] = useState<string | null>(null);

  const nodes = model?.nodes ?? null;
  const paints = model?.paints ?? [];

  // Load the real bike geometry + its paints once per open (cached backend-side).
  useEffect(() => {
    if (!open || !modelSource) {
      setModel(null);
      return;
    }
    let alive = true;
    setLoadingModel(true);
    loadBikeModel(modelSource)
      .then((m) => alive && setModel(m))
      .catch(() => alive && setModel(null))
      .finally(() => alive && setLoadingModel(false));
    return () => {
      alive = false;
    };
  }, [open, modelSource]);

  // Reset to the first paint each time it opens.
  useEffect(() => {
    if (open) {
      setPaintIdx(0);
      setGogglesIdx(0);
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

  // Rider outfit → load the real player body from the game's `rider.pkz` (the
  // profile the paint lives under, e.g. `riders/default_mx/paints/…`). Empty if the
  // game folder isn't set → the viewer falls back to the stand-in.
  const riderProfile = useMemo(() => {
    const m = (gearPath ?? "").replace(/\\/g, "/").match(/riders\/([^/]+)\//i);
    return m?.[1] ?? "default_mx";
  }, [gearPath]);
  useEffect(() => {
    // A gear preview shows the piece itself — no rider body behind it (whether an
    // installed gear model or a loose gear paint on the stock model).
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

  // Gear ships its paints inside the archive, so they aren't loose files the
  // Library can list — read them out for the picker.
  useEffect(() => {
    if (!open || !gearSource) {
      setGearPaints({ paints: [], goggles: [] });
      return;
    }
    let alive = true;
    listGearPaints(gearSource)
      .then((p) => alive && setGearPaints(p))
      .catch(() => alive && setGearPaints({ paints: [], goggles: [] }));
    return () => {
      alive = false;
    };
  }, [open, gearSource]);

  // The gear item to show: either an installed gear model (in the selected skin +
  // goggle paint), or — for a loose gear paint whose model isn't installed — the
  // game's stock model for that slot wearing the paint.
  const gearPaint = gearPaints.paints[paintIdx];
  const gogglePaint = gearPaints.goggles[gogglesIdx];
  useEffect(() => {
    if (!open || isBike) {
      setGear(null);
      return;
    }
    let load: Promise<RiderPart> | null = null;
    if (gearSource && gearPart) {
      load = loadGearModel(gearSource, gearPart, gearPaint, gogglePaint);
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
  }, [open, isBike, gearSource, gearPart, gearPaint, gogglePaint, stockGearPart, gearPath]);

  // Rider parts for the viewer: the real body (skinned with the outfit paint when
  // we're previewing one) plus any gear item. Empty → ModelViewer's stand-in.
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

  // Paint dropdown options: a bike's paints, a gear item's packed paints, or the
  // loose `.pnt` candidates for a paint entry.
  // Every paint stays selectable. A paint that touches none of the parts shown is
  // noted, so that "nothing happened" reads as expected rather than broken — but
  // it is NOT called wrong for the bike: it may simply paint the wheels or chain,
  // which this preview doesn't render.
  const paintOptions = isBike
    ? paints.map((p) => (p.changesPreview ? p.name : `${p.name} — no change here`))
    : gearSource
      ? gearPaints.paints
      : paintPaths.map(paintLabel);
  // A helmet's goggles carry their own paint set (lens/strap), shown as a second
  // picker next to the skin one.
  const goggleOptions = gearSource ? gearPaints.goggles : [];
  const paintNoChange = isBike && paints[paintIdx]?.changesPreview === false;

  const loading = loadingModel || loadingPaint;
  // A bike whose load finished but yielded no geometry (e.g. an older bike that
  // ships its parts as separate `.edf` files, no `model.edf`) — show a clear
  // message instead of a fake stand-in.
  const bikeFailed = isBike && !loadingModel && !!model && !model.nodes.length;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[85vh] w-[92vw] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none">
        <div className="flex flex-none items-center justify-between gap-3 border-b border-border px-4 py-2.5">
          <div className="flex min-w-0 items-center gap-2 text-sm font-medium">
            <Box className="h-4 w-4 flex-none text-muted-foreground" />
            <span className="truncate">{title ?? "3D Preview"}</span>
          </div>
          <div className="flex flex-none items-center gap-2">
            {paintOptions.length > 0 && (
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                Paint
                <select
                  value={paintIdx}
                  onChange={(e) => setPaintIdx(Number(e.target.value))}
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
                Goggles
                <select
                  value={gogglesIdx}
                  onChange={(e) => setGogglesIdx(Number(e.target.value))}
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
          </div>
        </div>

        <div className="relative min-h-0 flex-1">
          <ModelViewer
            mode={mode}
            texture={standInTex?.png ?? null}
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
                Shown on the game's stock {stockGearPart}. A paint made for a different
                model may not line up perfectly.
              </span>
            </div>
          )}
          {paintNoChange && !loading && (
            <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center p-3">
              <span className="rounded-md bg-black/70 px-3 py-1.5 text-center text-xs text-white/90">
                None of this paint's textures are used by the parts shown here, so the
                preview doesn't change. It may still paint the wheels or chain, which
                this view doesn't render.
              </span>
            </div>
          )}
          {bikeFailed && (
            <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-1 text-center">
              <span className="text-sm font-medium text-foreground">Can't load bike model</span>
              <span className="text-xs text-muted-foreground">
                This bike's 3D model isn't in a format the viewer supports yet.
              </span>
            </div>
          )}
          {loading && (
            <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/40">
              <Loader2 className="h-6 w-6 animate-spin text-white/80" />
              <span className="text-sm text-white/80">
                {loadingModel ? "Loading model…" : "Loading paint…"}
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
              No paint preview ({err})
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
