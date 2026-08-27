import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Maximize2,
  Bike,
  User,
  Users,
  PersonStanding,
  Box,
  Loader2,
  AlertTriangle,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { Button } from "../ui/button";
import { Dialog, DialogContent } from "../ui/dialog";
import { ModelViewer, type CaptureFn, type ViewerMode } from "./ModelViewer";
import { loadRiderModel, previewModelSwap } from "../../api/mods";
import type { BikeModel, Loadout, PaintTexture, RiderPart } from "../../types";
import { riderFrame, type PosableRig, type RiderPose } from "../../lib/riderPose";
import type { SceneId } from "../../lib/viewerScene";
import { useT } from "../../i18n/context";
import { TyresPicker } from "./TyresPicker";
import { useTyresPick } from "./tyresPick";

interface ViewerPanelProps {
  texture?: PaintTexture | null;
  loadout?: Loadout;
  riderOnly?: boolean;
  /**
   * Draw this bike beside the rider, wearing the loadout's `paint`.
   *
   * Absent means rider (or stand-in bike) only, exactly as before. Present turns the panel
   * into the pair view and adds "Both" to the toggle.
   */
  bikeId?: string;
  /**
   * Which model-swap variant to draw the bike as. An empty `modelSwap` slot means "leave
   * the model alone", which is the variant currently loose at the bike's root — the caller
   * knows that from its scan, so it passes it rather than this guessing "Stock".
   */
  bikeVariant?: string;
  hiddenParts?: RiderPart["part"][];
  /**
   * The rider's pose — a turn per bone. Absent draws the body as the model was authored,
   * which is what every caller but the Pose studio wants.
   */
  riderPose?: RiderPose;
  /**
   * Given, the rider wears a grab handle at each joint and dragging one writes a pose back
   * through this. Only the Pose studio passes it.
   */
  onRiderPose?: (pose: RiderPose) => void;
  /** Which bone a drag has just taken hold of — the Pose studio opens its sliders. */
  onPoseGrab?: (bone: string) => void;
  /**
   * Offer "On bike": the rider sitting on the bike rather than standing beside it.
   *
   * Opt-in, and the view this opens on where it is offered. Only the Pose studio asks — the
   * Rider tab is where a look is composed, and two models side by side is the clearer view
   * of one.
   */
  offerOnBike?: boolean;
  /** The backdrop to stand the model against. */
  scene?: SceneId;
  /** Photo mode: model and backdrop, no dots and no panels. */
  photo?: boolean;
  /**
   * The rider's rig as it was loaded, so a caller can state a move against it. Null whenever
   * there is no body on screen to move.
   */
  onRiderRig?: (rig: PosableRig | null) => void;
  /** Handed the way to take a photo of whichever canvas is on screen. */
  onCaptureReady?: (capture: CaptureFn | null) => void;
  className?: string;
}

function ModeToggle({
  mode,
  modes,
  disabled,
  onChange,
}: {
  mode: ViewerMode;
  /** Which segments to offer, in order. */
  modes: ViewerMode[];
  /** A segment that is offered but can't be entered, and why. */
  disabled?: { mode: ViewerMode; why: string };
  onChange: (m: ViewerMode) => void;
}) {
  const t = useT();
  const seg: Record<ViewerMode, { icon: typeof Bike; label: string }> = {
    bike: { icon: Bike, label: t("category.bike") },
    rider: { icon: User, label: t("nav.rider") },
    both: { icon: Users, label: t("viewer.both") },
    onBike: { icon: PersonStanding, label: t("viewer.onBike") },
  };
  return (
    <div className="inline-flex rounded-md border border-border bg-background/60 p-0.5">
      {modes.map((m) => ({ m, ...seg[m] })).map(({ m, icon: Icon, label }) => {
        const off = disabled?.mode === m;
        return (
          <button
            key={m}
            type="button"
            disabled={off}
            title={off ? disabled?.why : undefined}
            onClick={() => onChange(m)}
            className={cn(
              "flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-medium transition-colors",
              mode === m
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:text-foreground",
              off && "cursor-not-allowed opacity-40 hover:text-muted-foreground",
            )}
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
          </button>
        );
      })}
    </div>
  );
}

