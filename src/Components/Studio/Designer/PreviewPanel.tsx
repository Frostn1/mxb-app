import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Box, Loader2, Maximize2, Minimize2, TriangleAlert } from "lucide-react";
import type * as THREE from "three";
import { cn } from "@/lib/utils";
import { Dialog, DialogContent, DialogTitle } from "../../ui/dialog";
import { ModelViewer } from "../../Viewer/ModelViewer";
import { TyresPicker } from "../../Viewer/TyresPicker";
import { useTyresPick } from "../../Viewer/tyresPick";
import { loadBikeModel, loadRiderModel, scanLibrary } from "../../../api/mods";
import { displayName } from "../../../lib/mods";
import { EMPTY_LOADOUT } from "../../../lib/presets";
import type { EdfNode, Loadout, PaintTexture, RiderPart } from "../../../types";
import { useT, type TKey } from "../../../i18n/context";
import { useConfig } from "../../../Context/Config";
import { gearPartOf, isBikeKind, type PaintDestState } from "../paintDest";

/**
 * The model the paint is being drawn for, wearing what's on the canvas.
 *
 * The destination picker already answers "what am I painting, and for which model" — this
 * turns that same answer into geometry, so choosing where the file goes and choosing what you
 * see are one decision rather than two that can disagree.
 *
 * The drawing reaches the mesh as a texture override keyed by sheet name, which is exactly how
 * the game binds it: a sheet called `livery` lands wherever the model asked for `livery`. A
 * sheet named something the model never asks for shows up nowhere here — which is the truth,
 * and worth seeing before the file is saved rather than after it loads blank in game.
 */
/**
 * Rider pieces that can be taken off to see what they cover.
 *
 * Only the ones that sit *over* something: a chest protector hides most of a jersey, a helmet
 * hides the collar. Boots and gloves cover nothing else, so hiding them would be a control with
 * nothing behind it.
 */
/** "This model can't name its own textures" — one array, so reporting it twice is one value. */
const NO_STOCK: PaintTexture[] = [];

const HIDEABLE: { part: RiderPart["part"]; label: TKey }[] = [
  { part: "protection", label: "paints.kind.protection" },
  { part: "helmet", label: "paints.kind.helmet" },
];

