import { useEffect, useState } from "react";
import { loadBikeModel, previewModelSwap } from "../../api/mods";
import type { BikeModel } from "../../types";

/**
 * Load a bike's geometry and its paints.
 *
 * The one loader for both places a bike is drawn — the viewer dialog and the mod page's
 * stage — because the awkward parts are not obvious and were never going to stay in step if
 * they were written twice: a swap preview is the same bike assembled from another set, and a
 * *reload* (a file behind it changed) must keep the bike on screen when it fails, since a
 * save caught mid-write fails where the one after it won't.
 */
export interface BikeModelPick {
  /** Off while the dialog is closed, or the stage isn't showing a bike. */
  enabled: boolean;
  /** Bike folder or `.pkz` to load from. */
  modelSource?: string;
  /** Draw it as one of its model swaps instead. Resolved backend-side, hence bike + variant
   *  rather than a path. */
  modelSwap?: { bike: string; variant: string };
  /** The tyre pack to fit, from `useTyresPick`. */
  tyres?: string;
  /** Bumped by the caller when the files behind the bike changed — see `watchViewerSource`. */
  generation?: number;
}

export interface BikeModelState {
  model: BikeModel | null;
  loading: boolean;
  /** Why it wouldn't load. A swap preview can be refused outright, and that reason is worth
   *  showing; a failed *reload* never lands here, so the bike on screen stays. */
  error: string | null;
  /** Ticks each time a reload replaced the model, for a caller that says so on screen. */
  reloads: number;
}

export function useBikeModel({
  enabled,
  modelSource,
  modelSwap,
  tyres,
  generation = 0,
}: BikeModelPick): BikeModelState {
  const [model, setModel] = useState<BikeModel | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);

  const swapBike = modelSwap?.bike;
  const swapVariant = modelSwap?.variant;

  useEffect(() => {
    if (!enabled) {
      setModel(null);
      return;
    }
    const load =
      swapBike && swapVariant
        ? previewModelSwap(swapBike, swapVariant, tyres)
        : modelSource
          ? loadBikeModel(modelSource, tyres)
          : null;
    if (!load) {
      setModel(null);
      return;
    }
    let alive = true;
    const reload = generation > 0;
    setLoading(true);
    setError(null);
    load
      .then((m) => {
        if (!alive) return;
        setModel(m);
        if (reload) setReloads((n) => n + 1);
      })
      .catch((e) => {
        if (!alive) return;
        // A reload caught mid-write fails where the one after the write's last event won't,
        // so keep the bike on screen rather than blank it in between.
        if (reload) {
          console.warn("[viewer] bike changed but wouldn't load:", e);
          return;
        }
        setError(String(e).replace(/^Error:\s*/, ""));
        setModel(null);
      })
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [enabled, modelSource, swapBike, swapVariant, tyres, generation]);

  return { model, loading, error, reloads };
}
