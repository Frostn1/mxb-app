import { useEffect, useMemo, useState } from "react";
import { Box, Loader2, TriangleAlert } from "lucide-react";
import type * as THREE from "three";
import { cn } from "@/lib/utils";
import { ModelViewer } from "../../Viewer/ModelViewer";
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
const HIDEABLE: { part: RiderPart["part"]; label: TKey }[] = [
  { part: "protection", label: "paints.kind.protection" },
  { part: "helmet", label: "paints.kind.helmet" },
];

export function PreviewPanel({
  state,
  overrides,
  frameToken,
  className,
}: {
  state: PaintDestState;
  overrides: Map<string, THREE.Texture>;
  /** Bumped whenever the canvas changed, so a frame gets drawn without any prop moving. */
  frameToken?: number;
  className?: string;
}) {
  const t = useT();
  const { game } = useConfig();
  const { kind, model, bikePreview } = state;
  const [nodes, setNodes] = useState<EdfNode[] | null>(null);
  const [textures, setTextures] = useState<PaintTexture[]>([]);
  const [riderParts, setRiderParts] = useState<RiderPart[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [hidden, setHidden] = useState<RiderPart["part"][]>([]);

  const isBike = isBikeKind(kind);
  const gearPart = gearPartOf(kind);

  /**
   * Start with the pieces that cover what you're painting taken off.
   *
   * Painting a jersey with a chest protector over it means judging the third of it you can
   * see — and the stock protector renders untextured grey, which reads as the paint having
   * failed rather than as gear being in the way. The toggles above put them back.
   */
  useEffect(() => {
    setHidden(HIDEABLE.filter((h) => h.part !== gearPart).map((h) => h.part));
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
      setTextures([]);
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
        return loadBikeModel(found.path);
      })
      .then((m) => {
        if (!alive) return;
        setNodes(m.nodes);
        // The model's own look, under the drawing — so parts this paint doesn't cover still
        // read as the bike rather than as untextured grey.
        setTextures(m.paints[0]?.textures ?? []);
      })
      .catch((e) => {
        if (!alive) return;
        setNodes(null);
        setTextures([]);
        setErr(String(e).replace(/^Error:\s*/, ""));
      })
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [isBike, model, bikePreview, t]);

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

  // Toggled-off gear is dropped before it reaches the viewer, which is what makes hiding it
  // reveal what's underneath rather than just dimming it.
  const shownParts = hidden.length
    ? riderParts?.filter((p) => !hidden.includes(p.part)) ?? null
    : riderParts;

  // Two different ways there is nothing to stand a paint on, and they deserve different
  // sentences: this *title* has no part bindings (GP Bikes), or this *build* can't decode bike
  // geometry. Either way the editor and the save are untouched — only the picture is missing.
  const unavailable = !game.caps.viewer || (isBike && !bikePreview);
  const why = !game.caps.viewer ? t("designer.noPreviewForGame") : t("designer.noBikePreview");

  return (
    <div
      className={cn(
        "flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-card",
        className,
      )}
    >
      <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 text-[12.5px] font-medium">
        <Box className="size-3.5 text-muted-foreground" />
        {t("viewer.preview3d")}
        {/* Which pieces of the rider are in the way. A kit is worn under a chest protector,
            and judging a jersey you can only see half of is judging the protector. */}
        {!isBike && (
          <div className="ml-auto flex items-center gap-1">
            {HIDEABLE.map(({ part, label }) => (
              <button
                key={part}
                type="button"
                onClick={() =>
                  setHidden((h) =>
                    h.includes(part) ? h.filter((p) => p !== part) : [...h, part],
                  )
                }
                title={t(label)}
                className={cn(
                  "rounded border px-1.5 py-0.5 text-[10.5px] font-medium transition-colors",
                  hidden.includes(part)
                    ? "border-border text-faint"
                    : "border-primary/60 bg-primary/10 text-foreground",
                )}
              >
                {t(label)}
              </button>
            ))}
          </div>
        )}
        {loading && <Loader2 className="ml-1 size-3.5 animate-spin text-muted-foreground" />}
      </div>
      <div className="relative min-h-[240px] flex-1">
        {unavailable ? (
          <Message text={why} />
        ) : (
          <>
            <ModelViewer
              mode={isBike ? "bike" : "rider"}
              nodes={nodes}
              textures={textures}
              riderParts={shownParts}
              overrides={overrides}
              frameToken={frameToken}
              loading={loading}
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
        )}
      </div>
      {!!gearPart && (
        <p className="border-t border-border px-3 py-1.5 text-[11px] leading-snug text-faint">
          {t("designer.gearNote")}
        </p>
      )}
    </div>
  );
}

function Message({ text }: { text: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center px-6 text-center">
      <p className="max-w-xs text-[12px] leading-relaxed text-muted-foreground">{text}</p>
    </div>
  );
}