export function PreviewPanel({
  state,
  overrides,
  frameToken,
  onGeometry,
  onStock,
  highlight,
  className,
}: {
  state: PaintDestState;
  overrides: Map<string, THREE.Texture>;
  /** Bumped whenever the canvas changed, so a frame gets drawn without any prop moving. */
  frameToken?: number;
  /**
   * The mesh this is showing, handed back so the editor can draw its UV layout.
   *
   * Reported from here rather than loaded twice: this panel already resolves "which model, for
   * which destination" through the library, and a second loader would be a second answer to
   * that question — one that could quietly disagree about which bike is on screen.
   */
  /**
   * The mesh on screen, with whether its parts were assembled into one frame.
   *
   * The two travel together because the second qualifies the first: `assembled` is what makes a
   * position mean "left of the bike" rather than "left of whatever this part's frame is", and
   * handing over geometry without it invites reading the numbers as more than they say.
   */
  onGeometry?: (nodes: EdfNode[] | null, assembled: boolean) => void;
  /** Triangles to light up — what the pointer is over in the 2D editor. */
  highlight?: Int32Array | null;
  /**
   * The model's own textures, handed back with the mesh so the editor can show what the
   * bike already looks like under a sheet being drawn from blank.
   *
   * Bikes only, and empty for anything else. A rider part's textures are whatever dressed
   * it — often the mesh's own, but a helmet that ships paints and embeds no shell wears a
   * `.pnt` here, and offering that as "stock" would be a confident lie about somebody
   * else's paint.
   */
  onStock?: (textures: PaintTexture[]) => void;
  className?: string;
}) {
  const t = useT();
  const { game } = useConfig();
  const { kind, model, bikePreview } = state;
  const [nodes, setNodes] = useState<EdfNode[] | null>(null);
  // Reported by the backend with the mesh, never inferred here — see `BikeModel.assembled`.
  const [assembled, setAssembled] = useState(false);
  const [textures, setTextures] = useState<PaintTexture[]>([]);
  // The model's own textures, kept apart from the ones it is currently wearing.
  const [stock, setStock] = useState<PaintTexture[]>([]);
  const tyresPick = useTyresPick();
  const [riderParts, setRiderParts] = useState<RiderPart[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [hidden, setHidden] = useState<RiderPart["part"][]>([]);
  const [soloGear, setSoloGear] = useState(false);
  // The same panel, drawn over the editor rather than beside it — see the render below.
  const [full, setFull] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  /**
   * A tab away from the Designer takes the fullscreen view with it.
   *
   * The Studio hides its panes rather than unmounting them, and this view is portalled to
   * the document — so a Designer that went out of sight would leave its dialog covering
   * whatever is now on screen.
   */
  useEffect(() => {
    const el = panelRef.current;
    if (!full || !el) return;
    // A pane switched away from is `display: none`, which reports a 0×0 box here.
    const ro = new ResizeObserver(() => {
      if (!el.offsetParent) setFull(false);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [full]);

  const isBike = isBikeKind(kind);
  const gearPart = gearPartOf(kind);
  /** Gear-only, and something to actually show that way — no model means no piece. */
  const solo = soloGear && !!gearPart && !!model;

  /**
   * Start with the pieces that cover what you're painting taken off.
   *
   * Painting a jersey with a chest protector over it means judging the third of it you can
   * see — and the stock protector renders untextured grey, which reads as the paint having
   * failed rather than as gear being in the way. The toggles above put them back.
   */
  useEffect(() => {
    setHidden(HIDEABLE.filter((h) => h.part !== gearPart).map((h) => h.part));
    // Back to the rider whenever what's being painted changes: kit and gloves have no piece
    // to show on its own, so a toggle left on would be a control over nothing.
    setSoloGear(false);
  }, [gearPart]);

  /** The rider slots to fill so the piece being painted is the piece on screen. */
  const loadout = useMemo<Loadout | null>(() => {
    if (isBike || !model) return null;
    const base: Loadout = { ...EMPTY_LOADOUT };
    switch (gearPart) {
      case "helmet":
        return { ...base, helmet: model };
      case "boots":
        return { ...base, boots: model };
      case "protection":
        return { ...base, protection: model };
      default:
        // Kit and gloves are worn on the rider itself, so the profile *is* the model.
        return { ...base, rider: model };
    }
  }, [isBike, gearPart, model]);

  // Bike geometry, resolved through the library rather than by building a path: a bike may be
  // an extracted folder or a `.pkz` beside one, and the library already knows which.
  useEffect(() => {
    if (!isBike || !model || !bikePreview) {
      setNodes(null);
      setAssembled(false);
      setTextures([]);
      setStock([]);
      return;
    }
    let alive = true;
    setLoading(true);
    setErr(null);
    scanLibrary("mods/bikes")
      .then((entries) => {
        const named = entries.filter(
          (e) => e.name === model || displayName(e.name) === model,
        );
        // A bike is often both a folder and a `.pkz` of the same name, and which one holds
        // the mesh varies — the OEM bikes keep theirs in the archive and leave the folder to
        // paints. `gather_bike_files` already falls back from one to the other, so the folder
        // is the better thing to hand it: it works for a bike that ships either way.
        const found = named.find((e) => e.kind === "folder") ?? named[0];
        // Not `category === "bike"`: a folder holding nothing but paints is filed as
        // something else, and it is still the right path to try — the load then fails with
        // the accurate reason (no mesh) instead of this claiming it isn't installed.
        if (!found) throw new Error(t("designer.noModelFound", { model }));
        return loadBikeModel(found.path, tyresPick.tyres);
      })
      .then((m) => {
        if (!alive) return;
        setNodes(m.nodes);
        setAssembled(m.assembled);
        // The model's own look, under the drawing — so parts this paint doesn't cover still
        // read as the bike rather than as untextured grey.
        setTextures(m.paints[0]?.textures ?? []);
        // Not `paints[0]`: that's whichever livery happened to sort first, and on a bike
        // with an installed paint it is somebody's artwork rather than the bike's own.
        setStock(m.base ?? []);
      })
      .catch((e) => {
        if (!alive) return;
        setNodes(null);
        setAssembled(false);
        setTextures([]);
        setStock([]);
        setErr(String(e).replace(/^Error:\s*/, ""));
      })
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [isBike, model, bikePreview, t, tyresPick.tyres]);

  useEffect(() => {
    if (!loadout) {
      setRiderParts(null);
      return;
    }
    let alive = true;
    setLoading(true);
    setErr(null);
    loadRiderModel(loadout)
      .then((m) => alive && setRiderParts(m.parts))
      .catch((e) => {
        if (!alive) return;
        setRiderParts(null);
        setErr(String(e).replace(/^Error:\s*/, ""));
      })
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [loadout]);

  /**
   * Hand the loaded mesh up, whichever of the two shapes it arrived in.
   *
   * One effect over both, rather than a call inside each loader: a failure sets its state back
   * to null and this follows it, so the UV map cannot outlive the model it describes.
   *
   * The rider's parts are flattened because a UV layout is per texture name, not per part —
   * a `suit` sheet is bound by whichever pieces ask for it, and asking which of them a triangle
   * came from would be a distinction the sheet itself doesn't make.
   */
  useEffect(() => {
    // `assembled` only ever describes the bike branch: a rider's parts are posed by the rig
    // rather than by a `.geom`, and sides are never asked of them.
    onGeometry?.(
      nodes ?? (riderParts ? riderParts.flatMap((p) => p.nodes) : null),
      nodes ? assembled : false,
    );
    // Through the same effect, so the mesh and the textures said to be its own can never
    // describe two different models. Gated on `nodes` because that is the bike branch.
    onStock?.(nodes ? stock : NO_STOCK);
  }, [nodes, assembled, riderParts, stock, onGeometry, onStock]);

  // Toggled-off gear is dropped before it reaches the viewer, which is what makes hiding it
  // reveal what's underneath rather than just dimming it.
  //
  // Gear-only drops the body along with it, and that alone is the whole view: the viewer reads
  // "no body" as a solo piece and reframes to it — see `RiderGearSolo` and `CameraRig`.
  const shownParts = solo
    ? riderParts?.filter((p) => p.part === gearPart) ?? null
    : hidden.length
      ? riderParts?.filter((p) => !hidden.includes(p.part)) ?? null
      : riderParts;

  // Gear-only with nothing to show: the model didn't load. Say so rather than leave a black
  // canvas — on the rider there'd at least have been a body to make the absence read.
  const soloEmpty = solo && !shownParts?.length;

  // Two different ways there is nothing to stand a paint on, and they deserve different
  // sentences: this *title* has no part bindings (GP Bikes), or this *build* can't decode bike
  // geometry. Either way the editor and the save are untouched — only the picture is missing.
  const unavailable = !game.caps.viewer || (isBike && !bikePreview);
  const why = !game.caps.viewer ? t("designer.noPreviewForGame") : t("designer.noBikePreview");

  /* What you're looking at, and what's in the way of it. A kit is worn under a chest
     protector, and judging a jersey you can only see half of is judging the protector.

     One set of controls for both sizes: the toggles are what the picture *is*, so a
     fullscreen view that couldn't take the helmet off would be the smaller view. */
  const controls = (
    <>
      {isBike && <TyresPicker pick={tyresPick} className="ml-auto" />}
      {!isBike && (
        <div className="ml-auto flex items-center gap-1">
          {/* A helmet on a rider is a small thing across the canvas with half of it turned
              away. This takes it off and fills the frame with it. The hide toggles go while
              it's on — they'd be controls over a rider that isn't on screen. */}
          {!!gearPart && !!model && (
            <Chip
              on={solo}
              onClick={() => setSoloGear((s) => !s)}
              title={t("designer.gearOnlyHint")}
            >
              {t("designer.gearOnly")}
            </Chip>
          )}
          {!solo &&
            HIDEABLE.map(({ part, label }) => (
              <Chip
                key={part}
                on={!hidden.includes(part)}
                onClick={() =>
                  setHidden((h) =>
                    h.includes(part) ? h.filter((p) => p !== part) : [...h, part],
                  )
                }
                title={t(label)}
              >
                {t(label)}
              </Chip>
            ))}
        </div>
      )}
      {loading && <Loader2 className="ml-1 size-3.5 animate-spin text-muted-foreground" />}
      {/* Not offered when there is nothing to draw: filling the window with the sentence
          explaining why there's no preview is a bigger version of nothing. */}
      {!unavailable && (
        <button
          type="button"
          onClick={() => setFull((f) => !f)}
          title={t(full ? "viewer.exitFullscreen" : "viewer.fullscreen")}
          aria-label={t(full ? "viewer.exitFullscreen" : "viewer.fullscreen")}
          className="ml-0.5 rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground"
        >
          {full ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
        </button>
      )}
    </>
  );

  const body = unavailable ? (
    <Message text={why} />
  ) : soloEmpty && !loading ? (
    <Message text={t("designer.noModelFound", { model })} />
  ) : (
    <>
      <ModelViewer
        mode={isBike ? "bike" : "rider"}
        nodes={nodes}
        highlight={highlight}
        textures={textures}
        riderParts={shownParts}
        overrides={overrides}
        frameToken={frameToken}
        loading={loading}
        // No stand-in body behind a solo piece: a rider that only appears when the gear
        // fails to load reads as the gear, badly drawn.
        noStandIn={solo}
        className="absolute inset-0"
      />
      {/* The model on screen is the last one that loaded, so a failure has to keep
          saying so rather than fading — otherwise it reads as "this is your paint". */}
      {err && (
        <div
          title={err}
          className="absolute right-2 top-2 flex max-w-[85%] items-center gap-1.5 rounded-md bg-destructive/90 px-2 py-1 text-[11px] text-destructive-foreground"
        >
          <TriangleAlert className="size-3.5 flex-none" />
          <span className="truncate">{err}</span>
        </div>
      )}
    </>
  );

  // Only true of the rider view.
  const note = !!gearPart && !solo && (
    <p className="flex-none border-t border-border px-3 py-1.5 text-[11px] leading-snug text-faint">
      {t("designer.gearNote")}
    </p>
  );

  return (
    <>
      <div
        ref={panelRef}
        className={cn(
          "flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card",
          className,
        )}
      >
        <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 text-[12.5px] font-medium">
          <Box className="size-3.5 text-muted-foreground" />
          {t("viewer.preview3d")}
          {controls}
        </div>
        {/* Empty while the fullscreen view has it: the canvas is moved rather than copied, so
            there is only ever one model on a GPU and one camera to have turned. */}
        <div className="relative min-h-[240px] flex-1">{!full && body}</div>
        {note}
      </div>

      <Dialog open={full} onOpenChange={setFull}>
        {/* Everything below the title bar rather than the whole screen: the window's own
            minimise and close stay where they are, so filling the screen with a bike can
            never be the thing that leaves someone without a way out. Escape closes it too. */}
        <DialogContent
          showClose={false}
          // Nothing in here is typed into, so the focus ring the dialog would otherwise put
          // on the first control reads as a stray selection over a picture.
          onOpenAutoFocus={(e) => e.preventDefault()}
          className="left-0 top-[42px] flex h-[calc(100vh-42px)] w-screen max-w-none translate-x-0 translate-y-0 flex-col gap-0 overflow-hidden rounded-none border-0 p-0 sm:max-w-none"
        >
          <div className="flex flex-none items-center gap-2 border-b border-border px-3 py-2 text-[12.5px] font-medium">
            <Box className="size-3.5 text-muted-foreground" />
            <DialogTitle className="text-[12.5px] font-medium">
              {t("viewer.preview3d")}
            </DialogTitle>
            {controls}
          </div>
          <div className="relative min-h-0 flex-1">{full && body}</div>
          {note}
        </DialogContent>
      </Dialog>
    </>
  );
}

/** A header toggle: lit when what it names is on. */
function Chip({
  on,
  onClick,
  title,
  children,
}: {
  on: boolean;
  onClick: () => void;
  title: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={cn(
        "rounded border px-1.5 py-0.5 text-[10.5px] font-medium transition-colors",
        on ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-faint",
      )}
    >
      {children}
    </button>
  );
}

function Message({ text }: { text: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center px-6 text-center">
      <p className="max-w-xs text-[12px] leading-relaxed text-muted-foreground">{text}</p>
    </div>
  );
}