export function ViewerPanel({
  texture,
  loadout,
  riderOnly = false,
  bikeId,
  bikeVariant,
  hiddenParts,
  riderPose,
  onRiderPose,
  onPoseGrab,
  offerOnBike = false,
  scene,
  photo = false,
  onRiderRig,
  onCaptureReady,
  className,
}: ViewerPanelProps) {
  const t = useT();
  const withBike = !!bikeId && !riderOnly;
  const seated = offerOnBike && withBike;
  const modes: ViewerMode[] = seated
    ? ["onBike", "both", "bike", "rider"]
    : withBike
      ? ["both", "bike", "rider"]
      : ["bike", "rider"];
  const [mode, setMode] = useState<ViewerMode>(
    riderOnly ? "rider" : seated ? "onBike" : withBike ? "both" : "bike",
  );
  const [expanded, setExpanded] = useState(false);
  const [riderParts, setRiderParts] = useState<RiderPart[] | null>(null);
  const [loading, setLoading] = useState(false);
  // The bike half. Kept whole rather than as bare nodes: the model carries every paint
  // installed for it, so switching livery is a pick out of this and not another resolve.
  const [bikeModel, setBikeModel] = useState<BikeModel | null>(null);
  const tyresPick = useTyresPick();
  const [bikeLoading, setBikeLoading] = useState(false);
  const [bikeError, setBikeError] = useState<string | null>(null);
  const bikeFirst = useRef(true);
  const bikeToasted = useRef<string | null>(null);
  // A resolve that failed. Kept in state because the previous model stays on screen
  // (see below) — without this the panel looks like the pick simply did nothing.
  const [loadError, setLoadError] = useState<string | null>(null);
  // First resolve loads immediately; later slot edits are debounced so picks don't thrash the decoder.
  const firstLoad = useRef(true);
  // Toast once per distinct message: a resolve runs on every slot edit, and a fault that
  // persists (a missing profile) would otherwise raise one toast per pick.
  const toasted = useRef<string | null>(null);

  // Drop any toggled-off gear before rendering (keep the body + everything else).
  const shownParts = hiddenParts?.length
    ? riderParts?.filter((p) => !hiddenParts.includes(p.part)) ?? null
    : riderParts;

  // Re-resolve rider gear when a rider-affecting slot changes (debounced; loadout updates per keystroke).
  const riderKey = loadout
    ? [
        loadout.rider,
        loadout.helmet,
        loadout.helmetPaint,
        loadout.gogglesPaint,
        loadout.boots,
        loadout.bootsPaint,
        loadout.protection,
        loadout.protectionPaint,
        loadout.suitPaint,
        loadout.glovesPaint,
      ].join("|")
    : "";

  useEffect(() => {
    if (!loadout) {
      setRiderParts(null);
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    const delay = firstLoad.current ? 0 : 200;
    firstLoad.current = false;
    // Not `t` — that's the translator this scope needs for the failure toast.
    const timer = setTimeout(() => {
      loadRiderModel(loadout)
        // Keep the previous model on screen until the new one is ready (and on failure) so it never blanks.
        .then((m) => {
          if (!alive) return;
          setRiderParts(m.parts);
          setLoadError(null);
          toasted.current = null;
        })
        // A failure used to be swallowed here. With the old model left on screen that is
        // indistinguishable from a pick that resolved to the same look, so a real fault
        // reads as "changing this slot does nothing". Say so instead.
        .catch((e) => {
          const msg = String(e).replace(/^Error:\s*/, "");
          console.error("[viewer] rider resolve failed:", e);
          if (!alive) return;
          setLoadError(msg);
          if (toasted.current !== msg) {
            toasted.current = msg;
            toast.error(t("viewer.riderLoadFailed"), { description: msg });
          }
        })
        .finally(() => alive && setLoading(false));
    }, delay);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [riderKey]);

  // The bike, resolved the same way the rider is: debounced, and the previous model stays
  // up while the next one is read. Only the bike and its variant re-resolve — the livery is
  // already in hand, so picking one below costs nothing.
  useEffect(() => {
    if (!withBike) {
      setBikeModel(null);
      setBikeLoading(false);
      return;
    }
    let alive = true;
    setBikeLoading(true);
    const delay = bikeFirst.current ? 0 : 200;
    bikeFirst.current = false;
    const timer = setTimeout(() => {
      // "Stock" is the fallback the backend understands for a bike whose active variant the
      // caller couldn't name — the model packed in the archive.
      previewModelSwap(bikeId!, bikeVariant || "Stock", tyresPick.tyres)
        .then((m) => {
          if (!alive) return;
          setBikeModel(m);
          setBikeError(null);
          bikeToasted.current = null;
        })
        .catch((e) => {
          const msg = String(e).replace(/^Error:\s*/, "");
          console.error("[viewer] bike resolve failed:", e);
          if (!alive) return;
          setBikeError(msg);
          if (bikeToasted.current !== msg) {
            bikeToasted.current = msg;
            toast.error(t("viewer.bikeLoadFailed"), { description: msg });
          }
        })
        .finally(() => alive && setBikeLoading(false));
    }, delay);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [withBike, bikeId, bikeVariant, tyresPick.tyres]);

  // The rig the ready-made moves are stated against. Read here because this is where the
  // rider model is: the studio next door only ever sees a loadout.
  useEffect(() => {
    if (!onRiderRig) return;
    const body = riderParts?.find((p) => p.part === "body" && p.nodes.length);
    const bones = body?.skeleton;
    const frame = bones?.length ? riderFrame(bones, body?.nodes) : null;
    onRiderRig(bones?.length && frame ? { bones, frame } : null);
  }, [riderParts, onRiderRig]);

  // One way to take a photo, whichever canvas is up. The expanded dialog wins when it is
  // open — it is the bigger frame, and the one somebody opened to compose a shot in.
  const inlineShot = useRef<CaptureFn | null>(null);
  const bigShot = useRef<CaptureFn | null>(null);
  // Stable, so the canvases don't re-register every time a slot is typed into.
  const takeInline = useCallback((c: CaptureFn | null) => {
    inlineShot.current = c;
  }, []);
  const takeBig = useCallback((c: CaptureFn | null) => {
    bigShot.current = c;
  }, []);
  const capture = useCallback<CaptureFn>(
    (s) => (bigShot.current ?? inlineShot.current)?.(s) ?? null,
    [],
  );
  useEffect(() => {
    onCaptureReady?.(capture);
    return () => onCaptureReady?.(null);
  }, [onCaptureReady, capture]);

  // The livery the loadout names, out of what the bike carries. Nothing named, or a name
  // nothing installed answers to, leaves the model in the look it ships with.
  const bikeTextures = useMemo(() => {
    if (!bikeModel) return undefined;
    const pick = loadout?.paint
      ? bikeModel.paints.find((p) => p.name === loadout.paint)
      : undefined;
    return pick?.textures ?? bikeModel.base;
  }, [bikeModel, loadout?.paint]);

  // Which halves of the scene this mode actually draws.
  const drawsRider = mode !== "bike";
  const drawsBike = withBike && mode !== "rider";
  // A bike whose `.geom` names no seat can't be sat on. Offered but refused, with the reason
  // on the segment, rather than quietly drawing the pair side by side under an "On bike" label.
  const seatable = !bikeModel || !!bikeModel.rig?.seat;
  // Offered by default and then refused — a bike whose `.geom` names no seat. Fall back to
  // the pair rather than leaving the toggle on a segment that can't draw what it says.
  useEffect(() => {
    if (mode === "onBike" && !seatable) setMode("both");
  }, [mode, seatable]);

  // While a model is resolving for the first time, show a clear centered "Loading" state
  // instead of the placeholder (see `riderLoading` passed to the viewer). Once something is
  // on screen, a re-resolve only gets the corner chip so the current model stays visible.
  const riderFirstLoad = loading && drawsRider && !shownParts?.length;
  const bikeFirstLoad = bikeLoading && drawsBike && !bikeModel;
  // In the pair view neither half may claim the whole canvas: a bike still reading while the
  // rider is up would blank a model that is perfectly good. Only take over the canvas when
  // nothing at all is on screen yet.
  const nothingYet =
    (riderFirstLoad || bikeFirstLoad) &&
    !(drawsRider && shownParts?.length) &&
    !(drawsBike && bikeModel);
  // Suppress the stand-in rider while loading, but never hide the bike stand-in.
  const riderLoading = drawsRider && mode !== "both" && loading;
  const busy = (drawsRider && loading) || (drawsBike && bikeLoading);
  // Stale-model warning, per half — with two of them the badge has to say which one is out
  // of date, or "preview is out of date" points at whichever model you happen to be reading.
  const staleKey = drawsRider && loadError
    ? ("viewer.riderLoadFailed" as const)
    : drawsBike && bikeError
      ? ("viewer.bikeLoadFailed" as const)
      : null;
  const staleWhy = (drawsRider && loadError) || (drawsBike && bikeError) || "";

  const overlay = nothingYet ? (
    <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2 text-muted-foreground">
      <Loader2 className="h-6 w-6 animate-spin" />
      <span className="text-[12.5px]">
        {t(bikeFirstLoad && !riderFirstLoad ? "viewer.loadingBike" : "viewer.loadingRider")}
      </span>
    </div>
  ) : busy ? (
    <div className="pointer-events-none absolute right-3 top-3 flex items-center gap-1.5 rounded-md bg-black/55 px-2 py-1 text-[11px] text-white/85">
      <Loader2 className="h-3.5 w-3.5 animate-spin" />
      {t("common.loading")}
    </div>
  ) : (
    // The badge has to stay up (not a toast that fades) for as long as the model is out of date.
    staleKey && (
      <div
        title={staleWhy}
        className="absolute right-3 top-3 flex max-w-[85%] items-center gap-1.5 rounded-md bg-destructive/90 px-2 py-1 text-[11px] text-destructive-foreground"
      >
        <AlertTriangle className="h-3.5 w-3.5 flex-none" />
        <span className="truncate">{t(staleKey)}</span>
      </div>
    )
  );

  // What goes to the canvas. `nodes` is what turns the pair view on in `ModelViewer`, so it
  // is only handed over when this mode wants the bike drawn.
  const view = {
    mode,
    texture,
    textures: drawsBike ? bikeTextures : undefined,
    nodes: drawsBike ? bikeModel?.nodes ?? null : null,
    rig: drawsBike ? bikeModel?.rig ?? null : null,
    riderParts: drawsRider ? shownParts : null,
    loading: riderLoading,
    scene,
    photo,
    // With a real bike on the way, the cartoon stand-in beside the rider is worse than
    // nothing — it reads as the preset having resolved to that.
    noStandIn: withBike,
  };

  return (
    <>
      <div
        className={cn(
          "flex flex-col overflow-hidden rounded-lg border border-border bg-card",
          className,
        )}
      >
        <div className="flex items-center justify-between border-b border-border px-3 py-2">
          <div className="flex items-center gap-2 text-sm font-medium">
            <Box className="h-4 w-4 text-muted-foreground" />
            {t("viewer.preview3d")}
          </div>
          <div className="flex items-center gap-2">
            {withBike && <TyresPicker pick={tyresPick} />}
            {!riderOnly && (
              <ModeToggle
                mode={mode}
                modes={modes}
                disabled={seatable ? undefined : { mode: "onBike", why: t("viewer.noSeat") }}
                onChange={setMode}
              />
            )}
            <Button
              variant="chip"
              size="icon"
              className="h-7 w-7"
              title={t("viewer.expand")}
              onClick={() => setExpanded(true)}
            >
              <Maximize2 className="h-4 w-4" />
            </Button>
          </div>
        </div>
        <div className="relative min-h-[280px] flex-1">
          {/* Both panels are a single collapsed row until someone opens one, so the inline
              canvas only gives up its corner to a person who asked for it. */}
          <ModelViewer
            {...view}
            riderPose={riderPose}
            onRiderPose={onRiderPose}
            onPoseGrab={onPoseGrab}
            onCaptureReady={takeInline}
            poseControls
            placeControls
            className="absolute inset-0"
          />
          {overlay}
        </div>
      </div>

      <Dialog open={expanded} onOpenChange={setExpanded}>
        {/* `flex flex-col`, because a dialog is a grid by default: its two rows then stretch
            to equal heights, which gave the title bar half the window and left the canvas —
            absolutely positioned, so no height of its own — with the rest. */}
        <DialogContent className="flex h-[85vh] w-[92vw] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none">
          <div className="flex flex-none items-center justify-between border-b border-border px-4 py-2.5">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Box className="h-4 w-4 text-muted-foreground" />
              {t("viewer.preview3d")}
            </div>
            <div className="flex items-center gap-2">
              {withBike && <TyresPicker pick={tyresPick} />}
              {!riderOnly && (
                <ModeToggle
                  mode={mode}
                  modes={modes}
                  disabled={seatable ? undefined : { mode: "onBike", why: t("viewer.noSeat") }}
                  onChange={setMode}
                />
              )}
            </div>
          </div>
          <div className="relative min-h-0 flex-1">
            <ModelViewer
              {...view}
              riderPose={riderPose}
              onRiderPose={onRiderPose}
              onPoseGrab={onPoseGrab}
              onCaptureReady={takeBig}
              poseControls
              placeControls
              className="absolute inset-0"
            />
            {overlay}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
