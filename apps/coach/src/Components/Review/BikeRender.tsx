import { useEffect, useState } from "react";
import { ModelViewer } from "@frost/shared/Components/Viewer/ModelViewer";
import { previewModelSwap } from "@frost/shared/api/mods";
import type { BikeModel } from "@frost/shared/types";
import { useT } from "@/i18n";

/**
 * The rider's own bike, beside the feel chips.
 *
 * The shared renderer only — not `ViewerPanel`, which wraps it in a card, a "3D Preview"
 * header, an expand button and a rider/bike/both switch. None of that belongs on a step whose
 * whole subject is the machine, and the header sat on top of the travel figure.
 *
 * The model comes through Coach's `preview_model_swap`, which reads the bike's own archive.
 */
export default function BikeRender({
  bikeId,
  travel,
  maxTravel,
}: {
  /** The bike the session was ridden on, as the recorder wrote it. */
  bikeId: string;
  /** Share of each end's travel in use, 0 extended to 1 bottomed. Front then rear. */
  travel?: [number, number] | null;
  /** Each end's full stroke in metres. Without it a share has no distance to become. */
  maxTravel?: [number, number] | null;
}) {
  const t = useT();
  const [model, setModel] = useState<BikeModel | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setModel(null);
    setFailed(false);
    if (!bikeId) return;
    let live = true;
    previewModelSwap(bikeId, "", undefined)
      .then((m) => live && setModel(m))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [bikeId]);

  // A bike that came apart has nothing honest to show: its parts are each in their own frame,
  // so drawing them puts a pile on screen and calls it the rider's bike.
  if (failed || (model && !model.assembled)) {
    return <p className="text-[12px] text-faint">{t("bike.noModel")}</p>;
  }

  return (
    <div className="relative h-[260px] overflow-hidden rounded-[var(--radius)] border border-border">
      <ModelViewer
        mode="bike"
        nodes={model?.nodes ?? null}
        rig={model?.rig ?? null}
        textures={model?.base}
        loading={!model}
        hideHints
        className="absolute inset-0"
      />
      {travel && maxTravel && (
        <div className="pointer-events-none absolute right-3 top-3 text-right">
          <div className="font-mono text-[18px] font-bold tabular-nums text-primary">
            {Math.round(travel[0] * maxTravel[0] * 1000)}
            <span className="text-[11px] text-muted-foreground"> / {Math.round(maxTravel[0] * 1000)} mm</span>
          </div>
          <div className="text-[11px] text-muted-foreground">{t("bike.forkUsed")}</div>
        </div>
      )}
    </div>
  );
}
