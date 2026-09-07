import { useCallback, useEffect, useState } from "react";
import { scanLibrary, setPreviewTyres } from "../../api/mods";
import { useConfig } from "../../Context/Config";

/** The value that means "whatever the bike's own `gfx.cfg` names". */
export const BIKE_OWN_TYRES = "";

export interface TyresPick {
  /** The pack to render on, or {@link BIKE_OWN_TYRES}. */
  tyres: string;
  /** Installed packs, without extension. Empty while scanning, and on a machine with none. */
  options: string[];
  choose: (name: string) => void;
}

/**
 * Which tyre pack the 3D previews fit, shared by every place that draws a bike.
 *
 * A bike's `gfx.cfg` names exactly one pack, so seeing it on another means substituting that
 * name — which the backend does in memory, leaving the mod folder alone. The choice lives in
 * the app config rather than in each preview's own state, because it's a way you like looking
 * at bikes, not a decision per dialog: pick in the Viewer and the Designer agrees.
 *
 * Options come from the same `mods/tyres` scan the Presets tyre slot uses. Deliberately not
 * the built-in names that slot also offers (`p_mx`): the game accepts those, but there are no
 * files on disk to draw, so offering one here would be a pick that silently changes nothing.
 */
export function useTyresPick(): TyresPick {
  const { config, reloadConfig } = useConfig();
  const saved = config.previewTyres ?? BIKE_OWN_TYRES;
  const [tyres, setTyres] = useState(saved);
  const [options, setOptions] = useState<string[]>([]);

  // Follow the config when it changes under us — another preview's pick, or a reload.
  useEffect(() => setTyres(saved), [saved]);

  useEffect(() => {
    let alive = true;
    scanLibrary("mods/tyres")
      .then((entries) => {
        if (!alive) return;
        const names = [
          ...new Set(entries.map((e) => e.name.replace(/\.(pkz|zip)$/i, ""))),
        ].sort((a, b) => a.localeCompare(b));
        setOptions(names);
      })
      .catch(() => alive && setOptions([]));
    return () => {
      alive = false;
    };
  }, []);

  const choose = useCallback(
    (name: string) => {
      // Shown before it's saved: the reload behind this is a whole-app re-read, and waiting
      // on it would leave the dropdown showing the old pack while the bike reloads.
      setTyres(name);
      void setPreviewTyres(name)
        .then(reloadConfig)
        .catch((e) => console.warn("[viewer] couldn't remember the tyres pick:", e));
    },
    [reloadConfig],
  );

  return { tyres, options, choose };
}
