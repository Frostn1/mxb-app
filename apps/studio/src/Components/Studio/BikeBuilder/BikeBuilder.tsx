import { useCallback, useState } from "react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import BlenderBar from "./BlenderBar";
import PartTray from "./PartTray";
import PartSlots from "./PartSlots";
import PreviewPane, { useAssemblyView } from "./PreviewPane";
import BuildPanel from "./BuildPanel";
import PartMaker from "./PartMaker";
import { useBikeLibrary } from "./useBikeLibrary";

/**
 * Bike builder: put a bike together from parts, without having to be good at Blender.
 *
 * Studio chooses the parts and where they go; the rider's own Blender, run in the
 * background, does the importing, placing and exporting (see `src-tauri/src/blender.rs`).
 * Two modes. Assemble: the 3D view is the main panel — a base bike (a full-bike import or an
 * installed template) shows there right away, and placing a part means dragging one from the
 * tray onto it or clicking an open mount, not hunting through a slot grid. The tray sits to
 * its left; the role outliner is a collapsible strip on the right, for checking what's still
 * missing rather than for placing itself. Part Maker: parts made from templates and briefs,
 * which join the same library, as its own mode below the same bar.
 */
export default function BikeBuilder() {
  const t = useT();
  const [mode, setMode] = useState<"assemble" | "maker">("assemble");
  const [ready, setReady] = useState(false);
  /** Bumped whenever one panel changes the library, so the others read it again. */
  const [version, setVersion] = useState(0);
  const changed = useCallback(() => setVersion((v) => v + 1), []);
  const lib = useBikeLibrary(version, changed);
  const [view, setView] = useAssemblyView(version);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <BlenderBar onReadyChange={setReady} />

      <div className="flex items-center gap-1 border-b border-border px-4 py-2">
        {(["assemble", "maker"] as const).map((m) => (
          <Button key={m} size="sm" variant={mode === m ? "default" : "outline"} onClick={() => setMode(m)}>
            {t(m === "assemble" ? "bike.modeAssemble" : "bike.modeMaker")}
          </Button>
        ))}
      </div>

      {mode === "assemble" ? (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="grid min-h-0 flex-1 grid-cols-[18rem_1fr_auto] divide-x divide-border overflow-hidden">
            <PartTray ready={ready} lib={lib} />
            <PreviewPane version={version} onChanged={changed} view={view} setView={setView} lib={lib} />
            <PartSlots lib={lib} />
          </div>
          <BuildPanel view={view} placedCount={view?.assembly.placed.length ?? 0} />
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto p-6">
          <PartMaker ready={ready} onChanged={changed} />
        </div>
      )}
    </div>
  );
}
